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
# State "rivers" — Minor/Major rivers on hex EDGES + a NavigableRiver hex chain to the coast.
# The edge chain is generated as the BOUNDARY of a region (all hexes north of a staircase row f(x)):
# a region boundary is contiguous by construction, so the chain never breaks and every step of the
# staircase produces a real CORNER TURN — exactly what the corner joins need to be read against.
const RIVER_LAND_ID := 11        # prairie_steppe (flat) — a legible bank
const RIVER_OCEAN_ID := 0        # deep_ocean (water) — the sea the navigable river drains into
const RIVER_NAVIGABLE_ID := 37   # NavigableRiver — a water TERRAIN, rendered as a BANK with a channel through it
const RIVER_DELTA_ID := 6        # RiverDelta — the sim makes the chain's MOUTH a delta (a LAND tile), so the
                                 # channel must arm toward it or it dead-ends one hex short of the sea
const RIVER_LAKE_ID := 2         # inland_sea — the CONTROL: an actual lake in the same frame. The navigable
                                 # hexes used to render EXACTLY like this (a hex-shaped puddle ringed with
                                 # beach + surf); the two must now read as obviously different things.
const RIVER_OCEAN_COLS := 2      # rightmost columns of open sea
const RIVER_NAV_HEXES := 4       # nominal length of the NavigableRiver chain (where the edge chain hands off)
# The navigable chain WALKS these directions (sim odd-r order) from the last edge-river hex out to the sea,
# so the trunk turns corners instead of running dead straight — the arm/junction geometry is the thing the
# navigable pass has to get right, and a straight run would never exercise it. Cycled if more steps are
# needed than the pattern has.
const RIVER_NAV_STEPS := [0, 1, 0, 5, 0]   # E, SE, E, NE, E
const RIVER_NAV_MAX_STEPS := 16  # guard: SE/NE don't always advance the column on an odd-r grid
const RIVER_DIR_E := 0
const RIVER_DIR_SE := 1          # the side the mouth's delta lobe sits on
# The lake, as fractions of the grid (so it lands sensibly on the far-zoom grid too) + the hexes it spans.
# West of the trunk and a couple of rows SOUTH of the edge river's bank (so no lake hex ever carries river
# edges, which would draw an edge band across the lake and muddy the comparison).
const RIVER_LAKE_COL_FRAC := 0.13
const RIVER_LAKE_ROW_FRAC := 0.58
const RIVER_LAKE_HEXES := [[0, 0], [1, 0], [0, 1]]
# Crop framing for the river close-ups, as fractions of the frame (x, y, w, h) — the harness's own idiom.
# (Hex-anchored pixel crops were tried and abandoned: the viewport rect this harness reports does not match
# the captured framebuffer's geometry, so a hex-derived pixel rect lands somewhere else entirely.)
const RIVER_SEAM_CROP := Rect2(0.22, 0.10, 0.28, 0.50)    # mid edge-chain: the Minor→Major class change + turns
const RIVER_NAV_CROP := Rect2(0.52, 0.36, 0.36, 0.64)     # the trunk: several hexes, so hex-to-hex CONTINUITY shows
const RIVER_MOUTH_CROP := Rect2(0.76, 0.42, 0.20, 0.45)   # the mouth: channel → open sea + the delta lobe
# The HEAD of the trunk, close: the hex where the edge rivers hand over. It flanks three river edges (two
# Major + the Minor tributary) and used to fill with water; the two INFLOW SPURS must now arrive at their
# own class widths, each at a hex VERTEX, and merge into the channel with no notch. Judged ZOOMED IN and
# hex-anchored (State R's idiom), not as a fraction of the fitted frame: at fit, a hex is ~60 px across and
# a Minor spur a couple of px — far too coarse to see whether the join has a notch.
# Kept modest: the crop is a REGION of the framebuffer, so a crop taller than the (short, wide) window gets
# clamped and the head slides off-centre. 3 steps puts the head hex at a few hundred px and still fits.
const RIVER_JOIN_ZOOM_STEPS := 3   # zoom-in steps before the close-up
const RIVER_JOIN_CROP_RADII := 1.6 # crop half-size = this × hex_radius → the head hex plus its joins
# The map is COVER-fit and the fit is the zoom FLOOR (MapView.MIN_ZOOM_FACTOR = 1.0 — you cannot zoom out
# past it), so on a window wider than the grid's aspect the lowest rows are simply off-screen. The river,
# its trunk and the lake all live in those lower rows, so the state PANS up (the swim state's idiom) to
# bring them into frame instead. Measured in hex radii; the row pitch is 1.5 radii.
const RIVER_PAN_ROWS := -4.5
const RIVER_CLASS_MINOR := 1     # 2-bit edge classes (must match core_sim RiverClass)
const RIVER_CLASS_MAJOR := 2
const RIVER_CLASS_BITS := 2      # bits per slot in BOTH masks (river_edges by side, river_inflow by corner)
const RIVER_CLASS_MASK := 0b11
const RIVER_CORNERS := 6
# river_channel is the THIRD mask and is shaped differently: ONE bit per odd-r direction (exits(dir) =
# (mask >> dir) & 1), naming the sides a navigable hex's channel flows out through. It is what the shader
# arms the trunk from — see the "web" state below for why the renderer may not infer that from terrain.
const RIVER_CHANNEL_EXIT_BIT := 1
# Corner `i` is the vertex at angle 60*i + 30 with +y DOWN (MapView._hex_points order, and the wire
# contract river_inflow is packed against): 0 lower-right, 1 bottom, 2 lower-left, 3 upper-left, 4 top,
# 5 upper-right. Side `dir` spans corners {dir - 1, dir}.
const RIVER_CORNER_ANGLE_STEP_DEG := 60.0
const RIVER_CORNER_ANGLE_OFFSET_DEG := 30.0
# The MINOR tributary (below) hands over at the trunk head's BOTTOM vertex — the far end of the head's SW
# side, which is the last edge of that chain.
const RIVER_TRIB_TERMINUS_CORNER := 1
# The Minor tributary's 3 edges, walked OUT from the trunk head as (hex-from-head via these steps, side):
# (head, SW), (head's W neighbour, SE), (head's SW neighbour, W). Each consecutive pair shares a corner
# (three hexes meet at every corner), so the chain is contiguous by construction on either row parity.
const RIVER_DIR_SW := 2
const RIVER_DIR_W := 3
# A SECOND navigable head, fed by a MINOR tributary ONLY — the case the head TAPER exists for, and the one
# that read worst without it: a hairline Minor hands over at a vertex and the trunk sprang to a full great
# river a few px later. A short navigable BRANCH joins the main trunk from the NW; its head hex carries one
# Minor inflow corner, so its arm must START hairline and SWELL to the channel's full width by the shared
# edge with the trunk. Its 3 tributary edges are the vertical MIRROR of the main head's Minor tributary
# above (SW↔NW, SE↔NE, W↔W, bottom vertex ↔ top vertex), so the same contiguity argument holds.
const RIVER_DIR_NW := 4
const RIVER_DIR_NE := 5
const RIVER_BRANCH_TERMINUS_CORNER := 4   # top vertex — the mirror of RIVER_TRIB_TERMINUS_CORNER
# A MID-CHAIN tributary junction — the case a real drainage network produces and the old fixtures never
# did (they only ever fed a chain's HEAD). Since the network landed, river_inflow means "a tributary hands
# over at this vertex", which is true of ANY navigable hex; the shader must therefore NOT read a nonzero
# inflow as "this is a chain head" and taper the trunk there, or the full-width channel pinches to the
# tributary's width at that hex's centre and swells back on both sides — an HOURGLASS in mid-channel. This
# hangs the SAME 3-edge Minor tributary as the trunk head's onto a hex in the MIDDLE of the trunk (>= 2
# channel exits), so map_rivers_midchain.png is the frame that gate is judged on: constant width THROUGH the
# junction, and the spur still reaching the vertex.
const RIVER_MAJOR_FROM_FRAC := 0.45  # fraction along the edge chain where Minor becomes Major
# State "rivers web" — THE REGRESSION GUARD for the spider-web bug. The main rivers state builds its
# navigable chain by hand, so it is a PATH by construction and can never cross-link — which is exactly why
# the preview never caught the bug. Here the navigable hexes form a solid 2-D CLUMP (adjacent rows of
# adjacent hexes, the shape a real map produced), and the channel winds through it as a single boustrophedon
# SNAKE: every hex is on the path, but so is every hex ADJACENT to two or three other path hexes it does not
# connect to. The renderer's old rule — arm every navigable/water/delta neighbour — turned that into a mesh
# of triangles. Honouring river_channel, only the snake may draw. Any triangle in map_rivers_web.png is the
# bug, back.
const RIVER_WEB_COLS := 5        # clump width in hexes (the snake's run length per row)
const RIVER_WEB_ROWS := 4        # clump height — enough rows for the snake to double back on itself twice
const RIVER_WEB_ROW_FRAC := 0.30 # the clump's top row, as a fraction of grid height (upper half — see RIVER_PAN_ROWS)
const RIVER_WEB_CROP := Rect2(0.55, 0.10, 0.45, 0.80)  # the clump, filling the frame
# The river's mean row, as a fraction of grid height. Kept in the UPPER half deliberately: the map is
# COVER-fit and the fit is the zoom floor, so on a window wider than the grid's aspect the lower rows are
# unreachable (MapView's pan clamp will not scroll that far) — a river down there simply cannot be looked at.
const RIVER_BASE_ROW_FRAC := 0.25
# The bank row, as offsets (in rows) from the base row, sampled along the river's length.
# It is a mostly-MONOTONE downhill drift with one back-up, NOT an up-down-up staircase: a boundary that
# reverses every step wraps 4+ sides of the same hexagon and manufactures a honeycomb the render is then
# blamed for — real hydrology (a downhill walk on the corner lattice) never circles a hex. It still turns
# a corner at every step and double-steps twice, so the rounded joins are exercised just as hard.
const RIVER_PATTERN := [0, 0, 1, 1, 2, 2, 3, 2, 3, 3]
const RIVER_WANDER_BASE_H := 12      # the grid height RIVER_PATTERN's row offsets are authored for;
                                     # a taller grid scales them up so the far-zoom river still wanders
