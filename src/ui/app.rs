//! Main iced Application: state, update logic, view composition, subscriptions.

use crate::art::ArtCache;
use crate::config::AppConfig;
use crate::mpd::MpdClient;
use crate::store::Store;
use crate::mpd::types::{push_recent, *};
use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use crate::ui::views;
use crate::ui::widgets;
use iced::widget::{column, container, image::Handle as ImageHandle, row, scrollable};
use iced::{Element, Length, Subscription, Task, Theme};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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
    genre_albums: HashMap<String, Vec<String>>,
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

    // LAN server discovery (Settings view)
    discovered_servers: Vec<crate::discovery::DiscoveredServer>,
    discovery_scanning: bool,

    // Settings UI
    settings_host: String,
    settings_port: String,
    settings_password: String,
    settings_cd_device: String,
    settings_server_name: String,
    active_server: String,
    settings_renaming: Option<String>,
    settings_rename_input: String,

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
    lyrics_scroll_id: scrollable::Id,

    // Errors
    last_error: Option<String>,
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
        let cache_dir = AppConfig::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("./cache"));
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

            discovered_servers: Vec::new(),
            discovery_scanning: false,

            settings_host: String::new(),
            settings_port: String::new(),
            settings_password: String::new(),
            settings_cd_device: config.cd_device.clone().unwrap_or_default(),
            settings_server_name: String::new(),
            active_server: active_server.clone(),
            settings_renaming: None,
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
            lyrics_scroll_id: scrollable::Id::unique(),

            last_error: None,
        };

        (app, Task::perform(async {}, |_| Message::Connect))
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
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

        // LAN discovery: only while a scan is active (started from Settings,
        // self-timed — see Message::StartDiscovery/DiscoveryFinished).
        if self.discovery_scanning {
            subs.push(
                Subscription::run_with_id("server-discovery", crate::discovery::discover())
                    .map(Message::ServerDiscovered),
            );
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
                        self.last_error = Some(e);
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
                        self.config.save().ok();
                    }
                }

                let mut tasks: Vec<Task<Message>> = Vec::new();
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
                        client.add_all(&uris).await.ok();
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
                        client.add_all(&uris).await.ok();
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
                Task::none()
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
                        let mut albums = client
                            .list_tag_filtered("Album", "Genre", &name)
                            .await
                            .unwrap_or_default();
                        albums.sort();
                        (name, albums)
                    },
                    |(name, albums)| Message::GenreAlbumsLoaded(name, albums),
                )
            }
            Message::GenreAlbumsLoaded(genre, albums) => {
                self.genre_albums.insert(genre, albums);
                Task::none()
            }
            Message::ArtistAlbumsLoaded(artist, albums) => {
                let mut tasks = Vec::new();
                for group in &albums {
                    // group.base is already disc-stripped, so this is a
                    // no-op re-strip — kept as art_key_for for consistency
                    // with every other art-cache key site.
                    let key = art_key_for(&group.artist, &group.base);
                    if !self.art_handles.contains_key(&key) {
                        // Use the first disc variant to find a song URI for
                        // MPD art lookup.
                        let c = self.client.clone();
                        let variant = group
                            .variants
                            .first()
                            .cloned()
                            .unwrap_or_else(|| group.base.clone());
                        let base = group.base.clone();
                        let cache = self.art_cache.clone_inner();
                        let art_key = key.clone();
                        let art_artist = artist.clone();
                        let mb = self.mb_client.clone();
                        let gate = self.art_fetch_gate.clone();
                        tasks.push(Task::perform(
                            async move {
                                if let Some(data) = cache.get(&art_key).await {
                                    return (art_key, Some(data));
                                }
                                let _permit = gate.acquire().await;
                                // Try MPD first: tag art, then cover-file art.
                                let songs = c.find("Album", &variant).await.unwrap_or_default();
                                if let Some(first) = songs.first() {
                                    if let Ok(Some(data)) = c.tag_art(&first.file).await {
                                        let _ = cache.store(&art_key, &data).await;
                                        return (art_key, Some(data));
                                    }
                                    if let Ok(Some(data)) = c.cover_file_art(&first.file).await {
                                        let _ = cache.store(&art_key, &data).await;
                                        return (art_key, Some(data));
                                    }
                                }
                                // Fallback to MusicBrainz
                                if let Some(data) = mb.fetch_album_art(&art_artist, &base).await {
                                    let _ = cache.store(&art_key, &data).await;
                                    return (art_key, Some(data));
                                }
                                cache.store_empty(&art_key).await;
                                (art_key, None)
                            },
                            |(key, data)| Message::ArtLoaded(key, data),
                        ));
                    }
                }
                self.artist_albums.insert(artist, albums);
                if tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(tasks)
                }
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
                Task::none()
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
                        self.last_error = Some(
                            "Playlist names must not be empty or contain slashes.".to_string(),
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
                            self.last_error = Some(
                                "Playlist names must not be empty or contain slashes.".to_string(),
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
                        self.last_error = Some(
                            "Playlist names must not be empty or contain slashes.".to_string(),
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
                self.config.save().ok();
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
                    self.config.save().ok();
                    self.radio_add_name.clear();
                    self.radio_add_url.clear();
                }
                Task::none()
            }
            Message::RadioRemoveStation(url) => {
                self.config.remove_radio_station(&url);
                self.config.save().ok();
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
                        self.config.save().ok();
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
            Message::SaveCdDevice => {
                self.config.cd_device = if self.settings_cd_device.trim().is_empty() {
                    None
                } else {
                    Some(self.settings_cd_device.trim().to_string())
                };
                self.config.save().ok();
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

            // =================================================================
            // LAN server discovery
            // =================================================================
            Message::StartDiscovery => {
                self.discovered_servers.clear();
                self.discovery_scanning = true;
                // The discovery stream itself has no "finished" event; stop
                // showing "Searching..." shortly after its own scan window
                // (a small margin so in-flight events aren't cut off).
                Task::perform(
                    tokio::time::sleep(Duration::from_secs(11)),
                    |_| Message::DiscoveryFinished,
                )
            }
            Message::ServerDiscovered(server) => {
                if !self.discovered_servers.iter().any(|s| s.name == server.name) {
                    self.discovered_servers.push(server);
                }
                Task::none()
            }
            Message::DiscoveryFinished => {
                self.discovery_scanning = false;
                Task::none()
            }
            Message::UseDiscoveredServer(server) => {
                self.settings_server_name = server.name;
                self.settings_host = server.host;
                self.settings_port = server.port.to_string();
                Task::none()
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
                self.config.save().ok();
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
                    self.config.save().ok();
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
                self.config.save().ok();
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
                self.last_error = Some("Database update started".to_string());
                self.fetch_stats()
            }

            // =================================================================
            // Tick / Error / Noop
            // =================================================================
            Message::Tick => {
                self.log_entries = crate::logger::get_entries();
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
            Message::ErrorOccurred(e) => {
                tracing::error!("{e}");
                self.last_error = Some(e);
                Task::none()
            }
            Message::Noop => Task::none(),
        }
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
                        let key = format!(
                            "{}\x1f{}\x1f{}",
                            s.display_artist(),
                            s.display_title(),
                            s.display_album()
                        );
                        self.lyrics.get(&key).map(|opt| opt.as_ref())
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
                    self.lyrics_scroll_id.clone(),
                    self.playing_from_playlist.as_deref(),
                    self.replay_gain_mode.as_deref(),
                )
            }
            View::Queue => {
                views::queue::view(&self.queue, self.status.song_pos)
            }
            View::Library => {
                // Redirect to Artists if someone navigates here
                views::artists_list::view(&self.artists)
            }
            View::Artists => {
                views::artists_list::view(&self.artists)
            }
            View::Albums => {
                views::albums_list::view(&self.albums, "Albums")
            }
            View::Genres => {
                views::genres_list::view(&self.genres)
            }
            View::RecentlyAdded => {
                views::albums_list::view(&self.recently_added_albums, "Recently Added")
            }
            View::RecentlyPlayed => views::recently_played::view(
                &self.recently_played,
                self.recently_played_show_albums,
                &self.art_handles,
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
                views::album::view(name, songs, art, bio, self.show_album_bio)
            }
            View::GenreDetail(name) => {
                let albums = self
                    .genre_albums
                    .get(name)
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                views::genre_detail::view(name, albums)
            }
            View::Browser => {
                views::browser::view(&self.browser_path, &self.browser_entries)
            }
            View::Search => {
                views::search::view(&self.search_query, &self.search_results)
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
                views::playlist_detail::view(name, songs, art)
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
            widgets::player_bar::view(&self.status, &self.current_song);

        let content = column![
            row![sidebar, main_content].height(Length::Fill),
            player_bar,
        ];

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(AppColors::BG_PRIMARY.into()),
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

    /// Fetch an album's Wikipedia bio — same shape as `fetch_artist_bio`.
    /// Both the in-memory `album_bios` map and the redb key are scoped by
    /// `album_scoped_key(artist, album)`, matching `View::AlbumDetail`'s own
    /// `(name, artist)` so storage and render-time lookup always agree
    /// (previously the in-memory guard was keyed by album name alone, so
    /// two different artists' same-titled album showed each other's bio
    /// for the rest of the session — see
    /// docs/plans/review-fixes-correctness.md §2). `artist` also drives the
    /// actual MusicBrainz/Wikipedia query; when unknown (e.g. reached from
    /// Genre detail) it falls back to whichever artist page happens to be
    /// open as a best-effort search hint — that fallback is *only* for the
    /// query, never for the cache key, so a stale `selected_artist` can't
    /// cause a wrong-artist cache hit.
    fn fetch_album_bio(&self, artist: Option<String>, album: String) -> Task<Message> {
        let key = album_scoped_key(artist.as_deref(), &album);
        if self.album_bios.contains_key(&key) {
            return Task::none();
        }
        let query_artist = artist
            .clone()
            .or_else(|| self.selected_artist.clone())
            .unwrap_or_default();
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
    let key = format!(
        "{}\x1f{}\x1f{}",
        song.display_artist(),
        song.display_title(),
        song.display_album()
    );
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

/// Keep the highlighted synced-lyric line in view by snapping the lyrics
/// scrollable to a position proportional to the active line. No-op unless
/// lyrics are shown and the current track has synced lyrics.
fn lyrics_autoscroll(&self) -> Task<Message> {
    if !self.show_lyrics {
        return Task::none();
    }
    let Some(song) = &self.current_song else {
        return Task::none();
    };
    let key = format!(
        "{}\x1f{}\x1f{}",
        song.display_artist(),
        song.display_title(),
        song.display_album()
    );
    let Some(Some(lyrics)) = self.lyrics.get(&key) else {
        return Task::none();
    };
    let Some(synced) = &lyrics.synced else {
        return Task::none();
    };
    if synced.len() < 2 {
        return Task::none();
    }
    let elapsed = self.status.elapsed.map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let t = elapsed - crate::ui::views::now_playing::LYRIC_SYNC_OFFSET;
    let active = synced.iter().rposition(|l| l.secs <= t).unwrap_or(0);
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
                .color(AppColors::WARNING)
                .into(),
            None => Space::with_height(0).into(),
        };

        let conn_badge = if self.connected {
            text("Connected").size(13).color(AppColors::SUCCESS)
        } else {
            text("Disconnected").size(13).color(AppColors::ERROR)
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
                AppColors::BG_TERTIARY
            } else {
                AppColors::BG_SECONDARY
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
                    text(&server.name).size(13).color(AppColors::ACCENT).into()
                } else {
                    text(&server.name).size(13).color(AppColors::TEXT_PRIMARY).into()
                };

                let addr_text = text(server.addr()).size(11).color(AppColors::TEXT_MUTED);

                let connect_btn: Element<'_, Message> = if is_active {
                    text("●").size(13).color(AppColors::SUCCESS).into()
                } else {
                    button(text("Connect").size(11))
                        .on_press(Message::SwitchServer(server.name.clone()))
                        .padding([3, 8])
                        .into()
                };

                let default_btn: Element<'_, Message> = if is_default {
                    container(text("Default").size(10).color(AppColors::ACCENT))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme| container::Style {
                            background: None,
                            border: iced::Border {
                                color: AppColors::ACCENT,
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
                                    AppColors::TEXT_PRIMARY
                                }
                                _ => AppColors::TEXT_MUTED,
                            },
                            border: iced::Border {
                                color: AppColors::TEXT_MUTED,
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            shadow: iced::Shadow::default(),
                        })
                        .into()
                };

                let rename_btn: Element<'_, Message> =
                    button(text("Rename").size(10))
                        .on_press(Message::StartRename(server.name.clone()))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme, s: button::Status| button::Style {
                            background: None,
                            text_color: match s {
                                button::Status::Hovered | button::Status::Pressed => {
                                    AppColors::TEXT_PRIMARY
                                }
                                _ => AppColors::TEXT_MUTED,
                            },
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        })
                        .into();

                let remove_btn: Element<'_, Message> = if can_remove {
                    button(text("×").size(13))
                        .on_press(Message::RemoveServer(server.name.clone()))
                        .padding([3, 8])
                        .style(|_t: &iced::Theme, _s: button::Status| button::Style {
                            background: None,
                            text_color: AppColors::TEXT_MUTED,
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
                    rename_btn,
                    remove_btn,
                ]
                .align_y(iced::Alignment::Center)
                .spacing(4)
                .into()
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
            text("Add server").size(14).color(AppColors::TEXT_SECONDARY),
            Space::with_height(6),
            row![
                column![
                    text("Name").size(11).color(AppColors::TEXT_MUTED),
                    text_input("My Server", &self.settings_server_name)
                        .on_input(Message::ServerNameChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(Length::FillPortion(2)),
                Space::with_width(6),
                column![
                    text("Host").size(11).color(AppColors::TEXT_MUTED),
                    text_input("127.0.0.1", &self.settings_host)
                        .on_input(Message::HostChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(Length::FillPortion(3)),
                Space::with_width(6),
                column![
                    text("Port").size(11).color(AppColors::TEXT_MUTED),
                    text_input("6600", &self.settings_port)
                        .on_input(Message::PortChanged)
                        .padding(6)
                        .size(12),
                ]
                .spacing(2)
                .width(70),
                Space::with_width(6),
                column![
                    text("Password").size(11).color(AppColors::TEXT_MUTED),
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

        let nearby_section: Element<'_, Message> = {
            let mut list = column![].spacing(4);
            if self.discovery_scanning && self.discovered_servers.is_empty() {
                list = list.push(text("Searching...").size(12).color(AppColors::TEXT_MUTED));
            } else if self.discovered_servers.is_empty() {
                list = list.push(text("No servers found yet.").size(12).color(AppColors::TEXT_MUTED));
            }
            for server in &self.discovered_servers {
                list = list.push(
                    button(
                        row![
                            text(server.name.clone()).size(13).color(AppColors::TEXT_PRIMARY),
                            Space::with_width(Length::Fill),
                            text(format!("{}:{}", server.host, server.port))
                                .size(11)
                                .color(AppColors::TEXT_MUTED),
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .on_press(Message::UseDiscoveredServer(server.clone()))
                    .padding([6, 10])
                    .width(Length::Fill),
                );
            }

            let scan_label = if self.discovery_scanning { "Searching..." } else { "Rescan" };
            column![
                row![
                    text("Nearby Servers").size(14).color(AppColors::TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    button(text(scan_label).size(12))
                        .on_press_maybe((!self.discovery_scanning).then_some(Message::StartDiscovery))
                        .padding([4, 12]),
                ]
                .align_y(iced::Alignment::Center),
                Space::with_height(6),
                list,
                Space::with_height(4),
                text("Servers appear here if MPD has Zeroconf enabled. Manual entry always works.")
                    .size(10)
                    .color(AppColors::TEXT_MUTED),
            ]
            .spacing(2)
            .into()
        };

        let content = column![
            row![
                text("Settings").size(24).color(AppColors::TEXT_PRIMARY),
                Space::with_width(Length::Fill),
                conn_badge,
            ]
            .align_y(iced::Alignment::Center),
            error_text,
            Space::with_height(20),
            text("Servers").size(16).color(AppColors::TEXT_PRIMARY),
            Space::with_height(8),
            server_list,
            Space::with_height(12),
            nearby_section,
            Space::with_height(12),
            add_form,
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
