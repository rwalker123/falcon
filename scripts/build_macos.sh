#!/usr/bin/env bash
#
# build_macos.sh — build a macOS playtest package (native, on a Mac).
#
# Produces a ZIP a macOS playtester unzips and runs, with no toolchain and no
# config files on their side:
#
#   ShadowScale-macos/
#     run.command             # double-click — starts the server, then the client
#     ShadowScaleClient.app   # the Godot client (.dylib + game data embedded)
#     server                  # the core_sim server (binds 127.0.0.1:41000-41003)
#     README.txt
#
# Like the Windows build this is a CLIENT/SERVER package: the client connects to
# the server over 127.0.0.1 (default ports line up), so both ship and run.command
# starts them in order.
#
# Prerequisites:
#   - Rust stable, flatc on PATH (the FlatBuffers bindings are generated at build)
#   - Godot 4.7 with the macOS export templates installed
#
# Env:
#   GODOT_BIN      path to the godot binary (default: 'godot')
#   BUILD_NUMBER   optional; when set, the zip is named ShadowScale-macos-b<N>.zip

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

GODOT_PROJECT="clients/godot_thin_client"
DYLIB_NAME="libshadow_scale_godot.dylib"
SERVER_NAME="server"
APP_NAME="ShadowScaleClient.app"
PRESET="macOS"

DIST_ROOT="$ROOT_DIR/dist/macos"
PKG_NAME="ShadowScale-macos"
PKG_DIR="$DIST_ROOT/$PKG_NAME"

GODOT_BIN="${GODOT_BIN:-godot}"
BUILD_NUMBER="${BUILD_NUMBER:-}"

info() { printf '\033[1;36m[mac-build]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[mac-build] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null       || die "cargo not found"
command -v "$GODOT_BIN" >/dev/null || die "Godot ('$GODOT_BIN') not on PATH — set GODOT_BIN=/path/to/godot"

# --- 1. build the native server + GDExtension (godot-build copies the dylib) ---
info "Building server + GDExtension (native) ..."
cargo build --release --locked -p core_sim --bin server
cargo xtask godot-build   # builds shadow_scale_godot + copies dylib to native/bin/macos

SERVER_SRC="$ROOT_DIR/target/release/$SERVER_NAME"
[ -f "$SERVER_SRC" ] || die "server build produced no $SERVER_NAME"
[ -f "$ROOT_DIR/$GODOT_PROJECT/native/bin/macos/$DYLIB_NAME" ] || die "godot-build produced no $DYLIB_NAME"

# --- 2. export the Godot client .app (headless) --------------------------------
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR"
info "Exporting Godot client ('$PRESET') ..."
# Godot can crash on SHUTDOWN after writing a perfectly good bundle, so the exit code alone
# is not a trustworthy verdict — gate on the artifact and treat a late failure as a warning.
set +e
"$GODOT_BIN" --headless --path "$GODOT_PROJECT" \
  --export-release "$PRESET" "$PKG_DIR/$APP_NAME" 2>&1 | tee /tmp/ss_godot_export_macos.log
godot_status=${PIPESTATUS[0]}
set -e

# The bundle is only usable if Godot actually wrote its executable, so check that — not just
# the directory, which it creates before it can fail.
APP_EXEC="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' \
  "$PKG_DIR/$APP_NAME/Contents/Info.plist" 2>/dev/null || true)"
if [ -z "$APP_EXEC" ] || [ ! -x "$PKG_DIR/$APP_NAME/Contents/MacOS/$APP_EXEC" ]; then
  die "Godot export produced no usable $APP_NAME (exit $godot_status) — see /tmp/ss_godot_export_macos.log (macOS templates installed? preset '$PRESET' present?)"
fi
if [ "$godot_status" -ne 0 ]; then
  info "WARNING: Godot exited $godot_status but the export is complete — continuing."
fi

# --- 3. assemble the package ---------------------------------------------------
cp "$SERVER_SRC" "$PKG_DIR/$SERVER_NAME"
chmod +x "$PKG_DIR/$SERVER_NAME"
cp "$SCRIPT_DIR/macos_dist/run.command" "$PKG_DIR/run.command"
chmod +x "$PKG_DIR/run.command"
cp "$SCRIPT_DIR/macos_dist/README.txt"  "$PKG_DIR/README.txt"

# --- 4. zip it (ditto preserves the .app bundle + exec bits) -------------------
ZIP_NAME="$PKG_NAME"
[ -n "$BUILD_NUMBER" ] && ZIP_NAME="${PKG_NAME}-b${BUILD_NUMBER}"
info "Zipping package ..."
( cd "$DIST_ROOT" && rm -f "$ZIP_NAME.zip" && ditto -c -k --keepParent "$PKG_NAME" "$ZIP_NAME.zip" )

info "Done."
info "Package : $PKG_DIR"
info "Zip     : $DIST_ROOT/$ZIP_NAME.zip"
