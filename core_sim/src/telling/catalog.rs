//! The beat catalog — the **content** layer of The Telling, and the mod surface.
//!
//! Loaded from `data/beat_definitions.json`, mirroring `great_discovery_definitions.json` in
//! shape and load path. Content composes engine-provided signals and noun resolvers; it cannot
//! invent them (`docs/plan_the_telling.md` §1b).
//!
//! `validate()` runs inside `from_json_str`, so every load path is covered (the
//! `fauna_config.rs` convention). The single most valuable check here is that **every `{slot}`
//! placeholder in every template resolves to a declared noun slot** — an authoring typo fails at
//! load, never at render in front of a player.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{de, Deserialize, Deserializer};
use thiserror::Error;

use super::{
    config::BeatConfig,
    nouns::{is_known_biome_tag, is_registered_resolver, template_placeholders},
    predicate::Predicate,
    signals::is_registered_signal,
};

pub const BUILTIN_BEAT_DEFINITIONS: &str = include_str!("../data/beat_definitions.json");

/// How loud a beat is, and which budget it spends. `Fork` parses but is **inert in PR-A** — the
/// decision surface is PR-B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BeatTier {
    Ambient,
    Beat,
    Fork,
}

impl BeatTier {
    /// Stable persisted/config key.
    pub fn as_str(self) -> &'static str {
        match self {
            BeatTier::Ambient => "ambient",
            BeatTier::Beat => "beat",
            BeatTier::Fork => "fork",
        }
    }

    /// Unknown key is an error at load, never a panic or a silent default.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "ambient" => Some(BeatTier::Ambient),
            "beat" => Some(BeatTier::Beat),
            "fork" => Some(BeatTier::Fork),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for BeatTier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let key = String::deserialize(deserializer)?;
        BeatTier::from_key(&key).ok_or_else(|| {
            de::Error::custom(format!(
                "unknown beat tier {key:?} (expected ambient, beat, or fork)"
            ))
        })
    }
}

/// What the beat is *about*. `fork` names the stance axis a Fork writes to (PR-B).
#[derive(Debug, Clone, Deserialize)]
pub struct Soul {
    pub question: String,
    #[serde(default)]
    pub fork: Option<String>,
}

/// A noun slot binding: which resolver fills it, and what to try if that comes up empty.
#[derive(Debug, Clone, Deserialize)]
pub struct NounBinding {
    pub from: String,
    #[serde(default)]
    pub fallback: Option<String>,
}

/// Gating for one wardrobe entry. Both lists default empty (an entry that fits anywhere).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fit {
    /// Slots that must resolve, or the entry is excluded from selection.
    pub requires_noun: Vec<String>,
    /// Biome tags the band's current ground must carry (a hard gate).
    pub biome: Vec<String>,
}

/// One dressing of a beat's soul.
#[derive(Debug, Clone, Deserialize)]
pub struct WardrobeEntry {
    pub id: String,
    #[serde(default)]
    pub fit: Fit,
    /// Register → template. Every player-visible string is keyed by register (§2d).
    pub voice: BTreeMap<String, String>,
    /// Parsed in PR-A, **unused** until PR-B populates the stance vector.
    #[serde(default)]
    pub stance_affinity: Option<BTreeMap<String, f32>>,
}

/// One beat: a soul, a trigger, its nouns, and the wardrobe it can be dressed in.
#[derive(Debug, Clone, Deserialize)]
pub struct BeatDefinition {
    pub id: String,
    pub tier: BeatTier,
    pub soul: Soul,
    pub when: Predicate,
    /// `BTreeMap`, not `HashMap` — slot iteration order feeds resolution and must be stable.
    #[serde(default)]
    pub nouns: BTreeMap<String, NounBinding>,
    pub wardrobe: Vec<WardrobeEntry>,
    /// Signal ids sampled into the detail line ("the voice never lies").
    #[serde(default)]
    pub gloss: Vec<String>,
    pub cooldown_turns: Option<u32>,
    #[serde(default)]
    pub once: bool,
}

