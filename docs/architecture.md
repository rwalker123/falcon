# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas, emits structured tracing/log frames, and receives text commands (`turn N`, `heat entity delta`, `bias axis value`). Snapshot deltas broadcast on `SimulationConfig::snapshot_bind` / `snapshot_flat_bind`, log feed on `SimulationConfig::log_bind`, and commands on `SimulationConfig::command_bind`.
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_schema::schemas/snapshot.fbs` for cross-language clients.
- **Shared Runtime (`sim_runtime`)**: Lightweight helpers (command parsing, bias handling, validation) shared by tooling and the headless core.
- **Inspector Client (`clients/godot_thin_client`)**: Godot thin client that renders the map, streams snapshots, and exposes the tabbed inspector; the Logs tab now subscribes to the tracing feed and renders a per-turn duration sparkline alongside the scrollback. A Bevy-native inspector is under evaluation (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` Option F) but would live in a separate binary to keep the headless core deterministic.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

### Terrain Type Taxonomy
See `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b for the player-facing palette. Implementation uses a `TerrainType` enum (u16) plus per-type metadata and tag bitsets that downstream systems interpret.

- **Data Shape**
  - `TerrainType` IDs: `DeepOcean`, `ContinentalShelf`, `InlandSea`, `CoralShelf`, `HydrothermalVentField`, `TidalFlat`, `RiverDelta`, `MangroveSwamp`, `FreshwaterMarsh`, `Floodplain`, `AlluvialPlain`, `PrairieSteppe`, `MixedWoodland`, `BorealTaiga`, `PeatHeath`, `HotDesertErg`, `RockyReg`, `SemiAridScrub`, `SaltFlat`, `OasisBasin`, `Tundra`, `PeriglacialSteppe`, `Glacier`, `SeasonalSnowfield`, `RollingHills`, `HighPlateau`, `AlpineMountain`, `KarstHighland`, `CanyonBadlands`, `ActiveVolcanoSlope`, `BasalticLavaField`, `AshPlain`, `FumaroleBasin`, `ImpactCraterField`, `KarstCavernMouth`, `SinkholeField`, `AquiferCeiling`.
  - `TerrainTags`: smallbit set for shared traits (`Water`, `Freshwater`, `Coastal`, `Wetland`, `Fertile`, `Arid`, `Polar`, `Highland`, `Volcanic`, `Hazardous`, `Subsurface`, `Hydrothermal`). Tags unlock grouping logic (e.g., wetlands for disease sims, volcanic for eruption checks).
  - Metadata per type: `movement_profile` (per domain penalties), `logistics_penalty`, `attrition_rate`, `resource_bias` (weights into ore/material tables), `detection_modifier`, `infrastructure_cost`, `disaster_hooks` (event dispatcher keys), `albedo`/`heat_capacity` seeds for climate modelling.
  - Climate coupling: worldgen stores `climate_band`, `elevation_m`, `hydrology_id`, and `substrate_material` alongside `TerrainType` so chemistry-driven systems stay synchronized.

- **Generation Pipeline**
  - Heightfield + hydrology drive coarse assignments. We stamp `TerrainType` first, then layer micro-variants (e.g., jitter between `Floodplain` and `FreshwaterMarsh` along meanders) while respecting adjacency constraints (`DeepOcean` → `ContinentalShelf` → `TidalFlat`/`RiverDelta` before land, `KarstCavernMouth` must border `KarstHighland` or `SinkholeField`, etc.).
  - Store adjacency metadata for amphibious logic (`MangroveSwamp` flagged as both `Coastal` and `Wetland`).
  - Hook geothermal/impact masks so `HydrothermalVentField`, `ActiveVolcanoSlope`, and `ImpactCraterField` remain rare but purposeful.

- **Simulation Hooks**
  - Movement & logistics: `movement_profile` feeds route selection, attrition ticks, and throughput caps. Naval pathing treats `InlandSea`, `RiverDelta`, and `HydrothermalVentField` as navigable; land convoys incur heavy penalties on `SaltFlat`, `AshPlain`, `BasalticLavaField` absent upgrades.
  - Resource surfacing: `resource_bias` seeds procedural deposits (e.g., `RockyReg` boosts rare metals, `PeatHeath` favors organic fuel, `FumaroleBasin` injects geothermal isotopes). Keep aligned with material generation tables.
  - Event systems: `disaster_hooks` register with crisis generators—`Floodplain` handles river surges, `ActiveVolcanoSlope` registers eruption flows, `ImpactCraterField` feeds meteor resonance, `SinkholeField` checks collapse cadence.
  - Detection & stealth: `detection_modifier` ties into reconnaissance; dense cover (`MixedWoodland`, `MangroveSwamp`) reduces spotting while open `PrairieSteppe` amplifies.

- **Telemetry & Clients**
  - Snapshots expose `terrain_type` per tile and a dedicated `terrainOverlay` raster (width/height + packed samples of terrain ID & tags) so clients can stream biome layers without recomputing from component state.
- Godot inspector consumes the same channel (`overlays.terrain`, `terrain_palette`, `terrain_tag_labels`) to colorize tiles and now drives an interactive drill-down: biome list selection refreshes tag histograms, cached tile samples, and a tile list whose hover/selection reveals per-tile telemetry (coords, terrain, tags, temperature, mass, element id). Culture/Military overlay tabs are scaffolded as placeholders that will light up once those snapshot channels ship, and `MapView.hex_selected` routes map clicks into `InspectorLayer.focus_tile_from_map` so selecting a hex synchronises biome/tile selection in the Terrain panel. Keep the narrative-and-implementation link tight by updating the manual (§3b) before tweaking palette data here, and log future overlay work in `docs/godot_inspector_plan.md`.
- Colour mapping: `MapView.gd::_terrain_color_for_id` mirrors the hex values listed in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b. Adjust the manual first, then this lookup, then the HUD legend/inspector summaries (see `TASKS.md`).
- Logs tab currently derives summary entries (tile/population/generation/influencer counts) from delta metadata so designers can spot bursts without tailing the terminal; replace this with streamed tracing once the backend forwards log lines.
- Commands tab exposes the full command bridge: turn/rollback/autoplay controls plus axis bias adjustment, support/suppress and channel boosts for selected influencers, spawn utilities, corruption injection, and heat debug. Use it to sanity check backend hooks before retiring the CLI.
  - Runtime controls: the thin client binds `ui_accept` to toggle between logistics/sentiment composites and terrain palette mode, aiding QA comparisons of colour accuracy against the documented swatches.
- Inspector migration: see `docs/godot_inspector_plan.md` for the roadmap and progress checkpoints; cross-link new UX notes into the manual when player-facing explanations change. If the Bevy inspector option graduates from evaluation (manual §13 Option F), capture the delta plan here and spin tasks into `TASKS.md`.
  - Planned logistics/sentiment raster exports (see `TASKS.md`) can stack on the same grid dimensions for consistent blending.

### Frontend Client Strategy
- **Goal**: Select a graphical client stack capable of rendering the live strategy map (zoom/pan, unit animation, layered overlays) while consuming headless snapshots and dogfooding the scripting API.
- **Spikes**: Prioritize a Godot 4 proof-of-concept client that replays mock `WorldDelta` frames and command queues to benchmark frame pacing, overlay rendering cost, and command latency. If Godot exposes blocking gaps, schedule a Unity thin visualization shell spike as a contingency comparison.
- **Metrics**: Target ≤16 ms frame time at desktop spec, responsive input-to-command round-trip, and acceptable draw-call budget for layered heatmaps. Capture qualitative notes on tooling ergonomics, asset workflows, and licensing/operational implications. (See `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Map-Centric Evaluation Plan”.)
- **Scripting Surface**: Design a capability-scoped scripting layer once (JS/Lua sandbox managed by the host). Integrate the facade with the Godot spike and be ready to reuse it in the Unity contingency so dashboards/mod extensions remain portable across host choices.
- **Decision Artifact**: Summarize results in an engineering decision memo that recommends the preferred client, contingency option, and follow-on work (e.g., WebGPU dashboard, Unity licensing mitigation) before committing to full UX build-out.
- **Resources**: Godot spike scaffolding lives under `clients/godot_thin_client`; see `docs/godot_thin_client_spike.md` for usage and evaluation notes.
- **Networking**: `clients/godot_thin_client/src/scripts/SnapshotStream.gd` consumes length-prefixed FlatBuffers snapshots from `SimulationConfig::snapshot_flat_bind` (`res/src/native` provides the Godot extension that decodes the schema generated from `sim_schema/schemas/snapshot.fbs`).
- **Next Steps (UI plumbing)**:
  - Emit the real logistics/sentiment rasters directly from `core_sim` so the client is no longer visualising proxy temperature values. Extend `SnapshotHistory` to cache those layers and expose them through the FlatBuffers schema.
  - Enrich the Godot extension to surface multiple overlays (logistics intensity, sentiment pressure, corruption risk) and update `MapView.gd` to switch/toggle between them instead of hardcoding a single blend.
  - Add instrumentation hooks so overlays can be validated against CLI inspector metrics while we iterate on colour ramps/normalisation.

