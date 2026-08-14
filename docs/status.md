# Current state — session handoff

Living document: what's true right now, what's unverified, what to pick up
next. Durable architecture and domain rules belong in `CLAUDE.md`; this file
is the part that goes stale, so it lives here rather than there.

Last updated: 2026-08-14. Branch: `improve/cross-platform-and-ui` — **all five
plans implemented, plus lyrics sync/scroll, a track-list layout pass, release
CI and the 0.4.2 bump**. Merged to `main` and **not yet tagged**: the tag is
what publishes, and the visual work still needs a look on a real screen.

**v0.4.1 shipped.** `release/v0.4.1` was merged to `main` (PR #20, commit
`a810419`) and tagged `v0.4.1`. Everything in the sections below describing
that branch as pending is historical record, kept because the *findings* are
still current — only its "not yet merged" framing is out of date.

## Current branch: `improve/cross-platform-and-ui`

Five plans written on 2026-08-14 from a read-only investigation, targeting a
future `release/v0.4.2`. Umbrella:
[`docs/plans/cross-platform-and-ui-0.4.2.md`](plans/cross-platform-and-ui-0.4.2.md).

The headline finding is that **three separate bugs are the same mistake** —
a Windows-only resource named directly, with a silent fallback elsewhere:

1. `Font::with_name("Segoe UI Symbol")` (`link.rs:36`) — the row-action
   glyphs (`▶ + ⏭ ☰`) almost certainly render as **tofu boxes on macOS and
   Linux**, since the fallback is the very font the code's own comment says
   lacks them. This is the real cause of "the buttons are hard to understand".
2. `window::Settings.icon` → winit `set_window_icon`, which is a **documented
   no-op on macOS** and an **empty no-op on Wayland**. `application_id` is
   also left empty, so Wayland can't match a `.desktop` file either.
3. `reqwest` on default features → `native-tls` → **OpenSSL on Linux only**
   (schannel/Security.framework elsewhere).

Plus: the MusicBrainz `User-Agent` is a placeholder
(`winrmpc/0.1.0 (https://github.com/user/winrmpc)`) that risks being blocked;
only the Queue highlights the playing track; and macOS storage works but lives
in `~/Library/…/com.winrmpc.winrmpc/`, which Finder hides — that's the whole
of the "couldn't find the config file" report.

### Done on this branch — all five plans

One commit per plan, in the umbrella's suggested order. **202 offline tests
(was 181), 13 live, zero build warnings.**

| Plan | Commit | State |
|---|---|---|
| 1 — app icon | `190f14e` | steps A–C done; **D (macOS `.app`) and E (release flow) not done** |
| 4 — row actions | `a44e9f3` | A–D done, E declined |
| 5 — network | `8868b0a` | all five steps + the optional deadline; **verified on Linux** |
| 3 — highlighting | `7812d5c` | A–C done, D deferred as planned, E out of scope |
| 2 — storage | `8c21f53` | A–E done |

Then, on top of the five plans:

| Work | Commit | State |
|---|---|---|
| Lyrics sync/scroll toggle | `66cb985` | **verified against real LRCLIB data** |
| Version bump to 0.4.2 | `283c5b6` | crate description no longer says "Windows" |
| Release CI | `a91432d` | **not yet run** — no workflow has ever executed for this repo |
| Queue row/header alignment | `40ace14` | disabled-not-omitted actions; derived widths |
| Track-list column order | `118ed34` | four passes with the user; final order below |
| ship/release skills | `6d57337` | rewritten for CI-built binaries |

**Track-list column order**, now identical in all five lists (album, search,
browser, playlist detail, queue):

```
[playing marker] [play] [number] [title Fill] … [length] [function buttons]
```

Play is the primary action so it leads, beside the number and title it acts
on; the rest are secondary and follow the length. The queue gained a leading
play button it never had (`QueuePlay(pos)`, **not** `PlaySong(uri)` — that
would enqueue a second copy). Everything before the `Fill` title is
fixed-width, which is what keeps rows from drifting against each other.

Each plan file now carries a "What was actually built" section and a status
banner; the durable rules landed in CLAUDE.md.

