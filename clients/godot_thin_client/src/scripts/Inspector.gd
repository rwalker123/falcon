extends CanvasLayer
class_name InspectorLayer

const LogStreamClientScript = preload("res://src/scripts/LogStreamClient.gd")

@onready var sentiment_text: RichTextLabel = $RootPanel/TabContainer/Sentiment/SentimentText
@onready var terrain_text: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainVBox/TerrainText
@onready var terrain_biome_section_label: Label = $RootPanel/TabContainer/Terrain/TerrainVBox/BiomeSection/BiomeSectionLabel
@onready var terrain_biome_list: ItemList = $RootPanel/TabContainer/Terrain/TerrainVBox/BiomeSection/BiomeList
@onready var terrain_biome_detail_text: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainVBox/BiomeSection/BiomeDetailText
@onready var terrain_tile_section_label: Label = $RootPanel/TabContainer/Terrain/TerrainVBox/TileSection/TileSectionLabel
@onready var terrain_tile_list: ItemList = $RootPanel/TabContainer/Terrain/TerrainVBox/TileSection/TileList
@onready var terrain_tile_detail_text: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainVBox/TileSection/TileDetailText
@onready var terrain_overlay_section_label: Label = $RootPanel/TabContainer/Terrain/TerrainVBox/OverlaySection/OverlaySectionLabel
@onready var terrain_overlay_tabs: TabContainer = $RootPanel/TabContainer/Terrain/TerrainVBox/OverlaySection/OverlayTabs
@onready var terrain_overlay_culture_placeholder: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainVBox/OverlaySection/OverlayTabs/Culture/CulturePlaceholder
@onready var terrain_overlay_military_placeholder: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainVBox/OverlaySection/OverlayTabs/Military/MilitaryPlaceholder
@onready var culture_summary_text: RichTextLabel = $RootPanel/TabContainer/Culture/CultureVBox/CultureSummarySection/CultureSummaryText
@onready var culture_divergence_list: ItemList = $RootPanel/TabContainer/Culture/CultureVBox/CultureDivergenceSection/CultureDivergenceList
@onready var culture_divergence_detail: RichTextLabel = $RootPanel/TabContainer/Culture/CultureVBox/CultureDivergenceSection/CultureDivergenceDetail
@onready var culture_tension_text: RichTextLabel = $RootPanel/TabContainer/Culture/CultureVBox/CultureTensionSection/CultureTensionText
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
var _biome_entries: Array[Dictionary] = []
var _biome_tile_lookup: Dictionary = {}
var _biome_index_lookup: Dictionary = {}
var _selected_biome_id: int = -1
var _selected_tile_entity: int = -1
var _hovered_tile_entity: int = -1
var _tile_coord_lookup: Dictionary = {}
var _selected_culture_layer_id: int = -1
var _culture_layers: Dictionary = {}
var _culture_tensions: Array[Dictionary] = []
var _culture_tension_tracker: Dictionary = {}
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
const TERRAIN_TILE_DISPLAY_LIMIT = 24
const TERRAIN_BIOME_SAMPLE_LIMIT = 6
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
const CULTURE_TOP_TRAIT_LIMIT = 6
const CULTURE_MAX_DIVERGENCES = 6
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
    _connect_terrain_ui()
    _connect_culture_ui()
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

    if full_snapshot and data.has("culture_layers"):
        _rebuild_culture_layers(data["culture_layers"])
    elif data.has("culture_layer_updates"):
        _apply_culture_layer_updates(data["culture_layer_updates"])

    if data.has("culture_layer_removed"):
        _remove_culture_layers(data["culture_layer_removed"])

    if data.has("culture_tensions"):
        _update_culture_tensions(data["culture_tensions"], full_snapshot)

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
    _render_culture()
    _render_logs()

func _render_static_sections() -> void:
    _terrain_palette.clear()
    _terrain_tag_labels.clear()
    _tile_records.clear()
    _terrain_counts.clear()
    _terrain_tag_counts.clear()
    _tile_total = 0
    _culture_layers.clear()
    _culture_tensions.clear()
    _culture_tension_tracker.clear()
    _selected_culture_layer_id = -1
    _clear_terrain_ui()
    _log_messages.clear()
    _render_terrain()
    _render_culture()
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
    _apply_font_override(terrain_biome_section_label, font_size)
    _apply_font_override(terrain_biome_list, font_size)
    _apply_font_override(terrain_biome_detail_text, font_size)
    _apply_font_override(terrain_tile_section_label, font_size)
    _apply_font_override(terrain_tile_list, font_size)
    _apply_font_override(terrain_tile_detail_text, font_size)
    _apply_font_override(terrain_overlay_section_label, font_size)
    _apply_font_override(terrain_overlay_tabs, font_size)
    _apply_font_override(terrain_overlay_culture_placeholder, font_size)
    _apply_font_override(terrain_overlay_military_placeholder, font_size)
    _apply_font_override(culture_summary_text, font_size)
    _apply_font_override(culture_divergence_list, font_size)
    _apply_font_override(culture_divergence_detail, font_size)
    _apply_font_override(culture_tension_text, font_size)

    if root_panel != null:
        var panel_style = StyleBoxFlat.new()
        panel_style.bg_color = Color(0.09, 0.09, 0.12, 0.6)
        panel_style.border_color = Color(0.2, 0.22, 0.26, 0.6)
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
        tab_style.bg_color = Color(0.13, 0.13, 0.17, 0.6)
        tab_style.border_color = Color(0.22, 0.24, 0.28, 0.6)
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

