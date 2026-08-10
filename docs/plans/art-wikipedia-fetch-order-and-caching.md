# Plan: Album art fetch order, Wikipedia match quality, and network-call performance

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md)). Unlike the other
plans in this set, this one is not about a missing *feature* — both apps
already have tag art, cover-file art, and internet fallback, and both already
fetch Wikipedia bios. It's about **fetch order and network efficiency**,
which is where winrmpc currently does worse than mikMPD despite having the
same three art sources. This directly serves the "performance, not battery"
priority for this app: **no item below is about saving energy/CPU wakes**
(that's mikMPD's `energy-optimization.md`, deliberately excluded — see the
overview doc) — every item here reduces network round trips, wasted
allocations, or wrong/missing results.

## Cache audit: what's in redb today, what isn't, and what should be

`src/store/mod.rs` currently has four tables: `art`, `art_meta`, `lyrics`,
`meta` (`store/mod.rs:20-26`). Everything expensive or remote that the app
fetches, checked against those four tables:

| Data | Source | Cached in redb today? | Action |
|---|---|---|---|
| Album/artist art bytes | MPD tag/cover-file, MusicBrainz+CAA | ✅ `art` table, incl. negative caching | none |
| Synced/plain lyrics | LRCLIB | ✅ `lyrics` table, incl. negative caching | none |
| Server statistics | MPD `stats` | N/A — must stay live, this is current server state, not a fetch result | none (do **not** cache) |
| Recently-added query results | MPD `find modified-since` | N/A — must stay live for the same reason | none |
| CD track probe | MPD `cdda://` | N/A — physical disc contents change per insert | none |
| **Wikipedia bios (artist + album)** | MusicBrainz + Wikipedia | ❌ in-memory only (`self.artist_bios`/`self.album_bios`, lost on restart) | **§4 below — add `bios` table** |
| **MusicBrainz IDs (artist MBID, release-group MBID)** | MusicBrainz search | ❌ not cached at all — re-searched on every art fetch *and* every bio fetch for the same entity | **§6 below (new) — add `mb_ids` table** |

Two gaps, both closed by this plan. Everything else the app fetches from a
remote or expensive source already goes through redb — this audit is the
"make sure redb is used fully" check, and after §4+§6 land, it is.

Recently-played listening history (planned in
[`recently-added-and-played-history.md`](recently-added-and-played-history.md))
is deliberately **not** in this table — it isn't a cache of anything
re-fetchable, it's a record the app itself generates. See that plan's
Persistence section (updated alongside this plan) for why it's moving from
the originally-proposed `AppConfig`/TOML storage to redb anyway: not because
it's a "cache," but because redb is the better storage engine for
frequently-mutated, growing data in this codebase, for the same
write-amplification reason detailed in §6 below.

## 1. Album art fetch order is backwards

### Today (`src/mpd/client.rs:429-503`)

`MpdClient::album_art(uri)` — the **only** entry point `app.rs::fetch_art`
calls (`app.rs:1880`) — sends `albumart` (a **cover file** beside the song,
e.g. `cover.jpg`) first, and only on an MPD "no such thing" error (code 50)
falls back internally to `read_picture(uri)` (art **embedded in the song's
own tags**, `readpicture`). Internet (MusicBrainz/Cover Art Archive) is tried
third, in `app.rs`, only if `album_art()` returns `None`.

So the actual order today is: **cover file → tag → internet.**

### What it should be (mikMPD, and what was asked for)

**Tag → cover file → internet.** mikMPD's own `CLAUDE.md` documents exactly
why, and the reasoning applies identically here — it's a fact about typical
libraries, not about iOS: *"Tag art is probed first because on a tagged
library it is the one that exists — asking `albumart` first cost a wasted
ACK round trip per album."* Most well-tagged libraries (FLAC/MP3 with
embedded covers) have tag art on every file; a `cover.jpg`-beside-the-file
convention is the exception. Trying cover-file first means **every single
album** in a well-tagged library pays for a failed `albumart` call before
falling through to the tag art that was there all along — a full extra
MPD round trip per album, at both first-fetch and any time the disk-negative-
cache is invalidated.

