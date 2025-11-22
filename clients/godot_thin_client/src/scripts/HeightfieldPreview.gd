extends PanelContainer
class_name HeightfieldPreview

signal strategic_view_requested
signal ui_zoom_delta(delta: float)
signal ui_zoom_reset
signal unit_scout_requested(x: int, y: int, band_entity_bits: int)
signal unit_found_camp_requested(x: int, y: int)
signal herd_follow_requested(herd_id: String)
signal forage_requested(x: int, y: int, module_key: String)
signal next_turn_requested(steps: int)
signal overlay_changed(key: String)
signal inspector_toggle_requested
signal legend_toggle_requested

const HeightfieldLayer3D := preload("res://src/scripts/HeightfieldLayer3D.gd")
# Removed HudLayerScene and InspectorLayerScene preloads

var _viewport: SubViewport
var _container: SubViewportContainer
var _camera: Camera3D
var _light: DirectionalLight3D
var _heightfield: HeightfieldLayer3D
# Removed _hud_layer and _inspector_layer variables
var _tools_layer: CanvasLayer
var _orbit_drag_active := false
var _pan_drag_active := false
var _last_mouse_position := Vector2.ZERO
const ORBIT_SENSITIVITY := 0.25
const TILT_SENSITIVITY := 0.18
const PAN_SENSITIVITY := 0.05
const SCROLL_ZOOM_STEP := 0.05
const HUD_ZOOM_STEP := 0.05

func _ready() -> void:
    # Ensure we fill the parent
    set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    
    var root_window := get_tree().root
    # No need to listen to root size changes, anchors handle it.
    
    # PanelContainer has 'resized' signal, not 'size_changed'
    resized.connect(_on_view_resized)
    
    _container = SubViewportContainer.new()
    _container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    _container.size_flags_vertical = Control.SIZE_EXPAND_FILL
    _container.anchor_right = 1.0
    _container.anchor_bottom = 1.0
    _container.stretch = true
    add_child(_container)
    
    _viewport = SubViewport.new()
    _viewport.own_world_3d = true
    _viewport.handle_input_locally = true
    _viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    _container.add_child(_viewport)
    
    var env := Environment.new()
    env.background_mode = Environment.BG_COLOR
    env.background_color = Color(0.1, 0.1, 0.3) # Dark Blue
    var world_env := WorldEnvironment.new()
    world_env.environment = env
    _viewport.add_child(world_env)
    
    _heightfield = HeightfieldLayer3D.new()
    _viewport.add_child(_heightfield)
    _heightfield.strategic_view_requested.connect(_on_heightfield_strategic_view_requested)
    
    # Removed internal HUD and Inspector instantiation
    
    _setup_tools_layer()
    
    # Remove debug markers
    # _add_screen_width_markers()

    _camera = Camera3D.new()
    _camera.current = true
    _camera.fov = 55.0
    _camera.position = Vector3(40, 60, 40)
    _viewport.add_child(_camera)
    _camera.look_at(Vector3(40, 0, 26))

    _light = DirectionalLight3D.new()
    _light.light_energy = 2.2
    _light.rotation_degrees = Vector3(-60.0, 35.0, 0.0)
    _viewport.add_child(_light)
    
    # Initial resize handled by anchors
    print("[HUD->Preview] _ready complete. Mode: Control Overlay")

func _process(delta: float) -> void:
    pass

func relay_hud_call(method_name: String, args: Array = []) -> void:
    # No internal HUD to relay to
    pass

func apply_hud_state(state: Dictionary) -> void:
    # No internal HUD to apply state to
    pass

func update_selection(tile_info: Dictionary, unit_data: Dictionary, herd_data: Dictionary) -> void:
    # Main HUD handles selection display
    pass

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
    # _update_hud_zoom_label() - Main HUD handles this
    if not terrain_colors.is_empty():
        _heightfield.set_biome_colors(terrain_colors, grid_width, grid_height)
    var overlay_enabled := overlay_key != "" and not overlay_values.is_empty()
    _heightfield.set_overlay_values(overlay_values, grid_width, grid_height, overlay_color, overlay_enabled)
    
    _update_active_overlay_button(overlay_key)
    var wait_frames := 2
    await get_tree().process_frame
    while wait_frames > 0:
        await get_tree().process_frame
        wait_frames -= 1
    _heightfield.fit_camera(_camera)
    _log_camera_state(_camera)
    # _log_hud_panel_state("update_snapshot")

