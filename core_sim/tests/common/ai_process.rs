//! **The player-process helpers** shared by the tests that drive a built `sim_ai` against a built
//! `server` over the real sockets — `ai_seat_scenario.rs` and `ai_bench.rs`.
//!
//! `CARGO_BIN_EXE_server` is only defined for the package owning that bin, and the `sim_ai` binary
//! is resolved as its **sibling** in the target directory. `cargo test --workspace` builds it
//! there before any test runs (`sim_ai/tests/crate_boundary.rs` is an integration-test target, and
//! a package with one has its binaries built). Under a narrower invocation the sibling can be
//! absent, in which case it is built into a private target directory — private, because the outer
//! `cargo test` holds the shared one's lock for the whole run.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// The private target directory the fallback build uses (module docs).
const FALLBACK_TARGET_DIR: &str = "ai_process_fallback";
/// How many lines of a log a failure message quotes.
pub const LOG_TAIL_LINES: usize = 40;

/// The introducer of an ANSI escape sequence and the byte that ends an SGR one.
const ANSI_ESCAPE: char = '\x1b';
const ANSI_SGR_END: char = 'm';

/// A scratch directory removed on drop.
pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    /// A fresh, empty directory under the temp dir, named for `case` and this process.
    pub fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("shadow_scale_{case}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch directory");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A child killed on drop, including on a panic.
pub struct Process {
    pub child: Child,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The built `server`.
pub fn server_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_server"))
}

/// The built `sim_ai`: the server binary's sibling, or a private build of it (module docs).
pub fn sim_ai_binary() -> PathBuf {
    let server = server_binary();
    let sibling = server.with_file_name(format!("sim_ai{}", std::env::consts::EXE_SUFFIX));
    if sibling.is_file() {
        return sibling;
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core_sim sits in the workspace root");
    let target_dir = workspace.join("target").join(FALLBACK_TARGET_DIR);
    let status = Command::new(env!("CARGO"))
        .current_dir(workspace)
        .args(["build", "-p", "sim_ai", "--bin", "sim_ai", "--target-dir"])
        .arg(&target_dir)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "building sim_ai for the test failed");
    let built = target_dir
        .join("debug")
        .join(format!("sim_ai{}", std::env::consts::EXE_SUFFIX));
    assert!(
        built.is_file(),
        "the fallback build produced no sim_ai at {}",
        built.display()
    );
    built
}

/// `text` without its ANSI colour sequences — the server's fmt layer colours its fields, so
/// `faction=1` is not one substring until they are stripped.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_escape = false;
    for ch in text.chars() {
        if in_escape {
            if ch == ANSI_SGR_END {
                in_escape = false;
            }
        } else if ch == ANSI_ESCAPE {
            in_escape = true;
        } else {
            out.push(ch);
        }
    }
    out
}

/// The last [`LOG_TAIL_LINES`] lines of the log at `path`.
pub fn log_tail(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
        }
        Err(err) => format!("(the log at {} could not be read: {err})", path.display()),
    }
}
