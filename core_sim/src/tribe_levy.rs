//! **A tribe levies its bands** (#862): the tribe owns works, a levy ceiling and one scouting range,
//! and this module turns them into rows on the member bands' own lists.
//!
//! * [`apply_tribe_pass`] / [`advance_tribe_levy`] - the **tribe pass**: each work becomes a levy row
//!   (`LaborAssignment::for_tribe`) on the nearest member, with an overflow row on the next nearest
//!   member while the nearer one's row finished the last staffing short of its want. The staffing
//!   (`staffing.rs`, the levy step) deals those rows from the band's levy budget.
//! * [`scouting_stretches`] - each member's share of the tribe's scouting edge.
//! * the four commands' sim halves ([`tribe_work`], [`tribe_work_stop`], [`tribe_levy`],
//!   [`tribe_scout`]), each band-addressed and each followed by the pass and a restaff.
//!
//! Design: `docs/plan_labor_at_scale.md` ("Each level owns its own work, and levies the level below
//! for it"). Rules: `.claude/rules/core_sim/leadership.md`.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use crate::{
    components::{
        BandId, BuildJob, BuildSource, LaborAllocation, LaborTarget, PopulationCohort,
        ResidentBand, SourcePriority, UNSTRETCHED,
    },
    grid_utils::{hex_distance_wrapped, hex_neighbors_wrapped, hex_range_tiles},
    intensification::{LadderConfig, SourceKind},
    ladder_ids::{BranchId, RungId},
    leadership::{tribe_leader_reach, tribe_orders_carried_out_for_member},
    leadership_config::LeadershipConfig,
    orders::FactionId,
    staffing::{restaff_band, LevyPlan},
    tribe::{LevyCarrier, LevyCeiling, TribeId, TribeRegistry, TribeWork},
};

/// The levy plan a tribe member's staffing is dealt under: the ceiling's share and the share of the
/// tribe leader's orders that reach this member. `None` for a band in no tribe.
pub fn levy_plan(
    registry: &TribeRegistry,
    config: &LeadershipConfig,
    ladder: &LadderConfig,
    band: BandId,
    member_has_own_leader: bool,
) -> Option<LevyPlan> {
    let tribe = registry.tribe_of(band)?;
    let reach = tribe_leader_reach(config, tribe.members.len());
    Some(LevyPlan {
        share: tribe.levy.share(config),
        factor: tribe_orders_carried_out_for_member(config, &reach, member_has_own_leader),
        fixed_budget: None,
        road_tiles: tribe
            .works
            .iter()
            .filter(|work| is_road_branch(ladder, work.branch))
            .map(|work| work.tile)
            .collect(),
    })
}

/// Whether a branch is a road branch: its works are one road tile each, built by the carrier band
/// as a [`BuildSource::Road`] and kept by its Roadwork pool.
pub fn is_road_branch(ladder: &LadderConfig, branch: BranchId) -> bool {
    ladder.source_kind(branch) == Some(SourceKind::Road)
}

/// **EACH MEMBER'S SHARE OF THE TRIBE'S SCOUTING EDGE.** The union `U` of the hex discs of radius
/// `reach` around every member's camp; the edge is the hexes of `U` with a neighbour outside it
/// (off the map counts as outside); each edge hex goes to the nearest camp, ties to the lowest
/// `BandId`; a band's stretch is its edge hexes over `6 x reach`, so a lone band's ring reads `1.0`
/// and a member enclosed by the others reads `0`.
pub fn scouting_stretches(
    camps: &[(BandId, UVec2)],
    reach: u32,
    grid: (u32, u32),
    wrap: bool,
) -> BTreeMap<BandId, f32> {
    let (width, height) = grid;
    let mut union: BTreeSet<(u32, u32)> = BTreeSet::new();
    for (_, camp) in camps {
        for tile in hex_range_tiles(*camp, reach, width, height, wrap) {
            union.insert((tile.y, tile.x));
        }
    }
    let mut ordered: Vec<(BandId, UVec2)> = camps.to_vec();
    ordered.sort_by_key(|(band, _)| *band);
    let mut edge_hexes: BTreeMap<BandId, u32> =
        ordered.iter().map(|(band, _)| (*band, 0)).collect();
    for &(y, x) in &union {
        let neighbours: Vec<(u32, u32)> =
            hex_neighbors_wrapped(x, y, width, height, wrap).collect();
        let on_edge = neighbours.len() < crate::grid_utils::HEX_DIRECTION_COUNT
            || neighbours
                .iter()
                .any(|(nx, ny)| !union.contains(&(*ny, *nx)));
        if !on_edge {
            continue;
        }
        let hex = UVec2::new(x, y);
        let nearest = ordered
            .iter()
            .min_by_key(|(band, camp)| (hex_distance_wrapped(*camp, hex, width, wrap), *band))
            .map(|(band, _)| *band);
        if let Some(band) = nearest {
            *edge_hexes.entry(band).or_insert(0) += 1;
        }
    }
    let ring = (crate::grid_utils::HEX_DIRECTION_COUNT as u32 * reach).max(1) as f32;
    edge_hexes
        .into_iter()
        .map(|(band, hexes)| (band, hexes as f32 / ring))
        .collect()
}

