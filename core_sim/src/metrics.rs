use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    components::{PopulationCohort, Tile},
    crisis::CrisisTelemetry,
    fauna::HerdDensityMap,
    orders::FactionId,
    power::PowerGridState,
    resources::{SimulationConfig, SimulationTick},
    scalar::{scalar_from_u32, Scalar},
};

/// **One people's own figures**, as against the world's.
///
/// A victory threshold measures a *faction* — "control a dominant share of population" is a claim
/// about one people, and summing every cohort on the map answered it with the world's population
/// instead, so a rival's growth advanced your hegemony and a rival's misery dragged your morale
/// score. Every term here is therefore keyed by whose people it counts.
///
/// A faction with no row has **no people**, which is what an absent entry means and what
/// [`SimulationMetrics::for_faction`] answers with: zeros are the truth for a people that has died
/// out, not a gap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FactionMetrics {
    /// Heads in this faction's cohorts.
    pub population_total: u64,
    /// Mean cohort morale across this faction's cohorts, `0.0` when it has none.
    pub population_morale_avg: f32,
}

#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
pub struct SimulationMetrics {
    pub turn: u64,
    pub avg_temperature: f64,
    pub grid_size: (u32, u32),
    pub grid_stress_avg: f32,
    pub grid_surplus_margin: f32,
    pub instability_alerts: u32,
    pub great_discoveries_total: u32,
    pub great_discovery_candidates: u32,
    pub great_discovery_active: u32,
    pub knowledge_leak_warnings: u32,
    pub knowledge_leak_criticals: u32,
    pub knowledge_countermeasures_active: u32,
    pub knowledge_common_knowledge_total: u32,
    pub knowledge_counterintel_budget_spent: f64,
    pub crisis: crate::crisis::CrisisMetricsSnapshot,
    pub population_total: u64,
    pub population_morale_avg: f32,
    /// **The same two figures, split by whose people they are** — written in the same cohort pass
    /// as the world totals above, so the two can never describe different sets of bands.
    ///
    /// The world figures stay: several readers legitimately want *"how many people are alive on this
    /// map"*, and a world metric published to nobody is not a disclosure. What changed is that
    /// **victory** reads this map instead (`victory.rs`).
    pub population_by_faction: BTreeMap<FactionId, FactionMetrics>,
    /// **Great discoveries resolved, per faction** — the twin of [`Self::great_discoveries_total`]
    /// beside it, and defined the same way: *how many rows of the ledger name this faction*.
    ///
    /// It has its own home rather than a field on [`FactionMetrics`] because it has its own
    /// **owner**: `export_great_discovery_metrics` is the only system that can see the ledger, and
    /// it runs a stage before `collect_metrics` rebuilds the cohort map — a shared map would have
    /// this count written first and then wiped.
    pub great_discoveries_by_faction: BTreeMap<FactionId, u32>,
    pub herd_density_avg: f32,
    pub herd_density_peak: f32,
    pub herd_density_ratio: f32,
    /// Directed ties standing in the [`crate::connections::ConnectionLedger`] after this turn's
    /// clocks ran — a parked edge at strength 0 still counts, because the edge is the memory.
    ///
    /// The three below are written by `advance_connections` in the Visibility stage rather than by
    /// [`collect_metrics`]: only that system knows which edges *formed* and which were *reaped*,
    /// and re-deriving either from the ledger afterwards is impossible (both are differences).
    pub connections_live: u32,
    /// Edges that did not exist before this turn's contacts.
    pub connections_formed: u32,
    /// Edges removed this turn by clock 3 — nobody has seen those people in `forget_turns`.
    pub connections_reaped: u32,
}

impl SimulationMetrics {
    /// **This faction's own population and morale.** An unlisted faction has no cohorts, which is a
    /// people with nobody left rather than missing data — so it answers zeros rather than `None`,
    /// and a victory threshold measures it as the nothing it is.
    pub fn for_faction(&self, faction: FactionId) -> FactionMetrics {
        self.population_by_faction
            .get(&faction)
            .copied()
            .unwrap_or_default()
    }

    /// How many great discoveries **this faction** has resolved.
    pub fn great_discoveries_for(&self, faction: FactionId) -> u32 {
        self.great_discoveries_by_faction
            .get(&faction)
            .copied()
            .unwrap_or_default()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn collect_metrics(
    config: Res<SimulationConfig>,
    mut metrics: ResMut<SimulationMetrics>,
    tiles: Query<&Tile>,
    power: Option<Res<PowerGridState>>,
    crisis: Res<CrisisTelemetry>,
    tick: Res<SimulationTick>,
    populations: Query<&PopulationCohort>,
    herd_density: Res<HerdDensityMap>,
) {
    metrics.turn += 1;
    let mut total_temp = 0f64;
    let mut count = 0u64;

    for tile in tiles.iter() {
        total_temp += tile.temperature.to_f32() as f64;
        count += 1;
    }

    metrics.avg_temperature = if count > 0 {
        total_temp / count as f64
    } else {
        0.0
    };
    metrics.grid_size = (config.grid_size.x, config.grid_size.y);

    if let Some(power_state) = power {
        metrics.grid_stress_avg = power_state.grid_stress_avg;
        metrics.grid_surplus_margin = power_state.surplus_margin;
        metrics.instability_alerts = power_state.instability_alerts;
    } else {
        metrics.grid_stress_avg = 0.0;
        metrics.grid_surplus_margin = 0.0;
        metrics.instability_alerts = 0;
    }

    metrics.crisis = crisis.snapshot(tick.0);
    crisis.log_telemetry(tick.0);

    // **One pass, two answers.** The world totals and the per-faction split are accumulated
    // together rather than in two queries, so nothing can make them describe different sets of
    // bands — the failure that would let a faction's own progress disagree with the map it stands
    // on.
    let mut population_total = 0u64;
    let mut morale_total = Scalar::zero();
    let mut cohort_count = 0u32;
    let mut per_faction: BTreeMap<FactionId, (u64, Scalar, u32)> = BTreeMap::new();
    for cohort in populations.iter() {
        population_total = population_total.saturating_add(cohort.size as u64);
        morale_total += cohort.morale;
        cohort_count = cohort_count.saturating_add(1);
        let entry = per_faction
            .entry(cohort.faction)
            .or_insert((0, Scalar::zero(), 0));
        entry.0 = entry.0.saturating_add(cohort.size as u64);
        entry.1 += cohort.morale;
        entry.2 = entry.2.saturating_add(1);
    }
    metrics.population_total = population_total;
    metrics.population_morale_avg = if cohort_count > 0 {
        (morale_total / scalar_from_u32(cohort_count))
            .to_f32()
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    metrics.population_by_faction = per_faction
        .into_iter()
        .map(|(faction, (size, morale, cohorts))| {
            (
                faction,
                FactionMetrics {
                    population_total: size,
                    population_morale_avg: if cohorts > 0 {
                        (morale / scalar_from_u32(cohorts)).to_f32().clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                },
            )
        })
        .collect();

    metrics.herd_density_avg = herd_density.average_density();
    metrics.herd_density_peak = herd_density.max_density();
    metrics.herd_density_ratio = herd_density.normalized_average();
}
