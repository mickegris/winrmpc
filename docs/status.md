# Current state — session handoff

Living document: what's true right now, what's unverified, what to pick up
next. Durable architecture and domain rules belong in `CLAUDE.md`; this file
is the part that goes stale, so it lives here rather than there.

Last updated: 2026-08-14. Branch: `improve/cross-platform-and-ui` — planning
only so far, no code changes yet.

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

### Done so far on this branch

**Linux app icon + desktop integration** (plan 1, steps A–C). One shared
generator in `src/icon_design.rs` feeding the window icon, the Windows ICO and
new `packaging/linux/` assets; `application_id` wired up so Wayland can match
the `.desktop` file; `install.sh` with user/system prefixes and `--uninstall`;
README section. 185 offline tests (was 181), zero warnings. The install and
uninstall paths were exercised against a scratch prefix and the `.desktop`
file passes `desktop-file-validate` — but **the icon has not been seen on a
real Wayland or X11 session**, which is the only thing that actually proves it.
macOS still has no `.app` bundle and therefore still no icon.

**Nothing else here is verified on real macOS or Linux hardware.** Every claim is
sourced from the vendored `iced 0.13.1` / `iced_winit 0.13.0` /
`winit 0.30.13` / `Cargo.lock` with file:line references, and each plan ends
with a "How to confirm on the real OS" section that is the actual acceptance
criterion. Suggested implementation order: affordance → network → highlighting
→ storage → icons.

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

## Suggested next steps

1. Work the five plans in the order given above, on
   `improve/cross-platform-and-ui`.
2. Build and run on **macOS and Linux** — the tofu-box and icon findings can
   only be confirmed there, and they gate how wide the glyph audit in plan 4
   has to be.
3. Merge the branch into `release/v0.4.2` and ship via the `release` skill.

### Still open from v0.4.1

- If Albums is slow rather than broken at library scale, address item 3 under
  "Unverified" above.
