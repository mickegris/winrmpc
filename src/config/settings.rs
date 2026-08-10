use crate::mpd::types::RecentAlbum;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MpdServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub password: Option<String>,
    /// Per-server saved partition — restored on reconnect.
    #[serde(default)]
    pub default_partition: Option<String>,
    /// Snapcast server host, if this MPD server has one. Empty/`None` means
    /// "same host as MPD" (the common deployment).
    #[serde(default)]
    pub snapcast_host: Option<String>,
    /// Snapcast control port. `None` means the default, 1705.
    #[serde(default)]
    pub snapcast_port: Option<u16>,
}

impl MpdServer {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Snapcast connection address: configured host (or this server's MPD
    /// host when unset) + configured port (or 1705 when unset).
    pub fn snapcast_addr(&self) -> String {
        let host = self
            .snapcast_host
            .as_deref()
            .filter(|h| !h.is_empty())
            .unwrap_or(&self.host);
        let port = self.snapcast_port.unwrap_or(1705);
        format!("{host}:{port}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Legacy single-server fields — kept so old config files still load.
    // Mirrored from the active server after migration.
    pub mpd_host: String,
    pub mpd_port: u16,
    pub mpd_password: Option<String>,
    #[serde(default)]
    pub default_partition: Option<String>,

    pub art_cache_size_mb: u32,
    pub theme: ThemeConfig,
    #[serde(default = "default_radio_stations")]
    pub radio_stations: Vec<RadioStation>,
    #[serde(default)]
    pub cd_device: Option<String>,
    #[serde(default)]
    pub recent_albums: Vec<RecentAlbum>,

    // Multi-server
    #[serde(default)]
    pub servers: Vec<MpdServer>,
    #[serde(default)]
    pub default_server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub dark_mode: bool,
    pub accent_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RadioStation {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub is_builtin: bool,
}

fn default_radio_stations() -> Vec<RadioStation> {
    vec![
        RadioStation {
            name: "SR P1".into(),
            url: "https://live1.sr.se/p1-aac-320".into(),
            is_builtin: true,
        },
        RadioStation {
            name: "SR P2".into(),
            url: "https://live1.sr.se/p2-aac-320".into(),
            is_builtin: true,
        },
        RadioStation {
            name: "SR P2 Flac".into(),
            url: "https://live1.sr.se/p2-flac".into(),
            is_builtin: true,
        },
        RadioStation {
            name: "SR P3".into(),
            url: "https://live1.sr.se/p3-aac-320".into(),
            is_builtin: true,
        },
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        let server = MpdServer {
            name: "Default".into(),
            host: "127.0.0.1".into(),
            port: 6600,
            password: None,
            default_partition: None,
            snapcast_host: None,
            snapcast_port: None,
        };
        Self {
            mpd_host: "127.0.0.1".into(),
            mpd_port: 6600,
            mpd_password: None,
            default_partition: None,
            art_cache_size_mb: 500,
            theme: ThemeConfig {
                dark_mode: true,
                accent_color: "#4fc3f7".into(),
            },
            radio_stations: default_radio_stations(),
            cd_device: None,
            recent_albums: Vec::new(),
            servers: vec![server],
            default_server: Some("Default".into()),
        }
    }
}

impl AppConfig {
    /// Legacy addr — used as fallback only. Prefer `server_addr`.
    pub fn mpd_addr(&self) -> String {
        format!("{}:{}", self.mpd_host, self.mpd_port)
    }

    pub fn server(&self, name: &str) -> Option<&MpdServer> {
        self.servers.iter().find(|s| s.name == name)
    }

    pub fn server_mut(&mut self, name: &str) -> Option<&mut MpdServer> {
        self.servers.iter_mut().find(|s| s.name == name)
    }

    /// Address for the named server, falling back to first server then legacy fields.
    pub fn server_addr(&self, name: &str) -> String {
        self.server(name)
            .map(|s| s.addr())
            .or_else(|| self.servers.first().map(|s| s.addr()))
            .unwrap_or_else(|| self.mpd_addr())
    }

    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("com", "winrmpc", "winrmpc")
            .map(|p| p.config_dir().to_path_buf())
    }

    pub fn cache_dir() -> Option<PathBuf> {
        ProjectDirs::from("com", "winrmpc", "winrmpc")
            .map(|p| p.cache_dir().to_path_buf())
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("config.toml"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut config) = toml::from_str::<Self>(&content) {
                    // One-time migration: synthesise a server entry from legacy fields.
                    if config.servers.is_empty() {
                        let server = MpdServer {
                            name: "Default".into(),
                            host: config.mpd_host.clone(),
                            port: config.mpd_port,
                            password: config.mpd_password.clone(),
                            default_partition: config.default_partition.clone(),
                            snapcast_host: None,
                            snapcast_port: None,
                        };
                        config.servers.push(server);
                        config.default_server = Some("Default".into());
                        config.save().ok();
                    }
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::config_path() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let content = toml::to_string_pretty(self)?;
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    /// Ensure all built-in stations are present.
    pub fn ensure_builtin_stations(&mut self) {
        let builtins = default_radio_stations();
        for builtin in &builtins {
            if !self.radio_stations.iter().any(|s| s.url == builtin.url) {
                self.radio_stations.push(builtin.clone());
            }
        }
    }

    pub fn add_radio_station(&mut self, name: String, url: String) {
        self.radio_stations.push(RadioStation {
            name,
            url,
            is_builtin: false,
        });
    }

    pub fn remove_radio_station(&mut self, url: &str) {
        self.radio_stations.retain(|s| s.url != url || s.is_builtin);
    }
}
