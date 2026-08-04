//! **The harvest-floor property tests, plus the measurement harness they grew out of.**
//!
//! It drives a source turn by turn through the **shipped** functions in the shipped stage order
//! (Logistics regrowth → Population take → Population build accrual), never by re-deriving a formula,
//! and asserts the two properties the retired four-stance axis violated
//! (`docs/plan_harvest_floor.md` §9):
//!
//! - **turn one is monotone in the floor** — a deeper floor never takes less *now*;
//! - **the 600-turn total is monotone the other way** — a deeper floor yields less *over time*, for
//!   every floor at or below `K/2` (`§2`: the sustained take `r·fK·(1−f)` peaks at `f = 0.5`).
//!
//! Together those are the trade the arc exists to make real, in both directions. **Every
//! monotonicity assertion is paired with a liveness one** — a diff-based property improves when the
//! feature breaks, so ordering alone would pass on a take path that returned zero everywhere.
//!
//! The floors are swept as the numbers they are — a labor assignment carries a floor
//! ([`crate::components::LaborTarget`]), so these tests drive the take path's own argument rather
//! than a stance that stands for one. [`DESCENDING_FLOORS`] is the ladder they walk, and it reaches
//! **above** the food peak as well as below it, which the four-stance axis could not express.
//!
//! It lives as a submodule of `forage` because the plant half needs `regrow_patch`, which is private
//! to that module; the animal half needs nothing private and is here only to keep one probe in one
//! file. The four **report** functions below are still `#[ignore]`d measurement harnesses — run them
//! with:
//!
//! ```text
//! cargo test -p core_sim --lib stance_probe -- --ignored --nocapture
//! ```

use super::*;
use crate::components::Improvement;
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

/// The first simulated turn — the one a "how much do I get NOW" reading is taken on. A named
/// constant because the loops below are 1-indexed and `1` on its own reads as a magic offset.
const FIRST_TURN: u32 = 1;

/// The trailing window a run's "settled at" figure is read over: the mean of the last `N` turns, so
/// a stance that *chatters* around a boundary (plant Surplus at the Allee line) reports its centre
/// rather than whichever side of it turn 600 happened to land on.
const SETTLE_WINDOW: u32 = 60;

/// Crew sizes chosen so the **ceiling** binds and labour never does, which is the question asked.
const FULLY_STAFFED_FORAGERS: u32 = 10_000;
const FULLY_STAFFED_HUNTERS: u32 = 100_000;

/// A resident band's Hunt take has no carry limit — it banks the whole take (`hunt_take`'s own
/// contract).
/// The probe measures the FLOOR, so it holds the retreat draw fixed. Every probe species ships
/// `wariness 0`, which makes the draw an identity and the seed inert — pinning it anyway keeps the
/// probe deterministic if a species it uses is ever given a value.
const PROBE_RETREAT_SEED: u64 = 0;

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
    /// Biomass taken on turn **one**, from a full (`B = K`) stand — the "how much now" half of the
    /// trade the floor makes.
    first_turn_take: f32,
    /// Biomass taken over the whole `PROBE_TURNS` run — the "how much over time" half.
    total_take: f32,
}

/// Drive one forage patch forward under one `(stance, improvement)` pair, fully staffed, **without**
/// ever accruing or completing a build — so the equilibrium reported is the one the ceiling holds
/// the patch at for as long as that pair is in force.
fn run_patch(floor: f32, improvement: Option<Improvement>) -> PlantOutcome {
    run_patch_with_crew(floor, improvement, FULLY_STAFFED_FORAGERS)
}

