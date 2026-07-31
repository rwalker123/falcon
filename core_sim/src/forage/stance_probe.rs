//! **A TEMPORARY MEASUREMENT HARNESS — no assertions on gameplay, no shipped behaviour.**
//!
//! It answers one design question with numbers instead of algebra: *where does a fully-staffed
//! source end up under each of the four harvest stances, on each food web, with and without a build
//! running* — and it answers it by driving the **shipped** functions turn by turn in the shipped
//! stage order (Logistics regrowth → Population take → Population build accrual), never by
//! re-deriving a formula.
//!
//! It lives as a submodule of `forage` because the plant half needs `regrow_patch`, which is private
//! to that module; the animal half needs nothing private and is here only to keep one probe in one
//! file. Both tests are `#[ignore]`d — run them with:
//!
//! ```text
//! cargo test -p core_sim --lib stance_probe -- --ignored --nocapture
//! ```

use super::*;
use crate::components::{FollowPolicy, Improvement};
use crate::fauna::{
    herd_capacity, herd_ecology, regrow_biomass, EcologyPhase as FaunaEcologyPhase, Herd,
};
use crate::fauna_config::{FaunaConfig, HusbandryCeiling, SizeClass};
use crate::flora_config::FloraConfig;
use crate::intensification::{
    LadderConfig, RungDef, RungKey, RUNG_COMPLETE, RUNG_TIMESCALE_UNSCALED,
};
use crate::labor_config::LaborConfig;
use crate::systems::hunt_take;
use sim_runtime::TerrainType;

// ---- Probe constants (harness parameters, not gameplay levers) --------------------------------

/// **Every build in this probe is staffed to its full crew**, so the figures it reports are the
/// rung's own pace rather than a staffing shortfall's. A rung that declares no `crew_needed` (both
/// animal rungs) is unscaled by crew, so one worker measures its true rate there.
fn full_crew(rung: &RungDef) -> u32 {
    rung.build_crew_needed().unwrap_or(SOLE_WORKER)
}

/// The crew a rung with no declared crew is probed at — one, because the scale is the identity there
/// and any other number would imply the probe had chosen a staffing level.
const SOLE_WORKER: u32 = 1;

/// Turns each run is driven for. Long enough for every stance on both webs to reach its fixed point
/// (or its floor) with room to spare — the slowest mover is a mammoth at `r = 0.04`.
const PROBE_TURNS: u32 = 600;

/// The trailing window a run's "settled at" figure is read over: the mean of the last `N` turns, so
/// a stance that *chatters* around a boundary (plant Surplus at the Allee line) reports its centre
/// rather than whichever side of it turn 600 happened to land on.
const SETTLE_WINDOW: u32 = 60;

/// Crew sizes chosen so the **ceiling** binds and labour never does, which is the question asked.
const FULLY_STAFFED_FORAGERS: u32 = 10_000;
const FULLY_STAFFED_HUNTERS: u32 = 100_000;

/// A resident band's Hunt take has no carry limit — it banks the whole take (`hunt_take`'s own
/// contract).
const NO_CARRY_LIMIT: f32 = f32::INFINITY;

/// Neutral band productivity and a full growing season, so the numbers are the source's own.
const UNIT_OUTPUT_MULTIPLIER: f32 = 1.0;
const FULL_SEASONAL_WEIGHT: f32 = 1.0;

/// **The reference basket** the plant figures are quoted on (the one `.claude/rules/core_sim/`
/// already quotes): `AlluvialPlain`, tile `(0, 0)`, the shipped `sweep_tiles` fixture seed.
const REFERENCE_BIOME: TerrainType = TerrainType::AlluvialPlain;
const REFERENCE_MAP_SEED: u64 = 0xF10A_5EED_C011_0010;

/// The faction that owns every build in this probe.
const PROBE_FACTION: FactionId = FactionId(0);

/// A herd's `K` for the probe: the species' own upper biomass band, the value `spawn_initial_herds`
/// seeds a full group at.
fn species_capacity(def: &crate::fauna_config::SpeciesDef) -> f32 {
    def.biomass[1]
}

// ---- Plant web --------------------------------------------------------------------------------

