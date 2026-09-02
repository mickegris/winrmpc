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
    /// MPD 0.24's `Added` — when the file entered the database, as opposed to
    /// `last_modified`'s filesystem mtime. `#[serde(default)]` because older
    /// servers never send it.
    #[serde(default)]
    pub added: Option<String>,
    pub composer: Option<String>,
    pub performer: Option<String>,
    pub comment: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, Vec<String>>,
}

/// A tag's value, treating **present-but-blank as absent**.
///
/// A tag written as an empty string is common in sloppy rips, and without this
/// the fallback chain would stop at it and render an empty cell — arguably
/// worse than "Unknown Artist", because it looks like a rendering fault rather
/// than missing data.
///
/// The *original* value is returned, not a trimmed one: trimming would change
/// `display_album_artist`, which feeds `art_key`, and silently orphan cached
/// art for any file with a padded tag.
fn tag_or(tag: &Option<String>) -> Option<&str> {
    tag.as_deref().filter(|v| !v.trim().is_empty())
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

    /// The track artist, falling back to the **album artist** before giving up.
    ///
    /// Plenty of real files carry `AlbumArtist` and no `Artist` — a rip where
    /// only the album-level tag was written. Those showed as "Unknown Artist"
    /// in the queue and every track list while the album page, which uses
    /// [`Self::display_album_artist`], showed the name perfectly well. Falling
    /// back mirrors that method and makes the two agree.
    ///
    /// This is also what's sent to LRCLIB and MusicBrainz, so the fallback
    /// turns a guaranteed-miss lookup for "Unknown Artist" into one that can
    /// actually match.
    pub fn display_artist(&self) -> &str {
        tag_or(&self.artist)
            .or_else(|| tag_or(&self.album_artist))
            .unwrap_or(UNKNOWN_ARTIST)
    }

    pub fn display_album(&self) -> &str {
        tag_or(&self.album).unwrap_or(UNKNOWN_ALBUM)
    }

    /// The album artist, falling back to the track artist. Mirror image of
    /// [`Self::display_artist`], so the two agree on any file that carries
    /// only one of the two tags.
    pub fn display_album_artist(&self) -> &str {
        tag_or(&self.album_artist)
            .or_else(|| tag_or(&self.artist))
            .unwrap_or(UNKNOWN_ARTIST)
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
/// The placeholders `Song::display_artist`/`display_album`/
/// `display_album_artist` fall back to when a tag is missing or blank.
///
/// Defined here, where they are *produced*, rather than in the UI that
/// happens to test for them: they are values that flow through art keys,
/// grouping and external lookups, and every one of those has to recognise a
/// placeholder as "no data" rather than as a name. `link::is_real_name` is
/// the UI-side test that gates them out of clickable links.
pub const UNKNOWN_ARTIST: &str = "Unknown Artist";
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumGroup {
    pub artist: String,
    pub base: String,
    pub variants: Vec<String>,
}

/// Folds the punctuation that two rips of one album disagree about, so a
/// grouping key can survive the difference. Length is *not* preserved and
/// the result is never displayed — it exists only to be compared.
///
/// | Folded | To | Why |
/// |---|---|---|
/// | `–` `—` `−` `‐` `‑` | `-` | The Beatles' `1967-1970` (ASCII hyphen) and `1967–1970` (en dash) are two directories and two album tags for one 2-disc set |
/// | `‘` `’` `‛` | `'` | `Rockin' ` vs `Rockin’ ` |
/// | `“` `”` `‟` | `"` | same, for titles that quote |
/// | `…` | `...` | one character or three, depending on the tagger |
/// | runs of whitespace | one space | this library really does hold `"Blue  Oyster Cult"` |
///
/// Then trimmed and lowercased. This is deliberately *not*
/// `art::musicbrainz::normalize_for_lookup`, which does all of the above and
/// then moves a sort-order article to the front (`"Beatles, The"` →
/// `"The Beatles"`). That transform is right for asking a search engine a
/// question and wrong here: it would collapse two genuinely differently
/// tagged artists into one row on a guess, and `types.rs` has no business
/// depending on the lookup layer.
fn fold_for_grouping(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            // Collapse the run; emit it only once something follows, which
            // also trims the tail for free.
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        match ch {
            '\u{2013}' | '\u{2014}' | '\u{2212}' | '\u{2010}' | '\u{2011}' => out.push('-'),
            '\u{2018}' | '\u{2019}' | '\u{201B}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201F}' => out.push('"'),
            '\u{2026}' => out.push_str("..."),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out
}

/// The identity of one album row: artist and disc-stripped title, both
/// folded by [`fold_for_grouping`].
///
/// **Every consumer of album identity must go through this**, or a row
/// won't find its own disc data — mikMPD learned that the hard way and its
/// note is emphatic about it. Today that means
/// [`group_albums_by_artist`]'s map key and the variant lookup in
/// `AlbumSelected` (`app.rs`), which re-derives the artist's groups and has
/// to match the base it was handed.
///
/// The two halves are joined by the same ASCII Unit Separator the art keys
/// use, so an artist ending in the album's first characters can't be
/// confused for a different split.
pub fn album_grouping_key(artist: &str, base: &str) -> String {
    format!("{}\x1f{}", fold_for_grouping(artist), fold_for_grouping(base))
}

/// Case- and punctuation-insensitive containment, folded by the same rules
/// the album grouping key uses.
///
/// `folded_needle` must already have been through [`fold_query`] — the
/// caller folds once and tests many times, which is the whole reason this
/// takes a pre-folded needle instead of a raw one.
pub fn folded_contains(haystack: &str, folded_needle: &str) -> bool {
    fold_for_grouping(haystack).contains(folded_needle)
}

/// Prepares a user-typed query for [`folded_contains`].
pub fn fold_query(query: &str) -> String {
    fold_for_grouping(query)
}

/// The Artists and Albums sections of the Search view, derived from the
/// songs one `search any` already returned.
#[derive(Debug, Default, PartialEq)]
pub struct SearchSections {
    pub artists: Vec<String>,
    pub albums: Vec<AlbumGroup>,
}

/// Splits a flat search result into the entities it mentions.
///
/// **Derived from the one result set rather than fired as three separate
/// queries**, which is where this departs from the plan. `search any` has
/// already matched every tag, so the artists and albums are in the reply —
/// three queries would cost three round trips on the one shared connection
/// to rediscover them, and could disagree with the songs on screen if the
/// database changed between them.
///
/// The sections list entities whose **own name** matches. A search for
/// `beatles` should offer the artist and their albums; it should not list
/// every artist who happens to have a song called "Beatles" — and, more to
/// the point, a search for `love` must not promote all 300 artists who
/// recorded a song with "love" in the title into an artist section. The
/// Songs section is the one that keeps every match.
pub fn search_sections(results: &[Song], query: &str) -> SearchSections {
    let needle = fold_query(query);
    if needle.is_empty() {
        return SearchSections::default();
    }

    let mut artists: Vec<String> = Vec::new();
    let mut album_pairs: Vec<(String, String)> = Vec::new();
    for song in results {
        // Both artist tags are candidates: a compilation matches on the
        // track artist, an album-artist-only rip on the other, and
        // `display_*` already resolves one to the other when either is
        // missing, so the pair is at worst the same name twice.
        for name in [song.display_artist(), song.display_album_artist()] {
            // De-duplicated on the **exact** spelling, deliberately, even
            // though the *matching* above is folded. Each distinct spelling
            // is a distinct tag holding distinct songs — this library really
            // does have both `"Blue Oyster Cult"` and `"Blue  Oyster Cult"`
            // — and an artist row navigates by name, so folding the two into
            // one row would leave the other's albums unreachable. The
            // Artists list has the same duplicates for the same reason, so
            // this also keeps the two views telling the same story.
            if name != UNKNOWN_ARTIST
                && folded_contains(name, &needle)
                && !artists.iter().any(|a| a == name)
            {
                artists.push(name.to_string());
            }
        }
        let album = song.display_album();
        if album != UNKNOWN_ALBUM && folded_contains(album, &needle) {
            album_pairs.push((song.display_album_artist().to_string(), album.to_string()));
        }
    }

    artists.sort_by(|a, b| name_cmp(a, b));
    let mut albums = group_albums_by_artist(&album_pairs);
    // Album name first, artist as the tiebreak — the same rule the Albums
    // list follows, and for the same reason: the column being read is the
    // titles.
    albums.sort_by(|a, b| name_cmp(&a.base, &b.base).then_with(|| name_cmp(&a.artist, &b.artist)));

    SearchSections { artists, albums }
}

/// Groups `(album_artist, album)` pairs (as returned by
/// `MpdClient::list_albums_by_artist`) into artist-aware, disc-collapsed
/// rows. Keyed on `(artist.to_lowercase(), base.to_lowercase())`, so
/// same-named albums by different artists stay separate rows while
/// disc-suffixed variants of the same artist's album collapse into one.
/// Preserves first-seen order.
///
/// The **base** is case- and punctuation-folded for the key (though the
/// first-seen spelling is what gets displayed) — see
/// [`album_grouping_key`], which owns the folding rules and the reasons
/// for them.
pub fn group_albums_by_artist(pairs: &[(String, String)]) -> Vec<AlbumGroup> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<AlbumGroup> = Vec::new();
    for (artist, album) in pairs {
        let base = album_base_and_disc(album).0;
        let key = album_grouping_key(artist, &base);
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

impl RecentAlbum {
    /// This entry's album identity, for de-duplication.
    ///
    /// Disc-stripped **and** punctuation-folded, like every other album key
    /// in the app. Without the stripping, `"Nostradamus (CD 1/2)"` and
    /// `"Nostradamus (disc 2)"` are two entries for one album, and the
    /// strip is only eight slots deep — playing one 3-disc set used to
    /// consume nearly half of it and push out four genuinely different
    /// albums.
    ///
    /// Strips here rather than trusting the caller so the invariant holds
    /// for a `recent_albums` list loaded from an older config file, whose
    /// entries were written raw.
    pub fn grouping_key(&self) -> String {
        album_grouping_key(&self.artist, &album_base_and_disc(&self.album).0)
    }
}

/// Insert `entry` at the front of `recents`, deduplicating and capping at 8.
///
/// De-dup is by [`RecentAlbum::grouping_key`], not by field equality, so
/// moving between the discs of one set moves that one entry to the front
/// instead of adding a second.
///
/// Extracted as a pure function so it can be unit-tested without the GUI.
pub fn push_recent(recents: &mut Vec<RecentAlbum>, entry: RecentAlbum) {
    // Remove any existing occurrence (move-to-front semantics)
    let key = entry.grouping_key();
    recents.retain(|r| r.grouping_key() != key);
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

/// The comparator behind every A-Z / Z-A library list.
///
/// **Not `str::cmp`.** A plain `Vec<String>::sort()` is byte order, so every
/// lowercase initial files after every uppercase one: `ZZ Top` (`Z` = 0x5A)
/// sorts before `a-ha` (`a` = 0x61). A real library has `a-ha`, `dEUS`,
/// `k.d. lang` and `will.i.am`, all of which were exiled past Z in the
/// Artists list until this existed.
///
/// Case-folded first, then a byte-order tiebreak so the ordering stays
/// **total** — names differing only in case must still have a defined
/// order, or reversing the list twice isn't the identity and rows shuffle
/// each time the direction is toggled.
pub fn name_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let folded = a.to_lowercase().cmp(&b.to_lowercase());
    folded.then_with(|| a.cmp(b))
}

/// `name_cmp`, reversed when `desc`.
pub fn name_cmp_dir(a: &str, b: &str, desc: bool) -> std::cmp::Ordering {
    let ord = name_cmp(a, b);
    if desc {
        ord.reverse()
    } else {
        ord
    }
}

/// Accumulates each album's **newest** add-time from one page of raw `find`
/// pairs, without ever building a `Song`.
///
/// Deliberately not `parse_songs` + a fold: a full-library scan is the one
/// place where allocating a `Song` per track is most of the cost, and every
/// field but three is thrown away immediately.
///
/// **`AlbumArtist` falls back to `Artist`, because MPD's own grouping does.**
/// `list Album group AlbumArtist` reports `Dio` for an album whose song
/// records carry `Artist: Dio` and no `AlbumArtist` at all — the server
/// substitutes the fallback tag. Keying on the raw tag here therefore built
/// `"\x1fHoly Diver"` for an album row keyed `"Dio\x1fHoly Diver"`, and on
/// this library that was **more than half of them**: 348 of 801 rows matched.
/// `live_diagnose_album_added_coverage` is what found it.
pub fn fold_album_added(pairs: &[(String, String)], out: &mut HashMap<String, String>) {
    let mut album_artist = String::new();
    let mut artist = String::new();
    let mut album = String::new();
    let mut added = String::new();

    let mut flush = |album_artist: &mut String,
                     artist: &mut String,
                     album: &mut String,
                     added: &mut String| {
        if !album.is_empty() && !added.is_empty() {
            let base = album_base_and_disc(album).0;
            let credited = if album_artist.is_empty() { &*artist } else { &*album_artist };
            let key = album_scoped_key(Some(credited), &base);
            out.entry(key)
                .and_modify(|cur| {
                    if *added > *cur {
                        cur.clone_from(added);
                    }
                })
                .or_insert_with(|| added.clone());
        }
        album_artist.clear();
        artist.clear();
        album.clear();
        added.clear();
    };

    for (k, v) in pairs {
        match k.as_str() {
            // `file` opens a new song record, so it closes the previous one.
            "file" => flush(&mut album_artist, &mut artist, &mut album, &mut added),
            "AlbumArtist" => album_artist.clone_from(v),
            "Artist" => artist.clone_from(v),
            "Album" => album.clone_from(v),
            // Whichever the rung produced. `Added` wins if both are present.
            "Added" => added.clone_from(v),
            "Last-Modified" if added.is_empty() => added.clone_from(v),
            _ => {}
        }
    }
    flush(&mut album_artist, &mut artist, &mut album, &mut added);
}

/// What an album list is ordered by.
///
/// Only the album lists offer a choice — Artists, Genres and Playlists have
/// nothing but a name, so they always use [`name_cmp`] and show only the
/// direction control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SortKey {
    #[default]
    /// Album title. Deliberately **not** artist-then-title: an A-Z album list
    /// is expected to run A-Z by the name on the row, and sorting by artist
    /// first makes it look unsorted to anyone reading the titles.
    Name,
    /// When the file entered MPD's database (0.24's `Added`, else mtime).
    Added,
}

impl SortKey {
    pub const ALL: [SortKey; 2] = [SortKey::Name, SortKey::Added];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Added => "Added",
        }
    }

    /// What the direction control should read for this key. "A-Z" is
    /// meaningless for a date and "Oldest" is meaningless for a title, so the
    /// button's wording follows the key rather than being fixed.
    pub fn direction_label(self, desc: bool) -> &'static str {
        match (self, desc) {
            (SortKey::Name, false) => "A\u{2013}Z",
            (SortKey::Name, true) => "Z\u{2013}A",
            (_, false) => "Oldest",
            (_, true) => "Newest",
        }
    }
}

