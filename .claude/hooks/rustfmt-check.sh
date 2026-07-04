#!/usr/bin/env bash
# Stop hook: block turn end when Rust is not rustfmt-clean, but only if .rs files
# changed this turn. Keeps the pre-commit `cargo fmt --check` hook from surprising
# the user at commit time.
#
# Reads the Stop-hook JSON on stdin; emits a {"decision":"block",...} object when
# formatting is needed so the model is told to run `cargo fmt --all`.
input=$(cat)

# Avoid re-blocking loops: if this stop already followed a hook continuation, let it pass.
if [ "$(jq -r '.stop_hook_active // false' <<<"$input")" = "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$PWD}" || exit 0

# Only relevant when Rust files are dirty (modified/staged/untracked).
git status --porcelain 2>/dev/null | grep -qE '[.]rs$' || exit 0

if cargo fmt --all -- --check >/dev/null 2>&1; then
  exit 0
fi

printf '%s' '{"decision":"block","reason":"Rust is not rustfmt-clean. Run: cargo fmt --all before finishing (the pre-commit hook enforces fmt and will otherwise fail the commit)."}'
