//! MusicBrainz + Cover Art Archive client for fetching album and artist art,
//! plus Wikipedia artist/album bios via MusicBrainz's URL relations.
//! No API key required. Rate limit: 1 req/sec for MusicBrainz, none for
//! Cover Art Archive or Wikipedia.

use crate::store::Store;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const MB_BASE: &str = "https://musicbrainz.org/ws/2";
const CAA_BASE: &str = "https://coverartarchive.org";
const USER_AGENT: &str = "winrmpc/0.1.0 (https://github.com/user/winrmpc)";
const MB_MIN_INTERVAL: Duration = Duration::from_millis(1100);

/// Serializes MusicBrainz calls to ~1 req/s **globally** across every task
/// holding a clone of this handle — not just within one call chain. A local
/// `sleep(1100ms)` per call chain doesn't bound how many chains run
/// concurrently; this does (see docs/plans/art-wikipedia-fetch-order-and-caching.md §3).
#[derive(Clone)]
struct MusicBrainzThrottle {
    last_request: Arc<Mutex<Option<Instant>>>,
}

impl MusicBrainzThrottle {
    fn new() -> Self {
        Self {
            last_request: Arc::new(Mutex::new(None)),
        }
    }

    /// Blocks until at least `MB_MIN_INTERVAL` has passed since the *last*
    /// call from any task sharing this handle, then reserves the slot.
    async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < MB_MIN_INTERVAL {
                tokio::time::sleep(MB_MIN_INTERVAL - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }
}

#[derive(Debug, Deserialize)]
struct MbReleaseSearchResult {
    releases: Option<Vec<MbRelease>>,
}

#[derive(Debug, Deserialize)]
struct MbRelease {
    id: String,
    title: Option<String>,
    score: Option<u32>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbArtistCredit>>,
}

#[derive(Debug, Deserialize)]
struct MbArtistCredit {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseGroupSearchResult {
    #[serde(rename = "release-groups")]
    release_groups: Option<Vec<MbReleaseGroup>>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseGroup {
    id: String,
    title: Option<String>,
    score: Option<u32>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbArtistCredit>>,
}

#[derive(Debug, Deserialize)]
struct MbArtistSearchResult {
    artists: Option<Vec<MbArtist>>,
}

#[derive(Debug, Deserialize)]
struct MbArtist {
    id: String,
    name: Option<String>,
    score: Option<u32>,
    #[serde(rename = "release-groups")]
    release_groups: Option<Vec<MbReleaseGroup>>,
}

/// Used when fetching URL relations from a MusicBrainz entity
#[derive(Debug, Deserialize)]
struct MbEntityWithUrls {
    relations: Option<Vec<MbUrlRelation>>,
}

#[derive(Debug, Deserialize)]
struct MbUrlRelation {
    #[serde(rename = "type")]
    relation_type: Option<String>,
    url: Option<MbUrlResource>,
}

#[derive(Debug, Deserialize)]
struct MbUrlResource {
    resource: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WikiSearchResponse {
    query: Option<WikiSearchQuery>,
}

#[derive(Debug, Deserialize)]
struct WikiSearchQuery {
    search: Vec<WikiSearchResult>,
}

#[derive(Debug, Deserialize)]
struct WikiSearchResult {
    title: String,
}

#[derive(Clone)]
pub struct MusicBrainzClient {
    http: Client,
    store: Store,
    throttle: MusicBrainzThrottle,
}

impl MusicBrainzClient {
    /// `store` backs the MusicBrainz-ID cache (`mb_id_get`/`mb_id_put`) so
    /// `search_artist`/`search_release_group` don't re-resolve the same
    /// entity's MBID on every art *and* bio fetch.
    pub fn new(store: Store) -> Self {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        Self {
            http,
            store,
            throttle: MusicBrainzThrottle::new(),
        }
    }

    /// Fetch album cover art: search MusicBrainz for the release group, then get art from CAA
    pub async fn fetch_album_art(
        &self,
        artist: &str,
        album: &str,
    ) -> Option<Vec<u8>> {
        let lookup_album = Self::album_query_title(album);
        let artist = &Self::normalize_for_lookup(artist);

        // Try release-group search first (more reliable for cover art)
        let rg_id = self.search_release_group(artist, &lookup_album).await?;
        if let Some(data) = self.fetch_cover_art_release_group(&rg_id).await {
            tracing::info!(
                "Cover art from MusicBrainz: {artist} — {album} ({} KB)",
                data.len() / 1024
            );
            return Some(data);
        }

        // Fallback: search for individual release
        let release_id = self.search_release(artist, &lookup_album).await?;
        let data = self.fetch_cover_art_release(&release_id).await?;
        tracing::info!(
            "Cover art from MusicBrainz: {artist} — {album} ({} KB)",
            data.len() / 1024
        );
        Some(data)
    }

    /// Fetch artist art: find the artist, get their most popular release group, use its cover
    pub async fn fetch_artist_art(&self, artist: &str) -> Option<Vec<u8>> {
        let artist_id = self.search_artist(artist).await?;

        self.throttle.wait().await;

        // Get artist's release groups
        let url = format!(
            "{MB_BASE}/release-group?artist={artist_id}&type=album&limit=5&fmt=json"
        );
        let resp = self.http.get(&url).send().await.ok()?;
        let result: MbReleaseGroupSearchResult = resp.json().await.ok()?;
        let groups = result.release_groups?;

        // Try each release group until we find art
        for rg in groups {
            if let Some(data) = self.fetch_cover_art_release_group(&rg.id).await {
                tracing::info!(
                    "Artist image from MusicBrainz: {artist} ({} KB)",
                    data.len() / 1024
                );
                return Some(data);
            }
        }

        None
    }

    // ========================================================================
    // MusicBrainz ID resolution — cached (mb_ids table), since the art path
    // and the bio path each independently need the same artist/release-group
    // MBID for the same entity. See docs/plans/art-wikipedia-fetch-order-and-caching.md §6.
    // ========================================================================

    async fn mb_id_cached(&self, key: &str) -> Option<Option<String>> {
        let store = self.store.clone();
        let k = key.to_string();
        tokio::task::spawn_blocking(move || store.mb_id_get(&k))
            .await
            .ok()
            .flatten()
    }

