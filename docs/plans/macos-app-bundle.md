# macOS `.app` bundle

Part of [ui-0.4.3](ui-0.4.3.md). Finishes steps **D** and **E** of
[app-icon-cross-platform](app-icon-cross-platform.md), which shipped A–C in
0.4.2 and left macOS with no packaging at all.

## What exists now

- **macOS has no icon, at all.** `window::Settings.icon` is a documented no-op
  on macOS (`winit .../macos/window_delegate.rs:1541`), so the only way to give
  the app an icon there is an `.app` bundle with an `.icns` in it. There is no
  bundle.
- **macOS ships no binary.** The 0.4.2 release carries a Windows `.exe` and a
  Linux tarball; a Mac user has to build from source.
- **CI already runs on macOS.** `.github/workflows/release.yml` tests on
  `macos-latest` and always has — it just doesn't build anything there.

That last point resolves a question the original plan left open. It said the
`icns` crate would only be worth it "if a CI job cross-builds" and that
`iconutil` was simpler otherwise. **CI now exists and runs on real macOS**, so
`iconutil` — which ships with macOS — is available, and no extra crate is
needed.

## Bundle layout

```
winrmpc.app/
  Contents/
    Info.plist
    MacOS/winrmpc            ← the binary
    Resources/winrmpc.icns
```

`Info.plist` needs `CFBundleName`, `CFBundleIdentifier` (**`io.github.mickegris.winrmpc`
— the same `icon_design::APP_ID` the Linux `.desktop` and Wayland `app_id` use**),
`CFBundleExecutable`, `CFBundleIconFile`, `CFBundlePackageType` (`APPL`),
`CFBundleShortVersionString` + `CFBundleVersion`, and `NSHighResolutionCapable`.

**`CFBundleShortVersionString` must track `Cargo.toml`.** Generate the plist in
the build script from the crate version rather than committing a literal, or it
will silently say 0.4.2 forever — the same class of staleness as the MusicBrainz
User-Agent that 0.4.2 fixed.

## The icon

`src/icon_design.rs` already generates any size procedurally
(`rgba_pixels(size)`), and `examples/emit_icons.rs` already writes the Linux
PNG set from it. The `.icns` path is the same generator plus two steps:

1. Emit an `.iconset` directory — `icon_16x16.png`, `icon_16x16@2x.png`, …
   through `icon_512x512@2x.png`. That's sizes 16/32/128/256/512 at 1× and 2×,
   i.e. pixel sizes 16, 32, 64, 256, 512, 1024. Note `HICOLOR_SIZES` tops out at
   512, so **1024 is new** — worth confirming the generator's geometry still
   looks right there (it's procedural on a 32-unit grid, so it should).
2. `iconutil -c icns winrmpc.iconset -o winrmpc.icns` on the runner.

Extending `emit_icons.rs` with an `--iconset <dir>` mode keeps one generator
feeding all three platforms, which is the property that made the Linux icons
testable.

**Whether to commit the `.icns`** the way the Linux PNGs are committed: the
Linux ones are committed so packagers need no Rust toolchain. Nobody packages
a Mac `.app` from outside CI, and `.icns` is a binary blob that no test can
meaningfully diff. Recommend **generating it in CI and not committing it**.

## Architecture

`macos-latest` runners are **Apple Silicon**. Building only there produces an
arm64-only app that won't run on an Intel Mac.

Options: ship arm64 only (simplest, excludes older Macs); or build both targets
and `lipo -create` them into a universal binary (`rustup target add
x86_64-apple-darwin`, then two `cargo build --release --target …` runs). The
universal binary roughly doubles the ~22 MB download.

**Recommend universal.** It is about fifteen lines of CI and removes a whole
class of "it won't open" reports.

## Gatekeeper — the part that will actually generate complaints

**An unsigned, un-notarised `.app` downloaded from the internet will not open by
double-click on any current macOS.** It carries the `com.apple.quarantine`
attribute, and Gatekeeper reports it as *damaged* or *from an unidentified
developer* — wording that reads like a corrupt download, not a policy decision.

The workarounds are right-click → Open (then confirm), or
`xattr -dr com.apple.quarantine /Applications/winrmpc.app`.

Signing and notarising properly needs a **paid Apple Developer account ($99/yr)**,
a Developer ID certificate, and a notarisation step in CI with credentials in
repository secrets. That is a real cost and a real decision, and it is **not
something this plan should assume**.

So the plan is: **ship unsigned, and document it honestly and prominently** — in
the README, in the release notes, and ideally in the artifact filename or a
`README-macos.txt` inside the zip. A user who hits "damaged" with no explanation
concludes the download is broken.

## Distribution format

- **Zip the `.app`** — simplest, works, `ditto -c -k --keepParent` preserves the
  bundle correctly (a plain `zip` can mangle symlinks and resource forks).
- **`.dmg`** — the Mac convention, with a drag-to-Applications window. Nicer,
  needs `create-dmg` or `hdiutil` scripting, and buys little for an unsigned app
  that will need a right-click anyway.

**Recommend a zip** built with `ditto`, named
`winrmpc-vX.Y.Z-macos-universal.app.zip`.

## Plan

1. `packaging/macos/Info.plist.in` with `@VERSION@` substituted at build time.
2. `--iconset` mode in `examples/emit_icons.rs`; confirm 1024px renders.
3. `packaging/macos/bundle.sh` — lays out the `.app`, runs `iconutil`, `lipo`s
   the two targets, `ditto`s the zip. macOS-only by nature, which is fine.
4. A `build (macos)` job in `release.yml` producing the zip; attach it in
   `publish` alongside the existing two.
5. README + release-notes wording for the Gatekeeper step.
6. Update `.claude/skills/release/SKILL.md` — it currently promises exactly two
   assets and step 9 says to expect exactly two names.

## Explicitly not doing

- **Signing or notarisation.** Needs a paid account; revisit if one exists.
- **A Homebrew cask.** Wants a stable release cadence and a signed app first.
- **App Store distribution.** Sandboxing an MPD client that talks to arbitrary
  hosts on the LAN is its own project.

## How to confirm

Needs a Mac; none of it is verifiable from Linux.

- The `.app` **launches by double-click** after the quarantine step, and shows
  the winrmpc icon in the Dock and in ⌘-Tab — that icon is the entire point of
  the exercise, and it is the one thing that has never worked on macOS.
- The icon is crisp on a Retina display (this is what the `@2x` entries and
  `NSHighResolutionCapable` are for).
- `file winrmpc.app/Contents/MacOS/winrmpc` reports **both** architectures.
- The app finds its config at
  `~/Library/Application Support/com.winrmpc.winrmpc/` — note this is the
  `ProjectDirs` id, which is deliberately **not** the same string as
  `CFBundleIdentifier`. Changing either to match the other would orphan every
  existing install's settings, which
  [persistent-storage-cross-platform](persistent-storage-cross-platform.md)
  already ruled out.
