use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::{math::UVec2, prelude::*};

use crate::scalar::{scalar_from_f32, Scalar};

/// Global configuration parameters for the headless simulation prototype.
#[derive(Resource, Debug, Clone)]
pub struct SimulationConfig {
    pub grid_size: UVec2,
    pub ambient_temperature: Scalar,
    pub temperature_lerp: Scalar,
    pub logistics_flow_gain: Scalar,
    pub base_link_capacity: Scalar,
    pub mass_bounds: (Scalar, Scalar),
    pub population_growth_rate: Scalar,
    pub temperature_morale_penalty: Scalar,
    pub population_cluster_stride: u32,
    pub population_cap: u32,
    pub power_adjust_rate: Scalar,
    pub max_power_generation: Scalar,
    pub mass_flux_epsilon: Scalar,
    pub snapshot_bind: SocketAddr,
    pub command_bind: SocketAddr,
    pub snapshot_history_limit: usize,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            grid_size: UVec2::new(32, 32),
            ambient_temperature: scalar_from_f32(18.0),
            temperature_lerp: scalar_from_f32(0.05),
            logistics_flow_gain: scalar_from_f32(0.1),
            base_link_capacity: scalar_from_f32(0.8),
            mass_bounds: (scalar_from_f32(0.2), scalar_from_f32(15.0)),
            population_growth_rate: scalar_from_f32(0.01),
            temperature_morale_penalty: scalar_from_f32(0.004),
            population_cluster_stride: 8,
            population_cap: 25_000,
            power_adjust_rate: scalar_from_f32(0.02),
            max_power_generation: scalar_from_f32(25.0),
            mass_flux_epsilon: scalar_from_f32(0.001),
            snapshot_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41000),
            command_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001),
            snapshot_history_limit: 256,
        }
    }
}

/// Tracks total simulation ticks elapsed.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationTick(pub u64);

/// Authoritative sentiment axis bias values applied across factions.
#[derive(Resource, Debug, Clone)]
pub struct SentimentAxisBias {
    pub values: [Scalar; 4],
}

impl Default for SentimentAxisBias {
    fn default() -> Self {
        Self {
            values: [Scalar::zero(); 4],
        }
    }
}

impl SentimentAxisBias {
    pub fn set_axis(&mut self, axis: usize, value: Scalar) {
        if let Some(slot) = self.values.get_mut(axis) {
            *slot = value;
        }
    }

    pub fn as_raw(&self) -> [i64; 4] {
        self.values.map(Scalar::raw)
    }
}

/// Index of tile entities for reuse by other systems.
#[derive(Resource, Debug, Clone)]
pub struct TileRegistry {
    pub tiles: Vec<Entity>,
    pub width: u32,
    pub height: u32,
}

impl TileRegistry {
    pub fn index(&self, x: u32, y: u32) -> Option<Entity> {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.tiles.get(idx).cloned()
        } else {
            None
        }
    }
}
