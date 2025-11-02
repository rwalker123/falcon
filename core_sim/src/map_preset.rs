#![allow(dead_code)]

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

pub const BUILTIN_MAP_PRESETS: &str = include_str!("data/map_presets.json");

#[derive(Debug, Clone, Deserialize)]
pub struct MapPresetDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub seed_policy: String,
    pub dimensions: MapPresetDimensions,
    pub sea_level: f32,
    pub continent_scale: f32,
    pub mountain_scale: f32,
    pub moisture_scale: f32,
    pub river_density: f32,
    pub lake_chance: f32,
    #[serde(default)]
    pub climate_band_weights: HashMap<String, f32>,
    #[serde(default)]
    pub terrain_tag_targets: HashMap<String, f32>,
    #[serde(default)]
    pub biome_weights: HashMap<String, f32>,
    #[serde(default)]
    pub postprocess: serde_json::Value,
    #[serde(default)]
    pub tolerance: f32,
    #[serde(default = "default_river_accum_threshold_factor")]
    pub river_accum_threshold_factor: f32,
    #[serde(default = "default_river_min_accum")]
    pub river_min_accum: u16,
    #[serde(default = "default_river_min_length")]
    pub river_min_length: usize,
    #[serde(default = "default_river_fallback_min_length")]
    pub river_fallback_min_length: usize,
    #[serde(default = "default_river_accum_percentile")]
    pub river_accum_percentile: f32,
    #[serde(default = "default_river_land_ratio")]
    pub river_land_ratio: f32,
    #[serde(default = "default_river_min_count")]
    pub river_min_count: usize,
    #[serde(default = "default_river_max_count")]
    pub river_max_count: usize,
    #[serde(default = "default_river_source_percentile")]
    pub river_source_percentile: f32,
    #[serde(default = "default_river_source_sea_buffer")]
    pub river_source_sea_buffer: f32,
    #[serde(default = "default_river_min_spacing")]
    pub river_min_spacing: f32,
    #[serde(default = "default_river_uphill_step_limit")]
    pub river_uphill_step_limit: u8,
    #[serde(default = "default_river_uphill_gain_pct")]
    pub river_uphill_gain_pct: f32,

    #[serde(default)]
    pub macro_land: MacroLandConfig,
    #[serde(default)]
    pub shelf: ShelfConfig,
    #[serde(default)]
    pub islands: IslandConfig,
    #[serde(default)]
    pub inland_sea: InlandSeaConfig,
    #[serde(default)]
    pub ocean: OceanConfig,
    #[serde(default)]
    pub biomes: BiomeTransitionConfig,
}

const fn default_river_accum_threshold_factor() -> f32 {
    0.35
}

const fn default_river_min_accum() -> u16 {
    6
}

const fn default_river_min_length() -> usize {
    8
}

const fn default_river_fallback_min_length() -> usize {
    4
}

const fn default_river_accum_percentile() -> f32 {
    0.98
}

const fn default_river_land_ratio() -> f32 {
    300.0
}

const fn default_river_min_count() -> usize {
    2
}

const fn default_river_max_count() -> usize {
    128
}

const fn default_river_source_percentile() -> f32 {
    0.7
}

const fn default_river_source_sea_buffer() -> f32 {
    0.08
}

const fn default_river_min_spacing() -> f32 {
    12.0
}

const fn default_river_uphill_step_limit() -> u8 {
    2
}

