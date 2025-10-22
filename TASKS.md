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
- [ ] Introduce `TradeKnowledgeDiffusion` stage that consumes openness metrics to share discoveries between factions (Owner: Ravi, Estimate: 2d; Blocked by schema/runtime helpers).
- [ ] Integrate migration-driven knowledge seeding into population movement systems (Owner: Elena, Estimate: 1.5d; Requires migration knowledge fragments in snapshots).
- [ ] Implement corruption passes for logistics, trade, and military budgets (Owner: Ravi, Estimate: 3d; Requires `CorruptionLedger` resource from data contracts).

### Culture Trait Stack
- [x] Implement multi-layer culture storage (`CultureLayer`, `CultureTraitVector`) and the reconcile routine propagating global → regional → local weights (Owner: Elena, Estimate: 3d; Deps: finalize trait list per game manual §7c).
- [x] Emit divergence telemetry (`CultureDivergence`, `CultureTensionEvent`, `CultureSchismEvent`) and wire into sentiment/diplomacy hooks (Owner: Ravi, Estimate: 2.5d; Deps: reconcile routine + event bus triggers).
- [x] Derive trait-driven system modifiers (`CultureEffectsCache`) and expose `CultureLayerState` snapshots/CLI overlays (Owner: Jun, Estimate: 2d; Deps: schema updates in `sim_schema`, inspector UI bandwidth).

## Data Contracts (`sim_schema` + `sim_runtime`)
- [x] Define FlatBuffers schema for snapshots and deltas.
- [x] Implement hash calculation for determinism validation.
- [x] Provide serde-compatible adapters for early testing.
- [ ] Extend trade link schema with openness/knowledge diffusion fields and migration knowledge summary payloads (Owner: Devi, Estimate: 1.5d; Deps: coordinate with `core_sim` turn pipeline + population serialization).
- [x] Add `CorruptionLedger` structs and subsystem hooks to snapshots (Owner: Devi, Estimate: 2d; Deps: align with logistics/trade/military component schemas).

## Godot Inspector Pivot
- [x] Extend Godot snapshot decoder to expose influencer, corruption, sentiment, and demographic data currently consumed by the CLI (Owner: TBD, Estimate: 1.5d; Deps: FlatBuffers topics stable).
- [x] Implement Godot inspector shell (tabbed/collapsible panels) with Sentiment, Terrain, Influencers, Corruption, Logs, and Command Console sections (Owner: TBD, Estimate: 3d; Deps: decoder extensions).
- [x] Add Godot-side controls for turn stepping, autoplay, rollback, axis bias adjustments, influencer support/suppress/channel boost, spawn, corruption injection, and heat debug (Owner: TBD, Estimate: 2d; Deps: command bridge). _Status_: Commands tab now issues all debug actions through the Godot client.
- [x] Pipe sim logs/tracing output into Godot inspector and surface recent tick sparkline/summary (Owner: TBD, Estimate: 1d; Deps: inspector shell). _Status_: tracing log stream now feeds the Logs tab (structured scrollback + command echoes) and plots recent turn durations via sparkline.
- [ ] Add terrain drill-down UI (per-biome detail view, tile inspection, future culture/military overlays) building on the new summary panel (Owner: TBD, Estimate: 2d; Deps: terrain tab groundwork).
- [ ] Deprecate CLI inspector: document migration, update workflows, remove `cli_inspector` crate once parity achieved (Owner: TBD, Estimate: 0.5d; Deps: Godot inspector feature parity).

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
- [x] Add CLI inspector panel summarizing active influencers and their impacts (Owner: Jun, Estimate: 1.5d).
- [x] Introduce scope-tiered influencer lifecycle (Local → Regional → Global) with staged promotion thresholds and persistent dormant state; include tooling hooks for deterministic testing (Owner: Ravi, Estimate: 3d).
- [x] Add multi-channel support model (popular sentiment, peer prestige, institutional backing, humanitarian capital) and domain-weighted coherence/ notoriety gains (Owner: Elena, Estimate: 3d).
- [x] Extend `support`/`suppress` command surface to manipulate both coherence and notoriety, plus scoped commands for channel-specific boosts; update CLI inspector with lifecycle badges, channel breakdown, notoriety display, and filter controls (Owner: Jun, Estimate: 2d).
- [x] Update documentation: lifecycle & support changes in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` and implementation details + testing guidance in `docs/architecture.md` (Owner: Mira, Estimate: 1d).
- [x] Replace sentiment sphere prototype drivers with real policy/event inputs: capture policy levers, incident deltas, and influencer channel outputs; expose telemetry hooks for testing (Owner: Elena, Estimate: 3d).

## Frontend Client Evaluation
- [x] Run Godot 4 thin client spike focused on tactical map rendering, overlays, and command round-trip metrics (Owner: Mira, Estimate: 3d; Output: `clients/godot_thin_client`, notes in `docs/godot_thin_client_spike.md`).
- [ ] Draft shared scripting capability model (API surface, sandbox permissions) to integrate with the Godot spike and keep Unity contingency-ready (Owner: Leo, Estimate: 2d; Deps: finalize snapshot topic catalog).
- [ ] Capture Godot spike findings in a client evaluation memo, including go/no-go recommendation and follow-up needs (Owner: Omar, Estimate: 1d; Deps: completion of Godot spike).
- [ ] (Conditional) Run Unity thin client spike if Godot outcome signals gaps that require comparison (Owner: Jun, Estimate: 3d; Deps: decision from evaluation memo).
- [x] Build lightweight snapshot proxy that converts binary `bincode` frames to JSON for tooling (Owner: Sam, Estimate: 1d; Deps: settle on schema exposure).
- [x] Retire JSON snapshot proxy and stream FlatBuffers snapshots directly (Owner: Sam, Estimate: 1d; Deps: Godot decoding path).
- [ ] Integrate FlatBuffers stream into Godot client (Rust GDExtension or native parser) and retire JSON proxy once stable (Owner: Mira, Estimate: 4d; Deps: FlatBuffers schema stabilized).
- [ ] Export dedicated logistics/sentiment rasters from `core_sim` snapshots (Owner: Devi, Estimate: 2d; Deps: align `SnapshotHistory` ring buffer + schema update).
- [ ] Extend `shadow_scale_flatbuffers`/Godot extension to surface multi-layer overlays (logistics, sentiment, corruption, fog) with toggleable channels (Owner: Mira, Estimate: 2d; Deps: raster export task).
- [ ] Validate Godot overlay rendering against CLI inspector metrics (add debug telemetry + colour ramp checks) before enabling designers (Owner: Omar, Estimate: 1d; Deps: overlay channel support).

## Tooling & Tests
- [x] Add determinism regression test comparing dual runs.
- [x] Introduce benchmark harness for 10k/50k/100k entities.
- [x] Integrate tracing/tracing-subscriber metrics dump accessible via CLI.
- [ ] Add regression coverage ensuring `TerrainOverlayState` updates propagate on biome/tag changes (Owner: TBD, Estimate: 1d; Deps: finalized terrain legend work).

## Core Simulation Roadmap
- [x] Implement per-faction order submission and turn resolution phases (Owner: Sam, Estimate: 4d).
- [x] Persist snapshot history for replays and rollbacks (Owner: Devi, Estimate: 3d).
- [ ] Replace text command channel with protobuf or JSON-RPC once control surface stabilizes (Owner: Leo, Estimate: 2d).

## Documentation
- [x] Document workflow and architecture decisions in `/docs`.
- [x] Capture integration guide for frontend clients (API schema draft).
- [x] Write developer ergonomics survey template for week 2 milestone.