    async fn mb_id_store(&self, key: &str, value: &Option<String>) {
        let store = self.store.clone();
        let k = key.to_string();
        let v = value.clone();
        let _ = tokio::task::spawn_blocking(move || store.mb_id_put(&k, &v)).await;
    }

    async fn search_release_group(&self, artist: &str, album: &str) -> Option<String> {
        let key = format!("{artist}\x1f{album}");
        if let Some(cached) = self.mb_id_cached(&key).await {
            return cached;
        }
        let result = self.search_release_group_network(artist, album).await;
        self.mb_id_store(&key, &result).await;
        result
    }

    /// Does a MusicBrainz result actually name the album we asked about?
    ///
    /// A score alone doesn't say this: MusicBrainz will happily return "The
    /// Doors" as a strong hit for "Best of the Doors", and the old code took
    /// the top-scoring result unchecked, so that album got the wrong cover
    /// with no way to tell. Both sides are normalized first, then it's an
    /// exact-containment or word-overlap test — the same `title_matches`
    /// used for Wikipedia article titles, which is mikMPD's rule too.
    fn release_title_matches(candidate: &str, expected: &str) -> bool {
        if expected.is_empty() {
            return true; // nothing to check against
        }
        let c = Self::fold_for_compare(candidate);
        let e = Self::fold_for_compare(expected);
        c.contains(&e) || Self::title_matches(&c, &e)
    }

    /// The comparison form for two strings that came from different sources:
    /// punctuation folded, lowercased, diacritics folded to ASCII.
    ///
    /// Diacritic folding is what makes the *Wikipedia* path as forgiving as
    /// the MusicBrainz one. Article titles keep their accents ("Motörhead",
    /// "Blue Öyster Cult") while library tags routinely don't, and
    /// `title_matches` treats a single-token target as needing an exact
    /// match — so without folding, every accented single-word band name
    /// failed its own article.
    fn fold_for_compare(s: &str) -> String {
        Self::normalize_for_lookup(s)
            .to_lowercase()
            .chars()
            .map(|c| Self::fold_diacritic(c).unwrap_or(c))
            .collect()
    }

    /// Does a MusicBrainz artist credit match the artist we asked about?
    ///
    /// Letters only, containment either way: that is what lets the very
    /// common mis-tagging `ACDC` match `AC/DC`, and `Beatles` match `The
    /// Beatles`, without accepting an unrelated artist.
    fn artist_credit_matches(credit: Option<&Vec<MbArtistCredit>>, expected: &str) -> bool {
        let (exp_folded, exp_stripped) = Self::artist_fingerprints(expected);
        if exp_folded.is_empty() {
            return true;
        }
        let Some(name) = credit.and_then(|c| c.first()).and_then(|c| c.name.as_deref())
        else {
            return true; // absent data isn't a mismatch
        };
        let (got_folded, got_stripped) = Self::artist_fingerprints(name);

        let contains_either = |a: &str, b: &str| !a.is_empty() && (a.contains(b) || b.contains(a));
        if contains_either(&got_folded, &exp_folded) {
            return true;
        }
        // Second chance with accented letters *removed* rather than folded.
        // This library has "Blue Îyster Cult" — mojibake of "Blue Öyster
        // Cult" — where folding gives i-vs-o and still misses, but dropping
        // the accented letter on both sides leaves "blueystercult" either
        // way. Length-guarded so short names can't collide.
        exp_stripped.len() >= 6 && contains_either(&got_stripped, &exp_stripped)
    }

    /// Two comparable forms of an artist name, both lowercase ASCII letters
    /// only: one with diacritics **folded** to their base letter, one with
    /// non-ASCII letters **dropped** entirely.
    ///
    /// Folding is what matches a library's "Motörhead" to MusicBrainz's; the
    /// dropped form is the escape hatch for mis-encoded tags, where the two
    /// sides disagree about *which* accented letter it is.
    fn artist_fingerprints(s: &str) -> (String, String) {
        let lower = Self::normalize_for_lookup(s).to_lowercase();
        let folded: String = lower
            .chars()
            .filter_map(|c| {
                Self::fold_diacritic(c).or(if c.is_ascii_alphabetic() { Some(c) } else { None })
            })
            .collect();
        let stripped: String = lower.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        (folded, stripped)
    }

    /// A Latin letter with a diacritic → its ASCII base. Covers Latin-1 and
    /// the common Latin Extended-A letters that turn up in band names.
    fn fold_diacritic(c: char) -> Option<char> {
        Some(match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'ď' | 'đ' => 'd',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
            'ř' => 'r',
            'ś' | 'š' | 'ş' => 's',
            'ť' | 'ţ' => 't',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            _ => return None,
        })
    }

    /// The three query shapes, loosest last, mirroring mikMPD's ladder.
    /// Every result is validated regardless of which query found it, so
    /// loosening the query can't loosen correctness.
    fn search_queries(field: &str, artist: &str, album: &str) -> Vec<String> {
        let esc_album = Self::lucene_escape(album);
        let esc_artist = Self::lucene_escape(artist);
        if artist.is_empty() {
            return vec![format!("{field}:\"{esc_album}\"")];
        }
        vec![
            // Exact, quoted.
            format!("{field}:\"{esc_album}\" AND artist:\"{esc_artist}\""),
            // Unquoted, so MusicBrainz tokenizes: lets "AC/DC Live" find
            // "Live" credited to AC/DC.
            format!("{field}:{esc_album} AND artist:{esc_artist}"),
            // Album alone: the artist tag may simply be wrong ("ACDC"), and
            // artist validation is lenient enough to still accept the hit.
            format!("{field}:\"{esc_album}\""),
        ]
    }

    async fn search_release_group_network(&self, artist: &str, album: &str) -> Option<String> {
        for query in Self::search_queries("releasegroup", artist, album) {
            let url = format!(
                "{MB_BASE}/release-group/?query={}&limit=5&fmt=json",
                urlencoding::encode(&query)
            );

            self.throttle.wait().await;

            let Some(resp) = self.http.get(&url).send().await.ok() else {
                continue;
            };
            let Ok(result) = resp.json::<MbReleaseGroupSearchResult>().await else {
                continue;
            };
            let Some(groups) = result.release_groups else {
                continue;
            };

            let mut candidates: Vec<MbReleaseGroup> = groups
                .into_iter()
                .filter(|rg| rg.score.unwrap_or(0) > 50)
                .filter(|rg| {
                    rg.title
                        .as_deref()
                        .is_none_or(|t| Self::release_title_matches(t, album))
                })
                .filter(|rg| Self::artist_credit_matches(rg.artist_credit.as_ref(), artist))
                .collect();
            candidates.sort_by_key(|rg| std::cmp::Reverse(rg.score.unwrap_or(0)));
            if let Some(rg) = candidates.into_iter().next() {
                return Some(rg.id);
            }
        }
        None
    }

