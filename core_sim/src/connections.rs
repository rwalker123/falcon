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
//! | [`Connection::last_seen_position`] / [`Connection::subject_name`] | immediately on losing sight | you know where they *were*, and what they were called. Same as a herd. |
//! | [`Connection::strength`] | over turns without contact, down to zero | the currency of what you know. **At zero nothing flows.** |
//! | the edge itself | very slowly, but not never | eventually you have simply forgotten there was such a people |
//!
//! Zero **parks** the edge; it does not delete it. A parked edge means *"we know such a people
//! exist and have no current tie"*, which is what keeps the third clock a genuinely separate lever
//! rather than a duplicate of the second.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectionKey {
    pub observer: BandId,
    pub subject: BandId,
}

impl ConnectionKey {
    pub fn new(observer: BandId, subject: BandId) -> Self {
        Self { observer, subject }
    }
}

/// **One observation of a subject** — where it stood, on which turn, and the name it answered to.
///
/// The three travel together because they are one sighting: whoever saw the band saw all three at
/// once, so every place a contact is carried (the turn's [`ContactsThisTurn`], an expedition's
/// unreported `pending_contacts`) carries this whole, and [`ConnectionLedger::record_contact`]
/// takes all three or none of them. Splitting the name off would let a fresher position sit beside
/// a staler name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sighting {
    pub position: UVec2,
    /// The turn the subject was **observed** — not when the report arrived. See
    /// [`ConnectionLedger::record_contact`].
    pub observed_turn: u64,
    /// The subject's [`crate::components::BandName`] at `observed_turn`; empty when the subject
    /// carried none.
    pub subject_name: String,
}

impl Sighting {
    pub fn new(position: UVec2, observed_turn: u64, subject_name: impl Into<String>) -> Self {
        Self {
            position,
            observed_turn,
            subject_name: subject_name.into(),
        }
    }
}

/// One directed tie and its three clocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// **Clock 2.** `0..=1`, as a fixed-point [`Scalar`] rather than an `f32` — this is
    /// checkpointed state, and a float would put replay determinism at risk. Raised on contact,
    /// drained without it. At zero nothing flows.
    pub strength: Scalar,
    /// **Clock 1.** Where they were the last time we actually saw them…
    pub last_seen_position: UVec2,
    /// …and on which turn. Neither is touched by a turn without contact: that is the whole of
    /// "you know where they *were*".
    ///
    /// This is when the subject was **observed**, not when the report arrived, so
    /// `last_seen_turn < first_contact_turn` is reachable and correct: a party saw them on turn 40
    /// and the tie formed when the report landed home on turn 60.
    pub last_seen_turn: u64,
    /// …and the name they answered to then. **Clock 1, not a live lookup**: it is what lets a
    /// subject the observer cannot resolve from its own roster — a foreign band, or one that has
    /// since died or split while the tie lingers — still be named. A rename the observer has not
    /// seen does not reach it; a connection never grants live knowledge. Empty means unknown.
    pub subject_name: String,
    /// The turn the tie was last refreshed by a contact event — when the report *reached* the
    /// observer, which for a live sighting is the same turn it was observed. Drives clocks 2 and 3.
    pub last_contact_turn: u64,
    /// The turn the tie first formed. Never changes.
    pub first_contact_turn: u64,
}

