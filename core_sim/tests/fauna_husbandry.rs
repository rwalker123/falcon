//! Phase E husbandry: a sustained Sustain-follow on a Thriving herd tames it into
//! domesticated livestock (emergent accrual + decay), which then yields steady provisions
//! and is immune to the overhunting collapse. Mirrors `fauna_follow.rs` setup.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_fauna_pursuits, advance_herds, advance_husbandry, scalar_one, scalar_zero,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FaunaPursuit,
    FaunaPursuitMode, FogRevealLedger, FollowPolicy, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, MapPresets, MapPresetsHandle, PopulationCohort,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry,
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

/// A stationary game herd (route length 1) primed to half its cap → Thriving and a clean
/// domestication candidate. Returns its id.
fn prime_thriving_herd(app: &mut App) -> String {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
            .map(|h| h.id.clone())
            .expect("expected short-range game to spawn")
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = (herd.carrying_capacity * 0.5).max(1.0);
    id
}

fn spawn_follower(app: &mut App, herd_id: &str, policy: FollowPolicy) -> bevy::prelude::Entity {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_zero(),
                elders: scalar_zero(),
                food_store: scalar_zero(),
                morale: scalar_one(),
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            FaunaPursuit {
                faction: FactionId(0),
                band_label: "Test Band".to_string(),
                fauna_id: herd_id.to_string(),
                mode: FaunaPursuitMode::Follow { policy },
                elapsed_turns: 0,
                started_tick: 0,
            },
        ))
        .id()
}

/// One full turn's fauna pipeline in real stage order: Logistics (herds regrow, husbandry
/// upkeep) then Population (pursuits resolve + accrue).
fn run_turns_with_follow(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_fauna_pursuits);
    }
}

/// Turns with no active band: only the Logistics-stage systems run.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
}

fn progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0)
}

/// Total provisions carried by faction 0's bands (food is band-local now, so the husbandry
/// yield lands in the owner's cohort larders, not the faction pool).
fn provisions(app: &mut App) -> i64 {
    let mut total = 0.0f32;
    let mut query = app.world.query::<&PopulationCohort>();
    for cohort in query.iter(&app.world) {
        if cohort.faction == FactionId(0) {
            total += cohort.food_store.to_f32();
        }
    }
    total.round() as i64
}

/// A sustained Sustain-follow on a Thriving herd tames it: progress climbs to 1.0
/// (domesticated) and the follower's faction owns it.
#[test]
fn sustain_follow_domesticates_thriving_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_follower(&mut app, &id, FollowPolicy::Sustain);

    // net accrual = progress_per_turn(0.04) - decay(0.01) = 0.03/turn → ~34 turns to 1.0.
    run_turns_with_follow(&mut app, 45);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("domesticated herd persists");
    assert!(
        herd.is_domesticated(),
        "sustained Sustain-follow should domesticate: progress {}",
        herd.domestication_progress
    );
    assert_eq!(herd.owner, Some(FactionId(0)), "the follower owns the herd");
    assert_eq!(registry.domesticated_count(FactionId(0)), 1);
}

/// Only a Sustain follow tames; an Eradicate follow never accrues husbandry.
#[test]
fn eradicate_follow_does_not_domesticate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_follower(&mut app, &id, FollowPolicy::Eradicate);
    run_turns_with_follow(&mut app, 10);
    assert_eq!(
        progress_of(&app, &id),
        0.0,
        "eradicate accrues no husbandry"
    );
}

/// Husbandry progress decays and ownership lapses once the herd isn't being tended.
#[test]
fn progress_decays_without_sustained_follow() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let band = spawn_follower(&mut app, &id, FollowPolicy::Sustain);
    run_turns_with_follow(&mut app, 6);
    let built = progress_of(&app, &id);
    assert!(built > 0.0, "some progress should have accrued");

    // Stop following, then let husbandry decay run.
    app.world.despawn(band);
    run_turns_untended(&mut app, 6);
    let decayed = progress_of(&app, &id);
    assert!(
        decayed < built,
        "progress should decay: {built} -> {decayed}"
    );

    // Decay all the way down clears ownership.
    run_turns_untended(&mut app, 60);
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("herd still exists");
    assert_eq!(herd.domestication_progress, 0.0);
    assert_eq!(herd.owner, None, "ownership lapses at zero progress");
}

/// A domesticated (managed) herd is immune to the overhunting collapse: driven below the
/// Allee threshold it recovers logistically instead of crashing to extinction.
#[test]
fn domesticated_herd_is_collapse_immune() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let low = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0)); // sets owner + progress = 1.0 → domesticated
                                                // Below the 15% collapse threshold — a wild herd here would crash.
        let low = herd.carrying_capacity * 0.10;
        herd.biomass = low;
        low
    };

    run_turns_untended(&mut app, 10);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&id)
        .expect("a domesticated herd never collapses to extinction");
    assert!(
        herd.biomass > low,
        "managed herd should recover, not crash: {low} -> {}",
        herd.biomass
    );
}

/// A domesticated herd yields steady provisions to its owner each turn without depleting.
#[test]
fn domesticated_herd_yields_provisions() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let biomass_before = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        herd.biomass
    };
    assert_eq!(provisions(&mut app), 0);

    app.world.run_system_once(advance_husbandry);

    assert!(
        provisions(&mut app) > 0,
        "a domesticated herd should pay its owner provisions"
    );
    // The yield is a sustainable harvest — it does not reduce the herd.
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert_eq!(
        after, biomass_before,
        "husbandry yield must not deplete biomass"
    );
}
