//! **A tribe is earned.** Every pair of bands that are *local* to one another (one people, one
//! supply network that is not 0, one belief-anchor tile) gets a **closeness score** each turn: the
//! weighted sum of seven inputs in `[0, 1]` ([`CloseInputs`], [`closeness`]) -- shared origin,
//! culture likeness, the connection tie, a shared history built by being local and faded by being
//! apart, belief on the shared urn, a kept monument on it, and acting as a tribe. A pair is *close* at
//! `leadership_config.json -> tribe_score_threshold`. A tribe-candidate is a connected component
//! of close pairs within one (people, network, anchor) set, of [`MIN_TRIBE_BANDS`] or more bands,
//! at least one of which has a chief of its own.
//!
//! The [`TribeRegistry`] keeps a tribe's identity across turns (see [`resolve_matches`]), promotes
//! the largest chiefed member's own [`BandLeader`] to the tribe's leader, and tells the player when
//! a tribe forms, gains or loses a band, or dissolves.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    belief::BeliefRegistry,
    belief_config::{BeliefConfig, BeliefConfigHandle},
    components::{BandId, BandName, PopulationCohort, ResidentBand},
    connections::ConnectionLedger,
    culture::{max_axis_gap, CultureLayer, CultureManager, CultureOwner, CULTURE_TRAIT_SPAN},
    leadership::{BandLeader, BandLeadership, LeaderKind},
    leadership_config::{LeadershipConfig, LeadershipConfigHandle},
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    supply::SupplyNetworkMembership,
    systems::band_label,
    tile_improvement::TileImprovementRegistry,
    wellbeing_config::WellbeingConfigHandle,
};

/// How many bands make "several": the definition of a tribe, not a tuning lever.
pub const MIN_TRIBE_BANDS: usize = 2;

/// The full width of a culture trait axis (`-span..=span`): the denominator that turns the one
/// culture-distance metric ([`max_axis_gap`]) into a likeness in `[0, 1]`.
const CULTURE_AXIS_RANGE: f32 = 2.0 * CULTURE_TRAIT_SPAN;

/// A tribe's durable identity. Allocated from [`TribeRegistry`]; never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TribeId(pub u64);

/// What kind of leadership a tribe follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TribeLeaderKind {
    /// A member band's chief, promoted.
    ParamountChief,
    /// A member band's elders' council, promoted; its speaker is the named person.
    CouncilOfChiefs,
}

impl TribeLeaderKind {
    /// The tribe-level kind a member band's own leader kind is promoted to.
    pub fn promoted_from(kind: LeaderKind) -> Self {
        match kind {
            LeaderKind::Chief => TribeLeaderKind::ParamountChief,
            LeaderKind::ElderCouncil => TribeLeaderKind::CouncilOfChiefs,
        }
    }

    /// The `kind=` token a feed line carries.
    pub fn as_str(self) -> &'static str {
        match self {
            TribeLeaderKind::ParamountChief => "paramount_chief",
            TribeLeaderKind::CouncilOfChiefs => "council_of_chiefs",
        }
    }
}

/// The person a tribe follows: a member band's own leader, promoted. Re-promoted from another
/// member if `band` leaves the tribe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TribeLeader {
    pub name: String,
    pub kind: TribeLeaderKind,
    /// The turn this person became the tribe's leader.
    pub since_turn: u64,
    /// The member band whose leader this is.
    pub band: BandId,
}

impl TribeLeader {
    fn promote(from: BandId, leader: &BandLeader, turn: u64) -> Self {
        Self {
            name: leader.name.clone(),
            kind: TribeLeaderKind::promoted_from(leader.kind),
            since_turn: turn,
            band: from,
        }
    }
}

/// One tribe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tribe {
    pub id: TribeId,
    pub faction: FactionId,
    /// "<largest member band's name> tribe", fixed at formation.
    pub name: String,
    pub members: BTreeSet<BandId>,
    pub leader: TribeLeader,
    pub since_turn: u64,
}

/// An unordered pair of bands, lower id first: the key a shared history is kept under.
pub type BandPair = (BandId, BandId);

fn band_pair(a: BandId, b: BandId) -> BandPair {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

use sim_schema::{
    TRIBE_STANDING_ALONE, TRIBE_STANDING_LOCAL, TRIBE_STANDING_NO_NETWORK, TRIBE_STANDING_NO_URN,
};

/// Why a band is or is not in reach of a tribe this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TribeStanding {
    Local,
    NoUrn,
    NoNetwork,
    Alone,
}

