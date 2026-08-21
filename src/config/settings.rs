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
    /// Z-A instead of A-Z for every name-sorted library list (Artists,
    /// Albums, Genres, Playlists, and the album lists on Artist and Genre
    /// detail). One flag for all of them, same reasoning as
    /// `album_grid_view`: independent per-list sort memories would feel
    /// arbitrary rather than helpful. The recency lists — Recently Added and
    /// Recently Played — are not name-sorted and ignore it entirely.
    #[serde(default)]
    pub sort_desc: bool,

    // Multi-server
    /// Window geometry, restored at launch. See [`WindowConfig`].
    #[serde(default)]
    pub window: WindowConfig,

    #[serde(default)]
    pub servers: Vec<MpdServer>,
    #[serde(default)]
    pub default_server: Option<String>,
}

/// `accent_color` used to live here as a hex string. It was never read, and a
/// custom accent that stays legible on *both* a near-black and a near-white
/// background is a real design constraint for a feature nobody asked for — so
/// it was removed rather than implemented. Dropping a field is
/// backward-compatible: existing `config.toml`s carrying it still parse, and
/// the value is simply ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub dark_mode: bool,
}

/// Written out by hand rather than derived: the derived `Default` would be
/// `false`, and the app has shipped dark since 0.4.0. Every install that
/// predates the light palette must keep looking the way it did.
impl Default for ThemeConfig {
    fn default() -> Self {
        Self { dark_mode: true }
    }
}

/// Window geometry remembered between launches.
///
/// **Position is deliberately absent.** iced 0.13 exposes no way to enumerate
/// the monitors that exist at restore time, so a saved position cannot be
/// validated — and the failure is unrecoverable from inside the app: the
/// window reopens on a display that is no longer attached, is invisible, and
/// the only fix is hand-editing this file. Size is the actual irritation;
/// position is the part that can strand you.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WindowConfig {
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            maximized: false,
        }
    }
}

impl WindowConfig {
    /// The minimum the window may be restored to.
    ///
    /// Matches `main.rs`'s `min_size`. A saved size below it is **clamped, not
    /// honoured** — iced would let a smaller value through, and below roughly
    /// this Now Playing stops fitting (iced widgets don't clip their parent,
    /// so too-small overlaps rather than degrading).
    pub const MIN_W: f32 = 1000.0;
    pub const MIN_H: f32 = 700.0;

