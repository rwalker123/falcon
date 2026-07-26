//! `SnapshotDecoder` -- the GDScript entry point that turns a FlatBuffers envelope
//! into the snapshot `Dictionary` the client renders from.

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::campaign::{
    campaign_profiles_to_array, command_events_to_array, pending_forks_to_array,
    stance_axes_to_array, victory_state_to_dict, voice_medium_to_array,
};
use crate::dict::culture::{
    axis_bias_to_dict, culture_layers_to_array, culture_tensions_to_array, influencers_to_array,
    sentiment_to_dict,
};
use crate::dict::economy::trade_links_to_array;
use crate::dict::governance::{
    corruption_to_dict, crisis_overlay_to_dict, crisis_telemetry_to_dict, power_metrics_to_dict,
    power_nodes_to_array,
};
use crate::dict::knowledge::{
    discovered_sites_to_array, discovery_progress_to_array, great_discovery_definitions_to_array,
    great_discovery_progress_states_to_array, great_discovery_states_to_array,
    great_discovery_telemetry_to_dict,
};
use crate::dict::map::tiles_to_array;
use crate::dict::population::{demographics_to_array, generations_to_array, populations_to_array};
use crate::dict::subsistence::{
    forage_patches_to_array, herds_to_array, intensification_knowledge_to_array,
    sedentarization_to_array,
};
use crate::dict::{
    u16_vector_to_packed_int32, u32_vector_to_packed_int32, u64_vector_to_packed_int64,
};
use crate::snapshot::cache::{RasterCache, WorldCache};
use crate::snapshot::delta::DeltaAggregator;
use crate::snapshot::snapshot_to_dict;

/// Wall-clock microseconds a decode that measured nothing reports — the value
/// [`SnapshotDecoder::get_last_decode_usec`] answers before any frame has been decoded.
const NO_DECODE_RECORDED_USEC: i64 = 0;

/// Dictionary key naming which of the two envelope payloads produced this frame, and its two
/// values. **This is the authoritative answer** — it is read straight off
/// `Envelope::payload_type`, the same discriminant the encoder wrote.
///
/// It replaces `Main._snapshot_is_delta`, which guessed from the presence of six delta-only keys
/// (`tile_updates`, `population_updates`, …). That guess holds today only by accident: the delta
/// codec emits an *empty* vector rather than omitting an untouched section, so `tile_updates` is
/// present on every delta including one that changed no tile. A codec that ever starts omitting
/// empty sections — which is exactly what a bandwidth-conscious delta encoder wants to do — would
/// silently reclassify deltas as full snapshots, and a full snapshot resets the command feed and
/// can trip the world-epoch reset. See `docs/plan_delta_streaming.md` §2.
const FRAME_KIND_KEY: &str = "frame_kind";
const FRAME_KIND_SNAPSHOT: &str = "snapshot";
const FRAME_KIND_DELTA: &str = "delta";

/// Publication sequence keys (`snapshot.fbs` `frameSeq` / `baseFrameSeq`). `frame_seq` rides both
/// frame kinds; `base_frame_seq` is meaningful only on a delta, where it names the frame this one
/// applies to.
const FRAME_SEQ_KEY: &str = "frame_seq";
const BASE_FRAME_SEQ_KEY: &str = "base_frame_seq";

#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct SnapshotDecoder {
    /// The world the next delta will be applied to: the last complete dictionary, the raster
    /// inputs behind it, and the publication identity that decides whether a delta may be merged
    /// at all. `None` until the first full snapshot — a delta arriving before one is dropped,
    /// because there is nothing for it to be a delta *of*.
    cache: Option<WorldCache>,
    /// Set when a delta was DROPPED because it could not be applied — its `baseFrameSeq` names a
    /// frame this client never applied, or it belongs to a different world. Read and cleared by
    /// `take_resync_needed`, which is what turns a dropped frame into a `resync` request.
    ///
    /// Deliberately NOT set for a malformed or headerless frame: that is corruption, and asking
    /// the server to resend the world does not fix it.
    resync_needed: bool,
    /// Microseconds the most recent `decode_snapshot`/`decode_delta` spent inside
    /// `snapshot_to_dict` — i.e. the whole FlatBuffers→`VarDictionary` conversion, which is the
    /// dominant term. Read from GDScript by `SnapshotLoader` for its `decode.native` phase, so the
    /// client's per-turn profile line can separate the conversion itself from the Variant
    /// marshalling the binding adds on top of it.
    #[init(val = NO_DECODE_RECORDED_USEC)]
    last_decode_usec: i64,
}

