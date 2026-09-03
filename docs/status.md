# Current state — session handoff

Living document: what's true right now, what's unverified, what to pick up
next. Durable architecture and domain rules belong in `CLAUDE.md`; this file
is the part that goes stale, so it lives here rather than there.

Last updated: 2026-09-03. **v0.5.1** — a patch for two defects a manual pass
of 0.5.0 found, both in code 0.5.0 itself shipped. **v0.5.0 released**:
password authentication (which had never worked at all), plus three
deferrals.

## 0.5.1 — what shipped

The manual UI pass 0.5.0 was released without finally happened, and found
both of the things it was most likely to:

| Reported | Cause |
|---|---|
| A one-disc album captioned **"2 discs"** (Scorpions, *Love at First Sting*) | The caption counted `variants.len()`, but a variant is any raw tag that folded into the group — including two capitalisations of one album. The grouping merging them is the feature; counting them as discs was the bug. 5 rows on the real library. |
| **Search listed almost no albums** — `Metallica` gave 195 songs and 2 album rows | An album was offered only when its *title* matched the query. Searching an artist therefore hid their entire discography. Now the title **or** the artist matches: 2 → 14. |

Both are in code 0.5.0 introduced (the search rule) or made visible (the
caption — case-only variants already merged before 0.5.0, so that row was
mis-captioned in earlier releases too; the punctuation folding only added
one more).

`AlbumGroup::disc_count()` **under-reports rather than over-reports** by
design: a set distinguished only by a `disc` tag and not by its names — The
Beatles' `1967-1970`/`1967–1970` — now reports one disc, where before it said
two by accident. A missing caption is a gap; a wrong one is a claim, and the
album page still shows the true count from `effective_disc`.

**306 offline tests, 20 live.** Both fixes are pinned by live assertions, not
just unit tests: the grouping test now fails if a group with no disc marker
on any variant claims to be multi-disc, and the search test fails if an album
by a matched artist isn't offered.

## 0.5.0 — what's on the branch

| | Where it came from |
|---|---|
| **MPD password auth never worked** — the reported bug | new |
| Punctuation-folded album grouping key | [review-fixes-correctness](plans/review-fixes-correctness.md) §6, deferred since 0.4.1 |
| `recent_albums` de-duped on the disc-stripped base | same doc, item 1's "also worth folding in" note |
| Search sections + batch select | [library-album-identity-and-multidisc](plans/library-album-identity-and-multidisc.md) Part D, deferred since 0.4.1 |

**The password bug is the one worth remembering.** `MpdClient::password()`
existed from 0.4.0 and *nothing ever called it* — the field was loaded from
TOML, editable in Settings, saved back, and read by no one. What made it
present as "connected but empty" rather than as a login failure: `connect()`
only opened the socket and read the banner, which succeeds regardless of
auth, so the app set `connected = true`; every command then ACKed with a
permission error, and an ACK is deliberately *not* connection-fatal, so the
socket was never dropped and `ConnectionTick` never retried. The fix sends
the password inside `connect()`, before the connection is published, and
fails the whole connect on rejection.

Two things fell out of the work rather than the report:
- A persistent connect failure toasted **every 3 seconds forever**, since
  `ConnectionTick` retries at that rate and every attempt raised a fresh
  toast. Now only a *change* of error is reported.
- The `UNKNOWN_ARTIST`/`UNKNOWN_ALBUM` placeholders were defined in
  `ui/widgets/link.rs` while the strings they must match are produced in
  `mpd/types.rs`. Moved to where they're produced; `link.rs` re-exports.

**299 offline tests (was 268), 20 live (was 19).** No new clippy warnings —
note the repo still carries 15 pre-existing ones under a newer clippy than CI
pins.

### Verified against the real server (miknuc.klova:6600, MPD 0.24.0)

This is the user's own **password-protected** server, which is what made it
the acceptance check: the full live suite passes with
`WINRMPC_TEST_MPD_PASSWORD` set and fails with `ACK [4@0] … you don't have
permission for "find"` without it. 849 albums, 849 of 849 with add-times.

