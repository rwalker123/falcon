//! Shared runtime utilities for Shadow-Scale.
//!
//! This crate re-exports the data contracts from `sim_schema` and hosts helper
//! routines that operate on those contracts (validation, transforms, command
//! utilities) without depending on the full Bevy runtime in `core_sim`.

use std::cmp::{max, min};

pub use sim_schema::*;

pub mod commands;
pub use commands::{
    CommandDecodeError, CommandEncodeError, CommandEnvelope, CommandPayload, OrdersDirective,
    SupportChannel,
};

pub mod command_text;
pub use command_text::{parse_command_line, CommandParseError};

pub mod scripting;
pub use scripting::{
    capability_registry, manifest_schema, CapabilityRegistry, CapabilitySpec,
    ManifestValidationError, ScriptManifest, ScriptManifestRef, SessionAccess, SimScriptState,
};

/// Fixed-point scaling constant shared with `core_sim::Scalar`.
pub const FIXED_POINT_SCALE: i64 = 1_000_000;

/// Clamp a fixed-point value between two bounds.
pub fn clamp_fixed(value: i64, min_value: i64, max_value: i64) -> i64 {
    if value < min_value {
        min_value
    } else if value > max_value {
        max_value
    } else {
        value
    }
}

/// Multiply two fixed-point values (scaled by [`FIXED_POINT_SCALE`]).
pub fn fixed_mul(lhs: i64, rhs: i64) -> i64 {
    ((lhs as i128 * rhs as i128) / FIXED_POINT_SCALE as i128) as i64
}

/// Represents the tuning curve that maps openness to leak timers in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeLeakCurve {
    pub min_ticks: u32,
    pub max_ticks: u32,
    pub exponent: f32,
}

impl TradeLeakCurve {
    pub const fn new(min_ticks: u32, max_ticks: u32, exponent: f32) -> Self {
        Self {
            min_ticks,
            max_ticks,
            exponent,
        }
    }

    /// Resolve a leak timer (in ticks) based on the provided openness value.
    pub fn ticks_for_openness(&self, openness_raw: i64) -> u32 {
        let min_ticks = min(self.min_ticks, self.max_ticks);
        let max_ticks = max(self.min_ticks, self.max_ticks);
        if max_ticks == 0 {
            return 0;
        }

        let openness = (openness_raw as f64 / FIXED_POINT_SCALE as f64).clamp(0.0, 1.0);
        let exponent = if self.exponent <= 0.0 {
            1.0
        } else {
            self.exponent as f64
        };
        let blend = openness.powf(exponent);
        let ticks = (max_ticks as f64 * (1.0 - blend)) + (min_ticks as f64 * blend);
        ticks.round().clamp(min_ticks as f64, max_ticks as f64) as u32
    }
}

/// Apply per-tick openness decay, ensuring the result remains within [0, 1].
pub fn apply_openness_decay(openness_raw: i64, decay_raw: i64) -> i64 {
    let openness = clamp_fixed(openness_raw, 0, FIXED_POINT_SCALE);
    let decay = clamp_fixed(decay_raw, 0, FIXED_POINT_SCALE);
    clamp_fixed(openness - decay, 0, FIXED_POINT_SCALE)
}

/// Scale a set of known technology fragments for migration payload synthesis.
pub fn scale_migration_fragments(
    source: &[KnownTechFragment],
    scaling_raw: i64,
    fidelity_floor_raw: i64,
) -> Vec<KnownTechFragment> {
    if source.is_empty() {
        return Vec::new();
    }

    let scaling = clamp_fixed(scaling_raw, 0, FIXED_POINT_SCALE);
    if scaling == 0 {
        return Vec::new();
    }
    let fidelity_floor = clamp_fixed(fidelity_floor_raw, 0, FIXED_POINT_SCALE);

    let mut payload: Vec<KnownTechFragment> = source
        .iter()
        .filter_map(|fragment| {
            if fragment.progress <= 0 {
                return None;
            }
            let scaled_progress =
                clamp_fixed(fixed_mul(fragment.progress, scaling), 0, FIXED_POINT_SCALE);
            if scaled_progress == 0 {
                return None;
            }
            let base_fidelity = if fragment.fidelity > 0 {
                fragment.fidelity
            } else {
                FIXED_POINT_SCALE
            };
            let mut fidelity = fixed_mul(base_fidelity, scaling);
            fidelity = clamp_fixed(fidelity, fidelity_floor, FIXED_POINT_SCALE);
            Some(KnownTechFragment {
                discovery_id: fragment.discovery_id,
                progress: scaled_progress,
                fidelity,
            })
        })
        .collect();

    payload.sort_by_key(|fragment| fragment.discovery_id);
    payload
}

