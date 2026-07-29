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
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's whole SUBJECT ring — bands, herds, then the land — see Select-then-cycle below), navigation (WASD/QE/mouse), tile picking, and the coordinator for the **layered hex-marker system** (see Map markers below). Three cohesive subsystems are composed out into owned renderer helpers, each holding a `_view: MapView` back-ref and driven from MapView's `_ready`/`_draw` (all shared geometry/glyph/pill/fog primitives + the marker source arrays + selection state stay on MapView): the **2D minimap** (`ui/MinimapController.gd`, `_minimap`), the **primary band markers** (`ui/BandMarkerRenderer.gd`, `_band_markers`), and the **secondary markers** (`ui/SecondaryMarkerRenderer.gd`, `_secondary_markers`). A FOURTH helper owns the terrain rasters: the **terrain textures + Approach-B blend shader** (`ui/TerrainRenderer.gd`, `_terrain`), a FIFTH the **selected-band / selected-herd overlays** (`ui/BandOverlayRenderer.gd`, `_band_overlays`), and a SIXTH the **map annotations** (`ui/AnnotationRenderer.gd`, `_annotations`) — trade links, crisis annotations, the Terrain-tab highlight, order routes and command targeting. Still on MapView on the terrain side: the CPU base pass `_draw_terrain_direct` (the frame's base loop, which branches between the helper's textured hex and MapView's solid `_tile_color` fill) and the whole `_cache_*` SubViewport (it caches the non-shader base render and 9 of its 11 invalidation sites are non-terrain). Still on MapView: the `_draw_*` overlay families NOT yet extracted — supply links and the tile selection/hover outline (see the Step-4 report for why each was left). MapView keeps thin same-named pass-throughs for the annotation family's five reflectively-reached seams (`set_targeting`, `update_trade_overlay`, `set_trade_overlay_enabled`, `set_trade_overlay_selection`, `set_terrain_highlight`) |
| `ui/MinimapController.gd` | Owns MapView's 2D minimap: the `MinimapPanel` instance, its terrain/FoW image (rebuilt only on grid/data/FoW change), the viewport-indicator overlay and click-to-pan. Holds a `_view: MapView` back-ref; behaviour is identical to the old inlined minimap code |
| `ui/BandMarkerRenderer.gd` | Owns MapView's PRIMARY player-band markers: the offset card-stack of settlement-stage tokens / expedition flag-discs, the faction nameplate banner (+ its reused StyleBoxFlat), the food-days dot, the travel/task arrow, and the ×N over-cap count pill. `_view: MapView` back-ref; `draw_primary_bands()` called during MapView's `_draw`; pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/SecondaryMarkerRenderer.gd` | Owns MapView's SECONDARY markers (herds / food sites / discovered sites / harvest+scout overlays) + the per-frame edge-slot assignment (`compute_slots`) and `+N` overflow chip. Owns only the per-frame slot maps; all draw commands + shared primitives + marker source arrays stay on MapView via the `_view` back-ref. Pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/TerrainRenderer.gd` | Owns MapView's TERRAIN raster/shader subsystem: the Approach-B per-pixel biome-blend shader (the whole-map `TerrainBlendQuad` child + its `ShaderMaterial` + the six per-hex splatmap textures it is fed — id / vis / elev / river-edge / river-channel / navigable-underlying), the blend-OFF per-hex texture cache, the blend-class + canopy/peak code maps, and the `T`-key texture toggle (MapView keeps thin `get_terrain_textures_enabled` / `enable_terrain_textures` pass-throughs for the Inspector/HUD; `CachedMapRenderer` reads the cache via `hex_texture_for`). **All eight tuning-const families live here** (`EDGE_BLEND_*` / `WATER_BLEND_*` / `SHORE_*` / `CANOPY_*` / `PEAK_*` / `RIVER_*` / `BASE_DEFAULT_TEXTURE_SCALE`); `EDGE_BLEND_MIN_RADIUS` + `FOW_EXPLORED/VISIBLE_THRESHOLD` are `const X = MapView.Y` aliases, so each has exactly one definition. `_view: MapView` back-ref — every draw command plus the shared geometry/colour/visibility/river primitives stay on MapView. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on all 286 `map_preview` + `blend_probe` frames (the 4 `map_band_*` frames that vary run-to-run do so identically before and after — the `map_preview` window-maximize race, not this change) |
| `ui/BandOverlayRenderer.gd` | Owns MapView's SELECTED-BAND / SELECTED-HERD overlay family: the three range borders (`FORAGE`/`HUNT`/`SCOUT_RANGE_OUTLINE*`) + worked-forage fills + hunted-herd rings/links, the dashed-amber optimistic PENDING overlay (`LABOR_PENDING_*`), the travel-destination line + reticle (`TRAVEL_DEST_*`), the selected herd's graze-range ring (`HERD_RANGE_*`), the corralled herd's pen footprint (`PEN_FOOTPRINT_*`), and the deferred per-source yield-label batch (`YIELD_LABEL_*`). **Every one of those const families moved here** — each was measured to have no executable reference outside the family, so none needed a `const X = MapView.Y` alias (unlike `TerrainRenderer`); `ICON_MIN_DETAIL_RADIUS` is read as `_view.ICON_MIN_DETAIL_RADIUS`, the `SecondaryMarkerRenderer` idiom. `_view: MapView` back-ref — every draw command plus the shared geometry/hex/glyph/pill primitives (`_hex_center` / `_hex_points` / `_fill_hex` / `_outline_hex` / `_hex_distance` / `_band_effective_col` / `_wrapped_col_delta` / `_is_tile_visible` / `_herd_by_id` / `_draw_marker_glyph` / `_draw_pill_plate` / `_draw_reticle`) and the unit/herd/selection state stay on MapView. It owns only this family's own state: the pushed `_labor_pending` map and the per-frame `_deferred_yield_labels` batch. **FOUR entry points, and their ORDER in `_draw` is the contract**: `draw_band_work_highlights` / `draw_herd_range_highlights` / `draw_pen_footprint_highlight` first (over the tile tints, under the markers), then **`flush_yield_labels` LAST** — after markers, rings, links, pending overlays and targeting, because those layers used to paint over the numbers. **THE YIELD-LABEL LIFECYCLE IS WHOLLY INSIDE THIS FILE** (the decomposition plan's claim that extracting it would split the batch across MapView and a helper was measured FALSE): the clear happens at the top of `draw_band_work_highlights` **before its early-outs** (so a deselected band leaves no stale labels for the flush to paint), the queue is called only from within that same pass, and the flush renders + drains. The far-zoom LOD gate (`show_yields`) stays at the **QUEUE** site, never at the flush, so a suppressed label is never queued and deferral cannot bypass the suppression. **MapView's own selection/hover outline is drawn AFTER all three of those passes** (and still before the markers, so a token reads on top): each stamps a per-tile `_outline_hex` across its ENTIRE disk — the worked-forage one at the selection line's own 3.0 width — so an outline drawn ahead of them is erased on every tile inside a graze / prey-sense / pen disk (issue #405). The selection border is the topmost tile border; punching a hole in a disk instead would misstate the sim's range. `map_preview`'s `map_pasture_herd_range` / `map_predator_prey_sense` / `map_pasture_pen_footprint` states each select an OFF-ANCHOR tile inside the disk, which is what puts that collision in a frame. **`set_labor_pending` stays PUBLIC ON MAPVIEW as a thin same-named pass-through** (it stores here, MapView owns the `queue_redraw`) — `Main.gd` wires the HUD's `labor_pending_changed` signal to it BY NAME via `has_method`/`Callable`, and `tools/map_preview.gd` calls it on the MapView at 13 fixture sites, so that seam cannot move. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 56 `map_preview` frames — 0 differing** (the harness is a strict bit-identity reference since PR #310 pinned its canvas and froze `Engine.time_scale`), plus 205 of 230 `blend_probe` frames; the 25 that moved are the `BANK_*` state, which **varies run-to-run on a clean tree too** — `blend_probe` does NOT freeze time and BANK is its only state carrying a `TIME`-scrolled navigable-river channel |
| `ui/AnnotationRenderer.gd` | Owns MapView's ANNOTATION family — the overlays that say something *about* the map rather than drawing the world: the Trade tab's diffusion links, the Crisis overlay's annotations, the Terrain tab's "highlight all tiles of this type" tool, the per-faction order ROUTES, and the command-TARGETING overlay (valid-target glow / reticle / hover-ETA plate). **All five const families live here** (`TERRAIN_HIGHLIGHT_*` / `CRISIS_*` / `TRADE_*` / `ROUTE_*` / `TARGETING_*`), each measured to have no executable reference outside the family; the one exception is `CRISIS_COLOR`, which MapView's `OVERLAY_COLORS` table also reads, so it is a `const CRISIS_COLOR := MapView.CRISIS_COLOR` alias — one definition, two readers. It owns this family's own state (`_terrain_highlight_id`, the three trade-overlay fields, `_crisis_annotations`, `_routes`, `_targeting` + its pulse clock); every draw command plus the shared primitives (`_hex_center` / `_hex_points` / `_hex_center_wrapped` / `_draw_label` / `_draw_reticle` / `_hex_distance` / `_wrapped_col_delta` / `_is_player_unit` / `_get_adjusted_viewport_size`) and the world state it reads (`units` / `herds` / `terrain_overlay` / `tile_lookup` / `faction_colors` / `active_overlay_key` / `_hovered_tile`) stay on MapView via the `_view` back-ref. **FIVE PUBLIC SEAMS KEEP SAME-NAMED PASS-THROUGHS ON MAPVIEW** because every one is reached REFLECTIVELY — a rename would not error, it would silently do nothing: `set_targeting` (Main.gd connects the HUD signal by name via `has_method`/`Callable`), `update_trade_overlay` / `set_trade_overlay_enabled` / `set_trade_overlay_selection` (TradePanel.gd via `has_method`/`call`), and `set_terrain_highlight` (TerrainPanel.gd via `has_method`/`call`). The pass-throughs store here and MapView owns the `queue_redraw` (the `set_labor_pending` idiom); the two setters that only redrew CONDITIONALLY return a bool so that condition survives the move. `_targeting_time` is advanced from MapView's `_process` via `advance_targeting_time`, still gated on `is_targeting_active()`. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 60 `map_preview` frames and all 230 `blend_probe` frames — 0 differing** — and, crucially, four of those `map_preview` frames were **added first** (`map_trade_overlay` / `map_crisis_annotations` / `map_terrain_highlight` / `map_routes`), because before them this family had NO pixel coverage at all and the byte-diff would have proved nothing about it |
## Select-then-cycle — one hex, one ring of SUBJECTS

Re-clicking the already-selected hex advances the selection through **everything the tile panel
lists**: every band, every herd, and the LAND. `_selection_cycle_on_tile` **is** that ring —
`_occupants_on_tile` (every `_units_on_tile` entry, then every `_herds_on_tile` entry, each tagged
`{kind, data}` via `OCCUPANT_KEY_*` / `OCCUPANT_KIND_*`) with `LAND_CYCLE_ENTRY` appended. So a hex
with one animal toggles herd ↔ land, a hex with one band and two herds runs band → herd A → herd B
→ land → band, and no row of the panel is unreachable from the map.

**Order: bands, then herds, then LAND last.** Bands-then-herds is the older half of the contract — a
band still wins the first click on a shared hex — and putting the land at the END is what keeps the
FIRST click on a fresh hex landing on the top occupant, which is also the HUD's own fresh-hex
precedence (`SelectionCardController._resolve_auto_selected_subject`: first unit → first herd →
land). Composing the occupant half out of the two existing helpers rather than re-matching
coordinates is what makes **both fog gates** (a foreign band under fog via `_unit_hidden_by_fog`, a
herd on an unseen hex via `_is_tile_visible`) hold here by construction — which is also why
`_occupants_on_tile` stays occupant-ONLY and the land joins one level up, in the function whose name
says "cycle": the land is not an occupant and is not fog-gated the same way.

**The land joins only when the hex has an occupant, so the EMPTY hex is untouched.** A bare hex
yields an EMPTY cycle, exactly as before, and the click falls through `_handle_entity_selection`'s
clear branch to `selection_cleared` → `Hud.clear_selection`. A one-member `[land]` cycle would have
retired that path; `ui_preview`'s `tile_panel_deselect_keeps_tile` guards it.

The ring is the cycle because a band-only cycle made herds unreachable. `_handle_entity_selection`
used to take `herds_here[0]`, and only when the hex held no units at all: a multi-herd hex always
opened on the same herd, and a herd sharing a hex with any band could not be selected from the map
at any number of clicks (issue #429).

### A map-driven LAND pick must reach the HUD as a DELIBERATE choice

The land stop needs a signal of its own, and this is the reason. `_handle_entity_selection`'s land
branch clears `selected_unit_id` / `selected_herd_id` and emits neither `unit_selected` nor
`herd_selected` — there is no occupant — so on its own the HUD would see the same two empty occupant
dicts a fresh hex shows it, and `_resolve_auto_selected_subject` would auto-pick the first band
straight back through `roster_occupant_selected` → `Main` → `select_occupant`. The land stop would be
invisible. **This is the inverse of the herd case**, where a non-empty occupant dict suppresses the
auto-pick with no help from anyone.

So the branch emits **`land_selected`**, MapView's fourth map→HUD selection signal, wired in
`Main.gd` beside `unit_selected` / `herd_selected` / `selection_cleared` and relayed to
`Hud.show_land_selection` — the third `show_*` entry point, twin of `show_unit_selection` /
`show_herd_selection`. It carries **no payload**: the `_emit_tile_selection` one call earlier in the
same click already handed the HUD this hex's `tile_info`, which is the whole of the land subject
(the guarantee `selection_cleared` leans on too). `show_land_selection` goes through
`SelectionCardController.select_land_subject` — the same `note_choice_tile` + `select_land` pair the
panel's land ROW click uses — which is the load-bearing part: the recorded choice tile is what tells
the auto-pick a decided hex from a fresh one, both on that render and on every later
`reapply_selection("tile", …)`. `ui_preview`'s `tile_panel_occupant_cycle` asserts the land stop
both at the click and after a `refresh_selection_payload` → `reapply_selection` round trip, and 4 of
its assertions fail when `show_land_selection` drops down to a bare `select_land()`.

**The advance is derived from the SELECTED SUBJECT'S IDENTITY, not from the stored index.**
`_selected_cycle_index` finds `selected_unit_id` / `selected_herd_id` in the ring and the click takes
the next entry, falling back to `cycle_index` only when nothing on the hex is selected. That keeps
the map click coherent with a panel roster-row click (pick Wildlife row 3 in the panel, re-click the
hex, get row 4) and survives the occupant array reordering between snapshots. **Neither id set means
the LAND is the selection** — it is the state `select_occupant("land")` and the land branch both
leave behind — and since the only caller sits inside `handle_hex_click`'s `== selected_tile` guard,
that state names the land stop of THIS hex. Answering the land's own index is what makes the next
click advance OFF the land to the first occupant instead of restarting from `cycle_index`.

**`cycle_index` is written by `_handle_entity_selection` from a PARAMETER, never read back out of
the member mid-click.** `handle_hex_click` computes the next index into a LOCAL, because the
`_emit_tile_selection` that runs between the computation and the selection re-enters
`select_occupant` synchronously — `tile_selected` → `Hud.show_tile_selection` →
`SelectionCardController.render` → the fresh-hex auto-pick → `roster_occupant_selected` → `Main` →
`MapView.select_occupant` — which rewrites `cycle_index` to the FIRST occupant. Reading the member
after that re-entrancy pins every re-click on the top of the stack; `ui_preview`'s
`tile_panel_occupant_cycle` fails on exactly that mutation.

**The click builds the ring ONCE and passes it down.** `handle_hex_click` calls
`_selection_cycle_on_tile` and hands the resulting array to `_handle_entity_selection` alongside the
index, so the index is applied to the very list it was computed against rather than to a rebuild
that merely happens to match, and each occupant is deep-copied once per click instead of twice.
Those copies are also why the selected entry's `data` becomes the emitted payload **uncopied**:
`_units_on_tile`/`_herds_on_tile` already handed back private deep copies, and the ring is a
click-local temporary, so stamping `tile_info` onto one mutates nothing the decoder holds (the
"never write into a held snapshot sub-tree" rule of `turn-profiling.md` is satisfied by that first
copy). `LAND_CYCLE_ENTRY` is the one entry that carries no data and is never stamped — a `const`
Dictionary, hence read-only, so it cannot be.

`cycle_index` remains the stored per-world state (cleared with the selection triplet — see
`../core_sim/world-handoff.md`), and `select_occupant` keeps it in sync for ALL THREE kinds through
`_occupant_cycle_index`, so a roster herd click leaves it pointing at that herd and a panel land-row
click at the land — from where the next map click continues to the top of the ring. The land's own
lookups follow from its having no id: `_occupant_home_tile` answers `selected_tile` (the land IS that
hex) and `_occupant_matches` tests the kind alone (a hex holds exactly one land).

**The HUD's sticky-choice guard does not undo a map-driven cycle**, and needs no `note_choice_tile`
from this path: `_resolve_auto_selected_subject` only runs while both occupant dicts are empty, and
the click's `herd_selected` → `Hud.show_herd_selection` → `select_herd` lands after the auto-pick,
so the following snapshot's `refresh_selection_payload` answers `"herd"` and `reapply_selection`
restates it.

## Fog of war

`_fow_enabled`, `set_fow_enabled` and every downstream fog gate live in `MapView`, but the
rule that governs them does **not** live here — fog of war is server-owned and the flow spans
`MapView` + `Main` + `MenuShell` + `ClientSettings`. See `.claude/rules/client/fog-of-war.md`,
which loads on all four. **Do not restate its invariants here**; one home per fact.

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

**The rail is a SNAPPED LADDER, and it is the one zoom path the speed slider does not touch.**
Rungs sit every `ZOOM_BUTTON_STEP` from `MIN_ZOOM_FACTOR`; a click moves to the ADJACENT rung and
clamps to `[MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR]`. It still expresses that move as a delta through
`_apply_zoom`, so "one map-zoom code path" is unchanged — this is the `set_zoom_factor` idiom, not a
second path. Two decisions:

- **Unscaled by `ClientSettings.zoom_speed_multiplier`.** That slider means *speed*, and speed
  belongs to the CONTINUOUS inputs — wheel, pinch, `Q`/`E` — which still read it. The rail is a
  DISCRETE, deliberate step, which is what `ZOOM_BUTTON_STEP`'s own comment says it is for. Scaling
  it made the ladder unpredictable and its offset depend on where you last were: at the slider's max
  (3.0) each click became 1.5, so the rail ran 1.0 → 2.5 → 4.0 → 5.5 → 7.0 with **no 6.0 or 6.5**,
  and from `Main.STARTUP_ZOOM_FACTOR` (2.0) it ran a different ladder again (2.0 → 3.5 → 5.0 → 6.5)
  — which is why it appeared to behave differently on the huge map. Same precedent as mouse-drag
  pan, which is deliberately 1:1.
- **Snapped, not accumulated.** The wheel and pinch use their own step and leave `zoom_factor`
  off-grid; without snapping the readout could never get back onto the ladder. From 3.27 a `+1` goes
  to 3.5 and a `-1` to 3.0 — the adjacent rung in the direction of travel — so one click always
  restores a round readout. `ZOOM_RUNG_EPSILON` (in RUNGS) absorbs the float drift `_apply_zoom`'s
  pivot math leaves, so a value a hair below a rung still counts as ON it and advances a whole rung
  instead of degenerating into a near-zero step.

`MAX_ZOOM_FACTOR` need not lie on the ladder; the clamp merely makes the topmost click a short one
(at today's 7.0 it does lie on it, exactly 12 rungs up). At either limit the delta is 0 and
`_apply_zoom`'s `is_zero_approx` early-out makes the click a clean no-op with no spurious
`zoom_changed`. **The ladder is guarded behaviourally, not by a picture** — `map_preview`'s
`_assert_zoom_ladder` (see `test-harnesses.md`), because every rung renders as a plausible map and
the harness pins the speed slider anyway, so only an assertion can see a regression here.

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

