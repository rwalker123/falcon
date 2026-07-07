//! Phase D ecology: critical-depensation collapse (point of no return), phase
//! classification, and per-turn immigration. Mirrors the world setup of
//! `fauna_hunt.rs`/`fauna_spawn.rs`.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, repopulate_fauna, spawn_initial_herds, spawn_initial_world, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, EcologyPhase, FactionInventory, FaunaConfig,
    FaunaConfigHandle, FogRevealLedger, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, MapPresets, MapPresetsHandle, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle,
};

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
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Set the first herd's biomass to `fraction * carrying_capacity` and return its id.
fn prime_first_herd(app: &mut App, fraction: f32) -> (String, f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .first_mut()
        .expect("expected a herd to spawn");
    let biomass = (herd.carrying_capacity * fraction).max(0.1);
    herd.biomass = biomass;
    (herd.id.clone(), biomass)
}

fn biomass_of(app: &App, id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
}

fn phase_of(app: &App, id: &str) -> Option<EcologyPhase> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.ecology_phase)
}

/// A group driven below the collapse threshold crashes to local extinction with **no
/// further hunting** — the overhunting point of no return.
#[test]
fn sub_threshold_group_collapses_without_hunting() {
    let mut app = spawn_world();
    // 10% of cap is below the 15% Allee threshold → non-viable.
    let (id, start) = prime_first_herd(&mut app, 0.10);

    // First few turns: biomass strictly declines (negative net regrowth) and the herd
    // is classified Collapsing once `advance_herds` refreshes its phase.
    let mut prev = start;
    for turn in 0..3 {
        app.world.run_system_once(advance_herds);
        if turn == 0 {
            assert_eq!(phase_of(&app, &id), Some(EcologyPhase::Collapsing));
        }
        let now = biomass_of(&app, &id).expect("herd still collapsing, not yet extinct");
        assert!(
            now < prev,
            "collapsing biomass should fall: {prev} -> {now}"
        );
        prev = now;
    }

    // With no hunting at all, it reaches local extinction within a bounded horizon.
    for _ in 0..20 {
        app.world.run_system_once(advance_herds);
        if biomass_of(&app, &id).is_none() {
            return;
        }
    }
    panic!("a sub-threshold group should despawn (local extinction) even without hunting");
}

/// A depleted-but-above-threshold group recovers if left alone — proving the collapse
/// line is a genuine tipping point, not "anything below cap dies".
#[test]
fn stressed_group_above_threshold_recovers() {
    let mut app = spawn_world();
    // 25% of cap: below the 40% stressed band, above the 15% collapse threshold.
    let (id, start) = prime_first_herd(&mut app, 0.25);

    app.world.run_system_once(advance_herds);
    assert_eq!(phase_of(&app, &id), Some(EcologyPhase::Stressed));
    for _ in 0..7 {
        app.world.run_system_once(advance_herds);
    }
    let after = biomass_of(&app, &id).expect("stressed group should survive and recover");
    assert!(
        after > start,
        "a group above the collapse threshold should regrow: {start} -> {after}"
    );
}

/// Biomass fraction of carrying capacity maps to the expected `EcologyPhase`.
#[test]
fn ecology_phase_classifies_by_biomass() {
    let mut app = spawn_world();
    // Deep in each band so a single regrowth/collapse step can't cross a boundary.
    let (collapsing, _) = {
        let id = app.world.resource::<HerdRegistry>().herds[0].id.clone();
        let cap = app.world.resource::<HerdRegistry>().herds[0].carrying_capacity;
        let mut reg = app.world.resource_mut::<HerdRegistry>();
        reg.herds[0].biomass = cap * 0.05;
        (id, cap)
    };
    let stressed = {
        let cap = app.world.resource::<HerdRegistry>().herds[1].carrying_capacity;
        let mut reg = app.world.resource_mut::<HerdRegistry>();
        reg.herds[1].biomass = cap * 0.25;
        reg.herds[1].id.clone()
    };
    let thriving = {
        let cap = app.world.resource::<HerdRegistry>().herds[2].carrying_capacity;
        let mut reg = app.world.resource_mut::<HerdRegistry>();
        reg.herds[2].biomass = cap * 0.80;
        reg.herds[2].id.clone()
    };

    app.world.run_system_once(advance_herds);

    assert_eq!(phase_of(&app, &collapsing), Some(EcologyPhase::Collapsing));
    assert_eq!(phase_of(&app, &stressed), Some(EcologyPhase::Stressed));
    assert_eq!(phase_of(&app, &thriving), Some(EcologyPhase::Thriving));
}

/// Build a fauna config from the builtin with immigration and abundance overrides.
fn config_with(chance: f32, max_total_game: usize) -> FaunaConfigHandle {
    let mut cfg = (*FaunaConfig::builtin()).clone();
    cfg.immigration.chance_per_turn = chance;
    cfg.immigration.max_attempts = 200;
    cfg.abundance.max_total_game = max_total_game;
    FaunaConfigHandle::new(Arc::new(cfg))
}

/// Count short-range game groups (the ones `max_total_game` caps; migratory `herd_*`
/// are excluded).
fn game_count(app: &App) -> usize {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|h| h.id.starts_with("game_"))
        .count()
}

/// Below the abundance cap and with a guaranteed roll, immigration respawns one group.
#[test]
fn immigration_respawns_below_cap() {
    let mut app = spawn_world();
    let before = app.world.resource::<HerdRegistry>().herds.len();
    // Force the roll and keep the game cap above the current game count.
    app.world
        .insert_resource(config_with(1.0, game_count(&app) + 5));

    app.world.run_system_once(repopulate_fauna);

    let after = app.world.resource::<HerdRegistry>().herds.len();
    assert_eq!(
        after,
        before + 1,
        "one group should immigrate below the cap"
    );
}

/// At (or above) the abundance cap, immigration adds nothing. The cap counts only
/// short-range game groups, not migratory herds.
#[test]
fn immigration_respects_cap() {
    let mut app = spawn_world();
    let before = app.world.resource::<HerdRegistry>().herds.len();
    // Cap at the current game count → the early return fires despite the guaranteed roll.
    app.world
        .insert_resource(config_with(1.0, game_count(&app)));

    app.world.run_system_once(repopulate_fauna);

    let after = app.world.resource::<HerdRegistry>().herds.len();
    assert_eq!(after, before, "no group should immigrate at the game cap");
}
