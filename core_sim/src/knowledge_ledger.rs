use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bevy::prelude::*;
use log::debug;
use serde::Deserialize;
use sim_runtime::knowledge::{
    KnowledgeTelemetryEvent, KnowledgeTelemetryFrame, KnowledgeTelemetryMission,
    KNOWLEDGE_TELEMETRY_TOPIC,
};
use sim_runtime::{
    encode_knowledge_ledger_key, KnowledgeCountermeasureKind, KnowledgeCountermeasureState,
    KnowledgeInfiltrationState, KnowledgeLeakFlags, KnowledgeLedgerEntryState,
    KnowledgeMetricsState, KnowledgeModifierBreakdownState, KnowledgeModifierSource,
    KnowledgeSecurityPosture, KnowledgeTimelineEventKind, KnowledgeTimelineEventState,
    WorldSnapshot,
};

use crate::{
    espionage::{EspionageCatalog, EspionageMissionKind, EspionageMissionTemplate},
    metrics::SimulationMetrics,
    orders::FactionId,
    resources::SimulationTick,
    scalar::Scalar,
};

pub const BUILTIN_KNOWLEDGE_LEDGER_CONFIG: &str = include_str!("data/knowledge_ledger_config.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KnowledgeLedgerConfig {
    timeline_capacity: usize,
    default_half_life_ticks: u16,
    default_time_to_cascade: u16,
    max_suspicion: f32,
    suspicion_decay: f32,
    suspicion_retention_threshold: f32,
    countermeasure_bonus_scale: f32,
    countermeasure_progress_penalty_ratio: f32,
    infiltration_cells_weight: f32,
    infiltration_fidelity_weight: f32,
    max_progress_per_tick: i32,
}

impl KnowledgeLedgerConfig {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn timeline_capacity(&self) -> usize {
        self.timeline_capacity
    }

    pub fn default_half_life_ticks(&self) -> u16 {
        self.default_half_life_ticks
    }

    pub fn default_time_to_cascade(&self) -> u16 {
        self.default_time_to_cascade
    }

    pub fn max_suspicion(&self) -> Scalar {
        Scalar::from_f32(self.max_suspicion)
    }

    pub fn suspicion_decay(&self) -> Scalar {
        Scalar::from_f32(self.suspicion_decay)
    }

    pub fn suspicion_retention_threshold(&self) -> Scalar {
        Scalar::from_f32(self.suspicion_retention_threshold)
    }

    pub fn countermeasure_bonus_scale(&self) -> f32 {
        self.countermeasure_bonus_scale
    }

    pub fn countermeasure_progress_penalty_ratio(&self) -> f32 {
        self.countermeasure_progress_penalty_ratio
    }

    pub fn infiltration_cells_weight(&self) -> f32 {
        self.infiltration_cells_weight
    }

    pub fn infiltration_fidelity_weight(&self) -> f32 {
        self.infiltration_fidelity_weight
    }

    pub fn max_progress_per_tick(&self) -> i32 {
        self.max_progress_per_tick
    }
}

