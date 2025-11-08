extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")
const CommandClient = preload("res://src/scripts/CommandClient.gd")
const ScriptHostManager = preload("res://src/scripts/scripting/ScriptHostManager.gd")
const LocalizationStore = preload("res://src/scripts/LocalizationStore.gd")

@onready var map_view: Node2D = $MapLayer
@onready var hud: CanvasLayer = $HUD
@onready var camera: Camera2D = $Camera2D
@onready var inspector: CanvasLayer = $Inspector

var snapshot_loader: SnapshotLoader
var playback_timer: Timer
var streaming_mode: bool = false
var stream_connection_timer: float = 0.0
var command_client: CommandClient
var _warned_stream_fallback: bool = false
var _camera_initialized: bool = false
var script_host_manager: ScriptHostManager = null
var ui_zoom: float = 1.0
var localization_store: LocalizationStore = null
var _campaign_label_signature: String = ""
var _victory_analytics_signature: String = ""

const MOCK_DATA_PATH = "res://src/data/mock_snapshots.json"
const TURN_INTERVAL_SECONDS = 1.5
const STREAM_DEFAULT_ENABLED = false
const STREAM_HOST = "127.0.0.1"
const STREAM_PORT = 41002
const STREAM_CONNECTION_TIMEOUT = 5.0
const CAMERA_PAN_SPEED = 220.0
const CAMERA_ZOOM_STEP = 0.1
const CAMERA_ZOOM_MIN = 0.5
const CAMERA_ZOOM_MAX = 1.5
const COMMAND_HOST = "127.0.0.1"
const COMMAND_PORT = 41001
const COMMAND_PROTO_PORT = 41001
const UI_ZOOM_STEP = 0.1
const UI_ZOOM_MIN = 0.5
const UI_ZOOM_MAX = 2.0
const SNAPSHOT_DELTA_FIELDS := [
    "influencer_updates",
    "population_updates",
    "tile_updates",
    "trade_link_updates",
    "influencer_removed",
    "population_removed"
]

func _ready() -> void:
    var ext: Resource = load("res://native/shadow_scale_godot.gdextension")
    if ext == null:
        push_warning("ShadowScale Godot extension not found; streaming disabled.")
    snapshot_loader = SnapshotLoader.new()
    snapshot_loader.load_mock_data(MOCK_DATA_PATH)
    localization_store = LocalizationStore.new()
    localization_store.load_default()
    var stream_enabled: bool = _determine_stream_enabled()
    var stream_host: String = _determine_stream_host()
    var stream_port: int = _determine_stream_port()
    if stream_enabled:
        var err: Error = snapshot_loader.enable_stream(stream_host, stream_port)
        if err == OK:
            streaming_mode = true
            _warned_stream_fallback = false
        else:
            push_warning("Godot client: unable to connect to snapshot stream (error %d). Using mock data." % err)
    set_process(true)
    if not streaming_mode:
        _ensure_timer()
    var command_host: String = _determine_command_host()
    var command_port: int = _determine_command_port()
    var command_proto_port: int = _determine_command_proto_port()
    command_client = CommandClient.new()
    command_client.set_proto_port(command_proto_port)
    var command_err: Error = command_client.connect_to_host(command_host, command_port)
    if command_err == OK:
        command_client.poll()  # poll to update status
    if command_err != OK:
        push_warning("Godot client: unable to connect to command port (error %d)." % command_err)
    if inspector != null and inspector.has_method("set_command_client"):
        inspector.call("set_command_client", command_client, command_err == OK)
    if inspector != null and inspector.has_method("set_hud_layer"):
        inspector.call("set_hud_layer", hud)
    script_host_manager = ScriptHostManager.new()
    add_child(script_host_manager)
    script_host_manager.setup(command_client)
    if inspector != null and inspector.has_method("attach_script_host"):
        inspector.call("attach_script_host", script_host_manager)
    if hud != null and hud.has_method("set_localization_store"):
        hud.call("set_localization_store", localization_store)

    var initial: Dictionary = {}
    if streaming_mode and not snapshot_loader.last_stream_snapshot.is_empty():
        initial = snapshot_loader.last_stream_snapshot
    else:
        initial = snapshot_loader.current()
    _apply_snapshot(initial)
    _ensure_ui_zoom_actions()
    ui_zoom = _resolve_ui_zoom()
    _apply_ui_zoom()
    if hud != null:
        if not hud.is_connected("ui_zoom_delta", Callable(self, "_on_hud_zoom_delta")):
            hud.connect("ui_zoom_delta", Callable(self, "_on_hud_zoom_delta"))
        if not hud.is_connected("ui_zoom_reset", Callable(self, "_on_hud_zoom_reset")):
            hud.connect("ui_zoom_reset", Callable(self, "_on_hud_zoom_reset"))
    if inspector != null and inspector.has_method("attach_map_view"):
        inspector.call("attach_map_view", map_view)
    if map_view != null and inspector != null and map_view.has_signal("hex_selected") and inspector.has_method("focus_tile_from_map"):
        map_view.connect("hex_selected", Callable(inspector, "focus_tile_from_map"))
    if map_view != null and map_view.has_signal("overlay_legend_changed") and hud != null and hud.has_method("update_overlay_legend"):
        map_view.connect("overlay_legend_changed", Callable(self, "_on_overlay_legend_changed"))
        map_view.call_deferred("refresh_overlay_legend")
    if inspector != null and inspector.has_method("set_streaming_active"):
        inspector.call("set_streaming_active", streaming_mode)
    _ensure_action_binding("toggle_inspector", Key.KEY_I)
    _ensure_action_binding("toggle_legend", Key.KEY_L)

