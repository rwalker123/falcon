extends CanvasLayer
class_name InspectorLayer

## Emitted whenever the width this panel reserves on the left edge changes —
## on show/hide and on live resize. The game area (map + HUD) insets by this
## amount so the Inspector never overlaps other panels.
signal reserved_width_changed(width: float)

const ScriptManagerPanel := preload("res://src/scripts/scripting/ScriptManagerPanel.gd")
const ScriptHostManager := preload("res://src/scripts/scripting/ScriptHostManager.gd")
# TerrainDefinitions moved to TerrainPanel.

const Typography = preload("res://src/scripts/Typography.gd")

# MAP_SIZE_* constants moved to MapPanel.

# MOUNTAIN_KIND_LABELS / FOOD_MODULE_LABELS moved to TerrainPanel.
# (bit 1 / CAP_CONSTRUCTION was dropped with the retired camp-founding command —
# nothing client-side gates on it now.)
const CAP_INDUSTRY_T1 := 1 << 2
const CAP_INDUSTRY_T2 := 1 << 3
const CAP_POWER := 1 << 4
const CAP_NAVAL_OPS := 1 << 5
const CAP_AIR_OPS := 1 << 6
const CAP_ESPIONAGE_T2 := 1 << 7
const CAP_MEGAPROJECTS := 1 << 8

var capability_flags: int = 0

@onready var sentiment_panel: SentimentInspectorPanel = $RootPanel/TabContainer/Sentiment
@onready var terrain_panel: TerrainInspectorPanel = $RootPanel/TabContainer/Terrain
@onready var map_panel: MapInspectorPanel = $RootPanel/TabContainer/Map
@onready var overlay_panel: OverlayInspectorPanel = $RootPanel/TabContainer/Map/MapVBox/OverlaySection
@onready var culture_panel: CultureInspectorPanel = $RootPanel/TabContainer/Culture
@onready var victory_panel: VictoryInspectorPanel = $RootPanel/TabContainer/Victory
@onready var influencer_panel: InfluencerInspectorPanel = $RootPanel/TabContainer/Influencers
@onready var corruption_panel: CorruptionInspectorPanel = $RootPanel/TabContainer/Corruption
@onready var power_panel: PowerInspectorPanel = $RootPanel/TabContainer/Power
@onready var trade_panel: TradeInspectorPanel = $RootPanel/TabContainer/Trade
@onready var crisis_panel: CrisisInspectorPanel = $RootPanel/TabContainer/Crisis
## Extracted tab panels that implement the coordinator contract (apply_update/reset).
## Populated in _ready once the @onready handles resolve.
var _tab_panels: Array = []
@onready var knowledge_panel: KnowledgeInspectorPanel = $RootPanel/TabContainer/Knowledge
@onready var great_discoveries_panel: GreatDiscoveriesInspectorPanel = $RootPanel/TabContainer/GreatDiscoveries
@onready var logs_panel: LogsInspectorPanel = $RootPanel/TabContainer/Logs
@onready var root_panel: Panel = $RootPanel
@onready var tab_container: TabContainer = $RootPanel/TabContainer
@onready var commands_panel: CommandsInspectorPanel = $RootPanel/TabContainer/Commands
@onready var fauna_panel: FaunaInspectorPanel = $RootPanel/TabContainer/Fauna
@onready var rollback_ten_button: Button = $RootPanel/CommandToolbar/RollbackTenButton
@onready var rollback_button: Button = $RootPanel/CommandToolbar/RollbackButton
@onready var play_pause_button: Button = $RootPanel/CommandToolbar/PlayPauseButton
@onready var step_one_button: Button = $RootPanel/CommandToolbar/StepOneButton
@onready var step_ten_button: Button = $RootPanel/CommandToolbar/StepTenButton
@onready var scripts_panel: ScriptManagerPanel = $RootPanel/TabContainer/Scripts

