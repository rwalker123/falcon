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
# array layer (absent = no canopy). Today only 12 (mixed_woodland) has a canopy.
var canopy_textures: Texture2DArray = null
var canopy_layer_by_id: Dictionary = {}
# Peak overlay: faceted mountain relief on transparency (RGBA), a THIRD Texture2DArray sampled by the
# blend shader over the (rocky) highland/volcanic base floor — the mountain-drama analog of the canopy
# overlay. Only biomes with a `textures/peaks/NN_name.png` asset get a layer; `peak_layer_by_id` maps
# terrain id → that array layer (absent = no peaks). Today only 26 (alpine_mountain) has a peak asset.
var peak_textures: Texture2DArray = null
var peak_layer_by_id: Dictionary = {}
var terrain_config: Dictionary = {}
var use_terrain_textures: bool = false
var use_edge_blending: bool = false

# CPU-side copy of every terrain layer, captured ONCE at build time. Reused by the hex-texture
# cache and the get_terrain_image readback so we never call Texture2DArray.get_layer_data() again — a
# second readback returns a blank image on some drivers, which blanked the base terrain on any cache rebuild.
var _layer_images: Array[Image] = []
# terrain_id -> blend_class ("flat" | "water" | "rugged"), parsed once from terrain_config.
var _blend_class_by_id: Dictionary = {}


func _ready() -> void:
	_load_config()
	_build_blend_class_map()
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

	return array_tex


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
