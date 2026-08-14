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

    /// See `art_key_for` — this is just that function fed from the song's
    /// own display-fallback artist/album.
    pub fn art_key(&self) -> String {
        art_key_for(self.display_album_artist(), self.display_album())
    }

    /// Cache key for this track's lyrics: `artist\x1ftitle\x1falbum`.
    ///
    /// Unlike [`Song::art_key`] this is **per track**, and it is deliberately
    /// **not** disc-folded — two discs of a set are different songs with
    /// different words.
    ///
    /// It exists because the same `format!` was written out by hand in three
    /// places (the fetch, the view's lookup and the autoscroll's lookup). They
    /// happened to agree, but nothing made them: any drift would have been
    /// silent, showing up only as lyrics that load and then never scroll.
    pub fn lyrics_key(&self) -> String {
        format!(
            "{}\x1f{}\x1f{}",
            self.display_artist(),
            self.display_title(),
            self.display_album()
        )
    }

    /// The disc number to sort/group by: the `disc` tag if present and
    /// nonzero (handles both bare `"2"` and `"2/2"` forms), else the
    /// disc suffix embedded in the album name itself (`"X [Disc 2]"`),
    /// else `1`. Fixes track-order interleaving on albums that use a
    /// proper `disc` tag with no name suffix (MPD sorts by track number
    /// alone, so disc 1 and disc 2 tracks interleave without this).
    pub fn effective_disc(&self) -> u32 {
        if let Some(d) = self.disc.as_deref() {
            if let Some(n) = d.split('/').next().and_then(|s| s.trim().parse::<u32>().ok()) {
                if n > 0 {
                    return n;
                }
            }
        }
        album_base_and_disc(self.display_album()).1.unwrap_or(1)
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

// ============================================================================
// Album identity: artist-aware grouping + multi-disc collapsing.
// See docs/plans/library-album-identity-and-multidisc.md.
// ============================================================================

/// One row in a grouped album list: `variants` holds every raw album tag
/// that collapsed into this (artist, base-title) group — more than one
/// means a multi-disc set tagged with a name suffix (`"X [Disc 1]"` /
/// `"X [Disc 2]"`).
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumGroup {
    pub artist: String,
    pub base: String,
    pub variants: Vec<String>,
}

/// Groups `(album_artist, album)` pairs (as returned by
/// `MpdClient::list_albums_by_artist`) into artist-aware, disc-collapsed
/// rows. Keyed on `(artist.to_lowercase(), base.to_lowercase())`, so
/// same-named albums by different artists stay separate rows while
/// disc-suffixed variants of the same artist's album collapse into one.
/// Preserves first-seen order.
///
/// The **base** is case-folded for the key (though the first-seen spelling
/// is what gets displayed) because inconsistent capitalisation across the
/// discs of one set is common in real tags — this library has
/// `"Decade Of Aggression - Disc 2"` alongside
/// `"Decade of Aggression - Disc 1 of 2"`, which would otherwise be two
/// rows of one album.
pub fn group_albums_by_artist(pairs: &[(String, String)]) -> Vec<AlbumGroup> {
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut groups: Vec<AlbumGroup> = Vec::new();
    for (artist, album) in pairs {
        let base = album_base_and_disc(album).0;
        let key = (artist.to_lowercase(), base.to_lowercase());
        match index.get(&key) {
            Some(&i) => {
                if !groups[i].variants.contains(album) {
                    groups[i].variants.push(album.clone());
                }
            }
            None => {
                index.insert(key, groups.len());
                groups.push(AlbumGroup {
                    artist: artist.clone(),
                    base,
                    variants: vec![album.clone()],
                });
            }
        }
    }
    groups
}

/// The "N discs" count for a group, combining both signals a real library
/// needs (mikMPD's hard-won lesson): the name-suffix variant count *and*
/// the highest `disc` tag value seen among the group's songs. Neither
/// alone is reliable — a properly tagged multi-disc album (one album name,
/// `disc: 1..4`) has no name variants; a poorly tagged one has variants but
/// no disc tag. `max_tag_disc` is the caller-computed max of
/// `Song::effective_disc()` across the group's songs (0 if unknown/unfetched).
pub fn album_disc_count(variant_count: usize, max_tag_disc: u32) -> usize {
    variant_count.max(max_tag_disc as usize)
}

const DISC_MARKER_WORDS: [&str; 3] = ["disc", "disk", "cd"];
const DISC_SEPARATORS: [char; 5] = ['-', '\u{2013}', '\u{2014}', ':', ','];

/// Strips a trailing disc marker from an album title, returning
/// `(base, Some(disc_number))`. Recognised forms:
///
/// | Input | Base | Disc |
/// |---|---|---|
/// | `"X [Disc 1]"` / `"X (Disk 2)"` | `X` | 1 / 2 |
/// | `"X - Disc 1"` / `"X: disc 12"` / `"XCD2"` | `X` | 1 / 12 / 2 |
/// | `"X (Disc One)"` | `X` | 1 |
/// | `"X [Disc A]"` / `"X [Disc B]"` | `X` | 1 / 2 |
/// | `"X - Disc 1 of 2"` / `"X (CD 1/2)"` | `X` | 1 |
/// | `"X [24-bit Remaster CD 1]"` | `X [24-bit Remaster]` | 1 |
///
/// That last row is the only case where the base keeps a bracket: when the
/// marker is merely the *tail* of a qualifier bracket, only the marker is
/// removed and the qualifier stays. Dropping the whole bracket would fold a
/// remaster into the plain edition, which the library may hold separately —
/// and it's the same reason `strip_edition_qualifier` is lookup-only and
/// never touches a cache key.
///
/// Passes the input through unchanged (`(album, None)`) when there is no
/// marker, when the "marker" carries no disc identifier (`"Live CD"`,
/// `"Killers (CDM 7520192)"`, `"… [2001 CD Edition]"`), or when stripping
/// would leave an empty base (`"Disc 1"` alone).
pub fn album_base_and_disc(album: &str) -> (String, Option<u32>) {
    let trimmed = album.trim_end();
    if trimmed.is_empty() {
        return (album.to_string(), None);
    }

    // Bracketed form: "... [Disc 1]" / "... (CD 2)".
    if let Some(&last) = trimmed.as_bytes().last() {
        if last == b']' || last == b')' {
            let (open, close) = if last == b']' { ('[', ']') } else { ('(', ')') };
            if let Some(open_idx) = trimmed.rfind(open) {
                let inner = &trimmed[open_idx + 1..trimmed.len() - 1];

                // (a) The whole bracket is the marker: "X [Disc 1]" -> "X".
                if let Some(n) = parse_disc_marker_whole(inner) {
                    let base = trim_trailing_separator(&trimmed[..open_idx]);
                    if !base.is_empty() {
                        return (strip_disc_count_qualifier(base).to_string(), Some(n));
                    }
                }

                // (b) The marker is only the tail of a qualifier bracket:
                // "X [24-bit Remaster CD 1]" -> "X [24-bit Remaster]".
                // Keeping the qualifier is what lets both discs of a
                // remastered set collapse together without also merging
                // them into a differently-mastered copy of the album.
                if let Some((rest, n)) = parse_bare_trailing_marker(inner) {
                    let before = &trimmed[..open_idx];
                    if !before.trim().is_empty() {
                        return (format!("{before}{open}{rest}{close}"), Some(n));
                    }
                }
            }
        }
    }

    // Bare trailing form: "... - Disc 1" / "...CD2" (no brackets).
    if let Some((base, n)) = parse_bare_trailing_marker(trimmed) {
        return (strip_disc_count_qualifier(&base).to_string(), Some(n));
    }

    (strip_disc_count_qualifier(album).to_string(), None)
}

/// Drops a trailing "how many discs are in the box" bracket — `"(2CD)"`,
/// `"(3 CDs)"`, `"[2 Disc]"`. That's a packaging note, not a disc
/// *identifier* and not part of the title, and it is applied to every base
/// (marker or not) because it has to be: this library tags one album's two
/// discs as `"Nostradamus (2CD) (CD 1/2)"` and `"Nostradamus (disc 2)"`,
/// which only land on the same base if the count bracket goes away whether
/// or not a disc marker followed it.
fn strip_disc_count_qualifier(s: &str) -> &str {
    let trimmed = s.trim_end();
    let Some(&last) = trimmed.as_bytes().last() else { return s };
    let open = match last {
        b']' => '[',
        b')' => '(',
        _ => return s,
    };
    let Some(open_idx) = trimmed.rfind(open) else { return s };
    if !is_disc_count_phrase(&trimmed[open_idx + 1..trimmed.len() - 1]) {
        return s;
    }
    let base = trim_trailing_separator(&trimmed[..open_idx]);
    if base.is_empty() {
        s
    } else {
        base
    }
}

/// True for `"2CD"`, `"2 CD"`, `"3-CDs"`, `"2 Disc"` — a count, then a
/// marker word, and nothing else. Deliberately narrow: `"24-bit Remaster"`
/// also starts with digits, but what follows isn't a marker word.
fn is_disc_count_phrase(inner: &str) -> bool {
    let inner = inner.trim().to_lowercase();
    let digits_end = inner
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(inner.len());
    // A count of 1 disc isn't a box, and >99 isn't a count.
    if digits_end == 0 || digits_end > 2 {
        return false;
    }
    let rest = inner[digits_end..].trim_start_matches([' ', '-']);
    DISC_MARKER_WORDS
        .iter()
        .any(|w| rest == *w || (rest.len() == w.len() + 1 && rest.starts_with(w) && rest.ends_with('s')))
}

/// The single source of truth for building an album's art-cache key —
/// every call site that fetches, stores, or looks up cached art must go
/// through this (or `Song::art_key()`, which just calls it), or a
/// disc-suffixed album's art silently misses the cache. Folds the album
/// through `album_base_and_disc` so every disc of a set shares one entry.
pub fn art_key_for(artist: &str, album: &str) -> String {
    let base = album_base_and_disc(album).0;
    format!("{artist}\x1f{base}")
}

/// Key for the `App`-level `album_songs`/`album_bios` maps (and the redb
/// `bios` table), scoped by artist so two different artists' same-named
/// album don't collide — `View::AlbumDetail` already carries this same
/// `Option<String>`, so storage and render-time lookup always agree as
/// long as both go through this helper. `artist: None` (only when the
/// album was reached with no known artist, e.g. from Genre detail) keys
/// on an empty-string prefix, which can never collide with a real artist
/// name (`Song::display_album_artist()` always falls back to a non-empty
/// "Unknown Artist" rather than returning `""`).
///
/// Unlike `art_key_for`, this does **not** strip disc markers — the input
/// here is already the collapsed base name `AlbumSelected` resolved, not a
/// raw per-track tag.
pub fn album_scoped_key(artist: Option<&str>, album: &str) -> String {
    format!("{}\x1f{album}", artist.unwrap_or(""))
}

fn trim_trailing_separator(s: &str) -> &str {
    s.trim_end()
        .trim_end_matches(DISC_SEPARATORS.as_slice())
        .trim_end()
}

/// Parses e.g. "disc 1", "disk.2", "cd12", "Disc One", "Disc A",
/// "CD 1/2" when the *entire* (trimmed) input is exactly that — used for
/// the content of a trailing bracket.
///
/// Operates on a lowercased copy and never indexes back into `s`, so the
/// non-length-preserving-`to_lowercase` hazard `rfind_ascii_ci` exists to
/// avoid doesn't apply here.
fn parse_disc_marker_whole(s: &str) -> Option<u32> {
    let lower = s.trim().to_lowercase();
    for word in DISC_MARKER_WORDS {
        let Some(after_word) = lower.strip_prefix(word) else { continue };
        let rest = after_word.trim_start_matches('.').trim_start();
        if let Some(n) = parse_disc_number(rest) {
            return Some(n);
        }
        // A spelled-out number or a letter disc id is only accepted when
        // something separated it from the marker word. Without that,
        // "(CDs)" would read as "CD, disc S" and "(CDone)" as "CD, disc 1".
        if after_word.len() != rest.len() {
            if let Some(n) = parse_disc_word_or_letter(rest) {
                return Some(n);
            }
        }
    }
    None
}

/// Real multi-disc sets are small; anything larger is far more likely to be
/// a catalogue number or a year that happens to sit after a marker word.
const MAX_DISC_NUMBER: u32 = 99;

/// A numeric disc identifier: a short digit run (`"1"`, `"12"`), or the
/// "which of how many" forms `"1 of 2"` / `"1/2"`, from which the first
/// number is taken.
fn parse_disc_number(s: &str) -> Option<u32> {
    let head = disc_number_head(s.trim());
    if !is_short_digit_run(head) {
        return None;
    }
    head.parse().ok().filter(|n| *n <= MAX_DISC_NUMBER)
}

/// For `"1 of 2"` / `"1/2"` returns `"1"`; otherwise returns `s` unchanged.
/// The total is required to be a short digit run too, so `"1 of these"`
/// isn't mistaken for a disc-of-total form.
fn disc_number_head(s: &str) -> &str {
    if let Some((head, total)) = s.split_once('/') {
        if is_short_digit_run(total.trim()) {
            return head.trim();
        }
    }
    // `rfind_ascii_ci` rather than searching a lowercased copy: the index
    // is used to slice `s` itself.
    if let Some(idx) = rfind_ascii_ci(s, " of ") {
        let total = &s[idx + " of ".len()..];
        if is_short_digit_run(total.trim()) {
            return s[..idx].trim();
        }
    }
    s
}

const DISC_WORD_NUMBERS: [(&str, u32); 12] = [
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
];

/// A spelled-out disc number (`"One"`) or a single-letter disc id
/// (`"A"` -> 1, `"B"` -> 2), as used by e.g. Depeche Mode's `101 [Disc A]`.
fn parse_disc_word_or_letter(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some((_, n)) = DISC_WORD_NUMBERS.iter().find(|(w, _)| *w == lower) {
        return Some(*n);
    }
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() || !c.is_ascii_alphabetic() {
        return None;
    }
    Some(u32::from(c.to_ascii_lowercase() as u8 - b'a') + 1)
}

fn is_short_digit_run(s: &str) -> bool {
    !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit())
}

