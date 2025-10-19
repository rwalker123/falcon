# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas and receives text commands (`turn N`, `heat entity delta`).
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_proto::schemas/snapshot.fbs` for cross-language clients.
- **Inspector Client (`cli_inspector`)**: Ratatui TUI fed by snapshot stream; issues commands with keyboard shortcuts.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

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
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing`.

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

## Next Steps
- ~~Implement per-faction order submission and turn resolution phases.~~ (Handled via `TurnQueue` + per-faction `order` commands.)
- ~~Persist snapshot history for replays and rollbacks.~~ (Ring-buffered `SnapshotHistory` with `rollback` command.)
- Replace text commands with protocol buffers or JSON-RPC once control surface stabilizes.