# odd-r neighbour offsets in the SIM's direction order (core_sim grid_utils HEX_NEIGHBOR_OFFSETS,
# clockwise from E) — the order the river-edge bitmask is indexed by. (dx_even, dx_odd, dy).
const RIVER_DIR_OFFSETS := [
	[1, 1, 0],    # 0 E
	[0, 1, 1],    # 1 SE
	[-1, 0, 1],   # 2 SW
	[-1, -1, 0],  # 3 W
	[-1, 0, -1],  # 4 NW
	[0, 1, -1],   # 5 NE
]

var _map: Node2D
# Where _snapshot_rivers put the MINOR-only navigable head (see RIVER_BRANCH_TERMINUS_CORNER). Reported
# back rather than recomputed, because the placement walks the trunk and has to dodge it; (-1, -1) if the
# grid left no room for one (the far-zoom grid is built after the close-ups, so it may overwrite this).
var _river_branch_head := Vector2i(-1, -1)
# Where _snapshot_rivers put the MID-CHAIN tributary junction (see RIVER_MIDCHAIN_MIN_COL_MARGIN). Reported
# back for the same reason as the branch head; (-1, -1) if the grid left no room for one.
var _river_midchain_junction := Vector2i(-1, -1)

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

	# State "rivers" — Minor/Major rivers on hex EDGES (terrain_blend.gdshader's river pass, fed by the
	# per-tile 12-bit river_edges mask) plus a NavigableRiver hex chain (terrain 37) that turns corners,
	# is fed by the Major edge river, and drains to the sea through a delta lobe — with a real InlandSea
	# LAKE in the same frame as the control.
	# Read: the edge water must hug the hex EDGE (never the center) and visibly MEANDER (no honeycomb);
	# corner joins rounded with no gap/kink; the two half-bands meet symmetrically across an edge (no seam
	# down the middle); Minor visibly thinner than Major. And the NAVIGABLE hexes must read as a wide water
	# CHANNEL running through a silty BANK — never a hex-shaped puddle: no beach, no foam anywhere on them,
	# the channel CONTINUOUS across adjacent navigable hexes (no seam/pinch/gap at their shared edge), the
	# Major edge river visibly flowing INTO the trunk, and the trunk reaching the sea. The lake, which still
	# gets its beach + foam, must be obviously a different kind of thing.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_rivers(GRID_W, GRID_H))
	_map._fit_map_to_view()
	await _settle()  # last_hex_radius is only refreshed on draw — settle before panning by it
	# Pan up so the trunk + the lake (lower rows, clipped by the cover-fit on a wide window) are in frame.
	_map.pan_offset += Vector2(0.0, RIVER_PAN_ROWS * _map.last_hex_radius)
	await _settle()
	await _save("map_rivers")
	# Seam + corner close-up: the mid-chain region, where the staircase steps (corner turns) and the
	# Minor→Major transition both land — the frame to judge joins and the cross-edge seam on.
	await _save_crop_rect("map_rivers_seam", RIVER_SEAM_CROP)
	# The navigable trunk close-up: the edge-river → trunk join, the corner turns, and the hex-to-hex
	# CONTINUITY of the channel. This is the frame the "a channel through a bank, not a puddle" and "no seam
	# between adjacent navigable hexes" claims are judged on.
	await _save_crop_rect("map_rivers_navigable", RIVER_NAV_CROP)
	# The MOUTH: the channel must reach the sea and the delta lobe — no dead-end, and crucially NO surf line
	# drawn ACROSS the mouth (a river meeting the sea is not a coast; the shore pass skips navigable edges).
	await _save_crop_rect("map_rivers_mouth", RIVER_MOUTH_CROP)
	# The HAND-OVER, zoomed: the trunk HEAD is the hex where the edge rivers hand over. It flanks THREE
	# river edges (two Major + the Minor tributary) — the shape that used to fill the hex with water — and
	# is fed by TWO inflow spurs on different corners. It must read as a channel with two tributaries
	# entering at VERTICES, each at its own class width, merging with no notch. Zoom in and re-center on the
	# head (State R's hex-anchored crop), because a fitted hex is far too few pixels to judge that on.
	var nav_start: int = GRID_W - RIVER_OCEAN_COLS - RIVER_NAV_HEXES
	var head := Vector2i(nav_start - 1, _river_bank_row(nav_start - 1, GRID_W, GRID_H, nav_start))
	for _i in range(RIVER_JOIN_ZOOM_STEPS):
		_map.zoom_step(1)
	await _settle()
	# Re-center: the zoom is about the viewport center, so the head drifts off-frame without this. Recompute
	# its screen center AFTER the pan settles — MapView clamps pan_offset, so the request is not the result.
	_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
		- _map._hex_center(head.x, head.y, _map.last_hex_radius, _map.last_origin)
	_map.queue_redraw()
	await _settle()
	var head_center: Vector2 = _map._hex_center(head.x, head.y, _map.last_hex_radius, _map.last_origin)
	await _save_crop_px("map_rivers_join", head_center, RIVER_JOIN_CROP_RADII * _map.last_hex_radius)
	# The MINOR-ONLY head, same zoom: the trunk there is fed by ONE Minor tributary, so the HEAD TAPER must
	# start its arm at the Minor's hairline half-width at the hex centre and swell it to the full channel
	# width by the time it reaches the shared edge with the trunk — where the next (mid-chain, constant
	# full-width) navigable hex takes over. Read for: a visible SWELL across the head hex, no jump-cut at
	# the centre, and above all NO step or notch at that downstream edge. (The Major+Minor head above is the
	# other half of the test: it must start at the MAJOR — the widest inflow — width.)
	if _river_branch_head.x >= 0:
		_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
			- _map._hex_center(_river_branch_head.x, _river_branch_head.y, _map.last_hex_radius, _map.last_origin)
		_map.queue_redraw()
		await _settle()
		var branch_center: Vector2 = _map._hex_center(
			_river_branch_head.x, _river_branch_head.y, _map.last_hex_radius, _map.last_origin)
		await _save_crop_px("map_rivers_head_minor", branch_center, RIVER_JOIN_CROP_RADII * _map.last_hex_radius)
	else:
		push_warning("map_preview: no Minor-only navigable head placed — head-taper frame skipped")
	# The MID-CHAIN JUNCTION, same zoom: a Minor tributary hands over at a vertex of a hex in the MIDDLE of
	# the trunk (upstream AND downstream channel exits). Since the drainage network, river_inflow means "a
	# tributary hands over here", not "this is a chain head" — so the shader gates the head taper on the
	# channel-EXIT COUNT instead. Read for: the trunk holding CONSTANT full width straight through the
	# junction (any pinch-and-swell at the hex centre is the HOURGLASS this gate exists to prevent), and the
	# Minor spur still reaching its vertex to meet the tributary — no gap, no dead-end.
	if _river_midchain_junction.x >= 0:
		_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
			- _map._hex_center(_river_midchain_junction.x, _river_midchain_junction.y,
				_map.last_hex_radius, _map.last_origin)
		_map.queue_redraw()
		await _settle()
		var mid_center: Vector2 = _map._hex_center(
			_river_midchain_junction.x, _river_midchain_junction.y, _map.last_hex_radius, _map.last_origin)
		await _save_crop_px("map_rivers_midchain", mid_center, RIVER_JOIN_CROP_RADII * _map.last_hex_radius)
	else:
		push_warning("map_preview: no mid-chain tributary junction placed — hourglass frame skipped")
	# Far-zoom LOD: the same field on a large grid so hexes go tiny (radius ≪ EDGE_BLEND_MIN_RADIUS, so
	# the flat↔flat blend is off). The DECOUPLED river LOD (river_min_radius) must keep the river drawn,
	# smooth (mipmapped river array) and not shimmering.
	_map.display_snapshot(_snapshot_rivers(FAR_GRID_W, FAR_GRID_H))
	_map._fit_map_to_view()
	await _settle()
	await _save("map_rivers_farzoom")

	# State "rivers web" — the REGRESSION GUARD. A solid clump of adjacent navigable hexes with the channel
	# winding through it as ONE snake. Read: exactly one channel, winding; NO cross-links between the
	# snake's neighbouring runs, and above all NO triangular holes. Every navigable hex here is a legitimate
	# chain hex, so nothing is orphaned — the only difference between right and wrong is whether the
	# renderer takes the sim's river_channel or guesses from the terrain. If it ever guesses again, this
	# frame turns into a mesh.
	_map.display_snapshot(_snapshot_rivers_web(GRID_W, GRID_H))
	_map._fit_map_to_view()
	await _settle()
	_map.pan_offset += Vector2(0.0, RIVER_PAN_ROWS * _map.last_hex_radius)
	await _settle()
	await _save_crop_rect("map_rivers_web", RIVER_WEB_CROP)

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

