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
    #[serde(default)]
    pub map_seed: Option<u64>,
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
    #[serde(default)]
    pub locked_terrain_tags: Vec<String>,
    #[serde(default)]
    pub mountains: MountainsConfig,
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
    #[serde(default)]
    pub terrain_classifier: TerrainClassifierConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MountainsConfig {
    pub belt_width_tiles: u32,
    pub fold_strength: f32,
    pub fault_line_count: u32,
    pub fault_strength: f32,
    pub volcanic_arc_chance: f32,
    pub volcanic_chain_length: u32,
    pub volcanic_strength: f32,
    pub max_volcanic_chains_per_plate: u32,
    pub volcanic_strength_drop: f32,
    pub volcanic_tile_cap_per_plate: u32,
    pub plateau_density: f32,
    #[serde(default)]
    pub plateau_microrelief_strength: f32,
    #[serde(default)]
    pub plateau_rim_width: u32,
    #[serde(default)]
    pub plateau_terrace_variance: f32,
    #[serde(default = "default_polar_latitude_fraction")]
    pub polar_latitude_fraction: f32,
    #[serde(default = "default_polar_microplate_density")]
    pub polar_microplate_density: f32,
    #[serde(default = "default_polar_uplift_scale")]
    pub polar_uplift_scale: f32,
    #[serde(default = "default_polar_low_relief_scale")]
    pub polar_low_relief_scale: f32,
}

const fn default_polar_latitude_fraction() -> f32 {
    0.18
}

const fn default_polar_microplate_density() -> f32 {
    0.0015
}

const fn default_polar_uplift_scale() -> f32 {
    1.3
}

const fn default_polar_low_relief_scale() -> f32 {
    0.65
}

impl Default for MountainsConfig {
    fn default() -> Self {
        Self {
            belt_width_tiles: 3,
            fold_strength: 0.45,
            fault_line_count: 1,
            fault_strength: 0.3,
            volcanic_arc_chance: 0.35,
            volcanic_chain_length: 4,
            volcanic_strength: 0.35,
            max_volcanic_chains_per_plate: 2,
            volcanic_strength_drop: 1.5,
            volcanic_tile_cap_per_plate: 36,
            plateau_density: 0.05,
            plateau_microrelief_strength: 0.0,
            plateau_rim_width: 1,
            plateau_terrace_variance: 0.0,
            polar_latitude_fraction: default_polar_latitude_fraction(),
            polar_microplate_density: default_polar_microplate_density(),
            polar_uplift_scale: default_polar_uplift_scale(),
            polar_low_relief_scale: default_polar_low_relief_scale(),
        }
    }
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
    pub coastal_rainfall_decay: f32,
    pub interior_aridity_strength: f32,
    pub prevailing_wind_flip_chance: f32,
    pub rain_shadow_strength: f32,
    pub rain_shadow_decay: f32,
    pub windward_moisture_bonus: f32,
    pub base_humidity_weight: f32,
    pub latitude_humidity_weight: f32,
    pub dryness_thresholds: [f32; 3],
    pub humidity_scale: f32,
    pub humidity_bias: f32,
    pub coastal_bonus_scale: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct TerrainClassifierConfig {
    pub coastal_deep_ocean_edge: f32,
    pub coastal_shelf_edge: f32,
    pub coastal_inland_edge: f32,
    pub polar_latitude_cutoff: f32,
    pub high_latitude_threshold: f32,
}

impl TerrainClassifierConfig {
    pub const fn default_values() -> Self {
        Self {
            coastal_deep_ocean_edge: 0.04,
            coastal_shelf_edge: 0.08,
            coastal_inland_edge: 0.12,
            polar_latitude_cutoff: 0.35,
            high_latitude_threshold: 0.15,
        }
    }
}

impl Default for BiomeTransitionConfig {
    fn default() -> Self {
        Self {
            orographic_strength: 0.6,
            transition_width: 2,
            band_profile: "default".to_string(),
            coastal_rainfall_decay: 3.0,
            interior_aridity_strength: 0.35,
            prevailing_wind_flip_chance: 0.1,
            rain_shadow_strength: 0.28,
            rain_shadow_decay: 0.08,
            windward_moisture_bonus: 0.2,
            base_humidity_weight: 0.55,
            latitude_humidity_weight: 0.45,
            dryness_thresholds: [0.65, 0.45, 0.30],
            humidity_scale: 1.0,
            humidity_bias: 0.0,
            coastal_bonus_scale: 0.8,
        }
    }
}

impl Default for TerrainClassifierConfig {
    fn default() -> Self {
        Self::default_values()
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

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
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
