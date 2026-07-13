extends Node2D

## Dev-only FLAT↔FLAT BLEND probe, rendered at the GAME's on-screen hex radius.
##
## The other harnesses fit their grid to a small window, which lands hex radius well away from what the game
## actually runs at — and the blend's look is radius-relative, so every judgement made in a fitted frame has
## been wrong. This harness pins the radius instead: a 1:1 1920×1080 canvas + a grid sized so _fit_map_to_view
## lands on the target radius (it prints the achieved radius, which is the number to quote).
##
## Two states:
##   1. BAND STRIP (r ≈ 45) — a strip of flat biomes; every adjacent pair is a flat↔flat seam. Useful for
##      reading a single straight seam, but it CANNOT expose hex shredding: a straight band seam looks fine
##      even when the blend is destroying hexes.
##   2. ISOLATED HEXES (r ≈ 75, the user's on-screen size) — prairie hexes each SURROUNDED ON ALL SIX SIDES
##      by dark rocky soil. This is the user's real map, and it is the ONLY state that shows whether the
##      blend keeps a hex intact or tears holes in its interior. Every blend change MUST be judged here.
##      Rendered once per tuning variant (the V6 sweep) plus a labelled contact sheet.
##
##   godot --path . res://tools/blend_probe.tscn     (NOT --headless — the dummy renderer can't read back)
##
## then read ui_preview_out/blend_*.png and ui_preview_out/V6_*.png.

const MAP_VIEW := preload("res://src/scripts/MapView.gd")
const OUT_DIR := "res://ui_preview_out"

# 1:1 canvas at the game's window size. Pointy-top odd-r cover-fit radius:
#   r = max(vw / (√3·(W+0.5)), vh / (1.5·(H−1)+2))
# so the grid dims below are chosen to land each state on its target radius.
const CANVAS_SIZE := Vector2i(1920, 1080)
const HEX_RADIUS_TOLERANCE := 2.5

# --- state 1: the flat-biome band strip (24×16 at 1920×1080 → r ≈ 45) ---
const GRID_W := 24
const GRID_H := 16
const GAME_HEX_RADIUS := 45.0
# Flat biomes only — every adjacent pair is a flat↔flat seam, so one frame exercises the whole blend.
# Desert and prairie are adjacent (bands 0 and 1) because that pair is the arc's reference seam.
const FLAT_BAND_IDS := [15, 11, 17, 10, 20, 18]  # desert · prairie · scrub · alluvial · tundra · salt flat
const SEAM_BAND_INDEX := 1        # the desert↔prairie seam sits at the left edge of band 1
const SEAM_ROW := 8               # mid-height row, so the close-up is clear of the frame edges
const SEAM_CROP_RADII := 4.0      # crop half-size in hex radii → ~8 hexes across at the game radius

# --- state 2: isolated prairie hexes in a field of dark rocky soil (the USER'S situation) ---
# 14×10 at 1920×1080 → r ≈ 76, matching the ~150px-across hexes on the user's screen.
const ISO_GRID_W := 14
const ISO_GRID_H := 10
const ISO_HEX_RADIUS := 75.0
const ISO_FIELD_ID := 16          # rocky_regolith — the dark soil the user's prairie sits in
const ISO_ISLAND_ID := 11         # prairie — the hex that must stay INTACT
# Prairie only on even rows / even cols: odd-r neighbours are all ±1 row or ±1 col, so this spacing
# guarantees every prairie hex is surrounded on ALL SIX sides by soil (no prairie↔prairie edge exists).
const ISO_ISLAND_ROW_STRIDE := 2
const ISO_ISLAND_COL_STRIDE := 2
# Native-resolution close-up on ONE isolated prairie hex (the full frame is downscaled when viewed, which
# can hide a ragged edge). Centred on an island well clear of the frame edges.
const ISO_CROP_COL := 6
const ISO_CROP_ROW := 4
const ISO_CROP_RADII := 1.6        # crop half-size in hex radii → the hex plus its full soil collar

# --- the V6 tuning sweep, rendered on the isolated-hex state ---
# Each entry: the terrain_config blend levers to override, the output name, and a human label for the sheet.
const V6_VARIANTS := [
	{
		"name": "V6_A_feather",
		"label": "A  soft feather — soft .35 · noise .30/.25 · width .25 · height 0",
		"overrides": {
			"blend_soft": 0.35, "blend_noise_amount": 0.30, "blend_noise_scale": 0.25,
			"blend_width": 0.25, "blend_height_influence": 0.0,
		},
	},
	{
		"name": "V6_B_speckle",
		"label": "B  fine speckle — soft .03 · noise 1.0/.05 · width .20 · height 0",
		"overrides": {
			"blend_soft": 0.03, "blend_noise_amount": 1.0, "blend_noise_scale": 0.05,
			"blend_width": 0.20, "blend_height_influence": 0.0,
		},
	},
	{
		"name": "V6_C_both",
		"label": "C  feather + speckle — soft .18 · noise .6/.10 · width .22 · height 0",
		"overrides": {
			"blend_soft": 0.18, "blend_noise_amount": 0.6, "blend_noise_scale": 0.10,
			"blend_width": 0.22, "blend_height_influence": 0.0,
		},
	},
	{
		"name": "V6_D_detail",
		"label": "D  feather + detail-follow — A, but height influence .25",
		"overrides": {
			"blend_soft": 0.35, "blend_noise_amount": 0.30, "blend_noise_scale": 0.25,
			"blend_width": 0.25, "blend_height_influence": 0.25,
		},
	},
]

# --- state 3 (V7): WATER↔WATER — an irregular deep-ocean region embedded in continental shelf ---
# Same 14×10 / r ≈ 75 framing as the isolated-hex state (the user's on-screen hex size). The deep-ocean
# region is deliberately RAGGED (and includes two fully-isolated deep hexes) — a straight band seam cannot
# show whether a hex silhouette survives, exactly as with the flat↔flat state.
const WATER_GRID_W := 14
const WATER_GRID_H := 10
const WATER_HEX_RADIUS := 75.0
const WATER_SHELF_ID := 1          # continental_shelf — the surrounding water
const WATER_DEEP_ID := 0           # deep_ocean — the embedded deeper region
# Offset (col,row) hexes that are deep ocean; everything else is shelf. Ragged blob + two isolated hexes.
const WATER_DEEP_HEXES := [
	Vector2i(6, 2), Vector2i(7, 2),
	Vector2i(5, 3), Vector2i(6, 3), Vector2i(7, 3), Vector2i(8, 3),
	Vector2i(4, 4), Vector2i(5, 4), Vector2i(6, 4), Vector2i(7, 4), Vector2i(8, 4), Vector2i(9, 4),
	Vector2i(5, 5), Vector2i(6, 5), Vector2i(7, 5), Vector2i(8, 5),
	Vector2i(6, 6), Vector2i(7, 6),
	Vector2i(7, 7),
	Vector2i(11, 3),               # isolated deep hex (all six neighbours are shelf)
	Vector2i(3, 7),                # isolated deep hex
]
# Close-up straddles the blob's west edge: shelf → deep seam plus a deep interior.
const WATER_CROP_COL := 4
const WATER_CROP_ROW := 4
const WATER_CROP_RADII := 2.2
# The two candidate water lever sets, both applied by overriding the config's "water_blend" block:
#   W1 — water reuses the LAND levers (i.e. no per-class override at all).
#   W2 — the wider/softer/wobblier water set (ocean depth grades gradually, and smooth water gives the
#        height term nothing to interlock on, so only a bigger wobble can dissolve the hex silhouette).
# W2 is what ships (it mirrors terrain_config's "water_blend" / MapView's WATER_BLEND_DEFAULT_*).
const WATER_W1_OVERRIDES := {
	"water_blend": {"blend_width": 0.25, "blend_soft": 0.35, "blend_noise_amount": 0.30},
}
const WATER_W2_OVERRIDES := {
	"water_blend": {"blend_width": 0.45, "blend_soft": 0.45, "blend_noise_amount": 0.45},
}

# --- state 4 (V7): COAST (land↔water) — the shoreline pass must stay BIT-IDENTICAL across the change ---
# Only ONE water id is present, so nothing here can exercise the new water↔water path: any pixel difference
# vs. the pre-change render is a shoreline/flat-blend regression. An inland flat↔flat seam (prairie↔desert)
# is included so that path is covered by the same diff.
const COAST_WATER_ID := 1          # continental_shelf
const COAST_SHORE_ID := 11         # prairie — the coastal land band
const COAST_INLAND_ID := 15        # desert — inland, so prairie↔desert is a flat↔flat seam in-frame
const COAST_SHORE_BASE_COL := 5
const COAST_SHORE_WOBBLE := [0, 1, 2, 1, 0, -1, 0, 1, 2, 1]  # per-row, so the coastline is ragged
const COAST_SHORE_BAND_COLS := 3   # width of the prairie coastal band before desert takes over

