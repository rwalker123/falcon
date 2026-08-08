//! **Cash crops — the F4 coupling** (Flora Roster F4, `docs/plan_flora_roster.md` §6).
//!
//! The yield vector's third and final account. A cash crop (`cotton`/`flax`) is a `field`-ceiling
//! species whose vector is **trade-dominant** and whose `provisions_per_biomass` is `0`: harvesting
//! it as a Field credits the band's `trade_goods` store and (near) zero food. F4 is the exact
//! twin of F3's fodder work — the *same* managed harvest, routed by the vector's `trade_goods`
//! component with **no `role` branch**. This file pins that routing against the **loaded** configs,
//! never a literal, so a retune of a table fails the test instead of agreeing with a stale copy.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, commit_fodder_payoff, commit_payoff, commit_trade_payoff,
    generate_hydrology, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_world, tile_forage_capacity, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FloraConfig,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfig, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    RungKey, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    BUILTIN_LABOR_CONFIG, FODDER, FOOD, TRADE_GOODS,
};
use sim_runtime::TerrainType;

/// Head-count large enough that the per-worker collection cap never binds, so a Field harvest is
/// production-bound (the point of a managed rung).
const FORAGE_WORKERS: u32 = 5000;

/// The mechanic fixture's grid + seed — pinned here, immune to config edits (the `forage_field.rs`
/// lesson). Big enough to carry river-valley farmland with hydrology run.
const MECHANIC_GRID: UVec2 = UVec2::new(96, 64);
const PINNED_SEED: u64 = 119_304_647;

/// Float slack for a provisions/trade quote (a chain of ~4 multiplications).
const EPSILON: f32 = 1e-3;

/// The quotes are captured at neutral productivity (the client scales per-band), as the shipped
/// per-patch forecast is.
const QUOTE_MULTIPLIER: f32 = 1.0;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

/// The cotton share of `terrain`'s basket — cotton must actually grow there.
fn cotton_share(flora: &FloraConfig, terrain: TerrainType) -> f32 {
    flora
        .composition(terrain)
        .iter()
        .find(|entry| entry.species == "cotton")
        .unwrap_or_else(|| panic!("cotton must host {terrain:?}"))
        .share
}

// ---------------------------------------------------------------------------------------------
// Config-only: the vector routes with NO role branch (against the loaded roster).
// ---------------------------------------------------------------------------------------------

/// **A cash crop pays TRADE and nothing else; the vector routes.** On the same sowable ground, the
/// three published Field payoffs split cleanly by account: a cash crop pays trade (dominant) and `0`
/// food / `0` fodder; a grain pays food and only the flat trade *token*; hay pays fodder and `0`
/// trade. All three go through the *same* commodity-generic payoff seams — the only thing that
/// differs is which component of each species' vector is non-zero. Asserted against the LOADED
/// config, so a retune moves the test.
#[test]
fn the_yield_vector_routes_by_account_with_no_role_branch() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    // Sowable river-valley farmland (capacity 205 >= the 195 field floor) that cotton/hay/grain
    // all compete for. Cotton IS hosted on AlluvialPlain too (§10 per-tile realization keeps the
    // staples dominant on their own realized tiles); Floodplain just happens to carry all three here.
    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);

    let composition = flora.composition(terrain);
    let payoffs = |species: &str| {
        assert!(
            composition.iter().any(|entry| entry.species == species),
            "{species} must host {terrain:?}"
        );
        let food = commit_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        let fodder = commit_fodder_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        let trade = commit_trade_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        (food, fodder, trade)
    };

    // Cotton — the cash crop: worthless as food, no fodder, dominant trade.
    let (cotton_food, cotton_fodder, cotton_trade) = payoffs("cotton");
    assert!(
        cotton_food.abs() <= EPSILON,
        "a cash Field pays no food: {cotton_food}"
    );
    assert!(
        cotton_fodder.abs() <= EPSILON,
        "a cash Field pays no fodder: {cotton_fodder}"
    );
    assert!(
        cotton_trade > EPSILON,
        "a cash Field's trade payoff is dominant: {cotton_trade}"
    );

    // Wild Emmer — a grain: real food, no fodder, only the flat trade TOKEN (the 0.005 baseline).
    let (grain_food, grain_fodder, grain_trade) = payoffs("wild_emmer");
    assert!(
        grain_food > EPSILON,
        "a grain Field pays food: {grain_food}"
    );
    assert!(
        grain_fodder.abs() <= EPSILON,
        "a grain Field pays no fodder: {grain_fodder}"
    );
    assert!(
        grain_trade > 0.0 && grain_trade < cotton_trade,
        "a grain Field's trade is the negligible token, far below a cash crop's \
         ({grain_trade} vs {cotton_trade})"
    );

    // Hay Grass — a fodder crop: no food, real fodder, no trade.
    let (hay_food, hay_fodder, hay_trade) = payoffs("hay_grass");
    assert!(
        hay_food.abs() <= EPSILON,
        "a hay Field pays no food: {hay_food}"
    );
    assert!(
        hay_fodder > EPSILON,
        "a hay Field pays fodder: {hay_fodder}"
    );
    assert_eq!(
        hay_trade, 0.0,
        "a hay Field's vector pays no trade at all: {hay_trade}"
    );
}

