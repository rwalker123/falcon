//! **The knowledge rider on a connection** — a people learns what another people knows by being
//! around it (`docs/plan_contact_and_logistics.md` §Settled by #531, arc #527).
//!
//! Per observer people, per discovery it does not yet know, per turn:
//!
//! ```text
//! credit = max over channel instances( tie × channel_rate × observability ) / lesson_cost
//! ```
//!
//! - A **channel instance** is an `(observer band, subject band)` pair present this turn on one
//!   [`ContactChannel`]. Only discoveries the **subject band's people knows** — ledger progress at
//!   the ladder's `completion_threshold`, the bar every gate reads — teach.
//! - **`tie`** is the strength of the directed edge `observer → subject` in the
//!   [`ConnectionLedger`]. One direction is enough; a parked edge ([`NO_TIE`]) teaches nothing, on
//!   every channel.
//! - **Max, not sum** — across every band of the observer's people and every subject band. Ten bands
//!   that each glimpsed a camp do not learn ten times faster than one that trades with it.
//! - **`/ lesson_cost`** — the ladder's `lesson_costs[tag]`, through
//!   [`LadderKnowledge::ledger_credit`], the one place practice units become ledger progress. Contact
//!   and practice credit **one** ledger entry, so contact shortens the road and never replaces it.
//!
//! # There is no faction branch
//!
//! Knowledge is faction-level, so a band watching a band of its own people already knows everything
//! the subject's people knows, and *"credit what they know and you don't"* credits nothing. The
//! module discipline of `connections.rs` — **faction is a property of the endpoint, never a branch**
//! — holds here without a filter. Faction is read off each band for the one question the rule asks
//! of it: whose ledger row.
//!
//! # One rule, two callers
//!
//! [`contact_lessons`] is a pure function of the world: the turn system ([`learn_over_connections`])
//! applies what it returns, and the snapshot capture publishes what it returns for the viewer. Every
//! input it reads is checkpointed (`SimState` carries the two ledgers, the road registry and each
//! band's cohort, whose `last_turn_transfer_crossings` is the trade record), so the **first frame
//! after a load is correct with nothing extra carried** — the trap `SupplyNetworkMembership` fell
//! into (`.claude/rules/core_sim/checkpoints.md`) cannot open here because there is no derived
//! resource to go missing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    components::{
        BandId, LaborAllocation, PopulationCohort, Tile, TransferCause, TransferCrossing,
    },
    connections::{ConnectionKey, ConnectionLedger, NO_TIE},
    intensification::{knows, LadderConfigHandle, LadderKnowledge},
    knowledge_contact_config::{ContactChannelRates, KnowledgeContactConfigHandle},
    orders::FactionId,
    resources::DiscoveryProgressLedger,
    routes::RoadRegistry,
    scalar::scalar_from_f32,
    start_profile::{StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle},
};

/// **How two bands are together this turn** — each with its own strength
/// ([`ContactChannelRates`]). The derived `Ord` is the declaration order, which is what makes the
/// instance walk in [`contact_lessons`] deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContactChannel {
    /// The observer band holds a live tie toward the subject band.
    Watching,
    /// A shipment (`ExpeditionMission::Trade`) between the two bands landed this turn, in either
    /// direction — each band is observer of the other.
    Trade,
    /// The observer band stands on a road tile the subject band keeps, on the condition the kept
    /// road's sight grant uses (`crate::routes::Road::grants_sight`).
    Road,
}

impl ContactChannel {
    /// This channel's strength, in practice units per turn.
    pub fn rate(self, rates: &ContactChannelRates) -> f32 {
        match self {
            ContactChannel::Watching => rates.watching,
            ContactChannel::Trade => rates.trade,
            ContactChannel::Road => rates.road,
        }
    }

    /// The wire's `ubyte` — `0 = watching`, `1 = trade`, `2 = road`, as
    /// `ContactLessonState.channel` documents. Append-only, like the schema it rides.
    pub fn wire_code(self) -> u8 {
        match self {
            ContactChannel::Watching => 0,
            ContactChannel::Trade => 1,
            ContactChannel::Road => 2,
        }
    }
}

