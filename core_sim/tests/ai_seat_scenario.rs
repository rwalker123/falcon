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

use std::collections::BTreeMap;
use std::fs;

use common::ai_process::{jsonl, log_tail, sim_ai_binary, strip_ansi};
use common::seat_harness::{
    build_world, rival_band_count, run_scripted_sim_ai, run_utility_sim_ai, start_server, Link,
    RIVAL_SEAT, SCRIPT, SPLIT_WORKERS,
};
use core_sim::starting_loadout::clamped_kit_defaults;

/// How many resolved turns the AI plays before releasing its seat.
const AI_TURNS: u64 = 3;
/// Turns the utility seat plays: the window is tick 1's, the kit shows on tick 2's frame, and one
/// more turn proves the seat keeps playing outfitted.
const UTILITY_TURNS: u64 = 3;
/// The utility scenario's own port block — the two tests in this file run in parallel.
const UTILITY_PORT_BASE: u16 = 45500;
const UTILITY_CLAIM_ID: u64 = 300;
/// The utility scenario's read of the world before the AI plays: the parent's grant window.
const UTILITY_BEFORE_CLAIM_ID: u64 = 400;
/// The specialist and intent a `split_band` is logged under, and the loadout's.
const SPLIT_INTENT_PREFIX: &str = "food:split:";
const OUTFIT_INTENT_PREFIX: &str = "orchestrator:outfit:";
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
/// refuses nothing, and the frame after the first advance carries the kit the decisions log says
/// was sent — **summed over the band and every band it split off, less what the sim's own
/// partition floors away**. On this harness world *split to feed* fires on the grant turn, and
/// a split of a still-granting parent partitions the grant (`starting-loadout.md` → "What a
/// SPLIT gives the splinter"): the splinter's window gets `min(asked, the parent's remaining
/// kit budget)` slots, the parent is re-fitted to what is left by `clamp_allocation`'s
/// proportional-floored rule, and what the clamp shed is fitted to the splinter's slots by the
/// same rule — two floors per split, each dropping a fraction of a unit per kit row: on this
/// world one split of four against a budget of seventeen leaves the family 14 of the 15
/// baskets and 1 of the 2 spears on the line. The expectation is therefore the sim's
/// own rule replayed: the parent's kit budget read off the world **before** the AI plays, the
/// grant-turn splits in the order the log sent them, `clamped_kit_defaults` (the one
/// implementation of the rule, public) applied as `fission::rebalance_partitioned_grant`
/// applies it, and a splinter's own loadout line standing in for its share where it sent one.
/// With no split the replay is the identity and the family — the parent alone — must hold the
/// whole line.
#[test]
fn a_utility_sim_ai_outfits_its_band_on_the_first_turn() {
    let server = start_server("ai_seat_outfit", UTILITY_PORT_BASE, None);
    let sim_ai = sim_ai_binary();
    build_world(&server);

    // The grant BEFORE the AI plays, through the rival's own seat — then released: every
    // resident rival band's open window and the kit slots it may mint against.
    let kit_budgets: BTreeMap<u64, u32> = {
        let mut before = Link::open(server.ports.command, &server.log_path);
        let claim = before.claim_seat(UTILITY_BEFORE_CLAIM_ID, RIVAL_SEAT);
        assert!(
            claim.ok,
            "seat {RIVAL_SEAT} could not be claimed: {} — this world must seat a rival",
            claim.error
        );
        let snapshot = before.full_frame_for(server.ports.stream, claim.seat_token);
        snapshot
            .populations
            .iter()
            .filter(|cohort| cohort.faction == RIVAL_SEAT && !cohort.is_expedition)
            .filter_map(|cohort| {
                let window = cohort
                    .loadout_window
                    .as_ref()
                    .filter(|window| window.open)?;
                Some((cohort.band_id, window.kit_budget))
            })
            .collect()
    };
    assert!(
        !kit_budgets.is_empty(),
        "no rival band opens the world with a grant window"
    );

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

    // (2) The world after: the family carries what the loadout line granted, less the sim's own
    // flooring on each grant-turn split. The line is
    // `set_starting_loadout <faction> <band> [kit <id> <n>]... [material <id> <units>]...`.
    let granted = kit_rows(line);
    assert!(!granted.is_empty(), "the loadout named no kit: {line}");
    let grant_tick = outfit["tick"].as_u64().expect("the loadout's tick");
    // The grant-turn splits of the outfitted band, in the order they were sent: `split_band
    // <faction> <band> <workers>`. A later turn's split is a take on the parent, which moves
    // goods inside the family and changes nothing the family holds.
    let splits: Vec<u32> = records
        .iter()
        .filter(|record| {
            record["kind"] == "decision"
                && record["outcome"] == "accepted"
                && record["tick"].as_u64() == Some(grant_tick)
                && record["intent"].as_str() == Some(&format!("{SPLIT_INTENT_PREFIX}{band}"))
        })
        .map(|record| {
            let command = record["commands_text"][0]
                .as_str()
                .expect("the split's command line");
            command
                .rsplit(' ')
                .next()
                .and_then(|workers| workers.parse().ok())
                .unwrap_or_else(|| panic!("not a split_band line: {command}"))
        })
        .collect();
    // A splinter's own loadout line, by band: where it sent one, that is its allocation.
    let own_lines: BTreeMap<u64, BTreeMap<String, u32>> = records
        .iter()
        .filter(|record| record["kind"] == "decision" && record["outcome"] == "accepted")
        .filter_map(|record| {
            let child = record["intent"]
                .as_str()?
                .strip_prefix(OUTFIT_INTENT_PREFIX)?
                .parse::<u64>()
                .ok()?;
            let line = record["commands_text"][0].as_str()?;
            (child != band).then(|| (child, kit_rows(line)))
        })
        .collect();

    let mut rival = Link::open(server.ports.command, &server.log_path);
    let claim = rival.claim_seat(UTILITY_CLAIM_ID, RIVAL_SEAT);
    assert!(
        claim.ok,
        "seat {RIVAL_SEAT} could not be re-claimed: {}",
        claim.error
    );
    let after = rival.full_frame_for(server.ports.stream, claim.seat_token);
    assert!(
        after
            .populations
            .iter()
            .any(|cohort| cohort.band_id == band),
        "the outfitted band is still in the world"
    );
    // The family: the outfitted band and every resident band of its faction — a splinter of
    // the grant turn stands on the parent's tile carrying its share of the grant. Band ids are
    // minted in order, so the children sorted by id pair with the splits in the order sent.
    let family: Vec<_> = after
        .populations
        .iter()
        .filter(|cohort| cohort.faction == RIVAL_SEAT && !cohort.is_expedition)
        .collect();
    let family_ids: Vec<u64> = family.iter().map(|cohort| cohort.band_id).collect();
    let mut children: Vec<u64> = family_ids
        .iter()
        .copied()
        .filter(|id| *id != band)
        .collect();
    children.sort_unstable();
    assert!(
        children.len() >= splits.len(),
        "{} grant-turn splits but the family is {family_ids:?}",
        splits.len()
    );

    // The sim's rule replayed (`fission::open_splinter_loadout_window` and
    // `rebalance_partitioned_grant`): per split, the splinter takes `min(asked, remaining)` kit
    // slots off the parent's budget and MINTS ITS OWN DEFAULT against them
    // (`starting_loadout::outfit_band_with_defaults`), while the parent's own allocation is
    // re-fitted to what the partition left it.
    //
    // ⛔ **THE PARENT'S SHED ROWS ARE NOT HANDED DOWN.** That arm of `rebalance_partitioned_grant`
    // was deleted when every band began holding its default outfit from the moment it exists —
    // handing the leftovers down would fight the default the splinter has already applied. So the
    // family's per-kit totals are NOT the granted line re-cut: each side is minted from its own
    // budget, the two budgets still partition the grant exactly, and each side's proportional floor
    // leaves its own remainder unspent. `split_loadout.rs`
    // → `the_splinters_outfit_is_its_own_default_rather_than_the_parents_leftovers` pins the rule
    // in process; this replays it against a real server.
    let parent_budget = *kit_budgets
        .get(&band)
        .unwrap_or_else(|| panic!("band {band} had no open grant window before the AI played"));
    let expected = family_kits(&granted, parent_budget, &splits, &children, &own_lines);
    if splits.is_empty() {
        assert_eq!(
            expected, granted,
            "with no split the family holds the whole line"
        );
    }

    for (kit_id, count) in &expected {
        let kit = after
            .kits
            .iter()
            .find(|kit| &kit.id == kit_id)
            .unwrap_or_else(|| panic!("the roster has no kit `{kit_id}`"));
        for item in &kit.item_ids {
            let held: u32 = family
                .iter()
                .flat_map(|cohort| cohort.equipment_batches.iter())
                .filter(|batch| &batch.item_id == item)
                .map(|batch| batch.count)
                .sum();
            assert!(
                held >= *count,
                "bands {family_ids:?} hold {held} `{item}` for {count} `{kit_id}` expected of the \
                 {} `{kit_id}` granted to band {band} over {} grant-turn split(s) {splits:?} \
                 against a kit budget of {parent_budget}\n--- sim_ai log ---\n{}",
                granted.get(kit_id).copied().unwrap_or(0),
                splits.len(),
                log_tail(&ai.log_path)
            );
        }
    }
}