The punctuation fix was confirmed the same way, by an existing live test
**failing on the new behaviour**:
`live_album_grouping_collapses_real_multidisc_albums` asserted that every
variant strips to its group's base, which stops being true the moment
folding works — the library holds The Beatles' `1967-1970` and `1967–1970`
as two tags of one set. The assertion now compares through
`album_grouping_key`.

### Still not verified

- **No manual UI pass**, which matters more this round than usual: the Search
  view was rewritten (three sections, a checkbox column added to every song
  row, a batch action bar) and none of it has been seen rendered. Worth
  eyeballing: the song row's columns still line up now that a checkbox leads
  them, the action bar appearing/disappearing doesn't reflow the list
  underneath, and the Albums section's covers actually arrive.
- **Entering a wrong password** should now toast "Authentication failed:
  incorrect password" once, not repeatedly. Only tested against a mock.

### Known gap, deliberately not fixed here

**`art_key_for` is not punctuation- or case-folded**, so a per-track art key
built from a raw tag can differ from the group-level key the grid stores
under — the cover shows in one place and not the other. Pre-existing (it
already split on case). Folding it would re-key every cached cover in every
existing install, which is a migration rather than a fix, and was out of the
scope approved for this branch.
`live_album_grouping_collapses_real_multidisc_albums` prints every split it
finds: **6 of 45** multi-disc groups on the real library, 5 of them
case-only.

### Found by the smoke test, and fixed

**The playing track's cover was fetched from MusicBrainz once per status
poll while the fetch was in flight.** A 15-second run of the release binary
against the real server logged the *same* album's cover arriving **four
times**, ~1.2s apart — exactly the throttle interval, so four real requests
serialized behind `MusicBrainzThrottle`, each holding a slot the background
sweep needed.

Cause: `CurrentSongUpdated` fires on every 500ms poll and its only art guard
was `!self.art_handles.contains_key(&art_key)`, which becomes true when a
fetch *finishes*. The background queue has `art_pending` for exactly this;
the one-off path (playing track, artist images, `fetch_recent_art`) had
nothing.

Fixed with `oneoff_art_pending`, and by moving *both* guards inside
`fetch_art`/`fetch_artist_art` — three call sites each repeated the
`art_handles` check and none could have known a fetch was already running.
**Re-measured under the same cold-cache conditions: four fetches became
one.** Pre-existing, not introduced by this branch.

### Still pending, and blocked on the user

**macOS signing and notarisation** was flagged for 0.4.5, slipped it, and has
slipped 0.5.0 too. The repo-side work (B/C/D in
[`macos-signing-and-notarization.md`](plans/macos-signing-and-notarization.md))
is ready to write, but step A is an account action that cannot be done from
the repo, and `gh secret list` is **empty** — no secrets are configured. Until
they are, the `.app` stays unsigned and the four places documenting
`xattr -dr com.apple.quarantine` all stay correct and must not be changed.

## 0.4.5 — what shipped

The live suite had never been run against a real server during the 0.4.4 cycle.
It was, and it found two defects that **cannot be reproduced offline**:

| Defect | Why no offline test could catch it |
|---|---|
| `RecentlyAddedLoaded` re-sorted by `last_modified`, discarding the `sort -Added` order it had just requested | A fixture can't have a uniform mtime. On 10.0.1.3 every file reports the same `Last-Modified` (a mass rewrite) while `Added` spans eight months, so mtime there is nearly noise |
| `fold_album_added` keyed on the raw `AlbumArtist` tag; MPD substitutes `Artist` when it's absent | A mock server can't disagree with itself about a fallback tag. Cost: 453 of 801 album rows had no add-time |

`live_diagnose_album_added_coverage` is kept as a permanent diagnostic — it
prints the mismatched keys and is what found the second one.

### Verified against the real server (10.0.1.3, MPD 0.24.0)

**19 of 19 live tests pass**, including the two network-gated ones
(`WINRMPC_TEST_NETWORK=1`). Numbers worth keeping:

