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
//! | [`Connection::last_seen_position`] / [`Connection::subject_name`] / [`Connection::subject_faction`] | immediately on losing sight | you know where they *were*, what they were called and whose people they belonged to. Same as a herd. |
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
    belief::{credit_ritual_lesson, BeliefRegistry},
    belief_config::BeliefConfigHandle,
    components::{BandId, BandName, PopulationCohort, ResidentBand, Tile},
    connections_config::{ConnectionsConfig, ConnectionsConfigHandle},
    intensification::LadderConfigHandle,
    metrics::SimulationMetrics,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, DiscoveryProgressLedger,
        SimulationTick,
    },
    scalar::Scalar,
    systems::band_label,
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

/// **One observation of a subject** — where it stood, on which turn, the name it answered to and
/// the people it belonged to.
///
/// The four travel together because they are one sighting: whoever saw the band saw all four at
/// once, so every place a contact is carried (the turn's [`ContactsThisTurn`], an expedition's
/// unreported `pending_contacts`) carries this whole, and [`ConnectionLedger::record_contact`]
/// takes all four or none of them. Splitting the name or the people off would let a fresher
/// position sit beside a staler name or a staler people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sighting {
    pub position: UVec2,
    /// The turn the subject was **observed** — not when the report arrived. See
    /// [`ConnectionLedger::record_contact`].
    pub observed_turn: u64,
    /// The subject's [`crate::components::BandName`] at `observed_turn`; empty when the subject
    /// carried none.
    pub subject_name: String,
    /// The people the subject belonged to at `observed_turn`. Captured at the moment of sight,
    /// exactly like the name.
    pub subject_faction: FactionId,
}

impl Sighting {
    pub fn new(
        position: UVec2,
        observed_turn: u64,
        subject_name: impl Into<String>,
        subject_faction: FactionId,
    ) -> Self {
        Self {
            position,
            observed_turn,
            subject_name: subject_name.into(),
            subject_faction,
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
    /// …and the people they belonged to then. **Clock 1, like the name**: it is the faction the
    /// subject was last SEEN under, not a live lookup, so a band that has since defected is still
    /// remembered under its old people until it is seen again. It is carried and published; nothing
    /// in this module branches on it.
    pub subject_faction: FactionId,
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
    /// **The tile a traveling party sets out toward** — what its people know of where `target` is,
    /// never where it really is. The target's live tile when the sender knows it: the target is the
    /// sender's own people (your own bands are always visible to you), or its tile is `Active` on the
    /// sending faction's map. Otherwise the `home -> target` edge's `last_seen_position`; with no
    /// edge at all (the launch gate prevents it), the live tile is the only position there is.
    pub fn launch_aim(
        &self,
        visibility: &crate::visibility::VisibilityLedger,
        sender_faction: FactionId,
        home: BandId,
        target: BandId,
        target_faction: FactionId,
        target_live_tile: UVec2,
    ) -> UVec2 {
        let known = sender_faction == target_faction
            || visibility.is_visible(sender_faction, target_live_tile.x, target_live_tile.y);
        if known {
            return target_live_tile;
        }
        self.get(&ConnectionKey::new(home, target))
            .map(|connection| connection.last_seen_position)
            .unwrap_or(target_live_tile)
    }

    pub fn get(&self, key: &ConnectionKey) -> Option<&Connection> {
        self.edges.get(key)
    }

    /// **What `observer` holds a live tie to, as it last saw it** - every edge `observer -> X` with
    /// `strength > NO_TIE`, in `X` order, each as the [`Sighting`] the observer's own memory holds
    /// (clock 1: position, observed turn, name, people). A parked tie is not live and is skipped.
    /// Only edges whose `last_contact_turn` is `since_turn` or later count: a tie that is live but
    /// stale is not fresh news. The news a stationed delegate's runner carries about who its host
    /// knows.
    pub fn live_sightings_of(
        &self,
        observer: BandId,
        since_turn: u64,
    ) -> impl Iterator<Item = (BandId, Sighting)> + '_ {
        let from = ConnectionKey::new(observer, BandId(u64::MIN));
        let to = ConnectionKey::new(observer, BandId(u64::MAX));
        self.edges
            .range(from..=to)
            .filter(move |(_, edge)| edge.strength > NO_TIE && edge.last_contact_turn >= since_turn)
            .map(|(key, edge)| {
                (
                    key.subject,
                    Sighting::new(
                        edge.last_seen_position,
                        edge.last_seen_turn,
                        edge.subject_name.clone(),
                        edge.subject_faction,
                    ),
                )
            })
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
                subject_faction: sighting.subject_faction,
                last_contact_turn: turn,
                first_contact_turn: turn,
            },
        );
    }

    /// Refresh (or form) the tie `key` from a report that the subject was at `sighting.position`
    /// under `sighting.subject_name` (of `sighting.subject_faction`) on `sighting.observed_turn`, which reached the observer on
    /// `contact_turn`.
    ///
    /// **The two turns are separate because a report can be old.** A live sighting is observed and
    /// received on the same turn, so passing the same value for both is bit-identical to stamping
    /// one turn everywhere; an expedition's comm flush is what makes them differ, reporting on turn
    /// 60 what it saw on turn 40.
    ///
    /// Clock 1 (`last_seen_position` / `last_seen_turn` / `subject_name` / `subject_faction`) therefore moves **only
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
                    connection.subject_faction = sighting.subject_faction;
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
                        subject_faction: sighting.subject_faction,
                        last_contact_turn: contact_turn,
                        first_contact_turn: contact_turn,
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
    /// `last_seen_position` / `last_seen_turn` / `subject_name` / `subject_faction` are deliberately **not** touched:
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
    /// The edges at least one **live sighting** found this turn — one band standing on a tile and
    /// seeing another band on its tile. An edge only an expedition's report named is NOT here, so a
    /// report landing (which may be about a band far away) can never be read as two bands meeting.
    /// Read by [`advance_connections`] to detect a gathering.
    live: BTreeSet<ConnectionKey>,
}

