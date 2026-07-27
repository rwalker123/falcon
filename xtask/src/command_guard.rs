//! `cargo xtask command-guard` — the gate on the **client → server COMMAND** direction.
//!
//! ## The gap this closes
//!
//! [`crate::decode_guard`] proves the client reads what the server sends. Nothing proved the client
//! **sends what the server reads**, and that cost real play time: the sim was changed to resolve a
//! band by its durable `BandId`, the client kept sending ECS `entity` bits, and **both are `u64`**.
//! Nothing failed to compile, nothing failed to parse, nothing failed a test — the server looked up
//! a band that did not exist and no-op'd. Every band-addressed order silently stopped working, and a
//! human found it by playing.
//!
//! A grep could not have caught it. `int(band.get("entity", -1))` is perfectly valid GDScript that
//! simply *meant* something different than the server thought. **The only assertion that catches
//! this class is a value one**: run the client's real emit path, then resolve the number that comes
//! out with the real server parser.
//!
//! ## The two halves
//!
//! 1. `res://tools/command_guard.tscn` drives each band-addressed command through the code a
//!    player's click reaches, formats it with `Main`'s own builders, and writes the emitted lines to
//!    `ui_preview_out/emitted_band_commands.json` along with the fixture's handles.
//! 2. This module parses **every** line with `sim_runtime::command_text::parse_command_line` — the
//!    same function the server runs — and asserts the band handle equals the fixture's `band_id`.
//!
//! **The fixture's `entity` and `band_id` are deliberately different values.** If they agreed this
//! gate would prove nothing: sending the wrong handle would produce the right number. That
//! coincidence is exactly how the defect hid, so this module asserts the emitted handle is *not* the
//! entity as well as *is* the band id — a fixture edit that made them equal would fail here rather
//! than silently defanging the gate.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sim_runtime::commands::CommandPayload;

/// Where the Godot half writes what it emitted.
const EMITTED: &str = "clients/godot_thin_client/ui_preview_out/emitted_band_commands.json";

