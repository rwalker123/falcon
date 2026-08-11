//! **The connection primitive** — a directed, persisting, decaying tie between two bands.
//!
//! Contact is an event: a group is standing in a tile you can see. There is no radius test, no roll
//! and no config lever on the *finding* — if it is in your range, you have found it. What contact
//! leaves behind is a **connection**, and that is what this module owns
//! (`docs/plan_contact_and_logistics.md` §Q2, §Q6).
//!
//! # A connection is a RAW primitive
//!
//! It knows nothing about goods, culture or knowledge. It is not a trade agreement and not a
//! treaty. It says only: *these two groups know each other, this much, right now.* Everything else
//! builds on top of it and defines its own use of it, the same way `LocalStore` knows nothing about
//! food.
//!
//! That discipline is the whole point of the slice. The moment a connection carries a `tariff` or
//! an `openness_to_ideas` it has stopped being a primitive and has started being one rider's
//! opinion wearing the primitive's clothes — which is precisely how the retired `TradeLink` ended
//! up carrying `from_faction` / `to_faction` / `tariff` / `leak_timer` on what should have been an
//! edge. **No rider vocabulary in any field or method name here.**
//!
//! # Faction is a property of the ENDPOINT
//!
//! Both endpoints are a [`BandId`]. There is deliberately no faction field on the edge and no
//! same-faction / cross-faction branch anywhere in this module: the arrival of a second faction
//! must change almost nothing, and that is only true if the code never asks.
//!
//! # THE KEYSTONE — a connection can only ever grant `Discovered`
//!
//! > **Only presence makes a tile `Seen` ([`crate::visibility::VisibilityState::Active`]). A
//! > connection can only ever grant `Discovered`.**
//!
//! Meet a band, exchange maps, and their land becomes `Discovered` — frozen at the moment they told
//! you. To *watch* anything you need presence there, and presence has to be maintained. A
//! remembered band therefore behaves exactly like a remembered herd: you know where they *were*.
//!
//! Nothing in this slice grants visibility from a connection, so there is nothing here to enforce
//! it against — but every rider that lands on top of this (logistics, culture, knowledge) will be
//! tempted to break it, so the rule is stated here, where a rider author has to read it, and
//! asserted in `core_sim/tests/connections.rs`.
//!
//! # The three clocks
//!
//! | What decays | Speed | What it means |
//! |---|---|---|
//! | [`Connection::last_seen_position`] | immediately on losing sight | you know where they *were*. Same as a herd. |
//! | [`Connection::strength`] | over turns without contact, down to zero | the currency of what you know. **At zero nothing flows.** |
//! | the edge itself | very slowly, but not never | eventually you have simply forgotten there was such a people |
//!
//! Zero **parks** the edge; it does not delete it. A parked edge means *"we know such a people
//! exist and have no current tie"*, which is what keeps the third clock a genuinely separate lever
//! rather than a duplicate of the second.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{
    components::BandId,
    connections_config::{ConnectionsConfig, ConnectionsConfigHandle},
    metrics::SimulationMetrics,
    resources::SimulationTick,
    scalar::Scalar,
};

/// **A full tie.** Strength is a `0..=1` fraction by definition, so its ceiling is what the number
/// *means* rather than how fast it moves — a named constant, not a config lever. Tuning it would
/// change the unit, and every rider that reads "at zero nothing flows" reads this as the other end.
pub const FULL_TIE: Scalar = Scalar(Scalar::SCALE);

/// **No tie left.** The edge is still there — see the module docs on parking — but nothing flows.
pub const NO_TIE: Scalar = Scalar(0);

/// A directed tie: `observer` knows `subject`. The reverse edge is a separate entry.
///
/// The direction is *who found whom*, which is what lets influence be asymmetric without a second
/// mechanic: the scout on the ridge watching a settlement that has no idea anyone is there. A
/// mutual relationship is simply both edges existing, and whether a given rider *requires* both is
/// the rider's business, not the connection's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionKey {
    pub observer: BandId,
    pub subject: BandId,
}

impl ConnectionKey {
    pub fn new(observer: BandId, subject: BandId) -> Self {
        Self { observer, subject }
    }
}

/// One directed tie and its three clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    /// **Clock 2.** `0..=1`, as a fixed-point [`Scalar`] rather than an `f32` — this is
    /// checkpointed state, and a float would put replay determinism at risk. Raised on contact,
    /// drained without it. At zero nothing flows.
    pub strength: Scalar,
    /// **Clock 1.** Where they were the last time we actually saw them…
    pub last_seen_position: UVec2,
    /// …and on which turn. Neither is touched by a turn without contact: that is the whole of
    /// "you know where they *were*".
    pub last_seen_turn: u64,
    /// The turn the tie was last refreshed by a contact event. Drives clocks 2 and 3.
    pub last_contact_turn: u64,
    /// The turn the tie first formed. Never changes.
    pub first_contact_turn: u64,
}

