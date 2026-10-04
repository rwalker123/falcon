//! **Hunt yield = product × intensity** (`docs/plan_hunt_yield_model.md`, issue #337, phase B1).
//!
//! The stance decides HOW MUCH biomass comes home ([`core_sim::hunt_escapement_ceiling`]); the species'
//! [`core_sim::HuntYield`] decides WHAT that biomass is worth. These two axes used to be welded
//! together in two places — a 4× trade bonus that only the third rung earned, and an `Eradicate`
//! that was defined to carry nothing home — and this file is the guard against either coming back.
//!
//! The load-bearing cases: an **inedible** species (the wolf) is paid entirely in pelts and exactly
//! zero food; a **defaulting** species' food is byte-identical to the pre-arc arithmetic; and
//! **every** rung, Eradicate included, is paid its species' vector.

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
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, build_test_app, hunt_engage_workers, hunt_haul_workers,
    hunt_source_yield_preview, recapture_snapshot_in_place, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world,
    CombatConfigHandle, CommandEventLog, CreaturesConfigHandle, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfig, FaunaConfigHandle,
    FloraConfigHandle, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap,
    HerdRegistry, HerdTelemetry, HuntYield, HuntingParty, Improvement, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, SimulationConfig,
    SimulationTick, SnapshotHistory, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SourcePriority, StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
    TileRegistry, WellbeingConfigHandle, FOOD, MSY_BIOMASS_FRACTION, NO_IMPROVEMENT_UNDERWAY,
    STRIP_IT_BARE,
};

/// Four depths on the intensity dial. Every one of them must pay the species' product vector; none
/// of them may change *what* the take is worth.
const EXTRACTIVE: [f32; 4] = [0.5, 0.3, 0.15, 0.0];

/// A herd big enough that every rung's rate clears a whole body, so no test is measuring a
/// wait turn. Well above both rosters' `biomass[1]`, deliberately — this is a *yield* harness, not
/// an ecology one.
const TEST_CAPACITY: f32 = 4000.0;

/// A crew large enough that `collection = workers × per_worker_biomass_capacity` never binds, so
/// `carried == killed` and the food arithmetic is exactly `killed_biomass × provisions_per_biomass`.
/// (`quantise_animal_take` caps the kill by the crew's carry, which would otherwise smuggle a
/// second variable into the pinned number.)
const UNBOUNDED_CREW: u32 = 60;

/// A crew big enough to **take the whole herd in one turn** — which now means clearing **two** crew
/// bounds, not one. `quantise_animal_take` caps the kill by what the crew can collect *and*, since
/// `docs/plan_hunt_through_combat.md` §2, by how many animals it can bring into contact
/// (`workers × engage_rate`). Proving Eradicate empties a herd needs a crew that clears both, or the
/// test measures a crew bound instead of the policy.
///
/// **Three crew bounds now, not two** — slice 4 added the **fight**
/// (`docs/plan_hunt_through_combat.md` §4): the kill is `combat::resolve_fight`'s enemy losses, so a
/// crew must also do enough damage to put the whole herd on the ground in one turn.
///
/// `TEST_CAPACITY / body_mass` is ~267 animals at the shipped Red Deer mass. Red Deer engage at
/// `1.0` (so engagement wants ≥ 267) and a spear-armed hunter brings down `(20 − 1) / 25 = 0.76` of
/// one a turn (so the fight wants ≥ 351) — the fight is now the tighter of the three and sets this
/// number. It was `100` sized against carry alone, then `300` when engagement landed; at each of
/// those Sustain and Eradicate paid **identically**, which is a true statement about a small crew and
/// says nothing about the floor.
const HAUL_THE_WHOLE_HERD_CREW: u32 = 400;

/// **A crew far too small to clear the herd's escapement room** — the *labor-bound* half of the
/// forecast==actual sweep. One hunter carries `per_worker_biomass_capacity`, which is a rounding
/// error against `TEST_CAPACITY`, so the crew's throughput is what binds and the ceiling never does.
///
/// Since the harvest floor the two regimes are worth sweeping separately: a stance's ceiling is now
/// the whole standing surplus (`B − floor·K`), which on a full herd is enormous, so a realistic crew
/// is labor-bound at *every* stance and a forecast that only agreed with the take at the ceiling
/// would look correct on a fully-staffed harness and lie in play.
const LABOR_BOUND_CREW: u32 = 2;

/// **Two, not one, because the build dip has to have somewhere to land.** Engagement rounds up to a
/// whole animal (`fauna::animals_engaged`), so a single hunter engages `1` whether gentling or
/// hunting and the dip — correctly applied — is unobservable. At two the dipped crew engages `1`
/// against `2`, which is the smallest staffing where a dip can be *seen* rather than merely applied.
/// A crew that cannot see the dip proves nothing about whether the dip is there.
const _: () = assert!(LABOR_BOUND_CREW >= 2);

/// Float slop for a take reconstructed from a biomass delta through an `f32` rate.
const YIELD_EPSILON: f32 = 1e-3;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
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
    // **This harness is a deterministic pin, so the retreat stage is held at its identity.**
    // Slice 7 authored a non-zero `combat.wariness` across the roster
    // (`docs/plan_hunt_through_combat.md` §3.1); `FaunaConfig::without_retreat` carries the whole
    // reasoning for why the pre-existing suite neutralises it rather than re-baselining.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::SupplyNetworkConfigHandle::default());
    app.world.insert_resource(FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    // **The road ledger `advance_labor_allocation` counts spare road keepers against.** Empty
    // is the shipped turn-1 state — no traffic has worn anything in yet — so this harness's
    // keeping numbers are the roadless reading they have always been.
    app.world.insert_resource(core_sim::RoadRegistry::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CombatConfigHandle::default());
    app.world.insert_resource(CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world
        .insert_resource(core_sim::ExtractionConfigHandle::default());
    // An empty deposit registry is the shipped turn-1 state: a working is opened the
    // first turn a crew stands on it, so a harness with no `extract` row has none.
    app.world
        .insert_resource(core_sim::extraction::DepositRegistry::default());
    // Belief on a place — a hunt or a raid credits its dead to the tile the band stands on.
    app.world
        .insert_resource(core_sim::BeliefRegistry::default());
    app.world
        .insert_resource(core_sim::BeliefConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
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
    floor: f32,
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
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
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
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor,
                    },
                    workers,
                    kit: None,
                    priority: SourcePriority::default(),
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

/// **Every material on the producing/receiving band's own `LocalStore`, summed** — the non-food half
/// of what a take pays, held as batches on the very store `larder` reads. **Fractional**: the batch
/// store is fixed-point, so a sub-unit pelt haul accumulates rather than rounding away.
///
/// A bare sum across materials is the right shape *for these assertions only*: they ask whether a
/// take banked anything at all, or more than another take did. Which hide it is, and what its
/// characteristics read, is `materials.rs`'s subject.
fn materials(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| {
            c.stores
                .materials()
                .flat_map(|(_, batches)| batches.values())
                .map(|batch| batch.amount.to_f32())
                .sum()
        })
        .unwrap_or(0.0)
}

fn herd_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
        .unwrap_or(0.0)
}

/// One hunting turn on a re-shaped herd: `(food_banked, materials_banked, biomass_killed)`.
fn hunt_one_turn(display_name: &str, policy: f32, workers: u32) -> (f32, f32, f32) {
    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, display_name);
    let band = spawn_hunters(&mut app, pos, &id, policy, workers);
    let before = herd_biomass(&app, &id);
    app.world.run_system_once(advance_labor_allocation);
    (
        larder(&app, band),
        materials(&app, band),
        before - herd_biomass(&app, &id),
    )
}

