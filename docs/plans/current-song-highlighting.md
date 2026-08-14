# Current-song highlighting in every song list

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

> **Status (2026-08-14): A, B and C implemented. D deferred as planned, E out
> of scope.** Six views now mark the playing track. The match rule is unit
> tested, including the position-vs-URI trap this plan was written around; what
> is *not* verified is the visual result, which needs the running app — see
> "Verification" at the bottom.

## What exists now

**Exactly one view highlights the playing track: the Queue.**
`views::queue::view(&self.queue, self.status.song_pos)` (`app.rs:2275`) and
inside it (`queue.rs:62-74`):

```rust
let is_current = current_pos == Some(pos);
let bg = if is_current { … };
let title_color = if is_current { AppColors::ACCENT } else { … };
```

Every other song list renders identical rows regardless of what's playing —
verified by grepping each view for `current_song`/`song_pos`/`is_current`:
`album.rs`, `playlist_detail.rs`, `search.rs`, `browser.rs`,
`recently_played.rs` all come back empty (their `AppColors::ACCENT` hits are
back-buttons and links, not row state).

## The key question: position or URI?

This is the one design decision in the plan, and getting it wrong produces
wrong highlighting rather than missing highlighting.

**The Queue is right to use `song_pos`, and it is the only list that should.**
A queue can legitimately contain the same file twice; position is what
distinguishes them, and MPD's `status` gives it to us directly.

**Every other list must match on `Song::file` (the URI)**, because:

- The rows aren't queue entries at all — an album's track list, a stored
  playlist's contents, search results and the file browser are library
  listings. Their `pos` field is either absent or means something else
  entirely (in `playlist_detail.rs:90` `pos` is the *playlist* index, which has
  no relationship to the queue position in `status.song_pos`).
- Matching on position across those lists would highlight an arbitrary
  unrelated row. This is the failure mode to avoid.

So: `let is_current = Some(song.file.as_str()) == current_song.map(|s| s.file.as_str());`

Consequences worth accepting up front:

- **The same track appearing twice in one album/playlist listing highlights
  both.** Correct, and rare enough not to complicate the rule.
- **A track present in the library and also playing from a radio stream** won't
  collide, since stream URIs are `http(s)://…`.
- **CD tracks** (`cdda://…`) match fine — same URI shape both sides.

## Should a stopped player still highlight?

`status.state == PlayState::Stop` still leaves `current_song` populated (MPD
keeps a current song when stopped). The Queue highlights it regardless of
state today. Keep that behaviour and apply it uniformly — consistency with the
existing view matters more than the distinction, and the player bar already
communicates play/pause/stop.

## Plan

### A. A shared row-state helper

The Queue's inline styling should not be copy-pasted into five more views.
Add `src/ui/widgets/song_row.rs`:

```rust
/// Background for a list row, accounting for zebra striping and whether this
/// row is the currently playing track.
pub fn row_bg(index: usize, is_current: bool) -> iced::Color;

/// Title colour for a list row.
pub fn title_color(is_current: bool) -> iced::Color;

/// The leading now-playing marker: a fixed-width cell so rows stay aligned
/// whether or not the marker is present.
pub fn playing_marker<'a>(is_current: bool) -> Element<'a, Message>;
```

Add one colour to `ui/theme/colors.rs` — there is no `ROW_PLAYING` today, and
the Queue currently improvises. Something between `BG_HOVER` and `ROW_ODD` so
it reads as selected without fighting the zebra striping.

`playing_marker` matters for alignment: pushing a `▶` into the row only when
current would shift every other column. Render a fixed-width container that is
either the glyph or blank. **Which glyph** depends on
[row-action-affordance](row-action-affordance.md) — that plan bundles an icon
font because the current `Segoe UI Symbol` reference renders as tofu on macOS
and Linux. Use the same font here, or this marker is the next tofu box.

Rewrite `queue.rs` to use the helper (it keeps its `song_pos` comparison — only
the styling is shared, not the match rule).

### B. Thread the current song into each view

`App` already holds `current_song: Option<Song>`, so this is a signature change
per view plus a call-site change in `app.rs`. Pass `Option<&str>` (the URI)
rather than `&Option<Song>` — it makes the match rule obvious at the call site
and keeps the views from reaching for other fields.