func _connect_terrain_ui() -> void:
    if terrain_biome_list != null:
        var biome_selected_callable = Callable(self, "_on_terrain_biome_selected")
        if not terrain_biome_list.is_connected("item_selected", biome_selected_callable):
            terrain_biome_list.item_selected.connect(_on_terrain_biome_selected)
        if not terrain_biome_list.is_connected("item_activated", biome_selected_callable):
            terrain_biome_list.item_activated.connect(_on_terrain_biome_selected)
    if terrain_tile_list != null:
        var tile_selected_callable = Callable(self, "_on_terrain_tile_selected")
        if not terrain_tile_list.is_connected("item_selected", tile_selected_callable):
            terrain_tile_list.item_selected.connect(_on_terrain_tile_selected)
        if not terrain_tile_list.is_connected("item_activated", tile_selected_callable):
            terrain_tile_list.item_activated.connect(_on_terrain_tile_selected)
        var tile_gui_callable = Callable(self, "_on_terrain_tile_gui_input")
        if not terrain_tile_list.is_connected("gui_input", tile_gui_callable):
            terrain_tile_list.gui_input.connect(_on_terrain_tile_gui_input)

func _connect_culture_ui() -> void:
    if culture_divergence_list != null:
        var divergence_callable = Callable(self, "_on_culture_divergence_selected")
        if not culture_divergence_list.is_connected("item_selected", divergence_callable):
            culture_divergence_list.item_selected.connect(_on_culture_divergence_selected)
        if not culture_divergence_list.is_connected("item_activated", divergence_callable):
            culture_divergence_list.item_activated.connect(_on_culture_divergence_selected)

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
    if terrain_text == null:
        return

    if _tile_total <= 0:
        terrain_text.text = """[b]Terrain Overlay[/b]
No terrain data received yet. Palette legend remains available on the HUD."""
        _clear_terrain_ui()
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
    _refresh_biome_section(terrain_entries)

