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

# PER-TERRAIN BLEND PROFILE (R = width_scale, G = noise_scale, B = noise_cell_scale), one texel per terrain id
# — the same 1×N by-layer-index lookup table as layer_shore_texture, and the flat↔flat seam's analog of it.
# The global blend levers (blend_width / blend_noise_amount / blend_noise_scale) are ONE ecotone, tuned for the
# biome pairs that actually border each other on the map: neighbours a few brightness points apart, sharing a
# hue. Against a pair that is far apart in BOTH tone and hue the very same ecotone reads as a blurred hex edge
# — the boundary is only ~0.35·r wide and, because the wobble's displacement is a fraction of that, near
# STRAIGHT, so the eye locks onto the hexagon. The NavigableRiver BANK (id 37, a grey low-contrast gravel at
# mean luma 89) is exactly that pair against every neighbour a river corridor actually has: prairie/scrub at
# 112–127 on one side, floodplain/alluvial at 55–58 on the other. So a terrain may widen and roughen the seam
# it is ON, without touching anybody else's:
#   width_scale      — multiplies blend_band (the ecotone's REACH). The lever that turns a blurred edge into a
#                      transition you read as a valley floor merging into the land.
#   noise_scale      — multiplies blend_noise_amount (the boundary wobble's AMPLITUDE), so the boundary leaves
#                      the hexagon polyline instead of tracing it.
#   noise_cell_scale — multiplies blend_noise_cell (the wobble's WAVELENGTH). Amplitude without wavelength is a
#                      fine fringe along a straight line, not a meander: the lobes must be a fair fraction of
#                      the (now wider) band to read as organic.
# A terrain with no `blend_profile` block gets the NEUTRAL default (1, 1, 1) — bit-identical to before this
# table existed. Read by the shader as `layer_blend_map` and combined across an edge with **max()**, which is
# COMMUTATIVE: both hexes flanking a seam derive the same three scales, so the seam weight, the wobble and its
# cell are identical from both frames and the boundary stays continuous by construction (the same cross-edge
# agreement discipline the shore profile's water-side keying buys). A seam between two unprofiled terrains is
# max(1,1) = 1 on every axis — every one of main's biome seams is untouched.
var layer_blend_texture: ImageTexture = null

const BLEND_PROFILE_DEFAULT_WIDTH_SCALE := 1.0
const BLEND_PROFILE_DEFAULT_NOISE_SCALE := 1.0
const BLEND_PROFILE_DEFAULT_NOISE_CELL_SCALE := 1.0
# Guard rail: the band is a fraction of the hex radius and the apothem is 0.866·r, so a reach past ~4× the
# shipped 0.25·r would let one seam's ecotone cross the hex and collide with the opposite one.
const BLEND_PROFILE_MAX_SCALE := 4.0

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
	rebuild_layer_blend_map()
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


func _load_asset_image(res_path: String) -> Image:
	## Load a texture asset as an Image in BOTH the editor and an exported build.
	## Exports convert PNGs to `.ctex` inside the `.pck`, where `Image.load_from_file`
	## (OS-filesystem only) cannot reach them — so the imported resource is the primary
	## path and a loose-file read is only the fallback for un-imported assets.
	## The `exists` probe is NOT redundant: the canopy/peak builders ask for a path per biome and treat
	## an absent asset as "this biome has no overlay", and a bare `load()` on a missing path PUSHES AN
	## ERROR — which turned that ordinary negative answer into ~70 error lines per launch.
	var tex: Texture2D = null
	if ResourceLoader.exists(res_path):
		tex = ResourceLoader.load(res_path) as Texture2D
	if tex != null:
		var img := tex.get_image()
		if img != null:
			# Never mutate the cached resource's own Image — callers resize/convert/mipmap it.
			img = img.duplicate() as Image
			if img.is_compressed():
				img.decompress()
			return img
	var abs_path := ProjectSettings.globalize_path(res_path)
	if FileAccess.file_exists(abs_path):
		return Image.load_from_file(abs_path)
	return null


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

		var img: Image = _load_asset_image(filepath)

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


func rebuild_layer_blend_map() -> void:
	## Pack each terrain's optional `blend_profile` block into a 1×N RGBA float texture the shader fetches by
	## layer index — the flat↔flat seam's twin of rebuild_layer_shore_map (same construction, same in-place
	## ImageTexture update so MapView's one-time `layer_blend_map` binding survives a rebuild, same PUBLIC
	## reason: the blend probe sweeps profiles by mutating the live config and calling this).
	## Terrains with no block get the neutral (1, 1, 1) — a bit-exact no-op on their seams.
	var terrain_count: int = TerrainDefinitions.get_terrain_count()
	if terrain_count <= 0:
		return
	var neutral := Color(
		BLEND_PROFILE_DEFAULT_WIDTH_SCALE,
		BLEND_PROFILE_DEFAULT_NOISE_SCALE,
		BLEND_PROFILE_DEFAULT_NOISE_CELL_SCALE,
		1.0
	)
	var blend_img := Image.create(terrain_count, 1, false, Image.FORMAT_RGBAF)
	for terrain_id: int in range(terrain_count):
		blend_img.set_pixel(terrain_id, 0, neutral)
	for entry: Variant in terrain_config.get("terrains", []):
		if not (entry is Dictionary):
			continue
		var tid: int = int(entry.get("id", -1))
		if tid < 0 or tid >= terrain_count:
			continue
		var profile: Variant = entry.get("blend_profile", null)
		if not (profile is Dictionary):
			continue
		blend_img.set_pixel(tid, 0, Color(
			_blend_scale(profile, "width_scale", BLEND_PROFILE_DEFAULT_WIDTH_SCALE),
			_blend_scale(profile, "noise_scale", BLEND_PROFILE_DEFAULT_NOISE_SCALE),
			_blend_scale(profile, "noise_cell_scale", BLEND_PROFILE_DEFAULT_NOISE_CELL_SCALE),
			1.0
		))
	if layer_blend_texture == null:
		layer_blend_texture = ImageTexture.create_from_image(blend_img)
	else:
		layer_blend_texture.update(blend_img)


