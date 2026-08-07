#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_SERVER=true
RUN_CLIENT=true
RUN_GODOT=false
PORT_BASE=""

# Default port base; base+0 is RESERVED (it carried the retired bincode
# snapshot socket, #388), +1=command, +2=flat/stream, +3=log.
# base=41000 reproduces the historical fixed ports (41000..41003).
DEFAULT_PORT_BASE=41000
# Auto-derived bases are spaced this far apart so each worktree gets its own
# contiguous block of four slots without overlapping its neighbours.
PORT_BLOCK_STRIDE=10
# Number of distinct auto-derived slots (DEFAULT_PORT_BASE .. +99*stride).
PORT_SLOT_COUNT=100

# The server is built RELEASE by default. A debug core_sim is ~15x slower per turn, which the
# player feels directly: click-to-updated-map measured at ~1.9s debug vs ~0.1s release on an
# 80x52 map. That penalty is almost entirely INVISIBLE in the turn profile -- the sim's own
# `turn.completed` is only 161ms of it -- so it reads as "the game is just slow" rather than as a
# build-flag choice, which is exactly how it went unnoticed while #386 optimised the other 6%.
# `--debug` restores the unoptimised build for the two things it actually buys: integer-overflow
# traps (release wraps SILENTLY, and this sim runs on fixed-point Scalar math) and
# debug_assertions. See also the [profile.dev] block in the workspace Cargo.toml, which keeps
# that path usable by optimising dependencies even in a debug build.
SERVER_PROFILE_FLAG="--release"

usage() {
  cat <<'EOF'
Usage: scripts/run_stack.sh [--server-only|--client-only|--godot-only] [--port-base N] [--debug] [--help]
  --server-only  Start only the core simulation server.
  --client-only  Launch only the thin client (expects a running server).
  --godot-only   Launch a bare Godot editor wired to the sim ports.
  --port-base N  Base port for this run. The server binds N..N+3; the client
                 connects to the matching ports. Overrides auto-derivation.
  --debug        Build the server WITHOUT optimisation. Measured cost: a turn
                 goes from ~10ms to ~161ms, and click-to-updated-map from
                 ~0.1s to ~1.9s. Worth it only for the overflow traps and
                 debug_assertions a debug build adds -- never for normal play.
  -h, --help     Show this help text.

Without any options both the server and the client are started.

Port selection (so multiple worktrees/checkouts don't collide):
  1. --port-base N, if given.
  2. Else the SIM_PORT_BASE environment variable, if set.
  3. Else a base derived from this checkout's path. When starting the server,
     the derived block is probed and bumped until four free ports are found.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server-only)
      RUN_CLIENT=false
      ;;
    --client-only)
      RUN_SERVER=false
      ;;
    --godot-only)
      RUN_SERVER=false
      RUN_CLIENT=false
      RUN_GODOT=true
      ;;
    --port-base)
      shift
      if [[ $# -eq 0 ]]; then
        echo "[run_stack] --port-base requires a value" >&2
        exit 1
      fi
      PORT_BASE="$1"
      ;;
    --port-base=*)
      PORT_BASE="${1#*=}"
      ;;
    --debug)
      SERVER_PROFILE_FLAG=""
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[run_stack] Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if [[ "$RUN_SERVER" == false && "$RUN_CLIENT" == false && "$RUN_GODOT" != true ]]; then
  echo "[run_stack] Nothing to do: both server and client disabled." >&2
  usage >&2
  exit 1
fi

# --- Port base resolution -----------------------------------------------------

