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
    #[serde(default)]
    pub biome_palette: BiomePaletteConfig,
}

/// Per-preset tuning for the per-map biome palette (`docs/plan_biome_palette.md` §4.2).
/// The palette is always applied — this block only tunes the per-niche distinct-biome
/// counts `K`, interpolated by map area between `k_small` (at `small_map_tiles`) and
/// `k_large` (at `large_map_tiles`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BiomePaletteConfig {
    /// Map area (in tiles) at/below which each niche uses its `k_small`.
    pub small_map_tiles: u32,
    /// Map area (in tiles) at/above which each niche uses its `k_large`.
    pub large_map_tiles: u32,
    /// Per-niche `K` endpoints, keyed by [`crate::terrain::BiomeNiche::as_str`].
    pub niches: HashMap<String, NicheKConfig>,
}

/// The two `K` endpoints for one niche (small-map floor, large-map ceiling).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct NicheKConfig {
    pub k_small: u32,
    pub k_large: u32,
}

impl Default for NicheKConfig {
    fn default() -> Self {
        Self {
            k_small: 1,
            k_large: 3,
        }
    }
}

impl Default for BiomePaletteConfig {
    fn default() -> Self {
        // The §4.2 illustrative defaults: a small map reads ~one biome per climate zone
        // plus a couple of discovery-flavor anomalies; a large map fills back out.
        let niches = [
            ("Ocean", 2, 4),
            ("CoastWetland", 1, 4),
            ("FertileLowland", 2, 5),
            ("AridLowland", 1, 4),
            ("PolarLowland", 1, 3),
            // Highland + Volcanic are physically relief/elevation/mask-gated: each member
            // maps to a specific relief/moisture/mask regime, so any palette swap between
            // them stamps the wrong biome on a physically-specific tile. Never thin them —
            // keep every member always-available (K = full membership). Legibility comes
            // from thinning the interchangeable flat-land niches, not these.
            ("Highland", 5, 5),
            ("Volcanic", 3, 3),
            ("Anomaly", 2, 4),
        ]
        .into_iter()
        .map(|(name, k_small, k_large)| (name.to_string(), NicheKConfig { k_small, k_large }))
        .collect();
        Self {
            // Anchors correspond to the selectable map presets: Tiny (2016) → k_small so the
            // smallest map reads legibly, Huge (10240) → k_large so the largest reads rich.
            // Standard (4160) lands partway up the smoothstep curve between them.
            small_map_tiles: 2016,
            large_map_tiles: 10240,
            niches,
        }
    }
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
    /// Elevation (normalized 0..1) that separates lowlands from mountains. Non-mountain
    /// land is compressed into `[sea_level, elevation_base]`; every mountain tile is
    /// floored at least here so mountains always read higher than plains. Tie the
    /// elevation field to the (relief-based) biome so mountains are genuinely tall.
    #[serde(default = "default_mountain_elevation_base")]
    pub elevation_base: f32,
    /// Per-mountain-type prominence weights applied to the relief-driven floor above
    /// `elevation_base` — Fold/Volcanic peaks tower, Domes read as lower plateaus.
    #[serde(default = "default_fold_prominence")]
    pub fold_prominence: f32,
    #[serde(default = "default_fault_prominence")]
    pub fault_prominence: f32,
    #[serde(default = "default_volcanic_prominence")]
    pub volcanic_prominence: f32,
    #[serde(default = "default_dome_prominence")]
    pub dome_prominence: f32,
    /// Amplitude of the belt-position elevation texture added on top of a mountain tile's
    /// relief-driven floor (spine tiles slightly taller than edges). Bounded small so it
    /// never lifts a low-relief tile above a higher-relief one — relief still orders tiles.
    #[serde(default = "default_belt_texture")]
    pub belt_texture: f32,
    /// How much a belt tile's relief scales with its belt strength (core vs edge).
    /// Belt cores reach `1.0 + relief_belt_gain`, so with the default they clear the
    /// AlpineMountain relief threshold (1.45) and taper to plateaus/hills at the edges —
    /// giving genuine Alpine spines instead of flat, mask-less "mountains".
    #[serde(default = "default_relief_belt_gain")]
    pub relief_belt_gain: f32,
    /// Plate-boundary convergence cutoff for forming a fold belt (a mountain range).
    /// A boundary becomes a belt when the two plates' drift dot product is `<=` this.
    /// Drift is radial-outward, so most boundaries diverge (dot > 0); raising this from
    /// strongly-convergent (−0.1) toward 0 lets more boundaries qualify → more ranges,
    /// which matters most on small maps where continents have few plates.
    #[serde(default = "default_belt_convergence")]
    pub belt_convergence: f32,
    /// Continent-area thresholds (in land tiles) for how many tectonic plates a landmass
    /// is split into (1/2/3/4). More plates → more convergent boundaries → more ranges.
    /// `plate_area_bump` gives a 2nd plate to a landmass that would otherwise get 1.
    #[serde(default = "default_plate_area_bucket_2")]
    pub plate_area_bucket_2: u32,
    #[serde(default = "default_plate_area_bucket_3")]
    pub plate_area_bucket_3: u32,
    #[serde(default = "default_plate_area_bucket_4")]
    pub plate_area_bucket_4: u32,
    #[serde(default = "default_plate_area_bump")]
    pub plate_area_bump: u32,
    /// Polar-microplate boundary dot-product cutoffs: `<= polar_convergence` uplifts
    /// (fold), `>= polar_divergence` relaxes to low relief (the polar analogue of
    /// `belt_convergence`).
    #[serde(default = "default_polar_convergence")]
    pub polar_convergence: f32,
    #[serde(default = "default_polar_divergence")]
    pub polar_divergence: f32,
    /// Fault abundance/length: plates larger than these areas get +1 fault line each, and
    /// a seam's length is `plate_area * fault_length_fraction * rand`.
    #[serde(default = "default_fault_area_bonus_2")]
    pub fault_area_bonus_2: u32,
    #[serde(default = "default_fault_area_bonus_3")]
    pub fault_area_bonus_3: u32,
    #[serde(default = "default_fault_length_fraction")]
    pub fault_length_fraction: f32,
    /// Volcanic distribution: plate-area normalization for eruption weight, per-plate tile
    /// budget fraction, and the coastal-bias base/gain (arcs favor coasts).
    #[serde(default = "default_volcanic_area_norm")]
    pub volcanic_area_norm: f32,
    #[serde(default = "default_volcanic_tile_fraction")]
    pub volcanic_tile_fraction: f32,
    #[serde(default = "default_volcanic_coastal_base")]
    pub volcanic_coastal_base: f32,
    #[serde(default = "default_volcanic_coastal_gain")]
    pub volcanic_coastal_gain: f32,
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
            elevation_base: default_mountain_elevation_base(),
            fold_prominence: default_fold_prominence(),
            fault_prominence: default_fault_prominence(),
            volcanic_prominence: default_volcanic_prominence(),
            dome_prominence: default_dome_prominence(),
            belt_texture: default_belt_texture(),
            relief_belt_gain: default_relief_belt_gain(),
            belt_convergence: default_belt_convergence(),
            plate_area_bucket_2: default_plate_area_bucket_2(),
            plate_area_bucket_3: default_plate_area_bucket_3(),
            plate_area_bucket_4: default_plate_area_bucket_4(),
            plate_area_bump: default_plate_area_bump(),
            polar_convergence: default_polar_convergence(),
            polar_divergence: default_polar_divergence(),
            fault_area_bonus_2: default_fault_area_bonus_2(),
            fault_area_bonus_3: default_fault_area_bonus_3(),
            fault_length_fraction: default_fault_length_fraction(),
            volcanic_area_norm: default_volcanic_area_norm(),
            volcanic_tile_fraction: default_volcanic_tile_fraction(),
            volcanic_coastal_base: default_volcanic_coastal_base(),
            volcanic_coastal_gain: default_volcanic_coastal_gain(),
        }
    }
}