struct PlantOutcome {
    settled_fraction: f32,
    final_fraction: f32,
    phase: EcologyPhase,
    take_biomass: f32,
    provisions: f32,
    turns_to_floor: Option<u32>,
    turns_to_leave_thriving: Option<u32>,
}

/// Drive one forage patch forward under one `(stance, improvement)` pair, fully staffed, **without**
/// ever accruing or completing a build — so the equilibrium reported is the one the ceiling holds
/// the patch at for as long as that pair is in force.
fn run_patch(policy: FollowPolicy, improvement: Option<Improvement>) -> PlantOutcome {
    let labor = LaborConfig::builtin();
    let forage = &labor.forage;
    let flora = FloraConfig::builtin();
    let ladder = LadderConfig::builtin();
    let composition =
        flora.realized_composition(REFERENCE_BIOME, UVec2::new(0, 0), REFERENCE_MAP_SEED);
    let cap = forage.capacity_for(REFERENCE_BIOME);

    let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
    patch.refresh_ecology_phase(&patch_ecology(&patch, forage));

    let mut tail_fraction = 0.0;
    let mut tail_take = 0.0;
    let mut tail_provisions = 0.0;
    let mut turns_to_floor = None;
    let mut turns_to_leave_thriving = None;

    for turn in 1..=PROBE_TURNS {
        // Logistics.
        regrow_patch(&mut patch, forage);
        if turns_to_leave_thriving.is_none() && patch.ecology_phase != EcologyPhase::Thriving {
            turns_to_leave_thriving = Some(turn);
        }
        let before = patch.biomass;
        // Population.
        let provisions = forage_take(
            &mut patch,
            &composition,
            FULLY_STAFFED_FORAGERS,
            policy,
            improvement,
            forage,
            &flora,
            &ladder,
            UNIT_OUTPUT_MULTIPLIER,
            FULL_SEASONAL_WEIGHT,
        )
        .to_f32();
        let take = before - patch.biomass;

        if turns_to_floor.is_none() && patch.biomass <= forage.reseed_floor_fraction * cap {
            turns_to_floor = Some(turn);
        }
        if turn > PROBE_TURNS - SETTLE_WINDOW {
            tail_fraction += patch.biomass / cap;
            tail_take += take;
            tail_provisions += provisions;
        }
    }

    let window = SETTLE_WINDOW as f32;
    PlantOutcome {
        settled_fraction: tail_fraction / window,
        final_fraction: patch.biomass / cap,
        phase: patch.ecology_phase,
        take_biomass: tail_take / window,
        provisions: tail_provisions / window,
        turns_to_floor,
        turns_to_leave_thriving,
    }
}

struct PlantBuildOutcome {
    turns_to_complete: Option<u32>,
    provisions_over_build: f32,
    progress_at_horizon: f32,
    fraction_at_completion: f32,
}