var _axis_bias: Dictionary = {}
# Terrain tile/biome/food state moved to TerrainPanel.
# Culture layer/tension state moved to CulturePanel.
var _map_view: Node = null
# Map-size + scenario state moved to MapPanel.
var _panel_visible: bool = true
var _seen_command_events: Dictionary = {}
var _resolved_font_size: int = Typography.DEFAULT_FONT_SIZE
var _last_turn: int = 0
var command_client: Object = null
var command_connected: bool = false
var stream_active: bool = false
var autoplay_timer: Timer
var _hud_layer: Object = null
# TERRAIN_* histogram/limit constants moved to TerrainPanel.
const PANEL_WIDTH_DEFAULT = 340.0
const PANEL_WIDTH_MIN = 260.0
const PANEL_MIN_TOP_OFFSET = 48.0
const PANEL_MARGIN = 16.0
const PANEL_HANDLE_WIDTH = 12.0
const PANEL_TAB_PADDING = 16.0
const AXIS_NAMES: Array[String] = ["Knowledge", "Trust", "Equity", "Agency"]
const AXIS_KEYS: Array[String] = ["knowledge", "trust", "equity", "agency"]
# CULTURE_* constants moved to CulturePanel.
# CHANNEL_OPTIONS / SPAWN_SCOPE_OPTIONS / CORRUPTION_OPTIONS moved to CommandsPanel.
var _viewport: Viewport = null
var _panel_width: float = PANEL_WIDTH_DEFAULT
var _is_resizing = false
var _script_host: ScriptHostManager = null
# Overlay channel state moved to OverlayPanel.

func _ready() -> void:
	Typography.initialize()
	_resolved_font_size = Typography.base_font_size()
	set_process(true)
	_viewport = get_viewport()
	if _viewport != null:
		_viewport.size_changed.connect(_on_viewport_resized)
	if root_panel != null:
		root_panel.gui_input.connect(_on_root_panel_gui_input)
		root_panel.focus_mode = Control.FOCUS_CLICK
	# Axis/influencer/corruption/heat/config controls are owned by CommandsPanel; the
	# map-size/scenario/rivers controls are owned by MapPanel.
	_apply_capability_gating()
	apply_typography()
	_tab_panels = [power_panel, crisis_panel, knowledge_panel, trade_panel, sentiment_panel, victory_panel, fauna_panel, great_discoveries_panel, logs_panel, influencer_panel, corruption_panel, map_panel, culture_panel, terrain_panel]
	if map_panel != null:
		map_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if culture_panel != null:
		culture_panel.set_log_hook(Callable(self, "_append_log_entry"))
	# Terrain owns tile selection + its Export Map button; export_map sends directly via the
	# hook. (The tile scout button was retired with the single-task `scout` command.)
	if terrain_panel != null:
		terrain_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if logs_panel != null:
		logs_panel.log_entry_received.connect(_on_log_stream_entry)
	if crisis_panel != null:
		crisis_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if knowledge_panel != null:
		knowledge_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	# Trade diffusion records also feed the Knowledge event list; the panels stay
	# decoupled — Trade emits, the coordinator forwards to Knowledge.
	if trade_panel != null and knowledge_panel != null:
		trade_panel.knowledge_events_produced.connect(
			func(records: Array) -> void: knowledge_panel.append_events(records)
		)
	if victory_panel != null:
		victory_panel.set_log_hook(Callable(self, "_append_command_log"))
	# Fauna is now display-only telemetry (herd list + detail). The follow-herd command it
	# used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a);
	# hunting is now labor allocation via the HUD.
	# Commands tab owns the runtime command controls; the hub (_send_command / autoplay
	# timer / command_client) stays here. Axis bias apply routes back so _axis_bias stays
	# coordinator-owned (Sentiment depends on it); autoplay relays to the coordinator timer.
	if commands_panel != null:
		commands_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
		commands_panel.axis_bias_apply_requested.connect(_on_axis_bias_apply_requested)
		commands_panel.autoplay_toggled.connect(_on_autoplay_toggled)
		commands_panel.autoplay_interval_changed.connect(_on_autoplay_interval_changed)
	_update_panel_layout()
	_render_static_sections()
	_setup_command_controls()

func is_panel_visible() -> bool:
	return _panel_visible

func set_panel_visible(visible: bool) -> void:
	_panel_visible = visible
	if root_panel != null:
		root_panel.visible = visible
	set_process(visible)
	set_process_input(visible)
	reserved_width_changed.emit(reserved_width())

func toggle_panel_visibility() -> void:
	set_panel_visible(not _panel_visible)

## Width the docked panel occupies on the left edge (0 when hidden). The game
## area insets by this so the Inspector reserves space instead of overlapping.
func reserved_width() -> float:
	if not _panel_visible:
		return 0.0
	return _panel_width + PANEL_MARGIN * 2.0

