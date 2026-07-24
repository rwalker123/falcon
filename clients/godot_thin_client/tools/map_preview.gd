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

# The canvas every state renders at unless it asks for another (see PASTURE_WINDOW_SIZE). MapView is
# cover-fit, so the canvas ASPECT decides what a frame shows — which is why this is pinned rather than
# left to whatever the WM hands us.
const DEFAULT_CANVAS_SIZE := Vector2i(1000, 800)
# How many frames _ensure_canvas will keep re-asserting the pinned canvas while it waits for the WM to
# honour it (project.godot opens MAXIMIZED; the mode change lands asynchronously). Bounded so a WM that
# refuses to shrink the window fails loudly rather than hanging.
const CANVAS_PIN_MAX_FRAMES := 60

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
# Quarry-targeting state: the band's hunt reach and the two herd offsets that straddle it (one inside
# → a local hunt, no glow; one beyond → a valid quarry, glowed).
const QUARRY_HUNT_REACH := 3
const QUARRY_NEAR_OFFSET := 2
const QUARRY_FAR_OFFSET := 6
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
# --- State "pasture" (the graze layer, Phase 2a) -------------------------------------------------
const PASTURE_OVERLAY_KEY := "pasture"     # mirrors MapView.PASTURE_OVERLAY_KEY / the decoder's channel key
const PASTURE_GRID_W := 26
const PASTURE_GRID_H := 18
# The big-game herd parked mid-prairie for the "herd range over pasture" state (Grazing Phase 2b-iii):
# col 9 / row 7 sits inside the prairie block (rows 5–10, cols 6–12), so its radius-1 grazing range
# (7 tiles) lands entirely on the rich reference pasture — the ring-over-graze the state exists to show.
const PASTURE_HERD_ID := "game_deer_09"
const PASTURE_HERD_COL := 9
const PASTURE_HERD_ROW := 7
const PASTURE_HERD_RANGE_RADIUS := 1
# MapView is COVER-fit, so a grid whose aspect differs from the window's is CROPPED at the fit zoom —
# and a pasture distribution you can only see two thirds of is exactly the frame this state exists to
# avoid. The pointy-top odd-r extents of this grid are ≈ (W + 0.5)·√3 × (1.5·H + 0.5) hex radii, i.e.
# ≈ 45.9 × 27.5 ≈ 1.67:1, so the window is set to match for this state (it is the last one rendered).
const PASTURE_WINDOW_SIZE := Vector2i(1200, 720)
# The sim's own per-biome graze capacities (core_sim/src/data/fauna_config.json → graze.capacity_by_biome),
# keyed by terrain id. Transcribed, NOT invented — the whole state is worthless if the numbers are made up.
# PrairieSteppe (240) is the reference pasture; MixedWoodland (55) is deliberately poor (a closed canopy
# shades the ground cover out); water/glacier/lava are a stated 0.
const PASTURE_CAPACITY_BY_TERRAIN := {
	0: 0.0,      # deep_ocean
	1: 0.0,      # continental_shelf
	10: 110.0,   # alluvial_plain — the tag solver's fallback biome, so it is everywhere
	11: 240.0,   # prairie_steppe — the reference pasture
	12: 55.0,    # mixed_woodland — poor: the canopy shades out the ground cover
	15: 8.0,     # hot_desert_erg — marginal, but NOT zero (the "full 8/8" case)
	20: 100.0,   # tundra — thin but real
	22: 0.0,     # glacier — no pasture at all
	26: 65.0,    # alpine_mountain
	30: 0.0,     # basaltic_lava_field — no pasture at all
}
# The Water terrain tag (bit 0), the same server truth MapView._pasture_color splits sea from dead ground on.
const PASTURE_WATER_TAG := 1 << 0
const PASTURE_WATER_IDS := [0, 1]   # the water biomes in this fixture (deep_ocean / continental_shelf)
# Phase 2a ships the layer INERT — nothing eats graze yet — so every patch stands at FULL biomass and
# reads Thriving, and this fixture says so rather than staging a fictional overgrazed blob the sim
# cannot yet produce. (The stressed/collapsing tint is exercised on the tile card in ui_preview.)

# --- State "forage" (the human-food layer, the twin of "pasture") --------------------------------
const FORAGE_OVERLAY_KEY := "forage"       # mirrors MapView.FORAGE_OVERLAY_KEY / the decoder's channel key
# The sim's own per-biome HUMAN-food capacities (core_sim/src/data/labor_config.json →
# forage.capacity_by_biome), keyed by the SAME terrain ids the pasture fixture uses — so the two states
# render the identical earthlike shape and the DIVERGENCE reads directly (forest/river rich where prairie
# is poor; the coastal shelf LIGHTS UP as fishing where pasture is dead). Transcribed, NOT invented.
const FORAGE_CAPACITY_BY_TERRAIN := {
	0: 0.0,      # deep_ocean — no human food (barren)
	1: 130.0,    # continental_shelf — FISHING: the coastal larder lights up (pasture reads this 0)
	10: 195.0,   # alluvial_plain — silt + water = the richest cropland (the dominant interior)
	11: 70.0,    # prairie_steppe — grass feeds animals; humans get only seed heads (the INVERSION)
	12: 190.0,   # mixed_woodland — mast, nuts, berries: rich human food (the FLAGSHIP inversion)
	15: 5.0,     # hot_desert_erg — near-barren for humans
	20: 25.0,    # tundra — thin
	22: 0.0,     # glacier — a stated 0
	26: 20.0,    # alpine_mountain — thin (rangeland: better for animals than humans)
	30: 0.0,     # basaltic_lava_field — a stated 0
}
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
const NOTCH_ZOOM_IN := 1.5          # extra zoom for the notch frame so the head-hex channel reads clearly
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

# State "riverine split" — proof of the terrain-aware riverine_delta food glyph (FoodIcons.for_site).
# Two riverine_delta food sites in one frame on DIFFERENT terrains: an open navigable river (37 → 🐟)
# and a dry alluvial-plain floodplain (10 → 🎋), so the fish↔reeds split reads side by side. MapView
# stamps each site's terrain_id, which the map marker (and the HUD Forage row) resolve through for_site.
const RIVERINE_NAV_TERRAIN_ID := 37    # navigable_river → real open water → 🐟
const RIVERINE_LAND_TERRAIN_ID := 10   # alluvial_plain → dry floodplain LAND → 🎋
const RIVERINE_FISH_X := 6             # column of the open-water (fish) site
const RIVERINE_REED_X := 10            # column of the dry-floodplain (reed) site
const RIVERINE_SITE_Y := 6            # shared row so both markers sit at the same height, easy to compare

# --- The ANNOTATION states (trade / crisis / terrain highlight / routes) -------------------------
# These four cover the `AnnotationRenderer` family, which had NO fixture at all before. They were
# written AFTER the code they cover, so they encode CURRENT BEHAVIOUR — bugs included. They prove
# "this refactor changed nothing", NOT "this rendering is correct"; do not read a passing byte-diff
# as a correctness result.
#
# State "trade overlay". Trade links address their endpoints by TILE ENTITY id, which MapView resolves
# through `tile_lookup` (built from `tiles[].entity`) — so this is the one flat-backdrop fixture that
# has to publish a `tiles` array at all. The three link entities exist to fan the draw's branches out
# across one frame: the SELECTED one (green + widened), a busy open one, and a thin closed one whose
# leak is imminent (a red midpoint dot).
const TRADE_SELECTED_ENTITY := 4201    # the caravan TradePanel reports as selected → the selection branch
const TRADE_BUSY_ENTITY := 4202        # high throughput + openness → the widest, most opaque amber line
const TRADE_LEAKING_ENTITY := 4203     # low throughput + openness → thin/faint, and its leak fires
# `leak_timer <= 1` is the draw's own test for "this link leaks knowledge NOW" (the red midpoint dot).
const TRADE_LEAK_QUIET := 5            # above the test → no dot
const TRADE_LEAK_IMMINENT := 0         # at/below it → dot
# A link whose endpoints are NOT in `tile_lookup` — the draw skips it. Present so the guard is
# EXERCISED by the reference frame; a refactor that dropped it would start drawing a line here.
const TRADE_UNRESOLVED_TILE := -1
# Link endpoints (col, row) on the flat GRID_W×GRID_H backdrop, spread so no two lines overlap.
const TRADE_SELECTED_FROM := Vector2i(2, 2)
const TRADE_SELECTED_TO := Vector2i(13, 3)
const TRADE_BUSY_FROM := Vector2i(3, 9)
const TRADE_BUSY_TO := Vector2i(12, 8)
const TRADE_LEAKING_FROM := Vector2i(7, 1)
const TRADE_LEAKING_TO := Vector2i(6, 10)
const TRADE_BUSY_THROUGHPUT := 8.0     # ≫ the draw's 2.5 intensity clamp → full width
const TRADE_BUSY_OPENNESS := 0.9       # → the opacity clamp's top end
const TRADE_QUIET_THROUGHPUT := 1.0    # → a barely-thickened line
const TRADE_QUIET_OPENNESS := 0.05     # → the opacity clamp's floor