/// One discovery contact can teach, resolved from the two tables that each own half of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TeachableLesson {
    pub discovery: u32,
    /// The knowledge tag — `start_profile_knowledge_tags.json`'s key and `lesson_costs`' key.
    pub tag: String,
    pub observability: f32,
}

/// **Every discovery contact can teach** — every tag in `start_profile_knowledge_tags.json`, the
/// table that maps each knowledge tag to its discovery id and carries its `observability`. Sorted by
/// discovery id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TeachableLessons(Vec<TeachableLesson>);

#[derive(Debug, Error, PartialEq)]
pub enum KnowledgeContactError {
    #[error(
        "knowledge tag {tag:?} has no knowledge.lesson_costs entry in intensification_ladder.json — \
         contact can teach every tag, so every tag must be priced (a missing cost would pace the \
         lesson off a default nobody chose)"
    )]
    UnpricedLesson { tag: String },
}

impl TeachableLessons {
    /// Join the tag table to the ladder's prices. **Every tag must be priced** — contact can teach
    /// any of them, and a defaulted cost would pace a lesson off a number nobody chose — so a
    /// missing `lesson_costs` entry is an error, which boot turns into a panic.
    pub fn resolve(
        tags: &StartProfileKnowledgeTags,
        knowledge: &LadderKnowledge,
    ) -> Result<Self, KnowledgeContactError> {
        let mut lessons = Vec::with_capacity(tags.len());
        for (tag, definition) in tags.iter() {
            if knowledge.lesson_cost(tag).is_none() {
                return Err(KnowledgeContactError::UnpricedLesson {
                    tag: tag.to_string(),
                });
            }
            lessons.push(TeachableLesson {
                discovery: definition.discovery_id(),
                tag: tag.to_string(),
                observability: definition.observability(),
            });
        }
        lessons.sort_by_key(|lesson| lesson.discovery);
        Ok(Self(lessons))
    }

    pub fn iter(&self) -> impl Iterator<Item = &TeachableLesson> {
        self.0.iter()
    }

    /// The tag a discovery id is taught under, for the wire.
    pub fn tag(&self, discovery: u32) -> Option<&str> {
        self.0
            .iter()
            .find(|lesson| lesson.discovery == discovery)
            .map(|lesson| lesson.tag.as_str())
    }
}

/// A band as the rule sees it: whose people it is, and where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactBand {
    pub band: BandId,
    pub faction: FactionId,
    /// `None` when the band's tile does not resolve — it can then stand on no road.
    pub tile: Option<UVec2>,
}

/// A shipment that landed this turn: `receiver` took delivery of goods `sender` outfitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TradeLanding {
    pub receiver: BandId,
    pub sender: BandId,
}

/// **The shipments a band took delivery of this window**, read off its own crossings — the per-turn
/// record the Trade tab already publishes (`TransferCause::ShipmentIn`, counterparty = the sending
/// band). A shipment books one row per commodity, so the caller dedups; [`contact_lessons`] does.
///
/// The system reads `LaborAllocation::last_transfer_crossings` (the live accumulator) and the capture
/// reads `PopulationCohort::last_turn_transfer_crossings` (its published copy, which survives a
/// recapture after the accumulator resets): one window, two homes, one reader.
pub fn trade_landings(
    receiver: BandId,
    crossings: &[TransferCrossing],
) -> impl Iterator<Item = TradeLanding> + '_ {
    crossings
        .iter()
        .filter(|crossing| crossing.cause == TransferCause::ShipmentIn)
        .filter_map(move |crossing| {
            crossing.counterparty.map(|sender| TradeLanding {
                receiver,
                sender: sender.band,
            })
        })
}