func _render_culture() -> void:
    if culture_summary_text == null or culture_divergence_list == null or culture_tension_text == null:
        return

    if _culture_layers.is_empty():
        culture_summary_text.text = "[b]Culture[/b]\n[i]No culture data received yet.[/i]"
        culture_divergence_list.clear()
        if culture_divergence_detail != null:
            culture_divergence_detail.text = "[i]Awaiting regional or local layers.[/i]"
        culture_tension_text.text = "[i]No active tensions.[/i]"
        return

    var global_layer := {}
    for value in _culture_layers.values():
        if not (value is Dictionary):
            continue
        var scope := str(value.get("scope", ""))
        if scope == "Global":
            global_layer = value
            break
    var summary_lines: Array[String] = []
    summary_lines.append("[b]Global Identity[/b]")
    if global_layer.is_empty():
        summary_lines.append("[i]Global layer missing.[/i]")
    else:
        var traits: Array[Dictionary] = _extract_culture_traits(global_layer)
        traits.sort_custom(Callable(self, "_compare_trait_strength"))
        var limit: int = min(traits.size(), CULTURE_TOP_TRAIT_LIMIT)
        if limit == 0:
            summary_lines.append("[i]No trait telemetry available.[/i]")
        else:
            for idx in range(limit):
                var atrait: Dictionary = Dictionary()
                if idx < traits.size():
                    var candidate_trait: Variant = traits[idx]
                    if candidate_trait is Dictionary:
                        atrait = candidate_trait as Dictionary
                    else:
                        continue
                var label: String = str(atrait.get("label", atrait.get("axis", "Trait")))
                var value: float = float(atrait.get("value", 0.0))
                var modifier: float = float(atrait.get("modifier", 0.0))
                summary_lines.append("%d. %s: %+.2f (modifier %+.2f)" % [idx + 1, label, value, modifier])
    culture_summary_text.text = "\n".join(summary_lines)

    var divergence_entries: Array[Dictionary] = []
    for key in _culture_layers.keys():
        var layer_variant: Variant = _culture_layers[key]
        if not (layer_variant is Dictionary):
            continue
        var layer: Dictionary = layer_variant as Dictionary
        var scope_str := str(layer.get("scope", ""))
        if scope_str == "Global":
            continue
        var magnitude: float = float(layer.get("divergence", 0.0))
        divergence_entries.append({
            "layer": layer,
            "magnitude": absf(magnitude),
            "value": magnitude
        })
    divergence_entries.sort_custom(Callable(self, "_compare_culture_divergences"))

    var previous_selection: int = _selected_culture_layer_id
    culture_divergence_list.clear()
    var selection_index: int = -1
    var divergence_limit: int = min(divergence_entries.size(), CULTURE_MAX_DIVERGENCES)
    for idx in range(divergence_limit):
        var entry: Dictionary = divergence_entries[idx]
        var layer_dict: Dictionary = {}
        var layer_entry: Variant = entry.get("layer", {})
        if layer_entry is Dictionary:
            layer_dict = layer_entry as Dictionary
        var divergence_label := _format_culture_divergence_entry(layer_dict, float(entry.get("value", 0.0)))
        var item_index := culture_divergence_list.add_item(divergence_label)
        culture_divergence_list.set_item_metadata(item_index, layer_dict)
        if int(layer_dict.get("id", -1)) == previous_selection:
            selection_index = item_index
    if selection_index >= 0:
        culture_divergence_list.select(selection_index)
    elif culture_divergence_list.get_item_count() > 0:
        culture_divergence_list.select(0)
        var first_meta: Variant = culture_divergence_list.get_item_metadata(0)
        if first_meta is Dictionary:
            _selected_culture_layer_id = int((first_meta as Dictionary).get("id", -1))
    else:
        _selected_culture_layer_id = -1
    _update_culture_divergence_detail()

    var tension_lines: Array[String] = []
    if _culture_tensions.is_empty():
        tension_lines.append("[i]No active tensions.[/i]")
    else:
        for tension in _culture_tensions:
            if not (tension is Dictionary):
                continue
            var info: Dictionary = tension as Dictionary
            var kind_label: String = str(info.get("kind_label", info.get("kind", "Tension")))
            var scope_label: String = str(info.get("scope_label", info.get("scope", "")))
            var severity: float = float(info.get("severity", 0.0))
            var timer_val: int = int(info.get("timer", 0))
            var layer_id: int = int(info.get("layer_id", 0))
            tension_lines.append("• %s — layer #%03d [%s] | Δ %.2f | timer %d" % [
                kind_label,
                layer_id,
                scope_label,
                severity,
                timer_val
            ])
    culture_tension_text.text = "\n".join(tension_lines)

func _update_culture_divergence_detail() -> void:
    if culture_divergence_detail == null:
        return
    var selected_items := culture_divergence_list.get_selected_items()
    if selected_items.is_empty():
        culture_divergence_detail.text = "[i]Select a regional or local layer to inspect divergence.[/i]"
        return
    var index: int = selected_items[0]
    var meta: Variant = culture_divergence_list.get_item_metadata(index)
    if not (meta is Dictionary):
        culture_divergence_detail.text = "[i]Select a regional or local layer to inspect divergence.[/i]"
        return
    var layer: Dictionary = meta as Dictionary
    _selected_culture_layer_id = int(layer.get("id", -1))
    var lines: Array[String] = []
    var scope_label: String = str(layer.get("scope_label", layer.get("scope", "")))
    var owner_variant: Variant = layer.get("owner")
    if owner_variant == null:
        owner_variant = layer.get("owner_value", 0)
    var owner_display: String = _format_owner_display(owner_variant)
    var parent_id: int = int(layer.get("parent", 0))
    var divergence_val: float = float(layer.get("divergence", 0.0))
    var soft_threshold: float = float(layer.get("soft_threshold", 0.0))
    var hard_threshold: float = float(layer.get("hard_threshold", 0.0))
    var ticks_soft: int = int(layer.get("ticks_above_soft", 0))
    var ticks_hard: int = int(layer.get("ticks_above_hard", 0))
    lines.append("[b]Layer #%03d · %s[/b]" % [int(layer.get("id", 0)), scope_label])
    lines.append("Owner: %s | Parent: %d" % [owner_display, parent_id])
    lines.append("Δ %+.2f | soft %.2f | hard %.2f" % [divergence_val, soft_threshold, hard_threshold])
    lines.append("Ticks above soft: %d | hard: %d" % [ticks_soft, ticks_hard])
    lines.append("")
    lines.append("[b]Top Trait Drift[/b]")
    var traits: Array[Dictionary] = _extract_culture_traits(layer)
    traits.sort_custom(Callable(self, "_compare_trait_strength"))
    var limit: int = min(traits.size(), CULTURE_TOP_TRAIT_LIMIT)
    if limit == 0:
        lines.append("(no trait telemetry)")
    else:
        for idx in range(limit):
            var atrait: Dictionary = Dictionary()
            if idx < traits.size():
                var candidate_trait: Variant = traits[idx]
                if candidate_trait is Dictionary:
                    atrait = candidate_trait as Dictionary
                else:
                    continue
            var label: String = str(atrait.get("label", atrait.get("axis", "Trait")))
            var value: float = float(atrait.get("value", 0.0))
            var baseline: float = float(atrait.get("baseline", 0.0))
            var modifier: float = float(atrait.get("modifier", 0.0))
            lines.append("%d. %s: value %+.2f | baseline %+.2f | modifier %+.2f" % [
                idx + 1,
                label,
                value,
                baseline,
                modifier
            ])
    culture_divergence_detail.text = "\n".join(lines)

