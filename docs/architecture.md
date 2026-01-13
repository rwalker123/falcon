# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas, emits structured tracing/log frames, and receives control commands. Commands flow over a single length-prefixed Protobuf `CommandEnvelope` socket (`SimulationConfig::command_bind`), while snapshots broadcast on `SimulationConfig::snapshot_bind` / `snapshot_flat_bind` and logs on `SimulationConfig::log_bind`.
- **Simulation Defaults**: `core_sim/src/data/simulation_config.json` seeds `SimulationConfig` with map dimensions, environmental tuning, trade/power/corruption multipliers, migration knobs, and the default TCP bind addresses/snapshot history depth. Designers can edit these baselines (grid size, mass bounds, leak curve, corruption penalties, networking ports) without touching Rust; the loader converts floats to fixed-point `Scalar` values on startup.
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_schema::schemas/snapshot.fbs` for cross-language clients.
- **Shared Runtime (`sim_runtime`)**: Lightweight helpers (command parsing, bias handling, validation) shared by tooling and the headless core.
- **Inspector Client (`clients/godot_thin_client`)**: Godot thin client that renders the map, streams snapshots, and exposes the tabbed inspector; the Logs tab subscribes to the tracing feed, offers level/target/text filters, and renders a per-turn duration sparkline alongside scrollback. A Bevy-native inspector is under evaluation (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` Option F) but would live in a separate binary to keep the headless core deterministic.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

### Brand & Campaign Labels
- Working marketing label: "Trail Sovereigns" for the late-forager nomadic campaign described in the manual (§2a). Engineering keeps `ShadowScale` identifiers in code/assets until a rename decision lands.
- UI copy: surface "Trail Sovereigns" in client shells, campaign selection, and marketing strings loaded from localization tables; treat as data so alternating labels are possible without rebuilds.
- Implementation status: `core_sim` now loads `core_sim/src/data/start_profiles.json`, stores campaign label text/keys in a `CampaignLabel` resource, and serializes them via the new `SnapshotHeader.campaignLabel` FlatBuffer field consumed by `clients/godot_thin_client`.

---

## Subsystem Documentation

For detailed implementation documentation, see the subsystem-specific CLAUDE.md files:

### Simulation Engine (`core_sim/CLAUDE.md`)
- World Generation Pipeline (map builder, terrain, hydrology, biomes)
- Ecosystem Food Modules
- Campaign Loop & System Activation (start flow, capability flags, victory engine)
- Turn Loop & Phases
- ECS Systems (Power, Crisis, Culture, Knowledge/Espionage, Great Discoveries, Visibility/FoW)
- Trade-Fueled Knowledge Diffusion
- Snapshot History & Rollback

### Godot Client (`clients/godot_thin_client/CLAUDE.md`)
- Heightfield Rendering (3D relief visualization)
- Inspector Panels (Map, Terrain, Fauna, Culture, Military, Power, Crisis, Knowledge, Logs, Commands)
- Overlay Channels (logistics, sentiment, corruption, fog, culture, military, visibility/FoW)
- Typography & Theming
- Scripting Capability Model (QuickJS sandbox, capability families)
- Script Distribution & Trust Model

---

## Data Flow
- **Snapshots**: Binary `bincode` frames prefixed with length for streaming.
- **FlatBuffers**: Schema mirrors Rust structs for alternate clients.
- **Logs**: Length-prefixed JSON frames carrying `tracing` events published via the log stream socket (default `tcp://127.0.0.1:41003`).
- **Commands**: Length-prefixed Protobuf `CommandEnvelope` messages covering verbs such as turn stepping, axis bias, influencer directives, spawning, and corruption injection. `sim_runtime::command_bus` exposes builder/decoder helpers, and the Godot tooling issues structured payloads via the native `CommandBridge` instead of raw strings.
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing` (`turn.completed` now emits `duration_ms` alongside grid metrics for client consumption).

---

## Configuration (Map Presets)
`core_sim/src/data/map_presets.json` adds knobs for physically coherent coasts and biomes:
- `macro_land`: `{ continents, min_area, target_land_pct, jitter }`
- `shelf`: `{ width_tiles, slope_width_tiles }`
- `islands`: `{ continental_density, oceanic_density, fringing_shelf_width, min_distance_from_continent }`
- `inland_sea`: `{ min_area, merge_strait_width }`
- `ocean`: `{ ridge_density, ridge_amplitude }`
- `biomes`: `{ orographic_strength, transition_width, band_profile, coastal_rainfall_decay, interior_aridity_strength }`
- `mountains`: `{ belt_width_tiles, fold_strength, fault_line_count, fault_strength, volcanic_arc_chance, volcanic_chain_length, volcanic_strength, plateau_density }`

See `core_sim/CLAUDE.md` for full world generation pipeline details.

---

## Validation & Debug
- Invariants logged at startup (target `shadow_scale::mapgen`):
  - Every `ContinentalShelf` tile lies within `shelf.width_tiles` of land.
  - No `InlandSea` touches `DeepOcean` (unless merged via a strait).
  - Detached shelf tile count (should be 0 for contiguous coasts).
- Metrics: counts of land, shelf, slope, abyss, inland tiles are emitted for quick inspection.

---

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

---

## Next Steps
- ~~Implement per-faction order submission and turn resolution phases.~~ (Handled via `TurnQueue` + per-faction `order` commands.)
- ~~Persist snapshot history for replays and rollbacks.~~ (Ring-buffered `SnapshotHistory` with `rollback` command.)
- Protobuf `CommandEnvelope` command channel (with host helpers) now handles all control traffic; Godot tooling issues structured requests via the native bridge and the legacy text parser/wire format has been removed. Future protocol work can extend the envelope without reintroducing text compatibility.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `shadow_scale_strategy_game_concept_technical_plan_v_0.md` | Authoritative game manual (player-facing systems) |
| `core_sim/CLAUDE.md` | Simulation engine implementation details |
| `clients/godot_thin_client/CLAUDE.md` | Godot client implementation details |
| `sim_schema/README.md` | FlatBuffers schema contracts |
| `sim_runtime/README.md` | Shared runtime utilities |
| `docs/godot_inspector_plan.md` | Inspector migration progress |
| `TASKS.md` | Engineering backlog |