# --- state 5 (V8): FOG OF WAR vs. the water↔water blend — which draws the hard straight edges? ---
# The FoW tint is applied PER HEX from a NEAREST-sampled vis-map (0 unexplored / 0.5 discovered /
# 1 active), so a discovered (misty) hex beside an active one has a HARD HEX-SHAPED tint boundary that
# has nothing to do with terrain. This pair isolates that: the SAME deep-ocean-in-shelf terrain, once
# with FoW off (all active) and once with a mix of active + discovered hexes. NOTHING is unexplored, so
# the two frames differ ONLY in the mist tint — any hard edge present in (a) is the blend's fault, and
# any hard edge that appears only in (b) is the FoW tint's.
const V8_VIS_ACTIVE := 1.0
const V8_VIS_DISCOVERED := 0.5
const V8_ACTIVE_CENTER := Vector2i(6, 4)   # the "band's sight" blob, inside the deep-ocean region
const V8_ACTIVE_RADIUS := 2                # hexes within this hex-distance of the centre are Active

# --- state 8 (W): the FoW hex-step, BEFORE vs AFTER the boundary softening ---
# The "hard straight full-hexagon edges are back in open water" report. The water↔water blend is NOT the
# culprit (W_fow_off proves it: same terrain, no FoW, no steps). The culprit is the FoW tint: the vis-map is
# per-hex and NEAREST-sampled, so an active↔discovered adjacency draws a hex-shaped BRIGHTNESS step that is
# not a terrain seam at all — it cuts across uniform water, including water of the SAME terrain id.
# The three frames share one camera + the V8 terrain/visibility, and differ ONLY in `fow_softness`:
#   W_fow_off   — FoW off (every hex Active). The terrain-only reference.
#   W_fow_on    — FoW on with softness FOW_SOFTNESS_UNSMOOTHED, which reproduces the UNSMOOTHED per-hex tint
#                 that ships on main (the smoothing reach collapses to the shader's BLEND_SOFT_MIN floor, so
#                 `vis` is the raw per-hex value — and the continuous tint map is bit-identical at the pure
#                 states). This is the frame that must show the user's hexagonal steps.
#   W_fow_fixed — FoW on with the shipped softness. Same mist, same pure states, no hex steps.
# Judge on the CLOSE-UPs: the full frame is downscaled when viewed, which hides the very step in question.
const W_FOW_OFF_NAME := "W_fow_off"
const W_FOW_ON_NAME := "W_fow_on"
const W_FOW_FIXED_NAME := "W_fow_fixed"
const W_FOW_SOFTNESS_UNSMOOTHED := 0.0   # main's behaviour: no cross-edge smoothing → the raw per-hex step
const W_FOW_NOISE_UNSMOOTHED := 0.0      # …and no wisps, so the step is the ONLY thing under test
# Straddles the Active/Discovered boundary where it crosses SAME-terrain (deep-ocean) water — the step that
# cannot be blamed on any terrain seam.
const W_CROP_COL := 4
const W_CROP_ROW := 4
const W_CROP_RADII := 2.2
# The SAME-TERRAIN crop — the one that proves the step is not a terrain seam at all. Hex (4,3) is Active and
# its neighbour (3,3) is Discovered, and BOTH are continental shelf (neither is in WATER_DEEP_HEXES), so the
# only thing that can draw an edge between them is the FoW tint. This is the user's "…and it looks like also
# between water hexes of the SAME terrain".
const W_SAME_CROP_COL := 4
const W_SAME_CROP_ROW := 3
const W_SAME_CROP_RADII := 1.8

# --- state 6 (V10): SHORELINE — sand on the LAND ONLY, the surf washing up over it ---
# Rendered on state 4's terrain at r≈75. This is the frame every "is there a hard line anywhere on the
# coast?" call is made on, and it has caught three rejected passes: (1) a two-sided pass whose land beach and
# water foam both saturated AT the shared edge (hard tan↔white line tracing the hexagon); (2) an
# all-on-the-water-side pass whose sand stopped DEAD at the edge against the raw land texture (hard sand↔land
# line); (3) sand on BOTH sides, which had no hard line left but read TWICE AS WIDE — sand in the water hex is
# not wanted. The shipped profile keeps the sand strictly on the LAND side (fading inland) and blends the waves
# into it by letting the surf wash INLAND over the beach (`shore.foam_inland_width`) as well as out to sea.
# Rendered with the SHIPPED config (no overrides — the levers are `shore.sand_width` / `foam_inland_width` /
# `foam_width`; `_render_variant` can still sweep them). Judge on the CLOSE-UP: the full frame is downscaled
# when viewed, which hides a 1px line.
const V10_SHORE_NAME := "V10_shore"
const V10_SHORE_CROP_COL := 6
const V10_SHORE_CROP_ROW := 4
const V10_SHORE_CROP_RADII := 2.2
# The same coast against a DARK land biome: prairie is tan, so it HIDES sand-vs-land contrast — a dark land
# is the frame that shows how far the beach actually reaches inland.
const V10_SHORE_DARK_NAME := "V10_shore_dark_land"
const V10_DARK_LAND_ID := 16      # rocky_reg — dark brown, maximal contrast against the tan beach
# A/B contact sheet: the REJECTED sand-on-both-sides frame (rendered from the previous shader and left in
# OUT_DIR under this name) beside the shipped land-only one. Missing file → the sheet just skips the cell.
const V10_AB_NAME := "V10_shore_ab"
const V10_REJECTED_NAME := "V10_shore_rejected"

# --- state 7 (V11): SURF-REACH / WISP sweep — "the white foam dominates the map" ---
# Same DARK-land coast (rocky_regolith — prairie's tan camouflages the foam) at the game's r ≈ 75, WIDE shot
# (the full frame, several hexes of coastline) so the question actually being asked — how much of the sea is
# white? — is judgeable. Every frame uses the same camera/crop, so they are directly comparable.
# Only the surf's SEAWARD reach and the second wisp's geometry vary; `sand_width` / `foam_inland_width` are
# taken from the SHIPPED config in every frame (see _shore_sweep_overrides) — the beach is signed off and
# must be bit-identical across the sweep.
# A reproduces the OLD look through the new levers: the wisp used to be a multiple of foam_band
# (centre 1.35 × 0.55 = 0.74·r, half 0.35 × 0.55 = 0.19·r), so those numbers ARE the old ring.
const V11_SHORE_VARIANTS := [
	{"name": "A_current", "foam_width": 0.55, "wisp_center_width": 0.74, "wisp_half_width": 0.19},
	{"name": "B_proposed", "foam_width": 0.41, "wisp_center_width": 0.55, "wisp_half_width": 0.13},
	{"name": "C_tighter", "foam_width": 0.41, "wisp_center_width": 0.47, "wisp_half_width": 0.09},
	{"name": "D_no_wisp", "foam_width": 0.41, "wisp_center_width": 0.55, "wisp_half_width": 0.0},
]

# --- state 10 (L): the PER-TERRAIN shore profile on a SMALL INLAND SEA (the lake) ---
# The shipped shore profile (sand → surf → offshore wisp) was tuned on an OCEAN coast, where its reaches are a
# small fraction of a huge body of water. An `inland_sea` is typically a HANDFUL of hexes, and the same profile
# swamps it — in particular the second offshore wisp reads as noise on a lake. `shore_profile` (a per-terrain
# block on the WATER terrain, → the shader's `layer_shore_map`) scales the profile per water body; this state
# is where the lake variants are chosen. A real lake shape (7 hexes) in a field of DARK land — prairie's tan
# camouflages both the sand and the foam, so a lighter coast cannot be judged on it (the same trap the
# invisible-beach bug fell into). Same camera + crop in every frame, at the game's r ≈ 75.
const LAKE_WATER_ID := 2           # inland_sea — the terrain whose shore_profile is under test
const LAKE_LAND_ID := 16           # rocky_regolith — dark land, so sand + foam are actually visible
# A rounded 7-hex blob (offset col,row): a plausible lake, NOT an open-water expanse.
const LAKE_HEXES := [
	Vector2i(6, 3),
	Vector2i(5, 4), Vector2i(6, 4), Vector2i(7, 4),
	Vector2i(6, 5), Vector2i(7, 5),
	Vector2i(6, 6),
]
const LAKE_CROP_COL := 6
const LAKE_CROP_ROW := 4
const LAKE_CROP_RADII := 3.4       # the whole lake plus its land collar, at native resolution
# The four candidate profiles. reach_scale multiplies ALL FOUR shore reaches (sand / foam_inland / foam /
# wisp centre+half); wisp_scale multiplies the wisp's half-width AND its strength (0 = no second disturbance).
# L4 ("shrink the whole thing"): the profile's OUTERMOST reach is the wisp's far edge, wisp_center +
# wisp_half = 0.55 + 0.13 = 0.68·r. To land the lake's total shore disturbance at ~10% of a hex radius:
# reach_scale = 0.10 / 0.68 = 0.147 → total seaward reach 0.68 × 0.147 = 0.0999·r ≈ 0.10·r (with the wisp KEPT).
const LAKE_TOTAL_REACH_TARGET := 0.10                  # fraction of a hex radius the whole profile may reach
const LAKE_SHIPPED_OUTER_REACH := 0.68                 # = shore.wisp_center_width + shore.wisp_half_width
const LAKE_TENTH_REACH_SCALE := LAKE_TOTAL_REACH_TARGET / LAKE_SHIPPED_OUTER_REACH  # ≈ 0.147
const LAKE_VARIANTS := [
	{"name": "L1_current", "reach_scale": 1.0, "wisp_scale": 1.0},   # today's GLOBAL profile = the BEFORE
	{"name": "L2_no_wisp", "reach_scale": 1.0, "wisp_scale": 0.0},   # kill the wisp, keep sand + surf
	{"name": "L3_half", "reach_scale": 0.5, "wisp_scale": 0.0},      # lighter coast AND no wisp
	{"name": "L4_tenth", "reach_scale": LAKE_TENTH_REACH_SCALE, "wisp_scale": 1.0},  # whole profile → ~10%·r
]

