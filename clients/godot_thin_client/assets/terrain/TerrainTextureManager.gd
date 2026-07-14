extends Node
## Singleton that centralizes terrain texture loading and configuration.
## Loads terrain textures once for the 2D MapView renderer to avoid duplicate loading.
## Registered as autoload "TerrainTextureManager" in project.godot.

const TERRAIN_CONFIG_PATH := "res://assets/terrain/terrain_config.json"
const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

var terrain_textures: Texture2DArray = null
# Canopy overlay: whole tree crowns on transparency (RGBA), a SECOND Texture2DArray sampled by the
# blend shader over the (grass) forest floor so crowns can overhang the hex boundary. Only the biomes
# with a `textures/canopy/NN_name.png` asset get a layer; `canopy_layer_by_id` maps terrain id → that
# array layer (absent = no canopy). Layers are auto-discovered from whatever files are present in
# `textures/canopy/` — any biome with a `NN_name.png` there gets a canopy layer, so there's no fixed
# count to keep in sync here.
var canopy_textures: Texture2DArray = null
var canopy_layer_by_id: Dictionary = {}
# Peak overlay: faceted mountain relief on transparency (RGBA), a THIRD Texture2DArray sampled by the
# blend shader over the (rocky) highland/volcanic base floor — the mountain-drama analog of the canopy
# overlay. Only biomes with a `textures/peaks/NN_name.png` asset get a layer; `peak_layer_by_id` maps
# terrain id → that array layer (absent = no peaks). Layers are auto-discovered from whatever files are
# present in `textures/peaks/` — any biome with a `NN_name.png` there gets a peak layer, so there's no
# fixed count to keep in sync here.
var peak_textures: Texture2DArray = null
var peak_layer_by_id: Dictionary = {}
# River water: flowing-water bands (RGBA, fully opaque), a FOURTH Texture2DArray sampled by the blend
# shader's river passes. Unlike the canopy/peak arrays this one is NOT keyed by terrain id — a river is not
# a biome — so the layer index is the file's numeric prefix: `00_minor.png` → 0 and `01_major.png` → 1 are
# the hex-EDGE classes (layer = river CLASS - 1), and `02_navigable.png` → 2 is the CHANNEL water the
# navigable pass paints over a NavigableRiver hex's bank floor (that terrain's own base texture is the
# BANK ground, not water — see terrain_blend.gdshader's navigable pass).
var river_textures: Texture2DArray = null
var terrain_config: Dictionary = {}
var use_terrain_textures: bool = false
var use_edge_blending: bool = false

# Per-base-layer MEAN LUMINANCE (0..1), one entry per terrain id, computed once at build time from the
# CPU-side layer images. Feeds the shader's HEIGHT BLENDING at flat↔flat seams: with no height maps we use
# each texture's own per-pixel luminance as a pseudo-height, and the mean is what ZERO-CENTRES it — without
# it a bright prairie would always out-"height" a dark soil and the seam would be biased entirely to one
# biome instead of interlocking. Exposed to the shader as `layer_luma_texture` (1×N single-channel), so the
# layer count never has to be baked into the shader as a fixed array size.
var layer_mean_luma: PackedFloat32Array = PackedFloat32Array()
var layer_luma_texture: ImageTexture = null

# PER-WATER-TERRAIN SHORE PROFILE (R = sand_scale, G = foam_scale, B = wisp_scale), one texel per terrain id
# — the same 1×N by-layer-index lookup table as layer_luma_texture, but RGBA float. The shipped shore profile
# (sand → surf → offshore wisp) is ONE global, ocean-tuned coast, and a coast is not one thing:
#   · deep_ocean never touches ordinary land (the natural sequence is deep → shelf → land), so where it DOES
#     meet land it is a CLIFF — no beach at all, and the full dramatic surf.
#   · continental_shelf is the ordinary beach — sand, and a more muted wave.
#   · inland_sea is a handful of hexes, and the ocean profile swamps it (the offshore wisp reads as noise
#     across the middle of a lake).
# So each WATER terrain scales its OWN coastline's profile along three independent axes:
#   sand_scale — multiplies the beach's INLAND reach (sand_band). 0.0 = NO BEACH AT ALL (the cliff).
#   foam_scale — multiplies the MAIN WAVE's reaches, both ways: the wash up the beach (foam_inland_band) and
#                the surf's seaward reach (foam_band). REACH only — the surf's PEAK is never scaled, because
#                that peak is what conceals the base's own step at the waterline.
#   wisp_scale — multiplies the secondary offshore disturbance: its centre distance, its half-width AND its
#                strength (0 = no second disturbance).
# A water terrain with no `shore_profile` block gets the NEUTRAL default (1, 1, 1), i.e. exactly the global
# profile — bit-identical to before this table existed. Read by the shader as `layer_shore_map` and blended
# across the water NEIGHBOURS by shared-edge proximity, so a cliff coast transitions into a beach coast
# instead of switching at a bisector (see terrain_blend.gdshader's shore block).
var layer_shore_texture: ImageTexture = null