# State "crisis annotations". The draw is gated on the `crisis` overlay channel being ACTIVE, so the
# fixture publishes that channel (a west→east pressure ramp, so the backdrop isn't a flat wash) and
# selects it after the snapshot lands — `display_snapshot` clears the active overlay every time.
const CRISIS_CHANNEL_KEY := "crisis"   # mirrors the decoder's channel key / MapView's OVERLAY_COLORS entry
const CRISIS_RAW_SCALE := 100.0        # raw = normalized × this, so the legend reads as a 0..100 pressure
# The four annotation SHAPES the draw can produce, one per entry (see `_crisis_annotations`):
# a multi-hop PackedInt32Array path, a multi-hop Array-of-pairs path, a single-tile marker, and a
# single-tile marker with an unknown severity (which falls back to the base CRISIS_COLOR) and no label.
# The PACKED ones are authored as plain int Arrays because a PackedInt32Array constructor is not a
# constant expression in GDScript; `_crisis_annotations` converts them, and that conversion is
# LOAD-BEARING — the draw branches on the exact type, and a plain Array of flat ints would fall into
# the Array-of-pairs branch and render nothing.
const CRISIS_PATH_PACKED := [2, 2, 5, 3, 8, 3, 11, 4]   # flattened col,row pairs
const CRISIS_PATH_PAIRS := [[3, 8], [6, 9], [9, 9]]      # the Array-of-[col,row] form
const CRISIS_POINT_SAFE := [11, 7]   # kept off the right edge: MapView is cover-fit, and the label reads outward
const CRISIS_POINT_UNKNOWN := [5, 6]
const CRISIS_SEVERITY_UNKNOWN := "quiet"   # not in CRISIS_SEVERITY_COLORS → the CRISIS_COLOR fallback

# State "terrain highlight". The Terrain tab's "highlight every tile of this type" tool, run on the
# four-band biome map so the MATCHED band and the three UNMATCHED ones read in the same frame.
const TERRAIN_HIGHLIGHT_TARGET_ID := 11   # prairie_steppe — BIOME_BAND_IDS[1], the second of four bands
const TERRAIN_HIGHLIGHT_OFF := -1         # MapView's "no highlight" sentinel

# State "routes". Order paths, drawn as per-faction polylines. Faction lookup is by the raw `faction`
# value, so the three routes cover MapView.faction_colors' INT key, its STRING key, and an unknown
# faction (the amber default). Multi-hop with turns, because a straight two-point line would not
# exercise the segment loop.
const ROUTE_PLAYER_FACTION := 0             # int key → the player cyan
const ROUTE_RIVAL_FACTION := "Obsidian"     # string key → orange
const ROUTE_UNKNOWN_FACTION := "Wayfarers"  # absent from faction_colors → the default amber
const ROUTE_PLAYER_PATH := [[1, 2], [3, 3], [5, 3], [7, 4], [9, 4], [11, 5]]
const ROUTE_RIVAL_PATH := [[2, 10], [4, 9], [6, 9], [8, 8], [10, 8]]
const ROUTE_UNKNOWN_PATH := [[1, 5], [3, 6], [2, 8]]   # left of the other two, and inside the cover-fit crop
# A one-waypoint order — the draw bails at `points.size() < 2`. Present for the same reason as
# TRADE_UNRESOLVED_TILE: a guard only guards the reference frame if the frame exercises it.
const ROUTE_DEGENERATE_PATH := [[5, 11]]

var _map: Node2D
# Where _snapshot_rivers put the MINOR-only navigable head (see RIVER_BRANCH_TERMINUS_CORNER). Reported
# back rather than recomputed, because the placement walks the trunk and has to dodge it; (-1, -1) if the
# grid left no room for one (the far-zoom grid is built after the close-ups, so it may overwrite this).
var _river_branch_head := Vector2i(-1, -1)
# Where _snapshot_rivers put the MID-CHAIN tributary junction (see RIVER_MIDCHAIN_MIN_COL_MARGIN). Reported
# back for the same reason as the branch head; (-1, -1) if the grid left no room for one.
var _river_midchain_junction := Vector2i(-1, -1)
# Where _snapshot_rivers_notch put the head hex whose inflow corner and single exit side flank the SAME
# vertex — the geometry that used to draw a NOTCH. Reported back so the crop can centre on it.
var _river_notch_head := Vector2i(-1, -1)
# Where _snapshot_rivers_lake_alongside put the one-hex inland_sea ringed by navigable hexes that merely
# run ALONGSIDE it (no channel exits toward the lake) — the @21,61 case the shore-pass mouth test fixes.
var _river_lake_hex := Vector2i(-1, -1)

# The canvas currently pinned. A state changes it through _set_canvas() and does NOT restore it — every
# state that needs a particular canvas asks for it, and today's frames depend on that sequence. The
# aspect-matched pasture/forage/danger states switch to PASTURE_WINDOW_SIZE and leave it there (the
# river states inherit it); the ANNOTATION states that follow switch back to DEFAULT_CANVAS_SIZE,
# because their fixtures are authored against the GRID_W×GRID_H grid like the earlier states.
var _canvas_size: Vector2i = DEFAULT_CANVAS_SIZE