# True if something is already listening on 127.0.0.1:$1.
port_in_use() {
  (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null && { exec 3>&- 3<&-; return 0; }
  return 1
}

# True if every BOUND port in the block starting at $1 is free. Slot 0 is
# reserved and nothing binds it, so a stranger sitting on it must not push us to
# another block.
block_free() {
  local base="$1" offset
  for offset in 1 2 3; do
    if port_in_use "$((base + offset))"; then
      return 1
    fi
  done
  return 0
}

port_base_explicit=false
if [[ -n "$PORT_BASE" ]]; then
  port_base_explicit=true
elif [[ -n "${SIM_PORT_BASE:-}" ]]; then
  PORT_BASE="$SIM_PORT_BASE"
  port_base_explicit=true
else
  # Deterministic per-checkout derivation: identical for repeated runs from the
  # same path (so a plain --client-only run reconnects to the block the server
  # used), but distinct across worktrees. Caveat: if the server had to bump off a
  # port collision below, this un-bumped base no longer matches — pass an explicit
  # --port-base / SIM_PORT_BASE to a later --client-only run so both agree.
  path_hash="$(printf '%s' "$ROOT_DIR" | cksum | cut -d' ' -f1)"
  PORT_BASE="$((DEFAULT_PORT_BASE + (path_hash % PORT_SLOT_COUNT) * PORT_BLOCK_STRIDE))"
fi

if ! [[ "$PORT_BASE" =~ ^[0-9]+$ ]]; then
  echo "[run_stack] Invalid port base: '$PORT_BASE' (must be a number)" >&2
  exit 1
fi
if (( PORT_BASE < 1 || PORT_BASE + 3 > 65535 )); then
  echo "[run_stack] Port base $PORT_BASE out of range (need base+3 <= 65535)" >&2
  exit 1
fi

# When we're about to start the server on an auto-derived block, make sure the
# bound ports are actually free; otherwise bump to the next slot (wrapping).
if [[ "$RUN_SERVER" == true && "$port_base_explicit" == false ]]; then
  attempts=0
  while ! block_free "$PORT_BASE"; do
    slot="$(( ( (PORT_BASE - DEFAULT_PORT_BASE) / PORT_BLOCK_STRIDE + 1 ) % PORT_SLOT_COUNT ))"
    PORT_BASE="$((DEFAULT_PORT_BASE + slot * PORT_BLOCK_STRIDE))"
    attempts="$((attempts + 1))"
    if (( attempts >= PORT_SLOT_COUNT )); then
      echo "[run_stack] No free port block found after ${attempts} tries; using $PORT_BASE anyway." >&2
      break
    fi
  done
fi

COMMAND_PORT="$((PORT_BASE + 1))"
STREAM_PORT="$((PORT_BASE + 2))"
LOG_PORT="$((PORT_BASE + 3))"

# The server reads SIM_PORT_BASE; the client reads the individual ports.
export SIM_PORT_BASE="$PORT_BASE"

echo "[run_stack] Port base $PORT_BASE (command=$COMMAND_PORT stream=$STREAM_PORT log=$LOG_PORT; base+0 reserved)"

# --- Build --------------------------------------------------------------------

# Regenerate Godot's global class-name cache when stale. `class_name`-typed
# references (e.g. Hud.gd's `var _band_city_panel: BandCityPanel`) only resolve
# if the class is registered in .godot/global_script_class_cache.cfg, which is
# rewritten by the *editor's* project scan — never by launching the client. So
# after pulling a branch that adds a new `class_name`, a plain client run fails
# with "Could not find type ...". The cache is gitignored (a per-checkout
# artifact), so nothing else keeps it current. Rebuild it if it's missing or if
# any script is newer than it (analogous to build_terrain_textures.sh).
ensure_godot_class_cache() {
  local client_dir="$ROOT_DIR/clients/godot_thin_client"
  local cache="$client_dir/.godot/global_script_class_cache.cfg"

  if [[ -f "$cache" ]] \
      && ! find "$client_dir" -name '*.gd' -newer "$cache" -print -quit \
             2>/dev/null | grep -q .; then
    return 0
  fi

  echo "[run_stack] Regenerating Godot class cache (stale or missing)..."
  if ! godot --headless --editor --quit --path "$client_dir" >/dev/null 2>&1; then
    echo "[run_stack] Warning: class-cache regeneration exited non-zero; continuing." >&2
  fi
}

if [[ "$RUN_CLIENT" == true || "$RUN_GODOT" == true ]]; then
  echo "[run_stack] Building Godot package..."
  cargo xtask godot-build
  # Build terrain textures if out of date
  "$ROOT_DIR/scripts/build_terrain_textures.sh"
  # Refresh the global class-name cache if scripts changed since it was built.
  ensure_godot_class_cache
  # Stamp the client build id (mirrors core_sim/build.rs' CORE_SIM_BUILD_ID): the
  # git <commit-date>-<short-hash>, plus -dirty when the tree has uncommitted edits.
  # ClientBuild.gd reads this into the build overlay, so the shown cli build can never
  # go stale. On any git failure, remove the stamp so the client falls back to its const.
  stamp_file="$ROOT_DIR/clients/godot_thin_client/build_stamp.txt"
  if build_date=$(git -C "$ROOT_DIR" show -s --format=%cs HEAD 2>/dev/null) \
     && build_hash=$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null) \
     && [[ -n "$build_date" && -n "$build_hash" ]]; then
    build_stamp="${build_date}-${build_hash}"
    [[ -n "$(git -C "$ROOT_DIR" status --porcelain 2>/dev/null)" ]] && build_stamp="${build_stamp}-dirty"
    printf '%s\n' "$build_stamp" > "$stamp_file"
    echo "[run_stack] Client build stamp: $build_stamp"
  else
    rm -f "$stamp_file"
    echo "[run_stack] Client build stamp: git unavailable, using const fallback"
  fi