func _ensure_timer() -> void:
    if is_instance_valid(playback_timer):
        return
    playback_timer = Timer.new()
    playback_timer.wait_time = TURN_INTERVAL_SECONDS
    playback_timer.one_shot = false
    playback_timer.autostart = true
    add_child(playback_timer)
    playback_timer.timeout.connect(_on_tick)

func _on_tick() -> void:
    var snapshot: Dictionary = snapshot_loader.advance()
    _apply_snapshot(snapshot)

func _apply_snapshot(snapshot: Dictionary) -> void:
    if snapshot.is_empty():
        return
    var is_delta := _snapshot_is_delta(snapshot)
    _update_campaign_label(snapshot.get("campaign_label", {}))
    var metrics_variant: Variant = map_view.call("display_snapshot", snapshot)
    var metrics: Dictionary = metrics_variant if metrics_variant is Dictionary else {}
    hud.call("update_overlay", snapshot.get("turn", 0), metrics)
    if snapshot.has("victory"):
        var victory_variant: Variant = snapshot["victory"]
        if victory_variant is Dictionary:
            if hud != null and hud.has_method("update_victory_state"):
                hud.call("update_victory_state", victory_variant)
            _emit_victory_analytics(victory_variant)
    if hud != null and hud.has_method("set_ui_zoom"):
        hud.call("set_ui_zoom", ui_zoom)
    if inspector != null:
        if is_delta:
            if inspector.has_method("update_delta"):
                inspector.call("update_delta", snapshot)
        else:
            if inspector.has_method("update_snapshot"):
                inspector.call("update_snapshot", snapshot)
        if inspector.has_method("set_streaming_active"):
            inspector.call("set_streaming_active", streaming_mode)
    var recenter: bool = false
    if metrics.has("dimensions_changed"):
        recenter = bool(metrics["dimensions_changed"])
    var center_variant: Variant = map_view.call("get_world_center")
    if center_variant is Vector2 and (recenter or not _camera_initialized):
        camera.position = center_variant
        _camera_initialized = true
    if script_host_manager != null and script_host_manager.has_host():
        if is_delta:
            script_host_manager.handle_delta(snapshot)
        else:
            script_host_manager.handle_snapshot(snapshot)

func _emit_victory_analytics(data: Dictionary) -> void:
    if data.is_empty():
        return
    var winner_variant: Variant = data.get("winner", {})
    if not (winner_variant is Dictionary):
        return
    var winner: Dictionary = winner_variant
    var mode: String = String(winner.get("mode", "")).strip_edges()
    if mode == "":
        return
    var tick: int = int(winner.get("tick", -1))
    var signature := "%s#%d" % [mode, tick]
    if signature == _victory_analytics_signature:
        return
    _victory_analytics_signature = signature
    var label: String = String(winner.get("label", mode)).strip_edges()
    if label == "":
        label = mode
    var faction: int = int(winner.get("faction", -1))
    print("[analytics] victory mode=\"%s\" label=\"%s\" faction=%d tick=%d" % [mode, label, faction, tick])

func skip_to_next_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.advance())

func skip_to_previous_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.rewind())

