extends Node3D
class_name HeightfieldLayer3D

signal zoom_multiplier_changed(multiplier: float)
signal strategic_view_requested

const HEIGHTFIELD_SHADER := preload("res://src/shaders/heightfield.gdshader")
const HEX_GRID_SHADER := preload("res://src/shaders/hex_grid.gdshader")
const SQRT3 := 1.7320508075688772

@export var chunk_size := Vector2i(32, 32)
@export var tile_scale := 1.0
@export var height_exaggeration_default := 80.0
@export var overlay_strength := 0.75
@export var camera_distance_ratio_default := 1.0
@export var debug_visualize_height := true
@export var debug_visualize_axes := false
@export var debug_print_chunks := true
@export var debug_max_logged_chunks := 4
@export var debug_dump_vertices := true
@export var debug_max_vertices := 12
@export var show_hex_grid := true

var _width: int = 0
var _height: int = 0
var _mesh_root: Node3D
var _material: ShaderMaterial
var _biome_texture: Texture2D
var _overlay_texture: Texture2D
var _height_samples: PackedFloat32Array = PackedFloat32Array()
var _last_stats_signature: String = ""
var _logged_chunk_count: int = 0
var _max_height_world: float = 0.0
var _current_height_exaggeration: float
var _current_camera_distance_ratio: float
var _user_zoom_multiplier: float = 1.0
var _height_config: Dictionary = {}
var _last_camera: Camera3D = null
const HEIGHTFIELD_CONFIG_PATH := "res://src/data/heightfield_config.json"
var _hex_grid_instance: MeshInstance3D = null
var _hex_grid_color: Color = Color(0.1, 0.2, 0.4, 0.65)
var _hex_width_scale: float = 0.05
var _hex_min_width: float = 0.0
var _layout_ready: bool = false
var _layout_scale_x: float = 1.0
var _layout_scale_z: float = 1.0
var _layout_offset: Vector2 = Vector2.ZERO
const HEX_LAYOUT_RADIUS := 1.0
var _hex_centers: Dictionary = {}  # Stores Vector3(x,y,z) centers by "col,row" key
var _min_zoom_multiplier: float = 0.3
var _max_zoom_multiplier: float = 1.8
var _default_zoom_multiplier: float = 0.8
var _strategic_zoom_threshold: float = INF
var _strategic_request_emitted: bool = false
var _orbit_azimuth_radians: float = 0.0
var _pan_offset_world: Vector2 = Vector2.ZERO
var _tilt_degrees: float = 55.0
var _tilt_degrees_default: float = 55.0

func _ready() -> void:
    tile_scale = max(tile_scale, 0.1)
    _current_height_exaggeration = height_exaggeration_default
    _current_camera_distance_ratio = camera_distance_ratio_default
    _mesh_root = Node3D.new()
    _mesh_root.name = "HeightfieldMeshRoot"
    add_child(_mesh_root)
    _hex_grid_instance = MeshInstance3D.new()
    _hex_grid_instance.name = "HexGridOverlay"
    # Use a simple material that respects vertex colors instead of shader
    var hex_material := StandardMaterial3D.new()
    hex_material.vertex_color_use_as_albedo = true
    hex_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
    hex_material.cull_mode = BaseMaterial3D.CULL_DISABLED
    _hex_grid_instance.material_override = hex_material
    _hex_grid_instance.visible = show_hex_grid
    _hex_grid_instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
    _mesh_root.add_child(_hex_grid_instance)
    _material = ShaderMaterial.new()
    _material.shader = HEIGHTFIELD_SHADER
    _material.set_shader_parameter("overlay_mix", overlay_strength)
    _material.set_shader_parameter("overlay_enabled", false)
    _material.set_shader_parameter("ambient_strength", 0.35)
    _update_shader_debug_flags()
    


func set_heightfield_data(data: Dictionary) -> void:
    print("[HeightfieldLayer3D] set_heightfield_data called. Keys: ", data.keys())
    if data.is_empty():
        return
    var width: int = int(data.get("width", 0))
    var height: int = int(data.get("height", 0))
    if width <= 0 or height <= 0:
        _clear_mesh()
        return
    var samples: PackedFloat32Array = PackedFloat32Array(data.get("samples", PackedFloat32Array()))
    if samples.is_empty():
        _clear_mesh()
        return
    _height_samples = samples
    _apply_height_config(width, height)
    _log_height_stats(samples, width, height)
    _width = width
    _height = height
    _invalidate_layout_metrics()
    _reset_camera_state()
    _rebuild_chunks()
    _update_hex_overlay()

func set_biome_colors(colors: PackedColorArray, width: int, height: int) -> void:
    if _material == null:
        return
    var tex := _build_color_texture(colors, width, height)
    if tex == null:
        _biome_texture = null
        return
    _biome_texture = tex
    _material.set_shader_parameter("biome_texture", _biome_texture)

