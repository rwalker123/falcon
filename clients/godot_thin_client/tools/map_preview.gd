extends Node2D

## Dev-only MapView preview harness (companion to tools/ui_preview.gd, which is HUD-only).
##
## Instances the real MapView, feeds a canned snapshot via display_snapshot(), selects a
## player band, renders each state, and saves a PNG to ui_preview_out/. Lets us visually
## verify the selected-band labor highlights (work-range ring / worked forage tiles /
## hunted-herd ring + link) without a server. Run windowed (NOT headless —
## the dummy renderer can't read back the viewport). FROM THE REPO ROOT:
##
##   scripts/preview.sh res://tools/map_preview.tscn
##
## then read ui_preview_out/map_*.png.

const MAP_VIEW := preload("res://src/scripts/MapView.gd")
# For its CanvasLayer roster only (`HUD_LAYER` / `INSPECTOR_LAYER` / `WORKBENCH_LAYER` /
# `LOADING_OVERLAY_LAYER`) — this harness stands up no `Main`, and the overlay picker's popover has
# to be asserted against the layers it must clear rather than against a number written twice.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
# `ui_preview`'s real-pointer probe, for the picker's button-swap claim: it owns the canvas→window
# conversion `push_input` needs, and a second copy of that arithmetic is a second thing to keep right.
const INPUT_PROBE := preload("res://tools/ui_preview/input_probe.gd")
## The whole test tree's one transcription of the sim's rung derivation — every fixture patch and
## herd states its `current_rung` through it, never as a literal (see `fixtures_rung.gd`).
const RUNG_FX := preload("res://tools/ui_preview/fixtures_rung.gd")
const OUT_DIR := "res://ui_preview_out"
const WARMUP_SETTLES := 3   # frames burned before the first capture (the window is still sizing)

# The canvas every state renders at unless it asks for another (see PASTURE_WINDOW_SIZE). MapView is
# cover-fit, so the canvas ASPECT decides what a frame shows — which is why this is pinned rather than
# left to whatever the WM hands us.
const DEFAULT_CANVAS_SIZE := Vector2i(1000, 800)
# How many frames _ensure_canvas will keep re-asserting the pinned canvas while it waits for the WM to
# honour it (a window mode/size change lands asynchronously, whatever mode the process booted in). Bounded
# so a WM that refuses to shrink the window fails loudly rather than hanging.
const CANVAS_PIN_MAX_FRAMES := 60

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — a harness that printed an error and still exited 0 was
## indistinguishable from a green one to anything but a human reading stdout.
const EXIT_OK := 0
const EXIT_FAILED := 1

const GRID_W := 16
const GRID_H := 12
const BAND_ENTITY := 9001
const BAND_X := 8
const BAND_Y := 6
const TERRAIN_ID := 5  # arbitrary land biome for a legible backdrop
const STACK_ENTITY_BASE := 9100   # co-located band entities are STACK_ENTITY_BASE + i
const TRAVEL_SEAM_BAND_X := 1      # band column near the left edge for the seam-crossing case
const TRAVEL_SEAM_TARGET_X := 14   # target near the right edge → short path wraps LEFT across seam
# The seam-selection guard's probe: a point at the middle of the frame, after a half-map-width pan
# has put the WRAPPED copies of the low columns there (see `_assert_selection_outline_wraps`).
const SEAM_PROBE_FRACTION := Vector2(0.5, 0.5)
const SEAM_PAN_MAP_WIDTHS := 0.5
# A 3px-wide outline around a fitted hex runs to a few thousand pixels; the floor only has to be
# clear of antialiasing noise, so it is set an order of magnitude under the measured count.
const SEAM_OUTLINE_MIN_PIXELS := 200
# The clicked box, in hex radii from the pressed pixel — see `_assert_selection_outline_wraps`.
const SEAM_BOX_RADII := 2.0
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
# THE HAY FIELD — the work fixture's THIRD worked forage tile (issue #449), and the only source in
# either preview harness whose yield label states its FEED rate. It shares a row with forage tile B
# (three hexes west of it) rather than touching it: `+0.40 fodder ♻` is roughly 2.5× the width of
# the widest label this plate had ever drawn, and `_draw_pill_plate` sizes to the measured run, so
# what a frame has to answer is how far the plate REACHES — a question two touching hexes could not
# separate from ordinary neighbour crowding, and which a claim about WHICH account was chosen
# (`_assert_yield_label_component`) cannot see at all.
const FODDER_FIELD_X := 6
const FODDER_FIELD_Y := 8
const FODDER_FIELD_RATE := 0.40
# ---- THE WORKED BAND'S ESCAPEMENT FLOORS --------------------------------------------------------
# Where each worked source's crew stops, as a fraction of that source's capacity
# (`docs/plan_harvest_floor.md`). Every yield label on the map ends in the ZONE MARK of its
# assignment's floor (`BandOverlayRenderer._entry_floor_glyph`), so these fixtures are the only thing
# deciding which marks the frame set ever renders — and they are picked to land in TWO DIFFERENT
# zones deliberately. A fixture set that sat entirely on one floor would draw ONE mark everywhere and
# could not tell a working glyph from a broken one, which is exactly what these rows did while they
# still carried the retired `policy` stance strings: nothing on the client reads `policy` any more, so
# every row fell through to the default floor and rendered the peak mark.
const WORK_PEAK_FLOOR := SourceForecast.FLOOR_FOOD_PEAK
# Below the peak → the drawdown mark. 0.15 is the floor `band_panel_preview.LEGACY_STANCE_FLOORS`
# maps the retired `deplete` stance onto, so the rows that used to say "deplete" still mean it.
const WORK_DRAWDOWN_FLOOR := 0.15
# How many DISTINCT floor marks the worked-band fixture must render. Two is the smallest number that
# makes the mark falsifiable at all; the guard is `_assert_work_floor_marks`.
const WORK_FLOOR_MARKS_MIN := 2
# ---- THE YIELD LABEL'S ONE SLOT (issue #449) ----------------------------------------------------
# The three account rates `_assert_yield_label_component` drives the fall-through with, and the faces
# they must produce. Deliberately DISTINCT values, so a branch that returned the wrong account is
# visible as the wrong NUMBER rather than only as the wrong wording, and the faces are written out
# rather than composed through the renderer's own formatter — a needle built by the code under test
# agrees with whatever that code emits.
const YIELD_LABEL_FOOD_RATE := 0.31
const YIELD_LABEL_FOOD_FACE := "+0.31"
const YIELD_LABEL_FODDER_RATE := 0.40
const YIELD_LABEL_FODDER_FACE := "+0.40 fodder"
# What a source paying into NO account still prints: the food zero, which is the honest reading of a
# worked tile that produced nothing this turn and is what this label has always said.
const YIELD_LABEL_EMPTY_FACE := "+0.00"
# ---- …AND ITS THIRD ARM, THE MATERIALS (arc #527 follow-up) -------------------------------------
# What an INEDIBLE quarry pays: a vector, not a scalar, so the probe drives it with a row of the
# wire's own shape. The face is written out rather than composed, for the reason the two above are.
const YIELD_LABEL_MATERIAL_ROWS := [{"material_id": "hide", "amount": 0.22}]
const YIELD_LABEL_MATERIAL_FACE := "+0.22 hide"
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
# State "max zoom" grid. MIRRORS MapSizes' SMALLEST offered map (Tiny, 56×36), and that is the whole
# reason the state can judge the cap. `zoom_factor` is a multiple of the COVER FIT, so what
# MAX_ZOOM_FACTOR means in pixels is decided by the grid: the SMALLEST map the player can start has
# the largest fitted radius, hence the largest hexes — and therefore the most magnified terrain
# texture — the zoom rail can ever reach in a real game. Judging the cap on a bigger grid would flatter
# it; judging it on this harness's 16×12 grid would slander it (a single hex comes out wider than the
# viewport, so every label and marker falls off-frame and the state judges nothing).
const MAX_ZOOM_GRID_W := 56
const MAX_ZOOM_GRID_H := 36
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
# Predators Phase 1a — a selected Grey Wolf Pack (a carnivore) parked mid-prairie with some prey around
# it. Its `prey_sense_radius` (4) is BOTH the "this is a predator" signal and the ring radius, so the map
# must draw the radius-4 predator-orange PREY-SENSE ring (a 61-tile disk) INSTEAD of the small gold graze
# ring. Col 9 / row 7 leaves the radius-4 disc (cols 5–13, rows 3–11) inside the grid (26×18).
const PASTURE_WOLF_ID := "predator_wolf_09"
const PASTURE_WOLF_PREY_SENSE_RADIUS := 4
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

# --- The ANNOTATION states (crisis / terrain highlight / routes) ---------------------------------
# These three cover the `AnnotationRenderer` family, which had NO fixture at all before. They were
# written AFTER the code they cover, so they encode CURRENT BEHAVIOUR — bugs included. They prove
# "this refactor changed nothing", NOT "this rendering is correct"; do not read a passing byte-diff
# as a correctness result.
#
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
# A one-waypoint order — the draw bails at `points.size() < 2`. Present because a guard only guards
# the reference frame if the frame exercises it.
const ROUTE_DEGENERATE_PATH := [[5, 11]]

# State "max zoom". How far the achieved hex radius may sit from `base_hex_radius × MAX_ZOOM_FACTOR`
# before the state is no longer judging the cap. Both sides are floats computed through the clamp in
# `_update_layout_metrics`, so an exact compare would be a coin flip on the last bit; a fraction of a
# pixel is far tighter than any zoom step (the smallest, MOUSE_ZOOM_STEP, moves the radius by ~0.2 ×
# base) yet immune to float drift.
const MAX_ZOOM_RADIUS_EPSILON := 0.01

# The zoom-rail LADDER guard (`_assert_zoom_ladder`), in RUNGS above MIN_ZOOM_FACTOR so the probes
# follow ZOOM_BUTTON_STEP rather than restating it. The on-rung probe sits clear of both limits, so
# a click in either direction is a real move and not a clamp. The off-grid probe sits MID-WAY
# between two rungs — far enough from both that neither a round-up nor a round-down bug can land on
# the expected answer by luck, which a probe near either neighbour would let happen.
const LADDER_ON_RUNG := 2.0
const LADDER_PROBE_RUNG := 4.0
const LADDER_OFF_RUNG_FRACTION := 0.54
# Clicks in the printed ladder walk: enough to run MIN_ZOOM_FACTOR to the clamp at MAX_ZOOM_FACTOR
# and show the short final step, so the whole rail reads in one line of the log.
const LADDER_WALK_CLICKS := 13

# --- State "overlay picker" (the channel picker on the minimap's border) --------------------------
# `docs/plan_knowledge_screen.md` §6. The picker is on the MINIMAP, so it renders in EVERY frame this
# harness saves — but only its `◐` button does, and a closed button says nothing about the list, the
# stub marker or the legend. This state opens the popover, and the assertions beside it cover the two
# things no picture can carry: the ROSTER's composition and the fact that a chosen channel SURVIVES
# the next snapshot.
#
# The fixture publishes one channel with real values, one flagged `placeholder`, one painted through
# a ramp of the RENDERER's own, and terrain-tag data — so the frame carries a ramp legend, the stub
# marker and the client-side `terrain_tags` row at once, and every shape the legend button's face has
# to describe is in the roster.
const PICKER_CHANNEL_LIVE := "sentiment"
const PICKER_CHANNEL_LIVE_LABEL := "Sentiment"
const PICKER_CHANNEL_STUB := "military"
const PICKER_CHANNEL_STUB_LABEL := "Force Readiness"
const PICKER_LIVE_RAW_SCALE := 100.0
# A THIRD channel, published by the wire exactly like the other two, whose only distinction is on the
# RENDERER's side: `forage` paints through a ramp of its own (`MapView._forage_color`, wheat→green)
# instead of the generic `OVERLAY_COLORS` lerp. It is in the fixture so the legend-button face claim
# below has a channel of that shape to ask about — the shape whose face was wrong, and the one a
# roster of generic channels can never expose.
const PICKER_CHANNEL_OWN_RAMP := MapView.FORAGE_OVERLAY_KEY
const PICKER_CHANNEL_OWN_RAMP_LABEL := "Forage"
# Two tag bits with names, so the tag legend has something to count. The masks are per-tile; a tile's
# bit is chosen off its column so the two tags split the map rather than landing on one hex.
const PICKER_TAG_BIT_A := 1
const PICKER_TAG_BIT_B := 2
const PICKER_TAG_LABEL_A := "Riverine"
const PICKER_TAG_LABEL_B := "Upland"
# The roster the fixture must produce: the empty key leads (`PLACEMENT_FIRST`), the wire's own order
# follows, and the client-side tag channel is last (`PLACEMENT_LAST`).
const PICKER_EXPECTED_ORDER := ["", PICKER_CHANNEL_LIVE, PICKER_CHANNEL_STUB,
	PICKER_CHANNEL_OWN_RAMP, "terrain_tags"]
# A stand-in for a docked Band/City panel, whose shipped narrow shell is ~495px wide. Reserved on the
# MapView exactly as `Main` reserves it for the real panel, so the popover has a docked edge to be
# pushed clear of. It must be WIDER than the popover, or a clamped and an unclamped position land in
# the same place and the assertion passes on either — which is why the probe is paired with a
# precondition asserting the UNRESERVED popover really does reach into the strip.
const PICKER_DOCK_PROBE_ID := &"overlay_picker_probe"
const PICKER_DOCK_PROBE_WIDTH := 495.0
# Slack on the "a popover touches its own button" claim. The gap is `POPOVER_GAP` by construction, so
# this absorbs sub-pixel layout rounding only — wide enough and the claim stops meaning "attached".
const PICKER_ATTACH_TOLERANCE := 2.0
# Biome ids the picker fixture paints the map with, so the bare map's legend has several rows to show.
# Transcribed from the pasture fixture's own table, which is the sim's terrain id space.
#
# **THERE ARE ENOUGH OF THEM TO SCROLL, deliberately.** The legend caps at `LEGEND_MAX_HEIGHT` and the
# reserved scrollbar gutter is only judgeable against a bar that is actually SHOWN — a short key
# renders the same either way, which is how the value column came to be running under the bar in the
# first place.
const PICKER_BIOME_IDS := [0, 1, 6, 8, 9, 10, 11, 12, 14, 15, 17, 20, 22, 24, 26, 30]
# The floor the biome-key assertion holds the legend to. Under the roster, so the claim survives a
# biome that happens not to land on a tile but fails a merge that dropped the table.
const PICKER_BIOME_ROWS_MIN := 12