func _extract_culture_traits(layer: Dictionary) -> Array[Dictionary]:
    var result: Array[Dictionary] = []
    var traits_variant = layer.get("traits", [])
    if traits_variant is Array:
        for trait_entry in traits_variant:
            if not (trait_entry is Dictionary):
                continue
            result.append((trait_entry as Dictionary).duplicate(true))
    return result

func _format_culture_divergence_entry(layer: Dictionary, divergence: float) -> String:
    var layer_id: int = int(layer.get("id", 0))
    var scope_label: String = str(layer.get("scope_label", layer.get("scope", "")))
    return "#%03d [%s] Δ %+.2f" % [layer_id, scope_label, divergence]

func _compare_culture_divergences(a: Dictionary, b: Dictionary) -> bool:
    var a_mag: float = float(a.get("magnitude", 0.0))
    var b_mag: float = float(b.get("magnitude", 0.0))
    if absf(a_mag - b_mag) > 0.0001:
        return a_mag > b_mag
    return float(a.get("value", 0.0)) > float(b.get("value", 0.0))

func _compare_trait_strength(a: Dictionary, b: Dictionary) -> bool:
    var a_val: float = absf(float(a.get("value", 0.0)))
    var b_val: float = absf(float(b.get("value", 0.0)))
    if absf(a_val - b_val) > 0.0001:
        return a_val > b_val
    return absf(float(a.get("modifier", 0.0))) > absf(float(b.get("modifier", 0.0)))

func _format_owner_display(owner_variant: Variant) -> String:
    match typeof(owner_variant):
        TYPE_INT, TYPE_FLOAT:
            var numeric: int = int(owner_variant)
            return "0x%016x" % numeric
        TYPE_STRING:
            return String(owner_variant)
        TYPE_NIL:
            return "n/a"
        _:
            return str(owner_variant)

func _on_culture_divergence_selected(index: int) -> void:
    if culture_divergence_list == null:
        return
    var meta: Variant = culture_divergence_list.get_item_metadata(index)
    if meta is Dictionary:
        _selected_culture_layer_id = int((meta as Dictionary).get("id", -1))
    else:
        _selected_culture_layer_id = -1
    _update_culture_divergence_detail()

func _clear_terrain_ui() -> void:
    _biome_entries.clear()
    _biome_tile_lookup.clear()
    _biome_index_lookup.clear()
    _tile_coord_lookup.clear()
    _selected_biome_id = -1
    _selected_tile_entity = -1
    _hovered_tile_entity = -1
    if terrain_biome_list != null:
        terrain_biome_list.clear()
    if terrain_biome_detail_text != null:
        terrain_biome_detail_text.text = """[b]Biome Drill-down[/b]
Select a biome once terrain data is available."""
    if terrain_tile_list != null:
        terrain_tile_list.clear()
    if terrain_tile_detail_text != null:
        terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