    async fn search_release(&self, artist: &str, album: &str) -> Option<String> {
        for query in Self::search_queries("release", artist, album) {
            let url = format!(
                "{MB_BASE}/release/?query={}&limit=5&fmt=json",
                urlencoding::encode(&query)
            );

            self.throttle.wait().await;

            let Some(resp) = self.http.get(&url).send().await.ok() else {
                continue;
            };
            let Ok(result) = resp.json::<MbReleaseSearchResult>().await else {
                continue;
            };
            let Some(releases) = result.releases else {
                continue;
            };

            let mut candidates: Vec<MbRelease> = releases
                .into_iter()
                .filter(|r| r.score.unwrap_or(0) > 50)
                .filter(|r| {
                    r.title
                        .as_deref()
                        .is_none_or(|t| Self::release_title_matches(t, album))
                })
                .filter(|r| Self::artist_credit_matches(r.artist_credit.as_ref(), artist))
                .collect();
            candidates.sort_by_key(|r| std::cmp::Reverse(r.score.unwrap_or(0)));
            if let Some(r) = candidates.into_iter().next() {
                return Some(r.id);
            }
        }
        None
    }

    async fn search_artist(&self, artist: &str) -> Option<String> {
        let key = format!("artist:{artist}");
        if let Some(cached) = self.mb_id_cached(&key).await {
            return cached;
        }
        let result = self.search_artist_network(artist).await;
        self.mb_id_store(&key, &result).await;
        result
    }

    async fn search_artist_network(&self, artist: &str) -> Option<String> {
        let artist = Self::normalize_for_lookup(artist);
        let query = format!("artist:\"{}\"", Self::lucene_escape(&artist));
        let url = format!(
            "{MB_BASE}/artist/?query={}&limit=3&fmt=json",
            urlencoding::encode(&query)
        );

        self.throttle.wait().await;

        let resp = self.http.get(&url).send().await.ok()?;
        let result: MbArtistSearchResult = resp.json().await.ok()?;
        let artists = result.artists?;

        artists
            .into_iter()
            .filter(|a| a.score.unwrap_or(0) > 70)
            .max_by_key(|a| a.score.unwrap_or(0))
            .map(|a| a.id)
    }

    async fn fetch_cover_art_release_group(&self, rg_id: &str) -> Option<Vec<u8>> {
        let url = format!("{CAA_BASE}/release-group/{rg_id}/front-500");
        self.download_image(&url).await
    }

    async fn fetch_cover_art_release(&self, release_id: &str) -> Option<Vec<u8>> {
        let url = format!("{CAA_BASE}/release/{release_id}/front-500");
        self.download_image(&url).await
    }

    async fn download_image(&self, url: &str) -> Option<Vec<u8>> {
        let resp = self.http.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        if bytes.is_empty() {
            None
        } else {
            Some(bytes.to_vec())
        }
    }

    /// Backslash-escape the Lucene metacharacters in a MusicBrainz search
    /// term, mirroring mikMPD's `luceneEscape`.
    ///
    /// This replaces a `sanitize` that *deleted* `" \ ( ) [ ] { }` and left
    /// `/ + - ! ^ ~ * ? :` alone. Deleting loses information the query needs
    /// ("Songs (For Sale)" and "Songs For Sale" become the same term), and
    /// the untouched characters are the ones that actually break a query:
    /// `/` opens a Lucene regex, so **`AC/DC` — probably the most-tagged
    /// slash in any rock library — was issuing a malformed query on every
    /// lookup**. `-` and `!` are similarly operators.
    fn lucene_escape(s: &str) -> String {
        const SPECIAL: &[char] = &[
            '\\', '"', '+', '-', '!', '(', ')', '{', '}', '[', ']', '^', '~',
            '*', '?', ':', '/',
        ];
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            if SPECIAL.contains(&ch) {
                out.push('\\');
            }
            out.push(ch);
        }
        out
    }

    /// Fold the punctuation that taggers and databases disagree about, then
    /// undo sort-order artist names. Applied to both sides of every external
    /// lookup — the query we send *and* the title we compare the answer to.
    ///
    /// Ported from mikMPD's `normalizedForLookup`. The characters matter
    /// because a tag and a MusicBrainz/Wikipedia title routinely differ by
    /// exactly one of them: `Don’t Stop` (U+2019) vs `Don't Stop`,
    /// `1967–1970` (en dash) vs `1967-1970`, `Yes…` vs `Yes...`.
    ///
    /// `title_matches` is already immune to all of this — it tokenizes on
    /// non-alphanumerics, so quotes and dashes are separators either way —
    /// but the *query string* is not, and neither is the substring check in
    /// `is_music_article`.
    fn normalize_for_lookup(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            match ch {
                '\u{2026}' => out.push_str("..."),          // ellipsis
                '\u{2018}' | '\u{2019}' => out.push('\''),  // smart single quotes
                '\u{201C}' | '\u{201D}' => out.push('"'),   // smart double quotes
                '\u{2013}' | '\u{2014}' | '\u{2212}' => out.push('-'), // en/em dash, minus
                c => out.push(c),
            }
        }

        // Collapse whitespace runs. A real tag on this library reads
        // "Blue  Oyster Cult" with two spaces, which no exact-match query
        // will ever hit.
        if out.contains("  ") || out.trim() != out {
            out = out.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        // "Beatles, The" → "The Beatles". Library tags carry sort order far
        // more often than MusicBrainz does.
        for suffix in [", The", ", A", ", An"] {
            let Some(head_len) = out.len().checked_sub(suffix.len()) else {
                continue;
            };
            // `get` rather than slicing: a multi-byte char could straddle
            // this index, and indexing there would panic.
            let Some(tail) = out.get(head_len..) else {
                continue;
            };
            if head_len > 0 && tail.eq_ignore_ascii_case(suffix) {
                let article = &tail[2..]; // drop ", "
                return format!("{article} {}", &out[..head_len]);
            }
        }
        out
    }

