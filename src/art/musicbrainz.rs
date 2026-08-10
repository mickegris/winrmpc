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
        let lookup_album = Self::strip_edition_qualifier(album);

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

    async fn search_release_group_network(&self, artist: &str, album: &str) -> Option<String> {
        let query = format!(
            "releasegroup:\"{}\" AND artist:\"{}\"",
            Self::sanitize(album),
            Self::sanitize(artist)
        );
        let url = format!(
            "{MB_BASE}/release-group/?query={}&limit=3&fmt=json",
            urlencoding::encode(&query)
        );

        self.throttle.wait().await;

        let resp = self.http.get(&url).send().await.ok()?;
        let result: MbReleaseGroupSearchResult = resp.json().await.ok()?;
        let groups = result.release_groups?;

        // Take the highest scoring result
        groups
            .into_iter()
            .filter(|rg| rg.score.unwrap_or(0) > 50)
            .max_by_key(|rg| rg.score.unwrap_or(0))
            .map(|rg| rg.id)
    }

    async fn search_release(&self, artist: &str, album: &str) -> Option<String> {
        let query = format!(
            "release:\"{}\" AND artist:\"{}\"",
            Self::sanitize(album),
            Self::sanitize(artist)
        );
        let url = format!(
            "{MB_BASE}/release/?query={}&limit=3&fmt=json",
            urlencoding::encode(&query)
        );

        self.throttle.wait().await;

        let resp = self.http.get(&url).send().await.ok()?;
        let result: MbReleaseSearchResult = resp.json().await.ok()?;
        let releases = result.releases?;

        releases
            .into_iter()
            .filter(|r| r.score.unwrap_or(0) > 50)
            .max_by_key(|r| r.score.unwrap_or(0))
            .map(|r| r.id)
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
        let query = format!("artist:\"{}\"", Self::sanitize(artist));
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

    fn sanitize(s: &str) -> String {
        // Remove characters that break MusicBrainz Lucene queries
        s.replace('"', "")
            .replace('\\', "")
            .replace('(', "")
            .replace(')', "")
            .replace('[', "")
            .replace(']', "")
            .replace('{', "")
            .replace('}', "")
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
        const EDITION_KEYWORDS: &[&str] = &[
            "remaster", "remastered", "edition", "deluxe", "bonus",
            "expanded", "anniversary", "reissue", "version", "bit",
            "mono", "stereo",
        ];
        let trimmed = album.trim_end();
        let open = match trimmed.as_bytes().last() {
            Some(b']') => '[',
            Some(b')') => '(',
            _ => return album.to_string(),
        };
        let Some(open_idx) = trimmed.rfind(open) else {
            return album.to_string();
        };
        let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
        let lower = inner.to_lowercase();
        if !EDITION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
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
        let lower = text.to_lowercase();
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
            s.to_lowercase()
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
        let lookup_album = Self::strip_edition_qualifier(album);

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

        // Step 2: suffix fallback
        let candidates = [
            format!("{lookup_album} (album)"),
            format!("{lookup_album} ({artist} album)"),
            lookup_album.clone(),
        ];
        for title in &candidates {
            if let Some(summary) = self.try_bio_candidate(title, &lookup_album).await {
                return Some(summary);
            }
        }

        // Step 3: general search fallback
        for title in self.search_wikipedia(&format!("{lookup_album} {artist}")).await {
            if let Some(summary) = self.try_bio_candidate(&title, &lookup_album).await {
                return Some(summary);
            }
        }

        None
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
}
