//! Main iced Application: state, update logic, view composition, subscriptions.

use crate::art::ArtCache;
use crate::config::AppConfig;
use crate::mpd::MpdClient;
use crate::store::Store;
use crate::mpd::types::{push_recent, *};
use crate::ui::message::{ArtOutcome, Message, View};
use crate::ui::theme::colors;
use crate::ui::theme::AppColors;
use crate::ui::widgets::icon;
use crate::ui::views;
use crate::ui::widgets;
use iced::widget::{column, container, image::Handle as ImageHandle, row, scrollable};
use iced::{Element, Length, Subscription, Task, Theme};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

/// One toast: a rounded card, error-coloured or not, dismissible by click.
fn toast_view(toast: &Toast) -> Element<'_, Message> {
    let (border, fg) = if toast.is_error {
        (AppColors::error(), AppColors::error())
    } else {
        (AppColors::border(), AppColors::text_primary())
    };
    iced::widget::button(
        container(iced::widget::text(toast.text.clone()).size(12).color(fg))
            .padding([8, 14])
            .max_width(420)
            .style(move |_t: &iced::Theme| container::Style {
                background: Some(AppColors::bg_tertiary().into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: border,
                },
                ..Default::default()
            }),
    )
    .on_press(Message::DismissToast)
    .padding(0)
    .style(|_t: &iced::Theme, _s| iced::widget::button::Style {
        background: None,
        text_color: AppColors::text_primary(),
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
    })
    .into()
}

/// How long a toast stays up before it expires on its own.
const TOAST_INFO_SECS: u64 = 4;
/// Errors linger longer than info — they are the ones worth reading.
const TOAST_ERROR_SECS: u64 = 8;
/// More than this on screen and they'd cover the content they're reporting on.
const MAX_TOASTS: usize = 3;

/// A transient message shown over the main content.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    /// Errors are styled and timed differently from progress notes — the field
    /// this replaced was named `last_error` but also carried "Database update
    /// started", so the two were indistinguishable.
    pub is_error: bool,
    shown_at: std::time::Instant,
}

impl Toast {
    fn expired(&self) -> bool {
        let ttl = if self.is_error {
            TOAST_ERROR_SECS
        } else {
            TOAST_INFO_SECS
        };
        self.shown_at.elapsed().as_secs() >= ttl
    }
}

pub struct App {
    // MPD
    client: MpdClient,
    config: AppConfig,
    connected: bool,

    // State
    status: Status,
    current_song: Option<Song>,
    queue: Vec<Song>,
    outputs: Vec<Output>,
    partitions: Vec<Partition>,

    // Navigation
    current_view: View,
    view_history: Vec<View>,

    // Library
    artists: Vec<String>,
    albums: Vec<AlbumGroup>,
    genres: Vec<String>,
    artist_albums: HashMap<String, Vec<AlbumGroup>>,
    genre_albums: HashMap<String, Vec<AlbumGroup>>,
    album_songs: HashMap<String, Vec<Song>>,
    selected_artist: Option<String>,
    selected_album: Option<String>,

    // Browser
    browser_path: String,
    browser_entries: Vec<crate::mpd::DirectoryEntry>,

    // Search
    search_query: String,
    search_results: Vec<Song>,

    // Playlists
    playlists: Vec<PlaylistInfo>,
    playlist_songs: HashMap<String, Vec<Song>>,
    selected_playlist: Option<String>,
    /// Ephemeral client-side guess at "the queue was loaded from this stored
    /// playlist" — MPD has no native concept of this. Set when a playlist is
    /// loaded/played; cleared by any queue mutation that isn't a playlist
    /// load (see the Message handlers below). Not persisted.
    playing_from_playlist: Option<String>,
    new_playlist_name: String,
    playlist_renaming: Option<String>,
    playlist_rename_input: String,
    /// URIs staged for the shared "Add to Playlist" picker; `Some` while it's open.
    add_to_playlist_uris: Option<Vec<String>>,

    // Cache store (album art + lyrics, redb-backed)
    store: Store,

    // Album Art
    art_cache: ArtCache,
    mb_client: crate::art::MusicBrainzClient,
    art_handles: HashMap<String, ImageHandle>,
    /// Bounds peak concurrent art fetches (MPD binary reads + MusicBrainz/CAA
    /// HTTP) to 4, matching mikMPD's `ArtFetchGate` — without this, opening
    /// an artist with many uncached albums fires one fetch task per album,
    /// unboundedly (see docs/plans/art-wikipedia-fetch-order-and-caching.md §3).
    art_fetch_gate: Arc<tokio::sync::Semaphore>,
    /// Album covers still to fetch, as `(artist, base, variant-to-look-up)`,
    /// in the order the albums appear on screen. Browsing a list enqueues
    /// *every* album it shows; `drain_art_queue` keeps only
    /// `ART_FETCH_CONCURRENCY` fetches running and starts the next as each
    /// finishes, so a large library fills in progressively in the background
    /// instead of stopping dead at a fixed cap.
    art_queue: VecDeque<(String, String, String)>,
    /// Keys queued or in flight in the **local** stage. Guards against
    /// enqueuing the same album twice (re-entering a view, an album showing
    /// up in two lists).
    art_pending: HashSet<String>,
    art_inflight: usize,
    /// Second stage: albums the local stage found no art for, awaiting a
    /// MusicBrainz lookup. Drained only once `art_queue` is empty and only
    /// one at a time, because every entry costs 1.1–2.2s of globally
    /// serialized throttle time (`MB_MIN_INTERVAL`, plus a possible second
    /// search hop). Keys, not triples — `art_key_for` already carries
    /// `artist\x1fbase`, which is exactly what the lookup needs.
    mb_queue: VecDeque<String>,
    mb_pending: HashSet<String>,
    mb_inflight: usize,
    /// Keys a full fetch resolved as "no cover anywhere". `art_handles` only
    /// records hits, so without this every visit to a list would re-queue —
    /// and re-fetch — every album that has no art.
    art_missing: HashSet<String>,

    // Wikipedia bios. In-memory tri-state, mirroring `lyrics`: absent key =
    // never fetched this session, `Some(None)` = fetched, confirmed no bio,
    // `Some(Some(text))` = have a bio. Backed by the redb `bios` table.
    artist_bios: HashMap<String, Option<String>>,
    album_bios: HashMap<String, Option<String>>,
    show_artist_bio: bool,
    show_album_bio: bool,

    // Radio UI
    radio_add_name: String,
    radio_add_url: String,

    // CD
    cd_tracks: Vec<(String, Option<f64>)>,
    cd_probing: bool,

    // Partitions UI
    new_partition_name: String,

    // Snapcast — view-scoped connection (lazily created/connected on first
    // View::Snapcast enter, kept alive across subsequent visits rather than
    // torn down on every navigation-away, since Snapcast may be absent/down
    // independently of MPD and reconnecting on every visit isn't free).
    snapcast_client: Option<crate::snapcast::SnapcastClient>,
    snapcast_groups: Vec<crate::snapcast::SnapGroup>,
    snapcast_streams: Vec<crate::snapcast::SnapStream>,
    snapcast_error: Option<String>,
    /// Whether disconnected Snapcast clients are listed. Off by default —
    /// a Snapcast server keeps a stale entry for every device that ever
    /// connected, so most of the list is usually dead weight.
    snapcast_show_inactive: bool,


    // Settings UI
    /// Armed state for the cache purge — a purge is cheap to trigger and
    /// expensive to undo (every cover re-fetched, MusicBrainz re-crawled),
    /// so it takes two presses.
    confirm_clear_caches: bool,
    /// Bytes of cached art on disk, refreshed on entering Settings.
    cache_size_bytes: Option<u64>,
    settings_host: String,
    settings_port: String,
    settings_password: String,
    settings_cd_device: String,
    settings_server_name: String,
    active_server: String,
    settings_renaming: Option<String>,
    settings_rename_input: String,
    /// Server whose connection details are open for editing, with the
    /// in-progress field values. Separate from the "Add server" inputs so
    /// a half-typed edit can't leak into a half-typed addition.
    settings_editing: Option<String>,
    settings_edit_host: String,
    settings_edit_port: String,
    settings_edit_password: String,
    settings_edit_snap_host: String,
    settings_edit_snap_port: String,
    /// Art cache limit as typed, committed to `art_cache_size_mb` on Save.
    settings_cache_size: String,

    // Recently played albums (most recent first, capped at 8)
    recent_albums: Vec<RecentAlbum>,

    // Recently Added (library) — songs modified in the last 30 days,
    // grouped artist-aware/disc-collapsed like the main Albums list,
    // newest-modified-first.
    recently_added_albums: Vec<AlbumGroup>,

    // Recently Played history — per-server, 30 days / 100 entries, distinct
    // from `recent_albums` above. See PlayRecorder.
    recently_played: Vec<RecentlyPlayedEntry>,
    play_recorder: PlayRecorder,
    recently_played_show_albums: bool,

    // Log
    log_entries: Vec<crate::logger::LogEntry>,
    log_show_mpd_only: bool,

    // Server statistics
    stats: Option<Stats>,

    // Replay gain mode ("off"/"track"/"album"/"auto"); fetched once on connect
    replay_gain_mode: Option<String>,

    // Lyrics
    lyrics_client: crate::lyrics::LyricsClient,
    lyrics: HashMap<String, Option<crate::lyrics::Lyrics>>,
    show_lyrics: bool,
    /// Whether the synced-lyrics pane follows the song (true) or scrolls
    /// freely (false). Session-local and not persisted, matching
    /// `show_lyrics` — both are "how I want to read *this* track" rather than
    /// a durable preference.
    lyrics_follow: bool,
    lyrics_scroll_id: scrollable::Id,
    queue_scroll_id: scrollable::Id,

    // Errors
    last_error: Option<String>,
    /// Transient messages shown over the content, newest last.
    ///
    /// `last_error` is rendered *only* by `settings_view`, so before this
    /// existed an error was invisible unless the user happened to be standing
    /// on the Settings screen when it happened. A queue rather than a single
    /// slot: two failures in quick succession (a bulk enqueue hitting several
    /// bad URIs) must not have the second silently replace the first.
    toasts: std::collections::VecDeque<Toast>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut config = AppConfig::load();
        config.ensure_builtin_stations();

        let active_server = config
            .default_server
            .clone()
            .or_else(|| config.servers.first().map(|s| s.name.clone()))
            .unwrap_or_else(|| "Default".into());
        let client = MpdClient::new(&config.server_addr(&active_server));
        // Before anything is drawn: the palette is a process global, so it has
        // to reflect the saved preference before the first frame rather than
        // after the first toggle.
        colors::set_dark_mode(config.theme.dark_mode);

        // Log both resolved paths at startup so they land in the in-app Log
        // view. This is the zero-UI answer to "where does this thing keep its
        // settings" — on macOS the directory is `~/Library/Application
        // Support/com.winrmpc.winrmpc/`, which Finder hides by default and
        // whose name doesn't contain "winrmpc" in any form anyone would search
        // for. See also Settings → Storage.
        match AppConfig::config_path() {
            Some(p) => tracing::info!("Config file: {}", p.display()),
            None => tracing::error!("No config file: settings will not be saved this session"),
        }
        let cache_dir = match AppConfig::cache_dir() {
            Some(dir) => {
                tracing::info!("Cache database: {}", dir.join("winrmpc.redb").display());
                dir
            }
            None => {
                // Relative to the *working directory*, which from a Finder or
                // desktop launch is very often `/` — where the create fails
                // and `Store::open` degrades to memory. Logged as ERROR by
                // `cache_dir()` already; this records where we actually tried.
                let fallback = std::path::PathBuf::from("./cache");
                tracing::warn!(
                    "falling back to {} for the cache, relative to the working directory",
                    fallback.display()
                );
                fallback
            }
        };
        let store = Store::open(&cache_dir);
        let initial_recently_played = store.recently_played_get(&active_server);

        let app = Self {
            client,
            connected: false,
            config: config.clone(),

            status: Status::default(),
            current_song: None,
            queue: Vec::new(),
            outputs: Vec::new(),
            partitions: Vec::new(),

            current_view: View::NowPlaying,
            view_history: Vec::new(),

            artists: Vec::new(),
            albums: Vec::new(),
            genres: Vec::new(),
            artist_albums: HashMap::new(),
            genre_albums: HashMap::new(),
            album_songs: HashMap::new(),
            selected_artist: None,
            selected_album: None,

            browser_path: String::new(),
            browser_entries: Vec::new(),

            search_query: String::new(),
            search_results: Vec::new(),

            playlists: Vec::new(),
            playlist_songs: HashMap::new(),
            selected_playlist: None,
            playing_from_playlist: None,
            new_playlist_name: String::new(),
            playlist_renaming: None,
            playlist_rename_input: String::new(),
            add_to_playlist_uris: None,

            store: store.clone(),
            mb_client: crate::art::MusicBrainzClient::new(store.clone()),
            art_cache: ArtCache::new(store, config.art_cache_size_mb),
            art_handles: HashMap::new(),
            art_fetch_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            art_queue: VecDeque::new(),
            art_pending: HashSet::new(),
            art_inflight: 0,
            mb_queue: VecDeque::new(),
            mb_pending: HashSet::new(),
            mb_inflight: 0,
            art_missing: HashSet::new(),

            artist_bios: HashMap::new(),
            album_bios: HashMap::new(),
            show_artist_bio: false,
            show_album_bio: false,

            radio_add_name: String::new(),
            radio_add_url: String::new(),

            cd_tracks: Vec::new(),
            cd_probing: false,

            new_partition_name: String::new(),

            snapcast_client: None,
            snapcast_groups: Vec::new(),
            snapcast_streams: Vec::new(),
            snapcast_error: None,
            snapcast_show_inactive: false,


            confirm_clear_caches: false,
            cache_size_bytes: None,
            settings_host: String::new(),
            settings_port: String::new(),
            settings_password: String::new(),
            settings_cd_device: config.cd_device.clone().unwrap_or_default(),
            settings_server_name: String::new(),
            active_server: active_server.clone(),
            settings_renaming: None,
            settings_editing: None,
            settings_edit_host: String::new(),
            settings_edit_port: String::new(),
            settings_edit_password: String::new(),
            settings_edit_snap_host: String::new(),
            settings_edit_snap_port: String::new(),
            settings_cache_size: config.art_cache_size_mb.to_string(),
            settings_rename_input: String::new(),

            recent_albums: config.recent_albums.clone(),

            recently_added_albums: Vec::new(),
            recently_played: initial_recently_played,
            play_recorder: PlayRecorder::new(),
            recently_played_show_albums: true,

            log_entries: Vec::new(),
            log_show_mpd_only: true,

            stats: None,
            replay_gain_mode: None,

            lyrics_client: crate::lyrics::LyricsClient::new(),
            lyrics: HashMap::new(),
            show_lyrics: true,
            lyrics_follow: true,
            lyrics_scroll_id: scrollable::Id::unique(),
            queue_scroll_id: scrollable::Id::unique(),

            last_error: None,
            toasts: std::collections::VecDeque::new(),
        };