/// Handles an unbracketed marker at the very end of `s`. Requires a
/// delimiter (whitespace or one of `DISC_SEPARATORS`) — or start-of-string —
/// immediately before the marker word, so `"ABCD2"` doesn't match.
fn parse_bare_trailing_marker(s: &str) -> Option<(String, u32)> {
    for word in DISC_MARKER_WORDS {
        let Some(idx) = rfind_ascii_ci(s, word) else { continue };
        let after = &s[idx + word.len()..];
        let ident = after.trim_start_matches('.').trim_start();
        // Same delimiter rule as `parse_disc_marker_whole`: digits may abut
        // the marker word ("CD2"), a spelled-out number or letter may not.
        let separated = after.len() != ident.len();
        let Some(n) = parse_disc_number(ident)
            .or_else(|| separated.then(|| parse_disc_word_or_letter(ident)).flatten())
        else {
            continue;
        };
        let before = &s[..idx];
        let boundary_ok = before.is_empty()
            || before.ends_with(|c: char| c.is_whitespace() || DISC_SEPARATORS.contains(&c));
        if !boundary_ok {
            continue;
        }
        let base = trim_trailing_separator(before);
        if !base.is_empty() {
            return Some((base.to_string(), n));
        }
    }
    None
}

/// Case-insensitive (ASCII-only) rightmost search for `needle` in
/// `haystack`, returning a byte index **into `haystack` itself**.
///
/// Unlike matching against a `haystack.to_lowercase()` copy and reusing the
/// resulting index to slice `haystack`, this never drifts out of sync:
/// `str::to_lowercase` isn't length-preserving for some non-ASCII
/// characters (e.g. Turkish `'İ'`, U+0130, 2 bytes → `"i̇"`, 3 bytes), which
/// previously caused an out-of-bounds slice / non-char-boundary panic here.
/// `needle` is always ASCII (`DISC_MARKER_WORDS`), so ASCII-only case
/// folding via `eq_ignore_ascii_case` is correct and index-safe.
fn rfind_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let needle_len = needle.len();
    if needle_len == 0 || needle_len > haystack.len() {
        return None;
    }
    let mut found = None;
    for (i, _) in haystack.char_indices() {
        let end = i + needle_len;
        if end <= haystack.len()
            && haystack.is_char_boundary(end)
            && haystack[i..end].eq_ignore_ascii_case(needle)
        {
            found = Some(i); // keep overwriting — char_indices() is ascending, so the last hit is rightmost
        }
    }
    found
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
    /// The *track* artist (`display_artist()`) — what the history list
    /// shows per row. Not usable as an art-cache key; see `album_artist`.
    pub artist: String,
    /// The *album* artist (`display_album_artist()`), recorded separately
    /// because art is always cached under that (`Song::art_key()`), never
    /// under the track artist. On a compilation or a "feat." track the two
    /// differ, and keying art off `artist` silently missed the cache.
    ///
    /// `#[serde(default)]` for entries persisted before this field existed:
    /// they deserialize to `""`, and `art_artist()` falls back to `artist`.
    #[serde(default)]
    pub album_artist: String,
    pub album: String,
    /// Unix seconds.
    pub played_at: i64,
}

