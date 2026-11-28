//! **THE TRIBE LEADER PROPOSES TRIBE WORKS; THE PLAYER ANSWERS** (#863). The paramount chief never adds a
//! work himself: [`refresh_tribe_proposals`] offers **one** candidate per tribe and the player answers
//! `tribe_proposal <f> <b> approve <key>` or `defer <key>` ("Not now").
//!
//! The candidates are ranked monument, then storage pits, then roads:
//!
//! * a **monument** at the members' shared gathering place (the leader band's `belief_anchor`), on the
//!   rungs `belief_config.json -> improvement_belief` lists, through the gate `tribe_work` itself runs
//!   (`build_gates::validate_tile_verb`);
//! * a **storage pit** for each member that owns no store and whose food rots at least
//!   `labor.band_orders.storage_build_min_saved` a turn (the chief's own `storageSaves` reading,
//!   [`crate::chief_proposals::band_spoilage_for`]), on that band's camp;
//! * **roads** between member camps: a minimum spanning tree over wrap-aware hex distance, each edge
//!   without a standing link proposed as one road (one `route` work per tile still below grade), only
//!   when `tribe_levy`'s road gate accepts every tile.
//!
//! He proposes only for a tribe whose levy ceiling is above `none`, and only while every tribe build that
//! can progress holds its full crew (the band chief's "waits until the builds underway are full", at tribe
//! scope). The engine names no rung: every rung is read off the ladder or a config list.
//!
//! Rules: `.claude/rules/core_sim/leadership.md` -> "The tribe leader proposes tribe works".

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use sim_schema::TribeProposalState;

use crate::{
    chief_proposals::{band_spoilage_for, proposal_phrase, BandView},
    components::{BandId, BandName, BuildSource, LaborAllocation, PopulationCohort},
    grid_utils::hex_distance_wrapped,
    intensification::{RungId, SourceKind},
    keep_on_hand::Keeper,
    labor_config::LaborConfigHandle,
    ladder_ids::BranchId,
    leadership::BandLeadership,
    orders::FactionId,
    resources::SimulationTick,
    tribe::{LevyCeiling, Tribe, TribeId, TribeRegistry},
    tribe_levy::{
        is_road_branch, pass_scene, refuse, resident_members, road_work_gate, tribe_of_band,
        tribe_work, Member, TribeRefusal,
    },
};

/// The `kind` token of a monument proposal.
pub const KIND_MONUMENT: &str = "monument";
/// The `kind` token of a storage-pit proposal.
pub const KIND_STORAGE: &str = "storage";
/// The `kind` token of a road proposal.
pub const KIND_ROAD: &str = "road";
/// What a road proposal says the tribe does (the rung's own phrase belongs to a band's one tile).
const ROAD_PHRASE: &str = "build a road";
/// The refusal token for a key that is no longer the tribe's proposal.
pub const NO_SUCH_PROPOSAL: &str = "no_such_proposal";

/// One tribe's proposal state. Saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TribeProposals {
    /// "Not now": proposal key -> the tick it comes back at.
    pub deferred: BTreeMap<String, u64>,
    /// The proposal on offer now (zero or one).
    pub current: Option<TribeProposalState>,
}

/// Every tribe's proposal state (`SimState`, saved).
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TribeProposalBook {
    pub tribes: BTreeMap<TribeId, TribeProposals>,
}

impl TribeProposalBook {
    /// The proposal on offer to `tribe`, if any.
    pub fn current(&self, tribe: TribeId) -> Option<&TribeProposalState> {
        self.tribes
            .get(&tribe)
            .and_then(|state| state.current.as_ref())
    }
}

/// One work an approval adds.
struct WorkSpec {
    tile: UVec2,
    branch: BranchId,
    rung: RungId,
}

/// One proposal the leader could make now.
struct Candidate {
    key: String,
    kind: &'static str,
    /// The sentence after "<leader> proposes: ".
    action: String,
    tile: UVec2,
    for_band: u64,
    works: Vec<WorkSpec>,
}

fn band_name(world: &World, entity: Entity, band: BandId) -> String {
    world
        .get::<BandName>(entity)
        .map_or_else(|| crate::systems::band_label(band), |name| name.0.clone())
}

