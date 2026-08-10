//! Low-level async Snapcast JSON-RPC 2.0 protocol: raw TCP,
//! newline-delimited JSON. Mirrors `mpd::protocol`'s connection shape.
//!
//! Responses and push notifications are interleaved on the same connection;
//! notifications have no `"id"`. This app doesn't consume notifications yet
//! (see docs/plans/snapcast-control.md phasing — polling only for v1), so
//! `request()` simply skips any line that isn't the response to its own id
//! rather than routing notifications anywhere.

use crate::snapcast::error::{SnapcastError, SnapcastResult};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct SnapcastConnection {
    reader: BufReader<tokio::io::ReadHalf<TcpStream>>,
    writer: tokio::io::WriteHalf<TcpStream>,
    next_id: AtomicU64,
}

impl SnapcastConnection {
    pub async fn connect(addr: &str) -> SnapcastResult<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| SnapcastError::Connection(format!("Failed to connect to {addr}: {e}")))?;
        let (rh, wh) = tokio::io::split(stream);
        Ok(Self {
            reader: BufReader::new(rh),
            writer: wh,
            next_id: AtomicU64::new(1),
        })
    }

    /// Sends a JSON-RPC request and returns its `result` value. Skips any
    /// interleaved lines that aren't the response to this request's id
    /// (notifications, or — defensively — a stale response) until it finds
    /// one, or the connection closes.
    pub async fn request(&mut self, method: &str, params: Value) -> SnapcastResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::json!({
            "id": id,
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;

        loop {
            let mut raw = String::new();
            let n = self.reader.read_line(&mut raw).await?;
            if n == 0 {
                return Err(SnapcastError::Connection(
                    "Connection closed unexpectedly".into(),
                ));
            }
            let trimmed = raw.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if matches_response_id(&value, id) {
                return extract_result(value);
            }
            // Not our response (a notification, or another request's stale
            // response) — keep reading.
        }
    }
}

/// True when `value` is a JSON-RPC response carrying exactly `id`.
/// Pure, so the response/notification discrimination is unit-testable
/// without a real socket.
fn matches_response_id(value: &Value, id: u64) -> bool {
    value.get("id").and_then(Value::as_u64) == Some(id)
}

/// Pulls `result` out of a JSON-RPC response, or turns an `error` member
/// into an `Err`.
fn extract_result(value: Value) -> SnapcastResult<Value> {
    if let Some(error) = value.get("error") {
        return Err(SnapcastError::Rpc(error.to_string()));
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_response_id_picks_the_right_id_among_interleaved_lines() {
        let notification: Value =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"Client.OnVolumeChanged","params":{}}"#).unwrap();
        let stale: Value = serde_json::from_str(r#"{"id":1,"jsonrpc":"2.0","result":{}}"#).unwrap();
        let ours: Value = serde_json::from_str(r#"{"id":2,"jsonrpc":"2.0","result":{"ok":true}}"#).unwrap();

        assert!(!matches_response_id(&notification, 2));
        assert!(!matches_response_id(&stale, 2));
        assert!(matches_response_id(&ours, 2));
    }

    #[test]
    fn extract_result_returns_result_value() {
        let v: Value = serde_json::from_str(r#"{"id":1,"result":{"a":1}}"#).unwrap();
        let result = extract_result(v).unwrap();
        assert_eq!(result, serde_json::json!({"a": 1}));
    }

    #[test]
    fn extract_result_turns_error_member_into_err() {
        let v: Value =
            serde_json::from_str(r#"{"id":1,"error":{"code":-32601,"message":"no such method"}}"#).unwrap();
        assert!(extract_result(v).is_err());
    }

    #[test]
    fn extract_result_missing_result_is_null_not_error() {
        let v: Value = serde_json::from_str(r#"{"id":1}"#).unwrap();
        assert_eq!(extract_result(v).unwrap(), Value::Null);
    }
}