/// **What the family holds after its grant-turn splits**, per kit — the sim's own rule replayed.
///
/// `granted` is the accepted line the parent was outfitted with, `parent_budget` its kit budget
/// before any split, `splits` the workers each grant-turn split asked for in the order sent, and
/// `children` the splinters' band ids in the same order. A splinter that sent a loadout order of
/// its own holds that instead of its default, which is what `own_lines` carries.
fn family_kits(
    granted: &BTreeMap<String, u32>,
    parent_budget: u32,
    splits: &[u32],
    children: &[u64],
    own_lines: &BTreeMap<u64, BTreeMap<String, u32>>,
) -> BTreeMap<String, u32> {
    let defaults = opening_kit_defaults();
    let mut remaining = parent_budget;
    let mut parent_holds = granted.clone();
    let mut expected: BTreeMap<String, u32> = BTreeMap::new();
    for (child, asked) in children.iter().zip(splits) {
        let slots = (*asked).min(remaining);
        remaining -= slots;
        parent_holds = clamped_kit_defaults(&parent_holds, remaining)
            .0
            .into_iter()
            .collect();
        let childs_default: BTreeMap<String, u32> = clamped_kit_defaults(&defaults, slots)
            .0
            .into_iter()
            .collect();
        for (kit_id, count) in own_lines.get(child).unwrap_or(&childs_default) {
            *expected.entry(kit_id.clone()).or_default() += count;
        }
    }
    for (kit_id, count) in &parent_holds {
        *expected.entry(kit_id.clone()).or_default() += count;
    }
    expected
}

