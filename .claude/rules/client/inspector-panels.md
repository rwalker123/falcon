---
paths:
  - "clients/godot_thin_client/src/scripts/Inspector.gd"
  - "clients/godot_thin_client/src/scripts/ui/inspector/**"
---

<!-- Extracted verbatim from lines 152-168;3913-4026 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Inspector panels

> ## ⚠ The Inspector is legacy scaffolding and is expendable
>
> **Standing decision (Ray, 2026-07-26).** It dates from before the client had a real UX and was
> the stand-in for seeing simulation state at all. Many of its tabs need redoing, and it is slated
> for **rework after the performance arc lands** (#386 delta streaming, #392 multi-core, and their
> sub-issues).
>
> **Breaking it in the meantime is acceptable.** Do not let this panel constrain, delay, or
> complicate performance work, and do not invest in optimising code here — it is more likely to be
> replaced than tuned (#390 is parked for exactly this reason). The contracts below describe how it
> works *today* and are worth honouring for correctness while it lives; they are **not** a reason to
> hold back a change elsewhere.

## Key scripts

| Script | Purpose |
|--------|---------|
| `Inspector.gd` | Inspector coordinator: streaming fan-out, capability gating, typography; hosts per-tab panels |
| `ui/inspector/PowerPanel.gd` | Power tab panel — reference for the tab-panel extraction contract (`apply_update`/`reset`) |
| `ui/inspector/CrisisPanel.gd` | Crisis tab panel — adds command hooks (`set_command_hooks`) and `apply_typography` to the contract |
| `ui/inspector/KnowledgePanel.gd` | Knowledge tab panel — adds `set_command_connected` (connection-gating) and `ingest_log_entry` (log-path telemetry), which since issue #381 also parses the `trade.telemetry ` line itself. **`append_events` and the Trade→Knowledge signal seam are GONE** — the Trade tab was retired and this panel was that batch's only consumer, so the parse moved to the one place already being fed every log entry |
| `ui/inspector/SentimentPanel.gd` | Sentiment tab panel — display; axis bias is coordinator-owned and pushed in via `set_axis_bias` |
| `ui/inspector/VictoryPanel.gd` | Victory tab panel — display + one-shot "victory achieved" log via `set_log_hook` |
| `ui/inspector/FaunaPanel.gd` | Fauna tab panel — **display-only** herd list/detail + estimated hunt yields. The follow-herd command it used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a; hunting is now HUD labor allocation), so it issues no command; `set_command_connected` is a contract no-op |
| `ui/inspector/GreatDiscoveriesPanel.gd` | GreatDiscoveries tab panel — large, self-contained (ledger + progress + definition catalog + details); capability-gated (`CAP_MEGAPROJECTS`), no command/log/MapView coupling |
| `ui/inspector/LogsPanel.gd` | Logs tab panel — owns the LogStreamClient + polling + filters + tick sparkline; emits `log_entry_received` (coordinator dispatches to Knowledge); fed synthetic lines via `append_entry`. **Ingest is per entry, render is once per poll.** `_render()` rebuilds the whole `LOG_ENTRY_LIMIT` buffer into one BBCode string and re-shapes it (`set_text` + `get_line_count()`), so calling it from `_record` re-shaped the buffer once per ingested line — ~145 times per turn at `RUST_LOG=info`, measured at **1.9–2.2 s of blocked main thread per turn** (client-wide: `_apply_snapshot` was only 11–31 ms of it). It now renders once in `_poll_stream`'s `if updated:` block beside `_update_sparkline()`; `append_entry` renders its own single line. The INGEST stays per entry — a dropped log line is an accumulator loss, not a re-renderable one |
| `ui/inspector/InfluencerPanel.gd` | Influencers tab panel — owns the influencer roster; capability-gated (`CAP_INDUSTRY_T1`/`T2`) via `set_available`; exposes `aggregate_resonance()` (coordinator feeds it into the Culture tab) and `get_influencers()` (coordinator's still-inline influencer command controls read the roster back). The influencer *command* controls stay coordinator-owned |
| `ui/inspector/CorruptionPanel.gd` | Corruption tab panel — display-only ledger (reputation modifier, audit capacity, incidents); not capability-gated |
| `ui/inspector/CommandsPanel.gd` | Commands tab panel — the designer/debug console (axis-bias, influencer/channel/spawn, corruption inject, heat, config reload, autoplay row, command status/log; the scenario scout/follow rows were removed with the retired single-task commands). Outbound: issues verbs via `set_command_hooks` and logs via the sink; the command transport + autoplay timer + turn-sending stay in the coordinator. Couplings are coordinator-mediated: emits `axis_bias_apply_requested` (coordinator owns `_axis_bias`, pushes back via `set_axis_bias`), `autoplay_toggled`/`autoplay_interval_changed` (coordinator drives the timer, mirrors via `set_autoplay_active`); fed the roster via `set_influencer_roster` and gated via `set_command_connected`. NOT in `_tab_panels` (no snapshot inputs) |
| `ui/inspector/OverlayPanel.gd` | "Map Overlays" section (nested inside the Map tab, attached to `OverlaySection`) — owns the overlay-channel selector (built at runtime), channel metadata, and the culture/military readouts; drives `MapView.set_overlay_channel`. Fed via `set_map_view` + `ingest(overlay_dict, terrain_tag_labels)` (the coordinator re-homes the palette → Terrain and crisis_annotations → Crisis side-routes that share the `overlays` key, and passes Terrain's tag labels since the terrain-tags channel depends on them). NOT in `_tab_panels` |
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the highlight-rivers toggle (now a shader uniform — see Edge Blending → Rivers). Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` **and the trade overlay** via `set_map_view` — the `%LogisticsOverlayToggle` always lived physically under this tab and became this panel's when the Trade tab was retired (issue #381); the per-link selection highlight went with that tab, so the sync pushes an empty link array and only decides whether the overlay draws. The nested Map-Overlays section keeps its own `OverlayPanel` script |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `ui/inspector/TerrainPanel.gd` | Terrain tab panel — the largest: biome list + drill-down, tile list/detail, the runtime terrain-highlight dropdown, and the **Export Map** button (the tile Scout button was retired with the single-task `scout` command). Snapshot-driven (in `_tab_panels`): `apply_update` ingests `tiles`/`tile_updates`/`tile_removed`/`food_modules` and renders. Owns the inbound MapView hex-selection (`focus_tile_from_map`, coordinator forwards) and drives `set_terrain_highlight` / `relative_height_at` via `set_map_view`. The biome palette + tag labels arrive on the `overlays` key (coordinator routes them in via `set_terrain_palette`/`set_terrain_tag_labels`; `get_terrain_tag_labels()` feeds OverlayPanel). Export sends via `set_command_hooks`, gated by `set_command_connected` |
## A hidden Inspector does not render — the contract on `_apply_update`

**The Inspector is hidden by default** (`Main.gd` seats `set_panel_visible(false)` at startup; the
player reopens it with `I`), and `Main` calls `update_snapshot` on **every** snapshot. Before issue
#384 that fan-out ran unconditionally: `_apply_update` walked every `_tab_panels` entry and then
`_render_dynamic_sections()`, costing **~113 ms per turn — 61% of the client's entire
snapshot-apply cost — to render a panel nobody could see.**

The gate lives **inside `Inspector`**, not at the `Main.gd` call site: `Inspector` owns
`_panel_visible` and `set_panel_visible`, so the skip and the show-hook that undoes it sit
together and cannot drift, `update_snapshot` has more than one caller, and part of `_apply_update`
must run while hidden anyway.

Three clauses, and the third is the one that bites:

1. **EVERY frame is skippable — and that is a reversal, recorded here so the old reasoning is not
   re-derived from first principles.** The rule used to be *only a full snapshot is skippable*,
   because a delta described a change against state the panels already held and nothing later could
   reconstruct a dropped one. **Delta streaming (#386) inverted the premise**: the native decoder
   maintains a cached world and republishes it **whole** on every frame — base keys patched from
   each delta's `*_updates` — so a merged delta frame is byte-equivalent to a full snapshot of the
   same state and the NEXT frame reconstructs anything dropped. Self-containment, not payload kind,
   is what the skip ever depended on; now both kinds have it.

   **The skip and the decoder's base-key patching are ONE contract.** If merged frames ever stop
   being complete, this gate silently serves a stale panel. The completeness is pinned by
   `decode_guard`'s section-cache assertions (`.claude/rules/client/native-extension.md`), and it is
   already a client-wide invariant — `MapView.display_snapshot` reads `tiles` / `populations` /
   `culture_layers` straight off the same merged frame every turn. `Main._apply_snapshot` is the
   only live caller and hands the identical dict to `update_delta` and `update_snapshot`, so there
   is no second, thinner producer to worry about. **A partial frame is still unsafe to cache and
   replay — it is simply no longer producible**; if some path starts handing `update_delta` one
   again, revert this skip with it rather than patching around it.

   Measured: the hidden fan-out was 16–30 ms/turn once deltas became the steady-state carrier, ~60 %
   of the client's `apply`. The old gate skipped only full snapshots, which by then arrived once per
   world — so it had quietly stopped working.

   **…but applying a frame does not discharge the catch-up.** `_hidden_snapshot_pending` means
   *a frame has arrived that the panels have not ingested*, so it is set whenever one is skipped and
   cleared only when one is actually fanned out. The first version of this gate cleared it on every
   non-skipped update, hidden deltas included, which broke the sequence **full-while-hidden →
   delta-while-hidden → `I`**: the replay was declared complete and the panels opened holding only
   what the delta carried — precisely the stale-when-opened failure clause 2 exists to prevent. It
   self-heals on the next turn, which is why neither review nor the first guard harness caught it —
   that harness fed no deltas. `inspector_hidden_guard` case 5 pins it, now with THREE distinct
   roster sizes so "catch-up never ran" (3), "catch-up replayed the older frame" (5) and "correct"
   (6) are all distinguishable.

   **The residual loss this used to carry is gone.** While the replay rebuilt panels from the cached
   *full* snapshot, a delta arriving after it was overwritten and its sub-turn changes lost until
   the next full snapshot; the recorded eventual fix was to queue deltas while hidden and replay
   `cached full + queued deltas in order`. Neither is needed: `_cached_snapshot` holds the newest
   frame of either kind, and replaying one merged frame *is* the whole state.
2. **Catch up on show.** `set_panel_visible(false→true)` replays the cached latest snapshot. The
   cache holds it **by reference** — deep-copying would cost exactly the work the gate saves, and
   the decoder builds a fresh tree per frame that no consumer mutates.
3. **Anything ACCUMULATING must stay above the gate.** The test is not "is it cheap", it is **"is
   it reconstructible from the next full snapshot?"** Panel state is a rebuild-from-keys and so
   survives being skipped. `_ingest_command_events` is not: those are per-turn *events*, and a
   dropped one is gone from the running log forever. It runs unconditionally, and replay is safe
   because `_seen_command_events` dedupes.

   Adding anything to `_apply_update` means answering that question for it. Near-misses that are
   safe, each checked rather than assumed: `VictoryPanel._log_victory` is edge-triggered but on
   *persistent* state that rides every snapshot; `CulturePanel._log_new_culture_tensions` runs only
   on the delta branch, which is never gated; `KnowledgePanel.append_events` accumulates but hangs
   off `ingest_log_entry`, the log-stream path, not this one.

`tools/inspector_hidden_guard.gd` pins all of it (see `test-harnesses.md`) — the property is
invisible in normal play, since a stale-when-opened Inspector looks like a panel that just hasn't
updated yet.

## Inspector Panels

See `docs/godot_inspector_plan.md` for full roadmap.

| Tab | Purpose |
|-----|---------|
| Map | Overlay selector, logistics toggle, map size dropdown, Generate Map button |
| Terrain | Full biome histogram, tag histograms, tile drill-down, terrain-type highlight dropdown, **Export Map** button |
| Fauna | Herd registry + density telemetry (display-only; follow-herd command retired) |
| Culture | Layer trait vectors, divergence meters, resonance pushes |
| Military | Readiness heatmaps, cohort summaries |
| Power | Grid metrics, node list, incident feed |
| Crisis | Dashboard gauges, modifier tray, event log |
| Knowledge | Ledger overview, timeline graph, espionage mission queue, trade-diffusion events |
| Logs | Streaming tracing feed, level/target/text filters, duration sparkline |
| Commands | Turn/rollback/autoplay, axis bias, spawn utilities, debug hooks |

**Capability gating** (`Inspector._apply_capability_gating`): most tabs enable only when the matching `CapabilityFlags` bit is set. **Terrain is exempt** — it is an always-available inspection tab with no capability-gated actions (the former Found Camp action + its CAP_CONSTRUCTION gate were removed with the retired `found_camp` command). **Migrated tab panels don't grey out** — instead of disabling the tab (confusing: a dead tab with no explanation), the coordinator calls `panel.set_available(has_flag)` and the panel stays clickable, rendering a "🔒 Locked — unlocks via …" message while gated (see `PowerPanel`). `_set_tab_enabled` is still used for tabs not yet migrated to the panel contract. Its **terrain-type highlight** dropdown lists every defined terrain (via `TerrainDefinitions`), and selecting one calls `MapView.set_terrain_highlight(id)`, which outlines/tints all matching hexes map-wide (ignoring Fog of War) — handy for spotting a biome or confirming one is absent. Selecting "none" (`-1`) clears it.

The overview text draws a **full biome histogram** (`_render_terrain` → `_histogram_bar`): every present biome, sorted by count, with a monospace `[code]` bar scaled to the most common biome plus its tile count and percentage — all computed client-side from the streamed `_terrain_counts`. The **Export Map** button (`_on_export_map_button_pressed`) sends the fire-and-forget `export_map` runtime command; the server writes the current map (terrain snapshot + resolved seed) to its `exports/` scratch dir as JSON (see `sim_schema` `MapExport`). Tile coordinates shown here as `@x,y` (`_format_tile_coords`) index straight into the export's row-major samples, so the same coordinate names a hex in the client, in the export file, and in tests.

### Tab-panel extraction pattern

`Inspector.gd` is being decomposed from a single god-object into per-tab panels;
`Inspector` stays the **coordinator** (streaming, capability gating, typography,
reserved-width/resize) and forwards each update to the tab panels. A tab panel:

- Is a script attached to the tab's own scene node (its `class_name` typed by the
  node's base type — the Power tab is a `ScrollContainer`, so `PowerInspectorPanel
  extends ScrollContainer`). References its widgets by `%UniqueName` (mark those
  nodes `unique_name_in_owner` in `InspectorLayer.tscn`) and wires its own signals
  in `_ready()`. Same model as the pre-existing `scripting/ScriptManagerPanel`.
- Implements the coordinator contract: `apply_update(data: Dictionary,
  full_snapshot: bool)` — the panel reads only the snapshot/delta keys it owns and
  re-renders itself — and `reset()` — drop all panel state so the coordinator can
  re-seed it from a clean slate. `Inspector._apply_update` forwards to
  `panel.apply_update(...)`; `_render_static_sections` calls `panel.reset()` (today
  only on init; it is the hook a future disconnect/full-reinit flow would call). The panel owns its schema keys,
  state, and rendering; the coordinator knows none of them. Panels needing extra
  collaborators add setters (as `ScriptManagerPanel` does with `set_manager()`).
- Capability-gated panels also implement `set_available(available: bool)` — the
  coordinator maps the `CapabilityFlags` bit to it in `_apply_capability_gating`,
  and the panel renders a locked explanation while unavailable (the tab is *not*
  disabled). Always-on tabs (e.g. Terrain) skip this.

Optional contract hooks a panel adds only if it needs them:
- `apply_typography()` — the coordinator's `apply_typography()` calls it so the
  panel styles its own widgets (`CrisisPanel`). `Typography.gd` is currently a
  no-op stub, so this has no visual effect yet — it preserves intent for when
  typography is implemented.
- Collaborator setters for cross-cutting dependencies, kept narrow: `set_map_view`
  (overlay sync), `set_command_hooks(send: Callable, append_log: Callable)` for
  tabs that issue runtime commands (`CrisisPanel` spawn/auto-seed, `KnowledgePanel`
  policy/budget/mission). The panel never reaches back into the coordinator — it
  holds only the Callables/handles it is given.
- `set_command_connected(connected: bool)` — for tabs whose command controls
  enable/disable on the command socket state (`KnowledgePanel`). The coordinator's
  `_update_command_controls_enabled` delegates the panel's own controls to this.
- `ingest_log_entry(entry: Dictionary)` — for tabs fed by parsed *log messages*
  rather than snapshot keys (`KnowledgePanel` knowledge/espionage/counter-intel
  telemetry). The coordinator's log loop calls it per entry.
- Public feeder methods for cross-panel data flow. **The one instance of this is retired**:
  `KnowledgePanel.append_events` was fed by Trade's diffusion records via a
  `knowledge_events_produced` signal the coordinator forwarded. Issue #381 removed the Trade tab,
  and since Knowledge was the batch's only consumer AND already received every log entry through
  `ingest_log_entry`, the parse moved there — deleting the signal, the forward and the feeder.
  Prefer that shape: if the would-be producer and consumer are both already fed the same raw
  stream, parse at the consumer instead of adding a seam.
- Coordinator-owned state pushed into a display panel: `SentimentPanel.set_axis_bias`
  — axis bias belongs to the Commands axis controls (which mutate it optimistically),
  so the coordinator pushes it to the Sentiment view at both the snapshot and the
  optimistic-write sites, instead of the panel owning the key.
- Command-issuing via a signal when the command needs coordinator-only context (pattern
  reference; the Fauna/Terrain examples were retired with the single-task commands — FaunaPanel
  is now display-only and TerrainPanel's Scout button is gone). `set_log_hook(append_log)` is the
  log-only variant of `set_command_hooks` (`VictoryPanel`'s one-shot victory announcement).

The coordinator collects extracted panels in `_tab_panels` and fans `apply_update`
out to them at the **end** of `_apply_update`, after its own key routing (e.g.
`_ingest_overlays`), so a panel's own keys win over coordinator-side feeders on
conflict (see the `crisis_overlay` vs `overlays.crisis_annotations` precedence note).

**Reference implementations:** `ui/inspector/PowerPanel.gd` (Power — pure
snapshot/render), `ui/inspector/CrisisPanel.gd` (Crisis — command hooks +
typography), `ui/inspector/KnowledgePanel.gd` (Knowledge — the fullest: connection
gating and log-path ingestion). **The decomposition is complete** — every inspector tab is
now its own panel (see the key-scripts table). `Inspector.gd` (≈880 lines, down from
~6,500) is purely the coordinator: streaming fan-out, the command hub + autoplay timer,
capability gating, typography, MapView attach, and the cross-panel seams (faction
resolution for Fauna/Terrain, influencer resonance → Culture, the `overlays` fan-out
junction routing palette→Terrain / annotations→Crisis / channels→Overlay).

**Commands tab (designer/debug console).** The `Commands` tab (axis-bias, heat,
config-reload, autoplay row, influencer/corruption command
buttons, command status/log; the scenario scout/follow rows were removed with the retired
single-task commands) is now `CommandsPanel` (see the key-scripts table). Its
subtree once went missing in the 2025-11-21 scene split (`Main.tscn` → instanced
`InspectorLayer.tscn`) and sat dead for months — the coordinator's
`get_node_or_null("RootPanel/TabContainer/Commands/…")` refs silently resolved to
`null` — before it was transplanted back from git history and extracted onto the
tab-panel contract. The **command hub stays in the coordinator**: `_send_command` →
`command_client`, `_ensure_command_connection`, the `autoplay_timer`, and turn-sending
are shared with the turn controls in `RootPanel/CommandToolbar` (outside the
`TabContainer`) and the Terrain tab's Export Map button. The panel issues
verbs through `set_command_hooks` and is connection-gated via `set_command_connected`.
Autoplay is split: the toggle+interval widgets live in the panel (relayed as
`autoplay_toggled`/`autoplay_interval_changed`), while the timer that steps turns and
the toolbar Play/Pause mirroring stay in the coordinator (which calls back
`set_autoplay_active`). Axis bias is coordinator-owned (Sentiment depends on it): the
panel emits `axis_bias_apply_requested` and the coordinator sends + mirrors it back via
`set_axis_bias`. The influencer dropdown is fed `InfluencerPanel.get_influencers()`
through the coordinator (`set_influencer_roster`).

---

