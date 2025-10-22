# Godot Inspector Migration Plan

This document captures the steps required to retire the Ratatui-based CLI inspector and move all
debug tooling into the Godot thin client.

## Goals

- Provide a single visual inspector with panels that mirror (and eventually surpass) the CLI feature
  set.
- Reuse inspector components for future player-facing UX where possible.
- Maintain parity with existing command/control functionality before deprecating the CLI.

## Required Panels & Features

- **Sentiment Suite**
  - Heatmap rendering of sentiment sphere (current CLI heatmap parity).
  - Axis bias list with edit controls (increment/decrement/reset).
  - Driver diagnostics and demographic snapshot.

- **Terrain**
  - Top biomes summary, tag coverage, and the shared palette legend.
  - Hooks for future drill-down (per-biome stats, tile inspection).
  - _Status_: text summary for top biomes + tag coverage now live in the Godot inspector (see `clients/godot_thin_client/src/scripts/Inspector.gd`); drill-down UI still pending.

- **Influential Individuals**
  - Roster table with lifecycle filters, support/suppress values, domain breakdown.
  - Buttons for support, suppress, channel boost, and spawn influencer.

- **Corruption**
  - Active incidents/exposures list.
  - Target selector and injection button for debug workflows.

- **Logs & Recent Activity**
  - Scrollable log panel fed from tracing output.
  - Recent tick summary (sparkline or compact list).
  - _Status_: tracing feed now streams directly into the Logs tab, replacing the delta-summary placeholder. The panel shows structured scrollback plus a per-turn duration sparkline driven by `turn.completed` metrics.

- **Command Console**
  - Text entry for ad-hoc commands identical to CLI support (`turn`, `spawn_influencer`, etc.).
  - Playback controls: manual step, ±10 turns, autoplay toggle with adjustable cadence,
    rollback, heat debug.
  - _Status_: Godot UI now issues axis bias edits, influencer support/suppress/channel boosts, spawn, corruption injection, and heat commands alongside existing turn controls.

## Data & Command Surface

- Extend the Godot snapshot decoder (Rust GDExtension) to surface:
  - Influencer roster updates.
  - Corruption ledger entries/exposures.
  - Sentiment telemetry and demographic aggregates.
  - Terrain overlays (already present) plus future culture/military/logistics rasters.
- Implement a Godot command bridge mirroring `ClientCommand`:
  - Turn advancement, rollback, order submission.
  - Axis bias adjustments.
  - Support/suppress/channel support, influencer spawn.
  - Corruption injection, heat tile debug.
- Forward tracing/log output from the Rust backend into Godot (e.g., via channel or socket).

## UX Considerations

- Organize panels as tabs or collapsible sections to avoid overload on a single screen.
- Provide keyboard shortcuts aligned with legacy CLI controls while exposing mouse-driven UI.
- Keep layouts modular so future systems (culture, military, logistics) slot in without major
  redesign.

## Decommission Process

1. Implement all panels & controls in Godot and verify parity with the CLI inspector.
2. Update documentation/workflows to point designers/devs at the Godot inspector.
3. Remove the `cli_inspector` crate and associated tasks once parity is confirmed.

## Progress Log

- Terrain tab now renders top-biome coverage and tag distribution (text summary). The Logs tab consumes the tracing socket, surfaces structured log scrollback, and plots recent turn durations. Follow-ups: interactive terrain drill-down, sentiment/culture overlays, and richer drill-ins on log metadata (filters, pinning).
- Commands tab implements axis bias tuning, influencer support/suppress/channel boosts, spawn, corruption injection, and heat debug so designers can retire the CLI command surface once backend parity is confirmed.