func _ready() -> void:
	# FREEZE ANIMATION TIME. What it buys: with the canvas pinned, the only remaining run-to-run
	# difference was animated content, so this is what makes the frame set a STRICT BIT-IDENTITY
	# REFERENCE (56/56 identical across runs) — which is the whole reason the harness exists, since a
	# frame that varies cannot be pixel-diffed to prove a refactor changed nothing. What it costs:
	# every animation renders at a FIXED PHASE rather than being sampled wherever the clock happened
	# to land. It affects 14 frames — the 11 `map_rivers*` (the shader's `TIME * river_flow_speed`
	# channel scroll), `map_quarry_targeting` and `map_expeditions` (the `delta`-driven targeting and
	# awaiting-expedition pulses); every other frame is byte-identical with or without it.
	#
	# Nothing is erased by freezing at phase 0, and that was checked against the draw code before it
	# was taken, not assumed: both pulses are the `0.5 + 0.5 * sin(t)` idiom, so t = 0 is the MIDPOINT
	# (0.5) rather than zero amplitude — the awaiting ring draws at 1.46x radius / 0.65 alpha and the
	# quarry glow at 0.60x / 0.675 — and the river's phase is a UV OFFSET whose coverage alpha comes
	# from a purely geometric `smoothstep`, so the channel, banks and taper are unaffected. The
	# targeting frame's eligibility test (which herds are valid quarries) is pure distance and never
	# touched the pulse at all. `_settle` waits on `process_frame`, which still fires at time_scale 0.
	Engine.time_scale = 0.0
	_pin_canvas(get_window())
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

	# State "riverine split" — the terrain-aware riverine_delta food glyph. Two riverine_delta food
	# sites on different terrains in one frame: the LEFT marker sits on an open navigable river (🐟),
	# the RIGHT marker on a dry alluvial-plain floodplain (🎋). Proof that FoodIcons.for_site splits
	# fish↔reeds off the terrain MapView stamps onto each site (so the map marker + HUD row can't disagree).
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_riverine_split())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_riverine_split")

	# State "site sprites" — the FOOD-SITE SPRITE ROSTER: every bundled site icon in one frame,
	# including the hunted-site deer and the unknown-module sprig. Judge swapped/clipped art here.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_site_sprites())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_site_sprites")

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

	# State J-sprites — the FAUNA SPRITE ROSTER: one herd per bundled-art species, each on its own
	# hex, so every `FaunaSprites` PNG is judged at true marker size in one frame (right species, no
	# clipping, no key fringe). Every HERD_SPECIES key now has art, so this frame is the coverage
	# check that used to be spread across whichever fixtures happened to name a species.
	_map.display_snapshot(_snapshot_fauna_sprites())
	_map._fit_map_to_view()
	await _settle()
	await _save("map_fauna_sprites")

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

	# State M2 — QUARRY targeting: the party compose sheet asks for a herd, and the map glows the
	# VALID ones. A hunting party is for game the band cannot work from home, so only a herd strictly
	# beyond the band's `hunt_reach` qualifies — carried on the targeting info as `min_distance`, the
	# render-side mirror of `TargetingController.is_expedition_quarry`. Both herds here are huntable and visible;
	# ONLY the far one may wear the pulsing ring. A ring on the near herd would promise a target the
	# pick refuses.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_quarry_targeting())
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	_map.set_targeting({
		"active": true, "command": "quarry", "need": "herd",
		"origin_x": BAND_X, "origin_y": BAND_Y,
		"min_distance": QUARRY_HUNT_REACH, "context_label": "Band 1",
	})
	await _settle()
	await _save("map_quarry_targeting")
	_map.set_targeting({})

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

	# State "pasture" — THE GRAZE DISTRIBUTION (Grazing Phase 2a). The whole point of the phase is to
	# LOOK at where the pasture is before Phase 2b makes every herd's carrying capacity a function of
	# it. An earthlike-shaped map (ocean, an alluvial-plain interior — the tag solver's fallback, which
	# really does dominate — a prairie steppe, a desert, tundra, glacier and lava) painted by the
	# `pasture` overlay channel, so the three questions are answerable in one frame:
	#   * does prairie/steppe read as the RICHEST pasture?
	#   * is the alluvial plain visibly dominant?
	#   * are glacier / lava / water visibly distinct from merely-POOR ground?
	# It also carries a MIXED WOODLAND block, which a live earthlike map does NOT (the biome palette
	# thins forest out entirely — tracked separately): the forest-is-poor-pasture inversion, the whole
	# reason the two-stock split exists, is otherwise unobservable, so it is staged here deliberately.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(false)   # overlay mode paints solid per-hex colors; textures would fight it
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	await _set_canvas(PASTURE_WINDOW_SIZE)   # match the grid's aspect — see PASTURE_WINDOW_SIZE
	await _settle()
	_map.display_snapshot(_snapshot_pasture())
	# display_snapshot re-ingests the channels and clears the active overlay (the Inspector re-applies
	# the player's selection every snapshot), so the channel is selected AFTER the snapshot lands.
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_pasture")
	# The legend's numbers are the other half of the readout (min/avg/max + how much ground is dead),
	# and this harness has no HUD to draw them into — print them so they can be checked against the map.
	print("map_preview: pasture legend = ", _map._legend_for_current_view())

	# State "pasture herd range" — the herd's grazing RANGE ring OVER the pasture overlay (Grazing Phase
	# 2b-iii). The same earthlike frame with a big-game herd parked mid-prairie (its range-1 disc of 7
	# tiles sits entirely on the rich prairie steppe) and SELECTED: the warm graze-amber ring must read
	# clearly over the straw/green pasture ramp — that ring-over-graze is the whole point (the player sees
	# the exact ground the sim derives the herd's carrying capacity from).
	_map.display_snapshot(_snapshot_pasture_herd())
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map.selected_herd_id = PASTURE_HERD_ID
	_map._fit_map_to_view()
	await _settle()
	await _save("map_pasture_herd_range")
	_map.selected_herd_id = ""

	# State "pasture pen footprint" — the SAME frame, but the herd is CORRALLED with pen_radius 1 (Grazing
	# 2d-γ). A penned herd draws no roam-range ring; instead its fenced FOOTPRINT (the 7-tile hex disk of
	# radius 1 around the pen anchor) reads in the distinct enclosure-GREEN tint — deliberately NOT the gold
	# of the roam-range above, so a fenced footprint is unmistakably a different thing. Read it against
	# map_pasture_herd_range.png: same herd tile, green disc instead of gold.
	_map.display_snapshot(_snapshot_pasture_pen())
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map.selected_herd_id = PASTURE_HERD_ID
	_map._fit_map_to_view()
	await _settle()
	await _save("map_pasture_pen_footprint")
	_map.selected_herd_id = ""

	# State "forage" — THE HUMAN-FOOD DISTRIBUTION, the twin of "pasture". Same earthlike shape, the
	# OTHER food web: it must look VISIBLY DIFFERENT from the pasture frame (that divergence is the whole
	# point of the two-table split). Read against map_pasture.png:
	#   * forest + river valleys read RICH here where prairie reads richest on pasture (the inversion);
	#   * the coastal shelf LIGHTS UP as a fishing ground where pasture paints it dead water;
	#   * only deep ocean / glacier / lava are barren, and a barren forage tile can still be good land.
	await _set_canvas(PASTURE_WINDOW_SIZE)   # same aspect as pasture — the two are meant to be compared
	await _settle()
	_map.display_snapshot(_snapshot_forage())
	_map.set_overlay_channel(FORAGE_OVERLAY_KEY)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_forage")
	print("map_preview: forage legend = ", _map._legend_for_current_view())

	# States "hunt_danger" / "threat" (Predators Phase 0) — the two derived-danger overlays, projected
	# client-side from herd positions. Three herds on the earthlike shape: a fierce MAMMOTH (attack ×
	# ferocity high, attack × aggression 0), an aggressive DIRE WOLF (both high), a HARMLESS deer (both
	# 0). On hunt_danger the mammoth + wolf hexes glow orange and the deer stays grid-colored; on threat
	# ONLY the wolf's hex glows red (the mammoth is deadly to hunt yet no threat — strength ≠ danger).
	# Both ride the generic lerp path + generic scalar legend, printed here since this harness has no HUD.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	await _set_canvas(PASTURE_WINDOW_SIZE)
	await _settle()
	_map.display_snapshot(_snapshot_danger())
	_map.set_overlay_channel(HUNT_DANGER_OVERLAY_KEY)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_hunt_danger")
	print("map_preview: hunt_danger legend = ", _map._legend_for_current_view())
	# The threat channel — staged aggressive (a Phase-0 live map would omit it, which is correct).
	_map.set_overlay_channel(THREAT_OVERLAY_KEY)
	await _settle()
	await _save("map_threat")
	print("map_preview: threat legend = ", _map._legend_for_current_view())

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
	# The NOTCH case, same zoom: a chain HEAD whose tributary hands over at its BOTTOM vertex (corner 1) and
	# whose single channel exit is the ADJACENT SW side (dir 2) — both flanking the same corner. The old
	# centre-hub routing drew inflow_corner → centre → exit_mid, which doubled back into a visible NOTCH /
	# inverted-V at the corner (the tributary looked like it hooked into the wrong corner). Read for: the
	# tributary flowing SMOOTHLY from its edge ribbon into the channel and straight out the SW exit, with NO
	# notch/V at the bottom, and the slim bank following the new flow line.
	_map.display_snapshot(_snapshot_rivers_notch(GRID_W, GRID_H))
	_map._fit_map_to_view()
	# Zoom in so the head hex fills the crop — the notch is a small feature at the corner and reads clearly
	# only with plenty of pixels on the channel.
	_map._apply_zoom(NOTCH_ZOOM_IN, get_viewport().get_visible_rect().size * 0.5)
	await _settle()
	if _river_notch_head.x >= 0:
		_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
			- _map._hex_center(_river_notch_head.x, _river_notch_head.y, _map.last_hex_radius, _map.last_origin)
		_map.queue_redraw()
		await _settle()
		var notch_center: Vector2 = _map._hex_center(
			_river_notch_head.x, _river_notch_head.y, _map.last_hex_radius, _map.last_origin)
		await _save_crop_px("map_rivers_notch", notch_center, RIVER_JOIN_CROP_RADII * _map.last_hex_radius)
	else:
		push_warning("map_preview: no notch head placed — notch frame skipped")
	# The ALONGSIDE-LAKE case (@21,61), same zoom: a one-hex inland_sea ringed by navigable hexes whose
	# river_channel exits all point along their own chain / out to the eastern sea — NONE into the lake. The
	# old shore pass dropped the coast on ANY navigable↔water adjacency, so it ate the lake's beach/foam ring
	# on those three edges (a hard seam now that the bank renders the valley terrain). The mouth test must now
	# draw the lake its FULL ring INCLUDING the navigable-adjacent edges. Read for: an unbroken beach/foam ring
	# around the whole lake, and the navigable valley getting a normal coast against it.
	_map.display_snapshot(_snapshot_rivers_lake_alongside(GRID_W, GRID_H))
	_map._fit_map_to_view()
	_map._apply_zoom(NOTCH_ZOOM_IN, get_viewport().get_visible_rect().size * 0.5)
	await _settle()
	if _river_lake_hex.x >= 0:
		_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
			- _map._hex_center(_river_lake_hex.x, _river_lake_hex.y, _map.last_hex_radius, _map.last_origin)
		_map.queue_redraw()
		await _settle()
		var lake_center: Vector2 = _map._hex_center(
			_river_lake_hex.x, _river_lake_hex.y, _map.last_hex_radius, _map.last_origin)
		await _save_crop_px("map_rivers_lake_alongside", lake_center, RIVER_JOIN_CROP_RADII * _map.last_hex_radius)
	else:
		push_warning("map_preview: no alongside lake placed — lake frame skipped")
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

	# === THE ANNOTATION STATES ===================================================================
	# Trade overlay / crisis annotations / terrain highlight / routes — the four overlays that had no
	# fixture at all, so no refactor of them could be pixel-checked. They run LAST and each CLEARS its
	# own state afterwards, so a leak here can only ever show up in the annotation frames themselves.
	# They restore the default canvas (the river states above left the pasture aspect pinned) because
	# their fixtures are authored against the GRID_W×GRID_H grid the earlier states use.
	# **They prove UNCHANGED, not CORRECT** — see the comment on TRADE_SELECTED_ENTITY.
	await _set_canvas(DEFAULT_CANVAS_SIZE)
	await _settle()

	# State "trade overlay" — the Trade tab's diffusion overlay, pushed exactly the way TradePanel
	# pushes it (update_trade_overlay → set_trade_overlay_enabled → set_trade_overlay_selection, all
	# three reached BY NAME through has_method/call). Three links fan the draw's branches across one
	# frame: the SELECTED caravan (green, widened — the branch an unselected-only fixture would leave
	# unproven), a busy open link (widest amber), and a thin closed one whose leak is imminent (the red
	# midpoint dot). A fourth link addresses tiles that don't exist, so the skip guard is exercised too.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_trade_overlay())
	_map.update_trade_overlay(_trade_links(), true)
	_map.set_trade_overlay_enabled(true)
	_map.set_trade_overlay_selection(TRADE_SELECTED_ENTITY)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_trade_overlay")
	# Clear it: the overlay is only re-ingested by a snapshot that CARRIES `trade_links`, so without
	# this the links would persist into every following state.
	_map.set_trade_overlay_selection(-1)
	_map.update_trade_overlay([], false)
	_map.set_trade_overlay_enabled(false)

	# State "crisis annotations" — the Crisis overlay's map annotations, which draw ONLY while the
	# `crisis` channel is the active one. All four shapes the draw can produce in one frame: a
	# multi-hop path from the PackedInt32Array form (critical), a multi-hop path from the
	# Array-of-[col,row] form (warn), a single-tile marker (safe — halo disc + core disc instead of a
	# polyline), and a single-tile marker with an unknown severity (the CRISIS_COLOR fallback) and no
	# label. The channel is selected AFTER the snapshot: display_snapshot clears the active overlay.
	_map.set_fow_enabled(false)
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_crisis_annotations())
	_map.set_overlay_channel(CRISIS_CHANNEL_KEY)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_crisis_annotations")
	_map.set_overlay_channel("")   # back to plain terrain for the states after this one

	# State "terrain highlight" — the Terrain tab's "highlight every tile of this type" tool, run on
	# the four-band biome map. The MATCHED band (prairie) wears the magenta fill + outline while the
	# three UNMATCHED bands render untouched, so both paths of the per-tile test are in one frame.
	# The highlight ignores Fog of War by design (it doubles as a worldgen debugging tool).
	_map.set_fow_enabled(false)
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_biomes())
	_map.set_terrain_highlight(TERRAIN_HIGHLIGHT_TARGET_ID)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_terrain_highlight")
	_map.set_terrain_highlight(TERRAIN_HIGHLIGHT_OFF)

	# State "routes" — order paths, drawn as per-faction polylines from the snapshot's `orders`. Three
	# multi-hop routes that turn (a straight two-point line would never exercise the segment loop),
	# colored through MapView.faction_colors' INT key, its STRING key, and an unknown faction (the
	# amber default) — plus a one-waypoint order the draw must bail on.
	_map.set_fow_enabled(false)
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_routes())
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_routes")

	get_tree().quit()

