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
        let parsed: Self = serde_json::from_str(json)?;
        parsed
            .culture
            .validate()
            .map_err(<serde_json::Error as serde::de::Error>::custom)?;
        Ok(parsed)
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
    /// Culture over a connection: how fast two tied bands grow alike
    /// (`docs/plan_contact_and_logistics.md` §"Settled by #530").
    contact_drift: ContactDriftSettings,
    /// The Syncretic<->Purist value a band must EXCEED for a held schism to split it off as its
    /// own people (#702). A band at or below it is accepting and never splits.
    split_min_purist: f32,
    /// A people's weight of tradition (`docs/plan_minor_peoples.md` §Culture strength).
    strength: CultureStrengthSettings,
    /// How hard one people's culture weighs on another's band, and when it joins
    /// (`docs/plan_minor_peoples.md` §Pressure).
    joining: CultureJoiningSettings,
}

/// The contact-drift lever block (`culture.contact_drift`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ContactDriftSettings {
    /// The share of the gap two equal, neutral bands at a full tie close per turn. `0` turns
    /// contact drift off. Finite and `>= 0`.
    rate: f32,
}

impl ContactDriftSettings {
    pub fn rate(&self) -> f32 {
        self.rate
    }
}

impl Default for ContactDriftSettings {
    fn default() -> Self {
        Self {
            rate: DEFAULT_CONTACT_DRIFT_RATE,
        }
    }
}

/// The culture-strength lever block (`culture.strength`): `base + Σ weight_i × (input_i /
/// reference_i)`, the sedentarization score's weighted-sum shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureStrengthSettings {
    /// What a people with no belief, no ties and no discoveries weighs; keeps a ratio between two
    /// fresh peoples defined. Finite and `> 0`.
    base: f32,
    /// Each input's weight in the sum. Finite and `>= 0`.
    #[serde(default = "default_strength_weights")]
    weights: CultureStrengthInputs,
    /// Each input's reference: the amount that counts as one full unit. Finite and `> 0`.
    #[serde(default = "default_strength_references")]
    references: CultureStrengthInputs,
}

/// One value per culture-strength input (used for both the weights and the references).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CultureStrengthInputs {
    /// Belief held at the anchors of the people's bands, each anchor once.
    belief: f32,
    /// Distinct foreign bands live-tied to the people's bands.
    ties: f32,
    /// Wondrous sites the people has discovered.
    discoveries: f32,
}

impl CultureStrengthInputs {
    pub fn belief(&self) -> f32 {
        self.belief
    }

    pub fn ties(&self) -> f32 {
        self.ties
    }

    pub fn discoveries(&self) -> f32 {
        self.discoveries
    }
}

impl CultureStrengthSettings {
    pub fn base(&self) -> f32 {
        self.base
    }

    pub fn weights(&self) -> &CultureStrengthInputs {
        &self.weights
    }

    pub fn references(&self) -> &CultureStrengthInputs {
        &self.references
    }
}

const DEFAULT_STRENGTH_BASE: f32 = 1.0;
const DEFAULT_STRENGTH_WEIGHT_BELIEF: f32 = 1.0;
const DEFAULT_STRENGTH_WEIGHT_TIES: f32 = 0.5;
const DEFAULT_STRENGTH_WEIGHT_DISCOVERIES: f32 = 0.5;
/// Dead-equivalents of belief that count as one full unit of tradition.
const DEFAULT_STRENGTH_REFERENCE_BELIEF: f32 = 20.0;
/// Foreign bands in touch that count as one full unit.
const DEFAULT_STRENGTH_REFERENCE_TIES: f32 = 4.0;
/// Discovered wondrous sites that count as one full unit.
const DEFAULT_STRENGTH_REFERENCE_DISCOVERIES: f32 = 2.0;

fn default_strength_weights() -> CultureStrengthInputs {
    CultureStrengthInputs {
        belief: DEFAULT_STRENGTH_WEIGHT_BELIEF,
        ties: DEFAULT_STRENGTH_WEIGHT_TIES,
        discoveries: DEFAULT_STRENGTH_WEIGHT_DISCOVERIES,
    }
}

