#!/usr/bin/env bash
# =============================================================================
# bundle-macos.sh — Package Sleeve as a standalone macOS .app (+ optional .dmg)
# =============================================================================
#
# This script:
#   1. Builds the release binary
#   2. Creates a Sleeve.app bundle with proper Info.plist
#   3. Bundles all GTK4/libadwaita dylib dependencies via dylibbundler
#   4. Copies GTK runtime resources (schemas, pixbuf loaders, icons)
#   5. Optionally creates a .dmg disk image
#   6. Optionally code-signs and notarizes
#
# Prerequisites:
#   brew install dylibbundler
#
# Usage:
#   ./scripts/bundle-macos.sh              # → dist/Sleeve.app
#   ./scripts/bundle-macos.sh --dmg        # → dist/Sleeve.app + Sleeve.dmg
#   ./scripts/bundle-macos.sh --sign       # codesign + notarize
#   ./scripts/bundle-macos.sh --help       # full help
#
# Output goes to:  dist/
# =============================================================================
set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────
APP_NAME="Sleeve"
BINARY_NAME="sleeve"
IDENTIFIER="com.github.anson2251.sleeve"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n 1)"
MIN_OS_VERSION="11.0"

BUILD_DIR="target/release"
DIST_DIR="dist"
APP_DIR="$DIST_DIR/$APP_NAME.app"

# Detect Homebrew prefix (Apple Silicon vs Intel)
if [[ $(uname -m) == "arm64" ]]; then
    HOMEBREW_PREFIX="/opt/homebrew"
else
    HOMEBREW_PREFIX="/usr/local"
fi

# ── Colors ────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }
step()  { echo; echo -e "${BLUE}━━━ $1 ━━━${NC}"; }

# ── Flags ─────────────────────────────────────────────────────────────────
FLAG_DMG=false
FLAG_SIGN=false
FLAG_SKIP_BUILD=false

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  --dmg           Create a .dmg disk image after building the .app
  --sign          Code-sign the .app (requires Apple Developer ID certificate)
  --skip-build    Skip the cargo build step (use existing binary)
  --help          Show this help
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dmg)        FLAG_DMG=true; shift ;;
        --sign)       FLAG_SIGN=true; shift ;;
        --skip-build) FLAG_SKIP_BUILD=true; shift ;;
        --help|-h)    usage ;;
        *)  error "Unknown option: $1"; usage ;;
    esac
done

# ── Prerequisites ─────────────────────────────────────────────────────────
check_prereqs() {
    if [[ $(uname) != "Darwin" ]]; then
        error "This script is for macOS only."
        exit 1
    fi

    if ! command -v dylibbundler &>/dev/null; then
        error "dylibbundler is required. Install it with:  brew install dylibbundler"
        exit 1
    fi

    if [[ ! -d "$HOMEBREW_PREFIX" ]]; then
        error "Homebrew not found at $HOMEBREW_PREFIX"
        exit 1
    fi

    info "Prerequisites OK"
    info "  Homebrew prefix: $HOMEBREW_PREFIX"
    info "  Architecture:    $(uname -m)"
    info "  dylibbundler:    $(command -v dylibbundler)"
}

# ── Step 1: Build ─────────────────────────────────────────────────────────
step_build() {
    if [[ "$FLAG_SKIP_BUILD" == true ]]; then
        info "Skipping build (--skip-build)"
        if [[ ! -f "$BUILD_DIR/$BINARY_NAME" ]]; then
            error "No pre-built binary found at $BUILD_DIR/$BINARY_NAME"
            exit 1
        fi
        return
    fi

    info "Building Sleeve (release mode)..."
    cargo build --release
    info "Binary: $BUILD_DIR/$BINARY_NAME"
}

