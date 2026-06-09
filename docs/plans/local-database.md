# Plan: Local embedded database for caches & history

## TL;DR recommendation

**Yes — a small embedded DB is worth adding, but only for the *cache/history* layer, not for settings.** Keep `config.toml` exactly as-is (human-editable, tiny, git-diffable). Introduce a single embedded database file for:

1. **Art-cache metadata + LRU eviction** (closes a real latent bug — see below).
2. **Lyrics storage** (replaces the growing pile of `cache/lyrics/*.json`).
3. **Play history** (unlocks the already-planned "full recently-played history" view).

**Recommended engine: [`redb`](https://docs.rs/redb)** — pure-Rust, embedded, ACID, single file, zero C toolchain. Fallback: `rusqlite` (bundled) if we later want ad-hoc SQL reporting.

If we decide we don't care about cache-size enforcement or a history view, this can be skipped — the current flat-file approach works. But the unbounded art cache is a genuine defect, so doing *something* here is justified.

---

## 1. Current storage inventory

| Data | Where | Format | Problem? |
|------|-------|--------|----------|
| Settings (host, port, password, theme, radio stations, cd_device, recent_albums) | `%APPDATA%\winrmpc\winrmpc\config\config.toml` | TOML via `serde` | ✅ Fine. Small, human-editable. **Leave alone.** |
| Album/artist art | `cache_dir\<hash>.jpg` + in-memory `HashMap<String, Option<Vec<u8>>>` | JPEG files, hashed key | ⚠️ **`art_cache_size_mb` (default 500) is configured but never enforced.** `ArtCache` has no eviction, no size tracking, no last-access. Grows without bound. |
| Lyrics | `cache_dir\lyrics\<hash>.json` | JSON via `serde_json` | ⚠️ One tiny file per track; no index, no eviction, no metadata. Works but messy at scale. |
| Recent albums | inside `config.toml` (`recent_albums`, capped at 8) | TOML | ⚠️ Session-ish history jammed into the settings file; capped at 8; can't power a "full history" view. `now_playing.rs` already has a TODO for a "See all" history view. |

### The concrete bug this fixes
`src/config/settings.rs:12` declares `art_cache_size_mb: u32` (default `500`). Grep shows it is **read nowhere**. `src/art/cache.rs` never measures or trims the cache directory. On a large library the art cache can grow to many GB with no ceiling. Enforcing the configured limit requires per-entry **size** and **last-access** metadata — exactly what a DB gives cheaply.

---

## 2. Why a DB (vs. staying flat-file)

- **LRU eviction needs an index.** To honor `art_cache_size_mb` we must know each entry's size and last-access time, and evict oldest-first until under budget. Doing this with the filesystem means `stat`-ing every file on startup and parsing mtimes — slow and racy. A DB table makes it a single ordered scan.
- **Lyrics want one file, not thousands.** A key→blob table replaces N small JSON files, removes per-file open overhead, and gives us eviction for free.
- **Play history is append-only + queryable.** "Last 50 distinct albums, most recent first" is awkward in TOML, trivial in a KV/SQL store.
- **Atomicity.** redb/sqlite give crash-safe writes; today a half-written `.json`/`.jpg` is possible.

### Why NOT move settings into it
TOML config is a feature: users can hand-edit host/port/password, diff it, back it up. No reason to bury it in a binary DB. Hybrid is the right call.

---

## 3. Engine comparison

| Engine | Type | C deps / build | Async | ACID | Notes |
|--------|------|----------------|-------|------|-------|
| **redb** ✅ | Embedded KV (typed tables) | **None (pure Rust)** | Sync API (wrap in `spawn_blocking`) | Yes | Single file. Simple `TableDefinition` API. Ideal for blob + metadata. No build-toolchain risk on Windows. |
| rusqlite (bundled) | Embedded SQL | Compiles SQLite via `cc` (needs C compiler) | Sync | Yes | Mature, ad-hoc SQL, great if we want reporting queries. Adds a C build step. |
| sqlx (sqlite) | Async SQL | Needs SQLite lib or bundled | Yes | Yes | Heavier; compile-time-checked queries need a DB at build time or offline cache. Overkill here. |
| sled | Embedded KV | None | Sync | Yes | Popular but effectively in maintenance limbo; on-disk format churn risk. Avoid. |
| native_db | ORM over redb | None | Sync | Yes | Nice ergonomics but extra abstraction; redb directly is enough. |

**Pick `redb`.** Our access patterns are key→value (art blob, lyrics blob) plus a small append-only history list — no joins, no ad-hoc reporting. redb covers it with no C toolchain (a plus given `build.rs` already wrestles with `rc.exe`/`windres`). Choose `rusqlite` instead only if we anticipate rich SQL reporting on history.

---

## 4. Proposed schema (redb tables)

Single DB file at `cache_dir\winrmpc.redb`.

```
// Art: key = "artist\x1falbum" or "artist:<name>"; value = JPEG bytes
const ART:        TableDefinition<&str, &[u8]>
// Art metadata for eviction: key = same; value = bincode/serde (size_bytes, last_access_unix, is_empty)
const ART_META:   TableDefinition<&str, &[u8]>
// Lyrics: key = "artist\x1ftitle\x1falbum"; value = serde_json(Option<Lyrics>)
const LYRICS:     TableDefinition<&str, &[u8]>
// Play history: key = unix_millis (u64, monotonic insert); value = serde(RecentAlbum + title + ts)
const HISTORY:    TableDefinition<u64, &[u8]>
```

- **Eviction (art):** on store, update `ART_META`; periodically (startup + every N stores) sum `size_bytes`, and if over `art_cache_size_mb`, delete oldest-`last_access` rows from both `ART` and `ART_META` until under budget.
- **`store_empty`** negative cache (art not found) stays a metadata-only row with `is_empty=true`, so we don't refetch missing art every launch.
- **History dedup-on-read:** the Now Playing "recent 5" derives from a distinct-album scan of `HISTORY` (newest-first), replacing the `recent_albums` Vec in config. A future "See all" view reads the full table.

---

## 5. Implementation phases

**Phase 0 — add dep & module (no behavior change)**
- Add `redb = "2"` to `Cargo.toml`.
- New `src/store/mod.rs` + `src/store/db.rs`: open/create the DB, `Arc`-wrapped handle, typed get/put helpers, all DB calls inside `tokio::task::spawn_blocking` (redb is sync; keep the async surface the app already expects).

**Phase 1 — art cache → DB + enforce size**
- Reimplement `ArtCache` to back `get`/`store`/`store_empty`/`is_known`/`clear` with the `ART`/`ART_META` tables (keep the in-memory `HashMap` hot layer).
- Implement LRU eviction honoring `art_cache_size_mb`.
- **One-time migration:** on first run with the new build, optionally import existing `cache_dir\*.jpg` (we don't know their keys — they're hashed — so simplest is to *ignore/clear* old files and let art re-fetch lazily; document this). Cleaner: just `clear()` the old flat files on upgrade.

**Phase 2 — lyrics → DB**
- Point `fetch_lyrics` disk-cache read/write at the `LYRICS` table instead of `cache/lyrics/*.json`.
- Migration: ignore/delete old `lyrics/` dir; lyrics re-fetch lazily from LRCLIB.

**Phase 3 — play history → DB + history view (optional, larger)**
- On `CurrentSongUpdated` album-change, append to `HISTORY` instead of mutating `config.recent_albums`.
- Derive Now Playing "recent 5" from a distinct scan.
- Add a `View::History` showing the full list with paging (fulfills the existing `now_playing.rs` TODO).
- Keep `recent_albums` in config as a deprecated fallback for one release, then drop it.

**Phase 4 — settings/cleanup**
- Add a "Clear caches" button in Settings (art + lyrics) backed by `db.clear_*()`.
- Surface actual cache size in Settings (now that we track it).

---

## 6. Risks & mitigations

- **redb file-format stability:** pin a major version (`redb = "2"`); a future major bump can change on-disk format → gate with a schema-version key and rebuild-on-mismatch (cache is disposable, so "wipe & rebuild" is acceptable).
- **Blocking I/O on the async runtime:** always wrap redb calls in `spawn_blocking`. Never hold a redb transaction across an `.await`.
- **Lost cache on upgrade:** acceptable — both art and lyrics re-fetch lazily. Communicate in release notes ("art/lyrics caches rebuild on first launch").
- **Single-writer:** redb is single-process; fine for a desktop app (one instance). If we ever allow multiple instances, revisit.

## 7. Effort estimate
- Phase 0–2 (art + lyrics + eviction): ~1 focused session. **Highest value** (fixes the unbounded-cache bug, tidies lyrics).
- Phase 3 (history + view): ~1 session. Optional, user-facing.

## 8. Decision checklist
- [ ] Do we want `art_cache_size_mb` actually enforced? → if yes, do Phases 0–1.
- [ ] Do we want a full play-history view? → if yes, do Phase 3.
- [ ] OK with `redb` (pure Rust) vs `rusqlite` (SQL + C build)? → default **redb**.
- [ ] Accept one-time cache wipe on upgrade? → yes (caches are disposable).
