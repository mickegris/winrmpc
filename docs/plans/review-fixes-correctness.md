# Plan: Correctness + panic fixes from the post-parity code review

Status: **items 1-5 implemented** (in `f423f18`), and the six findings from
a **second review round against that fix commit itself** are implemented in
the follow-up commit — see "Second review round" at the bottom of this file.
Item 6 (punctuation-folded grouping key)
remains **deliberately deferred** — it was already marked "follow-up sized,
not part of the urgent batch" in this plan's own text, and that call still
stands; it's a genuine parity gap, not a regression, and is sized more like
a small feature than a bug fix. The "also worth folding in" `push_recent`
de-dup note under item 1 was likewise left for later — it's a soft
"consider," not a required fix, and changes `push_recent`'s de-dup
semantics, which is a separate risk surface from the key-consistency bug
it was attached to.

Findings from reviewing the seven parity commits landed on `release/v0.4.1`
(`b1329b3`..`b820700`). Every item below was **verified against the actual
code**, not inferred from the plans; two were reproduced with a standalone
program. Ordered by severity.

These are all small, mostly-independent fixes, implemented together in one
commit since none of them touch overlapping code (verified by a clean build
+ full test pass after all five landed).

**Implementation notes / deviations**:
- Item 1's fix is named `art_key_for(artist, album)` exactly as sketched,
  added to `types.rs` with `Song::art_key()` now a thin wrapper over it.
  All four raw-key sites (`fetch_recent_art`, both `now_playing.rs` sites,
  `recently_played.rs`) plus the two already-correct-but-inconsistent ones
  (`artist.rs`, the `ArtistAlbumsLoaded` handler) now go through it.
- Item 2 turned out to need one more layer than the plan's snippet showed:
  `fetch_album_bio`'s `artist` parameter is used for **two different
  things** — the cache key (must be the *exact* `Option<String>`
  `View::AlbumDetail` carries, so render-time lookups agree) and the
  MusicBrainz query string (fine to substitute a fallback when the real
  artist is unknown). Conflating them — e.g. resolving the fallback once in
  `AlbumSelected` and reusing that resolved value for both — would let a
  stale `self.selected_artist` from browsing a *different* artist earlier
  in the session enable multi-disc sibling-variant merging for a
  Genre-detail-originated album selection, which is exactly the unsafe
  merge mikMPD's rule exists to prevent. So `fetch_album_bio` keeps
  `artist: Option<String>` for keying and derives a separate
  `query_artist: String` only for the MusicBrainz call.
  New `album_scoped_key(artist: Option<&str>, album: &str)` helper in
  `types.rs` backs both the `album_bios` and `album_songs` maps (the
  render arm's lookup key and `AlbumSongsLoaded`'s stored key were both
  updated to match).
  **Superseded in the follow-up round** (see "Second review round" below):
  splitting key from query was right, but keeping the `selected_artist`
  *fallback* in `query_artist` was not — it made the stored value
  nondeterministic under a deterministic key. The fallback is now gone
  entirely; `query_artist` is just `artist.unwrap_or_default()`.
- Item 4's fix is `rfind_ascii_ci`, not literally `char_indices()` +
  `eq_ignore_ascii_case` inlined at the call site as sketched — factored
  into its own function since it's reused by nothing else but reads much
  more clearly named. One of the plan's own example test cases
  (`"İİ cd2"`) turned out to be mislabeled during writing — the *correct*
  panic-free result is a successful strip (`"İİ"`, disc 2), not a
  passthrough, which the standalone verification in the plan didn't
  actually compute (it only proved the *old* code panics, not what the
  *new* code should return). Caught by writing a second, independent
  verification program before trusting the test assertion.

---

## 1. Recently-played art keys don't match `art_key()` after disc-stripping

**Severity: high (user-visible, and wastes rate-limited network calls).**
Introduced by `284cd17` (round 6).

### What broke

`Song::art_key()` (`types.rs:172-175`) now runs the album through
`album_base_and_disc(...).0`, so a multi-disc set shares one cache entry:

```rust
pub fn art_key(&self) -> String {
    let base = album_base_and_disc(self.display_album()).0;
    format!("{}\x1f{}", self.display_album_artist(), base)
}
```

But **all four "recent albums" key sites still build the key from the raw
album tag**, because `RecentAlbum.album` / `RecentlyPlayedEntry.album` are
stored raw (from `display_album()`, `app.rs:541` / `app.rs:508`):

