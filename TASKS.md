# Prototype Task List

## Core Simulation (`core_sim`)
- [x] Flesh out deterministic ECS systems (materials, logistics, population).
- [x] Replace placeholder system with staged schedules and fixed-point math.
- [x] Add snapshot/delta serialization hooks feeding `sim_schema` schemas.
- [x] Draft power phase architecture updates that align `run_turn`'s power stage with `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §4, enumerate required energy subsystems (generation forms, instability handling, grid state), and spill follow-on implementation tickets once documented (Owner: TBD, Estimate: 1.5d; Deps: review existing materials/logistics schedules). _Status_: Plan captured in `docs/architecture.md` §Power Systems Plan and synced with manual §4 Power Simulation Pillars.
- [x] Capture knowledge ledger/leak mechanic design in `docs/architecture.md` per manual §5a—cover timers, espionage modifiers, UI data feeds—and produce downstream tasks for backend and Godot wiring (Owner: TBD, Estimate: 1.5d; Deps: Great Discovery architecture outline). _Status_: Architecture captured in `docs/architecture.md` §Knowledge Ledger & Leak Mechanics with manual §5a alignment; telemetry/command integrations enumerated and follow-on tickets below._
- [x] Stand up `KnowledgeLedger` infrastructure in `core_sim` (resource representations, stage registration, baseline leak math) according to `docs/architecture.md` §Knowledge Ledger & Leak Mechanics (Owner: Systems Team — Ravi, Estimate: 2d; Deps: trade diffusion plumbing, Great Discovery hooks). _Status_: Stage registration now runs the full leak math pipeline, espionage plumbing feeds infiltrations/countermeasures, and telemetry/snapshot surfaces are live (`core_sim/src/knowledge_ledger.rs`, `core_sim/src/lib.rs`, `core_sim/src/snapshot.rs`)._
  - [x] Implement leak modifier aggregation + half-life recomputation inside `knowledge_ledger_tick` following architecture table values. _Status_: Tick loop now folds modifier, countermeasure, and infiltration pressure into effective half-life and progress deltas (`core_sim/src/knowledge_ledger.rs`)._
  - [x] Persist espionage probes/counter-intel events into the ledger (requires upcoming espionage plumbing). _Status_: Mission resolution emits probe/sweep events and `process_espionage_events` records infiltrations plus countermeasures each turn (`core_sim/src/espionage.rs`, `core_sim/src/knowledge_ledger.rs`)._
  - [x] Build espionage agent & mission scheduling systems that emit `EspionageProbeEvent` / `CounterIntelSweepEvent` (Owner: Systems Team — Ravi, Estimate: 3d; Deps: ledger scaffolding, agent roster design). _Status_: `core_sim/src/espionage.rs` now loads data-driven agent/mission catalogs, queues missions via `EspionageMissionState::queue_mission`, resolves them before `process_espionage_events`, and is covered by `core_sim/src/espionage.rs` unit tests; mission metadata lives in `core_sim/src/data/espionage_{agents,missions}.json`._
- [x] Implement counter-intelligence mission scheduling & defensive sweeps (Owner: Systems Team — Ravi, Estimate: 2d; Deps: espionage mission system). _Status_: `core_sim/src/espionage.rs` now auto-schedules defensive sweeps via `schedule_counter_intel_missions`, integrates with mission resolution/tests, and runs each turn before `process_espionage_events`; telemetry-ready sweeps clear infiltrations and register countermeasures._
  - [x] Introduce counter-intel budget & policy hooks controlling defensive auto-scheduling (Owner: Systems Team — Ravi, Estimate: 1.5d; Deps: counter-intel scheduling, design docs updated). _Status_: `CounterIntelBudgets` and `FactionSecurityPolicies` now gate `schedule_counter_intel_missions`; sweeps spend/regen per `core_sim/src/data/espionage_config.json`, log budget shortfalls, and record usage via `SimulationMetrics::knowledge_counterintel_budget_spent` with coverage in `core_sim/src/espionage.rs` tests._
  - [x] Expose runtime commands for counter-intel posture and budgets (Owner: Systems Team — Ravi, Estimate: 1d; Deps: budget/policy hooks). _Status_: Added `counterintel_policy` / `counterintel_budget` commands via `sim_runtime` schema + CLI parsing, `core_sim` server handlers adjust `FactionSecurityPolicies`/`CounterIntelBudgets`, and docs reflect operator controls._
  - [x] Expose generated mission variants through telemetry/commands so UI tooling can list auto-generated probe templates. (Owner: Systems Team — Ravi, Estimate: 1d; Deps: mission generator output.) _Status_: `knowledge_ledger_tick` now emits `missions` alongside telemetry frames, powered by `KnowledgeTelemetryMission`, so client tooling can enumerate generated templates each turn._
  - [x] Allow runtime toggling of agent generators (`enabled` / `per_faction`) via hot-reload or command surface for mid-campaign adjustments. (Owner: Systems Team — Ravi, Estimate: 1d; Deps: generator catalog config.) _Status_: New `update_espionage_generators` command adjusts generator flags in `EspionageCatalog`; the roster reseeds generated agents immediately so changes land mid-campaign._
  - [x] Surface `EspionageMissionState::queue_mission` on the command server for client scheduling. (Owner: Systems Team — Ravi, Estimate: 1.5d; Deps: command routing.) _Status_: `queue_espionage_mission` command now maps to `QueueMissionParams`, invoking `EspionageMissionState::queue_mission` with catalog/roster resources so clients can schedule missions mid-turn._
  - [x] Expand mission outcomes (partial successes, misinformation branches) and counter-intel sweeps that actively clear infiltrations. (Owner: Systems Team — Ravi, Estimate: 2d; Deps: mission outcome framework.) _Status_: Probe resolution now supports partial success/misinformation tiers via `espionage_config`, and successful counter-intel sweeps clear infiltration records while applying suspicion relief._
  - [x] Support hot-reloading/command tweaks for `QueueMissionParams` defaults via the espionage config pipeline. (Owner: Systems Team — Ravi, Estimate: 1d; Deps: balance config scaffolding.) _Status_: `espionage_config.json` now carries queue defaults, `queue_espionage_mission` consumes them automatically, and the new `update_espionage_queue_defaults` command lets operators adjust offsets/tiers at runtime._
