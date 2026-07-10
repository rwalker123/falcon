# GitHub Copilot Instructions — Shadow-Scale (falcon)

These instructions apply to all Copilot interactions in this repository, including the **code review agent**. Use them to evaluate diffs against project conventions. When a PR violates a rule below, flag it as a review comment with the rule name.

The repository is a Cargo workspace: a Bevy-based headless ECS simulation (`core_sim`), pure FlatBuffers/data contracts (`sim_schema`), shared runtime utilities (`sim_runtime`), generated FlatBuffers bindings (`shadow_scale_flatbuffers`), a Godot thin client with a Rust GDExtension (`clients/godot_thin_client` + `clients/godot_thin_client/native`), build tooling (`xtask`, `tools/script_harness`), and integration tests (`integration_tests`). Follow **DRY and SOLID** — reject shortcuts and premature abstractions alike.

---

## 1. Absolute Prohibitions (flag any violation)

1. **Never hand-edit generated FlatBuffers bindings.** Files under `shadow_scale_flatbuffers/src/generated/**` (e.g. `snapshot_generated.rs`) are build outputs. Changes belong in the `.fbs` schema, followed by regeneration (see §3). Flag any manual edit.
2. **Never silence Clippy or the compiler to pass CI.** Inline `#[allow(...)]`, crate-level `#![allow(...)]`, or `#[allow(clippy::...)]` added only to suppress a warning are forbidden — CI runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Fix the underlying issue. A genuinely justified allow must carry a comment explaining why.
3. **Never introduce `unwrap()`, `expect()`, panicking indexing, or `panic!`/`todo!`/`unimplemented!` in simulation or server hot paths** (`core_sim/src`, `sim_runtime/src`, the server binary, snapshot/broadcast code). These run headless and must not crash the turn loop — return `Result`/`Option` and handle the error. `unwrap`/`expect` are acceptable in tests, benches, `xtask`, and one-time startup where a failure is genuinely unrecoverable and clearly intended.
4. **Never hardcode simulation tunables (magic numbers) that belong in config.** Grid size, thresholds, multipliers, clamps, decay rates, sight ranges, etc. are data-driven via the JSON files under `core_sim/src/data/` (e.g. `simulation_config.json`, `turn_pipeline_config.json`, `visibility_config.json`, `crisis_*.json`). New tunables must be added to the appropriate config and loaded, not baked into code.
5. **Never break the snapshot contract silently.** Changes to `sim_schema/schemas/*.fbs` (especially `snapshot.fbs`) are cross-cutting: both `core_sim` (producer) and the Godot client (consumer, via the native extension) must be updated in the same PR, and bindings regenerated (§3). Flag a `.fbs` edit whose consumers or generated bindings were not updated.
6. **Never add heavy dependencies to `sim_schema`.** It is a pure data-contract crate and purposely avoids Bevy and other heavy deps so tooling and external consumers can depend on it cheaply. Flag any Bevy/engine dependency added there.
7. **Never commit secrets.** Flag any added credential file, API key, token, or password in the diff.
8. **Never commit large binary/build artifacts or scratch files.** Flag additions like `target/**`, `tmp_*.bin`, `tmp_flat/**`, generated texture atlases, or `.godot/` caches. These are not source.
9. **Never leave the code unformatted or with a stale `Cargo.lock`.** CI enforces `cargo fmt --all -- --check` and builds `--locked`. A diff that changes dependencies must include the matching `Cargo.lock` update; a diff that touches Rust must be `rustfmt`-clean.

## 2. Comments & Documentation

- **Do not add comments that restate what the code does** or reference the PR/task ("// fix for review", "// added minimap"). Prefer self-documenting names. Comments are for non-obvious invariants, units, coordinate-system conventions, or safety justifications.
- Follow the **document hierarchy** (see root `CLAUDE.md`): gameplay-facing concepts go in the manual (`shadow_scale_strategy_game_concept_technical_plan_v_0.md`) first; implementation details go in the owning subsystem's `CLAUDE.md` (`core_sim/CLAUDE.md`, `clients/godot_thin_client/CLAUDE.md`); cross-system concerns go in `docs/architecture.md`; concrete work goes in `TASKS.md`. Flag PRs that add a new subsystem/system/config without the corresponding doc update, and avoid duplicating implementation details across files (define once in the owning doc, cross-link with "See Also").

## 3. FlatBuffers Schema & Generated Bindings

The schema in `sim_schema/schemas/snapshot.fbs` is the authoritative contract between the simulation and all clients.

