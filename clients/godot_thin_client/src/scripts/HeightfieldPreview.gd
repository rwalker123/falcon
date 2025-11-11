extends Window
class_name HeightfieldPreview

signal strategic_view_requested

const HeightfieldLayer3D := preload("res://src/scripts/HeightfieldLayer3D.gd")

var _viewport: SubViewport
var _container: SubViewportContainer
var _root: Node3D
var _camera: Camera3D
var _light: DirectionalLight3D
var _heightfield: HeightfieldLayer3D
var _ui_overlay: Control
var _zoom_slider: HSlider
var _zoom_label: Label
var _orbit_drag_active := false
var _pan_drag_active := false
var _last_mouse_position := Vector2.ZERO
const ORBIT_SENSITIVITY := 0.25
const TILT_SENSITIVITY := 0.18
const PAN_SENSITIVITY := 0.05
const SCROLL_ZOOM_STEP := 0.05

func _ready() -> void:
    title = "Heightfield Preview"
    min_size = Vector2i(640, 480)
    borderless = true
    always_on_top = false
    transient = false
    _resize_to_display()
    var root_window := get_tree().root
    if root_window != null and root_window.has_signal("size_changed"):
        root_window.size_changed.connect(_resize_to_display)
    close_requested.connect(_on_close_requested)
    size_changed.connect(_on_view_resized)

    _container = SubViewportContainer.new()
    _container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    _container.size_flags_vertical = Control.SIZE_EXPAND_FILL
    _container.anchor_right = 1.0
    _container.anchor_bottom = 1.0
    add_child(_container)

    _viewport = SubViewport.new()
    _viewport.disable_3d = false
    _viewport.use_debanding = true
    _viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    _viewport.size = size
    _container.add_child(_viewport)

    _root = Node3D.new()
    _root.name = "HeightfieldRoot"
    _viewport.add_child(_root)

    _heightfield = HeightfieldLayer3D.new()
    _root.add_child(_heightfield)
    _heightfield.zoom_multiplier_changed.connect(_on_zoom_multiplier_changed)
    _heightfield.strategic_view_requested.connect(_on_heightfield_strategic_view_requested)
    _build_ui_overlay()
    _configure_zoom_slider()

    _camera = Camera3D.new()
    _camera.current = true
    _camera.fov = 55.0
    _root.add_child(_camera)

    _light = DirectionalLight3D.new()
    _light.light_energy = 2.2
    _light.rotation_degrees = Vector3(-60.0, 35.0, 0.0)
    _root.add_child(_light)

func update_snapshot(
    heightfield: Dictionary,
    terrain_colors: PackedColorArray,
    overlay_values: PackedFloat32Array,
    overlay_color: Color,
    overlay_key: String,
    grid_width: int,
    grid_height: int
) -> void:
    if heightfield.is_empty():
        return
    _heightfield.set_heightfield_data(heightfield)
    _heightfield.reset_camera_controls()
    if not terrain_colors.is_empty():
        _heightfield.set_biome_colors(terrain_colors, grid_width, grid_height)
    var overlay_enabled := overlay_key != "" and not overlay_values.is_empty()
    _heightfield.set_overlay_values(overlay_values, grid_width, grid_height, overlay_color, overlay_enabled)
    var wait_frames := 2
    var camera := _camera
    await get_tree().process_frame
    while wait_frames > 0:
        await get_tree().process_frame
        wait_frames -= 1
    _heightfield.fit_camera(camera)
    _log_camera_state(camera)
    _update_zoom_from_heightfield()

func _on_view_resized() -> void:
    if _viewport != null:
        _viewport.size = size

func _on_close_requested() -> void:
    hide()

func _resize_to_display() -> void:
    var root_window := get_tree().root
    if root_window == null:
        return
    var desired_size: Vector2i = root_window.size
    if desired_size.x <= 0 or desired_size.y <= 0:
        return
    size = desired_size
    position = Vector2i.ZERO

func _request_strategic_exit() -> void:
    emit_signal("strategic_view_requested")
    hide()

func _nudge_zoom(delta: float) -> void:
    if _heightfield == null:
        return
    var current := _heightfield.get_user_zoom_multiplier()
    _heightfield.set_user_zoom_multiplier(current + delta)