- [x] Implement leak progression, espionage/counter-intel event handling, and metrics export in `knowledge_ledger_tick`, finishing snapshot serialization into the new schema tables (Owner: Systems Team — Ravi, Estimate: 1.5d; Deps: KnowledgeLedger infrastructure task). _Status_: Leak progression math, countermeasure decay, event ingestion, metrics export, and snapshot serialization are live across `core_sim/src/knowledge_ledger.rs` and related plumbing._
- [x] Externalize influencer system tuning into `core_sim/src/data/influencer_config.json`, wire config loading + optional hot-reload, and reflect the knobs across `shadow_scale_strategy_game_concept_technical_plan_v_0.md` (Influence chapter) and `docs/architecture.md` Influence systems notes (Owner: TBD, Estimate: 1.5d; Deps: config schema draft, documentation alignment). _Status_: Added `InfluencerBalanceConfig`, config handle, roster plumbing, and manual/architecture callouts; JSON lives at `core_sim/src/data/influencer_config.json`._
- [x] Move knowledge ledger timing/suspicion parameters to `core_sim/src/data/knowledge_ledger_config.json`, exposing runtime reload hooks and syncing values with Knowledge Systems coverage in both design documents (Owner: TBD, Estimate: 1.5d; Deps: influencer config task). _Status_: Introduced `KnowledgeLedgerConfig`/handle, rewired ledger math to config fields, and documented the workflow in both design docs._
- [x] Shift `SimulationConfig::default` and world bootstrapping constants into a data-driven `core_sim/src/data/simulation_config.json`, ensuring command surfaces preserve override behavior and updating manual §World Foundations plus architecture §Headless Core (Owner: TBD, Estimate: 2d; Deps: config schema alignment workshop). _Status_: Added JSON-backed loader (`SimulationConfig::from_json_str`), parsed sockets/Scalar fields, and documented tuning groups in both manuals._
- [x] Create phase-oriented configuration (e.g., `turn_pipeline_config.json`) for logistics, trade, population, and power clamps currently embedded in `systems.rs`, with accompanying documentation of designer-facing controls (Owner: TBD, Estimate: 2.5d; Deps: simulation config migration). _Status_: Added `TurnPipelineConfig` resource + JSON, rewired turn systems to consume data-driven clamps, wired reload/watch support, and documented knobs in manual §7c and architecture §Turn Pipeline Configuration._
- [x] Centralize culture tension and corruption severity scalars in `core_sim/src/data/culture_corruption_config.json`, extend telemetry expectations, and cross-reference diplomacy/corruption sections in both design docs (Owner: TBD, Estimate: 1.5d; Deps: phase configuration groundwork). _Status_: Added `CultureCorruptionConfig` + handle, rewired culture/corruption systems to consume JSON tunables, and documented clamps/telemetry alignment in both manual §7c and architecture §Corruption Simulation Backbone._
- [x] Expose snapshot overlay weighting constants via `core_sim/src/data/snapshot_overlays_config.json`, update inspector tooling expectations, and document the tunables under manual telemetry and architecture observer sections (Owner: TBD, Estimate: 1d; Deps: knowledge config load path). _Status_: Added `SnapshotOverlaysConfig` + handle/metadata, rewired snapshot generation to consume JSON constants, hooked `reload_config overlay` across server/CLI/inspector, and documented the knobs in manual §7c and architecture §Turn Pipeline Configuration._
- [x] Generate rich knowledge telemetry frames once event ingestion lands (Owner: Systems Team — Ravi, Estimate: 0.5d; Deps: espionage event hooks). _Status_: Frames emit the full timeline ring buffer via `knowledge.telemetry`, and the Godot inspector now ingests the payload for metrics/timeline display._
- [x] Extend the Godot thin client with the Knowledge Ledger panel and command surface described in `docs/architecture.md` §Knowledge Ledger & Leak Mechanics—subscribe to ledger/telemetry payloads, render overview/detail UI, and expose counter-intel/security posture controls (Owner: Client Team — Elena, Estimate: 2d; Deps: backend ledger payload & command endpoints). _Status_: Knowledge tab now streams missions/timeline data, includes a mission queue tester (auto agents, tier overrides, schedule offsets), and surfaces inline counter-intel policy/budget controls with live status synced from `shadow_scale::espionage` logs._
- [x] Scope crisis telemetry channels promised in manual §10 by inventorying required metrics, network payloads, and inspector overlays in `docs/architecture.md`, then derive execution tasks for simulation and client teams (Owner: TBD, Estimate: 1.5d; Deps: Godot inspector overlay bandwidth). _Status_: Implemented `TurnStage::Crisis` + `advance_crisis_system`, emitting real `CrisisTelemetryState`/`CrisisOverlayState` via `ActiveCrisisLedger` and `CrisisOverlayCache`; docs/manual now outline the live data flow and the Godot inspector consumes non-stub overlays._
- [ ] **Extend `LaborConfig::validate()` to the cultivation levers** (`FaunaConfig::validate()` **DONE** — landed with the corral-as-a-managed-population arc: it runs inside `from_json_str`, covers every load path, logs a rejected override at **error** level as `fauna_config.invalid_rejected`, falls back to the builtin, and enforces the pen's net-positive bound + the monotone husbandry ladder + the ordered phase bands in all three ecologies; the builtin-only assertions were ported into it with a rejection test per bound. `LaborConfig::validate()` now **exists** too — it runs inside `from_json_str` (every load path, the same `crisis_config.rs`/`FaunaConfig` convention: a broken invariant is logged at **error** level as `labor_config.invalid_rejected` and the builtin is used), but it covers **only** `forage.capacity_by_biome` — the total-over-every-`TerrainType` / finite / non-negative / not-all-zero table check, mirroring `validate_graze`.) — the **cultivation-lever invariants are still asserted only over the *builtin*, in `labor_config`'s own module tests**: `progress_per_turn > decay_per_turn`, `0 < cultivating_yield_fraction < 1`, `tended_provisions_per_biomass > 0`, `knowledge_progress_per_turn > 0`, and `0 < knowledge_completion_threshold <= 1`. So a `LABOR_CONFIG_PATH` override that breaks one (a `0` `tended_provisions_per_biomass`, `progress_per_turn <= decay_per_turn`, …) is still accepted silently and quietly changes or disables behaviour — the same hole the forage-table check (and `ExpeditionConfig::validate()`, PR #117) closed elsewhere. Port those builtin-only assertions into `LaborConfig::validate()`, extending the existing domain error variant, with a rejection test per bound. Bound the levers whose `0`/negative would silently disable a feature or divide by zero; leave genuinely free levers free, and say which in the doc. Then correct the "Intended invariants" note in `core_sim/CLAUDE.md`'s Cultivation config block (which still reads "`LaborConfig` has NO `validate()`") back to "Validated". (Owner: TBD, Estimate: 0.5d; Deps: none.)

### Great Discovery System
- [x] Outline Great Discovery subsystem architecture mirroring manual §5 by defining data structures, trigger flow, and snapshot payload contracts in `docs/architecture.md`, then break the work into implementation tickets after approval (Owner: TBD, Estimate: 2d; Deps: coordination with Knowledge Diffusion hooks). _Status_: Architecture captured in `docs/architecture.md` §Great Discovery System Plan aligning with manual §5; implementation tickets listed below.
- [x] Implement Great Discovery registries/resources (`GreatDiscoveryRegistry`, `GreatDiscoveryReadiness`, `GreatDiscoveryLedger`, `ObservationLedger`) and wire schedule placement after `trade_knowledge_diffusion` in `core_sim` (Owner: TBD, Estimate: 2d; Deps: architecture doc §Great Discovery System Plan → ECS Structure/Trigger Flow). _Status_: Resources/events live in `core_sim/src/great_discovery.rs`, scheduled via `TurnStage::GreatDiscovery` in `core_sim/src/lib.rs`; snapshots/telemetry updated and full test suite passes._
- [x] Build constellation evaluation systems (`collect_observation_signals`, `update_constellation_progress`, `screen_great_discovery_candidates`) with deterministic tests covering gating/freshness decay (Owner: TBD, Estimate: 2d; Deps: registries/resources task). _Status_: Implemented in `core_sim/src/great_discovery.rs` with unit tests validating observation gating and freshness decay; scheduled via `TurnStage::GreatDiscovery` in `core_sim/src/lib.rs`; `cargo test` passes._
- [x] Implement Great Discovery resolution pipeline (`resolve_great_discovery`, `propagate_diffusion_impacts`) plus effect hook routing into power/crisis/diplomacy subsystems (Owner: TBD, Estimate: 3d; Deps: constellation evaluation task, coordination with Knowledge Diffusion task). _Status_: Resolution now seeds effect events/state (`GreatDiscoveryEffectEvent`, `PowerDiscoveryEffects`, `PendingCrisisSeeds`, `DiplomacyLeverage::push_great_discovery`), forced-publication flags mark ledger entries in `propagate_diffusion_impacts`, and unit tests cover effect routing and freshness behavior; full test suite passes._
- [x] Extend snapshot contracts and FlatBuffers with `GreatDiscoveryState`, `GreatDiscoveryProgressState`, and integrate telemetry counts into `SimulationMetrics` (`sim_schema`, `core_sim`, `sim_runtime`) (Owner: TBD, Estimate: 1.5d; Deps: resolution pipeline for payload fields, schema review). _Status_: Schema + Rust bindings now include Great Discovery state/progress/telemetry, `core_sim::snapshot` diffs publish the data, `SimulationMetrics` exports aggregated counts, and regression test `great_discovery_snapshot_delta_tracks_changes` locks the snapshot/delta contract._
- [x] Author integration tests/benchmarks validating GDS turn budget, snapshot serialization, and leak timer adjustments handed to Knowledge Diffusion systems (Owner: TBD, Estimate: 1.5d; Deps: prior GDS implementation tasks, Knowledge Diffusion architecture). _Status_: Added `integration_tests/tests/great_discovery.rs` covering bulk constellation processing, snapshot/delta payloads, and forced-publication leak propagation into trade diffusion; suites run under `cargo test` to guard pipeline regressions._
- [x] Serve the Great Discovery definition catalog to thin clients via RPC/broadcast so the inspector no longer depends on local JSON reads (Owner: TBD, Estimate: 1d; Deps: `core_sim::great_discovery::load_catalog_from_str`, `docs/architecture.md` §Great Discovery Catalog). _Status_: `WorldSnapshot` now carries `great_discovery_definitions`, the headless server seeds the shared catalog on startup, and the Godot inspector consumes the streamed metadata so designers no longer rely on a local JSON file._

### Power Systems
- [x] Implement `PowerGridState`, `PowerTopology`, and associated ECS systems (`collect_generation_orders` through `export_power_metrics`) inside `core_sim`, ensuring deterministic scheduling and integration with materials/logistics/population phases (Owner: TBD, Estimate: 4d; Deps: `docs/architecture.md` §Power Systems Plan). _Status_: Power phase now models generation, routing, storage, instability, and exports telemetry through `PowerGridState`.
- [x] Extend `sim_schema`/`sim_runtime` with `PowerGridNode`, `PowerTelemetryState`, and headless diagnostics helpers consumed by automated tests (Owner: TBD, Estimate: 2d; Deps: core_sim ECS scaffolding). _Status_: Snapshot/delta payloads carry node telemetry plus aggregated metrics/incidents; tests cover telemetry conversion.
- [x] Enhance Godot thin client with a Power tab, overlay toggles, and incident feed consuming the new telemetry (Owner: TBD, Estimate: 3d; Deps: schema updates + snapshot payloads). _Status_: Inspector Power tab surfaces grid metrics, sortable node list, and node detail panel tied to streamed updates.
- [x] Add regression tests and benchmarks covering stability band transitions, cascade propagation, and serialization/delta output for power telemetry (Owner: TBD, Estimate: 2d; Deps: core_sim implementation + schema updates).

### Terrain Foundations
- [x] Implement `TerrainType` enum + tag metadata in worldgen and data contracts (Owner: TBD, Estimate: 2d; Deps: align with `docs/architecture.md` Terrain Type Taxonomy).
- [x] Expose terrain IDs/tag bitsets through snapshots and FlatBuffers for client overlays (Owner: TBD, Estimate: 1.5d; Deps: terrain enum integration).
- [x] Extend logistics/population systems to surface telemetry for terrain attrition effects (Owner: TBD, Estimate: 1d; Deps: terrain-aware simulation hooks).
- [x] Add client overlays (CLI & Godot) that visualise terrain classes and tags using the exported channel (Owner: TBD, Estimate: 1.5d; Deps: terrain telemetry stream).
- [x] Document palette alignment across `shadow_scale_strategy_game_concept_technical_plan_v_0.md` and `docs/architecture.md` (Owner: TBD, Estimate: 0.5d; Deps: verified Godot palette mapping).
- [x] Add Godot/CLI terrain legend surfaced from shared palette data (Owner: TBD, Estimate: 1d; Deps: palette documentation).

### Fluvial erosion in the heightfield
`heightfield::apply_fluvial_erosion` (shipped, `enabled: true` on both presets) runs the classic
landscape-evolution model minus uplift — `∂z/∂t = D∇²z − K·A^m·S^n` — at the end of
`build_elevation_field`, i.e. **before the land mask**, which is what makes it bite: the mask ranks
tiles by elevation, so the coastline is a level set of that field. Authoritative spec + config table
+ the full measured A/B: `core_sim/CLAUDE.md` → Rivers → "Fluvial erosion".

- [x] **Fluvial erosion pass on the elevation field.** Result, measured over 6 seeds with the river
      thresholds held fixed: **SPONGE fixed** — coastal tiles of the largest landmass 59.2% → **52.8%**,
      seed spread 14.3 → **9.6**, every seed improved. **The ~13-hex navigable ceiling is gone** —
      longest river **10 → 25 hexes** (it was never the threshold, it was the landscape). **CAPTURE is
      NOT fixed** — biggest-basin/landmass 11.0% → 13.3%, spread 39.5 → 34.1: seed 5 jumps 4.7% →
      21.0% and seeds 1/3 roughly double, but they are still single-digit while seed 4 runs at 38%.
- [x] Two things the pass had to learn, both now guarded by doc comments: base level is the mask's
      **rank contour**, not `sea_level` (only 24–37% of cells are above `sea_level` but the mask claims
      38% for land — freezing at `sea_level` froze the whole coastal band and measured as a no-op); and
      `restamp_elevation` **clamps a third of all land flat onto `sea_level`**, so the field is
      monotonically re-anchored (`anchor_contour_to_sea_level`) or the carved valleys never reach
      hydrology.

- [ ] **Capture: the divides, not the valleys.** Incision deepens the valleys a continent already has;
      it does not move its **divides**, which come from the continent-scale fbm in
      `build_elevation_field`. A noise-dome continent (seeds 1/3) sheds radially no matter how deep the
      valleys get. The next lever is therefore the **noise itself** — a tilted / warped / multi-ridge
      continent field (domain warping, or a large-scale tilt term), not more erosion. Measure with
      `hydrology_earthlike::drainage_census`'s CAPTURE column, which now A/Bs erosion on and off.
- [ ] *(Optional, cheap)* **Morphological open/close on the land mask** — a majority filter that fills
      1-hex nooks and deletes 1-hex specks. Erosion took the sponge from 59% → 53%; a compact blob is
      ~14%, so there is still headroom that a direct attack on crenellation could take.
- [x] **A navigable-chain mouth hex can be re-stamped to a dry biome, stranding an orphan
      `river_channel` bit.** ~~After `generate_hydrology` stamps a `NavigableRiver` chain, a later
      terrain pass (tag-solver / palette clamp) can overwrite the chain's **mouth** hex with a dry
      biome (e.g. `AlluvialPlain`) — even though `NavigableRiver` is a `must_have` — leaving a hex
      that carries a `river_channel` exit bit but renders as land.~~ **Fixed.** Root cause was
      `apply_tag_budget_solver`'s **Fertile-*add*** branch (primary filter + fallback loop): it
      restamped a hydrology-placed **`RiverDelta`** mouth cut through a **polar/non-fertile** biome to
      `AlluvialPlain`. Such a delta lacks the `Fertile` tag, so the branch's `Fertile`/`Water` skips
      didn't catch it, and — unlike every reduction branch and the water/wetland/coastal passes — the
      Fertile-add branch was the one path missing the `terrain != RiverDelta` guard. (`NavigableRiver`
      itself is `WATER`-tagged and was already excluded from this branch; the orphaned-channel symptom
      was the delta mouth, not the navigable chain.) Reproduced on earthlike seed
      12736602826901522706 at 104×64, hex @(50,17); the previously-cited seed 5226386361516556246 is
      also clean now. Guarded by `core_sim/tests/navigable_mouth_delta.rs` (invariant: no hex carries a
      `river_channel` bit on non-`NavigableRiver`/non-`RiverDelta` terrain, run through the real
      Startup chain via `build_headless_app` so a later-pass clobber can't hide).

### Trade Knowledge Diffusion
- [x] Introduce `TradeKnowledgeDiffusion` stage that consumes openness metrics to share discoveries between factions (Owner: Ravi, Estimate: 2d; Blocked by schema/runtime helpers).
- [x] Integrate migration-driven knowledge seeding into population movement systems (Owner: Elena, Estimate: 1.5d; Requires migration knowledge fragments in snapshots).
- [x] Implement corruption passes for logistics, trade, and military budgets (Owner: Ravi, Estimate: 3d; Requires `CorruptionLedger` resource from data contracts).

### Schema & Runtime
- [x] Extend `sim_schema/schemas/snapshot.fbs` and generated bindings with Knowledge Ledger tables (`KnowledgeLedgerState`, modifier breakdown children, espionage timeline) plus supporting enums, then update Rust builders and Godot bindings (Owner: Tooling Team — Mei, Estimate: 1d; Deps: KnowledgeLedger infrastructure task). _Status_: Schema/rust bindings now emit ledger entries, modifier breakdowns, espionage timeline, and knowledge metric payloads; FlatBuffers regenerated via `cargo build -p shadow_scale_flatbuffers`._
- [x] Surface the new knowledge metrics/log channels in `sim_runtime` (`SimulationMetrics`, `knowledge.telemetry`) and integrate helper views for ledger payloads (Owner: Tooling Team — Mei, Estimate: 0.5d; Deps: schema extension task). _Status_: sim_runtime now exposes knowledge ledger/timeline views, encode/decode helpers, telemetry parsing, and SimulationMetrics/WorldSnapshot wiring carries knowledge counters._

### Culture Trait Stack
- [x] Implement multi-layer culture storage (`CultureLayer`, `CultureTraitVector`) and the reconcile routine propagating global → regional → local weights (Owner: Elena, Estimate: 3d; Deps: finalize trait list per game manual §7c).
- [x] Emit divergence telemetry (`CultureDivergence`, `CultureTensionEvent`, `CultureSchismEvent`) and wire into sentiment/diplomacy hooks (Owner: Ravi, Estimate: 2.5d; Deps: reconcile routine + event bus triggers).
- [x] Derive trait-driven system modifiers (`CultureEffectsCache`) and expose `CultureLayerState` snapshots/CLI overlays (Owner: Jun, Estimate: 2d; Deps: schema updates in `sim_schema`, inspector UI bandwidth).
- [x] Introduce influencer culture resonance channels and serialize weights in snapshot payloads (Owner: Mira, Estimate: 2d; Deps: influencer roster refactor).
- [x] Apply influencer culture deltas during `reconcile_culture_layers`, blending with policy modifiers (Owner: Elena, Estimate: 1.5d; Deps: resonance channels).
- [x] Extend inspector UI to display per-influencer culture resonance and recent trait pushes (Owner: Jun, Estimate: 1d; Deps: snapshot serialization + Godot culture tab groundwork).
- [x] Implement the culture reconcile pipeline end-to-end (propagation, divergence timers, culture effect cache refresh) and ensure `CultureTensionEvent` / `CultureSchismEvent` handling matches `docs/architecture.md` §Culture Simulation Spine and manual §7c expectations (Owner: TBD, Estimate: 3d; Deps: existing `CultureLayer` scaffolding). _Status_: Culture reconcile now respects config-driven elasticity/trigger windows, emits structured tension/schism events with diagnostics, refreshes effect cache each turn, and carries unit coverage for drift, schism, and resolution paths._
- [x] Extend culture telemetry and inspector surfacing: add `CultureLayerState` / `CultureTensionState` payloads to snapshots, regenerate FlatBuffers, and update the Godot Cultural Inspector overlay with divergence heatmaps and clash forecasts per `docs/architecture.md` §Culture Simulation Spine and manual §7c UI notes (Owner: TBD, Estimate: 2d; Deps: previous task, FlatBuffers regeneration workflow). _Status_: Snapshot pipeline exports culture layers/tensions into FlatBuffers, native bindings/Godot inspector render divergence lists, detail views, and tension logs, and overlay stats/legends mirror the streamed telemetry._
- [x] Culture telemetry & inspector rollout checklist: document schema diffs (`sim_schema/snapshot.fbs`), regeneration steps (`cargo build -p shadow_scale_flatbuffers`, native autogen), Rust serialization touchpoints (`core_sim/src/snapshot.rs`, `core_sim/src/culture.rs`), Godot wiring (native bindings, `Inspector.gd` overlays), and QA steps (snapshot diff capture, Godot smoke run). _Status_: Checklist captured in `docs/architecture.md` §Culture Simulation Spine so future payload/UI updates follow the same sequence._

- [x] World setup: generate province/territory partitions and assign locals to distinct `RegionalCultureId` parents so regional divergence footprints narrow to their actual geography (Owner: TBD, Estimate: 3d; Deps: worldgen province data, `docs/architecture.md` §Culture Simulation Spine alignment).

## Fog of War / Visibility System
- [x] Implement `VisibilityLedger` and `FactionVisibilityMap` resources with three-state tracking (Unexplored, Discovered, Active) (Owner: TBD, Estimate: 2d). _Status_: Core visibility data structures landed in `core_sim/src/visibility.rs`; per-faction maps track tile states and last-seen turn.
- [x] Add visibility calculation systems for units and settlements with range-based reveal (Owner: TBD, Estimate: 2d; Deps: visibility resources). _Status_: `TurnStage::Visibility` runs `clear_active_visibility` → `calculate_visibility` → `apply_visibility_decay` in `core_sim/src/visibility_systems.rs`.
- [x] Implement elevation-based sight bonuses and line-of-sight blocking via Bresenham ray-cast (Owner: TBD, Estimate: 1.5d; Deps: heightfield access). _Status_: `calculate_visibility` applies configurable elevation bonuses; `has_line_of_sight` checks for terrain occlusion.
- [x] Add terrain modifiers for visibility range (water bonus, forest/wetland penalty) (Owner: TBD, Estimate: 1d; Deps: TerrainTags). _Status_: `reveal_tiles_in_range` applies per-tile terrain modifiers from `visibility_config.json`.
- [x] Export visibility raster in snapshots for client rendering (Owner: TBD, Estimate: 1d; Deps: snapshot pipeline). _Status_: `visibility_raster_from_ledger` emits normalized values; Godot client renders FoW overlay with 'F' toggle.
- [x] Implement visibility decay (Discovered → Unexplored after threshold turns) (Owner: TBD, Estimate: 0.5d). _Status_: `apply_visibility_decay` runs each turn, configurable via `decay.threshold_turns`.
- [x] Add `visibility_config.json` with sight ranges, decay, elevation, and terrain modifier settings (Owner: TBD, Estimate: 0.5d). _Status_: Config loaded via `VisibilityConfigHandle`; supports per-unit-type sight ranges.
- [x] Client-side FoW rendering: hide entity icons on non-visible tiles, FoW-aware tooltips (Owner: TBD, Estimate: 1d; Deps: visibility raster). _Status_: Godot `MapView.gd` filters food/herd icons and tooltips based on visibility state.
- [ ] Integrate trade routes with FoW (active trade routes grant visibility along path) (Owner: TBD, Estimate: 1.5d; Deps: TradeLink components). Description: See `docs/plan_trade_fow_integration.md` for implementation plan.

## Data Contracts (`sim_schema` + `sim_runtime`)
- [x] Define FlatBuffers schema for snapshots and deltas.
- [x] Implement hash calculation for determinism validation.
- [x] Provide serde-compatible adapters for early testing.
- [x] Extend trade link schema with openness/knowledge diffusion fields and migration knowledge summary payloads (Owner: Devi, Estimate: 1.5d; Deps: coordinate with `core_sim` turn pipeline + population serialization).
- [x] Add `CorruptionLedger` structs and subsystem hooks to snapshots (Owner: Devi, Estimate: 2d; Deps: align with logistics/trade/military component schemas).
- [ ] **Collapse the duplicate band-ceiling wire representation.** `HerdTelemetryState` carries the
  per-policy band hunt ceiling **twice**: the flat scalars (`ceilingSustain`/`ceilingSurplus`/
  `ceilingMarket`/`ceilingEradicate`/`ceilingCorral` + `corralYield`/`perWorkerYield`) and the
  `huntPolicyCeilings:[{policy, provisionsPerTurn}]` list. They are the **same numbers** — the list is a
  projection of the herd's `fauna::hunt_forecast`, the same object the scalars export, so they cannot
  drift. **The list should win:** a free-form `policy` string means a new policy needs no schema change,
  matching the convention already used for `species` (and `huntTripEstimates`). Retiring the scalars is
  a schema change **plus** a client refactor (the existing UI reads them), so it was deliberately
  deferred out of PR #117. Scope: drop the scalar fields from `snapshot.fbs`/`sim_schema`/`snapshot.rs`,
  repoint `Hud.gd`'s ceiling lookup at the list, keep `SourceYieldForecast` as the single sim-side
  source.

## Godot Inspector Pivot
- [x] Extend Godot snapshot decoder to expose influencer, corruption, sentiment, and demographic data currently consumed by the CLI (Owner: TBD, Estimate: 1.5d; Deps: FlatBuffers topics stable).
- [x] Implement Godot inspector shell (tabbed/collapsible panels) with Sentiment, Terrain, Influencers, Corruption, Logs, and Command Console sections (Owner: TBD, Estimate: 3d; Deps: decoder extensions).
- [x] Add Godot-side controls for turn stepping, autoplay, rollback, axis bias adjustments, influencer support/suppress/channel boost, spawn, corruption injection, and heat debug (Owner: TBD, Estimate: 2d; Deps: command bridge). _Status_: Commands tab now issues all debug actions through the Godot client.
- [x] Pipe sim logs/tracing output into Godot inspector and surface recent tick sparkline/summary (Owner: TBD, Estimate: 1d; Deps: inspector shell). _Status_: tracing log stream now feeds the Logs tab (structured scrollback + command echoes) and plots recent turn durations via sparkline.
- [x] Add terrain drill-down UI (per-biome detail view, tile inspection, future culture/military overlays) building on the new summary panel (Owner: TBD, Estimate: 2d; Deps: terrain tab groundwork). _Status_: Godot Terrain tab now offers biome selection with tag breakdowns, hover/click tile telemetry, and the Culture/Military overlay panels stream live divergence/readiness rasters with matching map legends.
- [x] Deprecate CLI inspector: document migration, update workflows, remove `cli_inspector` crate once parity achieved (Owner: TBD, Estimate: 0.5d; Deps: Godot inspector feature parity). _Status_: CLI crate removed, docs/workflows now point exclusively to the Godot thin client.
- [x] Support map zooming/panning via both mouse and keyboard inputs in the Godot inspector (Owner: TBD, Estimate: 1d; Deps: confirm MapView input bindings).
- [x] Introduce a shared typography theme for the Godot inspector that resolves `INSPECTOR_FONT_SIZE`, defines derived scale constants, and applies the theme to all static and runtime-created controls (Owner: TBD, Estimate: 1.5d; Deps: docs/architecture.md Inspector Typography Refactor Plan).
- [x] Rework HUD/inspector layout math to consume the shared typography metrics so panel placement adapts to base font changes without manual offsets (Owner: TBD, Estimate: 1d; Deps: shared typography theme scaffolding).
- [x] Add a Great Discoveries inspector tab rendering resolved ledger entries, telemetry summaries, and player-facing copy (Owner: TBD, Estimate: 1d; Deps: Great Discovery snapshot contracts). _Status_: Godot inspector now includes a Great Discoveries tab summarizing telemetry, listing resolved entries with detail panes, and wiring snapshot stream fields through the native decoder._
- [x] Surface faction-specific Great Discovery progress (constellation readiness, observation gates, ETA) in the Knowledge tab with filtering/summary affordances (Owner: TBD, Estimate: 1d; Deps: Great Discovery snapshot contracts). _Status_: The new tab also renders faction-specific constellation readiness with observation deficits, ETA, and posture details driven by snapshot/delta updates._
- [x] Visualise selected culture layers on the map: from the Culture tab, highlight the chosen regional/local footprint, dim non-selected tiles, and align overlay stats with the selection so divergence hotspots match the list (Owner: TBD, Estimate: 2d; Deps: culture overlay rendering hooks, selection broadcast). _Status_: Inspector selection now persists across refreshes, passes highlighted layer ids to the map, dims non-selected tiles, and reuses the selection set for overlay stats/legend context so divergence hotspots match the list._
- [x] Hook espionage mission control into the Godot inspector Knowledge tab: surface mission queues, success odds, misinformation flags, and counter-intel posture/budget controls using the existing command envelopes (`queue_espionage_mission`, `counterintel_policy`, `counterintel_budget`) per `docs/architecture.md` Knowledge Ledger follow-up notes (Owner: TBD, Estimate: 2d; Deps: command bridge, Knowledge telemetry bindings). _Status_: Knowledge inspector now renders mission telemetry/queue data, including success odds, MISINFO tags, and live counter-intel posture/budget feedback (`clients/godot_thin_client/src/Main.tscn`, `clients/godot_thin_client/src/scripts/Inspector.gd`). Documentation updated (`docs/architecture.md`, `shadow_scale_strategy_game_concept_technical_plan_v_0.md`) to match the new UI flow.

### Sentiment Sphere Enhancements
- [x] Implement quadrant heatmap widget with vector overlay and legend (Owner: Mira, Estimate: 2d).
- [x] Surface axis driver diagnostics listing top contributors per tick (Owner: Ravi, Estimate: 1.5d).
- [x] Integrate demographic snapshot panel tied to population cohorts/workforce (Owner: Elena, Estimate: 2d).
- [x] Extend event log to annotate sentiment-shifting actions with axis deltas (Owner: Omar, Estimate: 1d).
- [x] Wire axis bias editing and playback controls into command palette (Owner: Jun, Estimate: 1.5d).

### Influential Individuals System
- [x] Extend `sim_schema`/`sim_runtime` with `InfluentialIndividualState` and helper APIs (Owner: Mira, Estimate: 1.5d).
- [x] Implement `InfluentialRoster` resource and tick systems that spawn/grow influencers in `core_sim` (Owner: Ravi, Estimate: 2d).
- [x] Couple influencer outputs into sentiment axis deltas and other subsystems (Owner: Elena, Estimate: 2d).
- [x] Expose support/suppress commands and broadcast roster deltas via snapshot stream (Owner: Omar, Estimate: 1.5d).
- [x] Add legacy CLI inspector panel summarizing active influencers and their impacts (Owner: Jun, Estimate: 1.5d).
- [x] Introduce scope-tiered influencer lifecycle (Local → Regional → Global) with staged promotion thresholds and persistent dormant state; include tooling hooks for deterministic testing (Owner: Ravi, Estimate: 3d).
- [x] Add multi-channel support model (popular sentiment, peer prestige, institutional backing, humanitarian capital) and domain-weighted coherence/ notoriety gains (Owner: Elena, Estimate: 3d).
- [x] Extend `support`/`suppress` command surface to manipulate both coherence and notoriety, plus scoped commands for channel-specific boosts; update inspector tooling (legacy CLI at the time) with lifecycle badges, channel breakdown, notoriety display, and filter controls (Owner: Jun, Estimate: 2d).
- [x] Update documentation: lifecycle & support changes in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` and implementation details + testing guidance in `docs/architecture.md` (Owner: Mira, Estimate: 1d).
- [x] Replace sentiment sphere prototype drivers with real policy/event inputs: capture policy levers, incident deltas, and influencer channel outputs; expose telemetry hooks for testing (Owner: Elena, Estimate: 3d).

## Frontend Client Evaluation
- [x] Run Godot 4 thin client spike focused on tactical map rendering, overlays, and command round-trip metrics (Owner: Mira, Estimate: 3d; Output: `clients/godot_thin_client`, notes in `docs/godot_thin_client_spike.md`).
- [x] Draft shared scripting capability model (API surface, sandbox permissions) aligned with the Godot thin client reference (Owner: Leo, Estimate: 2d; Deps: finalize snapshot topic catalog). _Status_: Documented in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` and `docs/architecture.md`.
- [x] Capture Godot spike findings in a client evaluation memo, including go/no-go recommendation and follow-up needs (Owner: Omar, Estimate: 1d; Deps: completion of Godot spike).
- [x] (Conditional) Run Unity thin client spike if Godot outcome signals gaps that require comparison (Owner: Jun, Estimate: 3d; Deps: decision from evaluation memo).
- [x] Build lightweight snapshot proxy that converts binary `bincode` frames to JSON for tooling (Owner: Sam, Estimate: 1d; Deps: settle on schema exposure).
- [x] Retire JSON snapshot proxy and stream FlatBuffers snapshots directly (Owner: Sam, Estimate: 1d; Deps: Godot decoding path).
- [x] Integrate FlatBuffers stream into Godot client (Rust GDExtension or native parser) and retire JSON proxy once stable (Owner: Mira, Estimate: 4d; Deps: FlatBuffers schema stabilized).
- [x] Export dedicated logistics/sentiment rasters from `core_sim` snapshots (Owner: Devi, Estimate: 2d; Deps: align `SnapshotHistory` ring buffer + schema update). _Status_: `core_sim` now emits logistics and sentiment rasters; `SnapshotHistory` persists them and FlatBuffers/Godot clients consume the new channels.
- [x] Extend `shadow_scale_flatbuffers`/Godot extension to surface multi-layer overlays (logistics, sentiment, corruption, fog) with toggleable channels (Owner: Mira, Estimate: 2d; Deps: raster export task). _Status_: Tiles now carry real corruption pressure and fog-of-knowledge rasters with inspector tooltips describing the scale.
- [x] Validate Godot overlay rendering against CLI inspector metrics (add debug telemetry + colour ramp checks) before enabling designers (Owner: Omar, Estimate: 1d; Deps: overlay channel support). _Status_: Godot inspector now exposes normalized/raw overlay rasters with legend stats, replacing the retired CLI parity workflow.
- [x] Ship culture & military overlays end-to-end: emit rasters from `core_sim`, extend FlatBuffers contracts, render in Godot inspector tabs, and update docs/manual for player guidance (Owner: TBD, Estimate: 4d; Deps: Culture Simulation Spine telemetry). _Status_: Added `culture`/`military` rasters, updated FlatBuffers + Godot client, and refreshed manual/architecture docs to describe the new overlays.
- [x] Replace Logs tab delta counters with streamed tracing output (socket subscription, filtering, UI affordances) so designers can audit events without tailing the terminal (Owner: TBD, Estimate: 2d; Deps: tracing feed exposure in backend). _Status_: Godot Logs tab now attaches to the tracing socket, adds level/target/text filters plus clear/search controls, and documents the workflow in `docs/architecture.md` + `shadow_scale_strategy_game_concept_technical_plan_v_0.md`.
- [x] Implement crisis telemetry exporters in `core_sim`: add EMA-backed gauge metrics, trend deltas, stall detection, and warn/critical threshold handling aligned with `docs/architecture.md` §Crisis Telemetry Scope (Owner: Systems Team — Ravi, Estimate: 3d; Deps: crisis resolution loop finalized). _Status_: Crisis telemetry resource + exporter wiring landed in `core_sim` (EMA/trend/stale tracking available to downstream tooling).
- [x] Extend snapshot/log schemas for crisis telemetry (`CrisisTelemetryState`, `CrisisOverlayState`, `crisis.telemetry`/`crisis.alerts` channels) and wire serialization in `sim_schema`/`sim_runtime` (Owner: Systems Team — Devi, Estimate: 2d; Deps: crisis telemetry exporters). _Status_: Crisis telemetry now serializes via `CrisisTelemetryState`/`CrisisOverlayState`; log stream emits `crisis.telemetry` + `crisis.alerts` frames with EMA/trend data._
- [x] Build Godot inspector Crisis panels (Dashboard gauges, alert inbox, modifier tray wiring, overlay toggles, accessibility toggle) using new telemetry feeds (Owner: Client Team — Jun, Estimate: 3d; Deps: crisis telemetry schemas & channels). _Status_: Crisis overlay channel now feeds colored rasters + annotations into `MapView.gd`, inspector renders telemetry gauges/alerts in `Inspector.gd`, and native bindings serialize annotations/history via `clients/godot_thin_client/native/src/lib.rs`; manual verification complete in Godot.
- [x] Stand up crisis configuration loaders (`crisis_archetypes.json`, `crisis_modifiers.json`, `crisis_telemetry_config.json`) with env override + hot-reload plumbing (Owner: Systems Team — Ravi, Estimate: 3d; Deps: architecture plan §Crisis System Architecture). _Status_: `crisis_config` module now loads bundled JSON catalogs, supports `CRISIS_*_PATH` overrides, and the server exposes `reload_config crisis_archetypes|crisis_modifiers|crisis_telemetry` hot-reload hooks with file watchers and `shadow_scale::config` logging.
- [x] Author crisis archetype/modifier catalog JSON and seed baseline Plague/Replicator/AI Sovereign entries consistent with manual §§9–10 (Owner: Design Systems — Mira, Estimate: 2d; Deps: loader scaffolding). _Status_: Catalog now ships `plague_bloom`, `replicator_swarm`, and `ai_sovereign` archetypes plus aligned modifiers (`grid_segmented`, `counter_ai_standing`, `shutdown_protocols`, etc.); manual §9b/§10 and docs/architecture.md are cross-linked to the live data.
- [x] Extend command surface and tooling (`CommandEnvelope`, `cargo xtask command`) to consume catalog identifiers and drive crisis stage workflows (Owner: Systems Team — Leo, Estimate: 2d; Deps: config loader + archetype data). _Status_: Completed — generic `cargo xtask command` helper now issues protobuf envelopes and replaces the spawn-crisis stub; architecture docs/README updated._

## Shared Scripting Capability Model
- [x] Implement QuickJS GDNative module and runtime bootstrap inside `clients/godot_thin_client` (`ScriptHost` worker threads, capability token plumbing, manifest loading) (Owner: Mira, Estimate: 3d; Deps: manifest schema in `docs/architecture.md`). _Status_: QuickJS runtime migrated to the new `quick-js` bindings, manifest/session plumbing verified, and threads spawn/tear down cleanly after `cargo check`.
- [x] Wire script capability enforcement to Godot bridges (telemetry subscriptions, `CommandBridge` dispatch, session storage serialization) and add per-frame watchdog handling (Owner: Leo, Estimate: 2.5d; Deps: QuickJS runtime integration). _Status_: Telemetry, command, session, and alert capabilities verified in Godot; runtime watchdog/tick metrics confirmed under QuickJS.
- [x] Expose `CapabilitySpec` registry from `sim_runtime` and ship manifest lint/tests ensuring topic/command IDs stay in sync (Owner: Sam, Estimate: 1.5d; Deps: finalized capability list). _Status_: `sim_runtime::scripting` now publishes the capability registry and manifest parsing enforces coverage for telemetry subscriptions with unit tests.
- [x] Build Script Manager UI panel in Godot (list manifests, capability review, enable/disable, error surfaces) and integrate `console`/alert channels into the Logs tab (Owner: Jun, Estimate: 2d; Deps: runtime bootstrap + logging bridge). _Status_: Scripts tab loads packages from both roots, enable/disable wiring works, and log/alert signals flow into Inspector.
- [x] Deliver `tools/script_harness` headless runner with mock feeds, fuzz hooks, and CI budget assertions for sandbox violations (Owner: Omar, Estimate: 2d; Deps: capability registry & host bindings). _Status_: Harness builds against the native runtime and exposes tick/event CLI hooks; next step is adding scripted smoke tests.
- [x] Implement save/load serialization for active scripts and `storage.session` payloads via new `SimScriptState` struct and add regression coverage (Owner: Devi, Estimate: 1.5d; Deps: `sim_runtime` capability registry). _Status_: `SimScriptState`/`ScriptManifestRef` capture enabled script metadata; Godot host exposes `capture_state`/`restore_state`, and runtime applies session + subscription restores with validation.
- [x] Formalize the scripting manifest contract: publish JSON schema, add lint/validation tooling against `CapabilitySpec`, document host runtime checks, and sync manual/architecture references (Owner: TBD, Estimate: 2d; Deps: capability registry finalized). _Status_: Schema emitted to `docs/scripting_manifest.schema.json`, `cargo xtask validate-manifests` enforces shape + capability coverage, and docs/manual sections reference the contract + runtime checks.
- [ ] Decide and document the distribution model for inspector scripts/mods (signed bundles vs Workshop-style feeds), outline load/unload flows, and sync the plan between `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §Next Steps (Frontend) and `docs/architecture.md` Shared Scripting Capability Model (Owner: TBD, Estimate: 2d; Deps: stakeholder interviews, packaging spike).

### Roaming Bands — the missing half of settle/don't-settle (FUTURE ARC, design-doc first)

**The gap, in one line: we have built the *rooted* column carefully and left the *roaming* column without a
payoff.** Scarcity ([[scarcity-drives-the-real-decision]]), Fields on 46 river tiles, pens fixed at your
fence, the sedentarization pull — everything pushes one way. Staying nomadic currently reads as *a phase
you haven't grown out of*, not a strategy you chose. The design pillar says move/stay/fork must all be
live options with real advantages; today only "stay" has any.

**Four strategies, two of them under-built:**

| | rooted | roaming |
|---|---|---|
| **plants** | **Field** — you cannot carry a farm | *(nothing — plants don't travel)* |
| **animals** | **Pen** — fixed at your fence | **pastoral: your herd follows you** (`drift_to_owner`, 3b) · **big game: you follow the herd** |

**The seed of the payoff — megafauna is food too big to carry.** A mammoth is ~2,000–2,500 kg of usable
meat ≈ **60–100 person-loads**; nobody hauled one home, they *moved to the kill*, butchered, dried, cached,
and moved on. So:
- On an **expedition**, a big animal is mostly waste (a 3-hunter party keeps ~5% of a mammoth) — *"it may
  just not be worth killing a mammoth on an expedition"*, which is the game telling the truth, not a bug.
- **Move the band to it** and the carry cost vanishes — you're standing on the carcass. The whole beast is
  yours.
That is a food source that **rewards mobility and punishes rootedness** — the counterweight the rooted
column has been missing, and historically exactly what happened.

**Pieces that already exist** (this arc is mostly *connecting* them, not inventing): `drift_to_owner` (a
tamed herd travels with the band — 3b); migratory herds + loiter ranges; Seasonal Camps + trail knowledge;
the Nomadic Default start profile; `SedentarizationScore` (currently only models the pull *toward* rooting
— it has no opposite).

**Open questions for the design doc:** what does a roaming band *accumulate* if not buildings? (trail
knowledge, herd relationships, cached stores?) How does storage/spoilage interact — a lump you can't
preserve rots, so does drying/caching become the nomad's granary? Does `SedentarizationScore` need an
opposing "mobility" reading rather than being a one-way meter? What does forking a band to seed a second
group cost and gain? Does pastoral nomadism (herd follows you) and hunting nomadism (you follow the herd)
want different support?

**Depends on:** slice 8 (whole animals — megafauna only becomes a reason to move once a carcass is
indivisible and huge); the local-hunt carry distinction below. **Manual-first** when opened.
(Owner: TBD, Estimate: design-doc then phased; Deps: slice 8.)

## Core Simulation — Bugs

- [ ] **⚠ Rollback/load may permanently destroy tended patches, Fields, and pens.** Strongly evidenced,
  **not yet proven end-to-end** — verify through a real snapshot round-trip before fixing.
  **The mechanism:** `tended_this_turn` (and the pen's `corralled_tended_this_turn`) are **transient**,
  and the restore path seeds them `false` (`forage.rs:377`, `fauna.rs:850`). The maintenance writer that
  spares a source from decay is gated on `is_managed()` (`labor.rs:233`). So on the first Logistics pass
  after a restore:
  - **Tended patch / Field** — decays one tick → `is_cultivated()` flips false → the `is_managed()` gate
    **never re-fires** → the improvement is lost *even with a band working it every turn*, bleeding to 0
    over ~100 turns.
  - **Pen** — worse: `advance_husbandry` doesn't decay an untended pen, it **escapes** it outright
    (`corralled_at = None`, `pen_radius` zeroed) — the full ~25-turn rebuild *plus* every ExtendPen ring.
  **Pre-existing** — the pre-slice-7 managed branch used the identical `is_managed()` predicate; the
  intensification arc only surfaced it (a probe accidentally constructed the rollback state and
  "proved" a bug that turns out to be real only *after* a restore).
  **Minimal fix:** have the restore path seed the flag `true` — a one-turn grace, exactly the precedent
  `corral_at` already sets (`fauna.rs:502`). **Add a regression test that survives a real
  capture→restore→advance cycle**, since that's the only thing that would have caught this.
  (Owner: TBD, Estimate: 0.5d; Deps: none.)

- [ ] **A local hunt should not pay the carry-home cost.** `hunt_take` applies the same
  `per_worker_biomass_capacity` cap whether the band is **standing on the herd** (within its hunt reach)
  or a detached party is hauling the kill back N tiles. Those are different acts: **hunt = reach + carry;
  harvest-at-home = no haul, you're already there.** This is what makes megafauna coherent — a 3-hunter
  expedition keeps ~5% of a mammoth (correct: don't hunt mammoths from a distance), but a band that
  *moves to the carcass* should keep the beast. Without it, "go to the mammoth" is merely the absence of a
  punishment rather than a reward, and the Roaming Bands arc above has nothing to stand on. Small change,
  large consequence. Next slice after slice 8. (Owner: TBD, Estimate: 0.5d; Deps: slice 8.)
- [ ] **~~Hunting expeditions never say a hunter is idle~~ — LARGELY SUPERSEDED by slice 8.** The ticket
  below proposed *explaining the smooth model better*; the user's read was that **the smooth model was the
  bug**, and slice 8 replaced it (whole animals + real escapement). Extra hunters now genuinely change the
  take. **What remains is the UI half**: surfacing why a hunter is idle, and that Surplus is the answer to
  "more food, faster — at the herd's expense". Original analysis kept for the seam map:
  Playtest: 1 worker → 6 turns/4 food, 2 → 11/8, 3 → 16/12. Adding hunters didn't hunt faster,
  it just made the trip longer. **Not a bug at the time — the model was self-consistent and the UI silent.**
  `expedition_take_biomass` (`expeditions.rs:650-666`) is `min(workers × per_worker_biomass_capacity,
  policy_ceiling)`, so throughput *does* scale with the party; it just wasn't binding:
  - **Sustain** → ceiling = the herd's MSY (`hunt_policy_ceiling`, a *herd* property). **This is close to
    systematic, not a one-off:** one hunter's throughput is **40 biomass/turn**
    (`hunt.per_worker_biomass_capacity`), and most of the roster's `MSY = r·K/4` sits well under it —
    Rabbit ~16, Boar ~25, Aurochs ~29, Red Deer ~30. So on Sustain **hunter #2 is idle for nearly every
    herd in the game**, and only grows the pack (`workers × hunt.per_worker_carry` 4.0) → a longer trip at
    the same rate. The playtest herd was **Steppe Runners, K 3890, r 0.04 → MSY 38.9** (≈0.78 prov/turn,
    matching the observed 0.8). Extra hunters only earn their keep where MSY > 40: megafauna, or a
    migratory herd on rich range (Steppe Runners at the species-median K≈9000 → MSY ~90 → 3 hunters).
    Since 2b made **K ecological**, a herd's *pasture* decides how many hunters it can support — arguably
    good design, but wholly invisible. **Worth deciding separately:** whether
    `hunt.per_worker_biomass_capacity` 40 is simply too high relative to the roster's MSY, which would
    make the party slider meaningful on Sustain rather than only on Surplus.
  - **Surplus/Market** → the expedition ceiling is **stock headroom to the Allee floor** (`0.15·K`, via
    `hunt_expedition_floor:621-637`) — **not** the resident band's `MSY × surplus_multiplier`; the two
    paths deliberately disagree (documented `expeditions.rs:568-572`). So the *hunters* bind: rate scales
    linearly and **trip length stays flat** (bag and rate both ×N) until the stock is stripped, then the
    party crawls on the regrowth trickle. (That regime change is why the forecast simulates instead of
    dividing — see the 4-workers-on-a-rabbit-warren note at `expeditions.rs:879-891`.)
  So the same slider is *dead weight* on one policy and *the main lever* on another, and nothing says so.
  **Scope:** there is **no max-useful-workers concept in the expedition path at all** —
  `workers_needed_for_take` (`expeditions.rs:552-560`) exists but every caller is in-place labor
  (`labor.rs`, `fauna::forecast_source_yield`); `HuntTripForecast` carries only `turns_to_fill` /
  `delivers_food` / `first_turn_provisions`. Add it as a field on `HuntTripForecast`, populated in
  `simulate_hunt_trip` from the `expedition_take_biomass` result the sim already computes, exported
  beside `turns_to_fill` in `HuntTripEstimateState` (the outfit UI is a pure lookup by design — see
  `native/src/lib.rs:4117`, "Band = flow arithmetic; expedition = lookup"). Then surface *why* a hunter
  is idle, and that Surplus is the answer to "more food, faster — at the herd's expense".
  (Owner: TBD, Estimate: 1d; Deps: none.)

## Tooling & Tests
- [x] Add determinism regression test comparing dual runs.
- [x] Introduce benchmark harness for 10k/50k/100k entities.
- [x] Integrate tracing/tracing-subscriber metrics dump accessible via CLI.
- [x] Add regression coverage ensuring `TerrainOverlayState` updates propagate on biome/tag changes (Owner: TBD, Estimate: 1d; Deps: finalized terrain legend work). _Status_: Exercised by `snapshot::tests::terrain_overlay_delta_updates_on_biome_change` covering biome/tag mutation delta emission.
- [ ] **Config Tuning panel (inspector) — collapse the playtest turnaround loop.** Every balance
  question this project asks ("does 0.16 consumption feel right?", "is 125 turns to tame a Steppe
  Runner too long?", "should `pen_gain` be higher?") is answered by *editing a JSON file, rebuilding,
  and replaying from turn 0*. That edit→rebuild→replay cycle is now the slowest part of design
  iteration, and the dial count keeps growing (`demographics_config`, `fauna_config`, `labor_config`,
  `intensification_ladder`, `graze`, `map_presets`, …). Build a Godot inspector panel that **lists the
  tunable parameters, lets you edit them, and restarts the sim with the overrides** — no rebuild, no
  hand-editing JSON.
  - **Restart-scoped is enough** (the ask): tune → restart → observe. Live hot-reload is a non-goal;
    most of these dials only make sense applied from turn 0, and several configs deliberately are not
    on the reload path.
  - **Ride the existing seam, don't invent one:** each config already loads via `from_json_str` with a
    `*_CONFIG_PATH` / `*_PATH` env override and its own `validate()` that logs
    `<config>.invalid_rejected` and falls back to the builtin. The panel should write an override file
    (or push overrides to the server) and use that same path, so **validation is enforced for free and
    a bad dial can't silently corrupt a run** — surface the rejection in the panel rather than failing
    quietly.
  - **Discoverability is the real design question:** the configs are hand-written JSON with rich
    `_comment_*` documentation (deliberately — see the no-magic-numbers rule). A panel that drops those
    comments loses the *why* behind each dial. Prefer surfacing the comment as help text over inventing
    a parallel schema; consider whether the server should serve a config catalog (name, current value,
    bounds from `validate()`, comment) the way `great_discovery_definitions` is streamed, so the client
    doesn't re-read local JSON.
  - Scope to decide when picked up: which configs are in (start with the balance-heavy ones —
    `demographics_config`, `fauna_config`, `intensification_ladder`, `labor_config`), whether values
    persist to disk or live only for the session, and whether a "reset to builtin" is per-dial or
    per-config. (Owner: TBD, Estimate: 2–3d; Deps: none — every config seam it needs already exists.)

## Core Simulation Roadmap
- [x] Implement per-faction order submission and turn resolution phases (Owner: Sam, Estimate: 4d).
- [x] Persist snapshot history for replays and rollbacks (Owner: Devi, Estimate: 3d).
- [x] Define `CommandEnvelope` protobuf schema and expose encode/decode helpers in `sim_runtime` (Owner: Leo, Estimate: 0.5d; Deps: docs updated for protobuf migration).
- [x] Add dual-stack command server (Protobuf + legacy text) and update Godot/QuickJS host wrappers to issue structured commands (Owner: Leo, Estimate: 1.0d; Deps: `CommandEnvelope` helpers).
- [x] Retire legacy text parser, cut a migration guide for clients, and enable protobuf-only mode once tooling validates (Owner: Leo, Estimate: 0.5d; Deps: dual-stack validation).

## Documentation
- [x] Document workflow and architecture decisions in `/docs`.
- [x] Capture integration guide for frontend clients (API schema draft).
- [x] Write developer ergonomics survey template for week 2 milestone.

## Campaign & Victory

### Start Flow and Scenario Profiles
- [x] Implement `StartProfile` loader and schema (Owner: TBD, Estimate: 1.5d; Deps: `SimulationConfig` JSON plumbing). Description: Add `core_sim/src/data/start_profiles.json` and parse into `SimulationConfig`. Fields include starting units, knowledge tags, inventory, survey radius, fog mode, AI profile overrides. Cross-link: manual §2a Start of Game; architecture §Campaign Loop & System Activation. _Status_: Loader + schema ship in `core_sim/src/start_profile.rs`, JSON lives in `core_sim/src/data/start_profiles.json` and knowledge tags file, `SimulationConfig.start_profile_overrides` is populated on startup; documented in manual §2a and architecture §Campaign Loop.
- [x] Add `UnitKind::Founders` and `Command::FoundSettlement { q, r }` with validation (Owner: TBD, Estimate: 2d; Deps: unit/command plumbing). Description: Consume founders on success; create `Settlement` with TownCenter, unlock construction/logistics radius, emit `CampaignEvent::Founded`. _Status_: Added `found_settlement` command (proto + text), server handler, and `Settlement`/`TownCenter` components; Founders-only validation/consumption with provision cost; fog/survey recentering and command event logging; documented in manual §3a and architecture §Campaign Loop.
- [x] Telemetry and log frames for campaign events (Owner: TBD, Estimate: 1d; Deps: snapshot delta wiring). Description: Stream `CampaignEvent` frames (Founded, Milestone, Victory) for client narration. _Status_: Added campaign event kinds to the command event stream (including `campaign_founded` used by `found_settlement`), so snapshot/log frames now carry campaign narrations for client display.

### Capability Flags & System Gating
- [x] Add `CapabilityFlags` resource and snapshot field (Owner: TBD, Estimate: 1.5d; Deps: schedule registry). Description: Define bitflags for `Construction`, `IndustryT1`, `IndustryT2`, `Power`, `NavalOps`, `AirOps`, `EspionageT2`, `Megaprojects`. _Status_: Added `CapabilityFlags` resource (bitflags), inserted into app defaults, exported in snapshots/deltas (bincode + FlatBuffers), restored with snapshots, and surfaced via `sim_schema`/client payloads for UI gating.
- [x] Gate system schedules via run criteria (Owner: TBD, Estimate: 1.5d; Deps: `CapabilityFlags`). Description: Skip `power_tick`, `air_ops_tick`, etc., when flags are unset; ensure determinism unaffected. _Status_: Added run-if gating on stage chains (logistics, knowledge, great discovery, population, finalize/power) driven by `CapabilityFlags`.
- [x] Hook Great Discovery effects to flip capability flags (Owner: TBD, Estimate: 1d; Deps: Great Discovery resolver). Description: Register effect lambdas mapping discoveries → flags; allow scenarios to preflip. _Status_: Great Discovery effects now set Power/Megaprojects/EspionageT2 flags and drive schedule gating + UI locks.

### Victory Engine
- [ ] Add `victory_config.json` and `VictoryState` resource (Owner: TBD, Estimate: 2d; Deps: snapshot plumb). Description: Enumerate enabled modes, thresholds, scaling policy, dependencies.
- [ ] Implement `victory_tick` stage and win detection for Hegemony, Ascension, Economic, Diplomatic, Stewardship, Survival (Owner: TBD, Estimate: 3–4d; Deps: per-system metrics availability). Description: Compute progress per mode; when terminal, set winner and emit `CampaignEvent::Victory`.
- [ ] Expose victory progress and winner in snapshots; add continue-after-win toggle (Owner: TBD, Estimate: 1d; Deps: `VictoryState`).

### Client/UI
- [x] Scenario picker UI and Start Profile selection (Owner: TBD, Estimate: 1.5d; Deps: start profile loader). Description: Godot UI to choose scenario; pass id to server. _Status_: Inspector map tab now includes a Scenario picker tied to the new `start_profile` command; UI lists localized campaign profiles from snapshot telemetry, applies a selection, and optionally triggers map regeneration (`clients/godot_thin_client/src/scripts/Inspector.gd`, `Main.tscn`).
- [ ] Lock/disable UI tabs by `CapabilityFlags` until unlocked (Owner: TBD, Estimate: 1d; Deps: capability telemetry). Description: Gray out Power/Air/Naval/Spy tier 2 tabs until flags present; show tooltips linking to manual §2a.
- [ ] Victory panel rendering progress per mode + win screen flow (Owner: TBD, Estimate: 2d; Deps: `VictoryState` telemetry). Description: Allow enable/disable of modes per scenario.
- [x] Band/City panel — multi-column content fill when docked wide (top/bottom) (Owner: TBD, Estimate: 1d; Deps: `docs/plan_band_city_dock.md`, `BandCityPanel.gd`). Description: On an ultrawide top/bottom dock the bounded two-column layout leaves large empty space on the right and reads poorly. Make the band content flow into multiple columns to fill the strip. _Status_: `Hud._build_allocation_panel` split into discrete section blocks (`_build_allocation_sections`, per-row wiring preserved); panel now takes `set_band_sections(blocks)` and arranges them — tall = vertical stack, wide = manual balanced-column packing that **fits its T/B height to the content** (reservation = header + tallest column; re-packs on resize/content; cap-fraction scroll fallback) so it fills the width and never clips (`docs/plan_band_panel_wide_flow.md`).

### Campaign Labeling & Localization
- [x] Extend `StartProfile` schema with optional `display_title`/`display_subtitle` and propagate to `WorldSnapshot`/save metadata (Owner: Systems Team — TBD, Estimate: 1d; Deps: start profile loader). Description: Fallback when fields missing; expose via telemetry so clients can show “Trail Sovereigns” in chrome. _Status_: `core_sim` now loads `core_sim/src/data/start_profiles.json`, threads campaign labels through `SimulationConfig` → `CampaignLabel` resource → snapshot header/FlatBuffer field `SnapshotHeader.campaignLabel` (see `core_sim/src/start_profile.rs`, `core_sim/src/snapshot.rs`, `sim_schema/*`).
- [x] Client shell + localization wiring for campaign labels/taglines (Owner: Client Team — TBD, Estimate: 1d; Deps: schema extension). Description: Read localization keys for campaign title/subtitle, provide fallback strings, and emit label in analytics events per `docs/architecture.md` Brand section. _Status_: Godot HUD now loads `res://src/data/localization/en.json`, resolves `campaign_label` entries, displays them above the turn readout, and logs `[analytics] campaign_label …` when values change (`Main.gd`, `Hud.gd`, `LocalizationStore.gd`).

### Marketing & Narrative Support
- [ ] Lock primary marketing tagline for “Trail Sovereigns” and feed localization keys (Owner: Narrative Team — TBD, Estimate: 0.5d; Deps: campaign label schema). Description: Select from manual §Messaging Exploration, register `campaign.trail_sovereigns.tagline_primary` in string tables.
- [ ] Produce key art brief covering bands, portable hearth, and treaty visuals (Owner: Art Team — TBD, Estimate: 1d; Deps: messaging exploration). Description: Hand off manual cues to concept art with references for seasonal circuits, assay kits, and totemic route markers. Include the **evolving band map-icon stages** (Nomadic → Camp → Village, extensible) to replace the prototype emoji stand-ins (⛺/🛖/🏘️) with real sprite art — the `icon` per stage is config-defined (`settlement_stage_config.json`), so new art drops in by config.

### Nomadic Start Prototype
- [ ] Define default `late_forager_tribe` StartProfile (Owner: TBD, Estimate: 0.5d; Deps: StartProfile loader). Description: 2–3 bands, no permanent buildings, enable Nomadic commands; victory modes enabled: Hegemony, Cultural Diffusion, Stewardship, Survival.
- [ ] Implement `Band` units and roles (`Scout`, `Hunter`, `Crafter`/`Guardian`) (Owner: TBD, Estimate: 2d; Deps: unit registry). Description: Movement profiles, inventory capacity, fatigue, discovery throughput. **Reframed** by `docs/plan_early_game_labor.md`: the Scout/Hunter/Guardian *bands* become intra-band **roles** (a labor pool partitioned across Foraging/Hunting/Scouting/Warrior), not separate unit types — see the Early-Game Labor section below.
- [ ] ~~Implement `Camp` entity with portable buildings, light queue, storage, decay~~ **Superseded** by the Settlement & Population Economy arc (`docs/plan_settlement_population.md`): camps/settlements are **derived** from clustered populated tiles + tended improvements, not a discrete `Camp` entity. See the phase breakdown below.
- [ ] Commands: `FoundSeasonalCamp`, `AbandonCamp`, `SplitClan`, `MergeClan` (Owner: TBD; Deps: command plumbing). Note: reframed under the Settlement & Population Economy arc — camp lifecycle emerges from building/decay + migration rather than discrete found/abandon commands (clan split/merge becomes migration between locations).
- [x] Add `SedentarizationScore` resource and per-turn computation. `sedentarization_tick` (`sedentarization.rs`, `TurnStage::Population`) computes a per-faction 0–100 score — a config-weighted (`sedentarization_config.json`), EMA-smoothed blend of domestication (`HerdRegistry::domesticated_count`, the Phase E seam), provisions surplus, map resource density, and population — and pushes a `CommandEventKind::SedentarizationPrompt` on rising soft(~40)/hard(~70) crossings (edge-gated). Exported per-faction (`SedentarizationState`) + client HUD meter. _Remaining inputs deferred:_ storage/spoilage modifiers, trade-hub potential, travel fatigue, security; and per-faction-local (vs map-wide) resource density.

### Settlement & Population Economy

Design: `docs/plan_settlement_population.md`. The game's core early/mid economy — a localized
demographic population + labor allocation + a knowledge-gated improvement catalog, from which
**settlements emerge** (no discrete founding). Replaces the inert `found_settlement`/Founders
model and the `Camp`-entity item above; consumes the Wildlife Overlay's `domesticated_count` /
`SedentarizationScore` seams. Guiding principle: one general mechanism, scaled by config — a
lean-to and an arcology (and a 400k town vs a 5M city) are the same engine at different tuning.

- [ ] **Band / settlement Roster View (client; future).** A HUD table listing every player band
  (later: every settlement/city) with per-entry summary info — **population first** (the immediately
  useful column), later worker allocation, food/days, morale, activity. Motivation: the faction
  header shows the *total* population (e.g. "Pop 36") but a selected band shows only its own members
  (e.g. 32) — the difference is detached expeditions, which is currently opaque; a roster makes the
  whole faction legible at a glance and reconciles the totals (bands + expeditions = faction pop).
  Forward-looking: the same table becomes the cities/settlements list in the Settlement arc. Reuse
  `AutoSizingPanel.gd`. Data is already in the snapshot (`PopulationCohortState` per cohort incl.
  `isExpedition`); this is a client-only aggregation/rendering slice. Cross-ref this doc (feeds the
  emergent-settlement list).
- [x] Phase 0 — Design doc. Authored `docs/plan_settlement_population.md` (population/labor/
  improvements/settlements model + phasing) and cross-linked it (manual §Organic Settlement,
  `plan_wildlife_hunting_overlay.md`, `core_sim/CLAUDE.md`).
- [x] Phase 1 — Demographic population. `PopulationCohort` gained a 3-bracket age structure
  (children/working/elders, fixed-point) with per-turn births/aging/deaths modulated by
  food/morale/environment (`advance_demographics`, `systems.rs`), replacing the old growth clamp;
  rates in `demographics_config.json`. **Food is band-local from day one** — a per-cohort
  `food_store` larder that foraging/hunt/husbandry income fills and per-capita consumption drains
  (deficit-capped starvation → deaths); each band opens with `startup.food_reserve_days` of food
  seeded per-capita; provisions left `FactionInventory` (inter-band sharing + storage-pit
  distribution deferred to Phase 3). Brackets + larder persist in the snapshot (rollback), and a
  per-faction age-structure + dependency-ratio HUD readout ships (`PopulationDemographicsState`,
  wired like `SedentarizationState`). Localized (per band) from day one.
- [x] Supply network (Phase 1↔3 bridge). Band goods moved to a commodity-keyed `LocalStore`;
  `balance_supply_networks` (`supply.rs`, `TurnStage::Logistics`) unions same-faction bands within a
  configurable reach into networks that auto-balance stored goods per-capita each turn,
  throughput-limited + friction (`supply_network_config.json`) — a gatherer band can feed a nearby
  scout band. Reuses `grid_utils::wrapped_distance_sq`; the connected-components pass seeds Phase 4
  settlements. **Overlaps the unbuilt `RouteNetwork`/`RouteRightsTreaty` backlog below — coordinate
  when trade lands.** **`TradeLink`/`trade_knowledge_diffusion` are deprecated** (dormant; slated to
  become a trade *policy* on this network — consent gate + priced return flow — and removed then;
  fixes the latent logistics-snapshot-empty bug from the `TradeLink`-gated query).
- [ ] Phase 2 — Labor pool + hybrid allocation. Working-age → a local labor supply; a
  demand/allocation system (auto by priority + player override) across tending/construction/
  military/knowledge; client labor readout.
- [ ] Phase 3 — Improvement catalog + building + knowledge-gating. `Improvement` component +
  data-driven catalog by class (dwelling/storage/food-tending/…, each pure config: footprint,
  occupancy, labor_draw, build_cost, yield, decay, prerequisite) + `build` command (stockpiles +
  known knowledge tag + labor over turns) + multiple-per-tile footprint + dwellings housing tile
  population + tending-draw + decay. Sets the `CONSTRUCTION` capability bit. Snapshot + client
  (tile improvements + build UI). First catalog = the manual's storage hooks + corrals + tended
  patches. Likely splits (3a build+catalog, 3b tending/decay, 3c dwellings/housing).
- [ ] Phase 4 — Settlements as derived clusters. Compute the camp/settlement/town/city label from
  populated-tile clusters; tiering; retire discrete founding; rework `SedentarizationScore` into an
  emergent tether readout. Client settlement view. **Tiering should feed the config-driven
  settlement-stage system** (data-defined stages shipped per-band on `PopulationCohortState` as a
  single nested `settlementStage:SettlementStageView { id, label, icon }` — an interim size-driven proxy driving the evolving band
  map-icon; see follow-up below), enriching the stage-resolution inputs rather than adding new
  schema.
- [ ] Enrich settlement-stage resolution beyond the interim size proxy (Owner: TBD; Deps: Phase 3
  improvements, Phase 4 derived clusters). Settlement stages are **config-defined** (an ordered list
  of `{ id, label, icon, criteria: { min_size } }` in `settlement_stage_config.json`); the sim's
  `resolve_settlement_stage(&inputs, &stages)` helper picks the highest-ordered stage whose criteria
  are met and ships id/label/icon per band, driving the client band map-icon (⛺ Nomadic → 🛖 Camp → 🏘️ Village).
  Adding a new stage (e.g. a `town` entry) is a **pure config edit — no code**. The design is
  open/closed for new signals: the resolver takes an extensible `SettlementStageInputs` record and
  each stage declares an extensible `StageCriteria` record (only `min_size` today). Enriching the
  signal set as systems land — a first permanent improvement/structure (Phase 3), derived-cluster
  footprint + reworked sedentarization tether (Phase 4) — is a localized change: append a field to
  `SettlementStageInputs`, populate it at the one snapshot-builder call site, add an `Option` field to
  `StageCriteria`, and add one check line in the resolver predicate. Schema, config-list iteration,
  and client stay unchanged.
- [ ] Future arc — Borders & Government (deferred). A cluster of populated tiles is a **territory
  with borders**; a city *is* its (fluid) borders and the population within them → **government/
  governance** (settlement → town → city → nation). Border-claiming, territory, and government are
  a later arc; the link is documented in `docs/plan_settlement_population.md`.

### Early-Game Labor (Milestone 1)

Design: `docs/plan_early_game_labor.md`. Makes the first few turns playable by modeling the band
as a **labor pool** partitioned across equipment-gated **roles**, replacing the broken
"one band (~900) = one task" opening (a single food source sustainably feeds only ~10 people).
Realizes/concretizes Phase 2 (labor) of the Settlement & Population arc and adds two new concepts:
**equipment (TOE)** and a **carry-capacity population cap**. Everything config-driven (no hardcoded
literals). One uniform rule: each role has an *unequipped* and an *equipped* throughput tier;
equipment is consumable inventory; you allocate working-age labor across roles.

- [x] Design doc. Authored `docs/plan_early_game_labor.md` (labor-pool / TOE / carry-cap model +
  decisions + milestone breakdown); cross-linked (manual §Start of Game + §Wildlife/Sustain,
  `plan_settlement_population.md` Phase 2, `plan_wildlife_hunting_overlay.md`).
- [x] **Fractional food (step zero).** Hunt/forage/husbandry yields and the band larder accumulate
  fractionally — kill the round-to-0 that zeroes sub-1-per-source yields (the literal Issue-2 bug).
  Nothing else in M1 works without this. (`systems.rs` provisions math, `advance_fauna_pursuits`,
  larder store.)
- [x] **Config-driven small start.** Retired the hardcoded `900` in `spawn_profile_population`;
  `late_forager_tribe` now spawns **1 band of 30** via a per-unit `band_size` lever in
  `start_profiles.json` (default const `DEFAULT_STARTING_BAND_SIZE = 30` in `start_profile.rs`).
  Bracket split (initial_distribution) + `food_reserve_days` seeding flow through unchanged.
  _Carry-capacity headroom and starter TOEs land with their own slices below, not here._
- [x] **Labor allocation — sim (slice 3a).** Source-centric model: per-band assignment set
  (`{in-range source or band-role → workers}`, Σ ≤ working-age); **band work range `R`** (config,
  default 2) + a **move-band** command; **Forage** (in-range food-module tiles) + **Hunt** (in-range
  herds, with a **leashed follow** — bounded reuse of `FaunaPursuit` past `R`, lapses beyond the
  leash) yielding food **scaled by assigned workers** (the fix: `working` becomes a real producer —
  today no yield reads it); **Scout** (reveal outward) + **Warrior** (staffable, inert until the
  predator slice) as band-wide roles. Retires `reassign_band`'s one-task model + target-tile Harvest
  / whole-band Follow chase. Snapshot widened from the single `activity` string to a structured
  assignment set (schema + snapshot). Flat per-worker tier only (TOE multipliers = the TOE slice).
- [x] **Labor allocation — client (slice 3b).** Allocation panel: assign/unassign workers per
  in-range source (**per-source unassign is the new "cancel"** — supersedes the Issue-1 single-Cancel
  UI; command plumbing/optimistic-feedback pattern carries over); move-band command; role/worker
  readout. Consumes the widened snapshot assignment set.
- [ ] **Equipment / TOE model.** Per-role consumable equipment inventory; equipped/unequipped
  throughput tiers; **durability cliff** (full performance until expiry, then drop to unequipped);
  starter kit lasts ~15–20 turns (matched to `startup.food_reserve_days`); no crafting/replacement
  yet. Role-specific effects (baskets → forage yield + carry capacity; spears → hunt take; weapons
  → combat strength).
- [ ] **Carry-capacity population cap.** Band carry capacity gates population (births stop at cap,
  independent of food production); baskets and (future Phase-3) storage improvements raise it —
  the mechanical nomad→settle bridge.
- [ ] **Food ledger (client).** Per-band income/outflow breakdown (+forage/+hunt/+network/
  −consumption = net/turn → days to empty) + population-vs-carry-cap readout. Load-bearing
  legibility for the equilibrium/settle loop, not cosmetic.
- [ ] **M1-threats — minimal predators.** Predator pressure on the band / unguarded foragers &
  hunters, resolved against Warrior strength (equipped vs bare-handed) → casualties or yield loss.
  Folded into M1 (cheaper to build the Warrior↔threat interface now than retrofit); distinct slice
  so M1 can land without it if scope demands. Threat *variety* (barbarians, rival civs) deferred.
- [ ] Deferred (M2+): **Crafter role + crafting** to replenish/upgrade TOEs (the depletion pull);
  **larder spoilage + storage tiers** (spoilage matters once storage lets food sit); richer threats;
  the Settlement arc's Phase 3 improvement catalog (storage improvements consume the carry-cap seam).

### Intensification — Depletion → Domestication → Agriculture

Design: `docs/plan_intensification.md`. Completes the Neolithic transition: adds the missing
**pressure** (local resource depletion under population pressure — the honest carrying-capacity
mechanic that supersedes the reverted flat carry-cap) and the **plant** path (forage → cultivation →
farming), as a near-mechanical transpose of the shipped herd depletion + husbandry/domestication
systems. Realizes the Settlement arc's food-tending improvement class (tended patches / corrals).

- [ ] **Interaction + knowledge model — `docs/plan_intensification_ladder.md` (NEXT ARC, design done).**
  Unifies the two paths into one grammar: symmetric 3-rung ladders (forage→tended→farm /
  hunt→pastoral→pen), every transition a direct Cultivate-shaped verb (incl. a new **Tame** verb), all
  rungs **worker-driven** (retires passive-free pastoral; intensifying raises yield-per-worker + regen
  + ceiling + proximity), and a **practice-earns-next-knowledge** pattern (working rung N unlocks rung
  N+1's verb; two distinct meters — faction knowledge vs per-source progress — which is the fix to the
  hunting-vs-cultivate UX inconsistency). Core deliverable: a **generic rung engine + config ladder**
  (`intensification_ladder.json`) replacing today's bespoke pastoral/pen branches, so new rungs
  (selective breeding, irrigation) are config. Manual-first; reconcile with this section's Phase 0/1.
- [ ] **Phase 0 — Forage parity with hunting.** Make forage tiles **depletable**: transpose the herd
  `biomass`/`carrying_capacity`/logistic-regrowth (+ ecology phases) onto forage, moved into
  **persisted** state (`FoodModuleTag` is an unpersisted worldgen tag today — step zero). Forage take
  draws the stock down; `forage sustainable` becomes the real `net_biomass_delta` rate, so PR #110's
  overdraw ⚠ lights up for over-foraging automatically. Then give forage the **policy axis**
  (Sustain/Surplus/Market/Eradicate mirrored from Hunt): `LaborTarget::Forage` policy + `assign_labor`
  parse + snapshot + client picker. (May split: 0a depletion/persistence, 0b policy.)
- [ ] **Phase 1 — Cultivation + Corral.** Transpose husbandry to plants
  (`cultivation_progress`/`owner`/Sustain-accrual/`cultivate` command, mirroring
  `domestication_progress`/`domesticate`); complete cultivation into a **tended-patch** (farming) and
  land the **corral** (pastoral) — the place-bound food-tending improvements from
  `plan_settlement_population.md`, knowledge-gated (`farming`/`herding`), built/tended/decaying. Pulls
  forward a slice of the Settlement arc's `build`/improvement system. Feeds `SedentarizationScore`.
- [x] **Corral as a managed population (food upkeep → herd size → yield) — Phase 1a (sim) DONE.** The
  whole husbandry yield ladder is now **flow-based**: every rung pays MSY, and the rungs differ only in
  the **ecology** that MSY is computed against — wild `r` 0.05 → pastoral 0.15 → pen 0.60 (management
  buys a *growth rate*). The managed harvest **draws the herd down** (constant-escapement MSY, so it is
  stable from both sides), the pen **eats** (`pen.upkeep_per_biomass × biomass` from the keeper's
  larder), and **underfeeding shrinks the herd** — floored at the extinction floor, recoverable when fed
  again, announced in the feed. Both retired flat rates (`provisions_per_biomass` 0.01,
  `corral_provisions_per_biomass` 0.012), `fauna::corral_provisions`, and the "no draw-down" model are
  **deleted**. Also lands **`FaunaConfig::validate()`** (below), which enforces the pen's net-positive
  bound and the monotone ladder. Design: `docs/plan_corral_managed_population.md`; spec:
  `core_sim/CLAUDE.md` → **The husbandry yield ladder** + **Corral (Rung 1c)**.
  - [ ] **Phase 1b (client) — REQUIRED BEFORE THIS SHIPS TO A PLAYER.** The readout: the pen's
    `penUpkeep` as a **negative** row in the band's food ledger (against the **gross** `corralYield`),
    the `penFedFraction` starving warning, and the corrected policy hints. Both fields are already on
    the wire (`HerdTelemetryState`). Without it the player watches their larder drain with no
    explanation.
  - [ ] **Phase 2 (deferred) — grazing.** The pen's upkeep is drawn *first* from the tile's
    `ForagePatch` biomass (the animals eat grass — a resource humans cannot), and only the **shortfall**
    is hauled from the larder. Makes pasture quality gate pen size and tile choice matter, and removes
    the one honesty wrinkle Phase 1 cannot (with a single food scalar, food-in/food-out is a physically
    backwards conversion; real livestock is calorie-*negative* and worth it because it eats what we
    can't). `ForageRegistry`/`regrow_patch` already exist, so this is plumbing, not new modeling.
    **UPDATE (grazing 2d):** the *animal* side of this is now shipped — penned herds self-feed from
    the **graze** layer (`GrazePatch`), and only the shortfall is hauled from the larder
    (`pasture_fraction` offset; `docs/plan_grazing_2d.md` §2.3). What remains here is the *plant/forage*
    (`ForagePatch`) transpose for the cultivation side, not the pen.
- [ ] **Cross-cutting — command yield-vector + pre-commit forecast.** Model a command's
  multi-dimensional output (food + husbandry/cultivation progress + trade goods + discovery) and
  surface it live + as a compose-time **forecast** (projection fn mirroring the sim yield math, no
  mutation); policy becomes a visible tradeoff. Lands alongside Phase 0/1.
- [ ] Deferred (documented): full improvement catalog (dwellings/storage/defense), larder spoilage +
  storage tiers, richer crop/livestock variety, settlement-cluster derivation — owned by
  `plan_settlement_population.md`; this arc delivers the food-tending seam that feeds them.

### Fauna Roster & Ecology (data-driven species)

The grazing arc (2b pasture/graze layer, 2d pen economy + per-species husbandry ceiling) exposed that
the **start-game animal roster is thin** — after 2d only three species were pennable, none a true
pasture grazer. Grazing 2d added **Wild Aurochs** (→ cattle, grassland, `pen`) and **Crag Goats**
(hill/upland, `pen`) as the showcase livestock. This arc is the *considered* pass that fills the roster
out in totality with a coherent biome-ecology.

**Key finding — adding a species is already PURE CONFIG** (verified 2026-07-15). The old hardcoded
species enum is retired; a new animal is one `species` entry in `core_sim/src/data/fauna_config.json`
(id, `display_name`, `size_class`, `migratory`, `route_len`/`biomass`/`host_biomes`,
`fodder_per_biomass`, `regrowth_rate`, `husbandry_ceiling`). Spawn placement reads `host_biomes` (keys
into the 10 `FoodModule` buckets, `food.rs:41`), the client renders herds generically (icon inferred
from `display_name`; keywords for aurochs/bison/horse/goat/sheep already mapped in `FoodIcons.gd`), and
`species`/`sizeClass`/`husbandryCeiling` are free-form wire strings — **no Rust, client, or schema edit
to add an animal whose fields fit the existing enums.**

- [ ] **Fill out the start-game roster (config).** Add the missing animals as `fauna_config.json`
  entries with designed ecology (biome affinity via `host_biomes`, stat block, husbandry ceiling):
  **Bison** (migratory, `pastoral` — the plains counterpart to Steppe Runners); **Wild Horse**
  (grassland; decide `pastoral` vs `pen`); more **regional game** to give each biome a distinct fauna
  signature (currently many biomes share a short game list). Manual-first (new gameplay content →
  `shadow_scale_strategy_game_concept_technical_plan_v_0.md`), then the config entries. Verify each
  actually spawns (live `host_biomes` key + non-zero `abundance.per_biome`) — an unmatched key silently
  never spawns.
- [ ] **(Optional) Predators / threat fauna.** Wolves/big cats etc. are NOT husbandry — they need the
  M1 predator-pressure model (see Early-Game Labor → *M1-threats*), not a `SpeciesDef` alone. Scope
  with that arc, not this one.
- [ ] **(Optional) Remove the residual hardcoding — only if the roster needs it.** Two enums still bake
  behaviour: `SizeClass` (`fauna_config.rs:29`) fixes the movement cadence + `graze_range_radius` to
  three bands, and `FoodModule` + `classify_food_module_from_traits` (`food.rs:14`, `:164`) bucket
  TerrainType into 10 groups (so `host_biomes` can't target one exact terrain). To express a species
  with a *new* movement style, or biome affinity keyed on raw `TerrainType`, make those config-driven
  (movement fields on `SpeciesDef`; a data table or terrain-keyed affinity). The named roster above
  (bison/horse/caprines/aurochs) needs **none** of this — defer until a species actually requires it.

### Exploration & Wondrous Sites

Design: `docs/plan_exploration_and_sites.md`. The early-game exploration layer — makes scouting
real and gives exploration something to find. Companion to the Early-Game Labor arc (the Scout
role). Sequenced: local scout (small fix) → sites subsystem (foundation) → expeditions.

- [x] Design doc. Authored `docs/plan_exploration_and_sites.md` (local scout / expeditions /
  Wondrous Sites catalog); cross-linked from `plan_early_game_labor.md`.
- [ ] **Local scout — extend band sight.** The Scout labor role currently no-ops (radius-2 fog
  pulse < the band's passive sight 6, and unscaled). Make a band's sight range in
  `calculate_visibility` = `base_range + min(scouts × sight_bonus_per_scout, max_sight_bonus)` (read
  the cohort's `LaborAllocation` Scout count), so staffed scouts extend the live (Active) radius.
  New `labor_config.json scout` levers; retire the obsolete `reveal_radius`/`reveal_duration` use;
  client Scout hint → "Extends the band's sight; more scouts see further."
- [ ] **Wondrous Sites (minimal).** Data-driven site catalog (`sites_config.json` + loader,
  fauna-config pattern): `{id, category, display, placement_rule, discovery_reward}`. An optional
  per-tile site reference (schema + component), hidden under fog until discovered. Generic
  discovery: any vision source (band sight / local scout / expedition) reveals in-range sites → a
  discovered-sites registry → map marker + a **Discoveries** readout. Per-category reward hooks
  (settle-site / riches / tribe / landmark). **Point sites v1** (single tile; landmark = named
  point on its prominent tile); per-category placement (landmarks emergent from terrain at worldgen,
  riches from deposits, settle-sites derived, tribes seeded). Seed 2–3 site types.
- [ ] **PR 1 — Traveling-party system + scouting expedition** (folds in the `ResidentBand` isolation
  refactor). A visible detached party (own map marker) outfitted with workers + provisions
  (larder-drawn, scaled by size × distance), sent to a target via the **reused move-band click
  flow**; treks and uncovers the real Wondrous Sites along its path (deterministic finds).
  **Communication-range discovery** (the Columbus beat): the party carries a comm range (flat config
  lever, default 2 tiles, tech-scaling hook stubbed); it is **not** a live faction vision source
  (`Without<Expedition>` in `calculate_visibility`) — it accumulates observed tiles into a private
  **pending-reveal buffer**, and `advance_expeditions` flushes that into the faction map as
  `Discovered` only while **in comm range** of the band, so the map lights up as a lump **on return**,
  not live along the path. Site discovery then rides the existing `discover_sites` path for free.
  **Arrival is a decision point**, not an auto-turnaround: it enters an *awaiting-orders* state
  (marker state + feed line, player-visible regardless of comm range), and the player either **sends
  it onward to a new target** (chain waypoints — deciding blind if beyond comm range) or **orders it
  home**; return targets the band's **live** tile (band is nomadic). On return, workers + leftover
  provisions fold back into the band. Provisions **deplete per turn** (non-fatal if zero in v1 —
  deterministic success). Model: an expedition is **another `StartingUnit` band** reusing
  `PopulationCohort` + `BandTravel` + `LaborAllocation`, tagged with a new `Expedition` marker and
  **lacking `ResidentBand`**; isolate from the growing population/settlement arc via the positive
  **`ResidentBand`** marker on real bands (demographics / sedentarization / migration / startup-seed /
  supply-network / default-band command pickers filter `With<ResidentBand>`; expeditions excluded by
  construction). New `expedition_config.rs`/`.json`/`Handle`; `send_expedition` + `recall_expedition`
  commands (retarget reuses `move_band`); `advance_expeditions` system; `Expedition` snapshot-persisted
  (incl. pending-reveal buffer); snapshot discriminator `isExpedition`/`expeditionMission`/
  `expeditionPhase` on `PopulationCohortState`; client distinct glyph/label + outfit UI + recall +
  `marker_field_guard` update. Shares the deferred breakaway/split detached-party machinery (breakaway
  = an expedition that drops `Expedition`, gains `ResidentBand`, and keeps its `Discovered` map).
- [ ] **PR 2 — Hunting expedition** (second verb on PR 1's system; design §2b). Introduces the shared
  **take-food-from-a-nearby-source** primitive: the **hunt** verb does it continuously — a detached
  party that **follows a migratory herd beyond the leash** (which today lapses a Hunt at
  `band_work_range + hunt_leash_tiles`) by retargeting `BandTravel` to `herd.position()` each turn,
  takes food in reach (reusing the Hunt take math into its own store), **carries** it up to a carry
  capacity, and **drops it off at the band** when the herd's circuit nears the band OR the party is
  full. The same primitive **retrofits the scout's opportunistic replenish** (top up when provisions
  run low and it passes game — the real-life hunt-while-traveling). Resolved (§2b): **lives off its
  own kills** (no separate provisions for the hunt verb); **"full" = `party_workers × per_worker_carry`**
  (own lever — the band carry-cap slice is unbuilt); **auto-relaunch** on drop-off (loops the herd's
  circuit until recalled or herd extinct) + a recall/disband command; **risk/failure deferred** like
  the scout expedition. Client carried-food readout on the marker/panel.
- [x] **Expedition broadcast fix** (landed in the expedition work). World-mutating non-`Turn` commands
  (`send_expedition`/`send_hunt_expedition`/`recall_expedition`/`move_band`/`assign_labor`/…) mutate
  `app.world` immediately, but the post-command broadcast reused *last turn's* captured snapshot and
  only swapped the feed — so effects (worker draw, spawned expeditions, phase changes) were invisible
  until the next `run_turn`. Fixed: `recapture_snapshot_in_place` re-captures + rebroadcasts the fresh
  world after every command (ring-buffer-safe, no turn advance). Misleading `status=queued` → `applied`
  on expedition commands (`move_band` still says `queued` — cosmetic one-liner follow-up).
- [ ] **Hunting-expedition playtest fixes (finish the playable hunt — in progress).** From live
  playtest of PR 2: (1) **deliver flip-flop bug** — the party flipped to `Delivering` every turn when
  the herd sat within `drop_off_within_tiles` of the band regardless of carried amount, so it
  oscillated and never gathered; gate delivery on a worthwhile load (policy completion **or**
  `herd_near_band && carried ≥ hunt.min_deliver_fraction × cap`). (2) **productive take** — replace the
  near-zero surplus-skim with `workers × per_worker_biomass_capacity`/turn drawn from biomass. (3)
  **four-policy behaviours** (reuse `FollowPolicy`, chosen at launch — §2b): **Sustain** = harvest to
  the sustainable floor then one trip + done; **Surplus** = one full-cap haul + done; **Market** =
  repeated full-cap trips (grind down); **Eradicate** = hunt to extinction as *denial*, **no food
  delivered**, party self-feeds. (4) Client: `expeditionCarryCap` field (done) → **"carried / cap" +
  FULL** readout, a marker gather/haul indicator, the **Recall→"Returning"** hunt-panel fix, and a
  **policy picker** on the hunt launch.
- [x] **Fauna movement redesign — graze-wander + loiter-then-migrate (shipped; PR #100; fauna-layer).**
  Design: `docs/plan_wildlife_hunting_overlay.md` "Herd Movement". Fixes a latent bug: `advance_herds`
  calls `Herd::advance()` **every turn unconditionally**, but migratory `route`s are a sparse spiral of
  waypoints 4–12 tiles apart → a migratory herd **teleports 4–12 tiles/turn** (why an equal-speed party
  can't catch it). One primitive — **graze-wander** (dwell a turn, step ≤1 tile) — split by
  `Herd.size_class`: **wild game** (`Big`/`Small`) does permanent graze-wander in its local cluster
  (`dwell_turns` ~1 → ≈half speed, catchable); **migratory** (`Migratory`) alternates **loiter**
  (graze-wander ±1–2 of an anchor = the old waypoints, for `loiter_turns` many turns) with **migrate** (a
  directed leg to the next anchor at **1 hex/turn, no pause**). Requires **densifying** the sparse route
  into an adjacent hex-line path at spawn. New per-herd dwell/loiter/mode state on `Herd`; **per-species**
  config on `SpeciesDef` (`dwell_turns`, `loiter_turns [min,max]`, `loiter_radius`). Hunting works: catch
  during loiter, keep pace (trail 1 tile) through a migrate leg; a band's leashed Hunt still lapses on a
  long migration (→ that's what expeditions are for). No hard breakage (all consumers read live
  `position()`); pursuit becomes easier vs loitering herds — that's intended (difficulty is the risk
  layer, below). Document in `core_sim/CLAUDE.md` Fauna section too. Cross-ref: expedition hunt (§2b).
  - Future note: **hunt difficulty = danger, not movement** — mechanically-easy hunting now is intended;
    challenge lands with the expedition **risk/failure** layer (a mammoth can kill your party). Do NOT
    tune pursuit speed to make hunting hard.
- [ ] **Game trails → travel roads (future slice; documented).** Historically the first human paths
  followed game/herd trails. Repeated herd movement accumulates a **trail** on crossed tiles that, over
  time, becomes cheaper-to-traverse terrain — an emergent road network feeding movement/logistics
  (band/expedition travel cost). The movement redesign already leaves the signal (herds visit
  `position()` each turn); a trail-accumulation system consumes it. Retire the per-herd next-position
  heading arrow in favour of the accumulated trail when this lands. Design: `docs/plan_wildlife_hunting_overlay.md`
  "Herd Movement → Future concepts".
- [ ] Deferred / documented: expedition **risk/failure** (peril, non-return); **scouting-TOE**
  gating (with the TOE slice); **regional (multi-tile) sites**; richer per-category rewards;
  **tribes as real civilizations**.
- [ ] **Rollback restores `StartingUnit`** (pre-existing, all bands — surfaced during the expedition
  work, own small PR). `rollback <tick>` (dev/debug time-rewind) re-spawns each band's
  `PopulationCohort` from the snapshot but never persisted `StartingUnit` (unit kind/tags), so
  restored bands lose live fog-reveal **and** `move_band` controllability (expeditions inherit the
  same gap). Fix: persist `StartingUnit.kind`/`tags` on `PopulationCohortState`, re-attach on restore
  (build on the 2-pass restore the expedition PR added). Not expedition-specific; keep out of the
  expedition PR's diff. Check first whether `rollback` is player-reachable or dev-only (sets priority).
- [ ] **Hunt policy payoffs — make the four policies mean something distinct** (design-doc-first arc;
  surfaced by the Sustain-MSY work). The four `FollowPolicy` choices now differ cleanly in their *take*
  (Sustain = the MSY flow; Surplus/Market = stock headroom to the collapse floor; Eradicate = unfloored
  denial), but their **payoffs** do not yet differ in a way the player can reason about. Authoritative
  specs: `docs/plan_exploration_and_sites.md` §2b + `core_sim/CLAUDE.md` → Scouting & Hunting
  Expeditions. Four threads:
  - **(a) The expedition side-effect gap.** A **resident band's** Hunt arm credits, from the same take,
    food **+ trade goods** (Market) **+ husbandry/domestication accrual** (Sustain on a Thriving herd)
    — `advance_labor_allocation`. The **expedition's** Hunting arm credits **food only**. So a Sustain
    *expedition* builds no domestication and a Market *expedition* yields no trade goods, while the
    identical policy run by a band at home does both. Decide: does a detached party accrue husbandry /
    produce trade goods (one word, one meaning — the same discipline the take itself just got), or is
    the asymmetry *intentional* (husbandry needs a settled camp; a trade good needs somewhere to trade)?
    Either way it must be a decision, and the player-facing text must say which.
  - **(b) Market's trade goods have no live downstream.** The take credits `trade_goods` into
    `FactionInventory`, but the `TradeLink` path that consumed them is dormant/deprecated — so Market's
    distinguishing product currently buys nothing. Needs a real sink once trade re-lands on the supply
    network.
  - **(c) Eradicate needs a payoff beyond denial.** Today it is pure destruction: no food, no goods, no
    progress — meaningful only against a rival who wanted that herd, and there are no rivals yet. Define
    what it *earns* (hides/ivory? a cleared range for grazing/farming? a diplomatic lever?) or accept it
    as a deliberately costly scorched-earth verb and say so.
  - **(d) Communicate all of it.** Today Sedentarization consumes *both* a `domestication` input (fed by
    Sustain husbandry) **and** a `surplus` input (Σ band food larders, fed by any high-yield policy), so
    Sustain and Surplus both accelerate settling — by different inputs — and nothing tells the player
    that. The launch UI should state each policy's payoff, not just its yield rate.
- [ ] **Fog leak: herds are exported unfiltered** (pre-existing, surfaced by the hunt-trip-estimate
  work; own PR). `herd_snapshot_entries` (`core_sim/src/snapshot.rs`) applies **no visibility and no
  faction filter** — the server ships every herd's live biomass / position / ecology phase (and now its
  `huntTripEstimates` table) to every client, every turn. The client only hides them at *draw* time
  (`MapView._draw_herd` gates on `_is_tile_visible`), so at the wire level **the fog is decorative for
  fauna**: anyone reading the stream sees the whole map's game. Compare **Wondrous Sites**, which are
  exported per-faction precisely so the fog can't leak them (`snapshot_discovered_sites` — undiscovered
  sites never enter `TileState`). Fix = per-faction herd filtering in the snapshot, the same treatment
  sites got. Not urgent (single-player today, and every UX path already hides unseen herds — keep it
  that way meanwhile), but it is a real contract violation and it must land **before** any competitive
  multiplayer. Cross-ref: "Last-seen herd memory + estimated forecast" below (that feature *depends* on
  this fix, and the two together define what a client is allowed to know about a herd).
- [ ] **Last-seen herd memory + estimated (not exact) forecast** (design-then-build; **blocked on the
  fog-leak fix above**). Today a herd simply **vanishes** the moment its tile leaves `Active` — there is
  no remembered/ghost herd, so a player cannot plan around game they saw last turn. Add **last-seen herd
  memory**: a per-faction record of a herd's last-known biomass/position/phase + the turn it was seen,
  rendered as a stale "ghost" marker distinct from a live one. Then, for a **remembered but not
  currently visible** herd, the trip forecast (`HerdTelemetryState.huntTripEstimates`) must be shown as
  an **estimate, clearly flagged**: *"last seen N turns ago — the herd may have moved, grown, or been
  hunted; you won't know until you arrive."* Note **why this is safe today**: the exported forecast is
  unconditionally honest *only because you can currently target a herd only while you can see it* — the
  moment remembered herds become targetable, an unflagged exact number becomes a lie. Cross-ref: "Fog
  leak: herds are exported unfiltered" above; `core_sim/CLAUDE.md` → Scouting & Hunting Expeditions
  (the estimate's contract), `docs/plan_exploration_and_sites.md` §2b.
- [ ] **Band commands lack a faction check** (pre-existing, all band-command handlers — surfaced
  during the expedition work, own PR). `resolve_starting_unit_entity` validates entity-exists +
  `StartingUnit`/`ResidentBand` but never `cohort.faction == faction`, so a raw command with explicit
  `band_entity_bits` (`move_band`, `send_expedition`, etc.) could target **another faction's** band.
  Not UI-reachable, bounded harm, but a footgun. Fix uniformly at the shared resolver
  (`resolve_starting_unit_entity` / `select_starting_band`) — a faction-match gate applied across all
  band-command handlers — not per-handler. `send_expedition` already gained a `ResidentBand` gate in
  PR #96; the faction check is the remaining shared gap.

### Civilization Wellbeing (Morale → Discontent → Consequences)

Design: `docs/plan_civ_wellbeing.md`. Morale is a persistent, multi-factor civilization stat built
on the Phase-1 `PopulationCohort`: named factors → morale → discontent → consequences
(productivity, migration, [future] revolution). Extensible by design — every Phase-2 item below is
an addition to an existing seam, never a rewrite. Morale never causes faction population loss and
never gates births. **Phase 2 is deferred: those systems (government, tech, revolutions) live past
the early-turn loop currently being tuned.**

- [x] Phase 1 — the spine (PR #89). Morale via a named factor-contributor set
  (settling/terrain/climate/unrest; `MoraleContributions`); births decoupled from morale; discontent
  fraction + a **persisted `grievance` severity×duration accumulator** (populated, wired to no
  consequence yet); productivity as an `output_multiplier` **modifier stack** (discontent = entry #1),
  applied at every yield payout; tech-gated relocate-or-stay **migration**
  (`advance_population_migration`); `core_sim/src/data/wellbeing_config.json` levers; client
  Output%/itemized-morale-breakdown/recovery-guidance/"people leaving" readouts.
- [ ] Phase 2a — **Revolution** consequence off the `grievance` accumulator (sustained, trapped,
  rock-bottom grievance → uprising / loss of control / band schism). Seam:
  `PopulationCohort.grievance` (already persisted); wire a trigger + effect. (Owner: TBD; Deps: none —
  the state is live.)
- [ ] Phase 2b — **Additional morale factors**: nutrition/food, education, technology, government
  type, culture. Seam: add a `MoraleFactor` variant + one contributor line in `simulate_population`.
- [ ] Phase 2c — **Additional productivity modifiers** (education/tech/government). Seam: one line in
  `output_multiplier()`'s stack (the `// future:` comment marks the spot).
- [ ] Phase 2d — **Migration richness**: reach tied to concrete movement-tech tiers (replace the 1.0
  stub at the `TODO(phase2)` hook in `advance_population_migration`); cross-faction / settlement
  destinations.

### Early Diplomacy & Route Network
- [ ] Derive `RouteNetwork` overlay from movement/logistics traversals (Owner: TBD, Estimate: 2d; Deps: movement/logistics stage hooks). Description: Record adjacent-hex segment hits during movement/logistics; maintain exponentially decaying occupancy counters (fixed-point); surface segments above threshold via telemetry. Do not build a separate path graph.
- [ ] `RouteRightsTreaty` diplomacy primitive and pathing integration (Owner: TBD, Estimate: 2d; Deps: diplomacy system). Description: Treaties attach to traversal-derived segment keys (or named seasonal circuits); path cost/conflict checkers consult treaty state for friction/toll modifiers.
- [ ] Cultural Diffusion victory mode metrics (Owner: TBD, Estimate: 2d; Deps: VictoryState). Description: Compute influence along routes from time-weighted occupancy by faction, alliances, and cultural spread; feed into Victory Engine progress.

## World Generation (Map Builder)

### MVP Pipeline
- [ ] Heightfield + Coasts (Owner: TBD, Estimate: 2d; Deps: grid size config). Description: Generate seeded height raster, set sea level, classify ocean/shelf/inland sea; persist `elevation_m`.
- [ ] Climate Bands (Owner: TBD, Estimate: 1d; Deps: heightfield). Description: Assign `climate_band` via latitude proxy + elevation + moisture noise; store per tile.
- [ ] Hydrology: Flow/Accumulation + Rivers (Owner: TBD, Estimate: 3d; Deps: heightfield). Description: D8 flow, limited sink fill, flow accumulation; pick sources, trace polylines to sea; compute order/width; stamp `RiverDelta`, floodplain/wetland adjacencies; store `hydrology_id`.
- [x] Biome Stamping Integration (Owner: TBD, Estimate: 2d; Deps: climate/hydrology). Description: Feed climate/hydrology into `terrain_for_position` and add river/coast adjacency jitters; maintain terrain adjacency rules.
- [ ] Resource Surfacing (Owner: TBD, Estimate: 2d; Deps: chemistry tables). Description: Place deposits biased by `TerrainDefinition.resource_bias` and world chemistry tables; guarantee early fuel/conductor/structural paths near starts.
- [ ] Wildlife/Game Density (Owner: TBD, Estimate: 2d; Deps: biomes/hydrology). Description: Seed herd spawners and migratory paths; emit `game_density` scalar raster for foraging/hunting yields.
- [ ] Start Location Placement (Owner: TBD, Estimate: 1d; Deps: above). Description: Place nomadic bands near freshwater, forage clusters, soft metal + fuel path within N tiles; validate viability contract.
- [ ] Snapshot Overlays (Owner: TBD, Estimate: 1.5d; Deps: overlay plumbing). Description: Add `overlays.hydrology` (river polylines) and `overlays.game_density` (scalar raster) to snapshots and Godot client rendering.
  - [ ] Schema update (Owner: TBD, Estimate: 1d; Deps: sim_schema). Description: Add hydrology overlay contract (polylines or compressed raster) to schema and decoder; wire to Godot.

### Validation
- [ ] Determinism & Seeds (Owner: TBD, Estimate: 1d). Description: Ensure worldgen deterministic per seed; add test harness.
- [ ] Viability Checks (Owner: TBD, Estimate: 1d). Description: Assert starts satisfy manual §3a viability; fail-fast with regen if not.

### Map Presets & Tuning
- [ ] Map Presets file and loader (Owner: TBD, Estimate: 1d; Deps: SimulationConfig). Description: Add `core_sim/src/data/map_presets.json`, extend `SimulationConfig` with `map_preset_id`, load preset at startup, and include `preset_id` in `WorldSnapshot`.
- [ ] Implement Tag Budget Solver (Owner: TBD, Estimate: 2d; Deps: biome stamping). Description: Post-process to match `terrain_tag_targets` within `tolerance`, adjusting marginal tiles while respecting adjacency.
- [ ] Earthlike default preset (Owner: TBD, Estimate: 0.5d; Deps: presets loader). Description: Oceans/continents macro, climate weights, hydrology intensity, tag targets, and mild biome weight biases.

### Per-Map Biome Palette (`docs/plan_biome_palette.md`)
Restrict the *distinct biomes used on a given map* (legibility) while keeping the full 37-biome
library (variety). A curated, seed-driven, map-size-scaled subset — **this replaces the current
biome-placement behavior** (not an opt-in mode). Delivered as **one PR**. Design:
`docs/plan_biome_palette.md`.
- [x] Design doc (Owner: TBD, Estimate: 1d). Description: Palette model, `BiomeNiche` taxonomy, `must_have` flag, K-sizing curve, `bias_terrain_for_preset` remap seam, tag-budget-solver reconciliation, 3-biome revival, config schema. _Status_: `docs/plan_biome_palette.md`._
- [ ] Implementation — one PR (Owner: TBD, Estimate: ~5d; Deps: design doc). Description: (1) `BiomeNiche` enum + `niche`/`must_have` on `TerrainDefinition` (per §3.1/§3.2) + `biome_palette` `MapPreset` block/JSON; (2) revive Glacier/BasalticLavaField/AquiferCeiling with placement hooks (§3.6) + reachability tests; (3) `BiomePalette` resource + seeded coverage-first selection (`world_seed ^ PALETTE_SEED_SALT`), niche-nearest remap at the bias seam, tag-solver reconciliation (force-include locked-tag fallbacks + post-solver clamp); (4) tune `k_small`/`k_large` defaults. Verify via map-export biome histogram: small→few, large→many, seed-varied, climate coverage preserved, no off-palette biome present, all 37 reachable.

### Implemented (scaffold)
- [x] Load `map_preset_id` from SimulationConfig and presets file; log selection; insert preset handle as resource.
- [x] Generate hydrology rasters and basic river polylines at Startup using preset sea level.
- [x] Sea-level gating during tile spawn to stamp ocean vs land.
- [x] Simple Tag Budget Solver passes:
  - Wetland: promote tiles adjacent to rivers to `FreshwaterMarsh` until target share approached.
  - Fertile: promote adjacent-to-river land to `Floodplain` until target share approached.
  - Coastal: promote land adjacent to water to `TidalFlat` to raise `Coastal` coverage.
- [x] Hydrology overlay export: extended schema (FlatBuffers) with `HydrologyOverlay` and wired server snapshot + Godot decoder to render polylines.
- [ ] Godot preset selector (Owner: TBD, Estimate: 1d; Deps: loader). Description: UI to choose `map_preset_id`, pass through to server, display preset summary.




3. Once that lands, layer in the module-specific commands.
That second step will touch the protobuf/command parser, server handlers, inventory rewards, HUD buttons, and command log, so it’s best tackled after the overlay/schema change is merged.

scouting to expose terrain/food

## Wildlife & Hunting Overlay

Design: `docs/plan_wildlife_hunting_overlay.md`. Unifies wild game into the fauna
system (game = short-range herds), gives Harvest/Hunt/Follow their own verbs, and
retires the static `game_trail` food-site (which never survives food-site curation
— see the plan's Motivation). Phased so each lands independently.

### Phase A — game exists and is visible ✅
- [x] Species table in `fauna.rs`. `HerdSpecies` enum retired for a data-driven table in `core_sim/src/data/fauna_config.json` (loader `fauna_config.rs`): display name, size class, `migratory` flag, route length, group biomass, host biomes (keyed on `FoodModule`). Adds big game (deer, boar) and small game (rabbit, fowl) beside the migratory species. `Herd.species` is now the display-name `String`.
- [x] Short-route game spawning. `spawn_initial_herds` places migratory herds (long, start-anchored) plus short-route game by per-biome abundance (`classify_food_module`), shuffled + capped (`max_total_game`/`min_spacing`) for map-wide spread. Reuses `advance_herds` for roaming; `route_len == 1` game is stationary.
- [x] Retire `game_trail`. Removed the wild-game tile upgrade + `wild_game_*` config (`systems.rs` / `snapshot_overlays_config.rs` / `.json`) and `FoodSiteKind::GameTrail`; curation bug gone (no discounted candidates compete). Tile-based `HuntGame` handler neutralized (Hunt command kept for Phase B).
- [x] Client render pass. `FoodIcons.for_herd` gains rabbit/fowl glyphs; game renders with species icons keyed on the label's embedded species name; short routes show minimal/no trail. No schema change (snapshot already carries `species`).

### Phase B — Hunt (one-shot) ✅ (server + schema; client Hunt UI in Phase C)
- [x] Fauna-targeted Hunt command. `hunt_fauna <faction> <herd_id> [band]` (full proto/runtime/text/server plumbing) attaches a `FaunaPursuit` component; `advance_fauna_pursuits` (`TurnStage::Population`) re-targets the herd's live position each turn, steps the band toward it, and on ≤1 tile takes a portion of biomass → provisions/trade (config-driven yield). Server auto-picks a band when none is given.
- [x] Biomass regrowth. Per-turn logistic regrowth in `advance_herds` toward each group's per-species carrying cap (`Herd.carrying_capacity`, `ecology.regrowth_rate`); a group at ≤0 biomass despawns (local extinction).
- [x] Schema: fauna huntable/size fields. `HerdTelemetryState` gains `size_class` + `huntable` (biomass already present), wired producer→client-decode; no UI consumes them until Phase C.

### Phase C — Follow + policy ✅
- [x] Generalize Follow to carry a policy. `FaunaPursuitMode::Follow { policy: Sustain|Surplus|Eradicate }` on the Phase B `FaunaPursuit`; `advance_fauna_pursuits` pursues-to-adjacent then auto-hunts per policy each turn (Sustain=regrowth, Surplus=×mult, Eradicate=max) with a small non-food benefit (fog tracking pulse + morale). `follow_herd` gains `[policy] [band]` args; the one-shot teleport follow is retired. "Orders replace orders" (`reassign_band`) gives a band exactly one task.
- [x] Client: Harvest/Hunt/Follow in selection panel. Combined selection shows all applicable verbs when a hex has both a gather module and a fauna group; Hunt + Follow buttons + a Sustain/Surplus/Eradicate policy picker in the herd button group; both reuse the targeting-mode band-select banner. Dead `game_trail` tile-hunt path retired.

### Phase D — ecology ✅
- [x] Overhunting → collapse/extinction. `advance_herds` applies critical-depensation dynamics (`net_biomass_delta`): above the Allee threshold (`ecology.collapse_fraction * cap`) a group regrows logistically, below it it declines by `ecology.collapse_rate` — an irreversible crash to local extinction even without further hunting (despawned below `ecology.extinction_floor * cap`). New `ecology`/`immigration` tunables in `fauna_config.json`. Each `Herd` carries an `EcologyPhase` (Thriving/Stressed/Collapsing) exported as `HerdTelemetryState.ecologyPhase` and surfaced in the client selection panel (warned ⚠ readout); it is also the documented hook (`// Phase E hook:` in `advance_fauna_pursuits`) for the later domestication / industrialized-hunting arc. `repopulate_fauna` adds low per-turn immigration up to the abundance cap so an overhunted map slowly replenishes.

### Phase E — domestication (husbandry core) ✅
- [x] Emergent + explicit domestication. A `Herd` carries `domestication_progress` (0–1) + `owner`; a Sustain-follow on a Thriving herd accrues `husbandry.progress_per_turn` (`advance_fauna_pursuits`) while `advance_husbandry` (`TurnStage::Logistics`) decays untended progress and pays each domesticated herd's owner `biomass * husbandry.provisions_per_biomass` provisions without depleting it. A domesticated herd is collapse-immune (`regrow_biomass` uses logistic regrowth). The `domesticate <faction> <herd>` command (full proto/runtime/text/server plumbing, `handle_domesticate`) claims a herd early once progress ≥ `husbandry.claim_threshold`. New `husbandry` config block; `HerdTelemetryState.domestication` exported + client Husbandry readout (🐄). `HerdRegistry::domesticated_count` is the seam for the future `SedentarizationScore`. Deferred: industrialized/market hunting + the pastoral→settlement chain.

### Market hunting ✅
- [x] Commercial-hunt Follow policy. `FollowPolicy::Market` (Sustain | Surplus | **Market** | Eradicate) takes `market.take_fraction * biomass` each turn — a large commercial share that declines the herd fast into the Phase D depensation collapse — and sells it at `market.trade_goods_multiplier`× the normal trade-goods rate (`advance_fauna_pursuits`). New `market` config block; the policy is a free string parsed via `FollowPolicy::from_str`, so no proto/schema change — only the `FromStr`/`as_str`/resolve arms + the client policy picker (`FollowMarketButton`, `FOLLOW_POLICIES`). Completes the overlay's depletion-vs-domestication design. Deferred: the pastoral→corral→settlement chain (`Camp`, `SedentarizationScore`).
