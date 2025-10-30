use core_sim::{build_headless_app, SnapshotHistory};
use sim_runtime::WorldSnapshot;

fn run_simulation(ticks: usize) -> WorldSnapshot {
    let mut app = build_headless_app();
    for _ in 0..ticks {
        app.update();
    }
    app.world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|snapshot| (**snapshot).clone())
        .expect("snapshot available")
}

// Keep tick count low so CI doesn't spend minutes marching the full simulation.
// A dozen updates is sufficient to populate the snapshot history for comparison.
const SNAPSHOT_TICKS: usize = 12;

#[test]
fn deterministic_snapshots_match() {
    let snapshot_a = run_simulation(SNAPSHOT_TICKS);
    let snapshot_b = run_simulation(SNAPSHOT_TICKS);

    let mut normalized_a = snapshot_a.clone();
    normalized_a.influencers.clear();
    normalized_a.header.hash = 0;

    let mut normalized_b = snapshot_b.clone();
    normalized_b.influencers.clear();
    normalized_b.header.hash = 0;

    assert_eq!(
        sim_runtime::hash_snapshot(&normalized_a),
        sim_runtime::hash_snapshot(&normalized_b)
    );

    assert_eq!(snapshot_a.header.tile_count, snapshot_b.header.tile_count);
    assert_eq!(snapshot_a.tiles, snapshot_b.tiles);
    assert_eq!(snapshot_a.logistics, snapshot_b.logistics);
    assert_eq!(snapshot_a.populations, snapshot_b.populations);
    assert_eq!(snapshot_a.power, snapshot_b.power);
    assert_eq!(snapshot_a.axis_bias, snapshot_b.axis_bias);
    assert_eq!(snapshot_a.sentiment, snapshot_b.sentiment);
    assert_eq!(snapshot_a.influencers.len(), snapshot_b.influencers.len());
    assert_eq!(snapshot_a.generations, snapshot_b.generations);
    assert_eq!(snapshot_a.corruption, snapshot_b.corruption);
}