        (app, Task::perform(async {}, |_| Message::Connect))
    }

    /// The theme iced's **own** widgets style themselves from — `pick_list`,
    /// `slider`, `text_input`, `scrollable`, default buttons, the menu popup.
    ///
    /// Swapping only `AppColors` would give a light app with dark dropdowns
    /// and dark text inputs, because those don't go through `AppColors` at
    /// all. Both halves are built from the same `Palette` so they can't drift.
    ///
    /// Cached: this is called on every redraw, and `Theme::custom` allocates a
    /// `String` and an `Arc` and derives a full extended palette each time.
    pub fn theme(&self) -> Theme {
        use std::sync::OnceLock;
        static DARK_THEME: OnceLock<Theme> = OnceLock::new();
        static LIGHT_THEME: OnceLock<Theme> = OnceLock::new();

        fn build(name: &str, p: &colors::Palette) -> Theme {
            Theme::custom(
                name.to_string(),
                iced::theme::Palette {
                    background: p.bg_primary,
                    text: p.text_primary,
                    primary: p.accent,
                    success: p.success,
                    danger: p.error,
                },
            )
        }

        if colors::is_dark_mode() {
            DARK_THEME.get_or_init(|| build("winrmpc dark", &colors::DARK)).clone()
        } else {
            LIGHT_THEME.get_or_init(|| build("winrmpc light", &colors::LIGHT)).clone()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![if self.connected {
            iced::time::every(Duration::from_millis(500)).map(|_| Message::Tick)
        } else {
            iced::time::every(Duration::from_secs(3)).map(|_| Message::ConnectionTick)
        }];

        // Snapcast: only poll while its view is open — a background poll
        // for a subsystem the user isn't looking at is pure waste.
        if self.current_view == View::Snapcast {
            subs.push(iced::time::every(Duration::from_secs(2)).map(|_| Message::SnapcastPollTick));
        }

        Subscription::batch(subs)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // =================================================================
            // Connection
            // =================================================================
            Message::Connect => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.connect().await.map_err(|e| e.to_string())
                    },
                    Message::Connected,
                )
            }
            Message::Connected(result) => {
                match result {
                    Ok(()) => {
                        // Do NOT set connected=true yet. Keep the ConnectionTick
                        // subscription slow so no Tick fires before startup completes.
                        self.last_error = None;
                        tracing::info!("Connected to MPD");
                        let client = self.client.clone();
                        // Per-server partition, falling back to legacy global field.
                        let partition = self.config.server(&self.active_server)
                            .and_then(|s| s.default_partition.clone())
                            .or_else(|| self.config.default_partition.clone());
                        return Task::perform(
                            async move {
                                if let Some(p) = &partition {
                                    if let Err(e) = client.switch_partition(p).await {
                                        tracing::warn!("Could not restore partition '{p}': {e}");
                                    }
                                }
                            },
                            |_| Message::RefreshAll,
                        );
                    }
                    Err(e) => {
                        self.connected = false;
                        self.toast_error(e);
                    }
                }
                Task::none()
            }
            Message::RefreshAll => {
                self.connected = true;
                let art_tasks = self.fetch_recent_art();
                Task::batch([self.fetch_all(), art_tasks])
            }
            Message::ConnectionTick => {
                if !self.connected {
                    let client = self.client.clone();
                    Task::perform(
                        async move {
                            // Skip reconnect if the TCP connection is already up
                            // (e.g. startup partition switch is still in progress).
                            //
                            // This is only safe because `MpdClient` now drops
                            // the connection on any framing/IO error
                            // (`MpdError::is_connection_fatal`). Without that,
                            // a desynced socket stayed `Some` forever, this
                            // check-circuited every reconnect, and the app
                            // logged "Connected to MPD" every 3s while every
                            // command failed — recoverable only by restarting.
                            if client.is_connected().await {
                                return Ok(());
                            }
                            client.connect().await.map_err(|e| e.to_string())
                        },
                        Message::Connected,
                    )
                } else {
                    Task::none()
                }
            }
            Message::Disconnected => {
                self.connected = false;
                Task::none()
            }

            // =================================================================
            // Playback
            // =================================================================
            Message::Play => {
                let client = self.client.clone();
                let status = self.status.state.clone();
                Task::perform(
                    async move {
                        match status {
                            PlayState::Pause => client.resume().await.ok(),
                            PlayState::Stop => client.play().await.ok(),
                            PlayState::Play => client.play().await.ok(),
                        };
                    },
                    |_| Message::Tick,
                )
            }
            Message::Pause => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.pause().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::Stop => self.mpd_cmd(|c| async move { c.stop().await }),
            Message::Next => self.mpd_cmd(|c| async move { c.next().await }),
            Message::Previous => self.mpd_cmd(|c| async move { c.previous().await }),
            Message::SeekTo(pos) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.seek_cur(pos).await.ok();
                    },
                    |_| Message::Noop,
                )
            }
            Message::VolumeChanged(vol) => {
                let client = self.client.clone();
                let v = vol as u32;
                Task::perform(
                    async move {
                        client.set_volume(v).await.ok();
                    },
                    |_| Message::Noop,
                )
            }
            Message::ToggleRepeat => {
                let new_val = !self.status.repeat;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.set_repeat(new_val).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::ToggleRandom => {
                let new_val = !self.status.random;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.set_random(new_val).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::ToggleSingle => {
                let state = match self.status.single {
                    SingleState::Off => "1",
                    SingleState::On => "oneshot",
                    SingleState::Oneshot => "0",
                };
                let client = self.client.clone();
                let s = state.to_string();
                Task::perform(
                    async move {
                        client.set_single(&s).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::ToggleConsume => {
                let state = match self.status.consume {
                    ConsumeState::Off => "1",
                    ConsumeState::On => "oneshot",
                    ConsumeState::Oneshot => "0",
                };
                let client = self.client.clone();
                let s = state.to_string();
                Task::perform(
                    async move {
                        client.set_consume(&s).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::SetCrossfade(secs) => {
                self.mpd_cmd(move |c| async move { c.set_crossfade(secs).await })
            }
            Message::SetReplayGainMode(mode) => {
                self.replay_gain_mode = Some(mode.clone());
                self.mpd_cmd(move |c| async move { c.set_replay_gain_mode(&mode).await })
            }
            Message::ReplayGainModeLoaded(mode) => {
                self.replay_gain_mode = Some(mode);
                Task::none()
            }

            // =================================================================
            // Status Updates
            // =================================================================
            Message::StatusUpdated(status) => {
                self.status = *status;

                // Recently Played recording: tick the recorder on every poll
                // (it self-tracks deltas/file changes) and persist on commit.
                // Skip CD tracks (physical disc, not a "play" worth recording
                // the way a library file or radio stream is). Only reads
                // the two fields the recorder needs rather than cloning the
                // whole Song (~15 Option<String> fields plus a tags
                // HashMap) on every 500ms poll.
                let mut task = Task::none();
                let recorder_input = self.current_song.as_ref().and_then(|s| {
                    (!s.file.starts_with("cdda://")).then(|| (s.file.clone(), s.duration_secs))
                });
                if let Some((file, duration_secs)) = recorder_input {
                    let elapsed = self.status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
                    let is_playing = self.status.state == PlayState::Play;
                    if self.play_recorder.tick(&file, is_playing, elapsed, duration_secs) {
                        if let Some(song) = self.current_song.as_ref() {
                            let now = chrono::Utc::now().timestamp();
                            let entry = RecentlyPlayedEntry {
                                file: song.file.clone(),
                                title: song.display_title().to_string(),
                                artist: song.display_artist().to_string(),
                                album_artist: song.display_album_artist().to_string(),
                                album: song.display_album().to_string(),
                                played_at: now,
                            };
                            self.recently_played.insert(0, entry);
                            prune_recently_played(&mut self.recently_played, now);
                            let store = self.store.clone();
                            let server = self.active_server.clone();
                            let entries = self.recently_played.clone();
                            task = Task::perform(
                                async move {
                                    let _ = tokio::task::spawn_blocking(move || {
                                        store.recently_played_put(&server, &entries)
                                    })
                                    .await;
                                },
                                |_| Message::Noop,
                            );
                        }
                    }
                }
                task
            }
            Message::CurrentSongUpdated(song) => {
                // Track recently played: when the album changes, push the NEW
                // album to the recent list (so "recently played" = what's been
                // playing in this session, most recent first). Skip CD tracks.
                if let Some(ref new_song) = song {
                    let new_album = new_song.display_album();
                    let new_artist = new_song.display_album_artist();
                    let same_album = self.current_song.as_ref()
                        .map(|s| s.display_album() == new_album)
                        .unwrap_or(false);
                    if !same_album
                        && new_album != "Unknown Album"
                        && !new_song.file.starts_with("cdda://")
                    {
                        let entry = RecentAlbum {
                            artist: new_artist.to_string(),
                            album: new_album.to_string(),
                        };
                        push_recent(&mut self.recent_albums, entry);
                        self.config.recent_albums = self.recent_albums.clone();
                        self.config.save_and_log("recent albums");
                    }
                }

                // A new track means new lyrics: send the pane back to the top
                // rather than letting it keep the previous track's offset.
                let track_changed = song.as_ref().map(|s| &s.file)
                    != self.current_song.as_ref().map(|s| &s.file);

                let mut tasks: Vec<Task<Message>> = Vec::new();
                if track_changed {
                    tasks.push(self.reset_lyrics_scroll());
                }
                if let Some(ref s) = song {
                    let art_key = s.art_key();
                    if !self.art_handles.contains_key(&art_key) {
                        tasks.push(self.fetch_art(s.file.clone(), art_key));
                    }
                    tasks.push(self.fetch_lyrics(s));
                }
                self.current_song = song.map(|s| *s);
                if tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(tasks)
                }
            }
            Message::QueueUpdated(q) => {
                self.queue = q;
                Task::none()
            }
            Message::OutputsUpdated(o) => {
                self.outputs = o;
                Task::none()
            }
            Message::PartitionsUpdated(p) => {
                self.partitions = p;
                Task::none()
            }

            // =================================================================
            // Queue
            // =================================================================
            Message::QueuePlay(pos) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.play_pos(pos).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueRemove(id) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.delete_id(id).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueMoveUp(pos) => {
                if pos == 0 {
                    return Task::none();
                }
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.move_pos(pos, pos - 1).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueMoveDown(pos) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.move_pos(pos, pos + 1).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueAddNext(uri) => {
                let client = self.client.clone();
                let insert_at = self.status.song_pos.map(|p| p + 1);
                let is_stopped = self.status.state == PlayState::Stop;
                Task::perform(
                    async move {
                        if let Ok(id) = client.add_id(&uri).await {
                            if let Some(pos) = insert_at {
                                if let Ok(q) = client.queue().await {
                                    // Queue can only have shrunk to empty if
                                    // our own add above raced with something
                                    // clearing it — nothing to move in that
                                    // case, just leave it.
                                    if let Some(end) = (q.len() as u32).checked_sub(1) {
                                        if end != pos {
                                            client.move_pos(end, pos).await.ok();
                                        }
                                    }
                                }
                            } else if is_stopped {
                                // Nothing currently playing — no "next" position
                                // to insert before, so just start this song.
                                client.play_id(id).await.ok();
                            }
                        }
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueClear => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::JumpToCurrent => {
                // Same ratio trick the lyrics pane uses: iced 0.13 exposes no
                // per-item scroll offset, but snapping to index/(len-1) is
                // accurate here in a way it isn't there, because queue rows
                // are a uniform height and lyric lines aren't.
                let Some(pos) = self.status.song_pos else {
                    return Task::none();
                };
                let len = self.queue.len();
                if len < 2 {
                    return Task::none();
                }
                let ratio = pos as f32 / (len - 1) as f32;
                scrollable::snap_to(
                    self.queue_scroll_id.clone(),
                    scrollable::RelativeOffset { x: 0.0, y: ratio },
                )
            }
            Message::QueueShuffle => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.shuffle().await.ok();
                    },
                    |_| Message::Tick,
                )
            } 
            Message::QueueAddUri(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
                        // Start playing if not already
                        if let Ok(status) = client.status().await {
                            if status.state == PlayState::Stop {
                                client.play().await.ok();
                            }
                        }
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueAddAndPlay(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        // Clear queue, add the uri, and play
                        client.clear().await.ok();
                        client.add(&uri).await.ok();
                        client.play().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueAddOnly(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::PlaySong(uri) => {
                // Insert at end of queue and immediately play — non-destructive.
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        if let Ok(id) = client.add_id(&uri).await {
                            client.play_id(id).await.ok();
                        }
                    },
                    |_| Message::Tick,
                )
            }
            Message::PlayAlbum(uris) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        // Don't swallow this: `add_all` is one command list,
                        // which MPD aborts at the first bad URI, and the
                        // queue was just cleared — so a failure here means
                        // playback is about to start on a *partial* album
                        // rather than on nothing at all.
                        if let Err(e) = client.add_all(&uris).await {
                            tracing::warn!(
                                "Play All: queued a partial album ({} tracks requested) — {e}",
                                uris.len()
                            );
                        }
                        client.play().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::QueueAlbum(uris) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        if let Err(e) = client.add_all(&uris).await {
                            tracing::warn!(
                                "Queue All: appended a partial album ({} tracks requested) — {e}",
                                uris.len()
                            );
                        }
                        if let Ok(status) = client.status().await {
                            if status.state == PlayState::Stop {
                                client.play().await.ok();
                            }
                        }
                    },
                    |_| Message::Tick,
                )
            }

            // =================================================================
            // Navigation
            // =================================================================
            Message::NavigateTo(view) => {
                self.view_history.push(self.current_view.clone());
                self.current_view = view.clone();
                self.on_view_enter(view)
            }
            Message::GoBack => {
                if let Some(prev) = self.view_history.pop() {
                    self.current_view = prev;
                }
                Task::none()
            }

            // =================================================================
            // Library
            // =================================================================
            Message::ArtistsLoaded(a) => {
                self.artists = a;
                Task::none()
            }
            Message::AlbumsLoaded(a) => {
                self.albums = a;
                self.queue_album_art()
            }
            Message::GenresLoaded(g) => {
                self.genres = g;
                Task::none()
            }
            Message::ArtistSelected(name) => {
                self.selected_artist = Some(name.clone());
                self.view_history.push(self.current_view.clone());
                self.current_view = View::ArtistDetail(name.clone());
                let client = self.client.clone();

                let albums_task = Task::perform(
                    async move {
                        let mut albums = client
                            .list_tag_filtered("Album", "AlbumArtist", &name)
                            .await
                            .unwrap_or_default();
                        let artist_albums = client
                            .list_tag_filtered("Album", "Artist", &name)
                            .await
                            .unwrap_or_default();
                        for a in artist_albums {
                            if !albums.contains(&a) {
                                albums.push(a);
                            }
                        }
                        albums.sort();
                        // Every entry shares this page's artist, so grouping
                        // here only does multi-disc collapsing (no
                        // cross-artist ambiguity is possible on this page).
                        let pairs: Vec<(String, String)> =
                            albums.into_iter().map(|a| (name.clone(), a)).collect();
                        let groups = group_albums_by_artist(&pairs);
                        (name, groups)
                    },
                    |(name, albums)| Message::ArtistAlbumsLoaded(name, albums),
                );

                let art_name = self.selected_artist.clone().unwrap_or_default();
                let artist_art_task = self.fetch_artist_art(art_name);

                let bio_name = self.selected_artist.clone().unwrap_or_default();
                let bio_task = self.fetch_artist_bio(bio_name);

                self.show_artist_bio = false;
                Task::batch([albums_task, artist_art_task, bio_task])
            }
            Message::AlbumSelected(name, artist) => {
                self.selected_album = Some(name.clone());
                self.view_history.push(self.current_view.clone());
                self.current_view = View::AlbumDetail(name.clone(), artist.clone());
                self.show_album_bio = false;

                let client = self.client.clone();
                let album_name = name.clone();
                let artist_for_songs = artist.clone();
                // Scoped the same way View::AlbumDetail's (name, artist)
                // is, so the render-time lookup in album_songs always
                // agrees with what gets stored here — see
                // docs/plans/review-fixes-correctness.md §2.
                let cache_key = album_scoped_key(artist_for_songs.as_deref(), &album_name);
                let songs_task = Task::perform(
                    async move {
                        let mut songs = match &artist_for_songs {
                            Some(art) => {
                                // Resolve which raw album tags (disc
                                // variants) belong to this artist-scoped
                                // base name, then fetch and concatenate all
                                // of them — a multi-disc album's base name
                                // isn't any single track's literal Album
                                // tag, so a plain find("Album", base) would
                                // find nothing.
                                let all_albums = client
                                    .list_tag_filtered("Album", "AlbumArtist", art)
                                    .await
                                    .unwrap_or_default();
                                let pairs: Vec<(String, String)> = all_albums
                                    .into_iter()
                                    .map(|a| (art.clone(), a))
                                    .collect();
                                let groups = group_albums_by_artist(&pairs);
                                let variants = groups
                                    .into_iter()
                                    .find(|g| {
                                        g.artist.eq_ignore_ascii_case(art) && g.base == album_name
                                    })
                                    .map(|g| g.variants)
                                    .unwrap_or_else(|| vec![album_name.clone()]);

                                let mut all_songs = Vec::new();
                                for variant in &variants {
                                    let mut s = client
                                        .find_album_by_artist(variant, art)
                                        .await
                                        .unwrap_or_default();
                                    all_songs.append(&mut s);
                                }
                                all_songs
                            }
                            // Artist unknown (e.g. from Genre detail) — skip
                            // sibling-variant merging, matching mikMPD's
                            // "unsafe to merge without an artist" rule.
                            None => client.find("Album", &album_name).await.unwrap_or_default(),
                        };
                        songs.sort_by_key(|s| {
                            let track: u32 = s
                                .track
                                .as_deref()
                                .and_then(|t| t.split('/').next())
                                .and_then(|t| t.trim().parse().ok())
                                .unwrap_or(0);
                            (s.effective_disc(), track)
                        });
                        (cache_key, songs)
                    },
                    |(key, songs)| Message::AlbumSongsLoaded(key, songs),
                );

                let bio_task = self.fetch_album_bio(artist, name);

                Task::batch([songs_task, bio_task])
            }
            Message::GenreSelected(name) => {
                self.view_history.push(self.current_view.clone());
                self.current_view = View::GenreDetail(name.clone());
                let client = self.client.clone();
                Task::perform(
                    async move {
                        // Artist-scoped and disc-collapsed, like every other
                        // album listing — so an album opened from a genre gets
                        // the same view as one opened from Albums.
                        let pairs = client
                            .list_albums_by_artist_filtered("Genre", &name)
                            .await
                            .unwrap_or_default();
                        (name, group_albums_by_artist(&pairs))
                    },
                    |(name, albums)| Message::GenreAlbumsLoaded(name, albums),
                )
            }
            Message::GenreAlbumsLoaded(genre, albums) => {
                self.genre_albums.insert(genre, albums);
                Task::none()
            }
            Message::ArtistAlbumsLoaded(artist, albums) => {
                // Through the shared queue rather than one task per album:
                // an artist with a deep discography used to fire every fetch
                // at once, and they all then queued up on the connection
                // behind the `art_fetch_gate` semaphore anyway.
                //
                // `album_art_targets` uses each group's own `artist`, which
                // on this page is the artist whose albums these are, and its
                // `base` (already disc-stripped) with the first disc variant
                // as the tag to look a track up by.
                let targets = Self::album_art_targets(&albums);
                self.artist_albums.insert(artist, albums);
                self.enqueue_album_art(targets)
            }
            Message::AlbumSongsLoaded(album, songs) => {
                if let Some(first) = songs.first() {
                    let key = first.art_key();
                    if !self.art_handles.contains_key(&key) {
                        let task = self.fetch_art(first.file.clone(), key);
                        self.album_songs.insert(album, songs);
                        return task;
                    }
                }
                self.album_songs.insert(album, songs);
                Task::none()
            }

            // =================================================================
            // Recently Added / Recently Played history
            // =================================================================
            Message::RecentlyAddedLoaded(mut songs) => {
                // Newest-modified first; unknown last_modified sinks to the
                // bottom rather than the top.
                songs.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
                let pairs: Vec<(String, String)> = songs
                    .iter()
                    .map(|s| (s.display_album_artist().to_string(), s.display_album().to_string()))
                    .collect();
                self.recently_added_albums = group_albums_by_artist(&pairs);
                self.queue_album_art()
            }
            Message::RecentlyPlayedLoaded(entries) => {
                self.recently_played = entries;
                Task::none()
            }
            Message::ClearRecentlyPlayed => {
                self.recently_played.clear();
                let store = self.store.clone();
                let server = self.active_server.clone();
                Task::perform(
                    async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            store.recently_played_put(&server, &[])
                        })
                        .await;
                    },
                    |_| Message::Noop,
                )
            }
            Message::ToggleAlbumGridView => {
                self.config.album_grid_view = !self.config.album_grid_view;
                self.config.save_and_log("album grid layout");
                // Both layouts show covers (the list just shows them small),
                // so this only matters when the queue was never started for
                // this list — it's a no-op when it already was.
                self.queue_album_art()
            }
            Message::ClearCaches => {
                if !self.confirm_clear_caches {
                    self.confirm_clear_caches = true;
                    return Task::none();
                }
                self.confirm_clear_caches = false;

                // In-memory first, so the UI stops showing what's about to
                // stop existing. The art queues are dropped rather than left
                // running: their in-flight fetches would otherwise re-populate
                // the cache we're clearing, and their completions would
                // decrement counters that no longer mean anything.
                self.art_handles.clear();
                self.art_missing.clear();
                self.art_queue.clear();
                self.art_pending.clear();
                self.art_inflight = 0;
                self.mb_queue.clear();
                self.mb_pending.clear();
                self.mb_inflight = 0;
                self.lyrics.clear();
                self.artist_bios.clear();
                self.album_bios.clear();

                let cache = self.art_cache.clone_inner();
                let store = self.store.clone();
                Task::perform(
                    async move {
                        cache.clear().await; // in-memory layer + art tables
                        let _ = tokio::task::spawn_blocking(move || {
                            store.clear_lookup_caches();
                        })
                        .await;
                        tracing::info!("Cleared art, lyrics, bio and MusicBrainz caches");
                    },
                    |_| Message::CachesCleared,
                )
            }
            Message::CancelClearCaches => {
                self.confirm_clear_caches = false;
                Task::none()
            }
            Message::CachesCleared => {
                self.cache_size_bytes = Some(0);
                // Refill whatever list is open, so the covers come back
                // without needing a navigation.
                Task::batch([self.queue_album_art(), self.fetch_cache_size()])
            }
            Message::OpenStorageFolder(dir) => {
                // Create it first: the paths are shown before anything has
                // been written there, and "Open folder" on a folder that
                // doesn't exist yet is a dead button.
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    tracing::error!(dir, error = %e, "could not create the folder to open");
                    return Task::none();
                }
                if let Err(e) = open::that_detached(&dir) {
                    tracing::error!(dir, error = %e, "could not open the folder");
                }
                Task::none()
            }
            Message::CacheSizeLoaded(bytes) => {
                self.cache_size_bytes = Some(bytes);
                Task::none()
            }
            Message::ToggleRecentlyPlayedMode => {
                self.recently_played_show_albums = !self.recently_played_show_albums;
                Task::none()
            }

            // =================================================================
            // Playlists
            // =================================================================
            Message::PlaylistsLoaded(list) => {
                self.playlists = list;
                Task::none()
            }
            Message::PlaylistSelected(name) => {
                self.selected_playlist = Some(name.clone());
                self.view_history.push(self.current_view.clone());
                self.current_view = View::PlaylistDetail(name.clone());
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let songs = client.list_playlist(&name).await.unwrap_or_default();
                        (name, songs)
                    },
                    |(name, songs)| Message::PlaylistSongsLoaded(name, songs),
                )
            }
            Message::PlaylistSongsLoaded(name, songs) => {
                if let Some(first) = songs.first() {
                    let key = first.art_key();
                    if !self.art_handles.contains_key(&key) {
                        let task = self.fetch_art(first.file.clone(), key);
                        self.playlist_songs.insert(name, songs);
                        return task;
                    }
                }
                self.playlist_songs.insert(name, songs);
                Task::none()
            }
            Message::PlaylistPlay(name) => {
                self.playing_from_playlist = Some(name.clone());
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        client.load_playlist(&name).await.ok();
                        client.play_pos(0).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::PlaylistAppend(name) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.load_playlist(&name).await.ok();
                        if let Ok(status) = client.status().await {
                            if status.state == PlayState::Stop {
                                client.play().await.ok();
                            }
                        }
                    },
                    |_| Message::Tick,
                )
            }
            Message::PlaylistPlayAt(name, pos) => {
                self.playing_from_playlist = Some(name.clone());
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        client.load_playlist(&name).await.ok();
                        client.play_pos(pos).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::PlaylistDelete(name) => {
                self.playlist_songs.remove(&name);
                if self.current_view == View::PlaylistDetail(name.clone()) {
                    self.current_view = View::Playlists;
                }
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.delete_playlist(&name).await.ok();
                        client.list_playlists().await.unwrap_or_default()
                    },
                    Message::PlaylistsLoaded,
                )
            }
            Message::PlaylistRemoveSong(name, pos) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.playlist_delete(&name, pos).await.ok();
                        let songs = client.list_playlist(&name).await.unwrap_or_default();
                        (name, songs)
                    },
                    |(name, songs)| Message::PlaylistSongsLoaded(name, songs),
                )
            }
            Message::PlaylistMoveSongUp(name, pos) => {
                if pos == 0 {
                    return Task::none();
                }
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.playlist_move(&name, pos, pos - 1).await.ok();
                        let songs = client.list_playlist(&name).await.unwrap_or_default();
                        (name, songs)
                    },
                    |(name, songs)| Message::PlaylistSongsLoaded(name, songs),
                )
            }
            Message::PlaylistMoveSongDown(name, pos) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.playlist_move(&name, pos, pos + 1).await.ok();
                        let songs = client.list_playlist(&name).await.unwrap_or_default();
                        (name, songs)
                    },
                    |(name, songs)| Message::PlaylistSongsLoaded(name, songs),
                )
            }
            Message::SaveQueueAsPlaylist => {
                let name_input = self.new_playlist_name.clone();
                match validate_playlist_name(&name_input) {
                    Some(name) if !self.queue.is_empty() => {
                        self.new_playlist_name.clear();
                        let client = self.client.clone();
                        Task::perform(
                            async move {
                                client.save_playlist(&name).await.ok();
                                client.list_playlists().await.unwrap_or_default()
                            },
                            Message::PlaylistsLoaded,
                        )
                    }
                    Some(_) => Task::none(),
                    None => {
                        self.toast_error(
                            "Playlist names must not be empty or contain slashes.",
                        );
                        Task::none()
                    }
                }
            }
            Message::NewPlaylistNameChanged(s) => {
                self.new_playlist_name = s;
                Task::none()
            }
            Message::StartRenamePlaylist(name) => {
                self.playlist_rename_input = name.clone();
                self.playlist_renaming = Some(name);
                Task::none()
            }
            Message::RenamePlaylistInput(s) => {
                self.playlist_rename_input = s;
                Task::none()
            }
            Message::ConfirmRenamePlaylist => {
                let mut task = Task::none();
                if let Some(old_name) = self.playlist_renaming.take() {
                    match validate_playlist_name(&self.playlist_rename_input) {
                        Some(new_name) if new_name != old_name => {
                            let client = self.client.clone();
                            task = Task::perform(
                                async move {
                                    client.rename_playlist(&old_name, &new_name).await.ok();
                                    client.list_playlists().await.unwrap_or_default()
                                },
                                Message::PlaylistsLoaded,
                            );
                        }
                        Some(_) => {}
                        None => {
                            self.toast_error(
                                "Playlist names must not be empty or contain slashes.",
                            );
                        }
                    }
                }
                self.playlist_rename_input.clear();
                task
            }
            Message::CancelRenamePlaylist => {
                self.playlist_renaming = None;
                self.playlist_rename_input.clear();
                Task::none()
            }

            // =================================================================
            // Shared "Add to Playlist" picker
            // =================================================================
            Message::OpenAddToPlaylist(uris) => {
                self.add_to_playlist_uris = Some(uris);
                self.view_history.push(self.current_view.clone());
                self.current_view = View::AddToPlaylist;
                let client = self.client.clone();
                Task::perform(
                    async move { client.list_playlists().await.unwrap_or_default() },
                    Message::PlaylistsLoaded,
                )
            }
            Message::AddToPlaylistConfirm(name) => {
                let task = if let Some(uris) = self.add_to_playlist_uris.take() {
                    let client = self.client.clone();
                    Task::perform(
                        async move {
                            for uri in uris {
                                client.playlist_add(&name, &uri).await.ok();
                            }
                            client.list_playlists().await.unwrap_or_default()
                        },
                        Message::PlaylistsLoaded,
                    )
                } else {
                    Task::none()
                };
                if let Some(prev) = self.view_history.pop() {
                    self.current_view = prev;
                }
                task
            }
            Message::AddToNewPlaylist => {
                let name_input = self.new_playlist_name.clone();
                match validate_playlist_name(&name_input) {
                    Some(name) => {
                        if let Some(uris) = self.add_to_playlist_uris.take() {
                            self.new_playlist_name.clear();
                            if let Some(prev) = self.view_history.pop() {
                                self.current_view = prev;
                            }
                            let client = self.client.clone();
                            Task::perform(
                                async move {
                                    for uri in uris {
                                        client.playlist_add(&name, &uri).await.ok();
                                    }
                                    client.list_playlists().await.unwrap_or_default()
                                },
                                Message::PlaylistsLoaded,
                            )
                        } else {
                            Task::none()
                        }
                    }
                    None => {
                        self.toast_error(
                            "Playlist names must not be empty or contain slashes.",
                        );
                        Task::none()
                    }
                }
            }
            Message::CloseAddToPlaylist => {
                self.add_to_playlist_uris = None;
                if let Some(prev) = self.view_history.pop() {
                    self.current_view = prev;
                }
                Task::none()
            }

            // =================================================================
            // Browser
            // =================================================================
            Message::BrowsePath(path) => {
                self.browser_path = path.clone();
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let entries = client.lsinfo(&path).await.unwrap_or_default();
                        (path, entries)
                    },
                    |(path, entries)| Message::BrowseLoaded(path, entries),
                )
            }
            Message::BrowseLoaded(path, entries) => {
                if path == self.browser_path {
                    self.browser_entries = entries;
                }
                Task::none()
            }
            Message::BrowseAddToQueue(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
                    },
                    |_| Message::Tick,
                )
            }

            // =================================================================
            // Search
            // =================================================================
            Message::SearchQueryChanged(q) => {
                self.search_query = q;
                Task::none()
            }
            Message::SearchSubmit => {
                let query = self.search_query.clone();
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.search("any", &query).await.unwrap_or_default()
                    },
                    Message::SearchResults,
                )
            }
            Message::SearchResults(results) => {
                self.search_results = results;
                Task::none()
            }
            Message::SearchAddToQueue(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
                        if let Ok(status) = client.status().await {
                            if status.state == PlayState::Stop {
                                client.play().await.ok();
                            }
                        }
                    },
                    |_| Message::Tick,
                )
            }

            // =================================================================
            // Album Art
            // =================================================================
            Message::ArtLoaded(key, data) => {
                if let Some(bytes) = data {
                    if let Some(handle) =
                        widgets::art_image::bytes_to_handle(&bytes)
                    {
                        self.art_handles.insert(key, handle);
                    }
                }
                Task::none()
            }
            Message::AlbumArtFetched(key, outcome) => {
                // Which stage reported determines which counter to free.
                // A key is in exactly one of the two pending sets.
                if self.art_pending.remove(&key) {
                    self.art_inflight = self.art_inflight.saturating_sub(1);
                } else if self.mb_pending.remove(&key) {
                    self.mb_inflight = self.mb_inflight.saturating_sub(1);
                }

                match outcome {
                    ArtOutcome::Loaded(bytes) => {
                        if let Some(handle) =
                            widgets::art_image::bytes_to_handle(&bytes)
                        {
                            self.art_handles.insert(key, handle);
                        }
                    }
                    // Nothing locally — hand it to the MusicBrainz stage.
                    ArtOutcome::MpdMiss => {
                        self.mb_pending.insert(key.clone());
                        self.mb_queue.push_back(key);
                    }
                    ArtOutcome::Missing => {
                        self.art_missing.insert(key);
                    }
                }

                self.drain_art_queue()
            }

            // =================================================================
            // Wikipedia Bios
            // =================================================================
            Message::ArtistBioLoaded(name, bio) => {
                // Record both positive and negative results — a confirmed
                // "no bio found" must stick in-memory too, or contains_key
                // guards would refetch every visit.
                self.artist_bios.insert(name, bio);
                Task::none()
            }
            Message::AlbumBioLoaded(name, bio) => {
                self.album_bios.insert(name, bio);
                Task::none()
            }
            Message::ToggleArtistBio => {
                self.show_artist_bio = !self.show_artist_bio;
                Task::none()
            }
            Message::ToggleAlbumBio => {
                self.show_album_bio = !self.show_album_bio;
                Task::none()
            }

            // =================================================================
            // Outputs
            // =================================================================
            Message::ToggleOutput(id) => {
                let c1 = self.client.clone();
                let c2 = self.client.clone();
                let toggle_task = Task::perform(
                    async move { c1.toggle_output(id).await.ok(); },
                    |_| Message::Tick,
                );
                let outputs_task = Task::perform(
                    async move { c2.outputs().await.unwrap_or_default() },
                    Message::OutputsUpdated,
                );
                Task::batch([toggle_task, outputs_task])
            }
            Message::MoveOutput { output_name, target_partition } => {
                let client = self.client.clone();
                let current = self.status.partition
                    .clone()
                    .unwrap_or_else(|| "default".to_string());
                Task::perform(
                    async move {
                        // Switch to target partition
                        client.switch_partition(&target_partition).await.ok();
                        // Move the output into the now-current partition
                        client.move_output(&output_name).await.ok();
                        // Switch back to where we were
                        client.switch_partition(&current).await.ok();
                    },
                    |_| Message::RefreshAll,
                )
            }

            // =================================================================
            // Partitions
            // =================================================================
            Message::SwitchPartition(name) => {
                if let Some(server) = self.config.server_mut(&self.active_server) {
                    server.default_partition = Some(name.clone());
                }
                self.config.default_partition = Some(name.clone());
                self.config.save_and_log("default partition");
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.switch_partition(&name).await.ok();
                    },
                    |_| Message::RefreshAll,
                )
            }
            Message::NewPartition(name) => {
                if !name.is_empty() {
                    self.new_partition_name.clear();
                    let client = self.client.clone();
                    Task::perform(
                        async move {
                            client.new_partition(&name).await.ok();
                        },
                        |_| Message::Tick,
                    )
                } else {
                    Task::none()
                }
            }
            Message::DeletePartition(name) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.delete_partition(&name).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::PartitionNameInput(s) => {
                self.new_partition_name = s;
                Task::none()
            }

            // =================================================================
            // Radio
            // =================================================================
            Message::RadioPlay(url) => {
                // Clear the queue, add the stream URL, and play
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        client.add(&url).await.ok();
                        client.play().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::RadioAddCustomName(name) => {
                self.radio_add_name = name;
                Task::none()
            }
            Message::RadioAddCustomUrl(url) => {
                self.radio_add_url = url;
                Task::none()
            }
            Message::RadioAddCustomSubmit => {
                let name = self.radio_add_name.trim().to_string();
                let url = self.radio_add_url.trim().to_string();
                if !name.is_empty() && !url.is_empty() {
                    self.config.add_radio_station(name, url);
                    self.config.save_and_log("radio station (add)");
                    self.radio_add_name.clear();
                    self.radio_add_url.clear();
                }
                Task::none()
            }
            Message::RadioRemoveStation(url) => {
                self.config.remove_radio_station(&url);
                self.config.save_and_log("radio station (remove)");
                Task::none()
            }

            // =================================================================
            // CD
            // =================================================================
            Message::CdPlayWhole => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                let device = self.config.cd_device.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        let uri = match &device {
                            Some(dev) => format!("cdda://{dev}"),
                            None => "cdda:///".to_string(),
                        };
                        client.add(&uri).await.ok();
                        client.play().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::CdProbe => {
                self.cd_tracks.clear();
                self.cd_probing = true;
                let client = self.client.clone();
                let device = self.config.cd_device.clone();
                Task::perform(
                    async move {
                        // Preferred: lsinfo on the configured device path.
                        // Returns track URIs with durations without touching the queue.
                        if let Some(ref dev) = device {
                            let lsinfo_uri = format!("cdda://{dev}");
                            if let Ok(entries) = client.lsinfo(&lsinfo_uri).await {
                                let tracks: Vec<(String, Option<f64>)> = entries
                                    .into_iter()
                                    .filter_map(|e| match e {
                                        crate::mpd::DirectoryEntry::File(s) => {
                                            Some((s.file, s.duration_secs))
                                        }
                                        _ => None,
                                    })
                                    .collect();
                                if !tracks.is_empty() {
                                    return tracks;
                                }
                            }
                        }
                        // Batch probe: add all tracks first, read durations from the
                        // queue, then delete the range in one shot. This avoids the
                        // "exception: Failed to load file" log spam that the old
                        // add_id + immediate delete_id pattern caused (MPD starts a
                        // background read on add; deleting before it completes fails).
                        let Ok(status) = client.status().await else {
                            return vec![];
                        };
                        let queue_start = status.queue_length;
                        let mut added = 0u32;
                        for i in 1u32..=99 {
                            let uri = match &device {
                                Some(dev) => format!("cdda://{dev}/{i}"),
                                None => format!("cdda:///{i}"),
                            };
                            if client.add(&uri).await.is_err() {
                                break;
                            }
                            added += 1;
                        }
                        if added == 0 {
                            return vec![];
                        }
                        // Read the slice we just added to get URIs and durations.
                        let queue = client.queue().await.unwrap_or_default();
                        let tracks: Vec<(String, Option<f64>)> = queue
                            .into_iter()
                            .skip(queue_start as usize)
                            .take(added as usize)
                            .map(|s| (s.file, s.duration_secs))
                            .collect();
                        // Remove everything we added in a single range delete.
                        client.delete_range_from(queue_start).await.ok();
                        tracks
                    },
                    Message::CdTracksLoaded,
                )
            }
            Message::CdTracksLoaded(tracks) => {
                self.cd_tracks = tracks;
                self.cd_probing = false;
                Task::none()
            }
            Message::CdPlayTrack(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.clear().await.ok();
                        client.add(&uri).await.ok();
                        client.play().await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::CdAddTrack(uri) => {
                self.playing_from_playlist = None;
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::HostChanged(h) => {
                self.settings_host = h;
                Task::none()
            }
            Message::PortChanged(p) => {
                self.settings_port = p;
                Task::none()
            }
            Message::PasswordChanged(p) => {
                self.settings_password = p;
                Task::none()
            }
            Message::CdDeviceChanged(d) => {
                self.settings_cd_device = d;
                Task::none()
            }
            Message::StartRename(name) => {
                self.settings_rename_input = name.clone();
                self.settings_renaming = Some(name);
                Task::none()
            }
            Message::RenameInputChanged(s) => {
                self.settings_rename_input = s;
                Task::none()
            }
            Message::ConfirmRename => {
                let new_name = self.settings_rename_input.trim().to_string();
                if let Some(old_name) = self.settings_renaming.take() {
                    if !new_name.is_empty()
                        && !self.config.servers.iter().any(|s| s.name == new_name && s.name != old_name)
                    {
                        if let Some(s) = self.config.server_mut(&old_name) {
                            s.name = new_name.clone();
                        }
                        if self.active_server == old_name {
                            self.active_server = new_name.clone();
                        }
                        if self.config.default_server.as_deref() == Some(old_name.as_str()) {
                            self.config.default_server = Some(new_name.clone());
                        }
                        self.config.save_and_log("server rename");
                    }
                }
                self.settings_rename_input.clear();
                Task::none()
            }
            Message::CancelRename => {
                self.settings_renaming = None;
                self.settings_rename_input.clear();
                Task::none()
            }
            Message::StartEditServer(name) => {
                if let Some(s) = self.config.server(&name) {
                    self.settings_edit_host = s.host.clone();
                    self.settings_edit_port = s.port.to_string();
                    self.settings_edit_password = s.password.clone().unwrap_or_default();
                    self.settings_edit_snap_host = s.snapcast_host.clone().unwrap_or_default();
                    self.settings_edit_snap_port = s
                        .snapcast_port
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    self.settings_editing = Some(name);
                }
                Task::none()
            }
            Message::EditServerHost(v) => {
                self.settings_edit_host = v;
                Task::none()
            }
            Message::EditServerPort(v) => {
                self.settings_edit_port = v;
                Task::none()
            }
            Message::EditServerPassword(v) => {
                self.settings_edit_password = v;
                Task::none()
            }
            Message::EditServerSnapHost(v) => {
                self.settings_edit_snap_host = v;
                Task::none()
            }
            Message::EditServerSnapPort(v) => {
                self.settings_edit_snap_port = v;
                Task::none()
            }
            Message::CancelEditServer => {
                self.settings_editing = None;
                Task::none()
            }
            Message::ConfirmEditServer => {
                let Some(name) = self.settings_editing.take() else {
                    return Task::none();
                };
                let host = self.settings_edit_host.trim().to_string();
                if host.is_empty() {
                    return Task::none();
                }
                // Keep the existing port on unparseable input rather than
                // silently resetting it to 6600.
                let port = self
                    .settings_edit_port
                    .trim()
                    .parse::<u16>()
                    .unwrap_or_else(|_| {
                        self.config.server(&name).map(|s| s.port).unwrap_or(6600)
                    });
                let password = Self::opt_string(&self.settings_edit_password);
                let snap_host = Self::opt_string(&self.settings_edit_snap_host);
                let snap_port = self.settings_edit_snap_port.trim().parse::<u16>().ok();

                let addr_changed = self
                    .config
                    .server(&name)
                    .is_some_and(|s| s.host != host || s.port != port || s.password != password);

                if let Some(s) = self.config.server_mut(&name) {
                    s.host = host;
                    s.port = port;
                    s.password = password;
                    s.snapcast_host = snap_host;
                    s.snapcast_port = snap_port;
                }
                self.config.save_and_log("server details");

                // Editing the server we're talking to means reconnecting to
                // the new address; SwitchServer already does exactly that,
                // including mirroring the legacy fields.
                if addr_changed && self.active_server == name {
                    self.active_server.clear(); // force SwitchServer past its no-op guard
                    return self.update(Message::SwitchServer(name));
                }
                // Snapcast host/port may have moved even when MPD didn't.
                self.snapcast_client = None;
                Task::none()
            }
            Message::ArtCacheSizeChanged(v) => {
                self.settings_cache_size = v;
                Task::none()
            }
            Message::SaveArtCacheSize => {
                if let Ok(mb) = self.settings_cache_size.trim().parse::<u32>() {
                    if mb > 0 {
                        self.config.art_cache_size_mb = mb;
                        self.config.save_and_log("art cache size");
                        // Applies to the next store; already-cloned handles
                        // keep the old limit for their remaining lifetime,
                        // which only affects fetches already in flight.
                        self.art_cache.set_limit_mb(mb);
                    }
                }
                self.settings_cache_size = self.config.art_cache_size_mb.to_string();
                Task::none()
            }
            Message::SaveCdDevice => {
                self.config.cd_device = if self.settings_cd_device.trim().is_empty() {
                    None
                } else {
                    Some(self.settings_cd_device.trim().to_string())
                };
                self.config.save_and_log("CD device");
                Task::none()
            }

            // =================================================================
            // Snapcast
            // =================================================================
            Message::SnapcastPollTick => self.fetch_snapcast_status(),
            Message::SnapcastStatusLoaded(groups, streams) => {
                self.snapcast_groups = groups;
                self.snapcast_streams = streams;
                self.snapcast_error = None;
                Task::none()
            }
            Message::SnapcastToggleShowInactive => {
                self.snapcast_show_inactive = !self.snapcast_show_inactive;
                Task::none()
            }
            Message::SnapcastUnreachable(msg) => {
                self.snapcast_error = Some(msg);
                Task::none()
            }
            Message::SnapcastSetVolume(client_id, percent) => {
                let mut muted = false;
                for g in &mut self.snapcast_groups {
                    for c in &mut g.clients {
                        if c.id == client_id {
                            c.volume = percent;
                            muted = c.muted;
                        }
                    }
                }
                match self.snapcast_client.clone() {
                    Some(client) => Task::perform(
                        async move {
                            let _ = client.set_volume(&client_id, percent, muted).await;
                        },
                        |_| Message::Noop,
                    ),
                    None => Task::none(),
                }
            }
            Message::SnapcastToggleClientMute(client_id, was_muted) => {
                let new_muted = !was_muted;
                let mut percent = 0u8;
                for g in &mut self.snapcast_groups {
                    for c in &mut g.clients {
                        if c.id == client_id {
                            c.muted = new_muted;
                            percent = c.volume;
                        }
                    }
                }
                match self.snapcast_client.clone() {
                    Some(client) => Task::perform(
                        async move {
                            let _ = client.set_volume(&client_id, percent, new_muted).await;
                        },
                        |_| Message::Noop,
                    ),
                    None => Task::none(),
                }
            }
            Message::SnapcastToggleGroupMute(group_id, was_muted) => {
                let new_muted = !was_muted;
                for g in &mut self.snapcast_groups {
                    if g.id == group_id {
                        g.muted = new_muted;
                    }
                }
                match self.snapcast_client.clone() {
                    Some(client) => Task::perform(
                        async move {
                            let _ = client.set_group_mute(&group_id, new_muted).await;
                        },
                        |_| Message::Noop,
                    ),
                    None => Task::none(),
                }
            }
            Message::SnapcastSetGroupStream(group_id, stream_id) => {
                for g in &mut self.snapcast_groups {
                    if g.id == group_id {
                        g.stream_id = stream_id.clone();
                    }
                }
                match self.snapcast_client.clone() {
                    Some(client) => Task::perform(
                        async move {
                            let _ = client.set_group_stream(&group_id, &stream_id).await;
                        },
                        |_| Message::Noop,
                    ),
                    None => Task::none(),
                }
            }

            // Legacy — no longer in the UI; kept for compatibility.
            Message::SaveSettings => Task::none(),
            Message::ServerNameChanged(s) => {
                self.settings_server_name = s;
                Task::none()
            }
            Message::SwitchServer(name) => {
                if name == self.active_server {
                    return Task::none();
                }
                self.active_server = name.clone();
                let addr = self.config.server_addr(&name);
                self.client = MpdClient::new(&addr);
                self.connected = false;
                // Mirror legacy fields to the active server.
                if let Some(s) = self.config.server(&name) {
                    let host = s.host.clone();
                    let port = s.port;
                    let password = s.password.clone();
                    let partition = s.default_partition.clone();
                    self.config.mpd_host = host;
                    self.config.mpd_port = port;
                    self.config.mpd_password = password;
                    self.config.default_partition = partition;
                }
                self.recently_played.clear();

                // Abandon art still queued for the previous server's library
                // — its lists are about to be replaced, and every pending
                // fetch would run `find` against the new connection for an
                // album that may not exist there. Loaded handles and known
                // misses stay: both are keyed by artist/album, so they're
                // server-agnostic. Zeroing `art_inflight` alongside
                // `art_pending` keeps the two consistent — fetches already
                // running now report as non-queue completions (their key is
                // no longer pending) and so must not decrement it.
                self.art_queue.clear();
                self.art_pending.clear();
                self.art_inflight = 0;
                self.mb_queue.clear();
                self.mb_pending.clear();
                self.mb_inflight = 0;

                // Drop the Snapcast client and its cached view state — it
                // captured the *previous* server's address, and nothing
                // else would notice the server changed while that view is
                // closed. on_view_enter's is_none() check then rebuilds it
                // against the new server on the next Snapcast visit.
                self.snapcast_client = None;
                self.snapcast_groups.clear();
                self.snapcast_streams.clear();
                self.snapcast_error = None;

                let store = self.store.clone();
                let server = name.clone();
                let load_history = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || store.recently_played_get(&server))
                            .await
                            .unwrap_or_default()
                    },
                    Message::RecentlyPlayedLoaded,
                );
                Task::batch([load_history, Task::perform(async {}, |_| Message::Connect)])
            }
            Message::SetDefaultServer(name) => {
                self.config.default_server = Some(name.clone());
                self.config.save_and_log("default server");
                // Also switch active connection so the sidebar dropdown reflects
                // the new default immediately.
                if name != self.active_server {
                    return self.update(Message::SwitchServer(name));
                }
                Task::none()
            }
            Message::AddServer => {
                let name = self.settings_server_name.trim().to_string();
                let host = self.settings_host.trim().to_string();
                let port: u16 = self.settings_port.trim().parse().unwrap_or(6600);
                let password = if self.settings_password.is_empty() {
                    None
                } else {
                    Some(self.settings_password.clone())
                };
                if !name.is_empty()
                    && !host.is_empty()
                    && !self.config.servers.iter().any(|s| s.name == name)
                {
                    self.config.servers.push(crate::config::MpdServer {
                        name,
                        host,
                        port,
                        password,
                        default_partition: None,
                        snapcast_host: None,
                        snapcast_port: None,
                    });
                    self.config.save_and_log("server (add)");
                    self.settings_server_name.clear();
                    self.settings_host.clear();
                    self.settings_port.clear();
                    self.settings_password.clear();
                }
                Task::none()
            }
            Message::RemoveServer(name) => {
                if self.config.servers.len() <= 1 {
                    return Task::none();
                }
                let switching = self.active_server == name;
                self.config.servers.retain(|s| s.name != name);
                if self.config.default_server.as_deref() == Some(name.as_str()) {
                    self.config.default_server =
                        self.config.servers.first().map(|s| s.name.clone());
                }
                self.config.save_and_log("server (remove)");
                let store = self.store.clone();
                let removed = name.clone();
                let delete_history = Task::perform(
                    async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            store.recently_played_delete(&removed)
                        })
                        .await;
                    },
                    |_| Message::Noop,
                );
                if switching {
                    if let Some(first) = self.config.servers.first() {
                        let first_name = first.name.clone();
                        return Task::batch([
                            delete_history,
                            self.update(Message::SwitchServer(first_name)),
                        ]);
                    }
                }
                delete_history
            }

            // =================================================================
            // Lyrics
            // =================================================================
            Message::LyricsLoaded(key, lyrics) => {
                self.lyrics.insert(key, lyrics);
                Task::none()
            }
            Message::ToggleLyricsFollow => {
                self.lyrics_follow = !self.lyrics_follow;
                // Re-syncing on the next 500ms tick would feel like the button
                // didn't take, so snap straight away when following resumes.
                if self.lyrics_follow {
                    return self.lyrics_autoscroll();
                }
                Task::none()
            }
            Message::ToggleLyrics => {
                self.show_lyrics = !self.show_lyrics;
                Task::none()
            }

            // =================================================================
            // Log
            // =================================================================
            Message::LogClear => {
                crate::logger::clear_entries();
                self.log_entries.clear();
                Task::none()
            }
            Message::LogCopyAll => {
                let show_mpd = self.log_show_mpd_only;
                let text = self.log_entries
                    .iter()
                    .filter(|e| !show_mpd || e.target.contains("mpd"))
                    .map(|e| {
                        let target = e.target.strip_prefix("winrmpc::").unwrap_or(&e.target);
                        format!("[{}] {:5} {}  {}", e.timestamp, e.level, target, e.message)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                iced::clipboard::write(text)
            }
            Message::LogToggleMpdOnly => {
                self.log_show_mpd_only = !self.log_show_mpd_only;
                Task::none()
            }

            // =================================================================
            // Server Statistics
            // =================================================================
            Message::StatsLoaded(stats) => {
                self.stats = Some(stats);
                Task::none()
            }
            Message::UpdateDatabase => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        match client.update(None).await {
                            Ok(job_id) => Message::DatabaseUpdating(job_id),
                            Err(e) => Message::ErrorOccurred(e.to_string()),
                        }
                    },
                    |msg| msg,
                )
            }
            Message::DatabaseUpdating(_job_id) => {
                self.toast_info("Database update started");
                self.fetch_stats()
            }

            // =================================================================
            // Tick / Error / Noop
            // =================================================================
            Message::Tick => {
                self.log_entries = crate::logger::get_entries();
                // Expiry rides the 500ms poll that already exists — a
                // dedicated timer for a 4-second toast would be waste.
                self.toasts.retain(|t| !t.expired());
                if self.connected {
                    let mut tasks = vec![self.refresh_status(), self.lyrics_autoscroll()];
                    if self.current_view == View::ServerStats {
                        tasks.push(self.fetch_stats());
                    }
                    Task::batch(tasks)
                } else {
                    Task::none()
                }
            }
            Message::SetDarkMode(dark) => {
                self.config.theme.dark_mode = dark;
                colors::set_dark_mode(dark);
                self.config.save_and_log("appearance");
                Task::none()
            }
            Message::DismissToast => {
                self.toasts.pop_front();
                Task::none()
            }
            Message::ErrorOccurred(e) => {
                tracing::error!("{e}");
                self.toast_error(e);
                Task::none()
            }
            Message::Noop => Task::none(),
        }
    }

    /// Show a transient error over the content.
    ///
    /// Use this **instead of writing `last_error` directly** for anything the
    /// user should notice. `last_error` still backs the Settings status line,
    /// so both are set — but this is the half that is visible from wherever
    /// they actually are.
    fn toast_error(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.last_error = Some(text.clone());
        self.push_toast(Toast {
            text,
            is_error: true,
            shown_at: std::time::Instant::now(),
        });
    }

    /// Show a transient progress/status note.
    fn toast_info(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.last_error = Some(text.clone());
        self.push_toast(Toast {
            text,
            is_error: false,
            shown_at: std::time::Instant::now(),
        });
    }

    fn push_toast(&mut self, toast: Toast) {
        // Drop the oldest rather than refusing the newest: the most recent
        // failure is the one the user is most likely to be looking for.
        while self.toasts.len() >= MAX_TOASTS {
            self.toasts.pop_front();
        }
        self.toasts.push_back(toast);
    }

    /// The playing track's URI, for row highlighting in library listings.
    ///
    /// Views get the URI rather than `&Option<Song>` on purpose: it makes the
    /// match rule obvious at the call site and stops a view reaching for other
    /// fields. The Queue is the exception — it gets `status.song_pos`, because
    /// it is the only list where the same file can appear twice and position
    /// is what tells the two entries apart. See `widgets::song_row`.
    fn current_file(&self) -> Option<&str> {
        self.current_song.as_ref().map(|s| s.file.as_str())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = widgets::sidebar::view(
            &self.current_view,
            self.connected,
            &self.config.server_addr(&self.active_server),
            &self.config.servers,
            &self.active_server,
        );

        let main_content: Element<Message> = match &self.current_view {
            View::NowPlaying => {
                let art_handle = self
                    .current_song
                    .as_ref()
                    .and_then(|s| self.art_handles.get(&s.art_key()));
                // Resolve next song from status.next_song_pos + queue
                let next_song = self.status.next_song_pos.and_then(|pos| {
                    self.queue.iter().find(|s| s.pos == Some(pos))
                });
                // Lyrics: None=loading, Some(None)=not found, Some(Some(l))=found
                let lyrics: Option<Option<&crate::lyrics::Lyrics>> =
                    self.current_song.as_ref().and_then(|s| {
                        self.lyrics.get(&s.lyrics_key()).map(|opt| opt.as_ref())
                    });
                views::now_playing::view(
                    &self.current_song,
                    &self.status,
                    art_handle,
                    next_song,
                    &self.recent_albums,
                    &self.art_handles,
                    lyrics,
                    self.show_lyrics,
                    self.lyrics_follow,
                    self.lyrics_scroll_id.clone(),
                    self.playing_from_playlist.as_deref(),
                )
            }
            View::Queue => {
                views::queue::view(
                    &self.queue,
                    self.status.song_pos,
                    self.queue_scroll_id.clone(),
                )
            }
            View::Library => {
                // Redirect to Artists if someone navigates here
                views::artists_list::view(&self.artists)
            }
            View::Artists => {
                views::artists_list::view(&self.artists)
            }
            View::Albums => {
                views::albums_list::view(
                    &self.albums,
                    "Albums",
                    &self.art_handles,
                    self.config.album_grid_view,
                    self.current_song.as_ref(),
                )
            }
            View::Genres => {
                views::genres_list::view(&self.genres)
            }
            View::RecentlyAdded => {
                views::albums_list::view(
                    &self.recently_added_albums,
                    "Recently Added",
                    &self.art_handles,
                    self.config.album_grid_view,
                    self.current_song.as_ref(),
                )
            }
            View::RecentlyPlayed => views::recently_played::view(
                &self.recently_played,
                self.recently_played_show_albums,
                self.config.album_grid_view,
                &self.art_handles,
                self.current_file(),
                self.current_song.as_ref(),
            ),
            View::ArtistDetail(name) => {
                let albums = self
                    .artist_albums
                    .get(name)
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                let bio = self.artist_bios.get(name).and_then(|o| o.as_deref());
                views::artist::view(
                    name,
                    albums,
                    &self.art_handles,
                    bio,
                    self.show_artist_bio,
                    self.current_song.as_ref(),
                )
            }
            View::AlbumDetail(name, artist) => {
                let key = album_scoped_key(artist.as_deref(), name);
                let songs = self
                    .album_songs
                    .get(&key)
                    .map(|s| s.as_slice())
                    .unwrap_or(&[]);
                let art_key = songs
                    .first()
                    .map(|s| s.art_key())
                    .unwrap_or_default();
                let art = self.art_handles.get(&art_key);
                let bio = self.album_bios.get(&key).and_then(|o| o.as_deref());
                views::album::view(
                    name,
                    songs,
                    art,
                    bio,
                    self.show_album_bio,
                    self.current_file(),
                )
            }
            View::GenreDetail(name) => {
                let albums = self
                    .genre_albums
                    .get(name)
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                views::genre_detail::view(name, albums, self.current_song.as_ref())
            }
            View::Browser => {
                views::browser::view(
                    &self.browser_path,
                    &self.browser_entries,
                    self.current_file(),
                )
            }
            View::Search => {
                views::search::view(
                    &self.search_query,
                    &self.search_results,
                    self.current_file(),
                )
            }
            View::Radio => {
                views::radio::view(
                    &self.config.radio_stations,
                    &self.radio_add_name,
                    &self.radio_add_url,
                )
            }
            View::CD => {
                views::cd::view(&self.cd_tracks, self.cd_probing, &self.settings_cd_device)
            }
            View::Outputs => views::outputs::view(&self.outputs, &self.partitions),
            View::Snapcast => views::snapcast::view(
                &self.snapcast_groups,
                &self.snapcast_streams,
                self.snapcast_error.as_deref(),
                self.snapcast_show_inactive,
            ),
            View::Partitions => {
                let current = self
                    .status
                    .partition
                    .as_deref()
                    .unwrap_or("default");
                views::partitions::view(
                    &self.partitions,
                    current,
                    &self.new_partition_name,
                )
            }
            View::Settings => self.settings_view(),
            View::Log => views::log::view(&self.log_entries, self.log_show_mpd_only),
            View::ServerStats => views::server_stats::view(
                self.stats.as_ref(),
                self.status.updating_db.is_some(),
            ),
            View::Playlists => views::playlists_list::view(
                &self.playlists,
                &self.new_playlist_name,
                self.playlist_renaming.as_deref(),
                &self.playlist_rename_input,
                self.queue.is_empty(),
            ),
            View::PlaylistDetail(name) => {
                let songs = self
                    .playlist_songs
                    .get(name)
                    .map(|s| s.as_slice())
                    .unwrap_or(&[]);
                let art_key = songs
                    .first()
                    .map(|s| s.art_key())
                    .unwrap_or_default();
                let art = self.art_handles.get(&art_key);
                views::playlist_detail::view(name, songs, art, self.current_file())
            }
            View::AddToPlaylist => {
                let count = self
                    .add_to_playlist_uris
                    .as_ref()
                    .map(|u| u.len())
                    .unwrap_or(0);
                views::add_to_playlist::view(&self.playlists, &self.new_playlist_name, count)
            }
        };

        let player_bar =
            widgets::player_bar::view(
                &self.status,
                &self.current_song,
                self.replay_gain_mode.as_deref(),
            );

        let content = column![
            row![sidebar, main_content].height(Length::Fill),
            player_bar,
        ];

        // Toasts float over the content rather than displacing it, so a
        // message can't reflow the view underneath it while it's being read.
        // Bottom-aligned above the player bar; `Shrink` on the overlay column
        // is what keeps it from covering (and swallowing clicks meant for)
        // the whole window.
        let content: Element<'_, Message> = if self.toasts.is_empty() {
            content.into()
        } else {
            let mut stack = iced::widget::stack![content];
            let toasts = column(self.toasts.iter().map(toast_view).collect::<Vec<_>>())
                .spacing(6)
                .width(Length::Shrink);
            stack = stack.push(
                container(toasts)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right)
                    .align_y(iced::alignment::Vertical::Bottom)
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 20.0,
                        // Clear of the player bar.
                        bottom: 110.0,
                        left: 0.0,
                    }),
            );
            stack.into()
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(AppColors::bg_primary().into()),
                ..Default::default()
            })
            .into()
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn mpd_cmd<F, Fut>(&self, f: F) -> Task<Message>
    where
        F: FnOnce(MpdClient) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = crate::mpd::error::MpdResult<()>>
            + Send,
    {
        let client = self.client.clone();
        Task::perform(
            async move {
                if let Err(e) = f(client).await {
                    return Message::ErrorOccurred(e.to_string());
                }
                Message::Tick
            },
            |msg| msg,
        )
    }

    fn fetch_all(&self) -> Task<Message> {
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();
        let c4 = self.client.clone();
        let c5 = self.client.clone();
        let c6 = self.client.clone();

        let status_task = Task::perform(
            async move { c1.status().await.ok().map(Box::new) },
            |s| match s {
                Some(status) => Message::StatusUpdated(status),
                None => Message::Noop,
            },
        );

        let song_task = Task::perform(
            async move {
                c2.current_song().await.ok().flatten().map(Box::new)
            },
            |s| Message::CurrentSongUpdated(s),
        );

        let queue_task = Task::perform(
            async move { c3.queue().await.unwrap_or_default() },
            Message::QueueUpdated,
        );

        let outputs_task = Task::perform(
            async move { c4.outputs().await.unwrap_or_default() },
            Message::OutputsUpdated,
        );

        let partitions_task = Task::perform(
            async move {
                c5.list_partitions().await.unwrap_or_default()
            },
            Message::PartitionsUpdated,
        );

        let replay_gain_task = Task::perform(
            async move { c6.replay_gain_status().await.ok() },
            |mode| match mode {
                Some(m) => Message::ReplayGainModeLoaded(m),
                None => Message::Noop,
            },
        );

        Task::batch([
            status_task,
            song_task,
            queue_task,
            outputs_task,
            partitions_task,
            replay_gain_task,
        ])
    }

    fn refresh_status(&self) -> Task<Message> {
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();

        let status_task = Task::perform(
            async move { c1.status().await.ok().map(Box::new) },
            |s| match s {
                Some(status) => Message::StatusUpdated(status),
                None => Message::Disconnected,
            },
        );

        let song_task = Task::perform(
            async move {
                c2.current_song().await.ok().flatten().map(Box::new)
            },
            |s| Message::CurrentSongUpdated(s),
        );

        let queue_task = Task::perform(
            async move { c3.queue().await.unwrap_or_default() },
            Message::QueueUpdated,
        );

        Task::batch([status_task, song_task, queue_task])
    }

    fn fetch_art(&self, uri: String, key: String) -> Task<Message> {
        // Never request art for CD audio tracks. MPD tries to open the CD drive
        // when asked for albumart/readpicture on cdda:// URIs, which corrupts its
        // internal state and triggers cascading "Failed to open CD drive" failures.
        if uri.starts_with("cdda://") {
            return Task::none();
        }

        let client = self.client.clone();
        let cache = self.art_cache.clone_inner();
        let mb = self.mb_client.clone();
        let gate = self.art_fetch_gate.clone();

        Task::perform(
            async move {
                // Check cache first
                if let Some(data) = cache.get(&key).await {
                    return (key, Some(data));
                }

                let _permit = gate.acquire().await;

                // Try MPD embedded art: tag first, then a separate cover-file
                // image (e.g. cover.jpg) — tag art is probed first because on
                // a tagged library it's the one that actually exists (see
                // docs/plans/art-wikipedia-fetch-order-and-caching.md §1).
                // This runs even when the negative cache says "missing" —
                // it's a cheap local query, and a stale negative entry (e.g.
                // written by an empty-URI recents fetch that could only try
                // MusicBrainz) must not block it forever.
                if !uri.is_empty() {
                    if let Ok(Some(data)) = client.tag_art(&uri).await {
                        let _ = cache.store(&key, &data).await;
                        return (key, Some(data));
                    }
                    if let Ok(Some(data)) = client.cover_file_art(&uri).await {
                        let _ = cache.store(&key, &data).await;
                        return (key, Some(data));
                    }
                }

                // Persisted negative cache gates only the expensive,
                // rate-limited MusicBrainz lookup.
                if cache.is_known(&key).await {
                    return (key, None);
                }

                // Parse artist and album from the key (format: "artist\x1falbum")
                if let Some((artist, album)) = key.split_once('\x1f') {
                    // Try MusicBrainz / Cover Art Archive
                    if let Some(data) = mb.fetch_album_art(artist, album).await {
                        let _ = cache.store(&key, &data).await;
                        return (key, Some(data));
                    }
                }

                // Only persist "not found" when we had a real URI and actually
                // tried MPD. An empty-URI fetch (used for recently-played art at
                // startup) only reaches MusicBrainz; a miss there must NOT poison
                // the cache, because when the album is played later in the same
                // session fetch_art will be called with a real file URI and we
                // want the MPD embedded-art path to still run.
                if !uri.is_empty() {
                    cache.store_empty(&key).await;
                }
                (key, None)
            },
            |(key, data)| Message::ArtLoaded(key, data),
        )
    }

    fn fetch_artist_art(&self, artist_name: String) -> Task<Message> {
        let cache = self.art_cache.clone_inner();
        let key = format!("artist:{artist_name}");

        if self.art_handles.contains_key(&key) {
            return Task::none();
        }

        let mb = self.mb_client.clone();
        let gate = self.art_fetch_gate.clone();
        Task::perform(
            async move {
                // Check cache
                if let Some(data) = cache.get(&key).await {
                    return (key, Some(data));
                }

                let _permit = gate.acquire().await;

                // Fetch from MusicBrainz (uses first album cover as artist image)
                if let Some(data) = mb.fetch_artist_art(&artist_name).await {
                    let _ = cache.store(&key, &data).await;
                    return (key, Some(data));
                }

                cache.store_empty(&key).await;
                (key, None)
            },
            |(key, data)| Message::ArtLoaded(key, data),
        )
    }

    /// Fetch an artist's Wikipedia bio: in-memory session cache → redb →
    /// network (`MusicBrainzClient::fetch_artist_bio`) → persist. Mirrors
    /// `fetch_lyrics`'s cache-then-network shape.
    fn fetch_artist_bio(&self, artist: String) -> Task<Message> {
        if self.artist_bios.contains_key(&artist) {
            return Task::none();
        }
        let mb = self.mb_client.clone();
        let store = self.store.clone();
        let key = format!("artist:{artist}");
        Task::perform(
            async move {
                let s = store.clone();
                let k = key.clone();
                let cached = tokio::task::spawn_blocking(move || s.bio_get(&k))
                    .await
                    .ok()
                    .flatten();
                let bio = if let Some(cached) = cached {
                    cached
                } else {
                    let fetched = mb.fetch_artist_bio(&artist).await;
                    let s = store.clone();
                    let k = key.clone();
                    let v = fetched.clone();
                    let _ = tokio::task::spawn_blocking(move || s.bio_put(&k, &v)).await;
                    fetched
                };
                (artist, bio)
            },
            |(name, bio)| Message::ArtistBioLoaded(name, bio),
        )
    }


    /// How many **local** album-art fetches run at once. Each is an MPD
    /// `find` plus `readpicture`/`albumart` probes — single-digit
    /// milliseconds each on a warm server — sharing one connection with the
    /// 500ms status poll, so the useful knob is rate, not total work. Three
    /// leaves headroom under `art_fetch_gate` (4) for the playing track.
    ///
    /// This replaces a flat 24-album cap. The cap was the reason grid view
    /// looked like it had stopped fetching: the first 24 covers appeared and
    /// every album after them stayed a grey placeholder no matter how long
    /// you waited, because nothing ever started the 25th fetch.
    const ART_FETCH_CONCURRENCY: usize = 3;

    // The MusicBrainz stage is deliberately **unbounded** — it grinds
    // through every album the local stage found nothing for, one at a time,
    // for as long as it takes. There was a 50/session budget here; it was
    // removed because the thing that actually needed fixing was the *stall*
    // (a lookup blocking the local sweep), not the total number of lookups.
    // Once the stages are separate, a slow background queue costs nothing
    // visible, and every result — hit or miss — is cached permanently, so
    // the work shrinks with every session.

    /// **Stage 1 — local only.** MPD tag art (`readpicture`), then a cover
    /// file beside the track (`albumart`). Never touches the network, so it
    /// costs single-digit milliseconds and can run over a whole library.
    /// `variant` is the raw album tag to look a track up by (a disc variant
    /// for multi-disc sets); `base` is the disc-stripped name behind the key.
    ///
    /// Deliberately **not** gated by the negative cache. Local probing is
    /// cheap and a negative here would be wrong the moment a `cover.jpg` is
    /// dropped next to the music or a tag is fixed — the case where a user
    /// most expects the client to notice. `art_missing` still stops it
    /// repeating within a session. (mikMPD reaches the same place from the
    /// other side: its `.miss` markers cover the whole chain, so they carry a
    /// 7-day TTL.)
    fn fetch_album_art_local(
        &self,
        artist: String,
        base: String,
        variant: String,
    ) -> Task<Message> {
        let art_key = art_key_for(&artist, &base);
        let c = self.client.clone();
        let cache = self.art_cache.clone_inner();
        let gate = self.art_fetch_gate.clone();
        Task::perform(
            async move {
                if let Some(data) = cache.get(&art_key).await {
                    return (art_key, ArtOutcome::Loaded(data));
                }
                let _permit = gate.acquire().await;

                let songs = match c.find("Album", &variant).await {
                    Ok(songs) => songs,
                    // Couldn't ask. Report a miss so the album is retried on
                    // the next visit, and persist nothing.
                    Err(_) => return (art_key, ArtOutcome::MpdMiss),
                };
                if let Some(first) = songs.first() {
                    if let Ok(Some(data)) = c.tag_art(&first.file).await {
                        let _ = cache.store(&art_key, &data).await;
                        return (art_key, ArtOutcome::Loaded(data));
                    }
                    if let Ok(Some(data)) = c.cover_file_art(&first.file).await {
                        let _ = cache.store(&art_key, &data).await;
                        return (art_key, ArtOutcome::Loaded(data));
                    }
                }

                // Nothing locally. One redb read decides whether stage 2 is
                // worth queuing: a persisted negative means MusicBrainz was
                // already asked about this album — in an earlier session,
                // most likely — and had nothing. Answering `Missing` here is
                // what keeps those albums from burning the session's
                // MusicBrainz budget on a lookup whose answer is on disk.
                if cache.is_known(&art_key).await {
                    return (art_key, ArtOutcome::Missing);
                }
                (art_key, ArtOutcome::MpdMiss)
            },
            |(key, outcome)| Message::AlbumArtFetched(key, outcome),
        )
    }

    /// **Stage 2 — MusicBrainz / Cover Art Archive.** Only ever reached for
    /// an album stage 1 found nothing for, one at a time, and only once
    /// `art_queue` has drained. The persisted negative written here is what
    /// makes the answer stick across sessions.
    fn fetch_album_art_remote(&self, art_key: String) -> Task<Message> {
        let cache = self.art_cache.clone_inner();
        let mb = self.mb_client.clone();
        Task::perform(
            async move {
                // Stage 1 may have landed this album's cover in the meantime
                // (another list, or the track started playing).
                if let Some(data) = cache.get(&art_key).await {
                    return (art_key, ArtOutcome::Loaded(data));
                }

                // Deliberately outside `art_fetch_gate`. This stage is
                // already limited to one in flight and ~1 req/s by
                // `MusicBrainzThrottle`, and a lookup holds its slot for
                // seconds — long enough that taking one of the gate's four
                // permits would make the playing track's own cover wait
                // behind background work for an album nobody is looking at.

                // The key *is* the query: `art_key_for` builds
                // "artist\x1fdisc-stripped-album", which is exactly what the
                // lookup wants. An empty artist degrades to a title-only
                // search rather than guessing.
                if let Some((artist, album)) = art_key.split_once('\x1f') {
                    if let Some(data) = mb.fetch_album_art(artist, album).await {
                        let _ = cache.store(&art_key, &data).await;
                        return (art_key, ArtOutcome::Loaded(data));
                    }
                }
                // Both stages have now asked; record it so no future session
                // repeats either the lookup or the wait.
                cache.store_empty(&art_key).await;
                (art_key, ArtOutcome::Missing)
            },
            |(key, outcome)| Message::AlbumArtFetched(key, outcome),
        )
    }

    /// A trimmed field, or `None` when it's blank — the shape every optional
    /// `MpdServer` field wants ("" and "unset" mean the same thing in a form).
    fn opt_string(s: &str) -> Option<String> {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }

    /// Read the on-disk art size for the Settings readout. Iterating
    /// `art_meta` is a redb read, so it goes through `spawn_blocking` like
    /// every other `Store` call.
    fn fetch_cache_size(&self) -> Task<Message> {
        let store = self.store.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || store.art_cache_bytes())
                    .await
                    .unwrap_or(0)
            },
            Message::CacheSizeLoaded,
        )
    }

    /// Queue art for every album in the list currently on screen, at the
    /// front of the queue. No-op on views that aren't album lists.
    ///
    /// Front, not back, because the list you just opened is the one you're
    /// looking at: without it, opening Recently Added while an 800-album
    /// Albums sweep is still queued put its covers behind all 800.
    fn queue_album_art(&mut self) -> Task<Message> {
        // (artist, base, variant-to-look-up)
        let groups: Vec<(String, String, String)> = match &self.current_view {
            View::Albums => Self::album_art_targets(&self.albums),
            View::RecentlyAdded => Self::album_art_targets(&self.recently_added_albums),
            View::RecentlyPlayed => recently_played_albums(&self.recently_played)
                .into_iter()
                .map(|g| (g.artist.clone(), g.album.clone(), g.album.clone()))
                .collect(),
            _ => return Task::none(),
        };

        self.enqueue_album_art(groups)
    }

    /// `(artist, base, first-variant)` triples for a list of album groups —
    /// the variant is the raw album tag a track can actually be found by.
    fn album_art_targets(groups: &[AlbumGroup]) -> Vec<(String, String, String)> {
        groups
            .iter()
            .map(|g| {
                let v = g.variants.first().cloned().unwrap_or_else(|| g.base.clone());
                (g.artist.clone(), g.base.clone(), v)
            })
            .collect()
    }

    /// Put albums at the head of the local-stage queue, skipping any already
    /// loaded or already resolved as missing this session, then start as many
    /// fetches as the concurrency limit allows.
    ///
    /// Albums already queued are *moved* to the front rather than skipped —
    /// that's what makes re-entering a view actually reprioritise it, since
    /// `art_pending` would otherwise treat "queued 600 albums ago" as done.
    fn enqueue_album_art(&mut self, groups: Vec<(String, String, String)>) -> Task<Message> {
        let wanted: Vec<(String, String, String)> = groups
            .into_iter()
            .filter(|(artist, base, _)| {
                let key = art_key_for(artist, base);
                !self.art_handles.contains_key(&key) && !self.art_missing.contains(&key)
            })
            .collect();
        if wanted.is_empty() {
            return Task::none();
        }

        // Drop any stale copies of these albums from further back in the
        // queue before re-inserting them at the front, so an album can't sit
        // in the queue twice.
        let keys: HashSet<String> = wanted
            .iter()
            .map(|(artist, base, _)| art_key_for(artist, base))
            .collect();
        self.art_queue
            .retain(|(artist, base, _)| !keys.contains(&art_key_for(artist, base)));

        // Reverse, because each push_front lands ahead of the last — this
        // leaves the list in its on-screen order.
        for (artist, base, variant) in wanted.into_iter().rev() {
            self.art_pending.insert(art_key_for(&artist, &base));
            self.art_queue.push_front((artist, base, variant));
        }
        self.drain_art_queue()
    }

    /// Start fetches until `ART_FETCH_CONCURRENCY` local ones are in flight,
    /// then — only once the local queue is empty — at most one MusicBrainz
    /// lookup. Called after every enqueue and again as each fetch completes,
    /// so both queues keep draining on their own.
    ///
    /// The ordering between the two is the whole point. A MusicBrainz lookup
    /// blocks for 1.1–2.2s on a globally serialized throttle; when the stages
    /// shared one queue, three such albums were enough to stall every local
    /// cover behind them, so a grid filled in at roughly one album per second
    /// and looked broken well before it reached anything you'd scrolled to.
    fn drain_art_queue(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        while self.art_inflight < Self::ART_FETCH_CONCURRENCY {
            let Some((artist, base, variant)) = self.art_queue.pop_front() else {
                break;
            };
            self.art_inflight += 1;
            tasks.push(self.fetch_album_art_local(artist, base, variant));
        }

        if self.art_queue.is_empty() && self.mb_inflight == 0 {
            if let Some(key) = self.mb_queue.pop_front() {
                self.mb_inflight += 1;
                tasks.push(self.fetch_album_art_remote(key));
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    /// Fetch an album's Wikipedia bio — same shape as `fetch_artist_bio`.
    /// Both the in-memory `album_bios` map and the redb key are scoped by
    /// `album_scoped_key(artist, album)`, matching `View::AlbumDetail`'s own
    /// `(name, artist)` so storage and render-time lookup always agree
    /// (previously the in-memory guard was keyed by album name alone, so
    /// two different artists' same-titled album showed each other's bio
    /// for the rest of the session — see
    /// docs/plans/review-fixes-correctness.md §2). The same `artist` — and
    /// *only* that artist — also drives the MusicBrainz/Wikipedia query, so
    /// key and query are always derived from the same input and can never
    /// disagree.
    ///
    /// There used to be a `self.selected_artist` fallback here for the
    /// `artist: None` case (reached from Genre detail). It was removed: the
    /// key stays `"\x1f{album}"` regardless, so the fallback wrote a bio
    /// fetched for *whatever artist page happened to be open* into a slot
    /// shared by every artist-less album of that title — permanently, in
    /// redb, and it would be read back for unrelated albums too. Querying
    /// with an empty artist instead degrades to a title-only Wikipedia
    /// lookup (still guarded by `title_matches`), which finds fewer bios
    /// but can never attribute one to the wrong album.
    fn fetch_album_bio(&self, artist: Option<String>, album: String) -> Task<Message> {
        let key = album_scoped_key(artist.as_deref(), &album);
        if self.album_bios.contains_key(&key) {
            return Task::none();
        }
        let query_artist = artist.clone().unwrap_or_default();
        let mb = self.mb_client.clone();
        let store = self.store.clone();
        Task::perform(
            async move {
                let s = store.clone();
                let k = key.clone();
                let cached = tokio::task::spawn_blocking(move || s.bio_get(&k))
                    .await
                    .ok()
                    .flatten();
                let bio = if let Some(cached) = cached {
                    cached
                } else {
                    let fetched = mb.fetch_album_bio(&query_artist, &album).await;
                    let s = store.clone();
                    let k = key.clone();
                    let v = fetched.clone();
                    let _ = tokio::task::spawn_blocking(move || s.bio_put(&k, &v)).await;
                    fetched
                };
                (key, bio)
            },
            |(key, bio)| Message::AlbumBioLoaded(key, bio),
        )
    }

    /// Kick off art fetches for any recently-played album not already in
    /// `art_handles`.  Passes an empty URI so the MPD embedded-art step is
    /// skipped (we have no file path), but disk cache and MusicBrainz fallback
    /// both work via the `art_key_for` key alone.
    fn fetch_recent_art(&self) -> Task<Message> {
        let tasks: Vec<Task<Message>> = self
            .recent_albums
            .iter()
            .filter_map(|r| {
                let key = art_key_for(&r.artist, &r.album);
                if self.art_handles.contains_key(&key) {
                    None
                } else {
                    Some(self.fetch_art(String::new(), key))
                }
            })
            .collect();
        Task::batch(tasks)
    }

    fn on_view_enter(&mut self, view: View) -> Task<Message> {
        match view {
            // Entering from another view, the lyrics scrollable can pick up
            // the scroll offset of whatever scrollable last occupied that
            // tree position — see `reset_lyrics_scroll`.
            View::NowPlaying => self.reset_lyrics_scroll(),
            View::Artists => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let mut artists = client
                            .list_tag("AlbumArtist")
                            .await
                            .unwrap_or_default();
                        let track_artists = client
                            .list_tag("Artist")
                            .await
                            .unwrap_or_default();
                        for a in track_artists {
                            if !a.is_empty() && !artists.contains(&a) {
                                artists.push(a);
                            }
                        }
                        artists.sort();
                        artists
                    },
                    Message::ArtistsLoaded,
                )
            }
            View::Albums => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let pairs = client.list_albums_by_artist().await.unwrap_or_default();
                        group_albums_by_artist(&pairs)
                    },
                    Message::AlbumsLoaded,
                )
            }
            View::Genres => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.list_tag("Genre").await.unwrap_or_default()
                    },
                    Message::GenresLoaded,
                )
            }
            View::Browser => {
                let path = self.browser_path.clone();
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let entries =
                            client.lsinfo(&path).await.unwrap_or_default();
                        (path, entries)
                    },
                    |(path, entries)| Message::BrowseLoaded(path, entries),
                )
            }
            View::Outputs => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.outputs().await.unwrap_or_default()
                    },
                    Message::OutputsUpdated,
                )
            }
            View::Partitions => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client
                            .list_partitions()
                            .await
                            .unwrap_or_default()
                    },
                    Message::PartitionsUpdated,
                )
            }
            View::Library => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let mut artists = client
                            .list_tag("AlbumArtist")
                            .await
                            .unwrap_or_default();
                        let track_artists = client
                            .list_tag("Artist")
                            .await
                            .unwrap_or_default();
                        for a in track_artists {
                            if !a.is_empty() && !artists.contains(&a) {
                                artists.push(a);
                            }
                        }
                        artists.sort();
                        artists
                    },
                    Message::ArtistsLoaded,
                )
            }
            View::Radio => {
                // No async loading needed — stations come from config
                Task::none()
            }
            View::Playlists => {
                let client = self.client.clone();
                Task::perform(
                    async move { client.list_playlists().await.unwrap_or_default() },
                    Message::PlaylistsLoaded,
                )
            }
            View::ServerStats => self.fetch_stats(),
            View::Settings => {
                // Leaving and returning shouldn't find the purge still armed
                // or a half-finished edit still open.
                self.confirm_clear_caches = false;
                self.settings_editing = None;
                self.settings_renaming = None;
                self.settings_cache_size = self.config.art_cache_size_mb.to_string();
                self.fetch_cache_size()
            }
            // History is already in memory; nothing to load, but the covers
            // may not be cached yet.
            View::RecentlyPlayed => self.queue_album_art(),
            View::RecentlyAdded => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        let since = (chrono::Utc::now() - chrono::Duration::days(30))
                            .format("%Y-%m-%dT%H:%M:%SZ")
                            .to_string();
                        client
                            .find_recently_added(&since, 2000)
                            .await
                            .unwrap_or_default()
                    },
                    Message::RecentlyAddedLoaded,
                )
            }
            View::Snapcast => {
                let addr = self
                    .config
                    .server(&self.active_server)
                    .map(|s| s.snapcast_addr());
                let Some(addr) = addr else {
                    return Task::none();
                };
                // Rebuild if we don't have a client yet, or if the
                // configured address changed since it was built (e.g.
                // snapcast_host/port edited in Settings) — SwitchServer
                // already clears snapcast_client on a server change, but a
                // same-server address edit wouldn't otherwise be noticed.
                let needs_new_client = match &self.snapcast_client {
                    None => true,
                    Some(c) => c.addr() != addr,
                };
                if needs_new_client {
                    self.snapcast_client = Some(crate::snapcast::SnapcastClient::new(&addr));
                }
                let client = self.snapcast_client.clone().unwrap();
                Task::perform(
                    async move {
                        // Reuse an already-open connection instead of
                        // tearing down a working socket and reconnecting
                        // on every single visit to this view.
                        if !client.is_connected().await {
                            client.connect().await?;
                        }
                        client.get_status().await
                    },
                    |result: Result<_, crate::snapcast::error::SnapcastError>| match result {
                        Ok((groups, streams)) => Message::SnapcastStatusLoaded(groups, streams),
                        Err(e) => Message::SnapcastUnreachable(e.to_string()),
                    },
                )
            }
            _ => Task::none(),
        }
    }

    fn fetch_stats(&self) -> Task<Message> {
        let client = self.client.clone();
        Task::perform(
            async move { client.stats().await.ok() },
            |stats| match stats {
                Some(s) => Message::StatsLoaded(s),
                None => Message::Tick,
            },
        )
    }

    fn fetch_snapcast_status(&self) -> Task<Message> {
        let Some(client) = self.snapcast_client.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { client.get_status().await },
            |result: Result<_, crate::snapcast::error::SnapcastError>| match result {
                Ok((groups, streams)) => Message::SnapcastStatusLoaded(groups, streams),
                Err(e) => Message::SnapcastUnreachable(e.to_string()),
            },
        )
    }