impl TribeStanding {
    /// The wire byte (`TRIBE_STANDING_*`).
    pub fn wire(self) -> u8 {
        match self {
            TribeStanding::Local => TRIBE_STANDING_LOCAL,
            TribeStanding::NoUrn => TRIBE_STANDING_NO_URN,
            TribeStanding::NoNetwork => TRIBE_STANDING_NO_NETWORK,
            TribeStanding::Alone => TRIBE_STANDING_ALONE,
        }
    }
}

/// **Every tribe, every pair's shared history, and how many tribes each people has formed.**
/// Checkpoint state: identity, the leader and the histories persist across turns and nothing
/// rebuilds them. Keyed by [`BandId`], never `Entity`.
#[derive(Resource, Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TribeRegistry {
    tribes: BTreeMap<TribeId, Tribe>,
    /// Each pair's shared history in `(0, 1]`: built while the pair is local and at least one of
    /// the two has a leader, held while it is local with none, faded while it is not local, never
    /// reset. A pair absent has none.
    history: BTreeMap<BandPair, f64>,
    next_id: u64,
    /// Cumulative formations per people, the total the Telling diffs into
    /// `tribes.formed_this_turn`.
    formed: BTreeMap<FactionId, u64>,
    /// This turn's scoring, rebuilt from scratch each turn: the inputs of every local pair. Kept
    /// here (checkpoint state) so a snapshot published right after a load still shows progress.
    scores: BTreeMap<BandPair, CloseInputs>,
    /// Each resident band's standing this turn, rebuilt each turn.
    standing: BTreeMap<BandId, TribeStanding>,
}

impl TribeRegistry {
    /// Every local pair `band` is in, as `(partner, inputs)`, partner id ascending.
    pub fn pairs_of(&self, band: BandId) -> impl Iterator<Item = (BandId, &CloseInputs)> {
        let mut found: Vec<(BandId, &CloseInputs)> = self
            .scores
            .iter()
            .filter_map(|(pair, score)| {
                if pair.0 == band {
                    Some((pair.1, score))
                } else if pair.1 == band {
                    Some((pair.0, score))
                } else {
                    None
                }
            })
            .collect();
        found.sort_by_key(|(partner, _)| *partner);
        found.into_iter()
    }

    /// The band's standing this turn, if it was resident when last scored.
    pub fn standing_of(&self, band: BandId) -> Option<TribeStanding> {
        self.standing.get(&band).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tribe> {
        self.tribes.values()
    }

    pub fn get(&self, id: TribeId) -> Option<&Tribe> {
        self.tribes.get(&id)
    }

    pub fn len(&self) -> usize {
        self.tribes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tribes.is_empty()
    }

    /// The tribe `band` belongs to, if any.
    pub fn tribe_of(&self, band: BandId) -> Option<&Tribe> {
        self.tribes.values().find(|t| t.members.contains(&band))
    }

    /// The shared history of a pair of bands (`0.0` with none).
    pub fn shared_history(&self, a: BandId, b: BandId) -> f64 {
        self.history.get(&band_pair(a, b)).copied().unwrap_or(0.0)
    }

    /// Every tribe `faction` has formed, cumulative.
    pub fn formed_by(&self, faction: FactionId) -> u64 {
        self.formed.get(&faction).copied().unwrap_or(0)
    }

    fn allocate(&mut self) -> TribeId {
        let id = TribeId(self.next_id);
        self.next_id += 1;
        id
    }
}

/// One set of bands of one people: a local bucket or a tribe candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandGroup {
    pub faction: FactionId,
    pub members: BTreeSet<BandId>,
}

impl BandGroup {
    fn lowest(&self) -> Option<BandId> {
        self.members.iter().next().copied()
    }
}

/// **Match last turn's tribes to this turn's groups**, returning `tribe -> index into groups`.
/// A tribe absent from the result dissolves; a group no tribe maps to forms a new tribe.
///
/// 1. Each tribe (ascending id) picks its best group of its own people: the one holding the most
///    of its previous members; ties go to the group whose lowest band id is lowest. A tribe none
///    of whose members is in any group has no pick.
/// 2. Where several tribes picked one group, the one with more previous members in it keeps it
///    (ties to the lower tribe id). The losers dissolve -- they do not try a second group.
pub fn resolve_matches(
    tribes: &BTreeMap<TribeId, Tribe>,
    groups: &[BandGroup],
) -> BTreeMap<TribeId, usize> {
    // group index -> (overlap, tribe) of the best claimant so far.
    let mut winner: BTreeMap<usize, (usize, TribeId)> = BTreeMap::new();
    for (id, tribe) in tribes {
        let best = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.faction == tribe.faction)
            .map(|(index, g)| (tribe.members.intersection(&g.members).count(), index, g))
            .filter(|(overlap, _, _)| *overlap > 0)
            .min_by_key(|(overlap, _, g)| (std::cmp::Reverse(*overlap), g.lowest()));
        let Some((overlap, index, _)) = best else {
            continue;
        };
        // Tribes arrive in ascending id, so on an equal overlap the earlier claimant stands.
        match winner.get(&index) {
            Some((held, _)) if *held >= overlap => {}
            _ => {
                winner.insert(index, (overlap, *id));
            }
        }
    }
    winner
        .into_iter()
        .map(|(index, (_, tribe))| (tribe, index))
        .collect()
}