func update_snapshot(snapshot: Dictionary) -> void:
	_apply_update(snapshot, true)
	_render_dynamic_sections()
	if snapshot.has("capability_flags"):
		update_capability_flags(int(snapshot["capability_flags"]))

func update_delta(delta: Dictionary) -> void:
	_apply_update(delta, false)
	_render_dynamic_sections()
	if delta.has("capability_flags"):
		update_capability_flags(int(delta["capability_flags"]))

func _apply_update(data: Dictionary, full_snapshot: bool) -> void:
	if data.has("turn"):
		_last_turn = int(data.get("turn", _last_turn))
	if data.has("capability_flags"):
		capability_flags = int(data["capability_flags"])

	# campaign_profiles / campaign_label / faction_inventory / grid are consumed by
	# MapPanel via the _tab_panels fan-out at the end of this method.
	if data.has("command_events"):
		_ingest_command_events(data["command_events"])
	# food_modules + tiles/tile_updates/tile_removed are consumed by TerrainPanel via the
	# _tab_panels fan-out at the end of this method.

	if data.has("axis_bias"):
		var axis_dict: Dictionary = data["axis_bias"]
		_axis_bias = axis_dict.duplicate(true)
		if sentiment_panel != null:
			sentiment_panel.set_axis_bias(_axis_bias)
		if commands_panel != null:
			commands_panel.set_axis_bias(_axis_bias)

	# Influencer roster + corruption ledger are owned by InfluencerPanel / CorruptionPanel
	# and ingested via the _tab_panels fan-out at the end of this method.

	if data.has("overlays"):
		_ingest_overlays(data["overlays"])

	# culture_layers / culture_layer_updates / culture_layer_removed / culture_tensions are
	# consumed by CulturePanel via the _tab_panels fan-out; it renders from
	# _render_dynamic_sections with the coordinator-supplied influencer resonance.

	# Fan the update out to extracted tab panels last, so any coordinator-side
	# routing above (e.g. overlays.crisis_annotations via _ingest_overlays) is
	# already applied and a panel's own keys (e.g. crisis_overlay) win on conflict.
	for panel in _tab_panels:
		if panel != null:
			panel.apply_update(data, full_snapshot)

	# InfluencerPanel owns the roster; feed it to the Commands tab's influencer dropdown
	# after the panel has ingested this delta (panels stay decoupled — coordinator mediates).
	if (full_snapshot and data.has("influencers")) \
			or data.has("influencer_updates") or data.has("influencer_removed"):
		if commands_panel != null and influencer_panel != null:
			commands_panel.set_influencer_roster(influencer_panel.get_influencers())

func _render_dynamic_sections() -> void:
	# TerrainPanel renders in its own apply_update (no external dependency).
	# CulturePanel renders here so the coordinator can supply the influencer-resonance
	# summary (pulled from InfluencerPanel — panels stay decoupled).
	if culture_panel != null:
		culture_panel.render(influencer_panel.aggregate_resonance() if influencer_panel != null else {})

func _render_static_sections() -> void:
	if trade_panel != null:
		trade_panel.reset()
	if power_panel != null:
		power_panel.reset()
	if fauna_panel != null:
		fauna_panel.reset()
	if sentiment_panel != null:
		sentiment_panel.reset()
	if knowledge_panel != null:
		knowledge_panel.reset()
	if crisis_panel != null:
		crisis_panel.reset()
	if victory_panel != null:
		victory_panel.reset()
	if great_discoveries_panel != null:
		great_discoveries_panel.reset()
	if logs_panel != null:
		logs_panel.reset()
	_seen_command_events.clear()
	if terrain_panel != null:
		terrain_panel.reset()
	if culture_panel != null:
		culture_panel.reset()
	if commands_panel != null:
		commands_panel.reset()
	if overlay_panel != null:
		overlay_panel.reset()
	if map_panel != null:
		map_panel.reset()
	_panel_width = PANEL_WIDTH_DEFAULT
	_update_command_controls_enabled()

