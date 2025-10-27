# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas, emits structured tracing/log frames, and receives control commands. Commands flow over a single length-prefixed Protobuf `CommandEnvelope` socket (`SimulationConfig::command_bind`), while snapshots broadcast on `SimulationConfig::snapshot_bind` / `snapshot_flat_bind` and logs on `SimulationConfig::log_bind`.
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_schema::schemas/snapshot.fbs` for cross-language clients.
- **Shared Runtime (`sim_runtime`)**: Lightweight helpers (command parsing, bias handling, validation) shared by tooling and the headless core.
- **Inspector Client (`clients/godot_thin_client`)**: Godot thin client that renders the map, streams snapshots, and exposes the tabbed inspector; the Logs tab subscribes to the tracing feed, offers level/target/text filters, and renders a per-turn duration sparkline alongside scrollback. A Bevy-native inspector is under evaluation (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` Option F) but would live in a separate binary to keep the headless core deterministic.
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
- Godot inspector consumes the same channel (`overlays.terrain`, `terrain_palette`, `terrain_tag_labels`) to colorize tiles and now drives an interactive drill-down: biome list selection refreshes tag histograms, cached tile samples, and a tile list whose hover/selection reveals per-tile telemetry (coords, terrain, tags, temperature, mass, element id). Culture and Military overlay tabs now render live divergence and readiness heatmaps from the new raster channels, with the panel readouts echoing the map legend (min/avg/max plus normalized severity) so designers can cross-check hotspots without leaving the inspector. `MapView.hex_selected` routes map clicks into `InspectorLayer.focus_tile_from_map` so selecting a hex synchronises biome/tile selection in the Terrain panel. `MapView` now owns navigation: mouse wheel zooms about the cursor, right/middle drag pans, and `W/A/S/D` plus `Q/E` cover keyboard panning/zoom so designers can stay anchored on playback shortcuts. Keep the narrative-and-implementation link tight by updating the manual (§3b) before tweaking palette data here, and log future overlay work in `docs/godot_inspector_plan.md`.
  The **Map** tab consolidates live-map controls: the overlay selector now lives beside a logistics overlay toggle and a note that `Enter` flips terrain shading. A map-size dropdown exposes the curated presets (Tiny 56×36, Small 66×42, Standard 80×52, Large 104×64, Huge 128×80; Standard is the default in `SimulationConfig::default`). Changing the dropdown dispatches a `map_size <width> <height>` command through `Inspector.gd`, which the headless server handles by rebuilding the Bevy `App` with the new grid and broadcasting a fresh snapshot (`core_sim/src/bin/server.rs`). Manual context lives in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` (§3b).
- Colour mapping: `MapView.gd::_terrain_color_for_id` mirrors the hex values listed in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b. Adjust the manual first, then this lookup, then the HUD legend/inspector summaries (see `TASKS.md`).
- Logs tab now consumes the streamed tracing feed over the log socket, rendering a searchable, filterable scrollback (level thresholds, target channels, free-text) alongside the per-turn duration sparkline so designers can audit activity without touching the terminal.
- Commands tab exposes the full command bridge: turn/rollback/autoplay controls plus axis bias adjustment, support/suppress and channel boosts for selected influencers, spawn utilities, corruption injection, and heat debug. Use it to sanity check backend hooks before retiring the CLI.
  - Runtime controls: the thin client binds `ui_accept` to toggle between logistics/sentiment composites and terrain palette mode, aiding QA comparisons of colour accuracy against the documented swatches.
- Inspector migration: see `docs/godot_inspector_plan.md` for the roadmap and progress checkpoints; cross-link new UX notes into the manual when player-facing explanations change. If the Bevy inspector option graduates from evaluation (manual §13 Option F), capture the delta plan here and spin tasks into `TASKS.md`.
  - Logistics/sentiment raster exports now share the terrain grid so overlays blend consistently across clients.

### Frontend Client Strategy
- **Goal**: Select a graphical client stack capable of rendering the live strategy map (zoom/pan, unit animation, layered overlays) while consuming headless snapshots and dogfooding the scripting API.
- **Spikes**: Prioritize a Godot 4 proof-of-concept client that replays mock `WorldDelta` frames and command queues to benchmark frame pacing, overlay rendering cost, and command latency. If Godot exposes blocking gaps, run a focused evaluation of alternative hosts (Avalonia, Qt/QML, Rust+Slint) for visualization only; avoid maintaining a parallel Unity scripting surface.
- **Metrics**: Target ≤16 ms frame time at desktop spec, responsive input-to-command round-trip, and acceptable draw-call budget for layered heatmaps. Capture qualitative notes on tooling ergonomics, asset workflows, and licensing/operational implications. (See `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Map-Centric Evaluation Plan”.)
- **Scripting Surface**: Design a capability-scoped scripting layer once (JS/Lua sandbox managed by the host). Integrate the facade with the Godot spike and publish an engine-agnostic contract so any future host adopts the same manifest; no Unity contingency needs to stay warm.
- **Decision Artifact**: Summarize results in an engineering decision memo that recommends the preferred client, outlines alternative host risks, and queues follow-on work (e.g., WebGPU dashboard, licensing review) before committing to full UX build-out.
- **Resources**: Godot spike scaffolding lives under `clients/godot_thin_client`; see `docs/godot_thin_client_spike.md` for usage and evaluation notes.
- **Networking**: `clients/godot_thin_client/src/scripts/SnapshotStream.gd` consumes length-prefixed FlatBuffers snapshots from `SimulationConfig::snapshot_flat_bind` (`res/src/native` provides the Godot extension that decodes the schema generated from `sim_schema/schemas/snapshot.fbs`).
- **Next Steps (UI plumbing)**:
  - Logistics, sentiment, corruption, fog, culture, and military rasters now stream directly from `core_sim`; `SnapshotHistory` caches the layers and the FlatBuffers schema exposes them for clients alongside terrain overlays.
  - The Godot decoder lifts these rasters into `overlays.channels` with stable keys (`logistics`, `sentiment`, `corruption`, `fog`, `culture`, `military`). `MapView.gd` promotes those channels into a selectable overlay palette (defaulting to logistics blue, sentiment red, corruption amber, fog slate, culture violet, military green) and the inspector injects an option selector so designers can flip layers without touching code. Channel descriptions ship alongside the data and the selector/tooltips surface a concise legend so raw vs. normalized values stay interpretable during reviews.
  - `core_sim::corruption_raster_from_simulation` blends ledger intensities with normalized risk weights per tile: logistics throughput, trade throughput, power demand, and morale-adjusted population size feed the baseline, while active incidents and telemetry spikes inject additional pressure. The resulting `Scalar` values stay in the 0–1 fixed-point range (`Scalar::raw`) and diff cleanly because `SnapshotHistory` still treats the raster as optional.
  - `core_sim::culture_raster_from_layers` projects each local culture layer’s divergence magnitude against its hard schism threshold, applies a small boost for time-above-threshold, and emits the ratio as a 0–1 fixed-point sample. Tiles without a local layer fall back to zero so hotspots stand out for the inspector.
  - `core_sim::military_raster_from_state` fuses morale-weighted cohort size, nearby logistics throughput, and local power margin into a readiness scalar (clamped to ~5). The weighting exposes undersupplied garrisons while keeping values diff-friendly for snapshot deltas.
  - The HUD legend mirrors whichever layer is visible: terrain colouring renders the biome palette, and scalar overlays swap to a low/average/high gradient with live min/avg/max raw values plus the channel description so designers can interpret the heatmap without leaving the map view.
  - `core_sim::fog_raster_from_discoveries` inverts the controlling faction’s knowledge coverage. It averages global discovery progress with local cohort fragments, clamps the composite to `[0, 1]`, and writes “1.0 = fully unknown / 0.0 = fully scouted” samples into the raster. Tiles without a dominant cohort default to opaque fog, making gaps obvious to designers.
  - Inspector overlays now ship normalized, raw, and contrast rasters plus live min/avg/max readouts in the legend; the Culture and Military tabs surface the same stats alongside explanatory copy so designers can validate colour ramps against ledger telemetry without the retired CLI inspector (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Map-Centric Evaluation Plan”).

