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
| `Main.gd` | Scene orchestration, streaming toggle, 3D/2D view management |
| `MapView.gd` | Terrain rendering, overlays, hex selection, navigation (WASD/QE/mouse), 2D minimap |
| `HeightfieldLayer3D.gd` | 3D relief mesh rendering with chunked `ArrayMesh` |
| `HeightfieldPreview.gd` | 3D view container with camera controls, 3D minimap |
| `WaterLayer3D.gd` | Water plane rendering controlled by `heightfield_config.json` |
| `Inspector.gd` | Tabbed inspector panels, overlay selector |
| `Hud.gd` | HUD layer, legend, selection panel, turn readout |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandBridge.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Shared minimap component for 2D/3D views (click-to-pan, aspect ratio sizing) |
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content |
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading (shared by 2D/3D) |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |

---

## Architecture

### Scene Structure
- `Main.tscn` - Root scene with `CanvasLayer` for HUD/inspector, 3D viewport for relief view
- Camera: boots directly into 3D relief view, fits map width to viewport
- Toggle: `Enter` flips terrain shading, `I` hides/shows inspector, `L` collapses legend

### Data Flow
```
Server (FlatBuffers) -> SnapshotStream.gd -> parsed snapshot
                                          -> MapView (terrain/overlays)
                                          -> Inspector (panels)
                                          -> Hud (legend, selection)
```

### Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

---

## Heightfield Rendering

3D relief visualization that lifts terrain off the hex board.

### Data Pipeline
- `SnapshotOverlay` contains `heightfield` (u16 grid) + optional `normal_raster`
- Normalization metadata (global min/max) in overlay header
- Biome weight masks per hex for seamless tinting

### Shader Architecture
- `HeightfieldLayer3D` owns chunked `ArrayMesh` (64×64 quads) displaced via `ShaderMaterial`
- Inputs: grayscale height texture, normal map, biome weight texture array, AO/curvature LUTs
- Lambertian lighting with baked sun vector, contour colouring support

### Hex Integration
- Selection/hover via `ImmediateMesh` projected above terrain
- Ray-casting into height mesh for picking
- Existing heatmap overlays render as additive projected quads following height texture

### Configuration (`heightfield_config.json`)
- `minimap`: `base_height`, `min_width`, `max_width`, `margin` for panel sizing
- `markers`: toggle visibility, adjust scale/y_offset, `shaded` flag for lit vs unlit
- `water`: `sea_level_offset`, `sea_level_override`, deep/coastal/fresh colors

---

## Minimap System

Both 2D and 3D views display a minimap in the bottom-right corner showing the full map with a viewport indicator rectangle.

### Shared Component (`ui/MinimapPanel.gd`)
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

### 3D Minimap (HeightfieldPreview.gd)
- Uses a `SubViewport` with orthographic `Camera3D` sharing the main world_3d
- Renders the full 3D terrain from top-down view
- Click-to-pan converts normalized position → world coordinates → camera pan

### Configuration
Sizing parameters in `heightfield_config.json`:
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

Optional terrain texture graphics for both 2D and 3D views.

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
- Autoload singleton loads textures once at startup, shared by 2D and 3D renderers
- Builds `Texture2DArray` from individual PNGs in `textures/base/`
- Exposes: `terrain_textures` (Texture2DArray), `terrain_config`, `use_terrain_textures`, `use_edge_blending`

### 3D Rendering Pipeline
- `HeightfieldLayer3D` gets textures from `TerrainTextureManager` and builds terrain index texture
- `heightfield.gdshader` samples terrain textures via `sampler2DArray`
- LOD blending between textured (close) and solid color (far)
- Terrain IDs passed via terrain_index_texture (R8 format)
- Edge blending: neighbor terrain texture encodes 6 neighbors per hex for smooth transitions

### 2D Rendering Pipeline
- `MapView` gets textures from `TerrainTextureManager` and pre-renders hex-masked textures on startup
- Cached as `ImageTexture` per terrain ID for efficient drawing
- Falls back to solid colors when overlay mode is active
- Textures only displayed in base view (empty overlay key)
- Edge blending: gradient lines drawn at terrain boundaries

### Edge Blending - Overlay/Fringe Technique
When `use_edge_blending` is enabled:
- **2D**: Uses standard overlay/fringe technique:
  - 6 edge gradient masks (`assets/terrain/textures/edges/edge_mask_*.png`)
  - 222 pre-rendered edge overlays (37 terrains × 6 edges)
  - Neighbor terrain texture fades in at hex boundaries
- **3D**: Basic shader-based blending (uses neighbor texture sampling)

Generate edge masks: `godot --headless --script assets/terrain/EdgeMaskGenerator.gd`

---

## Inspector Panels

See `docs/godot_inspector_plan.md` for full roadmap.

| Tab | Purpose |
|-----|---------|
| Map | Overlay selector, logistics toggle, map size dropdown, Generate Map button |
| Terrain | Biome list, tag histograms, tile drill-down |
| Fauna | Herd registry, follow-herd commands, density telemetry |
| Culture | Layer trait vectors, divergence meters, resonance pushes |
| Military | Readiness heatmaps, cohort summaries |
| Power | Grid metrics, node list, incident feed |
| Crisis | Dashboard gauges, modifier tray, event log |
| Knowledge | Ledger overview, timeline graph, espionage mission queue |
| Logs | Streaming tracing feed, level/target/text filters, duration sparkline |
| Commands | Turn/rollback/autoplay, axis bias, spawn utilities, debug hooks |

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
| `Enter` | Toggle terrain shading |
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