/// **1. A wolf hunt credits hide and bone and EXACTLY zero food — on every rung.**
///
/// The first `edible = false` species. `provisions_per_biomass` is an explicit `0.0`, which is a
/// real configured value ("you do not eat me"), not an unset one — so the food component is zero on
/// every intensity, while the **material** account is strictly positive on every intensity. That is
/// the product/intensity split stated as an assertion. (It read the retired trade scalar until arc
/// #527; the wolf's payload is the same take's hide and bone, and always physically was.)
#[test]
fn a_wolf_hunt_credits_pelts_and_exactly_zero_food_on_every_rung() {
    for policy in EXTRACTIVE {
        let (food, stuff, killed) = hunt_one_turn("Grey Wolf Pack", policy, UNBOUNDED_CREW);
        assert!(
            killed > 0.0,
            "{policy:?}: the harness must actually take something, got {killed} biomass"
        );
        assert_eq!(
            food, 0.0,
            "{policy:?}: a wolf is not food — the larder must not move (killed {killed})"
        );
        assert!(
            stuff > 0.0,
            "{policy:?}: a wolf is a pelt — its materials must be credited (killed {killed})"
        );
    }
}

/// **2. A deer hunt credits meat AND hide under Sustain** — the rebalance.
///
/// Before the yield-vector arc only the deepest rung produced anything beyond meat (it alone carried
/// the retired 4× `market.trade_goods_multiplier`). Now every harvesting policy pays the species'
/// whole vector, so a restrained hunt banks hide and bone too.
#[test]
fn a_deer_hunt_credits_meat_and_hide_under_sustain() {
    let (food, stuff, killed) = hunt_one_turn("Red Deer", 0.5, UNBOUNDED_CREW);
    assert!(killed > 0.0, "the harness must take something");
    assert!(
        food > 0.0,
        "a Sustain deer hunt still feeds the band: {food}"
    );
    assert!(
        stuff > 0.0,
        "a Sustain deer hunt banks hide and bone too (it earned nothing beyond meat before): {stuff}"
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
        hunt_one_turn("Red Deer", 0.5, HAUL_THE_WHOLE_HERD_CREW);

    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, "Red Deer");
    let band = spawn_hunters(&mut app, pos, &id, 0.0, HAUL_THE_WHOLE_HERD_CREW);
    app.world.run_system_once(advance_labor_allocation);
    let eradicate_food = larder(&app, band);

    assert!(
        eradicate_food > sustain_food,
        "Eradicate takes the whole stock, so it must out-feed the Sustain skim: {eradicate_food} \
         vs {sustain_food} (Sustain killed {sustain_killed})"
    );
    assert!(
        materials(&app, band) > 0.0,
        "denial banks its hides too — every rung is paid the species' whole vector"
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

/// **5. The larder ledger still closes for a wolf hunt — materials are NOT food income.**
///
/// `foodIncome` is `Σ SourceYield::actual`, and the identity
/// `larder_delta == food_income − food_consumption − raid_forfeit − spoiled + received − sent` is
/// what makes the band's food panel honest. A hunt that credits a *second* account must not leak it
/// into that sum: a wolf's take contributes `0` to `food_income` while filling the band's material
/// batches. Run with only the labor system, so consumption, raid, rot (a local row walks no pack)
/// and transfers are all `0` and the identity reduces to `larder_delta == Σ actual`.
#[test]
fn the_larder_ledger_excludes_materials_for_a_wolf_hunt() {
    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, "Grey Wolf Pack");
    let band = spawn_hunters(&mut app, pos, &id, 0.15, UNBOUNDED_CREW);
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
        materials(&app, band) > 0.0,
        "…while the same take really did bank hide and bone, so this is not a no-op hunt"
    );
}

/// **A hunt row's FODDER is an honest zero, and it is structural** (issue #449).
///
/// The feed account is plant-only — no animal's yield vector pays it — so the third account exists
/// on the row to let a hay Field state its product, never to be silently populated on the animal
/// web. Asserted on a hunt that demonstrably took something, so this is *"a real take pays no
/// fodder"* rather than *"nothing happened"*.
#[test]
fn a_hunt_row_reports_no_fodder_because_no_animal_pays_it() {
    let mut app = spawn_world();
    let (id, pos) = reshape_first_herd(&mut app, "Red Deer");
    let band = spawn_hunters(&mut app, pos, &id, 0.15, UNBOUNDED_CREW);

    app.world.run_system_once(advance_labor_allocation);

    let row = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_yields
        .first()
        .expect("its one Hunt assignment has a yield row")
        .clone();
    assert!(
        row.actual > 0.0,
        "the harness must actually feed the band, or the zero below is vacuous"
    );
    assert_eq!(
        row.fodder, 0.0,
        "no animal pays fodder, so a hunt row's zero is structural rather than unset"
    );
}

/// **6. A `yields_nothing` species offers Eradicate ALONE.**
///
/// The only pruning rule in the picker: a pure pest, worth neither meat nor material, has no
/// meaningful *rate* at which to collect nothing — the one coherent verb left is *make it stop*. No
/// shipped species is this today (a wolf is a pelt and a bone), so it is pinned on a synthetic
/// config, and it is asserted through [`core_sim::species_requires_denial`] — the single seam the
/// `assign_labor` validator and the snapshot both read, so the two can never disagree.
///
/// **The synthetic pest must strip the MATERIAL rows as well as the food rate.** Until arc #527 the
/// second half of "worth nothing" was a trade rate; it is the material list now, and a deer with
/// `provisions 0` and its hide intact is still worth hunting.
#[test]
fn a_yields_nothing_species_may_only_be_worked_at_floor_zero() {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_FAUNA_CONFIG).expect("the builtin parses");
    json["species"]["deer"]["hunt_yield"] =
        serde_json::json!({ "provisions_per_biomass": 0.0, "materials": [] });
    let config =
        FaunaConfig::from_json_str(&json.to_string()).expect("a zero vector is a legal config");

    let pest = config.hunt_yield_for("Red Deer");
    assert!(pest.yields_nothing(), "the synthetic deer yields nothing");
    assert!(
        core_sim::species_requires_denial(pest),
        "a worthless quarry may only be worked at floor 0 — there is nothing else to do with it"
    );

    // …and the flags gate the yield ACCOUNTS, not the dial: an inedible species that is still made
    // of something may be worked at ANY floor, because every depth is a meaningful one at which to
    // collect hides.
    let wolf = config.hunt_yield_for("Grey Wolf Pack");
    assert!(!wolf.edible() && wolf.yields_materials);
    assert!(
        !core_sim::species_requires_denial(wolf),
        "a wolf may be worked at any floor and is paid in hide and bone"
    );
}

/// **The derived flags are a comparison against the vector, never a stored second copy** — the
/// property that keeps "is it food?" from drifting away from "what does it pay?".
#[test]
fn edible_and_worth_hunting_are_derived_from_the_vector() {
    let both = HuntYield {
        provisions_per_biomass: 0.02,
        yields_materials: true,
    };
    assert!(both.edible() && !both.yields_nothing());

    let pelt_only = HuntYield {
        provisions_per_biomass: 0.0,
        yields_materials: true,
    };
    assert!(!pelt_only.edible() && !pelt_only.yields_nothing());

    let meat_only = HuntYield {
        provisions_per_biomass: 0.02,
        yields_materials: false,
    };
    assert!(meat_only.edible() && !meat_only.yields_nothing());

    let pest = HuntYield {
        provisions_per_biomass: 0.0,
        yields_materials: false,
    };
    assert!(!pest.edible() && pest.yields_nothing());
}