# --- THE `ready_for_improvement` CHANNEL (docs/plan_knowledge_screen.md §7) ------------------------------
# The AGGREGATE ⌃: every source that could climb a rung right now, painted at once. Its fixture
# extends the ⌃-mark fixture (`_snapshot_work_ready`) rather than replacing it, so the per-source
# badges and the channel are asked about the SAME sources. **The two do not answer identically any
# more, and must not**: a lit hex here is a strict SUBSET of the hexes wearing a ⌃, because this
# channel also asks whether the source has been IMPROVED at all and whether it is the player's.
#
# What the extension adds is the half the badge cannot have: sources NOBODY IS WORKING. The badge is
# drawn on a worked source's own marker, so a map of unworked opportunities is exactly what the
# aggregate is for, and the legend's nearest-unworked line has nothing to name without them.
# **THE STANDING CASE: A FIELD YOU BUILT AND WALKED AWAY FROM.** Tended, ours, sowable and ungated,
# with NOBODY on it — and it lights, because being improved is enough on its own. It was a dark
# control for one round, when condition 1 was "worked" alone; that rule hid exactly this tile.
const READY_UNWORKED_NEAR := Vector2i(9, 5)
const READY_UNWORKED_HERD := Vector2i(4, 3)    # a tamed pen-ceiling herd nobody hunts → offers Corral
const READY_UNWORKED_HERD_ID := "game_aurochs_11"
# **THE HEADLINE CASE, AND IT LIGHTS.** Wild ground band 2 is working, with Cultivation learned: the
# FIRST rung on a source carrying no improvement at all. The rule this replaced demanded an EXISTING
# improvement to upgrade, which can never show a first improvement — reported from play as a faction
# that had just learned Herding, was hunting two tamable herds, and saw an empty map.
const READY_FIRST_RUNG := Vector2i(2, 10)
# Its animal twin, hunted by band 1 — a wild herd with Herding learned, so Tame is on offer now.
const READY_FIRST_RUNG_HERD := Vector2i(10, 4)
const READY_FIRST_RUNG_HERD_ID := "game_sheep_11"
# **THE OWNERSHIP CONTROL: WORKED, IMPROVED, UPGRADEABLE — AND NOT OURS.** Identical to
# `READY_UNWORKED_NEAR` in every ladder respect and worked like the lit sources, so a stated foreign
# owner is the ONLY term separating it from a lit tile.
const READY_FOREIGN := Vector2i(14, 2)
const READY_FOREIGN_FACTION := 1
# **THE LADDER CONTROL** — a WORKED, tended patch whose composition may climb nothing further
# (`can_cultivate` and `can_sow` both false), so no amount of knowledge opens a rung on it. Tended
# rather than wild so the LADDER question is the only thing refusing it.
const READY_BARREN_LADDER := Vector2i(12, 9)
# A half-cultivated patch NOBODY works — dark twice over (unworked, and mid-build), and kept for
# exactly that: it is the shape the fixture carried while the rule was "already improved", and a
# regression that re-lit it would otherwise be silent.
const READY_HALF_BUILT := Vector2i(5, 2)
const READY_HALF_BUILT_PROGRESS := 0.55
# **A TENDED PATCH PART-WAY INTO A FIELD, WITH NOTHING DECLARED — AND IT LIGHTS.** There is no
# "already being built" test: `next_rung_ready` declines only the verb a crew has DECLARED, and a
# meter carrying work that nobody has ordered is a rung the player really can put builders on. It was
# a dark control while the channel carried that fourth test; dropping the test is what lights it.
const READY_MID_FIELD := Vector2i(10, 10)
const READY_MID_FIELD_PROGRESS := 0.6
# The ladder position an UNTOUCHED patch stands at — `forage::RUNG_UNSTARTED`, restated here because
# `_stamp_patch_owner` compares each build meter against it. Anything strictly above it is work
# somebody has sunk into the patch, which is precisely when the sim records an owner.
const READY_LADDER_UNSTARTED := 0.0
# The SECOND player band. It works exactly one source — `READY_FIRST_RUNG`, beside it — so it supplies
# both the anchor for the nearest-moves-with-the-selection claim and the tile that claim names. With
# one band the anchor and its fallback are the same tile and a broken selection read passes.
const READY_SECOND_BAND_ENTITY := 9002
const READY_SECOND_BAND_TILE := Vector2i(2, 9)
# What the fixture's ladder offers, spelled out so a count assertion says WHICH source it expected
# rather than only a number. FOUR patches — `FORAGE_A` (worked + tended → Sow), `READY_FIRST_RUNG`
# (worked + wild → Cultivate), `READY_UNWORKED_NEAR` (improved, nobody on it → Sow) and
# `READY_MID_FIELD` (improved, part-way into a Field nobody declared → Sow) — and THREE herds: the
# worked deer (Corral), the worked wild sheep (Tame) and the abandoned tamed aurochs (Corral).
# Four more sources are staged to stay dark, each for a different reason.
const READY_EXPECTED_PATCHES := 4
const READY_EXPECTED_HERDS := 3
const READY_EXPECTED_READY := 7
# The knowledge row that opens all four rungs — the same one `map_worked_ready` pushes, restated here
# because this state also drives the EMPTY case by clearing it, and the two have to be one edit apart.
const READY_FULL_KNOWLEDGE := {
	"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0,
}
# The SCALING PROBE's synthetic world (§7: "measure that before assuming it is cheap"). A live
# earthlike is 256×192 and the sim seeds a forage patch on EVERY food-module tile that carries any
# human-edible capacity, with no cap anywhere in the capture — so the honest question is not "is one
# `RungGates` call fast" but "what does a map's worth of them cost". The probe builds a patch on every
# tile of a full-size grid, which is the ceiling rather than the expectation.
const READY_PROBE_GRID_W := 256
const READY_PROBE_GRID_H := 192

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
# How many times `_fail` fired this run — the ONE input to the exit status (see `_finish`).
var _failures := 0

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
	# STATE THE FOG CONDITION, never inherit it. `MapView._fow_enabled` defaults to `true` (it fails
	# CLOSED, so the live client cannot draw a revealed frame before the first snapshot's
	# `fog_enabled` arrives), and the states below — through `map_band_pending` — used to render on
	# whatever that default happened to be. When it was `false` they came out unfogged by accident;
	# the day it flipped, all five silently rendered as blank fog with their subject gone. Any state
	# that wants fog says so at its own site (the first is `map_sites_fogged`); this seats the
	# baseline for every state before it.
	_map.set_fow_enabled(false)
	# STATE THE INPUT-SPEED CONDITION too, by the same rule and for the same reason. The autoload has
	# already loaded the DEVELOPER'S real `user://client_settings.cfg`, and `MapView.zoom_step` scales
	# ZOOM_BUTTON_STEP by `zoom_speed_multiplier` — so the river close-ups (which reach their zoom
	# through RIVER_JOIN_ZOOM_STEPS × zoom_step) rendered at a DIFFERENT zoom on any machine whose
	# Options slider had been moved, which silently breaks the bit-identity reference this frame set
	# is. Assign the members DIRECTLY, never the setters: a setter would _save over the player's own
	# prefs file, the contamination `band_panel_preview` isolates its config paths to avoid.
	ClientSettings.zoom_speed_multiplier = ClientSettings.ZOOM_SPEED_DEFAULT
	ClientSettings.pan_speed_multiplier = ClientSettings.PAN_SPEED_DEFAULT
	# …and the INTERFACE SCALE, out of the same file and by the same rule. `UiScaler` has already pushed
	# whatever the developer's Options slider holds onto the window's `content_scale_factor`, which
	# shrinks the logical viewport and makes MapView counter-scale itself — so every frame here would
	# re-project on a machine whose slider has been moved. Pin the member and re-emit `changed`, which
	# is what makes `UiScaler` and this harness's MapView both take the pinned value.
	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()
	# PIN THE PALETTE, the theme half of the same contamination. `ClientSettings` read the developer's
	# real `user://client_settings.cfg` at boot and `HudPalette.apply()` has ALREADY installed whatever
	# theme it found, so a developer running Kiln would re-tint every frame in this set. Re-applying the
	# default here is safe at any point before UI is built: `HudStyle`/`MapView` and the vocabulary
	# modules are all re-derived by `apply`, and nothing on screen has read a colour yet.
	HudPalette.apply(HudPalette.DEFAULT_THEME)
	# And STATE THE INPUT CONDITION — the third of the same family, the treatment `blend_probe` already
	# carries. This harness renders in a REAL window, so `MapView._unhandled_input` picks up the OS
	# cursor and draws a faint HOVER hex outline into whichever frame happens to be rendering when the
	# pointer is over the window. Measured here: `map_riverine_split` came back with a brightened hex
	# outline on ~1 run in 5 — 319 pixels at a max delta of 37, on a DIFFERENT hex each time (hence a
	# different hash each time), which is far too small to notice by eye and easily large enough to
	# defeat the byte-diff this frame set exists to support. No state here is driven by input, so drop
	# input entirely rather than trying to park the pointer.
	_map.set_process_unhandled_input(false)
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
	_assert_work_floor_marks()
	_assert_yield_label_component()

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
	# State A-ready — THE ⌃ READY MARK (issue #412). Same band, but now the sources can climb: a
	# TENDED patch on sowable ground offers Sow, a fully TAMED "pen"-ceiling herd offers Corral, and a
	# third source (the wolf pack, ceiling "wild") offers nothing however much we know. The contrast is
	# the point — a chevron on every marker would prove nothing.
	#
	# Faction knowledge is PUSHED, not inherited: `map_preview` has no HUD, so the row that
	# `Hud.faction_knowledge_changed` normally supplies is set here directly. Without it every source
	# reads "not ready", which is the correct degradation but an unreadable frame.
	_map.display_snapshot(_snapshot_work_ready())
	_map.set_faction_knowledge({
		"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0,
	})
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_worked_ready")
	# State A-unstaffed — **THE SAME THREE SOURCES, with the mid-Cultivate patch's BUILD CREW taken
	# off.** Only the band's `builders` ROW moves between this frame and the one above, so the ⌃ marks and
	# the crew counts are held constant and the one thing that can differ is the building patch's own
	# plate: `🌱42%` in the deep signal ink becomes `🌱⚠` in WARN. **The A/B is the claim** — a plate
	# that always warned would pass this frame alone, and the percentage is what a build nobody is
	# staffing must stop showing.
	_map.display_snapshot(_snapshot_work_unstaffed())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_worked_unstaffed")
	_map.set_faction_knowledge({})  # leave the following states on the honest "knows nothing" default

	# State A-far — the SAME worked band on a large grid so fitted hexes go tiny (radius <
	# ICON_MIN_DETAIL_RADIUS): the per-source yield labels + ⚠ must LOD-SUPPRESS so far zoom stays a
	# clean token/highlight view, not floating-text soup. Regression guard for the yield-label LOD gate.
	_map.display_snapshot(_snapshot_work_on_grid(YIELD_FAR_GRID_W, YIELD_FAR_GRID_H))
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

	# State — THE OVERFLOW CHIP CARRIES WHAT IT HIDES (issue #412). Same crowded hex, but now the band
	# WORKS the herd and the food site standing on it, and the three wonders take every visible slot —
	# so both worked sources fall past the cap and have no marker to ring. Without the roll-up the chip
	# would read a bare `+2` over a hex where two sources are staffed and one can climb a rung, which
	# is precisely the "silent cap reads as nothing here" failure this feature exists to fix.
	_map.display_snapshot(_snapshot_mixed_worked())
	_map.set_faction_knowledge({
		"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0,
	})
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_overflow_worked")
	_map.set_faction_knowledge({})

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
	# hex. What this frame is FOR is JUDGING art that exists, at true marker size and side by side —
	# a swapped, clipped or key-fringed sprite, and species that read as one another. It does NOT
	# prove coverage: `FAUNA_SPRITE_ROSTER` (see its doc comment) is hand-written on the CLIENT side,
	# so it can only show species the client already knows about. **The coverage claim belongs to
	# `cargo xtask fauna-icon-guard`**, which checks this side against the sim's `fauna_config.json`.
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

	# A PNG-LESS guard riding the same wrapping fixture — see `_assert_selection_outline_wraps`. It
	# repans the map, so it comes AFTER the frame it borrows the snapshot from; the next state's
	# `_fit_map_to_view` puts the camera back.
	await _assert_selection_outline_wraps()

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

	# State — A HUNT EXPEDITION'S QUARRY IS MARKED (issue #412). The party is outbound to the wolf
	# pack while the resident band hunts the deer locally: two different routes to a worked source,
	# both wearing the same red ring and crew badge, because the mark describes the SOURCE and not who
	# reached it.
	_map.display_snapshot(_snapshot_hunt_expedition())
	_map.selected_unit_id = TRAVEL_EXPEDITION_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_hunt_expedition_quarry")

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
	# The legend's numbers are the other half of the readout (min/avg/max + how much ground is dead).
	# It used to be PRINTED here, this harness having had no surface to draw it into — the minimap's
	# picker is that surface now, so the reading is a FRAME beside the map it explains. `ui_preview`'s
	# `pasture_legend` state moved here with it, and is better off: it had to transcribe
	# `_build_pasture_legend`'s output into a fixture and hope the two stayed in step.
	await _save_overlay_legend("map_pasture_legend")

	# State "pasture herd range" — the herd's grazing RANGE ring OVER the pasture overlay (Grazing Phase
	# 2b-iii). The same earthlike frame with a big-game herd parked mid-prairie (its range-1 disc of 7
	# tiles sits entirely on the rich prairie steppe) and SELECTED: the warm graze-amber ring must read
	# clearly over the straw/green pasture ramp — that ring-over-graze is the whole point (the player sees
	# the exact ground the sim derives the herd's carrying capacity from). A tile one hex EAST of the herd
	# — inside the graze disc, not the anchor — is also selected: the white selection outline must survive
	# on top of the graze rim that the overlay stamps on every tile of the disc (issue #405).
	_map.display_snapshot(_snapshot_pasture_herd())
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map.selected_herd_id = PASTURE_HERD_ID
	_map.selected_tile = Vector2i(PASTURE_HERD_COL + 1, PASTURE_HERD_ROW)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_pasture_herd_range")
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)

	# State "predator prey-sense ring" — a selected Grey Wolf Pack (Predators Phase 1a). A carnivore
	# doesn't graze, so `prey_sense_radius` (4) REPLACES the graze ring: the map must draw the wide
	# radius-4 predator-ORANGE ring (a 61-tile disk), NOT the small gold graze ring. A prey deer sits
	# nearby to prove the two herd kinds coexist (selecting IT would draw the gold graze ring — the
	# replacement is carnivore-only). Read against map_pasture_herd_range.png: bigger disk, orange not gold.
	# A tile two hexes east of the wolf — well inside the radius-4 disk, not the anchor — is selected: the
	# white selection outline must survive on top of the prey-sense rim stamped on every disk tile (#405).
	_map.display_snapshot(_snapshot_pasture_wolf())
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map.selected_herd_id = PASTURE_WOLF_ID
	_map.selected_tile = Vector2i(PASTURE_HERD_COL + 2, PASTURE_HERD_ROW)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_predator_prey_sense")
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)

	# State "pasture pen footprint" — the SAME frame, but the herd is CORRALLED with pen_radius 1 (Grazing
	# 2d-γ). A penned herd draws no roam-range ring; instead its fenced FOOTPRINT (the 7-tile hex disk of
	# radius 1 around the pen anchor) reads in the distinct enclosure-GREEN tint — deliberately NOT the gold
	# of the roam-range above, so a fenced footprint is unmistakably a different thing. Read it against
	# map_pasture_herd_range.png: same herd tile, green disc instead of gold. Same off-anchor tile inside
	# the footprint is selected: the white selection outline must survive on top of the enclosure rim the
	# footprint stamps on every tile of the disc (issue #405).
	_map.display_snapshot(_snapshot_pasture_pen())
	_map.set_overlay_channel(PASTURE_OVERLAY_KEY)
	_map.selected_herd_id = PASTURE_HERD_ID
	_map.selected_tile = Vector2i(PASTURE_HERD_COL + 1, PASTURE_HERD_ROW)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_pasture_pen_footprint")
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)

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
	await _save_overlay_legend("map_forage_legend")

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
	# Crisis annotations / terrain highlight / routes — the three overlays that had no fixture at all,
	# so no refactor of them could be pixel-checked. They run LAST and each CLEARS its own state
	# afterwards, so a leak here can only ever show up in the annotation frames themselves.
	# They restore the default canvas (the river states above left the pasture aspect pinned) because
	# their fixtures are authored against the GRID_W×GRID_H grid the earlier states use.
	# **They prove UNCHANGED, not CORRECT.**
	#
	# There was a FOURTH, `map_trade_overlay`, and it went with the trade-link substrate itself: the
	# sim publishes no link network, so the overlay it drove drew the empty set on every frame
	# (`docs/plan_contact_and_logistics.md`). Issue #232's route-network overlay is what earns a
	# frame back here.
	await _set_canvas(DEFAULT_CANVAS_SIZE)
	await _settle()

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

	# State "max zoom" — the ZOOM CAP raised from 4× to 7× in issue #375. Every other state renders at
	# the cover fit (MIN_ZOOM_FACTOR), so nothing here had ever judged the OTHER end of the rail, and
	# the cap is only defensible if terrain, text and markers all still read at it. This state sits at
	# exactly MAX_ZOOM_FACTOR with all three in one frame: textured terrain with edge blending on, the
	# worked band's per-source yield labels, and BOTH marker families (the primary band token + its
	# nameplate, the secondary herd glyph). It then pans the band hex to the centre — at the cap the
	# viewport holds only a handful of hexes, so an unpanned frame is an arbitrary corner of the map
	# with none of the subject in it.
	#
	# It renders on MAX_ZOOM_GRID (the game's smallest offered map) rather than this harness's 16×12
	# grid, and that choice is the whole point of the frame — see the const for why the SMALLEST map is
	# the worst case. In short: `zoom_factor` is a multiple of the COVER FIT, so what 7× means in pixels
	# is decided by the grid, and on 16×12 a single hex comes out wider than the viewport (every label
	# and marker off-frame, nothing judged).
	#
	# Read for: terrain textures coherent rather than a magnified blur or a per-hex tiling grid, the
	# yield pills / nameplate at a sane size AGAINST THE HEXES (they scale with the marker, so the
	# failure mode is a label that swells into the neighbouring hex), and no clipped/missing layer.
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false
	_map.display_snapshot(_snapshot_work_on_grid(MAX_ZOOM_GRID_W, MAX_ZOOM_GRID_H))
	_map.selected_unit_id = BAND_ENTITY
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	_map.set_zoom_factor(MAP_VIEW.MAX_ZOOM_FACTOR)
	await _settle()
	# Recompute the band's screen centre AFTER the zoom settles — MapView clamps pan_offset, so the
	# request is not the result (the map_rivers_notch idiom).
	var max_zoom_band := _work_grid_center(MAX_ZOOM_GRID_W, MAX_ZOOM_GRID_H)
	_map.pan_offset += get_viewport().get_visible_rect().size * 0.5 \
		- _map._hex_center(max_zoom_band.x, max_zoom_band.y, _map.last_hex_radius, _map.last_origin)
	_map.queue_redraw()
	await _settle()
	var max_zoom_radius: float = _map.base_hex_radius * MAP_VIEW.MAX_ZOOM_FACTOR
	if absf(_map.last_hex_radius - max_zoom_radius) > MAX_ZOOM_RADIUS_EPSILON:
		push_warning("map_preview: max-zoom radius %.2f != base %.2f × MAX_ZOOM_FACTOR %.1f (= %.2f) — this state no longer sits at the zoom cap and stops guarding it" % [_map.last_hex_radius, _map.base_hex_radius, MAP_VIEW.MAX_ZOOM_FACTOR, max_zoom_radius])
	await _save("map_max_zoom")

	_assert_zoom_ladder()

	await _overlay_picker_state()
	await _ready_for_improvement_state()

	_finish()

