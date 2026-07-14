extends Node2D

## Dev-only MapView preview harness (companion to tools/ui_preview.gd, which is HUD-only).
##
## Instances the real MapView, feeds a canned snapshot via display_snapshot(), selects a
## player band, renders each state, and saves a PNG to ui_preview_out/. Lets us visually
## verify the selected-band labor highlights (work-range ring / worked forage tiles /
## hunted-herd ring + link) without a server. Run windowed (NOT headless —
## the dummy renderer can't read back the viewport):
##
##   godot --path . res://tools/map_preview.tscn
##
## then read ui_preview_out/map_*.png.

const MAP_VIEW := preload("res://src/scripts/MapView.gd")
const OUT_DIR := "res://ui_preview_out"
const WARMUP_SETTLES := 3   # frames burned before the first capture (the window is still sizing)

const GRID_W := 16
const GRID_H := 12
const BAND_ENTITY := 9001
const BAND_X := 8
const BAND_Y := 6
const TERRAIN_ID := 5  # arbitrary land biome for a legible backdrop
const STACK_ENTITY_BASE := 9100   # co-located band entities are STACK_ENTITY_BASE + i
const TRAVEL_SEAM_BAND_X := 1      # band column near the left edge for the seam-crossing case
const TRAVEL_SEAM_TARGET_X := 14   # target near the right edge → short path wraps LEFT across seam
const TRAVEL_EXPEDITION_ENTITY := 9301
const HERD_ON_TILE_ID := "game_boar_03"   # herd id used by the selected-hex herd fixture
# First worked forage tile of the work fixture — named because the draw-order guard (State A-overlap)
# parks a herd on exactly this tile so its glyph collides with that tile's yield label.
const FORAGE_A_X := 7
const FORAGE_A_Y := 6
const OVERLAP_HERD_ID := "game_boar_11"   # the herd parked on the worked forage tile
const OVERLAP_MOVE_X := 10                # pending-move target; band→target dash crosses forage tile B's label
const OVERLAP_MOVE_Y := 10
# Canned settlement-stage tokens (the native bridge doesn't run here, so preview band dicts must
# carry settlement_stage_* directly). Icons are opaque sim strings — the emoji here just mirror the
# current config so the map token glyphs render. EMPTY exercises the neutral non-circular fallback marker (square).
const STAGE_NOMADIC := {"id": "nomadic", "label": "Nomadic band", "icon": "⛺"}
const STAGE_CAMP := {"id": "camp", "label": "Seasonal camp", "icon": "🛖"}
const STAGE_VILLAGE := {"id": "village", "label": "Village", "icon": "🏘️"}
const STAGE_NONE := {"id": "", "label": "", "icon": ""}   # pre-stage / missing → neutral non-circular fallback marker
# Stage cycle used to fan mixed glyphs across a co-located band stack.
const STACK_STAGE_CYCLE := [STAGE_NOMADIC, STAGE_CAMP, STAGE_VILLAGE, STAGE_NONE]
# Far-zoom LOD grid: large enough that fitted hexes fall under ICON_MIN_DETAIL_RADIUS.
const FAR_GRID_W := 72
const FAR_GRID_H := 52
# Yield-label LOD guard grid. `_fit_map_to_view` IS the minimum zoom (MIN_ZOOM_FACTOR == 1.0), so the
# only way to push the fitted radius under the LOD gate is a bigger grid — and at this harness's
# 1000×800 window FAR_GRID (72×52) fits at radius ≈19.6, i.e. ABOVE the gate, so the state had
# silently stopped guarding anything. This grid fits at radius ≈13 (< LOD_MIN_RADIUS), so the
# yield-label suppression is genuinely exercised. `_ready` asserts the radius, so a future window/grid
# change can't silently un-guard it again.
const YIELD_FAR_GRID_W := 110
const YIELD_FAR_GRID_H := 80
# Mirrors MapView.ICON_MIN_DETAIL_RADIUS (the LOD threshold under which the annotation is suppressed).
const LOD_MIN_RADIUS := 16.0
# Multi-biome baseline: the four terrain ids that today have REAL base textures (the other 33 are
# noise placeholders), laid out as four vertical bands 4 columns wide each across GRID_W (16).
const BIOME_BAND_IDS := [15, 11, 12, 0]  # hot_desert_erg / prairie_steppe / mixed_woodland / deep_ocean
const BIOME_BAND_COLS := 4               # GRID_W (16) / 4 bands
const BIOME_OCEAN_ID := 0                # deep_ocean, blend_class "water"
# An ocean bay carved into the upper cols 8+ (rows 0..BIOME_BAY_ROWS-1) so the ocean ALSO borders the
# prairie band (a flat-land↔water coast at col 7↔8) alongside the woodland↔ocean coast at col 11↔12 —
# exercises beach+foam on BOTH a grassy and a wooded shore.
const BIOME_BAY_ROWS := 6
const BIOME_BAY_COL_MIN := 8
# State S (terrain-repetition repro): a large field of a DETAILED rugged texture (alpine, id 26 — staged
# to reproduce the per-hex repeating grid) bordering a flat prairie band (id 11). BEFORE the world-space
# base fix every alpine hex was an identical texture copy → an obvious grid with diagonal seams; AFTER,
# the base samples continuous world space so one texture spans several hexes and the grid is gone.
const REPEAT_ALPINE_ID := 26   # rugged, detailed staged texture
const REPEAT_PRAIRIE_ID := 11  # flat prairie boundary band
const REPEAT_PRAIRIE_COLS := 4 # left columns prairie; the rest alpine
# State "swatch" (reusable AI-texture check): a LARGE field of a single configurable biome bordering a
# known-good prairie band, so we can judge (a) the biome's own tiling and (b) cohesion + the flat↔flat
# blend against prairie. SWATCH_BIOME_ID is the ONE lever — change it to preview a different biome.
const SWATCH_BIOME_ID := 2            # the biome id rendered in the swatch harness — one-line change to preview any biome
const SWATCH_PRAIRIE_ID := 11         # prairie_steppe, the accepted flat neighbour to blend against
const SWATCH_PRAIRIE_COLS := 4        # left columns prairie (of GRID_W); the rest the swatch biome
const SWATCH_FAR_PRAIRIE_COLS := 18   # left prairie columns on the far-zoom grid (of FAR_GRID_W)
# State "cohesion" (accepted-set whole-set check): the FIVE accepted AI biomes laid out as vertical
# bands left→right — desert · scrub · prairie · woodland · tundra — so the set can be judged as one art
# family (stylization/palette/detail cohesion, per-biome distinctiveness, and the flat↔flat blends at
# every adjacent seam, all `flat`). Rendered at two zooms like State Q: a normal-zoom grid (a few hex
# columns per band) and a far-zoom grid (hexes go small, whole-region read).
const COHESION_BIOME_IDS := [15, 17, 11, 12, 20]  # desert · scrub · prairie · woodland(canopy) · tundra
const COHESION_GRID_W := 20            # 4 hex columns per band (COHESION_GRID_W / 5)
const COHESION_GRID_H := 12
const COHESION_FAR_GRID_W := 70        # 14 columns per band on the far-zoom grid → tiny hexes
const COHESION_FAR_GRID_H := 52
# State R (pan/zoom swim regression): a target hex solidly inside the mixed_woodland band (cols 8–11)
# on a LOWER row (below the bay) so tree crowns are in the crop. The pan and crop context are in units
# of the frame's hex radius so the SAME hex stays framed across fit/pan/zoom.
const SWIM_TARGET_COL := 9
const SWIM_TARGET_ROW := 8
const SWIM_PAN_COLS := 3.0    # pan right by this many hex-radii (viewport is wide → stays on-screen)
const SWIM_PAN_ROWS := -2.0   # pan UP by this many hex-radii → nudges the low target hex toward the
                              # viewport center so the crop stays unclamped (equal-sized fit vs pan crops)
