# Album art / Wikipedia / lyrics fetching on all three OSes

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

## Logical review — what is and isn't platform-dependent

The fetch pipeline was read end to end looking for anything that could behave
differently per OS. Most of it is clean:

**No platform dependency (verified):**

- **Everything caches through redb**, one `winrmpc.redb` file — art blobs,
  lyrics, bios, MBIDs. There are no per-item file paths, so none of the usual
  cross-platform filesystem traps apply: no case-sensitivity difference
  (macOS APFS default-insensitive vs Linux ext4 sensitive), no path-separator
  issue, no illegal-character-in-filename issue for artist/album names
  containing `:`/`?`/`*`. Cache keys are `artist\x1falbum` *inside* the DB,
  never on disk. This was a genuinely good design call and it makes item 5
  much smaller than it would otherwise be.
- `Store::cleanup_legacy` (`store/mod.rs:196-205`) is the only path-joining
  code left and it uses `Path::join` throughout.
- **MPD-side art** (`readpicture`/`albumart`, `client.rs`) is pure protocol
  over TCP — same bytes on every OS. The MPD *server* resolves cover files, so
  its filesystem semantics are its own concern, not the client's.
- **The `open` crate** for Wikipedia links maps to `start`/`open`/`xdg-open`
  correctly per platform.
- **`MusicBrainzThrottle`** is `Arc<Mutex<Option<Instant>>>` — no platform
  behaviour.
- **`image 0.25`** with `jpeg`/`png`/`webp`/`gif` features is pure Rust; the
  500px downscale on store behaves identically everywhere.

So the answer to "does it work logically on all three" is **yes, the logic is
portable**. What isn't portable is the TLS stack underneath it, and there's one
unrelated bug worth fixing in the same pass.

## Finding 1 — reqwest pulls OpenSSL in on Linux

`Cargo.toml`:

```toml
reqwest = { version = "0.12", features = ["json"] }
```

Default features are on, so `default-tls` is active, which is `native-tls`.
`Cargo.lock` confirms the resulting tree (`native-tls` at line 2694 depending
on `openssl`, `openssl-probe`, `openssl-sys`, `schannel`,
`security-framework`).

`native-tls` maps to a different backend per platform:

| Platform | Backend | Consequence |
|---|---|---|
| Windows | schannel | No extra dependency |
| macOS | Security.framework | No extra dependency |
| **Linux** | **OpenSSL** | Needs `libssl-dev`/`openssl-devel` + `pkg-config` **to build**, and a compatible `libssl` at runtime |

So the Linux build is the only one carrying a system dependency, and a Linux
binary built on one distro can fail to start on another over an OpenSSL soname
mismatch. Nothing is broken today on a dev box that has the headers — it's a
portability and distribution problem, and it bites exactly when someone tries
to ship a Linux build (see the packaging work in
[app-icon-cross-platform](app-icon-cross-platform.md)).

**Fix** — move to rustls so all three platforms use the same pure-Rust stack:

```toml
reqwest = { version = "0.12", default-features = false, features = [
    "json",
    "charset",
    "http2",
    "macos-system-configuration",
    "rustls-tls-native-roots",
] }
```

Two details that are easy to get wrong:

- **`default-features = false` drops more than TLS.** It also removes
  `charset`, `http2` and `macos-system-configuration`. Re-add all three or
  you'll silently lose HTTP/2 and, on macOS, respect for the system proxy
  settings — a regression that would only show up on one platform, which is
  exactly the class of bug this whole batch is about.
- **`rustls-tls-native-roots` vs `rustls-tls`**: native-roots reads the OS
  trust store, so corporate MITM proxies and custom CAs keep working;
  `rustls-tls` bundles Mozilla's webpki roots and is fully self-contained but
  fails behind such a proxy. For a desktop music client, native-roots is the
  safer default.

Trade-off to accept: rustls has no OS-level certificate revocation checking and
slightly larger binary size. Neither matters for read-only API calls to
MusicBrainz, Cover Art Archive, Wikipedia and LRCLIB.

