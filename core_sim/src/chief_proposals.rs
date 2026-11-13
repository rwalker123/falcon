//! **A CHIEF PROPOSES A BUILD; THE PLAYER APPROVES IT.** The chief never declares one himself.
//!
//! Whenever a band has a leader and idle hands (`BandWorkforce::idle() > 0`), [`refresh_proposals`]
//! offers **one** build: the best of every build the band could declare right now, judged by the
//! existing gates ([`crate::build_gates`], the very functions `build <verb>` runs), ranked food-first
//! (a build that adds or saves food, most food first - storage's figure is the band's `storageSaves`
//! gated by `band_orders.storage_build_min_saved`), then the rest in ladder order. A candidate short of
//! a material the band can gather is still eligible and carries the keep-target raise for it.
//!
//! The proposal is published on the band's cohort row (`chiefProposals`) with its words already
//! resolved ([`ChiefProposalState::text`]); the client only renders. `approve_proposal` re-validates
//! (the proposal may be stale), declares the build exactly as a player's declaration would and raises
//! the keep order in the same breath; `defer_proposal` ("Not now") hides that build for
//! `band_orders.proposal_not_now_turns` turns. An approved build that COMPLETES counts once as an order
//! met for the band's leader ([`ChiefProposalBook::met_builds_ready`], read by
//! `leadership::judge_leader`). Candidates are the band-kind builds (the drying rack) and the tile-kind
//! builds on the band's own tile (the storage pit, a monument).

use std::collections::BTreeMap;

use bevy::ecs::world::World;
use bevy::math::UVec2;
use bevy::prelude::{Entity, Resource, With};
use serde::{Deserialize, Serialize};
use sim_schema::{ChiefProposalState, KeepRaiseState};

use crate::band_spoilage::{band_spoilage_of, BandSpoilInputs};
use crate::build_gates::{validate_band_verb, validate_tile_verb};
use crate::components::{
    BandBench, BandId, BandName, BandWorkforce, BuildJob, BuildSource, LaborAllocation,
    LaborTarget, PopulationCohort, ResidentBand, Tile,
};
use crate::demographics_config::DemographicsConfigHandle;
use crate::fauna::HerdRegistry;
use crate::fauna_config::FaunaConfigHandle;
use crate::flora_config::FloraConfigHandle;
use crate::forage::ForageRegistry;
use crate::intensification::{
    BranchId, LadderConfig, LadderConfigHandle, RungDef, RungId, SourceKind,
};
use crate::keep_on_hand::Keeper;
use crate::labor_config::LaborConfigHandle;
use crate::leadership::BandLeadership;
use crate::orders::FactionId;
use crate::resources::{
    CommandEventLog, DiscoveryProgressLedger, SimulationConfig, SimulationTick, TileRegistry,
};
use crate::spoilage::PreservationScene;
use crate::supply_network_config::SupplyNetworkConfigHandle;
use crate::tile_improvement::TileImprovementRegistry;
use crate::{CommandEventEntry, CommandEventKind};

/// A build an approval is waiting on: tracked so its completion is credited once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovedBuild {
    pub branch: BranchId,
    pub dest: RungId,
    /// The tile of a tile-kind build; `None` for a band-kind one.
    pub tile: Option<UVec2>,
    /// The herd id of a herd build.
    pub herd: Option<String>,
}

/// One band's proposal state. Saved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BandProposals {
    /// Consecutive turns the band has had idle hands.
    pub idle_turns: u32,
    /// "Not now": build key -> the tick it comes back at.
    pub deferred: BTreeMap<String, u64>,
    /// Approved builds not yet completed.
    pub approved: Vec<ApprovedBuild>,
    /// Builds approved here that completed this turn (credited to the leader next turn).
    pub met_builds_fresh: u32,
    /// Builds the leader's next judgement counts as orders met, once.
    pub met_builds_ready: u32,
    /// The proposal on offer now (zero or one).
    pub current: Option<ChiefProposalState>,
}

/// Every band's proposal state (`SimState`, saved).
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChiefProposalBook {
    pub bands: BTreeMap<BandId, BandProposals>,
}

