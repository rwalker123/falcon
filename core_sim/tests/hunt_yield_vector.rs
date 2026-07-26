//! **Hunt yield = product × intensity** (`docs/plan_hunt_yield_model.md`, issue #337, phase B1).
//!
//! The policy decides HOW MUCH biomass comes home ([`core_sim::hunt_policy_rate`]); the species'
//! [`core_sim::HuntYield`] decides WHAT that biomass is worth. These two axes used to be welded
//! together in two places — a 4× trade bonus that only the third rung earned, and an `Eradicate`
//! that was defined to carry nothing home — and this file is the guard against either coming back.
//!
//! The load-bearing cases: an **inedible** species (the wolf) is paid entirely in pelts and exactly
//! zero food; a **defaulting** species' food is byte-identical to the pre-arc arithmetic; and
//! **every** rung, Eradicate included, is paid its species' vector.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_band_movement, advance_expeditions, advance_herds, advance_labor_allocation,
    build_headless_app, hunt_source_yield_preview, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world,
    CombatConfigHandle, CommandEventLog, CreaturesConfigHandle, CultureManager, Diet,
    DiscoveryProgressLedger, Expedition, ExpeditionMission, ExpeditionPhase, FactionId,
    FactionInventory, FaunaConfig, FaunaConfigHandle, FloraConfigHandle, FogRevealLedger,
    FollowPolicy, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, HuntYield, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    ResidentBand, SimulationConfig, SimulationTick, SnapshotHistory, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
};

/// The four **extractive** rungs — the intensity ladder. Every one of them must pay the species'
/// product vector; none of them may change *what* the take is worth.
const EXTRACTIVE: [FollowPolicy; 4] = [
    FollowPolicy::Sustain,
    FollowPolicy::Surplus,
    FollowPolicy::Deplete,
    FollowPolicy::Eradicate,
];

/// A herd big enough that every rung's rate clears a whole body, so no test is measuring a
/// wait turn. Well above both rosters' `biomass[1]`, deliberately — this is a *yield* harness, not
/// an ecology one.
const TEST_CAPACITY: f32 = 4000.0;

/// A crew large enough that `collection = workers × per_worker_biomass_capacity` never binds, so
/// `carried == killed` and the food arithmetic is exactly `killed_biomass × provisions_per_biomass`.
/// (`quantise_animal_take` caps the kill by the crew's carry, which would otherwise smuggle a
/// second variable into the pinned number.)
const UNBOUNDED_CREW: u32 = 60;

/// A crew big enough to **carry the whole herd home in one turn** (`workers ×
/// per_worker_biomass_capacity 40 >= TEST_CAPACITY`). `quantise_animal_take` caps the kill by what
/// the crew can collect, so proving Eradicate empties a herd needs a crew that could haul it —
/// otherwise the test would be measuring the carry cap, not the policy.
const HAUL_THE_WHOLE_HERD_CREW: u32 = 100;

/// Float slop for a take reconstructed from a biomass delta through an `f32` rate.
const YIELD_EPSILON: f32 = 1e-3;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CombatConfigHandle::default());
    app.world.insert_resource(CreaturesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// Re-shape the map's first short-range game herd into `display_name` at a healthy, whole-animal
/// capacity, and hand back its id + tile position. Re-speciating an existing herd (rather than
/// spawning one) keeps the placement/graze plumbing real while pinning the one variable under test:
/// which row of `fauna_config.json` the take resolves its yield vector from.
fn reshape_first_herd(app: &mut App, display_name: &str) -> (String, UVec2) {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
            .map(|h| h.id.clone())
            .expect("expected short-range game on the map")
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
        herd.carrying_capacity = TEST_CAPACITY;
        herd.biomass = TEST_CAPACITY;
        // The take sizes Sustain's rate against the PRE-regrowth biomass; without `advance_herds`
        // having run this turn that field is stale, and every rate would read 0.
        herd.biomass_before_regrowth = TEST_CAPACITY;
        herd.hunt_credit = 0.0;
        herd.position()
    };
    (id, pos)
}

/// Spawn a content band (morale 1 ⇒ output multiplier 1.0) hunting `fauna_id` under `policy`.
fn spawn_hunters(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    policy: FollowPolicy,
    workers: u32,
) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32(workers as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
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
                        policy,
                    },
                    workers,
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn larder(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

fn trade_goods(app: &App) -> i64 {
    app.world
        .resource::<FactionInventory>()
        .stockpile(FactionId(0))
        .and_then(|m| m.get("trade_goods"))
        .copied()
        .unwrap_or(0)
}

fn herd_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
        .unwrap_or(0.0)
}

