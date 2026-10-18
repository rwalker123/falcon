//! **THE TILE KIND - an improvement standing on ONE TILE of a branch the config declares**
//! (`docs/plan_intensification_ladder.md`, `.claude/rules/core_sim/intensification.md` -> "The tile
//! kind").
//!
//! A `tile` branch is the working's shape for a thing that yields nothing: a **standing** on the
//! ladder, a **bill**, a **neglect** clock and a **grace**, kept by a **crew row** a band assigns to
//! it ([`crate::components::LaborTarget::Tile`]) exactly as a deposit's crew is. It is a source kind
//! the code owns (each kind is a source with its own state, registry and turn arm) and a branch is a
//! JSON edit, so the code never names a tile branch: no tile branch ships, and one added to
//! `intensification_ladder.json` works with no code change (`core_sim/tests/tile_improvement.rs`
//! proves it on a test-only ladder).
//!
//! # Not a road, and not merged with one
//!
//! [`crate::routes::RoadRegistry`] is keyed by tile alone, worn in by traffic and kept by the band
//! that typed `grade`. This registry is keyed `(tile, branch)` and raised only by a band's builders;
//! it shares the keeping *machinery* with every other site and nothing else.
//!
//! # The crew is the keeper
//!
//! There is no keeper rule and no keeper field. A band **assigns a crew to the improvement** with
//! `assign_labor <faction> <band> tile <branch> <x> <y> <workers>`; the first crew places it at the
//! branch's floor. That crew keeps it **first** - `keep_hands = min(crew, bill / keep_rate)`, through
//! the same site-keeping claims a working's crew makes - and the rung's keeping tools follow from the
//! rung it stands on. Nobody assigned means unkept: the bill goes unpaid and the improvement decays
//! after its rung's grace. **Distance is nothing special**: a crew past `band_work_range` is a work
//! party exactly as a far working's is, and travels, and is priced, by that path.
//!
//! # What surplus hands do
//!
//! Nothing - the same rule a working's crew follows when its stock gives it nothing more to take. The
//! hands beyond the bill have no yield to collect, and a build is raised by the band's `builders`
//! pool (as it is on a working), so the row is a keeping row and the builders are the building ones.
//!
//! # Materials
//!
//! Refused on a tile rung by `LadderConfig::validate`: the keeping settles work, and a stone-eating
//! tile rung would be a bill nothing pays.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::components::{BandId, LaborAllocation, LaborTarget, PopulationCohort};
use crate::intensification::{
    interpolate, rung_span, upkeep_shortfall, BranchId, BuildGate, BuildTurns, LadderConfig,
    LadderConfigHandle, RungDef, RungId, RungStanding, SourceKind, NEGLECT_NONE, NO_UPKEEP_DECAY,
    NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
};
use crate::orders::FactionId;
use crate::resources::{CommandEventLog, SimulationTick};

/// **ONE SITE, ONE LOAD** - the keeper-loads a tile improvement's `scaled_by: source_load` upkeep is
/// multiplied by. A tile improvement has no stock, head count or crop to scale a bill by, so its
/// bill is the rung's `work_per_turn` as written.
pub const TILE_KEEPING_LOAD: f32 = 1.0;

/// **NO HANDS KEPT THE SITE THIS TURN** - the neutral of [`TileImprovement::upkeep_hands`].
pub const NO_HANDS_ON_THE_IMPROVEMENT: f32 = 0.0;

