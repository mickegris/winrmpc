> **REMOVED (0.4.1).** LAN server discovery shipped in `b820700` and was
> removed again at the user's request: the `mdns-sd` dependency, the
> `src/discovery/` module, the `Nearby Servers` section in Settings and the
> four `Message` variants are all gone. Manual server entry — which always
> worked and was the only path that saved a profile anyway — is now the only
> way to add a server. This document is kept as the record of what was built
> and why, not as a description of current behaviour.

---

# Plan: LAN MPD server discovery (mDNS/Zeroconf)

Status: **implemented**. Part of the mikMPD parity set (see
[`mikmpd-parity-overview.md`](mikmpd-parity-overview.md), gap #9).

**Implementation notes**: `mdns-sd` resolved cleanly (pinned to `"0.20"`,
the current major at implementation time). Its API is thread-based with a
`flume::Receiver<ServiceEvent>` supporting `recv_async()`, which made the
iced `Subscription` bridge straightforward — `iced::stream::channel` wraps
an async loop calling `receiver.recv_async()` under a `tokio::time::timeout`
against a 10s deadline, so no manual thread/channel bridging was needed
beyond what the plan already anticipated. `Subscription::run_with_id` (not
`run`, which requires a plain `fn` pointer with no captured state) is what
actually wires a pre-built `Stream` value into the subscription batch. The
stream has no separate "finished" event of its own, so `Message::StartDiscovery`
fires a companion 11s timer (`DiscoveryFinished`) to clear the "Searching…"
state 1s past the stream's own internal deadline.
Real mDNS discovery against a live MPD instance couldn't be manually tested
in this environment (no real network/MPD server available) — the pure
`instance_name_from_fullname` transform is unit-tested, everything else
here still needs the "point at a real server on the LAN" pass the plan's
own Testing section calls for.

Adapted from `../mikMPD/plans/server-discovery.md`. MPD advertises itself as
`_mpd._tcp` over Zeroconf/Bonjour when built with zeroconf support and
`zeroconf_enabled "yes"` (default in most distro packages) — this is a
network-protocol fact, not an iOS one, so the discovery target is identical;
only the platform API for browsing changes.

## Approach

mikMPD uses Apple's `Network.framework` (`NWBrowser`). winrmpc is
cross-platform Rust, so the equivalent is an mDNS-SD crate. `mdns-sd` (pure
Rust, no OS-level Bonjour dependency, works on Windows/Linux/macOS — matters
since this app explicitly targets all three per `CLAUDE.md`'s Overview) is
the natural fit; confirm it's still maintained and its feature set (browse +
resolve to host:port) before locking it in, but it's the standard choice in
the Rust ecosystem for this.

## New module

**`src/discovery/mod.rs`** (new top-level module, added to `main.rs`'s `mod`
list alongside `mpd`, `art`, `lyrics`, `store`, etc.):

```rust
pub struct DiscoveredServer {
    pub name: String,
    pub host: String,
    pub port: u16,
}

pub struct DiscoveryService { /* wraps the mdns-sd daemon/browser handle */ }

impl DiscoveryService {
    pub fn start() -> (Self, flume::Receiver<DiscoveredServer>);
    pub fn stop(self);
}
```

- Browse for service type `_mpd._tcp.local.`.
- `flume` (already a dependency) channel delivers each resolved server as it
  appears — the receiving side (`app.rs`) turns channel messages into
  `Message::ServerDiscovered(DiscoveredServer)` via a `Subscription` (iced
  0.13 subscriptions can wrap an arbitrary stream; if `mdns-sd`'s API is
  callback-based rather than async-stream-based, bridge it through the
  `flume` channel and an `iced::stream::channel`-style subscription — check
  current iced 0.13 subscription APIs for the exact wiring).
- De-dupe by service name (a server on multiple interfaces yields multiple
  resolves) — a `HashSet<String>` of seen names in the subscription state.
- Time-box the scan (mikMPD uses 10s); after the window, `stop()` — avoid
  burning CPU/network on a browse that runs for the app's whole lifetime.
  Re-scan is a manual "Rescan" button, not automatic.

## UI (Settings view, server-add flow)

- A **"Nearby Servers"** section above the manual add-server form (wherever
  `AddServer`/the server list currently lives in the Settings view per
  `CLAUDE.md`'s "Server switching" section).
- Each discovered row: name + `host:port`, a button that **pre-fills** the
  manual add-server form fields (name defaults to the discovered name, host/
  port from resolution; password stays manual — discovery can't know it).
  This mirrors mikMPD's exact behavior: discovery fills fields, it doesn't
  silently save a profile.
- "Searching…" indicator while the scan is active; "Rescan" button once it
  completes or times out.
- Footer note: "Servers appear here if MPD has Zeroconf enabled. Manual
  entry always works."

## Notes

- No permission-prompt handling needed (that's an iOS-specific `Info.plist`/
  `NSLocalNetworkUsageDescription` concern) — desktop firewalls may still
  block mDNS multicast, which is a platform/user config issue outside the
  app's control; document it in the UI footer rather than trying to detect
  it.
- MPD's advertised name comes from its `zeroconf_name` config
  ("Music Player @ %h" by default, user-configurable) — display as-is,
  don't parse/interpret it.
- This feature is **purely additive** to the existing manual add-server
  flow — no behavior changes to servers already saved in `AppConfig`.

## Implementation order

| # | Item | Size |
|---|------|------|
| 1 | Add `mdns-sd` dependency, `src/discovery` module, browse + resolve | M |
| 2 | iced `Subscription` bridging discovery events into `Message` | S |
| 3 | Settings view "Nearby Servers" section + pre-fill wiring | S |

## Testing

- Discovery itself is not meaningfully unit-testable (it's a live network
  browse) — same conclusion mikMPD's plan reaches ("browse/resolve flow is
  manual-test only"). If any pure transform exists (e.g. stripping an IPv6
  zone-id suffix from a resolved host string), unit-test that in isolation.
- Manual QA: run against a real MPD instance with `zeroconf_enabled "yes"`
  on the same LAN segment; confirm it appears, pre-fills correctly, and that
  a second app instance's Rescan still finds it after the first scan's
  timeout.