/// Every directed tie in the world, keyed by its endpoints.
///
/// **`BTreeMap`, not `HashMap`** — the iteration order is observed by the snapshot and by the
/// checkpoint, so it has to be an order and not an accident.
#[derive(Resource, Default, Debug, Clone)]
pub struct ConnectionLedger {
    edges: BTreeMap<ConnectionKey, Connection>,
}

impl ConnectionLedger {
    pub fn get(&self, key: &ConnectionKey) -> Option<&Connection> {
        self.edges.get(key)
    }

    /// Every edge, in key order. Deterministic by construction — see the type's note on `BTreeMap`.
    pub fn iter(&self) -> impl Iterator<Item = (&ConnectionKey, &Connection)> {
        self.edges.iter()
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Refresh (or form) the tie `key` from a contact at `position` on `turn`.
    ///
    /// Returns `true` when the edge did not exist and was formed here — which is what the "edges
    /// formed this turn" counter counts, and what a future first-meeting beat would hang off.
    pub fn record_contact(
        &mut self,
        key: ConnectionKey,
        position: UVec2,
        turn: u64,
        cfg: &ConnectionsConfig,
    ) -> bool {
        let gain = Scalar::from_f32(cfg.strength.gain_per_contact);
        match self.edges.get_mut(&key) {
            Some(connection) => {
                connection.strength = (connection.strength + gain).min(FULL_TIE);
                connection.last_seen_position = position;
                connection.last_seen_turn = turn;
                connection.last_contact_turn = turn;
                false
            }
            None => {
                self.edges.insert(
                    key,
                    Connection {
                        strength: gain.min(FULL_TIE),
                        last_seen_position: position,
                        last_seen_turn: turn,
                        last_contact_turn: turn,
                        first_contact_turn: turn,
                    },
                );
                true
            }
        }
    }

    /// Run clocks 2 and 3 over every edge that saw no contact on `turn`, and return how many edges
    /// clock 3 reaped.
    ///
    /// "Saw contact this turn" is read off `last_contact_turn`, which [`Self::record_contact`] has
    /// already stamped — so this needs no second copy of the turn's contact set, and cannot
    /// disagree with one.
    ///
    /// `last_seen_position` / `last_seen_turn` are deliberately **not** touched: clock 1 is exactly
    /// the memory of where they were.
    pub fn decay_all(&mut self, turn: u64, cfg: &ConnectionsConfig) -> usize {
        let drain = Scalar::from_f32(cfg.strength.decay_per_turn);
        let before = self.edges.len();
        self.edges.retain(|_, connection| {
            if connection.last_contact_turn != turn {
                connection.strength = (connection.strength - drain).max(NO_TIE);
            }
            // Clock 3 — the fact of them. Measured from the last CONTACT, not from the strength:
            // a parked edge is still a memory, and only time erases it.
            turn.saturating_sub(connection.last_contact_turn) < cfg.forget_turns
        });
        before - self.edges.len()
    }
}

/// **Every contact found this turn** — derived, cleared and rebuilt every turn, never checkpointed.
///
/// Written by two producers and read by one: [`crate::visibility_systems::calculate_visibility`]
/// fills it from inside the sight sweep it already runs (so contact can never disagree with what a
/// band can actually see), `advance_expeditions` drains a returning party's report into it, and
/// [`advance_connections`] consumes and clears it.
///
/// A [`BTreeMap`] keyed by the edge rather than a set of triples, for two reasons: the key space is
/// the ledger's own, and a subject stands in exactly one place, so the position is a value and not
/// part of the identity. The `u64` beside it is **the turn the position was observed**, which is
/// not always this turn — an expedition's report is what the party saw on the march. When two
/// reports name the same edge, the **fresher observation wins**.
#[derive(Resource, Default, Debug, Clone)]
pub struct ContactsThisTurn(BTreeMap<ConnectionKey, (UVec2, u64)>);

impl ContactsThisTurn {
    /// Record `observer` finding `subject` at `position`, observed on `observed_turn`. A staler
    /// observation of an edge already recorded this turn is dropped rather than overwriting it.
    pub fn record(
        &mut self,
        observer: BandId,
        subject: BandId,
        position: UVec2,
        observed_turn: u64,
    ) {
        let entry = self
            .0
            .entry(ConnectionKey::new(observer, subject))
            .or_insert((position, observed_turn));
        if observed_turn >= entry.1 {
            *entry = (position, observed_turn);
        }
    }

