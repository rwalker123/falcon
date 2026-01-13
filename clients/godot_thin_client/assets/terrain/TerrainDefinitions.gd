class_name TerrainDefinitions
## Single source of truth for terrain definitions.
## Loads from terrain_config.json to ensure consistency across all terrain-related scripts.

const CONFIG_PATH := "res://assets/terrain/terrain_config.json"

# Cached data
static var _loaded := false
static var _terrains: Array[Dictionary] = []
static var _by_id: Dictionary = {}
static var _by_name: Dictionary = {}
static var _config: Dictionary = {}


static func _ensure_loaded() -> void:
	if _loaded:
		return
	_load_config()


static func _load_config() -> void:
	# Try res:// path first (works in editor and exported builds)
	var file := FileAccess.open(CONFIG_PATH, FileAccess.READ)
	if not file:
		# Fallback: try globalized path
		var abs_path := ProjectSettings.globalize_path(CONFIG_PATH)
		if FileAccess.file_exists(abs_path):
			file = FileAccess.open(abs_path, FileAccess.READ)

	if not file:
		push_error("TerrainDefinitions: Config file not found: %s" % CONFIG_PATH)
		_loaded = true
		return

	var json_text := file.get_as_text()
	file.close()

	var parsed: Variant = JSON.parse_string(json_text)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_error("TerrainDefinitions: Invalid config format")
		_loaded = true
		return

	_config = parsed as Dictionary
	var terrains_array: Array = _config.get("terrains", [])

	_terrains.clear()
	_by_id.clear()
	_by_name.clear()

	for entry: Variant in terrains_array:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		var terrain: Dictionary = entry as Dictionary
		_terrains.append(terrain)
		# Explicitly cast to int since JSON may parse numbers as float
		var tid: int = int(terrain.get("id", -1))
		var tname: String = terrain.get("name", "")
		if tid >= 0:
			_by_id[tid] = terrain
		if tname != "":
			_by_name[tname] = terrain

	_loaded = true
	if _terrains.size() > 0:
		print("[TerrainDefinitions] Loaded %d terrain definitions" % _terrains.size())
	else:
		push_warning("[TerrainDefinitions] No terrains found in config")


static func get_terrain_count() -> int:
	_ensure_loaded()
	return _terrains.size()


static func get_terrains() -> Array[Dictionary]:
	_ensure_loaded()
	return _terrains


static func get_terrain(id: int) -> Dictionary:
	_ensure_loaded()
	return _by_id.get(id, {})


static func get_terrain_by_name(tname: String) -> Dictionary:
	_ensure_loaded()
	return _by_name.get(tname, {})


static func get_name(id: int) -> String:
	var terrain := get_terrain(id)
	return terrain.get("name", "unknown")


static func get_label(id: int) -> String:
	var terrain := get_terrain(id)
	return terrain.get("label", "Unknown")


static func get_category(id: int) -> String:
	var terrain := get_terrain(id)
	return terrain.get("category", "")


static func get_color(id: int) -> Color:
	var terrain := get_terrain(id)
	var color_arr: Array = terrain.get("color", [128, 128, 128])
	if color_arr.size() >= 3:
		return Color8(int(color_arr[0]), int(color_arr[1]), int(color_arr[2]))
	return Color.GRAY


static func get_names_dict() -> Dictionary:
	## Returns {id: name} dictionary for compatibility with existing code.
	_ensure_loaded()
	var result := {}
	for terrain: Dictionary in _terrains:
		# Explicitly cast to int since JSON may parse as float
		var tid: int = int(terrain.get("id", -1))
		result[tid] = terrain.get("name", "unknown")
	return result


static func get_colors_dict() -> Dictionary:
	## Returns {id: Color} dictionary for compatibility with existing code.
	_ensure_loaded()
	var result := {}
	for terrain: Dictionary in _terrains:
		# Explicitly cast to int since JSON may parse numbers as float
		var tid: int = int(terrain.get("id", -1))
		result[tid] = get_color(tid)
	return result


static func get_config_value(key: String, default: Variant = null) -> Variant:
	## Access other config values like texture_scale, blend_width, etc.
	_ensure_loaded()
	return _config.get(key, default)


static func reload() -> void:
	## Force reload from disk (useful for editor tools).
	_loaded = false
	_ensure_loaded()
