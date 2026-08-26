//! **The authored wariness values, and the behaviour they turn on**
//! (`docs/plan_hunt_through_combat.md` §3.1, slice 7).
//!
//! # This is the file where the take is allowed to vary
//!
//! Slice 2 shipped `CombatStats::wariness` at `0` across the roster as a provable identity; slice 7
//! authors a real value on all twenty species. Every mechanism it needs — the retreat stage, the
//! per-event seed, the quantile machinery, the exported band — was already built and tested, so what
//! this file pins is not new plumbing but the three *properties* an authored value makes true:
//!
//! - **the forecast's range stops being degenerate** — the band widens and the live take falls
//!   inside it (§6.4's contract, finally exercised for real rather than on an identity);
//! - **a wary herd costs the party hunter-turns, never herd biomass** — escaped animals are not
//!   dead, so the herd loses exactly what was killed (§3);
//! - **wariness orders the roster** — a warier quarry yields less to the same crew.
//!
//! # Every OTHER suite holds wariness at `0`, and that is deliberate
//!
//! The pre-existing suite is this arc's deterministic regression net: a test that carries variance
//! can no longer tell a real regression from a draw, which is the one thing it exists to do. So the
//! harnesses that were written against a deterministic take neutralise the retreat through
//! [`FaunaConfig::without_retreat`] — the same move `hunt_yield_vector::steady_quarry` already makes
//! for `engage_rate` and `defense` — and the variance lives **only** here.
//!
//! Which is why every distribution assertion below is paired with a **liveness** one (§6.3): a band
//! of `[0, ∞]` contains everything, a dead retreat makes every draw identical so containment is
//! trivial, and "both takes were zero" satisfies any ordering.

/// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
/// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since quality
/// tiers landed, so a fixture that wants "an ordinary band" asks the item table.
fn equipped_haul_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::HuntCarry,
        core_sim::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity,
    )
}

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    animals_affordable, animals_engaged, build_test_app, herd_capacity, herd_hunt_yield,
    hunt_escapement_ceiling, hunt_take, recapture_snapshot_in_place, retreat_seed, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_herds, CombatConfig, CombatConfigHandle, FactionId,
    FaunaConfig, FaunaConfigHandle, GenerationId, Herd, HerdRegistry, HuntDraw, HuntingParty,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore, MoraleCause,
    PopulationCohort, ResidentBand, SnapshotHistory, SourcePriority, TileRegistry, NO_RETREAT,
};

// ---------------------------------------------------------------------------------------------
// §3.1 — THE AUTHORED TABLE
// ---------------------------------------------------------------------------------------------

/// The row §3.1 pins at the **bottom** of the ordering: a mammoth stands and fights, and its defences
/// are hide (`defense 12`) and `ferocity`, not absence.
const LEAST_WARY: &str = "mammoth";
/// The row §3.1 pins at the **top**: §4.2 lists the gazelle as surviving by *wariness alone* — frail,
/// fast, `durability 8`. It is the row the field exists for.
const MOST_WARY: &str = "gazelle";
/// The pen small game, which §3.1 says clusters **high**: they have nothing else, and a warren that
/// scatters is the second half of §2.1's pressure toward penning.
const PEN_SMALL_GAME: [&str; 4] = ["rabbit", "snow_hare", "fowl", "forest_grouse"];

