use crate::mpd::types::RecentAlbum;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Set when a config file existed but couldn't be parsed. Makes `save`
    /// a no-op so a typo in the TOML isn't answered by silently replacing
    /// the user's settings with defaults. Never serialized — it describes
    /// this run, not the configuration.
    #[serde(skip)]
    pub load_failed: bool,

    // Legacy single-server fields — kept so old config files still load.
    // Mirrored from the active server after migration.
    //
    // All `#[serde(default)]`: these are *legacy*, so a config written in the
    // current `[[servers]]` form has no reason to carry them. Without the
    // defaults they were silently mandatory, and since `load()` falls back to
    // `Self::default()` on any parse error — then overwrites the file on the
    // next `save()` — a hand-written multi-server config would have quietly
    // wiped the user's settings instead of reporting a problem.
    #[serde(default = "default_mpd_host")]
    pub mpd_host: String,
    #[serde(default = "default_mpd_port")]
    pub mpd_port: u16,
    #[serde(default)]
    pub mpd_password: Option<String>,
    #[serde(default)]
    pub default_partition: Option<String>,

    #[serde(default = "default_art_cache_size_mb")]
    pub art_cache_size_mb: u32,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default = "default_radio_stations")]
    pub radio_stations: Vec<RadioStation>,
    #[serde(default)]
    pub cd_device: Option<String>,
    #[serde(default)]
    pub recent_albums: Vec<RecentAlbum>,
    /// Cover grid (true) vs compact list (false) for the Albums, Recently
    /// Added and Recently Played views. One flag for all three so the app
    /// doesn't feel inconsistent between them.
    #[serde(default)]
    pub album_grid_view: bool,

    // Multi-server
    #[serde(default)]
    pub servers: Vec<MpdServer>,
    #[serde(default)]
    pub default_server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub dark_mode: bool,
    pub accent_color: String,
}

/// Written out by hand rather than derived: the derived `Default` would be
/// `false` / `""`, not the dark theme + accent the app actually ships with.
/// `AppConfig::default()` defers to this so there's one source of truth.
impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            dark_mode: true,
            accent_color: "#4fc3f7".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RadioStation {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub is_builtin: bool,
}

fn default_mpd_host() -> String {
    "127.0.0.1".into()
}

fn default_mpd_port() -> u16 {
    6600
}