## Save a crop given as a fraction RECT of the frame (x, y, w, h) — the Rect2 form of _save_crop.
func _save_crop_rect(name: String, frac: Rect2) -> void:
	await _save_crop(name, frac.position.x, frac.position.y, frac.end.x, frac.end.y)

## Save a square crop of `2*half` px centered on `center` (VIEWPORT pixels — e.g. a hex center from
## MapView._hex_center), clamped to the image bounds. Used by State R to lock onto the SAME hex across
## fit/pan/zoom so a swim shows as a shift, and by the rivers state for the trunk-head close-up.
## The captured framebuffer can be LARGER than the viewport's logical rect (HiDPI / window content scale —
## e.g. a 3921-px-wide viewport captured as a 5120-px image), so the incoming viewport-space center and
## half-size are rescaled into IMAGE pixels first; without that the crop lands a hex or two off target.
func _save_crop_px(name: String, center: Vector2, half: float) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("map_preview: null image (dummy renderer?) — run without --headless")
		return
	var w := image.get_width()
	var h := image.get_height()
	var px_scale := float(w) / maxf(get_viewport().get_visible_rect().size.x, 1.0)  # viewport px → image px
	var cx := center.x * px_scale
	var cy := center.y * px_scale
	var half_px := half * px_scale
	var x0 := clampi(int(cx - half_px), 0, w - 1)
	var y0 := clampi(int(cy - half_px), 0, h - 1)
	var x1 := clampi(int(cx + half_px), 0, w)
	var y1 := clampi(int(cy + half_px), 0, h)
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