/// The monument candidates: the leader band's gathering place.
fn monument_candidates(world: &World, tribe: &Tribe, members: &[Member]) -> Vec<Candidate> {
    let Some(leader) = members.iter().find(|m| m.band == tribe.leader.band) else {
        return Vec::new();
    };
    let Some(cohort) = world.get::<PopulationCohort>(leader.entity) else {
        return Vec::new();
    };
    let Some(anchor) = cohort.belief_anchor else {
        return Vec::new();
    };
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let belief = world
        .resource::<crate::belief_config::BeliefConfigHandle>()
        .get();
    let improvements = world.resource::<crate::tile_improvement::TileImprovementRegistry>();
    let mut found = Vec::new();
    for source in &belief.improvement_belief {
        let rung = source.rung;
        let branch = rung.branch;
        if ladder.source_kind(branch) != Some(SourceKind::Tile) {
            continue;
        }
        if improvements
            .get(anchor, branch)
            .is_some_and(|held| ladder.at_or_above(held.held_rung(), rung))
        {
            continue;
        }
        if tribe
            .works
            .iter()
            .any(|work| work.same_site(anchor, branch))
        {
            continue;
        }
        let def = ladder.rung(rung);
        let allowed = crate::build_gates::with_scene(world, |scene| {
            crate::build_gates::validate_tile_verb(scene, tribe.faction, anchor, branch, def)
        });
        if allowed.is_err() {
            continue;
        }
        found.push(Candidate {
            key: format!("{KIND_MONUMENT}@{},{}", anchor.x, anchor.y),
            kind: KIND_MONUMENT,
            action: format!("{} at ({}, {})", proposal_phrase(def), anchor.x, anchor.y),
            tile: anchor,
            for_band: 0,
            works: vec![WorkSpec {
                tile: anchor,
                branch,
                rung,
            }],
        });
    }
    found
}

/// Whether `band` owns any food store (an improvement holding a fixed-store rung).
fn owns_a_store(world: &World, band: BandId, faction: FactionId) -> bool {
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let keeping = world
        .resource::<crate::demographics_config::DemographicsConfigHandle>()
        .get();
    world
        .resource::<crate::tile_improvement::TileImprovementRegistry>()
        .iter()
        .any(|improvement| {
            improvement.owner == Some(band)
                && improvement.faction == faction
                && keeping
                    .keeping
                    .store_spec(&ladder, improvement.held_rung())
                    .is_some()
        })
}

/// The storage-pit candidates, one per member that owns no store and loses enough food to rot.
fn storage_candidates(world: &World, tribe: &Tribe, members: &[Member]) -> Vec<Candidate> {
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let threshold = world
        .resource::<LaborConfigHandle>()
        .get()
        .band_orders
        .storage_build_min_saved;
    let mut found = Vec::new();
    for member in members {
        if owns_a_store(world, member.band, tribe.faction) {
            continue;
        }
        let view = BandView {
            entity: member.entity,
            id: member.band,
            faction: tribe.faction,
            forage_rows: Vec::new(),
        };
        let Some(spoilage) = band_spoilage_for(world, &view) else {
            continue;
        };
        // The pit is the tile-kind store rung the band's own `storageSaves` quotes the most for.
        let mut best: Option<(f32, RungId)> = None;
        for row in spoilage.actionable(threshold).filter(|row| row.tile_kept) {
            let Ok(rung) = row.rung.parse::<RungId>() else {
                continue;
            };
            let allowed = crate::build_gates::with_scene(world, |scene| {
                crate::build_gates::validate_tile_verb(
                    scene,
                    tribe.faction,
                    member.camp,
                    rung.branch,
                    ladder.rung(rung),
                )
            });
            if allowed.is_ok() && best.is_none_or(|(saved, _)| row.food_saved > saved) {
                best = Some((row.food_saved, rung));
            }
        }
        let Some((_, rung)) = best else {
            continue;
        };
        if tribe
            .works
            .iter()
            .any(|work| work.same_site(member.camp, rung.branch))
        {
            continue;
        }
        let name = band_name(world, member.entity, member.band);
        found.push(Candidate {
            key: format!(
                "{KIND_STORAGE}:{}@{},{}",
                rung.wire_key(),
                member.camp.x,
                member.camp.y
            ),
            kind: KIND_STORAGE,
            action: format!(
                "{} for {name} at ({}, {})",
                proposal_phrase(ladder.rung(rung)),
                member.camp.x,
                member.camp.y
            ),
            tile: member.camp,
            for_band: member.band.0,
            works: vec![WorkSpec {
                tile: member.camp,
                branch: rung.branch,
                rung,
            }],
        });
    }
    found
}

/// The member pairs of a minimum spanning tree over wrap-aware hex distance between camps (Kruskal;
/// ties broken by the lower `BandId` pair), each pair lower id first, in pair order.
fn spanning_pairs(members: &[Member], width: u32, wrap: bool) -> Vec<(usize, usize)> {
    let mut edges: Vec<(u32, BandId, BandId, usize, usize)> = Vec::new();
    for (i, a) in members.iter().enumerate() {
        for (j, b) in members.iter().enumerate().skip(i + 1) {
            let (low, high) = if a.band <= b.band { (a, b) } else { (b, a) };
            let (li, hi) = if a.band <= b.band { (i, j) } else { (j, i) };
            edges.push((
                hex_distance_wrapped(low.camp, high.camp, width, wrap),
                low.band,
                high.band,
                li,
                hi,
            ));
        }
    }
    edges.sort();
    let mut parent: Vec<usize> = (0..members.len()).collect();
    fn root(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let mut tree = Vec::new();
    for (_, _, _, a, b) in edges {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
            tree.push((a, b));
        }
    }
    tree.sort_by_key(|&(a, b)| (members[a].band, members[b].band));
    tree
}

