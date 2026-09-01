//! `cargo xtask hotkey-guard` — the gate on the client's KEYBOARD.
//!
//! Every gameplay key the Godot client reads is declared in one table
//! (`clients/godot_thin_client/src/scripts/KeyboardArbiter.gd`), and
//! `tools/hotkey_guard.tscn` walks that table in both directions: every row against every
//! keyboard owner, and every keyboard read in `src/` back to a row. This command is the wrapper
//! that runs it.
//!
//! **It is a Godot scene rather than a `cargo test`, and the source scan is inside that scene**, for
//! the same reason `decode-guard` is: the assertions have to reach the LIVE registry, the live
//! `InputMap` and a real `MapView`. A Rust-side scan could grep the `.gd` files, but it would have
//! to re-parse the registry out of GDScript to know what to check them against — a second, drifting
//! copy of the roster, which is the defect this whole arc exists to remove.
//!
//! Two steps. The native extension is NOT built: nothing here decodes a snapshot, and the arbiter is
//! pure GDScript. The project is still imported when it never has been, because `MapView.gd` reaches
//! autoloads that only register when the project is loaded.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

pub fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    if let Some(unexpected) = args.first() {
        return Err(format!(
            "hotkey-guard: unknown argument '{unexpected}' (this command takes none)"
        )
        .into());
    }

    let client_dir = Path::new("clients").join("godot_thin_client");
    crate::decode_guard::ensure_project_imported(&client_dir)?;

    let output = Command::new("godot")
        .arg("--headless")
        .arg("--path")
        .arg(&client_dir)
        .arg("res://tools/hotkey_guard.tscn")
        .output()
        .map_err(|err| format!("hotkey-guard: failed to launch `godot` ({err}). Is it on PATH?"))?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    if !output.status.success() {
        return Err(format!(
            "hotkey-guard failed (godot exited with {}) — see the numbered problems above",
            output.status
        )
        .into());
    }
    Ok(())
}