func apply_typography() -> void:
	Typography.initialize()
	_resolved_font_size = Typography.base_font_size()
	if root_panel != null:
		Typography.apply_theme(root_panel)
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
		Typography.apply(tab_container, Typography.STYLE_CONTROL)
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

	# Terrain widgets are styled by TerrainPanel.apply_typography().
	# The Commands-tab controls are styled by CommandsPanel.apply_typography().
	var control_nodes: Array = [
		rollback_ten_button,
		rollback_button,
		play_pause_button,
		step_one_button,
		step_ten_button
	]
	_apply_typography_style(control_nodes, Typography.STYLE_CONTROL)

	if crisis_panel != null:
		crisis_panel.apply_typography()
	if knowledge_panel != null:
		knowledge_panel.apply_typography()
	if trade_panel != null:
		trade_panel.apply_typography()
	if sentiment_panel != null:
		sentiment_panel.apply_typography()
	if great_discoveries_panel != null:
		great_discoveries_panel.apply_typography()
	if logs_panel != null:
		logs_panel.apply_typography()
	if influencer_panel != null:
		influencer_panel.apply_typography()
	if corruption_panel != null:
		corruption_panel.apply_typography()
	if commands_panel != null:
		commands_panel.apply_typography()
	if overlay_panel != null:
		overlay_panel.apply_typography()
	if map_panel != null:
		map_panel.apply_typography()
	if culture_panel != null:
		culture_panel.apply_typography()
	if terrain_panel != null:
		terrain_panel.apply_typography()

	_update_panel_layout()

func _setup_command_controls() -> void:
	if rollback_ten_button != null:
		rollback_ten_button.pressed.connect(_on_rollback_ten_button_pressed)
	if rollback_button != null:
		rollback_button.pressed.connect(_on_rollback_button_pressed)
	if play_pause_button != null:
		play_pause_button.pressed.connect(_on_play_pause_button_pressed)
		play_pause_button.button_pressed = false
	if step_one_button != null:
		step_one_button.pressed.connect(_on_step_one_button_pressed)
	if step_ten_button != null:
		step_ten_button.pressed.connect(_on_step_ten_button_pressed)
	# Autoplay toggle/interval + scenario buttons are owned by CommandsPanel; the timer
	# (which drives turn-stepping) stays here and is relayed via the panel's signals.
	autoplay_timer = Timer.new()
	autoplay_timer.one_shot = false
	autoplay_timer.wait_time = 0.5
	add_child(autoplay_timer)
	autoplay_timer.timeout.connect(_on_autoplay_timeout)
	# Terrain-tab command buttons (export/scout/found) are owned by TerrainPanel.
	_update_command_status()
	_append_command_log("Command console ready.")

func attach_script_host(manager: ScriptHostManager) -> void:
	if _script_host != null:
		if _script_host.is_connected("script_log", Callable(self, "_on_script_log_from_package")):
			_script_host.disconnect("script_log", Callable(self, "_on_script_log_from_package"))
		if _script_host.is_connected("script_alert", Callable(self, "_on_script_alert_from_package")):
			_script_host.disconnect("script_alert", Callable(self, "_on_script_alert_from_package"))
		if _script_host.is_connected("script_event", Callable(self, "_on_script_event_from_package")):
			_script_host.disconnect("script_event", Callable(self, "_on_script_event_from_package"))
	_script_host = manager
	if scripts_panel != null:
		scripts_panel.set_manager(manager)
	if _script_host != null:
		_script_host.script_log.connect(_on_script_log_from_package)
		_script_host.script_alert.connect(_on_script_alert_from_package)
		_script_host.script_event.connect(_on_script_event_from_package)

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
		if autoplay_timer != null and not autoplay_timer.is_stopped():
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
	if commands_panel != null:
		commands_panel.set_status(status_text)
	_update_command_controls_enabled()

func _append_command_log(entry: String) -> void:
	if commands_panel != null:
		commands_panel.append_log(entry)
	_append_log_entry("[CMD] %s" % entry, "COMMAND", "inspector.command")

func _update_command_controls_enabled() -> void:
	var connected = command_connected
	if map_panel != null:
		map_panel.set_command_connected(connected)
	# Terrain's tile action buttons are gated inside TerrainPanel (connection + tile
	# selection + construction capability).
	if terrain_panel != null:
		terrain_panel.set_command_connected(connected)
	# The Commands-tab controls (axis/influencer/corruption/heat/scenario/config) are
	# gated inside CommandsPanel.
	if commands_panel != null:
		commands_panel.set_command_connected(connected)
	if fauna_panel != null:
		fauna_panel.set_command_connected(connected)
	if knowledge_panel != null:
		knowledge_panel.set_command_connected(connected)

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

func send_runtime_command(line: String, success_message: String) -> bool:
	return _send_command(line, success_message)

