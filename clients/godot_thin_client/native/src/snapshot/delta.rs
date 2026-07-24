//! Delta application. A delta carries only the sections that changed, so
//! [`DeltaAggregator`] accumulates them into the same shape a full snapshot has and
//! then hands that to [`crate::snapshot::snapshot_dict`].

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use std::collections::HashMap;

use crate::dict::fixed64_to_f32;
use crate::snapshot::raster::{GridSize, OverlaySlices, TerrainSlices};
use crate::snapshot::snapshot_dict;

#[derive(Clone, Default)]
pub(crate) struct CrisisAnnotationRecord {
    pub(crate) label: Option<String>,
    pub(crate) severity: fb::CrisisSeverityBand,
    pub(crate) path: Vec<i32>,
}

#[derive(Default)]
pub(crate) struct DeltaAggregator {
    pub(crate) tick: u64,
    width: u32,
    height: u32,
    pub(crate) wrap_horizontal: bool,
    pub(crate) server_build: String,
    pub(crate) world_epoch: u32,
    tile_updates: HashMap<(u32, u32), f32>,
    terrain_width: u32,
    terrain_height: u32,
    terrain_types: Vec<u16>,
    terrain_tags: Vec<u16>,
    logistics_width: u32,
    logistics_height: u32,
    logistics_samples: Vec<f32>,
    sentiment_width: u32,
    sentiment_height: u32,
    sentiment_samples: Vec<f32>,
    corruption_width: u32,
    corruption_height: u32,
    corruption_samples: Vec<f32>,
    fog_width: u32,
    fog_height: u32,
    fog_samples: Vec<f32>,
    visibility_width: u32,
    visibility_height: u32,
    visibility_samples: Vec<f32>,
    culture_width: u32,
    culture_height: u32,
    culture_samples: Vec<f32>,
    military_width: u32,
    military_height: u32,
    military_samples: Vec<f32>,
    crisis_width: u32,
    crisis_height: u32,
    crisis_samples: Vec<f32>,
    elevation_width: u32,
    elevation_height: u32,
    elevation_samples: Vec<f32>,
    // Sea level on the same normalized 0..1 scale as `elevation_samples`, streamed from
    // the active map's preset so the client's relative-height readout floors at it.
    elevation_sea_level: f32,
    // Per-map climate-band cut points `[polarMaxTemp, borealMaxTemp, temperateMaxTemp]` (°C).
    // `None` unless this delta actually carried a `ClimateBands` table, so a delta that omits
    // it publishes no climate keys and the client keeps the last full snapshot's per-map value
    // (the bands are a per-map constant) rather than being handed a fabricated one.
    climate_bands: Option<[f32; 3]>,
    moisture_width: u32,
    moisture_height: u32,
    moisture_samples: Vec<f32>,
    crisis_annotations: Vec<CrisisAnnotationRecord>,
}

impl DeltaAggregator {
    pub(crate) fn update_tile(&mut self, x: u32, y: u32, temperature: i64) {
        self.width = self.width.max(x + 1);
        self.height = self.height.max(y + 1);
        self.tile_updates
            .insert((x, y), fixed64_to_f32(temperature));
    }

