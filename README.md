# winrmpc

A modern, native MPD (Music Player Daemon) client built in Rust with the [iced](https://iced.rs/) GUI framework. Runs without a console window; all diagnostic output is accessible through the built-in **Log** view.

## Cross-platform

winrmpc began as a Windows-only project (hence the name — **win**dows **r**ust **mpc**), but the goal is for it to run on every major desktop OS. The codebase is written against cross-platform crates (iced, tokio, directories), so it should build and run fine on **Linux** and **macOS** as well — only the icon embedding and console-hiding are Windows-specific (and are cleanly guarded behind `cfg(target_os = "windows")`).

The name stays `winrmpc` regardless of platform — consider the `win` a historical artifact, not a limitation.

## Features

### Music Playback & Control
- Full transport controls: play, pause, stop, previous, next, seek
- Volume control with slider
- Repeat, random, single, and consume mode toggles
- Real-time progress bar with elapsed/total time display
- Now Playing view with album art

### Library Management
- **Artist browsing** with album listings and artist art fetched from MusicBrainz
- **Album browsing** with cover art, track listings, and total duration
- **Genre browsing** with drill-down into albums per genre
- **File/folder browser** — navigate your MPD music directory tree directly
- **Search** — full-text search across your library

### Album & Artist Art
- Art fetched from MPD embedded tags (FLAC, ALAC, MP3, etc.)
- Fallback to **MusicBrainz** and **Cover Art Archive** for album covers
- Artist images sourced from MusicBrainz
- All art cached to disk — fast on subsequent loads

### Wikipedia Integration
- Artist biographies and album descriptions fetched from English Wikipedia via MusicBrainz URL relations
- Expandable info boxes on artist and album detail views

### CD Playback
- Play whole disc or individual tracks
- **Load Tracks** probes the disc and lists tracks with durations
- Optional CD device path in Settings (e.g. `/dev/sr0`) for direct lsinfo support
- Does not request album art for CD tracks, preventing MPD lockups

### Radio
- Built-in Swedish Radio streams (SR P1, P2, P3)
- Add and remove custom stream URLs

### Partitions (Multi-Room Support)
- List, create, and delete MPD partitions
- Switch between partitions; selected partition persists across restarts
- Move audio outputs between partitions

### Audio Outputs
- View all configured MPD audio outputs
- Enable/disable outputs individually
- Move outputs between partitions

### Log View
- All application events visible inside the app under **Log** (below Settings)
- No console window required — runs cleanly as a background-free desktop app
- Clear button to reset the log

## Screenshots

<a href="assets/nowplaying.jpg">
  <img src="assets/nowplaying.jpg" width="200" />
</a>
<a href="assets/album.jpg">
  <img src="assets/album.jpg" width="200" />
</a>
<a href="assets/artist.jpg">
  <img src="assets/artist.jpg" width="200" />
</a>
<a href="assets/queue.jpg">
  <img src="assets/queue.jpg" width="200" />
</a>
<a href="assets/search.jpg">
  <img src="assets/search.jpg" width="200" />
</a>
<a href="assets/partitions.jpg">
  <img src="assets/partitions.jpg" width="200" />
</a>
<a href="assets/outputs.jpg">
  <img src="assets/outputs.jpg" width="200" />
</a>

## Building from Source

All platforms need **Rust** (stable). Install it from [https://rust-lang.org/tools/install](https://rust-lang.org/tools/install). Then follow the prerequisites for your OS and run the common build step below.

### Windows

Rust on Windows requires the MSVC C++ build tools for linking.

**Option A: Visual Studio Build Tools (smaller download)**
1. Download [Build Tools for Visual Studio 2022](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
2. Run the installer, check **"Desktop development with C++"**, click Install (~1-2 GB)

**Option B: Full Visual Studio**  
Open the **Visual Studio Installer**, click **Modify**, ensure **"Desktop development with C++"** is checked.

> **Note:** Visual Studio *Code* is a different product and does not include the required build tools.

When the Rust installer asks, choose **option 1 (default)**, which selects the `x86_64-pc-windows-msvc` target. If you later hit linker errors, verify the toolchain:
```bash
rustup show          # expect stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc   # if it shows -gnu instead
```

### Linux

Install a C toolchain plus the development headers iced needs for graphics, fonts, and the clipboard. On Debian/Ubuntu:
```bash
sudo apt install build-essential pkg-config \
  libfontconfig1-dev libxkbcommon-dev \
  libwayland-dev libx11-dev
```
On Fedora:
```bash
sudo dnf install gcc pkg-config \
  fontconfig-devel libxkbcommon-devel \
  wayland-devel libX11-devel
```
Rendering uses `wgpu` (Vulkan/GL), so a working GPU driver or a software fallback (e.g. `mesa`) is required at runtime.

### macOS

Install the Xcode Command Line Tools (provides the C toolchain and system frameworks):
```bash
xcode-select --install
```
No other dependencies are needed — Metal is used for rendering.

### Build (all platforms)

```bash
git clone https://github.com/mickegris/winrmpc.git
cd winrmpc
cargo build --release
```

The first build downloads all dependencies and compiles everything (several minutes). Subsequent builds are incremental and much faster.

The compiled binary:
```
target/release/winrmpc        # winrmpc.exe on Windows
```

For development builds (faster compilation, slower runtime):
```bash
cargo build
```

### Running tests

```bash
cargo test                 # run the whole suite
cargo test escape          # run tests whose name matches "escape"
cargo test parse_status -- --exact   # run one specific test
```

The suite is pure-logic (command escaping, protocol parsing, type formatting) and needs no running MPD server.

## Configuration

On first launch winrmpc connects to MPD at `127.0.0.1:6600`. Use the **Settings** view (bottom of the sidebar) to change the connection and optionally set a CD device path.

Configuration file:
```
%APPDATA%\winrmpc\winrmpc\config\config.toml
```

Album art cache:
```
%LOCALAPPDATA%\winrmpc\winrmpc\cache\
```

### Example config.toml

```toml
mpd_host = "192.168.1.50"
mpd_port = 6600
# mpd_password = "your_password"
# cd_device = "/dev/sr0"
art_cache_size_mb = 500

[theme]
dark_mode = true
accent_color = "#4fc3f7"
```

## License

MIT

## Acknowledgments

- [iced](https://iced.rs/) — the GUI framework
- [MusicBrainz](https://musicbrainz.org/) — album and artist metadata
- [Cover Art Archive](https://coverartarchive.org/) — album cover art
- [Wikipedia](https://en.wikipedia.org/) — artist and album biographies
