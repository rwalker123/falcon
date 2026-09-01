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
//!    same function the server runs — and asserts the band handle equals the fixture's `band_id`
//!    **and that the parsed payload carries the kit the drive composed**.
//!
//! ## The kit tail is asserted for the band handle's own reason
//!
//! Four drives exist specifically to push a **non-default** kit through the real parser, because
//! `Main._kit_token` omits `kit <id>` whenever the selection equals the job default. Asserting only
//! that those lines *parse* proves nothing about the kit: if `_kit_token` regressed to `""` every
//! line would still parse, `EXPECTED_KINDS` would still count, and this gate would report **PASS**
//! while no kit ever left the client — the same silent-value class as the `entity`/`band_id` defect
//! above, one field over. So the Godot half records the kit each line is **expected** to carry
//! (`expected_kit`, `""` meaning "no token, the job default stands") and this module compares it
//! against what the server parser actually recovered.
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

/// The per-entry field a shipment drive states its piles in — `{cargo id: ticks}`, in the sim's own
/// fixed point. **Ticks rather than a decimal**, because the assertion is exactly about the last
/// digits: a JSON float would re-round the pile this gate is comparing against.
const HELD_TICKS_FIELD: &str = "cargo_held_ticks";

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
    // How many lines the Godot half says should carry a real `kit <id>`. **The gate's second
    // precondition**: if none do, every kit assertion below is the vacuous `""` case and a
    // `Main._kit_token` that emitted nothing would pass unnoticed — exactly the hole this closes.
    let mut kit_bearing_lines = 0_usize;
    // …and how many carried a manifest drawn from a FRACTIONAL pile. **The gate's third
    // precondition, and it is the same shape as the two above**: a manifest of whole units survives
    // any rounding at all, so a suite of round piles would assert nothing about the emitted amount
    // and a client that rounded every one of them UP would pass.
    let mut fractional_manifest_lines = 0_usize;
    for entry in commands {
        // **The object form is required, and a bare string is a hard error.** A string entry
        // carries no `expected_kit`, so accepting one would silently opt that line out of the kit
        // assertion — a skip that looks like a pass, which is the failure mode this whole module
        // exists to refuse.
        let Value::Object(map) = entry else {
            return Err(format!(
                "command-guard: a command entry is not an object ({entry}). The Godot half must \
                 emit `{{kind, line, expected_kit}}` — a bare line cannot be checked for its kit."
            )
            .into());
        };
        let line = map
            .get("line")
            .and_then(Value::as_str)
            .ok_or("command-guard: a command entry has no `line`")?
            .to_string();
        let label = map
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or(&line)
            .to_string();
        // Absent (rather than empty) means the two halves have drifted, and the drifted state is the
        // one that silently checks nothing — so it is an error, not a default.
        let expected_kit = map
            .get("expected_kit")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "command-guard: `{label}` carries no `expected_kit`. The Godot half must state \
                     the kit each line should carry (`\"\"` = the job default, no token), or this \
                     gate cannot tell a missing tail from an intended one."
                )
            })?
            .to_string();
        if !expected_kit.is_empty() {
            kit_bearing_lines += 1;
        }

        let payload = match sim_runtime::command_text::parse_command_line(&line) {
            Ok(payload) => payload,
            Err(err) => {
                failures.push(format!(
                    "{label}: the server parser REJECTED `{line}` — {err:?}"
                ));
                continue;
            }
        };

        // **A missing handle is a FAILURE here, never a skip.** Every command the Godot half drives
        // is band-addressed, so there is no benign reason for one to arrive without an id — and the
        // benign-looking outcome is the dangerous one: `select_starting_band` falls back to a
        // default-band picker when `band_id` is `None`, so the command *appears* to work while
        // addressing whichever band the server picked. Skipping that case would make this guard
        // report PASS for exactly the regression it exists to catch.
        let sent = match band_handle(&payload) {
            BandHandle::Named(id) => id,
            // **A SOURCE- OR PLACE-ADDRESSED VERB NAMES NO BAND, AND THAT IS CORRECT** — `build_kit`
            // sets a property of a queue ENTRY, which every band holding that source holds, and
            // `abandon` names a faction and a tile. The handle assertion has nothing to check, so it
            // is skipped; the KIT assertion below is not, and for `build_kit` it is the whole point.
            // **The exemption is keyed on the parsed VARIANT, never on the Godot half's own label**,
            // so a band-addressed command cannot be opted out of the handle check by being
            // relabelled.
            BandHandle::SourceAddressed | BandHandle::PlaceAddressed => {
                if let Some(failure) = kit_failure(&label, &line, &payload, &expected_kit) {
                    failures.push(failure);
                }
                continue;
            }
            BandHandle::Omitted => {
                failures.push(format!(
                    "{label}: emitted a band-addressed command with NO band handle. The server \
                     would fall back to its default-band picker and silently act on some other \
                     band. Line: `{line}`"
                ));
                continue;
            }
            BandHandle::NotBandAddressed => {
                failures.push(format!(
                    "{label}: parsed to a command variant that names no band, but this harness only \
                     drives band-addressed commands — so either the emit path changed or \
                     `band_handle` is missing a variant. Line: `{line}`"
                ));
                continue;
            }
        };
        // **A SPLIT NAMES A BAND, A RECALL NAMES A PARTY**, so only the recall reads the party
        // handle. Fission divides the band where it stands; there is no party in the payload at all.
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

        // **THE KIT, through the same real parser.**
        if let Some(failure) = kit_failure(&label, &line, &payload, &expected_kit) {
            failures.push(failure);
        }

        // **THE MANIFEST, against the piles it was drawn from.** Only a shipment carries one, and
        // the pile sizes come off the drive rather than out of this file — see `manifest_failures`.
        if let CommandPayload::SendTradeExpedition { cargo, .. } = &payload {
            let held = map.get(HELD_TICKS_FIELD).and_then(Value::as_object);
            let Some(held) = held else {
                failures.push(format!(
                    "{label}: carries no `{HELD_TICKS_FIELD}`. A shipment drive must state the \
                     piles it composed from, in the sim's own fixed-point ticks, or the emitted \
                     amounts are compared against nothing. Line: `{line}`"
                ));
                continue;
            };
            if held.values().any(|ticks| {
                ticks
                    .as_i64()
                    .is_some_and(|t| t % sim_runtime::FIXED_POINT_SCALE != 0)
            }) {
                fractional_manifest_lines += 1;
            }
            failures.extend(manifest_failures(&label, &line, cargo, held));
        }
    }

    // The gate's manifest precondition — see `fractional_manifest_lines`.
    if fractional_manifest_lines == 0 {
        return Err(
            "command-guard: not one emitted shipment was composed from a FRACTIONAL pile, so the \
             manifest assertions are vacuous — a whole-unit amount survives any rounding, which is \
             precisely what the emitted amount must not rely on. The Godot half's shipment drive \
             must load a pile the fixed point can express and a tenth cannot."
                .into(),
        );
    }

    // The gate's kit precondition, and the twin of the differing-handles check above: with every
    // line expecting `""`, each assertion above is satisfied by a client that emits no kit ever.
    if kit_bearing_lines == 0 {
        return Err(
            "command-guard: not one emitted line was composed with a NON-DEFAULT kit, so \
                    the kit assertions are all vacuous and this gate proves nothing about the \
                    `kit <id>` tail. The Godot half's drives must compose a kit the job default is \
                    not."
                .into(),
        );
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
         handle resolved to the fixture's BandId (and none to its entity), every `kit <id>` tail \
         resolved to the kit the drive composed ({kit_bearing_lines} non-default), and every cargo \
         amount landed inside the pile it was drawn from ({fractional_manifest_lines} fractional)",
        commands.len()
    );
    Ok(())
}