impl ChiefProposalBook {
    /// The builds the leader's next judgement counts as orders met.
    pub fn met_builds_ready(&self, band: BandId) -> u32 {
        self.bands
            .get(&band)
            .map_or(0, |state| state.met_builds_ready)
    }

    /// The proposal on offer to `band`.
    pub fn current(&self, band: BandId) -> Option<&ChiefProposalState> {
        self.bands
            .get(&band)
            .and_then(|state| state.current.as_ref())
    }

    /// Credits the leader has been judged on become spent, and this turn's completions become next
    /// turn's judgement. Called once a turn at the foot of `advance_leadership`.
    pub fn rotate_met_builds(&mut self) {
        for state in self.bands.values_mut() {
            state.met_builds_ready = std::mem::take(&mut state.met_builds_fresh);
        }
    }
}

/// One build the band could declare now.
struct Candidate {
    key: String,
    branch: BranchId,
    dest: RungId,
    tile: Option<UVec2>,
    target: LaborTarget,
    /// Where it stands in the ladder file: branch index then rung order.
    ladder_order: (usize, u32),
    food_saved: Option<f32>,
    raises: Vec<KeepRaiseState>,
    phrase: String,
    rung_key: String,
    verb: String,
}

/// What a band is, read once.
struct BandView {
    entity: Entity,
    id: BandId,
    faction: FactionId,
}

fn resident_bands(world: &mut World) -> Vec<BandView> {
    let mut query =
        world.query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>();
    let mut bands: Vec<BandView> = query
        .iter(world)
        .map(|(entity, id, cohort)| BandView {
            entity,
            id: *id,
            faction: cohort.faction,
        })
        .collect();
    bands.sort_by_key(|band| band.id);
    bands
}

fn band_tile(world: &World, cohort: &PopulationCohort) -> Option<UVec2> {
    world
        .get::<Tile>(cohort.current_tile)
        .map(|tile| tile.position)
}

fn source_of(target: &LaborTarget) -> Option<BuildSource> {
    BuildSource::of(target)
}

fn target_for(
    ladder: &LadderConfig,
    branch: BranchId,
    band: BandId,
    at: Option<UVec2>,
) -> Option<LaborTarget> {
    match ladder.source_kind(branch)? {
        SourceKind::Band => Some(LaborTarget::Band { band, branch }),
        SourceKind::Tile => at.map(|tile| LaborTarget::Tile { tile, branch }),
        _ => None,
    }
}

/// The build key `defer_proposal` names: the rung's wire key, with the tile for a tile-kind build.
fn build_key(rung: &RungDef, tile: Option<UVec2>) -> String {
    match tile {
        Some(tile) => format!("{}@{},{}", rung.wire_key(), tile.x, tile.y),
        None => rung.wire_key(),
    }
}