### Shared Scripting Capability Model
- **Runtime Host**: Embed QuickJS via a GDNative module inside the Godot thin client to execute sandboxed JavaScript. Script packs ship with `manifest.json` (id, version, entrypoint, declared capabilities, optional config schema). Development builds hot-reload files under `addons/shared_scripts/` while packaged builds look in `user://scripts`.
- Manifest contract lives at `docs/scripting_manifest.schema.json` (regenerate with `cargo xtask manifest-schema`). Lint manifests locally via `cargo xtask validate-manifests`, which applies the schema and validates capability/subscription coverage against `sim_runtime::scripting::CapabilitySpec`.
- **Capability Families** (aligned with the manual’s player-facing description):
  - `telemetry.subscribe`: host-managed subscriptions to snapshot feeds (`world.delta`, `overlays.*`, `ledger.discovery`, `log.events`). Tokens encode topic id, optional filters, and sampling rate; the host enforces read-only semantics and per-topic back-pressure (`max_messages_in_flight`).
  - `ui.compose`: declarative widget graph expressed through JS builders that map to Godot controls (`Panel`, `VBox`, `Table`, `Chart2D`, `OverlayLayer`, `MapAnnotation`). Script diffs resolve against stable component ids and render on the main thread.
  - `commands.issue`: vetted command endpoints (turn stepping, axis bias, influencer actions, debug hooks) routed through `sim_runtime::command_bus`. Tokens specify throttle windows (commands per turn) and whether debug-only verbs are available.
  - `storage.session`: scoped key/value cache that persists for a simulation session and travels with save games via a `SimScriptState` blob. No raw disk writes; host exposes `storage.snapshot()` for explicit exports within quota.
  - Optional `alerts.emit`: raise toast/banner notifications; host enforces rate caps and prefixes alerts with script ids for audit.