### Trade-Fueled Knowledge Diffusion (Concept Backlog)
- Model discovery diffusion through trade openness, matching the narrative beats in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §8.
- Each bilateral trade edge tracks an **Openness** scalar (0–1) derived from treaties, tariffs, and infrastructure compatibility. Openness modulates the leak timer for discoveries shared between the two factions.
- Migration subsystems consume trade attractiveness scores; when population cohorts relocate, they carry partial progress toward technologies they already know, seeding receiving factions with accelerated research ticks.
- Closed or embargoed states extend leak timers and suppress migration-based boosts, but also lower trade throughput and sentiment toward openness-aligned factions.

#### Implementation Notes
- **Data Model**: Extend trade graph entities with a `TradeLinkKnowledge` component storing openness, last shared tech id, and decay timers. Population units gain an optional `KnownTechnologies` summary used during migration events.
- **Simulation Systems**: Add a `TradeKnowledgeDiffusion` stage after logistics to decrement leak timers, instantiate tech share events, and trigger migration knowledge transfers. Hook outputs into existing tech progress and discovery notification pipelines.
- **Balancing Hooks**: Expose tuning constants via `SimulationConfig` (e.g., openness-to-timer curve, migration knowledge fidelity) to iterate quickly during playtests.
- **Telemetry**: Emit metrics (`trade.tech_diffusion_applied`, `trade.migration_knowledge_transfers`) so the Godot inspector (and legacy CLI fallback) can visualize how trade openness reshapes tech parity.
- **Future UI**: Plan inspector overlays showing heatmaps of openness and pending diffusion events, keeping the feature visible during iteration.
- **Schema & Runtime Scope**: `sim_schema` gains `table TradeLinkKnowledge { openness: float32; leak_timer: uint32; last_discovery: DiscoveryId; decay: float32; }` referenced from `TradeLinkState`, plus an optional `KnownTechFragment` vector on migrating population records. `sim_runtime` exposes helpers to (a) compute openness deltas from treaties/infrastructure assets and (b) fold migrating cohorts’ knowledge fragments into the receiving faction’s research progress. Both crates depend on existing discovery ids and population serialization, so coordinate ordering changes with `core_sim` before bumping schema version.