# --- state 11 (H): ROLLING HILLS — "the hills are CUT OFF at the hex edge" ---
# rolling_hills (24) is a PEAK biome: its base texture is a plain grass FLOOR and the mounds live in the
# `peaks/` overlay. It is also blend_class `rugged`, and rugged never blends in the base seam pass — so the
# grass floor ends in a razor-straight hexagon against its neighbour while the mound overlay overhangs.
# Two candidate causes, and this state is what tells them apart (one camera, one crop set, every frame):
#   H1 — the mound OVERHANG is too weak/short to read, so the mounds look sliced at the hex line.
#   H2 — the mounds overhang fine and what is actually cut is the BASE GRASS FLOOR under them.
# H_base_only (peaks LOD pushed above the render radius, so the peak pass is skipped) isolates the base and
# is decisive for H2; the H_before − H_base_only pixel diff is exactly the peak pass's footprint and is
# decisive for H1 (it shows whether the mounds paint across the hex boundary at all).
# High-contrast neighbours, as in the user's screenshot: rocky_reg (16, dark brown) west, prairie (11, tan)
# east — so the hills' green floor is cut against BOTH a darker and a lighter biome in one frame.
const HILLS_GRID_W := 14
const HILLS_GRID_H := 10
const HILLS_HEX_RADIUS := 75.0
const HILLS_ID := 24               # rolling_hills — the peak biome under test
const HILLS_WEST_FIELD_ID := 16    # rocky_reg — dark brown
const HILLS_EAST_FIELD_ID := 11    # prairie_steppe — tan
const HILLS_FIELD_SPLIT_COL := 7   # cols < split are the west field, cols >= split the east field
# A blob straddling the field split (so one frame carries hills↔rocky AND hills↔prairie seams) …
const HILLS_BLOB_HEXES := [
	Vector2i(5, 3), Vector2i(6, 3), Vector2i(7, 3), Vector2i(8, 3),
	Vector2i(4, 4), Vector2i(5, 4), Vector2i(6, 4), Vector2i(7, 4), Vector2i(8, 4),
	Vector2i(5, 5), Vector2i(6, 5), Vector2i(7, 5),
]
# … plus ISOLATED hills hexes (all six neighbours are the field biome). MANDATORY for any base-blend change:
# a straight band seam looks fine even when the blend is shredding hex interiors — only a surrounded hex
# shows it (that is how the shredding regression shipped). One in each field.
const HILLS_ISO_ROCKY := Vector2i(2, 7)
const HILLS_ISO_PRAIRIE := Vector2i(11, 7)
# …and an isolated ALPINE hex, because the rugged gate is GLOBAL: turning it on lets EVERY rugged biome's base
# blend, and alpine is the high-contrast/structured texture the height term is most likely to shred. rolling_hills'
# base is a plain grass floor and would never expose that, so the gate must also be judged here.
const HILLS_ISO_ALPINE := Vector2i(12, 5)   # mid-frame, so its crop is never clipped by the frame edge
const HILLS_ISO_ALPINE_ID := 26    # alpine_mountain — structured rock, the shred-risk case
# Crops (native resolution — the downscaled full frame hides exactly the edge under judgement).
const HILLS_SEAM_CROP_RADII := 2.4   # the blob's west edge: hills floor + mounds against dark rocky_reg
const HILLS_SEAM_CROP := Vector2i(4, 4)
const HILLS_ISO_CROP_RADII := 1.7    # one isolated hills hex plus its full collar of field biome
# Peaks OFF: `peak_min_radius` is the peak pass's LOD floor in px, so a value far above any on-screen hex
# radius makes `peaks_lod_enabled` false → the whole peak pass is skipped. No shader edit needed.
const HILLS_PEAKS_OFF_MIN_RADIUS := 100000.0
# Candidate fix 1 — a LONGER, SOFTER mound overhang, so crowns of the mound field clearly spill across the
# boundary instead of stopping at it (shipped: overhang 0.6 · softness 0.4, both fractions of the radius).
const HILLS_FIX_OVERHANG_WIDTH := 1.2
const HILLS_FIX_OVERHANG_SOFTNESS := 0.5
# Candidate fix 2 — let the rugged BASE floor blend (the new `blend_rugged_land` gate; shipped default false).
const HILLS_FIX_BASE_KEY := "blend_rugged_land"
# The diff frames: |before − base_only|, amplified so the peak pass's footprint is legible. Any painted pixel
# OUTSIDE a hills hexagon is mound overhang; a footprint that stops dead at the hex line is H1.
const HILLS_DIFF_GAIN := 6.0
# The rugged gate must not move any NON-hills seam. These two frames are re-rendered with the gate ON and
# byte-compared against the shipped ones (`blend_bands_full` / `V7_coast_unchanged`): a flat↔flat band strip
# and the ragged coast contain no rugged hex, so they must come out bit-identical.
const HILLS_GATE_BANDS_NAME := "H_gate_bands_full"
const HILLS_GATE_COAST_NAME := "H_gate_coast"

# Contact-sheet layout (a 2×2 grid of the sweep frames, each captioned).
const SHEET_COLS := 2
const SHEET_BG := Color(0.06, 0.06, 0.08)
const SHEET_CAPTION_HEIGHT := 34.0
const SHEET_CAPTION_FONT_SIZE := 20
const SHEET_PADDING := 8.0
const SHEET_NAME := "V6_sheet"
const SHEET_LAYER := 200   # above MapView's minimap CanvasLayer (102), which is not hidden with the map

