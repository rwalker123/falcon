# sim_runtime

Shared runtime utilities that sit between the pure data contracts in `sim_schema`
and the full ECS-driven simulation in `core_sim`. This crate re-exports schema
structures and will gradually accumulate helper functions, validation routines,
and shared logic required by tools (CLI inspector, integration tests) that need
more than the raw data definitions but less than the full Bevy runtime.
