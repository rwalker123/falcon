use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::{
    food::FoodModule,
    scalar::{scalar_from_f32, Scalar},
};

pub const BUILTIN_SNAPSHOT_OVERLAYS_CONFIG: &str =
    include_str!("data/snapshot_overlays_config.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SnapshotOverlaysConfig {
    corruption: CorruptionOverlayConfig,
    culture: CultureOverlayConfig,
    military: MilitaryOverlayConfig,
    food: FoodOverlayConfig,
}

impl SnapshotOverlaysConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SNAPSHOT_OVERLAYS_CONFIG)
                .expect("builtin snapshot overlays config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SnapshotOverlaysConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SnapshotOverlaysConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        let config = SnapshotOverlaysConfig::from_json_str(&contents)?;
        Ok(config)
    }

    pub fn corruption(&self) -> &CorruptionOverlayConfig {
        &self.corruption
    }

    pub fn culture(&self) -> &CultureOverlayConfig {
        &self.culture
    }

    pub fn military(&self) -> &MilitaryOverlayConfig {
        &self.military
    }

    pub fn food(&self) -> &FoodOverlayConfig {
        &self.food
    }
}

#[derive(Debug, Error)]
pub enum SnapshotOverlaysConfigError {
    #[error("failed to parse snapshot overlays config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read snapshot overlays config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ConfigLoadError for SnapshotOverlaysConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorruptionOverlayConfig {
    logistics_weight: f32,
    trade_weight: f32,
    military_weight: f32,
    governance_weight: f32,
    logistics_spike_multiplier: f32,
    trade_spike_multiplier: f32,
    military_spike_multiplier: f32,
    governance_spike_multiplier: f32,
}

impl CorruptionOverlayConfig {
    pub fn logistics_weight(&self) -> Scalar {
        scalar_from_f32(self.logistics_weight)
    }

    pub fn trade_weight(&self) -> Scalar {
        scalar_from_f32(self.trade_weight)
    }

    pub fn military_weight(&self) -> Scalar {
        scalar_from_f32(self.military_weight)
    }

    pub fn governance_weight(&self) -> Scalar {
        scalar_from_f32(self.governance_weight)
    }

    pub fn logistics_spike_multiplier(&self) -> f32 {
        self.logistics_spike_multiplier
    }

    pub fn trade_spike_multiplier(&self) -> f32 {
        self.trade_spike_multiplier
    }

    pub fn military_spike_multiplier(&self) -> f32 {
        self.military_spike_multiplier
    }

    pub fn governance_spike_multiplier(&self) -> f32 {
        self.governance_spike_multiplier
    }
}

impl Default for CorruptionOverlayConfig {
    fn default() -> Self {
        Self {
            logistics_weight: 0.35,
            trade_weight: 0.25,
            military_weight: 0.2,
            governance_weight: 0.2,
            logistics_spike_multiplier: 2.0,
            trade_spike_multiplier: 2.0,
            military_spike_multiplier: 1.0,
            governance_spike_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureOverlayConfig {
    hard_tick_bonus_step: f32,
    hard_tick_bonus_cap: f32,
    soft_tick_bonus_step: f32,
    soft_tick_bonus_cap: f32,
}

impl CultureOverlayConfig {
    pub fn hard_tick_bonus_step(&self) -> f32 {
        self.hard_tick_bonus_step
    }

    pub fn hard_tick_bonus_cap(&self) -> f32 {
        self.hard_tick_bonus_cap
    }

    pub fn soft_tick_bonus_step(&self) -> f32 {
        self.soft_tick_bonus_step
    }

    pub fn soft_tick_bonus_cap(&self) -> f32 {
        self.soft_tick_bonus_cap
    }
}

impl Default for CultureOverlayConfig {
    fn default() -> Self {
        Self {
            hard_tick_bonus_step: 0.05,
            hard_tick_bonus_cap: 0.5,
            soft_tick_bonus_step: 0.03,
            soft_tick_bonus_cap: 0.3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MilitaryOverlayConfig {
    size_factor_denominator: f32,
    presence_clamp_max: f32,
    heavy_size_threshold: u32,
    heavy_size_bonus: f32,
    support_clamp_max: f32,
    power_margin_max: f32,
    presence_weight: f32,
    support_weight: f32,
    combined_clamp_max: f32,
}

impl MilitaryOverlayConfig {
    pub fn size_factor_denominator(&self) -> f32 {
        self.size_factor_denominator.max(f32::EPSILON)
    }

    pub fn presence_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.presence_clamp_max)
    }

    pub fn heavy_size_threshold(&self) -> u32 {
        self.heavy_size_threshold
    }

    pub fn heavy_size_bonus(&self) -> Scalar {
        scalar_from_f32(self.heavy_size_bonus)
    }

    pub fn support_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.support_clamp_max)
    }

    pub fn power_margin_max(&self) -> Scalar {
        scalar_from_f32(self.power_margin_max)
    }

    pub fn presence_weight(&self) -> Scalar {
        scalar_from_f32(self.presence_weight)
    }

    pub fn support_weight(&self) -> Scalar {
        scalar_from_f32(self.support_weight)
    }

    pub fn combined_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.combined_clamp_max)
    }
}

