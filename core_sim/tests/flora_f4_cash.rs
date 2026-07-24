//! **Cash crops — the F4 coupling** (Flora Roster F4, `docs/plan_flora_roster.md` §6).
//!
//! The yield vector's third and final account. A cash crop (`cotton`/`flax`) is a `field`-ceiling
//! species whose vector is **trade-dominant** and whose `provisions_per_biomass` is `0`: harvesting
//! it as a Field credits the faction `trade_goods` stockpile and (near) zero food. F4 is the exact
//! twin of F3's fodder work — the *same* managed harvest, routed by the vector's `trade_goods`
//! component with **no `role` branch**. This file pins that routing against the **loaded** configs,
//! never a literal, so a retune of a table fails the test instead of agreeing with a stale copy.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, commit_fodder_payoff, commit_payoff, commit_trade_payoff,
    concentration_for_share, concentration_gain, generate_hydrology, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_world, tile_forage_capacity, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle,
    FloraConfig, FogRevealLedger, FollowPolicy, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, RungKey, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    BUILTIN_LABOR_CONFIG, FODDER, FOOD,
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
    // all compete for — cotton is hosted here, not on AlluvialPlain (staples stay dominant there).
    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);

    let payoffs = |species: &str| {
        let share = flora
            .composition(terrain)
            .iter()
            .find(|entry| entry.species == species)
            .unwrap_or_else(|| panic!("{species} must host {terrain:?}"))
            .share;
        let food = commit_payoff(
            tile,
            capacity,
            species,
            share,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        let fodder = commit_fodder_payoff(
            tile,
            capacity,
            species,
            share,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
        );
        let trade = commit_trade_payoff(
            tile,
            capacity,
            species,
            share,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
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

/// **The token is exactly `biomass × field_dial × token_rate / wild_rate`.** Pin the grain Field's
/// trade token against the loaded dials by name, so a change to `field_provisions_per_biomass`, the
/// grain's `trade_goods_per_biomass`, or the wild `provisions_per_biomass` moves this number — this
/// is the "assert the quote against the payoff function's inputs" discipline.
#[test]
fn the_grain_trade_token_carries_the_field_dial_and_the_wild_baseline() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);
    let share = flora
        .composition(terrain)
        .iter()
        .find(|entry| entry.species == "wild_emmer")
        .expect("emmer hosts the plain")
        .share;

    // The hypothetical Field the quote builds: this tile's K, concentrated by the field gain, at the
    // full standing crop.
    let biomass =
        capacity * concentration_for_share(share, concentration_gain(forage, RungKey::PlantField));
    let expected = biomass
        * forage.cultivation.field_provisions_per_biomass
        * (flora.species["wild_emmer"].yield_.trade_goods_per_biomass
            / forage.provisions_per_biomass)
        * QUOTE_MULTIPLIER;

    let quoted = commit_trade_payoff(
        tile,
        capacity,
        "wild_emmer",
        share,
        &flora,
        forage,
        QUOTE_MULTIPLIER,
    );
    assert!(
        (quoted - expected).abs() <= EPSILON * expected.max(1.0),
        "the grain trade token must be biomass x field_dial x token_rate/wild_rate: \
         {quoted} vs {expected}"
    );
}

// ---------------------------------------------------------------------------------------------
// Integration: the labor arm credits the FACTION trade_goods stockpile.
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
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
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

/// Turn the patch at `coord` into a completed cotton **Field** standing at `biomass`.
fn seat_cotton_field(app: &mut App, coord: UVec2, biomass: f32) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = Some("cotton".to_string());
    patch.field_progress = 1.0;
    patch.carrying_capacity = biomass;
    patch.biomass = biomass;
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
                        policy: FollowPolicy::Sustain,
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

/// **A cash Field credits the faction `trade_goods` stockpile and (near) zero food** — and no fodder
/// at all. The trade goods are a FACTION-level commodity, so unlike FOOD/FODDER they land in
/// `FactionInventory`, exactly as the Market-forage arm's wild sale does.
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
        faction_trade_goods(&app),
        0,
        "no trade goods before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        faction_trade_goods(&app) > 0,
        "a cotton Field must credit the faction trade_goods stockpile"
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

/// **The picker quote is the number the sim pays.** The labor arm's `field_trade_goods` and the
/// crop-picker's `commit_trade_payoff` are one seam: seed a Field at exactly the hypothetical patch
/// the quote builds (this tile's K for the biome, concentrated by the field gain, at full standing
/// crop) and the credited stockpile equals the quote, rounded — quote and payout cannot drift
/// (the §4.3 "assert the quote against the payoff function" rule, extended to the trade account).
#[test]
fn the_picker_trade_payoff_matches_the_credited_stockpile() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);

    // Build the quote for a biome cotton actually hosts, and reproduce the hypothetical patch's
    // standing crop so the sim's paid value and the quote read the identical biomass.
    let labor = labor();
    let flora = FloraConfig::builtin();
    let terrain = TerrainType::Floodplain;
    let quote_tile = UVec2::new(terrain as u32, 0);
    let quote_capacity = labor.forage.capacity_for(terrain);
    let share = cotton_share(&flora, terrain);
    let biomass = quote_capacity
        * concentration_for_share(
            share,
            concentration_gain(&labor.forage, RungKey::PlantField),
        );

    seat_cotton_field(&mut app, coord, biomass);
    spawn_forager(&mut app, tile, coord);
    app.world.run_system_once(advance_labor_allocation);

    let quoted = commit_trade_payoff(
        quote_tile,
        quote_capacity,
        "cotton",
        share,
        &flora,
        &labor.forage,
        QUOTE_MULTIPLIER,
    );
    assert!(quoted > 0.0, "the fixture must quote a real cash payoff");
    assert_eq!(
        faction_trade_goods(&app),
        quoted.round() as i64,
        "the credited trade goods must equal the picker's quote, rounded: paid {} vs quoted {quoted}",
        faction_trade_goods(&app)
    );
}
