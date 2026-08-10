use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayState {
    Play,
    Pause,
    Stop,
}

impl Default for PlayState {
    fn default() -> Self {
        PlayState::Stop
    }
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub volume: i32,
    pub repeat: bool,
    pub random: bool,
    pub single: SingleState,
    pub consume: ConsumeState,
    pub queue_version: u32,
    pub queue_length: u32,
    pub state: PlayState,
    pub song_pos: Option<u32>,
    pub song_id: Option<u32>,
    pub next_song_pos: Option<u32>,
    pub next_song_id: Option<u32>,
    pub elapsed: Option<Duration>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u32>,
    pub crossfade: Option<u32>,
    pub mixrampdb: Option<f64>,
    pub audio: Option<AudioFormat>,
    pub updating_db: Option<u32>,
    pub error: Option<String>,
    pub partition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SingleState {
    #[default]
    Off,
    On,
    Oneshot,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ConsumeState {
    #[default]
    Off,
    On,
    Oneshot,
}

#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub bits: u32,
    pub channels: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Song {
    pub file: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub track: Option<String>,
    pub disc: Option<String>,
    pub duration_secs: Option<f64>,
    pub pos: Option<u32>,
    pub id: Option<u32>,
    pub last_modified: Option<String>,
    pub composer: Option<String>,
    pub performer: Option<String>,
    pub comment: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, Vec<String>>,
}

impl Song {
    pub fn duration(&self) -> Option<Duration> {
        self.duration_secs.map(Duration::from_secs_f64)
    }

    /// Human-readable format label derived from the file extension.
    ///
    /// MPD does not report the actual codec, so for *container* formats that can
    /// hold multiple codecs (e.g. `.m4a` may be AAC **or** ALAC, `.ogg` may be
    /// Vorbis/Opus/FLAC) we show the honest container name rather than guessing a
    /// codec. Extensions that map 1:1 to a codec show that codec. Unknown
    /// extensions are uppercased; no extension yields an empty string.
    pub fn display_format(&self) -> String {
        // Strip any query-string / fragment that some URIs carry
        let path = self.file.split('?').next().unwrap_or(&self.file);
        // Extract the filename component so we don't confuse dots in directory
        // names with the file extension (e.g. `/some.dir/trackname`).
        let filename = path.rsplit('/').next().unwrap_or(path);
        if !filename.contains('.') {
            return String::new();
        }
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            // --- Ambiguous containers: show the container, never guess a codec
            "m4a" | "m4b" | "mp4" => "M4A".into(),
            "ogg" | "oga" => "OGG".into(),
            "mka" => "MKA".into(),

            // --- Unambiguous: extension maps 1:1 to a codec
            "flac" => "FLAC".into(),
            "mp3" => "MP3".into(),
            "aac" => "AAC".into(),
            "alac" => "ALAC".into(),
            "opus" => "Opus".into(),
            "wav" | "wave" => "WAV".into(),
            "aiff" | "aif" => "AIFF".into(),
            "wv" => "WavPack".into(),
            "ape" => "APE".into(),
            "wma" => "WMA".into(),
            "dsf" | "dff" => "DSD".into(),
            "mpc" | "mp+" | "mpp" => "Musepack".into(),

            _ if !ext.is_empty() => ext.to_uppercase(),
            _ => String::new(),
        }
    }

    pub fn display_title(&self) -> &str {
        self.title
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or_else(|| self.file.rsplit('/').next().unwrap_or(&self.file))
    }

    pub fn display_artist(&self) -> &str {
        self.artist.as_deref().unwrap_or("Unknown Artist")
    }

    pub fn display_album(&self) -> &str {
        self.album.as_deref().unwrap_or("Unknown Album")
    }

    pub fn display_album_artist(&self) -> &str {
        self.album_artist
            .as_deref()
            .or(self.artist.as_deref())
            .unwrap_or("Unknown Artist")
    }

    pub fn format_duration(&self) -> String {
        match self.duration() {
            Some(d) => {
                let s = d.as_secs();
                format!("{}:{:02}", s / 60, s % 60)
            }
            None => "--:--".into(),
        }
    }

    pub fn art_key(&self) -> String {
        format!("{}\x1f{}", self.display_album_artist(), self.display_album())
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub id: u32,
    pub name: String,
    pub plugin: String,
    pub enabled: bool,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Partition {
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum DirectoryEntry {
    File(Song),
    Directory(DirectoryInfo),
    Playlist(PlaylistInfo),
}

#[derive(Debug, Clone)]
pub struct DirectoryInfo {
    pub path: String,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaylistInfo {
    pub name: String,
    pub last_modified: Option<String>,
}

/// MPD stored-playlist names are file names (`NAME.m3u` on disk): returns the
/// trimmed name, or `None` for empty names or names containing path
/// separators/newlines.
pub fn validate_playlist_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\n')
        || trimmed.contains('\r')
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub uptime: Duration,
    pub playtime: Duration,
    pub artists: u64,
    pub albums: u64,
    pub songs: u64,
    pub db_playtime: Duration,
    pub db_update: u64,
}

#[derive(Debug, Clone)]
pub enum SearchTag {
    Any,
    Artist,
    Album,
    AlbumArtist,
    Title,
    Genre,
    Date,
    Composer,
    Performer,
    File,
    Base,
}

impl SearchTag {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Any => "any",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "albumartist",
            Self::Title => "title",
            Self::Genre => "genre",
            Self::Date => "date",
            Self::Composer => "composer",
            Self::Performer => "performer",
            Self::File => "file",
            Self::Base => "base",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlbumArt {
    pub data: Vec<u8>,
    pub mime_type: Option<String>,
}

/// A recently-played album entry kept in App state and persisted in config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentAlbum {
    pub artist: String,
    pub album: String,
}

/// Insert `entry` at the front of `recents`, deduplicating and capping at 8.
///
/// Extracted as a pure function so it can be unit-tested without the GUI.
pub fn push_recent(recents: &mut Vec<RecentAlbum>, entry: RecentAlbum) {
    // Remove any existing occurrence (move-to-front semantics)
    recents.retain(|r| r != &entry);
    recents.insert(0, entry);
    recents.truncate(8);
}

// ============================================================================
// Recently Played history (distinct from `RecentAlbum` above, which is an
// 8-item "what's been playing this session" glimpse). This is a longer,
// per-track, per-server history — see docs/plans/recently-added-and-played-history.md.
// ============================================================================

/// One committed play, as shown in the Recently Played history view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentlyPlayedEntry {
    pub file: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Unix seconds.
    pub played_at: i64,
}

/// Drop entries older than 30 days, then cap at the 100 newest. `entries`
/// must already be newest-first; this never reorders.
pub fn prune_recently_played(entries: &mut Vec<RecentlyPlayedEntry>, now: i64) {
    const MAX_AGE_SECS: i64 = 30 * 86_400;
    const CAP: usize = 100;
    entries.retain(|e| now - e.played_at < MAX_AGE_SECS);
    entries.truncate(CAP);
}

/// Tracks continuous play of one file and reports when it should be
/// committed to history — Spotify/mikMPD-style: a song "counts" once it's
/// accumulated `min(30, max(5, duration/2))` seconds of actual playback, so
/// short jingles still register but a track skipped after a few seconds
/// doesn't. A file change or `tick` returning true both reset the recorder
/// for the next song.
#[derive(Debug, Default)]
pub struct PlayRecorder {
    file: String,
    last_elapsed: f64,
    accumulated_secs: f64,
    committed: bool,
}

impl PlayRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call on every status poll with the current file, whether it's
    /// playing, the song's elapsed position (seconds into the track), and
    /// its total duration (if known). Returns `true` exactly once per
    /// continuous play, the moment the commit threshold is crossed.
    pub fn tick(
        &mut self,
        file: &str,
        is_playing: bool,
        elapsed_secs: f64,
        duration_secs: Option<f64>,
    ) -> bool {
        if file != self.file {
            self.file = file.to_string();
            self.last_elapsed = elapsed_secs;
            self.accumulated_secs = 0.0;
            self.committed = false;
            return false;
        }
        if !is_playing {
            self.last_elapsed = elapsed_secs;
            return false;
        }
        // Cap a single delta at 5s so a coarse/gapped poll cadence (or a
        // manual seek) can't fast-forward the accumulator.
        let delta = (elapsed_secs - self.last_elapsed).max(0.0).min(5.0);
        self.last_elapsed = elapsed_secs;
        if self.committed {
            return false;
        }
        self.accumulated_secs += delta;
        let threshold = duration_secs
            .map(|d| (d / 2.0).max(5.0).min(30.0))
            .unwrap_or(30.0);
        if self.accumulated_secs >= threshold {
            self.committed = true;
            return true;
        }
        false
    }
}

/// One tile's worth of data for the Recently Played "Albums" view — derived
/// from track history, not recorded separately, so there's one source of
/// truth (mirrors mikMPD's `recentAlbumGroups`).
#[derive(Debug, Clone, PartialEq)]
pub struct RecentlyPlayedAlbum {
    pub artist: String,
    pub album: String,
    pub last_played: i64,
}

/// Collapse per-track history into newest-first album groups. `entries` must
/// already be newest-first; the first occurrence of each (artist, album)
/// pair wins (it's the most recent one), so this is a single pass.
pub fn recently_played_albums(entries: &[RecentlyPlayedEntry]) -> Vec<RecentlyPlayedAlbum> {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut groups = Vec::new();
    for e in entries {
        let key = (e.artist.clone(), e.album.clone());
        if seen.insert(key) {
            groups.push(RecentlyPlayedAlbum {
                artist: e.artist.clone(),
                album: e.album.clone(),
                last_played: e.played_at,
            });
        }
    }
    groups
}

/// "3 min ago" / "2 hours ago" / "5 days ago"-style relative timestamp.
pub fn relative_time(secs_ago: i64) -> String {
    let secs_ago = secs_ago.max(0);
    if secs_ago < 60 {
        "just now".to_string()
    } else if secs_ago < 3600 {
        let mins = secs_ago / 60;
        format!("{mins} min ago")
    } else if secs_ago < 86_400 {
        let hours = secs_ago / 3600;
        format!("{hours}h ago")
    } else {
        let days = secs_ago / 86_400;
        format!("{days}d ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Song {
        Song::default()
    }

    #[test]
    fn display_title_falls_back_to_filename() {
        let mut s = song();
        s.file = "music/Artist/Album/05 - Track.flac".into();
        assert_eq!(s.display_title(), "05 - Track.flac");

        s.title = Some("Real Title".into());
        assert_eq!(s.display_title(), "Real Title");
    }

    #[test]
    fn display_album_artist_prefers_album_artist_then_artist() {
        let mut s = song();
        assert_eq!(s.display_album_artist(), "Unknown Artist");

        s.artist = Some("Track Artist".into());
        assert_eq!(s.display_album_artist(), "Track Artist");

        s.album_artist = Some("Album Artist".into());
        assert_eq!(s.display_album_artist(), "Album Artist");
    }

    #[test]
    fn art_key_uses_unit_separator() {
        let mut s = song();
        s.album_artist = Some("Pink Floyd".into());
        s.album = Some("The Wall".into());
        assert_eq!(s.art_key(), "Pink Floyd\u{1f}The Wall");
    }

    #[test]
    fn art_key_does_not_collide_on_hyphenated_names() {
        // The 0x1f separator must keep these distinct even though a naive
        // "{artist}-{album}" key would make both "A-B-C".
        let mut a = song();
        a.album_artist = Some("A-B".into());
        a.album = Some("C".into());

        let mut b = song();
        b.album_artist = Some("A".into());
        b.album = Some("B-C".into());

        assert_ne!(a.art_key(), b.art_key());
    }

    #[test]
    fn art_key_falls_back_for_missing_metadata() {
        assert_eq!(song().art_key(), "Unknown Artist\u{1f}Unknown Album");
    }

    #[test]
    fn format_duration_renders_minutes_and_seconds() {
        let mut s = song();
        s.duration_secs = Some(183.0);
        assert_eq!(s.format_duration(), "3:03");

        s.duration_secs = Some(5.0);
        assert_eq!(s.format_duration(), "0:05");
    }

    #[test]
    fn format_duration_unknown_when_absent() {
        assert_eq!(song().format_duration(), "--:--");
    }

    #[test]
    fn duration_converts_secs_to_duration() {
        let mut s = song();
        s.duration_secs = Some(90.0);
        assert_eq!(s.duration().unwrap().as_secs(), 90);
        s.duration_secs = None;
        assert!(s.duration().is_none());
    }

    // --- display_format -------------------------------------------------

    #[test]
    fn display_format_known_extensions() {
        let cases = [
            ("track.flac", "FLAC"),
            ("track.mp3", "MP3"),
            ("track.aac", "AAC"),
            ("track.opus", "Opus"),
            ("track.wav", "WAV"),
            ("track.aiff", "AIFF"),
            ("track.wv", "WavPack"),
            ("track.ape", "APE"),
            ("track.wma", "WMA"),
            ("track.dsf", "DSD"),
        ];
        for (file, expected) in cases {
            let mut s = song();
            s.file = file.into();
            assert_eq!(s.display_format(), expected, "file={file}");
        }
    }

    #[test]
    fn display_format_ambiguous_containers_show_container_name() {
        // MPD can't tell us the codec, so .m4a/.ogg must not claim AAC/Vorbis.
        for file in ["track.m4a", "track.m4b", "track.mp4"] {
            let mut s = song();
            s.file = file.into();
            assert_eq!(s.display_format(), "M4A", "file={file}");
        }
        for file in ["track.ogg", "track.oga"] {
            let mut s = song();
            s.file = file.into();
            assert_eq!(s.display_format(), "OGG", "file={file}");
        }
    }

    #[test]
    fn display_format_unknown_extension_uppercased() {
        let mut s = song();
        s.file = "track.xyz".into();
        assert_eq!(s.display_format(), "XYZ");
    }

    #[test]
    fn display_format_no_extension_returns_empty() {
        let mut s = song();
        s.file = "track".into();
        assert_eq!(s.display_format(), "");
    }

    #[test]
    fn display_format_case_insensitive() {
        let mut s = song();
        s.file = "track.FLAC".into();
        assert_eq!(s.display_format(), "FLAC");
    }

    // --- push_recent ----------------------------------------------------

    #[test]
    fn push_recent_inserts_at_front() {
        let mut v = vec![];
        push_recent(&mut v, RecentAlbum { artist: "A".into(), album: "1".into() });
        push_recent(&mut v, RecentAlbum { artist: "B".into(), album: "2".into() });
        assert_eq!(v[0].album, "2");
        assert_eq!(v[1].album, "1");
    }

    #[test]
    fn push_recent_deduplicates_and_moves_to_front() {
        let mut v = vec![
            RecentAlbum { artist: "A".into(), album: "1".into() },
            RecentAlbum { artist: "B".into(), album: "2".into() },
        ];
        push_recent(&mut v, RecentAlbum { artist: "A".into(), album: "1".into() });
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].album, "1");
    }

    #[test]
    fn push_recent_caps_at_eight() {
        let mut v = vec![];
        for i in 0..10u32 {
            push_recent(&mut v, RecentAlbum { artist: "X".into(), album: i.to_string() });
        }
        assert_eq!(v.len(), 8);
        // Most recent is at front
        assert_eq!(v[0].album, "9");
    }

    // --- prune_recently_played ---------------------------------------------

    fn played_entry(secs_ago: i64, now: i64) -> RecentlyPlayedEntry {
        RecentlyPlayedEntry {
            file: "song.mp3".into(),
            title: "T".into(),
            artist: "A".into(),
            album: "Al".into(),
            played_at: now - secs_ago,
        }
    }

    #[test]
    fn prune_recently_played_drops_entries_older_than_30_days() {
        let now = 1_000_000_000;
        let mut v = vec![
            played_entry(10, now),
            played_entry(31 * 86_400, now),
        ];
        prune_recently_played(&mut v, now);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].played_at, now - 10);
    }

    #[test]
    fn prune_recently_played_caps_at_100() {
        let now = 1_000_000_000;
        let mut v: Vec<RecentlyPlayedEntry> =
            (0..150).map(|i| played_entry(i, now)).collect();
        prune_recently_played(&mut v, now);
        assert_eq!(v.len(), 100);
    }

    #[test]
    fn prune_recently_played_applies_both_limits_together() {
        let now = 1_000_000_000;
        // 60 fresh entries + 60 stale entries.
        let mut v: Vec<RecentlyPlayedEntry> = (0..60).map(|i| played_entry(i, now)).collect();
        v.extend((0..60).map(|_| played_entry(40 * 86_400, now)));
        prune_recently_played(&mut v, now);
        assert_eq!(v.len(), 60);
        assert!(v.iter().all(|e| now - e.played_at < 30 * 86_400));
    }

    #[test]
    fn prune_recently_played_empty_input() {
        let mut v: Vec<RecentlyPlayedEntry> = vec![];
        prune_recently_played(&mut v, 1_000_000_000);
        assert!(v.is_empty());
    }

    // --- PlayRecorder --------------------------------------------------------

    /// Ticks `r` forward from `from` to `to` in <=1s steps (realistic polling
    /// cadence — winrmpc polls every 500ms) so no delta trips the 5s
    /// large-jump cap. Returns the last tick's result.
    fn tick_steps(r: &mut PlayRecorder, file: &str, from: f64, to: f64, duration: Option<f64>) -> bool {
        let mut elapsed = from;
        let mut committed = false;
        while elapsed < to {
            elapsed = (elapsed + 1.0).min(to);
            committed = r.tick(file, true, elapsed, duration);
        }
        committed
    }

    #[test]
    fn play_recorder_commits_at_30_seconds_when_duration_unknown() {
        let mut r = PlayRecorder::new();
        assert!(!r.tick("a.mp3", true, 0.0, None)); // file-change tick, resets
        assert!(!tick_steps(&mut r, "a.mp3", 0.0, 29.0, None));
        assert!(tick_steps(&mut r, "a.mp3", 29.0, 30.0, None));
    }

    #[test]
    fn play_recorder_half_duration_rule_for_short_track() {
        // 20s track → threshold = max(5, 10) = 10s.
        let mut r = PlayRecorder::new();
        r.tick("a.mp3", true, 0.0, Some(20.0));
        assert!(!tick_steps(&mut r, "a.mp3", 0.0, 9.0, Some(20.0)));
        assert!(tick_steps(&mut r, "a.mp3", 9.0, 10.0, Some(20.0)));
    }

    #[test]
    fn play_recorder_does_not_double_commit_same_file() {
        let mut r = PlayRecorder::new();
        r.tick("a.mp3", true, 0.0, None);
        assert!(tick_steps(&mut r, "a.mp3", 0.0, 30.0, None));
        assert!(!tick_steps(&mut r, "a.mp3", 30.0, 40.0, None));
    }

    #[test]
    fn play_recorder_file_change_resets_accumulator() {
        let mut r = PlayRecorder::new();
        r.tick("a.mp3", true, 0.0, None);
        assert!(!tick_steps(&mut r, "a.mp3", 0.0, 20.0, None)); // accumulated 20s, not yet committed
        assert!(!r.tick("b.mp3", true, 0.0, None)); // file-change tick itself never commits
        assert!(!tick_steps(&mut r, "b.mp3", 0.0, 20.0, None)); // only 20s into the new file
        assert!(tick_steps(&mut r, "b.mp3", 20.0, 30.0, None));
    }

    #[test]
    fn play_recorder_pause_freezes_accumulation() {
        let mut r = PlayRecorder::new();
        r.tick("a.mp3", true, 0.0, None);
        tick_steps(&mut r, "a.mp3", 0.0, 20.0, None);
        // Paused for a while — elapsed doesn't move; must not commit or misbehave.
        assert!(!r.tick("a.mp3", false, 20.0, None));
        assert!(!r.tick("a.mp3", false, 20.0, None));
        // Resume: needs 10 more accumulated seconds to reach 30s.
        assert!(!tick_steps(&mut r, "a.mp3", 20.0, 25.0, None));
        assert!(tick_steps(&mut r, "a.mp3", 25.0, 30.0, None));
    }

    #[test]
    fn play_recorder_large_elapsed_jump_is_capped_at_5s() {
        let mut r = PlayRecorder::new();
        r.tick("a.mp3", true, 0.0, None);
        // A seek or long poll gap must not instantly satisfy the threshold.
        assert!(!r.tick("a.mp3", true, 1000.0, None));
    }

    // --- recently_played_albums ---------------------------------------------

    #[test]
    fn recently_played_albums_dedupes_newest_wins() {
        let entries = vec![
            RecentlyPlayedEntry { file: "1".into(), title: "T1".into(), artist: "A".into(), album: "X".into(), played_at: 300 },
            RecentlyPlayedEntry { file: "2".into(), title: "T2".into(), artist: "A".into(), album: "X".into(), played_at: 200 },
            RecentlyPlayedEntry { file: "3".into(), title: "T3".into(), artist: "B".into(), album: "Y".into(), played_at: 100 },
        ];
        let groups = recently_played_albums(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].album, "X");
        assert_eq!(groups[0].last_played, 300);
        assert_eq!(groups[1].album, "Y");
    }

    #[test]
    fn recently_played_albums_distinct_artists_same_title_stay_separate() {
        let entries = vec![
            RecentlyPlayedEntry { file: "1".into(), title: "T".into(), artist: "A".into(), album: "Greatest Hits".into(), played_at: 200 },
            RecentlyPlayedEntry { file: "2".into(), title: "T".into(), artist: "B".into(), album: "Greatest Hits".into(), played_at: 100 },
        ];
        assert_eq!(recently_played_albums(&entries).len(), 2);
    }

    #[test]
    fn recently_played_albums_empty_input() {
        assert!(recently_played_albums(&[]).is_empty());
    }

    // --- relative_time ---------------------------------------------------

    #[test]
    fn relative_time_formats_buckets() {
        assert_eq!(relative_time(30), "just now");
        assert_eq!(relative_time(90), "1 min ago");
        assert_eq!(relative_time(3700), "1h ago");
        assert_eq!(relative_time(2 * 86_400 + 10), "2d ago");
    }

    // --- validate_playlist_name ------------------------------------------

    #[test]
    fn validate_playlist_name_trims_whitespace() {
        assert_eq!(validate_playlist_name("  My Mix  "), Some("My Mix".to_string()));
    }

    #[test]
    fn validate_playlist_name_rejects_empty() {
        assert_eq!(validate_playlist_name(""), None);
        assert_eq!(validate_playlist_name("   "), None);
    }

    #[test]
    fn validate_playlist_name_rejects_path_separators() {
        assert_eq!(validate_playlist_name("a/b"), None);
        assert_eq!(validate_playlist_name("a\\b"), None);
    }

    #[test]
    fn validate_playlist_name_rejects_newlines() {
        assert_eq!(validate_playlist_name("a\nb"), None);
        assert_eq!(validate_playlist_name("a\rb"), None);
    }

    #[test]
    fn validate_playlist_name_accepts_plain_name() {
        assert_eq!(validate_playlist_name("Road Trip"), Some("Road Trip".to_string()));
    }
}
