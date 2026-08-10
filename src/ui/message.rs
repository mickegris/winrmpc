use crate::mpd::types::*;

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
    PlayAlbum(String),
    QueueAlbum(String),

    // === Navigation ===
    NavigateTo(View),
    GoBack,

    // === Library ===
    ArtistsLoaded(Vec<String>),
    AlbumsLoaded(Vec<String>),
    GenresLoaded(Vec<String>),
    ArtistSelected(String),
    AlbumSelected(String),
    GenreSelected(String),
    GenreAlbumsLoaded(String, Vec<String>),
    ArtistAlbumsLoaded(String, Vec<String>),
    AlbumSongsLoaded(String, Vec<Song>),

    // === Recently Added / Recently Played history ===
    RecentlyAddedLoaded(Vec<Song>),
    RecentlyPlayedLoaded(Vec<RecentlyPlayedEntry>),
    ClearRecentlyPlayed,
    ToggleRecentlyPlayedMode,

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
    ArtLoaded(String, Option<Vec<u8>>),

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
    AlbumDetail(String),
    GenreDetail(String),
    Browser,
    Search,
    Radio,
    CD,
    Outputs,
    Partitions,
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