#### Inspector Overlay Prototype Plan
- Gate rendering behind the `trade.tech_diffusion_applied` metric; reuse the Godot inspector snapshot stream to surface openness values per trade link (legacy CLI subscription stays available for verification).
- Start with a map-overlay panel that colorizes trade edges by openness and displays countdowns for active leak timers; use the sentiment heatmap widget as a code reference for gradient rendering.
- Add a secondary list widget showing migration-driven knowledge transfers (source faction, destination faction, tech fragment %, remaining turns) to give designers quick validation feedback.
- Instrument a dedicated Godot input action (e.g., `inspector_toggle_trade_overlay`) to show/hide the overlay without disrupting existing layouts, and keep the legacy CLI key binding for verification runs.

### Corruption Simulation Backbone (Concept Backlog)
- Provide a shared corruption metric per faction and per subsystem (logistics, trade, military, governance) that influences efficiency, sentiment, and diplomatic leverage as laid out in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §§6–9a.
- Corruption incidents originate from budget allocations (logistics maintenance, military procurement), trade routes (smuggling, tariff evasion), and population governance nodes (agency capture, black markets).
- Detection pipelines feed into sentiment and diplomacy outputs: exposed scandals create trust shocks and diplomatic modifiers, successful reforms grant temporary efficiency buffs.
- Integrate with calamity triggers by letting systemic corruption raise vulnerability multipliers (e.g., disaster relief misallocation).