/// Merge migration payload fragments into an existing fragment list.
pub fn merge_fragment_payload(
    destination: &mut Vec<KnownTechFragment>,
    payload: &[KnownTechFragment],
    cap_raw: i64,
) {
    if payload.is_empty() {
        return;
    }

    let cap = clamp_fixed(cap_raw, 0, FIXED_POINT_SCALE);
    for fragment in payload {
        if fragment.progress <= 0 {
            continue;
        }
        if let Some(existing) = destination
            .iter_mut()
            .find(|entry| entry.discovery_id == fragment.discovery_id)
        {
            existing.progress = clamp_fixed(existing.progress + fragment.progress, 0, cap);
            existing.fidelity = max(existing.fidelity, fragment.fidelity);
        } else {
            let mut clone = fragment.clone();
            clone.progress = clamp_fixed(clone.progress, 0, cap);
            destination.push(clone);
        }
    }

    destination.sort_by_key(|fragment| fragment.discovery_id);
}

pub mod knowledge {
    use crate::{
        KnowledgeLedgerEntryState, KnowledgeMetricsState, KnowledgeTimelineEventKind,
        KnowledgeTimelineEventState, WorldDelta, WorldSnapshot,
    };
    use serde::{Deserialize, Serialize};

    /// Log topic prefix for knowledge telemetry frames emitted by the simulation.
    pub const KNOWLEDGE_TELEMETRY_TOPIC: &str = "knowledge.telemetry";