/// Every build `band` could declare right now, with its food figure and keep raises. The eligibility
/// is the gates' ([`crate::build_gates`]), never a second check here.
fn candidates(world: &World, view: &BandView) -> Vec<Candidate> {
    let Some(cohort) = world.get::<PopulationCohort>(view.entity) else {
        return Vec::new();
    };
    let Some(allocation) = world.get::<LaborAllocation>(view.entity) else {
        return Vec::new();
    };
    let ladder = world.resource::<LadderConfigHandle>().get();
    let labor = world.resource::<LaborConfigHandle>().get();
    let demographics = world.resource::<DemographicsConfigHandle>().get();
    let fauna = world.resource::<FaunaConfigHandle>().get();
    let flora = world.resource::<FloraConfigHandle>().get();
    let sim_config = world.resource::<SimulationConfig>();
    let tile_registry = world.resource::<TileRegistry>();
    let herds = world.resource::<HerdRegistry>();
    let forage = world.resource::<ForageRegistry>();
    let discovery = world.resource::<DiscoveryProgressLedger>();
    let improvements = world.resource::<TileImprovementRegistry>();
    let reach_tiles = world
        .get_resource::<SupplyNetworkConfigHandle>()
        .map_or(0, |handle| handle.get().reach_tiles);
    let at = band_tile(world, cohort);
    let scene = PreservationScene {
        keeping: &demographics.keeping,
        ladder: &ladder,
        improvements,
        reach_tiles,
        grid: (tile_registry.width, sim_config.map_topology.wrap_horizontal),
    };
    let composition = |tile: UVec2| {
        let entity = tile_registry.index(tile.x, tile.y)?;
        let ground = world.get::<Tile>(entity)?;
        Some(
            crate::forage::tile_flora_composition(
                &flora,
                &labor.forage,
                ground,
                sim_config.map_seed,
            )
            .into_owned(),
        )
    };
    let held = scene.held_by(Some(view.id));
    let stores = crate::fixed_store::reachable_stores(&scene, Some(view.id), cohort.faction, at);
    let spoilage = band_spoilage_of(&BandSpoilInputs {
        cohort,
        allocation,
        band: Some(view.id),
        at,
        demographics: &demographics,
        fauna: &fauna,
        flora: &flora,
        labor: &labor,
        herds,
        forage,
        composition: &|tile| composition(tile),
        ladder: &ladder,
        improvements,
        held: &held,
        stores: &stores,
        knows: &|discovery_id| {
            crate::intensification::knows(
                discovery,
                cohort.faction,
                discovery_id,
                ladder.knowledge.completion_threshold,
            )
        },
    });
    let threshold = labor.band_orders.storage_build_min_saved;
    let mut found = Vec::new();
    crate::build_gates::with_scene(world, |gates| {
        for (branch_index, branch) in ladder.branches_in_config_order().into_iter().enumerate() {
            let Some(target) = target_for(&ladder, branch, view.id, at) else {
                continue;
            };
            let tile = match &target {
                LaborTarget::Tile { tile, .. } => Some(*tile),
                _ => None,
            };
            let held_rung = match &target {
                LaborTarget::Band { band, .. } => improvements
                    .bands()
                    .get(*band, branch)
                    .map(|held| held.held_rung()),
                LaborTarget::Tile { tile, .. } => {
                    improvements.get(*tile, branch).map(|held| held.held_rung())
                }
                _ => None,
            };
            let Some(current) = held_rung.or_else(|| ladder.root_of(branch).map(RungDef::rung_id))
            else {
                continue;
            };
            let Some(rung) = ladder.above_rung(ladder.rung(current)) else {
                continue;
            };
            let (Some(verb), Some(build)) = (rung.verb.as_deref(), rung.build.as_ref()) else {
                continue;
            };
            let Some(source) = source_of(&target) else {
                continue;
            };
            if allocation.has_build(&source) {
                continue;
            }
            let eligible = match &target {
                LaborTarget::Band { band, .. } => {
                    validate_band_verb(gates, view.faction, *band, branch, rung)
                }
                LaborTarget::Tile { tile, .. } => {
                    validate_tile_verb(gates, view.faction, *tile, branch, rung)
                }
                _ => continue,
            };
            if eligible.is_err() {
                continue;
            }
            // **A storage rung's figure is the band's own `storageSaves` row**, and it is gated by the
            // lever: a rack that would save next to nothing is not proposed.
            let rung_key = rung.wire_key();
            let save = spoilage
                .storage_saves
                .iter()
                .find(|row| row.rung == rung_key);
            let food_saved = match save {
                Some(row) if row.lesson_known && !row.declared && row.food_saved >= threshold => {
                    Some(row.food_saved)
                }
                Some(_) => continue,
                None => None,
            };
            let Some(raises) = keep_raises(cohort, allocation, &labor, &build.materials) else {
                continue;
            };
            found.push(Candidate {
                key: build_key(rung, tile),
                branch,
                dest: rung.rung_id(),
                tile,
                target,
                ladder_order: (branch_index, rung.order),
                food_saved,
                raises,
                phrase: proposal_phrase(rung),
                rung_key,
                verb: verb.to_string(),
            });
        }
    });
    // **Rung upgrades on sources the band ALREADY works**: its crew row exists, so declaring the build
    // is what a player's `build <verb>` on that source does. The band never proposes starting work on
    // a source it does not hold a row on.
    let mut seen_sources: Vec<BuildSource> = Vec::new();
    for row in &allocation.assignments {
        if !(row.workers > 0 || row.target.is_staffed()) {
            continue;
        }
        let (kind, tile, herd, material) = match &row.target {
            LaborTarget::Forage { tile, .. } => (SourceKind::Patch, Some(*tile), None, None),
            LaborTarget::Hunt { fauna_id, .. } => {
                (SourceKind::Herd, None, Some(fauna_id.clone()), None)
            }
            LaborTarget::Extract { tile, material, .. } => (
                SourceKind::Deposit,
                Some(*tile),
                None,
                Some(material.clone()),
            ),
            _ => continue,
        };
        let Some(source) = source_of(&row.target) else {
            continue;
        };
        if seen_sources.contains(&source) || allocation.has_build(&source) {
            continue;
        }
        seen_sources.push(source);
        for (branch_index, branch) in ladder.branches_in_config_order().into_iter().enumerate() {
            if ladder.source_kind(branch) != Some(kind) {
                continue;
            }
            for rung in ladder.rungs_of(branch) {
                let (Some(verb), Some(build)) = (rung.verb.as_deref(), rung.build.as_ref()) else {
                    continue;
                };
                let Some(verb_id) = rung.verb_id() else {
                    continue;
                };
                if crate::build_gates::validate_improvement(
                    world,
                    view.faction,
                    &row.target,
                    verb_id,
                )
                .is_err()
                {
                    continue;
                }
                let Some(raises) = keep_raises(cohort, allocation, &labor, &build.materials) else {
                    continue;
                };
                let food_saved = source_food_gain(
                    world,
                    &ladder,
                    &labor,
                    &flora,
                    sim_config.map_seed,
                    &row.target,
                    rung,
                );
                let mut key = rung.wire_key();
                if let Some(tile) = tile {
                    key.push_str(&format!("@{},{}", tile.x, tile.y));
                }
                if let Some(herd) = &herd {
                    key.push_str(&format!("@{herd}"));
                }
                if let Some(material) = &material {
                    key.push_str(&format!(":{material}"));
                }
                found.push(Candidate {
                    key,
                    branch,
                    dest: rung.rung_id(),
                    tile,
                    target: row.target.clone(),
                    ladder_order: (branch_index, rung.order),
                    food_saved,
                    raises,
                    phrase: proposal_phrase(rung),
                    rung_key: rung.wire_key(),
                    verb: verb.to_string(),
                });
            }
        }
    }
    found
}