func set_overlay_values(values: PackedFloat32Array, width: int, height: int, color: Color, enabled: bool) -> void:
    if _material == null:
        return
    if not enabled or values.is_empty():
        _material.set_shader_parameter("overlay_enabled", false)
        _overlay_texture = null
        _update_shader_debug_flags()
        return
    var tex := _build_overlay_texture(values, width, height)
    if tex == null:
        _material.set_shader_parameter("overlay_enabled", false)
        _overlay_texture = null
        _update_shader_debug_flags()
        return
    _overlay_texture = tex
    _material.set_shader_parameter("overlay_texture", _overlay_texture)
    _material.set_shader_parameter("overlay_color", color)
    _material.set_shader_parameter("overlay_enabled", true)
    _material.set_shader_parameter("overlay_enabled", true)
    _update_shader_debug_flags()

func fit_to_view() -> void:
    _tilt_degrees = 90.0
    _orbit_azimuth_radians = 0.0
    _pan_offset_world = Vector2.ZERO
    _user_zoom_multiplier = 1.0
    if _last_camera != null:
        fit_camera(_last_camera, 90.0)

func fit_camera(camera: Camera3D, tilt_degrees: float = -1.0) -> void:
    if camera == null or _width <= 0 or _height <= 0:
        return
    _last_camera = camera
    if tilt_degrees >= 0.0:
        _tilt_degrees = clamp(tilt_degrees, 20.0, 90.0)
        _tilt_degrees_default = _tilt_degrees
    var dims := _map_dimensions_world()
    var base_center := Vector3(dims.x * 0.5, 0.0, dims.y * 0.5)
    var center := base_center + Vector3(_pan_offset_world.x, 0.0, _pan_offset_world.y)
    var radius: float = max(dims.x, dims.y)
    var highest: float = max(_max_height_world, _current_height_exaggeration * 0.25)
    var distance: float = (radius * _current_camera_distance_ratio + highest + 30.0) * _user_zoom_multiplier
    var tilt_radians: float = deg_to_rad(clampf(_tilt_degrees, 15.0, 90.0))
    var offset := Vector3(0.0, distance * sin(tilt_radians), distance * cos(tilt_radians))
    var orbit_basis := Basis(Vector3.UP, _orbit_azimuth_radians)
    offset = orbit_basis * offset
    var position := center + offset
    var look_target := center + Vector3(0.0, highest * 0.4, 0.0)
    
    var up_vector := Vector3.UP
    if abs(_tilt_degrees - 90.0) < 0.1:
        # When looking straight down, UP is degenerate. Use -Z (North) as up.
        up_vector = Vector3(0, 0, -1)
        
    camera.look_at_from_position(position, look_target, up_vector)
    camera.near = 0.1

func sync_from_2d(zoom_2d: float, pan_2d: Vector2, hex_radius_2d: float) -> void:
    _user_zoom_multiplier = 1.0 / max(zoom_2d, 0.001)
    var scale_ratio = tile_scale / max(hex_radius_2d, 1.0)
    _pan_offset_world = -pan_2d * scale_ratio
    _tilt_degrees = 90.0
    _orbit_azimuth_radians = 0.0
    if _last_camera != null:
        fit_camera(_last_camera, 90.0)

func _clear_mesh(reset_dims: bool = true) -> void:
    if _mesh_root == null:
        return
    for child in _mesh_root.get_children():
        if _hex_grid_instance != null and child == _hex_grid_instance:
            continue
        child.queue_free()
    if reset_dims:
        _width = 0
        _height = 0
        _layout_ready = false
    if _hex_grid_instance != null:
        _hex_grid_instance.mesh = null

func _invalidate_layout_metrics() -> void:
    _layout_ready = false

func _reset_camera_state() -> void:
    _orbit_azimuth_radians = 0.0
    _pan_offset_world = Vector2.ZERO
    _tilt_degrees = _tilt_degrees_default
    _strategic_request_emitted = false

func _rebuild_chunks() -> void:
    _clear_mesh(false)
    if _width <= 0 or _height <= 0 or _height_samples.is_empty():
        return
    _logged_chunk_count = 0
    _ensure_reference_plane()
    var chunk_w: int = max(chunk_size.x, 1)
    var chunk_h: int = max(chunk_size.y, 1)
    var chunks_x := int(ceil(float(_width) / float(chunk_w)))
    var chunks_y := int(ceil(float(_height) / float(chunk_h)))
    for cy in range(chunks_y):
        for cx in range(chunks_x):
            var start_x: int = cx * chunk_w
            var start_y: int = cy * chunk_h
            var local_w: int = min(chunk_w, _width - start_x)
            var local_h: int = min(chunk_h, _height - start_y)
            if local_w <= 0 or local_h <= 0:
                continue
            var mesh := _build_chunk_mesh(local_w, local_h, start_x, start_y)
            var instance := MeshInstance3D.new()
            instance.mesh = mesh
            instance.material_override = _material
            instance.transform = Transform3D(Basis(), Vector3(start_x * tile_scale, 0.0, start_y * tile_scale))
            instance.create_trimesh_collision()
            _mesh_root.add_child(instance)
            print("[HeightfieldLayer3D] Added chunk mesh at ", instance.transform.origin, " with ", mesh.get_surface_count(), " surfaces")
    if _material != null:
        _material.set_shader_parameter("overlay_mix", overlay_strength)

