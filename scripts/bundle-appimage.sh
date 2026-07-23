#!/usr/bin/env bash
# =============================================================================
# bundle-appimage.sh — Package Sleeve as an AppImage
# =============================================================================
#
# This script:
#   1. Downloads linuxdeploy + GTK plugin into .tools/ (git-ignored)
#   2. Builds the release binary
#   3. Creates an AppDir with the binary, resources, and runtime deps
#   4. Bundles GTK4/libadwaita via linuxdeploy-plugin-gtk
#   5. Produces a portable .AppImage in dist/
#
# Usage:
#   ./scripts/bundle-appimage.sh           # → dist/Sleeve-<arch>.AppImage
#
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
TOOLS_DIR="$PROJECT_DIR/.tools"
APP_DIR="$DIST_DIR/Sleeve.AppDir"
ARCH="$(uname -m)"
APP_ID="com.github.anson2251.sleeve"
VERSION="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n 1)"

if [ -z "$VERSION" ]; then
    echo "ERROR: Could not determine package version from Cargo.toml" >&2
    exit 1
fi

mkdir -p "$TOOLS_DIR" "$DIST_DIR"

# ---------------------------------------------------------------------------
# Download build tools
# ---------------------------------------------------------------------------
LINUXDEPLOY="$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
GTK_PLUGIN="$TOOLS_DIR/linuxdeploy-plugin-gtk.sh"

if [ ! -f "$LINUXDEPLOY" ]; then
    echo ":: Downloading linuxdeploy..."
    wget -O "$LINUXDEPLOY" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x "$LINUXDEPLOY"
fi

if [ ! -f "$GTK_PLUGIN" ]; then
    echo ":: Downloading linuxdeploy-plugin-gtk..."
    wget -O "$GTK_PLUGIN" \
        "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh"
    chmod +x "$GTK_PLUGIN"
fi

# ---------------------------------------------------------------------------
# Build release binary
# ---------------------------------------------------------------------------
echo ":: Building release binary..."
cd "$PROJECT_DIR"
cargo build --release

# Clean dist
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# ---------------------------------------------------------------------------
# Create AppDir skeleton
# ---------------------------------------------------------------------------
echo ":: Creating AppDir..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR"/usr/bin
mkdir -p "$APP_DIR"/usr/share/applications
mkdir -p "$APP_DIR"/usr/share/metainfo
mkdir -p "$APP_DIR"/usr/share/sleeve/lang
mkdir -p "$APP_DIR"/usr/share/icons/hicolor/512x512/apps
mkdir -p "$APP_DIR"/usr/share/glib-2.0/schemas

cp target/release/sleeve "$APP_DIR/usr/bin/"

# Desktop entry
cat > "$APP_DIR/usr/share/applications/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Sleeve
Comment=Audio tag and cover art editor
Exec=sleeve
Icon=$APP_ID
Categories=AudioVideo;Audio;
Terminal=false
StartupNotify=true
EOF

# Icon
cp assets/icons/sleeve-icon.png \
   "$APP_DIR/usr/share/icons/hicolor/512x512/apps/$APP_ID.png"

# AppStream metainfo
cat > "$APP_DIR/usr/share/metainfo/$APP_ID.metainfo.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>$APP_ID</id>
  <name>Sleeve</name>
  <summary>Audio tag and cover art editor</summary>
  <developer id="com.github.anson2251">
    <name>anson2251</name>
  </developer>
  <project_license>GPL-3.0-or-later</project_license>
  <metadata_license>CC0-1.0</metadata_license>
  <url type="homepage">https://github.com/anson2251/sleeve</url>
</component>
EOF

