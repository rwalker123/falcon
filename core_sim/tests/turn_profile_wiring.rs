//! Guards that the per-phase profiler is actually *scheduled*, not merely compiled.
//!
//! `turn_profile`'s own unit tests cover the accumulator. What they cannot see is the wiring in
//! `build_headless_app`: the stage markers are ordinary Bevy systems pinned between the chained
//! `TurnStage` sets, so adding a stage and forgetting its marker, or losing a marker to a schedule
//! edit, would silently drop that stage from every operator's `turn.profile` line with nothing
//! failing.

use core_sim::{build_headless_app, run_turn, turn_profile};

/// Every `TurnStage`, in `configure_sets` order. A new stage belongs here and in the marker block
/// in `lib.rs`.
const EXPECTED_STAGES: [&str; 11] = [
    "influence",
    "logistics",
    "knowledge",
    "great_discovery",
    "population",
    "visibility",
    "crisis",
    "telling",
    "finalize",
    "victory",
    "snapshot",
];

/// The capture sub-phases, which live in `snapshot/capture.rs` rather than the schedule.
///
/// **Every encode still on the turn path is listed** — this is "the live encodes", not an arbitrary
/// subset, so an encode that survives a future pass belongs here and one that is genuinely retired
/// comes out.
///
/// **`encode.flat_snapshot` is absent, and `encode.flat_delta` replaced it** (#386). The client
/// stream now carries a delta per turn; a full flat snapshot is encoded only for a world's first
/// publication, for rollback, and on a `resync` request — none of which is a steady-state turn.
/// This test failing with "encode.flat_snapshot vanished" is exactly the alarm working: it is the
/// only thing that would have caught the turn path silently losing an encode.
const EXPECTED_CAPTURE_PHASES: [&str; 11] = [
    "snapshot.build",
    "snapshot.build.tiles",
    "snapshot.build.sow_refusals",
    "snapshot.build.flora",
    "snapshot.build.rasters",
    "snapshot.finalize_hash",
    "snapshot.history",
    "snapshot.history.diff",
    "encode.bincode_snapshot",
    "encode.bincode_delta",
    "encode.flat_delta",
];

#[test]
fn a_resolved_turn_profiles_every_stage_in_order() {
    let mut app = build_headless_app();
    // Warm turn first: the profile under test must be a steady-state turn, not first-capture.
    run_turn(&mut app);

    turn_profile::begin_turn();
    run_turn(&mut app);
    let phases = turn_profile::take();

    let labels: Vec<&str> = phases.iter().map(|phase| phase.label).collect();

    // The stages must appear, and in schedule order — the markers are only correct if each one sits
    // between the right pair of sets.
    let staged: Vec<&str> = labels
        .iter()
        .copied()
        .filter(|label| EXPECTED_STAGES.contains(label))
        .collect();
    assert_eq!(
        staged,
        EXPECTED_STAGES.to_vec(),
        "every TurnStage needs a `stage_marker` pinned between its neighbours (rendered: {})",
        turn_profile::render(&phases)
    );

    for expected in EXPECTED_CAPTURE_PHASES {
        assert!(
            labels.contains(&expected),
            "capture sub-phase `{expected}` vanished from the profile (rendered: {})",
            turn_profile::render(&phases)
        );
    }
}
