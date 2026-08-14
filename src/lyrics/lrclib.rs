//! LRCLIB client — fetches plain and synced lyrics for a track.
//! No API key required; respects LRCLIB rate limits (generous for single users).

use reqwest::Client;
use serde::{Deserialize, Serialize};

const LRCLIB_BASE: &str = "https://lrclib.net/api";

// ── Public types ──────────────────────────────────────────────────────────────

/// A single timed line from a synced LRC source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricLine {
    pub secs: f64,
    pub text: String,
}

/// Lyrics for one track. `plain` and `synced` are not mutually exclusive;
/// LRCLIB often provides both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyrics {
    pub plain: Option<String>,
    pub synced: Option<Vec<LyricLine>>,
    pub instrumental: bool,
}

/// Seconds to delay synced-lyric highlighting.
///
/// LRCLIB timestamps mark where a line *starts*, which tends to land slightly
/// ahead of the audible vocal, so each line is held a touch longer before
/// advancing. Lives here rather than in the view because it's a property of
/// LRCLIB's data, and because the view and the autoscroll must apply the
/// identical value — see [`active_line`].
pub const SYNC_OFFSET: f64 = 0.5;

/// Index of the synced line that should be highlighted at `elapsed` seconds
/// into the track, or `None` before the first line's timestamp.
///
/// **One implementation on purpose.** The highlight (in the view) and the
/// autoscroll target (in `App::lyrics_autoscroll`) computed this separately
/// with copy-pasted arithmetic; if those two ever disagreed, the pane would
/// scroll to a different line than the one it highlights, which is both
/// obviously wrong and hard to attribute.
///
/// Assumes `lines` is sorted by `secs` — [`parse_lrc`] guarantees that, and
/// `live_lrclib_returns_parseable_synced_lyrics` asserts it against real
/// LRCLIB responses.
pub fn active_line(lines: &[LyricLine], elapsed: f64) -> Option<usize> {
    let t = elapsed - SYNC_OFFSET;
    lines.iter().rposition(|line| line.secs <= t)
}

impl Lyrics {
    /// [`active_line`] over this track's synced lines, if it has any.
    pub fn active_line(&self, elapsed: f64) -> Option<usize> {
        self.synced.as_deref().and_then(|l| active_line(l, elapsed))
    }
}

/// Async LRCLIB client. Clone-cheap because `reqwest::Client` is Arc-backed.
#[derive(Clone)]
pub struct LyricsClient {
    /// `None` when the HTTP client could not be built — see [`crate::net`].
    /// This used to be `unwrap_or_default()`, which quietly substituted a
    /// client with neither the User-Agent nor the timeout this code asks for,
    /// and would have panicked anyway if the real cause was the TLS backend
    /// (`Client::default()` is `Client::new()`, which panics).
    http: Option<Client>,
}

impl LyricsClient {
    pub fn new() -> Self {
        Self {
            http: crate::net::client("LRCLIB lyrics"),
        }
    }

    /// See `MusicBrainzClient::get` — same reasoning, same shape.
    async fn get(&self, url: &str) -> Option<reqwest::Response> {
        let http = self.http.as_ref()?;
        match http.get(url).send().await {
            Ok(resp) => Some(resp),
            Err(e) => {
                tracing::debug!(url, error = %e, "LRCLIB request failed");
                None
            }
        }
    }

    /// Fetch lyrics for a track. Tries an exact match first (duration narrows
    /// to the right recording), then falls back to a fuzzy search.
    pub async fn fetch(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        duration_secs: Option<f64>,
    ) -> Option<Lyrics> {
        // --- Exact match (preferred) -----------------------------------------
        let mut exact_url = format!(
            "{}/get?artist_name={}&track_name={}&album_name={}",
            LRCLIB_BASE,
            urlencoding::encode(artist),
            urlencoding::encode(title),
            urlencoding::encode(album),
        );
        if let Some(dur) = duration_secs {
            exact_url.push_str(&format!("&duration={}", dur as u32));
        }

        if let Some(resp) = self.get(&exact_url).await {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<LrclibResponse>().await {
                    let lyrics = parse_response(data);
                    if lyrics.plain.is_some() || lyrics.synced.is_some() || lyrics.instrumental {
                        return Some(lyrics);
                    }
                }
            }
        }

        // --- Fallback: search (relaxed — ignores album / duration) -----------
        let search_url = format!(
            "{}/search?artist_name={}&track_name={}",
            LRCLIB_BASE,
            urlencoding::encode(artist),
            urlencoding::encode(title),
        );
        if let Some(resp) = self.get(&search_url).await {
            if resp.status().is_success() {
                if let Ok(results) = resp.json::<Vec<LrclibResponse>>().await {
                    if let Some(data) = results.into_iter().next() {
                        let lyrics = parse_response(data);
                        if lyrics.plain.is_some() || lyrics.synced.is_some() || lyrics.instrumental {
                            return Some(lyrics);
                        }
                    }
                }
            }
        }

        None
    }
}

// ── Internal deserialization ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LrclibResponse {
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    instrumental: Option<bool>,
}

fn parse_response(data: LrclibResponse) -> Lyrics {
    let synced = data.synced_lyrics.as_deref().and_then(parse_lrc);
    Lyrics {
        plain: data.plain_lyrics,
        synced,
        instrumental: data.instrumental.unwrap_or(false),
    }
}

// ── LRC parser ────────────────────────────────────────────────────────────────

