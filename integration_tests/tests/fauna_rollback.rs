mod common;

use bevy::math::UVec2;
use core_sim::{
    build_headless_app, restore_world_from_snapshot, FactionId, HerdRegistry, SnapshotHistory,
};

/// Regression: the authoritative `HerdRegistry` (biomass / position / movement / domestication)
/// must round-trip through the rollback snapshot. Before the `HerdState` capture/restore was
/// added, only the lossy display telemetry was persisted, so a rollback silently kept the herd's
/// post-rollback biomass and position. This asserts a mutate-then-restore rewinds the herd exactly.
#[test]
fn herd_registry_biomass_and_position_rewind_on_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();

    // Turn 1: worldgen seeds herds and `capture_snapshot` records the ring entry.
    app.update();

    // Snapshot A (pre-mutation), plus the live herd's captured identity/state.
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        let stored = history.latest_entry().expect("snapshot captured");
        stored.snapshot.clone()
    };
    assert!(
        !snapshot.herd_registry.is_empty(),
        "capture must persist the authoritative herd registry, not just display telemetry"
    );

    let (herd_id, biomass0, pos0, route0, progress0, owner0) = {
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry
            .entries()
            .first()
            .expect("at least one herd spawned");
        (
            herd.id.clone(),
            herd.biomass,
            herd.current_pos,
            herd.route.clone(),
            herd.domestication_progress,
            herd.owner,
        )
    };

    // Mutate the live herd well away from its captured state.
    let mutated_pos = UVec2::new(pos0.x.wrapping_add(1) % 24, pos0.y.wrapping_add(1) % 16);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id == herd_id)
            .expect("mutable herd");
        herd.biomass = biomass0 + 5_000.0;
        herd.current_pos = mutated_pos;
        herd.route.push(UVec2::new(23, 15));
        herd.domestication_progress = 0.9;
        herd.owner = Some(FactionId(7));
    }
    assert_ne!(mutated_pos, pos0, "mutation must actually move the herd");

    // Roll back to snapshot A.
    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());

    // The authoritative registry is rewound to the captured values.
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&herd_id)
        .expect("herd present after rollback restore");
    assert_eq!(herd.biomass, biomass0, "biomass must rewind");
    assert_eq!(herd.current_pos, pos0, "position must rewind");
    assert_eq!(herd.route, route0, "route must rewind");
    assert_eq!(herd.domestication_progress, progress0);
    assert_eq!(herd.owner, owner0);
}

/// Slice 2 (`docs/plan_fauna_neglect_escape.md` §4 item 1): the **under-herded edge-gate** is
/// snapshot-persisted, so a rollback rewinds it and the notice does not spuriously re-fire after a
/// restore (unlike the transient `pen_starving`, which re-announces). A mutate-then-restore must bring
/// `Herd::under_herded` back to its captured value.
#[test]
fn under_herded_edge_state_rewinds_on_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();

    // Turn 1: worldgen seeds herds.
    app.update();

    // Make a herd genuinely under-contained (owned, over-stocked, zero herders) so the next turn's
    // `advance_husbandry` latches `under_herded = true`.
    let herd_id = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().next().expect("a herd spawned");
        herd.owner = Some(FactionId(0));
        herd.domestication_progress = 1.0;
        herd.herded_fraction = 0.0; // no herders → it will shed
        herd.under_herded = false;
        herd.id.clone()
    };

    // Turn 2: it sheds → the edge latches → captured in this turn's snapshot.
    app.update();
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        history
            .latest_entry()
            .expect("snapshot captured")
            .snapshot
            .clone()
    };
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd still present")
            .under_herded,
        "an under-contained managed herd latches the under-herded edge"
    );

    // Mutate the flag off the captured value.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry
            .herds
            .iter_mut()
            .find(|h| h.id == herd_id)
            .expect("mutable herd")
            .under_herded = false;
    }

    // Roll back: the persisted edge-gate rewinds to true.
    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd present after rollback restore")
            .under_herded,
        "the under-herded edge-gate must rewind on rollback (persisted, not transient)"
    );
}
