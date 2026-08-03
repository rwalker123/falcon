# Frontend Integration Guide

This guide describes how an external client can integrate with the Shadow-Scale
prototype to visualize state or issue turn commands.

## Ports & Protocols
- **Snapshot Stream (FlatBuffers)**: `tcp://127.0.0.1:41002` (configurable via `SimulationConfig::snapshot_flat_bind`).
  - Frames are `[u32 length][payload bytes]`; payloads are FlatBuffers envelopes matching `sim_schema/schemas/snapshot.fbs`.
  - Read the 4-byte little-endian length, then the envelope — a **full snapshot** for a world's first frame (and after a rollback or a `resync`), a **delta** every turn after it.
  - **This is the only snapshot stream.** A second socket on `41000` used to carry the same world bincode-encoded; it was retired in #388 along with its port (base+0 is now reserved, not rebound), because nothing consumed it and its frames were not in fact decodable — `serde`'s `skip_serializing_if` omits fields that bincode, being non-self-describing, still expects on the way back in.
- **Command Port**: `tcp://127.0.0.1:41001` (configurable via `SimulationConfig::command_bind`).
  - Frames follow the same `[u32 length][payload bytes]` pattern, but the payload is a Protobuf `CommandEnvelope` (`sim_runtime/proto/command.proto`).
  - Supported verbs map to the envelope's `oneof` cases (`turn`, `new_game`, `reset_map`, `order`, `rollback`, `assign_labor`, `move_band`, `reload_config`, …). `sim_runtime::COMMAND_VERBS` is the canonical list — it is what `cargo xtask command --help` prints.
  - **Some `oneof` field numbers are `reserved` and must never be reused**: 24 and 30 (`found_camp`, `domesticate`), and 2, 5, 6, 7, 8, 9, 10 — the retired debug pokes (`heat`, `axis_bias`, `support_influencer`, `suppress_influencer`, `support_channel`, `spawn_influencer`, `inject_corruption`). The systems behind those pokes — tile temperature, axis bias, the influencer roster and the corruption ledger — are still simulated and still published on the snapshot stream; only the manual injection went.
  - Use the helpers in `sim_runtime::commands` (Rust) or the Godot `CommandBridge` GDExtension to build and send envelopes; clients that cannot link against those helpers should mirror the schema directly.
- **Log Stream (tracing JSON)**: `tcp://127.0.0.1:41003` (configurable via `SimulationConfig::log_bind`).
  - Frames follow the same 4-byte little-endian length prefix as snapshot streams.
  - Payloads are JSON objects emitted from `tracing`, e.g. `{ "timestamp_ms": 1700000000000, "level": "INFO", "message": "turn.completed", "fields": { "turn": 42, "duration_ms": 11.8 } }`.
  - Clients can surface these events directly or derive telemetry (recent turn durations, command audit trail) without polling the snapshot stream.

## Data Contract
- See `sim_schema/schemas/snapshot.fbs` for the FlatBuffers schema equivalent to the Rust structs.
- Fixed-point values (`mass`, `temperature`, etc.) use a scale of 1e-6.
- Entities are encoded as `u64` `Entity::to_bits()` values; clients must map them to meaningful identifiers if needed.

## Client Workflow
1. Build a `CommandEnvelope`, open a command connection, and send the `[length][payload]` frame.
2. Connect to snapshot stream, consume deltas. Apply to your local model.
3. Optionally, resubscribe after dropped connections; server supports multiple snapshot clients. A reconnecting client is sent nothing until the next publication — send `resync` to ask for a full frame.
4. Subscribe to the log stream when you need structured tracing output (turn completion metrics, command acknowledgements) without parsing snapshots.

## Error Handling
- Snapshot TCP stream may close if the server restarts; clients should auto-reconnect.
- Command port is stateless; each command connection sends one framed envelope and then closes.
- Invalid commands are ignored with a warning logged server-side.

## Testing
- Run local server: `cargo run -p core_sim --bin server`.
- Example (Rust) issuing a `turn` command:
  ```rust
  use std::io::Write;
  use sim_runtime::{CommandEnvelope, CommandPayload};

  fn main() -> std::io::Result<()> {
      let envelope = CommandEnvelope {
          payload: CommandPayload::Turn { steps: 5 },
          correlation_id: None,
      };
      let bytes = envelope.encode_to_vec().unwrap();
      let mut stream = std::net::TcpStream::connect("127.0.0.1:41001")?;
      stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
      stream.write_all(&bytes)?;
      stream.flush()
  }
  ```
- Example using `nc` to inspect snapshots:
  ```bash
  nc 127.0.0.1 41002 | hexdump -C
  ```
  (use your own parser for real clients.)

## Next Steps
- Expose idiomatic client helpers for other runtimes (TypeScript, Python) atop the Protobuf command schema.
- Add authentication/control for multi-user clients.
- Provide pagination/resync endpoints for historical snapshots.
