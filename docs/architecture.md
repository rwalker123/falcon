# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in `TurnStage` order — Influence → Logistics → Knowledge → GreatDiscovery → Population → Visibility → Crisis → Telling → Finalize → Victory → Snapshot (`core_sim/src/lib.rs`; `core_sim/CLAUDE.md` is the doc-side authority).
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas, emits structured tracing/log frames, and receives control commands. Commands flow over a single length-prefixed Protobuf `CommandEnvelope` socket (`SimulationConfig::command_bind`), while snapshots broadcast on `SimulationConfig::snapshot_flat_bind` (the one snapshot socket since #388) and logs on `SimulationConfig::log_bind`.
- **Simulation Defaults**: `core_sim/src/data/simulation_config.json` seeds `SimulationConfig` with map dimensions, environmental tuning, trade/power/corruption multipliers, migration knobs, and the default TCP bind addresses/snapshot history depth. Designers can edit these baselines (grid size, mass bounds, leak curve, corruption penalties, networking ports) without touching Rust; the loader converts floats to fixed-point `Scalar` values on startup.
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_schema::schemas/snapshot.fbs` for cross-language clients.
- **Shared Runtime (`sim_runtime`)**: Lightweight helpers (command parsing, bias handling, validation) shared by tooling and the headless core.
- **Inspector Client (`clients/godot_thin_client`)**: Godot thin client that renders the map, streams snapshots, and exposes the tabbed inspector; the Logs tab subscribes to the tracing feed, offers level/target/text filters, and renders a per-turn duration sparkline alongside scrollback. A Bevy-native inspector is under evaluation (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` Option F) but would live in a separate binary to keep the headless core deterministic.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

### Brand & Campaign Labels
- Working marketing label: "Trail Sovereigns" for the late-forager nomadic campaign described in the manual (§2a). Engineering keeps `ShadowScale` identifiers in code/assets until a rename decision lands.
- UI copy: campaign and marketing strings are loaded from localization tables and treated as data, so alternating labels are possible without rebuilds.
- Implementation status: `core_sim` loads `core_sim/src/data/start_profiles.json`, stores campaign label text/keys in a `CampaignLabel` resource, and serializes them via the `SnapshotHeader.campaignLabel` FlatBuffer field.
- **The HUD does not display the campaign label.** Its one client-side reader is the Inspector's Map tab (`ui/inspector/MapPanel.gd`), beside the start-profile controls the label belongs to. The top-left title block the game shell used to carry was retired: it restated a fixed string on every frame of play and earned none of the screen it took.

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
- 2D Hex Map Rendering (terrain, overlays, minimap) — the client is 2D-only; see "Removed: 3D Relief Rendering" below
- Inspector Panels (Map, Terrain, Fauna, Culture, Military, Power, Crisis, Knowledge, Logs, Commands)
- Overlay Channels (logistics, sentiment, corruption, fog, culture, military, visibility/FoW)
- Typography & Theming
- Scripting Capability Model (QuickJS sandbox, capability families)
- Script Distribution & Trust Model

---

## Removed: 3D Relief Rendering (permanent)

The Godot thin client once carried an experimental **3D heightfield/relief view** (chunked
`ArrayMesh` terrain, `Camera3D` orbit controls, a water plane, and 3D unit markers). It was a
persistent source of instability and was **permanently removed**. The client is **2D-only** and
there is no plan to reintroduce a 3D view.

What was removed:
- Godot 3D scripts, scenes, and spatial shaders (`HeightfieldLayer3D`, `HeightfieldPreview`,
  `WaterLayer3D`, `UnitOverlay3D`, `heightfield.gdshader`, `water.gdshader`).
- The native decode of the relief overlay and the FlatBuffers `ElevationOverlay.normals` field
  (per-vertex normals were only ever consumed by the 3D shader).

What was intentionally kept (it is simulation/2D data, not 3D rendering):
- `core_sim/src/heightfield.rs` `ElevationField` — drives hydrology, mapgen, sea-level, and
  fog-of-war visibility.
- The FlatBuffers `ElevationOverlay.samples` raster and the 2D **Elevation Heatmap** overlay
  channel that renders from it.

---