fn default_strength_references() -> CultureStrengthInputs {
    CultureStrengthInputs {
        belief: DEFAULT_STRENGTH_REFERENCE_BELIEF,
        ties: DEFAULT_STRENGTH_REFERENCE_TIES,
        discoveries: DEFAULT_STRENGTH_REFERENCE_DISCOVERIES,
    }
}

impl Default for CultureStrengthSettings {
    fn default() -> Self {
        Self {
            base: DEFAULT_STRENGTH_BASE,
            weights: default_strength_weights(),
            references: default_strength_references(),
        }
    }
}

/// The joining lever block (`culture.joining`): the pressure formula's size exponent, the three
/// phase thresholds, how close a band's culture must be, and how long a condition must hold.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureJoiningSettings {
    /// Exponent on `people_B / people_A`. `0` ignores size; `>= 0`.
    size_exponent: f32,
    /// Pressure at which a band is told it is being pulled.
    pulled: f32,
    /// Pressure at which a close band asks to join.
    asked: f32,
    /// Pressure at which a close band joins without its owner's say.
    forced: f32,
    /// The widest per-axis gap between the band's culture and the other people's average that
    /// still counts as close. `>= 0`.
    max_culture_distance: f32,
    /// Turns the asked / forced condition must hold. `>= 1`.
    turns: u16,
}

impl CultureJoiningSettings {
    pub fn size_exponent(&self) -> f32 {
        self.size_exponent
    }

    pub fn pulled(&self) -> f32 {
        self.pulled
    }

    pub fn asked(&self) -> f32 {
        self.asked
    }

    pub fn forced(&self) -> f32 {
        self.forced
    }

    pub fn max_culture_distance(&self) -> f32 {
        self.max_culture_distance
    }

    pub fn turns(&self) -> u16 {
        self.turns
    }
}

const DEFAULT_JOINING_SIZE_EXPONENT: f32 = 0.5;
const DEFAULT_JOINING_PULLED: f32 = 1.5;
const DEFAULT_JOINING_ASKED: f32 = 2.0;
const DEFAULT_JOINING_FORCED: f32 = 4.0;
const DEFAULT_JOINING_MAX_CULTURE_DISTANCE: f32 = 0.3;
const DEFAULT_JOINING_TURNS: u16 = 10;

impl Default for CultureJoiningSettings {
    fn default() -> Self {
        Self {
            size_exponent: DEFAULT_JOINING_SIZE_EXPONENT,
            pulled: DEFAULT_JOINING_PULLED,
            asked: DEFAULT_JOINING_ASKED,
            forced: DEFAULT_JOINING_FORCED,
            max_culture_distance: DEFAULT_JOINING_MAX_CULTURE_DISTANCE,
            turns: DEFAULT_JOINING_TURNS,
        }
    }
}

/// First guess: half the gap in ~50 turns at a full tie between equal, neutral bands.
const DEFAULT_CONTACT_DRIFT_RATE: f32 = 0.014;

/// Neutral on the Syncretic<->Purist axis: any purist band splits.
const DEFAULT_SPLIT_MIN_PURIST: f32 = 0.0;

impl CultureSeverityConfig {
    pub fn contact_drift(&self) -> &ContactDriftSettings {
        &self.contact_drift
    }

    pub fn split_min_purist(&self) -> f32 {
        self.split_min_purist
    }

    pub fn strength(&self) -> &CultureStrengthSettings {
        &self.strength
    }

    pub fn joining(&self) -> &CultureJoiningSettings {
        &self.joining
    }

    /// Reject levers the contact-drift and split rules cannot read.
    fn validate(&self) -> Result<(), String> {
        let rate = self.contact_drift.rate;
        if !rate.is_finite() || rate < 0.0 {
            return Err(format!(
                "culture.contact_drift.rate must be finite and >= 0, got {rate}"
            ));
        }
        if !self.split_min_purist.is_finite() {
            return Err(format!(
                "culture.split_min_purist must be finite, got {}",
                self.split_min_purist
            ));
        }
        self.validate_strength()?;
        self.validate_joining()
    }