/// Drive a patch under `(stance, verb)` and accrue that rung's meter exactly as
/// `advance_labor_allocation` does — after the take. **The two plant rungs gate differently and that
/// is the point of parameterising this:** rung 2 (`Cultivate`) requires the patch to be `Thriving`
/// and to carry a committed crop; rung 3 (`Sow`) requires only the site rule + Seed Selection, so its
/// `eligible` is unconditional here.
fn run_plant_build(policy: FollowPolicy, verb: Improvement) -> PlantBuildOutcome {
    let labor = LaborConfig::builtin();
    let forage = &labor.forage;
    let flora = FloraConfig::builtin();
    let ladder = LadderConfig::builtin();
    let composition =
        flora.realized_composition(REFERENCE_BIOME, UVec2::new(0, 0), REFERENCE_MAP_SEED);
    let cap = forage.capacity_for(REFERENCE_BIOME);
    let rung = ladder.rung(match verb {
        Improvement::Sow => RungKey::PlantField,
        _ => RungKey::PlantTended,
    });
    let improvement = Some(verb);

    let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
    patch.refresh_ecology_phase(&patch_ecology(&patch, forage));
    // The crew commits the patch to the best crop its own basket offers at this rung — the same
    // choice `resolve_committed_species` makes when the player names none.
    patch.species = composition
        .iter()
        .find(|share| {
            flora
                .species
                .get(&share.species)
                .is_some_and(|def| def.cultivation_ceiling.allows_cultivate())
        })
        .map(|share| share.species.clone());

    let mut provisions_over_build = 0.0;
    let mut turns_to_complete = None;
    let mut fraction_at_completion = 0.0;

    for turn in 1..=PROBE_TURNS {
        regrow_patch(&mut patch, forage);
        let provisions = forage_take(
            &mut patch,
            &composition,
            FULLY_STAFFED_FORAGERS,
            policy,
            improvement,
            forage,
            &flora,
            &ladder,
            UNIT_OUTPUT_MULTIPLIER,
            FULL_SEASONAL_WEIGHT,
        )
        .to_f32();
        if turns_to_complete.is_none() {
            provisions_over_build += provisions;
            patch.tended_this_turn = true;
            let eligible = match verb {
                // The Cultivate arm's gate, minus the knowledge check this probe grants.
                Improvement::Cultivate => {
                    patch.ecology_phase == EcologyPhase::Thriving && patch.species.is_some()
                }
                // `accrue_field`'s gate is the site rule + Seed Selection and NOTHING else — no
                // health check, deliberately (sown ground starts Collapsing).
                _ => true,
            };
            let accrual = rung.build_accrual(
                improvement,
                eligible,
                RUNG_TIMESCALE_UNSCALED,
                full_crew(rung),
            );
            if accrual > 0.0 {
                // The completion bool is the labor arm's feed-line trigger; this probe reads the
                // meter itself just below, so it is deliberately discarded here.
                let _completed_this_turn = match verb {
                    Improvement::Cultivate => patch.accrue_cultivation(PROBE_FACTION, accrual),
                    _ => patch.accrue_field(PROBE_FACTION, accrual),
                };
                let done = match verb {
                    Improvement::Cultivate => patch.is_cultivated(),
                    _ => patch.is_field(),
                };
                if done {
                    turns_to_complete = Some(turn);
                    fraction_at_completion = patch.biomass / cap;
                }
            }
        }
    }

    PlantBuildOutcome {
        turns_to_complete,
        provisions_over_build,
        progress_at_horizon: patch.cultivation_progress,
        fraction_at_completion,
    }
}

// ---- Animal web -------------------------------------------------------------------------------

struct HerdOutcome {
    settled_fraction: f32,
    final_fraction: f32,
    phase: FaunaEcologyPhase,
    take_biomass: f32,
    provisions: f32,
    turns_to_extinction_floor: Option<u32>,
    turns_to_leave_thriving: Option<u32>,
}

/// The biomass a probed herd starts at, as a fraction of `K`. `FULL_HERD` is the honest "healthy
/// source" start; `HALF_K_HERD` is the operating point a Sustain-hunted herd lives at and the state
/// `fauna_husbandry.rs`'s own fixtures prime to, so both are measured.
const FULL_HERD: f32 = 1.0;
const HALF_K_HERD: f32 = 0.5;