# ── Step 2: Create .app skeleton ──────────────────────────────────────────
step_create_app() {
    info "Creating .app bundle at $APP_DIR..."

    rm -rf "$APP_DIR"
    mkdir -p "$APP_DIR/Contents/MacOS"
    mkdir -p "$APP_DIR/Contents/Resources"
    mkdir -p "$APP_DIR/Contents/Frameworks"
    mkdir -p "$APP_DIR/Contents/Resources/etc"   # for GTK settings

    # Copy the binary
    cp "$BUILD_DIR/$BINARY_NAME" "$APP_DIR/Contents/MacOS/$BINARY_NAME"
    chmod 755 "$APP_DIR/Contents/MacOS/$BINARY_NAME"

    # ── Info.plist ──────────────────────────────────────────────────────
    cat > "$APP_DIR/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
 "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>$BINARY_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$IDENTIFIER</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MIN_OS_VERSION</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright (c) 2025 Anson2251</string>
    <key>CFBundleIconFile</key>
    <string>sleeve</string>
</dict>
</plist>
EOF

    # ── App icon ────────────────────────────────────────────────────────
    if [[ -f "assets/icons/sleeve.icns" ]]; then
        cp "assets/icons/sleeve.icns" "$APP_DIR/Contents/Resources/"
        info "Icon: assets/icons/sleeve.icns"
    else
        warn "No sleeve.icns found — app will use a default icon."
        warn "  To create one:"
        warn "    1. Prepare a 1024×1024 PNG"
        warn "    2. Run:  iconutil -c icns icon.iconset -o assets/icons/sleeve.icns"
    fi

    # ── PkgInfo (minimal, required by some macOS versions) ──────────────
    echo "APPL????" > "$APP_DIR/Contents/PkgInfo"
}

# ── Step 3: Bundle dylib dependencies ─────────────────────────────────────
step_bundle_dylibs() {
    local binary="$APP_DIR/Contents/MacOS/$BINARY_NAME"
    local frameworks_dir="$APP_DIR/Contents/Frameworks"

    info "Bundling dylib dependencies with dylibbundler..."

    # dylibbundler usage:
    #   -x <executable>    binary to scan
    #   -b                 enable dependency bundling
    #   -d <dir>           where to copy dylibs
    #   -p <rpath>         rpath prefix to use
    #   -cd                change dylib IDs in copied dylibs
    #   -of                overwrite existing files
    #   -s <path>          additional search path
    #   -q                 quiet mode
    dylibbundler \
        -x "$binary" \
        -b \
        -d "$frameworks_dir" \
        -p "@executable_path/../Frameworks/" \
        -cd -of \
        -s "$HOMEBREW_PREFIX/lib"

    info "dylibbundler completed."
    info "  Frameworks count: $(ls "$frameworks_dir"/*.dylib 2>/dev/null | wc -l | tr -d ' ') dylibs"

    # Verify the binary's rpath is set correctly
    local rpath_check
    rpath_check=$(otool -L "$binary" | grep -c "$frameworks_dir" 2>/dev/null || true)
    info "  Binary linked libraries pointing into Frameworks/: $rpath_check"
}