impl ContactsThisTurn {
    /// Record `observer` finding `subject` as `sighting` describes — a **live sighting** from the
    /// sight sweep, which also marks the edge as a band-to-band meeting (see
    /// [`Self::is_live_sighting`]). A staler observation of an edge already recorded this turn is
    /// dropped rather than overwriting it.
    pub fn record(&mut self, observer: BandId, subject: BandId, sighting: Sighting) {
        self.live.insert(ConnectionKey::new(observer, subject));
        self.insert(observer, subject, sighting);
    }

    /// Record a contact an expedition's **report** carried home. It refreshes the tie exactly as a
    /// sighting does, but it is not two bands meeting, so it never marks the edge live.
    pub fn record_report(&mut self, observer: BandId, subject: BandId, sighting: Sighting) {
        self.insert(observer, subject, sighting);
    }

    /// Did a live band-to-band sighting (not only a report) find this edge this turn?
    pub fn is_live_sighting(&self, key: &ConnectionKey) -> bool {
        self.live.contains(key)
    }

    fn insert(&mut self, observer: BandId, subject: BandId, sighting: Sighting) {
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

    /// Every contact, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ConnectionKey, &Sighting)> {
        self.sightings.iter()
    }

    pub fn len(&self) -> usize {
        self.sightings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sightings.is_empty()
    }

    pub fn clear(&mut self) {
        self.sightings.clear();
        self.live.clear();
    }
}

