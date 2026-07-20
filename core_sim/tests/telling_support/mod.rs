//! Shared world harness for The Telling's integration tests (`telling.rs`, `telling_fork.rs`).
//!
//! One real earthlike world on a pinned seed, driven a turn at a time through the same systems the
//! turn pipeline runs, so both suites exercise the engine rather than a mock.

#![allow(dead_code)]

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    scalar_one, scalar_zero, sedentarization_tick, spawn_initial_herds, spawn_initial_world,
    telling_tick, BeatCatalogHandle, BeatConfigHandle, BeatLedger, CommandEventEntry,
    CommandEventKind, CommandEventLog, CultureManager, DiscoveredSites, DiscoveryProgressLedger,
    FactionId, FactionInventory, FactionRegistry, FaunaConfigHandle, FogRevealLedger,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Scalar,
    SedentarizationConfigHandle, SedentarizationScore, SimulationConfig, SimulationTick,
    SitesConfigHandle, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, FOOD,
};

/// Pinned so selection (seeded from `map_seed`) is reproducible run to run.
const MAP_SEED: u64 = 119_304_647;

pub fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
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
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.insert_resource(SedentarizationScore::default());
    app.world
        .insert_resource(SedentarizationConfigHandle::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.run_system_once(spawn_initial_herds);

    // The Telling's own resources.
    app.world.insert_resource(FactionRegistry::default());
    app.world.insert_resource(SitesConfigHandle::default());
    app.world.insert_resource(DiscoveredSites::default());
    app.world.insert_resource(BeatConfigHandle::default());
    app.world.insert_resource(BeatCatalogHandle::default());
    app.world.insert_resource(BeatLedger::default());
    app
}

/// Spawn a resident band of `size` people standing on a real map tile (so
/// `biome.current_dominant` resolves).
pub fn spawn_band(app: &mut App, faction: FactionId, size: u32) {
    let tile = app
        .world
        .query::<(bevy::prelude::Entity, &core_sim::Tile)>()
        .iter(&app.world)
        .next()
        .map(|(entity, _)| entity)
        .expect("worldgen produced tiles");
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size,
            children: scalar_zero(),
            working: scalar_zero(),
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
            faction,
            knowledge: Vec::new(),
            migration: None,
        },
        ResidentBand,
    ));
}

pub fn set_surplus(app: &mut App, faction: FactionId, amount: u32) {
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        if cohort.faction == faction {
            cohort.stores.set(FOOD, Scalar::from_u32(amount));
        }
    }
}

pub fn domesticate(app: &mut App, faction: FactionId, count: usize) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for herd in registry.herds.iter_mut().take(count) {
        herd.claim_domestication(faction);
    }
}

/// Advance one full turn: sedentarization then The Telling, then bump the tick (the real
/// pipeline's `advance_tick` lives in the Snapshot stage, after Telling).
pub fn run_turn(app: &mut App) {
    app.world.run_system_once(sedentarization_tick);
    app.world.run_system_once(telling_tick);
    app.world.resource_mut::<SimulationTick>().0 += 1;
}

/// Total people across the faction's resident bands — what `band.count` samples. Worldgen
/// already seeds the start profile's bands, so this is never just the one the test spawns.
pub fn resident_people(app: &mut App, faction: FactionId) -> u64 {
    let mut query = app
        .world
        .query_filtered::<&PopulationCohort, bevy::prelude::With<ResidentBand>>();
    query
        .iter(&app.world)
        .filter(|c| c.faction == faction)
        .map(|c| c.size as u64)
        .sum()
}

pub fn beats(app: &App) -> Vec<CommandEventEntry> {
    events_of(app, CommandEventKind::NarrativeBeat)
}

/// The fork-resolution feed lines (a player answer, or the expiry valve's auto-defer).
pub fn fork_events(app: &App) -> Vec<CommandEventEntry> {
    events_of(app, CommandEventKind::NarrativeFork)
}

fn events_of(app: &App, kind: CommandEventKind) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == kind)
        .cloned()
        .collect()
}

/// Release every herd the faction tamed, so the sedentarization score can fall again.
pub fn undomesticate_all(app: &mut App) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for herd in registry.herds.iter_mut() {
        herd.domestication_progress = 0.0;
        herd.owner = None;
    }
}

/// Drive sedentarization up past the soft threshold (40), the trigger the shipped fork rides.
pub fn drive_sedentarization_past_the_soft_threshold(app: &mut App, faction: FactionId) {
    // A few quiet turns first, so the score has a stored sub-threshold `prev` to cross from.
    for _ in 0..3 {
        run_turn(app);
    }
    assert!(
        app.world.resource::<SedentarizationScore>().score(faction) < 40.0,
        "the score must start below the threshold for a rising crossing to exist"
    );
    domesticate(app, faction, 3);
    for _ in 0..20 {
        set_surplus(app, faction, 300);
        run_turn(app);
    }
    let score = app.world.resource::<SedentarizationScore>().score(faction);
    assert!(
        score >= 40.0,
        "the score should have crossed 40, got {score}"
    );
}
