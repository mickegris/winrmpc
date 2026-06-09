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
}
