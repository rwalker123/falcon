# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas and receives text commands (`turn N`, `heat entity delta`).
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_proto::schemas/snapshot.fbs` for cross-language clients.
- **Inspector Client (`cli_inspector`)**: Ratatui TUI fed by snapshot stream; issues commands with keyboard shortcuts.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

## Turn Loop
```text
player orders -> command server -> run_turn -> snapshot -> broadcaster -> clients
```
- Commands are processed synchronously before `run_turn`.
- Each turn emits structured logs (`turn.completed`) with aggregate metrics.
- Frontends may queue multiple `turn` commands (e.g., advance 10 turns).

## Data Flow
- **Snapshots**: Binary `bincode` frames prefixed with length for streaming.
- **FlatBuffers**: Schema mirrors Rust structs for alternate clients.
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing`.

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

## Next Steps
- Implement per-faction order submission and turn resolution phases.
- Persist snapshot history for replays and rollbacks.
- Replace text commands with protocol buffers or JSON-RPC once control surface stabilizes.
