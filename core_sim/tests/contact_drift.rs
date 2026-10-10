//! **Culture over a connection** (#702, `docs/plan_contact_and_logistics.md` §"Settled by #530").
//!
//! ```text
//! pull_A = rate x tie x receptiveness_A x weight_B / (weight_A + weight_B) x (value_B - value_A)
//! ```
//!
//! The manager-level arms drive `CultureManager::apply_contact_drift` on hand-staged layers (every
//! band neutral unless the arm says otherwise), so each factor of the rule is isolated. The wire arm
//! drives the real systems and reads the ENCODED envelope.

use std::collections::BTreeMap;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use core_sim::{
    build_test_app, mutual_contact_ties, publish_baseline_snapshot, reconcile_band_culture_layers,
    reconcile_culture_layers, scalar_from_f32, BandId, ConnectionKey, ConnectionLedger,
    ConnectionsConfigHandle, ContactTie, CultureManager, CultureOwner, CultureTraitAxis,
    InfluencerCultureResonance, ResidentBand, Scalar, Sighting, SimulationTick, SnapshotHistory,
    CULTURE_TRAIT_AXES, CULTURE_TRAIT_SPAN,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

mod faction_support;
use faction_support::one_faction_world;

const RATE: f32 = 0.014;
const A: BandId = BandId(1);
const B: BandId = BandId(2);
const C: BandId = BandId(3);
/// The axis every arm opens a gap on. Not the Purist axis, so receptiveness is set independently.
const GAP_AXIS: usize = 0;
const GAP: f32 = 1.0;

fn json_round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T) -> T {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

fn purist_axis() -> usize {
    CultureTraitAxis::SyncreticPurist.index()
}
const EQUAL_WEIGHT: f32 = 10.0;
const TOLERANCE: f32 = 1e-4;
/// Turns for the half-gap claim, and the band either side of one half it must land in.
const HALF_GAP_TURNS: usize = 50;
const HALF_GAP_LOW: f32 = 0.45;
const HALF_GAP_HIGH: f32 = 0.55;

fn manager_with(bands: &[BandId]) -> CultureManager {
    let mut manager = CultureManager::new();
    let region = manager.upsert_regional(1);
    for band in bands {
        manager.attach_band(*band, region);
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(*band))
            .unwrap();
        // Neutral: no seeded character offset, no value.
        *layer.traits.modifier_mut() = [Scalar::zero(); CULTURE_TRAIT_AXES];
        for idx in 0..CULTURE_TRAIT_AXES {
            layer.traits.update_value(idx, Scalar::zero());
        }
    }
    manager
}

fn set_value(manager: &mut CultureManager, band: BandId, axis: usize, value: f32) {
    manager
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .unwrap()
        .traits
        .update_value(axis, scalar_from_f32(value));
}

fn modifier(manager: &CultureManager, band: BandId, axis: usize) -> f32 {
    manager
        .band_layer_by_owner(CultureOwner::from_band(band))
        .unwrap()
        .traits
        .modifier()[axis]
        .to_f32()
}

fn weights(entries: &[(BandId, f32)]) -> BTreeMap<u64, Scalar> {
    entries
        .iter()
        .map(|(band, weight)| (band.0, scalar_from_f32(*weight)))
        .collect()
}

fn full_tie(a: BandId, b: BandId) -> ContactTie {
    ContactTie {
        a,
        b,
        tie: Scalar::one(),
    }
}

/// Two bands, `A` neutral and `B` a `GAP` away on `GAP_AXIS`.
fn pair() -> CultureManager {
    let mut manager = manager_with(&[A, B]);
    set_value(&mut manager, B, GAP_AXIS, GAP);
    manager
}

#[test]
fn two_equal_neutral_bands_at_a_full_tie_close_the_gap_by_the_rate() {
    let mut manager = pair();
    manager.apply_contact_drift(
        RATE,
        &weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]),
        &[full_tie(A, B)],
    );
    let closed = modifier(&manager, A, GAP_AXIS) - modifier(&manager, B, GAP_AXIS);
    // A moves up by rate/2 of the gap and B down by rate/2: together, the rate.
    assert!((closed - RATE * GAP).abs() < TOLERANCE, "closed {closed}");
}

