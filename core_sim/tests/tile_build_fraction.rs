//! **A TILE IMPROVEMENT PUBLISHES ITS BUILD METER** (#709) - `TileImprovementState.buildFraction`,
//! the tile kind's twin of the deposit, road and patch meters, read off the ENCODED snapshot. A pit
//! being dug with some of its work banked must read strictly between 0 and 1, and must be the
//! shared `intensification::build_fraction` of the rung being raised.

use bevy::app::App;
use bevy::math::UVec2;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::{
    build_fraction, build_test_app, recapture_snapshot_in_place, tile_rung_span, BranchId,
    FactionId, LadderConfigHandle, RungId, SimulationConfig, SnapshotHistory,
    TileImprovementRegistry, ViewerFaction,
};

const FACTION: FactionId = FactionId(0);
const TILE: (u32, u32) = (17, 24);
/// Work banked on the pit, well short of its cost.
const BANKED: f32 = 5.0;

fn food_stores() -> BranchId {
    BranchId::new("food_stores")
}
fn storage_pit() -> RungId {
    RungId::new(food_stores(), "storage_pit")
}

fn published_fraction(app: &mut App, tile: UVec2) -> f32 {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .routes()
        .and_then(|section| section.tileImprovements())
        .expect("the improvement section")
        .iter()
        .find(|row| row.tileX() == tile.x && row.tileY() == tile.y)
        .expect("the improvement's row is published")
        .buildFraction()
}

#[test]
fn a_pit_part_dug_publishes_the_shared_build_fraction_of_its_rung() {
    let mut app = build_test_app();
    app.update();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.world.insert_resource(ViewerFaction(FACTION));
    let tile = UVec2::new(TILE.0, TILE.1);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = tile_rung_span(storage_pit(), &ladder);
    app.world.resource_mut::<TileImprovementRegistry>().place(
        tile,
        food_stores(),
        FACTION,
        &ladder,
    );

    assert_eq!(
        published_fraction(&mut app, tile),
        0.0,
        "nothing banked reads 0"
    );

    app.world
        .resource_mut::<TileImprovementRegistry>()
        .get_mut(tile, food_stores())
        .expect("placed")
        .set_position(base + BANKED, &ladder);
    let published = published_fraction(&mut app, tile);
    assert!(
        published > 0.0 && published < 1.0,
        "part dug reads strictly between 0 and 1, got {published}"
    );
    assert_eq!(
        published,
        build_fraction(BANKED, width),
        "and is the shared seam's answer for the rung being raised"
    );
}
