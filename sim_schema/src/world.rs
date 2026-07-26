//! The flat world payloads (`WorldSnapshot` / `WorldDelta`), their header, and the
//! bincode / JSON codecs plus the on-disk [`MapExport`].

use crate::state::campaign::{
    BeatLedgerState, CampaignLabel, CampaignProfileState, CommandEventState, PendingForksState,
    StanceState, VictorySnapshotState, VoiceMediumState,
};
use crate::state::culture::{
    AxisBiasState, CultureLayerState, CultureTensionState, InfluentialIndividualState,
    SentimentTelemetryState,
};
use crate::state::economy::{FactionInventoryState, LogisticsLinkState, TradeLinkState};
use crate::state::governance::{
    CorruptionLedger, CrisisOverlayState, CrisisTelemetryState, PowerNodeState, PowerTelemetryState,
};
use crate::state::knowledge::{
    DiscoveredSitesState, DiscoveryProgressEntry, GreatDiscoveryDefinitionState,
    GreatDiscoveryProgressState, GreatDiscoveryState, GreatDiscoveryTelemetryState,
    KnowledgeLedgerEntryState, KnowledgeMetricsState, KnowledgeTimelineEventState,
};
use crate::state::map::{
    ClimateBandsState, ElevationOverlayState, FloatRasterState, ScalarRasterState,
    StartMarkerState, TerrainOverlayState, TerrainSample, TileState,
};
use crate::state::population::{
    GenerationState, PopulationCohortState, PopulationDemographicsState,
};
use crate::state::subsistence::{
    FoodModuleState, ForagePatchState, ForageState, GrazeState, HerdState, HerdTelemetryState,
    IntensificationKnowledgeState, SedentarizationState,
};
use ahash::RandomState;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub tile_count: u32,
    pub logistics_count: u32,
    pub trade_link_count: u32,
    pub population_count: u32,
    pub power_count: u32,
    pub influencer_count: u32,
    pub hash: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_label: Option<CampaignLabel>,
    #[serde(default)]
    pub wrap_horizontal: bool,
    /// Build identifier of the server binary (see `snapshot.fbs`). Set by core_sim.
    #[serde(default)]
    pub server_build: String,
    /// Monotonic world-build counter (see `snapshot.fbs`). Incremented on every world (re)build,
    /// identical for every snapshot within one world; a client uses it to ignore a stale world the
    /// snapshot server replays to reconnecting subscribers. Set by core_sim.
    #[serde(default)]
    pub world_epoch: u32,
    /// Monotonic **publication** counter, reset with `world_epoch` (see `snapshot.fbs`). Counts
    /// frames, not ticks — `recapture_and_broadcast` publishes mid-tick on every world-mutating
    /// command, so several frames share a tick and tick-continuity cannot detect a gap. Set by
    /// core_sim.
    #[serde(default)]
    pub frame_seq: u64,
    /// Delta only: the [`Self::frame_seq`] this delta applies to. `0` on a full snapshot, which is
    /// always applicable. See `docs/plan_delta_streaming.md` §3.3 for the client's contract.
    #[serde(default)]
    pub base_frame_seq: u64,
}

impl SnapshotHeader {
    pub fn new(
        tick: u64,
        tile_count: usize,
        logistics_count: usize,
        trade_link_count: usize,
        population_count: usize,
        power_count: usize,
        influencer_count: usize,
    ) -> Self {
        Self {
            tick,
            tile_count: tile_count as u32,
            logistics_count: logistics_count as u32,
            trade_link_count: trade_link_count as u32,
            population_count: population_count as u32,
            power_count: power_count as u32,
            influencer_count: influencer_count as u32,
            hash: 0,
            campaign_label: None,
            wrap_horizontal: false,
            server_build: String::new(),
            world_epoch: 0,
            frame_seq: 0,
            base_frame_seq: 0,
        }
    }

    /// Sets the server build identifier reported to clients.
    pub fn with_server_build(mut self, build: impl Into<String>) -> Self {
        self.server_build = build.into();
        self
    }

    /// Creates a header with wrap_horizontal set.
    pub fn with_wrap_horizontal(mut self, wrap: bool) -> Self {
        self.wrap_horizontal = wrap;
        self
    }
}

