//! The flat world payloads (`WorldSnapshot` / `WorldDelta`), their header, and the
//! JSON codecs plus the on-disk [`MapExport`]. Bincode appears only inside
//! [`hash_snapshot`] as bytes to hash — there is no bincode codec and no bincode
//! decode path. Since #393 that helper has exactly one caller
//! (`integration_tests/tests/determinism.rs`) and is **off every publication path**:
//! nothing hashes a snapshot per frame any more. See [`SnapshotHeader::hash`].

use crate::state::campaign::{
    CampaignLabel, CampaignProfileState, CommandEventState, PendingForksState, StanceState,
    VictorySnapshotState, VoiceMediumState,
};
use crate::state::connections::ConnectionState;
use crate::state::culture::{
    AxisBiasState, CultureLayerState, CultureTensionState, InfluentialIndividualState,
    SentimentTelemetryState,
};
use crate::state::economy::FactionInventoryState;
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
use crate::state::routes::RouteState;
use crate::state::subsistence::{
    CharacteristicBandState, CraftKnowledgeState, FoodModuleState, ForagePatchState,
    HerdTelemetryState, IntensificationKnowledgeState, KitOptionState, LadderKnowledgeState,
    MaterialDefState, RecipeDefState, SedentarizationState,
};
use ahash::RandomState;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub tile_count: u32,
    pub population_count: u32,
    pub power_count: u32,
    pub influencer_count: u32,
    /// **Always `0` — nothing stamps this, and nothing ever read it (#393).**
    ///
    /// It used to be a content hash written by `WorldSnapshot::finalize`, which bincode-serialized
    /// the *entire world* every published frame (~1.0 ms on an 80×52 map) to produce it. Tracing the
    /// consumers found none: the client's decoder never touches it, rollback never compares it, and
    /// `integration_tests/tests/determinism.rs` — the one place a snapshot hash is genuinely
    /// compared — **zeroes this field** and calls [`hash_snapshot`] itself. So the stamp was a whole
    /// serialization of the world, per frame, for a value nobody consumed.
    ///
    /// The **field and its wire slot stay** deliberately. `snapshot.fbs`'s slots are positional and
    /// this repo's FlatBuffers merges are append-only (root `CLAUDE.md`), so retiring a slot is its
    /// own change with its own regeneration; leaving an always-zero `u64` on the wire costs 8 bytes.
    ///
    /// **Do not start stamping it again without wiring a reader first.** That is the standing rule
    /// in `.claude/rules/core_sim/turn-profiling.md`, and this is the third time it has applied.
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
        population_count: usize,
        power_count: usize,
        influencer_count: usize,
    ) -> Self {
        Self {
            tick,
            tile_count: tile_count as u32,
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

/// The **client view** of a world, and nothing else. Every field here exists because something
/// downstream renders or exports it; simulation state a turn reads lives on `core_sim::SimState`,
/// which is what a rollback restores. Adding a field that no client, export or delta consumes
/// costs a capture and a diff on every turn of every game to produce a value nobody reads —
/// see `.claude/rules/core_sim/checkpoints.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
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
    /// How many turns of `command_events` the sim keeps — the depth of the event dock's history.
    /// Anything older was evicted by the window, not lost on the wire.
    #[serde(default)]
    pub command_events_retention_turns: u32,
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
    /// Per-faction progress on every ladder knowledge, `0..1`. Sparse in FACTIONS (a faction that
    /// has learned nothing is absent) and never in knowledges.
    #[serde(default)]
    pub intensification_knowledge: Vec<IntensificationKnowledgeState>,
    /// **What there IS to learn** — the ladder's knowledge roster, derived from
    /// `intensification_ladder.json` and carrying no faction. A per-world constant, so it is
    /// published once and diffed whole like [`Self::kits`]. It is what lets a client build its
    /// knowledge columns without a hard-coded node list.
    #[serde(default)]
    pub ladder_knowledge: Vec<LadderKnowledgeState>,
    /// **The kit roster** (`equipment.json`'s `kits`) — every kit a party may be sent out with, in
    /// file order, with the tiers each grants. A per-world constant, published once so the client's
    /// picker needs no second copy of the TOE table.
    #[serde(default)]
    pub kits: Vec<KitOptionState>,
    /// The kit each verb uses when the player names none. Both always name a roster entry.
    #[serde(default)]
    pub default_hunt_kit_id: String,
    #[serde(default)]
    pub default_forage_kit_id: String,
    /// The two band-wide roles' defaults. They had no kit axis until the roster gained wayfinding
    /// gear and clubs; both always name a roster entry now, like the two above.
    #[serde(default)]
    pub default_scout_kit_id: String,
    #[serde(default)]
    pub default_warrior_kit_id: String,
    /// **The whole effective TOE config, `serde_json`-serialized** — the designer surface's
    /// read-only catalogue, so the Workbench can print keys nobody wrote client code for. A
    /// per-world constant. Empty string = the sim failed to serialize it. **Only the Workbench may
    /// read it**; a gameplay readout that wants one of these numbers gets a typed field of its own.
    /// See `snapshot.fbs` → `SubsistenceSection.equipmentConfigJson`.
    #[serde(default)]
    pub equipment_config_json: String,
    /// **The materials catalogue** (`materials.json`) — a per-world constant, published so the panel
    /// can name a material's craft and axes without a second copy of the table.
    #[serde(default)]
    pub materials: Vec<MaterialDefState>,
    /// **The shared rating vocabulary, once** — not per material. See [`CharacteristicBandState`].
    #[serde(default)]
    pub characteristic_bands: Vec<CharacteristicBandState>,
    /// **The recipe book** (`recipes.json`) — a per-world constant. The band-relative half is each
    /// cohort's own `craft_offers`.
    #[serde(default)]
    pub recipes: Vec<RecipeDefState>,
    /// **Per faction, per craft.** Not a per-world constant — a craft is *learned* — so it is diffed
    /// as a whole vector each frame.
    #[serde(default)]
    pub craft_knowledge: Vec<CraftKnowledgeState>,
    pub moisture_raster: FloatRasterState,
    pub elevation_overlay: ElevationOverlayState,
    /// Climate-band cut points (`docs/plan_climate_authority.md` §8.3), a per-map constant.
    #[serde(default)]
    pub climate_bands: ClimateBandsState,
    pub start_marker: Option<StartMarkerState>,
    pub terrain: TerrainOverlayState,
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
    /// **The viewer's own directed ties** (`docs/plan_contact_and_logistics.md` §Q2) — a raw
    /// primitive carrying no rider's opinion, filtered to edges whose *observer* band belongs to
    /// the viewer faction. Ordered by `(observer, subject)`, the ledger's own key order.
    #[serde(default)]
    pub connections: Vec<ConnectionState>,
    /// **The roads in the ground the viewer can see** (`docs/plan_standing_upkeep.md` §4.13b) — one
    /// row per road **TILE**, fog-filtered to the tiles the viewer faction has explored. Ordered by
    /// `(y, x)`, the registry's own row-major key order.
    #[serde(default)]
    pub routes: Vec<RouteState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldDelta {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub removed_tiles: Vec<u64>,
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
    /// **Append-only**: the rows appended since the client's cursor, not the whole retained ring.
    /// `None` means nothing new fired this frame. See `core_sim::snapshot::diff_appended` for why
    /// a dropped delta is still safe (`WorldCache::accepts` → `resync_needed` → full snapshot).
    pub command_events: Option<Vec<CommandEventState>>,
    /// `None` means unchanged — an ordinary whole-section diff, unlike the events themselves.
    pub command_events_retention_turns: Option<u32>,
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
    /// The ladder knowledge roster; a per-world constant, so a delta re-sends it only when the world
    /// is rebuilt. `None` means unchanged.
    #[serde(default)]
    pub ladder_knowledge: Option<Vec<LadderKnowledgeState>>,
    /// The kit roster; a per-world constant, so a delta re-sends it only when the world is rebuilt.
    /// `None` means unchanged.
    #[serde(default)]
    pub kits: Option<Vec<KitOptionState>>,
    #[serde(default)]
    pub default_hunt_kit_id: Option<String>,
    #[serde(default)]
    pub default_forage_kit_id: Option<String>,
    #[serde(default)]
    pub default_scout_kit_id: Option<String>,
    #[serde(default)]
    pub default_warrior_kit_id: Option<String>,
    /// The serialized TOE config; a per-world constant, so a delta re-sends it only when the world
    /// is rebuilt. `None` means unchanged.
    #[serde(default)]
    pub equipment_config_json: Option<String>,
    /// The crafting catalogues; per-world constants, so a delta re-sends them only on a world
    /// rebuild. `None` means unchanged.
    #[serde(default)]
    pub materials: Option<Vec<MaterialDefState>>,
    #[serde(default)]
    pub characteristic_bands: Option<Vec<CharacteristicBandState>>,
    #[serde(default)]
    pub recipes: Option<Vec<RecipeDefState>>,
    /// Per-faction craft progress — diffed whole like the ladder's own knowledge rows, not held as a
    /// world constant. `None` means unchanged.
    #[serde(default)]
    pub craft_knowledge: Option<Vec<CraftKnowledgeState>>,
    pub moisture_raster: Option<FloatRasterState>,
    pub elevation_overlay: Option<ElevationOverlayState>,
    /// Climate-band cut points; a per-map constant, so a delta re-sends it only when the map is
    /// (re)generated. `None` means unchanged.
    #[serde(default)]
    pub climate_bands: Option<ClimateBandsState>,
    pub start_marker: Option<StartMarkerState>,
    pub axis_bias: Option<AxisBiasState>,
    pub sentiment: Option<SentimentTelemetryState>,
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
    /// `None` = unchanged this frame. A whole-vector diff like the other `Whole` sections; a
    /// section with no delta twin is permanently stale on a delta-fed client.
    pub connections: Option<Vec<ConnectionState>>,
    /// `None` = unchanged this frame, on `connections`' own rules.
    pub routes: Option<Vec<RouteState>>,
}

pub fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut clone = snapshot.clone();
    clone.header.hash = 0;
    let encoded = bincode::serialize(&clone).expect("snapshot serialization for hashing");
    let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(&encoded);
    hasher.finish()
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

    /// `hash_snapshot` is the only content hash left, and its **only caller is
    /// `integration_tests/tests/determinism.rs`** — the per-frame `WorldSnapshot::finalize` stamp was
    /// retired in #393 because nothing read `header.hash` (see the type's doc comment). So pin the
    /// two properties that caller depends on and nothing else exercises: it is deterministic across
    /// calls, and it **ignores whatever `header.hash` already holds**, which is what lets the test
    /// zero the field on two snapshots and compare the rest.
    #[test]
    fn hash_snapshot_is_deterministic_and_ignores_the_stored_hash() {
        let mut snapshot = WorldSnapshot {
            fog_enabled: true,
            ..Default::default()
        };
        snapshot.header.tick = 7;
        snapshot.header.population_count = 3;

        let expected = hash_snapshot(&snapshot);
        assert_ne!(expected, 0, "a real hash of real content");
        assert_eq!(hash_snapshot(&snapshot), expected, "deterministic");

        let mut stale = snapshot.clone();
        stale.header.hash = u64::MAX;
        assert_eq!(
            hash_snapshot(&stale),
            expected,
            "the stored hash must not feed into the hash, or it would depend on its own history"
        );

        snapshot.header.tick = 8;
        assert_ne!(
            hash_snapshot(&snapshot),
            expected,
            "and it must actually depend on the content, or the equality above proves nothing"
        );
    }
}