/// **The token is exactly `K × field_dial × token_rate / wild_rate`.** Pin the grain Field's trade
/// token against the loaded dials by name, so a change to `field_provisions_per_biomass`, the
/// grain's `trade_goods_per_biomass`, or the wild `provisions_per_biomass` moves this number — this
/// is the "assert the quote against the payoff function's inputs" discipline.
///
/// **The standing crop is the tile's whole `K`** (#433): a Field neither raises nor lowers the land's
/// capacity, it only makes the whole of it one crop, so the crop's share of the wild basket does not
/// appear here at all. It used to, as a concentration factor — that was the bug.
#[test]
fn the_grain_trade_token_carries_the_field_dial_and_the_wild_baseline() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);
    let composition = flora.composition(terrain);
    assert!(
        composition
            .iter()
            .any(|entry| entry.species == "wild_emmer"),
        "emmer hosts the plain"
    );

    // The hypothetical Field the quote builds: this tile's own K at the full standing crop, its
    // basket forced to 100% the sown crop.
    let expected = capacity
        * forage.cultivation.field_provisions_per_biomass
        * (flora.species["wild_emmer"].yield_.trade_goods_per_biomass
            / forage.provisions_per_biomass)
        * QUOTE_MULTIPLIER;

    let quoted = commit_trade_payoff(
        tile,
        capacity,
        "wild_emmer",
        composition,
        &flora,
        forage,
        QUOTE_MULTIPLIER,
        RungKey::PlantField,
    );
    assert!(
        (quoted - expected).abs() <= EPSILON * expected.max(1.0),
        "the grain trade token must be biomass x field_dial x token_rate/wild_rate: \
         {quoted} vs {expected}"
    );
}

// ---------------------------------------------------------------------------------------------
// Integration: the labor arm credits the BAND's own trade_goods store.
// ---------------------------------------------------------------------------------------------

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = PINNED_SEED;
    config.grid_size = MECHANIC_GRID;
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
    generate_hydrology(&mut app.world);

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
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// The first land tile carrying a forage patch, in a totally-ordered `(y, x)` sweep — any real
/// ground will do: a Field's trade payoff reads the committed **species** off the patch, never the
/// tile's biome, so the tile only has to be somewhere a band can stand and work.
fn first_patch_tile(app: &App) -> (bevy::prelude::Entity, UVec2) {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    for y in 0..height {
        for x in 0..width {
            let coord = UVec2::new(x, y);
            let Some(entity) = app.world.resource::<TileRegistry>().index(x, y) else {
                continue;
            };
            if app.world.get::<Tile>(entity).is_some()
                && app
                    .world
                    .resource::<ForageRegistry>()
                    .patch(coord)
                    .is_some()
            {
                return (entity, coord);
            }
        }
    }
    panic!("the pinned map must carry at least one forage patch");
}

/// Turn the patch at `coord` into a completed **Field** of `species` standing at `biomass`. Written
/// straight onto the registry: what is under test is the *harvest routing* of a finished rung, not
/// the build that gets there.
fn seat_field(app: &mut App, coord: UVec2, species: &str, biomass: f32) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = Some(species.to_string());
    patch.field_progress = 1.0;
    patch.carrying_capacity = biomass;
    patch.biomass = biomass;
}

/// Turn the patch at `coord` into a completed cotton **Field** standing at `biomass`.
fn seat_cotton_field(app: &mut App, coord: UVec2, biomass: f32) {
    seat_field(app, coord, "cotton", biomass);
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
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
                    // Policy is irrelevant to a Field — the rung-3 branch resolves before the policy
                    // arms and `continue`s. Sustain is the harmless default.
                    target: LaborTarget::Forage {
                        tile: patch,
                        floor: 0.5,
                        species: None,
                    },
                    workers: FORAGE_WORKERS,
                    improvement: None,
                    kit: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// Trade goods on the producing band's own `LocalStore` — the third key beside `FOOD`/`FODDER`.
/// **Fractional**: a `LocalStore` is fixed-point, so a sub-unit harvest accumulates instead of
/// rounding away (`FactionInventory`'s `i64` stockpile no longer sees ongoing income at all).
fn band_trade_goods(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the keeper band still exists")
        .stores
        .get(TRADE_GOODS)
        .to_f32()
}

/// **A cash Field credits the band's own `trade_goods` store and (near) zero food** — and no fodder
/// at all. Trade goods are a third key on the *same* `LocalStore` as FOOD/FODDER: goods sit where
/// they were produced until a supply network reaches them.
#[test]
fn a_cash_field_credits_trade_goods_and_leaves_food_and_fodder_alone() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);
    let capacity = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile).expect("tile exists");
        tile_forage_capacity(&labor.forage, ground)
    };
    seat_cotton_field(&mut app, coord, capacity);
    let keeper = spawn_forager(&mut app, tile, coord);

    assert_eq!(
        band_trade_goods(&app, keeper),
        0.0,
        "no trade goods before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        band_trade_goods(&app, keeper) > 0.0,
        "a cotton Field must credit the keeper band's own trade_goods store"
    );
    assert_eq!(
        app.world
            .resource::<FactionInventory>()
            .stockpile(FactionId(0))
            .and_then(|items| items.get("trade_goods"))
            .copied()
            .unwrap_or(0),
        0,
        "ongoing harvest must NOT touch the start-profile faction stockpile"
    );
    let cohort = app.world.get::<PopulationCohort>(keeper).unwrap();
    assert!(
        cohort.stores.get(FOOD).to_f32() <= EPSILON,
        "a cash crop pays no food: {}",
        cohort.stores.get(FOOD).to_f32()
    );
    assert_eq!(
        cohort.stores.get(FODDER),
        scalar_zero(),
        "a cash crop pays no fodder"
    );
}