fn fetch_lyrics(&self, song: &Song) -> Task<Message> {
    let key = song.lyrics_key();
    // Already cached in memory (including "not found" = Some(None)) — skip.
    if self.lyrics.contains_key(&key) {
        return Task::none();
    }
    let client = self.lyrics_client.clone();
    let store = self.store.clone();
    let artist = song.display_artist().to_string();
    let title = song.display_title().to_string();
    let album = song.display_album().to_string();
    let duration = song.duration_secs;
    let k = key.clone();
    Task::perform(
        async move {
            // Check the persisted cache first (redb read on a blocking thread).
            let s = store.clone();
            let lookup_key = k.clone();
            let cached = tokio::task::spawn_blocking(move || s.lyrics_get(&lookup_key))
                .await
                .ok()
                .flatten();
            if let Some(cached) = cached {
                return (k, cached);
            }
            // Fetch from LRCLIB, then persist (including the negative result).
            let result = client.fetch(&artist, &title, &album, duration).await;
            let s = store.clone();
            let put_key = k.clone();
            let to_store = result.clone();
            tokio::task::spawn_blocking(move || s.lyrics_put(&put_key, &to_store))
                .await
                .ok();
            (k, result)
        },
        |(key, lyrics)| Message::LyricsLoaded(key, lyrics),
    )
}

