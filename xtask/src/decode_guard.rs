//! `cargo xtask decode-guard` — the one command that runs the client decode gate end to end.
//!
//! Three steps, in this order for a reason:
//!
//! 1. **Regenerate the fixtures** ([`crate::decode_fixture`]) — the saturated one the golden is
//!    diffed against, and the headerless one the malformed-snapshot assertion decodes. Both `.bin`s
//!    are committed so the Godot harness runs standalone, but regenerating first means the gate can
//!    never be measuring a stale envelope against a current decoder.
//! 2. **Build the native extension** (`godot-build`), because the guard calls the *real*
//!    `SnapshotDecoder`, which lives in it. Skipped with `--no-build` when you have just built it.
//! 3. **Run `tools/decode_guard.tscn` headless**, which decodes the fixture and diffs the resulting
//!    dictionary against `tests/golden/snapshot_dict.json`. Its exit code is this command's.
//!
//! `--write-golden` re-records the golden instead of diffing. Read the diff before you reach for it.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

pub fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let mut write_golden = false;
    let mut build_native = true;
    for arg in &args {
        match arg.as_str() {
            "--write-golden" => write_golden = true,
            "--no-build" => build_native = false,
            other => {
                return Err(format!(
                    "decode-guard: unknown flag '{other}' (expected --write-golden or --no-build)"
                )
                .into())
            }
        }
    }

    crate::decode_fixture::write_fixture()?;
    crate::decode_fixture::write_headerless_fixture()?;

    if build_native {
        crate::godot_build()?;
    }

    let client_dir = Path::new("clients").join("godot_thin_client");
    let mut command = Command::new("godot");
    command
        .arg("--headless")
        .arg("--path")
        .arg(&client_dir)
        .arg("res://tools/decode_guard.tscn");
    if write_golden {
        command.arg("--").arg("--write-golden");
    }

    // Captured rather than inherited so the run can be searched for a Rust panic — see
    // PANIC_MARKER below. Everything is forwarded verbatim afterwards, so the terminal output is
    // the same; a headless decode run is short enough that losing live streaming costs nothing.
    let output = command
        .output()
        .map_err(|err| format!("decode-guard: failed to launch `godot` ({err}). Is it on PATH?"))?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    if !output.status.success() {
        return Err(format!("decode-guard failed (godot exited with {})", output.status).into());
    }

    // **A caught panic is a FAILURE, even though Godot exits 0 and the SCENE reports PASS.** gdext
    // wraps every `#[func]` in a panic guard: a Rust panic inside the decoder does not unwind into
    // the engine, it is logged and the call returns the method's DEFAULT — for `decode_snapshot`, an
    // empty `Dictionary`, which is exactly what a deliberately-dropped frame returns. So the guard
    // scene structurally cannot tell the two apart (it says so at its headerless assertion), and
    // this transcript grep is the only place a panic is visible. Measured, not assumed: restoring
    // the old `header().unwrap()` produced a green run whose only trace was these log lines.
    let mut transcript = String::from_utf8_lossy(&output.stdout).into_owned();
    transcript.push_str(&String::from_utf8_lossy(&output.stderr));
    if let Some(marker) = PANIC_MARKERS.iter().find(|m| transcript.contains(**m)) {
        return Err(format!(
            "decode-guard failed: the engine reported a Rust PANIC during the run (matched {marker:?} \
             — see the output above). gdext catches the panic and returns a default value, so Godot \
             exits 0 and the scene can even print PASS; in the running client the same panic takes \
             the frame down. A malformed snapshot must DEGRADE, never panic."
        )
        .into());
    }
    Ok(())
}

/// The substrings that betray a caught Rust panic in the run's output.
///
/// `"[panic "` is gdext's own hook (`ERROR: [panic src/…rs:711]  called \`Option::unwrap()\` on a
/// \`None\` value`) — the form actually observed here, and note it does NOT contain the word
/// "panicked". `"panicked"` covers the std hook's `thread '…' panicked at …`, which is what shows
/// if the panic escapes a `#[func]` boundary or comes from a non-gdext thread. Neither string
/// appears in the guard's own prints or in Godot's normal headless chatter.
const PANIC_MARKERS: [&str; 2] = ["[panic ", "panicked"];