    /// The size to actually open at: the saved one, clamped, and sanity-checked
    /// against nonsense (a zero or NaN from a hand-edited file).
    pub fn restored_size(&self) -> (f32, f32) {
        let w = if self.width.is_finite() { self.width } else { 1200.0 };
        let h = if self.height.is_finite() { self.height } else { 800.0 };
        (w.max(Self::MIN_W), h.max(Self::MIN_H))
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
            sort_desc: false,
            window: WindowConfig::default(),
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

    /// Environment override for the config directory. Set it to run a second
    /// profile against a different server, or to reproduce someone's config
    /// without touching your own.
    pub const CONFIG_DIR_ENV: &'static str = "WINRMPC_CONFIG_DIR";
    /// Environment override for the cache directory. See [`Self::CONFIG_DIR_ENV`].
    pub const CACHE_DIR_ENV: &'static str = "WINRMPC_CACHE_DIR";

    fn env_dir(var: &str) -> Option<PathBuf> {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Where `config.toml` lives.
    ///
    /// `None` means home resolution failed (no `$HOME` on Linux, an unusual
    /// service or sandbox context). That is not a theoretical case and it used
    /// to be **completely silent**: `load_from(None)` returns the defaults
    /// before attempting any write and `save_to(None)` is a no-op, so every
    /// setting the user changed appeared to work and was gone at restart, with
    /// no log line anywhere. Hence the ERROR.
    pub fn config_dir() -> Option<PathBuf> {
        if let Some(dir) = Self::env_dir(Self::CONFIG_DIR_ENV) {
            return Some(dir);
        }
        match ProjectDirs::from("com", "winrmpc", "winrmpc") {
            Some(p) => Some(p.config_dir().to_path_buf()),
            None => {
                tracing::error!(
                    "could not resolve a config directory for this user — \
                     settings will NOT be saved this session. Set {} to a \
                     writable path to work around it.",
                    Self::CONFIG_DIR_ENV
                );
                None
            }
        }
    }

    /// Where `winrmpc.redb` lives. `None` has the same cause as
    /// [`Self::config_dir`]; the caller falls back to a relative path, which
    /// from a desktop launch is often `/` and fails.
    pub fn cache_dir() -> Option<PathBuf> {
        if let Some(dir) = Self::env_dir(Self::CACHE_DIR_ENV) {
            return Some(dir);
        }
        match ProjectDirs::from("com", "winrmpc", "winrmpc") {
            Some(p) => Some(p.cache_dir().to_path_buf()),
            None => {
                tracing::error!(
                    "could not resolve a cache directory for this user — \
                     album art, lyrics and bios will be re-fetched every \
                     launch. Set {} to a writable path to work around it.",
                    Self::CACHE_DIR_ENV
                );
                None
            }
        }
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

    /// `save()` for the call sites that can't do anything useful with the
    /// error — which is all of them, since they're UI handlers.
    ///
    /// They all used to write `config.save().ok()`, which is defensible for a
    /// frequent auto-save and useless to a user whose config directory is
    /// read-only: every setting appears to stick and none of it survives a
    /// restart, with nothing in the log. Every one of these is triggered by a
    /// deliberate user action rather than a poll, so logging each failure is
    /// not a spam risk.
    pub fn save_and_log(&self, what: &str) {
        if let Err(e) = self.save() {
            tracing::error!(
                setting = what,
                error = %e,
                "could not save settings to {}; this change will be lost at restart",
                Self::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<no config path>".into()),
            );
        }
    }

    /// The resolved storage locations, for the Settings view and the startup
    /// log line. `None` means persistence is off for this session.
    pub fn storage_paths() -> (Option<PathBuf>, Option<PathBuf>) {
        (Self::config_path(), Self::cache_dir())
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
        assert_eq!(config.mpd_host, d.mpd_host);
        assert_eq!(config.mpd_port, d.mpd_port);
        assert!(!config.radio_stations.is_empty(), "built-ins must be restored");
    }

    /// A `[theme]` table carrying the removed `accent_color` must still
    /// parse — dropping a field has to stay backward-compatible, or every
    /// config written before 0.4.3 becomes unreadable and (per `load_from`'s
    /// rules) the app runs on defaults without saving.
    #[test]
    fn a_theme_table_with_the_removed_accent_field_still_parses() {
        // r##: the value contains `"#`, which would close an `r#"` literal.
        let src = r##"
[[servers]]
name = "Only"
host = "10.0.0.1"
port = 6600

[theme]
dark_mode = false
accent_color = "#ff0000"
"##;
        let config: AppConfig = toml::from_str(src).expect("partial theme must parse");
        assert!(!config.theme.dark_mode);
    }

    /// A `[theme]` table with *no* fields falls back to `ThemeConfig`'s own
    /// Default (dark), not a derived all-false one.
    #[test]
    fn an_empty_theme_table_keeps_the_shipped_default() {
        let src = r#"
[[servers]]
name = "Only"
host = "10.0.0.1"
port = 6600

[theme]
"#;
        let config: AppConfig = toml::from_str(src).expect("empty theme must parse");
        assert!(config.theme.dark_mode, "the app has shipped dark since 0.4.0");
    }

    #[test]
    fn a_restored_size_below_the_minimum_is_clamped() {
        // iced would honour a smaller size; below roughly this, Now Playing
        // stops fitting and — since iced widgets don't clip their parent —
        // overlaps rather than degrading.
        let tiny = WindowConfig { width: 200.0, height: 100.0, maximized: false };
        assert_eq!(
            tiny.restored_size(),
            (WindowConfig::MIN_W, WindowConfig::MIN_H)
        );
    }

    #[test]
    fn a_saved_size_is_restored_as_is_when_it_is_big_enough() {
        let saved = WindowConfig { width: 1600.0, height: 980.0, maximized: false };
        assert_eq!(saved.restored_size(), (1600.0, 980.0));
    }

    #[test]
    fn nonsense_geometry_from_a_hand_edited_file_falls_back() {
        let bad = WindowConfig { width: f32::NAN, height: 0.0, maximized: false };
        let (w, h) = bad.restored_size();
        assert!(w.is_finite() && h.is_finite());
        assert!(w >= WindowConfig::MIN_W && h >= WindowConfig::MIN_H);
    }

    /// The window table is `#[serde(default)]` like everything else, so a
    /// config written before 0.4.3 keeps parsing and opens at the old size.
    #[test]
    fn a_config_without_a_window_table_uses_the_shipped_size() {
        let src = r#"
[[servers]]
name = "Only"
host = "10.0.0.1"
port = 6600
"#;
        let config: AppConfig = toml::from_str(src).expect("must parse");
        assert_eq!(config.window, WindowConfig::default());
        assert_eq!(config.window.restored_size(), (1200.0, 800.0));
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

    /// The env overrides exist so a portable install, a second profile, or a
    /// reproduction of someone else's config never has to touch the real one.
    ///
    /// These mutate process-wide state, so they run as **one** test rather
    /// than several — Rust runs tests in threads within a process, and two
    /// tests setting the same var would race.
    #[test]
    fn env_overrides_take_precedence_over_the_platform_directories() {
        // SAFETY: single-threaded within this test; see the doc comment above
        // for why these aren't split into separate tests.
        let platform_config = AppConfig::config_dir();
        let platform_cache = AppConfig::cache_dir();

        std::env::set_var(AppConfig::CONFIG_DIR_ENV, "/tmp/winrmpc-test-cfg");
        std::env::set_var(AppConfig::CACHE_DIR_ENV, "/tmp/winrmpc-test-cache");
        assert_eq!(
            AppConfig::config_dir(),
            Some(PathBuf::from("/tmp/winrmpc-test-cfg"))
        );
        assert_eq!(
            AppConfig::cache_dir(),
            Some(PathBuf::from("/tmp/winrmpc-test-cache"))
        );
        assert_eq!(
            AppConfig::config_path(),
            Some(PathBuf::from("/tmp/winrmpc-test-cfg/config.toml")),
            "config_path must be built on top of the overridden dir"
        );

        // The override has to redirect *writes*, not just path lookups —
        // otherwise `save()` still lands on the real user config, which is
        // the accident this whole guard exists to prevent.
        let scratch = std::env::temp_dir().join(format!(
            "winrmpc-envtest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::env::set_var(AppConfig::CONFIG_DIR_ENV, &scratch);
        AppConfig::default().save_and_log("env override test");
        assert!(
            scratch.join("config.toml").exists(),
            "save() must follow {}",
            AppConfig::CONFIG_DIR_ENV
        );
        std::fs::remove_dir_all(&scratch).ok();

        // An empty value is treated as unset, not as "the current directory" —
        // an exported-but-empty variable is a common shell accident and would
        // otherwise silently relocate the user's settings to wherever the app
        // happened to be launched from.
        std::env::set_var(AppConfig::CONFIG_DIR_ENV, "");
        std::env::set_var(AppConfig::CACHE_DIR_ENV, "");
        assert_eq!(AppConfig::config_dir(), platform_config);
        assert_eq!(AppConfig::cache_dir(), platform_cache);

        std::env::remove_var(AppConfig::CONFIG_DIR_ENV);
        std::env::remove_var(AppConfig::CACHE_DIR_ENV);
        assert_eq!(AppConfig::config_dir(), platform_config);
        assert_eq!(AppConfig::cache_dir(), platform_cache);
    }

    /// **Never call `save()`/`save_and_log()` in a test without an env
    /// override in place.** They resolve the *real* user config path, so an
    /// unguarded call overwrites the developer's own servers and radio
    /// stations with defaults. That is why every other test here goes through
    /// the path-injectable `save_to`/`load_from` cores.
    #[test]
    fn save_to_reports_an_unwritable_path_and_the_wrapper_swallows_it() {
        // Put a *file* where the config directory would go, then try to save
        // beneath it. `create_dir_all` refuses to treat an existing regular
        // file as a directory on every platform, so this fails the same way
        // everywhere.
        //
        // The first version of this test used `/proc/...`, which is
        // unwritable on Linux and an ordinary creatable path on Windows —
        // there the save succeeded and the assertion blew up. A platform
        // assumption smuggled into a test, in the very batch of work that
        // exists because of platform assumptions smuggled into code.
        let blocker = std::env::temp_dir().join(format!(
            "winrmpc-unwritable-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&blocker, b"not a directory").expect("scratch file");

        let config = AppConfig::default();
        let result = config.save_to(Some(&blocker.join("config.toml")));

        std::fs::remove_file(&blocker).ok();
        result.expect_err("saving beneath a regular file should fail");
    }
}
