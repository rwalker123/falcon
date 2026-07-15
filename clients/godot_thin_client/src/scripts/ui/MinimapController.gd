class_name MinimapController
extends RefCounted

## Owns the 2D minimap subsystem for MapView: the MinimapPanel instance, its
## terrain/FoW image (rebuilt only when the grid/data/FoW state changes), and the
## viewport-indicator overlay + click-to-pan handling. Extracted from MapView
## (composition — MapView holds one of these and delegates). Behaviour is
## unchanged; the minimap simply owns its own state now.
##
## MapView still owns `_explored_bounds_world` (a pan-clamp concern the image
## rebuild happens to compute); this controller writes it through the back-ref.

const MinimapPanelScript := preload("res://src/scripts/ui/MinimapPanel.gd")

var _view: MapView = null

var _minimap_2d: Node = null  # MinimapPanel instance
var _minimap_2d_image: Image = null
var _minimap_2d_last_grid_size: Vector2i = Vector2i.ZERO
var _minimap_2d_data_version: int = 0  # Incremented when terrain/visibility changes
var _minimap_2d_last_data_version: int = -1  # Last version used for rebuild
var _minimap_2d_last_fow_enabled: bool = false  # Track FoW state changes

func _init(view: MapView) -> void:
	_view = view

## Terrain/visibility changed — the next update() must regenerate the image.
func bump_data_version() -> void:
	_minimap_2d_data_version += 1

## Ask the minimap to redraw its viewport indicator (no image rebuild).
func queue_indicator_redraw() -> void:
	if _minimap_2d != null and _minimap_2d.has_method("queue_indicator_redraw"):
		_minimap_2d.queue_indicator_redraw()

func _setup() -> void:
	_minimap_2d = MinimapPanelScript.new()
	_view.add_child(_minimap_2d)

	# Prefer embedded mode if HUD reference is available
	var embedded := false
	if _view._hud_layer != null and _view._hud_layer.has_method("get_minimap_container"):
		var container: Control = _view._hud_layer.get_minimap_container()
		if container != null:
			_minimap_2d.setup_embedded(container)
			embedded = true

	if not embedded:
		# Fallback to floating mode (legacy behavior)
		_minimap_2d.setup(_view, MinimapPanelScript.MINIMAP_CANVAS_LAYER)

	_minimap_2d.pan_requested.connect(_on_pan_requested)
	_minimap_2d.connect_indicator_draw(_draw_viewport_indicator)

func update() -> void:
	if _view.grid_width == 0 or _view.grid_height == 0:
		return

	# Lazy initialization: set up minimap on first call (after Main.gd has a chance to set HUD reference)
	if _minimap_2d == null:
		_setup()

	_minimap_2d.set_visible(true)

	# Check if we need to regenerate the minimap image
	var current_size := Vector2i(_view.grid_width, _view.grid_height)
	var size_changed := _minimap_2d_last_grid_size != current_size
	var data_changed := _minimap_2d_last_data_version != _minimap_2d_data_version
	var fow_changed := _minimap_2d_last_fow_enabled != _view._fow_enabled
	var needs_rebuild := _minimap_2d_image == null or size_changed or data_changed or fow_changed

	if needs_rebuild:
		_minimap_2d_last_grid_size = current_size
		_minimap_2d_last_data_version = _minimap_2d_data_version
		_minimap_2d_last_fow_enabled = _view._fow_enabled
		_rebuild_image()

	# Update viewport indicator
	_minimap_2d.queue_indicator_redraw()