/// **A shortfall of a material the band can gather is carried as a keep raise**; one it cannot gather
/// makes the build ineligible (`None`).
fn keep_raises(
    cohort: &PopulationCohort,
    allocation: &LaborAllocation,
    labor: &crate::labor_config::LaborConfig,
    materials: &BTreeMap<String, f32>,
) -> Option<Vec<KeepRaiseState>> {
    let mut raises = Vec::new();
    for (material, need) in materials {
        let on_hand = cohort.stores.material_total(material).to_f32();
        if on_hand >= *need {
            continue;
        }
        let order = allocation.order_for(material);
        let from = order
            .map(|order| order.target)
            .or_else(|| labor.band_orders.starting_target_for(material))?;
        let player_owned =
            order.is_some_and(|order| order.owner == crate::components::OrderOwner::Player);
        if !labor.band_orders.is_ordered(material) || player_owned {
            return None;
        }
        raises.push(KeepRaiseState {
            material: material.clone(),
            from,
            to: from + need.ceil() as u32,
        });
    }
    Some(raises)
}

/// **The food a turn a source build adds**, from the figures the staffing's own food-first check reads
/// (`staffing::{patch,herd}_build_finish`): a patch's `forage::rung_payoff` at the destination rung less
/// the held one, a herd's sustained MSY at the destination block less the held block at the species'
/// hunt yield. `None` where the rung adds no food (and for a deposit, which yields none).
fn source_food_gain(
    world: &World,
    ladder: &LadderConfig,
    labor: &crate::labor_config::LaborConfig,
    flora: &crate::flora_config::FloraConfig,
    map_seed: u64,
    target: &LaborTarget,
    rung: &RungDef,
) -> Option<f32> {
    let gain = match target {
        LaborTarget::Forage { tile, .. } => {
            let patch = world.resource::<ForageRegistry>().patch(*tile)?;
            let registry = world.resource::<TileRegistry>();
            let ground = world.get::<Tile>(registry.index(tile.x, tile.y)?)?;
            let composition =
                crate::forage::tile_flora_composition(flora, &labor.forage, ground, map_seed)
                    .into_owned();
            let payoff_at = |payoff| {
                crate::forage::rung_payoff(
                    patch,
                    &composition,
                    &labor.forage,
                    flora,
                    UNBOOSTED_OUTPUT,
                    payoff,
                )
            };
            let finished = crate::forage::PlantPayoff::of(rung.trait_set());
            let held = crate::forage::PlantPayoff::of(patch.standing().traits);
            payoff_at(finished) - payoff_at(held)
        }
        LaborTarget::Hunt { fauna_id, .. } => {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let herd = world.resource::<HerdRegistry>().find(fauna_id)?;
            let yield_per_biomass =
                crate::fauna::herd_hunt_yield(herd, &fauna).provisions_per_biomass;
            let msy_at = |payoff| crate::fauna::herd_msy_biomass_at(herd, &fauna, payoff);
            let finished = crate::fauna::HerdPayoff::of(rung.trait_set());
            let held = crate::fauna::HerdPayoff::of(herd.standing().traits);
            (msy_at(finished) - msy_at(held)) * yield_per_biomass * UNBOOSTED_OUTPUT
        }
        _ => return None,
    };
    let _ = ladder;
    (gain > NO_FOOD).then_some(gain)
}