/// One hunting turn on a re-shaped herd: `(food_banked, trade_banked, biomass_killed)`.
fn hunt_one_turn(display_name: &str, policy: FollowPolicy, workers: u32) -> (f32, i64, f32) {
    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, display_name);
    let band = spawn_hunters(&mut app, pos, &id, policy, workers);
    let before = herd_biomass(&app, &id);
    app.world.run_system_once(advance_labor_allocation);
    (
        larder(&app, band),
        trade_goods(&app),
        before - herd_biomass(&app, &id),
    )
}

/// **1. A wolf hunt credits pelts and EXACTLY zero food — on every rung.**
///
/// The first `edible = false` species. `provisions_per_biomass` is an explicit `0.0`, which is a
/// real configured value ("you do not eat me"), not an unset one — so the food component is zero on
/// every intensity, while the trade component is strictly positive on every intensity. That is the
/// product/intensity split stated as an assertion.
#[test]
fn a_wolf_hunt_credits_pelts_and_exactly_zero_food_on_every_rung() {
    for policy in EXTRACTIVE {
        let (food, trade, killed) = hunt_one_turn("Grey Wolf Pack", policy, UNBOUNDED_CREW);
        assert!(
            killed > 0.0,
            "{policy:?}: the harness must actually take something, got {killed} biomass"
        );
        assert_eq!(
            food, 0.0,
            "{policy:?}: a wolf is not food — the larder must not move (killed {killed})"
        );
        assert!(
            trade > 0,
            "{policy:?}: a wolf is a pelt — trade goods must be credited (killed {killed})"
        );
    }
}

/// **2. A deer hunt credits meat AND hide under Sustain** — the rebalance.
///
/// Before this arc only the third rung sold anything (it alone carried the retired 4×
/// `market.trade_goods_multiplier`); Sustain and Surplus produced no trade goods at all. Now every
/// harvesting policy sells the species' trade component, so a restrained hunt earns hides too.
#[test]
fn a_deer_hunt_credits_meat_and_hide_under_sustain() {
    let (food, trade, killed) = hunt_one_turn("Red Deer", FollowPolicy::Sustain, UNBOUNDED_CREW);
    assert!(killed > 0.0, "the harness must take something");
    assert!(
        food > 0.0,
        "a Sustain deer hunt still feeds the band: {food}"
    );
    assert!(
        trade > 0,
        "a Sustain deer hunt now sells hides too (it earned nothing before #337): {trade}"
    );
}

/// **3. FOOD is byte-identical to pre-arc for a species that omits the block.**
///
/// The whole reason `HuntYieldDef`'s components are `Option<f32>` rather than floats: an omitted
/// block falls back to the `hunt.*` global, so a deer's take pays *literally* what it always paid.
/// Pinned against the arithmetic itself — `killed_biomass × hunt.provisions_per_biomass × mult` —
/// rather than against a captured constant, so it still reads as the pre-arc formula.
///
/// The crew is deliberately large enough that `carried == killed` (see [`UNBOUNDED_CREW`]),
/// otherwise the collection cap would put a second variable in the pinned number.
#[test]
fn food_is_byte_identical_to_pre_arc_for_a_defaulting_species() {
    for policy in EXTRACTIVE {
        let (food, _, killed) = hunt_one_turn("Red Deer", policy, UNBOUNDED_CREW);
        let global_rate = FaunaConfig::builtin().hunt.provisions_per_biomass;
        let pre_arc = killed * global_rate;
        assert!(
            (food - pre_arc).abs() <= YIELD_EPSILON,
            "{policy:?}: a defaulting species must pay the OLD global arithmetic: {food} vs \
             {killed} × {global_rate} = {pre_arc}"
        );
    }
}

/// **4. Eradicate pays the windfall, and the herd is gone afterwards.**
///
/// `delivers_food` is retired: denial is the END STATE (the species is gone, for you and everyone
/// else), never a promise that the party threw the carcasses away. Eradicate takes the whole
/// standing stock, so it must yield *strictly more* food than the same herd's Sustain skim — and
/// still leave nothing behind.
#[test]
fn eradicate_pays_a_windfall_and_still_ends_the_herd() {
    let (sustain_food, _, sustain_killed) =
        hunt_one_turn("Red Deer", FollowPolicy::Sustain, HAUL_THE_WHOLE_HERD_CREW);

    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, "Red Deer");
    let band = spawn_hunters(
        &mut app,
        pos,
        &id,
        FollowPolicy::Eradicate,
        HAUL_THE_WHOLE_HERD_CREW,
    );
    app.world.run_system_once(advance_labor_allocation);
    let eradicate_food = larder(&app, band);

    assert!(
        eradicate_food > sustain_food,
        "Eradicate takes the whole stock, so it must out-feed the Sustain skim: {eradicate_food} \
         vs {sustain_food} (Sustain killed {sustain_killed})"
    );
    assert!(
        trade_goods(&app) > 0,
        "denial sells its hides too — every rung is paid the species' vector"
    );
    let left = herd_biomass(&app, &id);
    assert!(
        left < app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .species_by_display("Red Deer")
            .expect("deer in the roster")
            .body_mass,
        "Eradicate must leave less than one whole animal standing, got {left}"
    );
}