| View | Call site | Notes |
|---|---|---|
| `album.rs` | `app.rs:2337` | track list, `songs.iter()` at `album.rs:150` |
| `playlist_detail.rs` | `app.rs:2406` | must match on `song.file`, **not** `pos` |
| `search.rs` | `app.rs:2351` | song rows only; the artist/album sections above them aren't tracks |
| `browser.rs` | `app.rs:2348` | only `DirectoryEntry::File` rows |
| `recently_played.rs` | — | tracks mode only; Albums mode is covered by D |

### C. Now Playing's recents strip

`now_playing.rs` already knows the current song. Its recents strip is album
tiles, so it falls under D rather than C — no per-track work needed here.

### D. Album-level highlighting (optional, second pass)

Grids and album lists could mark the album that contains the playing track —
`albums_list.rs`, `artist.rs`, `genre_detail.rs`, `recently_played.rs` in
Albums mode, and the shared `widgets/album_grid.rs`. The match would be on
`album_scoped_key(current_song.display_album_artist(), base)` so it agrees
with the disc-collapsing rule in `album_base_and_disc` — a playing `Disc 2`
track must highlight the single collapsed album row.

Worth doing, but it's a different comparison with its own edge cases. Land B
first; treat D as a follow-up so a bug in album-key matching can't hold up the
straightforward track-list work.

### E. Not in scope

- **Auto-scrolling a list to the playing track.** Different feature, and iced
  0.13's scroll APIs make it awkward (the same limitation CLAUDE.md records for
  visible-range art fetching). The lyrics panel does it via
  `scrollable::Id` + `snap_to`, so it's possible — just not part of this.
- **Radio view.** Stations are URLs, not songs; matching `current_song.file`
  against a station URL would work, but the Radio view has no row-state concept
  yet. Fold in later if wanted.

## What was actually built

**A — `src/ui/widgets/song_row.rs`.** The three styling helpers from the plan,
plus the two *match predicates* (`is_current_uri`, `is_current_pos`), which
turned out to be the part worth extracting: putting them in the same module as
the doc comment explaining position-vs-URI is what makes the rule discoverable
from a call site. `AppColors::ROW_PLAYING` was added as specified; the Queue's
rewrite dropped its improvised `BG_TERTIARY`.

One layout detail the plan called correctly and which cost real care:
`playing_marker` is a fixed-width cell that is *either* the glyph or blank.
The Queue's `#` header had to move 40 → 48px to stay aligned with it.

**B — six views threaded.** Album, Playlist detail, Search, Browser, Recently
Played (Tracks) and the Queue. `App::current_file()` supplies `Option<&str>`.

The plan listed five views; Search needed the extra care its note implied —
its row index is a running counter across the artist/album/song *sections*,
not `enumerate()` over one list, so the zebra parity had to keep using that
counter rather than a fresh index.

**C — nothing to do**, as the plan predicted: Now Playing's recents strip is
album tiles, so it belongs to D.

**D — deferred, deliberately.** Album-level highlighting is a different
comparison (`album_scoped_key` against the disc-collapsed base, so a playing
Disc 2 track marks the single collapsed row) with its own edge cases. Landing
B first means an album-key bug can't hold up the straightforward work — which
is exactly the plan's own reasoning, kept.

**E — out of scope**, unchanged: no auto-scroll-to-playing, no Radio view.

Test count 191 → 200. The nine new tests cover the match rule rather than the
rendering, per the plan: notably that a playlist row whose `pos` equals the
playing `song_pos` but whose file differs does **not** match, that the same
file at two queue positions is distinguished, that duplicate URIs in one
listing both match (an accepted consequence, asserted so it stays a decision),
and that `ROW_PLAYING` is distinct from both zebra stripes and from
`BG_HOVER` — a highlight equal to either stripe would be invisible on half the
rows.

## Verification

Unit-testable parts are thin (this is view code), but the match rule is not:
add tests in `mpd/types.rs` or alongside the helper for the URI-comparison
predicate, specifically that a `playlist_detail` row with `pos == song_pos`
but a **different** `file` does *not* match. That is the exact bug this plan
is written to avoid.

Manual: play a track from an album, then open that album, the stored playlist
containing it, a search that returns it, and the browser folder holding it —
the same row should be marked in all four, and no other row anywhere.