fn default_art_cache_size_mb() -> u32 {
    500
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
            load_failed: false,
            mpd_host: "127.0.0.1".into(),
            mpd_port: 6600,
            mpd_password: None,
            default_partition: None,
            art_cache_size_mb: default_art_cache_size_mb(),
            theme: ThemeConfig::default(),
            radio_stations: default_radio_stations(),
            cd_device: None,
            recent_albums: Vec::new(),
            album_grid_view: false,
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
        Self::load_from(Self::config_path())
    }

    /// Path-injectable core of `load`, so all three cases can be tested
    /// without touching the real user config.
    ///
    /// The distinction that matters is **missing vs. unparseable**:
    /// - *Missing* (first launch) — write the defaults out. The file used to
    ///   be created lazily, on whatever action next called `save()`, which
    ///   left new users with no file and no folder to edit. That matters
    ///   because `art_cache_size_mb`, `theme` and the per-server
    ///   `snapcast_host`/`snapcast_port` have **no UI at all** — the file is
    ///   the only way to set them.
    /// - *Present but unparseable* — keep the defaults for this session,
    ///   log loudly, and set `load_failed` so `save` refuses to write. One
    ///   stray character in the TOML would otherwise be silently overwritten
    ///   with defaults by the next setting change, taking every server,
    ///   radio station and saved partition with it.
    fn load_from(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };

        let Ok(content) = std::fs::read_to_string(&path) else {
            // No config yet: write one so there is something to edit.
            let config = Self::default();
            if let Err(e) = config.save_to(Some(&path)) {
                tracing::warn!("could not create {}: {e}", path.display());
            }
            return config;
        };

        match toml::from_str::<Self>(&content) {
            Ok(mut config) => {
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
                    config.save_to(Some(&path)).ok();
                }
                config
            }
            Err(e) => {
                tracing::error!(
                    "{} could not be parsed ({e}) — running with defaults for \
                     this session and leaving the file untouched. Fix or \
                     delete it; settings will not be saved until then.",
                    path.display()
                );
                Self {
                    load_failed: true,
                    ..Self::default()
                }
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(Self::config_path().as_deref())
    }

    fn save_to(&self, path: Option<&Path>) -> anyhow::Result<()> {
        if self.load_failed {
            tracing::warn!(
                "not saving settings: the config file on disk is unparseable \
                 and would be overwritten"
            );
            return Ok(());
        }
        if let Some(path) = path {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The multi-server config shape documented in README.md must actually
    /// deserialize. It carries none of the legacy `mpd_*` fields, which were
    /// silently mandatory until they got `#[serde(default)]` — and because
    /// `load()` falls back to `Self::default()` on a parse error and then
    /// overwrites the file on the next `save()`, getting this wrong would
    /// quietly destroy a hand-written config rather than report anything.
    #[test]
    fn readme_multi_server_config_example_parses() {
        let src = r##"
default_server = "Living Room"
art_cache_size_mb = 500

[[servers]]
name = "Living Room"
host = "192.168.1.50"
port = 6600

[[servers]]
name = "Office"
host = "192.168.1.51"
port = 6600

[theme]
dark_mode = true
accent_color = "#4fc3f7"
"##;
        let config: AppConfig = toml::from_str(src).expect("README example must parse");
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.default_server.as_deref(), Some("Living Room"));
        assert_eq!(config.server("Office").map(|s| s.host.as_str()), Some("192.168.1.51"));
        // Legacy fields fall back to the built-in defaults.
        assert_eq!(config.mpd_host, "127.0.0.1");
        assert_eq!(config.mpd_port, 6600);
    }

    /// The pre-0.4 single-server shape must still load, since `load()`'s
    /// migration reads these fields to synthesise a `[[servers]]` entry.
    #[test]
    fn legacy_single_server_config_still_parses() {
        let src = r##"
mpd_host = "192.168.1.50"
mpd_port = 6600
art_cache_size_mb = 250

[theme]
dark_mode = false
accent_color = "#ff0000"
"##;
        let config: AppConfig = toml::from_str(src).expect("legacy config must still parse");
        assert_eq!(config.mpd_host, "192.168.1.50");
        assert_eq!(config.art_cache_size_mb, 250);
        assert!(!config.theme.dark_mode);
        // Not yet migrated — `load()` does that, not `from_str`.
        assert!(config.servers.is_empty());
    }

    /// A bare minimum config (just one server) must work, and every
    /// defaulted field must land on the same value `Default` uses.
    #[test]
    fn minimal_config_matches_defaults_for_omitted_fields() {
        let src = r#"
[[servers]]
name = "Only"
host = "10.0.0.1"
port = 6600
"#;
        let config: AppConfig = toml::from_str(src).expect("minimal config must parse");
        let d = AppConfig::default();
        assert_eq!(config.art_cache_size_mb, d.art_cache_size_mb);
        assert_eq!(config.theme.dark_mode, d.theme.dark_mode);
        assert_eq!(config.theme.accent_color, d.theme.accent_color);
        assert_eq!(config.mpd_host, d.mpd_host);
        assert_eq!(config.mpd_port, d.mpd_port);
        assert!(!config.radio_stations.is_empty(), "built-ins must be restored");
    }

    /// A partial `[theme]` table must fill the rest from `ThemeConfig`'s own
    /// Default, not from a derived all-zero one.
    #[test]
    fn partial_theme_table_keeps_the_shipped_accent() {
        let src = r#"
[[servers]]
name = "Only"
host = "10.0.0.1"
port = 6600

[theme]
dark_mode = false
"#;
        let config: AppConfig = toml::from_str(src).expect("partial theme must parse");
        assert!(!config.theme.dark_mode);
        assert_eq!(config.theme.accent_color, ThemeConfig::default().accent_color);
    }

    fn scratch_path(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("winrmpc-cfg-{}-{tag}-{n}", std::process::id()))
            .join("config")
            .join("config.toml")
    }

    /// First launch must leave a file behind. It used to be written lazily by
    /// whatever action next called `save()`, so a new user had neither a file
    /// nor a folder — and `art_cache_size_mb`, `theme` and the per-server
    /// Snapcast fields had no other way in.
    #[test]
    fn load_creates_the_config_file_when_missing() {
        let path = scratch_path("missing");
        assert!(!path.exists());

        let config = AppConfig::load_from(Some(path.clone()));

        assert!(path.exists(), "first launch must write a config file");
        assert!(!config.load_failed);
        let written: AppConfig =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).expect("valid TOML");
        assert_eq!(written.servers.len(), 1);
        assert_eq!(written.default_server.as_deref(), Some("Default"));
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// A file that exists but doesn't parse must survive untouched. Defaults
    /// are used for the session, and `save` is disarmed — otherwise one typo
    /// is answered by silently replacing every server, station and saved
    /// partition with defaults.
    #[test]
    fn unparseable_config_is_never_overwritten() {
        let path = scratch_path("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let broken = "this is not = valid toml [[[";
        std::fs::write(&path, broken).unwrap();

        let config = AppConfig::load_from(Some(path.clone()));

        assert!(config.load_failed, "must remember that the file was bad");
        assert_eq!(config.servers.len(), 1, "runs on defaults meanwhile");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);

        // And a later settings change must not clobber it either.
        config.save_to(Some(&path)).expect("save is a no-op, not an error");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// A legacy single-server file gets its `[[servers]]` entry written back.
    #[test]
    fn legacy_file_is_migrated_and_saved_once() {
        let path = scratch_path("legacy");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "mpd_host = \"10.0.0.9\"\nmpd_port = 6601\n").unwrap();

        let config = AppConfig::load_from(Some(path.clone()));

        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].host, "10.0.0.9");
        assert_eq!(config.servers[0].port, 6601);
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("[[servers]]"), "migration must persist");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// Round-tripping through `toml::to_string` (what `save()` does) must
    /// produce something `from_str` accepts.
    #[test]
    fn config_round_trips_through_toml() {
        let original = AppConfig::default();
        let text = toml::to_string(&original).expect("serialize");
        let parsed: AppConfig = toml::from_str(&text).expect("re-parse what save() writes");
        assert_eq!(parsed.servers.len(), original.servers.len());
        assert_eq!(parsed.default_server, original.default_server);
        assert_eq!(parsed.mpd_port, original.mpd_port);
    }
}
