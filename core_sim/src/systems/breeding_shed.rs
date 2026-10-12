//! **A breeding population above its ceiling sheds** (issue #690). Rationale:
//! `.claude/rules/core_sim/campaign.md` → "The breeding ceiling".
//!
//! `simulate_population` already stops BIRTHS in a breeding population that sits at or above its
//! ceiling (`|lines| × K`, the tie-joined group's shared ceiling). Nobody is removed by that, so a
//! population whose lifter lapsed (a foreign tie bled out, a relink dropped a shared line) sat above
//! it for good. This system gives the excess somewhere to go.
//!
//! 1. **The leak** — each turn a group not latched into `FreeBreedingPeoples` sheds
//!    `lineage.shed_share × (head-count − ceiling)` people, apportioned across its member bands by
//!    their share of the head-count and across a band's brackets by migration's own weighting. They
//!    walk to the destinations the unhappy trickle already knows
//!    ([`walkable_destination`]) — with two more conditions: the destination is not in the source's
//!    own group, and its group has headroom, which is spent as the turn allocates.
//! 2. **The break-off** — a group over its ceiling with no eligible destination for any member
//!    queues one [`BreakOffAsk`]; `systems::break_off_breeding_excess` performs the split and hands
//!    the child to the independence break-away.
//!
//! A person crossing into another people carries their knowledge as migration's leavers do (the
//! shared `labor::carry_knowledge_across` credit).
//!
//! The ceiling is read off what `simulate_population`'s pre-pass already published on each cohort
//! this turn (`PopulationCohort::last_breeding`), so there is one notion of a group and its
//! ceiling, and the head-count is the live one.

use std::collections::BTreeMap;

use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, math::UVec2, prelude::*};

use super::labor::{
    carry_knowledge_across, credit_migrated_knowledge, merge_migrated_knowledge,
    walkable_destination, weighted_leavers, DestinationKind, KnowledgeCredit,
    MigrationKnowledgeSinks, MigrationTally, WalkEnd,
};
use crate::{
    components::{available_workers, BandId, PopulationCohort, ResidentBand, Tile},
    connections::ConnectionLedger,
    demographics_config::DemographicsConfigHandle,
    intensification::LadderConfigHandle,
    lineage::{FreeBreedingPeoples, NO_INBREEDING_CEILING},
    orders::FactionId,
    resources::TileRegistry,
    resources::{CommandEventLog, FactionBorderPolicies, SimulationConfig, SimulationTick},
    routes::RoadRegistry,
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    supply_network_config::SupplyNetworkConfigHandle,
    systems::population::push_migration_events,
    wellbeing_config::WellbeingConfigHandle,
};

/// One group's break-off, queued by [`shed_breeding_excess`] for `break_off_breeding_excess`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakOffAsk {
    /// The member with the most workers — the band the break-off splits from.
    pub parent: Entity,
    /// Workers to send: `round(E × parent.working ÷ parent.total)`, so about `E` people leave once
    /// the children and elders follow by share.
    pub asked: u32,
}

/// The break-offs queued this turn. Filled by [`shed_breeding_excess`] and DRAINED by
/// `break_off_breeding_excess` later in the same Population chain, so it is empty at the end of
/// every turn (`CultureSplitQueue`'s shape): derived, never checkpointed.
#[derive(Resource, Debug, Default)]
pub struct BreedingBreakOffs {
    pub asks: Vec<BreakOffAsk>,
}

/// A breeding group's identity this turn: its people and the lowest `BandId` of its members (the
/// published reading's first row), or the band alone when it has no id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum GroupKey {
    Tied(u64),
    Alone(Entity),
}

type Group = (FactionId, GroupKey);

/// Everything the shed reads beyond the cohorts, bundled so the system stays inside Bevy's
/// argument budget.
#[derive(SystemParam)]
pub struct ShedInputs<'w> {
    sim_config: Res<'w, SimulationConfig>,
    wellbeing: Res<'w, WellbeingConfigHandle>,
    demographics: Res<'w, DemographicsConfigHandle>,
    tile_registry: Res<'w, TileRegistry>,
    supply: Res<'w, SupplyNetworkConfigHandle>,
    ladder: Res<'w, LadderConfigHandle>,
    roads: Res<'w, RoadRegistry>,
    connections: Res<'w, ConnectionLedger>,
    borders: Res<'w, FactionBorderPolicies>,
    free: Res<'w, FreeBreedingPeoples>,
    tick: Res<'w, SimulationTick>,
}

