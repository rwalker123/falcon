//! ⛔ **A REAL `sim_ai` AGAINST A REAL SERVER, OVER THE REAL SOCKETS.**
//!
//! `sim_ai` is a player process (`docs/plan_ai_opponents.md` §1): it claims a seat, is sent that
//! seat's frames, and sends commands. Nothing in-process can prove that — the crate cannot link
//! `core_sim`, and the seat gate, the greeting and the turn scheduler all live in the server's main
//! loop. So this test drives the **built** `server` and the **built** `sim_ai`, exactly as the
//! launcher would, and observes the world afterwards through a connection of its own.
//!
//! **Why it lives in `core_sim/tests/`.** `CARGO_BIN_EXE_server` is only defined for the package
//! owning that bin, and the `sim_ai` binary is resolved as its **sibling** in the target directory
//! (`common::ai_process`, shared with `ai_bench.rs`, which also owns the fallback build). The
//! server, the world and the seated connection are `common::seat_harness`, shared with
//! `ai_record_import.rs`.
//!
//! **What it asserts, and through whom.** Frames are viewer-scoped and fogged — a rival band the
//! human has never seen is in no frame of seat 0's — so the rival's world is read through seat 1
//! both times: *before*, on a connection released before the AI starts, and *after*, once the AI
//! has exited and released it. The rival's band count moved by the scripted `split_band`, and the
//! tick advanced by the turns the AI submitted `ready` for.

mod common;

use std::fs;

use common::ai_process::{jsonl, log_tail, sim_ai_binary, strip_ansi};
use common::seat_harness::{
    build_world, rival_band_count, run_scripted_sim_ai, run_utility_sim_ai, start_server, Link,
    RIVAL_SEAT, SCRIPT, SPLIT_WORKERS,
};

/// How many resolved turns the AI plays before releasing its seat.
const AI_TURNS: u64 = 3;
/// Turns the utility seat plays: the window is tick 1's, the kit shows on tick 2's frame, and one
/// more turn proves the seat keeps playing outfitted.
const UTILITY_TURNS: u64 = 3;
/// The utility scenario's own port block — the two tests in this file run in parallel.
const UTILITY_PORT_BASE: u16 = 45500;
const UTILITY_CLAIM_ID: u64 = 300;
/// The profile and difficulty the outfit scenario seats: `hard` is argmax, so the run is the
/// rules' and not the seeded draw's.
const UTILITY_PROFILE: &str = "forager";
const UTILITY_DIFFICULTY: &str = "hard";
/// This test's port block; `ai_record_import.rs` uses the next one over.
const TEST_PORT_BASE: u16 = 45300;
/// Claim ids are spaced by the retry count, since a retried claim spends one id per attempt.
const BEFORE_CLAIM_ID: u64 = 100;
const AFTER_CLAIM_ID: u64 = 200;

/// ⛔ **The AI claims its seat, plays its script, submits its turns, and leaves the world changed.**
#[test]
fn a_scripted_sim_ai_plays_a_seat_over_the_real_sockets() {
    let server = start_server("ai_seat_scenario", TEST_PORT_BASE, None);
    let sim_ai = sim_ai_binary();
    build_world(&server);

    // The world BEFORE, through the rival's own seat — then released, so the AI can claim it and
    // is the only occupant, and turns resolve on its `ready` alone.
    let (tick_before, bands_before) = {
        let mut before = Link::open(server.ports.command, &server.log_path);
        let claim = before.claim_seat(BEFORE_CLAIM_ID, RIVAL_SEAT);
        assert!(
            claim.ok,
            "seat {RIVAL_SEAT} could not be claimed: {} — this world must seat a rival",
            claim.error
        );
        let snapshot = before.full_frame_for(server.ports.stream, claim.seat_token);
        (snapshot.header.tick, rival_band_count(&snapshot))
    };
    assert!(
        bands_before > 0,
        "the world seats no rival band, so there is nothing for the AI to command"
    );

    let ai = run_scripted_sim_ai(&sim_ai, &server, RIVAL_SEAT, SCRIPT, AI_TURNS, None);
    assert!(ai.status.success());

    // The world AFTER, through the seat the AI just released.
    let mut rival = Link::open(server.ports.command, &server.log_path);
    let claim = rival.claim_seat(AFTER_CLAIM_ID, RIVAL_SEAT);
    assert!(
        claim.ok,
        "seat {RIVAL_SEAT} could not be re-claimed: {}",
        claim.error
    );
    let after = rival.full_frame_for(server.ports.stream, claim.seat_token);

    assert_eq!(
        rival_band_count(&after),
        bands_before + 1,
        "the scripted split_band ({SPLIT_WORKERS} workers off band 0) did not make a second rival \
         band\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
        log_tail(&ai.log_path),
        log_tail(&server.log_path)
    );
    assert!(
        after.header.tick >= tick_before + AI_TURNS,
        "the tick moved {tick_before} → {} while the AI submitted ready for {AI_TURNS} turns",
        after.header.tick
    );

    // The server's fmt layer colours its fields, so `faction=1` is not one substring until the
    // escape sequences are stripped.
    let server_log =
        strip_ansi(&fs::read_to_string(&server.log_path).expect("the server log reads"));
    assert!(
        server_log
            .lines()
            .any(|line| line.contains("seat.claimed")
                && line.contains(&format!("faction={RIVAL_SEAT}"))),
        "the server never logged the AI's claim of seat {RIVAL_SEAT}\n{}",
        log_tail(&server.log_path)
    );
    assert!(
        !server_log.contains("command.rejected"),
        "the server refused a command during the run\n{}",
        log_tail(&server.log_path)
    );
}

