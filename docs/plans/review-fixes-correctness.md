# Plan: Correctness + panic fixes from the post-parity code review

Status: proposed — no code changes yet. Findings from reviewing the seven
parity commits landed on `release/v0.4.1` (`b1329b3`..`b820700`). Every item
below was **verified against the actual code**, not inferred from the plans;
two were reproduced with a standalone program. Ordered by severity.

These are all small, mostly-independent fixes. They should ship as one
"review fixes" round before any further feature work, since #1-#3 are
user-visible wrong behavior and #4 is a reachable panic.

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
