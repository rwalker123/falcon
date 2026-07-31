//! **The full yield vector at rung 2** — a completed Tended Patch pays food, fodder *and* trade from
//! the same take (issue #427, `docs/plan_flora_roster.md` §3).
//!
//! §3's spine is unconditional: *a harvest* of `B` biomass pays `B × yield.*` into three accounts. It
//! was only ever implemented inside the Field (rung 3) branch, so a tended patch committed to a cash
//! crop (`grapevine`/`cotton`/`flax`/`tobacco`/`tea`, all `provisions_per_biomass: 0`) or to
//! `hay_grass` produced **nothing in any currency** while still being drawn down at full MSY every
//! turn. These tests pin the three routings and, just as importantly, the two ways the fix could go
//! wrong: a wild staple basket's `Deplete` sale must come out numerically where it always did, and no
//! harvest may be credited trade twice. They also pin that the trade route is **policy-blind except
//! for the `Deplete` markup** (#433) — `Eradicate` credits its take like any other, because that take
//! already pays food.
//!
//! Every assertion runs against the sim's own payoff functions or the published `SourceYield` row —
//! never a re-derivation of their arithmetic (the §4.3 rule).

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, commit_fodder_payoff, commit_trade_payoff,
    patch_provisions_per_biomass, plant_policy_forecasts, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_forage, spawn_initial_world, tended_take_fodder, tended_take_trade_goods,
    tile_flora_composition, tile_forage_capacity, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FloraConfig,
    FloraShare, FollowPolicy, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, RungKey, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    BUILTIN_LABOR_CONFIG, FODDER, FOOD, TRADE_GOODS,
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
/// **And the tile must actually GROW the crop under test** (#433). A rung-2 commitment now *weeds*
/// the tile's realized basket toward its crop; a crop the tile does not grow cannot be weeded toward
/// at all, so seating one on arbitrary ground would test the "not here" no-op rather than the
/// routing. `species` therefore filters the search — which is itself the honest model: you tend what
/// grows where you stand.
fn richest_tile_growing(app: &mut App, species: &str) -> (bevy::prelude::Entity, UVec2) {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let coord = {
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(_, module)| module.seasonal_weight > 0.0)
            .filter(|(tile, _)| {
                tile_flora_composition(&flora, &labor.forage, tile, map_seed)
                    .iter()
                    .any(|entry| entry.species == species)
            })
            .filter_map(|(tile, _)| registry.patch(tile.position))
            .max_by(|a, b| {
                a.carrying_capacity
                    .total_cmp(&b.carrying_capacity)
                    .then_with(|| b.tile.y.cmp(&a.tile.y))
                    .then_with(|| b.tile.x.cmp(&a.tile.x))
            })
            .unwrap_or_else(|| {
                panic!("the pinned map must carry an in-season patch whose basket grows {species}")
            })
            .tile
    };
    drop(labor);
    drop(flora);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// **What the same tile pays in FOOD gathered wild** — the baseline a tended non-food crop must come
/// in under, because weeding a hay or cash crop up through the basket displaces the plants that were
/// feeding you. Measured on the sim's own conversion seam at the same take.
fn wild_food_rate(app: &App, coord: UVec2) -> f32 {
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let composition = tile_composition(app, coord);
    let wild = core_sim::ForagePatch::new(coord, 1.0);
    patch_provisions_per_biomass(&wild, &composition, &flora, forage)
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
    spawn_forager_with_workers(app, tile, patch, policy, FORAGE_WORKERS)
}

