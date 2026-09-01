//! The one implementation of "which config does the sim boot on, and which failures are fatal".
//!
//! Every boot-time config in `core_sim` follows the same shape: a builtin baked in with
//! `include_str!` of a file under `src/data/`, that same path as the default on-disk source, and an
//! optional `*_CONFIG_PATH` environment override. What used to differ was the *failure* handling —
//! each loader hand-rolled a `warn!` and fell through to the builtin, so a present-but-broken file
//! ran tuning nobody chose. [`resolve_config`] states the rule once; [`load_config_from_env`] is
//! the boot wrapper every `load_*_from_env` delegates to.
//!
//! **This is the boot path only.** The server's `handle_reload_config` hot-reload calls each
//! config's `from_file` directly and is deliberately *non*-fatal: a bad edit while a world is
//! running must log and keep the live world, not kill the server. The asymmetry is the point —
//! at boot there is no world to keep.

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
};

/// Staged in-process config overrides: `*_CONFIG_PATH` name → the file to load instead.
///
/// Consulted **ahead of** the environment variable by [`load_config_from_env`], so the precedence
/// is registry → `*_CONFIG_PATH` → default file → builtin. The client's tuning panel writes a
/// merged config to a scratch file and registers it here; because `new_game` rebuilds the app and
/// re-runs every `load_*_from_env`, that is all it takes for the next world to boot on it.
///
/// **Deliberately not `std::env::set_var`.** The server is multithreaded and live, so mutating the
/// process environment is both racy against any concurrent `getenv` and the wrong tool for handing
/// a value between two parts of the *same* process — this decision already has a home, and it is
/// this map.
static OVERRIDE_PATHS: OnceLock<RwLock<HashMap<String, PathBuf>>> = OnceLock::new();

fn override_paths() -> &'static RwLock<HashMap<String, PathBuf>> {
    OVERRIDE_PATHS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// A poisoned registry lock means some thread panicked mid-update, not that the map is unusable —
/// every entry is an independently-written `PathBuf`. Recovering the inner guard keeps a
/// config-panel mishap from bricking every later config load in the process.
fn recover<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Stage `path` as the file `env_var` names, from the next load of that config onward.
///
/// The caller is responsible for having **validated** the file first: this seam feeds the boot
/// loader, which panics on a present-but-broken file by design (see [`resolve_config`]).
pub fn set_override_path(env_var: &str, path: PathBuf) {
    recover(override_paths().write()).insert(env_var.to_string(), path);
}

/// Drop every staged override, so the next load falls back to `*_CONFIG_PATH` and the shipped
/// files again.
pub fn clear_override_paths() {
    recover(override_paths().write()).clear();
}

/// The staged path for `env_var`, if one was set. Crate-private on purpose: [`load_config_from_env`]
/// stays the single place the registry can change *what loads*, exactly as `env::var` was, and the
/// only other reader (`config_override`) uses it to answer "what would load right now?" before
/// merging a patch into it.
pub(crate) fn override_path_for(env_var: &str) -> Option<PathBuf> {
    recover(override_paths().read()).get(env_var).cloned()
}

/// Implemented by every boot-time config error enum. The NotFound-vs-broken split is the ONE
/// distinction the fallback rule turns on — see [`resolve_config`].
///
/// Implementors return `true` **only** for the io-read variant whose source is
/// [`std::io::ErrorKind::NotFound`]; every parse and validate variant returns `false`, because
/// those describe a file that is there and wrong.
pub trait ConfigLoadError {
    fn is_not_found(&self) -> bool;
}

/// Decide *which* config the sim runs on, and *which* failures are fatal. Pure: no environment
/// access, no logging, no panic, so the whole rule is unit-testable.
///
/// **Exactly one case falls back to the builtin: no override was named and the default path is
/// absent.** That is benign because the builtin is `include_str!` of that very file — the fallback
/// substitutes nothing. Every other outcome is an error:
/// - a **present but unreadable/unparseable/invalid** default file — someone's edit is
///   live-looking but broken, and quietly running the compiled-in tuning instead discards it;
/// - an **explicit override that is missing** — the operator named a file; ignoring it silently is
///   the bug;
/// - an **explicit override that is broken** — same, one step later.
///
/// A config that parsed but violates its own invariants counts as broken here: "looks live but
/// isn't" is exactly the case the rule exists to close.
pub fn resolve_config<T, E: ConfigLoadError>(
    override_path: Option<&Path>,
    default_path: &Path,
    builtin: impl FnOnce() -> Arc<T>,
    from_file: impl FnOnce(&Path) -> Result<T, E>,
) -> Result<(Arc<T>, Option<PathBuf>), E> {
    let path = override_path.unwrap_or(default_path);
    match from_file(path) {
        Ok(config) => Ok((Arc::new(config), Some(path.to_path_buf()))),
        Err(err) if override_path.is_none() && err.is_not_found() => Ok((builtin(), None)),
        Err(err) => Err(err),
    }
}

