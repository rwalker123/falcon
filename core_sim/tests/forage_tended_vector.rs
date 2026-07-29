//! **The full yield vector at rung 2** — a completed Tended Patch pays food, fodder *and* trade from
//! the same take (issue #427, `docs/plan_flora_roster.md` §3).
//!
//! §3's spine is unconditional: *a harvest* of `B` biomass pays `B × yield.*` into three accounts. It
//! was only ever implemented inside the Field (rung 3) branch, so a tended patch committed to a cash
//! crop (`grapevine`/`cotton`/`flax`/`tobacco`/`tea`, all `provisions_per_biomass: 0`) or to
//! `hay_grass` produced **nothing in any currency** while still being drawn down at full MSY every
//! turn. These tests pin the three routings and, just as importantly, the two ways the fix could go
//! wrong: an uncommitted patch's flat `Deplete` market sale must be untouched, and a committed patch
//! must never be credited from *both* routes. They also pin that the committed route is
//! **policy-blind** — `Eradicate` credits its take like any other, because that take already pays
//! food, and the "denial, not commerce" ruling scopes to the flat wild-ground sale alone.
//!
//! Every assertion runs against the sim's own payoff functions or the published `SourceYield` row —
//! never a re-derivation of their arithmetic (the §4.3 rule).

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, patch_provisions_per_biomass, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_world, tended_take_fodder,
    tended_take_trade_goods, CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId,
    FactionInventory, FaunaConfigHandle, FloraConfig, FollowPolicy, FoodModuleTag, ForageRegistry,
    GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfig, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore,
    MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    BUILTIN_LABOR_CONFIG, FODDER, FOOD,
};

/// Whole-worker head-count on the forage — large enough that `forage_take`'s worker cap never binds,
/// so every take is **ceiling-bound** and the accounts below are a clean function of the policy.
const FORAGE_WORKERS: u32 = 5000;

/// The mechanic fixture's map seed — pinned here so a preset retune cannot silently move the tile
/// these tests stand on (the `forage_field.rs` lesson).
const PINNED_SEED: u64 = 119_304_647;

/// The quotes are taken at neutral productivity, as the shipped per-patch forecast is; the client
/// scales per band.
const NEUTRAL_MULTIPLIER: f32 = 1.0;

/// The completed rung-2 meter — a patch whose `cultivation_progress` has reached `RUNG_COMPLETE`.
const CULTIVATION_COMPLETE: f32 = 1.0;

/// **The patch's standing crop as a fraction of its capacity**: `K/2`, the MSY operating point, so a
/// Sustain gather takes the largest skim the curve offers and the credited accounts are comfortably
/// above the integer-rounding floor on the trade stockpile.
const MSY_STANDING_CROP: f32 = 0.5;

/// Float slack for a provisions/fodder quote (a chain of ~3 multiplications through the fixed-point
/// store).
const EPSILON: f32 = 1e-3;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = PINNED_SEED;
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
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// The richest gatherable ground on the map: the `FoodModuleTag` tile with a seeded patch whose
/// carrying capacity is highest, and whose gather season is live.
///
/// **Capacity is the selector because the take is what these tests measure.** A rung-2 harvest rides
/// `forage_take`, so every account below is `take × rate`, and the trade account lands in an
/// **integer** faction stockpile — on thin ground a real cash harvest rounds to `0` and the test
/// would read "broken" where the sim is merely small. A dead season (`seasonal_weight == 0`) zeroes
/// the worker cap and the take with it, which would do the same. Neither is what is under test.
fn richest_gathered_tile(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let coord = {
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(_, module)| module.seasonal_weight > 0.0)
            .filter_map(|(tile, _)| registry.patch(tile.position))
            .max_by(|a, b| {
                a.carrying_capacity
                    .total_cmp(&b.carrying_capacity)
                    .then_with(|| b.tile.y.cmp(&a.tile.y))
                    .then_with(|| b.tile.x.cmp(&a.tile.x))
            })
            .expect("the pinned map must carry an in-season forage patch")
            .tile
    };
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// Seat the patch at `coord` as a **completed Tended Patch** committed to `species`, standing at its
/// MSY operating point. Written straight onto the registry (as `flora_f4_cash.rs` seats its Field):
/// what is under test is the *harvest routing* of a finished rung, not the build that gets there.
fn seat_tended_patch(app: &mut App, coord: UVec2, species: &str) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = Some(species.to_string());
    patch.cultivation_progress = CULTIVATION_COMPLETE;
    patch.biomass = patch.carrying_capacity * MSY_STANDING_CROP;
}

