//! Embedded redb-backed cache store for album art and lyrics.
//!
//! Replaces the previous flat-file caches (one `.jpg` per art entry, one
//! `.json` per lyric entry) with a single `winrmpc.redb` file. Crucially it
//! tracks each art entry's byte size and last-access time so the configured
//! `art_cache_size_mb` budget is actually enforced via LRU eviction — the old
//! `ArtCache` had no size accounting and grew without bound.
//!
//! redb's API is synchronous; every public method here is meant to be called
//! from inside `tokio::task::spawn_blocking` at the async call sites. Never hold
//! a redb transaction across an `.await`.

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// key = "artist\x1falbum" or "artist:<name>"; value = processed JPEG bytes
const ART: TableDefinition<&str, &[u8]> = TableDefinition::new("art");
// key = same as ART; value = serde_json(ArtMeta)
const ART_META: TableDefinition<&str, &[u8]> = TableDefinition::new("art_meta");
// key = "artist\x1ftitle\x1falbum"; value = serde_json(Option<Lyrics>)
const LYRICS: TableDefinition<&str, &[u8]> = TableDefinition::new("lyrics");
// key = server name; value = serde_json(Vec<RecentlyPlayedEntry>), newest-first
const RECENTLY_PLAYED: TableDefinition<&str, &[u8]> = TableDefinition::new("recently_played");
// key = "artist:<name>" or "artist\x1falbum"; value = serde_json(Option<String>)
const BIOS: TableDefinition<&str, &[u8]> = TableDefinition::new("bios");
// key = "artist:<name>" (artist MBID) or "artist\x1falbum" (release-group MBID);
// value = serde_json(Option<String>) — None = "searched, confirmed no match"
const MB_IDS: TableDefinition<&str, &[u8]> = TableDefinition::new("mb_ids");
// Schema/migration markers; key = marker name, value = ignored
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

#[derive(Serialize, Deserialize)]
struct ArtMeta {
    /// Stored blob size in bytes. 0 for negative ("known missing") entries.
    size: u64,
    /// Unix seconds of the last get/store — drives LRU eviction.
    last_access: u64,
    /// True when we looked and found no art; no blob is stored in `ART`.
    is_empty: bool,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Clone-cheap handle to the cache database (`Arc` inside).
#[derive(Clone)]
pub struct Store {
    db: Arc<Database>,
    /// `false` when [`Store::open`] fell all the way back to `InMemoryBackend`.
    /// See [`Store::is_persistent`].
    persistent: bool,
}

impl Store {
    /// Whether this store is backed by a file on disk.
    ///
    /// `false` means the app is running with caching effectively off for the
    /// session. The Settings → Storage section reports it, because a
    /// `tracing::error!` alone is only visible to someone who thinks to open
    /// the Log view — and the symptom (everything re-downloads, forever) does
    /// not obviously point at storage.
    pub fn is_persistent(&self) -> bool {
        self.persistent
    }

    /// Open (or create) the cache database under `cache_dir`. Falls back to an
    /// in-memory backend if the file can't be opened, so the app still runs
    /// (caches just won't persist that session).
    pub fn open(cache_dir: &Path) -> Self {
        if let Err(e) = std::fs::create_dir_all(cache_dir) {
            tracing::error!(
                dir = %cache_dir.display(),
                error = %e,
                "could not create the cache directory"
            );
        }
        let path = cache_dir.join("winrmpc.redb");
        let mut persistent = true;
        let db = match Database::create(&path) {
            Ok(db) => db,
            Err(first_err) => {
                // Most likely an incompatible on-disk format from an older redb
                // major (the cache predates this build). The cache is disposable,
                // so wipe the file and recreate it; fall back to an in-memory DB
                // only if even a fresh file can't be opened.
                tracing::warn!("Cache DB at {path:?} couldn't be opened ({first_err}); rebuilding it");
                std::fs::remove_file(&path).ok();
                Database::create(&path).unwrap_or_else(|second_err| {
                    // ERROR, not WARN: this is the app running with a core
                    // feature off. Nothing — art, lyrics, bios, MBIDs, play
                    // history — survives this session, and every cover
                    // re-downloads on the next launch. It was previously a
                    // WARN that nothing surfaced, so the only symptom was the
                    // app being mysteriously slow forever.
                    persistent = false;
                    tracing::error!(
                        path = %path.display(),
                        error = %second_err,
                        "cache database unavailable; running in memory only — \
                         album art, lyrics, bios and play history will NOT \
                         persist beyond this session"
                    );
                    Database::builder()
                        .create_with_backend(redb::backends::InMemoryBackend::new())
                        .expect("in-memory redb backend")
                })
            }
        };
        let store = Self {
            db: Arc::new(db),
            persistent,
        };
        store.ensure_tables();
        store.purge_poisoned_negatives();
        store.cleanup_legacy(cache_dir);
        store
    }

