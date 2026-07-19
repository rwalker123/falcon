#!/usr/bin/env bash
#
# build_windows.sh — cross-compile a Windows playtest package from macOS/Linux.
#
# Produces a self-contained ZIP a Windows playtester unzips and runs, with no
# toolchain, no Godot, and no config files on their side:
#
#   ShadowScale-windows/
#     server.exe                     # core_sim server (binds 127.0.0.1:41000-41003)
#     ShadowScaleClient.exe          # Godot thin client (+ its .pck)
#     shadow_scale_godot.dll         # the GDExtension, beside the client exe
#     run.bat                        # launches the server, then the client
#     README.txt
#
# This is a CLIENT/SERVER game: the Godot client connects to the server over TCP
# (defaults line up — client STREAM=41002/COMMAND=41001/LOG=41003 vs the server's
# default binds), so the package ships BOTH and run.bat starts them in order.
#
# Prerequisites (one-time, see docs/desktop_builds.md):
#   - rustup target add x86_64-pc-windows-msvc
#   - cargo install cargo-xwin
#   - brew install llvm            (provides lld — the MSVC-ABI linker)
#   - Godot 4.7 export templates   (via the editor: Editor > Manage Export Templates,
#                                    or `godot --headless --install-export-templates`)
#
# Everything is MSVC-ABI (matches Godot's official Windows builds); cargo-xwin
# downloads the Windows CRT/SDK on first run.

set -euo pipefail

# --- resolve paths (works from any CWD, honors the worktree it lives in) ------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

TARGET="x86_64-pc-windows-msvc"
GODOT_PROJECT="clients/godot_thin_client"
DLL_NAME="shadow_scale_godot.dll"
SERVER_NAME="server.exe"
CLIENT_EXE="ShadowScaleClient.exe"
PRESET="Windows Desktop"

DIST_ROOT="$ROOT_DIR/dist/windows"
PKG_NAME="ShadowScale-windows"
PKG_DIR="$DIST_ROOT/$PKG_NAME"

# LLVM's lld-link + clang-cl come from the Homebrew llvm keg; put it on PATH so
# cargo-xwin can find them without the caller exporting anything.
if [ -d /opt/homebrew/opt/llvm/bin ]; then
  export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
elif [ -d /usr/local/opt/llvm/bin ]; then
  export PATH="/usr/local/opt/llvm/bin:$PATH"
fi

GODOT_BIN="${GODOT_BIN:-godot}"
# When set (CI passes the run number), the zip is named ShadowScale-windows-b<N>.zip.
BUILD_NUMBER="${BUILD_NUMBER:-}"

info()  { printf '\033[1;36m[win-build]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[win-build]\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31m[win-build] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# --- preflight ----------------------------------------------------------------
command -v cargo >/dev/null       || die "cargo not found"
cargo xwin --version >/dev/null 2>&1 || die "cargo-xwin not installed — run: cargo install cargo-xwin"
rustup target list --installed 2>/dev/null | grep -q "$TARGET" \
  || die "Rust target missing — run: rustup target add $TARGET"
command -v "$GODOT_BIN" >/dev/null || die "Godot ('$GODOT_BIN') not on PATH — set GODOT_BIN=/path/to/godot"
command -v zip >/dev/null         || die "zip not found (needed to package the build)"

# --- 1. cross-compile the Rust artifacts (server + GDExtension) ---------------
info "Cross-compiling server + GDExtension for $TARGET ..."
cargo xwin build --release --locked --target "$TARGET" -p core_sim --bin server
cargo xwin build --release --locked --target "$TARGET" -p shadow_scale_godot

REL="$ROOT_DIR/target/$TARGET/release"
[ -f "$REL/$SERVER_NAME" ] || die "server build produced no $SERVER_NAME"
[ -f "$REL/$DLL_NAME" ]    || die "GDExtension build produced no $DLL_NAME"

# --- 2. stage the DLL where the .gdextension expects it, so export bundles it --
DLL_DEST="$ROOT_DIR/$GODOT_PROJECT/native/bin/windows"
mkdir -p "$DLL_DEST"
cp "$REL/$DLL_NAME" "$DLL_DEST/$DLL_NAME"
info "Staged $DLL_NAME -> $GODOT_PROJECT/native/bin/windows/"

# --- 3. export the Godot client .exe (headless) -------------------------------
# Clear any prior package so a rerun can't ship stale artifacts (e.g. an old .pck).
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR"
info "Exporting Godot client ('$PRESET') ..."
# --export-release fails hard if templates or the preset are missing, but Godot can also crash
# on SHUTDOWN after writing a good .exe — so the exit code alone is not a trustworthy verdict.
# Gate on the artifact and treat a late failure as a warning (mirrors build_macos.sh).
set +e
"$GODOT_BIN" --headless --path "$GODOT_PROJECT" \
  --export-release "$PRESET" "$PKG_DIR/$CLIENT_EXE" 2>&1 | tee /tmp/ss_godot_export.log
godot_status=${PIPESTATUS[0]}
set -e

[ -s "$PKG_DIR/$CLIENT_EXE" ] || die "Godot export produced no usable $CLIENT_EXE (exit $godot_status) — see /tmp/ss_godot_export.log (templates installed? preset '$PRESET' present?)"
if [ "$godot_status" -ne 0 ]; then
  info "WARNING: Godot exited $godot_status but the export is complete — continuing."
fi

# --- 4. assemble the package --------------------------------------------------
cp "$REL/$SERVER_NAME" "$PKG_DIR/$SERVER_NAME"
# Godot copies the GDExtension DLL next to the exe on export; copy defensively in
# case the export embed setting changes. Not silenced — the source is verified to
# exist above, so a failure here is a real problem worth surfacing.
cp "$REL/$DLL_NAME" "$PKG_DIR/$DLL_NAME"
cp "$SCRIPT_DIR/windows_dist/run.bat"    "$PKG_DIR/run.bat"
cp "$SCRIPT_DIR/windows_dist/README.txt" "$PKG_DIR/README.txt"

# --- 5. zip it ----------------------------------------------------------------
ZIP_NAME="$PKG_NAME"
[ -n "$BUILD_NUMBER" ] && ZIP_NAME="${PKG_NAME}-b${BUILD_NUMBER}"
info "Zipping package ..."
( cd "$DIST_ROOT" && rm -f "$ZIP_NAME.zip" && zip -r -q "$ZIP_NAME.zip" "$PKG_NAME" )

info "Done."
info "Package : $PKG_DIR"
info "Zip     : $DIST_ROOT/$ZIP_NAME.zip"
info "Hand the ZIP to a Windows playtester — they unzip and double-click run.bat."