### Fix

Split the current internal fallback in `client.rs` into two independent,
sequenceable calls instead of one method with a baked-in order:

```rust
/// Art embedded in the song file's own tags (readpicture).
pub async fn tag_art(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> { ... }

/// A separate cover-file image beside the song (albumart).
pub async fn cover_file_art(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> { ... }
```

Extract the shared chunked-binary-read loop (currently duplicated near-
identically between `album_art` and `read_picture`, `client.rs:429-467` and
`469-503`) into one private helper `fetch_binary_art(cmd: &str) -> MpdResult<Option<Vec<u8>>>`
that both thin wrappers call with `"readpicture \"{uri}\" {offset}"` or
`"albumart \"{uri}\" {offset}"` respectively — removes the duplication as a
side effect of the reorder.

`app.rs::fetch_art` (`app.rs:1879-1884`) sequences them explicitly:

```rust
if !uri.is_empty() {
    if let Ok(Some(data)) = client.tag_art(&uri).await {
        let _ = cache.store(&key, &data).await;
        return (key, Some(data));
    }
    if let Ok(Some(data)) = client.cover_file_art(&uri).await {
        let _ = cache.store(&key, &data).await;
        return (key, Some(data));
    }
}
```

Then the existing MusicBrainz/CAA fallback (`app.rs:1892-1899`) stays third,
unchanged — tag → cover file → internet, matching the request exactly.

Keep the old `album_art()` name only if something else outside this fetch
path still calls it (grep before removing); otherwise delete it in favor of
the two explicit methods.

## 2. Reuse the MusicBrainz HTTP client instead of constructing a new one per call

### Today

`App` already holds `mb_client: crate::art::MusicBrainzClient` (`app.rs:71`,
constructed once at `app.rs:172`) — but **every call site ignores it** and
constructs a fresh `MusicBrainzClient::new()` inline instead
(`app.rs:640, 678, 735, 1866, 1932`). `self.mb_client` is dead weight: set
once, read never. Each `MusicBrainzClient::new()` builds its own `reqwest::Client`
(`musicbrainz.rs:76-81`), which owns its own connection pool — so concurrent
artist-bio, album-bio, and art fetches each open fresh TCP+TLS connections to
`musicbrainz.org`/`coverartarchive.org`/`en.wikipedia.org` instead of sharing
keep-alive connections across requests, even within the same browsing
session.

### Fix

Delete the unused-in-practice pattern: every `crate::art::MusicBrainzClient::new()`
call site in `app.rs` becomes `self.mb_client.clone()` (make `MusicBrainzClient`
cheaply `Clone` — wrap its inner `reqwest::Client` in the struct as-is,
since `reqwest::Client` is already internally `Arc`-based and cheap to
clone: `#[derive(Clone)]` on `MusicBrainzClient` costs nothing extra). One
shared client across the app's lifetime means connection pooling actually
works.

## 3. No shared concurrency/rate limit across fetch tasks — this is already an active bug, not just a future risk

### Today

Each MusicBrainz-touching method sleeps `1100ms` **locally**, inside that
one call chain (`musicbrainz.rs:115,151,176,196,248`, "Respect rate limit").
This bounds the rate of *sequential* requests within one `fetch_album_art`/
`fetch_artist_bio` call, but does **not** bound how many such call chains run
**concurrently**.

