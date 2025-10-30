# Style and Conventions
- Rust code follows standard rustfmt formatting and clippy cleanliness; pre-commit hooks enforce `cargo fmt` and `cargo clippy`.
- Simulation modules lean on Bevy ECS patterns: systems registered in fixed schedules, resources typed structures, events for cross-system communication. Prefer declarative structs with serde/FlatBuffers derives aligned to `sim_schema` to keep serialization in sync.
- Documentation flow: gameplay narrative lives in `shadow_scale_strategy_game_concept_technical_plan_v_0.md`; engineering plans belong in `docs/architecture.md`; actionable work items must land in `TASKS.md`.
- Telemetry/logging uses the `tracing` crate with structured targets; new channels should integrate with existing sockets and Godot inspector expectations.