# ── Step 4: Copy GTK runtime resources ────────────────────────────────────
step_copy_gtk_resources() {
    local resources_dir="$APP_DIR/Contents/Resources"
    local lib_dir="$resources_dir/lib"
    local share_dir="$resources_dir/share"

    info "Copying GTK4 runtime resources..."

    # ── GdkPixbuf loaders ───────────────────────────────────────────────
    # GTK4 needs GdkPixbuf for image loading; the loaders are .so/.dylib
    # files that must be findable at runtime.
    local pixbuf_dir="$HOMEBREW_PREFIX/lib/gdk-pixbuf-2.0"
    if [[ -d "$pixbuf_dir" ]]; then
        mkdir -p "$lib_dir/gdk-pixbuf-2.0"
        cp -R "$pixbuf_dir/" "$lib_dir/gdk-pixbuf-2.0/"
        # Remove .la and .a files to save space
        find "$lib_dir/gdk-pixbuf-2.0" \( -name "*.la" -o -name "*.a" \) -delete 2>/dev/null || true
        info "  GdkPixbuf loaders: $lib_dir/gdk-pixbuf-2.0"

        # Fix rpaths on pixbuf loader dylibs so they find their deps
        local loader_dylib
        find "$lib_dir/gdk-pixbuf-2.0" -name "*.dylib" -o -name "*.so" | while read -r loader_dylib; do
            if [[ -f "$loader_dylib" ]]; then
                install_name_tool -add_rpath "@executable_path/../Frameworks/" "$loader_dylib" 2>/dev/null || true
                install_name_tool -add_rpath "@loader_path/../../../../" "$loader_dylib" 2>/dev/null || true
            fi
        done
    fi

    # ── GLib schemas ────────────────────────────────────────────────────
    # Required for libadwaita to find its style schemas at runtime.
    local schemas_src="$HOMEBREW_PREFIX/share/glib-2.0/schemas"
    if [[ -d "$schemas_src" ]]; then
        mkdir -p "$share_dir/glib-2.0"
        cp -R "$schemas_src/" "$share_dir/glib-2.0/schemas/"
        info "  GLib schemas: $share_dir/glib-2.0/schemas"
    fi

    # ── Icon themes ───────────────────────────────────────────────────
    # GTK needs icon themes for rendering. Adwaita is the default theme
    # and hicolor is the fallback. Homebrew installs index.theme as a
    # symlink to Cellar, so we use cp -RL to resolve all symlinks.
    for theme in Adwaita hicolor; do
        local src="$HOMEBREW_PREFIX/share/icons/$theme"
        if [[ -d "$src" ]]; then
            mkdir -p "$share_dir/icons/$theme"
            # Use rsync or cp -RL to follow symlinks (Homebrew's index.theme
            # is a symlink into Cellar that would break in the bundle)
            if command -v rsync &>/dev/null; then
                rsync -a --copy-unsafe-links "$src/" "$share_dir/icons/$theme/"
            else
                cp -RL "$src/" "$share_dir/icons/$theme/"
            fi
            rm -f "$share_dir/icons/$theme/icon-theme.cache"  # will be regenerated
            info "  Icons: $share_dir/icons/$theme"
        fi
    done

    # ── GTK settings.ini ────────────────────────────────────────────────
    # Prevents GTK from looking for settings outside the bundle.
    mkdir -p "$resources_dir/etc/gtk-4.0"
    cat > "$resources_dir/etc/gtk-4.0/settings.ini" <<'EOF'
[Settings]
gtk-hint-font-metrics=1
gtk-print-backends=
gtk-enable-animations=1
EOF
    info "  GTK settings: $resources_dir/etc/gtk-4.0/settings.ini"

    # ── Language / i18n files ──────────────────────────────────────────
    local lang_dir="assets/lang"
    if [[ -d "$lang_dir" ]]; then
        mkdir -p "$resources_dir/lang"
        cp -R "$lang_dir/" "$resources_dir/lang/"
        info "  Language files: $resources_dir/lang"
    else
        warn "No assets/lang directory found — translations will not be available in the bundle."
    fi

    # ── GStreamer runtime (if downloaded by Sleeve) ────────────────────
    local gst_dir="assets/gstreamer"
    if [[ -d "$gst_dir" ]]; then
        cp -R "$gst_dir" "$resources_dir/gstreamer"
        info "  GStreamer runtime: $resources_dir/gstreamer"
    fi
}

