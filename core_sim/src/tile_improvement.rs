//! **THE TILE KIND - an improvement standing on ONE TILE of a branch the config declares**
//! (`docs/plan_intensification_ladder.md`, `.claude/rules/core_sim/intensification.md` -> "The tile
//! kind").
//!
//! A `tile` branch is the road's shape for a thing that is not a road: a **standing** on the ladder,
//! a **keeper**, an **upkeep** bill, a **neglect** clock and a **grace** - and no crew row, no
//! stock, no traffic. It is a source kind the code owns (each kind is a source with its own state,
//! registry and turn arm) and a branch is a JSON edit, so the code never names a tile branch: no
//! tile branch ships, and one added to `intensification_ladder.json` works with no code change
//! (`core_sim/tests/tile_improvement.rs` proves it on a test-only ladder).
//!
//! # Not the road, and not merged with it
//!
//! [`crate::routes::RoadRegistry`] is keyed by tile alone because a tile carries one road and its
//! position is wore in by traffic. This registry is keyed `(tile, branch)` because one tile can carry
//! an improvement of several branches, and its position is raised **only** by a band's builders. The
//! two share the keeping *machinery* (the `Roadwork` pool pays both; the ladder's neglect and decay
//! seams read both) and nothing else.
//!
//! # The keeper is a REACH, not a command
//!
//! A road's keeper is the band that typed `grade`. A tile improvement's keeper is **whichever band of
//! the improvement's people stands within `supply_network_config.reach_tiles` of it** - the nearest,
//! ties to the lowest band id - recomputed every turn by [`assign_tile_keepers`]. The band the
//! `build` command names is the *builder* only; keeping thereafter follows the people, so a camp that
//! moves on stops keeping what it left, and the improvement decays after its rung's grace unless
//! another band comes within reach.
//!
//! # What the keeping draws on
//!
//! The band's `Roadwork` pool - the one band-level keeping pool, which already pays for tiles with no
//! labor row. A kept tile improvement's claim sits beside the roads' in the pool's split
//! (`systems::labor::route_keeping_claims`). **Materials are refused on a tile rung** (the ladder's
//! `validate`): the pool settles work, and a stone-eating tile rung would be a bill nothing pays.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::components::{BandId, PopulationCohort, ResidentBand, Tile};
use crate::grid_utils::hex_distance_wrapped;
use crate::intensification::{
    interpolate, rung_span, upkeep_shortfall, BranchId, BuildGate, BuildTurns, LadderConfig,
    LadderConfigHandle, RungDef, RungId, RungStanding, SourceKind, NEGLECT_NONE, NO_UPKEEP_DECAY,
    NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
};
use crate::orders::FactionId;
use crate::resources::{CommandEventLog, SimulationConfig, SimulationTick, TileRegistry};
use crate::supply_network_config::SupplyNetworkConfigHandle;

/// **ONE SITE, ONE LOAD** - the keeper-loads a tile improvement's `scaled_by: source_load` upkeep is
/// multiplied by. A tile improvement has no stock, head count or crop to scale a bill by, so its
/// bill is the rung's `work_per_turn` as written.
pub const TILE_KEEPING_LOAD: f32 = 1.0;

/// Slack on the keeping comparison, so an improvement funded to the last representable fraction of
/// its bill is not judged short by a rounding - [`crate::routes`]' reason, restated for this kind.
const KEEPING_EPSILON: f32 = 1.0e-4;

/// **THE BAND WHOSE JOB THIS IMPROVEMENT IS, THIS TURN** - the nearest of the improvement's people
/// within reach. Recomputed every turn, never commanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileKeeper {
    pub faction: FactionId,
    pub band: BandId,
}

/// **THE KEY OF ONE IMPROVEMENT** - the tile and the branch, `(y, x, branch)` so iteration is
/// row-major like every other tile sweep, with the branch's string order breaking the tie.
pub type TileImprovementKey = (u32, u32, BranchId);

/// The rung price a tile branch uses: the ladder's own, unscaled. A tile improvement carries no
/// per-site cost multiplier (the deposit ladder's arrangement), so two tiles' rungs are the same
/// width.
pub fn tile_rung_cost(rung: &RungDef) -> Option<f32> {
    rung.build_cost(RUNG_COST_UNSCALED)
}

