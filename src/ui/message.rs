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

    // === Status updates ===
    StatusUpdated(Box<Status>),
    CurrentSongUpdated(Option<Box<Song>>),
    QueueUpdated(Vec<Song>),
    OutputsUpdated(Vec<Output>),
    PartitionsUpdated(Vec<Partition>),

    // === Queue ===
    QueuePlay(u32),
    QueueRemove(u32),
    QueueClear,
    QueueShuffle,
    QueueAddUri(String),
    QueueAddAndPlay(String),
    QueueAddOnly(String),

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
    MoveOutput(String),

    // === Partitions ===
    SwitchPartition(String),
    NewPartition(String),
    DeletePartition(String),
    PartitionNameInput(String),

    // === Settings ===
    HostChanged(String),
    PortChanged(String),
    PasswordChanged(String),
    SaveSettings,

    // === Misc ===
    ErrorOccurred(String),
    Tick,
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
    Outputs,
    Partitions,
    Settings,
}

impl Default for View {
    fn default() -> Self {
        View::NowPlaying
    }
}