// ---------------------------------------------------------------------------------------------------
// B2 — the forecast and the wire
// ---------------------------------------------------------------------------------------------------

/// The two shipped species this file contrasts: one that omits `hunt_yield`'s rate (so it falls back
/// to the global) and the one that declares an inedible, pelt-bearing vector.
const DEFAULTING_SPECIES: &str = "Red Deer";
const INEDIBLE_SPECIES: &str = "Grey Wolf Pack";

/// A **full headless app** (the real turn pipeline + the real snapshot capture) with its first game
/// herd re-shaped into `display_name` at [`TEST_CAPACITY`], returning `(app, herd_id, herd_tile)`.
///
/// The B1 tests run on a light harness; these need the *shipped representation*, so they go through
/// `build_headless_app` + `recapture_snapshot_in_place` — the same path
/// `band_hunt_preview::exported_snapshot_fields_reproduce_band_hunt_take` uses.
fn headless_with_species(display_name: &str) -> (App, String, UVec2) {
    let mut app = build_test_app();
    // **This suite measures the yield VECTOR, so the retreat stage is held at its identity** — the
    // same move [`steady_quarry`] makes for `engage_rate` and `defense`, one field further along.
    // Slice 7 authored a non-zero `combat.wariness` roster-wide
    // (`docs/plan_hunt_through_combat.md` §3.1), and a `forecast == actual` pin cannot be read off a
    // stochastic take at all: the forecast reports the take's *expectation*, so an equality here
    // would be asserting that one draw equals a mean. See `FaunaConfig::without_retreat`.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
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
    // **Re-emit the display telemetry, or the WIRE describes the herd this fixture replaced.**
    // `WorldSnapshot.herds` is built from `HerdTelemetry`, which `app.update()` filled *before* the
    // reshape above — so a test reading the exported `biomass` would compose a ceiling against the
    // pre-reshape stock while the take reads the registry. `spawn_initial_herds` early-returns on a
    // non-empty registry and refreshes exactly the telemetry + density the turn loop would.
    app.world.run_system_once(spawn_initial_herds);
    (app, id, pos)
}

/// **Mark the herd's tile `Active` for the viewer faction.** `WorldSnapshot.herds` is fog-filtered
/// (see "Herd display telemetry is FOG-FILTERED" in the fauna rules), so an unwatched herd is simply
/// absent from the shipped snapshot — these tests read exported per-herd rows, so they must first put
/// eyes on it. The `band_hunt_preview::reveal_herds` pattern.
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
    floor: f32,
    workers: u32,
) -> bevy::prelude::Entity {
    spawn_resident_crew(app, pos, fauna_id, floor, workers, NO_IMPROVEMENT_UNDERWAY)
}

/// [`spawn_resident_hunters`] with a build verb in flight — the axis whose dip rides *crew
/// throughput* since `docs/plan_harvest_floor.md` §3.1, and therefore the one that can break the
/// forecast if the two halves apply it to different terms.
fn spawn_resident_crew(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    floor: f32,
    workers: u32,
    improvement: Option<Improvement>,
) -> bevy::prelude::Entity {
    // The builders pool this fixture stands up: the take crew's count when a verb is declared, and
    // nobody at all when there is nothing queued for them to raise.
    let build_crew = if improvement.is_some() { workers } else { 0 };
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
                // **BIG ENOUGH TO FIELD EVERY ROW IT STAFFS.** The hunters and the builders draw on
                // one pool (`docs/plan_standing_upkeep.md` §2.2), so a band sized at `workers` while
                // staffing `workers` on each row is over-committed and `LaborAllocation::normalize`
                // sheds one of them before the pass has resolved anything — which is what this
                // fixture used to do, dropping the builders on every call and running every "with a
                // build in flight" case with nobody raising it.
                children: scalar_zero(),
                working: scalar_from_f32((workers + build_crew) as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
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
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
            },
            ResidentBand,
            LaborAllocation {
                assignments: std::iter::once(LaborAssignment {
                    party: None,
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor,
                    },
                    workers,
                    kit: None,
                    priority: SourcePriority::default(),
                })
                // **The build's hands are a band-level pool** since
                // `docs/plan_standing_upkeep.md` §2.5, staffed at the same count the take is so the
                // declared build is genuinely running beside the hunters — and staffed **only when
                // something is declared**, because a builders row with an empty queue is hands the
                // band is paying for and nobody is spending.
                .chain((build_crew > 0).then_some(LaborAssignment {
                    party: None,
                    target: LaborTarget::Builders,
                    workers: build_crew,
                    kit: None,
                    priority: SourcePriority::default(),
                }))
                .collect(),
                build_queue: improvement
                    .map(|declared| core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Herd(fauna_id.to_string()),
                        declared: core_sim::BuildJob::Rung(declared),
                        priority: core_sim::SourcePriority::default(),
                    })
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        ))
        .id()
}

/// The **pre-commit forecast** for this herd/staffing/policy — the assign-time seed
/// (`hunt_source_yield_preview`), i.e. the pair the client is shown *before* committing. Read before
/// the turn resolves.
fn precommit_food(app: &App, id: &str, policy: f32, workers: u32) -> f32 {
    precommit_food_building(app, id, policy, workers, NO_IMPROVEMENT_UNDERWAY)
}

/// [`precommit_food`] at the band's **live** output multiplier rather than the content-band `1.0`.
///
/// A single-turn fixture can assume a content band; a multi-turn one cannot — morale moves under a
/// run, and the resolved take applies the multiplier while [`precommit_food`] does not. Reading the
/// exported cohort's own multiplier is what `band_hunt_preview`'s sweep already does, and
/// it keeps a multi-turn comparison about the fight rather than about wellbeing.
fn precommit_food_at_band_morale(
    app: &App,
    id: &str,
    band: bevy::prelude::Entity,
    policy: f32,
    workers: u32,
) -> f32 {
    let multiplier = {
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
        core_sim::Scalar::from_raw(cohort.output_multiplier).to_f32()
    };
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("the herd is on the map");
    let seed = hunt_source_yield_preview(
        herd,
        &fauna,
        equipped_haul_rate(),
        &HuntingParty::builtin_equipped(),
        multiplier,
        workers,
        core_sim::NO_HANDS,
        policy,
        labor.yield_average_horizon_turns,
        labor.arrivals_horizon_turns,
        app.world
            .resource::<CombatConfigHandle>()
            .get()
            .forecast_range_sigmas,
    );
    seed.actual
}

/// [`precommit_food`] with a build verb in flight.
fn precommit_food_building(
    app: &App,
    id: &str,
    policy: f32,
    workers: u32,
    _improvement: Option<Improvement>,
) -> f32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("the herd is on the map");
    let seed = hunt_source_yield_preview(
        herd,
        &fauna,
        equipped_haul_rate(),
        &HuntingParty::builtin_equipped(),
        FORECAST_OUTPUT_MULTIPLIER,
        workers,
        core_sim::NO_HANDS,
        policy,
        labor.yield_average_horizon_turns,
        labor.arrivals_horizon_turns,
        app.world
            .resource::<CombatConfigHandle>()
            .get()
            .forecast_range_sigmas,
    );
    seed.actual
}

/// A content band's output multiplier — morale `1.0`, so the forecast and the take share it.
const FORECAST_OUTPUT_MULTIPLIER: f32 = 1.0;

/// The **exported** `actualYield` for a band's single assignment, read off the shipped snapshot
/// rather than the in-process `SourceYield`.
fn exported_food(app: &App, band: bevy::prelude::Entity) -> f32 {
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
    row.actual_yield
}