Hover or select a tile to inspect biome tags and conditions."""

func _refresh_biome_section(entries: Array[Dictionary]) -> void:
    _biome_entries = entries.duplicate(true)
    _build_biome_tile_lookup()
    _biome_index_lookup.clear()
    for idx in range(_biome_entries.size()):
        var entry: Dictionary = _biome_entries[idx]
        var biome_id: int = int(entry.get("id", -1))
        _biome_index_lookup[biome_id] = idx
    _update_biome_list()

func _build_biome_tile_lookup() -> void:
    var lookup: Dictionary = {}
    for key in _tile_records.keys():
        var entity_id: int = int(key)
        var record_variant: Variant = _tile_records[key]
        if not (record_variant is Dictionary):
            continue
        var record: Dictionary = record_variant
        var terrain_id: int = int(record.get("terrain", -1))
        if terrain_id < 0:
            continue
        var tile_list: Array = []
        if lookup.has(terrain_id):
            tile_list = lookup[terrain_id]
        tile_list.append(entity_id)
        lookup[terrain_id] = tile_list
    _biome_tile_lookup = lookup

func _format_biome_list_entry(entry: Dictionary) -> String:
    var label: String = str(entry.get("label", "Biome"))
    var count: int = int(entry.get("count", 0))
    var percent: float = float(entry.get("percent", 0.0))
    return "%s – %d tiles (%.1f%%)" % [label, count, percent]

func _update_biome_list() -> void:
    if terrain_biome_list == null:
        return
    var previous_biome: int = _selected_biome_id
    terrain_biome_list.clear()
    var selection_index: int = -1
    for idx in range(_biome_entries.size()):
        var entry: Dictionary = _biome_entries[idx]
        terrain_biome_list.add_item(_format_biome_list_entry(entry))
        terrain_biome_list.set_item_metadata(idx, entry)
        if int(entry.get("id", -1)) == previous_biome:
            selection_index = idx
    var force_tile_reset: bool = false
    if selection_index >= 0:
        terrain_biome_list.select(selection_index)
    elif _biome_entries.size() > 0:
        selection_index = 0
        terrain_biome_list.select(selection_index)
        force_tile_reset = true
    else:
        _selected_biome_id = -1
        _render_selected_biome(true)
        return
    var selected_entry: Dictionary = _biome_entries[selection_index]
    var new_biome_id: int = int(selected_entry.get("id", -1))
    var selection_changed: bool = previous_biome != new_biome_id
    _selected_biome_id = new_biome_id
    _render_selected_biome(force_tile_reset or selection_changed)

func _render_selected_biome(reset_tile_selection: bool, pinned_tile_entity: int = -1) -> void:
    if terrain_biome_list == null:
        return
    var selected_items: PackedInt32Array = terrain_biome_list.get_selected_items()
    if selected_items.is_empty():
        _selected_biome_id = -1
        if terrain_biome_detail_text != null:
            terrain_biome_detail_text.text = """[b]Biome Drill-down[/b]