const SWIM_CROP_RADII := 2.4  # crop half-size = this × hex_radius → a couple hexes of context, small
                              # enough to stay within bounds after the pan/zoom on the short viewport

var _map: Node2D

func _ready() -> void:
	get_window().size = Vector2i(1000, 800)
	DirAccess.make_dir_absolute(OUT_DIR)
	_map = MAP_VIEW.new()
	add_child(_map)
	await get_tree().process_frame
	await get_tree().process_frame
	# Warm-up: the FIRST captured state came back all-black — the window is still sizing on the opening
	# frames, so the first viewport read-back has nothing in it. Burn a few settles here so State A is a
	# real frame like every state after it.
	for _i in WARMUP_SETTLES:
		await _settle()

	# State A — a band working two forage tiles + hunting a distant herd. Shows the
	# work-range ring (Chebyshev square), two strong-green worked forage tiles, and the
	# red herd ring + band→herd link (the herd sits OUTSIDE the ring: hunt reach = range + leash).
	_map.display_snapshot(_snapshot_work())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_work")

	# State A-overlap — the draw-ORDER guard for the yield labels. Every layer that used to paint OVER
	# them is forced to collide with one here: a herd parked ON a worked forage tile (its glyph lands in
	# a secondary slot right under that tile's label), a pending hunt on the already-hunted deer (dashed
	# hex + dashed band→herd link straight across the herd's label, on top of the confirmed red ring +
	# link), and a pending move whose dashed link crosses the second forage tile's label. The labels are
	# flushed LAST in _draw, so all of it must read UNDER the pills — no glyph or dash on the numbers.
	_map.display_snapshot(_snapshot_work_overlap())
	_map.selected_unit_id = BAND_ENTITY
	_map.set_labor_pending({
		BAND_ENTITY: {
			"turn": 0,
			"assign": {"hunt:game_deer_07": {"kind": "hunt", "x": 13, "y": 6, "herd_id": "game_deer_07"}},
			"move": {"x": OVERLAP_MOVE_X, "y": OVERLAP_MOVE_Y},
		}
	})
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_label_overlap")
	_map.set_labor_pending({})  # leave the pending overlay clear for the following states

	# State A-far — the SAME worked band on a large grid so fitted hexes go tiny (radius <
	# ICON_MIN_DETAIL_RADIUS): the per-source yield labels + ⚠ must LOD-SUPPRESS so far zoom stays a
	# clean token/highlight view, not floating-text soup. Regression guard for the yield-label LOD gate.
	_map.display_snapshot(_snapshot_far_work())
	_map.selected_unit_id = BAND_ENTITY
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	if _map.last_hex_radius >= LOD_MIN_RADIUS:
		push_warning("map_preview: yield-farzoom fitted radius %.1f >= LOD gate %.1f — this state no longer guards the LOD suppression; grow YIELD_FAR_GRID_*" % [_map.last_hex_radius, LOD_MIN_RADIUS])
	await _save("map_band_yield_farzoom")

	# State B — the same band with scouts staffed: scouting no longer draws a map highlight
	# (its effect is the extended sight visible in the fog; `scout_reveal_radius` is a
	# sight-range bonus, not a reveal disc). This state is a regression guard that NO blue
	# scouted disc appears — only the work-range ring + the single worked forage tile.
	_map.display_snapshot(_snapshot_scout())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_scout")

	# State C — optimistic pending overlay: a just-issued forage assign (new tile) + a pending
	# move destination show in a distinct dashed-amber style, over the confirmed highlights.
	_map.display_snapshot(_snapshot_work())
	_map.selected_unit_id = BAND_ENTITY
	_map.set_labor_pending({
		BAND_ENTITY: {
			"turn": 0,
			"assign": {"forage:6,7": {"kind": "forage", "x": 6, "y": 7, "herd_id": ""}},
			"move": {"x": 8, "y": 9},
		}
	})
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_pending")

	# State D — Wondrous Sites: a landmark (⛰) and a settle-site (⛲) glyph marker, plus one
	# placed on the herd tile to exercise the overlap nudge (offset up so both stay legible).
	_map.set_labor_pending({})  # clear State C's pending overlay so this frame reads clean
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_sites())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_sites")

	# State E — persistence under fog: FoW on, every tile only Discovered (remembered) except the
	# band's own hex (Active). A discovered site is permanent knowledge, so all three glyph markers
	# must STILL render on the fogged/remembered tiles (unlike the Active-only herd/food markers).
	_map.set_fow_enabled(true)
	_map.display_snapshot(_snapshot_sites_fogged())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_sites_fogged")

	# State F — scouting expeditions (docs/plan_exploration_and_sites.md §2): alongside the
	# resident band (solid faction dot, unchanged) two detached parties render as hollow
	# flag discs — one Outbound, one Awaiting-orders (pulsing amber ring). Verifies the distinct
	# marker + idle indicator without disturbing resident-band rendering.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_expeditions())
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_expeditions")

	# State G — multi-band card stack (hex-icon-stack UX): 4 player bands on one hex render as an
	# up-right offset stack (top card + 2 darkened/shrunk back cards) plus a `×4` count badge, the tile
	# carries the white selection outline, and the active (selected) band is the full-brightness top
	# card — no per-token ring.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stack(4))
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_unit_id = STACK_ENTITY_BASE + 1   # not the first band → verifies active reordering
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_stack")

	# State H — mixed hex: a band (center token) sharing a hex with 1 herd + 1 food site + 3 wonders.
	# Exercises the fixed edge slots (3 visible icons) AND the `+N` overflow chip (2 spill over), on a
	# selected hex (white outline). Priority fill is wonder → food → herd.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_mixed())
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_mixed_hex")

	# State I — far-zoom level-of-detail: a large grid makes fitted hexes tiny (radius <
	# ICON_MIN_DETAIL_RADIUS), so secondary edge icons + count/overflow chips are suppressed — only
	# the primary band tokens draw. Regression guard that far zoom stays legible, not a glyph soup.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_far_zoom())
	_map.selected_unit_id = -1
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_far_zoom")

	# State J — selected hex containing a herd: the white hex outline is the SOLE selection cue;
	# the herd glyph gets NO ring (fixes the redundant/confusing circle and the split-state where a
	# migrating herd's ring diverged from the outline). selected_herd_id targets the herd on the tile.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_herd_on_tile())
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_herd_id = HERD_ON_TILE_ID
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_herd_selected")

	# State J-starving — a CORRALLED herd whose keeper could not pay this turn's feed. A penned herd
	# cannot graze, so an unfed one is SHRINKING every turn (docs/plan_corral_managed_population.md);
	# the marker flags it with a DANGER ring + a hand-drawn "!" badge. **The fed pen beside it must
	# stay clean** — that A/B is the whole point of the frame (a tint-only treatment passed the "it's
	# red-ish" eye test and failed this one: full-color emoji swallow a modulate).
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_pens())
	_map._fit_map_to_view()
	await _settle()
	await _save("map_herd_starving")

	# State K — split-state guard: the selected band (selected_unit_id) stands on a DIFFERENT hex than
	# selected_tile, simulating a band that migrated off the clicked hex on turn-advance. The outline
	# stays on selected_tile; NO active-ring may draw on the band's actual hex (group_tile !=
	# selected_tile). Confirms the ring can never diverge from the outline into a split selection.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stack(1))       # one band on (BAND_X, BAND_Y)
	_map.selected_unit_id = STACK_ENTITY_BASE       # that band
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(BAND_X - 3, BAND_Y - 2)   # a different, empty hex
	_map._fit_map_to_view()
	await _settle()
	await _save("map_ring_divergence")

	# State L — settlement-stage glyph tokens: four bands (three stages + one empty-stage fallback),
	# side by side with DIFFERENT factions, so the ⛺→🛖→🏘️ progression + distinct faction-colored
	# nameplate banners read at a glance (no selection chrome).
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stages_row())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_stage_glyphs")

	# State M — hunting expeditions (PR 2, §2b): alongside the resident band (solid dot) and a scout
	# party (hollow ⚑ flag), two hunt parties render as hollow 🏹 bow discs — one Hunting, one
	# Delivering (with a green food pip, "carrying a haul home"). Verifies hunt vs scout markers +
	# the Hunting-vs-Delivering distinction.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_hunt_expeditions())
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_hunt_expeditions")

	# State N — selected TRAVELLING band destination (non-wrapping map): the band reports
	# `is_traveling` + a `travel_target` a few hexes away → a thin cyan line from its tile to the
	# destination hex + a target reticle on that hex. Only drawn because the band is selected.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.display_snapshot(_snapshot_travel_band())
	_map.selected_unit_id = BAND_ENTITY
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_travel_band")

	# State O — WRAP-AWARE seam-crossing destination: a horizontally-wrapping map with the band near
	# the left edge and its target near the RIGHT edge. The short path crosses the seam, so the line
	# must head LEFT (toward the wrapped-nearest copy of the target), not shoot right across the map.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_travel_seam())
	_map.selected_unit_id = BAND_ENTITY
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(TRAVEL_SEAM_BAND_X, BAND_Y)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_travel_seam")

	# State P — selected TRAVELLING expedition: a detached scout party in transit draws the same
	# destination reticle + line (the draw is unit-agnostic — band OR expedition).
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_travel_expedition())
	_map.selected_unit_id = TRAVEL_EXPEDITION_ENTITY
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(5, 9)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_travel_expedition")

	# State Q — MULTI-BIOME terrain + edge-blend (Approach B: per-pixel biome-blend shader). Four vertical
	# bands of the four REAL base textures (the other 33 are noise placeholders): hot_desert_erg /
	# prairie_steppe / mixed_woodland / deep_ocean, left→right. desert+prairie are blend_class "flat"
	# (their seam should blend symmetrically); woodland is "rugged" and ocean is "water" (their seams stay
	# hard). Empty of units/herds/fog so terrain renders unobstructed. Rendered twice: blend OFF (per-hex
	# textures, the reference) then Approach B ON (the whole-map blend shader) — a pure use_edge_blending
	# toggle. The shader path bypasses the CPU cache, so no cache flag juggling is needed.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	# Force the direct (non-cached) per-hex path for the blend-OFF reference frame (deterministic).
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_biomes())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	# Blend OFF (reference): crisp textured hex silhouettes, one texture per hex, every seam hard.
	TerrainTextureManager.use_edge_blending = false
	_map.queue_redraw()
	await _settle()
	await _save("map_biome_hard")
	# Blend ON (Approach B): the shader blends the desert↔prairie (flat↔flat) seam symmetrically with
	# world-noise dither; woodland/ocean seams stay hard. Terrain must still align with the grid lines.
	TerrainTextureManager.use_edge_blending = true
	_map.queue_redraw()
	await _settle()
	await _save("map_biome_blend")
	# Coast close-up: crop the right-center region so BOTH the grass↔ocean bay coast (col 7↔8, upper)
	# and the woodland↔ocean coast (col 11↔12, lower) land in one frame — beach + foam should read.
	await _save_crop("map_biome_shore_seam", 0.44, 0.06, 0.99, 0.95)
	# Woodland-edge close-up: the forest block (cols 8–11, lower rows) borders prairie (grassy floor,
	# left) and ocean (top + right) — verifies the canopy overhang/thinning treeline (no razor cut) AND
	# the forest coast (beach/foam + canopy overhanging the water).
	await _save_crop("map_biome_woods_edge_seam", 0.30, 0.28, 0.86, 0.99)

	# State Q-far — the SAME four biome bands on a LARGE grid so _fit_map_to_view makes hexes tiny
	# (radius << EDGE_BLEND_MIN_RADIUS, so the flat↔flat blend LOD is OFF). Verifies the DECOUPLED canopy
	# LOD (canopy_min_radius): the woodland band must still read as a distinct darker-green forest mass —
	# clearly NOT the prairie grass to its left — with no shimmer/aliasing (mipmapped crown array).
	_map.display_snapshot(_snapshot_biomes_far())
	_map._fit_map_to_view()
	await _settle()
	await _save("map_biome_farzoom")

	# State R — pan/zoom SWIM regression (terrain_blend.gdshader must anchor map-space terms to the MAP,
	# not the screen). Locks onto ONE hex inside the woodland band and re-crops it after a pan-only and a
	# pan+zoom, recomputing that hex's screen center each frame. With the bug, the canopy/dither content
	# under the hex slides between frames; fixed, the fit vs pan crops are terrain-identical (same zoom)
	# and the panzoom crop shows the same hex's terrain scaled — proof the terrain tracks the grid.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false  # shader path bypasses the cache anyway
	_map.display_snapshot(_snapshot_biomes())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	# 1) Fitted: full frame + the target-hex crop (the baseline the pan/zoom crops must match).
	_map._fit_map_to_view()
	await _settle()
	await _save("map_swim_fit")
	var center_fit: Vector2 = _map._hex_center(SWIM_TARGET_COL, SWIM_TARGET_ROW, _map.last_hex_radius, _map.last_origin)
	await _save_crop_px("map_swim_hex_fit", center_fit, SWIM_CROP_RADII * _map.last_hex_radius)
	# 2) Pan only (same zoom): recompute the SAME hex's screen center (last_origin changed) → crop. This
	# MUST be terrain/canopy-identical to map_swim_hex_fit — the crispest swim detector.
	_map.pan_offset += Vector2(SWIM_PAN_COLS * _map.last_hex_radius, SWIM_PAN_ROWS * _map.last_hex_radius)
	_map.queue_redraw()
	await _settle()
	var center_pan: Vector2 = _map._hex_center(SWIM_TARGET_COL, SWIM_TARGET_ROW, _map.last_hex_radius, _map.last_origin)
	await _save_crop_px("map_swim_hex_pan", center_pan, SWIM_CROP_RADII * _map.last_hex_radius)
	# 3) Pan AND zoom: one zoom-in step on top of the pan (origin AND radius change) → recompute the same
	# hex's center → crop + full frame. Same hex → same terrain/canopy content, scaled by the zoom.
	_map.zoom_step(1)
	_map.queue_redraw()
	await _settle()
	await _save("map_swim_panzoom")
	var center_pz: Vector2 = _map._hex_center(SWIM_TARGET_COL, SWIM_TARGET_ROW, _map.last_hex_radius, _map.last_origin)
	await _save_crop_px("map_swim_hex_panzoom", center_pz, SWIM_CROP_RADII * _map.last_hex_radius)

	# State S — terrain-repetition repro (fix+terrain-repetition): a large alpine (id 26, detailed rugged
	# texture) field bordering a flat prairie band (id 11). With the continuous world-space base sampling
	# the per-hex identical-copy grid (diagonal seams) is gone — a texture spans several hexes. Fitted
	# frame + a zoomed-in crop of the alpine field to inspect the texture's own tiling period up close.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_repetition())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_repetition_after")
	await _save_crop("map_repetition_after_zoom", 0.42, 0.12, 0.98, 0.88)

	# State "swatch" — reusable single-biome AI-texture check (the biome under SWATCH_BIOME_ID, whatever
	# it's currently set to): a large field of that biome bordering a prairie (id 11) band, blend on.
	# Rendered at TWO zooms like
	# State Q: a normal-zoom frame (judge the biome's own tiling + the flat↔flat blend against prairie)
	# and a far-zoom frame on the large grid (judge whole-region cohesion / read as a distinct biome).
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_swatch(GRID_W, GRID_H, SWATCH_PRAIRIE_COLS))
	_map._fit_map_to_view()
	await _settle()
	await _save("map_swatch")
	_map.display_snapshot(_snapshot_swatch(FAR_GRID_W, FAR_GRID_H, SWATCH_FAR_PRAIRIE_COLS))
	_map._fit_map_to_view()
	await _settle()
	await _save("map_swatch_farzoom")

	# State "cohesion" — the FIVE accepted AI biomes side by side (desert · scrub · prairie · woodland ·
	# tundra), blend on, to judge the SET as a cohesive whole: art-family consistency, per-biome
	# distinctiveness, and the flat↔flat blends at every adjacent seam. Rendered at two zooms like State Q.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_cohesion(COHESION_GRID_W, COHESION_GRID_H))
	_map._fit_map_to_view()
	await _settle()
	await _save("map_cohesion")
	_map.display_snapshot(_snapshot_cohesion(COHESION_FAR_GRID_W, COHESION_FAR_GRID_H))
	_map._fit_map_to_view()
	await _settle()
	await _save("map_cohesion_farzoom")

	get_tree().quit()

