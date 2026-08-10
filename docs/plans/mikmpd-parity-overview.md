# Plan: mikMPD feature-parity gap analysis

Status: proposed — no code changes yet. This is the index; each substantial gap
has its own plan file in this directory. Comparison is against
`../mikMPD` (`/root/mikMPD` in this environment, `C:\Users\mikae\mikMPD` on the
dev machine) as of its `CLAUDE.md`/`README.md` on 2026-08-10.

winrmpc is desktop/iced, mikMPD is iOS/SwiftUI — features that are inherently
mobile (lock-screen `MPRemoteCommandCenter`, `AVAudioSession` background audio,
"Listen on Phone" streaming *to the device itself*) are **out of scope** and
excluded below. Everything else in mikMPD's feature list is a fair target.

## Already at parity (verified in code, not just docs)

- Now Playing transport, volume, mode toggles, progress bar, art
- Artist/Album/Genre browsing, File browser, Search (song-only — see gap below)
- MusicBrainz + Cover Art Archive art, Wikipedia artist/album bios
- CD playback (`cdda://`)
- Radio (built-in Swedish stations + custom)
- Partitions + Outputs, multi-server (`servers: Vec<MpdServer>`)
- Stored playlists (`docs/plans/playlists.md`, shipped in `a776999`)
- Synced lyrics via LRCLIB (`docs/plans/enhancements.md` §3, shipped)
- MPD command activity in the Log view with an MPD-only filter
  (`docs/plans/enhancements.md` §2, shipped) — though see the diagnostics gap
  below for the parts of mikMPD's version this one doesn't yet match
- Queue shuffle-in-place (`Message::QueueShuffle` → `client.shuffle()`)

## Gaps (this plan set)

| # | Gap | mikMPD feature | Plan file | Size |
|---|---|---|---|---|
| 1 | **Queue can't be edited** | drag reorder, swipe-delete, "Add Next" | [`queue-management.md`](queue-management.md) | S |
| 2 | **Album list is name-only, no grid, no multi-disc, no artist disambiguation** | grid/list toggle, "N discs" collapsing, per-artist album rows, art thumbnails | [`library-album-identity-and-multidisc.md`](library-album-identity-and-multidisc.md) | L |
| 3 | **Search is song-only, no batch actions** | Artists/Albums/Songs sections, multi-select | folded into #2 | — |
| 4 | **No "Recently Added" library section** | last-30-days new albums | [`recently-added-and-played-history.md`](recently-added-and-played-history.md) | M |
| 5 | **"Recently played" is 8 albums in a sidebar, not real history** | 30-day/100-entry per-track history sheet, clock button in Now Playing | [`recently-added-and-played-history.md`](recently-added-and-played-history.md) | M |
| 6 | **No Server Statistics screen** | song/album/artist counts, uptime, playtime, trigger update/rescan | [`server-stats-and-diagnostics.md`](server-stats-and-diagnostics.md) | S |
| 7 | **No timed/copyable MPD command log** | 250-entry ring buffer with per-command duration, slow-command highlight, copy-to-clipboard, opt-in toggle | [`server-stats-and-diagnostics.md`](server-stats-and-diagnostics.md) | S |
| 8 | **No Snapcast control** | per-client volume/mute, group management | [`snapcast-control.md`](snapcast-control.md) | L |
| 9 | **No LAN server discovery** | Bonjour/Zeroconf `_mpd._tcp` browse in the Settings/Add-Server flow | [`server-discovery.md`](server-discovery.md) | M |
| 10 | **Crossfade has a client method but no UI** | crossfade slider/field in Now Playing | [`now-playing-quick-controls.md`](now-playing-quick-controls.md) | XS |
| 11 | **No replay gain control** | replay gain mode picker | [`now-playing-quick-controls.md`](now-playing-quick-controls.md) | XS |
| 12 | **No quick Outputs/Partition access from Now Playing** | buttons on the Now Playing screen | [`now-playing-quick-controls.md`](now-playing-quick-controls.md) | XS |
| 13 | **Album art fetch order is cover-file → tag, not tag → cover-file** | `readpicture` (tag) tried first, `albumart` (cover file) second — winrmpc has this backwards today | [`art-wikipedia-fetch-order-and-caching.md`](art-wikipedia-fetch-order-and-caching.md) | S |
| 14 | **Wikipedia bios never persist; no shared MusicBrainz throttle/concurrency cap; weaker title-match than mikMPD** | disk-cached bios, `ArtFetchGate` (4 concurrent), global `MusicBrainzThrottle`, title-token-overlap match preferred over extract-substring | [`art-wikipedia-fetch-order-and-caching.md`](art-wikipedia-fetch-order-and-caching.md) | M |

Gaps #13/#14 aren't missing *features* — winrmpc already has tag art, cover-file
art, internet fallback, and Wikipedia bios, same as mikMPD. They're
**performance and correctness gaps in how those existing sources are
fetched, ordered, and cached**, found by reading the actual fetch code
(`src/mpd/client.rs`, `src/art/musicbrainz.rs`, `src/ui/app.rs`) rather than
comparing feature lists — see that plan for the concrete evidence (line
numbers, current vs. desired order).

## Deliberately not planned (mobile-only, energy-focused, or already covered elsewhere)

