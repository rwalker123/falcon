//! SedentarizationScore: the per-faction "pressure to settle" rises with domestication +
//! surplus + population, EMA-smoothed, and fires soft/hard prompts once on rising crossings.
//! World setup mirrors `fauna_husbandry.rs`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    scalar_one, scalar_zero, sedentarization_tick, spawn_initial_herds, spawn_initial_world,
    CommandEventKind, CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId,
    FactionInventory, FaunaConfigHandle, FogRevealLedger, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, Scalar, SedentarizationConfigHandle, SedentarizationScore,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, FOOD,
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
    app.world.insert_resource(SedentarizationScore::default());
    app.world
        .insert_resource(SedentarizationConfigHandle::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Spawn a bare population cohort for `faction` with the given size (the tick only reads
/// `faction` + `size`, so the tile entity can be a placeholder).
fn spawn_cohort(app: &mut App, faction: FactionId, size: u32) {
    let tile = app.world.spawn_empty().id();
    app.world.spawn(PopulationCohort {
        home: tile,
        current_tile: tile,
        size,
        children: scalar_zero(),
        working: scalar_zero(),
        elders: scalar_zero(),
        stores: LocalStore::new(),
        morale: scalar_one(),
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        age_turns: 0,
        generation: 0 as GenerationId,
        faction,
        knowledge: Vec::new(),
        migration: None,
    });
}

/// Set the faction's carried food surplus (the sedentarization surplus input now reads the
/// bands' local larders, not the faction pool).
fn set_surplus(app: &mut App, faction: FactionId, amount: u32) {
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        if cohort.faction == faction {
            cohort.stores.set(FOOD, Scalar::from_u32(amount));
        }
    }
}

/// Claim `count` herds as domesticated for `faction` (drives the domestication input).
fn domesticate(app: &mut App, faction: FactionId, count: usize) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for herd in registry.herds.iter_mut().take(count) {
        herd.claim_domestication(faction);
    }
}

fn run_tick(app: &mut App) {
    app.world.run_system_once(sedentarization_tick);
}

fn score(app: &App, faction: FactionId) -> f32 {
    app.world.resource::<SedentarizationScore>().score(faction)
}

fn prompt_count(app: &App) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == CommandEventKind::SedentarizationPrompt)
        .count()
}

/// A pastoral base (domesticated herds + surplus + population) drives the score far above a
/// bare nomadic tribe's.
#[test]
fn score_rises_with_domestication_and_surplus() {
    let mut app = spawn_world();
    spawn_cohort(&mut app, FactionId(0), 300);
    run_tick(&mut app);
    let baseline = score(&app, FactionId(0));

    // Build the pastoral base: 3 domesticated herds + full provisions surplus.
    domesticate(&mut app, FactionId(0), 3);
    set_surplus(&mut app, FactionId(0), 300);
    for _ in 0..6 {
        run_tick(&mut app);
    }
    let after = score(&app, FactionId(0));
    assert!(
        after > baseline + 30.0,
        "domestication + surplus should raise the score: {baseline} -> {after}"
    );
}

/// Soft then hard prompts fire exactly once each on rising crossings, and not again while the
/// score stays above.
#[test]
fn prompts_fire_once_on_rising_crossings() {
    let mut app = spawn_world();
    // Max out the per-faction inputs so the score climbs across both thresholds.
    spawn_cohort(&mut app, FactionId(0), 300);
    domesticate(&mut app, FactionId(0), 3);
    set_surplus(&mut app, FactionId(0), 300);

    for _ in 0..8 {
        run_tick(&mut app);
    }

    // Exactly one soft + one hard crossing prompt (edge-gated).
    assert_eq!(
        prompt_count(&app),
        2,
        "expected one soft and one hard prompt, got {}",
        prompt_count(&app)
    );
    let stages: Vec<String> = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == CommandEventKind::SedentarizationPrompt)
        .filter_map(|e| e.detail.clone())
        .collect();
    assert!(stages.iter().any(|d| d.contains("stage=soft")));
    assert!(stages.iter().any(|d| d.contains("stage=hard")));
}

/// The score stays within [0, 100] even with all inputs maxed for many turns.
#[test]
fn score_is_bounded() {
    let mut app = spawn_world();
    spawn_cohort(&mut app, FactionId(0), 100_000);
    domesticate(&mut app, FactionId(0), 6);
    set_surplus(&mut app, FactionId(0), 1_000_000);
    for _ in 0..20 {
        run_tick(&mut app);
    }
    let s = score(&app, FactionId(0));
    assert!((0.0..=100.0).contains(&s), "score out of range: {s}");
}

/// EMA smoothing: the score climbs gradually toward the target rather than snapping.
#[test]
fn score_is_ema_smoothed() {
    let mut app = spawn_world();
    spawn_cohort(&mut app, FactionId(0), 300);
    domesticate(&mut app, FactionId(0), 3);
    set_surplus(&mut app, FactionId(0), 300);

    run_tick(&mut app);
    let after_one = score(&app, FactionId(0));
    for _ in 0..5 {
        run_tick(&mut app);
    }
    let after_six = score(&app, FactionId(0));
    assert!(
        after_six > after_one,
        "score should keep climbing under smoothing: {after_one} -> {after_six}"
    );
    // First step is only a fraction of the way to the target (smoothing < 1).
    assert!(after_one < after_six, "smoothing should not snap instantly");
}