/// A member band as the pass reads it.
struct Member {
    band: BandId,
    entity: Entity,
    camp: UVec2,
}

/// The world's grid and wrap, the supply reach and the ladder the pass reads.
struct PassScene {
    grid: (u32, u32),
    wrap: bool,
    reach: u32,
    ladder: std::sync::Arc<crate::intensification::LadderConfig>,
}

fn pass_scene(world: &World) -> Option<PassScene> {
    let tiles = world.get_resource::<crate::resources::TileRegistry>()?;
    Some(PassScene {
        grid: (tiles.width, tiles.height),
        wrap: world
            .get_resource::<crate::resources::SimulationConfig>()?
            .map_topology
            .wrap_horizontal,
        reach: world
            .get_resource::<crate::supply_network_config::SupplyNetworkConfigHandle>()?
            .get()
            .reach_tiles,
        ladder: world.get_resource::<crate::LadderConfigHandle>()?.get(),
    })
}

fn resident_members(world: &mut World, tribe: &BTreeSet<BandId>) -> Vec<Member> {
    let mut found: Vec<(BandId, Entity)> = world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(world)
        .filter(|(_, band)| tribe.contains(*band))
        .map(|(entity, band)| (*band, entity))
        .collect();
    found.sort_by_key(|(band, _)| *band);
    found
        .into_iter()
        .filter_map(|(band, entity)| {
            let camp = crate::staffing::camp_of(world, entity)?.position;
            Some(Member { band, entity, camp })
        })
        .collect()
}

/// The levy row a member holds for one work, if it holds one.
fn levy_row_of(
    allocation: &LaborAllocation,
    tile: UVec2,
    branch: BranchId,
) -> Option<&crate::components::LaborAssignment> {
    allocation.assignments.iter().find(|row| {
        matches!(&row.target, LaborTarget::Tile { tile: t, branch: b } if *t == tile && *b == branch)
    })
}

/// How many heads a carrier's row finished the last staffing short of its want: the row's
/// `reach_crew` less its workers, or its build's cap less its builders (a build counts only once it
/// holds a builder, so a build that cannot progress is never "short").
fn shortfall_of(allocation: &LaborAllocation, work: &TribeWork) -> u32 {
    let Some(row) = levy_row_of(allocation, work.tile, work.branch) else {
        return 0;
    };
    let row_short = row.staffing.reach_crew.saturating_sub(row.workers);
    let source = BuildSource::Tile {
        tile: work.tile,
        branch: work.branch,
    };
    let build_short = allocation.build_crew(&source).map_or(0, |crew| {
        if crew.total() > 0 {
            crew.cap.saturating_sub(crew.total())
        } else {
            0
        }
    });
    row_short.max(build_short)
}

