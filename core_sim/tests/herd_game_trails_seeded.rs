//! **The game trails the herds wore before the game began** (issue #215) — every migratory herd's
//! corridor stamped as a full `route:trail` at world creation by `fauna::stamp_migratory_game_trails`.
//!
//! Every test here builds its world through the **shipped Startup chain** (`build_test_app` +
//! `run_schedule(Startup)`), which is where the stamp runs — after the graze layer, so the trace reads
//! the same land a live `Migrate` leg reads. Startup alone is run, not a turn, so what is asserted is
//! the world as it is handed to the first turn.

use std::collections::HashSet;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Query, Res};

use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::save::{encode_save, load_save};
use core_sim::{
    advance_roads, migratory_corridor_tiles, spawn_initial_herds, traffic_ceiling, GrazeRegistry,
    HerdRegistry, LadderConfigHandle, RoadRegistry, SimulationConfig, SizeClass, Tile,
    TileRegistry,
};

/// Float tolerance for a stamped position — it is written as the ceiling itself, never summed.
const TOLERANCE: f32 = 1e-4;

/// A corridor tile's position the save test re-seats, so a re-stamp would be visible as a jump back
/// to the ceiling.
const RESEATED_POSITION: f32 = 10.0;
/// …and the herd clock it re-seats, so a re-stamp would be visible as a reset to zero.
const RESEATED_HERD_IDLE: u16 = 77;

/// The shipped world, through Startup only — worldgen, herds, forage, graze and the stamp.
fn created_world() -> App {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    app.world.insert_resource(config);
    app.world.run_schedule(bevy::app::Startup);
    app
}

/// Each migratory herd's corridor, read through the one seam the stamp traces with.
fn corridors(app: &mut App) -> Vec<(String, Vec<UVec2>, usize)> {
    app.world.run_system_once(
        |herds: Res<HerdRegistry>,
         tile_registry: Res<TileRegistry>,
         tiles: Query<&Tile>,
         graze: Res<GrazeRegistry>,
         config: Res<SimulationConfig>| {
            let width = config.grid_size.x.max(1);
            let height = config.grid_size.y.max(1);
            let wrap = config.map_topology.wrap_horizontal;
            herds
                .herds
                .iter()
                .filter(|h| h.size_class == SizeClass::Migratory)
                .map(|h| {
                    let (tiles, hemmed) = migratory_corridor_tiles(
                        h,
                        &tile_registry,
                        &tiles,
                        &graze,
                        // The stamp traced against the roads as they stood before it ran, which at
                        // world creation is none — so the bare land walk, with no trail to prefer.
                        None,
                        width,
                        height,
                        wrap,
                    );
                    (h.id.clone(), tiles, hemmed)
                })
                .collect()
        },
    )
}

fn ceiling(app: &App) -> f32 {
    traffic_ceiling(&app.world.resource::<LadderConfigHandle>().get())
}

/// **Every tile of every migratory corridor is a full trail at world creation, with a fresh herd
/// clock and no keeper — and nothing else on the map has a road.**
#[test]
fn every_migratory_corridor_starts_as_a_full_trail_and_nothing_else_does() {
    let mut app = created_world();
    let ceiling = ceiling(&app);
    let corridors = corridors(&mut app);
    assert!(
        !corridors.is_empty(),
        "liveness: the shipped map seats migratory herds"
    );

    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let mut stamped: HashSet<UVec2> = HashSet::new();
    for (herd, tiles, _) in &corridors {
        assert!(tiles.len() > 1, "{herd}'s corridor walks somewhere");
        // The trace is a walk: every step is one hex, so the stamped corridor has no gaps.
        for pair in tiles.windows(2) {
            assert!(
                hex_distance_wrapped(pair[0], pair[1], width, wrap) <= 1,
                "{herd}'s corridor steps one hex at a time: {pair:?}"
            );
        }
        stamped.extend(tiles.iter().copied());
    }

    let roads = app.world.resource::<RoadRegistry>();
    for tile in &stamped {
        let road = roads
            .road(*tile)
            .unwrap_or_else(|| panic!("corridor tile {tile:?} carries a road"));
        assert!(
            (road.position() - ceiling).abs() < TOLERANCE,
            "{tile:?} is a full trail: {}",
            road.position()
        );
        assert_eq!(
            road.herd_idle_turns,
            Some(0),
            "{tile:?}'s herd clock starts now"
        );
        assert_eq!(road.idle_turns, 0, "{tile:?} reads as freshly used");
        assert!(road.keeper.is_none(), "nobody keeps a trail");
    }
    let off_corridor: Vec<UVec2> = roads
        .iter()
        .map(|(tile, _)| tile)
        .filter(|tile| !stamped.contains(tile))
        .collect();
    assert!(
        off_corridor.is_empty(),
        "no tile off a migratory corridor has a road at world creation: {off_corridor:?}"
    );
}

