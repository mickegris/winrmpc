# Plan: Performance + accuracy fixes from the post-parity code review

Status: **all 4 items implemented**, in the same commit as
[`review-fixes-correctness.md`](review-fixes-correctness.md) (items 3-4
here directly interact with that plan's item 3, and the plan itself said to
do them together). Same source: review of the seven parity commits on
`release/v0.4.1` (`b1329b3`..`b820700`). This app's stated priority is
performance (not energy — see the overview doc's exclusion note).

**Implementation notes**: Item 1's partial-failure question was resolved in
favor of `command_list`'s native stop-at-first-error behavior (tracks
already applied before a failure stay queued; nothing after it is
attempted) rather than adding a fallback to the old per-track loop — the
plan flagged this as needing a deliberate choice, and a silent
per-track-swallow-errors loop defeats the entire point of surfacing a
mid-album problem instead of quietly playing a partial, wrong-order
selection. `add_all`'s command-string formation was split into a pure
`build_add_commands` helper so it's unit-testable without a live
connection, matching this module's existing `escape()`-test convention.

---

## 1. `PlayAlbum`/`QueueAlbum` send one `add` per track — the exact pattern mikMPD warns about

**Severity: medium.** Introduced by `284cd17` (round 6), as a *deliberate*
fix for a worse bug — but the replacement reintroduced a known anti-pattern.

### Background

Round 6 changed `PlayAlbum`/`QueueAlbum` from `find_add("Album", &name)` to
carrying explicit song URIs. That change was **necessary and correct**: once
album identity became the collapsed *base* name, a tag-exact
`find_add("Album", "Blast from the Past")` matches zero tracks on an album
whose real tags are `"Blast from the Past [Disc 1]/[Disc 2]"`. So the
regression it fixed was real.

But the replacement loops one round trip per track (`app.rs:679-701`):

```rust
client.clear().await.ok();
for uri in &uris {
    client.add(uri).await.ok();     // N sequential round trips
}
client.play().await.ok();
```

mikMPD's `CLAUDE.md` documents this precise shape as a past incident:

> **Bulk enqueue is server-side.** … the old path fetched every song then
> sent one `add` per track, so "Play All" on a large artist or genre was
> thousands of sequential commands that **starved the poll for minutes**.

winrmpc's blast radius is smaller than mikMPD's was — this is one album
(tens of tracks), not a whole artist or genre — so it's tens of round trips,
not thousands. But it's on the interactive path (the user pressed "Play
All"), each `add` takes the shared `MpdClient` connection mutex, and the
500ms status poll contends for that same mutex the entire time.

### Fix

`MpdConnection::command_list` **already exists** (`protocol.rs:53-89`,
`command_list_ok_begin` / `command_list_end`) but is **not exposed on
`MpdClient`** (verified: no `command_list` reference in `client.rs`). Add a
thin wrapper and use it:

```rust
// client.rs
pub async fn add_all(&self, uris: &[String]) -> MpdResult<()> {
    if uris.is_empty() { return Ok(()); }
    let cmds: Vec<String> = uris
        .iter()
        .map(|u| format!("add \"{}\"", Self::escape(u)))
        .collect();
    let refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    let mut guard = self.conn.lock().await;
    let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
    conn.command_list(&refs).await.map(|_| ())
}
```

One round trip, one mutex acquisition, and MPD applies it atomically. Note
`command_list` is also the right primitive for the CD batch-probe path
(`CdProbe`) documented in `CLAUDE.md`, and for `playlist_add` of a multi-song
selection from the Add-to-Playlist picker — worth checking those call sites
while in here.

**Care needed:** `command_list` returns `Err` on the first ACK, and unlike
the current `.ok()`-per-add loop it will abort the remaining adds. That's
arguably better (atomic), but it changes behavior when one URI in an album is
stale/unreadable — today the other tracks still queue. Decide deliberately;
if partial success matters, `command_list_begin` (without `_ok`) or catching
the ACK and falling back to the per-track loop are both options.

---

## 2. Full `Song` clone on every 500ms status poll

**Severity: low-medium (steady-state allocation churn).**
Introduced by `2f871d8` (round 4).

`Message::StatusUpdated` (`app.rs:493`) clones the entire current song purely
to satisfy the borrow checker before ticking the play recorder:

```rust
if let Some(song) = self.current_song.clone() {
    if !song.file.starts_with("cdda://") {
        …
        if self.play_recorder.tick(&song.file, is_playing, elapsed, song.duration_secs) {
```

`Song` (`types.rs:67-87`) is ~15 `Option<String>`s **plus a
`HashMap<String, Vec<String>>` of unknown tags**. This clones all of it twice
a second, for the entire lifetime of the app, and then uses exactly two
fields (`file`, `duration_secs`).