**This already happens today, not just hypothetically with a future grid
view.** `Message::ArtistAlbumsLoaded` (`app.rs:710-748`) fires **one
independent `Task::perform` per album** in the artist's list — for an artist
with, say, 20 albums whose art isn't already cached, that's 20 concurrent
closures each doing its own `find` + `album_art` + (on miss) its own ad-hoc
`MusicBrainzClient::new().fetch_album_art(...)`, each with its own local
1100ms sleep, all firing at once. Separately, `ArtistSelected` itself
(`app.rs:651`) already runs `Task::batch([albums_task, artist_art_task,
bio_task])`, so an artist-art fetch and a bio fetch race in parallel too.
MusicBrainz's usage policy (already the reason for the 1100ms constant) is a
*global* ~1 req/s courtesy limit, not "1 req/s per in-flight task" — opening
almost any artist with more than a couple of uncached albums can already
burst well past that limit today. The planned Albums **grid view**
(`library-album-identity-and-multidisc.md`, Part C) would make the *scale*
of the same existing bug worse (potentially 50+ concurrent tasks instead of
a handful), but it does not introduce the bug — fixing this is corrective,
not preventative.

mikMPD hit this exact scaling problem building its own grid view and fixed
it on two axes (`CLAUDE.md`, "Art fetching is throttled on three axes"):
`ArtFetchGate` caps concurrent fetches at 4; `MusicBrainzThrottle` serializes
MusicBrainz calls to ~1 req/s **globally**, across every in-flight fetch.

### Fix

Add both gates, shared via `Arc` on `App` (same "cheap `Arc`/clone-cheap
handle" shape this codebase already uses for `MpdClient`/`ArtCache`):

```rust
// New: src/art/throttle.rs (or fold into musicbrainz.rs)
pub struct MusicBrainzThrottle {
    last_request: Arc<tokio::sync::Mutex<Instant>>,
}
impl MusicBrainzThrottle {
    /// Blocks until at least 1100ms have passed since the *last* call from
    /// ANY task holding this handle, not just this call chain.
    pub async fn wait(&self) { ... }
}

pub struct ArtFetchGate {
    semaphore: Arc<tokio::sync::Semaphore>, // permits = 4, matches mikMPD
}
```

`MusicBrainzClient` takes a `MusicBrainzThrottle` handle (shared, cloned from
`App`) instead of doing an unconditional local `sleep(1100ms)` — the wait
becomes "wait until the shared clock says it's my turn" rather than "always
wait 1100ms regardless of what else is happening" (also a minor latency win
when nothing else is in flight: no throttle contention means no wait at
all, vs. today's unconditional sleep on every single call).

`fetch_art`/`fetch_artist_art`/bio fetches acquire an `ArtFetchGate` permit
before doing any work, released on completion (`tokio::sync::Semaphore`'s
RAII guard handles this automatically) — bounds peak concurrent HTTP
connections and MPD binary-protocol reads regardless of how many tiles a
future grid view queues at once.

## 4. Wikipedia bios are never persisted — refetched every session

### Today

`self.artist_bios: HashMap<String, Option<String>>` /
`self.album_bios: HashMap<String, Option<String>>` (referenced at
`app.rs:634, 673` and their `contains_key` guards) are **in-memory only** —
`src/store/mod.rs`'s redb tables are `art`, `art_meta`, `lyrics`, `meta`
(`store/mod.rs:20-26`); there is no bio table. Every app restart re-fetches
every artist/album bio from MusicBrainz + Wikipedia from scratch, each
subject to the 1100ms-per-call MusicBrainz throttle — this is the single
biggest avoidable network cost in the app on a cold start into a
frequently-browsed library, and it's strictly worse than what winrmpc
already does for art and lyrics (both persisted).

mikMPD persists Wikipedia results to disk (`Caches/` per its `CLAUDE.md`)
specifically to avoid this.

### Fix

Add a `bios` table to `src/store/mod.rs`, following the **exact existing
pattern** the `lyrics` table already established (documented in this
project's own `CLAUDE.md` under "Negative caching" — `Some(None)` means
"fetched, confirmed none exists"):

```rust
const BIOS: TableDefinition<&str, &[u8]> = TableDefinition::new("bios");
// value: serde_json::to_vec(&Option<String>)
pub fn bio_get(&self, key: &str) -> Option<Option<String>>;
pub fn bio_put(&self, key: &str, value: &Option<String>);
```

Key format: reuse the existing `art_key()`-style convention — artist bios
key on `"artist:{name}"` (already the convention `fetch_artist_art` uses for
the art cache, `app.rs` fetch_artist_art), album bios key on the existing
0x1f-separated `"{artist}\x1f{album}"` used everywhere else.

`ArtistSelected`/`AlbumSelected`'s bio-fetch guards
(`app.rs:634 if !self.artist_bios.contains_key(...)`) become: check the
in-memory map first (session cache, unchanged), then check the redb store
(via `spawn_blocking`, same as every other `Store` call per this project's
own concurrency rule), and only hit the network if **both** miss — then
write the result back to the store. Mirrors the lyrics fetch path in
`app.rs::fetch_lyrics` almost exactly; that function is a good template to
copy from directly.

## 5. Wikipedia match quality — close the gap with mikMPD's disambiguation

### Today (`src/art/musicbrainz.rs:280-362`)

`is_music_article` accepts a hit if the **extract text** contains a generic
music keyword or the artist/album name as a substring
(`musicbrainz.rs:280-293`). This is weaker than mikMPD's approach in two
ways worth closing to hit "similar or better than mikMPD":

1. **No general search fallback.** `fetch_album_bio` only tries three
   *exact*-title candidates (`musicbrainz.rs:348-352`: `"{album} (album)"`,
   `"{album} ({artist} album)"`, plain `{album}`) via the direct
   `page/summary/{title}` endpoint. If the real article title doesn't match
   any of the three exactly, the lookup gives up — even though Wikipedia's
   search API would likely find it. mikMPD's version additionally
   "search[es] over the top 3 hits" as a further fallback layer.
2. **No title-vs-extract preference.** `is_music_article` only inspects the
   extract; it doesn't check whether the **article's title** actually names
   the album/artist. mikMPD's `titleMatchesAlbum` (exact match or ≥2/3 token
   overlap) is checked **first** and wins immediately when it matches; only
   when the title doesn't match does mikMPD fall back to extract-content
   matching. This matters for the exact failure mode mikMPD's `CLAUDE.md`
   documents: "a sequel's article cites the album by name in its extract" —
   without title-preference, winrmpc's `is_music_article` (a pure substring
   check on the extract) is susceptible to exactly this kind of mismatch,
   returning a plausible-looking but wrong bio instead of the right one or
   a clean "no bio found".

