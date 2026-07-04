extends Node2D
class_name CachedMapRenderer
## Renders the hex map to a SubViewport for cached display.
## This allows panning without redrawing by simply moving the displayed texture.

signal cache_rendered  # Emitted when the cache has been rendered

const SQRT3 := 1.7320508075688772
const GRID_LINE_COLOR := Color(0.35, 0.45, 0.30, 0.45)

# Reference to the main MapView for accessing map data
var map_view: Node2D = null

# Buffer size (how much larger than viewport to render)
const BUFFER_MARGIN := 0.5  # 50% extra on each side

# Cached rendering state
var _render_origin: Vector2 = Vector2.ZERO  # World origin when cache was rendered
var _render_radius: float = 0.0  # Hex radius when cache was rendered
var _render_size: Vector2 = Vector2.ZERO  # Size of the rendered area
var _render_pan_offset: Vector2 = Vector2.ZERO  # Pan offset when rendered

# Cached hex offsets for this renderer
var _hex_offsets: PackedVector2Array = PackedVector2Array()
var _cached_radius: float = -1.0

func setup(view: Node2D) -> void:
	map_view = view

func _draw() -> void:
	if map_view == null:
		return

	# Get rendering parameters from MapView
	var radius: float = map_view.last_hex_radius
	var grid_width: int = map_view.grid_width
	var grid_height: int = map_view.grid_height

	if grid_width == 0 or grid_height == 0:
		return

	# Store render state
	_render_radius = radius
	_render_pan_offset = map_view.pan_offset

	# Pre-compute hex offsets
	_update_hex_offsets(radius)

	# Calculate the visible area with buffer
	var viewport_size: Vector2 = map_view.get_viewport_rect().size
	var buffer_size := viewport_size * (1.0 + BUFFER_MARGIN * 2.0)
	_render_size = buffer_size

	# Calculate origin for SubViewport rendering
	# The SubViewport's (0,0) will be displayed at screen position (-center_offset)
	# So we need to offset hex positions by +center_offset to place them correctly
	var center_offset := viewport_size * BUFFER_MARGIN
	_render_origin = map_view.last_origin + center_offset

	# Draw background
	draw_rect(Rect2(Vector2.ZERO, buffer_size), Color(0.3, 0.35, 0.25, 1.0))

	# Determine if using textured rendering
	var mgr = map_view.get_node("/root/TerrainTextureManager")
	var use_textures: bool = mgr != null and mgr.use_terrain_textures and mgr.terrain_textures != null and map_view.active_overlay_key == "" and not map_view._fow_enabled

	# Calculate visible range with buffer
	var hex_col_width := SQRT3 * radius
	var hex_row_height := 1.5 * radius

	var col_start: int = int((-_render_origin.x) / hex_col_width) - 2
	var col_end: int = int((buffer_size.x - _render_origin.x) / hex_col_width) + 2
	var row_start: int = maxi(0, int((-_render_origin.y) / hex_row_height) - 2)
	var row_end: int = mini(grid_height, int((buffer_size.y - _render_origin.y) / hex_row_height) + 2)

	# Handle horizontal wrapping
	var wrap_horizontal: bool = map_view._wrap_horizontal
	if not wrap_horizontal:
		col_start = maxi(0, col_start)
		col_end = mini(grid_width, col_end)

	# Draw hexes
	for y in range(row_start, row_end):
		for logical_x in range(col_start, col_end):
			var data_x: int = posmod(logical_x, grid_width) if wrap_horizontal else logical_x
			if not wrap_horizontal and (logical_x < 0 or logical_x >= grid_width):
				continue

			var center: Vector2 = _hex_center(logical_x, y, radius, _render_origin)

			if use_textures:
				var terrain_id: int = map_view._terrain_id_at(data_x, y)
				_draw_hex_textured(center, terrain_id, radius)
			else:
				var final_color: Color = map_view._tile_color(data_x, y)
				var polygon_points := _hex_points(center, radius)
				draw_polygon(polygon_points, PackedColorArray([final_color, final_color, final_color, final_color, final_color, final_color]))

	# Draw grid lines if enabled and radius is large enough
	if map_view._show_grid_lines and radius >= 12.0:
		for y in range(row_start, row_end):
			for logical_x in range(col_start, col_end):
				if not wrap_horizontal and (logical_x < 0 or logical_x >= grid_width):
					continue
				var center: Vector2 = _hex_center(logical_x, y, radius, _render_origin)
				draw_polyline(_hex_points(center, radius, true), GRID_LINE_COLOR, 2.0, true)

	cache_rendered.emit()

func _update_hex_offsets(radius: float) -> void:
	if radius == _cached_radius:
		return
	_hex_offsets.resize(6)
	for i in range(6):
		var angle := deg_to_rad(60.0 * float(i) + 30.0)
		_hex_offsets[i] = Vector2(radius * cos(angle), radius * sin(angle))
	_cached_radius = radius

func _hex_center(col: int, row: int, radius: float, origin: Vector2) -> Vector2:
	var q := col - ((row - (row & 1)) >> 1)
	var r := row
	var fq := float(q)
	var fr := float(r)
	var x: float = radius * (SQRT3 * fq + SQRT3 * 0.5 * fr)
	var y: float = radius * (1.5 * fr)
	return origin + Vector2(x, y)

func _hex_points(center: Vector2, radius: float, closed: bool = false) -> PackedVector2Array:
	var points := PackedVector2Array()
	points.resize(7 if closed else 6)
	for i in range(6):
		points[i] = center + _hex_offsets[i]
	if closed:
		points[6] = points[0]
	return points

func _draw_hex_textured(center: Vector2, terrain_id: int, radius: float) -> void:
	var tex: ImageTexture = map_view._hex_texture_cache.get(terrain_id)
	if tex == null:
		var color: Color = map_view._terrain_color_for_id(terrain_id)
		var polygon_points := _hex_points(center, radius)
		draw_polygon(polygon_points, PackedColorArray([color, color, color, color, color, color]))
		return

	var polygon_points := _hex_points(center, radius)
	var uvs := PackedVector2Array()
	for point in polygon_points:
		var uv := Vector2(
			(point.x - center.x) / radius * 0.5 + 0.5,
			(point.y - center.y) / radius * 0.5 + 0.5
		)
		uvs.append(uv)
	var colors := PackedColorArray([Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE, Color.WHITE])
	draw_polygon(polygon_points, colors, uvs, tex)

func get_render_origin() -> Vector2:
	return _render_origin

func get_render_size() -> Vector2:
	return _render_size

func get_render_radius() -> float:
	return _render_radius

func get_render_pan_offset() -> Vector2:
	return _render_pan_offset