- The add-time walk is **801 albums in 2 pages, ~115ms** — `ADDED_PAGE`
  (10 000) is comfortable and the Added sort is effectively instant, which had
  been flagged as unknown.
- Add-times span 2026-01-05 → 2026-08-21; the server answers on the
  `AddedSince` rung.
- Album add-time coverage is **801 of 801**.

### Still not verified

- **No manual UI pass** for 0.4.4 or 0.4.5. Worth eyeballing: a track with a
  long artist *and* long album in the player bar (it should truncate, not
  collide with Previous, and the bar's height shouldn't change between
  tracks), and the sort controls' placement in all six list headers.

## Pending since 0.4.5 — still open, see "blocked on the user" above

**The macOS app should be signed and notarised.** It wasn't by 0.4.5 and
isn't by 0.5.0; the account-side step has not been done. A paid Apple
Developer account now exists, which was the only blocker
([`macos-signing-and-notarization.md`](plans/macos-signing-and-notarization.md)).
Once it lands, the notes should say plainly that the app now opens by
double-click and that the `xattr -dr com.apple.quarantine` step is no longer
needed — anyone who has been running that command deserves to be told to stop.
Four places still document it (README, `bundle.sh`, the app-bundle plan, and
the release skill's notes template) and all four have to change in the same
commit, or the instruction outlives the problem.

The account work is on the user, not in the repo: a *Developer ID Application*
certificate exported as a password-protected `.p12`, an App Store Connect API
key, and five GitHub secrets. The plan lists them.

## 0.4.4 — what shipped

| | Plan |
|---|---|
| **Recently Added truncated the wrong end** — the reported bug | [recents-limits](plans/recents-limits.md) |
| Player bar's artist/album are links | [player-bar-clickable-links](plans/player-bar-clickable-links.md) |
| Sorting: A–Z/Z–A everywhere, plus Added on the album lists | [list-sorting](plans/list-sorting.md) |

The Recently Added fix is the one worth remembering: `find "(modified-since
…)" window 0:2000` sliced MPD's database order and the client sorted the
survivors, so past 2000 matches *which* albums appeared was decided by path
order. It now sorts server-side (`sort` runs before `window`), via a three-rung
ladder down from 0.24's real `added-since` add-time.

Two bugs came out of the work rather than the report:
- `Vec<String>::sort()` is byte order, so `a-ha`, `dEUS`, `k.d. lang` and
  `will.i.am` were all filed past `ZZ Top` in the Artists list.
- Splitting the player bar's one `text` into two links loses the word wrap
  that kept it inside its 250px slot; without a clip container it would draw
  over the transport buttons.

**268 offline tests (was 246), 18 live.** Clippy is clean on the changed
code — note the repo has 15 pre-existing warnings under a newer clippy than
CI pins, and `cargo fmt --check` was already dirty before this branch.

### Not verified yet

- **Nothing here has been run against the real server.** The Recently Added
  live tests (`live_recently_added_is_newest_first`,
  `live_recently_added_reports_which_rung_the_server_answers_on`) are the
  acceptance check and need `WINRMPC_TEST_MPD=10.0.1.3:6600 cargo test --
  --ignored --test-threads=1`. The first is designed to fail on the old query.
- **The add-time walk has never been timed against a real library.**
  `live_album_added_walk_covers_the_library` prints its duration and page
  count — that is what decides whether `ADDED_PAGE` (10 000) is set sensibly,
  and whether the Added sort is usable or merely present.
- **Sorting by release year was built and removed** before release: it rested
  on an unconfirmed assumption about MPD's two-level `group` nesting. See
  [list-sorting](plans/list-sorting.md); re-adding means confirming the
  nesting against a real server first.
- **No manual UI pass.** Specifically worth eyeballing: a track with a long
  artist *and* long album, to confirm the player bar truncates rather than
  colliding with Previous and that the bar's height stops changing between
  tracks; the sort controls' placement in all six headers; and how long the
  Added sort takes to become usable on the real library.
- Recently Played was reported as working and is **untouched** — its 30-day /
  100-entry cap stands (Part B of the recents plan, deliberately not done).

### Suggested next

1. Run the live suite against 10.0.1.3, then the manual UI pass — 0.4.4 was
   released without either.
2. macOS signing (see the pending section at the top).

## 0.4.3 — what shipped

The eight planned items (see [ui-0.4.3](plans/ui-0.4.3.md)): clickable
artist/album names, icons for the transport controls and Back, album-level
highlighting plus jump-to-current, error toasts, light/dark mode, window
size/maximised persistence, keyboard shortcuts, and the macOS `.app` bundle.

**Six bugs came out of the manual test pass**, which is the argument for having
done one:

| Found | Cause |
|---|---|
| Transport buttons drew **nothing** | Geometry, not the font: default button padding left an 18px box for a glyph needing ~22px, and iced draws no line rather than a clipped one. Blank-not-tofu is the tell. |
| "Unknown Artist" on tagged files | `display_artist()` gave up where `display_album_artist()` fell back, so the two disagreed about the same file |
| Outputs appeared multiple times | MPD leaves a `plugin: dummy` placeholder in **every** partition an output isn't in — the `default` partition listed ten outputs when one was real |
| Moving an output was incomplete | Only a third of mikMPD's fix was ported; the missing part disables the output first, which is what stops `moveoutput` deadlocking the MPD server |
| Transport glyphs too small | Fitted, but read as specks — now a ratio test guards "looks right", not just "fits" |
| **Views didn't scroll** | Settings, Outputs, Partitions and Stats had no scrollable at all; Album/Artist/Playlist detail had their tall header outside the list's scrollable |

**246 offline tests (was 214), 15 live**, zero warnings under `-D warnings`,
green on Windows, Linux and macOS in CI.

### Verified against a real server (10.0.1.3, MPD 0.24.0)

All 15 live tests pass. One caveat worth remembering:
`live_musicbrainz_resolves_locally_artless_albums` reported **9 of 14** where
earlier runs got 14/14 — the TLS check in the same run got **HTTP 503** from
MusicBrainz, so that is near-certainly the service, not the matching code.
Re-run before treating it as a regression.

## Pending for 0.4.4 — done

The v0.4.3 `install.sh` defect (tarball layout not recognised, so the launcher
pointed at a binary that was never installed) was fixed in PR #25 and **is
called out in the v0.4.4 release notes**, with the instruction to re-run
`install.sh` from the new tarball. It was nearly missed — the first draft of
those notes didn't mention it, which is the failure this section exists to
prevent, so it only works if the notes are checked against it before
publishing.

### Still not verified

- **Windows** was never opened by hand this round; its CI tests pass and the
  binary builds, and the platform-specific code paths (window close, fonts)
  are shared with the two that were tested.
- **macOS signing.** The bundle is unsigned, so first launch needs
  `xattr -dr com.apple.quarantine`. Documented in the README and the release
  notes.

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

1. **Run the app and look at it.** Everything in 0.4.3 is unverified visually —
   that was the stated verification stance of the whole round and it is still
   outstanding. Check in particular: the light palette across *every* view (27
   files changed; there will be one someone forgot), the toasts not covering
   the player bar, and that **the window still closes** (see the
   `exit_on_close_request` note above).
2. **Type in every text box** and confirm Space doesn't pause. That is the
   acceptance test for the shortcuts and the one failure that would be
   genuinely annoying.
3. Run the live suite against the real server — none of it has run since these
   changes landed:
   ```bash
   WINRMPC_TEST_MPD=10.0.1.3:6600 WINRMPC_TEST_SNAPCAST=10.0.1.3:1705 \
     cargo test -- --ignored --test-threads=1
   ```
4. **Dispatch the release workflow manually** to get Windows and macOS builds
   to test — it publishes nothing. The macOS job has never run; that bundle is
   the least-proven thing on the branch.
5. Then release 0.4.3 via the `release` skill, which bumps the version, tags,
   and lets CI attach all three binaries.