## The staircase row for column `x`: hexes with y < f(x) are the river's NORTH bank (the region whose
## boundary IS the river). Each step of the staircase is a corner turn in the edge chain.
func _river_bank_row(x: int, gw: int, gh: int, nav_start: int) -> int:
	var idx: int = int(float(x) * RIVER_PATTERN.size() / float(maxi(nav_start, 1)))
	idx = clampi(idx, 0, RIVER_PATTERN.size() - 1)
	var wander: int = maxi(1, gh / RIVER_WANDER_BASE_H)  # scale the staircase to taller (far-zoom) grids
	return clampi(int(gh * RIVER_BASE_ROW_FRAC) + int(RIVER_PATTERN[idx]) * wander, 1, gh - 2)

## Odd-r neighbour of (x, y) in sim direction `dir`; Vector2i(-1, -1) when the step leaves the map.
func _river_neighbor(x: int, y: int, dir: int, gw: int, gh: int) -> Vector2i:
	var off: Array = RIVER_DIR_OFFSETS[dir]
	var nx: int = x + int(off[1] if (y % 2) == 1 else off[0])
	var ny: int = y + int(off[2])
	if nx < 0 or nx >= gw or ny < 0 or ny >= gh:
		return Vector2i(-1, -1)
	return Vector2i(nx, ny)

