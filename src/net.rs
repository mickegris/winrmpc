//! Shared construction for every outbound HTTP client.
//!
//! Both network clients (MusicBrainz/Cover Art Archive/Wikipedia, and LRCLIB)
//! want the same thing: this app's User-Agent and a per-request timeout. They
//! used to build it separately and disagree about both the string and the
//! failure handling — `musicbrainz.rs` called `.expect(...)` and took the whole
//! app down, `lrclib.rs` called `.unwrap_or_default()` and silently threw away
//! the User-Agent and the timeout it had just asked for.
//!
//! Neither is right. Client construction can genuinely fail when the TLS
//! backend can't initialise, and the correct response to "we cannot make HTTP
//! requests" is to log it and let lookups fail — not to panic, and not to go
//! quiet.

use std::time::Duration;

/// Identifies this app to every service it calls.
///
/// **MusicBrainz requires this to identify the application and give real
/// contact information, and throttles or blocks clients that don't.** It was
/// previously hardcoded to `winrmpc/0.1.0 (https://github.com/user/winrmpc)` —
/// a version frozen four releases back and a placeholder URL identifying
/// nobody. Deriving the version from `CARGO_PKG_VERSION` is what keeps it from
/// going stale again, and having exactly one constant is what keeps the two
/// clients from drifting apart.
pub const USER_AGENT: &str = concat!(
    "winrmpc/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/mickegris/winrmpc)"
);

/// Per-request timeout. Note this bounds a *single* request, not a lookup
/// chain — see `MusicBrainzClient::fetch_album_art`, which applies its own
/// deadline across the whole MusicBrainz → CAA → image sequence.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Build the shared HTTP client, or `None` if it cannot be built.
///
/// `purpose` names the caller so the log line says which lookups just became
/// unavailable.
pub fn client(purpose: &str) -> Option<reqwest::Client> {
    match reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => Some(client),
        Err(e) => {
            tracing::error!(
                purpose,
                error = %e,
                "could not build an HTTP client; {purpose} lookups are \
                 unavailable for this session (album art from the internet, \
                 artist/album bios and lyrics). Local and MPD-served art is \
                 unaffected."
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_carries_the_real_version_and_a_real_url() {
        assert!(USER_AGENT.starts_with("winrmpc/"));
        assert!(
            USER_AGENT.contains(env!("CARGO_PKG_VERSION")),
            "User-Agent must track the crate version, not a frozen literal"
        );
        assert!(
            USER_AGENT.contains("github.com/mickegris/winrmpc"),
            "MusicBrainz requires contact information that identifies someone"
        );
        assert!(
            !USER_AGENT.contains("github.com/user/"),
            "the placeholder URL is back"
        );
    }

    #[test]
    fn a_client_can_actually_be_built() {
        // Also a canary for the TLS feature set: with `default-features = false`
        // and no TLS feature re-added, this is where that shows up.
        assert!(client("test").is_some());
    }
}