## Click a CANVAS point through the REAL input path — `Viewport.push_input`, so the GUI pass decides
## which control is on top exactly as it does for a player. Driving the button's own `pressed` signal
## instead would route around the very thing under test: the picker's full-screen catcher sits on a
## layer ABOVE the bar, so with a popover open it is the catcher, not the button, that receives this.
##
## **THE CONVERSION IS NOT OPTIONAL.** `push_input` takes WINDOW coordinates and a control's rect is in
## CANVAS ones; this harness pins a canvas the window does not match, so an unconverted press lands
## somewhere else entirely — measured, it missed the bar on every leg and every claim failed with
## nothing open. `InputProbe` is `ui_preview`'s, shared rather than copied for the reason
## `band_panel_preview` already shares `fixtures_band.gd`: one implementation of a conversion two
## harnesses need.
func _click_canvas(point: Vector2) -> void:
	var window_point := INPUT_PROBE.canvas_to_window(get_viewport(), get_window(), point)
	INPUT_PROBE.press_left(get_viewport(), window_point)
	INPUT_PROBE.release_left(get_viewport(), window_point)
	await _settle()

## **CLICKING THE OTHER BUTTON SWAPS THE POPOVER; CLICKING THE OPEN ONE'S BUTTON CLOSES IT.** Reported
## from play: with the menu up, pressing the legend button just dismissed the menu — the catcher was
## eating the press. **No frame can carry this**: the failing state renders as a map with nothing open,
## which is a perfectly ordinary map. Every leg is driven as a real press.
func _assert_picker_buttons_swap(picker: OverlayPicker) -> void:
	picker.close_popover()
	await _settle()
	var channel_at: Vector2 = picker.channel_button_rect().get_center()
	var legend_at: Vector2 = picker.legend_button_rect().get_center()

	await _click_canvas(channel_at)
	_assert_map("overlay picker — pressing ◐ with nothing open opens the MENU (%s)"
		% str(picker.open_popover_kind()), picker.open_popover_kind() == OverlayPicker.POPOVER_CHANNELS)
	await _click_canvas(legend_at)
	_assert_map("overlay picker — …then pressing the legend button SWAPS to the legend, not dismiss (%s)"
		% str(picker.open_popover_kind()), picker.open_popover_kind() == OverlayPicker.POPOVER_LEGEND)
	await _click_canvas(channel_at)
	_assert_map("overlay picker — …and back the other way (%s)"
		% str(picker.open_popover_kind()), picker.open_popover_kind() == OverlayPicker.POPOVER_CHANNELS)
	# The toggle half, without which "always open the button's own popover" passes every claim above.
	await _click_canvas(channel_at)
	_assert_map("overlay picker — pressing the OPEN one's own button closes it (%s)"
		% str(picker.open_popover_kind()), picker.open_popover_kind() == OverlayPicker.POPOVER_NONE)
	# And a press on bare map still dismisses, which is the catcher's whole job.
	await _click_canvas(legend_at)
	await _click_canvas(picker.popover_rect().get_center() - Vector2(0.0, picker.popover_rect().size.y))
	_assert_map("overlay picker — a press outside both still dismisses (%s)"
		% str(picker.open_popover_kind()), picker.open_popover_kind() == OverlayPicker.POPOVER_NONE)

## How many of the bare map's legend rows carry terrain art, and how many rows there are — as
## `Vector2i(with_art, total)`, so a caller can state BOTH halves and neither claim can go vacuous on
## an empty key.
func _terrain_legend_rows_with_art() -> Vector2i:
	var rows: Array = _map.current_overlay_legend().get("rows", [])
	var with_art := 0
	for entry in rows:
		if typeof(entry) == TYPE_DICTIONARY and entry.get("texture", null) is Texture2D:
			with_art += 1
	return Vector2i(with_art, rows.size())

## Is `color` a colour the map is ACTUALLY painting on some tile right now? The independent oracle
## behind the legend-button face claim: it asks `_tile_color`, the renderer's own answer per hex,
## rather than the colour TABLE the face is read from — so a face and a table that agree with each
## other and with nothing on screen still fails. A ramp reaches its top colour on its richest tile,
## so the tint a face states is either painted somewhere or is not that channel's colour at all —
## `is_equal_approx` rather than `==` only because `Color.lerp` computes `a + (b - a) * 1.0`, which
## lands within an ULP of `b` rather than on it. That tolerance is orders of magnitude tighter than
## the distance between any two colours in the palette, so it cannot admit a wrong one.
func _map_paints_color(color: Color) -> bool:
	for row in GRID_H:
		for col in GRID_W:
			if _map._tile_color(col, row).is_equal_approx(color):
				return true
	return false

## Open the minimap picker's LEGEND on whatever channel is painted, save the frame, and close it
## again — the reading that used to be a `print` here and a transcribed fixture in `ui_preview`.
##
## **IT CLOSES AGAIN, and that is not tidiness**: the picker is mounted on a long-lived MapView, so a
## popover left open renders in every later frame of the run.
func _save_overlay_legend(name: String) -> void:
	var picker: OverlayPicker = _map._minimap._minimap_2d.overlay_picker
	if picker == null:
		_fail("%s — the minimap panel built no picker to open the legend on" % name)
		return
	picker.open_legend()
	await _settle()
	await _save(name)
	picker.close_popover()
	await _settle()

