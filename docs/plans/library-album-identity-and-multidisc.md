# Plan: Album identity, multi-disc collapsing, grid view, richer search

Status: **Parts A+B implemented** (artist-aware grouping + multi-disc
collapsing — the plan's namesake). **Parts C (grid view) and D (search
sections/batch-select) deliberately deferred**, not implemented — Part D's
own text already flagged it as "bundle in only if the above ships smoothly,"
and Part C's prerequisite note calls out real scope; landing all four parts
in one pass risked a much larger, harder-to-review diff for a plan already
flagged as "largest item in the set." Left as clean follow-up work; nothing
about Parts A/B blocks them. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gaps #2/#3).

Mirrors three mikMPD plans folded into one winrmpc-shaped piece of work,
since in this codebase they touch the same view (`albums_list.rs`) and the
same MPD grouping query: `../mikMPD/plans/album-artist-identity.md`,
`../mikMPD/plans/multi-disc-albums.md`, and the Search portion of
`../mikMPD/README.md`.

**Implementation notes / deviations (Parts A+B)**:
- `View::AlbumDetail`/`Message::AlbumSelected` became `(String, Option<String>)`
  tuples rather than a named struct variant — smaller diff across the ~10
  call sites, same clarity at the point of use.
- Fixed a real bug found while rewriting `artist.rs`: its per-row art-cache
  key used a bare `"-"` separator (`format!("{artist}-{album}")`) instead of
  the `\x1f` unit separator every other art key in the codebase uses —
  meaning that view's art thumbnails could never actually hit the cache
  `ArtistAlbumsLoaded` populates. Fixed as part of this rewrite.
- Discovered and fixed a regression risk before it shipped: `PlayAlbum`/
  `QueueAlbum` used to re-query `find_add("Album", &name)` by tag name. Once
  `AlbumDetail`'s identity became the *collapsed base name*, that would
  silently match zero tracks on any multi-disc album (no track's literal
  `Album` tag equals the base name). Changed both messages to carry the
  already-loaded, variant-expanded, disc-sorted song URIs instead of
  re-querying — see CLAUDE.md's "Play All / Queue All" section.