**Lyrics.** The pane autoscrolled off the 500ms status poll and called
`snap_to` unconditionally, so any manual scroll was undone within half a
second — synced lyrics were readable *only* at the current line. There is now
a **Sync / Scroll** switch, with the highlight kept in both modes. Two silent
duplications behind it were collapsed: the lyrics cache key (written by hand
in three places) into `Song::lyrics_key()`, and the active-line calculation
(computed separately by the view and the autoscroll, so they could scroll to a
different line than they highlight) into `lyrics::active_line()`. Verified
live: `live_lrclib_returns_parseable_synced_lyrics` fetches three real tracks
and asserts sorted, advancing, non-empty timestamped lines.

**What is actually verified, and what is not.** Plan 5 is the only *plan*
proven against reality: `openssl-sys` and `native-tls` are gone from the dependency
tree, and the new `live_tls_reaches_every_lookup_host` gets HTTP 200 from
MusicBrainz, Cover Art Archive, Wikipedia and LRCLIB on Linux, with
`live_wikipedia_bios_for_awkward_tags` still resolving all ten awkward tags.
The Linux config/cache paths were confirmed on disk. The release binary
links **only libc, libm and libgcc_s** — no libssl, no libcrypto — which is
finding 1 proven at the binary level: this build is now portable across
distros.

The lyrics work is verified too, against live LRCLIB responses rather than
only self-written fixtures.

Everything visual is **code-verified only**. Nobody has seen the bundled icon
font render in iced, the tooltips, the playing-row highlight, or Settings →
Storage. The icon glyphs were rasterised directly from the built `.ttf` to
confirm each codepoint draws the intended shape, which proves the *font* is
right but not that iced resolves the bundled family at runtime. The window
icon has still not been seen on a real Wayland or X11 session, and macOS has
no `.app` bundle so it still has no icon at all.

**The GUI has not been launched this session, deliberately.** The previous
session ended in a hard machine freeze — kernel log shows
`xe … [drm] *ERROR* [CRTC:151:pipe A] flip_done timed out` seconds after the
last edit, then an unclean reboot. That is a display-driver hang, and the
likely trigger was launching this very (wgpu) app to check the Wayland icon.
Worth knowing before running it again.

**One self-inflicted incident, recorded because it cost real data.** A test
written for plan 2 called `AppConfig::save_and_log()`, which resolves the
*real* user config path — running `cargo test` overwrote
`~/.config/winrmpc/config.toml` with defaults. Servers, radio stations and the
saved partition in that file were lost and are not recoverable from the repo
or the cache DB. The test was replaced with one that sets
`WINRMPC_CONFIG_DIR` to a scratch dir, and the rule is now written down in
CLAUDE.md: **never call `save()`/`save_and_log()` from a test without the env
override in place.**

## Where things stood at v0.4.1

`release/v0.4.1` is green: **181 offline tests, 12 live tests, zero build
warnings**, debug and release both build. The branch contains a long run of
post-parity fix work: two rounds of code review and their fixes, real-library
album-matching improvements, live integration tests, and a batch of
user-requested UI/behaviour changes.

## Verified against a real server

A live MPD **0.24.0** at `10.0.1.3` (9846 songs, 802 albums, 4 partitions)
plus a Snapcast server on `1705`. Run the opt-in suite with:

```bash
WINRMPC_TEST_MPD=10.0.1.3:6600 WINRMPC_TEST_SNAPCAST=10.0.1.3:1705 \
  cargo test -- --ignored --test-threads=1
```

Anything that mutates state uses a throwaway MPD partition and deletes it;
the default partition's queue is never touched. Confirmed clean after every
run.

## Fixed after the first run on real hardware (2026-08-12)

The branch was finally built and run on Windows against the user's own
library (`miknuc.klova:6600`). Four issues came out of that, all now fixed —
see CLAUDE.md's "Background album-art fetching" and "Album cover grid vs
list" for the durable versions:

1. **Album art appeared to stop loading in grid view.** Two real causes: the
   flat 24-album prefetch cap (nothing ever started the 25th fetch), and
   `fetch_album_group_art` writing negative-cache entries it never read, so
   every coverless album re-ran the full MPD + MusicBrainz path on every
   visit. Replaced with a background queue (`art_queue` +
   `drain_art_queue`, concurrency 3) and a real `is_known` gate.
2. **Sidebar too tight** — 90px clipped the connection line and wrapped
   "Recently Added". Now `SIDEBAR_WIDTH` 132px with a slim 4px scrollbar and
   size-12 labels.
