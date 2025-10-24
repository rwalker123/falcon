extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")
const CommandClient = preload("res://src/scripts/CommandClient.gd")
const Typography = preload("res://src/scripts/Typography.gd")
const ScriptHostManager = preload("res://src/scripts/scripting/ScriptHostManager.gd")

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
const SNAPSHOT_DELTA_FIELDS := [
    "influencer_updates",
    "population_updates",
    "tile_updates",
    "trade_link_updates",
    "influencer_removed",
    "population_removed"
]

func _ready() -> void:
    Typography.initialize()
    var ext: Resource = load("res://native/shadow_scale_godot.gdextension")
    if ext == null:
        push_warning("ShadowScale Godot extension not found; streaming disabled.")
    snapshot_loader = SnapshotLoader.new()
    snapshot_loader.load_mock_data(MOCK_DATA_PATH)
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
    command_client = CommandClient.new()
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
    if hud != null and hud.has_method("apply_typography"):
        hud.call("apply_typography")
    if inspector != null and inspector.has_method("apply_typography"):
        inspector.call("apply_typography")
    var initial: Dictionary = {}
    if streaming_mode and not snapshot_loader.last_stream_snapshot.is_empty():
        initial = snapshot_loader.last_stream_snapshot
    else:
        initial = snapshot_loader.current()
    _apply_snapshot(initial)
    if inspector != null and inspector.has_method("attach_map_view"):
        inspector.call("attach_map_view", map_view)
    if map_view != null and inspector != null and map_view.has_signal("hex_selected") and inspector.has_method("focus_tile_from_map"):
        map_view.connect("hex_selected", Callable(inspector, "focus_tile_from_map"))
    if inspector != null and inspector.has_method("set_streaming_active"):
        inspector.call("set_streaming_active", streaming_mode)

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
    var metrics_variant: Variant = map_view.call("display_snapshot", snapshot)
    var metrics: Dictionary = metrics_variant if metrics_variant is Dictionary else {}
    hud.call("update_overlay", snapshot.get("turn", 0), metrics)
    if hud != null and inspector != null:
        if inspector.has_method("get_resolved_font_size") and hud.has_method("set_inspector_font_size"):
            var resolved_size_variant: Variant = inspector.call("get_resolved_font_size")
            if typeof(resolved_size_variant) in [TYPE_INT, TYPE_FLOAT]:
                hud.call("set_inspector_font_size", int(resolved_size_variant))
    if inspector != null:
        if is_delta:
            if inspector.has_method("update_delta"):
                inspector.call("update_delta", snapshot)
        else:
            if inspector.has_method("update_snapshot"):
                inspector.call("update_snapshot", snapshot)
        if inspector.has_method("set_streaming_active"):
            inspector.call("set_streaming_active", streaming_mode)
    var legend_variant: Variant = map_view.call("terrain_palette_entries")
    if legend_variant is Array:
        hud.call("update_terrain_legend", legend_variant)
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

func skip_to_next_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.advance())

func skip_to_previous_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.rewind())

func _unhandled_input(event: InputEvent) -> void:
    if event.is_action_pressed("ui_right"):
        skip_to_next_turn()
    elif event.is_action_pressed("ui_left"):
        skip_to_previous_turn()
    elif event.is_action_pressed("ui_accept"):
        if map_view != null:
            map_view.call("toggle_terrain_mode")
    elif event is InputEventMouseButton:
        var mouse_event: InputEventMouseButton = event as InputEventMouseButton
        if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
            _adjust_camera_zoom(-CAMERA_ZOOM_STEP)
        elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
            _adjust_camera_zoom(CAMERA_ZOOM_STEP)

func _process(delta: float) -> void:
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

func _snapshot_is_delta(snapshot: Dictionary) -> bool:
    for field in SNAPSHOT_DELTA_FIELDS:
        if snapshot.has(field):
            return true
    return false

func _adjust_camera_zoom(delta_zoom: float) -> void:
    var new_zoom: float = clamp(camera.zoom.x + delta_zoom, CAMERA_ZOOM_MIN, CAMERA_ZOOM_MAX)
    camera.zoom = Vector2(new_zoom, new_zoom)

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