/// The **exported** telemetry row for a band's single assignment — the whole shipped record, for the
/// staffing signals (`workersNeeded` / `wastedYield`) [`exported_food`] does not carry.
fn exported_row(app: &App, band: bevy::prelude::Entity) -> sim_runtime::LaborAssignmentState {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    snapshot
        .populations
        .iter()
        .find(|p| p.entity == band.to_bits())
        .expect("the hunting band is in the snapshot")
        .labor_assignments
        .first()
        .expect("its one Hunt assignment is exported")
        .clone()
}

/// The **exported** herd row — the terms a client composes any floor's ceiling from
/// (`biomass`, `carryingCapacity` and the per-biomass yield vector).
fn exported_herd(app: &App, id: &str) -> sim_runtime::HerdTelemetryState {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is in the snapshot")
        .clone()
}

/// **7. `forecast == actual` for BOTH products, for a wolf and for a deer — ON THE WIRE.**
///
/// **The arc's load-bearing test.** The pre-commit forecast, the assign-time seed and the resolved
/// take all read the same helpers, so what the client is shown before committing must be exactly what
/// the sim then pays — *per component*. A food-only forecast could not even state a wolf's yield (all
/// its food ceilings are `0`), which is precisely why `SourceYieldForecast` is a `YieldAccounts` rather
/// than a scalar with a bolted-on sibling: two halves can drift, one pair cannot.
///
/// Asserted on the **exported snapshot** (`laborAssignments[].actualYield` / `.tradeYield`), not on
/// the in-process struct, so it pins the shipped representation — an export that projected the wrong
/// component would pass an in-memory check and still lie to the client.
///
/// **Swept over both binding regimes** (`docs/plan_harvest_floor.md` §9): a [`LABOR_BOUND_CREW`],
/// where `workers × per_worker` is the smaller term, and crews large enough for the stance's
/// **escapement ceiling** to bind instead. The take is `min` of the two, so an agreement that held
/// on only one side would be an agreement about one term rather than about the take.
#[test]
fn the_forecast_equals_the_paid_take_on_the_wire() {
    let mut saw_labor_bound = false;
    let mut saw_escapement_bound = false;
    for species in [DEFAULTING_SPECIES, INEDIBLE_SPECIES] {
        for policy in EXTRACTIVE {
            for crew in [LABOR_BOUND_CREW, UNBOUNDED_CREW, HAUL_THE_WHOLE_HERD_CREW] {
                let (mut app, id, pos) = headless_with_species(species);
                // Which term binds, off the same two numbers the take composes.
                {
                    let fauna = app.world.resource::<FaunaConfigHandle>().get();

                    let registry = app.world.resource::<HerdRegistry>();
                    let herd = registry.find(&id).expect("the herd is on the map");
                    let ceiling = core_sim::hunt_escapement_ceiling(
                        policy,
                        herd.biomass,
                        core_sim::herd_capacity(herd, &fauna),
                    );
                    let collection = crew as f32 * equipped_haul_rate();
                    if collection < ceiling {
                        saw_labor_bound = true;
                    } else {
                        saw_escapement_bound = true;
                    }
                }
                let forecast = precommit_food(&app, &id, policy, crew);
                let band = spawn_resident_hunters(&mut app, pos, &id, policy, crew);

                app.world.run_system_once(advance_labor_allocation);
                recapture_snapshot_in_place(&mut app.world);
                let paid = exported_food(&app, band);

                assert!(
                    (forecast - paid).abs() <= YIELD_EPSILON,
                    "{species} {policy:?} × {crew} hunters: forecast FOOD {forecast} must equal \
                     the paid {paid}"
                );
                // …and the test must not be vacuously comparing two zeros. The EDIBLE species pays
                // food at every rung and every staffing; the inedible one honestly pays none at any,
                // and asserting that is a claim rather than an exemption — its real payload is hide
                // and bone, which the forecast does not project (arc #527).
                if species == INEDIBLE_SPECIES {
                    assert_eq!(
                        paid, 0.0,
                        "{species} {policy:?} × {crew} hunters: an inedible quarry pays no food, \
                         and the row must say so"
                    );
                } else {
                    assert!(
                        paid > 0.0,
                        "{species} {policy:?} × {crew} hunters: the harness must actually take \
                         something ({paid})"
                    );
                }
            }
        }
    }
    assert!(
        saw_labor_bound && saw_escapement_bound,
        "both regimes must be covered: labor-bound={saw_labor_bound} \
         escapement-bound={saw_escapement_bound}"
    );
}

/// **7c. `forecast == actual` ACROSS A MULTI-TURN KILL** (`docs/plan_hunt_through_combat.md` §4.2).
///
/// Damage now carries between turns, so a sub-threshold party takes **nothing** for several turns and
/// then a whole animal. That pulse is exactly the shape a forward projection can miss: a forecast
/// that resolved the fight once and froze it would quote the first turn's zero forever, and a
/// forecast that ignored the herd's banked wounds would quote a zero on the very turn the kill lands.
/// Either way the multi-turn kill is invisible in the preview while the sim pays it — the defect
/// class this suite exists for.
///
/// Asserted on the **exported snapshot**, per component, on **every** turn of the run: the wait turns
/// (both zero, honestly) and the kill turn (both the same body). Paired with the liveness half —
/// a kill must actually land, or a forecast frozen at zero would agree with a take frozen at zero.
#[test]
fn the_forecast_equals_the_paid_take_across_a_multi_turn_kill() {
    /// Far below `ceil(500 / (20 − 12)) = 63`, so the party grinds the mammoth down over several
    /// turns instead of taking one every turn.
    const SUB_THRESHOLD_CREW: u32 = 12;
    /// Long enough to contain at least one wait→kill cycle at that crew.
    const TURNS: u32 = 12;
    /// Take everything the herd can spare, so the escapement never binds and the *fight* is the only
    /// term that can produce a zero.
    const STRIP_IT_BARE: f32 = 0.0;

    let (mut app, id, pos) = headless_with_species(HEAVY_BODIED_SPECIES);
    reveal_herd(&mut app, &id);
    let band = spawn_resident_hunters(&mut app, pos, &id, STRIP_IT_BARE, SUB_THRESHOLD_CREW);
    // The band has to be on the wire before its multiplier can be read off it.
    recapture_snapshot_in_place(&mut app.world);

    let mut saw_wait_turn = false;
    let mut saw_kill_turn = false;
    for turn in 1..=TURNS {
        // **Quote the crew the turn will actually field.** A mammoth hunt costs the band people, so
        // `working` shrinks under the run and `advance_labor_allocation` normalizes the assignment
        // down to what is left. Forecasting a stale headcount would measure that lag rather than the
        // accumulator — casualty-aware staffing in the preview is its own follow-up.
        let crew = {
            let cohort = app
                .world
                .get::<PopulationCohort>(band)
                .expect("the band is alive");
            core_sim::available_workers(cohort.working).min(SUB_THRESHOLD_CREW)
        };
        // The pre-commit quote for THIS turn's herd state — banked wounds and all — at the band's
        // live morale, since a multi-turn run cannot assume a content band.
        let forecast = precommit_food_at_band_morale(&app, &id, band, STRIP_IT_BARE, crew);
        app.world.run_system_once(advance_labor_allocation);
        recapture_snapshot_in_place(&mut app.world);
        let paid = exported_food(&app, band);

        assert!(
            (forecast - paid).abs() <= YIELD_EPSILON,
            "turn {turn}: forecast FOOD {forecast} must equal the paid {paid}"
        );
        if paid > 0.0 {
            saw_kill_turn = true;
        } else {
            saw_wait_turn = true;
        }
    }
    assert!(
        saw_wait_turn,
        "the fixture must be genuinely sub-threshold — a party that kills every turn does not \
         exercise the accumulator"
    );
    assert!(
        saw_kill_turn,
        "liveness: the banked damage must eventually land a kill, or both sides agree on zero \
         for the wrong reason"
    );
}

