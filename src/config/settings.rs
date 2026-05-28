use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub mpd_host: String,
    pub mpd_port: u16,
    pub mpd_password: Option<String>,
    pub default_partition: Option<String>,
    pub art_cache_size_mb: u32,
    pub theme: ThemeConfig,
    #[serde(default = "default_radio_stations")]
    pub radio_stations: Vec<RadioStation>,
    /// Optional CD device path on the MPD server, e.g. `/dev/sr0`.
    /// When set, track listing uses `lsinfo cdda://{device}` instead of
    /// the add/delete probe loop.
    #[serde(default)]
    pub cd_device: Option<String>,
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
        }
    }
}

impl AppConfig {
    pub fn mpd_addr(&self) -> String {
        format!("{}:{}", self.mpd_host, self.mpd_port)
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
                if let Ok(config) = toml::from_str(&content) {
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

    /// Ensure all built-in stations are present (in case config was saved
    /// before a new built-in was added).
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
