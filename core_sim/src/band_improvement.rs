//! **THE BAND KIND - an improvement a BAND holds at its camp, and carries with it**
//! (`.claude/rules/core_sim/intensification.md` -> "The band kind").
//!
//! A `band` branch is the tile kind's twin with the tile taken away: a **standing** on the ladder, a
//! **bill**, a **neglect** clock and a **grace**, kept by a **crew row** the band assigns to it
//! ([`crate::components::LaborTarget::Band`]). What differs from [`crate::tile_improvement`] is only
//! *where it stands*:
//!
//! - **It is keyed by the band** - `(BandId, branch)` - so its standing moves with the band, survives
//!   a save (the registry rides the tile registry, which is `SimState`), and is neither visible to
//!   nor usable by any other band. A band that splits keeps it on the parent and starts the new band
//!   at the root. Bands do not merge, so there is no merge rule.
//! - **Distance is zero, always.** The improvement is at the camp, so its crew is never a work party
//!   and `party_source_position` has no tile to name.
//! - **A band rung may carry a build pile** (a drying rack's wood), drawn from the band's own store as
//!   the meter climbs exactly as the pen's hurdles are. The tile kind refuses a pile; a band's builders
//!   work from the band's own store, so there is a settle pass to pay it.
//! - **Upkeep is a fixed amount of work per turn regardless of the band's size**: `scaled_by:
//!   source_load` with the kind's load the constant [`BAND_KEEPING_LOAD`], as the tile kind's is
//!   [`crate::tile_improvement::TILE_KEEPING_LOAD`].
//!
//! # What it buys
//!
//! Nothing in this file and nothing in the ladder config: the payoff of a band branch is a shelf life,
//! and it lives in `demographics_config.json` -> `keeping.preservation`, read through
//! [`crate::spoilage::band_shelf_life`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::components::{BandId, LaborAllocation, PopulationCohort};
use crate::intensification::{
    interpolate, rung_span, upkeep_shortfall, BranchId, BuildGate, BuildTurns, LadderConfig,
    LadderConfigHandle, RungDef, RungId, RungStanding, SourceKind, NEGLECT_NONE, NO_UPKEEP_DECAY,
    NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
};
use crate::resources::{CommandEventLog, SimulationTick};
use crate::tile_improvement::TileImprovementRegistry;

/// **ONE SITE, ONE LOAD** - the keeper-load a band improvement's `scaled_by: source_load` upkeep is
/// multiplied by. A band improvement has no stock, head count or crop to scale a bill by, so its bill
/// is the rung's `work_per_turn` as written: the same amount whatever the band's size.
pub const BAND_KEEPING_LOAD: f32 = 1.0;

/// **NO HANDS KEPT THE IMPROVEMENT THIS TURN** - the neutral of [`BandImprovement::upkeep_hands`].
pub const NO_HANDS_ON_THE_BAND_IMPROVEMENT: f32 = 0.0;

/// Slack on the keeping comparison, so an improvement funded to the last representable fraction of
/// its bill is not judged short by a rounding.
const KEEPING_EPSILON: f32 = 1.0e-4;

/// **THE KEY OF ONE BAND IMPROVEMENT** - the band and the branch.
pub type BandImprovementKey = (BandId, BranchId);

/// The rung price a band branch uses: the ladder's own, unscaled.
pub fn band_rung_cost(rung: &RungDef) -> Option<f32> {
    rung.build_cost(RUNG_COST_UNSCALED)
}

/// Where a band improvement stands on `branch` for a ladder position.
pub fn band_standing_at(ladder: &LadderConfig, branch: BranchId, position: f32) -> RungStanding {
    RungStanding::at(ladder, branch, position, band_rung_cost)
}

/// The `(base, width)` of a rung in the band branch's cumulative work units.
pub fn band_rung_span(rung: RungId, ladder: &LadderConfig) -> (f32, f32) {
    rung_span(ladder, ladder.rung(rung), &band_rung_cost)
}

/// **AN IMPROVEMENT A BAND HOLDS ON ONE BAND BRANCH.** One position in cumulative work units and the
/// standing derived from it by [`Self::set_position`], the one mutator - the tile improvement's shape
/// without the tile and without a faction (the band is the owner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandImprovement {
    pub band: BandId,
    pub branch: BranchId,
    position: f32,
    standing: RungStanding,
    /// **This turn's keeping bill, as stamped before the crew paid it** - `None` where nobody has
    /// billed it yet. See [`crate::tile_improvement::TileImprovement::upkeep_demanded`].
    pub upkeep_demanded: Option<f32>,
    /// What the crew paid toward the bill this turn.
    pub upkeep_supplied: f32,
    /// How many of the crew's hands the keeping took this turn.
    pub upkeep_hands: f32,
    /// The keeping tools this turn's crew claimed, one line per `(faction, item)`.
    pub upkeep_toe: Vec<crate::components::KeepingToolLine>,
    /// Consecutive turns the bill went short, against the at-risk rung's grace.
    pub neglect_turns: u16,
    /// The highest rung ever held, so an *achieved* loss is told once.
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

