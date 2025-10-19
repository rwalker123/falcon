use core_sim::{build_headless_app, SnapshotHistory};
use sim_proto::WorldSnapshot;

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

#[test]
fn deterministic_snapshots_match() {
    let snapshot_a = run_simulation(120);
    let snapshot_b = run_simulation(120);

    assert_eq!(snapshot_a.header.hash, snapshot_b.header.hash);
    assert_eq!(snapshot_a.tiles, snapshot_b.tiles);
    assert_eq!(snapshot_a.logistics, snapshot_b.logistics);
    assert_eq!(snapshot_a.populations, snapshot_b.populations);
    assert_eq!(snapshot_a.power, snapshot_b.power);
    assert_eq!(snapshot_a.axis_bias, snapshot_b.axis_bias);
    assert_eq!(snapshot_a.generations, snapshot_b.generations);
}