func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("map_preview: null image (dummy renderer?) — run without --headless")
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("map_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("map_preview: saved ", name, ".png")

## Save a cropped region of the current frame (fractions of the viewport, 0..1) — used for coast close-ups.
func _save_crop(name: String, fx0: float, fy0: float, fx1: float, fy1: float) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("map_preview: null image (dummy renderer?) — run without --headless")
		return
	var w := image.get_width()
	var h := image.get_height()
	var rect := Rect2i(int(fx0 * w), int(fy0 * h), int((fx1 - fx0) * w), int((fy1 - fy0) * h))
	var crop := image.get_region(rect)
	var err := crop.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("map_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("map_preview: saved ", name, ".png")

## Save a square crop of `2*half` px centered on `center` (viewport pixels), clamped to the image
## bounds — used by State R to lock onto the SAME hex across fit/pan/zoom so a swim shows as a shift.
func _save_crop_px(name: String, center: Vector2, half: float) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("map_preview: null image (dummy renderer?) — run without --headless")
		return
	var w := image.get_width()
	var h := image.get_height()
	var x0 := clampi(int(center.x - half), 0, w - 1)
	var y0 := clampi(int(center.y - half), 0, h - 1)
	var x1 := clampi(int(center.x + half), 0, w)
	var y1 := clampi(int(center.y + half), 0, h)
	var rect := Rect2i(x0, y0, maxi(x1 - x0, 1), maxi(y1 - y0, 1))
	var crop := image.get_region(rect)
	var err := crop.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("map_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("map_preview: saved ", name, ".png")

func _terrain_array() -> Array:
	var arr: Array = []
	arr.resize(GRID_W * GRID_H)
	arr.fill(TERRAIN_ID)
	return arr

func _base_snapshot(band: Dictionary, herds: Array) -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [band],
		"herds": herds,
	}

## Merge a stage's presentation tokens into a band dict (in place) and return it.
func _with_stage(band: Dictionary, stage: Dictionary) -> Dictionary:
	band["settlement_stage_id"] = String(stage.get("id", ""))
	band["settlement_stage_label"] = String(stage.get("label", ""))
	band["settlement_stage_icon"] = String(stage.get("icon", ""))
	return band

func _band(assignments: Array, work_range: int, scout_radius: int) -> Dictionary:
	return _with_stage({
		"entity": BAND_ENTITY,
		"faction": 0,
		"current_x": BAND_X,
		"current_y": BAND_Y,
		"size": 30,
		"id": "Band 1",
		"work_range": work_range,
		"scout_reveal_radius": scout_radius,
		"labor_assignments": assignments,
	}, STAGE_NOMADIC)

func _deer_herd() -> Dictionary:
	# Well outside the work-range ring (Chebyshev distance 5 from the band).
	return {"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "x": 13, "y": 6, "biomass": 800.0, "huntable": true}

## Two pens side by side: one FED, one STARVING. `corralled` + `pen_fed_fraction` < 1 is the sim's
## starving signal — the herd is losing biomass every turn, and the map must show WHICH pen.
func _snapshot_pens() -> Dictionary:
	var fed := _deer_herd()
	fed["corralled"] = true
	fed["pen_fed_fraction"] = 1.0
	var starving := {
		"id": "game_aurochs_03", "label": "Aurochs (game_aurochs_03)",
		"x": 10, "y": 7, "biomass": 310.0, "huntable": true,
		"corralled": true, "pen_fed_fraction": 0.4,
	}
	return _base_snapshot(_band([], 2, 2), [fed, starving])

func _snapshot_work() -> Dictionary:
	# Per-source yields annotate the worked tiles/herd on the map. Forage is renewable (actual ==
	# sustainable, no ⚠); the hunt OVERDRAWS (0.46 > 0.20) so its herd label shows the amber ⚠ flag.
	var assignments := [
		# Policies drive the yield label's trailing policy glyph (♻ sustain / ⬆ surplus / 🪙 market /
		# 💀 eradicate) — two different ones here so the map read is verifiable in one frame.
		{"kind": "forage", "workers": 5, "target_x": FORAGE_A_X, "target_y": FORAGE_A_Y, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48},
		{"kind": "forage", "workers": 3, "target_x": 9, "target_y": 8, "policy": "market", "actual_yield": 0.27, "sustainable_yield": 0.20},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 13, "target_y": 6, "actual_yield": 0.46, "sustainable_yield": 0.20},
		{"kind": "warrior", "workers": 2},
	]
	return _base_snapshot(_band(assignments, 2, 2), [_deer_herd()])

## State A-overlap fixture: the worked band, plus a herd standing ON the first worked forage tile so
## its secondary glyph is drawn over that tile's yield label (the reported failure).
func _snapshot_work_overlap() -> Dictionary:
	var snap := _snapshot_work()
	var herds: Array = snap["herds"]
	herds.append({
		"id": OVERLAP_HERD_ID,
		"label": "Wild Boar (%s)" % OVERLAP_HERD_ID,
		"x": FORAGE_A_X, "y": FORAGE_A_Y,
		"biomass": 400.0, "huntable": true,
	})
	return snap

func _snapshot_scout() -> Dictionary:
	var assignments := [
		{"kind": "scout", "workers": 5},
		{"kind": "forage", "workers": 3, "target_x": 7, "target_y": 6},
	]
	return _base_snapshot(_band(assignments, 2, 2), [_deer_herd()])

func _sites_state() -> Array:
	return [{
		"faction": 0,
		"sites": [
			{"x": 6, "y": 5, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 10, "y": 7, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
			# On the deer-herd tile → exercises the overlap nudge (marker offset up).
			{"x": 13, "y": 6, "site_id": "sky_arch", "category": "landmark", "display_name": "Sky Arch", "glyph": "⛰"},
		],
	}]

func _snapshot_sites() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 2), [_deer_herd()])
	snap["discovered_sites"] = _sites_state()
	return snap

## A detached scouting party (docs/plan_exploration_and_sites.md §2): a cohort tagged Expedition
## flowing through the same populations[] array as a band. `awaiting` drives the pulsing idle ring.
func _expedition(entity: int, x: int, y: int, phase: String) -> Dictionary:
	return {
		"entity": entity,
		"faction": 0,
		"current_x": x,
		"current_y": y,
		"size": 6,
		"id": "Scouts",
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": phase,
		"is_traveling": phase != "awaiting",
	}

func _snapshot_expeditions() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 2), [])
	# Two expeditions alongside the resident band: one outbound, one awaiting (pulsing ring).
	snap["populations"].append(_expedition(9101, 11, 3, "outbound"))
	snap["populations"].append(_expedition(9102, 5, 9, "awaiting"))
	return snap

