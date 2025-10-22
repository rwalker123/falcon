extends CanvasLayer
class_name InspectorLayer

const LogStreamClientScript = preload("res://src/scripts/LogStreamClient.gd")

@onready var sentiment_text: RichTextLabel = $RootPanel/TabContainer/Sentiment/SentimentText
@onready var terrain_text: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainText
@onready var influencers_text: RichTextLabel = $RootPanel/TabContainer/Influencers/InfluencersText
@onready var corruption_text: RichTextLabel = $RootPanel/TabContainer/Corruption/CorruptionText
@onready var logs_text: RichTextLabel = $RootPanel/TabContainer/Logs/LogScroll/LogsText
@onready var log_status_label: Label = $RootPanel/TabContainer/Logs/SparklineContainer/SparklineStatusLabel
@onready var sparkline_graph: Control = $RootPanel/TabContainer/Logs/SparklineContainer/SparklineGraph
@onready var sparkline_stats_label: Label = $RootPanel/TabContainer/Logs/SparklineContainer/SparklineStatsLabel
@onready var root_panel: Panel = $RootPanel
@onready var tab_container: TabContainer = $RootPanel/TabContainer
@onready var command_status_label: Label = $RootPanel/TabContainer/Commands/StatusLabel
@onready var step_one_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/StepOneButton
@onready var step_ten_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/StepTenButton
@onready var rollback_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/RollbackButton
@onready var autoplay_toggle: CheckButton = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayToggle
@onready var autoplay_spin: SpinBox = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayIntervalSpin
@onready var autoplay_label: Label = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayIntervalLabel
@onready var command_log_text: RichTextLabel = $RootPanel/TabContainer/Commands/LogPanel/LogScroll/LogText
@onready var axis_dropdown: OptionButton = $RootPanel/TabContainer/Commands/AxisControls/AxisRow/AxisDropdown
@onready var axis_value_spin: SpinBox = $RootPanel/TabContainer/Commands/AxisControls/AxisRow/AxisValueSpin
@onready var axis_apply_button: Button = $RootPanel/TabContainer/Commands/AxisControls/AxisRow/AxisApplyButton
@onready var axis_reset_button: Button = $RootPanel/TabContainer/Commands/AxisControls/AxisRow/AxisResetButton
@onready var axis_reset_all_button: Button = $RootPanel/TabContainer/Commands/AxisControls/AxisResetAllButton
@onready var influencer_dropdown: OptionButton = $RootPanel/TabContainer/Commands/InfluencerControls/InfluencerRow/InfluencerDropdown
@onready var influencer_magnitude_spin: SpinBox = $RootPanel/TabContainer/Commands/InfluencerControls/InfluencerRow/InfluencerMagnitudeSpin
@onready var influencer_support_button: Button = $RootPanel/TabContainer/Commands/InfluencerControls/InfluencerRow/InfluencerSupportButton
@onready var influencer_suppress_button: Button = $RootPanel/TabContainer/Commands/InfluencerControls/InfluencerRow/InfluencerSuppressButton
@onready var channel_dropdown: OptionButton = $RootPanel/TabContainer/Commands/InfluencerControls/ChannelRow/ChannelDropdown
@onready var channel_magnitude_spin: SpinBox = $RootPanel/TabContainer/Commands/InfluencerControls/ChannelRow/ChannelMagnitudeSpin
@onready var channel_boost_button: Button = $RootPanel/TabContainer/Commands/InfluencerControls/ChannelRow/ChannelBoostButton
@onready var spawn_scope_dropdown: OptionButton = $RootPanel/TabContainer/Commands/InfluencerControls/SpawnRow/SpawnScopeDropdown
@onready var spawn_generation_spin: SpinBox = $RootPanel/TabContainer/Commands/InfluencerControls/SpawnRow/SpawnGenerationSpin
@onready var spawn_button: Button = $RootPanel/TabContainer/Commands/InfluencerControls/SpawnRow/SpawnButton
@onready var corruption_dropdown: OptionButton = $RootPanel/TabContainer/Commands/CorruptionControls/CorruptionRow/CorruptionSubsystemDropdown
@onready var corruption_intensity_spin: SpinBox = $RootPanel/TabContainer/Commands/CorruptionControls/CorruptionRow/CorruptionIntensitySpin
@onready var corruption_exposure_spin: SpinBox = $RootPanel/TabContainer/Commands/CorruptionControls/CorruptionRow/CorruptionExposureSpin
@onready var corruption_inject_button: Button = $RootPanel/TabContainer/Commands/CorruptionControls/CorruptionRow/CorruptionInjectButton
@onready var heat_entity_spin: SpinBox = $RootPanel/TabContainer/Commands/HeatControls/HeatRow/HeatEntitySpin
@onready var heat_delta_spin: SpinBox = $RootPanel/TabContainer/Commands/HeatControls/HeatRow/HeatDeltaSpin
@onready var heat_apply_button: Button = $RootPanel/TabContainer/Commands/HeatControls/HeatRow/HeatApplyButton

var _axis_bias: Dictionary = {}
var _sentiment: Dictionary = {}
var _influencers: Dictionary = {}
var _corruption: Dictionary = {}
var _terrain_palette: Dictionary = {}
var _terrain_tag_labels: Dictionary = {}
var _tile_records: Dictionary = {}
var _terrain_counts: Dictionary = {}
var _terrain_tag_counts: Dictionary = {}
var _tile_total: int = 0
var _log_messages: Array[String] = []
var _log_client: RefCounted = null
var _log_host: String = ""
var _log_port: int = 0
var _log_connected: bool = false
var _log_poll_timer: float = 0.0
var _log_retry_timer: float = 0.0
var _tick_samples: Array[Dictionary] = []
var _log_status_message: String = "Log stream offline."
var _last_turn: int = 0
var command_client: Object = null
var command_connected: bool = false
var stream_active: bool = false
var autoplay_timer: Timer
var command_log: Array[String] = []
const COMMAND_LOG_LIMIT = 40
const TERRAIN_TOP_LIMIT = 5
const TAG_TOP_LIMIT = 6
const LOG_ENTRY_LIMIT = 60
const LOG_HOST_DEFAULT = "127.0.0.1"
const LOG_PORT_DEFAULT = 41003
const LOG_POLL_INTERVAL = 0.1
const LOG_RECONNECT_INTERVAL = 2.0
const TICK_SAMPLE_LIMIT = 48
const DEFAULT_FONT_SIZE = 22
const MIN_FONT_SIZE = 12
const MAX_FONT_SIZE = 36
const PANEL_WIDTH_DEFAULT = 340.0
const PANEL_WIDTH_MIN = 260.0
const PANEL_WIDTH_MAX = 640.0
const PANEL_MARGIN = 16.0
const PANEL_TOP_OFFSET = 96.0
const PANEL_HANDLE_WIDTH = 12.0
const AXIS_NAMES: Array[String] = ["Knowledge", "Trust", "Equity", "Agency"]
const AXIS_KEYS: Array[String] = ["knowledge", "trust", "equity", "agency"]
const CHANNEL_OPTIONS = [
    {"label": "Popular", "key": "popular"},
    {"label": "Peer", "key": "peer"},
    {"label": "Institutional", "key": "institutional"},
    {"label": "Humanitarian", "key": "humanitarian"}
]
const SPAWN_SCOPE_OPTIONS = [
    {"label": "Auto", "key": null},
    {"label": "Local", "key": "local"},
    {"label": "Regional", "key": "regional"},
    {"label": "Global", "key": "global"},
    {"label": "Generation", "key": "generation"}
]
const CORRUPTION_OPTIONS = [
    {"label": "Logistics", "key": "logistics"},
    {"label": "Trade", "key": "trade"},
    {"label": "Military", "key": "military"},
    {"label": "Governance", "key": "governance"}
]

