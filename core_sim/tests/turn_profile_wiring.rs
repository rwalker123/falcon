//! Guards that the per-phase profiler is actually *scheduled*, not merely compiled.
//!
//! `turn_profile`'s own unit tests cover the accumulator. What they cannot see is the wiring in
//! `build_headless_app`: the stage markers are ordinary Bevy systems pinned between the chained
//! `TurnStage` sets, so adding a stage and forgetting its marker, or losing a marker to a schedule
//! edit, would silently drop that stage from every operator's `turn.profile` line with nothing
//! failing.

use core_sim::{build_headless_app, run_turn, turn_profile, SnapshotHistory};

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
/// **After #393 there is no encode here at all.** What the turn thread still does to a snapshot is
/// build it and hand it over; hashing, diffing and encoding are the publisher's
/// ([`PUBLISHER_PHASES`]). `snapshot.handoff` is the move onto the publisher's queue, and it is
/// listed precisely because it is the one phase that *can* grow: it is where a turn waits when the
/// publisher has fallen a full queue behind.
///
/// The `snapshot.build.*` entries are the capture's own passes. `sow_refusals` and `flora` were
/// folded into the single `patches` sweep in #387 — a merged label, not a lost one.
const EXPECTED_CAPTURE_PHASES: [&str; 5] = [
    "snapshot.build",
    "snapshot.build.tiles",
    "snapshot.build.patches",
    "snapshot.build.rasters",
    "snapshot.handoff",
];

/// Phases that must appear on the **publisher's** profile for a steady-state turn.
///
/// Every encode the server still pays for per turn is here, which is what makes the pair of lists a
/// statement about where the work went rather than about what stopped happening. After #388 a steady
/// turn produces exactly one encode, `publish.encode.flat_delta`: the flat socket is the only
/// socket, and a delta is all it is sent.
const PUBLISHER_PHASES: [&str; 3] = [
    "publish.finalize_hash",
    "publish.diff",
    "publish.encode.flat_delta",
];

/// Labels that must NOT appear on a steady-state turn, and why each one left.
///
/// Two different reasons, deliberately kept in one list because the assertion is the same: a turn
/// that starts paying for any of them again has regressed.
///
/// * **Relocated by #393** — `snapshot.finalize_hash`, `snapshot.history`, `snapshot.history.diff`
///   and `encode.flat_delta` are still work the server does every turn, on the publisher thread and
///   under `publish.*` names. Seeing an *old* name here means publication has crept back onto the
///   turn thread, which is the whole regression this issue was about.
/// * **Retired encodes** — `encode.flat_snapshot` is still reachable but not per turn (a world's
///   first publication, rollback, `resync` — #386). The two **bincode** labels are gone for good:
///   #388 retired the bincode snapshot socket, and its frames were never decodable in the first
///   place (`skip_serializing_if` in a non-self-describing format), so nothing consumes either.
const RETIRED_CAPTURE_PHASES: [&str; 7] = [
    "snapshot.finalize_hash",
    "snapshot.history",
    "snapshot.history.diff",
    "encode.flat_delta",
    "encode.flat_snapshot",
    "encode.bincode_snapshot",
    "encode.bincode_delta",
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

    for retired in RETIRED_CAPTURE_PHASES {
        assert!(
            !labels.contains(&retired),
            "`{retired}` is back on the steady-state turn path — it is either an encode nothing \
             reads (#386/#388) or publication work that belongs on the publisher thread (#393) \
             (rendered: {})",
            turn_profile::render(&phases)
        );
    }

    // The other half of the relocation: the work did not vanish, it moved. Asserting only its
    // absence from the turn would pass just as well if publication had silently stopped happening.
    let published = app
        .world
        .resource::<SnapshotHistory>()
        .last_publish_profile();
    let published_labels: Vec<&str> = published.iter().map(|phase| phase.label).collect();
    for expected in PUBLISHER_PHASES {
        assert!(
            published_labels.contains(&expected),
            "publisher phase `{expected}` vanished — a steady turn still hashes, diffs and encodes \
             one delta, it just does it off the turn thread (rendered: {})",
            turn_profile::render(&published)
        );
    }
}