impl Default for KnowledgeLedgerConfig {
    fn default() -> Self {
        Self {
            timeline_capacity: 64,
            default_half_life_ticks: 10,
            default_time_to_cascade: 10,
            max_suspicion: 5.0,
            suspicion_decay: 0.05,
            suspicion_retention_threshold: 0.05,
            countermeasure_bonus_scale: 4.0,
            countermeasure_progress_penalty_ratio: 0.5,
            infiltration_cells_weight: 1.0,
            infiltration_fidelity_weight: 2.0,
            max_progress_per_tick: 25,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct KnowledgeLedgerConfigHandle(pub Arc<KnowledgeLedgerConfig>);

impl KnowledgeLedgerConfigHandle {
    pub fn new(config: Arc<KnowledgeLedgerConfig>) -> Self {
        Self(config)
    }

    pub fn load_builtin() -> Self {
        let parsed = KnowledgeLedgerConfig::from_json_str(BUILTIN_KNOWLEDGE_LEDGER_CONFIG)
            .unwrap_or_else(|err| panic!("failed to parse builtin knowledge ledger config: {err}"));
        Self(Arc::new(parsed))
    }

    pub fn get(&self) -> Arc<KnowledgeLedgerConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace_from_json(
        &mut self,
        json: &str,
    ) -> Result<Arc<KnowledgeLedgerConfig>, serde_json::Error> {
        let parsed = KnowledgeLedgerConfig::from_json_str(json)?;
        let shared = Arc::new(parsed);
        self.0 = Arc::clone(&shared);
        Ok(shared)
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeCountermeasure {
    pub kind: KnowledgeCountermeasureKind,
    pub potency: Scalar,
    pub upkeep: Scalar,
    pub remaining_ticks: u16,
}

#[derive(Debug, Clone)]
pub struct InfiltrationRecord {
    pub faction: FactionId,
    pub blueprint_fidelity: Scalar,
    pub suspicion: Scalar,
    pub cells: u8,
    pub last_activity_tick: u64,
}

#[derive(Debug, Clone)]
pub struct KnowledgeModifier {
    pub source: KnowledgeModifierSource,
    pub delta_half_life: i16,
    pub delta_progress: i16,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeLedgerEntry {
    pub discovery_id: u32,
    pub owner_faction: FactionId,
    pub tier: u8,
    pub progress_percent: u16,
    pub half_life_ticks: u16,
    pub time_to_cascade: u16,
    pub security_posture: KnowledgeSecurityPosture,
    pub countermeasures: Vec<KnowledgeCountermeasure>,
    pub infiltrations: Vec<InfiltrationRecord>,
    pub modifiers: Vec<KnowledgeModifier>,
    pub flags: KnowledgeLeakFlags,
}

impl KnowledgeLedgerEntry {
    pub fn new(owner: FactionId, discovery_id: u32, config: &KnowledgeLedgerConfig) -> Self {
        Self {
            discovery_id,
            owner_faction: owner,
            tier: 0,
            progress_percent: 0,
            half_life_ticks: config.default_half_life_ticks(),
            time_to_cascade: config.default_time_to_cascade(),
            security_posture: KnowledgeSecurityPosture::Standard,
            countermeasures: Vec::new(),
            infiltrations: Vec::new(),
            modifiers: Vec::new(),
            flags: KnowledgeLeakFlags::empty(),
        }
    }

    pub fn key(&self) -> (FactionId, u32) {
        (self.owner_faction, self.discovery_id)
    }

    pub fn register_countermeasure(&mut self, countermeasure: KnowledgeCountermeasure) {
        self.countermeasures.push(countermeasure);
    }

    pub fn register_infiltration(&mut self, mut probe: InfiltrationRecord, max_suspicion: Scalar) {
        if let Some(existing) = self
            .infiltrations
            .iter_mut()
            .find(|inf| inf.faction == probe.faction)
        {
            existing.blueprint_fidelity = (existing.blueprint_fidelity + probe.blueprint_fidelity)
                .clamp(Scalar::zero(), Scalar::one());
            existing.suspicion =
                (existing.suspicion + probe.suspicion).clamp(Scalar::zero(), max_suspicion);
            existing.cells = existing.cells.saturating_add(probe.cells);
            existing.last_activity_tick = probe.last_activity_tick;
        } else {
            probe.blueprint_fidelity = probe
                .blueprint_fidelity
                .clamp(Scalar::zero(), Scalar::one());
            probe.suspicion = probe.suspicion.clamp(Scalar::zero(), max_suspicion);
            self.infiltrations.push(probe);
        }
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeTimelineEvent {
    pub tick: u64,
    pub kind: KnowledgeTimelineEventKind,
    pub source_faction: Option<FactionId>,
    pub delta_percent: Option<i16>,
    pub note: Option<String>,
}

#[derive(Resource, Debug)]
pub struct KnowledgeLedger {
    entries: HashMap<(FactionId, u32), KnowledgeLedgerEntry>,
    timeline: VecDeque<KnowledgeTimelineEvent>,
    max_timeline_events: usize,
    last_emitted_tick: Option<u64>,
    last_emitted_metrics: KnowledgeMetricsState,
    config: Arc<KnowledgeLedgerConfig>,
}

impl Default for KnowledgeLedger {
    fn default() -> Self {
        let config = Arc::new(
            KnowledgeLedgerConfig::from_json_str(BUILTIN_KNOWLEDGE_LEDGER_CONFIG)
                .expect("knowledge ledger config should parse"),
        );
        Self::with_config(config)
    }
}

impl KnowledgeLedger {
    pub fn with_config(config: Arc<KnowledgeLedgerConfig>) -> Self {
        Self {
            entries: HashMap::new(),
            timeline: VecDeque::new(),
            max_timeline_events: config.timeline_capacity(),
            last_emitted_tick: None,
            last_emitted_metrics: KnowledgeMetricsState::default(),
            config,
        }
    }

    pub fn config(&self) -> Arc<KnowledgeLedgerConfig> {
        Arc::clone(&self.config)
    }

    pub fn apply_config(&mut self, config: Arc<KnowledgeLedgerConfig>) {
        self.config = config;
        self.max_timeline_events = self.config.timeline_capacity();
        if self.timeline.len() > self.max_timeline_events {
            self.timeline.truncate(self.max_timeline_events);
        }
    }
}

#[derive(Event, Debug, Clone)]
pub struct EspionageProbeEvent {
    pub owner: FactionId,
    pub discovery_id: u32,
    pub infiltrator: FactionId,
    pub fidelity_gain: Scalar,
    pub suspicion_gain: Scalar,
    pub cells: u8,
    pub tick: u64,
    pub note: Option<String>,
}

#[derive(Event, Debug, Clone)]
pub struct CounterIntelSweepEvent {
    pub owner: FactionId,
    pub discovery_id: u32,
    pub countermeasure: KnowledgeCountermeasure,
    pub tick: u64,
    pub note: Option<String>,
    pub cleared_faction: Option<FactionId>,
    pub suspicion_relief: Scalar,
}

impl KnowledgeLedger {
    pub fn upsert_entry(&mut self, entry: KnowledgeLedgerEntry) {
        self.entries.insert(entry.key(), entry);
    }

    fn ensure_entry(&mut self, owner: FactionId, discovery_id: u32) -> &mut KnowledgeLedgerEntry {
        let key = (owner, discovery_id);
        self.entries
            .entry(key)
            .or_insert_with(|| KnowledgeLedgerEntry::new(owner, discovery_id, self.config.as_ref()))
    }

    pub fn apply_countermeasure(
        &mut self,
        owner: FactionId,
        discovery_id: u32,
        countermeasure: KnowledgeCountermeasure,
        tick: u64,
        note: Option<String>,
    ) -> bool {
        let entry = self.ensure_entry(owner, discovery_id);
        entry.register_countermeasure(countermeasure.clone());
        self.push_timeline_event(KnowledgeTimelineEvent {
            tick,
            kind: KnowledgeTimelineEventKind::CounterIntel,
            source_faction: Some(owner),
            delta_percent: None,
            note: Some(note.unwrap_or_else(|| "Countermeasure deployed".into())),
        });
        true
    }

    pub fn apply_counterintel_sweep(&mut self, event: CounterIntelSweepEvent) {
        let CounterIntelSweepEvent {
            owner,
            discovery_id,
            countermeasure,
            tick,
            note,
            cleared_faction,
            suspicion_relief,
        } = event;

        self.apply_countermeasure(owner, discovery_id, countermeasure, tick, note);

        if cleared_faction.is_some() || suspicion_relief > Scalar::zero() {
            self.apply_infiltration_relief(owner, discovery_id, cleared_faction, suspicion_relief);
        }
    }

    fn apply_infiltration_relief(
        &mut self,
        owner: FactionId,
        discovery_id: u32,
        cleared_faction: Option<FactionId>,
        suspicion_relief: Scalar,
    ) {
        if let Some(entry) = self.entries.get_mut(&(owner, discovery_id)) {
            if let Some(target) = cleared_faction {
                entry.infiltrations.retain(|inf| inf.faction != target);
            }

            if suspicion_relief > Scalar::zero() {
                for infiltration in &mut entry.infiltrations {
                    if infiltration.suspicion > suspicion_relief {
                        infiltration.suspicion -= suspicion_relief;
                    } else {
                        infiltration.suspicion = Scalar::zero();
                    }
                }
            }
        }
    }

    pub fn record_espionage_probe(&mut self, probe: EspionageProbeEvent) -> bool {
        let EspionageProbeEvent {
            owner: probe_owner,
            discovery_id: probe_discovery,
            infiltrator,
            fidelity_gain,
            suspicion_gain,
            cells,
            tick,
            note,
        } = probe;

        let owner = probe_owner;
        let discovery_id = probe_discovery;

        let max_suspicion = self.config.max_suspicion();
        let entry = self.ensure_entry(owner, discovery_id);
        let probe_record = InfiltrationRecord {
            faction: infiltrator,
            blueprint_fidelity: fidelity_gain,
            suspicion: suspicion_gain,
            cells,
            last_activity_tick: tick,
        };
        entry.register_infiltration(probe_record, max_suspicion);
        self.push_timeline_event(KnowledgeTimelineEvent {
            tick,
            kind: KnowledgeTimelineEventKind::SpyProbe,
            source_faction: Some(infiltrator),
            delta_percent: None,
            note: Some(note.unwrap_or_else(|| format!("Probe on discovery {}", discovery_id))),
        });
        true
    }

    pub fn remove_entry(
        &mut self,
        owner: FactionId,
        discovery_id: u32,
    ) -> Option<KnowledgeLedgerEntry> {
        self.entries.remove(&(owner, discovery_id))
    }

    pub fn entries(&self) -> impl Iterator<Item = &KnowledgeLedgerEntry> {
        self.entries.values()
    }

    pub fn entries_for_faction(
        &self,
        faction: FactionId,
    ) -> impl Iterator<Item = &KnowledgeLedgerEntry> {
        self.entries
            .values()
            .filter(move |entry| entry.owner_faction == faction)
    }

    pub fn entry(&self, faction: FactionId, discovery_id: u32) -> Option<&KnowledgeLedgerEntry> {
        self.entries.get(&(faction, discovery_id))
    }

    pub fn push_timeline_event(&mut self, event: KnowledgeTimelineEvent) {
        if self.timeline.len() >= self.max_timeline_events {
            self.timeline.pop_front();
        }
        self.timeline.push_back(event);
    }

    pub fn metrics(&self) -> KnowledgeMetricsState {
        let mut warnings = 0u32;
        let mut criticals = 0u32;
        let mut countermeasures_active = 0u32;
        let mut common_knowledge = 0u32;

        for entry in self.entries.values() {
            if !entry.countermeasures.is_empty() {
                countermeasures_active += 1;
            }
            if entry.flags.contains(KnowledgeLeakFlags::COMMON_KNOWLEDGE) {
                common_knowledge += 1;
            }
            if entry.flags.contains(KnowledgeLeakFlags::CASCADE_PENDING)
                || entry.progress_percent >= 90
            {
                criticals += 1;
            } else if entry.progress_percent >= 70 {
                warnings += 1;
            }
        }

        KnowledgeMetricsState {
            leak_warnings: warnings,
            leak_criticals: criticals,
            countermeasures_active,
            common_knowledge_total: common_knowledge,
        }
    }

    pub fn snapshot_payload(&self) -> KnowledgeSnapshotPayload {
        let mut ledger_states: Vec<_> = self.entries.values().map(to_contract_entry).collect();
        ledger_states.sort_by_key(|state| (state.owner_faction, state.discovery_id));

        let mut timeline_states: Vec<_> = self.timeline.iter().map(to_contract_timeline).collect();
        timeline_states.sort_by_key(|state| (state.tick, state.kind as u8));

        let metrics = self.metrics();

        KnowledgeSnapshotPayload {
            entries: ledger_states,
            timeline: timeline_states,
            metrics,
        }
    }

    fn telemetry_events(&self) -> Vec<KnowledgeTelemetryEvent> {
        self.timeline.iter().map(to_telemetry_event).collect()
    }

    fn should_emit(&self, tick: u64, metrics: &KnowledgeMetricsState) -> bool {
        let metrics_changed = metrics != &self.last_emitted_metrics;
        let tick_changed = self.last_emitted_tick != Some(tick);
        metrics_changed || tick_changed
    }

    fn mark_emitted(&mut self, tick: u64, metrics: KnowledgeMetricsState) {
        self.last_emitted_tick = Some(tick);
        self.last_emitted_metrics = metrics;
    }
}

pub struct KnowledgeSnapshotPayload {
    pub entries: Vec<KnowledgeLedgerEntryState>,
    pub timeline: Vec<KnowledgeTimelineEventState>,
    pub metrics: KnowledgeMetricsState,
}

fn to_contract_entry(entry: &KnowledgeLedgerEntry) -> KnowledgeLedgerEntryState {
    KnowledgeLedgerEntryState {
        discovery_id: entry.discovery_id,
        owner_faction: entry.owner_faction.0,
        tier: entry.tier,
        progress_percent: entry.progress_percent,
        half_life_ticks: entry.half_life_ticks,
        time_to_cascade: entry.time_to_cascade,
        security_posture: entry.security_posture,
        countermeasures: entry
            .countermeasures
            .iter()
            .map(|cm| KnowledgeCountermeasureState {
                kind: cm.kind,
                potency: cm.potency.raw(),
                upkeep: cm.upkeep.raw(),
                remaining_ticks: cm.remaining_ticks,
            })
            .collect(),
        infiltrations: entry
            .infiltrations
            .iter()
            .map(|infiltration| KnowledgeInfiltrationState {
                faction: infiltration.faction.0,
                blueprint_fidelity: infiltration.blueprint_fidelity.raw(),
                suspicion: infiltration.suspicion.raw(),
                cells: infiltration.cells,
                last_activity_tick: infiltration.last_activity_tick,
            })
            .collect(),
        modifiers: entry
            .modifiers
            .iter()
            .map(|modifier| KnowledgeModifierBreakdownState {
                source: modifier.source,
                delta_half_life: modifier.delta_half_life,
                delta_progress: modifier.delta_progress,
                note_handle: modifier.note.clone(),
            })
            .collect(),
        flags: entry.flags,
    }
}

fn to_contract_timeline(event: &KnowledgeTimelineEvent) -> KnowledgeTimelineEventState {
    KnowledgeTimelineEventState {
        tick: event.tick,
        kind: event.kind,
        source_faction: event.source_faction.map(|f| f.0).unwrap_or(u32::MAX),
        delta_percent: event.delta_percent.unwrap_or_default(),
        note_handle: event.note.clone(),
    }
}

fn to_telemetry_event(event: &KnowledgeTimelineEvent) -> KnowledgeTelemetryEvent {
    KnowledgeTelemetryEvent {
        tick: Some(event.tick),
        kind: event.kind,
        source_faction: event.source_faction.map(|f| f.0),
        delta_percent: event.delta_percent,
        note: event.note.clone(),
    }
}

fn mission_telemetry(catalog: &EspionageCatalog) -> Vec<KnowledgeTelemetryMission> {
    catalog.missions().map(to_telemetry_mission).collect()
}

fn to_telemetry_mission(template: &EspionageMissionTemplate) -> KnowledgeTelemetryMission {
    KnowledgeTelemetryMission {
        id: template.id.0.clone(),
        name: template.name.clone(),
        generated: template.generated,
        kind: mission_kind_label(template.kind),
        resolution_ticks: template.resolution_ticks,
        base_success: template.base_success.to_f32(),
        success_threshold: template.success_threshold.to_f32(),
        fidelity_gain: template.fidelity_gain.to_f32(),
        suspicion_on_success: template.suspicion_on_success.to_f32(),
        suspicion_on_failure: template.suspicion_on_failure.to_f32(),
        cell_gain_on_success: template.cell_gain_on_success,
        suspicion_relief: template.suspicion_relief.to_f32(),
        fidelity_suppression: template.fidelity_suppression.to_f32(),
        note: template.note.clone(),
    }
}

fn mission_kind_label(kind: EspionageMissionKind) -> String {
    match kind {
        EspionageMissionKind::Probe => "probe",
        EspionageMissionKind::CounterIntel => "counter_intel",
    }
    .to_string()
}

impl From<&KnowledgeCountermeasureState> for KnowledgeCountermeasure {
    fn from(state: &KnowledgeCountermeasureState) -> Self {
        Self {
            kind: state.kind,
            potency: Scalar::from_raw(state.potency),
            upkeep: Scalar::from_raw(state.upkeep),
            remaining_ticks: state.remaining_ticks,
        }
    }
}

impl From<&KnowledgeInfiltrationState> for InfiltrationRecord {
    fn from(state: &KnowledgeInfiltrationState) -> Self {
        Self {
            faction: FactionId(state.faction),
            blueprint_fidelity: Scalar::from_raw(state.blueprint_fidelity),
            suspicion: Scalar::from_raw(state.suspicion),
            cells: state.cells,
            last_activity_tick: state.last_activity_tick,
        }
    }
}

impl From<&KnowledgeModifierBreakdownState> for KnowledgeModifier {
    fn from(state: &KnowledgeModifierBreakdownState) -> Self {
        Self {
            source: state.source,
            delta_half_life: state.delta_half_life,
            delta_progress: state.delta_progress,
            note: state.note_handle.clone(),
        }
    }
}

impl From<&KnowledgeLedgerEntryState> for KnowledgeLedgerEntry {
    fn from(state: &KnowledgeLedgerEntryState) -> Self {
        Self {
            discovery_id: state.discovery_id,
            owner_faction: FactionId(state.owner_faction),
            tier: state.tier,
            progress_percent: state.progress_percent,
            half_life_ticks: state.half_life_ticks,
            time_to_cascade: state.time_to_cascade,
            security_posture: state.security_posture,
            countermeasures: state
                .countermeasures
                .iter()
                .map(KnowledgeCountermeasure::from)
                .collect(),
            infiltrations: state
                .infiltrations
                .iter()
                .map(InfiltrationRecord::from)
                .collect(),
            modifiers: state
                .modifiers
                .iter()
                .map(KnowledgeModifier::from)
                .collect(),
            flags: state.flags,
        }
    }
}

impl From<&KnowledgeTimelineEventState> for KnowledgeTimelineEvent {
    fn from(state: &KnowledgeTimelineEventState) -> Self {
        let source_faction = if state.source_faction == u32::MAX {
            None
        } else {
            Some(FactionId(state.source_faction))
        };
        Self {
            tick: state.tick,
            kind: state.kind,
            source_faction,
            delta_percent: Some(state.delta_percent),
            note: state.note_handle.clone(),
        }
    }
}

impl KnowledgeLedger {
    pub fn sync_from_snapshot(&mut self, snapshot: &WorldSnapshot) {
        self.entries.clear();
        for state in &snapshot.knowledge_ledger {
            let entry = KnowledgeLedgerEntry::from(state);
            self.entries.insert(entry.key(), entry);
        }

        self.timeline.clear();
        for state in &snapshot.knowledge_timeline {
            self.timeline.push_back(KnowledgeTimelineEvent::from(state));
        }

        self.last_emitted_metrics = snapshot.knowledge_metrics.clone();
        self.last_emitted_tick = None;
    }
}

pub fn knowledge_ledger_tick(
    tick: Res<SimulationTick>,
    mut ledger: ResMut<KnowledgeLedger>,
    mut metrics: ResMut<SimulationMetrics>,
    catalog: Res<EspionageCatalog>,
) {
    let config = ledger.config();
    let mut pending_events: Vec<KnowledgeTimelineEvent> = Vec::new();
    let mut expired_infiltrations: Vec<(FactionId, u32, FactionId)> = Vec::new();

    for entry in ledger.entries.values_mut() {
        let cfg = config.as_ref();
        let base_half_life = entry.half_life_ticks.max(2) as i32;
        let modifier_half_life: i32 = entry
            .modifiers
            .iter()
            .map(|modifier| modifier.delta_half_life as i32)
            .sum();
        let countermeasure_bonus_ticks: i32 = entry
            .countermeasures
            .iter()
            .map(|cm| (cm.potency.to_f32() * cfg.countermeasure_bonus_scale()).round() as i32)
            .sum();
        let infiltration_penalty_ticks: i32 = entry
            .infiltrations
            .iter()
            .map(|inf| {
                let cells_penalty =
                    (inf.cells as f32 * cfg.infiltration_cells_weight()).round() as i32;
                let fidelity_penalty = (inf.blueprint_fidelity.to_f32()
                    * cfg.infiltration_fidelity_weight())
                .round() as i32;
                cells_penalty + fidelity_penalty
            })
            .sum();

        let mut effective_half_life =
            base_half_life + modifier_half_life + countermeasure_bonus_ticks
                - infiltration_penalty_ticks;
        if effective_half_life < 2 {
            effective_half_life = 2;
        }

        let mut progress_delta = (100.0 / effective_half_life as f32).ceil() as i32;
        let modifier_progress: i32 = entry
            .modifiers
            .iter()
            .map(|modifier| modifier.delta_progress as i32)
            .sum();
        progress_delta += modifier_progress;
        progress_delta += infiltration_penalty_ticks.max(0);
        let counter_penalty = (countermeasure_bonus_ticks as f32
            * cfg.countermeasure_progress_penalty_ratio())
        .round() as i32;
        progress_delta -= counter_penalty.max(0);
        progress_delta = progress_delta.clamp(0, cfg.max_progress_per_tick());

        let mut emitted_progress_event = false;
        if progress_delta > 0 {
            let previous_progress = entry.progress_percent;
            let new_progress = (previous_progress as i32 + progress_delta).min(100) as u16;
            entry.progress_percent = new_progress;

            if new_progress >= 100 && !entry.flags.contains(KnowledgeLeakFlags::COMMON_KNOWLEDGE) {
                entry.flags.insert(KnowledgeLeakFlags::COMMON_KNOWLEDGE);
                entry.flags.remove(KnowledgeLeakFlags::CASCADE_PENDING);
                entry.time_to_cascade = 0;
                pending_events.push(KnowledgeTimelineEvent {
                    tick: tick.0,
                    kind: KnowledgeTimelineEventKind::Cascade,
                    source_faction: Some(entry.owner_faction),
                    delta_percent: Some(progress_delta as i16),
                    note: Some("Knowledge cascade reached 100%".into()),
                });
            } else {
                entry.time_to_cascade = if progress_delta > 0 {
                    let remaining = (100 - new_progress as i32).max(0);
                    ((remaining + progress_delta - 1) / progress_delta) as u16
                } else {
                    entry.time_to_cascade
                };
                if new_progress >= 90 {
                    entry.flags.insert(KnowledgeLeakFlags::CASCADE_PENDING);
                } else {
                    entry.flags.remove(KnowledgeLeakFlags::CASCADE_PENDING);
                }

                pending_events.push(KnowledgeTimelineEvent {
                    tick: tick.0,
                    kind: KnowledgeTimelineEventKind::LeakProgress,
                    source_faction: Some(entry.owner_faction),
                    delta_percent: Some((new_progress - previous_progress) as i16),
                    note: Some(format!(
                        "Half-life {}→{}",
                        base_half_life, effective_half_life
                    )),
                });
                emitted_progress_event = true;
            }
        }

        let mut countermeasures_expired = false;
        entry.countermeasures.retain_mut(|cm| {
            if cm.remaining_ticks > 0 {
                cm.remaining_ticks -= 1;
            }
            if cm.remaining_ticks == 0 {
                countermeasures_expired = true;
                false
            } else {
                true
            }
        });
        if countermeasures_expired {
            pending_events.push(KnowledgeTimelineEvent {
                tick: tick.0,
                kind: KnowledgeTimelineEventKind::CounterIntel,
                source_faction: Some(entry.owner_faction),
                delta_percent: None,
                note: Some("Countermeasure expired".into()),
            });
        }

        let suspicion_decay = cfg.suspicion_decay();
        entry.infiltrations.retain_mut(|inf| {
            inf.suspicion = if inf.suspicion > suspicion_decay {
                inf.suspicion - suspicion_decay
            } else {
                Scalar::zero()
            };
            let threshold = cfg.suspicion_retention_threshold();
            let should_keep = inf.suspicion > threshold || inf.blueprint_fidelity > threshold;
            if !should_keep {
                expired_infiltrations.push((entry.owner_faction, entry.discovery_id, inf.faction));
            }
            should_keep
        });

        if !emitted_progress_event && progress_delta == 0 {
            entry.time_to_cascade = entry.time_to_cascade.saturating_add(1);
        }
    }

    for event in pending_events {
        ledger.push_timeline_event(event);
    }

    for (owner, _discovery, infiltrator) in expired_infiltrations {
        ledger.push_timeline_event(KnowledgeTimelineEvent {
            tick: tick.0,
            kind: KnowledgeTimelineEventKind::CounterIntel,
            source_faction: Some(owner),
            delta_percent: None,
            note: Some(format!(
                "Infiltration cell from faction {} dismantled",
                infiltrator.0
            )),
        });
    }

    let summary = ledger.metrics();
    metrics.knowledge_leak_warnings = summary.leak_warnings;
    metrics.knowledge_leak_criticals = summary.leak_criticals;
    metrics.knowledge_countermeasures_active = summary.countermeasures_active;
    metrics.knowledge_common_knowledge_total = summary.common_knowledge_total;

    if !ledger.should_emit(tick.0, &summary) {
        return;
    }

    let events = ledger.telemetry_events();
    let missions = mission_telemetry(&catalog);
    if events.is_empty()
        && missions.is_empty()
        && summary.leak_warnings == 0
        && summary.leak_criticals == 0
        && summary.countermeasures_active == 0
        && summary.common_knowledge_total == 0
    {
        ledger.mark_emitted(tick.0, summary);
        return;
    }

    let frame = KnowledgeTelemetryFrame {
        tick: tick.0,
        leak_warnings: summary.leak_warnings,
        leak_criticals: summary.leak_criticals,
        countermeasures_active: summary.countermeasures_active,
        common_knowledge_total: summary.common_knowledge_total,
        events,
        missions,
    };

    if let Ok(payload) = serde_json::to_string(&frame) {
        debug!(target: KNOWLEDGE_TELEMETRY_TOPIC, "{} {}", KNOWLEDGE_TELEMETRY_TOPIC, payload);
    }

    ledger.mark_emitted(tick.0, summary);
}

pub fn process_espionage_events(
    mut ledger: ResMut<KnowledgeLedger>,
    mut probe_events: EventReader<EspionageProbeEvent>,
    mut counter_events: EventReader<CounterIntelSweepEvent>,
) {
    for event in probe_events.read() {
        let _ = ledger.record_espionage_probe(event.clone());
    }

    for event in counter_events.read() {
        ledger.apply_counterintel_sweep(event.clone());
    }
}

pub fn encode_ledger_key(owner: FactionId, discovery_id: u32) -> u64 {
    encode_knowledge_ledger_key(owner.0, discovery_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_probe_creates_entry() {
        let mut ledger = KnowledgeLedger::default();
        let owner = FactionId(7);
        let discovery = 42;
        assert!(ledger.entries.is_empty());

        ledger.record_espionage_probe(EspionageProbeEvent {
            owner,
            discovery_id: discovery,
            infiltrator: FactionId(9),
            fidelity_gain: Scalar::from_f32(0.25),
            suspicion_gain: Scalar::from_f32(0.4),
            cells: 2,
            tick: 10,
            note: Some("probe".into()),
        });

        let entry = ledger.entry(owner, discovery).expect("entry exists");
        assert_eq!(entry.infiltrations.len(), 1);
        assert_eq!(entry.infiltrations[0].cells, 2);
    }

    #[test]
    fn apply_countermeasure_tracks_active_countermeasures() {
        let mut ledger = KnowledgeLedger::default();
        let owner = FactionId(3);
        let discovery = 11;

        ledger.apply_countermeasure(
            owner,
            discovery,
            KnowledgeCountermeasure {
                kind: KnowledgeCountermeasureKind::SecurityInvestment,
                potency: Scalar::from_f32(0.5),
                upkeep: Scalar::from_f32(0.1),
                remaining_ticks: 3,
            },
            5,
            None,
        );

        let entry = ledger.entry(owner, discovery).expect("entry exists");
        assert_eq!(entry.countermeasures.len(), 1);
        assert_eq!(entry.countermeasures[0].remaining_ticks, 3);
    }
}