/// The sets of bands that are local to one another: same people, same network (not 0), same
/// belief-anchor tile. Only sets of [`MIN_TRIBE_BANDS`] or more are returned.
pub fn local_buckets(
    rows: impl IntoIterator<Item = (BandId, FactionId, u32, Option<UVec2>)>,
) -> Vec<BandGroup> {
    let mut keyed: BTreeMap<(FactionId, u32, (u32, u32)), BTreeSet<BandId>> = BTreeMap::new();
    for (band, faction, network, anchor) in rows {
        let Some(anchor) = anchor else { continue };
        if network == 0 {
            continue;
        }
        keyed
            .entry((faction, network, (anchor.x, anchor.y)))
            .or_default()
            .insert(band);
    }
    keyed
        .into_iter()
        .filter(|(_, members)| members.len() >= MIN_TRIBE_BANDS)
        .map(|((faction, _, _), members)| BandGroup { faction, members })
        .collect()
}

/// Every unordered pair inside the buckets: the pairs that are local this turn.
fn local_pairs(buckets: &[BandGroup]) -> BTreeSet<BandPair> {
    let mut pairs = BTreeSet::new();
    for bucket in buckets {
        let members: Vec<BandId> = bucket.members.iter().copied().collect();
        for (i, a) in members.iter().enumerate() {
            for b in &members[i + 1..] {
                pairs.insert((*a, *b));
            }
        }
    }
    pairs
}

/// **This turn's tribe candidates**: within each bucket, the connected components of the close
/// pairs that hold [`MIN_TRIBE_BANDS`] or more bands, at least one of them chiefed.
pub fn close_groups(
    buckets: &[BandGroup],
    close: impl Fn(BandId, BandId) -> bool,
    chiefed: impl Fn(BandId) -> bool,
) -> Vec<BandGroup> {
    let mut groups = Vec::new();
    for bucket in buckets {
        let mut unvisited: BTreeSet<BandId> = bucket.members.clone();
        while let Some(&start) = unvisited.iter().next() {
            let mut component = BTreeSet::from([start]);
            let mut frontier = vec![start];
            unvisited.remove(&start);
            while let Some(band) = frontier.pop() {
                let linked: Vec<BandId> = unvisited
                    .iter()
                    .copied()
                    .filter(|other| close(band, *other))
                    .collect();
                for other in linked {
                    unvisited.remove(&other);
                    component.insert(other);
                    frontier.push(other);
                }
            }
            if component.len() >= MIN_TRIBE_BANDS && component.iter().any(|b| chiefed(*b)) {
                groups.push(BandGroup {
                    faction: bucket.faction,
                    members: component,
                });
            }
        }
    }
    groups
}

/// How alike two cultures are, in `[0, 1]` (1 identical, 0 opposite): one minus the one
/// culture-distance metric ([`max_axis_gap`], the largest per-axis gap) over the full axis range.
pub fn likeness(a: &CultureLayer, b: &CultureLayer) -> f64 {
    let gap = max_axis_gap(a.traits.values(), b.traits.values()).to_f32();
    f64::from((1.0 - gap / CULTURE_AXIS_RANGE).clamp(0.0, 1.0))
}

/// The seven inputs to a pair's closeness score, each in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct CloseInputs {
    /// 1 when the two bands' founding lines share an origin band, else 0.
    pub origin: f64,
    /// How alike their own culture layers are ([`likeness`]); 0 when either has no layer.
    pub likeness: f64,
    /// The stronger direction of the connection tie between them (`FULL_TIE` = 1).
    pub tie: f64,
    /// Their shared history ([`TribeRegistry::shared_history`]).
    pub history: f64,
    /// The anchor weight of the belief standing on their shared urn; 0 when not local.
    pub belief: f64,
    /// 1 while a monument stands kept on their shared urn ([`kept_monument`]), else 0.
    pub monument: f64,
    /// [`acting_as_a_tribe`].
    pub acting: f64,
}

