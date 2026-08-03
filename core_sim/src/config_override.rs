//! Staged config-tuning overrides: take a **sparse** patch from the client's tuning panel, merge it
//! into the config that would load right now, prove the result is valid, and only then stage it for
//! the next `new_game`.
//!
//! The seam it rides is `config_load`'s override registry, and the reason the whole thing works
//! without a process restart is that `handle_new_game` rebuilds the app through
//! `build_headless_app`, which re-runs every `load_*_from_env`.
//!
//! **Validating before installing is the entire safety argument, not defensive coding.**
//! [`crate::config_load::load_config_from_env`] panics on a present-but-broken file *by design* — a
//! config the operator asked for must never be silently replaced by different numbers. So an
//! override staged without validation would not fail here; it would panic the server at the next
//! New Game, arbitrarily far from the edit that caused it. The boot path's strictness is correct and
//! stays put. This path answers instead by **rejecting at the edit**, where there is a human
//! watching and a UI to tell. (Boot panics, hot-reload warns; this is a third path with its own
//! answer — see `.claude/rules/core_sim/config-loading.md`.)

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::Value;
use sim_runtime::commands::ConfigOverrideKind;
use thiserror::Error;

use crate::{
    combat_config::{CombatConfig, BUILTIN_COMBAT_CONFIG},
    config_load::{clear_override_paths, override_path_for, set_override_path},
    demographics_config::{DemographicsConfig, BUILTIN_DEMOGRAPHICS_CONFIG},
    expedition_config::{ExpeditionConfig, BUILTIN_EXPEDITION_CONFIG},
    labor_config::{LaborConfig, BUILTIN_LABOR_CONFIG},
    resources::{SimulationConfig, BUILTIN_SIMULATION_CONFIG},
};

/// Everything the override path needs to know about one tunable config: where it loads from, what
/// it ships as, and how to prove a candidate is valid.
#[derive(Clone, Copy)]
pub struct ConfigKindSpec {
    pub kind: ConfigOverrideKind,
    /// The `*_CONFIG_PATH` variable this kind loads from — also the registry key, so a staged
    /// override lands on exactly the config the env var would have.
    pub env_var: &'static str,
    /// Path of the shipped file, relative to `core_sim/`'s manifest dir.
    pub default_rel_path: &'static str,
    /// `include_str!` of that same file, which is why substituting it for an *absent* default
    /// substitutes nothing.
    pub builtin_json: &'static str,
    /// Parse a candidate through the kind's own `from_json_str` — the one function every load path
    /// funnels through, and therefore the one place `validate()` runs. Discards the config: the
    /// question here is only whether the sim would accept it.
    validate: fn(&str) -> Result<(), String>,
}

/// The spec for a kind. A `match` rather than a table lookup so that adding a
/// [`ConfigOverrideKind`] fails to compile until it is given a config to point at.
pub fn spec_for(kind: ConfigOverrideKind) -> ConfigKindSpec {
    match kind {
        ConfigOverrideKind::Simulation => ConfigKindSpec {
            kind,
            env_var: "SIM_CONFIG_PATH",
            default_rel_path: "src/data/simulation_config.json",
            builtin_json: BUILTIN_SIMULATION_CONFIG,
            validate: |json| erase(SimulationConfig::from_json_str(json)),
        },
        ConfigOverrideKind::Labor => ConfigKindSpec {
            kind,
            env_var: "LABOR_CONFIG_PATH",
            default_rel_path: "src/data/labor_config.json",
            builtin_json: BUILTIN_LABOR_CONFIG,
            validate: |json| erase(LaborConfig::from_json_str(json)),
        },
        ConfigOverrideKind::Demographics => ConfigKindSpec {
            kind,
            env_var: "DEMOGRAPHICS_CONFIG_PATH",
            default_rel_path: "src/data/demographics_config.json",
            builtin_json: BUILTIN_DEMOGRAPHICS_CONFIG,
            validate: |json| erase(DemographicsConfig::from_json_str(json)),
        },
        ConfigOverrideKind::Expedition => ConfigKindSpec {
            kind,
            env_var: "EXPEDITION_CONFIG_PATH",
            default_rel_path: "src/data/expedition_config.json",
            builtin_json: BUILTIN_EXPEDITION_CONFIG,
            validate: |json| erase(ExpeditionConfig::from_json_str(json)),
        },
        ConfigOverrideKind::Combat => ConfigKindSpec {
            kind,
            env_var: "COMBAT_CONFIG_PATH",
            default_rel_path: "src/data/combat_config.json",
            builtin_json: BUILTIN_COMBAT_CONFIG,
            validate: |json| erase(CombatConfig::from_json_str(json)),
        },
    }
}