/// [`spawn_forager`] at an explicit head-count — the **labor-bound** half of the forecast, which the
/// file's default `FORAGE_WORKERS` (5000) deliberately cannot reach. It matters because the sim applies
/// `Deplete`'s trade markup to the *final* take, i.e. **after** the worker cap: a markup carried only
/// on the ceiling is invisible while the ceiling binds and wrong the moment labor does.
fn spawn_forager_with_workers(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    policy: FollowPolicy,
    workers: u32,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(workers as f32),
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
                    workers,
                    improvement: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// **What is growing on the tile at `coord`** — through the one `tile_flora_composition` seam every
/// rate reads, so a quote taken here is priced off the identical basket the turn paid from (#433).
fn tile_composition(app: &App, coord: UVec2) -> Vec<FloraShare> {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(entity).expect("the tile");
    tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
}

/// **This tile's gather season**, read off its `FoodModuleTag` exactly as the take path does — the
/// per-worker throughput folds it in, so a forecast taken at a different weight than the turn pays at
/// would disagree for a reason that has nothing to do with the accounts under test.
fn seasonal_weight(app: &mut App, coord: UVec2) -> f32 {
    let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
    query
        .iter(&app.world)
        .find(|(ground, _)| ground.position == coord)
        .map(|(_, module)| module.seasonal_weight)
        .expect("the fixture tile carries a food module")
}

/// Trade goods on the producing band's own `LocalStore` — the third key beside `FOOD`/`FODDER`, and
/// **fixed-point**, so a sub-unit credit accumulates instead of rounding away. (Ongoing harvest no
/// longer touches `FactionInventory` at all; that account is start-profile only.)
fn band_trade_goods(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .get(TRADE_GOODS)
        .to_f32()
}

/// The **published** per-source trade quote — `SourceYield::trade`, the number that rides the wire as
/// `LaborAssignmentState::tradeYield`. The twin of what the band store is credited, asserted where a
/// test wants the *quote* rather than the accumulated balance.
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

/// **#426 — the published per-rung vector is what a real turn CREDITS, and on BOTH binding sides.**
///
/// The whole point of the row carrying a per-worker triple beside its ceiling is that the client
/// composes `min(workers × per_worker, ceiling)`, and on the plant web one account's rate is
/// **policy-dependent**: `Deplete` multiplies trade by `market.trade_goods_multiplier`. The sim applies
/// that markup to the **final take** — after the worker cap — so a positive constant only factors out
/// of the `min` if it is present on *both* terms. A markup folded into the ceiling alone is invisible
/// while the ceiling binds and understates by the full multiplier the moment labor binds, which on a
/// forage patch (`Deplete` ceiling = `0.20 × biomass`) is the common case, not the rare one.
///
/// So this runs a real `advance_labor_allocation` turn against a **trade-dominant** crop and asserts
/// the faction's credited trade goods equal the row's own composition — **ceiling-bound** (5000
/// foragers) and **labor-bound** (1 forager), under Sustain (no markup) and Deplete (markup). Four
/// cases; the labor-bound Deplete one is the case the design exists for and the only one that fails
/// against a ceiling-only markup.
#[test]
fn the_published_forage_rung_row_is_what_a_real_turn_credits_on_both_binding_sides() {
    // A trade-dominant crop, so a mis-scaled markup cannot hide inside the epsilon.
    for policy in [FollowPolicy::Sustain, FollowPolicy::Deplete] {
        for workers in [1_u32, FORAGE_WORKERS] {
            let mut app = spawn_world();
            let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
            seat_tended_patch(&mut app, coord, "grapevine");

            // The row exactly as the capture publishes it: same patch, same basket, same neutral
            // multiplier, same seasonal weight the take will read.
            let composition = tile_composition(&app, coord);
            let labor_config = labor();
            let flora = FloraConfig::builtin();
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            let seasonal = seasonal_weight(&mut app, coord);
            let patch = app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("the seated patch")
                .clone();
            let row = plant_policy_forecasts(
                &patch,
                &composition,
                &labor_config.forage,
                &flora,
                &ladder,
                seasonal,
                NEUTRAL_MULTIPLIER,
            )
            .into_iter()
            .find(|rung| rung.policy == policy)
            .expect("every forage rung has a row");

            // THE CLIENT'S OWN COMPOSITION, per component — not a re-derivation of the sim's take.
            let expected_trade =
                (workers as f32 * row.per_worker.trade_goods).min(row.ceiling.trade_goods);

            let band = spawn_forager_with_workers(&mut app, tile, coord, policy, workers);
            app.world.run_system_once(advance_labor_allocation);
            let credited = band_trade_goods(&app, band);

            assert!(
                (credited - expected_trade).abs() <= EPSILON * expected_trade.max(1.0),
                "{policy:?} with {workers} forager(s): the published row composes to \
                 {expected_trade} trade but the turn credited {credited}. \
                 (per_worker {}, ceiling {})",
                row.per_worker.trade_goods,
                row.ceiling.trade_goods,
            );
            assert!(
                expected_trade > 0.0,
                "{policy:?} with {workers} forager(s) must exercise a real trade credit, \
                 or the assertion above is vacuous"
            );
        }
    }
}

/// **The markup is on the rung, not on the rate** — `Deplete`'s trade row must be exactly
/// `market.trade_goods_multiplier` × `Sustain`'s *at equal biomass*, in **both** the ceiling and the
/// per-worker term. Pinned as a RATIO rather than against literals, so a basket retune cannot move it
/// and the multiplier is named by the config rather than by a magic number.
///
/// Per-worker is the load-bearing half: it is invariant across the rows for food and fodder, and the
/// one component that legitimately varies by policy.
#[test]
fn the_deplete_rung_marks_trade_up_in_both_the_ceiling_and_the_per_worker_term() {
    let mut app = spawn_world();
    let (_tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let composition = tile_composition(&app, coord);
    let labor_config = labor();
    let flora = FloraConfig::builtin();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let seasonal = seasonal_weight(&mut app, coord);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("the seated patch")
        .clone();
    let rows = plant_policy_forecasts(
        &patch,
        &composition,
        &labor_config.forage,
        &flora,
        &ladder,
        seasonal,
        NEUTRAL_MULTIPLIER,
    );
    let row = |policy: FollowPolicy| {
        rows.iter()
            .find(|rung| rung.policy == policy)
            .copied()
            .expect("every forage rung has a row")
    };
    let sustain = row(FollowPolicy::Sustain);
    let deplete = row(FollowPolicy::Deplete);
    let markup = labor_config.forage.market.trade_goods_multiplier;

    assert!(
        (deplete.per_worker.trade_goods - sustain.per_worker.trade_goods * markup).abs() < EPSILON,
        "Deplete's per-worker trade must be {markup}x Sustain's: {} vs {}",
        deplete.per_worker.trade_goods,
        sustain.per_worker.trade_goods,
    );
    // Food and fodder carry NO markup — the multiplier is a trade-account concept, and a per-worker
    // throughput is otherwise policy-blind.
    assert!(
        (deplete.per_worker.provisions - sustain.per_worker.provisions).abs() < EPSILON,
        "the per-worker FOOD rate must be policy-blind: {} vs {}",
        deplete.per_worker.provisions,
        sustain.per_worker.provisions,
    );
    assert!(
        (deplete.per_worker.fodder - sustain.per_worker.fodder).abs() < EPSILON,
        "the per-worker FODDER rate must be policy-blind"
    );
}

/// **The #427 regression.** A Tended Patch committed to `grapevine` — `provisions_per_biomass: 0`,
/// trade-dominant — under **Sustain** credited nothing in any currency while being drawn down at full
/// MSY every turn: the fodder and trade routings existed only inside the Field branch. It must now
/// pay real trade goods into its own band store, and still no food.
///
/// **Sustain paying trade is intended, not a leak.** Rung 2 is drawn down by the ordinary gather, so
/// its non-food accounts ride the take like its food account does; the policy axis stays alive
/// through the *size* of the take rather than by gating the account.
///
/// **The food half is now "less", not "none"** (#433). Tending *weeds* the tile's realized basket
/// toward the vine rather than replacing it — the volunteers are still standing — so the patch keeps
/// paying whatever food they pay, at a strictly lower rate than gathering the same tile wild. That
/// trade-off (calories surrendered for cash, in proportion to how far you weeded) is the mechanic;
/// "a cash commitment pays exactly zero food" was an artifact of the retired concentration model.
#[test]
fn a_tended_cash_crop_under_sustain_credits_trade_goods_and_costs_food() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    assert_eq!(
        band_trade_goods(&app, band),
        0.0,
        "no trade goods before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        band_trade_goods(&app, band) > 0.0,
        "a tended grapevine patch must credit the band's own trade_goods store, not vanish"
    );
    let take = take_biomass(before, &app, coord);
    assert!(
        take > 0.0,
        "the patch is gathered, not managed — it is drawn down: took {take} off {before}"
    );
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let composition = tile_composition(&app, coord);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let tended_food_rate = patch_provisions_per_biomass(&patch, &composition, &flora, forage);
    assert!(
        tended_food_rate < wild_food_rate(&app, coord),
        "weeding a food-less vine up through the basket must COST calories: tended \
         {tended_food_rate} vs wild {}",
        wild_food_rate(&app, coord)
    );
    let cohort = app.world.get::<PopulationCohort>(band).unwrap();
    assert_eq!(
        cohort.stores.get(FODDER),
        scalar_zero(),
        "and the vine's basket grows no fodder crop"
    );
}

/// **The fodder routing at rung 2.** A tended `hay_grass` patch fills the working band's `FODDER`
/// store from the same take — the vector does the routing, with no `role` branch. The credited amount
/// is asserted against [`tended_take_fodder`] itself, the function the sim pays with.
///
/// **And it costs calories** (#433): weeding hay up through the basket displaces the plants that were
/// feeding you, so the patch's food rate falls below the same tile's wild one. It does not fall to
/// zero — the volunteers are still standing — which is the difference from the retired concentration
/// model, where a commitment replaced the whole stand.
///
/// **Rungs 2 and 3 are UNGATED on Foddering**: committing a patch to `hay_grass` *is* the bid. Only a
/// wild patch's hay credit reads the capability (see `forage_basket_reweight.rs`), and this fixture's
/// faction knows nothing at all — so the credit landing here *is* the ungated ruling.
#[test]
fn a_tended_hay_patch_credits_fodder_from_its_take() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "hay_grass");
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
    let composition = tile_composition(&app, coord);
    let quoted = tended_take_fodder(
        take,
        &patch,
        &composition,
        &flora,
        &labor().forage,
        NEUTRAL_MULTIPLIER,
    );
    assert!(quoted > 0.0, "hay's vector pays a real fodder rate");

    let cohort = app.world.get::<PopulationCohort>(band).unwrap();
    assert!(
        (cohort.stores.get(FODDER).to_f32() - quoted).abs() <= EPSILON,
        "the credited fodder must be the payoff function's own number: {} vs {quoted}",
        cohort.stores.get(FODDER).to_f32()
    );
    let patch_food_rate =
        patch_provisions_per_biomass(&patch, &composition, &flora, &labor().forage);
    assert!(
        patch_food_rate < wild_food_rate(&app, coord),
        "weeding hay up through the basket must COST calories: tended {patch_food_rate} vs wild {}",
        wild_food_rate(&app, coord)
    );
}