const fn default_belt_convergence() -> f32 {
    0.05
}

const fn default_plate_area_bucket_2() -> u32 {
    192
}

const fn default_plate_area_bucket_3() -> u32 {
    640
}

const fn default_plate_area_bucket_4() -> u32 {
    1500
}

const fn default_plate_area_bump() -> u32 {
    256
}

const fn default_polar_convergence() -> f32 {
    -0.2
}

const fn default_polar_divergence() -> f32 {
    0.45
}

const fn default_fault_area_bonus_2() -> u32 {
    600
}

const fn default_fault_area_bonus_3() -> u32 {
    1400
}

const fn default_fault_length_fraction() -> f32 {
    0.1
}

const fn default_volcanic_area_norm() -> f32 {
    800.0
}

const fn default_volcanic_tile_fraction() -> f32 {
    0.012
}

const fn default_volcanic_coastal_base() -> f32 {
    0.55
}

const fn default_volcanic_coastal_gain() -> f32 {
    0.7
}

const fn default_mountain_elevation_base() -> f32 {
    0.7
}

const fn default_relief_belt_gain() -> f32 {
    1.2
}

const fn default_fold_prominence() -> f32 {
    1.0
}

const fn default_fault_prominence() -> f32 {
    0.85
}

const fn default_volcanic_prominence() -> f32 {
    1.0
}