Select a biome to view tag breakdowns and representative tiles."""
        _refresh_tile_list(true, pinned_tile_entity)
        return
    var index: int = selected_items[0]
    var entry_variant: Variant = terrain_biome_list.get_item_metadata(index)
    var entry: Dictionary = entry_variant if entry_variant is Dictionary else {}
    if entry.is_empty() and index < _biome_entries.size():
        entry = _biome_entries[index]
    var biome_id: int = int(entry.get("id", -1))
    var label: String = str(entry.get("label", "Biome"))
    var count: int = int(entry.get("count", 0))
    var percent: float = float(entry.get("percent", 0.0))
    _selected_biome_id = biome_id

    if terrain_biome_detail_text != null:
        var lines: Array[String] = []
        lines.append("[b]%s[/b]" % label)
        lines.append("Tile coverage: %d (%.1f%% of tracked terrain)" % [count, percent])
        var tile_list: Array = _get_biome_tiles(biome_id)
        lines.append("Tracked tiles in biome: %d" % tile_list.size())
        var tag_summary: Array[Dictionary] = _summarize_biome_tags(biome_id)
        if tag_summary.is_empty():
            lines.append("Tag breakdown: none")
        else:
            lines.append("Tag breakdown:")
            var tag_limit: int = min(tag_summary.size(), TAG_TOP_LIMIT)
            for tag_idx in range(tag_limit):
                var tag_entry: Dictionary = tag_summary[tag_idx]
                lines.append(" • %s: %d tiles (%.1f%%)" % [
                    tag_entry.get("label", "Tag"),
                    int(tag_entry.get("count", 0)),
                    float(tag_entry.get("percent", 0.0))
                ])
        var sample_lines: Array[String] = _format_representative_tiles(biome_id)
        lines.append("")
        if sample_lines.is_empty():
            lines.append("Representative tiles: none recorded.")
        else:
            lines.append("Representative tiles:")
            for sample_line in sample_lines:
                lines.append(sample_line)
        terrain_biome_detail_text.text = "\n".join(lines)

    _refresh_tile_list(reset_tile_selection, pinned_tile_entity)

func _summarize_biome_tags(biome_id: int) -> Array[Dictionary]:
    var tile_list: Array = _get_biome_tiles(biome_id)
    if tile_list.is_empty():
        return []
    var counts: Dictionary = {}
    for entity_id in tile_list:
        var record_variant: Variant = _tile_records.get(entity_id, {})
        if not (record_variant is Dictionary):
            continue
        var record: Dictionary = record_variant
        var mask: int = int(record.get("tags", 0))
        if mask == 0:
            continue
        for bit in range(0, 16):
            var bit_value: int = 1 << bit
            if (mask & bit_value) == 0:
                continue
            counts[bit_value] = int(counts.get(bit_value, 0)) + 1
    var result: Array[Dictionary] = []
    var total: float = float(max(tile_list.size(), 1))
    for key in counts.keys():
        var bit_mask: int = int(key)
        var count: int = int(counts[key])
        result.append({
            "mask": bit_mask,
            "count": count,
            "percent": (float(count) / total) * 100.0,
            "label": _label_for_tag(bit_mask)
        })
    result.sort_custom(Callable(self, "_compare_tag_entries"))
    return result

func _get_biome_tiles(biome_id: int) -> Array:
    if biome_id < 0:
        return []
    if not _biome_tile_lookup.has(biome_id):
        return []
    var stored: Variant = _biome_tile_lookup[biome_id]
    if stored is Array:
        return (stored as Array).duplicate()
    return []

func _format_representative_tiles(biome_id: int) -> Array[String]:
    var tile_list: Array = _get_biome_tiles(biome_id)
    if tile_list.is_empty():
        return []
    tile_list.sort()
    var sample_limit: int = min(tile_list.size(), TERRAIN_BIOME_SAMPLE_LIMIT)
    var result: Array[String] = []
    for idx in range(sample_limit):
        var entity_id: int = int(tile_list[idx])
        var record_variant: Variant = _tile_records.get(entity_id, {})
        if not (record_variant is Dictionary):
            continue
        var record: Dictionary = record_variant
        var coords_text: String = _format_tile_coords(record)
        var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
        var tags_text: String = "none"
        if not tags.is_empty():
            tags_text = _join_strings_with_separator(tags, ", ")
        var temperature: float = float(record.get("temperature", 0.0))
        var mass: float = float(record.get("mass", 0.0))
        result.append(" • %s | entity %d | tags: %s | temp %.1f | mass %.1f" % [
            coords_text,
            entity_id,
            tags_text,
            temperature,
            mass
        ])
    return result

func _refresh_tile_list(reset_tile_selection: bool, pinned_entity: int = -1) -> void:
    if terrain_tile_list == null:
        return
    var previous_tile: int = _selected_tile_entity
    terrain_tile_list.clear()
    var tile_entities: Array = _get_biome_tiles(_selected_biome_id)
    tile_entities.sort()
    var display_limit: int = min(tile_entities.size(), TERRAIN_TILE_DISPLAY_LIMIT)
    var display_entities: Array = []
    for idx in range(display_limit):
        display_entities.append(int(tile_entities[idx]))
    if pinned_entity >= 0 and tile_entities.has(pinned_entity) and display_entities.find(pinned_entity) == -1:
        display_entities.append(pinned_entity)

    var selected_index: int = -1
    for idx in range(display_entities.size()):
        var entity_id: int = int(display_entities[idx])
        var record_variant: Variant = _tile_records.get(entity_id, {})
        if not (record_variant is Dictionary):
            continue
        var record: Dictionary = record_variant
        terrain_tile_list.add_item(_format_tile_list_entry(entity_id, record))
        var new_index: int = terrain_tile_list.get_item_count() - 1
        terrain_tile_list.set_item_metadata(new_index, entity_id)
        if entity_id == pinned_entity:
            selected_index = new_index
        elif entity_id == previous_tile and selected_index == -1:
            selected_index = new_index

    if terrain_tile_list.get_item_count() == 0:
        _selected_tile_entity = -1
        _render_tile_detail(-1)
        return

    var effective_previous: int = previous_tile
    if pinned_entity >= 0:
        effective_previous = pinned_entity

    var should_reset_tile: bool = reset_tile_selection or effective_previous < 0 or tile_entities.find(effective_previous) == -1
    var target_index: int = selected_index

    if target_index < 0:
        if not should_reset_tile:
            for idx in range(terrain_tile_list.get_item_count()):
                var entity_candidate: int = int(terrain_tile_list.get_item_metadata(idx))
                if entity_candidate == effective_previous:
                    target_index = idx
                    break
        if target_index < 0:
            target_index = 0

    var target_entity: int = int(terrain_tile_list.get_item_metadata(target_index))
    _selected_tile_entity = target_entity
    terrain_tile_list.select(target_index)
    _hovered_tile_entity = -1
    _render_tile_detail(target_entity)

func _format_tile_list_entry(entity_id: int, record: Dictionary) -> String:
    var coords_text: String = _format_tile_coords(record)
    var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
    var preview_tags: Array[String] = []
    var preview_limit: int = min(tags.size(), 2)
    for idx in range(preview_limit):
        preview_tags.append(tags[idx])
    var parts: Array[String] = []
    parts.append(coords_text)
    parts.append("entity %d" % entity_id)
    if not preview_tags.is_empty():
        parts.append(_join_strings_with_separator(preview_tags, ", "))
    return _join_strings_with_separator(parts, " • ")

func _format_tile_coords(record: Dictionary) -> String:
    var x: int = int(record.get("x", -1))
    var y: int = int(record.get("y", -1))
    return "@%d,%d" % [x, y]

func _render_tile_detail(entity_id: int, preview: bool = false) -> void:
    if terrain_tile_detail_text == null:
        return
    if entity_id < 0 or not _tile_records.has(entity_id):
        terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
Hover or select a tile to inspect biome tags and conditions."""
        return
    var record_variant: Variant = _tile_records.get(entity_id, {})
    if not (record_variant is Dictionary):
        terrain_tile_detail_text.text = "No data for tile %d." % entity_id
        return
    var record: Dictionary = record_variant
    var lines: Array[String] = []
    lines.append("[b]Tile %d[/b]" % entity_id)
    lines.append("Location: %s" % _format_tile_coords(record))
    lines.append("Biome: %s" % _label_for_terrain(int(record.get("terrain", -1))))
    var tags: Array[String] = _tag_labels_from_mask(int(record.get("tags", 0)))
    var tags_text: String = "none"
    if not tags.is_empty():
            tags_text = _join_strings_with_separator(tags, ", ")
    lines.append("Tags: %s" % tags_text)
    lines.append("Temperature: %.1f" % float(record.get("temperature", 0.0)))
    lines.append("Mass: %.1f" % float(record.get("mass", 0.0)))
    lines.append("Element ID: %d" % int(record.get("element", -1)))
    if preview:
        lines.append("")
        lines.append("[i]Hover preview[/i]")
    terrain_tile_detail_text.text = "\n".join(lines)

