//! Sedentarization Score — the emergent per-faction "pressure to root in place".
//!
//! Each turn `sedentarization_tick` blends normalized inputs (domestication — the Phase E
//! `HerdRegistry::domesticated_count` seam — plus surplus, map resource density, and
//! population) into a 0–100 score (config-weighted, EMA-smoothed) and, on a *rising* crossing
//! of the soft (~40) / hard (~70) thresholds, pushes a `SedentarizationPrompt` to the command
//! feed. The score is exported per-faction in the snapshot (a HUD meter). No new entities —
//! this is the first slice of the pastoral→settlement chain (`Camp`, corrals, and wiring
//! `found_settlement` to the hard prompt stay deferred).

use std::collections::HashMap;

use bevy::prelude::*;
use tracing::info;

use crate::{
    components::PopulationCohort,
    fauna::{HerdDensityMap, HerdRegistry},
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, FactionInventory, SimulationTick,
    },
    sedentarization_config::{SedentarizationConfig, SedentarizationConfigHandle},
};

/// Which settle-prompt threshold a faction has currently crossed. Ordered so a *rising* stage
/// (`new > stored`) edge-gates the prompt emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SedentarizationStage {
    /// Below the soft threshold — still comfortably nomadic.
    #[default]
    None,
    /// Soft prompt band — "establish a seasonal base?".
    Soft,
    /// Hard prompt band — "invest in storehouses and settle?".
    Hard,
}

impl SedentarizationStage {
    /// Stable string key (also the snapshot `stage` field).
    pub fn as_str(self) -> &'static str {
        match self {
            SedentarizationStage::None => "none",
            SedentarizationStage::Soft => "soft",
            SedentarizationStage::Hard => "hard",
        }
    }

    fn for_score(score: f32, cfg: &SedentarizationConfig) -> Self {
        if score >= cfg.hard_threshold {
            SedentarizationStage::Hard
        } else if score >= cfg.soft_threshold {
            SedentarizationStage::Soft
        } else {
            SedentarizationStage::None
        }
    }
}

/// One faction's current sedentarization pressure.
#[derive(Debug, Clone, Copy, Default)]
pub struct SedentarizationEntry {
    /// EMA-smoothed 0–100 score.
    pub score: f32,
    /// Highest prompt threshold currently crossed (edge-gates re-prompting).
    pub stage: SedentarizationStage,
}

/// Per-faction sedentarization scores (mirrors `FactionInventory`'s per-faction map shape).
#[derive(Resource, Debug, Clone, Default)]
pub struct SedentarizationScore {
    entries: HashMap<FactionId, SedentarizationEntry>,
}

impl SedentarizationScore {
    pub fn score(&self, faction: FactionId) -> f32 {
        self.entries.get(&faction).map(|e| e.score).unwrap_or(0.0)
    }

    pub fn entry(&self, faction: FactionId) -> Option<&SedentarizationEntry> {
        self.entries.get(&faction)
    }

    /// `(faction, entry)` pairs in a stable faction order (for snapshotting).
    pub fn iter_sorted(&self) -> Vec<(FactionId, SedentarizationEntry)> {
        let mut out: Vec<_> = self.entries.iter().map(|(f, e)| (*f, *e)).collect();
        out.sort_by_key(|(f, _)| f.0);
        out
    }
}

/// Player-facing prompt text for a rising stage crossing.
fn prompt_label(stage: SedentarizationStage) -> &'static str {
    match stage {
        SedentarizationStage::Soft => {
            "Sedentarization — your people feel the pull to establish a seasonal base."
        }
        SedentarizationStage::Hard => {
            "Sedentarization — time to invest in storehouses and found a settlement?"
        }
        // Never emitted (a prompt only fires on a rise into Soft/Hard).
        SedentarizationStage::None => "Sedentarization",
    }
}

/// Per-turn sedentarization computation (`TurnStage::Population`, after `advance_fauna_pursuits`
/// so the turn's domestication is current). Runs before the Snapshot stage so the score is
/// captured the same turn.
// Bevy system signature: each param is a distinct resource/query the score needs (the score
// state + prompt log + tick, the config, and the four inputs); they can't be collapsed without
// a container resource that adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn sedentarization_tick(
    mut score: ResMut<SedentarizationScore>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    config: Res<SedentarizationConfigHandle>,
    inventory: Res<FactionInventory>,
    herds: Res<HerdRegistry>,
    density: Res<HerdDensityMap>,
    cohorts: Query<&PopulationCohort>,
) {
    let cfg = config.get();

    // Per-faction total population (the set of active factions to score).
    let mut population: HashMap<FactionId, u64> = HashMap::new();
    for cohort in cohorts.iter() {
        *population.entry(cohort.faction).or_insert(0) += cohort.size as u64;
    }

    // Map-wide game richness (v1 environmental baseline; per-faction-local density is a
    // documented future refinement).
    let resource_density = density.normalized_average().clamp(0.0, 1.0);
    let refs = &cfg.references;
    let w = &cfg.weights;
    // Guard against a malformed env-override config: `< 0` would make the update term
    // negative, and `>= 1.0` would zero it and freeze the score forever — so cap strictly
    // below 1.0 (the largest representable float under 1) to always leave some movement.
    let smoothing = cfg.smoothing.clamp(0.0, 1.0 - f32::EPSILON);

    // Process factions in a stable order so prompt/command-feed ordering is deterministic
    // across runs (a `HashMap` iterates arbitrarily).
    let mut factions: Vec<FactionId> = population.keys().copied().collect();
    factions.sort_by_key(|f| f.0);

    for faction in factions {
        let pop = population[&faction];
        let domesticated = herds.domesticated_count(faction) as f32;
        let surplus = inventory
            .stockpile(faction)
            .and_then(|items| items.get("provisions"))
            .copied()
            .unwrap_or(0)
            .max(0) as f32;

        let dom_norm = (domesticated / (refs.domesticated_herds.max(1) as f32)).clamp(0.0, 1.0);
        let sur_norm = (surplus / refs.surplus.max(f32::EPSILON)).clamp(0.0, 1.0);
        let pop_norm = (pop as f32 / refs.population.max(f32::EPSILON)).clamp(0.0, 1.0);

        let raw = 100.0
            * (w.domestication * dom_norm
                + w.surplus * sur_norm
                + w.resource_density * resource_density
                + w.population * pop_norm);

        let entry = score.entries.entry(faction).or_default();
        // EMA smoothing (victory_tick pattern) so the pressure builds gradually.
        entry.score = (smoothing * entry.score + (1.0 - smoothing) * raw).clamp(0.0, 100.0);

        let new_stage = SedentarizationStage::for_score(entry.score, &cfg);
        // Edge-gate: only narrate a *rising* threshold crossing (a fall lowers the stage
        // silently so a later re-crossing re-prompts).
        if new_stage > entry.stage {
            event_log.push(CommandEventEntry::new(
                tick.0,
                CommandEventKind::SedentarizationPrompt,
                faction,
                prompt_label(new_stage),
                Some(format!(
                    "stage={} score={:.0}",
                    new_stage.as_str(),
                    entry.score
                )),
            ));
            info!(
                target: "shadow_scale::analytics",
                event = "sedentarization",
                faction = faction.0,
                score = entry.score,
                stage = new_stage.as_str(),
            );
        }
        entry.stage = new_stage;
    }
}
