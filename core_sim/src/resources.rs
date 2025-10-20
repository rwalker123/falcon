use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::{math::UVec2, prelude::*};
use sim_runtime::{CorruptionLedger, CorruptionSubsystem};

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
    pub snapshot_flat_bind: SocketAddr,
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
            snapshot_flat_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41002),
            command_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001),
            snapshot_history_limit: 256,
        }
    }
}

/// Tracks total simulation ticks elapsed.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationTick(pub u64);

/// Authoritative sentiment axis bias values applied across factions.
///
/// Sentiment is composed of three categories of forces:
/// - **Policy levers** (`policy`): long-lived adjustments driven by enacted reforms or manual tweaks.
/// - **Incident deltas** (`incidents`): short-lived shocks produced by exposed scandals, crises, etc.
/// - **Influencer output** (`influencer`): procedurally generated contributions from the influencer roster.
#[derive(Resource, Debug, Clone)]
pub struct SentimentAxisBias {
    policy: [Scalar; 4],
    incidents: [Scalar; 4],
    influencer: [Scalar; 4],
}

impl Default for SentimentAxisBias {
    fn default() -> Self {
        Self {
            policy: [Scalar::zero(); 4],
            incidents: [Scalar::zero(); 4],
            influencer: [Scalar::zero(); 4],
        }
    }
}

impl SentimentAxisBias {
    pub fn set_policy_axis(&mut self, axis: usize, value: Scalar) {
        if let Some(slot) = self.policy.get_mut(axis) {
            *slot = value;
        }
    }

    pub fn set_policy_axes(&mut self, values: [Scalar; 4]) {
        self.policy = values;
    }

    pub fn policy_values(&self) -> [Scalar; 4] {
        self.policy
    }

    pub fn set_influencer(&mut self, deltas: [Scalar; 4]) {
        self.influencer = deltas;
    }

    pub fn influencer_values(&self) -> [Scalar; 4] {
        self.influencer
    }

    pub fn incident_values(&self) -> [Scalar; 4] {
        self.incidents
    }

    pub fn apply_incident_delta(&mut self, axis: usize, delta: Scalar) {
        if let Some(slot) = self.incidents.get_mut(axis) {
            *slot = (*slot + delta).clamp(Scalar::from_f32(-2.0), Scalar::from_f32(2.0));
        }
    }

    pub fn reset_incidents(&mut self) {
        self.incidents = [Scalar::zero(); 4];
    }

    pub fn manual_environment(&self) -> [Scalar; 4] {
        let mut result = self.policy;
        for (idx, incident) in self.incidents.iter().enumerate() {
            result[idx] += *incident;
        }
        result
    }

    pub fn combined(&self) -> [Scalar; 4] {
        let mut result = self.manual_environment();
        for (idx, delta) in self.influencer.iter().enumerate() {
            result[idx] += *delta;
        }
        result
    }

    pub fn as_raw(&self) -> [i64; 4] {
        self.combined().map(Scalar::raw)
    }

    pub fn reset_to_state(&mut self, policy: [Scalar; 4], incidents: [Scalar; 4]) {
        self.policy = policy;
        self.incidents = incidents;
        self.influencer = [Scalar::zero(); 4];
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

/// Tracks corruption intensity across subsystems for snapshot export.
#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionLedgers {
    ledger: CorruptionLedger,
}

impl CorruptionLedgers {
    pub fn ledger(&self) -> &CorruptionLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut CorruptionLedger {
        &mut self.ledger
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorruptionExposureRecord {
    pub incident_id: u64,
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub trust_delta: i64,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionTelemetry {
    pub active_incidents: usize,
    pub exposures_this_turn: Vec<CorruptionExposureRecord>,
    pub exposures_total: u64,
}

impl CorruptionTelemetry {
    pub fn reset_turn(&mut self) {
        self.exposures_this_turn.clear();
    }

    pub fn record_exposure(&mut self, record: CorruptionExposureRecord) {
        self.exposures_this_turn.push(record);
        self.exposures_total += 1;
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiplomacyLeverage {
    pub recent: Vec<CorruptionExposureRecord>,
    pub max_entries: usize,
}

impl DiplomacyLeverage {
    pub fn push(&mut self, record: CorruptionExposureRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.recent.push(record);
        if self.recent.len() > self.max_entries {
            let overflow = self.recent.len() - self.max_entries;
            self.recent.drain(0..overflow);
        }
    }
}