/// Collapse any `from_json_str` result to "did the sim accept it, and if not why" — the five kinds
/// return five different config and error types, and this path cares about neither.
fn erase<T, E: std::fmt::Display>(parsed: Result<T, E>) -> Result<(), String> {
    parsed.map(|_| ()).map_err(|err| err.to_string())
}

/// Why a staged override was refused. Every variant means **nothing changed**: no file written, no
/// registry entry, the running world untouched.
#[derive(Debug, Error)]
pub enum ConfigOverrideError {
    #[error("the {kind} override patch is not valid JSON: {source}")]
    PatchParse {
        kind: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("the {kind} override patch must be a JSON object, got {actual}")]
    PatchNotAnObject {
        kind: &'static str,
        actual: &'static str,
    },
    #[error("could not read the current {kind} config from {path}: {source}")]
    ReadCurrent {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the current {kind} config at {path} is not valid JSON: {source}")]
    CurrentParse {
        kind: &'static str,
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the patched {kind} config could not be serialized: {source}")]
    Serialize {
        kind: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("the patched {kind} config is invalid and was NOT installed: {reason}")]
    Invalid { kind: &'static str, reason: String },
    #[error("could not write the patched {kind} config to {path}: {source}")]
    Write {
        kind: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// A staged override, for the caller's log line.
#[derive(Debug, Clone)]
pub struct InstalledOverride {
    pub kind: ConfigOverrideKind,
    pub path: PathBuf,
}

/// Merge `patch_json` into the kind's current effective config, validate the result, and — only if
/// it is valid — write it under `scratch_dir` and stage it for the next `new_game`.
///
/// The order of the last two steps is the whole point: see the module docs.
pub fn install_config_override(
    kind: ConfigOverrideKind,
    patch_json: &str,
    scratch_dir: &Path,
) -> Result<InstalledOverride, ConfigOverrideError> {
    let spec = spec_for(kind);
    let label = kind.as_str();

    let patch: Value =
        serde_json::from_str(patch_json).map_err(|source| ConfigOverrideError::PatchParse {
            kind: label,
            source,
        })?;
    if !patch.is_object() {
        return Err(ConfigOverrideError::PatchNotAnObject {
            kind: label,
            actual: json_type_name(&patch),
        });
    }

    let (current_text, current_origin) = current_effective_json(&spec)?;
    let mut merged: Value = serde_json::from_str(&current_text).map_err(|source| {
        ConfigOverrideError::CurrentParse {
            kind: label,
            path: current_origin,
            source,
        }
    })?;
    merge_patch(&mut merged, &patch);

    // Pretty-printed because this file is meant to be opened and read while debugging a playtest —
    // it is the only record of what the sim actually booted on.
    //
    // A `Value` that came from valid JSON re-serializes in practice, but this runs in a live command
    // handler, and the whole argument of this module is that a bad edit is refused *here* rather than
    // crashing the server later. A panic would contradict that for the sake of one unreachable arm.
    let merged_text = serde_json::to_string_pretty(&merged)
        .map(|text| format!("{text}\n"))
        .map_err(|source| ConfigOverrideError::Serialize {
            kind: label,
            source,
        })?;

    (spec.validate)(&merged_text).map_err(|reason| ConfigOverrideError::Invalid {
        kind: label,
        reason,
    })?;

    // Past this line, and only past it, does anything outside this function change.
    let path = scratch_dir.join(format!("{label}.json"));
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| ConfigOverrideError::Write {
                kind: label,
                path: path.clone(),
                source,
            })?;
        }
    }
    fs::write(&path, merged_text).map_err(|source| ConfigOverrideError::Write {
        kind: label,
        path: path.clone(),
        source,
    })?;
    set_override_path(spec.env_var, path.clone());

    Ok(InstalledOverride { kind, path })
}

/// Drop every staged override, so the next `new_game` boots on `*_CONFIG_PATH` and the shipped
/// files again. The scratch files are left on disk deliberately — the registry is the authority, and
/// the last-applied JSON is worth having around when a playtest result needs explaining.
pub fn clear_config_overrides() {
    clear_override_paths();
}

/// The JSON a patch must be merged into: **whatever would load right now**, following
/// [`crate::config_load::load_config_from_env`]'s precedence exactly (registry → `*_CONFIG_PATH` →
/// default file → builtin). Merging into anything else would silently drop either an operator's
/// `*_CONFIG_PATH` or the designer's own earlier edits, which is precisely what a tuning loop
/// accumulates.
///
/// Returns the text plus a human-readable origin for the error messages.
fn current_effective_json(spec: &ConfigKindSpec) -> Result<(String, String), ConfigOverrideError> {
    let named = override_path_for(spec.env_var)
        .or_else(|| std::env::var_os(spec.env_var).map(PathBuf::from));
    if let Some(path) = named {
        let text =
            fs::read_to_string(&path).map_err(|source| ConfigOverrideError::ReadCurrent {
                kind: spec.kind.as_str(),
                path: path.clone(),
                source,
            })?;
        return Ok((text, path.display().to_string()));
    }

    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(spec.default_rel_path);
    match fs::read_to_string(&default_path) {
        Ok(text) => Ok((text, default_path.display().to_string())),
        // The single benign absence, same as `resolve_config`'s: the builtin IS `include_str!` of
        // this exact path, so substituting it substitutes nothing.
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok((
            spec.builtin_json.to_string(),
            format!("<builtin {}>", spec.default_rel_path),
        )),
        Err(source) => Err(ConfigOverrideError::ReadCurrent {
            kind: spec.kind.as_str(),
            path: default_path,
            source,
        }),
    }
}

/// Deep-merge `patch` into `base`: **objects merge key by key, everything else replaces.**
///
/// This is RFC 7386 JSON Merge Patch minus its null-means-delete rule. A tuning patch only ever
/// *sets* values, and honouring delete would let a stray `null` remove a required key — turning an
/// edit of one number into a config that no longer parses.
fn merge_patch(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                merge_patch(
                    base_map.entry(key.clone()).or_insert(Value::Null),
                    patch_value,
                );
            }
        }
        (base_slot, patch_value) => *base_slot = patch_value.clone(),
    }
}