/// Every directed tie in the world, keyed by its endpoints.
///
/// **`BTreeMap`, not `HashMap`** — the iteration order is observed by the snapshot and by the
/// checkpoint, so it has to be an order and not an accident.
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
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

    /// **Is there a live tie between these two bands, in EITHER direction?**
    ///
    /// A connection is directed — *who found whom* — and whether a rider requires mutuality is the
    /// rider's business (see the module docs). Neither rider that asks this does: contact is one
    /// undirected fact, and requiring both edges would make it depend on two independent sight
    /// sweeps agreeing on the same turn.
    ///
    /// **A parked tie is not live.** `strength == NO_TIE` is the keystone's *"at zero nothing
    /// flows"*: the edge still exists — we know such a people exist — and it carries nothing.
    ///
    /// It lives on the ledger rather than in a rider because several riders ask it (the supply
    /// network's link rule, and defection's perception test in the wellbeing trickle), and a second
    /// copy of *"what counts as a live tie"* is a second answer free to drift.
    pub fn tie_is_live(&self, a: BandId, b: BandId) -> bool {
        [ConnectionKey::new(a, b), ConnectionKey::new(b, a)]
            .iter()
            .any(|key| {
                self.get(key)
                    .is_some_and(|connection| connection.strength > NO_TIE)
            })
    }

    /// **The two edges between `a` and `b`, as they stand** — `a → b` then `b → a`, each `None` where
    /// that direction has no edge. A reader that wants the undirected answer folds the pair; the
    /// ledger does not choose a fold for it.
    pub fn edges_between(&self, a: BandId, b: BandId) -> [Option<&Connection>; 2] {
        [
            self.get(&ConnectionKey::new(a, b)),
            self.get(&ConnectionKey::new(b, a)),
        ]
    }

    /// **Form (or overwrite) the tie `key` at full strength, as of `turn`.**
    ///
    /// For two groups that were ONE group a moment ago — a band and the splinter that walked out of
    /// it (`systems::split_band_from_parent`). They have not *found* each other; they have always
    /// known each other, so the tie starts where a long acquaintance ends rather than at one
    /// contact's `gain_per_contact`, and it then bleeds on the ordinary clock like any other. Both
    /// turns are `turn` and the sighting is where they stood together.
    pub fn insert_full_tie(&mut self, key: ConnectionKey, sighting: &Sighting, turn: u64) {
        self.edges.insert(
            key,
            Connection {
                strength: FULL_TIE,
                last_seen_position: sighting.position,
                last_seen_turn: sighting.observed_turn,
                subject_name: sighting.subject_name.clone(),
                last_contact_turn: turn,
                first_contact_turn: turn,
            },
        );
    }

    /// Refresh (or form) the tie `key` from a report that the subject was at `sighting.position`
    /// under `sighting.subject_name` on `sighting.observed_turn`, which reached the observer on
    /// `contact_turn`.
    ///
    /// **The two turns are separate because a report can be old.** A live sighting is observed and
    /// received on the same turn, so passing the same value for both is bit-identical to stamping
    /// one turn everywhere; an expedition's comm flush is what makes them differ, reporting on turn
    /// 60 what it saw on turn 40.
    ///
    /// Clock 1 (`last_seen_position` / `last_seen_turn` / `subject_name`) therefore moves **only
    /// when this observation is at least as fresh as the one already held**. A party flushing a stale sighting
    /// still refreshes clocks 2 and 3 — the news arrived, so the tie is live — but it cannot drag
    /// the remembered position backwards to somewhere the subject has since left.
    ///
    /// Returns `true` when the edge did not exist and was formed here — which is what the "edges
    /// formed this turn" counter counts, and what a future first-meeting beat would hang off.
    pub fn record_contact(
        &mut self,
        key: ConnectionKey,
        sighting: &Sighting,
        contact_turn: u64,
        cfg: &ConnectionsConfig,
    ) -> bool {
        let gain = Scalar::from_f32(cfg.strength.gain_per_contact);
        match self.edges.get_mut(&key) {
            Some(connection) => {
                connection.strength = (connection.strength + gain).min(FULL_TIE);
                connection.last_contact_turn = contact_turn;
                if sighting.observed_turn >= connection.last_seen_turn {
                    connection.last_seen_position = sighting.position;
                    connection.last_seen_turn = sighting.observed_turn;
                    connection.subject_name.clone_from(&sighting.subject_name);
                }
                false
            }
            None => {
                self.edges.insert(
                    key,
                    Connection {
                        strength: gain.min(FULL_TIE),
                        last_seen_position: sighting.position,
                        last_seen_turn: sighting.observed_turn,
                        subject_name: sighting.subject_name.clone(),
                        last_contact_turn: contact_turn,
                        first_contact_turn: contact_turn,
                    },
                );
                true
            }
        }
    }

    /// **Credit the tie `key` ONE contact turn without a report** — a stationed party standing at its
    /// host (`ExpeditionMission::Station`) is the home band's maintained presence there, so the edge
    /// takes contact every turn it stays.
    ///
    /// Strength rises by the config's `gain_per_contact` (capped at [`FULL_TIE`]) and
    /// `last_contact_turn` becomes `turn`, so clocks 2 and 3 treat it as a contact turn.
    /// **Clock 1 is untouched** — `last_seen_position` / `last_seen_turn` / `subject_name` move only
    /// when a report lands ([`Self::record_contact`]): being posted somewhere is not a sighting.
    ///
    /// A no-op when the edge does not exist (presence does not found a tie; a launch gate checked
    /// one), and when the edge was already contacted on `turn` — a report landing the same turn is the
    /// one contact, never two.
    pub fn credit_contact(&mut self, key: &ConnectionKey, turn: u64, cfg: &ConnectionsConfig) {
        let gain = Scalar::from_f32(cfg.strength.gain_per_contact);
        if let Some(connection) = self.edges.get_mut(key) {
            if connection.last_contact_turn == turn {
                return;
            }
            connection.strength = (connection.strength + gain).min(FULL_TIE);
            connection.last_contact_turn = turn;
        }
    }

    /// Run clocks 2 and 3 over every edge that saw no contact on `turn`, and return how many edges
    /// clock 3 reaped.
    ///
    /// "Saw contact this turn" is read off `last_contact_turn`, which [`Self::record_contact`] has
    /// already stamped — so this needs no second copy of the turn's contact set, and cannot
    /// disagree with one.
    ///
    /// `last_seen_position` / `last_seen_turn` / `subject_name` are deliberately **not** touched:
    /// clock 1 is exactly the memory of where they were.
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
/// the ledger's own, and a subject stands in exactly one place, so the [`Sighting`] is a value and
/// not part of the identity. Its turn is **when the subject was observed**, which is not always this
/// turn — an expedition's report is what the party saw on the march. When two reports name the same
/// edge, the **fresher observation wins**, and it wins whole: position and name together.
#[derive(Resource, Default, Debug, Clone)]
pub struct ContactsThisTurn {
    sightings: BTreeMap<ConnectionKey, Sighting>,
    /// **Edges maintained by presence this turn** — a stationed party credits its home band's tie
    /// toward the host without a sighting (see [`ConnectionLedger::credit_contact`]).
    credits: BTreeSet<ConnectionKey>,
}