# --- state 9 (X): the DARK-WATER report, on REAL GAME TERRAIN (not a synthetic blob) ---
# "Patches of open water render noticeably DARKER, with hard full-hexagon edges" (FoW OFF). The synthetic
# water state above never reproduced it because its deep-ocean region is one CLEAN ragged blob: a large
# same-id interior with a single boundary. The real map's ocean is nothing like that — it is SALT-AND-PEPPER
# (dumped from a live snapshot's id-map: 2332 deep↔shelf hex adjacencies on one 80×52 map, 16 deep hexes
# with SIX different-id water neighbours). A lone deep_ocean hex ringed by continental_shelf can only ever
# read as a dark HEXAGON: the seam blend feathers its rim, but its interior keeps the (much darker) deep
# texture and its silhouette is the hex. That is the reported artifact, and it is TERRAIN, not a FoW tint.
# X_WATER_IDS is a verbatim 14×10 window of that live id-map (ids: 0 deep_ocean · 1 continental_shelf ·
# 2 inland_sea · 9/10/11/14/20 land), rendered at the game's r ≈ 75 with FoW OFF.
const X_WATER_NAME := "X_dark_water"
const X_WATER_GRID_W := 14
const X_WATER_GRID_H := 10
const X_WATER_IDS := [
	[1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1],
	[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
	[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1],
	[1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 1, 1, 14],
	[1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 1, 1, 1, 2],
	[1, 1, 1, 0, 1, 1, 1, 14, 1, 0, 1, 1, 1, 20],
	[1, 0, 0, 0, 1, 9, 9, 2, 1, 0, 1, 10, 11, 10],
	[1, 1, 1, 1, 1, 1, 1, 10, 1, 0, 1, 10, 1, 1],
	[1, 1, 1, 1, 1, 1, 10, 10, 1, 1, 1, 14, 14, 1],
	[1, 0, 0, 0, 1, 1, 1, 10, 10, 2, 14, 11, 11, 14],
]
# Close-up on the shelf field around (1..3, 6) — three deep hexes sitting in shelf, the exact "dark hexagon"
# the report is about.
const X_WATER_CROP_COL := 2
const X_WATER_CROP_ROW := 5
const X_WATER_CROP_RADII := 2.6

# --- state 12 (R): the RUGGED-GATE SWEEP — every rugged biome as an ISOLATED hex, gate ON ---
# `blend_rugged_land` is a GLOBAL gate: flipping it lets EVERY rugged biome's base floor blend, not just
# rolling_hills. The failure mode it can reintroduce is SHREDDING (the height term punching holes deep inside a
# hex interior, neighbour texture leaking far in, winner-takes-all-by-luminance patches) — and a straight band
# seam CANNOT show it. So every rugged biome gets the mandatory case: ONE hex of it, ISOLATED (all six
# neighbours are a contrasting biome), at the game's r ≈ 75 with the grid overlay OFF, cropped at native res.
# Two fields, because the gate widens eligibility to BOTH flat↔rugged and rugged↔rugged:
#   R1 — a flat field, dark rocky_reg west / tan prairie east (the flat↔rugged case, high contrast both ways).
#   R2 — a RUGGED field of canyon_badlands (dark, high-variance structured rock: the harshest rugged↔rugged
#        partner) with the peak/canopy rugged biomes dropped in.
const R_GRID_W := 14
const R_GRID_H := 10
const R_HEX_RADIUS := 75.0
const R_WEST_FIELD_ID := 16        # rocky_reg — dark brown
const R_EAST_FIELD_ID := 11        # prairie_steppe — tan
const R_FIELD_SPLIT_COL := 7       # cols < split are the west field, cols >= split the east field
const R_RUGGED_FIELD_ID := 28      # canyon_badlands — the rugged↔rugged partner
const R_CROP_RADII := 1.7          # one isolated hex plus its full collar of field biome
# Isolated-hex slots: EVEN col and EVEN row, so no two subjects are ever neighbours (odd-r offset: every
# neighbour of an (even, even) hex has an odd col or an odd row). Kept off the frame edge so no crop clips.
const R_SLOT_COLS := [2, 4, 6, 8, 10, 12]
const R_SLOT_ROWS := [2, 4, 6]
# The biomes under test. alpine_mountain (26) and rolling_hills (24) already passed in state H, so they are
# only re-checked in the rugged field (R2), where their partner is structured rock rather than flat soil.
const R_SWEEP_FLAT := [
	{"id": 28, "name": "canyon_badlands"},
	{"id": 30, "name": "basaltic_lava_field"},
	{"id": 29, "name": "active_volcano_slope"},
	{"id": 27, "name": "karst_highland"},
	{"id": 13, "name": "boreal_taiga"},
	{"id": 7, "name": "mangrove_swamp"},
	{"id": 25, "name": "high_plateau"},
	{"id": 19, "name": "oasis_basin"},
	{"id": 32, "name": "fumarole_basin"},
	{"id": 33, "name": "impact_crater_field"},
	{"id": 34, "name": "karst_cavern_mouth"},
	{"id": 35, "name": "sinkhole_field"},
	{"id": 36, "name": "aquifer_ceiling"},
]
const R_SWEEP_RUGGED = [
	{"id": 26, "name": "alpine_mountain"},
	{"id": 24, "name": "rolling_hills"},
	{"id": 30, "name": "basaltic_lava_field"},
	{"id": 13, "name": "boreal_taiga"},
	{"id": 27, "name": "karst_highland"},
	{"id": 7, "name": "mangrove_swamp"},
]

# --- state 13 (S): the PEAK CAST-SHADOW HEXAGONS ---
# The peak block darkens the ground wherever `peak_code > 0` — which is true for any hex merely ADJACENT to
# relief — and the mound texture is near-opaque almost everywhere, so the occlusion term is roughly CONSTANT
# across that whole neighbour hex and then terminates on the hex's own boundary: a dark HEXAGON painted into
# the neighbouring biome. This state renders exactly that: an alpine massif + an isolated rolling_hills hex in
# a flat rocky_reg field (a light, uniform-ish floor is where a hex-shaped darkening is most legible), r ≈ 75,
# grid overlay OFF. The fix must kill the hexagons while KEEPING a directional cast shadow (long down-light of
# the massif, short up-light of it), so both crops straddle the massif's light axis.
const S_GRID_W := 14
const S_GRID_H := 10
const S_HEX_RADIUS := 75.0
const S_FIELD_ID := 11             # prairie_steppe — light tan, so the cast shadow reads clearly
const S_MASSIF_ID := 26            # alpine_mountain
const S_ISO_ID := 24               # rolling_hills — the single-hex case (its shadow hexagon has no massif to hide in)
const S_MASSIF_HEXES := [
	Vector2i(5, 3), Vector2i(6, 3),
	Vector2i(5, 4), Vector2i(6, 4), Vector2i(7, 4),
	Vector2i(6, 5),
]
const S_ISO_HEX := Vector2i(11, 6)
const S_NAME := "S_shadow"
const S_NOCAST_NAME := "S_shadow_nocast"   # the same frame with the cast shadow switched off, for the diff
const S_NO_SHADOW_STRENGTH := 0.0          # `peaks.shadow_strength` 0 = no cast shadow at all
# The massif plus its whole ring of neighbours: the light comes from the top-left, so the up-light neighbours
# must stay clean and the down-light ones must carry a shadow that FADES rather than filling their hexagon.
const S_CROP := Vector2i(6, 4)
const S_CROP_RADII := 2.8
# The isolated hills hex plus its collar — the decisive crop: one hex of relief, six neighbours, and any
# hex-shaped darkening in them has nowhere to hide.
const S_ISO_CROP_RADII := 2.0

var _map: Node2D
# The inland_sea `shore_profile` as SHIPPED in terrain_config, captured before the lake sweep overrides it.
var _shipped_lake_profile: Dictionary = {}


func _ready() -> void:
	var win := get_window()
	_pin_canvas(win)
	DirAccess.make_dir_absolute(OUT_DIR)
	_map = MAP_VIEW.new()
	add_child(_map)
	await get_tree().process_frame
	await get_tree().process_frame
	# Re-assert: project.godot opens the window MAXIMIZED (window/size/mode=3) and the WM applies that a few
	# frames in — AFTER _ready's first size assignment. That silently defeats the whole point of this harness:
	# the viewport becomes the monitor, _fit_map_to_view lands r ≈ 154 instead of the game's ~75 (the blend is
	# radius-relative, so the frames stop being an honest proxy), and the taller states overflow the canvas so
	# the native-res close-ups clip. Pin it again once the mode change has settled.
	_pin_canvas(win)
	await get_tree().process_frame
	await get_tree().process_frame

	_map.set_fow_enabled(false)
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false               # the shader path bypasses the cache anyway
	# DETERMINISM: the probe renders in a REAL window, so MapView's _unhandled_input would pick up the OS
	# cursor and draw a faint HOVER hex outline wherever the mouse happens to sit — a run-to-run difference of
	# a few thousand pixels that silently defeats the pixel-diff this harness exists to support (it is exactly
	# the magnitude of a shore-profile regression). No frame here is driven by input, so drop input entirely.
	_map.set_process_unhandled_input(false)
	var lake_entry: Dictionary = _lake_terrain_entry()
	_shipped_lake_profile = (lake_entry.get("shore_profile", {}) as Dictionary).duplicate(true)

	# --- state 1: the straight flat↔flat band seam, at the game's r ≈ 45 ---
	_map.display_snapshot(_snapshot_flat_bands())
	await _refit(GAME_HEX_RADIUS)
	await _save("blend_bands_full")
	await _save_seam_crop("blend_bands_seam")

	# --- state 2: isolated prairie hexes surrounded by dark soil, at the user's r ≈ 75 ---
	# The state that exposes hex shredding. Every tuning variant is rendered here.
	_map.display_snapshot(_snapshot_isolated_islands())
	await _refit(ISO_HEX_RADIUS)

	var sweep_names: Array[String] = []
	var sweep_labels: Array[String] = []
	for variant: Dictionary in V6_VARIANTS:
		await _render_variant(variant["overrides"], variant["name"])
		sweep_names.append(variant["name"])
		sweep_labels.append(variant["label"])
	await _save_contact_sheet(sweep_names, sweep_labels, SHEET_NAME)

	# The SHIPPED terrain_config values, rendered last: the very first capture after a window resize can
	# read back at the pre-HiDPI-scale resolution, which would make this frame incomparable to the sweep.
	await _settle()
	await _save("blend_isolated_shipped")
	await _save_crop("blend_isolated_shipped_closeup", ISO_CROP_COL, ISO_CROP_ROW, ISO_CROP_RADII)

	# --- state 3 (V7): WATER↔WATER — deep ocean embedded in continental shelf, at the user's r ≈ 75 ---
	# W1 = the shipped (land-tuned) levers; W2 = the wider/softer water hypothesis.
	_map.display_snapshot(_snapshot_water_patch())
	await _refit(WATER_HEX_RADIUS)
	await _render_variant(
		WATER_W1_OVERRIDES, "V7_water_W1", WATER_CROP_COL, WATER_CROP_ROW, WATER_CROP_RADII
	)
	await _render_variant(
		WATER_W2_OVERRIDES, "V7_water_W2", WATER_CROP_COL, WATER_CROP_ROW, WATER_CROP_RADII
	)

	# --- state 4 (V7): COAST (land↔water) — the shoreline reference frame, pixel-diffed across changes ---
	_map.display_snapshot(_snapshot_coast())
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save("V7_coast_unchanged")

	# --- state 5 (V8): the same water patch, FoW OFF vs FoW ON (active + discovered mix) ---
	# Same terrain, same levers — the only difference is the per-hex mist tint. See the const block.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_water_patch())
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save("V8_water_fow_off")

	_map.display_snapshot(_snapshot_water_patch(_v8_visibility()))
	_map.set_fow_enabled(true)
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save("V8_water_fow_on")
	_map.set_fow_enabled(false)

	# --- state 6 (V10): the shipped shoreline profile, on the ragged coast (full frame + close-up) ---
	_map.display_snapshot(_snapshot_coast())
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save(V10_SHORE_NAME)
	# Re-settle: a second get_image() in the same frame as the full-frame save reads back a stale viewport.
	await _settle()
	await _save_crop(
		"%s_closeup" % V10_SHORE_NAME, V10_SHORE_CROP_COL, V10_SHORE_CROP_ROW, V10_SHORE_CROP_RADII
	)

	# The same coast against a DARK land biome — prairie's tan hides how far the sand reaches inland.
	_map.display_snapshot(_snapshot_coast(V10_DARK_LAND_ID))
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save(V10_SHORE_DARK_NAME)
	await _settle()
	await _save_crop(
		"%s_closeup" % V10_SHORE_DARK_NAME,
		V10_SHORE_CROP_COL,
		V10_SHORE_CROP_ROW,
		V10_SHORE_CROP_RADII
	)

	# A/B: the rejected sand-on-both-sides close-up (left in OUT_DIR by the previous shader) vs the shipped one.
	await _save_contact_sheet(
		[V10_REJECTED_NAME, "%s_closeup" % V10_SHORE_NAME],
		["REJECTED: sand on BOTH sides (double width)", "SHIPPED: sand LAND-only, surf washes over it"],
		V10_AB_NAME
	)

	# --- state 7 (V11): the surf-reach / wisp sweep, on the DARK-land coast already displayed ---
	for variant: Dictionary in V11_SHORE_VARIANTS:
		await _render_variant(
			_shore_sweep_overrides(variant),
			variant["name"],
			V10_SHORE_CROP_COL,
			V10_SHORE_CROP_ROW,
			V10_SHORE_CROP_RADII
		)

	# --- state 8 (W): the FoW hex-step, BEFORE vs AFTER the boundary softening (see the const block) ---
	await _render_fow_softness_frames()

	# --- state 9 (X): the dark-water report, on a verbatim window of REAL game terrain, FoW OFF ---
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_real_water())
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save(X_WATER_NAME)
	await _settle()
	await _save_crop(
		"%s_closeup" % X_WATER_NAME, X_WATER_CROP_COL, X_WATER_CROP_ROW, X_WATER_CROP_RADII
	)

	# --- state 10 (L): the per-terrain shore profile on a SMALL inland sea (see the const block) ---
	_map.display_snapshot(_snapshot_lake())
	await _refit(WATER_HEX_RADIUS)
	for variant: Dictionary in LAKE_VARIANTS:
		await _render_lake_variant(variant)
	_restore_lake_shore_profile()

	# --- state 11 (H): rolling_hills cut off at the hex edge (see the const block) ---
	await _render_hills_state()

	# --- state 12 (R): the rugged-gate sweep — every rugged biome, isolated, gate ON (see the const block) ---
	await _render_rugged_sweep()

	# --- state 13 (S): the peak cast-shadow hexagons (see the const block) ---
	await _render_peak_shadow_state()

	get_tree().quit()


