//! Guards for delta streaming (#386, `docs/plan_delta_streaming.md` §7).
//!
//! Every failure mode here is **silent**: a dropped delta looks like a quiet turn, a stale baseline
//! looks like a world where nothing happened, and a section missing from `WorldDelta` looks like a
//! field that simply did not change. Nothing in normal play surfaces any of them, which is why they
//! are pinned rather than left to review.

use core_sim::{Scalar, SnapshotHistory};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_runtime::{
    CampaignProfileState, CommandEventState, CultureLayerScope, CultureTensionKind,
    CultureTensionState, ScalarRasterState, WorldDelta, WorldSnapshot,
};

/// One retained feed row, at the sequence the log would have stamped on it (**one-based** — a
/// fresh client cursor is `0`, so a zeroth event could never be delivered).
fn command_event(seq: u64, tick: u64) -> CommandEventState {
    CommandEventState {
        tick,
        kind: "born".to_string(),
        faction: 0,
        label: format!("event {seq}"),
        detail: Some(format!("count=1 seq={seq}")),
        seq,
    }
}

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
    // **APPEND, not replace.** `command_events` is the one section whose delta carries the rows
    // appended since the client's cursor rather than the whole section (`snapshot::diff_appended`),
    // so a client that overwrote here would hold only the newest turn's events and lose the ring.
    if let Some(v) = delta.command_events.as_ref() {
        base.command_events.extend(v.iter().cloned());
    }
    if let Some(v) = delta.command_events_retention_turns {
        base.command_events_retention_turns = v;
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
        .last_snapshot()
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
        // The event log grows on two separate turns, so the reconstruction has to have ACCUMULATED
        // rather than merely taken the last delta's rows.
        if tick == 2 || tick == 4 {
            let seq = next.command_events.len() as u64 + 1;
            next.command_events.push(command_event(seq, tick));
        }
        history.update(next);
        let delta = history.last_delta().expect("a delta per turn").clone();
        apply(&mut reconstructed, &delta);
        reconstructed.header = delta.header.clone();
    }

    let latest = history.last_snapshot().expect("latest");
    let authoritative = latest.as_ref();
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
    assert_eq!(
        authoritative.command_events.len(),
        2,
        "the run must actually append events, or the next assertion proves nothing"
    );
    assert_eq!(
        reconstructed.command_events, authoritative.command_events,
        "command_events must survive delta reconstruction — its delta is APPEND-only, so a client \
         that replaced the section (or a diff that shipped the whole ring) shows up here"
    );
}

/// **A recapture's delta is cumulative, so losing one loses nothing** — the property that makes
/// `Baseline::Hold` load-bearing for an append-only section.
///
/// `refresh_latest` does not commit the baseline, so each recapture diffs from the *turn's* cursor.
/// Advancing that cursor on a recapture would consume the rows: the recapture frame would carry
/// them, and the next real turn delta — which diffs from the turn baseline — would then never send
/// them at all. Silent, and unrecoverable without a resync.
#[test]
fn a_recapture_delta_carries_every_event_since_the_turn_baseline() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    // Two world-mutating commands land inside the same tick, each triggering a recapture.
    world.command_events.push(command_event(1, 1));
    history.refresh_latest(world.clone());
    let first = history.last_delta().expect("first recapture").clone();

    world.command_events.push(command_event(2, 1));
    history.refresh_latest(world.clone());
    let second = history.last_delta().expect("second recapture").clone();

    let seqs = |delta: &WorldDelta| -> Vec<u64> {
        delta
            .command_events
            .as_ref()
            .map(|rows| rows.iter().map(|row| row.seq).collect())
            .unwrap_or_default()
    };
    assert_eq!(seqs(&first), vec![1], "the first command's event");
    assert_eq!(
        seqs(&second),
        vec![1, 2],
        "the second recapture still carries the first command's event — it diffs from the TURN \
         cursor, not from the first recapture, which is what makes dropping the first one safe"
    );

    // And the turn AFTER the recaptures still owes both, because no recapture advanced the cursor.
    world.header.tick = 2;
    world.command_events.push(command_event(3, 2));
    history.update(world.clone());
    let turn = history.last_delta().expect("turn delta").clone();
    assert_eq!(
        seqs(&turn),
        vec![1, 2, 3],
        "a recapture must not consume rows the committed turn delta is responsible for"
    );
}