impl std::fmt::Display for SortKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Orders two albums for a list.
///
/// `a_added` / `b_added` are the albums' add-times, which don't live on
/// `AlbumGroup` because they arrive from a completely different (and much
/// more expensive) query than the album list itself — see
/// `App::album_added`. Passing them in keeps this a pure function.
///
/// **A missing add-time sorts last in *both* directions.** The unknown-ness
/// is checked outside the reversal on purpose: an album the walk never
/// reached jumping to the top of a "newest first" list would read as data,
/// and the same rule already governs `last_modified` in Recently Added.
///
/// The name is always the final tiebreak, so the order stays total and
/// flipping the direction twice restores the list rather than shuffling
/// albums added at the same moment.
pub fn album_cmp(
    a: &AlbumGroup,
    a_added: Option<&str>,
    b: &AlbumGroup,
    b_added: Option<&str>,
    key: SortKey,
    desc: bool,
) -> std::cmp::Ordering {
    let by_name = || {
        name_cmp(&a.base, &b.base)
            .then_with(|| name_cmp(&a.artist, &b.artist))
            .then_with(|| a.base.cmp(&b.base))
            .then_with(|| a.artist.cmp(&b.artist))
    };

    match key {
        SortKey::Name => by_name().pipe_reverse(desc),
        SortKey::Added => unknown_last(a_added, b_added, |x, y| x.cmp(y), desc, by_name),
    }
}

