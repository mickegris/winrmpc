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
}