/// **A staple keeps its food and gains its token trade.** `wild_emmer` pays real provisions through
/// the unchanged food path (asserted against `patch_provisions_per_biomass`, THE conversion seam) and
/// now also credits the flat `0.005` trade token its vector has always carried — the number is small,
/// so it is pinned on the published `SourceYield::trade` rather than the integer stockpile.
#[test]
fn a_tended_staple_still_pays_food_and_now_pays_its_token_trade() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "wild_emmer");
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
    let composition = tile_composition(&app, coord);
    let expected_food = take * patch_provisions_per_biomass(&patch, &composition, &flora, forage);
    assert!(expected_food > 0.0, "a grain is real food");
    assert!(
        (food - expected_food).abs() <= EPSILON,
        "the staple's food must still convert at its committed rate: {food} vs {expected_food}"
    );

    let quoted = tended_take_trade_goods(
        take,
        &patch,
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
    );
    assert!(quoted > 0.0, "a staple carries the flat trade token");
    assert!(
        (published_trade(&app, band) - quoted).abs() <= EPSILON,
        "the published trade quote must be the payoff function's number: {} vs {quoted}",
        published_trade(&app, band)
    );
}

// ---------------------------------------------------------------------------------------------
// Trade goods are a BAND-LOCAL store (issue #381 follow-up)
// ---------------------------------------------------------------------------------------------

