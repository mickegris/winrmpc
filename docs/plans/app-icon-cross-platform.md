# App icon on macOS and Linux

Part of [cross-platform-and-ui-0.4.2](cross-platform-and-ui-0.4.2.md).

## What exists now

Two independent generators draw the same design — a dark navy circle
(`#1a1a2e`) with three cyan equalizer bars (`#4fc3f7`):

- **`src/icon.rs`** — `make_rgba(32)` → top-down RGBA → `make_icon()` →
  `iced::window::icon::from_rgba(..., 32, 32)`, handed to
  `iced::window::Settings.icon` in `src/main.rs`.
- **`build.rs`** — `make_bgra(16)`/`make_bgra(32)` → bottom-up BGRA → a
  hand-assembled BMP-in-ICO → `winres`, embedded as a Windows resource.
  Guarded by an early `return` unless `CARGO_CFG_TARGET_OS == "windows"`.

The two generators are currently consistent (byte order and row order both
differ correctly, and both colour constants are right), but they are copy-paste
siblings, so any future tweak has to be made twice.

## The finding

`window::Settings.icon` reaches winit's `set_window_icon`
(`iced_winit-0.13.0/src/conversion.rs:35`, and again on
`program.rs:1342`). What winit does with it depends entirely on the platform:

| Platform | Behaviour | Evidence |
|---|---|---|
| Windows | Works | plus `build.rs` embeds the ICO for Explorer/taskbar |
| Linux / **X11** | Works — sets `_NET_WM_ICON` | — |
| Linux / **Wayland** | **No-op.** Empty function body | `winit-0.30.13/src/platform_impl/linux/wayland/window/mod.rs:433` — `pub(crate) fn set_window_icon(&self, _window_icon: Option<PlatformIcon>) {}` |
| **macOS** | **No-op.** Documented as such | `winit-0.30.13/src/platform_impl/macos/window_delegate.rs:1541` — *"macOS doesn't have window icons"* |

So on the two platforms the user asked about, the runtime icon path does
nothing at all. Each needs a different mechanism.

### Wayland needs an `app_id` that matches a `.desktop` file

Wayland has no per-window icon protocol. The compositor finds the icon by
matching the surface's `app_id` against the basename of an installed
`.desktop` file. iced does expose this — `window::Settings.platform_specific`
is `iced_core::window::settings::linux::PlatformSpecific`, whose
`application_id: String` is passed to winit's `with_name` for **both** X11
(WM_CLASS) and Wayland (app_id), at `iced_winit-0.13.0/src/conversion.rs:113`,
`:114`, `:122`, `:123`.

`src/main.rs` currently writes `..Default::default()` for the whole
`window::Settings`, so `application_id` is `""`. That means:

- **Wayland**: no icon, and the window groups badly in docks/switchers.
- **X11**: WM_CLASS is empty as well — the `_NET_WM_ICON` path still works, so
  the icon shows, but application matching is still broken.

### macOS needs an `.app` bundle

macOS takes its Dock and Finder icon from the bundle: an `.icns` file in
`YourApp.app/Contents/Resources/`, named by `CFBundleIconFile` in
`Contents/Info.plist`. A bare Mach-O binary run from a terminal gets the
generic executable icon and there is no API to change that. This is not an
iced or winit limitation to work around — it's how the platform works.

## Plan

### A. Single source for the icon design

Stop maintaining two copies of the same drawing. Make `src/icon.rs` the one
generator, parameterised by size, and have `build.rs` pull it in with
`include!("src/icon.rs")` (build scripts can't `use` crate modules, but
`include!` works and keeps one definition). `build.rs` keeps only the
format-specific parts: the RGBA→BGRA swizzle, the bottom-up row flip, and the
ICO/BMP container assembly.

Add a `rgba_pixels_at(size: u32) -> Vec<u8>` so callers can ask for the sizes
each platform wants. The design is procedural, so every size is free — this is
the reason not to check in binary assets.

### B. Set `application_id` on Linux

In `src/main.rs`'s `window::Settings`:

```rust
platform_specific: iced::window::settings::PlatformSpecific {
    application_id: "io.github.mickegris.winrmpc".into(),
    ..Default::default()
},
```

`PlatformSpecific` is a different type per OS, so this needs a
`#[cfg(target_os = "linux")]` split — either two `window::Settings`
constructions or a small helper returning the whole `Settings`. Prefer the
helper; the settings block already carries a long comment about `min_size`
that shouldn't be duplicated.

