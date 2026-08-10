//! LAN MPD server discovery via mDNS/Zeroconf (`_mpd._tcp.local.`).
//!
//! MPD advertises itself over Zeroconf when built with zeroconf support and
//! `zeroconf_enabled "yes"` (the default in most distro packages). This is a
//! network-protocol fact, not a platform one, so discovery works the same on
//! Windows/Linux/macOS via the pure-Rust `mdns-sd` crate (no OS-level
//! Bonjour dependency required).

use iced::futures::sink::SinkExt;
use iced::futures::Stream;
use std::collections::HashSet;
use std::time::Duration;

/// A server found on the LAN, ready to pre-fill the manual add-server form.
/// Discovery never saves a profile on its own — the user still confirms.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredServer {
    pub name: String,
    pub host: String,
    pub port: u16,
}

const SERVICE_TYPE: &str = "_mpd._tcp.local.";

/// How long one scan runs before stopping on its own. Re-scanning is a
/// manual "Rescan" action, not an automatic retry, to avoid burning
/// CPU/network on a browse that runs for the app's whole lifetime.
const SCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// Strips the `.{SERVICE_TYPE}` suffix from an mDNS fullname
/// (`"My MPD Server._mpd._tcp.local."`) to get the advertised instance name
/// (`"My MPD Server"`). Pure, so it's unit-testable without a live browse.
pub fn instance_name_from_fullname(fullname: &str) -> String {
    fullname
        .strip_suffix(SERVICE_TYPE)
        .unwrap_or(fullname)
        .trim_end_matches('.')
        .to_string()
}

/// An iced [`Stream`] that browses for `_mpd._tcp` services for up to
/// [`SCAN_TIMEOUT`], yielding each newly resolved server once (de-duped by
/// fullname — a server advertising on multiple interfaces resolves more
/// than once), then ends on its own.
pub fn discover() -> impl Stream<Item = DiscoveredServer> {
    iced::stream::channel(16, |mut output| async move {
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("mDNS discovery unavailable: {e}");
                return;
            }
        };
        let receiver = match daemon.browse(SERVICE_TYPE) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("mDNS browse failed: {e}");
                return;
            }
        };

        let mut seen: HashSet<String> = HashSet::new();
        let deadline = tokio::time::Instant::now() + SCAN_TIMEOUT;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;

            let event = match tokio::time::timeout(remaining, receiver.recv_async()).await {
                Ok(Ok(event)) => event,
                Ok(Err(_)) => break, // daemon channel closed
                Err(_) => break,     // scan window elapsed
            };

            if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                if seen.insert(info.fullname.clone()) {
                    let host = info
                        .get_addresses_v4()
                        .into_iter()
                        .next()
                        .map(|v4| v4.to_string())
                        .unwrap_or_else(|| info.host.trim_end_matches('.').to_string());
                    let server = DiscoveredServer {
                        name: instance_name_from_fullname(&info.fullname),
                        host,
                        port: info.port,
                    };
                    if output.send(server).await.is_err() {
                        break; // receiving end (the Subscription) went away
                    }
                }
            }
        }

        let _ = daemon.stop_browse(SERVICE_TYPE);
        let _ = daemon.shutdown();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_name_strips_service_type_suffix() {
        assert_eq!(
            instance_name_from_fullname("My MPD Server._mpd._tcp.local."),
            "My MPD Server"
        );
    }

    #[test]
    fn instance_name_passes_through_unrecognized_suffix() {
        // Doesn't end in SERVICE_TYPE, so only the trailing dot is trimmed.
        assert_eq!(
            instance_name_from_fullname("Something Else._http._tcp.local."),
            "Something Else._http._tcp.local"
        );
    }

    #[test]
    fn instance_name_handles_dots_in_the_name_itself() {
        assert_eq!(
            instance_name_from_fullname("Kitchen: MPD @ 1.2.3.4._mpd._tcp.local."),
            "Kitchen: MPD @ 1.2.3.4"
        );
    }
}
