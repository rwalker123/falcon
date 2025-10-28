use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use bevy::{math::UVec2, prelude::*};
use sim_runtime::{CorruptionLedger, CorruptionSubsystem};

use crate::{
    culture::CultureTensionRecord,
    orders::FactionId,
    scalar::{scalar_from_f32, Scalar},
};

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
    pub max_power_efficiency: Scalar,
    pub min_power_influence: f32,
    pub max_power_influence: f32,
    pub power_generation_adjust_rate: f32,
    pub power_demand_adjust_rate: f32,
    pub power_storage_stability_bonus: f32,
    pub power_line_capacity: Scalar,
    pub power_storage_efficiency: Scalar,
    pub power_storage_bleed: Scalar,
    pub power_instability_warn: Scalar,
    pub power_instability_critical: Scalar,
    pub mass_flux_epsilon: Scalar,
    pub base_trade_tariff: Scalar,
    pub base_trade_openness: Scalar,
    pub trade_openness_decay: Scalar,
    pub trade_leak_min_ticks: u32,
    pub trade_leak_max_ticks: u32,
    pub trade_leak_exponent: f32,
    pub trade_leak_progress: Scalar,
    pub migration_fragment_scaling: Scalar,
    pub migration_fidelity_floor: Scalar,
    pub corruption_logistics_penalty: Scalar,
    pub corruption_trade_penalty: Scalar,
    pub corruption_military_penalty: Scalar,
    pub snapshot_bind: SocketAddr,
    pub snapshot_flat_bind: SocketAddr,
    pub command_bind: SocketAddr,
    pub log_bind: SocketAddr,
    pub snapshot_history_limit: usize,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            grid_size: UVec2::new(80, 52),
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
            max_power_efficiency: scalar_from_f32(1.75),
            min_power_influence: -1.5,
            max_power_influence: 1.5,
            power_generation_adjust_rate: 0.4,
            power_demand_adjust_rate: 0.25,
            power_storage_stability_bonus: 0.25,
            power_line_capacity: scalar_from_f32(4.0),
            power_storage_efficiency: scalar_from_f32(0.85),
            power_storage_bleed: scalar_from_f32(0.02),
            power_instability_warn: scalar_from_f32(0.4),
            power_instability_critical: scalar_from_f32(0.2),
            mass_flux_epsilon: scalar_from_f32(0.001),
            base_trade_tariff: scalar_from_f32(0.08),
            base_trade_openness: scalar_from_f32(0.35),
            trade_openness_decay: scalar_from_f32(0.005),
            trade_leak_min_ticks: 3,
            trade_leak_max_ticks: 12,
            trade_leak_exponent: 1.4,
            trade_leak_progress: scalar_from_f32(0.12),
            migration_fragment_scaling: scalar_from_f32(0.25),
            migration_fidelity_floor: scalar_from_f32(0.35),
            corruption_logistics_penalty: scalar_from_f32(0.35),
            corruption_trade_penalty: scalar_from_f32(0.3),
            corruption_military_penalty: scalar_from_f32(0.4),
            snapshot_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41000),
            snapshot_flat_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41002),
            command_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41001),
            log_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41003),
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

    pub fn total_intensity(&self, subsystem: CorruptionSubsystem) -> i64 {
        self.ledger
            .entries
            .iter()
            .filter(|entry| entry.subsystem == subsystem)
            .map(|entry| entry.intensity.max(0))
            .sum()
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
    pub culture_signals: Vec<CultureTensionRecord>,
    pub great_discoveries: Vec<(FactionId, u16)>,
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

    pub fn push_culture_signal(&mut self, record: CultureTensionRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.culture_signals.push(record);
        if self.culture_signals.len() > self.max_entries {
            let overflow = self.culture_signals.len() - self.max_entries;
            self.culture_signals.drain(0..overflow);
        }
    }

    pub fn push_great_discovery(&mut self, faction: FactionId, discovery_id: u16) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.great_discoveries.push((faction, discovery_id));
        if self.great_discoveries.len() > self.max_entries {
            let overflow = self.great_discoveries.len() - self.max_entries;
            self.great_discoveries.drain(0..overflow);
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PendingCrisisSeeds {
    pub seeds: Vec<(FactionId, u16)>,
}

impl PendingCrisisSeeds {
    pub fn push(&mut self, faction: FactionId, discovery_id: u16) {
        self.seeds.push((faction, discovery_id));
    }

    pub fn drain(&mut self) -> Vec<(FactionId, u16)> {
        std::mem::take(&mut self.seeds)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiscoveryProgressLedger {
    pub progress: HashMap<FactionId, HashMap<u32, Scalar>>,
}

impl DiscoveryProgressLedger {
    pub fn add_progress(&mut self, faction: FactionId, discovery_id: u32, delta: Scalar) -> Scalar {
        let faction_entry = self.progress.entry(faction).or_default();
        let entry = faction_entry
            .entry(discovery_id)
            .or_insert_with(Scalar::zero);
        *entry = (*entry + delta).clamp(Scalar::zero(), Scalar::one());
        *entry
    }

    pub fn get_progress(&self, faction: FactionId, discovery_id: u32) -> Scalar {
        self.progress
            .get(&faction)
            .and_then(|map| map.get(&discovery_id))
            .copied()
            .unwrap_or_else(Scalar::zero)
    }
}

#[derive(Debug, Clone)]
pub struct TradeDiffusionRecord {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct TradeTelemetry {
    pub tech_diffusion_applied: u32,
    pub migration_transfers: u32,
    pub records: Vec<TradeDiffusionRecord>,
}

impl TradeTelemetry {
    pub fn reset_turn(&mut self) {
        self.tech_diffusion_applied = 0;
        self.migration_transfers = 0;
        self.records.clear();
    }

    pub fn push_record(&mut self, record: TradeDiffusionRecord) {
        self.records.push(record);
    }
}