impl BandImprovement {
    /// A new improvement at the branch's floor: nothing banked, nobody keeping it.
    pub fn placed(band: BandId, branch: BranchId, ladder: &LadderConfig) -> Self {
        Self {
            band,
            branch,
            position: RUNG_UNSTARTED,
            standing: band_standing_at(ladder, branch, RUNG_UNSTARTED),
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            upkeep_hands: NO_HANDS_ON_THE_BAND_IMPROVEMENT,
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

    /// The rung the improvement **holds** (an `on_completion` rung is held only once finished).
    pub fn held_rung(&self) -> RungId {
        self.standing.held
    }

    /// **THE ONE MUTATOR**, writing the position and its derived standing together. A finished rung
    /// starts with its full grace.
    pub fn set_position(&mut self, position: f32, ladder: &LadderConfig) {
        let was = self.standing.held;
        self.position = position.max(RUNG_UNSTARTED);
        self.standing = band_standing_at(ladder, self.branch, self.position);
        if crate::intensification::rung_rose(ladder, was, self.standing.held) {
            self.neglect_turns = NEGLECT_NONE;
            crate::intensification::raise_peak(ladder, &mut self.peak_rung, self.standing.held);
        }
    }

    /// **THE BILL THIS IMPROVEMENT OWES AT ITS STANDING**, interpolated across the rung it is
    /// climbing, at the constant band load.
    pub fn upkeep_demand(&self, ladder: &LadderConfig) -> f32 {
        interpolate(&self.standing, |rung| {
            ladder.rung(rung.id).upkeep_demand(BAND_KEEPING_LOAD)
        })
    }

    /// The stamped bill where this turn's pass has struck one, the live demand where it has not.
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

    /// **Whole hands the stamped bill calls for** at the bare rate.
    pub fn upkeep_workers_needed(&self, ladder: &LadderConfig) -> u32 {
        let demand = self.keeping_basis(ladder);
        if demand <= NO_UPKEEP_DEMAND {
            return crate::intensification::NO_CREW_ON_THIS_ACTIVITY;
        }
        (demand / PER_WORKER_OUTPUT).ceil() as u32
    }

    /// **WHAT THE MISSED KEEPING COSTS THE METER THIS TURN** - the at-risk rung's rot at the stamped
    /// shortfall. Zero for a rung that declares no upkeep.
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
    /// one held.
    pub fn at_risk_rung(&self) -> RungId {
        self.standing
            .raising
            .filter(|_| self.standing.banked > crate::intensification::NO_RUNG_WORK_BANKED)
            .unwrap_or(self.standing.held)
    }
}

/// **EVERY BAND-KIND IMPROVEMENT IN THE WORLD** - one record per `(band, branch)`.
///
/// `BTreeMap`, not `HashMap`: the iteration order is observed by the snapshot and the checkpoint. It
/// rides inside [`TileImprovementRegistry`] (`bands`), so every pass that reads the per-tile
/// improvements reads this beside them and the one resource is the one `SimState` field.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct BandImprovementRegistry {
    improvements: BTreeMap<BandImprovementKey, BandImprovement>,
}

impl BandImprovementRegistry {
    /// The improvement of `branch` held by `band`, or `None` where nothing has been placed.
    pub fn get(&self, band: BandId, branch: BranchId) -> Option<&BandImprovement> {
        self.improvements.get(&(band, branch))
    }

    pub fn get_mut(&mut self, band: BandId, branch: BranchId) -> Option<&mut BandImprovement> {
        self.improvements.get_mut(&(band, branch))
    }

    /// **The improvement of `branch` held by `band`, placing one at the branch's floor where there
    /// was none** - what the first crew assigned to it does.
    pub fn place(
        &mut self,
        band: BandId,
        branch: BranchId,
        ladder: &LadderConfig,
    ) -> &mut BandImprovement {
        self.improvements
            .entry((band, branch))
            .or_insert_with(|| BandImprovement::placed(band, branch, ladder))
    }

