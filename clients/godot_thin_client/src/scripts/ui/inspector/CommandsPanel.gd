extends VBoxContainer
class_name CommandsInspectorPanel

## Inspector "Commands" tab: the designer/debug console. Owns all the runtime command
## controls (axis-bias, influencer support/suppress/channel/spawn, corruption inject,
## heat delta, config reload, scenario scout/follow, the autoplay row) plus the
## command status/log display.
##
## Outbound/command-driven: it issues verbs through an injected command hook and logs
## through an injected sink — the command transport (command_client), the autoplay
## timer, and turn-sending stay in the coordinator (shared with the CommandToolbar and
## Terrain tab). Cross-panel data arrives via targeted setters (set_axis_bias,
## set_influencer_roster, set_command_connected); this panel is NOT in the snapshot
## fan-out (_tab_panels) because it has no snapshot inputs.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

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
const COMMAND_LOG_LIMIT := 40

# Command status/log display
@onready var command_status_label: Label = $StatusLabel
@onready var command_log_text: RichTextLabel = $LogPanel/LogScroll/LogText
# Autoplay row
@onready var autoplay_toggle: CheckButton = $AutoplayRow/AutoplayToggle
@onready var autoplay_label: Label = $AutoplayRow/AutoplayIntervalLabel
@onready var autoplay_spin: SpinBox = $AutoplayRow/AutoplayIntervalSpin
# Axis bias
@onready var axis_dropdown: OptionButton = $AxisControls/AxisRow/AxisDropdown
@onready var axis_value_spin: SpinBox = $AxisControls/AxisRow/AxisValueSpin
@onready var axis_apply_button: Button = $AxisControls/AxisRow/AxisApplyButton
@onready var axis_reset_button: Button = $AxisControls/AxisRow/AxisResetButton
@onready var axis_reset_all_button: Button = $AxisControls/AxisResetAllButton
# Influencer commands
@onready var influencer_dropdown: OptionButton = $InfluencerControls/InfluencerRow/InfluencerDropdown
@onready var influencer_magnitude_spin: SpinBox = $InfluencerControls/InfluencerRow/InfluencerMagnitudeSpin
@onready var influencer_support_button: Button = $InfluencerControls/InfluencerRow/InfluencerSupportButton
@onready var influencer_suppress_button: Button = $InfluencerControls/InfluencerRow/InfluencerSuppressButton
@onready var channel_dropdown: OptionButton = $InfluencerControls/ChannelRow/ChannelDropdown
@onready var channel_magnitude_spin: SpinBox = $InfluencerControls/ChannelRow/ChannelMagnitudeSpin
@onready var channel_boost_button: Button = $InfluencerControls/ChannelRow/ChannelBoostButton
@onready var spawn_scope_dropdown: OptionButton = $InfluencerControls/SpawnRow/SpawnScopeDropdown
@onready var spawn_generation_spin: SpinBox = $InfluencerControls/SpawnRow/SpawnGenerationSpin
@onready var spawn_button: Button = $InfluencerControls/SpawnRow/SpawnButton
# Corruption inject
@onready var corruption_dropdown: OptionButton = $CorruptionControls/CorruptionRow/CorruptionSubsystemDropdown
@onready var corruption_intensity_spin: SpinBox = $CorruptionControls/CorruptionRow/CorruptionIntensitySpin
@onready var corruption_exposure_spin: SpinBox = $CorruptionControls/CorruptionRow/CorruptionExposureSpin
@onready var corruption_inject_button: Button = $CorruptionControls/CorruptionRow/CorruptionInjectButton
# Heat
@onready var heat_entity_spin: SpinBox = $HeatControls/HeatRow/HeatEntitySpin
@onready var heat_delta_spin: SpinBox = $HeatControls/HeatRow/HeatDeltaSpin
@onready var heat_apply_button: Button = $HeatControls/HeatRow/HeatApplyButton
# Config reload
@onready var config_path_edit: LineEdit = $ConfigControls/ConfigRow/ConfigPathEdit
@onready var turn_pipeline_reload_button: Button = $ConfigControls/ConfigRow/TurnPipelineReloadButton
@onready var snapshot_overlays_reload_button: Button = $ConfigControls/ConfigRow/SnapshotOverlaysReloadButton
# Scenario commands
@onready var scenario_faction_spin: SpinBox = $ScenarioCommands/ScenarioFactionRow/ScenarioFactionSpin
@onready var scout_x_spin: SpinBox = $ScenarioCommands/ScoutRow/ScoutXSpin
@onready var scout_y_spin: SpinBox = $ScenarioCommands/ScoutRow/ScoutYSpin
@onready var scout_execute_button: Button = $ScenarioCommands/ScoutRow/ScoutExecuteButton
@onready var follow_herd_field: LineEdit = $ScenarioCommands/FollowRow/FollowHerdField
@onready var follow_herd_button: Button = $ScenarioCommands/FollowRow/FollowHerdButton