const fn default_dome_prominence() -> f32 {
    0.7
}

const fn default_belt_texture() -> f32 {
    0.06
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
    /// Optional shelf-width coefficient that scales with map size. When set, the
    /// effective (possibly fractional) shelf band width is
    /// `width_frac * min(width, height).powf(width_exp)` tiles instead of the
    /// fixed `width_tiles`. The band width is deliberately *not* floored to a
    /// whole tile — at coarse resolution Earth's shelf is thinner than one tile,
    /// and `classify_bands` renders a sub-1.0 width as a partial coastal ring.
    /// `None` falls back to the absolute `width_tiles` (historical behavior).
    pub width_frac: Option<f32>,
    /// Exponent for the map-size scaling of `width_frac` (`min_dim^width_exp`).
    /// `1.0` is pure dimension-proportional scaling; values below 1.0 grow the
    /// band sub-linearly to counteract the coastline complexity that larger maps
    /// accumulate, keeping the shelf a size-invariant fraction of the ocean.
    /// Only consulted when `width_frac` is set; defaults to `1.0`.
    pub width_exp: Option<f32>,
    /// Minimum shelf-band width in tiles, floored *after* the `width_frac`/`width_exp`
    /// (or `width_tiles`) computation. A qualifying gentle coast always gets a
    /// *continuous* ring at least this wide instead of the old sub-tile sparse fringe,
    /// while `width_frac`/`width_exp` still scale it wider on big maps. The shelf %
    /// stays self-limited because the `coast_height_threshold` gate keeps steep/cliff
    /// coasts off the shelf entirely (passive- vs active-margin model). Defaults to `1.0`.
    pub min_width_tiles: f32,
    /// Coast-height gate (normalized rise above sea level, i.e. `elevation.sample − sea_level`).
    /// A shelf-candidate ocean tile only becomes `ContinentalShelf` when the coast land it
    /// abuts rises gently — the MIN rise of its adjacent land tiles is **below** this. Cliff /
    /// mountain / highland coasts (rise ≥ this) instead show deep water right at the edge,
    /// matching how real continental shelves form off passive margins and are absent off
    /// active ones. Sits low in the compressed lowland band `[sea_level, elevation_base]`.
    pub coast_height_threshold: f32,
}

impl Default for ShelfConfig {
    fn default() -> Self {
        Self {
            width_tiles: 2,
            slope_width_tiles: 3,
            width_frac: None,
            width_exp: None,
            min_width_tiles: default_shelf_min_width_tiles(),
            coast_height_threshold: default_shelf_coast_height_threshold(),
        }
    }
}

