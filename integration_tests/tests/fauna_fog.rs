//! **Fog of war reaches the fauna layer on the WIRE** (issue #264).
//!
//! `herd_snapshot_entries` used to export every live herd to every client, so a player could read
//! the position, species, biomass and heading of animals standing on ground they had never seen —
//! wire-level fog was decorative for fauna. These tests drive the **real** headless sim through a
//! real turn and assert on the **encoded FlatBuffers snapshot the client actually receives**, not on
//! an in-process value or a re-derivation of the filter's own arithmetic.
//!
//! Two claims, and they are different claims:
//!   1. nothing on unseen ground reaches the wire, and
//!   2. the sim still knows about all of it — only the *view* is filtered, never the sim record.

mod common;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    build_headless_app, HerdRegistry, SimulationConfig, SnapshotHistory, ViewerFaction,
    VisibilityLedger,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// Every herd id on the encoded client wire, decoded exactly the way the Godot client decodes it
/// (`bridge/decoder.rs`: envelope → snapshot payload → subsistence section → herds).
fn herd_ids_on_the_wire(bytes: &[u8]) -> Vec<String> {
    let envelope = fb::root_as_envelope(bytes).expect("the snapshot encodes to a valid envelope");
    assert_eq!(
        envelope.payload_type(),
        fb::SnapshotPayload::snapshot,
        "the stored flat payload is a full snapshot"
    );
    let snapshot = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");
    snapshot
        .subsistence()
        .and_then(|section| section.herds())
        .map(|herds| {
            herds
                .iter()
                .filter_map(|herd| herd.id().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `VisionSection.fogEnabled` as the client reads it — the published state of the server-owned
/// switch, decoded from the same bytes as the herd list so the two can be compared.
fn fog_enabled_on_the_wire(bytes: &[u8]) -> bool {
    let envelope = fb::root_as_envelope(bytes).expect("the snapshot encodes to a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .vision()
        .expect("every snapshot carries a vision section")
        .fogEnabled()
}

/// The published herd list is exactly the herds the viewer can see — no more, and the sim's own
/// registry still holds the rest.
#[test]
fn the_encoded_snapshot_publishes_only_herds_the_viewer_can_see() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    // Two updates: the first generates the world, the second resolves a turn with visibility
    // computed and a snapshot captured from it.
    app.update();
    app.update();

    let viewer = app.world.resource::<ViewerFaction>().0;

    let published = {
        let history = app.world.resource::<SnapshotHistory>();
        // The FULL frame for this tick. Under delta streaming only a world's first publication
        // carries an encoded flat snapshot, so ask the ring entry to produce one rather than
        // reading a field that is `None` on most turns.
        let bytes = history
            .latest_entry()
            .expect("a turn was captured and encoded")
            .encode_flat();
        herd_ids_on_the_wire(bytes.as_ref())
    };

    let ledger = app.world.resource::<VisibilityLedger>();
    let registry = app.world.resource::<HerdRegistry>();

    assert!(
        !registry.entries().is_empty(),
        "worldgen seeded herds — otherwise this test proves nothing"
    );

    // (1) Nothing unseen and unowned is published.
    for id in &published {
        let herd = registry
            .find(id)
            .unwrap_or_else(|| panic!("published herd {id} exists in the registry"));
        let pos = herd.position();
        assert!(
            ledger.is_visible(viewer, pos.x, pos.y) || herd.owner == Some(viewer),
            "herd {id} at ({}, {}) is published but the viewer can neither see it nor owns it",
            pos.x,
            pos.y
        );
    }

    // (2) Everything visible IS published — the filter hides, it must not drop.
    for herd in registry.entries() {
        let pos = herd.position();
        if ledger.is_visible(viewer, pos.x, pos.y) || herd.owner == Some(viewer) {
            assert!(
                published.contains(&herd.id),
                "herd {} at ({}, {}) is visible but missing from the wire",
                herd.id,
                pos.x,
                pos.y
            );
        }
    }

    // (3) The leak was real: on a fresh map the viewer's sight is a pinhole, so the wire must carry
    //     dramatically fewer herds than exist. Without this the two loops above pass vacuously on a
    //     hypothetical fully-revealed map.
    assert!(
        published.len() < registry.entries().len(),
        "a starting band sees a pinhole of an 80x52 map, so the wire must be a strict subset \
         (published {} of {} herds) — if this ever fails, check the starting visibility ledger",
        published.len(),
        registry.entries().len()
    );
}

/// **Turning fog off reveals fauna, and only the server can do it.** The filter above drops unseen
/// herds *before* the payload is encoded, so no client render flag could bring them back — which is
/// why `fog_enabled` is a `SimulationConfig` setting driven by the `set_fog` command. Asserted on
/// the encoded wire bytes, like every other claim in this file.
#[test]
fn the_encoded_snapshot_publishes_every_herd_when_the_server_disables_fog() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    // First update builds the world; flip the switch before the turn that captures the snapshot.
    app.update();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.update();

    let published = {
        let history = app.world.resource::<SnapshotHistory>();
        // The FULL frame for this tick. This is the world's SECOND publication, and under delta
        // streaming only the first carries an encoded flat snapshot — so ask the ring entry to
        // produce one rather than reading a field that is `None` on every turn but the baseline.
        let bytes = history
            .latest_entry()
            .expect("a turn was captured and encoded")
            .encode_flat();
        assert!(
            !fog_enabled_on_the_wire(bytes.as_ref()),
            "the switch is published so the client renders the state it is told"
        );
        herd_ids_on_the_wire(bytes.as_ref())
    };

    let registry = app.world.resource::<HerdRegistry>();
    assert!(
        !registry.entries().is_empty(),
        "worldgen seeded herds — otherwise this test proves nothing"
    );
    assert_eq!(
        published.len(),
        registry.entries().len(),
        "with fog off the wire carries every live herd (published {} of {})",
        published.len(),
        registry.entries().len()
    );
    for herd in registry.entries() {
        assert!(
            published.contains(&herd.id),
            "herd {} is missing from the wire with fog disabled",
            herd.id
        );
    }
}

/// **The switch is a setting, not world state.** `SimulationConfig` is deliberately outside the
/// rollback record — rewinding the world must not silently switch fog back on under the player.
#[test]
fn disabling_fog_survives_a_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    app.update();
    app.update();

    let checkpoint = capture_sim_state(&app.world);

    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    restore_sim_state(&mut app.world, &checkpoint);

    assert!(
        !app.world.resource::<SimulationConfig>().fog_enabled,
        "fog_enabled is a display setting on SimulationConfig, which rollback does not restore"
    );
}

/// **The sim record is NOT a view.** `WorldSnapshot.herd_registry` is the authoritative rollback
/// state and `export_map`'s ground truth; filtering it would rewind fauna the player hadn't seen
/// out of existence. Only the display list (`herds`) is fog-filtered.
#[test]
fn the_rollback_record_keeps_every_herd_the_display_list_hides() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    app.update();
    app.update();

    let history = app.world.resource::<SnapshotHistory>();
    let snapshot = history.last_snapshot().expect("a turn was captured");

    let live = app.world.resource::<HerdRegistry>().entries().len();
    assert_eq!(
        snapshot.herd_registry.len(),
        live,
        "the authoritative herd record carries every live herd, fog or no fog"
    );
    assert!(
        snapshot.herds.len() < snapshot.herd_registry.len(),
        "the display list is the filtered one (display {} vs record {})",
        snapshot.herds.len(),
        snapshot.herd_registry.len()
    );
}
