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

    pub fn culture_mut(&mut self) -> &mut CultureSeverityConfig {
        &mut self.culture
    }

    pub fn corruption(&self) -> &CorruptionSeverityConfig {
        &self.corruption
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureSeverityConfig {
    trust_axis: usize,
    propagation: CulturePropagationSettings,
    drift_warning: CultureTensionTuning,
    assimilation_push: CultureTensionTuning,
    schism_risk: CultureTensionTuning,
}

impl CultureSeverityConfig {
    pub fn trust_axis(&self) -> usize {
        self.trust_axis
    }

    pub fn propagation(&self) -> &CulturePropagationSettings {
        &self.propagation
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
            propagation: CulturePropagationSettings::default(),
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
pub struct CulturePropagationSettings {
    global: CultureScopePropagation,
    regional: CultureScopePropagation,
    local: CultureScopePropagation,
    /// The band scope: a band's culture, carried with it as it moves.
    ///
    /// **A band is slower than a tile** (0.20 against local's 0.40) because it carries cultural
    /// memory across a move — re-parenting a band to a new province must make it *lag*, not snap.
    /// And unlike every other scope its trigger ticks are `> 1`, so a band passing briefly through
    /// a foreign province does not immediately read as a schism.
    band: CultureScopePropagation,
    /// How far a band's own character pulls it off its province, per trait axis — the amplitude of
    /// `culture::seeded_modifiers_for_band`.
    ///
    /// A lever rather than a constant because it sets how far bands drift and therefore **how often
    /// schism fires**. At `0.0` every band converges on its province and they are all identical.
    band_character_amplitude: f32,
    /// How fast culture responds to influencer resonance, as the per-turn rate of a low-pass
    /// filter on the resonance vector (`smoothed += (raw - smoothed) * rate`).
    ///
    /// **This is the "culture is slow" dial, and it is a rate, not a scale.** Scaling the
    /// resonance down would cap how far influencers can ever move a culture; smoothing caps only
    /// how fast. Sustained pressure still reaches full magnitude — it takes roughly `1/rate` turns
    /// to get there — while turn-to-turn churn in who is influential is damped away.
    ///
    /// Measured motivation (#386, `docs/plan_delta_streaming.md` §3.6): with the resonance applied
    /// raw, a local layer's traits moved up to **0.12 per turn** against magnitudes of ~0.05-0.13,
    /// so a trait could invert in a couple of turns and no layer ever settled.
    resonance_response: f32,
}

impl CulturePropagationSettings {
    pub fn global(&self) -> &CultureScopePropagation {
        &self.global
    }

    pub fn regional(&self) -> &CultureScopePropagation {
        &self.regional
    }

    pub fn local(&self) -> &CultureScopePropagation {
        &self.local
    }

    pub fn band(&self) -> &CultureScopePropagation {
        &self.band
    }

    pub fn resonance_response(&self) -> f32 {
        self.resonance_response
    }

    pub fn band_character_amplitude(&self) -> f32 {
        self.band_character_amplitude
    }
}

impl Default for CulturePropagationSettings {
    fn default() -> Self {
        Self {
            global: CultureScopePropagation {
                elasticity: 0.10,
                soft_threshold: 0.6,
                hard_threshold: 1.2,
                soft_trigger_ticks: 1,
                hard_trigger_ticks: 1,
            },
            regional: CultureScopePropagation {
                elasticity: 0.25,
                soft_threshold: 0.6,
                hard_threshold: 1.2,
                soft_trigger_ticks: 1,
                hard_trigger_ticks: 1,
            },
            local: CultureScopePropagation {
                elasticity: 0.40,
                soft_threshold: 0.6,
                hard_threshold: 1.2,
                soft_trigger_ticks: 1,
                hard_trigger_ticks: 1,
            },
            band: CultureScopePropagation {
                elasticity: DEFAULT_BAND_ELASTICITY,
                soft_threshold: 0.6,
                hard_threshold: 1.2,
                soft_trigger_ticks: DEFAULT_BAND_SOFT_TRIGGER_TICKS,
                hard_trigger_ticks: DEFAULT_BAND_HARD_TRIGGER_TICKS,
            },
            band_character_amplitude: DEFAULT_BAND_CHARACTER_AMPLITUDE,
            resonance_response: DEFAULT_RESONANCE_RESPONSE,
        }
    }
}

/// Culture reaches a sustained influencer pressure over roughly `1 / 0.08` = ~12 turns.
pub const DEFAULT_RESONANCE_RESPONSE: f32 = 0.08;

/// A band chases its province half as fast as a tile does (local is 0.40): a band carries its
/// culture with it, so arriving somewhere new has to lag rather than snap.
pub const DEFAULT_BAND_ELASTICITY: f32 = 0.20;

/// Unlike every other scope, a band's alerts wait several turns — a band crossing a foreign
/// province for a turn or two is a journey, not a drift warning.
pub const DEFAULT_BAND_SOFT_TRIGGER_TICKS: u16 = 3;

/// …and a schism has to be a settled fact about a band, not a waypoint on its route.
pub const DEFAULT_BAND_HARD_TRIGGER_TICKS: u16 = 5;

/// Per-axis amplitude of a band's own character offset. Matches the per-tile amplitude in
/// `seeded_modifiers_for_position`, so a band is as distinct from its province as a tile is.
pub const DEFAULT_BAND_CHARACTER_AMPLITUDE: f32 = 0.2;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureScopePropagation {
    elasticity: f32,
    soft_threshold: f32,
    hard_threshold: f32,
    soft_trigger_ticks: u16,
    hard_trigger_ticks: u16,
}

impl CultureScopePropagation {
    pub fn elasticity(&self) -> f32 {
        self.elasticity
    }

    pub fn soft_threshold(&self) -> f32 {
        self.soft_threshold
    }

    pub fn hard_threshold(&self) -> f32 {
        self.hard_threshold
    }

    pub fn soft_trigger_ticks(&self) -> u16 {
        self.soft_trigger_ticks
    }

    pub fn hard_trigger_ticks(&self) -> u16 {
        self.hard_trigger_ticks
    }
}

impl Default for CultureScopePropagation {
    fn default() -> Self {
        Self {
            elasticity: 0.25,
            soft_threshold: 0.6,
            hard_threshold: 1.2,
            soft_trigger_ticks: 1,
            hard_trigger_ticks: 1,
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