/// **The stamp moves no herd.** Re-running the herd spawn on the same created world — same tiles,
/// same seed — reproduces the exact layout the Startup chain left, so the stamp that ran after it
/// neither moved a herd nor drew from the spawn's random stream.
#[test]
fn the_stamp_leaves_the_herd_layout_exactly_as_spawned() {
    let mut app = created_world();
    let layout = |app: &App| -> Vec<(String, Vec<UVec2>, UVec2, u32)> {
        app.world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .map(|h| {
                (
                    h.id.clone(),
                    h.route.clone(),
                    h.current_pos,
                    h.biomass.to_bits(),
                )
            })
            .collect()
    };
    let after_startup = layout(&app);
    assert!(!after_startup.is_empty(), "liveness: herds were spawned");

    app.world.resource_mut::<HerdRegistry>().herds.clear();
    app.world.run_system_once(spawn_initial_herds);
    assert_eq!(
        layout(&app),
        after_startup,
        "a fresh spawn on the same world reproduces the Startup layout bit for bit"
    );
}

/// **A loaded save carries the trails it was saved with and stamps nothing again** — the worldgen
/// chain the stamp rides is suppressed on a load, so a corridor tile re-seated before the save comes
/// back exactly as it was, not reset to a fresh full trail.
#[test]
fn a_loaded_save_does_not_restamp_the_corridors() {
    let mut app = created_world();
    let corridors = corridors(&mut app);
    let tile = corridors
        .iter()
        .flat_map(|(_, tiles, _)| tiles.iter().copied())
        .next()
        .expect("liveness: a corridor tile exists");
    {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        let road = roads.road_or_trail(tile, &ladder);
        road.set_position(RESEATED_POSITION, &ladder);
        road.herd_idle_turns = Some(RESEATED_HERD_IDLE);
    }

    let blob = encode_save(&app.world).expect("the world encodes");
    let (mut loaded, _) = load_save(&blob).expect("the save loads");
    loaded.world.run_schedule(bevy::app::Startup);

    let roads = loaded.world.resource::<RoadRegistry>();
    let road = roads.road(tile).expect("the trail survived the load");
    assert!(
        (road.position() - RESEATED_POSITION).abs() < TOLERANCE,
        "the load did not re-stamp the tile to a full trail: {}",
        road.position()
    );
    assert_eq!(road.herd_idle_turns, Some(RESEATED_HERD_IDLE));
    assert_eq!(
        roads.iter().count(),
        app.world.resource::<RoadRegistry>().iter().count(),
        "the load neither added nor lost a road"
    );
}

/// **A corridor whose herd is gone holds for the herd grace, then bleeds away at the people rate.**
/// With every herd removed and only the road pass running, a stamped tile stands at the ceiling for
/// `herd_disuse_grace_turns`, then loses `disuse_loss_per_turn` a turn until it is pruned — exactly
/// `herd grace + ceiling / loss` turns after world creation.
#[test]
fn an_abandoned_corridor_bleeds_after_the_herd_grace_and_the_people_span() {
    let mut app = created_world();
    let corridors = corridors(&mut app);
    let tile = corridors
        .iter()
        .flat_map(|(_, tiles, _)| tiles.iter().copied())
        .next()
        .expect("liveness: a corridor tile exists");
    app.world.resource_mut::<HerdRegistry>().herds.clear();

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let ceiling = traffic_ceiling(&ladder);
    let herd_grace = ladder.route_traffic.herd_disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    let people_span = (ceiling / loss).ceil() as u32;

    let position = |app: &App| {
        app.world
            .resource::<RoadRegistry>()
            .road(tile)
            .map(|r| r.position())
    };
    for _ in 0..herd_grace {
        app.world.run_system_once(advance_roads);
    }
    assert_eq!(
        position(&app).map(|p| (p - ceiling).abs() < TOLERANCE),
        Some(true),
        "the trail holds through the herd grace"
    );
    app.world.run_system_once(advance_roads);
    let first_bleed = position(&app).expect("still standing one turn past the grace");
    assert!(
        (first_bleed - (ceiling - loss)).abs() < TOLERANCE,
        "one turn past the herd grace it loses the flat rate: {first_bleed}"
    );
    for _ in 1..people_span - 1 {
        app.world.run_system_once(advance_roads);
    }
    assert!(
        position(&app).is_some(),
        "still standing one turn before the span runs out"
    );
    app.world.run_system_once(advance_roads);
    assert_eq!(
        position(&app),
        None,
        "gone — and pruned — exactly herd grace + ceiling / loss turns after creation"
    );
}