const SHORE_PROFILE_DEFAULT_SAND_SCALE := 1.0
const SHORE_PROFILE_DEFAULT_FOAM_SCALE := 1.0
const SHORE_PROFILE_DEFAULT_WISP_SCALE := 1.0
# Guard rails on the config values: a negative scale is meaningless, and nothing needs to more than double
# the shipped (ocean-tuned) profile.
const SHORE_PROFILE_MAX_SCALE := 2.0

# Mean luminance is measured on a downscaled copy of each layer (Lanczos ≈ area-average) instead of walking
# every texel of a 512² image ×37 layers — same mean to well within the blend's sensitivity, ~1000× fewer
# get_pixel calls.
const MEAN_LUMA_SAMPLE_SIZE := 16
# Rec.709 luma weights — MUST match the luma() helper in terrain_blend.gdshader, or the shader's
# zero-centring would subtract a mean measured on a different quantity than it compares against.
const LUMA_WEIGHTS := Vector3(0.2126, 0.7152, 0.0722)

# CPU-side copy of every terrain layer, captured ONCE at build time. Reused by the hex-texture
# cache and the get_terrain_image readback so we never call Texture2DArray.get_layer_data() again — a
# second readback returns a blank image on some drivers, which blanked the base terrain on any cache rebuild.
var _layer_images: Array[Image] = []
# terrain_id -> blend_class ("flat" | "water" | "rugged"), parsed once from terrain_config.
var _blend_class_by_id: Dictionary = {}


func _ready() -> void:
	_load_config()
	_build_blend_class_map()
	rebuild_layer_shore_map()
	_load_textures()


func _build_blend_class_map() -> void:
	## Parse the per-terrain blend_class field (single source of truth for edge-blend eligibility).
	_blend_class_by_id.clear()
	var terrains: Array = terrain_config.get("terrains", [])
	for entry: Variant in terrains:
		if entry is Dictionary:
			var tid: int = int(entry.get("id", -1))
			if tid >= 0:
				_blend_class_by_id[tid] = String(entry.get("blend_class", "rugged"))


func blend_class_for(terrain_id: int) -> String:
	## Blend class of a terrain; unknown ids default to "rugged" (never blends), the safe fallback.
	return String(_blend_class_by_id.get(terrain_id, "rugged"))


func _load_config() -> void:
	if not FileAccess.file_exists(TERRAIN_CONFIG_PATH):
		return
	var file := FileAccess.open(TERRAIN_CONFIG_PATH, FileAccess.READ)
	if file == null:
		return
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	file.close()
	if typeof(parsed) == TYPE_DICTIONARY:
		terrain_config = parsed
		use_terrain_textures = bool(terrain_config.get("use_terrain_textures", false))
		use_edge_blending = bool(terrain_config.get("use_edge_blending", false))


func _load_textures() -> void:
	# Skip texture loading if disabled in config
	if not use_terrain_textures:
		print("[TerrainTextureManager] Terrain textures disabled in config (using solid colors)")
		return

	# Build texture array from individual PNGs at runtime
	terrain_textures = _build_terrain_texture_array()
	if terrain_textures != null and terrain_textures.get_layers() > 0:
		print("[TerrainTextureManager] Loaded terrain textures: %d layers" % terrain_textures.get_layers())
	else:
		print("[TerrainTextureManager] Terrain textures not found (using solid colors)")

	# Build the companion canopy array (transparent tree crowns) — only for biomes with a canopy asset.
	if terrain_textures != null:
		canopy_textures = _build_canopy_texture_array()
		# Build the companion peak array (transparent mountain relief) — only for biomes with a peak asset.
		peak_textures = _build_peak_texture_array()
		# Build the companion river array (flowing water bands) for the shader's hex-edge river pass.
		river_textures = _build_river_texture_array()


