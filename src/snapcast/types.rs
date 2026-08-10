//! Snapcast domain types, decoded from `Server.GetStatus`'s `result.server`
//! JSON value. Snapcast's wire format is already JSON, so `serde_json` does
//! the parsing — no hand-rolled line parser needed here, unlike the
//! MPD-adjacent modules in this codebase.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SnapVolume {
    pub percent: u8,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawHost {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawClientConfig {
    #[serde(default)]
    name: String,
    volume: SnapVolume,
    #[serde(default)]
    latency: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawClient {
    id: String,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    host: RawHost,
    config: RawClientConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapClient {
    pub id: String,
    pub connected: bool,
    pub host_name: String,
    pub name: String,
    pub volume: u8,
    pub muted: bool,
    pub latency: i32,
}

impl SnapClient {
    /// The configured client name, falling back to its hostname when unset.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.host_name
        } else {
            &self.name
        }
    }
}

impl From<RawClient> for SnapClient {
    fn from(r: RawClient) -> Self {
        Self {
            id: r.id,
            connected: r.connected,
            host_name: r.host.name,
            name: r.config.name,
            volume: r.config.volume.percent,
            muted: r.config.volume.muted,
            latency: r.config.latency,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawGroup {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    muted: bool,
    #[serde(default)]
    stream_id: String,
    #[serde(default)]
    clients: Vec<RawClient>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapGroup {
    pub id: String,
    pub name: String,
    pub muted: bool,
    pub stream_id: String,
    pub clients: Vec<SnapClient>,
}

impl SnapGroup {
    /// The configured group name, falling back to its stream id when unset.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.stream_id
        } else {
            &self.name
        }
    }
}

impl From<RawGroup> for SnapGroup {
    fn from(r: RawGroup) -> Self {
        Self {
            id: r.id,
            name: r.name,
            muted: r.muted,
            stream_id: r.stream_id,
            clients: r.clients.into_iter().map(SnapClient::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SnapStream {
    pub id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawServer {
    #[serde(default)]
    groups: Vec<RawGroup>,
    #[serde(default)]
    streams: Vec<SnapStream>,
}

/// Decodes the `result.server` value from `Server.GetStatus`. Degrades to
/// an empty `Vec` (not an error) on unexpected shape, so a transient/odd
/// response shows "nothing to display" rather than a parse-error toast.
/// Decodes both groups and streams from one `serde_json::from_value` call —
/// `get_status` uses this instead of calling `decode_snap_groups` and
/// `decode_snap_streams` separately, which each cloned and fully
/// re-parsed the whole tree (twice the work, every 2s poll).
pub fn decode_snap_status(server: &Value) -> (Vec<SnapGroup>, Vec<SnapStream>) {
    serde_json::from_value::<RawServer>(server.clone())
        .map(|r| (r.groups.into_iter().map(SnapGroup::from).collect(), r.streams))
        .unwrap_or_default()
}

pub fn decode_snap_groups(server: &Value) -> Vec<SnapGroup> {
    decode_snap_status(server).0
}

pub fn decode_snap_streams(server: &Value) -> Vec<SnapStream> {
    decode_snap_status(server).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "groups": [
                {
                    "id": "group1",
                    "name": "Living Room",
                    "muted": false,
                    "stream_id": "stream1",
                    "clients": [
                        {
                            "id": "client1",
                            "connected": true,
                            "host": {"name": "kitchen-pi"},
                            "config": {
                                "name": "Kitchen",
                                "volume": {"percent": 65, "muted": false},
                                "latency": 20
                            }
                        },
                        {
                            "id": "client2",
                            "connected": false,
                            "host": {"name": "bedroom-pi"},
                            "config": {
                                "name": "",
                                "volume": {"percent": 40, "muted": true},
                                "latency": 0
                            }
                        }
                    ]
                }
            ],
            "streams": [
                {"id": "stream1", "status": "playing"}
            ]
        })
    }

    #[test]
    fn decode_snap_groups_from_fixture() {
        let groups = decode_snap_groups(&fixture());
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.id, "group1");
        assert_eq!(g.display_name(), "Living Room");
        assert_eq!(g.clients.len(), 2);
    }

    #[test]
    fn decode_snap_groups_client_fields_and_connected_state() {
        let groups = decode_snap_groups(&fixture());
        let clients = &groups[0].clients;
        assert_eq!(clients[0].display_name(), "Kitchen");
        assert_eq!(clients[0].volume, 65);
        assert!(clients[0].connected);
        assert!(!clients[1].connected);
    }

    #[test]
    fn client_display_name_falls_back_to_host_name() {
        let groups = decode_snap_groups(&fixture());
        // client2 has an empty config name.
        assert_eq!(groups[0].clients[1].display_name(), "bedroom-pi");
    }

    #[test]
    fn group_display_name_falls_back_to_stream_id() {
        let mut f = fixture();
        f["groups"][0]["name"] = serde_json::json!("");
        let groups = decode_snap_groups(&f);
        assert_eq!(groups[0].display_name(), "stream1");
    }

    #[test]
    fn decode_snap_streams_from_fixture() {
        let streams = decode_snap_streams(&fixture());
        assert_eq!(streams, vec![SnapStream { id: "stream1".into(), status: "playing".into() }]);
    }

    #[test]
    fn decode_snap_status_returns_both_halves_matching_the_individual_decoders() {
        let (groups, streams) = decode_snap_status(&fixture());
        assert_eq!(groups, decode_snap_groups(&fixture()));
        assert_eq!(streams, decode_snap_streams(&fixture()));
    }

    #[test]
    fn decode_snap_groups_unexpected_shape_degrades_to_empty() {
        assert!(decode_snap_groups(&serde_json::json!("not an object")).is_empty());
        assert!(decode_snap_groups(&serde_json::json!({})).is_empty());
    }
}