/// The road candidates over the members' camps.
fn road_candidates(
    world: &mut World,
    id: TribeId,
    tribe: &Tribe,
    members: &[Member],
) -> Vec<Candidate> {
    let Some(scene) = pass_scene(world) else {
        return Vec::new();
    };
    let ladder = scene.ladder.clone();
    let first_built = crate::routes::first_built_rung(&ladder).clone();
    let floor = world
        .resource::<crate::diplomacy_config::DiplomacyConfigHandle>()
        .get()
        .offers
        .standing_link_floor(&ladder);
    let mut found = Vec::new();
    for (a, b) in spanning_pairs(members, scene.grid.0, scene.wrap) {
        let (from, to) = (&members[a], &members[b]);
        let (standing, tiles) = {
            let roads = world.resource::<crate::routes::RoadRegistry>();
            let standing = crate::routes::standing_link(
                roads,
                from.camp,
                to.camp,
                scene.grid.0,
                scene.grid.1,
                scene.wrap,
                floor,
            );
            let tiles: Vec<UVec2> = crate::routes::trace_path(
                from.camp,
                to.camp,
                scene.grid.0,
                scene.grid.1,
                scene.wrap,
                roads,
            )
            .into_iter()
            .filter(|tile| {
                !roads
                    .road(*tile)
                    .is_some_and(|road| ladder.at_or_above(road.held_rung(), first_built.rung_id()))
            })
            .filter(|tile| {
                !tribe
                    .works
                    .iter()
                    .any(|work| work.same_site(*tile, first_built.branch))
            })
            .collect();
            (standing, tiles)
        };
        if standing || tiles.is_empty() {
            continue;
        }
        let accepted = tiles
            .iter()
            .all(|tile| road_work_gate(world, id, tribe.faction, *tile, &first_built).is_ok());
        if !accepted {
            continue;
        }
        let from_name = band_name(world, from.entity, from.band);
        let to_name = band_name(world, to.entity, to.band);
        found.push(Candidate {
            key: format!("{KIND_ROAD}:{}-{}", from.band.0, to.band.0),
            kind: KIND_ROAD,
            action: format!("{ROAD_PHRASE} from {from_name} to {to_name}"),
            tile: tiles[0],
            for_band: 0,
            works: tiles
                .iter()
                .map(|tile| WorkSpec {
                    tile: *tile,
                    branch: first_built.branch,
                    rung: first_built.rung_id(),
                })
                .collect(),
        });
    }
    found
}

/// Every proposal the leader could make now, ranked: monument, storage pits, roads.
fn candidates(world: &mut World, id: TribeId) -> Vec<Candidate> {
    let Some(tribe) = world.resource::<TribeRegistry>().get(id).cloned() else {
        return Vec::new();
    };
    let members = resident_members(world, &tribe.members);
    let mut found = monument_candidates(world, &tribe, &members);
    found.extend(storage_candidates(world, &tribe, &members));
    found.extend(road_candidates(world, id, &tribe, &members));
    found
}

/// Whether every declared tribe build that **can progress** holds its full crew: the builders summed
/// across the work's carriers reach the crew's cap (`BuildCrew::cap`). A build that cannot progress (a
/// refusing gate, a materials stall) is never short.
fn builds_are_full(world: &World, tribe: &Tribe, members: &[Member]) -> bool {
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    for (index, work) in tribe.works.iter().enumerate() {
        if work.target_rung.is_none() {
            continue;
        }
        let source = if is_road_branch(&ladder, work.branch) {
            BuildSource::Road(work.tile)
        } else {
            BuildSource::Tile {
                tile: work.tile,
                branch: work.branch,
            }
        };
        let mut total = 0u32;
        let mut cap = 0u32;
        for carrier in tribe.carriers.get(index).into_iter().flatten() {
            let Some(member) = members.iter().find(|m| m.band == carrier.band) else {
                continue;
            };
            let Some(crew) = world
                .get::<LaborAllocation>(member.entity)
                .and_then(|allocation| allocation.build_crew(&source))
            else {
                continue;
            };
            total += crew.total();
            if crate::staffing::band_build_can_progress(world, member.entity, &source) {
                cap = cap.max(crew.cap);
            }
        }
        if total < cap {
            return false;
        }
    }
    true
}