/// **Kin relay a tie, within one people.** Runs after this turn's direct contacts are folded in and
/// before the decay. The graph is exactly this turn's DIRECT contacts (`direct`: the keys of
/// [`ContactsThisTurn`], sightings and landed reports), directed observer to subject, between
/// resident bands of one people. Band `a` learns of `b` when a directed path `a -> m -> ... -> b` of
/// length `2..=kin_relay.max_hops` runs over those edges: kin `m` saw `b` this turn and told `a`.
/// A length-one path is `a`'s own contact, so it is never relayed (no reverse edge, and a band that
/// sees nobody hears nothing). A relayed write is never a graph edge, this turn or later, so a tie
/// with no fresh direct news along its chain drains on the ordinary clock.
///
/// It is **news, never a sighting**: it goes straight to [`ConnectionLedger::record_contact`] and
/// never through [`ContactsThisTurn`], and it runs after the gathering's `meetings` were read, so it
/// can neither make nor suppress a gathering.
fn relay_among_kin(
    ledger: &mut ConnectionLedger,
    direct: &BTreeSet<ConnectionKey>,
    residents: &Query<(&BandId, &PopulationCohort, Option<&BandName>), With<ResidentBand>>,
    tiles: &Query<&Tile>,
    turn: u64,
    cfg: &ConnectionsConfig,
    formed: &mut u32,
) {
    let max_hops = cfg.kin_relay.max_hops;
    if max_hops < MIN_RELAY_HOPS {
        return;
    }
    // Bands by id (sorted), so every walk below is deterministic.
    let bands: BTreeMap<BandId, (FactionId, Sighting)> = residents
        .iter()
        .filter_map(|(id, cohort, name)| {
            let position = tiles.get(cohort.current_tile).ok()?.position;
            let name = name.map_or_else(|| band_label(*id), |name| name.0.clone());
            Some((
                *id,
                (
                    cohort.faction,
                    Sighting::new(position, turn, name, cohort.faction),
                ),
            ))
        })
        .collect();
    let mut sees: BTreeMap<BandId, Vec<BandId>> = BTreeMap::new();
    for key in direct {
        if let (Some((from, _)), Some((to, _))) =
            (bands.get(&key.observer), bands.get(&key.subject))
        {
            if from == to {
                sees.entry(key.observer).or_default().push(key.subject);
            }
        }
    }
    let mut relays: Vec<(ConnectionKey, Sighting)> = Vec::new();
    for a in bands.keys() {
        let mut reached: BTreeSet<BandId> = BTreeSet::from([*a]);
        let mut frontier = vec![*a];
        for hops in 1..=max_hops {
            let mut next = Vec::new();
            for at in &frontier {
                for b in sees.get(at).into_iter().flatten() {
                    if reached.insert(*b) {
                        next.push(*b);
                        if hops >= MIN_RELAY_HOPS {
                            relays.push((ConnectionKey::new(*a, *b), bands[b].1.clone()));
                        }
                    }
                }
            }
            frontier = next;
        }
    }
    for (key, sighting) in relays {
        if ledger.record_contact(key, &sighting, turn, cfg) {
            *formed += 1;
        }
    }
}

/// The shortest chain that is news rather than a band's own contact: `a` hears of `b` from a kin
/// who saw `b`.
const MIN_RELAY_HOPS: u32 = 2;

/// **How many gatherings each people has held** — a cumulative per-faction count, the total the
/// Telling diffs into `gatherings.held_this_turn` (the `sites.discovered_this_turn` pattern).
/// Checkpoint state: nothing rebuilds a count of past meetings.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatheringLedger {
    held: BTreeMap<FactionId, u64>,
}

impl GatheringLedger {
    /// Count one more gathering for `faction`.
    pub fn record(&mut self, faction: FactionId) {
        *self.held.entry(faction).or_insert(0) += 1;
    }

    /// Every gathering `faction` has held, cumulative.
    pub fn held_by(&self, faction: FactionId) -> u64 {
        self.held.get(&faction).copied().unwrap_or(0)
    }
}