var _viewport: Viewport = null
var _panel_width: float = PANEL_WIDTH_DEFAULT
var _is_resizing = false

func _ready() -> void:
    set_process(true)
    _viewport = get_viewport()
    if _viewport != null:
        _viewport.size_changed.connect(_on_viewport_resized)
    if root_panel != null:
        root_panel.gui_input.connect(_on_root_panel_gui_input)
        root_panel.focus_mode = Control.FOCUS_CLICK
    _initialize_axis_controls()
    _initialize_influencer_controls()
    _initialize_corruption_controls()
    _initialize_heat_controls()
    _apply_theme_overrides()
    _update_panel_layout()
    _render_static_sections()
    _setup_command_controls()
    _initialize_log_channel()
    _render_logs()
    _update_tick_sparkline()

func _process(delta: float) -> void:
    _poll_log_stream(delta)

func update_snapshot(snapshot: Dictionary) -> void:
    _apply_update(snapshot, true)
    _render_dynamic_sections()

func update_delta(delta: Dictionary) -> void:
    _apply_update(delta, false)
    _render_dynamic_sections()

func _apply_update(data: Dictionary, full_snapshot: bool) -> void:
    if data.has("turn"):
        _last_turn = int(data.get("turn", _last_turn))

    if data.has("axis_bias"):
        var axis_dict: Dictionary = data["axis_bias"]
        _axis_bias = axis_dict.duplicate(true)
        _refresh_axis_controls()

    if data.has("sentiment"):
        var sentiment_dict: Dictionary = data["sentiment"]
        _sentiment = sentiment_dict.duplicate(true)

    if full_snapshot and data.has("influencers"):
        _rebuild_influencers(data["influencers"])
    elif data.has("influencer_updates"):
        _merge_influencers(data["influencer_updates"])

    if data.has("influencer_removed"):
        _remove_influencers(data["influencer_removed"])

    if data.has("corruption"):
        var ledger: Dictionary = data["corruption"]
        _corruption = ledger.duplicate(true)

    if data.has("overlays"):
        _ingest_overlays(data["overlays"])

    if full_snapshot and data.has("tiles"):
        _rebuild_tiles(data["tiles"])
    elif data.has("tile_updates"):
        _apply_tile_updates(data["tile_updates"])

    if data.has("tile_removed"):
        _remove_tiles(data["tile_removed"])

func _rebuild_influencers(array_data) -> void:
    _influencers.clear()
    for entry in array_data:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry.duplicate(true)
        var id = int(info.get("id", 0))
        _influencers[id] = info
    _refresh_influencer_dropdown()

func _merge_influencers(array_data) -> void:
    var changed = false
    for entry in array_data:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry.duplicate(true)
        var id = int(info.get("id", 0))
        _influencers[id] = info
        changed = true
    if changed:
        _refresh_influencer_dropdown()

func _remove_influencers(ids) -> void:
    for id in ids:
        _influencers.erase(int(id))
    _refresh_influencer_dropdown()

func _render_dynamic_sections() -> void:
    _render_sentiment()
    _render_influencers()
    _render_corruption()
    _render_terrain()
    _render_logs()

func _render_static_sections() -> void:
    _terrain_palette.clear()
    _terrain_tag_labels.clear()
    _tile_records.clear()
    _terrain_counts.clear()
    _terrain_tag_counts.clear()
    _tile_total = 0
    _log_messages.clear()
    _render_terrain()
    _render_logs()
    command_status_label.text = "Commands: disconnected."
    command_log_text.text = ""
    _panel_width = PANEL_WIDTH_DEFAULT
    _refresh_axis_controls()
    _refresh_influencer_dropdown()
    _update_command_controls_enabled()

func _apply_theme_overrides() -> void:
    var font_size = DEFAULT_FONT_SIZE
    var env_value = OS.get_environment("INSPECTOR_FONT_SIZE")
    if env_value != "":
        var parsed = int(env_value)
        if parsed >= MIN_FONT_SIZE and parsed <= MAX_FONT_SIZE:
            font_size = parsed
    _apply_font_override(sentiment_text, font_size)
    _apply_font_override(terrain_text, font_size)
    _apply_font_override(influencers_text, font_size)
    _apply_font_override(corruption_text, font_size)
    _apply_font_override(logs_text, font_size)
    _apply_font_override(command_status_label, font_size)
    _apply_font_override(step_one_button, font_size)
    _apply_font_override(step_ten_button, font_size)
    _apply_font_override(rollback_button, font_size)
    _apply_font_override(autoplay_toggle, font_size)
    _apply_font_override(autoplay_label, font_size)
    _apply_font_override(command_log_text, font_size)
    _apply_font_override(tab_container, font_size)
    _apply_font_override(autoplay_spin, font_size)

    if root_panel != null:
        var panel_style = StyleBoxFlat.new()
        panel_style.bg_color = Color(0.09, 0.09, 0.12, 0.97)
        panel_style.border_color = Color(0.2, 0.22, 0.26, 1.0)
        panel_style.border_width_top = 1
        panel_style.border_width_bottom = 1
        panel_style.border_width_left = 1
        panel_style.border_width_right = 1
        panel_style.corner_radius_top_left = 6
        panel_style.corner_radius_top_right = 6
        panel_style.corner_radius_bottom_left = 6
        panel_style.corner_radius_bottom_right = 6
        root_panel.add_theme_stylebox_override("panel", panel_style)
    if tab_container != null:
        var tab_style = StyleBoxFlat.new()
        tab_style.bg_color = Color(0.13, 0.13, 0.17, 0.99)
        tab_style.border_color = Color(0.22, 0.24, 0.28, 1.0)
        tab_style.border_width_top = 1
        tab_style.border_width_bottom = 0
        tab_style.border_width_left = 1
        tab_style.border_width_right = 1
        tab_style.corner_radius_top_left = 6
        tab_style.corner_radius_top_right = 6
        tab_style.corner_radius_bottom_left = 0
        tab_style.corner_radius_bottom_right = 0
        tab_container.add_theme_stylebox_override("panel", tab_style)
        tab_container.tab_alignment = 0

func _setup_command_controls() -> void:
    step_one_button.pressed.connect(_on_step_one_button_pressed)
    step_ten_button.pressed.connect(_on_step_ten_button_pressed)
    rollback_button.pressed.connect(_on_rollback_button_pressed)
    autoplay_toggle.toggled.connect(_on_autoplay_toggled)
    autoplay_spin.value_changed.connect(_on_autoplay_interval_changed)
    autoplay_spin.min_value = 0.2
    autoplay_spin.max_value = 5.0
    autoplay_spin.step = 0.1
    if autoplay_spin.value < 0.2:
        autoplay_spin.value = 0.5
    autoplay_toggle.button_pressed = false
    autoplay_timer = Timer.new()
    autoplay_timer.one_shot = false
    autoplay_timer.wait_time = float(autoplay_spin.value)
    add_child(autoplay_timer)
    autoplay_timer.timeout.connect(_on_autoplay_timeout)
    _update_command_status()
    _append_command_log("Command console ready.")