fn default_fog_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub logistics: Vec<LogisticsLinkState>,
    pub trade_links: Vec<TradeLinkState>,
    pub populations: Vec<PopulationCohortState>,
    pub power: Vec<PowerNodeState>,
    pub power_metrics: PowerTelemetryState,
    pub great_discovery_definitions: Vec<GreatDiscoveryDefinitionState>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: GreatDiscoveryTelemetryState,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    pub knowledge_metrics: KnowledgeMetricsState,
    pub crisis_telemetry: CrisisTelemetryState,
    pub crisis_overlay: CrisisOverlayState,
    pub victory: VictorySnapshotState,
    #[serde(default)]
    pub capability_flags: u32,
    #[serde(default)]
    pub campaign_profiles: Vec<CampaignProfileState>,
    #[serde(default)]
    pub command_events: Vec<CommandEventState>,
    /// The Telling's fork tier, per faction: what is on the table right now.
    #[serde(default)]
    pub pending_forks: Vec<PendingForksState>,
    /// The Telling's effective stance per faction and axis.
    #[serde(default)]
    pub stance_axes: Vec<StanceState>,
    /// The Telling's narrator medium per faction (presentational — see `VoiceMediumState`).
    #[serde(default)]
    pub voice_medium: Vec<VoiceMediumState>,
    #[serde(default)]
    pub herds: Vec<HerdTelemetryState>,
    /// Authoritative herd sim state (`HerdRegistry`), round-tripped for rollback correctness —
    /// distinct from the lossy display `herds` above (which the client consumes). Not wired to the
    /// FlatBuffers client stream; rollback restore reads it via `HerdRegistry::update_from_states`.
    #[serde(default)]
    pub herd_registry: Vec<HerdState>,
    /// Authoritative depletable-forage sim state (`ForageRegistry`), round-tripped for rollback
    /// correctness (biomass / ecology phase per patch). Like `herd_registry`, this is not wired to
    /// the FlatBuffers client stream; rollback restore reads it via `ForageRegistry::update_from_states`.
    #[serde(default)]
    pub forage_registry: Vec<ForageState>,
    /// Authoritative graze/pasture sim state (`GrazeRegistry`), round-tripped for rollback correctness
    /// (biomass / ecology phase per land tile). Like `herd_registry` / `forage_registry` this is the
    /// *sim* record and is not on the FlatBuffers client stream — the client reads graze off the
    /// per-tile `TileState.graze_*` fields. Restore reads it via `GrazeRegistry::update_from_states`.
    #[serde(default)]
    pub graze_registry: Vec<GrazeState>,
    /// The Telling's narrative memory (`BeatLedger`), round-tripped for rollback correctness.
    /// Like the registries above this is the *sim* record and is not on the FlatBuffers client
    /// stream; restore reads it via `BeatLedger::from_state`.
    #[serde(default)]
    pub beat_ledger: BeatLedgerState,
    #[serde(default)]
    pub food_modules: Vec<FoodModuleState>,
    #[serde(default)]
    pub faction_inventory: Vec<FactionInventoryState>,
    #[serde(default)]
    pub sedentarization: Vec<SedentarizationState>,
    #[serde(default)]
    pub discovered_sites: Vec<DiscoveredSitesState>,
    #[serde(default)]
    pub demographics: Vec<PopulationDemographicsState>,
    /// Per-tile depletable-forage cultivation/ecology display state (Intensification Phase 1a).
    #[serde(default)]
    pub forage_patches: Vec<ForagePatchState>,
    /// Per-faction Cultivation/Herding knowledge progress (Intensification Rung 1b/1c).
    #[serde(default)]
    pub intensification_knowledge: Vec<IntensificationKnowledgeState>,
    pub moisture_raster: FloatRasterState,
    pub elevation_overlay: ElevationOverlayState,
    /// Climate-band cut points (`docs/plan_climate_authority.md` §8.3), a per-map constant.
    #[serde(default)]
    pub climate_bands: ClimateBandsState,
    pub start_marker: Option<StartMarkerState>,
    pub terrain: TerrainOverlayState,
    pub logistics_raster: ScalarRasterState,
    pub sentiment_raster: ScalarRasterState,
    pub corruption_raster: ScalarRasterState,
    pub culture_raster: ScalarRasterState,
    pub military_raster: ScalarRasterState,
    #[serde(default)]
    pub visibility_raster: ScalarRasterState,
    /// The server-owned fog-of-war master switch (`SimulationConfig::fog_enabled`), published so the
    /// client renders the sim's decision rather than a local flag. Serde defaults it TRUE to match
    /// the FlatBuffers field default; note the struct's *derived* `Default` still yields `false`, so
    /// every constructor sets it explicitly (`capture.rs`, the `sim_runtime` placeholder snapshot,
    /// the xtask fixture).
    #[serde(default = "default_fog_enabled")]
    pub fog_enabled: bool,
    pub axis_bias: AxisBiasState,
    pub sentiment: SentimentTelemetryState,
    pub generations: Vec<GenerationState>,
    pub corruption: CorruptionLedger,
    pub influencers: Vec<InfluentialIndividualState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldDelta {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub removed_tiles: Vec<u64>,
    pub logistics: Vec<LogisticsLinkState>,
    pub removed_logistics: Vec<u64>,
    pub trade_links: Vec<TradeLinkState>,
    pub removed_trade_links: Vec<u64>,
    pub populations: Vec<PopulationCohortState>,
    pub removed_populations: Vec<u64>,
    pub power: Vec<PowerNodeState>,
    pub removed_power: Vec<u64>,
    pub power_metrics: Option<PowerTelemetryState>,
    pub great_discovery_definitions: Option<Vec<GreatDiscoveryDefinitionState>>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: Option<GreatDiscoveryTelemetryState>,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub removed_knowledge_ledger: Vec<u64>,
    pub knowledge_metrics: Option<KnowledgeMetricsState>,
    pub victory: Option<VictorySnapshotState>,
    pub capability_flags: Option<u32>,
    pub command_events: Option<Vec<CommandEventState>>,
    /// The campaign profile roster. `None` means unchanged.
    ///
    /// This was absent from `WorldDelta` entirely until delta streaming — harmless while the
    /// client only ever saw the field on a full snapshot, and a silent hole the moment deltas
    /// became the steady-state carrier (`docs/plan_delta_streaming.md` §2.4). The FlatBuffers
    /// slot always existed on the shared `CampaignSection`; only this side and the codec were
    /// missing.
    pub campaign_profiles: Option<Vec<CampaignProfileState>>,
    pub pending_forks: Option<Vec<PendingForksState>>,
    pub stance_axes: Option<Vec<StanceState>>,
    pub voice_medium: Option<Vec<VoiceMediumState>>,
    /// The knowledge timeline, sent as a whole section. `None` means unchanged.
    ///
    /// Not a diff list: it carries no `removed_*` counterpart and the capture path replaces it
    /// wholesale, so `Some(vec![])` has to be able to say "the timeline is now empty" — see
    /// [`WorldDelta::culture_tensions`] for the bug a bare `Vec` caused on the sibling field.
    pub knowledge_timeline: Option<Vec<KnowledgeTimelineEventState>>,
    pub crisis_telemetry: Option<CrisisTelemetryState>,
    pub crisis_overlay: Option<CrisisOverlayState>,
    pub herds: Option<Vec<HerdTelemetryState>>,
    pub food_modules: Option<Vec<FoodModuleState>>,
    pub faction_inventory: Option<Vec<FactionInventoryState>>,
    pub sedentarization: Option<Vec<SedentarizationState>>,
    pub discovered_sites: Option<Vec<DiscoveredSitesState>>,
    pub demographics: Option<Vec<PopulationDemographicsState>>,
    pub forage_patches: Option<Vec<ForagePatchState>>,
    pub intensification_knowledge: Option<Vec<IntensificationKnowledgeState>>,
    pub moisture_raster: Option<FloatRasterState>,
    pub elevation_overlay: Option<ElevationOverlayState>,
    /// Climate-band cut points; a per-map constant, so a delta re-sends it only when the map is
    /// (re)generated. `None` means unchanged.
    #[serde(default)]
    pub climate_bands: Option<ClimateBandsState>,
    pub start_marker: Option<StartMarkerState>,
    pub axis_bias: Option<AxisBiasState>,
    pub sentiment: Option<SentimentTelemetryState>,
    pub logistics_raster: Option<ScalarRasterState>,
    pub sentiment_raster: Option<ScalarRasterState>,
    pub corruption_raster: Option<ScalarRasterState>,
    pub culture_raster: Option<ScalarRasterState>,
    pub military_raster: Option<ScalarRasterState>,
    pub visibility_raster: Option<ScalarRasterState>,
    /// Carried on EVERY delta, not diffed: the FlatBuffers field defaults to `true` when absent, so
    /// an omitted value would silently re-enable fog on the client one delta after it was turned off.
    #[serde(default = "default_fog_enabled")]
    pub fog_enabled: bool,
    pub generations: Vec<GenerationState>,
    pub removed_generations: Vec<u16>,
    pub corruption: Option<CorruptionLedger>,
    pub influencers: Vec<InfluentialIndividualState>,
    pub removed_influencers: Vec<u32>,
    pub terrain: Option<TerrainOverlayState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub removed_culture_layers: Vec<u32>,
    /// The culture-tension roster, sent as a whole section. `None` means unchanged.
    ///
    /// This is `Option` rather than a bare `Vec` because tensions have no `removed_culture_tensions`
    /// counterpart, so the list is only ever replaced wholesale. While it was a bare `Vec`, "nothing
    /// changed" and "the last tension just resolved" were the same bytes on the wire and the client
    /// had to guess — reading it as a replacement blanked the tension list on every delta, reading it
    /// as unchanged left a genuinely-emptied list stale until the next full snapshot. The codec now
    /// writes the FlatBuffers vector only for `Some`, so absence carries the distinction.
    pub culture_tensions: Option<Vec<CultureTensionState>>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

impl WorldSnapshot {
    /// Stamp the content hash onto the header.
    ///
    /// **Deliberately does not call [`hash_snapshot`], and must not be "simplified" back into one.**
    /// That helper takes `&WorldSnapshot`, so it has to deep-clone the entire world just to zero one
    /// `u64` before serializing — ~8% of a turn on an 80×52 map. `finalize` already *owns* `self`,
    /// so it can zero the header in place and hash the same bytes with no copy at all.
    ///
    /// The two are byte-for-byte equivalent by construction: both serialize the whole snapshot with
    /// `header.hash == 0` and hash the resulting buffer with the same fixed-seed hasher. Zeroing
    /// first also matters for re-finalizing an already-stamped snapshot (the on-demand feed paths
    /// clone the previous snapshot and call `finalize` again) — the stale hash must not be hashed in.
    pub fn finalize(mut self) -> Self {
        self.header.hash = 0;
        let encoded = bincode::serialize(&self).expect("snapshot serialization for hashing");
        let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
        hasher.write(&encoded);
        self.header.hash = hasher.finish();
        self
    }
}

pub fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut clone = snapshot.clone();
    clone.header.hash = 0;
    let encoded = bincode::serialize(&clone).expect("snapshot serialization for hashing");
    let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(&encoded);
    hasher.finish()
}