| Site | Code | Key used |
|---|---|---|
| `app.rs:2473` (`fetch_recent_art`) | `format!("{}\x1f{}", r.artist, r.album)` | **raw** |
| `now_playing.rs:295` (`recent_thumb`) | `format!("{}\x1f{}", recent.artist, recent.album)` | **raw** |
| `now_playing.rs:193` (recents filter) | `format!("{}\x1f{}", r.artist, r.album) != current_key` | **raw** vs `song.art_key()` (**stripped**) |
| `recently_played.rs:130` (`album_tile`) | `format!("{}\x1f{}", group.artist, group.album)` | **raw** |

For an album tagged `"Blast from the Past [Disc 1]"`, art is cached under
`"Gamma Ray\x1fBlast from the Past"` but every recents path looks up
`"Gamma Ray\x1fBlast from the Past [Disc 1]"`.

### Three concrete symptoms

1. **Recents thumbnails miss a cache entry that already exists.**
   `fetch_recent_art` passes an **empty URI** (`app.rs:2477`,
   `self.fetch_art(String::new(), key)`), so the MPD tag/cover-file paths are
   skipped by design — the raw-key miss goes **straight to MusicBrainz**,
   which is behind the 1-req/s global throttle added in `8a74dd8`. So a
   disc-suffixed album that already has perfectly good embedded art in the
   cache instead burns a throttled MusicBrainz lookup, and shows a
   placeholder if MusicBrainz has no match.
2. **The "don't show the currently-playing album in its own recents strip"
   filter never fires** for disc-suffixed albums — it compares a raw-key
   string against `song.art_key()` (stripped), which can't match. The album
   you're listening to appears in its own Recently Played row.
3. **Duplicate cache entries** for one cover (one stripped from `art_key()`,
   one raw from the recents fetch), consuming the `art_cache_size_mb` budget
   twice and doubling LRU pressure.

### Fix

Route every art-cache key through one helper so this can't drift again:

```rust
// types.rs — next to art_key(), which becomes a thin caller of it.
pub fn art_key_for(artist: &str, album: &str) -> String {
    format!("{}\x1f{}", artist, album_base_and_disc(album).0)
}
```

Then replace all four raw `format!("{}\x1f{}", …)` sites above with
`art_key_for(...)`, and make `Song::art_key()` delegate to it. Unit-test that
`art_key_for("A", "X [Disc 1]") == art_key_for("A", "X")` and that it equals
`Song::art_key()` for an equivalent song — locking the two paths together.

> Note: `artist.rs:124` and `app.rs:914` already use `group.base` (stripped),
> so those are correct and become `art_key_for` calls only for consistency.

### Also worth folding in

`push_recent` de-dupes `RecentAlbum` on the exact `(artist, album)` pair, so
`"X [Disc 1]"` and `"X [Disc 2]"` occupy two of the eight recents slots for
what is one album. Consider de-duping on the stripped base while still
storing the raw tag for display (the same "key vs. display" split `art_key`
already uses).

---

## 2. `album_bios` in-memory guard is keyed by album name only

