use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

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
        fb::SnapshotPayload::snapshot => envelope
            .payload_as_snapshot()
            .map(snapshot_to_dict),
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
    Some(agg.into_dictionary())
}

#[derive(Default)]
struct DeltaAggregator {
    tick: u64,
    width: u32,
    height: u32,
    temperatures: Vec<f32>,
}

impl DeltaAggregator {
    fn update_tile(&mut self, x: u32, y: u32, temperature: i64) {
        self.width = self.width.max(x + 1);
        self.height = self.height.max(y + 1);
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        if self.temperatures.len() <= idx {
            self.temperatures.resize(idx + 1, 0.0);
        }
        self.temperatures[idx] = fixed64_to_f32(temperature);
    }

    fn into_dictionary(mut self) -> Dictionary {
        if self.width == 0 || self.height == 0 {
            self.width = 1;
            self.height = 1;
        }
        let needed = (self.width as usize) * (self.height as usize);
        if self.temperatures.len() < needed {
            self.temperatures.resize(needed, 0.0);
        }
        normalize_overlay(&mut self.temperatures);
        snapshot_dict_from_overlay(self.tick, self.width, self.height, &self.temperatures)
    }
}

fn snapshot_to_dict(snapshot: fb::WorldSnapshot<'_>) -> Dictionary {
    let header = snapshot.header().unwrap();
    let mut width = 0u32;
    let mut height = 0u32;
    let mut temperatures = Vec::new();

    if let Some(tiles) = snapshot.tiles() {
        for tile in tiles {
            width = width.max(tile.x() + 1);
            height = height.max(tile.y() + 1);
        }
        let size = (width as usize).saturating_mul(height as usize).max(1);
        temperatures.resize(size, 0.0);
        for tile in tiles {
            let x = tile.x();
            let y = tile.y();
            let idx = (y as usize) * (width as usize) + (x as usize);
            if idx < temperatures.len() {
                temperatures[idx] = fixed64_to_f32(tile.temperature());
            }
        }
    }

    if width == 0 || height == 0 {
        width = 1;
        height = 1;
        temperatures.resize(1, 0.0);
    }

    normalize_overlay(&mut temperatures);
    snapshot_dict_from_overlay(header.tick(), width, height, &temperatures)
}

fn snapshot_dict_from_overlay(tick: u64, width: u32, height: u32, overlay: &[f32]) -> Dictionary {
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
        let count = overlay.len().min(slice.len());
        slice[..count].copy_from_slice(&overlay[..count]);
    }

    // Provide a secondary overlay that highlights relative differences across the grid.
    let mut contrast = PackedFloat32Array::new();
    contrast.resize(size);
    if size > 0 {
        let slice = contrast.as_mut_slice();
        let count = overlay.len().min(slice.len());
        if count > 0 {
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for &value in &overlay[..count] {
                if value.is_finite() {
                    min = min.min(value);
                    max = max.max(value);
                }
            }
            if min.is_finite() && max.is_finite() && (max - min).abs() > f32::EPSILON {
                let range = max - min;
                for i in 0..count {
                    let normalized = (overlay[i] - min) / range;
                    slice[i] = normalized * (1.0 - normalized);
                }
            }
        }
    }

    let mut overlays = Dictionary::new();
    let _ = overlays.insert("logistics", logistics);
    let _ = overlays.insert("contrast", contrast);
    let _ = dict.insert("overlays", overlays);

    let _ = dict.insert("units", VariantArray::new());
    let _ = dict.insert("orders", VariantArray::new());

    dict
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
