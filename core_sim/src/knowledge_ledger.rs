use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use log::debug;
use sim_runtime::knowledge::{
    KnowledgeTelemetryEvent, KnowledgeTelemetryFrame, KNOWLEDGE_TELEMETRY_TOPIC,
};
use sim_runtime::{
    encode_knowledge_ledger_key, KnowledgeCountermeasureKind, KnowledgeCountermeasureState,
    KnowledgeInfiltrationState, KnowledgeLeakFlags, KnowledgeLedgerEntryState,
    KnowledgeMetricsState, KnowledgeModifierBreakdownState, KnowledgeModifierSource,
    KnowledgeSecurityPosture, KnowledgeTimelineEventKind, KnowledgeTimelineEventState,
    WorldSnapshot,
};

use crate::{
    metrics::SimulationMetrics, orders::FactionId, resources::SimulationTick, scalar::Scalar,
};

const DEFAULT_TIMELINE_CAPACITY: usize = 64;

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
    pub fn key(&self) -> (FactionId, u32) {
        (self.owner_faction, self.discovery_id)
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
}

impl Default for KnowledgeLedger {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            timeline: VecDeque::new(),
            max_timeline_events: DEFAULT_TIMELINE_CAPACITY,
            last_emitted_tick: None,
            last_emitted_metrics: KnowledgeMetricsState::default(),
        }
    }
}

impl KnowledgeLedger {
    pub fn upsert_entry(&mut self, entry: KnowledgeLedgerEntry) {
        self.entries.insert(entry.key(), entry);
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

    fn telemetry_events_for_tick(&self, tick: u64) -> Vec<KnowledgeTelemetryEvent> {
        self.timeline
            .iter()
            .filter(|event| event.tick == tick)
            .map(to_telemetry_event)
            .collect()
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
) {
    let mut pending_events: Vec<KnowledgeTimelineEvent> = Vec::new();

    for entry in ledger.entries.values_mut() {
        let base_half_life = entry.half_life_ticks.max(2) as i32;
        let modifier_half_life: i32 = entry
            .modifiers
            .iter()
            .map(|modifier| modifier.delta_half_life as i32)
            .sum();
        let countermeasure_bonus_ticks: i32 = entry
            .countermeasures
            .iter()
            .map(|cm| (cm.potency.to_f32() * 4.0).round() as i32)
            .sum();
        let infiltration_penalty_ticks: i32 = entry
            .infiltrations
            .iter()
            .map(|inf| inf.cells as i32 + (inf.blueprint_fidelity.to_f32() * 2.0).round() as i32)
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
        progress_delta -= (countermeasure_bonus_ticks / 2).max(0);
        progress_delta = progress_delta.clamp(0, 25);

        let mut emitted_progress_event = false;
        if progress_delta > 0 {
            let previous_progress = entry.progress_percent;
            let new_progress = (previous_progress as i32 + progress_delta).min(100) as u16;
            entry.progress_percent = new_progress;

            if new_progress >= 100 {
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

        if !emitted_progress_event && progress_delta == 0 {
            entry.time_to_cascade = entry.time_to_cascade.saturating_add(1);
        }
    }

    for event in pending_events {
        ledger.push_timeline_event(event);
    }

    let summary = ledger.metrics();
    metrics.knowledge_leak_warnings = summary.leak_warnings;
    metrics.knowledge_leak_criticals = summary.leak_criticals;
    metrics.knowledge_countermeasures_active = summary.countermeasures_active;
    metrics.knowledge_common_knowledge_total = summary.common_knowledge_total;

    if !ledger.should_emit(tick.0, &summary) {
        return;
    }

    let events = ledger.telemetry_events_for_tick(tick.0);
    if events.is_empty()
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
    };

    if let Ok(payload) = serde_json::to_string(&frame) {
        debug!(target: KNOWLEDGE_TELEMETRY_TOPIC, "{} {}", KNOWLEDGE_TELEMETRY_TOPIC, payload);
    }

    ledger.mark_emitted(tick.0, summary);
}

pub fn encode_ledger_key(owner: FactionId, discovery_id: u32) -> u64 {
    encode_knowledge_ledger_key(owner.0, discovery_id)
}