/// The productivity multiplier a rung quote is struck at: the identity.
const UNBOOSTED_OUTPUT: f32 = 1.0;
/// No food added.
const NO_FOOD: f32 = 0.0;

/// "dig a storage pit": the rung's configured phrase, else its verb and its article'd name.
fn proposal_phrase(rung: &RungDef) -> String {
    rung.proposal_phrase.clone().unwrap_or_else(|| {
        format!(
            "{} {}",
            rung.verb.as_deref().unwrap_or("build"),
            rung.label_fallback
        )
    })
}

/// The ranking: food first, most food first; then the rest in ladder order.
fn rank(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| {
        let food = |c: &Candidate| c.food_saved.unwrap_or(f32::NEG_INFINITY);
        food(b)
            .total_cmp(&food(a))
            .then(a.ladder_order.cmp(&b.ladder_order))
            .then(a.key.cmp(&b.key))
    });
}

/// "3 idle hands" / "3 hands idle 4 turns" / "3 hands wasted for 9 turns".
fn hands_phrase(escalation: u32, idle: u32, idle_turns: u32) -> String {
    let hands = if idle == 1 { "hand" } else { "hands" };
    let turns = if idle_turns == 1 { "turn" } else { "turns" };
    match escalation {
        0 => format!("{idle} idle {hands}"),
        1 => format!("{idle} {hands} idle {idle_turns} {turns}"),
        _ => format!("{idle} {hands} wasted for {idle_turns} {turns}"),
    }
}

/// The escalation rung for `idle_turns` against the two levers.
pub fn escalation_rung(idle_turns: u32, notice: u32, wasted: u32) -> u32 {
    if idle_turns >= wasted {
        2
    } else if idle_turns >= notice {
        1
    } else {
        0
    }
}

