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