/// **THE TRIBE PASS FOR ONE TRIBE**: put each work on the nearest member (plus the overflow chain),
/// remove levy rows for works no longer listed, sync every member's Scout row to the tribe's range
/// and stamp its stretch.
pub fn apply_tribe_pass(world: &mut World, id: TribeId) {
    let Some(scene) = pass_scene(world) else {
        return;
    };
    let Some(tribe) = world.resource::<TribeRegistry>().get(id).cloned() else {
        return;
    };
    let members = resident_members(world, &tribe.members);
    let stretches = scouting_stretches(
        &members.iter().map(|m| (m.band, m.camp)).collect::<Vec<_>>(),
        scene.reach,
        scene.grid,
        scene.wrap,
    );
    let wrap_width = scene.grid.0;
    release_departed_keepers(world, &tribe, &scene.ladder);
    let mut road_carriers: Vec<(usize, BandId)> = Vec::new();
    // Which members carry which work, and with what Max.
    let mut desired: BTreeMap<BandId, Vec<(usize, Option<u32>)>> = BTreeMap::new();
    let mut carriers: Vec<Vec<LevyCarrier>> = Vec::with_capacity(tribe.works.len());
    let mut order: Vec<usize> = (0..tribe.works.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(tribe.works[i].priority), i));
    let mut per_work: BTreeMap<usize, Vec<LevyCarrier>> = BTreeMap::new();
    for &w in &order {
        let work = &tribe.works[w];
        if is_road_branch(&scene.ladder, work.branch) {
            // One band per road tile: no overflow chain.
            let carrier = road_carrier(world, &members, work.tile, wrap_width, scene.wrap);
            if let Some(band) = carrier {
                road_carriers.push((w, band));
            }
            per_work.insert(
                w,
                carrier
                    .map(|band| LevyCarrier {
                        band,
                        max: None,
                        workers: 0,
                    })
                    .into_iter()
                    .collect(),
            );
            continue;
        }
        let mut by_distance: Vec<&Member> = members.iter().collect();
        by_distance.sort_by_key(|m| {
            (
                hex_distance_wrapped(m.camp, work.tile, wrap_width, scene.wrap),
                m.band,
            )
        });
        let mut chain: Vec<LevyCarrier> = Vec::new();
        let mut max: Option<u32> = None;
        for member in by_distance {
            chain.push(LevyCarrier {
                band: member.band,
                max,
                workers: 0,
            });
            desired.entry(member.band).or_default().push((w, max));
            // Is this carrier short of the work's want? Its shortfall is the next one's Max.
            let short = world
                .get::<LaborAllocation>(member.entity)
                .map_or(0, |allocation| shortfall_of(allocation, work));
            if short == 0 {
                break;
            }
            max = Some(short);
        }
        per_work.insert(w, chain);
    }
    for w in 0..tribe.works.len() {
        carriers.push(per_work.remove(&w).unwrap_or_default());
    }
    // Apply to every member's list.
    for member in &members {
        let wanted: Vec<(usize, Option<u32>)> = desired.remove(&member.band).unwrap_or_default();
        apply_to_member(world, &scene, &tribe, id, member, &wanted);
        sync_scout_row(
            world,
            member.entity,
            Some(id),
            tribe.scouting_range,
            stretches.get(&member.band).copied().unwrap_or(UNSTRETCHED),
        );
    }
    apply_road_works(world, &scene, &tribe, &members, &road_carriers);
    if let Some(stored) = world.resource_mut::<TribeRegistry>().get_mut(id) {
        stored.carriers = carriers;
    }
}

/// **THE CARRIER OF A ROAD TILE**: the band that keeps it when it is a member of the tribe, else the
/// nearest member (wrap-aware hex distance from its camp, ties to the lowest `BandId`). The tribe
/// keeps handing a road tile to its keeper, so it does not change hands as bands move.
fn road_carrier(
    world: &World,
    members: &[Member],
    tile: UVec2,
    width: u32,
    wrap: bool,
) -> Option<BandId> {
    let keeper = world
        .resource::<crate::routes::RoadRegistry>()
        .road(tile)
        .and_then(|road| road.keeper)
        .map(|keeper| keeper.band);
    if let Some(band) = keeper.filter(|band| members.iter().any(|m| m.band == *band)) {
        return Some(band);
    }
    members
        .iter()
        .min_by_key(|m| (hex_distance_wrapped(m.camp, tile, width, wrap), m.band))
        .map(|m| m.band)
}

/// **A KEEPER THAT LEFT THE TRIBE STOPS KEEPING ITS ROAD TILES**: a road tile of one of the tribe's
/// road works whose keeper is a band of the tribe's people that is no longer a member (or is gone)
/// is released, so the pass hands the tile to the nearest member, which picks it up by grading it.
fn release_departed_keepers(world: &mut World, tribe: &crate::tribe::Tribe, ladder: &LadderConfig) {
    let mut roads = world.resource_mut::<crate::routes::RoadRegistry>();
    for work in tribe
        .works
        .iter()
        .filter(|work| is_road_branch(ladder, work.branch))
    {
        let Some(road) = roads.road_mut(work.tile) else {
            continue;
        };
        if road.keeper.is_some_and(|keeper| {
            keeper.faction == tribe.faction && !tribe.members.contains(&keeper.band)
        }) {
            road.release_keeper();
        }
    }
}