/// **A pair's closeness score**: the weighted sum of its inputs. The one pure place the weights are
/// applied, so the opening numbers can be pinned without a world.
pub fn closeness(config: &LeadershipConfig, inputs: &CloseInputs) -> f64 {
    config.tribe_weight_origin * inputs.origin
        + config.tribe_weight_likeness * inputs.likeness
        + config.tribe_weight_tie * inputs.tie
        + config.tribe_weight_history * inputs.history
        + config.tribe_weight_belief * inputs.belief
        + config.tribe_weight_monument * inputs.monument
        + config.tribe_weight_acting * inputs.acting
}

/// The reading of "acting as a tribe" a pair earns from doing things together as one tribe, in
/// `[0, 1]`. It applies only to a pair already in the same tribe (`same_tribe`); a pair outside one
/// reads 0. Nothing feeds it yet: the tribe-level orders (#862, #864) and works the tribe shares
/// are what will, and each adds its measure here.
pub fn acting_as_a_tribe(same_tribe: bool) -> f64 {
    if !same_tribe {
        return NOT_ACTING;
    }
    NOT_ACTING
}

/// The reading while no tribe-level act exists to measure.
const NOT_ACTING: f64 = 0.0;

/// Whether two bands' founding lines share an origin band: both descend from one starting band.
fn shares_origin(a: &BTreeSet<u64>, b: &BTreeSet<u64>) -> bool {
    a.intersection(b).next().is_some()
}

/// What the system reads of one resident band.
struct BandFacts {
    size: u32,
    name: String,
    /// The band's belief anchor tile.
    anchor: Option<UVec2>,
    /// The origin bands of the band's founding lines.
    origins: BTreeSet<u64>,
    /// The band's own leader, if it has accepted one.
    leader: Option<BandLeader>,
}

/// The largest of `bands` (most people; ties to the lowest id) among those still resident.
fn largest_band<'a>(
    bands: impl IntoIterator<Item = &'a BandId>,
    facts: &BTreeMap<BandId, BandFacts>,
) -> Option<BandId> {
    bands
        .into_iter()
        .filter_map(|band| facts.get(band).map(|f| (f.size, *band)))
        .min_by_key(|(size, band)| (std::cmp::Reverse(*size), *band))
        .map(|(_, band)| band)
}

/// The largest chiefed band among `bands` (most people; ties to the lowest id), with its leader.
fn largest_chiefed<'a>(
    bands: impl IntoIterator<Item = &'a BandId>,
    facts: &'a BTreeMap<BandId, BandFacts>,
) -> Option<(BandId, &'a BandLeader)> {
    bands
        .into_iter()
        .filter_map(|band| {
            let f = facts.get(band)?;
            Some((f.size, *band, f.leader.as_ref()?))
        })
        .min_by_key(|(size, band, _)| (std::cmp::Reverse(*size), *band))
        .map(|(_, band, leader)| (band, leader))
}

/// The feed line for a tribe's formation, naming its leader the way a band leader's line does.
fn tribe_formed_label(tribe: &Tribe) -> String {
    match tribe.leader.kind {
        TribeLeaderKind::ParamountChief => {
            format!(
                "{} forms; {} is paramount chief",
                tribe.name, tribe.leader.name
            )
        }
        TribeLeaderKind::CouncilOfChiefs => format!(
            "{} forms; a council of chiefs gathers, {} speaks for it",
            tribe.name, tribe.leader.name
        ),
    }
}

fn tribe_entry(
    tick: u64,
    kind: CommandEventKind,
    tribe: &Tribe,
    band: BandId,
    label: String,
    detail: String,
) -> CommandEventEntry {
    CommandEventEntry::new(tick, kind, tribe.faction, label, Some(detail)).with_band(band)
}

/// The inputs of a pair that need no shared urn: origin, likeness, tie and history.
fn pair_inputs(
    facts: &BTreeMap<BandId, BandFacts>,
    culture: &CultureManager,
    ledger: &ConnectionLedger,
    registry: &TribeRegistry,
    a: BandId,
    b: BandId,
) -> CloseInputs {
    let layers = (
        culture.band_layer_by_owner(CultureOwner::from_band(a)),
        culture.band_layer_by_owner(CultureOwner::from_band(b)),
    );
    let tie = ledger
        .edges_between(a, b)
        .iter()
        .flatten()
        .map(|connection| f64::from(connection.strength.to_f32()))
        .fold(0.0, f64::max);
    CloseInputs {
        origin: f64::from(u8::from(shares_origin(
            &facts[&a].origins,
            &facts[&b].origins,
        ))),
        likeness: match layers {
            (Some(la), Some(lb)) => likeness(la, lb),
            _ => 0.0,
        },
        tie,
        history: registry.shared_history(a, b),
        ..CloseInputs::default()
    }
}