#### Implementation Notes
- **Data Model**: Add `CorruptionLedger` resource tracking per-subsystem corruption intensity, active incidents, timers to exposure, and restitution potential. Extend relevant components (`LogisticsHubState`, `TradeLinkState`, `MilitaryUnitState`) with optional corruption hooks referencing ledger entries.
- **Simulation Systems**: Insert corruption evaluation passes after resource/budget allocation phases—e.g., `ApplyLogisticsCorruption`, `ApplyMilitaryProcurementCorruption`, `TradeSmugglingResolver`. Systems adjust throughput, equipment quality, or trade tariffs before downstream calculations.
- **Sentiment & Diplomacy Coupling**: Expose corruption-derived deltas to the sentiment sphere (Trust axis) and diplomacy engine via shared events (`CorruptionScandalEvent`, `PatronageStabilizerEvent`), keeping cross-system feedback explicit.
- **Controls & Policies**: Parameterize anti-corruption efforts (audits, special courts) as policy actions that reduce ledger magnitude at resource or political cost; enable espionage missions to induce or reveal corruption.
- **Telemetry**: Emit metrics (`corruption.incidents_active`, `corruption.resources_lost`, `corruption.trust_delta_applied`) for UI overlays and balancing.
- **Tooling**: Plan CLI inspector panels summarizing current corruption scores, incident timers, and recent exposés, aligning with future sentiment and trade overlays.
- **System Touchpoints**: Logistics throughput reducers (ghost shipments, maintenance fraud), trade smuggling/evasion modifiers, military procurement quality gates, and governance/population relief allocators must all consult the ledger. Each subsystem should expose hook points for both corruption inflow (register incidents) and mitigation (audits, reforms) so the ledger can orchestrate cross-system consequences.
- **Debug Hooks**: The headless server exposes a `corruption <subsystem> <intensity> <timer>` command (reachable from the CLI inspector via `g` after selecting a subsystem with `v`) that registers a synthetic incident for testing.