    /// Query-ready form of an album title: disc markers and edition
    /// qualifiers off, punctuation folded.
    fn album_query_title(album: &str) -> String {
        Self::normalize_for_lookup(&Self::lookup_title(album))
    }

    /// Strip a trailing bracketed edition/remaster qualifier
    /// ("[24-bit Remaster]", "(Deluxe Edition)") from an album title, for
    /// MusicBrainz/Wikipedia query candidates only — never for the art
    /// cache key or grouping (mirrors the disc-marker-stripping rule
    /// planned for `art_key` in library-album-identity-and-multidisc.md;
    /// both strip trailing bracketed suffixes from the same tag but for
    /// different reasons, so this only recognizes edition keywords, not
    /// disc markers).
    fn strip_edition_qualifier(album: &str) -> String {
        let trimmed = album.trim_end();
        let open = match trimmed.as_bytes().last() {
            Some(b']') => '[',
            Some(b')') => '(',
            Some(b'}') => '{',
            _ => return album.to_string(),
        };
        let Some(open_idx) = trimmed.rfind(open) else {
            return album.to_string();
        };
        let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
        if !Self::is_edition_qualifier(inner) {
            return album.to_string();
        }
        let base = trimmed[..open_idx]
            .trim_end()
            .trim_end_matches(['-', ':', ','])
            .trim_end();
        if base.is_empty() {
            album.to_string()
        } else {
            base.to_string()
        }
    }

    /// Does this bracket's contents read as an edition/remaster/catalog
    /// qualifier rather than part of the title?
    ///
    /// Matches on **whole tokens**, which is the fix for the previous
    /// substring test: `lower.contains("bit")` also fired on "Rabbit", so
    /// `"Album (White Rabbit)"` had its bracket stripped. Beyond keywords it
    /// recognizes the three things mikMPD's regex does — a year, an audio
    /// spec (`24-bit`, `96 kHz`), and a catalogue number (`VICP-60852`,
    /// `88697…`) — all of which appear in real tags and none of which belong
    /// in a lookup query.
    fn is_edition_qualifier(inner: &str) -> bool {
        const KEYWORDS: &[&str] = &[
            "remaster", "remastered", "edition", "deluxe", "bonus", "expanded",
            "anniversary", "reissue", "version", "mono", "stereo", "live",
            "explicit", "sacd", "original", "recording", "japan", "import",
            "promo", "limited", "digipack", "digipak",
        ];
        let tokens: Vec<&str> = inner
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect();

        for (i, tok) in tokens.iter().enumerate() {
            let lower = tok.to_lowercase();
            if KEYWORDS.contains(&lower.as_str()) {
                return true;
            }
            // "hi-res" / "hires" — split by the tokenizer when hyphenated.
            if lower == "hires"
                || (lower == "hi"
                    && tokens
                        .get(i + 1)
                        .is_some_and(|n| n.eq_ignore_ascii_case("res")))
            {
                return true;
            }

            // A short all-caps token is a region or format marker, not part
            // of a title: "[UK]", "[US]", "[EP]". This library's Depeche Mode
            // rips are all tagged "… [UK]", which no MusicBrainz title has.
            if tok.len() <= 3
                && tok.chars().all(|c| c.is_ascii_uppercase())
                && !tok.is_empty()
            {
                return true;
            }

            if lower.chars().all(|c| c.is_ascii_digit()) {
                // A year: 1900–2099.
                if lower.len() == 4 {
                    if let Ok(year) = lower.parse::<u32>() {
                        if (1900..=2099).contains(&year) {
                            return true;
                        }
                    }
                }
                // "24-bit", "96 kHz" — a number qualified by a unit.
                if tokens.get(i + 1).is_some_and(|n| {
                    let n = n.to_lowercase();
                    n == "bit" || n == "khz" || n == "hz"
                }) {
                    return true;
                }
                // A bare digit run this long is a catalogue number, not a
                // title. Four digits keeps "Part 2" and "Volume 3" safe.
                if lower.len() >= 4 {
                    return true;
                }
            }
        }
        false
    }

    /// Composes both lookup-only transforms for an album query candidate:
    /// strip the edition qualifier, then the disc-marker suffix (order
    /// matters for titles carrying both, e.g. `"X [Disc 1] [Remastered]"`).
    /// Never used for the art cache key — see `Song::art_key`, which folds
    /// disc markers but not edition qualifiers (a remaster isn't the same
    /// release as the original for art-lookup purposes, only for grouping).
    /// Applied repeatedly until the title stops changing, because real tags
    /// stack the suffixes: `"Album (Deluxe Edition) [2011 Remaster]"` needs
    /// two passes, and `"Album [Remastered] [Disc 1]"` needs both transforms
    /// to alternate. A single pass left the second suffix in the query.
    fn lookup_title(album: &str) -> String {
        let mut title = crate::mpd::types::album_base_and_disc(album).0;
        // Bounded: each iteration must shorten the title or we stop.
        loop {
            let stripped = Self::strip_edition_qualifier(&title);
            let stripped = crate::mpd::types::album_base_and_disc(&stripped).0;
            if stripped == title {
                return title;
            }
            title = stripped;
        }
    }

    /// Fetch the curated English Wikipedia URL from a MusicBrainz entity's URL relations.
    /// entity_type is "artist" or "release-group".
    async fn get_wikipedia_url(&self, entity_type: &str, id: &str) -> Option<String> {
        let url = format!("{MB_BASE}/{entity_type}/{id}?inc=url-rels&fmt=json");
        self.throttle.wait().await;
        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let entity: MbEntityWithUrls = resp.json().await.ok()?;
        let relations = entity.relations?;
        for rel in relations {
            if rel.relation_type.as_deref() == Some("wikipedia") {
                if let Some(resource) = rel.url.and_then(|u| u.resource) {
                    if resource.contains("en.wikipedia.org") {
                        return Some(resource);
                    }
                }
            }
        }
        None
    }

    /// Extract the Wikipedia page title from a full Wikipedia URL.
    /// e.g. "https://en.wikipedia.org/wiki/Tool_(band)" → "Tool_(band)"
    fn wiki_title_from_url(url: &str) -> Option<String> {
        let path = url.strip_prefix("https://en.wikipedia.org/wiki/")?;
        // Strip any fragment (e.g. #History)
        let title = path.split('#').next()?;
        // Decode percent-encoding
        let decoded = urlencoding::decode(title).ok()?;
        Some(decoded.into_owned())
    }