**The id must match the `.desktop` basename exactly**, and once chosen it is
awkward to change (it is what compositors and docks key off). Reverse-DNS
(`io.github.mickegris.winrmpc`) is the freedesktop convention and is what
Flatpak/AppStream would require later. The plain `winrmpc` form is also legal
and simpler. Recommendation: reverse-DNS, and note the three places it must
stay in sync — `main.rs`, the `.desktop` filename, and the installed icon
filenames.

Note this is **deliberately not** the same string as `directories`' macOS
bundle id (`com.winrmpc.winrmpc`, from
`ProjectDirs::from("com", "winrmpc", "winrmpc")` in `config/settings.rs:209`).
Changing that one would move the user's existing config and cache directories;
see [persistent-storage-cross-platform](persistent-storage-cross-platform.md).

### C. Linux packaging assets

Add `packaging/linux/`:

- `io.github.mickegris.winrmpc.desktop` — `Name`, `Exec=winrmpc`,
  `Icon=io.github.mickegris.winrmpc`, `Categories=AudioVideo;Audio;Player;`,
  `Type=Application`.
- A small `xtask`-style binary or a `--emit-icons <dir>` debug flag on the app
  itself that writes `io.github.mickegris.winrmpc.png` at 16/24/32/48/64/128/
  256/512 for `share/icons/hicolor/<size>x<size>/apps/`. Generating from
  `rgba_pixels_at` avoids checking in binaries, and the `image` crate (already
  a dependency) can encode the PNGs.
- `install.sh` copying the binary to `~/.local/bin`, the desktop file to
  `~/.local/share/applications`, and the icons into `~/.local/share/icons`,
  then `update-desktop-database`/`gtk-update-icon-cache` if present.

The 32×32 runtime icon should also grow — `make_icon()` hardcodes 32×32, which
is what X11 gets. Passing a larger size (128 or 256) gives sharper results on
HiDPI X11 sessions at no cost.

### D. macOS packaging assets

Add `packaging/macos/`:

- `Info.plist` with `CFBundleName`, `CFBundleIdentifier`, `CFBundleExecutable`,
  `CFBundleIconFile`, `CFBundleShortVersionString` (kept in step with
  `Cargo.toml`), and `NSHighResolutionCapable`.
- `bundle.sh` — builds `--release`, lays out `winrmpc.app/Contents/{MacOS,
  Resources}`, generates the iconset PNGs (16/32/128/256/512 at 1× and 2×) via
  the same emitter as the Linux icons, and runs `iconutil -c icns` to produce
  `winrmpc.icns`. `iconutil` ships with macOS, so the script is macOS-only by
  nature — that's fine, it only ever runs there.

An alternative is the `icns` crate, which would let the `.icns` be produced on
any host (useful if a CI job cross-builds). Only worth it if that CI job
appears; `iconutil` is simpler otherwise.

Unsigned bundles trip Gatekeeper on first launch (right-click → Open, or
`xattr -d com.apple.quarantine`). Worth a README line; actual signing/
notarisation needs an Apple Developer account and is out of scope.

### E. Release flow

`.claude/skills/release/SKILL.md` currently uploads the Windows `.exe` only.
Once C and D exist, the release should also carry a Linux tarball (binary +
desktop file + icons + install.sh) and a zipped `winrmpc.app`. Both need a
build on that OS, so unless CI is added this stays a manual step — call that
out in the skill rather than leaving it implicit.

## Scope call

A–B are small and land in 0.4.2 comfortably. C–D are packaging work with no
existing home in this repo, and E depends on them. If 0.4.2 needs to ship
before that, **A and B alone are still worth shipping**: B fixes the Wayland
icon outright (given the user installs a desktop file by hand), and neither
changes behaviour on Windows.

## How to confirm on the real OS

- **Wayland (before)**: run the app under a Wayland session, check the dock /
  Alt-Tab switcher shows a generic placeholder. `swaymsg -t get_tree` or GNOME
  Looking Glass will show an empty `app_id`.
- **Wayland (after B + C)**: `app_id` reads `io.github.mickegris.winrmpc` and
  the icon appears once the `.desktop` and hicolor PNGs are installed.
- **X11**: `xprop WM_CLASS` on the window — currently `"", ""`, should become
  the application id. The icon itself should be unaffected either way.
- **macOS (before)**: the Dock shows the generic terminal-executable icon.
- **macOS (after D)**: launching `winrmpc.app` shows the equalizer icon in the
  Dock and in Finder.
- **Windows**: must be unchanged — the `build.rs` guard and the ICO path are
  untouched. Confirm the taskbar and Explorer icons still render.