## How many of `cell`'s six neighbours are already NavigableRiver — i.e. how many trunk ARMS a navigable
## hex placed there would grow (the shader's arm rule, minus the water/delta cases, which the branch's
## inland placement cannot hit).
func _river_navigable_neighbors(terrain: Array, cell: Vector2i, gw: int, gh: int) -> int:
	var n := 0
	for dir in range(RIVER_CORNERS):
		var nb := _river_neighbor(cell.x, cell.y, dir, gw, gh)
		if nb.x >= 0 and int(terrain[nb.y * gw + nb.x]) == RIVER_NAVIGABLE_ID:
			n += 1
	return n

## Stamp river class `cls` on edge (x, y, dir) — on BOTH flanking hexes (the neighbour carries the
## opposite direction, (dir + 3) % 6), exactly as the sim does, so each hex can answer locally.
func _river_set_edge(masks: Dictionary, x: int, y: int, dir: int, nb: Vector2i, cls: int) -> void:
	var here := Vector2i(x, y)
	masks[here] = (int(masks.get(here, 0)) & ~(3 << (2 * dir))) | (cls << (2 * dir))
	var back: int = (dir + 3) % 6
	masks[nb] = (int(masks.get(nb, 0)) & ~(3 << (2 * back))) | (cls << (2 * back))

## Hang the standard 3-edge MINOR tributary off navigable hex `h`, handing over at h's BOTTOM vertex
## (RIVER_TRIB_TERMINUS_CORNER): the edges (h, SW), (h's W neighbour, SE) and (h's SW neighbour, W), each
## consecutive pair sharing a corner, so the chain is contiguous on either row parity. Sets the inflow bit on
## `h` — ORed, because a hex may be fed by more than one tributary (the trunk head is fed by two). Used for
## BOTH the head's Minor tributary and the MID-CHAIN junction: the sim's river_inflow no longer says anything
## about where in the chain a hex sits, so the fixture builds the two cases from one construction.
## Returns false (touching nothing) if the tributary's own hexes are off-map or are not plain land — running
## river edges over the trunk or the sea would be a lie about the geometry the shader is being judged on.
func _river_attach_minor_tributary(masks: Dictionary, inflow: Dictionary, terrain: Array, h: Vector2i,
		gw: int, gh: int) -> bool:
	var h_w := _river_neighbor(h.x, h.y, RIVER_DIR_W, gw, gh)
	var h_sw := _river_neighbor(h.x, h.y, RIVER_DIR_SW, gw, gh)
	if h_w.x < 0 or h_sw.x < 0:
		return false
	if int(terrain[h_w.y * gw + h_w.x]) != RIVER_LAND_ID:
		return false
	if int(terrain[h_sw.y * gw + h_sw.x]) != RIVER_LAND_ID:
		return false
	_river_set_edge(masks, h.x, h.y, RIVER_DIR_SW, h_sw, RIVER_CLASS_MINOR)
	_river_set_edge(masks, h_w.x, h_w.y, RIVER_DIR_SE, h_sw, RIVER_CLASS_MINOR)
	var trib_up := _river_neighbor(h_sw.x, h_sw.y, RIVER_DIR_W, gw, gh)
	if trib_up.x >= 0 and int(terrain[trib_up.y * gw + trib_up.x]) == RIVER_LAND_ID:
		_river_set_edge(masks, h_sw.x, h_sw.y, RIVER_DIR_W, trib_up, RIVER_CLASS_MINOR)
	inflow[h] = int(inflow.get(h, 0)) \
		| (RIVER_CLASS_MINOR << (RIVER_CLASS_BITS * RIVER_TRIB_TERMINUS_CORNER))
	return true

## Set the channel-EXIT bit for odd-r direction `dir` on `hex` — the fixture's stand-in for the sim's
## `river_channel`. OR-ed, never overwritten: a hex mid-chain carries both its upstream and its downstream
## side, and a confluence carries the union of the chains through it.
func _river_set_channel(channel: Dictionary, hex: Vector2i, dir: int) -> void:
	channel[hex] = int(channel.get(hex, 0)) | (RIVER_CHANNEL_EXIT_BIT << dir)

## Link two CONSECUTIVE navigable hexes: both name the side they share (a → b and b → a), exactly as the
## sim does. The chain is a path, so this is the only way an interior hex ever gets an exit.
func _river_link_channel(channel: Dictionary, a: Vector2i, b: Vector2i, gw: int, gh: int) -> void:
	for dir in range(RIVER_CORNERS):
		if _river_neighbor(a.x, a.y, dir, gw, gh) == b:
			_river_set_channel(channel, a, dir)
			_river_set_channel(channel, b, (dir + RIVER_CORNERS / 2) % RIVER_CORNERS)
			return
	push_warning("map_preview: river chain hexes %s and %s are not neighbours — channel link skipped" % [a, b])

## The chain's MOUTH exit: its final hex must ALSO exit toward the water it drains into, or the drawn river
## stops one hex short of the sea. Mirrors the sim (hydrology.rs): the first direction that is not the way
## back upstream and whose neighbour is open water or the river's own delta. Deliberately NOT mirrored back
## — that water carries no channel of its own, so this is the mask's one asymmetric bit.
func _river_mouth_channel(channel: Dictionary, terrain: Array, last: Vector2i, upstream: Vector2i,
		gw: int, gh: int) -> void:
	for dir in range(RIVER_CORNERS):
		var nb := _river_neighbor(last.x, last.y, dir, gw, gh)
		if nb.x < 0 or nb == upstream:
			continue
		var tid: int = int(terrain[nb.y * gw + nb.x])
		if tid == RIVER_DELTA_ID or tid == RIVER_OCEAN_ID or tid == RIVER_LAKE_ID:
			_river_set_channel(channel, last, dir)
			return

