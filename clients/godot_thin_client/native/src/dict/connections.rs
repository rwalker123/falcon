//! `connections` section -- the directed, decaying ties contact leaves behind (arc #527, #538).
//!
//! One row per edge, already filtered SIM-SIDE to the viewer's faction: the observer band is one of
//! yours, the subject may belong to anyone. There is deliberately no faction column and no
//! same-faction branch here — faction is a property of the endpoint
//! (`.claude/rules/core_sim/connections.md`), and a decoder that invented one would be the first
//! place the arc's discipline broke.
//!
//! **The remembered position is CLOCK 1, not a live one.** `last_seen_{x,y}` is where the subject
//! stood the last time the observer actually saw them, and `last_seen_turn` says when. A consumer
//! that renders it as a current position is claiming a sighting the tie never granted — a connection
//! can only ever grant `Discovered`.
//!
//! **Strength `0` is a PARKED tie, not an absent one** ("we know such a people exist and have no
//! current dealings"), so the row is published and must be rendered, disabled, rather than dropped.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

pub(crate) fn connections_to_array(
    list: Vector<'_, ForwardsUOffset<fb::ConnectionState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for connection in list {
        let mut dict = VarDictionary::new();
        // The two endpoints, as the durable `BandId`s every command addresses a band by — the same
        // `band_id` a cohort carries, so the join is an integer comparison and never an entity one.
        let _ = dict.insert("observer_band_id", connection.observerBandId() as i64);
        let _ = dict.insert("subject_band_id", connection.subjectBandId() as i64);
        let _ = dict.insert("strength", f64::from(connection.strength()));
        let _ = dict.insert("last_seen_x", connection.lastSeenX() as i64);
        let _ = dict.insert("last_seen_y", connection.lastSeenY() as i64);
        let _ = dict.insert("last_seen_turn", connection.lastSeenTurn() as i64);
        let _ = dict.insert("last_contact_turn", connection.lastContactTurn() as i64);
        let _ = dict.insert("first_contact_turn", connection.firstContactTurn() as i64);
        array.push(&dict.to_variant());
    }
    array
}