func _blend_scale(profile: Dictionary, key: String, fallback: float) -> float:
	## One `blend_profile` scale, defaulted and guard-railed. A missing key is NEUTRAL (the terrain keeps the
	## global lever on that axis), so a partial block is legal.
	return clampf(float(profile.get(key, fallback)), 0.0, BLEND_PROFILE_MAX_SCALE)


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
	## Loaded via _load_asset_image so exported builds (where the PNG is a `.ctex` in the `.pck`) work too.
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
		# A missing canopy asset means "this biome has no overlay" — skip it.
		var img: Image = _load_asset_image(CANOPY_PATH + filename)
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
	## transparency). Mirrors the canopy build exactly (once-only _load_asset_image, mipmaps + trilinear
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
		# A missing peak asset means "this biome has no overlay" — skip it.
		var img: Image = _load_asset_image(PEAK_PATH + filename)
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
	## the blend shader's river passes. Mirrors the canopy/peak builds (once-only _load_asset_image +
	## mipmaps, so the trilinear river sampler averages a thin band into a stable line at far zoom instead
	## of shimmering) with ONE difference: a river is not a biome, so the layer is keyed by the file's
	## numeric prefix, not by terrain id — 0 = Minor / 1 = Major (the hex-EDGE classes, layer = class - 1)
	## and 2 = the navigable CHANNEL water. **Layer 0 (Minor) is REQUIRED** — it anchors the `class - 1`
	## indexing, so without it every class would sample the wrong water; higher layers may be densified
	## from layer 0. Returns null when no river asset exists, or when layer 0 is missing (shader runs
	## river-disabled).
	const RIVER_PATH := "res://assets/terrain/textures/rivers/"
	# Layers 0/1 are the 2-bit edge mask's Minor/Major (class 3 is reserved and never drawn); layer 2 is the
	# navigable channel. A river file's prefix must land in [0, RIVER_MAX_LAYERS).
	const RIVER_MAX_LAYERS := 3
	# A directory listing does NOT report the authored `NN_class.png` verbatim: in an EXPORTED build an
	# imported resource is listed with a trailing `.remap`, and in the editor the `.import` sidecar shows up
	# beside the source. Both are stripped before the `.png` test (and the results de-duplicated), so the
	# directory-driven layer contract holds in both environments without a hardcoded filename roster.
	const RESOURCE_REMAP_SUFFIX := ".remap"
	const RESOURCE_IMPORT_SUFFIX := ".import"
	var by_layer: Dictionary = {}   # layer index (class - 1) -> Image
	var dir := DirAccess.open(RIVER_PATH)
	if dir == null:
		print("[TerrainTextureManager] No river textures found (river overlay disabled)")
		return null
	var first_size: Vector2i = Vector2i.ZERO
	var seen_files: Dictionary = {}
	for entry: String in dir.get_files():
		var filename: String = entry
		if filename.ends_with(RESOURCE_REMAP_SUFFIX):
			filename = filename.trim_suffix(RESOURCE_REMAP_SUFFIX)
		elif filename.ends_with(RESOURCE_IMPORT_SUFFIX):
			filename = filename.trim_suffix(RESOURCE_IMPORT_SUFFIX)
		if not filename.ends_with(".png"):
			continue
		if seen_files.has(filename):
			continue
		seen_files[filename] = true
		var prefix: String = filename.split("_")[0]
		if not prefix.is_valid_int():
			push_warning("[TerrainTextureManager] River texture '%s' has no NN_ layer prefix — skipped" % filename)
			continue
		var layer := int(prefix)
		if layer < 0 or layer >= RIVER_MAX_LAYERS:
			push_warning("[TerrainTextureManager] River texture '%s' layer %d out of range — skipped" % [filename, layer])
			continue
		var img: Image = _load_asset_image(RIVER_PATH + filename)
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

	# The shader indexes the array by (class - 1), so layer 0 IS Minor and the layers must be dense from
	# 0. Without layer 0 the indexing premise is void — every class would sample the wrong water (Minor
	# would render as Major) — so bail like the empty-directory case rather than densify a lie.
	const RIVER_ANCHOR_LAYER := 0
	if not by_layer.has(RIVER_ANCHOR_LAYER):
		push_warning("[TerrainTextureManager] River layer %d (Minor) missing — the array is indexed by (river class - 1), so without it every class would sample the wrong water. River overlay disabled." % RIVER_ANCHOR_LAYER)
		return null

	# Any remaining hole is densified from the anchor (never from "the lowest present layer").
	var layers: Array = by_layer.keys()
	layers.sort()
	var images: Array[Image] = []
	for layer: int in range(int(layers[layers.size() - 1]) + 1):
		if by_layer.has(layer):
			images.append(by_layer[layer])
		else:
			push_warning("[TerrainTextureManager] River layer %d missing — reusing layer %d" % [layer, RIVER_ANCHOR_LAYER])
			images.append(by_layer[RIVER_ANCHOR_LAYER])

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
