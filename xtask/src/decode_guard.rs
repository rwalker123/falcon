//! `cargo xtask decode-guard` — the one command that runs the client decode gate end to end.
//!
//! Four steps, in this order for a reason:
//!
//! 1. **Regenerate the fixtures** ([`crate::decode_fixture`]) — the saturated one the golden is
//!    diffed against, the headerless one the malformed-snapshot assertion decodes, and the DELTA
//!    one the merge assertions apply to the baseline. The `.bin`s are gitignored — regenerating
//!    here is what puts them on disk, and it is why the gate can never be measuring a stale
//!    envelope against a current decoder. The golden they are diffed against IS committed: that is
//!    the assertion, and only the input is derivable.
//! 2. **Build the native extension** (`godot-build`), because the guard calls the *real*
//!    `SnapshotDecoder`, which lives in it. Skipped with `--no-build` when you have just built it.
//! 3. **Import the project if it has never been imported** ([`ensure_project_imported`]) — building
//!    the extension is not enough on a fresh checkout or worktree; Godot only *loads* it if the
//!    import cache lists it.
//! 4. **Run `tools/decode_guard.tscn` headless**, which decodes the fixture and diffs the resulting
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
    crate::decode_fixture::write_delta_fixtures()?;

    if build_native {
        crate::godot_build()?;
    }

    let client_dir = Path::new("clients").join("godot_thin_client");
    ensure_project_imported(&client_dir)?;

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
    // PANIC_MARKERS below. Everything is still forwarded, but in two BLOCKS (all of stdout, then all
    // of stderr) rather than interleaved, and that is worth knowing while reading a FAILURE: the
    // guard's own `print` lines go to stdout while the engine's `ERROR:` / panic-report lines go to
    // stderr, so a panic report can appear BELOW a `PASS` line it actually preceded. A headless
    // decode run is short enough that losing live streaming costs nothing.
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

/// Runs Godot's import pass **iff** the project has never been imported.
///
/// **Building the native extension is not enough to make it LOAD.** Godot loads GDExtensions from
/// `.godot/extension_list.cfg`, which the import pass writes — and `.godot/` is a build artifact, so
/// a fresh checkout or (much more often here) a fresh **worktree** has none. The guard then reports
/// `SnapshotDecoder class is not registered — build the native extension first`, which is honest
/// about the symptom and actively misleading about the cause: the extension was built, copied and
/// signed moments earlier by step 2. That cost a real diagnosis loop the first time.
///
/// Keyed on `extension_list.cfg` rather than on `.godot/` itself, because that file is precisely the
/// thing whose absence stops the extension loading — and it is re-created by an import even when the
/// rest of the cache survives, so deleting just it is also the way to test this path.
///
/// The import is **skipped once the file exists**: it takes tens of seconds, this gate is run in a
/// tight edit loop, and a stale-asset import is not this command's business (a changed `.gdextension`
/// does not need re-importing — only the dylib it points at, which step 2 rebuilds).
pub(crate) fn ensure_project_imported(client_dir: &Path) -> Result<(), Box<dyn Error>> {
    let extension_list = client_dir.join(".godot").join("extension_list.cfg");
    if extension_list.exists() {
        return Ok(());
    }

    println!(
        "decode-guard: no {} — running Godot's import pass first, so the native extension loads \
         (once per fresh checkout or worktree; this takes a while).",
        extension_list.display()
    );

    // Output is INHERITED, unlike the guard run below: this is the slow one-off step, and its
    // progress bar is the only sign the command has not wedged.
    let status = Command::new("godot")
        .arg("--headless")
        .arg("--path")
        .arg(client_dir)
        .arg("--import")
        .status()
        .map_err(|err| format!("decode-guard: failed to launch `godot` ({err}). Is it on PATH?"))?;

    // **The import is judged by its OUTCOME, not by its exit status, and that is not laziness.**
    // Godot 4.7 headless `--import` CRASHES on shutdown in this project (signal 11, backtrace inside
    // the engine's own teardown, reported as SIGABRT) *after* writing a complete and perfectly good
    // cache — every subsequent run passes against it. Failing on the status would have made this fix
    // useless on precisely the setup it exists for. So the marker file is the verdict, and the
    // status only colours the message when the cache did not appear.
    if !extension_list.exists() {
        return Err(format!(
            "decode-guard: Godot's import pass did not produce {} (godot exited with {status}), so \
             the native extension will not load. Check that {} is present and parses.",
            extension_list.display(),
            client_dir
                .join("native/shadow_scale_godot.gdextension")
                .display()
        )
        .into());
    }

    if !status.success() {
        println!(
            "decode-guard: godot --import exited with {status}, but it wrote {} — continuing. \
             (Godot 4.7 crashes on shutdown here; the cache it leaves behind is sound.)",
            extension_list.display()
        );
    }
    Ok(())
}

/// The substrings that betray a caught Rust panic in the run's output.
///
/// `"[panic "` is gdext's own hook (``ERROR: [panic src/…rs:711]  called `Option::unwrap()` on a
/// `None` value``) — the form actually observed here, and note it does NOT contain the word
/// "panicked". `"panicked"` covers the std hook's `thread '…' panicked at …`, which is what shows
/// if the panic escapes a `#[func]` boundary or comes from a non-gdext thread. Neither string
/// appears in the guard's own prints or in Godot's normal headless chatter.
const PANIC_MARKERS: [&str; 2] = ["[panic ", "panicked"];