func _ensure_reference_plane() -> void:
    var existing := _mesh_root.get_node_or_null("ReferencePlane")
    if existing != null:
        return
    var plane_mesh := PlaneMesh.new()
    plane_mesh.size = Vector2(_width * tile_scale, _height * tile_scale)
    var plane_instance := MeshInstance3D.new()
    plane_instance.name = "ReferencePlane"
    plane_instance.mesh = plane_mesh
    plane_instance.material_override = StandardMaterial3D.new()
    plane_instance.material_override.albedo_color = Color(0.2, 0.2, 0.2, 1.0)
    plane_instance.position = Vector3((_width * tile_scale) * 0.5, -1.0, (_height * tile_scale) * 0.5)
    _mesh_root.add_child(plane_instance)
func _build_chunk_mesh(local_w: int, local_h: int, start_x: int, start_y: int) -> ArrayMesh:
    if debug_print_chunks and _logged_chunk_count < debug_max_logged_chunks:
        _debug_log_chunk(start_x, start_y, local_w, local_h)
        _logged_chunk_count += 1
    var st := SurfaceTool.new()
    st.begin(Mesh.PRIMITIVE_TRIANGLES)
    var global_w: float = max(float(_width - 1), 1.0)
    var global_h: float = max(float(_height - 1), 1.0)
    for ly in range(local_h):
        for lx in range(local_w):
            var fx: float = float(lx)
            var fy: float = float(ly)
            var height00 := _height_at(start_x + lx, start_y + ly)
            var height10 := _height_at(start_x + lx + 1, start_y + ly)
            var height11 := _height_at(start_x + lx + 1, start_y + ly + 1)
            var height01 := _height_at(start_x + lx, start_y + ly + 1)
            var v0 := Vector3(fx * tile_scale, height00, fy * tile_scale)
            var v1 := Vector3((fx + 1.0) * tile_scale, height10, fy * tile_scale)
            var v2 := Vector3((fx + 1.0) * tile_scale, height11, (fy + 1.0) * tile_scale)
            var v3 := Vector3(fx * tile_scale, height01, (fy + 1.0) * tile_scale)
            var uv0: Vector2 = Vector2(float(start_x + lx) / float(_width), float(start_y + ly) / float(_height))
            var uv1: Vector2 = Vector2(float(start_x + lx + 1) / float(_width), float(start_y + ly) / float(_height))
            var uv2: Vector2 = Vector2(float(start_x + lx + 1) / float(_width), float(start_y + ly + 1) / float(_height))
            var uv3: Vector2 = Vector2(float(start_x + lx) / float(_width), float(start_y + ly + 1) / float(_height))
            var h_norm0: float = height00 / max(_current_height_exaggeration, 0.0001)
            var h_norm1: float = height10 / max(_current_height_exaggeration, 0.0001)
            var h_norm2: float = height11 / max(_current_height_exaggeration, 0.0001)
            var h_norm3: float = height01 / max(_current_height_exaggeration, 0.0001)
            st.set_uv(uv0)
            st.set_color(_vertex_color_for_debug(start_x + lx, start_y + ly, h_norm0))
            st.add_vertex(v0)
            st.set_uv(uv1)
            st.set_color(_vertex_color_for_debug(start_x + lx + 1, start_y + ly, h_norm1))
            st.add_vertex(v1)
            st.set_uv(uv2)
            st.set_color(_vertex_color_for_debug(start_x + lx + 1, start_y + ly + 1, h_norm2))
            st.add_vertex(v2)
            st.set_uv(uv0)
            st.set_color(_vertex_color_for_debug(start_x + lx, start_y + ly, h_norm0))
            st.add_vertex(v0)
            st.set_uv(uv2)
            st.set_color(_vertex_color_for_debug(start_x + lx + 1, start_y + ly + 1, h_norm2))
            st.add_vertex(v2)
            st.set_uv(uv3)
            st.set_color(_vertex_color_for_debug(start_x + lx, start_y + ly + 1, h_norm3))
            st.add_vertex(v3)
    st.generate_normals()
    var mesh := st.commit()
    if debug_dump_vertices and _logged_chunk_count <= debug_max_logged_chunks:
        _debug_dump_vertices(mesh, start_x, start_y)
    return mesh

