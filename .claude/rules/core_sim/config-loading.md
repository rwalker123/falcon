---
paths:
  - "core_sim/src/config_load.rs"
  - "core_sim/src/*_config.rs"
  - "core_sim/src/{resources,map_preset,start_profile,victory,intensification}.rs"
  - "core_sim/src/telling/{catalog,config}.rs"
  - "core_sim/src/bin/server.rs"
---

# Boot config loading

Every boot-time config in `core_sim` has the same three parts: a **builtin** baked in with
`include_str!` of a file under `src/data/`, that same path as the **default** on-disk source, and an
optional **`*_CONFIG_PATH`** environment override. What used to differ was the *failure* handling —
each of the ~26 `load_*_from_env` entry points hand-rolled its own `warn!`-and-fall-through. This
file is the rationale for the shared seam that replaced them (`config_load.rs`, issue #361).

## The rule: only an absent DEFAULT path falls back

| Situation | Outcome |
|---|---|
| No override named, default path **absent** | Builtin. **The only benign case.** |
| No override named, default path **present but unreadable/unparseable/invalid** | **Boot panic** |
| `*_CONFIG_PATH` names a file that **is not there** | **Boot panic** |
| `*_CONFIG_PATH` names a file that is **broken** | **Boot panic** |

The absent-default case is benign for one specific reason: **the builtin *is* `include_str!` of that
exact path**, so substituting it substitutes nothing. Every other case substitutes *different
numbers* — the sim runs tuning nobody chose while the operator's edit sits on disk looking live. A
`tracing::warn!` in a headless server log is not loud enough to catch that; this is
`.github/copilot-instructions.md` item 10.

**A parsed-but-incoherent config counts as broken.** The validate steps in `labor_config`,
`fauna_config`, `flora_config` and `creatures_config` used to log `error!` + `*.invalid_rejected` and
fall back. They now panic, because "looks live but isn't" is precisely the case the rule exists to
close.

**Strictness without a loud loader is worse than neither.** The lesson from #350/#358: making a
schema strict (`deny_unknown_fields`, required fields) while the loader still swallows does not
remove the silent substitution — it *moves it one layer out*, from one fat-fingered key to the whole
file. Tighten the schema and the loader together or not at all.

## The seam

`config_load.rs` holds three things, and the split between them is the point:

- **`trait ConfigLoadError { fn is_not_found(&self) -> bool }`** — the single distinction the
  fallback turns on. Implement it on each config's error enum returning `true` **only** for the
  io-read variant whose source is `ErrorKind::NotFound`; every parse and validate variant returns
  `false`, because those describe a file that is there and wrong.
- **`resolve_config(...)`** — the whole rule, and **pure**: no env access, no logging, no panic. That
  is what makes it unit-testable without touching a process-global `*_CONFIG_PATH`, and it is why
  the five tests in that module describe the *rule* (against a throwaway config type) rather than any
  one config's tuning.
- **`load_config_from_env(...)`** — the boot wrapper: env var → path → `resolve_config` → panic with
  the path, the error and the remedy, then the `{label}.loaded=file|builtin` info event on
  `target: "shadow_scale::config"`. It returns `(Arc<T>, Option<PathBuf>)` and lets each caller wrap
  that `Option` in its own `*Metadata` resource — that is what keeps one helper generic over ~20
  distinct metadata structs.

**Adding a config loader?** Implement `ConfigLoadError`, then call `load_config_from_env`. Do not
hand-roll a candidate loop or a fallback branch — if `grep -rn 'load_failed"' core_sim/src` returns
anything outside `bin/server.rs`, a loader has drifted back off the seam.

## Boot panics, hot reload does NOT — the asymmetry is deliberate

`bin/server.rs`'s `handle_reload_config` calls each config's `from_file` **directly**, keeps its
`warn!`, and stays non-fatal. That is not an oversight:

- At **boot** there is no world yet, `build_headless_app` hands back an `App` rather than a `Result`,
  and there is no recovery that isn't "run different numbers than the operator asked for".
- At **runtime** a live campaign is in memory. A typo in a hot-reloaded file must log and leave the
  running world alone, not kill the server mid-turn.

So the two paths answer opposite questions and correctly reach opposite conclusions. **Do not
"unify" them.**

## What is deliberately NOT covered

`#[serde(default)]` was left in place everywhere by #361. Several configs still carry it, so their
"broken" surface is malformed JSON only — a *partial* file still parses and silently takes defaults
for the missing keys. Whether a given config should require its fields is a **per-config** decision
(demographics made it, #350), separate from the loader. The seam guarantees the file the operator
named is the file that loads; it does not guarantee that file is complete.

## Config files

This rule owns no `src/data/*.json` of its own — it is the loading mechanism, and each config's key
table lives with the arc that owns it (see the routing table in `core_sim/CLAUDE.md`).
