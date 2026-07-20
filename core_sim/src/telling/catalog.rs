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
    nouns::{is_known_biome_tag, is_registered_resolver_for, template_placeholders},
    predicate::Predicate,
    signals::is_registered_signal,
    stance,
};

pub const BUILTIN_BEAT_DEFINITIONS: &str = include_str!("../data/beat_definitions.json");

/// How loud a beat is, and which budget it spends. A `Fork` posts a [`PendingFork`](super::
/// PendingFork) instead of a feed line, and is marked fired when **answered**, not when posted.
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

/// What the beat is *about*. `fork` names the stance axis a Fork writes to.
#[derive(Debug, Clone, Deserialize)]
pub struct Soul {
    pub question: String,
    #[serde(default)]
    pub fork: Option<String>,
}

/// What answering a choice writes into the [`BeatLedger`](super::BeatLedger).
///
/// **An empty `writes` is what makes a choice the fork's `defer`** — the explicit out the
/// client's turn gate depends on, and validation guarantees every fork has exactly one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ChoiceWrites {
    /// Stance axis id → the **declared offset** added to the ledger (see `telling::stance`).
    pub stance: BTreeMap<String, f32>,
    /// Consequence flags, readable by the `{ "flag": F }` predicate.
    pub flags: Vec<String>,
}

impl ChoiceWrites {
    /// Does answering this choice commit the player to anything?
    pub fn is_empty(&self) -> bool {
        self.stance.is_empty() && self.flags.is_empty()
    }
}

/// One answer a player may give to a `fork`-tier beat.
#[derive(Debug, Clone, Deserialize)]
pub struct BeatChoice {
    pub id: String,
    /// Register → button copy. Rendered for **every** register at post time, because the register
    /// is a live user toggle.
    pub label: BTreeMap<String, String>,
    #[serde(default)]
    pub writes: ChoiceWrites,
    /// Lift a `once` beat's guard this many turns after the answer — the defer branch's
    /// "it returns, sharper".
    #[serde(default)]
    pub rearm_after_turns: Option<u32>,
    /// Register → the line pushed to the feed once the choice is taken.
    pub echo: BTreeMap<String, String>,
}

impl BeatChoice {
    /// The **defer** choice: it commits to nothing. Computed server-side so no consumer (least of
    /// all the client) has to know what makes a choice a defer.
    pub fn is_defer(&self) -> bool {
        self.writes.is_empty()
    }
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
    /// Axis id → how strongly this dressing leans that way. The **re-coloring** term: one shared
    /// event reads with opposite valence depending on who the player has become
    /// (`telling::stance::affinity_term`).
    #[serde(default)]
    pub stance_affinity: Option<BTreeMap<String, f32>>,
}

/// Promote a resolved noun into a durable **memory thread** when the beat lands.
///
/// `kind` is free-form and content-defined; declaring it here is what registers the
/// `thread.<kind>.oldest` / `.recent` noun resolvers and makes `{ "thread": kind, … }` gateable.
#[derive(Debug, Clone, Deserialize)]
pub struct Remembers {
    /// A noun slot this beat declares. Its resolved value is snapshotted into the thread.
    pub slot: String,
    pub kind: String,
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
    /// The decision surface. **Required (≥2, exactly one of them a defer) on a `fork`-tier beat
    /// and forbidden on every other tier** — a beat with choices nobody can answer, or a fork with
    /// none, is an authoring error, not a silent no-op.
    #[serde(default)]
    pub choices: Vec<BeatChoice>,
    /// Nouns this beat promotes into durable memory threads when it lands (emits, or — for a fork
    /// — posts, since a fork's nouns are pinned at post time exactly as a thread's are).
    #[serde(default)]
    pub remembers: Vec<Remembers>,
    /// Signal ids sampled into the detail line ("the voice never lies").
    #[serde(default)]
    pub gloss: Vec<String>,
    pub cooldown_turns: Option<u32>,
    #[serde(default)]
    pub once: bool,
}

impl BeatDefinition {
    /// The choice `id` names, if this beat declares it.
    pub fn choice(&self, id: &str) -> Option<&BeatChoice> {
        self.choices.iter().find(|choice| choice.id == id)
    }

    /// The fork's defer choice. Validation guarantees exactly one on every fork, so this is
    /// `Some` for any loaded fork — the expiry valve relies on that.
    pub fn defer_choice(&self) -> Option<&BeatChoice> {
        self.choices.iter().find(|choice| choice.is_defer())
    }
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

