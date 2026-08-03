//! One manifest, two readers.
//!
//! `clients/godot_thin_client/src/config/tuning_manifest.json` is the client's curated list of
//! tunable levers, and the client cannot ask the server what any of them currently are — the
//! command channel is one-way. So the manifest carries its own `default`, `min`, `max` and `type`,
//! and every one of those can rot the moment somebody retunes a config or renames a key.
//!
//! A stale entry can only ever mis-render a *hint* (edits ship as a sparse patch, so the server's
//! real values still win), but a hint that quietly lies about the shipped default is exactly the
//! kind of thing a designer trusts. This test is the drift guard: it loads the same file the client
//! does and checks every entry against the config `core_sim` actually ships.

use std::path::PathBuf;

use core_sim::config_override_spec_for;
use serde::Deserialize;
use serde_json::Value;
use sim_runtime::commands::ConfigOverrideKind;

#[derive(Debug, Deserialize)]
struct TuningManifest {
    kinds: Vec<TuningKind>,
}

#[derive(Debug, Deserialize)]
struct TuningKind {
    kind: String,
    env_var: String,
    params: Vec<TuningParam>,
}

#[derive(Debug, Deserialize)]
struct TuningParam {
    pointer: String,
    #[serde(rename = "type")]
    value_type: String,
    min: f64,
    max: f64,
    step: f64,
    default: f64,
}

/// Located relative to `CARGO_MANIFEST_DIR` rather than a hardcoded absolute path, so the test
/// works from any checkout — including the several worktrees this repo is developed in at once.
fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../clients/godot_thin_client/src/config/tuning_manifest.json")
}

/// Resolve a JSON pointer, returning `None` for a path that does not exist — a renamed key.
fn resolve<'a>(doc: &'a Value, pointer: &str) -> Option<&'a Value> {
    doc.pointer(pointer)
}

#[test]
fn every_tuning_manifest_entry_matches_the_shipped_config() {
    let path = manifest_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read the tuning manifest at {}: {err}", path.display()));
    let manifest: TuningManifest = serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("parse the tuning manifest at {}: {err}", path.display()));

    assert!(
        !manifest.kinds.is_empty(),
        "the tuning manifest declares no kinds; a silently emptied manifest renders an empty panel"
    );

    for kind_entry in &manifest.kinds {
        let kind = ConfigOverrideKind::from_wire_str(&kind_entry.kind).unwrap_or_else(|| {
            panic!(
                "tuning manifest kind '{}' is not a ConfigOverrideKind; \
                 the server would reject every override it stages",
                kind_entry.kind
            )
        });
        let spec = config_override_spec_for(kind);
        assert_eq!(
            kind_entry.env_var, spec.env_var,
            "tuning manifest kind '{}' names env var {}, but the server loads it from {}",
            kind_entry.kind, kind_entry.env_var, spec.env_var
        );

        // The builtin is `include_str!` of `spec.default_rel_path`, so this IS the shipped file.
        let shipped: Value = serde_json::from_str(spec.builtin_json).unwrap_or_else(|err| {
            panic!(
                "shipped {} config is not valid JSON: {err}",
                kind_entry.kind
            )
        });

        assert!(
            !kind_entry.params.is_empty(),
            "tuning manifest kind '{}' declares no parameters",
            kind_entry.kind
        );

        for param in &kind_entry.params {
            let where_ = format!("{} {}", kind_entry.kind, param.pointer);

            let value = resolve(&shipped, &param.pointer).unwrap_or_else(|| {
                panic!(
                    "{where_}: the pointer does not resolve in {} — the key was renamed or removed",
                    spec.default_rel_path
                )
            });
            let number = value
                .as_f64()
                .unwrap_or_else(|| panic!("{where_}: resolves to {value}, which is not a number"));

            match param.value_type.as_str() {
                // An `int` lever renders a whole-number spinner, so a fractional shipped value
                // means the manifest is describing a different parameter than the one it points at.
                // `float` stays permissive: JSON writes `8` and `8.0` interchangeably, and both are
                // legitimate for a real-valued knob.
                "int" => assert!(
                    value.as_i64().is_some() || value.as_u64().is_some(),
                    "{where_}: declared int but ships as {value}"
                ),
                "float" => {}
                other => {
                    panic!("{where_}: unknown declared type '{other}' (expected int or float)")
                }
            }

            // The actual drift guard. A retuned config with a stale manifest entry renders a
            // "default 8.0" caption beside a value that has been 12.0 for a month.
            assert_eq!(
                param.default, number,
                "{where_}: the manifest declares default {} but {} ships {number}",
                param.default, spec.default_rel_path
            );

            assert!(
                param.min <= param.default && param.default <= param.max,
                "{where_}: default {} is outside the declared range [{}, {}]",
                param.default,
                param.min,
                param.max
            );
            assert!(
                param.step > 0.0,
                "{where_}: step {} must be positive, or the spinner cannot move",
                param.step
            );
        }
    }
}