/// **5. The larder ledger still closes for a wolf hunt — trade is NOT food income.**
///
/// `foodIncome` is `Σ SourceYield::actual`, and the identity
/// `larder_delta == food_income − food_consumption − pen_feed_upkeep` is what makes the band's food
/// panel honest. A hunt that now credits a *second* currency must not leak it into that sum: a
/// wolf's take contributes `0` to `food_income` while filling the faction's trade stockpile. Run
/// with only the labor system, so consumption and pen feed are both `0` and the identity reduces to
/// `larder_delta == Σ actual`.
#[test]
fn the_larder_ledger_excludes_trade_goods_for_a_wolf_hunt() {
    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, "Grey Wolf Pack");
    let band = spawn_hunters(&mut app, pos, &id, FollowPolicy::Deplete, UNBOUNDED_CREW);
    let before = larder(&app, band);

    app.world.run_system_once(advance_labor_allocation);

    let food_income: f32 = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_yields
        .iter()
        .map(|y| y.actual)
        .sum();
    let larder_delta = larder(&app, band) - before;

    assert_eq!(
        food_income, 0.0,
        "a wolf's take contributes NOTHING to food income — it is not food"
    );
    assert!(
        (larder_delta - food_income).abs() <= YIELD_EPSILON,
        "larder_delta must equal food_income with no consumption and no pen: {larder_delta} vs \
         {food_income}"
    );
    assert!(
        trade_goods(&app) > 0,
        "…while the same take really did earn trade goods, so this is not a no-op hunt"
    );
}

/// **6. A `yields_nothing` species offers Eradicate ALONE.**
///
/// The only pruning rule in the picker: a pure pest, worth neither meat nor pelt, has no meaningful
/// *rate* at which to collect nothing — the one coherent verb left is *make it stop*. No shipped
/// species is this today (the wolf trades), so it is pinned on a synthetic config, and it is
/// asserted through [`core_sim::hunt_policies_for`] — the single seam the `assign_labor` validator
/// and the snapshot's exported `huntPolicyCeilings` both read, so the two can never become two
/// lists that disagree.
#[test]
fn a_yields_nothing_species_offers_eradicate_only() {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_FAUNA_CONFIG).expect("the builtin parses");
    json["species"]["deer"]["hunt_yield"] =
        serde_json::json!({ "provisions_per_biomass": 0.0, "trade_goods_per_biomass": 0.0 });
    let config =
        FaunaConfig::from_json_str(&json.to_string()).expect("a zero vector is a legal config");

    let pest = config.hunt_yield_for("Red Deer");
    assert!(pest.yields_nothing(), "the synthetic deer yields nothing");
    assert_eq!(
        core_sim::hunt_policies_for(pest),
        &[FollowPolicy::Eradicate],
        "a worthless quarry offers denial and nothing else"
    );

    // …and the flags gate the yield COMPONENTS, not the buttons: an inedible-but-tradeable species
    // keeps the FULL ladder, because each rung is a meaningful rate at which to collect pelts.
    let wolf = config.hunt_yield_for("Grey Wolf Pack");
    assert!(!wolf.edible() && wolf.tradeable());
    assert_eq!(
        core_sim::hunt_policies_for(wolf),
        &FollowPolicy::HUNT_POLICIES,
        "a wolf shows the whole ladder and is paid in pelts"
    );
}

/// **The derived flags are a comparison against the vector, never a stored second copy** — the
/// property that keeps "is it food?" from drifting away from "what does it pay?".
#[test]
fn edible_and_tradeable_are_derived_from_the_vector() {
    let both = HuntYield {
        provisions_per_biomass: 0.02,
        trade_goods_per_biomass: 0.005,
    };
    assert!(both.edible() && both.tradeable() && !both.yields_nothing());

    let pelt_only = HuntYield {
        provisions_per_biomass: 0.0,
        trade_goods_per_biomass: 0.02,
    };
    assert!(!pelt_only.edible() && pelt_only.tradeable() && !pelt_only.yields_nothing());

    let pest = HuntYield {
        provisions_per_biomass: 0.0,
        trade_goods_per_biomass: 0.0,
    };
    assert!(!pest.edible() && !pest.tradeable() && pest.yields_nothing());
}