func _override_config(overrides: Dictionary) -> Array:
	## Apply lever overrides to the LIVE terrain_config (MapView re-reads it on every redraw) and return a
	## restore token. Restoring must ERASE a key that was ABSENT, never write `null` back over it: MapView
	## reads levers as `bool(config.get(key, DEFAULT))` / `float(...)`, and the default only applies when the
	## key is MISSING — a key present with a null value reaches `bool(null)`, which is a RUNTIME ERROR
	## ("Nonexistent 'bool' constructor") that aborts _update_terrain_shader_quad *before it pushes a single
	## uniform*. Every later frame then renders with STALE uniforms and silently lies. This bit us on
	## `blend_rugged_land`, the first lever with no entry in terrain_config.json.
	var previous: Dictionary = {}
	var had: Dictionary = {}
	for key: String in overrides:
		had[key] = TerrainTextureManager.terrain_config.has(key)
		previous[key] = TerrainTextureManager.terrain_config.get(key)
		TerrainTextureManager.terrain_config[key] = overrides[key]
	return [previous, had]


func _restore_config(token: Array) -> void:
	var previous: Dictionary = token[0]
	var had: Dictionary = token[1]
	for key: String in previous:
		if bool(had[key]):
			TerrainTextureManager.terrain_config[key] = previous[key]
		else:
			TerrainTextureManager.terrain_config.erase(key)


func _pin_canvas(win: Window) -> void:
	## The 1:1 1920×1080 canvas this harness's grid dims (and therefore its target hex radii) assume.
	win.mode = Window.MODE_WINDOWED
	win.size = CANVAS_SIZE
	win.content_scale_size = CANVAS_SIZE          # 1:1 canvas — no content scaling between px and map px
	win.content_scale_factor = 1.0


func _render_rugged_sweep() -> void:
	## The mandatory shred check for the GLOBAL `blend_rugged_land` gate: one ISOLATED hex per rugged biome,
	## against a flat field (R1) and against a rugged field (R2), with the gate forced ON. One full frame with
	## the gate OFF first, as the razor-hexagon reference.
	_map.set_fow_enabled(false)
	_map._show_grid_lines = false   # a drawn hexagon would answer the very question under test
	_map.display_snapshot(_snapshot_rugged_sweep(R_SWEEP_FLAT, 0))
	await _refit(R_HEX_RADIUS)
	# The gate-OFF pair of every frame, so each biome is judged as a controlled A/B: the razor-cut hexagon
	# (today's shipped look) against the blended one. Without it there is no way to tell a blend artifact from
	# something the biome's own art was always doing.
	await _render_sweep_field(R_SWEEP_FLAT, "R_flatoff")

	var token: Array = _override_config({HILLS_FIX_BASE_KEY: true})
	await _render_sweep_field(R_SWEEP_FLAT, "R_flat")
	_restore_config(token)

	_map.display_snapshot(_snapshot_rugged_sweep(R_SWEEP_RUGGED, R_RUGGED_FIELD_ID))
	await _refit(R_HEX_RADIUS)
	await _render_sweep_field(R_SWEEP_RUGGED, "R_ruggedoff")
	token = _override_config({HILLS_FIX_BASE_KEY: true})
	await _render_sweep_field(R_SWEEP_RUGGED, "R_rugged")
	_restore_config(token)


func _render_sweep_field(sweep: Array, prefix: String) -> void:
	## One full frame of the already-displayed sweep field, then a native-res close-up of EVERY isolated hex in
	## it — the crop is the only frame a shredded interior can be judged on.
	_map._fit_map_to_view()
	await _settle()
	await _save("%s_field_full" % prefix)
	for i in sweep.size():
		var slot: Vector2i = _sweep_slot(i)
		await _settle()
		await _save_crop(
			"%s_%02d_%s" % [prefix, int(sweep[i]["id"]), sweep[i]["name"]],
			slot.x,
			slot.y,
			R_CROP_RADII
		)


func _sweep_slot(index: int) -> Vector2i:
	## Row-major over the isolated-hex slots (even col, even row → never adjacent to another subject).
	var col: int = R_SLOT_COLS[index % R_SLOT_COLS.size()]
	var row: int = R_SLOT_ROWS[(index / R_SLOT_COLS.size()) % R_SLOT_ROWS.size()]
	return Vector2i(col, row)


func _snapshot_rugged_sweep(sweep: Array, field_id: int) -> Dictionary:
	## A field with one ISOLATED hex per swept biome. field_id 0 = the split flat field (dark rocky_reg west,
	## tan prairie east); any other id = a uniform field of that biome (the rugged↔rugged case).
	var arr: Array = []
	arr.resize(R_GRID_W * R_GRID_H)
	for y in range(R_GRID_H):
		for x in range(R_GRID_W):
			var field: int = field_id
			if field_id == 0:
				field = R_WEST_FIELD_ID if x < R_FIELD_SPLIT_COL else R_EAST_FIELD_ID
			arr[y * R_GRID_W + x] = field
	for i in sweep.size():
		var slot: Vector2i = _sweep_slot(i)
		arr[slot.y * R_GRID_W + slot.x] = int(sweep[i]["id"])
	return _snapshot(arr, R_GRID_W, R_GRID_H)