/// **A band holds the trade goods it produced, and the faction stockpile does not move.**
///
/// Trade goods used to be a faction-global integer account, so a band's harvest teleported into a
/// number no place on the map owned — which made "a trade network connects two settlements" an
/// unrepresentable idea. They are now a third key on the *same* `LocalStore` as `FOOD`/`FODDER`, so
/// the goods sit where they were produced and `balance_supply_networks` (commodity-generic) shares
/// them exactly as far as `SupplyNetworkConfig.reach_tiles` allows.
///
/// `FactionInventory` survives for the **start profile** alone — `seed_starting_inventory` writes the
/// opening grant, the Startup-only `apply_trade_goods_bonus` drains it into the trade-link openness
/// bonus — so this asserts both halves: the band gains, and the faction account is untouched.
#[test]
fn trade_income_lands_in_the_producing_bands_store_not_the_faction_stockpile() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    app.world.run_system_once(advance_labor_allocation);

    assert!(
        band_trade_goods(&app, band) > 0.0,
        "the working band must hold the trade goods it produced"
    );
    assert_eq!(
        app.world
            .resource::<FactionInventory>()
            .stockpile(FactionId(0))
            .and_then(|items| items.get("trade_goods").copied())
            .unwrap_or(0),
        0,
        "no ongoing harvest may credit the start-profile faction stockpile"
    );
}