/// One resident band, as the shed reads it off the pre-move world.
struct Row {
    entity: Entity,
    band_id: Option<BandId>,
    faction: FactionId,
    pos: Option<UVec2>,
    morale: Scalar,
    total: Scalar,
    working: Scalar,
    /// `None` for a band with no published reading (a fresh splinter): no known group, so neither
    /// a source nor a destination.
    group: Option<Group>,
}

impl Row {
    fn end(&self) -> Option<WalkEnd> {
        Some(WalkEnd {
            faction: self.faction,
            band_id: self.band_id,
            pos: self.pos?,
        })
    }
}

/// One breeding group: its ceiling (`None` when the people breeds freely), its live head-count and
/// its member rows.
struct GroupInfo {
    ceiling: Option<Scalar>,
    headcount: Scalar,
    members: Vec<usize>,
}

/// A move the pass has decided: `amount` people from row `src` to row `dest`.
struct Shed {
    src: usize,
    dest: usize,
    amount: Scalar,
}

/// **The leak and the break-off decision.** See the module doc. Runs in the Population chain after
/// `advance_population_migration`: that system writes each band's `last_emigrated` /
/// `last_immigrated` unconditionally, so the shed adds to them after it rather than being
/// overwritten, and the ceiling readings it uses are still this turn's (parked by
/// `simulate_population` and untouched since).
pub fn shed_breeding_excess(
    inputs: ShedInputs,
    mut break_offs: ResMut<BreedingBreakOffs>,
    mut sinks: MigrationKnowledgeSinks,
    mut event_log: ResMut<CommandEventLog>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    let share = scalar_from_f32(inputs.demographics.get().lineage.shed_share);
    let mig_cfg = inputs.wellbeing.get().migration.clone();
    let dependent_weight = scalar_from_f32(mig_cfg.dependent_weight);
    let walk = crate::supply::WalkReach::for_people(
        mig_cfg.base_reach,
        &inputs.supply.get(),
        &inputs.ladder.get(),
        inputs.tile_registry.width,
        inputs.tile_registry.height,
        inputs.sim_config.map_topology.wrap_horizontal,
    );

    // ---- The pre-move snapshot, in entity order (migration's order, so ties break the same way) ----
    let mut rows: Vec<Row> = cohorts
        .iter()
        .map(|(entity, cohort, band_id)| {
            let reading = &cohort.last_breeding;
            // A reading struck for another people (the band flipped in migration this turn) says
            // nothing about the people it is in now.
            let group = match (band_id, reading.members.first()) {
                _ if reading.faction != Some(cohort.faction) => None,
                (_, None) => None,
                (None, Some(_)) => Some((cohort.faction, GroupKey::Alone(entity))),
                (Some(_), Some(first)) => Some((cohort.faction, GroupKey::Tied(first.band))),
            };
            Row {
                entity,
                band_id: band_id.copied(),
                faction: cohort.faction,
                pos: tiles.get(cohort.home).ok().map(|tile| tile.position),
                morale: cohort.morale,
                total: cohort.total(),
                working: cohort.working,
                group,
            }
        })
        .collect();
    rows.sort_by_key(|row| row.entity.to_bits());

    let mut groups: BTreeMap<Group, GroupInfo> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let Some(group) = row.group else { continue };
        let reading = cohorts
            .get(row.entity)
            .map(|(_, cohort, _)| cohort.last_breeding.ceiling)
            .unwrap_or(NO_INBREEDING_CEILING);
        let info = groups.entry(group).or_insert_with(|| GroupInfo {
            ceiling: (!inputs.free.contains(row.faction) && reading != NO_INBREEDING_CEILING)
                .then(|| Scalar::from_u32(reading)),
            headcount: scalar_zero(),
            members: Vec::new(),
        });
        info.headcount += row.total;
        info.members.push(index);
    }

    // Headroom left in every capped group, spent as the pass allocates. A group absent from the
    // map breeds freely and has room for anyone.
    let mut headroom: BTreeMap<Group, Scalar> = groups
        .iter()
        .filter_map(|(group, info)| {
            info.ceiling
                .map(|ceiling| (*group, ceiling - info.headcount))
        })
        .collect();
    let has_room = |headroom: &BTreeMap<Group, Scalar>, group: &Group| {
        headroom.get(group).is_none_or(|room| *room > scalar_zero())
    };

    // ---- Who may take whom: migration's predicate, plus not-my-group and headroom ----
    let destinations_of = |source: usize, headroom: &BTreeMap<Group, Scalar>| -> Vec<usize> {
        let (Some(src), Some(src_group)) = (rows[source].end(), rows[source].group) else {
            return Vec::new();
        };
        let mut own: Vec<usize> = Vec::new();
        let mut other: Vec<usize> = Vec::new();
        for (j, dest) in rows.iter().enumerate() {
            let (Some(dest_end), Some(dest_group)) = (dest.end(), dest.group) else {
                continue;
            };
            if dest_group == src_group || !has_room(headroom, &dest_group) {
                continue;
            }
            match walkable_destination(
                &walk,
                &inputs.roads,
                &inputs.connections,
                &inputs.borders,
                src,
                dest_end,
            ) {
                Some(DestinationKind::OwnPeople) => own.push(j),
                Some(DestinationKind::OtherPeople) => other.push(j),
                None => {}
            }
        }
        // Own people first, then the best morale; a tie keeps the snapshot (entity) order.
        let by_morale = |list: &mut Vec<usize>| {
            list.sort_by(|a, b| rows[*b].morale.cmp(&rows[*a].morale));
        };
        by_morale(&mut own);
        by_morale(&mut other);
        own.extend(other);
        own
    };

    // ---- The leak, and the groups with nowhere to go ----
    let mut sheds: Vec<Shed> = Vec::new();
    let mut asks: Vec<BreakOffAsk> = Vec::new();
    for info in groups.values() {
        let Some(ceiling) = info.ceiling else {
            continue;
        };
        if info.headcount <= ceiling || info.headcount <= scalar_zero() {
            continue;
        }
        let excess = info.headcount - ceiling;
        let shed = excess * share;
        // Eligibility is struck on the headroom the turn OPENED with, so "nowhere to go" is a fact
        // about the world, not about who got there first.
        let eligible: Vec<Vec<usize>> = info
            .members
            .iter()
            .map(|&member| destinations_of(member, &headroom))
            .collect();
        if eligible.iter().all(Vec::is_empty) {
            if let Some(ask) = break_off_ask(&rows, info, excess) {
                asks.push(ask);
            }
            continue;
        }
        for (&member, ordered) in info.members.iter().zip(&eligible) {
            let Ok((_, cohort, _)) = cohorts.get(rows[member].entity) else {
                continue;
            };
            let wanted = shed * rows[member].total / info.headcount;
            let (mut left, ..) = weighted_leavers(cohort, wanted, dependent_weight);
            for &dest in ordered {
                if left <= scalar_zero() {
                    break;
                }
                let Some(dest_group) = rows[dest].group else {
                    continue;
                };
                let take = headroom
                    .get(&dest_group)
                    .map_or(left, |room| left.min(*room));
                if take <= scalar_zero() {
                    continue;
                }
                if let Some(room) = headroom.get_mut(&dest_group) {
                    *room -= take;
                }
                left -= take;
                sheds.push(Shed {
                    src: member,
                    dest,
                    amount: take,
                });
            }
        }
    }
    break_offs.asks.extend(asks);
    if sheds.is_empty() {
        return;
    }

    // ---- Tally and book every move against the snapshot, then apply in one pass ----
    let mut deltas: BTreeMap<Entity, (Scalar, Scalar, Scalar)> = BTreeMap::new();
    // Per source, the people that left toward one destination people (`None`: its own).
    let mut out: BTreeMap<(Entity, Option<FactionId>), Scalar> = BTreeMap::new();
    // Per destination, the people that arrived from one people.
    let mut arrived: BTreeMap<(Entity, FactionId), Scalar> = BTreeMap::new();
    let mut knowledge_in: HashMap<Entity, Vec<_>> = HashMap::new();
    let mut credits: Vec<KnowledgeCredit> = Vec::new();
    for shed in &sheds {
        let (src, dest) = (&rows[shed.src], &rows[shed.dest]);
        let Ok((_, cohort, _)) = cohorts.get(src.entity) else {
            continue;
        };
        let (_, working, children, elders) =
            weighted_leavers(cohort, shed.amount, dependent_weight);
        let source = deltas.entry(src.entity).or_default();
        source.0 -= working;
        source.1 -= children;
        source.2 -= elders;
        let target = deltas.entry(dest.entity).or_default();
        target.0 += working;
        target.1 += children;
        target.2 += elders;
        let joined = (dest.faction != src.faction).then_some(dest.faction);
        if joined.is_some() && src.total > scalar_zero() {
            // A person crossing into another people carries what they know, exactly as the
            // unhappy trickle's leavers do (`labor::carry_knowledge_across`).
            carry_knowledge_across(
                cohort,
                (working + children + elders) / src.total,
                inputs.sim_config.migration_fragment_scaling,
                (src.faction, dest.faction),
                dest.entity,
                &mut knowledge_in,
                &mut credits,
            );
        }
        *out.entry((src.entity, joined)).or_default() += shed.amount;
        *arrived.entry((dest.entity, src.faction)).or_default() += shed.amount;
    }

    credit_migrated_knowledge(&credits, inputs.tick.0, &mut sinks);

    let band_of = |entity: Entity| rows.iter().find(|row| row.entity == entity);
    let now = inputs.tick.0;
    let mut emigrated: BTreeMap<Entity, u32> = BTreeMap::new();
    let mut immigrated: BTreeMap<Entity, u32> = BTreeMap::new();
    for ((entity, joined), amount) in &out {
        let people = amount.round().to_u32();
        *emigrated.entry(*entity).or_default() += people;
        let Some(row) = band_of(*entity) else {
            continue;
        };
        if let Some(band) = row.band_id {
            let tally = MigrationTally {
                emigrated: people,
                joined_people: *joined,
                ..MigrationTally::default()
            };
            push_migration_events(&mut event_log, now, row.faction, band, &tally);
        }
    }
    for ((entity, from), amount) in &arrived {
        let people = amount.round().to_u32();
        *immigrated.entry(*entity).or_default() += people;
        let Some(row) = band_of(*entity) else {
            continue;
        };
        if let Some(band) = row.band_id {
            let mut tally = MigrationTally::default();
            if *from == row.faction {
                tally.immigrated_own = people;
            } else {
                tally.immigrated_foreign.insert(*from, people);
            }
            push_migration_events(&mut event_log, now, row.faction, band, &tally);
        }
    }

    for (entity, mut cohort, _) in cohorts.iter_mut() {
        if let Some((working, children, elders)) = deltas.get(&entity) {
            cohort.working = (cohort.working + *working).max(scalar_zero());
            cohort.children = (cohort.children + *children).max(scalar_zero());
            cohort.elders = (cohort.elders + *elders).max(scalar_zero());
            cohort.sync_size();
        }
        if let Some(payload) = knowledge_in.get(&entity) {
            merge_migrated_knowledge(&mut cohort, payload);
        }
        cohort.last_emigrated += emigrated.get(&entity).copied().unwrap_or_default();
        cohort.last_immigrated += immigrated.get(&entity).copied().unwrap_or_default();
    }
}

/// **The break-off a stranded group asks for**: the member with the most workers (ties to the
/// first in entity order) sends `round(E × working ÷ total)` workers, so about `E` people leave
/// once its children and elders follow by share. Capped at what the parent can give; `None` when
/// that comes to nobody, which is a break-off with nothing to send. The floors
/// (`min_founding_workers`, `parent_min_workers`) are the split's own and refuse there.
fn break_off_ask(rows: &[Row], info: &GroupInfo, excess: Scalar) -> Option<BreakOffAsk> {
    let parent = info
        .members
        .iter()
        .copied()
        .reduce(|best, next| {
            if rows[next].working > rows[best].working {
                next
            } else {
                best
            }
        })
        .map(|index| &rows[index])?;
    if parent.total <= scalar_zero() {
        return None;
    }
    let asked = (excess * parent.working / parent.total)
        .to_u32()
        .min(available_workers(parent.working));
    (asked > 0).then_some(BreakOffAsk {
        parent: parent.entity,
        asked,
    })
}