func _band_at(entity: int, x: int, y: int, stage: Dictionary = STAGE_NOMADIC, faction: int = 0) -> Dictionary:
	return _with_stage({
		"entity": entity,
		"faction": faction,
		"current_x": x,
		"current_y": y,
		"size": 30,
		"id": "Band %d" % entity,
		"work_range": 2,
		"scout_reveal_radius": 0,
		"labor_assignments": [],
	}, stage)

## N player bands co-located on (BAND_X, BAND_Y) → exercises the offset card stack + `×N` badge
## folded onto the banner's right end. Fans DIFFERENT stage glyphs (and DIFFERENT factions) across
## the cards so the active (top) card's faction banner shows a distinct color; only the active card
## draws a banner, so the back cards are bare dimmed glyphs behind it.
func _snapshot_stack(n: int) -> Dictionary:
	var bands: Array = []
	for i in range(n):
		var stage: Dictionary = STACK_STAGE_CYCLE[i % STACK_STAGE_CYCLE.size()]
		bands.append(_band_at(STACK_ENTITY_BASE + i, BAND_X, BAND_Y, stage, i % 3))
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": bands,
		"herds": [],
	}

## One band sharing (BAND_X, BAND_Y) with 1 herd + 1 food site + 3 wonders → 3 edge slots + `+2` chip.
func _snapshot_mixed() -> Dictionary:
	var snap := {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [_band_at(BAND_ENTITY, BAND_X, BAND_Y, STAGE_VILLAGE)],
		"herds": [{"id": "game_boar_03", "label": "Wild Boar (game_boar_03)", "x": BAND_X, "y": BAND_Y, "biomass": 400.0, "huntable": true}],
		"food_modules": [{"x": BAND_X, "y": BAND_Y, "module": "berry_patch", "kind": "forage"}],
		"discovered_sites": [{
			"faction": 0,
			"sites": [
				{"x": BAND_X, "y": BAND_Y, "site_id": "peak_a", "category": "landmark", "display_name": "Peak A", "glyph": "⛰"},
				{"x": BAND_X, "y": BAND_Y, "site_id": "spring_b", "category": "settle_site", "display_name": "Spring B", "glyph": "⛲"},
				{"x": BAND_X, "y": BAND_Y, "site_id": "grove_c", "category": "landmark", "display_name": "Grove C", "glyph": "🌋"},
			],
		}],
	}
	return snap