# ── Step 5: Create GTK environment wrapper ────────────────────────────────
# GTK4 on macOS needs environment variables set to find resources inside
# the .app bundle. We ship a small launcher script that sets these before
# exec-ing the real binary.
step_create_launcher() {
    local macos_dir="$APP_DIR/Contents/MacOS"
    local resources_dir="$APP_DIR/Contents/Resources"

    info "Creating launcher script with GTK environment..."

    # Rename the real binary to "sleeve-bin"
    mv "$macos_dir/$BINARY_NAME" "$macos_dir/${BINARY_NAME}-bin"

    # Create the launcher script
    cat > "$macos_dir/$BINARY_NAME" <<'LAUNCHER'
#!/usr/bin/env bash
# Sleeve launcher — sets GTK environment for bundled dylibs/resources
set -euo pipefail

# Resolve the bundle root from the script's location
BUNDLE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
RESOURCES="$BUNDLE_DIR/Resources"

# ── GTK runtime paths ────────────────────────────────────────────────
export DYLD_LIBRARY_PATH="$BUNDLE_DIR/Frameworks${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"

# Tell GTK where to find its resource files
export GTK_DATA_PREFIX="$RESOURCES"
export GTK_EXE_PREFIX="$RESOURCES"
export GTK_PATH="$RESOURCES"

# GdkPixbuf — point to bundled loaders
export GDK_PIXBUF_MODULE_FILE="$RESOURCES/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
export GDK_PIXBUF_MODULEDIR="$RESOURCES/lib/gdk-pixbuf-2.0/2.10.0/loaders"

# GLib — point to bundled schemas
export XDG_DATA_DIRS="$RESOURCES/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"

# Adwaita icon theme
export GTK_ICON_THEME_DIR="$RESOURCES/share/icons"

# Prevent GTK from looking outside the bundle
export GTK_DEBUG=no-window-state

# Fallback: regenerate pixbuf loaders cache if the cached one is missing
if [[ ! -f "$GDK_PIXBUF_MODULE_FILE" ]]; then
    gdk-pixbuf-query-loaders --loaders "$GDK_PIXBUF_MODULEDIR" > "$GDK_PIXBUF_MODULE_FILE" 2>/dev/null || true
fi

# Launch the real binary
exec "$BUNDLE_DIR/MacOS/sleeve-bin" "$@"
LAUNCHER

    chmod 755 "$macos_dir/$BINARY_NAME"
    info "Launcher created at $macos_dir/$BINARY_NAME"
    info "  Real binary at $macos_dir/${BINARY_NAME}-bin"
}

# ── Step 6: Regenerate GdkPixbuf loaders cache ────────────────────────────
step_regenerate_caches() {
    local resources_dir="$APP_DIR/Contents/Resources"
    local loaders_dir="$resources_dir/lib/gdk-pixbuf-2.0"
    local share_dir="$resources_dir/share"

    # ── GdkPixbuf loaders cache ────────────────────────────────────────
    if [[ -d "$loaders_dir" ]]; then
        local loaders_subdir
        loaders_subdir=$(find "$loaders_dir" -name "loaders" -type d 2>/dev/null | head -1 || true)

        if [[ -n "$loaders_subdir" ]]; then
            info "Regenerating GdkPixbuf loaders cache..."

            local query_tool="$HOMEBREW_PREFIX/bin/gdk-pixbuf-query-loaders"
            if [[ -x "$query_tool" ]]; then
                env GDK_PIXBUF_MODULEDIR="$loaders_subdir" \
                    "$query_tool" > "$loaders_subdir/loaders.cache" 2>/dev/null || true

                if [[ -f "$loaders_subdir/loaders.cache" ]]; then
                    sed -i '' "s|$HOMEBREW_PREFIX|@executable_path/../..|g" "$loaders_subdir/loaders.cache" 2>/dev/null || true
                    info "  Pixbuf cache: $loaders_subdir/loaders.cache"
                fi
            else
                warn "gdk-pixbuf-query-loaders not found — launcher will generate cache at first run."
            fi
        fi
    fi

    # ── GTK icon cache ─────────────────────────────────────────────────
    # Regenerate icon-theme.cache for each bundled icon theme so GTK4
    # doesn't have to scan directories at startup.
    if command -v gtk4-update-icon-cache &>/dev/null; then
        find "$share_dir/icons" -maxdepth 1 -type d ! -name "icons" | while read -r theme_dir; do
            info "Generating icon cache for $(basename "$theme_dir")..."
            gtk4-update-icon-cache --quiet "$theme_dir" 2>/dev/null || true
        done
    elif command -v gtk-update-icon-cache &>/dev/null; then
        find "$share_dir/icons" -maxdepth 1 -type d ! -name "icons" | while read -r theme_dir; do
            info "Generating icon cache for $(basename "$theme_dir")..."
            gtk-update-icon-cache --quiet "$theme_dir" 2>/dev/null || true
        done
    else
        warn "gtk-update-icon-cache not found — icon lookups may be slower."
    fi
}