    /// Serialised form of a knowledge telemetry frame (`knowledge.telemetry {json}`).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    pub struct KnowledgeTelemetryFrame {
        pub tick: u64,
        #[serde(default)]
        pub leak_warnings: u32,
        #[serde(default)]
        pub leak_criticals: u32,
        #[serde(default)]
        pub countermeasures_active: u32,
        #[serde(default)]
        pub common_knowledge_total: u32,
        #[serde(default)]
        pub events: Vec<KnowledgeTelemetryEvent>,
        #[serde(default)]
        pub missions: Vec<KnowledgeTelemetryMission>,
    }

    /// Describes a single knowledge telemetry event (usually mirrored from the timeline payload).
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
    pub struct KnowledgeTelemetryEvent {
        #[serde(default)]
        pub tick: Option<u64>,
        pub kind: KnowledgeTimelineEventKind,
        #[serde(default)]
        pub source_faction: Option<u32>,
        #[serde(default)]
        pub delta_percent: Option<i16>,
        #[serde(default)]
        pub note: Option<String>,
    }

    /// Describes a mission template exposed alongside knowledge telemetry frames.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    pub struct KnowledgeTelemetryMission {
        pub id: String,
        pub name: String,
        #[serde(default)]
        pub generated: bool,
        #[serde(default)]
        pub kind: String,
        pub resolution_ticks: u16,
        pub base_success: f32,
        pub success_threshold: f32,
        pub fidelity_gain: f32,
        pub suspicion_on_success: f32,
        pub suspicion_on_failure: f32,
        pub cell_gain_on_success: u8,
        #[serde(default)]
        pub suspicion_relief: f32,
        #[serde(default)]
        pub fidelity_suppression: f32,
        #[serde(default)]
        pub note: Option<String>,
    }

    /// Parse a knowledge telemetry JSON payload (after stripping the `knowledge.telemetry ` prefix).
    pub fn parse_knowledge_telemetry(payload: &str) -> serde_json::Result<KnowledgeTelemetryFrame> {
        serde_json::from_str(payload)
    }

    /// Borrowed view over the knowledge ledger payload embedded in a [`WorldSnapshot`].
    #[derive(Debug, Clone, Copy)]
    pub struct KnowledgeLedgerView<'a> {
        entries: &'a [KnowledgeLedgerEntryState],
        timeline: &'a [KnowledgeTimelineEventState],
        metrics: &'a KnowledgeMetricsState,
    }

    impl<'a> KnowledgeLedgerView<'a> {
        /// Construct a new view from a snapshot reference.
        pub fn from_snapshot(snapshot: &'a WorldSnapshot) -> Self {
            Self {
                entries: &snapshot.knowledge_ledger,
                timeline: &snapshot.knowledge_timeline,
                metrics: &snapshot.knowledge_metrics,
            }
        }

        /// Returns every ledger entry contained in the snapshot.
        pub fn entries(&self) -> &'a [KnowledgeLedgerEntryState] {
            self.entries
        }

        /// Returns an iterator over entries owned by the given faction.
        pub fn entries_for_faction(
            &self,
            owner_faction: u32,
        ) -> impl Iterator<Item = &'a KnowledgeLedgerEntryState> {
            self.entries
                .iter()
                .filter(move |entry| entry.owner_faction == owner_faction)
        }

        /// Look up a specific entry by owner/discovery identifiers.
        pub fn entry(
            &self,
            owner_faction: u32,
            discovery_id: u32,
        ) -> Option<&'a KnowledgeLedgerEntryState> {
            self.entries.iter().find(|entry| {
                entry.owner_faction == owner_faction && entry.discovery_id == discovery_id
            })
        }

        /// Returns the metrics summary that was serialised alongside the ledger.
        pub fn metrics(&self) -> &'a KnowledgeMetricsState {
            self.metrics
        }

        /// Returns the most recent knowledge timeline events included in the snapshot.
        pub fn timeline(&self) -> &'a [KnowledgeTimelineEventState] {
            self.timeline
        }

        pub fn leak_warnings(&self) -> u32 {
            self.metrics.leak_warnings
        }

        pub fn leak_criticals(&self) -> u32 {
            self.metrics.leak_criticals
        }

        pub fn countermeasures_active(&self) -> u32 {
            self.metrics.countermeasures_active
        }

        pub fn common_knowledge_total(&self) -> u32 {
            self.metrics.common_knowledge_total
        }
    }

    /// Borrowed view over the knowledge ledger diff embedded in a [`WorldDelta`].
    #[derive(Debug, Clone, Copy)]
    pub struct KnowledgeLedgerDeltaView<'a> {
        entries: &'a [KnowledgeLedgerEntryState],
        removed: &'a [u64],
        timeline: &'a [KnowledgeTimelineEventState],
        metrics: Option<&'a KnowledgeMetricsState>,
    }

    impl<'a> KnowledgeLedgerDeltaView<'a> {
        /// Construct a new view from a delta reference.
        pub fn from_delta(delta: &'a WorldDelta) -> Self {
            Self {
                entries: &delta.knowledge_ledger,
                removed: &delta.removed_knowledge_ledger,
                timeline: &delta.knowledge_timeline,
                metrics: delta.knowledge_metrics.as_ref(),
            }
        }

        /// Entries added or updated within this delta.
        pub fn entries(&self) -> &'a [KnowledgeLedgerEntryState] {
            self.entries
        }

        /// Ledger keys that were removed within this delta (owner/discovery pairs).
        pub fn removed_keys(&self) -> &'a [u64] {
            self.removed
        }

        /// Convenience iterator over removed (`owner_faction`, `discovery_id`) tuples.
        pub fn removed(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
            self.removed
                .iter()
                .map(|key| decode_knowledge_ledger_key(*key))
        }

        /// Optional metrics summary included with the delta.
        pub fn metrics(&self) -> Option<&'a KnowledgeMetricsState> {
            self.metrics
        }

        /// Timeline events emitted alongside the delta.
        pub fn timeline(&self) -> &'a [KnowledgeTimelineEventState] {
            self.timeline
        }
    }

    /// Encode a knowledge ledger key (owner faction + discovery) into the compact u64 used on the wire.
    pub const fn encode_knowledge_ledger_key(owner_faction: u32, discovery_id: u32) -> u64 {
        ((discovery_id as u64) << 32) | owner_faction as u64
    }

    /// Decode an encoded knowledge ledger key into `(owner_faction, discovery_id)`.
    pub const fn decode_knowledge_ledger_key(key: u64) -> (u32, u32) {
        ((key & 0xFFFF_FFFF) as u32, (key >> 32) as u32)
    }

    #[cfg(test)]
    mod tests {
        use super::{
            decode_knowledge_ledger_key, encode_knowledge_ledger_key, parse_knowledge_telemetry,
            KnowledgeLedgerDeltaView, KnowledgeLedgerView, KnowledgeTelemetryEvent,
            KnowledgeTelemetryFrame,
        };
        use crate::{
            AxisBiasState, CorruptionLedger, GreatDiscoveryTelemetryState, KnowledgeLeakFlags,
            KnowledgeLedgerEntryState, KnowledgeMetricsState, KnowledgeSecurityPosture,
            KnowledgeTimelineEventKind, KnowledgeTimelineEventState, PowerTelemetryState,
            ScalarRasterState, SentimentTelemetryState, SnapshotHeader, TerrainOverlayState,
            WorldDelta, WorldSnapshot,
        };

        fn empty_snapshot() -> WorldSnapshot {
            WorldSnapshot {
                header: SnapshotHeader::default(),
                tiles: Vec::new(),
                logistics: Vec::new(),
                trade_links: Vec::new(),
                populations: Vec::new(),
                power: Vec::new(),
                power_metrics: PowerTelemetryState::default(),
                great_discovery_definitions: Vec::new(),
                great_discoveries: Vec::new(),
                great_discovery_progress: Vec::new(),
                great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
                knowledge_ledger: Vec::new(),
                knowledge_timeline: Vec::new(),
                knowledge_metrics: KnowledgeMetricsState::default(),
                terrain: TerrainOverlayState::default(),
                logistics_raster: ScalarRasterState::default(),
                sentiment_raster: ScalarRasterState::default(),
                corruption_raster: ScalarRasterState::default(),
                fog_raster: ScalarRasterState::default(),
                culture_raster: ScalarRasterState::default(),
                military_raster: ScalarRasterState::default(),
                axis_bias: AxisBiasState::default(),
                sentiment: SentimentTelemetryState::default(),
                generations: Vec::new(),
                corruption: CorruptionLedger::default(),
                influencers: Vec::new(),
                culture_layers: Vec::new(),
                culture_tensions: Vec::new(),
                discovery_progress: Vec::new(),
            }
        }

        #[test]
        fn ledger_key_roundtrip() {
            let owner = 42;
            let discovery = 1337;
            let key = encode_knowledge_ledger_key(owner, discovery);
            assert_ne!(key, 0);
            let (decoded_owner, decoded_discovery) = decode_knowledge_ledger_key(key);
            assert_eq!(decoded_owner, owner);
            assert_eq!(decoded_discovery, discovery);
        }

        #[test]
        fn ledger_view_entry_lookup() {
            let mut snapshot = empty_snapshot();
            snapshot.knowledge_metrics = KnowledgeMetricsState {
                leak_warnings: 2,
                leak_criticals: 1,
                countermeasures_active: 3,
                common_knowledge_total: 4,
            };
            snapshot.knowledge_ledger = vec![
                KnowledgeLedgerEntryState {
                    discovery_id: 7,
                    owner_faction: 1,
                    tier: 3,
                    progress_percent: 55,
                    half_life_ticks: 12,
                    time_to_cascade: 4,
                    security_posture: KnowledgeSecurityPosture::Hardened,
                    countermeasures: Vec::new(),
                    infiltrations: Vec::new(),
                    modifiers: Vec::new(),
                    flags: KnowledgeLeakFlags::COMMON_KNOWLEDGE,
                },
                KnowledgeLedgerEntryState {
                    discovery_id: 8,
                    owner_faction: 2,
                    tier: 4,
                    progress_percent: 90,
                    half_life_ticks: 8,
                    time_to_cascade: 1,
                    security_posture: KnowledgeSecurityPosture::BlackVault,
                    countermeasures: Vec::new(),
                    infiltrations: Vec::new(),
                    modifiers: Vec::new(),
                    flags: KnowledgeLeakFlags::empty(),
                },
            ];
            let view = KnowledgeLedgerView::from_snapshot(&snapshot);
            assert_eq!(view.leak_warnings(), 2);
            assert_eq!(view.leak_criticals(), 1);
            assert_eq!(view.countermeasures_active(), 3);
            assert_eq!(view.common_knowledge_total(), 4);
            let entry = view.entry(1, 7).expect("entry should exist");
            assert!(entry.has_flag(KnowledgeLeakFlags::COMMON_KNOWLEDGE));
            assert_eq!(entry.progress_percent, 55);
            assert!(view.entry(1, 8).is_none());
            let faction_entries: Vec<_> = view.entries_for_faction(2).collect();
            assert_eq!(faction_entries.len(), 1);
            assert_eq!(faction_entries[0].discovery_id, 8);
        }

        #[test]
        fn ledger_delta_view_access() {
            let removed_key = encode_knowledge_ledger_key(2, 11);
            let delta = WorldDelta {
                knowledge_ledger: vec![KnowledgeLedgerEntryState {
                    discovery_id: 9,
                    owner_faction: 3,
                    tier: 2,
                    progress_percent: 30,
                    half_life_ticks: 14,
                    time_to_cascade: 6,
                    security_posture: KnowledgeSecurityPosture::Standard,
                    countermeasures: Vec::new(),
                    infiltrations: Vec::new(),
                    modifiers: Vec::new(),
                    flags: KnowledgeLeakFlags::CASCADE_PENDING,
                }],
                removed_knowledge_ledger: vec![removed_key],
                knowledge_metrics: Some(KnowledgeMetricsState {
                    leak_warnings: 5,
                    leak_criticals: 0,
                    countermeasures_active: 2,
                    common_knowledge_total: 1,
                }),
                knowledge_timeline: vec![KnowledgeTimelineEventState {
                    tick: 12,
                    kind: KnowledgeTimelineEventKind::LeakProgress,
                    source_faction: 3,
                    delta_percent: 5,
                    note_handle: Some("probe success".to_string()),
                }],
                ..WorldDelta::default()
            };

            let view = KnowledgeLedgerDeltaView::from_delta(&delta);
            assert_eq!(view.entries().len(), 1);
            let entry = &view.entries()[0];
            assert!(entry.has_flag(KnowledgeLeakFlags::CASCADE_PENDING));

            let removed: Vec<_> = view.removed().collect();
            assert_eq!(removed, vec![(2, 11)]);

            let metrics = view.metrics().expect("metrics should be present");
            assert_eq!(metrics.leak_warnings, 5);
            assert_eq!(view.timeline().len(), 1);
        }

        #[test]
        fn parse_telemetry_roundtrip() {
            let frame = KnowledgeTelemetryFrame {
                tick: 42,
                leak_warnings: 1,
                leak_criticals: 2,
                countermeasures_active: 3,
                common_knowledge_total: 4,
                events: vec![KnowledgeTelemetryEvent {
                    tick: Some(42),
                    kind: KnowledgeTimelineEventKind::CounterIntel,
                    source_faction: Some(7),
                    delta_percent: Some(-5),
                    note: Some("counter-intel sweep".into()),
                }],
                missions: Vec::new(),
            };
            let payload = serde_json::to_string(&frame).expect("serialize frame");
            let parsed = parse_knowledge_telemetry(&payload).expect("parse frame");
            assert_eq!(parsed, frame);
        }
    }
}

pub use knowledge::{
    decode_knowledge_ledger_key, encode_knowledge_ledger_key, parse_knowledge_telemetry,
    KnowledgeLedgerDeltaView, KnowledgeLedgerView, KnowledgeTelemetryEvent,
    KnowledgeTelemetryFrame, KNOWLEDGE_TELEMETRY_TOPIC,
};