func _settle() -> void:
	await _ensure_canvas()
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

## Hold the window at the pinned canvas. Deliberately does NOT touch content_scale_size /
## content_scale_factor (blend_probe does, to get a 1:1 canvas): project.godot stretches
## `canvas_items` with an `expand` aspect, so pinning those here would re-project EVERY frame this
## harness renders — a mass pixel change, not a race fix. The race is a window mode/size problem.
func _pin_canvas(win: Window) -> void:
	win.mode = Window.MODE_WINDOWED
	win.size = _canvas_size

## Switch the pinned canvas for the states that need a different aspect (see PASTURE_WINDOW_SIZE) and
## wait for the WM to honour it, so the state renders at the size it asked for rather than whatever
## the previous state left behind.
func _set_canvas(size: Vector2i) -> void:
	_canvas_size = size
	await _ensure_canvas()

## Hold the window at the pinned canvas, and WAIT for the WM to honour it, before anything is measured
## or captured. project.godot opens MAXIMIZED and macOS applies (and RE-applies) that asynchronously,
## many frames in — so the bare `get_window().size = …` + two process_frames this harness used to do in
## _ready is a RACE, and it does not stay won. Measured on a clean run: 33 of 41 saved frames came out
## at the monitor's 3840x1050 rather than the pinned 1000x800, and the four earliest states flipped
## between the two from run to run, which is what made this frame set unusable as a pixel reference.
## Hence: check the WINDOW, re-pin, and give the WM frames to comply.
func _ensure_canvas() -> void:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		if get_window().size == _canvas_size and get_window().mode == Window.MODE_WINDOWED:
			return
		_pin_canvas(get_window())
		await get_tree().process_frame

## The viewport image, GUARANTEED to be the pinned canvas (or an integer HiDPI multiple of it). The
## WM's deferred maximize can resize the render target between a settle and a capture, and a raw
## get_image() then hands back a monitor-sized frame: the pixel-diff dies on a size mismatch and every
## fractional crop lands somewhere else on the map. Re-pin and re-draw until the geometry is the
## canvas's, then give up loudly rather than silently saving a bad frame.
##
## THE GUARD IS DERIVED FROM WHAT THIS HARNESS ACTUALLY SEES, not copied from blend_probe: with
## content_scale_* deliberately unpinned, the captured image matches the WINDOW size (measured 1:1 on
## every frame of a clean run), while the viewport's logical rect is the content-scale `expand`
## projection of it and matches NEITHER (win 1000x800 -> vprect 1920x1536). So this compares against
## the window-sized canvas; the integer-multiple form keeps it satisfiable on a HiDPI display, where
## it reduces to plain equality at 1x. Testing the viewport rect here could never be satisfied.
func _capture() -> Image:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			push_warning("map_preview: null image (dummy renderer?) — run without --headless")
			return null
		var w := image.get_width()
		var h := image.get_height()
		if w % _canvas_size.x == 0 and h % _canvas_size.y == 0 and w / _canvas_size.x == h / _canvas_size.y:
			return image
		_pin_canvas(get_window())
		await get_tree().process_frame
		RenderingServer.force_draw()
		await get_tree().process_frame
	push_error("map_preview: viewport never came back to the pinned %s canvas" % _canvas_size)
	return null

