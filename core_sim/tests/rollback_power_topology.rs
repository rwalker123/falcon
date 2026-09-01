//! A rollback must leave the power grid routing over the same topology it routed over before.
//!
//! `PowerTopology` is `WORLD_STATIC_*`: worldgen builds it and nothing rewrites it, so it survives
//! a rollback only because a restore rebuilds into the same live `World`. That is fine for the
//! parts of it that are keyed by position — `PowerNodeId` is `y * width + x`, and `adjacency` is
//! indexed by it — and it was **not** fine for the `node_entities: Vec<Entity>` the resource used
//! to carry. `restore_sim_state` despawns and respawns every tile, so every one of those handles
//! was stale afterwards (measured: 4160 of 4160 dead on the standard test map) while `TileRegistry`
//! was correctly rebuilt beside it in pass 4a.
//!
//! **It never produced a wrong world**, because the only thing anything ever asked that vector was
//! its `.len()`, and a stale handle still counts. That is the whole reason it went unnoticed, and
//! it is why the guard below pins the *behaviour* — that the grid still routes after a restore —
//! rather than the representation, which has now changed under it once.
//!
//! The specific failure this rules out: `simulate_power` gates inter-node transfer on
//! `topology.node_count() == node_count`. If a restore ever left those two disagreeing, the grid
//! would silently stop sharing power between neighbours and every deficit would read as a real
//! shortfall rather than one the neighbour could have covered — a plausible wrong number, not an
//! obvious failure.

use bevy::prelude::*;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    build_test_app, run_turn, PowerGridState, PowerNode, PowerTopology, SimulationConfig, Tile,
};

/// Turns to run past the checkpoint before rolling back, so the restore has real work to undo
/// rather than landing on the world it was taken from.
const TURNS_PAST_CHECKPOINT: usize = 3;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn live_power_nodes(app: &mut App) -> usize {
    app.world
        .query_filtered::<Entity, (With<Tile>, With<PowerNode>)>()
        .iter(&app.world)
        .count()
}

#[test]
fn a_restore_leaves_the_topology_matching_the_world_it_routes_over() {
    let mut app = spawn_world();
    run_turn(&mut app);

    let nodes_before = live_power_nodes(&mut app);
    assert!(
        nodes_before > 0,
        "the standard map must spawn power nodes, or this test proves nothing"
    );
    assert_eq!(
        app.world.resource::<PowerTopology>().node_count(),
        nodes_before,
        "worldgen must build the topology against the nodes it spawned"
    );

    let checkpoint = capture_sim_state(&app.world);
    for _ in 0..TURNS_PAST_CHECKPOINT {
        run_turn(&mut app);
    }
    restore_sim_state(&mut app.world, &checkpoint);

    // The restore renumbers every tile entity. The topology is untouched by it — nothing rebuilds
    // this resource — so this asserts it did not NEED rebuilding.
    let nodes_after = live_power_nodes(&mut app);
    assert_eq!(
        nodes_after, nodes_before,
        "a restore reinstates exactly the tiles the checkpoint held"
    );
    assert_eq!(
        app.world.resource::<PowerTopology>().node_count(),
        nodes_after,
        "the topology's node count must still match the live grid, or `simulate_power` silently \
         stops sharing power between neighbours"
    );
}

#[test]
fn a_restored_world_resolves_the_same_power_grid_as_the_original() {
    // Capture at tick T, advance one turn, and record the grid. Then restore to T and advance the
    // same one turn: the grid must resolve identically. If the topology were stale in any way the
    // routing reads, the second run would route differently and this would diverge.
    let mut app = spawn_world();
    run_turn(&mut app);

    let checkpoint = capture_sim_state(&app.world);
    run_turn(&mut app);
    let expected = app.world.resource::<PowerGridState>().clone();

    for _ in 0..TURNS_PAST_CHECKPOINT {
        run_turn(&mut app);
    }
    restore_sim_state(&mut app.world, &checkpoint);
    run_turn(&mut app);
    let actual = app.world.resource::<PowerGridState>();

    assert_eq!(actual.total_supply, expected.total_supply);
    assert_eq!(actual.total_demand, expected.total_demand);
    assert_eq!(actual.total_storage, expected.total_storage);
    assert_eq!(actual.total_capacity, expected.total_capacity);
    assert_eq!(actual.instability_alerts, expected.instability_alerts);
    assert_eq!(actual.incidents.len(), expected.incidents.len());
    assert_eq!(actual.nodes.len(), expected.nodes.len());
}