/// Names the JSON kind for an error message, so "must be an object" says what arrived instead.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_load::lock_config_registry_for_test;

    /// A scratch directory unique per test, so cases cannot collide over a shared file name.
    fn scratch_dir(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shadow_scale_config_override_{}_{case}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn merge_patch_replaces_scalars_and_recurses_into_objects() {
        let mut base = serde_json::json!({
            "forage": { "rate": 1.0, "kept": "yes" },
            "top": 3,
            "list": [1, 2],
        });
        merge_patch(
            &mut base,
            &serde_json::json!({ "forage": { "rate": 9.5 }, "top": 4, "list": [7] }),
        );
        assert_eq!(
            base,
            serde_json::json!({
                "forage": { "rate": 9.5, "kept": "yes" },
                "top": 4,
                "list": [7],
            }),
            "sibling keys survive, scalars and arrays replace wholesale"
        );
    }

    /// The sparse-patch contract: a parameter the designer never touched must come through as the
    /// shipped value, not as a serde default and not as a missing key.
    #[test]
    fn a_sparse_patch_leaves_every_untouched_parameter_alone() {
        let _guard = lock_config_registry_for_test();
        clear_override_paths();

        let dir = scratch_dir("sparse");
        let installed =
            install_config_override(ConfigOverrideKind::Combat, r#"{"lethality": 2.5}"#, &dir)
                .expect("a lethality inside its range is valid");

        let written: Value =
            serde_json::from_str(&fs::read_to_string(&installed.path).expect("read back"))
                .expect("the installed file is JSON");
        let shipped: Value =
            serde_json::from_str(BUILTIN_COMBAT_CONFIG).expect("the builtin is JSON");

        assert_eq!(written["lethality"], serde_json::json!(2.5));
        assert_eq!(
            written["disengage_fraction"], shipped["disengage_fraction"],
            "an untouched parameter must keep the shipped value"
        );
        assert_eq!(
            written.as_object().expect("object").len(),
            shipped.as_object().expect("object").len(),
            "merging must not add or drop keys"
        );

        clear_override_paths();
    }

    /// **The reject path.** `CombatConfig::validate` refuses a non-positive lethality, so the merged
    /// config never parses — and because the boot loader panics on a broken file, the only safe
    /// answer is to change nothing at all: no file, no registry entry.
    #[test]
    fn a_patch_that_violates_validate_installs_nothing() {
        let _guard = lock_config_registry_for_test();
        clear_override_paths();

        let dir = scratch_dir("rejected");
        let spec = spec_for(ConfigOverrideKind::Combat);
        let err =
            install_config_override(ConfigOverrideKind::Combat, r#"{"lethality": 0.0}"#, &dir)
                .expect_err("a zero lethality must be refused");

        assert!(
            matches!(err, ConfigOverrideError::Invalid { .. }),
            "the refusal must come from the config's own validate, got {err}"
        );
        assert!(
            !dir.join("combat.json").exists(),
            "a rejected patch must leave no file behind for the boot loader to panic on"
        );
        assert!(
            override_path_for(spec.env_var).is_none(),
            "a rejected patch must leave nothing staged"
        );
    }

    /// Malformed JSON is refused at the same seam, and just as inertly.
    #[test]
    fn an_unparseable_patch_installs_nothing() {
        let _guard = lock_config_registry_for_test();
        clear_override_paths();

        let dir = scratch_dir("unparseable");
        let spec = spec_for(ConfigOverrideKind::Labor);
        assert!(matches!(
            install_config_override(ConfigOverrideKind::Labor, "{not json", &dir),
            Err(ConfigOverrideError::PatchParse { .. })
        ));
        assert!(matches!(
            install_config_override(ConfigOverrideKind::Labor, "[1, 2]", &dir),
            Err(ConfigOverrideError::PatchNotAnObject { .. })
        ));
        assert!(!dir.join("labor.json").exists());
        assert!(override_path_for(spec.env_var).is_none());
    }

    /// Successive edits accumulate: the second patch merges into the file the first one staged, not
    /// back into the shipped config. That is what makes the tuning panel usable one slider at a
    /// time, and it only works because `current_effective_json` consults the registry first.
    #[test]
    fn a_second_patch_merges_onto_the_first() {
        let _guard = lock_config_registry_for_test();
        clear_override_paths();

        let dir = scratch_dir("accumulate");
        install_config_override(ConfigOverrideKind::Combat, r#"{"lethality": 2.0}"#, &dir)
            .expect("first patch installs");
        let installed = install_config_override(
            ConfigOverrideKind::Combat,
            r#"{"disengage_fraction": 0.25}"#,
            &dir,
        )
        .expect("second patch installs");

        let written: Value =
            serde_json::from_str(&fs::read_to_string(&installed.path).expect("read back"))
                .expect("the installed file is JSON");
        assert_eq!(written["lethality"], serde_json::json!(2.0));
        assert_eq!(written["disengage_fraction"], serde_json::json!(0.25));

        clear_config_overrides();
        assert!(override_path_for(spec_for(ConfigOverrideKind::Combat).env_var).is_none());
    }
}