/// The loaded beat catalog, in authored (stable) order.
#[derive(Debug, Clone, Default)]
pub struct BeatCatalog {
    beats: Vec<BeatDefinition>,
}

impl BeatCatalog {
    pub fn builtin() -> Arc<Self> {
        let config = BeatConfig::builtin();
        Arc::new(
            Self::from_json_str(BUILTIN_BEAT_DEFINITIONS, &config)
                .expect("builtin beat definitions should be valid"),
        )
    }

    /// Parse **and validate** against `config` (the register + stance vocabulary lives there).
    pub fn from_json_str(json: &str, config: &BeatConfig) -> Result<Self, BeatCatalogError> {
        let beats: Vec<BeatDefinition> = serde_json::from_str(json)?;
        let catalog = Self { beats };
        catalog.validate(config)?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path, config: &BeatConfig) -> Result<Self, BeatCatalogError> {
        let contents = fs::read_to_string(path).map_err(|source| BeatCatalogError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents, config)
    }

    /// Beats in authored order — the stable candidate-evaluation order.
    pub fn beats(&self) -> &[BeatDefinition] {
        &self.beats
    }

    pub fn find(&self, id: &str) -> Option<&BeatDefinition> {
        self.beats.iter().find(|beat| beat.id == id)
    }

    fn validate(&self, config: &BeatConfig) -> Result<(), BeatCatalogError> {
        let register = &config.voice.default_register;
        let mut seen_ids: BTreeSet<&str> = BTreeSet::new();

        for beat in &self.beats {
            if !seen_ids.insert(beat.id.as_str()) {
                return Err(BeatCatalogError::invalid(format!(
                    "duplicate beat id {:?}",
                    beat.id
                )));
            }
            if beat.wardrobe.is_empty() {
                return Err(BeatCatalogError::invalid(format!(
                    "beat {:?} has no wardrobe entries — it could never emit",
                    beat.id
                )));
            }

            // Noun bindings must name registered resolvers (`from` and any `fallback`).
            for (slot, binding) in &beat.nouns {
                for resolver in [Some(&binding.from), binding.fallback.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if !is_registered_resolver(resolver) {
                        return Err(BeatCatalogError::invalid(format!(
                            "beat {:?} noun slot {slot:?} names unknown resolver {resolver:?}",
                            beat.id
                        )));
                    }
                }
            }

            // Every signal referenced in `when` and `gloss` must exist in the registry.
            let mut signals = Vec::new();
            beat.when.collect_signals(&mut signals);
            signals.extend(beat.gloss.iter().cloned());
            for signal in signals {
                if !is_registered_signal(&signal) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} references unknown signal {signal:?}",
                        beat.id
                    )));
                }
            }

            let mut seen_wardrobe: BTreeSet<&str> = BTreeSet::new();
            for entry in &beat.wardrobe {
                if !seen_wardrobe.insert(entry.id.as_str()) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} has duplicate wardrobe entry id {:?}",
                        beat.id, entry.id
                    )));
                }
                if !entry.voice.contains_key(register) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} wardrobe entry {:?} is missing the default register {register:?}",
                        beat.id, entry.id
                    )));
                }
                for slot in &entry.fit.requires_noun {
                    if !beat.nouns.contains_key(slot) {
                        return Err(BeatCatalogError::invalid(format!(
                            "beat {:?} wardrobe entry {:?} requires undeclared noun slot {slot:?}",
                            beat.id, entry.id
                        )));
                    }
                }
                for tag in &entry.fit.biome {
                    if !is_known_biome_tag(tag) {
                        return Err(BeatCatalogError::invalid(format!(
                            "beat {:?} wardrobe entry {:?} names unknown biome tag {tag:?}",
                            beat.id, entry.id
                        )));
                    }
                }
                // The most valuable check: an unresolvable `{slot}` fails here, not at render.
                for (register_key, template) in &entry.voice {
                    let placeholders = template_placeholders(template).map_err(|err| {
                        BeatCatalogError::invalid(format!(
                            "beat {:?} wardrobe entry {:?} register {register_key:?}: {err}",
                            beat.id, entry.id
                        ))
                    })?;
                    for placeholder in placeholders {
                        if !beat.nouns.contains_key(&placeholder.slot) {
                            return Err(BeatCatalogError::invalid(format!(
                                "beat {:?} wardrobe entry {:?} register {register_key:?} \
                                 references undeclared noun slot {:?}",
                                beat.id, entry.id, placeholder.slot
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BeatCatalogError {
    #[error("failed to read beat definitions from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse beat definitions: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid beat definitions: {0}")]
    Invalid(String),
}

impl BeatCatalogError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

/// Handle for accessing the beat catalog.
#[derive(Resource, Debug, Clone)]
pub struct BeatCatalogHandle(pub Arc<BeatCatalog>);

impl BeatCatalogHandle {
    pub fn new(catalog: Arc<BeatCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<BeatCatalog> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, catalog: Arc<BeatCatalog>) {
        self.0 = catalog;
    }
}

impl Default for BeatCatalogHandle {
    fn default() -> Self {
        Self(BeatCatalog::builtin())
    }
}

/// Metadata about the beat catalog source.
#[derive(Resource, Debug, Clone, Default)]
pub struct BeatCatalogMetadata {
    path: Option<PathBuf>,
}

impl BeatCatalogMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the beat catalog from `BEAT_DEFINITIONS_PATH` or the default data path, falling back to
/// the baked-in builtin. Invalid content is refused at **error** level — a catalog with a broken
/// placeholder would otherwise render holes into player-facing copy.
pub fn load_beat_catalog_from_env(config: &BeatConfig) -> (Arc<BeatCatalog>, BeatCatalogMetadata) {
    let override_path = env::var("BEAT_DEFINITIONS_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/beat_definitions.json");
    let path = override_path.unwrap_or(default_path);

    match BeatCatalog::from_file(&path, config) {
        Ok(catalog) => {
            tracing::info!(
                target: "shadow_scale::config",
                path = %path.display(),
                beats = catalog.beats().len(),
                "beat_catalog.loaded=file"
            );
            return (Arc::new(catalog), BeatCatalogMetadata::new(Some(path)));
        }
        Err(err @ BeatCatalogError::Invalid(_)) => {
            tracing::error!(
                target: "shadow_scale::config",
                path = %path.display(),
                error = %err,
                "beat_catalog.invalid_rejected"
            );
        }
        Err(err) => {
            tracing::warn!(
                target: "shadow_scale::config",
                path = %path.display(),
                error = %err,
                "beat_catalog.load_failed"
            );
        }
    }

    tracing::info!(target: "shadow_scale::config", "beat_catalog.loaded=builtin");
    (BeatCatalog::builtin(), BeatCatalogMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Arc<BeatConfig> {
        BeatConfig::builtin()
    }

    fn builtin_json() -> serde_json::Value {
        serde_json::from_str(BUILTIN_BEAT_DEFINITIONS).expect("builtin definitions parse as json")
    }

    fn load(json: &serde_json::Value) -> Result<BeatCatalog, BeatCatalogError> {
        BeatCatalog::from_json_str(&json.to_string(), &config())
    }

    fn mutate(f: impl FnOnce(&mut serde_json::Value)) -> Result<BeatCatalog, BeatCatalogError> {
        let mut json = builtin_json();
        f(&mut json);
        load(&json)
    }

    /// The shipped catalog parses *and* satisfies every invariant.
    #[test]
    fn builtin_catalog_parses_and_validates() {
        let catalog = BeatCatalog::builtin();
        assert!(!catalog.beats().is_empty());
        assert!(catalog.find("opening.cold_open").is_some());
        assert!(catalog.find("sedentarization.soft_drift").is_some());
        for beat in catalog.beats() {
            assert!(!beat.wardrobe.is_empty());
        }
    }

    #[test]
    fn tier_keys_round_trip_and_reject_unknowns() {
        for tier in [BeatTier::Ambient, BeatTier::Beat, BeatTier::Fork] {
            assert_eq!(BeatTier::from_key(tier.as_str()), Some(tier));
        }
        assert_eq!(BeatTier::from_key("tentpole"), None);
        assert!(mutate(|j| j[0]["tier"] = "tentpole".into()).is_err());
    }

    #[test]
    fn validate_rejects_duplicate_beat_ids() {
        let err = mutate(|j| {
            let first = j[0].clone();
            j.as_array_mut().unwrap().push(first);
        })
        .unwrap_err();
        assert!(err.to_string().contains("duplicate beat id"), "{err}");
    }

    #[test]
    fn validate_rejects_duplicate_wardrobe_ids_within_a_beat() {
        let err = mutate(|j| {
            let entry = j[0]["wardrobe"][0].clone();
            j[0]["wardrobe"].as_array_mut().unwrap().push(entry);
        })
        .unwrap_err();
        assert!(err.to_string().contains("duplicate wardrobe"), "{err}");
    }

    #[test]
    fn validate_rejects_a_beat_with_no_wardrobe() {
        let err = mutate(|j| j[0]["wardrobe"] = serde_json::json!([])).unwrap_err();
        assert!(err.to_string().contains("no wardrobe entries"), "{err}");
    }

    #[test]
    fn validate_rejects_a_missing_default_register() {
        let err = mutate(|j| {
            j[0]["wardrobe"][0]["voice"]
                .as_object_mut()
                .unwrap()
                .remove("mythic");
        })
        .unwrap_err();
        assert!(err.to_string().contains("default register"), "{err}");
    }

    /// The single most valuable validation: a typo'd placeholder fails at load, not at render.
    #[test]
    fn validate_rejects_an_unresolvable_placeholder() {
        let err = mutate(|j| {
            j[0]["wardrobe"][0]["voice"]["mythic"] = "We are {tally}.".into();
        })
        .unwrap_err();
        assert!(err.to_string().contains("undeclared noun slot"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_placeholder_field() {
        let err = mutate(|j| {
            j[0]["wardrobe"][0]["voice"]["mythic"] = "We are {count.colour}.".into();
        })
        .unwrap_err();
        assert!(err.to_string().contains("unknown noun field"), "{err}");
    }

    #[test]
    fn validate_rejects_requires_noun_naming_an_undeclared_slot() {
        let err = mutate(|j| {
            j[0]["wardrobe"][0]["fit"] = serde_json::json!({ "requires_noun": ["ghost"] });
        })
        .unwrap_err();
        assert!(err.to_string().contains("undeclared noun slot"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_signal_in_when() {
        let err =
            mutate(|j| j[0]["when"] = serde_json::json!({ "signal": "vibes.total", "eq": 0 }))
                .unwrap_err();
        assert!(err.to_string().contains("unknown signal"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_signal_in_gloss() {
        let err = mutate(|j| j[0]["gloss"] = serde_json::json!(["vibes.total"])).unwrap_err();
        assert!(err.to_string().contains("unknown signal"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_noun_resolver() {
        let err = mutate(|j| j[0]["nouns"]["ground"]["from"] = "biome.vibes".into()).unwrap_err();
        assert!(err.to_string().contains("unknown resolver"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_biome_fit_tag() {
        let err = mutate(|j| {
            j[0]["wardrobe"][0]["fit"] = serde_json::json!({ "biome": ["moonscape"] });
        })
        .unwrap_err();
        assert!(err.to_string().contains("unknown biome tag"), "{err}");
    }
}
