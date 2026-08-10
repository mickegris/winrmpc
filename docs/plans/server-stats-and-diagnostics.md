# Plan: Server Statistics view + timed MPD command log

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gaps #6/#7). Two
small, independent features bundled because both live under a "server info"
umbrella and both build on existing, already-tested machinery.

## Part A — Server Statistics view

mikMPD (`README.md`): "Library totals (songs, albums, artists, total playing
time) and server uptime, playtime and last database update, from the More
tab. Trigger a database update or a full rescan from the same screen; the
figures refresh themselves when the scan finishes."

### Today

Everything server-side already exists and is already unit-tested at the
parser level:
- `MpdClient::stats() -> MpdResult<Stats>` (`src/mpd/client.rs:184`)
- `Stats { uptime, playtime, artists, albums, songs, db_playtime, db_update }`
  (`src/mpd/types.rs:224-232`)
- `MpdClient::update(path: Option<&str>) -> MpdResult<u32>` (`client.rs:290`)
  — `update` with no path is a full rescan trigger; per-path is a partial
  update (MPD's `rescan` command, distinct from `update`, may also be worth
  exposing — check the protocol reference for the `force`/rescan distinction
  mikMPD's own `CLAUDE.md` points at).

None of this is wired to any view — `stats()` has zero call sites in
`src/ui/`.

### UI

- **New `View::ServerStats`**, sidebar entry (near Settings/Log, matching
  mikMPD's "More" tab placement — winrmpc's sidebar is the closest
  equivalent to mikMPD's More tab for this kind of secondary screen).
- Render `Stats` fields as labeled rows: song/album/artist counts, uptime
  (format via the existing `format_duration`-style helper in `types.rs`),
  playtime, DB playtime, last DB update (convert `db_update`'s unix
  timestamp to a readable date via `chrono`, already a dependency).
- Two buttons: **Update Database** (`client.update(None)`) and, if
  distinguishing update-vs-rescan is worth it, a second **Full Rescan**
  action. Trigger via a message that fires the command and then re-polls
  `stats()` — mikMPD polls until `db_update` stops changing; simplest port
  is to just re-fetch `stats()` on the next few `Tick`s (the existing 500ms
  poll already gives near-immediate feedback) rather than building a new
  polling loop.
- `on_view_enter(View::ServerStats)` fires `stats()` → `Message::StatsLoaded(Stats)`.

### Implementation

| # | Item | Size |
|---|------|------|
| 1 | `View::ServerStats` + sidebar entry + `on_view_enter` | S |
| 2 | `views/server_stats.rs` (new, render `Stats`) | S |
| 3 | Update/rescan buttons + refresh-after-trigger | S |

No new MPD surface, no new parsing — this is pure UI wiring on top of
existing, tested code.

### Testing

- **Unit**: none needed beyond what already exists — `Stats` parsing is
  already covered (`CLAUDE.md`'s `mpd/commands.rs` test bullet lists
  `parse_stats`), and this section adds no new pure logic, only view
  wiring around an already-tested parser and an already-existing client
  method.
- **Manual QA**: open Server Statistics, confirm counts match a known
  library (cross-check against MPD's own `mpc stats` or a manual `stats`
  command over the protocol); trigger Update, confirm `db_update`'s
  timestamp changes once the scan completes and the view reflects it
  without a manual refresh; trigger against a server with nothing to
  update (figures stay stable, no error surfaced).

## Part B — Timed, copyable MPD command log

mikMPD (`CLAUDE.md`, "Every MPD command is logged"): a 250-entry ring buffer
of `(time, command, duration, outcome)`, **off by default**, read from a
"Diagnostics" screen with copy-to-clipboard, commands slower than 2s
highlighted. Reason given: "the daemon has hung hard enough to need `kill -9`
with nothing on the client recording what it was doing."

### Today

`src/ui/app.rs`'s Log view (`docs/plans/enhancements.md` §2, shipped) already
surfaces MPD command activity via `tracing::info!` in `MpdClient::cmd`
(`client.rs:54-73`) with an MPD-only filter toggle
(`Message::LogToggleMpdOnly`, `log_show_mpd_only`). What it's **missing**
relative to mikMPD's version:
- No per-command **duration** — `cmd()` doesn't measure elapsed time.
- No slow-command highlighting.
- Shares the general 500-entry ring buffer (`logger.rs`) rather than a
  dedicated, larger, opt-in-off buffer — acceptable difference, not worth
  forking into two systems, but the missing duration is a real gap since
  it's the entire point of mikMPD's feature (diagnosing hangs).
- No dedicated copy-to-clipboard button (confirm: check if the Log view
  already has one generically — if so this is already covered and only the
  duration/highlight parts remain a gap).

### Changes

1. **`MpdClient::cmd`** (`client.rs:54-73`): wrap the `conn.command(cmd)`
   call with `std::time::Instant::now()` timing, include the duration in the
   log line, e.g.:
   ```rust
   let started = std::time::Instant::now();
   let result = conn.command(cmd).await;
   let elapsed = started.elapsed();
   match &result {
       Ok(_) if !quiet => tracing::info!("→ {verb} ({elapsed:?})"),
       // ...
   }
   ```
   Also log **quiet** commands when they're slow (`elapsed > Duration::from_secs(2)`)
   even though they're normally suppressed — a hung `status` poll is exactly
   the failure mode mikMPD built this for, and it's currently invisible
   because `status`/`currentsong`/etc. are in the `quiet` list.
2. **`logger.rs`**: if `LogEntry` doesn't already carry a free-form message
   that can embed the duration, check its shape — if it's just
   `(timestamp, level, target, message)`, the duration-in-message approach
   above needs no struct change. If highlighting slow commands in the UI
   (not just the text) is wanted, `LogEntry` would need a `slow: bool` or
   the view can regex/parse `(\d+\.\d+m?s)` out of the message at render
   time — prefer a small explicit field if practical, since string-parsing
   the log for a duration to color it feels fragile.
3. **`views/log.rs`**: color entries whose embedded duration exceeds 2s
   (`AppColors` presumably has a warning/error color already — reuse it).

### Implementation order

| # | Item | Size |
|---|------|------|
| 1 | Timing in `MpdClient::cmd` + log slow "quiet" commands | S |
| 2 | Slow-command visual highlight in `views/log.rs` | S |
| 3 | Confirm/add copy-to-clipboard (skip if already present) | XS |

### Testing

- No new pure-logic surface beyond what's already covered — this is
  instrumentation, not parsing. If a `slow: bool` field is added to
  `LogEntry`, a one-line test on the threshold comparison is easy but
  optional.
- Manual QA: watch the Log view during a normal session (durations show, all
  short); artificially slow the server or watch for a real slow query
  (e.g. an unfiltered `find`) to confirm the highlight triggers.