## Four separate bands on adjacent hexes → the ⛺ / 🛖 / 🏘️ glyph tokens side by side for a direct
## progression read, each over its faction-colored nameplate banner. Bands are assigned DIFFERENT
## factions (blue / orange / green / orange) so distinct banner colors read at a glance. The fourth
## band is empty-stage → exercises the neutral non-circular fallback marker (with a banner, no disc).
func _snapshot_stages_row() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [
			_band_at(STACK_ENTITY_BASE, BAND_X - 3, BAND_Y, STAGE_NOMADIC, 0),
			_band_at(STACK_ENTITY_BASE + 1, BAND_X - 1, BAND_Y, STAGE_CAMP, 1),
			_band_at(STACK_ENTITY_BASE + 2, BAND_X + 1, BAND_Y, STAGE_VILLAGE, 2),
			_band_at(STACK_ENTITY_BASE + 3, BAND_X + 3, BAND_Y, STAGE_NONE, 1),
		],
		"herds": [],
	}

## A single herd alone on the band's hex → selecting that hex must show only the outline, no ring.
func _snapshot_herd_on_tile() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [],
		"herds": [{"id": HERD_ON_TILE_ID, "label": "Wild Boar (game_boar_03)", "x": BAND_X, "y": BAND_Y, "biomass": 400.0, "huntable": true}],
	}

