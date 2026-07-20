//! The Telling (PR-A): beats fire once, on real sim state, dressed in live nouns, and land in
//! the command feed with a gloss showing the numbers behind the line. World setup mirrors
//! `sedentarization.rs`.

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

fn spawn_world() -> App {
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
fn spawn_band(app: &mut App, faction: FactionId, size: u32) {
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

fn set_surplus(app: &mut App, faction: FactionId, amount: u32) {
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        if cohort.faction == faction {
            cohort.stores.set(FOOD, Scalar::from_u32(amount));
        }
    }
}

fn domesticate(app: &mut App, faction: FactionId, count: usize) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for herd in registry.herds.iter_mut().take(count) {
        herd.claim_domestication(faction);
    }
}

/// Advance one full turn: sedentarization then The Telling, then bump the tick (the real
/// pipeline's `advance_tick` lives in the Snapshot stage, after Telling).
fn run_turn(app: &mut App) {
    app.world.run_system_once(sedentarization_tick);
    app.world.run_system_once(telling_tick);
    app.world.resource_mut::<SimulationTick>().0 += 1;
}

/// Total people across the faction's resident bands — what `band.count` samples. Worldgen
/// already seeds the start profile's bands, so this is never just the one the test spawns.
fn resident_people(app: &mut App, faction: FactionId) -> u64 {
    let mut query = app
        .world
        .query_filtered::<&PopulationCohort, bevy::prelude::With<ResidentBand>>();
    query
        .iter(&app.world)
        .filter(|c| c.faction == faction)
        .map(|c| c.size as u64)
        .sum()
}

fn beats(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == CommandEventKind::NarrativeBeat)
        .cloned()
        .collect()
}

/// Turn 0 fires the opening beat, and the band count is interpolated into the copy.
#[test]
fn turn_zero_fires_the_cold_open_with_the_band_count_interpolated() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    let people = resident_people(&mut app, FactionId(0));
    run_turn(&mut app);

    let fired = beats(&app);
    assert_eq!(fired.len(), 1, "exactly the opening beat fires on turn 0");
    let entry = &fired[0];
    assert!(
        entry.label.contains(&people.to_string()),
        "the band count ({people}) must be interpolated into the line: {:?}",
        entry.label
    );
    assert!(
        !entry.label.contains('{'),
        "no unrendered placeholder may reach the player: {:?}",
        entry.label
    );
    let detail = entry.detail.as_deref().expect("gloss present");
    assert!(detail.contains(&format!("band.count={people}")), "{detail}");
    assert!(detail.contains("turn.index=0"), "{detail}");
    assert!(detail.contains("tier=beat"), "{detail}");

    // `once: true` — it never fires again.
    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("opening.cold_open"));
    for _ in 0..10 {
        run_turn(&mut app);
    }
    assert_eq!(
        beats(&app)
            .iter()
            .filter(|e| e
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("turn.index=")))
            .count(),
        1,
        "the cold open is a `once` beat"
    );
}

/// Driving sedentarization across the soft threshold fires `sedentarization.soft_drift` exactly
/// once, and it lands in the feed with a gloss showing the real score.
#[test]
fn crossing_sedentarization_forty_fires_the_soft_drift_beat_exactly_once() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 300);

    // A few quiet turns first, so the score has a stored sub-threshold `prev` to cross from.
    for _ in 0..3 {
        run_turn(&mut app);
    }
    assert!(
        app.world
            .resource::<SedentarizationScore>()
            .score(FactionId(0))
            < 40.0,
        "the score must start below the threshold for a rising crossing to exist"
    );

    // Build the pastoral base and let the EMA climb past 40.
    domesticate(&mut app, FactionId(0), 3);
    for _ in 0..20 {
        set_surplus(&mut app, FactionId(0), 300);
        run_turn(&mut app);
    }

    let score = app
        .world
        .resource::<SedentarizationScore>()
        .score(FactionId(0));
    assert!(
        score >= 40.0,
        "the score should have crossed 40, got {score}"
    );

    let drift: Vec<_> = beats(&app)
        .into_iter()
        .filter(|e| {
            e.detail
                .as_deref()
                .is_some_and(|d| d.contains("sedentarization.score="))
        })
        .collect();
    assert_eq!(
        drift.len(),
        1,
        "the soft-drift beat must fire exactly once per crossing, got {drift:#?}"
    );
    let detail = drift[0].detail.as_deref().unwrap();
    // "The voice never lies": the gloss carries the real sampled score.
    let glossed: f64 = detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix("sedentarization.score="))
        .and_then(|v| v.parse().ok())
        .expect("gloss carries a numeric score");
    assert!(
        glossed >= 40.0,
        "the glossed score must be the real crossing value, got {glossed}"
    );
    assert_eq!(
        app.world
            .resource::<BeatLedger>()
            .fired_ticks("sedentarization.soft_drift")
            .len(),
        1
    );
}

/// Same seed, same world, same run → identical beat *and* wardrobe selection.
#[test]
fn selection_is_reproducible_across_two_runs_of_the_same_seed() {
    fn transcript() -> Vec<(String, Option<String>)> {
        let mut app = spawn_world();
        spawn_band(&mut app, FactionId(0), 42);
        domesticate(&mut app, FactionId(0), 3);
        for _ in 0..25 {
            set_surplus(&mut app, FactionId(0), 300);
            run_turn(&mut app);
        }
        beats(&app)
            .into_iter()
            .map(|e| (e.label, e.detail))
            .collect()
    }

    let first = transcript();
    let second = transcript();
    assert!(!first.is_empty(), "the run must produce some narration");
    assert_eq!(
        first, second,
        "a fixed seed must reproduce the same beats and the same dressings"
    );
}

/// A beat whose every wardrobe entry is excluded (its required noun is unresolvable) must emit
/// nothing **and must not be marked fired** — it can still land once the world can dress it.
#[test]
fn a_beat_with_no_dressable_wardrobe_does_not_fire() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 30);
    // No sites are ever discovered here, so `discovery.site_found` can never resolve `place`.
    for _ in 0..15 {
        run_turn(&mut app);
    }
    assert!(
        !app.world
            .resource::<BeatLedger>()
            .has_fired("discovery.site_found"),
        "an undressable beat must not be marked fired"
    );
}