    /// Create the tables up front so later read transactions don't fail on a
    /// missing table before the first write.
    fn ensure_tables(&self) {
        if let Ok(wtx) = self.db.begin_write() {
            let _ = wtx.open_table(ART);
            let _ = wtx.open_table(ART_META);
            let _ = wtx.open_table(LYRICS);
            let _ = wtx.open_table(RECENTLY_PLAYED);
            let _ = wtx.open_table(BIOS);
            let _ = wtx.open_table(MB_IDS);
            let _ = wtx.open_table(META);
            let _ = wtx.commit();
        }
    }

    /// One-time cleanup of "we looked and found nothing" records, re-run
    /// whenever the lookup rules change enough that those records are no
    /// longer trustworthy. Bump the marker to trigger it again.
    ///
    /// - `neg_purge_v1` (2026-06-10): the empty-URI recently-played fetch was
    ///   persisting negatives even though it could only try MusicBrainz,
    ///   permanently blocking the MPD embedded-art path for those albums.
    /// - `neg_purge_v2` (2026-08-12): the MusicBrainz matching rules changed
    ///   materially — Lucene escaping (`AC/DC` had been sending a malformed
    ///   query), Unicode folding, sort-order artist names, region-tag
    ///   stripping, and result validation. Every negative recorded before
    ///   that is an answer to a question we no longer ask, and because the
    ///   remote stage short-circuits on a stored negative, those albums would
    ///   never be retried.
    ///
    /// Purges **both** the negative art entries and the "confirmed no
    /// match" MBIDs — leaving the latter would have `search_release_group`
    /// return the cached `None` without ever issuing the corrected query.
    /// Genuinely missing art is simply re-recorded on the next lookup.
    fn purge_poisoned_negatives(&self) {
        const MARKER: &str = "neg_purge_v2";
        let already_done = (|| {
            let rtx = self.db.begin_read().ok()?;
            let table = rtx.open_table(META).ok()?;
            Some(table.get(MARKER).ok()?.is_some())
        })()
        .unwrap_or(false);
        if already_done {
            return;
        }

        let mut empties: Vec<String> = Vec::new();
        if let Ok(rtx) = self.db.begin_read() {
            if let Ok(table) = rtx.open_table(ART_META) {
                if let Ok(iter) = table.iter() {
                    for (k, v) in iter.flatten() {
                        if let Ok(m) = serde_json::from_slice::<ArtMeta>(v.value()) {
                            if m.is_empty {
                                empties.push(k.value().to_string());
                            }
                        }
                    }
                }
            }
        }

        // "Confirmed no match" MBIDs, recorded under the old query rules.
        let mut stale_ids: Vec<String> = Vec::new();
        if let Ok(rtx) = self.db.begin_read() {
            if let Ok(table) = rtx.open_table(MB_IDS) {
                if let Ok(iter) = table.iter() {
                    for (k, v) in iter.flatten() {
                        if let Ok(None) = serde_json::from_slice::<Option<String>>(v.value()) {
                            stale_ids.push(k.value().to_string());
                        }
                    }
                }
            }
        }

        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut table) = wtx.open_table(ART_META) {
                    for k in &empties {
                        let _ = table.remove(k.as_str());
                    }
                }
                if let Ok(mut table) = wtx.open_table(MB_IDS) {
                    for k in &stale_ids {
                        let _ = table.remove(k.as_str());
                    }
                }
                if let Ok(mut table) = wtx.open_table(META) {
                    let _ = table.insert(MARKER, [1u8].as_slice());
                }
            }
            let _ = wtx.commit();
            if !empties.is_empty() || !stale_ids.is_empty() {
                tracing::info!(
                    "Purged {} negative art-cache entries and {} stale MBID misses",
                    empties.len(),
                    stale_ids.len()
                );
            }
        }
    }

    /// Best-effort removal of the pre-DB flat caches: top-level `*.jpg` art
    /// files and the `lyrics/` subdir. Both rebuild lazily, so deleting them on
    /// first run of the DB-backed build is safe and reclaims disk.
    fn cleanup_legacy(&self, cache_dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(cache_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("jpg") {
                    std::fs::remove_file(&p).ok();
                }
            }
        }
        let lyrics_dir = cache_dir.join("lyrics");
        if lyrics_dir.is_dir() {
            std::fs::remove_dir_all(&lyrics_dir).ok();
        }
    }

    // ===================================================================
    // Album art
    // ===================================================================

    /// Fetch cached art bytes, bumping the entry's last-access time on a hit.
    pub fn art_get(&self, key: &str) -> Option<Vec<u8>> {
        let data = {
            let rtx = self.db.begin_read().ok()?;
            let table = rtx.open_table(ART).ok()?;
            let guard = table.get(key).ok()??;
            guard.value().to_vec()
        };
        self.touch(key);
        Some(data)
    }

    /// Update an existing art entry's `last_access` (no-op if absent).
    fn touch(&self, key: &str) {
        let now = now_secs();
        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut meta) = wtx.open_table(ART_META) {
                    let existing = meta.get(key).ok().flatten().map(|g| g.value().to_vec());
                    if let Some(bytes) = existing {
                        if let Ok(mut m) = serde_json::from_slice::<ArtMeta>(&bytes) {
                            m.last_access = now;
                            if let Ok(b) = serde_json::to_vec(&m) {
                                let _ = meta.insert(key, b.as_slice());
                            }
                        }
                    }
                }
            }
            let _ = wtx.commit();
        }
    }

    /// Store processed art bytes, then evict oldest entries if over `limit_bytes`.
    pub fn art_put(&self, key: &str, bytes: &[u8], limit_bytes: u64) {
        let meta = ArtMeta {
            size: bytes.len() as u64,
            last_access: now_secs(),
            is_empty: false,
        };
        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut t) = wtx.open_table(ART) {
                    let _ = t.insert(key, bytes);
                }
                if let Ok(mut m) = wtx.open_table(ART_META) {
                    if let Ok(b) = serde_json::to_vec(&meta) {
                        let _ = m.insert(key, b.as_slice());
                    }
                }
            }
            let _ = wtx.commit();
        }
        self.art_evict(limit_bytes);
    }

    /// Record that `key` has no art (negative cache), so we don't keep refetching
    /// missing art on every launch. Stores metadata only — no blob.
    pub fn art_put_empty(&self, key: &str) {
        let meta = ArtMeta {
            size: 0,
            last_access: now_secs(),
            is_empty: true,
        };
        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut m) = wtx.open_table(ART_META) {
                    if let Ok(b) = serde_json::to_vec(&meta) {
                        let _ = m.insert(key, b.as_slice());
                    }
                }
            }
            let _ = wtx.commit();
        }
    }

    /// Whether we've ever resolved this key (positive or negative).
    pub fn art_known(&self, key: &str) -> bool {
        (|| {
            let rtx = self.db.begin_read().ok()?;
            let table = rtx.open_table(ART_META).ok()?;
            Some(table.get(key).ok()?.is_some())
        })()
        .unwrap_or(false)
    }

    /// Drop all cached art (blobs + metadata).
    pub fn art_clear(&self) {
        if let Ok(wtx) = self.db.begin_write() {
            let _ = wtx.delete_table(ART);
            let _ = wtx.delete_table(ART_META);
            let _ = wtx.commit();
        }
        self.ensure_tables();
    }

    /// Drop the lookup caches that sit alongside art: lyrics, Wikipedia bios
    /// and resolved MusicBrainz IDs.
    ///
    /// Deliberately leaves `recently_played` (app-generated history, not a
    /// cache — nothing could re-derive it) and `meta` (migration markers;
    /// clearing those would re-run one-time purges pointlessly). Pair with
    /// `ArtCache::clear` for a full "forget everything re-fetchable".
    pub fn clear_lookup_caches(&self) {
        if let Ok(wtx) = self.db.begin_write() {
            let _ = wtx.delete_table(LYRICS);
            let _ = wtx.delete_table(BIOS);
            let _ = wtx.delete_table(MB_IDS);
            let _ = wtx.commit();
        }
        self.ensure_tables();
    }

    /// Bytes currently held by cached art blobs, for display next to the
    /// configured limit. Negative entries carry no bytes.
    pub fn art_cache_bytes(&self) -> u64 {
        (|| {
            let rtx = self.db.begin_read().ok()?;
            let table = rtx.open_table(ART_META).ok()?;
            let mut total = 0u64;
            for (_, v) in table.iter().ok()?.flatten() {
                if let Ok(m) = serde_json::from_slice::<ArtMeta>(v.value()) {
                    total = total.saturating_add(m.size);
                }
            }
            Some(total)
        })()
        .unwrap_or(0)
    }

    /// Evict least-recently-accessed art until the total stored size is within
    /// `limit_bytes`. Negative (empty) entries carry no bytes and are kept.
    fn art_evict(&self, limit_bytes: u64) {
        // Collect (key, size, last_access) for non-empty entries and the running
        // total, in one short-lived read transaction.
        let mut ordered: Vec<(String, u64, u64)> = Vec::new();
        let mut total = 0u64;
        if let Ok(rtx) = self.db.begin_read() {
            if let Ok(table) = rtx.open_table(ART_META) {
                if let Ok(iter) = table.iter() {
                    for (k, v) in iter.flatten() {
                        if let Ok(m) = serde_json::from_slice::<ArtMeta>(v.value()) {
                            if !m.is_empty {
                                total += m.size;
                                ordered.push((k.value().to_string(), m.size, m.last_access));
                            }
                        }
                    }
                }
            }
        }

        if total <= limit_bytes {
            return;
        }

        // Oldest first; remove until under budget.
        ordered.sort_by_key(|(_, _, last)| *last);
        let mut to_remove: Vec<String> = Vec::new();
        for (k, size, _) in ordered {
            if total <= limit_bytes {
                break;
            }
            total = total.saturating_sub(size);
            to_remove.push(k);
        }
        if to_remove.is_empty() {
            return;
        }

        if let Ok(wtx) = self.db.begin_write() {
            {
                let mut art = wtx.open_table(ART).ok();
                let mut meta = wtx.open_table(ART_META).ok();
                for k in &to_remove {
                    if let Some(t) = art.as_mut() {
                        let _ = t.remove(k.as_str());
                    }
                    if let Some(t) = meta.as_mut() {
                        let _ = t.remove(k.as_str());
                    }
                }
            }
            let _ = wtx.commit();
        }
    }

    // ===================================================================
    // Lyrics
    // ===================================================================

    /// Look up cached lyrics. Returns:
    /// - `None` — not cached (fetch from network),
    /// - `Some(None)` — cached "no lyrics exist",
    /// - `Some(Some(l))` — cached lyrics.
    pub fn lyrics_get(&self, key: &str) -> Option<Option<crate::lyrics::Lyrics>> {
        let rtx = self.db.begin_read().ok()?;
        let table = rtx.open_table(LYRICS).ok()?;
        let guard = table.get(key).ok()??;
        serde_json::from_slice::<Option<crate::lyrics::Lyrics>>(guard.value()).ok()
    }

    /// Persist a lyrics fetch result (including the negative `None` case).
    pub fn lyrics_put(&self, key: &str, value: &Option<crate::lyrics::Lyrics>) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            if let Ok(wtx) = self.db.begin_write() {
                {
                    if let Ok(mut t) = wtx.open_table(LYRICS) {
                        let _ = t.insert(key, bytes.as_slice());
                    }
                }
                let _ = wtx.commit();
            }
        }
    }

    // ===================================================================
    // Recently Played history (per server; app-generated data, not a cache
    // of re-fetchable remote data — kept in redb for the same write-pattern
    // reasons as art/lyrics, not because it's a "cache". See
    // docs/plans/recently-added-and-played-history.md.)
    // ===================================================================

    /// Full history for `server`, newest-first. Empty (not an error) if the
    /// server has none yet.
    pub fn recently_played_get(&self, server: &str) -> Vec<crate::mpd::types::RecentlyPlayedEntry> {
        (|| {
            let rtx = self.db.begin_read().ok()?;
            let table = rtx.open_table(RECENTLY_PLAYED).ok()?;
            let guard = table.get(server).ok()??;
            serde_json::from_slice(guard.value()).ok()
        })()
        .unwrap_or_default()
    }

    /// Replace `server`'s stored history with `entries` (caller is
    /// responsible for pruning/order — this is a plain overwrite, not an
    /// append, so a stale on-disk tail can't reappear).
    pub fn recently_played_put(&self, server: &str, entries: &[crate::mpd::types::RecentlyPlayedEntry]) {
        if let Ok(bytes) = serde_json::to_vec(entries) {
            if let Ok(wtx) = self.db.begin_write() {
                {
                    if let Ok(mut t) = wtx.open_table(RECENTLY_PLAYED) {
                        let _ = t.insert(server, bytes.as_slice());
                    }
                }
                let _ = wtx.commit();
            }
        }
    }

    /// Remove a server's history entirely (called when the server profile
    /// itself is deleted).
    pub fn recently_played_delete(&self, server: &str) {
        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut t) = wtx.open_table(RECENTLY_PLAYED) {
                    let _ = t.remove(server);
                }
            }
            let _ = wtx.commit();
        }
    }

    // ===================================================================
    // Wikipedia bios (genuine cache of re-fetchable remote data — same
    // three-state convention as `lyrics_get`/`lyrics_put`: absent key =
    // never fetched, `Some(None)` = fetched, confirmed no bio exists,
    // `Some(Some(text))` = have a bio.)
    // ===================================================================

    pub fn bio_get(&self, key: &str) -> Option<Option<String>> {
        let rtx = self.db.begin_read().ok()?;
        let table = rtx.open_table(BIOS).ok()?;
        let guard = table.get(key).ok()??;
        serde_json::from_slice::<Option<String>>(guard.value()).ok()
    }

    pub fn bio_put(&self, key: &str, value: &Option<String>) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            if let Ok(wtx) = self.db.begin_write() {
                {
                    if let Ok(mut t) = wtx.open_table(BIOS) {
                        let _ = t.insert(key, bytes.as_slice());
                    }
                }
                let _ = wtx.commit();
            }
        }
    }

    // ===================================================================
    // MusicBrainz ID resolution cache — same three-state convention as
    // `bio_get`/`bio_put` above.
    // ===================================================================

    pub fn mb_id_get(&self, key: &str) -> Option<Option<String>> {
        let rtx = self.db.begin_read().ok()?;
        let table = rtx.open_table(MB_IDS).ok()?;
        let guard = table.get(key).ok()??;
        serde_json::from_slice::<Option<String>>(guard.value()).ok()
    }

    pub fn mb_id_put(&self, key: &str, value: &Option<String>) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            if let Ok(wtx) = self.db.begin_write() {
                {
                    if let Ok(mut t) = wtx.open_table(MB_IDS) {
                        let _ = t.insert(key, bytes.as_slice());
                    }
                }
                let _ = wtx.commit();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpd::types::RecentlyPlayedEntry;

    fn temp_store() -> Store {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "winrmpc-test-{}-{}-{}",
            std::process::id(),
            now_secs(),
            n
        ));
        Store::open(&dir)
    }

    fn entry(file: &str, played_at: i64) -> RecentlyPlayedEntry {
        RecentlyPlayedEntry {
            file: file.to_string(),
            title: "T".into(),
            artist: "A".into(),
            album_artist: String::new(),
            album: "Al".into(),
            played_at,
        }
    }

    #[test]
    fn recently_played_empty_server_returns_empty_vec() {
        let store = temp_store();
        assert!(store.recently_played_get("nonexistent").is_empty());
    }

    #[test]
    fn recently_played_round_trips() {
        let store = temp_store();
        let entries = vec![entry("a.mp3", 200), entry("b.mp3", 100)];
        store.recently_played_put("server1", &entries);
        assert_eq!(store.recently_played_get("server1"), entries);
    }

    #[test]
    fn recently_played_keys_do_not_leak_across_servers() {
        let store = temp_store();
        store.recently_played_put("server1", &[entry("a.mp3", 100)]);
        store.recently_played_put("server2", &[entry("b.mp3", 200)]);
        assert_eq!(store.recently_played_get("server1"), vec![entry("a.mp3", 100)]);
        assert_eq!(store.recently_played_get("server2"), vec![entry("b.mp3", 200)]);
    }

    #[test]
    fn recently_played_put_overwrites_not_appends() {
        let store = temp_store();
        store.recently_played_put("server1", &[entry("a.mp3", 100), entry("b.mp3", 90)]);
        store.recently_played_put("server1", &[entry("c.mp3", 300)]);
        assert_eq!(store.recently_played_get("server1"), vec![entry("c.mp3", 300)]);
    }

    #[test]
    fn recently_played_delete_removes_key() {
        let store = temp_store();
        store.recently_played_put("server1", &[entry("a.mp3", 100)]);
        store.recently_played_delete("server1");
        assert!(store.recently_played_get("server1").is_empty());
    }

    // --- bios ---------------------------------------------------------

    #[test]
    fn bio_get_absent_key_returns_none() {
        let store = temp_store();
        assert_eq!(store.bio_get("artist:Nobody"), None);
    }

    #[test]
    fn bio_round_trips_positive_result() {
        let store = temp_store();
        store.bio_put("artist:Tool", &Some("A band.".to_string()));
        assert_eq!(store.bio_get("artist:Tool"), Some(Some("A band.".to_string())));
    }

    #[test]
    fn bio_round_trips_negative_result_distinct_from_absent() {
        let store = temp_store();
        store.bio_put("artist:Obscure", &None);
        // Some(None) = "looked, found nothing" — distinct from an absent
        // key, which is `None` (never looked up).
        assert_eq!(store.bio_get("artist:Obscure"), Some(None));
        assert_eq!(store.bio_get("artist:NeverLookedUp"), None);
    }

    // --- mb_ids ---------------------------------------------------------

    #[test]
    fn mb_id_get_absent_key_returns_none() {
        let store = temp_store();
        assert_eq!(store.mb_id_get("artist:Nobody"), None);
    }

    #[test]
    fn mb_id_round_trips_both_key_forms() {
        let store = temp_store();
        store.mb_id_put("artist:Tool", &Some("mbid-artist-123".to_string()));
        store.mb_id_put("Tool\x1fLateralus", &Some("mbid-rg-456".to_string()));
        assert_eq!(store.mb_id_get("artist:Tool"), Some(Some("mbid-artist-123".to_string())));
        assert_eq!(store.mb_id_get("Tool\x1fLateralus"), Some(Some("mbid-rg-456".to_string())));
    }

    #[test]
    fn mb_id_round_trips_negative_result_distinct_from_absent() {
        let store = temp_store();
        store.mb_id_put("artist:Obscure", &None);
        assert_eq!(store.mb_id_get("artist:Obscure"), Some(None));
        assert_eq!(store.mb_id_get("artist:NeverLookedUp"), None);
    }

    #[test]
    fn clear_lookup_caches_drops_lookups_but_keeps_play_history() {
        let store = temp_store();
        store.bio_put("artist:X", &Some("bio".to_string()));
        store.mb_id_put("artist:X", &Some("mbid".to_string()));
        store.lyrics_put("X\x1fAlbum", &None);
        store.recently_played_put("srv", &[entry("f.flac", 1)]);

        store.clear_lookup_caches();

        assert_eq!(store.bio_get("artist:X"), None);
        assert_eq!(store.mb_id_get("artist:X"), None);
        assert!(store.lyrics_get("X\x1fAlbum").is_none());
        // History is app-generated state, not a cache — nothing could
        // re-derive it, so purging caches must leave it alone.
        assert_eq!(store.recently_played_get("srv").len(), 1);
    }

    #[test]
    fn art_cache_bytes_sums_blobs_and_ignores_negatives() {
        let store = temp_store();
        let limit = 100 * 1024 * 1024;
        store.art_put("A\x1fOne", &[7u8; 512], limit);
        store.art_put("A\x1fTwo", &[7u8; 256], limit);
        store.art_put_empty("A\x1fNone");
        assert_eq!(store.art_cache_bytes(), 768);
    }
}