## Large grid so fitted hexes are tiny (< ICON_MIN_DETAIL_RADIUS): bands + secondaries present, but
## only the primary tokens should draw (secondary icons + chips suppressed by LOD).
## The worked band (forage tile + overdrawing hunt, both carrying yields) on the FAR grid, so a fit
## makes hexes tiny — exercises the yield-label LOD suppression at far zoom.
func _snapshot_far_work() -> Dictionary:
	var terrain: Array = []
	terrain.resize(YIELD_FAR_GRID_W * YIELD_FAR_GRID_H)
	terrain.fill(TERRAIN_ID)
	var cx := YIELD_FAR_GRID_W / 2
	var cy := YIELD_FAR_GRID_H / 2
	var assignments := [
		{"kind": "forage", "workers": 5, "target_x": cx + 1, "target_y": cy, "actual_yield": 0.48, "sustainable_yield": 0.48},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": cx + 2, "target_y": cy, "actual_yield": 0.46, "sustainable_yield": 0.20},
	]
	var band := _with_stage({
		"entity": BAND_ENTITY, "faction": 0, "current_x": cx, "current_y": cy, "size": 30,
		"id": "Band 1", "work_range": 2, "scout_reveal_radius": 2, "labor_assignments": assignments,
	}, STAGE_NOMADIC)
	return {
		"grid": {"width": YIELD_FAR_GRID_W, "height": YIELD_FAR_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [band],
		"herds": [{"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "x": cx + 2, "y": cy, "biomass": 800.0, "huntable": true}],
	}

func _snapshot_far_zoom() -> Dictionary:
	var terrain: Array = []
	terrain.resize(FAR_GRID_W * FAR_GRID_H)
	terrain.fill(TERRAIN_ID)
	var cx := FAR_GRID_W / 2
	var cy := FAR_GRID_H / 2
	return {
		"grid": {"width": FAR_GRID_W, "height": FAR_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [
			_band_at(BAND_ENTITY, cx, cy),
			_band_at(STACK_ENTITY_BASE, cx + 3, cy + 2),
		],
		"herds": [{"id": "game_deer_09", "label": "Red Deer (game_deer_09)", "x": cx + 1, "y": cy, "biomass": 600.0, "huntable": true}],
		"food_modules": [{"x": cx - 1, "y": cy + 1, "module": "berry_patch", "kind": "forage"}],
	}

## A detached hunting party (PR 2, §2b): mission "hunt" → the bow-disc marker; "delivering" phase
## adds the green food pip. Shares the expedition marker path with the scout party.
func _hunt_expedition(entity: int, x: int, y: int, phase: String) -> Dictionary:
	var party := _expedition(entity, x, y, phase)
	party["expedition_mission"] = "hunt"
	party["expedition_target_herd"] = "game_deer_07"
	return party

func _snapshot_hunt_expeditions() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 2), [_deer_herd()])
	# A scout party (flag) + three hunt parties (bow): Hunting (red gathering cue), Delivering and
	# Returning (both hauling home → green food pip).
	snap["populations"].append(_expedition(9201, 11, 3, "outbound"))
	snap["populations"].append(_hunt_expedition(9202, 5, 9, "hunting"))
	snap["populations"].append(_hunt_expedition(9203, 10, 8, "delivering"))
	snap["populations"].append(_hunt_expedition(9204, 3, 4, "returning"))
	return snap

## A selected band in transit: carries `is_traveling` + a `travel_target` a few hexes SE of its
## tile, so the destination reticle + line draw on a non-wrapping map.
func _snapshot_travel_band() -> Dictionary:
	var band := _band([{"kind": "warrior", "workers": 2}], 2, 0)
	band["is_traveling"] = true
	band["travel_target_x"] = 13
	band["travel_target_y"] = 6
	return _base_snapshot(band, [])

## Seam-crossing destination on a horizontally-wrapping map: band near the LEFT edge, target near the
## RIGHT edge. The short wrapped path runs left across the seam, so the line must head left.
func _snapshot_travel_seam() -> Dictionary:
	var band := _band_at(BAND_ENTITY, TRAVEL_SEAM_BAND_X, BAND_Y)
	band["is_traveling"] = true
	band["travel_target_x"] = TRAVEL_SEAM_TARGET_X
	band["travel_target_y"] = BAND_Y
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": true},
		"overlays": {"terrain": _terrain_array()},
		"populations": [band],
		"herds": [],
	}

## A selected scouting expedition in transit → the same destination reticle + line (unit-agnostic).
func _snapshot_travel_expedition() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 0), [])
	var party := _expedition(TRAVEL_EXPEDITION_ENTITY, 5, 9, "outbound")
	party["travel_target_x"] = 11
	party["travel_target_y"] = 3
	snap["populations"].append(party)
	return snap

