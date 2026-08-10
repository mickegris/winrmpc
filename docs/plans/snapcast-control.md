# Plan: Snapcast multiroom control

Status: proposed — no code changes yet. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gap #8). Net-new
subsystem, independent of the rest of the app's MPD connection.

Direct port of `../mikMPD/plans/snapcast-control.md`'s protocol research,
adapted from mikMPD's raw-POSIX-socket/actor design to winrmpc's
tokio-async shape.

## Protocol (unchanged from mikMPD's research — same wire format regardless of client)

snapserver exposes JSON-RPC 2.0 over **raw TCP, port 1705 by default,
newline-delimited** — not HTTP. Key calls:

- `Server.GetStatus` → `{ result: { server: { groups: [...], streams: [...] } } }`.
  Groups: `id`, `name`, `muted`, `stream_id`, `clients: [...]`. Clients:
  `id`, `connected`, `host.name`, `config.name`, `config.volume.{percent,muted}`,
  `config.latency`.
- `Client.SetVolume` `{id, volume: {percent, muted}}`
- `Client.SetLatency`, `Client.SetName`, `Group.SetMute`, `Group.SetStream`,
  `Group.SetClients` (move clients between groups), `Server.DeleteClient`.
- Push notifications (`Client.OnVolumeChanged`, `Group.OnMute`,
  `Server.OnUpdate`, etc.) arrive **interleaved with RPC responses** on the
  same connection, distinguished by presence/absence of a matching `"id"`.

## Architecture (tokio-shaped, mirrors `src/mpd/protocol.rs`)

winrmpc already has exactly this shape for MPD — a raw `TcpStream` read/write
split with line-based response parsing (`src/mpd/protocol.rs`) wrapped by a
typed command layer (`src/mpd/client.rs`). Snapcast gets the same treatment
as a **sibling module**, not bolted onto `MpdClient`:

- **`src/snapcast/mod.rs`** — re-exports.
- **`src/snapcast/protocol.rs`** — `SnapcastConnection` over `tokio::net::TcpStream`,
  `tokio::io::BufReader`/`AsyncBufReadExt::lines()` for the newline-delimited
  JSON, matching `protocol.rs`'s existing pattern (including the **EOF
  guard** convention already documented in `CLAUDE.md` — an empty line means
  connection closed, return an error rather than hang). Since responses and
  notifications interleave, the read loop for a `request()` call must **skip
  lines that don't match the outstanding id** and route them to a
  notification channel instead (use `flume`, already a dependency, for the
  notification stream — same crate winrmpc already uses for other
  channel needs per `CLAUDE.md`'s Tech Stack list).
- **`src/snapcast/client.rs`** — `SnapcastClient` (`Arc<Mutex<...>>`, same
  clone-cheap shape as `MpdClient`): `get_status()`, `set_volume(client_id, percent, muted)`,
  `set_group_mute(group_id, muted)`, `set_group_stream(group_id, stream_id)`,
  `set_client_name`, `set_client_latency`, `move_client(client_id, from_group, to_group)`
  (→ `Group.SetClients` on both groups), `delete_client(client_id)`.
- **`src/snapcast/types.rs`** — `SnapClient { id, connected, host_name, name, volume, muted, latency }`,
  `SnapGroup { id, name, muted, stream_id, clients: Vec<SnapClient> }`,
  `SnapStream { id, status }`. Deserialize via `serde_json` (already a
  dependency) directly from the `Server.GetStatus` result's `server` key —
  add `#[derive(Deserialize)]` structs rather than hand-rolling a parser like
  the MPD side needs to (MPD's wire format is line pairs; Snapcast's is
  already JSON, so `serde_json` does the parsing for free — no line-parser
  needed here, unlike every other MPD-adjacent module in this codebase).

## Configuration (per server, mirrors `MpdServer`)

`src/config/settings.rs`, extend `MpdServer`:
```rust
#[serde(default)]
pub snapcast_host: Option<String>,   // None/empty => use `host` (same MPD box)
#[serde(default)]
pub snapcast_port: Option<u16>,      // defaults to 1705 when None
```
Both `#[serde(default)]` per the existing migration rule in `CLAUDE.md`
("All new optional fields must carry `#[serde(default)]` so existing config
files still load"). Edited in the Settings view's per-server form, alongside
`host`/`port`/`password`.

## UI

- **Sidebar entry** "Snapcast" (near Outputs/Partitions).
- **`View::Snapcast`**: one section per group (name/stream + mute toggle),
  rows per client (connected dot, name, volume slider, mute button). Volume
  slider follows the same "local state while dragging, commit on release"
  pattern the existing player-bar volume slider already uses (`widgets/player_bar.rs`
  — confirm the exact mechanism there and reuse it) so a 2s poll doesn't
  fight the user's drag.
- **Poll**: a `Subscription::every(2s)` while `View::Snapcast` is active
  (mirrors the app's existing Tick/ConnectionTick subscription-swap pattern
  in `app.rs` rather than a permanent background poll) → `Snapcast::GetStatus`
  → `Message::SnapcastStatusLoaded`.
- **Connection lifecycle**: connect lazily when the view is entered
  (`on_view_enter(View::Snapcast)`), disconnect on leaving — view-scoped,
  not app-wide, since Snapcast may be absent/down while MPD is fine (same
  reasoning mikMPD's plan gives for a separate store).
- Empty/unreachable state: a simple "Snapcast unreachable at host:port" — no
  error alert, the view itself explains (matches this app's existing
  low-friction error surfacing via `last_error`/inline messages rather than
  modal alerts).

## Phasing (same split mikMPD's plan used, still sound advice here)

1. **Read-only**: connect, `Server.GetStatus`, render groups/clients/volumes.
   Proves the protocol layer.
2. **Control**: volume slider commit, client/group mute, optimistic update
   + poll-skip-while-dragging.
3. **Later, explicitly out of scope for v1**: notification-driven live
   updates (poll is enough to start), rename, latency editing, move-between-
   groups, stream switching, delete disconnected clients.

## Implementation order

| # | Item | Size |
|---|------|------|
| 1 | `snapcast::protocol` (TCP, newline JSON, request/notification split) | M |
| 2 | `snapcast::types` (serde structs) + `snapcast::client` (`get_status`) | S |
| 3 | `MpdServer.snapcast_host/port` config fields | XS |
| 4 | `View::Snapcast` read-only render + poll subscription | M |
| 5 | `Client.SetVolume` / `Group.SetMute` controls | S |

## Testing

- **Unit** (I/O-free): `Server.GetStatus` JSON fixture → `SnapGroup`/`SnapClient`
  deserialization (capture a real response shape from the protocol doc or a
  test server, same "fixture test" approach mikMPD's plan recommends);
  request-id/notification discrimination given a synthetic interleaved line
  sequence (pure function over `&[String]`, no real socket needed — same
  spirit as this codebase's existing `pairs_to_map`/`split_groups` tests);
  volume percent clamping (0-100).
- **Manual QA**: point at a real Snapcast server, confirm groups/clients
  render, drag a volume slider without it snapping back mid-drag, mute a
  group, verify against `snapclient`/the reference web UI.
