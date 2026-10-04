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
/// was sent — **summed over the band and every band it split off, at least what the sim's own
/// partition leaves the family**. On this harness world *split to feed* fires on the grant turn,
/// and a split of a still-granting parent recomputes both carries (`starting-loadout.md` → "What a
/// SPLIT gives the splinter"): the splinter carries `asked × pack`, the parent what its remaining
/// workers carry, and the parent is re-fitted by `fit_to_carry` to what that carry leaves its fixed
/// larder. The replay is a **lower bound**: the parent keeps at least
/// `carry − Σ asked × pack − larder` for goods ([`family_kits`]), and a splinter holds its own
/// loadout line where it sent one, else something at least nothing. With no split the family — the
/// parent alone — must hold the whole line.
#[test]
fn a_utility_sim_ai_outfits_its_band_on_the_first_turn() {
    let server = start_server("ai_seat_outfit", UTILITY_PORT_BASE, None);
    let sim_ai = sim_ai_binary();
    build_world(&server);

    // The grant BEFORE the AI plays, through the rival's own seat — then released: every
    // resident rival band's open window, the carry it may mint against and its working-age hands.
    let grants: BTreeMap<u64, GrantWindow> = {
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
                Some((
                    cohort.band_id,
                    GrantWindow {
                        carry: window.carry_capacity,
                        per_worker_carry: cohort.carry_per_worker,
                        // A fixed larder counts against the carry; a yielding one does not.
                        fixed_larder: if window.food_fixed {
                            window.food_carried
                        } else {
                            0.0
                        },
                    },
                ))
            })
            .collect()
    };
    assert!(
        !grants.is_empty(),
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
    let granted_material_units: u32 = material_rows(line).values().sum();
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

    // The sim's rule replayed as a lower bound (`fission::open_splinter_loadout_window` and
    // `rebalance_partitioned_grant`): the parent keeps at least its line fitted to the carry the
    // splits left it, and a splinter holds its own line where it sent one. A splinter's minted
    // default (`starting_loadout::outfit_band_with_defaults`) is struck against a carry net of the
    // food it walked out with — a number the frame does not carry — so it is bounded below by
    // nothing. `split_loadout.rs` pins the rule exactly in process; this replays it against a real
    // server.
    let grant = *grants
        .get(&band)
        .unwrap_or_else(|| panic!("band {band} had no open grant window before the AI played"));
    let parent_budget = grant.carry;
    let item_weight = after.opening_loadout.item_carry_weight;
    let material_weight = after.opening_loadout.material_carry_weight;
    let items_per_kit: BTreeMap<String, u32> = after
        .kits
        .iter()
        .map(|kit| (kit.id.clone(), kit.item_ids.len() as u32))
        .collect();
    let line_load = granted
        .iter()
        .map(|(kit_id, count)| {
            *count as f32 * items_per_kit.get(kit_id).copied().unwrap_or(0) as f32 * item_weight
        })
        .sum::<f32>()
        + granted_material_units as f32 * material_weight;
    let expected = family_kits(&granted, line_load, grant, &splits, &children, &own_lines);
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
                 against a carry of {parent_budget}\n--- sim_ai log ---\n{}",
                granted.get(kit_id).copied().unwrap_or(0),
                splits.len(),
                log_tail(&ai.log_path)
            );
        }
    }
}

/// **A grant window as the rival's frame published it before the AI played** — its whole carry,
/// one worker's pack, and the larder that counts against the carry when it is fixed.
#[derive(Debug, Clone, Copy)]
struct GrantWindow {
    carry: f32,
    per_worker_carry: f32,
    fixed_larder: f32,
}

/// **At least what the family holds after its grant-turn splits**, per kit — the sim's own rule
/// replayed as a lower bound.
///
/// `granted` is the accepted line the parent was outfitted with and `line_load` what it weighs
/// (kits and materials together, the one currency), `grant` the parent's window before any split,
/// `splits` the workers each grant-turn split asked for in the order sent, and `children` the
/// splinters' band ids in the same order. A split recomputes the parent's carry to what its
/// remaining workers carry — its carry less `asked × per_worker_carry` — and its fixed larder counts
/// against it. That larder only shrinks as splinters take their food (a revision hands back at most
/// what was taken), so the parent keeps at least `carry − Σ asked × per_worker_carry − larder` for
/// goods, and is fitted to it proportionally and floored; a splinter that sent a loadout order of
/// its own holds that.
fn family_kits(
    granted: &BTreeMap<String, u32>,
    line_load: f32,
    grant: GrantWindow,
    splits: &[u32],
    children: &[u64],
    own_lines: &BTreeMap<u64, BTreeMap<String, u32>>,
) -> BTreeMap<String, u32> {
    let asked: u32 = splits.iter().sum();
    let kept_carry =
        (grant.carry - asked as f32 * grant.per_worker_carry - grant.fixed_larder).max(0.0);
    let kept_share = if line_load <= kept_carry || line_load <= 0.0 {
        1.0
    } else {
        kept_carry / line_load
    };
    let mut expected: BTreeMap<String, u32> = granted
        .iter()
        .map(|(kit_id, count)| (kit_id.clone(), (*count as f32 * kept_share).floor() as u32))
        .filter(|(_, count)| *count > 0)
        .collect();
    for child in children.iter().take(splits.len()) {
        for (kit_id, count) in own_lines.get(child).into_iter().flatten() {
            *expected.entry(kit_id.clone()).or_default() += count;
        }
    }
    expected
}

