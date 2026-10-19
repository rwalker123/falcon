//! **A herd's owner is on the ENCODED frame**, the forage patch's `hasOwner`/`owner` pair: an untouched
//! herd carries none, and a herd one Tame turn up carries the faction taming it — the viewer's or a
//! rival's — so a client can name another people's taming.

use bevy::app::App;
use core_sim::{
    build_test_app, recapture_snapshot_in_place, FactionId, HerdRegistry, SimulationConfig,
    SnapshotHistory,
};

fn world() -> (App, String) {
    let mut app = build_test_app();
    // Fog off, so the untouched herd is on the wire whatever it stands on.
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.update();
    let id = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.id.starts_with("game_"))
        .map(|herd| herd.id.clone())
        .expect("the map seeded short-range game");
    (app, id)
}

/// `(hasOwner, owner)` of the herd's row, off the encoded buffer.
fn published_owner(app: &mut App, id: &str) -> (bool, u32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.subsistence())
        .and_then(|section| section.herds())
        .expect("the herd list is published")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the herd is on the wire");
    (row.hasOwner(), row.owner())
}

fn set_owner(app: &mut App, id: &str, owner: Option<FactionId>) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
        .expect("the herd")
        .owner = owner;
}

#[test]
fn an_untouched_herd_has_no_owner_and_a_tamed_one_names_its_people() {
    let (mut app, id) = world();
    assert_eq!(
        published_owner(&mut app, &id),
        (false, 0),
        "an untouched herd carries no owner"
    );

    // Faction 0 is a real id, so the bool is what separates it from "none".
    set_owner(&mut app, &id, Some(FactionId(0)));
    assert_eq!(published_owner(&mut app, &id), (true, 0));

    // And another people's taming reads as theirs.
    set_owner(&mut app, &id, Some(FactionId(3)));
    assert_eq!(published_owner(&mut app, &id), (true, 3));

    set_owner(&mut app, &id, None);
    assert_eq!(published_owner(&mut app, &id), (false, 0));
}