pub fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let mut build_native = true;
    for arg in &args {
        match arg.as_str() {
            "--no-build" => build_native = false,
            other => {
                return Err(
                    format!("command-guard: unknown flag '{other}' (expected --no-build)").into(),
                )
            }
        }
    }

    if build_native {
        crate::godot_build()?;
    }

    let client_dir = Path::new("clients").join("godot_thin_client");
    crate::decode_guard::ensure_project_imported(&client_dir)?;

    // Removed first so a Godot run that dies before writing cannot leave this gate asserting against
    // a stale file from a previous run — which would pass, and would be the same silent-staleness
    // failure the gate exists to catch.
    let emitted_path = Path::new(EMITTED);
    if emitted_path.exists() {
        std::fs::remove_file(emitted_path)?;
    }

    let output = Command::new("godot")
        .arg("--headless")
        .arg("--path")
        .arg(&client_dir)
        .arg("res://tools/command_guard.tscn")
        .output()
        .map_err(|err| {
            format!("command-guard: failed to launch `godot` ({err}). Is it on PATH?")
        })?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    if !output.status.success() {
        return Err(format!("command-guard failed (godot exited with {})", output.status).into());
    }

    let raw = std::fs::read_to_string(emitted_path).map_err(|err| {
        format!(
            "command-guard: the Godot half exited 0 but wrote no {EMITTED} ({err}). It must emit \
             the commands this gate parses; an exit code alone proves nothing."
        )
    })?;
    let doc: Value = serde_json::from_str(&raw)?;

    let band_id = u64_field(&doc, "band_id")?;
    let band_entity = u64_field(&doc, "band_entity")?;
    let expedition_band_id = u64_field(&doc, "expedition_band_id")?;
    let expedition_entity = u64_field(&doc, "expedition_entity")?;

    // The gate's own precondition. A fixture whose two handles agree cannot distinguish a client
    // sending the right one from a client sending the wrong one.
    if band_id == band_entity || expedition_band_id == expedition_entity {
        return Err(format!(
            "command-guard: the fixture's handles must DIFFER, or this gate proves nothing — \
             band_id={band_id} band_entity={band_entity} expedition_band_id={expedition_band_id} \
             expedition_entity={expedition_entity}"
        )
        .into());
    }

    let commands = doc
        .get("commands")
        .and_then(Value::as_array)
        .ok_or("command-guard: emitted document has no `commands` array")?;
    if commands.is_empty() {
        return Err("command-guard: the Godot half emitted no commands at all".into());
    }

    let mut failures = Vec::new();
    for entry in commands {
        let (label, line) = match entry {
            Value::String(line) => (line.clone(), line.clone()),
            Value::Object(map) => {
                let line = map
                    .get("line")
                    .and_then(Value::as_str)
                    .ok_or("command-guard: a command entry has no `line`")?;
                let label = map
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or(line)
                    .to_string();
                (label, line.to_string())
            }
            other => return Err(format!("command-guard: unexpected command entry {other}").into()),
        };

        let payload = match sim_runtime::command_text::parse_command_line(&line) {
            Ok(payload) => payload,
            Err(err) => {
                failures.push(format!(
                    "{label}: the server parser REJECTED `{line}` — {err:?}"
                ));
                continue;
            }
        };

        let Some(sent) = band_handle(&payload) else {
            // Not every emitted command names a band; those simply have nothing to check here.
            continue;
        };
        let expected = if matches!(payload, CommandPayload::RecallExpedition { .. }) {
            expedition_band_id
        } else {
            band_id
        };
        let wrong_handle = if expected == band_id {
            band_entity
        } else {
            expedition_entity
        };

        if sent == wrong_handle {
            failures.push(format!(
                "{label}: sent the ENTITY ({sent}) where the server resolves a BandId — this is the \
                 exact defect this gate exists for. Expected {expected}. Line: `{line}`"
            ));
        } else if sent != expected {
            failures.push(format!(
                "{label}: sent {sent}, expected the fixture's band handle {expected}. Line: `{line}`"
            ));
        }
    }

    if !failures.is_empty() {
        let mut report = String::from(
            "command-guard FAILED — the client is not sending what the server reads:\n",
        );
        for failure in &failures {
            report.push_str("  - ");
            report.push_str(failure);
            report.push('\n');
        }
        return Err(report.into());
    }

    println!(
        "command-guard: PASS — {} emitted command(s) parsed by the real server parser, every band \
         handle resolved to the fixture's BandId (and none to its entity)",
        commands.len()
    );
    Ok(())
}

/// The band handle a payload names, whatever the variant calls it.
///
/// Every `CommandPayload` variant carrying a band handle is listed. **The gate's coverage is not
/// bounded by this function** — it is bounded by which commands the Godot half actually drives, so a
/// new band-addressed command needs adding *there* to be checked at all. This match only has to know
/// where the handle lives once a command is emitted.
fn band_handle(payload: &CommandPayload) -> Option<u64> {
    match payload {
        CommandPayload::AssignLabor { band_id, .. }
        | CommandPayload::CancelOrder { band_id, .. }
        | CommandPayload::FollowHerd { band_id, .. }
        | CommandPayload::ForageTile { band_id, .. }
        | CommandPayload::HuntFauna { band_id, .. }
        | CommandPayload::HuntGame { band_id, .. }
        | CommandPayload::MoveBand { band_id, .. }
        | CommandPayload::ScoutArea { band_id, .. }
        | CommandPayload::SendExpedition { band_id, .. }
        | CommandPayload::SendHuntExpedition { band_id, .. } => *band_id,
        CommandPayload::RecallExpedition {
            expedition_band_id, ..
        } => Some(*expedition_band_id),
        _ => None,
    }
}

fn u64_field(doc: &Value, key: &str) -> Result<u64, Box<dyn Error>> {
    doc.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("command-guard: emitted document has no numeric `{key}`").into())
}
