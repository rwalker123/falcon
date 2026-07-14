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
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's band stack), navigation (WASD/QE/mouse), 2D minimap, and the **layered hex-marker system** (see Map markers below) |
| `Inspector.gd` | Inspector coordinator: streaming fan-out, capability gating, typography; hosts per-tab panels |
| `ui/inspector/PowerPanel.gd` | Power tab panel — reference for the tab-panel extraction contract (`apply_update`/`reset`) |
| `ui/inspector/CrisisPanel.gd` | Crisis tab panel — adds command hooks (`set_command_hooks`) and `apply_typography` to the contract |
| `ui/inspector/KnowledgePanel.gd` | Knowledge tab panel — adds `set_command_connected` (connection-gating), `ingest_log_entry` (log-path telemetry), and `append_events` (Trade→Knowledge feed) |
| `ui/inspector/TradePanel.gd` | Trade tab panel — `set_map_view` (overlay), owns the Map-tab overlay toggle, and emits `knowledge_events_produced` (the coordinator forwards it to KnowledgePanel — panels stay decoupled) |
| `ui/inspector/SentimentPanel.gd` | Sentiment tab panel — display; axis bias is coordinator-owned and pushed in via `set_axis_bias` |
| `ui/inspector/VictoryPanel.gd` | Victory tab panel — display + one-shot "victory achieved" log via `set_log_hook` |
| `ui/inspector/FaunaPanel.gd` | Fauna tab panel — **display-only** herd list/detail + estimated hunt yields. The follow-herd command it used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a; hunting is now HUD labor allocation), so it issues no command; `set_command_connected` is a contract no-op |
| `ui/inspector/GreatDiscoveriesPanel.gd` | GreatDiscoveries tab panel — large, self-contained (ledger + progress + definition catalog + details); capability-gated (`CAP_MEGAPROJECTS`), no command/log/MapView coupling |
| `ui/inspector/LogsPanel.gd` | Logs tab panel — owns the LogStreamClient + polling + filters + tick sparkline; emits `log_entry_received` (coordinator dispatches to Knowledge/Trade); fed synthetic lines via `append_entry` |
| `ui/inspector/InfluencerPanel.gd` | Influencers tab panel — owns the influencer roster; capability-gated (`CAP_INDUSTRY_T1`/`T2`) via `set_available`; exposes `aggregate_resonance()` (coordinator feeds it into the Culture tab) and `get_influencers()` (coordinator's still-inline influencer command controls read the roster back). The influencer *command* controls stay coordinator-owned |
| `ui/inspector/CorruptionPanel.gd` | Corruption tab panel — display-only ledger (reputation modifier, audit capacity, incidents); not capability-gated |
| `ui/inspector/CommandsPanel.gd` | Commands tab panel — the designer/debug console (axis-bias, influencer/channel/spawn, corruption inject, heat, config reload, autoplay row, command status/log; the scenario scout/follow rows were removed with the retired single-task commands). Outbound: issues verbs via `set_command_hooks` and logs via the sink; the command transport + autoplay timer + turn-sending stay in the coordinator. Couplings are coordinator-mediated: emits `axis_bias_apply_requested` (coordinator owns `_axis_bias`, pushes back via `set_axis_bias`), `autoplay_toggled`/`autoplay_interval_changed` (coordinator drives the timer, mirrors via `set_autoplay_active`); fed the roster via `set_influencer_roster` and gated via `set_command_connected`. NOT in `_tab_panels` (no snapshot inputs) |
| `ui/inspector/OverlayPanel.gd` | "Map Overlays" section (nested inside the Map tab, attached to `OverlaySection`) — owns the overlay-channel selector (built at runtime), channel metadata, and the culture/military readouts; drives `MapView.set_overlay_channel`. Fed via `set_map_view` + `ingest(overlay_dict, terrain_tag_labels)` (the coordinator re-homes the palette → Terrain and crisis_annotations → Crisis side-routes that share the `overlays` key, and passes Terrain's tag labels since the terrain-tags channel depends on them). NOT in `_tab_panels` |
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the highlight-rivers toggle (now a shader uniform — see Edge Blending → Rivers). Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` via `set_map_view`. The nested Map-Overlays section keeps its own `OverlayPanel` script |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `ui/inspector/TerrainPanel.gd` | Terrain tab panel — the largest: biome list + drill-down, tile list/detail, the runtime terrain-highlight dropdown, and the **Export Map** button (the tile Scout button was retired with the single-task `scout` command). Snapshot-driven (in `_tab_panels`): `apply_update` ingests `tiles`/`tile_updates`/`tile_removed`/`food_modules` and renders. Owns the inbound MapView hex-selection (`focus_tile_from_map`, coordinator forwards) and drives `set_terrain_highlight` / `relative_height_at` via `set_map_view`. The biome palette + tag labels arrive on the `overlays` key (coordinator routes them in via `set_terrain_palette`/`set_terrain_tag_labels`; `get_terrain_tag_labels()` feeds OverlayPanel). Export sends via `set_command_hooks`, gated by `set_command_connected` |
| `Hud.gd` | HUD layer, legend (the right-dock **TerrainLegendPanel**; `update_overlay_legend` renders rows `{color,label,value_text}` and, for the base terrain legend (`key == "terrain"`) only, shows a runtime-built **sort header** — `Name`/`Count` toggle buttons with a ▲/▼ arrow on the active field. Sort mode is display-only HUD state — field ∈ {name,count} × per-field direction, default **Count desc** — held in `_legend_sort_*` and re-applied via `_sorted_terrain_rows` on every legend push, so the chosen order persists across map regen; MapView's `_build_terrain_legend` supplies a numeric `count` per row for the count sort. Non-terrain (overlay/tag) legends hide the control and render in the given order), the split **Tile card** (`TilePanel`/`%TileDetail` — terrain + the `%ForageAssignControls` "assign foragers" stepper) + **Occupants roster card** (`OccupantsPanel`/`%RosterList`/`%OccupantDetail` — selectable bands+wildlife roster with a per-occupant detail drawer for **herds/expeditions**; a herd shows the `%HerdAssignControls` "assign hunters" stepper+policy picker, an expedition the `%AllocationPanel` Recall/Move panel). **Player-band detail relocated into the dockable `BandCityPanel`** (summary + `%AllocationPanel`-style labor UI render there via `_render_band_into_panel`; the Occupants card keeps only the roster row) — see "Band/City dockable panel". Turn readout (the standalone band Alerts panel was folded into the turn-orb attention model — see "Turn orb & attention model"). Both cards + all selection state (`_selected_tile_info`/`_selected_unit`/`_selected_herd`) + the snapshot-captured `_player_band` (and `_player_bands`, the full player-faction list backing the band-picker + the panel cycler) live here; roster selection emits `roster_occupant_selected`; labor edits emit `assign_labor_requested` / `move_band_requested` / `cancel_order_requested` (clear-all) |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail. Header (stage glyph/name/label + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse), body hosts the relocated band detail as **section blocks** via `set_band_sections` (tall = vertical stack that fits its width to the content, wide = manual balanced-column packing that fits its height to the content). Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`. See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale warn/critical thresholds + `color_for_morale`/`hex_for_morale`), shared by MapView's band dot and Hud's food/morale lines + alerts |
| `ui/TileHabitability.gd` | Single source of truth for the Tile-card Habitability rating: buckets `TileState.habitability` (band-independent per-turn morale drain) into Hospitable/Fair/Harsh/Hostile via `tile_habitability_config.json` thresholds, with the HEALTHY/INK/WARN/DANGER color / `hex_for_rating` mapping. Consumed by `Hud._tile_terrain_lines` + `_format_detail_bbcode` |
| `ui/TileClimate.gd` | Single source of truth for the Tile-card Climate band: maps `TileState.temperature` (°, a latitude+elevation climate, equator-in-the-middle) into Tropical/Warm/Temperate/Cool/Polar via `tile_climate_config.json` cutoffs. INFORMATIONAL only — deliberately no HEALTHY/WARN/DANGER tint (renders neutral ink), so it doesn't compete with the Habitability row's semantic palette. Consumed by `Hud._tile_terrain_lines` |
| `ui/RiverEdges.gd` | Single source of truth for the TEXT reading of hex-EDGE rivers: owns the class vocabulary (Minor/Major), the 6 direction names, and the mask bit-widths as named constants, and formats `TileState.riverEdges` into `Major River: NE, NW` / `Minor River: SW` rows (`summary_lines`, Major first, directions in compass order from NE). Consumed by BOTH `Hud._tile_terrain_lines` (Tile card) and `Hud.show_tooltip` (map hover) — one formatter, two surfaces. See Edge Blending → Rivers |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandBridge.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Minimap component for the 2D map view (click-to-pan, aspect ratio sizing) |
| `ui/TurnOrb.gd` / `ui/TurnOrb.tscn` | The bottom-right **turn orb** (replaces the old "Advance Turn" button): calm cyan pulse when the attention registry is empty, else a severity-tinted count badge + a reasons popover (see "Turn orb & attention model"). Re-emits `focus_requested` (jump) / `advance_requested` so Main's advance/jump wiring is unchanged; palette from `HudStyle`, all geometry/severity/kind as named constants |
| `ui/MagnifierButton.gd` | Zoom-rail in/out button that `_draw`s a crisp magnifier icon (lens + handle + inner `+`/`−`, `zoom_sign` picks which) — font magnifier glyphs render as tofu/blobs. Monochrome `HudStyle` ink → `SIGNAL` on hover |
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content |
| `ui/HudStyle.gd` | Single source of truth for the dark HUD console look: palette (cyan `SIGNAL`, amber `WARN`, ink/line neutrals), `card_stylebox()`, `header_stylebox()`, `banner_stylebox()`, `apply_button(btn, "primary"/"ghost"/"armed")`, and `apply_link_button(btn, base_color)` — the **inline link** treatment for a clickable label inside a row (no box at rest; hover tint + cyan text + pointing hand), used by the band panel's clickable Current-actions rows. Every HUD surface styles through here |
| `ui/FoodIcons.gd` | Shared glyph vocabulary — food modules (`for_site`), fauna herds (`for_herd`, species keyword matched in the herd label, longest-first), and **take policies** (`for_policy`, `POLICY_ICONS`: the four extractive rungs sustain ♻ / surplus ⬆ / market ⇄ / eradicate 💀, plus the two **investment** rungs cultivate 🌱 / corral 🐄 — 🐄 is the same glyph the herd drawer's Domesticated/Corralled badge uses; both verified legible at picker size in `forage_cultivate.png` / `herd_corral.png`; `""` for unknown). Used by the map's food-site / herd markers (`MapView._draw_food_site` / `_draw_herd`), the Harvest/Hunt button + the **band panel's Current-actions rows** (each row leads with its resource glyph), and — for policies — BOTH the Hud policy-picker buttons (`_build_policy_picker`) and the map's yield labels (`MapView._draw_yield_label` appends the icon: `+0.38 ♻`), so a resource/policy always reads the same on the panel and on the map. **Policy glyphs are deliberately line-art** (♻ ⬆ ⇄) plus the high-contrast 💀: pictographic emoji (🪙 coin, 💰 money bag) render as a featureless grey blob at the ~12–13px these are drawn at, and ⚖ renders tiny/faint — same glyph-legibility hazard that forced `MagnifierButton` to hand-draw. Verified in `band_panel_left.png` / `map_band_work.png`. Also the **action-status** glyphs (`for_status`, `STATUS_ICONS`) the Band panel's Current-actions + Active-expeditions rows use instead of words — `pending ○` (the ORDER isn't acknowledged yet; a modifier that rides on any row, amber) / `working ●` (a confirmed local forage/hunt row, and expedition phase `hunting`) / `outbound ➤` / `awaiting ▮▮` / `delivering ◄` = `returning ◄` (both are "coming home"; the tooltip distinguishes them). Same line-art rule and the same hazard: `◌` (dotted circle) was tried for `pending` and rejected — it renders thin and faint at row size — and `⏸` for `awaiting` carries emoji presentation (tofu/blob), so `▮▮` is used. Verified at true size in `band_panel_status_glyphs.png` |
| `tools/ui_preview.gd` / `.tscn` | Dev-only preview harness: instances the real `HudLayer` with canned selection/targeting data, renders each state, and saves PNGs to `ui_preview_out/` (gitignored). Iterate on HUD styling without a server: `godot --path . res://tools/ui_preview.tscn` |
| `tools/map_preview.gd` / `.tscn` | Dev-only **MapView** preview harness (HUD-only ui_preview's companion): instances the real `MapView`, feeds a canned `display_snapshot` + selects a band, and dumps PNGs (`map_*.png`) to `ui_preview_out/`. Verifies the selected-band labor highlights (work-range ring / worked forage tiles / hunted-herd ring+link; scouting draws no disc — it extends sight in the fog), the terrain/blend states, and the **rivers** state (`map_rivers*.png` — hex-edge Minor/Major rivers + the NavigableRiver terrain chain, incl. `map_rivers_join.png`: a zoomed, hex-anchored close-up of the trunk HEAD, where two tributaries hand over at corners — the frame the `river_inflow` spurs are judged on — and `map_rivers_head_minor.png`: a second navigable head fed by a **Minor tributary only**, the frame the HEAD TAPER is judged on) without a server: `godot --path . res://tools/map_preview.tscn` |
| `tools/band_panel_preview.gd` / `.tscn` | Dev-only preview harness for the **Band/City dockable panel**: instances the real `BandCityPanel` + `HudLayer`, injects the panel into the HUD, pushes a seeded player band through `update_band_alerts`, and dumps the panel docked left/right/top/bottom + collapsed (`band_panel_*.png`) so the chrome + the relocated band detail + the HUD reflow can be eyeballed without a server: `godot --path . res://tools/band_panel_preview.tscn` |
| `tools/marker_field_guard.gd` / `.tscn` | Headless **regression guard** for the "unit marker drops a panel-consumed field" bug class (twice hit: `hunt_mode`, then `working_age`/`idle_workers`). Feeds one realistic population entry through the real `MapView._rebuild_unit_markers` and asserts the produced marker is a superset of `PANEL_CONSUMED_KEYS` (the keys `Hud._unit_summary_lines` + `_build_allocation_panel` read off `_selected_unit`) and that the drop-prone fields round-trip (not defaulted). Exits non-zero on failure (CI-usable). No rendering, so headless: `godot --headless --path . res://tools/marker_field_guard.tscn`. When the panel starts reading a new marker field, add it to `PANEL_CONSUMED_KEYS`. |
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |

---

## Architecture

### Scene Structure
- `Main.tscn` - Root `Node2D` scene with a `Camera2D`, the `MapView` map layer, and `CanvasLayer`s for HUD/inspector/Band-City panel
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

The 2D minimap lives in the HUD **bottom-left** `NavCluster` (an HBox in `BottomBar`,
`HudLayer.tscn`) — a `MinimapContainer` (the map thumbnail with its viewport indicator
rectangle) with a docked **zoom rail** to its right. `MapView._setup_2d_minimap` finds the
container via `Hud.get_minimap_container()`, so the container abstracts the move.

### Zoom rail — the on-screen map-zoom control
The rail (`ZoomRail` VBox) is `＋` (`MagnifierButton`, zoom in) / a live `1.0×` readout /
`－` (`MagnifierButton`, zoom out) / `▣` fit ("Fit map to view (C)"). It rides the **one**
map-zoom path: the buttons emit `Hud.map_zoom_step(±1)` / `map_zoom_fit` → `Main` →
`MapView.zoom_step()` / `fit_to_view()` (thin wrappers over `_apply_zoom`, pivoting on the
map center), and `MapView.zoom_changed(zoom_factor)` → `Hud.set_zoom_readout` renders the
readout (so it also reflects the wheel and `Q`/`E`). The old top-right **interface-scale**
widget (which drove `content_scale_factor` — it scaled the whole canvas uniformly, so map
icons never crossed the icon-LOD threshold) was **removed**; map zoom is now solely
`MapView._apply_zoom`. Interface scale returns later via an Options menu. See
`docs/plan_hud_nav_turn_orb.md`.

The map view displays this minimap showing the full map with a viewport indicator rectangle.

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
    base/                        # 38 terrain textures (512x512 PNG); forest bases are grass FLOOR (no trees)
      00_deep_ocean.png
      ...
      37_navigable_river.png     # NavigableRiver's BANK ground (the channel water is rivers/02) — see Rivers
    canopy/                      # RGBA tree-crown overlays (transparency); one per canopy biome (3 today: 07/12/13)
    peaks/                       # RGBA mountain-relief overlays (transparency); one per relief biome (5 today: 24/25/26/27/29)
    rivers/                      # flowing water, NOT keyed by terrain id (see Rivers): 00_minor / 01_major
                                 # are the hex-EDGE classes (layer = class - 1); 02_navigable is the CHANNEL
                                 # water painted over a NavigableRiver hex's bank
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
  "blend_width": 0.42,
  "blend_noise_cell": 6.0,
  "lod_near_distance": 50.0,
  "lod_far_distance": 200.0
}
```
Every terrain entry also carries a `"blend_class"` (`flat` | `water` | `rugged`) — the single
source of truth for edge-blend eligibility (see Edge Blending below). `blend_width` is the
interlock band fraction; `blend_noise_cell` is the dither value-noise cell size (px).

### Texture Loading (TerrainTextureManager)
- Autoload singleton loads textures once at startup for the 2D map renderer
- Builds `Texture2DArray` from individual PNGs in `textures/base/`
- Exposes: `terrain_textures` (Texture2DArray), `terrain_config`, `use_terrain_textures`, `use_edge_blending`
- Also builds `canopy_textures` (a second Texture2DArray of RGBA crowns from `textures/canopy/`) +
  `canopy_layer_by_id` / `canopy_layer_for(id)` (`terrain_id → canopy array layer`, -1 = none) for the
  blend shader's canopy overlay (see Edge Blending → Canopy overlay), and `peak_textures` (a third
  Texture2DArray of RGBA mountain relief from `textures/peaks/`) + `peak_layer_by_id` / `peak_layer_for(id)`
  for the blend shader's peak overlay (see Edge Blending → Peak overlay), and `river_textures` (a FOURTH
  Texture2DArray of flowing water from `textures/rivers/`) for the blend shader's river pass (see Edge
  Blending → Rivers). The river array is the one array **not** keyed by terrain id — a river is not a
  biome, it rides an edge — so its layer is the file's numeric prefix = river **class - 1**, and there is
  no `river_layer_for(id)`

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
- Edge blending: a flat↔flat **per-pixel biome blend shader** at biome seams (see Edge Blending below)

### Edge Blending — per-pixel biome-blend shader (Approach B)
When `use_edge_blending` is enabled, biome **seams** blend per-pixel in a **fragment shader**
(`assets/terrain/terrain_blend.gdshader`): a symmetric, world-noise **dither** where the two biomes
interlock across the boundary, NOT a gradient blur (blur ghosts on detailed textures). It is
deliberately narrow in scope: only truly *flat* biomes blend, and only against each other; every
other seam stays a **crisp hard edge**. Approach B replaced the earlier baked-overlay dither
(Approach A), fixing its three caveats: **symmetric** mutual intrusion (0.5 at the exact edge on
both sides via signed distance), **no tiling** (world-space noise varies per hex), and **cleaner
grain** (smooth in-shader value noise).

**Eligibility — `blend_class` (config, `terrain_config.json`):** every terrain carries a
`blend_class` of `flat` | `water` | `rugged`. Blend fires for an edge **only** when this hex is
`flat` AND the neighbour across that edge is `flat` AND their terrain ids differ. Any `water`
(crisp shoreline) or `rugged` (forests/hills/mountains/volcanic — never bleed discrete-object
textures) on either side → hard edge. `MapView._terrain_is_flat` / `_blend_class_code` read a cached
`_terrain_blend_class` map (`_build_terrain_blend_class_map`); `TerrainTextureManager.blend_class_for`
mirrors it. On the 4-texture preview this means **desert↔prairie blends**; prairie↔forest and
forest↔ocean stay hard.

**Mechanism — whole-map shader quad + hex splatmap:**
- `terrain_blend.gdshader` (canvas_item) is drawn as **one whole-map rect** by a dedicated child
  node `TerrainBlendQuad` (`show_behind_parent = true`, so it renders BEHIND MapView's grid/markers —
  a separate node is required because a canvas item's ShaderMaterial applies to *all* its draw
  commands). `MapView._setup_terrain_blend_shader` builds it once; `_update_terrain_shader_quad`
  pushes uniforms each frame. Per fragment the shader **inverts the pointy-top odd-r hex layout**
  (MUST match `MapView._hex_center`/`_axial_center`/`_offset_to_axial` + the `hex_origin`/`hex_radius`
  uniforms exactly — this is the alignment contract with grid lines/selection/markers), reads its
  hex's biome from the **`sampler2DArray`**, and — if flat — checks the 6 neighbours (wrap-aware) for
  a different flat biome; near a qualifying shared edge it dithers the neighbour's array sample in.
  The mix is **symmetric**: `p = clamp(0.5 + signed_dist_to_edge / (2·blend_band), 0, 1)` is 0.5 at
  the edge on both sides; `show neighbour if p > value_noise(world_pos / noise_cell)`.
- **Base biome UV — CONTINUOUS world space** (like the canopy pass, NOT per-hex-normalized): the base
  biome is sampled at `base_uv = v_map / (2·hex_radius) · base_scale` (`v_map = v_world - hex_origin`,
  pan/zoom-anchored), so **one texture tile spans ~`1/base_scale` hex-rows** and adjacent hexes show
  DIFFERENT regions of it. This kills the **per-hex identical-repeat grid** (with diagonal seams) that
  any *detailed* (non-homogeneous) base texture used to show when each hex was mapped to one whole
  centered copy — invisible on homogeneous grass/water, obvious on a rocky/alpine texture. The
  **flat↔flat dither samples the neighbour biome at the SAME `base_uv`** (only the array layer differs),
  so the cross-edge interlock stays continuous (two world-sampled biomes at one world point). `repeat_enable`
  tiles the array. The canopy pass already sampled this way; the base now matches it.
- **id-map splatmap** (`_rebuild_terrain_shader_maps`, per snapshot): a `grid_w × grid_h` **RGBA8**
  texture, R = terrain id, G = `blend_class` code (0 water / 1 flat / 2 rugged), B = canopy code
  (0 none, else canopy layer + 1), A = 255, NEAREST-sampled. A
  companion **R8 vis-map** carries FoW state (0 unexplored / 0.5 discovered / 1 active).
- **Config levers:** `blend_width` (→ `blend_band = blend_width · radius`, the interlock half-band in
  px), `blend_noise_cell` (world-noise cell px), and top-level `base_texture_scale`
  (→ `base_scale`, default `0.25` = one base texture spans ~4 hex-rows; smaller covers MORE hexes,
  larger fewer — `BASE_DEFAULT_TEXTURE_SCALE` in `MapView.gd`). **LOD:** below `EDGE_BLEND_MIN_RADIUS`
  (`= ICON_MIN_DETAIL_RADIUS`) the shader renders base-only (no shimmer at far zoom). **FoW:** the
  shader applies the same discovered-mist multiply / unexplored-fog fill as the per-hex path
  (`_fow_texture_tint_for_state` semantics) via the vis-map — it dims, never drops, the blend.
- **Integration:** the shader is the base-terrain renderer whenever `use_terrain_textures` and no
  overlay and `use_edge_blending` (`_shader_terrain_active`); it **bypasses the CPU map cache** (a
  single cheap GPU draw, so the cache's per-hex-loop purpose is moot). With `use_edge_blending` off,
  the **per-hex texture path** (`_build_hex_texture_cache` / `_draw_hex_textured_direct` +
  `CachedMapRenderer`) renders crisp hard hexes — that is the blend-OFF reference. Overlay/solid
  modes are unchanged.

**Shoreline — foam + sand beach at land↔water coasts (universal for now):** separate from the
flat↔flat interlock, every **land↔water** edge gets a two-sided coastal treatment in the same shader,
reusing the signed-distance-to-shared-edge machinery. It fires for any edge where **exactly one side is
water** (`blend_class` code 0) — so it's independent of the land side's class (**both flat-land and
rugged-land** coasts get it) and never touches inland edges (flat↔flat interlock and rugged↔* inland
edges stay exactly as before — both sides non-water → skipped).
- **Foam (water side):** in a water hex within `foam_band` px of a non-water neighbour edge, `result`
  mixes toward `foam_color` (light desaturated cyan/near-white), strongest at the seam and fading
  seaward. The inward reach is noise-perturbed (`reach = foam_band · mix(SHORE_REACH_NOISE_MIN, 1, noise)`,
  reusing the same `noise_cell`) so the surf reads as irregular fingers, not a clean stripe, plus a
  faint second **inner wisp** further out (`SHORE_WISP_*` consts).
- **Beach (land side):** in a non-water hex (flat OR rugged) within `beach_band` px of a water-neighbour
  edge, `result` mixes toward `beach_color` (warm tan), strongest at the seam and fading inland,
  noise-modulated the same way. Net read at a coast: land → thin beach → foam → water.
- **Config levers** (`terrain_config.json` → `shore` block): `foam_width` / `beach_width` (fractions of
  the hex radius → `foam_band` / `beach_band` px uniforms, computed in `MapView._update_terrain_shader_quad`
  like `blend_width`), and `foam_color` / `beach_color` (RGB 0–255, parsed by `MapView._shore_color` into
  normalized `vec3` uniforms). Fallbacks are the `SHORE_DEFAULT_*` consts in `MapView.gd`. LOD-suppressed
  and FoW-tinted like the rest of the shader (shares the `blend_enabled` gate + the vis-map).
- **Per-biome shore gating is deliberately NOT built yet** — all coasts render identical beach+foam.
  Verify via `tools/map_preview.gd` State Q (`_biome_band_terrain` now carves an ocean bay so the ocean
  borders BOTH prairie (grassy shore) and woodland (wooded shore)) → `map_biome_blend.png` +
  `map_biome_shore_seam.png` (coast close-up).

**Canopy overlay — forest = grass floor + overhanging tree crowns:** a forest biome is split into a
**ground layer** that blends like any flat land and a **canopy overlay** of whole crowns that overhang
the hex boundary and thin out, so a forest edge is a natural treeline instead of a razor-cut hex
silhouette. Today the only canopy biome is **12 (mixed_woodland)** — its `blend_class` is now **`flat`**
(the grass floor flat↔flat-blends with prairie and gets a shoreline at coasts, like any flat land); 13
(boreal_taiga) stays `rugged` (no canopy asset yet).
- **Assets:** `textures/base/NN_name.png` is the **forest-floor grass** (trees removed);
  `textures/canopy/NN_name.png` (**new dir**, RGBA crowns on transparency) is the canopy.
- **Second Texture2DArray:** `TerrainTextureManager` builds `canopy_textures` (a companion
  `Texture2DArray` from `textures/canopy/`, same once-only `Image.load_from_file` pattern as the base)
  plus `canopy_layer_by_id` (`terrain_id → canopy array layer`, `canopy_layer_for()` returns -1 for
  none). Only biomes with a canopy file get a layer. Two `sampler2DArray`s in **one** canvas shader work
  fine (base `biome_array` + `canopy_tex`).
- **Canopy code in the splatmap:** the id-map is now **RGBA8** (was RG8) — R=terrain id, G=blend_class
  code, **B=canopy code** (`0` none, else canopy layer + 1), A unused (`MapView._canopy_code`). This
  reuses the per-neighbour id-map fetch the shader already does rather than a separate id-indexed uniform
  array, so both own and neighbour canopy state come from one texture read.
- **Overhang density D (shader):** using the same signed-distance-to-shared-edge machinery vs the
  **canopy↔non-canopy** boundary (`s` = signed distance, + inside the forest): D = 1 deep inside, **~0.5
  at the exact edge**, ramping to 1 over `canopy_softness` px inside and down to 0 at `canopy_overhang` px
  **outside** the forest (crowns overhang the neighbour, then fade). The treeline is world-noise
  perturbed (`CANOPY_TREELINE_NOISE`, reusing `noise_cell`) so it's bumpy, not a clean arc. Interior
  forest hexes (all-canopy neighbours) → D=1. Composited **after** blend+shoreline, before FoW:
  `result = mix(result, crown.rgb, crown.a · D)`.
- **Map-space canopy UV:** `cuv = v_map / (2·hex_radius) · canopy_scale`, where `v_map = v_world -
  hex_origin` is the pan/zoom-anchored MAP coordinate (raw `v_world` is the quad-LOCAL/screen-fixed
  coord and would slide against the grid on pan/zoom — all map-space terms, canopy UV + the
  dither/shore/treeline noise, use `v_map`). Continuous across hexes (a crown straddling a boundary
  reads as one tree). The base biome now samples in the same continuous world space (see **Base biome
  UV** above), so `canopy_scale` and `base_scale` are the two independent world-UV density knobs (a
  crown tile per hex at `canopy_scale = 1.0`; a base tile per ~`1/base_scale` hexes). FoW-tinted like the rest.
- **Canopy LOD is DECOUPLED from the blend LOD** (own `canopy_lod_enabled` uniform, `radius ≥
  canopy_min_radius`, NOT the flat↔flat `blend_enabled`/`EDGE_BLEND_MIN_RADIUS` gate). `canopy_min_radius`
  sits WELL BELOW `EDGE_BLEND_MIN_RADIUS` (3.0 vs 16.0) so the canopy pass keeps running at far zoom:
  interior forest density (D=1) persists into a **distinct darker-green forest mass** (a forest region no
  longer reads as bare grassland when zoomed out); the edge overhang naturally shrinks to nothing as hexes
  shrink. The crown array (`canopy_textures`) is built **with mipmaps** and the `canopy_tex` sampler uses
  **trilinear** (`filter_linear_mipmap`) filtering, so far-zoom crowns AVERAGE into a smooth tone instead of
  shimmering/aliasing. (The base biome array has no mipmaps — `filter_linear` only; the canopy is the layer
  that visibly aliases at far zoom because whole crowns tile many times per tiny hex. If the base ever
  shimmers it can take mipmaps the same way.)
- **Config levers** (`terrain_config.json` → `canopy` block): `overhang_width` / `softness_width`
  (fractions of the hex radius → `canopy_overhang` / `canopy_softness` px uniforms, like `blend_width`),
  `texture_scale` (→ `canopy_scale`), and `canopy_min_radius` (the decoupled canopy LOD floor in px, ≪
  `EDGE_BLEND_MIN_RADIUS`). Fallbacks are the `CANOPY_DEFAULT_*` consts in `MapView.gd`.
- **Caveat — canopy is shader-only:** the blend-OFF **per-hex CPU path** (`use_edge_blending = false`,
  `map_biome_hard.png`) renders only the base, so forests there read as the **bare grass floor** (no
  crowns). The live client runs blend-on, so this affects only the reference/fallback path.
- Verify via `tools/map_preview.gd` State Q → `map_biome_blend.png` + `map_biome_woods_edge_seam.png`
  (the forest block borders prairie floor left + ocean top/right): whole crowns overhang + thin into a
  treeline, interior stays dense, the prairie↔forest floor blends softly, and the forest coast shows
  beach/foam with canopy overhanging the water. Far-zoom decoupled-canopy LOD via State Q-far →
  `map_biome_farzoom.png` (same four bands on a large grid so hexes go tiny): the woodland band reads as a
  distinct darker-green forest mass vs the prairie grass, smooth (mipmapped), not shimmering.

**Peak overlay — highland/volcanic relief = flat rocky floor + overhanging faceted peaks + cast shadow:**
the mountain-drama analog of the canopy overlay, built on the exact same machinery (DRY). A relief biome
keeps its flat rocky base floor and gets an RGBA **peaks overlay** of faceted mountains composited on top:
they overhang the hex boundary and thin to a footline (like the treeline), have an **elevation-driven
prominence**, and **cast a shadow** onto neighbouring hexes, so mountains read as raised relief on the 2D
map. Five relief biomes carry real AI-gen peak art today — **24 (rolling_hills)**, **25 (high_plateau)**,
**26 (alpine_mountain)**, **27 (karst_highland)**, **29 (active_volcano_slope)** — each a magenta-keyed,
offset-blend-seamless RGBA overlay in `textures/peaks/`. (28 canyon_badlands is intentionally NOT a peak
biome — its drama is incision, handled at the base-floor level, not raised relief.)
- **Assets + third Texture2DArray:** `textures/peaks/NN_name.png` (**new dir**, RGBA relief on
  transparency). `TerrainTextureManager` builds `peak_textures` (a THIRD `Texture2DArray`, same once-only
  `Image.load_from_file` + **mipmaps** pattern as the canopy) plus `peak_layer_by_id` /
  `peak_layer_for()` (`terrain_id → peak array layer`, -1 = none). Only biomes with a peak file get a
  layer. Three `sampler2DArray`s in one canvas shader (base + canopy + peaks) work fine.
- **Peak code in the splatmap A channel:** the id-map A channel (previously the unused `255`) now carries
  the **peak code** (`0` none, else peak layer + 1, `MapView._peak_code`) — the peak analog of B=canopy
  code, so both own and neighbour peak state come from the one id-map read the shader already does.
- **New elev-map (R8):** a companion `grid_w × grid_h` R8 texture (parallel to the vis-map), each texel =
  the hex's relative height (`MapView.relative_height_at` 0..100 → 0..255; `PEAK_ELEV_FALLBACK = 200` when
  a snapshot lacks an elevation raster, so relief still renders in preview/rehydrated frames). Drives the
  shader's per-hex `prominence` (`mix(peak_min_prominence, 1, elev)`) and shadow length.
- **Peak pass (shader), after canopy, before FoW:** mirrors the canopy signed-distance-to-boundary scan
  vs the **peak↔non-peak** boundary to get `s` (+inside relief) + `peak_code` (own, else nearest
  peak-neighbour's for the overhang/shadow region). Where `peak_code > 0`: (1) a multi-tap **cast shadow**
  looks back toward `peak_light_dir` (TOWARD the light; top-left = `(-0.7,-0.7)`, canvas +y DOWN) and
  darkens the ground by up to `peak_shadow_strength` where a peak occludes; (2) a **peak composite** over
  the shadowed ground using the shared `canopy_density(s, overhang, softness)` × prominence and the
  world-noise `CANOPY_TREELINE_NOISE` bumpy footline (reused, not duplicated). Peak UV = the same
  continuous map-space `v_map / (2·hex_radius) · peak_scale` as the canopy.
- **Peak LOD is DECOUPLED from the blend LOD** (own `peaks_lod_enabled`, `radius ≥ peak_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`), so the mountain mass persists at far zoom; trilinear-mipmapped
  peak array keeps it smooth (no shimmer).
- **Config levers** (`terrain_config.json` → `peaks` block): `overhang_width` / `softness_width`
  (→ `peak_overhang` / `peak_softness` px, like canopy), `texture_scale` (→ `peak_scale`),
  `peak_min_radius` (LOD floor px), `shadow_length` (→ `peak_shadow_len` px) / `shadow_strength`,
  `min_prominence`, and `light_dir_x` / `light_dir_y` (normalized → `peak_light_dir`). Fallbacks are the
  `PEAK_DEFAULT_*` consts in `MapView.gd`. Peaks are shader-only (same caveat as canopy).
- Verify via `tools/map_preview.gd` **State swatch** with `SWATCH_BIOME_ID = 26` (alpine) →
  `map_swatch.png` (+ `map_swatch_farzoom.png`): faceted peaks composite with light-left/dark-right
  self-shading, overhang the alpine↔prairie seam + cast a darkening shadow onto the prairie, and the
  far-zoom alpine band reads as a raised mountain mass. Restore `SWATCH_BIOME_ID = 2` after.

**Rivers — Minor/Major on hex EDGES, Navigable as a water TERRAIN:** rivers are two different kinds of
thing, and the split is the whole design (see `docs/plan_rivers.md`). A **Minor/Major** river lives on a
hex **edge** — that is where a future crossing cost can live ("the side the river is on is the side that
costs") — and is drawn by a **river pass in `terrain_blend.gdshader`** so the water is painted exactly on
the edge the penalty will apply to. A **Navigable** river is a body of water you are *in*, so **in the sim**
it stays an ordinary water terrain (`TerrainType::NavigableRiver`, **id 37** — blocking + boats fall out of
the existing water rules). **Its RENDER, though, is not a water hex** — see the navigable-channel pass
below: a water hex ran through the land↔water shore pass and came out a hex-shaped puddle with a sandy
beach and surf, i.e. **visually identical to an InlandSea lake** and nothing like a river. It is now drawn
as a silty **BANK with a wide channel through it**. The old `HydrologyOverlay` polyline (and
`MapView._draw_hydrology`) is **deleted** — the tiles now fully determine the render.
- **The wire primitive:** `TileState.riverEdges` (`ushort`), decoded in `native/src/lib.rs tile_to_dict`
  as `river_edges` (both the snapshot and delta tile paths share that one function). A **12-bit mask, 2
  bits per odd-r direction** — `class = (river_edges >> (2*dir)) & 0b11`, `0 = none / 1 = Minor / 2 =
  Major` (3 reserved). **Both hexes flanking an edge carry it** (hex `H` dir `d`; the neighbour dir
  `(d+3)%6`), so a hex answers "is there a river on my side `d`?" locally, with no cross-hex sampling.
  Ingested by `MapView.display_snapshot` into `tile_river_edges` (`Vector2i → int`, like
  `tile_habitability`).
- **The SECOND wire primitive — `TileState.riverInflow`** (`ushort`, decoded as `river_inflow` by the same
  `tile_to_dict`, ingested into `tile_river_inflow`): the same 12-bit / 2-bits-per-slot packing, but keyed
  by hex **CORNER** — `class = (river_inflow >> (2*corner)) & 0b11`. **Why it must exist:** an edge river
  runs *along* a side, so it does not end mid-edge, **it ends at a VERTEX** — and a trunk hex can flank two
  or three river edges (the tributary ran along several of its sides on the way in), which `river_edges`
  alone cannot disambiguate. So the sim names the hand-over vertex. Nonzero on the **first hex of a
  navigable chain only** (a river navigable from its first step has no tributary and reports 0); more than
  one corner may be set (two tributaries can terminate on the same trunk head), so **loop all 6**. Corner
  `i` is the vertex at angle `60*i + 30`, +y down — **exactly `MapView._hex_points` order** (0 lower-right,
  1 bottom, 2 lower-left, 3 upper-left, 4 top, 5 upper-right), so the shader derives it from the hex centre
  and radius with no table; side `dir` spans corners `{dir-1, dir}`. **Deliberately NOT surfaced in the
  Tile card / tooltip** — it is a rendering detail, not player-facing geography (`RiverEdges.gd` still
  reports the SIDES, which is what a crossing cost will key on).
- **The shader's `neighbor_offset` table IS a wire contract now.** It was reordered to the SIM's odd-r
  direction order (`core_sim` `grid_utils::HEX_NEIGHBOR_OFFSETS`, clockwise from E: 0=E, 1=SE, 2=SW, 3=W,
  4=NW, 5=NE) because the river pass indexes the mask **by direction**. The blend/shore/canopy/peak passes
  only ever loop over all 6 and are order-agnostic, so the reorder was free — but **do not reorder it
  again**.
- **RGBA8 river-map splatmap** (`_rebuild_terrain_shader_maps`): all four id-map channels are already
  taken (id / blend_class / canopy / peak), so the river masks get their **own** texture — and BOTH ride
  it: `R/G = river_edges` (low 8 / high 4), `B/A = river_inflow` (low 8 / high 4). Two 12-bit masks are 24
  bits, so they do not fit one RG8 texel; one RGBA8 texture is cheaper than a second sampler. NEAREST,
  rebuilt each snapshot — **after** the tile loop in `display_snapshot` (it reads `tile_river_edges` /
  `tile_river_inflow`, which the tiles populate).
- **River pass (shader), after the shore pass, before canopy/peaks:** trees overhang a river and mountains
  sit above it; sitting before the FoW tint, a river in a Discovered tile **dims with the mist rather than
  disappearing**. Per fragment, for each of the own hex's carrying edges: distance to the **shared edge
  SEGMENT** — `mid ± perp * (hex_radius * 0.5)` (a regular hexagon's side == its circumradius), clamped to
  the segment, **not** the infinite bisector, which would smear the band across the whole hex — then keep
  the edge with the **max coverage** (`half_width - distance`). That min-distance-over-edges pick is what
  **rounds the corner joins for free**: a 120° turn softens with no spline math. The water samples in
  continuous map space (`v_map`, like the canopy) plus a **`TIME` scroll along the winning edge's tangent**
  so it flows.
- **THE HONEYCOMB, and what actually fixes it — read this before touching the river look.** An edge river
  drawn as a wide, constant-width, hard-edged stroke reads as *the hex borders, inked blue*. The instinct is
  to meander harder. **That is a dead end, and not because the meander is under-tuned:**
  - the amplitude ceiling is real — past ~`0.24` of the warp cell the warp's gradient exceeds the band
    half-width and the river **tears into disconnected pools**; and
  - more fundamentally the river is **edge-LOCKED by design**. The water must be painted on the edge the
    future crossing cost applies to ("the side the river is on is the side that costs"), so a warp can only
    displace the band about a band-width before it **detaches from its own edge and starts lying about the
    geometry**. Pushing meander trades a honeycomb for a lie.
  What actually kills the honeycomb, in order of impact: **(1) THINNESS** — halved to `minor_width 0.05` /
  `major_width 0.09`; a thin stroke reads as a river, a wide one as an outline. **(2) WIDTH VARIATION ALONG
  the river** (`width_variation`, low-frequency world noise on a `RIVER_WIDTH_NOISE_CELL = 2.6` hex-radii
  cell — deliberately several radii, so a swell is a property of the *reach*, not of the hex; a cell near 1
  would re-key the variation to the lattice and *reinforce* the honeycomb). **(3) RAGGED BANKS** — a
  higher-frequency wobble of the half-width (`bank_noise_width`, `RIVER_BANK_NOISE_CELL = 0.35`), the same
  idiom as the shore pass's noisy `reach`, plus a wider `softness_width`. Both noises are sampled in
  **world space** (`v_map`), so the two hexes flanking an edge get identical values at the shared boundary —
  the symmetric **no-seam** meeting of the two half-bands survives. A `RIVER_MIN_HALF_WIDTH` px floor keeps
  the noise from severing the band (and keeps it a legible hairline at far zoom).
- **MEANDER — a domain warp, not a distance bias.** Kept (it still bends the centerline rather than
  bulging/pinching a straight one) but **capped**, per the above: `RIVER_MEANDER_CELL = 0.9` hex radii,
  `meander_width = 0.22`. The warp cell is keyed to `hex_radius`, **not** the shared px-sized `noise_cell`
  (which would make the wander's character change with zoom and only fuzz the bank). It is warped ONCE per
  fragment in world space, so both flanking hexes warp the same point → no seam.
- **ONE river growing, not two spliced.** The two class textures are deliberately different art (`00_minor`
  light/shallow-over-gravel, `01_major` dark/deep), and untreated they meet as turquoise-next-to-near-black:
  a class change read as *two waterways joining*. Two shader fixes, no art edits: (a) the class **crossfades**
  — the pass tracks the best coverage per class and mixes the two layers by
  `smoothstep(-river_class_blend, river_class_blend, cov_major - cov_minor)`, so a hex carrying both classes
  dissolves one into the other over `class_blend_width` (a pure-class hex is unaffected: the loser stays at
  `-1e9`); and (b) `river_harmonize()` pulls both layers' luma toward `RIVER_DEPTH_PIVOT`
  (`depth_compress`) and their chroma toward `RIVER_SHARED_HUE` (`tint_strength`), preserving the luma
  ORDER — Minor stays lighter, Major deeper — which is the thing that should say *bigger*.
- **NAVIGABLE-CHANNEL pass (shader), right AFTER the Minor/Major pass** (so a Major feeding a navigable
  trunk composites into it), before canopy/peaks. **This is a RENDER-ONLY change — the sim is untouched.**
  Three parts:
  - **`blend_class` is `"flat"`, not `"water"`** (a *render* eligibility class with no sim meaning — the
    sim's `WATER | FRESHWATER` tags and water movement profile are unchanged). The hex is now a **bank**
    with water painted on it, so treating it as land is correct: it takes it **out of the shore pass**
    (no beach, no foam) and lets the bank **blend softly into neighbouring flat land**, merging the
    corridor into the landscape. Its base texture (`textures/base/37_navigable_river.png`) is now the
    **BANK ground** (placeholder: a copy of `09_floodplain`; real silty-bank art lands later — do not tune
    to its colours), and its config `color` (the fallback solid + minimap pixel) is a bank tone.
  - The shore pass additionally **skips any edge with a navigable hex on EITHER side**: `blend_class`
    alone is not enough at the MOUTH, where the (now-land) bank would take a beach and the sea across from
    it would draw a **surf line across the river's mouth** — the river visibly walled off from the sea it
    drains into. A river meeting the sea is not a coast.
  - The **channel** (`river_tex` **layer 2**, `textures/rivers/02_navigable.png` — the deep teal water that
    used to be the terrain's base) is TWO kinds of stroke, unioned by the **max-coverage (min-distance)**
    pick — the same trick that rounds the Minor/Major corner joins, here fusing them into one connected
    body with rounded junctions for free:
    - **TRUNK ARMS**, at the channel's own (navigable) width: hex **CENTRE → the MIDPOINT** of each side
      the river continues through — the neighbour is **navigable** (the chain), **water** (the sea/lake it
      drains into), or a **`RiverDelta`** (the sim makes the chain's MOUTH a delta — a LAND tile — so
      without this rule the river **dead-ends one hex short of the sea**).
    - **INFLOW SPURS**, at the arriving tributary's **own Minor/Major width**: hex **CENTRE → the CORNER**
      named by `river_inflow` (all 6 checked; a mask bit needs no neighbour fetch, so it spurs even at the
      map border / across the wrap seam). The spur wears the tributary's class art and **crossfades** into
      the channel over `class_blend_width` — the edge pass's Minor→Major crossfade, for the same reason:
      one river growing, not two waterways spliced.
    - **HEAD TAPER — a trunk does not spring to full width at a hex centre.** On the **first hex of a
      chain** (the only hex with a nonzero `river_inflow`) the arms **start at the half-width of the
      WIDEST class feeding in** (max over the 6 inflow corners — Major wins if any Major lands, and the
      sim already stores the widest class per corner) and **swell to the full navigable width by the hex
      EDGE**: `half_w = mix(inflow_half_width, navigable_half_width, pow(smoothstep(0,1,t), head_taper_curve))`,
      `t` = the arm's own centre→edge-midpoint projection. Without it a hairline Minor arrived at a vertex
      and was a great river a few px later — a jump-cut, not a river. A hex with `river_inflow == 0`
      (mid-chain, or navigable from its first step) is **unchanged**: `inflow_half_width` stays the
      navigable width and the mix is a no-op — no extra per-hex branching.
      **`t` is taken from the UNWARPED point** (unlike the distance-to-centerline `t`, which must use the
      meander-warped one), and that is load-bearing: every fragment on the shared edge projects to
      **exactly `1.0`** on the arm axis (the edge line's projection onto the arm direction is the apothem,
      whatever the lateral offset), so the taper lands on **exactly** `navigable_half_width` where the
      next, constant-width navigable hex takes over — no step, no notch at the head's downstream edge. The
      warped point's projection would wander by the meander amplitude and leave one. Width is a scalar
      field of world position here, the same as `river_width_mod` / `river_bank_wobble` (both also sampled
      unwarped), and the organic machinery rides **on top of** the tapered base width, so the continuity
      guarantees survive. The taper also makes the **spur→trunk join seamless**: the trunk now leaves the
      centre at the same width the spur arrives there with.
    - **An arm is NOT keyed off `river_edges`** — that was the fat-teal-blob bug. An edge river runs ALONG
      a side; it does not flow through the side's MIDPOINT, and a trunk head can flank two or three river
      edges, so the mask-armed rule drew three fat centre→midpoint arms **at the trunk's width** and the
      hex filled with water. Water enters a trunk hex at a **vertex**, which is what `river_inflow` names.
    A navigable hex with **zero arms** (the sim should never emit one; an inflow spur is not an arm) draws
    a centre **blob** rather than a hex of bare bank, and `MapView._warn_orphan_navigable_rivers`
    `push_warning`s it (a deliberate GDScript mirror of the shader's arm rule — keep the two in step).
  - It reuses the **same organic machinery** as the edge pass — the `river_meander_warp` domain warp, the
    low-frequency `river_width_mod` swell, the `river_bank_wobble` ragged bank (all three factored into
    shared shader functions rather than copied) — and `river_harmonize`, so the trunk reads as the same
    river grown bigger. All noise is sampled in **WORLD space**, which is exactly what makes the channel
    **continuous across adjacent navigable hexes**: both hexes warp the same point and read the same width
    at their shared boundary, so the half-channels line up with no seam, pinch or gap. The **spurs ride the
    same three**, which is why a tributary's band arrives at the vertex already warped exactly as the edge
    pass warped it on the far side — the two meet without a notch.
- **Config levers** (`terrain_config.json` → `rivers` block): `minor_width` / `major_width` /
  **`navigable_width`** (the channel HALF-width as a fraction of the hex radius — `0.24`, i.e. clearly
  wider than Major's `0.09` but well short of filling the hex; softness / meander / width-variation /
  bank-noise / flow-speed are **shared with the edge classes**, not duplicated per class) /
  `softness_width` / `meander_width` / `bank_noise_width` / `class_blend_width` (fractions of the hex radius
  → px uniforms, computed in `_update_terrain_shader_quad` exactly like `blend_width` / `canopy_overhang`),
  the unitless `width_variation` / `tint_strength` / `depth_compress` / **`head_taper_curve`** (the
  exponent on the head taper's smoothstep — `0.8` ships, i.e. swell slightly EARLY; `1.0` = plain
  smoothstep, `> 1` holds the tributary's width longer then flares. An exponent, never a width, so it
  cannot disturb the exact navigable-width match at the hex edge), plus `texture_scale`,
  `river_min_radius` (the LOD floor), and `flow_speed`. Fallbacks are the `RIVER_DEFAULT_*` consts in
  `MapView.gd`.
- **River LOD is DECOUPLED from the blend LOD** (own `rivers_lod_enabled`, `radius ≥ river_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`) — a river is a landmark you navigate *by*, so it must survive
  zooming out; the mipmapped/trilinear river array keeps the thin band stable (no shimmer).
- **`set_highlight_rivers`** (the Map tab toggle) survives, repointed from the deleted polyline draw to the
  shader's `river_highlight` uniform.
- **TEXT surfacing — `ui/RiverEdges.gd`, ONE formatter, two surfaces.** Seeing the water isn't knowing
  which SIDES carry it — which is exactly what a crossing penalty will key on. `MapView._tile_info_at`
  copies the mask onto the tile dict as `river_edges` (from `tile_river_edges`; **deliberately NOT in
  `FOW_DISCOVERED_HIDDEN_KEYS`** — a river is permanent geography like the terrain label or a discovered
  Wondrous Site, so a remembered tile still reports it; never-seen tiles are already covered by the
  `unexplored` redaction), and both the **Tile card** (`Hud._tile_terrain_lines`, with the other
  terrain-intrinsic rows, before the FoW discovered early-return) and the **map hover tooltip**
  (`Hud.show_tooltip`, after `Terrain:`) render it from the same `RiverEdges.summary_lines(mask)` call.
  `RiverEdges` owns the vocabulary (classes + direction names + bit widths as named constants) and emits
  **one line PER CLASS, Major first** — `Major River: NE, NW` / `Minor River: SW` — plain `Key: Value`
  rows needing no `_format_detail_bbcode` tint case, and an **empty array on a riverless tile** so no
  empty label renders. It keeps **two direction orders**: the sim's `HEX_NEIGHBOR_OFFSETS` order
  (clockwise from E — the wire contract) DECODES the mask, and a **compass display order** (clockwise
  from NE) lists the directions within a line, because a compass reading is what a player parses.
  ui_preview: `river_tile_both` (two-class) / `river_tile_minor` (single-class) / `river_tile_none` (no row).
- **Caveat — rivers are shader-only** (same as canopy/peaks): the blend-OFF **per-hex CPU path** renders no
  rivers. That is the reference/fallback path only; the live client runs blend-on.
- Verify via `tools/map_preview.gd` State **rivers** → `map_rivers.png` (a Minor→Major edge river wandering
  west→east with corner turns, joining a NavigableRiver chain that turns corners of its own and drains to
  the eastern sea — **with a real InlandSea lake in the same frame as the control**: the lake keeps its
  beach + surf, the navigable hexes have neither, and the two must read as obviously different things) +
  `map_rivers_seam.png` (edge/corner close-up framing the class change: the band hugs the EDGE, joins are
  rounded, the two half-bands meet with no seam down the middle, Minor grows into Major) +
  `map_rivers_navigable.png` (the trunk: the Major edge river flowing INTO it, the corner turns, and the
  channel CONTINUOUS across adjacent navigable hexes) + `map_rivers_mouth.png` (the channel reaching open
  sea + its delta lobe — no dead-end, and no surf line across the mouth) +
  `map_rivers_head_minor.png` (the HEAD TAPER's own frame: a second, one-hex navigable branch fed by a
  **Minor tributary only** — its arm must start hairline at the centre and swell to the full channel width
  by the shared edge with the trunk, with **no step** there; the Major+Minor head in `map_rivers_join.png`
  is the other half of the test, starting at the **wider** — Major — width) +
  `map_rivers_farzoom.png` (decoupled LOD). The fixture generates the edge chain as the **boundary of a
  region** (hexes north of a bank row `f(x)`), which is contiguous by construction — no gaps — and turns a
  corner at every step; the navigable chain then WALKS `RIVER_NAV_STEPS` (E/SE/E/NE/E) out to the sea, so the
  trunk's arm/junction geometry is actually exercised. The river is kept in the map's **upper rows**
  deliberately: the map is cover-fit and that fit is the zoom FLOOR, so on a window wider than the grid's
  aspect the lower rows cannot be scrolled into view at all. **`RIVER_PATTERN` must stay a mostly-MONOTONE drift**: an up-down-up staircase makes
  the boundary wrap 4+ sides of the same hexagon, manufacturing a honeycomb that real hydrology (a downhill
  walk on the corner lattice) never produces — the original fixture did exactly that and made the render
  look far worse than it is.

**Texture readback fix (kept from A):** `TerrainTextureManager` retains the CPU-side layer Images
(`_layer_images`) captured once at build time; `get_terrain_image` serves duplicates from it and
**never** calls `Texture2DArray.get_layer_data()` again (a second readback returned a blank image on
some drivers, whitening the base). The `sampler2DArray` uniform is the same `terrain_textures`.

Verify via `tools/map_preview.gd` State Q → `ui_preview_out/map_biome_hard.png` (blend off, the
reference) vs `map_biome_blend.png` (Approach B on), plus `map_biome_blend_seam.png` (desert↔prairie
close-up): the flat pair blends symmetrically, prairie↔forest / forest↔ocean stay crisp, and terrain
stays aligned with the grid. **State S** (`map_repetition_after.png` + `..._zoom.png`) renders a large
detailed-rugged field (alpine id 26) beside a flat prairie band: the continuous world-space base
sampling means NO per-hex identical-repeat grid on the alpine (each hex shows a different region of the
texture, features flow across boundaries), while the prairie↔alpine seam stays a hard edge.

**Fallback considered:** a MultiMesh (one instance per hex) was the fallback if whole-map inverse-hex
alignment couldn't be matched; the splatmap alignment held, so the single-quad path was chosen (fewer
moving parts, no per-frame instance transforms). **Future:** blue-noise sample instead of hash value
noise. A **per-hex UV rotation+offset for rugged biomes** (hard-edged, so cross-edge rotation
discontinuities hide) was speced to break the texture's *own* tiling-period repeat, but the continuous
world-space base sampling alone removed the objectionable per-hex grid (verified on alpine id 26 at
`base_scale = 0.25`), so it was NOT needed. Do NOT rotate flat biomes — it would break their cross-edge
blend continuity.

---

## HUD Panel Framework (Docked PanelCards)

The HUD (`HudLayer.tscn`) owns the screen regions with one layout authority — a
`RootColumn` VBox split into `TopBar` / `ContentRow(LeftDock · center · RightDock)`
/ `BottomBar`. No panel positions itself with absolute offsets into a region;
everything is container-sized so regions never collide.

### Reserved-edge docking (4-edge, multi-reserver registry)
A docked panel does not overlap or rearrange gameplay panels — it *reserves* a
strip of one screen edge, shrinking the game area to fit beside it, as if the
window were that much smaller. The mechanism is a **reservation registry** keyed
by reserver id, so multiple panels can reserve (possibly different) edges at once:

- **`MapView.set_reserved_inset(id: StringName, edge: int, size: float)`** and
  **`Hud.set_reserved_inset(id, edge, size)`** — `edge` is a Godot `Side` const
  (`SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM`); `size <= 0` releases the reserver.
  Each stores `{edge, size}` under `id` and recomputes four per-edge totals
  (`left/right/top/bottom` = Σ of sizes whose edge matches).
- **`Main._apply_reservation(id, edge, size)`** fans a reserver's contribution out
  to both surfaces. Two reservers today: the **Inspector** (`&"inspector"`,
  `SIDE_LEFT` — `reserved_width()` / `reserved_width_changed` on show/hide + live
  drag-resize) and the **Band/City panel** (`&"band_panel"`, its currently-docked
  edge — see below).
- **`MapView`** applies the totals via three coordinated pieces:
  1. `_get_adjusted_viewport_size()` subtracts `left+right` on x and `top+bottom`
     on y, so fit, pan-clamp, draw extents, hit-testing and the minimap indicator
     all treat the remaining rect as the whole viewport.
  2. The node is translated by the **leading** insets only (`position =
     Vector2(left, top)`; trailing right/bottom just shrink the viewport), so the
     reduced coordinate space renders beside the panel(s). Because
     `get_local_mouse_position()` accounts for the node transform, clicks stay
     correct without touching any screen↔hex math.
  3. `_apply_view_clip()` (in `_draw`, via `RenderingServer.canvas_item_set_clip`)
     clips every draw command to the usable rect whenever **any** inset > 0. The
     map is **cover-fit**, so its content is larger than the reduced viewport and
     would otherwise overflow into a reserved strip; clipping confines it.
  - `_is_local_point_in_view()` bounds hit-testing to the full adjusted-viewport
    rect on **both** axes (`0 ≤ local ≤ adjusted` in x and y), so a click under a
    left/top/right/bottom strip is rejected, not just a left one.
- **`Hud`** applies the four totals to `LayoutRoot` offsets: `offset_left = left`,
  `offset_top = top`, `offset_right = -right`, `offset_bottom = -bottom`, so every
  bar and dock lives in the smaller rect.

Because the HUD, reservers, and map all sit under the same `content_scale`
transform, each reservation is a single canvas-space value that applies to all
surfaces with no per-surface scaling. Panels keep their natural docks.

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

**Migration status:** `TilePanel`, `OccupantsPanel` (the split selection cards),
`CommandFeedPanel`, and
`TerrainLegendPanel` are now `PanelCard`s (the last two dropped the bespoke
`AutoSizingPanel` height math and the legend's absolute `PRESET_TOP_RIGHT`
positioning that used to overlap the Victory panel). `StockpilePanel` and
`VictoryPanel` are still plain `PanelContainer`s (correctly container-sized, but
not yet cards). `AutoSizingPanel.gd` remains only for the Inspector.

---

## Map markers (MapView hex-icon stack UX)

Co-located hex markers no longer overlap at the hex center. Markers split into two
classes by their source array (not a predicate): **PRIMARY** = player bands, drawn by
`MapView._draw_primary_bands` over the `units`/`populations` array; **SECONDARY** = herds /
food sites / wondrous sites, placed by `MapView._compute_secondary_slots`. (Tuning consts
are grouped near the top of `MapView.gd`, after the FoW/height consts.)

- **PRIMARY — player bands** own the **center spotlight** as an offset card-stack
  (`_draw_primary_bands`/`_draw_band_stack`/`_draw_band_token`). Each band's token is its
  **settlement-stage glyph** — the opaque `settlement_stage_icon` string the sim resolves from
  `settlement_stage_config.json` (⛺ nomadic / 🛖 camp / 🏘️ village today) — drawn via the shared
  `_draw_marker_glyph` drop-shadow helper (`BAND_STAGE_GLYPH_SIZE_FACTOR`), **no faction ring or
  disc**. Ownership is carried by a **faction-colored nameplate banner** (`_draw_band_banner`,
  `BAND_BANNER_*` consts) — a short rounded bar under the token filled with the band's faction
  color, drawn for the **active (primary) card only** and LOD-suppressed below
  `ICON_MIN_DETAIL_RADIUS`. The banner is intentionally sized as the substrate for an optional
  faction/band **name label** later (text on the bar). When `settlement_stage_icon` is empty
  (pre-stage / missing snapshot — rare) the token draws a small **neutral non-circular** fallback
  marker (gray square, `BAND_FALLBACK_MARKER_*`) instead of the glyph, never a disc. The stage
  label (`settlement_stage_label`) surfaces as the Occupants roster row's hover tooltip.
  Multiple bands on one hex fan up-right: up to `BAND_STACK_MAX_CARDS` (3) cards,
  back cards **darkened** (glyph multiplied by `BAND_STACK_BEHIND_TINT` so they recede/shadow),
  the **active** band (the one whose `entity == selected_unit_id`, else the first) drawn
  full-brightness on top. The active band reads by brightness alone — there is **no per-token
  selection ring** (the hex selection outline marks the tile); `BAND_STACK_BEHIND_TINT` is the
  single lever for the recede effect (RGB<1 darkens, alpha<1 fades — swap between the two there).
  Beyond 3, a `×N` count pill folded onto the **right end of the banner** (nameplate-with-count).
  Food-days dot + the travel arrow draw on the active card only.
- **SECONDARY — herds / food sites / wondrous sites** ring the hex in **fixed edge slots**
  (`SECONDARY_SLOT_OFFSETS`, near the hex corners), computed once per frame in
  `_compute_secondary_slots` by category priority **wonder → food → herd** (sequential fill,
  so icons never jump frame-to-frame). Cap `SECONDARY_VISIBLE_CAP` (3) visible icons; extras
  collapse into a `+N` overflow chip (`_draw_secondary_overflow`). Glyphs drop the old dark
  backing disc for a 1px drop shadow (`_draw_marker_glyph`). Herd migration arrow is thinner
  and only drawn on the hovered/selected herd tile. The `×N`/`+N` pills share `_draw_count_pill`.
- **Selected + hovered hex outline** (`_draw_tile_selection_highlight`, reusing `_outline_hex`):
  a solid white hex outline on `selected_tile`, a faint one on `_hovered_tile` (skipped when
  hover == selection) — this replaces the old selection-as-marker-ring feel.
- **Select-then-cycle** (`handle_hex_click` + `cycle_index`): re-clicking the current
  `selected_tile` with >1 band advances `cycle_index` (mod band count) so the stack surfaces the
  next band on top; a fresh tile resets to the top band. `select_occupant` (roster click) syncs
  `cycle_index` to the picked band's stack position via `_cycle_index_for_unit`.
- **Zoom LOD**: below `ICON_MIN_DETAIL_RADIUS` (far zoom, tiny hexes) secondary icons + all
  count/overflow chips are suppressed; only primary tokens draw.

Verify visual changes via `tools/map_preview.gd` (`godot --path . res://tools/map_preview.tscn`
→ `ui_preview_out/map_band_stack.png` / `map_mixed_hex.png` / `map_far_zoom.png` /
`map_stage_glyphs.png` (the ⛺→🛖→🏘️ progression + empty-stage neutral non-circular fallback marker) + the existing
labor-highlight states).

