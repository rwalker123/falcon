use std::{
    collections::{BTreeMap, HashMap},
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

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
        let mut seen = HashMap::new();
        for archetype in &mut self.archetypes {
            archetype.normalize();
            if let Some(previous) = seen.insert(archetype.id.clone(), true) {
                if previous {
                    return Err(CrisisArchetypeCatalogError::Duplicate {
                        id: archetype.id.clone(),
                    });
                }
            }
        }
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
        let mut seen = HashMap::new();
        for modifier in &mut self.modifiers {
            modifier.normalize();
            if let Some(previous) = seen.insert(modifier.id.clone(), true) {
                if previous {
                    return Err(CrisisModifierCatalogError::Duplicate {
                        id: modifier.id.clone(),
                    });
                }
            }
        }
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
