# Plan: Queue editing — remove, reorder, Add Next

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gap #1).

## Today (`src/ui/views/queue.rs`, `src/ui/app.rs`)

The queue view (`views::queue::view`) renders title/artist/album/duration per
row with only two interactive elements per row: the title (plays that track
via `Message::QueuePlay(pos)`) and a `☰` "Add to Playlist" button. There is
**no way to remove a single song, no reorder, and no "insert after current
track"**. Toolbar-level actions are `QueueShuffle` and `QueueClear` only
(`queue.rs:12-25`). `MpdClient` already has everything needed server-side:
`delete_pos(pos)`, `move_pos(from, to)`, `add_id(uri) -> u32`
(`src/mpd/client.rs:211,223,202`).

mikMPD's Queue tab (`README.md`): "Double-tap to jump to a song, drag to
reorder, swipe to delete, shuffle the queue in place, clear all, toggle
consume mode. 'Add Next' (from album, search and playlist rows) queues a
track right after the one playing."

## 1. Remove a single row

- **`message.rs`**: `QueueRemove(u32)` (pos).
- **`app.rs`**: `Message::QueueRemove(pos) => self.mpd_cmd(|c| async move { c.delete_pos(pos).await })`
  followed by the existing queue-refresh path (the 500ms `Tick` will pick it
  up, but fire an immediate `RefreshQueue`-style task the way other mutating
  handlers already do so the row disappears without a half-second lag).
- **`views/queue.rs`**: add a third icon button per row, `✕` (reuse
  `icon_btn` from `widgets/link.rs`, same as the existing `☰`) →
  `Message::QueueRemove(pos)`.

## 2. Reorder

iced 0.13 has no built-in drag-and-drop list (mikMPD's `.onMove` has no
direct analogue). Use **up/down buttons** per row, matching the pattern the
`enhancements.md` plan already chose for CD track probing and other
list-adjacent actions in this codebase (small `▲`/`▼` `icon_btn`s):

- **`message.rs`**: `QueueMoveUp(u32)`, `QueueMoveDown(u32)` (pos).
- **`app.rs`**: `QueueMoveUp(pos)` → `client.move_pos(pos, pos.saturating_sub(1)).await`
  (no-op guarded when `pos == 0`); `QueueMoveDown(pos)` → `move_pos(pos, pos + 1)`
  (no-op guarded when `pos == queue.len() - 1`, checked in the view by simply
  not rendering the button on the last row — mirrors how the CD/playlist
  detail views already omit actions that don't apply to the boundary row).
- **`views/queue.rs`**: two more `icon_btn`s per row, disabled/hidden at the
  first/last position.

This is deliberately simpler than mikMPD's drag gesture — same end state
(any-to-any reorder via repeated single-step moves), less UI work, and
consistent with how this app already avoids custom gesture recognizers.

## 3. "Add Next"

Insert a URI immediately after the currently-playing song, without disturbing
anything else in the queue — mikMPD's version is reachable from album, search,
and playlist rows.

- **`message.rs`**: `QueueAddNext(String)` (uri).
- **`app.rs`**: needs the current queue position, which is already tracked
  (`self.status.song_pos` or equivalent — confirm the exact field via
  `Status` in `types.rs`). Implementation:
  ```rust
  Message::QueueAddNext(uri) => {
      let client = self.client.clone();
      let insert_at = self.status.as_ref().and_then(|s| s.song_pos).map(|p| p + 1);
      self.mpd_cmd(move |c| async move {
          if let Ok(id) = c.add_id(&uri).await {
              if let Some(pos) = insert_at {
                  // add_id appends to the end; move it to just after current.
                  let end = c.queue().await.map(|q| q.len() as u32 - 1).unwrap_or(pos);
                  c.move_pos(end, pos).await.ok();
              }
          }
      })
  }
  ```
  Note: `move_pos` operates on **positions**, and `add_id` appends at the end
  of the queue — so the sequence is add → find its position (the new queue
  length − 1, or track the id and re-resolve) → move. Verify against
  `move_pos`'s actual MPD semantics (`move FROM TO`) before implementing;
  there may be an off-by-one to work out depending on whether TO is "insert
  before" or "insert after" in MPD's `move` command (check the protocol
  docs referenced in mikMPD's own `CLAUDE.md`: `mpd.readthedocs.io`).
- **Wire-up points** (mirrors mikMPD's "from album, search and playlist
  rows"): `views/album.rs` per-track row, `views/search.rs` result row,
  `views/playlist_detail.rs` per-track row — one more `icon_btn` (e.g. `⏭`)
  next to the existing add/play buttons, → `Message::QueueAddNext(file)`.

## 4. Consume mode toggle

CLAUDE.md already lists "consume mode toggle" as an existing player-bar
feature (`src/ui/widgets/player_bar.rs`) — confirm it's wired to
`set_consume` before assuming this is a gap; if it's already there, this
section is a no-op (verify at implementation time, don't re-plan it here).

## Implementation order

| # | Item | Size |
|---|------|------|
| 1 | `QueueRemove` (message + handler + row button) | XS |
| 2 | `QueueMoveUp`/`QueueMoveDown` (message + handler + row buttons) | S |
| 3 | `QueueAddNext` (message + handler + three view wire-ups) | S |

## Testing

- Pure logic: none of this needs new unit tests beyond what `move_pos`
  already implies at the command-string level (if `move_pos`'s command
  string isn't already covered by an `escape`/format test, add one for the
  new `QueueAddNext` position-resolution arithmetic if it ends up being
  more than a one-liner).
- Manual QA: remove a middle row (queue shifts, current-playing indicator
  stays correct); move the currently-playing row up/down (playback doesn't
  hiccup); Add Next from Album/Search/Playlist while something is playing
  and while the queue is empty (falls back to plain `add_id`+play, matching
  existing "queue empty" fallbacks elsewhere in the app).