var _command_log: Array[String] = []
## Display mirror of the coordinator-owned axis bias, pushed via set_axis_bias().
var _axis_bias: Dictionary = {}
## Influencer roster, pushed via set_influencer_roster() (owned by InfluencerPanel).
var _influencer_roster: Dictionary = {}
var _connected: bool = false
## Command hook: (line: String, success_msg: String) -> bool.
var _send: Callable = Callable()
## Command-log sink: (entry: String) -> void (the coordinator's _append_command_log).
var _append_log_sink: Callable = Callable()
## Guards set_autoplay_active() so mirroring the toolbar Play/Pause into the tab toggle
## does not re-emit autoplay_toggled back to the coordinator.
var _suppress_autoplay_signal: bool = false

## Panel -> coordinator: the axis-bias command + optimistic mirror stay coordinator-side
## (Sentiment depends on _axis_bias); the panel only requests the apply.
signal axis_bias_apply_requested(axis_idx: int, value: float)
## Panel -> coordinator: the autoplay timer + turn-sending live in the coordinator.
signal autoplay_toggled(pressed: bool)
signal autoplay_interval_changed(value: float)

func _ready() -> void:
	# Axis controls
	_populate_axis_items()
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
	# Influencer controls
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
	# Corruption controls
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
	# Heat controls
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
	# Config controls
	if config_path_edit != null:
		config_path_edit.clear_button_enabled = true
		config_path_edit.text = ""
	if turn_pipeline_reload_button != null:
		turn_pipeline_reload_button.pressed.connect(_on_turn_pipeline_reload_button_pressed)
	if snapshot_overlays_reload_button != null:
		snapshot_overlays_reload_button.pressed.connect(_on_snapshot_overlays_reload_button_pressed)
	# Scenario controls
	if scout_execute_button != null:
		scout_execute_button.pressed.connect(_on_scout_command_pressed)
	if follow_herd_button != null:
		follow_herd_button.pressed.connect(_on_follow_herd_button_pressed)
		follow_herd_button.tooltip_text = "Teleport bands to the selected herd and gain morale, supplies, fauna lore, and a fog reveal pulse."
	# Autoplay row (timer + turn-sending live in the coordinator; these only relay state)
	if autoplay_toggle != null:
		autoplay_toggle.toggled.connect(_on_autoplay_toggle_local)
		autoplay_toggle.button_pressed = false
	if autoplay_spin != null:
		autoplay_spin.value_changed.connect(_on_autoplay_interval_changed_local)
		autoplay_spin.min_value = 0.2
		autoplay_spin.max_value = 5.0
		autoplay_spin.step = 0.1
		if autoplay_spin.value < 0.2:
			autoplay_spin.value = 0.5
	if command_log_text != null:
		command_log_text.selection_enabled = true
	_refresh_axis_controls()
	_refresh_influencer_dropdown()
	_apply_enabled()

