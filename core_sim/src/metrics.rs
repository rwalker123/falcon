use bevy::prelude::*;

use crate::{components::Tile, power::PowerGridState, resources::SimulationConfig};

#[derive(Resource, Default, Debug, Clone)]
pub struct SimulationMetrics {
    pub turn: u64,
    pub total_mass: i128,
    pub avg_temperature: f64,
    pub grid_size: (u32, u32),
    pub grid_stress_avg: f32,
    pub grid_surplus_margin: f32,
    pub instability_alerts: u32,
}

pub fn collect_metrics(
    config: Res<SimulationConfig>,
    mut metrics: ResMut<SimulationMetrics>,
    tiles: Query<&Tile>,
    power: Option<Res<PowerGridState>>,
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
}
