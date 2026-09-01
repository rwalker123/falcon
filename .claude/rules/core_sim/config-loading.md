---
paths:
  - "core_sim/src/config_load.rs"
  - "core_sim/src/config_override.rs"
  - "core_sim/tests/tuning_manifest_drift.rs"
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

Precedence is **staged override → `*_CONFIG_PATH` → default file → builtin** (the first rung is the
in-process registry described under "Staged overrides" below; before that arc there were only the
last three). "Override named" in the table means *either* of the first two.

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

## Staged overrides — the client's tuning panel, and the third path

The Config Tuning panel edits numbers in the client and starts a run on them without restarting the
server (`docs/plan_config_tuning_panel.md`). Two existing facts carry it:

- **`load_config_from_env` is the one place that decides which file loads**, so the override is an
  **in-process registry consulted ahead of the env var** (`OVERRIDE_PATHS` in `config_load.rs`;
  `set_override_path` / `clear_override_paths`). Deliberately **not** `std::env::set_var` — the
  server is multithreaded and live, and mutating the process environment to hand a value between two
  parts of the *same* process is the wrong tool for a decision that already has a home.
- **`new_game` already re-reads every config** (`handle_new_game` → `rebuild_world_from_config` →
  `build_headless_app`, a wall of `load_*_from_env` calls), so "restart the sim on new tuning" needs
  no process restart.

**The `simulation` kind needs one extra step, and it is not optional.** The other four kinds are
installed as their own resources inside `build_headless_app` and are simply whatever loaded. The
`SimulationConfig` is not: the rebuild overwrites it with a config the *caller* supplies, and
`handle_new_game` used to supply a clone of the outgoing world's — which discarded the fresh load
and made every `simulation` lever on the tuning panel inert. It now starts from
`load_simulation_config_for_new_world` (`resources.rs`), which loads afresh and then carries back a
deliberately narrow set.

**The file is the authority at world start; only what the file CANNOT know gets carried.** Anything
the file *could* have said is a tunable, and a carried tunable is permanently un-overridable — a
staged override for it would install, log, and do nothing, re-creating the very bug this function
fixed. Two things qualify:

- **`fog_enabled`** — not a tunable at all. It is a *player preference* with its own persisted home
  in the client (`.claude/rules/client/fog-of-war.md`), pushed over as a `set_fog` command; it would
  never appear on the tuning panel, and resetting it every New Game would be a visible regression.
- **the four bind addresses** — port allocation auto-bumps on a collision, and a fresh load can only
  reproduce an explicit `SIM_PORT_BASE`, never a bump. The in-world config must describe the ports
  the process actually holds.

**`crisis_auto_seed` does not qualify**, though `set_crisis_auto_seed` writes it at runtime: it sits
in `simulation_config.json` among exactly the levers the panel exists to change, so it comes from the
fresh load like any other tunable. The cost is re-issuing one debug command after a New Game.
`start_profile_id`/`start_profile_overrides` are runtime-owned too but likewise not carried —
`apply_start_profile` re-applies the profile the command names right after. `ResetMap` keeps cloning
the running config: it is a map reroll, not a retune.

**A `load_game` needs the same two, and asks the same list for them.** `handle_load_game` does not
go through `rebuild_world_from_config` — it builds its replacement app with `build_headless_app` and
therefore arrives holding the *file's* fog switch and the *file's* four binds. Both callers get the
set from `carry_runtime_owned_fields` (`resources.rs`), and
`load_simulation_config_for_new_world` is now a fresh load plus that call, because a carried-field
list written at two sites is a list that will disagree. Fog is the half a player sees: turn it off,
save, load, and the reveal frame `publish_loaded_world` captures comes back **fogged** — with the
herds filtered out of the payload — until the client's reconcile round-trips a `set_fog` and forces a
recapture. The ports half is quieter and just as wrong: a process whose block auto-bumped would go
back to naming the file's sockets, and the next `reload_config` would log a spurious
`socket_changed=restart_required`. `bin/server.rs`'s
`a_load_carries_the_fields_the_config_file_cannot_know` asserts both, on the config *and* on the
published frame.

**The staged file is never the watched file.** The rebuild keeps the outgoing world's
`SimulationConfigMetadata` path, so the config watcher still watches the shipped
`simulation_config.json`. Pointing it at `config_overrides/simulation.json` would make each staged
edit hot-reload into the *running* world, which is the opposite of the "applies at the next New
Game" contract. The consequence to know: a `reload_config` after a New Game booted on an override
reloads the shipped file, dropping the override from the live world until the next New Game.

`resolve_config` stays **pure** — the registry lookup lives in `load_config_from_env` only, so the
rule above is still unit-testable without touching any process-global state.

**Both override seams also write the config fingerprint** — `load_config_from_env` records the bytes
that actually loaded, and `install_config_override` records the merged text it stages. See
`.claude/rules/core_sim/checkpoints.md` → "The config fingerprint is per file, and it has two seams"
for what that is for; the fact worth having here is that neither seam may change what loads without
also saying so.

`config_override.rs` is the seam the `set_config_override` command lands on. Per kind it holds the
env var, the shipped path, the builtin, and a `validate` fn that runs the kind's **own
`from_json_str`** — a `match`, not a table lookup, so a new `ConfigOverrideKind` cannot compile
until it names a config. Installing means: resolve the kind's *current effective* JSON (same
precedence as above, so successive edits accumulate rather than each starting from the shipped
file), deep-merge the sparse patch (RFC 7386 minus null-deletion), **validate**, and only then write
`config_overrides/<kind>.json` and register it.

**Validating before installing is the whole safety argument, not defensive coding.** The boot path
panics on a present-but-broken file *on purpose*; an override staged without validation would
therefore not fail at the edit, it would kill the server at the **next New Game**, arbitrarily far
from the edit that caused it. Do not weaken the boot path to compensate — the third path answers by
**rejecting at the edit**, where a human is watching and the UI can say so. A rejected patch writes
no file, registers nothing, and leaves the running world untouched; it logs
`config_override.rejected` at `warn!`.

The `set_config_override` / `clear_config_overrides` commands are **not** written to the replay log
for the same reason `reload_config` is not: they change what the *next* world boots on, and a
`SimState` carries no config.

**One manifest, two readers.** The client's curated lever list
(`clients/godot_thin_client/src/config/tuning_manifest.json`) carries its own `default`/`min`/`max`,
because the command channel is one-way and it cannot ask. `core_sim/tests/tuning_manifest_drift.rs`
loads that same file and asserts every pointer resolves in the shipped config, the type matches, and
**the declared default equals the shipped value** — so a retune or a renamed key fails CI instead of
silently rendering a wrong hint.

## What is deliberately NOT covered

`#[serde(default)]` was left in place everywhere by #361. Several configs still carry it, so their
"broken" surface is malformed JSON only — a *partial* file still parses and silently takes defaults
for the missing keys. Whether a given config should require its fields is a **per-config** decision
(demographics made it, #350), separate from the loader. The seam guarantees the file the operator
named is the file that loads; it does not guarantee that file is complete.

## Config files

This rule owns no `src/data/*.json` of its own — it is the loading mechanism, and each config's key
table lives with the arc that owns it (see the routing table in `core_sim/CLAUDE.md`).