fn probe_herd(fauna: &FaunaConfig, species_key: &str, start_fraction: f32) -> Herd {
    let def = fauna
        .species
        .get(species_key)
        .expect("probe names a shipped species");
    let cap = species_capacity(def);
    let mut herd = Herd::new(
        format!("probe_{species_key}"),
        def.display_name.clone(),
        SizeClass::Small,
        vec![UVec2::new(1, 1)],
        cap * start_fraction,
        cap,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.refresh_ecology_phase(fauna);
    herd
}

fn run_herd(
    species_key: &str,
    policy: FollowPolicy,
    improvement: Option<Improvement>,
    start_fraction: f32,
) -> HerdOutcome {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let ladder = LadderConfig::builtin();
    let mut herd = probe_herd(&fauna, species_key, start_fraction);
    let cap = herd_capacity(&herd, &fauna);
    let hunt_yield = fauna.hunt_yield_for(&herd.species);
    let extinction_floor = herd_ecology(&herd, &fauna).extinction_floor * cap;

    let mut tail_fraction = 0.0;
    let mut tail_take = 0.0;
    let mut tail_provisions = 0.0;
    let mut turns_to_extinction_floor = None;
    let mut turns_to_leave_thriving = None;

    for turn in 1..=PROBE_TURNS {
        // Logistics.
        regrow_biomass(&mut herd, &fauna);
        if turns_to_leave_thriving.is_none() && herd.ecology_phase != FaunaEcologyPhase::Thriving {
            turns_to_leave_thriving = Some(turn);
        }
        // Population.
        let take = hunt_take(
            &mut herd,
            FULLY_STAFFED_HUNTERS,
            policy,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
        );
        let provisions = hunt_yield
            .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
            .provisions;

        if turns_to_extinction_floor.is_none() && herd.biomass <= extinction_floor {
            turns_to_extinction_floor = Some(turn);
        }
        if turn > PROBE_TURNS - SETTLE_WINDOW {
            tail_fraction += herd.biomass / cap;
            tail_take += take.killed_biomass();
            tail_provisions += provisions;
        }
    }

    let window = SETTLE_WINDOW as f32;
    HerdOutcome {
        settled_fraction: tail_fraction / window,
        final_fraction: herd.biomass / cap,
        phase: herd.ecology_phase,
        take_biomass: tail_take / window,
        provisions: tail_provisions / window,
        turns_to_extinction_floor,
        turns_to_leave_thriving,
    }
}

struct HerdBuildOutcome {
    turns_to_complete: Option<u32>,
    provisions_over_build: f32,
    progress_at_horizon: f32,
    fraction_at_completion: f32,
}

/// Drive an already-**tamed** herd under `(stance, Corral)` and accrue the rung-3 meter exactly as
/// `advance_labor_allocation` does. `accrue_corral`'s gate is Penning + the species ceiling +
/// ownership — **no health check**, so this measures whether a stance can stop a pen being built.
fn run_corral(species_key: &str, policy: FollowPolicy, start_fraction: f32) -> HerdBuildOutcome {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let ladder = LadderConfig::builtin();
    let pen = ladder.rung(RungKey::AnimalPen);
    let improvement = Some(Improvement::Corral);
    let mut herd = probe_herd(&fauna, species_key, start_fraction);
    herd.accrue_domestication(PROBE_FACTION, RUNG_COMPLETE);
    let cap = herd_capacity(&herd, &fauna);
    let hunt_yield = fauna.hunt_yield_for(&herd.species);

    let mut provisions_over_build = 0.0;
    let mut turns_to_complete = None;
    let mut fraction_at_completion = 0.0;

    for turn in 1..=PROBE_TURNS {
        regrow_biomass(&mut herd, &fauna);
        let take = hunt_take(
            &mut herd,
            FULLY_STAFFED_HUNTERS,
            policy,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
        );
        if turns_to_complete.is_none() {
            provisions_over_build += hunt_yield
                .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
                .provisions;
            let eligible =
                herd.can_pen() && herd.is_domesticated() && herd.owner == Some(PROBE_FACTION);
            let accrual = pen.build_accrual(
                improvement,
                eligible,
                RUNG_TIMESCALE_UNSCALED,
                full_crew(pen),
            );
            if accrual > 0.0 {
                let tile = herd.position();
                if herd.accrue_corral(PROBE_FACTION, accrual, tile) {
                    turns_to_complete = Some(turn);
                    fraction_at_completion = herd.biomass / cap;
                }
            }
        }
    }

    HerdBuildOutcome {
        turns_to_complete,
        provisions_over_build,
        progress_at_horizon: herd.corral_progress,
        fraction_at_completion,
    }
}

/// Drive a herd under `(stance, Tame)` and accrue the rung-2 meter exactly as
/// `advance_labor_allocation` does — after the take, gated on the herd being `Thriving` and its
/// species' husbandry ceiling allowing domestication, at the species' own `taming_rate` timescale.
fn run_tame(species_key: &str, policy: FollowPolicy, start_fraction: f32) -> HerdBuildOutcome {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let ladder = LadderConfig::builtin();
    let pastoral = ladder.rung(RungKey::AnimalPastoral);
    let improvement = Some(Improvement::Tame);
    let mut herd = probe_herd(&fauna, species_key, start_fraction);
    let cap = herd_capacity(&herd, &fauna);
    let hunt_yield = fauna.hunt_yield_for(&herd.species);
    let timescale = fauna.taming_rate_for(&herd.species);

    let mut provisions_over_build = 0.0;
    let mut turns_to_complete = None;
    let mut fraction_at_completion = 0.0;

    for turn in 1..=PROBE_TURNS {
        regrow_biomass(&mut herd, &fauna);
        let take = hunt_take(
            &mut herd,
            FULLY_STAFFED_HUNTERS,
            policy,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
        );
        if turns_to_complete.is_none() {
            provisions_over_build += hunt_yield
                .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
                .provisions;
            herd.tamed_this_turn = true;
            let eligible =
                herd.can_domesticate() && herd.ecology_phase == FaunaEcologyPhase::Thriving;
            let accrual =
                pastoral.build_accrual(improvement, eligible, timescale, full_crew(pastoral));
            if accrual > 0.0 {
                herd.accrue_domestication(PROBE_FACTION, accrual);
                if herd.is_domesticated() {
                    turns_to_complete = Some(turn);
                    fraction_at_completion = herd.biomass / cap;
                }
            }
        }
    }

    HerdBuildOutcome {
        turns_to_complete,
        provisions_over_build,
        progress_at_horizon: herd.domestication_progress,
        fraction_at_completion,
    }
}

// ---- Reports ----------------------------------------------------------------------------------

const STANCES: [FollowPolicy; 4] = [
    FollowPolicy::Sustain,
    FollowPolicy::Surplus,
    FollowPolicy::Deplete,
    FollowPolicy::Eradicate,
];

/// The species the animal report covers: a fast breeder, two mid ones, and two slow ones — chosen so
/// both `husbandry_ceiling: wild` (never tameable) and both tameable ceilings are represented.
const PROBE_SPECIES: [&str; 5] = ["rabbit", "deer", "boar", "steppe_runner", "mammoth"];

fn opt(turn: Option<u32>) -> String {
    turn.map_or_else(|| "-".to_string(), |t| t.to_string())
}

#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_plant_stances() {
    let labor = LaborConfig::builtin();
    let flora = FloraConfig::builtin();
    let ladder = LadderConfig::builtin();
    let cap = labor.forage.capacity_for(REFERENCE_BIOME);
    let composition =
        flora.realized_composition(REFERENCE_BIOME, UVec2::new(0, 0), REFERENCE_MAP_SEED);
    println!("\n=== PLANT WEB — forage patch, {REFERENCE_BIOME:?}, K = {cap} ===");
    print!("basket:");
    for share in &composition {
        print!(" {} {:.3}", share.species, share.share);
    }
    println!(
        "\nr {}  collapse<{}K  stressed<{}K  reseed floor {}K  surplus x{}  deplete {}B  \
         eradicate {}B  cultivate dip x{}",
        labor.forage.ecology.regrowth_rate,
        labor.forage.ecology.collapse_fraction,
        labor.forage.ecology.stressed_fraction,
        labor.forage.reseed_floor_fraction,
        labor.forage.surplus_multiplier,
        labor.forage.market.take_fraction,
        labor.forage.eradicate.take_fraction,
        ladder.build_dip(Some(Improvement::Cultivate)),
    );

    println!("\n-- Part 1: no build running ({PROBE_TURNS} turns, starting at K) --");
    println!(
        "{:<10} {:>9} {:>9} {:>11} {:>10} {:>10} {:>8} {:>9}",
        "stance", "settles", "final", "phase", "take/turn", "food/turn", "->!thriv", "->floor"
    );
    for stance in STANCES {
        let out = run_patch(stance, None);
        println!(
            "{:<10} {:>8.3}K {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>9}",
            stance.as_str(),
            out.settled_fraction,
            out.final_fraction,
            format!("{:?}", out.phase),
            out.take_biomass,
            out.provisions,
            opt(out.turns_to_leave_thriving),
            opt(out.turns_to_floor),
        );
    }

    println!("\n-- Part 2: Cultivate in flight --");
    println!(
        "{:<10} {:>9} {:>11} {:>10} {:>10} {:>8} {:>11} {:>11} {:>9}",
        "stance",
        "settles",
        "phase",
        "take/turn",
        "food/turn",
        "->!thriv",
        "buildturns",
        "food/build",
        "B at done"
    );
    for stance in STANCES {
        let held = run_patch(stance, Some(Improvement::Cultivate));
        let built = run_plant_build(stance, Improvement::Cultivate);
        println!(
            "{:<10} {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>11} {:>11.2} {:>8.3}K",
            stance.as_str(),
            held.settled_fraction,
            format!("{:?}", held.phase),
            held.take_biomass,
            held.provisions,
            opt(held.turns_to_leave_thriving),
            built.turns_to_complete.map_or_else(
                || format!("never({:.2})", built.progress_at_horizon),
                |t| t.to_string()
            ),
            built.provisions_over_build,
            built.fraction_at_completion,
        );
    }
}

