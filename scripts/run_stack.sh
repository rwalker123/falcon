#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_SERVER=true
RUN_CLIENT=true

usage() {
  cat <<'EOF'
Usage: scripts/run_stack.sh [--server-only|--client-only] [--help]
  --server-only  Start only the core simulation server.
  --client-only  Launch only the thin client (expects a running server).
  -h, --help     Show this help text.

Without any options both the server and the client are started.
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

if [[ "$RUN_SERVER" == false && "$RUN_CLIENT" == false ]]; then
  echo "[run_stack] Nothing to do: both server and client disabled." >&2
  usage >&2
  exit 1
fi

if [[ "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Building Godot package..."
  cargo xtask godot-build
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

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == false ]]; then
  echo "[run_stack] Starting core simulation server..."
  exec env RUST_LOG=shadow_scale::server=info,core_sim=info cargo run -p core_sim --bin server
fi

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Starting core simulation server..."
  RUST_LOG=shadow_scale::server=info,core_sim=info cargo run -p core_sim --bin server &
  SERVER_PID=$!
  trap cleanup EXIT INT TERM
fi

CLIENT_EXIT_CODE=0

if [[ "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Launching thin client..."
  set +e
  STREAM_ENABLED=true \
  STREAM_HOST=127.0.0.1 \
  STREAM_PORT=41002 \
  COMMAND_HOST=127.0.0.1 \
  COMMAND_PORT=41001 \
  INSPECTOR_FONT_SIZE=96 \
  godot --path clients/godot_thin_client
  CLIENT_EXIT_CODE=$?
  set -e
fi

if [[ "$RUN_SERVER" == true && "$RUN_CLIENT" == true ]]; then
  echo "[run_stack] Thin client exited (code=${CLIENT_EXIT_CODE}). Stopping server..."
  trap - EXIT INT TERM
  cleanup
fi

exit "$CLIENT_EXIT_CODE"
