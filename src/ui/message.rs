use crate::mpd::types::*;
use crate::snapcast::{SnapGroup, SnapStream};

/// How one background album-art fetch ended.
///
/// The two stages are deliberately separate messages-worth of information.
/// The local stage (MPD `readpicture`/`albumart`) costs single-digit
/// milliseconds; a MusicBrainz lookup costs 1.1–2.2 seconds of *globally
/// serialized* throttle time. Reporting "MPD had nothing" as its own outcome
/// is what lets the queue finish every local cover first and defer the
/// network work, instead of letting one artless album stall the sweep.
#[derive(Debug, Clone)]
pub enum ArtOutcome {
    /// Cover bytes, from either stage.
    Loaded(Vec<u8>),
    /// MPD answered and had no art (or couldn't be reached). MusicBrainz has
    /// *not* been asked yet, and no negative has been persisted.
    MpdMiss,
    /// Final: nothing anywhere, or a persisted negative already said so.
    Missing,
}

#[derive(Debug, Clone)]
pub enum Message {
    // === Connection ===
    Connect,
    Connected(Result<(), String>),
    Disconnected,
    ConnectionTick,

    // === Playback ===
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    SeekTo(f64),
    VolumeChanged(f64),
    ToggleRepeat,
    ToggleRandom,
    ToggleSingle,
    ToggleConsume,
    SetCrossfade(u32),
    SetReplayGainMode(String),
    ReplayGainModeLoaded(String),

    // === Status updates ===
    StatusUpdated(Box<Status>),
    CurrentSongUpdated(Option<Box<Song>>),
    QueueUpdated(Vec<Song>),
    OutputsUpdated(Vec<Output>),
    PartitionsUpdated(Vec<Partition>),

    // === Queue ===
    QueuePlay(u32),
    QueueRemove(u32),
    QueueMoveUp(u32),
    QueueMoveDown(u32),
    QueueAddNext(String),
    QueueClear,
    QueueShuffle,
    QueueAddUri(String),
    QueueAddAndPlay(String),
    QueueAddOnly(String),
    /// Song URIs, in order — the already-loaded/variant-expanded track
    /// list from the album detail page, not a re-query by tag name (which
    /// would miss tracks on a multi-disc album whose base name isn't any
    /// single track's literal Album tag).
    PlayAlbum(Vec<String>),
    QueueAlbum(Vec<String>),

    // === Navigation ===
    NavigateTo(View),
    GoBack,

    // === Library ===
    ArtistsLoaded(Vec<String>),
    AlbumsLoaded(Vec<AlbumGroup>),
    GenresLoaded(Vec<String>),
    ArtistSelected(String),
    /// (album base name, artist) — artist is `Some` whenever the entry
    /// point already knows it (grouped album lists, artist detail, search,
    /// song links), enabling artist-scoped, multi-disc-variant-expanded
    /// track loading. `None` only from artist-less entry points (genre
    /// detail), which fall back to a plain tag-exact `find`.
    AlbumSelected(String, Option<String>),
    GenreSelected(String),
    GenreAlbumsLoaded(String, Vec<String>),
    ArtistAlbumsLoaded(String, Vec<AlbumGroup>),
    AlbumSongsLoaded(String, Vec<Song>),

    // === Recently Added / Recently Played history ===
    RecentlyAddedLoaded(Vec<Song>),
    RecentlyPlayedLoaded(Vec<RecentlyPlayedEntry>),
    ClearRecentlyPlayed,
    ToggleRecentlyPlayedMode,
    /// Switch Albums / Recently Added / Recently Played between the cover
    /// grid and the compact list. Shared by all three; persisted.
    ToggleAlbumGridView,

    // === Editing an existing server (Settings) ===
    /// Opens the inline editor for a server's connection details. Adding a
    /// server was already possible; changing one wasn't, so a moved host
    /// meant delete-and-re-add (losing its saved partition).
    StartEditServer(String),
    EditServerHost(String),
    EditServerPort(String),
    EditServerPassword(String),
    EditServerSnapHost(String),
    EditServerSnapPort(String),
    ConfirmEditServer,
    CancelEditServer,

    // === Cache maintenance (Settings) ===
    ArtCacheSizeChanged(String),
    SaveArtCacheSize,
    /// First press arms the confirmation, second press purges. Clearing costs
    /// a full re-fetch — including a slow MusicBrainz crawl — so it isn't a
    /// single-click action.
    ClearCaches,
    CancelClearCaches,
    CachesCleared,
    /// On-disk art size, for the readout next to the purge button.
    CacheSizeLoaded(u64),
    /// Reveal a storage directory in the platform file manager (Settings →
    /// Storage). Carries the *directory*, never the file — Explorer and
    /// Finder both handle "open this folder" more predictably than "open this
    /// .toml", which would launch a text editor instead.
    OpenStorageFolder(String),