/// **THE AUTHORED ORDERING IS THE ONE THE DESIGN STATES** — a guard on the shape of the table, not
/// on its exact numbers, so a retune is free to move a value and not free to invert the design.
///
/// §3.1 states four things a re-tune must not quietly undo, and each is asserted here: the mammoth is
/// the strict minimum, the gazelle the strict maximum, the pen small game sit above the roster's
/// median, and **no row ships `0` or `1.0`** — `1.0` would be unhuntable at every headcount and
/// weapon tier, and `0` is reserved for the identity path a deterministic harness installs.
#[test]
fn the_authored_ordering_is_the_one_the_design_states() {
    let fauna = FaunaConfig::builtin();
    let wariness = |key: &str| {
        fauna
            .species
            .get(key)
            .unwrap_or_else(|| panic!("{key} is in the shipped roster"))
            .combat
            .wariness
    };

    let least = wariness(LEAST_WARY);
    let most = wariness(MOST_WARY);
    for (key, def) in &fauna.species {
        let value = def.combat.wariness;
        assert!(
            value.is_finite() && (NO_RETREAT..1.0).contains(&value),
            "{key}: wariness must be finite in [0, 1) — 1.0 is unhuntable at any headcount: {value}"
        );
        assert!(
            value > NO_RETREAT,
            "{key}: every row of the roster is AUTHORED (§3.1); `0` is the identity a deterministic \
             harness installs, not a species' value"
        );
        assert!(
            value >= least,
            "{key} ({value}) undercuts the mammoth ({least}) — §3.1 pins the mammoth lowest because \
             it stands and fights"
        );
        assert!(
            value <= most,
            "{key} ({value}) outruns the gazelle ({most}) — §3.1 pins the gazelle highest because \
             evasion is all it has"
        );
    }
    // Strict, not merely bounding: the two anchors must be the sole holders of their extremes, or
    // the ordering above is satisfied by a table that tied everything together.
    assert!(least < most, "liveness: the roster is not one flat value");
    for (key, def) in &fauna.species {
        if key != LEAST_WARY {
            assert!(
                def.combat.wariness > least,
                "{key} ties the mammoth's floor"
            );
        }
        if key != MOST_WARY {
            assert!(
                def.combat.wariness < most,
                "{key} ties the gazelle's ceiling"
            );
        }
    }

    let mut sorted: Vec<f32> = fauna.species.values().map(|d| d.combat.wariness).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    for key in PEN_SMALL_GAME {
        assert!(
            wariness(key) >= median,
            "{key} ({}) must sit at or above the roster median ({median}) — §3.1's pen small game \
             cluster high",
            wariness(key)
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------------------------

/// A stock far above anything a crew can take, so the escapement floor never binds and the retreat is
/// what moves the number.
const TEST_CAPACITY: f32 = 4000.0;

/// The food peak — the default floor a fresh assignment gets.
const FOOD_PEAK: f32 = 0.5;

/// **The roster row where the retreat moves whole animals rather than fractions of one.** Defenceless
/// (`defense 0`), frail (`durability 2`), harmless (`ferocity 0`), cheap to reach (`engage_rate 10`)
/// and **wary** (`0.75`) — so a crew engages hundreds, keeps a quarter of them, and the whole-animal
/// quantiser at `body_mass 0.27` cannot absorb the spread the way it would on a deer.
const FRAIL_WARY_SPECIES: &str = "Rabbit Warren";

/// A crew large enough that neither its carry nor its `attack` binds: what is left to decide the take
/// is engagement and the retreat.
const CREW: u32 = 20;

/// A content band's output multiplier — morale `1.0`, so the forecast and the take share it.
const CONTENT_BAND_OUTPUT_MULTIPLIER: f32 = 1.0;

/// A resident band eats and banks its whole take, exactly as the Hunt labor arm passes.
const NO_CARRY_LIMIT: f32 = f32::INFINITY;

/// **The map seed every fixture here builds on.** The shipped default is `0`, which means *seed from
/// entropy* — so an un-pinned harness generates a different world per run. That is tolerable where a
/// fixture pins every term it reads off the herd, and **not** tolerable here: `fauna::retreat_seed`
/// folds `map_seed` in, so an entropy world would draw a different retreat every run and this file's
/// numbers would be irreproducible in exactly the dimension it exists to measure. Same value as the
/// sibling hunt suites, for no reason beyond reproducibility.
const SEED: u64 = 119_304_647;

/// A world whose first game herd is re-shaped into `display_name` at a healthy, whole-animal stock.
/// Re-speciating an existing herd keeps the placement/graze plumbing real while pinning the one
/// variable under test — the `hunt_forecast_range` pattern.
fn headless_with_species(display_name: &str) -> (App, String, UVec2) {
    let mut app = build_test_app();
    app.world
        .resource_mut::<core_sim::SimulationConfig>()
        .map_seed = SEED;
    app.update();
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_"))
            .map(|h| h.id.clone())
            .expect("the map seeded game")
    };
    let body_mass = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(display_name)
        .expect("the species is in the shipped roster")
        .body_mass;
    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.species = display_name.to_string();
        herd.body_mass = body_mass;
        // Freeze the range-derived `K` so the forecast and the take see one capacity: this file
        // measures the RETREAT, not the grazing loop.
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = TEST_CAPACITY;
        herd.biomass = TEST_CAPACITY;
        herd.biomass_before_regrowth = TEST_CAPACITY;
        herd.hunt_credit = 0.0;
        herd.position()
    };
    // Re-emit the display telemetry, or the wire describes the herd this fixture replaced.
    app.world.run_system_once(spawn_initial_herds);
    (app, id, pos)
}

/// **Author a species' `combat.wariness` to `value`, leaving everything else alone** — the lever every
/// ordering and identity assertion below turns, so "all else equal" is a fact about the config rather
/// than a hope about two different animals.
fn set_wariness(app: &mut App, display_name: &str, value: f32) {
    let mut config = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
    let key = config
        .species
        .iter()
        .find(|(_, def)| def.display_name == display_name)
        .map(|(key, _)| key.clone())
        .expect("the species is in the shipped roster");
    config
        .species
        .get_mut(&key)
        .expect("just resolved")
        .combat
        .wariness = value;
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// **Mark the herd's tile `Active` for the viewer**, so the fog-filtered snapshot actually publishes
/// the band that stands on it — a test reading a wire row has to reveal it first.
fn reveal(app: &mut App, pos: UVec2) {
    let grid = app.world.resource::<core_sim::SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    map.mark_active(pos.x, pos.y, 0);
}

/// A resident band of [`CREW`] hunting `fauna_id` at `floor`, standing on the herd's tile.
fn spawn_hunters(app: &mut App, pos: UVec2, fauna_id: &str, floor: f32) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            ResidentBand,
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor,
                    },
                    workers: CREW,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// **Seed the band's assignment row from its pre-commit forecast**, exactly as
/// `server::seed_source_yield` does when the player composes the assignment — the only row that
/// carries a distribution, since a resolved row is a fact and reports the point it paid.
fn seed_the_forecast(app: &mut App, band: bevy::prelude::Entity, fauna_id: &str, floor: f32) {
    let seeded = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let combat = app.world.resource::<CombatConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(fauna_id).expect("the herd is on the map");
        core_sim::hunt_source_yield_preview(
            herd,
            &fauna,
            equipped_haul_rate(),
            &party_at(&combat),
            CONTENT_BAND_OUTPUT_MULTIPLIER,
            CREW,
            floor,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            combat.forecast_range_sigmas,
        )
    };
    let target = LaborTarget::Hunt {
        fauna_id: fauna_id.to_string(),
        floor,
    };
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band carries its allocation")
        .set_source_yield(&target, seeded);
}