## State "overlay picker" — the channel picker OPEN on the minimap's top border
## (`docs/plan_knowledge_screen.md` §6). The `◐` button rides every frame this harness saves; only
## this one opens the popover, which is where the list, the `stub data` marker and the legend are.
##
## **THE POPOVER IS IN THE CAPTURE BECAUSE IT IS A `Control`, NOT A `PopupPanel`.** A `PopupPanel` is
## a Window and renders to its own surface, so the shipped popover would have been absent from this
## frame and unjudgeable — the reason `OverlayPicker` follows `TurnOrb`'s catcher shape instead.
func _overlay_picker_state() -> void:
	await _set_canvas(DEFAULT_CANVAS_SIZE)
	await _settle()
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map.display_snapshot(_snapshot_overlay_channels())
	_map._fit_map_to_view()
	await _settle()

	var picker: OverlayPicker = _map._minimap._minimap_2d.overlay_picker
	if picker == null:
		_fail("overlay picker — the minimap panel built none; the picker's mount is gone")
		return

	# THE ROSTER'S COMPOSITION. No picture can carry it: a list of four plausible names renders
	# identically whichever order the merge put them in, and the empty key leading / the client-side
	# tag row trailing are the two placements `OverlayChannels` exists to decide.
	var keys := PackedStringArray()
	for descriptor in picker.roster():
		keys.append(String(descriptor.get("key", "")))
	_assert_map("overlay picker — the roster merges wire + registry in order (%s)" % ", ".join(keys),
		Array(keys) == PICKER_EXPECTED_ORDER)

	# The wire's `placeholder` flag reaches the descriptor, which is what puts the `stub data` marker
	# on the row and in the legend without any channel being named in either.
	var stub_flagged := false
	for descriptor in picker.roster():
		if String(descriptor.get("key", "")) == PICKER_CHANNEL_STUB:
			stub_flagged = bool(descriptor.get("placeholder", false))
	_assert_map("overlay picker — the wire's placeholder flag reaches the '%s' descriptor" % PICKER_CHANNEL_STUB,
		stub_flagged)

	# Choosing a row paints the channel.
	picker.select_channel(PICKER_CHANNEL_LIVE)
	_assert_map("overlay picker — choosing '%s' paints it (active_overlay_key = '%s')"
		% [PICKER_CHANNEL_LIVE, _map.active_overlay_key],
		_map.active_overlay_key == PICKER_CHANNEL_LIVE)

	# **AND IT SURVIVES THE NEXT SNAPSHOT.** `_ingest_overlay_channels` clears `active_overlay_key` on
	# every frame it ingests, so without the re-apply a chosen channel is painted for exactly one turn
	# and then silently reverts to bare terrain — which looks like the player never clicked. The
	# Inspector panel this replaced did the re-apply from its own ingest; the picker does it off
	# `overlay_channels_ingested`, and this is the assertion that says so. NOT off
	# `overlay_legend_changed` — re-asserting on THAT signal is the regression the next block guards.
	_map.display_snapshot(_snapshot_overlay_channels())
	await _settle()
	_assert_map("overlay picker — the chosen channel survives the next snapshot (active_overlay_key = '%s')"
		% _map.active_overlay_key,
		_map.active_overlay_key == PICKER_CHANNEL_LIVE and picker.selected_key() == PICKER_CHANNEL_LIVE)

	# **AND THE PICKER DOES NOT STOMP A CHANNEL IT DID NOT SET.** `set_overlay_channel` emits
	# `overlay_legend_changed`, so a picker that re-asserted on THAT signal would overwrite every
	# other caller of it — `MapView.set_terrain_mode`, `set_fow_enabled`'s deliberate clear, and every
	# state in this harness that drives a channel directly. It shipped that way for one render:
	# `map_pasture` / `map_forage` / the two danger frames / the three pasture-selection frames all
	# came out as bare terrain, each a perfectly plausible picture of a map with no overlay on it.
	_map.set_overlay_channel(PICKER_CHANNEL_STUB)
	await _settle()
	_assert_map("overlay picker — a channel set by someone else STANDS, and the picker adopts it (painted '%s', row '%s')"
		% [_map.active_overlay_key, picker.selected_key()],
		_map.active_overlay_key == PICKER_CHANNEL_STUB and picker.selected_key() == PICKER_CHANNEL_STUB)
	picker.select_channel(PICKER_CHANNEL_LIVE)

	picker.open_channels()
	await _settle()
	_assert_map("overlay picker — the channel menu is a Control in this viewport, not a Window",
		picker.open_popover_kind() == OverlayPicker.POPOVER_CHANNELS)

	# **THE POPOVER MUST CLEAR EVERY DOCKED SURFACE'S LAYER**, and this harness stands up none of
	# them: it has no HUD, so the shipped defect — the popover drawing UNDER the Band/City dock,
	# because the embedded minimap put it on the HUD's own layer — renders here as a perfectly
	# correct frame. So the claim is made against those panels' OWN constants rather than a picture
	# or a number, which is also what makes it fail if one of them is ever raised.
	var docked_layers := {
		"Band/City panel": BandCityPanel.LAYER_INDEX,
		"event dock": EventDockPanel.LAYER_INDEX,
		"Workbench": MAIN_SCRIPT.WORKBENCH_LAYER,
		"Inspector": MAIN_SCRIPT.INSPECTOR_LAYER,
		"HUD": MAIN_SCRIPT.HUD_LAYER,
	}
	var covered := PackedStringArray()
	for name in docked_layers:
		if picker.popover_layer_index() <= int(docked_layers[name]):
			covered.append("%s (%d)" % [name, int(docked_layers[name])])
	_assert_map("overlay picker — the popover's layer (%d) clears every docked surface%s"
		% [picker.popover_layer_index(),
			"" if covered.is_empty() else " — UNDER " + ", ".join(covered)],
		covered.is_empty())
	_assert_map("overlay picker — …and stays under the loading overlay (%d)" % MAIN_SCRIPT.LOADING_OVERLAY_LAYER,
		picker.popover_layer_index() < int(MAIN_SCRIPT.LOADING_OVERLAY_LAYER))

	# **PICKING A ROW MUST NOT GROW THE POPOVER**, and the frame above cannot say so — it is the FIRST
	# render, where the list is built once and every height is correct. The defect is on the REBUILD:
	# `queue_free` deletes at the end of the frame, so the outgoing rows were still counted while the
	# incoming ones went in, the minimum height doubled for that frame, and a `Control` that grows to
	# satisfy a minimum writes the grown size back into its offsets — so it never shrank again. It
	# COMPOUNDS, which is what makes two picks the right probe rather than one.
	var first_height: float = picker.popover_rect().size.y
	picker.select_channel(PICKER_CHANNEL_STUB)
	await _settle()
	picker.select_channel(PICKER_CHANNEL_LIVE)
	await _settle()
	_assert_map("overlay picker — two picks leave the popover its own height (%.0f → %.0f)"
		% [first_height, picker.popover_rect().size.y],
		is_equal_approx(picker.popover_rect().size.y, first_height))

	# **A POPOVER TOUCHES THE BUTTON THAT OPENED IT.** This is the whole reason the legend is its own
	# button rather than a standing panel between the menu and the minimap: with the legend in
	# between, the menu floated a legend's height away from the `◐` it belongs to, which is not what a
	# menu does. The claim is the GAP, measured — a picture cannot separate "attached" from "close".
	_assert_map("overlay picker — the menu hangs off its own button (gap %.0fpx)"
		% (picker.anchor_rect().position.y - picker.popover_rect().end.y),
		absf(picker.anchor_rect().position.y - picker.popover_rect().end.y)
			<= OverlayPicker.POPOVER_GAP + PICKER_ATTACH_TOLERANCE)

	await _save("map_overlay_picker")

	# **THE LEGEND IS THE OTHER BUTTON'S POPOVER, and opening it closes the menu.** Two small cards
	# over one corner of the map would fight for the same space and the same dismiss click.
	picker.open_legend()
	await _settle()
	_assert_map("overlay picker — opening the legend replaces the menu rather than stacking on it",
		picker.open_popover_kind() == OverlayPicker.POPOVER_LEGEND)
	_assert_map("overlay picker — the legend hangs off ITS button, not the menu's (gap %.0fpx)"
		% (picker.anchor_rect().position.y - picker.popover_rect().end.y),
		picker.anchor_rect() == picker.legend_button_rect()
			and absf(picker.anchor_rect().position.y - picker.popover_rect().end.y)
				<= OverlayPicker.POPOVER_GAP + PICKER_ATTACH_TOLERANCE)
	await _save("map_overlay_legend")

	# **THE LEGEND BUTTON'S FACE IS THE STANDING READOUT OF WHICH CHANNEL IS ON**, and with no icon in
	# the registry yet it is that channel's own map tint — or, for a channel that HAS no single tint,
	# the neutral glyph.
	#
	# **THE CLAIM IS OVER THE WHOLE ROSTER, AND ITS ORACLE IS THE PAINTED MAP.** The obvious form of
	# it — `legend_face_color() == overlay_color_for(key)` — is true BY CONSTRUCTION for every key,
	# `overlay_color_for` being the same table lookup the face itself reads; asked only of a channel
	# that has a row it can never fail. That is how `forage` came to wear `OVERLAY_FALLBACK_COLOR`, a
	# blue that appears nowhere on a map painted in the wheat→green ramp `_forage_color` gives it. So
	# a channel painted through a path of its OWN is held to a colour the map is REALLY PAINTING on
	# some tile, sampled from `_tile_color`, which no colour table can satisfy by construction.
	var face_lies := PackedStringArray()
	var own_ramp_faces := PackedStringArray()
	for descriptor in picker.roster():
		picker.select_channel(String(descriptor.get("key", "")))
		await _settle()
		# `set_overlay_channel` may refuse a key, so judge whatever the picker ended up showing.
		var shown := picker.selected_key()
		var face: Color = picker.legend_face_color()
		var neutral: bool = picker.legend_face_glyph() == OverlayPicker.LEGEND_NEUTRAL_GLYPH \
			and face == HudStyle.INK_DIM
		if _map.paints_with_overlay_color(shown):
			# The generic lerp climbs to exactly this tint, so the table's answer IS the map's.
			if face != _map.overlay_color_for(shown):
				face_lies.append("'%s' wears %s, not its own %s" % [shown, face, _map.overlay_color_for(shown)])
		elif neutral:
			pass  # A channel with no colour to state says so, which is the whole point of the glyph.
		elif _map_paints_color(face):
			own_ramp_faces.append(shown)
		else:
			face_lies.append("'%s' wears %s, which the map paints on no tile%s"
				% [shown, face, " (the meaningless fallback)" if face == MapView.OVERLAY_FALLBACK_COLOR else ""])
	_assert_map("overlay picker — every channel's face states a colour the map really paints, else the neutral glyph%s"
		% ("" if face_lies.is_empty() else " — " + ", ".join(face_lies)),
		face_lies.is_empty())
	# **THE POSITIVE COMPANION, and it NAMES ITS CHANNEL, because the claim above is satisfied by a
	# picker that gave up and went neutral on everything.** A channel painted through a ramp of its
	# own still has a colour to state — the ramp's own top — and losing its `OVERLAY_COLORS` row does
	# not make the map paint it any differently, only the button lie about it (with the row gone the
	# face falls to the fallback blue, and the neutral glyph would merely hide that). `forage` is in
	# the roster precisely so one channel of that shape is asked by name.
	_assert_map("overlay picker — …and '%s', painted through a ramp of its own, still wears a real one%s"
		% [PICKER_CHANNEL_OWN_RAMP,
			"" if own_ramp_faces.has(PICKER_CHANNEL_OWN_RAMP)
			else " — it states no colour the map paints (faces that do: %s)" % ", ".join(own_ramp_faces)],
		own_ramp_faces.has(PICKER_CHANNEL_OWN_RAMP))

	picker.select_channel(OverlayChannels.NO_OVERLAY_KEY)
	await _settle()

	# **NO OVERLAY HAS A LEGEND TOO — the biome key**, which is what the retired `L` card carried and
	# is NOT the same table as `terrain_tags` (biomes, not environmental tags). Without it the legend
	# button would have a dead state and that key would have no home in the client at all.
	_assert_map("overlay picker — the bare map's legend is the biome key, not an empty card (%d rows)"
		% _map.current_overlay_legend().get("rows", []).size(),
		_map.current_overlay_legend().get("rows", []).size() >= PICKER_BIOME_ROWS_MIN)

	# **AND ITS SWATCHES ARE THE TERRAIN ART, NOT THE PALETTE** — with textures on, a flat colour names
	# a biome the player cannot match to anything on screen, the hexes being painted art. **The claim
	# is a PAIR and has to be**: a swatch is a small square either way at a glance, so a frame cannot
	# separate "the art" from "the colour", and a one-sided claim passes on a key that hands out a
	# texture whether the map is textured or not. Every row is asked, since a key that textured its
	# first row and gave up would satisfy a spot check.
	_map.enable_terrain_textures(true)
	await _settle()
	var textured := _terrain_legend_rows_with_art()
	_assert_map("overlay picker — with textures ON the biome key wears the terrain art (%d of %d rows)"
		% [textured.x, textured.y], textured.x == textured.y and textured.y > 0)
	await _save_overlay_legend("map_overlay_legend_terrain")
	_map.enable_terrain_textures(false)
	await _settle()
	var flat := _terrain_legend_rows_with_art()
	_assert_map("overlay picker — with textures OFF it falls back to the palette colour (%d of %d rows carry art)"
		% [flat.x, flat.y], flat.x == 0 and flat.y > 0)

	picker.select_channel(PICKER_CHANNEL_LIVE)
	# The legend frames above CLOSE the popover on their way out, so the dock probe below re-opens the
	# menu rather than inheriting whatever the last block left. Its precondition reads the popover's
	# own rect, which is an empty `Rect2` when nothing is open — i.e. it would fail rather than pass
	# silently, but it would fail for the wrong reason.
	picker.open_channels()
	await _settle()

	# **AND IT OPENS INTO THE PLAY AREA, NOT UNDER THE DOCK.** Right-aligning a ~290px popover to a
	# button in the nav cluster puts its far edge inside a ~495px docked panel, so clearing the dock's
	# LAYER alone would only trade an unreadable popover for one covering the panel being read.
	# Reserving the edge on the MapView is exactly what `BandCityPanel` does through `Main`.
	#
	# **THE PROBE RESERVES THE *RIGHT* EDGE, AND THAT IS NOT THE SHIPPED CASE — it is the only one
	# this harness can ask.** The reported defect was a LEFT-docked panel over an EMBEDDED minimap in
	# the HUD's nav cluster; here there is no HUD, so the minimap takes its FLOATING mount at the
	# bottom-right of the viewport, where a left dock is a thousand pixels away and a left-edge
	# assertion would pass with the clamp deleted. Reserving the near edge instead drives the same
	# `_play_area()` bound from the other side, which is the half a fixture can actually move.
	_assert_map("overlay picker — the UNRESERVED popover reaches past where the dock will be (right edge %.0f)"
		% picker.popover_rect().end.x,
		picker.popover_rect().end.x > get_viewport_rect().size.x - PICKER_DOCK_PROBE_WIDTH)
	_map.set_reserved_inset(PICKER_DOCK_PROBE_ID, SIDE_RIGHT, PICKER_DOCK_PROBE_WIDTH)
	picker.close_popover()
	picker.open_channels()
	await _settle()
	var dock_edge: float = get_viewport_rect().size.x - PICKER_DOCK_PROBE_WIDTH
	_assert_map("overlay picker — a %.0fpx dock pushes the popover clear of it (right edge %.0f <= %.0f)"
		% [PICKER_DOCK_PROBE_WIDTH, picker.popover_rect().end.x, dock_edge],
		picker.popover_rect().end.x <= dock_edge)
	picker.close_popover()
	_map.set_reserved_inset(PICKER_DOCK_PROBE_ID, SIDE_RIGHT, 0.0)

	# The `terrain_tags` row's `available` predicate is the registry's one gate, and a world with no
	# tag data is the case it exists for — the tag channel has no wire raster to fall back on.
	picker.close_popover()
	_map.display_snapshot(_base_snapshot(_band([], 2, 0), []))
	await _settle()
	var untagged := PackedStringArray()
	for descriptor in picker.roster():
		untagged.append(String(descriptor.get("key", "")))
	# **PAIRED WITH THE EMPTY KEY'S SURVIVAL, deliberately.** "does not contain `terrain_tags`" is also
	# true of a roster the merge dropped on the floor, and the empty key is spelled `""` — so a
	# one-entry roster prints as an empty string and a zero-entry one prints identically. The two
	# claims together can only be satisfied by the roster this world should actually have.
	_assert_map("overlay picker — a world with no tag data keeps the empty key and is not offered 'terrain_tags' (%d entries)"
		% untagged.size(),
		Array(untagged) == [OverlayChannels.NO_OVERLAY_KEY])

	await _assert_picker_buttons_swap(picker)


## State "ready for improvement" — THE AGGREGATE ⌃ (`docs/plan_knowledge_screen.md` §7), the channel and its
## `facts` legend.
##
## **THE FRAME IS THE CONTRAST, NOT THE GLOW.** A cyan map is a plausible picture of a correct channel
## and of a channel that lights every source it can see; what the frame has to show is that the two
## mid-Cultivate patches, the wild-ceiling wolf, the can-climb-nothing patch, the untouched WILD patch
## and the foreign one stay DARK while `READY_EXPECTED_PATCHES` patches and `READY_EXPECTED_HERDS`
## herds glow. The counts are named rather than restated here because a fixture gains sources over
## time and a number written into prose does not follow them. Everything a picture cannot separate —
## which web each lit hex is on, whether the counts split right, which coordinate the legend named and
## why — is asserted below.
func _ready_for_improvement_state() -> void:
	await _set_canvas(DEFAULT_CANVAS_SIZE)
	await _settle()
	_map.set_fow_enabled(false)
	_map.set_labor_pending({})
	_map.enable_terrain_textures(false)
	_map._map_cache_enabled = false
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)

	# **KNOWLEDGE IS PUSHED AFTER THE SNAPSHOT, AND THAT IS THE SHIPPED ORDER RATHER THAN A
	# CONVENIENCE.** `Main._apply_snapshot` renders the map first and fans the HUD out after it, so
	# `faction_knowledge_changed` always lands behind `display_snapshot` — a channel derived only at
	# ingest would state the PREVIOUS turn's knowledge for the whole turn a discovery arrives on. This
	# state drives that order deliberately: the snapshot goes in with the knowledge row still EMPTY.
	_map.set_faction_knowledge({})
	_map.display_snapshot(_snapshot_ready_for_improvement())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	var picker: OverlayPicker = _map._minimap._minimap_2d.overlay_picker
	if picker == null:
		_fail("ready for improvement — the minimap panel built no picker")
		return

	# **THE INGEST BUILT NOTHING, WHICH IS THE WHOLE POINT OF THE MEASUREMENT.** A `RungGates` pass per
	# source costs ~7 µs a source — 342 ms for a full-size world's worth (the probe at the end of this
	# state) — and paying it on every turn boundary for a channel nobody has selected is work that
	# should not happen. The claim is asked of the CHANNEL TABLE rather than of a flag: an unbuilt
	# channel is one `overlay_channels` has no raster for, which is the same thing
	# `set_overlay_channel` tests. **Paired with the roster claim below**, because "the map holds no
	# raster for it" is also true of a channel that was never wired at all.
	_assert_map("ready for improvement — the snapshot ingest builds NO raster for it (deferred until something asks)",
		not _map.overlay_channels.has(ReadyForImprovement.CHANNEL_KEY))

	# **THE EMPTY CASE IS A REAL STATE, NOT A DEGENERATE ONE.** A faction that has learned nothing can
	# climb nothing, and the channel still exists to say so — it is offered off the WORLD's sources,
	# never off the count, so it does not appear and disappear as the ladder is learned.
	_assert_map("ready for improvement — a world with sources offers the channel even before any knowledge (roster: %s)"
		% ", ".join(_roster_keys(picker)),
		Array(_roster_keys(picker)).has(ReadyForImprovement.CHANNEL_KEY))
	_assert_map("ready for improvement — …and it states the empty answer rather than a count (%s)"
		% ", ".join(_map.ready_for_improvement_facts()),
		Array(_map.ready_for_improvement_facts()) == [ReadyForImprovement.FACTS_NONE])
	_assert_map("ready for improvement — …with nothing lit on the map (%d tiles)" % _lit_ready_tiles().size(),
		_lit_ready_tiles().is_empty())

	# **AND NOW THE PUSH THAT ARRIVES LATE.** No new snapshot: only the knowledge row moves, exactly as
	# it does on the turn a track completes. If the channel were derived at ingest alone this is where
	# it would stay empty.
	_map.set_faction_knowledge(READY_FULL_KNOWLEDGE)
	await _settle()
	var facts: PackedStringArray = _map.ready_for_improvement_facts()
	_assert_map("ready for improvement — the knowledge push that lands AFTER the snapshot re-derives the channel (%s)"
		% ", ".join(facts),
		facts.size() == 2 and String(facts[0]) != ReadyForImprovement.FACTS_NONE)

	# THE COUNTS, split by web. A single total cannot say whether the two walks both ran: one web
	# answering for both produces a perfectly plausible number.
	var model: Dictionary = _map._ready_for_improvement
	_assert_map("ready for improvement — %d patches and %d herds offer a rung (expected %d / %d)"
		% [int(model.get(ReadyForImprovement.MODEL_PATCHES, -1)), int(model.get(ReadyForImprovement.MODEL_HERDS, -1)),
			READY_EXPECTED_PATCHES, READY_EXPECTED_HERDS],
		int(model.get(ReadyForImprovement.MODEL_PATCHES, -1)) == READY_EXPECTED_PATCHES
			and int(model.get(ReadyForImprovement.MODEL_HERDS, -1)) == READY_EXPECTED_HERDS)

	# **A RUNG UNDER WAY IS NOT AN OFFER**, and it takes TWO patches to say so, because the two halves
	# of "under way" are answered by different things. Asked as TILES rather than as a count, since a
	# count that is right for the wrong reason is exactly what a fixture of this size can produce.
	var lit: Array[Vector2i] = _lit_ready_tiles()
	# The DECLARED half — a worked patch whose assignment names `cultivate`. `next_rung_ready` excludes
	# the declared verb by itself, so this one passes with or without the channel's own in-progress
	# question, and it is here for the other reason: it pins that the aggregate reads the same
	# `improvement` axis the badge does.
	_assert_map("ready for improvement — the WORKED patch whose crew declared Cultivate at (9, 8) stays dark (it also still stands on plant:wild)",
		not lit.has(Vector2i(9, 8)))
	# DARK — wild, half-cultivated, and nobody on it: neither half of the candidate union admits it.
	_assert_map("ready for improvement — the UNWORKED, half-cultivated patch at %s stays dark (wild, and nobody on it)"
		% READY_HALF_BUILT, not lit.has(READY_HALF_BUILT))
	_assert_map("ready for improvement — the worked wild-ceiling wolf pack at (11, 4) stays dark however much we know",
		not lit.has(Vector2i(11, 4)))
	# DARK — the LADDER refuses it: worked, tended, ours, and nothing growing here may climb further.
	_assert_map("ready for improvement — a WORKED patch whose plants may climb no further stays dark (%s)"
		% READY_BARREN_LADDER, not lit.has(READY_BARREN_LADDER))
	# DARK — ladder-identical to the lit `READY_UNWORKED_NEAR` and worked like the lit sources, so a
	# stated foreign owner is the only term between it and a lit tile.
	_assert_map("ready for improvement — a WORKED, upgradeable patch at %s owned by faction %d stays dark"
		% [READY_FOREIGN, READY_FOREIGN_FACTION], not lit.has(READY_FOREIGN))

	# **THE TWO HALVES OF THE CANDIDATE UNION, EACH ASSERTED ALONE.** Every wrong version of this
	# channel passed a fixture built around its own set, so both halves are named positively here: a
	# rule that lost either one fails by name rather than by a count that could move for any reason.
	#
	# WORKED but not improved — the FIRST rung, on both webs. The reported defect: a faction that had
	# just learned Herding, hunting two tamable herds, saw an empty map.
	#
	# **AND THIS LINE IS THE OWNERSHIP TEST'S ONLY GUARD.** `READY_FIRST_RUNG` is wild, worked by band
	# 2, and carries no meter above the floor — so `_stamp_patch_owner` gives it NO owner at all,
	# which is exactly what the wire publishes for untouched ground. That is what makes the difference
	# between `ReadyForImprovement._not_another_faction_s` as a REFUSAL (shipped: no owner is fine) and
	# as a REQUIREMENT (`has_owner` and `owner == player`) visible here: the requirement form refuses
	# every first-rung opportunity on the plant web, and this tile is the first one it darkens. While
	# the fixture hard-coded `has_owner: true` on every row, that rewrite left this state green.
	_assert_map("ready for improvement — WILD ground band 2 works at %s lights: a first improvement is an improvement"
		% READY_FIRST_RUNG, lit.has(READY_FIRST_RUNG))
	_assert_map("ready for improvement — …and so does the WILD herd band 1 hunts at %s, one Herding away from Tame"
		% READY_FIRST_RUNG_HERD, lit.has(READY_FIRST_RUNG_HERD))
	# IMPROVED but not worked — the field you built and walked away from, and its herd twin.
	_assert_map("ready for improvement — the tended patch at %s lights with NOBODY on it: improved is enough"
		% READY_UNWORKED_NEAR, lit.has(READY_UNWORKED_NEAR))
	_assert_map("ready for improvement — …and so does the abandoned tamed herd at %s"
		% READY_UNWORKED_HERD, lit.has(READY_UNWORKED_HERD))
	# AND A MID-BUILD METER NOBODY DECLARED IS AN OFFER, not an exclusion — there is no fourth test.
	_assert_map("ready for improvement — the tended patch part-way into a Field at %s lights; nobody declared that job"
		% READY_MID_FIELD, lit.has(READY_MID_FIELD))
	_assert_map("ready for improvement — every worked ready source IS lit (%s)" % str(lit),
		lit.has(Vector2i(FORAGE_A_X, FORAGE_A_Y)) and lit.has(Vector2i(13, 6)))

	# THE NEAREST LINE, and the anchor it is measured from. `FORAGE_A` is one hex from band 1.
	_assert_map("ready for improvement — %d sources are lit, nearest to the SELECTED band (%s)"
		% [int(model.get(ReadyForImprovement.MODEL_READY, []).size()), ", ".join(facts)],
		int(model.get(ReadyForImprovement.MODEL_READY, []).size()) == READY_EXPECTED_READY
			and String(facts[1]) == ReadyForImprovement.FACTS_NEAREST_FORMAT
				% [FORAGE_A_X, FORAGE_A_Y])

	# **SELECT THE OTHER BAND AND NOTHING ELSE.** No snapshot, no re-derive of the sources — the model
	# is cached and only the legend's own scan re-runs, which is the whole reason `facts` is answered
	# on demand instead of stamped into the model at ingest. "Nearest" is a question about where the
	# player is standing, and a fixture with one band cannot tell a real read of the selection from a
	# hardcoded first-band one.
	_map.selected_unit_id = READY_SECOND_BAND_ENTITY
	var moved: PackedStringArray = _map.ready_for_improvement_facts()
	_assert_map("ready for improvement — selecting the far band moves 'nearest' with it, off the CACHED model (%s)"
		% ", ".join(moved),
		String(moved[1]) == ReadyForImprovement.FACTS_NEAREST_FORMAT
			% [READY_FIRST_RUNG.x, READY_FIRST_RUNG.y])
	_map.selected_unit_id = BAND_ENTITY
	await _settle()

	# The channel is PICKABLE — the `province` property. `set_overlay_channel` silently refuses a key
	# it holds no raster for, so this is asked of the map's own `active_overlay_key` rather than of the
	# picker's lit row, which would agree with itself either way.
	picker.select_channel(ReadyForImprovement.CHANNEL_KEY)
	await _settle()
	_assert_map("ready for improvement — choosing the row paints it (active_overlay_key = '%s')"
		% _map.active_overlay_key,
		_map.active_overlay_key == ReadyForImprovement.CHANNEL_KEY)

	# **IT IS PAINTED IN `HudStyle.HEALTHY`, AND THE MAP LAYS IT ON AS A DIM WASH.** Three different
	# values, so each term is spelled as what it actually is: the channel's DECLARED hue, the hue the
	# legend BUTTON states (undimmed — a swatch states the channel's colour, not one tile's tint), and
	# the wash `_tile_color` really paints on a lit hex, which at `TILE_READY` is
	# `GRID_COLOR.lerp(HEALTHY, TILE_READY)` rather than `HEALTHY` itself. **The expected wash is
	# COMPOSED from those two named constants**, never written as a literal colour: the third term
	# exists to make the painted map the oracle, and a literal would only agree with itself.
	#
	# **THE FOURTH TERM IS WHAT PINS THE DIMNESS, and composing the wash is exactly why it is needed.**
	# `expected_wash` moves WITH `TILE_READY`, so raising the constant back to a full fill leaves the
	# third term agreeing with itself — sabotage-verified, the whole run passes at `TILE_READY = 1.0`.
	# What a full fill actually costs is the wash: the map would paint `HEALTHY` itself and blow the
	# lit hexes out against the grid, which is the defect the dimming was for. So the map is also
	# asked NOT to paint the undimmed hue anywhere, and the constant's VALUE becomes load-bearing
	# rather than self-referential.
	#
	# **THE ROSTER-WIDE FACE GUARD CANNOT MAKE THIS CLAIM, and neither can a restatement of it here.**
	# That guard asks whether a face states a colour the map really paints — and this channel rides the
	# GENERIC `GRID_COLOR.lerp(OVERLAY_COLORS.get(key, FALLBACK), value)` path, so with its
	# `OVERLAY_COLORS` row deleted the map paints the meaningless fallback blue and the button states
	# that same blue, honestly. Sabotage-verified: the row can be removed and the whole run still
	# passes, face guard included. It is out of reach for a second reason as well —
	# `_overlay_picker_state`'s fixture carries no patches and no herds, so this key is not even in
	# that roster. What a dropped row actually costs is the HEALTHY GREEN, so that is what is asserted
	# here by name, reached through the map, through the button, and through `_tile_color` as the
	# oracle, so a table that lies to the button and a button that lies about the table both fail.
	var face: Color = picker.legend_face_color()
	var expected_wash := MapView.GRID_COLOR.lerp(HudStyle.HEALTHY, ReadyForImprovement.TILE_READY)
	_assert_map("ready for improvement — the channel declares HudStyle.HEALTHY, the legend button states it undimmed (%s), and the map paints a DIM wash of it (%s) rather than the full hue"
		% [face, expected_wash],
		_map.overlay_color_for(ReadyForImprovement.CHANNEL_KEY) == HudStyle.HEALTHY
			and face == HudStyle.HEALTHY and _map_paints_color(expected_wash)
			and not _map_paints_color(HudStyle.HEALTHY))

	await _save("map_ready_for_improvement")
	await _save_overlay_legend("map_ready_for_improvement_legend")

	# **A WORLD WITH NO SOURCES OFFERS NO CHANNEL.** Paired with the roster claim at the top of this
	# state: "does not contain the key" is also true of a roster the merge dropped, so the empty key
	# has to survive alongside it.
	picker.close_popover()
	_map.display_snapshot(_base_snapshot(_band([], 2, 0), []))
	await _settle()
	_assert_map("ready for improvement — a world with no patches and no herds keeps the empty key and is not offered the channel (%s)"
		% ", ".join(_roster_keys(picker)),
		Array(_roster_keys(picker)) == [OverlayChannels.NO_OVERLAY_KEY])
	_map.set_faction_knowledge({})

	_assert_ready_for_improvement_scale()