/// **7b. `forecast == actual` WITH A BUILD IN FLIGHT, swept over the floor** — the sweep that
/// matters most for `docs/plan_standing_upkeep.md` §2.2.
///
/// The build's price moved off the take ceiling, then off a `yield_fraction_while_building` factor
/// on crew throughput, and is now the crew's **whole work budget**. That is exactly the kind of
/// change that can split the forecast from the take: the two halves compose the same `min` out of
/// two terms, and moving a claim from one term to the other has to happen in both places at once or
/// the client is quoted a number the sim will not pay. Asserted on the **exported snapshot**, per
/// component, on a defaulting species (food) and an inedible one, across every build verb the animal
/// web offers and both binding regimes.
///
/// It also pins the property the move exists to create: **the build's price is
/// floor-independent**. A builder's collection is zero at *every* floor — there is no floor a builder
/// can pick to dodge the cost, which is what §0.3 measured going wrong when the dip multiplied the
/// ceiling instead.
///
/// **The improvement axis includes the pure harvest**, because a builder is labor-bound at zero by
/// construction and could never reach the escapement-bound regime on its own — the sweep's own
/// liveness needs a crew that actually carries something.
#[test]
fn the_forecast_equals_the_paid_take_with_a_build_in_flight_at_every_floor() {
    let mut saw_labor_bound = false;
    let mut saw_escapement_bound = false;
    for species in [DEFAULTING_SPECIES, INEDIBLE_SPECIES] {
        for floor in EXTRACTIVE {
            for improvement in [
                NO_IMPROVEMENT_UNDERWAY,
                Some(Improvement::Tame),
                Some(Improvement::Corral),
            ] {
                for crew in [LABOR_BOUND_CREW, UNBOUNDED_CREW, HAUL_THE_WHOLE_HERD_CREW] {
                    let (mut app, id, pos) = headless_with_species(species);
                    {
                        let fauna = app.world.resource::<FaunaConfigHandle>().get();

                        let registry = app.world.resource::<HerdRegistry>();
                        let herd = registry.find(&id).expect("the herd is on the map");
                        let ceiling = core_sim::hunt_escapement_ceiling(
                            floor,
                            herd.biomass,
                            core_sim::herd_capacity(herd, &fauna),
                        );
                        // The collection is the TAKE crew's own — a build on this herd is staffed
                        // in its own right (`docs/plan_standing_upkeep.md` §2.2), so nothing scales
                        // what a hauler carries and the regime is decided by the hunters alone.
                        let collection = crew as f32 * equipped_haul_rate();
                        if collection < ceiling {
                            saw_labor_bound = true;
                        } else {
                            saw_escapement_bound = true;
                        }
                    }
                    let forecast = precommit_food_building(&app, &id, floor, crew, improvement);
                    let band = spawn_resident_crew(&mut app, pos, &id, floor, crew, improvement);

                    app.world.run_system_once(advance_labor_allocation);
                    recapture_snapshot_in_place(&mut app.world);
                    let paid = exported_food(&app, band);

                    assert!(
                        (forecast - paid).abs() <= YIELD_EPSILON,
                        "{species} floor {floor} + {improvement:?} × {crew}: forecast FOOD \
                         {forecast} must equal the paid {paid}"
                    );
                }
            }
        }
    }
    assert!(
        saw_labor_bound && saw_escapement_bound,
        "both regimes must be covered: labor-bound={saw_labor_bound} \
         escapement-bound={saw_escapement_bound}"
    );

    // **The build's price is floor-independent by construction, and it is not in the take at all.**
    // A build is staffed in its own right (`docs/plan_standing_upkeep.md` §2.2), so the hunters
    // carry the same at every floor whether or not a verb is in flight beside them — what a Tame
    // costs is the hands on it. Measured on the exported wire, not on the forecast, so it is the
    // shipped number.
    for floor in EXTRACTIVE {
        let take = |improvement| {
            let (mut app, id, pos) = headless_with_species(DEFAULTING_SPECIES);
            let band =
                spawn_resident_crew(&mut app, pos, &id, floor, LABOR_BOUND_CREW, improvement);
            app.world.run_system_once(advance_labor_allocation);
            recapture_snapshot_in_place(&mut app.world);
            exported_food(&app, band)
        };
        let harvesting = take(NO_IMPROVEMENT_UNDERWAY);
        let taming = take(Some(Improvement::Tame));
        assert!(
            harvesting > 0.0,
            "floor {floor}: the harness must actually take something"
        );
        assert_eq!(
            taming, harvesting,
            "floor {floor}: a Tame in flight moves nothing about what the hunters carry"
        );
    }
}

/// **8. A wolf's exported per-policy ceilings read ZERO food at every floor, and it is still
/// huntable at every one of them.**
///
/// The wire-level statement of *product × intensity*: the rungs are a pressure ladder over one
/// species' vector, so an inedible quarry offers every rung — each a meaningful depth at which to
/// collect **hides** — and every one of them is honestly `0` food.
///
/// **The "and it is still huntable" half is what the retirement of the trade axis put at risk**
/// (arc #527): the wolf's whole scalar payload was the trade rate, so a picker rule that tested
/// only food would have pruned every floor but denial off it. It reads its material rows instead,
/// which is what `species_requires_denial` is asserted against here.
#[test]
fn a_wolves_exported_rate_reads_no_food_and_it_is_still_huntable_at_every_floor() {
    let (mut app, id, _pos) = headless_with_species(INEDIBLE_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);

    let exported = exported_herd(&app, &id);
    // **The vector states it once, and it holds at every floor by construction.** A food-only export
    // could not say "0 food, real trade" at all — which is why the wire carries a per-biomass
    // VECTOR rather than a food scalar.
    assert_eq!(
        exported.provisions_per_biomass, 0.0,
        "a wolf is not food — its exported food rate must be 0"
    );
    // **The species is nonetheless worth working at every depth**, and that is a fact about its
    // material rows rather than about any rate on this row.
    let hunt_yield = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .hunt_yield_for(INEDIBLE_SPECIES);
    assert!(
        hunt_yield.yields_materials && !core_sim::species_requires_denial(hunt_yield),
        "a wolf is a pelt and a bone, so every floor is a meaningful depth to work it at"
    );
    // **Composed at every floor the herd actually stands above.** A floor above the standing stock
    // composes an honest zero — that is the escapement rule, not a wolf fact — so the sweep is over
    // the floors that have room, with a liveness bound so it cannot be empty.
    let standing_fraction = exported.biomass / exported.carrying_capacity;
    let mut saw_room = false;
    for floor in EXTRACTIVE {
        let room = (exported.biomass - floor * exported.carrying_capacity).max(0.0);
        assert_eq!(
            room * exported.provisions_per_biomass,
            0.0,
            "floor {floor}: a wolf's composed FOOD ceiling is 0 at every depth, room or not"
        );
        if floor >= standing_fraction {
            assert_eq!(room, 0.0, "floor {floor} is above the standing stock");
            continue;
        }
        saw_room = true;
        assert!(
            room > 0.0,
            "floor {floor}: …and where there IS room, the herd really stands above it"
        );
    }
    assert!(
        saw_room,
        "the sweep must reach a floor the herd stands above ({standing_fraction} of K), or it is \
         asserting about zeros"
    );

    // The per-herd row itself is still published — only the estimate TABLE that used to hang off it
    // is retired — so the species-aware rates below are read off the shipped snapshot as before.
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
        .cloned()
        .expect("the herd is in the snapshot");

    // The per-herd, species-aware per-worker rates agree: no food, real trade. (The cohort-level
    // `huntPerWorkerProvisions` is species-blind by construction — see its doc — which is exactly why
    // a band preview must clamp with THESE.)
    assert_eq!(herd.per_worker_yield, 0.0, "no food per hunter on a wolf");
    assert_eq!(herd.food_per_animal, 0.0, "a wolf is worth no food");
    // …and the CREW half of the same question is still answerable, which is exactly why
    // `perWorkerBiomass` is on the wire: the food quotient a client would otherwise divide is `0/0`.
    assert!(
        herd.per_worker_biomass > 0.0,
        "a hunter still carries wolf biomass home — the term a crew count divides by"
    );
}

