extends Window
class_name HeightfieldPreview

signal strategic_view_requested

const HeightfieldLayer3D := preload("res://src/scripts/HeightfieldLayer3D.gd")
const HudLayerScene := preload("res://src/ui/HudLayer.tscn")
const InspectorLayerScene := preload("res://src/ui/InspectorLayer.tscn")

var _viewport: SubViewport
var _container: SubViewportContainer
var _camera: Camera3D
var _light: DirectionalLight3D
var _heightfield: HeightfieldLayer3D
var _hud_layer: CanvasLayer
var _inspector_layer: CanvasLayer
var _orbit_drag_active := false
var _pan_drag_active := false
var _last_mouse_position := Vector2.ZERO
const ORBIT_SENSITIVITY := 0.25
const TILT_SENSITIVITY := 0.18
const PAN_SENSITIVITY := 0.05
const SCROLL_ZOOM_STEP := 0.05
const HUD_ZOOM_STEP := 0.05

func _ready() -> void:
    title = "Heightfield Preview"
    min_size = Vector2i(640, 480)
    borderless = true
    always_on_top = false
    transient = false
    _resize_to_display()
    
    # Force content scale mode to handle high DPI potentially better
    content_scale_mode = Window.CONTENT_SCALE_MODE_CANVAS_ITEMS
    content_scale_aspect = Window.CONTENT_SCALE_ASPECT_EXPAND
    
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
    _container.stretch = true
    add_child(_container)
    
    _viewport = SubViewport.new()
    _viewport.own_world_3d = true
    _viewport.handle_input_locally = true
    _viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
    _container.add_child(_viewport)
    
    _heightfield = HeightfieldLayer3D.new()
    _viewport.add_child(_heightfield)
    _heightfield.strategic_view_requested.connect(_on_heightfield_strategic_view_requested)
    
    _hud_layer = HudLayerScene.instantiate()
    _hud_layer.layer = 50
    _hud_layer.visible = true
    add_child(_hud_layer)
    
    _connect_hud_controls()
    
    _inspector_layer = InspectorLayerScene.instantiate()
    _inspector_layer.layer = 51 # Above HUD
    add_child(_inspector_layer)
    
    # Inject dependencies into Inspector
    var main_node = root_window.get_node_or_null("Main")
    if main_node:
        if _inspector_layer.has_method("set_command_client") and main_node.get("command_client"):
            var client = main_node.command_client
            var connected = client.is_connection_active() if client.has_method("is_connection_active") else false
            _inspector_layer.call("set_command_client", client, connected)
            
        if _inspector_layer.has_method("set_hud_layer"):
            _inspector_layer.call("set_hud_layer", _hud_layer)
            
        if _inspector_layer.has_method("attach_script_host") and main_node.get("script_host_manager"):
            _inspector_layer.call("attach_script_host", main_node.script_host_manager)

    _camera = Camera3D.new()
    _camera.current = true
    _camera.fov = 55.0
    _camera.position = Vector3(512, 512, 512)
    _camera.look_at(Vector3(256, 0, 256))
    _viewport.add_child(_camera)

    _light = DirectionalLight3D.new()
    _light.light_energy = 2.2
    _light.rotation_degrees = Vector3(-60.0, 35.0, 0.0)
    _viewport.add_child(_light)
    
    print("[HUD->Preview] _ready complete. Window Size: ", size)

func _process(delta: float) -> void:
    pass

func relay_hud_call(method_name: String, args: Array = []) -> void:
    if _hud_layer == null or method_name == "":
        return
    if _hud_layer.has_method(method_name):
        print("[HUD->Preview] relay_hud_call: ", method_name, " args: ", args)
        _hud_layer.callv(method_name, args)
    else:
        print("[HUD->Preview] relay_hud_call: Method not found: ", method_name)

