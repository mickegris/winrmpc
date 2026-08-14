# Every artist and album name should be a link

Part of [ui-0.4.3](ui-0.4.3.md).

## The report

> All album names and artist names across the app should be clickable to links.
> You should always be able to click on such things and come to the right view.

This is a consistency rule, and the app currently keeps it in exactly one view.

## What exists now

**The Queue is the model.** `views/queue.rs:87-126` builds three separate
buttons per row — title (plays), artist (`ArtistSelected`), album
(`AlbumSelected`) — so every name in the row is a link to the obvious place.

Nowhere else does this. The audit, by view:

| View | Album name | Artist name | Why |
|---|---|---|---|
| **Queue** | ✅ link | ✅ link | the model |
| **Now Playing** (main) | ✅ link | ✅ link | `now_playing.rs:75-89` |
| Album detail (header) | — (is the page) | ✅ link | `album.rs:66-71` |
| Artist detail | ✅ per album | — (is the page) | `artist.rs:167` |
| **Albums list / grid** | ✅ whole row/tile | ❌ **plain text** | see below |
| **Recently Added** | ✅ whole row/tile | ❌ **plain text** | shares `album_grid` |
| **Recently Played** (albums) | ✅ whole row/tile | ❌ **plain text** | `recently_played.rs:96-97` |
| **Recently Played** (tracks) | — not shown | ❌ **plain text** | `recently_played.rs:135` |
| **Search** (song rows) | — not shown | ❌ **plain text** | `search.rs:121` |
| **Search** (album headers) | ✅ link | ❌ not shown separately | `search.rs:68` |
| **Now Playing** (Up Next) | — not shown | ❌ **plain text** | `now_playing.rs:137` |
| **Browser** (file rows) | — | ❌ **not even separate** | see below |
| **Genre detail** | ⚠️ link, but degraded | — not shown | see finding 2 |

## Finding 1 — the tile/row is *one* button, so the artist inside it can't be another

This is the structural reason most of the table is ❌, and it needs a design
decision rather than a find-and-replace.

`widgets/album_grid.rs:23-63` — `tile()` takes a single `on_press` and wraps
cover + title + subtitle in one button. `views/albums_list.rs:60-71` does the
same for list mode: the whole row, artist text included, is one button whose
press is `AlbumSelected`.

So the artist name is *inside* the album button. Making it independently
clickable means either:

1. **Nesting a button in a button.** iced allows it and the inner one wins the
   click, but the outer button's hover highlight then covers a region that
   behaves differently in one spot, which is exactly the kind of "why did that
   do something else" the row-action work spent 0.4.2 removing.
2. **Restructuring so they are siblings.** The cover becomes the album button;
   the caption lines become their own link buttons stacked beneath it. No
   nesting, each target obvious.

**Recommend 2.** It costs a layout change in `album_grid::tile` and
`albums_list`, and it makes the grid behave like the Queue rows already do.

The cost to weigh: in grid mode the whole tile currently being clickable is a
big, forgiving target. Splitting it means the *cover* is the album target and
the title line is a second one — so keep **both the cover and the title** firing
`AlbumSelected`, and only the artist subtitle differs. That preserves the large
target for the common action.

## Finding 2 — Genre detail navigates to a *degraded* album view

`views/genre_detail.rs:24`:

```rust
.on_press(Message::AlbumSelected(album.clone(), None))
```

The `None` is deliberate — a genre listing isn't artist-grouped, so there is no
artist to pass — but per CLAUDE.md it has consequences that a user experiences
as bugs:

- **No multi-disc expansion.** `AlbumSelected` with `artist: None` falls back to
  a plain `find("Album", base)`, so a multi-disc album opened from a genre shows
  only the tracks whose literal `Album` tag matches.
- **Degraded bio lookup.** `fetch_album_bio` with an empty artist becomes a
  title-only Wikipedia search.

