# Prototype Task List

## Core Simulation (`core_sim`)
- [x] Flesh out deterministic ECS systems (materials, logistics, population).
- [x] Replace placeholder system with staged schedules and fixed-point math.
- [x] Add snapshot/delta serialization hooks feeding `sim_schema` schemas.
- [x] Draft power phase architecture updates that align `run_turn`'s power stage with `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §4, enumerate required energy subsystems (generation forms, instability handling, grid state), and spill follow-on implementation tickets once documented (Owner: TBD, Estimate: 1.5d; Deps: review existing materials/logistics schedules). _Status_: Plan captured in `docs/architecture.md` §Power Systems Plan and synced with manual §4 Power Simulation Pillars.
- [x] Capture knowledge ledger/leak mechanic design in `docs/architecture.md` per manual §5a—cover timers, espionage modifiers, UI data feeds—and produce downstream tasks for backend and Godot wiring (Owner: TBD, Estimate: 1.5d; Deps: Great Discovery architecture outline). _Status_: Architecture captured in `docs/architecture.md` §Knowledge Ledger & Leak Mechanics with manual §5a alignment; telemetry/command integrations enumerated and follow-on tickets below._
- [ ] Stand up `KnowledgeLedger` infrastructure in `core_sim` (resource representations, stage registration, baseline leak math) according to `docs/architecture.md` §Knowledge Ledger & Leak Mechanics (Owner: Systems Team — Ravi, Estimate: 2d; Deps: trade diffusion plumbing, Great Discovery hooks). _Status_: Resource scaffolding, snapshot serialization, telemetry logging, and the new `TurnStage::Knowledge` are in place (`core_sim/src/knowledge_ledger.rs`, `core_sim/src/lib.rs`, `core_sim/src/snapshot.rs`); leak math/infiltration flows remain._
  - [ ] Implement leak modifier aggregation + half-life recomputation inside `knowledge_ledger_tick` following architecture table values.
  - [ ] Persist espionage probes/counter-intel events into the ledger (requires upcoming espionage plumbing).
- [ ] Implement leak progression, espionage/counter-intel event handling, and metrics export in `knowledge_ledger_tick`, finishing snapshot serialization into the new schema tables (Owner: Systems Team — Ravi, Estimate: 1.5d; Deps: KnowledgeLedger infrastructure task).
- [ ] Generate rich knowledge telemetry frames once event ingestion lands (Owner: Systems Team — Ravi, Estimate: 0.5d; Deps: espionage event hooks).
- [ ] Extend the Godot thin client with the Knowledge Ledger panel and command surface described in `docs/architecture.md` §Knowledge Ledger & Leak Mechanics—subscribe to ledger/telemetry payloads, render overview/detail UI, and expose counter-intel/security posture controls (Owner: Client Team — Elena, Estimate: 2d; Deps: backend ledger payload & command endpoints).
- [ ] Scope crisis telemetry channels promised in manual §10 by inventorying required metrics, network payloads, and inspector overlays in `docs/architecture.md`, then derive execution tasks for simulation and client teams (Owner: TBD, Estimate: 1.5d; Deps: Godot inspector overlay bandwidth).

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
- [ ] Add regression tests and benchmarks covering stability band transitions, cascade propagation, and serialization/delta output for power telemetry (Owner: TBD, Estimate: 2d; Deps: core_sim implementation + schema updates).

### Terrain Foundations
- [x] Implement `TerrainType` enum + tag metadata in worldgen and data contracts (Owner: TBD, Estimate: 2d; Deps: align with `docs/architecture.md` Terrain Type Taxonomy).
- [x] Expose terrain IDs/tag bitsets through snapshots and FlatBuffers for client overlays (Owner: TBD, Estimate: 1.5d; Deps: terrain enum integration).
- [x] Extend logistics/population systems to surface telemetry for terrain attrition effects (Owner: TBD, Estimate: 1d; Deps: terrain-aware simulation hooks).
- [x] Add client overlays (CLI & Godot) that visualise terrain classes and tags using the exported channel (Owner: TBD, Estimate: 1.5d; Deps: terrain telemetry stream).
- [x] Document palette alignment across `shadow_scale_strategy_game_concept_technical_plan_v_0.md` and `docs/architecture.md` (Owner: TBD, Estimate: 0.5d; Deps: verified Godot palette mapping).
- [x] Add Godot/CLI terrain legend surfaced from shared palette data (Owner: TBD, Estimate: 1d; Deps: palette documentation).

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