func _initialize_axis_controls() -> void:
    if axis_dropdown == null:
        return
    axis_dropdown.clear()
    for idx in range(AXIS_NAMES.size()):
        axis_dropdown.add_item(AXIS_NAMES[idx], idx)
    axis_dropdown.select(0)
    axis_dropdown.item_selected.connect(_on_axis_dropdown_selected)
    if axis_apply_button != null:
        axis_apply_button.pressed.connect(_on_axis_apply_button_pressed)
    if axis_reset_button != null:
        axis_reset_button.pressed.connect(_on_axis_reset_button_pressed)
    if axis_reset_all_button != null:
        axis_reset_all_button.pressed.connect(_on_axis_reset_all_button_pressed)
    if axis_value_spin != null:
        axis_value_spin.step = 0.01
        axis_value_spin.min_value = -1.0
        axis_value_spin.max_value = 1.0
        axis_value_spin.allow_greater = true
        axis_value_spin.allow_lesser = true
        axis_value_spin.value = 0.0
    _refresh_axis_controls()
    _update_command_controls_enabled()

func _initialize_influencer_controls() -> void:
    if influencer_support_button != null:
        influencer_support_button.pressed.connect(_on_influencer_support_button_pressed)
    if influencer_suppress_button != null:
        influencer_suppress_button.pressed.connect(_on_influencer_suppress_button_pressed)
    if channel_boost_button != null:
        channel_boost_button.pressed.connect(_on_channel_boost_button_pressed)
    if spawn_button != null:
        spawn_button.pressed.connect(_on_spawn_button_pressed)
    if influencer_dropdown != null:
        influencer_dropdown.clear()
        influencer_dropdown.disabled = true
        influencer_dropdown.item_selected.connect(_on_influencer_dropdown_selected)
    if channel_dropdown != null:
        channel_dropdown.clear()
        for option in CHANNEL_OPTIONS:
            var index = channel_dropdown.get_item_count()
            channel_dropdown.add_item(option["label"])
            channel_dropdown.set_item_metadata(index, option["key"])
        channel_dropdown.select(0)
    if spawn_scope_dropdown != null:
        spawn_scope_dropdown.clear()
        for option in SPAWN_SCOPE_OPTIONS:
            var index = spawn_scope_dropdown.get_item_count()
            spawn_scope_dropdown.add_item(option["label"])
            spawn_scope_dropdown.set_item_metadata(index, option["key"])
        spawn_scope_dropdown.select(0)
    if influencer_magnitude_spin != null:
        influencer_magnitude_spin.value = 1.0
    if channel_magnitude_spin != null:
        channel_magnitude_spin.value = 1.0
    if spawn_generation_spin != null:
        spawn_generation_spin.min_value = 0
        spawn_generation_spin.max_value = 65535
        spawn_generation_spin.step = 1
        spawn_generation_spin.value = 0
    _refresh_influencer_dropdown()
    _update_command_controls_enabled()

func _initialize_corruption_controls() -> void:
    if corruption_dropdown != null:
        corruption_dropdown.clear()
        for option in CORRUPTION_OPTIONS:
            var index = corruption_dropdown.get_item_count()
            corruption_dropdown.add_item(option["label"])
            corruption_dropdown.set_item_metadata(index, option["key"])
        corruption_dropdown.select(0)
    if corruption_intensity_spin != null:
        corruption_intensity_spin.value = 0.25
    if corruption_exposure_spin != null:
        corruption_exposure_spin.value = 3
    if corruption_inject_button != null:
        corruption_inject_button.pressed.connect(_on_corruption_inject_button_pressed)
    _update_command_controls_enabled()

func _initialize_heat_controls() -> void:
    if heat_entity_spin != null:
        heat_entity_spin.min_value = 0
        heat_entity_spin.max_value = 999999999
        heat_entity_spin.step = 1
    if heat_delta_spin != null:
        heat_delta_spin.min_value = -1000000
        heat_delta_spin.max_value = 1000000
        heat_delta_spin.step = 1000
        heat_delta_spin.value = 100000
    if heat_apply_button != null:
        heat_apply_button.pressed.connect(_on_heat_apply_button_pressed)
    _update_command_controls_enabled()

func set_command_client(client: Object, connected: bool) -> void:
    command_client = client
    var was_connected: bool = command_connected
    command_connected = connected and command_client != null and command_client.has_method("is_connection_active") and command_client.call("is_connection_active")
    _update_command_status()
    if command_connected and not was_connected:
        var host_value: String = "?"
        if command_client.has_method("get"):
            var host_variant = command_client.call("get", "host")
            if typeof(host_variant) == TYPE_STRING:
                host_value = host_variant
        var port_value: int = 0
        if command_client.has_method("get"):
            var port_variant = command_client.call("get", "port")
            if typeof(port_variant) in [TYPE_INT, TYPE_FLOAT]:
                port_value = int(port_variant)
        _append_command_log("Connected to command endpoint %s:%d." % [host_value, port_value])
    elif not command_connected and was_connected:
        _append_command_log("Command endpoint disconnected.")
    elif not command_connected and not was_connected:
        if command_client != null:
            var host_unavailable: String = "?"
            if command_client.has_method("get"):
                var host_unavailable_variant = command_client.call("get", "host")
                if typeof(host_unavailable_variant) == TYPE_STRING:
                    host_unavailable = host_unavailable_variant
            var port_unavailable: int = 0
            if command_client.has_method("get"):
                var port_unavailable_variant = command_client.call("get", "port")
                if typeof(port_unavailable_variant) in [TYPE_INT, TYPE_FLOAT]:
                    port_unavailable = int(port_unavailable_variant)
            _append_command_log("Command endpoint unavailable (%s:%d)." % [host_unavailable, port_unavailable])
        else:
            _append_command_log("Command endpoint unavailable.")

func set_streaming_active(active: bool) -> void:
    if stream_active == active:
        return
    stream_active = active
    if stream_active:
        _append_command_log("Streaming snapshots active.")
    else:
        _append_command_log("Streaming unavailable; using mock playback.")
        if autoplay_toggle.button_pressed:
            _disable_autoplay(true)
    _update_command_status()

func _update_command_status() -> void:
    var status_text: String = "Commands:"
    if command_client == null or not command_client.has_method("status"):
        status_text += " disabled."
        command_connected = false
    else:
        var st_variant = command_client.call("status")
        var st: int = st_variant if typeof(st_variant) == TYPE_INT else StreamPeerTCP.STATUS_NONE
        var host_value: String = "?"
        var port_value: int = 0
        if command_client.has_method("get"):
            var maybe_host = command_client.call("get", "host")
            var maybe_port = command_client.call("get", "port")
            if typeof(maybe_host) == TYPE_STRING:
                host_value = maybe_host
            if typeof(maybe_port) in [TYPE_INT, TYPE_FLOAT]:
                port_value = int(maybe_port)
        match st:
            StreamPeerTCP.STATUS_CONNECTED:
                status_text += " connected (%s:%d)." % [host_value, port_value]
                command_connected = true
            StreamPeerTCP.STATUS_CONNECTING:
                status_text += " connecting..."
                command_connected = false
            StreamPeerTCP.STATUS_ERROR:
                status_text += " error."
                command_connected = false
            _:
                status_text += " disconnected."
                command_connected = false
    if stream_active:
        status_text += " Streaming: active."
    else:
        status_text += " Streaming: paused."
    command_status_label.text = status_text
    _update_command_controls_enabled()