/// The belief anchor a bucket's bands share.
fn bucket_anchor(bucket: &BandGroup, facts: &BTreeMap<BandId, BandFacts>) -> Option<UVec2> {
    bucket
        .members
        .iter()
        .find_map(|member| facts.get(member).and_then(|f| f.anchor))
}

/// The anchor weight of the belief standing on a shared urn: the weight belief already uses
/// (`CultureConfig::anchor_weight`), read at the anchor.
fn anchor_weight(
    anchor: Option<UVec2>,
    belief: &BeliefRegistry,
    wellbeing: &crate::wellbeing_config::WellbeingConfig,
) -> f64 {
    anchor.map_or(0.0, |anchor| {
        f64::from(wellbeing.culture.anchor_weight(belief.get(anchor)))
    })
}

/// **A kept monument on the shared urn**: 1 while the anchor tile's improvement HOLDS a rung that
/// `belief_config.json -> improvement_belief` lists, else 0. The list is the one that already says
/// which rungs make a place a place of belief (the monument), so the engine names no rung here
/// either. Held, not built: a half-raised monument reads 0, and an unkept one that has slipped back
/// to its root reads 0 the turn it slips.
pub fn kept_monument(
    anchor: Option<UVec2>,
    improvements: &TileImprovementRegistry,
    belief: &BeliefConfig,
) -> f64 {
    let Some(anchor) = anchor else {
        return NO_MONUMENT;
    };
    let kept = belief.improvement_belief.iter().any(|source| {
        improvements
            .get(anchor, source.rung.branch)
            .is_some_and(|improvement| improvement.held_rung() == source.rung)
    });
    if kept {
        MONUMENT_KEPT
    } else {
        NO_MONUMENT
    }
}

/// The monument input's two readings.
const MONUMENT_KEPT: f64 = 1.0;
const NO_MONUMENT: f64 = 0.0;

/// What the tribe system reads of each resident band: entity, id, people, name and leadership.
type TribeBandParts = (
    Entity,
    &'static BandId,
    &'static PopulationCohort,
    Option<&'static BandName>,
    Option<&'static BandLeadership>,
);