#### Schema Alignment Plan
- FlatBuffers: introduce `table CorruptionEntry { subsystem: CorruptionSubsystem; intensity: float32; incident_id: ulong; exposure_timer: uint16; restitution_window: uint16; last_update_tick: uint64; }` and `table CorruptionLedger { entries: [CorruptionEntry]; reputation_modifier: float32; audit_capacity: uint16; }`. Extend `FactionSnapshot` with `corruption: CorruptionLedger` and reference `incident_id` from `LogisticsHubState`, `TradeLinkState`, and `MilitaryFormationState`.
- Enum additions: `CorruptionSubsystem` extends existing subsystem enum; ensure values are appended to maintain backward compatibility. Version gate the schema by bumping `snapshot_schema_version` and providing upgrade notes in `sim_runtime`.
- Serialization: `sim_runtime` offers helper fns `ledger_mut(faction_id)` and `register_corruption_incident` to keep ECS code from touching FlatBuffer internals. Provide conversions for deterministic hashing (include ledger in hash inputs).
- Migration: author an interim adapter that treats missing ledger fields as empty (for save compatibility). Update determinism tests to account for ledger serialization order.
- Dependency sequencing: land schema change PR ahead of `core_sim` corruption passes; coordinate with CLI inspector instrumentation so telemetry ids stay stable.

#### Incident Prototype Plan
- Event Types: define `CorruptionIncident` (hidden) and `CorruptionExposure` (public) structs carrying subsystem, magnitude, implicated entities, and suggested follow-up actions. Wire both into the global event bus so sentiment and diplomacy systems subscribe without tight coupling.
- Generation Loop: after ledger updates, spawn incidents when intensity exceeds configurable thresholds; roll exposure each tick using audit capacity, external espionage pressure, or media freedom modifiers.
- Sentiment Hook: upon exposure, push `SentimentDelta { axis: Trust, magnitude: -incident.magnitude * trust_multiplier }`; anti-corruption projects dispatch inverse deltas when successful.
- Diplomacy Hook: publish diplomatic modifiers (`CorruptionLeverage` for rivals, `CorruptionSolidarity` for allies who cover up) with expiration timers mirrored to the incident restitution window.
- Metrics & Inspector: log incidents to `corruption.incidents_active` and `corruption.exposures_this_turn`; inspector dashboard will list active incidents with countdowns, plus last three exposures for quick validation.
- Validation Harness: craft scripted scenario in integration tests spawning deterministic corruption events, asserting sentiment/diplomacy metrics, and verifying ledger serialization via snapshot diff.
### Influential Individuals System
- **Data Contracts**: `InfluentialIndividualState` now captures scope tier (Local/Regional/Global/Generation), lifecycle, audience generations, notoriety, coherence, multi-channel support (popular, peer, institutional, humanitarian), channel weights, and cross-system bonuses in addition to id/domain/influence. Snapshots/deltas carry creation/removal diffs plus partial updates for lifecycle/coherence/channel changes to keep rollbacks deterministic.
- **Core Resources**: `InfluentialRoster` (deterministic SmallRng spawn pipeline) advances potentials through scope tiers, evaluates multi-channel coherence each tick, accumulates notoriety, and re-computes logistics/morale/power modifiers. `InfluencerImpacts` continues to expose aggregate multipliers used by downstream systems.
- **System Coupling**:
  - `tick_influencers` precedes materials/logistics/population/power, writing sentiment deltas into `SentimentAxisBias` and scaling logistics/morale/power via `InfluencerImpacts`.
  - Coherence is now a weighted blend of the four support channels. Popular sentiment draws from axis alignment and demographic share; peer prestige leans on Knowledge axes; institutional backing references Equity/Agency signals; humanitarian capital uses Trust + demographic morale. Channel boosts decay over time, ensuring propaganda spikes are temporary unless reinforced.
  - Lifecycle logic is scope-aware: Local potentials have lighter promotion thresholds, Regional/Global tiers require steeper coherence *and* notoriety, and Dormant figures persist (zero impact) until extreme stagnation clears them.
- **Commands & Networking**:
  - `support <id> [magnitude]` / `suppress <id> [magnitude]` nudge momentum while adjusting notoriety. `support_channel <id> <popular|peer|institutional|humanitarian> [magnitude]` applies targeted propaganda. `spawn_influencer [scope] [generation]` remains for deterministic testing.
  - Snapshot history retains influencer maps; deltas broadcast via `update_influencers` and `update_axis_bias` whenever lifecycle, channels, or manual biases change materially.