    /// Check that a Wikipedia extract is actually about a music artist/band/album,
    /// not an unrelated article with the same name.
    fn is_music_article(text: &str, name: &str) -> bool {
        // Folded on both sides: the final clause is a substring test, so a
        // smart apostrophe or an umlaut in the tag would otherwise never
        // match the article's straight quote / plain letter (or vice versa).
        let lower = Self::fold_for_compare(text);
        let name = Self::fold_for_compare(&name.to_string());
        lower.contains("band")
            || lower.contains("musician")
            || lower.contains("singer")
            || lower.contains("rapper")
            || lower.contains("album")
            || lower.contains("discography")
            || lower.contains("record label")
            || lower.contains("music")
            || lower.contains("song")
            || lower.contains("track")
            || lower.contains(name.to_lowercase().as_str())
    }

    /// Word-level title check, mirroring mikMPD's `titleTokensMatch`:
    /// stopwords dropped, whole-word matching. A single-token target needs
    /// an exact (post-normalization) match; two or more tokens need ≥2/3
    /// overlap. Used to prefer a hit whose *title* names the artist/album
    /// over one that merely mentions it in the extract (the "sequel cites
    /// the album by name" failure mode `is_music_article` alone is prone to).
    fn title_matches(article_title: &str, target: &str) -> bool {
        const STOPWORDS: &[&str] = &["the", "a", "an", "of", "and", "in", "on", "at", "to", "for"];
        fn tokenize(s: &str) -> Vec<String> {
            MusicBrainzClient::fold_for_compare(s)
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty() && !STOPWORDS.contains(w))
                .map(|w| w.to_string())
                .collect()
        }
        let a = tokenize(article_title);
        let t = tokenize(target);
        if t.is_empty() {
            return false;
        }
        if t.len() == 1 {
            return a.len() == 1 && a[0] == t[0];
        }
        let overlap = t.iter().filter(|w| a.contains(w)).count();
        overlap * 3 >= t.len() * 2
    }

    /// Fetch a summary and accept it if either the article's title strongly
    /// matches `match_target` (immediate win) or, failing that, the extract
    /// passes the weaker keyword-based `is_music_article` check.
    async fn try_bio_candidate(&self, title: &str, match_target: &str) -> Option<String> {
        let summary = self.fetch_wikipedia_summary(title).await?;
        if Self::title_matches(title, match_target) || Self::is_music_article(&summary, match_target) {
            Some(summary)
        } else {
            None
        }
    }

    /// Wikipedia's search API — fallback layer for when none of the guessed
    /// exact titles match. Returns up to 3 candidate titles.
    async fn search_wikipedia(&self, query: &str) -> Vec<String> {
        let url = format!(
            "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&format=json&srlimit=3",
            urlencoding::encode(query)
        );
        let Ok(resp) = self.http.get(&url).send().await else {
            return Vec::new();
        };
        let Ok(parsed) = resp.json::<WikiSearchResponse>().await else {
            return Vec::new();
        };
        parsed
            .query
            .map(|q| q.search.into_iter().map(|r| r.title).collect())
            .unwrap_or_default()
    }

    /// Fetch a Wikipedia summary for an artist.
    /// Step 1: look up the MusicBrainz artist entry's curated Wikipedia URL relation.
    /// Step 2: fall back to suffix-guessing if MusicBrainz has no Wikipedia link.
    /// Step 3: fall back to Wikipedia's own search API.
    pub async fn fetch_artist_bio(&self, artist: &str) -> Option<String> {
        // Same normalization the art path gets, and for the same reason: the
        // raw tag is what every step below is built from, so a sort-order
        // name ("Alan Parsons Project, The") would otherwise be guessed as
        // the Wikipedia title "Alan Parsons Project, The (band)", searched
        // for verbatim, *and* used as the `title_matches` target — three
        // chances to fail on one unfolded string.
        let artist = &Self::normalize_for_lookup(artist);

        // Step 1: MusicBrainz canonical Wikipedia link
        if let Some(artist_id) = self.search_artist(artist).await {
            if let Some(wiki_url) = self.get_wikipedia_url("artist", &artist_id).await {
                if let Some(title) = Self::wiki_title_from_url(&wiki_url) {
                    if let Some(summary) = self.try_bio_candidate(&title, artist).await {
                        return Some(summary);
                    }
                }
            }
        }

        // Step 2: suffix fallback
        let suffixes = ["(band)", "(musician)", "(singer)", "(rapper)", "(DJ)", ""];
        for suffix in suffixes {
            let title = if suffix.is_empty() {
                artist.to_string()
            } else {
                format!("{artist} {suffix}")
            };
            if let Some(summary) = self.try_bio_candidate(&title, artist).await {
                return Some(summary);
            }
        }

        // Step 3: general search fallback
        for title in self.search_wikipedia(artist).await {
            if let Some(summary) = self.try_bio_candidate(&title, artist).await {
                return Some(summary);
            }
        }

        None
    }

