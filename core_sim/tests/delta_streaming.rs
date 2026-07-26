//! Guards for delta streaming (#386, `docs/plan_delta_streaming.md` §7).
//!
//! Every failure mode here is **silent**: a dropped delta looks like a quiet turn, a stale baseline
//! looks like a world where nothing happened, and a section missing from `WorldDelta` looks like a
//! field that simply did not change. Nothing in normal play surfaces any of them, which is why they
//! are pinned rather than left to review.

use core_sim::SnapshotHistory;
use sim_runtime::{
    CampaignProfileState, CultureLayerScope, CultureTensionKind, CultureTensionState, WorldDelta,
    WorldSnapshot,
};

/// Apply a delta to a snapshot the way the client's merge does: a section present on the delta
/// replaces its counterpart, an absent one is left alone.
///
/// Deliberately written against the payload structs rather than driving the real client, because
/// the property under test is a property of the WIRE — whether the delta stream carries enough to
/// reconstruct the world — and that has to hold no matter what the client does with it.
fn apply(base: &mut WorldSnapshot, delta: &WorldDelta) {
    if !delta.tiles.is_empty() || !delta.removed_tiles.is_empty() {
        base.tiles
            .retain(|t| !delta.removed_tiles.contains(&t.entity));
        for tile in &delta.tiles {
            match base.tiles.iter_mut().find(|t| t.entity == tile.entity) {
                Some(existing) => *existing = tile.clone(),
                None => base.tiles.push(tile.clone()),
            }
        }
    }
    if let Some(v) = delta.herds.as_ref() {
        base.herds = v.clone();
    }
    if let Some(v) = delta.forage_patches.as_ref() {
        base.forage_patches = v.clone();
    }
    if let Some(v) = delta.faction_inventory.as_ref() {
        base.faction_inventory = v.clone();
    }
    if let Some(v) = delta.demographics.as_ref() {
        base.demographics = v.clone();
    }
    if let Some(v) = delta.campaign_profiles.as_ref() {
        base.campaign_profiles = v.clone();
    }
    if let Some(v) = delta.intensification_knowledge.as_ref() {
        base.intensification_knowledge = v.clone();
    }
    if let Some(v) = delta.sedentarization.as_ref() {
        base.sedentarization = v.clone();
    }
    if let Some(v) = delta.visibility_raster.as_ref() {
        base.visibility_raster = v.clone();
    }
    if let Some(v) = delta.terrain.as_ref() {
        base.terrain = v.clone();
    }
    if let Some(v) = delta.culture_tensions.as_ref() {
        base.culture_tensions = v.clone();
    }
    if let Some(v) = delta.knowledge_timeline.as_ref() {
        base.knowledge_timeline = v.clone();
    }
    if !delta.populations.is_empty() || !delta.removed_populations.is_empty() {
        base.populations
            .retain(|p| !delta.removed_populations.contains(&p.entity));
        for pop in &delta.populations {
            match base.populations.iter_mut().find(|p| p.entity == pop.entity) {
                Some(existing) => *existing = pop.clone(),
                None => base.populations.push(pop.clone()),
            }
        }
    }
}

/// **The core property: a baseline plus N deltas equals the full snapshot at turn N.**
///
/// This is the test that catches a section which exists on `WorldSnapshot` but has no counterpart
/// on `WorldDelta` — the bug `campaign_profiles` actually was, where a field changed on the server
/// and could never reach a delta-fed client. Adding a snapshot field without its delta twin fails
/// here and nowhere else.
#[test]
fn a_baseline_plus_its_deltas_reconstructs_the_world() {
    let mut history = SnapshotHistory::with_capacity(64);

    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 0;
    history.update(world.clone());

    // The baseline the client would have applied.
    let mut reconstructed = history
        .last_snapshot
        .as_ref()
        .expect("baseline captured")
        .as_ref()
        .clone();

    for tick in 1..=5u64 {
        let mut next = reconstructed.clone();
        next.header.tick = tick;
        next.header.population_count = tick as u32;
        // MUTATE the section under test partway through, so the assertion below has teeth. With
        // `campaign_profiles` empty on both sides the comparison passes whether or not the delta
        // can carry it — which is exactly the vacuous test that would have missed the original bug.
        if tick == 3 {
            next.campaign_profiles = vec![CampaignProfileState {
                id: Some("delta-guard".to_string()),
                ..Default::default()
            }];
        }
        history.update(next);
        let delta = history
            .last_delta
            .as_ref()
            .expect("a delta per turn")
            .clone();
        apply(&mut reconstructed, &delta);
        reconstructed.header = delta.header.clone();
    }

    let authoritative = history.last_snapshot.as_ref().expect("latest").as_ref();
    assert_eq!(
        reconstructed.header.tick, authoritative.header.tick,
        "the reconstructed world must be at the same tick"
    );
    assert!(
        !authoritative.campaign_profiles.is_empty(),
        "the run must actually change campaign_profiles, or the next assertion proves nothing"
    );
    assert_eq!(
        reconstructed.campaign_profiles, authoritative.campaign_profiles,
        "campaign_profiles must survive delta reconstruction — it had no WorldDelta field at all \
         until #386, and a field the delta cannot carry is permanently stale on the client"
    );
    assert_eq!(
        reconstructed.tiles.len(),
        authoritative.tiles.len(),
        "tile set must match"
    );
}

