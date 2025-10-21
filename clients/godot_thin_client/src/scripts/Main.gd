extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")

@onready var map_view: Node2D = $MapLayer
@onready var hud: CanvasLayer = $HUD
@onready var camera: Camera2D = $Camera2D

var snapshot_loader: SnapshotLoader
var playback_timer: Timer
var streaming_mode := false
var stream_connection_timer := 0.0

const MOCK_DATA_PATH := "res://src/data/mock_snapshots.json"
const TURN_INTERVAL_SECONDS := 1.5
const STREAM_DEFAULT_ENABLED := false
const STREAM_HOST := "127.0.0.1"
const STREAM_PORT := 41002
const STREAM_CONNECTION_TIMEOUT := 5.0
const CAMERA_PAN_SPEED := 220.0
const CAMERA_ZOOM_STEP := 0.1
const CAMERA_ZOOM_MIN := 0.5
const CAMERA_ZOOM_MAX := 1.5

func _ready() -> void:
    var ext := load("res://native/shadow_scale_godot.gdextension")
    if ext == null:
        push_warning("ShadowScale Godot extension not found; streaming disabled.")
    snapshot_loader = SnapshotLoader.new()
    snapshot_loader.load_mock_data(MOCK_DATA_PATH)
    var stream_enabled := _determine_stream_enabled()
    var stream_host := _determine_stream_host()
    var stream_port := _determine_stream_port()
    if stream_enabled:
        var err := snapshot_loader.enable_stream(stream_host, stream_port)
        if err == OK:
            streaming_mode = true
        else:
            push_warning("Godot client: unable to connect to snapshot stream (error %d). Using mock data." % err)
    if streaming_mode:
        set_process(true)
    else:
        _ensure_timer()
    var initial: Dictionary = {}
    if streaming_mode and not snapshot_loader.last_stream_snapshot.is_empty():
        initial = snapshot_loader.last_stream_snapshot
    else:
        initial = snapshot_loader.current()
    _apply_snapshot(initial)

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
    var metrics_variant: Variant = map_view.call("display_snapshot", snapshot)
    var metrics: Dictionary = metrics_variant if metrics_variant is Dictionary else {}
    hud.call("update_overlay", snapshot.get("turn", 0), metrics)
    var center_variant: Variant = map_view.call("get_world_center")
    if center_variant is Vector2:
        camera.position = center_variant

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
    if streaming_mode:
        var streamed := snapshot_loader.poll_stream(delta)
        if not streamed.is_empty():
            _apply_snapshot(streamed)
            stream_connection_timer = 0.0
        else:
            if not snapshot_loader.is_streaming():
                stream_connection_timer += delta
                if stream_connection_timer > STREAM_CONNECTION_TIMEOUT:
                    push_warning("Godot client: snapshot stream unavailable; falling back to mock playback.")
                    streaming_mode = false
                    snapshot_loader.disable_stream()
                    _ensure_timer()
                    stream_connection_timer = 0.0
    var pan_input: Vector2 = Vector2(
        Input.get_action_strength("ui_right") - Input.get_action_strength("ui_left"),
        Input.get_action_strength("ui_down") - Input.get_action_strength("ui_up")
    )
    if pan_input != Vector2.ZERO:
        camera.position += pan_input * CAMERA_PAN_SPEED * delta

func _adjust_camera_zoom(delta_zoom: float) -> void:
    var new_zoom: float = clamp(camera.zoom.x + delta_zoom, CAMERA_ZOOM_MIN, CAMERA_ZOOM_MAX)
    camera.zoom = Vector2(new_zoom, new_zoom)

func _determine_stream_enabled() -> bool:
    var env_flag := OS.get_environment("STREAM_ENABLED")
    if env_flag != "":
        return env_flag.to_lower() == "true"
    return STREAM_DEFAULT_ENABLED

func _determine_stream_host() -> String:
    var env_host := OS.get_environment("STREAM_HOST")
    if env_host != "":
        return env_host
    return STREAM_HOST

func _determine_stream_port() -> int:
    var env_port := OS.get_environment("STREAM_PORT")
    if env_port != "":
        var parsed := int(env_port)
        if parsed > 0:
            return parsed
    return STREAM_PORT