#[test]
fn half_the_gap_closes_in_about_fifty_turns() {
    let mut manager = pair();
    // The layer resolves fully onto its modifier each turn (parent 0): value follows modifier.
    set_value(&mut manager, A, GAP_AXIS, 0.0);
    for band in [A, B] {
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(band))
            .unwrap();
        layer.traits.modifier_mut()[GAP_AXIS] = layer.traits.values()[GAP_AXIS];
    }
    let weights = weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]);
    for _ in 0..HALF_GAP_TURNS {
        manager.apply_contact_drift(RATE, &weights, &[full_tie(A, B)]);
        for band in [A, B] {
            let follow = scalar_from_f32(modifier(&manager, band, GAP_AXIS));
            manager
                .band_layer_mut_by_owner(CultureOwner::from_band(band))
                .unwrap()
                .traits
                .update_value(GAP_AXIS, follow);
        }
    }
    let gap = modifier(&manager, B, GAP_AXIS) - modifier(&manager, A, GAP_AXIS);
    assert!(
        (HALF_GAP_LOW..=HALF_GAP_HIGH).contains(&gap),
        "gap after {HALF_GAP_TURNS} turns: {gap}"
    );
}

#[test]
fn a_tie_that_runs_one_way_moves_nothing_and_a_mutual_one_is_the_weaker_side() {
    let app = build_test_app();
    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    let sighting = Sighting::new(UVec2::ZERO, 1, "");
    let mut ledger = ConnectionLedger::default();
    ledger.record_contact(ConnectionKey::new(A, B), &sighting, 1, &config);
    let residents = weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]);
    assert!(
        mutual_contact_ties(&ledger, &residents).is_empty(),
        "A->B alone is a map carried home, not a way of life"
    );

    // B->A refreshed three times is stronger than A->B once: the tie is the weaker, A->B.
    for _ in 0..3 {
        ledger.record_contact(ConnectionKey::new(B, A), &sighting, 1, &config);
    }
    let ties = mutual_contact_ties(&ledger, &residents);
    assert_eq!(ties.len(), 1);
    let forward = ledger.get(&ConnectionKey::new(A, B)).unwrap().strength;
    let back = ledger.get(&ConnectionKey::new(B, A)).unwrap().strength;
    assert!(back > forward);
    assert_eq!(ties[0].tie, forward);

    // A tie to a band that is not resident is no tie.
    assert!(mutual_contact_ties(&ledger, &weights(&[(A, EQUAL_WEIGHT)])).is_empty());

    // And a half tie moves half as far as a full one.
    let mut full = pair();
    let mut half = pair();
    let w = weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]);
    full.apply_contact_drift(RATE, &w, &[full_tie(A, B)]);
    half.apply_contact_drift(
        RATE,
        &w,
        &[ContactTie {
            a: A,
            b: B,
            tie: scalar_from_f32(0.5),
        }],
    );
    assert!((modifier(&half, A, GAP_AXIS) * 2.0 - modifier(&full, A, GAP_AXIS)).abs() < TOLERANCE);
}

#[test]
fn the_smaller_band_moves_more() {
    let mut manager = pair();
    manager.apply_contact_drift(RATE, &weights(&[(A, 10.0), (B, 30.0)]), &[full_tie(A, B)]);
    let small = modifier(&manager, A, GAP_AXIS).abs();
    let large = modifier(&manager, B, GAP_AXIS).abs();
    // A (10 of 40) is pulled by B's 30/40 share; B by A's 10/40: a ratio of three.
    assert!((small / large - 3.0).abs() < 0.01, "{small} vs {large}");
    assert!((small - RATE * 0.75 * GAP).abs() < TOLERANCE);
}