/// Appended to the boot panic so the message says what to do, not just what broke. Covers both
/// fatal shapes (a broken file, and an override naming one that isn't there), since only an absent
/// *default* path is benign. It names both override sources because either can be the one at
/// fault — a staged path (whose scratch file may have been deleted under the running server) or the
/// environment variable.
fn load_failure_remedy(env_var: &str) -> String {
    format!(
        "the sim will NOT fall back to the builtin here — only an absent default path is benign. \
         Fix the file, or the staged config override / {env_var} that names it"
    )
}

/// Boot-load a config from the staged override, `env_var`, or the default data path, emitting the
/// `shadow_scale::config` load event.
///
/// Precedence is **registry → `*_CONFIG_PATH` → default file → builtin**: a path staged with
/// [`set_override_path`] wins over the environment, because it was set by a human at the client's
/// tuning panel *during this run* while the env var was fixed at launch. Both are "an override the
/// operator named", so both take the same strict failure handling below.
///
/// `default_rel_path` is resolved against this crate's `CARGO_MANIFEST_DIR` — every boot config
/// ships from `core_sim/src/data/`, so the manifest dir is the same for all callers and computing
/// it here keeps it from being re-derived twenty times.
///
/// Returns the config plus the on-disk path it came from (`None` for the builtin); the caller
/// wraps that `Option<PathBuf>` in its own `*Metadata` resource, which is what keeps this helper
/// generic over the ~20 distinct metadata structs.
///
/// **Panics** when the resolved file exists but cannot be read, parsed, or validated, or when an
/// explicit `env_var` names a file that is not there — see [`resolve_config`] for why only an
/// absent *default* path is benign. Boot of a headless server is the right place for it:
/// `build_headless_app` hands back an `App`, not a `Result`, no world exists yet, and there is no
/// recovery that isn't "run different numbers than the operator asked for".
pub fn load_config_from_env<T, E: ConfigLoadError + std::fmt::Display>(
    env_var: &str,
    label: &str,
    default_rel_path: &str,
    builtin: impl FnOnce() -> Arc<T>,
    from_file: impl FnOnce(&Path) -> Result<T, E>,
) -> (Arc<T>, Option<PathBuf>) {
    let override_path =
        override_path_for(env_var).or_else(|| env::var(env_var).ok().map(PathBuf::from));
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(default_rel_path);

    let (config, source) =
        match resolve_config(override_path.as_deref(), &default_path, builtin, from_file) {
            Ok(loaded) => loaded,
            Err(err) => panic!(
                "{label} at {} could not be loaded: {err}; {}",
                override_path.as_deref().unwrap_or(&default_path).display(),
                load_failure_remedy(env_var)
            ),
        };

    // The fingerprint records the bytes that ACTUALLY loaded, whichever rung of the precedence
    // ladder won — so a staged override is already covered here, not just the shipped file.
    crate::config_fingerprint::record_loaded_config(default_rel_path, source.as_deref());

    match source.as_ref() {
        Some(path) => tracing::info!(
            target: "shadow_scale::config",
            path = %path.display(),
            "{label}.loaded=file"
        ),
        None => tracing::info!(
            target: "shadow_scale::config",
            "{label}.loaded=builtin"
        ),
    }

    (config, source)
}

