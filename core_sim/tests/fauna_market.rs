//! Market hunting: the commercial `FollowPolicy::Market` over-harvests a herd for boosted
//! trade goods, declining it much faster than Surplus into the Phase D collapse. Mirrors
//! `fauna_follow.rs` setup.

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

/// Two distinct stationary game herds (route length 1) primed to a large half-capacity
/// size (Thriving) for side-by-side policy comparison. The size is inflated so the
/// per-turn take is big enough that integer trade/provisions yields don't quantize to
/// zero (small warrens otherwise round down every turn).
fn prime_two_stationary_herds(app: &mut App) -> (String, String) {
    const CAP: f32 = 4000.0;
    let ids: Vec<String> = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .filter(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .map(|h| h.id.clone())
            .take(2)
            .collect()
    };
    assert!(ids.len() == 2, "need two stationary game herds");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for id in &ids {
        let herd = registry.herds.iter_mut().find(|h| &h.id == id).unwrap();
        herd.carrying_capacity = CAP;
        herd.biomass = CAP * 0.5;
    }
    (ids[0].clone(), ids[1].clone())
}

fn spawn_follower(
    app: &mut App,
    herd_id: &str,
    policy: FollowPolicy,
    faction: FactionId,
) -> bevy::prelude::Entity {
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
                faction,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            FaunaPursuit {
                faction,
                band_label: "Test Band".to_string(),
                fauna_id: herd_id.to_string(),
                mode: FaunaPursuitMode::Follow { policy },
                elapsed_turns: 0,
                started_tick: 0,
            },
        ))
        .id()
}

fn run_turns(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_fauna_pursuits);
    }
}

fn biomass_ratio(app: &App, id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass / h.carrying_capacity)
}

fn trade_goods(app: &App, faction: FactionId) -> i64 {
    app.world
        .resource::<FactionInventory>()
        .stockpile(faction)
        .and_then(|m| m.get("trade_goods"))
        .copied()
        .unwrap_or(0)
}

#[test]
fn market_policy_string_round_trips() {
    assert_eq!("market".parse::<FollowPolicy>(), Ok(FollowPolicy::Market));
    assert_eq!(FollowPolicy::Market.as_str(), "market");
}

/// Market declines a herd far faster than Surplus and earns far more trade goods.
#[test]
fn market_declines_faster_and_earns_more_trade_than_surplus() {
    let mut app = spawn_world();
    let (market_herd, surplus_herd) = prime_two_stationary_herds(&mut app);
    spawn_follower(&mut app, &market_herd, FollowPolicy::Market, FactionId(0));
    spawn_follower(&mut app, &surplus_herd, FollowPolicy::Surplus, FactionId(1));

    run_turns(&mut app, 6);

    let market_ratio = biomass_ratio(&app, &market_herd).expect("market herd still exists");
    let surplus_ratio = biomass_ratio(&app, &surplus_herd).expect("surplus herd still exists");
    assert!(
        market_ratio < surplus_ratio,
        "market should deplete faster than surplus: market {market_ratio} vs surplus {surplus_ratio}"
    );
    // Commercial harvest: bigger take + boosted trade rate → far more trade goods.
    let market_trade = trade_goods(&app, FactionId(0));
    let surplus_trade = trade_goods(&app, FactionId(1));
    assert!(
        market_trade > surplus_trade,
        "market should out-earn surplus on trade: market {market_trade} vs surplus {surplus_trade}"
    );
}

/// Sustained market hunting drives the group to local extinction (Phase D collapse reuse).
#[test]
fn market_follow_drives_collapse() {
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    let band = spawn_follower(&mut app, &herd, FollowPolicy::Market, FactionId(0));
    run_turns(&mut app, 40);

    assert!(
        app.world.resource::<HerdRegistry>().find(&herd).is_none(),
        "market hunting should drive the group extinct"
    );
    // Once the herd is gone the pursuit cancels itself.
    assert!(
        app.world.get::<FaunaPursuit>(band).is_none(),
        "pursuit should clear after the herd despawns"
    );
}

/// Market hunting never tames a herd — only Sustain accrues husbandry.
#[test]
fn market_follow_does_not_domesticate() {
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    spawn_follower(&mut app, &herd, FollowPolicy::Market, FactionId(0));
    run_turns(&mut app, 4);
    let progress = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0);
    assert_eq!(
        progress, 0.0,
        "market hunting must not accrue domestication"
    );
}