func apply_hud_state(state: Dictionary) -> void:
    if _hud_layer == null or state.is_empty():
        return
    print("[HUD->Preview] applying cached state: ", state.keys())
    for method_name in state.keys():
        var args: Array = state[method_name] if state[method_name] is Array else [state[method_name]]
        relay_hud_call(method_name, args)
    _log_hud_panel_state("cached_state")

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
    _update_hud_zoom_label()
    if not terrain_colors.is_empty():
        _heightfield.set_biome_colors(terrain_colors, grid_width, grid_height)
    var overlay_enabled := overlay_key != "" and not overlay_values.is_empty()
    _heightfield.set_overlay_values(overlay_values, grid_width, grid_height, overlay_color, overlay_enabled)
    var wait_frames := 2
    await get_tree().process_frame
    while wait_frames > 0:
        await get_tree().process_frame
        wait_frames -= 1
    _heightfield.fit_camera(_camera)
    _log_camera_state(_camera)
    _log_hud_panel_state("update_snapshot")

func _on_view_resized() -> void:
    if _viewport != null:
        print("[HUD->Preview] View resized. Window: ", size, " Viewport: ", _viewport.size)
        # _viewport.size handled by stretch=true

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
    _update_hud_zoom_label()

func _unhandled_input(event: InputEvent) -> void:
    if not visible or _heightfield == null:
        return
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
    if _hud_layer == null:
        return
    if not _hud_layer.is_connected("ui_zoom_delta", Callable(self, "_on_hud_zoom_delta")):
        _hud_layer.ui_zoom_delta.connect(_on_hud_zoom_delta)
    if not _hud_layer.is_connected("ui_zoom_reset", Callable(self, "_on_hud_zoom_reset")):
        _hud_layer.ui_zoom_reset.connect(_on_hud_zoom_reset)
    var next_turn_button := _hud_node("LayoutRoot/RootColumn/BottomBar/NextTurnButton") as Button
    if next_turn_button != null:
        next_turn_button.disabled = true

func _hud_node(path: String) -> Node:
    if _hud_layer == null:
        return null
    return _hud_layer.get_node_or_null(path)

func _update_hud_zoom_label() -> void:
    if _hud_layer == null or _heightfield == null:
        return
    if _hud_layer.has_method("set_ui_zoom"):
        _hud_layer.set_ui_zoom(_heightfield.get_user_zoom_multiplier())

func _on_hud_zoom_delta(step: float) -> void:
    _nudge_zoom(step * HUD_ZOOM_STEP)

func _on_hud_zoom_reset() -> void:
    if _heightfield == null:
        return
    _heightfield.reset_camera_controls()
    _update_hud_zoom_label()

func _mark_input_handled() -> void:
    var viewport := get_viewport()
    if viewport != null:
        viewport.set_input_as_handled()

func _on_heightfield_strategic_view_requested() -> void:
    _request_strategic_exit()

func _log_hud_panel_state(context: String) -> void:
    if _hud_layer == null:
        return
    var panels: Dictionary = {
        "campaign_block": _hud_node("LayoutRoot/RootColumn/TopBar/CampaignBlock"),
        "turn_block": _hud_node("LayoutRoot/RootColumn/TopBar/TurnBlock"),
        "command_feed": _hud_node("LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel"),
        "victory_panel": _hud_node("LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel"),
        "terrain_legend": _hud_node("LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel"),
        "right_dock": _hud_node("LayoutRoot/RootColumn/ContentRow/RightDock"),
        "layout_root": _hud_node("LayoutRoot"),
        "root_column": _hud_node("LayoutRoot/RootColumn"),
        "content_row": _hud_node("LayoutRoot/RootColumn/ContentRow"),
        "center_spacer": _hud_node("LayoutRoot/RootColumn/ContentRow/CenterSpacer"),
        "next_turn": _hud_node("LayoutRoot/RootColumn/BottomBar/NextTurnButton")
    }
    var visibility: Array[String] = []
    for key in panels.keys():
        var node: Node = panels[key]
        var visible: bool = node != null and node.is_visible_in_tree()
        var rect: String = ""
        if node is Control:
            rect = " rect=" + str(node.get_global_rect())
            rect += " size=" + str(node.size)
        visibility.append("%s=%s%s" % [key, visible, rect])
    print("[HUD->Preview] panels(%s): %s" % [context, ", ".join(visibility)])
