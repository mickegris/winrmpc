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

# ── Binary ───────────────────────────────────────────────────────────────────
# Exec=winrmpc is resolved against PATH, so the binary has to be on it. If you
# keep winrmpc elsewhere, skip this and edit the Exec= line to an absolute path.
#
# Two layouts have to work, and only the second one used to:
#
#   released tarball          git checkout
#   ------------------        ---------------------------
#   winrmpc          <- bin   target/release/winrmpc  <- bin
#   packaging/                packaging/linux/
#     install.sh                install.sh
#
# In the tarball the binary is the script's *parent* directory, and there is no
# target/ at all — so this reported "no release binary, run cargo build first"
# to someone who had just downloaded a prebuilt binary, installed the .desktop
# entry pointing at an executable that wasn't there, and left the launcher
# broken.
BIN_SRC=""
for candidate in \
    "$SRC_DIR/../winrmpc" \
    "$REPO_ROOT/target/release/winrmpc" \
    "$SRC_DIR/winrmpc"
do
    if [ -x "$candidate" ] && [ ! -d "$candidate" ]; then
        BIN_SRC="$candidate"
        break
    fi
done

if [ -n "$BIN_SRC" ]; then
    mkdir -p "$BIN_DIR"
    cp -f "$BIN_SRC" "$BIN_DIR/winrmpc"
    echo "Installed $BIN_DIR/winrmpc"
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *) echo "NOTE: $BIN_DIR is not on your PATH — the launcher will not find winrmpc" ;;
    esac
else
    echo "NOTE: no winrmpc binary found next to this script or in target/release."
    echo "      Icons and .desktop are installed, but the launcher won't start"
    echo "      anything until winrmpc is on your PATH. From a source checkout:"
    echo "        cargo build --release && $0"
fi

refresh_caches
echo "Done. You may need to log out and back in for the icon to appear."