func _unhandled_input(event: InputEvent) -> void:
    if not visible or _heightfield == null:
        return
    if event.is_action_pressed("map_switch_strategic_view"):
        _request_strategic_exit()
        event.accept()
        return
    if event.is_action_pressed("map_toggle_relief"):
        hide()
        event.accept()
        return
    if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
        hide()
        event.accept()
        return
    if event is InputEventMouseButton:
        var mouse_event: InputEventMouseButton = event
        match mouse_event.button_index:
            MOUSE_BUTTON_WHEEL_UP:
                if mouse_event.pressed:
                    _nudge_zoom(-SCROLL_ZOOM_STEP)
                    mouse_event.accept()
                return
            MOUSE_BUTTON_WHEEL_DOWN:
                if mouse_event.pressed:
                    _nudge_zoom(SCROLL_ZOOM_STEP)
                    mouse_event.accept()
                return
            MOUSE_BUTTON_RIGHT:
                _orbit_drag_active = mouse_event.pressed
                _last_mouse_position = mouse_event.position
                mouse_event.accept()
                return
            MOUSE_BUTTON_MIDDLE:
                _pan_drag_active = mouse_event.pressed
                _last_mouse_position = mouse_event.position
                mouse_event.accept()
                return
    elif event is InputEventMouseMotion:
        var motion: InputEventMouseMotion = event
        if _orbit_drag_active:
            _heightfield.adjust_orbit(motion.relative.x * ORBIT_SENSITIVITY)
            _heightfield.adjust_tilt(-motion.relative.y * TILT_SENSITIVITY)
            motion.accept()
        elif _pan_drag_active:
            var tile_scale := _heightfield.get_tile_scale_value()
            var pan_delta := Vector2(-motion.relative.x, motion.relative.y) * tile_scale * PAN_SENSITIVITY
            _heightfield.adjust_pan(pan_delta)
            motion.accept()

var _camera_logged := false
func _log_camera_state(camera: Camera3D) -> void:
    if camera == null or _camera_logged:
        return
    var pos := camera.global_transform.origin
    var forward := -camera.global_transform.basis.z
    print("[HeightfieldCamera] pos=(%.2f, %.2f, %.2f) forward=(%.2f, %.2f, %.2f)" % [
        pos.x, pos.y, pos.z, forward.x, forward.y, forward.z
    ])
    _camera_logged = true

func _build_ui_overlay() -> void:
    _ui_overlay = PanelContainer.new()
    _ui_overlay.name = "PreviewControls"
    _ui_overlay.anchor_right = 1.0
    _ui_overlay.offset_right = -16.0
    _ui_overlay.offset_top = 16.0
    _ui_overlay.custom_minimum_size = Vector2(260, 56)
    var margin := MarginContainer.new()
    margin.add_theme_constant_override("margin_left", 8)
    margin.add_theme_constant_override("margin_right", 8)
    margin.add_theme_constant_override("margin_top", 6)
    margin.add_theme_constant_override("margin_bottom", 6)
    _ui_overlay.add_child(margin)
    var vbox := VBoxContainer.new()
    vbox.custom_minimum_size = Vector2(240, 44)
    margin.add_child(vbox)
    _zoom_label = Label.new()
    _zoom_label.text = "Camera Zoom"
    vbox.add_child(_zoom_label)
    _zoom_slider = HSlider.new()
    _zoom_slider.min_value = 0.3
    _zoom_slider.max_value = 1.8
    _zoom_slider.step = 0.02
    _zoom_slider.value = 1.0
    _zoom_slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    _zoom_slider.tooltip_text = "Adjust distance of the preview camera."
    _zoom_slider.value_changed.connect(_on_zoom_slider_changed)
    vbox.add_child(_zoom_slider)
    add_child(_ui_overlay)

func _configure_zoom_slider() -> void:
    if _heightfield == null or _zoom_slider == null:
        return
    var bounds: Vector2 = _heightfield.get_zoom_bounds()
    _zoom_slider.min_value = bounds.x
    _zoom_slider.max_value = bounds.y
    var span: float = max(bounds.y - bounds.x, 0.001)
    _zoom_slider.step = span / 200.0

func _on_zoom_slider_changed(value: float) -> void:
    if _heightfield == null:
        return
    _heightfield.set_user_zoom_multiplier(value)
    _zoom_label.text = "Camera Zoom (%.2f×)" % value

func _update_zoom_from_heightfield() -> void:
    if _heightfield == null or _zoom_slider == null:
        return
    _configure_zoom_slider()
    var value := _heightfield.get_user_zoom_multiplier()
    _zoom_slider.value = value
    _zoom_label.text = "Camera Zoom (%.2f×)" % value

func _on_zoom_multiplier_changed(value: float) -> void:
    if _zoom_slider == null:
        return
    if not is_equal_approx(_zoom_slider.value, value):
        _zoom_slider.value = value
    _zoom_label.text = "Camera Zoom (%.2f×)" % value

func _on_heightfield_strategic_view_requested() -> void:
    _request_strategic_exit()