func _save(name: String) -> void:
	var image: Image = await _capture()
	if image == null:
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("map_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("map_preview: saved ", name, ".png")

## Save a cropped region of the current frame (fractions of the viewport, 0..1) — used for coast close-ups.
func _save_crop(name: String, fx0: float, fy0: float, fx1: float, fy1: float) -> void:
	var image: Image = await _capture()
	if image == null:
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
	var image: Image = await _capture()
	if image == null:
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

## The terrain of the pasture state — an earthlike-SHAPED map, not a band strip: an ocean on the west
## with a shelf, an ALLUVIAL-PLAIN interior (the fallback biome that really does carry most of a live
## map's graze), a prairie steppe, a desert, a woodland block (staged — a live map has no forest), a
## tundra/glacier north edge, an alpine spine and a lava scar. Returns terrain ids per tile, row-major.
func _pasture_terrain() -> Array:
	var ids: Array = []
	ids.resize(PASTURE_GRID_W * PASTURE_GRID_H)
	for row in PASTURE_GRID_H:
		for col in PASTURE_GRID_W:
			var id := 10                                   # alluvial_plain — the default ground
			if col < 3:
				id = 0                                     # deep_ocean
			elif col == 3:
				id = 1                                     # continental_shelf
			elif row < 2:
				id = 22 if col > 8 else 20                 # glacier cap over a tundra fringe
			elif row < 4:
				id = 20                                    # tundra
			elif row >= 5 and row <= 10 and col >= 6 and col <= 12:
				id = 11                                    # prairie_steppe — the reference pasture
			elif row >= 12 and col >= 5 and col <= 11:
				id = 15                                    # hot_desert_erg — marginal (8), NOT dead
			elif row >= 4 and row <= 9 and col >= 16 and col <= 21:
				id = 12                                    # mixed_woodland — the staged forest
			elif col >= 22 and row >= 3 and row <= 13:
				id = 26                                    # alpine_mountain spine
			elif row >= 14 and col >= 16 and col <= 19:
				id = 30                                    # basaltic_lava_field — dead ground
			ids[row * PASTURE_GRID_W + col] = id
	return ids

## The pasture snapshot: terrain + the Water tag mask + per-tile graze (`tiles`) + the `pasture`
## overlay channel. The channel mirrors what the native decoder publishes (raw = capacity, normalized
## = capacity ÷ the map's RICHEST pasture — a max scale, not a min-max stretch, because 0 here is a
## real reading: no pasture at all).
func _snapshot_pasture() -> Dictionary:
	var ids := _pasture_terrain()
	var total := PASTURE_GRID_W * PASTURE_GRID_H
	var tags: Array = []
	tags.resize(total)
	var raw := PackedFloat32Array()
	raw.resize(total)
	var tiles: Array = []
	var max_capacity := 0.0
	for i in total:
		var id := int(ids[i])
		var capacity := float(PASTURE_CAPACITY_BY_TERRAIN.get(id, 0.0))
		max_capacity = maxf(max_capacity, capacity)
		tags[i] = (PASTURE_WATER_TAG if PASTURE_WATER_IDS.has(id) else 0)
		raw[i] = capacity
		tiles.append({
			"entity": i,
			"x": i % PASTURE_GRID_W,
			"y": i / PASTURE_GRID_W,
			"terrain": id,
			# Phase 2a: every patch stands full, hence Thriving. A biome with no pasture reports no
			# capacity and no phase at all — an ABSENT reading, never a zero-but-healthy one.
			"graze_capacity": capacity,
			"graze_biomass": capacity,
			"graze_ecology_phase": ("thriving" if capacity > 0.0 else ""),
		})
	var normalized := PackedFloat32Array()
	normalized.resize(total)
	for i in total:
		normalized[i] = (raw[i] / max_capacity if max_capacity > 0.0 else 0.0)
	return {
		"grid": {"width": PASTURE_GRID_W, "height": PASTURE_GRID_H, "wrap_horizontal": false},
		"overlays": {
			"terrain": ids,
			"terrain_tags": tags,
			"channels": {
				PASTURE_OVERLAY_KEY: {
					"label": "Pasture (Graze Capacity)",
					"description": "Graze capacity by biome.",
					"normalized": normalized,
					"raw": raw,
				},
			},
			"channel_order": PackedStringArray([PASTURE_OVERLAY_KEY]),
		},
		"tiles": tiles,
		"populations": [],
		"herds": [],
	}

## The pasture snapshot with a big-game herd parked mid-prairie and selectable — for the range-ring
## state. Reuses `_snapshot_pasture()` verbatim (so the overlay/legend are identical) and only injects
## the herd into the empty `herds` array; MapView draws its grazing-range ring for the selected herd.
func _snapshot_pasture_herd() -> Dictionary:
	var snapshot := _snapshot_pasture()
	snapshot["herds"] = [{
		"id": PASTURE_HERD_ID,
		"label": "Red Deer (%s)" % PASTURE_HERD_ID,
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"x": PASTURE_HERD_COL,
		"y": PASTURE_HERD_ROW,
		"biomass": 1480.0,
		"carrying_capacity": 2150.0,
		"graze_range_radius": PASTURE_HERD_RANGE_RADIUS,
	}]
	return snapshot

## The pasture snapshot with a CORRALLED herd (pen_radius 1) at the same tile — for the pen-footprint
## state. Same herd position as `_snapshot_pasture_herd()`, but penned: MapView draws the fenced
## footprint disc (enclosure green) instead of the roam-range ring.
func _snapshot_pasture_pen() -> Dictionary:
	var snapshot := _snapshot_pasture()
	snapshot["herds"] = [{
		"id": PASTURE_HERD_ID,
		"label": "Red Deer (%s)" % PASTURE_HERD_ID,
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"x": PASTURE_HERD_COL,
		"y": PASTURE_HERD_ROW,
		"biomass": 1480.0,
		"carrying_capacity": 2150.0,
		"graze_range_radius": PASTURE_HERD_RANGE_RADIUS,
		"corralled": true,
		"corral_progress": 1.0,
		"pen_radius": 1,
		"pen_footprint_tiles": 7,
		"pen_pasture_fraction": 1.0,
		"pen_fed_fraction": 1.0,
	}]
	return snapshot

## The forage snapshot: the SAME earthlike terrain as pasture, painted by the `forage` overlay channel
## off the HUMAN-food table. Each tile carries `forage_capacity` (which MapView caches into `tile_forage`
## for the legend) + the pre-normalized channel (raw = capacity, normalized = capacity ÷ the map's
## RICHEST forage — a max scale, mirroring the native decoder). Water is NOT an off-category here:
## continental_shelf carries 130 forage and rides the ramp (fishing), the divergence from pasture.
func _snapshot_forage() -> Dictionary:
	var ids := _pasture_terrain()   # reuse the pasture SHAPE so the two frames compare tile-for-tile
	var total := PASTURE_GRID_W * PASTURE_GRID_H
	var tags: Array = []
	tags.resize(total)
	var raw := PackedFloat32Array()
	raw.resize(total)
	var tiles: Array = []
	var max_capacity := 0.0
	for i in total:
		var id := int(ids[i])
		var capacity := float(FORAGE_CAPACITY_BY_TERRAIN.get(id, 0.0))
		max_capacity = maxf(max_capacity, capacity)
		tags[i] = (PASTURE_WATER_TAG if PASTURE_WATER_IDS.has(id) else 0)
		raw[i] = capacity
		tiles.append({
			"entity": i,
			"x": i % PASTURE_GRID_W,
			"y": i / PASTURE_GRID_W,
			"terrain": id,
			"forage_capacity": capacity,
		})
	var normalized := PackedFloat32Array()
	normalized.resize(total)
	for i in total:
		normalized[i] = (raw[i] / max_capacity if max_capacity > 0.0 else 0.0)
	return {
		"grid": {"width": PASTURE_GRID_W, "height": PASTURE_GRID_H, "wrap_horizontal": false},
		"overlays": {
			"terrain": ids,
			"terrain_tags": tags,
			"channels": {
				FORAGE_OVERLAY_KEY: {
					"label": "Forage (Human Food Capacity)",
					"description": "Human-food capacity by biome.",
					"normalized": normalized,
					"raw": raw,
				},
			},
			"channel_order": PackedStringArray([FORAGE_OVERLAY_KEY]),
		},
		"tiles": tiles,
		"populations": [],
		"herds": [],
	}

## The DANGER snapshot (Predators Phase 0). Danger is DERIVED per-ENTITY, so the native decoder projects
## TWO channels onto tiles from herd positions: hunt_danger = attack × ferocity, threat = attack ×
## aggression. This hand-built harness snapshot reproduces both projections (a zero-init grid,
## `max(existing, value)` at each herd's tile, normalized against that channel's own map-max). It reuses
## the pasture terrain SHAPE, then drops three herds so BOTH channels light: a fierce MAMMOTH (attack 8,
## ferocity 0.9, aggression 0 → high hunt_danger, zero threat), an aggressive DIRE WOLF (attack 4,
## ferocity 0.7, aggression 0.9 → both channels), and a HARMLESS deer (all zero → colors neither).
const HUNT_DANGER_OVERLAY_KEY := "hunt_danger"  # mirrors MapView.HUNT_DANGER_OVERLAY_KEY / the channel key
const THREAT_OVERLAY_KEY := "threat"            # mirrors MapView.THREAT_OVERLAY_KEY / the channel key
const DANGER_MAMMOTH_COL := 9
const DANGER_MAMMOTH_ROW := 7
const DANGER_WOLF_COL := 16
const DANGER_WOLF_ROW := 9
const DANGER_DEER_COL := 21
const DANGER_DEER_ROW := 12
func _snapshot_danger() -> Dictionary:
	var ids := _pasture_terrain()
	var total := PASTURE_GRID_W * PASTURE_GRID_H
	var herds := [
		{
			"id": "game_mammoth_02", "label": "Woolly Mammoth (game_mammoth_02)",
			"species": "Woolly Mammoth", "size_class": "big", "huntable": true,
			"ecology_phase": "thriving", "x": DANGER_MAMMOTH_COL, "y": DANGER_MAMMOTH_ROW,
			"biomass": 900.0, "attack": 8.0, "ferocity": 0.9, "aggression": 0.0,
		},
		{
			"id": "game_direwolf_05", "label": "Dire Wolf (game_direwolf_05)",
			"species": "Dire Wolf", "size_class": "medium", "huntable": true,
			"ecology_phase": "thriving", "x": DANGER_WOLF_COL, "y": DANGER_WOLF_ROW,
			"biomass": 240.0, "attack": 4.0, "ferocity": 0.7, "aggression": 0.9,
		},
		{
			"id": "game_deer_09", "label": "Red Deer (game_deer_09)",
			"species": "Red Deer", "size_class": "big", "huntable": true,
			"ecology_phase": "thriving", "x": DANGER_DEER_COL, "y": DANGER_DEER_ROW,
			"biomass": 820.0, "attack": 0.0, "ferocity": 0.0, "aggression": 0.0,
		},
	]
	var hunt_raw := PackedFloat32Array()
	hunt_raw.resize(total)
	var threat_raw := PackedFloat32Array()
	threat_raw.resize(total)
	var hunt_max := 0.0
	var threat_max := 0.0
	for herd in herds:
		var attack := float(herd.get("attack", 0.0))
		var hunt := attack * float(herd.get("ferocity", 0.0))
		var threat := attack * float(herd.get("aggression", 0.0))
		var idx := int(herd["y"]) * PASTURE_GRID_W + int(herd["x"])
		if idx < 0 or idx >= total:
			continue
		hunt_raw[idx] = maxf(hunt_raw[idx], hunt)
		threat_raw[idx] = maxf(threat_raw[idx], threat)
		hunt_max = maxf(hunt_max, hunt_raw[idx])
		threat_max = maxf(threat_max, threat_raw[idx])
	var tiles: Array = []
	for i in total:
		tiles.append({
			"entity": i, "x": i % PASTURE_GRID_W, "y": i / PASTURE_GRID_W, "terrain": int(ids[i]),
		})
	var channels := {}
	var channel_order := PackedStringArray()
	if hunt_max > 0.0:
		channels[HUNT_DANGER_OVERLAY_KEY] = {
			"label": "Hunt danger", "description": "How costly the wildlife here is to hunt.",
			"normalized": _danger_normalized(hunt_raw, hunt_max), "raw": hunt_raw,
		}
		channel_order.append(HUNT_DANGER_OVERLAY_KEY)
	if threat_max > 0.0:
		channels[THREAT_OVERLAY_KEY] = {
			"label": "Threat", "description": "How much the wildlife here menaces you unprovoked.",
			"normalized": _danger_normalized(threat_raw, threat_max), "raw": threat_raw,
		}
		channel_order.append(THREAT_OVERLAY_KEY)
	return {
		"grid": {"width": PASTURE_GRID_W, "height": PASTURE_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": ids, "channels": channels, "channel_order": channel_order},
		"tiles": tiles,
		"populations": [],
		"herds": herds,
	}

func _danger_normalized(raw: PackedFloat32Array, channel_max: float) -> PackedFloat32Array:
	var normalized := PackedFloat32Array()
	normalized.resize(raw.size())
	for i in raw.size():
		normalized[i] = (raw[i] / channel_max if channel_max > 0.0 else 0.0)
	return normalized

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
		# hunt_reach = work_range + the hunt leash (the sim ships 5 = 2 + 3), so the selected-band
		# HUNT range border draws at R=5 and the deer herd at (13,6) sits right on it.
		"hunt_reach": work_range + 3,
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
	# A THIRD pen, starving, whose species has BUNDLED SPRITE ART (boar) — the aurochs above is an
	# emoji species, so without this the frame never proves the distress ring/badge still reads over a
	# sprite marker (the sprite is drawn untinted, exactly like the emoji, so the geometry is the whole
	# distress signal on both paths).
	var starving_sprite := {
		"id": "game_boar_05", "label": "Wild Boar (game_boar_05)",
		"x": 7, "y": 7, "biomass": 260.0, "huntable": true,
		"corralled": true, "pen_fed_fraction": 0.3,
	}
	return _base_snapshot(_band([], 2, 2), [fed, starving, starving_sprite])

## Every species in `FoodIcons.HERD_SPECIES`, one herd per ALIAS GROUP, laid out on its own hex so
## each `FaunaSprites` marker can be judged at TRUE marker size. This is the roster frame: it is the
## only place the whole bundled-art set is visible at once, so a swapped/clipped/fringed sprite shows
## up here and nowhere else. One entry per group is enough — aliases resolve to the same PNG.
const FAUNA_SPRITE_ROSTER := [
	["game_rabbit_01", "Rabbit Warren"],
	["game_deer_01", "Red Deer"],
	["game_boar_01", "Wild Boar"],
	["game_mammoth_01", "Thunder Mammoth"],
	["game_aurochs_01", "Aurochs"],
	["game_cattle_01", "Cattle"],
	["game_goat_01", "Wild Goat"],
	["game_horse_01", "Wild Horse"],
	["game_sheep_01", "Sheep"],
	["game_fowl_01", "Jungle Fowl"],
]
## The roster is laid out as ONE row: MapView is cover-fit, so on this wide preview window only a
## few middle rows are on screen and a second roster row is cropped away unseen.
const FAUNA_ROSTER_COLUMNS := 10
## A middle row (well inside the cover-fit crop) and a leading margin off the map border.
const FAUNA_ROSTER_ORIGIN := Vector2i(3, 5)
## Hexes between roster entries — one apart, so ten fit across GRID_W without markers colliding.
const FAUNA_ROSTER_SPACING := 1

func _snapshot_fauna_sprites() -> Dictionary:
	var herds: Array = []
	for i in FAUNA_SPRITE_ROSTER.size():
		var entry: Array = FAUNA_SPRITE_ROSTER[i]
		var col := FAUNA_ROSTER_ORIGIN.x + (i % FAUNA_ROSTER_COLUMNS) * FAUNA_ROSTER_SPACING
		var row := FAUNA_ROSTER_ORIGIN.y + (i / FAUNA_ROSTER_COLUMNS) * FAUNA_ROSTER_SPACING
		herds.append({
			"id": entry[0],
			"label": "%s (%s)" % [entry[1], entry[0]],
			"x": col, "y": row, "biomass": 400.0, "huntable": true,
		})
	return _base_snapshot(_band([], 2, 2), herds)

func _snapshot_work() -> Dictionary:
	# Per-source yields annotate the worked tiles/herd on the map. The ⚠ overhunt flag is now the
	# sim-answered `overdraws` bool (policy-driven, false for Sustain), NOT `actual > sustainable`.
	# The DECOUPLING this proves: the SUSTAIN hunt has `actual 0.46 > sustainable 0.20` (a banked
	# whole animal cashed on this kill turn) yet `overdraws=false` → NO ⚠ (label reads +0.20, clean),
	# while the MARKET forage genuinely overdraws → `overdraws=true` → ⚠.
	var assignments := [
		# Policies drive the yield label's trailing policy glyph (♻ sustain / ⬆ surplus / 🪙 market /
		# 💀 eradicate) — two different ones here so the map read is verifiable in one frame.
		{"kind": "forage", "workers": 5, "target_x": FORAGE_A_X, "target_y": FORAGE_A_Y, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "overdraws": false},
		{"kind": "forage", "workers": 3, "target_x": 9, "target_y": 8, "policy": "market", "actual_yield": 0.27, "sustainable_yield": 0.20, "overdraws": true},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 13, "target_y": 6, "actual_yield": 0.46, "sustainable_yield": 0.20, "overdraws": false},
		{"kind": "warrior", "workers": 2},
	]
	# work_range 2 (forage green), scout radius 4 (azure) → three DISTINCT nested range borders in one
	# frame: green R2 innermost, azure R4, red hunt R5 outermost (the deer sits on the hunt border).
	return _base_snapshot(_band(assignments, 2, 4), [_deer_herd()])

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

## Two riverine_delta food sites on different terrains — an open navigable river (🐟) and a dry
## alluvial-plain floodplain (🎋) — so the terrain-aware FoodIcons.for_site split reads side by side.
func _snapshot_riverine_split() -> Dictionary:
	var terrain := _terrain_array()
	terrain[RIVERINE_SITE_Y * GRID_W + RIVERINE_FISH_X] = RIVERINE_NAV_TERRAIN_ID
	terrain[RIVERINE_SITE_Y * GRID_W + RIVERINE_REED_X] = RIVERINE_LAND_TERRAIN_ID
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [],
		"herds": [],
		"food_modules": [
			{"x": RIVERINE_FISH_X, "y": RIVERINE_SITE_Y, "module": "riverine_delta", "kind": "forage"},
			{"x": RIVERINE_REED_X, "y": RIVERINE_SITE_Y, "module": "riverine_delta", "kind": "forage"},
		],
	}

