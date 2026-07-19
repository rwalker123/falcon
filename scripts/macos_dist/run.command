#!/usr/bin/env bash
# ============================================================================
#  ShadowScale playtest launcher (macOS)
#
#  The game is two programs: a simulation SERVER and a game CLIENT that talk to
#  each other over local ports (127.0.0.1). This script starts the server, waits
#  for it to bind, launches the client, and stops the server when you quit.
#
#  Double-click this file in Finder. (First time: see README.txt — macOS
#  quarantines apps downloaded from the internet.)
# ============================================================================
set -u
cd "$(dirname "$0")"

# Best-effort: clear the download quarantine on this package so the app opens.
xattr -dr com.apple.quarantine . 2>/dev/null || true

APP_BUNDLE="ShadowScaleClient.app"
# Godot names the bundle's inner binary after the *project* name, not the export
# filename, so read the authoritative name out of the bundle instead of guessing.
APP_EXEC="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' \
  "$APP_BUNDLE/Contents/Info.plist" 2>/dev/null || true)"
APP="$APP_BUNDLE/Contents/MacOS/$APP_EXEC"
if [ -z "$APP_EXEC" ] || [ ! -x "$APP" ]; then
  echo "Could not find the client program inside $APP_BUNDLE — is the package unzipped fully?"
  read -r -p "Press Return to close."
  exit 1
fi
if [ ! -x "./server" ]; then
  echo "Could not find the 'server' program (or it isn't executable) — is the package unzipped fully?"
  read -r -p "Press Return to close."
  exit 1
fi

echo "Starting ShadowScale server..."
./server &
SERVER_PID=$!

# Stop the server whenever this script exits (client quit, Ctrl-C, error).
trap 'kill "$SERVER_PID" 2>/dev/null' EXIT

# Give the server a moment to bind 127.0.0.1:41000-41003.
sleep 2

echo "Starting ShadowScale client..."
"$APP"

echo "Shutting down server..."