/// The fully resolved proposal row for the chosen candidate.
fn resolve(
    candidate: &Candidate,
    keeper: &Keeper,
    idle: u32,
    idle_turns: u32,
    escalation: u32,
) -> ChiefProposalState {
    let reason = candidate
        .food_saved
        .map(|saved| {
            if matches!(
                candidate.target,
                LaborTarget::Band { .. } | LaborTarget::Tile { .. }
            ) {
                format!("{saved:.1} food a turn spoiling")
            } else {
                format!("{saved:.1} more food a turn")
            }
        })
        .unwrap_or_default();
    let verb = if keeper.plural { "propose" } else { "proposes" };
    let mut text = format!(
        "{} \u{b7} {} {verb}: {}",
        hands_phrase(escalation, idle, idle_turns),
        keeper.subject,
        candidate.phrase
    );
    if !reason.is_empty() {
        text.push_str(&format!(" \u{2014} {reason}"));
    }
    if !candidate.raises.is_empty() {
        let raised: Vec<String> = candidate
            .raises
            .iter()
            .map(|raise| format!("{} keep to {}", raise.material, raise.to))
            .collect();
        text.push_str(&format!(" (raises {})", raised.join(" and ")));
    }
    ChiefProposalState {
        build_key: candidate.key.clone(),
        rung: candidate.rung_key.clone(),
        branch: candidate.branch.as_str().to_string(),
        verb: candidate.verb.clone(),
        phrase: candidate.phrase.clone(),
        has_tile: candidate.tile.is_some(),
        tile_x: candidate.tile.map_or(0, |tile| tile.x),
        tile_y: candidate.tile.map_or(0, |tile| tile.y),
        reason,
        food_saved: candidate.food_saved.unwrap_or(0.0),
        idle,
        idle_turns,
        escalation,
        keep_raises: candidate.raises.clone(),
        text,
    }
}

/// Has the approved build finished (the improvement or source stands at or above the rung it raised)?
fn approved_completed(world: &World, band: BandId, approved: &ApprovedBuild) -> bool {
    let ladder = world.resource::<LadderConfigHandle>().get();
    if let Some(herd) = &approved.herd {
        return world
            .resource::<HerdRegistry>()
            .find(herd)
            .is_some_and(|herd| {
                herd.standing()
                    .holds_at_or_above(ladder.rung(approved.dest))
            });
    }
    let improvements = world.resource::<TileImprovementRegistry>();
    let held = match approved.tile {
        Some(tile) => improvements
            .get(tile, approved.branch)
            .map(|held| held.held_rung()),
        None => improvements
            .bands()
            .get(band, approved.branch)
            .map(|held| held.held_rung()),
    };
    if let (None, Some(tile)) = (held, approved.tile) {
        // A patch build: no tile improvement, the patch itself holds the rung.
        return world
            .resource::<ForageRegistry>()
            .patch(tile)
            .is_some_and(|patch| {
                patch
                    .standing()
                    .holds_at_or_above(ladder.rung(approved.dest))
            });
    }
    held.is_some_and(|held| ladder.at_or_above(held, approved.dest))
}

/// Is the approved build still declared (so it is still in progress)?
fn approved_declared(world: &World, view: &BandView, approved: &ApprovedBuild) -> bool {
    let source = if let Some(herd) = &approved.herd {
        BuildSource::Herd(herd.clone())
    } else {
        match (
            approved.tile,
            world
                .resource::<LadderConfigHandle>()
                .get()
                .source_kind(approved.branch),
        ) {
            (Some(tile), Some(SourceKind::Patch)) => BuildSource::Patch(tile),
            (Some(tile), _) => BuildSource::Tile {
                tile,
                branch: approved.branch,
            },
            (None, _) => BuildSource::Band {
                band: view.id,
                branch: approved.branch,
            },
        }
    };
    world
        .get::<LaborAllocation>(view.entity)
        .is_some_and(|allocation| allocation.has_build(&source))
}

