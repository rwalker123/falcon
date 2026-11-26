//! **The scenario schema cannot drift from the loader or from the checked-in scenarios.**
//!
//! `scenario::scenario_schema()` is generated from the structs `parse_scenario` deserializes, and the
//! Workbench builds its forms from it. These tests walk every `core_sim/scenarios/*.json` against it:
//! each object's keys must be exactly its schema's properties (an `Option` key may be left out, and
//! nothing else may be), each `oneOf` value must match exactly one variant, every `x-pick: tile`
//! location holds a two-integer array, and every `x-pick: band | herd` location names an id the file
//! itself declares.

use std::collections::BTreeSet;
use std::path::PathBuf;

use core_sim::scenario::{list_scenario_files, scenario_schema};
use serde_json::Value;

/// The ids a file declares, which the pick locations must resolve to.
struct Declared {
    bands: BTreeSet<String>,
    herds: BTreeSet<String>,
}

fn declared(file: &Value) -> Declared {
    let ids = |items: Option<&Vec<Value>>| -> BTreeSet<String> {
        items
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    };
    let bands = file["factions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|faction| ids(faction["bands"].as_array()))
        .collect();
    Declared {
        bands,
        herds: ids(file["herds"].as_array()),
    }
}

fn allows_null(schema: &Value) -> bool {
    let typed_null = match &schema["type"] {
        Value::String(kind) => kind == "null",
        Value::Array(kinds) => kinds.iter().any(|kind| kind == "null"),
        _ => false,
    };
    typed_null
        || schema["anyOf"]
            .as_array()
            .is_some_and(|branches| branches.iter().any(allows_null))
}

fn is_tile(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|pair| pair.len() == 2 && pair.iter().all(|n| n.as_u64().is_some()))
}

/// Walk `value` against `schema`, pushing one message per disagreement.
fn walk(value: &Value, schema: &Value, at: &str, ids: &Declared, errors: &mut Vec<String>) {
    if value.is_null() {
        if !allows_null(schema) {
            errors.push(format!("{at}: null where the schema allows none"));
        }
        return;
    }
    if let Some(branches) = schema["anyOf"].as_array() {
        // An `Option<T>`: the non-null branch describes the value.
        for branch in branches.iter().filter(|b| !allows_null_only(b)) {
            let mut scratch = Vec::new();
            walk(value, branch, at, ids, &mut scratch);
            if scratch.is_empty() {
                return;
            }
        }
        errors.push(format!("{at}: matches no branch of its anyOf"));
        return;
    }
    if let Some(variants) = schema["oneOf"].as_array() {
        let matching: Vec<&Value> = variants
            .iter()
            .filter(|variant| {
                let mut scratch = Vec::new();
                walk(value, variant, at, ids, &mut scratch);
                scratch.is_empty()
            })
            .collect();
        if matching.len() != 1 {
            errors.push(format!(
                "{at}: matches {} oneOf variants, expected exactly 1",
                matching.len()
            ));
        }
        return;
    }
    if let Some(constant) = schema.get("const") {
        if value != constant {
            errors.push(format!("{at}: {value} is not the constant {constant}"));
        }
        return;
    }
    if let Some(options) = schema["enum"].as_array() {
        if !options.contains(value) {
            errors.push(format!("{at}: {value} is not one of {options:?}"));
        }
        return;
    }
    match pick(schema) {
        Some("tile") if !is_tile(value) => {
            errors.push(format!("{at}: x-pick tile must be a [x, y] integer pair"));
        }
        Some(kind @ ("band" | "herd")) => match value.as_str() {
            Some(id) => {
                let known = if kind == "band" {
                    &ids.bands
                } else {
                    &ids.herds
                };
                if !known.contains(id) {
                    errors.push(format!("{at}: '{id}' names no {kind} the file declares"));
                }
            }
            None => errors.push(format!("{at}: x-pick {kind} must be a string")),
        },
        _ => {}
    }
    match value {
        Value::Object(map) => walk_object(map, schema, at, ids, errors),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, &schema["items"], &format!("{at}[{i}]"), ids, errors);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

/// Keys that name a band or a herd by its scenario id, and what each must be annotated as. A new
/// section's key of one of these names that forgets its `x-pick` is caught here.
const NAME_KEYS: [(&str, &str); 5] = [
    ("band", "band"),
    ("keeper", "band"),
    ("herd", "herd"),
    ("herd_id", "herd"),
    ("fauna_id", "herd"),
];

fn allows_null_only(schema: &Value) -> bool {
    schema["type"] == "null"
}

fn pick(schema: &Value) -> Option<&str> {
    schema["x-pick"].as_str()
}

fn walk_object(
    map: &serde_json::Map<String, Value>,
    schema: &Value,
    at: &str,
    ids: &Declared,
    errors: &mut Vec<String>,
) {
    let Some(properties) = schema["properties"].as_object() else {
        // A map (`kits`, `archetypes`): every value follows `additionalProperties`.
        match schema.get("additionalProperties") {
            Some(Value::Object(_)) => {
                for (key, item) in map {
                    walk(
                        item,
                        &schema["additionalProperties"],
                        &format!("{at}.{key}"),
                        ids,
                        errors,
                    );
                }
            }
            _ => errors.push(format!("{at}: the schema describes no object here")),
        }
        return;
    };
    for (key, property) in properties {
        let is_pair = property["minItems"] == 2 && property["maxItems"] == 2;
        if is_pair && pick(property) != Some("tile") {
            errors.push(format!(
                "{at}.{key}: a two-element array with no x-pick tile"
            ));
        }
        if let Some(kind) = NAME_KEYS
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, k)| *k)
        {
            if pick(property) != Some(kind) {
                errors.push(format!(
                    "{at}.{key}: a key naming a {kind} with no x-pick {kind}"
                ));
            }
        }
    }
    for key in map.keys() {
        if !properties.contains_key(key) {
            errors.push(format!("{at}.{key}: a key the schema does not have"));
        }
    }
    let required: BTreeSet<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    for (key, property) in properties {
        match map.get(key) {
            Some(item) => walk(item, property, &format!("{at}.{key}"), ids, errors),
            None if required.contains(key.as_str()) => {
                errors.push(format!("{at}.{key}: a required key the file lacks"));
            }
            None => {}
        }
    }
    if let Some(pair) = schema["x-pick-tile"].as_array() {
        let keys: Vec<&str> = pair.iter().filter_map(Value::as_str).collect();
        if keys.len() != 2 || keys.iter().any(|key| !properties.contains_key(*key)) {
            errors.push(format!("{at}: x-pick-tile names keys the object lacks"));
        }
        for key in keys {
            if let Some(item) = map.get(key) {
                if !item.is_null() && item.as_u64().is_none() {
                    errors.push(format!("{at}.{key}: a tile coordinate must be an integer"));
                }
            }
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core_sim sits in the repo root")
        .to_path_buf()
}

/// Every checked-in scenario, as `(path, parsed JSON)`.
fn checked_in() -> Vec<(String, Value)> {
    let root = repo_root();
    let files = list_scenario_files(&root);
    assert!(!files.is_empty(), "no scenarios found under {root:?}");
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(root.join(&path)).expect("scenario reads");
            let json = serde_json::from_str(&text).expect("scenario is JSON");
            (path, json)
        })
        .collect()
}