/// Where a tile improvement stands on `branch` for a ladder position.
pub fn tile_standing_at(ladder: &LadderConfig, branch: BranchId, position: f32) -> RungStanding {
    RungStanding::at(ladder, branch, position, tile_rung_cost)
}

/// The `(base, width)` of a rung in the tile branch's cumulative work units.
pub fn tile_rung_span(rung: RungId, ladder: &LadderConfig) -> (f32, f32) {
    rung_span(ladder, ladder.rung(rung), &tile_rung_cost)
}

/// **AN IMPROVEMENT ON ONE TILE OF ONE TILE BRANCH.**
///
/// One position in cumulative work units, exactly as a patch, a herd, a working and a road carry one;
/// the standing beside it is derived from the position by [`Self::set_position`], the one mutator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileImprovement {
    pub tile: UVec2,
    pub branch: BranchId,
    /// **The people whose improvement this is** - set when it is placed, and the only faction whose
    /// bands may raise or keep it.
    pub faction: FactionId,
    position: f32,
    standing: RungStanding,
    /// This turn's keeper, `None` when no band of the people stands within reach.
    pub keeper: Option<TileKeeper>,
    /// **This turn's keeping bill, as stamped before the pool paid it** - `None` where nobody has
    /// billed it yet. The road's `upkeep_demanded` exactly: every reader of the keeping takes the
    /// stamp, so `demand - supplied == shortfall` holds verbatim on the wire.
    pub upkeep_demanded: Option<f32>,
    /// What the keeper's pool paid toward the bill this turn.
    pub upkeep_supplied: f32,
    /// Consecutive turns the bill went short, against the at-risk rung's grace.
    pub neglect_turns: u16,
    /// The highest rung ever held, so an *achieved* loss is told once ([`crate::intensification::raise_peak`]).
    pub peak_rung: Option<RungId>,
    /// Why the head build is stuck, scratch cleared every turn.
    pub build_blocked_reason: BuildGate,
    /// The head build's countdown, scratch cleared every turn.
    pub build_turns_remaining: Option<BuildTurns>,
    /// Its 0-based place in the builder's queue, or [`sim_schema::NOT_IN_ANY_BUILD_QUEUE`].
    pub build_queue_position: i32,
    /// The rung the head entry is raising it to - published, scratch cleared every turn.
    pub build_destination: Option<RungId>,
    /// The legs still to lay on the way there - published, scratch cleared every turn.
    pub build_legs: Vec<crate::intensification::PublishedBuildLeg>,
}

impl TileImprovement {
    /// A new improvement at the branch's floor: nothing banked, nobody keeping it.
    pub fn placed(
        tile: UVec2,
        branch: BranchId,
        faction: FactionId,
        ladder: &LadderConfig,
    ) -> Self {
        Self {
            tile,
            branch,
            faction,
            position: RUNG_UNSTARTED,
            standing: tile_standing_at(ladder, branch, RUNG_UNSTARTED),
            keeper: None,
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            neglect_turns: NEGLECT_NONE,
            peak_rung: None,
            build_blocked_reason: BuildGate::Open,
            build_turns_remaining: None,
            build_queue_position: sim_schema::NOT_IN_ANY_BUILD_QUEUE,
            build_destination: None,
            build_legs: Vec::new(),
        }
    }

    /// Where the improvement stands. Read-only - [`Self::set_position`] is the one mutator.
    pub fn standing(&self) -> RungStanding {
        self.standing
    }

    /// The raw position, in cumulative work units.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// The rung the improvement **holds**.
    pub fn held_rung(&self) -> RungId {
        self.standing.held
    }

    /// **THE ONE MUTATOR**, writing the position and its derived standing together so a caller cannot
    /// leave the pair disagreeing. A finished rung starts with its full grace
    /// ([`crate::intensification::rung_rose`]).
    pub fn set_position(&mut self, position: f32, ladder: &LadderConfig) {
        let was = self.standing.held;
        self.position = position.max(RUNG_UNSTARTED);
        self.standing = tile_standing_at(ladder, self.branch, self.position);
        if crate::intensification::rung_rose(ladder, was, self.standing.held) {
            self.neglect_turns = NEGLECT_NONE;
            crate::intensification::raise_peak(ladder, &mut self.peak_rung, self.standing.held);
        }
    }