pub fn encode_snapshot(snapshot: &WorldSnapshot) -> bincode::Result<Vec<u8>> {
    bincode::serialize(snapshot)
}

pub fn encode_delta(delta: &WorldDelta) -> bincode::Result<Vec<u8>> {
    bincode::serialize(delta)
}

pub fn encode_snapshot_json(snapshot: &WorldSnapshot) -> serde_json::Result<String> {
    serde_json::to_string(snapshot)
}

pub fn decode_snapshot_json(data: &str) -> serde_json::Result<WorldSnapshot> {
    serde_json::from_str(data)
}

pub fn encode_delta_json(delta: &WorldDelta) -> serde_json::Result<String> {
    serde_json::to_string(delta)
}

pub fn decode_delta_json(data: &str) -> serde_json::Result<WorldDelta> {
    serde_json::from_str(data)
}

/// A self-describing on-disk export of a running game's map: the full
/// [`WorldSnapshot`] plus the resolved worldgen seed and preset needed to
/// reproduce it. Written by the `export_map` command and consumed as a test
/// fixture (see [`decode_map_export_json`]). Wrapping the snapshot rather than
/// adding a seed to [`SnapshotHeader`] keeps the wire schema untouched while
/// giving offline consumers everything in one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapExport {
    /// Resolved worldgen seed the running game was generated from.
    pub seed: u64,
    /// Preset id the map was generated with (empty when none was active).
    pub preset: String,
    /// Terrain grid width in tiles; mirrors `snapshot.terrain.width` so the
    /// row-major `(x, y)` indexing of the samples is self-documenting.
    pub width: u32,
    /// Terrain grid height in tiles; mirrors `snapshot.terrain.height`.
    pub height: u32,
    /// Full world snapshot captured at export time.
    pub snapshot: WorldSnapshot,
}