fi

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    echo
    echo "[run_stack] Shutting down server (pid=$SERVER_PID)..."
    if kill -0 "$SERVER_PID" 2>/dev/null; then
      kill "$SERVER_PID" 2>/dev/null || true
      wait "$SERVER_PID" 2>/dev/null || true
    fi
  fi
}

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == false && "$RUN_GODOT" != true ]]; then
  echo "[run_stack] Starting core simulation server..."
  exec env RUST_LOG=info SIM_PORT_BASE="$PORT_BASE" cargo run $SERVER_PROFILE_FLAG -p core_sim --bin server
fi

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Starting core simulation server..."
  RUST_LOG=info SIM_PORT_BASE="$PORT_BASE" cargo run $SERVER_PROFILE_FLAG -p core_sim --bin server &
  SERVER_PID=$!
  trap cleanup EXIT INT TERM
fi

CLIENT_EXIT_CODE=0

if [[ "$RUN_CLIENT" == true || "$RUN_GODOT" == true ]]; then
  echo "[run_stack] Launching thin client..."
  # `scripts/preview.sh` drops an override.cfg so a RENDER HARNESS gets a windowed, no-focus
  # window; it removes it again on exit, including on Ctrl-C. A SIGKILL or a crash can still
  # strand one, and a stranded override boots the GAME windowed and UNFOCUSABLE -- an app you
  # cannot click into, with nothing on screen saying why. This is the self-heal: the game's own
  # launcher clears any override it recognises as the harness's. A preview that boots straight
  # after merely renders loud, which is a nuisance rather than a defect.
  preview_override="$ROOT_DIR/clients/godot_thin_client/override.cfg"
  if [[ -e "$preview_override" ]] && grep -q 'written by scripts/preview.sh' "$preview_override"; then
    rm -f "$preview_override"
    echo "[run_stack] Cleared a stranded scripts/preview.sh override.cfg"
  fi
  set +e
  if [[ "$RUN_GODOT" == true ]]; then
    env \
      STREAM_ENABLED=true \
      STREAM_HOST=127.0.0.1 \
      STREAM_PORT="$STREAM_PORT" \
      COMMAND_HOST=127.0.0.1 \
      COMMAND_PORT="$COMMAND_PORT" \
      COMMAND_PROTO_PORT="$COMMAND_PORT" \
      LOG_HOST=127.0.0.1 \
      LOG_PORT="$LOG_PORT" \
      INSPECTOR_FONT_SIZE=32 \
      godot
  else
    env \
      STREAM_ENABLED=true \
      STREAM_HOST=127.0.0.1 \
      STREAM_PORT="$STREAM_PORT" \
      COMMAND_HOST=127.0.0.1 \
      COMMAND_PORT="$COMMAND_PORT" \
      COMMAND_PROTO_PORT="$COMMAND_PORT" \
      LOG_HOST=127.0.0.1 \
      LOG_PORT="$LOG_PORT" \
      INSPECTOR_FONT_SIZE=32 \
      godot --path clients/godot_thin_client
  fi
  CLIENT_EXIT_CODE=$?
  set -e
fi

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Thin client exited (code=${CLIENT_EXIT_CODE}). Stopping server..."
  trap - EXIT INT TERM
  cleanup
fi

exit "$CLIENT_EXIT_CODE"
