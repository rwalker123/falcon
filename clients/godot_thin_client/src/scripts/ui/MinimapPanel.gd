extends Node
class_name MinimapPanel
## Shared minimap panel component for both 2D and 3D views.
## Handles UI setup, aspect ratio sizing, and click-to-pan interaction.
## Configuration loaded from heightfield_config.json, with fallback defaults.

signal pan_requested(normalized_pos: Vector2)
signal drag_started()
signal drag_ended()

const CONFIG_PATH := "res://src/data/heightfield_config.json"

# Fallback defaults (overridden by config if available)
const DEFAULT_BASE_HEIGHT := 220
const DEFAULT_MIN_WIDTH := 140.0
const DEFAULT_MAX_WIDTH := 520.0
const DEFAULT_MARGIN := 16.0

# Loaded config values
var _base_height: int = DEFAULT_BASE_HEIGHT
var _min_width: float = DEFAULT_MIN_WIDTH
var _max_width: float = DEFAULT_MAX_WIDTH

var canvas_layer: CanvasLayer
var anchor: Control
var panel: PanelContainer
var texture_rect: TextureRect
var viewport_indicator: Control

var _drag_active: bool = false
var _margin: float = DEFAULT_MARGIN
var _grid_width: int = 0
var _grid_height: int = 0

## Load minimap configuration from JSON file.
func _load_config() -> void:
	if not FileAccess.file_exists(CONFIG_PATH):
		return
	var file := FileAccess.open(CONFIG_PATH, FileAccess.READ)
	if file == null:
		return
	var json = JSON.parse_string(file.get_as_text())
	if json is Dictionary and json.has("minimap"):
		var cfg: Dictionary = json["minimap"]
		_base_height = int(cfg.get("base_height", DEFAULT_BASE_HEIGHT))
		_min_width = float(cfg.get("min_width", DEFAULT_MIN_WIDTH))
		_max_width = float(cfg.get("max_width", DEFAULT_MAX_WIDTH))
		_margin = float(cfg.get("margin", DEFAULT_MARGIN))

## Initialize the minimap panel UI hierarchy.
## parent: Node to add the CanvasLayer to
## layer_index: CanvasLayer layer number (default 102)
## margin: Distance from screen edge (uses config value if not specified)
## style: Optional StyleBox for the panel (null = default semi-transparent)
func setup(parent: Node, layer_index: int = 102, margin: float = -1.0, style: StyleBox = null) -> void:
	_load_config()
	if margin >= 0.0:
		_margin = margin

	# Create CanvasLayer
	canvas_layer = CanvasLayer.new()
	canvas_layer.layer = layer_index
	canvas_layer.name = "MinimapLayer"
	parent.add_child(canvas_layer)

	# Create anchor control for bottom-right positioning
	anchor = Control.new()
	anchor.anchor_left = 1.0
	anchor.anchor_right = 1.0
	anchor.anchor_top = 1.0
	anchor.anchor_bottom = 1.0
	anchor.offset_left = -300.0
	anchor.offset_right = -margin
	anchor.offset_top = -300.0
	anchor.offset_bottom = -margin
	canvas_layer.add_child(anchor)

	# Create panel container
	panel = PanelContainer.new()
	panel.name = "MinimapPanel"
	panel.mouse_filter = Control.MOUSE_FILTER_STOP
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)

	# Apply style
	if style != null:
		panel.add_theme_stylebox_override("panel", style)
	else:
		var default_style := StyleBoxFlat.new()
		default_style.bg_color = Color(0.1, 0.1, 0.15, 0.85)
		default_style.border_color = Color(0.3, 0.35, 0.4, 1.0)
		default_style.set_border_width_all(2)
		default_style.set_corner_radius_all(4)
		default_style.content_margin_left = 4
		default_style.content_margin_top = 4
		default_style.content_margin_right = 4
		default_style.content_margin_bottom = 4
		panel.add_theme_stylebox_override("panel", default_style)
	anchor.add_child(panel)

	# Create texture rect for minimap content
	texture_rect = TextureRect.new()
	texture_rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	texture_rect.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	texture_rect.size_flags_vertical = Control.SIZE_EXPAND_FILL
	texture_rect.mouse_filter = Control.MOUSE_FILTER_STOP
	texture_rect.gui_input.connect(_on_gui_input)
	panel.add_child(texture_rect)

	# Create viewport indicator overlay
	viewport_indicator = Control.new()
	viewport_indicator.name = "ViewportIndicator"
	viewport_indicator.mouse_filter = Control.MOUSE_FILTER_IGNORE
	viewport_indicator.set_anchors_preset(Control.PRESET_FULL_RECT)
	texture_rect.add_child(viewport_indicator)

