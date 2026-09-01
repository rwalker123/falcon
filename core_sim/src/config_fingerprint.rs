//! **What tuning was live when a world booted**, per config file, so a load can say which files
//! moved since a save was written.
//!
//! One global hash over every config would answer *"config changed"*, which a player cannot act on.
//! Per-file digests answer *"`fauna_config.json` and `recipes.json` changed"*, which names the two
//! files to look at. That is the whole reason this is a map rather than a `u64`.
//!
//! ## Two layers, and the split matters
//!
//! - A **process-global registry** of what would load right now, written by the two seams where
//!   effective tuning is decided: [`crate::config_load::load_config_from_env`] at boot, and
//!   `config_override::install_config_override` when the tuning panel stages an edit. It is global
//!   for the same reason the override registry beside it is: the ~37 boot configs resolve through a
//!   free function called before any `World` exists.
//! - A [`ConfigFingerprint`] **resource**, snapshotted out of that registry when the app is built.
//!
//! The snapshot is what makes this correct across a staged override. Staging one updates the
//! *registry* — the tuning the **next** `new_game` will boot on — while the running world keeps the
//! resource it was built with, which is what it is actually simulating. A save therefore records
//! what its world ran, and a load compares that against what the loading process would run.
//!
//! ## Hashing
//!
//! [`FnvHasher`], the repo's deterministic hasher. `DefaultHasher` is randomized per process, so a
//! digest built with it would differ between the save and the load for reasons that have nothing to
//! do with tuning.

use std::collections::BTreeMap;
use std::hash::Hasher;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::hashing::FnvHasher;

/// What one config resolved to.
///
/// `Builtin` is deliberately **not** a hash of the compiled-in text. "No file was there, so the
/// `include_str!` copy loaded" and "a file was there and hashed to N" are different facts about
/// where tuning came from, and collapsing them would make a build that ships a file
/// indistinguishable from one that does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigDigest {
    /// No file at the default path, so the compiled-in copy loaded — the one benign absence
    /// (see [`crate::config_load::resolve_config`]).
    Builtin,
    /// FNV-1a 64 of the exact bytes that loaded, from whichever path won the precedence ladder.
    File(u64),
}

/// Per-config-file digests of the tuning a world booted on.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFingerprint {
    /// Keyed by the shipped file's **name** (`fauna_config.json`), which is what a warning has to
    /// print. Every boot config lives in `core_sim/src/data/`, so the name is unambiguous, and a
    /// `BTreeMap` keeps the encoded save byte-reproducible.
    entries: BTreeMap<String, ConfigDigest>,
}

impl ConfigFingerprint {
    pub fn digest(&self, file_name: &str) -> Option<ConfigDigest> {
        self.entries.get(file_name).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, ConfigDigest)> {
        self.entries
            .iter()
            .map(|(name, digest)| (name.as_str(), *digest))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// FNV-1a of a config's bytes. Free-standing so both seams hash the same way.
fn digest_of(bytes: &[u8]) -> u64 {
    let mut hasher = FnvHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// The shipped file's name, which is the registry key — see [`ConfigFingerprint`].
fn key_for(default_rel_path: &str) -> String {
    Path::new(default_rel_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| default_rel_path.to_string())
}

static DIGESTS: OnceLock<RwLock<BTreeMap<String, ConfigDigest>>> = OnceLock::new();

fn digests() -> &'static RwLock<BTreeMap<String, ConfigDigest>> {
    DIGESTS.get_or_init(|| RwLock::new(BTreeMap::new()))
}

/// A poisoned lock means some thread panicked mid-update, not that the map is unusable — every
/// entry is an independently-written digest. Same reasoning as the override registry's `recover`.
fn recover<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Record what a boot load resolved to. `source` is the path that won, or `None` for the builtin.
///
/// A file that cannot be re-read here is left **absent** rather than guessed at: an absent entry
/// compares as a change, and erring toward "tuning may have moved" is the safe direction for a
/// warning whose whole job is to stop a silent one.
pub(crate) fn record_loaded_config(default_rel_path: &str, source: Option<&Path>) {
    let digest = match source {
        None => ConfigDigest::Builtin,
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => ConfigDigest::File(digest_of(&bytes)),
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "config.fingerprint.unreadable"
                );
                recover(digests().write()).remove(&key_for(default_rel_path));
                return;
            }
        },
    };
    recover(digests().write()).insert(key_for(default_rel_path), digest);
}

