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
| `Inspector.gd` | Tabbed inspector panels, overlay selector |
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

**Capability gating** (`Inspector._apply_capability_gating`): most tabs enable only when the matching `CapabilityFlags` bit is set. **Terrain is exempt** — it is an always-available inspection tab (the one construction *action* inside it, Found Camp, stays separately gated). Its **terrain-type highlight** dropdown lists every defined terrain (via `TerrainDefinitions`), and selecting one calls `MapView.set_terrain_highlight(id)`, which outlines/tints all matching hexes map-wide (ignoring Fog of War) — handy for spotting a biome or confirming one is absent. Selecting "none" (`-1`) clears it.

The overview text draws a **full biome histogram** (`_render_terrain` → `_histogram_bar`): every present biome, sorted by count, with a monospace `[code]` bar scaled to the most common biome plus its tile count and percentage — all computed client-side from the streamed `_terrain_counts`. The **Export Map** button (`_on_export_map_button_pressed`) sends the fire-and-forget `export_map` runtime command; the server writes the current map (terrain snapshot + resolved seed) to its `exports/` scratch dir as JSON (see `sim_schema` `MapExport`). Tile coordinates shown here as `@x,y` (`_format_tile_coords`) index straight into the export's row-major samples, so the same coordinate names a hex in the client, in the export file, and in tests.

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