func _rebuild_image() -> void:
	if _view.grid_width == 0 or _view.grid_height == 0:
		return

	# Cache terrain colors lookup for faster access
	var colors := _view._get_terrain_colors()
	var fallback_color := Color(0.2, 0.2, 0.2, 1.0)
	var fog_color := _view._fow_fog_fill_color  # Dark color for unexplored
	var mist_color := _view._fow_mist_color  # Light gray-blue mist for explored-but-not-visible

	# Get visibility data for FoW (if enabled)
	var visibility_data: PackedFloat32Array = PackedFloat32Array()
	if _view._fow_enabled:
		visibility_data = _view._visibility_array()

	# The minimap always renders the FULL grid so its shape/aspect ratio stays
	# constant whether FoW is on or off; unexplored tiles are painted as fog in
	# the pixel loop below (standard 4X behaviour — no unseen terrain is revealed).
	# Explored bounds are still computed when FoW is on so that panning stays
	# clamped to discovered space (see _clamp_pan_offset).
	var img_width := _view.grid_width
	var img_height := _view.grid_height

	# Pre-allocate byte array for RGB8 image data (3 bytes per pixel)
	var pixel_count := img_width * img_height
	var data := PackedByteArray()
	data.resize(pixel_count * 3)

	# Track the bounding box of explored tiles while painting (FoW only), so pan
	# clamping can use it without a second full pass over the visibility array.
	# Gate fog on _fow_enabled alone (not on visibility_data being populated): when
	# FoW is on but the visibility channel hasn't streamed yet, the per-tile lookup
	# below falls back to vis == 0.0, so every tile paints as fog rather than leaking
	# the unexplored map as full terrain. Explored bounds simply stay empty.
	var min_col := _view.grid_width
	var max_col := -1
	var min_row := _view.grid_height
	var max_row := -1

	# Fill byte array with terrain colors
	var byte_index := 0
	for grid_row in range(img_height):
		for grid_col in range(img_width):
			var grid_index := grid_row * _view.grid_width + grid_col

			var terrain_id := int(_view.terrain_overlay[grid_index]) if grid_index < _view.terrain_overlay.size() else -1
			var color: Color = colors.get(terrain_id, fallback_color)

			# Apply Fog of War visibility
			if _view._fow_enabled:
				var vis: float = visibility_data[grid_index] if grid_index < visibility_data.size() else 0.0
				if vis <= 0.0:
					# Unexplored - show dark fog
					color = fog_color
				else:
					# Explored (discovered or active) - grow the explored bounds
					min_col = mini(min_col, grid_col)
					max_col = maxi(max_col, grid_col)
					min_row = mini(min_row, grid_row)
					max_row = maxi(max_row, grid_row)
					if vis <= _view.FOW_VISIBLE_THRESHOLD:
						# Explored but not currently visible - show terrain with light mist overlay
						# Desaturate slightly and blend with mist to show "remembered" state
						color = color.lerp(mist_color, _view._fow_mist_blend)
					# else: vis > FOW_VISIBLE_THRESHOLD - fully visible, use terrain color as-is

			# Convert Color (0-1 floats) to RGB bytes (0-255)
			data[byte_index] = int(color.r * 255.0)
			data[byte_index + 1] = int(color.g * 255.0)
			data[byte_index + 2] = int(color.b * 255.0)
			byte_index += 3

	# Update world bounds for pan clamping (at unit radius, scaled in _clamp_pan_offset).
	# Cleared when FoW is off (full map) or nothing is explored yet.
	if _view._fow_enabled and max_col >= 0:
		var explored := Rect2i(min_col, min_row, max_col - min_col + 1, max_row - min_row + 1)
		_view._explored_bounds_world = _view._compute_explored_bounds_world(explored, 1.0)
	else:
		_view._explored_bounds_world = Rect2()

	# Create image from byte array
	_minimap_2d_image = Image.create_from_data(img_width, img_height, false, Image.FORMAT_RGB8, data)

	# Create texture from image and update panel
	var tex := ImageTexture.create_from_image(_minimap_2d_image)
	_minimap_2d.set_texture(tex)
	_minimap_2d.set_grid_size(img_width, img_height)

