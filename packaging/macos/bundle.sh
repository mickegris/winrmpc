#!/usr/bin/env bash
# Build winrmpc.app — a macOS application bundle.
#
# macOS-only by nature: it needs `iconutil` (ships with macOS), `lipo` and
# `ditto`. That is fine — it only ever runs there, in CI's macos job or on a
# developer's Mac.
#
#   ./packaging/macos/bundle.sh              # universal, into ./dist
#   ./packaging/macos/bundle.sh --arm64-only # skip the x86_64 half
#
# Why a bundle at all: `window::Settings.icon` is a documented no-op on macOS
# (winit's window_delegate.rs), so an .icns inside a bundle is the *only* way
# the app gets a Dock icon there.
#
# The result is UNSIGNED. See the Gatekeeper note at the end.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$REPO/dist}"
APP="$DIST/winrmpc.app"
ARM64_ONLY=0
[[ "${1:-}" == "--arm64-only" ]] && ARM64_ONLY=1

VERSION="$(grep -m1 '^version' "$REPO/Cargo.toml" | cut -d'"' -f2)"
echo "==> winrmpc $VERSION"

for tool in iconutil ditto; do
    command -v "$tool" >/dev/null || { echo "error: $tool not found (macOS only)" >&2; exit 1; }
done

# --- binary -----------------------------------------------------------------
cd "$REPO"
if [[ $ARM64_ONLY -eq 1 ]]; then
    cargo build --release --target aarch64-apple-darwin
    BIN="target/aarch64-apple-darwin/release/winrmpc"
else
    # macos-latest runners are Apple Silicon, so an unqualified build produces
    # an arm64-only binary that will not run on an Intel Mac. Universal costs
    # about fifteen lines and removes a whole class of "it won't open".
    rustup target add x86_64-apple-darwin aarch64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    mkdir -p "$DIST"
    lipo -create \
        target/aarch64-apple-darwin/release/winrmpc \
        target/x86_64-apple-darwin/release/winrmpc \
        -output "$DIST/winrmpc-universal"
    BIN="$DIST/winrmpc-universal"
fi

# --- icon -------------------------------------------------------------------
ICONSET="$DIST/winrmpc.iconset"
rm -rf "$ICONSET"
# Same procedural generator as the window icon and the Linux PNGs — one design,
# three platforms, no source image to drift from.
cargo run --release --example emit_icons -- --iconset "$ICONSET"
iconutil -c icns "$ICONSET" -o "$DIST/winrmpc.icns"

# --- lay out the bundle -----------------------------------------------------
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/winrmpc"
chmod +x "$APP/Contents/MacOS/winrmpc"
cp "$DIST/winrmpc.icns" "$APP/Contents/Resources/winrmpc.icns"
sed "s/@VERSION@/$VERSION/g" "$REPO/packaging/macos/Info.plist.in" \
    > "$APP/Contents/Info.plist"

# --- zip --------------------------------------------------------------------
# ditto, not zip: a plain zip mangles symlinks and resource forks inside a
# bundle.
ZIP="$DIST/winrmpc-v$VERSION-macos-universal.app.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

rm -rf "$ICONSET"
echo "==> $APP"
echo "==> $ZIP"
cat <<'EOF'

NOTE: this bundle is unsigned and un-notarised. macOS will refuse to open it by
double-click and may report it as "damaged" — that is Gatekeeper's wording for
an unsigned download, not a corrupt file. To run it:

    xattr -dr com.apple.quarantine /Applications/winrmpc.app

or right-click the app and choose Open, then confirm.

Signing properly needs a paid Apple Developer account and a notarisation step;
see docs/plans/macos-app-bundle.md.
EOF