// ---------------------------------------------------------------------------------------------------
// B2 — the forecast and the wire
// ---------------------------------------------------------------------------------------------------

/// The two shipped species this file contrasts: one that omits `hunt_yield` (so both components fall
/// back to the globals) and the one that declares an inedible, pelt-bearing vector.
const DEFAULTING_SPECIES: &str = "Red Deer";
const INEDIBLE_SPECIES: &str = "Grey Wolf Pack";

/// A **full headless app** (the real turn pipeline + the real snapshot capture) with its first game
/// herd re-shaped into `display_name` at [`TEST_CAPACITY`], returning `(app, herd_id, herd_tile)`.
///
/// The B1 tests run on a light harness; these need the *shipped representation*, so they go through
/// `build_headless_app` + `recapture_snapshot_in_place` — the same path
/// `expedition_hunt::exported_snapshot_fields_reproduce_band_hunt_take` uses.
fn headless_with_species(display_name: &str) -> (App, String, UVec2) {
    let mut app = build_headless_app();
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
        // Freeze the range-derived K so the forecast and the take see the same capacity: this file
        // measures the YIELD VECTOR, not the grazing loop.
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = TEST_CAPACITY;
        herd.biomass = TEST_CAPACITY;
        herd.biomass_before_regrowth = TEST_CAPACITY;
        herd.hunt_credit = 0.0;
        herd.position()
    };
    (app, id, pos)
}

/// **Mark the herd's tile `Active` for the viewer faction.** `WorldSnapshot.herds` is fog-filtered
/// (see "Herd display telemetry is FOG-FILTERED" in the fauna rules), so an unwatched herd is simply
/// absent from the shipped snapshot — these tests read exported per-herd rows, so they must first put
/// eyes on it. The `expedition_hunt::reveal_herds` pattern.
fn reveal_herd(app: &mut App, id: &str) {
    let pos = {
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(id).map(|herd| herd.position())
    };
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    if let Some(pos) = pos {
        map.mark_active(pos.x, pos.y, 0);
    }
}

/// A resident band of `workers` hunting `fauna_id` under `policy`, standing on the herd's tile.
fn spawn_resident_hunters(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    policy: FollowPolicy,
    workers: u32,
) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32(workers as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
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
            ResidentBand,
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        policy,
                    },
                    workers,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// The **pre-commit forecast** for this herd/staffing/policy — the assign-time seed
/// (`hunt_source_yield_preview`), i.e. the pair the client is shown *before* committing. Read before
/// the turn resolves.
fn precommit_pair(app: &App, id: &str, policy: FollowPolicy, workers: u32) -> (f32, f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("the herd is on the map");
    let seed = hunt_source_yield_preview(
        herd,
        &fauna,
        &ladder,
        labor.hunt.per_worker_biomass_capacity,
        FORECAST_OUTPUT_MULTIPLIER,
        workers,
        policy,
        labor.yield_average_horizon_turns,
        labor.arrivals_horizon_turns,
    );
    (seed.actual, seed.trade)
}

/// A content band's output multiplier — morale `1.0`, so the forecast and the take share it.
const FORECAST_OUTPUT_MULTIPLIER: f32 = 1.0;

/// The **exported** `(actualYield, tradeYield)` for a band's single assignment, read off the shipped
/// snapshot rather than the in-process `SourceYield`.
fn exported_pair(app: &App, band: bevy::prelude::Entity) -> (f32, f32) {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let cohort = snapshot
        .populations
        .iter()
        .find(|p| p.entity == band.to_bits())
        .expect("the hunting band is in the snapshot");
    let row = cohort
        .labor_assignments
        .first()
        .expect("its one Hunt assignment is exported");
    (row.actual_yield, row.trade_yield)
}

/// The **exported** per-policy ceiling rows for a herd, as `(policy, provisions, trade)`.
fn exported_ceilings(app: &App, id: &str) -> Vec<(String, f32, f32)> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is in the snapshot");
    herd.hunt_policy_ceilings
        .iter()
        .map(|row| {
            (
                row.policy.clone(),
                row.provisions_per_turn,
                row.trade_goods_per_turn,
            )
        })
        .collect()
}