func _unhandled_input(event: InputEvent) -> void:
    _ensure_ui_zoom_actions()
    if event.is_action_pressed("ui_right"):
        skip_to_next_turn()
    elif event.is_action_pressed("ui_left"):
        skip_to_previous_turn()
    elif event.is_action_pressed("ui_accept"):
        if map_view != null:
            map_view.call("toggle_terrain_mode")
    elif event.is_action_pressed("ui_zoom_in"):
        _adjust_ui_zoom(UI_ZOOM_STEP)
    elif event.is_action_pressed("ui_zoom_out"):
        _adjust_ui_zoom(-UI_ZOOM_STEP)
    elif event.is_action_pressed("ui_zoom_reset"):
        set_ui_zoom(1.0)
    elif event is InputEventMouseButton:
        var mouse_event: InputEventMouseButton = event as InputEventMouseButton
        if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
            _adjust_camera_zoom(-CAMERA_ZOOM_STEP)
        elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
            _adjust_camera_zoom(CAMERA_ZOOM_STEP)

func _toggle_inspector_visibility() -> void:
    if inspector == null:
        return
    if inspector.has_method("toggle_panel_visibility"):
        inspector.call("toggle_panel_visibility")
    elif inspector.has_method("set_panel_visible") and inspector.has_method("is_panel_visible"):
        var current_visible: bool = bool(inspector.call("is_panel_visible"))
        inspector.call("set_panel_visible", not current_visible)

func _toggle_legend_visibility() -> void:
    if hud == null:
        return
    if hud.has_method("toggle_legend"):
        hud.call("toggle_legend")

func _update_campaign_label(raw_value: Variant) -> void:
    var label_dict: Dictionary = {}
    if raw_value is Dictionary:
        label_dict = raw_value.duplicate(true)
    if hud != null and hud.has_method("update_campaign_label"):
        hud.call("update_campaign_label", label_dict)
    var title_text: String = _resolve_campaign_field(label_dict, "title")
    var subtitle_text: String = _resolve_campaign_field(label_dict, "subtitle")
    var title_key := String(label_dict.get("title_loc_key", ""))
    var subtitle_key := String(label_dict.get("subtitle_loc_key", ""))
    var profile_id := String(label_dict.get("profile_id", ""))
    var signature := "%s|%s|%s|%s|%s" % [
        profile_id,
        title_text,
        subtitle_text,
        title_key,
        subtitle_key
    ]
    if signature == _campaign_label_signature:
        return
    _campaign_label_signature = signature
    if title_text != "" or subtitle_text != "" or title_key != "" or subtitle_key != "":
        print("[analytics] campaign_label title=\"%s\" subtitle=\"%s\" loc_title=\"%s\" loc_subtitle=\"%s\"" % [
            title_text,
            subtitle_text,
            title_key,
            subtitle_key
        ])

func _resolve_campaign_field(label_dict: Dictionary, field: String) -> String:
    var raw_text := String(label_dict.get(field, ""))
    var loc_key_field := "%s_loc_key" % field
    var loc_key := String(label_dict.get(loc_key_field, ""))
    if localization_store != null and loc_key != "":
        var localized: String = localization_store.resolve(loc_key, raw_text)
        if localized.strip_edges() != "":
            return localized
    return raw_text

func _process(delta: float) -> void:
    if Input.is_action_just_pressed("toggle_inspector"):
        _toggle_inspector_visibility()
    if Input.is_action_just_pressed("toggle_legend"):
        _toggle_legend_visibility()
    if command_client != null:
        command_client.poll()
        command_client.ensure_connected()
    if streaming_mode:
        var streamed: Dictionary = snapshot_loader.poll_stream(delta)
        if not streamed.is_empty():
            if inspector != null and inspector.has_method("set_streaming_active"):
                inspector.call("set_streaming_active", true)
            _apply_snapshot(streamed)
            stream_connection_timer = 0.0
            _warned_stream_fallback = false
        else:
            var status: int = snapshot_loader.stream_status()
            match status:
                StreamPeerTCP.STATUS_CONNECTED, StreamPeerTCP.STATUS_CONNECTING:
                    stream_connection_timer = 0.0
                _:
                    stream_connection_timer += delta
                    if stream_connection_timer > STREAM_CONNECTION_TIMEOUT:
                        if not _warned_stream_fallback:
                            push_warning("Godot client: snapshot stream unavailable; falling back to mock playback.")
                            _warned_stream_fallback = true
                        streaming_mode = false
                        snapshot_loader.disable_stream()
                        _ensure_timer()
                        stream_connection_timer = 0.0
                        if inspector != null and inspector.has_method("set_streaming_active"):
                            inspector.call("set_streaming_active", false)
    var pan_input: Vector2 = Vector2(
        Input.get_action_strength("ui_right") - Input.get_action_strength("ui_left"),
        Input.get_action_strength("ui_down") - Input.get_action_strength("ui_up")
    )
    if pan_input != Vector2.ZERO:
        camera.position += pan_input * CAMERA_PAN_SPEED * delta