func _height_at(x: int, y: int) -> float:
    if _height_samples.is_empty():
        return 0.0
    if _width <= 0 or _height <= 0:
        return 0.0
    var clamped_x: int = clamp(x, 0, _width - 1)
    var clamped_y: int = clamp(y, 0, _height - 1)
    var idx := clamped_y * _width + clamped_x
    if idx < 0 or idx >= _height_samples.size():
        return 0.0
    return clampf(float(_height_samples[idx]), 0.0, 1.0) * _current_height_exaggeration

func _log_height_stats(samples: PackedFloat32Array, width: int, height: int) -> void:
    var total_expected := width * height
    if samples.size() < total_expected:
        push_warning("Heightfield samples smaller than expected: %d < %d" % [samples.size(), total_expected])
    var min_v: float = 1.0
    var max_v: float = 0.0
    var sum_v: float = 0.0
    var count: int = min(samples.size(), total_expected)
    if count == 0:
        push_warning("Heightfield samples empty for %dx%d grid" % [width, height])
        return
    for i in range(count):
        var v := clampf(float(samples[i]), 0.0, 1.0)
        min_v = min(min_v, v)
        max_v = max(max_v, v)
        sum_v += v
    var avg_v: float = sum_v / max(count, 1)
    var signature := "%d:%d:%0.3f:%0.3f:%0.3f" % [width, height, min_v, max_v, avg_v]
    if signature == _last_stats_signature:
        return
    _last_stats_signature = signature
    _max_height_world = max_v * _current_height_exaggeration
    print("[Heightfield] size=%dx%d samples=%d min=%.3f max=%.3f avg=%.3f scale=%.1f" % [
        width, height, samples.size(), min_v, max_v, avg_v, _max_height_world
    ])

func _vertex_color_for_debug(sample_x: int, sample_y: int, h_norm: float) -> Color:
    if debug_visualize_axes:
        var nx: float = float(sample_x) / max(float(_width), 1.0)
        var ny: float = float(sample_y) / max(float(_height), 1.0)
        return Color(nx, ny, h_norm, 1.0)
    if debug_visualize_height:
        return Color(h_norm, h_norm, h_norm, 1.0)
    return Color(1.0, 1.0, 1.0, 1.0)

func _debug_log_chunk(start_x: int, start_y: int, local_w: int, local_h: int) -> void:
    var samples: Array[String] = []
    var max_cols: int = min(local_w, 8)
    for dx in range(max_cols):
        var h := _height_at(start_x + dx, start_y)
        samples.append("%.2f" % (h / max(_current_height_exaggeration, 0.0001)))
    var min_h: float = 1e9
    var max_h: float = -1e9
    var sum_h: float = 0.0
    for dy in range(local_h):
        for dx in range(local_w):
            var h_local := _height_at(start_x + dx, start_y + dy)
            min_h = min(min_h, h_local)
            max_h = max(max_h, h_local)
            sum_h += h_local
    var avg_h: float = sum_h / max(local_w * local_h, 1)
    var world_min_x := start_x * tile_scale
    var world_max_x := (start_x + local_w) * tile_scale
    var world_min_z := start_y * tile_scale
    var world_max_z := (start_y + local_h) * tile_scale
    print("[HeightfieldChunk] origin=(%d,%d) size=%dx%d worldX=[%.1f, %.1f] worldZ=[%.1f, %.1f] minY=%.2f maxY=%.2f avgY=%.2f row0=%s" % [
        start_x, start_y, local_w, local_h, world_min_x, world_max_x, world_min_z, world_max_z,
        min_h, max_h, avg_h, ", ".join(samples)
    ])

func _update_shader_debug_flags() -> void:
    if _material == null:
        return
    var enable_debug := debug_visualize_height or debug_visualize_axes
    _material.set_shader_parameter("debug_mode", enable_debug)

