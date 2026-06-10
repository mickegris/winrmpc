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
}

impl Store {
    /// Open (or create) the cache database under `cache_dir`. Falls back to an
    /// in-memory backend if the file can't be opened, so the app still runs
    /// (caches just won't persist that session).
    pub fn open(cache_dir: &Path) -> Self {
        std::fs::create_dir_all(cache_dir).ok();
        let path = cache_dir.join("winrmpc.redb");
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
                    tracing::warn!(
                        "Rebuilding cache DB failed ({second_err}); using in-memory cache"
                    );
                    Database::builder()
                        .create_with_backend(redb::backends::InMemoryBackend::new())
                        .expect("in-memory redb backend")
                })
            }
        };
        let store = Self { db: Arc::new(db) };
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
            let _ = wtx.open_table(META);
            let _ = wtx.commit();
        }
    }

    /// One-time cleanup: builds before 2026-06-10 let the empty-URI
    /// recently-played art fetch persist negative entries even though it could
    /// only try MusicBrainz, permanently blocking the MPD embedded-art path
    /// for those albums. Purge all negative entries once so they re-resolve;
    /// genuinely missing art just gets re-recorded on the next real lookup.
    fn purge_poisoned_negatives(&self) {
        const MARKER: &str = "neg_purge_v1";
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

        if let Ok(wtx) = self.db.begin_write() {
            {
                if let Ok(mut table) = wtx.open_table(ART_META) {
                    for k in &empties {
                        let _ = table.remove(k.as_str());
                    }
                }
                if let Ok(mut table) = wtx.open_table(META) {
                    let _ = table.insert(MARKER, [1u8].as_slice());
                }
            }
            let _ = wtx.commit();
            if !empties.is_empty() {
                tracing::info!(
                    "Purged {} stale negative art-cache entries",
                    empties.len()
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
}
