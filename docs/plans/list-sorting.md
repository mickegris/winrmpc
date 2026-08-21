# Plan: A–Z / Z–A sorting for the library lists

Status: **implemented** (0.4.4). Requested for the Artists view; this
covers which other lists should get it and which deliberately shouldn't.

## What's there now

Every library list is sorted **once, at load time, in the task that fetches
it**, and the direction is not a thing the user can change:

| List | Sorted at | How |
|---|---|---|
| Artists | `app.rs:3538` (`on_view_enter`) | `artists.sort()` |
| Albums | `group_albums_by_artist` order | first-seen order of MPD's `list` |
| Artist detail → albums | `app.rs:1096` | `albums.sort()` then grouped |
| Genres | server order | not sorted at all |
| Playlists | server order | not sorted at all |
| Recently Added | `app.rs:1260` | `last_modified` descending |

### The existing sort is already wrong

`Vec<String>::sort()` is **byte order**, so every lowercase initial sorts
after every uppercase one: `ZZ Top` (`Z` = 0x5A) lands before `a-ha`
(`a` = 0x61). A real library has `a-ha`, `dEUS`, `will.i.am`, `k.d. lang` —
all currently exiled to the bottom of the Artists list, well past Z. That is a
bug this plan should fix whether or not the direction toggle lands, and it is
probably part of why the list feels arbitrary today.

Fix: one shared `name_cmp(a, b)` — case-folded, with a byte-order tiebreak so
the ordering stays total and stable for names differing only in case.

**Not** in scope: ignoring a leading article, so "The Beatles" files under B.
That is an opinionated, locale-flavoured transform (and `normalize_for_lookup`
already moves the article the *other* way for lookups, which would be actively
confusing to reuse here). Worth its own decision later.

## The control

**A text button reading `A–Z` / `Z–A`**, in each list's header row beside the
existing count and — where there is one — the Grid/List toggle.

No new glyph, deliberately. The icon font is a 17-glyph subset and adding to
it means editing `packaging/fonts/build-icon-font.py`, a `fontTools` venv, and
re-passing the `icon.rs` parity tests. Worse, the obvious reuse is barred:
`MOVE_UP`/`MOVE_DOWN` *are* `arrow_upward`/`arrow_downward`, and
`icon::tests` asserts no two constants share a codepoint, so a `SORT_ASC`
alias would fail the suite. And a bare arrow doesn't say *what* is being
sorted anyway. This is the same argument CLAUDE.md already makes for Single
and Consume keeping their words: `A–Z` is unambiguous where a glyph isn't.

## State and where the sort happens

`AppConfig::sort_desc: bool` (`#[serde(default)]`, persisted), one flag for
every name-sorted list — the same call `album_grid_view` made, and for the
same reason: per-list sort memories would feel arbitrary rather than helpful.

`Message::ToggleSortDirection` flips it, `save_and_log`s, and **re-sorts the
already-loaded lists in memory**. It must not trigger a refetch — the data is
in hand, and a round trip to reverse a list the user is looking at would show
as a flicker.

Sorting stays in the `update` handlers, never in `view()`: views take
`&'a [T]` and would have to allocate a reordered copy every frame.

So there are two entry points, and both must use the same comparator:
`App::sort_library_lists()`, called from `ToggleSortDirection` and from each
`*Loaded` handler.

## Which lists get it

| List | Toggle | Why |
|---|---|---|
| **Artists** | ✅ | the requested one, and the longest list in the app |
| Albums | ✅ | same shape; sorts on `(artist, base)` so an artist's albums stay together |
| Genres | ✅ | currently unsorted entirely, which is worse than either direction |
| Artist detail → albums | ✅ | already name-sorted, just not reversible |
| Genre detail → albums | ✅ | same |
| Playlists | ✅ | currently server order |
| **Recently Added** | ❌ | it is a *recency* list; alphabetising it destroys the only thing it's for. The A–Z button is simply absent here, not present-and-ignored. |
| Recently Played | ❌ | same |
| Queue | ❌ | the order **is** the queue — sorting the display would decouple the rows from the positions the move/remove actions use |
| Album detail → tracks | ❌ | `(effective_disc, track)` is the album's own order |
| Search | ❌ | results are relevance-ranked by `fuzzy-matcher`; an alphabetical sort throws the ranking away |
| Browser | ❌ | a directory listing's order is the server's, the same reason its rows carry no track number |

The albums lists share `views::albums_list::view`, which Recently Added also
uses — so the toggle is a parameter on that view, not an unconditional part of
its header.

## Tests

- `name_cmp` puts `a-ha` before `ZZ Top` (the byte-order bug, asserted), is a
  total order, and breaks case-only ties deterministically.
- Reversing twice is the identity, on a list containing case-only duplicates —
  the property that fails if the comparator isn't total.
- `sort_library_lists` orders `AlbumGroup`s by `(artist, base)` and doesn't
  split one artist's albums apart.
- Recently Added's order survives a `ToggleSortDirection` — the guard against
  a future refactor quietly folding it into the shared re-sort.

## Revised after review: albums sort by album name, and by more than name

The first cut sorted albums artist-then-title, which reads as unsorted to
anyone scanning the column of titles. Albums now sort on the **title**, with
the artist as the tiebreak.

Two more keys landed with it — **Year** and **Added** — as a Name/Year/Added
picker beside the direction button, whose wording follows the key ("A–Z"/"Z–A"
for names, "Oldest"/"Newest" for dates). A missing year or add-time sorts
**last in both directions**: the unknown check sits outside the reversal,
because an undated album heading a "Newest" list would read as data rather
than as a gap.

The cost asymmetry between the two is the design:

- **Years** come from `list Date group AlbumArtist group Album` once on
  connect. `list` returns one line per distinct value, so a whole library is a
  few thousand lines.
- **Add-times** have no `list` equivalent — `Added` is not a tag — so they
  need a paged walk of every song, done only when the Added sort is selected
  and folded down to three fields per song without building a `Song`.

## What landed

| | |
|---|---|
| `src/mpd/types.rs` | `name_cmp` / `name_cmp_dir` / `SortKey` / `album_cmp` / `parse_year` / `album_year_index` / `fold_album_added` |
| `src/mpd/commands.rs` | `parse_grouped_values2` — two nested `group` levels, clearing the inner one when the outer changes |
| `src/mpd/client.rs` | `list_album_years`, `added_page` |
| `src/config/settings.rs` | `sort_desc: bool`, `#[serde(default)]` |
| `src/ui/app.rs` | `App::sort_library_lists()` + `load_album_added()`; `album_years` / `album_added` state, cleared on `SwitchServer` |
| `src/ui/widgets/link.rs` | `sort_toggle(desc)` for name-only lists, `album_sort_controls(key, desc)` for the album lists |
| six view headers | `artists_list`, `genres_list`, `playlists_list`, `albums_list` (as `Option<bool>`), `artist`, `genre_detail` |

`albums_list::view` takes `Option<(SortKey, bool)>` rather than a plain pair
precisely because Recently Added shares it: `None` there means the control is
**absent**, not present and doing nothing.
