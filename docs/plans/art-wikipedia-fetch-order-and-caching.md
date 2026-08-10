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

## 3. No shared concurrency/rate-say limit across fetch tasks

### Today

Each MusicBrainz-touching method sleeps `1100ms` **locally**, inside that
one call chain (`musicbrainz.rs:115,151,176,196,248`, "Respect rate limit").
This bounds the rate of *sequential* requests within one `fetch_album_art`/
`fetch_artist_bio` call, but does **not** bound how many such call chains run
**concurrently** — `Task::batch` already fires several art/bio fetches in
parallel today (e.g. `ArtistSelected`'s `Task::batch([albums_task,
artist_art_task, bio_task])`, `app.rs:651`), and the planned Albums **grid
view** (`library-album-identity-and-multidisc.md`, Part C) will make this
much worse: a grid of N tiles queues N independent `fetch_art` tasks, each
with its own local 1100ms sleep, all racing to hit MusicBrainz at roughly
the same time. MusicBrainz's usage policy (already the reason for the
1100ms constant) is a *global* ~1 req/s courtesy limit, not "1 req/s per
in-flight task" — the current design can violate it under exactly the
condition the grid view is about to create, risking throttling/bans that
degrade art loading for the whole session.

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

## Implementation order

| # | Item | Size | Why this order |
|---|------|------|---|
| 1 | Split `tag_art`/`cover_file_art`, reorder in `fetch_art` | S | Immediate correctness fix, zero new dependencies |
| 2 | `self.mb_client` reuse (`#[derive(Clone)]`, delete inline `::new()` calls) | XS | Trivial, unblocks nothing else but should ship early |
| 3 | `MusicBrainzThrottle` (global) + `ArtFetchGate` (semaphore) | S | Needed *before* the grid view work in the library plan lands, not after |
| 4 | `bios` redb table + store/load wiring | S | Independent of 1-3 |
| 5 | Wikipedia title-match helper + search fallback | M | Independent, can slot in anytime |
| 6 | `strip_edition_qualifier` | XS | Small, pairs naturally with #5 |

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
  - With the future grid view (once built): scroll through a 50+ album grid
    and confirm the MPD/HTTP log (per `server-stats-and-diagnostics.md`)
    never shows more than 4 concurrent art fetches or a burst of
    MusicBrainz calls tighter than ~1s apart.
  - Pick an artist/album whose Wikipedia article title doesn't exactly match
    any of the three guessed candidates but is findable via search (e.g. an
    album with a colon or subtitle Wikipedia normalizes differently) —
    confirm the search fallback now finds it where it previously returned
    "no bio found".