impl ContactsThisTurn {
    /// Record `observer` finding `subject` as `sighting` describes. A staler observation of an edge
    /// already recorded this turn is dropped rather than overwriting it.
    pub fn record(&mut self, observer: BandId, subject: BandId, sighting: Sighting) {
        match self.sightings.entry(ConnectionKey::new(observer, subject)) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(sighting);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                if sighting.observed_turn >= slot.get().observed_turn {
                    slot.insert(sighting);
                }
            }
        }
    }

    /// Record `observer`'s presence at `subject` this turn: the edge takes a contact turn with no
    /// sighting behind it.
    pub fn credit(&mut self, observer: BandId, subject: BandId) {
        self.credits.insert(ConnectionKey::new(observer, subject));
    }

    /// Every contact, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ConnectionKey, &Sighting)> {
        self.sightings.iter()
    }

    /// Every edge credited by presence this turn, in key order.
    pub fn credits(&self) -> impl Iterator<Item = &ConnectionKey> {
        self.credits.iter()
    }

    pub fn len(&self) -> usize {
        self.sightings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sightings.is_empty() && self.credits.is_empty()
    }

    pub fn clear(&mut self) {
        self.sightings.clear();
        self.credits.clear();
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
    for (key, sighting) in contacts.iter() {
        // **Observed then, received now.** A sight-sweep contact observed this very turn passes the
        // same value twice, so direct sight behaves exactly as it did before the two turns split;
        // an expedition's flush is what makes them differ.
        if ledger.record_contact(*key, sighting, turn, &cfg) {
            formed += 1;
        }
    }
    // **Presence after the reports**, so an edge a report already contacted this turn is not credited
    // twice (`credit_contact` skips it).
    for key in contacts.credits() {
        ledger.credit_contact(key, turn, &cfg);
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

    const SUBJECT_NAME: &str = "Reed People";
    const RENAMED: &str = "Ash People";

    fn seen(position: UVec2, observed_turn: u64) -> Sighting {
        Sighting::new(position, observed_turn, SUBJECT_NAME)
    }

    #[test]
    fn a_tie_climbs_on_consecutive_contact_and_stops_at_a_full_tie() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        let mut previous = NO_TIE;
        // Four turns at the shipped gain is exactly a full tie; a fifth must not overshoot.
        for turn in 0..5 {
            ledger.record_contact(edge(), &seen(SOMEWHERE, turn), turn, &cfg);
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
        ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg);
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
        ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg);
        assert_eq!(ledger.decay_all(cfg.forget_turns - 1, &cfg), 0);
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger.decay_all(cfg.forget_turns, &cfg), 1);
        assert!(ledger.is_empty());
    }

    #[test]
    fn a_contact_moves_the_remembered_position_and_a_quiet_turn_does_not() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg);
        ledger.record_contact(edge(), &seen(ELSEWHERE, 1), 1, &cfg);
        ledger.decay_all(2, &cfg);
        let connection = ledger.get(&edge()).expect("the edge");
        assert_eq!(connection.last_seen_position, ELSEWHERE);
        assert_eq!(connection.last_seen_turn, 1);
        assert_eq!(
            connection.first_contact_turn, 0,
            "first contact never moves"
        );
    }

    /// **A stale report refreshes the tie but cannot drag clock 1 backwards.** A party flushing on
    /// turn 52 what it saw on turn 40 is real news — the strength climbs and clock 3 restarts — but
    /// the observer saw them itself on turn 50, and that is still the freshest thing it knows.
    #[test]
    fn a_staler_report_refreshes_the_tie_without_moving_the_remembered_position() {
        const SEEN_TURN: u64 = 50;
        const MARCH_TURN: u64 = 40;
        const FLUSH_TURN: u64 = 52;

        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, SEEN_TURN), SEEN_TURN, &cfg);
        let direct = ledger.get(&edge()).expect("the edge formed").strength;

        ledger.record_contact(edge(), &seen(ELSEWHERE, MARCH_TURN), FLUSH_TURN, &cfg);

        let connection = ledger.get(&edge()).expect("the edge");
        assert_eq!(
            connection.last_seen_position, SOMEWHERE,
            "an older sighting cannot overwrite where we saw them ourselves"
        );
        assert_eq!(connection.last_seen_turn, SEEN_TURN);
        assert!(
            connection.strength > direct,
            "the report still arrived, so the tie is refreshed: {direct:?} -> {:?}",
            connection.strength
        );
        assert_eq!(
            connection.last_contact_turn, FLUSH_TURN,
            "clocks 2 and 3 run off when the news LANDED"
        );
    }

    /// **`last_seen_turn` may precede `first_contact_turn`, and that is the correct reading**: the
    /// party saw them on the march and the tie only existed once the report came home.
    #[test]
    fn a_tie_formed_by_an_old_report_remembers_the_turn_it_was_seen() {
        const MARCH_TURN: u64 = 40;
        const FLUSH_TURN: u64 = 60;

        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        assert!(ledger.record_contact(edge(), &seen(SOMEWHERE, MARCH_TURN), FLUSH_TURN, &cfg));

        let connection = ledger.get(&edge()).expect("the edge formed");
        assert_eq!(connection.last_seen_turn, MARCH_TURN);
        assert_eq!(connection.last_contact_turn, FLUSH_TURN);
        assert_eq!(connection.first_contact_turn, FLUSH_TURN);
    }

    /// **Presence credits the tie without touching clock 1.** A stationed party's turn raises the
    /// strength and refreshes clocks 2 and 3, and leaves where and when the subject was last SEEN
    /// (and by what name) exactly as the last report left them.
    #[test]
    fn a_credit_raises_the_tie_and_never_touches_the_remembered_sighting() {
        const CREDIT_TURN: u64 = 7;
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg);
        let before = ledger.get(&edge()).expect("the edge formed").clone();

        ledger.credit_contact(&edge(), CREDIT_TURN, &cfg);

        let after = ledger.get(&edge()).expect("the edge");
        assert!(
            after.strength > before.strength,
            "the credit raised the tie"
        );
        assert_eq!(after.last_contact_turn, CREDIT_TURN);
        assert_eq!(
            (
                &after.last_seen_position,
                after.last_seen_turn,
                &after.subject_name,
                after.first_contact_turn
            ),
            (
                &before.last_seen_position,
                before.last_seen_turn,
                &before.subject_name,
                before.first_contact_turn
            ),
            "presence is not a sighting"
        );
    }

    /// A credit saturates at a full tie, keeps one at a full tie forever against the decay it is
    /// run beside, forms no edge, and never counts a turn twice.
    #[test]
    fn a_credit_caps_at_full_forms_nothing_and_counts_a_turn_once() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.credit_contact(&edge(), 1, &cfg);
        assert!(ledger.is_empty(), "presence does not found a tie");

        ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg);
        for turn in 1..200 {
            ledger.credit_contact(&edge(), turn, &cfg);
            ledger.decay_all(turn, &cfg);
            assert!(
                ledger.get(&edge()).expect("never reaped").strength >= cfg_gain_floor(&cfg),
                "turn {turn}"
            );
        }
        assert_eq!(ledger.get(&edge()).expect("edge").strength, FULL_TIE);

        // A report that already contacted the edge this turn is the one contact: the credit adds
        // nothing on top of it.
        let mut once = ConnectionLedger::default();
        once.record_contact(edge(), &seen(SOMEWHERE, 3), 3, &cfg);
        let reported = once.get(&edge()).expect("edge").strength;
        once.credit_contact(&edge(), 3, &cfg);
        assert_eq!(once.get(&edge()).expect("edge").strength, reported);
    }

    /// The least a credited tie may ever read once it has formed: one contact's gain.
    fn cfg_gain_floor(cfg: &ConnectionsConfig) -> Scalar {
        Scalar::from_f32(cfg.strength.gain_per_contact)
    }

    #[test]
    fn the_turns_credits_ride_the_contact_set_and_clear_with_it() {
        let mut contacts = ContactsThisTurn::default();
        assert!(contacts.is_empty());
        contacts.credit(OBSERVER, SUBJECT);
        contacts.credit(OBSERVER, SUBJECT);
        assert_eq!(
            contacts.credits().count(),
            1,
            "an edge is credited once a turn"
        );
        assert!(
            contacts.iter().next().is_none(),
            "a credit carries no sighting"
        );
        assert!(!contacts.is_empty());
        contacts.clear();
        assert!(contacts.is_empty());
    }

    #[test]
    fn the_reverse_edge_is_a_separate_entry() {
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        assert!(ledger.record_contact(edge(), &seen(SOMEWHERE, 0), 0, &cfg));
        assert_eq!(ledger.len(), 1);
        assert!(ledger.record_contact(
            ConnectionKey::new(SUBJECT, OBSERVER),
            &seen(SOMEWHERE, 0),
            0,
            &cfg
        ));
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn a_fresher_observation_of_the_same_edge_wins() {
        let mut contacts = ContactsThisTurn::default();
        contacts.record(OBSERVER, SUBJECT, seen(SOMEWHERE, 12));
        contacts.record(OBSERVER, SUBJECT, Sighting::new(ELSEWHERE, 3, RENAMED));
        assert_eq!(contacts.len(), 1);
        let held = contacts.iter().next().expect("one contact").1;
        assert_eq!(held.position, SOMEWHERE);
        assert_eq!(
            held.subject_name, SUBJECT_NAME,
            "a staler observation loses whole — its name with its position"
        );
        contacts.record(OBSERVER, SUBJECT, Sighting::new(ELSEWHERE, 20, RENAMED));
        let held = contacts.iter().next().expect("one contact").1;
        assert_eq!(held.position, ELSEWHERE);
        assert_eq!(held.subject_name, RENAMED);
    }

    /// **The name is clock 1**: a stale report under a different name cannot overwrite the name the
    /// observer last saw them under, and a fresher one does.
    #[test]
    fn the_remembered_name_moves_with_the_remembered_position() {
        const SEEN_TURN: u64 = 50;
        const MARCH_TURN: u64 = 40;
        const LATER_TURN: u64 = 55;

        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, SEEN_TURN), SEEN_TURN, &cfg);
        ledger.record_contact(
            edge(),
            &Sighting::new(ELSEWHERE, MARCH_TURN, RENAMED),
            LATER_TURN,
            &cfg,
        );
        assert_eq!(
            ledger.get(&edge()).expect("the edge").subject_name,
            SUBJECT_NAME
        );
        ledger.record_contact(
            edge(),
            &Sighting::new(ELSEWHERE, LATER_TURN, RENAMED),
            LATER_TURN,
            &cfg,
        );
        assert_eq!(ledger.get(&edge()).expect("the edge").subject_name, RENAMED);
    }
}