- **Listen on Phone** (`AVPlayer` streaming to the device, lock-screen
  transport) — winrmpc is the playback *server's* remote control, not a
  streaming client; N/A on desktop.
- **MPD stickers / star ratings** — mikMPD marks this "planned, not built" in
  its own `CLAUDE.md`; nothing to mirror yet.
- **First-run server setup polish / Bonjour permission prompt copy** — iOS
  `Info.plist`-specific; desktop's equivalent is just "no servers configured"
  in Settings, which winrmpc already has.
- **Anything from mikMPD's `energy-optimization.md`** (poll-interval
  throttling while paused, stopping the 0.1s display timer when idle,
  suspending background timers) — that plan exists to save **phone battery**
  under iOS background-execution limits. winrmpc is a desktop app with no
  battery constraint driving its polling design; none of that plan's
  reasoning applies here and nothing from it is mirrored anywhere in this
  set. Where this plan set *does* touch things that look adjacent (the
  shared MusicBrainz throttle and concurrency gate in gap #14), the
  motivation is strictly network-politeness and reducing redundant HTTP
  calls/connections — a performance and correctness concern, not a power
  one — never "wake up less to save the battery."

## Suggested sequencing

1. **Queue management** (#1) — small, high-value, fixes a real usability hole
   (there is currently no way to remove a single queued song).
2. **Server stats & diagnostics** (#6, #7) — small, self-contained, uses
   MpdClient methods (`stats()`, `update()`) that already exist and are
   already unit-tested at the parser level.
3. **Now Playing quick controls** (#10-12) — small, no new MPD surface.
4. **Recently Added / Played history** (#4, #5) — medium, extends the existing
   `recent_albums`/`push_recent` machinery rather than replacing it.
5. **Art/Wikipedia fetch order and caching** (#13, #14) — small-to-medium,
   fixes existing behavior rather than adding new surface; land items 1-3 of
   that plan (fetch-order swap, client reuse, concurrency/rate gate)
   **before or alongside** item 6 below, since the grid view is exactly the
   change that turns today's latent concurrency-throttle gap into a visible
   problem.
6. **Library album identity & multi-disc** (#2, #3) — the largest single
   piece; do last since it touches the most view code and benefits from the
   smaller plans (queue editing, stats, art throttling) shipping first to
   keep PRs reviewable.
7. **Server discovery** (#9) and **Snapcast** (#8) — independent, net-new
   subsystems; can happen in any order relative to the rest, and in parallel
   with each other since they touch disjoint files.

Each linked plan is self-contained (own MPD commands / new types / view
sketch / implementation order / tests) so it can be picked up independently.

## Testing strategy across this plan set

Every linked plan has its own **Testing** section scoped to that feature;
this is the cross-cutting summary of *what kinds* of tests this whole effort
needs, following the conventions already established in this codebase's
`CLAUDE.md` (inline `#[cfg(test)] mod tests`, I/O-free, because this is a
binary crate with no integration-test access to private items):

- **Pure-logic unit tests are the primary tool**, same as the 50 tests that
  already exist. Every plan above is written to extract new logic into pure,
  argument-in/value-out functions specifically so it's unit-testable without
  a live MPD/HTTP server — e.g. `parse_grouped_values`, `album_base_and_disc`,
  `group_albums_by_artist`, `title_matches`, `strip_edition_qualifier`,
  `PlayRecorder`/`prune_recently_played`, JSON-RPC request/notification
  discrimination for Snapcast, command-string formation for every new
  `MpdClient`/Snapcast-client method (mirroring the existing `escape()`-style
  format tests). Each plan's own Testing section enumerates the specific
  cases; add them to the existing `#[cfg(test)]` module in whichever file
  gains the new pure function, next to the tests that already cover that
  file (per `CLAUDE.md`'s per-file test breakdown).
- **Concurrency/timing tests** are new territory for this codebase (today's
  50 tests are all synchronous pure functions) — needed specifically for
  gap #14's `MusicBrainzThrottle`/`ArtFetchGate` (`tokio::test` +
  `tokio::join!` to prove throttling is global, not per-call-chain) and
  optionally for `PlayRecorder`'s tick-based accumulation if it's exercised
  async rather than as a plain struct method. Keep these narrowly scoped to
  the new concurrency primitive itself, not the surrounding view/app code.
- **No new integration or UI-automation tests are planned or needed.** This
  matches the existing project posture (`CLAUDE.md`: the *only* documented
  gap in current coverage is the `protocol.rs` read loops, which would need
  a mock `AsyncRead`/`AsyncWrite` — none of the plans above touch that
  layer). Every plan's view-level work (new screens, buttons, layout) is
  manual-QA-only, called out explicitly in each plan's Testing section as a
  checklist — consistent with how this project already treats UI code
  (verified by running the app, not by an automated UI test harness).
- **Fixture-based tests** are called for exactly once: Snapcast's
  `Server.GetStatus` JSON response, decoded via a captured real-response
  fixture (same idea as mikMPD's own fixture test for the same protocol) —
  everything else in this plan set works from synthetic inline test data.
- When a plan's Testing section says "manual QA" for something that could
  plausibly be made a pure function instead (e.g. a view-layout decision),
  that's a deliberate call, not an oversight — trust each plan's own
  judgment on the unit-vs-manual split; this section is about the aggregate
  pattern, not a mandate to maximize test count.