## Optional observer invoked after a turn is advanced through THIS coordinator — i.e. the dev
## toolbar and autoplay, which are DELIBERATELY NOT gated by the client-side end-turn gate the
## turn orb applies (docs/plan_the_telling.md §1a: autoplay disables itself on a failed advance,
## so a hard gate here would deadlock the dev loop, and the server auto-expires an unanswered
## fork to its defer branch anyway). Main uses it to make that consequence VISIBLE rather than
## silent — skipping the question is a coherent dev-tool act, but it must not go unremarked.
var _turn_advance_observer: Callable = Callable()

func set_turn_advance_observer(observer: Callable) -> void:
	_turn_advance_observer = observer

func _send_turn(steps: int) -> bool:
	var sent := _send_command("turn %d" % steps, "+%d turns requested." % steps)
	if sent and _turn_advance_observer.is_valid():
		_turn_advance_observer.call(steps)
	return sent

func _request_rollback(steps: int) -> void:
	if _last_turn <= 0:
		_append_command_log("Rollback unavailable (turn 0).")
		return
	var target: int = max(_last_turn - steps, 0)
	if target == _last_turn:
		_append_command_log("Rollback unavailable (turn 0).")
		return
	_send_command("rollback %d" % target, "Rollback to turn %d requested." % target)

func _on_step_one_button_pressed() -> void:
	_send_turn(1)

func _on_step_ten_button_pressed() -> void:
	_send_turn(10)

func _on_rollback_ten_button_pressed() -> void:
	_request_rollback(10)

func _on_rollback_button_pressed() -> void:
	_request_rollback(1)

func _on_play_pause_button_pressed() -> void:
	# The toolbar Play/Pause and the Commands-tab autoplay toggle drive the same timer;
	# _on_autoplay_toggled mirrors the state into both.
	_on_autoplay_toggled(play_pause_button.button_pressed)

func _on_autoplay_toggled(pressed: bool) -> void:
	if play_pause_button != null and play_pause_button.button_pressed != pressed:
		play_pause_button.button_pressed = pressed
	if commands_panel != null:
		commands_panel.set_autoplay_active(pressed)
	if pressed:
		if not _ensure_command_connection():
			if commands_panel != null:
				commands_panel.set_autoplay_active(false)
			if play_pause_button != null:
				play_pause_button.button_pressed = false
			_append_command_log("Auto-play requires an active command connection.")
			return
		var interval := commands_panel.get_autoplay_interval() if commands_panel != null else 0.5
		if autoplay_timer != null:
			autoplay_timer.wait_time = interval
			autoplay_timer.start()
		_append_command_log("Auto-play enabled (%.2fs)." % interval)
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
	if commands_panel != null:
		commands_panel.set_autoplay_active(false)

func attach_map_view(view: Node) -> void:
	_map_view = view
	if trade_panel != null:
		trade_panel.set_map_view(view)
	if map_panel != null:
		map_panel.set_map_view(view)
	if overlay_panel != null:
		overlay_panel.set_map_view(view)
	if culture_panel != null:
		culture_panel.set_map_view(view)
	if terrain_panel != null:
		terrain_panel.set_map_view(view)

func set_hud_layer(layer: Object) -> void:
	_hud_layer = layer
	_update_panel_layout()

## Inbound MapView hex-selection (wired in Main.gd to inspector.focus_tile_from_map);
## forwarded to the Terrain tab which owns tile drill-down.
func focus_tile_from_map(col: int, row: int, terrain_id: int) -> void:
	if terrain_panel != null:
		terrain_panel.focus_tile_from_map(col, row, terrain_id)

func _on_log_stream_entry(entry: Dictionary) -> void:
	# Cross-panel dispatch of a raw log-stream entry (LogsPanel owns display/sparkline).
	if knowledge_panel != null:
		knowledge_panel.ingest_log_entry(entry)
	if trade_panel != null:
		trade_panel.ingest_log_entry(entry)

func get_resolved_font_size() -> int:
	return _resolved_font_size

func _apply_typography_style(controls: Array, style: StringName) -> void:
	for control in controls:
		if control is Control:
			Typography.apply(control, style)

func _panel_top_offset() -> float:
	var baseline := PANEL_MARGIN + Typography.line_height(Typography.STYLE_HEADING)
	baseline = max(baseline, PANEL_MIN_TOP_OFFSET)
	if _hud_layer != null and _hud_layer.has_method("get_upper_stack_height"):
		var height_variant: Variant = _hud_layer.call("get_upper_stack_height")
		if typeof(height_variant) in [TYPE_FLOAT, TYPE_INT]:
			baseline = max(baseline, float(height_variant))
	return baseline