func _apply_height_config(width: int, height: int) -> void:
    if _height_config.is_empty():
        _load_height_config()
    var applied := height_exaggeration_default
    if _height_config.has("default_height_exaggeration"):
        applied = float(_height_config["default_height_exaggeration"])
    var key := "%dx%d" % [width, height]
    if _height_config.has("per_map_dimensions"):
        var per_map: Dictionary = _height_config["per_map_dimensions"]
        if per_map.has(key):
            applied = float(per_map[key])
    applied = max(applied, 1.0)
    if !is_equal_approx(applied, _current_height_exaggeration):
        _current_height_exaggeration = applied

    var camera_ratio := camera_distance_ratio_default
    if _height_config.has("camera"):
        var camera_block: Dictionary = _height_config["camera"]
        if camera_block.has("default_distance_ratio"):
            camera_ratio = float(camera_block["default_distance_ratio"])
        if camera_block.has("per_map_distance_ratio"):
            var per_map_cam: Dictionary = camera_block["per_map_distance_ratio"]
            if per_map_cam.has(key):
                camera_ratio = float(per_map_cam[key])
        if camera_block.has("default_tilt"):
            _tilt_degrees_default = clamp(float(camera_block["default_tilt"]), 20.0, 80.0)
            _tilt_degrees = _tilt_degrees_default
        var zoom_block_variant: Variant = camera_block.get("zoom", {})
        if zoom_block_variant is Dictionary:
            _apply_zoom_block(zoom_block_variant, key)
        else:
            _apply_zoom_block({}, key)
    else:
        _apply_zoom_block({}, key)
    camera_ratio = clamp(camera_ratio, 0.2, 2.0)
    _current_camera_distance_ratio = camera_ratio

    if _height_config.has("visualization"):
        var viz: Dictionary = _height_config["visualization"]
        if viz.has("debug_visualize_height"):
            debug_visualize_height = bool(viz["debug_visualize_height"])
        if viz.has("debug_visualize_axes"):
            debug_visualize_axes = bool(viz["debug_visualize_axes"])
        if viz.has("show_hex_grid"):
            show_hex_grid = bool(viz["show_hex_grid"])
        if viz.has("hex_color"):
            var color_arr: Array = viz["hex_color"]
            if color_arr.size() >= 3:
                var r := float(color_arr[0])
                var g := float(color_arr[1])
                var b := float(color_arr[2])
                var a := float(color_arr[3]) if color_arr.size() >= 4 else 0.65
                _hex_grid_color = Color(r, g, b, a)
        if viz.has("hex_width_scale"):
            _hex_width_scale = clamp(float(viz["hex_width_scale"]), 0.001, 0.5)
        if viz.has("hex_min_width_world"):
            _hex_min_width = max(float(viz["hex_min_width_world"]), 0.0)
        _update_shader_debug_flags()
    _update_hex_overlay()
    _strategic_request_emitted = false

func set_user_zoom_multiplier(value: float) -> void:
    var clamped: float = clamp(value, _min_zoom_multiplier, _max_zoom_multiplier)
    if is_equal_approx(clamped, _user_zoom_multiplier):
        return
    _user_zoom_multiplier = clamped
    if _last_camera != null:
        fit_camera(_last_camera)
    emit_signal("zoom_multiplier_changed", _user_zoom_multiplier)
    if _strategic_zoom_threshold < INF and _user_zoom_multiplier >= _strategic_zoom_threshold:
        if not _strategic_request_emitted:
            _strategic_request_emitted = true
            emit_signal("strategic_view_requested")
    else:
        _strategic_request_emitted = false

func get_user_zoom_multiplier() -> float:
    return _user_zoom_multiplier

func get_zoom_bounds() -> Vector2:
    return Vector2(_min_zoom_multiplier, _max_zoom_multiplier)

func get_zoom_threshold() -> float:
    return _strategic_zoom_threshold

func get_tile_scale_value() -> float:
    return tile_scale

func adjust_orbit(delta_degrees: float) -> void:
    if is_zero_approx(delta_degrees):
        return
    _orbit_azimuth_radians = wrapf(_orbit_azimuth_radians + deg_to_rad(delta_degrees), -TAU, TAU)
    _refit_camera()

func adjust_tilt(delta_degrees: float) -> void:
    if is_zero_approx(delta_degrees):
        return
    _tilt_degrees = clamp(_tilt_degrees + delta_degrees, 20.0, 90.0)
    _refit_camera()

func adjust_pan(delta_world: Vector2) -> void:
    if delta_world == Vector2.ZERO:
        return
    var dims := _map_dimensions_world()
    var limit_x := dims.x * 0.5
    var limit_z := dims.y * 0.5
    _pan_offset_world.x = clamp(_pan_offset_world.x + delta_world.x, -limit_x, limit_x)
    _pan_offset_world.y = clamp(_pan_offset_world.y + delta_world.y, -limit_z, limit_z)
    _refit_camera()

func reset_camera_controls() -> void:
    _reset_camera_state()
    _user_zoom_multiplier = clamp(_default_zoom_multiplier, _min_zoom_multiplier, _max_zoom_multiplier)
    _refit_camera()
    emit_signal("zoom_multiplier_changed", _user_zoom_multiplier)

func _refit_camera() -> void:
    if _last_camera != null:
        fit_camera(_last_camera)

func _load_height_config() -> void:
    if not FileAccess.file_exists(HEIGHTFIELD_CONFIG_PATH):
        _height_config = {}
        return
    var file := FileAccess.open(HEIGHTFIELD_CONFIG_PATH, FileAccess.READ)
    if file == null:
        push_warning("Failed to open heightfield config at %s" % HEIGHTFIELD_CONFIG_PATH)
        _height_config = {}
        return
    var text := file.get_as_text()
    var parsed: Variant = JSON.parse_string(text)
    if typeof(parsed) == TYPE_DICTIONARY:
        _height_config = parsed
    else:
        push_warning("Invalid heightfield config JSON, ignoring.")
        _height_config = {}

