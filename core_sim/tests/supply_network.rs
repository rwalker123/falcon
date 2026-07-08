//! Supply network integration: same-faction bands within `reach_tiles` auto-balance their food,
//! so a fed band feeds an empty neighbor — but a band beyond reach is on its own. World setup
//! mirrors `sedentarization.rs`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    balance_supply_networks, scalar_zero, spawn_initial_world, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, GenerationId, GenerationRegistry,
    LocalStore, MapPresets, MapPresetsHandle, PopulationCohort, Scalar, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle,
    TileRegistry, FOOD,
};

/// A distinct faction for the test bands so they never network with the spawned starting bands.
const TEST_FACTION: FactionId = FactionId(7);
const BAND_POP: u32 = 100;

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
    app.world
        .insert_resource(SupplyNetworkConfigHandle::default());

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    app
}

/// Spawn a test band of `BAND_POP` working-age people on the tile at `(x, y)` carrying `food`.
fn spawn_band(app: &mut App, x: u32, y: u32, food: i64) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("tile coords resolve");
    let mut stores = LocalStore::new();
    stores.set(FOOD, Scalar::from_i64(food));
    app.world
        .spawn(PopulationCohort {
            home: tile,
            current_tile: tile,
            size: BAND_POP,
            children: scalar_zero(),
            working: Scalar::from_u32(BAND_POP),
            elders: scalar_zero(),
            stores,
            morale: scalar_zero(),
            age_turns: 0,
            generation: 0 as GenerationId,
            faction: TEST_FACTION,
            knowledge: Vec::new(),
            migration: None,
        })
        .id()
}

fn food_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

/// Two same-faction bands two tiles apart (within the default reach of 3) equalize their food.
#[test]
fn nearby_bands_share_food() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 2, cy, 0);

    app.world.run_system_once(balance_supply_networks);

    assert!(
        food_of(&app, empty) > 0.0,
        "an empty band near a fed one should receive food, got {}",
        food_of(&app, empty)
    );
    assert!(
        food_of(&app, fed) < 1_000.0,
        "the fed band should have shipped some of its surplus"
    );
}

/// A band ten tiles away (beyond reach) shares nothing — it's on its own larder.
#[test]
fn distant_bands_do_not_share() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 10, cy, 0);

    app.world.run_system_once(balance_supply_networks);

    assert_eq!(
        food_of(&app, empty),
        0.0,
        "a band beyond reach should receive nothing"
    );
    assert_eq!(
        food_of(&app, fed),
        1_000.0,
        "the fed band keeps all its food when no one is in reach"
    );
}
