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
        conn.command(cmd).await
    }

    async fn cmd_ok(&self, cmd: &str) -> MpdResult<()> {
        self.cmd(cmd).await.map(|_| ())
    }

    async fn cmd_binary(&self, cmd: &str) -> MpdResult<Option<(Vec<u8>, usize)>> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(MpdError::NotConnected)?;
        conn.command_binary(cmd).await
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

    pub async fn find(&self, tag: &str, value: &str) -> MpdResult<Vec<Song>> {
        let pairs = self
            .cmd(&format!("find {tag} \"{}\"", Self::escape(value)))
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
        Ok(commands::parse_songs(&pairs))
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

    /// Fetch full album art for a song URI.
    /// Uses "albumart" command with chunked binary reads.
    pub async fn album_art(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> {
        let mut offset: usize = 0;
        let mut full_data = Vec::new();
        let mut total_size: usize = 0;

        loop {
            let result = self
                .cmd_binary(&format!("albumart \"{}\" {offset}", Self::escape(uri)))
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
                Err(MpdError::Server { code: 50, .. }) => {
                    // No album art, try readpicture
                    return self.read_picture(uri).await;
                }
                Err(e) => return Err(e),
            }
        }

        if full_data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(full_data))
        }
    }

    /// Fallback: readpicture command for embedded art
    pub async fn read_picture(&self, uri: &str) -> MpdResult<Option<Vec<u8>>> {
        let mut offset: usize = 0;
        let mut full_data = Vec::new();
        let mut total_size: usize = 0;

        loop {
            let result = self
                .cmd_binary(&format!("readpicture \"{}\" {offset}", Self::escape(uri)))
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
            Ok(Some(full_data))
        }
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