func _apply_zoom_block(zoom_block: Dictionary, map_key: String) -> void:
    var min_multiplier: float = float(zoom_block.get("min_multiplier", 0.2))
    var max_multiplier: float = float(zoom_block.get("max_multiplier", 2.2))
    min_multiplier = max(min_multiplier, 0.02)
    max_multiplier = max(max_multiplier, min_multiplier + 0.01)
    var default_multiplier: float = float(zoom_block.get("default_multiplier", 0.8))
    if zoom_block.has("per_map_default_multiplier"):
        var per_map_defaults: Dictionary = zoom_block["per_map_default_multiplier"]
        if per_map_defaults.has(map_key):
            default_multiplier = float(per_map_defaults[map_key])
    var threshold_value: float = float(zoom_block.get("strategic_threshold", INF))
    if threshold_value <= 0.0 or threshold_value < min_multiplier or threshold_value > max_multiplier + 0.001:
        threshold_value = INF
    _min_zoom_multiplier = min_multiplier
    _max_zoom_multiplier = max_multiplier
    _default_zoom_multiplier = clamp(default_multiplier, _min_zoom_multiplier, _max_zoom_multiplier)
    _strategic_zoom_threshold = threshold_value
    _user_zoom_multiplier = _default_zoom_multiplier
func _debug_dump_vertices(mesh: ArrayMesh, start_x: int, start_y: int) -> void:
    if mesh == null:
        return
    var arrays := mesh.surface_get_arrays(0)
    if arrays.is_empty():
        return
    var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
    if vertices.is_empty():
        return
    var max_print: int = min(debug_max_vertices, vertices.size())
    var segments: Array[String] = []
    for i in range(max_print):
        var v: Vector3 = vertices[i]
        segments.append("(%.2f, %.2f, %.2f)" % [v.x, v.y, v.z])
    print("[HeightfieldVertices] chunk=(%d,%d) count=%d samples=%s" % [
        start_x, start_y, max_print, "; ".join(segments)
    ])

func _map_dimensions_world() -> Vector2:
    var map_width := float(max(_width - 1, 1)) * tile_scale
    var map_depth := float(max(_height - 1, 1)) * tile_scale
    return Vector2(map_width, map_depth)

func get_map_dimensions_world() -> Vector2:
    return _map_dimensions_world()

func get_hex_layout_scale() -> Vector2:
    return Vector2(_layout_scale_x, _layout_scale_z)

func _update_hex_overlay() -> void:
    if _hex_grid_instance == null:
        return
    if not show_hex_grid or _width <= 0 or _height <= 0 or _height_samples.is_empty():
        _hex_grid_instance.visible = false
        _hex_grid_instance.mesh = null
        return
    _hex_grid_instance.visible = true
    _ensure_layout_metrics()
    if not _layout_ready:
        _hex_grid_instance.visible = false
        _hex_grid_instance.mesh = null
        return
    var vertices: PackedVector3Array = PackedVector3Array()
    var colors: PackedColorArray = PackedColorArray()
    var indices: PackedInt32Array = PackedInt32Array()
    var grid_color: Color = _hex_grid_color
    var base_scale: float = min(_layout_scale_x, _layout_scale_z)
    var line_width: float = max(base_scale * _hex_width_scale, _hex_min_width)
    var surface_offset: float = max(line_width * 0.35, 0.1)
    var vert_index: int = 0
    var layout_corner_offsets: Array[Vector2] = []
    for i in range(6):
        var angle := deg_to_rad(60.0 * float(i) + 30.0)
        var offset := Vector2(cos(angle), sin(angle)) * HEX_LAYOUT_RADIUS
        layout_corner_offsets.append(offset)
    var world_corner_offsets: Array[Vector2] = []
    for offset in layout_corner_offsets:
        var world_offset := _layout_offset_to_world(offset)
        world_corner_offsets.append(Vector2(world_offset.x, world_offset.z))
    
    # Clear and rebuild hex centers
    _hex_centers.clear()
    
    for row in range(_height):
        for col in range(_width):
            var axial := _offset_to_axial(col, row)
            var axial_center := _axial_to_world(axial.x, axial.y)
            
            # Store the center for this hex WITH the terrain height
            var center_y := _height_at_world(axial_center.x, axial_center.z)
            var center_key := "%d,%d" % [col, row]
            _hex_centers[center_key] = Vector3(axial_center.x, center_y, axial_center.z)
            
            var corners: Array[Vector3] = []
            for offset in world_corner_offsets:
                var corner_x := axial_center.x + offset.x
                var corner_z := axial_center.z + offset.y
                var corner_y := _height_at_world(corner_x, corner_z) + surface_offset
                corners.append(Vector3(corner_x, corner_y, corner_z))
            
            # Draw hex edges
            for i in range(6):
                var p0: Vector3 = corners[i]
                var p1: Vector3 = corners[(i + 1) % 6]
                var dir: Vector3 = (p1 - p0).normalized()
                var perp: Vector3 = Vector3(-dir.z, 0.0, dir.x).normalized() * (line_width * 0.5)
                var v0: Vector3 = p0 - perp
                var v1: Vector3 = p0 + perp
                var v2: Vector3 = p1 + perp
                var v3: Vector3 = p1 - perp
                vertices.append_array([v0, v1, v2, v3])
                colors.append_array([grid_color, grid_color, grid_color, grid_color])
                indices.append_array([vert_index, vert_index + 1, vert_index + 2, vert_index, vert_index + 2, vert_index + 3])
                vert_index += 4
            
    if vertices.is_empty():
        _hex_grid_instance.mesh = null
        return
    var arrays: Array = []
    arrays.resize(Mesh.ARRAY_MAX)
    arrays[Mesh.ARRAY_VERTEX] = vertices
    arrays[Mesh.ARRAY_COLOR] = colors
    arrays[Mesh.ARRAY_INDEX] = indices
    var mesh := ArrayMesh.new()
    mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
    _hex_grid_instance.mesh = mesh
    var aabb := mesh.get_aabb()
    print("[HexGrid] vertices=", vertices.size(), " tris=", indices.size() / 3, " aabb=", aabb)