## Coordinator contract: no-op. This panel is command-driven (outbound) and takes no
## snapshot inputs — its data arrives via targeted setters (set_axis_bias,
## set_influencer_roster, set_command_connected), so it is intentionally kept out of the
## _tab_panels fan-out. Defined anyway to satisfy the tab-panel contract uniformly and
## stay crash-safe if it is ever added to the fan-out.
func apply_update(_data: Dictionary, _full_snapshot: bool) -> void:
	pass

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_command_log.clear()
	if command_log_text != null:
		command_log_text.text = ""
	if command_status_label != null:
		command_status_label.text = "Commands: disconnected."
	_refresh_axis_controls()
	_refresh_influencer_dropdown()
	_apply_enabled()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if command_log_text != null:
		Typography.apply(command_log_text, Typography.STYLE_BODY)
	if command_status_label != null:
		Typography.apply(command_status_label, Typography.STYLE_CAPTION)
	if autoplay_label != null:
		Typography.apply(autoplay_label, Typography.STYLE_CAPTION)
	for control in [
		autoplay_toggle, autoplay_spin,
		axis_dropdown, axis_value_spin, axis_apply_button, axis_reset_button, axis_reset_all_button,
		influencer_dropdown, influencer_magnitude_spin, influencer_support_button, influencer_suppress_button,
		channel_dropdown, channel_magnitude_spin, channel_boost_button,
		spawn_scope_dropdown, spawn_generation_spin, spawn_button,
		corruption_dropdown, corruption_intensity_spin, corruption_exposure_spin, corruption_inject_button,
		heat_entity_spin, heat_delta_spin, heat_apply_button,
		scenario_faction_spin, scout_x_spin, scout_y_spin, scout_execute_button,
		follow_herd_field, follow_herd_button,
		config_path_edit, turn_pipeline_reload_button, snapshot_overlays_reload_button
	]:
		if control != null:
			Typography.apply(control, Typography.STYLE_CONTROL)

## Coordinator collaborator: inject the command hook + log sink.
func set_command_hooks(send: Callable, append_log: Callable) -> void:
	_send = send
	_append_log_sink = append_log

## Coordinator contract: connection-gated enable/disable of the tab's controls.
func set_command_connected(connected: bool) -> void:
	_connected = connected
	_apply_enabled()

## Coordinator push: axis bias is coordinator-owned (Sentiment depends on it); mirrored
## here so the axis spin reflects the current + optimistic values.
func set_axis_bias(bias: Dictionary) -> void:
	_axis_bias = bias.duplicate(true)
	_refresh_axis_controls()

## Coordinator push: the influencer roster (owned by InfluencerPanel) for the dropdown.
func set_influencer_roster(roster: Dictionary) -> void:
	_influencer_roster = roster
	_refresh_influencer_dropdown()

## Coordinator collaborator: append + render a command-log line (buffer owned here).
func append_log(entry: String) -> void:
	_command_log.append(entry)
	while _command_log.size() > COMMAND_LOG_LIMIT:
		_command_log.pop_front()
	if command_log_text != null:
		command_log_text.text = "\n".join(_command_log)
		if command_log_text.get_line_count() > 0:
			command_log_text.scroll_to_line(command_log_text.get_line_count() - 1)

## Coordinator collaborator: set the command status line.
func set_status(text: String) -> void:
	if command_status_label != null:
		command_status_label.text = text

## Coordinator collaborator: the faction the scenario/fauna commands act on.
func get_scenario_faction() -> int:
	return int(scenario_faction_spin.value) if scenario_faction_spin != null else 0

## Coordinator collaborator: mirror the Fauna-selected herd into the follow field.
func set_follow_herd(herd_id: String) -> void:
	if follow_herd_field != null:
		follow_herd_field.text = herd_id

## Coordinator collaborator: mirror the toolbar Play/Pause into the tab toggle without
## re-emitting autoplay_toggled.
func set_autoplay_active(on: bool) -> void:
	if autoplay_toggle == null:
		return
	if autoplay_toggle.button_pressed == on:
		return
	_suppress_autoplay_signal = true
	autoplay_toggle.button_pressed = on
	_suppress_autoplay_signal = false

## Coordinator collaborator: the current autoplay interval (seconds).
func get_autoplay_interval() -> float:
	return float(autoplay_spin.value) if autoplay_spin != null else 0.5