func _render_peak_shadow_state() -> void:
	## The peak cast-shadow frames: an alpine massif + an isolated rolling_hills hex in a light flat field.
	## Rendered with whatever the shader currently does — the BEFORE/AFTER pair is captured by running this
	## harness on either side of the shader edit (the shadow is shader code, not a config lever).
	_map.set_fow_enabled(false)
	_map._show_grid_lines = false
	_map.display_snapshot(_snapshot_peak_shadow())
	await _refit(S_HEX_RADIUS)
	await _settle()
	await _save(S_NAME)
	await _settle()
	await _save_crop("%s_closeup" % S_NAME, S_CROP.x, S_CROP.y, S_CROP_RADII)
	await _settle()
	await _save_crop("%s_iso" % S_NAME, S_ISO_HEX.x, S_ISO_HEX.y, S_ISO_CROP_RADII)

	# The cast shadow IN ISOLATION. The relief art overhangs the footline and is semi-transparent out there, so
	# eyeballing (or sampling) the ground near a massif cannot separate "shadow" from "dark mound fringe". Render
	# the identical frame with shadow_strength 0 and diff: the amplified difference IS the shadow's exact
	# footprint — the frame that answers "is it hex-shaped?" and "is it still directional?".
	var token: Array = _override_config(_peak_overrides({"shadow_strength": S_NO_SHADOW_STRENGTH}))
	_map.queue_redraw()   # MapView pushes the shader uniforms from _draw — a config change alone redraws nothing
	await _settle()
	await _save(S_NOCAST_NAME)
	await _settle()
	await _save_crop("%s_closeup" % S_NOCAST_NAME, S_CROP.x, S_CROP.y, S_CROP_RADII)
	await _settle()
	await _save_crop("%s_iso" % S_NOCAST_NAME, S_ISO_HEX.x, S_ISO_HEX.y, S_ISO_CROP_RADII)
	_restore_config(token)
	_save_diff(S_NAME, S_NOCAST_NAME, "%s_footprint" % S_NAME)
	_save_diff(
		"%s_closeup" % S_NAME, "%s_closeup" % S_NOCAST_NAME, "%s_footprint_closeup" % S_NAME
	)
	_save_diff("%s_iso" % S_NAME, "%s_iso" % S_NOCAST_NAME, "%s_footprint_iso" % S_NAME)


func _peak_overrides(changes: Dictionary) -> Dictionary:
	## The shipped `peaks` block with specific levers replaced — every other peak lever stays as configured.
	var peaks: Dictionary = (
		(TerrainTextureManager.terrain_config.get("peaks", {}) as Dictionary).duplicate(true)
	)
	for key: String in changes:
		peaks[key] = changes[key]
	return {"peaks": peaks}


func _snapshot_peak_shadow() -> Dictionary:
	var arr: Array = []
	arr.resize(S_GRID_W * S_GRID_H)
	arr.fill(S_FIELD_ID)
	for hex: Vector2i in S_MASSIF_HEXES:
		arr[hex.y * S_GRID_W + hex.x] = S_MASSIF_ID
	arr[S_ISO_HEX.y * S_GRID_W + S_ISO_HEX.x] = S_ISO_ID
	return _snapshot(arr, S_GRID_W, S_GRID_H)


func _render_hills_state() -> void:
	## The rolling_hills "cut off at the edges" report. One camera + one crop set across every frame:
	## the shipped look, the base-only look (peaks skipped), the two candidate fixes and both together,
	## plus the before−base_only diff (the peak pass's exact footprint) and the two rugged-gate
	## regression frames. Lever overrides go through _override_config/_restore_config (see the null trap there).
	_map.set_fow_enabled(false)
	# Grid lines OFF: the question is whether the TERRAIN cuts along the hex boundary, and a drawn hexagon
	# would answer it for us. (Scoped to this state; the earlier states keep the harness's shipped look.)
	_map._show_grid_lines = false
	_map.display_snapshot(_snapshot_hills())
	await _refit(HILLS_HEX_RADIUS)

	await _render_hills_variant({}, "H_before")
	await _render_hills_variant(_hills_peaks_off_overrides(), "H_base_only")
	await _render_hills_variant(_hills_overhang_overrides(), "H_fix_overhang")
	await _render_hills_variant({HILLS_FIX_BASE_KEY: true}, "H_fix_base")
	var both: Dictionary = _hills_overhang_overrides()
	both[HILLS_FIX_BASE_KEY] = true
	await _render_hills_variant(both, "H_fix_both")

	# The peak pass in isolation: before − base_only. Painted pixels beyond a hills hexagon ARE the overhang.
	_save_diff("H_before", "H_base_only", "H_peaks_only")
	_save_diff("H_before_closeup", "H_base_only_closeup", "H_peaks_only_closeup")
	_save_diff("H_before_iso", "H_base_only_iso", "H_peaks_only_iso")

	# Regression: the rugged gate must leave every non-rugged seam bit-identical (byte-compared outside).
	# Grid lines back ON first — the shipped baselines these are compared against (`blend_bands_full` /
	# `V7_coast_unchanged`) were rendered with the harness's default grid, so the pair must match in that too.
	_map._show_grid_lines = true
	_map.display_snapshot(_snapshot_flat_bands())
	await _refit(GAME_HEX_RADIUS)
	await _render_variant({HILLS_FIX_BASE_KEY: true}, HILLS_GATE_BANDS_NAME, 0, SEAM_ROW, SEAM_CROP_RADII)
	_map.display_snapshot(_snapshot_coast())
	await _refit(WATER_HEX_RADIUS)
	await _render_variant(
		{HILLS_FIX_BASE_KEY: true},
		HILLS_GATE_COAST_NAME,
		V10_SHORE_CROP_COL,
		V10_SHORE_CROP_ROW,
		V10_SHORE_CROP_RADII
	)


func _hills_peaks_off_overrides() -> Dictionary:
	## The shipped `peaks` block with its LOD floor pushed above any on-screen radius → the peak pass is
	## skipped entirely (mounds off), which isolates the BASE grass floor.
	return _peak_overrides({"peak_min_radius": HILLS_PEAKS_OFF_MIN_RADIUS})


func _hills_overhang_overrides() -> Dictionary:
	## The shipped `peaks` block with ONLY the overhang geometry widened — every other peak lever (texture
	## scale, shadow, prominence, light) stays exactly as configured, so the frame isolates the overhang.
	return _peak_overrides({
		"overhang_width": HILLS_FIX_OVERHANG_WIDTH,
		"softness_width": HILLS_FIX_OVERHANG_SOFTNESS,
	})


func _render_hills_variant(overrides: Dictionary, name: String) -> void:
	## One hills frame: the full view, the blob's west seam (hills vs dark rocky_reg), and the ISOLATED
	## hills hex in that same field — the only crop that can expose a shredded interior.
	var token: Array = _override_config(overrides)
	_map._fit_map_to_view()
	await _settle()
	await _save(name)
	# Re-settle between captures: a second get_image() in the same frame reads back a stale viewport.
	await _settle()
	await _save_crop(
		"%s_closeup" % name, HILLS_SEAM_CROP.x, HILLS_SEAM_CROP.y, HILLS_SEAM_CROP_RADII
	)
	await _settle()
	await _save_crop("%s_iso" % name, HILLS_ISO_ROCKY.x, HILLS_ISO_ROCKY.y, HILLS_ISO_CROP_RADII)
	# The shred check on the structured rugged texture (see HILLS_ISO_ALPINE).
	await _settle()
	await _save_crop(
		"%s_alpine" % name, HILLS_ISO_ALPINE.x, HILLS_ISO_ALPINE.y, HILLS_ISO_CROP_RADII
	)
	_restore_config(token)


func _snapshot_hills() -> Dictionary:
	## rolling_hills blob + two isolated hills hexes, in a field that is dark rocky_reg west of
	## HILLS_FIELD_SPLIT_COL and tan prairie east of it (both high-contrast against the hills' green floor).
	var arr: Array = []
	arr.resize(HILLS_GRID_W * HILLS_GRID_H)
	for y in range(HILLS_GRID_H):
		for x in range(HILLS_GRID_W):
			var field: int = (
				HILLS_WEST_FIELD_ID if x < HILLS_FIELD_SPLIT_COL else HILLS_EAST_FIELD_ID
			)
			arr[y * HILLS_GRID_W + x] = field
	for hex: Vector2i in HILLS_BLOB_HEXES:
		arr[hex.y * HILLS_GRID_W + hex.x] = HILLS_ID
	for hex: Vector2i in [HILLS_ISO_ROCKY, HILLS_ISO_PRAIRIE]:
		arr[hex.y * HILLS_GRID_W + hex.x] = HILLS_ID
	arr[HILLS_ISO_ALPINE.y * HILLS_GRID_W + HILLS_ISO_ALPINE.x] = HILLS_ISO_ALPINE_ID
	return _snapshot(arr, HILLS_GRID_W, HILLS_GRID_H)