## Four vertical biome bands across the 16×12 grid (see BIOME_BAND_IDS): cols 0–3 desert, 4–7
## prairie, 8–11 woodland, 12–15 ocean — plus an ocean bay carved into the upper cols 8+ so the ocean
## also borders the prairie band (see BIOME_BAY_*). Straight band edges — the point is the coast/seam look.
func _biome_band_terrain() -> Array:
	var arr: Array = []
	arr.resize(GRID_W * GRID_H)
	for y in range(GRID_H):
		for x in range(GRID_W):
			var band: int = mini(x / BIOME_BAND_COLS, BIOME_BAND_IDS.size() - 1)
			var tid: int = BIOME_BAND_IDS[band]
			if y < BIOME_BAY_ROWS and x >= BIOME_BAY_COL_MIN:
				tid = BIOME_OCEAN_ID  # bay → prairie↔ocean (grassy) coast in the upper rows
			arr[y * GRID_W + x] = tid
	return arr

## The same four biome bands as _biome_band_terrain, but on the LARGE far-zoom grid (no bay — the point
## is forest-vs-prairie readability at far zoom, not the coast). Bands split FAR_GRID_W evenly.
func _snapshot_biomes_far() -> Dictionary:
	var band_cols: int = FAR_GRID_W / BIOME_BAND_IDS.size()
	var arr: Array = []
	arr.resize(FAR_GRID_W * FAR_GRID_H)
	for y in range(FAR_GRID_H):
		for x in range(FAR_GRID_W):
			var band: int = mini(x / band_cols, BIOME_BAND_IDS.size() - 1)
			arr[y * FAR_GRID_W + x] = BIOME_BAND_IDS[band]
	return {
		"grid": {"width": FAR_GRID_W, "height": FAR_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": arr},
		"populations": [],
		"herds": [],
	}

## Terrain-only snapshot for the multi-biome baseline: no bands/herds/sites, fog off.
func _snapshot_biomes() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _biome_band_terrain()},
		"populations": [],
		"herds": [],
	}

