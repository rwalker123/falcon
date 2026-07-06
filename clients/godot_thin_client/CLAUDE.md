# Godot Thin Client

Inspector and visualization client for the Shadow-Scale simulation. Renders the map, streams snapshots, and exposes the tabbed inspector.

## Quick Reference

```bash
# Build native extension
cargo xtask godot-build

# Build terrain texture atlas (if out of date)
scripts/build_terrain_textures.sh

# Run client (auto-builds textures if needed)
scripts/run_stack.sh --client-only

# Regenerate FlatBuffers bindings
cargo build -p shadow_scale_flatbuffers && cargo xtask godot-build
```

**Sockets**:
- Snapshot stream: `127.0.0.1:41002` (FlatBuffers via `SimulationConfig::snapshot_flat_bind`)
- Command socket: `127.0.0.1:41001` (Protobuf `CommandEnvelope`)
- Log stream: `127.0.0.1:41003` (length-prefixed JSON tracing frames)

---

## Key Scripts Reference

| Script | Purpose |
|--------|---------|
| `Main.gd` | Scene orchestration, streaming toggle |
| `MapView.gd` | Terrain rendering, overlays, hex selection, navigation (WASD/QE/mouse), 2D minimap |
| `Inspector.gd` | Inspector coordinator: streaming fan-out, capability gating, typography; hosts per-tab panels |
| `ui/inspector/PowerPanel.gd` | Power tab panel — reference for the tab-panel extraction contract (`apply_update`/`reset`) |
| `ui/inspector/CrisisPanel.gd` | Crisis tab panel — adds command hooks (`set_command_hooks`) and `apply_typography` to the contract |
| `ui/inspector/KnowledgePanel.gd` | Knowledge tab panel — adds `set_command_connected` (connection-gating), `ingest_log_entry` (log-path telemetry), and `append_events` (Trade→Knowledge feed) |
| `ui/inspector/TradePanel.gd` | Trade tab panel — `set_map_view` (overlay), owns the Map-tab overlay toggle, and emits `knowledge_events_produced` (the coordinator forwards it to KnowledgePanel — panels stay decoupled) |
| `ui/inspector/SentimentPanel.gd` | Sentiment tab panel — display; axis bias is coordinator-owned and pushed in via `set_axis_bias` |
| `ui/inspector/VictoryPanel.gd` | Victory tab panel — display + one-shot "victory achieved" log via `set_log_hook` |
| `ui/inspector/FaunaPanel.gd` | Fauna tab panel — herd list/detail; emits `follow_herd_requested` + `herd_selected` (coordinator resolves faction, issues command, mirrors the Commands follow field); `set_command_connected` gates the follow button |
| `ui/inspector/GreatDiscoveriesPanel.gd` | GreatDiscoveries tab panel — large, self-contained (ledger + progress + definition catalog + details); capability-gated (`CAP_MEGAPROJECTS`), no command/log/MapView coupling |
| `ui/inspector/LogsPanel.gd` | Logs tab panel — owns the LogStreamClient + polling + filters + tick sparkline; emits `log_entry_received` (coordinator dispatches to Knowledge/Trade); fed synthetic lines via `append_entry` |
| `ui/inspector/InfluencerPanel.gd` | Influencers tab panel — owns the influencer roster; capability-gated (`CAP_INDUSTRY_T1`/`T2`) via `set_available`; exposes `aggregate_resonance()` (coordinator feeds it into the Culture tab) and `get_influencers()` (coordinator's still-inline influencer command controls read the roster back). The influencer *command* controls stay coordinator-owned |
| `ui/inspector/CorruptionPanel.gd` | Corruption tab panel — display-only ledger (reputation modifier, audit capacity, incidents); not capability-gated |
| `ui/inspector/CommandsPanel.gd` | Commands tab panel — the designer/debug console (axis-bias, influencer/channel/spawn, corruption inject, heat, config reload, scenario scout/follow/camp, autoplay row, command status/log). Outbound: issues verbs via `set_command_hooks` and logs via the sink; the command transport + autoplay timer + turn-sending stay in the coordinator. Couplings are coordinator-mediated: emits `axis_bias_apply_requested` (coordinator owns `_axis_bias`, pushes back via `set_axis_bias`), `autoplay_toggled`/`autoplay_interval_changed` (coordinator drives the timer, mirrors via `set_autoplay_active`); fed the roster via `set_influencer_roster` and gated via `set_command_connected`. NOT in `_tab_panels` (no snapshot inputs) |
| `ui/inspector/OverlayPanel.gd` | "Map Overlays" section (nested inside the Map tab, attached to `OverlaySection`) — owns the overlay-channel selector (built at runtime), channel metadata, and the culture/military readouts; drives `MapView.set_overlay_channel`. Fed via `set_map_view` + `ingest(overlay_dict, terrain_tag_labels)` (the coordinator re-homes the palette → Terrain and crisis_annotations → Crisis side-routes that share the `overlays` key, and passes Terrain's tag labels since the terrain-tags channel depends on them). NOT in `_tab_panels` |
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the hydrology rivers toggle. Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` via `set_map_view`. The nested Map-Overlays section keeps its own `OverlayPanel` script |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `Hud.gd` | HUD layer, legend, selection panel, turn readout |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandBridge.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Minimap component for the 2D map view (click-to-pan, aspect ratio sizing) |
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content |
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |

---

## Architecture

### Scene Structure
- `Main.tscn` - Root `Node2D` scene with a `Camera2D`, the `MapView` map layer, and `CanvasLayer`s for HUD/inspector
- The client is **2D-only**; an experimental 3D relief view was permanently removed (see `docs/architecture.md` → "Removed: 3D Relief Rendering")
- Toggle: `I` hides/shows inspector, `L` collapses legend

### Data Flow
```
Server (FlatBuffers) -> SnapshotStream.gd -> parsed snapshot
                                          -> MapView (terrain/overlays)
                                          -> Inspector (panels)
                                          -> Hud (legend, selection)
```

### Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

> **Note:** Elevation is not rendered as 3D relief. A shallow-3D heightfield view was
> prototyped and permanently removed; elevation is surfaced as the 2D **Elevation
> Heatmap** overlay and as a per-tile **Height** readout in the tile panels (the HUD
> selection panel via `MapView._tile_info_at` → `Hud._tile_summary_lines`, and the
> Inspector Terrain tab). All read the same normalized `ElevationOverlay.samples` raster —
> there is no per-tile elevation on `TileState`. **Height is a relative 0..100 indicator**
> (a number + filled/empty bar), NOT meters: it exists so a player can reason about line of
> sight — a higher tile can occlude the tile behind it (matching the LOS raycast in
> `visibility_systems.rs`). `MapView.relative_height_at` rescales the above-sea-level span
> into 0..100 (at/below sea level reads 0, since open water occludes nothing). The sea level
> is the **active map's** `sea_level`, streamed per-snapshot as `ElevationOverlay.seaLevel`
> (pre-normalized server-side to the raster's [min,max] scale) and read into
> `MapView._elevation_sea_level` — no hardcode; `HEIGHT_DEFAULT_SEA_LEVEL` is only the
> pre-first-snapshot fallback. `MapView.format_height` is the single source of truth for the
> number+bar formatting. The
> raster still streams from the core for the heatmap and for gameplay (LOS), but the
> per-vertex `normals` field (3D-only) was dropped from the schema. See
> `docs/architecture.md` → "Removed: 3D Relief Rendering".

---

## Minimap System

The map view displays a minimap in the bottom-right corner showing the full map with a viewport indicator rectangle.

### Component (`ui/MinimapPanel.gd`)
Reusable minimap UI component handling:
- CanvasLayer hierarchy setup (layer 102)
- Aspect ratio sizing from grid dimensions
- Click-to-pan with drag support
- Viewport indicator overlay with draw callbacks

### 2D Minimap (MapView.gd)
- Renders terrain at 1 pixel per hex as an `ImageTexture`
- Viewport indicator uses pointy-top hex coordinate math:
  - Screen corners → axial coords (q,r) → offset coords (col,row) → normalized [0,1]
- Click-to-pan converts normalized position → hex grid coords → pan_offset

### Configuration
Minimap sizing parameters live in `heightfield_config.json` (the file also holds fog-of-war appearance tunables; its 3D-relief sections were removed):
```json
"minimap": {
  "base_height": 220,
  "min_width": 140.0,
  "max_width": 520.0,
  "margin": 16.0
}
```

---

## Terrain Texture System

Optional terrain texture graphics for the 2D map view.

### Asset Structure
```
assets/terrain/
  textures/
    base/                        # 37 terrain textures (512x512 PNG)
      00_deep_ocean.png
      ...
      36_aquifer_ceiling.png
    edges/                       # 6 edge masks for blending (optional)
    wang/                        # Wang tile variants (future)
  terrain_config.json            # Configuration
  TerrainTextureManager.gd       # Autoload singleton for centralized texture loading
  TerrainDefinitions.gd          # Single source of truth for terrain definitions
  TerrainTextureGenerator.gd     # CLI script to generate placeholder textures
```

### Enabling Terrain Textures
1. Generate placeholder textures from command line:
   ```bash
   godot --headless --path clients/godot_thin_client --script assets/terrain/TerrainTextureGenerator.gd
   ```
2. Replace placeholders in `assets/terrain/textures/base/` with AI-generated or hand-crafted textures
3. Set `"use_terrain_textures": true` in `terrain_config.json`

Textures are loaded at runtime from individual PNGs and combined into a `Texture2DArray`.

### Configuration (`terrain_config.json`)
```json
{
  "use_terrain_textures": true,
  "use_edge_blending": true,
  "texture_scale": 4.0,
  "blend_width": 0.15,
  "lod_near_distance": 50.0,
  "lod_far_distance": 200.0
}
```

### Texture Loading (TerrainTextureManager)
- Autoload singleton loads textures once at startup for the 2D map renderer
- Builds `Texture2DArray` from individual PNGs in `textures/base/`
- Exposes: `terrain_textures` (Texture2DArray), `terrain_config`, `use_terrain_textures`, `use_edge_blending`

### 2D Rendering Pipeline
- `MapView` gets textures from `TerrainTextureManager` and pre-renders hex-masked textures on startup
- Cached as `ImageTexture` per terrain ID for efficient drawing
- Falls back to solid colors when overlay mode is active
- Textures only displayed in base view (empty overlay key)
- Fog of War keeps textures: the draw loop classifies each tile once via
  `_visibility_state_at()` — Active tiles draw full-brightness, Discovered tiles
  are tinted toward the mist color (cloudy) via `_fow_texture_tint_for_state()`,
  Unexplored tiles fill with the fog color.
- Runtime toggle: `T` key (`enable_terrain_textures` / `_toggle_terrain_textures`)
- Edge blending: gradient lines drawn at terrain boundaries

### Edge Blending - Overlay/Fringe Technique
When `use_edge_blending` is enabled, the 2D renderer uses a standard overlay/fringe technique:
- 6 edge gradient masks (`assets/terrain/textures/edges/edge_mask_*.png`)
- 222 pre-rendered edge overlays (37 terrains × 6 edges)
- Neighbor terrain texture fades in at hex boundaries

Generate edge masks: `godot --headless --script assets/terrain/EdgeMaskGenerator.gd`

---

## HUD Panel Framework (Docked PanelCards)

The HUD (`HudLayer.tscn`) owns the screen regions with one layout authority — a
`RootColumn` VBox split into `TopBar` / `ContentRow(LeftDock · center · RightDock)`
/ `BottomBar`. No panel positions itself with absolute offsets into a region;
everything is container-sized so regions never collide.

### Inspector as a reserved side dock
The `Inspector` is a debug/telemetry CanvasLayer docked (resizable) against the
**left** edge. It does not overlap or rearrange gameplay panels — instead it
*reserves* space, shrinking the game area to fit beside it, as if the window were
narrower:

- `Inspector.reserved_width()` reports the strip it occupies (`_panel_width +
  2·PANEL_MARGIN`, or 0 when hidden) and emits `reserved_width_changed` on
  show/hide and on live drag-resize.
- `Main._on_inspector_reserved_width_changed` fans that width out to both
  surfaces: `Hud.set_left_inset(px)` (insets `LayoutRoot.offset_left`, so every
  bar and dock lives in the narrower rect) and `MapView.set_view_inset_left(px)`.
- `MapView.set_view_inset_left` makes the map behave as if the window were that
  much narrower, via three coordinated pieces:
  1. `_get_adjusted_viewport_size()` subtracts the inset, so fit, pan-clamp, draw
     extents, hit-testing and the minimap indicator all treat the remaining width
     as the whole viewport.
  2. The node is translated right by the same amount (`position.x = inset`) so
     that reduced coordinate space renders beside the panel. Because
     `get_local_mouse_position()` accounts for the node transform, clicks stay
     correct without touching any screen↔hex math.
  3. `_apply_view_clip()` (in `_draw`, via `RenderingServer.canvas_item_set_clip`)
     clips every draw command to the usable rect. This is essential: the map is
     **cover-fit**, so its content is wider than the reduced viewport and would
     otherwise overflow left into the reserved strip. Clipping confines it.

Because the HUD, Inspector, and map all sit under the same `content_scale`
transform, the reserved width is a single canvas-space value that applies to all
three with no per-surface scaling. Panels keep their natural left/right docks.

### PanelCard (`ui/PanelCard.gd`)
The single building block for every dock panel. It is a `PanelContainer` (never a
bare `Panel`) that owns the chrome — styled background + title header — and hosts
caller content in a plain `VBoxContainer`. Because it is container-sized, it
always reports a correct minimum size, so the dock reflows automatically.

- **Content contract:** author one child `VBoxContainer` named `CardContent`. The
  card inserts its title header as that container's first row and **never
  reparents the authored widgets** — reparenting them into a runtime wrapper
  silently clears `unique_name_in_owner`, so `%Name` references from the owner
  script break. Reference inner widgets by unique name (`%Name`).
- **Rule:** no anchor-positioned children inside a card. Anchor layout inside a
  container parent is what made the legacy `Panel`s overlap.
- API: `card_title` / `set_card_title()`, `get_content()`, and `hotkey_hint`
  (renders the toggle key in the header, e.g. `"Terrain Types (L)"`; leave empty
  for panels with no show/hide hotkey).
- Replaces the bespoke `ui/AutoSizingPanel.gd` height math — the dock's own
  `ScrollContainer` owns overflow, so cards only size to content.

### PanelDock (`ui/PanelDock.gd`)
Ordered controller for one dock region's `VBoxContainer`. Panels `add(panel,
priority)` to register; the dock reparents them in priority order. Visibility is
data-driven — `set_relevant(panel, false)` (or `panel.visible = false`) removes a
panel from layout flow and the stack reflows with no gap. Hud builds `left_dock`
and `right_dock` in `_ready()`.

**Scroll behaviour:** on construction the dock disables **horizontal** scrolling
on its enclosing `ScrollContainer` and zeroes the stack's horizontal minimum, so
the stack always fills the dock width and content wraps to fit rather than
spilling under a sideways scrollbar (which reads as unpolished for a game HUD).
**Vertical** scroll mode is *not* set by PanelDock — it is configured per dock in
the scene (`HudLayer.tscn`); both docks use `AUTO`, so a scrollbar appears only
when the stack actually overflows.

**Migration status:** `SelectionPanel`, `CommandFeedPanel`, and
`TerrainLegendPanel` are now `PanelCard`s (the last two dropped the bespoke
`AutoSizingPanel` height math and the legend's absolute `PRESET_TOP_RIGHT`
positioning that used to overlap the Victory panel). `StockpilePanel` and
`VictoryPanel` are still plain `PanelContainer`s (correctly container-sized, but
not yet cards). `AutoSizingPanel.gd` remains only for the Inspector.

---

## Inspector Panels

See `docs/godot_inspector_plan.md` for full roadmap.

| Tab | Purpose |
|-----|---------|
| Map | Overlay selector, logistics toggle, map size dropdown, Generate Map button |
| Terrain | Full biome histogram, tag histograms, tile drill-down, terrain-type highlight dropdown, **Export Map** button |
| Fauna | Herd registry, follow-herd commands, density telemetry |
| Culture | Layer trait vectors, divergence meters, resonance pushes |
| Military | Readiness heatmaps, cohort summaries |
| Power | Grid metrics, node list, incident feed |
| Crisis | Dashboard gauges, modifier tray, event log |
| Knowledge | Ledger overview, timeline graph, espionage mission queue |
| Logs | Streaming tracing feed, level/target/text filters, duration sparkline |
| Commands | Turn/rollback/autoplay, axis bias, spawn utilities, debug hooks |

**Capability gating** (`Inspector._apply_capability_gating`): most tabs enable only when the matching `CapabilityFlags` bit is set. **Terrain is exempt** — it is an always-available inspection tab (the one construction *action* inside it, Found Camp, stays separately gated). **Migrated tab panels don't grey out** — instead of disabling the tab (confusing: a dead tab with no explanation), the coordinator calls `panel.set_available(has_flag)` and the panel stays clickable, rendering a "🔒 Locked — unlocks via …" message while gated (see `PowerPanel`). `_set_tab_enabled` is still used for tabs not yet migrated to the panel contract. Its **terrain-type highlight** dropdown lists every defined terrain (via `TerrainDefinitions`), and selecting one calls `MapView.set_terrain_highlight(id)`, which outlines/tints all matching hexes map-wide (ignoring Fog of War) — handy for spotting a biome or confirming one is absent. Selecting "none" (`-1`) clears it.

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
- Public feeder methods for cross-panel data flow (`KnowledgePanel.append_events`,
  fed by Trade's diffusion records). The two panels never reference each other —
  `TradePanel` emits `knowledge_events_produced(records)` and the coordinator
  forwards the batch to `KnowledgePanel.append_events` (wired in `_ready`).
- Coordinator-owned state pushed into a display panel: `SentimentPanel.set_axis_bias`
  — axis bias belongs to the Commands axis controls (which mutate it optimistically),
  so the coordinator pushes it to the Sentiment view at both the snapshot and the
  optimistic-write sites, instead of the panel owning the key.
- Command-issuing via a signal when the command needs coordinator-only context:
  `FaunaPanel` emits `follow_herd_requested(herd_id)` (the follow command needs the
  active faction, which lives in the coordinator) rather than taking `set_command_hooks`;
  it also emits `herd_selected(herd_id)` so the coordinator can mirror the Commands
  follow field. `set_log_hook(append_log)` is the log-only variant of `set_command_hooks`
  (`VictoryPanel`'s one-shot victory announcement).

The coordinator collects extracted panels in `_tab_panels` and fans `apply_update`
out to them at the **end** of `_apply_update`, after its own key routing (e.g.
`_ingest_overlays`), so a panel's own keys win over coordinator-side feeders on
conflict (see the `crisis_overlay` vs `overlays.crisis_annotations` precedence note).

**Reference implementations:** `ui/inspector/PowerPanel.gd` (Power — pure
snapshot/render), `ui/inspector/CrisisPanel.gd` (Crisis — command hooks +
typography), `ui/inspector/KnowledgePanel.gd` (Knowledge — the fullest: connection
gating, log-path ingestion, and the Trade→Knowledge event feed), and
`ui/inspector/TradePanel.gd` (Trade — map-overlay collaborator + the emit side of
the Knowledge↔Trade seam). Tabs still living inline in `Inspector.gd` are migrated
onto this contract one at a time.

**Commands tab (designer/debug console).** The `Commands` tab (axis-bias, heat,
config-reload, scenario scout/follow/camp, autoplay row, influencer/corruption command
buttons, command status/log) is now `CommandsPanel` (see the key-scripts table). Its
subtree once went missing in the 2025-11-21 scene split (`Main.tscn` → instanced
`InspectorLayer.tscn`) and sat dead for months — the coordinator's
`get_node_or_null("RootPanel/TabContainer/Commands/…")` refs silently resolved to
`null` — before it was transplanted back from git history and extracted onto the
tab-panel contract. The **command hub stays in the coordinator**: `_send_command` →
`command_client`, `_ensure_command_connection`, the `autoplay_timer`, and turn-sending
are shared with the turn controls in `RootPanel/CommandToolbar` (outside the
`TabContainer`) and the scout/found_camp buttons in the Terrain tab. The panel issues
verbs through `set_command_hooks` and is connection-gated via `set_command_connected`.
Autoplay is split: the toggle+interval widgets live in the panel (relayed as
`autoplay_toggled`/`autoplay_interval_changed`), while the timer that steps turns and
the toolbar Play/Pause mirroring stay in the coordinator (which calls back
`set_autoplay_active`). Axis bias is coordinator-owned (Sentiment depends on it): the
panel emits `axis_bias_apply_requested` and the coordinator sends + mirrors it back via
`set_axis_bias`. The influencer dropdown is fed `InfluencerPanel.get_influencers()`
through the coordinator (`set_influencer_roster`).

---

## Overlay Channels

Raster overlays streamed from `core_sim`:

| Channel | Color | Source |
|---------|-------|--------|
| `logistics` | Blue | Throughput flow |
| `sentiment` | Red | Morale/agency composite |
| `corruption` | Amber | Ledger intensity + risk weights |
| `fog` | Slate | Inverted knowledge coverage |
| `culture` | Violet | Divergence magnitude |
| `military` | Green | Readiness scalar |
| `terrain_tags` | Blended | Per-tag colors averaged |

Legend rendering: min/avg/max values + channel description.

---

## Typography & Theming

Shared `Theme` resource reads `INSPECTOR_FONT_SIZE`, applies to root `CanvasLayer`. Typography map: `body`, `heading`, `caption`, `legend`, `control`.

Helper: `Typography.gd` provides offset deltas (heading = base + 4, caption = base − 2).

---

## Scripting Capability Model

QuickJS sandbox for user scripts.

### Capability Families
- `telemetry.subscribe` - Read-only snapshot feeds with back-pressure
- `ui.compose` - Declarative widget graph (Panel, VBox, Table, Chart2D, OverlayLayer)
- `commands.issue` - Vetted command endpoints with throttle windows
- `storage.session` - Scoped key/value cache persisted with saves
- `alerts.emit` - Toast/banner notifications with rate caps

### Script Distribution
- `.sscmod` bundles (zip) with `manifest.json`, Ed25519 signature
- Local install: `mods/inspector/` or UI import
- Workshop feeds: JSON index of signed bundles

### Lifecycle
- Manifest validation on load
- Hot reload via esbuild-lite bundling
- Suspension on sandbox violations (memory/instruction limits)

---

## Hotkeys

| Key | Action |
|-----|--------|
| `W/A/S/D` | Pan map |
| `Q/E` | Zoom |
| Mouse wheel | Zoom at cursor |
| Right/middle drag | Pan |
| `C` | Fit map to view |
| `H` | Toggle hex grid lines |
| `F` | Toggle fog of war |
| `T` | Toggle terrain textures |
| `I` | Hide/show inspector |
| `L` | Collapse/restore legend |
| Double-click herd | Issue `FollowHerd` |
| Shift+double-click herd | Queue `ScoutArea` |

---

## See Also

- `README.md` - Setup and running instructions
- `docs/godot_inspector_plan.md` - Inspector migration progress
- `core_sim/CLAUDE.md` - Simulation engine (snapshot contracts, commands)
- `docs/architecture.md` - Cross-system data flow