const fn default_shelf_min_width_tiles() -> f32 {
    1.0
}

const fn default_shelf_coast_height_threshold() -> f32 {
    // Sits in the bimodal gap between the compressed lowland band's top
    // (`elevation_base − sea_level ≈ 0.10`) and the mountain-coast rises that jump to
    // ≈0.16+, so every lowland coast reads gentle (→ shelf) and every mountain/highland
    // coast reads steep (→ deep water at the edge). Measured on generated earthlike maps.
    0.10
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
    /// Half-width (fraction of latitude from the equator) of the easterly trade-wind belt.
    pub trade_wind_band: f32,
    /// How fast the equatorial humidity bonus falls off toward the poles.
    pub latitude_dryness_falloff: f32,
    /// Distance scale for continental-interior drying: `distance/(distance + this)`.
    pub interior_aridity_distance: f32,
    /// How much a tile's elevation shifts its humidity (`(elev - 0.5) * this`).
    pub elevation_humidity_weight: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct TerrainClassifierConfig {
    pub coastal_deep_ocean_edge: f32,
    pub coastal_shelf_edge: f32,
    pub coastal_inland_edge: f32,
    pub polar_latitude_cutoff: f32,
    pub high_latitude_threshold: f32,
    /// Relief scale (from the mountain mask) at/above which a Fold belt tile becomes an
    /// AlpineMountain. `MountainsConfig::relief_belt_gain` and `elevation_base` defaults
    /// are tuned relative to this, so belt cores clear it and edges taper to plateaus/hills.
    pub alpine_relief_threshold: f32,
    /// Elevation (normalized 0..1) above which a dry non-mountain tile becomes
    /// CanyonBadlands. Sits near the top of the compressed lowland band (just under
    /// `MountainsConfig::elevation_base`) so high-dry plains still vary.
    pub high_dry_elevation: f32,
    /// Elevation (normalized 0..1) above which a wet non-mountain tile becomes
    /// RollingHills. Also near the top of the compressed lowland band.
    pub high_wet_elevation: f32,
    /// Moisture below which a high-dry non-mountain tile becomes CanyonBadlands (the
    /// companion gate to `high_dry_elevation`).
    pub high_dry_moisture: f32,
    /// Relief (from the volcanic mask) below which a non-polar Volcanic tile becomes a
    /// cooled-flow `BasalticLavaField` instead of an `ActiveVolcanoSlope` — the revived
    /// biome hook (`docs/plan_biome_palette.md` §3.6). Sits below `alpine_relief_threshold`
    /// so only lower-relief volcanic edges cool to basalt.
    pub basaltic_relief_threshold: f32,
    /// Fraction of eligible (flat, non-coastal, non-polar) lowland tiles that become a rare
    /// anomaly / "discovery" biome (crater/sinkhole/karst-cavern/fumarole/volcano/aquifer).
    /// A per-tile rarity roll gates the anomaly branch in `classify_terrain`; the surviving
    /// tiles split evenly across the 6 anomaly biomes. Kept low so anomalies read as rare
    /// discovery sites, not a blanket over the land (`docs/plan_biome_palette.md` §3.6).
    pub anomaly_fraction: f32,
}

impl TerrainClassifierConfig {
    pub const fn default_values() -> Self {
        Self {
            coastal_deep_ocean_edge: 0.04,
            coastal_shelf_edge: 0.08,
            coastal_inland_edge: 0.12,
            polar_latitude_cutoff: 0.35,
            high_latitude_threshold: 0.15,
            alpine_relief_threshold: 1.45,
            high_dry_elevation: 0.68,
            high_wet_elevation: 0.66,
            high_dry_moisture: 0.28,
            basaltic_relief_threshold: 1.0,
            anomaly_fraction: 0.04,
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
            trade_wind_band: 0.18,
            latitude_dryness_falloff: 1.8,
            interior_aridity_distance: 3.5,
            elevation_humidity_weight: 0.08,
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