/// **THE PUBLISHED MATERIAL QUOTE IS THE MATERIAL THE SIM CREDITS** (arc #527) — the fauna mirror of
/// `flora_f4_cash::the_picker_material_quote_is_the_material_the_sim_credits`, and the guard on the
/// regression retiring the trade axis caused.
///
/// A wolf paid `trade_goods_per_biomass: 0.02`, so its compose sheet had a rate to show. When that
/// went, nothing per-herd replaced it: the sheet quoted **no rate at all**, the board row and map
/// label read `+0.00`, and the pelts still landed in the band's store when the hunt resolved. A wolf
/// is the natural subject precisely because its **entire** yield is material — every food reading it
/// publishes is an honest zero, so nothing else on its row can cover for a missing material quote.
///
/// Four claims, and the third is the one that matters:
/// 1. the herd row quotes a **rate** where its food rate is `0` — the compose-sheet fix;
/// 2. the resolved assignment row publishes the materials it credited — the `+0.00` fix;
/// 3. **that published amount is what `LocalStore::material_total` actually holds**, asserted against
///    the store rather than a re-derivation of the credit's arithmetic;
/// 4. the published per-biomass rate is the rate the credit was *paid at* — every material's
///    `credited ÷ rate` is the **same** positive number (the carried biomass), which a rate published
///    from a second derivation would not satisfy.
#[test]
fn a_wolfs_published_material_quote_is_what_the_hunt_credits() {
    let (mut app, id, pos) = headless_with_species(INEDIBLE_SPECIES);
    reveal_herd(&mut app, &id);
    let band = spawn_resident_hunters(&mut app, pos, &id, STRIP_IT_BARE, UNBOUNDED_CREW);
    recapture_snapshot_in_place(&mut app.world);

    // 1. THE COMPOSE-SHEET RATE. A wolf's food readings are honestly zero; the material ones are not,
    //    and they are what a client composes `max(0, B − f·K) × rate` from at any floor.
    let herd = exported_herd(&app, &id);
    assert_eq!(
        herd.provisions_per_biomass, 0.0,
        "the fixture's premise: a wolf publishes no food rate"
    );
    assert_eq!(herd.per_worker_yield, 0.0);
    assert!(
        !herd.material_per_biomass.is_empty(),
        "a wolf must quote WHAT IT IS MADE OF, or its compose sheet quotes nothing at all"
    );
    assert!(
        herd.material_per_biomass.iter().all(|row| row.amount > 0.0),
        "a quoted rate is a rate that pays: {:?}",
        herd.material_per_biomass
    );
    assert!(
        !herd.per_worker_material.is_empty()
            && herd.per_worker_material.iter().all(|row| row.amount > 0.0),
        "…and one hunter's own throughput, the material twin of perWorkerYield: {:?}",
        herd.per_worker_material
    );

    // 2/3. THE RESOLVED ROW IS THE STORE. Run a real turn and read both.
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
    let row = exported_row(&app, band);
    assert_eq!(
        row.actual_yield, 0.0,
        "a wolf hunt still adds nothing to the larder"
    );
    assert!(
        !row.material_yield.is_empty(),
        "…and the row must state what it DID pay, or the board reads +0.00 for a hunt that banked \
         hides"
    );

    let cohort = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the hunting band still exists");
    for published in &row.material_yield {
        let held = cohort
            .stores
            .material_total(&published.material_id)
            .to_f32();
        assert!(
            (held - published.amount).abs() <= YIELD_EPSILON,
            "the row published {} of {} and the band's store holds {held}",
            published.amount,
            published.material_id
        );
        assert!(published.amount > 0.0, "a published row is a row that paid");
    }

    // 4. THE RATE IS THE RATE THE CREDIT USED. Every material's credited amount over its published
    //    per-biomass rate is the same number — the biomass the party carried home — which is only
    //    true if the quote and the payout read one set of rows.
    let mut carried: Option<f32> = None;
    for rate in &herd.material_per_biomass {
        let credited = row
            .material_yield
            .iter()
            .find(|paid| paid.material_id == rate.material_id)
            .unwrap_or_else(|| {
                panic!(
                    "the take paid no {}, which the rate promised",
                    rate.material_id
                )
            })
            .amount;
        let implied = credited / rate.amount;
        match carried {
            None => carried = Some(implied),
            Some(first) => assert!(
                (implied - first).abs() <= first * MATERIAL_RATE_TOLERANCE,
                "every material must imply the SAME carried biomass — {} implies {implied} against \
                 {first}",
                rate.material_id
            ),
        }
    }
    assert!(
        carried.is_some_and(|biomass| biomass > 0.0),
        "the harness must actually carry something home, or claim 4 compared nothing"
    );
}

/// The slack claim 4 allows. Both sides are `f32`s that have been through the store's fixed-point
/// grid on the way out, so two materials at very different rates round differently; a percent is
/// orders of magnitude below any real disagreement (a rate published from a second derivation would
/// be off by a factor, not by a rounding).
const MATERIAL_RATE_TOLERANCE: f32 = 0.01;

/// **9. A composed ceiling carries the windfall at floor `0`.**
///
/// Denial once quoted a zeroed food row, on the premise it carries nothing home — the premise #337
/// reversed. There is no row to zero any more: the client composes `max(0, B − f·K) × rate` from the
/// exported per-biomass vector (`docs/plan_harvest_floor.md` §5), so floor `0` is simply the whole
/// standing stock and must top the ladder.
#[test]
fn a_composed_ceiling_carries_the_windfall_at_floor_zero() {
    let (mut app, id, _pos) = headless_with_species(DEFAULTING_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);

    let herd = exported_herd(&app, &id);
    // THE CLIENT'S OWN COMPOSITION, from the three terms the wire publishes.
    let ceiling_at = |floor: f32| -> f32 {
        let room = (herd.biomass - floor * herd.carrying_capacity).max(0.0);
        room * herd.provisions_per_biomass
    };

    // Two floors the herd genuinely stands above, derived from the exported stock so the comparison
    // is never between two zeros whatever biomass worldgen seeded.
    let standing_fraction = herd.biomass / herd.carrying_capacity;
    assert!(
        standing_fraction > 0.0,
        "the fixture herd must be standing on something"
    );
    let strip = ceiling_at(0.0);
    let deep = ceiling_at(standing_fraction * 0.5);
    assert!(
        strip > 0.0,
        "floor 0 composes the windfall, not a zeroed denial row: {strip}"
    );
    assert!(
        strip > deep && deep > 0.0,
        "floor 0 takes the whole stock, so it must top the ladder: {strip} vs {deep}"
    );
}

// ---------------------------------------------------------------------------------------------------
// B4 — the ENGAGEMENT bound reaches the WIRE (`docs/plan_hunt_through_combat.md` §2)
// ---------------------------------------------------------------------------------------------------

