//! Tunables for The Telling beat engine.
//!
//! Loaded from `data/beat_config.json`. Mirrors the `sedentarization_config.rs` loader
//! (baked-in builtin + optional file/env override) and the `fauna_config.rs` validation
//! convention (`validate()` runs inside `from_json_str`, so *every* load path is covered; a
//! broken invariant is logged at **error** level and the known-good builtin is used).
//!
//! Design: `docs/plan_the_telling.md` §2c/§3.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use super::{catalog::BeatTier, predicate::Predicate};

pub const BUILTIN_BEAT_CONFIG: &str = include_str!("../data/beat_config.json");

/// Per-tier scalar (the `{ambient, beat, fork}` shape the JSON uses for both budget tables).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct TierScalars {
    pub ambient: u32,
    pub beat: u32,
    pub fork: u32,
}

impl TierScalars {
    pub fn for_tier(self, tier: BeatTier) -> u32 {
        match tier {
            BeatTier::Ambient => self.ambient,
            BeatTier::Beat => self.beat,
            BeatTier::Fork => self.fork,
        }
    }
}

impl Default for TierScalars {
    fn default() -> Self {
        Self {
            ambient: 2,
            beat: 1,
            fork: 1,
        }
    }
}

/// How many beats of each tier may fire per turn, and how long a tier rests after one does.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    pub max_per_turn: TierScalars,
    pub global_cooldown_turns: TierScalars,
    /// **The safety valve.** A pending fork older than this auto-resolves to its defer choice.
    /// Forks post for *every* faction, including AI and unattended ones, and the server never
    /// blocks a turn on an answer — without this, a fork posted to a faction with no client would
    /// sit in `pending` forever and accumulate.
    pub fork_expire_turns: u32,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_per_turn: TierScalars::default(),
            global_cooldown_turns: TierScalars {
                ambient: 0,
                beat: 2,
                fork: 10,
            },
            fork_expire_turns: DEFAULT_FORK_EXPIRE_TURNS,
        }
    }
}

/// Three fork cooldowns' worth of patience (`global_cooldown_turns.fork` = 10). Generous enough
/// that a fork never expires under a player who is simply taking their time, short enough that an
/// unattended faction's `pending` list cannot grow without bound.
const DEFAULT_FORK_EXPIRE_TURNS: u32 = 30;

/// Wardrobe selection weighting (`weight = fit × novelty × stance_affinity`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SelectionConfig {
    /// Turns for a used wardrobe entry's novelty to ramp back from `novelty_floor` to 1.0.
    pub novelty_window_turns: u32,
    /// Novelty multiplier immediately after an entry is used.
    pub novelty_floor: f32,
    /// Weight added per matched *soft* tag on a fitting wardrobe entry.
    pub fit_soft_tag_weight: f32,
    /// How hard a stance affinity pulls selection (`telling::stance::affinity_term`).
    pub stance_affinity_weight: f32,
    /// Floor under the stance-affinity term. It **scales** a wrong-stance dressing rather than
    /// eliminating it: the wardrobe pool is small, and hard exclusion risks a beat with nothing
    /// left to dress it in.
    pub stance_affinity_floor: f32,
    /// Entries weighing less than this are dropped from the draw.
    pub min_selection_weight: f32,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            novelty_window_turns: 40,
            novelty_floor: 0.05,
            fit_soft_tag_weight: 0.25,
            stance_affinity_weight: 0.5,
            stance_affinity_floor: 0.1,
            min_selection_weight: 0.001,
        }
    }
}

/// Rolling-history tuning behind the `trend` predicate.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TrendConfig {
    /// Cap on the per-signal sample history the ledger retains.
    pub max_history_turns: u32,
    /// How far a signal must move over the window to count as a trend.
    pub min_delta: f32,
}

impl Default for TrendConfig {
    fn default() -> Self {
        Self {
            max_history_turns: 16,
            min_delta: 0.01,
        }
    }
}

/// One rung of the narrator's **medium** ladder (concept §7: oral saga → painted chronicle →
/// written record). A medium is just a named threshold over signals, so it reuses the predicate
/// evaluator rather than inventing a second condition language.
///
/// **Medium is presentational.** It does *not* select different wardrobe copy — see the note on
/// [`VoiceConfig::mediums`].
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceMedium {
    pub id: String,
    /// The condition that unlocks this medium. Absent on the first (default) entry, which is
    /// always satisfied.
    #[serde(default)]
    pub when: Option<Predicate>,
}

