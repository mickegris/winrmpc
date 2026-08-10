# Plan: Now Playing quick controls — crossfade, replay gain, outputs/partition shortcuts

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gaps #10-12). Three
small, independent additions to `views/now_playing.rs`; none require new MPD
protocol research beyond what's already in the codebase or the standard
protocol reference.

mikMPD (`README.md`): "Shows bitrate and audio format info, plus replay gain
and crossfade controls... Quick buttons reach outputs and partition
switching without leaving the screen."

## 1. Crossfade control

### Today

`MpdClient::set_crossfade(secs: u32)` already exists (`src/mpd/client.rs:162-164`,
sends `crossfade {secs}`) but has **zero call sites** in `src/ui/` — dead
code from the client's perspective.

### Changes

- **`message.rs`**: `SetCrossfade(u32)`.
- **`app.rs`**: `Message::SetCrossfade(secs) => self.mpd_cmd(move |c| async move { c.set_crossfade(secs).await })`.
- **Status already carries the current value**: `Status.crossfade: Option<u32>`
  is already parsed from `xfade` (`commands.rs:44`, `get_u32("xfade")`).
  Worth inheriting mikMPD's documented gotcha here (their own `CLAUDE.md`):
  **`xfade` is omitted from `status` entirely when crossfade is 0**, not
  reported as `xfade: 0` — winrmpc's `Option<u32>` already models that
  correctly (`None` = omitted), so the view just needs to treat
  `crossfade.unwrap_or(0)` as the displayed/default value rather than
  assuming `None` means "unknown".
- **`views/now_playing.rs`**: a small numeric field or +/- stepper near the
  existing mode toggles (repeat/random/single/consume), labeled "Crossfade"
  with the current seconds value, → `Message::SetCrossfade`.

## 2. Replay gain control

### Today

No `replay_gain_*` command exists anywhere in `client.rs` — this is genuinely
net-new, unlike crossfade.

### MPD protocol

- `replaygain_status` → current mode (`off`/`track`/`album`/`auto`).
- `replaygain_mode <mode>` → set it.

### Changes

- **`client.rs`**: `pub async fn replay_gain_status(&self) -> MpdResult<String>`
  and `pub async fn set_replay_gain_mode(&self, mode: &str) -> MpdResult<()>`
  (`self.cmd_ok(&format!("replaygain_mode {mode}"))`).
- **`message.rs`**: `SetReplayGainMode(String)`.
- **State**: fetch current mode on connect/refresh (fold into the existing
  status/mode refresh cycle rather than a separate poll) or lazily on
  entering Now Playing.
- **`views/now_playing.rs`**: a small picker (off/track/album/auto) next to
  the crossfade control.

## 3. Outputs/Partition quick access

### Today

`View::Outputs` and `View::Partitions` are full separate views, reachable
only via the sidebar — no shortcut from Now Playing.

### Changes

- **`views/now_playing.rs`**: two small buttons/links (e.g. near the mode
  toggles or in a compact header row) — "Outputs" → `Message::NavigateTo(View::Outputs)`,
  "Partitions" → `Message::NavigateTo(View::Partitions)`. This is pure
  navigation, reusing the existing `NavigateTo`/`view_history` stack
  documented in `CLAUDE.md`'s "Navigation" section — no new state.
- Optional richer version (skip for v1 unless requested): inline output
  toggle checkboxes directly in Now Playing, avoiding navigation entirely —
  matches mikMPD's "toggle audio outputs... straight from Now Playing" more
  literally, but is more view work for a feature most users reach for
  rarely enough that one extra navigation tap is a reasonable v1 scope cut.

## Implementation order

| # | Item | Size |
|---|------|------|
| 1 | Crossfade message + handler + `xfade`-defaults-to-0 verification + view control | XS |
| 2 | `replay_gain_status`/`set_replay_gain_mode` client methods + tests | XS |
| 3 | Replay gain message + handler + view picker | XS |
| 4 | Outputs/Partitions quick-nav buttons | XS |

## Testing

- Unit: command-string formation for `set_replay_gain_mode` (mirrors the
  existing `escape`/command-format test style already used for other
  `client.rs` methods — mode is a fixed enum-like string, not user input, so
  no escaping needed, just confirm the literal command string).
- Manual QA: crossfade field reflects `status`'s current value after an
  external client changes it (via the normal 500ms poll); replay gain mode
  round-trips; Outputs/Partitions buttons navigate correctly and `GoBack`
  returns to Now Playing.
