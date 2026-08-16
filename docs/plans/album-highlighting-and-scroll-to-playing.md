# Album-level highlighting, and finding the playing track

Part of [ui-0.4.3](ui-0.4.3.md). **Not requested — proposed.** Finishes work
deliberately deferred out of 0.4.2.

## Where this comes from

[current-song-highlighting](current-song-highlighting.md) shipped steps A–C in
0.4.2: six song lists now mark the playing **track**. It deferred two items on
purpose, and both are still open:

- **Step D — album-level highlighting.** Marking the album that *contains* the
  playing track, in the grids and album lists. Deferred because it is a
  different comparison with its own edge cases, and landing the track work first
  meant a bug in album-key matching couldn't hold it up.
- **Step E — scrolling a list to the playing track.** Ruled out of scope, not
  ruled out.

Both are now worth doing, and D's stated risk has since been de-risked from an
unexpected direction: 0.4.2's row-layout work already forced every list through
`widgets/song_row.rs`, so there is one place to put the album predicate too.

## D — mark the album containing the playing track

### The comparison

Not `album == album`. It has to be `album_scoped_key(artist, base)` against the
**disc-collapsed** base name, because a playing `Disc 2` track must light up the
single collapsed album row that represents the whole set — the row the user is
looking at doesn't have "Disc 2" in its name.

```rust
// widgets/song_row.rs
pub fn is_current_album(group_artist: &str, group_base: &str, current: Option<&Song>) -> bool
```

built on `album_scoped_key` / `album_base_and_disc`, so it agrees with the
grouping rule by construction rather than by coincidence.

### Where it applies

`albums_list.rs`, `artist.rs`, `genre_detail.rs`, `recently_played.rs` (Albums
mode), and the shared `widgets/album_grid.rs` — which means `tile()` and the
list rows both need a `is_current` parameter, exactly as the song rows did.

### The visual

Song rows use `ROW_PLAYING` background + `ACCENT` title + a leading play glyph.
A grid **tile** has no row to tint. Options:

- accent border on the cover,
- the play glyph overlaid on the cover corner,
- accent-coloured title text only.

**Recommend accent title + a border on the cover.** A glyph overlay needs
`iced::widget::stack` and has to survive the cover being a placeholder block;
the title colour alone is too subtle at grid density.

### Edge cases to get right

- **Two artists, same album title** — the reason the key is artist-scoped.
- **Radio streams.** `display_album()` on a stream is usually junk or empty;
  the predicate must not light up an album called "" or match every album when
  nothing sensible is playing.
- **`Unknown Album`.** Same problem: a literal-match would mark every untagged
  album at once. Both this and the stream case argue for the predicate returning
  `false` for the known fallback strings.

## E — scroll to the playing track

### Why it was deferred, and what changed

CLAUDE.md records that iced 0.13 exposes no per-item scroll position, which is
also why background art fetching has no visible-range optimisation. That is
still true. But **scrolling *to* a known index is a different problem from
knowing what is visible**, and the lyrics panel already does it:
`scrollable::snap_to(id, RelativeOffset { y })` with the ratio computed from the
line index (`app.rs::lyrics_autoscroll`).

The same trick works for a song list: `index / (len - 1)` as a relative offset.
It is approximate for the same reason it is approximate in the lyrics pane —
it assumes uniform row heights — but song rows genuinely *are* uniform height,
so it should land more accurately here than it does on wrapped lyric lines.

### Shape

**A button, not automatic scrolling.** The lyrics pane learned this the hard
way in 0.4.2: unconditional autoscroll made the panel unreadable, and the fix
was an explicit Sync/Scroll toggle. A list that yanks itself around while you
are trying to read it is the same mistake.

So: a small "Jump to current" control, shown only when the playing track is in
the list being viewed and not already visible. Since "not already visible"
isn't knowable (see above), showing it whenever the list *contains* the playing
track is the honest simplification.

Each list needs a `scrollable::Id` — currently only the lyrics pane has one.

## Plan

1. `song_row::is_current_album`, with tests for the disc-collapse case, the
   two-artists-same-title case, and the `Unknown Album`/stream fallbacks. This
   is the piece that can be got wrong invisibly, so it gets tests first.
2. Thread `is_current` into `album_grid::tile` / `list_thumb` call sites.
3. Pick and apply the visual; check it at grid density before committing.
4. Give each song list a `scrollable::Id`; add `Message::JumpToCurrent`.
5. "Jump to current" control in the lists that can contain the playing track.

Steps 1–3 and 4–5 are independent; either can land alone.

## How to confirm

- Play a **Disc 2** track and open Albums: the single collapsed album row for
  that set is marked, not nothing and not two rows.
- Two artists with an identically titled album: only the right one is marked.
- Play a **radio stream** and confirm no album is marked anywhere.
- With a long queue, press "Jump to current" and confirm the playing row is
  actually on screen — the ratio approximation is exactly the thing that needs
  eyes rather than reasoning.