/// The shipped, fully-kitted party fighting at `combat`'s tuning — the composition
/// `server::seed_source_yield` builds for a band whose spears are whole.
fn party_at(combat: &CombatConfig) -> HuntingParty {
    // **Uniform** — a fully-covered party is one crew, which is what the builtin fixture is; only
    // the resolver dials are restated, because this suite installs its own `combat` config.
    let base = HuntingParty::builtin_equipped();
    HuntingParty::uniform(
        base.best_equipped_hunter(),
        combat.tuning(),
        combat.hunt_injury_damage_per_animal,
        base.dispersion,
    )
}

/// The **exported** assignment row — the shipped artifact the client reads, not the in-process
/// `SourceYield`.
fn exported_row(app: &App, band: bevy::prelude::Entity) -> sim_runtime::LaborAssignmentState {
    app.world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot
        .populations
        .iter()
        .find(|p| p.entity == band.to_bits())
        .expect("the hunting band is in the snapshot")
        .labor_assignments
        .first()
        .expect("its one Hunt assignment is exported")
        .clone()
}

/// One live take off a clone of `herd`, at one per-event seed — the take path's own function, so this
/// file never re-derives the number it is asserting about.
fn take_at(app: &App, herd: &Herd, seed: u64) -> core_sim::HuntOutcome {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();

    let combat = app.world.resource::<CombatConfigHandle>().get();
    // **THE HERD AS THE TURN THE FORECAST PRICES WILL FIND IT** — one Logistics regrowth on
    // (`core_sim::next_turns_quarry`). A pre-commit row is read after the Population take and
    // predicts the take *after the next* regrowth, so a comparison against the un-regrown herd
    // compares two turns and reads the growth as a drift.
    let mut quarry = core_sim::next_turns_quarry(herd, &fauna);
    hunt_take(
        &mut quarry,
        CREW,
        FOOD_PEAK,
        equipped_haul_rate(),
        &party_at(&combat),
        &fauna,
        NO_CARRY_LIMIT,
        HuntDraw::Seeded(seed),
    )
}