/// **A gap in the delta chain must not silently lose events.**
///
/// An append-only delta is not self-healing: the rows it carried are gone if it is dropped, where
/// the old whole-vector resend would have re-stated them on the next turn. It is safe for exactly
/// one reason — the client applies a delta only when it holds the named base frame
/// (`WorldCache::accepts`), and a mismatch raises `resync_needed`, whose answer is a **full
/// snapshot** carrying the entire retained ring. This pins both halves: the gap is detectable, and
/// the resync answer really does re-backfill.
#[test]
fn a_dropped_delta_is_detectable_and_the_resync_answer_re_backfills_every_event() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 0;
    history.update(world.clone());

    let mut client = history
        .last_snapshot()
        .expect("baseline captured")
        .as_ref()
        .clone();
    let mut client_frame = client.header.frame_seq;

    let mut deltas = Vec::new();
    for tick in 1..=3u64 {
        world.header.tick = tick;
        let seq = world.command_events.len() as u64 + 1;
        world.command_events.push(command_event(seq, tick));
        history.update(world.clone());
        deltas.push(history.last_delta().expect("a delta per turn").clone());
    }

    // The middle frame never arrives.
    for (index, delta) in deltas.iter().enumerate() {
        let dropped = index == 1;
        let applicable = delta.header.base_frame_seq == client_frame;
        if dropped {
            continue;
        }
        if !applicable {
            // The client's gate: it CANNOT merge this, so it must ask for a resync instead of
            // applying it against the wrong baseline.
            assert_eq!(
                index, 2,
                "only the frame after the gap should be unapplicable"
            );
            continue;
        }
        apply(&mut client, delta);
        client_frame = delta.header.frame_seq;
    }

    assert_eq!(
        client.command_events.len(),
        1,
        "the gap really did cost the client events — an append-only delta is not self-healing, \
         which is why the base-frame gate is load-bearing rather than belt-and-braces"
    );

    // The resync answer: a full snapshot, carrying the whole retained ring.
    let authoritative = history.last_snapshot().expect("latest").as_ref().clone();
    assert_eq!(
        authoritative.command_events.len(),
        3,
        "the sim still holds every event inside its retention window"
    );
    assert_eq!(
        authoritative
            .command_events
            .iter()
            .map(|row| row.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "so a resynced client is whole again — nothing was lost, only undelivered"
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
        let delta = history.last_delta().expect("delta").clone();
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
    let turn_seq = history.last_delta().expect("turn delta").header.frame_seq;

    // A command mutated the world mid-tick.
    world.header.population_count = 42;
    history.refresh_latest(world.clone());

    let recapture_seq = history
        .last_delta()
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
    let first = history.last_delta().expect("first").clone();

    world.header.power_count = 9;
    history.refresh_latest(world.clone());
    let second = history.last_delta().expect("second").clone();

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

/// `SnapshotHeader.frameSeq` as the CLIENT reads it — decoded from the published bytes, because the
/// number that matters is the one on the wire, not the one on the struct we happen to hold.
fn frame_seq_on_the_wire(bytes: &[u8]) -> u64 {
    let envelope = fb::root_as_envelope(bytes).expect("the rollback frame is a valid envelope");
    assert_eq!(
        envelope.payload_type(),
        fb::SnapshotPayload::snapshot,
        "a rollback re-baselines the client, so it must publish a FULL snapshot"
    );
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .header()
        .expect("every snapshot carries a header")
        .frameSeq()
}

/// **A rollback frame claims a FRESH sequence number, so the next delta still applies.**
///
/// The publication counter is never rewound — it numbers publications, not ticks. So broadcasting a
/// ring entry with the number stamped when that tick was *originally* published leaves the client
/// baselined at an old number while the server's next delta names the current one as its base:
/// `WorldCache::accepts` rejects it and the client burns a `resync` round trip recovering. Silent —
/// it self-heals — which is why it is pinned.
#[test]
fn a_rollback_frame_is_the_base_the_next_delta_names() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    for tick in 0..3u64 {
        world.header.tick = tick;
        world.header.population_count = tick as u32;
        history.update(world.clone());
    }

    let entry = history.entry(1).expect("a ring entry per tick");
    let stamped_when_originally_published = entry.snapshot.header.frame_seq;
    history.reset_to_entry(&entry);
    let broadcast = history.publish_full_frame(&entry);
    let broadcast_seq = frame_seq_on_the_wire(&broadcast);

    assert!(
        broadcast_seq > stamped_when_originally_published,
        "the rollback frame must claim a fresh publication number ({broadcast_seq}), not reuse the \
         one the ring entry was originally published under \
         ({stamped_when_originally_published}) — the counter is monotonic per world and is never \
         rewound"
    );

    // The next turn after the rollback.
    world.header.tick = 2;
    world.header.population_count = 99;
    history.update(world.clone());
    let delta = history.last_delta().expect("a delta per turn");
    assert_eq!(
        delta.header.base_frame_seq, broadcast_seq,
        "the next delta must name the rollback frame as its base — the client applied that frame, \
         so any other base_frame_seq makes it drop this delta and ask for a resync"
    );
}

/// **A resync answer claims a live sequence number too — and this one cannot self-heal.**
///
/// Resync is the *recovery* path: the client asks for it precisely because it could not apply a
/// delta. If the answer carries a stale number the client baselines behind the server, the next
/// delta is rejected, and it resyncs again — the mechanism meant to close a sequence gap opens one.
///
/// The window pinned here is a **mid-tick recapture**, which fires on every world-mutating command.
/// A recapture claims a sequence number and refreshes `history.back().snapshot`, but **not** that
/// entry's cached `encoded_snapshot_flat` — so the world's first ring entry still holds bytes
/// stamped with the pre-recapture number, and republishing them as stored is exactly the bug. (An
/// auxiliary delta — `update_axis_bias` and friends — opens the same window without touching the
/// ring at all.)
#[test]
fn a_resync_frame_is_the_base_the_next_delta_names_after_a_recapture() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };
    world.header.tick = 0;
    // The world's first publication — the only one whose ring entry caches encoded flat bytes.
    history.update(world.clone());

    // A world-mutating command lands mid-tick: a publication, but not a new ring entry.
    world.header.population_count = 42;
    history.refresh_latest(world.clone());

    let entry = history.latest_entry().expect("a world has been published");
    let stored_on_the_entry = entry.snapshot.header.frame_seq;
    let resync = history.publish_full_frame(&entry);
    let resync_seq = frame_seq_on_the_wire(&resync);

    assert!(
        resync_seq > stored_on_the_entry,
        "a resync answer must claim a LIVE publication number ({resync_seq}), never republish one \
         stored on the ring entry ({stored_on_the_entry}) — the client baselines on this frame"
    );

    // The next turn after the resync.
    world.header.tick = 1;
    world.header.population_count = 99;
    history.update(world.clone());
    let delta = history.last_delta().expect("a delta per turn");
    assert_eq!(
        delta.header.base_frame_seq, resync_seq,
        "the next delta must name the resync frame as its base — otherwise the client drops it and \
         resyncs again, and the recovery path never converges"
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
            .last_delta()
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
            .last_delta()
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
        &history.last_delta().expect("a delta per turn"),
    );
    assert!(
        reconstructed.culture_tensions.is_empty(),
        "applying the emptied-roster delta must clear the client's tensions"
    );
}