func _tag_labels_from_mask(mask: int) -> Array[String]:
    var labels: Array[String] = []
    if mask == 0:
        return labels
    for bit in range(0, 16):
        var bit_value: int = 1 << bit
        if (mask & bit_value) != 0:
            labels.append(_label_for_tag(bit_value))
    return labels

func focus_tile_from_map(col: int, row: int, terrain_id: int) -> void:
    if terrain_biome_list == null:
        return
    var coord := Vector2i(col, row)
    var entity_id: int = -1
    if _tile_coord_lookup.has(coord):
        entity_id = int(_tile_coord_lookup[coord])
    else:
        for key in _tile_records.keys():
            var record_variant: Variant = _tile_records[key]
            if not (record_variant is Dictionary):
                continue
            var record: Dictionary = record_variant
            if int(record.get("x", -1)) == col and int(record.get("y", -1)) == row:
                entity_id = int(key)
                _tile_coord_lookup[coord] = entity_id
                break

    if terrain_id >= 0 and _biome_entries.size() > 0:
        var biome_index: int = int(_biome_index_lookup.get(terrain_id, -1))
        if biome_index >= 0:
            var previous_biome: int = _selected_biome_id
            var reset_required: bool = previous_biome != terrain_id
            terrain_biome_list.select(biome_index, false)
            var selected_indices: PackedInt32Array = terrain_biome_list.get_selected_items()
            if selected_indices.is_empty() or selected_indices[0] != biome_index:
                terrain_biome_list.select(biome_index, false)
            _selected_biome_id = terrain_id
            _render_selected_biome(reset_required, entity_id)
        else:
            _render_selected_biome(false, entity_id)
    else:
        _render_selected_biome(false, entity_id)

    if entity_id < 0 and _selected_tile_entity < 0 and terrain_tile_detail_text != null:
        terrain_tile_detail_text.text = """[b]Tile Inspection[/b]
No detailed data available for the selected tile (%d, %d).""" % [col, row]

func _on_terrain_biome_selected(index: int) -> void:
    if terrain_biome_list == null:
        return
    if index < 0 or index >= terrain_biome_list.get_item_count():
        return
    var metadata: Variant = terrain_biome_list.get_item_metadata(index)
    var biome_id: int = -1
    if metadata is Dictionary:
        var entry: Dictionary = metadata
        biome_id = int(entry.get("id", -1))
    elif index < _biome_entries.size():
        biome_id = int(_biome_entries[index].get("id", -1))
    var reset_tiles: bool = biome_id != _selected_biome_id
    _selected_biome_id = biome_id
    _render_selected_biome(reset_tiles)

func _on_terrain_tile_selected(index: int) -> void:
    if terrain_tile_list == null:
        return
    if index < 0 or index >= terrain_tile_list.get_item_count():
        return
    var metadata: Variant = terrain_tile_list.get_item_metadata(index)
    var entity_id: int = int(metadata)
    _selected_tile_entity = entity_id
    _hovered_tile_entity = -1
    _render_tile_detail(entity_id)

func _on_terrain_tile_gui_input(event: InputEvent) -> void:
    if terrain_tile_list == null or event == null:
        return
    if event is InputEventMouseMotion:
        var motion: InputEventMouseMotion = event
        var hovered_index: int = terrain_tile_list.get_item_at_position(motion.position, true)
        if hovered_index < 0:
            if _hovered_tile_entity != -1:
                _hovered_tile_entity = -1
                if _selected_tile_entity >= 0:
                    _render_tile_detail(_selected_tile_entity)
            return
        if hovered_index >= terrain_tile_list.get_item_count():
            return
        var metadata: Variant = terrain_tile_list.get_item_metadata(hovered_index)
        var entity_id: int = int(metadata)
        if entity_id == _selected_tile_entity:
            if _hovered_tile_entity != -1:
                _hovered_tile_entity = -1
                _render_tile_detail(_selected_tile_entity)
            return
        if entity_id == _hovered_tile_entity:
            return
        _hovered_tile_entity = entity_id
        _render_tile_detail(entity_id, true)

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