func _update_panel_layout() -> void:
	if root_panel == null:
		return
	var required_width: float = PANEL_WIDTH_MIN
	if tab_container != null:
		var min_from_content: float = tab_container.get_combined_minimum_size().x
		var actual_content: float = tab_container.size.x
		var inner_width: float = max(min_from_content, actual_content)
		if inner_width > 0.0:
			required_width = max(required_width, inner_width + PANEL_TAB_PADDING)
	var max_width: float = _max_panel_width()
	if required_width > max_width:
		required_width = max_width
	_panel_width = clamp(_panel_width, required_width, max_width)
	root_panel.offset_left = PANEL_MARGIN
	root_panel.offset_right = PANEL_MARGIN + _panel_width
	root_panel.offset_top = _panel_top_offset()
	root_panel.offset_bottom = -PANEL_MARGIN
	root_panel.custom_minimum_size = Vector2(_panel_width, 0)
	reserved_width_changed.emit(reserved_width())

func _on_viewport_resized() -> void:
	_update_panel_layout()

func _max_panel_width() -> float:
	var target_viewport = _viewport if _viewport != null else get_viewport()
	if target_viewport == null:
		return PANEL_WIDTH_DEFAULT
	var viewport_size = target_viewport.get_visible_rect().size
	var max_allowed = viewport_size.x - (PANEL_MARGIN * 2.0)
	return max(max_allowed, PANEL_WIDTH_MIN)

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

func _ingest_overlays(overlays: Variant) -> void:
	if not (overlays is Dictionary):
		return
	var overlay_dict: Dictionary = overlays
	# The biome palette + tag labels arrive on the overlays key but belong to Terrain.
	if overlay_dict.has("terrain_palette") and terrain_panel != null:
		var palette_variant: Variant = overlay_dict["terrain_palette"]
		if palette_variant is Dictionary:
			terrain_panel.set_terrain_palette(palette_variant as Dictionary)
	if overlay_dict.has("terrain_tag_labels") and terrain_panel != null:
		var tag_variant: Variant = overlay_dict["terrain_tag_labels"]
		if tag_variant is Dictionary:
			terrain_panel.set_terrain_tag_labels(tag_variant as Dictionary)
	if overlay_dict.has("crisis_annotations") and crisis_panel != null:
		crisis_panel.ingest_annotations(overlay_dict["crisis_annotations"])
	# Overlay channels are owned by OverlayPanel; hand it the payload plus Terrain's tag
	# labels (which gate the terrain-tags channel).
	if overlay_panel != null:
		var tag_labels: Dictionary = terrain_panel.get_terrain_tag_labels() if terrain_panel != null else {}
		overlay_panel.ingest(overlay_dict, tag_labels)

# CommandsPanel owns the axis widgets and requests an apply via axis_bias_apply_requested.
# _axis_bias stays coordinator-owned here (Sentiment depends on it); on a successful send we
# update the mirror and push it to both the Sentiment view and the Commands axis spin.
func _on_axis_bias_apply_requested(axis_idx: int, value: float) -> void:
	if axis_idx < 0 or axis_idx >= AXIS_NAMES.size():
		_append_command_log("Invalid axis selection.")
		return
	var clamped: float = clamp(value, -1.0, 1.0)
	var message: String = "Axis %s set to %.3f" % [AXIS_NAMES[axis_idx], clamped]
	if _send_command("bias %d %.6f" % [axis_idx, clamped], message):
		var key: String = String(AXIS_KEYS[axis_idx])
		_axis_bias[key] = clamped
		if sentiment_panel != null:
			sentiment_panel.set_axis_bias(_axis_bias)
		if commands_panel != null:
			commands_panel.set_axis_bias(_axis_bias)

func _ingest_command_events(events_variant: Variant) -> void:
	if not (events_variant is Array):
		return
	var events_array: Array = events_variant
	for entry_variant in events_array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var tick: int = int(entry.get("tick", -1))
		var kind: String = String(entry.get("kind", "")).strip_edges()
		var label: String = String(entry.get("label", "")).strip_edges()
		var detail: String = String(entry.get("detail", "")).strip_edges()
		var signature := "%d|%s|%s|%s" % [tick, kind, label, detail]
		if _seen_command_events.has(signature):
			continue
		_seen_command_events[signature] = true
		var prefix := kind.capitalize() if kind != "" else "Command"
		var message := "[SIM] %s: %s" % [prefix, label]
		if detail != "":
			message += " (%s)" % detail
		_append_command_log(message)