/// **Score the pairs, then keep the tribes: match, form, change membership, dissolve.** Runs at the
/// end of the Population stage, after the belief anchors, the supply networks and the leaders of
/// the turn are settled. Bands and tribes are visited in id order, so identity and the feed are
/// deterministic.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_tribes(
    tick: Res<SimulationTick>,
    config: Res<LeadershipConfigHandle>,
    culture: Res<CultureManager>,
    ledger: Res<ConnectionLedger>,
    belief: Res<BeliefRegistry>,
    belief_config: Res<BeliefConfigHandle>,
    improvements: Res<TileImprovementRegistry>,
    wellbeing: Res<WellbeingConfigHandle>,
    supply: Res<SupplyNetworkMembership>,
    mut registry: ResMut<TribeRegistry>,
    mut event_log: ResMut<CommandEventLog>,
    bands: Query<TribeBandParts, With<ResidentBand>>,
) {
    let turn = tick.0;
    let config = config.get();
    let wellbeing = wellbeing.get();
    let belief_config = belief_config.get();
    let mut facts: BTreeMap<BandId, BandFacts> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut networks: BTreeMap<BandId, u32> = BTreeMap::new();
    for (entity, band, cohort, name, leadership) in bands.iter() {
        networks.insert(*band, supply.network_of(entity));
        facts.insert(
            *band,
            BandFacts {
                size: cohort.size,
                name: name
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|| band_label(*band)),
                anchor: cohort.belief_anchor,
                origins: cohort
                    .founding_lines
                    .iter()
                    .map(|line| line.origin_band)
                    .collect(),
                leader: leadership.and_then(|l| l.leader.clone()),
            },
        );
        rows.push((
            *band,
            cohort.faction,
            supply.network_of(entity),
            cohort.belief_anchor,
        ));
    }

    // 1. Shared history: built a step by being local with a chief on at least one side, held while
    //    local with none, faded a step by not being local; never reset. An exhausted entry or one
    //    naming a band that is gone is pruned.
    let buckets = local_buckets(rows);
    let local = local_pairs(&buckets);
    let build = 1.0 / config.tribe_history_build_turns;
    let fade = 1.0 / config.tribe_history_fade_turns;
    let has_leader = |band: &BandId| facts.get(band).is_some_and(|f| f.leader.is_some());
    for pair in &local {
        if has_leader(&pair.0) || has_leader(&pair.1) {
            let entry = registry.history.entry(*pair).or_insert(0.0);
            *entry = (*entry + build).min(1.0);
        }
    }
    registry.history.retain(|pair, value| {
        if !local.contains(pair) {
            *value = (*value - fade).max(0.0);
        }
        *value > 0.0 && facts.contains_key(&pair.0) && facts.contains_key(&pair.1)
    });

    // 2. Closeness of every local pair, then the candidate groups: close components with a chief.
    let mut close_pairs: BTreeSet<BandPair> = BTreeSet::new();
    let mut scores: BTreeMap<BandPair, CloseInputs> = BTreeMap::new();
    for bucket in &buckets {
        let anchor = bucket_anchor(bucket, &facts);
        let urn_belief = anchor_weight(anchor, &belief, &wellbeing);
        let monument = kept_monument(anchor, &improvements, &belief_config);
        let members: Vec<BandId> = bucket.members.iter().copied().collect();
        for (i, a) in members.iter().enumerate() {
            for b in &members[i + 1..] {
                let same_tribe = registry
                    .tribe_of(*a)
                    .is_some_and(|tribe| tribe.members.contains(b));
                let mut inputs = pair_inputs(&facts, &culture, &ledger, &registry, *a, *b);
                inputs.belief = urn_belief;
                inputs.monument = monument;
                inputs.acting = acting_as_a_tribe(same_tribe);
                scores.insert((*a, *b), inputs);
                if closeness(&config, &inputs) >= config.tribe_score_threshold {
                    close_pairs.insert((*a, *b));
                }
            }
        }
    }
    let local_bands: BTreeSet<BandId> = buckets
        .iter()
        .flat_map(|bucket| bucket.members.iter().copied())
        .collect();
    let standing = facts
        .iter()
        .map(|(band, f)| {
            let standing = if local_bands.contains(band) {
                TribeStanding::Local
            } else if f.anchor.is_none() {
                TribeStanding::NoUrn
            } else if networks.get(band).copied().unwrap_or(0) == 0 {
                TribeStanding::NoNetwork
            } else {
                TribeStanding::Alone
            };
            (*band, standing)
        })
        .collect();
    registry.scores = scores;
    registry.standing = standing;
    let groups = close_groups(
        &buckets,
        |a, b| close_pairs.contains(&band_pair(a, b)),
        |band| facts.get(&band).is_some_and(|f| f.leader.is_some()),
    );
    let matches = resolve_matches(&registry.tribes, &groups);
    let band_exists = |band: BandId| facts.contains_key(&band);

    // 3. Existing tribes: update the matched, dissolve the rest.
    let previous: Vec<TribeId> = registry.tribes.keys().copied().collect();
    for id in previous {
        let Some(&index) = matches.get(&id) else {
            let tribe = registry.tribes.remove(&id).expect("listed above");
            if let Some(band) = largest_band(tribe.members.iter(), &facts) {
                event_log.push(tribe_entry(
                    turn,
                    CommandEventKind::TribeDissolved,
                    &tribe,
                    band,
                    format!("The {} breaks up", tribe.name),
                    format!("tribe={}", tribe.id.0),
                ));
            }
            continue;
        };
        let group = &groups[index];
        let tribe = registry.tribes.get_mut(&id).expect("listed above");
        let joined: Vec<BandId> = group.members.difference(&tribe.members).copied().collect();
        let left: Vec<BandId> = tribe.members.difference(&group.members).copied().collect();
        tribe.members = group.members.clone();
        // The leader's band left: promote the largest chiefed member now standing, quietly.
        if !tribe.members.contains(&tribe.leader.band) {
            if let Some((band, leader)) = largest_chiefed(tribe.members.iter(), &facts) {
                tribe.leader = TribeLeader::promote(band, leader, turn);
            }
        }
        let tribe = tribe.clone();
        for band in joined {
            event_log.push(tribe_entry(
                turn,
                CommandEventKind::TribeBandJoined,
                &tribe,
                band,
                format!("{} joins the {}", band_label(band), tribe.name),
                format!("tribe={} band={}", tribe.id.0, band.0),
            ));
        }
        for band in left {
            // A band that no longer exists did not leave; it is gone.
            if !band_exists(band) {
                continue;
            }
            event_log.push(tribe_entry(
                turn,
                CommandEventKind::TribeBandLeft,
                &tribe,
                band,
                format!("{} leaves the {}", band_label(band), tribe.name),
                format!("tribe={} band={}", tribe.id.0, band.0),
            ));
        }
    }

    // 4. New tribes: every group no tribe kept.
    let claimed: BTreeSet<usize> = matches.values().copied().collect();
    for (index, group) in groups.iter().enumerate() {
        if claimed.contains(&index) {
            continue;
        }
        let Some(largest) = largest_band(group.members.iter(), &facts) else {
            continue;
        };
        let Some((leader_band, leader)) = largest_chiefed(group.members.iter(), &facts) else {
            continue;
        };
        let id = registry.allocate();
        let tribe = Tribe {
            id,
            faction: group.faction,
            name: format!("{} tribe", facts[&largest].name),
            members: group.members.clone(),
            leader: TribeLeader::promote(leader_band, leader, turn),
            since_turn: turn,
        };
        *registry.formed.entry(group.faction).or_insert(0) += 1;
        event_log.push(tribe_entry(
            turn,
            CommandEventKind::TribeFormed,
            &tribe,
            largest,
            tribe_formed_label(&tribe),
            format!(
                "tribe={} kind={} leader={}",
                tribe.id.0,
                tribe.leader.kind.as_str(),
                tribe.leader.name
            ),
        ));
        registry.tribes.insert(id, tribe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(id: u64) -> BandId {
        BandId(id)
    }

    fn group(ids: &[u64]) -> BandGroup {
        BandGroup {
            faction: FactionId(0),
            members: ids.iter().copied().map(band).collect(),
        }
    }

    fn tribe(id: u64, ids: &[u64]) -> (TribeId, Tribe) {
        (
            TribeId(id),
            Tribe {
                id: TribeId(id),
                faction: FactionId(0),
                name: format!("T{id}"),
                members: ids.iter().copied().map(band).collect(),
                leader: TribeLeader {
                    name: "Orrin".into(),
                    kind: TribeLeaderKind::CouncilOfChiefs,
                    since_turn: 1,
                    band: band(ids[0]),
                },
                since_turn: 1,
            },
        )
    }

    #[test]
    fn a_bands_leader_kind_is_promoted_one_to_one() {
        assert_eq!(
            TribeLeaderKind::promoted_from(LeaderKind::Chief),
            TribeLeaderKind::ParamountChief
        );
        assert_eq!(
            TribeLeaderKind::promoted_from(LeaderKind::ElderCouncil),
            TribeLeaderKind::CouncilOfChiefs
        );
    }

    #[test]
    fn a_split_keeps_the_tribe_on_the_majority_side() {
        let tribes = BTreeMap::from([tribe(0, &[1, 2, 3, 4, 5])]);
        let groups = [group(&[4, 5]), group(&[1, 2, 3])];
        let matches = resolve_matches(&tribes, &groups);
        assert_eq!(matches.get(&TribeId(0)), Some(&1));
    }

    #[test]
    fn an_even_split_goes_to_the_side_with_the_lowest_band() {
        let tribes = BTreeMap::from([tribe(0, &[1, 2, 3, 4])]);
        let groups = [group(&[3, 4]), group(&[1, 2])];
        assert_eq!(resolve_matches(&tribes, &groups).get(&TribeId(0)), Some(&1));
    }

    #[test]
    fn a_merge_goes_to_the_tribe_holding_more_and_the_other_dissolves() {
        let tribes = BTreeMap::from([tribe(0, &[1, 2]), tribe(1, &[3, 4, 5])]);
        let groups = [group(&[1, 2, 3, 4, 5])];
        let matches = resolve_matches(&tribes, &groups);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches.get(&TribeId(1)), Some(&0));
    }

    #[test]
    fn a_merge_of_equals_goes_to_the_older_tribe() {
        let tribes = BTreeMap::from([tribe(0, &[1, 2]), tribe(1, &[3, 4])]);
        let groups = [group(&[1, 2, 3, 4])];
        let matches = resolve_matches(&tribes, &groups);
        assert_eq!(matches.get(&TribeId(0)), Some(&0));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn buckets_skip_network_zero_a_missing_anchor_and_a_lone_band() {
        let at = |x, y| Some(UVec2::new(x, y));
        let f = FactionId(0);
        let buckets = local_buckets([
            (band(1), f, 1, at(2, 2)),
            (band(2), f, 1, at(2, 2)),
            (band(3), f, 1, at(5, 5)),
            (band(4), f, 0, at(2, 2)),
            (band(5), f, 1, None),
        ]);
        assert_eq!(buckets, vec![group(&[1, 2])]);
    }

    #[test]
    fn candidates_are_close_components_with_a_chief_among_them() {
        let buckets = [group(&[1, 2, 3, 4, 5])];
        // 1-2-3 close in a chain, 4-5 close; only band 4 is chiefed.
        let bonded = |a: BandId, b: BandId| {
            matches!(
                band_pair(a, b),
                (BandId(1), BandId(2)) | (BandId(2), BandId(3)) | (BandId(4), BandId(5))
            )
        };
        let chiefed = |b: BandId| b == band(4);
        assert_eq!(
            close_groups(&buckets, bonded, chiefed),
            vec![group(&[4, 5])],
            "the chiefless chain is no tribe"
        );
        assert_eq!(
            close_groups(&buckets, bonded, |b| b == band(3)),
            vec![group(&[1, 2, 3])],
        );
    }

    /// The shipped weights, so the opening numbers are pinned as they ship.
    fn shipped() -> LeadershipConfig {
        LeadershipConfig::default()
    }

    fn inputs(origin: f64, likeness: f64, tie: f64, history: f64, belief: f64) -> CloseInputs {
        CloseInputs {
            origin,
            likeness,
            tie,
            history,
            belief,
            monument: 0.0,
            acting: 0.0,
        }
    }

    #[test]
    fn a_fresh_split_scores_under_the_line_and_history_carries_it_over() {
        let config = shipped();
        // Shared origin 3 + identical culture 3 + full tie 2, no history yet.
        assert_eq!(closeness(&config, &inputs(1.0, 1.0, 1.0, 0.0, 0.0)), 8.0);
        // A little belief on the urn still leaves it short.
        assert!(
            closeness(&config, &inputs(1.0, 1.0, 1.0, 0.0, 0.5)) < config.tribe_score_threshold
        );
        // A fifth of the history (5 local turns) closes the gap exactly.
        assert_eq!(closeness(&config, &inputs(1.0, 1.0, 1.0, 0.2, 0.0)), 9.0);
    }

    #[test]
    fn likeness_and_tie_each_move_the_score_by_their_weight() {
        let config = shipped();
        let base = closeness(&config, &inputs(1.0, 1.0, 1.0, 0.0, 0.0));
        assert_eq!(
            closeness(&config, &inputs(1.0, 0.5, 1.0, 0.0, 0.0)),
            base - 1.5
        );
        assert_eq!(
            closeness(&config, &inputs(1.0, 1.0, 0.5, 0.0, 0.0)),
            base - 1.0
        );
        assert_eq!(
            closeness(&config, &inputs(0.0, 1.0, 1.0, 0.0, 0.0)),
            base - 3.0
        );
        assert_eq!(closeness(&config, &inputs(1.0, 1.0, 1.0, 1.0, 1.0)), 14.0);
    }

    #[test]
    fn a_kept_monument_closes_a_fresh_split_on_its_own() {
        let config = shipped();
        let fresh = inputs(1.0, 1.0, 1.0, 0.0, 0.0);
        let with_monument = CloseInputs {
            monument: 1.0,
            ..fresh
        };
        // 8 + monument 2 = 10: over the line with no history and no belief.
        assert_eq!(closeness(&config, &with_monument), 10.0);
        assert!(closeness(&config, &with_monument) >= config.tribe_score_threshold);
        // Unrelated, alike, no tie: 3 + full history 5 = 8 never closes; a monument makes it 10.
        let strangers = CloseInputs {
            monument: 1.0,
            ..inputs(0.0, 1.0, 0.0, 1.0, 0.0)
        };
        assert_eq!(closeness(&config, &strangers), 10.0);
    }

    #[test]
    fn acting_as_a_tribe_reads_nothing_until_something_feeds_it() {
        assert_eq!(acting_as_a_tribe(false), 0.0);
        assert_eq!(acting_as_a_tribe(true), 0.0);
    }

    #[test]
    fn origins_are_shared_when_any_origin_band_is_common() {
        let a = BTreeSet::from([1, 2]);
        assert!(shares_origin(&a, &BTreeSet::from([2, 9])));
        assert!(!shares_origin(&a, &BTreeSet::from([3])));
    }
}
