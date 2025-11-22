use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Number, Value};
use thiserror::Error;

use crate::hashing::FnvHasher;

pub const BUILTIN_CRISIS_ARCHETYPES: &str = include_str!("data/crisis_archetypes.json");
pub const BUILTIN_CRISIS_MODIFIERS: &str = include_str!("data/crisis_modifiers.json");
pub const BUILTIN_CRISIS_TELEMETRY_CONFIG: &str = include_str!("data/crisis_telemetry_config.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisArchetypeCatalog {
    pub version: u32,
    pub archetypes: Vec<CrisisArchetype>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CrisisArchetypeCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_CRISIS_ARCHETYPES)
                .expect("builtin crisis archetype catalog should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, CrisisArchetypeCatalogError> {
        let mut catalog: CrisisArchetypeCatalog = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path) -> Result<Self, CrisisArchetypeCatalogError> {
        let contents =
            fs::read_to_string(path).map_err(|source| CrisisArchetypeCatalogError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    pub fn archetype(&self, id: &str) -> Option<&CrisisArchetype> {
        self.archetypes.iter().find(|entry| entry.id == id)
    }

    fn validate(&mut self) -> Result<(), CrisisArchetypeCatalogError> {
        let mut seen = HashSet::new();
        let mut expanded = Vec::new();
        for mut archetype in self.archetypes.drain(..) {
            archetype.normalize();
            let generator = archetype.generator.take();
            if !seen.insert(archetype.id.clone()) {
                return Err(CrisisArchetypeCatalogError::Duplicate {
                    id: archetype.id.clone(),
                });
            }
            if let Some(generator) = generator {
                for mut variant in generator.generate_variants(&archetype) {
                    variant.generator = None;
                    variant.normalize();
                    if !seen.insert(variant.id.clone()) {
                        return Err(CrisisArchetypeCatalogError::Duplicate {
                            id: variant.id.clone(),
                        });
                    }
                    expanded.push(variant);
                }
            }
            expanded.push(archetype);
        }
        self.archetypes = expanded;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisArchetype {
    pub id: String,
    pub name: String,
    pub manual_ref: Option<String>,
    pub tags: Vec<String>,
    pub synopsis: Option<String>,
    pub generator: Option<CrisisArchetypeGeneratorEntry>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CrisisArchetype {
    fn normalize(&mut self) {
        self.id.make_ascii_lowercase();
        self.tags
            .iter_mut()
            .for_each(|tag| tag.make_ascii_lowercase());
    }
}

#[derive(Debug, Error)]
pub enum CrisisArchetypeCatalogError {
    #[error("failed to parse crisis archetype catalog: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read crisis archetype catalog from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("duplicate crisis archetype id {id}")]
    Duplicate { id: String },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisModifierCatalog {
    pub version: u32,
    pub modifiers: Vec<CrisisModifier>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CrisisModifierCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_CRISIS_MODIFIERS)
                .expect("builtin crisis modifier catalog should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, CrisisModifierCatalogError> {
        let mut catalog: CrisisModifierCatalog = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path) -> Result<Self, CrisisModifierCatalogError> {
        let contents =
            fs::read_to_string(path).map_err(|source| CrisisModifierCatalogError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    pub fn modifier(&self, id: &str) -> Option<&CrisisModifier> {
        self.modifiers.iter().find(|entry| entry.id == id)
    }

    fn validate(&mut self) -> Result<(), CrisisModifierCatalogError> {
        let mut seen = HashSet::new();
        let mut expanded = Vec::new();
        for mut modifier in self.modifiers.drain(..) {
            modifier.normalize();
            let generator = modifier.generator.take();
            if !seen.insert(modifier.id.clone()) {
                return Err(CrisisModifierCatalogError::Duplicate {
                    id: modifier.id.clone(),
                });
            }
            if let Some(generator) = generator {
                for mut variant in generator.generate_variants(&modifier) {
                    variant.generator = None;
                    variant.normalize();
                    if !seen.insert(variant.id.clone()) {
                        return Err(CrisisModifierCatalogError::Duplicate {
                            id: variant.id.clone(),
                        });
                    }
                    expanded.push(variant);
                }
            }
            expanded.push(modifier);
        }
        self.modifiers = expanded;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisModifier {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub generator: Option<CrisisModifierGeneratorEntry>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CrisisModifier {
    fn normalize(&mut self) {
        self.id.make_ascii_lowercase();
        if let Some(category) = &mut self.category {
            category.make_ascii_lowercase();
        }
        self.tags
            .iter_mut()
            .for_each(|tag| tag.make_ascii_lowercase());
    }
}

#[derive(Debug, Error)]
pub enum CrisisModifierCatalogError {
    #[error("failed to parse crisis modifier catalog: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read crisis modifier catalog from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("duplicate crisis modifier id {id}")]
    Duplicate { id: String },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisArchetypeGeneratorEntry {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub variant_count: u8,
    pub id_pattern: Option<String>,
    pub name_suffixes: Option<Vec<String>>,
    pub synopsis_pool: Option<Vec<String>>,
    pub add_tags: Option<Vec<String>>,
    pub seed_offset: Option<u64>,
    pub propagation: Option<CrisisArchetypePropagationRanges>,
    pub telemetry: Option<CrisisArchetypeTelemetryRanges>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisArchetypePropagationRanges {
    pub base_r0: Option<FloatBand>,
    pub max_r0: Option<FloatBand>,
    pub base_growth: Option<FloatBand>,
    pub incident_acceleration: Option<FloatBand>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisArchetypeTelemetryRanges {
    pub r0_weight: Option<FloatBand>,
    pub grid_stress_weight: Option<FloatBand>,
    pub queue_pressure_weight: Option<FloatBand>,
    pub swarms_active_weight: Option<FloatBand>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisModifierGeneratorEntry {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub variant_count: u8,
    pub id_pattern: Option<String>,
    pub name_suffixes: Option<Vec<String>>,
    pub description_pool: Option<Vec<String>>,
    pub add_tags: Option<Vec<String>>,
    pub seed_offset: Option<u64>,
    pub effects: BTreeMap<String, FloatBand>,
    pub decay: Option<CrisisModifierDecayRanges>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CrisisModifierDecayRanges {
    pub per_tick: Option<FloatBand>,
    pub ticks: Option<FloatBand>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FloatBand {
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl FloatBand {
    fn sample(&self, rng: &mut SmallRng, fallback: f32) -> f32 {
        let min = self.min.unwrap_or(fallback);
        let max = self.max.unwrap_or(fallback);
        if min == max {
            min
        } else if min > max {
            rng.gen_range(max..=min)
        } else {
            rng.gen_range(min..=max)
        }
    }
}

impl CrisisArchetypeGeneratorEntry {
    pub fn generate_variants(&self, base: &CrisisArchetype) -> Vec<CrisisArchetype> {
        if !self.enabled || self.variant_count == 0 {
            return Vec::new();
        }

        let seed_offset = self
            .seed_offset
            .unwrap_or_else(|| hash_identifier(&base.id));
        let base_propagation = base
            .extra
            .get("propagation")
            .and_then(|value| value.as_object());
        let base_telemetry = base
            .extra
            .get("telemetry")
            .and_then(|value| value.as_object());

        let mut variants = Vec::with_capacity(self.variant_count as usize);
        for index in 0..self.variant_count {
            let mut rng = rng_for_variant(seed_offset, index);
            let mut variant = base.clone();

            variant.id = self.variant_id(&base.id, index);
            variant.name = self.variant_name(&base.name, index, &mut rng);
            variant.synopsis = self.variant_synopsis(&base.synopsis, &mut rng);
            variant.tags = merge_tags(&base.tags, self.add_tags.as_ref());

            let mut extra = base.extra.clone();
            if let Some(ranges) = &self.propagation {
                apply_archetype_propagation(&mut extra, ranges, base_propagation, &mut rng);
            }
            if let Some(ranges) = &self.telemetry {
                apply_archetype_telemetry(&mut extra, ranges, base_telemetry, &mut rng);
            }
            variant.extra = extra;
            variants.push(variant);
        }

        variants
    }

    fn variant_id(&self, base_id: &str, index: u8) -> String {
        if let Some(pattern) = &self.id_pattern {
            pattern
                .replace("{base}", base_id)
                .replace("{index}", &(index as usize + 1).to_string())
        } else {
            format!("{}::{}", base_id, index as usize + 1)
        }
    }

    fn variant_name(&self, base_name: &str, index: u8, rng: &mut SmallRng) -> String {
        if let Some(pool) = &self.name_suffixes {
            if let Some(suffix) = pool.choose(rng) {
                return format!("{} — {}", base_name, suffix);
            }
        }
        format!("{} Variant {}", base_name, index as usize + 1)
    }

    fn variant_synopsis(
        &self,
        base_synopsis: &Option<String>,
        rng: &mut SmallRng,
    ) -> Option<String> {
        if let Some(pool) = &self.synopsis_pool {
            if let Some(picked) = pool.choose(rng) {
                return Some(picked.clone());
            }
        }
        base_synopsis.clone()
    }
}

impl CrisisModifierGeneratorEntry {
    pub fn generate_variants(&self, base: &CrisisModifier) -> Vec<CrisisModifier> {
        if !self.enabled || self.variant_count == 0 {
            return Vec::new();
        }

        let seed_offset = self
            .seed_offset
            .unwrap_or_else(|| hash_identifier(&base.id));
        let base_effects = base
            .extra
            .get("effects")
            .and_then(|value| value.as_object());
        let base_decay = base.extra.get("decay").and_then(|value| value.as_object());

        let mut variants = Vec::with_capacity(self.variant_count as usize);
        for index in 0..self.variant_count {
            let mut rng = rng_for_variant(seed_offset, index);
            let mut variant = base.clone();

            variant.id = self.variant_id(&base.id, index);
            variant.name = self.variant_name(&base.name, index, &mut rng);
            variant.description = self.variant_description(&base.description, &mut rng);
            variant.tags = merge_tags(&base.tags, self.add_tags.as_ref());

            let mut extra = base.extra.clone();
            apply_modifier_effects(&mut extra, &self.effects, base_effects, &mut rng);
            if let Some(decay) = &self.decay {
                apply_modifier_decay(&mut extra, decay, base_decay, &mut rng);
            }
            variant.extra = extra;

            variants.push(variant);
        }

        variants
    }

    fn variant_id(&self, base_id: &str, index: u8) -> String {
        if let Some(pattern) = &self.id_pattern {
            pattern
                .replace("{base}", base_id)
                .replace("{index}", &(index as usize + 1).to_string())
        } else {
            format!("{}::{}", base_id, index as usize + 1)
        }
    }

    fn variant_name(&self, base_name: &str, index: u8, rng: &mut SmallRng) -> String {
        if let Some(pool) = &self.name_suffixes {
            if let Some(suffix) = pool.choose(rng) {
                return format!("{} — {}", base_name, suffix);
            }
        }
        format!("{} Variant {}", base_name, index as usize + 1)
    }

    fn variant_description(
        &self,
        base_description: &Option<String>,
        rng: &mut SmallRng,
    ) -> Option<String> {
        if let Some(pool) = &self.description_pool {
            if let Some(picked) = pool.choose(rng) {
                return Some(picked.clone());
            }
        }
        base_description.clone()
    }
}

fn rng_for_variant(seed_offset: u64, index: u8) -> SmallRng {
    let seed = seed_offset ^ ((index as u64 + 1) << 12);
    SmallRng::seed_from_u64(seed)
}

fn merge_tags(base_tags: &[String], additions: Option<&Vec<String>>) -> Vec<String> {
    if let Some(additions) = additions {
        let mut existing: HashSet<String> = base_tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect();
        let mut merged = base_tags.to_vec();
        for tag in additions {
            let lowered = tag.to_ascii_lowercase();
            if existing.insert(lowered) {
                merged.push(tag.clone());
            }
        }
        merged
    } else {
        base_tags.to_vec()
    }
}

fn apply_archetype_propagation(
    extra: &mut BTreeMap<String, Value>,
    ranges: &CrisisArchetypePropagationRanges,
    base_map: Option<&JsonMap<String, Value>>,
    rng: &mut SmallRng,
) {
    if !ranges.has_values() {
        return;
    }

    let propagation = extra
        .entry("propagation".to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));
    let obj = propagation
        .as_object_mut()
        .expect("propagation should be an object");

    if let Some(band) = &ranges.base_r0 {
        let fallback = lookup_f32(base_map, "base_r0").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "base_r0".to_string(),
            value_like(base_map, "base_r0", sampled),
        );
    }

    if let Some(band) = &ranges.max_r0 {
        let fallback = lookup_f32(base_map, "max_r0").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "max_r0".to_string(),
            value_like(base_map, "max_r0", sampled),
        );
    }

    if let Some(band) = &ranges.base_growth {
        let fallback = lookup_f32(base_map, "base_growth").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "base_growth".to_string(),
            value_like(base_map, "base_growth", sampled),
        );
    }

    if let Some(band) = &ranges.incident_acceleration {
        let fallback = lookup_f32(base_map, "incident_acceleration").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "incident_acceleration".to_string(),
            value_like(base_map, "incident_acceleration", sampled),
        );
    }
}

fn apply_archetype_telemetry(
    extra: &mut BTreeMap<String, Value>,
    ranges: &CrisisArchetypeTelemetryRanges,
    base_map: Option<&JsonMap<String, Value>>,
    rng: &mut SmallRng,
) {
    if !ranges.has_values() {
        return;
    }

    let telemetry = extra
        .entry("telemetry".to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));
    let obj = telemetry
        .as_object_mut()
        .expect("telemetry should be an object");

    if let Some(band) = &ranges.r0_weight {
        let fallback = lookup_f32(base_map, "r0_weight").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "r0_weight".to_string(),
            value_like(base_map, "r0_weight", sampled),
        );
    }
    if let Some(band) = &ranges.grid_stress_weight {
        let fallback = lookup_f32(base_map, "grid_stress_weight").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "grid_stress_weight".to_string(),
            value_like(base_map, "grid_stress_weight", sampled),
        );
    }
    if let Some(band) = &ranges.queue_pressure_weight {
        let fallback = lookup_f32(base_map, "queue_pressure_weight").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "queue_pressure_weight".to_string(),
            value_like(base_map, "queue_pressure_weight", sampled),
        );
    }
    if let Some(band) = &ranges.swarms_active_weight {
        let fallback = lookup_f32(base_map, "swarms_active_weight").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "swarms_active_weight".to_string(),
            value_like(base_map, "swarms_active_weight", sampled),
        );
    }
}

fn apply_modifier_effects(
    extra: &mut BTreeMap<String, Value>,
    bands: &BTreeMap<String, FloatBand>,
    base_map: Option<&JsonMap<String, Value>>,
    rng: &mut SmallRng,
) {
    if bands.is_empty() {
        return;
    }

    let effects = extra
        .entry("effects".to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));
    let obj = effects
        .as_object_mut()
        .expect("effects should be an object");

    for (key, band) in bands {
        let fallback = lookup_f32(base_map, key).unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(key.clone(), value_like(base_map, key, sampled));
    }
}

fn apply_modifier_decay(
    extra: &mut BTreeMap<String, Value>,
    ranges: &CrisisModifierDecayRanges,
    base_map: Option<&JsonMap<String, Value>>,
    rng: &mut SmallRng,
) {
    if !ranges.has_values() {
        return;
    }

    let decay = extra
        .entry("decay".to_string())
        .or_insert_with(|| Value::Object(JsonMap::new()));
    let obj = decay.as_object_mut().expect("decay should be an object");

    if let Some(band) = &ranges.per_tick {
        let fallback = lookup_f32(base_map, "per_tick").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert(
            "per_tick".to_string(),
            value_like(base_map, "per_tick", sampled),
        );
    }
    if let Some(band) = &ranges.ticks {
        let fallback = lookup_f32(base_map, "ticks").unwrap_or(0.0);
        let sampled = band.sample(rng, fallback);
        obj.insert("ticks".to_string(), value_like(base_map, "ticks", sampled));
    }
}

fn lookup_f32(map: Option<&JsonMap<String, Value>>, key: &str) -> Option<f32> {
    map.and_then(|object| object.get(key))
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
}

fn value_like(base_map: Option<&JsonMap<String, Value>>, key: &str, sample: f32) -> Value {
    let base_value = base_map.and_then(|object| object.get(key));
    json_number_like(base_value, sample)
}

fn json_number_like(base: Option<&Value>, sample: f32) -> Value {
    let mut value = sample as f64;
    if !value.is_finite() {
        value = base.and_then(|existing| existing.as_f64()).unwrap_or(0.0);
    }

    if let Some(Value::Number(number)) = base {
        if number.is_i64() {
            let rounded = value.round() as i64;
            return Value::Number(Number::from(rounded));
        }
        if number.is_u64() {
            let clamped = value.round().max(0.0) as u64;
            return Value::Number(Number::from(clamped));
        }
    }

    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or_else(|| Value::Number(Number::from(0)))
}

impl CrisisArchetypePropagationRanges {
    fn has_values(&self) -> bool {
        self.base_r0.is_some()
            || self.max_r0.is_some()
            || self.base_growth.is_some()
            || self.incident_acceleration.is_some()
    }
}

impl CrisisArchetypeTelemetryRanges {
    fn has_values(&self) -> bool {
        self.r0_weight.is_some()
            || self.grid_stress_weight.is_some()
            || self.queue_pressure_weight.is_some()
            || self.swarms_active_weight.is_some()
    }
}

impl CrisisModifierDecayRanges {
    fn has_values(&self) -> bool {
        self.per_tick.is_some() || self.ticks.is_some()
    }
}

const fn default_true() -> bool {
    true
}

fn hash_identifier(identifier: &str) -> u64 {
    let mut hasher = FnvHasher::new();
    identifier.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CrisisTelemetryConfig {
    pub ema_alpha: f32,
    pub history_depth: usize,
    pub trend_window: usize,
    pub stale_tick_warning: u64,
    pub stale_tick_critical: u64,
    pub alert_cooldown_ticks: u64,
    pub gauges: BTreeMap<String, CrisisTelemetryThreshold>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl CrisisTelemetryConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_CRISIS_TELEMETRY_CONFIG)
                .expect("builtin crisis telemetry config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, CrisisTelemetryConfigError> {
        let config: CrisisTelemetryConfig = serde_json::from_str(json)?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, CrisisTelemetryConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| CrisisTelemetryConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json_str(&contents)
    }
}

impl Default for CrisisTelemetryConfig {
    fn default() -> Self {
        Self {
            ema_alpha: 0.35,
            history_depth: 6,
            trend_window: 5,
            stale_tick_warning: 6,
            stale_tick_critical: 12,
            alert_cooldown_ticks: 5,
            gauges: {
                let mut map = BTreeMap::new();
                map.insert(
                    "r0".to_string(),
                    CrisisTelemetryThreshold {
                        warn: 0.9,
                        critical: 1.2,
                        ..Default::default()
                    },
                );
                map.insert(
                    "grid_stress_pct".to_string(),
                    CrisisTelemetryThreshold {
                        warn: 70.0,
                        critical: 85.0,
                        ..Default::default()
                    },
                );
                map.insert(
                    "unauthorized_queue_pct".to_string(),
                    CrisisTelemetryThreshold {
                        warn: 10.0,
                        critical: 25.0,
                        ..Default::default()
                    },
                );
                map.insert(
                    "swarms_active".to_string(),
                    CrisisTelemetryThreshold {
                        warn: 2.0,
                        critical: 5.0,
                        ..Default::default()
                    },
                );
                map.insert(
                    "phage_density".to_string(),
                    CrisisTelemetryThreshold {
                        warn: 0.35,
                        critical: 0.6,
                        ..Default::default()
                    },
                );
                map
            },
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CrisisTelemetryThreshold {
    pub warn: f32,
    pub critical: f32,
    pub escalation_delta: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Error)]
pub enum CrisisTelemetryConfigError {
    #[error("failed to parse crisis telemetry config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read crisis telemetry config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Resource, Debug, Clone)]
pub struct CrisisArchetypeCatalogHandle(pub Arc<CrisisArchetypeCatalog>);

impl CrisisArchetypeCatalogHandle {
    pub fn new(catalog: Arc<CrisisArchetypeCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<CrisisArchetypeCatalog> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, catalog: Arc<CrisisArchetypeCatalog>) {
        self.0 = catalog;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CrisisModifierCatalogHandle(pub Arc<CrisisModifierCatalog>);

impl CrisisModifierCatalogHandle {
    pub fn new(catalog: Arc<CrisisModifierCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<CrisisModifierCatalog> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, catalog: Arc<CrisisModifierCatalog>) {
        self.0 = catalog;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CrisisTelemetryConfigHandle(pub Arc<CrisisTelemetryConfig>);

impl CrisisTelemetryConfigHandle {
    pub fn new(config: Arc<CrisisTelemetryConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<CrisisTelemetryConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<CrisisTelemetryConfig>) {
        self.0 = config;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CrisisArchetypeCatalogMetadata {
    path: Option<PathBuf>,
}

impl CrisisArchetypeCatalogMetadata {
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

#[derive(Resource, Debug, Clone)]
pub struct CrisisModifierCatalogMetadata {
    path: Option<PathBuf>,
}

impl CrisisModifierCatalogMetadata {
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

#[derive(Resource, Debug, Clone)]
pub struct CrisisTelemetryConfigMetadata {
    path: Option<PathBuf>,
}

impl CrisisTelemetryConfigMetadata {
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

pub fn load_crisis_archetypes_from_env(
) -> (Arc<CrisisArchetypeCatalog>, CrisisArchetypeCatalogMetadata) {
    load_with_env_paths(
        "CRISIS_ARCHETYPES_PATH",
        "crisis_archetypes",
        CrisisArchetypeCatalog::builtin,
        CrisisArchetypeCatalog::from_file,
        CrisisArchetypeCatalogMetadata::new,
    )
}

pub fn load_crisis_modifiers_from_env(
) -> (Arc<CrisisModifierCatalog>, CrisisModifierCatalogMetadata) {
    load_with_env_paths(
        "CRISIS_MODIFIERS_PATH",
        "crisis_modifiers",
        CrisisModifierCatalog::builtin,
        CrisisModifierCatalog::from_file,
        CrisisModifierCatalogMetadata::new,
    )
}

pub fn load_crisis_telemetry_config_from_env(
) -> (Arc<CrisisTelemetryConfig>, CrisisTelemetryConfigMetadata) {
    load_with_env_paths(
        "CRISIS_TELEMETRY_CONFIG_PATH",
        "crisis_telemetry_config",
        CrisisTelemetryConfig::builtin,
        CrisisTelemetryConfig::from_file,
        CrisisTelemetryConfigMetadata::new,
    )
}

fn load_with_env_paths<T, E, M>(
    env_var: &str,
    label: &'static str,
    builtin: fn() -> Arc<T>,
    from_file: fn(&Path) -> Result<T, E>,
    metadata_ctor: fn(Option<PathBuf>) -> M,
) -> (Arc<T>, M)
where
    E: std::fmt::Display,
{
    let override_path = env::var(env_var).ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("src/data/{label}.json"));

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match from_file(&path) {
            Ok(cfg) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "{label}.loaded=file"
                );
                return (Arc::new(cfg), metadata_ctor(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "{label}.load_failed"
                );
            }
        }
    }

    tracing::info!(
        target: "shadow_scale::config",
        "{label}.loaded=builtin"
    );
    (builtin(), metadata_ctor(None))
}