### Fix

1. **Search fallback**: add `search_wikipedia(query: &str) -> Vec<(title, extract_snippet)>`
   using Wikipedia's `action=query&list=search` API (or the REST search
   endpoint), tried after the existing exact-title candidates are exhausted.
2. **Title-match helper** (pure, unit-testable, mirrors mikMPD's
   `titleTokensMatch`): `title_matches(article_title: &str, target: &str) -> bool`
   — stopword-dropped, whole-word token overlap ≥ 2/3, minimum two tokens.
   Apply it **before** falling back to the current extract-substring check:
   a hit whose title matches wins immediately; otherwise fall back to
   today's `is_music_article` extract check as the weaker signal.
3. Keep `is_music_article`'s current behavior as the final fallback layer —
   don't remove it, it's still a reasonable filter for the case where
   neither the title check nor a strong extract match is available.

### Related: edition-qualifier stripping for album lookups

mikMPD strips bracketed edition qualifiers ("[24-bit remaster]",
"[Deluxe Edition]") from the album tag *for lookup purposes only* before
building Wikipedia candidate titles (`albumLookupTitle`, distinct from the
disc-marker stripping already planned in
[`library-album-identity-and-multidisc.md`](library-album-identity-and-multidisc.md)
Part B — that plan handles `[Disc N]`, this handles edition/remaster
qualifiers; both strip suffixes from the *same* album tag but for different
reasons and shouldn't be conflated into one regex). Add a small
`strip_edition_qualifier(album: &str) -> String` used only when building
Wikipedia/MusicBrainz query candidates, never for the art cache key
(same "lookup-only, never grouping/art keys" rule mikMPD documents and this
codebase's own `art_key` design already follows for the 0x1f separator).

## 6. MusicBrainz IDs are resolved repeatedly, never cached

### Today

Both the art path and the bio path independently resolve the **same**
MusicBrainz entity ID for the same artist/album, and neither remembers the
answer:

- **Artist**: `fetch_artist_art` calls `search_artist(artist)` →
  `artist_id` (`musicbrainz.rs:112`). `fetch_artist_bio` calls
  `search_artist(artist)` again (`musicbrainz.rs:300`) — a second, fully
  independent search for the identical MBID. `ArtistSelected`
  (`app.rs:633-651`) fires both in the same `Task::batch`, so this isn't a
  rare double-lookup — it happens on **every single artist page visit**
  whose bio and art aren't both already in memory.
- **Album/release-group**: `fetch_album_art` calls `search_release_group`
  (`musicbrainz.rs:91`); `fetch_album_bio` calls `search_release_group`
  again (`musicbrainz.rs:335`) for the same artist+album. Additionally, the
  per-album loop in `ArtistAlbumsLoaded` (`app.rs:722-737`) does its own
  `search_release_group`-driven `fetch_album_art` call for **every album in
  an artist's list**, each of which gets searched *again* later if/when the
  user opens that album's detail page and `AlbumSelected`'s bio fetch runs.

Each of these is a full MusicBrainz search API round trip (subject to the
~1 req/s throttle from §3) purely to re-derive an ID the app already
resolved minutes or seconds earlier — and, since nothing persists it, an ID
the app will resolve identically again on the next visit or the next
session.

### Fix

Add a small `mb_ids` redb table, same shape and same negative-caching
convention as the planned `bios` table (§4) and the existing `lyrics` table:

```rust
const MB_IDS: TableDefinition<&str, &[u8]> = TableDefinition::new("mb_ids");
// key: "artist:{name}" -> artist MBID, or "{artist}\x1f{album}" -> release-group MBID
// value: serde_json::to_vec(&Option<String>) — None = "searched, confirmed no match"
pub fn mb_id_get(&self, key: &str) -> Option<Option<String>>;
pub fn mb_id_put(&self, key: &str, value: &Option<String>);
```

`search_artist`/`search_release_group` become cache-checking wrappers: look
up the store first (via `spawn_blocking`, same rule as every other `Store`
call), only hit the network on a miss, then persist the result — including
the negative case, so a confirmed "no MusicBrainz match" for an obscure or
mistagged artist/album doesn't get re-searched on every visit either (same
reasoning as `art_put_empty`/the planned `bio_put(key, &None)`).

This is a strict win layered on top of §1-§4: it removes the *art-vs-bio*
duplicate lookup immediately, and once §6 lands, the `ArtistAlbumsLoaded`
per-album loop's MusicBrainz calls (still gated by §3's throttle/semaphore)
become one-time-ever costs per album instead of a recurring cost on every
uncached-art page load.