    fn validate_strength(&self) -> Result<(), String> {
        let strength = &self.strength;
        let positive = |name: &str, value: f32| {
            if value.is_finite() && value > 0.0 {
                Ok(())
            } else {
                Err(format!(
                    "culture.strength.{name} must be finite and > 0, got {value}"
                ))
            }
        };
        positive("base", strength.base)?;
        positive("references.belief", strength.references.belief)?;
        positive("references.ties", strength.references.ties)?;
        positive("references.discoveries", strength.references.discoveries)?;
        let non_negative = |name: &str, value: f32| {
            if value.is_finite() && value >= 0.0 {
                Ok(())
            } else {
                Err(format!(
                    "culture.strength.{name} must be finite and >= 0, got {value}"
                ))
            }
        };
        non_negative("weights.belief", strength.weights.belief)?;
        non_negative("weights.ties", strength.weights.ties)?;
        non_negative("weights.discoveries", strength.weights.discoveries)
    }

    fn validate_joining(&self) -> Result<(), String> {
        let joining = &self.joining;
        let (pulled, asked, forced) = (joining.pulled, joining.asked, joining.forced);
        if !(pulled.is_finite() && asked.is_finite() && forced.is_finite())
            || pulled <= 0.0
            || pulled > asked
            || asked > forced
        {
            return Err(format!(
                "culture.joining needs 0 < pulled <= asked <= forced, got {pulled} / {asked} / {forced}"
            ));
        }
        if !joining.max_culture_distance.is_finite() || joining.max_culture_distance < 0.0 {
            return Err(format!(
                "culture.joining.max_culture_distance must be finite and >= 0, got {}",
                joining.max_culture_distance
            ));
        }
        if joining.turns < 1 {
            return Err("culture.joining.turns must be >= 1".to_string());
        }
        if !joining.size_exponent.is_finite() || joining.size_exponent < 0.0 {
            return Err(format!(
                "culture.joining.size_exponent must be finite and >= 0, got {}",
                joining.size_exponent
            ));
        }
        Ok(())
    }

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
            contact_drift: ContactDriftSettings::default(),
            split_min_purist: DEFAULT_SPLIT_MIN_PURIST,
            strength: CultureStrengthSettings::default(),
            joining: CultureJoiningSettings::default(),
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

#[cfg(test)]
mod joining_tests {
    use super::*;

    fn with_culture(block: &str) -> Result<CultureCorruptionConfig, serde_json::Error> {
        CultureCorruptionConfig::from_json_str(&format!(r#"{{ "culture": {block} }}"#))
    }

    #[test]
    fn the_shipped_json_equals_the_defaults() {
        let shipped = CultureCorruptionConfig::from_json_str(BUILTIN_CULTURE_CORRUPTION_CONFIG)
            .expect("the shipped config parses");
        let default = CultureSeverityConfig::default();
        let (s, d) = (shipped.culture().strength(), default.strength());
        assert_eq!(s.base(), d.base());
        assert_eq!(s.weights().ties(), d.weights().ties());
        assert_eq!(s.references().belief(), d.references().belief());
        let (j, k) = (shipped.culture().joining(), default.joining());
        assert_eq!(
            (j.pulled(), j.asked(), j.forced(), j.turns()),
            (k.pulled(), k.asked(), k.forced(), k.turns())
        );
        assert_eq!(j.max_culture_distance(), k.max_culture_distance());
        assert_eq!(j.size_exponent(), k.size_exponent());
    }

    #[test]
    fn the_levers_the_pressure_rule_cannot_read_are_refused() {
        for bad in [
            r#"{ "strength": { "base": 0.0 } }"#,
            r#"{ "strength": { "references": { "belief": 0.0, "ties": 4.0, "discoveries": 2.0 } } }"#,
            r#"{ "strength": { "weights": { "belief": -1.0, "ties": 0.5, "discoveries": 0.5 } } }"#,
            r#"{ "joining": { "pulled": 0.0 } }"#,
            r#"{ "joining": { "pulled": 3.0 } }"#,
            r#"{ "joining": { "asked": 5.0 } }"#,
            r#"{ "joining": { "max_culture_distance": -0.1 } }"#,
            r#"{ "joining": { "turns": 0 } }"#,
            r#"{ "joining": { "size_exponent": -1.0 } }"#,
        ] {
            assert!(with_culture(bad).is_err(), "{bad} should be refused");
        }
        assert!(with_culture(r#"{ "joining": { "size_exponent": 0.0 } }"#).is_ok());
    }
}
