# Agent Collaboration Guide

This repository uses two top-level design documents:

- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — authoritative game manual. This weaves narrative, player-facing systems, and the intended gameplay experience. Whenever design ideas mature into player explanation, update this manual.
- `docs/architecture.md` — implementation playbook. Document engineering choices, subsystem decomposition, and items that feed directly into `TASKS.md`.

### When updating documents
- Add new concepts first to the manual if they affect gameplay communication.
- Mirror actionable engineering work in `docs/architecture.md`, extracting concrete tasks into `TASKS.md`.
- Cross-link between documents when gameplay description references technical constraints and vice versa.

### PR expectations for agents
- Mention in summaries which document(s) were touched and why (manual vs architecture).
- Verify narrative additions remain consistent with implementation notes.

Stay consistent with this flow to keep design intent and engineering execution aligned.

### UI Panel Sizing
- Reuse the shared helper at `clients/godot_thin_client/src/scripts/ui/AutoSizingPanel.gd` for any HUD panels that need to expand to fit content (e.g., selection panel, command feed, future hex-info widgets). Attach the script to the panel node and call `fit_to_content` from the owning script; this prevents each panel from reimplementing bespoke height/scroll logic.
