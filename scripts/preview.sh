#!/usr/bin/env bash
#
# Run one of the client's RENDER harnesses in a window that does not steal the keyboard.
# Invoke it from the REPO ROOT (it cd's there itself, but the path you type to reach it does not).
#
#   scripts/preview.sh res://tools/ui_preview.tscn
#   scripts/preview.sh res://tools/blend_probe.tscn -- --only=G
#
# RENDER harnesses only. The `--headless` guards (decode_guard, marker_field_guard,
# snapshot_alias_guard, …) open no window, so they have no focus to take and want the dummy
# renderer this wrapper's override would not give them -- run those bare, or through their
# `cargo xtask` entry point.
#
# WHY THIS EXISTS. A harness that captures pixels cannot run `--headless`: that selects the
# headless display driver, whose only rendering driver is `dummy`, so there is no viewport
# texture to read back. So every preview opens a real window, and it inherits `project.godot`'s
# player window -- FULLSCREEN, and focus-grabbing. Measured with
# `DisplayServer.window_is_focused()` from inside the run: the window took the keyboard, and on
# exit macOS left `loginwindow` frontmost rather than handing focus back -- so a verification run
# in one worktree cost a click in whatever OTHER session was being typed in, several times an hour.
#
# WHY IT IS A WRAPPER AND NOT A FLAG. Godot's own display flags are IGNORED when the project
# declares a fullscreen mode: `-w`, `--position` and `--resolution` all leave `window_get_mode()`
# reporting 3, with the flags placed either side of `--path` (measured, Godot 4.7). A custom flag
# read through `OS.get_cmdline_user_args()` is worse -- it is read after the window already
# exists, so the fullscreen window has already appeared and taken focus. The boot window can only
# be moved by a config file, and `override.cfg` is the only one Godot reads per-project-directory.
# So the harness runs get the quiet window and `project.godot` -- i.e. THE GAME -- is untouched:
# it still boots straight to fullscreen with no windowed frame in front of it.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CLIENT_DIR="clients/godot_thin_client"
OVERRIDE="$CLIENT_DIR/override.cfg"
# Stamped into the file so a run can tell ITS override from one a human wrote by hand, and so
# concurrent runs in the same worktree do not delete each other's.
MARKER="; written by scripts/preview.sh -- safe to delete"

usage() {
  cat <<'EOF'
Usage: scripts/preview.sh <res://tools/SCENE.tscn> [extra godot args...]

Renders a preview/probe harness with a window that is WINDOWED and NO-FOCUS, so it cannot take
the keyboard from another session. Reimport separately when scenes/scripts changed:

  godot --headless --path clients/godot_thin_client --import

The exit status is the harness's own.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

# A MISSING scene is not a help request, and it must not exit 0. `client-dev` gates its render
# step on this status, so an invocation whose scene argument expanded empty would otherwise print
# usage, render nothing, and report a clean pass.
if [[ $# -eq 0 ]]; then
  echo "[preview] no scene given." >&2
  usage >&2
  exit 2
fi

SCENE="$1"
shift

if [[ -e "$OVERRIDE" ]] && ! grep -qF "$MARKER" "$OVERRIDE"; then
  echo "[preview] $OVERRIDE exists and was not written by this script -- refusing to touch it." >&2
  echo "[preview] Move it aside and re-run, or delete it if it is stale." >&2
  exit 1
fi

# Only the run that CREATES the override removes it, so a second concurrent harness cannot pull
# the file out from under a third one that has not booted yet.
#
# The marker is re-checked HERE and not just at the entry test above, so "this script never
# clobbers an override it did not write" holds for the whole run rather than only at its start.
# The file can legitimately change underneath a long render: `run_stack.sh` clears a marked
# override whenever the client launches, and a human is then free to write their own. Removing by
# path alone would delete theirs on the way out.
OWNS_OVERRIDE=false
cleanup() {
  if [[ "$OWNS_OVERRIDE" == true && -e "$OVERRIDE" ]] && grep -qF "$MARKER" "$OVERRIDE"; then
    rm -f "$OVERRIDE"
  fi
}

if [[ ! -e "$OVERRIDE" ]]; then
  cat > "$OVERRIDE" <<EOF
$MARKER
[display]

window/size/mode=0
window/size/no_focus=true
EOF
  OWNS_OVERRIDE=true
  # EXIT alone does not fire on a signal under `set -e`, and a harness that is Ctrl-C'd must not
  # leave a file behind that would make the GAME's next launch unfocusable.
  trap cleanup EXIT INT TERM HUP
fi

set +e
godot --path "$CLIENT_DIR" "$SCENE" "$@"
rc=$?
set -e

cleanup
trap - EXIT INT TERM HUP
exit "$rc"