/// **THE PROPOSAL PASS**, run each turn after labor (`advance_chief_proposals`) and after any command
/// that changes what a proposal rests on. `advance` is true only for the turn's own pass: it moves the
/// idle streaks and credits completed approvals; a command's refresh leaves both alone.
pub fn refresh_proposals(world: &mut World, advance: bool) {
    let bands = resident_bands(world);
    let tick = world.resource::<SimulationTick>().0;
    let labor = world.resource::<LaborConfigHandle>().get();
    let orders = &labor.band_orders;
    world.resource_scope::<ChiefProposalBook, _>(|world, mut book| {
        book.bands
            .retain(|id, _| bands.iter().any(|band| band.id == *id));
        for view in &bands {
            let (idle, led, keeper) = {
                let (Some(cohort), Some(allocation)) = (
                    world.get::<PopulationCohort>(view.entity),
                    world.get::<LaborAllocation>(view.entity),
                ) else {
                    continue;
                };
                let workforce = BandWorkforce::resolve(
                    Some(cohort),
                    Some(allocation),
                    world.get::<BandBench>(view.entity),
                );
                let name = world
                    .get::<BandName>(view.entity)
                    .map_or_else(|| format!("Band {}", view.id.0), |name| name.0.clone());
                let keeper = Keeper::of(world.get::<BandLeadership>(view.entity), &name);
                (workforce.idle(), keeper.is_some(), keeper)
            };
            let state = book.bands.entry(view.id).or_default();
            if advance {
                state.idle_turns = if idle > 0 { state.idle_turns + 1 } else { 0 };
                let approved = std::mem::take(&mut state.approved);
                for build in approved {
                    if approved_completed(world, view.id, &build) {
                        state.met_builds_fresh += 1;
                    } else if approved_declared(world, view, &build) {
                        state.approved.push(build);
                    }
                }
            }
            state.deferred.retain(|_, until| *until > tick);
            state.current = None;
            let (true, Some(keeper)) = (led && idle > 0, keeper) else {
                continue;
            };
            let mut found = candidates(world, view);
            found.retain(|candidate| !state.deferred.contains_key(&candidate.key));
            rank(&mut found);
            if let Some(best) = found.first() {
                let escalation = escalation_rung(
                    state.idle_turns,
                    orders.proposal_idle_turns_notice,
                    orders.proposal_idle_turns_wasted,
                );
                state.current = Some(resolve(best, &keeper, idle, state.idle_turns, escalation));
            }
        }
    });
}

/// The Population-chain system: see the module documentation. Exclusive because the gates read the
/// world through a [`crate::build_gates::GateScene`] and the pass writes the book.
pub fn advance_chief_proposals(world: &mut World) {
    refresh_proposals(world, true);
}

/// A proposal that has been checked against the world as it stands now, ready to commit.
pub struct Approval {
    view_entity: Entity,
    candidate: Candidate,
}

impl Approval {
    /// For a build on a source the band already works (a patch, a herd, a deposit): the row's target
    /// and the rung to raise. The caller declares it through the path a player's `build <verb>` on that
    /// source takes and then calls [`commit_approval`] with `declared = true`. `None` for a band or tile
    /// improvement, which [`commit_approval`] declares itself.
    pub fn source_build(&self) -> Option<(&LaborTarget, RungId)> {
        match self.candidate.target {
            LaborTarget::Band { .. } | LaborTarget::Tile { .. } => None,
            _ => Some((&self.candidate.target, self.candidate.dest)),
        }
    }
}

/// **CHECK THE BAND'S PROPOSAL AGAINST THE WORLD AS IT STANDS** (the offer may be stale): the band is
/// the faction's, it has a proposal, and the chosen build is still a candidate through the gates.
pub fn prepare_approval(
    world: &mut World,
    faction: FactionId,
    band: BandId,
) -> Result<Approval, String> {
    let view = resident_bands(world)
        .into_iter()
        .find(|view| view.id == band && view.faction == faction)
        .ok_or_else(|| format!("Band {} is not one of your people.", band.0))?;
    let key = world
        .resource::<ChiefProposalBook>()
        .current(band)
        .map(|proposal| proposal.build_key.clone())
        .ok_or_else(|| "Nobody is proposing a build for that band.".to_string())?;
    let candidate = candidates(world, &view)
        .into_iter()
        .find(|candidate| candidate.key == key)
        .ok_or_else(|| "That build is no longer possible.".to_string())?;
    Ok(Approval {
        view_entity: view.entity,
        candidate,
    })
}