/// **7. `forecast == actual` for BOTH products, for a wolf and for a deer — ON THE WIRE.**
///
/// **The arc's load-bearing test.** The pre-commit forecast, the assign-time seed and the resolved
/// take all read the same helpers, so what the client is shown before committing must be exactly what
/// the sim then pays — *per component*. A food-only forecast could not even state a wolf's yield (all
/// its food ceilings are `0`), which is precisely why `SourceYieldForecast` is a `YieldPair` rather
/// than a scalar with a bolted-on sibling: two halves can drift, one pair cannot.
///
/// Asserted on the **exported snapshot** (`laborAssignments[].actualYield` / `.tradeYield`), not on
/// the in-process struct, so it pins the shipped representation — an export that projected the wrong
/// component would pass an in-memory check and still lie to the client.
#[test]
fn the_forecast_equals_the_paid_take_in_both_products_on_the_wire() {
    for species in [DEFAULTING_SPECIES, INEDIBLE_SPECIES] {
        for policy in EXTRACTIVE {
            let (mut app, id, pos) = headless_with_species(species);
            let forecast = precommit_pair(&app, &id, policy, UNBOUNDED_CREW);
            let band = spawn_resident_hunters(&mut app, pos, &id, policy, UNBOUNDED_CREW);

            app.world.run_system_once(advance_labor_allocation);
            recapture_snapshot_in_place(&mut app.world);
            let paid = exported_pair(&app, band);

            assert!(
                (forecast.0 - paid.0).abs() <= YIELD_EPSILON,
                "{species} {policy:?}: forecast FOOD {} must equal the paid {}",
                forecast.0,
                paid.0
            );
            assert!(
                (forecast.1 - paid.1).abs() <= YIELD_EPSILON,
                "{species} {policy:?}: forecast TRADE {} must equal the paid {}",
                forecast.1,
                paid.1
            );
            // …and the test must not be vacuously comparing two zeros: every rung of every species
            // here pays *something*, in one currency or the other.
            assert!(
                paid.0 > 0.0 || paid.1 > 0.0,
                "{species} {policy:?}: the harness must actually take something ({paid:?})"
            );
        }
    }
}

/// **8. A wolf's exported per-policy ceilings read ZERO food and NON-ZERO trade, on all four rungs.**
///
/// The wire-level statement of *product × intensity*: the rungs are a pressure ladder over one
/// species' vector, so an inedible quarry offers every rung — each a meaningful rate at which to
/// collect pelts — and every one of them is honestly `0` food. Also pins the trip-estimate flags,
/// which say the same thing for an expedition: "pelts, no meat", never "denial mission".
#[test]
fn a_wolves_exported_ceilings_read_no_food_and_real_trade_on_every_rung() {
    let (mut app, id, _pos) = headless_with_species(INEDIBLE_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);

    let rows = exported_ceilings(&app, &id);
    for policy in EXTRACTIVE {
        let key = policy.as_str();
        let (_, provisions, trade) = rows
            .iter()
            .find(|(name, _, _)| name == key)
            .unwrap_or_else(|| panic!("a wolf offers the full ladder — {key} row missing"));
        assert_eq!(
            *provisions, 0.0,
            "{key}: a wolf is not food — its exported food ceiling must be 0"
        );
        assert!(
            *trade > 0.0,
            "{key}: a wolf is a pelt — its exported trade ceiling must be positive, got {trade}"
        );
    }

    // The expedition side says the same thing with two flags rather than two numbers.
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is in the snapshot");
    assert!(
        !herd.hunt_trip_estimates.is_empty(),
        "a huntable herd exports trip estimates"
    );
    for row in &herd.hunt_trip_estimates {
        assert!(
            !row.delivers_food,
            "{}: a wolf trip brings home no food",
            row.policy
        );
        assert!(
            row.delivers_trade,
            "{}: …but it does bring home pelts — the flag that keeps it from reading as denial",
            row.policy
        );
    }
    // The per-herd, species-aware per-worker rates agree: no food, real trade. (The cohort-level
    // `huntPerWorkerProvisions` is species-blind by construction — see its doc — which is exactly why
    // a band preview must clamp with THESE.)
    assert_eq!(herd.per_worker_yield, 0.0, "no food per hunter on a wolf");
    assert!(herd.per_worker_trade > 0.0, "pelts per hunter on a wolf");
    assert_eq!(herd.food_per_animal, 0.0, "a wolf is worth no food");
    assert!(
        herd.trade_per_animal > 0.0,
        "…but one wolf is worth a pelt — the only quantum it has"
    );
}

