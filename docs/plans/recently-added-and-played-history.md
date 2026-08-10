# Plan: Recently Added (library) + Recently Played history (Now Playing)

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gaps #4/#5). Two
related but distinct features — "added" is a server-side query, "played" is
client-side history — sharing this file because both extend the existing
`RecentAlbum`/`recent_albums` machinery in `src/mpd/types.rs` /
`src/config/settings.rs`.

## Part A — Recently Added (new library section)

mikMPD (`README.md`): "Albums added or modified in the last 30 days, newest
first, as a list or art grid."

### Today

No equivalent view exists. `MpdClient` has no bounded "recently modified"
query yet.

### MPD side

mikMPD's own `CLAUDE.md` (Common Pitfalls) already learned the sharp edge
here: **unbounded library queries must be bounded** —
`find "(modified-since …)" window 0:2000` with an in-flight guard, because an
unbounded scan can outrun the socket read and disconnect mid-response. Port
the same shape:

- **New client method** (`src/mpd/client.rs`):
  ```rust
  pub async fn find_recently_added(&self, since: &str, limit: u32) -> MpdResult<Vec<Song>> {
      self.cmd(&format!(
          "find \"(modified-since '{}')\" window 0:{limit}",
          Self::escape(since)
      ))
      // parse like find()
  }
  ```
  `since` as an MPD-format timestamp (`YYYY-MM-DDTHH:MM:SSZ`) computed from
  `chrono::Utc::now() - Duration::days(30)` (already a dependency).
- Group the returned songs into albums (reuse `AlbumGroup`/`art_key` grouping
  from the library plan if that's landed, or a simpler
  `(album_artist, album)` dedup here if not — don't block this on the larger
  library rework) sorted by newest `last_modified` first.

### UI