func _append_command_log(entry: String) -> void:
    command_log.append(entry)
    while command_log.size() > COMMAND_LOG_LIMIT:
        command_log.pop_front()
    command_log_text.text = "\n".join(command_log)
    if command_log_text.get_line_count() > 0:
        command_log_text.scroll_to_line(command_log_text.get_line_count() - 1)
    _append_log_entry("[CMD] %s" % entry)

func _update_command_controls_enabled() -> void:
    var connected = command_connected
    if axis_apply_button != null:
        axis_apply_button.disabled = not connected
    if axis_reset_button != null:
        axis_reset_button.disabled = not connected
    if axis_reset_all_button != null:
        axis_reset_all_button.disabled = not connected
    if axis_value_spin != null:
        axis_value_spin.editable = connected
    var has_influencer = _selected_influencer_id() != -1
    if influencer_support_button != null:
        influencer_support_button.disabled = not (connected and has_influencer)
    if influencer_suppress_button != null:
        influencer_suppress_button.disabled = not (connected and has_influencer)
    if influencer_magnitude_spin != null:
        influencer_magnitude_spin.editable = connected
    if channel_boost_button != null:
        var has_channel = channel_dropdown != null and channel_dropdown.get_item_count() > 0
        channel_boost_button.disabled = not (connected and has_influencer and has_channel)
    if channel_magnitude_spin != null:
        channel_magnitude_spin.editable = connected
    if spawn_button != null:
        spawn_button.disabled = not connected
    if spawn_generation_spin != null:
        spawn_generation_spin.editable = connected
    if corruption_inject_button != null:
        corruption_inject_button.disabled = not connected
    if corruption_intensity_spin != null:
        corruption_intensity_spin.editable = connected
    if corruption_exposure_spin != null:
        corruption_exposure_spin.editable = connected
    if heat_apply_button != null:
        heat_apply_button.disabled = not connected
    if heat_entity_spin != null:
        heat_entity_spin.editable = connected
    if heat_delta_spin != null:
        heat_delta_spin.editable = connected

func _ensure_command_connection() -> bool:
    if command_client == null:
        command_connected = false
        _update_command_status()
        return false
    if not command_client.has_method("ensure_connected"):
        command_connected = false
        _update_command_status()
        return false
    var ensure_err: Error = command_client.call("ensure_connected")
    match ensure_err:
        OK:
            command_connected = true
            _update_command_status()
            return true
        ERR_BUSY:
            command_connected = false
            _append_command_log("Command pending: command socket still connecting.")
            _update_command_status()
            return false
        _:
            command_connected = false
            _append_command_log("Command unavailable (%s)." % error_string(ensure_err))
            _update_command_status()
            return false

func _send_command(line: String, success_message: String) -> bool:
    if not _ensure_command_connection():
        return false
    var err: Error = command_client.call("send_line", line)
    if err == ERR_BUSY:
        command_client.call("poll")
        err = command_client.call("send_line", line)
    if err != OK:
        _append_command_log("Command failed (%s): %s" % [line, error_string(err)])
        _update_command_status()
        return false
    _append_command_log(success_message)
    _update_command_status()
    return true

func _send_turn(steps: int) -> bool:
    return _send_command("turn %d" % steps, "+%d turns requested." % steps)

func _on_step_one_button_pressed() -> void:
    _send_turn(1)

func _on_step_ten_button_pressed() -> void:
    _send_turn(10)

func _on_rollback_button_pressed() -> void:
    if _last_turn <= 0:
        _append_command_log("Rollback unavailable (turn 0).")
        return
    var target: int = max(_last_turn - 1, 0)
    _send_command("rollback %d" % target, "Rollback to turn %d requested." % target)

func _on_autoplay_toggled(pressed: bool) -> void:
    if pressed:
        if not _ensure_command_connection():
            autoplay_toggle.button_pressed = false
            _append_command_log("Auto-play requires an active command connection.")
            return
        autoplay_timer.wait_time = float(autoplay_spin.value)
        autoplay_timer.start()
        _append_command_log("Auto-play enabled (%.2fs)." % autoplay_timer.wait_time)
    else:
        _disable_autoplay(false)

func _on_autoplay_interval_changed(value: float) -> void:
    if autoplay_timer != null and not autoplay_timer.is_stopped():
        autoplay_timer.wait_time = value
        _append_command_log("Auto-play interval set to %.2fs." % value)

func _on_autoplay_timeout() -> void:
    if not _send_turn(1):
        _disable_autoplay(true)

func _disable_autoplay(log_message: bool) -> void:
    if autoplay_timer != null and not autoplay_timer.is_stopped():
        autoplay_timer.stop()
        if log_message:
            _append_command_log("Auto-play paused.")
    if autoplay_toggle.button_pressed:
        autoplay_toggle.button_pressed = false

func _render_sentiment() -> void:
    var lines: Array[String] = []
    lines.append("[b]Turn[/b] %d" % _last_turn)

    if not _axis_bias.is_empty():
        lines.append("[b]Axis Bias[/b]")
        for key in ["knowledge", "trust", "equity", "agency"]:
            var bias_value = float(_axis_bias.get(key, 0.0))
            lines.append(" • %s: %.3f" % [key.capitalize(), bias_value])

    if not _sentiment.is_empty():
        lines.append("")
        lines.append("[b]Axis Totals[/b]")
        for key in ["knowledge", "trust", "equity", "agency"]:
            if not _sentiment.has(key):
                continue
            var axis: Dictionary = _sentiment[key]
            var total = float(axis.get("total", 0.0))
            var policy = float(axis.get("policy", 0.0))
            var incidents = float(axis.get("incidents", 0.0))
            var influencer_val = float(axis.get("influencers", 0.0))
            lines.append(" • %s: %.3f (policy %.3f | incidents %.3f | influencers %.3f)"
                % [key.capitalize(), total, policy, incidents, influencer_val])

            var drivers = axis.get("drivers", [])
            var count = 0
            for driver in drivers:
                if count >= 3:
                    break
                if not (driver is Dictionary):
                    continue
                var driver_dict: Dictionary = driver
                var label: String = str(driver_dict.get("label", "Unnamed"))
                var category = str(driver_dict.get("category", ""))
                var value = float(driver_dict.get("value", 0.0))
                var weight = float(driver_dict.get("weight", 0.0))
                lines.append("    · [%s] %s: %.3f × %.3f" % [category, label, value, weight])
                count += 1

    sentiment_text.text = "\n".join(lines)

func _render_influencers() -> void:
    if _influencers.is_empty():
        influencers_text.text = "[b]Influencers[/b]\nNo roster data received yet."
        return

    var entries: Array = _influencers.values()
    entries.sort_custom(Callable(self, "_compare_influencers"))

    var lines: Array[String] = []
    lines.append("[b]Influencers[/b] (%d tracked)" % entries.size())
    var limit: int = min(entries.size(), 8)
    for index in range(limit):
        var info: Dictionary = entries[index]
        var id = int(info.get("id", 0))
        var name: String = str(info.get("name", "Unnamed"))
        var lifecycle = str(info.get("lifecycle", ""))
        var influence = float(info.get("influence", 0.0))
        var growth = float(info.get("growth_rate", 0.0))
        var support = float(info.get("support_charge", 0.0))
        var suppress = float(info.get("suppress_pressure", 0.0))
        lines.append("%d. %s [ID %d] — %s" % [index + 1, name, id, lifecycle])
        lines.append("    influence %.3f | growth %.3f | support %.3f | suppress %.3f"
            % [influence, growth, support, suppress])

        var domains_variant = info.get("domains")
        if domains_variant is PackedStringArray:
            var domain_str = _join_strings(domains_variant)
            if domain_str != "":
                lines.append("    domains: %s" % domain_str)

    influencers_text.text = "\n".join(lines)