/// **Every published frame takes the next sequence number, and a delta names the one before it.**
///
/// The client applies a delta only when its `base_frame_seq` matches what it last applied, so a
/// publication that forgets to claim a number strands every later delta.
#[test]
fn each_publication_claims_the_next_sequence_and_names_its_base() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };

    let mut expected_base = 0u64;
    for tick in 0..4u64 {
        world.header.tick = tick;
        history.update(world.clone());
        let delta = history.last_delta.as_ref().expect("delta").clone();
        assert_eq!(
            delta.header.base_frame_seq, expected_base,
            "a delta must name the frame it applies to"
        );
        assert_eq!(
            delta.header.frame_seq,
            expected_base + 1,
            "sequence numbers are consecutive across publications"
        );
        expected_base = delta.header.frame_seq;
    }
}

/// **A recapture is a publication too.** It claims a sequence number like any other frame — a
/// mid-tick recapture that reused the turn's number would leave the client's `base_frame_seq`
/// check comparing against a frame it never saw, rejecting every subsequent turn delta.
#[test]
fn a_recapture_advances_the_sequence_without_pushing_a_ring_entry() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    let after_turn = history.len();
    let turn_seq = history
        .last_delta
        .as_ref()
        .expect("turn delta")
        .header
        .frame_seq;

    // A command mutated the world mid-tick.
    world.header.population_count = 42;
    history.refresh_latest(world.clone());

    let recapture_seq = history
        .last_delta
        .as_ref()
        .expect("recapture delta")
        .header
        .frame_seq;
    assert_eq!(
        recapture_seq,
        turn_seq + 1,
        "a recapture claims the next sequence number"
    );
    assert_eq!(
        history.len(),
        after_turn,
        "the rollback ring stays one entry per TICK — a recapture re-baselines the current entry"
    );
}

/// **Intra-turn recaptures are cumulative, so missing one is harmless.**
///
/// `refresh_latest` does not commit the baseline, so each recapture delta is
/// `baseline(last turn) → now` rather than `previous recapture → now`. That is what makes the
/// second one a superset of the first, and it is the property the client's "apply every frame in
/// order" relies on to stay correct when a frame is coalesced away.
#[test]
fn a_later_recapture_delta_supersedes_an_earlier_one() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    world.header.population_count = 7;
    history.refresh_latest(world.clone());
    let first = history.last_delta.as_ref().expect("first").clone();

    world.header.power_count = 9;
    history.refresh_latest(world.clone());
    let second = history.last_delta.as_ref().expect("second").clone();

    assert_eq!(
        first.header.population_count, 7,
        "the first recapture carries the first command's change"
    );
    assert_eq!(
        second.header.population_count, 7,
        "the second still carries it — it diffs from the TURN baseline, not from the first \
         recapture, which is what makes dropping the first one safe"
    );
    assert_eq!(
        second.header.power_count, 9,
        "…and the second command's change as well"
    );
}

/// A tension that stays put, so the "unchanged" and "emptied" cases differ only in the delta.
fn guard_tension() -> CultureTensionState {
    CultureTensionState {
        layer_id: 1,
        scope: CultureLayerScope::Global,
        owner: 1,
        severity: 1,
        timer: 1,
        kind: CultureTensionKind::DriftWarning,
    }
}

/// **"Unchanged" and "now empty" must not be the same bytes.**
///
/// `culture_tensions` is a whole-section field with no `removed_culture_tensions` counterpart, so
/// an empty `Vec` cannot say which of the two happened. While it was a bare `Vec`, the capture path
/// encoded "unchanged" as `Vec::new()` and the receiver had to guess: read it as a replacement and
/// every delta blanked the client's tension list, read it as unchanged and a genuinely-resolved
/// last tension stayed on screen until the next full snapshot. `Option` is what makes the two
/// distinguishable — `None` unchanged, `Some(vec![])` emptied.
#[test]
fn an_unchanged_tension_list_is_absent_and_an_emptied_one_is_present_but_empty() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        culture_tensions: vec![guard_tension()],
        ..Default::default()
    };
    world.header.tick = 0;
    history.update(world.clone());

    // A turn where nothing about tensions moved.
    world.header.tick = 1;
    world.header.population_count = 1;
    history.update(world.clone());
    assert_eq!(
        history
            .last_delta
            .as_ref()
            .expect("a delta per turn")
            .culture_tensions,
        None,
        "an unchanged tension roster must be ABSENT from the delta, so the client leaves its own \
         list alone"
    );

    // The last tension resolves.
    world.header.tick = 2;
    world.culture_tensions.clear();
    history.update(world.clone());
    assert_eq!(
        history
            .last_delta
            .as_ref()
            .expect("a delta per turn")
            .culture_tensions,
        Some(Vec::new()),
        "an emptied tension roster must be PRESENT and empty — the client cannot clear a list it \
         was never told about"
    );

    // And the merge the client performs actually drops it.
    let mut reconstructed = WorldSnapshot {
        culture_tensions: vec![guard_tension()],
        ..Default::default()
    };
    apply(
        &mut reconstructed,
        history.last_delta.as_ref().expect("a delta per turn"),
    );
    assert!(
        reconstructed.culture_tensions.is_empty(),
        "applying the emptied-roster delta must clear the client's tensions"
    );
}