/// Voice registers. Every player-visible string is keyed by register (`docs/plan_the_telling.md`
/// §2d) so the choice stays a data-level toggle rather than a design commitment.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub default_register: String,
    pub registers: Vec<String>,
    /// The medium ladder, ordered **least → most advanced**. The first entry is the default and
    /// needs no `when`; the **highest satisfied** entry wins, and the attained rung never regresses
    /// (a people that learned to write does not forget).
    ///
    /// **Do NOT author per-medium wardrobe strings.** Medium is presentational: it changes how the
    /// telling *looks* to the client and fires a beat when it advances, and that is the whole of
    /// it. Four mediums × two registers per wardrobe entry would be an 8× authoring cost for the
    /// layer's thinnest payoff, and `docs/Emergent Narrative.md` §13 names authoring cost as a real
    /// risk. The index is readable as the `voice.medium_index` signal, which is how the
    /// medium-advance beats gate themselves.
    pub mediums: Vec<VoiceMedium>,
}

/// The rung every telling starts on: a story told aloud, by people, from memory.
pub const DEFAULT_VOICE_MEDIUM: &str = "oral";

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            default_register: "mythic".to_string(),
            registers: vec!["mythic".to_string(), "warm".to_string()],
            mediums: vec![VoiceMedium {
                id: DEFAULT_VOICE_MEDIUM.to_string(),
                when: None,
            }],
        }
    }
}

/// How much the layer remembers (`telling::memory`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// Threads retained per kind. When full, the **least recently referenced** thread is evicted —
    /// a thread the story keeps returning to is the one worth keeping.
    pub max_threads_per_kind: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_threads_per_kind: DEFAULT_MAX_THREADS_PER_KIND,
        }
    }
}

/// Enough that a long campaign's landmarks and finished herds are all still callable, small enough
/// that the ledger stays a *memory* rather than a log — a kind with more than this many live
/// threads has stopped being a set of things the story can meaningfully return to.
const DEFAULT_MAX_THREADS_PER_KIND: u32 = 8;

/// One narrative stance axis and the sim signal backing it (`docs/plan_the_telling.md` §1c).
/// Parsed and validated in PR-A; the vector itself is not populated until PR-B.
#[derive(Debug, Clone, Deserialize)]
pub struct StanceAxis {
    pub id: String,
    /// A registered signal id — validated at load, so a typo fails fast.
    pub signal: String,
    /// `[min, max]` of the backing signal, for normalizing onto the axis.
    pub range: [f32; 2],
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StanceConfig {
    pub axes: Vec<StanceAxis>,
}

/// Root beat-engine configuration. Each block carries its own hand-written `Default`, so the
/// root derive composes them rather than restating the values.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BeatConfig {
    pub budget: BudgetConfig,
    pub selection: SelectionConfig,
    pub trend: TrendConfig,
    pub voice: VoiceConfig,
    pub stance: StanceConfig,
    pub memory: MemoryConfig,
}