- **Sandbox Enforcement**:
  - QuickJS contexts are created per script on a worker thread with memory limits (default 16 MB) and an instruction watchdog that yields every 4 ms. Disallowed globals (File, Socket, Thread) are removed; only whitelisted helpers (`host`, `console`, math/time shims) remain.
  - Capability tokens are signed blobs issued during manifest load; runtime APIs validate tokens each call to prevent escalation if scripts exchange references.
  - Violations trigger suspension (`ScriptHost` moves the script to quarantine, unsubscribes telemetry, surfaces error toast) until the player re-enables it.
- **Lifecycle & Tooling**:
  - Manifest parsing occurs at client boot and when players toggle scripts via the forthcoming Script Manager. Errors include actionable hints (missing capability, schema mismatch, syntax failure).
  - Hot reload path uses an esbuild-lite pass (in `tools/script_pipeline`) to bundle modules, then recreates the QuickJS context while preserving session storage when allowed.
  - Logging funnels `console.*` through the Godot Logs tab with per-script channels and stack traces. Structured metrics publish to `log.events` so scripts can introspect their own health.
- **Integration Touchpoints**:
  - `clients/godot_thin_client/src/scripts/scripting/ScriptHost.gd` owns runtime initialisation, capability validation, and bridging to `SnapshotStream`/`CommandBridge`.
  - `sim_runtime::scripting::capability_registry` enumerates capability specs (`telemetry.subscribe`, `commands.issue`, `storage.session`, `alerts.emit`, `ui.compose`) so manifests, hosts, and tooling share a single source of truth.
  - `sim_runtime` exports `CapabilitySpec` definitions so manifests can be linted offline and Rust-side tests ensure topic/command ids stay in sync.