/// An unordered pair of bands, lower id first — the unit a gathering is deduplicated on.
fn band_pair(a: BandId, b: BandId) -> (BandId, BandId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// One band of a pair, as a gathering reads it.
struct Attendee {
    band: BandId,
    faction: FactionId,
    size: u32,
    tile: Entity,
    name: String,
    /// Turns the band has been simulated; see [`FOUNDING_AGE_TURNS`].
    age_turns: u32,
}

/// A band whose `age_turns` is at most this has not yet lived past the turn it came into being.
/// `simulate_population` (Population) bumps `age_turns` before this Visibility pass, so a band
/// founded between turns (worldgen, a split command) reads `1` here on its first turn, and one
/// founded mid-turn after the Population pass reads `0`. A band founded later (a split's child)
/// holds no gathering on that turn: what it sees is where it started, not a meeting. The world's
/// opening sightings are the exception: they count as a meeting at turn 0 (see [`OPENING_TURN`]).
const FOUNDING_AGE_TURNS: u32 = 1;

/// The `SimulationTick` read while the world's first turn resolves (`advance_tick` runs in
/// `Snapshot`, after the turn). Bands in sight of each other when the game starts met "at turn 0",
/// so the founding-turn skip does not apply on this turn.
const OPENING_TURN: u64 = 0;

/// The label of a gathering's event line.
const GATHERING_LABEL: &str = "A gathering";

/// Advance the three clocks: raise every tie contacted this turn, drain every tie that was not, and
/// reap the ones nobody has seen in `forget_turns`.
///
/// Chained after `calculate_visibility` (which is what *finds* the contacts) and before
/// `apply_visibility_decay`, inside the serial `TurnStage::Visibility` chain. It consumes
/// [`ContactsThisTurn`] and clears it, so the set is rebuilt from scratch every turn.
#[allow(clippy::too_many_arguments)]
pub fn advance_connections(
    mut ledger: ResMut<ConnectionLedger>,
    mut contacts: ResMut<ContactsThisTurn>,
    config: Res<ConnectionsConfigHandle>,
    tick: Res<SimulationTick>,
    mut metrics: ResMut<SimulationMetrics>,
    residents: Query<(&BandId, &PopulationCohort, Option<&BandName>), With<ResidentBand>>,
    tiles: Query<&Tile>,
    belief_config: Res<BeliefConfigHandle>,
    mut belief: ResMut<BeliefRegistry>,
    mut gatherings: ResMut<GatheringLedger>,
    mut event_log: ResMut<CommandEventLog>,
    ladder: Res<LadderConfigHandle>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
) {
    let cfg = config.get();
    let turn = tick.0;

    // **A gathering is two bands meeting after having no live tie.** Read before any of this
    // turn's contacts is folded in, so the first directed edge of a pair to be refreshed cannot make
    // the pair look tied to the second. Deduplicated per UNORDERED pair: edges are directed.
    let meetings: BTreeSet<(BandId, BandId)> = contacts
        .iter()
        .map(|(key, _)| key)
        .filter(|key| contacts.is_live_sighting(key))
        .filter(|key| !ledger.tie_is_live(key.observer, key.subject))
        .map(|key| band_pair(key.observer, key.subject))
        .collect();

    let direct: BTreeSet<ConnectionKey> = contacts.iter().map(|(key, _)| *key).collect();
    let mut formed = 0u32;
    for (key, sighting) in contacts.iter() {
        // **Observed then, received now.** A sight-sweep contact observed this very turn passes the
        // same value twice, so direct sight behaves exactly as it did before the two turns split;
        // an expedition's flush is what makes them differ.
        if ledger.record_contact(*key, sighting, turn, &cfg) {
            formed += 1;
        }
    }
    relay_among_kin(
        &mut ledger,
        &direct,
        &residents,
        &tiles,
        turn,
        &cfg,
        &mut formed,
    );
    let reaped = ledger.decay_all(turn, &cfg);
    contacts.clear();

    if !meetings.is_empty() {
        let belief_cfg = belief_config.get();
        let ladder = ladder.get();
        let attendee = |band: BandId| -> Option<Attendee> {
            residents
                .iter()
                .find(|(id, _, _)| **id == band)
                .map(|(id, cohort, name)| Attendee {
                    band: *id,
                    faction: cohort.faction,
                    size: cohort.size,
                    tile: cohort.current_tile,
                    age_turns: cohort.age_turns,
                    name: name.map_or_else(|| band_label(*id), |name| name.0.clone()),
                })
        };
        for (low, high) in meetings {
            let (Some(low), Some(high)) = (attendee(low), attendee(high)) else {
                continue;
            };
            // The world's opening sightings count as a meeting at turn 0; a band founded later (a
            // split) holds none on its founding turn, because what it sees is where it started.
            let founding =
                low.age_turns <= FOUNDING_AGE_TURNS || high.age_turns <= FOUNDING_AGE_TURNS;
            if founding && turn != OPENING_TURN {
                continue;
            }
            // The larger band hosts; a tie goes to the lower band id (`low` is the lower id).
            let host = if high.size > low.size { &high } else { &low };
            let Ok(place) = tiles.get(host.tile) else {
                continue;
            };
            belief.credit_gathering(place.position, &belief_cfg);
            // **A gathering teaches ritual to the people of EACH band in the pair** - and a pair of
            // one people pays it once, not twice (`belief::credit_ritual_lesson`).
            credit_ritual_lesson(&ladder.knowledge, low.faction, &mut discovery);
            if high.faction != low.faction {
                credit_ritual_lesson(&ladder.knowledge, high.faction, &mut discovery);
            }
            for (own, other) in [(&low, &high), (&high, &low)] {
                // One line per people: a pair of one people is told once, from its lower band.
                if own.faction == other.faction && own.band != low.band {
                    continue;
                }
                gatherings.record(own.faction);
                event_log.push(
                    CommandEventEntry::new(
                        turn,
                        CommandEventKind::GatheringHeld,
                        own.faction,
                        GATHERING_LABEL,
                        Some(format!(
                            "{} met {} at ({}, {}).",
                            own.name, other.name, place.position.x, place.position.y
                        )),
                    )
                    .with_band(other.band),
                );
            }
        }
    }

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
    /// The people the subject belonged to when first seen, and the people a later report names.
    const SUBJECT_FACTION: FactionId = FactionId(1);
    const DEFECTED_TO: FactionId = FactionId(2);

    fn seen(position: UVec2, observed_turn: u64) -> Sighting {
        Sighting::new(position, observed_turn, SUBJECT_NAME, SUBJECT_FACTION)
    }

    #[test]
    fn a_launch_aims_at_the_live_tile_only_when_the_sender_knows_it() {
        const MINE: FactionId = FactionId(0);
        const THEIRS: FactionId = FactionId(1);
        const MAP_SIDE: u32 = 16;
        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, 1), 1, &cfg);
        let mut visibility = crate::visibility::VisibilityLedger::default();
        let _ = visibility.ensure_faction(MINE, MAP_SIDE, MAP_SIDE);
        let aim = |ledger: &ConnectionLedger,
                   visibility: &crate::visibility::VisibilityLedger,
                   target_faction: FactionId| {
            ledger.launch_aim(
                visibility,
                MINE,
                OBSERVER,
                SUBJECT,
                target_faction,
                ELSEWHERE,
            )
        };
        assert_eq!(
            aim(&ledger, &visibility, THEIRS),
            SOMEWHERE,
            "a foreign band out of sight is where it was last seen"
        );
        assert_eq!(
            aim(&ledger, &visibility, MINE),
            ELSEWHERE,
            "your own people are always known"
        );
        visibility
            .get_faction_mut(MINE)
            .expect("map")
            .mark_active(ELSEWHERE.x, ELSEWHERE.y, 1);
        assert_eq!(
            aim(&ledger, &visibility, THEIRS),
            ELSEWHERE,
            "a foreign band on a tile you see is where it is"
        );
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
        assert_eq!(connection.subject_name, SUBJECT_NAME);
        assert_eq!(
            connection.subject_faction, SUBJECT_FACTION,
            "decay never touches the remembered people"
        );
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
        assert_eq!(connection.subject_name, SUBJECT_NAME);
        assert_eq!(
            connection.subject_faction, SUBJECT_FACTION,
            "formation records the people the subject was seen under"
        );
        assert_eq!(connection.last_contact_turn, FLUSH_TURN);
        assert_eq!(connection.first_contact_turn, FLUSH_TURN);
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
        contacts.record(
            OBSERVER,
            SUBJECT,
            Sighting::new(ELSEWHERE, 3, RENAMED, DEFECTED_TO),
        );
        assert_eq!(contacts.len(), 1);
        let held = contacts.iter().next().expect("one contact").1;
        assert_eq!(held.position, SOMEWHERE);
        assert_eq!(
            held.subject_name, SUBJECT_NAME,
            "a staler observation loses whole — its name with its position"
        );
        assert_eq!(
            held.subject_faction, SUBJECT_FACTION,
            "…and its people with its position"
        );
        contacts.record(
            OBSERVER,
            SUBJECT,
            Sighting::new(ELSEWHERE, 20, RENAMED, DEFECTED_TO),
        );
        let held = contacts.iter().next().expect("one contact").1;
        assert_eq!(held.position, ELSEWHERE);
        assert_eq!(held.subject_name, RENAMED);
        assert_eq!(held.subject_faction, DEFECTED_TO);
    }

    /// **The name and the people are clock 1**: a stale report under a different name or people
    /// cannot overwrite what the observer last saw them as, and a fresher one does.
    #[test]
    fn the_remembered_name_and_people_move_with_the_remembered_position() {
        const SEEN_TURN: u64 = 50;
        const MARCH_TURN: u64 = 40;
        const LATER_TURN: u64 = 55;

        let cfg = config();
        let mut ledger = ConnectionLedger::default();
        ledger.record_contact(edge(), &seen(SOMEWHERE, SEEN_TURN), SEEN_TURN, &cfg);
        ledger.record_contact(
            edge(),
            &Sighting::new(ELSEWHERE, MARCH_TURN, RENAMED, DEFECTED_TO),
            LATER_TURN,
            &cfg,
        );
        let held = ledger.get(&edge()).expect("the edge");
        assert_eq!(held.subject_name, SUBJECT_NAME);
        assert_eq!(
            held.subject_faction, SUBJECT_FACTION,
            "a stale report cannot move the remembered people either"
        );
        ledger.record_contact(
            edge(),
            &Sighting::new(ELSEWHERE, LATER_TURN, RENAMED, DEFECTED_TO),
            LATER_TURN,
            &cfg,
        );
        let held = ledger.get(&edge()).expect("the edge");
        assert_eq!(held.subject_name, RENAMED);
        assert_eq!(held.subject_faction, DEFECTED_TO);
    }
}