/// Compares two optional keys with `Some` always ahead of `None`, reversing
/// only the comparison between two `Some`s. Ties fall through to `tiebreak`,
/// which is **not** reversed either — albums sharing an add-time stay A-Z
/// whichever way the dates run.
fn unknown_last<T>(
    a: Option<T>,
    b: Option<T>,
    cmp: impl Fn(T, T) -> std::cmp::Ordering,
    desc: bool,
    tiebreak: impl Fn() -> std::cmp::Ordering,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Some(x), Some(y)) => cmp(x, y).pipe_reverse(desc).then_with(tiebreak),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => tiebreak(),
    }
}

/// Small helper so the `.then_with` chains above read in one direction.
trait PipeReverse {
    fn pipe_reverse(self, yes: bool) -> Self;
}

impl PipeReverse for std::cmp::Ordering {
    fn pipe_reverse(self, yes: bool) -> Self {
        if yes {
            self.reverse()
        } else {
            self
        }
    }
}

/// Which `find` form a server accepts for the Recently Added query.
///
/// The three rungs differ in *what* they call "recent" as much as in syntax,
/// and the top one is the only one that means what the view says:
///
/// - `Added` is MPD 0.24's real database add-time. Immune to the mtime
///   problem below.
/// - `Last-Modified` is the file's mtime, which lies in both directions:
///   `cp -p` / `rsync -a` / a restored backup carry the original date
///   forward, so files added today can be years old and never appear, while
///   editing a tag on an old file resurfaces it as "recently added".
/// - The bottom rung is mtime *unsorted*, for servers with no `sort` clause
///   (pre-0.22). There the `window` truncates MPD's database order — an
///   effectively arbitrary slice — so the caller must sort what it gets and
///   accept that the set itself may be wrong. It exists so an ancient server
///   shows something rather than nothing.
///
/// Discriminants are the on-the-wire cache value in `MpdClient`; `0` is
/// reserved for "not probed yet", so they start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecentlyAddedRung {
    /// MPD 0.24+ — database add-time, sorted server-side.
    AddedSince = 1,
    /// MPD 0.22+ — file mtime, sorted server-side.
    ModifiedSinceSorted = 2,
    /// Pre-0.22 — file mtime, unsorted; the caller must sort.
    ModifiedSinceUnsorted = 3,
}

