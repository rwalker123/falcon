# Task Completion Checklist
- Ensure formatting and linting are clean via `cargo fmt` and `cargo clippy` (or `pre-commit run --all-files`).
- Run targeted `cargo test` (and integration/benchmark suites if relevant to the change) to protect determinism guarantees.
- Update `docs/architecture.md`, `shadow_scale_strategy_game_concept_technical_plan_v_0.md`, and `TASKS.md` whenever gameplay intent, engineering plans, or backlog items shift.
- Verify FlatBuffers or protocol changes are regenerated with `cargo xtask prepare-client` and documented.
- Summarize touched documents/components in final updates and highlight any follow-up tasks or tests left outstanding.