/// The lightest-bodied huntable species in the shipped roster (`body_mass` 0.13, `engage_rate` 10) —
/// the regime where **reach** is the binding bound by a wide margin: one hunter's 40 biomass of carry
/// is 307 birds, and one hunter reaches ten.
const SMALL_BODIED_SPECIES: &str = "Wild Fowl";

/// The heaviest (`body_mass` 800, `engage_rate` 0.05) — the other end of the same authoring rule
/// (`engage_rate × body_mass ≤ per_worker_biomass_capacity`), where the two bounds meet.
const HEAVY_BODIED_SPECIES: &str = "Thunder Mammoths";

/// **One hunter**, because that is the staffing the defect was measured at and the one where the two
/// bounds are furthest apart.
const LONE_HUNTER: u32 = 1;

/// **12. The wire carries the ENGAGEMENT term, and without it a client's composed take is ~30× the
/// take the sim pays.**
///
/// The escapement ceiling and the per-worker carry ship as **terms** because the composition
/// `min(workers × carry, ceiling)` is linear and exact (`.claude/rules/core_sim/yield-forecast.md`,
/// "THE BOUNDARY"). Engagement is the **third** bound and is linear in the same way — but it was not
/// on the wire, so a compose sheet built the `min()` out of two of the three and quoted a carry-bound
/// number. Measured on a Wild Fowl herd with one hunter: 307 birds/turn against a take of 10.
///
/// Asserted on the **exported snapshot** — the herd row's terms and the assignment row's
/// `actualYield` — because the whole point is that the client cannot re-derive the answer from
/// anything it does not ship: an in-process check would pass on a wire missing the field entirely.
///
/// The pair is **carry-only overstates** (the defect) beside **carry+reach reproduces exactly** (the
/// fix), so a term that silently went dead could not satisfy both.
#[test]
fn the_exported_terms_reproduce_the_engagement_bounded_take() {
    let (mut app, id, pos) = headless_with_species(SMALL_BODIED_SPECIES);
    reveal_herd(&mut app, &id);
    let floor = MSY_BIOMASS_FRACTION;
    let band = spawn_resident_hunters(&mut app, pos, &id, floor, LONE_HUNTER);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    let herd = exported_herd(&app, &id);
    let paid = exported_food(&app, band);

    assert!(
        herd.engage_rate > 0.0,
        "the herd row must publish a real engagement throughput, got {}",
        herd.engage_rate
    );
    assert!(
        herd.body_mass > 0.0 && herd.per_worker_biomass > 0.0 && herd.provisions_per_biomass > 0.0,
        "liveness: the three terms this composition already had must be on the row too ({herd:?})"
    );

    // THE CLIENT'S OWN COMPOSITION, from exported terms alone. `engaged` mirrors the sim's
    // `animals_engaged` (floor to whole animals, never below one for a party that exists) and the
    // rest is the documented whole-animal quantiser.
    let room = (herd.biomass - floor * herd.carrying_capacity).max(0.0);
    let carry = LONE_HUNTER as f32 * herd.per_worker_biomass;
    let compose = |reachable: f32| -> f32 {
        let affordable = (room / herd.body_mass).floor();
        let carryable = (carry / herd.body_mass).floor();
        let killed = affordable.min(carryable.max(1.0)).min(reachable);
        (killed * herd.body_mass).min(carry) * herd.provisions_per_biomass
    };
    let engaged = (LONE_HUNTER as f32 * herd.engage_rate).floor().max(1.0);
    let with_reach = compose(engaged);
    let carry_only = compose(f32::INFINITY);

    assert!(
        (with_reach - paid).abs() <= YIELD_EPSILON,
        "composed WITH the engagement term ({with_reach}) must equal the exported take ({paid})"
    );
    assert!(
        carry_only > with_reach * 2.0,
        "…and composing without it must be the overstatement this field exists to close: \
         {carry_only} vs {with_reach}"
    );
    assert!(
        paid > 0.0,
        "liveness: the harness must actually take something, or both readings are zero"
    );
}

/// **13. A PEN publishes no engagement stage — a penned animal is not stalked.**
///
/// The no-regress half of the pair above, and the one case where `0` on the wire is the *correct*
/// reading rather than a missing field: a reader must treat `<= 0` as unbounded and drop the term,
/// exactly as `fauna::hunt_engage_workers` does. Asserted beside a wild herd of the same species so
/// the `0` is demonstrably the pen's answer and not the field being dead.
#[test]
fn a_penned_herd_publishes_no_engagement_stage() {
    let (mut app, id, _pos) = headless_with_species(SMALL_BODIED_SPECIES);
    reveal_herd(&mut app, &id);
    recapture_snapshot_in_place(&mut app.world);
    let wild = exported_herd(&app, &id).engage_rate;
    assert!(
        wild > 0.0,
        "liveness: the same species reads a real rate while it is wild, got {wild}"
    );

    {
        // `headless_with_species` re-speciates the herd's yield terms, not its husbandry ceiling, so
        // the fixture states the roster's ceiling for the species under test before penning it —
        // `corral_at` gates on `can_pen()` and refuses (loudly) otherwise.
        let ceiling = app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .species_by_display(SMALL_BODIED_SPECIES)
            .expect("the species is in the shipped roster")
            .husbandry_ceiling;
        let pos = {
            let registry = app.world.resource::<HerdRegistry>();
            registry
                .find(&id)
                .expect("the herd is on the map")
                .position()
        };
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.husbandry_ceiling = ceiling;
        assert!(
            herd.corral_at(pos, &core_sim::LadderConfig::builtin()),
            "the fixture herd pens"
        );
    }
    app.world.run_system_once(spawn_initial_herds);
    recapture_snapshot_in_place(&mut app.world);

    assert_eq!(
        exported_herd(&app, &id).engage_rate,
        0.0,
        "a pen has no engagement stage — the wire's finite reading of unbounded"
    );
}