### Fix

Extract the two needed values before touching `&mut self`, so nothing is
cloned but one short `String` (or clone nothing, by restructuring):

```rust
let recorder_input = self.current_song.as_ref().and_then(|s| {
    (!s.file.starts_with("cdda://")).then(|| (s.file.clone(), s.duration_secs))
});
if let Some((file, duration)) = recorder_input {
    if self.play_recorder.tick(&file, is_playing, elapsed, duration) { … }
}
```

Better still: give `PlayRecorder::tick` the fields it needs and have the
commit path build `RecentlyPlayedEntry` from `self.current_song.as_ref()`
inside the `if` — the entry is only constructed on an actual commit
(roughly once per 30s of playback), so *that* clone is genuinely rare and
fine to keep.

---

## 3. Snapcast reopens a TCP connection on every view visit — and the comment says otherwise

**Severity: low (waste), but the comment is actively misleading.**
Introduced by `b820700`.

`on_view_enter` (`app.rs:2624-2631`) carries this comment:

```rust
// connect() is a no-op-ish cheap call if a prior
// connection attempt already succeeded — the
// client's internal Option is simply overwritten;
```

That is **wrong**. `SnapcastClient::connect` (`client.rs:28-32`)
unconditionally does `SnapcastConnection::connect(&self.addr)` — a fresh
`TcpStream::connect` — and *then* overwrites the `Option`, dropping (closing)
the previous socket. So every visit to the Snapcast view tears down a working
connection and opens a new one; if the server is unreachable, every visit
re-pays the full connect timeout before showing the error.

`CLAUDE.md`'s Snapcast section repeats the same incorrect claim ("connect
lazily on first enter, keep alive across later visits") and must be corrected
alongside the code.

### Fix

Make reconnection conditional — try `get_status()` first and only
`connect()` on `NotConnected`/IO failure, or add an `is_connected()` probe
mirroring `MpdClient::is_connected()`:

```rust
if !client.is_connected().await {
    client.connect().await?;
}
client.get_status().await
```

Then fix the comment and the `CLAUDE.md` paragraph to describe what the code
actually does. (Item #3 of the correctness plan — resetting
`snapcast_client` on `SwitchServer` — interacts with this; do them together
so the "when do we rebuild vs. reuse" logic is written once.)

---

## 4. `decode_snap_groups` / `decode_snap_streams` each deep-clone and re-parse the whole status tree

**Severity: low.** Introduced by `b820700`.

`SnapcastClient::get_status` (`client.rs:47`) calls both decoders on the same
`server` value, and each one does
`serde_json::from_value::<RawServer>(server.clone())` (`types.rs`) — so the
entire group/client tree is **cloned and fully parsed twice** per poll, every
2 seconds while the view is open.

### Fix

Parse once into `RawServer` and split:

```rust
pub fn decode_snap_status(server: &Value) -> (Vec<SnapGroup>, Vec<SnapStream>) {
    serde_json::from_value::<RawServer>(server.clone())
        .map(|r| (r.groups.into_iter().map(SnapGroup::from).collect(), r.streams))
        .unwrap_or_default()
}
```

Keep the two existing functions as thin wrappers if the unit tests read
better that way, but have `get_status` call the combined one. (Taking
`server` by value from `get_status` would drop the remaining clone entirely,
since the `Value` isn't needed afterwards.)

---

## Suggested order

| # | Item | Size | Notes |
|---|------|------|---|
| 3 | Snapcast conditional reconnect + fix comment/CLAUDE.md | S | Do with correctness-plan #3 |
| 4 | Single-parse Snapcast status decode | XS | Same file, same round |
| 2 | Stop cloning `Song` every tick | XS | Self-contained |
| 1 | `add_all` via `command_list` | S | Decide the partial-failure semantics first |

## Testing

- **Unit**: `add_all`'s generated command list (escaping + one `add` line per
  URI, empty input → no-op) mirroring the existing `escape`/command-string
  test style; `decode_snap_status` returning both halves from the existing
  `Server.GetStatus` fixture (reuse the fixture already in
  `snapcast/types.rs`).
- **Manual QA**: "Play All" on a 20+ track album — the queue fills in one
  step and the transport stays responsive throughout (previously the poll
  contended with N sequential adds); open/leave/reopen the Snapcast view
  repeatedly and confirm via the Log view that it isn't reconnecting each
  time.
- **No test needed** for the `Song`-clone change — it's a pure refactor with
  identical observable behavior; the existing `PlayRecorder` tests already
  cover the logic it feeds.