func _on_overlay_legend_changed(legend: Dictionary) -> void:
    if hud != null and hud.has_method("update_overlay_legend"):
        hud.call("update_overlay_legend", legend)

func _ensure_action_binding(action_name: String, keycode: Key) -> void:
    if not InputMap.has_action(action_name):
        InputMap.add_action(action_name)
    var events := InputMap.action_get_events(action_name)
    for event in events:
        if event is InputEventKey:
            var key_event := event as InputEventKey
            if key_event.physical_keycode == keycode or key_event.keycode == keycode:
                return
    var ev := InputEventKey.new()
    ev.physical_keycode = keycode
    ev.keycode = keycode
    InputMap.action_add_event(action_name, ev)

func _snapshot_is_delta(snapshot: Dictionary) -> bool:
    for field in SNAPSHOT_DELTA_FIELDS:
        if snapshot.has(field):
            return true
    return false

func _adjust_camera_zoom(delta_zoom: float) -> void:
    var new_zoom: float = clamp(camera.zoom.x + delta_zoom, CAMERA_ZOOM_MIN, CAMERA_ZOOM_MAX)
    camera.zoom = Vector2(new_zoom, new_zoom)

func _adjust_ui_zoom(delta: float) -> void:
    set_ui_zoom(ui_zoom + delta)

func set_ui_zoom(scale: float) -> void:
    ui_zoom = clamp(scale, UI_ZOOM_MIN, UI_ZOOM_MAX)
    _apply_ui_zoom()

func _apply_ui_zoom() -> void:
    var root := get_tree().root
    if root != null:
        root.content_scale_factor = ui_zoom
    if hud != null and hud.has_method("set_ui_zoom"):
        hud.call("set_ui_zoom", ui_zoom)

func _on_hud_zoom_delta(step: float) -> void:
    _adjust_ui_zoom(step * UI_ZOOM_STEP)

func _on_hud_zoom_reset() -> void:
    set_ui_zoom(1.0)

func _resolve_ui_zoom() -> float:
    var env_value: String = OS.get_environment("UI_ZOOM")
    if env_value != "":
        var parsed := env_value.to_float()
        if parsed > 0.0:
            return clamp(parsed, UI_ZOOM_MIN, UI_ZOOM_MAX)
    return 1.0

func _ensure_ui_zoom_actions() -> void:
    var zoom_actions := {
        "ui_zoom_in": KEY_EQUAL,
        "ui_zoom_out": KEY_MINUS,
        "ui_zoom_reset": KEY_0,
    }
    for action in zoom_actions.keys():
        if not InputMap.has_action(action):
            InputMap.add_action(action)
        var keycode: int = zoom_actions[action]
        var has_event := false
        for existing_event in InputMap.action_get_events(action):
            if existing_event is InputEventKey and existing_event.keycode == keycode:
                has_event = true
                break
        if not has_event:
            var key_event := InputEventKey.new()
            key_event.keycode = keycode
            key_event.physical_keycode = keycode
            InputMap.action_add_event(action, key_event)

func _determine_stream_enabled() -> bool:
    var env_flag: String = OS.get_environment("STREAM_ENABLED")
    if env_flag != "":
        return env_flag.to_lower() == "true"
    return STREAM_DEFAULT_ENABLED

func _determine_stream_host() -> String:
    var env_host: String = OS.get_environment("STREAM_HOST")
    if env_host != "":
        return env_host
    return STREAM_HOST

func _determine_stream_port() -> int:
    var env_port: String = OS.get_environment("STREAM_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    return STREAM_PORT

func _determine_command_host() -> String:
    var env_host: String = OS.get_environment("COMMAND_HOST")
    if env_host != "":
        return env_host
    return COMMAND_HOST

func _determine_command_port() -> int:
    var env_port: String = OS.get_environment("COMMAND_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    return COMMAND_PORT

func _determine_command_proto_port() -> int:
    var env_port: String = OS.get_environment("COMMAND_PROTO_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    return COMMAND_PROTO_PORT
