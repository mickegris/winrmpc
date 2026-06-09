# Enhancement Plan — single-song actions, MPD log, lyrics

Status: **proposed** (not yet implemented). Three independent features; each
compiles and ships on its own. Implement in any order.

---

## 1. Add / Play single songs (Album, Search, Browser views)

### Today
- **Album view** (`views/album.rs`): each track row is one big `button` →
  `Message::QueueAddUri(file)` (appends + plays only if stopped).
- **Search view** (`views/search.rs`): row → `Message::SearchAddToQueue(file)`
  (same append-if-stopped behaviour).
- **Browser view** (`views/browser.rs`): likely the same pattern — confirm.
- So a click can only ever *append*. There is no "play this song now" and no
  explicit "add" vs "play" distinction.

### Goal
Per-row actions: **▶ Play now** and **＋ Add to queue**.

### Play-now semantics — DECISION
MPD already exposes the right primitives in `client.rs`:
- `add_id(uri) -> u32` (sends `addid`, returns the new song id)
- `play_id(id)` (sends `playid`)

Three options:
| Option | Behaviour | Verdict |
|---|---|---|
| **A. Insert + play (non-destructive)** | `add_id(uri)` → `play_id(id)`; queue is preserved, the song is appended and immediately played | **Recommended** — least surprising, keeps the queue |
| B. Replace queue | `clear` → `add` → `play` (this is what the existing `QueueAddAndPlay` does) | Destroys the queue; surprising from a track list |
| C. Append only | what we have today | Not "play now" |

Recommendation: **Option A**. Add one new message; reuse the existing
`QueueAddOnly` for the ＋ button.

### Changes
- **`message.rs`**: add `PlaySong(String)` (uri). (`QueueAddOnly(String)`
  already exists for the ＋ action.)
- **`app.rs`**: handle `PlaySong(uri)` →
  ```rust
  Task::perform(async move {
      if let Ok(id) = client.add_id(&uri).await {
          client.play_id(id).await.ok();
      }
  }, |_| Message::Tick)
  ```
- **`views/album.rs`** + **`views/search.rs`** (+ **browser.rs** if it matches):
  restructure each track row the way `queue.rs` was just restructured — the row
  becomes a `container` (for the alternating background) holding a `row!` of
  cells, with the title still triggering the default action and two small
  trailing icon buttons:
  - `▶` → `Message::PlaySong(file)`
  - `＋` → `Message::QueueAddOnly(file)`
  - Keep the title clickable as "play now" too (so a plain click = play), or
    leave title = add — pick one and be consistent across views.
- A tiny shared **icon-button** helper in `widgets/` (transparent bg, accent on
  hover) would keep album/search/browser consistent. Mirrors `widgets/link.rs`.

### Tests
- None strictly needed (UI + thin command wrappers). Optionally a smoke test
  that `PlaySong` maps to `add_id`+`play_id` is not worth the mock cost.

### Scope flag
`QueueAddUri` / `SearchAddToQueue` / `QueueAddAndPlay` become partly redundant
once per-row actions exist. Leave them for now (radio/CD still use some); prune
in a follow-up if they end up unused.

---

## 2. More MPD info in the Log view

### Today
- `logger.rs` `InAppLayer` captures all `tracing` events that pass the
  `winrmpc=info` EnvFilter (set in `main.rs`), into a 500-entry ring buffer.
- Almost nothing is logged except connect, partition-restore warnings, and
  errors. There is **no per-command logging**, so the log is sparse and what
  little shows is winrmpc-internal rather than MPD activity.

### Goal
Surface the actual MPD conversation (commands sent, OK/ACK results), and let the
view focus on MPD activity rather than internal noise.

### Approach
**Instrument the single command chokepoint** in `client.rs::cmd()` (the text
path) — it has both the command string and the result:

```rust
async fn cmd(&self, cmd: &str) -> MpdResult<Vec<(String, String)>> {
    let verb = cmd.split_whitespace().next().unwrap_or("");
    // Don't spam the log with the 500ms status poll trio.
    let routine = matches!(verb,
        "status" | "currentsong" | "playlistinfo" | "idle");
    if !routine {
        tracing::info!(target: "winrmpc::mpd", "→ {cmd}");
    }
    let mut guard = self.conn.lock().await;
    let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
    let result = conn.command(cmd).await;
    match &result {
        Ok(_)  if !routine => tracing::info!(target: "winrmpc::mpd", "← OK ({verb})"),
        Err(e)             => tracing::warn!(target: "winrmpc::mpd", "← {e} ({verb})"),
        _ => {}
    }
    result
}
```

Notes:
- **Target `winrmpc::mpd`** is deliberate: it keeps the `winrmpc` prefix so the
  existing `winrmpc=info` EnvFilter still passes it, and the Log view already
  strips `winrmpc::` → it renders as `mpd`.
- Errors are logged even for routine polls, so a dropped connection still shows.
- `cmd_binary` (albumart/readpicture) stays silent or DEBUG — it's high-volume
  and not interesting in the log.