/// **THE TRIBE'S ROAD BUILDS**: each road work's carrier declares the build exactly as its own
/// `grade` / `pave` would (the same gate, the keeper named with the remoteness quote), and every
/// other member drops a declaration it holds on a tile of the tribe's road works. The builders are
/// dealt in the levy step (`LevyPlan::road_tiles`); the keeping is the keeper's Roadwork pool.
fn apply_road_works(
    world: &mut World,
    scene: &PassScene,
    tribe: &crate::tribe::Tribe,
    members: &[Member],
    carriers: &[(usize, BandId)],
) {
    for member in members {
        let foreign: Vec<UVec2> = carriers
            .iter()
            .filter(|(_, band)| *band != member.band)
            .map(|(w, _)| tribe.works[*w].tile)
            .collect();
        if let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) {
            for tile in foreign {
                allocation.stop_build(&BuildSource::Road(tile));
            }
        }
    }
    for &(w, carrier) in carriers {
        let work = tribe.works[w];
        let Some(rung) = work.target_rung else {
            continue;
        };
        let Some(member) = members.iter().find(|m| m.band == carrier) else {
            continue;
        };
        let source = BuildSource::Road(work.tile);
        let road = world
            .resource::<crate::routes::RoadRegistry>()
            .road(work.tile)
            .map(|road| (road.held_rung(), road.keeper));
        let Some((held, keeper)) = road else {
            if let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) {
                allocation.stop_build(&source);
            }
            continue;
        };
        let keeps = keeper.is_some_and(|keeper| keeper.band == carrier);
        if keeps && scene.ladder.at_or_above(held, rung) {
            if let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) {
                allocation.stop_build(&source);
            }
            continue;
        }
        let def = scene.ladder.rung(rung).clone();
        if crate::build_gates::validate_road_verb(world, tribe.faction, carrier, work.tile, &def)
            .is_err()
        {
            continue;
        }
        if !keeps {
            let distance = hex_distance_wrapped(member.camp, work.tile, scene.grid.0, scene.wrap);
            let remoteness = crate::routes::remoteness_multiplier(distance, &scene.ladder);
            if let Some(road) = world
                .resource_mut::<crate::routes::RoadRegistry>()
                .road_mut(work.tile)
            {
                road.take_keeper(
                    crate::routes::RoadKeeper {
                        faction: tribe.faction,
                        band: carrier,
                    },
                    remoteness,
                    &scene.ladder,
                );
            }
        }
        if let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) {
            allocation.declare_build(source, BuildJob::Climb(rung));
        }
    }
}

fn apply_to_member(
    world: &mut World,
    scene: &PassScene,
    tribe: &crate::tribe::Tribe,
    id: TribeId,
    member: &Member,
    wanted: &[(usize, Option<u32>)],
) {
    let faction = world
        .get::<PopulationCohort>(member.entity)
        .map(|cohort| cohort.faction);
    let Some(faction) = faction else {
        return;
    };
    // Remove levy rows of this tribe for works no longer carried here.
    let keep: Vec<(UVec2, BranchId)> = wanted
        .iter()
        .map(|(w, _)| (tribe.works[*w].tile, tribe.works[*w].branch))
        .collect();
    let stale: Vec<LaborTarget> = world
        .get::<LaborAllocation>(member.entity)
        .map(|allocation| {
            allocation
                .assignments
                .iter()
                .filter(|row| row.for_tribe == Some(id))
                .filter(|row| {
                    !matches!(&row.target, LaborTarget::Tile { tile, branch }
                        if keep.iter().any(|(t, b)| t == tile && b == branch))
                })
                .map(|row| row.target.clone())
                .collect()
        })
        .unwrap_or_default();
    for target in stale {
        drop_levy_row(world, member.entity, &target);
    }
    for &(w, max) in wanted {
        let work = tribe.works[w];
        let target = LaborTarget::Tile {
            tile: work.tile,
            branch: work.branch,
        };
        let source = BuildSource::Tile {
            tile: work.tile,
            branch: work.branch,
        };
        // The improvement exists where a build is declared (the band's own `build` verb places it).
        if work.target_rung.is_some() {
            let mut improvements =
                world.resource_mut::<crate::tile_improvement::TileImprovementRegistry>();
            if improvements.get(work.tile, work.branch).is_none() {
                improvements.place_owned(
                    work.tile,
                    work.branch,
                    faction,
                    Some(member.band),
                    &scene.ladder,
                );
            }
        }
        let held_at_or_above = work.target_rung.is_some_and(|rung| {
            world
                .resource::<crate::tile_improvement::TileImprovementRegistry>()
                .get(work.tile, work.branch)
                .is_some_and(|improvement| scene.ladder.at_or_above(improvement.held_rung(), rung))
        });
        let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) else {
            continue;
        };
        allocation.set_staffed_row(target.clone(), max, None);
        allocation.set_source_priority(&target, work.priority);
        if let Some(row) = allocation
            .assignments
            .iter_mut()
            .find(|row| row.target.same_source(&target))
        {
            row.for_tribe = Some(id);
        }
        match work.target_rung {
            Some(rung) if !held_at_or_above => {
                allocation.declare_build(source, BuildJob::Climb(rung));
            }
            _ => {
                allocation.stop_build(&source);
            }
        }
    }
}

