---
paths:
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/CachedMapRenderer.gd"
  - "clients/godot_thin_client/src/scripts/ui/{MinimapController,MinimapPanel}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{BandMarkerRenderer,SecondaryMarkerRenderer,AnnotationRenderer}.gd"
---

<!-- Extracted verbatim from lines 145-151;337-395 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# MapView renderers and the 2D minimap

## Key scripts

| Script | Purpose |
|--------|---------|
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's band stack), navigation (WASD/QE/mouse), tile picking, and the coordinator for the **layered hex-marker system** (see Map markers below). Three cohesive subsystems are composed out into owned renderer helpers, each holding a `_view: MapView` back-ref and driven from MapView's `_ready`/`_draw` (all shared geometry/glyph/pill/fog primitives + the marker source arrays + selection state stay on MapView): the **2D minimap** (`ui/MinimapController.gd`, `_minimap`), the **primary band markers** (`ui/BandMarkerRenderer.gd`, `_band_markers`), and the **secondary markers** (`ui/SecondaryMarkerRenderer.gd`, `_secondary_markers`). A FOURTH helper owns the terrain rasters: the **terrain textures + Approach-B blend shader** (`ui/TerrainRenderer.gd`, `_terrain`), a FIFTH the **selected-band / selected-herd overlays** (`ui/BandOverlayRenderer.gd`, `_band_overlays`), and a SIXTH the **map annotations** (`ui/AnnotationRenderer.gd`, `_annotations`) — trade links, crisis annotations, the Terrain-tab highlight, order routes and command targeting. Still on MapView on the terrain side: the CPU base pass `_draw_terrain_direct` (the frame's base loop, which branches between the helper's textured hex and MapView's solid `_tile_color` fill) and the whole `_cache_*` SubViewport (it caches the non-shader base render and 9 of its 11 invalidation sites are non-terrain). Still on MapView: the `_draw_*` overlay families NOT yet extracted — supply links and the tile selection/hover outline (see the Step-4 report for why each was left). MapView keeps thin same-named pass-throughs for the annotation family's five reflectively-reached seams (`set_targeting`, `update_trade_overlay`, `set_trade_overlay_enabled`, `set_trade_overlay_selection`, `set_terrain_highlight`) |
| `ui/MinimapController.gd` | Owns MapView's 2D minimap: the `MinimapPanel` instance, its terrain/FoW image (rebuilt only on grid/data/FoW change), the viewport-indicator overlay and click-to-pan. Holds a `_view: MapView` back-ref; behaviour is identical to the old inlined minimap code |
| `ui/BandMarkerRenderer.gd` | Owns MapView's PRIMARY player-band markers: the offset card-stack of settlement-stage tokens / expedition flag-discs, the faction nameplate banner (+ its reused StyleBoxFlat), the food-days dot, the travel/task arrow, and the ×N over-cap count pill. `_view: MapView` back-ref; `draw_primary_bands()` called during MapView's `_draw`; pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/SecondaryMarkerRenderer.gd` | Owns MapView's SECONDARY markers (herds / food sites / discovered sites / harvest+scout overlays) + the per-frame edge-slot assignment (`compute_slots`) and `+N` overflow chip. Owns only the per-frame slot maps; all draw commands + shared primitives + marker source arrays stay on MapView via the `_view` back-ref. Pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/TerrainRenderer.gd` | Owns MapView's TERRAIN raster/shader subsystem: the Approach-B per-pixel biome-blend shader (the whole-map `TerrainBlendQuad` child + its `ShaderMaterial` + the six per-hex splatmap textures it is fed — id / vis / elev / river-edge / river-channel / navigable-underlying), the blend-OFF per-hex texture cache, the blend-class + canopy/peak code maps, and the `T`-key texture toggle (MapView keeps thin `get_terrain_textures_enabled` / `enable_terrain_textures` pass-throughs for the Inspector/HUD; `CachedMapRenderer` reads the cache via `hex_texture_for`). **All eight tuning-const families live here** (`EDGE_BLEND_*` / `WATER_BLEND_*` / `SHORE_*` / `CANOPY_*` / `PEAK_*` / `RIVER_*` / `BASE_DEFAULT_TEXTURE_SCALE`); `EDGE_BLEND_MIN_RADIUS` + `FOW_EXPLORED/VISIBLE_THRESHOLD` are `const X = MapView.Y` aliases, so each has exactly one definition. `_view: MapView` back-ref — every draw command plus the shared geometry/colour/visibility/river primitives stay on MapView. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on all 286 `map_preview` + `blend_probe` frames (the 4 `map_band_*` frames that vary run-to-run do so identically before and after — the `map_preview` window-maximize race, not this change) |
| `ui/BandOverlayRenderer.gd` | Owns MapView's SELECTED-BAND / SELECTED-HERD overlay family: the three range borders (`FORAGE`/`HUNT`/`SCOUT_RANGE_OUTLINE*`) + worked-forage fills + hunted-herd rings/links, the dashed-amber optimistic PENDING overlay (`LABOR_PENDING_*`), the travel-destination line + reticle (`TRAVEL_DEST_*`), the selected herd's graze-range ring (`HERD_RANGE_*`), the corralled herd's pen footprint (`PEN_FOOTPRINT_*`), and the deferred per-source yield-label batch (`YIELD_LABEL_*`). **Every one of those const families moved here** — each was measured to have no executable reference outside the family, so none needed a `const X = MapView.Y` alias (unlike `TerrainRenderer`); `ICON_MIN_DETAIL_RADIUS` is read as `_view.ICON_MIN_DETAIL_RADIUS`, the `SecondaryMarkerRenderer` idiom. `_view: MapView` back-ref — every draw command plus the shared geometry/hex/glyph/pill primitives (`_hex_center` / `_hex_points` / `_fill_hex` / `_outline_hex` / `_hex_distance` / `_band_effective_col` / `_wrapped_col_delta` / `_is_tile_visible` / `_herd_by_id` / `_draw_marker_glyph` / `_draw_pill_plate` / `_draw_reticle`) and the unit/herd/selection state stay on MapView. It owns only this family's own state: the pushed `_labor_pending` map and the per-frame `_deferred_yield_labels` batch. **FOUR entry points, and their ORDER in `_draw` is the contract**: `draw_band_work_highlights` / `draw_herd_range_highlights` / `draw_pen_footprint_highlight` at their existing positions (under the markers), then **`flush_yield_labels` LAST** — after markers, rings, links, pending overlays and targeting, because those layers used to paint over the numbers. **THE YIELD-LABEL LIFECYCLE IS WHOLLY INSIDE THIS FILE** (the decomposition plan's claim that extracting it would split the batch across MapView and a helper was measured FALSE): the clear happens at the top of `draw_band_work_highlights` **before its early-outs** (so a deselected band leaves no stale labels for the flush to paint), the queue is called only from within that same pass, and the flush renders + drains. The far-zoom LOD gate (`show_yields`) stays at the **QUEUE** site, never at the flush, so a suppressed label is never queued and deferral cannot bypass the suppression. **`set_labor_pending` stays PUBLIC ON MAPVIEW as a thin same-named pass-through** (it stores here, MapView owns the `queue_redraw`) — `Main.gd` wires the HUD's `labor_pending_changed` signal to it BY NAME via `has_method`/`Callable`, and `tools/map_preview.gd` calls it on the MapView at 13 fixture sites, so that seam cannot move. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 56 `map_preview` frames — 0 differing** (the harness is a strict bit-identity reference since PR #310 pinned its canvas and froze `Engine.time_scale`), plus 205 of 230 `blend_probe` frames; the 25 that moved are the `BANK_*` state, which **varies run-to-run on a clean tree too** — `blend_probe` does NOT freeze time and BANK is its only state carrying a `TIME`-scrolled navigable-river channel |
| `ui/AnnotationRenderer.gd` | Owns MapView's ANNOTATION family — the overlays that say something *about* the map rather than drawing the world: the Trade tab's diffusion links, the Crisis overlay's annotations, the Terrain tab's "highlight all tiles of this type" tool, the per-faction order ROUTES, and the command-TARGETING overlay (valid-target glow / reticle / hover-ETA plate). **All five const families live here** (`TERRAIN_HIGHLIGHT_*` / `CRISIS_*` / `TRADE_*` / `ROUTE_*` / `TARGETING_*`), each measured to have no executable reference outside the family; the one exception is `CRISIS_COLOR`, which MapView's `OVERLAY_COLORS` table also reads, so it is a `const CRISIS_COLOR := MapView.CRISIS_COLOR` alias — one definition, two readers. It owns this family's own state (`_terrain_highlight_id`, the three trade-overlay fields, `_crisis_annotations`, `_routes`, `_targeting` + its pulse clock); every draw command plus the shared primitives (`_hex_center` / `_hex_points` / `_hex_center_wrapped` / `_draw_label` / `_draw_reticle` / `_hex_distance` / `_wrapped_col_delta` / `_is_player_unit` / `_get_adjusted_viewport_size`) and the world state it reads (`units` / `herds` / `terrain_overlay` / `tile_lookup` / `faction_colors` / `active_overlay_key` / `_hovered_tile`) stay on MapView via the `_view` back-ref. **FIVE PUBLIC SEAMS KEEP SAME-NAMED PASS-THROUGHS ON MAPVIEW** because every one is reached REFLECTIVELY — a rename would not error, it would silently do nothing: `set_targeting` (Main.gd connects the HUD signal by name via `has_method`/`Callable`), `update_trade_overlay` / `set_trade_overlay_enabled` / `set_trade_overlay_selection` (TradePanel.gd via `has_method`/`call`), and `set_terrain_highlight` (TerrainPanel.gd via `has_method`/`call`). The pass-throughs store here and MapView owns the `queue_redraw` (the `set_labor_pending` idiom); the two setters that only redrew CONDITIONALLY return a bool so that condition survives the move. `_targeting_time` is advanced from MapView's `_process` via `advance_targeting_time`, still gated on `is_targeting_active()`. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 60 `map_preview` frames and all 230 `blend_probe` frames — 0 differing** — and, crucially, four of those `map_preview` frames were **added first** (`map_trade_overlay` / `map_crisis_annotations` / `map_terrain_highlight` / `map_routes`), because before them this family had NO pixel coverage at all and the byte-diff would have proved nothing about it |
## Fog of war is SERVER-authoritative — the client renders what it is told

The sim owns fog of war. `SimulationConfig.fog_enabled` gates **both** the visibility raster **and
the herd display list**, and that second half is the whole reason the setting moved server-side:
with fog off, the fauna the herd filter used to drop are now genuinely *sent*, so the Fauna tab
shows them with **no client-side special case at all** (`FaunaPanel` renders whatever
`data["herds"]` holds and was deliberately left untouched). A client-only flag could never have
fixed that — it can hide things the server sent, never conjure things it withheld.

**Three things share the word "enabled"; keep them apart.**

| | What it is | Written by | Read by |
|---|---|---|---|
| `ClientSettings.fog_of_war_enabled` | the player's persisted **preference** (`user://client_settings.cfg`, `[map]`, default `true`) | the Options toggle, the `F` key | `Main`, to decide whether to send a command |
| snapshot `fog_enabled` | the server's **current state** (top-level bool from `VisionSection.fogEnabled`) | the sim | `Main._sync_fog_of_war` |
| `MapView._fow_enabled` | a **render cache** — plus the **fail-closed initial state** | `Main`, off each snapshot | every fog gate and renderer |

**One direction only:** preference → `set_fog` command → server → snapshot → render. **Never write
`ClientSettings` from a snapshot** — that closes the loop into an echo, where a rejected or
server-overridden command silently rewrites the preference it came from.

**`F` no longer touches MapView.** `Main._toggle_fow_overlay` flips *only* the preference, which is
what makes the hotkey and the Options checkbox one state rather than two that drift. `Main` is the
sole sender: it listens on `ClientSettings.changed` and emits `set_fog on|off` through
`_send_runtime_command`. **`MenuShell` deliberately has no handle to Main / Inspector /
CommandClient** — the Options row writes `ClientSettings` and stops there, which is also why the
row works in the LANDING menu with no server up: the preference just persists.

**The resend guard is `Main._fog_server_state`** — a tri-state (`UNKNOWN` before the first snapshot
carries the key), not a bool, so "haven't heard yet" stays distinct from "heard, and it is off". A
command goes out *only* when the preference disagrees with it. That one guard does double duty: it
stops the `changed` handler and the per-snapshot reconcile from ping-ponging, **and** it is what
applies a persisted "fog off" to a freshly generated world — every snapshot re-checks, so after
`new_game` the disagreement fires one `set_fog` and the next snapshot agrees.

**`_fow_enabled` defaults to `true`, and that default is load-bearing.** It is not merely a cache:
between startup and the first snapshot carrying `fog_enabled` there is nothing to render *from*, so
the flag has to **fail closed** or the client draws a fully-revealed map in that window. `Main._ready`
used to seat it (`set_fow_enabled(true)` before the first world rendered); that seat is gone now the
sim owns the flag, so the declaration at `MapView.gd`'s `_fow_enabled` carries it, matching the
server's `SimulationConfig.fog_enabled` default.

**The offline harnesses must STATE their fog condition, never inherit the default** — this is the
guardrail, and it exists because the absence of it cost a silent regression. When the default was
`false`, `map_preview`'s first five states (`map_band_work` / `_label_overlap` / `_yield_farzoom` /
`_scout` / `_pending`, all saved *before* its first `set_fow_enabled` call) came out unfogged **by
accident**; the day the default flipped they rendered as blank fog with their subject gone. Worse,
`ui_preview`'s `tile_panel_land_sticky` — a *behavioural* guard that clicks a crowded hex and asserts
the land selection survives — kept printing **PASS while asserting nothing**, because fog gated every
band and herd out of `tile_info` and left no occupant to fail to stick to. Both now call
`set_fow_enabled(false)` at their setup site. `blend_probe` was already immune (its first call
precedes its first save) and `band_panel_preview` reads unit *markers*, which are built unfiltered.
**Any new harness state that instances a MapView declares its fog state explicitly**; a frame that is
green because its subject disappeared is worse than one that varies.

**`MapView.set_fow_enabled` stays a public, locally-callable setter** — it is now the *only*
MapView-side fog entry point and `Main` is its only live caller, but `tools/map_preview.gd` (30
call sites) and `tools/blend_probe.gd` (11) drive fog states **offline with no server to ask**, so
it cannot become private or snapshot-only. Its early-out on an unchanged value is what makes the
per-snapshot push free; its side effect of clearing `active_overlay_key` when *enabling* is
load-bearing and must be preserved. Nothing else changed: `_is_tile_visible`,
`_visibility_state_at`, `_apply_visibility_to_info`, `_unit_hidden_by_fog`, the pan clamp and every
renderer gate are untouched, because with fog off the server now sends an all-Active visibility
raster and those gates pass naturally.

**Deltas.** `Main._sync_fog_of_war` takes an `is_delta` flag and returns early if a delta omits the
key. Currently unexercised — the native decoder resolves `fog_enabled` from its own cached `Option`
and always emits it — but the failure mode if that stops holding is silent and ugly: on a delta an
absent key means *unchanged*, not *fog on*, so taking the `true` default would strobe the fog back
on every turn.

**Verify the Options row** with `godot --path clients/godot_thin_client res://tools/menu_preview.tscn`
→ `ui_preview_out/menu_options.png`.

---

## Minimap System

The 2D minimap lives in the HUD **bottom-left** `NavCluster` (an HBox in `BottomBar`,
`HudLayer.tscn`) — a `MinimapContainer` (the map thumbnail with its viewport indicator
rectangle) with a docked **zoom rail** to its right. `MapView._setup_2d_minimap` finds the
container via `Hud.get_minimap_container()`, so the container abstracts the move.

**It is `BottomBar`-resident only while the Band/City panel is docked to a VERTICAL edge.** On a
HORIZONTAL dock (`SIDE_TOP`/`SIDE_BOTTOM`) the whole nav cluster — minimap, zoom rail and its
`NavBacking` — **relocates into a single chrome column at the TRAILING (right) end of that panel's
reserved row, sitting directly ABOVE the turn cluster**, so the chrome shares the row instead of
stacking against it (issue #324; see `ui/hud/DockRowController.gd`). The row's leading end gets no rail
at all — the band zone stays flush to the left edge, as on a vertical dock. Minimap-on-top for BOTH
horizontal edges, so the stack reads the same either way (and on a bottom dock the orb stays where it
already lives, bottom-right). The reparenting is `NavBacking`'s, never `MinimapContainer`'s: the
container keeps its name and identity, so `Hud.get_minimap_container()` and the `MinimapController`
cache `MapView._setup_2d_minimap` takes stay valid across the move. On a top dock the cluster reads
top-right rather than bottom-left, which is intended — the chrome follows the dock.

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

