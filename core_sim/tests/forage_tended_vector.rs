//! **The full yield vector at rung 2** — a completed Tended Patch pays food, fodder *and* materials
//! from the same take (issue #427, `docs/plan_flora_roster.md` §3).
//!
//! §3's spine is unconditional: *a harvest* of `B` biomass pays `B × yield.*` into every account it
//! names. It was only ever implemented inside the Field (rung 3) branch, so a tended patch committed
//! to a cash crop (`grapevine`/`cotton`/`flax`/`tobacco`/`tea`, all `provisions_per_biomass: 0`) or
//! to `hay_grass` produced **nothing at all** while still being drawn down at full MSY every turn.
//! These tests pin the routings and, just as importantly, the way the fix could go wrong: no harvest
//! may be credited twice.
//!
//! **The trade-goods account these tests were written around is retired (arc #527)**, and a cash
//! crop's payoff is its **materials** — cotton fibre, tobacco leaf, grapes. The claims that survived
//! are restated on that account; the ones that were about the account itself are named in the
//! gravestone below rather than silently dropped.
//!
//! Every assertion runs against the sim's own payoff functions or the published `SourceYield` row —
//! never a re-derivation of their arithmetic (the §4.3 rule).

/// **The shipped EQUIPPED gather rate** — what a kitted crew carries, off the baskets' own tier.
/// `labor_config`'s `forage.per_worker_biomass_capacity` is the *bare-handed* baseline since quality
/// tiers landed, so a fixture that wants "an ordinary band" asks the item table.
fn equipped_gather_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::ForageCarry,
        core_sim::LaborConfig::builtin()
            .forage
            .per_worker_biomass_capacity,
    )
}

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, commit_fodder_payoff, patch_provisions_per_biomass, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_forage, spawn_initial_world, tended_take_fodder,
    tile_flora_composition, tile_forage_capacity, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FloraConfig,
    FloraShare, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig, LaborConfigHandle,
    LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause,
    PopulationCohort, RungKey, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
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

/// **Whose ground the fixture's tended patch belongs to.** A completed meter needs an owner (the
/// accrual sets one on first progress); the harness runs one faction, so it is the same one that
/// harvests.
const PATCH_OWNER: core_sim::FactionId = core_sim::FactionId(0);

/// **Sustain's escapement floor as a fraction of capacity** — `K/2`, the MSY operating point a
/// Sustain gather holds a patch at. The seat helpers put the patch here and then run **one turn of
/// Logistics regrowth**, so it stands exactly one turn's growth above the floor: the state the real
/// turn order (regrow → take) hands the Population stage, and the one biomass at which a `Sustain`
/// take *is* the MSY skim these tests price their quotes on.
///
/// Seating it here and taking *without* the regrowth would gather exactly nothing
/// (`docs/plan_harvest_floor.md` §1) — the escapement ceiling at the floor is `0`.
const SUSTAIN_ESCAPEMENT_FLOOR: f32 = 0.5;

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
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// The richest gatherable ground on the map: the `FoodModuleTag` tile with a seeded patch whose
/// carrying capacity is highest, and whose gather season is live.
///
/// **Capacity is the selector because the take is what these tests measure.** A rung-2 harvest rides
/// `forage_take`, so every account below is `take × rate` — on thin ground a real cash harvest is a
/// sliver, and a quote compared against it inside [`EPSILON`] would read "broken" where the sim is
/// merely small. A dead season (`seasonal_weight == 0`) zeroes the worker cap and the take with it,
/// which would do the same. Neither is what is under test.
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
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch exists");
        patch.species = Some(species.to_string());
        patch.complete_cultivation(PATCH_OWNER, &core_sim::LadderConfig::builtin());
        patch.biomass = patch.carrying_capacity * SUSTAIN_ESCAPEMENT_FLOOR;
    }
    grow_one_turn(app);
}

/// One Logistics regrowth pass — the half of the turn these harvest-routing fixtures would otherwise
/// skip, and the half that puts a floor-seated patch back above its escapement floor.
fn grow_one_turn(app: &mut App) {
    app.world.run_system_once(core_sim::advance_forage_regrowth);
}

/// Leave the patch at `coord` **uncommitted** — a wild stand — at the same standing crop, so the
/// wild and tended cases differ in exactly one thing.
///
/// Its last caller went with the trade-account tests (arc #527); it is kept because "the wild twin
/// of the tended fixture" is the shape every account added to this file will want next.
#[allow(dead_code)]
fn seat_wild_patch(app: &mut App, coord: UVec2) {
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch exists");
        patch.species = None;
        patch.set_ladder_position(0.0, &core_sim::LadderConfig::builtin());
        patch.biomass = patch.carrying_capacity * SUSTAIN_ESCAPEMENT_FLOOR;
    }
    grow_one_turn(app);
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    policy: f32,
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
    floor: f32,
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
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
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
                        floor,
                        species: None,
                    },
                    workers,
                    kit: None,
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