## Implementation order

| # | Item | Size | Why this order |
|---|------|------|---|
| 1 | Split `tag_art`/`cover_file_art`, reorder in `fetch_art` | S | Immediate correctness fix, zero new dependencies |
| 2 | `self.mb_client` reuse (`#[derive(Clone)]`, delete inline `::new()` calls) | XS | Trivial, unblocks nothing else but should ship early |
| 3 | `MusicBrainzThrottle` (global) + `ArtFetchGate` (semaphore) | S | Fixes an active bug (§3) present today in `ArtistAlbumsLoaded` — not just prep for the future grid view |
| 4 | `bios` redb table + store/load wiring | S | Independent of 1-3 |
| 5 | Wikipedia title-match helper + search fallback | M | Independent, can slot in anytime |
| 6 | `strip_edition_qualifier` | XS | Small, pairs naturally with #5 |
| 7 | `mb_ids` redb table + cache-checking `search_artist`/`search_release_group` | S | Do after #4 (same table-adding pattern, easy to review together); eliminates the art-vs-bio duplicate lookup this section documents |

Items 1-3 are pure performance/correctness fixes to *existing* behavior and
should land **before** or **alongside** the grid view work in
`library-album-identity-and-multidisc.md`, not after — a grid view without
the concurrency gate would ship a regression, not just miss an optimization.