/// Take a levy row off a member: its declared build goes with the row, its walkers go home, and the
/// progress stays on the tile's own meter.
fn drop_levy_row(world: &mut World, entity: Entity, target: &LaborTarget) {
    let Some(mut allocation) = world.get_mut::<LaborAllocation>(entity) else {
        return;
    };
    if let LaborTarget::Tile { tile, branch } = target {
        allocation.stop_build(&BuildSource::Tile {
            tile: *tile,
            branch: *branch,
        });
    }
    if let Some(mut row) = allocation.drop_source_row(target) {
        if let Some(party) = row.party.take() {
            let walks = party.walk_home(target);
            allocation.homeward.extend(walks);
        }
    }
}

/// Make a band's Scout row carry the tribe's range and the stretch (or, with `tribe == None`, hand
/// it back: the range stays on the row, the stretch returns to a whole ring).
fn sync_scout_row(
    world: &mut World,
    entity: Entity,
    tribe: Option<TribeId>,
    range: u32,
    stretch: f32,
) {
    let Some(mut allocation) = world.get_mut::<LaborAllocation>(entity) else {
        return;
    };
    if tribe.is_some() && allocation.scouting_range_tiles() != range {
        allocation.set_scouting_range(range, None);
    }
    if let Some(row) = allocation
        .assignments
        .iter_mut()
        .find(|row| matches!(row.target, LaborTarget::Scout))
    {
        row.for_tribe = tribe;
        row.scouting_stretch = stretch;
    }
}

/// Hand bands that are in no tribe back: levy rows of a tribe they are no longer in are dropped
/// (their builds with them) and their Scout row keeps its range as a band's own.
fn release_non_members(world: &mut World) {
    let registry = world.resource::<TribeRegistry>().clone();
    let bands: Vec<(Entity, BandId)> = world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(world)
        .map(|(entity, band)| (entity, *band))
        .collect();
    for (entity, band) in bands {
        let tribe = registry.tribe_of(band).map(|tribe| tribe.id);
        let stale: Vec<LaborTarget> = world
            .get::<LaborAllocation>(entity)
            .map(|allocation| {
                allocation
                    .assignments
                    .iter()
                    .filter(|row| row.for_tribe.is_some() && row.for_tribe != tribe)
                    .filter(|row| !matches!(row.target, LaborTarget::Scout))
                    .map(|row| row.target.clone())
                    .collect()
            })
            .unwrap_or_default();
        for target in stale {
            drop_levy_row(world, entity, &target);
        }
        if tribe.is_none() {
            sync_scout_row(world, entity, None, 0, UNSTRETCHED);
        }
    }
}

/// **THE TRIBE PASS, EVERY TURN**: runs at the end of the Population stage after `advance_tribes`,
/// preparing the rows the next labor pass staffs.
pub fn advance_tribe_levy(world: &mut World) {
    release_non_members(world);
    let ids: Vec<TribeId> = world
        .resource::<TribeRegistry>()
        .iter()
        .map(|t| t.id)
        .collect();
    for id in ids {
        apply_tribe_pass(world, id);
        refresh_levy_workers(world, id);
    }
}

/// Record how many heads each carrier's levy row holds (read by the wire).
pub fn refresh_levy_workers(world: &mut World, id: TribeId) {
    let Some(tribe) = world.resource::<TribeRegistry>().get(id).cloned() else {
        return;
    };
    let members = resident_members(world, &tribe.members);
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let mut carriers = tribe.carriers.clone();
    for (w, chain) in carriers.iter_mut().enumerate() {
        let Some(work) = tribe.works.get(w) else {
            continue;
        };
        let road_source =
            is_road_branch(&ladder, work.branch).then_some(BuildSource::Road(work.tile));
        for carrier in chain.iter_mut() {
            let allocation = members
                .iter()
                .find(|m| m.band == carrier.band)
                .and_then(|m| world.get::<LaborAllocation>(m.entity));
            carrier.workers = match (&road_source, allocation) {
                // A road work has no row: its heads are the build's builders.
                (Some(source), Some(allocation)) => {
                    allocation.build_crew(source).map_or(0, |crew| crew.total())
                }
                (None, Some(allocation)) => {
                    levy_row_of(allocation, work.tile, work.branch).map_or(0, |row| row.workers)
                }
                (_, None) => 0,
            };
        }
    }
    if let Some(stored) = world.resource_mut::<TribeRegistry>().get_mut(id) {
        stored.carriers = carriers;
    }
}