/// Turns of sub-unit income run in
/// [`a_sub_unit_trade_income_accumulates_instead_of_vanishing`]. Enough that the total clears a whole
/// trade good even though no single turn does, so "it accumulated" cannot be confused with "one turn
/// happened to be big".
const SUB_UNIT_ACCUMULATION_TURNS: u32 = 12;

/// The per-turn credit that the retired `.round() as i64` silently discarded: anything under half a
/// trade good rounded to `0`, forever.
const ROUNDING_FLOOR: f32 = 0.5;

/// **A sub-unit trade income ACCUMULATES instead of vanishing** — the live playtest bug.
///
/// The ongoing trade credits used to be `round()`ed into `FactionInventory`'s `i64` stockpile, so a
/// source paying (say) `0.04` trade/turn contributed **exactly nothing, every turn, forever**, while
/// the UI honestly reported `+0.04 /turn` off `SourceYield::trade`. A `LocalStore` is fixed-point
/// precisely so small per-turn flows accumulate, and the credit now goes in through `scalar_from_f32`
/// like `FOOD`/`FODDER` beside it.
///
/// A tended **staple** is the natural fixture: every staple carries the same flat `0.005` trade token,
/// so one turn's honest sale is a small fraction of a good — the exact shape the rounding erased. The
/// test asserts the per-turn quote really is sub-unit (or it would be vacuous) and that the running
/// balance grows past a whole good regardless.
#[test]
fn a_sub_unit_trade_income_accumulates_instead_of_vanishing() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "wild_emmer");
    seat_tended_patch(&mut app, coord, "wild_emmer");
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    app.world.run_system_once(advance_labor_allocation);
    let first_turn = band_trade_goods(&app, band);
    assert!(
        first_turn > 0.0 && first_turn < ROUNDING_FLOOR,
        "the fixture must pay a SUB-UNIT trade income or this test is vacuous: {first_turn}/turn"
    );

    for _ in 1..SUB_UNIT_ACCUMULATION_TURNS {
        app.world.run_system_once(advance_labor_allocation);
    }

    let total = band_trade_goods(&app, band);
    assert!(
        total > first_turn,
        "every turn's sub-unit income must add to the store, not round away: \
         {total} after {SUB_UNIT_ACCUMULATION_TURNS} turns vs {first_turn} after one"
    );
    assert_eq!(
        app.world
            .resource::<FactionInventory>()
            .stockpile(FactionId(0))
            .and_then(|items| items.get("trade_goods").copied())
            .unwrap_or(0),
        0,
        "…and none of it leaked into the start-profile faction stockpile"
    );
}