## Finding 2 — the MusicBrainz User-Agent is a placeholder

`src/art/musicbrainz.rs:15`:

```rust
const USER_AGENT: &str = "winrmpc/0.1.0 (https://github.com/user/winrmpc)";
```

Two problems: the version is frozen at `0.1.0` (the app is at 0.4.1), and
`https://github.com/user/winrmpc` is a **placeholder URL that does not
identify anyone**. MusicBrainz's terms require an application-identifying
User-Agent with real contact information, and they do throttle or block
clients that don't provide one. This is not a platform issue — it fails
identically everywhere — but it's the single highest-risk line in the fetch
path, and the fix is to copy what `lrclib.rs:8-12` already does correctly:

```rust
const USER_AGENT: &str = concat!(
    "winrmpc/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/mickegris/winrmpc)"
);
```

Cover Art Archive redirects to `archive.org`; reqwest follows redirects
(10 max) by default, so the same UA carries through. No change needed there.

## Finding 3 — HTTP client construction fails differently in the two clients

- `musicbrainz.rs:141-145` — `.build().expect("Failed to create HTTP client")`
  → **panics**, taking the app down.
- `lrclib.rs:40-44` — `.build().unwrap_or_default()` → silently substitutes a
  default client, discarding the User-Agent and the 10s timeout.

Neither is right. Client construction can genuinely fail when the TLS backend
can't initialise — which under `native-tls` on Linux is a real scenario (no
system certs, unreadable trust store). Both should log an ERROR and degrade to
a working default rather than panicking or going quiet. Moving to rustls
(Finding 1) makes this much less likely, but the handling should still be
correct.

## Finding 4 — no timeout on the whole art chain

Both clients set a 10s per-request timeout. The album path can issue several
requests in sequence (MusicBrainz release-group search → CAA lookup → image
fetch, plus the retry ladder in `search_queries`). Worst case a single album
occupies the stage-2 queue for far longer than 10s. Since stage 2 is
**one in flight and unbounded** by design (per CLAUDE.md), a pathological album
slows the whole background sweep.

Not a platform issue and not a regression — noting it because this is the pass
where the HTTP layer is open. A per-album deadline (`tokio::time::timeout`
around the whole `fetch_album_art` call) would bound it cheaply. Optional.

## Plan

1. **Switch reqwest to rustls** with the full feature list above. Verify the
   `openssl-sys` entries disappear from `Cargo.lock`.
2. **Fix the MusicBrainz User-Agent** to the `concat!`/`CARGO_PKG_VERSION`
   form. Consider hoisting both User-Agent constants into one shared place so
   they can't drift again.
3. **Fix both client-construction failure paths** — log at ERROR, degrade,
   never panic.
4. *(Optional)* per-album deadline on the stage-2 MusicBrainz chain.
5. **Run the live suite on each OS** (below). No new automated tests are
   proposed: the existing ones already cover the logic, and what changes here
   is the transport, which only a real request exercises.

## How to confirm on the real OS

The repo already has the right tool for this — `src/live_tests.rs`. Two of its
tests are gated behind `WINRMPC_TEST_MUSICBRAINZ=1` specifically so a routine
sweep doesn't hammer a free service, and they are exactly the ones that
exercise this code:

```bash
WINRMPC_TEST_MPD=host:6600 WINRMPC_TEST_MUSICBRAINZ=1 \
  cargo test -- --ignored --test-threads=1 \
  live_musicbrainz_resolves_locally_artless_albums \
  live_wikipedia_bios_for_awkward_tags
```

Run that on **Windows, macOS and Linux** after the rustls switch. It proves
TLS negotiation, redirect following, the User-Agent being accepted, and JSON
decoding all still work on each platform. `live_diagnose_album_art_sources`
is the companion diagnostic if an album comes back blank.

For lyrics there is no live test; play a track with known LRCLIB coverage on
each OS and confirm synced lyrics appear and cache (second play is instant).

Also worth checking on a clean Linux box **without** `libssl-dev` installed
that `cargo build` now succeeds — that's the concrete proof Finding 1 is fixed.