#[godot_api]
impl SnapshotDecoder {
    /// Decode one frame of either kind, applying a delta to the cached world.
    ///
    /// **An unapplicable delta answers an empty dictionary**, which reaches the loader as "no
    /// frame" and is skipped — the same contract a malformed snapshot already had. That is the
    /// whole point of the frame-sequence gate: merging a delta whose base the client never saw
    /// produces a world that is silently wrong rather than visibly broken, and silently wrong is
    /// the failure mode this arc is least able to detect (`docs/plan_delta_streaming.md` §3.3).
    /// The caller recovers by asking for a full snapshot.
    #[func]
    pub fn decode_snapshot(&mut self, data: PackedByteArray) -> VarDictionary {
        let started = std::time::Instant::now();
        let decoded = self.decode_frame(&data).unwrap_or_default();
        self.last_decode_usec = elapsed_usec(started);
        decoded
    }

    #[func]
    pub fn decode_delta(&mut self, data: PackedByteArray) -> VarDictionary {
        self.decode_snapshot(data)
    }

    /// Has a delta been dropped since this was last asked? Clears on read, so each dropped frame
    /// produces at most one request and the caller decides the retry cadence.
    #[func]
    pub fn take_resync_needed(&mut self) -> bool {
        std::mem::replace(&mut self.resync_needed, false)
    }

    /// True once a full snapshot has established a baseline. `SnapshotLoader` reads it to tell
    /// "dropped because we have no baseline yet" from "dropped because the frame was bad".
    #[func]
    pub fn has_baseline(&self) -> bool {
        self.cache.is_some()
    }

    /// `frameSeq` of the last frame merged, or 0 before any. The value a resync request quotes.
    #[func]
    pub fn get_applied_frame_seq(&self) -> i64 {
        self.cache.as_ref().map(|c| c.frame_seq as i64).unwrap_or(0)
    }

    /// Microseconds the LAST decode call took, or [`NO_DECODE_RECORDED_USEC`] before the first
    /// one. Per-call rather than accumulated: a poll decodes every queued frame, and the caller
    /// (`SnapshotLoader.poll_stream`) is the thing that knows how many that was.
    #[func]
    pub fn get_last_decode_usec(&self) -> i64 {
        self.last_decode_usec
    }
}

/// Elapsed microseconds since `started`, saturating rather than wrapping. A decode is milliseconds
/// long, so the clamp is unreachable in practice and exists only so the cast cannot be lossy.
fn elapsed_usec(started: std::time::Instant) -> i64 {
    i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX)
}

