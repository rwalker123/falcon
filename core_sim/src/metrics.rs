use bevy::prelude::*;

use crate::{
    components::{LogisticsLink, PopulationCohort, Tile, TradeLink},
    crisis::CrisisTelemetry,
    power::PowerGridState,
    resources::{SimulationConfig, SimulationTick},
    scalar::{scalar_from_u32, Scalar},
};

#[derive(Resource, Default, Debug, Clone)]
pub struct SimulationMetrics {
    pub turn: u64,
    pub total_mass: i128,
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
    pub trade_openness_avg: f32,
    pub logistics_flow_avg: f32,
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
    trade_links: Query<&TradeLink>,
    logistics_links: Query<&LogisticsLink>,
) {
    metrics.turn += 1;
    let mut total_mass = 0i128;
    let mut total_temp = 0f64;
    let mut count = 0u64;

    for tile in tiles.iter() {
        total_mass += tile.mass.raw() as i128;
        total_temp += tile.temperature.to_f32() as f64;
        count += 1;
    }

    metrics.total_mass = total_mass;
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

    let mut population_total = 0u64;
    let mut morale_total = Scalar::zero();
    let mut cohort_count = 0u32;
    for cohort in populations.iter() {
        population_total = population_total.saturating_add(cohort.size as u64);
        morale_total += cohort.morale;
        cohort_count = cohort_count.saturating_add(1);
    }
    metrics.population_total = population_total;
    metrics.population_morale_avg = if cohort_count > 0 {
        (morale_total / scalar_from_u32(cohort_count))
            .to_f32()
            .clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut openness_total = Scalar::zero();
    let mut trade_count = 0u32;
    for link in trade_links.iter() {
        openness_total += link.openness.clamp(Scalar::zero(), Scalar::one());
        trade_count = trade_count.saturating_add(1);
    }
    metrics.trade_openness_avg = if trade_count > 0 {
        (openness_total / scalar_from_u32(trade_count))
            .to_f32()
            .clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut flow_total = Scalar::zero();
    let mut flow_count = 0u32;
    for link in logistics_links.iter() {
        flow_total += link.flow.max(Scalar::zero());
        flow_count = flow_count.saturating_add(1);
    }
    metrics.logistics_flow_avg = if flow_count > 0 {
        (flow_total / scalar_from_u32(flow_count))
            .to_f32()
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
}
