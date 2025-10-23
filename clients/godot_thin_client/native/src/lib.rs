use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use std::collections::{BTreeSet, HashMap};

fn snapshot_dict(
    tick: u64,
    width: u32,
    height: u32,
    logistics_overlay: &[f32],
    terrain: Option<&[u16]>,
    terrain_tags: Option<&[u16]>,
) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("turn", tick as i64);

    let mut grid = Dictionary::new();
    let _ = grid.insert("width", width as i64);
    let _ = grid.insert("height", height as i64);
    let _ = dict.insert("grid", grid);

    let mut logistics = PackedFloat32Array::new();
    let size = (width as usize).saturating_mul(height as usize);
    logistics.resize(size);
    if size > 0 {
        let slice = logistics.as_mut_slice();
        let count = logistics_overlay.len().min(slice.len());
        slice[..count].copy_from_slice(&logistics_overlay[..count]);
    }

    let mut contrast = PackedFloat32Array::new();
    contrast.resize(size);
    if size > 0 {
        let slice = contrast.as_mut_slice();
        let count = logistics_overlay.len().min(slice.len());
        if count > 0 {
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for &value in &logistics_overlay[..count] {
                if value.is_finite() {
                    min = min.min(value);
                    max = max.max(value);
                }
            }
            if min.is_finite() && max.is_finite() && (max - min).abs() > f32::EPSILON {
                let range = max - min;
                for i in 0..count {
                    let normalized = (logistics_overlay[i] - min) / range;
                    slice[i] = normalized * (1.0 - normalized);
                }
            }
        }
    }

    let mut overlays = Dictionary::new();
    let _ = overlays.insert("logistics", logistics);
    let _ = overlays.insert("contrast", contrast);

    if let Some(terrain_data) = terrain {
        let mut terrain_array = PackedInt32Array::new();
        terrain_array.resize(size);
        if size > 0 {
            let slice = terrain_array.as_mut_slice();
            let count = terrain_data.len().min(slice.len());
            for i in 0..count {
                slice[i] = terrain_data[i] as i32;
            }
        }
        let _ = overlays.insert("terrain", terrain_array);

        if let Some(tag_data) = terrain_tags {
            let mut tag_array = PackedInt32Array::new();
            tag_array.resize(size);
            if size > 0 {
                let slice = tag_array.as_mut_slice();
                let count = tag_data.len().min(slice.len());
                for i in 0..count {
                    slice[i] = tag_data[i] as i32;
                }
            }
            let _ = overlays.insert("terrain_tags", tag_array);
        }

        let mut palette = Dictionary::new();
        let mut seen = BTreeSet::new();
        for &value in terrain_data {
            if seen.insert(value) {
                let _ = palette.insert(value as i64, terrain_label_from_id(value));
            }
        }
        let _ = overlays.insert("terrain_palette", palette);

        let mut tag_labels = Dictionary::new();
        for (mask, label) in TERRAIN_TAG_LABELS.iter() {
            let _ = tag_labels.insert(*mask as i64, *label);
        }
        let _ = overlays.insert("terrain_tag_labels", tag_labels);
    }

    let _ = dict.insert("overlays", overlays);

    let _ = dict.insert("units", VariantArray::new());
    let _ = dict.insert("orders", VariantArray::new());

    dict
}
#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct SnapshotDecoder;

#[godot_api]
impl SnapshotDecoder {
    #[func]
    pub fn decode_snapshot(&self, data: PackedByteArray) -> Dictionary {
        decode_snapshot(&data).unwrap_or_else(Dictionary::new)
    }

    #[func]
    pub fn decode_delta(&self, data: PackedByteArray) -> Dictionary {
        decode_delta(&data).unwrap_or_else(Dictionary::new)
    }
}