/// Leave the patch at `coord` **uncommitted** — a wild stand — at the same standing crop, so the
/// wild and tended cases differ in exactly one thing.
fn seat_wild_patch(app: &mut App, coord: UVec2) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = None;
    patch.cultivation_progress = 0.0;
    patch.biomass = patch.carrying_capacity * MSY_STANDING_CROP;
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    policy: FollowPolicy,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(FORAGE_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
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
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: patch,
                        policy,
                        species: None,
                    },
                    workers: FORAGE_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn faction_trade_goods(app: &App) -> i64 {
    app.world
        .resource::<FactionInventory>()
        .stockpile(FactionId(0))
        .and_then(|items| items.get("trade_goods"))
        .copied()
        .unwrap_or(0)
}

/// The **published** per-source trade quote — `SourceYield::trade`, the number that rides the wire as
/// `LaborAssignmentState::tradeYield`. Asserted instead of the stockpile wherever the honest credit
/// is a fraction of a trade good: the stockpile is an integer, so rounding would hide the very
/// difference under test.
fn published_trade(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band forages")
        .last_yields
        .first()
        .expect("the forage assignment has a yield row")
        .trade
}

/// The biomass the turn actually took off the patch, measured from the registry either side of the
/// run — the input every account is `rate ×`.
fn take_biomass(before: f32, app: &App, coord: UVec2) -> f32 {
    before
        - app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch exists")
            .biomass
}

fn standing_crop(app: &App, coord: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .biomass
}

/// **The #427 regression.** A Tended Patch committed to `grapevine` — `provisions_per_biomass: 0`,
/// trade-dominant — under **Sustain** credited nothing in any currency while being drawn down at full
/// MSY every turn: the fodder and trade routings existed only inside the Field branch. It must now
/// pay real trade goods into the faction stockpile, and still no food.
///
/// **Sustain paying trade is intended, not a leak.** Rung 2 is drawn down by the ordinary gather, so
/// its non-food accounts ride the take like its food account does; the policy axis stays alive
/// through the *size* of the take rather than by gating the account.
#[test]
fn a_tended_cash_crop_under_sustain_credits_trade_goods_and_no_food() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    assert_eq!(
        faction_trade_goods(&app),
        0,
        "no trade goods before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        faction_trade_goods(&app) > 0,
        "a tended grapevine patch must credit the faction trade_goods stockpile, not vanish"
    );
    let cohort = app.world.get::<PopulationCohort>(band).unwrap();
    assert!(
        cohort.stores.get(FOOD).to_f32() <= EPSILON,
        "grapevine is worthless as food: {}",
        cohort.stores.get(FOOD).to_f32()
    );
    assert_eq!(
        cohort.stores.get(FODDER),
        scalar_zero(),
        "grapevine's vector pays no fodder"
    );
    let take = take_biomass(before, &app, coord);
    assert!(
        take > 0.0,
        "the patch is gathered, not managed — it is drawn down: took {take} off {before}"
    );
}

/// **The fodder routing at rung 2.** A tended `hay_grass` patch fills the working band's `FODDER`
/// store from the same take, and pays no food — the vector does the routing, with no `role` branch.
/// The credited amount is asserted against [`tended_take_fodder`] itself, the function the sim pays
/// with.
#[test]
fn a_tended_hay_patch_credits_fodder_from_its_take() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_tended_patch(&mut app, coord, "hay_grass");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    assert!(take > 0.0, "the fixture must actually harvest something");
    let flora = FloraConfig::builtin();
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let quoted = tended_take_fodder(take, &patch, &flora, NEUTRAL_MULTIPLIER);
    assert!(quoted > 0.0, "hay's vector pays a real fodder rate");

    let cohort = app.world.get::<PopulationCohort>(band).unwrap();
    assert!(
        (cohort.stores.get(FODDER).to_f32() - quoted).abs() <= EPSILON,
        "the credited fodder must be the payoff function's own number: {} vs {quoted}",
        cohort.stores.get(FODDER).to_f32()
    );
    assert!(
        cohort.stores.get(FOOD).to_f32() <= EPSILON,
        "hay is no food: {}",
        cohort.stores.get(FOOD).to_f32()
    );
}

/// **A staple keeps its food and gains its token trade.** `wild_emmer` pays real provisions through
/// the unchanged food path (asserted against `patch_provisions_per_biomass`, THE conversion seam) and
/// now also credits the flat `0.005` trade token its vector has always carried — the number is small,
/// so it is pinned on the published `SourceYield::trade` rather than the integer stockpile.
#[test]
fn a_tended_staple_still_pays_food_and_now_pays_its_token_trade() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_tended_patch(&mut app, coord, "wild_emmer");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();

    let food = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32();
    let expected_food = take * patch_provisions_per_biomass(&patch, &flora, forage);
    assert!(expected_food > 0.0, "a grain is real food");
    assert!(
        (food - expected_food).abs() <= EPSILON,
        "the staple's food must still convert at its committed rate: {food} vs {expected_food}"
    );

    let quoted = tended_take_trade_goods(take, &patch, &flora, NEUTRAL_MULTIPLIER);
    assert!(quoted > 0.0, "a staple carries the flat trade token");
    assert!(
        (published_trade(&app, band) - quoted).abs() <= EPSILON,
        "the published trade quote must be the payoff function's number: {} vs {quoted}",
        published_trade(&app, band)
    );
}