/// Slack on the keeping comparison, so an improvement funded to the last representable fraction of
/// its bill is not judged short by a rounding - [`crate::routes`]' reason, restated for this kind.
const KEEPING_EPSILON: f32 = 1.0e-4;

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
    /// **The people whose improvement this is** - set when the first crew places it, and the only
    /// faction whose bands may crew, raise or keep it.
    pub faction: FactionId,
    position: f32,
    standing: RungStanding,
    /// **This turn's keeping bill, as stamped before the crews paid it** - `None` where nobody has
    /// billed it yet. The working's `upkeep_demanded` exactly: every reader of the keeping takes the
    /// stamp, so `demand - supplied == shortfall` holds verbatim on the wire.
    pub upkeep_demanded: Option<f32>,
    /// What the crews paid toward the bill this turn.
    pub upkeep_supplied: f32,
    /// **How many of the crews' hands the keeping took this turn** - a working's `upkeep_hands`.
    pub upkeep_hands: f32,
    /// **The keeping tools this turn's crews claimed**, one line per `(faction, item)`.
    pub upkeep_toe: Vec<crate::components::KeepingToolLine>,
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
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            upkeep_hands: NO_HANDS_ON_THE_IMPROVEMENT,
            upkeep_toe: Vec::new(),
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
    /// climbing - the working's `deposit_upkeep_demand` with the tile's constant load.
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

    /// **Whole hands the stamped bill calls for** at the bare rate - the working's
    /// `deposit_upkeep_workers_needed`, published as `upkeepWorkersNeeded`.
    pub fn upkeep_workers_needed(&self, ladder: &LadderConfig) -> u32 {
        let demand = self.keeping_basis(ladder);
        if demand <= NO_UPKEEP_DEMAND {
            return crate::intensification::NO_CREW_ON_THIS_ACTIVITY;
        }
        (demand / PER_WORKER_OUTPUT).ceil() as u32
    }

    /// **WHAT THE MISSED KEEPING COSTS THE METER THIS TURN** - the at-risk rung's rot at the stamped
    /// shortfall, which the build quote subtracts from the crew's output. Zero for a rung that
    /// declares no upkeep.
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

    /// **The rung whose bill is at risk**, the one being raised once work is banked on it, else the
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
    /// none** - what the first crew assigned to it does.
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

    /// **CLEAR EVERY SOURCE'S PER-TURN BUILD SCRATCH - what a restore does so a load never republishes
    /// a stale date.** The scratch is stamped by the labor pass and cleared by the decay pass each
    /// turn, yet the checkpoint carries it, so a loaded world's first frame repeated the last
    /// turn's stamp (a pre-fix 2,101,109-turn date outlived the fix that removed it). Cleared here
    /// the frame reads "not yet estimated" until the first turn restamps it.
    pub(crate) fn clear_build_scratch(&mut self) {
        for improvement in self.improvements.values_mut() {
            improvement.build_turns_remaining = None;
            improvement.build_queue_position = sim_schema::NOT_IN_ANY_BUILD_QUEUE;
            improvement.build_blocked_reason = BuildGate::Open;
            improvement.build_destination = None;
            improvement.build_legs.clear();
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TileImprovement> {
        self.improvements.values_mut()
    }

    pub fn len(&self) -> usize {
        self.improvements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.improvements.is_empty()
    }
}

/// **THE DECAY, THIS TURN'S BILL AND THE ONE-TURN CYCLE** - [`crate::extraction::advance_deposits`]'
/// shape for the tile kind, in `TurnStage::Logistics` ahead of the next labour pass.
///
/// 1. **How short**, off the stamped basis, and the neglect counter that trips on it. An improvement
///    nobody billed reads fully supplied and is forgiven.
/// 2. **The bleed**, at the at-risk rung's own rate, past that rung's own grace
///    ([`RungDef::upkeep_decay`] owns the `>`), told to the people whose crews work it as the two
///    edges [`crate::intensification::rung_decay_edges`] names.
/// 3. **The payment, the hands, the tools and the build scratch cleared**, and **the next bill
///    stamped at the position the bleed left** - the labour pass then pays that stamp.
///
/// A rung that declares no upkeep owes nothing and cannot be short, so it never decays - the
/// ladder's own rule, not this kind's. **An improvement nobody crews is billed all the same**: the
/// stamp is what makes an unkept one decay.
#[allow(clippy::too_many_arguments)] // a Bevy system: one parameter per resource it reads or writes
pub fn advance_tile_improvements(
    mut registry: ResMut<TileImprovementRegistry>,
    ladder: Res<LadderConfigHandle>,
    bands: Query<(&BandId, &PopulationCohort, &LaborAllocation)>,
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
                // The people told are every faction with a crew row on the site; each hears it with
                // the band whose crew there is largest - a working's rule.
                let candidates: Vec<_> = bands
                    .iter()
                    .filter_map(|(band, cohort, allocation)| {
                        let crew: u32 = allocation
                            .assignments
                            .iter()
                            .filter(|row| {
                                matches!(
                                    &row.target,
                                    LaborTarget::Tile { tile, branch }
                                        if *tile == improvement.tile && *branch == improvement.branch
                                )
                            })
                            .map(|row| row.workers)
                            .sum();
                        (crew > 0 || row_exists(allocation, improvement))
                            .then_some((cohort.faction, *band, crew))
                    })
                    .collect();
                let mut factions: Vec<FactionId> =
                    candidates.iter().map(|(faction, _, _)| *faction).collect();
                factions.sort_unstable();
                factions.dedup();
                for faction in factions {
                    crate::intensification::announce_rung_decay(
                        log,
                        tick.as_deref().map_or(0, |tick| tick.0),
                        faction,
                        crate::intensification::site_band_for(&candidates, faction),
                        &ladder,
                        improvement.tile,
                        improvement.position(),
                        &edges,
                    );
                }
            }
        }
        improvement.upkeep_supplied = NO_UPKEEP_DEMAND;
        improvement.upkeep_hands = NO_HANDS_ON_THE_IMPROVEMENT;
        improvement.upkeep_toe.clear();
        improvement.build_blocked_reason = BuildGate::Open;
        improvement.build_turns_remaining = None;
        improvement.build_queue_position = sim_schema::NOT_IN_ANY_BUILD_QUEUE;
        improvement.build_destination = None;
        improvement.build_legs.clear();
        // The next bill, struck at the position the bleed left.
        improvement.upkeep_demanded = Some(improvement.upkeep_demand(&ladder));
    }
}

/// Does this band hold a row (staffed or held at zero) on the improvement?
fn row_exists(allocation: &LaborAllocation, improvement: &TileImprovement) -> bool {
    allocation.assignments.iter().any(|row| {
        matches!(
            &row.target,
            LaborTarget::Tile { tile, branch }
                if *tile == improvement.tile && *branch == improvement.branch
        )
    })
}

/// Is `branch` a per-tile branch of this ladder?
pub fn is_tile_branch(ladder: &LadderConfig, branch: BranchId) -> bool {
    ladder.source_kind(branch) == Some(SourceKind::Tile)
}
