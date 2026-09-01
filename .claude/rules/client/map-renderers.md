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
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's whole SUBJECT ring — bands, herds, then the land — see Select-then-cycle below), navigation (WASD/QE/mouse), tile picking, and the coordinator for the **layered hex-marker system** (see Map markers below). Three cohesive subsystems are composed out into owned renderer helpers, each holding a `_view: MapView` back-ref and driven from MapView's `_ready`/`_draw` (all shared geometry/glyph/pill/fog primitives + the marker source arrays + selection state stay on MapView): the **2D minimap** (`ui/MinimapController.gd`, `_minimap`), the **primary band markers** (`ui/BandMarkerRenderer.gd`, `_band_markers`), and the **secondary markers** (`ui/SecondaryMarkerRenderer.gd`, `_secondary_markers`). A FOURTH helper owns the terrain rasters: the **terrain textures + Approach-B blend shader** (`ui/TerrainRenderer.gd`, `_terrain`), a FIFTH the **selected-band / selected-herd overlays** (`ui/BandOverlayRenderer.gd`, `_band_overlays`), and a SIXTH the **map annotations** (`ui/AnnotationRenderer.gd`, `_annotations`) — crisis annotations, the Terrain-tab highlight, order routes and command targeting. Still on MapView on the terrain side: the CPU base pass `_draw_terrain_direct` (the frame's base loop, which branches between the helper's textured hex and MapView's solid `_tile_color` fill) and the whole `_cache_*` SubViewport (it caches the non-shader base render and 9 of its 11 invalidation sites are non-terrain). Still on MapView: the `_draw_*` overlay families NOT yet extracted — supply links and the tile selection/hover outline (see the Step-4 report for why each was left). MapView keeps thin same-named pass-throughs for the annotation family's two reflectively-reached seams (`set_targeting`, `set_terrain_highlight`) |
| `ui/MinimapController.gd` | Owns MapView's 2D minimap: the `MinimapPanel` instance, its terrain/FoW image (rebuilt only on grid/data/FoW change), the viewport-indicator overlay and click-to-pan. Holds a `_view: MapView` back-ref; behaviour is identical to the old inlined minimap code |
| `ui/BandMarkerRenderer.gd` | Owns MapView's PRIMARY player-band markers: the offset card-stack of settlement-stage tokens / expedition flag-discs, the faction nameplate banner (+ its reused StyleBoxFlat), the food-days dot, the travel/task arrow, and the ×N over-cap count pill. `_view: MapView` back-ref; `draw_primary_bands()` called during MapView's `_draw`; pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/SecondaryMarkerRenderer.gd` | Owns MapView's SECONDARY markers (herds / food sites / discovered sites / harvest+scout overlays) + the per-frame edge-slot assignment (`compute_slots`) and `+N` overflow chip. Owns only the per-frame slot maps; all draw commands + shared primitives + marker source arrays stay on MapView via the `_view` back-ref. Pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/TerrainRenderer.gd` | Owns MapView's TERRAIN raster/shader subsystem: the Approach-B per-pixel biome-blend shader (the whole-map `TerrainBlendQuad` child + its `ShaderMaterial` + the six per-hex splatmap textures it is fed — id / vis / elev / river-edge / river-channel / navigable-underlying), the blend-OFF per-hex texture cache, the blend-class + canopy/peak code maps, and the `T`-key texture toggle (MapView keeps thin `get_terrain_textures_enabled` / `enable_terrain_textures` pass-throughs for the Inspector/HUD; `CachedMapRenderer` reads the cache via `hex_texture_for`). **All eight tuning-const families live here** (`EDGE_BLEND_*` / `WATER_BLEND_*` / `SHORE_*` / `CANOPY_*` / `PEAK_*` / `RIVER_*` / `BASE_DEFAULT_TEXTURE_SCALE`); `EDGE_BLEND_MIN_RADIUS` + `FOW_EXPLORED/VISIBLE_THRESHOLD` are `const X = MapView.Y` aliases, so each has exactly one definition. `_view: MapView` back-ref — every draw command plus the shared geometry/colour/visibility/river primitives stay on MapView. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on all 286 `map_preview` + `blend_probe` frames (the 4 `map_band_*` frames that vary run-to-run do so identically before and after — the `map_preview` window-maximize race, not this change) |
| `ui/BandOverlayRenderer.gd` | Owns MapView's SELECTED-BAND / SELECTED-HERD overlay family: the three range borders (`FORAGE`/`HUNT`/`SCOUT_RANGE_OUTLINE*`) + worked-forage fills + hunted-herd rings/links, the dashed-amber optimistic PENDING overlay (`LABOR_PENDING_*`), the travel-destination line + reticle (`TRAVEL_DEST_*`), the selected herd's graze-range ring (`HERD_RANGE_*`), the corralled herd's pen footprint (`PEN_FOOTPRINT_*`), and the deferred per-source yield-label batch (`YIELD_LABEL_*`). **Every one of those const families moved here** — each was measured to have no executable reference outside the family, so none needed a `const X = MapView.Y` alias (unlike `TerrainRenderer`); `ICON_MIN_DETAIL_RADIUS` is read as `_view.ICON_MIN_DETAIL_RADIUS`, the `SecondaryMarkerRenderer` idiom. `_view: MapView` back-ref — every draw command plus the shared geometry/hex/glyph/pill primitives (`_hex_center` / `_hex_points` / `_fill_hex` / `_outline_hex` / `_hex_distance` / `_band_effective_col` / `_wrapped_col_delta` / `_is_tile_visible` / `_herd_by_id` / `_draw_marker_glyph` / `_draw_pill_plate` / `_draw_reticle`) and the unit/herd/selection state stay on MapView. It owns only this family's own state: the pushed `_labor_pending` map and the per-frame `_deferred_yield_labels` batch. **FOUR entry points, and their ORDER in `_draw` is the contract**: `draw_band_work_highlights` / `draw_herd_range_highlights` / `draw_pen_footprint_highlight` first (over the tile tints, under the markers), then **`flush_yield_labels` LAST** — after markers, rings, links, pending overlays and targeting, because those layers used to paint over the numbers. **THE YIELD-LABEL LIFECYCLE IS WHOLLY INSIDE THIS FILE** (the decomposition plan's claim that extracting it would split the batch across MapView and a helper was measured FALSE): the clear happens at the top of `draw_band_work_highlights` **before its early-outs** (so a deselected band leaves no stale labels for the flush to paint), the queue is called only from within that same pass, and the flush renders + drains. The far-zoom LOD gate (`show_yields`) stays at the **QUEUE** site, never at the flush, so a suppressed label is never queued and deferral cannot bypass the suppression. **MapView's own selection/hover outline is drawn AFTER all three of those passes** (and still before the markers, so a token reads on top): each stamps a per-tile `_outline_hex` across its ENTIRE disk — the worked-forage one at the selection line's own 3.0 width — so an outline drawn ahead of them is erased on every tile inside a graze / prey-sense / pen disk (issue #405). The selection border is the topmost tile border; punching a hole in a disk instead would misstate the sim's range. `map_preview`'s `map_pasture_herd_range` / `map_predator_prey_sense` / `map_pasture_pen_footprint` states each select an OFF-ANCHOR tile inside the disk, which is what puts that collision in a frame. **`set_labor_pending` stays PUBLIC ON MAPVIEW as a thin same-named pass-through** (it stores here, MapView owns the `queue_redraw`) — `Main.gd` wires the HUD's `labor_pending_changed` signal to it BY NAME via `has_method`/`Callable`, and `tools/map_preview.gd` calls it on the MapView at 13 fixture sites, so that seam cannot move. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 56 `map_preview` frames — 0 differing** (the harness is a strict bit-identity reference since PR #310 pinned its canvas and froze `Engine.time_scale`), plus 205 of 230 `blend_probe` frames; the 25 that moved are the `BANK_*` state, which **varies run-to-run on a clean tree too** — `blend_probe` does NOT freeze time and BANK is its only state carrying a `TIME`-scrolled navigable-river channel |
| `ui/AnnotationRenderer.gd` | Owns MapView's ANNOTATION family — the overlays that say something *about* the map rather than drawing the world: the Crisis overlay's annotations, the Terrain tab's "highlight all tiles of this type" tool, the per-faction order ROUTES, and the command-TARGETING overlay (valid-target glow / reticle / hover-ETA plate). **All four const families live here** (`TERRAIN_HIGHLIGHT_*` / `CRISIS_*` / `ROUTE_*` / `TARGETING_*`), each measured to have no executable reference outside the family; the one exception is `CRISIS_COLOR`, which MapView's `OVERLAY_COLORS` table also reads, so it is a `const CRISIS_COLOR := MapView.CRISIS_COLOR` alias — one definition, two readers. **The fifth family, `TRADE_*`, went with the trade-link overlay** — `draw_trade_overlay`, the three trade-overlay fields, the three reflective seams and the `trade_links` ingest, all removed because the sim publishes no link network (`overlay-channels.md` → "RETIRED", `docs/plan_contact_and_logistics.md`); issue #232's route-network overlay is what replaces it. It owns this family's own state (`_terrain_highlight_id`, `_crisis_annotations`, `_routes`, `_targeting` + its pulse clock); every draw command plus the shared primitives (`_hex_center` / `_hex_points` / `_hex_center_wrapped` / `_unwrapped_path_points` / `_draw_label` / `_draw_reticle` / `_hex_distance` / `_wrapped_col_delta` / `_is_player_unit` / `_get_adjusted_viewport_size`) and the world state it reads (`units` / `herds` / `terrain_overlay` / `tile_lookup` / `faction_colors` / `active_overlay_key` / `_hovered_tile`) stay on MapView via the `_view` back-ref. **A SIXTH FAMILY, `ROAD_*`, DRAWS THE ROADS IN THE GROUND (arc #532) AND IS NOT THE `ROUTE_*` FAMILY BESIDE IT** — that one draws ORDER PATHS, coloured by faction, which vanish with the order; a road is a world object with a stamped path, coloured by rung. Its state lives on `MapView.road_network` / `road_tile_lookup` (world state, read through `_view` exactly as `units` and `herds` are, ingested by `_ingest_road_network` and cleared by `MapView.reset_world_state`), and `draw_road_network` is called from `_draw` right after the crisis annotations — above the tile tints, beneath every marker and outline. **Do not merge or rename the two**; the whole family is `.claude/rules/client/roads.md`'s. **TWO PUBLIC SEAMS KEEP SAME-NAMED PASS-THROUGHS ON MAPVIEW** because both are reached REFLECTIVELY — a rename would not error, it would silently do nothing: `set_targeting` (Main.gd connects the HUD signal by name via `has_method`/`Callable`) and `set_terrain_highlight` (TerrainPanel.gd via `has_method`/`call`). The pass-throughs store here and MapView owns the `queue_redraw` (the `set_labor_pending` idiom); `set_terrain_highlight` returns a bool so its CONDITIONAL redraw survives the move. `_targeting_time` is advanced from MapView's `_process` via `advance_targeting_time`, still gated on `is_targeting_active()`. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on **all 60 `map_preview` frames and all 230 `blend_probe` frames — 0 differing** — and, crucially, four of those `map_preview` frames were **added first** (`map_trade_overlay` / `map_crisis_annotations` / `map_terrain_highlight` / `map_routes`), because before them this family had NO pixel coverage at all and the byte-diff would have proved nothing about it. `map_trade_overlay` has since gone with the overlay it covered; the other three stand |
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

## THE SELECTION OUTLINE WRAPS — a single tile named by its DATA column must be stamped on the
## copy the viewport is over

`selected_tile` and `_hovered_tile` hold **data** columns: `_point_to_offset` posmods the pick into
`[0, grid_width)`. The terrain loop draws a different space — `logical_x` runs past both ends of the
grid and resolves its data through `posmod`, so on a wrapping map the hex under the cursor is
routinely a COPY of its column, a whole map width from where that column's canonical position sits.
An outline drawn from the raw `_hex_center` therefore lands off-frame, and the two spaces agree only
while the viewport happens not to straddle the seam.

**The symptom is not "the outline is in the wrong place" — it is a click that appears not to
register.** Everything else about the selection worked: the tile panel filled in, the roster listed
the hex's occupants, the cycle advanced. Only the white box was missing, on exactly the tiles the
seam had pushed into a wrapped copy — so it read as a flaky hit-test rather than a draw, and the
tiles it happened to on moved with the camera.

So `_draw_tile_selection_highlight` uses **`_outline_hex_wrapped`** (`_hex_center_wrapped`, the
marker idiom) for both outlines. **`_outline_hex` stays, and is still the right call for the range
disks**: `BandOverlayRenderer` resolves the ANCHOR's effective column once (`_band_effective_col`)
and walks its neighbourhood as `eff_col + delta`, so those columns are already in the drawn space —
wrapping each one again would snap every tile back toward the viewport centre independently and tear
a seam-crossing disk into two halves. The rule is which space the column is already in: a data
column wraps, a resolved or logical one does not.

Guarded by `map_preview`'s PNG-less `_assert_selection_outline_wraps` — see
`harness-map-probes.md`, which also says why it reads PIXELS rather than re-asking
`_hex_center_wrapped` the question the draw asks it.

## A CONNECTED PATH IS UNWRAPPED INTO ONE FRAME — it does not wrap point by point

The rule above governs a SINGLE tile. A path — a herd's migration trail, an order route, a multi-hop
crisis annotation — is the other case the same seam produces, and neither of the two idioms on this
page is the answer to it.
`_hex_center` alone draws a herd that has just stepped over the seam as a segment between data
columns `15` and `0`: **one line the full width of the map, at nearly constant row**, over unexplored
ground, which is what it looks like on screen and why it reads as a terrain bug rather than an
overlay one. And `_hex_center_wrapped` per point is worse than useless here — it snaps every point
toward the viewport centre *independently*, tearing the path in half at exactly the seam it was
called to fix (the range-disk reasoning one section up, applied to a polyline).

**`_unwrapped_path_points(tiles, radius, origin)` is the third idiom, and connected geometry uses
it.** It resolves the LAST tile's effective column with `_band_effective_col` — the copy
`_hex_center_wrapped` puts a MARKER on, so a trail's head lands on its own herd — then walks
backwards placing each earlier tile by `_wrapped_col_delta` off the frame already fixed. Every step
is then at most half a map width **by construction**, so a connected path needs none of the
`0.4 * last_map_size.x` skip the DISCONNECTED links carry (supply links, the migration arrow, the
band task arrow, the pending link): there is no artifact to skip. A path that genuinely circles the
world draws longer than one map width, which is the truth about it.

Its callers are the connected paths the client draws: `MapView._draw_herd_trail`,
`AnnotationRenderer._draw_route`, and `AnnotationRenderer.draw_crisis_annotations`' multi-hop branch.
**Only the trail was reported.** The other two are the same defect in the same shape and were fixed
with it rather than because a frame showed them — the route half being latent (nothing publishes
`orders`, so only `map_routes`' fixture has ever fed it).

**Crisis annotations are the case that shows why the count picks the idiom.** That draw ingests a
path and then branches on its length, and the two branches want DIFFERENT helpers: a multi-hop
annotation is a path and takes this one, while a ONE-TILE annotation is a marker and takes
`_hex_center_wrapped` (the section above). Both were built from the raw `_hex_center`, and the
one-tile branch is the LIVE case — `core_sim/src/crisis.rs` publishes `vec![primary_coordinate()]`,
so every annotation a real map carries is one tile, and off the raw centre it draws a whole map width
off-frame. That is the "the overlay just isn't there" symptom, not a misplaced line, which is why it
survived beside a reported one.

**On a NON-wrapping map the walk is an identity**, and so is `_hex_center_wrapped`:
`_band_effective_col` returns the column and `_wrapped_col_delta` the raw difference, so every point
lands where `_hex_center` put it. Measured — the whole `map_preview` set came back **72/72
byte-identical** across the change, every wrapping fixture in it being one that none of these three
draws appears on. **Which is also the limit of what a frame proves here**: `map_crisis_annotations`
and `map_routes` are non-wrapping fixtures, so they show the refactor changed nothing and cannot show
the seam behaviour at all. The only seam claim with pixels behind it is
`_assert_herd_trail_unwraps`, the PNG-less probe in `harness-map-probes.md` — which reads pixels for
the reason the selection guard does.

## A POINTER-DRIVEN NAVIGATION INPUT IS DECLINED WHERE A CONTROL CLAIMS THE PIXEL

**The GUI pass stops a press for the map and stops a scroll for nobody.** Measured in Godot 4.7 by
pushing each event over a `MOUSE_FILTER_STOP` card through `Viewport.push_input`, with a sniffer on
the `_unhandled_input` chain:

| event | over a STOP control | over a PASS control | over open canvas |
|---|---|---|---|
| left press | **consumed** by the GUI pass | survives | survives |
| `InputEventPanGesture` | survives | survives | survives |
| `InputEventMagnifyGesture` | survives | survives | survives |
| wheel button | survives | survives | survives |

So a `STOP` card is a wall to a click and a window to a scroll, and `MapView._unhandled_input` acted
on all three of the survivors — which is why a two-finger scroll over the Materials & Crafting ledger
panned the map underneath it. **On macOS a trackpad or Magic Mouse scroll arrives as a PAN GESTURE,
not a wheel button**, which is why the reported symptom was a pan rather than a zoom.

**`MapView` therefore declines them itself, once, for every surface in the client.** This is not a
per-panel fix: the Telling panel, the compose sheet, the band dock and the Inspector all sit on the
same routing, so a guard written in a panel would be the same guard written five times and missed on
the sixth. `_is_pointer_navigation_input` names the three kinds; `_pointer_claimed_by_ui` decides.

**THE CLAIM TEST IS `MOUSE_FILTER_STOP`, WALKED UP THE ANCESTORS.** `STOP` is the contract this
client already states for presses — `band-city-panel.md`'s `PanelRoot` autopsy: every pixel a `STOP`
control claims is a pixel of dead map — and **a `PASS` control does not claim its pixel**, so it must
not block the map, several HUD containers being `PASS` over visually empty space. The walk is what
makes the leaf reading agree with Godot's own `Viewport::_gui_call_input`: `gui_get_hovered_control`
answers the INNERMOST pickable control, which inside a card is routinely a `PASS` row, and a
**`PASS` child of a `STOP` panel really does have its press eaten by that ancestor** (measured). A
leaf-only test would therefore declare most of a card's own surface unclaimed. The walk stops at the
first `STOP`, at a `top_level` node, or where the chain leaves `Control`s — which is what keeps a
full-screen `MOUSE_FILTER_IGNORE` root, `PanelRoot` being one, out of it entirely.

**`gui_get_hovered_control()` answers `null` over open map**, confirmed rather than assumed: it is
exactly the property `PanelRoot`'s retirement bought, and a full-screen catcher would make the whole
map dead to navigation rather than to clicks alone.

**THE MAP ONLY DECLINES; IT DOES NOT CONSUME.** The guard `return`s without `_mark_input_handled`, so
whatever the pointer is really over stays free to answer. That is what a live `ScrollContainer`
does — and it is also why the fix is not redundant with it: **a container only accepts a pan gesture
it can actually act on**, so the moment the ledger reaches its floor the gesture falls straight
through again. That is the state a player is in when they keep scrolling at the bottom of a list.

**Guarded by EFFECT, never off a `mouse_filter`** — the `band_panel_preview` idiom. The state is
`ui_preview`'s crafting chapter (`harness-ui-preview.md` → "A scroll over the card must not also
drive the map"): every claim is a pairing, since a one-sided one passes on a map that has stopped
answering gestures altogether.

## `_tile_info_at`'s forage-patch cross-ref is a WIRING, and it is guarded

The patch block in `_tile_info_at` copies the `forage_patches` row across key by key from an explicit
list, `patch_`-prefixing each one, and every forage compose sheet reads its source out of that
`tile_info` and nowhere else. **A key the decoder emits but this block omits is silently absent on the
plant web** — no error, no zero to notice — and it has shipped that way five times
(`perWorkerBiomass`/`regrowthSamples`, then `materialPerBiomass`/`perWorkerMaterial`, then
`buildKitId`, which crossed onto no `tile_info` for a release while `SourceForecast.build_kit_id`
read it there and there only, so every forage build quoted no kit at all, and then the
material-half-of-upkeep arc's SEVEN at once — see `harness-headless-guards.md`). Adding a
`ForagePatchState` field is therefore **two edits here**: the copy line, and an entry in
`FOW_DISCOVERED_HIDDEN_KEYS` under the one rule the whole patch payload follows.

**`patch_current_rung` is the worked example of the fog half.** The wire's `currentRung` states the
rung a patch STANDS on (`plant:tended`, `plant:field`) — which is exactly the ladder position
`patch_is_cultivated` / `patch_is_field` are redacted to hide, and the same reading
`patch_carrying_capacity` was added to that list to withhold. A cross-ref line without the redaction
entry re-opens that leak in one token, on a hex the player cannot currently see.

`tools/patch_crossref_guard.gd` enforces both as a partition over this block's own output, so an
omission fails at the wiring rather than in a panel. **Why the copy exists at all, and why no preview
frame can see it break, are one home over in `.claude/rules/client/labor-ui.md`** → "THE PATCH'S
FORECAST FIELDS REACH THE SHEET THROUGH `tile_info`"; do not restate them here.

## `set_overlay_channel`'s FIRST line builds a DEFERRED channel — it is not a channel branch

`DEFERRED_OVERLAY_BUILDERS` is a `{key: builder method}` table and `_realize_deferred_overlay` is the
one thing that reads it. A channel in it is **not** synthesized during the snapshot ingest the way
`province` is; it is built the first time each frame that anything asks for it, and the per-turn
refresh falls out of the overlay picker re-asserting the painted channel on `overlay_channels_ingested`.

Two things to hold to if you touch that function. **The realize call has to stay ahead of the
`overlay_channels.has(key)` test**, which would otherwise refuse a channel this renderer has simply
not built yet. And it **names no channel on purpose** — `docs/plan_knowledge_screen.md` §6b forbids a
second `if key ==` in the render path, so a new deferred channel is a row in that table and nothing
else.

Why any of it is deferred (a `RungGates` pass per SOURCE, measured at ~331 ms for a full-size world's
worth) belongs to the channel that needed it: `.claude/rules/client/overlay-channels.md` →
`ready_for_improvement`. **Do not restate it here**; one home per fact.

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
`_assert_zoom_ladder` (see `harness-map-probes.md`), because every rung renders as a plausible map and
the harness pins the speed slider anyway, so only an assertion can see a regression here.

The map view displays this minimap showing the full map with a viewport indicator rectangle.

### Component (`ui/MinimapPanel.gd`)
Reusable minimap UI component handling:
- CanvasLayer hierarchy setup (layer 102)
- Aspect ratio sizing from grid dimensions
- Click-to-pan with drag support
- Viewport indicator overlay with draw callbacks
- The **map-overlay picker docked on its TOP BORDER** (`ui/overlay/OverlayPicker.gd`, built by both
  setup paths; `MinimapController` hands it the MapView). Docked ON the border rather than floated
  beside the panel, so it costs the nav cluster no width and cannot be mistaken for a zoom control —
  see `overlay-channels.md` → "The picker is three modules on the MINIMAP's border"

**THE PICKER HANGS OFF `texture_rect`, and neither node above it would have worked.** `panel` is a
`PanelContainer` and, embedded, its own parent is the HUD's `MinimapContainer` `MarginContainer` —
both lay a second child out on top of the first, so the button would have covered the map. A
`TextureRect` is neither a container nor a clipper: its children sit where they are put, and the part
of the button reaching above its top edge is not cut off, which is what puts the button ON the border
instead of inside the map. It is added AFTER `viewport_indicator` so it draws over it.

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

## Typing must not drive the game

Both of this node's keyboard readers ask `KeyboardArbiter` first: the POLLED pan/zoom in `_process`
(which never enters the event system, so a focused field does not starve it) and the RAW `C`/`H`/`T`
in `_unhandled_input` (which a focused `LineEdit` does not starve either — it consumes the keys it
USES and lets the rest fall through). The registry, the three-owner policy, exact matching and the
focus release they depend on are `.claude/rules/client/keyboard-arbiter.md`. **The targeting Escape
is the one deliberately unarbitrated key here** — `ESCAPE` acts under every owner.