    /// Every thread kind the catalog's `remembers` entries declare. This — not a hardcoded list —
    /// is what registers the `thread.*` noun resolvers and the `thread` predicate's vocabulary.
    pub fn thread_kinds(&self) -> BTreeSet<String> {
        self.beats
            .iter()
            .flat_map(|beat| beat.remembers.iter())
            .map(|remembers| remembers.kind.clone())
            .collect()
    }

    fn validate(&self, config: &BeatConfig) -> Result<(), BeatCatalogError> {
        let register = &config.voice.default_register;
        let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
        let thread_kinds = self.thread_kinds();

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
                    if !is_registered_resolver_for(resolver, &thread_kinds) {
                        return Err(BeatCatalogError::invalid(format!(
                            "beat {:?} noun slot {slot:?} names unknown resolver {resolver:?} \
                             (a `thread.<kind>.oldest`/`.recent` resolver needs some beat to \
                             declare that kind in `remembers`, or it could never resolve)",
                            beat.id
                        )));
                    }
                }
            }

            // `remembers` promotes a *declared* slot into a thread; a typo would silently remember
            // nothing forever.
            for remembers in &beat.remembers {
                if !beat.nouns.contains_key(&remembers.slot) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} remembers undeclared noun slot {:?}",
                        beat.id, remembers.slot
                    )));
                }
                if remembers.kind.trim().is_empty() {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} remembers slot {:?} with an empty kind",
                        beat.id, remembers.slot
                    )));
                }
            }

            // A `trend` window wider than the retained history is unevaluable: `evaluate` needs
            // `over_turns` samples *plus* this turn's, so it can only ever read false. Silent, and
            // exactly the class of bug the `answered` check exists for.
            let mut windows = Vec::new();
            beat.when.collect_trend_windows(&mut windows);
            for over_turns in windows {
                if over_turns >= config.trend.max_history_turns {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} trends over {over_turns} turns, but the ledger retains only \
                         trend.max_history_turns = {} samples — the window could never be filled, \
                         so the beat could never fire",
                        beat.id, config.trend.max_history_turns
                    )));
                }
            }

            // A `thread` predicate on a kind nothing writes can never be satisfied.
            let mut gated_kinds = Vec::new();
            beat.when.collect_thread_kinds(&mut gated_kinds);
            for kind in gated_kinds {
                if !thread_kinds.contains(&kind) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} gates on thread kind {kind:?}, which no beat's `remembers` \
                         declares — it could never be satisfied",
                        beat.id
                    )));
                }
            }

            // Every signal referenced in `when` and `gloss` must exist in the registry.
            let mut signals = Vec::new();
            beat.when.collect_signals(&mut signals);
            signals.extend(beat.gloss.iter().cloned());
            for signal in signals {
                // The `stance.*` family is config-driven (the axes live in `beat_config.json`),
                // so it resolves through the config rather than the static registry.
                if !is_registered_signal(&signal) && !stance::is_stance_signal(&signal, config) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} references unknown signal {signal:?}",
                        beat.id
                    )));
                }
            }

            // `soul.fork` names the axis the fork steers — it must exist, or the beat writes into
            // a stance nothing reads.
            if let Some(axis) = &beat.soul.fork {
                if !stance::is_configured_axis(axis, config) {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} soul.fork names unknown stance axis {axis:?}",
                        beat.id
                    )));
                }
            }

            Self::validate_choices(beat, register, config)?;

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

        // Cross-beat pass: an `answered` target must be resolvable *and* answerable.
        self.validate_answered_targets()?;
        Ok(())
    }

    /// **Validate every `{ "answered": B, "choice": C }` hard.** A typo here silently produces a
    /// beat that can never fire, which is the worst failure mode a content system has: nothing
    /// errors, nothing logs, the beat is simply never seen. So the target must exist, be a `fork`
    /// (only a fork records an answer), and declare that choice.
    fn validate_answered_targets(&self) -> Result<(), BeatCatalogError> {
        for beat in &self.beats {
            let mut answered = Vec::new();
            beat.when.collect_answered_gates(&mut answered);
            for (target_id, choice_id, _) in answered {
                let Some(target) = self.find(&target_id) else {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} gates on `answered` beat {target_id:?}, which is not in the \
                         catalog — the gate could never be satisfied",
                        beat.id
                    )));
                };
                if target.tier != BeatTier::Fork {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} gates on `answered` beat {target_id:?}, which is tier {:?} — \
                         only a fork is ever answered, so the gate could never be satisfied",
                        beat.id,
                        target.tier.as_str()
                    )));
                }
                if target.choice(&choice_id).is_none() {
                    return Err(BeatCatalogError::invalid(format!(
                        "beat {:?} gates on `answered` beat {target_id:?} choice {choice_id:?}, \
                         which that fork does not declare — the gate could never be satisfied",
                        beat.id
                    )));
                }
            }
        }
        Ok(())
    }

    /// The fork tier's decision surface. **Every fork must carry exactly one defer choice** — the
    /// explicit out the client's turn gate depends on. A fork without one would be unanswerable
    /// without committing, and the gate would trap the player in it, so a missing defer fails the
    /// load rather than shipping.
    fn validate_choices(
        beat: &BeatDefinition,
        register: &str,
        config: &BeatConfig,
    ) -> Result<(), BeatCatalogError> {
        /// A fork with one answer is not a fork.
        const MIN_FORK_CHOICES: usize = 2;
        /// The gate needs exactly one unambiguous "not now".
        const REQUIRED_DEFER_CHOICES: usize = 1;

        if beat.tier != BeatTier::Fork {
            if !beat.choices.is_empty() {
                return Err(BeatCatalogError::invalid(format!(
                    "beat {:?} is tier {:?} but declares choices — only a fork has a decision \
                     surface, and nothing would ever present these",
                    beat.id,
                    beat.tier.as_str()
                )));
            }
            return Ok(());
        }

        if beat.choices.len() < MIN_FORK_CHOICES {
            return Err(BeatCatalogError::invalid(format!(
                "fork beat {:?} has {} choice(s) — a fork needs at least {MIN_FORK_CHOICES}",
                beat.id,
                beat.choices.len()
            )));
        }

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for choice in &beat.choices {
            if !seen.insert(choice.id.as_str()) {
                return Err(BeatCatalogError::invalid(format!(
                    "fork beat {:?} has duplicate choice id {:?}",
                    beat.id, choice.id
                )));
            }
            for (field, copy) in [("label", &choice.label), ("echo", &choice.echo)] {
                if !copy.contains_key(register) {
                    return Err(BeatCatalogError::invalid(format!(
                        "fork beat {:?} choice {:?} {field} is missing the default register \
                         {register:?}",
                        beat.id, choice.id
                    )));
                }
                // Choice copy is rendered at post time like the narration, so its placeholders
                // are subject to the same slot check.
                for (register_key, template) in copy {
                    let placeholders = template_placeholders(template).map_err(|err| {
                        BeatCatalogError::invalid(format!(
                            "fork beat {:?} choice {:?} {field} register {register_key:?}: {err}",
                            beat.id, choice.id
                        ))
                    })?;
                    for placeholder in placeholders {
                        if !beat.nouns.contains_key(&placeholder.slot) {
                            return Err(BeatCatalogError::invalid(format!(
                                "fork beat {:?} choice {:?} {field} register {register_key:?} \
                                 references undeclared noun slot {:?}",
                                beat.id, choice.id, placeholder.slot
                            )));
                        }
                    }
                }
            }
            for axis in choice.writes.stance.keys() {
                if !stance::is_configured_axis(axis, config) {
                    return Err(BeatCatalogError::invalid(format!(
                        "fork beat {:?} choice {:?} writes unknown stance axis {axis:?}",
                        beat.id, choice.id
                    )));
                }
            }
        }

        let defers = beat.choices.iter().filter(|c| c.is_defer()).count();
        if defers != REQUIRED_DEFER_CHOICES {
            return Err(BeatCatalogError::invalid(format!(
                "fork beat {:?} has {defers} defer choice(s) (an empty `writes`), needs exactly \
                 {REQUIRED_DEFER_CHOICES} — the client's turn gate depends on there being one \
                 explicit out, and would otherwise trap the player in this fork",
                beat.id
            )));
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

    // --- the fork tier's decision surface -------------------------------------------------

    /// The shipped fork is well-formed: ≥2 choices, ids unique, exactly one defer, and it steers
    /// a configured stance axis.
    #[test]
    fn the_shipped_fork_declares_a_well_formed_decision_surface() {
        let catalog = BeatCatalog::builtin();
        let fork = catalog
            .find("sedentarization.soft_drift")
            .expect("the shipped fork");
        assert_eq!(fork.tier, BeatTier::Fork);
        assert_eq!(fork.soul.fork.as_deref(), Some("roam_settle"));
        assert!(fork.choices.len() >= 2);
        assert_eq!(fork.choices.iter().filter(|c| c.is_defer()).count(), 1);
        assert!(fork.defer_choice().is_some());
        // Every non-fork beat keeps its decision surface empty.
        for beat in catalog.beats().iter().filter(|b| b.tier != BeatTier::Fork) {
            assert!(beat.choices.is_empty(), "{} is not a fork", beat.id);
        }
    }

    /// Index of the shipped fork in the builtin catalog, for the JSON mutations below.
    fn fork_index() -> usize {
        builtin_json()
            .as_array()
            .unwrap()
            .iter()
            .position(|beat| beat["tier"] == "fork")
            .expect("the builtin catalog ships a fork")
    }

    #[test]
    fn validate_rejects_a_fork_with_no_choices() {
        let fork = fork_index();
        let err = mutate(|j| j[fork]["choices"] = serde_json::json!([])).unwrap_err();
        assert!(err.to_string().contains("at least"), "{err}");
    }

    #[test]
    fn validate_rejects_a_non_fork_beat_that_declares_choices() {
        let fork = fork_index();
        let err = mutate(|j| {
            let choices = j[fork]["choices"].clone();
            let victim = if fork == 0 { 1 } else { 0 };
            j[victim]["choices"] = choices;
        })
        .unwrap_err();
        assert!(err.to_string().contains("only a fork"), "{err}");
    }

    /// **The client's turn gate depends on this.** A fork whose every choice commits the player is
    /// a trap, so it must never load.
    #[test]
    fn validate_rejects_a_fork_with_no_defer_choice() {
        let fork = fork_index();
        let err = mutate(|j| {
            for choice in j[fork]["choices"].as_array_mut().unwrap() {
                if choice["writes"].as_object().is_none_or(|w| w.is_empty()) {
                    choice["writes"] = serde_json::json!({ "stance": { "roam_settle": 0.1 } });
                }
            }
        })
        .unwrap_err();
        assert!(err.to_string().contains("defer choice"), "{err}");
    }

    #[test]
    fn validate_rejects_a_fork_with_two_defer_choices() {
        let fork = fork_index();
        let err = mutate(|j| j[fork]["choices"][0]["writes"] = serde_json::json!({})).unwrap_err();
        assert!(err.to_string().contains("defer choice"), "{err}");
    }

    #[test]
    fn validate_rejects_duplicate_choice_ids() {
        let fork = fork_index();
        let err = mutate(|j| {
            let first = j[fork]["choices"][0]["id"].clone();
            j[fork]["choices"][1]["id"] = first;
        })
        .unwrap_err();
        assert!(err.to_string().contains("duplicate choice id"), "{err}");
    }

    #[test]
    fn validate_rejects_a_choice_writing_an_unknown_stance_axis() {
        let fork = fork_index();
        let err = mutate(|j| {
            j[fork]["choices"][0]["writes"] =
                serde_json::json!({ "stance": { "vibes_chill": -0.4 } });
        })
        .unwrap_err();
        assert!(err.to_string().contains("unknown stance axis"), "{err}");
    }

    #[test]
    fn validate_rejects_a_choice_missing_the_default_register() {
        let fork = fork_index();
        let err = mutate(|j| {
            j[fork]["choices"][0]["label"]
                .as_object_mut()
                .unwrap()
                .remove("mythic");
        })
        .unwrap_err();
        assert!(err.to_string().contains("default register"), "{err}");
    }

    #[test]
    fn validate_rejects_soul_fork_naming_an_unknown_stance_axis() {
        let fork = fork_index();
        let err = mutate(|j| j[fork]["soul"]["fork"] = "vibes_chill".into()).unwrap_err();
        assert!(err.to_string().contains("unknown stance axis"), "{err}");
    }

    /// The `stance.*` signal family is config-driven, so a gloss may read it — and a typo in one
    /// must still fail at load like any other unknown signal.
    #[test]
    fn stance_signals_are_glossable_but_a_typo_still_fails() {
        let fork = fork_index();
        assert!(mutate(|j| {
            j[fork]["gloss"] = serde_json::json!(["stance.roam_settle"]);
        })
        .is_ok());
        let err =
            mutate(|j| j[fork]["gloss"] = serde_json::json!(["stance.roam_setle"])).unwrap_err();
        assert!(err.to_string().contains("unknown signal"), "{err}");
    }

    // --- memory threads + the `answered` gate ---------------------------------------------

    /// The shipped catalog declares the two thread kinds its resolvers and gates depend on.
    #[test]
    fn the_shipped_catalog_declares_the_thread_kinds_it_calls_back_to() {
        let catalog = BeatCatalog::builtin();
        let kinds = catalog.thread_kinds();
        assert!(kinds.contains("place"), "{kinds:?}");
        assert!(kinds.contains("beast"), "{kinds:?}");
        let site = catalog.find("discovery.site_found").expect("the site beat");
        assert_eq!(site.remembers.len(), 1);
        assert_eq!(site.remembers[0].slot, "place");
    }

    /// Registration is **generic over kind**: a brand-new kind declared by content registers its
    /// resolvers with no engine change.
    #[test]
    fn a_new_thread_kind_registers_its_resolvers_without_an_engine_change() {
        let catalog = mutate(|j| {
            j[1]["remembers"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({ "slot": "place", "kind": "hearth" }));
            j[1]["nouns"]["place"]["fallback"] = "thread.hearth.recent".into();
        })
        .expect("a content-defined kind is enough to register thread.hearth.*");
        assert!(catalog.thread_kinds().contains("hearth"));
    }

    #[test]
    fn validate_rejects_a_thread_resolver_for_a_kind_nothing_remembers() {
        let err = mutate(|j| j[1]["nouns"]["place"]["fallback"] = "thread.ghost.oldest".into())
            .unwrap_err();
        assert!(err.to_string().contains("unknown resolver"), "{err}");
    }

    #[test]
    fn validate_rejects_an_unknown_thread_selector() {
        let err = mutate(|j| j[1]["nouns"]["place"]["fallback"] = "thread.place.middling".into())
            .unwrap_err();
        assert!(err.to_string().contains("unknown resolver"), "{err}");
    }

    #[test]
    fn validate_rejects_remembering_an_undeclared_slot() {
        let err = mutate(|j| {
            j[1]["remembers"] = serde_json::json!([{ "slot": "ghost", "kind": "place" }]);
        })
        .unwrap_err();
        assert!(err.to_string().contains("undeclared noun slot"), "{err}");
    }

    /// A `trend` window wider than the retained history can only ever read false — silent, like a
    /// typo'd `answered` target, so it fails at load instead.
    #[test]
    fn validate_rejects_a_trend_window_wider_than_the_retained_history() {
        let window = config().trend.max_history_turns;
        let err = mutate(|j| {
            j[0]["when"] = serde_json::json!({
                "signal": "turn.index",
                "trend": "rising",
                "over_turns": window
            });
        })
        .unwrap_err();
        assert!(err.to_string().contains("max_history_turns"), "{err}");
    }

    #[test]
    fn validate_rejects_a_thread_gate_on_a_kind_nothing_remembers() {
        let err = mutate(|j| {
            j[0]["when"] = serde_json::json!({ "thread": "ghost", "min_count": 1 });
        })
        .unwrap_err();
        assert!(err.to_string().contains("thread kind"), "{err}");
    }

    /// **The `answered` gate is validated hard.** A typo'd target silently produces a beat that can
    /// never fire — nothing errors, nothing logs, the beat is simply never seen — which is the
    /// worst failure mode a content system has.
    #[test]
    fn validate_rejects_an_answered_gate_on_a_beat_that_does_not_exist() {
        let err = mutate(|j| {
            j[0]["when"] = serde_json::json!({ "answered": "no.such_fork", "choice": "yes" });
        })
        .unwrap_err();
        assert!(err.to_string().contains("not in the catalog"), "{err}");
    }

    #[test]
    fn validate_rejects_an_answered_gate_on_a_non_fork_beat() {
        let err = mutate(|j| {
            j[0]["when"] =
                serde_json::json!({ "answered": "discovery.site_found", "choice": "yes" });
        })
        .unwrap_err();
        assert!(err.to_string().contains("only a fork"), "{err}");
    }

    #[test]
    fn validate_rejects_an_answered_gate_on_a_choice_the_fork_does_not_declare() {
        let err = mutate(|j| {
            j[0]["when"] = serde_json::json!({
                "answered": "sedentarization.soft_drift",
                "choice": "yes_trale"
            });
        })
        .unwrap_err();
        assert!(err.to_string().contains("does not declare"), "{err}");
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