/// Re-run the pass for a tribe and restaff every member: what every tribe command ends with.
fn rerun_and_restaff(world: &mut World, id: TribeId) {
    apply_tribe_pass(world, id);
    let members: BTreeSet<BandId> = world
        .resource::<TribeRegistry>()
        .get(id)
        .map(|t| t.members.clone())
        .unwrap_or_default();
    let entities = resident_members(world, &members);
    for member in &entities {
        restaff_band(world, member.entity);
    }
    refresh_levy_workers(world, id);
}

/// The refusal a tribe command gives (`token` is the stable word, `message` the player's sentence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TribeRefusal {
    pub token: &'static str,
    pub message: String,
}

fn refuse<T>(token: &'static str, message: impl Into<String>) -> Result<T, TribeRefusal> {
    Err(TribeRefusal {
        token,
        message: message.into(),
    })
}

/// The tribe the band names, for the band's own faction.
fn tribe_of_band(
    world: &mut World,
    faction: FactionId,
    band: BandId,
) -> Result<TribeId, TribeRefusal> {
    let owned = world
        .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(world)
        .any(|(id, cohort)| *id == band && cohort.faction == faction);
    if !owned {
        return refuse(
            "no_such_band",
            format!(
                "{} is not one of your bands.",
                crate::systems::band_label(band)
            ),
        );
    }
    match world.resource::<TribeRegistry>().tribe_of(band) {
        Some(tribe) => Ok(tribe.id),
        None => refuse(
            "band_not_in_tribe",
            format!("{} is not in a tribe.", crate::systems::band_label(band)),
        ),
    }
}

/// **`tribe_work <faction> <band> <branch> <x> <y> [<target_rung>] [high|normal|low]`** - add or
/// update a work on the tribe's list. Tile-kind branches and the road branch (a road work needs its
/// `<branch>:<rung>`); with a `target_rung` the work declares a build to that rung through the same
/// gate the band's own build verb on the site runs.
#[allow(clippy::too_many_arguments)]
pub fn tribe_work(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    branch: &str,
    tile: UVec2,
    target_rung: Option<&str>,
    priority: Option<SourcePriority>,
) -> Result<String, TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let branch_id = BranchId::new(branch);
    let is_road = is_road_branch(&ladder, branch_id);
    if !is_road && ladder.source_kind(branch_id) != Some(SourceKind::Tile) {
        return refuse(
            "not_a_tribe_work",
            format!("A tribe's works are improvements or roads of a tile; `{branch}` is neither."),
        );
    }
    let rung = match target_rung {
        None => None,
        Some(token) => {
            let Ok(rung) = token.parse::<RungId>() else {
                return refuse("unknown_rung", format!("`{token}` is not a rung."));
            };
            if rung.branch != branch_id || ladder.rung_by_id(rung).is_none() {
                return refuse(
                    "unknown_rung",
                    format!("`{token}` is not a rung of `{branch}`."),
                );
            }
            Some(rung)
        }
    };
    if is_road && rung.is_none() {
        return refuse(
            "road_needs_rung",
            format!("A road work names the rung to build to, like `{branch}:<rung>`."),
        );
    }
    if let Some(rung) = rung {
        let def = ladder.rung(rung).clone();
        let gate = if is_road {
            road_work_gate(world, id, faction, tile, &def)
        } else {
            crate::build_gates::with_scene(world, |scene| {
                crate::build_gates::validate_tile_verb(scene, faction, tile, branch_id, &def)
            })
        };
        if let Err(message) = gate {
            return refuse("build_refused", message);
        }
    }
    // A member's existing Tile row on the site is adopted, and its declared build carries over.
    let adopted_rung = if rung.is_none() && !is_road {
        declared_rung_on_site(world, id, tile, branch_id)
    } else {
        None
    };
    if !is_road {
        adopt_member_rows(world, id, tile, branch_id);
    }
    let mut registry = world.resource_mut::<TribeRegistry>();
    let Some(tribe) = registry.get_mut(id) else {
        return refuse("band_not_in_tribe", "That tribe is gone.");
    };
    let existing = tribe
        .works
        .iter()
        .position(|work| work.same_site(tile, branch_id));
    let name = match existing {
        Some(index) => {
            let work = &mut tribe.works[index];
            if rung.is_some() {
                work.target_rung = rung;
            }
            if let Some(priority) = priority {
                work.priority = priority;
            }
            "updates"
        }
        None => {
            tribe.works.push(TribeWork {
                tile,
                branch: branch_id,
                target_rung: rung.or(adopted_rung),
                priority: priority.unwrap_or_default(),
            });
            "takes on"
        }
    };
    let tribe_name = tribe.name.clone();
    rerun_and_restaff(world, id);
    Ok(format!(
        "The {tribe_name} {name} the {branch} at ({}, {}).",
        tile.x, tile.y
    ))
}

