//! High-level async Snapcast client — typed methods over the raw JSON-RPC
//! connection. Mirrors `MpdClient`'s clone-cheap `Arc<Mutex<Option<...>>>`
//! shape, but is a fully separate connection/lifecycle: Snapcast may be
//! absent or unreachable while MPD is fine, and this client is only
//! connected while `View::Snapcast` is open (see `App::on_view_enter`).

use crate::snapcast::error::{SnapcastError, SnapcastResult};
use crate::snapcast::protocol::SnapcastConnection;
use crate::snapcast::types::{decode_snap_status, SnapGroup, SnapStream};
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

    /// Whether a connection object is currently held. Note this is a
    /// bookkeeping check, not a socket probe — it only stays accurate
    /// because `request()` drops the connection the moment a transport
    /// error proves it dead, so a `true` here means "not known to be
    /// broken" rather than "verified alive".
    pub async fn is_connected(&self) -> bool {
        self.conn.lock().await.is_some()
    }

    /// The address this client was constructed with — lets a caller detect
    /// a stale client (e.g. the server's `snapcast_host`/`snapcast_port`
    /// were edited) and rebuild rather than keep talking to the old one.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    async fn request(&self, method: &str, params: Value) -> SnapcastResult<Value> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(SnapcastError::NotConnected)?;
        let result = conn.request(method, params).await;
        // A transport failure means this socket is dead (snapserver
        // restarted, LAN blip, laptop resumed from sleep). Drop it here so
        // `is_connected()` reports false and the next `on_view_enter` /
        // poll reconnects. Without this the connection object lingers as
        // `Some` forever, the `if !is_connected() { connect() }` guard
        // never fires again, and the whole view is stuck erroring until
        // the app is restarted or the server switched.
        //
        // `Rpc` errors are deliberately excluded: those are well-formed
        // JSON-RPC error *responses*, which prove the connection is fine.
        if matches!(
            result,
            Err(SnapcastError::Connection(_) | SnapcastError::Io(_) | SnapcastError::Json(_))
        ) {
            *guard = None;
        }
        result
    }

    /// `Server.GetStatus` — the full group/client/stream tree.
    pub async fn get_status(&self) -> SnapcastResult<(Vec<SnapGroup>, Vec<SnapStream>)> {
        let mut result = self.request("Server.GetStatus", serde_json::json!({})).await?;
        // Take ownership of the "server" sub-value instead of cloning it,
        // since `result` isn't needed after this — decode_snap_status then
        // parses it exactly once instead of the previous two separate
        // decode calls each cloning + re-parsing the whole tree.
        let server = result.get_mut("server").map(std::mem::take).unwrap_or(Value::Null);
        Ok(decode_snap_status(&server))
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
