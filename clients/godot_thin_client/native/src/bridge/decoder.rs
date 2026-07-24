//! `SnapshotDecoder` -- the GDScript entry point that turns a FlatBuffers envelope
//! into the snapshot `Dictionary` the client renders from.

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::campaign::{
    command_events_to_array, pending_forks_to_array, stance_axes_to_array, victory_state_to_dict,
    voice_medium_to_array,
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
use crate::snapshot::delta::DeltaAggregator;
use crate::snapshot::snapshot_to_dict;

#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct SnapshotDecoder;

#[godot_api]
impl SnapshotDecoder {
    #[func]
    pub fn decode_snapshot(&self, data: PackedByteArray) -> VarDictionary {
        decode_snapshot(&data).unwrap_or_default()
    }

    #[func]
    pub fn decode_delta(&self, data: PackedByteArray) -> VarDictionary {
        decode_delta(&data).unwrap_or_default()
    }
}

fn decode_snapshot(data: &PackedByteArray) -> Option<VarDictionary> {
    if data.is_empty() {
        return None;
    }
    let bytes = data.as_slice();
    let envelope = fb::root_as_envelope(bytes).ok()?;
    match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => envelope.payload_as_snapshot().map(snapshot_to_dict),
        fb::SnapshotPayload::delta => decode_delta(data),
        _ => None,
    }
}

fn decode_delta(data: &PackedByteArray) -> Option<VarDictionary> {
    if data.is_empty() {
        return None;
    }
    let bytes = data.as_slice();
    let envelope = fb::root_as_envelope(bytes).ok()?;
    if envelope.payload_type() != fb::SnapshotPayload::delta {
        return None;
    }
    let delta = envelope.payload_as_delta()?;
    // For now, render deltas by synthesizing a snapshot-sized dictionary where only
    // updated tiles affect the overlays. This keeps the UI responsive while we pump
    // full snapshots on the same stream.
    let mut agg = DeltaAggregator::default();
    if let Some(header) = delta.header() {
        agg.tick = header.tick();
        agg.wrap_horizontal = header.wrapHorizontal();
        agg.world_epoch = header.worldEpoch();
        if let Some(build) = header.serverBuild() {
            agg.server_build = build.to_string();
        }
    }
    if let Some(tiles) = delta.map().and_then(|s| s.tiles()) {
        for tile in tiles {
            agg.update_tile(tile.x(), tile.y(), tile.temperature());
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
    if let Some(raster) = delta.vision().and_then(|s| s.fogRaster()) {
        agg.apply_fog_raster(raster);
    }
    if let Some(raster) = delta.vision().and_then(|s| s.visibilityRaster()) {
        agg.apply_visibility_raster(raster);
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
    let mut dict = agg.into_dictionary();

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

    Some(dict)
}
