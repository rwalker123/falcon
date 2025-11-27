# Relief Map UI Roadmap

## Goal
Transform the Heightfield preview into a full gameplay viewport with in-scene HUD, minimap, and unit/overlay visualization so players can interact entirely within the 3D relief view.

## Milestones

1. **HUD Shell & Window Integration**
   - [x] Embed a CanvasLayer/Control stack inside `HeightfieldPreview` for persistent UI (zoom slider, overlay legend, selection info).
   - [x] Mirror existing 2D HUD styling so toggling views feels seamless.
   - [x] Ensure input focus and ESC/shortcut behavior match MapView.

2. **Shared Data Plumbing**
   - [x] Reuse MapView data sources (selection state, overlay stats, inspector text) via shared signals or a lightweight state service.
   - [x] Keep selection/highlight state synchronized when switching between 2D↔3D views.
   - [x] Expose a preview HUD API so existing `Hud.gd` / `Inspector.gd` logic can render in the 3D overlay without duplication.

3. **Minimap Integration**
   - [ ] Add a secondary top-down Viewport/Cam that renders the relief mesh to a `ViewportTexture` for the minimap panel.
   - [ ] Support click/drag on the minimap to pan the main relief camera, matching MapView behavior.
   - [ ] Keep the minimap camera bounds aligned with the current map extents & overlays.

4. **Overlay & Unit Markers**
   - [ ] Introduce a `UnitOverlay3D` (or similar) that places mesh/billboard markers at hex centers using `_height_at_world` to rest on the terrain.
   - [ ] Sync colors/icons with 2D overlay legend (e.g., logistic flows, cultural highlights) and keep shader uniforms consistent.
   - [ ] Provide config toggles in `heightfield_config.json` for marker visibility, scale, and shading.

5. **Interaction Refinements**
   - [x] Debug HUD visibility in 3D view <!-- id: 4 -->
     - [x] Verify `HudLayer` instantiation and parenting <!-- id: 5 -->
     - [x] Check for hidden visibility toggles in `Hud.gd` <!-- id: 6 -->
     - [x] Debug `SubViewportContainer` clipping issues <!-- id: 7 -->
     - [x] Fix clipping by disabling `embed_subwindows` <!-- id: 8 -->
   - [x] Fix 2D Window width on ultrawide screens <!-- id: 9 -->
   - [x] Add Inspector HUD to 3D view <!-- id: 10 -->
   - [x] Add HUD buttons/hotkeys for switching overlays, issuing commands, and opening inspectors directly from the 3D view.
   - [x] Implement tooltip/highlight feedback when hovering units/hexes with raycasts from the relief camera.
   - [x] Polish transitions between relief and strategic views (animations, saved camera states, etc.).

## Dependencies / Notes
- Requires continued use of the existing snapshot data (`heightfield_data`, overlay buffers) already pushed into the preview.
- Any new config knobs should live in `clients/godot_thin_client/src/data/heightfield_config.json` to keep tuning code-free.