func _build_terrain_texture_array() -> Texture2DArray:
	const BASE_PATH := "res://assets/terrain/textures/base/"
	var terrain_count: int = TerrainDefinitions.get_terrain_count()
	var terrain_names: Dictionary = TerrainDefinitions.get_names_dict()

	if terrain_count == 0:
		push_warning("[TerrainTextureManager] No terrain definitions loaded - check terrain_config.json")
		return null

	var images: Array[Image] = []
	var first_size: Vector2i = Vector2i.ZERO
	var missing_textures: Array[String] = []

	for terrain_id: int in range(terrain_count):
		var tname: String = terrain_names.get(terrain_id, "unknown")
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var filepath := BASE_PATH + filename

		var img: Image = null
		# Load directly from file (more reliable than ResourceLoader which requires import cache)
		var abs_path := ProjectSettings.globalize_path(filepath)
		if FileAccess.file_exists(abs_path):
			img = Image.load_from_file(abs_path)

		if img == null:
			missing_textures.append(filename)
			img = Image.create(512, 512, false, Image.FORMAT_RGBA8)
			img.fill(Color.MAGENTA)

		if first_size == Vector2i.ZERO:
			first_size = Vector2i(img.get_width(), img.get_height())
		elif Vector2i(img.get_width(), img.get_height()) != first_size:
			img.resize(first_size.x, first_size.y)

		if img.get_format() != Image.FORMAT_RGBA8:
			img.convert(Image.FORMAT_RGBA8)

		images.append(img)

	if missing_textures.size() > 0:
		push_warning("[TerrainTextureManager] Missing %d terrain textures (showing magenta): %s" % [
			missing_textures.size(),
			", ".join(missing_textures.slice(0, 5)) + ("..." if missing_textures.size() > 5 else "")
		])

	if images.size() != terrain_count:
		push_error("[TerrainTextureManager] Expected %d textures, got %d" % [terrain_count, images.size()])
		return null

	var array_tex := Texture2DArray.new()
	var err := array_tex.create_from_images(images)
	if err != OK:
		push_error("[TerrainTextureManager] Failed to create Texture2DArray: %d" % err)
		return null

	# Retain the CPU-side layer Images so hex/edge caches never re-read back from the GPU array.
	_layer_images = images
	_build_layer_luma()

	return array_tex


func _build_layer_luma() -> void:
	## Measure each base layer's mean luminance (pseudo-height zero-point for the shader's height blending)
	## and pack it into a 1×N single-channel float texture the shader fetches by layer index.
	layer_mean_luma = PackedFloat32Array()
	layer_luma_texture = null
	if _layer_images.is_empty():
		return
	var luma_img := Image.create(_layer_images.size(), 1, false, Image.FORMAT_RF)
	for layer: int in range(_layer_images.size()):
		var mean: float = _mean_luma(_layer_images[layer])
		layer_mean_luma.append(mean)
		luma_img.set_pixel(layer, 0, Color(mean, 0.0, 0.0, 1.0))
	layer_luma_texture = ImageTexture.create_from_image(luma_img)


