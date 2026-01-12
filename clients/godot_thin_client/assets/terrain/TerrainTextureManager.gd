extends Node
## Singleton that centralizes terrain texture loading and configuration.
## Shared by MapView (2D) and HeightfieldLayer3D (3D) to avoid duplicate loading.
## Registered as autoload "TerrainTextureManager" in project.godot.

const TERRAIN_CONFIG_PATH := "res://assets/terrain/terrain_config.json"
const TerrainDefinitions := preload("res://assets/terrain/TerrainDefinitions.gd")

var terrain_textures: Texture2DArray = null
var terrain_config: Dictionary = {}
var use_terrain_textures: bool = false
var use_edge_blending: bool = false


func _ready() -> void:
	_load_config()
	_load_textures()


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
	# Build texture array from individual PNGs at runtime
	terrain_textures = _build_terrain_texture_array()
	if terrain_textures != null and terrain_textures.get_layers() > 0:
		print("[TerrainTextureManager] Loaded terrain textures: %d layers" % terrain_textures.get_layers())
	else:
		print("[TerrainTextureManager] Terrain textures not found (using solid colors)")


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
		# Try loading via ResourceLoader first (works with imported resources)
		if ResourceLoader.exists(filepath):
			var tex: Texture2D = load(filepath)
			if tex:
				img = tex.get_image()
		# Fallback to direct file loading
		if img == null:
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

	return array_tex


func get_config_value(key: String, default: Variant = null) -> Variant:
	return terrain_config.get(key, default)


func get_terrain_image(terrain_id: int) -> Image:
	## Extract a single terrain layer as an Image (for 2D hex cache building).
	if terrain_textures == null:
		return null
	if terrain_id < 0 or terrain_id >= terrain_textures.get_layers():
		return null
	return terrain_textures.get_layer_data(terrain_id)


func is_ready() -> bool:
	## Returns true if textures are loaded and available.
	return terrain_textures != null and terrain_textures.get_layers() > 0