/// **One demand round-trips posted → planned → fulfilled** (`docs/plan_ai_driver.md` §11 row 7):
/// the utility seat outfits its band on the first turn from the specialists' demands, the server
/// refuses nothing, and the band's frame after the first advance carries the kit the decisions
/// log says was sent.
#[test]
fn a_utility_sim_ai_outfits_its_band_on_the_first_turn() {
    let server = start_server("ai_seat_outfit", UTILITY_PORT_BASE, None);
    let sim_ai = sim_ai_binary();
    build_world(&server);
    let log_dir = server.scratch.dir.join("seat_logs");
    let ai = run_utility_sim_ai(
        &sim_ai,
        &server,
        RIVAL_SEAT,
        UTILITY_PROFILE,
        UTILITY_DIFFICULTY,
        UTILITY_TURNS,
        Some(&log_dir),
    );
    assert!(ai.status.success());

    // (1) The server refused nothing.
    let server_log =
        strip_ansi(&fs::read_to_string(&server.log_path).expect("the server log reads"));
    assert!(
        !server_log.contains("command.rejected"),
        "the server refused a command during the run\n{}",
        log_tail(&server.log_path)
    );

    // (3) The decisions log: the loadout decision, and a demand that went the whole way.
    let records = jsonl(&log_dir.join("decisions.jsonl"));
    let outfit = records
        .iter()
        .find(|record| {
            record["kind"] == "decision"
                && record["intent"]
                    .as_str()
                    .is_some_and(|intent| intent.starts_with("orchestrator:outfit:"))
        })
        .unwrap_or_else(|| {
            panic!(
                "no loadout decision\n--- sim_ai log ---\n{}",
                log_tail(&ai.log_path)
            )
        });
    assert_eq!(outfit["outcome"], "accepted");
    let line = outfit["commands_text"][0]
        .as_str()
        .expect("the loadout's command line");
    assert!(
        line.starts_with("set_starting_loadout"),
        "the loadout is in the grammar: {line}"
    );
    let band = outfit["intent"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u64>()
        .expect("the intent's band");
    let states: Vec<String> = records
        .iter()
        .filter(|record| record["kind"] == "demand" && record["band"] == band)
        .map(|record| {
            format!(
                "{}:{}",
                record["resource"].as_str().unwrap_or_default(),
                record["state"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let round_trip = |resource: &str| {
        let position = |state: &str| {
            states
                .iter()
                .position(|s| s == &format!("{resource}:{state}"))
        };
        matches!(
            (position("posted"), position("planned"), position("fulfilled")),
            (Some(posted), Some(planned), Some(fulfilled)) if posted < planned && planned < fulfilled
        )
    };
    assert!(
        states.iter().any(|state| state.ends_with(":fulfilled")),
        "no demand was fulfilled: {states:?}\n--- sim_ai log ---\n{}",
        log_tail(&ai.log_path)
    );
    let fulfilled: Vec<&str> = states
        .iter()
        .filter_map(|state| state.strip_suffix(":fulfilled"))
        .collect();
    assert!(
        fulfilled.iter().any(|resource| round_trip(resource)),
        "no demand went posted → planned → fulfilled in that order: {states:?}"
    );

    // (2) The world after: the band carries what the loadout line granted. The line is
    // `set_starting_loadout <faction> <band> [kit <id> <n>]... [material <id> <units>]...`.
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let granted_kits: Vec<(&str, u32)> = tokens
        .windows(3)
        .filter(|window| window[0] == "kit")
        .map(|window| (window[1], window[2].parse().expect("a kit count")))
        .collect();
    assert!(!granted_kits.is_empty(), "the loadout named no kit: {line}");
    let mut rival = Link::open(server.ports.command, &server.log_path);
    let claim = rival.claim_seat(UTILITY_CLAIM_ID, RIVAL_SEAT);
    assert!(
        claim.ok,
        "seat {RIVAL_SEAT} could not be re-claimed: {}",
        claim.error
    );
    let after = rival.full_frame_for(server.ports.stream, claim.seat_token);
    let cohort = after
        .populations
        .iter()
        .find(|cohort| cohort.band_id == band)
        .expect("the outfitted band is still in the world");
    for (kit_id, count) in granted_kits {
        let kit = after
            .kits
            .iter()
            .find(|kit| kit.id == kit_id)
            .unwrap_or_else(|| panic!("the roster has no kit `{kit_id}`"));
        for item in &kit.item_ids {
            let held: u32 = cohort
                .equipment_batches
                .iter()
                .filter(|batch| &batch.item_id == item)
                .map(|batch| batch.count)
                .sum();
            assert!(
                held >= count,
                "band {band} holds {held} `{item}` for {count} `{kit_id}` granted\n--- sim_ai log ---\n{}",
                log_tail(&ai.log_path)
            );
        }
    }
}
