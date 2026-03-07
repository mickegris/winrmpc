//! Main iced Application: state, update logic, view composition, subscriptions.

use crate::art::ArtCache;
use crate::config::AppConfig;
use crate::mpd::MpdClient;
use crate::mpd::types::*;
use crate::ui::message::{Message, View};
use crate::ui::theme::AppColors;
use crate::ui::views;
use crate::ui::widgets;
use iced::widget::{column, container, image::Handle as ImageHandle, row};
use iced::{Element, Length, Subscription, Task, Theme};
use std::collections::HashMap;
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
    albums: Vec<String>,
    genres: Vec<String>,
    artist_albums: HashMap<String, Vec<String>>,
    album_songs: HashMap<String, Vec<Song>>,
    selected_artist: Option<String>,
    selected_album: Option<String>,

    // Browser
    browser_path: String,
    browser_entries: Vec<crate::mpd::DirectoryEntry>,

    // Search
    search_query: String,
    search_results: Vec<Song>,

    // Album Art
    art_cache: ArtCache,
    mb_client: crate::art::MusicBrainzClient,
    art_handles: HashMap<String, ImageHandle>,

    // Wikipedia bios
    artist_bios: HashMap<String, String>,
    album_bios: HashMap<String, String>,
    show_artist_bio: bool,
    show_album_bio: bool,

    // Partitions UI
    new_partition_name: String,

    // Settings UI
    settings_host: String,
    settings_port: String,
    settings_password: String,

    // Errors
    last_error: Option<String>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let config = AppConfig::load();
        let client = MpdClient::new(&config.mpd_addr());
        let cache_dir = AppConfig::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("./cache/art"));

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
            album_songs: HashMap::new(),
            selected_artist: None,
            selected_album: None,

            browser_path: String::new(),
            browser_entries: Vec::new(),

            search_query: String::new(),
            search_results: Vec::new(),

            art_cache: ArtCache::new(cache_dir),
            mb_client: crate::art::MusicBrainzClient::new(),
            art_handles: HashMap::new(),

            artist_bios: HashMap::new(),
            album_bios: HashMap::new(),
            show_artist_bio: false,
            show_album_bio: false,

            new_partition_name: String::new(),

            settings_host: config.mpd_host.clone(),
            settings_port: config.mpd_port.to_string(),
            settings_password: config.mpd_password.clone().unwrap_or_default(),

            last_error: None,
        };

        (app, Task::perform(async {}, |_| Message::Connect))
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.connected {
            iced::time::every(Duration::from_millis(500)).map(|_| Message::Tick)
        } else {
            iced::time::every(Duration::from_secs(3)).map(|_| Message::ConnectionTick)
        }
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
                        self.connected = true;
                        self.last_error = None;
                        tracing::info!("Connected to MPD");
                        return self.fetch_all();
                    }
                    Err(e) => {
                        self.connected = false;
                        self.last_error = Some(e);
                    }
                }
                Task::none()
            }
            Message::ConnectionTick => {
                if !self.connected {
                    let client = self.client.clone();
                    Task::perform(
                        async move {
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
            Message::Stop => Task::none(),
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

            // =================================================================
            // Status Updates
            // =================================================================
            Message::StatusUpdated(status) => {
                self.status = *status;
                Task::none()
            }
            Message::CurrentSongUpdated(song) => {
                if let Some(ref s) = song {
                    let key = s.art_key();
                    if !self.art_handles.contains_key(&key) {
                        let task = self.fetch_art(s.file.clone(), key);
                        self.current_song = song.map(|s| *s);
                        return task;
                    }
                }
                self.current_song = song.map(|s| *s);
                Task::none()
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
            Message::QueueClear => {
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
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.add(&uri).await.ok();
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
                        (name, albums)
                    },
                    |(name, albums)| Message::ArtistAlbumsLoaded(name, albums),
                );

                let art_name = self.selected_artist.clone().unwrap_or_default();
                let artist_art_task = self.fetch_artist_art(art_name);

                let bio_name = self.selected_artist.clone().unwrap_or_default();
                let bio_task = if !self.artist_bios.contains_key(&bio_name) {
                    let bn = bio_name.clone();
                    Task::perform(
                        async move {
                            let mb = crate::art::MusicBrainzClient::new();
                            let bio = mb.fetch_artist_bio(&bn).await;
                            (bn, bio)
                        },
                        |(name, bio)| Message::ArtistBioLoaded(name, bio),
                    )
                } else {
                    Task::none()
                };

                self.show_artist_bio = false;
                Task::batch([albums_task, artist_art_task, bio_task])
            }
            Message::AlbumSelected(name) => {
                self.selected_album = Some(name.clone());
                self.view_history.push(self.current_view.clone());
                self.current_view = View::AlbumDetail(name.clone());
                self.show_album_bio = false;

                let client = self.client.clone();
                let album_name = name.clone();
                let songs_task = Task::perform(
                    async move {
                        let songs = client
                            .find("Album", &album_name)
                            .await
                            .unwrap_or_default();
                        (album_name, songs)
                    },
                    |(name, songs)| Message::AlbumSongsLoaded(name, songs),
                );

                let bio_task = if !self.album_bios.contains_key(&name) {
                    let bn = name.clone();
                    // We need artist name for the search - get it from existing data if possible
                    let artist = self.selected_artist.clone().unwrap_or_default();
                    Task::perform(
                        async move {
                            let mb = crate::art::MusicBrainzClient::new();
                            let bio = mb.fetch_album_bio(&artist, &bn).await;
                            (bn, bio)
                        },
                        |(name, bio)| Message::AlbumBioLoaded(name, bio),
                    )
                } else {
                    Task::none()
                };

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
                self.artist_albums.insert(format!("genre:{genre}"), albums);
                Task::none()
            }
            Message::ArtistAlbumsLoaded(artist, albums) => {
                let mut tasks = Vec::new();
                for album in &albums {
                    let key = format!("{artist}-{album}");
                    if !self.art_handles.contains_key(&key) {
                        // Find first song of this album to get URI for MPD art
                        let c = self.client.clone();
                        let a = album.clone();
                        let cache = self.art_cache.clone_inner();
                        let art_key = key.clone();
                        let art_artist = artist.clone();
                        tasks.push(Task::perform(
                            async move {
                                if let Some(data) = cache.get(&art_key).await {
                                    return (art_key, Some(data));
                                }
                                // Try MPD first
                                let songs = c.find("Album", &a).await.unwrap_or_default();
                                if let Some(first) = songs.first() {
                                    if let Ok(Some(data)) = c.album_art(&first.file).await {
                                        let _ = cache.store(&art_key, &data).await;
                                        return (art_key, Some(data));
                                    }
                                }
                                // Fallback to MusicBrainz
                                let mb = crate::art::MusicBrainzClient::new();
                                if let Some(data) = mb.fetch_album_art(&art_artist, &a).await {
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
                if let Some(text) = bio {
                    self.artist_bios.insert(name, text);
                }
                Task::none()
            }
            Message::AlbumBioLoaded(name, bio) => {
                if let Some(text) = bio {
                    self.album_bios.insert(name, text);
                }
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
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.toggle_output(id).await.ok();
                    },
                    |_| Message::Tick,
                )
            }
            Message::MoveOutput(name) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.move_output(&name).await.ok();
                    },
                    |_| Message::Tick,
                )
            }

            // =================================================================
            // Partitions
            // =================================================================
            Message::SwitchPartition(name) => {
                let client = self.client.clone();
                Task::perform(
                    async move {
                        client.switch_partition(&name).await.ok();
                    },
                    |_| Message::Tick,
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
            // Settings
            // =================================================================
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
            Message::SaveSettings => {
                self.config.mpd_host = self.settings_host.clone();
                self.config.mpd_port =
                    self.settings_port.parse().unwrap_or(6600);
                self.config.mpd_password = if self.settings_password.is_empty() {
                    None
                } else {
                    Some(self.settings_password.clone())
                };
                self.config.save().ok();
                self.client = MpdClient::new(&self.config.mpd_addr());
                self.connected = false;
                Task::perform(async {}, |_| Message::Connect)
            }

            // =================================================================
            // Tick / Error / Noop
            // =================================================================
            Message::Tick => {
                if self.connected {
                    self.refresh_status()
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
            &self.config.mpd_addr(),
        );

        let main_content: Element<Message> = match &self.current_view {
            View::NowPlaying => {
                let art_handle = self
                    .current_song
                    .as_ref()
                    .and_then(|s| self.art_handles.get(&s.art_key()));
                views::now_playing::view(
                    &self.current_song,
                    &self.status,
                    art_handle,
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
                views::albums_list::view(&self.albums)
            }
            View::Genres => {
                views::genres_list::view(&self.genres)
            }
            View::ArtistDetail(name) => {
                let albums = self
                    .artist_albums
                    .get(name)
                    .map(|a| a.as_slice())
                    .unwrap_or(&[]);
                let bio = self.artist_bios.get(name).map(|s| s.as_str());
                views::artist::view(
                    name,
                    albums,
                    &self.art_handles,
                    bio,
                    self.show_artist_bio,
                )
            }
            View::AlbumDetail(name) => {
                let songs = self
                    .album_songs
                    .get(name)
                    .map(|s| s.as_slice())
                    .unwrap_or(&[]);
                let art_key = songs
                    .first()
                    .map(|s| s.art_key())
                    .unwrap_or_default();
                let art = self.art_handles.get(&art_key);
                let bio = self.album_bios.get(name).map(|s| s.as_str());
                views::album::view(name, songs, art, bio, self.show_album_bio)
            }
            View::GenreDetail(name) => {
                let albums = self
                    .artist_albums
                    .get(&format!("genre:{name}"))
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
            View::Outputs => views::outputs::view(&self.outputs),
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

        Task::batch([
            status_task,
            song_task,
            queue_task,
            outputs_task,
            partitions_task,
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
        let client = self.client.clone();
        let cache = self.art_cache.clone_inner();
        let mb = crate::art::MusicBrainzClient::new();

        Task::perform(
            async move {
                // Check cache first
                if let Some(data) = cache.get(&key).await {
                    return (key, Some(data));
                }

                // Try MPD embedded art
                if let Ok(Some(data)) = client.album_art(&uri).await {
                    let _ = cache.store(&key, &data).await;
                    return (key, Some(data));
                }

                // Parse artist and album from the key (format: "artist-album")
                let parts: Vec<&str> = key.splitn(2, '-').collect();
                if parts.len() == 2 {
                    let artist = parts[0];
                    let album = parts[1];

                    // Try MusicBrainz / Cover Art Archive
                    if let Some(data) = mb.fetch_album_art(artist, album).await {
                        let _ = cache.store(&key, &data).await;
                        return (key, Some(data));
                    }
                }

                cache.store_empty(&key).await;
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

        Task::perform(
            async move {
                // Check cache
                if let Some(data) = cache.get(&key).await {
                    return (key, Some(data));
                }

                // Fetch from MusicBrainz (uses first album cover as artist image)
                let mb = crate::art::MusicBrainzClient::new();
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

    fn on_view_enter(&self, view: View) -> Task<Message> {
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
                        client.list_tag("Album").await.unwrap_or_default()
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
            _ => Task::none(),
        }
    }

    fn settings_view(&self) -> Element<'_, Message> {
        use iced::widget::{button, column, container, text, text_input, Space};

        let error_text: Element<Message> = match &self.last_error {
            Some(e) => text(format!("Error: {e}"))
                .size(13)
                .color(AppColors::ERROR)
                .into(),
            None => Space::with_height(0).into(),
        };

        let connection_status = if self.connected {
            text("Connected")
                .size(14)
                .color(AppColors::SUCCESS)
        } else {
            text("Disconnected")
                .size(14)
                .color(AppColors::ERROR)
        };

        let content = column![
            text("Settings").size(24).color(AppColors::TEXT_PRIMARY),
            Space::with_height(12),
            connection_status,
            error_text,
            Space::with_height(16),
            text("MPD Host")
                .size(14)
                .color(AppColors::TEXT_SECONDARY),
            text_input("127.0.0.1", &self.settings_host)
                .on_input(Message::HostChanged)
                .padding(8),
            Space::with_height(8),
            text("MPD Port")
                .size(14)
                .color(AppColors::TEXT_SECONDARY),
            text_input("6600", &self.settings_port)
                .on_input(Message::PortChanged)
                .padding(8),
            Space::with_height(8),
            text("Password (optional)")
                .size(14)
                .color(AppColors::TEXT_SECONDARY),
            text_input("", &self.settings_password)
                .on_input(Message::PasswordChanged)
                .padding(8)
                .secure(true),
            Space::with_height(16),
            button(text("Save & Reconnect").size(14))
                .on_press(Message::SaveSettings)
                .padding([8, 20]),
        ]
        .spacing(4)
        .padding(20)
        .max_width(500);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