## Data Flow
- **Snapshots**: Length-prefixed FlatBuffers frames — a full snapshot for a world's first frame, a delta every turn after it. The parallel `bincode` snapshot socket was retired in #388.
- **FlatBuffers**: Schema mirrors Rust structs for alternate clients.
- **Logs**: Length-prefixed JSON frames carrying `tracing` events published via the log stream socket (default `tcp://127.0.0.1:41003`).
- **Commands**: Length-prefixed Protobuf `CommandEnvelope` messages covering verbs such as turn stepping, world setup, band orders and labor assignment, the intensification verbs, espionage and counter-intel, and config hot reload. `sim_runtime::commands` exposes builder/decoder helpers and `sim_runtime::command_text` the text parser behind `cargo xtask command`; the Godot client issues structured payloads via the native `CommandBridge` instead of raw strings. **The envelope carries no debug pokes**: axis bias, influencer support/suppress/spawn, corruption injection and tile heat were hand-injection entry points for systems the sim runs on its own, and went with the Inspector tab that was their only caller — their proto field numbers are `reserved`, never reused.
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing` (`turn.completed` now emits `duration_ms` alongside grid metrics for client consumption).

---

## Seats & Players — who drives a faction

> **Status: partly built.** `docs/plan_multiplayer_seats.md` is the authority; #646 builds it.
> Seats, the claim handshake and the per-seat turn wait are as-built (`core_sim/src/seats.rs`), and
> `sim_ai` (AI-side, below) occupies one. What is described below as "as-built" is built; the rest is
> the decided target.

**The sim knows seats. It never knows who fills one.** A world has N faction seats; a seat is occupied by whatever connection claimed it, or vacant. Human clients, algorithmic AI, an LLM and a test script are all seat occupants, indistinguishable to the server — there is no AI code path and no `is_ai` branch below the socket. Multiplayer is not a layer on top of this; it is this architecture with some processes on other machines.

- **One process per player, including the host's human.** The person who starts the game is a remote player whose process happens to be local. A single code path is the point: an in-thread fast path for the local AI is where a shortcut appears that the socket cannot take, and the AI then plays a different game from the player.
- **The launcher fills local seats.** `launcher/src/main.rs` already supervises the server and client with a ports handshake and a reaping `Drop` guard; N players is the same job with a loop. The **server never spawns players** — it cannot, since remote seats are on other machines.
- **A connection claims a seat at handshake**, and the server thereafter takes the faction from the seat rather than from the wire. *As-built* (`core_sim/src/seats.rs`): a claim mints a `SeatToken`, the greeting presents it, and the server attributes commands to the claimed seat. Before it, every accepted socket shared one `Sender<Command>` and the faction was whatever `faction_id` the client wrote, checked only for existence (`apply_command`) — harmless with one local trusted client, load-bearing with two parties.
- **Waiting is the default; auto-submit is the timeout.** `resolve_turn_with_auto_orders` force-submits an end-turn for every faction still awaited, which is why an AI faction passes forever. Under seats the turn scheduler waits for each occupied seat and auto-submits only past a timeout — *as-built*, that timeout is `seat_turn_timeout_seconds` (120 s by default), and `resolve_turn_with_auto_orders` is the path a vacant or silent seat takes. `TurnQueue` already awaits all factions control-blind, so the pacing model needs no change.
- **One frame per seat, not one broadcast.** `capture_snapshot` reads a single global `ViewerFaction` and `SnapshotServer` sends one frame to every client. Since PR #648 made frames viewer-scoped, a second connected client sees *its own* people as a foreign band. Per-viewer capture is also what makes an AI honest: unseen tiles are not in the bytes it receives, so fog becomes a property of the transport rather than a policy. ⛔ Capture (`snapshot.build`, ~3.16 ms) is ~70% of a ~4.55 ms turn and is the half still **on the turn thread** — hashing, diffing and encoding moved to a publisher thread in #393. So N seats means N captures on the critical path and N diffs/encodes somewhere already parallel; capture-once-then-project-per-viewer-at-publish is the shape that exploits that. See `plan_multiplayer_seats.md` §4.3. (The older "7.6 ms of 8.4 ms" figure predates #393 — do not use it.)
- **Replay and rollback are unaffected.** A seat emits commands rather than mutating the world, so decisions land in the command log (`LogEntry::Command`) and a rollback replays them without re-consulting the occupant. **A non-deterministic occupant — an LLM, a human — costs the determinism suites nothing.** Rollback does need to become host-only, and an occupant needs to react to `Command::Resync`.
- **Occupancy is a session fact, never save state.** A save is a world with N seats; who sat in them is not in `SimState`.

**AI-side** (`docs/plan_ai_opponents.md`): the `sim_ai` binary crate — as-built, one process per rival seat, spawned and supervised by the launcher — depends on `sim_runtime` for the wire types and **not** on `core_sim`, so "the AI may not read the simulation directly" is a build error rather than a review comment. The seat's decoded frame *is* its perception — there is deliberately no second representation of what a faction can see. The layering inside that process, and the instruments that measure each layer, are `docs/plan_ai_driver.md`.

---

## Configuration (Map Presets)
`core_sim/src/data/map_presets.json` adds knobs for physically coherent coasts and biomes:
- `macro_land`: `{ continents, min_area, target_land_pct, jitter }`
- `shelf`: `{ width_tiles, slope_width_tiles }`
- `islands`: `{ continental_density, oceanic_density, fringing_shelf_width, min_distance_from_continent }`
- `ocean`: `{ ridge_density, ridge_amplitude }`
- `biomes`: `{ orographic_strength, transition_width, band_profile, coastal_rainfall_decay, interior_aridity_strength }`
- `mountains`: `{ belt_width_tiles, fold_strength, fault_line_count, fault_strength, volcanic_arc_chance, volcanic_chain_length, volcanic_strength, plateau_density }`

See `core_sim/CLAUDE.md` for full world generation pipeline details.

---

## Validation & Debug
- Invariants logged at startup (target `shadow_scale::mapgen`):
  - Every `ContinentalShelf` tile lies within `shelf.width_tiles` of land.
  - No `InlandSea` touches `DeepOcean`. A lake is water the mask left unconnected to the ocean;
    nothing merges one into the sea (the `inland_sea` strait carver is deleted — see `core_sim/CLAUDE.md`
    → "Lakes are emergent").
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
| `docs/plan_multiplayer_seats.md` | The seat model — connection identity, turn waiting, per-viewer frames, who launches a player |
| `docs/plan_ai_opponents.md` | What fills a seat — brain patterns, personality vectors, the LLM path, difficulty |
| `docs/plan_ai_driver.md` | The AI player process — orchestrator / specialists / arbiter layering, how each layer is measured, the growth procedure |

The engineering **backlog** is not a file — it lives in GitHub Issues + the Falcon Backlog
project (https://github.com/users/rwalker123/projects/2). See root `CLAUDE.md` → Task Tracking.