/// The gate a road work's carrier would meet if it declared the build itself: the band's own
/// `grade` / `pave` gate, asked of the band the tribe would hand the tile to.
fn road_work_gate(
    world: &mut World,
    id: TribeId,
    faction: FactionId,
    tile: UVec2,
    def: &crate::intensification::RungDef,
) -> Result<(), String> {
    let Some(scene) = pass_scene(world) else {
        return Err("There is no map to build a road on.".to_string());
    };
    let members = world
        .resource::<TribeRegistry>()
        .get(id)
        .map(|tribe| tribe.members.clone())
        .unwrap_or_default();
    let found = resident_members(world, &members);
    let Some(carrier) = road_carrier(world, &found, tile, scene.grid.0, scene.wrap) else {
        return Err("The tribe has no band to carry the road.".to_string());
    };
    crate::build_gates::validate_road_verb(world, faction, carrier, tile, def)
}

/// **ADOPT** every member's own Tile row on the site: it becomes a levy row of the tribe, which the
/// pass then keeps on the member that should carry it and drops from the rest (its declared build
/// goes with a dropped row; the progress stays on the tile's meter).
fn adopt_member_rows(world: &mut World, id: TribeId, tile: UVec2, branch: BranchId) {
    let Some(members) = world
        .resource::<TribeRegistry>()
        .get(id)
        .map(|tribe| tribe.members.clone())
    else {
        return;
    };
    for member in resident_members(world, &members) {
        let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) else {
            continue;
        };
        if let Some(row) = allocation.assignments.iter_mut().find(|row| {
            matches!(&row.target, LaborTarget::Tile { tile: t, branch: b } if *t == tile && *b == branch)
        }) {
            row.for_tribe = Some(id);
        }
    }
}

/// The rung a member's declared build on the site climbs to, if a member band holds one.
fn declared_rung_on_site(
    world: &mut World,
    id: TribeId,
    tile: UVec2,
    branch: BranchId,
) -> Option<RungId> {
    let members = world.resource::<TribeRegistry>().get(id)?.members.clone();
    let found = resident_members(world, &members);
    let source = BuildSource::Tile { tile, branch };
    found.iter().find_map(|m| {
        let allocation = world.get::<LaborAllocation>(m.entity)?;
        match allocation.build_declaration(&source)?.declared {
            BuildJob::Climb(rung) => Some(rung),
            _ => None,
        }
    })
}

/// **`tribe_work <faction> <band> stop <branch> <x> <y>`** - remove a work and drop its levy rows.
pub fn tribe_work_stop(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    branch: &str,
    tile: UVec2,
) -> Result<String, TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let branch_id = BranchId::new(branch);
    let mut registry = world.resource_mut::<TribeRegistry>();
    let Some(tribe) = registry.get_mut(id) else {
        return refuse("band_not_in_tribe", "That tribe is gone.");
    };
    let Some(index) = tribe
        .works
        .iter()
        .position(|work| work.same_site(tile, branch_id))
    else {
        return refuse(
            "no_such_work",
            format!(
                "The tribe has no {branch} work at ({}, {}).",
                tile.x, tile.y
            ),
        );
    };
    tribe.works.remove(index);
    if index < tribe.carriers.len() {
        tribe.carriers.remove(index);
    }
    let tribe_name = tribe.name.clone();
    let members = tribe.members.clone();
    if is_road_branch(
        &world.resource::<crate::LadderConfigHandle>().get(),
        branch_id,
    ) {
        // The tribe's declared build goes; the road and its keeper stay as they are.
        for member in resident_members(world, &members) {
            if let Some(mut allocation) = world.get_mut::<LaborAllocation>(member.entity) {
                allocation.stop_build(&BuildSource::Road(tile));
            }
        }
    }
    rerun_and_restaff(world, id);
    Ok(format!(
        "The {tribe_name} gives up the {branch} at ({}, {}).",
        tile.x, tile.y
    ))
}

