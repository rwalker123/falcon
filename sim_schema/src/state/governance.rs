//! Governance-section state: the power grid, corruption ledger, and crisis gauges.

use crate::state::map::ScalarRasterState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisMetricKind {
    #[default]
    R0 = 0,
    GridStressPct = 1,
    UnauthorizedQueuePct = 2,
    SwarmsActive = 3,
    PhageDensity = 4,
}

impl CrisisMetricKind {
    pub const VALUES: [CrisisMetricKind; 5] = [
        CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisSeverityBand {
    #[default]
    Safe = 0,
    Warn = 1,
    Critical = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTrendSample {
    pub tick: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisGaugeState {
    pub kind: CrisisMetricKind,
    pub raw: f32,
    pub ema: f32,
    pub trend_5t: f32,
    pub warn_threshold: f32,
    pub critical_threshold: f32,
    pub last_updated_tick: u64,
    pub stale_ticks: u64,
    pub band: CrisisSeverityBand,
    pub history: Vec<CrisisTrendSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTelemetryState {
    pub gauges: Vec<CrisisGaugeState>,
    pub modifiers_active: u32,
    pub foreshock_incidents: u32,
    pub containment_incidents: u32,
    pub warnings_active: u32,
    pub criticals_active: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayAnnotationState {
    pub label: String,
    pub severity: CrisisSeverityBand,
    pub path: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayState {
    pub heatmap: ScalarRasterState,
    pub annotations: Vec<CrisisOverlayAnnotationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowerNodeState {
    pub entity: u64,
    pub node_id: u32,
    pub generation: i64,
    pub demand: i64,
    pub efficiency: i64,
    pub storage_level: i64,
    pub storage_capacity: i64,
    pub stability: i64,
    pub surplus: i64,
    pub deficit: i64,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PowerIncidentSeverity {
    #[default]
    Warning = 0,
    Critical = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerIncidentState {
    pub node_id: u32,
    pub severity: PowerIncidentSeverity,
    pub deficit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerTelemetryState {
    pub total_supply: i64,
    pub total_demand: i64,
    pub total_storage: i64,
    pub total_capacity: i64,
    pub grid_stress_avg: f32,
    pub surplus_margin: f32,
    pub instability_alerts: u32,
    pub incidents: Vec<PowerIncidentState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CorruptionSubsystem {
    #[default]
    Logistics = 0,
    Trade = 1,
    Military = 2,
    Governance = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionEntry {
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub incident_id: u64,
    pub exposure_timer: u16,
    pub restitution_window: u16,
    pub last_update_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionLedger {
    pub entries: Vec<CorruptionEntry>,
    pub reputation_modifier: i64,
    pub audit_capacity: u16,
}

impl CorruptionLedger {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn register_incident(&mut self, entry: CorruptionEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.incident_id == entry.incident_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn incident_mut(&mut self, incident_id: u64) -> Option<&mut CorruptionEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.incident_id == incident_id)
    }

    pub fn remove_incident(&mut self, incident_id: u64) -> Option<CorruptionEntry> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.incident_id == incident_id)?;
        Some(self.entries.remove(index))
    }
}