func _compare_influencers(a: Dictionary, b: Dictionary) -> bool:
    var a_score = float(a.get("influence", 0.0))
    var b_score = float(b.get("influence", 0.0))
    return a_score > b_score

func _render_corruption() -> void:
    if _corruption.is_empty():
        corruption_text.text = "[b]Corruption[/b]\nNo ledger data received yet."
        return

    var lines: Array[String] = []
    lines.append("[b]Corruption[/b]")
    lines.append("Reputation modifier: %.3f" % float(_corruption.get("reputation_modifier", 0.0)))
    lines.append("Audit capacity: %d" % int(_corruption.get("audit_capacity", 0)))

    var entries = _corruption.get("entries", [])
    if entries.size() == 0:
        lines.append("No active incidents.")
    else:
        lines.append("Active incidents:")
        for entry in entries:
            if not (entry is Dictionary):
                continue
            var info: Dictionary = entry
            var subsystem = str(info.get("subsystem", "Unknown"))
            var intensity = float(info.get("intensity", 0.0))
            var timer = int(info.get("exposure_timer", 0))
            var last_tick = int(info.get("last_update_tick", 0))
            lines.append(" • %s: intensity %.3f | τ=%d | updated %d"
                % [subsystem, intensity, timer, last_tick])

    corruption_text.text = "\n".join(lines)

func _render_terrain() -> void:
    if _tile_total <= 0:
        terrain_text.text = """[b]Terrain Overlay[/b]
No terrain data received yet. Palette legend remains available on the HUD."""
        return

    var lines: Array[String] = []
    lines.append("[b]Terrain Overview[/b]")
    lines.append("Tracked tiles: %d" % _tile_total)

    var terrain_entries: Array[Dictionary] = []
    for key in _terrain_counts.keys():
        var terrain_id = int(key)
        var count = int(_terrain_counts[key])
        if count <= 0:
            continue
        var percent = (float(count) / float(max(_tile_total, 1))) * 100.0
        terrain_entries.append({
            "id": terrain_id,
            "count": count,
            "percent": percent,
            "label": _label_for_terrain(terrain_id)
        })
    terrain_entries.sort_custom(Callable(self, "_compare_terrain_entries"))

    var limit: int = min(terrain_entries.size(), TERRAIN_TOP_LIMIT)
    if limit > 0:
        lines.append("Top biomes:")
        for idx in range(limit):
            var entry: Dictionary = terrain_entries[idx]
            lines.append(" • %s (ID %d): %d tiles (%.1f%%)"
                % [entry.get("label", "Unknown"), int(entry.get("id", -1)), int(entry.get("count", 0)), float(entry.get("percent", 0.0))])

    var tag_entries: Array[Dictionary] = []
    for key in _terrain_tag_counts.keys():
        var mask = int(key)
        var count = int(_terrain_tag_counts[key])
        if count <= 0:
            continue
        var percent = (float(count) / float(max(_tile_total, 1))) * 100.0
        tag_entries.append({
            "mask": mask,
            "count": count,
            "percent": percent,
            "label": _label_for_tag(mask)
        })
    tag_entries.sort_custom(Callable(self, "_compare_tag_entries"))

    var tag_limit: int = min(tag_entries.size(), TAG_TOP_LIMIT)
    if tag_limit > 0:
        lines.append("")
        lines.append("Tag coverage:")
        for idx in range(tag_limit):
            var entry2: Dictionary = tag_entries[idx]
            lines.append(" • %s: %d tiles (%.1f%%)"
                % [entry2.get("label", "Tag"), int(entry2.get("count", 0)), float(entry2.get("percent", 0.0))])

    terrain_text.text = "\n".join(lines)

func _initialize_log_channel() -> void:
    _log_client = LogStreamClientScript.new()
    _log_host = _determine_log_host()
    _log_port = _determine_log_port()
    _log_connected = false
    _log_poll_timer = 0.0
    _log_retry_timer = 0.0
    _update_log_status("Connecting to log stream (%s:%d)..." % [_log_host, _log_port])
    var err: Error = ERR_UNAVAILABLE
    if _log_client != null and _log_client.has_method("connect_to"):
        err = _log_client.call("connect_to", _log_host, _log_port)
    if err != OK:
        _update_log_status("Log stream connection failed (%s)." % error_string(err))
        _log_retry_timer = LOG_RECONNECT_INTERVAL

func _determine_log_host() -> String:
    var env_host: String = OS.get_environment("LOG_HOST")
    if env_host != "":
        return env_host
    env_host = OS.get_environment("STREAM_HOST")
    if env_host != "":
        return env_host
    env_host = OS.get_environment("COMMAND_HOST")
    if env_host != "":
        return env_host
    return LOG_HOST_DEFAULT