impl BeatConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_BEAT_CONFIG).expect("builtin beat config should be valid"),
        )
    }

    /// Parse **and validate**. Every load path goes through here (the `fauna_config.rs`
    /// convention), so an invalid override can never reach the sim.
    pub fn from_json_str(json: &str) -> Result<Self, BeatConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, BeatConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| BeatConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    /// Is `register` one the config declares?
    pub fn has_register(&self, register: &str) -> bool {
        self.registers_contains(register)
    }

    fn registers_contains(&self, register: &str) -> bool {
        self.voice.registers.iter().any(|r| r == register)
    }

    fn validate(&self) -> Result<(), BeatConfigError> {
        let sel = &self.selection;
        if sel.novelty_window_turns == 0 {
            return Err(BeatConfigError::invalid(
                "selection.novelty_window_turns must be > 0 (a zero window divides by zero when \
                 ramping novelty back)",
            ));
        }
        if !(0.0..=1.0).contains(&sel.novelty_floor) {
            return Err(BeatConfigError::invalid(
                "selection.novelty_floor must be within [0, 1]",
            ));
        }
        for (name, weight) in [
            ("selection.fit_soft_tag_weight", sel.fit_soft_tag_weight),
            (
                "selection.stance_affinity_weight",
                sel.stance_affinity_weight,
            ),
            ("selection.stance_affinity_floor", sel.stance_affinity_floor),
            ("selection.min_selection_weight", sel.min_selection_weight),
            ("trend.min_delta", self.trend.min_delta),
        ] {
            if !weight.is_finite() || weight < 0.0 {
                return Err(BeatConfigError::invalid(format!(
                    "{name} must be finite and >= 0 (got {weight})"
                )));
            }
        }
        if sel.stance_affinity_floor <= 0.0 {
            return Err(BeatConfigError::invalid(
                "selection.stance_affinity_floor must be > 0 — the stance term scales a \
                 wrong-stance dressing down, it must never eliminate it",
            ));
        }
        if self.budget.fork_expire_turns == 0 {
            return Err(BeatConfigError::invalid(
                "budget.fork_expire_turns must be > 0 (a zero expiry auto-defers every fork on \
                 the turn it posts, so the player never sees it)",
            ));
        }
        if self.trend.max_history_turns == 0 {
            return Err(BeatConfigError::invalid(
                "trend.max_history_turns must be > 0 (a zero history makes `trend` unevaluable)",
            ));
        }
        if self.voice.registers.is_empty() {
            return Err(BeatConfigError::invalid(
                "voice.registers must list at least one register",
            ));
        }
        if !self.registers_contains(&self.voice.default_register) {
            return Err(BeatConfigError::invalid(format!(
                "voice.default_register {:?} is not in voice.registers",
                self.voice.default_register
            )));
        }
        self.validate_mediums()?;
        if self.memory.max_threads_per_kind == 0 {
            return Err(BeatConfigError::invalid(
                "memory.max_threads_per_kind must be > 0 — a zero cap silently discards every \
                 thread, so every `thread` predicate and `thread.*` resolver in the catalog would \
                 be permanently unsatisfiable",
            ));
        }
        let mut seen_axes: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for axis in &self.stance.axes {
            if !seen_axes.insert(axis.id.as_str()) {
                return Err(BeatConfigError::invalid(format!(
                    "duplicate stance axis id {:?} — a fork's `writes.stance` keys by id, so two \
                     axes sharing one would be indistinguishable",
                    axis.id
                )));
            }
            // Deliberately the **base** registry, not the stance family: an axis backed by a
            // `stance.*` signal would define its own accreted value in terms of itself.
            if !super::signals::is_registered_signal(&axis.signal) {
                return Err(BeatConfigError::invalid(format!(
                    "stance axis {:?} names unknown signal {:?}",
                    axis.id, axis.signal
                )));
            }
            let [lo, hi] = axis.range;
            if !lo.is_finite() || !hi.is_finite() || lo >= hi {
                return Err(BeatConfigError::invalid(format!(
                    "stance axis {:?} range must be finite with range[0] < range[1]",
                    axis.id
                )));
            }
        }
        Ok(())
    }

    /// The medium ladder must be a genuine ladder: at least one rung, a `when`-free default first,
    /// unique ids, and every later rung gated on registered signals.
    fn validate_mediums(&self) -> Result<(), BeatConfigError> {
        let mediums = &self.voice.mediums;
        let Some(first) = mediums.first() else {
            return Err(BeatConfigError::invalid(
                "voice.mediums must list at least one medium (the default the telling starts on)",
            ));
        };
        if first.when.is_some() {
            return Err(BeatConfigError::invalid(format!(
                "voice.mediums[0] {:?} declares a `when` — the first entry is the default rung and \
                 is always satisfied, so a condition on it could only ever be misleading",
                first.id
            )));
        }
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for medium in mediums {
            if !seen.insert(medium.id.as_str()) {
                return Err(BeatConfigError::invalid(format!(
                    "duplicate voice medium id {:?}",
                    medium.id
                )));
            }
            let Some(when) = &medium.when else { continue };
            let mut signals = Vec::new();
            when.collect_signals(&mut signals);
            for signal in signals {
                // The stance family is config-driven, so it resolves through this same config.
                if !super::signals::is_registered_signal(&signal)
                    && !super::stance::is_stance_signal(&signal, self)
                {
                    return Err(BeatConfigError::invalid(format!(
                        "voice medium {:?} references unknown signal {signal:?}",
                        medium.id
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BeatConfigError {
    #[error("failed to read beat config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse beat config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid beat config: {0}")]
    Invalid(String),
}

impl BeatConfigError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

/// Handle for accessing the beat configuration.
#[derive(Resource, Debug, Clone)]
pub struct BeatConfigHandle(pub Arc<BeatConfig>);

impl BeatConfigHandle {
    pub fn new(config: Arc<BeatConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<BeatConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<BeatConfig>) {
        self.0 = config;
    }
}

impl Default for BeatConfigHandle {
    fn default() -> Self {
        Self(BeatConfig::builtin())
    }
}

/// Metadata about the beat configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct BeatConfigMetadata {
    path: Option<PathBuf>,
}

impl BeatConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the beat config from `BEAT_CONFIG_PATH` or the default data path, falling back to the
/// baked-in builtin. An invalid config is refused at **error** level rather than silently
/// disabling the narrative layer with nonsense levers.
pub fn load_beat_config_from_env() -> (Arc<BeatConfig>, BeatConfigMetadata) {
    let override_path = env::var("BEAT_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/beat_config.json");
    let path = override_path.unwrap_or(default_path);

    match BeatConfig::from_file(&path) {
        Ok(config) => {
            tracing::info!(
                target: "shadow_scale::config",
                path = %path.display(),
                "beat_config.loaded=file"
            );
            return (Arc::new(config), BeatConfigMetadata::new(Some(path)));
        }
        Err(err @ BeatConfigError::Invalid(_)) => {
            tracing::error!(
                target: "shadow_scale::config",
                path = %path.display(),
                error = %err,
                "beat_config.invalid_rejected"
            );
        }
        Err(err) => {
            tracing::warn!(
                target: "shadow_scale::config",
                path = %path.display(),
                error = %err,
                "beat_config.load_failed"
            );
        }
    }

    tracing::info!(target: "shadow_scale::config", "beat_config.loaded=builtin");
    (BeatConfig::builtin(), BeatConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin_json() -> serde_json::Value {
        serde_json::from_str(BUILTIN_BEAT_CONFIG).expect("builtin config parses as json")
    }

    /// The shipped config parses *and* satisfies every invariant (the
    /// `sedentarization_config.rs:203` pattern).
    #[test]
    fn builtin_config_parses_and_validates() {
        let config = BeatConfig::builtin();
        assert!(config.selection.novelty_window_turns > 0);
        assert!(config.trend.max_history_turns > 0);
        assert!(config.has_register(&config.voice.default_register));
        assert!(!config.stance.axes.is_empty());
        // Every stance axis names a real signal — the reason the culture signals exist in PR-A.
        for axis in &config.stance.axes {
            assert!(super::super::signals::is_registered_signal(&axis.signal));
        }
    }

    fn mutate(f: impl FnOnce(&mut serde_json::Value)) -> Result<BeatConfig, BeatConfigError> {
        let mut json = builtin_json();
        f(&mut json);
        BeatConfig::from_json_str(&json.to_string())
    }

    #[test]
    fn validate_rejects_zero_novelty_window() {
        assert!(mutate(|j| j["selection"]["novelty_window_turns"] = 0.into()).is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_novelty_floor() {
        assert!(mutate(|j| j["selection"]["novelty_floor"] = 1.5.into()).is_err());
    }

    #[test]
    fn validate_rejects_negative_weight() {
        assert!(mutate(|j| j["selection"]["fit_soft_tag_weight"] = (-0.5).into()).is_err());
    }

    #[test]
    fn validate_rejects_zero_history() {
        assert!(mutate(|j| j["trend"]["max_history_turns"] = 0.into()).is_err());
    }

    #[test]
    fn validate_rejects_default_register_not_in_registers() {
        assert!(mutate(|j| j["voice"]["default_register"] = "operatic".into()).is_err());
    }

    #[test]
    fn validate_rejects_stance_axis_with_unknown_signal() {
        assert!(mutate(|j| j["stance"]["axes"][0]["signal"] = "vibes.total".into()).is_err());
    }

    /// The shipped medium ladder is ordered, `when`-free at the bottom, and every rung's condition
    /// reads signals that actually exist.
    #[test]
    fn the_shipped_medium_ladder_is_a_ladder() {
        let config = BeatConfig::builtin();
        let ids: Vec<&str> = config
            .voice
            .mediums
            .iter()
            .map(|medium| medium.id.as_str())
            .collect();
        assert_eq!(ids, vec!["oral", "painted", "written"]);
        assert!(config.voice.mediums[0].when.is_none());
        for medium in &config.voice.mediums[1..] {
            assert!(medium.when.is_some(), "{} needs a gate", medium.id);
        }
        assert!(config.memory.max_threads_per_kind > 0);
    }

    #[test]
    fn validate_rejects_an_empty_medium_ladder() {
        assert!(mutate(|j| j["voice"]["mediums"] = serde_json::json!([])).is_err());
    }

    #[test]
    fn validate_rejects_a_condition_on_the_default_medium() {
        assert!(mutate(|j| {
            j["voice"]["mediums"][0]["when"] =
                serde_json::json!({ "signal": "band.count", "gte": 1 });
        })
        .is_err());
    }

    #[test]
    fn validate_rejects_a_medium_gated_on_an_unknown_signal() {
        assert!(mutate(|j| {
            j["voice"]["mediums"][1]["when"] =
                serde_json::json!({ "signal": "vibes.total", "gte": 1 });
        })
        .is_err());
    }

    #[test]
    fn validate_rejects_a_zero_thread_cap() {
        assert!(mutate(|j| j["memory"]["max_threads_per_kind"] = 0.into()).is_err());
    }

    #[test]
    fn validate_rejects_inverted_stance_range() {
        assert!(
            mutate(|j| j["stance"]["axes"][0]["range"] = serde_json::json!([1.0, 0.0])).is_err()
        );
    }
}
