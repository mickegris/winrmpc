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

## Deliberately not planned (mobile-only or already covered elsewhere)

- **Listen on Phone** (`AVPlayer` streaming to the device, lock-screen
  transport) — winrmpc is the playback *server's* remote control, not a
  streaming client; N/A on desktop.
- **MPD stickers / star ratings** — mikMPD marks this "planned, not built" in
  its own `CLAUDE.md`; nothing to mirror yet.
- **First-run server setup polish / Bonjour permission prompt copy** — iOS
  `Info.plist`-specific; desktop's equivalent is just "no servers configured"
  in Settings, which winrmpc already has.

## Suggested sequencing

1. **Queue management** (#1) — small, high-value, fixes a real usability hole
   (there is currently no way to remove a single queued song).
2. **Server stats & diagnostics** (#6, #7) — small, self-contained, uses
   MpdClient methods (`stats()`, `update()`) that already exist and are
   already unit-tested at the parser level.
3. **Now Playing quick controls** (#10-12) — small, no new MPD surface.
4. **Recently Added / Played history** (#4, #5) — medium, extends the existing
   `recent_albums`/`push_recent` machinery rather than replacing it.
5. **Library album identity & multi-disc** (#2, #3) — the largest single
   piece; do last since it touches the most view code and benefits from the
   smaller plans (queue editing, stats) shipping first to keep PRs reviewable.
6. **Server discovery** (#9) and **Snapcast** (#8) — independent, net-new
   subsystems; can happen in any order relative to the rest, and in parallel
   with each other since they touch disjoint files.

Each linked plan is self-contained (own MPD commands / new types / view
sketch / implementation order / tests) so it can be picked up independently.