func _on_view_resized() -> void:
    if _viewport != null:
        # _viewport.size handled by stretch=true
        pass

func _on_close_requested() -> void:
    hide()

func _request_strategic_exit() -> void:
    print("[HUD->Preview] _request_strategic_exit called")
    hide()
    emit_signal("strategic_view_requested")
    hide()

func _nudge_zoom(delta: float) -> void:
    if _heightfield == null:
        return
    var current := _heightfield.get_user_zoom_multiplier()
    _heightfield.set_user_zoom_multiplier(current + delta)
    _update_hud_zoom_label()

func _input(event: InputEvent) -> void:
    if not visible or _heightfield == null:
        return

    if event.is_action_pressed("ui_cancel"):
        _request_strategic_exit()
        _mark_input_handled()
        return
    
    if event is InputEventKey and event.pressed:
        if event.keycode == KEY_I:
            emit_signal("inspector_toggle_requested")
            _mark_input_handled()
            return
        if event.keycode == KEY_L:
            emit_signal("legend_toggle_requested")
            _mark_input_handled()
            return
        # Overlay hotkeys 1-9
        if event.keycode >= KEY_1 and event.keycode <= KEY_9:
            var overlay_idx = event.keycode - KEY_1
            _handle_overlay_hotkey(overlay_idx)
            _mark_input_handled()
            return
    
    # Original _unhandled_input logic for actions and escape
    if event.is_action_pressed("map_switch_strategic_view"):
        _request_strategic_exit()
        _mark_input_handled()
        return
    if event.is_action_pressed("map_toggle_relief"):
        hide()
        _mark_input_handled()
        return
    if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
        hide()
        _mark_input_handled()
        return
    if event is InputEventMouseButton:
        var mouse_event: InputEventMouseButton = event
        match mouse_event.button_index:
            MOUSE_BUTTON_WHEEL_UP:
                if mouse_event.pressed:
                    _nudge_zoom(-SCROLL_ZOOM_STEP)
                    _mark_input_handled()
                return
            MOUSE_BUTTON_WHEEL_DOWN:
                if mouse_event.pressed:
                    _nudge_zoom(SCROLL_ZOOM_STEP)
                    _mark_input_handled()
                return
            MOUSE_BUTTON_RIGHT:
                _orbit_drag_active = mouse_event.pressed
                _last_mouse_position = mouse_event.position
                _mark_input_handled()
                return
            MOUSE_BUTTON_MIDDLE:
                _pan_drag_active = mouse_event.pressed
                _last_mouse_position = mouse_event.position
                _mark_input_handled()
                return
    elif event is InputEventMouseMotion:
        var motion: InputEventMouseMotion = event
        if _orbit_drag_active:
            _heightfield.adjust_orbit(motion.relative.x * ORBIT_SENSITIVITY)
            _heightfield.adjust_tilt(-motion.relative.y * TILT_SENSITIVITY)
            _mark_input_handled()
        elif _pan_drag_active:
            var tile_scale := _heightfield.get_tile_scale_value()
            var pan_delta := Vector2(-motion.relative.x, motion.relative.y) * tile_scale * PAN_SENSITIVITY
            _heightfield.adjust_pan(pan_delta)
            _mark_input_handled()

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

func _connect_hud_controls() -> void:
    # Main HUD connections handled by Main.gd
    pass

func _hud_node(path: String) -> Node:
    return null

func _update_hud_zoom_label() -> void:
    # Main HUD handles this
    pass

func _on_hud_zoom_delta(step: float) -> void:
    _nudge_zoom(step * HUD_ZOOM_STEP)

func _on_hud_zoom_reset() -> void:
    if _heightfield == null:
        return
    _heightfield.reset_camera_controls()
    # _update_hud_zoom_label()

func _mark_input_handled() -> void:
    var viewport := get_viewport()
    if viewport != null:
        viewport.set_input_as_handled()

func _on_heightfield_strategic_view_requested() -> void:
    _request_strategic_exit()

func _log_hud_panel_state(context: String) -> void:
    pass

# --- New Overlay Tools Implementation ---

var _overlay_buttons: Dictionary = {}
var _overlay_keys: Array[String] = [
    "logistics", "sentiment", "corruption", "fog", 
    "culture", "military", "crisis", "elevation", "moisture"
]