#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_animal_stances() {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    println!("\n=== ANIMAL WEB — wild herds ({PROBE_TURNS} turns) ===");
    println!(
        "collapse<{}K  stressed<{}K  extinction floor {}K  surplus x{} MSY  deplete x{} MSY  \
         tame dip x{}",
        fauna.ecology.collapse_fraction,
        fauna.ecology.stressed_fraction,
        fauna.ecology.extinction_floor,
        fauna.hunt.surplus_multiplier,
        fauna.hunt.deplete_multiplier,
        ladder.build_dip(Some(Improvement::Tame)),
    );

    for key in PROBE_SPECIES {
        let def = &fauna.species[key];
        let cap = species_capacity(def);
        println!(
            "\n-- {} (r {}, K {}, body {}, ceiling {:?}) --",
            def.display_name,
            def.regrowth_rate.unwrap_or(fauna.ecology.regrowth_rate),
            cap,
            def.body_mass,
            def.husbandry_ceiling,
        );
        println!(
            "{:<10} {:>9} {:>9} {:>11} {:>10} {:>10} {:>8} {:>9}",
            "stance", "settles", "final", "phase", "take/turn", "food/turn", "->!thriv", "->floor"
        );
        for stance in STANCES {
            let out = run_herd(key, stance, None, FULL_HERD);
            println!(
                "{:<10} {:>8.3}K {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>9}",
                stance.as_str(),
                out.settled_fraction,
                out.final_fraction,
                format!("{:?}", out.phase),
                out.take_biomass,
                out.provisions,
                opt(out.turns_to_leave_thriving),
                opt(out.turns_to_extinction_floor),
            );
        }
        if def.husbandry_ceiling == HusbandryCeiling::Wild {
            println!("  (Tame: n/a — husbandry_ceiling is `wild`, the species never tames)");
            continue;
        }
        for (label, start) in [("from K", FULL_HERD), ("from K/2", HALF_K_HERD)] {
            println!(
                "  Tame in flight, {label} (taming_rate x{}):",
                fauna.taming_rate_for(&def.display_name)
            );
            println!(
                "  {:<10} {:>9} {:>11} {:>10} {:>10} {:>8} {:>11} {:>11} {:>9}",
                "stance",
                "settles",
                "phase",
                "take/turn",
                "food/turn",
                "->!thriv",
                "buildturns",
                "food/build",
                "B at done"
            );
            for stance in STANCES {
                let held = run_herd(key, stance, Some(Improvement::Tame), start);
                let built = run_tame(key, stance, start);
                println!(
                    "  {:<10} {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>11} {:>11.2} {:>8.3}K",
                    stance.as_str(),
                    held.settled_fraction,
                    format!("{:?}", held.phase),
                    held.take_biomass,
                    held.provisions,
                    opt(held.turns_to_leave_thriving),
                    built.turns_to_complete.map_or_else(
                        || format!("never({:.2})", built.progress_at_horizon),
                        |t| t.to_string()
                    ),
                    built.provisions_over_build,
                    built.fraction_at_completion,
                );
            }
        }
    }
}