    /// Fetch a Wikipedia summary for an album.
    /// Step 1: look up the MusicBrainz release-group's curated Wikipedia URL relation.
    /// Step 2: fall back to "(album)" suffix guessing.
    /// Step 3: fall back to Wikipedia's own search API.
    pub async fn fetch_album_bio(&self, artist: &str, album: &str) -> Option<String> {
        // Normalized on the way in, so every Wikipedia title guess, the
        // MusicBrainz query and the `title_matches` target below are all
        // built from the same folded string.
        let lookup_album = Self::album_query_title(album);
        let artist = &Self::normalize_for_lookup(artist);

        // Step 1: MusicBrainz canonical Wikipedia link
        if let Some(rg_id) = self.search_release_group(artist, &lookup_album).await {
            if let Some(wiki_url) = self.get_wikipedia_url("release-group", &rg_id).await {
                if let Some(title) = Self::wiki_title_from_url(&wiki_url) {
                    if let Some(summary) = self.try_bio_candidate(&title, &lookup_album).await {
                        return Some(summary);
                    }
                }
            }
        }

        // A title like "Greatest Hits" belongs to hundreds of artists and
        // usually has a generic Wikipedia article attached to none of them.
        // Every loose path below is tightened or skipped when it applies.
        let generic = !artist.is_empty() && Self::is_generic_album_title(&lookup_album);

        // Step 2: Wikipedia's own album naming patterns.
        //
        // A "(… album)" title looks self-validating and isn't: `Greatest
        // Hits (album)` **redirects to Wikipedia's article about the concept
        // of a greatest-hits record**, which was duly returned as Bob Dylan's
        // album bio. So every hit goes through the same validation as a
        // search hit — for a real album the extract names both the album and
        // the artist, and the concept article names neither.
        let mut candidates = Vec::new();
        if !artist.is_empty() {
            candidates.push(format!("{lookup_album} ({artist} album)"));
        }
        candidates.push(format!("{lookup_album} (album)"));
        for title in &candidates {
            if let Some(extract) = self.fetch_wikipedia_summary(title).await {
                if Self::album_result_matches(title, &extract, &lookup_album, artist) {
                    return Some(extract);
                }
            }
        }

        // Step 3: the plain title. Distinctive album names ("An Acoustic
        // Evening at the Vienna Opera House") are articles with no "(album)"
        // suffix at all — but a plain "Greatest Hits" page is almost never
        // about *this* artist's record, so generic titles skip this unless
        // the album name itself carries the artist.
        if !generic
            || Self::fold_for_compare(&lookup_album).contains(&Self::fold_for_compare(artist))
        {
            if let Some(extract) = self.fetch_wikipedia_summary(&lookup_album).await {
                if Self::is_music_article(&extract, &lookup_album)
                    && Self::album_result_matches(&lookup_album, &extract, &lookup_album, artist)
                {
                    return Some(extract);
                }
            }
        }

        // Step 4: search, loosest last. A hit whose *title* names the album
        // wins immediately; an extract-only match is held back as a fallback,
        // because a related article (a sequel, another compilation) will
        // happily cite this album by name.
        let searches: Vec<String> = if artist.is_empty() {
            vec![format!("{lookup_album} album")]
        } else if generic {
            // Drop the artist-free query: "Greatest Hits album" returns
            // something for everyone, and none of it is this record.
            vec![format!("{lookup_album} {artist} album")]
        } else {
            vec![
                format!("{lookup_album} {artist} album"),
                format!("{lookup_album} album"),
            ]
        };

        let mut extract_only: Option<String> = None;
        for query in searches {
            for title in self.search_wikipedia(&query).await {
                let Some(extract) = self.fetch_wikipedia_summary(&title).await else {
                    continue;
                };
                if !Self::album_result_matches(&title, &extract, &lookup_album, artist) {
                    continue;
                }
                if Self::title_matches(&title, &lookup_album) {
                    return Some(extract);
                }
                if extract_only.is_none() {
                    extract_only = Some(extract);
                }
            }
        }

        // For a generic title, a matching extract is too weak to stand on
        // its own — the title it came from could be anything.
        if generic {
            None
        } else {
            extract_only
        }
    }

    /// Album titles with a Wikipedia article that belongs to no single
    /// artist's release. Ported from mikMPD's `genericAlbumTitles`, which
    /// notes why this is a curated list rather than a token-count heuristic:
    /// a count also catches distinctive short titles like Depeche Mode's
    /// "101".
    fn is_generic_album_title(album: &str) -> bool {
        const GENERIC: &[&str] = &[
            "greatest hits", "gold", "live", "the best of", "best of", "hits",
            "anthology", "collection", "the collection", "essential",
            "the essential", "platinum", "compilation", "singles",
            "the singles", "the very best of", "the hits", "unplugged",
            "in concert",
        ];
        let a = Self::fold_for_compare(album);
        GENERIC.contains(&a.trim())
    }

    /// A Wikipedia hit must be about **this album by this artist**, not just
    /// something in the artist's discography.
    ///
    /// Token overlap counts only toward the *title*: the extract has to
    /// contain the album name outright, because a related article mentions
    /// enough of the album's words in passing to fool a token match. The
    /// artist check is what winrmpc was missing entirely — without it, a
    /// search hit for a different artist's same-titled album passed as long
    /// as the extract mentioned music.
    fn album_result_matches(title: &str, extract: &str, album: &str, artist: &str) -> bool {
        let album_f = Self::fold_for_compare(album);
        let title_f = Self::fold_for_compare(title);
        let extract_f = Self::fold_for_compare(extract);

        let about_album = title_f.contains(&album_f)
            || Self::title_matches(&title_f, &album_f)
            || extract_f.contains(&album_f);

        let artist_f = Self::fold_for_compare(artist);
        let letters = |s: &str| s.chars().filter(|c| c.is_alphabetic()).collect::<String>();
        let about_artist = artist_f.trim().is_empty()
            || extract_f.contains(&artist_f)
            || letters(&extract_f).contains(&letters(&artist_f));

        about_album && about_artist
    }

    async fn fetch_wikipedia_summary(&self, title: &str) -> Option<String> {
        let encoded = urlencoding::encode(title);
        let url = format!(
            "https://en.wikipedia.org/api/rest_v1/page/summary/{encoded}"
        );

        let resp = self.http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }

        let json: serde_json::Value = resp.json().await.ok()?;

        // Only use "standard" type articles (not disambiguation pages etc)
        let page_type = json.get("type")?.as_str()?;
        if page_type != "standard" {
            return None;
        }

        let extract = json.get("extract")?.as_str()?;
        if extract.is_empty() {
            return None;
        }