- **CLI Inspector**:
  - The roster panel displays lifecycle badges, scope tier, notoriety, channel breakdowns (scores + weights), and dominant support lanes. Hotkeys: `j/k` cycle selection, `s`/`x` issue support/suppress, `c` boosts the dominant channel, `f` cycles lifecycle filters, `i` spawns a fresh potential. Legend + filter metadata remain visible even when filters hide all entries.
- **Restore Path**: `restore_world_from_snapshot` rebuilds roster, channel boosts, impacts, and manual bias state from snapshot payloads, recomputing filters so UI/client state stays consistent. (See §7b of the game manual for narrative framing and player-facing messaging.)

### Sentiment Telemetry
- `SentimentAxisBias` now tracks three sources of pressure per axis: long-lived **policy levers**, transient **incident shocks**, and emergent **influencer deltas**. Corruption exposures call `apply_incident_delta`, which preserves the sampled trust hit alongside the ledger metadata.
- Snapshots emit a `SentimentTelemetryState` parallel to `AxisBiasState`. Each axis carries totals plus ranked `SentimentDriverState` entries tagged by category (Policy/Incident/Influencer). `SnapshotHistory` diffs that payload via `WorldDelta.sentiment`, so inspectors receive lightweight updates whenever contributions change.
- The CLI inspector swaps the previous heuristics for this telemetry feed. Policy adjustments, exposed incidents, and dominant influencer channels now surface explicitly in the Sentiment panel and event log, keeping balancing conversations grounded in the same numbers designers tune. The narrative framing lives in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7b.