## The tiles the `ready_for_improvement` raster is lit on, read back off the CHANNEL rather than off the
## model's counts — the raster is what the map actually paints, and a count that agreed with a plane
## it was not built from would be the assertion agreeing with itself.
func _lit_ready_tiles() -> Array[Vector2i]:
	var out: Array[Vector2i] = []
	var plane: Variant = _map.overlay_channels.get(ReadyForImprovement.CHANNEL_KEY, null)
	if not (plane is PackedFloat32Array):
		return out
	var values: PackedFloat32Array = plane
	for idx in range(values.size()):
		if values[idx] > 0.0:
			out.append(Vector2i(idx % _map.grid_width, idx / _map.grid_width))
	return out

func _roster_keys(picker: OverlayPicker) -> PackedStringArray:
	var keys := PackedStringArray()
	for descriptor in picker.roster():
		keys.append(String(descriptor.get("key", "")))
	return keys

## **WHAT A MAP'S WORTH OF `RungGates` COSTS** — §7 says to measure it before assuming it is cheap, and
## the answer is what decided that this channel is derived once per SNAPSHOT rather than per frame.
##
## It is a REPORT, not a threshold: a timing assertion on a shared machine fails for reasons that have
## nothing to do with the code under test, and a harness that cries wolf stops being read. What is
## asserted is the thing a number cannot drift on — that the probe really did evaluate a full-size
## world, so the microseconds printed beside it are about that world and not about an empty one.
func _assert_ready_for_improvement_scale() -> void:
	_map.set_faction_knowledge(READY_FULL_KNOWLEDGE)
	_map.display_snapshot(_snapshot_ready_probe())
	var sources: int = _map.forage_patch_lookup.size() + _map.herds.size()
	var started: int = Time.get_ticks_usec()
	var model: Dictionary = ReadyForImprovement.derive(_map)
	var elapsed: int = Time.get_ticks_usec() - started
	print("map_preview: ready_for_improvement scale — %d sources on a %d×%d world in %d µs (%d ready)"
		% [sources, READY_PROBE_GRID_W, READY_PROBE_GRID_H, elapsed,
			int(model[ReadyForImprovement.MODEL_PATCHES]) + int(model[ReadyForImprovement.MODEL_HERDS])])
	_assert_map("ready for improvement — the scale probe really walked a full-size world (%d sources)" % sources,
		sources >= READY_PROBE_GRID_W * READY_PROBE_GRID_H)
	_map.set_faction_knowledge({})

## The `ready_for_improvement` fixture: `_snapshot_work_ready`'s three worked sources, plus the unworked ones
## the aggregate exists to surface and the two controls that must stay dark.
func _snapshot_ready_for_improvement() -> Dictionary:
	var snap := _snapshot_work_ready()
	var patches: Array = snap["forage_patches"]
	var worked: Array = []
	# --- THE LIT SET -------------------------------------------------------------------------------
	# `FORAGE_A` is already worked by band 1 and already tended, so it carries the UPPER-rung case.
	# **THE FIRST-RUNG CASE IS THIS ONE**, and it is the whole reason condition 1 is "worked" rather
	# than "already improved": wild ground a band has hands on, with Cultivation learned, is an
	# opportunity the player can take THIS turn. Worked by BAND 2, which is also what gives the
	# nearest-moves-with-the-selection claim a tile of its own to name.
	patches.append(_ready_patch(READY_FIRST_RUNG, false, true, false))
	# --- THE DARK CONTROLS, one per condition ------------------------------------------------------
	# LIT, WITH NOBODY ON IT — improved is enough on its own.
	patches.append(_ready_patch(READY_UNWORKED_NEAR, true, true, true))
	# DARK — `READY_UNWORKED_NEAR`'s ladder exactly, WORKED, on another faction's ground.
	patches.append(_ready_patch(READY_FOREIGN, true, true, true, 0.0, READY_FOREIGN_FACTION))
	worked.append(READY_FOREIGN)
	# DARK — worked and tended, but nothing growing here may climb, so no knowledge opens a rung.
	patches.append(_ready_patch(READY_BARREN_LADDER, true, false, false))
	worked.append(READY_BARREN_LADDER)
	# LIT — tended and sowable, with a Field meter nobody declared. See the constant.
	patches.append(_ready_patch(READY_MID_FIELD, true, true, true, 0.0,
		MapView.PLAYER_FACTION_ID, READY_MID_FIELD_PROGRESS))
	worked.append(READY_MID_FIELD)
	# DARK — wild, half-cultivated, and nobody is on it: neither half of the candidate union admits it.
	patches.append(_ready_patch(READY_HALF_BUILT, false, true, false, READY_HALF_BUILT_PROGRESS))

	var herds: Array = snap["herds"]
	# **THE HERD FIRST-RUNG CASE — the one reported from play.** A wild herd a band is hunting, with
	# Herding learned: Tame is available right now, and the rule this fixture replaced could never have
	# shown it, a wild herd carrying no improvement to upgrade.
	herds.append({
		"id": READY_FIRST_RUNG_HERD_ID, "label": "Wild Sheep (%s)" % READY_FIRST_RUNG_HERD_ID,
		"x": READY_FIRST_RUNG_HERD.x, "y": READY_FIRST_RUNG_HERD.y,
		"biomass": 260.0, "huntable": true,
		"domestication": 0.0, "husbandry_ceiling": "pen",
		"current_rung": RUNG_FX.herd_rung_key(0.0, false),
	})
	# LIT — tamed, penable, and nobody is hunting it: the herd twin of the abandoned field.
	herds.append({
		"id": READY_UNWORKED_HERD_ID, "label": "Aurochs (%s)" % READY_UNWORKED_HERD_ID,
		"x": READY_UNWORKED_HERD.x, "y": READY_UNWORKED_HERD.y,
		"biomass": 420.0, "huntable": true,
		"domestication": 1.0, "husbandry_ceiling": "pen",
		# Tamed and unpenned → `animal:pastoral`, struck off the two meters beside it rather than
		# written out, so this row cannot claim a rung its own state contradicts.
		"current_rung": RUNG_FX.herd_rung_key(1.0, false),
	})

	# **BAND 1 IS PUT ON EVERY WORKED CONTROL, and on the sheep.** A dark control only proves its own
	# condition if every OTHER condition passes, and condition 1 now gates all of them.
	var assignments: Array = snap["populations"][0]["labor_assignments"]
	for tile_variant in worked:
		var tile: Vector2i = tile_variant
		assignments.append({"kind": SourceForecast.LABOR_KIND_FORAGE, "workers": 1,
			"target_x": tile.x, "target_y": tile.y, "improvement": ""})
	assignments.append({"kind": SourceForecast.LABOR_KIND_HUNT, "workers": 1,
		"fauna_id": READY_FIRST_RUNG_HERD_ID,
		"target_x": READY_FIRST_RUNG_HERD.x, "target_y": READY_FIRST_RUNG_HERD.y,
		"improvement": ""})

	# The second band works exactly ONE source — the first-rung patch beside it — so it supplies both
	# an anchor and the tile that anchor is nearest to.
	var second := _band([{"kind": SourceForecast.LABOR_KIND_FORAGE, "workers": 1,
		"target_x": READY_FIRST_RUNG.x, "target_y": READY_FIRST_RUNG.y, "improvement": ""}], 2, 0)
	second["entity"] = READY_SECOND_BAND_ENTITY
	second["current_x"] = READY_SECOND_BAND_TILE.x
	second["current_y"] = READY_SECOND_BAND_TILE.y
	second["id"] = "Band 2"
	snap["populations"].append(second)
	return snap

## One forage patch for the fixture. `tended` fills the cultivated rung so a sowable patch offers Sow
## rather than Cultivate; the two legality flags are SPECIES-global ("can this plant ever climb this
## rung"), which is what `RungGates.any_crop_allows` reads. `progress` puts work on the CULTIVATE
## METER without stamping the rung done — the state `RungGates.rung_in_progress` answers off.
##
## **`current_rung` AND OWNERSHIP ARE BOTH DERIVED HERE, NEVER PASSED IN**, and that is what keeps the
## fixture honest: a caller free to set the standing rung and `is_cultivated` independently could stage
## a patch the wire cannot produce and prove something about it. `owner` is the faction to record IF
## this patch turns out to have one — `_stamp_patch_owner` decides whether it does — and defaults to
## the player because that is what every owned source in this state is; the one foreign patch says so
## explicitly.
func _ready_patch(tile: Vector2i, tended: bool, can_cultivate: bool, can_sow: bool,
		progress: float = 0.0, owner: int = MapView.PLAYER_FACTION_ID,
		field_progress: float = 0.0) -> Dictionary:
	return _stamp_patch_owner({
		"x": tile.x, "y": tile.y,
		"ecology_phase": "thriving",
		"is_cultivated": tended, "is_field": false,
		"current_rung": RUNG_FX.patch_rung_key(tended, false),
		"cultivation_progress": progress,
		# **A PART-FILLED METER IS NOT A BUILT RUNG**, so `is_field` stays false and the standing rung
		# stays whatever the patch has actually finished — the same split the wire makes, and the whole
		# reason a mid-Field patch can clear the improved test and still be refused as under way.
		"field_progress": field_progress,
		"sow_site_refusal": "",
		"composition": [{"species": "wild_wheat", "display_name": "Wild Wheat",
			"share": 1.0, "can_cultivate": can_cultivate, "can_sow": can_sow}],
	}, owner)

