#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-all}"

run_flatbuffers() {
  echo "Regenerating FlatBuffers bindings"
  cargo build --locked -p shadow_scale_flatbuffers
}

run_godot_extension() {
  echo "Building Godot extension (release)"
  cargo build --release -p shadow_scale_godot
}

run_fmt() {
  echo "Running cargo fmt --all -- --check"
  cargo fmt --all -- --check
}

run_clippy() {
  echo "Running cargo clippy --workspace --all-targets --all-features -- -D warnings"
  cargo clippy --workspace --all-targets --all-features -- -D warnings
}

case "$ACTION" in
  flatbuffers)
    run_flatbuffers
    ;;
  godot)
    run_godot_extension
    ;;
  fmt)
    run_fmt
    ;;
  clippy)
    run_clippy
    ;;
  all)
    run_flatbuffers
    run_godot_extension
    run_fmt
    run_clippy
    ;;
  *)
    echo "Unknown action: $ACTION" >&2
    echo "Usage: $0 [flatbuffers|godot|fmt|clippy|all]" >&2
    exit 1
    ;;
esac