/// **The strongest source teaching one discovery to one people this turn.**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactLesson {
    /// The band learned from…
    pub subject_band: BandId,
    /// …and its people — what the readout names.
    pub subject_faction: FactionId,
    pub channel: ContactChannel,
    /// Ledger progress credited this turn (already divided by the lesson's cost).
    pub credit: f32,
}

/// Every lesson contact teaches this turn, keyed by `(observer people, discovery)`. A `BTreeMap` so
/// both the apply order and the wire order are an order rather than an accident.
pub type ContactLessons = BTreeMap<(FactionId, u32), ContactLesson>;

/// Everything [`contact_lessons`] reads. Borrowed, so the turn system and the capture each assemble
/// it from what they already hold.
pub struct ContactWorld<'a> {
    pub bands: &'a [ContactBand],
    pub landings: &'a [TradeLanding],
    pub connections: &'a ConnectionLedger,
    pub roads: &'a RoadRegistry,
    pub progress: &'a DiscoveryProgressLedger,
    pub lessons: &'a TeachableLessons,
    pub knowledge: &'a LadderKnowledge,
    pub rates: &'a ContactChannelRates,
}

/// **THE RULE** — see the module docs. Pure: reads the ledger, writes nothing, so "knows" is read
/// off one fixed ledger state and the order the instances are walked in changes nothing but the
/// tie-break (below).
///
/// **Ties between equal credits go to the first instance in `(observer, subject, channel)` order** —
/// the instances are collected into a sorted set, and a later instance must be strictly stronger to
/// win. Equal credits teach the same amount either way; the order only picks which source the
/// readout names.
pub fn contact_lessons(world: &ContactWorld) -> ContactLessons {
    let factions: HashMap<BandId, FactionId> = world
        .bands
        .iter()
        .map(|band| (band.band, band.faction))
        .collect();

    // **Every channel instance present this turn.** Each still multiplies by the observer → subject
    // tie below, so an instance on an edge that does not exist or is parked teaches nothing.
    let mut instances: BTreeSet<(BandId, BandId, ContactChannel)> = BTreeSet::new();
    for (key, _) in world.connections.iter() {
        instances.insert((key.observer, key.subject, ContactChannel::Watching));
    }
    for landing in world.landings {
        instances.insert((landing.receiver, landing.sender, ContactChannel::Trade));
        instances.insert((landing.sender, landing.receiver, ContactChannel::Trade));
    }
    for band in world.bands {
        let Some(road) = band.tile.and_then(|tile| world.roads.road(tile)) else {
            continue;
        };
        if !road.grants_sight() {
            continue;
        }
        if let Some(keeper) = road.keeper {
            instances.insert((band.band, keeper.band, ContactChannel::Road));
        }
    }

    let threshold = world.knowledge.completion_threshold;
    let mut lessons = ContactLessons::new();
    for (observer, subject, channel) in instances {
        let Some(tie) = world
            .connections
            .get(&ConnectionKey::new(observer, subject))
            .map(|connection| connection.strength)
            .filter(|strength| *strength > NO_TIE)
        else {
            continue;
        };
        let (Some(&observer_faction), Some(&subject_faction)) =
            (factions.get(&observer), factions.get(&subject))
        else {
            continue;
        };
        let reach = tie.to_f32() * channel.rate(world.rates);
        if reach <= 0.0 {
            continue;
        }
        for lesson in world.lessons.iter() {
            if !knows(world.progress, subject_faction, lesson.discovery, threshold)
                || knows(
                    world.progress,
                    observer_faction,
                    lesson.discovery,
                    threshold,
                )
            {
                continue;
            }
            let Some(credit) = world
                .knowledge
                .ledger_credit(&lesson.tag, reach * lesson.observability)
            else {
                // `TeachableLessons::resolve` refused any unpriced tag, so this is unreachable for
                // a resolved set.
                continue;
            };
            if credit <= 0.0 {
                continue;
            }
            let candidate = ContactLesson {
                subject_band: subject,
                subject_faction,
                channel,
                credit,
            };
            lessons
                .entry((observer_faction, lesson.discovery))
                .and_modify(|best| {
                    if candidate.credit > best.credit {
                        *best = candidate;
                    }
                })
                .or_insert(candidate);
        }
    }
    lessons
}