/// What a parsed payload says about the band it addresses.
///
/// **Three outcomes, not two.** Collapsing `Omitted` and `NotBandAddressed` into a single `None`
/// made the caller `continue`, which turned "the client sent a band command with no handle" — the
/// exact regression this gate exists to catch — into a silent pass.
enum BandHandle {
    /// The command names a band.
    Named(u64),
    /// A band-addressed variant whose optional handle is absent. The server would fall back to its
    /// default-band picker, so the command works and addresses the wrong band.
    Omitted,
    /// A variant that names no band at all.
    NotBandAddressed,
    /// A variant that addresses a SOURCE rather than a band, deliberately — `build_kit`, whose
    /// subject is one queue entry and which therefore reaches every band holding that source. It is
    /// its own outcome rather than [`Self::NotBandAddressed`] so that *"this verb has no band"* stays
    /// a stated fact about one variant instead of a hole any un-listed command falls through.
    SourceAddressed,
    /// A variant that addresses a FACTION AND A PLACE and names no band — `abandon`, which drops
    /// every band-of-that-faction's holding on the tile, a forage assignment there included. Its own
    /// outcome for the same reason [`Self::SourceAddressed`] is: the exemption has to be a stated
    /// fact about one variant, keyed on the PARSED variant rather than on the Godot half's label, so
    /// a band-addressed command cannot be opted out of the handle check by being relabelled.
    PlaceAddressed,
}

