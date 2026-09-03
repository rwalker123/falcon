//! `cargo xtask doc-guard` — the gate on this repo's DOC LINKS.
//!
//! ## The gap this closes (issue #623)
//!
//! A doc comment here carries the engineering rationale, and `` [`Thing::method`] `` links are how
//! one fact points at another. **A link that resolves to nothing is a pointer to a fact the reader
//! cannot reach** — the same failure as a stale field name, and it shipped silently because
//! `rustdoc::broken_intra_doc_links` is warn-by-default and nothing on the critical path builds
//! docs at all.
//!
//! The measurement is the argument. PR #621's review caught **one** broken link by eye; running
//! the lint over that same branch found **112** across the workspace. A defect class a reviewer
//! hits at roughly 1% is a class that wants a machine.
//!
//! ## Why an xtask rather than a `RUSTDOCFLAGS=` line in CI
//!
//! The gate *is* the flag string plus the two scope arguments, and it has to be identical in CI,
//! in the pre-commit hook, and when a developer runs it by hand to see what they broke. Three
//! copies of an env-var incantation drift, and the drift is invisible: a CI line that lost
//! `--document-private-items` still passes, it just quietly stops checking a third of the repo.
//! One command, one definition — the same reason `decode-guard` and `hotkey-guard` are commands
//! and not shell snippets.
//!
//! ## Why `--document-private-items`
//!
//! Because that is where much of the rationale in this repo lives. `cargo doc` alone documents
//! only the public surface, and on the branch this guard was written against that surface held 77
//! of the 112 dead links — the other **35 were invisible to it**, in private helpers in
//! `forage.rs`, `systems/labor.rs`, `snapshot/capture.rs` and `bin/server.rs`. A gate that cannot
//! see a private doc comment is a gate over the minority of the prose it exists to protect.
//!
//! ## The two failure modes are different
//!
//! A link naming a **renamed or moved** item is a stale pointer, and the fix is the real path. A
//! link naming something that was **never an item** — `#[func]` read out of an attribute, a
//! bracketed word in prose — is markdown that wants backslash-escaping. Inventing a target for
//! the second kind would be strictly worse than the warning it silenced.

use std::error::Error;
use std::io::{self, Write};
use std::process::Command;

/// The lint, denied. Carried as a `RUSTDOCFLAGS` value rather than a crate-level `#![deny]` so one
/// definition covers every crate in the workspace, including the ones added tomorrow.
const DENY_FLAG: &str = "-D rustdoc::broken_intra_doc_links";

pub fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    if let Some(arg) = args.first() {
        return Err(
            format!("doc-guard: unknown argument '{arg}' (this command takes none)").into(),
        );
    }

    // `--no-deps` keeps the check on code this repo owns: a dependency's broken links are not ours
    // to fix, and denying them would make the gate hostage to a crates.io release.
    //
    // `--keep-going` is what makes the failure USABLE. Under `-D`, cargo abandons the build at the
    // first crate that errors, so a run over the original 112 reported `sim_runtime`'s one link and
    // hid the other 111 — four blind rebuilds to see a list the tool already had. It reports every
    // crate instead; the exit status is unchanged.
    let output = Command::new("cargo")
        .args([
            "doc",
            "--workspace",
            "--no-deps",
            "--document-private-items",
            "--keep-going",
        ])
        .env("RUSTDOCFLAGS", DENY_FLAG)
        .output()
        .map_err(|err| format!("doc-guard: failed to launch `cargo doc` ({err})"))?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    if !output.status.success() {
        return Err(concat!(
            "doc-guard failed — each `unresolved link to ...` above is a doc comment pointing at ",
            "something the reader cannot reach. Fix the path if the item was renamed or moved; ",
            "backslash-escape the brackets if it was never an item (an attribute like `#[func]`, ",
            "or a bracketed word in prose)."
        )
        .into());
    }
    Ok(())
}