    /// Every improvement `band` holds, in branch order.
    pub fn held_by(&self, band: BandId) -> impl Iterator<Item = &BandImprovement> {
        self.improvements
            .values()
            .filter(move |improvement| improvement.band == band)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BandImprovement> {
        self.improvements.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut BandImprovement> {
        self.improvements.values_mut()
    }

    pub fn len(&self) -> usize {
        self.improvements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.improvements.is_empty()
    }

    /// **FORGET THE IMPROVEMENTS OF BANDS THAT ARE GONE** - a band that dies or is despawned takes
    /// its camp with it, so its records are dropped rather than left decaying for ever.
    pub fn retain_bands(&mut self, mut live: impl FnMut(BandId) -> bool) {
        self.improvements.retain(|(band, _), _| live(*band));
    }

    /// **CLEAR EVERY IMPROVEMENT'S PER-TURN BUILD SCRATCH** - what a restore does so a load never
    /// republishes a stale date.
    pub(crate) fn clear_build_scratch(&mut self) {
        for improvement in self.improvements.values_mut() {
            improvement.build_turns_remaining = None;
            improvement.build_queue_position = sim_schema::NOT_IN_ANY_BUILD_QUEUE;
            improvement.build_blocked_reason = BuildGate::Open;
            improvement.build_destination = None;
            improvement.build_legs.clear();
        }
    }
}

/// Is `branch` a per-band branch of this ladder?
pub fn is_band_branch(ladder: &LadderConfig, branch: BranchId) -> bool {
    ladder.source_kind(branch) == Some(SourceKind::Band)
}

/// **THE DECAY, THIS TURN'S BILL AND THE ONE-TURN CYCLE** for the band kind -
/// [`crate::tile_improvement::advance_tile_improvements`]' shape, in `TurnStage::Logistics` ahead of
/// the next labour pass. An improvement nobody keeps is billed all the same, so an unkept one cannot
/// read as met and decays after its at-risk rung's grace, announced to the band's own people.
pub fn advance_band_improvements(
    mut registry: ResMut<TileImprovementRegistry>,
    ladder: Res<LadderConfigHandle>,
    bands: Query<(&BandId, &PopulationCohort, &LaborAllocation)>,
    mut event_log: Option<ResMut<CommandEventLog>>,
    tick: Option<Res<SimulationTick>>,
) {
    let ladder = ladder.get();
    let live: std::collections::BTreeSet<BandId> = bands.iter().map(|(band, _, _)| *band).collect();
    registry
        .bands_mut()
        .retain_bands(|band| live.contains(&band));
    for improvement in registry.bands_mut().iter_mut() {
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
                |rung| band_rung_span(rung.rung_id(), &ladder),
            );
            crate::intensification::settle_achieved_losses(
                &ladder,
                &mut edges,
                &mut improvement.peak_rung,
            );
            if let (false, Some(log)) = (edges.is_empty(), event_log.as_deref_mut()) {
                // The people told are the band's own: it is the one that holds, and loses, the thing.
                if let Some((_, cohort, _)) =
                    bands.iter().find(|(band, _, _)| **band == improvement.band)
                {
                    announce_band_rung_decay(
                        log,
                        tick.as_deref().map_or(0, |tick| tick.0),
                        cohort.faction,
                        improvement.band,
                        &ladder,
                        improvement.position(),
                        &edges,
                    );
                }
            }
        }
        improvement.upkeep_supplied = NO_UPKEEP_DEMAND;
        improvement.upkeep_hands = NO_HANDS_ON_THE_BAND_IMPROVEMENT;
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

/// **ANNOUNCE A BAND IMPROVEMENT'S DECAY EDGES** to the band's people, on the rung's own feed
/// channel: [`crate::intensification::announce_rung_decay`]'s twin for a thing with no tile to name.
/// One Info line per slip and one Alert line per loss, `band=` linking the line to the band's Work
/// row.
fn announce_band_rung_decay(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: crate::orders::FactionId,
    band: BandId,
    ladder: &LadderConfig,
    now: f32,
    edges: &crate::intensification::RungDecayEdges,
) {
    for (rung, _) in &edges.lost {
        let record = ladder.rung(*rung);
        let back_to = &ladder
            .rung(ladder.below_id(*rung).unwrap_or(*rung))
            .label_fallback;
        event_log.push(crate::resources::CommandEventEntry::new(
            tick,
            record
                .feed_channel_kind()
                .expect("validate requires a feed channel on every built rung"),
            faction,
            format!("{} lost - back to {back_to}", record.label),
            Some(format!(
                "status={} reason=untended rung={} band={}",
                crate::intensification::DECAY_LOST_STATUS,
                rung.wire_key(),
                band.0
            )),
        ));
    }
    for (rung, (base, width)) in &edges.slipped {
        let percent = crate::intensification::slip_percent(now, *base, *width);
        let record = ladder.rung(*rung);
        event_log.push(crate::resources::CommandEventEntry::new(
            tick,
            record
                .feed_channel_kind()
                .expect("validate requires a feed channel on every built rung"),
            faction,
            format!("{} is slipping - {percent}%", record.label),
            Some(format!(
                "status={} rung={} progress={percent} band={}",
                crate::intensification::DECAY_SLIP_STATUS,
                rung.wire_key(),
                band.0
            )),
        ));
    }
}