/// **9. The Eradicate ceiling row is no longer zeroed** on the provisions side for an edible species.
///
/// It was zeroed on the premise that denial carries nothing home — the premise #337 reverses. The row
/// now carries the windfall the take path actually pays: the whole standing stock, so it must be the
/// **largest** rung on the ladder.
#[test]
fn the_eradicate_ceiling_carries_the_windfall_for_an_edible_species() {
    let (mut app, id, _pos) = headless_with_species(DEFAULTING_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);

    let rows = exported_ceilings(&app, &id);
    let ceiling_of = |policy: FollowPolicy| -> (f32, f32) {
        rows.iter()
            .find(|(name, _, _)| name == policy.as_str())
            .map(|(_, provisions, trade)| (*provisions, *trade))
            .unwrap_or_else(|| panic!("the {} row is exported", policy.as_str()))
    };

    let eradicate = ceiling_of(FollowPolicy::Eradicate);
    let deplete = ceiling_of(FollowPolicy::Deplete);
    assert!(
        eradicate.0 > 0.0,
        "Eradicate's exported FOOD ceiling is the windfall, not a zeroed denial row: {}",
        eradicate.0
    );
    assert!(
        eradicate.0 > deplete.0,
        "Eradicate takes the whole stock, so it must top the ladder: {} vs Deplete {}",
        eradicate.0,
        deplete.0
    );
    // Every rung sells, so the trade ladder is ordered the same way — one vector, one intensity axis.
    assert!(
        eradicate.1 > deplete.1 && deplete.1 > 0.0,
        "the trade ceilings ride the same intensity ladder: eradicate {} vs deplete {}",
        eradicate.1,
        deplete.1
    );
}

// ---------------------------------------------------------------------------------------------------
// B3 — the EXPEDITION arm: a raid PAYS both products it forecast
// ---------------------------------------------------------------------------------------------------

/// The reference raiding party (`≤ expedition_config.max_party_size`, so the pre-launch estimate
/// table carries a row for it).
const RAID_PARTY: u32 = 4;

/// How far the home band camps from the quarry, on the herd's own row. It must sit **outside** both
/// `hunt.drop_off_within_tiles` (3 — else a near-band early delivery would end the raid on a
/// different turn than the forecast, which does not model that gate) and `comm_range_tiles` (2 —
/// else the party would be "home" while still hunting). Same row so the party's Chebyshev
/// `step_toward` walk covers exactly the hex distance the phase machine measures.
const HOME_BAND_DISTANCE_TILES: u32 = 6;

/// Turn budget for one raid + the walk home: `hunt.forecast_horizon_turns` (60) plus the return leg
/// at `band_move_tiles_per_turn`. A raid that has not resolved by then is a hung test, not a slow one.
const MAX_RAID_TURNS: u32 = 90;

/// The forecast sums each turn's landed food as a raw `f32` while the party's pack accumulates on
/// the fixed-point `Scalar` grid, so a multi-turn raid can end a few quanta apart. Sized as the
/// sibling `expedition_hunt` guards are — a handful of `Scalar` quanta, not a free pass.
const RAID_FOOD_EPSILON: f32 = 8.0 / core_sim::Scalar::SCALE as f32;

/// Whole trade goods, the granularity the faction stockpile (an `i64` account) works in — the
/// rounding `settle_carried_trade` applies to a party's carried pelts when it delivers.
fn whole_trade_goods(carried: f32) -> i64 {
    carried.round() as i64
}

/// A detached party / camp cohort of `workers` standing on `tile`, content (morale 1 ⇒ output
/// multiplier 1.0, the multiplier the expedition path asserts it does *not* apply).
fn party_cohort(tile: bevy::prelude::Entity, workers: u32) -> PopulationCohort {
    PopulationCohort {
        home: tile,
        current_tile: tile,
        last_fertility_factors: Default::default(),
        size: workers,
        children: scalar_zero(),
        working: scalar_from_f32(workers as f32),
        elders: scalar_zero(),
        stores: LocalStore::new(),
        morale: scalar_one(),
        last_food_consumption: 0.0,
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
    }
}