    /// **THE BILL THIS IMPROVEMENT OWES AT ITS STANDING**, interpolated across the rung it is
    /// climbing - the road's `road_upkeep_demand` with the tile's constant load.
    pub fn upkeep_demand(&self, ladder: &LadderConfig) -> f32 {
        interpolate(&self.standing, |rung| {
            ladder.rung(rung.id).upkeep_demand(TILE_KEEPING_LOAD)
        })
    }

    /// **The stamped bill where this turn's pass has struck one, the live demand where it has not** -
    /// the plant web's own rule.
    pub fn keeping_basis(&self, ladder: &LadderConfig) -> f32 {
        self.upkeep_demanded
            .unwrap_or_else(|| self.upkeep_demand(ladder))
    }

    /// What this turn's bill left unpaid, off the stamped basis.
    pub fn upkeep_shortfall(&self) -> f32 {
        upkeep_shortfall(
            self.upkeep_demanded.unwrap_or(NO_UPKEEP_DEMAND),
            self.upkeep_supplied,
        )
    }

    /// Was this turn's bill met? An unbilled improvement is treated as met.
    pub fn keeping_is_met(&self) -> bool {
        match self.upkeep_demanded {
            Some(demand) => self.upkeep_supplied + KEEPING_EPSILON >= demand,
            None => true,
        }
    }

    /// **WHAT THE MISSED KEEPING COSTS THE METER THIS TURN** - the at-risk rung's rot at the stamped
    /// shortfall, which the build quote subtracts from the crew's output ([`crate::routes::road_meter_rot`]'s
    /// twin). Zero for a rung that declares no upkeep.
    pub fn meter_rot(&self, ladder: &LadderConfig) -> f32 {
        let rung = ladder.rung(self.at_risk_rung());
        if rung.upkeep.is_none() {
            return NO_UPKEEP_DECAY;
        }
        let no_materials = BTreeMap::new();
        rung.meter_rot_at_fraction(
            crate::intensification::keeping_shortfall_fraction(
                self.keeping_basis(ladder),
                self.upkeep_supplied,
                &no_materials,
                &no_materials,
            ),
            self.neglect_turns,
        )
    }

    /// **The rung whose bill is at risk** - the one being raised once work is banked on it, else the
    /// one held ([`crate::routes::road_at_risk_rung`]).
    pub fn at_risk_rung(&self) -> RungId {
        self.standing
            .raising
            .filter(|_| self.standing.banked > crate::intensification::NO_RUNG_WORK_BANKED)
            .unwrap_or(self.standing.held)
    }
}

/// **EVERY TILE IMPROVEMENT IN THE WORLD** - one record per `(tile, branch)`.
///
/// `BTreeMap`, not `HashMap`: the iteration order is observed by the snapshot and the checkpoint.
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
pub struct TileImprovementRegistry {
    improvements: BTreeMap<TileImprovementKey, TileImprovement>,
}

impl TileImprovementRegistry {
    fn key(tile: UVec2, branch: BranchId) -> TileImprovementKey {
        (tile.y, tile.x, branch)
    }

    /// The improvement of `branch` on `tile`, or `None` where nothing has been placed.
    pub fn get(&self, tile: UVec2, branch: BranchId) -> Option<&TileImprovement> {
        self.improvements.get(&Self::key(tile, branch))
    }

    pub fn get_mut(&mut self, tile: UVec2, branch: BranchId) -> Option<&mut TileImprovement> {
        self.improvements.get_mut(&Self::key(tile, branch))
    }

    /// **The improvement of `branch` on `tile`, placing one at the branch's floor where there was
    /// none** - what the first `build` verb of a tile branch does.
    pub fn place(
        &mut self,
        tile: UVec2,
        branch: BranchId,
        faction: FactionId,
        ladder: &LadderConfig,
    ) -> &mut TileImprovement {
        self.improvements
            .entry(Self::key(tile, branch))
            .or_insert_with(|| TileImprovement::placed(tile, branch, faction, ladder))
    }

