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
    advance_labor_allocation, patch_provisions_per_biomass, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_world, tended_take_fodder,
    tended_take_trade_goods, tile_flora_composition, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FloraConfig,
    FloraShare, FollowPolicy, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
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
        faction_trade_goods(&app),
        0,
        "no trade goods before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        faction_trade_goods(&app) > 0,
        "a tended grapevine patch must credit the faction trade_goods stockpile, not vanish"
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