## The FOOD-SITE SPRITE ROSTER — one site per bundled-art key on its own hex, so the whole art set is
## judged at once for swapped/clipped/fringed sprites (the food twin of `map_fauna_sprites`). One row
## per band of keys because MapView is cover-fit and rows past the fit are cropped away unseen.
## Includes the two NON-module art keys — a hunted site (`kind = game_trail` → the fauna deer) and an
## unknown module (→ the `default` sprig) — since neither is reachable from `FoodIcons.ICONS`.
const SITE_ROSTER_MODULES := [
	"coastal_littoral", "savanna_grassland", "temperate_forest", "boreal_arctic",
	"montane_highland", "wetland_swamp", "semi_arid_scrub", "coastal_upwelling",
	"mixed_woodland",
]
const SITE_ROSTER_Y := 4                  # shared row so every sprite sits at the same height
const SITE_ROSTER_X0 := 2                 # first column of the roster row
const SITE_ROSTER_STEP := 1               # one hex between sites — no tile shares a slot
const SITE_ROSTER_HUNT_MODULE := "savanna_grassland"   # a hunted site; `kind` is what picks the deer
const SITE_ROSTER_UNKNOWN_MODULE := "berry_patch"      # not in ICONS → the `default` sprig

func _snapshot_site_sprites() -> Dictionary:
	var sites: Array = []
	var x := SITE_ROSTER_X0
	for module in SITE_ROSTER_MODULES:
		sites.append({"x": x, "y": SITE_ROSTER_Y, "module": module, "kind": "forage"})
		x += SITE_ROSTER_STEP
	sites.append({"x": x, "y": SITE_ROSTER_Y, "module": SITE_ROSTER_HUNT_MODULE, "kind": "game_trail"})
	x += SITE_ROSTER_STEP
	sites.append({"x": x, "y": SITE_ROSTER_Y, "module": SITE_ROSTER_UNKNOWN_MODULE, "kind": "forage"})
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [],
		"herds": [],
		"food_modules": sites,
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
		{"kind": "forage", "workers": 5, "target_x": cx + 1, "target_y": cy, "actual_yield": 0.48, "sustainable_yield": 0.48, "overdraws": false},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": cx + 2, "target_y": cy, "actual_yield": 0.46, "sustainable_yield": 0.20, "overdraws": false},
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