func _join_strings_with_separator(values: Array[String], separator: String) -> String:
    var result: String = ""
    for idx in range(values.size()):
        result += String(values[idx])
        if idx < values.size() - 1:
            result += separator
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
    _tile_coord_lookup.clear()
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

func _rebuild_culture_layers(array_data) -> void:
    _culture_layers.clear()
    if array_data is Array:
        for entry in array_data:
            var layer_dict: Dictionary = _normalize_culture_layer(entry)
            if layer_dict.is_empty():
                continue
            var id = int(layer_dict.get("id", 0))
            _culture_layers[id] = layer_dict
    _selected_culture_layer_id = -1

func _apply_culture_layer_updates(array_data) -> void:
    if not (array_data is Array):
        return
    for entry in array_data:
        var layer_dict: Dictionary = _normalize_culture_layer(entry)
        if layer_dict.is_empty():
            continue
        var id = int(layer_dict.get("id", 0))
        _culture_layers[id] = layer_dict

func _remove_culture_layers(ids) -> void:
    if ids is Array:
        for value in ids:
            _erase_culture_layer(int(value))
    elif ids is PackedInt32Array:
        var packed_ids: PackedInt32Array = ids
        for value in packed_ids:
            _erase_culture_layer(int(value))

func _erase_culture_layer(id: int) -> void:
    if _culture_layers.has(id):
        _culture_layers.erase(id)
    if _selected_culture_layer_id == id:
        _selected_culture_layer_id = -1

func _normalize_culture_layer(entry) -> Dictionary:
    if not (entry is Dictionary):
        return {}
    var info: Dictionary = (entry as Dictionary).duplicate(true)
    var traits_variant: Variant = info.get("traits", [])
    if traits_variant is Array:
        var cleaned: Array[Dictionary] = []
        for trait_entry in traits_variant:
            if trait_entry is Dictionary:
                cleaned.append((trait_entry as Dictionary).duplicate(true))
        info["traits"] = cleaned
    return info

func _update_culture_tensions(array_data, full_snapshot: bool) -> void:
    var tensions: Array[Dictionary] = []
    if array_data is Array:
        for entry in array_data:
            if not (entry is Dictionary):
                continue
            tensions.append((entry as Dictionary).duplicate(true))
    if full_snapshot:
        _culture_tension_tracker.clear()
    else:
        _log_new_culture_tensions(tensions)
    _culture_tensions = tensions

func _log_new_culture_tensions(tensions: Array[Dictionary]) -> void:
    for tension in tensions:
        var layer_id = int(tension.get("layer_id", 0))
        var kind_key = str(tension.get("kind", ""))
        var key = "%d:%s" % [layer_id, kind_key]
        var timer_val: int = int(tension.get("timer", 0))
        var previous: int = int(_culture_tension_tracker.get(key, -1))
        if timer_val > previous:
            var kind_label: String = str(tension.get("kind_label", kind_key.capitalize()))
            var scope_label: String = str(tension.get("scope_label", tension.get("scope", "")))
            var severity: float = float(tension.get("severity", 0.0))
            _append_log_entry("[color=#ffd166]%s[/color] layer #%03d [%s] severity %.2f (timer %d)" % [
                kind_label,
                layer_id,
                scope_label,
                severity,
                timer_val
            ])
            _culture_tension_tracker[key] = timer_val
        else:
            _culture_tension_tracker[key] = max(previous, timer_val)

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
        "tags": tags_mask,
        "x": int(info.get("x", -1)),
        "y": int(info.get("y", -1)),
        "element": int(info.get("element", -1)),
        "temperature": float(info.get("temperature", 0.0)),
        "mass": float(info.get("mass", 0.0))
    }
    _tile_records[entity] = record
    var coord := Vector2i(int(record.get("x", -1)), int(record.get("y", -1)))
    if coord.x >= 0 and coord.y >= 0:
        _tile_coord_lookup[coord] = entity
    _tile_total = max(_tile_records.size(), _tile_total + 1)
    _bump_terrain_count(terrain_id, 1)
    _bump_tag_counts(tags_mask, 1)

func _forget_tile(entity_id: int) -> void:
    if not _tile_records.has(entity_id):
        return
    var record: Dictionary = _tile_records[entity_id]
    var terrain_id = int(record.get("terrain", -1))
    var tags_mask = int(record.get("tags", 0))
    var coord := Vector2i(int(record.get("x", -1)), int(record.get("y", -1)))
    if _tile_coord_lookup.has(coord):
        _tile_coord_lookup.erase(coord)
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
