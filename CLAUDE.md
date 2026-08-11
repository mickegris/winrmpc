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
  - `config/settings.rs` — TOML shapes: the README's multi-server example, a legacy single-server file, a minimal config, a partial `[theme]` table, and a `save()`→`load()` round trip. These exist because every legacy field is `#[serde(default)]` *by necessity*: `AppConfig::load()` falls back to `Self::default()` on any parse error and then overwrites the file on the next `save()`, so a config shape that fails to deserialize silently destroys the user's settings rather than reporting anything.
  - `live_tests.rs` — integration tests against a **real** MPD/Snapcast server, all `#[ignore]`d so `cargo test` stays offline. Run with `WINRMPC_TEST_MPD=host:6600 WINRMPC_TEST_SNAPCAST=host:1705 cargo test -- --ignored --test-threads=1`. They cover `add_all`'s bulk enqueue and its stop-at-first-failure semantics, `group_albums_by_artist` against a real album list, and Snapcast `Server.GetStatus` decoding. Anything that mutates state creates a throwaway MPD **partition**, works there, and deletes it — the default partition's queue is never touched. Two Snapcast tests in this file are *not* ignored: they drive the client against a local `TcpListener` mock to prove a dead socket is dropped (so the view can reconnect) while an RPC error response is not.
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
- `mdns-sd` — pure-Rust mDNS/Zeroconf, used only for LAN MPD server discovery (`src/discovery/`); no OS-level Bonjour dependency, so it works the same on Windows/Linux/macOS

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
  discovery/
    mod.rs                   mDNS LAN server discovery (_mpd._tcp.local.); iced Stream via mdns-sd
  snapcast/
    mod.rs                   Re-exports SnapcastClient, SnapClient, SnapGroup, SnapStream
    protocol.rs              Raw TCP, newline-delimited JSON-RPC 2.0, request/notification split
    client.rs                SnapcastClient — get_status, set_volume, set_group_mute, set_group_stream
    types.rs                 SnapClient/SnapGroup/SnapStream + decode_snap_groups/decode_snap_streams
    error.rs                 SnapcastError enum
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
      server_stats.rs
      outputs.rs
      partitions.rs
      snapcast.rs
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
- **Multi-server** (v0.4.0): `servers: Vec<MpdServer>` + `default_server: Option<String>` (name). `MpdServer { name, host, port, password, default_partition, snapcast_host, snapcast_port }` — partition is **per-server** (partitions live on one MPD instance). Helpers: `server(name)`, `server_mut(name)`, `server_addr(name)` (falls back to first server, then legacy `mpd_addr()`); `MpdServer::snapcast_addr()` falls back to the MPD `host` and port `1705` when `snapcast_host`/`snapcast_port` are unset (the common deployment: Snapcast colocated with MPD). There's no Settings UI yet for editing `snapcast_host`/`snapcast_port` directly — only the config fields and the fallback exist; a per-server form field is a deliberate follow-up, not an oversight.
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
- Key methods: `status()`, `current_song()`, `queue()`, `add()`, `add_id()`, `find()`, `find_add()`, `lsinfo()`, `tag_art()`/`cover_file_art()`, `switch_partition()`, etc.

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
`PlayAlbum(Vec<String>)`/`QueueAlbum(Vec<String>)` take song URIs — the already-loaded, variant-expanded, disc-sorted track list from the album detail page — not a re-query by tag name. A tag-exact `find_add("Album", &base_name)` would find nothing on a multi-disc album, since no single track's literal `Album` tag equals the collapsed base name. Both use `MpdClient::add_all(uris)`, which sends the whole batch as one MPD `command_list` round trip rather than one `add` per track — the per-track-loop version starved the shared connection mutex (and the 500ms status poll that also needs it) for the entire "Play All" duration, the same anti-pattern mikMPD's own `CLAUDE.md` documents having to fix ("Bulk enqueue is server-side"). `command_list` stops at the first failing command; tracks already applied before that point stay queued, and nothing after it is attempted. Because that means a bad URI silently yields a *partial* album, neither call site swallows the error with `.ok()` — both log a "queued a partial album" warning (visible in the Log view), and `add_all` itself logs the underlying MPD error with a structured `duration_ms` field so a slow bulk enqueue still trips `LogEntry::is_slow()`'s ⚠. `PlayAlbum` clears the queue first; `QueueAlbum` appends and starts playing if stopped.