- List-view "N discs" captions use `variants.len()` alone, not the full
  `album_disc_count` two-signal max (that needs each song's `disc` tag,
  which isn't fetched until the detail page loads) — acceptable since it's
  a hint, refined once the album is opened.
- `Recently Added` and `Genre Detail` also touched: Recently Added now
  groups through the same `AlbumGroup`/`group_albums_by_artist` pipeline
  (gets multi-disc collapsing and correct artists for free, derived from
  the song data `find_recently_added` already returns). Genre Detail was
  **not** regrouped — kept as a plain `Vec<String>` list in a new
  `genre_albums` map (split out of the old dual-purpose `artist_albums`
  map, which used a `"genre:"`-prefixed key hack) — out of scope, and genre
  listings don't have the artist-ambiguity problem this plan is centrally
  about.
- Known gap carried forward (see CLAUDE.md): `album_songs`/`album_bios`
  caches are still keyed by base name alone, not artist-scoped.

## Today (`src/ui/views/albums_list.rs`, `src/mpd/client.rs`)

`Albums` is `list_tag("Album")` (`client.rs:241`) — a flat, **name-only**
list of unique album tags, rendered as plain text rows
(`albums_list.rs:6-31`). This means:

- Two different artists' albums sharing a title ("Greatest Hits") collapse
  into one ambiguous row; selecting it does `find("Album", name)` with no
  artist filter, mixing tracks from unrelated artists.
- A multi-disc album tagged `"Blast from the Past [Disc 1]"` /
  `"...[Disc 2]"` is two separate rows, each playing only one disc.
- No cover art in the list, no grid option — text only.
- `views/search.rs` is a single flat `Vec<Song>` result list (`app.rs:49`,
  `Message::SearchResults(Vec<Song>)`) — no artist/album sections, no
  multi-select.

## Part A — Artist-aware album grouping

Port `../mikMPD/plans/album-artist-identity.md`'s approach:

1. **New client method** (`src/mpd/client.rs`): `list_albums_by_artist()`
   sending `list album group albumartist` (MPD 0.21+, same floor as
   `album_art`/`readpicture` which this codebase already requires). Falls
   back to plain `list_tag("Album")` on ACK (old server) — same graceful
   degradation mikMPD uses.
2. **New parser** (`src/mpd/commands.rs`): `list … group …` output has no
   record-starter key MPD-side, so it needs a dedicated line parser distinct
   from `pairs_to_map`/`split_groups` — a pure function
   `parse_grouped_values(lines: &[(String, String)], group_key: &str, value_key: &str) -> Vec<(String, String)>`
   that tracks the current group as it walks the pairs, defaulting to `""`
   for values seen before any group line. **Unit-testable** — add it to the
   `mpd/commands.rs` test module alongside the existing parser tests
   (CLAUDE.md's `Commands` bullet already lists this file as fully covered;
   keep it that way).
3. **Grouping type**: `AlbumGroup { artist: String, base: String, variants: Vec<String> }`
   in `src/mpd/types.rs`, built by a pure `group_albums_by_artist(pairs: &[(artist, album)]) -> Vec<AlbumGroup>`
   keyed on `(artist.to_lowercase(), base)` — same shape as mikMPD's
   `groupAlbumVariants`, unit-testable without a server.
4. **`AlbumDetail`** needs an artist parameter now (`View::AlbumDetail(String)`
   → `View::AlbumDetail { name: String, artist: Option<String> }`, or keep the
   `String` and pack `"artist\x1falbum"` the way `art_key` already does —
   prefer the struct variant for clarity). Entry points that already know the
   artist (Artist detail, Search, song links) pass it through; the plain
   Albums list passes the grouped artist. `find`/sibling-variant queries in
   the album-detail loader become artist-filtered when an artist is known,
   matching mikMPD's "skip sibling merging when artist is unknown" safety
   rule.

## Part B — Multi-disc collapsing

Port `../mikMPD/plans/multi-disc-albums.md`:

1. **Pure helper** (`src/mpd/types.rs`): `album_base_and_disc(album: &str) -> (String, Option<u32>)`
   — strips a trailing `[Disc N]` / `(Disc N)` / `Disk N` / bare `CDN`
   marker (case-insensitive, digit required, must be preceded by a
   delimiter or bracket so "ABCD2" survives). Direct port of mikMPD's regex
   approach; **unit-test the same edge cases** mikMPD's plan enumerates:
   `[Disc 1]`, `(Disc 2)`, `[Disk 3]`, `(CD 1)`, trailing bare `CD2`, `- Disc 1`,
   `: disc 12`, no-marker passthrough, "Disc 1" alone → passthrough, "Live CD"
   → passthrough (no false match), base-trimming.
2. **Shared art-cache key**: `art_key()` (`types.rs:168-170`) should run the
   album through `album_base_and_disc(...).0` before building the key, so all
   discs of a set share one cache entry — mirrors mikMPD's fix and this
   codebase's existing 0x1f-separator design (already documented in
   `CLAUDE.md` under "Art cache key format"). Add a test alongside the
   existing `art_key_*` tests in `types.rs` asserting disc-1/disc-2 variants
   produce the same key.