/// **Every material on the producing band's own `LocalStore`, summed** — batches beside the
/// `FOOD`/`FODDER` keys, and **fixed-point**, so a sub-unit credit accumulates instead of rounding
/// away. (Ongoing harvest does not touch `FactionInventory` at all; that account is start-profile
/// only.)
///
/// A bare sum across materials is the right shape *for these assertions*: they ask how much stuff a
/// take banked and whether a deeper draw banks more. Which material, and what its characteristics
/// read, is `materials.rs`'s subject.
fn band_materials(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .materials()
        .flat_map(|(_, batches)| batches.values())
        .map(|batch| batch.amount.to_f32())
        .sum()
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

/// **THE PUBLISHED PER-BIOMASS RATE IS WHAT A REAL TURN CREDITS** — on both binding sides.
///
/// The wire no longer carries per-stance ceiling rows: it carries the patch's **per-biomass yield
/// vector**, and the client composes any floor's ceiling from it
/// (`docs/plan_harvest_floor.md` §5). That makes the rate the load-bearing export, so this asserts
/// it against what a real `advance_labor_allocation` turn credits — **ceiling-bound** (5000
/// foragers) and **labor-bound** (1 forager), at two floors.
///
/// The labor-bound case is the one that matters: it is where a rate that were only correct on the
/// ceiling term would come apart, which is exactly how the retired markup used to fail.
///
/// It read the retired trade rate until arc #527; it reads the **provisions** rate now, which is the
/// account that survives on every basket and is the one the client's ceiling curve is drawn from.
#[test]
fn the_published_per_biomass_rate_is_what_a_real_turn_credits_on_both_binding_sides() {
    // A tended cash crop, so the patch is a genuinely weeded basket rather than a plain stand — the
    // rung whose conversion gain the rate has to carry.
    for floor in [0.5_f32, 0.15] {
        for workers in [1_u32, FORAGE_WORKERS] {
            let mut app = spawn_world();
            let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
            seat_tended_patch(&mut app, coord, "grapevine");

            let composition = tile_composition(&app, coord);
            let labor_config = labor();
            let flora = FloraConfig::builtin();
            let seasonal = seasonal_weight(&mut app, coord);
            let patch = app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("the seated patch")
                .clone();

            // **The client's own composition**, from exactly the three terms the wire publishes:
            //   ceiling = max(0, B − floor·K) × rate      collection = workers × throughput × rate
            let rate =
                patch_provisions_per_biomass(&patch, &composition, &flora, &labor_config.forage);
            let room = (patch.biomass - floor * patch.carrying_capacity).max(0.0);
            let throughput = core_sim::forage_per_worker_biomass(equipped_gather_rate(), seasonal)
                * workers as f32;
            let expected_food =
                core_sim::forage_provisions(room.min(throughput), rate, NEUTRAL_MULTIPLIER);

            let band = spawn_forager_with_workers(&mut app, tile, coord, floor, workers);
            app.world.run_system_once(advance_labor_allocation);
            let credited = app
                .world
                .get::<PopulationCohort>(band)
                .expect("the foraging band still exists")
                .stores
                .get(FOOD)
                .to_f32();

            assert!(
                (credited - expected_food).abs() <= EPSILON * expected_food.max(1.0),
                "floor {floor} with {workers} forager(s): the published rate composes to \
                 {expected_food} food but the turn credited {credited}"
            );
            assert!(
                expected_food > 0.0,
                "floor {floor} with {workers} forager(s) must exercise a real food credit, \
                 or the assertion above is vacuous"
            );
        }
    }
}

/// **The #427 regression.** A Tended Patch committed to `grapevine` — `provisions_per_biomass: 0`,
/// paid in **grapes** — under **Sustain** credited nothing at all while being drawn down at full MSY
/// every turn: the non-food routings existed only inside the Field branch. It must now bank real
/// material into its own band store, and still (almost) no food.
///
/// **Sustain paying it is intended, not a leak.** Rung 2 is drawn down by the ordinary gather, so its
/// non-food accounts ride the take like its food account does; the policy axis stays alive through
/// the *size* of the take rather than by gating the account.
///
/// **The food half is "less", not "none"** (#433). Tending *weeds* the tile's realized basket toward
/// the vine rather than replacing it — the volunteers are still standing — so the patch keeps paying
/// whatever food they pay, at a strictly lower rate than gathering the same tile wild. That trade-off
/// (calories surrendered for the crop, in proportion to how far you weeded) is the mechanic; "a cash
/// commitment pays exactly zero food" was an artifact of the retired concentration model.
#[test]
fn a_tended_cash_crop_under_sustain_credits_materials_and_costs_food() {
    let mut app = spawn_world();
    let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
    seat_tended_patch(&mut app, coord, "grapevine");
    let before = standing_crop(&app, coord);
    let band = spawn_forager(&mut app, tile, coord, 0.5);

    assert_eq!(
        band_materials(&app, band),
        0.0,
        "no material before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        band_materials(&app, band) > 0.0,
        "a tended grapevine patch must bank material into the band's own store, not vanish"
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
    let band = spawn_forager(&mut app, tile, coord, 0.5);

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

// ---------------------------------------------------------------------------------------------
// RETIRED: the trade-account tests (arc #527)
// ---------------------------------------------------------------------------------------------

// Nine tests in this file were about the **trade-goods** account and are deleted with it, rather
// than restated on materials, because their subjects no longer exist:
// `a_tended_staple_still_pays_food_and_now_pays_its_token_trade` (the flat token),
// `trade_income_lands_in_the_producing_bands_store_not_the_faction_stockpile` and
// `a_sub_unit_trade_income_accumulates_instead_of_vanishing` (the store's shape — a material batch
// is band-local and fixed-point by construction, and `materials.rs` owns that),
// `a_wild_deplete_sale_is_the_baskets_own_rate_marked_up`,
// `a_tended_patch_under_deplete_takes_the_markup_and_is_credited_once`,
// `a_committed_cash_crop_under_eradicate_still_credits_its_trade`,
// `the_published_cultivate_trade_quote_is_the_trade_a_tended_patch_actually_credits` and
// `a_staples_cultivate_trade_quote_is_the_flat_token_not_zero_and_not_a_cash_crops` (the crop
// picker's cash quote, which went with `FloraShareInfo::cultivate_trade_payoff`).
//
// **What they were guarding survives on the accounts that remain**: no-factor-rides-the-depth is
// pinned in food by `labor_allocation::a_deeper_floor_pays_more_because_it_takes_more` and in
// materials by `a_deeper_floor_banks_more_material_off_a_tended_cash_crop` below; the #427
// regression itself is `a_tended_cash_crop_under_sustain_credits_materials_and_costs_food`.

/// **A deeper floor banks more material off the same tended cash crop** — the pressure axis stays
/// alive at rung 2, because the credit rides the *take* rather than a managed rate. That is the
/// deliberate difference from the Field arm, whose harvest collapses the axis.
///
/// **And it earns more for one reason only: it takes more biomass.** No factor rides the depth of the
/// draw anywhere in the model (`docs/plan_harvest_floor.md` §4), so the ordering here is the
/// intensity ladder doing the work — asserted as a **ratio against the biomass ratio**, which is the
/// statement a per-depth bonus would break, and which the retired 4× markup used to fail at `0.15`.
#[test]
fn a_deeper_floor_banks_more_material_off_a_tended_cash_crop() {
    let banked = |floor: f32| {
        let mut app = spawn_world();
        let (tile, coord) = richest_tile_growing(&mut app, "grapevine");
        seat_tended_patch(&mut app, coord, "grapevine");
        let before = standing_crop(&app, coord);
        let band = spawn_forager(&mut app, tile, coord, floor);
        app.world.run_system_once(advance_labor_allocation);
        (
            band_materials(&app, band),
            take_biomass(before, &app, coord),
        )
    };
    let (peak_material, peak_take) = banked(SUSTAIN_ESCAPEMENT_FLOOR);
    let (deep_material, deep_take) = banked(0.15);

    assert!(
        peak_material > 0.0 && peak_take > 0.0,
        "a harvest at the food peak still banks material ({peak_material} off {peak_take} biomass)"
    );
    assert!(
        deep_material > peak_material,
        "drawing the stand down harder must bank more: deep {deep_material} vs peak {peak_material}"
    );
    // …and in exactly the proportion of the biomass taken. Under the retired markup this ratio was
    // ~4× the take ratio at precisely this floor.
    let material_ratio = deep_material / peak_material;
    let take_ratio = deep_take / peak_take;
    assert!(
        (material_ratio - take_ratio).abs() < 1e-2,
        "the material ordering must track the DRAWDOWN, with no factor of its own: material \
         ×{material_ratio} against take ×{take_ratio}"
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
    let band = spawn_forager(&mut app, tile, coord, 0.5);
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