So "click the album, get the right view" is *already* not quite true from
Genres. Worth fixing in the same pass: the genre listing could carry the album
artist alongside the album name (it comes from the same `find` results), which
would make every `AlbumSelected` in the app artist-scoped and let the
`artist: None` fallback path be deleted entirely.

That is a data-shape change (`genre_albums: HashMap<String, Vec<String>>` →
`Vec<AlbumGroup>` or `Vec<(String, String)>`), not a view change. It may deserve
to be its own item.

## Finding 3 — the Browser concatenates the artist into a string

`views/browser.rs:87`:

```rust
let label = format!("{} – {}", s.display_artist(), s.display_title());
```

Artist and title are fused into one text run, so neither can be a link and the
artist can't even be styled differently. Splitting them into two cells (artist
muted, title primary) matches every other list and is a prerequisite for making
the artist clickable.

## Plan

### A. A shared link widget for names

Add to `ui::widgets::link`:

```rust
pub fn artist_link<'a>(name: &str, size: u16) -> Element<'a, Message>;
pub fn album_link<'a>(album: &str, artist: Option<&str>, size: u16) -> Element<'a, Message>;
```

Both emit the right `Message` themselves, so no call site has to remember that
`AlbumSelected` takes an `Option<String>` artist or what to put in it. This is
the same reasoning as `song_row::number` — one definition, so the twelve call
sites can't disagree.

Styling: these are links, so they should look like the Queue's do — secondary
colour at rest, accent on hover. **Not** underlined; at size 11-12 in a dense
list, underlines turn a track list into a mess.

### B. Restructure `album_grid::tile` and `albums_list` (finding 1)

Cover **and** title fire `AlbumSelected`; the artist subtitle becomes an
`artist_link`. Applies automatically to Albums, Recently Added and Recently
Played, since all three render through `album_grid`.

### C. Fill in the plain-text artists

`recently_played.rs` (both modes), `search.rs` song rows, `now_playing.rs`
Up Next. Each becomes an `artist_link`.

Note **Up Next is itself a button** (`QueuePlay`), so it has finding 1's
nesting problem in miniature — the same fix applies: make the row's columns
siblings rather than nesting a link inside the play button.

### D. Split the browser's label (finding 3)

Two cells, artist then title, artist linked.

### E. Artist-scope every album navigation (finding 2)

Carry the album artist through the Genres data so `AlbumSelected(_, None)`
stops being reachable from the UI. Whether the `None` arm is then deleted or
kept as a defensive fallback is a judgement call to make when the data change
lands — deleting it is cleaner, keeping it means one less way to break the
`GenreDetail` path.

## Consequences worth accepting up front

- **"Unknown Artist" / "Unknown Album" must not be links.** `display_artist()`
  falls back to those strings; navigating to an artist page for a literal
  "Unknown Artist" produces a junk view. `artist_link` should render
  non-interactive muted text for the known fallback values.
- **Compilations.** Clicking the *track* artist on a compilation goes to that
  artist, not to the album's `AlbumArtist`. That is the correct behaviour and
  worth stating, since the row's album link goes somewhere the artist link
  doesn't.
- **More clickable regions per row means more hover states.** Dense lists can
  end up feeling twitchy. Worth a look at real density before deciding whether
  the hover highlight should be per-cell or suppressed for name links.

## How to confirm

Code can't answer this one; it needs clicking.

- From **each** of Albums (grid *and* list), Recently Added, Recently Played
  (albums *and* tracks), Search, Browser, Queue and Now Playing's Up Next:
  click the artist and land on that artist's page; click the album and land on
  that album.
- Open a **multi-disc album from Genres** and confirm every disc's tracks are
  present — that is finding 2, and it is the one item here that is a real bug
  rather than a missing affordance.
- Click an "Unknown Artist" row and confirm nothing happens.
- Click a **compilation** track's artist and confirm it goes to the track
  artist, then the album link and confirm it goes to the compilation.
