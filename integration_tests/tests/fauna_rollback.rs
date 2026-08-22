mod common;

use bevy::math::UVec2;
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{build_test_app, FactionId, HerdRegistry};

/// Regression: the authoritative `HerdRegistry` (biomass / position / movement / domestication)
/// must round-trip through rollback. Before herd capture/restore was added, only the lossy display
/// telemetry was persisted, so a rollback silently kept the herd's post-rollback biomass and
/// position. This asserts a mutate-then-restore rewinds the herd exactly.
#[test]
fn herd_registry_biomass_and_position_rewind_on_rollback() {
    common::ensure_test_config();
    let mut app = build_test_app();

    // Turn 1: worldgen seeds herds and `capture_snapshot` records the ring entry.
    app.update();

    // Snapshot A (pre-mutation), plus the live herd's captured identity/state.
    // The checkpoint, taken the way the server's rollback path takes it.
    let checkpoint = capture_sim_state(&app.world);
    assert!(
        !checkpoint.herds.entries().is_empty(),
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
            herd.rung_work_done(
                core_sim::RungKey::AnimalPastoral,
                &core_sim::LadderConfig::builtin(),
            ),
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
        // Part-tamed: banked work against a job it has not paid off.
        herd.set_ladder_position(
            core_sim::FABRICATED_BUILD_COST / 2.0,
            &core_sim::LadderConfig::builtin(),
        );
        herd.owner = Some(FactionId(7));
    }
    assert_ne!(mutated_pos, pos0, "mutation must actually move the herd");

    // Roll back to snapshot A.
    restore_sim_state(&mut app.world, &checkpoint);

    // The authoritative registry is rewound to the captured values.
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&herd_id)
        .expect("herd present after rollback restore");
    assert_eq!(herd.biomass, biomass0, "biomass must rewind");
    assert_eq!(herd.current_pos, pos0, "position must rewind");
    assert_eq!(herd.route, route0, "route must rewind");
    assert_eq!(
        herd.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin()
        ),
        progress0
    );
    assert_eq!(herd.owner, owner0);
}

/// **A rollback rewinds the QUARRY'S WOUNDS** (`docs/plan_hunt_through_combat.md` §4.2). The
/// cross-turn damage ledger is durable herd state like any other — a party seven turns into wearing
/// a mammoth down must resume seven turns in, not start over — and it is the newest field on `Herd`,
/// so this is the guard that the checkpoint's whole-registry clone actually carried it.
///
/// Asserted on **both halves of the ledger**: the banked damage *and* the in-contact flag that gates
/// healing. Restoring the damage while dropping the flag would hand the herd a free turn of recovery
/// after every rollback — a silent, drifting divergence rather than a visible one.
#[test]
fn a_quarry_s_accumulated_wounds_rewind_on_rollback() {
    common::ensure_test_config();
    let mut app = build_test_app();
    app.update();

    /// Damage no default produces, banked on the herd before the checkpoint is taken.
    const CAPTURED_DAMAGE: f32 = 137.0;
    /// The damage the ledger is dragged to afterwards — the value the rollback must throw away.
    const DRAGGED_DAMAGE: f32 = 4.0;
    /// A body deep enough that neither figure completes an animal, so both stay banked.
    const DEEP_BODY: core_sim::CombatStats = core_sim::CombatStats {
        attack: 0.0,
        defense: 0.0,
        durability: 1_000.0,
        range: core_sim::RangeBand::Melee,
        wariness: 0.0,
    };

    let herd_id = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .first_mut()
            .expect("at least one herd spawned");
        herd.wounds.strike(CAPTURED_DAMAGE, &DEEP_BODY, 1.0);
        herd.id.clone()
    };
    assert_eq!(
        wounds_of(&app, &herd_id).pending(),
        CAPTURED_DAMAGE,
        "the fixture must bank the damage it claims to"
    );

    let checkpoint = capture_sim_state(&app.world);

    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id == herd_id)
            .expect("mutable herd");
        herd.wounds = core_sim::DamageLedger::default();
        herd.wounds.strike(DRAGGED_DAMAGE, &DEEP_BODY, 1.0);
    }
    assert_eq!(
        wounds_of(&app, &herd_id).pending(),
        DRAGGED_DAMAGE,
        "the edit must land, or the rewind below asserts nothing"
    );

    restore_sim_state(&mut app.world, &checkpoint);

    let restored = wounds_of(&app, &herd_id);
    assert_eq!(
        restored.pending(),
        CAPTURED_DAMAGE,
        "the banked damage must rewind"
    );
    // The contact flag rides with it: `strike` set it, so a restored herd is still mid-hunt and does
    // not heal on the first post-restore Logistics pass.
    assert!(
        !restored.is_clean(),
        "the in-contact half of the ledger must rewind too, or a rollback grants a free heal"
    );
}

fn wounds_of(app: &bevy::app::App, herd_id: &str) -> core_sim::DamageLedger {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("herd present")
        .wounds
}

/// Slice 2 (`docs/plan_fauna_neglect_escape.md` §4 item 1): the **under-herded edge-gate** is
/// snapshot-persisted, so a rollback rewinds it and the notice does not spuriously re-fire after a
/// restore (unlike the transient `pen_starving`, which re-announces). A mutate-then-restore must bring
/// `Herd::under_herded` back to its captured value.
#[test]
fn under_herded_edge_state_rewinds_on_rollback() {
    common::ensure_test_config();
    let mut app = build_test_app();

    // Turn 1: worldgen seeds herds.
    app.update();

    // Make a herd genuinely under-contained (owned, over-stocked, zero herders) so the next turn's
    // `advance_husbandry` latches `under_herded = true`.
    let herd_id = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.first_mut().expect("a herd spawned");
        herd.owner = Some(FactionId(0));
        herd.set_ladder_position(
            core_sim::FABRICATED_BUILD_COST,
            &core_sim::LadderConfig::builtin(),
        );
        herd.upkeep_supplied = 0.0; // no keepers → it will shed
        herd.under_herded = false;
        herd.id.clone()
    };

    // Turn 2: it sheds → the edge latches → captured in this turn's snapshot.
    app.update();
    // The checkpoint, taken the way the server's rollback path takes it.
    let checkpoint = capture_sim_state(&app.world);
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
    restore_sim_state(&mut app.world, &checkpoint);
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd present after rollback restore")
            .under_herded,
        "the under-herded edge-gate must rewind on rollback (persisted, not transient)"
    );
}