## Two huntable, visible herds straddling the band's hunt reach: the Roe Deer sits INSIDE it (a local
## hunt — no glow) and the Wild Boar well beyond (a party's job — glow). The frame is judged on the
## ring appearing on exactly one of them.
func _snapshot_quarry_targeting() -> Dictionary:
	return _base_snapshot(_band([], 2, 2), [
		{"id": "game_deer_79", "label": "Roe Deer (game_deer_79)",
			"x": BAND_X + QUARRY_NEAR_OFFSET, "y": BAND_Y, "biomass": 500.0, "huntable": true},
		{"id": "game_boar_04", "label": "Wild Boar (game_boar_04)",
			"x": BAND_X + QUARRY_FAR_OFFSET, "y": BAND_Y, "biomass": 800.0, "huntable": true},
	])

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
func _river_tiles(gw: int, terrain: Array, masks: Dictionary, inflow: Dictionary, channel: Dictionary) -> Array:
	var keys: Dictionary = {}
	for key: Vector2i in masks:
		keys[key] = true
	for key: Vector2i in inflow:
		keys[key] = true
	for key: Vector2i in channel:
		keys[key] = true
	var tiles: Array = []
	for key: Vector2i in keys:
		# underlying_terrain is the VALLEY biome the river cut — the wire field the client swaps in for a
		# navigable hex's base. Ordinary tiles carry their own terrain; a navigable hex (terrain 37) preserves
		# the underlying land (here the surrounding prairie), so its body reads as the valley with only a slim
		# bank skirt on the channel, not a whole hex of gravel.
		var tid: int = int(terrain[key.y * gw + key.x])
		var underlying: int = RIVER_LAND_ID if tid == RIVER_NAVIGABLE_ID else tid
		tiles.append({
			"entity": key.y * gw + key.x,
			"x": key.x,
			"y": key.y,
			"underlying_terrain": underlying,
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
		"tiles": _river_tiles(gw, terrain, masks, inflow, channel),
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
		"tiles": _river_tiles(gw, terrain, {}, {}, channel),
		"populations": [],
		"herds": [],
	}

## State "rivers notch" — the render-routing regression guard. A chain HEAD whose tributary hands over at
## its BOTTOM vertex (corner 1) and whose single channel exit is the ADJACENT SW side (dir 2). Both flank
## corner 1, so the retired centre-hub routing (inflow spur centre→corner + exit arm centre→edge-midpoint)
## drew inflow_corner → centre → exit_mid, doubling back into a NOTCH / inverted-V at the corner. The direct
## inflow-corner → exit-midpoint routing must draw ONE smooth tapered channel with no notch.
func _snapshot_rivers_notch(gw: int, gh: int) -> Dictionary:
	var terrain: Array = []
	terrain.resize(gw * gh)
	for y in range(gh):
		for x in range(gw):
			terrain[y * gw + x] = RIVER_OCEAN_ID if x >= gw - RIVER_OCEAN_COLS else RIVER_LAND_ID

	_river_notch_head = Vector2i(-1, -1)
	var head := Vector2i(clampi(int(gw * 0.42), 2, gw - RIVER_OCEAN_COLS - 2),
		clampi(int(gh * 0.42), 2, gh - 3))
	var exit_nb := _river_neighbor(head.x, head.y, RIVER_DIR_SW, gw, gh)  # the head's single exit side
	var se := _river_neighbor(head.x, head.y, RIVER_DIR_SE, gw, gh)       # the tributary ribbon's approach
	if exit_nb.x < 0 or se.x < 0 or exit_nb == se:
		return {  # grid too small for the topology — render riverless rather than a wrong frame
			"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
			"overlays": {"terrain": terrain},
			"tiles": [], "populations": [], "herds": [],
		}

	var masks: Dictionary = {}
	var inflow: Dictionary = {}
	var channel: Dictionary = {}

	terrain[head.y * gw + head.x] = RIVER_NAVIGABLE_ID
	# Tributary EDGE ribbon: rides the head's SE side (dir 1, which flanks corner 1) into the bottom vertex,
	# plus a hop further SE, so a Minor stream visibly arrives at the corner the channel hands over on.
	_river_set_edge(masks, head.x, head.y, RIVER_DIR_SE, se, RIVER_CLASS_MINOR)
	var se2 := _river_neighbor(se.x, se.y, RIVER_DIR_SE, gw, gh)
	if se2.x >= 0 and int(terrain[se2.y * gw + se2.x]) == RIVER_LAND_ID:
		_river_set_edge(masks, se.x, se.y, RIVER_DIR_SE, se2, RIVER_CLASS_MINOR)
	# Hand over at the head's BOTTOM vertex (corner 1) — the corner the SW exit side also flanks.
	inflow[head] = RIVER_CLASS_MINOR << (RIVER_CLASS_BITS * RIVER_TRIB_TERMINUS_CORNER)

	# The trunk leaves through the SW side and runs a short way WEST (away from the tributary) so the head
	# has exactly ONE exit. The crop shows only the head + immediate joins, so the tail need not reach sea.
	var path: Array[Vector2i] = [head, exit_nb]
	terrain[exit_nb.y * gw + exit_nb.x] = RIVER_NAVIGABLE_ID
	var cur := exit_nb
	for _i in range(2):
		var nb := _river_neighbor(cur.x, cur.y, RIVER_DIR_W, gw, gh)
		if nb.x < 0 or int(terrain[nb.y * gw + nb.x]) != RIVER_LAND_ID:
			break
		terrain[nb.y * gw + nb.x] = RIVER_NAVIGABLE_ID
		path.append(nb)
		cur = nb
	for i in range(path.size() - 1):
		_river_link_channel(channel, path[i], path[i + 1], gw, gh)

	_river_notch_head = head
	return {
		"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"tiles": _river_tiles(gw, terrain, masks, inflow, channel),
		"populations": [],
		"herds": [],
	}

## State "rivers lake alongside" — the @21,61 case for the shore-pass MOUTH test. A one-hex inland_sea
## ringed by three navigable hexes (its NW/NE/E neighbours) that form a chain RUNNING ALONGSIDE the lake and
## draining to the eastern sea — none of their river_channel exits point INTO the lake. The old shore pass
## dropped the coast on any navigable↔water adjacency, eating the lake's ring there; the mouth test must draw
## the full ring because none of these edges is a true mouth.
func _snapshot_rivers_lake_alongside(gw: int, gh: int) -> Dictionary:
	var terrain: Array = []
	terrain.resize(gw * gh)
	for y in range(gh):
		for x in range(gw):
			terrain[y * gw + x] = RIVER_OCEAN_ID if x >= gw - RIVER_OCEAN_COLS else RIVER_LAND_ID

	_river_lake_hex = Vector2i(-1, -1)
	var lake := Vector2i(clampi(int(gw * 0.44), 3, gw - RIVER_OCEAN_COLS - 3),
		clampi(int(gh * 0.5), 2, gh - 3))
	# The three navigable neighbours (consecutive ring positions NW→NE→E, so each pair shares an edge and the
	# three form a contiguous chain), each adjacent to the lake but chained only to EACH OTHER + downstream.
	var ring_dirs := [RIVER_DIR_NW, RIVER_DIR_NE, RIVER_DIR_E]
	var nav_cells: Array[Vector2i] = []
	for d: int in ring_dirs:
		var c := _river_neighbor(lake.x, lake.y, d, gw, gh)
		if c.x < 0 or c.x >= gw - RIVER_OCEAN_COLS:
			return {  # grid too small / too close to the sea for the topology — render riverless
				"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
				"overlays": {"terrain": terrain}, "tiles": [], "populations": [], "herds": [],
			}
		terrain[c.y * gw + c.x] = RIVER_NAVIGABLE_ID
		nav_cells.append(c)
	terrain[lake.y * gw + lake.x] = RIVER_LAKE_ID  # the inland_sea hex, stamped AFTER its ring

	var channel: Dictionary = {}
	# Chain the three navigable neighbours to each other (consecutive ring positions are edge-adjacent).
	for i in range(nav_cells.size() - 1):
		_river_link_channel(channel, nav_cells[i], nav_cells[i + 1], gw, gh)
	# Drain the east end (E of the lake) further EAST to the open sea, so the chain has a real mouth (which
	# STAYS excluded — the frame shows the alongside ring AND the open mouth at once).
	var cur: Vector2i = nav_cells[nav_cells.size() - 1]
	var path: Array[Vector2i] = [cur]
	var mouth_col: int = gw - RIVER_OCEAN_COLS - 1
	var guard := 0
	while cur.x < mouth_col and guard < gw:
		guard += 1
		var nb := _river_neighbor(cur.x, cur.y, RIVER_DIR_E, gw, gh)
		if nb.x < 0 or int(terrain[nb.y * gw + nb.x]) != RIVER_LAND_ID:
			break
		terrain[nb.y * gw + nb.x] = RIVER_NAVIGABLE_ID
		path.append(nb)
		cur = nb
	for i in range(path.size() - 1):
		_river_link_channel(channel, path[i], path[i + 1], gw, gh)
	_river_mouth_channel(channel, terrain, path[path.size() - 1], path[path.size() - 2] if path.size() > 1 else path[0], gw, gh)

	_river_lake_hex = lake
	return {
		"grid": {"width": gw, "height": gh, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"tiles": _river_tiles(gw, terrain, {}, {}, channel),
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

# --- The ANNOTATION fixtures (see the TRADE_* / CRISIS_* / ROUTE_* consts) -----------------------
# Written AFTER the code they cover: they encode CURRENT behaviour, so they prove "unchanged", not
# "correct".

## Row-major tile ENTITY id on the flat GRID_W×GRID_H backdrop. Trade links address their endpoints by
## entity and MapView resolves them through `tile_lookup`, which is built from `tiles[].entity` — so
## the trade fixture is the one flat-backdrop state that has to publish a `tiles` array.
func _tile_entity(x: int, y: int) -> int:
	return y * GRID_W + x

func _entity_tiles() -> Array:
	var tiles: Array = []
	for y in GRID_H:
		for x in GRID_W:
			tiles.append({"entity": _tile_entity(x, y), "x": x, "y": y, "terrain": TERRAIN_ID})
	return tiles

## One trade link in the shape the Trade tab hands to `update_trade_overlay`: endpoints as tile
## entities, a throughput (drives line width) and a knowledge sub-dict (openness drives opacity,
## leak_timer arms the red midpoint dot).
func _trade_link(entity: int, from_tile: Vector2i, to_tile: Vector2i,
		throughput: float, openness: float, leak_timer: int) -> Dictionary:
	return {
		"entity": entity,
		"from_tile": _tile_entity(from_tile.x, from_tile.y),
		"to_tile": _tile_entity(to_tile.x, to_tile.y),
		"throughput": throughput,
		"knowledge": {"openness": openness, "leak_timer": leak_timer},
	}

func _trade_links() -> Array:
	return [
		_trade_link(TRADE_SELECTED_ENTITY, TRADE_SELECTED_FROM, TRADE_SELECTED_TO,
			TRADE_BUSY_THROUGHPUT, TRADE_BUSY_OPENNESS, TRADE_LEAK_QUIET),
		_trade_link(TRADE_BUSY_ENTITY, TRADE_BUSY_FROM, TRADE_BUSY_TO,
			TRADE_BUSY_THROUGHPUT, TRADE_BUSY_OPENNESS, TRADE_LEAK_QUIET),
		_trade_link(TRADE_LEAKING_ENTITY, TRADE_LEAKING_FROM, TRADE_LEAKING_TO,
			TRADE_QUIET_THROUGHPUT, TRADE_QUIET_OPENNESS, TRADE_LEAK_IMMINENT),
		# Endpoints that are not in `tile_lookup` — the draw skips this link. Kept so the guard is
		# exercised by the reference frame rather than merely present in the source.
		{
			"entity": TRADE_SELECTED_ENTITY,
			"from_tile": TRADE_UNRESOLVED_TILE,
			"to_tile": TRADE_UNRESOLVED_TILE,
			"throughput": TRADE_BUSY_THROUGHPUT,
			"knowledge": {"openness": TRADE_BUSY_OPENNESS, "leak_timer": TRADE_LEAK_IMMINENT},
		},
	]

## The trade backdrop: the flat terrain every band state uses, PLUS the per-tile entity table the link
## endpoints resolve through. No units or herds — the frame is about the links.
func _snapshot_trade_overlay() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"tiles": _entity_tiles(),
		"populations": [],
		"herds": [],
	}

## The four annotation SHAPES the crisis draw can produce, in the order it walks them:
##   1. a multi-hop path in the PackedInt32Array (flattened col,row) form → polyline + head/tail discs
##   2. a multi-hop path in the Array-of-[col,row] form → the same geometry from the other wire shape
##   3. a SINGLE tile → halo disc + core disc, no polyline
##   4. a single tile with a severity that is not in CRISIS_SEVERITY_COLORS (→ the CRISIS_COLOR
##      fallback) and NO label (→ the label block is skipped)
func _crisis_annotations() -> Array:
	return [
		{"severity": "critical", "label": "Famine front", "path": PackedInt32Array(CRISIS_PATH_PACKED)},
		{"severity": "warn", "label": "Unrest march", "path": CRISIS_PATH_PAIRS},
		{"severity": "safe", "label": "Contained", "path": PackedInt32Array(CRISIS_POINT_SAFE)},
		{"severity": CRISIS_SEVERITY_UNKNOWN, "path": PackedInt32Array(CRISIS_POINT_UNKNOWN)},
	]

## The crisis backdrop: flat terrain under a west→east `crisis` pressure ramp (so the channel tint is
## not a flat wash and the annotations are read against a real overlay), plus the annotations
## themselves on the `overlays` payload — the same key the server publishes them under.
func _snapshot_crisis_annotations() -> Dictionary:
	var total := GRID_W * GRID_H
	var normalized := PackedFloat32Array()
	normalized.resize(total)
	var raw := PackedFloat32Array()
	raw.resize(total)
	for i in total:
		var pressure := float(i % GRID_W) / float(GRID_W - 1)
		normalized[i] = pressure
		raw[i] = pressure * CRISIS_RAW_SCALE
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {
			"terrain": _terrain_array(),
			"channels": {
				CRISIS_CHANNEL_KEY: {
					"label": "Crisis Pressure",
					"description": "Staged crisis pressure, west to east.",
					"normalized": normalized,
					"raw": raw,
				},
			},
			"channel_order": PackedStringArray([CRISIS_CHANNEL_KEY]),
			"crisis_annotations": _crisis_annotations(),
		},
		"populations": [],
		"herds": [],
	}

## An order in the shape `display_snapshot` reads into `routes`: a faction (looked up in
## MapView.faction_colors) and a path of [col, row] waypoints.
func _route_order(faction: Variant, path: Array) -> Dictionary:
	return {"faction": faction, "path": path}

## The routes backdrop: flat terrain, the resident band for scale, and four orders — three multi-hop
## routes covering the int/string/unknown faction-color lookups, and one one-waypoint order the draw
## must bail on.
func _snapshot_routes() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 0), [])
	snap["orders"] = [
		_route_order(ROUTE_PLAYER_FACTION, ROUTE_PLAYER_PATH),
		_route_order(ROUTE_RIVAL_FACTION, ROUTE_RIVAL_PATH),
		_route_order(ROUTE_UNKNOWN_FACTION, ROUTE_UNKNOWN_PATH),
		_route_order(ROUTE_PLAYER_FACTION, ROUTE_DEGENERATE_PATH),
	]
	return snap