## **OWNERSHIP, STRUCK OFF THE ROW'S OWN METERS** — the sim's derivation restated, exactly as
## `fixtures_rung.gd` restates the standing rung, and for the same reason.
##
## `forage::ForagePatch::owner` is `Some` only while the patch stands above `RUNG_UNSTARTED`, and
## `reconcile_owner` clears it the moment the ladder position falls back to nothing: a patch is owned
## because somebody has sunk work into it, and for no other reason. So a patch is the player's iff one
## of its improvement meters is above the floor — a built rung (`is_cultivated` / `is_field`) or a
## part-filled one (`cultivation_progress` / `field_progress`) — and a WILD, untouched patch states
## `has_owner: false` with no `owner` key at all.
##
## ⛔ **A FIXTURE THAT HARD-CODES `has_owner` STAGES A SOURCE THE SERVER CANNOT PUBLISH**, and every
## row here did until the review that found it: untouched wild ground claiming a faction. What that
## cost was the ownership test's only guard. `ReadyForImprovement._not_another_faction_s` is written as
## a REFUSAL on purpose — no owner is fine, our owner is fine, only a stated FOREIGN owner refuses —
## and with every row claiming an owner, rewriting it into a REQUIREMENT (`has_owner` and `owner ==
## player`) left this whole state green, while on a live map it would have darkened every first-rung
## opportunity on the plant web. `READY_FIRST_RUNG` is now unowned, as the wire has it, so that rewrite
## fails by name.
##
## The meters are reached through `SourceForecast`'s own key tables, so a row cannot be judged owned
## off a field the client has stopped publishing.
func _stamp_patch_owner(patch: Dictionary, owner: int) -> Dictionary:
	var owned := _patch_rung_built(patch, SourceForecast.IMPROVEMENT_CULTIVATE) \
		or _patch_rung_built(patch, SourceForecast.IMPROVEMENT_SOW) \
		or _patch_rung_started(patch, SourceForecast.IMPROVEMENT_CULTIVATE) \
		or _patch_rung_started(patch, SourceForecast.IMPROVEMENT_SOW)
	patch[ReadyForImprovement.SOURCE_HAS_OWNER_KEY] = owned
	if owned:
		patch[ReadyForImprovement.SOURCE_OWNER_KEY] = owner
	else:
		# **NO `owner` KEY AT ALL**, not a sentinel: that is what the decoder publishes when `has_owner`
		# is false, and a reader that consults `owner` without checking `has_owner` first must find
		# nothing there. Erased rather than skipped, so a row RE-stamped after a mutation cannot keep
		# an owner its meters no longer justify.
		patch.erase(ReadyForImprovement.SOURCE_OWNER_KEY)
	return patch

## Has this patch FINISHED the rung `improvement` builds — the wire's done-flag for it.
func _patch_rung_built(patch: Dictionary, improvement: String) -> bool:
	return bool(patch.get(
		String(SourceForecast.FORECAST_DONE_FLAG_KEYS[improvement]), false))

## …and has it put any work at all into that rung's meter. Strictly above the floor, because a meter
## resting AT `READY_LADDER_UNSTARTED` is untouched ground, not a part-built rung.
func _patch_rung_started(patch: Dictionary, improvement: String) -> bool:
	return float(patch.get(
		String(SourceForecast.FORECAST_BUILD_METER_KEYS[improvement]),
		READY_LADDER_UNSTARTED)) > READY_LADDER_UNSTARTED

