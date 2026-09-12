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

use common::ai_process::{log_tail, sim_ai_binary, strip_ansi};
use common::seat_harness::{
    build_world, rival_band_count, run_scripted_sim_ai, start_server, Link, RIVAL_SEAT, SCRIPT,
    SPLIT_WORKERS,
};

/// How many resolved turns the AI plays before releasing its seat.
const AI_TURNS: u64 = 3;
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
