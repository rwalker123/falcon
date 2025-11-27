extends Node3D
class_name UnitOverlay3D

# Configuration
var marker_scale: float = 1.0
var marker_y_offset: float = 2.0
var visible_markers: bool = true

# Resources
var _marker_mesh: Mesh
var _marker_material: StandardMaterial3D

# State
var _markers: Array[MeshInstance3D] = []
var _active_marker_count: int = 0

# Constants copied from MapView for consistency
const FACTION_COLORS := {
	"Aurora": Color(0.55, 0.85, 1.0, 1.0),
	"Obsidian": Color(0.95, 0.62, 0.2, 1.0),
	"Verdant": Color(0.4, 0.9, 0.55, 1.0),
	"0": Color(0.55, 0.85, 1.0, 1.0),
	"1": Color(0.95, 0.62, 0.2, 1.0),
	"2": Color(0.4, 0.9, 0.55, 1.0)
}

const FOOD_SITE_STYLES := {
	"littoral": {"color": Color(0.95, 0.74, 0.32, 0.9), "shape": "diamond"},
	"river_garden": {"color": Color(0.4, 0.75, 0.9, 0.9), "shape": "droplet"},
	"savanna_track": {"color": Color(0.92, 0.78, 0.4, 0.9), "shape": "triangle"},
	"forest_forage": {"color": Color(0.52, 0.78, 0.56, 0.9), "shape": "square"},
	"arctic_fishing": {"color": Color(0.78, 0.88, 0.98, 0.9), "shape": "circle"},
	"highland_grove": {"color": Color(0.78, 0.7, 0.9, 0.9), "shape": "diamond"},
	"wetland_harvest": {"color": Color(0.42, 0.66, 0.52, 0.9), "shape": "square"},
	"scrub_roots": {"color": Color(0.9, 0.6, 0.38, 0.9), "shape": "triangle"},
	"upwelling_drying": {"color": Color(0.58, 0.84, 0.94, 0.9), "shape": "droplet"},
	"woodland_cache": {"color": Color(0.6, 0.78, 0.66, 0.9), "shape": "circle"},
	"game_trail": {"color": Color(0.85, 0.5, 0.35, 0.95), "shape": "circle"}
}

func _ready() -> void:
	_marker_mesh = CylinderMesh.new()
	_marker_mesh.top_radius = 0.5
	_marker_mesh.bottom_radius = 0.5
	_marker_mesh.height = 0.3
	_marker_material = StandardMaterial3D.new()
	_marker_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_marker_material.albedo_color = Color.WHITE

func update_markers(snapshot: Dictionary, height_layer: Node) -> void:
	if not visible_markers:
		_hide_all_markers()
		return
		
	if height_layer == null:
		return

	_active_marker_count = 0
	
	# Process Units
	var units = snapshot.get("units", [])
	if units is Array:
		for unit in units:
			if unit is Dictionary:
				_add_unit_marker(unit, height_layer)
				
	# Process Herds
	var herds = snapshot.get("herds", [])
	if herds is Array:
		for herd in herds:
			if herd is Dictionary:
				_add_herd_marker(herd, height_layer)
				
	# Process Food Sites
	var food_sites = snapshot.get("food_modules", [])
	if food_sites is Array:
		for site in food_sites:
			if site is Dictionary:
				_add_food_marker(site, height_layer)

	# Hide unused markers
	for i in range(_active_marker_count, _markers.size()):
		_markers[i].visible = false
	
	# print("[UnitOverlay3D] Active markers: ", _active_marker_count)

func apply_config(config: Dictionary) -> void:
	if config.has("markers"):
		var m_config = config["markers"]
		if m_config.has("visible"):
			visible_markers = bool(m_config["visible"])
		if m_config.has("scale"):
			marker_scale = float(m_config["scale"])
		if m_config.has("y_offset"):
			marker_y_offset = float(m_config["y_offset"])

func _add_unit_marker(unit: Dictionary, height_layer: Node) -> void:
	var pos_arr = unit.get("pos", [])
	if pos_arr.size() < 2:
		return
		
	var grid_x = pos_arr[0]
	var grid_y = pos_arr[1]
	
	var faction_id = str(unit.get("faction_id", "0"))
	var color = FACTION_COLORS.get(faction_id, Color.WHITE)
	
	_place_marker(grid_x, grid_y, color, height_layer, 1.0)

func _add_herd_marker(herd: Dictionary, height_layer: Node) -> void:
	var x = int(herd.get("x", -1))
	var y = int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
		
	# Herds are usually white or greyish
	_place_marker(x, y, Color(0.9, 0.9, 0.9), height_layer, 1.0)

func _add_food_marker(site: Dictionary, height_layer: Node) -> void:
	var x = int(site.get("x", -1))
	var y = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
		
	var module = str(site.get("module", ""))
	var style = FOOD_SITE_STYLES.get(module, FOOD_SITE_STYLES.get("game_trail"))
	var color = style.get("color", Color.GREEN)
	
	_place_marker(x, y, color, height_layer, 0.8)

func _place_marker(grid_x: int, grid_y: int, color: Color, height_layer: Node, scale_mod: float) -> void:
	# Get the actual hex center from HeightfieldLayer3D
	var hex_center: Vector3
	if height_layer.has_method("get_hex_center"):
		hex_center = height_layer.get_hex_center(grid_x, grid_y)
	else:
		# Fallback to simple calculation
		var tile_scale = height_layer.get("tile_scale") if "tile_scale" in height_layer else 1.0
		var world_x = float(grid_x) * tile_scale
		var world_z = float(grid_y) * tile_scale
		var y_height = 0.0
		if height_layer.has_method("_height_at_world"):
			y_height = height_layer._height_at_world(world_x, world_z)
		hex_center = Vector3(world_x, y_height, world_z)
	
	var marker = _get_marker_instance()
	marker.visible = true
	# Cylinder height is 0.3, so center is at 0.15 above base
	# Position at hex center Y + cylinder half height + y_offset
	var cylinder_half_height = 0.15
	marker.position = Vector3(hex_center.x, hex_center.y + cylinder_half_height + marker_y_offset, hex_center.z)
	
	var final_scale = marker_scale * scale_mod
	marker.scale = Vector3(final_scale, final_scale, final_scale)
	
	# Update color (material already duplicated in _get_marker_instance)
	(marker.material_override as StandardMaterial3D).albedo_color = color

func _get_marker_instance() -> MeshInstance3D:
	if _active_marker_count < _markers.size():
		var m = _markers[_active_marker_count]
		_active_marker_count += 1
		return m
	else:
		var m = MeshInstance3D.new()
		m.mesh = _marker_mesh
		m.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# Duplicate material once when creating the marker
		m.material_override = _marker_material.duplicate()
		add_child(m)
		_markers.append(m)
		_active_marker_count += 1
		return m

func _hide_all_markers() -> void:
	for m in _markers:
		m.visible = false
	_active_marker_count = 0

func _add_debug_marker(height_layer: Node) -> void:
	# Place a marker at 10,10 to verify visibility
	var x = 10
	var y = 10
	var color = Color(1.0, 0.0, 1.0) # Magenta for high visibility
	
	print("[UnitOverlay3D] Adding manual debug marker at %d,%d" % [x, y])
	_place_marker(x, y, color, height_layer, 2.0)
