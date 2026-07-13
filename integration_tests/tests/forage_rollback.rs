mod common;

use core_sim::{
    build_headless_app, restore_world_from_snapshot, FactionId, ForageRegistry, SnapshotHistory,
};

/// Regression: the authoritative `ForageRegistry` (per-patch biomass / ecology phase) must
/// round-trip through the rollback snapshot, mirroring the herd-registry rewind. A mutate-then-
/// restore must rewind the patch exactly — depletion is meaningless if it resets on rollback.
#[test]
fn forage_registry_biomass_rewinds_on_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();

    // Turn 1: worldgen seeds forage patches on every `FoodModuleTag` tile and captures the ring.
    app.update();

    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        let stored = history.latest_entry().expect("snapshot captured");
        stored.snapshot.clone()
    };
    assert!(
        !snapshot.forage_registry.is_empty(),
        "capture must persist the authoritative forage registry"
    );

    // Grab a live patch's captured biomass/phase.
    let (tile, biomass0, phase0) = {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry
            .patches
            .values()
            .next()
            .expect("at least one forage patch seeded");
        (patch.tile, patch.biomass, patch.ecology_phase)
    };

    // Mutate the live patch well away from its captured state (heavy depletion).
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(tile).expect("mutable patch");
        patch.biomass = 1.0;
        patch.ecology_phase = core_sim::EcologyPhase::Collapsing;
    }

    // Roll back to the captured snapshot.
    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());

    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(tile).expect("patch present after restore");
    assert_eq!(patch.biomass, biomass0, "patch biomass must rewind");
    assert_eq!(
        patch.ecology_phase, phase0,
        "patch ecology phase must rewind"
    );
    // A newly-seeded patch starts full at its carrying capacity.
    assert_eq!(patch.biomass, patch.carrying_capacity);
}

/// Cultivation state (`cultivation_progress`/`owner`, Phase 1a) must round-trip through the
/// rollback snapshot exactly like biomass — a mutate-then-restore must rewind a patch's cultivation
/// to its captured value, mirroring the biomass rewind.
#[test]
fn forage_registry_cultivation_rewinds_on_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();

    // Turn 1: seed patches (all uncultivated) and capture the ring.
    app.update();
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        history
            .latest_entry()
            .expect("snapshot captured")
            .snapshot
            .clone()
    };

    // Grab a live patch's captured (uncultivated) cultivation state.
    let (tile, progress0, owner0) = {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry
            .patches
            .values()
            .next()
            .expect("at least one forage patch seeded");
        (patch.tile, patch.cultivation_progress, patch.owner)
    };
    assert_eq!(progress0, 0.0, "a freshly-seeded patch is uncultivated");
    assert_eq!(owner0, None);

    // Mutate: claim the patch as a cultivated crop for a faction.
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(tile).expect("mutable patch");
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(3));
        assert!(patch.is_cultivated());
    }

    // Roll back to the captured (uncultivated) snapshot.
    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());

    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(tile).expect("patch present after restore");
    assert_eq!(
        patch.cultivation_progress, progress0,
        "cultivation progress must rewind"
    );
    assert_eq!(patch.owner, owner0, "cultivation owner must rewind");
    assert!(!patch.is_cultivated());
}