### Album identity: artist-aware grouping + multi-disc collapsing (`src/mpd/types.rs`)
`AlbumGroup { artist, base, variants }` is the unit the Albums list, Artist detail, and Recently Added all render (`views::albums_list::view`/`views::artist::view` take `&[AlbumGroup]`, not `&[String]`) — this is what lets two artists' same-named albums stay as separate rows while `"X [Disc 1]"`/`"X [Disc 2]"` collapse into one "2 discs" row.
- **`MpdClient::list_albums_by_artist()`** sends `list Album group AlbumArtist` (MPD 0.21+; falls back to a flat, artist-less list on ACK from older servers). `group_albums_by_artist(pairs)` groups the resulting `(artist, album)` pairs, keyed `(artist.to_lowercase(), base)` — `base` comes from `album_base_and_disc`.
- **`album_base_and_disc(album) -> (String, Option<u32>)`** strips a trailing disc marker — bracketed (`"X [Disc 1]"`, `"X (CD 2)"`) or bare-trailing with a required delimiter (`"X - Disc 1"`, `"XCD2"` but never `"ABCD2"`). Passes through unchanged when there's no marker, the "marker" has no digits (`"Live CD"`), or stripping would leave an empty base (`"Disc 1"` alone).
- **`Song::art_key()`** runs the album through `album_base_and_disc(...).0` first, so every disc of a set shares one art-cache entry/fetch. **`Song::effective_disc()`** prefers the `disc` tag (handles `"2"` and `"2/2"`) over the name-suffix-derived disc, defaulting to `1` — album detail sorts tracks by `(effective_disc, track)` instead of track alone, fixing interleaving on `disc`-tagged-but-unsuffixed multi-disc albums.
- **`View::AlbumDetail(name, artist: Option<String>)`** — when `artist` is `Some`, `AlbumSelected`'s song-loader re-derives this artist's `AlbumGroup`s (a fresh `list_tag_filtered("Album", "AlbumArtist", artist)` + `group_albums_by_artist`), finds the group matching `base`, and fetches+concatenates **every** variant via `MpdClient::find_album_by_artist` — this is the multi-disc expansion. `artist: None` (only from Genre detail, which isn't artist-grouped) falls back to a plain `find("Album", base)`, matching mikMPD's "skip sibling merging when the artist is unknown" safety rule.
- **`album_disc_count(variant_count, max_tag_disc)`** takes the max of both signals — name-suffix variants and the highest `effective_disc` seen — since a properly tagged multi-disc album has no name variants and a poorly tagged one has no disc tag; list-view "N discs" captions currently use `variants.len()` alone (cheap, no song fetch), while the full two-signal max is available once `AlbumDetail`'s songs are loaded.
- **`album_scoped_key(artist, album)`** is the single key shape for the `App`-level `album_songs`/`album_bios` maps (and the redb `bios` table) — `format!("{artist_or_empty}\x1falbum")`, **not** disc-folded (its input is already the collapsed base name). `View::AlbumDetail(name, artist)` and `fetch_album_bio` both build their key through it, so storage and render-time lookup always agree; two different artists' same-titled album no longer collide (previously the in-memory guard was keyed by album name alone). `fetch_album_bio` derives **both** the key and the MusicBrainz query from the same `Option<String>` artist and nothing else — there is deliberately **no** `self.selected_artist` fallback for the `artist: None` case (Genre detail). That fallback used to exist and was removed: the key is `"\x1f{album}"` either way, so it wrote a bio fetched for whatever artist page happened to be open into a slot shared by every artist-less album of that title, persisted it to redb, and read it back for unrelated albums. Querying with an empty artist instead degrades to a title-only Wikipedia lookup (still guarded by `title_matches`) — fewer bios found, but never one attributed to the wrong album.
- **`art_key_for(artist, album)`** is the single builder for every art-cache key (`Song::art_key()` is a thin wrapper over it) — every site that fetches, stores, or looks up cached art goes through it, including the four "recently played" sites (`fetch_recent_art`, `now_playing.rs`'s recents strip/filter, `recently_played.rs`'s album tiles) that used to build the key from the raw (un-disc-stripped) tag and silently miss the cache `Song::art_key()` populates.
- `parse_bare_trailing_marker` (backing `album_base_and_disc`) searches via `rfind_ascii_ci`, a length-preserving ASCII-only case-insensitive search over the *original* string — not `s.to_lowercase()` — because `to_lowercase()` isn't byte-length-preserving for some non-ASCII input (Turkish `İ`), which previously could panic or mis-slice when reused to index into `s`.
- **Not yet done** (scoped out of the initial pass, see `docs/plans/library-album-identity-and-multidisc.md`): grid view (Part C) and Search sections/batch-select (Part D). Also not done, deliberately deferred as a separate follow-up: punctuation-folding the grouping key (mikMPD folds en/em-dash and smart quotes so two rips of one album with a differently-encoded dash don't split into two rows — winrmpc doesn't yet; see `docs/plans/review-fixes-correctness.md` §6) and de-duping `recent_albums` on the disc-stripped base (§1's "also worth folding in" note).

### Queue editing
`QueueRemove(id)` uses `delete_id` (song id, not position — stable across concurrent queue mutations). `QueueMoveUp`/`QueueMoveDown(pos)` wrap `move_pos(from, to)`; MPD's `move FROM TO` leaves the song at position `TO` in the *final* list (remove-then-insert semantics), so `move(pos, pos-1)`/`move(pos, pos+1)` are simple adjacent swaps with no off-by-one. `QueueAddNext(uri)` composes this: `add_id` appends to the end, then `move_pos(end, current_song_pos + 1)` relocates it to play right after the current track; if nothing is playing (`song_pos` is `None`), it falls back to `play_id` on the newly added song instead of trying to insert "next" of nothing.

### Now Playing quick controls
Crossfade (`Status.crossfade`, already parsed) and replay gain mode (`MpdClient::replay_gain_status`/`set_replay_gain_mode`, sends `replaygain_mode <mode>`) live in `now_playing.rs`'s top `toggle_row`, alongside `Outputs`/`Partitions` quick-nav links (plain `NavigateTo`, no new state). `replay_gain_mode` is fetched once in `fetch_all()` on connect (not polled — it rarely changes and isn't part of `status`), stored on `App`, and optimistically updated in the `SetReplayGainMode` handler before the command round-trips.

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
Single `winrmpc.redb` file under the platform cache dir. Tables: `art` (blobs), `art_meta` (`ArtMeta { size, last_access, is_empty }`), `lyrics` (serde_json `Option<Lyrics>`), `bios` (serde_json `Option<String>`, keyed `"artist:{name}"` / `"{artist}\x1falbum"` — see "Wikipedia / MusicBrainz" below), `mb_ids` (serde_json `Option<String>` MBIDs, same key shape as `bios`), `recently_played` (serde_json `Vec<RecentlyPlayedEntry>`, keyed by server name — see "Recently Added / Recently Played" below; app-generated state, not a cache of re-fetchable data, but stored here rather than `AppConfig`/TOML for the same write-amplification reason art/lyrics moved out of flat files: frequent small writes need a targeted key update, not a whole-file rewrite), `meta` (migration markers). `bios`/`mb_ids`/`lyrics` all share the same three-state convention: absent key = never fetched, `Some(None)` = fetched, confirmed nothing found, `Some(Some(v))` = have a value.
- **redb is synchronous** — every `Store` method must be called inside `spawn_blocking`; never hold a transaction across `.await`. Writes commit (fsync) immediately — there is **no flush-on-close**; a crash after a fetch loses nothing.
- `ArtCache` (`art/cache.rs`) is an in-memory `HashMap` hot layer over `Store`; `Store` is the source of truth. `store()` downscales to a 500px JPEG before persisting.
- **LRU eviction**: `art_put` calls `art_evict(limit_bytes)` — removes oldest `last_access` entries until under `art_cache_size_mb`. Negative entries (`is_empty`, no blob) are exempt.
- **Negative caching**: `art_put_empty` / `store_empty` records "known missing" so art isn't refetched every launch. `art_known` / `is_known` gate whether to fetch. Lyrics use `Some(None)` for "cached: no lyrics exist".
- **Startup housekeeping** (`open`): recreates tables; `purge_poisoned_negatives` (one-time, `neg_purge_v1` marker) clears stale negatives that blocked embedded-art lookups; `cleanup_legacy` deletes the pre-DB flat `*.jpg` + `lyrics/` caches. On DB-open failure it wipes+rebuilds, falling back to an in-memory backend so the app still runs.

## Server switching (`src/ui/app.rs`, `src/ui/message.rs`)
`active_server: String` tracks the current server by name. `SwitchServer(name)` rebuilds `MpdClient`, sets `connected = false`, emits `Connect`, and restores that server's `default_partition` on `Connected`. It also reloads `recently_played` history for the new server (`recently_played_get`, via `spawn_blocking`) and clears the in-memory list first so a slow load can't briefly show the old server's history. `SetDefaultServer` / `AddServer` / `RemoveServer` manage the list from the Settings view; `RemoveServer` also deletes the removed server's `recently_played` key. Startup connects to `default_server`.

## LAN Server Discovery (`src/discovery/mod.rs`, Settings view)
`discovery::discover()` returns an `impl Stream<Item = DiscoveredServer>` (built with `iced::stream::channel`, wrapped into a `Subscription` via `Subscription::run_with_id("server-discovery", ...)`) that browses `_mpd._tcp.local.` via `mdns_sd::ServiceDaemon` for a fixed 10s window, then stops itself (`stop_browse`/`shutdown`) — re-scan is the "Rescan" button in Settings' new "Nearby Servers" section, not automatic. `Message::StartDiscovery` sets `discovery_scanning = true` (which is what puts the subscription in the batch) and fires a companion `Task::perform(sleep(11s), |_| Message::DiscoveryFinished)` — 1s past the stream's own deadline — to flip it back off; the stream has no separate "I'm done" event of its own. `Message::ServerDiscovered` de-dupes by name. Clicking a discovered row (`UseDiscoveredServer`) **pre-fills** the manual add-server form fields (`settings_server_name`/`settings_host`/`settings_port`) — it never silently saves a profile; password stays manual, matching mikMPD's exact behavior. `instance_name_from_fullname` (pure, tested) strips the `._mpd._tcp.local.` suffix from the raw mDNS fullname.

## Snapcast Multiroom Control (`src/snapcast/`, `src/ui/views/snapcast.rs`)
Sibling module to `mpd/`, not bolted onto `MpdClient` — Snapcast is a fully independent JSON-RPC-2.0-over-raw-TCP connection (port 1705 by default) that may be absent/unreachable while MPD is fine. `SnapcastConnection::request()` (`protocol.rs`) sends one line, then reads lines until one carries the matching `"id"`, silently skipping anything else (interleaved push notifications, or a stale response) — this app doesn't consume notifications, polling only (`Message::SnapcastPollTick` every 2s, subscribed only while `View::Snapcast` is the active view). `SnapClient`/`SnapGroup`/`SnapStream` (`types.rs`) are decoded straight from `Server.GetStatus`'s `result.server` JSON via `serde_json` (no hand-rolled parser needed — Snapcast's wire format is already JSON, unlike MPD's).
- **Connection lifecycle**: `self.snapcast_client: Option<SnapcastClient>` is created lazily on `View::Snapcast` entry (`on_view_enter`, now `&mut self`) and reused across later visits — `on_view_enter` only rebuilds it when there's none yet or `SnapcastClient::addr()` no longer matches the server's current `snapcast_addr()` (host/port edited in Settings), and only calls `SnapcastClient::connect()` when `is_connected()` is false, so a working connection isn't torn down and reopened on every single visit. `is_connected()` is only bookkeeping (`Option::is_some()`), **not** a socket probe — what keeps it honest is that `SnapcastClient::request()` sets the connection back to `None` whenever a call fails with `Connection`/`Io`/`Json` (a dead socket: snapserver restarted, LAN blip, resume from sleep), so the next poll or view entry reconnects. `Rpc` errors deliberately don't drop the connection — a well-formed JSON-RPC error response proves the socket is fine. Without that reset the `if !is_connected()` guard would never fire again after the first drop and the view would stay stuck erroring until an app restart. `Message::SwitchServer` clears `snapcast_client`/`snapcast_groups`/`snapcast_streams`/`snapcast_error` outright — otherwise a client built against server A would silently keep controlling A's Snapcast instance after switching to server B, since nothing else would notice the server changed while that view is closed.
- **Controls** (`Client.SetVolume`, `Group.SetMute`, `Group.SetStream`): each handler optimistically mutates `self.snapcast_groups` in place first (so the UI reflects the change immediately, e.g. a dragged slider), then fires the RPC. No drag-lock against the 2s poll — the existing MPD volume slider (`player_bar.rs`) doesn't have one either, so this matches the codebase's established pattern rather than adding new complexity; the only failure mode is a rare mid-drag poll overwrite, acceptable for a v1.
- **Not implemented** (explicitly deferred in the plan's own phasing): notification-driven live updates, client rename/latency editing, moving clients between groups, deleting disconnected clients, and a Settings UI for editing `snapcast_host`/`snapcast_port` per server (the fields exist and default to "same host as MPD, port 1705").

## Recently Added / Recently Played (`src/mpd/types.rs`, `src/ui/views/{albums_list,recently_played}.rs`)
Two distinct features sharing one plan (`docs/plans/recently-added-and-played-history.md`) because both extend history-adjacent state — **not to be confused with `RecentAlbum`/`recent_albums`**, the pre-existing 8-item "what's been playing this session" strip shown inline in Now Playing, which is untouched by this section.
- **Recently Added** (`View::RecentlyAdded`, sidebar beneath Genres): `MpdClient::find_recently_added(since, limit)` sends `find "(modified-since '…')" window 0:limit` — always bounded (an unbounded `modified-since` scan can outrun the socket read on a large library). `on_view_enter` computes `since` as `now - 30 days`. Songs are sorted newest-`last_modified`-first and collapsed to unique album names, rendered through the same `views::albums_list::view` used for the plain Albums list (now takes a `title: &str` param so it can say "Recently Added" instead of "Albums").
- **Recently Played** (`View::RecentlyPlayed`, opened via a "🕐 History" link in Now Playing's toggle row, not the sidebar): per-server track-level history, distinct from `recent_albums`. `PlayRecorder` (`types.rs`) is a self-contained tick-based reducer — call `tick(file, is_playing, elapsed_secs, duration_secs)` on every `StatusUpdated`; it tracks its own last-seen elapsed internally (no caller-side delta bookkeeping needed) and returns `true` the moment a play should commit: `accumulated >= min(30, max(5, duration/2))` seconds of actual playback, capping any single delta at 5s so a seek or coarse poll gap can't fast-forward the threshold. A commit pushes a `RecentlyPlayedEntry` to `self.recently_played`, prunes to 30 days / 100 entries (`prune_recently_played`), and persists via `spawn_blocking`. **CD tracks are skipped** (no recording at all while `cdda://` is playing); radio streams are recorded (unlike `recent_albums`, which effectively excludes them via its "Unknown Album" filter). The view's Albums mode **derives** groups from track history (`recently_played_albums` — first occurrence per (artist, album) wins, since entries are already newest-first) rather than recording albums separately, so there's one source of truth. `RecentlyPlayedEntry` carries **both** artists: `artist` (`display_artist()`, the track artist, shown per row) and `album_artist` (`display_album_artist()`, `#[serde(default)]` for pre-existing history). `RecentlyPlayedEntry::art_artist()` returns `album_artist` when set and falls back to `artist`, and it's what `recently_played_albums` groups and labels by — art is only ever cached under `Song::art_key()`, i.e. the *album* artist, so grouping by the track artist both split compilations into one tile per guest artist and made every one of those tiles miss the art cache.

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

## Album Art Fetch Order (`src/mpd/client.rs`, `App::fetch_art`)
**Tag → cover file → internet.** `MpdClient::tag_art(uri)` (`readpicture`) is tried before `cover_file_art(uri)` (`albumart`) — tag art is probed first because on a tagged library it's the one that actually exists; probing the separate-cover-file path first would cost a wasted round trip per album on every well-tagged library. Both share one private `fetch_binary_art(verb, uri)` chunked-read loop. `App::fetch_art` sequences them, falling through to `MusicBrainzClient::fetch_album_art` only if both MPD paths miss.
- **`art_fetch_gate: Arc<Semaphore>`** (size 4, on `App`) bounds peak concurrent art fetches — acquired in `fetch_art`/`fetch_artist_art`/the `ArtistAlbumsLoaded` per-album loop before touching MPD or MusicBrainz. Without it, opening an artist with many uncached albums fires one unbounded fetch task per album.

## Wikipedia / MusicBrainz (`src/art/musicbrainz.rs`)
- **`MusicBrainzClient`** is `Clone` and holds a `Store` (for MBID/negative caching) plus a `MusicBrainzThrottle`; `App` constructs **one** instance (`self.mb_client`) and every fetch site clones it — do not `MusicBrainzClient::new(...)` ad hoc, it defeats connection pooling.
- **`MusicBrainzThrottle`**: an `Arc<Mutex<Option<Instant>>>`-backed rate limiter serializing MusicBrainz calls to ~1 req/s **globally** across every clone/task, not per-call-chain like a local `sleep` would. Replaces the old per-method `sleep(1100ms)`.
- **MusicBrainz ID caching**: `search_artist`/`search_release_group` check the redb `mb_ids` table (`"artist:{name}"` / `"{artist}\x1falbum"` → MBID, `Some(None)` = confirmed no match) before hitting the network — both the art path and the bio path resolve the same entity's MBID, so this cache removes a duplicate search per artist/album visit.
- **Artist bio**: 1) MusicBrainz Wikipedia URL relation; 2) suffix fallback `["(band)", "(musician)", …]`; 3) Wikipedia's own search API (`search_wikipedia`) as a last resort.
- **Album bio**: same three-step shape, keyed on the release-group. Album lookup titles go through `strip_edition_qualifier` first (strips a trailing `[24-bit Remaster]`/`(Deluxe Edition)`-style bracket) — lookup-only, never touches the art cache key.
- **`try_bio_candidate(title, target)`**: fetches the summary, then accepts it if `title_matches(title, target)` (word-token overlap ≥2/3, mirrors mikMPD's `titleTokensMatch`) wins immediately, else falls back to the weaker `is_music_article(text, name)` keyword check. Applied uniformly at every candidate stage (MB canonical link, suffix guesses, search fallback).
- **Bios persist** in the redb `bios` table (`App::fetch_artist_bio`/`fetch_album_bio`, mirroring `fetch_lyrics`'s cache-then-network shape exactly) — `artist_bios`/`album_bios` in-memory maps are `HashMap<String, Option<String>>` (tri-state, like `lyrics`), so a confirmed "no bio" is remembered too, not just a hit.

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
- **Timed MPD commands**: `MpdClient::cmd()` measures each command's elapsed time and emits it as a structured `duration_ms` tracing field (captured into `LogEntry.duration_ms`) plus in the human-readable message. Commands ≥ `logger::SLOW_COMMAND_MS` (2000ms) are logged even if they're normally in the "quiet"/high-frequency list (`status`/`currentsong`/etc.) — a hung poll is exactly the case worth surfacing despite the quiet rule. `LogEntry::is_slow()` drives a `⚠` prefix + warning color in the Log view.

## Server Statistics (`src/ui/views/server_stats.rs`)
`View::ServerStats` (sidebar "Stats", beneath Log) renders `MpdClient::stats()` (song/album/artist counts, uptime, playtime, last DB update) and hosts the **Update Database** trigger — moved here from Settings, mirroring how `cd_device` was already moved out of Settings into the CD view. `on_view_enter` fetches `stats()`; while `View::ServerStats` is the active view, `Tick` also re-fetches `stats()` (piggybacking on the existing 500ms poll rather than a dedicated timer) so the figures refresh live once a triggered scan finishes. The Update button is disabled while `Status.updating_db` is `Some` (MPD's own "a scan is running" signal — no client-side flag needed).

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