/// The herd as the world holds it.
fn herd_of(app: &App, id: &str) -> Herd {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the herd is on the map")
        .clone()
}

// ---------------------------------------------------------------------------------------------
// §6.4 — THE RANGE STOPS BEING DEGENERATE
// ---------------------------------------------------------------------------------------------

/// How many independent per-event seeds the containment sweep draws. Large enough that a coverage
/// fraction means something.
const CONTAINMENT_SEEDS: u32 = 400;

/// **The share of live takes a `±2σ` band must contain.** A normal-approximated binomial's `2σ` band
/// is ~95%, and the whole-animal `floor()` shifts a *discrete* band's coverage either way, so the
/// floor sits below the nominal figure deliberately — asserting 95% of a quantised binomial is the
/// flaky-by-construction shape §6.3 warns about. It is still far above what a **broken** range would
/// reach: a point estimate contains roughly one draw in four hundred.
const RANGE_COVERAGE_FLOOR: f64 = 0.85;

/// **THE EXPORTED RANGE CONTAINS THE TAKE, AT AN AUTHORED WARINESS** — §6.4's contract exercised for
/// real rather than degenerately.
///
/// Everything before slice 7 could only pin this on a **test-local** stochastic term
/// (`hunt_forecast_range` drives `hit_chance` down to `0.5`), because the roster shipped
/// `wariness 0` and the band was a point everywhere. This is the same assertion on the *shipped*
/// config: a Rabbit Warren's authored `0.75` is the only stochastic term in play, and the band the
/// **client** reads has to contain the take the sim pays.
///
/// Three liveness assertions ride with it, because a containment check alone is the weakest shape
/// there is (§6.3): the band is strictly wider than a point (a `[0, ∞]` band contains everything),
/// the takes genuinely vary across seeds (a constant sits inside any band), and their mean lands on
/// the reported `likely` (which is the half of the restated invariant containment cannot see).
#[test]
fn the_exported_range_contains_the_take_across_many_seeds_at_the_authored_wariness() {
    let (mut app, id, pos) = headless_with_species(FRAIL_WARY_SPECIES);
    reveal(&mut app, pos);
    let band = spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    seed_the_forecast(&mut app, band, &id, FOOD_PEAK);
    recapture_snapshot_in_place(&mut app.world);
    let reported = exported_row(&app, band);

    // **Liveness 1 — the band is not a point.** This is the assertion slice 7 exists to make true:
    // before it, every source on both webs reported `low == likely == high`.
    assert!(
        reported.actual_yield_low < reported.actual_yield
            && reported.actual_yield < reported.actual_yield_high,
        "an authored wariness must widen the EXPORTED band: {reported:?}"
    );

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let herd = herd_of(&app, &id);
    let hunt_yield = herd_hunt_yield(&herd, &fauna);
    let takes: Vec<f32> = (0..CONTAINMENT_SEEDS)
        .map(|seed| {
            let outcome = take_at(&app, &herd, u64::from(seed));
            hunt_yield
                .apply(outcome.take.carried, CONTENT_BAND_OUTPUT_MULTIPLIER)
                .provisions
        })
        .collect();

    // **Liveness 2 — the takes actually vary.** A dead retreat stage would make every draw identical,
    // and a constant sits inside any band; the spread is what makes containment mean something.
    let spread = takes.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        - takes.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        spread > 0.0,
        "the live take must genuinely vary across seeds, or containment is vacuous: {takes:?}"
    );

    let contained = takes
        .iter()
        .filter(|take| **take >= reported.actual_yield_low && **take <= reported.actual_yield_high)
        .count();
    let coverage = contained as f64 / takes.len() as f64;
    assert!(
        coverage >= RANGE_COVERAGE_FLOOR,
        "the reported band must contain the take it is a band around: {coverage:.3} of \
         {CONTAINMENT_SEEDS} seeds fell inside [{}, {}]",
        reported.actual_yield_low,
        reported.actual_yield_high
    );

    // **Liveness 3 — the reported `likely` is the take's EXPECTATION.** Compared against the band's
    // own half-width, so the tolerance is a property of the distribution rather than a hand-picked
    // number.
    let mean = takes.iter().sum::<f32>() / takes.len() as f32;
    let half_width = (reported.actual_yield_high - reported.actual_yield_low) / 2.0;
    assert!(
        (mean - reported.actual_yield).abs() <= half_width,
        "the mean take {mean} must track the reported likely {} (band half-width {half_width})",
        reported.actual_yield
    );
}

