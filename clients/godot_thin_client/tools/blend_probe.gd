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

# --- state 6 (V8): SHORELINE sweep — the sand/foam moved onto the WATER side ---
# Rendered on the ragged coast (state 4's terrain). The old two-sided pass painted the land solid tan and
# the water solid white AT the shared edge, so they met in a hard tan↔white line that traced the hexagon,
# and the land-side beach covered ~0.4·radius of the land texture. Now: land (untouched) → sand shallows
# → surf → open water, all fades smoothstepped. The sweep varies the three width levers.
const V8_SHORE_VARIANTS := [
	{
		"name": "V8_shore_S1",
		"label": "S1  all on the water side — sand .35 · foam .55 · land rim 0 (SHIPPED)",
		"overrides": {"shore": {"sand_width": 0.35, "foam_width": 0.55, "land_beach_width": 0.0}},
	},
	{
		"name": "V8_shore_S2",
		"label": "S2  + thin damp rim on the land — sand .35 · foam .55 · land rim .12",
		"overrides": {"shore": {"sand_width": 0.35, "foam_width": 0.55, "land_beach_width": 0.12}},
	},
	{
		"name": "V8_shore_S3",
		"label": "S3  more sand, less foam — sand .50 · foam .45 · land rim 0",
		"overrides": {"shore": {"sand_width": 0.50, "foam_width": 0.45, "land_beach_width": 0.0}},
	},
]
const V8_SHORE_SHEET_NAME := "V8_shore_sheet"
# Close-up on the coastline itself (the frame the "is the hex-tracing line gone?" call is made on).
const V8_SHORE_CROP_COL := 6
const V8_SHORE_CROP_ROW := 4
const V8_SHORE_CROP_RADII := 2.2

# Contact-sheet layout (a 2×2 grid of the sweep frames, each captioned).
const SHEET_COLS := 2
const SHEET_BG := Color(0.06, 0.06, 0.08)
const SHEET_CAPTION_HEIGHT := 34.0
const SHEET_CAPTION_FONT_SIZE := 20
const SHEET_PADDING := 8.0
const SHEET_NAME := "V6_sheet"
const SHEET_LAYER := 200   # above MapView's minimap CanvasLayer (102), which is not hidden with the map

var _map: Node2D


func _ready() -> void:
	var win := get_window()
	win.size = CANVAS_SIZE
	win.content_scale_size = CANVAS_SIZE          # 1:1 canvas — no content scaling between px and map px
	win.content_scale_factor = 1.0
	DirAccess.make_dir_absolute(OUT_DIR)
	_map = MAP_VIEW.new()
	add_child(_map)
	await get_tree().process_frame
	await get_tree().process_frame

	_map.set_fow_enabled(false)
	_map.enable_terrain_textures(true)
	TerrainTextureManager.use_edge_blending = true
	_map._map_cache_enabled = false               # the shader path bypasses the cache anyway

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

	# --- state 6 (V8): the shoreline sweep, on the ragged coast ---
	_map.display_snapshot(_snapshot_coast())
	await _refit(WATER_HEX_RADIUS)
	var shore_names: Array[String] = []
	var shore_labels: Array[String] = []
	for variant: Dictionary in V8_SHORE_VARIANTS:
		await _render_variant(
			variant["overrides"], variant["name"],
			V8_SHORE_CROP_COL, V8_SHORE_CROP_ROW, V8_SHORE_CROP_RADII
		)
		shore_names.append(variant["name"])
		shore_labels.append(variant["label"])
	await _save_contact_sheet(shore_names, shore_labels, V8_SHORE_SHEET_NAME)

	get_tree().quit()


func _refit(target_radius: float) -> void:
	## Fit, settle, and assert the achieved hex radius — the blend look is radius-relative, so a frame is
	## only an honest proxy for the game when it was rendered at the game's on-screen radius.
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
	var previous: Dictionary = {}
	for key: String in overrides:
		previous[key] = TerrainTextureManager.terrain_config.get(key)
		TerrainTextureManager.terrain_config[key] = overrides[key]
	_map._fit_map_to_view()   # window sizing can settle late; re-fit so every frame is at the target radius
	await _settle()
	await _save(name)
	# …plus a native-res close-up of one isolated hex: the full frame is downscaled when viewed, which can
	# hide a ragged/torn edge. The close-up is the frame the "is the hex intact?" call is made on.
	# Re-settle first: a second get_image() in the same frame as the full-frame save can read back a stale
	# (black) viewport texture.
	await _settle()
	await _save_crop("%s_closeup" % name, crop_col, crop_row, crop_radii)
	for key: String in previous:
		TerrainTextureManager.terrain_config[key] = previous[key]


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


func _snapshot_coast() -> Dictionary:
	## A ragged land↔water coastline with a single water id (so no water↔water edge exists anywhere) and an
	## inland flat↔flat seam. The shoreline (foam/beach) and flat-interlock passes own every pixel here, so
	## this frame must be BIT-IDENTICAL before and after any eligibility-gate change.
	var arr: Array = []
	arr.resize(WATER_GRID_W * WATER_GRID_H)
	for y in range(WATER_GRID_H):
		var shore_col: int = COAST_SHORE_BASE_COL + int(COAST_SHORE_WOBBLE[y % COAST_SHORE_WOBBLE.size()])
		for x in range(WATER_GRID_W):
			var id: int = COAST_WATER_ID
			if x >= shore_col + COAST_SHORE_BAND_COLS:
				id = COAST_INLAND_ID
			elif x >= shore_col:
				id = COAST_SHORE_ID
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
	# The captured image can be a HiDPI multiple of the 1:1 canvas — rescale map-space px into image px.
	var px_scale: float = float(w) / float(CANVAS_SIZE.x)
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
