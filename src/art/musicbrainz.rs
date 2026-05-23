//! MusicBrainz + Cover Art Archive client for fetching album and artist art.
//! No API key required. Rate limit: 1 req/sec for MusicBrainz, none for Cover Art Archive.

use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

const MB_BASE: &str = "https://musicbrainz.org/ws/2";
const CAA_BASE: &str = "https://coverartarchive.org";
const USER_AGENT: &str = "winrmpc/0.1.0 (https://github.com/user/winrmpc)";

pub struct MusicBrainzClient {
    http: Client,
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

impl MusicBrainzClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        Self { http }
    }

    /// Fetch album cover art: search MusicBrainz for the release group, then get art from CAA
    pub async fn fetch_album_art(
        &self,
        artist: &str,
        album: &str,
    ) -> Option<Vec<u8>> {
        // Try release-group search first (more reliable for cover art)
        let rg_id = self.search_release_group(artist, album).await?;
        if let Some(data) = self.fetch_cover_art_release_group(&rg_id).await {
            return Some(data);
        }

        // Fallback: search for individual release
        let release_id = self.search_release(artist, album).await?;
        self.fetch_cover_art_release(&release_id).await
    }

    /// Fetch artist art: find the artist, get their most popular release group, use its cover
    pub async fn fetch_artist_art(&self, artist: &str) -> Option<Vec<u8>> {
        let artist_id = self.search_artist(artist).await?;

        // Respect rate limit
        sleep(Duration::from_millis(1100)).await;

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
                return Some(data);
            }
        }

        None
    }

    async fn search_release_group(&self, artist: &str, album: &str) -> Option<String> {
        let query = format!(
            "releasegroup:\"{}\" AND artist:\"{}\"",
            Self::sanitize(album),
            Self::sanitize(artist)
        );
        let url = format!(
            "{MB_BASE}/release-group/?query={}&limit=3&fmt=json",
            urlencoding::encode(&query)
        );

        // Respect rate limit
        sleep(Duration::from_millis(1100)).await;

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

        sleep(Duration::from_millis(1100)).await;

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
        let query = format!("artist:\"{}\"", Self::sanitize(artist));
        let url = format!(
            "{MB_BASE}/artist/?query={}&limit=3&fmt=json",
            urlencoding::encode(&query)
        );

        sleep(Duration::from_millis(1100)).await;

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

    /// Fetch the curated English Wikipedia URL from a MusicBrainz entity's URL relations.
    /// entity_type is "artist" or "release-group".
    async fn get_wikipedia_url(&self, entity_type: &str, id: &str) -> Option<String> {
        let url = format!("{MB_BASE}/{entity_type}/{id}?inc=url-rels&fmt=json");
        sleep(Duration::from_millis(1100)).await;
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

    /// Fetch a Wikipedia summary for an artist.
    /// Step 1: look up the MusicBrainz artist entry's curated Wikipedia URL relation.
    /// Step 2: fall back to suffix-guessing if MusicBrainz has no Wikipedia link.
    pub async fn fetch_artist_bio(&self, artist: &str) -> Option<String> {
        // Step 1: MusicBrainz canonical Wikipedia link
        if let Some(artist_id) = self.search_artist(artist).await {
            if let Some(wiki_url) = self.get_wikipedia_url("artist", &artist_id).await {
                if let Some(title) = Self::wiki_title_from_url(&wiki_url) {
                    if let Some(summary) = self.fetch_wikipedia_summary(&title).await {
                        if Self::is_music_article(&summary, artist) {
                            return Some(summary);
                        }
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
            if let Some(summary) = self.fetch_wikipedia_summary(&title).await {
                if Self::is_music_article(&summary, artist) {
                    return Some(summary);
                }
            }
        }

        None
    }

    /// Fetch a Wikipedia summary for an album.
    /// Step 1: look up the MusicBrainz release-group's curated Wikipedia URL relation.
    /// Step 2: fall back to "(album)" suffix guessing.
    pub async fn fetch_album_bio(&self, artist: &str, album: &str) -> Option<String> {
        // Step 1: MusicBrainz canonical Wikipedia link
        if let Some(rg_id) = self.search_release_group(artist, album).await {
            if let Some(wiki_url) = self.get_wikipedia_url("release-group", &rg_id).await {
                if let Some(title) = Self::wiki_title_from_url(&wiki_url) {
                    if let Some(summary) = self.fetch_wikipedia_summary(&title).await {
                        if Self::is_music_article(&summary, artist) {
                            return Some(summary);
                        }
                    }
                }
            }
        }

        // Step 2: suffix fallback
        let candidates = [
            format!("{album} (album)"),
            format!("{album} ({artist} album)"),
            album.to_string(),
        ];
        for title in &candidates {
            if let Some(summary) = self.fetch_wikipedia_summary(title).await {
                if Self::is_music_article(&summary, artist) {
                    return Some(summary);
                }
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