/// The band handle a payload names, whatever the variant calls it.
///
/// Every `CommandPayload` variant carrying a band handle is listed. **The gate's coverage is not
/// bounded by this function** — it is bounded by which commands the Godot half actually drives, so a
/// new band-addressed command needs adding *there* to be checked at all. This match only has to know
/// where the handle lives once a command is emitted.
fn band_handle(payload: &CommandPayload) -> BandHandle {
    let optional = match payload {
        CommandPayload::AssignLabor { band_id, .. }
        | CommandPayload::CancelOrder { band_id, .. }
        | CommandPayload::FollowHerd { band_id, .. }
        | CommandPayload::ForageTile { band_id, .. }
        | CommandPayload::HuntFauna { band_id, .. }
        | CommandPayload::HuntGame { band_id, .. }
        | CommandPayload::MoveBand { band_id, .. }
        | CommandPayload::ScoutArea { band_id, .. }
        | CommandPayload::SendExpedition { band_id, .. }
        | CommandPayload::SendDenialRaid { band_id, .. }
        | CommandPayload::SendHuntExpedition { band_id, .. }
        | CommandPayload::SendTradeExpedition { band_id, .. }
        | CommandPayload::SplitBand { band_id, .. } => *band_id,
        // ⛔ **THE ROUTE BRANCH'S TWO TILE VERBS NAME A BAND, and no other tile verb does** (arc
        // #532). A patch's keeper is whoever is already foraging it; a road has no work row at all,
        // so the band that will KEEP the tile has to be a token — and the handle is REQUIRED, since
        // a road with nobody on the hook is not a road the sim will accept.
        CommandPayload::Grade { band_id, .. } | CommandPayload::Pave { band_id, .. } => {
            Some(*band_id)
        }
        CommandPayload::RecallExpedition {
            expedition_band_id, ..
        } => Some(*expedition_band_id),
        // **THE QUEUE BELONGS TO A BAND, so its reorder names one** — and its handle is REQUIRED
        // rather than optional, which is why it is wrapped here (`docs/plan_standing_upkeep.md`
        // §4.7b ③).
        CommandPayload::BuildOrder { band_id, .. } => Some(*band_id),
        // **THE WORKED ROW'S RANK BELONGS TO A BAND TOO** — the shedding walk it feeds partitions
        // that band's own rows and the pen-feed split serves that band's own stores, so its handle is
        // REQUIRED exactly as the queue reorder's is (`docs/plan_standing_upkeep.md` §4.9 item 9b).
        CommandPayload::WorkPriority { band_id, .. } => Some(*band_id),
        // …and the BENCH's rank names a band too, and requires it: a bench belongs to exactly one
        // band, so there is no faction-wide reading of this verb to fall back on.
        CommandPayload::BenchPriority { band_id, .. } => Some(*band_id),
        // …while the per-entry kit names a SOURCE and no band at all (§4.7a ②).
        CommandPayload::BuildKit { .. } => return BandHandle::SourceAddressed,
        // ⛔ **AND `abandon` NAMES A PLACE** (arc #532). The roadwork roster's `✕` is its only
        // emitter and it deliberately carries no band token: the sim drops every holding this
        // FACTION has on the tile, which is why the roster's own tooltip warns that a forage
        // assignment there goes down with the road.
        CommandPayload::Abandon { .. } => return BandHandle::PlaceAddressed,
        _ => return BandHandle::NotBandAddressed,
    };
    match optional {
        Some(id) => BandHandle::Named(id),
        None => BandHandle::Omitted,
    }
}