### Culture Simulation Spine
- **See Also**: `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7c for player-facing framing of culture layers and trait axes.
- **Data Model**: Introduce `CultureLayer` resources scoped at faction (`Global`), region (`RegionalCultureId` keyed to provinces/territories), and settlement (`LocalCultureId`). Each layer stores a normalized trait vector (`CultureTraitVector { axis: [f32; N] }`, N=15 per manual) plus metadata: inheritance weights, divergence tolerance, last recalculated tick.
- **Trait Propagation**: On turn start, `reconcile_culture_layers` copies global baselines downward, then blends with regional/local deltas using configurable elasticity coefficients. Local events (policies, influencer actions, incidents) write deltas into the relevant layer; the reconcile system decays temporary modifiers and accumulates persistent shifts.
- **Divergence Tracking**: Maintain `CultureDivergence` components (per region/local) storing current deviation magnitude, warning thresholds, and time-above-threshold. When deviation crosses soft limits emit `CultureTensionEvent`; hard limits queue `CultureSchismEvent` for faction split/suppression logic. These events feed Sentiment (Agency/Trust axes) and Diplomacy reaction hooks.
- **Trait Effects Bridge**: Convert trait vectors into system-ready coefficients each turn: e.g., `Aggressive` drives `MilitaryStanceBias`, `Open` modifies knowledge leak timers, `Devout` seeds ritual demand for logistics. Implement via `CultureEffectsCache` resource consumed by population, logistics, diplomacy, and espionage systems.
- **Religion Integration**: Represent sect dynamics as tagged modifiers on the `Devout`, `Mystical`, and `Syncretic` axes rather than a discrete subsystem. High Devout + Mystical regions spawn `RitualSchedule` entries that schedule pilgrimage logistics and sentiment modifiers; secular regions skip creation.
- **Telemetry & UI**: Extend snapshots with `CultureLayerState` payloads (per layer trait vectors + divergence meters) so the Godot inspector’s Culture tab can surface the “Cultural Inspector” referenced in the manual. Provide layer filters and clash forecasts derived from pending `CultureTensionEvent`s.
- **Task Hooks**: Flag follow-up work in `TASKS.md` (forthcoming entries) for implementing the reconcile system, divergence events, and telemetry serialization so engineering backlog stays aligned with the manual.
- **Schema Plan (`sim_schema`)**: Amend `snapshot.fbs` with the culture payload contracts before engineering begins:
  - `enum CultureLayerScope { Global, Regional, Local }` to disambiguate layer granularity in snapshots/deltas.
  - `enum CultureTraitAxis` enumerating the 15 axes in the manual (`PassiveAggressive`, `OpenClosed`, … , `PluralisticMonocultural`) so trait vectors are stable across runtimes.
  - `table CultureTraitEntry { axis: CultureTraitAxis; baseline: long; modifier: long; value: long; }` capturing inherited baseline, applied adjustments, and resolved value per axis.
  - `table CultureLayerState { id: uint; owner: ulong; parent: uint; scope: CultureLayerScope; traits: [CultureTraitEntry]; divergence: long; softThreshold: long; hardThreshold: long; ticksAboveSoft: ushort; ticksAboveHard: ushort; lastUpdatedTick: ulong; }` describing each layer plus divergence telemetry.
  - `enum CultureTensionKind { DriftWarning, AssimilationPush, SchismRisk }` with `table CultureTensionState { layerId: uint; scope: CultureLayerScope; severity: long; timer: ushort; kind: CultureTensionKind; }` to expose pending clash forecasts to clients.
  - Extend `WorldSnapshot`/`WorldDelta` with `cultureLayers:[CultureLayerState]`, `removedCultureLayers:[uint]`, and `cultureTensions:[CultureTensionState]` payloads so downstream tooling can visualize divergence without additional queries.

## Turn Loop
```text
per-faction orders -> command server -> turn queue -> run_turn -> snapshot -> broadcaster -> clients
```
- Server collects one order bundle per faction before resolving a turn.
- Orders enqueue into [`TurnQueue`] and, once all factions submit, the simulation resolves via `run_turn`.
- Legacy `turn N` command auto-fills missing factions with `EndTurn` orders for rapid testing.
- Each turn emits structured logs (`turn.completed`) with aggregate metrics.
- Frontends may queue multiple `turn` commands (e.g., advance 10 turns) or submit explicit `order <faction> ready`.

### Turn Phases
1. **Collect** – `TurnQueue` awaits submissions from all registered factions (`order <faction> ready`).
2. **Resolve** – When all orders are present the server applies directives, executes `run_turn`, captures metrics, and broadcasts the snapshot delta.
3. **Advance** – Queue resets for the next turn, reopening the Collect phase. Auto-generated orders keep single-faction testing ergonomic while preserving multi-faction semantics.

## Snapshot History & Rollback
- `SnapshotHistory` retains a ring buffer of recent `WorldSnapshot` + `WorldDelta` pairs (default 256 entries; configurable via `SimulationConfig.snapshot_history_limit`).
- `rollback <tick>` rewinds the simulation to a stored snapshot, resets the ECS world, resets `SimulationTick`, and truncates history beyond that point.
- After a rollback the server broadcasts the archived snapshot payload and logs a warning instructing clients to reconnect (current delta stream cannot apply reverse diffs).
- Ring entries expose encoded binary blobs for offline replay tooling and deterministic validation.

## Data Flow
- **Snapshots**: Binary `bincode` frames prefixed with length for streaming.
- **FlatBuffers**: Schema mirrors Rust structs for alternate clients.
- **Logs**: Length-prefixed JSON frames carrying `tracing` events published via the log stream socket (default `tcp://127.0.0.1:41003`).
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing` (`turn.completed` now emits `duration_ms` alongside grid metrics for client consumption).

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

## Next Steps
- ~~Implement per-faction order submission and turn resolution phases.~~ (Handled via `TurnQueue` + per-faction `order` commands.)
- ~~Persist snapshot history for replays and rollbacks.~~ (Ring-buffered `SnapshotHistory` with `rollback` command.)
- Replace text commands with protocol buffers or JSON-RPC once control surface stabilizes.