func _on_autoplay_toggle_local(pressed: bool) -> void:
	if _suppress_autoplay_signal:
		return
	autoplay_toggled.emit(pressed)

func _on_autoplay_interval_changed_local(value: float) -> void:
	autoplay_interval_changed.emit(value)

# --- Axis bias ---

func _populate_axis_items() -> void:
	if axis_dropdown == null:
		return
	axis_dropdown.clear()
	for idx in range(AXIS_NAMES.size()):
		axis_dropdown.add_item(AXIS_NAMES[idx], idx)
	axis_dropdown.select(0)

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
		_populate_axis_items()
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

func _on_axis_dropdown_selected(_index: int) -> void:
	_update_axis_spin_value(_selected_axis_index())

func _on_axis_apply_button_pressed() -> void:
	var axis_idx = _selected_axis_index()
	if axis_idx < 0:
		_append_log_sink.call("Select an axis before applying bias.")
		return
	axis_bias_apply_requested.emit(axis_idx, float(axis_value_spin.value))

func _on_axis_reset_button_pressed() -> void:
	var axis_idx = _selected_axis_index()
	if axis_idx < 0:
		_append_log_sink.call("Select an axis before resetting bias.")
		return
	axis_value_spin.value = 0.0
	axis_bias_apply_requested.emit(axis_idx, 0.0)

func _on_axis_reset_all_button_pressed() -> void:
	for idx in range(AXIS_NAMES.size()):
		axis_bias_apply_requested.emit(idx, 0.0)

# --- Influencer commands ---

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
	var info = _influencer_roster.get(id, null)
	if info == null:
		return "ID %d" % id
	var name: String = str(info.get("name", "Influencer %d" % id))
	return name if name.strip_edges() != "" else "ID %d" % id

func _refresh_influencer_dropdown() -> void:
	if influencer_dropdown == null:
		return
	var previous_id: int = _selected_influencer_id()
	var entries: Array = []
	for key in _influencer_roster.keys():
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
	_apply_enabled()

func _compare_influencer_option(a: Dictionary, b: Dictionary) -> bool:
	var a_label: String = String(a.get("label", ""))
	var b_label: String = String(b.get("label", ""))
	return a_label < b_label

func _on_influencer_dropdown_selected(_index: int) -> void:
	_apply_enabled()

func _on_influencer_support_button_pressed() -> void:
	var id = _selected_influencer_id()
	if id < 0:
		_append_log_sink.call("Select an influencer before sending support.")
		return
	var magnitude: float = max(float(influencer_magnitude_spin.value), 0.0)
	var name: String = _influencer_display_name(id)
	_send.call("support %d %.3f" % [id, magnitude], "Support +%.2f sent to %s" % [magnitude, name])

func _on_influencer_suppress_button_pressed() -> void:
	var id = _selected_influencer_id()
	if id < 0:
		_append_log_sink.call("Select an influencer before sending suppress.")
		return
	var magnitude: float = max(float(influencer_magnitude_spin.value), 0.0)
	var name: String = _influencer_display_name(id)
	_send.call("suppress %d %.3f" % [id, magnitude], "Suppress −%.2f sent to %s" % [magnitude, name])

func _on_channel_boost_button_pressed() -> void:
	var id = _selected_influencer_id()
	if id < 0:
		_append_log_sink.call("Select an influencer before applying channel boost.")
		return
	if channel_dropdown == null or channel_dropdown.get_item_count() == 0:
		_append_log_sink.call("No channel options configured.")
		return
	var channel_index: int = channel_dropdown.get_selected()
	if channel_index < 0:
		channel_index = 0
	var channel_key_variant: Variant = channel_dropdown.get_item_metadata(channel_index)
	var channel_key: String = String(channel_key_variant) if typeof(channel_key_variant) == TYPE_STRING else "popular"
	var magnitude: float = max(float(channel_magnitude_spin.value), 0.0)
	var name: String = _influencer_display_name(id)
	var channel_label: String = String(channel_dropdown.get_item_text(channel_index))
	_send.call(
		"support_channel %d %s %.3f" % [id, channel_key, magnitude],
		"Channel boost (%s, +%.2f) sent to %s" % [channel_label, magnitude, name]
	)

