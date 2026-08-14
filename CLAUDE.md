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

- **Tests**: inline `#[cfg(test)] mod tests` blocks (this is a *binary* crate — a top-level `tests/` dir can't reach internal/private items like `escape` and `parse_ack`). 185 offline tests; none touch the network or need an MPD server (the `store` ones do open a real redb, over `InMemoryBackend`):
  - `mpd/client.rs` — `escape` injection safety (quotes, backslashes, ordering)
  - `mpd/protocol.rs` — `pairs_to_map`, `split_groups`, `parse_ack`
  - `mpd/commands.rs` — every response parser (`parse_status`/`song`/`songs`/`outputs`/`partitions`/`directory_listing`/`stats`/`tag_list`); note the `Time`→`duration` fallback rule
  - `mpd/types.rs` — `Song` display fallbacks, `format_duration`, `art_key` (0x1f separator, hyphen-collision guard), `display_format` (codec from extension), `push_recent` (dedup/move-to-front/cap-at-8)
  - `lyrics/lrclib.rs` — `parse_lrc` (`[mm:ss.xx]` timestamps, sort-by-time, malformed-skip, empty input, integer seconds)
  - `art/musicbrainz.rs` — `title_matches` acceptance rules (stopwords, 2/3 token overlap, single-token exactness), `lucene_escape`, `normalize_for_lookup` (smart punctuation, sort-order articles, multi-byte tails), `strip_edition_qualifier`/`is_edition_qualifier`, `lookup_title`, `release_title_matches`/`artist_credit_matches`/`search_queries`, and that `MusicBrainzThrottle` serializes across clones
  - `store/mod.rs` — `bios`/`mb_ids`/`recently_played` round trips over an in-memory redb, including the absent-vs-`Some(None)` distinction and per-server key isolation
  - `snapcast/types.rs` + `snapcast/protocol.rs` — `decode_snap_groups`/`decode_snap_streams` from fixtures, degrade-to-empty on unexpected JSON, `extract_result`'s error/null handling, and response-id matching among interleaved notification lines
  - Not yet covered (would need a mock `AsyncRead`/`AsyncWrite`): the `protocol.rs` read loops & EOF guards.
  - `icon.rs` — the window icon builds, corners are transparent / centre is bar-coloured, the committed `packaging/linux/` PNGs still match the generator, and `svg()` emits one circle + one rect per bar
  - `config/settings.rs` — TOML shapes (the README's multi-server example, a legacy single-server file, a minimal config, a partial `[theme]` table, a `save()`→`load()` round trip) plus the three `load_from` cases against a scratch path: a missing file is **created**, an unparseable one is **never overwritten**, a legacy one is **migrated and persisted**. Every legacy field is `#[serde(default)]` *by necessity* — a shape that fails to deserialize used to cost the user their settings, since `load` fell back to defaults and the next `save` wrote them over the file. `load_failed` now disarms `save` instead, but the defaults are still what keeps a valid-but-old file parsing at all.
  - `live_tests.rs` — integration tests against a **real** MPD/Snapcast server, all `#[ignore]`d so `cargo test` stays offline. Run with `WINRMPC_TEST_MPD=host:6600 WINRMPC_TEST_SNAPCAST=host:1705 cargo test -- --ignored --test-threads=1`. They cover `add_all`'s bulk enqueue and its stop-at-first-failure semantics, `group_albums_by_artist` against a real album list, and Snapcast `Server.GetStatus` decoding. Anything that mutates state creates a throwaway MPD **partition**, works there, and deletes it — the default partition's queue is never touched.
  - Four of them are **diagnostics rather than assertions**, and they are the fastest way to answer "why is this album's cover blank" — reach for them before theorising: `live_diagnose_album_art_sources` prints, per album, which stage answers (`readpicture` / `albumart` / nothing) plus the raw artist tags; `live_probe_cue_album_art_fallback` checks whether a CUE-sheet track's cover is reachable via the `.cue` path (on this library: no); `live_musicbrainz_resolves_locally_artless_albums` and `live_wikipedia_bios_for_awkward_tags` run the real external lookups over the tag shapes that used to defeat them. The last two need **`WINRMPC_TEST_MUSICBRAINZ=1`** on top of `--ignored`, so a routine sweep can't start hammering a free community service. `live_tls_reaches_every_lookup_host` (gated on **`WINRMPC_TEST_NETWORK=1`**) is the odd one out: it needs **no MPD server at all** and issues one cheap request to each of the four hosts, so it's the acceptance check for the rustls switch and the fastest way to tell "the network is broken" from "the lookup logic is broken". Two Snapcast tests in this file are *not* ignored: they drive the client against a local `TcpListener` mock to prove a dead socket is dropped (so the view can reconnect) while an RPC error response is not.
- Icon embedding (`build.rs` → `winres`) needs `rc.exe`/`windres` on PATH; if absent it's skipped with a `cargo:warning`, build still succeeds.

## Tech Stack
- Rust 2021 edition
- `iced 0.13` — Elm-style GUI (Model / Message / Update / View). NB: an attempted 0.14 migration was reverted (see commits `61024b2`/`31c7dba`); stay on 0.13.
- `tokio` — async runtime
- `redb 4` — embedded key-value DB backing the art + lyrics cache (`src/store/mod.rs`)
- `image 0.25` — decode/downscale album art
- `reqwest 0.12` — HTTP for MusicBrainz / Wikipedia / LRCLIB. **Pinned to `default-features = false` + rustls**; see "Outbound HTTP" below before touching its features
- `serde` / `serde_json` / `toml` — config + cache serialization
- `directories` — platform config/cache paths
- `fuzzy-matcher` — search ranking; `flume` — channels; `open` — launch URLs; `urlencoding`; `chrono`
- `anyhow`, `thiserror`, `tracing`

## Project Structure

```
build.rs                     (repo root, not src/) Generates 16×16+32×32 BMP-in-ICO, embeds via winres on Windows
assets/fonts/                Bundled icon font (committed; rebuilt by packaging/fonts/build-icon-font.py)
packaging/                   Desktop-integration assets: linux/ (.desktop + hicolor icons + install.sh), fonts/ (icon-font generator)
src/
  main.rs                    Entry point: windows_subsystem, tracing layers, window icon, launches iced
  logger.rs                  InAppLayer (tracing Layer) + static ring-buffer; get_entries() / clear_entries()
  net.rs                     Shared User-Agent + HTTP client construction for every outbound fetch
  icon.rs                    Programmatic 32×32 RGBA icon (equalizer bars); make_icon() → iced::window::Icon
  live_tests.rs              Opt-in #[ignore]d integration tests against a real MPD/Snapcast server
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
      recently_played.rs
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
      sidebar.rs             Navigation sidebar — fixed `SIDEBAR_WIDTH` (132px), wrapped in a `scrollable` with a slimmed 4px scrollbar; with a `Length::Fill` spacer the bottom group (Settings/Log/Stats) used to be pushed off a short window with no way to reach it, and at the old 90px width "Recently Added" wrapped to two lines and the connection line was clipped
      art_image.rs           Bytes → iced ImageHandle helper
      icon.rs                Bundled icon font + every glyph constant the UI draws
      song_row.rs            Shared song-list row state: is-current predicates, row/title colours, playing marker
      link.rs                Clickable hyperlink widget (opens URLs via `open`), icon buttons, tooltips
      album_grid.rs          Shared cover-grid: tile/grid/layout_toggle/art_for, used by Albums, Recently Added and Recently Played
      mod.rs
```

## AppConfig (`src/config/settings.rs`)
Fields saved to TOML via `directories`, from one `ProjectDirs::from("com", "winrmpc", "winrmpc")` call:

| | config (`config.toml`) | cache (`winrmpc.redb`) |
|---|---|---|
| **Windows** | `%APPDATA%\winrmpc\winrmpc\config\` | `%LOCALAPPDATA%\winrmpc\winrmpc\cache\` |
| **Linux** | `~/.config/winrmpc/` | `~/.cache/winrmpc/` |
| **macOS** | `~/Library/Application Support/com.winrmpc.winrmpc/` | `~/Library/Caches/com.winrmpc.winrmpc/` |

Overridable with **`WINRMPC_CONFIG_DIR`** / **`WINRMPC_CACHE_DIR`** (an empty value counts as unset — an exported-but-empty variable is a common shell accident and must not relocate settings to the working directory). See "Storage discoverability" below. Fields:
- **Multi-server** (v0.4.0): `servers: Vec<MpdServer>` + `default_server: Option<String>` (name). `MpdServer { name, host, port, password, default_partition, snapcast_host, snapcast_port }` — partition is **per-server** (partitions live on one MPD instance). Helpers: `server(name)`, `server_mut(name)`, `server_addr(name)` (falls back to first server, then legacy `mpd_addr()`); `MpdServer::snapcast_addr()` falls back to the MPD `host` and port `1705` when `snapcast_host`/`snapcast_port` are unset (the common deployment: Snapcast colocated with MPD). There's no Settings UI yet for editing `snapcast_host`/`snapcast_port` directly — only the config fields and the fallback exist; a per-server form field is a deliberate follow-up, not an oversight.
- **Legacy single-server fields** kept for back-compat: `mpd_host`, `mpd_port`, `mpd_password`, `default_partition`. Mirror the active server; drop in a future release.
- `art_cache_size_mb: u32` — enforced via LRU eviction in the redb store (not just advisory)
- `theme: ThemeConfig`
- `radio_stations: Vec<RadioStation>` — built-ins + user customs; `ensure_builtin_stations()` re-adds missing built-ins on load
- `cd_device: Option<String>` — e.g. `/dev/sr0`; edited in the **CD view** (moved out of Settings), `#[serde(default)]`
- `recent_albums: Vec<RecentAlbum>` — most-recent-first, capped at 8, `#[serde(default)]`; updated on `CurrentSongUpdated` when the album changes
- `album_grid_view: bool` — cover grid vs compact list for Albums / Recently Added / Recently Played, `#[serde(default)]`; see "Album cover grid vs list"

**Migration** (`AppConfig::load`): if `servers` is empty after deserialize, synthesize `MpdServer { name: "Default", … }` from the legacy fields, set `default_server`, and `save()` once so the file upgrades. All new optional fields must carry `#[serde(default)]` so existing config files still load.

**`load` distinguishes missing from unparseable, and that distinction is the point.** `load_from(Option<PathBuf>)` / `save_to(Option<&Path>)` are the path-injectable cores so all three cases are testable without touching the real user config.
- *Missing* (first launch) → write the defaults out. Previously the file was created lazily by whatever action next called `save()`, so a new user had **no file and no folder** — which matters because `art_cache_size_mb`, `theme` and the per-server `snapcast_host`/`snapcast_port` had no other way in.
- *Present but unparseable* → keep defaults for the session, log at ERROR, and set `load_failed`, which makes `save` a **no-op**. Without it, one stray character in the TOML was answered by silently overwriting every server, radio station and saved partition with defaults on the next setting change. This is the hazard the `config/settings.rs` tests were written for; nothing actually prevented it until now.

**Everything in the config now has a UI** except `theme` — see "Settings UI coverage" below.

## Settings UI coverage
Every persisted option is editable in the app, so the config file is a convenience rather than a requirement:
| Option | Where |
|---|---|
| `servers[]` name | Settings → server row → **Rename** |
| `servers[]` host / port / password / `snapcast_host` / `snapcast_port` | Settings → server row → **Edit** (inline expansion) |
| `default_server` | Settings → **Set as default** |
| `art_cache_size_mb` | Settings → Cache → **Limit** |
| `cd_device` | CD view |
| `radio_stations` | Radio view |
| `album_grid_view` | the ▦ Grid / ☰ List toggle |
| `default_partition` (per server) | Partitions view (saved on switch) |
| `recent_albums` | not a setting — session state |

- **Editing a server that is the active one reconnects**, by delegating to `SwitchServer` (which mirrors the legacy fields and restores the partition). `active_server` is cleared first so `SwitchServer` doesn't short-circuit on its "same name" guard. Snapcast's client is dropped either way, since its host/port may have moved even when MPD's didn't.
- **A bad port keeps the old one** instead of silently resetting to 6600 — a half-typed port shouldn't be committed as a real change.
- **`theme` (`dark_mode`, `accent_color`) is the one exception, and it is dead config**: `App::theme()` returns `Theme::Dark` unconditionally and nothing reads `accent_color` — every colour comes from the `AppColors` constants. It deliberately has **no UI**, because a toggle that does nothing is worse than none. Making it real means adding a light `AppColors` palette and threading it through every view (the constants are `const`, referenced directly by all 20+ view modules); until someone wants that, the field is either a future feature or a deletion.

## MPD protocol reference
**<https://mpd.readthedocs.io/en/stable/protocol.html>** — the authoritative command reference. Check it before adding or changing any command; exact spelling matters and MPD rejects anything else outright with `ACK [5@0] {} unknown command` (see the `replay_gain_mode` note under "Playback options"). Underscore conventions are inconsistent across the protocol — `replay_gain_mode` and `list_OK` have them, `setvol`, `playid`, `seekcur`, `listplaylistinfo`, `delpartition` don't — so guessing is unreliable.

## MpdClient (`src/mpd/client.rs`)
- `Arc<Mutex<Option<MpdConnection>>>` — clone-cheap, shared across async tasks
- `fn escape(s: &str)` — **always** use this when interpolating user strings into MPD commands (prevents injection of `"` and `\`)
- Binary protocol via `cmd_binary` (album art chunks)
- Key methods: `status()`, `current_song()`, `queue()`, `add()`, `add_id()`, `find()`, `find_add()`, `lsinfo()`, `tag_art()`/`cover_file_art()`, `switch_partition()`, etc.

## Iced App Architecture (`src/ui/app.rs`)
`app.rs` is ~3200 lines and holds the whole Elm loop: `App`'s state, every `Message` handler, `view`, `subscription`, and all the `fetch_*` task builders. Note that despite the `ui/views/` directory, **Settings has no view module** — it's `App::settings_view()`, a method on `App` (it reads and writes a lot of `settings_*` state fields). Every other view is its own module.

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
- **`MpdClient::list_albums_by_artist()`** sends `list Album group AlbumArtist` (MPD 0.21+; falls back to a flat, artist-less list on ACK from older servers). `group_albums_by_artist(pairs)` groups the resulting `(artist, album)` pairs, keyed `(artist.to_lowercase(), base.to_lowercase())` — `base` comes from `album_base_and_disc`. **Both** halves are case-folded (the first-seen spelling is still what gets displayed): inconsistent capitalisation across one album's discs is common in real tags, and without folding `"Decade Of Aggression - Disc 2"` and `"Decade of Aggression - Disc 1 of 2"` are two rows of one album.
- **`album_base_and_disc(album) -> (String, Option<u32>)`** strips a trailing disc marker — bracketed (`"X [Disc 1]"`, `"X (CD 2)"`) or bare-trailing with a required delimiter (`"X - Disc 1"`, `"XCD2"` but never `"ABCD2"`). Also handles the forms a real library actually contains: letter disc ids (`"101 [Disc A]"` → disc 1), spelled-out numbers (`"Lotus (Disc One)"`), of-total forms (`"… - Disc 1 of 2"`, `"… (CD 1/2)"`), and a marker sitting at the **tail of a qualifier bracket** (`"X [24-bit Remaster CD 1]"` → base `"X [24-bit Remaster]"` — only the marker is removed, because dropping the whole bracket would fold a remaster into a plain edition the library may hold separately, the same reason `strip_edition_qualifier` is lookup-only). A trailing disc-*count* bracket (`"(2CD)"`, `"(3 CDs)"`) is stripped from **every** base, marker or not — one real album is tagged `"Nostradamus (2CD) (CD 1/2)"` and `"Nostradamus (disc 2)"`, which only meet if the count bracket always goes. Passes through unchanged when there's no marker, the "marker" carries no disc id (`"Live CD"`, `"Killers (CDM 7520192)"`, `"… [2001 CD Edition]"`), or stripping would leave an empty base (`"Disc 1"` alone). Guard rails that keep the wider matcher from over-stripping: a spelled-out number or letter id needs a separator after the marker word (so `"(CDs)"` isn't "CD, disc S"; digits may still abut, as in `"CD2"`), and disc numbers are capped at `MAX_DISC_NUMBER` (99) so a 3-digit catalogue number or year can't read as a disc.
- **`Song::art_key()`** runs the album through `album_base_and_disc(...).0` first, so every disc of a set shares one art-cache entry/fetch. **`Song::effective_disc()`** prefers the `disc` tag (handles `"2"` and `"2/2"`) over the name-suffix-derived disc, defaulting to `1` — album detail sorts tracks by `(effective_disc, track)` instead of track alone, fixing interleaving on `disc`-tagged-but-unsuffixed multi-disc albums.
- **`View::AlbumDetail(name, artist: Option<String>)`** — when `artist` is `Some`, `AlbumSelected`'s song-loader re-derives this artist's `AlbumGroup`s (a fresh `list_tag_filtered("Album", "AlbumArtist", artist)` + `group_albums_by_artist`), finds the group matching `base`, and fetches+concatenates **every** variant via `MpdClient::find_album_by_artist` — this is the multi-disc expansion. `artist: None` (only from Genre detail, which isn't artist-grouped) falls back to a plain `find("Album", base)`, matching mikMPD's "skip sibling merging when the artist is unknown" safety rule.
- **`album_disc_count(variant_count, max_tag_disc)`** takes the max of both signals — name-suffix variants and the highest `effective_disc` seen — since a properly tagged multi-disc album has no name variants and a poorly tagged one has no disc tag; list-view "N discs" captions currently use `variants.len()` alone (cheap, no song fetch), while the full two-signal max is available once `AlbumDetail`'s songs are loaded.
- **`album_scoped_key(artist, album)`** is the single key shape for the `App`-level `album_songs`/`album_bios` maps (and the redb `bios` table) — `format!("{artist_or_empty}\x1falbum")`, **not** disc-folded (its input is already the collapsed base name). `View::AlbumDetail(name, artist)` and `fetch_album_bio` both build their key through it, so storage and render-time lookup always agree; two different artists' same-titled album no longer collide (previously the in-memory guard was keyed by album name alone). `fetch_album_bio` derives **both** the key and the MusicBrainz query from the same `Option<String>` artist and nothing else — there is deliberately **no** `self.selected_artist` fallback for the `artist: None` case (Genre detail). That fallback used to exist and was removed: the key is `"\x1f{album}"` either way, so it wrote a bio fetched for whatever artist page happened to be open into a slot shared by every artist-less album of that title, persisted it to redb, and read it back for unrelated albums. Querying with an empty artist instead degrades to a title-only Wikipedia lookup (still guarded by `title_matches`) — fewer bios found, but never one attributed to the wrong album.
- **`art_key_for(artist, album)`** is the single builder for every art-cache key (`Song::art_key()` is a thin wrapper over it) — every site that fetches, stores, or looks up cached art goes through it, including the four "recently played" sites (`fetch_recent_art`, `now_playing.rs`'s recents strip/filter, `recently_played.rs`'s album tiles) that used to build the key from the raw (un-disc-stripped) tag and silently miss the cache `Song::art_key()` populates.
- `parse_bare_trailing_marker` (backing `album_base_and_disc`) searches via `rfind_ascii_ci`, a length-preserving ASCII-only case-insensitive search over the *original* string — not `s.to_lowercase()` — because `to_lowercase()` isn't byte-length-preserving for some non-ASCII input (Turkish `İ`), which previously could panic or mis-slice when reused to index into `s`.
- **Not yet done** (scoped out of the initial pass, see `docs/plans/library-album-identity-and-multidisc.md`): grid view (Part C) and Search sections/batch-select (Part D). Also not done, deliberately deferred as a separate follow-up: punctuation-folding the grouping key (mikMPD folds en/em-dash and smart quotes so two rips of one album with a differently-encoded dash don't split into two rows — winrmpc doesn't yet; see `docs/plans/review-fixes-correctness.md` §6) and de-duping `recent_albums` on the disc-stripped base (§1's "also worth folding in" note).

### Queue editing
`QueueRemove(id)` uses `delete_id` (song id, not position — stable across concurrent queue mutations). `QueueMoveUp`/`QueueMoveDown(pos)` wrap `move_pos(from, to)`; MPD's `move FROM TO` leaves the song at position `TO` in the *final* list (remove-then-insert semantics), so `move(pos, pos-1)`/`move(pos, pos+1)` are simple adjacent swaps with no off-by-one. `QueueAddNext(uri)` composes this: `add_id` appends to the end, then `move_pos(end, current_song_pos + 1)` relocates it to play right after the current track; if nothing is playing (`song_pos` is `None`), it falls back to `play_id` on the newly added song instead of trying to insert "next" of nothing.

### Playback options: crossfade + replay gain (`src/ui/widgets/player_bar.rs`)
Crossfade (`Status.crossfade`) and replay gain both live in the **player bar**, in their own 190px column to the *left* of the repeat/random/single/consume grid (crossfade above replay gain) — they're server-wide playback settings exactly like those, so they belong next to them rather than in a single view's header (they were originally in `now_playing.rs`'s `toggle_row`). The replay-gain `pick_list` carries a visible **"Replay Gain"** label; a bare dropdown reading `off/track/album/auto` gives no clue what it controls.
- **The MPD commands are `replay_gain_status` and `replay_gain_mode <mode>` — with the underscore between "replay" and "gain".** `replaygain_status`/`replaygain_mode` are not commands; MPD answers `ACK [5@0] {} unknown command` and replay gain silently never works. This was a real shipped bug, fixed in 0.4.1; `live_replay_gain_round_trips` in `live_tests.rs` guards it against a real server, because a unit test can only check the string we build, not that MPD accepts it.
- `replay_gain_mode` is fetched once in `fetch_all()` on connect (not polled — it rarely changes and isn't part of `status`), stored on `App`, and optimistically updated in the `SetReplayGainMode` handler before the command round-trips.

### CD playback
- **Play whole disc**: `add("cdda://")` (no device) or `add("cdda://{device}")` when configured
- **Track probe** (`CdProbe`):
  1. If `config.cd_device` is set: try `lsinfo("cdda://{device}")` → extracts `(file_uri, duration_secs)` from `DirectoryEntry::File` entries
  2. Batch fallback: `status()` → record `queue_length` as `start`, `add` each `cdda:///1`…`cdda:///99` until error, `queue()` → take the slice, `delete_range_from(start)` in one shot. Gets real durations; avoids "Failed to load file" log spam from the old add_id + immediate delete_id pattern (MPD starts a background read on add; deleting before it completes causes the exception).
- `cd_tracks: Vec<(String, Option<f64>)>` — URI + optional duration
- Note: `lsinfo cdda:///` and `lsinfo cdda:///dev/sr0` both fail on this setup. The batch add+delete fallback is the actual working path.
- `MpdClient::delete_range_from(start)` — sends `delete {start}:` (open-ended range).

### Connection desync + self-healing (`src/mpd/error.rs`, `client.rs`)
**`MpdError::is_connection_fatal()`** — true for everything except `Server` (an `ACK`, which is a well-framed reply proving the socket is fine) and `NotConnected`. `MpdClient::cmd`/`cmd_binary`/`add_all` set `*guard = None` on a fatal error, so the next `ConnectionTick` opens a genuinely new socket.
- **Why this is load-bearing**: `read_line` fails with `stream did not contain valid UTF-8` the moment binary art bytes are left in the buffer, and tokio consumes those bytes on the failed read — so one bad framing event poisons the connection permanently. Worse, `ConnectionTick` short-circuits on `client.is_connected()` (which is just `conn.is_some()`), so a desynced-but-`Some` connection made the app log **"Connected to MPD" every 3s while every command failed**, recoverable only by restarting. Shipped symptom: `currentsong` answering with `ACK … {albumart} No file exists`, i.e. reading a *previous* command's response.
- `read_binary` must consume the trailing newline **and** the terminating line on every path, including `binary: 0` — an early return there left two lines for the next command to misread.
- Header parse failures (`size:`/`binary:`) return `Parse`, which is fatal by design: we've read a header announcing a payload we can no longer locate.

### Protocol EOF guard (`src/mpd/protocol.rs`)
All three read loops (`read_pairs`, `command_list`, `read_binary`) check `if line.is_empty()` and return `MpdError::Connection("Connection closed unexpectedly")` to prevent infinite hang on server drop.

## Cache Store (`src/store/mod.rs`)
Single `winrmpc.redb` file under the platform cache dir. Tables: `art` (blobs), `art_meta` (`ArtMeta { size, last_access, is_empty }`), `lyrics` (serde_json `Option<Lyrics>`), `bios` (serde_json `Option<String>`, keyed `"artist:{name}"` / `"{artist}\x1falbum"` — see "Wikipedia / MusicBrainz" below), `mb_ids` (serde_json `Option<String>` MBIDs, same key shape as `bios`), `recently_played` (serde_json `Vec<RecentlyPlayedEntry>`, keyed by server name — see "Recently Added / Recently Played" below; app-generated state, not a cache of re-fetchable data, but stored here rather than `AppConfig`/TOML for the same write-amplification reason art/lyrics moved out of flat files: frequent small writes need a targeted key update, not a whole-file rewrite), `meta` (migration markers). `bios`/`mb_ids`/`lyrics` all share the same three-state convention: absent key = never fetched, `Some(None)` = fetched, confirmed nothing found, `Some(Some(v))` = have a value.
- **redb is synchronous** — every `Store` method must be called inside `spawn_blocking`; never hold a transaction across `.await`. Writes commit (fsync) immediately — there is **no flush-on-close**; a crash after a fetch loses nothing.
- `ArtCache` (`art/cache.rs`) is an in-memory `HashMap` hot layer over `Store`; `Store` is the source of truth. `store()` downscales to a 500px JPEG before persisting.
- **LRU eviction**: `art_put` calls `art_evict(limit_bytes)` — removes oldest `last_access` entries until under `art_cache_size_mb`. Negative entries (`is_empty`, no blob) are exempt.
- **Negative caching**: `art_put_empty` / `store_empty` records "known missing" so art isn't refetched every launch. `art_known` / `is_known` gate whether to fetch. Lyrics use `Some(None)` for "cached: no lyrics exist".
- **Manual purge** (Settings → Cache): `Message::ClearCaches` calls `ArtCache::clear` (in-memory layer + `art`/`art_meta`) and `Store::clear_lookup_caches` (`lyrics`/`bios`/`mb_ids`). It deliberately spares `recently_played` — app-generated history that nothing could re-derive — and `meta`, whose migration markers would otherwise re-run one-time purges. The handler also drops both art queues and zeroes their counters: in-flight fetches would otherwise re-populate the cache being cleared and then decrement counters that no longer mean anything. **It takes two presses** (`confirm_clear_caches`, disarmed on view enter) because a purge is one click to trigger and hours to undo — every cover re-downloads, and the MusicBrainz stage re-crawls. `Store::art_cache_bytes` backs the "N MB of M MB limit" readout beside it.
- **Startup housekeeping** (`open`): recreates tables; `purge_poisoned_negatives` clears "we looked and found nothing" records whenever the lookup rules change enough to invalidate them — **bump its marker to re-run it** (`neg_purge_v1`: negatives written by the empty-URI recents fetch; `neg_purge_v2`: everything recorded before the MusicBrainz matching fixes). It purges negative `art_meta` entries **and** `Some(None)` `mb_ids`; leaving the latter would have `search_release_group` return the cached "no match" without ever issuing the corrected query, so the art fix would be invisible on any existing cache. `cleanup_legacy` deletes the pre-DB flat `*.jpg` + `lyrics/` caches. On DB-open failure it wipes+rebuilds, falling back to an in-memory backend so the app still runs.

## Server switching (`src/ui/app.rs`, `src/ui/message.rs`)
`active_server: String` tracks the current server by name. `SwitchServer(name)` rebuilds `MpdClient`, sets `connected = false`, emits `Connect`, and restores that server's `default_partition` on `Connected`. It also reloads `recently_played` history for the new server (`recently_played_get`, via `spawn_blocking`) and clears the in-memory list first so a slow load can't briefly show the old server's history. `SetDefaultServer` / `AddServer` / `RemoveServer` manage the list from the Settings view; `RemoveServer` also deletes the removed server's `recently_played` key. Startup connects to `default_server`.

## Snapcast Multiroom Control (`src/snapcast/`, `src/ui/views/snapcast.rs`)
Sibling module to `mpd/`, not bolted onto `MpdClient` — Snapcast is a fully independent JSON-RPC-2.0-over-raw-TCP connection (port 1705 by default) that may be absent/unreachable while MPD is fine. `SnapcastConnection::request()` (`protocol.rs`) sends one line, then reads lines until one carries the matching `"id"`, silently skipping anything else (interleaved push notifications, or a stale response) — this app doesn't consume notifications, polling only (`Message::SnapcastPollTick` every 2s, subscribed only while `View::Snapcast` is the active view). `SnapClient`/`SnapGroup`/`SnapStream` (`types.rs`) are decoded straight from `Server.GetStatus`'s `result.server` JSON via `serde_json` (no hand-rolled parser needed — Snapcast's wire format is already JSON, unlike MPD's).
- **Connection lifecycle**: `self.snapcast_client: Option<SnapcastClient>` is created lazily on `View::Snapcast` entry (`on_view_enter`, now `&mut self`) and reused across later visits — `on_view_enter` only rebuilds it when there's none yet or `SnapcastClient::addr()` no longer matches the server's current `snapcast_addr()` (host/port edited in Settings), and only calls `SnapcastClient::connect()` when `is_connected()` is false, so a working connection isn't torn down and reopened on every single visit. `is_connected()` is only bookkeeping (`Option::is_some()`), **not** a socket probe — what keeps it honest is that `SnapcastClient::request()` sets the connection back to `None` whenever a call fails with `Connection`/`Io`/`Json` (a dead socket: snapserver restarted, LAN blip, resume from sleep), so the next poll or view entry reconnects. `Rpc` errors deliberately don't drop the connection — a well-formed JSON-RPC error response proves the socket is fine. Without that reset the `if !is_connected()` guard would never fire again after the first drop and the view would stay stuck erroring until an app restart. `Message::SwitchServer` clears `snapcast_client`/`snapcast_groups`/`snapcast_streams`/`snapcast_error` outright — otherwise a client built against server A would silently keep controlling A's Snapcast instance after switching to server B, since nothing else would notice the server changed while that view is closed.
- **Inactive clients are hidden by default.** A Snapcast server keeps a stale entry for every device that ever connected (a real server here showed 17 groups of which 2 had a connected client), so the unfiltered list is mostly dead weight. `snapcast_show_inactive: bool` on `App` (session-local, not persisted) drives a "Show/Hide N inactive" button in the view header, shown only when there *are* inactive clients. `visible_groups()` also drops a group whose clients are *all* disconnected, so hiding doesn't leave empty cards behind.
- **Controls** (`Client.SetVolume`, `Group.SetMute`, `Group.SetStream`): each handler optimistically mutates `self.snapcast_groups` in place first (so the UI reflects the change immediately, e.g. a dragged slider), then fires the RPC. No drag-lock against the 2s poll — the existing MPD volume slider (`player_bar.rs`) doesn't have one either, so this matches the codebase's established pattern rather than adding new complexity; the only failure mode is a rare mid-drag poll overwrite, acceptable for a v1.
- **Not implemented** (explicitly deferred in the plan's own phasing): notification-driven live updates, client rename/latency editing, moving clients between groups, deleting disconnected clients, and a Settings UI for editing `snapcast_host`/`snapcast_port` per server (the fields exist and default to "same host as MPD, port 1705").

## Recently Added / Recently Played (`src/mpd/types.rs`, `src/ui/views/{albums_list,recently_played}.rs`)
Two distinct features sharing one plan (`docs/plans/recently-added-and-played-history.md`) because both extend history-adjacent state — **not to be confused with `RecentAlbum`/`recent_albums`**, the pre-existing 8-item "what's been playing this session" strip shown inline in Now Playing, which is untouched by this section.
- **Recently Added** (`View::RecentlyAdded`, sidebar beneath Genres): `MpdClient::find_recently_added(since, limit)` sends `find "(modified-since '…')" window 0:limit` — always bounded (an unbounded `modified-since` scan can outrun the socket read on a large library). `on_view_enter` computes `since` as `now - 30 days`. Songs are sorted newest-`last_modified`-first and collapsed to unique album names, rendered through the same `views::albums_list::view` used for the plain Albums list (now takes a `title: &str` param so it can say "Recently Added" instead of "Albums").
- **Recently Played** (`View::RecentlyPlayed`, opened via a `icon::HISTORY` + "History" link in Now Playing's toggle row, not the sidebar): per-server track-level history, distinct from `recent_albums`. `PlayRecorder` (`types.rs`) is a self-contained tick-based reducer — call `tick(file, is_playing, elapsed_secs, duration_secs)` on every `StatusUpdated`; it tracks its own last-seen elapsed internally (no caller-side delta bookkeeping needed) and returns `true` the moment a play should commit: `accumulated >= min(30, max(5, duration/2))` seconds of actual playback, capping any single delta at 5s so a seek or coarse poll gap can't fast-forward the threshold. A commit pushes a `RecentlyPlayedEntry` to `self.recently_played`, prunes to 30 days / 100 entries (`prune_recently_played`), and persists via `spawn_blocking`. **CD tracks are skipped** (no recording at all while `cdda://` is playing); radio streams are recorded (unlike `recent_albums`, which effectively excludes them via its "Unknown Album" filter). The view's Albums mode **derives** groups from track history (`recently_played_albums` — first occurrence per (artist, album) wins, since entries are already newest-first) rather than recording albums separately, so there's one source of truth. `RecentlyPlayedEntry` carries **both** artists: `artist` (`display_artist()`, the track artist, shown per row) and `album_artist` (`display_album_artist()`, `#[serde(default)]` for pre-existing history). `RecentlyPlayedEntry::art_artist()` returns `album_artist` when set and falls back to `artist`, and it's what `recently_played_albums` groups and labels by — art is only ever cached under `Song::art_key()`, i.e. the *album* artist, so grouping by the track artist both split compilations into one tile per guest artist and made every one of those tiles miss the art cache.

## Storage discoverability + loud failures (`src/config/settings.rs`, `src/store/mod.rs`)
Persistence **works** on all three platforms; what it wasn't was discoverable, or loud when it failed.
- **macOS is why this exists.** `~/Library` is hidden in Finder by default, the directory is named `com.winrmpc.winrmpc` (reverse-DNS, the platform convention `directories` follows) rather than `winrmpc`, and Spotlight doesn't index `~/Library` — so "I couldn't find the config file" was an entirely reasonable report. On macOS `config_dir()` and `data_dir()` are the *same* directory; that's the convention, not a bug.
- **Settings → Storage** prints both resolved paths with an **Open folder** button each (`open::that_detached`, mapping to Explorer/Finder/`xdg-open`). It opens the *directory*, never the file — "open this .toml" launches a text editor. The folder is created on click, since the paths are shown before anything has been written there and a dead button is worse than no button.
- **Both paths are logged at INFO on startup**, so they land in the Log view even without visiting Settings.
- **The three silent paths are now loud**, which was the actual bug class here:
  - `config_dir()`/`cache_dir()` returning `None` (no `$HOME`, odd sandbox) logs **ERROR** naming the consequence. Previously `load_from(None)` returned defaults before attempting any write and `save_to(None)` was a silent `Ok(())` — every setting appeared to work and was gone at restart, with nothing in the log at all.
  - `Store::open`'s final `InMemoryBackend` fallback is **ERROR, not WARN** (the app is running with a core feature off), and `Store::is_persistent()` surfaces it in Settings → Storage. The symptom otherwise is just "everything re-downloads, forever", which doesn't point at storage.
  - **`AppConfig::save_and_log(what)`** replaces all 12 `config.save().ok()` call sites. Each is a deliberate user action rather than a poll, so logging every failure is not a spam risk.
- **`ProjectDirs::from("com", "winrmpc", "winrmpc")` is deliberately unchanged.** A friendlier macOS directory name would orphan every existing install's config and cache and need a migration that reads the old location. The cache is disposable; the config is not.
- ⚠️ **Never call `AppConfig::save()` or `save_and_log()` from a test without setting `WINRMPC_CONFIG_DIR` first.** They resolve the *real* user config path, so an unguarded call overwrites your own servers and radio stations with defaults. This has happened. Every other test in `config/settings.rs` uses the path-injectable `save_to`/`load_from` cores for exactly this reason; the one test that exercises the env override sets it to a scratch dir and asserts the write landed there.

## Current-song highlighting (`src/ui/widgets/song_row.rs`)
Six views mark the playing track: Queue, Album detail, Playlist detail, Search, Browser and Recently Played (Tracks mode). One shared helper owns the styling (`row_bg`, `title_color`, `playing_marker`) plus the two match predicates.
- **The Queue matches on queue *position*; every other list matches on the song's *URI*.** This is the whole design decision, and getting it wrong produces **wrong** highlighting rather than missing highlighting. A queue can legitimately hold the same file twice, and `status.song_pos` is what distinguishes the two entries — so the Queue keeps `is_current_pos`. Every other list is a *library* listing whose rows have no queue position at all: **`playlist_detail`'s `pos` is the playlist index**, unrelated to `song_pos`, so a position compare there lights up an arbitrary row. `song_row::tests::a_playlist_row_at_the_playing_queue_position_does_not_match_by_position` is that exact bug, asserted.
- `App::current_file()` hands views `Option<&str>`, not `&Option<Song>` — it makes the match rule obvious at the call site and keeps views from reaching for other fields.
- **`playing_marker` is a fixed-width cell that is either the glyph or blank**, never a widget pushed in only when current — otherwise every other column shifts depending on what's playing. The Queue's `#` header is 48px to stay aligned with it (14 marker + 8 spacing + 26 number).
- **`AppColors::ROW_PLAYING`** is new. It sits above `ROW_ODD` and below `BG_HOVER` so it reads as selected against *both* zebra stripes while still changing visibly on hover; a test asserts it equals none of the three. The Queue previously improvised with `BG_TERTIARY`, which is also several panels' background colour.
- **A stopped player still highlights.** MPD keeps a current song when stopped and the Queue already behaved this way; the player bar communicates play/pause/stop. Consistency with the existing view beat the distinction.
- **Accepted consequence**: the same track twice in one album or playlist highlights **both** rows. Rare enough not to complicate the rule, and asserted so it stays a decision.
- **Not done** (deliberately deferred, plan step D): album-level highlighting — marking the album *containing* the playing track in the grids and album lists. It's a different comparison (`album_scoped_key` against the disc-collapsed base, so a playing Disc 2 track marks the single collapsed row) with its own edge cases, and landing it separately keeps an album-key bug from holding up the track-list work. Also out of scope (step E): auto-scrolling a list to the playing track, and the Radio view.

## Icon font + row actions (`src/ui/widgets/icon.rs`, `link.rs`)
**Every glyph in the UI comes from the bundled icon font — never from a system font, and never from a bare Unicode literal in a view.** `ui::widgets::icon` owns `assets/fonts/winrmpc-icons.ttf` (a ~2.6 KB, 17-glyph subset of Material Symbols, Apache-2.0), registered once at startup via `iced::application().font(icon::FONT_BYTES)` in `main.rs`, and exposes one `pub const` per glyph plus `icon()`/`icon_sized()`.
- **Why bundling, not naming a font**: `link.rs` used to say `Font::with_name("Segoe UI Symbol")` with a comment explaining that the default font renders `▶`/`＋` as tofu. But iced only bundles `Iced-Icons.ttf` on native targets (`iced_graphics-0.13.0/src/text.rs:159`) and resolves every other family through **system** fonts — so on macOS and Linux that lookup fell back to precisely the font the comment called broken, and the row actions rendered as **empty boxes**. Naming any system font is the bug; the fix is to carry the glyphs.
- **Constants are named for the action, not the upstream icon** (`ADD_PLAYLIST`, not `playlist_add`), so call sites read as intent.
- **Two glyphs were re-picked, not just re-fonted**: `⏭` (the universal *skip-to-next* transport glyph, sitting inches from the player bar's actual Next button) became `PLAY_NEXT`, and `☰` — which meant *both* "add to playlist" and "list layout" on the same screen — split into `ADD_PLAYLIST` and `LIST`.
- **`DOT` is the one glyph instanced at `FILL=1`.** Outlined, `fiber_manual_record` is a hollow ring, and that glyph's entire job is to be a solid status dot. `packaging/fonts/build-icon-font.py` instances the two fills separately and merges them.
- **Icon and label are always separate widgets** (`link_icon`, `layout_toggle`, `centered_icon_note`, the Log view's slow marker). One `text` carries one font, so `"🕐 History"` as a single string could only ever render one half correctly — and the Log line is monospace besides.
- **Typographic punctuation stays as text**: `…`, `–`, `—`, `·` are General Punctuation and covered essentially everywhere. `→` did *not* stay — it's in the Arrows block, whose coverage is far less certain, so it became `ARROW_FORWARD`.
- **The font is committed and kept honest by tests**, mirroring the app-icon PNGs: `icon.rs`'s tests parse the TTF's `cmap` and assert every constant resolves to a glyph, that the font carries no glyph without a constant, that no two constants collide, and that the family name is the bundled one. Change the icon set → edit `build-icon-font.py`, rerun it, add the constant.
- **Regenerating** needs `fonttools` (a dev-only dependency, not part of `cargo build`): `python3 -m venv .venv && .venv/bin/pip install fonttools brotli && .venv/bin/python3 packaging/fonts/build-icon-font.py`.

**Row actions are a deliberate table, not per-view accident.** Every row action is an `icon_btn_tip` (or `icon_btn_danger` for destructive ones) carrying a fixed tooltip — the wording is part of the contract, so the same button never reads differently between views:

| Action | Glyph | Tooltip | album | search | browser | playlist_detail | queue |
|---|---|---|---|---|---|---|---|
| `PlaySong` / `PlaylistPlayAt` | `PLAY` | Play now | ✓ | ✓ | ✓ | ✓ | — |
| `QueueAddOnly` | `ADD_QUEUE` | Add to end of queue | ✓ | ✓ | ✓ | ✓ | — |
| `QueueAddNext` | `PLAY_NEXT` | Play next | ✓ | ✓ | ✓ | ✓ | — |
| `OpenAddToPlaylist` | `ADD_PLAYLIST` | Add to playlist… | ✓ | ✓ | ✓ | ✓ | ✓ |
| `QueueMoveUp`/`Down` | `MOVE_UP`/`MOVE_DOWN` | Move up / Move down | — | — | — | — | ✓ |
| `PlaylistMoveSongUp`/`Down` | `MOVE_UP`/`MOVE_DOWN` | Move up in playlist / Move down in playlist | — | — | — | ✓ | — |
| `QueueRemove` | `REMOVE` | Remove from queue | — | — | — | — | ✓ |
| `PlaylistRemoveSong` | `REMOVE` | Remove from playlist | — | — | — | ✓ | — |

The two deliberate gaps: **the queue has no "add to end of queue"** (those rows already *are* the queue), and **only reorderable lists get move arrows** (you can't reorder an album). Browser file rows gained the full four — they previously had play/add only, for no stated reason. The ellipsis in "Add to playlist…" is load-bearing: it's the only row action that opens a picker rather than acting immediately.

Tooltips sit **above** the button (`tooltip::Position::Top`) because the right-most actions are near the window edge, where a side-placed tooltip clips.

## Album cover grid vs list (`src/ui/widgets/album_grid.rs`)
The Albums list, Recently Added and Recently Played (Albums mode) all render through one shared widget, so they look and behave identically. `album_grid` exposes `tile()` (cover + title + subtitle + optional caption), `grid()`, `list_thumb()` (the list-mode cover), `layout_toggle()` (the Grid / List button — `icon::GRID` / `icon::LIST` plus a text label) and `art_for()` (cache lookup via `art_key_for`).
- **One flag for all three views**: `AppConfig::album_grid_view` (`#[serde(default)]`, persisted), toggled by `Message::ToggleAlbumGridView`. Deliberately not per-view — three independent layout memories would feel arbitrary.
- **List mode also shows art** in all three, via the shared `list_thumb()` (36px), so switching layouts changes the density and never *which* albums appear to have a cover.
- **`grid()` is a wrapping row** (`row(tiles).width(Fill).wrap()`), so the column count follows the window width. It used to chunk into a fixed 5 per row, which left a widening band of dead space to the right on any window wider than 5 tiles.
- **Now Playing's recents strip is a *horizontal scrollable*, not a wrapping row** — the one place where wrapping is wrong. All three of the obvious options fail differently: a plain `row` squeezes the overflow into its last child (the right-most cover rendered as a sliver), and `.wrap()` grows the strip downwards where, because a `column` doesn't clip its children, the second line drew straight over the player bar. A horizontal scrollable is the only one that stays exactly one row tall at any width.
- **`window::Settings::min_size` is 1000×700** (`main.rs`). iced widgets don't clip to their parent, so a too-small window doesn't degrade — it overlaps.
- Grid mode builds ~4 widgets per album with **no virtualisation** — on an 800-album library that is ~3200 widgets laid out every frame, which is untested for responsiveness.

### Background album-art fetching — two stages (`App::drain_art_queue`)
Art fetching is a **two-stage queue with bounded concurrency**, not a capped prefetch. The split between stages is the single most important thing in this section: **a local probe costs milliseconds, a MusicBrainz lookup costs 1.1–2.2 seconds of globally serialized throttle time**, so anything that lets them share a queue converts the whole sweep into a ~1 album/second crawl.
- **Stage 1, `fetch_album_art_local`** — MPD `readpicture` then `albumart`, no network. `art_queue: VecDeque<(artist, base, variant)>`, `ART_FETCH_CONCURRENCY` (3) in flight. Reports `ArtOutcome::Loaded` / `MpdMiss` / `Missing`.
- **Stage 2, `fetch_album_art_remote`** — MusicBrainz/CAA, keyed by the same `art_key` (which already *is* `artist\x1fbase`, i.e. the query). `mb_queue`, **one** in flight, and only started once `art_queue` is empty. Runs **outside `art_fetch_gate`** on purpose: it is already limited to 1 concurrent + ~1 req/s, and holding one of the gate's 4 permits for seconds would make the playing track's own cover queue behind background work.
- **`Message::AlbumArtFetched(key, ArtOutcome)`** is the queue's own message; `ArtLoaded` is now only for one-off fetches (playing track, artist images, recents). This replaced an earlier trick of inferring the source from `art_pending` membership — the stage split needs a real tri-state (`MpdMiss` is "not found yet", `Missing` is "final"), which a bare `Option<Vec<u8>>` can't carry.
- **Stage 2 is unbounded** — it grinds through every locally-artless album for as long as it takes. A 50/session budget was tried and removed: the thing that actually needed fixing was the *stall* (a lookup blocking the local sweep), not the number of lookups. Once the stages are separate, a slow background queue costs nothing visible, and every result caches permanently, so the work shrinks each session.
- **Stage 1 is deliberately NOT gated by the negative cache.** Local probing is cheap and a persisted negative would be wrong the moment a `cover.jpg` appears next to the music or a tag is fixed. Only stage 2 reads the negative — and stage 1 checks `is_known` at the *end*, on the miss path only, so an album MusicBrainz already answered "no" for doesn't burn a budget slot. (mikMPD lands in the same place from the other side: its `.miss` markers cover the whole chain, so they carry a 7-day TTL.)
- **`art_missing`** (session-local) exists because `art_handles` only records hits; without it every re-entry to a list re-queues every coverless album.
- **Enqueue is front-insertion, and re-queues albums already in the queue.** `enqueue_album_art` pulls matching entries out of `art_queue` and pushes the current view's list back on at the head (reversed, so on-screen order survives). Skipping already-queued albums instead — the obvious implementation — meant opening Recently Added mid-sweep put its covers behind the 800 albums still queued from the Albums list.
- `Message::SwitchServer` clears both queues, both pending sets, and zeroes both inflight counters (fetches still running then find their key absent from the pending sets, so they must not decrement). `art_handles`/`art_missing` stay — keyed by artist/album, so server-agnostic.
- **Known limitation**: no visible-range fetch. mikMPD gets this free from SwiftUI (`.task(id:)` per tile, cancelled on scroll away); iced 0.13 exposes no per-item scroll position, so the queue works from the top of the list down. Stage 1 finishing a whole library in seconds is what makes that acceptable.
- Everything goes through the queue, including `ArtistAlbumsLoaded`, which previously fired one task per album at once.

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
- **Both lyric branches must carry `.id(lyrics_scroll_id)`.** iced reuses widget state by *(tree position, widget type)* only — `scrollable::Id` is for targeting operations, not identity — so the synced and plain scrollables, which sit at the same tree path, share scroll state. The plain branch originally had no id: it inherited the synced branch's autoscrolled offset (near the bottom), opened scrolled past its own much shorter content, i.e. **blank**, and had no id to reset and no autoscroll to re-sync. `App::reset_lyrics_scroll()` snaps to `RelativeOffset::START` on track change and on entering Now Playing. `lyrics_autoscroll()` is additionally gated on `current_view == View::NowPlaying`, since off that view the operation walks the tree every 500ms to reach a widget that isn't there.
- `fetch_lyrics(song)` (`app.rs`) skips the request if the key is already in `self.lyrics`.

## Album Art Fetch Order (`src/mpd/client.rs`, `App::fetch_art`)
**Tag → cover file → internet.** `MpdClient::tag_art(uri)` (`readpicture`) is tried before `cover_file_art(uri)` (`albumart`) — tag art is probed first because on a tagged library it's the one that actually exists; probing the separate-cover-file path first would cost a wasted round trip per album on every well-tagged library. Both share one private `fetch_binary_art(verb, uri)` chunked-read loop. `App::fetch_art` sequences them, falling through to `MusicBrainzClient::fetch_album_art` only if both MPD paths miss.
- **`art_fetch_gate: Arc<Semaphore>`** (size 4, on `App`) bounds peak concurrent art fetches — acquired in `fetch_art`/`fetch_artist_art`/the `ArtistAlbumsLoaded` per-album loop before touching MPD or MusicBrainz. Without it, opening an artist with many uncached albums fires one unbounded fetch task per album.

## Outbound HTTP (`src/net.rs`, `Cargo.toml`)
Every outbound request — MusicBrainz, Cover Art Archive, Wikipedia, LRCLIB — goes through one place.
- **`reqwest` is pinned to `default-features = false` + `rustls-tls-native-roots`.** Default features mean `native-tls`, which is schannel on Windows, Security.framework on macOS and **OpenSSL on Linux** — so only the Linux build carried a system dependency (`openssl-devel` + `pkg-config` to build, a matching `libssl` soname to run, meaning a binary built on one distro could fail to start on another). rustls is the same pure-Rust stack everywhere. **`default-features = false` drops more than TLS**, so `json`, `charset`, `http2` and `macos-system-configuration` are re-added deliberately — without the last one macOS silently stops honouring system proxy settings. `native-roots` rather than `webpki-roots` so a corporate MITM proxy or a custom CA in the OS trust store keeps working. Verify with `cargo tree -i openssl-sys` (must not match); `openssl-probe` in `Cargo.lock` is a red herring — it's a pure-Rust crate that only *locates* certificate files.
- **`net::USER_AGENT` is the single User-Agent**, built from `CARGO_PKG_VERSION`. MusicBrainz requires a UA that identifies the application with real contact information and throttles or blocks clients without one; this was hardcoded to `winrmpc/0.1.0 (https://github.com/user/winrmpc)` — a version four releases stale and a URL identifying nobody. A unit test asserts the placeholder can't come back.
- **`net::client(purpose)` returns `Option<Client>`, never panicking.** The two clients previously disagreed: `musicbrainz.rs` used `.expect(...)` (took the app down) and `lrclib.rs` used `.unwrap_or_default()` (silently discarded the UA and timeout — and `Client::default()` is `Client::new()`, which panics anyway, so it wasn't even a real fallback). Both now hold `http: Option<Client>` and route every request through a private `get()` that turns "no client" and "request failed" into the same `None`, so exactly one place knows the client is optional. Failure logs at ERROR naming which lookups are unavailable.
- **`ALBUM_ART_DEADLINE` (45s) bounds a whole album's remote lookup**, not just one request. The 10s per-request timeout doesn't help when one album issues the `search_queries` ladder, then a release-group lookup, then the CAA image fetch, each behind a ~1.1s throttle wait. Stage 2 of the art queue is one-at-a-time and unbounded by design, so one pathological album could stall the whole background sweep.

## Wikipedia / MusicBrainz (`src/art/musicbrainz.rs`)
- **`MusicBrainzClient`** is `Clone` and holds a `Store` (for MBID/negative caching) plus a `MusicBrainzThrottle`; `App` constructs **one** instance (`self.mb_client`) and every fetch site clones it — do not `MusicBrainzClient::new(...)` ad hoc, it defeats connection pooling.
- **`MusicBrainzThrottle`**: an `Arc<Mutex<Option<Instant>>>`-backed rate limiter serializing MusicBrainz calls to ~1 req/s **globally** across every clone/task, not per-call-chain like a local `sleep` would. Replaces the old per-method `sleep(1100ms)`.
- **MusicBrainz ID caching**: `search_artist`/`search_release_group` check the redb `mb_ids` table (`"artist:{name}"` / `"{artist}\x1falbum"` → MBID, `Some(None)` = confirmed no match) before hitting the network — both the art path and the bio path resolve the same entity's MBID, so this cache removes a duplicate search per artist/album visit.
- **Artist bio**: 1) MusicBrainz Wikipedia URL relation; 2) suffix fallback `["(band)", "(musician)", …]`; 3) Wikipedia's own search API (`search_wikipedia`) as a last resort.
- **Album bio**: same three-step shape, keyed on the release-group. Album lookup titles go through `strip_edition_qualifier` first (strips a trailing `[24-bit Remaster]`/`(Deluxe Edition)`-style bracket) — lookup-only, never touches the art cache key.
- **External-lookup string handling is ported from mikMPD** (`normalizedForLookup` / `luceneEscape` / `albumLookupTitle` / result validation), which had a round of real-library fixes winrmpc never got:
  - **`lucene_escape`** backslash-escapes `\ " + - ! ( ) { } [ ] ^ ~ * ? : /`. It replaced a `sanitize` that *deleted* brackets and quotes and ignored the rest — including `/`, which opens a Lucene regex, so **every `AC/DC` lookup had been sending a malformed query**.
  - **`normalize_for_lookup`** folds ellipsis, smart quotes and en/em dash/minus, collapses whitespace runs (a real tag here reads `"Blue  Oyster Cult"` with two spaces), and moves a sort-order article to the front (`"Beatles, The"` → `"The Beatles"`). Applied to both sides of every lookup — the query *and* the candidate title. `title_matches` is already immune (it tokenizes on non-alphanumerics) but query strings and `is_music_article`'s substring test are not. Uses `str::get` for the article check: indexing from the end panics on a multi-byte tail.
  - **`is_edition_qualifier`** matches whole tokens, not substrings — the old `lower.contains("bit")` also fired on "Rabbit". It additionally recognizes a year (1900–2099), an audio spec (`24-bit`, `96 kHz`), a catalogue number (4+ digit run, `VICP-60852`), and a short **all-caps** token as a region/format marker (`[UK]`, `[US]`, `[EP]` — this library's whole Depeche Mode discography is tagged `… [UK]`), while a short digit run keeps `(Part 2)`/`(Volume 3)` intact and mixed case keeps `(Rain)`.
  - **`artist_credit_matches` compares two fingerprints**, accepting either: diacritics **folded** to ASCII (`Motörhead` ↔ `Motorhead`), and non-ASCII letters **dropped** entirely. The second exists for mis-encoded tags, where the two sides disagree about *which* accented letter it is — this library holds `"Blue Îyster Cult"`, mojibake of `"Blue Öyster Cult"`, where folding gives i-vs-o and still misses but dropping leaves `blueystercult` both ways. Length-guarded at 6 so short names can't collide. `lookup_title` now loops until stable, so stacked suffixes (`"Album (Deluxe Edition) [2011 Remaster]"`) fully strip.
  - **MusicBrainz results are validated, not just scored.** `release_title_matches` (normalized containment or `title_matches`) plus `artist_credit_matches` (letters-only, containment either way, so `ACDC` matches `AC/DC`) filter every candidate. Previously the top-scoring hit was taken unchecked, which is how "Best of the Doors" could come back with the debut album's cover.
  - **`search_queries`** builds mikMPD's three-step ladder — exact quoted, unquoted/tokenized, album-only — and every result is validated regardless of which query found it, so loosening the query can't loosen correctness. With no artist known, only the album-only form is issued.
- **`try_bio_candidate(title, target)`**: fetches the summary, then accepts it if `title_matches(title, target)` (word-token overlap ≥2/3, mirrors mikMPD's `titleTokensMatch`) wins immediately, else falls back to the weaker `is_music_article(text, name)` keyword check. Used by the artist path at every candidate stage.
- **The album bio path is stricter than that, and has to be** (also ported from mikMPD's `WikipediaService`):
  - **`album_result_matches(title, extract, album, artist)`** gates every candidate. Token overlap counts only toward the *title*; the **extract must contain the album name outright** (a sequel or sibling compilation cites enough of the words to fool a token match) **and must name the artist**. That artist half didn't exist before — a search hit for a different artist's same-titled album passed on "mentions music" alone.
  - **`is_generic_album_title`** — a curated set ("greatest hits", "gold", "live", "best of", "unplugged", …). mikMPD's comment explains why it's a list and not a token-count heuristic: a count also catches distinctive short titles like Depeche Mode's "101". When it fires: the plain-title step is skipped (unless the album name carries the artist), the artist-free search query is dropped, and the extract-only fallback is suppressed.
  - **Naming-pattern hits are validated too, not taken on trust.** `"{album} (album)"` looks self-validating and isn't: **`Greatest Hits (album)` redirects to Wikipedia's article about the *concept* of a greatest-hits record**, which was duly served as Bob Dylan's album bio until every step-2 hit went through `album_result_matches`. Verified by `live_wikipedia_bios_for_awkward_tags`.
- **`fold_for_compare`** is the shared comparison form (punctuation folded, lowercased, diacritics folded) behind `title_matches`, `is_music_article`, `release_title_matches` and `album_result_matches`. Diacritic folding is what makes the Wikipedia path as forgiving as the MusicBrainz one: article titles keep their accents ("Motörhead", "Blue Öyster Cult") where tags don't, and `title_matches` requires an *exact* match for a single-token target — so every accented one-word band name used to fail its own article.
- **`fetch_artist_bio` normalizes its input** like the album path. Without it a sort-order tag ("Alan Parsons Project, The") was fed to the title guesses, the search query *and* the match target — three chances to fail on one unfolded string.
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
**`src/icon_design.rs` is the single generator** — a dark-navy circle with three cyan equalizer bars, drawn procedurally on a 32-unit grid so every size is free and no source image exists. It holds `APP_ID`, the colours, `rgba_pixels(size)` and `svg()`. Three consumers share it, two of them via `include!` because neither a build script nor an example can `use` a crate module:
- `src/icon.rs` — thin iced wrapper (`make_icon()`, 32×32) + the icon tests.
- `build.rs` — `include!`s it, then does the *only* Windows-specific work: row flip + RGBA→BGRA swizzle + the BMP-in-ICO container, embedded via `winres` (`winres = "0.1"` in `[build-dependencies]`).
- `examples/emit_icons.rs` — `include!`s it and writes `packaging/linux/icons/hicolor/**` (PNG at 8 sizes + scalable SVG).

**Two constraints on `icon_design.rs`, both load-bearing**: `std` only (no `iced`, no `image`, no `use crate::…`), and **no inner `//!` doc comments anywhere in the file** — `include!` splices it mid-file where inner docs are a hard `E0753` error.

**What the runtime icon actually does, per platform** — it is not uniform, and the two no-ops are why `packaging/` exists:

| Platform | `window::Settings.icon` |
|---|---|
| Windows | Works; `build.rs` also embeds the ICO for Explorer/taskbar |
| Linux / X11 | Works (`_NET_WM_ICON`) |
| Linux / Wayland | **No-op** — `winit .../wayland/window/mod.rs:433` is an empty fn |
| macOS | **No-op** — `winit .../macos/window_delegate.rs:1541`, documented |

Wayland resolves an icon by matching the surface's `app_id` to a `.desktop` basename, so `main.rs` sets `platform_specific.application_id` from `icon_design::APP_ID` (`io.github.mickegris.winrmpc`). `PlatformSpecific` is a **different type per OS**, hence the cfg-split `platform_specific()` helper next to `window_settings()`. That id must stay byte-identical in three places — the `app_id`, the `.desktop` filename, and the icon filenames — or the icon silently doesn't resolve. **macOS has no packaging yet** and therefore still no icon; see `docs/plans/app-icon-cross-platform.md`.

The generated PNGs **are committed** (packagers shouldn't need a Rust toolchain), and `icon::tests::packaged_png_assets_match_the_generator` keeps them honest by decoding each one and comparing *pixels* — not encoded bytes, so an `image` upgrade can't fail it spuriously. Change the design → run `cargo run --example emit_icons`, or `cargo test` fails and tells you to.

## Planning Docs (`docs/plans/`)
Design docs written before implementing a feature — read the relevant one before starting related work, and add new ones there for anything non-trivial. `mikmpd-parity-overview.md` tracks the gap between winrmpc and its sibling iOS client [mikMPD](https://github.com/mickegris/mikMPD) (`../mikMPD`), with one linked plan file per gap (queue editing, multi-disc album grouping, recently-added/played history, server stats & diagnostics, Snapcast control, Now Playing quick controls). `server-discovery.md` is kept only as a record — that feature was **removed** in 0.4.1. `playlists.md` and `enhancements.md` (playlists, MPD log, lyrics) are earlier plans from this same parity effort — already shipped. Others not covered by the parity overview: `art-wikipedia-fetch-order-and-caching.md` (the tag → cover-file → internet order and the fetch gate), `local-database.md` (the redb store), `review-fixes-correctness.md` / `review-fixes-performance.md` (the two code-review rounds, including the deferrals listed in `docs/status.md`), and `iced-0.14-migration.md` — whose **§9 post-mortem is the part that matters**: the migration was tried and reverted, and §§0–8 predate that.

`cross-platform-and-ui-0.4.2.md` is the newest umbrella (targeting 0.4.2), linking five plans: `app-icon-cross-platform.md`, `persistent-storage-cross-platform.md`, `current-song-highlighting.md`, `row-action-affordance.md`, `network-fetch-cross-platform.md`. **Read that umbrella before touching fonts, glyphs, the window icon, `ProjectDirs`, or the `reqwest` features** — three of the five findings are one recurring mistake (a Windows-only resource named directly, silently falling back to nothing on macOS/Linux). The `Segoe UI Symbol` reference in `ui/widgets/link.rs` was the load-bearing example — the fallback font was the one its own comment said renders tofu — and is **fixed**: see "Icon font + row actions" above. Every claim in those plans is sourced from the vendored `iced`/`winit` crates with file:line references and **none is yet verified on real macOS or Linux hardware** — each plan's "How to confirm on the real OS" section is the acceptance criterion.

## Current Version
`0.4.1` — see `Cargo.toml`. There are `release` and `ship` skills that automate the release/merge flow — prefer them over doing the steps by hand.

`Cargo.lock` **is committed** (`.gitignore` has `*.lock` with a `!Cargo.lock` exception). This is a binary crate, so the lockfile belongs in version control: without it every machine resolves its own versions, builds aren't reproducible, and a bad upstream patch release can't be pinned back.

## Session state
**`docs/status.md`** — where the current branch stands, what's verified against a real server, what's still unverified, and the suggested next steps. That file is the one that goes stale; keep it there rather than here, and update it at the end of a working session.
