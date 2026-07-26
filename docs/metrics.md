# Telemetry & Metrics Overview

Shadow-Scale exposes lightweight metrics via Rust's `tracing` ecosystem using
`tracing-subscriber`. The goal is to let operators and tooling attach JSON
stream subscribers or log viewers to inspect turn-level stats with minimal
runtime cost.

## Server (core_sim::bin::server)

* Uses `tracing_subscriber::fmt()` by default when the `RUST_LOG` env var is
  present (e.g. `RUST_LOG=info,shadow_scale=trace`).
* After each `turn` command, the server logs a `turn.completed` event with:
  - `turn`: sequential counter (u64)
  - `grid_size`: `width` x `height`
  - `total_mass`: sum of tile mass (raw fixed-point i128)
  - `avg_temperature`: float average (f64)
* Additional commands (e.g. `heat`) emit structured `command.applied` events.

> Example log entry (JSON):
>
> ```json
> {"level":"INFO","target":"shadow_scale::server","fields":{"turn.completed":{"turn":12,"grid_width":32,"grid_height":32,"total_mass":123456789,"avg_temp":21.5}}}
> ```

## Godot Inspector

* The Godot thin client subscribes to the structured log socket exposed by the server.
* The Logs tab renders the streamed entries alongside a recent-turn duration sparkline.
* When server-side `RUST_LOG` filters are enabled, the inspector automatically reflects the richer event payloads.

## Custom Subscribers

The `metrics::SimulationMetrics` resource can be swapped or extended to report
into other sinks (Prometheus exporter, gRPC, etc.) by adding additional systems
after `collect_metrics`.

## Running with Logs

```bash
RUST_LOG=info cargo run -p core_sim --bin server
```

Use tools like `tokio-console` or `otlp` subscribers by composing a different
subscriber in `main`. For example:

```rust
let subscriber = tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .finish();
tracing::subscriber::set_global_default(subscriber)?;
```

## Per-phase turn timings

Alongside `turn.completed`, the server emits a `turn.profile` event carrying a
`phases` field: one flat `label=ms` string covering each `TurnStage` plus the
snapshot build, hash, delta diff and each encode. Rationale, how to read it (the
labels nest, and deliberately do **not** sum to the turn duration), and what the
numbers say about where a turn's time goes live in
`.claude/rules/core_sim/turn-profiling.md`.

## Future Work

- Export Prometheus metrics alongside logs if needed.
- Surface metrics through inspector UI overlays.