/// **Part 3 — the build/teaching axis, read off the two seams rather than off the docs.**
/// `RungDef::knowledge_earned` is the one earn seam and `RungDef::build_accrual` the one build seam,
/// so this prints exactly what a caller gets for every (rung × stance × improvement × health) cell.
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_build_and_teach_axis() {
    let ladder = LadderConfig::builtin();
    let rungs = [
        ("plant:wild", RungKey::PlantWild, None),
        (
            "plant:tended",
            RungKey::PlantTended,
            Some(Improvement::Cultivate),
        ),
        ("plant:field", RungKey::PlantField, Some(Improvement::Sow)),
        ("animal:wild", RungKey::AnimalWild, None),
        (
            "animal:pastoral",
            RungKey::AnimalPastoral,
            Some(Improvement::Tame),
        ),
        ("animal:pen", RungKey::AnimalPen, Some(Improvement::Corral)),
    ];

    println!("\n=== Part 3 — what each rung TEACHES, per stance (RungDef::knowledge_earned) ===");
    println!("(`eligible` is the caller's health gate — uniformly `phase == Thriving` at both earn sites)");
    println!(
        "{:<16} {:<10} {:>18} {:>18}",
        "rung", "stance", "eligible=true", "eligible=false"
    );
    for (label, key, _) in rungs {
        let rung = ladder.rung(key);
        for stance in STANCES {
            println!(
                "{:<16} {:<10} {:>18} {:>18}",
                label,
                stance.as_str(),
                rung.knowledge_earned(stance, true)
                    .map_or("-".to_string(), |id| id.to_string()),
                rung.knowledge_earned(stance, false)
                    .map_or("-".to_string(), |id| id.to_string()),
            );
        }
    }

    println!("\n=== Part 3 — what each rung BUILDS per turn (RungDef::build_accrual) ===");
    println!(
        "(the stance is NOT an argument — only the improvement and the caller's `eligible` are)"
    );
    println!(
        "{:<16} {:>10} {:>16} {:>16} {:>10}",
        "rung", "dip", "accrual eligible", "accrual !eligible", "decay"
    );
    for (label, key, verb) in rungs {
        let rung = ladder.rung(key);
        println!(
            "{:<16} {:>10} {:>16.4} {:>16.4} {:>10.4}",
            label,
            verb.map_or("-".to_string(), |v| format!(
                "x{}",
                ladder.build_dip(Some(v))
            )),
            rung.build_accrual(verb, true, RUNG_TIMESCALE_UNSCALED, full_crew(rung)),
            rung.build_accrual(verb, false, RUNG_TIMESCALE_UNSCALED, full_crew(rung)),
            rung.build_decay(RUNG_TIMESCALE_UNSCALED),
        );
    }
}