/// **The uncommitted patch is untouched.** A wild stand's only trade route is still the `Deplete`
/// policy's species-blind `forage.market.*` sale, markup included — the roster stays economy-neutral
/// until you commit. Pinned against the shipped dials by name (not literals) on the published quote,
/// because at those dials a single wild patch's honest sale is a *fraction* of a trade good and the
/// integer stockpile would read `0` either way.
#[test]
fn an_uncommitted_patch_keeps_the_flat_market_sale() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_wild_patch(&mut app, coord);
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Deplete);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    assert!(take > 0.0, "a Deplete gather draws the stand down");
    let market = &labor().forage.market;
    let expected =
        take * market.trade_goods_per_biomass * market.trade_goods_multiplier * NEUTRAL_MULTIPLIER;
    assert!(
        (published_trade(&app, band) - expected).abs() <= EPSILON,
        "the wild Deplete sale must be unchanged: {} vs {expected}",
        published_trade(&app, band)
    );
}

/// **No double credit.** A committed Tended Patch under `Deplete` routes its trade through the
/// species vector *instead of* the flat market rate — never both. The published quote must equal the
/// crop's payoff exactly; anything larger means the market path fired alongside it.
#[test]
fn a_committed_patch_under_deplete_is_credited_once_from_its_crop() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Deplete);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    let flora = FloraConfig::builtin();
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let crop_only = tended_take_trade_goods(take, &patch, &flora, NEUTRAL_MULTIPLIER);
    assert!(crop_only > 0.0, "the fixture must earn real trade");

    let market = &labor().forage.market;
    let market_only =
        take * market.trade_goods_per_biomass * market.trade_goods_multiplier * NEUTRAL_MULTIPLIER;
    assert!(
        market_only > 0.0,
        "the flat market rate is non-zero, so a double credit would be visible"
    );

    let published = published_trade(&app, band);
    assert!(
        (published - crop_only).abs() <= EPSILON,
        "a committed patch sells at its crop's rate alone: {published} vs {crop_only}"
    );
    assert!(
        published < crop_only + market_only - EPSILON,
        "the flat market sale must NOT also fire: {published} >= {crop_only} + {market_only}"
    );
}

/// **`Eradicate` on a committed stand still pays its crop.** The committed branch is **policy-blind**
/// on purpose: an `Eradicate` gather already credits **food** out of its take, so refusing trade from
/// that same take would be the inconsistent case — the vector routes whatever was actually harvested,
/// in every currency. The surviving *"Eradicate is denial, not commerce"* ruling is about the flat
/// `forage.market.*` sale on **wild** ground, which this patch does not take; pinned here so a later
/// reading of that ruling cannot quietly re-gate a committed harvest on policy.
#[test]
fn a_committed_cash_crop_under_eradicate_still_credits_its_trade() {
    let mut app = spawn_world();
    let (tile, coord) = richest_gathered_tile(&mut app);
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Eradicate);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    assert!(take > 0.0, "an Eradicate strip harvests the stand");
    let flora = FloraConfig::builtin();
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let quoted = tended_take_trade_goods(take, &patch, &flora, NEUTRAL_MULTIPLIER);
    assert!(quoted > 0.0, "the fixture must earn real trade");
    assert!(
        (published_trade(&app, band) - quoted).abs() <= EPSILON,
        "an Eradicate of a committed crop sells at the crop's own rate: {} vs {quoted}",
        published_trade(&app, band)
    );
}

/// **`Deplete` out-earns `Sustain` on the same tended cash crop** — the policy axis stays alive at
/// rung 2, because the credit rides the *take* rather than a managed rate. This is the deliberate
/// difference from the Field arm, whose harvest collapses the axis.
#[test]
fn a_tended_cash_crop_earns_more_trade_under_deplete_than_sustain() {
    let trade_under = |policy: FollowPolicy| {
        let mut app = spawn_world();
        let (tile, coord) = richest_gathered_tile(&mut app);
        seat_tended_patch(&mut app, coord, "grapevine");
        let band = spawn_forager(&mut app, tile, coord, policy);
        app.world.run_system_once(advance_labor_allocation);
        published_trade(&app, band)
    };
    let sustain = trade_under(FollowPolicy::Sustain);
    let deplete = trade_under(FollowPolicy::Deplete);
    assert!(
        deplete > sustain,
        "drawing the stand down harder must sell more: deplete {deplete} vs sustain {sustain}"
    );
    assert!(
        sustain > 0.0,
        "a Sustain harvest of a cash crop still sells"
    );
}
