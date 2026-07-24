//! `cargo xtask decode-guard` — the one command that runs the client decode gate end to end.
//!
//! Three steps, in this order for a reason:
//!
//! 1. **Regenerate the fixture** ([`crate::decode_fixture`]). The `.bin` is committed so the Godot
//!    harness runs standalone, but regenerating first means the gate can never be measuring a stale
//!    envelope against a current decoder.
//! 2. **Build the native extension** (`godot-build`), because the guard calls the *real*
//!    `SnapshotDecoder`, which lives in it. Skipped with `--no-build` when you have just built it.
//! 3. **Run `tools/decode_guard.tscn` headless**, which decodes the fixture and diffs the resulting
//!    dictionary against `tests/golden/snapshot_dict.json`. Its exit code is this command's.
//!
//! `--write-golden` re-records the golden instead of diffing. Read the diff before you reach for it.

use std::error::Error;
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

    let status = command
        .status()
        .map_err(|err| format!("decode-guard: failed to launch `godot` ({err}). Is it on PATH?"))?;

    if !status.success() {
        return Err(format!("decode-guard failed (godot exited with {status})").into());
    }
    Ok(())
}
