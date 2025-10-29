use std::sync::Arc;

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::scalar::{scalar_from_f32, Scalar};

pub const BUILTIN_CULTURE_CORRUPTION_CONFIG: &str =
    include_str!("data/culture_corruption_config.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CultureCorruptionConfig {
    culture: CultureSeverityConfig,
    corruption: CorruptionSeverityConfig,
}

impl CultureCorruptionConfig {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn culture(&self) -> &CultureSeverityConfig {
        &self.culture
    }

    pub fn corruption(&self) -> &CorruptionSeverityConfig {
        &self.corruption
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureSeverityConfig {
    trust_axis: usize,
    drift_warning: CultureTensionTuning,
    assimilation_push: CultureTensionTuning,
    schism_risk: CultureTensionTuning,
}

impl CultureSeverityConfig {
    pub fn trust_axis(&self) -> usize {
        self.trust_axis
    }

    pub fn drift_warning(&self) -> &CultureTensionTuning {
        &self.drift_warning
    }

    pub fn assimilation_push(&self) -> &CultureTensionTuning {
        &self.assimilation_push
    }

    pub fn schism_risk(&self) -> &CultureTensionTuning {
        &self.schism_risk
    }
}

impl Default for CultureSeverityConfig {
    fn default() -> Self {
        Self {
            trust_axis: 1,
            drift_warning: CultureTensionTuning {
                severity_min: 0.0,
                severity_max: 3.0,
                incident_delta_scale: 0.02,
                incident_delta_min: 0.0,
                incident_delta_max: 0.08,
            },
            assimilation_push: CultureTensionTuning {
                severity_min: 0.0,
                severity_max: 3.0,
                incident_delta_scale: 0.01,
                incident_delta_min: 0.0,
                incident_delta_max: 0.05,
            },
            schism_risk: CultureTensionTuning {
                severity_min: 0.5,
                severity_max: 4.0,
                incident_delta_scale: 0.03,
                incident_delta_min: 0.05,
                incident_delta_max: 0.15,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureTensionTuning {
    severity_min: f32,
    severity_max: f32,
    incident_delta_scale: f32,
    incident_delta_min: f32,
    incident_delta_max: f32,
}

impl CultureTensionTuning {
    pub fn delta_for_magnitude(&self, magnitude: Scalar) -> Scalar {
        let severity = magnitude
            .to_f32()
            .abs()
            .clamp(self.severity_min, self.severity_max);
        let scaled = severity * self.incident_delta_scale;
        let clamped = scaled.clamp(self.incident_delta_min, self.incident_delta_max);
        scalar_from_f32(clamped)
    }

    pub fn severity_range(&self) -> (f32, f32) {
        (self.severity_min, self.severity_max)
    }

    pub fn delta_scale(&self) -> f32 {
        self.incident_delta_scale
    }

    pub fn delta_range(&self) -> (f32, f32) {
        (self.incident_delta_min, self.incident_delta_max)
    }
}

impl Default for CultureTensionTuning {
    fn default() -> Self {
        Self {
            severity_min: 0.0,
            severity_max: 3.0,
            incident_delta_scale: 0.02,
            incident_delta_min: 0.0,
            incident_delta_max: 0.08,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorruptionSeverityConfig {
    trust_axis: usize,
    sentiment_delta_min: f32,
    sentiment_delta_max: f32,
    max_penalty_ratio: f32,
    min_output_multiplier: f32,
}

impl CorruptionSeverityConfig {
    pub fn trust_axis(&self) -> usize {
        self.trust_axis
    }

    pub fn sentiment_delta_bounds(&self) -> (Scalar, Scalar) {
        (
            scalar_from_f32(self.sentiment_delta_min),
            scalar_from_f32(self.sentiment_delta_max),
        )
    }

    pub fn max_penalty_ratio(&self) -> Scalar {
        scalar_from_f32(self.max_penalty_ratio)
    }

    pub fn min_output_multiplier(&self) -> Scalar {
        scalar_from_f32(self.min_output_multiplier)
    }
}

impl Default for CorruptionSeverityConfig {
    fn default() -> Self {
        Self {
            trust_axis: 1,
            sentiment_delta_min: -0.5,
            sentiment_delta_max: 0.5,
            max_penalty_ratio: 0.9,
            min_output_multiplier: 0.1,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CultureCorruptionConfigHandle(pub Arc<CultureCorruptionConfig>);

impl CultureCorruptionConfigHandle {
    pub fn new(config: Arc<CultureCorruptionConfig>) -> Self {
        Self(config)
    }

    pub fn load_builtin() -> Self {
        let parsed = CultureCorruptionConfig::from_json_str(BUILTIN_CULTURE_CORRUPTION_CONFIG)
            .unwrap_or_else(|err| {
                panic!("failed to parse builtin culture corruption config: {err}")
            });
        Self(Arc::new(parsed))
    }

    pub fn config(&self) -> &CultureCorruptionConfig {
        &self.0
    }

    pub fn get(&self) -> Arc<CultureCorruptionConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace_from_json(
        &mut self,
        json: &str,
    ) -> Result<Arc<CultureCorruptionConfig>, serde_json::Error> {
        let parsed = CultureCorruptionConfig::from_json_str(json)?;
        let shared = Arc::new(parsed);
        self.0 = Arc::clone(&shared);
        Ok(shared)
    }
}