func _save_diff(a_name: String, b_name: String, out_name: String) -> void:
	## |a − b| × HILLS_DIFF_GAIN, written as a PNG: the pixels ONE pass paints and the other doesn't.
	var a := Image.load_from_file(ProjectSettings.globalize_path("%s/%s.png" % [OUT_DIR, a_name]))
	var b := Image.load_from_file(ProjectSettings.globalize_path("%s/%s.png" % [OUT_DIR, b_name]))
	if a == null or b == null:
		push_warning("blend_probe: diff could not load %s / %s" % [a_name, b_name])
		return
	if a.get_size() != b.get_size():
		push_warning("blend_probe: diff size mismatch %s vs %s" % [a_name, b_name])
		return
	var out := Image.create_empty(a.get_width(), a.get_height(), false, Image.FORMAT_RGB8)
	var changed: int = 0
	for y in range(a.get_height()):
		for x in range(a.get_width()):
			var ca := a.get_pixel(x, y)
			var cb := b.get_pixel(x, y)
			var d := Vector3(absf(ca.r - cb.r), absf(ca.g - cb.g), absf(ca.b - cb.b))
			if d.length() > 0.0:
				changed += 1
			out.set_pixel(x, y, Color(
				minf(d.x * HILLS_DIFF_GAIN, 1.0),
				minf(d.y * HILLS_DIFF_GAIN, 1.0),
				minf(d.z * HILLS_DIFF_GAIN, 1.0)
			))
	var err := out.save_png("%s/%s.png" % [OUT_DIR, out_name])
	if err != OK:
		push_error("blend_probe: failed to save %s (err %d)" % [out_name, err])
	else:
		print("blend_probe: saved %s.png (%d px differ)" % [out_name, changed])


func _render_lake_variant(variant: Dictionary) -> void:
	## Override the inland_sea terrain's `shore_profile` in the live config, rebuild the shader's
	## layer_shore_map (the manager updates the ImageTexture in place, so MapView's binding survives), and
	## dump one full frame + one native-res close-up of the lake. Same camera/crop in every variant.
	_set_lake_shore_profile({
		"reach_scale": float(variant["reach_scale"]),
		"wisp_scale": float(variant["wisp_scale"]),
	})
	var name: String = String(variant["name"])
	_map._fit_map_to_view()   # window sizing can settle late; re-fit so every frame is at the target radius
	await _settle()
	await _save("%s_full" % name)
	# Re-settle: a second get_image() in the same frame as the full-frame save reads back a stale viewport.
	await _settle()
	await _save_crop(name, LAKE_CROP_COL, LAKE_CROP_ROW, LAKE_CROP_RADII)


func _set_lake_shore_profile(profile: Dictionary) -> void:
	var entry: Dictionary = _lake_terrain_entry()
	if entry.is_empty():
		push_warning("blend_probe: inland_sea (id %d) missing from terrain_config" % LAKE_WATER_ID)
		return
	entry["shore_profile"] = profile
	TerrainTextureManager.rebuild_layer_shore_map()


func _restore_lake_shore_profile() -> void:
	## Put the SHIPPED inland_sea profile back, so any frame rendered after this state is judged on config.
	_set_lake_shore_profile(_shipped_lake_profile)


func _lake_terrain_entry() -> Dictionary:
	for entry: Variant in TerrainTextureManager.terrain_config.get("terrains", []):
		if entry is Dictionary and int(entry.get("id", -1)) == LAKE_WATER_ID:
			return entry
	return {}


func _snapshot_lake() -> Dictionary:
	## A small inland_sea (LAKE_HEXES) in a field of dark rocky land — a lake, not an open-water expanse.
	var arr: Array = []
	arr.resize(WATER_GRID_W * WATER_GRID_H)
	arr.fill(LAKE_LAND_ID)
	for hex: Vector2i in LAKE_HEXES:
		arr[hex.y * WATER_GRID_W + hex.x] = LAKE_WATER_ID
	return _snapshot(arr, WATER_GRID_W, WATER_GRID_H)


func _snapshot_real_water() -> Dictionary:
	## A verbatim 14×10 window of a LIVE snapshot's id-map (see X_WATER_IDS): salt-and-pepper
	## continental_shelf / deep_ocean, which the synthetic blob state never reproduced.
	var arr: Array = []
	arr.resize(X_WATER_GRID_W * X_WATER_GRID_H)
	for y in range(X_WATER_GRID_H):
		var row: Array = X_WATER_IDS[y]
		for x in range(X_WATER_GRID_W):
			arr[y * X_WATER_GRID_W + x] = int(row[x])
	return _snapshot(arr, X_WATER_GRID_W, X_WATER_GRID_H)


func _render_fow_softness_frames() -> void:
	## One camera, one terrain, one visibility map — only `fow_softness` changes. Isolates the FoW tint as
	## the source of the hexagonal brightness steps in open water (the blend is exonerated by W_fow_off).
	var shipped_softness: float = _map._fow_softness
	var shipped_noise: float = _map._fow_noise_amount

	# (a) FoW OFF — the terrain-only reference: same water, no mist, so any hard edge here IS the blend's.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_water_patch())
	await _refit(WATER_HEX_RADIUS)
	await _settle()
	await _save(W_FOW_OFF_NAME)
	await _settle()
	await _save_crop("%s_closeup" % W_FOW_OFF_NAME, W_CROP_COL, W_CROP_ROW, W_CROP_RADII)
	await _settle()
	await _save_crop(
		"%s_same_terrain" % W_FOW_OFF_NAME, W_SAME_CROP_COL, W_SAME_CROP_ROW, W_SAME_CROP_RADII
	)

	# (b) + (c) FoW ON over the same terrain: unsmoothed (main's per-hex step) vs the shipped softening.
	_map.display_snapshot(_snapshot_water_patch(_v8_visibility()))
	_map.set_fow_enabled(true)
	await _refit(WATER_HEX_RADIUS)
	for frame: Dictionary in [
		{
			"name": W_FOW_ON_NAME,
			"softness": W_FOW_SOFTNESS_UNSMOOTHED,
			"noise": W_FOW_NOISE_UNSMOOTHED,
		},
		{"name": W_FOW_FIXED_NAME, "softness": shipped_softness, "noise": shipped_noise},
	]:
		_map._fow_softness = float(frame["softness"])
		_map._fow_noise_amount = float(frame["noise"])
		_map.queue_redraw()
		await _settle()
		await _save(String(frame["name"]))
		await _settle()
		await _save_crop(
			"%s_closeup" % String(frame["name"]), W_CROP_COL, W_CROP_ROW, W_CROP_RADII
		)
		await _settle()
		await _save_crop(
			"%s_same_terrain" % String(frame["name"]),
			W_SAME_CROP_COL,
			W_SAME_CROP_ROW,
			W_SAME_CROP_RADII
		)

	_map._fow_softness = shipped_softness
	_map._fow_noise_amount = shipped_noise
	_map.set_fow_enabled(false)


func _shore_sweep_overrides(variant: Dictionary) -> Dictionary:
	## The shipped `shore` block with ONLY the sweep's surf/wisp keys replaced — so `sand_width`,
	## `foam_inland_width` and the colors stay exactly as configured in every frame of the sweep.
	var shore: Dictionary = (
		(TerrainTextureManager.terrain_config.get("shore", {}) as Dictionary).duplicate(true)
	)
	for key: String in ["foam_width", "wisp_center_width", "wisp_half_width"]:
		shore[key] = variant[key]
	return {"shore": shore}


func _refit(target_radius: float) -> void:
	## Fit, settle, and assert the achieved hex radius — the blend look is radius-relative, so a frame is
	## only an honest proxy for the game when it was rendered at the game's on-screen radius.
	# Re-pin the canvas first: the WM can still push the window back to the project's MAXIMIZED mode after
	# _ready has run (see _pin_canvas), and a maximized viewport throws every radius off target.
	_pin_canvas(get_window())
	await _settle()
	_map._fit_map_to_view()
	await _settle()
	# Settle twice: the window's backing scale (HiDPI) can land a frame late, and the first capture after a
	# resize otherwise reads back at the pre-scale resolution — which silently makes frames incomparable.
	_map._fit_map_to_view()
	await _settle()
	var radius: float = _map.last_hex_radius
	print("blend_probe: hex radius = %.1f px (target ≈ %.0f)" % [radius, target_radius])
	if absf(radius - target_radius) > HEX_RADIUS_TOLERANCE:
		push_warning("blend_probe: radius %.1f is off the target ~%.0f — retune the grid dims"
			% [radius, target_radius])


func _render_variant(
	overrides: Dictionary,
	name: String,
	crop_col: int = ISO_CROP_COL,
	crop_row: int = ISO_CROP_ROW,
	crop_radii: float = ISO_CROP_RADII
) -> void:
	## Re-render with config levers overridden in the live config (MapView re-reads the config every frame
	## in _update_terrain_shader_quad, so a redraw is all it takes), then restore the shipped values.
	var token: Array = _override_config(overrides)
	_map._fit_map_to_view()   # window sizing can settle late; re-fit so every frame is at the target radius
	await _settle()
	await _save(name)
	# …plus a native-res close-up of one isolated hex: the full frame is downscaled when viewed, which can
	# hide a ragged/torn edge. The close-up is the frame the "is the hex intact?" call is made on.
	# Re-settle first: a second get_image() in the same frame as the full-frame save can read back a stale
	# (black) viewport texture.
	await _settle()
	await _save_crop("%s_closeup" % name, crop_col, crop_row, crop_radii)
	_restore_config(token)