- Save/load flows serialise active scripts (`ScriptManifestRef`) and session payloads via new `SimScriptState` struct so resumes restore contexts before the next snapshot.
  - Godot’s `ScriptHostManager` exposes `capture_state()` / `restore_state()` which wrap `ScriptHostBridge.snapshot_active_scripts` and `apply_script_state`, persisting `SimScriptState` payloads alongside the save game.
- **Verification Plan**:
  - Headless harness (`tools/script_harness`) spins up QuickJS with mock feeds to exercise API contracts and fuzz capability enforcement.
  - Integration tests replay recorded turns and assert scripts cannot exceed the 8 ms per-frame budget; watchdog faults are logged and scripts suspended.
  - Security checklist tracks manifest review, capability coverage, and suspension/resume flows, keeping parity with guarantees in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Shared Scripting Capability Model (Player-Facing)”.

### Trade-Fueled Knowledge Diffusion
- **Data model**: `TradeLinkState` serializes throughput, tariff, and a `TradeLinkKnowledge` payload (`openness`, `leak_timer`, `last_discovery`, `decay`). `PopulationCohortState` now exposes optional `knowledge_fragments:[KnownTechFragment]`, letting migrations ship tacit knowledge. These additions participate in snapshot/delta hashing.
- **Runtime helpers**: `sim_runtime` ships `TradeLeakCurve`, `apply_openness_decay`, `scale_migration_fragments`, and `merge_fragment_payload`, mirroring the fixed-point arithmetic used inside `core_sim` so tooling can project leak cadence without embedding Bevy.
- **Simulation stage**: `trade_knowledge_diffusion` runs after logistics, refreshes throughput/tariff (already reduced by corruption), decrements leak timers, emits `TradeDiffusionEvent`s when timers expire, applies linear research progress to `DiscoveryProgressLedger`, and resets timers via the configured leak curve. Telemetry increments `trade.tech_diffusion_applied` and archives the record for inspector use.
- **Migration flow**: `simulate_population` manages optional `PendingMigration` payloads. When morale/openess align, cohorts snapshot scaled fragments (`migration_fragment_scaling`, `migration_fidelity_floor`) headed to a destination faction. On arrival the fragments merge into the destination ledger, the cohort flips ownership, and telemetry increments `trade.migration_knowledge_transfers` with `via_migration=true`.
- **Configuration surface**: `SimulationConfig` exposes diffusion knobs (`trade_leak_min_ticks`, `trade_leak_max_ticks`, `trade_leak_exponent`, `trade_leak_progress`, `trade_openness_decay`) and migration knobs (`migration_fragment_scaling`, `migration_fidelity_floor`). Designers should cross-reference `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §8 while tuning these values.
- **Telemetry & logging**: `TradeTelemetry` resets each tick, tracks diffusion/migration counts, stores per-event records, and emits `trade.telemetry` log lines after population resolution. Inspector overlays will subscribe directly to these counters.

### Corruption Simulation Backbone
- **Subsystem multipliers**: `CorruptionLedgers::total_intensity` aggregates raw intensity by subsystem. `corruption_multiplier` converts that intensity into a clamped scalar applied by logistics (flow gain/capacity), trade (tariff yield), and military power (net generation), making corruption drag explicit.
- **Config knobs**: `SimulationConfig` adds `corruption_logistics_penalty`, `corruption_trade_penalty`, and `corruption_military_penalty` so balance passes can tune how hard incidents bite. Integration tests confirm corrupted scenarios reduce throughput without breaking determinism.
- **Telemetry coupling**: Exposures still feed sentiment/diplomacy via `CorruptionTelemetry` and `DiplomacyLeverage`; the new multipliers execute in the same tick, so designers see both scandal fallout and economic losses together.

#### Inspector Overlay Prototype Plan
- Gate rendering behind the `trade.tech_diffusion_applied` metric; reuse the Godot inspector snapshot stream to surface openness values per trade link (legacy CLI subscription stays available for verification).
- Start with a map-overlay panel that colorizes trade edges by openness and displays countdowns for active leak timers; use the sentiment heatmap widget as a code reference for gradient rendering.
- Add a secondary list widget showing migration-driven knowledge transfers (source faction, destination faction, tech fragment %, remaining turns) to give designers quick validation feedback.
- Instrument a dedicated Godot input action (e.g., `inspector_toggle_trade_overlay`) to show/hide the overlay without disrupting existing layouts, and keep the legacy CLI key binding for verification runs.

#### Inspector Typography Refactor Plan
- **Shared theme bootstrap**: stand up a `Theme` resource at startup (likely in `Main.gd`) that reads `INSPECTOR_FONT_SIZE`, clamps to the existing min/max, and writes the resolved size into a central typography map (`body`, `heading`, `caption`, `legend`, `control`). Apply the theme to the root `CanvasLayer` so child controls inherit defaults instead of each script hand-wiring overrides.
- **Derived scale registry**: encode offset deltas (e.g., heading = base + 4, caption = base − 2) alongside the theme and expose a light-weight helper (`Typography.gd`) that scripts can import. Replace magic numbers in `Inspector.gd` and `Hud.gd` with lookups so dynamic nodes (terrain legend rows, command dropdown labels, BBCode content) all resolve to the same base + delta.
- **Runtime node adoption**: remove `_apply_font_override` loops in `Inspector.gd` in favor of attaching the shared theme or calling a single helper that tags each control with the correct style name. Ensure runtime-created controls (terrain legend rows, HUD labels, dropdown menu items) grab the themed font via `theme_type_variation` or `add_theme_font_size_override` using the shared constants.
- **Layout healing**: swap the inspector’s absolute top offset (`offset_top = 96`) for a computed margin: read the HUD layer’s combined minimum size (post-theme) or add an API on `HudLayer` that returns the stacked label height + margin, then update `InspectorLayer._update_panel_layout()` and `Main.tscn` defaults to respect that value. While touching layout, replace hard-coded legend row heights (`LEGEND_ROW_HEIGHT`) with `Font.get_height()` derived sizing and audit panels that still assume fixed pixel counts, nudging them toward Containers + size flags.
- **Rich text verification**: confirm `RichTextLabel` widgets honor the base font when fed via theme keys (`default_font_size` vs. `normal_font_size`) on the Godot 4 build we ship. If `RichTextLabel` ignores the theme default, extend the helper to set both keys so BBCode sections stay legible at larger scales. Capture any user-facing typography guidance in the game manual when we start messaging accessibility options.

### Culture Simulation Spine
- **See Also**: `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7c for player-facing framing of culture layers and trait axes.
- **Data Model**: Introduce `CultureLayer` resources scoped at faction (`Global`), region (`RegionalCultureId` keyed to provinces/territories), and settlement (`LocalCultureId`). Each layer stores a normalized trait vector (`CultureTraitVector { axis: [f32; N] }`, N=15 per manual) plus metadata: inheritance weights, divergence tolerance, last recalculated tick.
- **Trait Propagation**: On turn start, `reconcile_culture_layers` copies global baselines downward, then blends with regional/local deltas using configurable elasticity coefficients. Local events (policies, influencer actions, incidents) write deltas into the relevant layer; the reconcile system decays temporary modifiers and accumulates persistent shifts.
- **Divergence Tracking**: Maintain `CultureDivergence` components (per region/local) storing current deviation magnitude, warning thresholds, and time-above-threshold. When deviation crosses soft limits emit `CultureTensionEvent`; hard limits queue `CultureSchismEvent` for faction split/suppression logic. These events feed Sentiment (Agency/Trust axes) and Diplomacy reaction hooks.
- **Trait Effects Bridge**: Convert trait vectors into system-ready coefficients each turn: e.g., `Aggressive` drives `MilitaryStanceBias`, `Open` modifies knowledge leak timers, `Devout` seeds ritual demand for logistics. Implement via `CultureEffectsCache` resource consumed by population, logistics, diplomacy, and espionage systems.
- **Influencer Coupling**: Extend `InfluencerImpacts` (or companion resource) with culture resonance channels. Each influencer publishes weighted deltas onto specific trait axes; the reconcile pass blends those impulses with policy modifiers before divergence calculations.
  - Implemented: `InfluencerImpacts` now carries `InfluencerCultureResonance` (global/regional/local buckets). `InfluentialRoster::recompute_outputs` aggregates per-axis weights based on scope and coherence, clamping to ±1.0 to stay within Scalar bounds. `CultureManager::reconcile` applies those deltas before divergence checks, averaging within scope to avoid order-dependent bias. Inspector pulls the serialized resonance vectors via FlatBuffers (see below).