// ---------------------------------------------------------------------------------------------
// §3 — A WARY HERD COSTS HUNTER-TURNS, NEVER HERD BIOMASS
// ---------------------------------------------------------------------------------------------

/// **The slack an identity over [`HUNTED_TURNS`] accumulated subtractions is allowed.** The herd's
/// biomass is decremented once per turn and the comparison is a single multiplication, so the two
/// sides accrue different rounding: one relative `f32::EPSILON` per accumulated step, with a factor
/// of four of headroom. It is ~five orders of magnitude below one animal's `body_mass`, so it cannot
/// hide a miscounted body — which is the only error this identity exists to catch.
fn ledger_tolerance(magnitude: f32) -> f32 {
    magnitude.abs().max(1.0) * f32::EPSILON * HUNTED_TURNS as f32 * 4.0
}

/// Turns each of the two runs below is driven for — long enough that the retreat's cost accumulates
/// into a difference no single draw could produce.
const HUNTED_TURNS: u64 = 30;

/// **ESCAPED ANIMALS ARE NOT DEAD, so the herd loses nothing for them** (§3) — the property that makes
/// wariness a different lever from the escapement floor.
///
/// Drives the same crew over [`HUNTED_TURNS`] against the same herd at the shipped wariness, composing
/// each turn's seed the way `advance_labor_allocation` does (`fauna::retreat_seed` over
/// `(map_seed, tick, herd, party)`), and asserts two things at once:
///
/// - **the ledger balances on the KILL** — the herd's whole biomass loss is exactly the animals put
///   down times their body mass, so not one unit of it is attributable to an animal that merely fled;
/// - **the cost is HUNTER-TURNS** — the same crew, the same herd and the same turns yield strictly
///   less than at wariness `0`, and needs strictly more turns to bring home what the unwary run
///   brought home.
///
/// The liveness half is `fled > 0`: a run where nothing ever broke off satisfies the biomass identity
/// trivially, and would still pass if the retreat stage had been deleted.
#[test]
fn a_wary_herd_costs_hunter_turns_and_never_herd_biomass() {
    let (mut app, id, _pos) = headless_with_species(FRAIL_WARY_SPECIES);
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;

    // One run of `turns` against a fresh copy of the herd, reporting what the party took and what the
    // herd lost. `hunt_take` mutates the herd, so the copy is the ledger.
    let run = |app: &App, turns: u64| {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();

        let combat = app.world.resource::<CombatConfigHandle>().get();
        let mut herd = herd_of(app, &id);
        let herd_id = herd.id.clone();
        let opening_biomass = herd.biomass;
        let (mut engaged, mut fled, mut killed, mut carried) = (0.0_f32, 0.0_f32, 0_u32, 0.0_f32);
        for tick in 1..=turns {
            let outcome = hunt_take(
                &mut herd,
                CREW,
                FOOD_PEAK,
                equipped_haul_rate(),
                &party_at(&combat),
                &fauna,
                NO_CARRY_LIMIT,
                HuntDraw::Seeded(retreat_seed(map_seed, tick, &herd_id, CREW)),
            );
            engaged += outcome.engaged;
            fled += outcome.fled;
            killed += outcome.take.killed;
            carried += outcome.take.carried;
        }
        (
            engaged,
            fled,
            killed,
            carried,
            opening_biomass - herd.biomass,
        )
    };

    let (engaged, fled, killed, carried, biomass_lost) = run(&app, HUNTED_TURNS);
    let body_mass = herd_of(&app, &id).body_mass;

    // Liveness: animals genuinely broke off, and the party genuinely hunted.
    assert!(
        fled > 0.0 && killed > 0,
        "liveness: a wary quarry must both flee and be caught (fled {fled}, killed {killed})"
    );
    assert!(
        engaged > fled,
        "liveness: the party must engage more than fled, or nothing was ever fought \
         (engaged {engaged}, fled {fled})"
    );

    // **The ledger balances on the KILL.** Every unit the herd lost is an animal that died; the ones
    // that broke off cost it nothing.
    let killed_biomass = killed as f32 * body_mass;
    assert!(
        (biomass_lost - killed_biomass).abs() <= ledger_tolerance(killed_biomass),
        "the herd must lose exactly what was KILLED ({killed} × {body_mass} = {killed_biomass}), \
         never what was engaged ({engaged}): it lost {biomass_lost}"
    );

    // **The cost is hunter-turns.** The unwary twin is the same run with the one field zeroed.
    set_wariness(&mut app, FRAIL_WARY_SPECIES, NO_RETREAT);
    let (unwary_engaged, unwary_fled, _, unwary_carried, _) = run(&app, HUNTED_TURNS);
    assert_eq!(
        unwary_fled, 0.0,
        "the zero-wariness identity: nothing may break off"
    );
    assert!(
        (unwary_engaged - engaged).abs() <= ledger_tolerance(engaged),
        "all else IS equal: the same crew reaches the same animals either way \
         ({engaged} vs {unwary_engaged})"
    );
    assert!(
        carried < unwary_carried,
        "a wary herd must yield less to the same crew over the same turns: {carried} vs \
         {unwary_carried}"
    );
}