**Severity: high (shows the wrong artist's bio, and never self-corrects).**
The key shape pre-dates today, but `284cd17` is what made it *reachable*.

`fetch_album_bio` (`app.rs:2433-2439`):

```rust
if self.album_bios.contains_key(&album) {   // album NAME only
    return Task::none();
}
let key = format!("{artist}\x1f{album}");   // redb key IS artist-scoped
```

The redb `bios` table is correctly artist-scoped; the **in-memory** map and
its guard are not. Before round 6, two artists' same-named albums collapsed
into a single ambiguous row, so you couldn't easily view them separately —
now they're deliberately separate rows, so this is newly hit in normal use:

> Open "Greatest Hits" by Artist A → bio loads and is cached as
> `album_bios["Greatest Hits"]`. Open "Greatest Hits" by Artist B → the
> guard short-circuits → **B's page shows A's bio**, for the rest of the
> session.

`album_songs` (`app.rs:41`) has the same album-name-only key shape, but
`AlbumSelected` always fires its songs task unconditionally, so it
self-corrects after a load — the *transient* wrong track list is a much
milder symptom. `View::AlbumDetail` already carries the artist
(`message.rs:205`), so the data needed for a correct key is in hand.

### Fix

Key both maps on the same artist-scoped string the redb store already uses
(`format!("{artist}\x1f{album}")`). `View::AlbumDetail(name, artist)` has
both halves at every lookup site (`app.rs:2076`, and the bio lookup in the
same arm), so this is a mechanical change — the only care needed is the
`artist: None` case (genre detail), which should fall back to a distinct
sentinel (e.g. `"\x1f{album}"`) rather than colliding with a real artist's
entry.

Add a regression test for the pure key builder: two different artists, same
album title → different keys.

---

## 3. Snapcast client is never rebuilt when the active server changes

**Severity: high (talks to the wrong machine).** Introduced by `b820700`.

`on_view_enter` (`app.rs:2619`):

```rust
if self.snapcast_client.is_none() {
    self.snapcast_client = Some(SnapcastClient::new(&addr));
}
```

`SnapcastClient` captures its address at construction. The `is_none()` guard
means it is built **once, from whichever server was active on the first
Snapcast visit**, and never rebuilt. `Message::SwitchServer` (`app.rs:1588`)
rebuilds `MpdClient`, clears/reloads `recently_played`, and restores the
partition — but **does not touch `snapcast_client`** (verified: no `snapcast`
reference anywhere in that arm).

So: connect to server A, open Snapcast, switch to server B, open Snapcast →
you are still controlling **A's** Snapcast instance, with no indication.

### Fix

In `Message::SwitchServer`, drop the client and its cached view state:

```rust
self.snapcast_client = None;
self.snapcast_groups.clear();
self.snapcast_streams.clear();
self.snapcast_error = None;
```

The lazy `is_none()` construction in `on_view_enter` then rebuilds it against
the new server's `snapcast_addr()` on the next visit. Also worth guarding
against the *same* server's address changing (edited host/port in Settings) —
simplest is to store the addr alongside the client and rebuild when it
differs, rather than relying on `is_none()` alone.

---

## 4. `album_base_and_disc` panics on titles whose lowercase changes byte length

**Severity: high (panic in the render path), low probability.**
Introduced by `284cd17`.

`parse_bare_trailing_marker` (`types.rs:371-393`) searches a **lowercased
copy** but slices the **original**:

```rust
let lower = s.to_lowercase();
let Some(idx) = lower.rfind(word) else { continue };
let after = &s[idx + word.len()..];   // idx is an index into `lower`, not `s`
...
let before = &s[..idx];
```

`str::to_lowercase` is not length-preserving. `'İ'` (U+0130, 2 bytes)
lowercases to `"i̇"` (U+0069 U+0307, 3 bytes), so indices drift by one byte
per occurrence. Reproduced with a standalone program:

```
"İİ cd2"  (len 8)  lower len 10  idx 7  -> *** PANIC ***
"İİİ cd2" (len 10) lower len 13  idx 10 -> *** PANIC ***
"İ - cd2" (len 8)  lower len 9   idx 6  -> ok, but after="" (marker silently missed)
```

Two failure modes: a hard panic (`byte index … is out of bounds` / not a char
boundary), or a silent mis-parse. **This is reachable from `Song::art_key()`,
which is called inside `App::view()`** (`app.rs:2078` and elsewhere) — a panic
there tears down the UI on every render of the offending album.

Narrow trigger (mainly Turkish/Azerbaijani `İ`), but the fix is trivial and
the blast radius is the whole app.

### Fix

Don't cross-index between two strings. Either:

- **(Preferred)** search case-insensitively over the original: walk
  `s.char_indices()` and compare `s[i..].chars()` against the marker word
  with `eq_ignore_ascii_case` (the marker words are all ASCII, so ASCII-only
  case folding is correct and length-preserving here); or
- lowercase **only for comparison** on slices taken from `s` by char
  boundaries, never using an index derived from the lowercased copy.

`strip_edition_qualifier` (`musicbrainz.rs:354`) is **not** affected — it
takes its index from `trimmed.rfind(open)` on the original string and only
lowercases the already-sliced `inner` for a `contains` check.

Add tests: `album_base_and_disc("İİ cd2")` must not panic, and
`album_base_and_disc("Aİ cd12")` must still strip correctly.

---

## 5. `QueueAddNext` can underflow when the queue is emptied mid-operation

**Severity: low (narrow race, degrades quietly in release).**
Introduced by `b1329b3`.

`app.rs:628`:

```rust
if let Ok(q) = client.queue().await {
    let end = q.len() as u32 - 1;
```

Between `add_id` succeeding and `queue()` returning, another client (or the
user hitting Clear) can empty the queue, making `q.len() == 0`. Then
`0u32 - 1` panics in debug and wraps to `u32::MAX` in release (the release
profile in `Cargo.toml` doesn't enable `overflow-checks`), sending
`move 4294967295 …` — harmless, swallowed by `.ok()`, but garbage.

### Fix

```rust
let Some(end) = (q.len() as u32).checked_sub(1) else { return };
```

or skip the move entirely when `q.is_empty()`.

---

## 6. Album identity isn't punctuation-folded (parity gap, not a regression)

**Severity: medium, but it is a *missing* feature rather than something
today's work broke.** Noticed while re-reading mikMPD's `CLAUDE.md` during
this review.

mikMPD folds punctuation into its album grouping key
(`albumGroupingKey`: en/em dash and minus → hyphen, smart quotes → straight,
ellipsis → `...`, then trim + lowercase) and documents a concrete real-world
failure it fixes:

> "two rips of one album can differ by a single character: The Beatles'
> '1967-1970' (ASCII hyphen, disc 2) and '1967–1970' (en dash, disc 1) are
> separate directories and separate album tags, which split one 2-disc set
> into two single-disc rows, each with a wrong caption and half the tracks."

winrmpc's `group_albums_by_artist` (`types.rs`) keys on
`(artist.to_lowercase(), base)` with **no punctuation folding**, so it has
exactly the splitting bug mikMPD describes.

### Fix (follow-up sized, not part of the urgent batch)

Add a pure `album_grouping_key(artist, album)` that strips the disc marker
then folds punctuation + case, and use it as the `group_albums_by_artist`
map key **and** for the variant-matching lookup in `AlbumSelected`
(`app.rs`, the `g.base == album_name` comparison) — mikMPD's note is
emphatic that *every* consumer of the key must agree or "a row won't find
its own disc data".

Critically, **do not fold the art cache key**: mikMPD flags this explicitly
("`artCacheKey` is deliberately *not* folded: changing it would orphan the
whole album-art disk cache for a cosmetic gain"), and the same applies to
winrmpc's `art_key`. Grouping key and art key are allowed to differ; disc
stripping applies to both, punctuation folding only to grouping. Item #1
above should land first so there is a single art-key helper to keep
un-folded.

---

## Suggested order

| # | Item | Size | Why this order |
|---|------|------|---|
| 4 | `album_base_and_disc` index-drift panic | XS | Panic in the render path; smallest fix |
| 1 | Unify art-cache key via `art_key_for` | S | User-visible, wastes throttled MusicBrainz calls |
| 3 | Reset `snapcast_client` on `SwitchServer` | XS | One-line class of bug, wrong-machine control |
| 2 | Artist-scope `album_bios`/`album_songs` keys | S | Wrong bio persists for the session |
| 5 | `QueueAddNext` `checked_sub` | XS | Trivial, bundle with any of the above |
| 6 | Punctuation-folded grouping key | M | Genuine parity gap; separate follow-up |

## Testing

- **Unit** (I/O-free, this codebase's convention): `art_key_for` disc-collapsing
  and its agreement with `Song::art_key()`; artist-scoped bio/song key builder
  (two artists + same album title → distinct keys); `album_base_and_disc`
  non-ASCII no-panic + still-correct-stripping cases; `album_grouping_key`
  punctuation folding (en dash vs hyphen collapse to one group) if #6 is taken.
- **Manual QA**: play a `[Disc 1]`-suffixed album — its cover must appear in
  the Recently Played strip *and* it must not appear in its own strip; open
  two different artists' same-titled albums back to back and confirm each
  shows its own bio; connect to server A, open Snapcast, switch to server B,
  reopen Snapcast, confirm it reflects B.

---

## Second review round (review of the fix commit itself)

`f423f18` — the commit implementing items 1-5 above plus all four items of
[`review-fixes-performance.md`](review-fixes-performance.md) — was itself
code-reviewed. Six findings, **all fixed in the immediate follow-up
commit**; nothing from this round is deferred.

| # | Finding | Where |
|---|---------|-------|
| 1 | **Snapcast could no longer self-heal after a connection drop.** Gating `connect()` on `is_connected()` was correct in isolation, but `is_connected()` is a bare `Option::is_some()` and *nothing* ever cleared a dead connection — `request()` returned `Err` on EOF and left `conn` as `Some`, and `SnapcastUnreachable` only stored an error string. After any snapserver restart or LAN blip the 2s poll and every view re-entry failed forever; only `SwitchServer` or an app restart recovered. A regression: the wasteful unconditional `connect()` it replaced *did* heal. Fixed by dropping the connection in `SnapcastClient::request()` on `Connection`/`Io`/`Json` errors (but **not** on `Rpc` — a well-formed JSON-RPC error response proves the socket is fine). The performance plan's §3 offered two options and only the probe half was implemented; the probe alone is insufficient. | `snapcast/client.rs` |
| 2 | **Artist-less album bios persisted a wrong-artist result under a shared key.** With `artist: None` the redb key is `"\x1f{album}"` for every artist, while `query_artist` still fell back to the transient `self.selected_artist` — so a bio fetched for whatever artist page happened to be open got written permanently into a slot shared by every artist-less album of that title, and read back for unrelated ones. Nondeterministic content under a deterministic key. Fixed by removing the fallback outright (see the amended item-2 note above). | `ui/app.rs` |
| 3 | **`PlayAlbum` could silently start playback on a truncated album.** `command_list` aborts at the first bad URI and the call site did `.ok()`, having already cleared the queue. Fixed by logging a "queued a partial album" warning at both `PlayAlbum` and `QueueAlbum` instead of discarding the error. | `ui/app.rs` |
| 4 | **`add_all` omitted the `duration_ms` structured tracing field**, so the one command this whole change exists to make fast could never trip `LogEntry::is_slow()`'s ⚠ in the Log view. Fixed. | `mpd/client.rs` |
| 5 | **The `art_key_for` fix didn't actually land for Recently Played.** `RecentlyPlayedEntry.artist` is the *track* artist, but art is only ever cached under the *album* artist, so compilation tiles still missed. Fixed by recording `album_artist` alongside `artist` (`#[serde(default)]` for existing history) and adding `art_artist()`, which `recently_played_albums` now groups and labels by — which also collapses a compilation to one tile instead of one per guest artist. | `mpd/types.rs`, `ui/app.rs` |
| 6 | **The new `decode_snap_status` test was tautological** — it compared against `decode_snap_groups`/`decode_snap_streams`, which are now thin wrappers over the function under test, so it asserted a function equals itself. Replaced with literal expected values, plus a degrades-to-empty case. | `snapcast/types.rs` |

One finding from that review was **investigated and rejected**: the claim
that `decode_snap_groups`/`decode_snap_streams` are now dead outside tests
and would warn `never used`. A clean full rebuild emits zero warnings —
they stay reachable as `pub` items of a `pub mod`. The tautological-test
half of that finding was real and is #6 above.

### Testing added this round

- `art_artist_prefers_album_artist_and_falls_back_to_track_artist` —
  covers the `#[serde(default)]` back-compat path for history written
  before `album_artist` existed.
- `recently_played_albums_art_key_matches_song_art_key_on_a_compilation` —
  the actual regression guard: builds a `Song` with a differing track /
  album artist *and* a disc suffix, and asserts the key the tile looks up
  equals the key `Song::art_key()` caches under.
- `recently_played_albums_collapse_guest_artists_of_one_compilation`.
- `decode_snap_status_returns_both_halves_from_one_parse` and
  `decode_snap_status_degrades_to_empty_halves_on_unexpected_shape`.
- **Not unit-testable without a mock socket** (consistent with the existing
  `protocol.rs` gap): the Snapcast drop-on-transport-error path. Manual QA
  instead — open the Snapcast view, restart snapserver, confirm the view
  recovers on its own within a poll or two rather than needing a restart.

---

## Measured against a real library (10.0.1.3, MPD 0.24.0, 9846 songs)

The live integration tests in `src/live_tests.rs` were run against a real
server. All the fixes above hold up: `add_all` enqueues in order and its
stop-at-first-failure semantics are exactly as documented (a bad URI in the
middle leaves the earlier tracks queued and drops the rest), grouping
collapses real multi-disc sets, and every disc of a set resolves to one
shared art key.

**Grouping coverage on real data: 829 album rows → 789 groups, 35 collapsed
multi-disc sets.** 14 marker-looking names were left as singletons. Most of
those are *correct* refusals — `Killers (CDM 7520192)` is a catalogue
number, `Screaming For Vengeance [2001 CD Edition]` and
`Journeyman [2014 Audio Fidelity SACD AFZ 180]` are editions, not discs.

But three real sets (six rows) **should** have collapsed and didn't:

| Rows | Why `album_base_and_disc` misses |
|---|---|
| `101 [Disc A]` / `101 [Disc B]` | disc identified by a **letter**, not a digit — `is_short_digit_run` requires digits |
| `Clutching at Straws [24-bit Remaster CD 1]` / `[… CD 2]` | marker is **inside a bracket with other text**, so the bracketed-marker path doesn't match and the bare-trailing path sees `]` after the digits |
| `Misplaced Childhood [24-bit Remaster, CD 1]` / `[… CD 2]` | same |

Also not collapsed, each a lone row so nothing to merge with anyway:
`Lotus (Disc One)` (spelled-out number), `Decade of Aggression - Disc 1 of 2`
(trailing text after the number), `Volume 3 Disc3 (Rem.2007)` (marker not
trailing), `Nostradamus (2CD) (CD 1/2)` (`N/M` disc form).

This is a **pre-existing coverage gap, not a regression** — `album_base_and_disc`
was only ever specified for trailing digit markers, and each of these needs a
separate rule (letters as disc ids; markers embedded in a qualifier bracket;
spelled-out numbers; `1 of 2` / `1/2` forms). Deliberately **not** fixed in
this round: widening the matcher is exactly the kind of change that starts
stripping real album titles, and it wants its own plan with these six rows as
the test corpus. It belongs with the deferred item 6 (punctuation folding) as
a single "grouping-key robustness" follow-up.

---

## Album-matching round: closing the real-world gaps

The gaps measured above are now **fixed**, and re-measured against the same
library: **829 rows → 782 groups, 42 collapsed** (was 789 / 35).

| Form added | Example from the library | Base | Disc |
|---|---|---|---|
| Letter disc ids | `101 [Disc A]` / `[Disc B]` | `101` | 1 / 2 |
| Spelled-out numbers | `Lotus (Disc One)` | `Lotus` | 1 |
| Of-total | `Decade of Aggression - Disc 1 of 2` | `Decade of Aggression` | 1 |
| Slash-total | `Nostradamus (2CD) (CD 1/2)` | `Nostradamus` | 1 |
| Marker at the tail of a qualifier bracket | `Clutching at Straws [24-bit Remaster CD 1]` | `Clutching at Straws [24-bit Remaster]` | 1 |
| Disc-*count* bracket | `X (2CD)`, `X (3 CDs)` | `X` | — |

Two design calls worth keeping:

- **The qualifier bracket is preserved, not dropped.** `[24-bit Remaster CD 1]`
  becomes `[24-bit Remaster]`, not nothing. Both discs still collapse, but a
  remaster doesn't get merged into a differently-mastered copy of the same
  album that the library may hold separately — the same reasoning that keeps
  `strip_edition_qualifier` lookup-only.
- **The disc-count bracket is stripped unconditionally**, even with no disc
  marker following. It has to be: this library tags one album's two discs as
  `Nostradamus (2CD) (CD 1/2)` and `Nostradamus (disc 2)`, which only land on
  the same base if `(2CD)` always goes.

**Grouping keys now case-fold the base too** (`base.to_lowercase()`, first-seen
spelling still displayed). This was a separate real split: `Crime Of The
Century` / `Crime of the Century`, `Music For The Jilted Generation` /
`Music for the Jilted Generation`, and `Decade Of/of Aggression` were each two
rows of one album. This is a slice of deferred item 6 — the *case* half of the
folding. The punctuation half (en/em-dash, smart quotes) is still deferred; no
album in this library needed it.

### Guard rails against over-stripping

Widening the matcher is how you start eating real titles, so each addition is
gated:

- A spelled-out number or a letter id is only accepted when a separator sits
  between it and the marker word — otherwise `(CDs)` reads as "CD, disc S" and
  `(CDone)` as "CD, disc 1". Digits may still abut, since `CD2` is a real form.
- Disc numbers are capped at `MAX_DISC_NUMBER` (99). Without it,
  `[… SACD 180]` would strip as disc 180.
- The count-bracket rule requires digits *then* a marker word and nothing
  else, so `[24-bit Remaster]` — which also starts with digits — is untouched.

### How this was validated

Not by unit tests alone. Every one of the **88 names the matcher alters** and
all **42 collapsed groups** were dumped from the real library and read
individually to confirm each strip and each merge is genuinely the same
album. That review is what caught the case-folding split and the
`Nostradamus` mismatch, neither of which was in the original finding list.
The negative corpus (`Killers (CDM 7520192)`,
`Screaming For Vengeance [2001 CD Edition]`,
`Journeyman [2014 Audio Fidelity SACD AFZ 180]`) is now a unit test, since
those are exactly what a looser matcher would break.

### Still not handled, deliberately

`Volume 3 Disc3 (Rem.2007)` — marker followed by a *non*-marker bracket.
Handling it means stripping from the middle of the name, which is a
materially riskier rule, and it is a lone row: collapsing gains nothing.