#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_rung_three_builds() {
    println!("\n=== Rung 3 — the builds with NO health gate (Sow / Corral) ===");
    println!(
        "plant:field, {REFERENCE_BIOME:?}, K = {}",
        LaborConfig::builtin().forage.capacity_for(REFERENCE_BIOME)
    );
    println!(
        "{:<10} {:>12} {:>12} {:>9}",
        "stance", "buildturns", "food/build", "B at done"
    );
    for stance in STANCES {
        let built = run_plant_build(stance, Improvement::Sow);
        println!(
            "{:<10} {:>12} {:>12.2} {:>8.3}K",
            stance.as_str(),
            built.turns_to_complete.map_or_else(
                || format!("never({:.2})", built.progress_at_horizon),
                |t| t.to_string()
            ),
            built.provisions_over_build,
            built.fraction_at_completion,
        );
    }

    let fauna = FaunaConfig::builtin();
    for key in ["rabbit", "boar"] {
        let def = &fauna.species[key];
        println!(
            "\nanimal:pen on an already-tamed {} (r {} x pastoral_gain, K {})",
            def.display_name,
            def.regrowth_rate.unwrap_or(fauna.ecology.regrowth_rate),
            species_capacity(def)
        );
        println!(
            "{:<10} {:>12} {:>12} {:>9}",
            "stance", "buildturns", "food/build", "B at done"
        );
        for stance in STANCES {
            let built = run_corral(key, stance, FULL_HERD);
            println!(
                "{:<10} {:>12} {:>12.2} {:>8.3}K",
                stance.as_str(),
                built.turns_to_complete.map_or_else(
                    || format!("never({:.2})", built.progress_at_horizon),
                    |t| t.to_string()
                ),
                built.provisions_over_build,
                built.fraction_at_completion,
            );
        }
    }
}