func _ensure_layout_metrics() -> void:
    if _layout_ready:
        return
    if _width <= 0 or _height <= 0:
        return
    var min_x := INF
    var max_x := -INF
    var min_z := INF
    var max_z := -INF
    for row in range(_height):
        for col in range(_width):
            var axial := _offset_to_axial(col, row)
            var center := _axial_center(axial.x, axial.y, HEX_LAYOUT_RADIUS)
            min_x = min(min_x, center.x)
            max_x = max(max_x, center.x)
            min_z = min(min_z, center.y)
            max_z = max(max_z, center.y)
            for i in range(6):
                var angle := deg_to_rad(60.0 * float(i) + 30.0)
                var offset := Vector2(cos(angle), sin(angle)) * HEX_LAYOUT_RADIUS
                var corner := center + offset
                min_x = min(min_x, corner.x)
                max_x = max(max_x, corner.x)
                min_z = min(min_z, corner.y)
                max_z = max(max_z, corner.y)
    if min_x == INF or min_z == INF:
        return
    var layout_width: float = max(max_x - min_x, 0.0001)
    var layout_depth: float = max(max_z - min_z, 0.0001)
    var target_width: float = max(float(_width), 1.0) * tile_scale
    var target_depth: float = max(float(_height), 1.0) * tile_scale
    _layout_scale_x = target_width / layout_width
    _layout_scale_z = target_depth / layout_depth
    _layout_offset = Vector2(min_x, min_z)
    _layout_ready = true

func _layout_to_world(layout: Vector2) -> Vector3:
    return Vector3(
        (layout.x - _layout_offset.x) * _layout_scale_x,
        0.0,
        (layout.y - _layout_offset.y) * _layout_scale_z
    )

func _layout_offset_to_world(offset: Vector2) -> Vector3:
    return Vector3(offset.x * _layout_scale_x, 0.0, offset.y * _layout_scale_z)

func _axial_to_world(q: int, r: int) -> Vector3:
    var layout_center := _axial_center(q, r, HEX_LAYOUT_RADIUS)
    return _layout_to_world(layout_center)

func _axial_center(q: int, r: int, radius: float) -> Vector2:
    var fq := float(q)
    var fr := float(r)
    var x := radius * (SQRT3 * fq + SQRT3 * 0.5 * fr)
    var z := radius * (1.5 * fr)
    return Vector2(x, z)

func _offset_to_axial(col: int, row: int) -> Vector2i:
    var q := col - ((row - (row & 1)) >> 1)
    return Vector2i(q, row)

func _height_at_world(world_x: float, world_z: float) -> float:
    if _width <= 0 or _height <= 0 or tile_scale <= 0.0:
        return 0.0
    var grid_x: float = clampf(world_x / tile_scale, 0.0, float(max(_width - 1, 0)))
    var grid_z: float = clampf(world_z / tile_scale, 0.0, float(max(_height - 1, 0)))
    var x0: int = int(floor(grid_x))
    var z0: int = int(floor(grid_z))
    var x1: int = min(x0 + 1, _width - 1)
    var z1: int = min(z0 + 1, _height - 1)
    var tx: float = grid_x - float(x0)
    var tz: float = grid_z - float(z0)
    var h00: float = _height_at(x0, z0)
    var h10: float = _height_at(x1, z0)
    var h01: float = _height_at(x0, z1)
    var h11: float = _height_at(x1, z1)
    var hx0: float = lerp(h00, h10, tx)
    var hx1: float = lerp(h01, h11, tx)
    return lerp(hx0, hx1, tz)