## Testing

- **Unit** (inline `#[cfg(test)]`, I/O-free, matching this codebase's
  existing convention):
  - `tag_art`/`cover_file_art` command-string formation (mirrors the
    existing pattern for other `client.rs` methods).
  - `MusicBrainzThrottle`: two calls made back-to-back from different
    "tasks" (simulate with `tokio::test` + `tokio::join!`) observe at least
    ~1100ms between the *underlying* HTTP attempts, not just within one
    call chain — the test that actually proves the global-vs-local
    distinction this plan is fixing.
  - `ArtFetchGate`: N+1 concurrent acquires when the gate is size N block
    the (N+1)th until one of the first N releases (standard semaphore
    test, `tokio::sync::Semaphore` already guarantees this — the test is
    really about confirming the gate is actually wired into the fetch path,
    not re-testing tokio itself).
  - `title_matches`: exact match, ≥2/3 token overlap with stopwords
    dropped, below-threshold overlap rejected, two-token minimum.
  - `strip_edition_qualifier`: `"Album [24-bit Remaster]"` → `"Album"`,
    `"Album (Deluxe Edition)"` → `"Album"`, no-marker passthrough, doesn't
    collide with the separate disc-marker stripping (a title with both a
    disc marker and an edition qualifier should be handled by composing
    both helpers, not by one over-eager regex — add a test asserting the
    two are independent and composable).
  - `bio_get`/`bio_put` round-trip on a temp redb store (mirrors the
    existing `lyrics_get`/`lyrics_put` tests already presumably covering
    this shape in `store/mod.rs`, if any exist — check and follow the same
    pattern; if the lyrics store methods aren't unit-tested either, this is
    a good moment to add coverage for both, since neither currently appears
    in `CLAUDE.md`'s enumerated test list).
  - `mb_id_get`/`mb_id_put` round-trip on a temp redb store, both key forms
    (`"artist:{name}"` and `"{artist}\x1f{album}"`); negative case
    (`Some(None)` stored/retrieved distinctly from "never looked up" —
    matches the same three-state convention `lyrics_get`/`bio_get` already
    use: absent key vs. `Some(None)` vs. `Some(Some(v))`).
- **Manual QA**:
  - On a well-tagged library (embedded covers, no `cover.jpg` files),
    confirm art loads with **no** ACK-error log line for a missing
    `albumart` cover file — only the successful `readpicture` call should
    appear in the MPD log for those albums (verifies the reorder actually
    took effect, not just that art still loads).
  - Browse into 10+ artists/albums in one session, restart the app, browse
    the same ones again — bios should appear instantly (from the redb
    store) instead of re-triggering visible "loading" states and MusicBrainz
    log entries.
  - Open a single artist page (with art not yet cached) and watch the
    MPD/HTTP log (per `server-stats-and-diagnostics.md`): confirm **one**
    `search_artist` call, not two — the concrete case §6 exists to fix.
    Re-open the same artist in a later session: confirm **zero**
    `search_artist`/`search_release_group` calls (served entirely from
    `mb_ids`).
  - Open an artist with 15+ uncached albums and watch the same log: confirm
    no more than 4 concurrent art-fetch tasks in flight and no burst of
    MusicBrainz calls tighter than ~1s apart — this is the *existing* bug
    from §3, reproducible today, not a hypothetical.
  - With the future grid view (once built): repeat the same concurrency
    check at grid scale (50+ tiles).
  - Pick an artist/album whose Wikipedia article title doesn't exactly match
    any of the three guessed candidates but is findable via search (e.g. an
    album with a colon or subtitle Wikipedia normalizes differently) —
    confirm the search fallback now finds it where it previously returned
    "no bio found".