    // === Browser ===
    BrowsePath(String),
    BrowseLoaded(String, Vec<crate::mpd::DirectoryEntry>),
    BrowseAddToQueue(String),

    // === Search ===
    SearchQueryChanged(String),
    SearchSubmit,
    SearchResults(Vec<Song>),
    SearchAddToQueue(String),

    // === Album Art ===
    /// A one-off fetch: the playing track's cover, an artist image, a
    /// recently-played thumb. Not part of the background queue.
    ArtLoaded(String, Option<Vec<u8>>),
    /// One album's cover from the background queue, carrying which stage it
    /// came from so the queue knows what to do next. Keyed by `art_key_for`.
    AlbumArtFetched(String, ArtOutcome),

    // === Wikipedia info ===
    ArtistBioLoaded(String, Option<String>),
    AlbumBioLoaded(String, Option<String>),
    ToggleArtistBio,
    ToggleAlbumBio,

    // === Outputs ===
    ToggleOutput(u32),
    MoveOutput { output_name: String, target_partition: String },

    // === Partitions ===
    SwitchPartition(String),
    NewPartition(String),
    DeletePartition(String),
    PartitionNameInput(String),

    // === Radio ===
    RadioPlay(String),
    RadioAddCustomName(String),
    RadioAddCustomUrl(String),
    RadioAddCustomSubmit,
    RadioRemoveStation(String),

    // === Single-song actions ===
    /// Insert song at end of queue and immediately play it (non-destructive).
    PlaySong(String),

    // === Playlists ===
    PlaylistsLoaded(Vec<PlaylistInfo>),
    PlaylistSelected(String),
    PlaylistSongsLoaded(String, Vec<Song>),
    /// Replace the queue with the playlist and play from the start.
    PlaylistPlay(String),
    /// Append the playlist to the queue (does not set the "playing from" context).
    PlaylistAppend(String),
    /// Replace the queue with the playlist and play the track at `pos`.
    PlaylistPlayAt(String, u32),
    PlaylistDelete(String),
    PlaylistRemoveSong(String, u32),
    PlaylistMoveSongUp(String, u32),
    PlaylistMoveSongDown(String, u32),
    SaveQueueAsPlaylist,
    NewPlaylistNameChanged(String),
    StartRenamePlaylist(String),
    RenamePlaylistInput(String),
    ConfirmRenamePlaylist,
    CancelRenamePlaylist,

    // === Shared "Add to Playlist" picker ===
    OpenAddToPlaylist(Vec<String>),
    AddToPlaylistConfirm(String),
    AddToNewPlaylist,
    CloseAddToPlaylist,

    // === CD ===
    CdProbe,
    /// (uri, optional_duration_secs)
    CdTracksLoaded(Vec<(String, Option<f64>)>),
    CdPlayWhole,
    CdPlayTrack(String),
    CdAddTrack(String),
    CdDeviceChanged(String),

    // === Log ===
    LogClear,
    LogCopyAll,
    LogToggleMpdOnly,

    // === Server Statistics ===
    StatsLoaded(Stats),
    UpdateDatabase,
    DatabaseUpdating(u32),

    // === Lyrics ===
    LyricsLoaded(String, Option<crate::lyrics::Lyrics>),
    ToggleLyrics,

    // === Settings / Servers ===
    HostChanged(String),
    PortChanged(String),
    PasswordChanged(String),
    ServerNameChanged(String),
    SaveSettings,
    SwitchServer(String),
    SetDefaultServer(String),
    AddServer,
    RemoveServer(String),
    StartRename(String),
    RenameInputChanged(String),
    ConfirmRename,
    CancelRename,

    // === CD device ===
    SaveCdDevice,

    // === Snapcast ===
    SnapcastPollTick,
    SnapcastStatusLoaded(Vec<SnapGroup>, Vec<SnapStream>),
    SnapcastUnreachable(String),
    SnapcastSetVolume(String, u8),
    SnapcastToggleClientMute(String, bool),
    SnapcastToggleGroupMute(String, bool),
    /// Show/hide disconnected Snapcast clients (view-local, not persisted).
    SnapcastToggleShowInactive,
    SnapcastSetGroupStream(String, String),


    // === Misc ===
    ErrorOccurred(String),
    Tick,
    RefreshAll,
    Noop,
}

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    NowPlaying,
    Queue,
    Library,
    Artists,
    Albums,
    Genres,
    ArtistDetail(String),
    AlbumDetail(String, Option<String>),
    GenreDetail(String),
    Browser,
    Search,
    Radio,
    CD,
    Outputs,
    Partitions,
    Snapcast,
    Settings,
    Log,
    ServerStats,
    RecentlyAdded,
    RecentlyPlayed,
    Playlists,
    PlaylistDetail(String),
    AddToPlaylist,
}

impl Default for View {
    fn default() -> Self {
        View::NowPlaying
    }
}