## Set the texture to display in the minimap.
func set_texture(tex: Texture2D) -> void:
	if texture_rect != null:
		texture_rect.texture = tex

## Update the grid dimensions for aspect ratio calculation.
func set_grid_size(width: int, height: int) -> void:
	_grid_width = width
	_grid_height = height
	resize_to_aspect()

## Show or hide the minimap.
func set_visible(visible: bool) -> void:
	if canvas_layer != null:
		canvas_layer.visible = visible

## Check if the minimap is visible.
func is_visible() -> bool:
	return canvas_layer != null and canvas_layer.visible

## Calculate the map aspect ratio.
func get_aspect_ratio() -> float:
	if _grid_width > 0 and _grid_height > 0:
		return float(_grid_width) / float(_grid_height)
	return 1.0

## Resize the panel to match the map aspect ratio.
func resize_to_aspect() -> void:
	if panel == null or _grid_width == 0 or _grid_height == 0:
		return

	var map_aspect := get_aspect_ratio()
	var target_height := float(_base_height)
	var target_width := clampf(target_height * map_aspect, _min_width, _max_width)

	panel.custom_minimum_size = Vector2(target_width, target_height)

	# Update anchor offsets based on new size
	if anchor != null:
		anchor.offset_left = -(target_width + _margin)
		anchor.offset_top = -(target_height + _margin)

## Get the display rect of the texture within the TextureRect.
## Accounts for STRETCH_KEEP_ASPECT_CENTERED mode.
func get_texture_display_rect() -> Rect2:
	if texture_rect == null or texture_rect.texture == null:
		return Rect2()

	var tex_size := texture_rect.texture.get_size()
	var container_size := texture_rect.size

	if tex_size.x <= 0 or tex_size.y <= 0:
		return Rect2()
	if container_size.x <= 0 or container_size.y <= 0:
		return Rect2()

	var tex_aspect := tex_size.x / tex_size.y
	var container_aspect := container_size.x / container_size.y

	var display_size: Vector2
	var display_pos: Vector2

	if tex_aspect > container_aspect:
		# Texture is wider - fit to width
		display_size.x = container_size.x
		display_size.y = container_size.x / tex_aspect
		display_pos.x = 0
		display_pos.y = (container_size.y - display_size.y) * 0.5
	else:
		# Texture is taller - fit to height
		display_size.y = container_size.y
		display_size.x = container_size.y * tex_aspect
		display_pos.y = 0
		display_pos.x = (container_size.x - display_size.x) * 0.5

	return Rect2(display_pos, display_size)

## Convert a local position within the texture to normalized (0-1) coordinates.
func local_to_normalized(local_pos: Vector2) -> Vector2:
	var display_rect := get_texture_display_rect()
	if display_rect.size.x <= 0 or display_rect.size.y <= 0:
		return Vector2(0.5, 0.5)

	var rel_pos := local_pos - display_rect.position
	return Vector2(
		clampf(rel_pos.x / display_rect.size.x, 0.0, 1.0),
		clampf(rel_pos.y / display_rect.size.y, 0.0, 1.0)
	)

## Check if dragging is active.
func is_dragging() -> bool:
	return _drag_active

## Request a redraw of the viewport indicator.
func queue_indicator_redraw() -> void:
	if viewport_indicator != null:
		viewport_indicator.queue_redraw()

## Connect a draw callback for the viewport indicator.
func connect_indicator_draw(callback: Callable) -> void:
	if viewport_indicator != null:
		viewport_indicator.draw.connect(callback)

func _on_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mb: InputEventMouseButton = event
		if mb.button_index == MOUSE_BUTTON_LEFT:
			if mb.pressed:
				_drag_active = true
				drag_started.emit()
				var norm := local_to_normalized(mb.position)
				pan_requested.emit(norm)
			else:
				_drag_active = false
				drag_ended.emit()
	elif event is InputEventMouseMotion and _drag_active:
		var motion: InputEventMouseMotion = event
		var norm := local_to_normalized(motion.position)
		pan_requested.emit(norm)
