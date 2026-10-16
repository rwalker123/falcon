//! Mobility Score - the emergent per-faction "how roaming is this people", the counterpart of
//! [`crate::sedentarization`] (`.claude/rules/core_sim/campaign.md` -> "Mobility").
//!
//! Each turn `mobility_tick` blends two normalized inputs into a 0-100 score (config-weighted,
//! EMA-smoothed exactly as sedentarization smooths):
//!
//! - **skills** - the mean over [`crate::roaming_skills::ROAMING_SKILLS`] of the faction's ledger
//!   progress against the ladder's completion threshold. Partial progress counts.
//! - **distance** - the hexes a resident band stepped THIS turn while following a herd, in any
//!   roam state ([`HerdFollowSteps`], filled by `advance_band_movement`), as a share of
//!   `references.hexes_per_turn`, population-weighted over the faction's resident bands.
//!
//! Distance is a **rate, not a lifetime total**: a people that stops roaming stops earning it and
//! the EMA lets the score fall - the mirror of sedentarization, which a people loses by moving.
//! There are no stages and no prompts; the score is a readout.

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandId, PopulationCohort, ResidentBand},
    intensification::LadderConfigHandle,
    mobility_config::MobilityConfigHandle,
    orders::FactionId,
    resources::DiscoveryProgressLedger,
    roaming_skills::ROAMING_SKILL_DISCOVERY_IDS,
};

/// The raw blend of normalized inputs and weights is scaled to a 0-100 score.
const RAW_SCALE: f32 = 100.0;

/// One faction's current mobility.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MobilityEntry {
    /// EMA-smoothed 0-100 score.
    pub score: f32,
    /// The skills input's raw points this turn - `100 x weights.skills x skills_norm`, before the
    /// EMA that smooths `score`. Published as `MobilityState.skillsPoints`.
    pub skills_points: f32,
    /// The distance input's raw points this turn - `100 x weights.distance x distance_norm`,
    /// before the EMA. Published as `MobilityState.distancePoints`.
    pub distance_points: f32,
}

/// Per-faction mobility scores (`SedentarizationScore`'s shape).
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct MobilityScore {
    entries: HashMap<FactionId, MobilityEntry>,
}

impl MobilityScore {
    pub fn score(&self, faction: FactionId) -> f32 {
        self.entries.get(&faction).map(|e| e.score).unwrap_or(0.0)
    }

    pub fn entry(&self, faction: FactionId) -> Option<&MobilityEntry> {
        self.entries.get(&faction)
    }

    /// `(faction, entry)` pairs in a stable faction order (for snapshotting).
    pub fn iter_sorted(&self) -> Vec<(FactionId, MobilityEntry)> {
        let mut out: Vec<_> = self.entries.iter().map(|(f, e)| (*f, *e)).collect();
        out.sort_by_key(|(f, _)| f.0);
        out
    }
}

/// **Hexes each resident band stepped this turn while following a herd, in any roam state.**
///
/// Written by `advance_band_movement` at the same seam that credits herd lore, but without that
/// lesson's `Migrate`-leg gate (`roaming_skills::walks_beside_a_migration`); DRAINED by [`mobility_tick`] later in
/// the same Population chain, so it is empty at the end of every turn (derived, not checkpointed).
#[derive(Resource, Debug, Clone, Default)]
pub struct HerdFollowSteps {
    steps: BTreeMap<BandId, u32>,
}

impl HerdFollowSteps {
    /// Record `hexes` stepped by `band` this turn (a band steps once a turn; repeats add).
    pub fn record(&mut self, band: BandId, hexes: u32) {
        *self.steps.entry(band).or_insert(0) += hexes;
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    fn drain(&mut self) -> BTreeMap<BandId, u32> {
        std::mem::take(&mut self.steps)
    }
}

/// The skills input: the mean over the roaming skills of `clamp(progress / threshold, 0, 1)`.
/// A world with no ledger knows nothing.
fn skills_norm(
    ledger: Option<&DiscoveryProgressLedger>,
    faction: FactionId,
    completion_threshold: f32,
) -> f32 {
    let Some(ledger) = ledger else {
        return 0.0;
    };
    let threshold = completion_threshold.max(f32::EPSILON);
    let total: f32 = ROAMING_SKILL_DISCOVERY_IDS
        .iter()
        .map(|id| (ledger.get_progress(faction, *id).to_f32() / threshold).clamp(0.0, 1.0))
        .sum();
    total / ROAMING_SKILL_DISCOVERY_IDS.len() as f32
}

/// Per-turn mobility computation (`TurnStage::Population`, after `advance_band_movement` so the
/// turn's steps are recorded - it follows `sedentarization_tick` in the one Population chain).
/// Runs before the Snapshot stage so the score is captured the same turn.
pub fn mobility_tick(
    mut score: ResMut<MobilityScore>,
    mut follow_steps: ResMut<HerdFollowSteps>,
    config: Res<MobilityConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    discovery: Option<Res<DiscoveryProgressLedger>>,
    // `With<ResidentBand>`: a detached expedition is not the people's roaming camp.
    cohorts: Query<(&PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    let cfg = config.get();
    let ladder = ladder.get();
    let steps = follow_steps.drain();
    let w = &cfg.weights;
    let reference = cfg.references.hexes_per_turn.max(f32::EPSILON);
    // Validated `< 1` at load; the clamp guards a hand-built config the same way sedentarization's
    // does, so the update term always leaves the score some movement.
    let smoothing = cfg.smoothing.clamp(0.0, 1.0 - f32::EPSILON);

    // Per-faction total population and sum of (normalized walk x band size); divided by the
    // population, the latter is the population-weighted mean over the resident bands. A band that
    // did not follow a herd contributes 0.
    let mut population: HashMap<FactionId, u64> = HashMap::new();
    let mut walked_weighted: HashMap<FactionId, f32> = HashMap::new();
    for (cohort, band_id) in cohorts.iter() {
        *population.entry(cohort.faction).or_insert(0) += cohort.size as u64;
        let hexes = band_id.map_or(0, |id| steps.get(id).copied().unwrap_or(0));
        let band_norm = (hexes as f32 / reference).clamp(0.0, 1.0);
        *walked_weighted.entry(cohort.faction).or_insert(0.0) += band_norm * cohort.size as f32;
    }

    // Stable faction order so the update is deterministic across runs.
    let mut factions: Vec<FactionId> = population.keys().copied().collect();
    factions.sort_by_key(|f| f.0);

    for faction in factions {
        let pop = population[&faction];
        let distance_norm = if pop == 0 {
            0.0
        } else {
            walked_weighted.get(&faction).copied().unwrap_or(0.0) / pop as f32
        };
        let skills_norm = skills_norm(
            discovery.as_deref(),
            faction,
            ladder.knowledge.completion_threshold,
        );

        let skills_points = RAW_SCALE * w.skills * skills_norm;
        let distance_points = RAW_SCALE * w.distance * distance_norm;
        let raw = skills_points + distance_points;

        let entry = score.entries.entry(faction).or_default();
        entry.skills_points = skills_points;
        entry.distance_points = distance_points;
        // EMA smoothing (sedentarization's convention): `smoothing` weighs the previous score, the
        // first reading starts from 0.
        entry.score = (smoothing * entry.score + (1.0 - smoothing) * raw).clamp(0.0, 100.0);
    }
}