#[test]
fn every_checked_in_scenario_matches_the_generated_schema() {
    let schema = scenario_schema();
    let mut failures = Vec::new();
    for (path, file) in checked_in() {
        let mut errors = Vec::new();
        walk(&file, &schema, "$", &declared(&file), &mut errors);
        failures.extend(errors.into_iter().map(|e| format!("{path}: {e}")));
    }
    assert!(
        failures.is_empty(),
        "scenario files drifted from the schema:\n{}",
        failures.join("\n")
    );
}

/// The walk is only worth anything if it can fail: a key the schema lacks, a wrong tile and an
/// undeclared band each make it report.
#[test]
fn the_walk_reports_a_drifted_file() {
    let schema = scenario_schema();
    let (_, mut file) = checked_in()
        .into_iter()
        .find(|(path, _)| path.ends_with("kept_roads.json"))
        .expect("kept_roads ships");
    file["factions"][0]["bands"][0]["surplus_key"] = Value::from(1);
    file["factions"][0]["bands"][0]["at"] = Value::from(vec![1u64, 2, 3]);
    file["factions"][0]["roads"][2]["keeper"] = Value::from("nobody");
    file["factions"][0]["labor"][0] = serde_json::json!({ "no_such_verb": {} });
    let mut errors = Vec::new();
    walk(&file, &schema, "$", &declared(&file), &mut errors);
    let joined = errors.join("\n");
    for expected in [
        "surplus_key: a key the schema does not have",
        "bands[0].at: x-pick tile",
        "names no band the file declares",
        "labor[0]: matches 0 oneOf variants",
    ] {
        assert!(
            joined.contains(expected),
            "missing '{expected}' in:\n{joined}"
        );
    }
}

#[test]
fn the_schema_carries_the_pick_annotations_and_closes_every_object() {
    let schema = scenario_schema();
    let text = schema.to_string();
    for needle in [
        "\"x-pick\":\"tile\"",
        "\"x-pick\":\"band\"",
        "\"x-pick\":\"herd\"",
        "\"x-pick-tile\":[\"target_x\",\"target_y\"]",
    ] {
        assert!(text.contains(needle), "schema lacks {needle}");
    }
    assert_eq!(schema["additionalProperties"], false);
    let band = &schema["properties"]["factions"]["items"]["properties"]["bands"]["items"];
    assert_eq!(band["properties"]["at"]["x-pick"], "tile");
    assert_eq!(band["additionalProperties"], false);
    let labor = &schema["properties"]["factions"]["items"]["properties"]["labor"]["items"];
    let variants = labor["oneOf"].as_array().expect("labor entries are oneOf");
    assert_eq!(variants.len(), 3);
    for variant in variants {
        assert_eq!(variant["required"].as_array().map(Vec::len), Some(1));
        assert_eq!(variant["additionalProperties"], false);
    }
}