/// Record the effective text of a config the tuning panel just staged.
///
/// The panel merges a patch into whatever would load right now and writes the result to a scratch
/// file; hashing that text here keeps the registry describing **effective** tuning rather than
/// shipped tuning, which is the case a file-bytes-only fingerprint would call unchanged.
pub(crate) fn record_config_text(default_rel_path: &str, text: &str) {
    recover(digests().write()).insert(
        key_for(default_rel_path),
        ConfigDigest::File(digest_of(text.as_bytes())),
    );
}

/// The tuning a world built **now** would boot on.
pub fn current_config_fingerprint() -> ConfigFingerprint {
    ConfigFingerprint {
        entries: recover(digests().read()).clone(),
    }
}

/// Drop every recorded digest. Test-only: the registry is process-global, so a case that stages an
/// override would otherwise leak its digest into every later fingerprint in the same process.
#[cfg(test)]
pub(crate) fn clear_config_fingerprint() {
    recover(digests().write()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_PATH: &str = "src/data/fingerprint_test_a.json";
    const B_PATH: &str = "src/data/fingerprint_test_b.json";
    const SOME_JSON: &str = "{\"x\":1}";

    #[test]
    fn the_key_is_the_shipped_file_name() {
        assert_eq!(key_for("src/data/fauna_config.json"), "fauna_config.json");
    }

    #[test]
    fn the_builtin_is_not_a_hash_of_anything() {
        // `Builtin` must not be expressible as a `File` digest, or "shipped no file" and "shipped
        // a file that happened to hash to N" would compare equal.
        assert_ne!(ConfigDigest::Builtin, ConfigDigest::File(digest_of(b"")));
    }

    #[test]
    fn different_text_gives_a_different_digest() {
        assert_ne!(digest_of(b"{\"a\":1}"), digest_of(b"{\"a\":2}"));
        assert_eq!(digest_of(b"{\"a\":1}"), digest_of(b"{\"a\":1}"));
    }

    /// **The boot seam records the bytes that loaded**, and a different file gives a different
    /// digest. This is the case a load-time warning turns on.
    #[test]
    fn the_boot_seam_digests_the_file_that_actually_loaded() {
        let _guard = crate::config_load::lock_config_registry_for_test();
        clear_config_fingerprint();

        const ENV_VAR: &str = "SHADOW_SCALE_FINGERPRINT_TEST_PATH";
        // Never written, so "no override named" resolves to the builtin.
        const ABSENT_DEFAULT: &str = "src/data/fingerprint_boot_test.json";
        const FIRST_TUNING: &str = "17";
        const RETUNED: &str = "23";

        let dir =
            std::env::temp_dir().join(format!("shadow_scale_fingerprint_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let file = dir.join("tuning.json");

        let load = || {
            crate::config_load::load_config_from_env(
                ENV_VAR,
                "fingerprint_test",
                ABSENT_DEFAULT,
                || std::sync::Arc::new(0u32),
                |path: &std::path::Path| {
                    std::fs::read_to_string(path)
                        .map_err(|source| ProbeError::Read { source })
                        .and_then(|text| text.trim().parse::<u32>().map_err(|_| ProbeError::Parse))
                },
            )
        };

        // With no file anywhere, the builtin loaded — a fact, not a hash.
        crate::config_load::clear_override_paths();
        std::env::remove_var(ENV_VAR);
        load();
        assert_eq!(
            current_config_fingerprint().digest("fingerprint_boot_test.json"),
            Some(ConfigDigest::Builtin),
            "an absent default must record Builtin, not a digest of the compiled-in text"
        );

        std::fs::write(&file, FIRST_TUNING).expect("write tuning");
        std::env::set_var(ENV_VAR, &file);
        load();
        let before = current_config_fingerprint();
        let first = before.digest("fingerprint_boot_test.json");
        assert_eq!(
            first,
            Some(ConfigDigest::File(digest_of(FIRST_TUNING.as_bytes())))
        );

        // Edit the file. The digest must move — this is the whole feature.
        std::fs::write(&file, RETUNED).expect("retune");
        load();
        let after = current_config_fingerprint();
        assert_ne!(
            after.digest("fingerprint_boot_test.json"),
            first,
            "editing a config file must change its digest"
        );
        assert_ne!(before, after, "and therefore the fingerprint as a whole");

        std::env::remove_var(ENV_VAR);
        clear_config_fingerprint();
    }

    /// **A staged override moves the fingerprint too**, and this is the case a file-bytes-only
    /// digest would miss: the tuning panel edits numbers that never reach a shipped file, so a
    /// fingerprint taken from `src/data/` alone would report "unchanged" for a world whose tuning
    /// was edited. That silent false negative is what the feature exists to prevent.
    #[test]
    fn staging_an_override_moves_the_fingerprint() {
        use crate::config_override::{clear_config_overrides, install_config_override};
        use sim_runtime::commands::ConfigOverrideKind;

        let _guard = crate::config_load::lock_config_registry_for_test();
        clear_config_overrides();
        clear_config_fingerprint();

        let kind = ConfigOverrideKind::Simulation;
        let file_name =
            std::path::Path::new(crate::config_override::spec_for(kind).default_rel_path)
                .file_name()
                .expect("the spec names a file")
                .to_string_lossy()
                .into_owned();

        let before = current_config_fingerprint();
        assert_eq!(
            before.digest(&file_name),
            None,
            "nothing has been loaded or staged yet"
        );

        let dir = std::env::temp_dir().join(format!(
            "shadow_scale_fingerprint_override_{}",
            std::process::id()
        ));
        // A lever the panel actually exposes, set to a value the shipped file does not carry.
        install_config_override(kind, "{\"crisis_auto_seed\": true}", &dir)
            .expect("a valid patch installs");

        let after = current_config_fingerprint();
        assert!(
            matches!(after.digest(&file_name), Some(ConfigDigest::File(_))),
            "staging an override must record the EFFECTIVE tuning, not leave the entry absent"
        );
        assert_ne!(before, after, "the fingerprint must move");

        clear_config_overrides();
        clear_config_fingerprint();
    }

    /// A stand-in config error for the boot-seam case above.
    #[derive(Debug)]
    enum ProbeError {
        Read { source: std::io::Error },
        Parse,
    }

    impl std::fmt::Display for ProbeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Read { source } => write!(f, "read: {source}"),
                Self::Parse => write!(f, "parse"),
            }
        }
    }

    impl crate::config_load::ConfigLoadError for ProbeError {
        fn is_not_found(&self) -> bool {
            matches!(self, Self::Read { source } if source.kind() == std::io::ErrorKind::NotFound)
        }
    }

    #[test]
    fn recorded_text_lands_under_the_file_name() {
        record_config_text(A_PATH, SOME_JSON);
        record_config_text(B_PATH, SOME_JSON);
        let fingerprint = current_config_fingerprint();
        assert_eq!(
            fingerprint.digest("fingerprint_test_a.json"),
            Some(ConfigDigest::File(digest_of(SOME_JSON.as_bytes())))
        );
        // Two configs with identical text still occupy two entries, so a warning can name either.
        assert_eq!(
            fingerprint.digest("fingerprint_test_b.json"),
            fingerprint.digest("fingerprint_test_a.json")
        );
    }
}