impl Default for MilitaryOverlayConfig {
    fn default() -> Self {
        Self {
            size_factor_denominator: 1_500.0,
            presence_clamp_max: 5.0,
            heavy_size_threshold: 2_500,
            heavy_size_bonus: 0.1,
            support_clamp_max: 5.0,
            power_margin_max: 5.0,
            presence_weight: 0.6,
            support_weight: 0.4,
            combined_clamp_max: 5.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FoodOverlayConfig {
    site_land_fraction: f32,
    min_sites: usize,
    default_radius: u32,
    radius_padding: u32,
    min_site_spacing: u32,
    provisions_per_weight: f32,
    trade_goods_per_weight: f32,
    trade_bonus_modules: HashMap<String, f32>,
    fresh_water_site_weight: f32,
}

impl FoodOverlayConfig {
    /// **What share of a map's LAND carries a gathering marker** — the whole site budget, in one
    /// number: `sites = clamp(site_land_fraction × land_tiles, min_sites, land_tiles)`.
    ///
    /// **Of land, not of the grid**, because a marker can only sit on ground and `target_land_pct`
    /// differs per preset (earthlike 0.38, polar_contrast 0.42) — a fraction of the whole grid would
    /// hand the two presets different marker density on the land a player actually walks, for free.
    ///
    /// This replaced a three-number arrangement (`max_total_sites` 90, `land_tiles_per_site` 120,
    /// `min_scaled_sites` 24) whose flat count acted as a **floor**, so the budget scaled *up* on maps
    /// past ~10,800 land tiles and never *down*: a Tiny map got the same 90 markers as the Standard.
    /// One fraction scales in both directions and reads as what it is.
    pub fn site_land_fraction(&self) -> f32 {
        self.site_land_fraction.max(0.0)
    }

    /// The floor under the site budget, so a very small map still carries somewhere to gather. A
    /// floor only — unlike the `max_total_sites` it replaced, it does not bind at Standard size.
    pub fn min_sites(&self) -> usize {
        self.min_sites
    }

    /// **The site budget for a map with this much land.** One seam, so the curation and anything that
    /// reports the budget cannot disagree about it. Clamped to `land_tiles` because a map cannot carry
    /// more markers than it has ground.
    ///
    /// **At least one marker, always.** `site_land_fraction` explicitly permits `0.0` and `min_sites`
    /// is the raw field, so a config of `{"site_land_fraction": 0.0, "min_sites": 0}` would otherwise
    /// budget **zero** markers on any map — and since `food_modules` is the only source the client's
    /// Forage gate reads, that is a world where no band can ever Forage, therefore never Sow, with no
    /// boot warning to say why. The `max_total_sites` this replaced carried the same `.max(1)`.
    pub fn site_budget(&self, land_tiles: usize) -> usize {
        let scaled = (self.site_land_fraction() * land_tiles as f32).round() as usize;
        scaled.max(self.min_sites()).max(1).min(land_tiles.max(1))
    }

    pub fn default_radius(&self) -> u32 {
        self.default_radius
    }

    pub fn radius_padding(&self) -> u32 {
        self.radius_padding
    }

    pub fn provisions_per_weight(&self) -> f32 {
        self.provisions_per_weight.max(0.0)
    }

    pub fn trade_goods_per_weight(&self) -> f32 {
        self.trade_goods_per_weight.max(0.0)
    }

    pub fn trade_bonus_for(&self, module: &FoodModule) -> f32 {
        self.trade_bonus_modules
            .get(module.as_str())
            .copied()
            .unwrap_or(0.0)
    }

    pub fn min_site_spacing(&self) -> u32 {
        self.min_site_spacing.max(1)
    }

    /// **What fresh water is worth to a gathering site, in forage-capacity units** (issue #466).
    ///
    /// The curated marker list is the only ground a player can Forage — and therefore the only
    /// ground they can `Sow` — so a marker that lands away from water puts the whole plant ladder out
    /// of reach on that hex. Site quality is scored as `tile_forage_capacity + this × (the tile is
    /// fresh-watered)`, and `bias_food_sites_toward_fresh_water` moves each marker to the best-scoring
    /// tile in its **own spatial bucket**. Expressed in capacity units so the two terms are directly
    /// comparable: at the shipped value a watered tile outranks a dry one carrying up to this much
    /// more forage, and richer watered ground still outranks poorer watered ground.
    ///
    /// **`0.0` reproduces the pre-#466 map exactly, because the pass early-returns on it.** Note that
    /// the early return is what does it, *not* the scoring: with the bonus off the score is bare
    /// forage capacity, and curation did not pick the highest-capacity hex in each bucket, so a pass
    /// that ran anyway would still relocate (measured: 82 of ~130 markers on one seed). The kill
    /// switch is an explicit branch, and `FoodSiteWaterBiasReport::moved` is the test's evidence
    /// that it fires.
    pub fn fresh_water_site_weight(&self) -> f32 {
        self.fresh_water_site_weight.max(0.0)
    }
}

impl Default for FoodOverlayConfig {
    fn default() -> Self {
        Self {
            site_land_fraction: 0.08,
            min_sites: 24,
            default_radius: 6,
            radius_padding: 2,
            min_site_spacing: 4,
            provisions_per_weight: 120.0,
            trade_goods_per_weight: 35.0,
            trade_bonus_modules: HashMap::from([
                ("coastal_littoral".to_string(), 25.0),
                ("riverine_delta".to_string(), 15.0),
                ("coastal_upwelling".to_string(), 30.0),
            ]),
            fresh_water_site_weight: 60.0,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SnapshotOverlaysConfigHandle(pub Arc<SnapshotOverlaysConfig>);

impl SnapshotOverlaysConfigHandle {
    pub fn new(config: Arc<SnapshotOverlaysConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SnapshotOverlaysConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SnapshotOverlaysConfig>) {
        self.0 = config;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SnapshotOverlaysConfigMetadata {
    path: Option<PathBuf>,
}

impl SnapshotOverlaysConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `SNAPSHOT_OVERLAYS_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_snapshot_overlays_config_from_env(
) -> (Arc<SnapshotOverlaysConfig>, SnapshotOverlaysConfigMetadata) {
    let (config, source) = load_config_from_env(
        "SNAPSHOT_OVERLAYS_CONFIG_PATH",
        "snapshot_overlays_config",
        "src/data/snapshot_overlays_config.json",
        SnapshotOverlaysConfig::builtin,
        SnapshotOverlaysConfig::from_file,
    );
    (config, SnapshotOverlaysConfigMetadata::new(source))
}