func rebuild_layer_shore_map() -> void:
	## Pack each terrain's optional `shore_profile` block into a 1×N RGBA float texture the shader fetches by
	## layer index (same construction/binding pattern as _build_layer_luma). Terrains with no block get the
	## neutral (1, 1, 1) default, which is a no-op on the shore profile.
	## PUBLIC because it re-reads `terrain_config` from scratch: the blend probe sweeps per-terrain shore
	## profiles by mutating the live config and calling this. The ImageTexture is UPDATED in place (never
	## replaced) so MapView's one-time `layer_shore_map` binding stays valid across a rebuild.
	var terrain_count: int = TerrainDefinitions.get_terrain_count()
	if terrain_count <= 0:
		return
	var neutral := Color(
		SHORE_PROFILE_DEFAULT_SAND_SCALE,
		SHORE_PROFILE_DEFAULT_FOAM_SCALE,
		SHORE_PROFILE_DEFAULT_WISP_SCALE,
		1.0
	)
	var shore_img := Image.create(terrain_count, 1, false, Image.FORMAT_RGBAF)
	for terrain_id: int in range(terrain_count):
		shore_img.set_pixel(terrain_id, 0, neutral)
	for entry: Variant in terrain_config.get("terrains", []):
		if not (entry is Dictionary):
			continue
		var tid: int = int(entry.get("id", -1))
		if tid < 0 or tid >= terrain_count:
			continue
		var profile: Variant = entry.get("shore_profile", null)
		if not (profile is Dictionary):
			continue
		shore_img.set_pixel(tid, 0, Color(
			_shore_scale(profile, "sand_scale", SHORE_PROFILE_DEFAULT_SAND_SCALE),
			_shore_scale(profile, "foam_scale", SHORE_PROFILE_DEFAULT_FOAM_SCALE),
			_shore_scale(profile, "wisp_scale", SHORE_PROFILE_DEFAULT_WISP_SCALE),
			1.0
		))
	if layer_shore_texture == null:
		layer_shore_texture = ImageTexture.create_from_image(shore_img)
	else:
		layer_shore_texture.update(shore_img)


func _shore_scale(profile: Dictionary, key: String, fallback: float) -> float:
	## One `shore_profile` scale, defaulted and guard-railed. A missing key is NEUTRAL (the water keeps the
	## global profile on that axis), so a partial block is legal.
	return clampf(float(profile.get(key, fallback)), 0.0, SHORE_PROFILE_MAX_SCALE)


func _mean_luma(img: Image) -> float:
	## Mean Rec.709 luminance of a layer, sampled from a MEAN_LUMA_SAMPLE_SIZE² Lanczos downscale
	## (an area-average of the full image) rather than every texel.
	if img == null:
		return 0.0
	var small: Image = img.duplicate()
	small.resize(MEAN_LUMA_SAMPLE_SIZE, MEAN_LUMA_SAMPLE_SIZE, Image.INTERPOLATE_LANCZOS)
	var total: float = 0.0
	for y: int in range(MEAN_LUMA_SAMPLE_SIZE):
		for x: int in range(MEAN_LUMA_SAMPLE_SIZE):
			var c: Color = small.get_pixel(x, y)
			total += c.r * LUMA_WEIGHTS.x + c.g * LUMA_WEIGHTS.y + c.b * LUMA_WEIGHTS.z
	return total / float(MEAN_LUMA_SAMPLE_SIZE * MEAN_LUMA_SAMPLE_SIZE)


func get_layer_mean_luma() -> PackedFloat32Array:
	## Per-terrain-id mean luminance (0..1) of the base layers; empty until textures are built.
	return layer_mean_luma


