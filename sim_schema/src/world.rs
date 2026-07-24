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
    pub fog_raster: ScalarRasterState,
    pub culture_raster: ScalarRasterState,
    pub military_raster: ScalarRasterState,
    #[serde(default)]
    pub visibility_raster: ScalarRasterState,
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
    pub pending_forks: Option<Vec<PendingForksState>>,
    pub stance_axes: Option<Vec<StanceState>>,
    pub voice_medium: Option<Vec<VoiceMediumState>>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
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
    pub fog_raster: Option<ScalarRasterState>,
    pub culture_raster: Option<ScalarRasterState>,
    pub military_raster: Option<ScalarRasterState>,
    pub visibility_raster: Option<ScalarRasterState>,
    pub generations: Vec<GenerationState>,
    pub removed_generations: Vec<u16>,
    pub corruption: Option<CorruptionLedger>,
    pub influencers: Vec<InfluentialIndividualState>,
    pub removed_influencers: Vec<u32>,
    pub terrain: Option<TerrainOverlayState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub removed_culture_layers: Vec<u32>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

impl WorldSnapshot {
    pub fn finalize(mut self) -> Self {
        let hash = hash_snapshot(&self);
        let mut header = self.header;
        header.hash = hash;
        self.header = header;
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