impl RecentlyAddedRung {
    /// The rung tried first on an unprobed server.
    pub const TOP: Self = Self::AddedSince;

    pub fn from_repr(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::AddedSince),
            2 => Some(Self::ModifiedSinceSorted),
            3 => Some(Self::ModifiedSinceUnsorted),
            _ => None,
        }
    }

    /// This rung and every weaker one, in the order they should be tried.
    pub fn from(start: Self) -> impl Iterator<Item = Self> {
        [
            Self::AddedSince,
            Self::ModifiedSinceSorted,
            Self::ModifiedSinceUnsorted,
        ]
        .into_iter()
        .skip_while(move |r| *r != start)
    }

    /// Whether the server orders the result for us. When false the caller's
    /// own sort is the only ordering there is — and the `window` has already
    /// chosen *which* rows, so sorting can't recover the ones it dropped.
    pub fn is_server_sorted(self) -> bool {
        !matches!(self, Self::ModifiedSinceUnsorted)
    }

    /// The `find` command for this rung. `since` must already be escaped.
    ///
    /// `sort` runs **before** `window` in MPD, which is the whole point: a
    /// descending sort (the `-` prefix) makes the limit drop the *oldest*
    /// matches instead of an arbitrary slice of database order.
    pub fn query(self, since: &str, limit: u32) -> String {
        match self {
            Self::AddedSince => {
                format!("find \"(added-since '{since}')\" sort -Added window 0:{limit}")
            }
            Self::ModifiedSinceSorted => format!(
                "find \"(modified-since '{since}')\" sort -Last-Modified window 0:{limit}"
            ),
            Self::ModifiedSinceUnsorted => {
                format!("find \"(modified-since '{since}')\" window 0:{limit}")
            }
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

    /// A file tagged with only `AlbumArtist` used to read "Unknown Artist" in
    /// the queue while its album page showed the name — the two methods
    /// disagreed about the same file.
    #[test]
    fn display_artist_falls_back_to_the_album_artist() {
        let mut s = song();
        assert_eq!(s.display_artist(), "Unknown Artist");

        s.album_artist = Some("Blue Öyster Cult".into());
        assert_eq!(
            s.display_artist(),
            "Blue Öyster Cult",
            "an AlbumArtist-only file must not read as Unknown"
        );
        assert_eq!(s.display_artist(), s.display_album_artist());

        // A real track artist still wins — on a compilation the two differ,
        // and the row is showing the *track's* artist.
        s.artist = Some("Nina Simone".into());
        assert_eq!(s.display_artist(), "Nina Simone");
        assert_eq!(s.display_album_artist(), "Blue Öyster Cult");
    }

    /// A tag written as an empty string must not stop the fallback chain — it
    /// would render a blank cell, which reads as a rendering fault rather than
    /// as missing data.
    #[test]
    fn a_blank_tag_counts_as_absent() {
        let mut s = song();
        s.artist = Some("   ".into());
        s.album_artist = Some("Eagles".into());
        assert_eq!(s.display_artist(), "Eagles");

        s.album = Some(String::new());
        assert_eq!(s.display_album(), "Unknown Album");

        s.album_artist = Some("".into());
        s.artist = Some("".into());
        assert_eq!(s.display_artist(), "Unknown Artist");
        assert_eq!(s.display_album_artist(), "Unknown Artist");
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

    // --- album_grouping_key -------------------------------------------------

    /// The case mikMPD documents: one 2-disc set whose two rips disagree by
    /// a single character. Without folding these are two rows, each captioned
    /// as a single disc and each holding half the tracks.
    #[test]
    fn an_en_dash_and_a_hyphen_are_one_album() {
        let pairs = vec![
            ("The Beatles".into(), "1967-1970 [Disc 1]".into()),
            ("The Beatles".into(), "1967\u{2013}1970 [Disc 2]".into()),
        ];
        let groups = group_albums_by_artist(&pairs);

        assert_eq!(groups.len(), 1, "one album, not two: {groups:?}");
        assert_eq!(groups[0].variants.len(), 2, "both discs land in it");
        assert_eq!(
            groups[0].base, "1967-1970",
            "the first-seen spelling is what gets displayed"
        );
    }

    #[test]
    fn smart_quotes_ellipses_and_doubled_spaces_all_fold() {
        for (a, b) in [
            ("Rockin\u{2019} the Joint", "Rockin' the Joint"),
            ("Wish You Were Here\u{2026}", "Wish You Were Here..."),
            ("Blue  Oyster Cult", "Blue Oyster Cult"),
            ("  Trimmed  ", "Trimmed"),
            ("\u{201C}Heroes\u{201D}", "\"Heroes\""),
        ] {
            assert_eq!(
                album_grouping_key("Artist", a),
                album_grouping_key("Artist", b),
                "{a:?} and {b:?} should be one album"
            );
        }
    }

    /// Folding must not start merging albums that genuinely differ, and the
    /// artist must still separate two same-titled albums.
    #[test]
    fn folding_does_not_collapse_genuinely_different_albums() {
        assert_ne!(
            album_grouping_key("A", "Greatest Hits"),
            album_grouping_key("A", "Greatest Hits II")
        );
        assert_ne!(
            album_grouping_key("Queen", "Greatest Hits"),
            album_grouping_key("ABBA", "Greatest Hits")
        );
        // The separator stops an artist's tail being read as the album's head.
        assert_ne!(
            album_grouping_key("AB", "C"),
            album_grouping_key("A", "BC")
        );
    }

    /// The key is what `AlbumSelected` matches a row against, so it has to be
    /// stable under the disc stripping the grouping already did.
    #[test]
    fn the_key_is_case_folded_like_the_grouping_it_replaces() {
        assert_eq!(
            album_grouping_key("Slayer", "Decade Of Aggression"),
            album_grouping_key("slayer", "decade of aggression")
        );
    }

    // --- search_sections ----------------------------------------------------

    fn track(artist: &str, album_artist: &str, album: &str, title: &str) -> Song {
        Song {
            file: format!("{artist}/{album}/{title}.flac"),
            artist: Some(artist.into()),
            album_artist: Some(album_artist.into()),
            album: Some(album.into()),
            title: Some(title.into()),
            ..Song::default()
        }
    }

    /// The sections list entities whose **own name** matches. A search for
    /// "love" must not promote every artist who recorded a song with "love"
    /// in the title into an Artists section — that is the Songs section's job.
    #[test]
    fn sections_only_list_entities_whose_own_name_matches() {
        let results = vec![
            track("The Beatles", "The Beatles", "Love", "All You Need Is Love"),
            track("Nirvana", "Nirvana", "Nevermind", "Lithium"),
            track("Nirvana", "Nirvana", "Nevermind", "Love Buzz"),
        ];
        let s = search_sections(&results, "love");

        assert!(s.artists.is_empty(), "no artist is called 'love': {:?}", s.artists);
        assert_eq!(s.albums.len(), 1);
        assert_eq!(s.albums[0].base, "Love");
    }

    #[test]
    fn an_artist_search_lists_the_artist_and_their_matching_albums() {
        let results = vec![
            track("The Beatles", "The Beatles", "Revolver", "Taxman"),
            track("The Beatles", "The Beatles", "Beatles For Sale", "No Reply"),
        ];
        let s = search_sections(&results, "beatles");

        assert_eq!(s.artists, vec!["The Beatles".to_string()]);
        // "Revolver" does not contain "beatles"; only the album that does.
        assert_eq!(s.albums.len(), 1);
        assert_eq!(s.albums[0].base, "Beatles For Sale");
    }

    /// Both artist tags are candidates, so a compilation's guest artist is
    /// findable and an album-artist-only rip still surfaces.
    #[test]
    fn both_artist_tags_are_searched() {
        let results = vec![track("Bobby Womack", "Various Artists", "Jackie Brown", "Across 110th Street")];

        assert_eq!(search_sections(&results, "womack").artists, vec!["Bobby Womack".to_string()]);
        assert_eq!(
            search_sections(&results, "various").artists,
            vec!["Various Artists".to_string()]
        );
    }

    /// The sections are disc-collapsed and artist-scoped like every other
    /// album list, because they are built by the same grouping function.
    #[test]
    fn album_sections_collapse_discs_and_separate_artists() {
        let results = vec![
            track("Slayer", "Slayer", "Decade Of Aggression - Disc 1", "Hell Awaits"),
            track("Slayer", "Slayer", "Decade of Aggression - Disc 2 of 2", "Angel Of Death"),
            track("Queen", "Queen", "Greatest Hits", "We Will Rock You"),
            track("ABBA", "ABBA", "Greatest Hits", "SOS"),
        ];

        let discs = search_sections(&results, "aggression");
        assert_eq!(discs.albums.len(), 1, "one album, two discs: {:?}", discs.albums);
        assert_eq!(discs.albums[0].variants.len(), 2);

        let hits = search_sections(&results, "greatest");
        assert_eq!(hits.albums.len(), 2, "two artists, two rows");
        assert_eq!(hits.albums[0].artist, "ABBA", "sorted by title then artist");
    }

    #[test]
    fn the_placeholders_are_never_offered_as_entities() {
        let mut s = song();
        s.file = "x.flac".into();
        let results = vec![s];
        // `display_artist()`/`display_album()` are the placeholders here, and
        // navigating to one renders an empty page.
        let sections = search_sections(&results, "unknown");
        assert!(sections.artists.is_empty());
        assert!(sections.albums.is_empty());
    }

    #[test]
    fn an_empty_or_blank_query_yields_no_sections() {
        let results = vec![track("A", "A", "B", "C")];
        assert_eq!(search_sections(&results, ""), SearchSections::default());
        assert_eq!(search_sections(&results, "   "), SearchSections::default());
    }

    /// Query folding matches the grouping's: a typed hyphen finds an en dash.
    #[test]
    fn the_query_is_folded_like_the_grouping_key() {
        let results = vec![track("The Beatles", "The Beatles", "1967\u{2013}1970", "Hey Jude")];
        let s = search_sections(&results, "1967-1970");
        assert_eq!(s.albums.len(), 1, "a typed hyphen must find an en dash");
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

    /// Two discs of one set are one entry. The strip is eight deep, so a
    /// 3-disc album used to eat nearly half of it and evict four genuinely
    /// different albums.
    #[test]
    fn push_recent_treats_the_discs_of_one_album_as_one_entry() {
        let mut v = Vec::new();
        push_recent(&mut v, RecentAlbum { artist: "Judas Priest".into(), album: "Nostradamus (CD 1/2)".into() });
        push_recent(&mut v, RecentAlbum { artist: "Judas Priest".into(), album: "Nostradamus (disc 2)".into() });

        assert_eq!(v.len(), 1, "one album: {v:?}");
        assert_eq!(v[0].album, "Nostradamus (disc 2)", "the newest spelling wins the slot");
    }

    /// Same folding as the album lists — an entry written by an older build
    /// (raw tag, en dash) must still match one recorded now.
    #[test]
    fn push_recent_folds_punctuation_like_the_album_lists() {
        let mut v = vec![RecentAlbum {
            artist: "The Beatles".into(),
            album: "1967\u{2013}1970 [Disc 2]".into(),
        }];
        push_recent(&mut v, RecentAlbum { artist: "The Beatles".into(), album: "1967-1970".into() });

        assert_eq!(v.len(), 1, "one album: {v:?}");
    }

    #[test]
    fn push_recent_still_separates_different_albums() {
        let mut v = Vec::new();
        push_recent(&mut v, RecentAlbum { artist: "A".into(), album: "One".into() });
        push_recent(&mut v, RecentAlbum { artist: "A".into(), album: "Two".into() });
        push_recent(&mut v, RecentAlbum { artist: "B".into(), album: "One".into() });
        assert_eq!(v.len(), 3);
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

    // --- name_cmp / list sorting --------------------------------------------

    #[test]
    fn name_cmp_files_lowercase_initials_where_they_belong() {
        // The bug this replaces: `Vec<String>::sort()` is byte order, so
        // every lowercase initial landed after every uppercase one. A real
        // library has all four of these and they all sat past Z.
        let mut v = vec!["ZZ Top", "a-ha", "will.i.am", "Blur", "dEUS", "k.d. lang"];
        v.sort_by(|a, b| name_cmp(a, b));
        assert_eq!(
            v,
            vec!["a-ha", "Blur", "dEUS", "k.d. lang", "will.i.am", "ZZ Top"]
        );
        // Explicitly: the byte comparison this replaced got it wrong.
        assert!("ZZ Top" < "a-ha", "premise of the test no longer holds");
    }

    #[test]
    fn name_cmp_is_a_total_order_across_case_only_differences() {
        // Case-folding alone makes these Equal, which is not a total order:
        // a sort could then leave them in either order and reversing twice
        // would shuffle the list. The byte tiebreak is what fixes that.
        assert_ne!(name_cmp("abba", "ABBA"), std::cmp::Ordering::Equal);
        assert_eq!(name_cmp("ABBA", "abba"), std::cmp::Ordering::Less);
        assert_eq!(name_cmp("abba", "abba"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn reversing_the_sort_twice_is_the_identity() {
        let names = ["ZZ Top", "abba", "ABBA", "a-ha", "Blur"];
        let sorted = |desc: bool| {
            let mut v = names.to_vec();
            v.sort_by(|a, b| name_cmp_dir(a, b, desc));
            v
        };
        let asc = sorted(false);
        let mut back = sorted(true);
        back.reverse();
        assert_eq!(asc, back, "A-Z and reversed Z-A must agree");
    }

    fn ag(artist: &str, base: &str) -> AlbumGroup {
        AlbumGroup {
            artist: artist.into(),
            base: base.into(),
            variants: vec![base.into()],
        }
    }

    fn sorted(src: &[AlbumGroup], key: SortKey, desc: bool) -> Vec<String> {
        sorted_with(src, key, desc, &HashMap::new())
    }

    fn sorted_with(
        src: &[AlbumGroup],
        key: SortKey,
        desc: bool,
        added: &HashMap<String, String>,
    ) -> Vec<String> {
        let mut v = src.to_vec();
        let look = |g: &AlbumGroup| added.get(&album_scoped_key(Some(&g.artist), &g.base)).cloned();
        v.sort_by(|a, b| {
            album_cmp(a, look(a).as_deref(), b, look(b).as_deref(), key, desc)
        });
        v.into_iter().map(|g| g.base).collect()
    }

    #[test]
    fn albums_sort_by_album_name_not_by_artist() {
        // The whole point of the fix: an A-Z album list must run A-Z by the
        // name on the row. Sorting by artist first put "Arrival" after
        // "Reign in Blood" and the list read as unsorted.
        let src = [
            ag("Slayer", "Reign in Blood"),
            ag("ABBA", "Arrival"),
            ag("Slayer", "Hell Awaits"),
            ag("ABBA", "Waterloo"),
        ];
        assert_eq!(
            sorted(&src, SortKey::Name, false),
            vec!["Arrival", "Hell Awaits", "Reign in Blood", "Waterloo"]
        );
        assert_eq!(
            sorted(&src, SortKey::Name, true),
            vec!["Waterloo", "Reign in Blood", "Hell Awaits", "Arrival"]
        );
    }

    #[test]
    fn two_artists_same_album_title_stay_deterministically_ordered() {
        // Name-only sorting makes these tie, so the artist is the tiebreak —
        // without it the pair could swap on every re-sort.
        let src = [
            ag("Slayer", "Greatest Hits"),
            ag("ABBA", "Greatest Hits"),
        ];
        let mut v = src.to_vec();
        v.sort_by(|a, b| album_cmp(a, None, b, None, SortKey::Name, false));
        assert_eq!(v[0].artist, "ABBA");
    }

    #[test]
    fn albums_sort_by_add_time() {
        let src = [ag("A", "First"), ag("A", "Second")];
        let mut added = HashMap::new();
        added.insert(
            album_scoped_key(Some("A"), "First"),
            "2020-01-01T00:00:00Z".to_string(),
        );
        added.insert(
            album_scoped_key(Some("A"), "Second"),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert_eq!(
            sorted_with(&src, SortKey::Added, false, &added),
            vec!["First", "Second"]
        );
        assert_eq!(
            sorted_with(&src, SortKey::Added, true, &added),
            vec!["Second", "First"]
        );
        // An album the walk never reached sorts last either way, rather than
        // pretending to be the oldest or the newest.
        assert_eq!(
            sorted_with(&src, SortKey::Added, true, &HashMap::new()),
            vec!["First", "Second"],
        );
    }

    #[test]
    fn reversing_an_album_sort_twice_is_the_identity_for_every_key() {
        let src = [
            ag("Slayer", "Reign in Blood"),
            ag("ABBA", "Arrival"),
            ag("ABBA", "Arrival"),
            ag("Nobody", "Undated"),
        ];
        for key in SortKey::ALL {
            let mut back = sorted(&src, key, true);
            back.reverse();
            if key == SortKey::Name {
                assert_eq!(sorted(&src, key, false), back, "{key:?}");
            } else {
                // With unknowns pinned last in both directions, reversing
                // can't be a pure mirror — but the known ones must still
                // mirror, and the unknown must stay at the end.
                assert_eq!(sorted(&src, key, false).last().unwrap(), "Undated");
                assert_eq!(sorted(&src, key, true).last().unwrap(), "Undated");
            }
        }
    }

    #[test]
    fn fold_album_added_keeps_the_newest_track_per_album() {
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let pairs = vec![
            p("file", "a/1.flac"),
            p("Album", "Reign in Blood"),
            p("AlbumArtist", "Slayer"),
            p("Added", "2020-01-01T00:00:00Z"),
            p("file", "a/2.flac"),
            p("Album", "Reign in Blood"),
            p("AlbumArtist", "Slayer"),
            p("Added", "2026-03-03T00:00:00Z"),
        ];
        let mut out = HashMap::new();
        fold_album_added(&pairs, &mut out);
        assert_eq!(
            out.get(&album_scoped_key(Some("Slayer"), "Reign in Blood")).map(String::as_str),
            Some("2026-03-03T00:00:00Z")
        );
    }

    #[test]
    fn fold_album_added_collapses_discs_and_accumulates_across_pages() {
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[
                p("file", "a/1.flac"),
                p("Album", "Blast from the Past [Disc 1]"),
                p("AlbumArtist", "Gamma Ray"),
                p("Last-Modified", "2019-01-01T00:00:00Z"),
            ],
            &mut out,
        );
        // The second page must merge into the same key, not replace it.
        fold_album_added(
            &[
                p("file", "a/2.flac"),
                p("Album", "Blast from the Past [Disc 2]"),
                p("AlbumArtist", "Gamma Ray"),
                p("Last-Modified", "2021-01-01T00:00:00Z"),
            ],
            &mut out,
        );
        assert_eq!(out.len(), 1, "the two discs did not collapse");
        assert_eq!(
            out.get(&album_scoped_key(Some("Gamma Ray"), "Blast from the Past")).map(String::as_str),
            Some("2021-01-01T00:00:00Z")
        );
    }

    #[test]
    fn fold_album_added_falls_back_to_artist_like_mpd_does() {
        // MPD's own `list Album group AlbumArtist` substitutes `Artist` when
        // the AlbumArtist tag is absent — it reported "Dio" for an album whose
        // song records carry only `Artist: Dio`. Keying on the raw tag here
        // built "\x1fHoly Diver" against a row keyed "Dio\x1fHoly Diver", and
        // on the real library that was 453 of 801 rows silently missing an
        // add-time.
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[
                p("file", "itunes/Dio/Holy Diver/01.m4a"),
                p("Added", "2026-01-05T11:50:58Z"),
                p("Artist", "Dio"),
                p("Album", "Holy Diver"),
            ],
            &mut out,
        );
        assert_eq!(
            out.keys().next().map(String::as_str),
            Some(album_scoped_key(Some("Dio"), "Holy Diver").as_str())
        );
    }

    #[test]
    fn fold_album_added_prefers_album_artist_over_artist() {
        // The fallback must not override a real AlbumArtist, or every
        // compilation would split into one entry per guest artist.
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[
                p("file", "comp/01.flac"),
                p("Added", "2026-01-01T00:00:00Z"),
                p("Artist", "Guest Artist"),
                p("AlbumArtist", "Various Artists"),
                p("Album", "Comp"),
                p("file", "comp/02.flac"),
                p("Added", "2026-02-01T00:00:00Z"),
                p("Artist", "Another Guest"),
                p("AlbumArtist", "Various Artists"),
                p("Album", "Comp"),
            ],
            &mut out,
        );
        assert_eq!(out.len(), 1, "the compilation split by guest artist");
        assert_eq!(
            out.get(&album_scoped_key(Some("Various Artists"), "Comp")).map(String::as_str),
            Some("2026-02-01T00:00:00Z")
        );
    }

    #[test]
    fn fold_album_added_does_not_leak_an_artist_between_songs() {
        // `flush` clears on every `file`, so a song with no artist tag at all
        // must not inherit the previous song's.
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[
                p("file", "a/1.flac"),
                p("Artist", "Dio"),
                p("Album", "Holy Diver"),
                p("Added", "2026-01-01T00:00:00Z"),
                p("file", "b/1.flac"),
                p("Album", "Untagged"),
                p("Added", "2026-01-02T00:00:00Z"),
            ],
            &mut out,
        );
        assert!(out.contains_key(&album_scoped_key(Some("Dio"), "Holy Diver")));
        assert!(out.contains_key(&album_scoped_key(Some(""), "Untagged")));
        assert!(!out.contains_key(&album_scoped_key(Some("Dio"), "Untagged")));
    }

    #[test]
    fn fold_album_added_prefers_added_over_last_modified() {
        // The rung decides which the server sent; when both arrive, the real
        // database add-time is the one that means what the sort claims.
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[
                p("file", "a/1.flac"),
                p("Last-Modified", "1999-01-01T00:00:00Z"),
                p("Added", "2026-01-01T00:00:00Z"),
                p("Album", "X"),
                p("AlbumArtist", "A"),
            ],
            &mut out,
        );
        assert_eq!(
            out.get(&album_scoped_key(Some("A"), "X")).map(String::as_str),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn fold_album_added_ignores_a_song_with_no_timestamp() {
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut out = HashMap::new();
        fold_album_added(
            &[p("file", "a/1.flac"), p("Album", "X"), p("AlbumArtist", "A")],
            &mut out,
        );
        assert!(out.is_empty(), "an undated song must not claim an add-time");
    }

    // --- RecentlyAddedRung --------------------------------------------------

    #[test]
    fn recently_added_top_rung_sorts_by_added_descending() {
        // `sort` must come before `window`: MPD applies them in that order,
        // and that ordering is the whole fix — it makes the limit drop the
        // oldest matches instead of an arbitrary slice of database order.
        let q = RecentlyAddedRung::AddedSince.query("2026-01-01T00:00:00Z", 5000);
        assert_eq!(
            q,
            "find \"(added-since '2026-01-01T00:00:00Z')\" sort -Added window 0:5000"
        );
        assert!(q.find("sort").unwrap() < q.find("window").unwrap());
    }

    #[test]
    fn recently_added_middle_rung_sorts_by_last_modified_descending() {
        let q = RecentlyAddedRung::ModifiedSinceSorted.query("2026-01-01T00:00:00Z", 10);
        assert_eq!(
            q,
            "find \"(modified-since '2026-01-01T00:00:00Z')\" sort -Last-Modified window 0:10"
        );
        // The minus is what makes it *descending*. Without it the window
        // keeps the oldest additions, i.e. exactly the wrong end.
        assert!(q.contains("sort -Last-Modified"));
    }

    #[test]
    fn recently_added_bottom_rung_is_the_legacy_unsorted_query() {
        let q = RecentlyAddedRung::ModifiedSinceUnsorted.query("2026-01-01T00:00:00Z", 10);
        assert_eq!(q, "find \"(modified-since '2026-01-01T00:00:00Z')\" window 0:10");
        assert!(!q.contains("sort"));
    }

    #[test]
    fn only_the_bottom_rung_needs_a_client_side_sort() {
        assert!(RecentlyAddedRung::AddedSince.is_server_sorted());
        assert!(RecentlyAddedRung::ModifiedSinceSorted.is_server_sorted());
        assert!(!RecentlyAddedRung::ModifiedSinceUnsorted.is_server_sorted());
    }

    #[test]
    fn the_ladder_descends_and_never_retries_a_rung_already_rejected() {
        let from_top: Vec<_> = RecentlyAddedRung::from(RecentlyAddedRung::TOP).collect();
        assert_eq!(
            from_top,
            vec![
                RecentlyAddedRung::AddedSince,
                RecentlyAddedRung::ModifiedSinceSorted,
                RecentlyAddedRung::ModifiedSinceUnsorted,
            ]
        );
        // A server already known to lack `added-since` must not be asked for
        // it again on every view entry.
        let from_middle: Vec<_> =
            RecentlyAddedRung::from(RecentlyAddedRung::ModifiedSinceSorted).collect();
        assert_eq!(
            from_middle,
            vec![
                RecentlyAddedRung::ModifiedSinceSorted,
                RecentlyAddedRung::ModifiedSinceUnsorted,
            ]
        );
        assert_eq!(
            RecentlyAddedRung::from(RecentlyAddedRung::ModifiedSinceUnsorted).count(),
            1
        );
    }

    #[test]
    fn rung_discriminants_round_trip_and_reserve_zero_for_unprobed() {
        // `MpdClient` caches the rung as a `u8` where 0 means "not probed",
        // so no rung may claim it.
        assert_eq!(RecentlyAddedRung::from_repr(0), None);
        for r in RecentlyAddedRung::from(RecentlyAddedRung::TOP) {
            assert_eq!(RecentlyAddedRung::from_repr(r as u8), Some(r));
            assert_ne!(r as u8, 0);
        }
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