/// **A turn in which nothing moved must produce a delta that carries nothing.**
///
/// This is the observable half of the `O(changed)` property the publication path now rests on: the
/// diff walks the baselines without touching the entries that did not change, so a still world is
/// cheap *and* silent. The two go together — a delta that carried an unchanged section would mean
/// the diff had rewritten a baseline it should have left alone.
///
/// The guard needs teeth, so the world here is not empty: it carries tiles, a tension roster and a
/// campaign profile, all of which are re-*captured* every turn and must still compare out.
#[test]
fn a_turn_that_changes_nothing_publishes_a_delta_that_carries_nothing() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        culture_tensions: vec![guard_tension()],
        campaign_profiles: vec![CampaignProfileState {
            id: Some("still-world".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    world.header.tick = 0;
    history.update(world.clone());

    // The world's first publication is a baseline and legitimately carries everything; the turn
    // after it is the steady state under test.
    world.header.tick = 1;
    history.update(world.clone());

    let delta = history.last_delta().expect("a delta per turn");
    assert!(
        delta.tiles.is_empty() && delta.removed_tiles.is_empty(),
        "an unchanged tile set must not be re-sent"
    );
    assert!(
        delta.culture_layers.is_empty() && delta.power.is_empty() && delta.populations.is_empty(),
        "no indexed collection may re-send an unchanged entry"
    );
    assert_eq!(delta.culture_tensions, None, "unchanged section: tensions");
    assert_eq!(
        delta.campaign_profiles, None,
        "unchanged section: campaign profiles"
    );
    assert_eq!(delta.terrain, None, "unchanged section: terrain overlay");
    assert_eq!(delta.victory, None, "unchanged section: victory");
    assert_eq!(delta.herds, None, "unchanged section: herds");
}

/// A map small enough to write out, and big enough that "every cell" is a claim rather than a
/// single value.
const GUARD_RASTER_WIDTH: u32 = 2;
const GUARD_RASTER_HEIGHT: u32 = 2;

/// The two visibility samples `visibility_raster_from_ledger` publishes at the extremes: fog off
/// fills the whole raster with `Scalar::SCALE` (Active), an unseen cell is `0` (Unexplored).
const ACTIVE: i64 = Scalar::SCALE;
const UNEXPLORED: i64 = 0;

fn visibility_raster(sample: i64) -> ScalarRasterState {
    ScalarRasterState {
        width: GUARD_RASTER_WIDTH,
        height: GUARD_RASTER_HEIGHT,
        samples: vec![sample; (GUARD_RASTER_WIDTH * GUARD_RASTER_HEIGHT) as usize],
    }
}

/// **A section a command moved and a later command in the SAME TICK moved back is restated.**
///
/// The reported instance is `set_fog off` then `set_fog on` with no turn between. The fog-off
/// recapture publishes the all-Active raster; the fog-on recapture finds the fogged raster equal to
/// the *turn* baseline and — before the `held` flag — published nothing, leaving the client
/// rendering a fully-revealed map while `fogEnabled` on the very same delta said fog was on.
///
/// It does not self-heal: the visibility raster is byte-identical turn over turn whenever nothing
/// moves, so no later turn ever finds it changed. That is the property that makes this class of bug
/// permanent rather than one-turn, and it is why the guard is on this section.
#[test]
fn a_visibility_raster_toggled_off_and_back_within_a_tick_is_restated() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        visibility_raster: visibility_raster(UNEXPLORED),
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    // `set_fog off`: a mid-tick recapture reveals the whole map.
    world.fog_enabled = false;
    world.visibility_raster = visibility_raster(ACTIVE);
    history.refresh_latest(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per recapture")
            .visibility_raster,
        Some(visibility_raster(ACTIVE)),
        "the fog-off recapture must carry the revealed raster — this half always worked"
    );

    // `set_fog on`, still with no turn between: back to exactly the turn baseline.
    world.fog_enabled = true;
    world.visibility_raster = visibility_raster(UNEXPLORED);
    history.refresh_latest(world.clone());
    let delta = history.last_delta().expect("a delta per recapture");
    assert_eq!(
        delta.visibility_raster,
        Some(visibility_raster(UNEXPLORED)),
        "the fog-on recapture must RESTATE the fogged raster: it equals the turn baseline, but the \
         client is holding the revealed one the previous recapture published"
    );
    assert!(
        delta.fog_enabled,
        "…and the flag that rides every delta must agree with the raster beside it"
    );
}

/// The same property on a section that has nothing to do with fog, so the guard reads as the
/// general rule it is: any whole section published on a held frame is restated when it comes back.
#[test]
fn a_tension_roster_changed_and_reverted_within_a_tick_is_restated() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        culture_tensions: vec![guard_tension()],
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    world.culture_tensions.clear();
    history.refresh_latest(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per recapture")
            .culture_tensions,
        Some(Vec::new()),
        "the first recapture carries the emptied roster"
    );

    world.culture_tensions = vec![guard_tension()];
    history.refresh_latest(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per recapture")
            .culture_tensions,
        Some(vec![guard_tension()]),
        "the second must restate the roster the client was told to clear, even though it now equals \
         the turn baseline again"
    );
}