    pub(crate) fn apply_terrain_overlay(&mut self, overlay: fb::TerrainOverlay<'_>) {
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

    pub(crate) fn apply_logistics_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.logistics_width = raster.width();
        self.logistics_height = raster.height();
        let count = (self.logistics_width as usize)
            .saturating_mul(self.logistics_height as usize)
            .max(1);
        self.logistics_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.logistics_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_sentiment_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.sentiment_width = raster.width();
        self.sentiment_height = raster.height();
        let count = (self.sentiment_width as usize)
            .saturating_mul(self.sentiment_height as usize)
            .max(1);
        self.sentiment_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.sentiment_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_corruption_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.corruption_width = raster.width();
        self.corruption_height = raster.height();
        let count = (self.corruption_width as usize)
            .saturating_mul(self.corruption_height as usize)
            .max(1);
        self.corruption_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.corruption_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_fog_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.fog_width = raster.width();
        self.fog_height = raster.height();
        let count = (self.fog_width as usize)
            .saturating_mul(self.fog_height as usize)
            .max(1);
        self.fog_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.fog_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_visibility_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.visibility_width = raster.width();
        self.visibility_height = raster.height();
        let count = (self.visibility_width as usize)
            .saturating_mul(self.visibility_height as usize)
            .max(1);
        self.visibility_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.visibility_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_culture_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.culture_width = raster.width();
        self.culture_height = raster.height();
        let count = (self.culture_width as usize)
            .saturating_mul(self.culture_height as usize)
            .max(1);
        self.culture_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.culture_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_military_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.military_width = raster.width();
        self.military_height = raster.height();
        let count = (self.military_width as usize)
            .saturating_mul(self.military_height as usize)
            .max(1);
        self.military_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.military_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    pub(crate) fn apply_crisis_overlay(&mut self, overlay: fb::CrisisOverlayState<'_>) {
        if let Some(raster) = overlay.heatmap() {
            self.crisis_width = raster.width();
            self.crisis_height = raster.height();
            let count = (self.crisis_width as usize)
                .saturating_mul(self.crisis_height as usize)
                .max(1);
            self.crisis_samples.resize(count, 0.0);
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= count {
                        break;
                    }
                    self.crisis_samples[idx] = fixed64_to_f32(value);
                }
            }
        }
        self.crisis_annotations.clear();
        if let Some(entries) = overlay.annotations() {
            self.crisis_annotations.reserve(entries.len());
            for entry in entries {
                let mut path = Vec::new();
                if let Some(route) = entry.path() {
                    path.reserve(route.len());
                    for value in route {
                        path.push(value as i32);
                    }
                }
                self.crisis_annotations.push(CrisisAnnotationRecord {
                    label: entry.label().map(|value| value.to_string()),
                    severity: entry.severity(),
                    path,
                });
            }
        }
    }

    pub(crate) fn apply_elevation_overlay(&mut self, overlay: fb::ElevationOverlay<'_>) {
        self.elevation_width = overlay.width();
        self.elevation_height = overlay.height();
        self.elevation_sea_level = overlay.seaLevel();
        let count = (self.elevation_width as usize)
            .saturating_mul(self.elevation_height as usize)
            .max(1);
        self.elevation_samples.resize(count, 0.0);
        if let Some(samples) = overlay.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                let normalized = (value as f32) / 65535.0;
                let clamped = normalized.clamp(0.0, 1.0);
                self.elevation_samples[idx] = clamped;
            }
        }
    }

    pub(crate) fn apply_climate_bands(&mut self, bands: fb::ClimateBands<'_>) {
        self.climate_bands = Some([
            bands.polarMaxTemp(),
            bands.borealMaxTemp(),
            bands.temperateMaxTemp(),
        ]);
    }

    pub(crate) fn apply_moisture_raster(&mut self, raster: fb::FloatRaster<'_>) {
        self.moisture_width = raster.width();
        self.moisture_height = raster.height();
        let count = (self.moisture_width as usize)
            .saturating_mul(self.moisture_height as usize)
            .max(1);
        self.moisture_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.moisture_samples[idx] = value;
            }
        }
    }

    pub(crate) fn into_dictionary(self) -> VarDictionary {
        let DeltaAggregator {
            tick,
            width,
            height,
            wrap_horizontal,
            server_build,
            world_epoch,
            tile_updates,
            terrain_width,
            terrain_height,
            terrain_types,
            terrain_tags,
            logistics_width,
            logistics_height,
            logistics_samples,
            sentiment_width,
            sentiment_height,
            sentiment_samples,
            corruption_width,
            corruption_height,
            corruption_samples,
            fog_width,
            fog_height,
            fog_samples,
            visibility_width,
            visibility_height,
            visibility_samples,
            culture_width,
            culture_height,
            culture_samples,
            military_width,
            military_height,
            military_samples,
            crisis_width,
            crisis_height,
            crisis_samples,
            elevation_width,
            elevation_height,
            elevation_samples,
            elevation_sea_level,
            climate_bands,
            moisture_width,
            moisture_height,
            moisture_samples,
            crisis_annotations,
        } = self;

        let mut final_width = terrain_width
            .max(width)
            .max(logistics_width)
            .max(sentiment_width)
            .max(corruption_width)
            .max(fog_width)
            .max(visibility_width)
            .max(culture_width)
            .max(military_width)
            .max(crisis_width)
            .max(elevation_width)
            .max(moisture_width);
        let mut final_height = terrain_height
            .max(height)
            .max(logistics_height)
            .max(sentiment_height)
            .max(corruption_height)
            .max(fog_height)
            .max(visibility_height)
            .max(culture_height)
            .max(military_height)
            .max(crisis_height)
            .max(elevation_height)
            .max(moisture_height);
        if final_width == 0 || final_height == 0 {
            final_width = final_width.max(1);
            final_height = final_height.max(1);
        }
        let total = (final_width as usize)
            .saturating_mul(final_height as usize)
            .max(1);

        let mut logistics = vec![0.0f32; total];
        if logistics_width > 0 && logistics_height > 0 && !logistics_samples.is_empty() {
            for y in 0..logistics_height {
                for x in 0..logistics_width {
                    let src_idx = (y as usize) * (logistics_width as usize) + x as usize;
                    if src_idx >= logistics_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    logistics[dst_idx] = logistics_samples[src_idx];
                }
            }
        } else {
            for ((x, y), value) in tile_updates {
                if x >= final_width || y >= final_height {
                    continue;
                }
                let idx = (y as usize) * (final_width as usize) + x as usize;
                logistics[idx] = value;
            }
        }

        let mut sentiment = vec![0.0f32; total];
        if sentiment_width > 0 && sentiment_height > 0 && !sentiment_samples.is_empty() {
            for y in 0..sentiment_height {
                for x in 0..sentiment_width {
                    let src_idx = (y as usize) * (sentiment_width as usize) + x as usize;
                    if src_idx >= sentiment_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    sentiment[dst_idx] = sentiment_samples[src_idx];
                }
            }
        }

        let mut corruption = vec![0.0f32; total];
        if corruption_width > 0 && corruption_height > 0 && !corruption_samples.is_empty() {
            for y in 0..corruption_height {
                for x in 0..corruption_width {
                    let src_idx = (y as usize) * (corruption_width as usize) + x as usize;
                    if src_idx >= corruption_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    corruption[dst_idx] = corruption_samples[src_idx];
                }
            }
        }

        let mut fog = vec![0.0f32; total];
        if fog_width > 0 && fog_height > 0 && !fog_samples.is_empty() {
            for y in 0..fog_height {
                for x in 0..fog_width {
                    let src_idx = (y as usize) * (fog_width as usize) + x as usize;
                    if src_idx >= fog_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    fog[dst_idx] = fog_samples[src_idx];
                }
            }
        }

        let mut visibility = vec![0.0f32; total];
        if visibility_width > 0 && visibility_height > 0 && !visibility_samples.is_empty() {
            for y in 0..visibility_height {
                for x in 0..visibility_width {
                    let src_idx = (y as usize) * (visibility_width as usize) + x as usize;
                    if src_idx >= visibility_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    visibility[dst_idx] = visibility_samples[src_idx];
                }
            }
        }

        let mut culture = vec![0.0f32; total];
        if culture_width > 0 && culture_height > 0 && !culture_samples.is_empty() {
            for y in 0..culture_height {
                for x in 0..culture_width {
                    let src_idx = (y as usize) * (culture_width as usize) + x as usize;
                    if src_idx >= culture_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    culture[dst_idx] = culture_samples[src_idx];
                }
            }
        }

        let mut military = vec![0.0f32; total];
        if military_width > 0 && military_height > 0 && !military_samples.is_empty() {
            for y in 0..military_height {
                for x in 0..military_width {
                    let src_idx = (y as usize) * (military_width as usize) + x as usize;
                    if src_idx >= military_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    military[dst_idx] = military_samples[src_idx];
                }
            }
        }

        let mut crisis = vec![0.0f32; total];
        if crisis_width > 0 && crisis_height > 0 && !crisis_samples.is_empty() {
            for y in 0..crisis_height {
                for x in 0..crisis_width {
                    let src_idx = (y as usize) * (crisis_width as usize) + x as usize;
                    if src_idx >= crisis_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    crisis[dst_idx] = crisis_samples[src_idx];
                }
            }
        }

        let mut elevation = vec![0.0f32; total];
        if elevation_width > 0 && elevation_height > 0 && !elevation_samples.is_empty() {
            for y in 0..elevation_height {
                for x in 0..elevation_width {
                    let src_idx = (y as usize) * (elevation_width as usize) + x as usize;
                    if src_idx >= elevation_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    elevation[dst_idx] = elevation_samples[src_idx];
                }
            }
        }

        let mut moisture = vec![0.0f32; total];
        if moisture_width > 0 && moisture_height > 0 && !moisture_samples.is_empty() {
            for y in 0..moisture_height {
                for x in 0..moisture_width {
                    let src_idx = (y as usize) * (moisture_width as usize) + x as usize;
                    if src_idx >= moisture_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    moisture[dst_idx] = moisture_samples[src_idx];
                }
            }
        }

        let terrain_ref = if terrain_types.is_empty() {
            None
        } else {
            Some(terrain_types)
        };
        let tags_ref = if terrain_tags.is_empty() {
            None
        } else {
            Some(terrain_tags)
        };

        let mut dict = snapshot_dict(
            tick,
            GridSize {
                width: final_width,
                height: final_height,
                wrap_horizontal,
            },
            OverlaySlices {
                logistics: &logistics,
                sentiment: &sentiment,
                corruption: &corruption,
                fog: &fog,
                culture: &culture,
                military: &military,
                crisis: &crisis,
                elevation: &elevation,
                elevation_sea_level,
                climate_bands,
                moisture: &moisture,
                visibility: &visibility,
                // A delta carries only the tiles that CHANGED, so it can never assemble a whole
                // pasture field — it publishes NO pasture channel rather than a field of zeros
                // that would claim the world has no pasture. (The delta path degrades the same way
                // for every other channel it did not receive; the live stream is full snapshots.)
                pasture_capacity: &[],
                // Same reasoning: a delta carries no forage-patch list, so it publishes NO forage
                // channel rather than a field of zeros that would claim there are no gathering sites.
                forage_capacity: &[],
            },
            TerrainSlices {
                terrain: terrain_ref.as_deref(),
                tags: tags_ref.as_deref(),
            },
            &crisis_annotations,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        if !server_build.is_empty() {
            let _ = dict.insert("server_build", server_build.as_str());
        }
        // Same world-generation counter the full-snapshot path carries (see snapshot.fbs
        // `worldEpoch`), so a delta arriving before the first full frame can be recognised as
        // pre-/post-rebuild by the loading gate. Default 0 (idle boot app / absent header).
        let _ = dict.insert("world_epoch", world_epoch as i64);
        dict
    }
}