func _snapshot_flat_bands() -> Dictionary:
	var band_cols: int = GRID_W / FLAT_BAND_IDS.size()
	var arr: Array = []
	arr.resize(GRID_W * GRID_H)
	for y in range(GRID_H):
		for x in range(GRID_W):
			var band: int = mini(x / band_cols, FLAT_BAND_IDS.size() - 1)
			arr[y * GRID_W + x] = FLAT_BAND_IDS[band]
	return _snapshot(arr, GRID_W, GRID_H)


func _snapshot_isolated_islands() -> Dictionary:
	## A field of dark rocky soil with prairie hexes dropped in, each ISOLATED (all six neighbours are soil).
	## The straight-band seam CANNOT show hex shredding; this can.
	var arr: Array = []
	arr.resize(ISO_GRID_W * ISO_GRID_H)
	for y in range(ISO_GRID_H):
		for x in range(ISO_GRID_W):
			var is_island: bool = (
				y % ISO_ISLAND_ROW_STRIDE == 0 and x % ISO_ISLAND_COL_STRIDE == 0
			)
			arr[y * ISO_GRID_W + x] = ISO_ISLAND_ID if is_island else ISO_FIELD_ID
	return _snapshot(arr, ISO_GRID_W, ISO_GRID_H)


func _snapshot_water_patch(visibility := PackedFloat32Array()) -> Dictionary:
	## Continental shelf with a RAGGED deep-ocean region embedded in it (plus two isolated deep hexes).
	## Both ids are blend_class `water`, so this is the water↔water state — pre-change it renders razor-sharp
	## hexagon silhouettes; post-change the depths must grade into each other with no hex outline left.
	## An optional visibility raster feeds the FoW vis-map (state 5).
	var arr: Array = []
	arr.resize(WATER_GRID_W * WATER_GRID_H)
	arr.fill(WATER_SHELF_ID)
	for hex: Vector2i in WATER_DEEP_HEXES:
		arr[hex.y * WATER_GRID_W + hex.x] = WATER_DEEP_ID
	return _snapshot(arr, WATER_GRID_W, WATER_GRID_H, visibility)


func _v8_visibility() -> PackedFloat32Array:
	## Active blob around V8_ACTIVE_CENTER, everything else DISCOVERED (misty). Nothing unexplored, so the
	## FoW-on frame shows exactly the same terrain as the FoW-off one and isolates the mist tint.
	var vis := PackedFloat32Array()
	vis.resize(WATER_GRID_W * WATER_GRID_H)
	for y in range(WATER_GRID_H):
		for x in range(WATER_GRID_W):
			var d: int = _map._hex_distance(x, y, V8_ACTIVE_CENTER.x, V8_ACTIVE_CENTER.y)
			vis[y * WATER_GRID_W + x] = (
				V8_VIS_ACTIVE if d <= V8_ACTIVE_RADIUS else V8_VIS_DISCOVERED
			)
	return vis


func _snapshot_coast(shore_id: int = COAST_SHORE_ID) -> Dictionary:
	## A ragged land↔water coastline with a single water id (so no water↔water edge exists anywhere) and an
	## inland flat↔flat seam. The shoreline (foam/beach) and flat-interlock passes own every pixel here, so
	## this frame must be BIT-IDENTICAL before and after any eligibility-gate change.
	## `shore_id` swaps the coastal land band (default tan prairie; pass a DARK biome to judge the sand's
	## inland reach, which tan land hides).
	var arr: Array = []
	arr.resize(WATER_GRID_W * WATER_GRID_H)
	for y in range(WATER_GRID_H):
		var shore_col: int = COAST_SHORE_BASE_COL + int(COAST_SHORE_WOBBLE[y % COAST_SHORE_WOBBLE.size()])
		for x in range(WATER_GRID_W):
			var id: int = COAST_WATER_ID
			if x >= shore_col + COAST_SHORE_BAND_COLS:
				id = COAST_INLAND_ID
			elif x >= shore_col:
				id = shore_id
			arr[y * WATER_GRID_W + x] = id
	return _snapshot(arr, WATER_GRID_W, WATER_GRID_H)


func _snapshot(
	terrain: Array, w: int, h: int, visibility := PackedFloat32Array()
) -> Dictionary:
	var overlays: Dictionary = {"terrain": terrain}
	if not visibility.is_empty():
		# Same shape MapView._ingest_overlay_channels expects; _visibility_state_at reads the RAW channel.
		overlays["channels"] = {
			"visibility": {"raw": visibility, "normalized": visibility, "label": "Visibility"},
		}
	return {
		"grid": {"width": w, "height": h, "wrap_horizontal": false},
		"overlays": overlays,
		"populations": [],
		"herds": [],
	}


func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame


func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("blend_probe: null image (dummy renderer?) — run without --headless")
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("blend_probe: failed to save %s (err %d)" % [name, err])
	else:
		print("blend_probe: saved ", name, ".png")


func _save_seam_crop(name: String) -> void:
	var band_cols: int = GRID_W / FLAT_BAND_IDS.size()
	await _save_crop(name, SEAM_BAND_INDEX * band_cols, SEAM_ROW, SEAM_CROP_RADII)


func _save_crop(name: String, col: int, row: int, radii: float) -> void:
	## Native-resolution crop centred on a hex (no rescale — the pixels are the game's).
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("blend_probe: null image (dummy renderer?) — run without --headless")
		return
	var radius: float = _map.last_hex_radius
	var center: Vector2 = _map._hex_center(col, row, radius, _map.last_origin)
	print("blend_probe: %s at hex radius %.1f px" % [name, radius])   # the radius the frame was judged at
	var w := image.get_width()
	var h := image.get_height()
	# The captured image can be a HiDPI multiple of the canvas — rescale MAP-space px into IMAGE px. The
	# map is laid out in the VIEWPORT's coordinate space (that is what _fit_map_to_view measures), which on
	# a HiDPI window is ALREADY the backing-store size, not CANVAS_SIZE. Dividing by CANVAS_SIZE.x instead
	# double-counted the 2× scale and threw every close-up a screenful off-target (the coast crops silently
	# landed out in the inland desert), so scale by image ÷ viewport — 1.0 in both the 1:1 and HiDPI cases.
	var px_scale: float = float(w) / get_viewport().get_visible_rect().size.x
	center *= px_scale
	var half: float = radii * radius * px_scale
	var x0 := clampi(int(center.x - half), 0, w - 1)
	var y0 := clampi(int(center.y - half), 0, h - 1)
	var x1 := clampi(int(center.x + half), 0, w)
	var y1 := clampi(int(center.y + half), 0, h)
	var crop := image.get_region(Rect2i(x0, y0, maxi(x1 - x0, 1), maxi(y1 - y0, 1)))
	var err := crop.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("blend_probe: failed to save %s (err %d)" % [name, err])
	else:
		print("blend_probe: saved ", name, ".png")


func _save_contact_sheet(names: Array[String], labels: Array[String], out_name: String) -> void:
	## Compose the already-saved sweep frames into one labelled sheet, by building a throwaway CanvasLayer
	## of TextureRects + captions over the hidden map and capturing the viewport (Image has no text drawing).
	_map.visible = false
	var layer := CanvasLayer.new()
	layer.layer = SHEET_LAYER
	add_child(layer)
	var bg := ColorRect.new()
	bg.color = SHEET_BG
	bg.size = Vector2(CANVAS_SIZE)
	layer.add_child(bg)

	var rows: int = int(ceil(float(names.size()) / float(SHEET_COLS)))
	var cell := Vector2(float(CANVAS_SIZE.x) / float(SHEET_COLS), float(CANVAS_SIZE.y) / float(rows))
	for i in names.size():
		# globalize: Image.load_from_file wants an OS path (the PNGs were just written, so they are not
		# imported resources and cannot be `load()`ed).
		var img := Image.load_from_file(
			ProjectSettings.globalize_path("%s/%s.png" % [OUT_DIR, names[i]])
		)
		if img == null:
			push_warning("blend_probe: contact sheet could not load %s" % names[i])
			continue
		var origin := Vector2(float(i % SHEET_COLS) * cell.x, float(i / SHEET_COLS) * cell.y)
		var rect := TextureRect.new()
		rect.texture = ImageTexture.create_from_image(img)
		rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		rect.position = origin + Vector2(SHEET_PADDING, SHEET_PADDING + SHEET_CAPTION_HEIGHT)
		rect.size = cell - Vector2(2.0 * SHEET_PADDING, 2.0 * SHEET_PADDING + SHEET_CAPTION_HEIGHT)
		layer.add_child(rect)
		var caption := Label.new()
		caption.text = labels[i]
		caption.add_theme_font_size_override("font_size", SHEET_CAPTION_FONT_SIZE)
		caption.position = origin + Vector2(SHEET_PADDING, SHEET_PADDING)
		caption.size = Vector2(cell.x - 2.0 * SHEET_PADDING, SHEET_CAPTION_HEIGHT)
		layer.add_child(caption)

	await _settle()
	await _save(out_name)
	layer.queue_free()
	_map.visible = true