func _build_canopy_texture_array() -> Texture2DArray:
	## Build the canopy Texture2DArray from `textures/canopy/NN_name.png` (RGBA crowns on transparency).
	## Skips biomes with no canopy file, recording `canopy_layer_by_id[terrain_id] = array layer` for the
	## ones present. Returns null when no canopy asset exists (shader then runs canopy-disabled).
	const CANOPY_PATH := "res://assets/terrain/textures/canopy/"
	var terrain_count: int = TerrainDefinitions.get_terrain_count()
	var terrain_names: Dictionary = TerrainDefinitions.get_names_dict()
	var images: Array[Image] = []
	var first_size: Vector2i = Vector2i.ZERO
	canopy_layer_by_id.clear()

	for terrain_id: int in range(terrain_count):
		var tname: String = terrain_names.get(terrain_id, "unknown")
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var abs_path := ProjectSettings.globalize_path(CANOPY_PATH + filename)
		if not FileAccess.file_exists(abs_path):
			continue
		var img: Image = Image.load_from_file(abs_path)
		if img == null:
			continue
		if first_size == Vector2i.ZERO:
			first_size = Vector2i(img.get_width(), img.get_height())
		elif Vector2i(img.get_width(), img.get_height()) != first_size:
			img.resize(first_size.x, first_size.y)
		if img.get_format() != Image.FORMAT_RGBA8:
			img.convert(Image.FORMAT_RGBA8)
		# Generate mipmaps so the blend shader's trilinear (filter_linear_mipmap) canopy sampler AVERAGES
		# crowns into a smooth darker-green forest mass at far zoom instead of shimmering/aliasing. The base
		# biome array has none (filter_linear only) — canopy is the layer that visibly aliases when zoomed out
		# because whole crowns tile many times per tiny hex; if the base ever shimmers it can take mipmaps too.
		img.generate_mipmaps()
		canopy_layer_by_id[terrain_id] = images.size()
		images.append(img)

	if images.is_empty():
		print("[TerrainTextureManager] No canopy textures found (canopy overlay disabled)")
		return null

	var array_tex := Texture2DArray.new()
	var err := array_tex.create_from_images(images)
	if err != OK:
		push_error("[TerrainTextureManager] Failed to create canopy Texture2DArray: %d" % err)
		canopy_layer_by_id.clear()
		return null
	print("[TerrainTextureManager] Loaded canopy textures: %d layers" % images.size())
	return array_tex


func canopy_layer_for(terrain_id: int) -> int:
	## Canopy array layer for a terrain, or -1 when the biome has no canopy overlay.
	return int(canopy_layer_by_id.get(terrain_id, -1))


func _build_peak_texture_array() -> Texture2DArray:
	## Build the peak Texture2DArray from `textures/peaks/NN_name.png` (RGBA faceted mountain relief on
	## transparency). Mirrors the canopy build exactly (once-only Image.load_from_file, mipmaps + trilinear
	## for far-zoom stability). Skips biomes with no peak file, recording `peak_layer_by_id[terrain_id] =
	## array layer` for the ones present. Returns null when no peak asset exists (shader runs peak-disabled).
	const PEAK_PATH := "res://assets/terrain/textures/peaks/"
	var terrain_count: int = TerrainDefinitions.get_terrain_count()
	var terrain_names: Dictionary = TerrainDefinitions.get_names_dict()
	var images: Array[Image] = []
	var first_size: Vector2i = Vector2i.ZERO
	peak_layer_by_id.clear()

	for terrain_id: int in range(terrain_count):
		var tname: String = terrain_names.get(terrain_id, "unknown")
		var filename := "%02d_%s.png" % [terrain_id, tname]
		var abs_path := ProjectSettings.globalize_path(PEAK_PATH + filename)
		if not FileAccess.file_exists(abs_path):
			continue
		var img: Image = Image.load_from_file(abs_path)
		if img == null:
			continue
		if first_size == Vector2i.ZERO:
			first_size = Vector2i(img.get_width(), img.get_height())
		elif Vector2i(img.get_width(), img.get_height()) != first_size:
			img.resize(first_size.x, first_size.y)
		if img.get_format() != Image.FORMAT_RGBA8:
			img.convert(Image.FORMAT_RGBA8)
		# Mipmaps so the blend shader's trilinear (filter_linear_mipmap) peak sampler AVERAGES the faceted
		# relief into a smooth raised mountain mass at far zoom instead of shimmering/aliasing (same reason
		# as the canopy crowns — whole peaks tile many times per tiny hex when zoomed out).
		img.generate_mipmaps()
		peak_layer_by_id[terrain_id] = images.size()
		images.append(img)

	if images.is_empty():
		print("[TerrainTextureManager] No peak textures found (peak overlay disabled)")
		return null

	var array_tex := Texture2DArray.new()
	var err := array_tex.create_from_images(images)
	if err != OK:
		push_error("[TerrainTextureManager] Failed to create peak Texture2DArray: %d" % err)
		peak_layer_by_id.clear()
		return null
	print("[TerrainTextureManager] Loaded peak textures: %d layers" % images.size())
	return array_tex


func peak_layer_for(terrain_id: int) -> int:
	## Peak array layer for a terrain, or -1 when the biome has no peak overlay.
	return int(peak_layer_by_id.get(terrain_id, -1))