# Language files (i18n.rs finds them via ancestor walk: share/sleeve/lang)
cp assets/lang/*.json "$APP_DIR/usr/share/sleeve/lang/"

# ---------------------------------------------------------------------------
# Bundle GTK4/libadwaita runtime data
# ---------------------------------------------------------------------------

# Adwaita icon theme (symbolic icons for toolbar buttons)
if [ -d /usr/share/icons/Adwaita ]; then
    echo ":: Bundling Adwaita icon theme..."
    mkdir -p "$APP_DIR/usr/share/icons"
    cp -r /usr/share/icons/Adwaita "$APP_DIR/usr/share/icons/"
    rm -rf "$APP_DIR/usr/share/icons/Adwaita/cursors" 2>/dev/null || true
fi

# hicolor icon theme (fallback, always expected by GTK)
if [ -d /usr/share/icons/hicolor ]; then
    mkdir -p "$APP_DIR/usr/share/icons"
    cp -r /usr/share/icons/hicolor "$APP_DIR/usr/share/icons/"
fi

# GLib schemas (including org.gnome.desktop.interface for gtk-theme etc.)
if [ -d /usr/share/glib-2.0/schemas ]; then
    echo ":: Bundling GLib schemas..."
    cp /usr/share/glib-2.0/schemas/*.xml "$APP_DIR/usr/share/glib-2.0/schemas/" 2>/dev/null || true
    cp /usr/share/glib-2.0/schemas/*.gschema.override "$APP_DIR/usr/share/glib-2.0/schemas/" 2>/dev/null || true
fi

# Compile schemas
if [ -d "$APP_DIR/usr/share/glib-2.0/schemas" ]; then
    echo ":: Compiling GLib schemas..."
    glib-compile-schemas "$APP_DIR/usr/share/glib-2.0/schemas"
fi

# ---------------------------------------------------------------------------
# linuxdeploy apprun-hook — libadwaita runtime environment
# ---------------------------------------------------------------------------
# libadwaita's stylesheet is a GResource embedded in libadwaita-1.so, rather
# than a GTK theme under /usr/share/themes.  The GTK plugin writes a hook that
# forces GTK_THEME=Adwaita:* and sets GTK_DATA_PREFIX to the AppDir root; both
# are incorrect for GTK4/libadwaita.  Name this hook "zz-" so it is sourced
# after the plugin hook and can correct those values.
mkdir -p "$APP_DIR/apprun-hooks"
cat > "$APP_DIR/apprun-hooks/zz-sleeve-libadwaita.sh" <<'HOOK'
# Sourced by AppRun before launching the binary.
APPDIR="${APPDIR:-"$(dirname "$(readlink -f "$0")")"}"
export GTK_DATA_PREFIX="$APPDIR/usr"
export XDG_DATA_DIRS="$APPDIR/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GSETTINGS_SCHEMA_DIR="$APPDIR/usr/share/glib-2.0/schemas"
# Let libadwaita select its embedded light/dark stylesheet itself.
unset GTK_THEME
HOOK
chmod +x "$APP_DIR/apprun-hooks/zz-sleeve-libadwaita.sh"

# ---------------------------------------------------------------------------
# Symlinks for AppImage conventions
# ---------------------------------------------------------------------------
ln -sf usr/share/applications/$APP_ID.desktop "$APP_DIR/"
ln -sf usr/share/icons/hicolor/512x512/apps/$APP_ID.png "$APP_DIR/"

# ---------------------------------------------------------------------------
# Bundle GTK4/libadwaita with linuxdeploy + GTK plugin
# ---------------------------------------------------------------------------
echo ":: Running linuxdeploy..."

export LDAI_OUTPUT="$DIST_DIR/sleeve-${VERSION}-linux-${ARCH}.AppImage"
export GTK_DATA_PREFIX="$APP_DIR/usr"

# Explicitly deploy libadwaita: its embedded GResource contains the complete
# libadwaita stylesheet, so no host theme files are required at runtime.
LIBADWAITA_LIBRARY="$(pkg-config --variable=libdir libadwaita-1)/libadwaita-1.so.0"
if [ ! -e "$LIBADWAITA_LIBRARY" ]; then
    echo "ERROR: libadwaita runtime library not found: $LIBADWAITA_LIBRARY" >&2
    exit 1
fi

# The GTK plugin uses UPDINFO; set it from git tag if available
if git describe --tags --exact-match 2>/dev/null; then
    export UPDINFO="gh-releases-zsync|anson2251|sleeve|latest|sleeve-${VERSION}-linux-${ARCH}.AppImage.zsync"
fi

# Auto-enable AppImage extraction if FUSE is unavailable (common in CI)
if ! command -v fusermount &>/dev/null && ! command -v fusermount3 &>/dev/null; then
    export APPIMAGE_EXTRACT_AND_RUN=1
fi

"$LINUXDEPLOY" \
    --appdir "$APP_DIR" \
    --library "$LIBADWAITA_LIBRARY" \
    --plugin gtk \
    --output appimage

echo ":: Done! --> $LDAI_OUTPUT"

# Clean up AppDir
rm -rf "$APP_DIR"