func _determine_log_port() -> int:
    var env_port: String = OS.get_environment("LOG_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    return LOG_PORT_DEFAULT

func _poll_log_stream(delta: float) -> void:
    if _log_client == null:
        return
    _log_poll_timer += delta
    if _log_poll_timer < LOG_POLL_INTERVAL:
        return
    _log_poll_timer = 0.0
    if not _log_client.has_method("poll"):
        return
    var entries_variant: Variant = _log_client.call("poll")
    if typeof(entries_variant) != TYPE_ARRAY:
        entries_variant = []
    var entries: Array = entries_variant
    var status_code_variant: Variant = _log_client.call("status") if _log_client.has_method("status") else StreamPeerTCP.STATUS_NONE
    var status_code: int = int(status_code_variant)
    match status_code:
        StreamPeerTCP.STATUS_CONNECTING:
            var connecting_message: String = "Log stream connecting (%s:%d)..." % [_log_host, _log_port]
            if _log_status_message != connecting_message:
                _update_log_status(connecting_message)
            _log_connected = false
            return
        StreamPeerTCP.STATUS_CONNECTED:
            if not _log_connected:
                _update_log_status("Log stream connected (%s:%d)." % [_log_host, _log_port])
            _log_connected = true
            _log_retry_timer = 0.0
        _:
            if _log_connected:
                _update_log_status("Log stream disconnected; retrying...")
            _log_connected = false

    if not _log_connected:
        _log_retry_timer += LOG_POLL_INTERVAL
        if _log_retry_timer >= LOG_RECONNECT_INTERVAL:
            _log_retry_timer = 0.0
            var retry_err: Error = ERR_UNAVAILABLE
            if _log_client.has_method("connect_to"):
                retry_err = _log_client.call("connect_to", _log_host, _log_port)
            if retry_err != OK:
                _update_log_status("Log stream retry failed (%s)." % error_string(retry_err))
            else:
                _update_log_status("Reconnecting to log stream (%s:%d)..." % [_log_host, _log_port])
        return

    var updated: bool = false
    for entry in entries:
        if typeof(entry) != TYPE_DICTIONARY:
            continue
        _ingest_log_entry(entry)
        updated = true
    if updated:
        _update_tick_sparkline()

func _update_log_status(message: String) -> void:
    if _log_status_message == message:
        return
    _log_status_message = message
    if log_status_label != null:
        log_status_label.text = message
    _render_logs()

func _ingest_log_entry(entry: Dictionary) -> void:
    _record_tick_sample(entry)
    var formatted: String = _format_log_entry(entry)
    if formatted != "":
        _append_log_entry(formatted)

func _format_log_entry(entry: Dictionary) -> String:
    var level: String = String(entry.get("level", "INFO")).to_upper()
    var message: String = String(entry.get("message", ""))
    var timestamp_ms: int = int(entry.get("timestamp_ms", 0))
    var time_str: String = _format_timestamp(timestamp_ms)
    var suffix: String = ""
    var fields_variant: Variant = entry.get("fields", {})
    if typeof(fields_variant) == TYPE_DICTIONARY:
        var field_map: Dictionary = fields_variant
        var keys: Array = field_map.keys()
        keys.sort()
        var parts: Array[String] = []
        for key in keys:
            var key_str: String = String(key)
            parts.append("%s=%s" % [key_str, _stringify_field(key_str, field_map[key])])
        if not parts.is_empty():
            suffix = " " + ", ".join(parts)
    return "[%s] [%s] %s%s" % [time_str, level, message, suffix]

func _stringify_field(name: String, value) -> String:
    match typeof(value):
        TYPE_BOOL:
            return "true" if value else "false"
        TYPE_INT:
            return str(value)
        TYPE_FLOAT:
            if name == "duration_ms":
                return "%.1fms" % float(value)
            return "%.2f" % float(value)
        TYPE_STRING:
            return String(value)
        TYPE_ARRAY:
            return "[%d]" % (value as Array).size()
        TYPE_DICTIONARY:
            return "{...}"
        TYPE_NIL:
            return "null"
        _:
            return str(value)

func _format_timestamp(ms: int) -> String:
    if ms <= 0:
        return "--:--:--"
    var seconds: int = ms / 1000
    var millis: int = ms % 1000
    var datetime: Dictionary = Time.get_datetime_dict_from_unix_time(float(seconds))
    var hour: int = int(datetime.get("hour", 0))
    var minute: int = int(datetime.get("minute", 0))
    var second: int = int(datetime.get("second", 0))
    return "%02d:%02d:%02d.%03d" % [hour, minute, second, millis]

func _record_tick_sample(entry: Dictionary) -> void:
    var fields_variant: Variant = entry.get("fields", {})
    if typeof(fields_variant) != TYPE_DICTIONARY:
        return
    var fields: Dictionary = fields_variant
    var turn_id: int = int(fields.get("turn", -1))
    var duration_val: float = float(fields.get("duration_ms", 0.0))
    if duration_val <= 0.0:
        return
    var sample := {
        "turn": turn_id,
        "duration_ms": duration_val
    }
    _tick_samples.append(sample)
    while _tick_samples.size() > TICK_SAMPLE_LIMIT:
        _tick_samples.pop_front()

func _update_tick_sparkline() -> void:
    if sparkline_graph == null:
        return
    if _tick_samples.is_empty():
        if sparkline_graph.has_method("clear_samples"):
            sparkline_graph.call("clear_samples")
        if sparkline_stats_label != null:
            sparkline_stats_label.text = "Awaiting telemetry."
        return
    var durations: Array = []
    var total: float = 0.0
    for sample in _tick_samples:
        var value: float = float(sample.get("duration_ms", 0.0))
        durations.append(value)
        total += value
    if sparkline_graph.has_method("set_samples"):
        sparkline_graph.call("set_samples", durations)
    var latest: Dictionary = _tick_samples[_tick_samples.size() - 1]
    var turn_id: int = int(latest.get("turn", -1))
    var last_duration: float = float(latest.get("duration_ms", 0.0))
    var avg_duration: float = total / max(durations.size(), 1)
    if sparkline_stats_label != null:
        sparkline_stats_label.text = "Turn %d: %.1f ms (avg %.1f ms over %d turns)" % [
            turn_id,
            last_duration,
            avg_duration,
            durations.size()
        ]

func _render_logs() -> void:
    if logs_text == null:
        return
    var lines: Array[String] = []
    lines.append("[b]Logs[/b]")
    if _log_status_message != "":
        lines.append("[color=#a4c6ff]%s[/color]" % _log_status_message)
    if _log_messages.is_empty():
        lines.append("No log entries yet.")
    else:
        for entry in _log_messages:
            lines.append(entry)
    logs_text.text = "\n".join(lines)
    if logs_text.get_line_count() > 0:
        logs_text.scroll_to_line(logs_text.get_line_count() - 1)

func _apply_font_override(control: Control, size: int) -> void:
    if control == null:
        return
    if control is RichTextLabel:
        var rich: RichTextLabel = control
        rich.add_theme_font_size_override("normal_font_size", size)
        rich.add_theme_font_size_override("bold_font_size", size)
        rich.add_theme_font_size_override("italics_font_size", size)
        rich.add_theme_font_size_override("mono_font_size", max(size - 1, MIN_FONT_SIZE))
    else:
        control.add_theme_font_size_override("font_size", size)

func _update_panel_layout() -> void:
    if root_panel == null:
        return
    _panel_width = clamp(_panel_width, PANEL_WIDTH_MIN, _max_panel_width())
    root_panel.offset_left = PANEL_MARGIN
    root_panel.offset_right = PANEL_MARGIN + _panel_width
    root_panel.offset_top = PANEL_TOP_OFFSET
    root_panel.offset_bottom = -PANEL_MARGIN
    root_panel.custom_minimum_size = Vector2(PANEL_WIDTH_MIN, 0)

func _on_viewport_resized() -> void:
    _update_panel_layout()

func _max_panel_width() -> float:
    var viewport_size = get_viewport().get_visible_rect().size
    var max_allowed = viewport_size.x - (PANEL_MARGIN * 2.0 + 120.0)
    return clamp(max_allowed, PANEL_WIDTH_MIN, PANEL_WIDTH_MAX)

func _is_in_resize_region(local_pos: Vector2) -> bool:
    return root_panel != null and local_pos.x >= (root_panel.size.x - PANEL_HANDLE_WIDTH)

func _on_root_panel_gui_input(event: InputEvent) -> void:
    if event is InputEventMouseButton:
        var mouse_event = event as InputEventMouseButton
        if mouse_event.button_index == MOUSE_BUTTON_LEFT:
            if mouse_event.pressed and _is_in_resize_region(mouse_event.position):
                _is_resizing = true
                root_panel.mouse_default_cursor_shape = Control.CURSOR_HSIZE
                root_panel.grab_focus()
                root_panel.accept_event()
            elif not mouse_event.pressed and _is_resizing:
                _is_resizing = false
                root_panel.mouse_default_cursor_shape = Control.CURSOR_ARROW
                root_panel.accept_event()
    elif event is InputEventMouseMotion:
        var motion = event as InputEventMouseMotion
        if _is_resizing:
            _panel_width = clamp(_panel_width + motion.relative.x, PANEL_WIDTH_MIN, _max_panel_width())
            _update_panel_layout()
            root_panel.accept_event()
        else:
            if _is_in_resize_region(motion.position):
                root_panel.mouse_default_cursor_shape = Control.CURSOR_HSIZE
            else:
                root_panel.mouse_default_cursor_shape = Control.CURSOR_ARROW

func _join_strings(values: PackedStringArray) -> String:
    var parts: Array[String] = []
    for value in values:
        parts.append(String(value))
    var result = ""
    for i in range(parts.size()):
        result += parts[i]
        if i < parts.size() - 1:
            result += ", "
    return result

func _compare_terrain_entries(a: Dictionary, b: Dictionary) -> bool:
    var a_count = int(a.get("count", 0))
    var b_count = int(b.get("count", 0))
    if a_count == b_count:
        return int(a.get("id", -1)) < int(b.get("id", -1))
    return a_count > b_count

func _compare_tag_entries(a: Dictionary, b: Dictionary) -> bool:
    var a_count = int(a.get("count", 0))
    var b_count = int(b.get("count", 0))
    if a_count == b_count:
        return int(a.get("mask", 0)) < int(b.get("mask", 0))
    return a_count > b_count

func _label_for_terrain(terrain_id: int) -> String:
    if _terrain_palette.has(terrain_id):
        return str(_terrain_palette[terrain_id])
    for key in _terrain_palette.keys():
        if int(key) == terrain_id:
            return str(_terrain_palette[key])
    return "Terrain %d" % terrain_id

func _label_for_tag(mask: int) -> String:
    if _terrain_tag_labels.has(mask):
        return str(_terrain_tag_labels[mask])
    for key in _terrain_tag_labels.keys():
        if int(key) == mask:
            return str(_terrain_tag_labels[key])
    return "Tag %d" % mask

func _ingest_overlays(overlays: Variant) -> void:
    if not (overlays is Dictionary):
        return
    var overlay_dict: Dictionary = overlays
    if overlay_dict.has("terrain_palette"):
        var palette_variant: Variant = overlay_dict["terrain_palette"]
        if palette_variant is Dictionary:
            _terrain_palette = (palette_variant as Dictionary).duplicate(true)
    if overlay_dict.has("terrain_tag_labels"):
        var tag_variant: Variant = overlay_dict["terrain_tag_labels"]
        if tag_variant is Dictionary:
            _terrain_tag_labels = (tag_variant as Dictionary).duplicate(true)

func _rebuild_tiles(tile_entries) -> void:
    _tile_records.clear()
    _terrain_counts.clear()
    _terrain_tag_counts.clear()
    _tile_total = 0
    if tile_entries is Array:
        for entry in tile_entries:
            _store_tile(entry)
    _tile_total = _tile_records.size()

func _apply_tile_updates(tile_entries) -> void:
    if not (tile_entries is Array):
        return
    for entry in tile_entries:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry
        var entity = int(info.get("entity", -1))
        if entity >= 0:
            _forget_tile(entity)
        _store_tile(info)
    _tile_total = _tile_records.size()

func _remove_tiles(ids) -> void:
    if ids is Array:
        for id_value in ids:
            _forget_tile(int(id_value))
    elif ids is PackedInt64Array:
        var packed: PackedInt64Array = ids
        for idx in packed:
            _forget_tile(int(idx))
    elif ids is PackedInt32Array:
        var packed32: PackedInt32Array = ids
        for idx in packed32:
            _forget_tile(int(idx))
    _tile_total = max(_tile_records.size(), 0)

func _store_tile(entry) -> void:
    if not (entry is Dictionary):
        return
    var info: Dictionary = entry
    var entity = int(info.get("entity", -1))
    if entity < 0:
        return
    var terrain_id = int(info.get("terrain", -1))
    var tags_mask = int(info.get("terrain_tags", 0))
    var record = {
        "terrain": terrain_id,
        "tags": tags_mask
    }
    _tile_records[entity] = record
    _tile_total = max(_tile_records.size(), _tile_total + 1)
    _bump_terrain_count(terrain_id, 1)
    _bump_tag_counts(tags_mask, 1)

func _forget_tile(entity_id: int) -> void:
    if not _tile_records.has(entity_id):
        return
    var record: Dictionary = _tile_records[entity_id]
    var terrain_id = int(record.get("terrain", -1))
    var tags_mask = int(record.get("tags", 0))
    _bump_terrain_count(terrain_id, -1)
    _bump_tag_counts(tags_mask, -1)
    _tile_records.erase(entity_id)
    _tile_total = max(_tile_records.size(), _tile_total - 1)

func _bump_terrain_count(terrain_id: int, delta: int) -> void:
    if terrain_id < 0 or delta == 0:
        return
    var current = int(_terrain_counts.get(terrain_id, 0)) + delta
    if current <= 0:
        _terrain_counts.erase(terrain_id)
    else:
        _terrain_counts[terrain_id] = current

func _bump_tag_counts(mask: int, delta: int) -> void:
    if mask == 0 or delta == 0:
        return
    var remaining = mask
    while remaining != 0:
        var bit = remaining & -remaining
        if bit <= 0:
            break
        if delta > 0 and not _terrain_tag_labels.has(bit):
            _terrain_tag_labels[bit] = "Tag %d" % bit
        var current = int(_terrain_tag_counts.get(bit, 0)) + delta
        if current <= 0:
            _terrain_tag_counts.erase(bit)
        else:
            _terrain_tag_counts[bit] = current
        remaining &= remaining - 1

func _selected_axis_index() -> int:
    if axis_dropdown == null:
        return -1
    var selected_id = axis_dropdown.get_selected_id()
    if selected_id != -1:
        return int(selected_id)
    var idx = axis_dropdown.get_selected()
    if idx >= 0 and idx < axis_dropdown.get_item_count():
        return int(axis_dropdown.get_item_id(idx))
    return -1

func _refresh_axis_controls() -> void:
    if axis_dropdown == null or axis_value_spin == null:
        return
    if axis_dropdown.get_item_count() == 0:
        _initialize_axis_controls()
    var axis_idx = _selected_axis_index()
    if axis_idx < 0:
        axis_dropdown.select(0)
        axis_idx = _selected_axis_index()
    _update_axis_spin_value(axis_idx)

func _update_axis_spin_value(axis_idx: int) -> void:
    if axis_value_spin == null:
        return
    if axis_idx < 0 or axis_idx >= AXIS_KEYS.size():
        axis_value_spin.value = 0.0
        return
    var key: String = String(AXIS_KEYS[axis_idx])
    var value: float = 0.0
    if _axis_bias.has(key):
        value = float(_axis_bias.get(key, 0.0))
    axis_value_spin.value = clamp(value, axis_value_spin.min_value, axis_value_spin.max_value)

func _send_axis_bias(axis_idx: int, value: float) -> bool:
    if axis_idx < 0 or axis_idx >= AXIS_NAMES.size():
        _append_command_log("Invalid axis selection.")
        return false
    var clamped: float = clamp(value, -1.0, 1.0)
    var message: String = "Axis %s set to %.3f" % [AXIS_NAMES[axis_idx], clamped]
    if _send_command("bias %d %.6f" % [axis_idx, clamped], message):
        var key: String = String(AXIS_KEYS[axis_idx])
        _axis_bias[key] = clamped
        _update_axis_spin_value(axis_idx)
        return true
    return false

func _on_axis_dropdown_selected(_index: int) -> void:
    _update_axis_spin_value(_selected_axis_index())

func _on_axis_apply_button_pressed() -> void:
    var axis_idx = _selected_axis_index()
    if axis_idx < 0:
        _append_command_log("Select an axis before applying bias.")
        return
    _send_axis_bias(axis_idx, float(axis_value_spin.value))

func _on_axis_reset_button_pressed() -> void:
    var axis_idx = _selected_axis_index()
    if axis_idx < 0:
        _append_command_log("Select an axis before resetting bias.")
        return
    axis_value_spin.value = 0.0
    _send_axis_bias(axis_idx, 0.0)

func _on_axis_reset_all_button_pressed() -> void:
    for idx in range(AXIS_NAMES.size()):
        _send_axis_bias(idx, 0.0)

func _selected_influencer_id() -> int:
    if influencer_dropdown == null or influencer_dropdown.get_item_count() == 0:
        return -1
    var selected_id = influencer_dropdown.get_selected_id()
    if selected_id != -1:
        return int(selected_id)
    var idx = influencer_dropdown.get_selected()
    if idx >= 0 and idx < influencer_dropdown.get_item_count():
        return int(influencer_dropdown.get_item_id(idx))
    return -1

func _influencer_display_name(id: int) -> String:
    var info = _influencers.get(id, null)
    if info == null:
        return "ID %d" % id
    var name: String = str(info.get("name", "Influencer %d" % id))
    return name if name.strip_edges() != "" else "ID %d" % id

func _refresh_influencer_dropdown() -> void:
    if influencer_dropdown == null:
        return
    var previous_id: int = _selected_influencer_id()
    var entries: Array = []
    for key in _influencers.keys():
        var id = int(key)
        var name: String = _influencer_display_name(id)
        var entry = {
            "id": id,
            "label": "%s (ID %d)" % [name, id]
        }
        entries.append(entry)
    entries.sort_custom(Callable(self, "_compare_influencer_option"))
    influencer_dropdown.clear()
    if entries.is_empty():
        influencer_dropdown.disabled = true
    else:
        influencer_dropdown.disabled = false
        var selected_index: int = 0
        for idx in range(entries.size()):
            var entry: Dictionary = entries[idx]
            var label: String = entry["label"]
            var entry_id: int = entry["id"]
            influencer_dropdown.add_item(label, entry_id)
            if entry_id == previous_id:
                selected_index = idx
        influencer_dropdown.select(selected_index)
    _update_command_controls_enabled()

func _compare_influencer_option(a: Dictionary, b: Dictionary) -> bool:
    var a_label: String = String(a.get("label", ""))
    var b_label: String = String(b.get("label", ""))
    return a_label < b_label

func _on_influencer_dropdown_selected(_index: int) -> void:
    _update_command_controls_enabled()

func _on_influencer_support_button_pressed() -> void:
    var id = _selected_influencer_id()
    if id < 0:
        _append_command_log("Select an influencer before sending support.")
        return
    var magnitude: float = max(float(influencer_magnitude_spin.value), 0.0)
    var name: String = _influencer_display_name(id)
    _send_command("support %d %.3f" % [id, magnitude], "Support +%.2f sent to %s" % [magnitude, name])

func _on_influencer_suppress_button_pressed() -> void:
    var id = _selected_influencer_id()
    if id < 0:
        _append_command_log("Select an influencer before sending suppress.")
        return
    var magnitude: float = max(float(influencer_magnitude_spin.value), 0.0)
    var name: String = _influencer_display_name(id)
    _send_command("suppress %d %.3f" % [id, magnitude], "Suppress −%.2f sent to %s" % [magnitude, name])

func _on_channel_boost_button_pressed() -> void:
    var id = _selected_influencer_id()
    if id < 0:
        _append_command_log("Select an influencer before applying channel boost.")
        return
    if channel_dropdown == null or channel_dropdown.get_item_count() == 0:
        _append_command_log("No channel options configured.")
        return
    var channel_index: int = channel_dropdown.get_selected()
    if channel_index < 0:
        channel_index = 0
    var channel_key_variant: Variant = channel_dropdown.get_item_metadata(channel_index)
    var channel_key: String = String(channel_key_variant) if typeof(channel_key_variant) == TYPE_STRING else "popular"
    var magnitude: float = max(float(channel_magnitude_spin.value), 0.0)
    var name: String = _influencer_display_name(id)
    var channel_label: String = String(channel_dropdown.get_item_text(channel_index))
    _send_command(
        "support_channel %d %s %.3f" % [id, channel_key, magnitude],
        "Channel boost (%s, +%.2f) sent to %s" % [channel_label, magnitude, name]
    )

func _on_spawn_button_pressed() -> void:
    var scope_key: Variant = null
    if spawn_scope_dropdown != null and spawn_scope_dropdown.get_item_count() > 0:
        var scope_index: int = spawn_scope_dropdown.get_selected()
        scope_key = spawn_scope_dropdown.get_item_metadata(scope_index)
    var generation_id: int = int(spawn_generation_spin.value) if spawn_generation_spin != null else 0
    var line: String
    var message: String
    if scope_key == null:
        if generation_id > 0:
            line = "spawn_influencer %d" % generation_id
            message = "Spawn influencer from generation %d requested." % generation_id
        else:
            line = "spawn_influencer"
            message = "Spawn influencer requested."
    else:
        var scope_text: String = String(scope_key)
        match scope_text:
            "generation":
                if generation_id <= 0:
                    _append_command_log("Specify a generation ID when spawning by generation.")
                    return
                line = "spawn_influencer generation %d" % generation_id
                message = "Spawn influencer (generation %d) requested." % generation_id
            _:
                line = "spawn_influencer %s" % scope_text
                message = "Spawn influencer (%s) requested." % scope_text.capitalize()
    _send_command(line, message)

func _on_corruption_inject_button_pressed() -> void:
    if corruption_dropdown == null:
        return
    var idx: int = corruption_dropdown.get_selected()
    if idx < 0 and corruption_dropdown.get_item_count() > 0:
        idx = 0
    var key_variant: Variant = corruption_dropdown.get_item_metadata(idx)
    var key: String = String(key_variant) if typeof(key_variant) == TYPE_STRING else "logistics"
    var label: String = corruption_dropdown.get_item_text(idx)
    var intensity: float = float(corruption_intensity_spin.value)
    var exposure: int = int(corruption_exposure_spin.value)
    var line: String = "corruption %s %.3f %d" % [key, intensity, exposure]
    var message: String = "Corruption (%s, %.2f, τ=%d) requested." % [label, intensity, exposure]
    _send_command(line, message)

func _on_heat_apply_button_pressed() -> void:
    var entity_id: int = int(heat_entity_spin.value) if heat_entity_spin != null else 0
    var delta: int = int(heat_delta_spin.value) if heat_delta_spin != null else 0
    if entity_id <= 0:
        _append_command_log("Heat command requires a valid entity id.")
        return
    var line: String = "heat %d %d" % [entity_id, delta]
    var message: String = "Heat delta %d applied to entity %d." % [delta, entity_id]
    _send_command(line, message)

func _append_log_entry(entry: String) -> void:
    var trimmed: String = entry.strip_edges(true, true)
    if trimmed == "":
        return
    _log_messages.append(trimmed)
    while _log_messages.size() > LOG_ENTRY_LIMIT:
        _log_messages.pop_front()
    _render_logs()