/// **A HAY FIELD'S WHOLE PRODUCT RIDES ITS OWN ROW** (issue #449) — the case the third account was
/// added for, and the mirror of the cash Field above.
///
/// `hay_grass` pays no provisions and no trade, so before `SourceYield::fodder` existed the row for
/// a fully productive Field read `actual 0 · trade 0` and every compact yield readout in the client
/// rendered `+0.00` — a live source indistinguishable from dead ground. The row now states its
/// fodder, and states **the number the band's `FODDER` store was actually credited**: asserted as
/// the store's own movement rather than against a re-derivation of `field_fodder`, per the §4.3
/// rule, so a readout that recomputed its own quote would fail here.
#[test]
fn a_hay_field_publishes_the_fodder_it_credits_and_nothing_in_the_other_two_accounts() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);
    let capacity = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile).expect("tile exists");
        tile_forage_capacity(&labor.forage, ground)
    };
    seat_field(&mut app, coord, "hay_grass", capacity);
    let keeper = spawn_forager(&mut app, tile, coord);

    app.world.run_system_once(advance_labor_allocation);

    let credited = app
        .world
        .get::<PopulationCohort>(keeper)
        .expect("the keeper band still exists")
        .stores
        .get(FODDER)
        .to_f32();
    assert!(
        credited > 0.0,
        "the fixture must actually harvest hay, or the row assertions below are vacuous"
    );

    let row = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("the band forages")
        .last_yields
        .first()
        .expect("the Field assignment has a yield row")
        .clone();
    assert!(
        (row.fodder - credited).abs() <= EPSILON,
        "the row must publish the credited fodder, not recompute it: {} vs {credited}",
        row.fodder
    );
    assert_eq!(
        (row.actual, row.trade),
        (0.0, 0.0),
        "hay is no food and no cash — this is the row that read +0.00"
    );
    assert!(
        band_trade_goods(&app, keeper) <= EPSILON,
        "and the fodder must not have leaked into the trade store: {}",
        band_trade_goods(&app, keeper)
    );
}

/// **The picker quote is the number the sim pays.** The labor arm's `field_trade_goods` and the
/// crop-picker's `commit_trade_payoff` are one seam: seed a Field at exactly the hypothetical patch
/// the quote builds (this tile's own `K` for the biome, at full standing crop — a Field neither
/// raises nor lowers it, #433) and the credited store equals the quote **exactly** — quote and payout
/// cannot drift (the §4.3 "assert the quote against the payoff function" rule, extended to the trade
/// account). It used to compare against `quoted.round()`, the integer stockpile's granularity; the
/// band store is fixed-point, so the comparison is now the honest one.
#[test]
fn the_picker_trade_payoff_matches_the_credited_store() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);

    // Build the quote for a biome cotton actually hosts, and reproduce the hypothetical patch's
    // standing crop so the sim's paid value and the quote read the identical biomass.
    let labor = labor();
    let flora = FloraConfig::builtin();
    let terrain = TerrainType::Floodplain;
    let quote_tile = UVec2::new(terrain as u32, 0);
    let quote_capacity = labor.forage.capacity_for(terrain);
    let composition = flora.composition(terrain);
    assert!(
        cotton_share(&flora, terrain) > 0.0,
        "cotton must host the quote biome"
    );
    let biomass = quote_capacity;

    seat_cotton_field(&mut app, coord, biomass);
    let keeper = spawn_forager(&mut app, tile, coord);
    app.world.run_system_once(advance_labor_allocation);

    let quoted = commit_trade_payoff(
        quote_tile,
        quote_capacity,
        "cotton",
        composition,
        &flora,
        &labor.forage,
        QUOTE_MULTIPLIER,
        RungKey::PlantField,
    );
    assert!(quoted > 0.0, "the fixture must quote a real cash payoff");
    let paid = band_trade_goods(&app, keeper);
    assert!(
        (paid - quoted).abs() <= EPSILON * quoted.max(1.0),
        "the credited trade goods must equal the picker's quote: paid {paid} vs quoted {quoted}"
    );
}
