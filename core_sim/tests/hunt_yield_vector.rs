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
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, CombatConfigHandle, CommandEventLog,
    CreaturesConfigHandle, CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory,
    FaunaConfig, FaunaConfigHandle, FloraConfigHandle, FogRevealLedger, FollowPolicy,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    HuntYield, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, TileRegistry,
    WellbeingConfigHandle, FOOD,
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