/// Snap the lyrics pane back to the top.
///
/// Needed because iced reuses widget state by *(tree position, widget type)*
/// only — `scrollable::Id` is for targeting operations, not for identity —
/// so the lyrics scrollable inherits whatever offset the previously-rendered
/// scrollable at that path had. Going from a synced track (autoscrolled near
/// the bottom) to a plain-lyrics one would otherwise open scrolled past the
/// end of much shorter content, i.e. blank.
fn reset_lyrics_scroll(&self) -> Task<Message> {
    scrollable::snap_to(
        self.lyrics_scroll_id.clone(),
        scrollable::RelativeOffset::START,
    )
}

/// Keep the highlighted synced-lyric line in view by snapping the lyrics
/// scrollable to a position proportional to the active line. No-op unless
/// lyrics are shown and the current track has synced lyrics.
fn lyrics_autoscroll(&self) -> Task<Message> {
    // Also gated on the active view: the lyrics scrollable only exists in
    // the widget tree while Now Playing is open, so from any other view
    // this snap_to walks the tree every 500ms to reach nothing.
    // `lyrics_follow` is what makes the pane readable: without it this runs
    // every 500ms and yanks the view back to the active line, so the user can
    // never scroll anywhere else.
    if !self.show_lyrics || !self.lyrics_follow || self.current_view != View::NowPlaying {
        return Task::none();
    }
    let Some(song) = &self.current_song else {
        return Task::none();
    };
    let Some(Some(lyrics)) = self.lyrics.get(&song.lyrics_key()) else {
        return Task::none();
    };
    let Some(synced) = &lyrics.synced else {
        return Task::none();
    };
    if synced.len() < 2 {
        return Task::none();
    }
    let elapsed = self.status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
    // Before the first timestamp there is no active line; hold at the top.
    let active = crate::lyrics::active_line(synced, elapsed).unwrap_or(0);
    let ratio = active as f32 / (synced.len() - 1) as f32;
    scrollable::snap_to(
        self.lyrics_scroll_id.clone(),
        scrollable::RelativeOffset { x: 0.0, y: ratio },
    )
}