## One tile dict per hex carrying ANY of the three river masks — shaped exactly like the native decoder's
## tile_to_dict: river_edges by SIDE (2 bits), river_inflow by CORNER (2 bits), river_channel by SIDE
## (1 bit). A hex may carry any combination (a trunk head carries all three).
func _river_tiles(gw: int, masks: Dictionary, inflow: Dictionary, channel: Dictionary) -> Array:
	var keys: Dictionary = {}
	for key: Vector2i in masks:
		keys[key] = true
	for key: Vector2i in inflow:
		keys[key] = true
	for key: Vector2i in channel:
		keys[key] = true
	var tiles: Array = []
	for key: Vector2i in keys:
		tiles.append({
			"entity": key.y * gw + key.x,
			"x": key.x,
			"y": key.y,
			"river_edges": int(masks.get(key, 0)),
			"river_inflow": int(inflow.get(key, 0)),
			"river_channel": int(channel.get(key, 0)),
		})
	return tiles

## The CORNER an edge chain running along this hex's sides terminates on, plus that chain's class — the
## fixture's stand-in for the sim's `river_inflow` (which the real snapshot ships per tile). Side `dir`
## spans corners {dir - 1, dir}, so within one hex's carried edges a corner has DEGREE 2 where the chain
## turns and DEGREE 1 at each of its two ends. This river flows west→east, so the downstream end — the
## vertex the water leaves the edge model at and enters the trunk through — is the degree-1 corner
## furthest EAST. Returns Vector2i(corner, class), or (-1, 0) when the hex carries no edge chain.
func _river_inflow_corner(mask: int) -> Vector2i:
	var degree := PackedInt32Array()
	degree.resize(RIVER_CORNERS)
	var corner_class := PackedInt32Array()
	corner_class.resize(RIVER_CORNERS)
	for dir in range(RIVER_CORNERS):
		var cls: int = (mask >> (RIVER_CLASS_BITS * dir)) & RIVER_CLASS_MASK
		if cls == 0:
			continue
		for corner: int in [(dir + RIVER_CORNERS - 1) % RIVER_CORNERS, dir]:
			degree[corner] += 1
			corner_class[corner] = maxi(corner_class[corner], cls)  # the wider class wins (as in the sim)
	var best := -1
	var best_x := -INF
	for corner in range(RIVER_CORNERS):
		if degree[corner] != 1:
			continue
		var cx: float = cos(deg_to_rad(RIVER_CORNER_ANGLE_STEP_DEG * corner + RIVER_CORNER_ANGLE_OFFSET_DEG))
		if cx > best_x:
			best_x = cx
			best = corner
	if best < 0:
		return Vector2i(-1, 0)
	return Vector2i(best, corner_class[best])