/// [`run_patch`] at a chosen crew size, so the properties can be swept over the labor-bound regime
/// as well as the ceiling-bound one.
fn run_patch_with_crew(
    floor: f32,
    improvement: Option<Improvement>,
    foragers: u32,
) -> PlantOutcome {
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
    let mut first_turn_take = 0.0;
    let mut total_take = 0.0;

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
            foragers,
            floor,
            improvement,
            forage,
            &flora,
            &ladder,
            UNIT_OUTPUT_MULTIPLIER,
            FULL_SEASONAL_WEIGHT,
        )
        .to_f32();
        let take = before - patch.biomass;
        if turn == FIRST_TURN {
            first_turn_take = take;
        }
        total_take += take;

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
        first_turn_take,
        total_take,
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
fn run_plant_build(floor: f32, verb: Improvement) -> PlantBuildOutcome {
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
        let biomass_before = patch.biomass;
        // The escapement room, PRE-take — the work predicate the labor arm's Cultivate gate reads
        // (`systems::labor::crew_is_working_the_source`).
        let standing_above_floor =
            escapement_ceiling(floor, biomass_before, patch.carrying_capacity);
        let provisions = forage_take(
            &mut patch,
            &composition,
            FULLY_STAFFED_FORAGERS,
            floor,
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
                // The Cultivate arm's gate, minus the knowledge check this probe grants. The health
                // gate is gone (`docs/plan_harvest_floor.md` §3.2); the escapement room replaced it.
                Improvement::Cultivate => standing_above_floor > 0.0 && patch.species.is_some(),
                // `accrue_field`'s gate is the site rule + Seed Selection and NOTHING else — no
                // health check and no work predicate, deliberately: sown ground draws nothing.
                _ => true,
            };
            let accrual = rung.build_accrual(
                improvement,
                eligible,
                floor,
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
    /// Biomass killed on turn **one** — the "how much now" half of the trade the floor makes.
    first_turn_take: f32,
    /// Biomass killed over the whole `PROBE_TURNS` run — the "how much over time" half.
    total_take: f32,
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
    floor: f32,
    improvement: Option<Improvement>,
    start_fraction: f32,
) -> HerdOutcome {
    run_herd_with_crew(
        species_key,
        floor,
        improvement,
        start_fraction,
        FULLY_STAFFED_HUNTERS,
    )
}

/// [`run_herd`] at a chosen crew size, so the properties can be swept over the labor-bound regime as
/// well as the ceiling-bound one.
fn run_herd_with_crew(
    species_key: &str,
    floor: f32,
    improvement: Option<Improvement>,
    start_fraction: f32,
    hunters: u32,
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
    let mut first_turn_take = 0.0;
    let mut total_take = 0.0;

    for turn in 1..=PROBE_TURNS {
        // Logistics.
        regrow_biomass(&mut herd, &fauna);
        if turns_to_leave_thriving.is_none() && herd.ecology_phase != FaunaEcologyPhase::Thriving {
            turns_to_leave_thriving = Some(turn);
        }
        // Population.
        let take = hunt_take(
            &mut herd,
            hunters,
            floor,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            // The probe measures what the FLOOR does to a herd, so it hunts with the shipped kit —
            // the tier an ordinary band is on. A dry-speared party is a different probe.
            &crate::fauna::HuntingParty::builtin_equipped(),
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            PROBE_RETREAT_SEED,
        )
        .take;
        let provisions = hunt_yield
            .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
            .provisions;
        if turn == FIRST_TURN {
            first_turn_take = take.killed_biomass();
        }
        total_take += take.killed_biomass();

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
        first_turn_take,
        total_take,
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
fn run_corral(species_key: &str, floor: f32, start_fraction: f32) -> HerdBuildOutcome {
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
            floor,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            &crate::fauna::HuntingParty::builtin_equipped(),
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            PROBE_RETREAT_SEED,
        )
        .take;
        if turns_to_complete.is_none() {
            provisions_over_build += hunt_yield
                .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
                .provisions;
            let eligible =
                herd.can_pen() && herd.is_domesticated() && herd.owner == Some(PROBE_FACTION);
            let accrual = pen.build_accrual(
                improvement,
                eligible,
                floor,
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
fn run_tame(species_key: &str, floor: f32, start_fraction: f32) -> HerdBuildOutcome {
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
        // The escapement room, PRE-take and PRE-quantisation — the work predicate the labor arm's
        // Tame gate reads (`systems::labor::crew_is_working_the_source`).
        let standing_above_floor = escapement_ceiling(floor, herd.biomass, cap);
        let take = hunt_take(
            &mut herd,
            FULLY_STAFFED_HUNTERS,
            floor,
            improvement,
            labor.hunt.per_worker_biomass_capacity,
            &crate::fauna::HuntingParty::builtin_equipped(),
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            PROBE_RETREAT_SEED,
        )
        .take;
        if turns_to_complete.is_none() {
            provisions_over_build += hunt_yield
                .apply(take.carried, UNIT_OUTPUT_MULTIPLIER)
                .provisions;
            herd.tamed_this_turn = true;
            // The Tame arm's gate, minus the knowledge check this probe grants. The health gate is
            // gone (`docs/plan_harvest_floor.md` §3.2); what replaced it is the **escapement room**,
            // read pre-take and pre-quantisation, never "an animal died".
            let eligible = herd.can_domesticate() && standing_above_floor > 0.0;
            let accrual = pastoral.build_accrual(
                improvement,
                eligible,
                floor,
                timescale,
                full_crew(pastoral),
            );
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

/// The floors the **ignored report harnesses** print a row for — the four the retired stance axis
/// named, so the measured tables stay comparable with the ones in the rule files. The property tests
/// above sweep [`DESCENDING_FLOORS`] instead, which reaches above the food peak as well.
const REPORT_FLOORS: [f32; 4] = [0.5, 0.3, 0.15, 0.0];

// ---- The properties ---------------------------------------------------------------------------

/// **The floor ladder, deepening** — swept as the numbers they are, now that a labor assignment
/// carries a floor rather than a stance. `1.0` and `0.8` are included because they are reachable and
/// were not before: a dial can be dragged *above* the food peak, into deliberate under-harvest.
const DESCENDING_FLOORS: [f32; 6] = [1.0, 0.8, 0.5, 0.3, 0.15, 0.0];

/// *"Take everything"* — the floor-`0` end of the dial, named because `0.0` as a bare argument reads
/// as an absent value rather than as the deliberate instruction it is.
const STRIP_IT_BARE: f32 = 0.0;

/// Crew sizes the turn-one property is swept over: **one** worker (labor-bound on any real source),
/// a small band, and a crew so large the ceiling is the only thing that can bind. The property must
/// hold in every regime — a take that were monotone only when the ceiling binds would be monotone in
/// the *crew*, not in the floor.
const PROBE_CREW_SIZES: [u32; 3] = [1, 8, FULLY_STAFFED_HUNTERS];

/// Nothing may be taken from a source and reported as zero: the paired **liveness** bound every
/// monotonicity assertion below carries, so an ordering cannot pass by everything collapsing to `0`.
const SOME_TAKE: f32 = 0.0;

/// **A floor of `1.0` leaves the whole stock standing** — `B − 1.0·K` is `0` at capacity, so the
/// crew takes nothing. Deliberate under-harvest is a legal instruction the dial can give and the
/// retired stance axis could not express, so it is swept beside the rest and asserted as the zero it
/// honestly is rather than being excused from the liveness bound.
const TAKE_NOTHING_FLOOR: f32 = 1.0;

#[test]
fn a_deeper_floor_never_takes_less_on_turn_one_on_either_web() {
    for &crew in &PROBE_CREW_SIZES {
        // --- The plant web. A full stand, so every floor below `K` has room above it.
        let plant: Vec<f32> = DESCENDING_FLOORS
            .iter()
            .map(|&floor| {
                run_patch_with_crew(floor, None, crew.min(FULLY_STAFFED_FORAGERS)).first_turn_take
            })
            .collect();
        for (deeper, shallower) in plant.iter().skip(1).zip(plant.iter()) {
            assert!(
                *deeper >= *shallower - PROBE_TAKE_EPSILON,
                "plant, {crew} foragers: a deeper floor must never take LESS on turn one: {plant:?}"
            );
        }
        assert_live_below_capacity(&plant, &format!("plant, {crew} foragers"));

        // --- The animal web, over the whole probe roster (fast breeders to megafauna): the
        // whole-animal quantiser is where a monotone ceiling could still produce a non-monotone take.
        for key in PROBE_SPECIES {
            let animal: Vec<f32> = DESCENDING_FLOORS
                .iter()
                .map(|&floor| run_herd_with_crew(key, floor, None, FULL_HERD, crew).first_turn_take)
                .collect();
            for (deeper, shallower) in animal.iter().skip(1).zip(animal.iter()) {
                assert!(
                    *deeper >= *shallower - PROBE_TAKE_EPSILON,
                    "{key}, {crew} hunters: a deeper floor must never take LESS on turn one: \
                     {animal:?}"
                );
            }
            // **The liveness pair now asks whether the party could bring one down** — the premise the
            // fight added (`docs/plan_hunt_through_combat.md` §4.2). Below that threshold the take is
            // honestly zero at *every* floor, and that is a statement about the GATE, not about the
            // floor: asserting liveness there would be asserting that a lone hunter can kill a
            // mammoth. So the two regimes are asserted as the two different things they are.
            if crew >= hunters_to_bring_one_down(key) {
                assert_live_below_capacity(&animal, &format!("{key}, {crew} hunters"));
            } else {
                assert!(
                    animal.iter().all(|take| *take == 0.0),
                    "{key}, {crew} hunters: a party that cannot bring one down takes nothing at ANY \
                     floor — no floor is deep enough to substitute for a weapon: {animal:?}"
                );
            }
        }
    }
}

/// **The smallest party that can bring one animal of this species down in a single turn** —
/// `ceil(durability / max(0, attack − defense))` at the shipped kitted tier
/// (`docs/plan_hunt_through_combat.md` §4.2), derived from config rather than tabulated so a retune
/// of any of the three inputs moves it.
///
/// **Damage does not bank between turns** (§7: *the animal does not wait* — there is no partial-kill
/// meter), so a party below this threshold takes **nothing, at every floor, forever**. That list of
/// zeros orders perfectly, which is exactly why the sweep above must not read it as a floor
/// property.
///
/// A party that cannot beat the quarry's `defense` at all can never bring one down, at any headcount
/// — §0.2's founding case — so this answers [`u32::MAX`] there rather than a large finite crew.
fn hunters_to_bring_one_down(species_key: &str) -> u32 {
    let fauna = FaunaConfig::builtin();
    let party = crate::fauna::HuntingParty::builtin_equipped();
    let quarry = fauna
        .species
        .get(species_key)
        .expect("probe names a shipped species");
    let per_hunter = crate::combat::strike_damage(party.hunter.attack, quarry.combat.defense);
    if per_hunter <= 0.0 {
        return u32::MAX;
    }
    (quarry.combat.durability / per_hunter).ceil() as u32
}

/// **The liveness bound that pairs with every turn-one monotonicity assertion** — a full source has
/// room above every floor *below* `K`, so each of those must take something; the `1.0` end must take
/// exactly nothing, because that is what "leave it all standing" means.
///
/// Stated as one helper because an ordering assertion is satisfied by a list of zeros, and the
/// interesting failure — a take path that quietly returns `0` everywhere — orders perfectly.
fn assert_live_below_capacity(takes: &[f32], label: &str) {
    for (floor, take) in DESCENDING_FLOORS.iter().zip(takes.iter()) {
        if *floor >= TAKE_NOTHING_FLOOR {
            assert_eq!(
                *take, 0.0,
                "{label}, floor {floor}: leaving the whole stock standing takes nothing: {takes:?}"
            );
        } else {
            assert!(
                *take > SOME_TAKE,
                "{label}, floor {floor}: a full source has room above every floor below `K`, so it \
                 must take something on turn one: {takes:?}"
            );
        }
    }
}

/// **The other half of the trade, and the PIVOT AT `K/2`** (`docs/plan_harvest_floor.md` §2).
///
/// The sustained take at floor `f` is the regrowth there, `r·fK·(1−f)`, which **peaks at `f = 0.5`**.
/// So the 600-turn total is not monotone across the dial's whole range — it is monotone on each side
/// of the food peak, and the peak is the answer:
///
/// - **below `K/2`** a deeper floor yields LESS over time (the trade against taking more *now*, which
///   the turn-one test pins);
/// - **above `K/2`** a shallower floor also yields less — deliberate under-harvest, which the retired
///   four-stance axis could not express at all and the dial can.
///
/// Asserting the peak rather than a one-sided ordering is what makes this test see the model instead
/// of the four values the old axis happened to name, all of which sat at or below `0.5`.
///
/// Read at the fully-staffed crew: the question is what the FLOOR costs over time, so labour must not
/// be the binding term.
#[test]
fn the_six_hundred_turn_total_peaks_at_the_food_peak_on_either_web() {
    let plant: Vec<f32> = DESCENDING_FLOORS
        .iter()
        .map(|&floor| run_patch(floor, None).total_take)
        .collect();
    assert_peaks_at_the_food_peak(&plant, "plant");
    for (floor, total) in DESCENDING_FLOORS.iter().zip(plant.iter()) {
        if *floor >= TAKE_NOTHING_FLOOR {
            continue;
        }
        assert!(
            *total > SOME_TAKE,
            "plant, floor {floor}: a patch reseeds, so every floor below `K` keeps paying \
             something over {PROBE_TURNS} turns: {plant:?}"
        );
    }

    for key in PROBE_SPECIES {
        let animal: Vec<f32> = DESCENDING_FLOORS
            .iter()
            .map(|&floor| run_herd(key, floor, None, FULL_HERD).total_take)
            .collect();
        assert_peaks_at_the_food_peak(&animal, key);
        for (floor, total) in DESCENDING_FLOORS.iter().zip(animal.iter()) {
            if *floor >= TAKE_NOTHING_FLOOR {
                continue;
            }
            assert!(
                *total > SOME_TAKE,
                "{key}, floor {floor}: every floor below `K` delivers SOMETHING over \
                 {PROBE_TURNS} turns — the deepest one at least strips the source once: {animal:?}"
            );
        }
    }
}

/// The 600-turn totals rise to the food peak and fall away from it on both sides, and the peak is a
/// **strict** maximum — a tie would mean the pivot the whole model turns on is not actually there.
///
/// `DESCENDING_FLOORS` runs shallow → deep, so the peak's index splits it into a rising tail and a
/// falling one when read in that order.
fn assert_peaks_at_the_food_peak(totals: &[f32], label: &str) {
    let peak = DESCENDING_FLOORS
        .iter()
        .position(|floor| *floor == crate::fauna::MSY_BIOMASS_FRACTION)
        .expect("the swept ladder must contain the food peak, or this asserts nothing");
    for window in totals[..=peak].windows(2) {
        assert!(
            window[1] >= window[0] - PROBE_TAKE_EPSILON,
            "{label}: above the food peak, a LOWER floor must not yield less over {PROBE_TURNS} \
             turns — under-harvest is the trade on that side: {totals:?}"
        );
    }
    for window in totals[peak..].windows(2) {
        assert!(
            window[1] <= window[0] + PROBE_TAKE_EPSILON,
            "{label}: below the food peak, a deeper floor must not out-yield a shallower one over \
             {PROBE_TURNS} turns: {totals:?}"
        );
    }
    let best = totals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (totals[peak] - best).abs() < PROBE_TAKE_EPSILON,
        "{label}: the 600-turn total must be MAXIMISED at the food peak `K/2`: {totals:?}"
    );
    assert!(
        totals[peak] > totals[totals.len() - 1] + PROBE_TAKE_EPSILON
            && totals[peak] > totals[0] + PROBE_TAKE_EPSILON,
        "{label}: the peak must be strict on both sides, or the trade is not real: {totals:?}"
    );
}

/// **Same constant, opposite outcome, both pinned** (`docs/plan_harvest_floor.md` §1.1). `floor = 0`
/// means *harvest maximally*, and the two food webs answer it differently **by config that already
/// exists**: a stripped patch is lifted by `reseed_floor_fraction` every turn and comes back; a herd
/// falls under `extinction_floor` and is gone.
///
/// This is the pair that makes "take everything" a web-specific decision rather than one verb with
/// one meaning — and it is asserted here rather than assumed, because both halves ride the same
/// `0.02` and a reader would reasonably expect them to behave the same.
#[test]
fn floor_zero_strips_a_patch_that_recovers_and_a_herd_that_does_not() {
    let labor = LaborConfig::builtin();
    let forage = &labor.forage;
    let cap = forage.capacity_for(REFERENCE_BIOME);

    // --- The plant web: stripped bare, and still alive there.
    let stripped = run_patch(STRIP_IT_BARE, None);
    assert!(
        stripped.turns_to_floor.is_some(),
        "a floor-0 gather must actually strip the stand — otherwise the recovery below is vacuous"
    );
    assert!(
        stripped.take_biomass > SOME_TAKE,
        "…and it keeps paying at the floor rather than dying there — the reseed lift refills it \
         every Logistics turn, which is the whole reason `floor = 0` is survivable on the plant \
         web: {} biomass/turn over the trailing window",
        stripped.take_biomass
    );

    // Left alone, the same stand climbs back out — no despawn, no permanent dead ground.
    let mut recovering = ForagePatch::new(UVec2::new(0, 0), cap);
    recovering.biomass = cap * forage.reseed_floor_fraction;
    recovering.refresh_ecology_phase(&patch_ecology(&recovering, forage));
    for _ in 0..PROBE_TURNS {
        regrow_patch(&mut recovering, forage);
    }
    assert_eq!(
        recovering.ecology_phase,
        EcologyPhase::Thriving,
        "a patch driven to floor 0 recovers once the gathering stops: {} of K",
        recovering.biomass / cap
    );

    // --- The animal web: the same 0.02, and the herd crosses it. `advance_herds` despawns there
    // (pinned live in `core_sim/tests/fauna_deplete.rs`); this pins that the take path takes it
    // under.
    let fauna = FaunaConfig::builtin();
    for key in PROBE_SPECIES {
        let wiped = run_herd(key, STRIP_IT_BARE, None, FULL_HERD);
        assert!(
            wiped.turns_to_extinction_floor.is_some(),
            "{key}: a floor-0 hunt takes the herd under `extinction_floor` ({}), where it disperses",
            fauna.ecology.extinction_floor
        );
        assert!(
            wiped.total_take > SOME_TAKE,
            "{key}: …and it is a HARVEST that does it, not an accounting hole: {}",
            wiped.total_take
        );
    }
}

/// Float slack for a take comparison — a chain of a few multiplications through the conversion rates,
/// on biomass values in the thousands. Ties are legitimate (two floors both labor-bound take the same
/// amount), so the comparisons are non-strict and this only covers the rounding.
const PROBE_TAKE_EPSILON: f32 = 1e-3;

/// The species the animal report covers: a fast breeder, two mid ones, and two slow ones — chosen so
/// both `husbandry_ceiling: wild` (never tameable) and both tameable ceilings are represented.
const PROBE_SPECIES: [&str; 5] = ["rabbit", "deer", "boar", "steppe_runner", "mammoth"];

/// The stance ladder printed as what it now is — four escapement floors, in fractions of `K`.
fn floor_ladder() -> String {
    REPORT_FLOORS
        .iter()
        .map(|floor| format!("{floor:.2}K"))
        .collect::<Vec<_>>()
        .join("  ")
}

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
        "\nr {}  collapse<{}K  stressed<{}K  reseed floor {}K  floors {}  cultivate dip x{}",
        labor.forage.ecology.regrowth_rate,
        labor.forage.ecology.collapse_fraction,
        labor.forage.ecology.stressed_fraction,
        labor.forage.reseed_floor_fraction,
        floor_ladder(),
        ladder.build_dip(Some(Improvement::Cultivate)),
    );

    println!("\n-- Part 1: no build running ({PROBE_TURNS} turns, starting at K) --");
    println!(
        "{:<10} {:>9} {:>9} {:>11} {:>10} {:>10} {:>8} {:>9}",
        "stance", "settles", "final", "phase", "take/turn", "food/turn", "->!thriv", "->floor"
    );
    for stance in REPORT_FLOORS {
        let out = run_patch(stance, None);
        println!(
            "{:<10} {:>8.3}K {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>9}",
            format!("{stance:.2}K"),
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
    for stance in REPORT_FLOORS {
        let held = run_patch(stance, Some(Improvement::Cultivate));
        let built = run_plant_build(stance, Improvement::Cultivate);
        println!(
            "{:<10} {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>11} {:>11.2} {:>8.3}K",
            format!("{stance:.2}K"),
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
        "collapse<{}K  stressed<{}K  extinction floor {}K  floors {}  tame dip x{}",
        fauna.ecology.collapse_fraction,
        fauna.ecology.stressed_fraction,
        fauna.ecology.extinction_floor,
        floor_ladder(),
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
        for stance in REPORT_FLOORS {
            let out = run_herd(key, stance, None, FULL_HERD);
            println!(
                "{:<10} {:>8.3}K {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>9}",
                format!("{stance:.2}K"),
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
            for stance in REPORT_FLOORS {
                let held = run_herd(key, stance, Some(Improvement::Tame), start);
                let built = run_tame(key, stance, start);
                println!(
                    "  {:<10} {:>8.3}K {:>11} {:>10.3} {:>10.3} {:>8} {:>11} {:>11.2} {:>8.3}K",
                    format!("{stance:.2}K"),
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

    println!("\n=== Part 3 — what each rung TEACHES, per floor (RungDef::knowledge_accrual) ===");
    println!("(`eligible` is the caller's 'is anything standing above the floor' gate; the AMOUNT is the floor's, normalised so the food peak is x1.0)");
    println!(
        "{:<16} {:<10} {:>24} {:>18}",
        "rung", "floor", "eligible=true", "eligible=false"
    );
    for (label, key, _) in rungs {
        let rung = ladder.rung(key);
        for floor in REPORT_FLOORS {
            println!(
                "{:<16} {:<10} {:>24} {:>18}",
                label,
                format!("{floor:.2}K"),
                rung.knowledge_accrual(floor, true, &ladder.knowledge)
                    .map_or("-".to_string(), |(id, amount)| format!(
                        "{id} @ {amount:.4}"
                    )),
                rung.knowledge_accrual(floor, false, &ladder.knowledge)
                    .map_or("-".to_string(), |(id, amount)| format!(
                        "{id} @ {amount:.4}"
                    )),
            );
        }
    }

    println!(
        "\n=== Part 3 — what each rung BUILDS per turn, per floor (RungDef::build_accrual) ==="
    );
    println!(
        "(the floor IS an argument now — it paces the build exactly as it paces the lesson; decay takes no floor)"
    );
    println!(
        "{:<16} {:>10} {:<10} {:>16} {:>16} {:>10}",
        "rung", "dip", "floor", "accrual eligible", "accrual !eligible", "decay"
    );
    for (label, key, verb) in rungs {
        let rung = ladder.rung(key);
        for floor in REPORT_FLOORS {
            println!(
                "{:<16} {:>10} {:<10} {:>16.4} {:>16.4} {:>10.4}",
                label,
                verb.map_or("-".to_string(), |v| format!(
                    "x{}",
                    ladder.build_dip(Some(v))
                )),
                format!("{floor:.2}K"),
                rung.build_accrual(verb, true, floor, RUNG_TIMESCALE_UNSCALED, full_crew(rung)),
                rung.build_accrual(verb, false, floor, RUNG_TIMESCALE_UNSCALED, full_crew(rung)),
                rung.build_decay(RUNG_TIMESCALE_UNSCALED),
            );
        }
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
    for stance in REPORT_FLOORS {
        let built = run_plant_build(stance, Improvement::Sow);
        println!(
            "{:<10} {:>12} {:>12.2} {:>8.3}K",
            format!("{stance:.2}K"),
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
        for stance in REPORT_FLOORS {
            let built = run_corral(key, stance, FULL_HERD);
            println!(
                "{:<10} {:>12} {:>12.2} {:>8.3}K",
                format!("{stance:.2}K"),
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