## Terrain-only repetition repro: left REPEAT_PRAIRIE_COLS columns prairie (flat), the rest alpine (rugged,
## detailed) — a large alpine field to expose (and, post-fix, confirm the absence of) the per-hex grid.
func _snapshot_repetition() -> Dictionary:
	var arr: Array = []
	arr.resize(GRID_W * GRID_H)
	for y in range(GRID_H):
		for x in range(GRID_W):
			arr[y * GRID_W + x] = REPEAT_PRAIRIE_ID if x < REPEAT_PRAIRIE_COLS else REPEAT_ALPINE_ID
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": arr},
		"populations": [],
		"herds": [],
	}

## Terrain-only single-biome swatch: left `prairie_cols` columns prairie (SWATCH_PRAIRIE_ID, flat), the
## rest a large field of SWATCH_BIOME_ID — a reusable one-biome AI-texture check (own tiling + the
## flat↔flat blend + cohesion against the accepted prairie). Sized to the passed grid so the same builder
## serves both the normal and far-zoom frames.
func _snapshot_swatch(grid_w: int, grid_h: int, prairie_cols: int) -> Dictionary:
	var arr: Array = []
	arr.resize(grid_w * grid_h)
	for y in range(grid_h):
		for x in range(grid_w):
			arr[y * grid_w + x] = SWATCH_PRAIRIE_ID if x < prairie_cols else SWATCH_BIOME_ID
	return {
		"grid": {"width": grid_w, "height": grid_h, "wrap_horizontal": false},
		"overlays": {"terrain": arr},
		"populations": [],
		"herds": [],
	}

## Terrain-only cohesion field: the five accepted biomes (COHESION_BIOME_IDS) as equal vertical bands
## across the passed grid, left→right. All `flat`, so every adjacent seam flat↔flat dither-blends. Sized
## to the passed grid so the same builder serves both the normal and far-zoom frames.
func _snapshot_cohesion(grid_w: int, grid_h: int) -> Dictionary:
	var band_cols: int = grid_w / COHESION_BIOME_IDS.size()
	var arr: Array = []
	arr.resize(grid_w * grid_h)
	for y in range(grid_h):
		for x in range(grid_w):
			var band: int = mini(x / band_cols, COHESION_BIOME_IDS.size() - 1)
			arr[y * grid_w + x] = COHESION_BIOME_IDS[band]
	return {
		"grid": {"width": grid_w, "height": grid_h, "wrap_horizontal": false},
		"overlays": {"terrain": arr},
		"populations": [],
		"herds": [],
	}

func _snapshot_sites_fogged() -> Dictionary:
	var snap := _snapshot_sites()
	# Visibility raster (raw encoding: 0.0 unexplored / 0.5 discovered / 1.0 active). All tiles
	# Discovered except the band's own hex Active, so the site markers sit on remembered tiles.
	var vis := PackedFloat32Array()
	vis.resize(GRID_W * GRID_H)
	vis.fill(0.5)
	vis[BAND_Y * GRID_W + BAND_X] = 1.0
	snap["overlays"] = {
		"terrain": _terrain_array(),
		"channels": {"visibility": {"raw": vis, "normalized": vis, "label": "Visibility"}},
	}
	return snap