/// **14. `workersNeeded` counts the hands that can REACH the animals, not only the ones that can
/// carry them.**
///
/// The mirror defect of #12, in the opposite direction and on the same panel: `hunt_haul_workers`
/// sized the crew as a pure carry question, so a Wild Fowl herd with hundreds of head standing above
/// its floor reported a crew of two — *"more workers would be idle"* about the very hands the take
/// was short of. Engagement is a **third unit** (animals reachable per hunter, beside biomass carried
/// and heads minded) and belongs in the same `max()`.
///
/// Asserted on the **exported** assignment row, and paired both ways: the light-bodied herd's count
/// must exceed its own haul crew (the new term is live and binding), while the heavy-bodied one must
/// still be its haul crew (the old term is not dead). Plus the standing invariant — a row may not
/// report *overstaffed* and *understaffed* at once.
///
/// **The reach crew is re-derived at the band's OWN retreat**, through the same `stay_fraction` seam
/// the take is priced with — the quarry's `wariness` against the assignment's default kit's
/// `dispersion`. This harness holds wariness at its identity (`headless_with_species`, so a
/// `forecast == actual` pin is not asserting that one draw equals a mean), so that term resolves to
/// `1.0` here and the two units below are the only thing this case measures. The retreat's own effect
/// on the exported crew is pinned by
/// [`the_exported_crew_pays_for_the_retreat`], on the shipped roster's wariness.
#[test]
fn the_exported_crew_counts_the_hands_that_can_reach_the_herd() {
    let mut saw_reach_bound = false;
    let mut saw_carry_bound = false;
    for species in [SMALL_BODIED_SPECIES, HEAVY_BODIED_SPECIES] {
        let (mut app, id, pos) = headless_with_species(species);
        reveal_herd(&mut app, &id);
        let floor = MSY_BIOMASS_FRACTION;
        let (haul, reach, stay) = {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();

            let equipment = app
                .world
                .resource::<core_sim::EquipmentConfigHandle>()
                .get();
            let registry = app.world.resource::<HerdRegistry>();
            let herd = registry.find(&id).expect("the herd is on the map");
            let ceiling = core_sim::hunt_escapement_ceiling(
                floor,
                herd.biomass,
                core_sim::herd_capacity(herd, &fauna),
            );
            // The band's own kit — `spawn_resident_hunters` names none, so the Hunt job default is
            // what `LaborAssignment::kit_choice` resolves — at zero wear, which is what a
            // freshly-spawned fixture band carries.
            let stay = core_sim::stay_fraction(
                fauna.wariness_for(&herd.species),
                equipment.dispersion(
                    &equipment.default_kit(core_sim::KitJob::Hunt),
                    &core_sim::BandEquipment::start_stocked(&core_sim::EquipmentConfig::builtin()),
                ),
            );
            (
                hunt_haul_workers(ceiling, herd.body_mass, equipped_haul_rate()),
                hunt_engage_workers(
                    ceiling,
                    herd.body_mass,
                    fauna.engage_rate_for(&herd.species),
                    stay,
                ),
                stay,
            )
        };
        assert_eq!(
            stay, NOTHING_BREAKS_OFF,
            "{species}: this harness holds the retreat at its identity, so the sizing below is the \
             two units and nothing else"
        );
        let band = spawn_resident_hunters(&mut app, pos, &id, floor, LONE_HUNTER);
        // ⛔ **THE HAUL IS WALKED OVER THE SLEDS THE BAND OWNS** (`docs/plan_site_crews.md` §4): the
        // count is crew-independent, so it reads the band's stock — a hand past the sleds it holds
        // hauls at the bare rate. A count that sledded every hand it named would invite haulers the
        // band could never equip.
        let haul = {
            // The band's own ledger, or the one the turn falls back to for a band spawned with none.
            let equipment = app
                .world
                .resource::<core_sim::EquipmentConfigHandle>()
                .get();
            let sleds = app
                .world
                .get::<core_sim::BandEquipment>(band)
                .cloned()
                .unwrap_or_else(|| {
                    core_sim::BandEquipment::start_stocked_for(&equipment, LONE_HUNTER as f32)
                })
                .live_units("sled", &equipment) as f32;
            let bare = core_sim::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity;
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            let registry = app.world.resource::<HerdRegistry>();
            let herd = registry.find(&id).expect("the herd is on the map");
            let ceiling = core_sim::hunt_escapement_ceiling(
                floor,
                herd.biomass,
                core_sim::herd_capacity(herd, &fauna),
            );
            let peak = ((ceiling / herd.body_mass).floor() + 1.0) * herd.body_mass;
            let armed = sleds * equipped_haul_rate();
            if peak <= armed {
                haul
            } else {
                (sleds + (peak - armed) / bare).ceil() as u32
            }
        };
        app.world.run_system_once(advance_labor_allocation);
        recapture_snapshot_in_place(&mut app.world);
        let row = exported_row(&app, band);

        assert!(
            haul > 0 && reach > 0,
            "{species}: liveness — both crew terms must be real counts ({haul} haul, {reach} reach)"
        );
        assert_eq!(
            row.workers_needed,
            haul.max(reach),
            "{species}: the exported crew is the larger of the two jobs"
        );
        if reach > haul {
            saw_reach_bound = true;
        } else {
            saw_carry_bound = true;
        }
        // **The two staffing signals may not contradict each other** — the invariant the haul term
        // was sized off the ceiling to preserve, restated with the third term in the `max()`.
        assert!(
            !(LONE_HUNTER > row.workers_needed && row.wasted_yield > 0.0),
            "{species}: a row cannot say drop workers and add workers at once ({row:?})"
        );
    }
    assert!(
        saw_reach_bound && saw_carry_bound,
        "both units must bind somewhere in the roster: reach={saw_reach_bound} \
         carry={saw_carry_bound}"
    );
}

/// The retreat's identity — a quarry that never breaks off, which is what
/// `FaunaConfigHandle::hold_wariness_at_zero` puts the whole roster at.
const NOTHING_BREAKS_OFF: f32 = 1.0;

/// **15. `workersNeeded` counts the hands that bring the animals DOWN, not the ones that get near
/// them.**
///
/// A crew was sized on the raw reach (`engage_rate × dip`) while the take beside it was priced
/// through the retreat as well, so on a Wild Boar herd the compose sheet's *clear it now* target and
/// the stepper cap it was capped by named two different crews — the sheet asking for hands the panel
/// refused to assign. The retreat prices both now: a party that keeps three animals in four needs
/// four-thirds the hands to draw the same stock down.
///
/// Run on the **shipped** wariness (this file's fixture otherwise holds it at its identity, so #14
/// deliberately cannot see this), against the same seam the sim uses, and paired sharply: the
/// exported crew must be the retreat-aware count AND strictly above the retreat-blind one.
#[test]
fn the_exported_crew_pays_for_the_retreat() {
    let (mut app, id, pos) = headless_with_species(WARY_SPECIES);
    // **Undo the harness's retreat hold** — this case is about the retreat, so it needs the roster's
    // authored wariness rather than the identity every other case in this file pins against.
    *app.world.resource_mut::<FaunaConfigHandle>() = FaunaConfigHandle::default();
    reveal_herd(&mut app, &id);
    let floor = MSY_BIOMASS_FRACTION;

    let (retreat_aware, retreat_blind, stay) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let equipment = app
            .world
            .resource::<core_sim::EquipmentConfigHandle>()
            .get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(&id).expect("the herd is on the map");
        let ceiling = core_sim::hunt_escapement_ceiling(
            floor,
            herd.biomass,
            core_sim::herd_capacity(herd, &fauna),
        );
        // The band's own kit — `spawn_resident_hunters` names none, so the Hunt job default is what
        // `LaborAssignment::kit_choice` resolves — at zero wear, which is what a freshly-spawned
        // fixture band carries. The species' bare `1 − wariness` would be a second spelling of the
        // retreat that a kit could silently move away from.
        let stay = core_sim::stay_fraction(
            fauna.wariness_for(&herd.species),
            equipment.dispersion(
                &equipment.default_kit(core_sim::KitJob::Hunt),
                &core_sim::BandEquipment::start_stocked(&core_sim::EquipmentConfig::builtin()),
            ),
        );
        let engage = |stay| {
            hunt_engage_workers(
                ceiling,
                herd.body_mass,
                fauna.engage_rate_for(&herd.species),
                stay,
            )
        };
        (engage(stay), engage(NOTHING_BREAKS_OFF), stay)
    };
    assert!(
        stay < NOTHING_BREAKS_OFF,
        "liveness: the shipped quarry must actually retreat, or this case measures nothing (stay \
         {stay})"
    );
    assert!(
        retreat_aware > retreat_blind,
        "the retreat must COST hands: {retreat_aware} vs the raw reach's {retreat_blind}"
    );

    let band = spawn_resident_hunters(&mut app, pos, &id, floor, LONE_HUNTER);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
    let row = exported_row(&app, band);

    assert_eq!(
        row.workers_needed, retreat_aware,
        "the exported crew must be sized on what the party puts DOWN ({retreat_aware}), not on what \
         it gets near ({retreat_blind})"
    );
}

/// A wary, light-bodied quarry — the reach term binds (so the exported crew *is* the engagement
/// count) and the shipped `wariness 0.65` is high enough that the retreat moves it by a lot.
const WARY_SPECIES: &str = SMALL_BODIED_SPECIES;
