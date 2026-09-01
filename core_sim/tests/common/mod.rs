//! Comparing two encoded checkpoints, without comparing their map ORDER.
//!
//! Shared by more than one integration-test binary. Each binary compiles this module separately and
//! uses a different subset of it, so the unused half is dead code in that binary and only in that
//! binary — the standard shape for `tests/common`.
//!
//! ## Why a byte comparison is the wrong instrument between two worlds
//!
//! Encoding one world twice gives identical bytes — `HashMap` iteration is a deterministic function
//! of a map's contents *and its table capacity*, and encoding the same instance twice walks the same
//! table. Encoding **two different instances that hold equal content** does not: a map the sim grew
//! entry by entry and a map serde rebuilt with a size hint have different capacities, so they list
//! the same entries in different orders.
//!
//! That is a difference in the encoding, not in the world. Eight checkpoint fields still hold
//! `HashMap`s (`ForageRegistry::patches`, `GrazeRegistry::patches`, `PowerGridState::nodes`, the
//! three `CultureManagerCheckpoint` layer maps, `DiscoveryProgressLedger::progress`,
//! `GreatDiscoveryReadiness::per_faction`), and each of them can order two equal worlds differently.
//!
//! So the comparison sorts every map by the encoded form of its key, recursively, and compares the
//! result. **Arrays are left alone**: a `Vec`'s order is meaningful state — `SimState::tiles` is
//! sorted by `(y, x)` and `bands` by `BandId` precisely so a checkpoint compares — and sorting them
//! too would blind the comparison to a real reordering.
//!
//! `PartialEq` is deliberately not used for any of this: `LaborAllocation`'s hand-written impl
//! compares intent and skips its telemetry, so value equality would pass while dropping fields.
//! serde walks every field regardless.

#![allow(dead_code)]

use ciborium::value::Value;

/// Encode with CBOR and parse back into a tree with every map in canonical key order.
pub fn canonical_tree<T: serde::Serialize>(value: &T) -> Value {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("the value encodes");
    let raw: Value = ciborium::from_reader(bytes.as_slice()).expect("the encoding parses");
    canonical(&raw)
}

/// The same, starting from bytes that are already encoded.
pub fn canonical_tree_of_bytes(bytes: &[u8]) -> Value {
    let raw: Value = ciborium::from_reader(bytes).expect("the encoding parses");
    canonical(&raw)
}

/// Put every map's entries in a canonical order, recursively. Arrays keep theirs.
pub fn canonical(value: &Value) -> Value {
    match value {
        Value::Map(entries) => {
            let mut canonical_entries: Vec<(Value, Value)> = entries
                .iter()
                .map(|(key, val)| (canonical(key), canonical(val)))
                .collect();
            canonical_entries.sort_by_cached_key(|(key, _)| {
                let mut bytes = Vec::new();
                ciborium::into_writer(key, &mut bytes).expect("a map key re-encodes");
                bytes
            });
            Value::Map(canonical_entries)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Tag(tag, inner) => Value::Tag(*tag, Box::new(canonical(inner))),
        other => other.clone(),
    }
}

/// **Every field path on which two serializable values disagree, named.**
///
/// `assert_eq!` on two whole worlds is unusable as a failure report: it prints both, and the one
/// field that moved is somewhere inside megabytes of identical structure. This walks the two in
/// step and returns `section.field[i].leaf: <before> -> <after>` for each disagreement, so a failing
/// assertion *names the section that broke* rather than asking a reader to diff two dumps.
///
/// Object keys are matched by name and arrays pairwise by index, with a length line where they
/// differ — deliberately not a set comparison, because a `Vec`'s order is meaningful state
/// everywhere in this schema (see this module's header).
///
/// Leaf values are truncated to [`DIFF_VALUE_CHARS`]: the point of the line is *which* field, and a
/// 4,160-sample raster printed whole would bury the twenty other paths beside it.
pub fn differing_paths<T: serde::Serialize>(before: &T, after: &T) -> Vec<String> {
    let before = serde_json::to_value(before).expect("the value serializes");
    let after = serde_json::to_value(after).expect("the value serializes");
    let mut paths = Vec::new();
    walk_difference("", &before, &after, &mut paths);
    paths
}

/// How much of a differing leaf value a path line quotes. See [`differing_paths`].
const DIFF_VALUE_CHARS: usize = 120;

fn walk_difference(
    path: &str,
    before: &serde_json::Value,
    after: &serde_json::Value,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    match (before, after) {
        (Value::Object(before_fields), Value::Object(after_fields)) => {
            let mut names: Vec<&std::string::String> =
                before_fields.keys().chain(after_fields.keys()).collect();
            names.sort();
            names.dedup();
            for name in names {
                let child = format!("{path}.{name}");
                match (before_fields.get(name), after_fields.get(name)) {
                    (Some(a), Some(b)) => walk_difference(&child, a, b, out),
                    (Some(_), None) => out.push(format!("{child}: present before, absent after")),
                    (None, Some(_)) => out.push(format!("{child}: absent before, present after")),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(before_items), Value::Array(after_items)) => {
            if before_items.len() != after_items.len() {
                out.push(format!(
                    "{path}: length {} -> {}",
                    before_items.len(),
                    after_items.len()
                ));
            }
            for (index, (a, b)) in before_items.iter().zip(after_items.iter()).enumerate() {
                walk_difference(&format!("{path}[{index}]"), a, b, out);
            }
        }
        _ if before != after => {
            let (a, b) = (before.to_string(), after.to_string());
            out.push(format!(
                "{path}: {} -> {}",
                &a[..a.len().min(DIFF_VALUE_CHARS)],
                &b[..b.len().min(DIFF_VALUE_CHARS)]
            ));
        }
        _ => {}
    }
}

/// Whether any map in the tree carries `key` as a text key.
pub fn mentions_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Map(entries) => entries
            .iter()
            .any(|(k, v)| matches!(k, Value::Text(text) if text == key) || mentions_key(v, key)),
        Value::Array(items) => items.iter().any(|item| mentions_key(item, key)),
        Value::Tag(_, inner) => mentions_key(inner, key),
        _ => false,
    }
}
