extends Node3D
class_name WaterLayer3D

const WATER_SHADER := preload("res://src/shaders/water.gdshader")

@export var tile_scale := 1.0
@export var sea_level_offset := 0.05 # Slightly above calculated sea level to avoid z-fighting

var _mesh_instance: MeshInstance3D
var _material: ShaderMaterial
var _width: int = 0
var _height: int = 0
var _sea_level: float = 0.6
var _exaggeration: float = 80.0
var _sea_level_override: float = -1.0

# Terrain tags bitmask (must match MapView.gd)
const TAG_WATER := 1 << 0
const TAG_FRESHWATER := 1 << 1
const TAG_COASTAL := 1 << 2

func _ready() -> void:
	_mesh_instance = MeshInstance3D.new()
	_mesh_instance.name = "WaterMesh"
	_mesh_instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_mesh_instance)
	
	_material = ShaderMaterial.new()
	_material.shader = WATER_SHADER
	_mesh_instance.material_override = _material

func apply_config(config: Dictionary) -> void:
	if config.has("sea_level_offset"):
		sea_level_offset = float(config["sea_level_offset"])
	if config.has("sea_level_override"):
		_sea_level_override = float(config["sea_level_override"])
	if config.has("visible"):
		_mesh_instance.visible = bool(config["visible"])
	
	if _material:
		if config.has("color_deep"):
			var c = config["color_deep"]
			_material.set_shader_parameter("deep_color", Color(c[0], c[1], c[2], c[3]))
		if config.has("color_coastal"):
			var c = config["color_coastal"]
			_material.set_shader_parameter("coastal_color", Color(c[0], c[1], c[2], c[3]))
		if config.has("color_fresh"):
			var c = config["color_fresh"]
			_material.set_shader_parameter("fresh_color", Color(c[0], c[1], c[2], c[3]))

func update_water_level(sea_level: float, exaggeration: float) -> void:
	_sea_level = sea_level
	_exaggeration = exaggeration
	# If we already have a mesh, we might need to rebuild it if we want to be perfect,
	# but for now, let's assume update_water is called after this or frequently enough.
	# Actually, if we just change the level, we should probably trigger a rebuild if we have data.
	# But simpler to just rely on the next update_water call for now, or just store values.

func update_water(width: int, height: int, tags: PackedInt32Array, terrain_ids: PackedInt32Array, heightfield_data: Dictionary) -> void:
	if width <= 0 or height <= 0:
		_clear_mesh()
		return
		
	_width = width
	_height = height
	
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	
	# We can use a single surface for now. If performance becomes an issue, we can chunk it.
	# For water, we just need a flat quad at sea level for each water tile.
	
	var water_count := 0
	var debug_printed := 0
	
	print("[WaterLayer3D] update_water: w=%d h=%d tags_len=%d ids_len=%d" % [width, height, tags.size(), terrain_ids.size()])

	for y in range(height):
		for x in range(width):
			var idx := y * width + x
			var is_water := false
			var tag_mask := 0
			
			if idx < tags.size():
				tag_mask = tags[idx]
				if (tag_mask & TAG_WATER) != 0 or (tag_mask & TAG_FRESHWATER) != 0 or (tag_mask & TAG_COASTAL) != 0:
					is_water = true
			
			var tid := -999
			if idx < terrain_ids.size():
				tid = terrain_ids[idx]

			if not is_water:
				# 0: Deep Ocean, 1: Continental Shelf, 2: Inland Sea, 3: Coral Shelf
				# Reverting the <= 0 check to strictly check 0 for debugging purposes as requested
				if tid == 0 or tid == 1 or tid == 2 or tid == 3:
					is_water = true
			
			# Debug logging for suspicious tiles (e.g. Deep Ocean but not water)
			if tid == 0 and not is_water:
				if debug_printed < 10:
					print("[WaterLayer3D] FAIL: x=%d y=%d idx=%d tag_mask=%d tid=%d is_water=%s" % [x, y, idx, tag_mask, tid, is_water])
					debug_printed += 1
			elif tid == 0 and is_water and debug_printed < 5:
				# Print a few successes too
				print("[WaterLayer3D] OK: x=%d y=%d idx=%d tag_mask=%d tid=%d is_water=%s" % [x, y, idx, tag_mask, tid, is_water])
				debug_printed += 1

			if is_water:
				_add_water_quad(st, x, y, tag_mask)
				water_count += 1
				
	if water_count > 0:
		st.generate_normals()
		st.generate_tangents()
		_mesh_instance.mesh = st.commit()
		print("[WaterLayer3D] Generated mesh with %d water tiles." % water_count)
	else:
		_clear_mesh()
		print("[WaterLayer3D] No water tiles found. Cleared mesh.")

func _add_water_quad(st: SurfaceTool, x: int, y: int, tag_mask: int) -> void:
	var fx := float(x)
	var fy := float(y)
	var ts := tile_scale
	
	# Determine water type for shader (optional, can pass via vertex color or UV2)
	# For now, let's just use a simple blue.
	# We can encode type in Color: R=Type, G=Unused, B=Unused
	var type_val := 0.0
	if (tag_mask & TAG_FRESHWATER) != 0:
		type_val = 0.5 # Freshwater
	elif (tag_mask & TAG_COASTAL) != 0:
		type_val = 0.25 # Coastal
	else:
		type_val = 0.0 # Deep Ocean
		
	var color := Color(type_val, 0.0, 0.0, 1.0)
	
	# Quad vertices (0,0) to (1,1) scaled
	# v0 -- v1
	# |      |
	# v2 -- v3
	
	# However, HeightfieldLayer3D likely uses vertices at grid points.
	# Let's assume (x, y) is the top-left corner of the tile.
	# We want the water to cover the tile.
	
	var effective_sea_level := _sea_level
	if _sea_level_override > -1.0:
		effective_sea_level = _sea_level_override
		
	var y_pos := (effective_sea_level * _exaggeration) + sea_level_offset
	
	var v0 := Vector3(fx * ts, y_pos, fy * ts)
	var v1 := Vector3((fx + 1.0) * ts, y_pos, fy * ts)
	var v2 := Vector3(fx * ts, y_pos, (fy + 1.0) * ts)
	var v3 := Vector3((fx + 1.0) * ts, y_pos, (fy + 1.0) * ts)
	
	var uv0 := Vector2(0.0, 0.0)
	var uv1 := Vector2(1.0, 0.0)
	var uv2 := Vector2(0.0, 1.0)
	var uv3 := Vector2(1.0, 1.0)
	
	# Triangle 1: v0, v1, v2
	st.set_color(color)
	st.set_uv(uv0)
	st.add_vertex(v0)
	
	st.set_color(color)
	st.set_uv(uv1)
	st.add_vertex(v1)
	
	st.set_color(color)
	st.set_uv(uv2)
	st.add_vertex(v2)
	
	# Triangle 2: v1, v3, v2
	st.set_color(color)
	st.set_uv(uv1)
	st.add_vertex(v1)
	
	st.set_color(color)
	st.set_uv(uv3)
	st.add_vertex(v3)
	
	st.set_color(color)
	st.set_uv(uv2)
	st.add_vertex(v2)

func _clear_mesh() -> void:
	_mesh_instance.mesh = null
