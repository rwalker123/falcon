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
    Some(agg.into_dictionary())
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

    snapshot_dict(
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
    )
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

struct ShadowScaleExtension;

#[gdextension(entry_symbol = godot_rs_shadow_scale_godot_init)]
unsafe impl ExtensionLibrary for ShadowScaleExtension {}