fn settings_view(&self) -> Element<'_, Message> {
        use iced::widget::{button, column, container, row, text, text_input, Space};

        let error_text: Element<'_, Message> = match &self.last_error {
            Some(e) => text(format!("Status: {e}"))
                .size(13)
                .color(AppColors::warning())
                .into(),
            None => Space::with_height(0).into(),
        };

        let conn_badge = if self.connected {
            text("Connected").size(13).color(AppColors::success())
        } else {
            text("Disconnected").size(13).color(AppColors::error())
        };

        // Server list rows
        let mut server_list = column![].spacing(4);
        for server in &self.config.servers {
            let is_active = server.name == self.active_server;
            let is_default =
                self.config.default_server.as_deref() == Some(server.name.as_str());
            let can_remove = self.config.servers.len() > 1;
            let is_renaming = self.settings_renaming.as_deref() == Some(server.name.as_str());

            let row_bg = if is_active {
                AppColors::bg_tertiary()
            } else {
                AppColors::bg_secondary()
            };

            let row_content: Element<'_, Message> = if is_renaming {
                // Inline rename row
                row![
                    text_input("Server name", &self.settings_rename_input)
                        .on_input(Message::RenameInputChanged)
                        .on_submit(Message::ConfirmRename)
                        .padding([4, 8])
                        .size(13)
                        .width(200),
                    Space::with_width(6),
                    button(text("Save").size(11))
                        .on_press(Message::ConfirmRename)
                        .padding([3, 10]),
                    Space::with_width(4),
                    button(text("Cancel").size(11))
                        .on_press(Message::CancelRename)
                        .padding([3, 10]),
                ]
                .align_y(iced::Alignment::Center)
                .spacing(4)
                .into()
            } else {
                let name_text: Element<'_, Message> = if is_active {
                    text(&server.name).size(13).color(AppColors::accent()).into()
                } else {
                    text(&server.name).size(13).color(AppColors::text_primary()).into()
                };

                let addr_text = text(server.addr()).size(11).color(AppColors::text_muted());

                let connect_btn: Element<'_, Message> = if is_active {
                    icon::icon_sized(icon::DOT, 13).color(AppColors::success()).into()
                } else {
                    button(text("Connect").size(11))
                        .on_press(Message::SwitchServer(server.name.clone()))
                        .padding([3, 8])
                        .into()
                };

                let default_btn: Element<'_, Message> = if is_default {
                    container(text("Default").size(10).color(AppColors::accent()))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme| container::Style {
                            background: None,
                            border: iced::Border {
                                color: AppColors::accent(),
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..Default::default()
                        })
                        .into()
                } else {
                    button(text("Set as default").size(10))
                        .on_press(Message::SetDefaultServer(server.name.clone()))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme, s: button::Status| button::Style {
                            background: None,
                            text_color: match s {
                                button::Status::Hovered | button::Status::Pressed => {
                                    AppColors::text_primary()
                                }
                                _ => AppColors::text_muted(),
                            },
                            border: iced::Border {
                                color: AppColors::text_muted(),
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            shadow: iced::Shadow::default(),
                        })
                        .into()
                };

                let quiet_btn = |label: &'static str, msg: Message| {
                    button(text(label).size(10))
                        .on_press(msg)
                        .padding([3, 8])
                        .style(|_t: &iced::Theme, s: button::Status| button::Style {
                            background: None,
                            text_color: match s {
                                button::Status::Hovered | button::Status::Pressed => {
                                    AppColors::text_primary()
                                }
                                _ => AppColors::text_muted(),
                            },
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        })
                };

                let edit_btn: Element<'_, Message> =
                    quiet_btn("Edit", Message::StartEditServer(server.name.clone())).into();
                let rename_btn: Element<'_, Message> =
                    quiet_btn("Rename", Message::StartRename(server.name.clone())).into();

                let remove_btn: Element<'_, Message> = if can_remove {
                    button(icon::icon_sized(icon::REMOVE, 13))
                        .on_press(Message::RemoveServer(server.name.clone()))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                            background: None,
                            text_color: AppColors::text_muted(),
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        })
                        .into()
                } else {
                    Space::with_width(0).into()
                };

                row![
                    name_text,
                    Space::with_width(8),
                    addr_text,
                    Space::with_width(Length::Fill),
                    connect_btn,
                    default_btn,
                    edit_btn,
                    rename_btn,
                    remove_btn,
                ]
                .align_y(iced::Alignment::Center)
                .spacing(4)
                .into()
            };

            // Connection details, expanded beneath the row being edited.
            let row_content: Element<'_, Message> =
                if self.settings_editing.as_deref() == Some(server.name.as_str()) {
                    let field = |label: &'static str,
                                 placeholder: &'static str,
                                 value: &str,
                                 width: u16,
                                 on_input: fn(String) -> Message| {
                        column![
                            text(label).size(10).color(AppColors::text_muted()),
                            text_input(placeholder, value)
                                .on_input(on_input)
                                .on_submit(Message::ConfirmEditServer)
                                .padding([4, 8])
                                .size(12)
                                .width(width),
                        ]
                        .spacing(2)
                    };

                    column![
                        row_content,
                        Space::with_height(6),
                        row![
                            field("Host", "192.168.1.50", &self.settings_edit_host, 150,
                                  Message::EditServerHost),
                            field("Port", "6600", &self.settings_edit_port, 60,
                                  Message::EditServerPort),
                            field("Password", "(none)", &self.settings_edit_password, 110,
                                  Message::EditServerPassword),
                        ]
                        .spacing(6),
                        Space::with_height(4),
                        row![
                            field("Snapcast host", "same as MPD",
                                  &self.settings_edit_snap_host, 150,
                                  Message::EditServerSnapHost),
                            field("Snapcast port", "1705",
                                  &self.settings_edit_snap_port, 60,
                                  Message::EditServerSnapPort),
                            column![
                                text(" ").size(10),
                                row![
                                    button(text("Save").size(11))
                                        .on_press(Message::ConfirmEditServer)
                                        .padding([4, 10]),
                                    Space::with_width(4),
                                    button(text("Cancel").size(11))
                                        .on_press(Message::CancelEditServer)
                                        .padding([4, 10]),
                                ],
                            ]
                            .spacing(2),
                        ]
                        .spacing(6)
                        .align_y(iced::Alignment::Start),
                    ]
                    .into()
                } else {
                    row_content
                };

            server_list = server_list.push(
                container(row_content)
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(move |_t: &iced::Theme| container::Style {
                        background: Some(row_bg.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }

        // Add-server form
        let add_form = column![
            text("Add server").size(14).color(AppColors::text_secondary()),
            Space::with_height(6),
            row![
                column![
                    text("Name").size(11).color(AppColors::text_muted()),
                    text_input("My Server", &self.settings_server_name)
                        .on_input(Message::ServerNameChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(Length::FillPortion(2)),
                Space::with_width(6),
                column![
                    text("Host").size(11).color(AppColors::text_muted()),
                    text_input("127.0.0.1", &self.settings_host)
                        .on_input(Message::HostChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(Length::FillPortion(3)),
                Space::with_width(6),
                column![
                    text("Port").size(11).color(AppColors::text_muted()),
                    text_input("6600", &self.settings_port)
                        .on_input(Message::PortChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(70),
                Space::with_width(6),
                column![
                    text("Password").size(11).color(AppColors::text_muted()),
                    text_input("", &self.settings_password)
                        .on_input(Message::PasswordChanged)
                        .padding(6)
                        .size(12)
                        .secure(true),
                ]
                .spacing(2)
                .width(Length::FillPortion(2)),
                Space::with_width(6),
                column![
                    Space::with_height(15),
                    button(text("Add").size(12))
                        .on_press(Message::AddServer)
                        .padding([6, 14]),
                ]
                .spacing(2),
            ]
            .align_y(iced::Alignment::End),
        ]
        .spacing(4);

        // --- Cache section ---
        let size_label = match self.cache_size_bytes {
            Some(bytes) => format!(
                "{:.1} MB of {} MB limit",
                bytes as f64 / (1024.0 * 1024.0),
                self.config.art_cache_size_mb
            ),
            None => "…".to_string(),
        };

        let size_limit_row = row![
            text("Limit").size(11).color(AppColors::text_muted()),
            Space::with_width(8),
            text_input("500", &self.settings_cache_size)
                .on_input(Message::ArtCacheSizeChanged)
                .on_submit(Message::SaveArtCacheSize)
                .padding([4, 8])
                .size(12)
                .width(70),
            Space::with_width(4),
            text("MB").size(11).color(AppColors::text_muted()),
            Space::with_width(8),
            button(text("Save").size(11))
                .on_press(Message::SaveArtCacheSize)
                .padding([4, 10]),
        ]
        .align_y(iced::Alignment::Center);

        let clear_row: Element<'_, Message> = if self.confirm_clear_caches {
            row![
                text("Clear all cached art, lyrics and biographies?")
                    .size(12)
                    .color(AppColors::text_secondary()),
                Space::with_width(10),
                button(text("Clear").size(12).color(AppColors::error()))
                    .on_press(Message::ClearCaches)
                    .padding([4, 12]),
                Space::with_width(6),
                button(text("Cancel").size(12))
                    .on_press(Message::CancelClearCaches)
                    .padding([4, 12]),
            ]
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                text(size_label).size(12).color(AppColors::text_muted()),
                Space::with_width(Length::Fill),
                button(text("Clear cache").size(12))
                    .on_press(Message::ClearCaches)
                    .padding([4, 12]),
            ]
            .align_y(iced::Alignment::Center)
            .into()
        };

        let cache_section = column![
            text("Cache").size(16).color(AppColors::text_primary()),
            Space::with_height(4),
            text(
                "Album art, lyrics and Wikipedia biographies are cached on disk. \
                 Clearing frees the space and forces a fresh lookup — covers \
                 re-download in the background, which for albums that need \
                 MusicBrainz takes a while. Play history is not affected."
            )
            .size(11)
            .color(AppColors::text_muted()),
            Space::with_height(8),
            size_limit_row,
            Space::with_height(8),
            clear_row,
        ]
        .spacing(2);

        // --- Storage section ---
        //
        // This exists because "I couldn't find the config file" is a real
        // report, and on macOS it is entirely explicable: the directory is
        // `~/Library/Application Support/com.winrmpc.winrmpc/`, which Finder
        // hides by default, under a reverse-DNS name that doesn't contain
        // "winrmpc" in the form anyone would search Spotlight for. Printing
        // the resolved path and offering a button that opens it answers the
        // question from inside the app, on every platform.
        let storage_row = |label: &'static str,
                           path: Option<String>,
                           dir: Option<String>,
                           note: Option<String>|
         -> Element<'_, Message> {
            let path_line: Element<'_, Message> = match &path {
                Some(p) => text(p.clone())
                    .size(11)
                    .color(AppColors::text_secondary())
                    .into(),
                None => text("unavailable — this will not be saved this session")
                    .size(11)
                    .color(AppColors::error())
                    .into(),
            };
            let mut left = column![
                text(label).size(12).color(AppColors::text_primary()),
                path_line,
            ]
            .spacing(1);
            if let Some(note) = note {
                left = left.push(text(note).size(10).color(AppColors::warning()));
            }

            let mut r = row![left.width(Length::Fill)].align_y(iced::Alignment::Center);
            if let Some(dir) = dir {
                r = r.push(
                    button(text("Open folder").size(11))
                        .on_press(Message::OpenStorageFolder(dir))
                        .padding([4, 10]),
                );
            }
            r.into()
        };

        // --- Appearance section ---
        let dark = self.config.theme.dark_mode;
        let mode_btn = |label: &'static str, is_dark: bool| {
            let selected = dark == is_dark;
            button(text(label).size(12))
                .on_press(Message::SetDarkMode(is_dark))
                .padding([4, 14])
                .style(move |_t: &iced::Theme, _s: button::Status| button::Style {
                    background: Some(if selected {
                        AppColors::accent().into()
                    } else {
                        AppColors::bg_tertiary().into()
                    }),
                    text_color: if selected {
                        AppColors::bg_primary()
                    } else {
                        AppColors::text_muted()
                    },
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
        };
        let appearance_section = column![
            text("Appearance").size(16).color(AppColors::text_primary()),
            Space::with_height(4),
            text("Applies immediately and is remembered.")
                .size(11)
                .color(AppColors::text_muted()),
            Space::with_height(8),
            row![mode_btn("Dark", true), mode_btn("Light", false)].spacing(6),
        ]
        .spacing(2);

        let config_path = AppConfig::config_path();
        let cache_dir = AppConfig::cache_dir();
        let storage_section = column![
            text("Storage").size(16).color(AppColors::text_primary()),
            Space::with_height(4),
            text(
                "Where winrmpc keeps your settings and its cache. Both paths \
                 can be overridden with the WINRMPC_CONFIG_DIR and \
                 WINRMPC_CACHE_DIR environment variables."
            )
            .size(11)
            .color(AppColors::text_muted()),
            Space::with_height(8),
            storage_row(
                "Settings",
                config_path.as_ref().map(|p| p.display().to_string()),
                config_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|d| d.display().to_string()),
                None,
            ),
            Space::with_height(8),
            storage_row(
                "Cache",
                cache_dir
                    .as_ref()
                    .map(|d| d.join("winrmpc.redb").display().to_string()),
                cache_dir.as_ref().map(|d| d.display().to_string()),
                (!self.store.is_persistent()).then(|| {
                    "running in memory only — nothing cached will survive this session"
                        .to_string()
                }),
            ),
        ]
        .spacing(2);

        let content = column![
            row![
                text("Settings").size(24).color(AppColors::text_primary()),
                Space::with_width(Length::Fill),
                conn_badge,
            ]
            .align_y(iced::Alignment::Center),
            error_text,
            Space::with_height(20),
            text("Servers").size(16).color(AppColors::text_primary()),
            Space::with_height(8),
            server_list,
            Space::with_height(12),
            add_form,
            Space::with_height(24),
            cache_section,
            Space::with_height(24),
            appearance_section,
            Space::with_height(24),
            storage_section,
        ]
        .spacing(4)
        .padding(20)
        .max_width(600);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(is_error: bool, age_secs: u64) -> Toast {
        Toast {
            text: "x".into(),
            is_error,
            shown_at: std::time::Instant::now()
                - std::time::Duration::from_secs(age_secs),
        }
    }

    /// The field this replaced was called `last_error` but also carried
    /// "Database update started", so the two were indistinguishable. They now
    /// differ in styling *and* in how long they stay up.
    #[test]
    fn errors_linger_longer_than_info() {
        assert!(toast(false, TOAST_INFO_SECS).expired());
        assert!(
            !toast(true, TOAST_INFO_SECS).expired(),
            "an error must outlive the info timeout — it is the one worth reading"
        );
        assert!(toast(true, TOAST_ERROR_SECS).expired());
    }

    #[test]
    fn a_fresh_toast_has_not_expired() {
        assert!(!toast(false, 0).expired());
        assert!(!toast(true, 0).expired());
    }
}