    /// Every contact, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ConnectionKey, &(UVec2, u64))> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// Advance the three clocks: raise every tie contacted this turn, drain every tie that was not, and
/// reap the ones nobody has seen in `forget_turns`.
///
/// Chained after `calculate_visibility` (which is what *finds* the contacts) and before
/// `apply_visibility_decay`, inside the serial `TurnStage::Visibility` chain. It consumes
/// [`ContactsThisTurn`] and clears it, so the set is rebuilt from scratch every turn.
pub fn advance_connections(
    mut ledger: ResMut<ConnectionLedger>,
    mut contacts: ResMut<ContactsThisTurn>,
    config: Res<ConnectionsConfigHandle>,
    tick: Res<SimulationTick>,
    mut metrics: ResMut<SimulationMetrics>,
) {
    let cfg = config.get();
    let turn = tick.0;

    let mut formed = 0u32;
    for (key, (position, _observed_turn)) in contacts.iter() {
        if ledger.record_contact(*key, *position, turn, &cfg) {
            formed += 1;
        }
    }
    let reaped = ledger.decay_all(turn, &cfg);
    contacts.clear();

    metrics.connections_live = ledger.len() as u32;
    metrics.connections_formed = formed;
    metrics.connections_reaped = reaped as u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVER: BandId = BandId(1);
    const SUBJECT: BandId = BandId(2);
    const SOMEWHERE: UVec2 = UVec2::new(4, 7);
    const ELSEWHERE: UVec2 = UVec2::new(9, 1);

    fn edge() -> ConnectionKey {
        ConnectionKey::new(OBSERVER, SUBJECT)
    }

    fn config() -> ConnectionsConfig {
        ConnectionsConfig::default()
    }

    #[test]
    fn a_tie_climbs_on_consecutive_contact_and_stops_at_a_full_tie() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        let mut previous = NO_TIE;
        // Four turns at the shipped gain is exactly a full tie; a fifth must not overshoot.
        for turn in 0..5 {
            ledger.record_contact(edge(), SOMEWHERE, turn, &cfg);
            let strength = ledger.get(&edge()).expect("the edge formed").strength;
            assert!(strength > previous || strength == FULL_TIE);
            assert!(strength <= FULL_TIE, "strength is a 0..=1 fraction");
            previous = strength;
        }
        assert_eq!(previous, FULL_TIE);
    }

    #[test]
    fn losing_sight_drains_the_tie_to_zero_and_parks_it_there() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), SOMEWHERE, 0, &cfg);
        // Long enough to drain a full tie several times over, but well inside `forget_turns`.
        for turn in 1..100 {
            ledger.decay_all(turn, &cfg);
        }
        let connection = ledger
            .get(&edge())
            .expect("a drained edge PARKS, it is not deleted");
        assert_eq!(connection.strength, NO_TIE);
        assert_eq!(
            connection.last_seen_position, SOMEWHERE,
            "clock 1 still reports where they were"
        );
        assert_eq!(connection.last_seen_turn, 0);
    }

    #[test]
    fn the_fact_of_them_is_forgotten_after_forget_turns() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), SOMEWHERE, 0, &cfg);
        assert_eq!(ledger.decay_all(cfg.forget_turns - 1, &cfg), 0);
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger.decay_all(cfg.forget_turns, &cfg), 1);
        assert!(ledger.is_empty());
    }

    #[test]
    fn a_contact_moves_the_remembered_position_and_a_quiet_turn_does_not() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), SOMEWHERE, 0, &cfg);
        ledger.record_contact(edge(), ELSEWHERE, 1, &cfg);
        ledger.decay_all(2, &cfg);
        let connection = ledger.get(&edge()).expect("the edge");
        assert_eq!(connection.last_seen_position, ELSEWHERE);
        assert_eq!(connection.last_seen_turn, 1);
        assert_eq!(
            connection.first_contact_turn, 0,
            "first contact never moves"
        );
    }

    #[test]
    fn the_reverse_edge_is_a_separate_entry() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        assert!(ledger.record_contact(edge(), SOMEWHERE, 0, &cfg));
        assert_eq!(ledger.len(), 1);
        assert!(ledger.record_contact(ConnectionKey::new(SUBJECT, OBSERVER), SOMEWHERE, 0, &cfg));
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn a_fresher_observation_of_the_same_edge_wins() {
        let mut contacts = ContactsThisTurn::default();
        contacts.record(OBSERVER, SUBJECT, SOMEWHERE, 12);
        contacts.record(OBSERVER, SUBJECT, ELSEWHERE, 3);
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts.iter().next().expect("one contact").1 .0, SOMEWHERE);
        contacts.record(OBSERVER, SUBJECT, ELSEWHERE, 20);
        assert_eq!(contacts.iter().next().expect("one contact").1 .0, ELSEWHERE);
    }
}
