//! Build script: stamp a non-stale server build id into the crate.
//!
//! Computes `CORE_SIM_BUILD_ID` at compile time as `<commit-date>-<short-hash>`
//! (e.g. `2026-07-09-a1b2c3d`) by shelling out to `git`, so the value read by
//! `crate::BUILD_ID` always reflects the actual build and can never be a stale
//! hand-bumped constant. Falls back to `dev-unknown` when git is unavailable
//! (offline/CI/exported source) — robustness over precision. Re-runs whenever
//! HEAD or the checked-out ref moves so a new commit re-stamps.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Value stamped when git metadata cannot be resolved. Kept in sync with the
/// `option_env!` fallback in `lib.rs`.
const FALLBACK_BUILD_ID: &str = "dev-unknown";

fn main() {
    // The build script itself is a rerun trigger.
    println!("cargo:rerun-if-changed=build.rs");

    // Re-stamp when a new commit is checked out. `.git` lives at the workspace
    // root, not the crate dir, so locate it from CARGO_MANIFEST_DIR and handle a
    // worktree (`.git` is a file, not a directory) gracefully.
    if let Some(git_dir) = locate_git_dir() {
        register_head_rerun_triggers(&git_dir);
    }

    let build_id = git_build_id().unwrap_or_else(|| FALLBACK_BUILD_ID.to_string());
    println!("cargo:rustc-env=CORE_SIM_BUILD_ID={build_id}");
}

/// `<commit-date>-<short-hash>`, e.g. `2026-07-09-a1b2c3d`, or `None` if either
/// git invocation fails (git absent, not a repo, no commits yet).
fn git_build_id() -> Option<String> {
    let short_hash = git_output(&["rev-parse", "--short", "HEAD"])?;
    let commit_date = git_output(&["show", "-s", "--format=%cs", "HEAD"])?;
    if short_hash.is_empty() || commit_date.is_empty() {
        return None;
    }
    Some(format!("{commit_date}-{short_hash}"))
}

/// Run `git <args>` in the crate dir (inside the repo) and return trimmed stdout,
/// or `None` on any failure.
fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string())
}

/// Resolve the `.git` directory (following a worktree `.git` file) by walking up
/// from `CARGO_MANIFEST_DIR`. Returns `None` when no `.git` is found.
fn locate_git_dir() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    for ancestor in manifest_dir.ancestors() {
        let dot_git = ancestor.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            // Worktree: `.git` is a file containing `gitdir: <path>`.
            return resolve_worktree_git_dir(&dot_git);
        }
    }
    None
}

/// Parse a worktree `.git` file (`gitdir: <path>`) into an absolute `.git` dir.
fn resolve_worktree_git_dir(git_file: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(git_file).ok()?;
    let raw = contents.strip_prefix("gitdir:")?.trim();
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        git_file.parent()?.join(path)
    };
    Some(path)
}

/// Emit `rerun-if-changed` for `HEAD` and the ref it points at, so committing on
/// the current branch re-stamps the build id. Best-effort: a detached HEAD (no
/// symbolic ref) or packed refs simply rely on the `HEAD` trigger.
fn register_head_rerun_triggers(git_dir: &Path) {
    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    if let Ok(head) = std::fs::read_to_string(&head_path) {
        if let Some(ref_rel) = head.strip_prefix("ref:") {
            let ref_path = git_dir.join(ref_rel.trim());
            println!("cargo:rerun-if-changed={}", ref_path.display());
        }
    }
}