/// Credit every lesson to its people's ledger — the same `add_progress` practice writes through,
/// clamped as it clamps.
pub fn apply_contact_lessons(lessons: &ContactLessons, progress: &mut DiscoveryProgressLedger) {
    for ((faction, discovery), lesson) in lessons {
        progress.add_progress(*faction, *discovery, scalar_from_f32(lesson.credit));
    }
}

/// **The knowledge rider, once a turn.**
///
/// It runs in `TurnStage::Visibility`, **after `connections::advance_connections`** (declared as
/// after `sites::discover_sites`, the end of that stage's chain, because the site sweep takes
/// `PopulationCohort` mutably and the ambiguity gate wants the edge stated), for two reasons that
/// both have to hold:
///
/// - the tie it multiplies by is the one this turn's contact just refreshed — the sight sweep that
///   finds contact runs earlier in the same stage, and `advance_connections` is what folds it into
///   the ledger;
/// - the two other channels' inputs are already final: a shipment lands in `advance_expeditions`
///   and a road's keeping is paid inside `advance_labor_allocation`, both in `TurnStage::Population`,
///   which precedes this stage.
///
/// It computes every credit off the start-of-pass ledger and then applies them, so no lesson learned
/// this pass can teach onward in the same pass and iteration order changes nothing.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn learn_over_connections(
    connections: Res<ConnectionLedger>,
    roads: Res<RoadRegistry>,
    mut progress: ResMut<DiscoveryProgressLedger>,
    tags: Res<StartProfileKnowledgeTagsHandle>,
    ladder: Res<LadderConfigHandle>,
    config: Res<KnowledgeContactConfigHandle>,
    bands: Query<(&BandId, &PopulationCohort, Option<&LaborAllocation>)>,
    tiles: Query<&Tile>,
) {
    let ladder = ladder.get();
    let config = config.get();
    let lessons = TeachableLessons::resolve(&tags.get(), &ladder.knowledge)
        .expect("boot validates that every knowledge tag has a lesson cost");
    let mut contact_bands = Vec::new();
    let mut landings = Vec::new();
    for (band, cohort, allocation) in bands.iter() {
        contact_bands.push(ContactBand {
            band: *band,
            faction: cohort.faction,
            tile: tiles
                .get(cohort.current_tile)
                .ok()
                .map(|tile| tile.position),
        });
        if let Some(allocation) = allocation {
            landings.extend(trade_landings(*band, &allocation.last_transfer_crossings));
        }
    }
    let credits = contact_lessons(&ContactWorld {
        bands: &contact_bands,
        landings: &landings,
        connections: &connections,
        roads: &roads,
        progress: &progress,
        lessons: &lessons,
        knowledge: &ladder.knowledge,
        rates: &config.channel_rates,
    });
    apply_contact_lessons(&credits, &mut progress);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        connections::Sighting,
        connections_config::ConnectionsConfig,
        intensification::{LadderConfig, RungKey},
        knowledge_contact_config::KnowledgeContactConfig,
        routes::{road_rung_span, RoadKeeper, NEAR_ENOUGH_TO_KEEP},
        scalar::Scalar,
    };

    const PEOPLE_A: FactionId = FactionId(0);
    const PEOPLE_B: FactionId = FactionId(1);
    const A1: BandId = BandId(1);
    const A2: BandId = BandId(2);
    const B1: BandId = BandId(10);
    /// The taught discovery: `penning`, fully observable in the shipped tag table.
    const PENNING_TAG: &str = "penning";
    /// A second discovery, for the people the other way round.
    const WEAVING_TAG: &str = "weaving";
    /// Where every fixture band stands — the bands are all together, so only the ties differ.
    const SOMEWHERE: UVec2 = UVec2::new(3, 3);
    /// Contacts that pin a tie at full strength at the shipped gain (`0.25` × 4).
    const FULL_CONTACTS: u32 = 4;
    /// Contacts that leave a tie at half strength — so the tie term is not the identity.
    const HALF_CONTACTS: u32 = 2;
    /// Progress short of the completion bar: a people halfway to a discovery.
    const HALFWAY: f32 = 0.5;

    fn discovery(tag: &str) -> u32 {
        StartProfileKnowledgeTags::builtin()
            .get(tag)
            .expect("a shipped tag")
            .discovery_id()
    }

    struct Fixture {
        connections: ConnectionLedger,
        roads: RoadRegistry,
        progress: DiscoveryProgressLedger,
        lessons: TeachableLessons,
        ladder: LadderConfig,
        rates: ContactChannelRates,
        bands: Vec<ContactBand>,
        landings: Vec<TradeLanding>,
        /// The ledger's clock — each contact and each decay pass is its own turn.
        turn: u64,
    }

    impl Fixture {
        fn new() -> Self {
            let ladder = (*LadderConfig::builtin()).clone();
            let lessons =
                TeachableLessons::resolve(&StartProfileKnowledgeTags::builtin(), &ladder.knowledge)
                    .expect("the shipped tables are fully priced");
            let band = |band, faction| ContactBand {
                band,
                faction,
                tile: Some(SOMEWHERE),
            };
            Self {
                connections: ConnectionLedger::default(),
                roads: RoadRegistry::default(),
                progress: DiscoveryProgressLedger::default(),
                lessons,
                ladder,
                rates: KnowledgeContactConfig::builtin().channel_rates.clone(),
                bands: vec![band(A1, PEOPLE_A), band(A2, PEOPLE_A), band(B1, PEOPLE_B)],
                landings: Vec::new(),
                turn: 0,
            }
        }

        /// Raise `observer → subject` by `contacts` contacts, through the ledger's own path.
        fn tie(&mut self, observer: BandId, subject: BandId, contacts: u32) {
            let cfg = ConnectionsConfig::default();
            for _ in 0..contacts {
                self.connections.record_contact(
                    ConnectionKey::new(observer, subject),
                    &Sighting::new(SOMEWHERE, self.turn, ""),
                    self.turn,
                    &cfg,
                );
                self.turn += 1;
            }
        }

        /// Form `observer → subject` and let it decay to a **parked** tie — still an edge, at
        /// [`NO_TIE`]. Call before forming the ties that must stay live: decay drains every edge.
        fn park(&mut self, observer: BandId, subject: BandId) {
            let cfg = ConnectionsConfig::default();
            self.tie(observer, subject, 1);
            let key = ConnectionKey::new(observer, subject);
            while self
                .connections
                .get(&key)
                .expect("parked, not reaped")
                .strength
                > NO_TIE
            {
                self.connections.decay_all(self.turn, &cfg);
                self.turn += 1;
            }
        }

        fn strength(&self, observer: BandId, subject: BandId) -> f32 {
            self.connections
                .get(&ConnectionKey::new(observer, subject))
                .expect("the edge exists")
                .strength
                .to_f32()
        }

        fn knows_fully(&mut self, faction: FactionId, discovery: u32) {
            self.progress
                .add_progress(faction, discovery, Scalar::one());
        }

        /// Seat a dirt road — the cheapest BUILT rung — on the bands' tile, kept by `keeper` with
        /// its keeping met (no bill stamped reads as met). The position is written before the
        /// keeper: `set_position` releases a keeper on a road in the free floor.
        fn kept_road(&mut self, keeper: RoadKeeper) {
            let (base, width) =
                road_rung_span(RungKey::RouteDirtRoad, &self.ladder, NEAR_ENOUGH_TO_KEEP);
            let road = self.roads.road_or_trail(SOMEWHERE, &self.ladder);
            road.set_position(base + width, &self.ladder);
            road.take_keeper(keeper, NEAR_ENOUGH_TO_KEEP, &self.ladder);
            assert!(road.grants_sight(), "fixture: a built, kept road");
        }

        fn run(&self) -> ContactLessons {
            contact_lessons(&ContactWorld {
                bands: &self.bands,
                landings: &self.landings,
                connections: &self.connections,
                roads: &self.roads,
                progress: &self.progress,
                lessons: &self.lessons,
                knowledge: &self.ladder.knowledge,
                rates: &self.rates,
            })
        }

        /// `tie × rate × observability / lesson_cost`, from the shipped tables.
        fn expected(&self, tie: f32, rate: f32, tag: &str) -> f32 {
            let observability = StartProfileKnowledgeTags::builtin()
                .get(tag)
                .expect("a shipped tag")
                .observability();
            let cost = self
                .ladder
                .knowledge
                .lesson_cost(tag)
                .expect("a priced tag");
            tie * rate * observability / cost
        }
    }

    fn assert_close(got: f32, want: f32) {
        /// Well under one fixed-point step of the ledger.
        const TOLERANCE: f32 = 1e-7;
        assert!((got - want).abs() < TOLERANCE, "{got} vs {want}");
    }

    #[test]
    fn watching_credits_exactly_tie_times_rate_times_observability_over_cost() {
        let mut f = Fixture::new();
        f.tie(A1, B1, HALF_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        let lessons = f.run();
        let lesson = lessons[&(PEOPLE_A, penning)];
        let want = f.expected(f.strength(A1, B1), f.rates.watching, PENNING_TAG);
        assert_close(lesson.credit, want);
        assert_eq!(lesson.channel, ContactChannel::Watching);
        assert_eq!(lesson.subject_faction, PEOPLE_B);
        assert_eq!(lesson.subject_band, B1);

        let mut progress = f.progress.clone();
        apply_contact_lessons(&lessons, &mut progress);
        assert_eq!(
            progress.get_progress(PEOPLE_A, penning),
            scalar_from_f32(want),
            "the ledger rises by exactly the credit"
        );
        assert!(
            !lessons.keys().any(|(faction, _)| *faction == PEOPLE_B),
            "B learns nothing from A, who knows nothing B does not"
        );
    }

    #[test]
    fn one_direction_is_enough_and_only_the_observer_learns() {
        let mut f = Fixture::new();
        f.park(B1, A1);
        f.tie(A1, B1, FULL_CONTACTS);
        let (penning, weaving) = (discovery(PENNING_TAG), discovery(WEAVING_TAG));
        f.knows_fully(PEOPLE_B, penning);
        f.knows_fully(PEOPLE_A, weaving);
        let lessons = f.run();
        assert!(
            lessons.contains_key(&(PEOPLE_A, penning)),
            "A learns over A → B"
        );
        assert!(
            !lessons.contains_key(&(PEOPLE_B, weaving)),
            "B's parked edge teaches B nothing of what A knows"
        );
    }

    #[test]
    fn the_strongest_source_counts_never_the_sum() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        f.tie(A2, B1, HALF_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        let lesson = f.run()[&(PEOPLE_A, penning)];
        assert_close(
            lesson.credit,
            f.expected(f.strength(A1, B1), f.rates.watching, PENNING_TAG),
        );
    }

    #[test]
    fn two_bands_watching_at_the_same_strength_do_not_double() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        f.tie(A2, B1, FULL_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        let lessons = f.run();
        let lesson = lessons[&(PEOPLE_A, penning)];
        assert_close(
            lesson.credit,
            f.expected(f.strength(A1, B1), f.rates.watching, PENNING_TAG),
        );
        let mut progress = f.progress.clone();
        apply_contact_lessons(&lessons, &mut progress);
        assert_eq!(
            progress.get_progress(PEOPLE_A, penning),
            scalar_from_f32(lesson.credit),
            "one credit for the people, not one per band"
        );
    }

    #[test]
    fn a_landed_shipment_beats_watching_in_either_direction() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        // A1 SENT to B1: A1 is still the observer of B1 for the turn it landed.
        f.landings.push(TradeLanding {
            receiver: B1,
            sender: A1,
        });
        let lesson = f.run()[&(PEOPLE_A, penning)];
        assert_eq!(lesson.channel, ContactChannel::Trade);
        assert_close(
            lesson.credit,
            f.expected(f.strength(A1, B1), f.rates.trade, PENNING_TAG),
        );
    }

    #[test]
    fn standing_on_their_kept_road_beats_watching() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        f.kept_road(RoadKeeper {
            faction: PEOPLE_B,
            band: B1,
        });
        let lesson = f.run()[&(PEOPLE_A, penning)];
        assert_eq!(lesson.channel, ContactChannel::Road);
        assert_close(
            lesson.credit,
            f.expected(f.strength(A1, B1), f.rates.road, PENNING_TAG),
        );
    }

    #[test]
    fn a_road_in_shortfall_teaches_only_by_watching() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        let penning = discovery(PENNING_TAG);
        f.knows_fully(PEOPLE_B, penning);
        f.kept_road(RoadKeeper {
            faction: PEOPLE_B,
            band: B1,
        });
        // A bill stamped and nothing paid against it: the road `light_kept_routes` leaves dark.
        let road = f.roads.road_mut(SOMEWHERE).expect("the road");
        road.upkeep_demanded = Some(1.0);
        road.upkeep_supplied = 0.0;
        assert!(!road.grants_sight(), "fixture: the keeping is short");
        assert_eq!(
            f.run()[&(PEOPLE_A, penning)].channel,
            ContactChannel::Watching
        );
    }

    #[test]
    fn a_people_only_partly_knowing_teaches_nothing() {
        let mut f = Fixture::new();
        f.tie(A1, B1, FULL_CONTACTS);
        f.progress
            .add_progress(PEOPLE_B, discovery(PENNING_TAG), scalar_from_f32(HALFWAY));
        assert!(
            f.run().is_empty(),
            "halfway to Penning shows you nothing of it"
        );
    }

    #[test]
    fn a_parked_tie_teaches_nothing_on_any_channel() {
        let mut f = Fixture::new();
        f.park(A1, B1);
        f.knows_fully(PEOPLE_B, discovery(PENNING_TAG));
        f.landings.push(TradeLanding {
            receiver: A1,
            sender: B1,
        });
        assert!(f.run().is_empty());
    }

    #[test]
    fn a_band_of_your_own_people_teaches_nothing_without_a_branch() {
        let mut f = Fixture::new();
        f.tie(A1, A2, FULL_CONTACTS);
        f.knows_fully(PEOPLE_A, discovery(PENNING_TAG));
        assert!(
            f.run().is_empty(),
            "your own people already knows what your own people knows"
        );
    }

    #[test]
    fn every_knowledge_tag_must_be_priced() {
        let mut ladder = (*LadderConfig::builtin()).clone();
        ladder.knowledge.lesson_costs.remove("portable_forge");
        let err =
            TeachableLessons::resolve(&StartProfileKnowledgeTags::builtin(), &ladder.knowledge)
                .expect_err("an unpriced tag is a load failure, not a default");
        assert_eq!(
            err,
            KnowledgeContactError::UnpricedLesson {
                tag: "portable_forge".to_string()
            }
        );
    }

    #[test]
    fn a_missing_or_out_of_range_observability_is_rejected() {
        assert!(
            StartProfileKnowledgeTags::from_json_str(r#"{ "herding": { "discovery_id": 2004 } }"#)
                .is_err(),
            "observability is required, with no default"
        );
        assert!(StartProfileKnowledgeTags::from_json_str(
            r#"{ "herding": { "discovery_id": 2004, "observability": 1.5 } }"#
        )
        .is_err());
        assert!(
            StartProfileKnowledgeTags::from_json_str(
                r#"{ "herding": { "discovery_id": 2004, "observability": 0.5, "fidelity": 0.9 } }"#
            )
            .is_err(),
            "the retired fidelity key must not parse"
        );
    }
}
