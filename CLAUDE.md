# Agent Collaboration Guide

Contributors are expected to follow DRY and SOLID principles to ensure code quality, maintainability, and a strong user experience. Avoid shortcuts and prioritize best practices in both development and design.

## Document Hierarchy

This repository uses a layered documentation structure:

### Design Documents
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — Authoritative game manual. Weaves narrative, player-facing systems, and intended gameplay experience.
- `docs/architecture.md` — System-wide implementation overview. Cross-system data flow, extensibility patterns, and configuration reference.

### Subsystem Documentation
- `core_sim/CLAUDE.md` — Simulation engine: ECS systems, world generation, turn loop, power/crisis/culture/knowledge subsystems
- `clients/godot_thin_client/CLAUDE.md` — Godot inspector: 2D hex map rendering, panels, overlays, scripting capability model
- `sim_schema/README.md` — FlatBuffers schema contracts
- `sim_runtime/README.md` — Shared runtime utilities

### Task Tracking
- `TASKS.md` — Engineering backlog extracted from architecture and manual

---

## When Updating Documents
- Add new concepts first to the **manual** if they affect gameplay communication.
- Add implementation details to the **subsystem CLAUDE.md** files for the relevant directory.
- Keep `docs/architecture.md` focused on cross-system concerns and overview.
- Extract concrete tasks into `TASKS.md`.
- Cross-link between documents when gameplay description references technical constraints and vice versa.

### Cross-linking Convention
- Define authoritative specs in the owning subsystem's CLAUDE.md
- Add "See Also" cross-references in dependent documentation
- Avoid duplicating implementation details across files

---

## PR Expectations for Agents
- Mention in summaries which document(s) were touched and why
- Verify narrative additions remain consistent with implementation notes
- When modifying subsystem code, check if the corresponding CLAUDE.md needs updates

Stay consistent with this flow to keep design intent and engineering execution aligned.

---

## Delegating Implementation to Coder Agents

Long sessions fill the orchestrator's context fast because writing code churns
through file reads, builds, and test output. Two subagents in `.claude/agents/`
absorb that churn — they do the read → edit → build → test loop in their own
context and return only a terse report:

- **`server-dev`** — Rust side (`core_sim`, `sim_runtime`, `sim_schema`, `xtask`).
  Self-verifies with `cargo fmt` + `clippy -D warnings` + `cargo test`.
- **`client-dev`** — Godot/GDScript + native extension (`clients/godot_thin_client`).
  Self-verifies with `cargo xtask godot-build` + the ui_preview PNG harness (it
  reads the rendered frames).

**The workflow:** the orchestrator owns design and produces a *complete,
comprehensive spec* — decided approach, files to touch, contracts, edge cases,
config levers — and the agent just implements it. Design and architecture
decisions stay with the orchestrator; only settled specs are delegated. Do **not**
hand an agent an open-ended or ambiguous task — if the spec isn't complete enough
to implement without further design judgment, it isn't ready to delegate.

Guidance:
- Split cross-cutting work: `server-dev` does the schema/sim half, `client-dev`
  consumes it; each flags the other's remaining work in its report.
- Continue the *same* agent (via SendMessage) for iterative follow-ups so its
  context persists, rather than cold-starting a fresh one and re-explaining.
- Agents don't branch or commit — they leave the tree changed and report; git
  stays with the orchestrator.

---

## UI Panel Sizing
Reuse the shared helper at `clients/godot_thin_client/src/scripts/ui/AutoSizingPanel.gd` for any HUD panels that need to expand to fit content (e.g., selection panel, command feed, future hex-info widgets). Attach the script to the panel node and call `fit_to_content` from the owning script; this prevents each panel from reimplementing bespoke height/scroll logic.
