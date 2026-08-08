#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-all}"

REPO_ROOT="$(git rev-parse --show-toplevel)"
GENERATED_BINDINGS="$REPO_ROOT/shadow_scale_flatbuffers/src/generated/snapshot_generated.rs"

run_flatbuffers() {
  echo "Regenerating FlatBuffers bindings"
  # build.rs declares rerun-if-changed on the .fbs schema only, so cargo will happily skip the
  # build script when its fingerprint is warm but the generated file is gone — and then fail to
  # compile the module that file provides. Drop this one package's fingerprint first in that case.
  # A fresh worktree has nothing to clean and pays nothing for the check.
  if [[ ! -f "$GENERATED_BINDINGS" ]]; then
    cargo clean -p shadow_scale_flatbuffers
  fi
  cargo build --locked -p shadow_scale_flatbuffers
}

run_godot_extension() {
  echo "Building Godot extension (release)"
  cargo build --release -p shadow_scale_godot
}

# Anything that reads the whole workspace needs the generated bindings on disk first — rustfmt
# walks the module tree before it formats anything, and clippy has to compile. A fresh worktree
# has an empty target/, so the first commit in one — including a markdown-only commit — used to
# die on "failed to resolve mod `snapshot_generated`" before either check ran. CI generates them
# in the same order for the same reason (.github/workflows/rust.yml).
ensure_bindings() {
  if [[ ! -f "$GENERATED_BINDINGS" ]]; then
    echo "Generated FlatBuffers bindings missing (fresh worktree?)"
    run_flatbuffers
  fi
}

run_fmt() {
  ensure_bindings
  echo "Running cargo fmt --all -- --check"
  cargo fmt --all -- --check
}

run_clippy() {
  ensure_bindings
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