func _setup_tools_layer() -> void:
    _tools_layer = CanvasLayer.new()
    _tools_layer.layer = 103 # Above Inspector
    add_child(_tools_layer)
    
    var container := VBoxContainer.new()
    container.mouse_filter = Control.MOUSE_FILTER_IGNORE
    container.anchor_right = 1.0
    container.anchor_bottom = 1.0
    _tools_layer.add_child(container)
    
    # Top bar spacer
    var top_spacer := Control.new()
    top_spacer.custom_minimum_size = Vector2(0, 80)
    top_spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    container.add_child(top_spacer)
    
    var row := HBoxContainer.new()
    row.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    container.add_child(row)
    
    # Left spacer
    var left_spacer := Control.new()
    left_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    left_spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(left_spacer)
    
    # Tools Panel
    var panel := PanelContainer.new()
    panel.custom_minimum_size = Vector2(60, 0)
    row.add_child(panel)
    
    var tools_vbox := VBoxContainer.new()
    panel.add_child(tools_vbox)
    
    var title := Label.new()
    title.text = "Overlays"
    title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    tools_vbox.add_child(title)
    
    for key in _overlay_keys:
        var btn := Button.new()
        btn.text = key.capitalize()
        btn.toggle_mode = true
        btn.focus_mode = Control.FOCUS_NONE
        btn.pressed.connect(func(): _on_overlay_button_pressed(key))
        tools_vbox.add_child(btn)
        _overlay_buttons[key] = btn
        
    tools_vbox.add_child(HSeparator.new())
    
    var inspector_btn := Button.new()
    inspector_btn.text = "Inspector (I)"
    inspector_btn.focus_mode = Control.FOCUS_NONE
    inspector_btn.pressed.connect(func(): emit_signal("inspector_toggle_requested"))
    tools_vbox.add_child(inspector_btn)

    var legend_btn := Button.new()
    legend_btn.text = "Legend (L)"
    legend_btn.focus_mode = Control.FOCUS_NONE
    legend_btn.pressed.connect(func(): emit_signal("legend_toggle_requested"))
    tools_vbox.add_child(legend_btn)

    # Right spacer (small padding)
    var right_pad := Control.new()
    right_pad.custom_minimum_size = Vector2(16, 0)
    right_pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(right_pad)

func _on_overlay_button_pressed(key: String) -> void:
    emit_signal("overlay_changed", key)
    _update_active_overlay_button(key)

func _handle_overlay_hotkey(idx: int) -> void:
    if idx >= 0 and idx < _overlay_keys.size():
        var key := _overlay_keys[idx]
        _on_overlay_button_pressed(key)

func _update_active_overlay_button(active_key: String) -> void:
    for key in _overlay_buttons.keys():
        var btn: Button = _overlay_buttons[key]
        btn.set_pressed_no_signal(key == active_key)

func _add_screen_width_markers() -> void:
    var debug_layer = CanvasLayer.new()
    debug_layer.layer = 100
    debug_layer.name = "DebugMarkers"
    add_child(debug_layer)
    
    var screen_size := DisplayServer.screen_get_size()
    var width := float(screen_size.x)
    
    print("[HUD->Preview] Adding fine-grained markers starting at 3400px.")
    
    # Add markers every 100px starting from 3400
    var start_x := 3400
    var end_x := int(width)
    var step := 100
    
    for x in range(start_x, end_x + step, step):
        var marker = ColorRect.new()
        marker.size = Vector2(40, 80)
        marker.position = Vector2(x, 300)
        # Alternate colors for visibility
        marker.color = Color.RED if (x / 100) % 2 == 0 else Color.YELLOW
        
        var label = Label.new()
        label.text = "%d" % x
        label.position = Vector2(0, -30)
        label.modulate = Color.WHITE
        label.add_theme_font_size_override("font_size", 20)
        
        # Background for label
        var lbl_bg = ColorRect.new()
        lbl_bg.color = Color(0, 0, 0, 0.8)
        lbl_bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
        lbl_bg.show_behind_parent = true
        label.add_child(lbl_bg)
        
        marker.add_child(label)
        debug_layer.add_child(marker)
        
    # Also keep the 0 marker for reference
    var zero_marker = ColorRect.new()
    zero_marker.size = Vector2(60, 60)
    zero_marker.position = Vector2(0, 300)
    zero_marker.color = Color.GREEN
    var zero_lbl = Label.new()
    zero_lbl.text = "0 px"
    zero_lbl.add_theme_font_size_override("font_size", 24)
    zero_marker.add_child(zero_lbl)
    debug_layer.add_child(zero_marker)

