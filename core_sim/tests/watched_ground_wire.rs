//! `predatorsKeptOff` on the encoded frame (#875): how many wild predator packs a band's watched
//! ring changed the move of this turn, read through the client's own accessor chain.

mod faction_support;

use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{App, Entity, With};
use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::staffing::ScoutCrew;
use core_sim::{
    advance_herds, publish_baseline_snapshot, Herd, HerdRegistry, LaborAllocation,
    PopulationCohort, ResidentBand, SimulationConfig, SizeClass, SnapshotHistory, Tile,
    TileRegistry,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_runtime::TerrainTags;

/// Tiles the scouts reach past the local range.
const REACH: u32 = 2;
/// How far from the camp the pack stands: well inside the ring (3 + 2).
const PACK_DISTANCE: u32 = 2;

fn seat(app: &mut App, id: &str, species: &str, at: UVec2) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            id.to_string(),
            species.to_string(),
            SizeClass::Small,
            vec![at],
            120.0,
            120.0,
            0.0,
            0.15,
            30.0,
            core_sim::shipped::root_standing(core_sim::shipped::animal()),
        ));
}

/// Stage a wolf pack and a deer inside the home band's ring (with `reach` of scouts), run the
/// movement, and return `(sim value, encoded value)` of `predators_kept_off`.
fn kept_off(reach: u32) -> (u32, u32) {
    let mut app = one_faction_world();
    let (band, camp) = {
        let (entity, tile) = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>()
            .iter(&app.world)
            .filter(|(_, cohort)| cohort.faction == HOME)
            .map(|(entity, cohort)| (entity, cohort.current_tile))
            .next()
            .expect("the home faction has a band");
        (entity, app.world.get::<Tile>(tile).unwrap().position)
    };
    let (width, height, wrap) = {
        let config = app.world.resource::<SimulationConfig>();
        (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        )
    };
    let mut spots = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let entity = app.world.resource::<TileRegistry>().index(x, y).unwrap();
            let water = app
                .world
                .get::<Tile>(entity)
                .unwrap()
                .terrain_tags
                .contains(TerrainTags::WATER);
            if !water && hex_distance_wrapped(camp, pos, width, wrap) == PACK_DISTANCE {
                spots.push(pos);
            }
        }
    }
    let wolf = *spots.first().expect("a land tile inside the ring");
    let deer = *spots.get(1).expect("a second land tile inside the ring");
    app.world.resource_mut::<HerdRegistry>().herds.clear();
    seat(&mut app, "wolf", "Grey Wolf Pack", wolf);
    seat(&mut app, "deer", "Red Deer", deer);
    if reach > 0 {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.set_scouting_range(reach, None);
        allocation.apply_scouting(ScoutCrew {
            kitted: 0,
            bare: reach,
            reach,
        });
    }
    app.world.run_system_once(advance_herds);
    let simulated = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .predators_kept_off;

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let published = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire")
        .predatorsKeptOff();
    (simulated, published)
}

#[test]
fn a_band_with_scouts_publishes_the_packs_its_ring_kept_off_and_one_without_publishes_none() {
    assert_eq!(
        kept_off(REACH),
        (1, 1),
        "the pack inside the ring is kept off, and the frame says so"
    );
    assert_eq!(kept_off(0), (0, 0), "no scouts, no ring, nothing kept off");
}