/// **COMMIT AN APPROVAL**: declare a band or tile build as a player's declaration would (a source build
/// was already declared by the caller, `declared`), raise the keep orders the proposal carries, track
/// the build for the order-met credit and say so on the feed.
pub fn commit_approval(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    approval: &Approval,
    declared: bool,
) -> Result<(), String> {
    let candidate = &approval.candidate;
    let entity = approval.view_entity;
    let ladder = world.resource::<LadderConfigHandle>().get();
    let tick = world.resource::<SimulationTick>().0;
    let subject = {
        let name = world
            .get::<BandName>(entity)
            .map_or_else(|| format!("Band {}", band.0), |name| name.0.clone());
        Keeper::of(world.get::<BandLeadership>(entity), &name)
            .map(|keeper| keeper.subject)
            .unwrap_or(name)
    };
    if !declared {
        {
            let mut improvements = world.resource_mut::<TileImprovementRegistry>();
            match &candidate.target {
                LaborTarget::Band { band, branch } => {
                    improvements.bands_mut().place(*band, *branch, &ladder);
                }
                LaborTarget::Tile { tile, branch } => {
                    improvements.place_owned(*tile, *branch, faction, Some(band), &ladder);
                }
                _ => return Err("That build has no place to stand.".to_string()),
            }
        }
        let source = source_of(&candidate.target)
            .ok_or_else(|| "That build has no place to stand.".to_string())?;
        let Some(mut allocation) = world.get_mut::<LaborAllocation>(entity) else {
            return Err("That band has no work to direct.".to_string());
        };
        allocation.set_staffed_row(candidate.target.clone(), None, None);
        if !allocation.declare_build(source, BuildJob::Climb(candidate.dest)) {
            return Err("That build could not be declared.".to_string());
        }
    }
    if let Some(mut allocation) = world.get_mut::<LaborAllocation>(entity) {
        for raise in &candidate.raises {
            let priority = allocation
                .order_for(&raise.material)
                .map(|order| order.priority)
                .unwrap_or_default();
            crate::keep_on_hand::set_order(&mut allocation, &raise.material, raise.to, priority);
        }
    }
    let note = ladder.rung(candidate.dest).label.to_lowercase();
    let herd = match &candidate.target {
        LaborTarget::Hunt { fauna_id, .. } => Some(fauna_id.clone()),
        _ => None,
    };
    let credited = !matches!(candidate.target, LaborTarget::Extract { .. });
    world.resource_scope::<ChiefProposalBook, _>(|_, mut book| {
        if credited {
            book.bands
                .entry(band)
                .or_default()
                .approved
                .push(ApprovedBuild {
                    branch: candidate.branch,
                    dest: candidate.dest,
                    tile: candidate.tile,
                    herd,
                });
        }
    });
    world.resource_mut::<CommandEventLog>().push(
        CommandEventEntry::new(
            tick,
            CommandEventKind::Extraction,
            faction,
            format!("{subject}: {} begins ({note}).", candidate.phrase),
            Some(format!(
                "status=applied action=approve_proposal build={}",
                candidate.key
            )),
        )
        .with_band(band),
    );
    refresh_proposals(world, false);
    Ok(())
}

/// **APPROVE A BAND OR TILE PROPOSAL IN ONE STEP** (a build on a worked source needs the server's
/// declaration path: [`prepare_approval`] / [`commit_approval`]).
pub fn approve_proposal(world: &mut World, faction: FactionId, band: BandId) -> Result<(), String> {
    let approval = prepare_approval(world, faction, band)?;
    if approval.source_build().is_some() {
        return Err("That build is declared through the build command.".to_string());
    }
    commit_approval(world, faction, band, &approval, false)
}

/// **"NOT NOW"** (`defer_proposal <faction> <band> <build-key>`): hide that build for
/// `band_orders.proposal_not_now_turns` turns.
pub fn defer_proposal(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    key: &str,
) -> Result<(), String> {
    let owned = resident_bands(world)
        .iter()
        .any(|view| view.id == band && view.faction == faction);
    if !owned {
        return Err(format!("Band {} is not one of your people.", band.0));
    }
    let tick = world.resource::<SimulationTick>().0;
    let turns = u64::from(
        world
            .resource::<LaborConfigHandle>()
            .get()
            .band_orders
            .proposal_not_now_turns,
    );
    world.resource_scope::<ChiefProposalBook, _>(|_, mut book| {
        book.bands
            .entry(band)
            .or_default()
            .deferred
            .insert(key.to_string(), tick + turns);
    });
    refresh_proposals(world, false);
    Ok(())
}