## Draw the viewport indicator rectangle on the 2D minimap.
##
## This shows which portion of the map is currently visible in the main view.
## The coordinate transformation uses the same axial hex math as _point_to_offset:
##
## Screen-to-Hex Coordinate Conversion (pointy-top hexes):
##   1. Subtract origin and divide by hex radius to get relative position
##   2. Convert to axial coordinates (q, r) using pointy-top hex formulas:
##      q = (sqrt(3)/3 * x - 1/3 * y)
##      r = (2/3 * y)
##   3. Round to nearest hex using cube coordinate rounding
##   4. Convert axial (q, r) to offset (col, row) coordinates
##
## The resulting hex coordinates are then normalized to [0,1] range and
## mapped to pixel positions within the minimap texture display area.
func _draw_viewport_indicator() -> void:
	if _minimap_2d == null or _view.grid_width == 0 or _view.grid_height == 0:
		return
	if _view.last_hex_radius <= 0:
		return

	var viewport_size := _view._get_adjusted_viewport_size()
	if viewport_size.x <= 0 or viewport_size.y <= 0:
		return

	# Use the visible column/row range stored during the last render
	# This ensures the indicator matches exactly what's being drawn
	var tl_col_f: float = _view._last_visible_col_start
	var tl_row_f: float = _view._last_visible_row_start
	var br_col_f: float = _view._last_visible_col_end
	var br_row_f: float = _view._last_visible_row_end

	# Normalize hex coordinates to [0,1] range for minimap positioning.
	# The minimap image spans the full grid (FoW or not), so normalize against it.
	var view_left: float
	var view_right: float
	var view_top: float
	var view_bottom: float

	if _view._wrap_horizontal:
		# When wrapping, don't clamp X - allow values outside [0,1] to indicate wrap
		view_left = tl_col_f / float(_view.grid_width)
		view_right = br_col_f / float(_view.grid_width)
		view_top = clampf(tl_row_f / float(_view.grid_height), 0.0, 1.0)
		view_bottom = clampf(br_row_f / float(_view.grid_height), 0.0, 1.0)
	else:
		# Full grid normalization with clamping
		view_left = clampf(tl_col_f / float(_view.grid_width), 0.0, 1.0)
		view_right = clampf(br_col_f / float(_view.grid_width), 0.0, 1.0)
		view_top = clampf(tl_row_f / float(_view.grid_height), 0.0, 1.0)
		view_bottom = clampf(br_row_f / float(_view.grid_height), 0.0, 1.0)

	# Map normalized coords to pixel positions within minimap texture display area
	var texture_display_rect: Rect2 = _minimap_2d.get_texture_display_rect()
	var indicator_color := Color(1.0, 1.0, 1.0, 0.8)

	# Calculate viewport width in normalized coords
	var viewport_width_norm := view_right - view_left

	# When wrapping is enabled and viewport spans the wrap boundary, may need split rectangles
	# But if viewport shows >= entire map width, draw full-width indicator instead
	if _view._wrap_horizontal and (view_left < 0.0 or view_right > 1.0):
		if viewport_width_norm >= 1.0:
			# Viewport shows entire map width or more - draw full-width indicator
			var rect := Rect2(
				texture_display_rect.position.x,
				texture_display_rect.position.y + view_top * texture_display_rect.size.y,
				texture_display_rect.size.x,
				(view_bottom - view_top) * texture_display_rect.size.y
			)
			_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)
		else:
			# Wrap the normalized coordinates to [0,1] range
			var wrapped_left := fposmod(view_left, 1.0)
			var wrapped_right := fposmod(view_right, 1.0)

			# If viewport spans wrap, wrapped_right < wrapped_left
			if wrapped_right < wrapped_left:
				# Draw left portion (from wrapped_left to right edge)
				var rect_left := Rect2(
					texture_display_rect.position.x + wrapped_left * texture_display_rect.size.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					(1.0 - wrapped_left) * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect_left, indicator_color, false, 2.0)

				# Draw right portion (from left edge to wrapped_right)
				var rect_right := Rect2(
					texture_display_rect.position.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					wrapped_right * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect_right, indicator_color, false, 2.0)
			else:
				# Viewport doesn't span wrap, just draw single rectangle at wrapped position
				var rect := Rect2(
					texture_display_rect.position.x + wrapped_left * texture_display_rect.size.x,
					texture_display_rect.position.y + view_top * texture_display_rect.size.y,
					(wrapped_right - wrapped_left) * texture_display_rect.size.x,
					(view_bottom - view_top) * texture_display_rect.size.y
				)
				_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)
	else:
		# Standard non-wrapping case
		var rect := Rect2(
			texture_display_rect.position.x + view_left * texture_display_rect.size.x,
			texture_display_rect.position.y + view_top * texture_display_rect.size.y,
			(view_right - view_left) * texture_display_rect.size.x,
			(view_bottom - view_top) * texture_display_rect.size.y
		)
		_minimap_2d.viewport_indicator.draw_rect(rect, indicator_color, false, 2.0)

## Handle minimap click/drag to pan the main view.
##
## Converts the normalized minimap position (0-1) to hex grid coordinates,
## then calculates the pan_offset needed to center that hex in the viewport.
##
## normalized_pos: Position within minimap texture, (0,0)=top-left, (1,1)=bottom-right
func _on_pan_requested(normalized_pos: Vector2) -> void:
	if _view.grid_width == 0 or _view.grid_height == 0:
		return
	if _view.last_hex_radius <= 0:
		return

	# Convert normalized [0,1] position to hex grid coordinates (col, row).
	# The minimap image spans the full grid, so denormalize against it directly;
	# _clamp_pan_offset() still confines the resulting pan to explored space.
	# normalized_pos is clamped to [0,1], so x/y == 1.0 must map to the LAST
	# column/row; clamp the source index here so the wrap branch's posmod() below
	# doesn't turn a right-edge click (col == grid_width) into column 0.
	var target_col := mini(int(normalized_pos.x * float(_view.grid_width)), _view.grid_width - 1)
	var target_row := mini(int(normalized_pos.y * float(_view.grid_height)), _view.grid_height - 1)
	_view.focus_on_tile(target_col, target_row)