const fn default_river_uphill_gain_pct() -> f32 {
    0.05
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MacroLandConfig {
    pub continents: u32,
    pub min_area: u32,
    pub target_land_pct: f32,
    pub jitter: f32,
}

impl Default for MacroLandConfig {
    fn default() -> Self {
        Self {
            continents: 3,
            min_area: 128,
            target_land_pct: 0.35,
            jitter: 0.15,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ShelfConfig {
    pub width_tiles: u32,
    pub slope_width_tiles: u32,
}

impl Default for ShelfConfig {
    fn default() -> Self {
        Self {
            width_tiles: 2,
            slope_width_tiles: 3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IslandConfig {
    pub continental_density: f32,
    pub oceanic_density: f32,
    pub fringing_shelf_width: u32,
    pub min_distance_from_continent: u32,
}

impl Default for IslandConfig {
    fn default() -> Self {
        Self {
            continental_density: 0.002,
            oceanic_density: 0.001,
            fringing_shelf_width: 2,
            min_distance_from_continent: 12,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InlandSeaConfig {
    pub min_area: u32,
    pub merge_strait_width: u32,
}

impl Default for InlandSeaConfig {
    fn default() -> Self {
        Self {
            min_area: 24,
            merge_strait_width: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OceanConfig {
    pub ridge_density: f32,
    pub ridge_amplitude: f32,
}

impl Default for OceanConfig {
    fn default() -> Self {
        Self {
            ridge_density: 0.0,
            ridge_amplitude: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BiomeTransitionConfig {
    pub orographic_strength: f32,
    pub transition_width: u32,
    pub band_profile: String,
}

impl Default for BiomeTransitionConfig {
    fn default() -> Self {
        Self {
            orographic_strength: 0.6,
            transition_width: 2,
            band_profile: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapPresetsFile {
    pub presets: Vec<MapPreset>,
}

#[derive(Debug, Clone)]
pub struct MapPresets {
    by_id: std::collections::HashMap<String, MapPreset>,
}

impl MapPresets {
    pub fn builtin() -> Arc<Self> {
        let parsed: MapPresetsFile =
            serde_json::from_str(BUILTIN_MAP_PRESETS).expect("builtin map presets should parse");
        let mut by_id = std::collections::HashMap::new();
        for p in parsed.presets.into_iter() {
            by_id.insert(p.id.clone(), p);
        }
        Arc::new(Self { by_id })
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        let parsed: MapPresetsFile = serde_json::from_str(json)?;
        let mut by_id = std::collections::HashMap::new();
        for p in parsed.presets.into_iter() {
            by_id.insert(p.id.clone(), p);
        }
        Ok(Self { by_id })
    }

    pub fn from_file(path: &Path) -> Result<Self, MapPresetsError> {
        let contents = fs::read_to_string(path).map_err(|source| MapPresetsError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let config = MapPresets::from_json_str(&contents)?;
        Ok(config)
    }

    pub fn get(&self, id: &str) -> Option<&MapPreset> {
        self.by_id.get(id)
    }

    pub fn first(&self) -> Option<&MapPreset> {
        self.by_id.values().next()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

#[derive(Debug, Error)]
pub enum MapPresetsError {
    #[error("failed to parse map presets: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read map presets from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Resource, Debug, Clone)]
pub struct MapPresetsHandle(Arc<MapPresets>);

impl MapPresetsHandle {
    pub fn new(presets: Arc<MapPresets>) -> Self {
        Self(presets)
    }

    pub fn get(&self) -> Arc<MapPresets> {
        self.0.clone()
    }
}

#[derive(Resource, Debug, Clone)]
pub struct MapPresetsMetadata {
    path: Option<PathBuf>,
}

impl MapPresetsMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

pub fn load_map_presets_from_env() -> (Arc<MapPresets>, MapPresetsMetadata) {
    let override_path = env::var("MAP_PRESETS_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/map_presets.json");
    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match MapPresets::from_file(&path) {
            Ok(presets) => {
                tracing::info!(
                    target: "shadow_scale::mapgen",
                    path = %path.display(),
                    "map_presets.loaded=file"
                );
                return (Arc::new(presets), MapPresetsMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::mapgen",
                    path = %path.display(),
                    error = %err,
                    "map_presets.load_failed"
                );
            }
        }
    }

    let presets = MapPresets::builtin();
    tracing::info!(
        target = "shadow_scale::mapgen",
        "map_presets.loaded=builtin"
    );
    (presets, MapPresetsMetadata::new(None))
}
