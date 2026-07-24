#!/usr/bin/env bash
# =============================================================================
# bundle-msys2.sh — Bundle Sleeve for Windows (MSYS2/UCRT64)
# =============================================================================
#
# Copies the release binary, all required DLLs, and GTK4/libadwaita runtime
# resources (schemas, icons, GdkPixbuf loaders) into dist/ so the result is a
# self-contained directory that works without MSYS2.
#
# Prerequisites:
#   pacman -S mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-libadwaita
#
# Usage:
#   ./scripts/bundle-msys2.sh              # → dist/sleeve.exe + resources
#   ./scripts/bundle-msys2.sh --skip-build  # reuse existing release binary
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
BUILD_DIR="$PROJECT_DIR/target/release"

MINGW_PREFIX="${MINGW_PREFIX:-/ucrt64}"
ICON="$PROJECT_DIR/assets/icons/sleeve-icon.png"

FLAG_SKIP_BUILD=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build) FLAG_SKIP_BUILD=true; shift ;;
        --help|-h)
            echo "Usage: $0 [--skip-build]"
            exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

info()  { echo -e "${GREEN}[INFO]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }
step()  { echo; echo -e "${BLUE}━━━ $1 ━━━${NC}"; }

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# ── Step 1: Build ──────────────────────────────────────────────────────────
step "1/7  Building Sleeve (release)"
if [[ "$FLAG_SKIP_BUILD" == true ]]; then
    info "Skipping build (--skip-build)"
    if [[ ! -f "$BUILD_DIR/sleeve.exe" ]]; then
        error "No pre-built binary at $BUILD_DIR/sleeve.exe"
        exit 1
    fi
else
    cargo build --release
fi

# ── Step 2: Create dist and copy binary + DLLs to bin/ ──────────────────────
step "2/7  Copying binary and deployment DLLs"

LDD_DEPLOY="$PROJECT_DIR/.tools/ldd_deploy.sh"
LDD_DEPLOY_URL="https://raw.githubusercontent.com/lostjared/ldd-deploy/refs/heads/main/ldd_deploy.sh"

if [[ ! -f "$LDD_DEPLOY" ]]; then
    info "Downloading ldd_deploy.sh..."
    mkdir -p "$(dirname "$LDD_DEPLOY")"
    wget -O "$LDD_DEPLOY" "$LDD_DEPLOY_URL"
fi

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR/bin"
cp "$BUILD_DIR/sleeve.exe" "$DIST_DIR/bin/"
"$LDD_DEPLOY" -i "$BUILD_DIR/sleeve.exe" -o "$DIST_DIR/bin"

# ── Step 3: Copy GTK runtime resources ─────────────────────────────────────
step "3/7  Copying GTK runtime resources"

# GdkPixbuf loaders
PIXBUF_SRC="$MINGW_PREFIX/lib/gdk-pixbuf-2.0"
if [[ -d "$PIXBUF_SRC" ]]; then
    mkdir -p "$DIST_DIR/lib/gdk-pixbuf-2.0"
    cp -R "$PIXBUF_SRC/" "$DIST_DIR/lib/gdk-pixbuf-2.0/"
    find "$DIST_DIR/lib/gdk-pixbuf-2.0" \( -name "*.a" -o -name "*.la" \) -delete 2>/dev/null || true
    info "  GdkPixbuf loaders: $DIST_DIR/lib/gdk-pixbuf-2.0"
fi

# GLib schemas
SCHEMAS_SRC="$MINGW_PREFIX/share/glib-2.0/schemas"
if [[ -d "$SCHEMAS_SRC" ]]; then
    mkdir -p "$DIST_DIR/share/glib-2.0/schemas"
    cp "$SCHEMAS_SRC"/*.xml "$DIST_DIR/share/glib-2.0/schemas/" 2>/dev/null || true
    cp "$SCHEMAS_SRC"/*.gschema.override "$DIST_DIR/share/glib-2.0/schemas/" 2>/dev/null || true
    info "  GLib schemas: $DIST_DIR/share/glib-2.0/schemas"
fi

# Icon themes
mkdir -p "$DIST_DIR/share/icons"
for theme in Adwaita hicolor; do
    ICON_SRC="$MINGW_PREFIX/share/icons/$theme"
    if [[ -d "$ICON_SRC" ]]; then
        rm -rf "$DIST_DIR/share/icons/$theme"
        cp -R "$ICON_SRC" "$DIST_DIR/share/icons/"
        rm -rf "$DIST_DIR/share/icons/$theme/cursors" 2>/dev/null || true
        info "  Icons: $DIST_DIR/share/icons/$theme"
    fi
done

# Language files
LANG_DIR="$PROJECT_DIR/assets/lang"
if [[ -d "$LANG_DIR" ]]; then
    mkdir -p "$DIST_DIR/share/sleeve/lang"
    cp "$LANG_DIR"/*.json "$DIST_DIR/share/sleeve/lang/"
    info "  Language files: $DIST_DIR/share/sleeve/lang"
fi

# ── Step 4: Regenerate caches ──────────────────────────────────────────────
step "4/7  Regenerating caches"

glib-compile-schemas "$DIST_DIR/share/glib-2.0/schemas" 2>/dev/null || true
info "  GLib schemas compiled"

LOADERS_DIR=$(find "$DIST_DIR/lib/gdk-pixbuf-2.0" -name "loaders" -type d 2>/dev/null | head -1 || true)
if [[ -n "$LOADERS_DIR" && -x "$MINGW_PREFIX/bin/gdk-pixbuf-query-loaders" ]]; then
    "$MINGW_PREFIX/bin/gdk-pixbuf-query-loaders" "$LOADERS_DIR" > "$LOADERS_DIR/loaders.cache" 2>/dev/null || true
    info "  GdkPixbuf loaders cache regenerated"
fi

if command -v gtk-update-icon-cache &>/dev/null; then
    find "$DIST_DIR/share/icons" -maxdepth 1 -mindepth 1 -type d | while read -r theme_dir; do
        gtk-update-icon-cache --quiet "$theme_dir" 2>/dev/null || true
    done
    info "  Icon caches regenerated"
fi

# ── Step 5: Pack standalone executable with wrappe ──────────────────────────
step "5/7  Packing standalone executable"

WRAPPE="$PROJECT_DIR/.tools/wrappe.exe"
WRAPPE_URL="https://github.com/Systemcluster/wrappe/releases/download/v1.0.6/wrappe.exe"

if [[ ! -f "$WRAPPE" ]]; then
    info "Downloading wrappe..."
    mkdir -p "$(dirname "$WRAPPE")"
    wget -O "$WRAPPE" "$WRAPPE_URL"
fi

VERSION="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n 1)"
OUTPUT="$DIST_DIR/Sleeve-${VERSION}.exe"

"$WRAPPE" "$DIST_DIR" "bin/sleeve.exe" "$OUTPUT" -c 12 -t temp -v none -e none -n never -m "$ICON"
info "  Standalone: $OUTPUT"

# ── Step 6: Create zip archive ─────────────────────────────────────────────
step "6/7  Creating zip archive"

ZIP_OUTPUT="$DIST_DIR/Sleeve-${VERSION}.zip"
cd "$DIST_DIR"
zip -r "$ZIP_OUTPUT" bin share lib -x "Sleeve-*.exe" "Sleeve-*.zip"
info "  Archive: $ZIP_OUTPUT"

# ── Step 7: Clean up — keep only standalone exe and zip ────────────────────
step "7/7  Cleaning up"
rm -rf "$DIST_DIR/bin" "$DIST_DIR/lib" "$DIST_DIR/share"
info "  Removed build directory, retained:"
info "    $OUTPUT"
info "    $ZIP_OUTPUT"

# ── Summary ────────────────────────────────────────────────────────────────
echo
echo -e "${BLUE}━━━ Summary ━━━${NC}"
echo "  Standalone: $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
echo "  Archive:    $ZIP_OUTPUT ($(du -h "$ZIP_OUTPUT" | cut -f1))"
