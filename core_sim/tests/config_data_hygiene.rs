//! **Hygiene guards over every shipped config in `core_sim/src/data/`.**
//!
//! These configs are documentation as much as data: the `_comment_*` keys carry the *why* behind
//! every lever (the no-magic-numbers rule means a number's justification lives beside it). That makes
//! them worth a structural guard, because their failure mode is **silent**.

use std::{collections::BTreeMap, fs, path::PathBuf};

/// Every `*.json` under `core_sim/src/data/`, in a stable order (a directory read is not ordered).
fn shipped_configs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the shipped config directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no configs found — the guard is vacuous");
    paths
}

/// **A key written twice is a key silently deleted.** `serde_json` accepts a duplicate and keeps the
/// **last**, so the earlier one — invariably a `_comment_*` explaining a lever — vanishes with no
/// error, no warning, and nothing to notice until someone goes looking for the explanation and finds
/// it gone. That is exactly what happened to `labor_config.json`'s `_comment_cultivation`: two
/// adjacent keys of that name, and the one recording *where the plant rung-2 build dials moved to*
/// was dropped on every load.
///
/// Caught by parsing with a hook that sees the raw key/value pairs **before** serde folds them into a
/// map — the only point at which the duplicate still exists.
#[test]
fn no_shipped_config_names_a_key_twice() {
    let mut offenders: Vec<String> = Vec::new();
    for path in shipped_configs() {
        let text = fs::read_to_string(&path).expect("config is readable");
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        // A deserializer that counts keys per object instead of collapsing them.
        let mut duplicates: Vec<String> = Vec::new();
        let mut deserializer = serde_json::Deserializer::from_str(&text);
        serde::de::DeserializeSeed::deserialize(
            DuplicateKeySeed {
                duplicates: &mut duplicates,
            },
            &mut deserializer,
        )
        .unwrap_or_else(|err| panic!("{name} must be valid JSON: {err}"));

        for key in duplicates {
            offenders.push(format!("{name}: '{key}'"));
        }
    }
    assert!(
        offenders.is_empty(),
        "these configs name a key twice — serde keeps only the LAST, so the other is silently \
         dropped (rename it, don't delete it):\n  {}",
        offenders.join("\n  ")
    );
}

/// Every config must actually parse — nearly free once the file is being read, and it keeps
/// `no_shipped_config_names_a_key_twice` honest (that guard would pass vacuously on a file that never
/// parsed at all).
#[test]
fn every_shipped_config_is_valid_json() {
    for path in shipped_configs() {
        let text = fs::read_to_string(&path).expect("config is readable");
        serde_json::from_str::<serde_json::Value>(&text)
            .unwrap_or_else(|err| panic!("{} must be valid JSON: {err}", path.display()));
    }
}

// --- The duplicate-detecting deserializer. `serde_json::Value` cannot express a duplicate (it *is*
// the collapse), so the check has to sit at the visitor, where the pairs still arrive one by one.

/// Threads one `duplicates` sink through every nested object/array of a document.
struct DuplicateKeySeed<'a> {
    duplicates: &'a mut Vec<String>,
}

impl<'de> serde::de::DeserializeSeed<'de> for DuplicateKeySeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for DuplicateKeySeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            *seen.entry(key).or_insert(0) += 1;
            map.next_value_seed(DuplicateKeySeed {
                duplicates: self.duplicates,
            })?;
        }
        for (key, count) in seen {
            if count > 1 {
                self.duplicates.push(key);
            }
        }
        Ok(())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        while seq
            .next_element_seed(DuplicateKeySeed {
                duplicates: self.duplicates,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }
    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }
}
