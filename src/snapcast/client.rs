//! High-level async Snapcast client — typed methods over the raw JSON-RPC
//! connection. Mirrors `MpdClient`'s clone-cheap `Arc<Mutex<Option<...>>>`
//! shape, but is a fully separate connection/lifecycle: Snapcast may be
//! absent or unreachable while MPD is fine, and this client is only
//! connected while `View::Snapcast` is open (see `App::on_view_enter`).

use crate::snapcast::error::{SnapcastError, SnapcastResult};
use crate::snapcast::protocol::SnapcastConnection;
use crate::snapcast::types::{decode_snap_groups, decode_snap_streams, SnapGroup, SnapStream};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SnapcastClient {
    conn: Arc<Mutex<Option<SnapcastConnection>>>,
    addr: String,
}

impl SnapcastClient {
    pub fn new(addr: &str) -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
            addr: addr.to_string(),
        }
    }

    pub async fn connect(&self) -> SnapcastResult<()> {
        let connection = SnapcastConnection::connect(&self.addr).await?;
        *self.conn.lock().await = Some(connection);
        Ok(())
    }

    pub async fn disconnect(&self) {
        *self.conn.lock().await = None;
    }

    async fn request(&self, method: &str, params: Value) -> SnapcastResult<Value> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(SnapcastError::NotConnected)?;
        conn.request(method, params).await
    }

    /// `Server.GetStatus` — the full group/client/stream tree.
    pub async fn get_status(&self) -> SnapcastResult<(Vec<SnapGroup>, Vec<SnapStream>)> {
        let result = self.request("Server.GetStatus", serde_json::json!({})).await?;
        let server = result.get("server").cloned().unwrap_or(Value::Null);
        Ok((decode_snap_groups(&server), decode_snap_streams(&server)))
    }

    pub async fn set_volume(&self, client_id: &str, percent: u8, muted: bool) -> SnapcastResult<()> {
        let percent = percent.min(100);
        self.request(
            "Client.SetVolume",
            serde_json::json!({
                "id": client_id,
                "volume": { "percent": percent, "muted": muted },
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_group_mute(&self, group_id: &str, muted: bool) -> SnapcastResult<()> {
        self.request(
            "Group.SetMute",
            serde_json::json!({ "id": group_id, "mute": muted }),
        )
        .await?;
        Ok(())
    }

    pub async fn set_group_stream(&self, group_id: &str, stream_id: &str) -> SnapcastResult<()> {
        self.request(
            "Group.SetStream",
            serde_json::json!({ "id": group_id, "stream_id": stream_id }),
        )
        .await?;
        Ok(())
    }
}
