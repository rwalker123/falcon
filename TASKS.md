# Prototype Task List

## Core Simulation (`core_sim`)
- [x] Flesh out deterministic ECS systems (materials, logistics, population).
- [x] Replace placeholder system with staged schedules and fixed-point math.
- [x] Add snapshot/delta serialization hooks feeding `sim_proto` schemas.

## Serialization & Protocol (`sim_proto`)
- [x] Define FlatBuffers schema for snapshots and deltas.
- [x] Implement hash calculation for determinism validation.
- [x] Provide serde-compatible adapters for early testing.

## CLI Inspector (`cli_inspector`)
- [x] Connect to headless sim via TCP/WebSocket stub.
- [x] Render entity/resource dashboards using `ratatui`.
- [x] Add command palette to pause/step sim and mutate components.

### Sentiment Sphere Enhancements
- [x] Implement quadrant heatmap widget with vector overlay and legend (Owner: Mira, Estimate: 2d).
- [x] Surface axis driver diagnostics listing top contributors per tick (Owner: Ravi, Estimate: 1.5d).
- [x] Integrate demographic snapshot panel tied to population cohorts/workforce (Owner: Elena, Estimate: 2d).
- [x] Extend event log to annotate sentiment-shifting actions with axis deltas (Owner: Omar, Estimate: 1d).
- [x] Wire axis bias editing and playback controls into command palette (Owner: Jun, Estimate: 1.5d).

## Tooling & Tests
- [x] Add determinism regression test comparing dual runs.
- [x] Introduce benchmark harness for 10k/50k/100k entities.
- [x] Integrate tracing/tracing-subscriber metrics dump accessible via CLI.

## Core Simulation Roadmap
- [x] Implement per-faction order submission and turn resolution phases (Owner: Sam, Estimate: 4d).
- [x] Persist snapshot history for replays and rollbacks (Owner: Devi, Estimate: 3d).
- [ ] Replace text command channel with protobuf or JSON-RPC once control surface stabilizes (Owner: Leo, Estimate: 2d).

## Documentation
- [x] Document workflow and architecture decisions in `/docs`.
- [x] Capture integration guide for frontend clients (API schema draft).
- [x] Write developer ergonomics survey template for week 2 milestone.