func _build_color_texture(colors: PackedColorArray, width: int, height: int) -> Texture2D:
    var tex_width: int = max(width, 1)
    var tex_height: int = max(height, 1)
    if colors.is_empty():
        return null
    var total_expected: int = tex_width * tex_height
    var count: int = min(colors.size(), total_expected)
    var image: Image = Image.create(tex_width, tex_height, false, Image.FORMAT_RGBA8)
    if image == null:
        return null
    for idx in range(count):
        var color: Color = colors[idx]
        var x: int = idx % tex_width
        var y: int = idx / tex_width
        image.set_pixel(x, y, color)
    if count < total_expected:
        var last_color: Color = colors[count - 1] if count > 0 else Color.BLACK
        for idx in range(count, total_expected):
            var x2: int = idx % tex_width
            var y2: int = idx / tex_width
            image.set_pixel(x2, y2, last_color)
    return ImageTexture.create_from_image(image)

func _build_overlay_texture(values: PackedFloat32Array, width: int, height: int) -> Texture2D:
    var tex_width: int = max(width, 1)
    var tex_height: int = max(height, 1)
    if values.is_empty():
        return null
    var total_expected: int = tex_width * tex_height
    var count: int = min(values.size(), total_expected)
    var image: Image = Image.create(tex_width, tex_height, false, Image.FORMAT_R8)
    if image == null:
        return null
    for idx in range(count):
        var value: float = clampf(values[idx], 0.0, 1.0)
        var x: int = idx % tex_width
        var y: int = idx / tex_width
        image.set_pixel(x, y, Color(value, value, value, 1.0))
    if count < total_expected:
        var filler: float = clampf(values[count - 1] if count > 0 else 0.0, 0.0, 1.0)
        var filler_color := Color(filler, filler, filler, 1.0)
        for idx in range(count, total_expected):
            var x2: int = idx % tex_width
            var y2: int = idx / tex_width
            image.set_pixel(x2, y2, filler_color)
    return ImageTexture.create_from_image(image)

func get_hex_center(col: int, row: int) -> Vector3:
    var key := "%d,%d" % [col, row]
    if _hex_centers.has(key):
        return _hex_centers[key]
    # Fallback: calculate on the fly if not found
    var axial := _offset_to_axial(col, row)
    return _axial_to_world(axial.x, axial.y)

func get_hex_corners(col: int, row: int) -> PackedVector3Array:
    var corners := PackedVector3Array()
    var axial := _offset_to_axial(col, row)
    var center := _axial_to_world(axial.x, axial.y)
    
    var layout_corners: Array[Vector2] = []
    for i in range(6):
        var angle := deg_to_rad(60.0 * float(i) + 30.0)
        var offset := Vector2(cos(angle), sin(angle)) * HEX_LAYOUT_RADIUS
        layout_corners.append(offset)
        
    var base_scale: float = min(_layout_scale_x, _layout_scale_z)
    var line_width: float = max(base_scale * _hex_width_scale, _hex_min_width)
    var surface_offset: float = max(line_width * 0.35, 0.1)
    
    for offset in layout_corners:
        var world_offset := _layout_offset_to_world(offset)
        var corner_x := center.x + world_offset.x
        var corner_z := center.z + world_offset.z
        var corner_y := _height_at_world(corner_x, corner_z) + surface_offset
        corners.append(Vector3(corner_x, corner_y, corner_z))
        
    return corners

func world_to_hex(world_pos: Vector3) -> Vector2i:
    # Inverse of _layout_to_world
    var layout_x: float = 0.0
    var layout_z: float = 0.0
    
    if _layout_scale_x != 0.0:
        layout_x = world_pos.x / _layout_scale_x + _layout_offset.x
    if _layout_scale_z != 0.0:
        layout_z = world_pos.z / _layout_scale_z + _layout_offset.y
        
    var R := HEX_LAYOUT_RADIUS
    # Inverse of _axial_center (Pointy Top)
    # z = R * 1.5 * r  ->  r = z / (1.5 * R)
    # x = R * sqrt(3) * (q + r/2)  ->  q = x / (R * sqrt(3)) - r/2
    
    var r_float: float = layout_z / (1.5 * R)
    var q_float: float = layout_x / (SQRT3 * R) - r_float / 2.0
    var s_float: float = -q_float - r_float
    
    var q := roundi(q_float)
    var r := roundi(r_float)
    var s := roundi(s_float)
    
    var q_diff := absf(q - q_float)
    var r_diff := absf(r - r_float)
    var s_diff := absf(s - s_float)
    
    if q_diff > r_diff and q_diff > s_diff:
        q = -r - s
    elif r_diff > s_diff:
        r = -q - s
    else:
        s = -q - r
        
    # Convert axial (q, r) to offset (col, row)
    # Inverse of: q = col - ((row - (row & 1)) >> 1)
    var col := q + ((r - (r & 1)) >> 1)
    var row := r
    return Vector2i(col, row)