/// What a parsed payload says about the kit it names — the kit half of [`BandHandle`], with the same
/// three-outcome shape and for the same reason.
///
/// **[`Self::Omitted`] and [`Self::NotKitBearing`] must stay apart.** Collapsing them into one
/// "no kit" answer would make *"the client dropped the tail off a kit-bearing command"* — the
/// regression this is here to catch — indistinguishable from *"this command has no kit axis"*.
enum KitToken {
    /// The command names a kit.
    Named(String),
    /// A kit-bearing variant whose optional tail is absent. To the server that means **the job's
    /// default**, which is exactly what a dropped selection also looks like.
    Omitted,
    /// A variant with no kit axis at all (`move_band`, `cancel_order`, `recall_expedition`, and the
    /// scouting `send_expedition`, which is not a kit job).
    NotKitBearing,
}

/// The kit a payload names, whatever the variant calls it. Every `CommandPayload` variant carrying
/// one is listed; as with [`band_handle`], coverage is bounded by which commands the Godot half
/// actually drives.
fn kit_token(payload: &CommandPayload) -> KitToken {
    let optional = match payload {
        CommandPayload::AssignLabor { kit_id, .. }
        | CommandPayload::BuildKit { kit_id, .. }
        | CommandPayload::SendDenialRaid { kit_id, .. }
        | CommandPayload::SendHuntExpedition { kit_id, .. }
        | CommandPayload::SendTradeExpedition { kit_id, .. } => kit_id.clone(),
        _ => return KitToken::NotKitBearing,
    };
    match optional {
        Some(id) => KitToken::Named(id),
        None => KitToken::Omitted,
    }
}

/// Does this parsed line disagree with the kit the drive composed? `None` = it agrees.
///
/// `expected_kit` is the drive's own selection; `""` means the drive named the job default (or a
/// command with no kit axis), which `Main._kit_token` renders as no token and the server reads as the
/// default. Pulled out of the loop so the comparison is reachable from a unit test — the regression
/// it guards is a *client* change, and a check that can only be exercised by launching Godot is one
/// nobody runs while editing it.
fn kit_failure(
    label: &str,
    line: &str,
    payload: &CommandPayload,
    expected_kit: &str,
) -> Option<String> {
    match (kit_token(payload), expected_kit) {
        (KitToken::Named(sent_kit), want) if sent_kit == want => None,
        (KitToken::Omitted | KitToken::NotKitBearing, "") => None,
        (KitToken::Omitted, want) => Some(format!(
            "{label}: the drive composed kit `{want}`, but the emitted line carries NO `kit` token \
             at all — the server would silently run the job default. This is the regression the kit \
             assertion exists for. Line: `{line}`"
        )),
        (KitToken::NotKitBearing, want) => Some(format!(
            "{label}: the drive composed kit `{want}`, but the line parsed to a command variant that \
             carries no kit — either the emit path changed or `kit_token` is missing a variant. \
             Line: `{line}`"
        )),
        (KitToken::Named(sent_kit), want) => Some(format!(
            "{label}: sent kit `{sent_kit}`, expected `{want}`{}. Line: `{line}`",
            if want.is_empty() {
                " (no token at all — the drive named the job default)"
            } else {
                ""
            }
        )),
    }
}

/// The amount an emitted decimal is worth **once the server has finished with it** — the parse into
/// `f32` has already happened (that is `amount`), and this is the `Scalar` quantisation that follows
/// it, mirroring `core_sim::Scalar::from_f32`. **The multiply is deliberately done in `f32`**,
/// exactly as the sim does it: at 137 units an `f32` product of `x × 10^6` steps in 16s, and doing
/// this arithmetic in `f64` here would make the gate agree with a client the server refuses.
fn scalar_ticks(amount: f32) -> i64 {
    (amount * sim_runtime::FIXED_POINT_SCALE as f32).round() as i64
}

