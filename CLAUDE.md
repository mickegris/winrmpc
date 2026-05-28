# winrmpc — Project Guide for Claude

## Overview
Windows MPD (Music Player Daemon) client built in **Rust** with **Iced 0.13** (GUI) and **Tokio** (async runtime). Connects to a remote MPD server over TCP. Target: Windows desktop, MPD server typically on Linux.

## Tech Stack
- Rust 2021 edition
- `iced 0.13` — Elm-style GUI (Model / Message / Update / View)
- `tokio` — async runtime
- `serde` / `toml` — config serialization
- `directories` — platform config/cache paths
- `anyhow`, `tracing`

## Project Structure

```
src/
  main.rs                    Entry point, sets up tracing, launches iced
  config/
    mod.rs                   Re-exports AppConfig
    settings.rs              AppConfig struct + load/save (TOML, platform dirs)
  mpd/
    mod.rs                   Re-exports MpdClient, DirectoryEntry, etc.
    protocol.rs              Raw TCP connection, line reader, binary protocol
    commands.rs              Parse MPD key-value responses into typed structs
    client.rs                High-level async API (one method per MPD command)
    types.rs                 All domain types: Song, Status, Output, Partition, …
    error.rs                 MpdError enum
  art/
    mod.rs                   Re-exports ArtCache, MusicBrainzClient
    cache.rs                 Disk-backed art cache (Arc<Inner>, keyed by string)
    musicbrainz.rs           MusicBrainz + Wikipedia fetch (artist bio, album bio, cover art)
  ui/
    mod.rs
    message.rs               Message enum + View enum
    app.rs                   App struct (state + update + view + subscriptions)
    theme/
      mod.rs
      colors.rs              AppColors constants (BG_PRIMARY, ACCENT, …)
    views/
      now_playing.rs
      queue.rs
      artists_list.rs
      artist.rs
      albums_list.rs
      album.rs
      genres_list.rs
      genre_detail.rs
      browser.rs
      search.rs
      radio.rs
      cd.rs
      outputs.rs
      partitions.rs
      mod.rs
    widgets/
      player_bar.rs          Transport controls bar (play/pause/stop/prev/next, seek, volume)
      sidebar.rs             Navigation sidebar
      art_image.rs           Bytes → iced ImageHandle helper
      mod.rs
```

## AppConfig (`src/config/settings.rs`)
Fields saved to TOML via `directories` (Windows: `%APPDATA%\winrmpc\winrmpc\config\config.toml`):
- `mpd_host`, `mpd_port`, `mpd_password: Option<String>`
- `default_partition: Option<String>` — restored on startup
- `art_cache_size_mb: u32`
- `theme: ThemeConfig`
- `radio_stations: Vec<RadioStation>` — built-ins + user customs
- `cd_device: Option<String>` — e.g. `/dev/sr0`; used for CD lsinfo, `#[serde(default)]`

All new optional fields must carry `#[serde(default)]` so existing config files still load.