/// **A wild `Deplete` sale is the basket's own rate, marked up** (#433) — and on a **staple-only**
/// basket that is numerically what the retired flat `market.trade_goods_per_biomass` (0.005) always
/// paid, because every staple carries exactly that token. This is the pin that says the retirement
/// moved no balance: only baskets holding a cash crop are supposed to move.
///
/// Asserted on the published quote rather than the integer stockpile, because at these dials a single
/// wild patch's honest sale is a *fraction* of a trade good and the stockpile would read `0` either
/// way.
#[test]
fn a_wild_deplete_sale_is_the_baskets_own_rate_marked_up() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "wild_emmer");
    seat_wild_patch(&mut app, coord);
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Deplete);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    assert!(take > 0.0, "a Deplete gather draws the stand down");
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let composition = tile_composition(&app, coord);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let rate = tended_take_trade_goods(
        take,
        &patch,
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
    );
    let expected = rate * forage.market.trade_goods_multiplier;
    assert!(
        (published_trade(&app, band) - expected).abs() <= EPSILON,
        "the wild Deplete sale must be the basket rate times the markup: {} vs {expected}",
        published_trade(&app, band)
    );

    // The retirement moved no balance on a staple basket: every staple's token is one number, so
    // pricing the same take at that token reproduces the published sale exactly.
    let token = composition
        .iter()
        .map(|entry| flora.species[&entry.species].yield_.trade_goods_per_biomass)
        .fold(f32::MIN, f32::max);
    let staple_only = composition.iter().all(|entry| {
        (flora.species[&entry.species].yield_.trade_goods_per_biomass - token).abs() <= f32::EPSILON
    });
    if staple_only {
        let retired_flat_rate = take * token * forage.market.trade_goods_multiplier;
        assert!(
            (published_trade(&app, band) - retired_flat_rate).abs() <= EPSILON,
            "a staple-only basket's wild Deplete sale must be numerically what the retired flat \
             rate paid: {} vs {retired_flat_rate}",
            published_trade(&app, band)
        );
    }
}