- **Religion Integration**: Represent sect dynamics as tagged modifiers on the `Devout`, `Mystical`, and `Syncretic` axes rather than a discrete subsystem. High Devout + Mystical regions spawn `RitualSchedule` entries that schedule pilgrimage logistics and sentiment modifiers; secular regions skip creation.
- **Telemetry & UI**: Extend snapshots with `CultureLayerState` payloads (per layer trait vectors + divergence meters) so the Godot inspector’s Culture tab can surface the “Cultural Inspector” referenced in the manual. Provide layer filters and clash forecasts derived from pending `CultureTensionEvent`s.
  - Inspector update: `Influencers` tab now displays each figure’s strongest culture pushes (weight + current output), while the Culture tab aggregates the top scoped pushes (global/regional/local) for quick validation. Snapshot payload exposes `InfluencerCultureResonanceEntry` entries to keep tooling type-safe.
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
- **Commands**: Length-prefixed Protobuf `CommandEnvelope` messages covering verbs such as turn stepping, axis bias, influencer directives, spawning, and corruption injection. `sim_runtime::command_bus` exposes builder/decoder helpers, and the Godot tooling issues structured payloads via the native `CommandBridge` instead of raw strings.
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing` (`turn.completed` now emits `duration_ms` alongside grid metrics for client consumption).

## Power Systems Plan
Power resolution sits fourth in the turn chain (materials → logistics → population → **power** → tick increment → snapshot capture). This section translates the player-facing intent outlined in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §4 into engineering work.

### ECS Structure
- **Identifiers & Resources**
  - `PowerNodeId` (u32) indexes generation sites, substations, storage pools, and demand clusters.
  - `PowerGridState` resource captures the latest per-node supply, demand, transmission loss, storage charge, and stability score.
  - `PowerTopology` holds adjacency lists with per-edge impedance and capacity, seeded during worldgen and mutable via infrastructure projects.
- **Components**
  - `PowerGeneration` (per entity) maps fuel/material inputs to potential output curves with efficiency bands and waste signatures.
  - `PowerDemandProfile` annotates consumer entities (logistics hubs, factories, population blocks, military formations) with baseline draw, surge modifiers, and criticality weights.
  - `PowerStorage` models buffer capacity, charge/discharge efficiency, and bleed rate.
  - `PowerSafety` tracks reactor health, maintenance backlog, and mitigation investments driving instability calculations.
- **Auxiliary Data**
  - `FuelReserve` interfaces with materials/logistics data to ensure generation output honors available feedstock.
  - `GridEventLedger` collects incidents (brownouts, overloads, catastrophic failures) for downstream crisis and knowledge systems.

### Simulation Flow (Power Phase)
1. `collect_generation_orders` resolves facility directives emitted during command processing (fuel assignments, output throttles, maintenance toggles).
2. `resolve_generation` converts materials into produced energy, applying efficiency curves, temperature/terrain modifiers, and downtime for maintenance or damage.
3. `route_energy` propagates supply across `PowerTopology`, accounting for capacity caps, impedance losses, and priority routing; outputs populate per-node surplus/deficit deltas.
4. `apply_storage_buffers` charges or discharges `PowerStorage` entities against node deltas, honoring efficiency/bleed modifiers and spillover to contingency microgrids.
5. `satisfy_demand` decrements consumer queues by delivered energy; unmet demand feeds attrition hooks in logistics, industry, population, and military systems on the following turn.
6. `evaluate_instability` computes stability scores per node, triggers incidents, and records Knowledge Debt adjustments where secrecy constrains workforce familiarity.
7. `export_power_metrics` gathers telemetry (grid stress, surplus margin, instability score, incident feed) into `SimulationMetrics`, snapshot payloads, and overlay rasters.

### Instability Model
- **Stability Bands**: Scores normalised 0–1 combine load ratio, maintenance backlog, reactor health, and redundancy. Thresholds at 0.4 (warn) and 0.2 (critical) drive event probabilities.
- **Incident Types**
  - Brownout/blackout events propagate attrition modifiers to dependent systems and raise unrest.
  - Containment breach incidents inject contamination and heat events into materials/logistics subsystems and flag crisis hooks.
  - Cascading failures traverse `PowerTopology` edges, escalating if redundancy is insufficient.
- **Mitigation Hooks**: `PowerSafety` exposes investments (redundant lines, hardened reactors, microgrids) that increase effective stability; command verbs will toggle these investments and consume resources produced by earlier phases.
- **Knowledge Exposure**: Public deployment of advanced reactors tick leak meters in the Knowledge Diffusion subsystem; incident logs tag technology tiers for reverse-engineering chances.

### Telemetry & Clients
- Extend snapshots with `PowerGridNode` payloads (node id, supply, demand, storage %, stability, active incidents) and `PowerTelemetryState` aggregates (totals, stress/margin, incident feed).
- `SimulationMetrics` gains aggregate values (`grid_stress_avg`, `grid_surplus_margin`, `instability_alerts`) surfaced in the Crisis Dashboard gauges referenced in the manual.
- Godot thin client receives a Power tab extension: grid metrics summary, sortable node list, and incident-aware detail synchronized with the streamed telemetry.
- Headless diagnostics rely on existing script harnesses and tracing exports; add targeted test helpers instead of resurrecting CLI inspectors.

### Integration Points & Dependencies
- **Materials**: Reactor recipes depend on discovered elements/alloys; waste products feed back into materials for recycling/cleanup quests.
- **Logistics**: Fuel routing leverages logistics queues; brownouts inject throughput penalties and additional maintenance cargo demands.
- **Population**: Residential draw scales with prosperity and culture policies; outages influence morale and migration.
- **Military**: High-demand formations reserve power; deficits throttle readiness and advanced weapon availability.
- **Crisis Systems**: Power incidents create triggers for Crisis Framework archetypes (plague/replicator) by disabling containment infrastructure or sparking AI runoff events.

### Follow-on Engineering Tasks
- Finalise schema additions (`PowerGridNode`, power rasters, metrics) in `sim_schema` and `sim_runtime`.
- Implement the ECS systems outlined above inside `core_sim`, ensuring deterministic ordering within the existing schedule.
- Extend Godot inspector to visualise the new telemetry and expose relevant command toggles.
- Author regression tests/benchmarks covering stability band transitions, cascade propagation, and serialization of power telemetry.

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

## Next Steps
- ~~Implement per-faction order submission and turn resolution phases.~~ (Handled via `TurnQueue` + per-faction `order` commands.)
- ~~Persist snapshot history for replays and rollbacks.~~ (Ring-buffered `SnapshotHistory` with `rollback` command.)
- Protobuf `CommandEnvelope` command channel (with host helpers) now handles all control traffic; Godot tooling issues structured requests via the native bridge and the legacy text parser/wire format has been removed. Future protocol work can extend the envelope without reintroducing text compatibility.