/// **`tribe_levy <faction> <band> none|low|normal|high`** - set the tribe's levy ceiling.
pub fn tribe_levy(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    level: &str,
) -> Result<String, TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let Some(ceiling) = LevyCeiling::parse(level) else {
        return refuse(
            "unknown_levy",
            format!("`{level}` is not a levy level (none, low, normal or high)."),
        );
    };
    let mut registry = world.resource_mut::<TribeRegistry>();
    let Some(tribe) = registry.get_mut(id) else {
        return refuse("band_not_in_tribe", "That tribe is gone.");
    };
    tribe.levy = ceiling;
    let tribe_name = tribe.name.clone();
    rerun_and_restaff(world, id);
    Ok(format!(
        "The {tribe_name} levies its bands at {}.",
        ceiling.token()
    ))
}

/// **`tribe_scout <faction> <band> <N>`** - set the tribe's one scouting range, clamped to
/// `labor_config.scout.range_past_local_max` exactly as `assign_labor ... scout` clamps. Returns the
/// echo and the range that was set.
pub fn tribe_scout(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    range: u32,
) -> Result<(String, u32), TribeRefusal> {
    let id = tribe_of_band(world, faction, band)?;
    let max = world
        .resource::<crate::labor_config::LaborConfigHandle>()
        .get()
        .scout
        .range_past_local_max;
    let set = range.min(max);
    let mut registry = world.resource_mut::<TribeRegistry>();
    let Some(tribe) = registry.get_mut(id) else {
        return refuse("band_not_in_tribe", "That tribe is gone.");
    };
    tribe.scouting_range = set;
    let tribe_name = tribe.name.clone();
    rerun_and_restaff(world, id);
    let note = if set < range {
        format!(" (the most it can name is {max})")
    } else {
        String::new()
    };
    Ok((
        format!("The {tribe_name} scouts {set} tiles past its local range{note}."),
        set,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camp(band: u64, x: u32, y: u32) -> (BandId, UVec2) {
        (BandId(band), UVec2::new(x, y))
    }

    const GRID: (u32, u32) = (64, 64);
    const REACH: u32 = 3;

    #[test]
    fn a_lone_band_reads_a_whole_ring() {
        let stretches = scouting_stretches(&[camp(1, 30, 30)], REACH, GRID, false);
        assert_eq!(stretches[&BandId(1)], 1.0);
    }

    #[test]
    fn two_overlapping_members_sum_to_their_unions_edge_over_one_ring() {
        let camps = [camp(1, 30, 30), camp(2, 33, 30)];
        let stretches = scouting_stretches(&camps, REACH, GRID, false);
        let total: f32 = stretches.values().sum();
        // The union's edge, counted directly.
        let mut union = BTreeSet::new();
        for (_, c) in &camps {
            for t in hex_range_tiles(*c, REACH, GRID.0, GRID.1, false) {
                union.insert((t.y, t.x));
            }
        }
        let edge = union
            .iter()
            .filter(|(y, x)| {
                hex_neighbors_wrapped(*x, *y, GRID.0, GRID.1, false)
                    .any(|(nx, ny)| !union.contains(&(ny, nx)))
            })
            .count() as f32;
        assert!((total - edge / (6.0 * REACH as f32)).abs() < 1e-6);
        assert!(stretches.values().all(|s| *s > 0.0 && *s < 1.0 + 1e-6));
        assert!(
            total > 1.0,
            "two spread bands watch more edge than one ring"
        );
    }

    #[test]
    fn a_member_enclosed_by_the_others_reads_nothing() {
        // Band 1 stands at the centre of a ring of five others, one tile from each: its own disc
        // lies inside theirs.
        let centre = camp(1, 30, 30);
        let mut camps = vec![centre];
        for (i, (dx, dy)) in [(2i32, 0i32), (-2, 0), (1, 2), (-1, 2), (1, -2), (-1, -2)]
            .iter()
            .enumerate()
        {
            camps.push(camp(10 + i as u64, (30 + dx) as u32, (30 + dy) as u32));
        }
        let stretches = scouting_stretches(&camps, REACH, GRID, false);
        assert_eq!(stretches[&BandId(1)], 0.0);
    }

    #[test]
    fn the_levy_budget_is_the_scaled_share_and_never_below_one_while_allowed() {
        assert_eq!(crate::staffing::levy_budget(0.5, 1.0, 10), 5);
        assert_eq!(crate::staffing::levy_budget(0.5, 0.5, 10), 2);
        assert_eq!(
            crate::staffing::levy_budget(0.25, 0.25, 3),
            1,
            "at least one head"
        );
        assert_eq!(
            crate::staffing::levy_budget(0.0, 1.0, 10),
            0,
            "none takes nothing"
        );
        assert_eq!(
            crate::staffing::levy_budget(1.0, 1.0, 0),
            0,
            "no hands left"
        );
    }
}