        Some(extract.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the throttle is *global* (shared across clones), not merely
    /// serializing calls within one call chain the way the old per-method
    /// `sleep(1100ms)` did.
    #[tokio::test]
    async fn musicbrainz_throttle_serializes_across_clones() {
        let throttle = MusicBrainzThrottle::new();
        let other_handle = throttle.clone();

        let start = Instant::now();
        other_handle.wait().await; // first call anywhere: no wait
        let after_first = Instant::now();
        throttle.wait().await; // second call, from a *different* clone
        let after_second = Instant::now();

        assert!(after_first.duration_since(start) < Duration::from_millis(200));
        assert!(after_second.duration_since(after_first) >= Duration::from_millis(1000));
    }

    #[test]
    fn title_matches_exact_match() {
        assert!(MusicBrainzClient::title_matches("Blast from the Past", "Blast from the Past"));
    }

    #[test]
    fn title_matches_stopwords_dropped_and_case_insensitive() {
        assert!(MusicBrainzClient::title_matches(
            "The Blast From The Past",
            "blast from past"
        ));
    }

    #[test]
    fn title_matches_two_thirds_overlap_accepted() {
        // "Live at Carnegie Hall" (4 tokens) vs "Live at Vienna Hall" (4 tokens):
        // overlap = {live, at, hall} = 3/4, which is >= 2/3.
        assert!(MusicBrainzClient::title_matches(
            "Live at Vienna Hall",
            "Live at Carnegie Hall"
        ));
    }

    #[test]
    fn title_matches_below_threshold_rejected() {
        // Only "hall" overlaps out of 4 target tokens (1/4 < 2/3).
        assert!(!MusicBrainzClient::title_matches(
            "Symphony Concert Hall",
            "Live at Carnegie Hall"
        ));
    }

    #[test]
    fn title_matches_single_token_requires_exact() {
        assert!(MusicBrainzClient::title_matches("Nevermind", "Nevermind"));
        assert!(!MusicBrainzClient::title_matches("Nevermind (film)", "Nevermind"));
    }

    #[test]
    fn title_matches_empty_target_rejected() {
        assert!(!MusicBrainzClient::title_matches("Anything", ""));
    }

    #[test]
    fn strip_edition_qualifier_remaster_bracket() {
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album [24-bit Remaster]"),
            "Album"
        );
    }

    #[test]
    fn strip_edition_qualifier_deluxe_paren() {
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (Deluxe Edition)"),
            "Album"
        );
    }

