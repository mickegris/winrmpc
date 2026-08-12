//! High-level async MPD client with full command coverage.
//! Wraps MpdConnection with typed methods for every MPD operation.

use crate::mpd::commands;
use crate::mpd::error::{MpdError, MpdResult};
use crate::mpd::protocol::MpdConnection;
use crate::mpd::types::*;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Thread-safe async MPD client.
/// Clone-friendly via internal Arc<Mutex>.
pub struct MpdClient {
    conn: Arc<Mutex<Option<MpdConnection>>>,
    addr: String,
}

impl Clone for MpdClient {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
            addr: self.addr.clone(),
        }
    }
}

impl MpdClient {
    pub fn new(addr: &str) -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
            addr: addr.to_string(),
        }
    }

    pub async fn connect(&self) -> MpdResult<()> {
        let connection = MpdConnection::connect(&self.addr).await?;
        tracing::info!(
            "Connected to MPD {} at {}",
            connection.protocol_version,
            self.addr
        );
        *self.conn.lock().await = Some(connection);
        Ok(())
    }

    pub async fn disconnect(&self) {
        *self.conn.lock().await = None;
    }

    pub async fn is_connected(&self) -> bool {
        self.conn.lock().await.is_some()
    }

    async fn cmd(&self, cmd: &str) -> MpdResult<Vec<(String, String)>> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
        let verb = cmd.split_whitespace().next().unwrap_or(cmd);
        // Suppress high-frequency polling verbs from the in-app log, unless
        // they turn out to be slow — a hung status/currentsong poll is
        // exactly the failure mode worth surfacing despite the normal quiet
        // rule (see docs/plans/server-stats-and-diagnostics.md §Part B).
        let quiet = matches!(
            verb,
            "status" | "currentsong" | "playlistinfo"
                | "outputs" | "listpartitions"
                | "idle" | "noidle"
        );
        let started = std::time::Instant::now();
        let result = conn.command(cmd).await;
        let elapsed = started.elapsed();
        let slow = elapsed.as_millis() as u64 >= crate::logger::SLOW_COMMAND_MS;
        let duration_ms = elapsed.as_millis() as u64;
        if let Err(e) = &result {
            if e.is_connection_fatal() {
                tracing::warn!(duration_ms, "connection desynced on {verb} ({e}) — dropping it so the next tick reconnects");
                *guard = None;
            }
        }
        match &result {
            Ok(_) if !quiet || slow => {
                tracing::info!(duration_ms, "→ {verb} ({elapsed:?})")
            }
            Err(e) if !quiet || slow => {
                tracing::warn!(duration_ms, "← ERR {verb}: {e} ({elapsed:?})")
            }
            Err(e) => tracing::debug!(duration_ms, "← ERR {verb}: {e} ({elapsed:?})"),
            _ => {}
        }
        result
    }

    async fn cmd_ok(&self, cmd: &str) -> MpdResult<()> {
        self.cmd(cmd).await.map(|_| ())
    }

    async fn cmd_binary(&self, cmd: &str) -> MpdResult<Option<(Vec<u8>, usize)>> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
        let result = conn.command_binary(cmd).await;
        // The binary path is the one most able to desync the stream: it
        // reads a declared byte count out of the socket, so any disagreement
        // between the header and what we consume leaves the remainder to be
        // misread as the *next* command's response.
        if let Err(e) = &result {
            if e.is_connection_fatal() {
                let verb = cmd.split_whitespace().next().unwrap_or(cmd);
                tracing::warn!("connection desynced on {verb} ({e}) — dropping it so the next tick reconnects");
                *guard = None;
            }
        }
        result
    }

    /// Escape a user-supplied string value for inclusion inside MPD protocol quotes.
    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    // ========================================================================
    // Playback Control
    // ========================================================================

    pub async fn play(&self) -> MpdResult<()> {
        self.cmd_ok("play").await
    }

    pub async fn play_pos(&self, pos: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("play {pos}")).await
    }

    pub async fn play_id(&self, id: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("playid {id}")).await
    }

    pub async fn pause(&self) -> MpdResult<()> {
        self.cmd_ok("pause 1").await
    }

    pub async fn resume(&self) -> MpdResult<()> {
        self.cmd_ok("pause 0").await
    }

    pub async fn toggle_pause(&self) -> MpdResult<()> {
        self.cmd_ok("pause").await
    }

    pub async fn stop(&self) -> MpdResult<()> {
        self.cmd_ok("stop").await
    }

    pub async fn next(&self) -> MpdResult<()> {
        self.cmd_ok("next").await
    }

    pub async fn previous(&self) -> MpdResult<()> {
        self.cmd_ok("previous").await
    }

    pub async fn seek_pos(&self, pos: u32, time: f64) -> MpdResult<()> {
        self.cmd_ok(&format!("seek {pos} {time}")).await
    }

    pub async fn seek_cur(&self, time: f64) -> MpdResult<()> {
        self.cmd_ok(&format!("seekcur {time}")).await
    }

    // ========================================================================
    // Playback Options
    // ========================================================================

    pub async fn set_volume(&self, vol: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("setvol {vol}")).await
    }

    pub async fn set_repeat(&self, on: bool) -> MpdResult<()> {
        self.cmd_ok(&format!("repeat {}", if on { 1 } else { 0 })).await
    }

    pub async fn set_random(&self, on: bool) -> MpdResult<()> {
        self.cmd_ok(&format!("random {}", if on { 1 } else { 0 })).await
    }

    pub async fn set_single(&self, state: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("single {state}")).await
    }

    pub async fn set_consume(&self, state: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("consume {state}")).await
    }

    pub async fn set_crossfade(&self, secs: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("crossfade {secs}")).await
    }

    /// Current replay gain mode ("off"/"track"/"album"/"auto"). Defaults to
    /// "off" if the server omits the field (matches MPD's own default).
    ///
    /// The command is `replay_gain_status`, **with** the underscore between
    /// "replay" and "gain" — `replaygain_status` is not a command and MPD
    /// answers `ACK [5@0] {} unknown command`. Verified against the protocol
    /// docs and a live MPD 0.24.0.
    pub async fn replay_gain_status(&self) -> MpdResult<String> {
        let pairs = self.cmd("replay_gain_status").await?;
        Ok(pairs
            .iter()
            .find(|(k, _)| k == "replay_gain_mode")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "off".to_string()))
    }

    pub async fn set_replay_gain_mode(&self, mode: &str) -> MpdResult<()> {
        self.cmd_ok(&Self::replay_gain_mode_cmd(mode)).await
    }

    fn replay_gain_mode_cmd(mode: &str) -> String {
        format!("replay_gain_mode {mode}")
    }

    // ========================================================================
    // Status / Current Song
    // ========================================================================

    pub async fn status(&self) -> MpdResult<Status> {
        let pairs = self.cmd("status").await?;
        commands::parse_status(&pairs)
    }

    pub async fn current_song(&self) -> MpdResult<Option<Song>> {
        let pairs = self.cmd("currentsong").await?;
        if pairs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(commands::parse_song(&pairs)))
        }
    }

    pub async fn stats(&self) -> MpdResult<Stats> {
        let pairs = self.cmd("stats").await?;
        commands::parse_stats(&pairs)
    }

    // ========================================================================
    // Queue
    // ========================================================================

    pub async fn queue(&self) -> MpdResult<Vec<Song>> {
        let pairs = self.cmd("playlistinfo").await?;
        Ok(commands::parse_songs(&pairs))
    }

    pub async fn add(&self, uri: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("add \"{}\"", Self::escape(uri))).await
    }

    pub async fn add_id(&self, uri: &str) -> MpdResult<u32> {
        let pairs = self.cmd(&format!("addid \"{}\"", Self::escape(uri))).await?;
        pairs
            .iter()
            .find(|(k, _)| k == "Id")
            .and_then(|(_, v)| v.parse().ok())
            .ok_or_else(|| MpdError::Parse("No Id in addid response".into()))
    }

    /// Bulk-add multiple URIs in one round trip via `command_list`, instead
    /// of one `add` per URI (which starves the shared connection mutex —
    /// and the 500ms status poll that also needs it — for the duration of
    /// the whole list; see docs/plans/review-fixes-performance.md §1, and
    /// mikMPD's own "Bulk enqueue is server-side" note for the same lesson
    /// learned the hard way there). MPD applies the list in order and stops
    /// at the first failing command — tracks queued before the failure stay
    /// queued; nothing after it is attempted.
    pub async fn add_all(&self, uris: &[String]) -> MpdResult<()> {
        if uris.is_empty() {
            return Ok(());
        }
        let cmds = Self::build_add_commands(uris);
        let refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
        let started = std::time::Instant::now();
        let result = conn.command_list(&refs).await;
        let elapsed = started.elapsed();
        // `duration_ms` must be a structured tracing field, not just text in
        // the message: `logger::InAppLayer` reads that field into
        // `LogEntry.duration_ms`, which is what drives `is_slow()` and the
        // Log view's ⚠ prefix. A bulk enqueue is exactly the command worth
        // flagging when it runs long, so it has to carry the field like
        // `cmd()` does.
        let duration_ms = elapsed.as_millis() as u64;
        if let Err(e) = &result {
            if e.is_connection_fatal() {
                *guard = None;
            }
        }
        match &result {
            Ok(_) => tracing::info!(duration_ms, "→ add_all ({} tracks, {elapsed:?})", uris.len()),
            Err(e) => tracing::warn!(
                duration_ms,
                "← ERR add_all: {e} ({elapsed:?}) — MPD aborts a command list at the \
                 first failure, so some of the {} tracks were not queued",
                uris.len()
            ),
        }
        result.map(|_| ())
    }

    /// Pure command-string formation for `add_all`, split out so it's
    /// testable without a live connection (mirrors the `escape()` tests'
    /// style for the rest of this module).
    fn build_add_commands(uris: &[String]) -> Vec<String> {
        uris.iter()
            .map(|u| format!("add \"{}\"", Self::escape(u)))
            .collect()
    }

    pub async fn delete_pos(&self, pos: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("delete {pos}")).await
    }

    pub async fn delete_id(&self, id: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("deleteid {id}")).await
    }

    pub async fn clear(&self) -> MpdResult<()> {
        self.cmd_ok("clear").await
    }

    pub async fn move_pos(&self, from: u32, to: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("move {from} {to}")).await
    }

    pub async fn shuffle(&self) -> MpdResult<()> {
        self.cmd_ok("shuffle").await
    }

    /// Delete all queue items from `start` to the end of the queue.
    /// Sends `delete {start}:` (open-ended range).
    pub async fn delete_range_from(&self, start: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("delete {start}:")).await
    }

    // ========================================================================
    // Database / Library
    // ========================================================================

    pub async fn list_tag(&self, tag: &str) -> MpdResult<Vec<String>> {
        let pairs = self.cmd(&format!("list {tag}")).await?;
        Ok(commands::parse_tag_list(&pairs, tag))
    }

    pub async fn list_tag_filtered(
        &self,
        tag: &str,
        filter_tag: &str,
        filter_val: &str,
    ) -> MpdResult<Vec<String>> {
        let pairs = self
            .cmd(&format!("list {tag} {filter_tag} \"{}\"", Self::escape(filter_val)))
            .await?;
        Ok(commands::parse_tag_list(&pairs, tag))
    }

    /// `list Album group AlbumArtist` (MPD 0.21+) — returns
    /// `(album_artist, album)` pairs, so same-named albums by different
    /// artists can be told apart. Falls back to a flat, artist-less list
    /// (empty-string artist) on ACK from a pre-0.21 server that doesn't
    /// support `group`.
    pub async fn list_albums_by_artist(&self) -> MpdResult<Vec<(String, String)>> {
        match self.cmd("list Album group AlbumArtist").await {
            Ok(pairs) => Ok(commands::parse_grouped_values(&pairs, "AlbumArtist", "Album")),
            Err(_) => {
                let albums = self.list_tag("Album").await?;
                Ok(albums.into_iter().map(|a| (String::new(), a)).collect())
            }
        }
    }

    /// `find Album "X" AlbumArtist "Y"` — artist-scoped album lookup, used
    /// once album identity is artist-aware so same-named albums by
    /// different artists don't mix tracks.
    pub async fn find_album_by_artist(&self, album: &str, artist: &str) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!(
                "find Album \"{}\" AlbumArtist \"{}\"",
                Self::escape(album),
                Self::escape(artist)
            ))
            .await?;
        Ok(commands::parse_songs(&pairs))
    }

    pub async fn find(&self, tag: &str, value: &str) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!("find {tag} \"{}\"", Self::escape(value)))
            .await?;
        Ok(commands::parse_songs(&pairs))
    }

    /// Songs added/modified since `since` (MPD timestamp format,
    /// `YYYY-MM-DDTHH:MM:SSZ`), bounded to `limit` results. Unbounded
    /// `modified-since` queries can outrun the socket's read timeout on a
    /// large library, so this always uses a `window`.
    pub async fn find_recently_added(&self, since: &str, limit: u32) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!(
                "find \"(modified-since '{}')\" window 0:{limit}",
                Self::escape(since)
            ))
            .await?;
        Ok(commands::parse_songs(&pairs))
    }

    pub async fn search(&self, tag: &str, value: &str) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!("search {tag} \"{}\"", Self::escape(value)))
            .await?;
        Ok(commands::parse_songs(&pairs))
    }

    pub async fn search_add(&self, tag: &str, value: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("searchadd {tag} \"{}\"", Self::escape(value))).await
    }

    pub async fn find_add(&self, tag: &str, value: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("findadd {tag} \"{}\"", Self::escape(value))).await
    }

    pub async fn lsinfo(&self, path: &str) -> MpdResult<Vec<DirectoryEntry>> {
        let cmd = if path.is_empty() {
            "lsinfo".to_string()
        } else {
            format!("lsinfo \"{}\"", Self::escape(path))
        };
        let pairs = self.cmd(&cmd).await?;
        Ok(commands::parse_directory_listing(&pairs))
    }

    pub async fn update(&self, path: Option<&str>) -> MpdResult<u32> {
        let cmd = match path {
            Some(p) => format!("update \"{}\"", Self::escape(p)),
            None => "update".to_string(),
        };
        let pairs = self.cmd(&cmd).await?;
        pairs
            .iter()
            .find(|(k, _)| k == "updating_db")
            .and_then(|(_, v)| v.parse().ok())
            .ok_or_else(|| MpdError::Parse("No updating_db in response".into()))
    }

    // ========================================================================
    // Stored Playlists
    // ========================================================================

    pub async fn list_playlists(&self) -> MpdResult<Vec<PlaylistInfo>> {
        let pairs = self.cmd("listplaylists").await?;
        Ok(commands::parse_directory_listing(&pairs)
            .into_iter()
            .filter_map(|e| match e {
                DirectoryEntry::Playlist(p) => Some(p),
                _ => None,
            })
            .collect())
    }

    pub async fn list_playlist(&self, name: &str) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!("listplaylistinfo \"{}\"", Self::escape(name)))
            .await?;
        let mut songs = commands::parse_songs(&pairs);
        // Some MPD versions omit "Pos" from listplaylistinfo; assign it from
        // the record index so duplicate files in a playlist still resolve to
        // a unique, correct position for play-at/remove/move.
        for (i, song) in songs.iter_mut().enumerate() {
            if song.pos.is_none() {
                song.pos = Some(i as u32);
            }
        }
        Ok(songs)
    }

    pub async fn save_playlist(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("save \"{}\"", Self::escape(name))).await
    }

    pub async fn delete_playlist(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("rm \"{}\"", Self::escape(name))).await
    }

    pub async fn load_playlist(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("load \"{}\"", Self::escape(name))).await
    }

    pub async fn playlist_add(&self, name: &str, uri: &str) -> MpdResult<()> {
        self.cmd_ok(&format!(
            "playlistadd \"{}\" \"{}\"",
            Self::escape(name),
            Self::escape(uri)
        ))
        .await
    }

    pub async fn playlist_delete(&self, name: &str, pos: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("playlistdelete \"{}\" {pos}", Self::escape(name)))
            .await
    }

    pub async fn playlist_move(&self, name: &str, from: u32, to: u32) -> MpdResult<()> {
        self.cmd_ok(&format!(
            "playlistmove \"{}\" {from} {to}",
            Self::escape(name)
        ))
        .await
    }

    pub async fn rename_playlist(&self, old: &str, new: &str) -> MpdResult<()> {
        self.cmd_ok(&format!(
            "rename \"{}\" \"{}\"",
            Self::escape(old),
            Self::escape(new)
        ))
        .await
    }

    // ========================================================================
    // Outputs (MUST HAVE)
    // ========================================================================

    pub async fn outputs(&self) -> MpdResult<Vec<Output>> {
        let pairs = self.cmd("outputs").await?;
        Ok(commands::parse_outputs(&pairs))
    }

    pub async fn enable_output(&self, id: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("enableoutput {id}")).await
    }

    pub async fn disable_output(&self, id: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("disableoutput {id}")).await
    }

    pub async fn toggle_output(&self, id: u32) -> MpdResult<()> {
        self.cmd_ok(&format!("toggleoutput {id}")).await
    }

    pub async fn move_output(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("moveoutput \"{}\"", Self::escape(name))).await
    }

    // ========================================================================
    // Partitions (MUST HAVE)
    // ========================================================================

    pub async fn list_partitions(&self) -> MpdResult<Vec<Partition>> {
        let pairs = self.cmd("listpartitions").await?;
        Ok(commands::parse_partitions(&pairs))
    }

    pub async fn switch_partition(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("partition \"{}\"", Self::escape(name))).await
    }

    pub async fn new_partition(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("newpartition \"{}\"", Self::escape(name))).await
    }

    pub async fn delete_partition(&self, name: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("delpartition \"{}\"", Self::escape(name))).await
    }

    // ========================================================================
    // Album Art (binary protocol)
    // ========================================================================

    /// Shared chunked binary-read loop for `readpicture`/`albumart`, which
    /// share an identical wire protocol (offset-paginated binary chunks)
    /// and differ only in which MPD verb is sent.
    async fn fetch_binary_art(&self, verb: &str, uri: &str) -> MpdResult<Option<Vec<u8>>> {
        let mut offset: usize = 0;
        let mut full_data = Vec::new();
        let mut total_size: usize = 0;

        loop {
            let result = self
                .cmd_binary(&format!("{verb} \"{}\" {offset}", Self::escape(uri)))
                .await;

            match result {
                Ok(Some((chunk, size))) => {
                    if total_size == 0 {
                        total_size = size;
                        full_data.reserve(total_size);
                    }
                    offset += chunk.len();
                    full_data.extend_from_slice(&chunk);

                    if offset >= total_size {
                        break;
                    }
                }
                Ok(None) => return Ok(None),
                Err(e) => return Err(e),
            }
        }

        if full_data.is_empty() {
            Ok(None)
        } else {
            tracing::info!("→ {verb} \"{uri}\" ({} KB)", full_data.len() / 1024);
            Ok(Some(full_data))
        }
    }

    /// Art embedded in the song file's own tags ("readpicture"). Tried
    /// first — on a tagged library this is the art that actually exists,
    /// so probing it before the separate-cover-file path avoids a wasted
    /// round trip per album (see
    /// docs/plans/art-wikipedia-fetch-order-and-caching.md §1).
    pub async fn tag_art(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> {
        self.fetch_binary_art("readpicture", uri).await
    }

    /// A separate cover-file image beside the song, e.g. `cover.jpg`
    /// ("albumart"). Tried after `tag_art`.
    pub async fn cover_file_art(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> {
        self.fetch_binary_art("albumart", uri).await
    }

    // ========================================================================
    // Idle (for subscriptions)
    // ========================================================================

    /// Wait for MPD events. Returns list of changed subsystems.
    /// This blocks until something changes or idle is cancelled.
    pub async fn idle(&self, subsystems: &[&str]) -> MpdResult<Vec<String>> {
        let cmd = if subsystems.is_empty() {
            "idle".to_string()
        } else {
            format!("idle {}", subsystems.join(" "))
        };
        let pairs = self.cmd(&cmd).await?;
        Ok(pairs
            .into_iter()
            .filter(|(k, _)| k == "changed")
            .map(|(_, v)| v)
            .collect())
    }

    pub async fn noidle(&self) -> MpdResult<()> {
        self.cmd_ok("noidle").await
    }

    // ========================================================================
    // Authentication
    // ========================================================================

    pub async fn password(&self, pw: &str) -> MpdResult<()> {
        self.cmd_ok(&format!("password \"{}\"", Self::escape(pw))).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_passes_plain_text_through() {
        assert_eq!(MpdClient::escape("Pink Floyd"), "Pink Floyd");
        assert_eq!(MpdClient::escape(""), "");
        assert_eq!(MpdClient::escape("AC/DC"), "AC/DC");
    }

    #[test]
    fn escape_escapes_double_quotes() {
        // A quote would otherwise close the MPD argument early.
        assert_eq!(MpdClient::escape(r#"say "hi""#), r#"say \"hi\""#);
    }

    #[test]
    fn escape_escapes_backslashes() {
        assert_eq!(MpdClient::escape(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_handles_backslash_then_quote_in_order() {
        // Backslash must be escaped before the quote, otherwise the
        // backslash we add for the quote would itself get doubled.
        // Input:  \"   →  \\\"
        assert_eq!(MpdClient::escape("\\\""), "\\\\\\\"");
    }

    #[test]
    fn escape_neutralizes_injection_attempt() {
        // A naive interpolation would let this close the argument and run a
        // second command. After escaping, every embedded quote is preceded
        // by a backslash, so the whole thing stays one string argument.
        let malicious = r#"x" ; clear ; add "evil"#;
        let escaped = MpdClient::escape(malicious);
        assert_eq!(escaped, r#"x\" ; clear ; add \"evil"#);
        // No bare (unescaped) double-quote survives.
        let bytes = escaped.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'"' {
                assert!(i > 0 && bytes[i - 1] == b'\\', "bare quote at {i}");
            }
        }
    }

    #[test]
    fn replay_gain_mode_cmd_formats_mode() {
        assert_eq!(MpdClient::replay_gain_mode_cmd("off"), "replay_gain_mode off");
        assert_eq!(MpdClient::replay_gain_mode_cmd("auto"), "replay_gain_mode auto");
    }

    #[test]
    fn build_add_commands_one_add_per_uri_escaped() {
        let uris = vec!["a.flac".to_string(), "dir/b \"weird\".mp3".to_string()];
        let cmds = MpdClient::build_add_commands(&uris);
        assert_eq!(cmds, vec![
            "add \"a.flac\"".to_string(),
            "add \"dir/b \\\"weird\\\".mp3\"".to_string(),
        ]);
    }

    #[test]
    fn connection_fatal_classification() {
        // An ACK is a well-framed reply — the socket is fine, keep it.
        assert!(!MpdError::Server { code: 50, message: "No file exists".into() }
            .is_connection_fatal());
        assert!(!MpdError::NotConnected.is_connection_fatal());
        // These all mean the stream position is unknown.
        assert!(MpdError::Connection("closed".into()).is_connection_fatal());
        assert!(MpdError::Parse("binary: bad".into()).is_connection_fatal());
        assert!(MpdError::Protocol("bad greeting".into()).is_connection_fatal());
        assert!(MpdError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        ))
        .is_connection_fatal());
    }

    #[test]
    fn build_add_commands_empty_input() {
        assert!(MpdClient::build_add_commands(&[]).is_empty());
    }
}