/// The sentence a proposal reads as: "<leader> proposes: <action>" (a council: "<Band>'s elders propose").
fn proposal_text(world: &World, tribe: &Tribe, members: &[Member], action: &str) -> String {
    let leader = members.iter().find(|m| m.band == tribe.leader.band);
    let keeper = leader.and_then(|member| {
        let name = band_name(world, member.entity, member.band);
        Keeper::of(world.get::<BandLeadership>(member.entity), &name)
    });
    let (subject, plural) = keeper.map_or((tribe.leader.name.clone(), false), |keeper| {
        (keeper.subject, keeper.plural)
    });
    let verb = if plural { "propose" } else { "proposes" };
    format!("{subject} {verb}: {action}")
}

/// The proposal one tribe makes now: the first ranked candidate whose key is not deferred.
fn proposal_for(
    world: &mut World,
    id: TribeId,
    deferred: &BTreeMap<String, u64>,
) -> Option<TribeProposalState> {
    let tribe = world.resource::<TribeRegistry>().get(id).cloned()?;
    if tribe.levy == LevyCeiling::None {
        return None;
    }
    let members = resident_members(world, &tribe.members);
    if !builds_are_full(world, &tribe, &members) {
        return None;
    }
    let candidate = candidates(world, id)
        .into_iter()
        .find(|candidate| !deferred.contains_key(&candidate.key))?;
    Some(TribeProposalState {
        text: proposal_text(world, &tribe, &members, &candidate.action),
        key: candidate.key,
        kind: candidate.kind.to_string(),
        tile_x: candidate.tile.x,
        tile_y: candidate.tile.y,
        for_band: candidate.for_band,
    })
}

/// **THE PASS**: re-derive every tribe's proposal, drop the deferrals whose time is up and the state of
/// tribes that no longer exist. Run after `advance_tribe_levy` (it reads the fresh levy) and after every
/// command that changes the works or the ceiling.
pub fn refresh_tribe_proposals(world: &mut World) {
    let tick = world.resource::<SimulationTick>().0;
    let ids: Vec<TribeId> = world
        .resource::<TribeRegistry>()
        .iter()
        .map(|tribe| tribe.id)
        .collect();
    world
        .resource_mut::<TribeProposalBook>()
        .tribes
        .retain(|id, _| ids.contains(id));
    for id in ids {
        let mut deferred = world
            .resource::<TribeProposalBook>()
            .tribes
            .get(&id)
            .map(|state| state.deferred.clone())
            .unwrap_or_default();
        deferred.retain(|_, until| *until > tick);
        let current = proposal_for(world, id, &deferred);
        let mut book = world.resource_mut::<TribeProposalBook>();
        let state = book.tribes.entry(id).or_default();
        state.deferred = deferred;
        state.current = current;
    }
}

/// The Population-chain system: see the module documentation.
pub fn advance_tribe_proposals(world: &mut World) {
    refresh_tribe_proposals(world);
}

/// **`tribe_proposal <faction> <band> approve <key>`**: re-derive the candidates, match the key against the
/// tribe's current proposal and add each of its works through `tribe_levy::tribe_work` at the default
/// priority, exactly as the player's `tribe_work` would.
pub fn approve_tribe_proposal(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    key: &str,
) -> Result<String, TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let on_offer = world
        .resource::<TribeProposalBook>()
        .current(id)
        .is_some_and(|current| current.key == key);
    let candidate = if on_offer {
        candidates(world, id)
            .into_iter()
            .find(|candidate| candidate.key == key)
    } else {
        None
    };
    let Some(candidate) = candidate else {
        return refuse(NO_SUCH_PROPOSAL, "The tribe leader is not proposing that.");
    };
    for work in &candidate.works {
        tribe_work(
            world,
            faction,
            band,
            work.branch.as_str(),
            work.tile,
            Some(&work.rung.wire_key()),
            None,
        )?;
    }
    let name = world
        .resource::<TribeRegistry>()
        .get(id)
        .map(|tribe| tribe.name.clone())
        .unwrap_or_default();
    refresh_tribe_proposals(world);
    Ok(format!("The {name} agrees to {}.", candidate.action))
}

/// **`tribe_proposal <faction> <band> defer <key>`**: hide that proposal for
/// `labor.band_orders.proposal_not_now_turns` turns.
pub fn defer_tribe_proposal(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    key: &str,
) -> Result<String, TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let tick = world.resource::<SimulationTick>().0;
    let turns = u64::from(
        world
            .resource::<LaborConfigHandle>()
            .get()
            .band_orders
            .proposal_not_now_turns,
    );
    world
        .resource_mut::<TribeProposalBook>()
        .tribes
        .entry(id)
        .or_default()
        .deferred
        .insert(key.to_string(), tick + turns);
    let name = world
        .resource::<TribeRegistry>()
        .get(id)
        .map(|tribe| tribe.name.clone())
        .unwrap_or_default();
    refresh_tribe_proposals(world);
    Ok(format!("The {name} puts that off for now."))
}