    #[test]
    fn strip_edition_qualifier_no_marker_passthrough() {
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Plain Album"),
            "Plain Album"
        );
    }

    #[test]
    fn strip_edition_qualifier_non_edition_bracket_passthrough() {
        // "[Disc 1]" has no edition keyword — strip_edition_qualifier must
        // not touch it (that's a separate, disc-marker-specific concern).
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album [Disc 1]"),
            "Album [Disc 1]"
        );
    }

    #[test]
    fn strip_edition_qualifier_composes_with_a_second_trailing_bracket() {
        // Stripping is a single trailing-group operation, so it composes:
        // running it twice peels one bracket at a time.
        let once = MusicBrainzClient::strip_edition_qualifier("Album [Disc 1] [Remastered]");
        assert_eq!(once, "Album [Disc 1]");
    }

    // === Lucene escaping ===

    #[test]
    fn lucene_escape_protects_the_ac_dc_slash() {
        // The regression this replaced: `/` opens a Lucene regex, and the old
        // `sanitize` didn't touch it, so every AC/DC lookup sent a malformed
        // query.
        assert_eq!(MusicBrainzClient::lucene_escape("AC/DC"), r"AC\/DC");
    }

    #[test]
    fn lucene_escape_keeps_bracket_content_instead_of_deleting_it() {
        // The old `sanitize` removed brackets outright, collapsing two
        // different albums onto one query string.
        assert_eq!(
            MusicBrainzClient::lucene_escape("Songs (For Sale)"),
            r"Songs \(For Sale\)"
        );
    }

    #[test]
    fn lucene_escape_covers_the_operator_characters() {
        assert_eq!(MusicBrainzClient::lucene_escape("a+b-c!d"), r"a\+b\-c\!d");
    }

    // === Unicode normalization ===

    #[test]
    fn normalize_folds_smart_punctuation() {
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("Don\u{2019}t Stop\u{2026}"),
            "Don't Stop..."
        );
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("1967\u{2013}1970"),
            "1967-1970"
        );
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("\u{201C}Heroes\u{201D}"),
            "\"Heroes\""
        );
    }

    #[test]
    fn normalize_moves_sort_order_article_to_front() {
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("Beatles, The"),
            "The Beatles"
        );
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("Doors, The"),
            "The Doors"
        );
    }

    #[test]
    fn normalize_leaves_a_normal_title_alone() {
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("The Wall"),
            "The Wall"
        );
        // A comma that isn't a sort-order article must survive.
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("Sgt. Pepper, Live"),
            "Sgt. Pepper, Live"
        );
    }

    #[test]
    fn normalize_does_not_panic_on_multibyte_tails() {
        // The article check indexes from the end; a multi-byte char straddling
        // that index must not panic.
        assert_eq!(MusicBrainzClient::normalize_for_lookup("Björk"), "Björk");
        assert_eq!(MusicBrainzClient::normalize_for_lookup("Å"), "Å");
        assert_eq!(MusicBrainzClient::normalize_for_lookup(""), "");
    }

    // === Edition qualifier recognition ===

    #[test]
    fn edition_qualifier_does_not_fire_on_a_word_containing_bit() {
        // The substring test this replaced stripped "(White Rabbit)" because
        // "rabbit" contains "bit".
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (White Rabbit)"),
            "Album (White Rabbit)"
        );
    }

    #[test]
    fn edition_qualifier_recognizes_year_and_audio_spec_and_catalog() {
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (2011)"),
            "Album"
        );
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album [24-bit]"),
            "Album"
        );
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Killers (CDM 7520192)"),
            "Killers"
        );
    }

    #[test]
    fn edition_qualifier_leaves_part_and_volume_brackets_alone() {
        // Short digit runs must not read as catalogue numbers.
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (Part 2)"),
            "Album (Part 2)"
        );
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (Volume 3)"),
            "Album (Volume 3)"
        );
    }

    #[test]
    fn lookup_title_strips_stacked_qualifiers() {
        // One pass left the second suffix in the query.
        assert_eq!(
            MusicBrainzClient::lookup_title("Album (Deluxe Edition) [2011 Remaster]"),
            "Album"
        );
    }

    // === MusicBrainz result validation ===

    #[test]
    fn release_title_match_rejects_a_different_album() {
        // The case the unchecked top-score pick got wrong.
        assert!(!MusicBrainzClient::release_title_matches(
            "The Doors",
            "Best of the Doors"
        ));
        assert!(MusicBrainzClient::release_title_matches(
            "Best of the Doors",
            "Best of the Doors"
        ));
    }

    #[test]
    fn release_title_match_is_punctuation_insensitive() {
        assert!(MusicBrainzClient::release_title_matches(
            "Don't Stop the Music",
            "Don\u{2019}t Stop the Music"
        ));
    }

    #[test]
    fn artist_credit_match_accepts_common_tag_spellings() {
        let credit = vec![MbArtistCredit {
            name: Some("AC/DC".into()),
        }];
        assert!(MusicBrainzClient::artist_credit_matches(
            Some(&credit),
            "ACDC"
        ));
        let beatles = vec![MbArtistCredit {
            name: Some("The Beatles".into()),
        }];
        assert!(MusicBrainzClient::artist_credit_matches(
            Some(&beatles),
            "Beatles, The"
        ));
    }

    #[test]
    fn artist_credit_match_folds_diacritics() {
        let credit = vec![MbArtistCredit {
            name: Some("Motörhead".into()),
        }];
        assert!(MusicBrainzClient::artist_credit_matches(
            Some(&credit),
            "Motorhead"
        ));
    }

    #[test]
    fn artist_credit_match_survives_mojibake_and_double_spaces() {
        // Both real tags on this library for one band.
        let credit = vec![MbArtistCredit {
            name: Some("Blue Öyster Cult".into()),
        }];
        assert!(MusicBrainzClient::artist_credit_matches(
            Some(&credit),
            "Blue  Oyster Cult"
        ));
        assert!(MusicBrainzClient::artist_credit_matches(
            Some(&credit),
            "Blue Îyster Cult"
        ));
    }

    #[test]
    fn edition_qualifier_strips_a_short_all_caps_region_tag() {
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("A Broken Frame [UK]"),
            "A Broken Frame"
        );
        // Mixed case is a title word, not a marker.
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (Live at Leeds)"),
            "Album"
        );
        assert_eq!(
            MusicBrainzClient::strip_edition_qualifier("Album (Rain)"),
            "Album (Rain)"
        );
    }

    #[test]
    fn normalize_collapses_whitespace_runs() {
        assert_eq!(
            MusicBrainzClient::normalize_for_lookup("Blue  Oyster Cult"),
            "Blue Oyster Cult"
        );
    }

    // === Wikipedia matching ===

    #[test]
    fn title_matches_folds_diacritics() {
        // Wikipedia keeps the umlaut, library tags usually don't. A
        // single-token target needs an exact match, so without folding every
        // accented one-word band name failed its own article.
        assert!(MusicBrainzClient::title_matches("Motörhead", "Motorhead"));
        assert!(MusicBrainzClient::title_matches(
            "Blue Öyster Cult",
            "Blue Oyster Cult"
        ));
    }

    #[test]
    fn generic_album_titles_are_recognized() {
        assert!(MusicBrainzClient::is_generic_album_title("Greatest Hits"));
        assert!(MusicBrainzClient::is_generic_album_title("the very best of"));
        // Distinctive short titles must not be caught — the reason this is a
        // curated list and not a token-count rule.
        assert!(!MusicBrainzClient::is_generic_album_title("101"));
        assert!(!MusicBrainzClient::is_generic_album_title("Powerage"));
    }

    #[test]
    fn album_result_requires_the_extract_to_name_the_artist() {
        // The gap this closes: a same-titled album by someone else used to
        // pass on "mentions music" alone.
        assert!(!MusicBrainzClient::album_result_matches(
            "Powerage",
            "Powerage is the fifth studio album by the band Someone Else.",
            "Powerage",
            "AC/DC"
        ));
        assert!(MusicBrainzClient::album_result_matches(
            "Powerage",
            "Powerage is the fifth studio album by Australian rock band AC/DC.",
            "Powerage",
            "AC/DC"
        ));
    }

    #[test]
    fn album_result_rejects_the_generic_concept_article() {
        // `Greatest Hits (album)` redirects to Wikipedia's article about
        // greatest-hits records as a *concept*, which was being returned as
        // Bob Dylan's album bio. It names the album words but no artist.
        assert!(!MusicBrainzClient::album_result_matches(
            "Greatest Hits (album)",
            "A greatest hits album or best-of album is a type of compilation \
             album that collects popular songs by a particular artist.",
            "Greatest Hits",
            "Bob Dylan"
        ));
        assert!(MusicBrainzClient::album_result_matches(
            "Bob Dylan's Greatest Hits",
            "Bob Dylan's Greatest Hits is a 1967 compilation album of songs \
             by the American singer-songwriter Bob Dylan.",
            "Greatest Hits",
            "Bob Dylan"
        ));
    }

    #[test]
    fn album_result_matches_artist_across_diacritics() {
        assert!(MusicBrainzClient::album_result_matches(
            "Fire of Unknown Origin",
            "Fire of Unknown Origin is an album by Blue Öyster Cult.",
            "Fire of Unknown Origin",
            "Blue  Oyster Cult"
        ));
    }

    #[test]
    fn album_result_requires_the_extract_to_name_the_album_outright() {
        // Token overlap counts toward the title only; a sequel's extract
        // shares words freely.
        assert!(!MusicBrainzClient::album_result_matches(
            "Live at Carnegie Hall",
            "A live album recorded at the Vienna Opera House by the band.",
            "An Acoustic Evening at the Vienna Opera House",
            ""
        ));
    }

    #[test]
    fn artist_credit_match_rejects_an_unrelated_artist() {
        let credit = vec![MbArtistCredit {
            name: Some("Metallica".into()),
        }];
        assert!(!MusicBrainzClient::artist_credit_matches(
            Some(&credit),
            "Megadeth"
        ));
    }

    #[test]
    fn artist_credit_match_passes_when_data_is_absent() {
        assert!(MusicBrainzClient::artist_credit_matches(None, "Anyone"));
    }

    #[test]
    fn search_queries_loosen_and_drop_the_artist_clause_when_unknown() {
        let qs = MusicBrainzClient::search_queries("release", "AC/DC", "Live");
        assert_eq!(qs.len(), 3);
        assert!(qs[0].contains(r#"artist:"AC\/DC""#), "{}", qs[0]);
        assert!(!qs[2].contains("artist:"), "{}", qs[2]);

        let no_artist = MusicBrainzClient::search_queries("release", "", "Live");
        assert_eq!(no_artist.len(), 1);
        assert!(!no_artist[0].contains("artist:"));
    }

    fn lookup_title_strips_both_edition_and_disc_marker() {
        assert_eq!(
            MusicBrainzClient::lookup_title("Album [Disc 1] [Remastered]"),
            "Album"
        );
        assert_eq!(MusicBrainzClient::lookup_title("Album [Disc 2]"), "Album");
        assert_eq!(
            MusicBrainzClient::lookup_title("Album (Deluxe Edition)"),
            "Album"
        );
    }
}