- After editing a `.fbs`, regenerate: `cargo build -p shadow_scale_flatbuffers`, then `rustfmt shadow_scale_flatbuffers/src/generated/snapshot_generated.rs`, and rebuild the Godot native extension with `cargo xtask godot-build` so the client decoder matches.
- The generated bindings are **not committed** — `shadow_scale_flatbuffers/src/generated/snapshot_generated.rs` is gitignored (`.gitignore`) and regenerated from the schema by `build.rs` (`cargo:rerun-if-changed` on the `.fbs`). CI rebuilds and rustfmts it, then fails if it ends up missing or unformatted. Do not add the generated file to a commit, and do not treat its absence from the checkout as a defect.
- Schema evolution should be additive and back-compatible where possible (append new fields/tables; don't renumber or repurpose existing fields). Flag breaking reorderings.

## 4. `core_sim` — Simulation Engine (Bevy ECS)

- **Turn loop ordering matters.** Systems execute in a defined `TurnStage` sequence (materials → logistics → population → visibility → crisis → power → tick → snapshot; see `core_sim/CLAUDE.md` for the authoritative order). A new system must be registered in the correct stage; flag additions that run in the wrong phase or bypass `run_turn`.
- **Respect capability gating.** Systems are inert until their `CapabilityFlags` bit is set. New subsystems should gate behind the appropriate flag rather than always running.
- **Config-driven, hot-reloadable.** New tuning surfaces should be loadable via the existing `reload_config` paths where applicable (turn / overlay / crisis / visibility). Don't add a config file with no loader.
- **Determinism.** The simulation must be reproducible from a seed. Flag nondeterministic sources in turn resolution — wall-clock time, unseeded RNG, or iteration over unordered collections (`HashMap`) where output order affects results. Use seeded RNG and stable ordering.
- **Snapshot/rollback integrity.** Changes to world state that must survive rollback need to be represented in `WorldSnapshot`/`WorldDelta`. Flag new persistent state that isn't captured in the snapshot ring buffer.

## 5. Godot Thin Client (`clients/godot_thin_client`)

- **Reuse shared UI helpers — don't reimplement.** HUD panels that expand to fit content must attach `src/scripts/ui/AutoSizingPanel.gd` and call `fit_to_content` rather than rolling bespoke height/scroll logic (root `CLAUDE.md`). The minimap must reuse `src/scripts/ui/MinimapPanel.gd`, not a copy.
- **Single sources of truth.** Terrain data comes from `assets/terrain/TerrainDefinitions.gd`; textures load through the `TerrainTextureManager` autoload singleton (shared by 2D and 3D). Flag duplicated terrain tables or ad-hoc per-renderer texture loading.
- **Typography via `Typography.gd`** and the shared `Theme` resource — don't hardcode font sizes; derive from the typography map (`body`/`heading`/`caption`/`legend`/`control`).
- **The client is a thin inspector.** It renders snapshots and issues vetted commands — it must not contain simulation/game logic that belongs in `core_sim`. Flag business logic leaking into GDScript.
- **Native extension** (`native/`) is the only place that decodes FlatBuffers on the client; keep decoding there rather than parsing bytes in GDScript.

## 6. Rust Coding Style

- Rust 2021+, `rustfmt` defaults (CI enforces `cargo fmt --all -- --check`). Four-space indent.
- `snake_case` for functions/variables/modules, `CamelCase` for types/traits/enums, `SCREAMING_SNAKE_CASE` for consts/statics.
- Prefer iterators and pattern matching over manual index loops; prefer `?` and typed errors over `unwrap` in fallible paths.
- Keep functions cohesive; extract shared logic (DRY) but don't create speculative abstractions — three similar lines beat a premature trait. No dead code or unused `pub` surface.
- Prefer editing existing files over creating new ones; match the surrounding module's idioms.

## 7. Testing

- Tests run via `cargo test --workspace --locked` (CI). Unit tests live in `#[cfg(test)]` modules beside source or under a crate's `tests/`; cross-crate flows live in `integration_tests`.
- Benchmarks live under `core_sim/benchmarks` (`cargo bench -p core_sim`). Don't regress a benchmarked hot path without noting it.
- Behavior changes to simulation systems, worldgen, or snapshot shapes must come with test coverage or updated fixtures in the same PR. Flag simulation logic changes with no accompanying test.
- Determinism/regression: worldgen and turn-resolution changes should be validated against seeded expectations.

## 8. Security

- Flag command injection, path traversal, or unsanitized input in tooling and the command/log/snapshot socket handlers.
- The client accepts data over TCP sockets (`41001`/`41002`/`41003`) — decoders must handle malformed/oversized frames without panicking or unbounded allocation. Flag missing length/bounds validation on incoming frames.
- The scripting sandbox (QuickJS) must enforce its capability model and memory/instruction limits; flag capability escalation or unsandboxed file/network access from user scripts.

---

## Review Checklist (use for every PR)

1. Does the diff violate any absolute prohibition in §1 (generated bindings, clippy suppression, panics in hot paths, hardcoded tunables, broken schema contract, `sim_schema` deps, secrets, artifacts, formatting/lock)?
2. If a `.fbs` schema changed, were bindings regenerated and both producer and client consumers updated (§3, §1.5)?
3. Are new `core_sim` systems registered in the correct `TurnStage`, capability-gated, deterministic, and snapshot-safe (§4)?
4. Do new tunables live in `core_sim/src/data/*.json` with a loader, not as magic numbers (§1.4, §4)?
5. Does Godot code reuse shared helpers (`AutoSizingPanel`, `MinimapPanel`, `TerrainDefinitions`, `TerrainTextureManager`, `Typography`) instead of reimplementing (§5)?
6. Is simulation logic kept in `core_sim`, not leaking into the thin client (§5)?
7. Is the Rust `rustfmt`-clean, clippy-clean under `-D warnings`, and is `Cargo.lock` consistent (§1.9, §6)?
8. Are behavior changes covered by tests / updated fixtures, and are benchmarked paths not silently regressed (§7)?
9. Are socket decoders and the script sandbox robust against malformed input and capability escalation (§8)?
10. Were the right docs updated per the hierarchy in root `CLAUDE.md` (§2)?

When in doubt, flag for human review rather than approve. Schema/contract changes, turn-loop ordering changes, and anything affecting determinism or rollback always warrant explicit maintainer sign-off.