impl SnapshotDecoder {
    /// Decode one envelope, updating [`Self::cache`]. `None` means "no frame" — malformed,
    /// headerless, or a delta that cannot be applied to what we hold.
    fn decode_frame(&mut self, data: &PackedByteArray) -> Option<VarDictionary> {
        if data.is_empty() {
            return None;
        }
        let envelope = fb::root_as_envelope(data.as_slice()).ok()?;
        match envelope.payload_type() {
            fb::SnapshotPayload::snapshot => {
                let snapshot = envelope.payload_as_snapshot()?;
                // `?`, not `map`: `snapshot_to_dict` answers `None` for a headerless
                // snapshot (see its docs), and that `None` must reach the caller as "no frame"
                // rather than a `Some(...)` the loader would treat as a decoded world.
                let (mut dict, rasters) = snapshot_to_dict(snapshot)?;
                let header = snapshot.header()?;
                let _ = dict.insert(FRAME_KIND_KEY, FRAME_KIND_SNAPSHOT);
                // A full snapshot names no base — it is applicable against any client state — so
                // only `frame_seq` is published here.
                let _ = dict.insert(FRAME_SEQ_KEY, header.frameSeq() as i64);
                self.cache = Some(WorldCache {
                    world_epoch: header.worldEpoch(),
                    frame_seq: header.frameSeq(),
                    dict: dict.duplicate_shallow(),
                    rasters,
                });
                Some(dict)
            }
            fb::SnapshotPayload::delta => {
                let delta = envelope.payload_as_delta()?;
                let header = delta.header()?;
                // No baseline, wrong world, or a base we never applied: drop the frame. Merging it
                // anyway is precisely how the client's world drifts from the server's without
                // anything failing.
                let Some(cache) = self.cache.as_ref() else {
                    // No baseline at all. The client has nothing to apply a delta to and must be
                    // sent a full world before anything can render.
                    self.resync_needed = true;
                    return None;
                };
                if !cache.accepts(header.worldEpoch(), header.baseFrameSeq()) {
                    self.resync_needed = true;
                    return None;
                }
                let (delta_dict, rasters) = decode_delta_against(delta, &cache.rasters)?;
                // Merge: the cached world, overwritten by every key this delta carried. Absent
                // keys keep their cached value, which is what "absent means unchanged" has always
                // meant on the wire — the difference is that there is now something for it to be
                // unchanged *from*.
                let mut merged = cache.dict.duplicate_shallow();
                for (key, value) in delta_dict.iter_shared() {
                    merged.set(&key, &value);
                }
                self.cache = Some(WorldCache {
                    world_epoch: header.worldEpoch(),
                    frame_seq: header.frameSeq(),
                    dict: merged.duplicate_shallow(),
                    rasters,
                });
                Some(merged)
            }
            _ => None,
        }
    }
}