/// **The `Deplete` markup rides the basket at rung 2 too, and credits exactly once** (#433). A
/// tended patch under `Deplete` is credited `take × basket trade rate × trade_goods_multiplier` —
/// the *same* expression a wild `Deplete` uses, because the markup is a **policy** concept, not a
/// rung one. This changes rung 2's shipped behaviour, which used to pin no-markup-when-committed.
///
/// The "credited once" half is the reason there is a single expression at all: with the retired flat
/// wild-ground sale gone there is no second route left to fire alongside it, so the published quote
/// must land on the markup exactly — neither the bare rate (markup dropped) nor rate + markup (both
/// routes fired).
#[test]
fn a_tended_patch_under_deplete_takes_the_markup_and_is_credited_once() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Deplete);

    app.world.run_system_once(advance_labor_allocation);

    let take = take_biomass(before, &app, coord);
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let composition = tile_composition(&app, coord);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch exists")
        .clone();
    let bare_rate = tended_take_trade_goods(
        take,
        &patch,
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
    );
    assert!(bare_rate > 0.0, "the fixture must earn real trade");
    let markup = forage.market.trade_goods_multiplier;
    assert!(
        markup > 1.0,
        "the markup must be visible for this to prove anything"
    );

    let published = published_trade(&app, band);
    let expected = bare_rate * markup;
    assert!(
        (published - expected).abs() <= EPSILON,
        "a tended Deplete sells at the basket rate times the markup: {published} vs {expected}"
    );
    assert!(
        published < bare_rate + expected - EPSILON,
        "and it is credited ONCE — {published} would be a double credit against \
         {bare_rate} + {expected}"
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
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
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
    let composition = tile_composition(&app, coord);
    let quoted = tended_take_trade_goods(
        take,
        &patch,
        &composition,
        &flora,
        &labor().forage,
        NEUTRAL_MULTIPLIER,
    );
    assert!(quoted > 0.0, "the fixture must earn real trade");
    assert!(
        (published_trade(&app, band) - quoted).abs() <= EPSILON,
        "an Eradicate of a committed crop sells at the basket's own rate, unmarked-up: {} vs \
         {quoted}",
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
        let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
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

/// **THE QUOTE IS THE PAYOUT, at rung 2, in the trade account** (issue #419) — the §4.3 rule applied
/// to the crop picker's Cultivate row.
///
/// #427/#433 made a tended cash crop *pay* trade; nothing *quoted* it. `FloraShareInfo` carried only
/// `sowTradePayoff`, a **Field** number, so the picker's Cultivate row advertised a managed rate on the
/// full standing crop for a rung that pays an MSY skim off a merely-weeded basket — cotton read
/// `10.2 trade` beside a rung that pays a fraction of it. `cultivateTradePayoff` is that rung's own
/// number, and this pins it against what the turn actually credits.
///
/// **Why the two are exactly equal here:** the fixture seats the patch at [`MSY_STANDING_CROP`] (`K/2`,
/// the MSY operating point) and staffs it past any worker cap, so a `Sustain` take *is* the MSY skim on
/// the tended curve — which is the take `tended_msy_take` prices the quote on. Anywhere else the two
/// legitimately differ (that is what the policy axis *is*); here they must agree to the float.
#[test]
fn the_published_cultivate_trade_quote_is_the_trade_a_tended_patch_actually_credits() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
    let composition = tile_composition(&app, coord);
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let capacity = {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .expect("tile entity resolves");
        let ground = app.world.get::<Tile>(entity).expect("the tile");
        tile_forage_capacity(forage, ground)
    };

    // The picker's Cultivate-row quote, through the same public seam the snapshot builds it with.
    let quoted = commit_trade_payoff(
        coord,
        capacity,
        "grapevine",
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
        RungKey::PlantTended,
    );
    assert!(
        quoted > 0.0,
        "a rung-2 cash crop must QUOTE trade, not preview as 0 while being paid"
    );

    app.world.run_system_once(advance_labor_allocation);
    let paid = published_trade(&app, band);
    assert!(
        (quoted - paid).abs() <= EPSILON,
        "the Cultivate row must state what the rung pays: quoted {quoted} vs credited {paid}"
    );

    // **The defect this replaces, named:** the Field figure is a materially different number, so a
    // picker that kept quoting `sowTradePayoff` on this row was not merely imprecise.
    let field_quote = commit_trade_payoff(
        coord,
        capacity,
        "grapevine",
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
        RungKey::PlantField,
    );
    assert!(
        field_quote > quoted,
        "rung 3 must out-pay rung 2 in trade as it does in food, or the Sow rung is pointless: \
         field {field_quote} vs tended {quoted}"
    );
}

/// **The fodder twin of the above** — a tended hay patch's `cultivateFodderPayoff` is the fodder the
/// turn credits, for the same reason and on the same MSY-seated fixture. Guards the half of #419 that
/// has no cash crop to make it visible: `hay_grass` climbs to `field`, so its Cultivate row was
/// quoting a hay Field's managed rate too.
#[test]
fn the_published_cultivate_fodder_quote_is_the_fodder_a_tended_patch_actually_credits() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "hay_grass");
    seat_tended_patch(&mut app, coord, "hay_grass");
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
    let composition = tile_composition(&app, coord);
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let capacity = {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .expect("tile entity resolves");
        let ground = app.world.get::<Tile>(entity).expect("the tile");
        tile_forage_capacity(forage, ground)
    };

    let quoted = commit_fodder_payoff(
        coord,
        capacity,
        "hay_grass",
        &composition,
        &flora,
        forage,
        NEUTRAL_MULTIPLIER,
        RungKey::PlantTended,
    );
    assert!(quoted > 0.0, "a rung-2 hay patch must QUOTE its fodder");

    app.world.run_system_once(advance_labor_allocation);
    let paid = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band forages")
        .stores
        .get(FODDER)
        .to_f32();
    assert!(
        (quoted - paid).abs() <= EPSILON,
        "the Cultivate row must state the hay the rung pays: quoted {quoted} vs credited {paid}"
    );
}