### Log view filter — DECISION
Add a toggle so the user can collapse to MPD-only:
- **`message.rs`**: `LogToggleMpdOnly`.
- **`app.rs`**: `log_show_mpd_only: bool` state (default **true** — the user said
  the log is interesting for MPD info).
- **`views/log.rs`**: a header toggle button ("MPD only" ⇄ "All"); when on,
  filter entries to `entry.target.contains("mpd")`. `LogCopyAll` should copy the
  *currently visible* (filtered) set.

Alternative (simpler, no toggle): always show everything but color MPD lines
distinctly. The toggle is recommended since it directly matches the request.

### Tests
- Pure-logic only; the filter predicate could be extracted and unit-tested, but
  it's a one-liner. Skip unless desired.

---

## 3. Lyrics in Now Playing (two-column layout)

### Goal
Move art + song info to the **left**; show **lyrics on the right**.

### Lyrics source — DECISION
**LRCLIB** (`https://lrclib.net`) is the clear pick:
- Free, open, **no API key**, generous, purpose-built for players.
- `GET /api/get?artist_name=..&track_name=..&album_name=..&duration=..`
  → `200` JSON `{ plainLyrics, syncedLyrics, instrumental, ... }` or `404`.
- `duration` (seconds) lets it match the exact recording (±2s); we have
  `song.duration_secs`.
- Fallback: `GET /api/search?...` when the exact get misses.
- Returns **both** plain text and synced (LRC, `[mm:ss.xx]` timestamped) lyrics.

Rejected: Genius (needs key + HTML scraping, ToS), Musixmatch (paid), AZLyrics
(scraping/ToS), lyrics.ovh (plain-only, flaky).

`reqwest` is already a dependency (used by `musicbrainz.rs`) — reuse the pattern.

### New module
`src/lyrics/mod.rs` + `src/lyrics/lrclib.rs`:
```rust
pub struct Lyrics {
    pub plain: Option<String>,
    pub synced: Option<Vec<(f64, String)>>, // parsed LRC (secs, line)
}
pub struct LyricsClient { http: reqwest::Client }
impl LyricsClient {
    pub async fn fetch(&self, artist: &str, title: &str,
                       album: &str, duration_secs: Option<f64>) -> Option<Lyrics>;
}
```
- Send a descriptive `User-Agent` (LRCLIB asks for one), like `musicbrainz.rs`.
- LRC parser: split lines, parse `[mm:ss.xx]` prefixes → `(seconds, text)`,
  sort by time. Pure function → **unit-testable** (good test target).

### Caching — DECISION (user already chose "persistent" for recents; mirror it)
- In-memory `HashMap<String, Option<Lyrics>>` keyed by `"artist\x1ftitle"`.
- **Disk cache** under `AppConfig::cache_dir()/lyrics/<hash>.lrc` (store the raw
  synced or plain text). Lyrics are tiny; persisting avoids re-fetching every
  launch. Reuse the hashing approach from `art/cache.rs`.

### App wiring
- **state**: `lyrics: HashMap<String, Option<Lyrics>>`, `lyrics_client`.
- **message.rs**: `LyricsLoaded(String /*key*/, Option<Lyrics>)`.
- **app.rs `CurrentSongUpdated`**: when the song changes, if the key isn't
  cached, spawn a fetch (disk → LRCLIB) → `LyricsLoaded`. Skip `cdda://` and
  radio streams (no useful tags).

### Layout (`views/now_playing.rs`)
Restructure the playing branch into a top-level **two-column `row!`**:
- **Left column** (fixed ~340px): album art (300) + title/artist(link)/album
  (link) + tech/meta lines + Up Next, and **Recently Played** stays pinned at
  the bottom-left (as it is now).
- **Right column** (`Length::Fill`): a `scrollable` lyrics panel.
  - Heading "Lyrics".
  - If `plain`/`synced` present → render lines.
  - If `None` → muted "No lyrics found".
  - If still loading → "Loading lyrics…".

### Synced highlighting — DECISION (phasing)
- **Phase 1**: render **plain** lyrics (scrollable, static). Ship this first.
- **Phase 2** (stretch): when `synced` exists, highlight the current line using
  `status.elapsed` and auto-scroll. Requires passing `elapsed` into the view and
  a `scrollable` with programmatic scroll-to. Nice "wow" feature; more work.

Recommendation: build Phase 1, leave a `// TODO` for Phase 2.

### Tests
- LRC parser: timestamp parsing, multi-line, malformed-line tolerance,
  sort-by-time. (3–4 pure tests.)

---

## Suggested order
1. **#2 MPD log** — smallest, self-contained, immediately useful for debugging
   the other two.
2. **#1 single-song actions** — mechanical view restructure + one message.
3. **#3 lyrics** — largest (new module + HTTP + layout); Phase 1 then Phase 2.

## Open decisions to confirm before implementing
- **1A** Play-now = insert+play (non-destructive)? *(recommended)*
- **2** Log filter default = MPD-only with an "All" toggle? *(recommended)*
- **3** Lyrics Phase 1 plain-only first, synced highlight later? *(recommended)*