/// **A revert that straddles a turn boundary is restated by the TURN.**
///
/// The flag outlives the held frames, so the `Baseline::Advance` arm has to consult it too:
/// otherwise a command that changes a section and a turn that puts it back leaves the client on the
/// command's intermediate value with the baseline agreeing it is correct.
#[test]
fn a_turn_restates_a_section_a_recapture_moved_and_gave_back() {
    let mut history = SnapshotHistory::with_capacity(64);
    let mut world = WorldSnapshot {
        fog_enabled: true,
        campaign_profiles: vec![CampaignProfileState {
            id: Some("straddle".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    world.header.tick = 1;
    history.update(world.clone());

    let baseline_profiles = world.campaign_profiles.clone();
    world.campaign_profiles = vec![CampaignProfileState {
        id: Some("mid-tick".to_string()),
        ..Default::default()
    }];
    history.refresh_latest(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per recapture")
            .campaign_profiles,
        Some(world.campaign_profiles.clone()),
        "the recapture carries the command's change"
    );

    // The turn resolves with the roster back where the last turn left it.
    world.header.tick = 2;
    world.campaign_profiles = baseline_profiles.clone();
    history.update(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per turn")
            .campaign_profiles,
        Some(baseline_profiles),
        "the turn must restate the roster — the client is holding the recapture's value"
    );

    // …and the turn after that is quiet again, because the restatement cleared the flag.
    world.header.tick = 3;
    history.update(world.clone());
    assert_eq!(
        history
            .last_delta()
            .expect("a delta per turn")
            .campaign_profiles,
        None,
        "a restatement clears the flag, so the steady turn after it carries nothing"
    );
}