## MpdClient (`src/mpd/client.rs`)
- `Arc<Mutex<Option<MpdConnection>>>` — clone-cheap, shared across async tasks
- `fn escape(s: &str)` — **always** use this when interpolating user strings into MPD commands (prevents injection of `"` and `\`)
- Binary protocol via `cmd_binary` (album art chunks)
- Key methods: `status()`, `current_song()`, `queue()`, `add()`, `add_id()`, `find()`, `find_add()`, `lsinfo()`, `album_art()`, `switch_partition()`, etc.

## Iced App Architecture (`src/ui/app.rs`)
### Subscription
- **Connected**: `every(500ms)` → `Message::Tick` → `refresh_status()`
- **Disconnected**: `every(3s)` → `Message::ConnectionTick` → reconnect attempt

### Startup / connection sequence
1. `Message::Connect` → `client.connect()`
2. `Message::Connected(Ok(()))` → restore saved partition → emit `Message::RefreshAll`
   - **Do NOT set `connected = true` here** — delays Tick subscription until after partition switch
3. `Message::RefreshAll` → sets `connected = true` + calls `fetch_all()`

`ConnectionTick` guards with `client.is_connected()` to avoid double-connect during startup.

### Task pattern
```rust
// Fire-and-forget MPD command:
self.mpd_cmd(|c| async move { c.some_command().await })

// Async task returning a message:
Task::perform(async move { ... }, Message::SomeVariant)

// Multiple parallel tasks:
Task::batch([task1, task2, task3])
```

### Art cache key format
`"{artist}\x1f{album}"` — ASCII Unit Separator (0x1f) as separator, never appears in metadata.
Artist images: `"artist:{name}"`.

### Navigation
`view_history: Vec<View>` stack. `NavigateTo` pushes current, `GoBack` pops.
`on_view_enter(view)` fires async loads when entering Artists, Albums, Genres, Browser, Outputs, Partitions.

## Key Domain Rules

### Partition persistence
`SwitchPartition` saves to `config.default_partition` immediately and calls `config.save()`.

### Play All / Queue All (albums)
Uses `find_add("Album", &album_name)` — tag-exact match. `PlayAlbum` clears queue first; `QueueAlbum` appends and starts playing if stopped.

### CD playback
- **Play whole disc**: `add("cdda://")` (no device) or `add("cdda://{device}")` when configured
- **Track probe** (`CdProbe`):
  1. If `config.cd_device` is set: try `lsinfo("cdda://{device}")` → extracts `(file_uri, duration_secs)` from `DirectoryEntry::File` entries
  2. Batch fallback: `status()` → record `queue_length` as `start`, `add` each `cdda:///1`…`cdda:///99` until error, `queue()` → take the slice, `delete_range_from(start)` in one shot. Gets real durations; avoids "Failed to load file" log spam from the old add_id + immediate delete_id pattern (MPD starts a background read on add; deleting before it completes causes the exception).
- `cd_tracks: Vec<(String, Option<f64>)>` — URI + optional duration
- Note: `lsinfo cdda:///` and `lsinfo cdda:///dev/sr0` both fail on this setup. The batch add+delete fallback is the actual working path.
- `MpdClient::delete_range_from(start)` — sends `delete {start}:` (open-ended range).

### Protocol EOF guard (`src/mpd/protocol.rs`)
All three read loops (`read_pairs`, `command_list`, `read_binary`) check `if line.is_empty()` and return `MpdError::Connection("Connection closed unexpectedly")` to prevent infinite hang on server drop.

## Wikipedia / MusicBrainz (`src/art/musicbrainz.rs`)
- **Artist bio**: 1) MusicBrainz Wikipedia URL relation; 2) suffix fallback `["(band)", "(musician)", …]`
- **Album bio**: 1) MusicBrainz release-group Wikipedia URL relation; 2) `"(album)"` fallback
- `is_music_article(text, name)` validates the article is music-related before storing
- Rate-limit: 1 req/s to MusicBrainz API (User-Agent required)

## Common Pitfalls
- **Edit requires prior Read** — always read a file before editing it in a session
- **PowerShell heredocs**: use `@'...'@` not `$(cat <<'EOF'...)` for multiline git commit messages
- **Glob on Windows**: use the directory as `path`, filename as `pattern` — absolute path as pattern returns nothing
- **`cargo check` cwd**: run from `C:\Users\mikae\winrmpc`, not from `src/`
- **`gh` path**: `C:\Program Files\GitHub CLI\gh.exe` (or just `gh` if on PATH)
- **Never amend after hook failure** — create a new commit instead

## Release Process
```
git checkout -b release/vX.Y.Z
# bump version in Cargo.toml
git commit ...
gh pr create ...
gh pr merge ... --merge
git checkout main && git pull
git tag vX.Y.Z && git push origin vX.Y.Z
gh release create vX.Y.Z --title "vX.Y.Z" --notes "..."
```

## Current Version
`0.1.4` — see `Cargo.toml`
