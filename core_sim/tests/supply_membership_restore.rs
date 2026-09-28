//! **A restored world publishes the exchange network the world it restored did.**
//!
//! `SupplyNetworkMembership` is rebuilt every turn by `balance_supply_networks`, so nothing a turn
//! steers by lives in it — but the capture publishes it (`supplyNetworkId`, `poolingLinks`,
//! `supplyNetworkSpanTiles`), and a restored world is captured before any turn runs. A save load
//! publishes the client's first frame straight off the restore; left out of the checkpoint, that
//! frame drew every band as network 0 with no links, and the exchange-network overlay stayed empty
//! until the next turn. So each band's membership rides its `BandRecord`.
//!
//! **A dead field cannot diverge**: every comparison below is paired with a liveness assertion that
//! the network really formed, because a world with no network round-trips "no links" to "no links"
//! and would pass with the membership dropped entirely.

use std::collections::BTreeMap;

use bevy::prelude::*;

use core_sim::save::{encode_save, load_save};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    build_test_app, publish_baseline_snapshot, run_turn, split_band_from_parent, BandId,
    BandSupplyMembership, PopulationCohort, ResidentBand, Scalar, SettleConfig, SimulationConfig,
    SnapshotHistory, SupplyNetworkMembership,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// Turns resolved before the split, so the world has run rather than being bare worldgen output.
const TURNS_BEFORE_SPLIT: usize = 2;

/// The most turns the co-located pair is given to find each other and pool. Contact is found by the
/// sight sweep and the tie has to be live before the balancer links them, so it is not the turn of
/// the split itself.
const MAX_TURNS_TO_NETWORK: usize = 6;

/// Workers handed to the second band — over `settle.min_founding_workers`, and small enough that
/// the parent stays a real band.
const SPLIT_WORKERS: u32 = 5;

/// Working-age people the parent is stocked with before splitting, so both halves are real bands.
const PARENT_WORKERS: f32 = 20.0;

/// The shipped load path, stated the way `save_round_trip.rs` states it: earthlike on the harness
/// seed, with the **shipped** equipment config so a reload (which boots `build_headless_app` and
/// reads the shipped `equipment.json`) differs from the live world in nothing but the save.
fn spawn_world() -> App {
    let mut app = build_test_app();
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn first_resident_band(app: &mut App) -> Entity {
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band")
}

/// Every band's membership, keyed by the durable [`BandId`] rather than the `Entity` a restore
/// replaces.
fn memberships(app: &mut App) -> BTreeMap<BandId, BandSupplyMembership> {
    let bands: Vec<(Entity, BandId)> = app
        .world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .map(|(entity, id)| (entity, *id))
        .collect();
    let resource = app.world.resource::<SupplyNetworkMembership>();
    bands
        .into_iter()
        .map(|(entity, id)| {
            (
                id,
                BandSupplyMembership {
                    network_id: resource.network_of(entity),
                    links: resource.pooling_links_of(entity).to_vec(),
                    span_tiles: resource.span_tiles_of(entity),
                },
            )
        })
        .collect()
}

/// A world whose starting band has split into a co-located pair that has pooled — a live
/// multi-band network, which is the state worth losing.
fn a_world_with_a_network() -> App {
    let mut app = spawn_world();
    for _ in 0..TURNS_BEFORE_SPLIT {
        run_turn(&mut app);
    }
    let parent = first_resident_band(&mut app);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(parent)
            .expect("the band exists");
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");

    for _ in 0..MAX_TURNS_TO_NETWORK {
        run_turn(&mut app);
        let networked = memberships(&mut app)
            .values()
            .filter(|membership| membership.network_id != 0)
            .count();
        if networked >= 2 {
            return app;
        }
    }
    panic!("the co-located pair never formed a supply network in {MAX_TURNS_TO_NETWORK} turns");
}

/// `(supplyNetworkId, poolingLinks as (bandId, distanceTiles, rungId), supplyNetworkSpanTiles)`
/// per band, decoded from the **encoded envelope** the client reads.
type WireMembership = (u32, Vec<(u64, u32, String)>, u32);

fn wire_memberships(app: &App) -> BTreeMap<u64, WireMembership> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("a snapshot was published");
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let populations = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a full snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the band list");
    populations
        .iter()
        .map(|cohort| {
            let links = cohort
                .poolingLinks()
                .map(|links| {
                    links
                        .iter()
                        .map(|link| {
                            (
                                link.bandId(),
                                link.distanceTiles(),
                                link.rungId().unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (
                cohort.bandId(),
                (
                    cohort.supplyNetworkId(),
                    links,
                    cohort.supplyNetworkSpanTiles(),
                ),
            )
        })
        .collect()
}

/// **The in-process checkpoint** — what a rollback restores from. Captured and restored into the
/// same world, every band reads the same network, links and span by `BandId`, even though every
/// band entity it was keyed by has been despawned and respawned.
#[test]
fn a_checkpoint_round_trip_keeps_every_bands_supply_membership() {
    let mut app = a_world_with_a_network();
    let before = memberships(&mut app);
    assert!(
        before
            .values()
            .any(|membership| !membership.links.is_empty()),
        "liveness: the fixture must hold pooling links worth losing: {before:?}"
    );

    let state = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &state);

    assert_eq!(
        memberships(&mut app),
        before,
        "a restored world must read the membership the captured one did, band for band"
    );
}

/// **The save load, at the published level.** The live world's frame and the loaded world's first
/// frame — published by `publish_baseline_snapshot` with no turn in between, exactly as a load is —
/// carry the same `supplyNetworkId`, `poolingLinks` and `supplyNetworkSpanTiles` for every band.
#[test]
fn a_loaded_world_publishes_the_exchange_network_the_live_one_did() {
    let original = a_world_with_a_network();
    let live = wire_memberships(&original);
    assert!(
        live.values()
            .filter(|(network, _, _)| *network != 0)
            .count()
            >= 2,
        "liveness: the live frame must publish a multi-band network: {live:?}"
    );
    assert!(
        live.values().any(|(_, links, _)| !links.is_empty()),
        "liveness: the live frame must publish pooling links: {live:?}"
    );

    let blob = encode_save(&original.world).expect("the world encodes");
    let (mut loaded, _) = load_save(&blob).expect("the save loads");
    publish_baseline_snapshot(&mut loaded.world);

    assert_eq!(
        wire_memberships(&loaded),
        live,
        "the loaded world's first frame must publish the exchange network the live world did"
    );
}