fn decode_snapshot(data: &PackedByteArray) -> Option<Dictionary> {
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

fn decode_delta(data: &PackedByteArray) -> Option<Dictionary> {
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
    }
    if let Some(tiles) = delta.tiles() {
        for tile in tiles {
            agg.update_tile(tile.x(), tile.y(), tile.temperature());
        }
    }
    if let Some(layer) = delta.terrainOverlay() {
        agg.apply_terrain_overlay(layer);
    }
    let mut dict = agg.into_dictionary();

    if let Some(axis_bias) = delta.axisBias() {
        let _ = dict.insert("axis_bias", axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = delta.sentiment() {
        let _ = dict.insert("sentiment", sentiment_to_dict(sentiment));
    }

    if let Some(influencers) = delta.influencers() {
        let _ = dict.insert("influencer_updates", influencers_to_array(influencers));
    }

    let removed_influencers = u32_vector_to_packed_int32(delta.removedInfluencers());
    if removed_influencers.len() > 0 {
        let _ = dict.insert("influencer_removed", removed_influencers);
    }

    if let Some(ledger) = delta.corruption() {
        let _ = dict.insert("corruption", corruption_to_dict(ledger));
    }

    if let Some(populations) = delta.populations() {
        let _ = dict.insert("population_updates", populations_to_array(populations));
    }

    let removed_populations = u64_vector_to_packed_int64(delta.removedPopulations());
    if removed_populations.len() > 0 {
        let _ = dict.insert("population_removed", removed_populations);
    }

    if let Some(trade_links) = delta.tradeLinks() {
        let _ = dict.insert("trade_link_updates", trade_links_to_array(trade_links));
    }

    let removed_trade_links = u64_vector_to_packed_int64(delta.removedTradeLinks());
    if removed_trade_links.len() > 0 {
        let _ = dict.insert("trade_link_removed", removed_trade_links);
    }

    if let Some(tiles) = delta.tiles() {
        let _ = dict.insert("tile_updates", tiles_to_array(tiles));
    }

    let removed_tiles = u64_vector_to_packed_int64(delta.removedTiles());
    if removed_tiles.len() > 0 {
        let _ = dict.insert("tile_removed", removed_tiles);
    }

    if let Some(generations) = delta.generations() {
        let _ = dict.insert("generation_updates", generations_to_array(generations));
    }

    let removed_generations = u16_vector_to_packed_int32(delta.removedGenerations());
    if removed_generations.len() > 0 {
        let _ = dict.insert("generation_removed", removed_generations);
    }

    if let Some(layers) = delta.cultureLayers() {
        let _ = dict.insert("culture_layer_updates", culture_layers_to_array(layers));
    }

    let removed_layers = u32_vector_to_packed_int32(delta.removedCultureLayers());
    if removed_layers.len() > 0 {
        let _ = dict.insert("culture_layer_removed", removed_layers);
    }

    if let Some(tensions) = delta.cultureTensions() {
        let _ = dict.insert("culture_tensions", culture_tensions_to_array(tensions));
    }

    if let Some(progress) = delta.discoveryProgress() {
        let _ = dict.insert("discovery_progress_updates", discovery_progress_to_array(progress));
    }

    Some(dict)
}

#[derive(Default)]
struct DeltaAggregator {
    tick: u64,
    width: u32,
    height: u32,
    tile_updates: HashMap<(u32, u32), f32>,
    terrain_width: u32,
    terrain_height: u32,
    terrain_types: Vec<u16>,
    terrain_tags: Vec<u16>,
}

impl DeltaAggregator {
    fn update_tile(&mut self, x: u32, y: u32, temperature: i64) {
        self.width = self.width.max(x + 1);
        self.height = self.height.max(y + 1);
        self.tile_updates
            .insert((x, y), fixed64_to_f32(temperature));
    }

    fn apply_terrain_overlay(&mut self, overlay: fb::TerrainOverlay<'_>) {
        self.terrain_width = overlay.width();
        self.terrain_height = overlay.height();
        let count = (self.terrain_width as usize)
            .saturating_mul(self.terrain_height as usize)
            .max(1);
        self.terrain_types.resize(count, 0);
        self.terrain_tags.resize(count, 0);
        if let Some(samples) = overlay.samples() {
            for (idx, sample) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.terrain_types[idx] = sample.terrain().0;
                self.terrain_tags[idx] = sample.tags();
            }
        }
    }

    fn into_dictionary(self) -> Dictionary {
        let mut final_width = self.terrain_width.max(self.width);
        let mut final_height = self.terrain_height.max(self.height);
        if final_width == 0 || final_height == 0 {
            final_width = final_width.max(1);
            final_height = final_height.max(1);
        }
        let total = (final_width as usize)
            .saturating_mul(final_height as usize)
            .max(1);
        let mut logistics = vec![0.0f32; total];
        for ((x, y), value) in self.tile_updates {
            if x >= final_width || y >= final_height {
                continue;
            }
            let idx = (y as usize) * (final_width as usize) + x as usize;
            logistics[idx] = value;
        }
        normalize_overlay(&mut logistics);

        let terrain_ref = if !self.terrain_types.is_empty() {
            Some(self.terrain_types)
        } else {
            None
        };
        let tags_ref = if !self.terrain_tags.is_empty() {
            Some(self.terrain_tags)
        } else {
            None
        };

        snapshot_dict(
            self.tick,
            final_width,
            final_height,
            &logistics,
            terrain_ref.as_ref().map(|v| v.as_slice()),
            tags_ref.as_ref().map(|v| v.as_slice()),
        )
    }
}

const TERRAIN_TAG_LABELS: &[(u16, &str)] = &[
    (1 << 0, "Water"),
    (1 << 1, "Freshwater"),
    (1 << 2, "Coastal"),
    (1 << 3, "Wetland"),
    (1 << 4, "Fertile"),
    (1 << 5, "Arid"),
    (1 << 6, "Polar"),
    (1 << 7, "Highland"),
    (1 << 8, "Volcanic"),
    (1 << 9, "Hazardous"),
    (1 << 10, "Subsurface"),
    (1 << 11, "Hydrothermal"),
];

const CULTURE_AXIS_KEYS: [&str; 15] = [
    "PassiveAggressive",
    "OpenClosed",
    "CollectivistIndividualist",
    "TraditionalistRevisionist",
    "HierarchicalEgalitarian",
    "SyncreticPurist",
    "AsceticIndulgent",
    "PragmaticIdealistic",
    "RationalistMystical",
    "ExpansionistInsular",
    "AdaptiveStubborn",
    "HonorBoundOpportunistic",
    "MeritOrientedLineageOriented",
    "SecularDevout",
    "PluralisticMonocultural",
];

const CULTURE_AXIS_LABELS: [&str; 15] = [
    "Passive ↔ Aggressive",
    "Open ↔ Closed",
    "Collectivist ↔ Individualist",
    "Traditionalist ↔ Revisionist",
    "Hierarchical ↔ Egalitarian",
    "Syncretic ↔ Purist",
    "Ascetic ↔ Indulgent",
    "Pragmatic ↔ Idealistic",
    "Rationalist ↔ Mystical",
    "Expansionist ↔ Insular",
    "Adaptive ↔ Stubborn",
    "Honor-Bound ↔ Opportunistic",
    "Merit ↔ Lineage",
    "Secular ↔ Devout",
    "Pluralistic ↔ Monocultural",
];

const CULTURE_SCOPE_LABELS: [&str; 3] = ["Global", "Regional", "Local"];
const CULTURE_TENSION_LABELS: [&str; 3] = ["Drift Warning", "Assimilation Push", "Schism Risk"];

fn snapshot_to_dict(snapshot: fb::WorldSnapshot<'_>) -> Dictionary {
    let header = snapshot.header().unwrap();
    let mut logistics = HashMap::new();
    let mut width = 0u32;
    let mut height = 0u32;
    if let Some(tiles) = snapshot.tiles() {
        for tile in tiles {
            let x = tile.x();
            let y = tile.y();
            width = width.max(x + 1);
            height = height.max(y + 1);
            logistics.insert((x, y), fixed64_to_f32(tile.temperature()));
        }
    }

    let mut terrain_width = 0u32;
    let mut terrain_height = 0u32;
    let mut terrain_samples: Vec<(u16, u16)> = Vec::new();
    if let Some(layer) = snapshot.terrainOverlay() {
        terrain_width = layer.width();
        terrain_height = layer.height();
        if let Some(samples) = layer.samples() {
            terrain_samples.reserve(samples.len());
            for sample in samples {
                terrain_samples.push((sample.terrain().0, sample.tags()));
            }
        }
    }

    let final_width = width.max(terrain_width).max(1);
    let final_height = height.max(terrain_height).max(1);
    let total = (final_width as usize)
        .saturating_mul(final_height as usize)
        .max(1);

    let mut logistics_vec = vec![0.0f32; total];
    for ((x, y), value) in logistics.into_iter() {
        if x >= final_width || y >= final_height {
            continue;
        }
        let idx = (y as usize) * (final_width as usize) + x as usize;
        logistics_vec[idx] = value;
    }
    normalize_overlay(&mut logistics_vec);

    let mut terrain_vec: Vec<u16> = Vec::new();
    let mut tag_vec: Vec<u16> = Vec::new();
    if terrain_width > 0 && terrain_height > 0 && !terrain_samples.is_empty() {
        terrain_vec = vec![0u16; total];
        tag_vec = vec![0u16; total];
        for y in 0..terrain_height {
            for x in 0..terrain_width {
                let src_idx = (y as usize) * (terrain_width as usize) + x as usize;
                if src_idx >= terrain_samples.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                let (terrain, tags) = terrain_samples[src_idx];
                terrain_vec[dst_idx] = terrain;
                tag_vec[dst_idx] = tags;
            }
        }
    }

    let mut dict = snapshot_dict(
        header.tick(),
        final_width,
        final_height,
        &logistics_vec,
        if terrain_vec.is_empty() {
            None
        } else {
            Some(terrain_vec.as_slice())
        },
        if tag_vec.is_empty() {
            None
        } else {
            Some(tag_vec.as_slice())
        },
    );

    if let Some(axis_bias) = snapshot.axisBias() {
        let _ = dict.insert("axis_bias", axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = snapshot.sentiment() {
        let _ = dict.insert("sentiment", sentiment_to_dict(sentiment));
    }

    if let Some(influencers) = snapshot.influencers() {
        let _ = dict.insert("influencers", influencers_to_array(influencers));
    }

    if let Some(ledger) = snapshot.corruption() {
        let _ = dict.insert("corruption", corruption_to_dict(ledger));
    }

    if let Some(populations) = snapshot.populations() {
        let _ = dict.insert("populations", populations_to_array(populations));
    }

    if let Some(trade_links) = snapshot.tradeLinks() {
        let _ = dict.insert("trade_links", trade_links_to_array(trade_links));
    }

    if let Some(tiles_fb) = snapshot.tiles() {
        let _ = dict.insert("tiles", tiles_to_array(tiles_fb));
    }

    if let Some(generations) = snapshot.generations() {
        let _ = dict.insert("generations", generations_to_array(generations));
    }

    if let Some(layers) = snapshot.cultureLayers() {
        let _ = dict.insert("culture_layers", culture_layers_to_array(layers));
    }

    if let Some(tensions) = snapshot.cultureTensions() {
        let _ = dict.insert("culture_tensions", culture_tensions_to_array(tensions));
    }

    if let Some(progress) = snapshot.discoveryProgress() {
        let _ = dict.insert("discovery_progress", discovery_progress_to_array(progress));
    }

    dict
}

fn terrain_label_from_id(id: u16) -> &'static str {
    match id {
        0 => "Deep Ocean",
        1 => "Continental Shelf",
        2 => "Inland Sea",
        3 => "Coral Shelf",
        4 => "Hydrothermal Vent Field",
        5 => "Tidal Flat",
        6 => "River Delta",
        7 => "Mangrove Swamp",
        8 => "Freshwater Marsh",
        9 => "Floodplain",
        10 => "Alluvial Plain",
        11 => "Prairie Steppe",
        12 => "Mixed Woodland",
        13 => "Boreal Taiga",
        14 => "Peatland/Heath",
        15 => "Hot Desert Erg",
        16 => "Rocky Reg Desert",
        17 => "Semi-Arid Scrub",
        18 => "Salt Flat",
        19 => "Oasis Basin",
        20 => "Tundra",
        21 => "Periglacial Steppe",
        22 => "Glacier",
        23 => "Seasonal Snowfield",
        24 => "Rolling Hills",
        25 => "High Plateau",
        26 => "Alpine Mountain",
        27 => "Karst Highland",
        28 => "Canyon Badlands",
        29 => "Active Volcano Slope",
        30 => "Basaltic Lava Field",
        31 => "Ash Plain",
        32 => "Fumarole Basin",
        33 => "Impact Crater Field",
        34 => "Karst Cavern Mouth",
        35 => "Sinkhole Field",
        36 => "Aquifer Ceiling",
        _ => "Unknown",
    }
}

fn fixed64_to_f32(value: i64) -> f32 {
    (value as f32) / 1_000_000.0
}

fn fixed64_to_f64(value: i64) -> f64 {
    (value as f64) / 1_000_000.0
}

fn normalize_overlay(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &v in values.iter() {
        if !v.is_finite() {
            continue;
        }
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if !min.is_finite() || !max.is_finite() || (max - min).abs() < f32::EPSILON {
        values.fill(0.0);
        return;
    }
    let range = max - min;
    for v in values.iter_mut() {
        if v.is_finite() {
            *v = ((*v - min) / range).clamp(0.0, 1.0);
        } else {
            *v = 0.0;
        }
    }
}

fn axis_bias_to_dict(axis: fb::AxisBiasState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("knowledge", fixed64_to_f64(axis.knowledge()));
    let _ = dict.insert("trust", fixed64_to_f64(axis.trust()));
    let _ = dict.insert("equity", fixed64_to_f64(axis.equity()));
    let _ = dict.insert("agency", fixed64_to_f64(axis.agency()));
    dict
}

fn sentiment_driver_category_label(category: fb::SentimentDriverCategory) -> &'static str {
    match category {
        fb::SentimentDriverCategory::Policy => "Policy",
        fb::SentimentDriverCategory::Incident => "Incident",
        fb::SentimentDriverCategory::Influencer => "Influencer",
        _ => "Unknown",
    }
}

fn sentiment_axis_to_dict(axis: fb::SentimentAxisTelemetry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("policy", fixed64_to_f64(axis.policy()));
    let _ = dict.insert("incidents", fixed64_to_f64(axis.incidents()));
    let _ = dict.insert("influencers", fixed64_to_f64(axis.influencers()));
    let _ = dict.insert("total", fixed64_to_f64(axis.total()));

    let mut drivers = VariantArray::new();
    if let Some(list) = axis.drivers() {
        for driver in list {
            let mut driver_dict = Dictionary::new();
            let _ = driver_dict.insert(
                "category",
                sentiment_driver_category_label(driver.category()),
            );
            let _ = driver_dict.insert("label", driver.label().unwrap_or_default());
            let _ = driver_dict.insert("value", fixed64_to_f64(driver.value()));
            let _ = driver_dict.insert("weight", fixed64_to_f64(driver.weight()));
            let variant = driver_dict.to_variant();
            drivers.push(&variant);
        }
    }
    let _ = dict.insert("drivers", drivers);
    dict
}

fn sentiment_to_dict(sentiment: fb::SentimentTelemetryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Some(axis) = sentiment.knowledge() {
        let _ = dict.insert("knowledge", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.trust() {
        let _ = dict.insert("trust", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.equity() {
        let _ = dict.insert("equity", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.agency() {
        let _ = dict.insert("agency", sentiment_axis_to_dict(axis));
    }
    dict
}

fn influence_scope_label(scope: fb::InfluenceScopeKind) -> &'static str {
    match scope {
        fb::InfluenceScopeKind::Local => "Local",
        fb::InfluenceScopeKind::Regional => "Regional",
        fb::InfluenceScopeKind::Global => "Global",
        fb::InfluenceScopeKind::Generation => "Generation",
        _ => "Unknown",
    }
}

fn influence_lifecycle_label(lifecycle: fb::InfluenceLifecycle) -> &'static str {
    match lifecycle {
        fb::InfluenceLifecycle::Potential => "Potential",
        fb::InfluenceLifecycle::Active => "Active",
        fb::InfluenceLifecycle::Dormant => "Dormant",
        _ => "Unknown",
    }
}

fn influence_domain_labels(mask: u32) -> PackedStringArray {
    let mut labels = PackedStringArray::new();
    for value in 0..=4 {
        let bit = 1u32 << value;
        if mask & bit == 0 {
            continue;
        }
        let label = match value {
            0 => "Sentiment",
            1 => "Discovery",
            2 => "Logistics",
            3 => "Production",
            4 => "Humanitarian",
            _ => continue,
        };
        let gstring = GString::from(label);
        labels.push(&gstring);
    }
    labels
}

fn audience_generations_to_array(
    generations: Option<flatbuffers::Vector<'_, u16>>,
) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(list) = generations {
        array.resize(list.len());
        let slice = array.as_mut_slice();
        for (index, value) in list.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn influencer_to_dict(state: fb::InfluentialIndividualState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("influence", fixed64_to_f64(state.influence()));
    let _ = dict.insert("growth_rate", fixed64_to_f64(state.growthRate()));
    let _ = dict.insert("baseline_growth", fixed64_to_f64(state.baselineGrowth()));
    let _ = dict.insert("notoriety", fixed64_to_f64(state.notoriety()));
    let _ = dict.insert(
        "sentiment_knowledge",
        fixed64_to_f64(state.sentimentKnowledge()),
    );
    let _ = dict.insert("sentiment_trust", fixed64_to_f64(state.sentimentTrust()));
    let _ = dict.insert("sentiment_equity", fixed64_to_f64(state.sentimentEquity()));
    let _ = dict.insert("sentiment_agency", fixed64_to_f64(state.sentimentAgency()));
    let _ = dict.insert(
        "sentiment_weight_knowledge",
        fixed64_to_f64(state.sentimentWeightKnowledge()),
    );
    let _ = dict.insert(
        "sentiment_weight_trust",
        fixed64_to_f64(state.sentimentWeightTrust()),
    );
    let _ = dict.insert(
        "sentiment_weight_equity",
        fixed64_to_f64(state.sentimentWeightEquity()),
    );
    let _ = dict.insert(
        "sentiment_weight_agency",
        fixed64_to_f64(state.sentimentWeightAgency()),
    );
    let _ = dict.insert("logistics_bonus", fixed64_to_f64(state.logisticsBonus()));
    let _ = dict.insert("morale_bonus", fixed64_to_f64(state.moraleBonus()));
    let _ = dict.insert("power_bonus", fixed64_to_f64(state.powerBonus()));
    let _ = dict.insert("logistics_weight", fixed64_to_f64(state.logisticsWeight()));
    let _ = dict.insert("morale_weight", fixed64_to_f64(state.moraleWeight()));
    let _ = dict.insert("power_weight", fixed64_to_f64(state.powerWeight()));
    let _ = dict.insert("support_charge", fixed64_to_f64(state.supportCharge()));
    let _ = dict.insert(
        "suppress_pressure",
        fixed64_to_f64(state.suppressPressure()),
    );
    let domains_mask = state.domains();
    let _ = dict.insert("domains_mask", domains_mask as i64);
    let _ = dict.insert("domains", influence_domain_labels(domains_mask));
    let _ = dict.insert("scope", influence_scope_label(state.scope()));
    let generation_scope = state.generationScope();
    if generation_scope != u16::MAX {
        let _ = dict.insert("generation_scope", generation_scope as i64);
    }
    let _ = dict.insert("supported", state.supported());
    let _ = dict.insert("suppressed", state.suppressed());
    let _ = dict.insert("lifecycle", influence_lifecycle_label(state.lifecycle()));
    let _ = dict.insert("coherence", fixed64_to_f64(state.coherence()));
    let _ = dict.insert("ticks_in_status", state.ticksInStatus() as i64);
    let audience = audience_generations_to_array(state.audienceGenerations());
    let _ = dict.insert("audience_generations", audience);
    let _ = dict.insert("support_popular", fixed64_to_f64(state.supportPopular()));
    let _ = dict.insert("support_peer", fixed64_to_f64(state.supportPeer()));
    let _ = dict.insert(
        "support_institutional",
        fixed64_to_f64(state.supportInstitutional()),
    );
    let _ = dict.insert(
        "support_humanitarian",
        fixed64_to_f64(state.supportHumanitarian()),
    );
    let _ = dict.insert("weight_popular", fixed64_to_f64(state.weightPopular()));
    let _ = dict.insert("weight_peer", fixed64_to_f64(state.weightPeer()));
    let _ = dict.insert(
        "weight_institutional",
        fixed64_to_f64(state.weightInstitutional()),
    );
    let _ = dict.insert(
        "weight_humanitarian",
        fixed64_to_f64(state.weightHumanitarian()),
    );
    if let Some(resonance) = state.cultureResonance() {
        let array = culture_resonance_to_array(resonance);
        let _ = dict.insert("culture_resonance", array);
    }
    dict
}

fn influencers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::InfluentialIndividualState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = influencer_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_resonance_entry_to_dict(entry: fb::InfluencerCultureResonanceEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("weight", fixed64_to_f64(entry.weight()));
    let _ = dict.insert("output", fixed64_to_f64(entry.output()));
    dict
}

fn culture_resonance_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'_>>,
    >,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in list {
        let dict = culture_resonance_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn corruption_subsystem_label(subsystem: fb::CorruptionSubsystem) -> &'static str {
    match subsystem {
        fb::CorruptionSubsystem::Logistics => "Logistics",
        fb::CorruptionSubsystem::Trade => "Trade",
        fb::CorruptionSubsystem::Military => "Military",
        fb::CorruptionSubsystem::Governance => "Governance",
        _ => "Unknown",
    }
}

fn corruption_entry_to_dict(entry: fb::CorruptionEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("subsystem", corruption_subsystem_label(entry.subsystem()));
    let _ = dict.insert("intensity", fixed64_to_f64(entry.intensity()));
    let _ = dict.insert("incident_id", entry.incidentId() as i64);
    let _ = dict.insert("exposure_timer", entry.exposureTimer() as i64);
    let _ = dict.insert("restitution_window", entry.restitutionWindow() as i64);
    let _ = dict.insert("last_update_tick", entry.lastUpdateTick() as i64);
    dict
}

fn corruption_to_dict(ledger: fb::CorruptionLedger<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut entries = VariantArray::new();
    if let Some(list) = ledger.entries() {
        for entry in list {
            let dict = corruption_entry_to_dict(entry);
            let variant = dict.to_variant();
            entries.push(&variant);
        }
    }
    let _ = dict.insert("entries", entries);
    let _ = dict.insert(
        "reputation_modifier",
        fixed64_to_f64(ledger.reputationModifier()),
    );
    let _ = dict.insert("audit_capacity", ledger.auditCapacity() as i64);
    dict
}

fn population_to_dict(cohort: fb::PopulationCohortState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", cohort.entity() as i64);
    let _ = dict.insert("home", cohort.home() as i64);
    let _ = dict.insert("size", cohort.size() as i64);
    let _ = dict.insert("morale", fixed64_to_f64(cohort.morale()));
    let _ = dict.insert("generation", cohort.generation() as i64);
    let _ = dict.insert("faction", cohort.faction() as i64);

    if let Some(fragments) = cohort.knowledgeFragments() {
        let mut array = VariantArray::new();
        for fragment in fragments {
            let dict = fragment_to_dict(fragment);
            array.push(&dict.to_variant());
        }
        let _ = dict.insert("knowledge_fragments", array);
    }

    dict
}

fn populations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::PopulationCohortState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for cohort in list {
        let dict = population_to_dict(cohort);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn fragment_to_dict(fragment: fb::KnownTechFragment<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("discovery", fragment.discoveryId() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(fragment.progress()));
    let _ = dict.insert("progress_raw", fragment.progress());
    let _ = dict.insert("fidelity", fixed64_to_f64(fragment.fidelity()));
    dict
}

fn discovery_progress_entry_to_dict(entry: fb::DiscoveryProgressEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("faction", entry.faction() as i64);
    let _ = dict.insert("discovery", entry.discovery() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(entry.progress()));
    let _ = dict.insert("progress_raw", entry.progress());
    dict
}

fn discovery_progress_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::DiscoveryProgressEntry<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in list {
        let dict = discovery_progress_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn tile_to_dict(tile: fb::TileState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", tile.entity() as i64);
    let _ = dict.insert("x", tile.x() as i64);
    let _ = dict.insert("y", tile.y() as i64);
    let _ = dict.insert("element", tile.element() as i64);
    let _ = dict.insert("mass", fixed64_to_f64(tile.mass()));
    let _ = dict.insert("temperature", fixed64_to_f64(tile.temperature()));
    let _ = dict.insert("terrain", tile.terrain().0 as i64);
    let _ = dict.insert("terrain_tags", tile.terrainTags() as i64);
    dict
}

fn tiles_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TileState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for tile in list {
        let dict = tile_to_dict(tile);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn trade_link_to_dict(link: fb::TradeLinkState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", link.entity() as i64);
    let _ = dict.insert("from_faction", link.fromFaction() as i64);
    let _ = dict.insert("to_faction", link.toFaction() as i64);
    let _ = dict.insert("throughput", fixed64_to_f64(link.throughput()));
    let _ = dict.insert("tariff", fixed64_to_f64(link.tariff()));
    let _ = dict.insert("from_tile", link.fromTile() as i64);
    let _ = dict.insert("to_tile", link.toTile() as i64);

    if let Some(knowledge) = link.knowledge() {
        let mut knowledge_dict = Dictionary::new();
        let _ = knowledge_dict.insert("openness", fixed64_to_f64(knowledge.openness()));
        let _ = knowledge_dict.insert("openness_raw", knowledge.openness());
        let _ = knowledge_dict.insert("leak_timer", knowledge.leakTimer() as i64);
        let _ = knowledge_dict.insert("last_discovery", knowledge.lastDiscovery() as i64);
        let _ = knowledge_dict.insert("decay", fixed64_to_f64(knowledge.decay()));
        let _ = knowledge_dict.insert("decay_raw", knowledge.decay());
        let _ = dict.insert("knowledge", knowledge_dict);
    }

    dict
}

fn trade_links_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TradeLinkState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for link in list {
        let dict = trade_link_to_dict(link);
        array.push(&dict.to_variant());
    }
    array
}

fn generation_to_dict(state: fb::GenerationState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("bias_knowledge", fixed64_to_f64(state.biasKnowledge()));
    let _ = dict.insert("bias_trust", fixed64_to_f64(state.biasTrust()));
    let _ = dict.insert("bias_equity", fixed64_to_f64(state.biasEquity()));
    let _ = dict.insert("bias_agency", fixed64_to_f64(state.biasAgency()));
    dict
}

fn culture_layer_to_dict(layer: fb::CultureLayerState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let id = layer.id();
    let scope = layer.scope();
    let scope_label = culture_scope_to_label(scope);
    let owner = layer.owner();
    let parent = layer.parent();
    let baseline = layer.divergence();
    let soft = layer.softThreshold();
    let hard = layer.hardThreshold();
    let _ = dict.insert("id", id as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", scope_label);
    let _ = dict.insert("owner", format!("{owner:016X}"));
    if owner <= i64::MAX as u64 {
        let _ = dict.insert("owner_value", owner as i64);
    }
    let _ = dict.insert("parent", parent as i64);
    let _ = dict.insert("divergence", fixed64_to_f64(baseline));
    let _ = dict.insert("soft_threshold", fixed64_to_f64(soft));
    let _ = dict.insert("hard_threshold", fixed64_to_f64(hard));
    let _ = dict.insert("ticks_above_soft", layer.ticksAboveSoft() as i64);
    let _ = dict.insert("ticks_above_hard", layer.ticksAboveHard() as i64);
    let _ = dict.insert("last_updated_tick", layer.lastUpdatedTick() as i64);

    let mut traits_array = VariantArray::new();
    if let Some(traits) = layer.traits() {
        for trait_entry in traits {
            let trait_dict = culture_trait_to_dict(trait_entry);
            traits_array.push(&trait_dict.to_variant());
        }
    }
    let _ = dict.insert("traits", traits_array);

    dict
}

fn culture_trait_to_dict(entry: fb::CultureTraitEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("baseline", fixed64_to_f64(entry.baseline()));
    let _ = dict.insert("modifier", fixed64_to_f64(entry.modifier()));
    let _ = dict.insert("value", fixed64_to_f64(entry.value()));
    dict
}

fn culture_tension_to_dict(state: fb::CultureTensionState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let scope = state.scope();
    let kind = state.kind();
    let _ = dict.insert("layer_id", state.layerId() as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", culture_scope_to_label(scope));
    let _ = dict.insert("kind", culture_tension_to_key(kind));
    let _ = dict.insert("kind_label", culture_tension_to_label(kind));
    let _ = dict.insert("severity", fixed64_to_f64(state.severity()));
    let _ = dict.insert("timer", state.timer() as i64);
    dict
}

fn generations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GenerationState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = generation_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_layers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureLayerState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for layer in list {
        let dict = culture_layer_to_dict(layer);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_tensions_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureTensionState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for tension in list {
        let dict = culture_tension_to_dict(tension);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_scope_to_key(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => "Global",
        fb::CultureLayerScope::Regional => "Regional",
        fb::CultureLayerScope::Local => "Local",
        _ => "Unknown",
    }
}

fn culture_scope_to_label(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => CULTURE_SCOPE_LABELS[0],
        fb::CultureLayerScope::Regional => CULTURE_SCOPE_LABELS[1],
        fb::CultureLayerScope::Local => CULTURE_SCOPE_LABELS[2],
        _ => "Unknown",
    }
}

fn culture_axis_to_key(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_KEYS.get(idx).copied().unwrap_or("Trait")
}

fn culture_axis_to_label(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_LABELS.get(idx).copied().unwrap_or("Trait")
}

fn culture_tension_to_key(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => "DriftWarning",
        fb::CultureTensionKind::AssimilationPush => "AssimilationPush",
        fb::CultureTensionKind::SchismRisk => "SchismRisk",
        _ => "Unknown",
    }
}

fn culture_tension_to_label(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => CULTURE_TENSION_LABELS[0],
        fb::CultureTensionKind::AssimilationPush => CULTURE_TENSION_LABELS[1],
        fb::CultureTensionKind::SchismRisk => CULTURE_TENSION_LABELS[2],
        _ => "Unknown",
    }
}

fn u32_vector_to_packed_int32(list: Option<flatbuffers::Vector<'_, u32>>) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn u16_vector_to_packed_int32(list: Option<flatbuffers::Vector<'_, u16>>) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn u64_vector_to_packed_int64(list: Option<flatbuffers::Vector<'_, u64>>) -> PackedInt64Array {
    let mut array = PackedInt64Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i64;
        }
    }
    array
}

struct ShadowScaleExtension;

#[gdextension(entry_symbol = godot_rs_shadow_scale_godot_init)]
unsafe impl ExtensionLibrary for ShadowScaleExtension {}