/// Serializes every test that touches the process-global override registry (or a `*_CONFIG_PATH`
/// env var, which is just as global). [`clear_override_paths`] wipes **all** entries, so two such
/// tests on cargo's thread pool would tear down each other's setup. Shared with `config_override`'s
/// tests, which install into the same map.
#[cfg(test)]
pub(crate) fn lock_config_registry_for_test() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A poisoned lock only means some earlier test panicked; the guarded state is rebuilt by each
    // test anyway, so recovering keeps one failure from cascading into every other case.
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io};
    use thiserror::Error;

    /// A stand-in for any real config: one field, so a file can be "valid", "broken" or absent.
    #[derive(Debug, Clone, PartialEq)]
    struct TestConfig {
        value: u32,
    }

    /// The builtin the fallback is allowed to substitute. Distinct from every value written to
    /// disk in these tests, so "the builtin loaded" can never be mistaken for "the file loaded".
    const BUILTIN_VALUE: u32 = 7;

    #[derive(Debug, Error)]
    enum TestConfigError {
        #[error("failed to read test config from {path:?}: {source}")]
        Read {
            path: PathBuf,
            #[source]
            source: io::Error,
        },
        #[error("failed to parse test config: {0}")]
        Parse(String),
    }

    impl ConfigLoadError for TestConfigError {
        fn is_not_found(&self) -> bool {
            match self {
                Self::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
                Self::Parse(_) => false,
            }
        }
    }

    fn builtin() -> Arc<TestConfig> {
        Arc::new(TestConfig {
            value: BUILTIN_VALUE,
        })
    }

    fn from_file(path: &Path) -> Result<TestConfig, TestConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| TestConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        contents
            .trim()
            .parse::<u32>()
            .map(|value| TestConfig { value })
            .map_err(|err| TestConfigError::Parse(err.to_string()))
    }

    /// A scratch directory unique per test, so the cases can't collide with each other (or with a
    /// concurrent `cargo test` run) over a shared file name.
    fn scratch_dir(case: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "shadow_scale_config_load_{}_{case}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn resolve(
        override_path: Option<&Path>,
        default_path: &Path,
    ) -> Result<(Arc<TestConfig>, Option<PathBuf>), TestConfigError> {
        resolve_config(override_path, default_path, builtin, from_file)
    }

    /// The **only** benign absence: nobody named a file, and the default one isn't there. The
    /// builtin is `include_str!` of exactly that path, so falling back substitutes nothing.
    #[test]
    fn an_absent_default_path_falls_back_to_the_builtin() {
        let missing = scratch_dir("absent_default").join("not_written.json");
        let (config, source) = resolve(None, &missing).expect("an absent default is benign");
        assert_eq!(config.value, BUILTIN_VALUE);
        assert!(
            source.is_none(),
            "the builtin has no on-disk source to report"
        );
    }

    /// A *present* default file that no longer parses is the defect this rule closes: falling back
    /// runs tuning nobody chose while the operator's edit sits on disk looking live.
    #[test]
    fn a_broken_default_file_is_fatal() {
        let path = scratch_dir("broken_default").join("config.json");
        fs::write(&path, "not-a-number").expect("write the broken config");

        let err =
            resolve(None, &path).expect_err("a present-but-broken default must not fall back");
        assert!(
            matches!(err, TestConfigError::Parse(_)),
            "the breakage must surface as the parse error, not as NotFound, got {err}"
        );
        assert!(!err.is_not_found());
    }

    /// The operator named a file. Not finding it and quietly running the shipped builtin is exactly
    /// the ignored-override bug.
    #[test]
    fn a_missing_explicit_override_is_fatal() {
        let dir = scratch_dir("missing_override");
        let missing = dir.join("experiment.json");
        let err = resolve(Some(&missing), &dir.join("default.json"))
            .expect_err("a named-but-absent override must not fall back");
        assert!(
            err.is_not_found(),
            "the failure must be the missing override itself, got {err}"
        );
    }

    /// Same for an override that is there and wrong — the fallback would run the builtin the
    /// operator explicitly overrode.
    #[test]
    fn a_broken_explicit_override_is_fatal() {
        let path = scratch_dir("broken_override").join("experiment.json");
        fs::write(&path, "not-a-number").expect("write the broken override");

        let err = resolve(Some(&path), Path::new("unused_default.json"))
            .expect_err("a broken override must not fall back");
        assert!(
            matches!(err, TestConfigError::Parse(_)),
            "the breakage must surface as the parse error, got {err}"
        );
    }

    /// The happy override path wins over the default, and reports its own path so the boot log
    /// names the tuning that actually ran.
    #[test]
    fn a_valid_explicit_override_wins_and_reports_its_path() {
        let dir = scratch_dir("valid_override");
        let override_path = dir.join("experiment.json");
        let default_path = dir.join("default.json");
        fs::write(&override_path, "11").expect("write the override");
        fs::write(&default_path, "22").expect("write the default");

        let (config, source) =
            resolve(Some(&override_path), &default_path).expect("override loads");
        assert_eq!(config.value, 11, "the override must win over the default");
        assert_eq!(source.as_deref(), Some(override_path.as_path()));
    }

    /// The staged-override ladder, as `load_config_from_env` sees it: **registry beats
    /// `*_CONFIG_PATH`, which beats the shipped file.** The registry wins because it was set by a
    /// human during this run, while the env var was fixed at launch.
    ///
    /// Driven through `load_config_from_env` rather than `resolve_config` on purpose —
    /// `resolve_config` stays pure and knows nothing of either source, so the ladder itself only
    /// exists in the wrapper and only the wrapper can be wrong about it.
    #[test]
    fn a_staged_override_outranks_the_env_var_which_outranks_the_builtin() {
        let _guard = lock_config_registry_for_test();

        const ENV_VAR: &str = "SHADOW_SCALE_CONFIG_LOAD_PRECEDENCE_TEST_PATH";
        const STAGED_VALUE: u32 = 31;
        const ENV_VALUE: u32 = 41;
        // Never written, so the "no override at all" rung resolves to the builtin.
        const ABSENT_DEFAULT_REL_PATH: &str = "src/data/__config_load_precedence_test_absent.json";

        let dir = scratch_dir("precedence");
        let staged = dir.join("staged.json");
        let from_env = dir.join("from_env.json");
        fs::write(&staged, STAGED_VALUE.to_string()).expect("write the staged override");
        fs::write(&from_env, ENV_VALUE.to_string()).expect("write the env override");

        let load = || {
            load_config_from_env(
                ENV_VAR,
                "precedence_test_config",
                ABSENT_DEFAULT_REL_PATH,
                builtin,
                from_file,
            )
        };

        env::set_var(ENV_VAR, &from_env);
        set_override_path(ENV_VAR, staged.clone());
        let (config, source) = load();
        assert_eq!(
            config.value, STAGED_VALUE,
            "the staged override must beat the env var"
        );
        assert_eq!(source.as_deref(), Some(staged.as_path()));

        clear_override_paths();
        let (config, source) = load();
        assert_eq!(
            config.value, ENV_VALUE,
            "with nothing staged the env var must load again"
        );
        assert_eq!(source.as_deref(), Some(from_env.as_path()));

        env::remove_var(ENV_VAR);
        let (config, source) = load();
        assert_eq!(
            config.value, BUILTIN_VALUE,
            "with neither source and an absent default, the builtin is the benign fallback"
        );
        assert!(source.is_none());
    }

    /// Overrides are keyed by env-var name, so staging one config cannot move another. The registry
    /// is process-global; a leak across keys would retune a config nobody touched.
    #[test]
    fn a_staged_override_only_affects_its_own_env_var() {
        let _guard = lock_config_registry_for_test();

        const STAGED_VAR: &str = "SHADOW_SCALE_CONFIG_LOAD_ISOLATION_TEST_STAGED_PATH";
        const OTHER_VAR: &str = "SHADOW_SCALE_CONFIG_LOAD_ISOLATION_TEST_OTHER_PATH";
        const STAGED_VALUE: u32 = 55;
        const ABSENT_DEFAULT_REL_PATH: &str = "src/data/__config_load_isolation_test_absent.json";

        let dir = scratch_dir("isolation");
        let staged = dir.join("staged.json");
        fs::write(&staged, STAGED_VALUE.to_string()).expect("write the staged override");

        clear_override_paths();
        env::remove_var(OTHER_VAR);
        set_override_path(STAGED_VAR, staged);

        let (config, source) = load_config_from_env(
            OTHER_VAR,
            "isolation_test_config",
            ABSENT_DEFAULT_REL_PATH,
            builtin,
            from_file,
        );
        assert_eq!(
            config.value, BUILTIN_VALUE,
            "the other config must not move"
        );
        assert!(source.is_none());

        clear_override_paths();
    }
}