func _on_spawn_button_pressed() -> void:
	var scope_key: Variant = null
	if spawn_scope_dropdown != null and spawn_scope_dropdown.get_item_count() > 0:
		var scope_index: int = spawn_scope_dropdown.get_selected()
		if scope_index < 0:
			scope_index = 0
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
					_append_log_sink.call("Specify a generation ID when spawning by generation.")
					return
				line = "spawn_influencer generation %d" % generation_id
				message = "Spawn influencer (generation %d) requested." % generation_id
			_:
				line = "spawn_influencer %s" % scope_text
				message = "Spawn influencer (%s) requested." % scope_text.capitalize()
	_send.call(line, message)

# --- Corruption inject ---

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
	_send.call(line, message)

# --- Heat ---

func _on_heat_apply_button_pressed() -> void:
	var entity_id: int = int(heat_entity_spin.value) if heat_entity_spin != null else 0
	var delta: int = int(heat_delta_spin.value) if heat_delta_spin != null else 0
	if entity_id <= 0:
		_append_log_sink.call("Heat command requires a valid entity id.")
		return
	var line: String = "heat %d %d" % [entity_id, delta]
	var message: String = "Heat delta %d applied to entity %d." % [delta, entity_id]
	_send.call(line, message)

# --- Config reload ---

func _on_turn_pipeline_reload_button_pressed() -> void:
	var path: String = ""
	if config_path_edit != null:
		path = String(config_path_edit.text).strip_edges()
	var command_line: String = "reload_config turn"
	var summary: String = "Turn pipeline config reload requested (watched file)."
	if path != "":
		command_line += " %s" % path
		summary = "Turn pipeline config reload requested (%s)." % path
	_send.call(command_line, summary)

func _on_snapshot_overlays_reload_button_pressed() -> void:
	var path: String = ""
	if config_path_edit != null:
		path = String(config_path_edit.text).strip_edges()
	var command_line: String = "reload_config overlay"
	var summary: String = "Snapshot overlays config reload requested (watched file)."
	if path != "":
		command_line += " %s" % path
		summary = "Snapshot overlays config reload requested (%s)." % path
	_send.call(command_line, summary)

# --- Scenario commands ---

func _on_scout_command_pressed() -> void:
	if scout_x_spin == null or scout_y_spin == null:
		return
	var x := int(scout_x_spin.value)
	var y := int(scout_y_spin.value)
	var faction := get_scenario_faction()
	var message := "Scout order queued for faction %d at (%d, %d)." % [faction, x, y]
	_send.call("scout %d %d %d" % [faction, x, y], message)

func _on_follow_herd_button_pressed() -> void:
	if follow_herd_field == null:
		return
	var herd_id := follow_herd_field.text.strip_edges()
	if herd_id.is_empty():
		_append_log_sink.call("Provide a herd id before issuing Hunt.")
		return
	var normalized := herd_id.to_lower().replace(" ", "_")
	var faction := get_scenario_faction()
	var message := "Hunt '%s' requested for faction %d." % [herd_id, faction]
	_send.call("follow_herd %d %s sustain" % [faction, normalized], message)

# --- Connection gating (the moving half of the coordinator's old
# _update_command_controls_enabled) ---

func _apply_enabled() -> void:
	var connected = _connected
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
	if scenario_faction_spin != null:
		scenario_faction_spin.editable = connected
	if scout_x_spin != null:
		scout_x_spin.editable = connected
	if scout_y_spin != null:
		scout_y_spin.editable = connected
	if scout_execute_button != null:
		scout_execute_button.disabled = not connected
	if follow_herd_field != null:
		follow_herd_field.editable = connected
	if follow_herd_button != null:
		follow_herd_button.disabled = not connected
	if turn_pipeline_reload_button != null:
		turn_pipeline_reload_button.disabled = not connected
	if snapshot_overlays_reload_button != null:
		snapshot_overlays_reload_button.disabled = not connected
	if config_path_edit != null:
		config_path_edit.editable = connected