/// **Hold everything about the quarry steady EXCEPT its yield vector** — install a fauna config in
/// which `display_name` neither fights back nor has a moving carrying capacity.
///
/// The sibling pin `expedition_hunt::the_raid_forecast_matches_a_real_party_run` gets both for free
/// by retagging its herd to a harmless grazer. This file **cannot** retag: the species *is* what is
/// under test (a wolf's whole point is that it is inedible), so it neutralises the two confounds on
/// the row instead. Neither touches [`core_sim::HuntYield`], the take, or the policy ladder.
///
/// - **`combat.attack = 0`** — `hunt_trip_forecast` deliberately does not model the hunt's
///   casualties (Predators Phase 0 left that to a later slice), so a quarry that fights back thins
///   the party mid-raid and the real run diverges from the forecast for a reason that has nothing to
///   do with the yield vector.
/// - **`diet = Herbivore`** — the *only* reason is to freeze `K`. `headless_with_species` already
///   freezes the range-derived capacity with `fodder_per_biomass = 0.0`, but that lever is on the
///   **graze** branch; a carnivore's `K` is recomputed every turn from the live prey base, which
///   both drifts under the forecast's fixed-`K` clone and (at `TEST_CAPACITY`) puts the herd under
///   the extinction floor of its own prey-derived `K`, despawning the quarry on turn one.
fn steady_quarry(app: &mut App, display_name: &str) {
    let mut config = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
    let key = config
        .species
        .iter()
        .find(|(_, def)| def.display_name == display_name)
        .map(|(key, _)| key.clone())
        .expect("the species is in the shipped roster");
    let def = config.species.get_mut(&key).expect("just resolved");
    def.combat.attack = 0.0;
    def.diet = Diet::Herbivore;
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// The wild-game reference growth rate the raid harness pins its quarry to — the same `r` the
/// sibling `expedition_hunt` raid tests use for their worked boar example.
const WILD_REGROWTH_RATE: f32 = 0.10;

/// Pin the two things about a quarry that decide **when a raid ends**, so the pin measures the yield
/// vector rather than whichever herd the map handed the fixture.
///
/// - **Route** → the tile it stands on, so the party stays in reach for the whole raid (the map
///   seeds roaming game, and `hunt_trip_forecast` assumes a stationary quarry — the
///   `expedition_hunt::pinned_game_herd` pattern).
/// - **`regrowth_rate`** → [`WILD_REGROWTH_RATE`]. `headless_with_species` re-speciates *whichever*
///   `game_` herd the registry lists first, and that herd's own `r` rides along; a raid ends when
///   the standing surplus is spent, so a faster-regrowing quarry keeps handing the party another
///   animal and never lets it come home. Stating `r` makes the trip length reproducible.
fn pin_quarry(app: &mut App, id: &str) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("the herd is on the map");
    herd.route = vec![herd.current_pos];
    herd.step_index = 0;
    herd.regrowth_rate = WILD_REGROWTH_RATE;
}

/// A resident home band `HOME_BAND_DISTANCE_TILES` along the herd's row — the larder a hunting
/// party's provisions fold into, and the tile it walks back to.
fn spawn_raid_home_band(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let width = app.world.resource::<TileRegistry>().width;
    let camp = UVec2::new((herd_pos.x + HOME_BAND_DISTANCE_TILES) % width, herd_pos.y);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(camp.x, camp.y)
        .expect("the camp tile resolves");
    app.world
        .spawn((party_cohort(tile, RAID_PARTY), ResidentBand))
        .id()
}

/// A `RAID_PARTY`-strong hunting party already in the `Hunting` phase on the herd's tile — as
/// `send_hunt_expedition` spawns it, minus the walk out.
fn spawn_raid_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    policy: FollowPolicy,
) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            party_cohort(tile, RAID_PARTY),
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.to_string(),
                    policy,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
            },
        ))
        .id()
}

/// The **exported** pre-launch raid promise for `(policy, RAID_PARTY)` — the very row the client's
/// "delivers ≈X over ≈N turns · ⇄ ~Y trade goods" line reads — as
/// `(turns_to_fill, delivered_food, delivered_trade)`.
fn exported_raid_promise(app: &App, id: &str, policy: FollowPolicy) -> (u32, f32, f32) {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is in the snapshot");
    let row = herd
        .hunt_trip_estimates
        .iter()
        .find(|row| row.policy == policy.as_str() && row.party_workers == RAID_PARTY)
        .unwrap_or_else(|| {
            panic!(
                "the herd exports a {} × {RAID_PARTY}-worker trip estimate",
                policy.as_str()
            )
        });
    (row.turns_to_fill, row.delivered_food, row.delivered_trade)
}

/// Run one raid to its first delivery and report `(food_landed_in_the_band_larder,
/// trade_goods_banked_by_the_faction)`.
///
/// Drives the **real** systems in the real order (`advance_herds` → `advance_band_movement` →
/// `advance_expeditions`) and stops on the party's first completed trip: either it folded back
/// (despawned) or — the `Deplete` relaunch — it dropped its load off and returned to `Hunting`.
fn run_one_raid(
    app: &mut App,
    party: bevy::prelude::Entity,
    home: bevy::prelude::Entity,
) -> (f32, i64) {
    let mut left_hunting = false;
    for _ in 0..MAX_RAID_TURNS {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_band_movement);
        app.world.run_system_once(advance_expeditions);
        let Some(expedition) = app.world.get::<Expedition>(party) else {
            break; // folded back into the band and despawned — the trip is over
        };
        if expedition.phase == ExpeditionPhase::Hunting {
            if left_hunting {
                break; // dropped its load off and relaunched — one trip's worth is banked
            }
        } else {
            left_hunting = true;
        }
    }
    (larder(app, home), trade_goods(app))
}

