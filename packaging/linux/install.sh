#!/usr/bin/env sh
#
# Install winrmpc's desktop entry and icons (and optionally the binary itself)
# into an XDG icon/application path so the app gets a real icon in launchers,
# docks and window switchers.
#
#   ./install.sh                 install for the current user (~/.local)
#   PREFIX=/usr/local ./install.sh    install system-wide (needs root)
#   ./install.sh --uninstall     remove everything this script installed
#
# Why this is needed at all: Wayland has no per-window icon protocol. The
# compositor matches a window's app_id against an installed .desktop file and
# takes the icon from there, so on Wayland the icon simply does not exist until
# these files are in place. X11 gets an icon from the running app either way,
# but still needs the .desktop entry to appear in menus.

set -eu

APP_ID="io.github.mickegris.winrmpc"
PREFIX="${PREFIX:-$HOME/.local}"
SRC_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH='' cd -- "$SRC_DIR/../.." && pwd)"

DESKTOP_DIR="$PREFIX/share/applications"
ICON_ROOT="$PREFIX/share/icons/hicolor"
BIN_DIR="$PREFIX/bin"

refresh_caches() {
    # Both are best-effort: desktop environments pick the files up on their own
    # eventually, and neither tool is guaranteed to be installed.
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$ICON_ROOT" 2>/dev/null || true
    fi
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$DESKTOP_DIR/$APP_ID.desktop"
    find "$ICON_ROOT" -name "$APP_ID.png" -delete 2>/dev/null || true
    find "$ICON_ROOT" -name "$APP_ID.svg" -delete 2>/dev/null || true
    rm -f "$BIN_DIR/winrmpc"
    refresh_caches
    echo "Removed winrmpc from $PREFIX"
    exit 0
fi

# ── Icons ────────────────────────────────────────────────────────────────────
# Copied rather than generated, so this script needs no Rust toolchain. To
# regenerate them after changing the design: cargo run --example emit_icons
for src in "$SRC_DIR"/icons/hicolor/*/apps/"$APP_ID".*; do
    [ -e "$src" ] || continue
    rel="${src#"$SRC_DIR"/icons/hicolor/}"   # e.g. 48x48/apps/<id>.png
    dest="$ICON_ROOT/$rel"
    mkdir -p "$(dirname -- "$dest")"
    cp -f "$src" "$dest"
done
echo "Installed icons into $ICON_ROOT"

# ── Desktop entry ────────────────────────────────────────────────────────────
mkdir -p "$DESKTOP_DIR"
cp -f "$SRC_DIR/$APP_ID.desktop" "$DESKTOP_DIR/$APP_ID.desktop"
echo "Installed $DESKTOP_DIR/$APP_ID.desktop"

# ── Binary (only if one has been built) ──────────────────────────────────────
# Exec=winrmpc is resolved against PATH, so the binary has to be on it. If you
# keep winrmpc elsewhere, skip this and edit the Exec= line to an absolute path.
BIN_SRC="$REPO_ROOT/target/release/winrmpc"
if [ -x "$BIN_SRC" ]; then
    mkdir -p "$BIN_DIR"
    cp -f "$BIN_SRC" "$BIN_DIR/winrmpc"
    echo "Installed $BIN_DIR/winrmpc"
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *) echo "NOTE: $BIN_DIR is not on your PATH — the launcher will not find winrmpc" ;;
    esac
else
    echo "NOTE: no release binary at $BIN_SRC — run 'cargo build --release' first,"
    echo "      or put winrmpc on your PATH yourself. Icons and .desktop are installed."
fi

refresh_caches
echo "Done. You may need to log out and back in for the icon to appear."