## Command Targeting

Labor allocation is source-centric (assign workers to a source/role, see the **Labor
allocation UI** bullet below). The one remaining **targeting mode** is **move-band** —
picking a destination tile — replacing the old easy-to-miss "select a band…" line.

- **Selection split — Tile card + Occupants roster** (`Hud.gd`): the old single
  selection panel is now **two left-dock `PanelCard`s driven by one script**. The
  **Tile card** (`TilePanel`/`%TileDetail`, priority 10) is the *place* — terrain
  rows (Biome/Height/Tags + the gather module relabeled `Forage:`) and, on a
  food-module tile, the `%ForageAssignControls` "assign foragers" stepper. The
  **Occupants card** (`OccupantsPanel`, priority 12,
  hidden via the dock on an empty hex) is a **selectable roster** of the bands +
  wildlife on the hex, built at runtime into `%RosterList` as two sub-groups
  (`Bands (N)` / `Wildlife (N)`); each row is a `Button` hosting a mouse-transparent
  HBox — a selection accent, a **vitality dot**, name, size, and (bands) an
  activity glyph; a **wildlife** row also carries the **fauna id** as a dim meta suffix
  (`🦌 Red Deer   game_deer_07   Big game`). **A detail row never restates what its
  roster row already shows** (the same rule the Band/City panel header follows). The roster
  row IS the identity line — name + size (+ the herd's fauna id) — so every drawer dropped
  the rows that echoed it: band → `Unit` + `Size`; herd → `Herd` / `Species` / `Size`
  (the name appeared three times, the size twice); expedition → `Unit` + `Party` (`Party`
  printed the same `size` field the row's meta shows). The herd's **fauna id moved INTO the
  row** as a dim meta — it appears nowhere else in the UI and the command feed names herds
  by it, so it had to survive the `Herd:` row; nothing else was load-bearing (an expedition
  rides `_roster_units`, so `_build_band_row` already prints the very `id` its `Unit` line
  did). What's left in a drawer is only what the row can't show — herd: Biomass / Ecology /
  Husbandry / Corral / Position; expedition: Mission / Target / Policy / Phase / Carried /
  Position. **Expedition `Policy` / `Phase` keep their WORDS** — the compact
  Active-expeditions row is where the glyph vocabulary belongs; the drawer IS the
  disclosure. Below the roster,
  `%OccupantDetail` is the selected occupant's
  **detail drawer** for **herds/expeditions** (`_herd_summary_lines` +
  `%HerdAssignControls`; expedition → `_build_expedition_panel` into
  `%AllocationPanel`). **Player-band detail relocated out of the Occupants card into
  the dockable `BandCityPanel`** (see **Band/City dockable panel** below): the roster
  still lists the band, but its summary + labor allocation render in the panel, not
  the card. Selecting a row (`_on_roster_row_selected`) re-homes the
  selection and emits `roster_occupant_selected(kind, id)`; **Main forwards it to
  `MapView.select_occupant`, which moves the map selection ring** (sets
  `selected_unit_id`/`selected_herd_id`) with no hex click. A fresh tile click
  auto-selects the first occupant through the same path. The **vitality dot is
  unified** across map/roster/drawer: a band's dot uses `BandFoodStatus.color_for_days`
  (`days_of_food` → green/amber/red), a herd's uses `_ecology_tier_color`
  (`ecology_phase` → thriving green / stressed amber / collapsing red), sharing the
  exact `HudStyle` HEALTHY/WARN/DANGER constants. Non-player bands list with a neutral
  dot and no allocation panel (their larder/orders aren't ours to see). (The Tile card
  has no camp action — the `found_camp` command was removed end-to-end.)
- **Labor allocation UI** (`Hud.gd`, Early-Game Labor slice 3b — `docs/plan_early_game_labor.md`):
  the band is a **labor pool** whose working-age workers are assigned source-centrically to
  in-range sources/roles. There is **exactly one player band today**, captured each snapshot
  into `_player_band` (first player-faction cohort in `update_band_alerts`); assign/move/clear
  all target it. Every player band is also collected into `_player_bands`, which backs the
  **band-picker dropdown** on the herd/tile assign controls (see `%HerdAssignControls` /
  `%ForageAssignControls` below) — an assignment explicitly names WHICH band supplies the
  workers (built for N even though only one exists live). Three runtime-built control sets replace the retired single-task Scout/Cancel,
  Hunt/policy, and Forage buttons:
  - **`%AllocationPanel`** (band drawer, player band only, `_build_allocation_panel`): reads as a
    "current actions" report — a `Population <size> · Workers <working_age> (Idle <n>)` line (spells
    out that only the ~16 working-age labor, not the 30 people — children/elders are dependents;
    `WORKERS_HEADER_FORMAT`, idle from `_effective_idle` so it counts optimistically), a
    **Current actions** section with one `−/+` **worker-stepper** row per staffed Forage tile / Hunt
    herd (from the cohort's `labor_assignments`; an empty-state hint when none). **A row states its
    policy and its status as GLYPHS, not words** (`🌰 Forage (27, 26) +0.48 /turn  ♻  ●`) — the old
    `[sustain]` / `· pending` word-tags were long and, for pending, redundant with the amber tint.
    Both come from the one glyph registry, `FoodIcons` (`for_policy` / `for_status`; see the
    **action-status vocabulary** header block in `Hud.gd`), and the WORDS move into the row tooltip
    (policy name + its existing `FORAGE_POLICY_HINTS`/`HUNT_POLICY_HINTS` behaviour hint, plus the
    status in words), composed WITH the tooltip the row already carried (yield readout, overstaffing
    explanation, click-to-focus hint). Two orthogonal layers: **status** = what the action is doing
    (a confirmed local forage/hunt row has no sim phase — it is simply `working` `●`), and
    **`pending`** = a state of the ORDER (composed locally, not yet acknowledged; it rides on ANY row,
    is a modifier rather than a phase member, wins the glyph slot with `○`, and keeps the amber label
    tint). The policy glyph is read off the assignment's `policy` field (populated for forage too); an
    older snapshot with no policy falls back to no glyph. **Each source row headlines its per-turn food yield**
    (`… +0.31 /turn`, the assignment's `actual_yield`), with a WARN-tinted `⚠` **overdraw flag** when
    `actual > sustainable + ε` (`OVERHUNT_EPSILON`). A Sustain source gathers at its renewable ceiling
    (`actual == sustainable` → no flag, reads `… · renewable`); a Surplus/Market/Eradicate **forage
    patch** or an over-hunted herd pushes `actual` above `sustainable` → the flag trips (forage is no
    longer hardcoded renewable now that the policy axis can decline a patch). A
    `tooltip_text` spells out actual-vs-sustainable. **Each source row also flags overstaffing** — a
    WARN-tinted `· only N of M working` note (`OVERSTAFF_NOTE_FORMAT`) when `workers > workers_needed`
    (and `workers_needed > 0`), i.e. the source's take was capped at its ceiling so the surplus workers
    idled HERE and should be reassigned; the `tooltip_text` (`OVERSTAFF_TOOLTIP`) explains it. This is
    **orthogonal to the ⚠ overdraw flag** and deliberately NOT the same glyph: overdraw is *ecological*
    (taking past regrowth), overstaffing is *labor* (wasted workers) — a source can be overstaffed while
    perfectly sustainable (every policy has a ceiling), or overdrawn while fully used. `workers_needed
    == 0` (rehydrated/older snapshot, or a pending optimistic assign) means "unknown" → no note, never a
    wrong one. Both the ⚠ and the note are rendered by `_build_worker_stepper` (`warn` / `note` params)
    off one `_source_yield_readout`, so Forage and Hunt rows share the logic.
    **Each source row leads with its resource glyph** — `FoodIcons.for_site(module)` for a Forage
    row (resolved from `_food_module_by_tile`, the snapshot `food_modules` array pushed by `Main` →
    **`Hud.update_food_modules`**, keyed by tile) and `FoodIcons.for_herd(species)` for a Hunt row —
    the SAME icon the map marker draws, so a source reads identically in the panel and on the map. An
    unresolvable module renders the row bare (no fallback sprig).
    **Each source row's LABEL is clickable — it jumps the map to the source being worked.**
    `_build_worker_stepper`'s optional `on_focus_source` Callable turns the label into an inline link
    Button (`HudStyle.apply_link_button` — plain at rest, hover tint + `SIGNAL` text + pointing-hand
    cursor, a far tighter padding than the boxed ghost chrome); it is a *separate child* from the
    `−`/`+` stepper, which is untouched, and the count stays right-aligned. Both handlers route
    through `_focus_labor_source` — the SAME path the Active-expeditions rows and the turn-orb
    "Jump →" use: `alert_focus_requested` → `MapView.focus_and_select_tile`, plus (herd only)
    `roster_occupant_selected` → `MapView.select_occupant` so the herd's own drawer opens rather than
    whatever occupant the hex auto-selects; `_panel_band` is restored afterwards, so focusing a hex
    that hosts another band can't hijack the panel. **Forage** jumps to the assignment's
    `target_x/target_y` (a patch is a fixed tile). **Hunt** deliberately does NOT — herds MIGRATE, so
    `_focus_hunt_source` resolves the herd's **live** tile from `_world_herds` via `_find_world_herd`
    (the Hud mirror of `MapView._herd_by_id`, which the hunted-herd ring already resolves through),
    falling back to the assignment target only when the herd is unknown. `_world_herds` is the
    snapshot `herds` array, pushed each snapshot by `Main` → **`Hud.update_herds`**; it also backs
    `_herd_label_for_id`'s new fallback, so an off-hex hunted herd reads "Red Deer" instead of the raw
    `game_deer_07` id. **Scout/Warrior are band-wide roles with no tile → plain, non-clickable
    labels.** Verified by `band_panel_preview` state `band_panel_source_row_hover` (the harness
    force-hovers the Hunt link, so the affordance shows in a static frame).
    `actual_yield`/`sustainable_yield`/`workers_needed` are decoded per assignment in
    `native/src/lib.rs` (inside
    `labor_assignments`); the band-level food flow (net rate + Gathered/Hunted/Eaten breakdown) lives
    on the **Food summary line**, not here — see "Band food status". Then a **Band roles**
    section with the always-shown **Scout** + **Warrior** rows (even at 0), each with a one-line hint so
    the `−/+` steppers read as "this is how you staff this standing role" (Scout's hint reads "Extends
    the band's sight — more scouts see further"; more staffed scouts extend the band's actual sight
    range, so the effect shows directly in the fog, not as a map-action or a reveal disc). Then
    **Move** / **Clear all**.
    Each stepper re-sends `assign_labor_requested` with the new count (0 removes); `+` is gated on idle.
  - **Optimistic pending feedback** (slice 3b UX): assigning workers or moving the band shows
    immediately, before the next snapshot. `_emit_assign_labor` / `_try_dispatch_pending_move_band`
    record a HUD-local **pending** entry per band entity (`_pending_labor[entity] = {turn, assign:{key→…},
    move:{x,y}}`) and re-render. In the panel, a pending source row reads **amber with the `○` pending
    glyph** (the words live in its tooltip — "Pending — starts when you advance the turn"; the amber
    stays the primary signal, tying the row to the amber pending hex on the map) and the header
    **Idle** counts optimistically (`_effective_idle` = working-age − effective
    assigned, overlaying pending). **Reconciliation is turn-based:** each pending entry is tagged with the
    snapshot `turn` (header tick, set in `update_overlay`); `_reconcile_pending` (called from
    `update_band_alerts` each snapshot) drops entries issued on an OLDER turn — a newer-turn snapshot is
    authoritative confirmation and cleanly absorbs server-side clamping (the snapshot shows the real
    count). Pending is emitted to MapView via `labor_pending_changed` → `set_labor_pending`.
  - **Selected-band map highlights** (`MapView._draw_band_work_highlights`, drawn when a player band
    is selected, cleared on deselect): the **worked forage tiles** (strong green fill on each
    `forage` assignment's `target_x/y`), the **work-range ring** (thin cyan outline on every tile
    within `work_range`, replicating the sim's true **odd-r hex distance** `hex_distance_wrapped`
    via `MapView._hex_distance` — a real hexagonal ring of 19 tiles at range 2, so highlighted ==
    actually-assignable; the old Chebyshev square wrongly lit its diagonal corners, which are 3
    hex-steps away), and the **hunted
    herds** (red ring on the herd tile + a band→herd link, drawn wherever the herd is since hunt reach
    = `work_range` + leash). **Per-source yield annotations** (`_draw_yield_label`): each staffed forage
    tile / hunted herd is labeled with its `actual_yield` (food/turn, from the assignment inside
    `labor_assignments`) as a small drop-shadow number above the tile center (reusing `_draw_marker_glyph`),
    food-income **green**; a source that overdraws (`actual_yield > sustainable_yield + ε`, reusing the
    panel's overdraw test) reads **WARN amber + a `⚠`** — an over-hunted herd, or a non-Sustain forage
    patch now that the forage policy axis can decline one (a Sustain forage gathers at regrowth, so it
    stays green). The label sits on a **dark rounded banner/pill plate** (`_draw_pill_plate`, the shared
    pill chrome extracted out of `_draw_count_pill` — the `×N`/`+N` badges draw the same primitive):
    bare drop-shadowed text washed out on the light tan biomes (prairie/desert), so the plate is sized to
    the MEASURED text+glyph run plus symmetric padding (`YIELD_LABEL_PLATE_PAD_FACTOR`, a fraction of the
    font size) and centered on the label's existing anchor, near-black + slightly translucent
    (`YIELD_LABEL_PLATE_BG`) so the terrain still reads through. The
    label font scales with the hex radius (clamped) and the whole annotation (plate included) is
    **LOD-suppressed below
    `ICON_MIN_DETAIL_RADIUS`** (like the secondary markers) so far zoom stays clean. Scout/Warrior
    produce no food → no label. **The labels are DEFERRED to the very end of `_draw`** — they are an
    annotation OVER the map, and drawn inline in the highlight pass they were painted over by every
    later layer (the dashed-amber pending overlays, the band→herd links, the hunted-herd rings, and the
    secondary herd/food glyphs — a deer glyph landing squarely on the number). The highlight pass now
    `_queue_yield_label`s each request into `_deferred_yield_labels` (cleared at the top of
    `_draw_band_work_highlights`, before its early-outs) and `_flush_yield_labels()` renders the batch
    as the LAST draw call, after the markers/rings/links/pending/targeting. The LOD gate stays at the
    QUEUE site (`show_yields`), so a far-zoom label is never queued and deferral can't bypass the
    suppression. Guarded by `map_preview` state `map_band_label_overlap` (a herd parked ON a worked
    forage tile + a pending hunt dashing across the hunted herd's label) and `map_band_yield_farzoom`. **Scouting draws no map highlight** — staffed scouts extend the band's
    real sight range (visible directly in the fog as a wider Active radius); the old faint-blue scouted
    disc was removed because `scout_reveal_radius` no longer means a reveal-disc radius — it now carries
    the band's effective sight-range bonus (extra tiles beyond base, `0` when no scouts), which the
    client can't turn into a true ring without the server-side `base_range`. New snapshot fields
    `work_range` / `scout_reveal_radius` are decoded in `native/src/lib.rs population_to_dict` and flowed
    onto the MapView unit marker in `_rebuild_unit_markers` (alongside `labor_assignments`);
    `scout_reveal_radius` is still carried (it documents the field) but no longer drawn. **Optimistic pending**
    actions for the selected band draw in a distinct **dashed-amber** style (`_draw_band_pending`, fed by
    `set_labor_pending`) — the pending forage tile, the pending hunted herd (dashed ring-hex + dashed
    band→herd link), and the pending move destination (dashed hex + dashed link) — clearly apart from the
    solid confirmed styles, cleared when the snapshot confirms.
  - **Travel destination** (`MapView._draw_travel_destination`, drawn for the selected traveling unit —
    band OR expedition — from `_draw_band_work_highlights`): when the unit reports `is_traveling`, a
    thin cyan line runs from its tile to the destination hex plus a steady (non-pulsing) cyan target
    reticle on it. The target coords (`travel_target_x` / `travel_target_y`, `uint`, `0,0` and ignored
    unless `is_traveling`) are decoded in `native/src/lib.rs population_to_dict` and flowed onto the
    marker in `_rebuild_unit_markers`. **Wrap-aware:** the target is brought into the band's effective
    column frame via `_wrapped_col_delta`, so the line follows the SHORT (possibly seam-crossing)
    wrapped path the sim actually takes rather than shooting the long way across the map. Only the
    selected unit's destination draws (no clutter). Covered by `marker_field_guard`
    (`travel_target_x`/`travel_target_y`/`is_traveling`) and `map_preview` states `map_travel_band` /
    `map_travel_seam` (seam-crossing) / `map_travel_expedition`.
  - **Band-picker dropdown** (`_build_band_picker`, on BOTH assign controls, above the worker
    stepper so it reads "which band → how many workers"): a `Band:` `OptionButton` listing every
    `_player_bands` cohort by positional name ("Band N", via `_band_display_name`; the cohort has
    no label field), item metadata = the band `entity`. The selection is the **actor band**:
    `_hunt_assign_band` / `_forage_assign_band` hold the picked entity (defaulting to
    `_resolve_assign_band()` when the selected source changes, else persisted across re-renders);
    the worker stepper's cap is that band's `_assignable_hunt_workers` / `_assignable_forage_workers`
    (its `idle_workers` + any it already staffs on that source, so re-editing isn't capped below
    current staffing), and the Assign emit + optimistic pending key off the picked band. Switching
    the dropdown re-caps the stepper and re-renders. Always shown (single-item with one band, so the
    actor is explicit). Lists **all** player bands — in-range filtering (Forage `work_range` / Hunt
    `work_range` + leash) is deferred to the multi-band slice (needs hunt-leash reach in the snapshot).
  - **`%HerdAssignControls`** (herd drawer, huntable herds, `_build_herd_assign_controls`): the
    band-picker, then a **distance-aware** "Assign hunters" **compose** control — a `−/+` worker/party
    count (`_hunt_assign_count`) + a sustain/surplus/market/eradicate **policy picker**
    (`_build_policy_picker`, `_hunt_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`). The
    button + command switch on the **wrap-aware hex distance** from the **SELECTED band's** own tile
    to the herd vs that band's **`hunt_reach`** (= `work_range` + hunt leash, decoded as `hunt_reach`
    and flowed onto the marker): **within reach** → a `Hunters` stepper + **"Assign Local Hunt"** →
    `assign_labor hunt <herd_id> <policy> <workers>`; **beyond reach** → a `Party` stepper (cap
    `min(idle_workers, max_expedition_party_size)`) + a distance hint + **"Send Hunting Expedition"** →
    `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> <policy>` (emitted directly, no
    herd-targeting step — the herd is already selected). Every part of the decision (distance, reach,
    band-entity target) keys off the band the picker selects, explicitly threaded — never the faction's
    default band. Distance uses Hud-local mirrors of MapView's odd-r `_hex_distance` /
    `_wrapped_col_delta`, fed grid width + wrap via `Hud.set_grid_dimensions` (Main forwards the
    snapshot `grid` key). Compose state re-seeds from current staffing when the selected herd changes.
    Covered by ui_preview states `herd_verbs` (local) / `herd_hunt_expedition` (single far band) /
    `herd_hunt_band_near` + `herd_hunt_band_far` (two bands, one herd — picker flips local↔expedition).
  - **`%ForageAssignControls`** (Tile card, food-module tiles, `_build_forage_assign_controls`): the
    band-picker, then a sustain/surplus/market/eradicate **policy picker** (`_build_policy_picker`,
    `_forage_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`) with a **forage-appropriate**
    behaviour hint (`FORAGE_POLICY_HINTS` — "gather at the patch's regrowth" etc., NOT the herd-cull
    hints), an "Assign foragers" Foragers `−/+` count (`_forage_assign_count`), and a
    **range-aware** **Forage** button → `assign_labor forage <x> <y> <policy> <workers>` (the policy is
    the optional token the sim accepts before the worker count; the policy persists across re-renders
    and re-seeds from the tile's current forage policy via `_policy_for_forage` when the tile changes).
    Mirrors `%HerdAssignControls`' policy affordance. Foraging is
    **stationary** gathering — there is **no forage-expedition fallback** — so the button gates on the
    **wrap-aware hex distance** from the **SELECTED band's** own tile to the forage tile vs that band's
    **`work_range`** (the plain `workRange` field, NOT `hunt_reach`; already decoded/on the marker):
    **within range** → enabled **Forage**; **beyond range** → the button is **disabled** + an
    out-of-range hint (`"(x,y) is N tiles away — beyond this band's forage range (R)"`), no alternative.
    Reuses the same `_hex_distance_wrapped` / `_band_tile` / grid-dim plumbing and explicit
    selected-band threading as the herd hunt. Covered by ui_preview states `food_tile` (in range) /
    `food_forage_out_of_range` (single far band) / `food_forage_band_near` + `food_forage_band_far`
    (two bands, one tile — picker flips enabled↔disabled).

  - **Cultivate / Corral — the INVESTMENT rungs** (on BOTH assign controls; the sim's
    `FollowPolicy::Cultivate` / `Corral`): the extractive four take from a wild source; these two pay
    an **up-front cost** — while the patch is being prepared / the pen built, the source yields only
    its `ceilingCultivate` / `ceilingCorral` dip yield, then flips to the much higher `tendedYield` /
    `corralYield`. **Kind-specific and the sim rejects the cross pairing**: Cultivate is forage-only
    (`FORAGE_POLICY_OPTIONS`), Corral hunt-only (`HUNT_POLICY_OPTIONS`) — and Corral is offered on a
    **local hunt only** (a hunting expedition follows the herd and builds no pen, so it keeps the
    extractive `LABOR_HUNT_POLICIES`, as does the send-expedition launch picker).
    - **Disabled-with-reason-AND-remedy, never hidden.** `_build_policy_picker(on_pick, selected,
      options, gates)` renders a gated option **greyed, with every reason in the tooltip (one per
      line) AND spelled out under the row**, so the player discovers the rung and its prerequisites
      *before* acting. `gates` maps **policy → `Array[String]` of reasons** (read only through
      `_gate_reasons`); **1 reason** renders the compact one-liner `🌱 Cultivate — <reason>`, **2+**
      render a `🐄 Corral needs:` header + one indented `· <reason>` bullet each (a reason now carries
      its remedy, so two on one line would not fit).
      **Each reason states what's missing + live progress + the action that fixes it** — naming the
      prerequisite alone told the player a door was locked without saying where the key is. All three
      tracks are taught by the same action, so the remedy names the **Sustain** glyph (pulled from
      `FoodIcons.POLICY_ICONS`, i.e. literally the button beside it): `Cultivation knowledge 55% — ♻
      Sustain-forage a Thriving patch to learn it` / `Herding knowledge 35% — ♻ Sustain-hunt a Thriving
      herd to learn it` / `Herd 40% tamed — ♻ Sustain-hunt this Thriving herd to finish taming it`.
      The **patch-ecology** gate is a *stock* condition, not a policy one — a fully staffed Sustain
      takes the whole regrowth and holds a Stressed patch Stressed forever — so its remedy is the
      opposite advice: `Patch is Stressed — ease workers off and let it regrow to Thriving` (live
      `patch_ecology_phase`, capitalized). Gates (`_forage_policy_gates` / `_hunt_policy_gates`,
      mirroring the sim's `assign_labor` validation): Cultivate needs faction `cultivation >= 1.0`
      **and** a Thriving patch; Corral needs `herding >= 1.0` **and** `domestication >= 1.0`. A gated
      rung can never be the composed policy (re-validated every render, since a patch can leave
      Thriving under a standing selection). **Known gap:** `_hunt_policy_gates` does NOT check herd
      **ownership** — the sim's domestication track is per-faction, so a herd domesticated by ANOTHER
      faction would enable Corral client-side while the sim rejects the assign.
    - **The forecast states the deal.** `_forecast_inputs` maps an investment policy's ceiling to the
      DIP yield and additionally returns its `payoff`; `_forecast_yield_row` then reads
      **`Preparing: +0.24 /turn → then +1.20 /turn`** instead of `Expected yield:` — both halves
      scaled by the band's `output_multiplier` like every other forecast. The managed source reports
      per-worker == ceiling, so the stepper caps at **1 worker**, as it should.
    - **Progress meters.** The tile card's `Cultivation N%` row is joined by the herd drawer's
      `Corral: Building N%` (`corralProgress`, `_corral_label` / `_corral_value_hex`), flipping to the
      SIGNAL-tinted `🐄 Corralled` once penned — the animal twin of `🌾 Tended Patch`.
    - **Knowledge-unlock nudge.** `_ingest_intensification` keeps the per-faction tracks and fires a
      ONE-SHOT command-feed note the turn a track crosses to complete ("Cultivation learned — The
      Cultivate policy is now available on Thriving patches."). Only a real `<1 → >=1` transition
      fires it (a track already complete on the first snapshot / a rehydrated save is silent), and
      only for the player faction; the announced set is keyed per faction+track.
    - Wire fields decoded in `native/src/lib.rs` (snapshot + delta, both paths share the same
      `herds_to_array` / `forage_patches_to_array`): `ForagePatchState.ceilingCultivate` /
      `tendedYield` → `patch_ceiling_cultivate` / `patch_tended_yield` on `tile_info` (and in
      `FOW_DISCOVERED_HIDDEN_KEYS`); `HerdTelemetryState.ceilingCorral` / `corralYield` /
      `corralProgress` → bare keys on the herd dict.
    - ui_preview: `forage_cultivate` (enabled + the Preparing→then forecast + the feed nudge) /
      `forage_cultivate_locked` (1 reason — knowledge 55% + its Sustain-forage remedy) /
      `forage_cultivate_stressed` (1 reason — the ease-off-and-regrow ecology remedy) / `herd_corral`
      (enabled + `Corral: Building 40%`) / `herd_corral_locked` (1 reason — herd 40% tamed) /
      `herd_corral_locked_both` (**2 reasons** — the `🐄 Corral needs:` header + bullets layout).
  - **Pre-commit yield forecast** (on BOTH assign controls): setting up a forage/hunt assignment used
    to give no feedback — you staffed 6 workers, committed, advanced a turn, and only then learned 5
    were wasted. The sim now streams, with **identical field names** on `ForagePatchState` and
    `HerdTelemetryState` (`perWorkerYield` / `ceilingSustain` / `ceilingSurplus` / `ceilingMarket` /
    `ceilingEradicate` — all food/turn at the source's **current biomass**, exported at
    `output_multiplier = 1.0`), enough to compute the take *while composing*:
    `expected(workers, policy) = min(workers × per_worker_yield, ceiling[policy])` (the ceilings are
    already biomass-clamped, so that `min` IS the take) and `max_useful_workers(policy) =
    ceil(ceiling[policy] / per_worker_yield)`. Decoded in `native/src/lib.rs`
    (`herds_to_array` bare / `forage_patches_to_array`, both the snapshot + delta paths), carried to
    the controls via the herd dict and — for the patch — via `forage_patch_lookup` → `_tile_info_at`
    as `patch_`-prefixed keys (in `FOW_DISCOVERED_HIDDEN_KEYS`, so a remembered tile redacts them).
    Two affordances, both recomputed on **every** stepper *and* policy change (both already re-render
    the controls): a live HEALTHY-green **"Expected yield: +X.XX /turn"** row (scaled by the
    **selected band's `output_multiplier`** — the sim exports at 1.0), and a **worker-stepper cap** of
    `min(idle-worker cap, max_useful_workers(policy))` — the `+` goes dead at the cap and, when
    max-useful is the binding one, a `"max N worker(s) useful here — more would be idle"` note
    explains why (a Market/Eradicate ceiling exceeds Sustain's, so switching policy moves the cap).
    Shared helpers `_forecast_inputs` / `_max_useful_workers` / `_expected_yield` /
    `_forecast_worker_cap` / `_forecast_yield_row` serve both controls. **Guards:**
    `per_worker_yield == 0` (dead-season tile, or an older snapshot with no forecast fields) → no row,
    no cap, never a divide-by-zero; a **tended patch / corralled herd** reports every ceiling ==
    `per_worker_yield` ⇒ max-useful 1, policy irrelevant. Applied to the **local hunt only** — an
    expedition accumulates toward a carry cap over several turns of travel, so the herd's per-turn
    ceiling is not the bound on its party size. The **post-hoc** `"· only N of M working"` overstaffing
    note on the allocation rows stays: it still covers a source whose biomass FELL after you staffed
    it. ui_preview: `food_tile` / `forage_forecast_cap` / `tended_tile` / `herd_hunt_band_near`.

  All emit `assign_labor_requested(payload)` (payload: `faction/band/kind/workers/x/y/herd_id/policy`);
  `Main._on_hud_assign_labor` formats the `assign_labor …` text command. **Clear all** emits
  `cancel_order_requested` (the repurposed `cancel_order` = clear-all → fully idle). The roster
  glyph keeps reading the still-populated `activity` (now the largest-worker
  kind: `idle|forage|hunt|scout|warrior`) and `hunt_mode`. `harvestTask`/`scoutTask` are always
  null server-side and no longer decoded. **Convenience shortcut:** double-clicking a herd on the
  map (`MapView.herd_quick_hunt_requested` → `Main._on_map_herd_quick_hunt` → `Hud.quick_assign_hunters`)
  assigns the player band's idle workers to hunt that herd at Sustain — a no-op with a command-feed
  note when there are no idle workers (never silently nothing).
- **Herd ecology readout** (`Hud.gd` `_herd_summary_lines`): the selection panel shows
  the group's `ecology_phase` (snapshot `HerdTelemetryState.ecologyPhase`) as an
  **Ecology** row — a neutral "Thriving", or a warned "⚠ Stressed" / "⚠ Collapsing"
  that `_format_detail_bbcode` tints amber / red (`_ecology_value_hex`, `HudStyle.WARN_HEX`
  / `DANGER_HEX`). A `Collapsing` herd has been overhunted past the point of no return and
  is crashing to local extinction (see `core_sim` Fauna & Wild Game — depensation collapse).
- **Clear-all / move-band** (`Hud.gd`, Early-Game Labor slice 3b): the single-task
  Scout/Cancel affordance + its optimistic `_pending_transition_bands` machinery were
  **retired** with the labor-allocation model. There is no longer a band-global task to
  cancel — you staff a source down to 0 (`assign_labor … 0`). The **Clear all** button on
  `%AllocationPanel` emits `cancel_order_requested`; `Main._on_hud_cancel_order` sends the
  **repurposed** `cancel_order <faction> <band_bits>` (now clears ALL assignments → fully
  idle). **Move band** is the one remaining targeting flow: the panel's **Move** button
  (`_on_move_band_pressed`) enters tile-targeting (`_pending_move_band` → `_current_targeting_info`
  returns `command: "move", need: "tile"`), the top-centre banner reads "MOVE … click a
  destination tile", and the destination click (`_try_dispatch_pending_move_band`, via
  `show_tile_selection` / `notify_hex_selected`) emits `move_band_requested(payload)` →
  `Main._on_hud_move_band` → `move_band <faction> <band> <x> <y>`. Esc/right-click cancel
  via `cancel_active_targeting` → `_cancel_pending_move_band`.
- **Herd husbandry readout** (`Hud.gd` `_herd_summary_lines`): when a herd's
  `domestication` (snapshot `HerdTelemetryState.domestication`, 0–1) is above 0, a
  **Husbandry** row shows "Domesticating N%" while it's being tamed and "🐄 Domesticated"
  (SIGNAL tint via `_husbandry_value_hex`) once fully domesticated. Progress builds while a
  band Sustain-follows a Thriving herd; the `domesticate` command claims it early (see
  `core_sim` Fauna & Wild Game — Domestication / husbandry).
- **Herd corral readout** (`Hud.gd` `_herd_summary_lines`): when a herd's `corralled`
  (snapshot `HerdTelemetryState.corralled`, decoded beside `domestication` in
  `native/src/lib.rs herds_to_array`) is true, a **Corral** row shows "🐄 Corralled"
  (SIGNAL tint). The herd end of the intensification ladder — a penned, domesticated herd.
  While the pen is still being built under the Corral policy (`corralProgress`, decoded as
  `corral_progress`; `0 < p < 1`) the SAME row reports the meter — "Corral: Building 40%" —
  the animal twin of the tile card's "Cultivation N%". See the Cultivate/Corral investment-rung
  bullet under **Labor allocation UI**.
- **Forage-patch cultivation readout** (`Hud.gd` `_tile_terrain_lines`): a forage tile's
  intensification state, mirroring the herd Husbandry row. `native/src/lib.rs
  forage_patches_to_array` decodes `foragePatches[]` (`ForagePatchState`) into both the
  snapshot and delta dicts under `forage_patches`; `MapView.display_snapshot` ingests it into
  the tile-keyed `forage_patch_lookup`, and `_tile_info_at` cross-refs it onto `tile_info`
  (`cultivation_progress` / `is_cultivated` / `patch_ecology_phase` / `patch_has_owner` /
  `patch_owner` / `patch_biomass` / `patch_carrying_capacity`, all in `FOW_DISCOVERED_HIDDEN_KEYS`
  so a remembered tile redacts them). The
  card shows a **Cultivation** row: "N%" while the patch is being tended, "🌾 Tended Patch"
  (SIGNAL tint via `_cultivation_value_hex`) once `is_cultivated`. See `core_sim`
  intensification ladder — cultivation.
  It also shows an **Ecology** row (`patch_ecology_phase`) for **every** tile carrying a patch —
  cultivated or not, directly under **Forage biomass**. The phase gates whether cultivation can
  accrue at all, so it is the tile's headline condition; it is deliberately **not** gated on
  `is_cultivated` (it was, which hid it on exactly the ordinary forage tiles that needed it).
  Named and rendered **identically to the herd's Ecology row** — same `_ecology_phase_label`
  (neutral `Thriving`, warned `⚠ Stressed` / `⚠ Collapsing`) and the same `_ecology_value_hex`
  amber/red tint applied by `_format_detail_bbcode`, which now keys one shared `"Ecology"` case
  for both surfaces. The module's internal `seasonal_weight` is **not** printed on the `Forage:`
  row (it is a yield coefficient, meaningless to the player); it still drives the sim's yield.
  ui_preview: `food_tile` (Thriving) / `food_tile_stressed` (⚠ Stressed) / `tended_tile`.
  It also shows a **Forage biomass** row — `Forage biomass: 84 / 120` (`biomass` /
  `carryingCapacity`, decoded in `forage_patches_to_array`) — the patch counterpart to a herd's
  **Biomass** row, so a foraged patch reads like wild game does ("how much there is"). Foraging draws
  the biomass down and it regrows logistically toward the capacity (sim default 120). Rendered only
  when `patch_carrying_capacity > 0`, so a plain food-module tile with no patch stays bare.
- **Sedentarization meter** (`Hud.gd` `update_sedentarization`, dispatched from `Main.gd`):
  the player faction's `SedentarizationState.score` (snapshot `sedentarization[]`) shows as a
  compact top-bar block-glyph meter (`▰▰▰▰▰▱▱ 62/100 · soft`, `SedentarizationLabel` in
  `TurnBlock`), tinted amber (soft) / cyan (hard) by stage and hidden until the score is
  meaningful. The soft/hard threshold prompts themselves arrive in the command feed
  (`CommandEventKind::SedentarizationPrompt`). See `core_sim` Campaign Loop — Sedentarization.
- **Intensification-knowledge meters** (`Hud.gd` `update_intensification`, dispatched from
  `Main.gd`): the player faction's Cultivation / Herding knowledge from
  `IntensificationKnowledgeState` (snapshot `intensification_knowledge[]`, decoded in
  `native/src/lib.rs intensification_knowledge_to_array` into snapshot + delta dicts) shows as a
  compact top-bar block-glyph meter mirroring the Sedentarization one (`Cultivation ▰▰▰▱▱▱
  learning · Herding ✔ known`, `IntensificationLabel` in `TurnBlock`). Each track (0..1 progress)
  is hidden until the faction begins learning it (the snapshot row is sparse) and reads "✔ known"
  once complete; the label tints cyan when every learned track is fully known, else neutral ink.
  See `core_sim` intensification ladder — knowledge.
- **Demographics readout** (`Hud.gd` `update_demographics`, dispatched from `Main.gd`): the player
  faction's age structure from `PopulationDemographicsState` (snapshot `demographics[]`) shows as a
  top-bar line (`Pop 100  👶34 🛠51 🧓15  dep 96/100`, `DemographicsLabel` in `TurnBlock`) — total
  head-count, the three brackets, and the **dependency ratio** `(children+elders)/working` per 100
  workers, tinted amber when dependents outnumber workers / cyan on a healthy labor surplus. Hidden
  until the faction has population. See `core_sim` Campaign Loop — Population & Demographics.
- **Wondrous Sites (discovered)** (snapshot `discovered_sites[]`, per-faction like
  `sedentarization`/`demographics`; each entry `{faction, sites:[{x,y,site_id,category,display_name,
  glyph}]}` with `category`/`display_name`/`glyph` resolved server-side — client renders the provided
  glyph/name, no client-side site config; undiscovered sites are never sent). Decoded in
  `native/src/lib.rs discovered_sites_to_array` into both the full-snapshot and delta dicts under
  `discovered_sites`. Surfaced three ways, all filtered to `PLAYER_FACTION_ID`:
  (1) **Top-bar readout** (`Hud.gd update_discoveries`, dispatched from `Main.gd`): a compact
  `◈ Discoveries N  <distinct glyphs>` line (`DiscoveriesLabel` in `TurnBlock`, cyan), hidden when 0.
  (2) **Map glyph markers** (`MapView.gd`): ingested into `discovered_sites` + a `discovered_site_lookup`
  (`Vector2i → site`) mirroring `food_modules`; `_draw_discovered_site` draws the site's `glyph` (drop-shadow,
  no backing disc) in a fixed **edge slot** via the shared secondary-marker system (see Map markers below),
  gated on `_visibility_state_at != "unexplored"` (persists on any known/remembered tile — Discovered OR
  Active — since a site is permanent geographic knowledge, unlike the Active-only food-site/herd markers).
  (3) **Tile card** (`Hud._tile_terrain_lines`): a `Site: <display_name>` row (from `_tile_info_at`'s
  `discovered_site_lookup` cross-ref → `site_name`), shown before the FoW discovered early-return since
  it's known knowledge. The server also pushes a `SiteDiscovered` command-feed entry, which renders
  generically via the server-provided `kind`/`label` (no client kind→label map needed). See
  `core_sim` — Wondrous Sites.
- **Band food status** (snapshot `PopulationCohortState.daysOfFood` / `activity` / `supplyNetworkId` /
  `stores[]`, decoded in `native/src/lib.rs` `population_to_dict` as `days_of_food` / `activity` /
  `supply_network_id` / `stores{item:qty}`): the green/amber/red warn·critical thresholds and the
  day→color mapping live in one place, `ui/BandFoodStatus.gd` (config `src/config/band_status_config.json`,
  key `food_days.{warn,critical}`; `999` = not food-limited → ∞). Surfaced three ways:
  (1) `MapView._draw_band_status` draws a food-days dot on each **player** band
  (`_is_player_unit`); (2) `Hud._band_food_line` adds a `Food  <N>  (<D> days)`
  row to the band selection panel, tinted by the thresholds via `_format_detail_bbcode`
  — **player bands only** (`_is_player_unit`, the same gate Morale uses, and for the same
  reason: **a rival's larder is not ours to see**). A foreign cohort carries no
  `days_of_food`/`stores` on the wire, so rendering the row for one **fabricated knowledge**
  — a healthy-green `Food 0 (∞)`, the UI claiming we'd counted a larder we cannot observe.
  A foreign band's drawer now shows only what is honestly observable from outside: its
  **Position**, plus the name/size on its roster row. The reset of the disclosure context
  (`_food_flow_present` / `_selected_band_food_days` / `_disclosure_state`) lives at the top
  of `_unit_summary_lines`, NOT inside `_band_food_line` — the skipped call must not leave the
  previous render's caret or food-days tint behind;
  (3) `MapView._draw_supply_links` faint-chains player bands sharing a `supply_network_id` (`0` = solo).
  **Band food flow on the Food line** (snapshot `PopulationCohortState.foodIncome`/`foodConsumption`,
  decoded as `food_income`/`food_consumption`, flowed onto the MapView unit marker + guarded by
  `marker_field_guard`): for a **player** band with real flow, `_band_food_line` appends the **net
  per-turn rate** — `Food 15 (19 days) · −0.77 /turn` — where net = `food_income − food_consumption`,
  tinted green (≥0) / red (<0). The days-to-empty stays only in the `(N days)` figure; it is not
  repeated. The `Food` label is a **click-to-expand disclosure** (a `▸/▾` caret) toggling a
  **category breakdown** beneath it — indented `▲ +X  Gathered` / `▲ +Y  Hunted` / `▼ −Z  Eaten`
  sub-lines (Gathered/Hunted = Σ per-source `actual_yield` by kind, Eaten = `food_consumption`),
  rendered through the **shared morale-breakdown path** in `_format_detail_bbcode` (income ▲ green,
  eaten ▼ amber). The breakdown **auto-shows when food is concerning** (`_food_is_concerning`:
  net-negative OR runway below the warn threshold, mirroring `_morale_is_concerning`), else it's
  collapsed but reachable via the click. Older snapshot / no flow → the bare `Food N (D days)` line,
  no net/disclosure. **The Food + Morale rows share ONE disclosure mechanism** (see "Band morale
  readout" for the shared helpers) — see `_register_disclosure` / `_on_detail_meta_clicked` /
  `_breakdown_open_for` / `_breakdown_expanded`. (The label + click are wired on BOTH the Occupants-card
  drawer's `%OccupantDetail` and the dockable Band/City panel's `get_band_detail_label()`.)
- **Band morale readout** (snapshot `PopulationCohortState.morale`, decoded in `native/src/lib.rs`
  `population_to_dict` as `morale`, a 0–1 float on each cohort dict; flowed into the MapView unit marker
  in `_rebuild_unit_markers`): a band can shrink while well-fed when a harsh tile erodes morale until
  births fall below elder mortality. `BandFoodStatus.gd` owns the morale thresholds too (config key
  `morale.{warn,critical}` = `0.40`/`0.25`, just above the ~0.20 birth floor) and the mirrored
  `color_for_morale`/`hex_for_morale` helpers (same green/amber/red palette, but a plain scalar — no
  "unlimited" sentinel). `Hud._band_morale_line` adds a `Morale: <N>%` row to the drawer **for player
  bands only** (`_is_player_unit`), tinted by `hex_for_morale` via `_format_detail_bbcode` (same
  stash-then-tint pattern as the Food row, using `_selected_band_morale`).
- **Morale trend + named cause** (snapshot `PopulationCohortState.moraleDelta` / `moraleCause`, decoded in
  `native/src/lib.rs` `population_to_dict` as `morale_delta` (raw Scalar/1e6, signed) / `morale_cause`
  (int; `0=None,1=Terrain,2=Cold,3=Unrest`), flowed into the MapView unit marker): "low morale" named the
  symptom, not the cause — the morale drivers live server-side and were discarded each turn until the
  cohort started exporting the per-turn trend + dominant negative driver. `Hud._band_morale_line` appends
  a trend arrow (`▼` falling / `▲` rising / none when `|morale_delta| < MORALE_TREND_EPSILON`) and, when
  falling, the plain-language cause via `_morale_cause_label` — `Terrain`→"harsh terrain", `Cold`→"harsh
  climate" (the server penalty fires on hot **or** cold deviation, so not literally "cold"),
  `Unrest`→"unrest". `Terrain` appends the band's `_selected_tile_info.terrain_label` in parens
  (`Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)`) — the "it's the hex you're on" payload. A
  rehydrated save reports `morale_delta 0 / cause None` for one turn (the sim doesn't persist them); the
  row degrades to a bare percentage.
- **Civilization Wellbeing — productivity, itemized morale, recovery** (see
  `docs/plan_civ_wellbeing.md`; snapshot `PopulationCohortState.outputMultiplier` /
  `discontentFraction` / `lastEmigrated` / `lastImmigrated` / `grievance` + the four signed
  Layer-1 contributions `moraleSettling` / `moraleTerrain` / `moraleClimate` / `moraleUnrest`,
  decoded in `native/src/lib.rs population_to_dict` as `output_multiplier` / `discontent_fraction`
  / `last_emigrated` / `last_immigrated` / `grievance` (telemetry only, not displayed in P1) /
  `morale_settling` / `morale_terrain` / `morale_climate` / `morale_unrest`, all flowed onto the
  MapView unit marker in `_rebuild_unit_markers`). Player-band drawer only (`_unit_summary_lines`):
  - **Output row** (`_band_output_line`): `Output: N%` shown when `output_multiplier < OUTPUT_FULL`
    (1.0), placed just under Morale. Tinted ink → amber → red by `BandFoodStatus.hex_for_output`
    (config `band_status_config.json` `output.{warn,critical}` = `0.85`/`0.60`; near-full reads
    neutral ink, *not* green — it's a productivity note, not a "good"). Ties productivity to morale.
  - **Itemized morale breakdown** (`_morale_breakdown_lines`): the four signed contributions
    (their sum IS `morale_delta`) as indented sub-lines (e.g. `    ▲ +1.0%  settling`). Only
    contributions above `BandFoodStatus.morale_breakdown_epsilon()` (config `morale.breakdown_epsilon`
    = `0.002`) list. Labels: `settling`, `harsh terrain (<terrain_label>)` (matches the headline cause
    treatment), `harsh climate`, and `unrest`/`culture` by sign. `_format_detail_bbcode` tints each
    row two-tone by its sign glyph (▲ = HEALTHY green, ▼ = WARN amber — deliberately not a rainbow);
    the indented breakdown lines are intercepted before the KV split. The **Morale row is a
    click-to-expand disclosure identical to Food** (the `▸/▾` caret + `meta_clicked` toggle share
    `_register_disclosure` / `_on_detail_meta_clicked` / `_breakdown_open_for` / `_breakdown_expanded`,
    keyed `"morale:<entity>"`): **auto-shown when concerning** (`_morale_is_concerning`: below warn
    **or** falling past `MORALE_TREND_EPSILON`), else collapsed but expandable via the click. The
    contributions always compute so the good state can be manually expanded; the disclosure is offered
    only when there's actually something to show (a contribution above epsilon, or the concerning
    recovery line).
  - **Recovery guidance** (`RECOVERY_GUIDANCE_TEXT`): a dim `↑ Recover: move to Hospitable ground ·
    Scout · Hunt` line (the real levers, NOT harvest), appended under the breakdown **only when
    morale is concerning** (a healthy band that manually expands its breakdown is not told to
    "recover"). `_split_detail_kv` skips lines beginning with `↑` so it renders as a dim sentence.
  - **Action morale hints**: the Scout button tooltip (`MORALE_HINT_SCOUT`, "(+morale)") and the four
    persistent Hunt/Follow policy tooltips (Sustain/Surplus/Market/Eradicate get `MORALE_HINT_PERSISTENT`
    appended, "(+morale/turn)") advertise the positive levers; the one-shot Single policy does not.
- **Tile-card Habitability** (snapshot `TileState.habitability`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `habitability` (raw Scalar/1e6; band-independent per-turn morale drain of the tile's
  terrain + temperature, ≥0, bigger = harsher), stored in `MapView.tile_habitability` keyed by
  `Vector2i` and copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a
  `Habitability: <rating>` row (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present). `ui/TileHabitability.gd` is the single
  source of truth — config `src/config/tile_habitability_config.json` (`habitability.{hospitable_max,
  fair_max,harsh_max}` = `0.02`/`0.05`/`0.09`) buckets the drain into Hospitable/Fair/Harsh/Hostile,
  tinted HEALTHY/INK/WARN/DANGER via `hex_for_rating` in `_format_detail_bbcode` (mirrors the
  `BandFoodStatus` bucketing pattern). The Karst Cavern Mouth (~0.0825) reads "Harsh" (amber).
  With the latitude climate + cold-morale tolerance dead-band (see `core_sim`), temperate
  mid-latitudes read "Hospitable", the equator "Hospitable/Fair", and poles/high-alt/caverns
  "Harsh/Hostile" — the config buckets (`0.02`/`0.05`/`0.09`) spread cleanly across that range,
  so no re-tune was needed.
- **Tile-card Climate** (snapshot `TileState.temperature`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `temperature` (°); temperature is now a **latitude + elevation** climate
  (equator-in-the-middle, poles cold) with a small element jitter, NOT the old element
  checkerboard — see `core_sim`), stored in `MapView.tile_temperature` keyed by `Vector2i` and
  copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a `Climate: <band>` row
  next to Habitability (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present so rehydrated tiles degrade
  gracefully). `ui/TileClimate.gd` is the single source of truth — config
  `src/config/tile_climate_config.json` (`climate.{tropical_min,warm_min,temperate_min,cool_min}`
  = `26`/`20`/`12`/`3`) maps the temperature into Tropical/Warm/Temperate/Cool/Polar, making the
  latitude gradient legible ("far south → Polar"). The row is **informational** — neutral ink, no
  HEALTHY/WARN/DANGER tint, so it doesn't overload the Habitability row's warning semantics.
- **Band alerts → the turn orb** (`Hud.gd` `update_band_alerts`, dispatched from `Main.gd` on the
  snapshot `populations`): the standalone left-dock **Alerts panel was removed** and its alerts folded
  into the turn-orb attention model (see next bullet) — the single player-faction loop now builds the
  orb's `attention` array instead of a separate alerts array. NOTE: cohorts carry no top-level band label
  in the snapshot — names fall back to a positional "Band N"; a server-side band-label field would make
  names authoritative.
- **Turn orb & attention model** (`ui/TurnOrb.gd` + `ui/TurnOrb.tscn`, last `BottomBar` child;
  `docs/plan_hud_nav_turn_orb.md`): the bottom-right orb replaces the "Advance Turn" button and
  is a **generic attention hub**. Readiness = the attention registry is **empty** → a calm cyan
  `SIGNAL` pulse ("nothing needs you"); any entries → the pulse stops and a **count badge** tinted
  by the highest severity shows. **The orb face always advances the turn** (`_on_face_pressed`): with
  an **empty** registry the click emits `advance_requested` directly (no popover — an empty popover has
  nothing to review, and once mis-stretched to full height it pushed its own `Advance ▸` footer
  off-screen, trapping the player); with **entries** it toggles a **reasons popover** (built at
  runtime, `HudStyle.card_stylebox()`) — one row per entry (severity stripe + kind icon + label +
  detail + right-aligned `Jump →`), highest-severity first, plus an `Advance ▸` footer. The orb
  knows nothing about producers; it renders a list of generic **Attention** dicts:
  `{kind, severity ("info"|"warn"|"critical" → SIGNAL/WARN/DANGER), label, detail, x, y}` where
  `x < 0` = non-locating (renders `Open ▸`, a no-op stub for now). Kind→icon (in `TurnOrb.gd`):
  `starving`→🍖, `losing_population`→📉, `idle_workers`→🛠, `awaiting_orders`→▮▮ (read from
  `FoodIcons.STATUS_ICONS` — the same glyph the Band panel's awaiting row wears), unknown→●.
  Row labels **clip** and `POPOVER_WIDTH` is sized to the widest producer row: a row's inner HBox is
  anchored to its Button (not a container child), so an over-wide label used to spill its `Jump →`
  outside the card instead of widening it. Wiring stays stable via Hud
  relays: a row's jump → `focus_requested` → `alert_focus_requested` → `MapView.focus_on_tile`
  (the same centering the retired Alerts panel used); the footer → `advance_requested` →
  `next_turn_requested(1)`; `update_overlay` pushes the turn number via `set_turn`. The **four live
  producers** (all in `Hud.update_band_alerts`, each pushed with the tile `current_x`/`current_y` so
  Jump locates it) — the folded-in Alerts panel, plus the expedition one. The first three run in one
  loop over the player faction's BANDS:
  - **`starving`** (critical) — `BandFoodStatus.is_critical(days)`; label `"<band> starving"`, detail = `_food_days_text(days)`.
  - **`losing_population`** (warn) — shrank vs the previous snapshot (`_prev_band_sizes`); label `"<band> losing population"`, detail = `_decline_reason(days, morale, morale_cause, last_emigrated)` (`— starving` / `— people leaving` / `— harsh terrain|climate|unrest` / `— low morale`).
  - **`idle_workers`** (warn) — `idle_workers > 0`; label `"N idle workers"`, detail = band name. Supersedes the old `activity == idle` alert (a worker count is more actionable).

  The fourth (`_awaiting_orders_attention`) runs over the **EXPEDITIONS** split out of that loop:
  - **`awaiting_orders`** (warn) — an expedition in `ExpeditionPhase::Awaiting`: parked at its
    objective, burning provisions, doing nothing until the player acts. Structurally the same class
    as idle workers (a demand on the player, an efficiency loss, not a crisis) — hence WARN, and
    hence it belongs on the orb rather than only on a band panel you happen to have open. **One row
    per party, not one aggregate** (each is a separate decision with its own destination; idle
    workers genuinely IS one aggregate): label = the phase words from `EXPEDITION_PHASE_LABELS`
    ("Awaiting orders"), detail = `"<mission> · <objective>"` (mission from
    `EXPEDITION_MISSION_LABELS`; objective = the followed herd for a hunt party, the party's tile for
    a scout). Capped at `ATTENTION_AWAITING_MAX_ROWS` — the popover is positioned ABOVE the orb, so an
    unbounded list would climb off-screen and take the `Advance ▸` footer with it — with the remainder
    folded into one `"+N more awaiting orders"` row that jumps to the first party past the cap (so
    even the aggregate row is actionable, not a dead `Open ▸` stub). **Its Jump reuses the Band
    panel's expedition-row path**: `Hud._on_turn_orb_focus` resolves an awaiting expedition standing
    on the jumped-to tile (`_awaiting_expedition_at`) and routes through
    `_on_panel_expedition_selected` (recenter + pin that exact expedition so its drawer opens),
    falling back to the plain `alert_focus_requested` recenter for the band-located producers.

  The orb severity-sorts (critical floats up), so a starving band tops the popover. Future producers
  (`war` / `decision`) are stubs the model already fits — one producer each, **no orb changes** (the
  awaiting one needed only a kind→icon entry). ui_preview: `turn_orb_attention` (the three band
  producers) / `turn_orb_awaiting_orders` (awaiting rows + idle workers coexisting, incl. the cap's
  overflow row).
- **Targeting: move-band + send-expedition + send-hunt-expedition** (`Hud.gd`): the single-task
  forage/scout/hunt/follow `_pending_*` flows were retired with labor allocation. Three targeting
  flows remain, all built on the same `_pending_*` → `_current_targeting_info()` →
  `_refresh_targeting()` machinery: `_pending_move_band` (`command: "move"`, `need: "tile"`),
  `_pending_send_expedition` (`command: "expedition"`, `need: "tile"`, carries the outfitted band +
  party size), and `_pending_send_hunt_expedition` (`command: "hunt_expedition"`, `need: "herd"`).
  `_current_targeting_info()` returns a descriptor (`{active, command, need, origin_x/y,
  context_label}`) for whichever is set; `_refresh_targeting()` shows the floating **targeting
  banner** (top-centre, `HudStyle.banner_stylebox()`: cyan reticle + command + instruction + Cancel)
  and emits `targeting_changed(info)`. `show_tile_selection` + `notify_hex_selected` dispatch all
  three pending flows on the click (the tile click carries `tile_info.herds`, which the hunt flow
  resolves its target from).
- **Main forwards** `hud.targeting_changed → map_view.set_targeting` and
  `map_view.targeting_cancel_requested → hud.cancel_active_targeting`.
- **MapView draws** the overlay (`_draw_targeting`): `need == "tile"` draws a reticle on the
  hovered hex (the `need == "band"` path is now unused). Esc / right-click during targeting emit
  `targeting_cancel_requested` instead of panning; the pulse is animated from `_process`.
- **Resolution**: the destination tile click (`_try_dispatch_pending_move_band`) emits
  `move_band_requested` → `Main._on_hud_move_band` → `move_band …`; the expedition-target click
  (`_try_dispatch_pending_send_expedition`) emits `send_expedition_requested` →
  `Main._on_hud_send_expedition` → `send_expedition …`.
- **Scouting expedition** (`docs/plan_exploration_and_sites.md` §2; snapshot
  `PopulationCohortState.isExpedition`/`expeditionMission`/`expeditionPhase`, decoded in
  `native/src/lib.rs population_to_dict` as `is_expedition`/`expedition_mission`/`expedition_phase`,
  flowed onto the MapView unit marker in `_rebuild_unit_markers`; `homeBandEntity` is decoded as
  `home_band_entity` (the outfitting band — powers the Band panel's Active-expeditions section),
  while the persistence-only `expeditionAnnounced`/`pendingReveal*` fields stay undecoded). A
  detached party is a `PopulationCohort` tagged `Expedition` that flows through the same
  `populations[]` array as a band. Surfaced four ways:
  (1) **Distinct map marker** (`MapView._draw_unit` → `_draw_expedition_body`): a hollow,
  faction-tinted **flag disc** (⚑) instead of a resident band's solid dot; when
  `expedition_phase == "awaiting"` a **pulsing amber (WARN) ring** signals idle-at-objective needing
  an order (animated from `_expedition_time` in `_process`, gated on `_has_awaiting_expedition` set
  at marker-rebuild). Resident-band rendering is untouched.
  (2) **Expedition drawer panel** (`Hud._render_occupant_drawer` → `_build_expedition_panel`):
  replaces the labor-allocation panel for a selected expedition (no labor in v1). Drawer text
  (`_expedition_summary_lines`) shows Mission / humanized Phase / Party / Provisions (`daysOfFood`);
  the panel hosts **Recall** (→ `recall_expedition_requested` → `Main._on_hud_recall_expedition` →
  `recall_expedition …`) + **Move** (reuses `_on_move_band_pressed`; `_resolve_assign_band` returns
  the selected expedition since it's a player unit — Move retargets it via `move_band` unchanged, no
  un-gating needed).
  (3) **Outfit UI** (`Hud._build_allocation_panel` → `_build_send_expedition_controls`): on a
  selected resident band, a "Send scouting expedition" party-size stepper (max =
  `min(idle_workers, max_expedition_party_size)`; the server's hard cap comes from the
  `maxExpeditionPartySize` snapshot field, decoded as `max_expedition_party_size`, defensively
  falling back to idle when absent/0) + a button entering `_pending_send_expedition` targeting.
  (4) The `marker_field_guard` covers the four new marker keys (`is_expedition`,
  `expedition_mission`, `expedition_phase`, `max_expedition_party_size`). The server still rejects
  a genuinely over-cap request with a feed message as a backstop.
- **Hunting expedition** (PR 2, `docs/plan_exploration_and_sites.md` §2b; snapshot
  `PopulationCohortState.expeditionTargetHerd` (string fauna_id) / `expeditionHuntPolicy` (string
  `sustain|surplus|market|eradicate`) / `expeditionCarryCap` (float), decoded as
  `expedition_target_herd` / `expedition_hunt_policy` / `expedition_carry_cap` and flowed onto the
  marker; `expedition_mission` also takes `"hunt"`, `expedition_phase` also takes
  `"hunting"`/`"delivering"`). A hunt party follows a migratory herd, accumulates food up to a carry
  cap, and drops it at the band — the second verb on the same expedition machinery. Surfaced:
  (1) **Distinct map marker** (`MapView._draw_expedition_body`): a hollow 🏹 **bow disc** (vs the
  scout's ⚑ flag), keyed on `expedition_mission == "hunt"`. Phase read: `hunting` (gathering) draws a
  small red "working" cue ring; `delivering`/`returning` (hauling home) draw a green food pip.
  (2) **Hunt drawer panel** (`Hud._expedition_summary_lines` branches on mission): Mission "Hunting
  expedition", **Target** herd (`expedition_target_herd`, species via `_herd_label_for_id` → raw id
  fallback), **Policy** (`expedition_hunt_policy`, capitalized), humanized **Phase**
  (Hunting/Delivering/Returning), Party, and **Carried X / cap** (`stores` total vs
  `expedition_carry_cap`, days from `daysOfFood`) with a **· FULL** badge at the ceiling. Reuses
  `_build_expedition_panel` (Recall + Move, "Returning"-when-returning treatment — mission-agnostic,
  so hunt parties get it too).
  (3) **Outfit UI** (`Hud._build_send_expedition_controls`): under the shared "Send expedition"
  section (party stepper + "Send scouting expedition"), a **hunt policy radio**
  (`_build_policy_picker(…, _send_hunt_policy)`, Sustain/Surplus/Market/Eradicate, default Sustain)
  with a one-line behaviour hint (`SEND_HUNT_POLICY_HINTS`), then "Send hunting expedition". It enters
  a HERD-targeting pending mode (`_pending_send_hunt_expedition`, `command: "hunt_expedition"`,
  `need: "herd"`) carrying band + party + policy; the target click resolves to a huntable herd on the
  clicked hex (`_huntable_herd_id_on_tile` reads `tile_info.herds`) and emits
  `send_hunt_expedition_requested` → `Main._on_hud_send_hunt_expedition` →
  `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]` (trailing policy;
  server defaults Sustain). No huntable herd on the hex → a command-feed nudge, stays in targeting.
  `MapView._draw_targeting` glows huntable herds + reticles the hovered hex for `need == "herd"`.
  (4) `marker_field_guard` covers `expedition_target_herd` / `expedition_hunt_policy` /
  `expedition_carry_cap`. Recall is the unchanged `recall_expedition` (works for hunt parties too).
- **Retired verbs (Early-Game Labor slice 3a):** the server now parses-but-ignores
  `follow_herd` / `scout` / `forage` / `hunt_fauna` / `hunt_game`. Every client control that
  emitted them was removed or repointed so nothing is silently dead: the map double-click
  `scout` shortcut was dropped and `follow` repointed to quick-assign hunters; Main's
  `_issue_*`/`_on_hud_follow_herd`/`_on_hud_unit_scout` builders are gone; the Fauna tab's
  follow button, the Terrain tab's Scout Tile button, and the Commands tab's scenario
  Scout/Follow rows were removed (script + `InspectorLayer.tscn` nodes). No code path in
  `Main.gd`/`Hud.gd`/`MapView.gd`/`Inspector.gd` builds any of those five lines.

## Band/City dockable panel

`ui/BandCityPanel.gd`/`.tscn` — a CanvasLayer that is the **persistent band/city
command center**: shown whenever ≥1 player band exists, always displaying a
"current band" (`_panel_band`). Design/roadmap: `docs/plan_band_city_dock.md`.

- **Dockable + persisted.** The user docks it to any of the 4 edges (default
  `SIDE_LEFT`) or collapses it to a thin rail; the choice (+ collapsed bool)
  persists to `user://band_city_dock.cfg` via `ConfigFile` (loaded in `_ready`,
  saved on change — the client's first user-pref file). It reserves its edge
  through the registry above: `reservation_changed(edge, size)` →
  `Main._apply_reservation(&"band_panel", edge, size)` (size = the cross-axis
  width/height, `COLLAPSED_SIZE` when railed, or 0 when hidden), so the map + HUD
  reflow off the reserved edge. All geometry/typography are named constants +
  `HudStyle`; the map-facing edge gets a `SIGNAL_DEEP` accent seam.
- **Header chrome.** Settlement **stage glyph + name + stage label**
  (`set_header` — glyph/label from the band marker's `settlement_stage_icon` /
  `settlement_stage_label`, neutral glyph fallback), a `◀ n/N ▶` **cycler**
  (`set_cycler`) over `_player_bands`, a 2×2 **dock chooser** (active edge
  highlighted), and a **collapse** toggle. `cycle_requested(delta)` → Main relays
  to `Hud.cycle_panel_band`.
- **Header rows — no restated identity.** The panel's own chrome already states the band's **name +
  settlement stage**, so its summary grid does NOT repeat them: `_unit_summary_lines(unit, in_panel =
  true)` **drops the `Unit: <name>` row** (it was a third copy of the name) and **replaces `Size: <n>`**
  — population under another name — with a **`Population  29 · Workers 14 (Idle 12)`** row
  (`WORKERS_VALUE_FORMAT`, idle from the SAME `_effective_idle` the `+` steppers gate on). That labor
  line used to render as the allocation stack's first block, which meant it appeared wherever CURRENT
  ACTIONS did — **stranded between Active expeditions and Current actions**; the panel now passes
  `with_population_header = false` to `_build_allocation_sections`, so it exists once, in the identity
  grid. The header reads: name / stage / Population / Food / Morale / Position.
  `Unit` and `Size` are gone from **both** hosts — the Occupants drawer's roster row names the band
  and shows its size, so they restated it there too. `in_panel` survives as the gate on the
  **Population** row alone: the dock is the only host with a labor readout, and a foreign band has no
  `working_age`/`idle_workers`, so rendering it in the drawer would print a fabricated
  `Workers 0 (Idle 0)`. `_unit_summary_lines` is still shared with the Occupants-card drawer (foreign
  bands + the no-panel `ui_preview` fallback), and the legacy in-card allocation host keeps the
  population header block.
- **Content relocation (from the Occupants card).** The **player-band** branch of
  `Hud._render_occupant_drawer` now renders into the panel via `_render_band_into_panel`,
  which assembles an ordered array of **section blocks** — a summary block
  (`_unit_summary_lines`), the Active-expeditions block, then the allocation sections
  (`_build_allocation_sections`) — and hands them to `BandCityPanel.set_band_sections`
  (see "Responsive body"). `_build_allocation_sections` returns the discrete Workers /
  Current actions / Band roles / Orders / Send-expedition VBoxes; the legacy
  `_build_allocation_panel(band, target)` wrapper still exists and fills the flat
  `%AllocationPanel` (the no-panel `ui_preview` fallback) by appending those same blocks.
  Herd/expedition detail stays in the Occupants card (`%OccupantDetail` / `%AllocationPanel`
  — still the expedition host **and** the no-panel fallback).
- **Live + persistent.** `_refresh_panel_band()` (called each snapshot from
  `update_band_alerts`) hides the panel when there are zero player bands, else
  re-resolves `_panel_band` against the fresh snapshot (by entity, falling back to
  the first band) and re-renders so steppers/idle stay current. Selecting a
  herd/empty tile leaves `_panel_band` intact — the panel persists across selection
  changes. `cycle_panel_band(delta)` walks `_player_bands`, **recenters the map**
  on the band (`alert_focus_requested` → `MapView.focus_and_select_tile`), then
  pins the exact band so ring/Tile card/roster/panel all agree.
- **Bands vs expeditions.** `update_band_alerts` splits the player faction into
  `_player_bands` (resident bands — NOT `is_expedition`) and `_player_expeditions`
  (detached scout/hunt parties). The cycler + band-picker read `_player_bands`
  only, so a band + 2 expeditions reads **1/1**, not 1/3. Expeditions surface
  instead as an **Active expeditions** section on their home band (see below).
- **Active expeditions section.** `_render_band_into_panel` → `_build_panel_expeditions_block`
  builds a self-contained expeditions **section block** (handed to the panel in the section
  array, so it's its own flow item / stack row) with one ghost-button
  row per `_player_expeditions` entry whose `home_band_entity == _panel_band.entity`
  (correct for N bands; omitted when none). Row summary — mission glyph + subject + the sim
  `ExpeditionPhase` as a **glyph** (`FoodIcons.for_status`), the phase WORD having moved into the row
  tooltip: hunt `🏹 <herd> · <Policy>  ●`, scout `⚑ → (x,y)  ➤`. The tooltip spells out the mission,
  the hunt policy's behaviour hint, the phase + what it means, and the click affordance.
  **`awaiting` is the one exception — it keeps its words, WARN-amber** (`▮▮ Awaiting orders`): it is
  not a status but a demand on the player (the party is parked at its objective burning provisions
  until you act), and a call to action must never require a hover to find. (A follow-up will make
  `awaiting` a turn-orb attention producer; the orb model already fits it.)
  A row click reuses the cycler's routing —
  `alert_focus_requested`→`focus_and_select_tile` + `roster_occupant_selected`→
  `MapView.select_occupant` — so the map ring moves to the expedition and the
  **Occupants card** (not the band panel) renders its `_build_expedition_panel`
  drawer; `_panel_band` stays put. `home_band_entity` is decoded in
  `native/src/lib.rs population_to_dict` from the snapshot's `homeBandEntity`,
  flowed onto the MapView unit marker, and covered by `marker_field_guard`.
- **Responsive body — section blocks (tall stack vs wide column-flow).** The band
  content is a list of discrete **section blocks** Hud hands the panel via
  **`set_band_sections(blocks: Array)`** (replacing the old
  `get_band_alloc_container()`/`get_band_detail_label()`/`get_band_expeditions_container()`
  fill-a-container contract): the summary RichTextLabel block, the Active-expeditions
  block, then the allocation sections (Workers / Current actions / Band roles / Orders /
  Send expedition). Hud builds them in `_render_band_into_panel` (allocation sections from
  `_build_allocation_sections` — the per-row stepper/band-picker/pending/expedition wiring
  is unchanged, only each row's *parent* is its section VBox now; the legacy flat
  `%AllocationPanel` fallback still fills by appending the same blocks). The panel **owns**
  the blocks (frees the prior set on each call) and arranges them by dock aspect
  (`_relayout_body`/`_arrange_sections`, hooked off `_apply_dock_layout`, reparenting the
  **same** block nodes on a tall↔wide flip — no Hud re-render): **tall** (LEFT/RIGHT) = a
  vertical `ScrollContainer` stack whose reserved **WIDTH fits the content** (`_measure_tall_width`,
  the mirror of the wide height fit): the cross-axis width is `maxf(PANEL_WIDTH, content-min)` (the
  PanelContainer's combined min width — margins + widest section), floored at `PANEL_WIDTH`, so
  `_root`, the seam (`_position_seam`), and the reservation all track the **true card edge** — a wide
  section (a long Hunt row, the send-expedition button) no longer overflows a fixed-380 `_root` and
  freezes the seam mid-card. Re-measured (deferred one frame, `is_equal_approx`-guarded — the content
  min is width-independent so there's no resize feedback) on `set_band_sections`, dock/collapse change,
  and viewport resize. **Wide** (TOP/BOTTOM) = **manual balanced-column packing** (`_pack_wide_columns`):
  column count from the
  available width (`num_cols = clamp(avail / (_widest_block_width() + WIDE_FLOW_SEPARATION), 1,
  #blocks)` — the budget is `max(SECTION_COLUMN_WIDTH, widest section's own min width)`, NOT the
  nominal column width: a section wider than nominal (a Current-actions row now carries a resource
  glyph + label + policy tag + yield + ⚠ + the stepper) grows its column, and budgeting off the
  nominal width summed the columns past the window — the last one clipped behind a horizontal
  scrollbar), blocks distributed **greedily into the shortest column** so the tallest column
  is minimized, columns in an HBox. The panel then **sizes its T/B height to the content** —
  the reservation it reports (`reservation_changed`) is `header + tallest-column + margins`,
  so the map/HUD reflow to exactly fit and **nothing clips** (fit-to-content, not a fixed
  `PANEL_HEIGHT`). Re-packs on dock change, `set_band_sections` (content change), and window
  `size_changed`; a deferred re-measure (`await process_frame`) lets the `fit_content` summary
  RichTextLabel settle before the height is finalized. Safety net: reserved height is capped
  at `MAX_WIDE_HEIGHT_FRACTION` of the window, past which the columns' ScrollContainer
  re-enables vertical scroll. (Earlier `VFlowContainer` / fixed-height wide layouts were
  replaced — VFlowContainer can't do fit-to-content *and* multi-column: unbounded height
  stops it wrapping.)
- Verify chrome + reflow via `tools/band_panel_preview.gd`
  (`godot --path . res://tools/band_panel_preview.tscn` → `ui_preview_out/
  band_panel_{left,right,top,bottom,collapsed}.png`). State `band_panel_status_glyphs` is the
  **row-vocabulary** frame: a confirmed working forage row (`●` + `♻` + the overstaffing note) and a
  working hunt row (`●` + `⚠`) beside a pending row (`○`, amber), plus one Active-expeditions row per
  phase (`➤` outbound / `●` hunting / `◄` delivering / `◄` returning / `▮▮ Awaiting orders` in amber)
  — read it at true size whenever a glyph changes.

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
| Knowledge | Ledger overview, timeline graph, espionage mission queue |
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
- Public feeder methods for cross-panel data flow (`KnowledgePanel.append_events`,
  fed by Trade's diffusion records). The two panels never reference each other —
  `TradePanel` emits `knowledge_events_produced(records)` and the coordinator
  forwards the batch to `KnowledgePanel.append_events` (wired in `_ready`).
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
gating, log-path ingestion, and the Trade→Knowledge event feed), and
`ui/inspector/TradePanel.gd` (Trade — map-overlay collaborator + the emit side of
the Knowledge↔Trade seam). **The decomposition is complete** — every inspector tab is
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
| Double-click herd | Quick-assign the player band's idle workers to hunt it (Sustain) |

---

## See Also

- `README.md` - Setup and running instructions
- `docs/godot_inspector_plan.md` - Inspector migration progress
- `core_sim/CLAUDE.md` - Simulation engine (snapshot contracts, commands)
- `docs/architecture.md` - Cross-system data flow