3. **MusicBrainz queries** (`src/art/musicbrainz.rs`): strip the disc suffix
   before building release-search queries, same as the Wikipedia album-bio
   lookup already has to (both currently get raw tags with the suffix
   attached, which is why disc-tagged rips today likely get worse Wikipedia
   match rates than single-disc albums — port mikMPD's fix here too).
4. **Disc-tag parsing**: `Song.disc` already exists (`types.rs:76`) but
   isn't consulted for sort order anywhere. Add an `effective_disc()` helper
   (tag value if present, else the suffix-derived disc) and sort album-detail
   track lists by `(effective_disc, track)` instead of `track` alone — fixes
   the interleaving bug mikMPD documents for convention-(b) albums (one
   album tag, per-song `disc: 1/2`, no name suffix).
5. **Grouping in list views**: `group_albums_by_artist` (Part A) collapses
   name-suffixed variants into one `AlbumGroup` row with `variants.len() > 1`
   driving a "N discs" caption. `AlbumDetail`'s loader expands variants back
   out (fetch each variant tag's songs, concatenate, sort by
   `effective_disc`) the same way mikMPD's `loadSongs` does.
6. **Disc count needs two signals** (mikMPD's harder-won lesson, worth
   inheriting directly rather than rediscovering): name-suffix variants
   *and* the `disc` tag both under-report on their own — a properly tagged
   multi-disc album with one album name and `disc: 1..4` has zero name
   variants; a poorly-tagged one has variants but no `disc` tag. Take the
   **max** of "variant count" and "highest `disc` tag value seen", the way
   mikMPD's `albumDiscCount(variants:tagDiscs:)` does, rather than trusting
   either signal alone.

## Part C — Grid view

> **Prerequisite:** land
> [`art-wikipedia-fetch-order-and-caching.md`](art-wikipedia-fetch-order-and-caching.md)
> items 1-3 (fetch-order fix, shared MusicBrainz client, `ArtFetchGate` +
> `MusicBrainzThrottle`) **before** this part. A grid view queues one art
> fetch per visible tile — today's per-call-chain `sleep(1100ms)` in
> `musicbrainz.rs` has no cross-task concurrency cap, so a grid of N tiles
> would fire N near-simultaneous MusicBrainz/HTTP requests instead of
> respecting a global ~1 req/s courtesy limit and a bounded number of
> concurrent connections. Shipping the grid without that gate first would be
> a performance regression, not just a missed optimization.

Add a list/grid toggle to `Albums` and `Artists`, since album art is already
fetched and cached (`art/cache.rs`) — the grid just needs a different layout
for data the app already has:

- **State**: `library_view_mode: LibraryViewMode` (`List | Grid`) on `App`,
  persisted or not (mikMPD doesn't persist it either — reasonable to keep
  ephemeral, defaulting to `List` to match current behavior).
- **Message**: `ToggleLibraryViewMode`.
- **View**: a small toggle button in the Albums/Artists header (same header
  row that already has the back button and title); Grid mode renders a
  wrapping row of fixed-size tiles (art via the existing `art_image.rs`
  helper + `art_handles` cache lookup, title/artist caption beneath) instead
  of the current text rows. iced 0.13's `wrap` widget or a manually chunked
  `column![row![...]]` grid (chunk `AlbumGroup`s into rows of N based on a
  fixed tile width) — no new dependency needed.

## Part D — Search sections + batch select

Smaller, bundle in only if the above ships smoothly:

- Extend `Message::SearchResults` (or add parallel messages) to carry
  separate artist/album/song result sets — three `list_tag_filtered`/`find`
  queries fired concurrently via `Task::batch`, same pattern `on_view_enter`
  already uses for other multi-fetch views.
- `views/search.rs` renders three sections (Artists → navigate, Albums →
  navigate, Songs → row actions), matching mikMPD's layout.
- Batch selection (checkbox per song row + a "Queue selected" /
  "Add selected to playlist" action) — lower priority than the sections
  themselves; can follow as a separate small increment.

## Implementation order

| # | Item | Size | Depends on |
|---|------|------|---|
| 1 | `parse_grouped_values` + tests | S | — |
| 2 | `list_albums_by_artist` client method + `AlbumGroup` + `group_albums_by_artist` + tests | S | 1 |
| 3 | `album_base_and_disc` + tests | S | — |
| 4 | `art_key` disc-folding + MusicBrainz/Wikipedia suffix stripping | S | 3 |
| 5 | `View::AlbumDetail` artist param + artist-filtered queries | M | 2 |
| 6 | Disc-aware sort + "N discs" caption + variant expansion in detail loader | M | 2, 3 |
| 7 | Albums/Artists list rewrite to grouped rows | M | 2, 6 |
| 8 | Grid view toggle | M | 7 |
| 9 | Search sections + batch select | M | independent |

## Testing

- **Unit** (I/O-free, inline `#[cfg(test)]`, matching this codebase's
  existing convention): `parse_grouped_values` (multi-group input, values
  before first group, empty input), `group_albums_by_artist` (same-name
  different-artist stays separate, disc-suffix variants merge, ordering),
  `album_base_and_disc` (full case list above), `art_key` disc-collapsing,
  `effective_disc` fallback logic, disc-count max-of-two-signals helper.
- **Manual QA**: two artists sharing an album title show two rows and two
  correct track sets; a `[Disc 1]`/`[Disc 2]` pair shows one row, "2 discs",
  and Play pulls in both in correct order; a `disc: 1/2` tagged album (no
  name suffix) sorts tracks correctly instead of interleaving; grid/list
  toggle round-trips without losing scroll position awkwardly; search shows
  three sections.