// ---------------------------------------------------------------------------------------------
// §3.1 — WARINESS ORDERS THE ROSTER
// ---------------------------------------------------------------------------------------------

/// The display names of §3.1's two anchors. Their **authored values** are read out of the shipped
/// roster rather than restated here, so this test measures the table that ships: flatten the roster
/// and the two ends coincide, which the liveness assertion below catches.
const LEAST_WARY_DISPLAY: &str = "Thunder Mammoths";
const MOST_WARY_DISPLAY: &str = "Desert Gazelle";

/// Seeds the ordering sweep averages over. The retreat is binomial, so a single draw can invert the
/// ordering between two nearby values; the claim is about the distributions.
const ORDERING_SEEDS: u32 = 200;

/// **A WARIER QUARRY YIELDS LESS TO THE SAME CREW, ALL ELSE EQUAL** (§3.1).
///
/// "All else equal" is made a fact rather than a hope: both runs hunt the *same species* on the *same
/// herd* with the *same crew* at the *same seeds*, and the only thing that changes between them is
/// `combat.wariness`. Comparing two different roster rows would have confounded the claim with
/// `engage_rate`, `defense`, `durability` and `body_mass` all at once.
///
/// Paired with liveness on **both** sides — a pair of zero takes satisfies any strict ordering, and a
/// dead retreat stage would make the two runs identical rather than ordered.
#[test]
fn a_warier_quarry_yields_less_to_the_same_crew() {
    let (mut app, id, _pos) = headless_with_species(FRAIL_WARY_SPECIES);

    let mean_take = |app: &App| {
        let herd = herd_of(app, &id);
        let total: f32 = (0..ORDERING_SEEDS)
            .map(|seed| take_at(app, &herd, u64::from(seed)).take.carried)
            .sum();
        total / ORDERING_SEEDS as f32
    };

    let (bold_wariness, wary_wariness) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            fauna.wariness_for(LEAST_WARY_DISPLAY),
            fauna.wariness_for(MOST_WARY_DISPLAY),
        )
    };
    set_wariness(&mut app, FRAIL_WARY_SPECIES, bold_wariness);
    let bold = mean_take(&app);
    set_wariness(&mut app, FRAIL_WARY_SPECIES, wary_wariness);
    let wary = mean_take(&app);

    // Liveness: neither end is a dead take, so the ordering is between two real numbers.
    assert!(
        bold > 0.0 && wary > 0.0,
        "liveness: both ends must actually hunt (bold {bold}, wary {wary}) — two zeroes satisfy \
         any ordering"
    );
    assert!(
        wary < bold,
        "the warier quarry must yield less to the same crew: {wary} at {wary_wariness} vs \
         {bold} at {bold_wariness}"
    );
}

// ---------------------------------------------------------------------------------------------
// §3 — THE ZERO IDENTITY IS STILL AN IDENTITY
// ---------------------------------------------------------------------------------------------

