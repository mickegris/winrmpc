# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview
MPD (Music Player Daemon) client built in **Rust** with **Iced 0.13** (GUI) and **Tokio** (async runtime). Connects to a remote MPD server over TCP; the MPD server itself typically runs on Linux. Primary dev/build target is Windows desktop (`winrmpc` = **win**dows **r**ust **mpc**), but the codebase is written against cross-platform crates and also builds on Linux/macOS — only icon embedding and console-hiding are Windows-specific, and those are cleanly guarded behind `cfg(target_os = "windows")`.

## Commands
Run all of these from the repo root (`C:\Users\mikae\winrmpc`), **not** from `src/`.

```powershell
cargo check              # fast type-check — primary feedback loop
cargo build              # dev build (fast compile, slow runtime)
cargo build --release    # optimized build → target\release\winrmpc.exe (embeds icon via build.rs)
cargo run                # build + launch the GUI
cargo test               # run the test suite
cargo test <name>        # run tests whose name matches <name>
cargo test <mod>::tests::<fn> -- --exact   # run one specific test
```

- **Tests**: inline `#[cfg(test)] mod tests` blocks (this is a *binary* crate — a top-level `tests/` dir can't reach internal/private items like `escape` and `parse_ack`). 50 tests cover the pure-logic core, all I/O-free:
  - `mpd/client.rs` — `escape` injection safety (quotes, backslashes, ordering)
  - `mpd/protocol.rs` — `pairs_to_map`, `split_groups`, `parse_ack`
  - `mpd/commands.rs` — every response parser (`parse_status`/`song`/`songs`/`outputs`/`partitions`/`directory_listing`/`stats`/`tag_list`); note the `Time`→`duration` fallback rule
  - `mpd/types.rs` — `Song` display fallbacks, `format_duration`, `art_key` (0x1f separator, hyphen-collision guard), `display_format` (codec from extension), `push_recent` (dedup/move-to-front/cap-at-8)
  - `lyrics/lrclib.rs` — `parse_lrc` (`[mm:ss.xx]` timestamps, sort-by-time, malformed-skip, empty input, integer seconds)
  - Not yet covered (would need a mock `AsyncRead`/`AsyncWrite`): the `protocol.rs` read loops & EOF guards.
- Icon embedding (`build.rs` → `winres`) needs `rc.exe`/`windres` on PATH; if absent it's skipped with a `cargo:warning`, build still succeeds.

## Tech Stack
- Rust 2021 edition
- `iced 0.13` — Elm-style GUI (Model / Message / Update / View). NB: an attempted 0.14 migration was reverted (see commits `61024b2`/`31c7dba`); stay on 0.13.
- `tokio` — async runtime
- `redb 4` — embedded key-value DB backing the art + lyrics cache (`src/store/mod.rs`)
- `image 0.25` — decode/downscale album art
- `reqwest 0.12` — HTTP for MusicBrainz / Wikipedia / LRCLIB
- `serde` / `serde_json` / `toml` — config + cache serialization
- `directories` — platform config/cache paths
- `fuzzy-matcher` — search ranking; `flume` — channels; `open` — launch URLs; `urlencoding`; `chrono`
- `anyhow`, `thiserror`, `tracing`

## Project Structure

```
src/
  main.rs                    Entry point: windows_subsystem, tracing layers, window icon, launches iced
  logger.rs                  InAppLayer (tracing Layer) + static ring-buffer; get_entries() / clear_entries()
  icon.rs                    Programmatic 32×32 RGBA icon (equalizer bars); make_icon() → iced::window::Icon
  build.rs                   Generates 16×16+32×32 BMP-in-ICO, embeds via winres on Windows
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
  store/
    mod.rs                   redb-backed cache DB (winrmpc.redb): art blobs + LRU meta + lyrics; sync API, call inside spawn_blocking
  art/
    mod.rs                   Re-exports ArtCache, MusicBrainzClient
    cache.rs                 In-memory hot layer (HashMap) over the redb Store; downscales art to 500px JPEG on store
    musicbrainz.rs           MusicBrainz + Wikipedia fetch (artist bio, album bio, cover art)
  lyrics/
    mod.rs                   Re-exports LyricsClient, Lyrics, LyricLine, cache_path
    lrclib.rs                LRCLIB fetch + parse_lrc ([mm:ss.xx] synced lyrics)
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
      log.rs
      outputs.rs
      partitions.rs
      playlists_list.rs
      playlist_detail.rs
      add_to_playlist.rs
      mod.rs
    widgets/
      player_bar.rs          Transport controls bar (play/pause/stop/prev/next, seek, volume)
      sidebar.rs             Navigation sidebar
      art_image.rs           Bytes → iced ImageHandle helper
      link.rs                Clickable hyperlink widget (opens URLs via `open`)
      mod.rs
```

## AppConfig (`src/config/settings.rs`)
Fields saved to TOML via `directories` (Windows: `%APPDATA%\winrmpc\winrmpc\config\config.toml`):
- **Multi-server** (v0.4.0): `servers: Vec<MpdServer>` + `default_server: Option<String>` (name). `MpdServer { name, host, port, password, default_partition }` — partition is **per-server** (partitions live on one MPD instance). Helpers: `server(name)`, `server_mut(name)`, `server_addr(name)` (falls back to first server, then legacy `mpd_addr()`).
- **Legacy single-server fields** kept for back-compat: `mpd_host`, `mpd_port`, `mpd_password`, `default_partition`. Mirror the active server; drop in a future release.
- `art_cache_size_mb: u32` — enforced via LRU eviction in the redb store (not just advisory)
- `theme: ThemeConfig`
- `radio_stations: Vec<RadioStation>` — built-ins + user customs; `ensure_builtin_stations()` re-adds missing built-ins on load
- `cd_device: Option<String>` — e.g. `/dev/sr0`; edited in the **CD view** (moved out of Settings), `#[serde(default)]`
- `recent_albums: Vec<RecentAlbum>` — most-recent-first, capped at 8, `#[serde(default)]`; updated on `CurrentSongUpdated` when the album changes

**Migration** (`AppConfig::load`): if `servers` is empty after deserialize, synthesize `MpdServer { name: "Default", … }` from the legacy fields, set `default_server`, and `save()` once so the file upgrades. All new optional fields must carry `#[serde(default)]` so existing config files still load.

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

### Queue editing
`QueueRemove(id)` uses `delete_id` (song id, not position — stable across concurrent queue mutations). `QueueMoveUp`/`QueueMoveDown(pos)` wrap `move_pos(from, to)`; MPD's `move FROM TO` leaves the song at position `TO` in the *final* list (remove-then-insert semantics), so `move(pos, pos-1)`/`move(pos, pos+1)` are simple adjacent swaps with no off-by-one. `QueueAddNext(uri)` composes this: `add_id` appends to the end, then `move_pos(end, current_song_pos + 1)` relocates it to play right after the current track; if nothing is playing (`song_pos` is `None`), it falls back to `play_id` on the newly added song instead of trying to insert "next" of nothing.

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

## Cache Store (`src/store/mod.rs`)
Single `winrmpc.redb` file under the platform cache dir. Tables: `art` (blobs), `art_meta` (`ArtMeta { size, last_access, is_empty }`), `lyrics` (serde_json `Option<Lyrics>`), `meta` (migration markers).
- **redb is synchronous** — every `Store` method must be called inside `spawn_blocking`; never hold a transaction across `.await`. Writes commit (fsync) immediately — there is **no flush-on-close**; a crash after a fetch loses nothing.
- `ArtCache` (`art/cache.rs`) is an in-memory `HashMap` hot layer over `Store`; `Store` is the source of truth. `store()` downscales to a 500px JPEG before persisting.
- **LRU eviction**: `art_put` calls `art_evict(limit_bytes)` — removes oldest `last_access` entries until under `art_cache_size_mb`. Negative entries (`is_empty`, no blob) are exempt.
- **Negative caching**: `art_put_empty` / `store_empty` records "known missing" so art isn't refetched every launch. `art_known` / `is_known` gate whether to fetch. Lyrics use `Some(None)` for "cached: no lyrics exist".
- **Startup housekeeping** (`open`): recreates tables; `purge_poisoned_negatives` (one-time, `neg_purge_v1` marker) clears stale negatives that blocked embedded-art lookups; `cleanup_legacy` deletes the pre-DB flat `*.jpg` + `lyrics/` caches. On DB-open failure it wipes+rebuilds, falling back to an in-memory backend so the app still runs.

## Server switching (`src/ui/app.rs`, `src/ui/message.rs`)
`active_server: String` tracks the current server by name. `SwitchServer(name)` rebuilds `MpdClient`, sets `connected = false`, emits `Connect`, and restores that server's `default_partition` on `Connected`. `SetDefaultServer` / `AddServer` / `RemoveServer` manage the list from the Settings view; startup connects to `default_server`.

## Stored Playlists (`src/mpd/client.rs`, `src/ui/views/{playlists_list,playlist_detail,add_to_playlist}.rs`)
Mirrors mikMPD's setup. `PlaylistInfo` (`src/mpd/types.rs`) backs `View::Playlists` / `View::PlaylistDetail(name)` / `View::AddToPlaylist`, driven by `on_view_enter` (`View::Playlists` → `list_playlists`).
- `MpdClient` playlist commands: `list_playlists`, `list_playlist(name)` (`listplaylistinfo`; MPD sometimes omits `Pos`, so it's assigned from the record index), `save_playlist`, `delete_playlist`, `load_playlist`, `playlist_add`, `playlist_delete(name, pos)`, `playlist_move(name, from, to)`, `rename_playlist`.
- **Save queue as playlist**: `SaveQueueAsPlaylist` validates the name (`validate_playlist_name`), calls `save_playlist`, then reloads the list via `PlaylistsLoaded`.
- **Add to Playlist picker**: `OpenAddToPlaylist(Vec<String>)` (song URIs) opens `View::AddToPlaylist` from Now Playing, albums, the queue, or search; `AddToPlaylistConfirm(name)` / `AddToNewPlaylist` add and `CloseAddToPlaylist` dismisses.
- Rename is inline in the playlist list row (`StartRenamePlaylist` → `RenamePlaylistInput` → `ConfirmRenamePlaylist`/`CancelRenamePlaylist`), not a separate view.

## Lyrics (`src/lyrics/lrclib.rs`, `src/ui/views/now_playing.rs`)
`LyricsClient` fetches synced/plain lyrics from LRCLIB for the current song; `parse_lrc` parses `[mm:ss.xx]` timestamps. Results cache through the redb `Store` (`lyrics_get`/`lyrics_put`, keyed like art) so they're fetched once per track.
- **State**: `lyrics: HashMap<String, Option<Lyrics>>` on `App` — `None` entry = loading, `Some(None)` = fetched-but-none-found, `Some(Some(l))` = have lyrics. `show_lyrics: bool` (default `true`) toggled by `Message::ToggleLyrics`.
- **Now Playing layout**: when `show_lyrics`, the view splits into a left column (art/info/recents) and a right `FillPortion(2)` lyrics panel; synced lines highlight the one matching `elapsed - LYRIC_SYNC_OFFSET` (0.5s, since LRCLIB timestamps tend to lead slightly) and auto-scroll via `lyrics_scroll_id`.
- `fetch_lyrics(song)` (`app.rs`) skips the request if the key is already in `self.lyrics`.

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

## In-App Log (`src/logger.rs`)
- `InAppLayer` implements `tracing_subscriber::Layer` — appends INFO+ events to a static `Mutex<Vec<LogEntry>>`
- `get_entries()` / `clear_entries()` — called from app.rs
- `App.log_entries` refreshed on every `Tick` (synchronous Mutex read, negligible cost)
- Log view is `View::Log`, sidebar button beneath Settings

## Windowless Startup
`#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]` in `main.rs` hides the console.  
Two tracing layers: `fmt` (stderr, useful in dev) + `InAppLayer` (ring-buffer for the in-app view).

## App Icon
`src/icon.rs` — `make_icon()` generates RGBA pixels at runtime for the iced window icon.  
`build.rs` — generates the same design as 16×16 + 32×32 BMP-in-ICO and embeds it via `winres`.  
Build dependency: `winres = "0.1"` in `[build-dependencies]`.

## Planning Docs (`docs/plans/`)
Design docs written before implementing a feature — read the relevant one before starting related work, and add new ones there for anything non-trivial. `mikmpd-parity-overview.md` tracks the gap between winrmpc and its sibling iOS client [mikMPD](https://github.com/mickegris/mikMPD) (`../mikMPD`), with one linked plan file per gap (queue editing, multi-disc album grouping, recently-added/played history, server stats & diagnostics, Snapcast control, LAN server discovery, Now Playing quick controls). `playlists.md` and `enhancements.md` (playlists, MPD log, lyrics) are earlier plans from this same parity effort — already shipped.

## Current Version
`0.4.1` — see `Cargo.toml`. There are `release` and `ship` skills that automate the release/merge flow — prefer them over doing the steps by hand.