/// Does this shipment name more of anything than the band was holding? One string per line that
/// does; empty means every amount landed inside its pile.
///
/// **THE COMPARISON IS THE SERVER'S OWN, AND IT IS STRICT.** `resolve_shipment` refuses on
/// `held < amount` after quantising the parsed `f32` to a `Scalar`, so an amount rounded up by a
/// single tick is a refused shipment — and the compose sheet's `+` clamps a press to the pile, which
/// means the documented one-press way to load a fractional pile puts the EXACT held amount on the
/// row. A formatter that rounds (`%.1f` did) turns that press into *"the band holds 21.05
/// provisions, not 21.10"*, and nothing else in this repo would have caught it: the emitted line
/// parses perfectly, names the right band and carries the right kit.
fn manifest_failures(
    label: &str,
    line: &str,
    cargo: &[sim_runtime::TradeCargoItem],
    held: &serde_json::Map<String, Value>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for item in cargo {
        let Some(held_ticks) = held.get(&item.id).and_then(Value::as_i64) else {
            failures.push(format!(
                "{label}: names `{}` in the manifest, which the drive never said the band holds — \
                 so the emitted amount is compared against nothing. Line: `{line}`",
                item.id
            ));
            continue;
        };
        let sent_ticks = scalar_ticks(item.amount);
        if sent_ticks > held_ticks {
            failures.push(format!(
                "{label}: the manifest names {} of `{}` — {sent_ticks} ticks once the server has \
                 parsed and quantised it — but the band holds {held_ticks}. `resolve_shipment` \
                 compares strictly and REFUSES this shipment; the emitted amount must be floored, \
                 never rounded. Line: `{line}`",
                item.amount, item.id
            ));
        }
    }
    failures
}

