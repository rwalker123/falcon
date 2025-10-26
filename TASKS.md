# Prototype Task List

## Core Simulation (`core_sim`)
- [x] Flesh out deterministic ECS systems (materials, logistics, population).
- [x] Replace placeholder system with staged schedules and fixed-point math.
- [x] Add snapshot/delta serialization hooks feeding `sim_schema` schemas.

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
- [x] Add terrain drill-down UI (per-biome detail view, tile inspection, future culture/military overlays) building on the new summary panel (Owner: TBD, Estimate: 2d; Deps: terrain tab groundwork). _Status_: Godot Terrain tab now offers biome selection with tag breakdowns, hover/click tile telemetry, and placeholder culture/military overlay tabs ready for incoming streams.
- [x] Deprecate CLI inspector: document migration, update workflows, remove `cli_inspector` crate once parity achieved (Owner: TBD, Estimate: 0.5d; Deps: Godot inspector feature parity). _Status_: CLI crate removed, docs/workflows now point exclusively to the Godot thin client.
- [x] Support map zooming/panning via both mouse and keyboard inputs in the Godot inspector (Owner: TBD, Estimate: 1d; Deps: confirm MapView input bindings).
- [x] Introduce a shared typography theme for the Godot inspector that resolves `INSPECTOR_FONT_SIZE`, defines derived scale constants, and applies the theme to all static and runtime-created controls (Owner: TBD, Estimate: 1.5d; Deps: docs/architecture.md Inspector Typography Refactor Plan).
- [x] Rework HUD/inspector layout math to consume the shared typography metrics so panel placement adapts to base font changes without manual offsets (Owner: TBD, Estimate: 1d; Deps: shared typography theme scaffolding).

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

## Shared Scripting Capability Model
- [x] Implement QuickJS GDNative module and runtime bootstrap inside `clients/godot_thin_client` (`ScriptHost` worker threads, capability token plumbing, manifest loading) (Owner: Mira, Estimate: 3d; Deps: manifest schema in `docs/architecture.md`). _Status_: QuickJS runtime migrated to the new `quick-js` bindings, manifest/session plumbing verified, and threads spawn/tear down cleanly after `cargo check`.
- [x] Wire script capability enforcement to Godot bridges (telemetry subscriptions, `CommandBridge` dispatch, session storage serialization) and add per-frame watchdog handling (Owner: Leo, Estimate: 2.5d; Deps: QuickJS runtime integration). _Status_: Telemetry, command, session, and alert capabilities verified in Godot; runtime watchdog/tick metrics confirmed under QuickJS.
- [x] Expose `CapabilitySpec` registry from `sim_runtime` and ship manifest lint/tests ensuring topic/command IDs stay in sync (Owner: Sam, Estimate: 1.5d; Deps: finalized capability list). _Status_: `sim_runtime::scripting` now publishes the capability registry and manifest parsing enforces coverage for telemetry subscriptions with unit tests.
- [x] Build Script Manager UI panel in Godot (list manifests, capability review, enable/disable, error surfaces) and integrate `console`/alert channels into the Logs tab (Owner: Jun, Estimate: 2d; Deps: runtime bootstrap + logging bridge). _Status_: Scripts tab loads packages from both roots, enable/disable wiring works, and log/alert signals flow into Inspector.
- [x] Deliver `tools/script_harness` headless runner with mock feeds, fuzz hooks, and CI budget assertions for sandbox violations (Owner: Omar, Estimate: 2d; Deps: capability registry & host bindings). _Status_: Harness builds against the native runtime and exposes tick/event CLI hooks; next step is adding scripted smoke tests.
- [x] Implement save/load serialization for active scripts and `storage.session` payloads via new `SimScriptState` struct and add regression coverage (Owner: Devi, Estimate: 1.5d; Deps: `sim_runtime` capability registry). _Status_: `SimScriptState`/`ScriptManifestRef` capture enabled script metadata; Godot host exposes `capture_state`/`restore_state`, and runtime applies session + subscription restores with validation.

## Tooling & Tests
- [x] Add determinism regression test comparing dual runs.
- [x] Introduce benchmark harness for 10k/50k/100k entities.
- [x] Integrate tracing/tracing-subscriber metrics dump accessible via CLI.
- [ ] Add regression coverage ensuring `TerrainOverlayState` updates propagate on biome/tag changes (Owner: TBD, Estimate: 1d; Deps: finalized terrain legend work).

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