impl MapExport {
    /// Build an export from a captured snapshot, deriving the grid dimensions
    /// from the terrain overlay so callers cannot desync `width`/`height` from
    /// the sample buffer.
    pub fn from_snapshot(seed: u64, preset: impl Into<String>, snapshot: WorldSnapshot) -> Self {
        let width = snapshot.terrain.width;
        let height = snapshot.terrain.height;
        Self {
            seed,
            preset: preset.into(),
            width,
            height,
            snapshot,
        }
    }

    /// Return the terrain sample at row-major `(x, y)`, or `None` when the
    /// coordinate is outside the grid. This is the canonical way for offline
    /// consumers (tests, inspection) to reference a hex by coordinate.
    pub fn tile_at(&self, x: u32, y: u32) -> Option<&TerrainSample> {
        // Use the terrain overlay's own dimensions as canonical rather than the
        // top-level `width`/`height` mirrors: a hand-edited or corrupted export
        // could desync the mirrors from the sample buffer, and indexing off a
        // stale mirror would silently return the wrong (but in-bounds) tile.
        let width = self.snapshot.terrain.width;
        let height = self.snapshot.terrain.height;
        if x >= width || y >= height {
            return None;
        }
        let idx = (y as usize) * (width as usize) + (x as usize);
        self.snapshot.terrain.samples.get(idx)
    }
}

