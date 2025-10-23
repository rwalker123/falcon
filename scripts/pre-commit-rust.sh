#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-all}"

run_fmt() {
  echo "Running cargo fmt --all -- --check"
  cargo fmt --all -- --check
}

run_clippy() {
  echo "Running cargo clippy --workspace --all-targets --all-features -- -D warnings"
  cargo clippy --workspace --all-targets --all-features -- -D warnings
}

case "$ACTION" in
  fmt)
    run_fmt
    ;;
  clippy)
    run_clippy
    ;;
  all)
    run_fmt
    run_clippy
    ;;
  *)
    echo "Unknown action: $ACTION" >&2
    echo "Usage: $0 [fmt|clippy|all]" >&2
    exit 1
    ;;
esac