func _build_river_texture_array() -> Texture2DArray:
	## Build the river Texture2DArray from `textures/rivers/NN_class.png` (flowing water, RGBA/opaque) for
	## the blend shader's river passes. Mirrors the canopy/peak builds (once-only Image.load_from_file +
	## mipmaps, so the trilinear river sampler averages a thin band into a stable line at far zoom instead
	## of shimmering) with ONE difference: a river is not a biome, so the layer is keyed by the file's
	## numeric prefix, not by terrain id — 0 = Minor / 1 = Major (the hex-EDGE classes, layer = class - 1)
	## and 2 = the navigable CHANNEL water. Returns null when no river asset exists (shader runs
	## river-disabled).
	const RIVER_PATH := "res://assets/terrain/textures/rivers/"
	# Layers 0/1 are the 2-bit edge mask's Minor/Major (class 3 is reserved and never drawn); layer 2 is the
	# navigable channel. A river file's prefix must land in [0, RIVER_MAX_LAYERS).
	const RIVER_MAX_LAYERS := 3
	var by_layer: Dictionary = {}   # layer index (class - 1) -> Image
	var dir := DirAccess.open(RIVER_PATH)
	if dir == null:
		print("[TerrainTextureManager] No river textures found (river overlay disabled)")
		return null
	var first_size: Vector2i = Vector2i.ZERO
	for filename: String in dir.get_files():
		if not filename.ends_with(".png"):
			continue
		var prefix: String = filename.split("_")[0]
		if not prefix.is_valid_int():
			push_warning("[TerrainTextureManager] River texture '%s' has no NN_ layer prefix — skipped" % filename)
			continue
		var layer := int(prefix)
		if layer < 0 or layer >= RIVER_MAX_LAYERS:
			push_warning("[TerrainTextureManager] River texture '%s' layer %d out of range — skipped" % [filename, layer])
			continue
		var img: Image = Image.load_from_file(ProjectSettings.globalize_path(RIVER_PATH + filename))
		if img == null:
			continue
		if first_size == Vector2i.ZERO:
			first_size = Vector2i(img.get_width(), img.get_height())
		elif Vector2i(img.get_width(), img.get_height()) != first_size:
			img.resize(first_size.x, first_size.y)
		if img.get_format() != Image.FORMAT_RGBA8:
			img.convert(Image.FORMAT_RGBA8)
		img.generate_mipmaps()
		by_layer[layer] = img

	if by_layer.is_empty():
		print("[TerrainTextureManager] No river textures found (river overlay disabled)")
		return null

	# The shader indexes the array by (class - 1), so the layers must be dense from 0 — a gap would make
	# every higher class sample the wrong water. Fill any hole with the lowest present layer.
	var layers: Array = by_layer.keys()
	layers.sort()
	var images: Array[Image] = []
	for layer: int in range(int(layers[layers.size() - 1]) + 1):
		if by_layer.has(layer):
			images.append(by_layer[layer])
		else:
			push_warning("[TerrainTextureManager] River layer %d missing — reusing layer %d" % [layer, int(layers[0])])
			images.append(by_layer[layers[0]])

	var array_tex := Texture2DArray.new()
	var err := array_tex.create_from_images(images)
	if err != OK:
		push_error("[TerrainTextureManager] Failed to create river Texture2DArray: %d" % err)
		return null
	print("[TerrainTextureManager] Loaded river textures: %d layers" % images.size())
	return array_tex


func get_config_value(key: String, default: Variant = null) -> Variant:
	return terrain_config.get(key, default)


func get_terrain_image(terrain_id: int) -> Image:
	## Return a single terrain layer as an Image (for 2D hex/edge cache building). Serves the CPU-side
	## copy captured at build time — NEVER Texture2DArray.get_layer_data() (a second readback returns
	## a blank image on some drivers, which blanked the base terrain on cache rebuild). Returns a
	## duplicate so callers can resize/convert without corrupting the shared source.
	if terrain_id < 0 or terrain_id >= _layer_images.size():
		return null
	var img: Image = _layer_images[terrain_id]
	return img.duplicate() if img != null else null


func is_ready() -> bool:
	## Returns true if textures are loaded and available.
	return terrain_textures != null and terrain_textures.get_layers() > 0