impl RecentlyPlayedEntry {
    /// The artist to build an art-cache key from — `album_artist` when
    /// known, else the track artist (pre-`album_artist` history entries).
    pub fn art_artist(&self) -> &str {
        if self.album_artist.is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }
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
        // Group and label by the *album* artist: an album tile is about the
        // album, so a compilation collapses to one tile instead of one per
        // guest artist, and `artist` here is then the same value
        // `Song::art_key()` cached the art under.
        let artist = e.art_artist();
        let key = (artist.to_string(), e.album.clone());
        if seen.insert(key) {
            groups.push(RecentlyPlayedAlbum {
                artist: artist.to_string(),
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
    fn lyrics_key_is_per_track_and_not_disc_folded() {
        let mut s = song();
        s.artist = Some("Pink Floyd".into());
        s.title = Some("Comfortably Numb".into());
        s.album = Some("The Wall [Disc 2]".into());
        assert_eq!(
            s.lyrics_key(),
            "Pink Floyd\u{1f}Comfortably Numb\u{1f}The Wall [Disc 2]"
        );
        // Art folds discs together so one cover serves the set; lyrics must
        // not, since the two discs hold different songs.
        assert_eq!(s.art_key(), "Pink Floyd\u{1f}The Wall");
    }

    #[test]
    fn lyrics_key_uses_the_track_artist_not_the_album_artist() {
        // LRCLIB is queried per recording, and `fetch_lyrics` sends
        // `display_artist()` — so the cache key has to agree, or a
        // compilation's guest artists would all share one cached result.
        let mut s = song();
        s.album_artist = Some("Various Artists".into());
        s.artist = Some("Nina Simone".into());
        s.title = Some("Sinnerman".into());
        s.album = Some("Compilation".into());
        assert!(s.lyrics_key().starts_with("Nina Simone\u{1f}"));
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
    fn art_key_collapses_disc_variants_to_one_entry() {
        let mut a = song();
        a.album_artist = Some("Gamma Ray".into());
        a.album = Some("Blast from the Past [Disc 1]".into());
        let mut b = song();
        b.album_artist = Some("Gamma Ray".into());
        b.album = Some("Blast from the Past [Disc 2]".into());
        assert_eq!(a.art_key(), b.art_key());
    }

    // --- art_key_for --------------------------------------------------------

    #[test]
    fn art_key_for_collapses_disc_suffix() {
        assert_eq!(
            art_key_for("Gamma Ray", "Blast from the Past [Disc 1]"),
            art_key_for("Gamma Ray", "Blast from the Past"),
        );
    }

    #[test]
    fn art_key_for_agrees_with_song_art_key() {
        let mut s = song();
        s.album_artist = Some("Gamma Ray".into());
        s.album = Some("Blast from the Past [Disc 2]".into());
        assert_eq!(s.art_key(), art_key_for("Gamma Ray", "Blast from the Past [Disc 2]"));
    }

    // --- album_scoped_key -----------------------------------------------

    #[test]
    fn album_scoped_key_distinguishes_artists() {
        let a = album_scoped_key(Some("Artist A"), "Greatest Hits");
        let b = album_scoped_key(Some("Artist B"), "Greatest Hits");
        assert_ne!(a, b);
    }

    #[test]
    fn album_scoped_key_none_artist_does_not_collide_with_named_artist() {
        let unknown = album_scoped_key(None, "Greatest Hits");
        let named = album_scoped_key(Some(""), "Greatest Hits");
        // Both degrade to the same empty-artist prefix, which is fine —
        // the guarantee is only that this never matches a *real* artist
        // name, and display_album_artist() never actually returns "".
        assert_eq!(unknown, named);
        assert_ne!(unknown, album_scoped_key(Some("Someone"), "Greatest Hits"));
    }

    // --- effective_disc ---------------------------------------------------

    #[test]
    fn effective_disc_prefers_disc_tag() {
        let mut s = song();
        s.album = Some("Album [Disc 1]".into()); // would say 1 if consulted
        s.disc = Some("2".into());
        assert_eq!(s.effective_disc(), 2);
    }

    #[test]
    fn effective_disc_parses_fraction_form() {
        let mut s = song();
        s.disc = Some("2/2".into());
        assert_eq!(s.effective_disc(), 2);
    }

    #[test]
    fn effective_disc_falls_back_to_name_suffix() {
        let mut s = song();
        s.album = Some("Blast from the Past [Disc 2]".into());
        assert_eq!(s.effective_disc(), 2);
    }

    #[test]
    fn effective_disc_defaults_to_one() {
        let mut s = song();
        s.album = Some("Plain Album".into());
        assert_eq!(s.effective_disc(), 1);
    }

    // --- album_base_and_disc -----------------------------------------------

    #[test]
    fn album_base_and_disc_bracket_forms() {
        assert_eq!(album_base_and_disc("Blast [Disc 1]"), ("Blast".to_string(), Some(1)));
        assert_eq!(album_base_and_disc("Blast (Disc 2)"), ("Blast".to_string(), Some(2)));
        assert_eq!(album_base_and_disc("Blast [Disk 3]"), ("Blast".to_string(), Some(3)));
        assert_eq!(album_base_and_disc("Blast (CD 1)"), ("Blast".to_string(), Some(1)));
    }

    #[test]
    fn album_base_and_disc_bare_trailing_forms() {
        assert_eq!(album_base_and_disc("Blast CD2"), ("Blast".to_string(), Some(2)));
        assert_eq!(album_base_and_disc("Blast Disc 2"), ("Blast".to_string(), Some(2)));
        assert_eq!(album_base_and_disc("Blast - Disc 1"), ("Blast".to_string(), Some(1)));
        assert_eq!(album_base_and_disc("Blast: disc 12"), ("Blast".to_string(), Some(12)));
    }

    #[test]
    fn album_base_and_disc_no_marker_passthrough() {
        assert_eq!(
            album_base_and_disc("Plain Album"),
            ("Plain Album".to_string(), None)
        );
    }

    #[test]
    fn album_base_and_disc_disc_alone_passthrough() {
        // Stripping would leave an empty base — reject.
        assert_eq!(album_base_and_disc("Disc 1"), ("Disc 1".to_string(), None));
    }

    #[test]
    fn album_base_and_disc_live_cd_no_false_match() {
        // "CD" with no digits after it is not a disc marker.
        assert_eq!(album_base_and_disc("Live CD"), ("Live CD".to_string(), None));
    }

    #[test]
    fn album_base_and_disc_no_delimiter_no_false_match() {
        // "cd2" is glued onto "AB" with no delimiter — must not match.
        assert_eq!(album_base_and_disc("ABCD2"), ("ABCD2".to_string(), None));
    }

    #[test]
    fn album_base_and_disc_trims_trailing_space_and_dash() {
        let (base, disc) = album_base_and_disc("Blast   -   [Disc 1]");
        assert_eq!(base, "Blast");
        assert_eq!(disc, Some(1));
    }

    // --- disc-marker forms found in a real 9846-song library ------------
    //
    // Every string below is a genuine album tag from the library these rules
    // were validated against. Keeping the real names (rather than tidy
    // invented ones) is the point: each captures a tagging habit that
    // actually occurs, and the negative cases are the ones a looser matcher
    // would wrongly strip.

    #[test]
    fn album_base_and_disc_letter_disc_ids() {
        assert_eq!(album_base_and_disc("101 [Disc A]"), ("101".into(), Some(1)));
        assert_eq!(album_base_and_disc("101 [Disc B]"), ("101".into(), Some(2)));
    }

    #[test]
    fn album_base_and_disc_spelled_out_number() {
        assert_eq!(album_base_and_disc("Lotus (Disc One)"), ("Lotus".into(), Some(1)));
        assert_eq!(album_base_and_disc("X (Disc Three)"), ("X".into(), Some(3)));
    }

    #[test]
    fn album_base_and_disc_of_total_forms() {
        assert_eq!(
            album_base_and_disc("Decade of Aggression - Disc 1 of 2"),
            ("Decade of Aggression".into(), Some(1)),
        );
        assert_eq!(
            album_base_and_disc("Nostradamus (2CD) (CD 1/2)"),
            ("Nostradamus".into(), Some(1)),
        );
    }

    /// A marker at the tail of a qualifier bracket strips only the marker —
    /// the qualifier stays, so a remastered set collapses across its discs
    /// without merging into a differently-mastered copy.
    #[test]
    fn album_base_and_disc_marker_inside_a_qualifier_bracket() {
        assert_eq!(
            album_base_and_disc("Clutching at Straws [24-bit Remaster CD 1]"),
            ("Clutching at Straws [24-bit Remaster]".into(), Some(1)),
        );
        assert_eq!(
            album_base_and_disc("Misplaced Childhood [24-bit Remaster, CD 2]"),
            ("Misplaced Childhood [24-bit Remaster]".into(), Some(2)),
        );
        // Both discs of one set must land on the same base.
        assert_eq!(
            album_base_and_disc("Clutching at Straws [24-bit Remaster CD 1]").0,
            album_base_and_disc("Clutching at Straws [24-bit Remaster CD 2]").0,
        );
    }

    #[test]
    fn album_base_and_disc_strips_disc_count_qualifier() {
        assert_eq!(album_base_and_disc("X (2CD)"), ("X".into(), None));
        assert_eq!(album_base_and_disc("X (3 CDs)"), ("X".into(), None));
        assert_eq!(album_base_and_disc("X [2 Disc]"), ("X".into(), None));
        // The count bracket must go whether or not a disc marker follows it,
        // or the two discs of Nostradamus never meet.
        assert_eq!(
            album_base_and_disc("Nostradamus (2CD) (CD 1/2)").0,
            album_base_and_disc("Nostradamus (disc 2)").0,
        );
    }

    /// Names that look marker-ish but must be left completely alone. These
    /// are the false positives a wider matcher buys.
    #[test]
    fn album_base_and_disc_real_world_non_markers_pass_through() {
        for name in [
            "Killers (CDM 7520192)",                        // catalogue number
            "Screaming For Vengeance [2001 CD Edition]",     // an edition
            "Journeyman [2014 Audio Fidelity SACD AFZ 180]", // an edition
            "Lightbulb Sun (Special Edition)",               // no marker at all
            "Live CD",                                       // marker, no disc id
        ] {
            assert_eq!(
                album_base_and_disc(name),
                (name.to_string(), None),
                "{name:?} must pass through untouched"
            );
        }
    }

    /// A letter or spelled-out number is only a disc id when something
    /// separates it from the marker word — otherwise "(CDs)" reads as
    /// "CD, disc S". Digits may still abut ("CD2"), which is a real form.
    #[test]
    fn album_base_and_disc_letter_requires_a_separator() {
        assert_eq!(album_base_and_disc("X (CDs)"), ("X (CDs)".into(), None));
        assert_eq!(album_base_and_disc("X (CDone)"), ("X (CDone)".into(), None));
        assert_eq!(album_base_and_disc("X (CD2)"), ("X".into(), Some(2)));
    }

    /// Disc numbers are small. A 3-digit run after a marker word is far more
    /// likely to be a catalogue number or a year.
    #[test]
    fn album_base_and_disc_rejects_implausibly_large_disc_numbers() {
        assert_eq!(album_base_and_disc("X (CD 180)"), ("X (CD 180)".into(), None));
        assert_eq!(album_base_and_disc("X (CD 12)"), ("X".into(), Some(12)));
    }

    #[test]
    fn group_albums_folds_case_differences_in_the_base() {
        // Real pair: one album, two rips, inconsistent capitalisation.
        let pairs = gpairs(&[
            ("Supertramp", "Crime Of The Century"),
            ("Supertramp", "Crime of the Century"),
        ]);
        let groups = group_albums_by_artist(&pairs);
        assert_eq!(groups.len(), 1);
        // First-seen spelling is what gets displayed.
        assert_eq!(groups[0].base, "Crime Of The Century");
        assert_eq!(groups[0].variants.len(), 2);
    }

    #[test]
    fn group_albums_folds_case_across_disc_variants() {
        let pairs = gpairs(&[
            ("Slayer", "Decade Of Aggression - Disc 2"),
            ("Slayer", "Decade of Aggression - Disc 1 of 2"),
        ]);
        let groups = group_albums_by_artist(&pairs);
        assert_eq!(groups.len(), 1, "one album, not two");
        assert_eq!(groups[0].variants.len(), 2);
    }

    #[test]
    fn album_base_and_disc_non_ascii_lowercase_expansion_does_not_panic() {
        // 'İ' (U+0130, 2 bytes) lowercases to "i̇" (U+0069 U+0307, 3 bytes) —
        // matching against a lowercased copy but slicing the original by
        // that copy's byte offsets used to panic (or, for some inputs,
        // silently mis-slice) here. These all have a valid marker at the
        // end, so the panic-free, *correct* result is a successful strip,
        // not a passthrough — verified against a standalone reproduction
        // of this exact function, not asserted from a guess.
        assert_eq!(album_base_and_disc("İİ cd2"), ("İİ".to_string(), Some(2)));
        assert_eq!(album_base_and_disc("İİİ cd2"), ("İİİ".to_string(), Some(2)));
        assert_eq!(album_base_and_disc("İ - cd2"), ("İ".to_string(), Some(2)));
    }

    #[test]
    fn album_base_and_disc_non_ascii_prefix_still_strips_correctly() {
        let (base, disc) = album_base_and_disc("Aİ cd12");
        assert_eq!(base, "Aİ");
        assert_eq!(disc, Some(12));
    }

    // --- group_albums_by_artist ---------------------------------------------

    fn gpairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn group_albums_same_name_different_artist_stays_separate() {
        let pairs = gpairs(&[("Artist A", "Greatest Hits"), ("Artist B", "Greatest Hits")]);
        let groups = group_albums_by_artist(&pairs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].artist, "Artist A");
        assert_eq!(groups[1].artist, "Artist B");
    }

    #[test]
    fn group_albums_disc_suffix_variants_merge() {
        let pairs = gpairs(&[
            ("Gamma Ray", "Blast from the Past [Disc 1]"),
            ("Gamma Ray", "Blast from the Past [Disc 2]"),
        ]);
        let groups = group_albums_by_artist(&pairs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].base, "Blast from the Past");
        assert_eq!(groups[0].variants.len(), 2);
    }

    #[test]
    fn group_albums_preserves_first_seen_order() {
        let pairs = gpairs(&[("B", "Second"), ("A", "First")]);
        let groups = group_albums_by_artist(&pairs);
        assert_eq!(groups[0].base, "Second");
        assert_eq!(groups[1].base, "First");
    }

    #[test]
    fn group_albums_empty_input() {
        assert!(group_albums_by_artist(&[]).is_empty());
    }

    // --- album_disc_count ---------------------------------------------------

    #[test]
    fn album_disc_count_takes_max_of_both_signals() {
        // Properly tagged: one variant, disc tag goes up to 4.
        assert_eq!(album_disc_count(1, 4), 4);
        // Poorly tagged: two name variants, no disc tag.
        assert_eq!(album_disc_count(2, 0), 2);
        // Agreeing signals must not double-count.
        assert_eq!(album_disc_count(2, 2), 2);
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
            album_artist: String::new(),
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
            RecentlyPlayedEntry { file: "1".into(), title: "T1".into(), artist: "A".into(), album_artist: String::new(), album: "X".into(), played_at: 300 },
            RecentlyPlayedEntry { file: "2".into(), title: "T2".into(), artist: "A".into(), album_artist: String::new(), album: "X".into(), played_at: 200 },
            RecentlyPlayedEntry { file: "3".into(), title: "T3".into(), artist: "B".into(), album_artist: String::new(), album: "Y".into(), played_at: 100 },
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
            RecentlyPlayedEntry { file: "1".into(), title: "T".into(), artist: "A".into(), album_artist: String::new(), album: "Greatest Hits".into(), played_at: 200 },
            RecentlyPlayedEntry { file: "2".into(), title: "T".into(), artist: "B".into(), album_artist: String::new(), album: "Greatest Hits".into(), played_at: 100 },
        ];
        assert_eq!(recently_played_albums(&entries).len(), 2);
    }

    #[test]
    fn recently_played_albums_empty_input() {
        assert!(recently_played_albums(&[]).is_empty());
    }

    #[test]
    fn art_artist_prefers_album_artist_and_falls_back_to_track_artist() {
        let mut e = RecentlyPlayedEntry {
            file: "1".into(),
            title: "T".into(),
            artist: "Guest Artist".into(),
            album_artist: "Various Artists".into(),
            album: "Comp".into(),
            played_at: 0,
        };
        assert_eq!(e.art_artist(), "Various Artists");
        // Entries persisted before `album_artist` existed deserialize to "".
        e.album_artist = String::new();
        assert_eq!(e.art_artist(), "Guest Artist");
    }

    #[test]
    fn recently_played_albums_art_key_matches_song_art_key_on_a_compilation() {
        // The bug this guards: history recorded the *track* artist, while
        // art is always cached under `Song::art_key()` (the *album* artist),
        // so compilation tiles never found their art.
        let mut track = song();
        track.artist = Some("Guest Artist".into());
        track.album_artist = Some("Various Artists".into());
        track.album = Some("Comp [Disc 1]".into());

        let entries = vec![RecentlyPlayedEntry {
            file: "1".into(),
            title: "T".into(),
            artist: track.display_artist().to_string(),
            album_artist: track.display_album_artist().to_string(),
            album: track.display_album().to_string(),
            played_at: 0,
        }];
        let groups = recently_played_albums(&entries);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            art_key_for(&groups[0].artist, &groups[0].album),
            track.art_key(),
        );
    }

    #[test]
    fn recently_played_albums_collapse_guest_artists_of_one_compilation() {
        let mk = |artist: &str, played_at: i64| RecentlyPlayedEntry {
            file: artist.into(),
            title: "T".into(),
            artist: artist.into(),
            album_artist: "Various Artists".into(),
            album: "Comp".into(),
            played_at,
        };
        let entries = vec![mk("Guest A", 300), mk("Guest B", 200)];
        let groups = recently_played_albums(&entries);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].artist, "Various Artists");
        assert_eq!(groups[0].last_played, 300);
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
