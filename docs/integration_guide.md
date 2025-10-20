# Frontend Integration Guide

This guide describes how an external client can integrate with the Shadow-Scale
prototype to visualize state or issue turn commands.

## Ports & Protocols
- **Snapshot Stream (bincode)**: `tcp://127.0.0.1:41000` (configurable via `SimulationConfig::snapshot_bind`).
  - Frames are `[u32 length][payload bytes]` encoded with `bincode` using the
    structures from `sim_schema::src/lib.rs`. Runtime helpers such as axis bias transforms live in `sim_runtime`.
  - Consumers should read 4-byte little-endian length, then deserialize into
    `WorldDelta` (Rust).
- **Snapshot Stream (FlatBuffers)**: `tcp://127.0.0.1:41002` (configurable via `SimulationConfig::snapshot_flat_bind`).
  - Frames share the same length prefix but payloads are FlatBuffers envelopes matching `sim_schema/schemas/snapshot.fbs`.
  - Non-Rust clients should prefer this stream to avoid pulling in `bincode` and serde dependencies.
- **Command Port**: `tcp://127.0.0.1:41001` (configurable via `SimulationConfig::command_bind`).
  - Plain-text commands terminated by `\n`.
  - Supported commands:
    - `turn <n>` – advances `n` turns (default 1).
    - `heat <entity_bits> <delta>` – adjusts tile temperature by fixed-point delta.
    - `bias <axis_index> <value>` – sets the authoritative sentiment axis bias (value clamped to [-1.0, 1.0]).
  - Future commands will follow the same `verb args` pattern; unrecognized
    commands return no response but are logged.

## Data Contract
- See `sim_schema/schemas/snapshot.fbs` for the FlatBuffers schema equivalent to the Rust structs.
- Fixed-point values (`mass`, `temperature`, etc.) use a scale of 1e-6.
- Entities are encoded as `u64` `Entity::to_bits()` values; clients must map them to meaningful identifiers if needed.

## Client Workflow
1. Open command connection, send desired turn count or control commands.
2. Connect to snapshot stream, consume deltas. Apply to your local model.
3. Optionally, resubscribe after dropped connections; server supports multiple snapshot clients.
4. Use log metrics (see `docs/metrics.md`) to monitor progress.

## Error Handling
- Snapshot TCP stream may close if the server restarts; clients should auto-reconnect.
- Command port is stateless; each command opens a new TCP connection in CLI and server expects short-lived sessions.
- Invalid commands are ignored with a warning logged server-side.

## Testing
- Run local server: `cargo run -p core_sim --bin server`.
- Example using `netcat` to advance turns:
  ```bash
  printf "turn 5\n" | nc 127.0.0.1 41001
  ```
- Example using `nc` to inspect snapshots:
  ```bash
  nc 127.0.0.1 41002 | hexdump -C
  ```
  (use your own parser for real clients.)

## Next Steps
- Formalize a binary command protocol (e.g., FlatBuffers/JSON-RPC).
- Add authentication/control for multi-user clients.
- Provide pagination/resync endpoints for historical snapshots.