    pub fn iter(&self) -> impl Iterator<Item = &TileImprovement> {
        self.improvements.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TileImprovement> {
        self.improvements.values_mut()
    }

    /// **The improvements a band keeps this turn**, in registry order.
    pub fn kept_by(&self, band: BandId) -> impl Iterator<Item = &TileImprovement> {
        self.improvements
            .values()
            .filter(move |improvement| improvement.keeper.is_some_and(|keeper| keeper.band == band))
    }

    pub fn len(&self) -> usize {
        self.improvements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.improvements.is_empty()
    }
}

/// **THE KEEPER RULE**, pure: of the candidate bands, the nearest to `site` within `reach` hex steps;
/// a tie goes to the lowest band id; none within reach is `None`.
///
/// Candidates are `(band, position)` pairs of the improvement's own people, so the caller has already
/// filtered the faction. Distance is the hex metric every other reach in the sim is measured in.
pub fn nearest_band_in_reach(
    site: UVec2,
    candidates: &[(BandId, UVec2)],
    reach: u32,
    width: u32,
    wrap: bool,
) -> Option<BandId> {
    candidates
        .iter()
        .map(|(band, position)| (hex_distance_wrapped(*position, site, width, wrap), *band))
        .filter(|(distance, _)| *distance <= reach)
        .min()
        .map(|(_, band)| band)
}

/// **THE KEEPER RULE, EVERY TURN** - and the turn's bill, struck in the same breath so the keeper and
/// the bill describe one moment.
///
/// Runs in the Population stage ahead of the labour pass (which pays the bill from the keeper's
/// `Roadwork` pool), after the movement chain so the reach is measured on the positions the turn's
/// movement left. The bill is stamped on **every** improvement, keeper or not: an unstamped
/// improvement reads as met, so a pass that billed only the kept ones would let an unkept one hold
/// for ever - the same trap [`crate::systems::bill_and_stock_roads`] documents for roads.
pub fn assign_tile_keepers(
    mut registry: ResMut<TileImprovementRegistry>,
    supply: Res<SupplyNetworkConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    bands: Query<(&PopulationCohort, &BandId), With<ResidentBand>>,
) {
    if registry.is_empty() {
        return;
    }
    let reach = supply.get().reach_tiles;
    let ladder = ladder.get();
    let width = tile_registry.width;
    let wrap = sim_config.map_topology.wrap_horizontal;
    let placed: Vec<(FactionId, BandId, UVec2)> = bands
        .iter()
        .filter_map(|(cohort, band)| {
            tiles
                .get(cohort.current_tile)
                .ok()
                .map(|tile| (cohort.faction, *band, tile.position))
        })
        .collect();
    for improvement in registry.iter_mut() {
        let candidates: Vec<(BandId, UVec2)> = placed
            .iter()
            .filter(|(faction, _, _)| *faction == improvement.faction)
            .map(|(_, band, position)| (*band, *position))
            .collect();
        improvement.keeper =
            nearest_band_in_reach(improvement.tile, &candidates, reach, width, wrap).map(|band| {
                TileKeeper {
                    faction: improvement.faction,
                    band,
                }
            });
        if improvement.upkeep_demanded.is_none() {
            improvement.upkeep_demanded = Some(improvement.upkeep_demand(&ladder));
        }
    }
}

/// **THE DECAY AND THE ONE-TURN CYCLE** - [`crate::routes::advance_roads`]' phases 1-3 for the tile
/// kind, in `TurnStage::Logistics` ahead of the next turn's bill.
///
/// 1. **How short**, off the stamped basis through the ladder's own seam, and the neglect counter
///    that trips on it. An improvement nobody billed reads fully supplied and is forgiven.
/// 2. **The bleed**, at the at-risk rung's own rate, past that rung's own grace
///    ([`RungDef::upkeep_decay`] owns the `>`), told to the keeper's people as the two edges
///    [`crate::intensification::rung_decay_edges`] names.
/// 3. **The bill, this turn's payment and the build scratch cleared** on the one-turn cycle.
///
/// A rung that declares no upkeep owes nothing and cannot be short, so it never decays - the
/// ladder's own rule, not this kind's.
pub fn advance_tile_improvements(
    mut registry: ResMut<TileImprovementRegistry>,
    ladder: Res<LadderConfigHandle>,
    mut event_log: Option<ResMut<CommandEventLog>>,
    tick: Option<Res<SimulationTick>>,
) {
    let ladder = ladder.get();
    for improvement in registry.iter_mut() {
        let basis = improvement.upkeep_demanded.unwrap_or(NO_UPKEEP_DEMAND);
        let supplied = improvement.upkeep_supplied;
        let no_materials = BTreeMap::new();
        let shortfall_fraction = crate::intensification::keeping_shortfall_fraction(
            basis,
            supplied,
            &no_materials,
            &no_materials,
        );
        if crate::intensification::keeping_is_short(basis, supplied, &no_materials, &no_materials) {
            improvement.neglect_turns = improvement.neglect_turns.saturating_add(1);
        } else {
            improvement.neglect_turns = NEGLECT_NONE;
        }
        let at_risk = improvement.at_risk_rung();
        let decay = ladder
            .rung(at_risk)
            .upkeep_decay(shortfall_fraction, improvement.neglect_turns);
        if decay > NO_UPKEEP_DECAY {
            let was = improvement.position();
            let faction = improvement.faction;
            let band = improvement.keeper.map(|keeper| keeper.band);
            improvement.set_position(was - decay, &ladder);
            let mut edges = crate::intensification::rung_decay_edges(
                &ladder,
                improvement.branch,
                was,
                improvement.position(),
                |rung| tile_rung_span(rung.rung_id(), &ladder),
            );
            crate::intensification::settle_achieved_losses(
                &ladder,
                &mut edges,
                &mut improvement.peak_rung,
            );
            if let (false, Some(log)) = (edges.is_empty(), event_log.as_deref_mut()) {
                crate::intensification::announce_rung_decay(
                    log,
                    tick.as_deref().map_or(0, |tick| tick.0),
                    faction,
                    band,
                    &ladder,
                    improvement.tile,
                    improvement.position(),
                    &edges,
                );
            }
        }
        improvement.upkeep_demanded = None;
        improvement.upkeep_supplied = NO_UPKEEP_DEMAND;
        improvement.build_blocked_reason = BuildGate::Open;
        improvement.build_turns_remaining = None;
        improvement.build_queue_position = sim_schema::NOT_IN_ANY_BUILD_QUEUE;
        improvement.build_destination = None;
        improvement.build_legs.clear();
    }
}

/// Is `branch` a per-tile branch of this ladder?
pub fn is_tile_branch(ladder: &LadderConfig, branch: BranchId) -> bool {
    ladder.source_kind(branch) == Some(SourceKind::Tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: u32 = 32;
    const NO_WRAP: bool = false;
    const REACH: u32 = 3;

    fn at(band: u64, x: u32, y: u32) -> (BandId, UVec2) {
        (BandId(band), UVec2::new(x, y))
    }

    #[test]
    fn the_nearest_band_in_reach_keeps_it() {
        let site = UVec2::new(10, 10);
        let candidates = [at(1, 12, 10), at(2, 11, 10)];
        assert_eq!(
            nearest_band_in_reach(site, &candidates, REACH, WIDTH, NO_WRAP),
            Some(BandId(2))
        );
    }

    #[test]
    fn a_tie_goes_to_the_lowest_band_id() {
        let site = UVec2::new(10, 10);
        let candidates = [at(9, 12, 10), at(4, 8, 10)];
        assert_eq!(
            nearest_band_in_reach(site, &candidates, REACH, WIDTH, NO_WRAP),
            Some(BandId(4)),
            "both stand two steps away; the lower id keeps it"
        );
    }

    #[test]
    fn nobody_in_reach_means_no_keeper() {
        let site = UVec2::new(10, 10);
        let candidates = [at(1, 20, 10)];
        assert_eq!(
            nearest_band_in_reach(site, &candidates, REACH, WIDTH, NO_WRAP),
            None
        );
        assert_eq!(
            nearest_band_in_reach(site, &[], REACH, WIDTH, NO_WRAP),
            None
        );
    }

    #[test]
    fn a_band_exactly_at_reach_still_keeps_it() {
        let site = UVec2::new(10, 10);
        let candidates = [at(1, 13, 10)];
        assert_eq!(
            nearest_band_in_reach(site, &candidates, REACH, WIDTH, NO_WRAP),
            Some(BandId(1))
        );
    }
}