## Terrain + per-tile river-edge masks for State "rivers": a Minor→Major edge river wandering west→east
## with corner turns, joining a NavigableRiver hex chain that runs out to the eastern sea.
func _snapshot_rivers(gw: int, gh: int) -> Dictionary:
	var nav_start: int = gw - RIVER_OCEAN_COLS - RIVER_NAV_HEXES  # edge chain stops here; hexes take over
	var major_from: int = int(nav_start * RIVER_MAJOR_FROM_FRAC)
	var terrain: Array = []
	terrain.resize(gw * gh)
	for y in range(gh):
		for x in range(gw):
			terrain[y * gw + x] = RIVER_OCEAN_ID if x >= gw - RIVER_OCEAN_COLS else RIVER_LAND_ID

	# Every edge between the north-bank region (y < f(x)) and its complement, within the edge-chain
	# columns. A region boundary is a contiguous chain by construction — no gaps, corners for free.
	var masks: Dictionary = {}
	for x in range(nav_start):
		for y in range(_river_bank_row(x, gw, gh, nav_start)):  # y < f(x) → in the region
			for dir in range(6):
				var nb := _river_neighbor(x, y, dir, gw, gh)
				if nb.x < 0 or nb.x >= nav_start:
					continue  # off-map, or past where the river stops being an edge
				if nb.y < _river_bank_row(nb.x, gw, gh, nav_start):
					continue  # neighbour is in the region too → interior, not a boundary edge
				_river_set_edge(masks, x, y, dir, nb,
					RIVER_CLASS_MAJOR if x >= major_from else RIVER_CLASS_MINOR)

	# The navigable chain starts at the SOUTH-bank hex the last edge flanks, so the edge river and the hex
	# river join with no gap (exactly how the sim hands off at the navigable discharge threshold). That HEAD
	# hex flanks the incoming Major chain along two of its sides — and an edge river ends at a VERTEX, not
	# mid-edge, so what the trunk needs to know is WHICH CORNER the chain terminates on. That is the sim's
	# river_inflow (nonzero on the head only); here it is reconstructed geometrically. From the head the
	# chain WALKS to the sea, turning corners on the way.
	var mouth_col: int = gw - RIVER_OCEAN_COLS - 1   # last land column; everything beyond is open sea
	var head := Vector2i(nav_start - 1, _river_bank_row(nav_start - 1, gw, gh, nav_start))
	var inflow: Dictionary = {}
	var trunk_inflow := _river_inflow_corner(int(masks.get(head, 0)))
	if trunk_inflow.x >= 0:
		inflow[head] = int(trunk_inflow.y) << (RIVER_CLASS_BITS * int(trunk_inflow.x))

	# A SECOND tributary — MINOR — joining the same head at a DIFFERENT corner (its bottom vertex). Three
	# jobs: it puts a THIRD river edge on the head (the playtest case that used to blob: several river edges
	# on one navigable hex → several fat centre→midpoint arms → a hex full of water); it proves a tributary
	# arrives at ITS OWN width, not the trunk's, since the Major and Minor spurs land side by side; and it
	# proves the inflow mask is read for ALL SIX corners, not just one.
	_river_attach_minor_tributary(masks, inflow, terrain, head, gw, gh)

	var p := head
	terrain[p.y * gw + p.x] = RIVER_NAVIGABLE_ID
	var trunk: Array[Vector2i] = [head]
	var step := 0
	while p.x < mouth_col and step < RIVER_NAV_MAX_STEPS:
		var nb := _river_neighbor(p.x, p.y, int(RIVER_NAV_STEPS[step % RIVER_NAV_STEPS.size()]), gw, gh)
		step += 1
		if nb.x < 0:
			break
		p = nb
		terrain[p.y * gw + p.x] = RIVER_NAVIGABLE_ID
		trunk.append(p)

	# The trunk's CHANNEL EXITS — the sim's river_channel, and the only thing the shader arms an arm from.
	# A chain is a path: each hex names the sides it shares with its upstream and downstream neighbours, and
	# nothing else. (The head names no exit toward its tributary: that water arrives at a VERTEX and is drawn
	# by the inflow SPUR, so an exit there would double-encode it.)
	var channel: Dictionary = {}
	for i in range(trunk.size() - 1):
		_river_link_channel(channel, trunk[i], trunk[i + 1], gw, gh)

	# The MINOR-ONLY head: a one-hex navigable BRANCH hanging off the trunk's NW, fed by a single Minor
	# tributary (the mirror of the main head's, so it is contiguous by the same argument). Its ONE arm runs
	# to the trunk hex it joins, so with the head taper it must start at the Minor's hairline width at its
	# centre and reach the full channel width exactly at that shared edge — the whole point of the taper,
	# and the frame it is judged on (map_rivers_head_minor). Placed at the first trunk hex whose NW
	# neighbour is well clear of the edge chain's columns, so the branch's own masks cannot collide with it.
	_river_branch_head = Vector2i(-1, -1)
	for i in range(1, trunk.size()):
		var b := _river_neighbor(trunk[i].x, trunk[i].y, RIVER_DIR_NW, gw, gh)
		if b.x < nav_start + 1 or b.x > mouth_col:
			continue  # off-map, in the sea, or close enough to the edge chain to share hexes with it
		if terrain[b.y * gw + b.x] == RIVER_NAVIGABLE_ID:
			continue  # the trunk already turned through this hex
		if _river_navigable_neighbors(terrain, b, gw, gh) != 1:
			continue  # must hang off ONE trunk hex: two would give the branch head two arms (a loop), and
			          # the frame is meant to read as one tapering arm handing over at one shared edge
		var b_w := _river_neighbor(b.x, b.y, RIVER_DIR_W, gw, gh)
		var b_nw := _river_neighbor(b.x, b.y, RIVER_DIR_NW, gw, gh)
		if b_w.x < 0 or b_nw.x < 0:
			continue
		terrain[b.y * gw + b.x] = RIVER_NAVIGABLE_ID
		_river_set_edge(masks, b.x, b.y, RIVER_DIR_NW, b_nw, RIVER_CLASS_MINOR)
		_river_set_edge(masks, b_w.x, b_w.y, RIVER_DIR_NE, b_nw, RIVER_CLASS_MINOR)
		var b_up := _river_neighbor(b_nw.x, b_nw.y, RIVER_DIR_W, gw, gh)
		if b_up.x >= 0:
			_river_set_edge(masks, b_nw.x, b_nw.y, RIVER_DIR_W, b_up, RIVER_CLASS_MINOR)
		inflow[b] = RIVER_CLASS_MINOR << (RIVER_CLASS_BITS * RIVER_BRANCH_TERMINUS_CORNER)
		# The branch is a one-hex chain that CONFLUENCES into the trunk: its single exit is the side it
		# shares with trunk[i], and trunk[i] carries the mirrored bit back (a confluence hex holds the union
		# of the chains through it). That one arm is what the head taper is judged on.
		_river_link_channel(channel, b, trunk[i], gw, gh)
		_river_branch_head = b
		break

	# The MID-CHAIN TRIBUTARY JUNCTION — the case the drainage network created and this fixture never had.
	# The same 3-edge Minor tributary as the head's, but hung on a hex in the MIDDLE of the trunk: it has an
	# upstream AND a downstream channel exit, so it is NOT a chain head, yet it now carries a nonzero
	# river_inflow. The shader must gate its head taper on the EXIT COUNT, not on that inflow, or the trunk
	# pinches to the Minor's width at this hex's centre — the hourglass. Read map_rivers_midchain.png for:
	# constant full width straight through the junction, and the Minor spur still reaching its vertex.
	# Placement is not free: the tributary hangs off the junction's W and SW neighbours, and on most steps of
	# RIVER_NAV_STEPS the trunk's own UPSTREAM hex already sits there (an E step arrives from the W; an NE
	# step from the SW), so the tributary would be drawn over the channel. Only a hex the trunk entered from
	# the NW (an SE step) has both slots free — and they must also be clear of the EDGE chain's own masks, or
	# the Minor would fuse into the staircase river instead of reading as its own tributary.
	_river_midchain_junction = Vector2i(-1, -1)
	for i in range(1, trunk.size() - 1):   # never the head (i = 0) nor the mouth (the last hex)
		var m: Vector2i = trunk[i]
		if inflow.has(m):
			continue  # already fed (the trunk head) — this frame is about a hex that is NOT a head
		var m_w := _river_neighbor(m.x, m.y, RIVER_DIR_W, gw, gh)
		var m_sw := _river_neighbor(m.x, m.y, RIVER_DIR_SW, gw, gh)
		if m_w.x < 0 or m_sw.x < 0:
			continue
		if masks.has(m_w) or masks.has(m_sw):
			continue  # would collide with the edge chain's river
		if not _river_attach_minor_tributary(masks, inflow, terrain, m, gw, gh):
			continue  # a trunk hex (or the sea) is sitting where the tributary would run
		_river_midchain_junction = m
		break

	# The MOUTH: the final navigable hex sits against OPEN SEA on its seaward side and a RiverDelta
	# distributary lobe on its SE (the shape the sim actually produces — the chain hands off to a delta LAND
	# tile before the coast). Its exit into that water is the one bit of river_channel that is NOT mirrored
	# back, and without it the river dead-ends a hex short of the sea.
	var delta := _river_neighbor(p.x, p.y, RIVER_DIR_SE, gw, gh)
	if delta.x >= 0 and delta.x <= mouth_col:
		terrain[delta.y * gw + delta.x] = RIVER_DELTA_ID
	var mouth_upstream: Vector2i = trunk[trunk.size() - 2] if trunk.size() > 1 else Vector2i(-1, -1)
	_river_mouth_channel(channel, terrain, p, mouth_upstream, gw, gh)

	# The lake — a real InlandSea, inland, far from the river. It still gets the beach + foam shore pass; a
	# navigable hex no longer does. Side by side in one frame, they must not read as the same thing.
	var lake_col: int = clampi(int(gw * RIVER_LAKE_COL_FRAC), 1, gw - 2)
	var lake_row: int = clampi(int(gh * RIVER_LAKE_ROW_FRAC), 1, gh - 2)
	for cell: Array in RIVER_LAKE_HEXES:
		var lx: int = clampi(lake_col + int(cell[0]), 0, gw - 1)
		var ly: int = clampi(lake_row + int(cell[1]), 0, gh - 1)
		terrain[ly * gw + lx] = RIVER_LAKE_ID

	return {
		"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"tiles": _river_tiles(gw, masks, inflow, channel),
		"populations": [],
		"herds": [],
	}