3. **Right-most Recently Played cover rendered as a sliver** when the window
   was narrow — five 120px thumbs don't fit beside the lyrics pane and a
   plain `row` squeezes the overflow into the last child. Now a wrapping row.
4. **Grid view had dead space on the right** — it chunked into a fixed 5
   tiles per row. Now a wrapping row, so the column count follows the window.

## Second run — the art fix was wrong (2026-08-12)

Running against the real library showed the queue *working* but crawling: a
log of the Albums sweep had `find` at ~2.7ms and `readpicture`/`albumart`
returning real covers, separated by 0.5–3s gaps of nothing. Cause: every
album with no local art went straight on to MusicBrainz **inside the same
queue slot**, and `MusicBrainzThrottle` serializes those globally at
`MB_MIN_INTERVAL` (1.1s), plus a possible second `search_release` hop. Three
such albums stalled all three workers, so the sweep moved at ~1 album/second
and albums late in the alphabet (the reported case: Stone Temple Pilots —
*Purple*, which has embedded art) were still minutes away.

Fixed by splitting the stages — see CLAUDE.md "Background album-art fetching".
Local probing now runs to completion first (whole library in seconds),
MusicBrainz drains afterwards one at a time, unbounded. Embedded art was never
broken; it just never got a turn. (A 50/session MusicBrainz budget was tried
and removed at the user's request: the problem was the stall, not the count.)

Also fixed in the same pass: the recents strip is a horizontal scrollable
(the `.wrap()` from the first pass grew it downwards over the player bar),
and `min_size` is now 1000×700.

## External-lookup matching ported from mikMPD (2026-08-12)

mikMPD had a round of real-library fixes to how album/artist names are
matched against MusicBrainz and Wikipedia that winrmpc never received. Ported
with tests (CLAUDE.md "Wikipedia / MusicBrainz" has the full list). The two
that were outright bugs:

- **`AC/DC` broke every query it appeared in.** The old `sanitize` deleted
  brackets and quotes but left `/`, `+`, `-`, `!` — and `/` opens a Lucene
  regex. Now backslash-escaped like mikMPD's `luceneEscape`.
- **`(White Rabbit)` was being stripped as an edition qualifier**, because
  the keyword test was a substring match and "rabbit" contains "bit".

Also added: Unicode folding on both sides of every lookup, sort-order article
handling (`"Beatles, The"`), year/audio-spec/catalogue-number recognition,
looping qualifier stripping, MusicBrainz **result validation** (title +
artist — previously the top-scoring hit was taken unchecked), and the
three-step query ladder.

### Verified live against the real data (2026-08-12)

`live_diagnose_album_art_sources` against `10.0.1.3` settled the "common
albums show no art" report: **none of them have local art at all** — the
Depeche Mode ones are CUE-sheet tracks (`….cue/track0001`) whose covers MPD
cannot reach, and `live_probe_cue_album_art_fallback` confirmed asking about
the `.cue` path doesn't help either. So those albums depend entirely on
MusicBrainz, which is why the budget mattered and why the matching fixes did.

The diagnostic also exposed the tag shapes involved: `ACDC` (no slash),
`Alan Parsons Project, The`, `Blue  Oyster Cult` (two spaces), **`Blue Îyster
Cult`** (mojibake of `Öyster`), and Depeche Mode's whole discography tagged
`… [UK]`. `live_musicbrainz_resolves_locally_artless_albums` now resolves
**14 of 14**, and `live_wikipedia_bios_for_awkward_tags` returns a correct
bio for every one of them.

**Existing caches must be purged or none of this shows.** `neg_purge_v2` in
`Store::open` does it automatically on next launch — it clears negative art
entries *and* `Some(None)` MBIDs, since a cached "no match" would otherwise
suppress the corrected query.

## Cache purge UI (2026-08-12)

**Settings → Cache**: shows art bytes on disk against the configured limit
and clears art + lyrics + bios + MBIDs on a two-press confirm. Play history is
spared (it isn't a cache — nothing could re-derive it), as are the migration
markers. Two presses because a purge is one click to trigger and hours to
undo: every cover re-downloads and the MusicBrainz stage re-crawls.

This also makes `neg_purge_v2` less load-bearing — a user who hits a stale
cache after a matching change now has a button rather than needing a code-side
migration.

**No UI change has been seen rendered.** Code-verified only (`cargo check`,
`cargo build --release`, 181 tests).

## Open / unverified — read this before continuing

1. **The MPD connection desync trigger is still unknown.** The *recovery*
   failure is confirmed and fixed (see CLAUDE.md "Connection desync"). The
   original cause is not reproduced: the test server has no embedded album
   art, so the binary path only ever returns `OK` or `ACK`, never multi-chunk
   data — `live_concurrent_art_fetches_do_not_desync_the_connection` passes
   because it cannot exercise the risky path. The user's own library *does*
   have embedded art, so a run against it is the real test. If it recurs, the
   `WARN connection desynced on <verb> …` line names the command
   responsible; that is the thread to pull.
2. **The art queue's sustained load is untested at library scale.** Opening
   Albums on an 800-album library now enqueues all 800 and works through them
   3 at a time, where before it did 24 and stopped. Each is an MPD `find`
   plus art probes on the one shared connection. Expect roughly 10–20s of
   background traffic on a cold cache, then near-silence (positives and
   negatives are both cached). If the 500ms status poll visibly stutters
   during a first browse, `ART_FETCH_CONCURRENCY` is the knob.
3. **Album grid may not be usable at library scale.** ~4 widgets per album,
   no virtualisation, ~3200 widgets on an 800-album library. If the Albums
   view feels slow or freezes (as opposed to erroring), this is the suspect,
   not the connection. Options: page the grid, or only build tiles for
   albums with cached art.
4. **`docs/plans/review-fixes-correctness.md` lists two deliberate
   deferrals**: punctuation-folding the album grouping key (en/em-dash,
   smart quotes — the *case*-folding half is done), and de-duping
   `recent_albums` on the disc-stripped base.
5. **Grouping-matcher gap, with a ready test corpus**: `Volume 3 Disc3
   (Rem.2007)` — a marker followed by a non-marker bracket — is deliberately
   not handled, since stripping from the middle of a name is materially
   riskier. Same doc has the six real album names that motivated the rules.
6. **One leftover from the deleted `servers-version-alignment.md` plan.**
   That plan was verified shipped and removed on 2026-08-12: multi-server
   config + migration + Settings UI (0.4.0), the sidebar version line
   (`sidebar.rs`), the CD-device field moved out of Settings into the CD
   view, and the Now Playing structural-stabilization fix (the stable-column
   skeleton in `now_playing.rs`). The single item never done was its own
   explicitly-optional "nice to have": a `winrmpc vX.Y.Z` line in the
   **Settings** view under the title. The sidebar already shows the version,
   so this is cosmetic — recorded here only so deleting the plan didn't
   silently drop it.

## Suggested next steps — manual testing, then release

Merged, not tagged. The local release build is at `target/release/winrmpc`, and
`dist/winrmpc-v0.4.2-linux-x86_64.tar.gz` is the artifact CI would produce
(binary + `packaging/linux/` + README + LICENSE).

**The workflow had to reach `main` before it could be run at all** —
`workflow_dispatch` is only offered for workflows already on the default
branch. That is why this batch was shipped before being tagged, and it is a
one-time constraint: from here, a manual run can build an `.exe` from any
branch.

1. **Run the app and look at it.** Everything visual is unverified: icon
   glyphs and tooltips, the playing-row highlight, the lyrics Sync/Scroll
   switch, Settings → Storage. **Note the display-driver hang above before
   launching on this machine.**
2. **Re-enter the MPD server in Settings** — the config was overwritten with
   defaults (see the incident note above), so it currently points at
   `127.0.0.1:6600`.
3. Run the live suite against the real server, none of which has run since
   these changes landed:
   ```bash
   WINRMPC_TEST_MPD=10.0.1.3:6600 WINRMPC_TEST_SNAPCAST=10.0.1.3:1705 \
     cargo test -- --ignored --test-threads=1
   ```
4. **Push the branch and use "Run workflow" on the Release action** to get a
   Windows `.exe` to test. That path publishes nothing — the `publish` job is
   gated on `refs/tags/`.
5. Only then: merge to `main`, tag `v0.4.2`, and let the workflow build and
   attach both binaries.

Deferred and not blocking a release: plan 1 steps D/E (macOS `.app` bundle),
plan 3 step D (album-level highlighting).

### Still open from v0.4.1

- If Albums is slow rather than broken at library scale, address item 3 under
  "Unverified" above.