/// ⛔ **THE REPORTED CASE, PINNED WITHOUT A SERVER.**
///
/// Whether the utility seat splits at all on its grant turn is the AI's decision, and it does not
/// on every host — the CI runner split and the developer machine did not, so the arithmetic above
/// went unexercised locally while it failed in CI. This replays the reported line: a band granted
/// 15 `gathering` and 2 `big_game` (19 items) against a 17-hand carry, splitting once for 4 workers.
/// The numbers are a fixture, held fixed so the arithmetic is pinned whatever the shipped pack is.
///
/// Under the retired two-budget rule the parent was re-fitted to the 13 kit slots the split left it
/// and the family held 12 `gathering`. Under one carry the parent keeps its carry less the
/// splinter's `asked × pack` — `136 − 32 = 104` with no larder, far above the 19 its line weighs,
/// so it keeps the whole line. Its fixed larder counts against what it keeps.
#[test]
fn a_grant_turn_split_leaves_the_parent_holding_a_line_its_carry_still_covers() {
    const FIXTURE_PACK: f32 = 8.0;
    const FIXTURE_HANDS: f32 = 17.0;
    let unfed = GrantWindow {
        carry: FIXTURE_HANDS * FIXTURE_PACK,
        per_worker_carry: FIXTURE_PACK,
        fixed_larder: 0.0,
    };
    let granted = BTreeMap::from([("big_game".to_owned(), 2), ("gathering".to_owned(), 15)]);
    let expected = family_kits(&granted, 19.0, unfed, &[4], &[3], &BTreeMap::new());
    assert_eq!(
        expected, granted,
        "the parent's carry after the split still covers its whole line"
    );
    // And a line heavier than what the split leaves is fitted proportionally: 120 of load against
    // the 104 kept keeps `floor(count × 104 / 120)` of each row.
    let heavy = BTreeMap::from([("big_game".to_owned(), 30), ("gathering".to_owned(), 30)]);
    let fitted = family_kits(&heavy, 120.0, unfed, &[4], &[3], &BTreeMap::new());
    assert_eq!(
        fitted,
        BTreeMap::from([("big_game".to_owned(), 26), ("gathering".to_owned(), 26)])
    );
    // A fixed larder of 66 leaves 38 of the 104 for goods: the 19-load line keeps half of each row.
    let fed = GrantWindow {
        fixed_larder: 66.0,
        ..unfed
    };
    let kept = family_kits(&granted, 19.0, fed, &[4], &[3], &BTreeMap::new());
    assert_eq!(kept, granted, "38 of room still covers a 19-load line");
    let kept = family_kits(&heavy, 120.0, fed, &[4], &[3], &BTreeMap::new());
    assert_eq!(
        kept,
        BTreeMap::from([("big_game".to_owned(), 9), ("gathering".to_owned(), 9)]),
        "and a 120-load line keeps floor(30 × 38 / 120) = 9 of each row"
    );
}

/// The `kit <id> <n>` rows of a `set_starting_loadout` line, summed per kit.
fn kit_rows(line: &str) -> BTreeMap<String, u32> {
    keyword_rows(line, "kit")
}

/// The `material <id> <units>` rows of a `set_starting_loadout` line, summed per material.
fn material_rows(line: &str) -> BTreeMap<String, u32> {
    keyword_rows(line, "material")
}

fn keyword_rows(line: &str, keyword: &str) -> BTreeMap<String, u32> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut rows = BTreeMap::new();
    for window in tokens.windows(3).filter(|window| window[0] == keyword) {
        *rows.entry(window[1].to_owned()).or_default() +=
            window[2].parse::<u32>().expect("a row count");
    }
    rows
}