## Data Contracts (`sim_schema` + `sim_runtime`)
- [x] Define FlatBuffers schema for snapshots and deltas.
- [x] Implement hash calculation for determinism validation.
- [x] Provide serde-compatible adapters for early testing.
- [x] Extend trade link schema with openness/knowledge diffusion fields and migration knowledge summary payloads (Owner: Devi, Estimate: 1.5d; Deps: coordinate with `core_sim` turn pipeline + population serialization).
- [x] Add `CorruptionLedger` structs and subsystem hooks to snapshots (Owner: Devi, Estimate: 2d; Deps: align with logistics/trade/military component schemas).

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

## Shared Scripting Capability Model
- [x] Implement QuickJS GDNative module and runtime bootstrap inside `clients/godot_thin_client` (`ScriptHost` worker threads, capability token plumbing, manifest loading) (Owner: Mira, Estimate: 3d; Deps: manifest schema in `docs/architecture.md`). _Status_: QuickJS runtime migrated to the new `quick-js` bindings, manifest/session plumbing verified, and threads spawn/tear down cleanly after `cargo check`.
- [x] Wire script capability enforcement to Godot bridges (telemetry subscriptions, `CommandBridge` dispatch, session storage serialization) and add per-frame watchdog handling (Owner: Leo, Estimate: 2.5d; Deps: QuickJS runtime integration). _Status_: Telemetry, command, session, and alert capabilities verified in Godot; runtime watchdog/tick metrics confirmed under QuickJS.
- [x] Expose `CapabilitySpec` registry from `sim_runtime` and ship manifest lint/tests ensuring topic/command IDs stay in sync (Owner: Sam, Estimate: 1.5d; Deps: finalized capability list). _Status_: `sim_runtime::scripting` now publishes the capability registry and manifest parsing enforces coverage for telemetry subscriptions with unit tests.
- [x] Build Script Manager UI panel in Godot (list manifests, capability review, enable/disable, error surfaces) and integrate `console`/alert channels into the Logs tab (Owner: Jun, Estimate: 2d; Deps: runtime bootstrap + logging bridge). _Status_: Scripts tab loads packages from both roots, enable/disable wiring works, and log/alert signals flow into Inspector.
- [x] Deliver `tools/script_harness` headless runner with mock feeds, fuzz hooks, and CI budget assertions for sandbox violations (Owner: Omar, Estimate: 2d; Deps: capability registry & host bindings). _Status_: Harness builds against the native runtime and exposes tick/event CLI hooks; next step is adding scripted smoke tests.
- [x] Implement save/load serialization for active scripts and `storage.session` payloads via new `SimScriptState` struct and add regression coverage (Owner: Devi, Estimate: 1.5d; Deps: `sim_runtime` capability registry). _Status_: `SimScriptState`/`ScriptManifestRef` capture enabled script metadata; Godot host exposes `capture_state`/`restore_state`, and runtime applies session + subscription restores with validation.
- [x] Formalize the scripting manifest contract: publish JSON schema, add lint/validation tooling against `CapabilitySpec`, document host runtime checks, and sync manual/architecture references (Owner: TBD, Estimate: 2d; Deps: capability registry finalized). _Status_: Schema emitted to `docs/scripting_manifest.schema.json`, `cargo xtask validate-manifests` enforces shape + capability coverage, and docs/manual sections reference the contract + runtime checks.

## Tooling & Tests
- [x] Add determinism regression test comparing dual runs.
- [x] Introduce benchmark harness for 10k/50k/100k entities.
- [x] Integrate tracing/tracing-subscriber metrics dump accessible via CLI.
- [x] Add regression coverage ensuring `TerrainOverlayState` updates propagate on biome/tag changes (Owner: TBD, Estimate: 1d; Deps: finalized terrain legend work). _Status_: Exercised by `snapshot::tests::terrain_overlay_delta_updates_on_biome_change` covering biome/tag mutation delta emission.

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