# ── Step 7: Code-sign (optional) ──────────────────────────────────────────
step_codesign() {
    if [[ "$FLAG_SIGN" != true ]]; then
        return
    fi

    info "Code-signing the .app bundle..."

    # Build the codesign command
    local entitlements=""
    if [[ -f "scripts/entitlements.plist" ]]; then
        entitlements="--entitlements scripts/entitlements.plist"
        info "  Using entitlements: scripts/entitlements.plist"
    fi

    # Sign every dylib and framework first, then the app
    find "$APP_DIR" \( -name "*.dylib" -o -name "*.so" \) -type f | while read -r lib; do
        codesign --force --options runtime --sign - "$lib" 2>/dev/null || true
    done

    # Sign the app bundle
    codesign --force --options runtime \
        --sign - \
        --timestamp \
        --verbose \
        "$APP_DIR"

    info "Code-signing complete."

    # Verify
    codesign --verify --verbose "$APP_DIR"
}

# ── Step 8: Create .dmg (optional) ────────────────────────────────────────
step_create_dmg() {
    if [[ "$FLAG_DMG" != true ]]; then
        return
    fi

    local dmg_path="$DIST_DIR/$APP_NAME-$VERSION.dmg"
    local tmp_dir="$DIST_DIR/.dmg-tmp"
    local volume_name="$APP_NAME $VERSION"

    info "Creating .dmg disk image..."

    rm -rf "$tmp_dir"
    mkdir -p "$tmp_dir"

    # Create a symlink to /Applications for drag-and-drop install
    ln -s /Applications "$tmp_dir/Applications"

    # Copy the .app
    cp -R "$APP_DIR" "$tmp_dir/"

    # Create the DMG using hdiutil (macOS built-in)
    # This gives us a basic DMG without custom background
    rm -f "$dmg_path"
    hdiutil create \
        -fs HFS+ \
        -srcfolder "$tmp_dir" \
        -volname "$volume_name" \
        -format UDZO \
        -imagekey zlib-level=9 \
        "$dmg_path"

    # Clean up
    rm -rf "$tmp_dir"

    info "DMG created: $dmg_path"
    info "  Size: $(du -h "$dmg_path" | cut -f1)"
}

# ── Final summary ─────────────────────────────────────────────────────────
summary() {
    echo
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  ✅ Packaging complete!${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo
    echo "  Application bundle:"
    echo "    $APP_DIR"
    echo
    echo "  Bundle size: $(du -sh "$APP_DIR" | cut -f1)"
    echo "  Number of bundled frameworks: $(ls "$APP_DIR/Contents/Frameworks"/*.dylib 2>/dev/null | wc -l | tr -d ' ')"
    echo
    echo "  To test:"
    echo "    open $APP_DIR"
    echo

    if [[ "$FLAG_DMG" == true ]]; then
        echo "  Disk image:"
        echo "    $DIST_DIR/$APP_NAME-$VERSION.dmg"
        echo
    fi

    if [[ "$FLAG_SIGN" != true ]]; then
        echo -e "  ${YELLOW}Note: Not code-signed. Users will need to right-click → Open.${NC}"
        echo "  To sign:  ./scripts/bundle-macos.sh --dmg --sign"
    fi
    echo
}

# ═══════════════════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════════════════

main() {
    echo
    echo -e "${BLUE}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║${NC}  💿  Sleeve macOS Packager                           ${BLUE}║${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}"
    echo

    check_prereqs

    step "1/8  Building Sleeve"
    step_build

    step "2/8  Creating .app bundle skeleton"
    step_create_app

    step "3/8  Bundling GTK dylib dependencies"
    step_bundle_dylibs

    step "4/8  Copying GTK runtime resources"
    step_copy_gtk_resources

    step "5/8  Creating launcher with GTK environment"
    step_create_launcher

    step "6/8  Regenerating GdkPixbuf & icon caches"
    step_regenerate_caches

    step "7/8  Code-signing"
    step_codesign

    step "8/8  Creating DMG"
    step_create_dmg

    summary
}

main "$@"