/// **A staple's rung-2 trade quote is small but REAL, and that is why the client cannot threshold on
/// it** (issue #419's first fault). Every staple carries `trade_goods_per_biomass: 0.005`, so the
/// "detect a cash crop purely from the payoff being > 0" test the picker used fired on all 27 of them
/// and printed every row as trade-only. The quote must stay non-zero — it is honest income — while
/// being a different order of magnitude from a cash crop's on comparable ground, which is what makes
/// rendering *both* accounts the readable answer rather than picking one.
#[test]
fn a_staples_cultivate_trade_quote_is_the_flat_token_not_zero_and_not_a_cash_crops() {
    let mut app = spawn_world();
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;
    let quote_for = |app: &mut App, species: &str, rung: RungKey| {
        let (_, coord) = richest_tile_growing(app, species);
        let composition = tile_composition(app, coord);
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .expect("tile entity resolves");
        let ground = app.world.get::<Tile>(entity).expect("the tile");
        let capacity = tile_forage_capacity(forage, ground);
        let food = core_sim::commit_payoff(
            coord,
            capacity,
            species,
            &composition,
            &flora,
            forage,
            NEUTRAL_MULTIPLIER,
            rung,
        );
        let trade = commit_trade_payoff(
            coord,
            capacity,
            species,
            &composition,
            &flora,
            forage,
            NEUTRAL_MULTIPLIER,
            rung,
        );
        (food, trade)
    };

    let (staple_food, staple_trade) = quote_for(&mut app, "wild_emmer", RungKey::PlantTended);
    assert!(
        staple_trade > 0.0,
        "a staple's flat trade token is real income, so the picker cannot treat >0 as 'cash crop'"
    );
    assert!(
        staple_food > 0.0,
        "and a staple is a FOOD crop — both accounts are non-zero on the same row"
    );

    let (cash_food, cash_trade) = quote_for(&mut app, "cotton", RungKey::PlantTended);
    assert!(
        cash_trade > staple_trade,
        "the cash crop's trade must dominate the staple's token, or the row comparison says nothing: \
         cotton {cash_trade} vs emmer {staple_trade}"
    );

    // **A rung-2 cash crop still pays FOOD, and the row has to say so** — #433's weeding model, on the
    // quote. Tending raises cotton's share to `min(1, share × tended_weeding_gain)` and leaves the
    // volunteers standing, so the basket keeps paying whatever *they* pay. It is only a sown **Field**
    // that is 100% crop and therefore pays exactly zero calories. So at rung 2 BOTH accounts are live
    // even for a cash crop, and a row that states one of them is wrong whichever one it picks.
    assert!(
        cash_food > 0.0,
        "a tended cash crop keeps its volunteers' calories (#433), so its Cultivate row has a food \
         term too: {cash_food}"
    );
    let (field_food, _) = quote_for(&mut app, "cotton", RungKey::PlantField);
    assert_eq!(
        field_food, 0.0,
        "a sown cotton Field is 100% cotton — THAT is the rung with no food number at all"
    );
}