fn u64_field(doc: &Value, key: &str) -> Result<u64, Box<dyn Error>> {
    doc.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("command-guard: emitted document has no numeric `{key}`").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::command_text::parse_command_line;

    /// The kit the `command_guard.tscn` drives compose — a real roster id that is **not** any job's
    /// default, which is what makes `Main._kit_token` emit a tail at all.
    const COMPOSED_KIT: &str = "none";

    fn parse(line: &str) -> CommandPayload {
        parse_command_line(line).expect("the fixture line must parse with the real server parser")
    }

    /// **The regression this gate was blind to**: `Main._kit_token` answering `""` for every
    /// selection. The line still parses, still names the right band and still counts toward
    /// `EXPECTED_KINDS` — only the kit is gone, on all three kit-bearing grammars.
    #[test]
    fn a_dropped_kit_tail_is_a_failure_on_every_kit_bearing_grammar() {
        for untailed in [
            "assign_labor 0 71204 hunt game_deer_07 0.5 2",
            "assign_labor 0 71204 forage 44 23 0.5 2",
            "send_hunt_expedition 0 71204 1 game_boar_04 0.5",
            "send_denial_raid 0 71204 1 game_boar_04",
        ] {
            let failure = kit_failure("drive", untailed, &parse(untailed), COMPOSED_KIT)
                .unwrap_or_else(|| {
                    panic!(
                        "a line with no `kit` token must FAIL against a composed kit: {untailed}"
                    )
                });
            assert!(
                failure.contains("NO `kit` token"),
                "the failure must name what went missing, not merely differ: {failure}"
            );
        }
    }

    /// The liveness half — the tailed twins must PASS, or the assertion above is satisfied by a gate
    /// that rejects everything.
    #[test]
    fn the_tailed_twins_carry_the_composed_kit_through_the_real_parser() {
        for tailed in [
            "assign_labor 0 71204 hunt game_deer_07 0.5 2 kit none",
            "assign_labor 0 71204 forage 44 23 0.5 2 kit none",
            "send_hunt_expedition 0 71204 1 game_boar_04 0.5 kit none",
            "send_denial_raid 0 71204 1 game_boar_04 kit none",
        ] {
            assert!(
                kit_failure("drive", tailed, &parse(tailed), COMPOSED_KIT).is_none(),
                "a line carrying the composed kit must pass: {tailed}"
            );
        }
    }

    /// The two `""` readings, which are the same answer from different causes and must stay apart in
    /// the failure text: a kit-bearing command that named the default, and a command with no kit axis
    /// at all. Both agree with `""`; both DISAGREE with a composed kit, and for different reasons.
    #[test]
    fn an_untailed_line_agrees_with_no_expectation_whichever_kind_it_is() {
        let untailed_assign = "assign_labor 0 71204 hunt game_deer_07 0.5 2";
        let no_kit_axis = "move_band 0 71204 44 23";
        assert!(kit_failure("drive", untailed_assign, &parse(untailed_assign), "").is_none());
        assert!(kit_failure("drive", no_kit_axis, &parse(no_kit_axis), "").is_none());

        let axis_failure = kit_failure("drive", no_kit_axis, &parse(no_kit_axis), COMPOSED_KIT)
            .expect("a kitless variant cannot satisfy a composed kit");
        assert!(
            axis_failure.contains("carries no kit"),
            "a kitless VARIANT must not report as a dropped tail — the remedies differ: {axis_failure}"
        );
    }

    /// The band's larder in the fixture, in ticks — `21.050001`, and it is adversarial on PURPOSE.
    /// A tenth rounds it UP to `21.1`; flooring it onto the fixed-point grid alone still emits
    /// `21.050001`, which the server's `f32` parse-then-quantise lands one tick ABOVE the pile. Only
    /// an amount that also backs off the 32-bit wire's own rounding survives.
    const HELD_FOOD_TICKS: i64 = 21_050_001;

    fn held_food() -> serde_json::Map<String, Value> {
        let mut held = serde_json::Map::new();
        held.insert(
            sim_runtime::commands::FOOD_CARGO_KEY.to_string(),
            Value::from(HELD_FOOD_TICKS),
        );
        held
    }

    fn manifest_of(line: &str) -> Vec<sim_runtime::TradeCargoItem> {
        match parse(line) {
            CommandPayload::SendTradeExpedition { cargo, .. } => cargo,
            other => panic!("the fixture line must parse as a shipment, got {other:?}"),
        }
    }

    /// **The defect this half of the gate exists for**: a manifest spelled with a ROUNDED amount.
    /// Both spellings below parse, name the right band and carry no kit — every other assertion in
    /// this module is green on them — and both name more provisions than the band holds, so the
    /// server refuses the shipment the compose sheet's own `+` composed.
    #[test]
    fn an_amount_rounded_above_the_pile_is_a_failure() {
        for over in [
            "send_trade_expedition 0 71204 2 71301 food 21.1",
            "send_trade_expedition 0 71204 2 71301 food 21.050001",
        ] {
            let failures = manifest_failures("shipment", over, &manifest_of(over), &held_food());
            assert_eq!(
                failures.len(),
                1,
                "an amount above the pile must be reported: {over}"
            );
            assert!(
                failures[0].contains("the band holds"),
                "the failure must state the pile it exceeded: {}",
                failures[0]
            );
        }
    }

    /// The liveness half — the FLOORED spelling `Main.cargo_wire_amount` emits must pass, or the
    /// assertion above is satisfied by a gate that rejects every shipment.
    #[test]
    fn the_floored_amount_lands_inside_the_pile() {
        let floored = "send_trade_expedition 0 71204 2 71301 food 21.049995";
        assert!(
            manifest_failures("shipment", floored, &manifest_of(floored), &held_food()).is_empty(),
            "the floored amount must survive the server's parse-and-quantise"
        );
    }

    /// A tail that survives but names the **wrong** kit — the mis-wired-picker case, distinct from
    /// the dropped one and reported as such.
    #[test]
    fn a_line_naming_a_different_kit_is_a_failure_that_names_both() {
        let line = "assign_labor 0 71204 hunt game_deer_07 0.5 2 kit big_game";
        let failure = kit_failure("drive", line, &parse(line), COMPOSED_KIT)
            .expect("a kit that is not the composed one must fail");
        assert!(
            failure.contains("big_game") && failure.contains(COMPOSED_KIT),
            "the failure must name what was sent AND what was wanted: {failure}"
        );
    }
}
