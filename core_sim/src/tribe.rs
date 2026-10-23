//! **A tribe forms when linked bands share a gathering place** (`docs/plan_labor_at_scale.md` ->
//! "Leaders: four levels of orders", level 2). Rationale: `.claude/rules/core_sim/leadership.md`.
//!
//! Each turn [`advance_tribes`] groups the resident bands by `(people, supply network, belief
//! anchor tile)` -- bands that pool with one another AND remember the same place -- and every
//! group of [`MIN_TRIBE_BANDS`] or more is a tribe. The [`TribeRegistry`] keeps a tribe's identity
//! across turns (see [`resolve_matches`]), gives a new tribe its [`TribeLeader`] and name once, and
//! tells the player when a tribe forms, gains or loses a band, or dissolves.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandId, BandName, PopulationCohort, ResidentBand},
    culture::{CultureManager, CultureOwner, CultureTraitAxis},
    leader_names::LeaderNameCatalogHandle,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, SimulationConfig, SimulationTick,
    },
    supply::SupplyNetworkMembership,
    systems::band_label,
};

/// How many bands make "several": the definition of a tribe, not a tuning lever.
pub const MIN_TRIBE_BANDS: usize = 2;

/// The axis value below which a tribe's leader is a paramount chief -- the same cut
/// `LeaderKind::for_axis_value` makes for a band (negative = hierarchical).
const PARAMOUNT_BELOW_AXIS_VALUE: f32 = 0.0;

/// A tribe's durable identity. Allocated from [`TribeRegistry`]; never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TribeId(pub u64);

/// What kind of leadership a tribe formed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TribeLeaderKind {
    /// One person the bands follow. Where the largest band's culture leans hierarchical.
    ParamountChief,
    /// The bands' chiefs sit together, with one named speaker. Where it leans egalitarian.
    CouncilOfChiefs,
}

impl TribeLeaderKind {
    /// The kind a tribe whose largest band reads `axis_value` on `HierarchicalEgalitarian` forms
    /// under; zero goes to the council, as for a band.
    pub fn for_axis_value(axis_value: f32) -> Self {
        if axis_value < PARAMOUNT_BELOW_AXIS_VALUE {
            TribeLeaderKind::ParamountChief
        } else {
            TribeLeaderKind::CouncilOfChiefs
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

/// The person a tribe follows (a council's speaker, for a council). Fixed at formation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TribeLeader {
    pub name: String,
    pub kind: TribeLeaderKind,
    pub since_turn: u64,
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

/// **Every tribe, and how many each people has formed.** Checkpoint state: identity and the leader
/// persist across turns and nothing rebuilds them.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TribeRegistry {
    tribes: BTreeMap<TribeId, Tribe>,
    next_id: u64,
    /// Cumulative formations per people, the total the Telling diffs into
    /// `tribes.formed_this_turn`.
    formed: BTreeMap<FactionId, u64>,
}

impl TribeRegistry {
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

/// One candidate group this turn: the bands of one people that pool with each other and remember
/// one place.
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

/// Group `(band, people, network, anchor)` rows into candidate tribes: same people, same network,
/// same anchor, at least [`MIN_TRIBE_BANDS`] bands. Network 0 and a missing anchor are skipped.
pub fn group_bands(
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

/// What the system reads of one resident band.
struct BandFacts {
    size: u32,
    name: String,
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

/// **Keep the tribes: match, form, change membership, dissolve.** Runs each turn after the
/// belief anchors and the supply networks of the turn are settled and after `advance_leadership`.
/// Bands are visited in id order and tribes in id order, so identity and the feed are
/// deterministic.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_tribes(
    tick: Res<SimulationTick>,
    sim_config: Res<SimulationConfig>,
    names: Res<LeaderNameCatalogHandle>,
    culture: Res<CultureManager>,
    supply: Res<SupplyNetworkMembership>,
    mut registry: ResMut<TribeRegistry>,
    mut event_log: ResMut<CommandEventLog>,
    bands: Query<(Entity, &BandId, &PopulationCohort, Option<&BandName>), With<ResidentBand>>,
) {
    let turn = tick.0;
    let mut facts: BTreeMap<BandId, BandFacts> = BTreeMap::new();
    let mut rows = Vec::new();
    for (entity, band, cohort, name) in bands.iter() {
        facts.insert(
            *band,
            BandFacts {
                size: cohort.size,
                name: name
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|| band_label(*band)),
            },
        );
        rows.push((
            *band,
            cohort.faction,
            supply.network_of(entity),
            cohort.belief_anchor,
        ));
    }
    let groups = group_bands(rows);
    let matches = resolve_matches(&registry.tribes, &groups);
    let band_name = |band: BandId| facts.get(&band).map(|f| f.name.clone());

    // Existing tribes: update the matched, dissolve the rest.
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
            if band_name(band).is_none() {
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

    // New tribes: every group no tribe kept.
    let claimed: BTreeSet<usize> = matches.values().copied().collect();
    for (index, group) in groups.iter().enumerate() {
        if claimed.contains(&index) {
            continue;
        }
        let Some(largest) = largest_band(group.members.iter(), &facts) else {
            continue;
        };
        // The kind is fixed forever, so it is not read off a default: a largest band with no
        // culture layer yet waits a turn.
        let Some(layer) = culture.band_layer_by_owner(CultureOwner::from_band(largest)) else {
            continue;
        };
        let axis_value =
            layer.traits.values()[CultureTraitAxis::HierarchicalEgalitarian.index()].to_f32();
        let id = registry.allocate();
        let name = format!(
            "{} tribe",
            band_name(largest).unwrap_or_else(|| band_label(largest))
        );
        let leader = TribeLeader {
            name: names.catalog().name_for_tribe(sim_config.map_seed, id.0),
            kind: TribeLeaderKind::for_axis_value(axis_value),
            since_turn: turn,
        };
        let tribe = Tribe {
            id,
            faction: group.faction,
            name,
            members: group.members.clone(),
            leader,
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
                },
                since_turn: 1,
            },
        )
    }

    #[test]
    fn the_kind_follows_the_sign_and_zero_goes_to_the_council() {
        assert_eq!(
            TribeLeaderKind::for_axis_value(-0.01),
            TribeLeaderKind::ParamountChief
        );
        assert_eq!(
            TribeLeaderKind::for_axis_value(0.0),
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
    fn grouping_skips_network_zero_a_missing_anchor_and_a_lone_band() {
        let at = |x, y| Some(UVec2::new(x, y));
        let f = FactionId(0);
        let groups = group_bands([
            (band(1), f, 1, at(2, 2)),
            (band(2), f, 1, at(2, 2)),
            (band(3), f, 1, at(5, 5)),
            (band(4), f, 0, at(2, 2)),
            (band(5), f, 1, None),
        ]);
        assert_eq!(groups, vec![group(&[1, 2])]);
    }
}