## State "rivers web" — a solid CLUMP of navigable hexes with the channel winding through it as a single
## snake (see RIVER_WEB_* ). The regression guard for the spider-web bug: honour river_channel and only the
## snake draws; infer arms from terrain again and every adjacent pair in the clump cross-links into a mesh.
func _snapshot_rivers_web(gw: int, gh: int) -> Dictionary:
	var terrain: Array = []
	terrain.resize(gw * gh)
	for y in range(gh):
		for x in range(gw):
			terrain[y * gw + x] = RIVER_OCEAN_ID if x >= gw - RIVER_OCEAN_COLS else RIVER_LAND_ID

	# The clump: RIVER_WEB_ROWS × RIVER_WEB_COLS of adjacent navigable hexes, its EAST column against the
	# last land column so the snake's final hex can open straight into the sea.
	var mouth_col: int = gw - RIVER_OCEAN_COLS - 1
	var col0: int = maxi(mouth_col - (RIVER_WEB_COLS - 1), 0)
	var row0: int = clampi(int(gh * RIVER_WEB_ROW_FRAC), 1, maxi(gh - RIVER_WEB_ROWS - 1, 1))
	for dr in range(RIVER_WEB_ROWS):
		for dc in range(RIVER_WEB_COLS):
			terrain[(row0 + dr) * gw + col0 + dc] = RIVER_NAVIGABLE_ID

	# The snake: a boustrophedon walk over the clump — run the row, drop one row in the SAME column, run
	# back. Walked with real odd-r steps (never index arithmetic), so every consecutive pair is genuinely
	# adjacent. Rows are run W, E, W, E so the LAST hex is the clump's SE corner, on the coast.
	# Dropping a row in the same column is SE from an even row and SW from an odd one (odd-r offsets).
	var path: Array[Vector2i] = [Vector2i(col0 + RIVER_WEB_COLS - 1, row0)]
	var cur := path[0]
	for r in range(RIVER_WEB_ROWS):
		var run_dir: int = RIVER_DIR_W if (r % 2) == 0 else RIVER_DIR_E
		for _i in range(RIVER_WEB_COLS - 1):
			cur = _river_neighbor(cur.x, cur.y, run_dir, gw, gh)
			path.append(cur)
		if r == RIVER_WEB_ROWS - 1:
			break
		var down_dir: int = RIVER_DIR_SE if (cur.y % 2) == 0 else RIVER_DIR_SW
		cur = _river_neighbor(cur.x, cur.y, down_dir, gw, gh)
		path.append(cur)

	var channel: Dictionary = {}
	for i in range(path.size() - 1):
		_river_link_channel(channel, path[i], path[i + 1], gw, gh)
	# ... plus the mouth exit, straight east into the open sea (unmirrored, as in the sim).
	_river_mouth_channel(channel, terrain, path[path.size() - 1], path[path.size() - 2], gw, gh)

	return {
		"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"tiles": _river_tiles(gw, {}, {}, channel),
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