/// **THE DEGENERATE PATH STILL WORKS, AND NO SEED CAN MOVE IT.**
///
/// No roster row ships `0` any more (`the_authored_ordering_is_the_one_the_design_states` pins that),
/// so this code path is now reachable only through config — which is exactly why it needs its own
/// test: it is what every *other* suite in the workspace installs to stay deterministic
/// (`FaunaConfig::without_retreat`), and a rot in it would show up as a hundred unexplained failures
/// rather than as one.
///
/// Asserted at both ends of the seam: the take is bit-identical at every seed, **and** the exported
/// band collapses back to a point. The paired liveness is the wary run's spread, so "identical
/// everywhere" cannot pass because the take was dead.
#[test]
fn a_zero_wariness_species_is_an_exact_identity_at_every_seed() {
    let (mut app, id, pos) = headless_with_species(FRAIL_WARY_SPECIES);
    reveal(&mut app, pos);

    // Liveness first, on the shipped value: the seeds under test genuinely disagree before the field
    // is zeroed, so the identity below is a property of `0` and not of this fixture.
    let wary_takes: Vec<u32> = (0..ORDERING_SEEDS)
        .map(|seed| {
            take_at(&app, &herd_of(&app, &id), u64::from(seed))
                .take
                .killed
        })
        .collect();
    assert!(
        wary_takes.iter().any(|k| *k != wary_takes[0]),
        "liveness: the authored wariness must make the seed matter, or the identity below is vacuous"
    );

    set_wariness(&mut app, FRAIL_WARY_SPECIES, NO_RETREAT);
    let herd = herd_of(&app, &id);
    let baseline = take_at(&app, &herd, 0);
    assert!(
        baseline.take.killed > 0,
        "liveness: the identity take is real"
    );
    assert_eq!(baseline.fled, 0.0, "wariness 0 lets nothing break off");
    for seed in [1_u64, 7, 999, u64::MAX] {
        let outcome = take_at(&app, &herd, seed);
        assert_eq!(
            outcome.take, baseline.take,
            "seed {seed} moved a wariness-0 take — the retreat must make no draw at all"
        );
    }

    // …and the readout collapses with it: a degenerate distribution is a point on the wire.
    let band = spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    seed_the_forecast(&mut app, band, &id, FOOD_PEAK);
    recapture_snapshot_in_place(&mut app.world);
    let reported = exported_row(&app, band);
    assert_eq!(
        (reported.actual_yield_low, reported.actual_yield_high),
        (reported.actual_yield, reported.actual_yield),
        "with nothing stochastic left the exported band must be a point, bit-for-bit: {reported:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// §1 + §6.4 — THE FORECAST IS THE TAKE **WHERE THE ESCAPEMENT FLOOR BINDS**
// ---------------------------------------------------------------------------------------------

/// **The room the binding-regime fixture leaves standing, in whole animals.** Far below the crew's
/// reach ([`CREW`] × the warren's `engage_rate 10` = 200 rabbits) so the herd — not the party —
/// decides how many animals are engaged, and far above one body so the take is a real number rather
/// than a wait turn. Every other fixture in this file stands at [`TEST_CAPACITY`], where the room is
/// thousands of animals and this regime is unreachable.
const BINDING_ROOM_ANIMALS: f32 = 37.0;

/// **Stand the herd `room` whole animals above its [`FOOD_PEAK`] floor** — the ordinary steady state
/// of a worked herd, and the one regime the rest of this file deliberately excludes.
///
/// `biomass_before_regrowth` moves with `biomass` because they describe the same herd; only the MSY
/// reference line reads it, and a stale value there would report a `sustainable` for a herd that no
/// longer exists.
fn stand_at_the_floor(app: &mut App, id: &str, room_animals: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("the herd is on the map");
    let standing = FOOD_PEAK * TEST_CAPACITY + room_animals * herd.body_mass;
    herd.biomass = standing;
    herd.biomass_before_regrowth = standing;
}

/// **THE EXPORTED FORECAST IS THE TAKE WHEN THE ESCAPEMENT ROOM — NOT THE PARTY — BINDS.**
///
/// `forecast == actual` is a claim about the wire, and the take path bounds its engagement by what
/// the herd can spare **before** the retreat runs (`docs/plan_hunt_through_combat.md` §1). While the
/// roster shipped `wariness 0` the forecast could skip that clamp for free, because the retreat was
/// the identity and the quantiser re-clamped the kill either way. With a real wariness the two paths
/// retreat **different populations**: the take keeps a fraction of the 37 animals the herd can spare,
/// a forecast that engaged the party's full 200 keeps a fraction of *that*, and the quantiser then
/// trims it back to the 37 — a four-fold over-promise on `actualYield`.
///
/// The sibling `the_exported_range_contains_the_take_across_many_seeds_at_the_authored_wariness`
/// cannot see this: it stands the herd at [`TEST_CAPACITY`] *so that the escapement floor never
/// binds*, which excludes exactly this regime. This is that assertion in it.
///
/// Three liveness checks ride along, because each headline assertion is otherwise satisfiable by a
/// dead fixture: the room really is below the party's reach (or nothing is being clamped), the live
/// take genuinely varies (or containment is vacuous), and the take is non-zero (or every bound holds
/// trivially).
#[test]
fn the_exported_forecast_is_the_take_when_the_escapement_floor_binds() {
    let (mut app, id, pos) = headless_with_species(FRAIL_WARY_SPECIES);
    stand_at_the_floor(&mut app, &id, BINDING_ROOM_ANIMALS);
    reveal(&mut app, pos);

    // **Liveness 1 — the regime is the binding one.** Read off the same helpers the take path uses,
    // so this cannot drift from the thing under test.
    let (room_animals, reach) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let herd = herd_of(&app, &id);
        let ceiling =
            hunt_escapement_ceiling(FOOD_PEAK, herd.biomass, herd_capacity(&herd, &fauna));
        (
            animals_affordable(ceiling, herd.body_mass),
            // A pure hunt builds nothing and holds nothing, so its whole crew reaches.
            animals_engaged(CREW, fauna.engage_rate_for(&herd.species)),
        )
    };
    assert!(
        room_animals >= 1.0 && room_animals < reach,
        "liveness: the herd must be able to spare fewer animals ({room_animals}) than the party can \
         reach ({reach}), or the clamp under test does nothing"
    );

    let band = spawn_hunters(&mut app, pos, &id, FOOD_PEAK);
    seed_the_forecast(&mut app, band, &id, FOOD_PEAK);
    recapture_snapshot_in_place(&mut app.world);
    let reported = exported_row(&app, band);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let herd = herd_of(&app, &id);
    let hunt_yield = herd_hunt_yield(&herd, &fauna);
    let takes: Vec<f32> = (0..CONTAINMENT_SEEDS)
        .map(|seed| {
            let outcome = take_at(&app, &herd, u64::from(seed));
            hunt_yield
                .apply(outcome.take.carried, CONTENT_BAND_OUTPUT_MULTIPLIER)
                .provisions
        })
        .collect();
    let highest = takes.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let lowest = takes.iter().cloned().fold(f32::INFINITY, f32::min);

    // **Liveness 2 and 3** — a real, varying take to be right about.
    assert!(
        lowest > 0.0,
        "liveness: every live take must be real, not a wait turn: {takes:?}"
    );
    assert!(
        highest > lowest,
        "liveness: the live take must genuinely vary across seeds, or containment is vacuous"
    );

    // **THE HEADLINE — the preview may not promise food no draw of the take ever pays.** The
    // over-quote this pins is systematic, not a tail: before the engagement clamp reached the
    // forecast, the reported figure was the whole escapement room while the live take was ~a quarter
    // of it, so it sat above *every* one of the seeds below.
    assert!(
        reported.actual_yield <= highest,
        "the exported forecast {} exceeds the BEST of {CONTAINMENT_SEEDS} live takes ({highest}) — \
         a promise the sim never pays at any draw",
        reported.actual_yield
    );

    // …and it is the take's EXPECTATION, not merely inside its span: compared against the band's own
    // half-width, so the tolerance is a property of the distribution rather than a chosen number.
    let mean = takes.iter().sum::<f32>() / takes.len() as f32;
    let half_width = (reported.actual_yield_high - reported.actual_yield_low) / 2.0;
    assert!(
        (mean - reported.actual_yield).abs() <= half_width,
        "the mean take {mean} must track the reported likely {} (band half-width {half_width})",
        reported.actual_yield
    );

    // **The exported RANGE still contains the take here too** — §6.4's contract holds in the regime
    // its own suite excluded, and it is the band the client draws.
    let contained = takes
        .iter()
        .filter(|take| **take >= reported.actual_yield_low && **take <= reported.actual_yield_high)
        .count();
    let coverage = contained as f64 / takes.len() as f64;
    assert!(
        coverage >= RANGE_COVERAGE_FLOOR,
        "the reported band must contain the take it is a band around: {coverage:.3} of \
         {CONTAINMENT_SEEDS} seeds fell inside [{}, {}]",
        reported.actual_yield_low,
        reported.actual_yield_high
    );
}