## The SCALING probe's world: a full-size grid with a patch on every tile and one band, which is the
## ceiling on how many sources the derivation can ever be handed. Deliberately not a plausible map —
## a real earthlike is part ocean — because a ceiling is the number worth knowing.
func _snapshot_ready_probe() -> Dictionary:
	var patches: Array = []
	# TENDED and the player's — a probe measuring the ceiling has to make every source QUALIFY, or the
	# number it prints is the cost of rejecting them early rather than the cost of the full walk.
	# `tended` does both jobs: it puts each patch above its branch's floor AND, being a built rung, it
	# is what gives the patch an owner at all (`_stamp_patch_owner`), which `_ready_patch` records as
	# the player by default.
	for row in range(READY_PROBE_GRID_H):
		for col in range(READY_PROBE_GRID_W):
			patches.append(_ready_patch(Vector2i(col, row), true, true, true))
	var terrain: Array = []
	terrain.resize(READY_PROBE_GRID_W * READY_PROBE_GRID_H)
	terrain.fill(PICKER_BIOME_IDS[0])
	return {
		"grid": {"width": READY_PROBE_GRID_W, "height": READY_PROBE_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [_band([], 2, 0)],
		"forage_patches": patches,
		"herds": [],
	}

## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("map_preview: FAIL — %s" % message)


## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place.
func _finish() -> void:
	if _failures > 0:
		print("map_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("map_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)

## The zoom rail's LADDER, asserted rather than photographed — it SAVES NO PNG, deliberately, so the
## frame set stays a 62-frame bit-identity reference and this guard cannot re-baseline anything.
## A picture could never carry these claims anyway: every rung renders as a plausible map, so the
## difference between a correct ladder and a drifting one is invisible in a frame.
##
## It exists because the rail shipped SCALED by `ClientSettings.zoom_speed_multiplier`: at the
## slider's max each click was 1.5, so the rail ran 1.0 → 2.5 → 4.0 → 5.5 → 7.0 with no 6.0 or 6.5,
## and a different ladder again from the startup zoom. `zoom_step` now snaps to rungs and ignores the
## slider (the harness pins that slider anyway, so ONLY an assertion can see a regression here).
func _assert_zoom_ladder() -> void:
	var step: float = MAP_VIEW.ZOOM_BUTTON_STEP
	var floor_zoom: float = MAP_VIEW.MIN_ZOOM_FACTOR
	var ceil_zoom: float = MAP_VIEW.MAX_ZOOM_FACTOR

	# ON a rung: one click must move exactly one rung, in both directions. The epsilon inside
	# `zoom_step` is what this catches if it is wrong — too small and float drift makes the click a
	# near-zero no-op, too large and it skips a rung.
	var on_rung: float = floor_zoom + LADDER_ON_RUNG * step
	_map.set_zoom_factor(on_rung)
	_map.zoom_step(1)
	_assert_ladder("on-rung +1 from %.2f" % on_rung, _map.zoom_factor, on_rung + step)
	_map.zoom_step(-1)
	_assert_ladder("on-rung -1 back to %.2f" % on_rung, _map.zoom_factor, on_rung)

	# OFF the ladder — where the wheel and pinch leave it. One click must SNAP to the adjacent rung in
	# the direction of travel, never add a step to the off-grid value. The probe sits mid-way between
	# two rungs so neither a round-up nor a round-down bug can pass by luck.
	var below_rung: float = floor_zoom + LADDER_PROBE_RUNG * step
	var off_grid: float = below_rung + LADDER_OFF_RUNG_FRACTION * step
	_map.set_zoom_factor(off_grid)
	_map.zoom_step(1)
	_assert_ladder("off-grid %.2f +1 snaps up" % off_grid, _map.zoom_factor, below_rung + step)
	_map.set_zoom_factor(off_grid)
	_map.zoom_step(-1)
	_assert_ladder("off-grid %.2f -1 snaps down" % off_grid, _map.zoom_factor, below_rung)

	# Both limits: the delta comes out 0 and `_apply_zoom`'s `is_zero_approx` early-out makes the
	# click a clean no-op (no wrap, no crawl past the clamp, no spurious `zoom_changed`).
	_map.set_zoom_factor(ceil_zoom)
	_map.zoom_step(1)
	_assert_ladder("+1 at MAX_ZOOM_FACTOR is a no-op", _map.zoom_factor, ceil_zoom)
	_map.set_zoom_factor(floor_zoom)
	_map.zoom_step(-1)
	_assert_ladder("-1 at MIN_ZOOM_FACTOR is a no-op", _map.zoom_factor, floor_zoom)

	# The ladder as a player walks it, printed so the rungs can be read at a glance in the run log.
	_map.set_zoom_factor(floor_zoom)
	var walk := PackedStringArray([("%.1f" % _map.zoom_factor)])
	for _i in range(LADDER_WALK_CLICKS):
		_map.zoom_step(1)
		walk.append("%.1f" % _map.zoom_factor)
	print("map_preview: zoom ladder = ", " → ".join(walk))

## **THE SELECTION OUTLINE IS STAMPED WHERE THE CLICK LANDED**, on a wrapping map as much as a flat
## one. It SAVES NO PNG (the frame set stays a bit-identity reference) and could not usefully be one
## anyway: the failure is an outline drawn a whole map width away, i.e. a frame with nothing in it —
## indistinguishable by eye from a hex that was never selected, which is exactly how it shipped.
##
## `selected_tile` holds a DATA column (`_point_to_offset` posmods the pick) while the terrain loop
## draws each column at whatever LOGICAL copy the viewport is over, so an unwrapped outline lands on
## the canonical copy off-frame. In game it read as "clicking some hexes doesn't select them" — the
## panel filled in correctly every time, and only the white box was missing, on exactly the tiles the
## seam had pushed into a wrapped copy.
##
## The three assertions are one claim in three parts. The first is the PREMISE — the probe really is
## over a wrapped copy — and without it the rest pass on any map at all, seam or no seam. The other
## two READ PIXELS, deselected against selected, because a geometry assertion could only re-ask
## `_hex_center_wrapped` the question the DRAW asks it, and would stay green if the draw stopped
## calling it. Ink appearing in the clicked hex's own box, and nowhere else in the frame, is the
## thing a player actually reports missing.
func _assert_selection_outline_wraps() -> void:
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	# Pan half a map west: the low columns' wrapped copies move into the middle of the frame, a whole
	# map width from where their canonical copy sits.
	_map.pan_offset.x = -_map.last_map_size.x * SEAM_PAN_MAP_WIDTHS
	_map.queue_redraw()
	await _settle()

	var radius: float = _map.last_hex_radius
	var origin: Vector2 = _map.last_origin
	var viewport := Rect2(Vector2.ZERO, _map._get_adjusted_viewport_size())
	var probe := viewport.size * SEAM_PROBE_FRACTION

	# The round trip a player makes: press a pixel, get a tile — then the tile's outline has to come
	# back to the pixel that was pressed.
	var tile: Vector2i = _map._point_to_offset(probe)
	var canonical: Vector2 = _map._hex_center(tile.x, tile.y, radius, origin)
	_assert_map(
		"seam probe lands on a WRAPPED copy — tile %d,%d draws its canonical copy at x=%.0f, off a %.0f-wide frame"
			% [tile.x, tile.y, canonical.x, viewport.size.x],
		not viewport.has_point(canonical)
	)

	var before: Image = await _capture()
	_map.selected_tile = tile
	_map.queue_redraw()
	await _settle()
	var after: Image = await _capture()
	if before == null or after == null:
		return

	# Logical map units → captured-image pixels. The capture matches the pinned WINDOW while the
	# viewport reports the `expand` projection, so the two spaces differ by a constant factor.
	var to_image: float = float(before.get_width()) / viewport.size.x
	# TWO radii around the click, not one: the press lands anywhere inside the hex, so the outline
	# reaches a full radius past it on the far side. A tighter box splits the ring and the
	# "inks nothing else" half fails on the outline's own far edge.
	var reach: Vector2 = Vector2(radius, radius) * SEAM_BOX_RADII * to_image
	var box := Rect2i(Vector2i(probe * to_image - reach), Vector2i(reach * 2.0))
	var inked_in_box: int = _count_changed_pixels(before, after, box)
	var inked_elsewhere: int = _count_changed_pixels(before, after, Rect2i()) - inked_in_box

	_assert_map(
		"selecting tile %d,%d inks its own hex — %d px changed in the clicked box (min %d)"
			% [tile.x, tile.y, inked_in_box, SEAM_OUTLINE_MIN_PIXELS],
		inked_in_box >= SEAM_OUTLINE_MIN_PIXELS
	)
	_assert_map(
		"and inks nothing else — %d px changed outside the clicked box" % inked_elsewhere,
		inked_elsewhere == 0
	)
	_map.selected_tile = Vector2i(-1, -1)

## Pixels differing between two captures of the same frame, within `rect` (an EMPTY rect means the
## whole image). The images are the same scene rendered twice with one thing changed, and this
## harness freezes `Engine.time_scale`, so every differing pixel is that one thing.
func _count_changed_pixels(before: Image, after: Image, rect: Rect2i) -> int:
	var bounds := Rect2i(Vector2i.ZERO, before.get_size())
	var region: Rect2i = bounds if not rect.has_area() else rect.intersection(bounds)
	var changed := 0
	for y in range(region.position.y, region.end.y):
		for x in range(region.position.x, region.end.x):
			if before.get_pixel(x, y) != after.get_pixel(x, y):
				changed += 1
	return changed

## Same shape as `ui_preview`'s `_assert_hud`: PASS prints, FAIL goes through `_fail` — the harness's
## ONE sink, so the run's exit status counts this claim. It used to print its own `FAIL` line and
## `push_warning` beside it, which is the whole defect the sink exists to remove: every assertion in
## this harness reaches the log through here, so the run printed the family's `FAIL` token and then
## exited 0. The `zoom-ladder — ` category mirrors the `PASS zoom-ladder — ` line it fails against, the
## way `ui_preview`'s `hud — ` category does.
func _assert_ladder(label: String, actual: float, expected: float) -> void:
	if is_equal_approx(actual, expected):
		print("map_preview: PASS zoom-ladder — %s (%.2f)" % [label, actual])
	else:
		_fail("zoom-ladder — %s: got %.4f, expected %.4f" % [label, actual, expected])

## The general form of the above, for a claim that is already a bool. Same PASS/FAIL wording and the
## same sink, so one grep reads every assertion in this harness and `$?` agrees with it.
func _assert_map(label: String, condition: bool) -> void:
	if condition:
		print("map_preview: PASS — %s" % label)
	else:
		_fail(label)

## **THE WORKED-BAND FRAMES RENDER MORE THAN ONE HARVEST MARK.** A picture cannot carry this: every
## zone mark is a plausible-looking glyph on a yield label, so a renderer that answered ONE mark for
## every floor would render a perfectly reasonable frame and freeze a wrong picture into the
## bit-identity baselines. That is the state these fixtures were actually in — they carried the
## retired `policy` stance strings, which no client code reads, so every row fell through to the
## default floor and every label wore the peak mark.
##
## It asks the RENDERER (`_entry_floor_glyph`), not the fixtures' floors, because the fixtures'
## floors differing is the premise, not the claim — running them back through
## `SourceForecast.floor_zone` here would only restate the fixture. An empty mark is failed
## separately: an unknown zone answers `""`, which is a silently unmarked label rather than a wrong
## one.
func _assert_work_floor_marks() -> void:
	var marks := {}
	var floored := 0
	for entry_variant in _snapshot_work()["populations"][0]["labor_assignments"]:
		var entry: Dictionary = entry_variant
		if not entry.has("floor"):
			continue
		floored += 1
		var glyph: String = _map._band_overlays._entry_floor_glyph(entry)
		marks[glyph] = int(marks.get(glyph, 0)) + 1
	_assert_map("worked-band floor marks — %d assignments carry a floor and render %d DISTINCT marks (%s)"
		% [floored, marks.size(), " ".join(PackedStringArray(marks.keys()))],
		marks.size() >= WORK_FLOOR_MARKS_MIN)
	_assert_map("worked-band floor marks — every floored assignment resolves to a mark (no blank zone)",
		not marks.has(""))

## **THE ONE-SLOT FALL-THROUGH, food → FODDER** (issue #449; a trade branch stood between the two
## until arc #527 retired that account). A map label has room for exactly one rate, so which account
## it states is the whole claim — and a PNG cannot carry it: `+0.00` and
## `+0.40 fodder` are the same badge at map scale, and every fall-through renders a perfectly plausible
## label. So the CHOICE is asked of the renderer directly (`_yield_label_rate_text`), over values rather
## than over a fixture, and each case is paired with the one that must NOT change: a source paying food
## keeps its food figure whatever else it pays, which is what stops "always show fodder" passing.
##
## `_entry_fodder` is asked beside them, because the fall-through is only reachable if the entry's feed
## rate is read at all — and it has NO realized fallback to make, fodder being plant-only.
func _assert_yield_label_component() -> void:
	var overlays: BandOverlayRenderer = _map._band_overlays
	_assert_map("yield label — a fodder-only source states its feed rate, not +0.00",
		overlays._yield_label_rate_text(0.0, YIELD_LABEL_FODDER_RATE) == YIELD_LABEL_FODDER_FACE)
	_assert_map("yield label — food still leads wherever there is food",
		overlays._yield_label_rate_text(YIELD_LABEL_FOOD_RATE, YIELD_LABEL_FODDER_RATE)
			== YIELD_LABEL_FOOD_FACE)
	_assert_map("yield label — an inedible quarry states its material, not +0.00",
		overlays._yield_label_rate_text(0.0, 0.0, YIELD_LABEL_MATERIAL_ROWS)
			== YIELD_LABEL_MATERIAL_FACE)
	_assert_map("yield label — food still leads a source that pays food AND a material",
		overlays._yield_label_rate_text(YIELD_LABEL_FOOD_RATE, 0.0, YIELD_LABEL_MATERIAL_ROWS)
			== YIELD_LABEL_FOOD_FACE)
	_assert_map("yield label — fodder still beats a material, in the wire's own order",
		overlays._yield_label_rate_text(0.0, YIELD_LABEL_FODDER_RATE, YIELD_LABEL_MATERIAL_ROWS)
			== YIELD_LABEL_FODDER_FACE)
	_assert_map("yield label — a source paying nothing still prints its food zero",
		overlays._yield_label_rate_text(0.0, 0.0) == YIELD_LABEL_EMPTY_FACE
			and overlays._yield_label_rate_text(0.0, 0.0, []) == YIELD_LABEL_EMPTY_FACE)
	_assert_map("yield label — the material vector is read off the entry, empty when absent",
		overlays._entry_materials({SourceForecast.ASSIGNMENT_MATERIAL_YIELD_KEY:
			YIELD_LABEL_MATERIAL_ROWS}).size() == YIELD_LABEL_MATERIAL_ROWS.size()
		and overlays._entry_materials({}).is_empty())
	_assert_map("yield label — the feed rate is read off the entry with no realized fallback",
		is_equal_approx(overlays._entry_fodder({"fodder_yield": YIELD_LABEL_FODDER_RATE}),
			YIELD_LABEL_FODDER_RATE)
		and is_equal_approx(overlays._entry_fodder({}), 0.0))

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
## or captured. macOS applies (and RE-applies) a window mode/size change asynchronously,
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
	_fail("viewport never came back to the pinned %s canvas" % _canvas_size)
	return null

func _save(name: String) -> void:
	var image: Image = await _capture()
	if image == null:
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		_fail("failed to save %s (err %d)" % [name, err])
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
		_fail("failed to save %s (err %d)" % [name, err])
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
		_fail("failed to save %s (err %d)" % [name, err])
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

## The pasture snapshot with a selected Grey Wolf Pack (a CARNIVORE) mid-prairie and some prey herds
## around it — for the prey-sense-ring state (Predators Phase 1a). The wolf carries
## `prey_sense_radius` (4), so MapView draws its radius-4 predator ring INSTEAD of a graze ring; the
## deer alongside it are herbivores (`prey_sense_radius` absent → 0), so if one is selected it still
## draws the small gold graze ring — the replacement is carnivore-only.
func _snapshot_pasture_wolf() -> Dictionary:
	var snapshot := _snapshot_pasture()
	snapshot["herds"] = [
		{
			"id": PASTURE_WOLF_ID,
			"label": "Grey Wolf Pack (%s)" % PASTURE_WOLF_ID,
			"species": "Grey Wolf Pack",
			"size_class": "big",
			"huntable": false,
			"ecology_phase": "thriving",
			"x": PASTURE_HERD_COL,
			"y": PASTURE_HERD_ROW,
			"biomass": 320.0,
			"prey_sense_radius": PASTURE_WOLF_PREY_SENSE_RADIUS,
		},
		{
			"id": PASTURE_HERD_ID,
			"label": "Red Deer (%s)" % PASTURE_HERD_ID,
			"species": "Red Deer",
			"size_class": "big",
			"huntable": true,
			"ecology_phase": "thriving",
			"x": PASTURE_HERD_COL + 3,
			"y": PASTURE_HERD_ROW + 2,
			"biomass": 1480.0,
			"carrying_capacity": 2150.0,
			"graze_range_radius": PASTURE_HERD_RANGE_RADIUS,
		},
	]
	return snapshot

## The pasture snapshot with a CORRALLED herd (pen_radius 1) at the same tile — for the pen-footprint
## state. Same herd position as `_snapshot_pasture_herd()`, but penned: MapView draws the fenced
## footprint disc (enclosure green) instead of the roam-range ring.
func _snapshot_pasture_pen() -> Dictionary:
	var snapshot := _snapshot_pasture()
	# **THE STANDING RUNG IS STAMPED OFF THE ROW'S OWN FLAGS**, never written beside them: the pen flag
	# below is what makes this `animal:pen`, and `RUNG_FX.stamp_herd` is the only thing that says so.
	snapshot["herds"] = [RUNG_FX.stamp_herd({
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
	})]
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

## The INEDIBLE quarry (issue #337, arc #527) — a wolf pack whose hunt pays no food at all. Its label
## used to fall through to a trade rate; that account is retired and a herd's materials carry no
## per-turn figure, so what it prints now is the honest `+0.00` of a hunt that banks no commodity the
## one-slot label can name.
func _pelt_only_wolf_herd() -> Dictionary:
	return {"id": "game_wolf_03", "label": "Grey Wolf (game_wolf_03)", "x": 11, "y": 4,
		"biomass": 240.0, "huntable": true, "prey_sense_radius": 4}

## Two pens side by side: one FED, one STARVING. `corralled` + `pen_fed_fraction` < 1 is the sim's
## starving signal — the herd is losing biomass every turn, and the map must show WHICH pen.
func _snapshot_pens() -> Dictionary:
	var fed := _deer_herd()
	fed["corralled"] = true
	fed["pen_fed_fraction"] = 1.0
	# Every pen here states its standing rung off its own `corralled` flag rather than being told one.
	RUNG_FX.stamp_herd(fed)
	var starving := RUNG_FX.stamp_herd({
		"id": "game_aurochs_03", "label": "Aurochs (game_aurochs_03)",
		"x": 10, "y": 7, "biomass": 310.0, "huntable": true,
		"corralled": true, "pen_fed_fraction": 0.4,
	})
	# A THIRD pen, starving, whose species has BUNDLED SPRITE ART (boar) — the aurochs above is an
	# emoji species, so without this the frame never proves the distress ring/badge still reads over a
	# sprite marker (the sprite is drawn untinted, exactly like the emoji, so the geometry is the whole
	# distress signal on both paths).
	var starving_sprite := RUNG_FX.stamp_herd({
		"id": "game_boar_05", "label": "Wild Boar (game_boar_05)",
		"x": 7, "y": 7, "biomass": 260.0, "huntable": true,
		"corralled": true, "pen_fed_fraction": 0.3,
	})
	return _base_snapshot(_band([], 2, 2), [fed, starving, starving_sprite])

## Every species in `FoodIcons.HERD_SPECIES`, one herd per ALIAS GROUP, laid out on its own hex so
## each `FaunaSprites` marker can be judged at TRUE marker size. This is the roster frame: it is the
## only place the whole bundled-art set is visible at once, so a swapped/clipped/fringed sprite shows
## up here and nowhere else. One entry per group is enough — aliases resolve to the same PNG.
## THE FOUR CERVIDS LEAD THE LIST, ADJACENT, AND THAT ORDERING IS THE POINT OF THE FRAME (issue
## #439). Red Deer / Wild Elk / Wild Reindeer / Desert Gazelle are four distinct roster species that
## all drew `deer.png` until they were given their own art, and the failure that hid it was that no
## frame ever put them side by side — each looked fine alone. Standing them in a row makes "these two
## are the same picture" the first thing the eye catches. Keep them adjacent.
const FAUNA_SPRITE_ROSTER := [
	["game_deer_01", "Red Deer"],
	["game_elk_01", "Wild Elk"],
	["game_reindeer_01", "Wild Reindeer"],
	["game_gazelle_01", "Desert Gazelle"],
	["game_rabbit_01", "Rabbit Warren"],
	["game_boar_01", "Wild Boar"],
	["game_mammoth_01", "Thunder Mammoth"],
	["game_aurochs_01", "Aurochs"],
	["game_cattle_01", "Cattle"],
	["game_goat_01", "Wild Goat"],
	["game_horse_01", "Wild Horse"],
	["game_sheep_01", "Sheep"],
	["game_fowl_01", "Jungle Fowl"],
	["game_wolf_01", "Grey Wolf Pack"],
	["game_seal_01", "Grey Seals"],
	["game_catfish_01", "Silt Catfish"],
	["game_steppe_runner_01", "Steppe Runners"],
	["game_marsh_grazer_01", "Marsh Grazers"],
]
## THIS LIST CANNOT PROVE COVERAGE, and adding these last two is what made that concrete. It is
## hand-written on the CLIENT side, so it enumerates the client's own vocabulary: Steppe Runners and
## Marsh Grazers were absent from `FaunaSprites.SPRITE_PATHS` AND from here, and a frame that only
## shows what the table already knows cannot fail on a species the table has never heard of. Both
## drew an OS emoji on a live map for as long as they have existed (issue #439). **The coverage
## claim belongs to `cargo xtask fauna-icon-guard`**, which checks this side against the sim's
## `fauna_config.json`; what this frame is for is JUDGING art that exists — swapped, clipped or
## fringed sprites, and species that read as one another — which no guard can do.
## Rows of eight — two full ones and a short third. It was one row of eleven until the roster outgrew `GRID_W` (16 columns, and a
## single spaced row of 16 would run off the map), and `seal` + `catfish` were simply pushed OFF a
## frame whose whole job is to put every sprite this list names in one picture — so the row count is
## not cosmetic, it is what let two PNGs go unjudged. MapView is COVER-fit, so the axis that gets cropped is whichever one the grid is
## longer in relative to the window: on this state's `DEFAULT_CANVAS_SIZE` the 16×12 grid is wider
## than the window's aspect, so all twelve ROWS are on screen and it is the outer COLUMNS that are
## cut (roughly cols 2–14 survive). Cols 4–11 therefore sit well inside with margin to spare.
const FAUNA_ROSTER_COLUMNS := 8
## Starts on row 4, NOT row 5, and that is the whole reason this constant is not simply centred: the
## band camp stands on (BAND_X, BAND_Y) = (8, 6). Starting on 5 put the roster's second row on 6,
## where the Jungle Fowl landed on that very hex and rendered STACKED under the camp marker instead
## of alone at true marker size. A roster frame that judges sprites cannot let one share a hex with
## the band. **The roster now spills onto row 6 anyway (entries 17-18), and that is fine only because
## it wraps at column 4** — cols 4-5, nowhere near the band's col 8. If this list ever grows past 20
## entries the wrap reaches col 8 on row 6 and the collision returns, so move the origin or widen
## `FAUNA_ROSTER_COLUMNS` at that point rather than discovering it in a frame.
const FAUNA_ROSTER_ORIGIN := Vector2i(4, 4)
## Hexes between roster entries — one apart, so eight fit across GRID_W without markers colliding.
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
	# sim-answered `overdraws` bool (it answers the crew's own floor), NOT `actual > sustainable`.
	# The DECOUPLING this proves: the PEAK-floor hunt has `actual 0.46 > sustainable 0.20` (a banked
	# whole animal cashed on this kill turn) yet `overdraws=false` → NO ⚠ (label reads +0.20, clean),
	# while the DRAWDOWN forage genuinely overdraws → `overdraws=true` → ⚠. The two flags are
	# independent of the floor MARK beside them, which is why one frame carries both spreads.
	var assignments := [
		# The FLOOR drives the yield label's trailing zone mark (♻ peak / ⇊ drawdown / 💀 strip /
		# ⬆ learning / ⊘ untouched) — two different ones here so the map read is verifiable in one
		# frame. `_assert_work_floor_marks` pins that they stay different.
		{"kind": "forage", "workers": 5, "target_x": FORAGE_A_X, "target_y": FORAGE_A_Y, "floor": WORK_PEAK_FLOOR, "actual_yield": 0.48, "sustainable_yield": 0.48, "overdraws": false},
		{"kind": "forage", "workers": 3, "target_x": 9, "target_y": 8, "floor": WORK_DRAWDOWN_FLOOR, "actual_yield": 0.27, "sustainable_yield": 0.20, "overdraws": true},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": WORK_PEAK_FLOOR, "target_x": 13, "target_y": 6, "actual_yield": 0.46, "sustainable_yield": 0.20, "overdraws": false},
		# THE INEDIBLE QUARRY's label (issue #337, arc #527): a hunted wolf pack pays NO food, so every
		# food field here is honestly 0. It used to fall through to a `⇄+0.22` trade rate; with that
		# account retired and no per-turn material figure on the wire, the label reads `+0.00` — the
		# honest statement that this hunt banks no commodity a one-slot label can name. The deer label
		# beside it is the control: it still reads its food rate.
		{"kind": "hunt", "workers": 2, "fauna_id": "game_wolf_03", "floor": WORK_DRAWDOWN_FLOOR, "target_x": 11, "target_y": 4, "actual_yield": 0.0, "sustainable_yield": 0.0, "overdraws": false},
		# THE SOWN HAY FIELD's label (issue #449), the same argument one account further out: a Field
		# pays FEED and no provisions, so every food field here is honestly 0 and the label falls
		# through to `+0.40 fodder`. It is also the only rendered
		# fodder label in either preview harness — `_assert_yield_label_component` pins WHICH account
		# fills the one slot, and only a frame can say whether the chosen string FITS beside its
		# neighbours (the widest run this plate has ever drawn).
		{"kind": "forage", "workers": 2, "target_x": FODDER_FIELD_X, "target_y": FODDER_FIELD_Y, "floor": WORK_PEAK_FLOOR, "actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0, "fodder_yield": FODDER_FIELD_RATE, "overdraws": false},
		{"kind": "warrior", "workers": 2},
	]
	# work_range 2 (forage green), scout radius 4 (azure) → three DISTINCT nested range borders in one
	# frame: green R2 innermost, azure R4, red hunt R5 outermost (the deer sits on the hunt border).
	var snap := _base_snapshot(_band(assignments, 2, 4), [_deer_herd(), _pelt_only_wolf_herd()])
	# BOTH worked forage tiles carry a FOOD SITE, and that is load-bearing rather than dressing: the
	# worked mark is a ring on the SOURCE's own marker (docs/plan_worked_source_marks.md §2.1), so a
	# forage assignment on a tile with no site has nothing to ring and degrades to the bare tile
	# outline. Without these two the frame could not show the green ring at all — which is exactly how
	# the first cut of this state rendered, and why the fallback is visible here as well as the ring.
	snap["food_modules"] = [
		{"x": FORAGE_A_X, "y": FORAGE_A_Y, "module": "berry_patch", "kind": "forage"},
		{"x": 9, "y": 8, "module": "berry_patch", "kind": "forage"},
		# The hay Field's own site, for the same load-bearing reason: no site, no marker to ring.
		# `savanna_grassland` rather than the berry patch beside it — grassland is where hay comes
		# from, and it resolves to a DIFFERENT bundled sprite, so the fodder tile is identifiable in
		# the frame as something other than a third berry patch.
		{"x": FODDER_FIELD_X, "y": FODDER_FIELD_Y, "module": "savanna_grassland", "kind": "forage"},
	]
	return snap

## State A-ready fixture: the same worked band, with its sources standing one rung short of the top so
## the ⌃ mark has something to offer. The deer is fully tamed with a "pen" ceiling (→ Corral); the
## first forage tile is a tended patch on willing ground (→ Sow); the wolf pack keeps its "wild"
## ceiling, so it stays unmarked and proves the mark is selective.
## The crew on the mid-Cultivate patch in `_snapshot_work_ready`. Any positive count serves — the
## badge asks whether ANYBODY is on the rung, not how many — and it is named so the unstaffed twin
## below can be spelled as its absence rather than as a second literal zero.
const WORKED_READY_BUILDERS := 2

## The band-level BUILD pool's row kind (`docs/plan_standing_upkeep.md` §2.5) — an ordinary standing
## role, spelled here rather than reached for because this harness stands in for the decoder.
const LABOR_KIND_BUILDERS := "builders"

## **THE SAME PATCH WITH ITS BUILD CREW TAKEN OFF** — the map half of the declared-but-unstaffed
## readout. The rung is still declared and its meter still holds 42% of the job, and NOTHING is
## happening: a `🌱42%` plate in the building ink says the opposite, and at a meter of zero the same
## plate reads `🌱0%`, which is pixel-identical to a build that started this turn.
func _snapshot_work_unstaffed() -> Dictionary:
	var snap := _snapshot_work_ready()
	# **THE POOL IS WHAT IS TAKEN OFF, NOT A PER-SOURCE CREW** (`docs/plan_standing_upkeep.md` §2.5).
	# The `builders` ROLE ROW is dropped from the band's assignments, which is exactly how a band with
	# nobody on the pool publishes: a row exists only where somebody is staffed.
	var kept: Array = []
	for entry_variant in snap["populations"][0]["labor_assignments"]:
		if entry_variant is Dictionary \
				and String((entry_variant as Dictionary).get("kind", "")) == LABOR_KIND_BUILDERS:
			continue
		kept.append(entry_variant)
	snap["populations"][0]["labor_assignments"] = kept
	return snap

func _snapshot_work_ready() -> Dictionary:
	var snap := _snapshot_work()
	snap["forage_patches"] = [{
		"x": FORAGE_A_X, "y": FORAGE_A_Y,
		"ecology_phase": "thriving",
		"is_cultivated": true, "is_field": false,
		# The STANDING rung, struck from this row's own state rather than written out — see
		# `fixtures_rung.gd`. The owner is struck from it too, by the loop below. The
		# `ready_for_improvement` channel reads both (a source has to be improved-or-worked, and not
		# another faction's, before it can be offered a further rung); the ⌃ badge does not.
		"current_rung": RUNG_FX.patch_rung_key(true, false),
		"sow_site_refusal": "",
		"composition": [{"species": "wild_wheat", "display_name": "Wild Wheat",
			"share": 1.0, "can_cultivate": true, "can_sow": true}],
	}, {
		# The SECOND worked tile is mid-Cultivate — the state that used to render nothing at all, so
		# a patch you were actively building looked emptier than the untouched one beside it. Its
		# assignment's improvement is switched to `cultivate` below, which is what makes it "in progress"
		# (a meter alone is a standing rung, not work in flight).
		"x": 9, "y": 8,
		"ecology_phase": "thriving",
		"is_cultivated": false, "is_field": false,
		# **STILL `plant:wild` AT 42%**, and that is the wire's own reading: a patch stands on the rung
		# it has finished, not on the one it is climbing away from.
		"current_rung": RUNG_FX.patch_rung_key(false, false),
		"cultivation_progress": 0.42,
		"sow_site_refusal": "too_dry",
		"composition": [{"species": "wild_emmer", "display_name": "Wild Emmer",
			"share": 1.0, "can_cultivate": true, "can_sow": false}],
	}]
	# **OWNERSHIP IS DERIVED, NOT DECLARED** (`_stamp_patch_owner`). Both rows above carry cultivation
	# work — one finished, one at 42% — so both come out the player's, which is what this state's
	# badges want; the point of routing them through the derivation anyway is that a row whose meters
	# are edited can never keep an owner they no longer justify.
	for patch_variant in snap["forage_patches"]:
		_stamp_patch_owner(patch_variant, MapView.PLAYER_FACTION_ID)
	for entry_variant in snap["populations"][0]["labor_assignments"]:
		var entry: Dictionary = entry_variant
		if String(entry.get("kind", "")) == "forage" and int(entry.get("target_x", -1)) == 9:
			# The BUILD BADGE keys on the IMPROVEMENT axis, not the floor (issue #442): the crew holds
			# its floor while it cultivates, and the badge reads the second field.
			entry["improvement"] = "cultivate"
			entry["overdraws"] = false
	# **AND HANDS ON THE BAND'S `builders` POOL, because a rung DECLARED with nobody building it is a
	# third state and wears a different face** (`SourceForecast.unstaffed_build_state`). It is a
	# standing ROLE ROW now, like `scout` — not a per-source count on the assignment above
	# (`docs/plan_standing_upkeep.md` §2.5) — so the fixture staffs the pool rather than the tile.
	snap["populations"][0]["labor_assignments"].append({
		"kind": LABOR_KIND_BUILDERS, "workers": WORKED_READY_BUILDERS,
		"target_x": -1, "target_y": -1, "fauna_id": "",
	})
	for herd_variant in snap["herds"]:
		var herd: Dictionary = herd_variant
		if String(herd.get("id", "")) == "game_deer_07":
			herd["domestication"] = 1.0
			herd["husbandry_ceiling"] = "pen"
			# Tamed, unpenned → `animal:pastoral`. Struck off the meters set on the two lines above, so
			# the rung and the state it is derived from move together.
			herd["current_rung"] = RUNG_FX.herd_rung_key(
				float(herd.get("domestication", 0.0)), bool(herd.get("corralled", false)))
		# **AND THE WOLF'S CEILING IS STATED, because the ABSENT one is not the one this frame
		# claims.** `SourceForecast.husbandry_ceiling` normalizes an absent field to `"pen"` — the
		# FULL ladder, so an untagged herd behaves as it did before the field existed — which made the
		# wolf offer `Tame` and wear a ⌃ of its own. The state's whole value is the CONTRAST ("a
		# chevron on every marker would prove nothing"), so the one source that must offer nothing has
		# to say so rather than rely on a default that means the opposite. Stated HERE and not on
		# `_pelt_only_wolf_herd`, so only the two frames that push knowledge move.
		elif String(herd.get("id", "")) == "game_wolf_03":
			herd["husbandry_ceiling"] = "wild"
			# Untamed → `animal:wild`, the branch FLOOR. The wolf is now held dark by two independent
			# terms (its ceiling admits no rung, and nothing has been built on it); it stays because the
			# ceiling is the one a knowledge push could otherwise open.
			herd["current_rung"] = RUNG_FX.herd_rung_key(
				float(herd.get("domestication", 0.0)), bool(herd.get("corralled", false)))
	return snap

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

## A HUNTING party and its quarry (issue #412). A hunt expedition carries its target on the COHORT
## (`expedition_target_herd`) rather than in `labor_assignments`, so before this it was the one kind of
## work the map never marked: the party walked and the map never said what it was walking to. The
## resident band beside it hunts a DIFFERENT herd locally, so the frame shows both routes to a marked
## source in one picture — and the party is still `outbound`, which is exactly when "this herd is
## already claimed" is worth knowing.
func _snapshot_hunt_expedition() -> Dictionary:
	var snap := _base_snapshot(_band([
		{"kind": "hunt", "workers": 3, "fauna_id": "game_deer_07", "floor": WORK_PEAK_FLOOR,
			"target_x": 13, "target_y": 6, "actual_yield": 0.20, "sustainable_yield": 0.20,
			"overdraws": false},
	], 2, 0), [_deer_herd(), _pelt_only_wolf_herd()])
	var party := _expedition(TRAVEL_EXPEDITION_ENTITY, 8, 5, "outbound")
	party["id"] = "Hunt Party"
	party["expedition_mission"] = "hunt"
	party["expedition_target_herd"] = "game_wolf_03"
	# The party's ORDERS ride the cohort, not a labor row — and the wire field is the floor
	# (`expedition_floor`); the retired `expeditionHuntPolicy` slot is one the sim no longer writes.
	party["expedition_floor"] = WORK_DRAWDOWN_FLOOR
	party["travel_target_x"] = 11
	party["travel_target_y"] = 4
	snap["populations"].append(party)
	return snap

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

## The crowded hex again, with its herd and its food site both WORKED and both pushed past the visible
## marker cap by the three wonders. The herd is fully tamed on a "pen" ceiling, so one of the two
## hidden sources is also READY — which is what makes the chip's `⌃` mark mean something here.
func _snapshot_mixed_worked() -> Dictionary:
	var snap := _snapshot_mixed()
	var herds: Array = snap["herds"]
	var herd: Dictionary = herds[0]
	herd["domestication"] = 1.0
	herd["husbandry_ceiling"] = "pen"
	# Tamed, unpenned → `animal:pastoral`, struck off the meter above: this is the READY half of the
	# chip's `⌃`, and a row stating no rung at all would read as a herd nothing has been built on.
	RUNG_FX.stamp_herd(herd)
	snap["populations"] = [_band([
		{"kind": "forage", "workers": 3, "target_x": BAND_X, "target_y": BAND_Y, "floor": WORK_PEAK_FLOOR,
			"actual_yield": 0.31, "sustainable_yield": 0.31, "overdraws": false},
		{"kind": "hunt", "workers": 2, "fauna_id": String(herd.get("id", "")), "floor": WORK_PEAK_FLOOR,
			"target_x": BAND_X, "target_y": BAND_Y,
			"actual_yield": 0.22, "sustainable_yield": 0.22, "overdraws": false},
	], 2, 0)]
	snap["forage_patches"] = [{
		"x": BAND_X, "y": BAND_Y,
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"current_rung": RUNG_FX.patch_rung_key(false, false),
		"sow_site_refusal": "too_dry",
		"composition": [{"species": "wild_wheat", "display_name": "Wild Wheat",
			"share": 1.0, "can_cultivate": true, "can_sow": true}],
	}]
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
## The worked band (forage tile + hunted deer, both carrying yields) parked mid-grid on a grid of the
## caller's choosing — the two ENDS of the zoom rail want the same subject at opposite scales. The
## LOD state passes YIELD_FAR_GRID (fitted hexes go tiny, so the yield labels must suppress); the
## max-zoom state passes MAX_ZOOM_GRID (the cap makes them large, so the labels must still read).
## The band sits at `_work_grid_center`, its worked forage tile one hex east and the deer two.
func _snapshot_work_on_grid(w: int, h: int) -> Dictionary:
	var terrain: Array = []
	terrain.resize(w * h)
	terrain.fill(TERRAIN_ID)
	var center := _work_grid_center(w, h)
	var cx := center.x
	var cy := center.y
	var assignments := [
		# Two DIFFERENT floors again, for the same reason the flat-grid fixture carries them: the
		# far-zoom end of the rail must SUPPRESS both marks and the max-zoom end must draw both large.
		{"kind": "forage", "workers": 5, "target_x": cx + 1, "target_y": cy, "floor": WORK_DRAWDOWN_FLOOR, "actual_yield": 0.48, "sustainable_yield": 0.48, "overdraws": false},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "floor": WORK_PEAK_FLOOR, "target_x": cx + 2, "target_y": cy, "actual_yield": 0.46, "sustainable_yield": 0.20, "overdraws": false},
	]
	var band := _with_stage({
		"entity": BAND_ENTITY, "faction": 0, "current_x": cx, "current_y": cy, "size": 30,
		"id": "Band 1", "work_range": 2, "scout_reveal_radius": 2, "labor_assignments": assignments,
	}, STAGE_NOMADIC)
	return {
		"grid": {"width": w, "height": h, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [band],
		"herds": [{"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "x": cx + 2, "y": cy, "biomass": 800.0, "huntable": true}],
	}

## Where _snapshot_work_on_grid parks the band. Shared with the max-zoom state, which has to CENTRE
## the view on that hex: at MAX_ZOOM_FACTOR the viewport holds only a handful of hexes, so an
## unpanned frame is an arbitrary corner of the map with none of the subject in it.
func _work_grid_center(w: int, h: int) -> Vector2i:
	return Vector2i(w / 2, h / 2)

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

# --- The ANNOTATION fixtures (see the CRISIS_* / ROUTE_* consts) ---------------------------------
# Written AFTER the code they cover: they encode CURRENT behaviour, so they prove "unchanged", not
# "correct".

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

## The overlay-picker backdrop: flat terrain under a west→east `sentiment` ramp, a SECOND channel
## flagged `placeholder` (so the popover renders the stub marker), a THIRD the renderer paints through
## a ramp of its own, and per-tile terrain-tag masks with
## two named bits (so the client-side `terrain_tags` row is offered at all). One fixture carrying every
## shape the popover can draw.
func _snapshot_overlay_channels() -> Dictionary:
	var total := GRID_W * GRID_H
	var normalized := PackedFloat32Array()
	normalized.resize(total)
	var raw := PackedFloat32Array()
	raw.resize(total)
	var empty := PackedFloat32Array()
	empty.resize(total)
	var tags: Array = []
	tags.resize(total)
	# SEVERAL biomes, not the usual flat fill: the bare map's legend is the BIOME KEY, so a one-biome
	# fixture would render a one-row card and the assertion over it would pass on a merge that had
	# lost every other row.
	var terrain: Array = []
	terrain.resize(total)
	for i in total:
		var col := i % GRID_W
		var pressure := float(col) / float(GRID_W - 1)
		normalized[i] = pressure
		raw[i] = pressure * PICKER_LIVE_RAW_SCALE
		# Split the map between the two tags so the tag legend counts both.
		tags[i] = PICKER_TAG_BIT_A if col < GRID_W / 2 else PICKER_TAG_BIT_B
		terrain[i] = PICKER_BIOME_IDS[(col * GRID_H + i / GRID_W) % PICKER_BIOME_IDS.size()]
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {
			"terrain": terrain,
			"terrain_tags": tags,
			"terrain_tag_labels": {
				PICKER_TAG_BIT_A: PICKER_TAG_LABEL_A,
				PICKER_TAG_BIT_B: PICKER_TAG_LABEL_B,
			},
			"channels": {
				PICKER_CHANNEL_LIVE: {
					"label": PICKER_CHANNEL_LIVE_LABEL,
					"description": "Morale and agency composite, staged west to east.",
					"normalized": normalized,
					"raw": raw,
				},
				# A channel the sim publishes a SHAPE for and no telemetry behind — the state the
				# `stub data` marker exists to report, and the reason the marker is a descriptor field
				# rather than a hand-written tab per channel.
				PICKER_CHANNEL_STUB: {
					"label": PICKER_CHANNEL_STUB_LABEL,
					"description": "Composite of garrison morale, manpower, and supply margin.",
					"normalized": empty,
					"raw": empty,
					"placeholder": true,
				},
				# A channel the RENDERER paints through a ramp of its own. The same west→east values
				# as the live channel, so its richest column reaches the ramp's top colour and the map
				# really does paint the tint the legend button claims for it.
				PICKER_CHANNEL_OWN_RAMP: {
					"label": PICKER_CHANNEL_OWN_RAMP_LABEL,
					"description": "Human-edible potential — seeds, nuts, tubers, fruit, and fish.",
					"normalized": normalized,
					"raw": raw,
				},
			},
			"channel_order": PackedStringArray([PICKER_CHANNEL_LIVE, PICKER_CHANNEL_STUB,
				PICKER_CHANNEL_OWN_RAMP]),
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