#[test]
fn a_purist_band_moves_less_and_a_syncretic_one_more() {
    let moved = |purist: f32| {
        let mut manager = pair();
        set_value(&mut manager, A, purist_axis(), purist);
        manager.apply_contact_drift(
            RATE,
            &weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]),
            &[full_tie(A, B)],
        );
        modifier(&manager, A, GAP_AXIS)
    };
    let neutral = moved(0.0);
    assert!(neutral > 0.0);
    assert!(
        moved(CULTURE_TRAIT_SPAN).abs() < TOLERANCE,
        "fully purist: receptiveness 0"
    );
    assert!(
        (moved(-CULTURE_TRAIT_SPAN) - 2.0 * neutral).abs() < TOLERANCE,
        "fully syncretic: receptiveness 2"
    );
}

#[test]
fn the_result_does_not_depend_on_the_order_bands_or_ties_are_visited() {
    let run = |band_order: &[BandId], tie_order: &[(BandId, BandId)]| {
        let mut manager = manager_with(band_order);
        set_value(&mut manager, A, GAP_AXIS, 0.3);
        set_value(&mut manager, B, GAP_AXIS, 1.1);
        set_value(&mut manager, C, GAP_AXIS, -0.7);
        set_value(&mut manager, B, purist_axis(), 0.9);
        let ties: Vec<ContactTie> = tie_order.iter().map(|(a, b)| full_tie(*a, *b)).collect();
        manager.apply_contact_drift(RATE, &weights(&[(A, 5.0), (B, 12.0), (C, 20.0)]), &ties);
        [A, B, C].map(|band| {
            (0..CULTURE_TRAIT_AXES)
                .map(|axis| modifier(&manager, band, axis).to_bits())
                .collect::<Vec<_>>()
        })
    };
    let forward = run(&[A, B, C], &[(A, B), (B, C), (A, C)]);
    let scrambled = run(&[C, A, B], &[(A, C), (A, B), (B, C)]);
    assert_eq!(forward, scrambled);
}

#[test]
fn a_rate_of_zero_reproduces_the_culture_with_no_drift() {
    let settle = |drift: bool| {
        let mut manager = pair();
        let tick = SimulationTick(1);
        for _ in 0..5 {
            if drift {
                manager.apply_contact_drift(
                    0.0,
                    &weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]),
                    &[full_tie(A, B)],
                );
            }
            manager.reconcile(
                &tick,
                &InfluencerCultureResonance::default(),
                &BTreeMap::new(),
            );
        }
        [A, B].map(|band| {
            let layer = manager
                .band_layer_by_owner(CultureOwner::from_band(band))
                .unwrap();
            (*layer.traits.values(), *layer.traits.modifier())
        })
    };
    assert_eq!(settle(true), settle(false));
}

#[test]
fn drift_writes_the_modifier_it_persists_and_it_survives_a_checkpoint() {
    let mut manager = pair();
    manager.apply_contact_drift(
        RATE,
        &weights(&[(A, EQUAL_WEIGHT), (B, EQUAL_WEIGHT)]),
        &[full_tie(A, B)],
    );
    let drifted = modifier(&manager, A, GAP_AXIS);
    assert!(drifted > 0.0);
    // A reconcile moves `value`, never the character offset.
    manager.reconcile(
        &SimulationTick(1),
        &InfluencerCultureResonance::default(),
        &BTreeMap::new(),
    );
    assert_eq!(modifier(&manager, A, GAP_AXIS), drifted);

    // The checkpoint is serde state (the save blob's codec); round-trip it through JSON.
    let restored = json_round_trip(&manager.checkpoint());
    let mut other = CultureManager::new();
    other.restore_checkpoint(&restored);
    assert_eq!(modifier(&other, A, GAP_AXIS), drifted);
    let owner = CultureOwner::from_band(A);
    assert_eq!(
        other.applied_contact_pull(owner),
        manager.applied_contact_pull(owner)
    );
    assert!(other.applied_contact_pull(owner).is_some());
}