/// Encode a [`MapExport`] as pretty-printed JSON (human-readable for offline
/// inspection).
pub fn encode_map_export_json(export: &MapExport) -> serde_json::Result<String> {
    serde_json::to_string_pretty(export)
}

/// Decode a [`MapExport`] previously written by [`encode_map_export_json`].
pub fn decode_map_export_json(data: &str) -> serde_json::Result<MapExport> {
    serde_json::from_str(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WorldSnapshot::finalize` hashes in place instead of deep-cloning through [`hash_snapshot`],
    /// which is only sound while the two produce identical bytes. Pin that equivalence here: the
    /// optimisation is invisible in behaviour, so nothing else would catch a divergence, and
    /// `hash_snapshot` still has a direct caller (`integration_tests/tests/determinism.rs`).
    #[test]
    fn finalize_stamps_exactly_the_free_standing_hash() {
        let mut snapshot = WorldSnapshot {
            fog_enabled: true,
            ..Default::default()
        };
        snapshot.header.tick = 7;
        snapshot.header.population_count = 3;

        let expected = hash_snapshot(&snapshot);
        let finalized = snapshot.clone().finalize();

        assert_eq!(finalized.header.hash, expected);
        assert_ne!(
            finalized.header.hash, 0,
            "a real hash, not the zeroed field"
        );
    }

    /// Re-finalizing an already-stamped snapshot must ignore the stale hash — the on-demand feed
    /// paths clone the previous snapshot, mutate one field, and call `finalize` again, so hashing
    /// the old value in would make the hash depend on its own history.
    #[test]
    fn finalize_is_idempotent_and_ignores_a_stale_hash() {
        let snapshot = WorldSnapshot::default().finalize();
        let first = snapshot.header.hash;

        let refinalized = snapshot.clone().finalize();
        assert_eq!(refinalized.header.hash, first);

        let mut stale = snapshot;
        stale.header.hash = u64::MAX;
        assert_eq!(stale.finalize().header.hash, first);
    }
}
