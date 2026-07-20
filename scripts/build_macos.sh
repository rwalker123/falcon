#!/usr/bin/env bash
#
# build_macos.sh — build a macOS playtest package (native, on a Mac).
#
# Produces a ZIP a macOS playtester unzips and runs, with no toolchain and no
# config files on their side:
#
#   ShadowScale-macos/
#     ShadowScale.app         # double-click — ONE bundle, everything inside it
#       Contents/MacOS/shadowscale_launcher    # the launcher (crate `launcher`)
#       Contents/Helpers/server                # the core_sim server
#       Contents/Helpers/ShadowScaleClient.app # the Godot client (.dylib + data embedded)
#     README.txt
#
# Like the Windows build this is a CLIENT/SERVER package: the client connects to
# the server over 127.0.0.1, so both ship and the launcher starts them in order.
# The launcher hands both halves an explicit SIM_PORTS_FILE so they rendezvous on
# whatever block the server actually binds (see core_sim/src/port_alloc.rs).
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
# The outer bundle the player double-clicks; the client .app nests inside it.
LAUNCHER_APP="ShadowScale.app"
LAUNCHER_NAME="shadowscale_launcher"
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

# --- 1. build the native server + launcher + GDExtension (godot-build copies the dylib) ---
info "Building server + launcher + GDExtension (native) ..."
cargo build --release --locked -p core_sim --bin server
cargo build --release --locked -p launcher --bin "$LAUNCHER_NAME"
cargo xtask godot-build   # builds shadow_scale_godot + copies dylib to native/bin/macos

SERVER_SRC="$ROOT_DIR/target/release/$SERVER_NAME"
LAUNCHER_SRC="$ROOT_DIR/target/release/$LAUNCHER_NAME"
[ -f "$SERVER_SRC" ] || die "server build produced no $SERVER_NAME"
[ -f "$LAUNCHER_SRC" ] || die "launcher build produced no $LAUNCHER_NAME"
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

# --- 3. assemble the single ShadowScale.app bundle -----------------------------
# One bundle, not a folder of loose pieces: the launcher is the bundle executable,
# and the server + the Godot client .app ride along in Contents/Helpers/. The
# player double-clicks ONE thing, and Gatekeeper has ONE thing to approve.
info "Assembling $LAUNCHER_APP ..."
LAUNCHER_APP_DIR="$PKG_DIR/$LAUNCHER_APP"
mkdir -p "$LAUNCHER_APP_DIR/Contents/MacOS" "$LAUNCHER_APP_DIR/Contents/Helpers"

cp "$SCRIPT_DIR/macos_dist/Launcher-Info.plist" "$LAUNCHER_APP_DIR/Contents/Info.plist"
cp "$LAUNCHER_SRC" "$LAUNCHER_APP_DIR/Contents/MacOS/$LAUNCHER_NAME"
chmod +x "$LAUNCHER_APP_DIR/Contents/MacOS/$LAUNCHER_NAME"
cp "$SERVER_SRC" "$LAUNCHER_APP_DIR/Contents/Helpers/$SERVER_NAME"
chmod +x "$LAUNCHER_APP_DIR/Contents/Helpers/$SERVER_NAME"
# The client was exported to the package root; move it inside the bundle.
mv "$PKG_DIR/$APP_NAME" "$LAUNCHER_APP_DIR/Contents/Helpers/$APP_NAME"

cp "$SCRIPT_DIR/macos_dist/README.txt"  "$PKG_DIR/README.txt"

# --- 3b. ad-hoc code-sign, INSIDE OUT ------------------------------------------
# Unsigned arm64 binaries can fail to launch outright ("app is damaged"), which
# offers the player no override at all. An ad-hoc signature (`-s -`) costs nothing,
# needs no Apple Developer account, and downgrades that hard failure to the ordinary
# unidentified-developer prompt the README walks through. It is NOT notarization —
# Gatekeeper still asks once.
#
# Order matters: a signature covers its bundle's contents, so every nested piece
# must be signed BEFORE the thing that contains it, or sealing the outer bundle
# invalidates it. Hence deepest-first, outer bundle last.
#
# Godot's own export signing stays off (codesign/codesign=0 in export_presets.cfg):
# all signing for the package happens here, in one place, in one order.
info "Ad-hoc signing the bundle (inside out) ..."
codesign --force --sign - --timestamp=none \
  "$LAUNCHER_APP_DIR/Contents/Helpers/$SERVER_NAME"
codesign --force --sign - --timestamp=none \
  "$LAUNCHER_APP_DIR/Contents/Helpers/$APP_NAME"
codesign --force --sign - --timestamp=none \
  "$LAUNCHER_APP_DIR/Contents/MacOS/$LAUNCHER_NAME"
codesign --force --sign - --timestamp=none "$LAUNCHER_APP_DIR"
codesign --verify --deep --strict "$LAUNCHER_APP_DIR" \
  || die "ad-hoc signature did not verify — the package would fail to launch"

# --- 4. zip it (ditto preserves the .app bundle + exec bits) -------------------
ZIP_NAME="$PKG_NAME"
[ -n "$BUILD_NUMBER" ] && ZIP_NAME="${PKG_NAME}-b${BUILD_NUMBER}"
info "Zipping package ..."
( cd "$DIST_ROOT" && rm -f "$ZIP_NAME.zip" && ditto -c -k --keepParent "$PKG_NAME" "$ZIP_NAME.zip" )

info "Done."
info "Package : $PKG_DIR"
info "Zip     : $DIST_ROOT/$ZIP_NAME.zip"