/// **10. A hunting EXPEDITION delivers BOTH products it promised — wolf and deer.**
///
/// The raid arm used to credit `FOOD ONLY` while its forecast already advertised `deliveredTrade`,
/// so the outfit UI promised pelts the sim never paid and a **wolf raid came home with literally
/// nothing** (`provisions_per_biomass == 0`). That is `forecast == actual` — Decision 8, the one
/// invariant this arc rests on — broken on the expedition path, and this is its guard.
///
/// Asserted against the **exported** `huntTripEstimates` row (the client's own pre-launch readout),
/// not an in-process forecast, and against the two accounts the sim really credits: the home band's
/// larder for provisions and the faction stockpile for trade goods.
#[test]
fn a_hunting_expedition_delivers_both_products_it_forecast() {
    for species in [DEFAULTING_SPECIES, INEDIBLE_SPECIES] {
        let mut raids_compared = 0;
        for policy in [
            FollowPolicy::Sustain,
            FollowPolicy::Surplus,
            FollowPolicy::Deplete,
        ] {
            let (mut app, id, pos) = headless_with_species(species);
            steady_quarry(&mut app, species);
            pin_quarry(&mut app, &id);
            reveal_herd(&mut app, &id);
            recapture_snapshot_in_place(&mut app.world);
            let (turns, promised_food, promised_trade) = exported_raid_promise(&app, &id, policy);
            let context = format!("{species} {policy:?} raid");
            // **`turnsToFill == 0` = "the raid never completes within `hunt.forecast_horizon_turns`"**
            // — a herd whose regrowth keeps handing the party another whole animal forever. Its
            // `deliveredFood`/`deliveredTrade` are then *"what the horizon saw"*, not *"what comes
            // home"*, because the party never comes home; the client shows such a row without an
            // ETA. `forecast == actual` is a statement about a trip that ENDS, so those rows are
            // out of this pin's scope (the coverage assertion below keeps that from hollowing it).
            if turns == 0 {
                continue;
            }
            raids_compared += 1;

            let home = spawn_raid_home_band(&mut app, pos);
            let party = spawn_raid_party(&mut app, home, pos, &id, policy);
            let (landed_food, banked_trade) = run_one_raid(&mut app, party, home);

            assert!(
                (landed_food - promised_food).abs() <= RAID_FOOD_EPSILON,
                "{context}: the band larder must receive the {promised_food} food the exported \
                 estimate promised, got {landed_food}"
            );
            assert_eq!(
                banked_trade,
                whole_trade_goods(promised_trade),
                "{context}: the faction must bank the {promised_trade} trade goods the exported \
                 estimate promised (rounded to whole goods)"
            );
            // …and the test must not be vacuously comparing zeros: a completing raid really pays.
            assert!(
                banked_trade > 0,
                "{context}: the harness must actually earn trade goods (promised {promised_trade})"
            );
        }
        assert!(
            raids_compared > 0,
            "{species}: at least one rung must produce a raid that comes home, or this test \
             asserts nothing at all"
        );
    }
}

/// **11. An INEDIBLE raid comes home with pelts and exactly zero food.**
///
/// The wolf case stated on its own, because it is the one the bug made visible: before the fix this
/// party returned with nothing at all, and the feed line called a full pack of pelts "EMPTY".
#[test]
fn an_inedible_raid_comes_home_with_pelts_and_no_food() {
    let (mut app, id, pos) = headless_with_species(INEDIBLE_SPECIES);
    steady_quarry(&mut app, INEDIBLE_SPECIES);
    pin_quarry(&mut app, &id);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);
    let (turns, promised_food, promised_trade) =
        exported_raid_promise(&app, &id, FollowPolicy::Sustain);
    assert_eq!(
        promised_food, 0.0,
        "a wolf is not food — the exported raid promise must be 0 provisions"
    );
    assert!(
        turns > 0,
        "…but it is still a real trip that COMES HOME: the raid must report an ETA, not the \
         zeroed projection an inedible quarry used to short-circuit to"
    );

    let home = spawn_raid_home_band(&mut app, pos);
    let party = spawn_raid_party(&mut app, home, pos, &id, FollowPolicy::Sustain);
    let (landed_food, banked_trade) = run_one_raid(&mut app, party, home);

    assert_eq!(
        landed_food, 0.0,
        "a wolf hunt adds nothing to the larder — the larder ledger must stay food-only"
    );
    assert!(
        banked_trade > 0,
        "…but the pelts DO come home: the raid promised {promised_trade} trade goods and banked \
         {banked_trade}"
    );
    assert_eq!(
        banked_trade,
        whole_trade_goods(promised_trade),
        "and it banks exactly what the outfit UI promised"
    );
}