#[test]
fn the_strongest_pull_names_its_source_and_its_axis_and_ties_go_to_the_lower_id() {
    let mut manager = manager_with(&[A, B, C]);
    // B is a small step away on axis 0; C a larger one on axis 2.
    set_value(&mut manager, B, 0, 0.2);
    set_value(&mut manager, C, 2, -1.0);
    manager.apply_contact_drift(
        RATE,
        &weights(&[(A, 10.0), (B, 10.0), (C, 10.0)]),
        &[full_tie(A, B), full_tie(A, C)],
    );
    let pull = manager
        .applied_contact_pull(CultureOwner::from_band(A))
        .copied()
        .unwrap();
    assert_eq!(pull.source, C.0);
    assert_eq!(pull.axis, 2);
    assert!(pull.delta.to_f32() < 0.0);

    // Equal pulls: the lower BandId wins.
    let mut tied = manager_with(&[A, B, C]);
    set_value(&mut tied, B, 0, 0.5);
    set_value(&mut tied, C, 0, 0.5);
    tied.apply_contact_drift(
        RATE,
        &weights(&[(A, 10.0), (B, 10.0), (C, 10.0)]),
        &[full_tie(A, C), full_tie(A, B)],
    );
    let pull = tied
        .applied_contact_pull(CultureOwner::from_band(A))
        .copied()
        .unwrap();
    assert_eq!(pull.source, B.0);

    // Identical bands pull nothing, and publish nothing.
    let mut same = manager_with(&[A, B]);
    same.apply_contact_drift(RATE, &weights(&[(A, 10.0), (B, 10.0)]), &[full_tie(A, B)]);
    assert!(same
        .applied_contact_pull(CultureOwner::from_band(A))
        .is_none());
}

#[test]
fn the_strongest_pull_is_on_the_encoded_snapshot() {
    let mut app = one_faction_world();
    let (home_entity, home_id) = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(e, b)| (e, *b))
        .min_by_key(|(_, b)| *b)
        .unwrap();
    let split = core_sim::split_band_from_parent(
        &mut app.world,
        home_entity,
        3,
        &core_sim::SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        },
    )
    .expect("the staged band can split");
    let sibling = split.band;
    app.world.run_system_once(reconcile_band_culture_layers);

    // A full mutual tie, and a gap between the two layers on axis 1.
    let mut ledger = ConnectionLedger::default();
    for key in [
        ConnectionKey::new(home_id, sibling),
        ConnectionKey::new(sibling, home_id),
    ] {
        ledger.insert_full_tie(key, &Sighting::new(UVec2::ZERO, 1, ""), 1);
    }
    app.world.insert_resource(ledger);
    let gap_axis = CultureTraitAxis::OpenClosed.index();
    {
        let mut manager = app.world.resource_mut::<CultureManager>();
        for (band, value) in [(home_id, -1.0), (sibling, 1.0)] {
            let layer = manager
                .band_layer_mut_by_owner(CultureOwner::from_band(band))
                .unwrap();
            layer.traits.update_value(gap_axis, scalar_from_f32(value));
        }
    }
    app.world.run_system_once(reconcile_culture_layers);

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec();
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows: Vec<_> = envelope
        .payload_as_snapshot()
        .unwrap()
        .population()
        .and_then(|section| section.populations())
        .unwrap()
        .iter()
        .collect();
    let home_row = rows.iter().find(|r| r.bandId() == home_id.0).unwrap();
    let sibling_row = rows.iter().find(|r| r.bandId() == sibling.0).unwrap();
    assert_eq!(home_row.cultureDriftSourceBand(), sibling.0);
    assert_eq!(sibling_row.cultureDriftSourceBand(), home_id.0);
    assert_eq!(home_row.cultureDriftAxis() as usize, gap_axis);
    assert!(
        home_row.cultureDriftDelta() > 0.0,
        "home drifts up toward +1"
    );
    assert!(sibling_row.cultureDriftDelta() < 0.0, "sibling drifts down");
}