/// Parse an LRC string into sorted, timed lyric lines.  Returns `None` if no
/// valid timestamped lines are found.
///
/// LRC line format: `[mm:ss.xx] lyric text`
pub fn parse_lrc(lrc: &str) -> Option<Vec<LyricLine>> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some(rest) = raw.strip_prefix('[') {
            if let Some(bracket_end) = rest.find(']') {
                let timestamp = &rest[..bracket_end];
                let text = rest[bracket_end + 1..].trim().to_string();
                if let Some(secs) = parse_timestamp(timestamp) {
                    lines.push(LyricLine { secs, text });
                }
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.sort_by(|a, b| a.secs.partial_cmp(&b.secs).unwrap_or(std::cmp::Ordering::Equal));
    Some(lines)
}

/// Parse `mm:ss.xx` (or `mm:ss`) into total seconds.
fn parse_timestamp(ts: &str) -> Option<f64> {
    let (mins_str, secs_str) = ts.split_once(':')?;
    let mins: f64 = mins_str.trim().parse().ok()?;
    let secs: f64 = secs_str.trim().parse().ok()?;
    Some(mins * 60.0 + secs)
}

/// Disk-cache path for the lyrics JSON file.
pub fn cache_path(lyrics_dir: &std::path::Path, key: &str) -> std::path::PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    lyrics_dir.join(format!("{:016x}.json", hasher.finish()))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(times: &[f64]) -> Vec<LyricLine> {
        times
            .iter()
            .enumerate()
            .map(|(i, &secs)| LyricLine {
                secs,
                text: format!("line {i}"),
            })
            .collect()
    }

    #[test]
    fn no_line_is_active_before_the_first_timestamp() {
        // Real tracks routinely open with 20-30s of intro — "Creep" starts at
        // 19.16s — so this is the normal state at the start of a song, not an
        // edge case.
        let l = lines(&[19.16, 24.0, 28.0]);
        assert_eq!(active_line(&l, 0.0), None);
        assert_eq!(active_line(&l, 10.0), None);
    }

    #[test]
    fn the_active_line_is_the_last_one_whose_timestamp_has_passed() {
        let l = lines(&[10.0, 20.0, 30.0]);
        assert_eq!(active_line(&l, 10.0 + SYNC_OFFSET), Some(0));
        assert_eq!(active_line(&l, 25.0), Some(1));
        assert_eq!(active_line(&l, 999.0), Some(2), "must hold the last line to the end");
    }

    #[test]
    fn the_sync_offset_delays_the_advance_rather_than_hurrying_it() {
        // The offset exists because LRCLIB marks where a line *starts*, which
        // runs slightly ahead of the audible vocal. Getting its sign wrong
        // would make the highlight lead the song instead of trailing it — and
        // would still "work" in a casual glance, so it is worth pinning.
        let l = lines(&[10.0, 20.0]);
        assert_eq!(
            active_line(&l, 10.0 - 0.01),
            None,
            "must not highlight before the timestamp"
        );
        assert_eq!(
            active_line(&l, 20.0 + SYNC_OFFSET - 0.01),
            Some(0),
            "line 0 should still be held just before the offset elapses"
        );
        assert_eq!(active_line(&l, 20.0 + SYNC_OFFSET), Some(1));
    }

    #[test]
    fn repeated_timestamps_settle_on_the_last_of_them() {
        // Two lines at the same instant is common in chorus transcriptions;
        // rposition picks the later one, which is what a reader expects to see
        // highlighted, and matters because the autoscroll targets this index.
        let l = lines(&[10.0, 10.0, 20.0]);
        assert_eq!(active_line(&l, 15.0), Some(1));
    }

    #[test]
    fn active_line_on_lyrics_needs_synced_data() {
        let plain_only = Lyrics {
            plain: Some("just words".into()),
            synced: None,
            instrumental: false,
        };
        assert_eq!(plain_only.active_line(30.0), None);

        let synced = Lyrics {
            plain: None,
            synced: Some(lines(&[0.0, 10.0])),
            instrumental: false,
        };
        assert_eq!(synced.active_line(30.0), Some(1));
    }

    #[test]
    fn empty_synced_lyrics_never_report_an_active_line() {
        assert_eq!(active_line(&[], 42.0), None);
    }

    #[test]
    fn parse_lrc_output_is_ordered_so_active_line_can_use_rposition() {
        // active_line's correctness depends on parse_lrc's sort; assert the
        // contract between them directly rather than trusting it.
        let lrc = "[00:20.00] Late\n[00:05.00] Early\n[00:12.50] Middle\n";
        let parsed = parse_lrc(lrc).unwrap();
        assert!(parsed.windows(2).all(|w| w[0].secs <= w[1].secs));
        assert_eq!(active_line(&parsed, 13.0 + SYNC_OFFSET), Some(1));
    }

    #[test]
    fn parse_lrc_basic_timestamps() {
        let lrc = "[00:12.34] First line\n[00:15.00] Second line\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].secs - 12.34).abs() < 0.01);
        assert_eq!(lines[0].text, "First line");
        assert!((lines[1].secs - 15.0).abs() < 0.01);
        assert_eq!(lines[1].text, "Second line");
    }

    #[test]
    fn parse_lrc_sorts_by_time() {
        let lrc = "[00:20.00] Late line\n[00:05.00] Early line\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].secs - 5.0).abs() < 0.01);
        assert_eq!(lines[0].text, "Early line");
    }

    #[test]
    fn parse_lrc_skips_malformed_timestamps() {
        let lrc = "[bad] garbage line\n[00:10.00] Good line\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Good line");
    }

    #[test]
    fn parse_lrc_empty_input_returns_none() {
        assert!(parse_lrc("").is_none());
        assert!(parse_lrc("   \n  ").is_none());
    }

    #[test]
    fn parse_lrc_integer_seconds() {
        let lrc = "[01:30] Whole seconds only\n";
        let lines = parse_lrc(lrc).unwrap();
        assert!((lines[0].secs - 90.0).abs() < 0.01);
    }
}