/// Build the dictionary of everything one delta carried, seeded from `cache` so the channels it
/// omits re-derive from the previous frame instead of zeroing.
fn decode_delta_against(
    delta: fb::WorldDelta<'_>,
    cache: &RasterCache,
) -> Option<(VarDictionary, RasterCache)> {
    let mut agg = DeltaAggregator::from_cache(cache);
    let mut frame_seq: u64 = 0;
    let mut base_frame_seq: u64 = 0;
    if let Some(header) = delta.header() {
        agg.tick = header.tick();
        agg.wrap_horizontal = header.wrapHorizontal();
        agg.world_epoch = header.worldEpoch();
        frame_seq = header.frameSeq();
        base_frame_seq = header.baseFrameSeq();
        if let Some(build) = header.serverBuild() {
            agg.server_build = build.to_string();
        }
    }
    if let Some(tiles) = delta.map().and_then(|s| s.tiles()) {
        for tile in tiles {
            agg.update_tile(
                tile.x(),
                tile.y(),
                tile.temperature(),
                tile.grazeCapacity(),
                tile.forageCapacity(),
            );
        }
    }
    if let Some(layer) = delta.map().and_then(|s| s.terrainOverlay()) {
        agg.apply_terrain_overlay(layer);
    }
    if let Some(raster) = delta.economy().and_then(|s| s.logisticsRaster()) {
        agg.apply_logistics_raster(raster);
    }
    if let Some(raster) = delta.culture().and_then(|s| s.sentimentRaster()) {
        agg.apply_sentiment_raster(raster);
    }
    if let Some(raster) = delta.governance().and_then(|s| s.corruptionRaster()) {
        agg.apply_corruption_raster(raster);
    }
    if let Some(raster) = delta.vision().and_then(|s| s.visibilityRaster()) {
        agg.apply_visibility_raster(raster);
    }
    if let Some(vision) = delta.vision() {
        agg.apply_fog_enabled(vision.fogEnabled());
    }
    if let Some(raster) = delta.culture().and_then(|s| s.cultureRaster()) {
        agg.apply_culture_raster(raster);
    }
    if let Some(raster) = delta.vision().and_then(|s| s.militaryRaster()) {
        agg.apply_military_raster(raster);
    }
    if let Some(overlay) = delta.governance().and_then(|s| s.crisisOverlay()) {
        agg.apply_crisis_overlay(overlay);
    }
    if let Some(overlay) = delta.map().and_then(|s| s.elevationOverlay()) {
        agg.apply_elevation_overlay(overlay);
    }
    if let Some(bands) = delta.map().and_then(|s| s.climateBands()) {
        agg.apply_climate_bands(bands);
    }
    if let Some(raster) = delta.map().and_then(|s| s.moistureRaster()) {
        agg.apply_moisture_raster(raster);
    }
    let rasters = agg.raster_cache();
    let mut dict = agg.into_dictionary();
    let _ = dict.insert(FRAME_KIND_KEY, FRAME_KIND_DELTA);
    let _ = dict.insert(FRAME_SEQ_KEY, frame_seq as i64);
    let _ = dict.insert(BASE_FRAME_SEQ_KEY, base_frame_seq as i64);

    if let Some(profiles) = delta.campaign().and_then(|s| s.campaignProfiles()) {
        let _ = dict.insert("campaign_profiles", &campaign_profiles_to_array(profiles));
    }

    if let Some(victory) = delta.campaign().and_then(|s| s.victory()) {
        let _ = dict.insert("victory", &victory_state_to_dict(victory));
    }

    if let Some(events) = delta.campaign().and_then(|s| s.commandEvents()) {
        let _ = dict.insert("command_events", &command_events_to_array(events));
    }

    if let Some(pending_forks) = delta.campaign().and_then(|s| s.pendingForks()) {
        let _ = dict.insert("pending_forks", &pending_forks_to_array(pending_forks));
    }

    if let Some(stance_axes) = delta.campaign().and_then(|s| s.stanceAxes()) {
        let _ = dict.insert("stance_axes", &stance_axes_to_array(stance_axes));
    }

    if let Some(voice_medium) = delta.campaign().and_then(|s| s.voiceMedium()) {
        let _ = dict.insert("voice_medium", &voice_medium_to_array(voice_medium));
    }

    if let Some(herds) = delta.subsistence().and_then(|s| s.herds()) {
        let _ = dict.insert("herds", &herds_to_array(herds));
    }

    if let Some(sedentarization) = delta.subsistence().and_then(|s| s.sedentarization()) {
        let _ = dict.insert(
            "sedentarization",
            &sedentarization_to_array(sedentarization),
        );
    }

    if let Some(forage_patches) = delta.subsistence().and_then(|s| s.foragePatches()) {
        let _ = dict.insert("forage_patches", &forage_patches_to_array(forage_patches));
    }

    if let Some(intensification) = delta
        .subsistence()
        .and_then(|s| s.intensificationKnowledge())
    {
        let _ = dict.insert(
            "intensification_knowledge",
            &intensification_knowledge_to_array(intensification),
        );
    }

    if let Some(demographics) = delta.population().and_then(|s| s.demographics()) {
        let _ = dict.insert("demographics", &demographics_to_array(demographics));
    }

    if let Some(discovered_sites) = delta.knowledge().and_then(|s| s.discoveredSites()) {
        let _ = dict.insert(
            "discovered_sites",
            &discovered_sites_to_array(discovered_sites),
        );
    }

    if let Some(definitions) = delta
        .knowledge()
        .and_then(|s| s.greatDiscoveryDefinitions())
    {
        let _ = dict.insert(
            "great_discovery_definitions",
            &great_discovery_definitions_to_array(definitions),
        );
    }

    if let Some(axis_bias) = delta.culture().and_then(|s| s.axisBias()) {
        let _ = dict.insert("axis_bias", &axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = delta.culture().and_then(|s| s.sentiment()) {
        let _ = dict.insert("sentiment", &sentiment_to_dict(sentiment));
    }

    if let Some(crisis) = delta.governance().and_then(|s| s.crisisTelemetry()) {
        let _ = dict.insert("crisis_telemetry", &crisis_telemetry_to_dict(crisis));
    }

    if let Some(crisis_overlay) = delta.governance().and_then(|s| s.crisisOverlay()) {
        let _ = dict.insert("crisis_overlay", &crisis_overlay_to_dict(crisis_overlay));
    }

    if let Some(great_discoveries) = delta.knowledge().and_then(|s| s.greatDiscoveries()) {
        let updates = great_discovery_states_to_array(great_discoveries);
        if !updates.is_empty() {
            let _ = dict.insert("great_discovery_updates", &updates);
        }
    }

    if let Some(great_progress) = delta.knowledge().and_then(|s| s.greatDiscoveryProgress()) {
        let updates = great_discovery_progress_states_to_array(great_progress);
        if !updates.is_empty() {
            let _ = dict.insert("great_discovery_progress_updates", &updates);
        }
    }

    if let Some(gd_telemetry) = delta.knowledge().and_then(|s| s.greatDiscoveryTelemetry()) {
        let _ = dict.insert(
            "great_discovery_telemetry",
            &great_discovery_telemetry_to_dict(gd_telemetry),
        );
    }

    if let Some(influencers) = delta.culture().and_then(|s| s.influencers()) {
        let _ = dict.insert("influencer_updates", &influencers_to_array(influencers));
    }

    let removed_influencers =
        u32_vector_to_packed_int32(delta.culture().and_then(|s| s.removedInfluencers()));
    if !removed_influencers.is_empty() {
        let _ = dict.insert("influencer_removed", &removed_influencers);
    }

    if let Some(ledger) = delta.governance().and_then(|s| s.corruption()) {
        let _ = dict.insert("corruption", &corruption_to_dict(ledger));
    }

    if let Some(populations) = delta.population().and_then(|s| s.populations()) {
        let _ = dict.insert("population_updates", &populations_to_array(populations));
    }

    let removed_populations =
        u64_vector_to_packed_int64(delta.population().and_then(|s| s.removedPopulations()));
    if !removed_populations.is_empty() {
        let _ = dict.insert("population_removed", &removed_populations);
    }

    if let Some(trade_links) = delta.economy().and_then(|s| s.tradeLinks()) {
        let _ = dict.insert("trade_link_updates", &trade_links_to_array(trade_links));
    }

    let removed_trade_links =
        u64_vector_to_packed_int64(delta.economy().and_then(|s| s.removedTradeLinks()));
    if !removed_trade_links.is_empty() {
        let _ = dict.insert("trade_link_removed", &removed_trade_links);
    }

    if let Some(power_nodes) = delta.governance().and_then(|s| s.power()) {
        let _ = dict.insert("power_updates", &power_nodes_to_array(power_nodes));
    }

    let removed_power =
        u64_vector_to_packed_int64(delta.governance().and_then(|s| s.removedPower()));
    if !removed_power.is_empty() {
        let _ = dict.insert("power_removed", &removed_power);
    }

    if let Some(power_metrics) = delta.governance().and_then(|s| s.powerMetrics()) {
        let _ = dict.insert("power_metrics", &power_metrics_to_dict(power_metrics));
    }

    if let Some(tiles) = delta.map().and_then(|s| s.tiles()) {
        let _ = dict.insert("tile_updates", &tiles_to_array(tiles));
    }

    let removed_tiles = u64_vector_to_packed_int64(delta.map().and_then(|s| s.removedTiles()));
    if !removed_tiles.is_empty() {
        let _ = dict.insert("tile_removed", &removed_tiles);
    }

    if let Some(generations) = delta.population().and_then(|s| s.generations()) {
        let _ = dict.insert("generation_updates", &generations_to_array(generations));
    }

    let removed_generations =
        u16_vector_to_packed_int32(delta.population().and_then(|s| s.removedGenerations()));
    if !removed_generations.is_empty() {
        let _ = dict.insert("generation_removed", &removed_generations);
    }

    if let Some(layers) = delta.culture().and_then(|s| s.cultureLayers()) {
        let _ = dict.insert("culture_layer_updates", &culture_layers_to_array(layers));
    }

    let removed_layers =
        u32_vector_to_packed_int32(delta.culture().and_then(|s| s.removedCultureLayers()));
    if !removed_layers.is_empty() {
        let _ = dict.insert("culture_layer_removed", &removed_layers);
    }

    if let Some(tensions) = delta.culture().and_then(|s| s.cultureTensions()) {
        let _ = dict.insert("culture_tensions", &culture_tensions_to_array(tensions));
    }

    if let Some(progress) = delta.knowledge().and_then(|s| s.discoveryProgress()) {
        let _ = dict.insert(
            "discovery_progress_updates",
            &discovery_progress_to_array(progress),
        );
    }

    Some((dict, rasters))
}