- **New `View::RecentlyAdded`**, sidebar entry ("Recently Added" beneath
  "Genres", matching mikMPD's Library-tab placement).
- Reuse whatever the Albums list renders per-row (art thumbnail + title +
  artist) — this view is just a differently-sourced, differently-sorted feed
  into the same row widget, not a new visual design.
- `on_view_enter(View::RecentlyAdded)` fires `find_recently_added` →
  `Message::RecentlyAddedLoaded(Vec<Song>)`.

## Part B — Recently Played history (upgrade from sidebar list)

mikMPD (`README.md`): "Client-side listening history, accessible from the
clock button in Now Playing. Shows an album grid (tap to open the album) and
a per-track list; history is kept per server for 30 days / 100 entries."

### Today

`src/config/settings.rs` + `src/mpd/types.rs` already have most of the
scaffolding: `RecentAlbum { artist, album }`, `push_recent` (dedup,
move-to-front, **cap at 8**), persisted in `AppConfig.recent_albums`, updated
in `app.rs` on `CurrentSongUpdated` when the album changes
(`src/ui/app.rs:404-422`), rendered inline in the Now Playing left column
(`app.rs:1618`). This is album-level, capped at 8, and has no per-track
timestamps or a dedicated view — it's a "recently playing" glimpse, not the
history sheet mikMPD has.

**Decision: extend, don't replace.** Keep `RecentAlbum`/`push_recent`/the
inline Now Playing strip exactly as they are (they serve a different,
still-useful purpose: "what have I been bouncing between this session").
Add a **separate, richer history** underneath for the dedicated view.

### New data model (`src/mpd/types.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentlyPlayedEntry {
    pub file: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub played_at: i64,   // unix seconds (chrono, already a dependency)
}
```

Pure, testable retention helper (mirrors mikMPD's `prunedRecentHistory`):

```rust
pub fn prune_recently_played(entries: &mut Vec<RecentlyPlayedEntry>, now: i64) {
    const MAX_AGE_SECS: i64 = 30 * 86_400;
    const CAP: usize = 100;
    entries.retain(|e| now - e.played_at < MAX_AGE_SECS);
    entries.truncate(CAP); // caller ensures newest-first insertion order
}
```

### Recording — "committed after ~30s of continuous play"

MPD has no history command (winrmpc, like mikMPD, only sees `status`/
`currentsong` — this constraint is identical, not iOS-specific). Record from
the existing 500ms `Tick` poll (`src/ui/app.rs`, `Message::Tick` →
`refresh_status()`), not a new timer:

- **State**: a small reducer struct, pure and unit-testable like
  `push_recent`:
  ```rust
  pub struct PlayRecorder {
      file: String,
      accumulated_secs: f64,
      committed: bool,
  }
  impl PlayRecorder {
      pub fn tick(&mut self, file: &str, is_playing: bool, delta_secs: f64,
                   duration_secs: Option<f64>) -> bool // true = commit now
  }
  ```
  Rules (direct port of mikMPD's `RecentlyPlayedRecorder`): accumulate
  wall-clock deltas while playing and the file is unchanged; cap a single
  delta at 5s (survives a suspended/minimized window's coarse timer, though
  winrmpc's 500ms `Tick` makes this less likely than mikMPD's 1-2s poll);
  commit once `accumulated >= min(30, max(5, duration/2))`; file change or
  stop resets `accumulated`/`committed`; already-committed doesn't
  double-commit until the file changes.
- **Wiring**: in the existing `Message::CurrentSongUpdated`/`Tick` handling
  in `app.rs` (same spot `recent_albums` is already updated,
  `app.rs:404-422`), call `recorder.tick(...)` and on `true` push a
  `RecentlyPlayedEntry` to `self.recently_played`, then `prune_recently_played`.
- **Skip CD/radio** the same way the existing `recent_albums` push already
  does (`app.rs:406` comment: "Skip CD tracks") — extend that same guard.

### Persistence — per server

Mirrors mikMPD's per-server-profile storage. `AppConfig` currently has one
global `recent_albums: Vec<RecentAlbum>`; the new list should **not** follow
that pattern, since it's meaningless across servers. Two options:
1. Add `recently_played: HashMap<String, Vec<RecentlyPlayedEntry>>` keyed by
   server name to `AppConfig` (simplest, consistent with existing
   single-TOML-file storage — no new file format).
2. A dedicated file per server under the cache dir.

**Recommendation: option 1** — smaller diff, and `AppConfig` already has the
precedent of per-server data (`MpdServer.default_partition`). Load/save the
active server's slice on `SwitchServer`, matching how `default_partition`
restore already happens on server switch (`CLAUDE.md`'s "Server switching"
section).

### UI

- **Button** in Now Playing header (clock icon, next to wherever the
  existing lyrics/add-to-playlist controls live in `views/now_playing.rs`) →
  `Message::OpenRecentlyPlayed`.
- **New `View::RecentlyPlayed`** (or an overlay if winrmpc gains a modal
  pattern from the Add-to-Playlist picker's design — reuse whichever
  approach `docs/plans/playlists.md` §6c settled on for `AddToPlaylist`, for
  consistency): an Albums/Tracks toggle (default Albums, matching mikMPD's
  default), each tile/row showing art + title/artist + relative time
  ("3 min ago" — reuse or extend `format_duration`-style helpers, or a small
  new `relative_time(secs_ago: i64) -> String`).
  - Album mode: derive `Vec<RecentAlbum>`-shaped groups from
    `recently_played` the same way mikMPD's `recentAlbumGroups` derives from
    track history — **derive, don't record separately**, so there's one
    source of truth (mirrors mikMPD's explicit design choice in
    `../mikMPD/plans/recently-played-albums.md`).
  - Track mode: flat list, newest first, tap → play (`add_id` + `play_id`,
    same pattern as the existing `PlaySong` message from
    `docs/plans/enhancements.md` §1 if that shipped).
  - "Clear" action → empty the active server's history.

## Implementation order

| # | Item | Size |
|---|------|------|
| 1 | `find_recently_added` client method + test | S |
| 2 | `View::RecentlyAdded` + sidebar entry + `on_view_enter` wiring | S |
| 3 | `RecentlyPlayedEntry` + `prune_recently_played` + tests | S |
| 4 | `PlayRecorder` + tests | S |
| 5 | Per-server persistence in `AppConfig` | S |
| 6 | Recording wired into `Tick`/`CurrentSongUpdated` | S |
| 7 | `View::RecentlyPlayed` UI (Albums/Tracks toggle) | M |

## Testing

- `prune_recently_played`: drops >30-day entries, caps at 100, both at once,
  empty input (same shape as the existing `push_recent_*` tests in
  `types.rs`, keep them adjacent).
- `PlayRecorder`: commits at 30s; half-duration rule for a short track; no
  double-commit while the same file keeps playing; file-change resets;
  pause freezes accumulation; delta capped at 5s.
- Manual QA: play a track past 30s, open Recently Played, confirm it
  appears; switch servers, confirm history is server-scoped; Recently Added
  reflects a freshly-imported album within the 30-day window.