/// ⛔ **THE REPORTED CASE, PINNED WITHOUT A SERVER.**
///
/// Whether the utility seat splits at all on its grant turn is the AI's decision, and it does not
/// on every host — the CI runner split and the developer machine did not, so the arithmetic above
/// went unexercised locally while it failed in CI. This replays the exact reported numbers: a band
/// granted 15 `gathering` and 2 `big_game` against a budget of 17, splitting once for 4 workers.
///
/// Under the retired rule the parent's shed rows were re-fitted to the splinter's slots, which put
/// **14** `gathering` in the family. The splinter mints its **own default** now, so the two sides
/// are minted from two budgets and each floors its own remainder away: 12.
#[test]
fn a_grant_turn_split_leaves_the_family_holding_two_minted_defaults() {
    let granted = BTreeMap::from([("big_game".to_owned(), 2), ("gathering".to_owned(), 15)]);
    let expected = family_kits(&granted, 17, &[4], &[3], &BTreeMap::new());

    // The parent is re-fitted to the 13 slots the split left it: `floor(2 x 13 / 17) = 1` and
    // `floor(15 x 13 / 17) = 11`. The splinter mints the profile's `4/4/4` against 4 slots:
    // `floor(4 x 4 / 12) = 1` of each.
    assert_eq!(
        expected,
        BTreeMap::from([
            ("big_game".to_owned(), 2),
            ("gathering".to_owned(), 12),
            ("trapping".to_owned(), 1),
        ]),
        "the family holds each side's own minted default, not the granted line re-cut"
    );
}

/// The kit half of the **default outfit every band mints for itself at creation**, off the shipped
/// start profile the harness builds its world from (`build_world` sends a plain `new_game`, so the
/// server resolves the same builtin profiles this reads).
fn opening_kit_defaults() -> BTreeMap<String, u32> {
    let profiles = core_sim::StartProfiles::builtin();
    profiles
        .first()
        .expect("the builtin start profiles carry a profile")
        .overrides()
        .opening_loadout
        .kit_defaults
        .clone()
}

/// The `kit <id> <n>` rows of a `set_starting_loadout` line, summed per kit.
fn kit_rows(line: &str) -> BTreeMap<String, u32> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut rows = BTreeMap::new();
    for window in tokens.windows(3).filter(|window| window[0] == "kit") {
        *rows.entry(window[1].to_owned()).or_default() +=
            window[2].parse::<u32>().expect("a kit count");
    }
    rows
}