func _on_script_log_from_package(script_id: int, level: String, message: String) -> void:
	var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
	var normalized_level: String = _normalize_log_level(level)
	var target: String = "script.%d" % script_id if script_id >= 0 else "script"
	var entry: String = "%s %s" % [prefix, message]
	_append_log_entry(entry, normalized_level, target)

func _on_script_alert_from_package(script_id: int, data: Dictionary) -> void:
	var title: String = data.get("title", "Alert")
	var level: String = data.get("level", "info")
	var body: String = data.get("message", "")
	var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
	var normalized_level: String = _normalize_log_level(level)
	var target: String = "script.%d" % script_id if script_id >= 0 else "script"
	_append_log_entry("%s alert (%s): %s" % [prefix, normalized_level.to_lower(), title], normalized_level, target)
	if not body.is_empty():
		_append_log_entry("  %s" % body, normalized_level, target)

func _on_script_event_from_package(script_id: int, event_name: String, payload: Variant) -> void:
	if event_name == "commands.issue.result" and typeof(payload) == TYPE_DICTIONARY:
		var ok: bool = payload.get("ok", false)
		var line: String = payload.get("line", "")
		var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
		var target: String = "script.%d" % script_id if script_id >= 0 else "script"
		if ok:
			_append_log_entry("%s command acknowledged: %s" % [prefix, line], "INFO", target)
		else:
			_append_log_entry("%s command failed: %s" % [prefix, line], "WARN", target)

func _append_log_entry(entry: String, level: String = "INFO", target: String = "inspector", timestamp_ms: int = -1) -> void:
	# Thin forwarder: synthetic log lines (command log, culture tensions, script logs)
	# are recorded/displayed by the LogsPanel, which owns the log buffer.
	if logs_panel != null:
		logs_panel.append_entry(entry, level, target, timestamp_ms)

# Small local copy for the script-alert display strings, which need the normalized
# level before handing off (LogsPanel re-normalizes on record).
func _normalize_log_level(level: String) -> String:
	var upper: String = level.to_upper()
	match upper:
		"WARNING":
			return "WARN"
		"ERR":
			return "ERROR"
		_:
			return upper

# Capability gating
func update_capability_flags(flags: int) -> void:
	capability_flags = flags
	_apply_capability_gating()

func _apply_capability_gating() -> void:
	# Power stays a clickable tab; when its capability is locked the panel renders an
	# explanation of how it unlocks rather than being greyed out (see PowerPanel).
	if power_panel != null:
		power_panel.set_available(_has_flag(CAP_POWER))
	if great_discoveries_panel != null:
		great_discoveries_panel.set_available(_has_flag(CAP_MEGAPROJECTS))
	# Knowledge stays a clickable tab; the panel renders a locked explanation while gated.
	if knowledge_panel != null:
		knowledge_panel.set_available(_has_flag(CAP_ESPIONAGE_T2))
	# Trade stays a clickable tab; the panel renders a locked explanation while gated.
	if trade_panel != null:
		trade_panel.set_available(_has_flag(CAP_INDUSTRY_T1) or _has_flag(CAP_INDUSTRY_T2))
	# Terrain is an always-available inspection tab (biome list, tile drill-down, terrain
	# highlight) with no capability-gated actions.
	_set_tab_enabled("Terrain", true)
	# Crisis stays a clickable tab; the panel renders a locked explanation while gated.
	if crisis_panel != null:
		crisis_panel.set_available(_has_flag(CAP_MEGAPROJECTS))
	# Influencers stays a clickable tab; the panel renders a locked explanation while gated.
	if influencer_panel != null:
		influencer_panel.set_available(_has_flag(CAP_INDUSTRY_T1) or _has_flag(CAP_INDUSTRY_T2))

func _set_tab_enabled(name: String, enabled: bool) -> void:
	if tab_container == null:
		return
	for i in range(tab_container.get_tab_count()):
		if tab_container.get_tab_title(i) == name:
			tab_container.set_tab_disabled(i, not enabled)
			break

func _has_flag(bit: int) -> bool:
	return (capability_flags & bit) != 0
