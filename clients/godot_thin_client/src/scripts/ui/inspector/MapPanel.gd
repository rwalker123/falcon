extends ScrollContainer
class_name MapInspectorPanel

## Inspector "Map" tab. Owns the map-size controls, the start-profile (scenario)
## controls, and the hydrology "highlight rivers" toggle. Issues map_size / start_profile
## commands through the injected command hook and drives MapView.set_highlight_rivers.
##
## Snapshot-driven (in _tab_panels): apply_update() consumes grid / campaign_profiles /
## campaign_label / faction_inventory. Collaborators: set_command_hooks + set_command_connected
## (command seam) and set_map_view (rivers toggle). The nested "Map Overlays" section has
## its own OverlayPanel script — this panel does not touch it.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

const MAP_SIZE_OPTIONS := [
	{"key": "tiny", "label": "Tiny", "width": 56, "height": 36},
	{"key": "small", "label": "Small", "width": 66, "height": 42},
	{"key": "standard", "label": "Standard", "width": 80, "height": 52},
	{"key": "large", "label": "Large", "width": 104, "height": 64},
	{"key": "huge", "label": "Huge", "width": 128, "height": 80}
]
const MAP_SIZE_DEFAULT_KEY := "standard"
const MAP_SIZE_DEFAULT_DIMENSIONS := Vector2i(80, 52)

@onready var map_size_label: Label = $MapVBox/MapSizeSection/MapSizeLabel
@onready var map_size_dropdown: OptionButton = $MapVBox/MapSizeSection/MapSizeDropdown
@onready var map_generate_button: Button = $MapVBox/MapSizeSection/GenerateMapButton
@onready var map_terrain_hint_label: Label = $MapVBox/MapTerrainHint
@onready var scenario_label: Label = $MapVBox/ScenarioSection/ScenarioLabel
@onready var scenario_dropdown: OptionButton = $MapVBox/ScenarioSection/ScenarioDropdown
@onready var scenario_description_label: Label = $MapVBox/ScenarioSection/ScenarioDescription
@onready var scenario_apply_button: Button = $MapVBox/ScenarioSection/ScenarioActions/ApplyScenarioButton
@onready var scenario_regen_toggle: CheckButton = $MapVBox/ScenarioSection/ScenarioActions/RegenerateToggle
@onready var highlight_rivers_toggle: CheckButton = $MapVBox/HydrologySection/HighlightRiversToggle

var _faction_inventory_state: Array = []
var _map_size_key: String = MAP_SIZE_DEFAULT_KEY
var _map_dimensions: Vector2i = MAP_SIZE_DEFAULT_DIMENSIONS
var _map_size_custom_index: int = -1
var _suppress_map_size_signal: bool = false
var _campaign_profiles: Array = []
var _active_profile_id: String = ""
var _selected_profile_id: String = ""
var _suppress_scenario_signal: bool = false
## Pushed by the coordinator; the rivers toggle is applied to it.
var _map_view: Node = null
## Command hook: (line: String, success_msg: String) -> bool.
var _send: Callable = Callable()
## Command-log sink: (entry: String) -> void.
var _append_log_sink: Callable = Callable()
var _connected: bool = false

func _ready() -> void:
	_initialize_map_controls()
	_initialize_scenario_controls()
	if highlight_rivers_toggle != null:
		highlight_rivers_toggle.toggled.connect(_on_highlight_rivers_toggled)
	_apply_enabled()

## Coordinator contract: consume the map/scenario snapshot keys.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	if data.has("campaign_profiles"):
		var profiles_variant: Variant = data["campaign_profiles"]
		if profiles_variant is Array:
			_campaign_profiles = (profiles_variant as Array).duplicate(true)
			_refresh_scenario_dropdown()
	if data.has("campaign_label"):
		var label_variant: Variant = data["campaign_label"]
		if label_variant is Dictionary:
			var label_dict: Dictionary = label_variant
			_active_profile_id = String(label_dict.get("profile_id", _active_profile_id))
			if _selected_profile_id == "":
				_selected_profile_id = _active_profile_id
			_refresh_scenario_selection()
	if data.has("faction_inventory"):
		var inventory_variant: Variant = data["faction_inventory"]
		if inventory_variant is Array:
			_faction_inventory_state = (inventory_variant as Array).duplicate(true)
			_refresh_scenario_description()
	if data.has("grid"):
		var grid_variant: Variant = data["grid"]
		if grid_variant is Dictionary:
			var grid_dict: Dictionary = grid_variant
			var width: int = int(grid_dict.get("width", _map_dimensions.x))
			var height: int = int(grid_dict.get("height", _map_dimensions.y))
			if width > 0 and height > 0:
				_set_map_size_selection_from_dimensions(width, height)

## Coordinator contract: re-render from retained state. Map/scenario config persists
## across reconnects (unlike per-turn data), so this only re-populates the widgets.
func reset() -> void:
	_populate_map_size_dropdown()
	_refresh_scenario_dropdown()
	_apply_enabled()

## Coordinator collaborator: inject the command hook + log sink.
func set_command_hooks(send: Callable, append_log: Callable) -> void:
	_send = send
	_append_log_sink = append_log

## Coordinator contract: connection-gated enable/disable of the command controls.
func set_command_connected(connected: bool) -> void:
	_connected = connected
	_apply_enabled()

## Coordinator collaborator: the map view the rivers toggle is pushed to.
func set_map_view(view: Node) -> void:
	_map_view = view
	if highlight_rivers_toggle != null and _map_view != null and _map_view.has_method("set_highlight_rivers"):
		_map_view.call("set_highlight_rivers", highlight_rivers_toggle.button_pressed)

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if map_size_label != null:
		Typography.apply(map_size_label, Typography.STYLE_HEADING)
	if map_terrain_hint_label != null:
		Typography.apply(map_terrain_hint_label, Typography.STYLE_CAPTION)
	if map_size_dropdown != null:
		Typography.apply(map_size_dropdown, Typography.STYLE_CONTROL)

func _apply_enabled() -> void:
	var connected = _connected
	if map_size_dropdown != null:
		map_size_dropdown.disabled = not connected
	if map_generate_button != null:
		map_generate_button.disabled = not connected
	if scenario_apply_button != null:
		scenario_apply_button.disabled = not connected
	if scenario_regen_toggle != null:
		scenario_regen_toggle.disabled = not connected

func _on_highlight_rivers_toggled(pressed: bool) -> void:
	if _map_view != null and _map_view.has_method("set_highlight_rivers"):
		_map_view.call("set_highlight_rivers", pressed)

func _initialize_map_controls() -> void:
	if map_size_dropdown != null:
		_populate_map_size_dropdown()
		var callable = Callable(self, "_on_map_size_selected")
		if not map_size_dropdown.is_connected("item_selected", callable):
			map_size_dropdown.item_selected.connect(_on_map_size_selected)
		map_size_dropdown.focus_mode = Control.FOCUS_ALL
	if map_generate_button != null:
		map_generate_button.focus_mode = Control.FOCUS_ALL
		var generate_callable = Callable(self, "_on_map_generate_button_pressed")
		if not map_generate_button.is_connected("pressed", generate_callable):
			map_generate_button.pressed.connect(_on_map_generate_button_pressed)
		map_generate_button.tooltip_text = "Regenerate the map using the current dimensions."

func _initialize_scenario_controls() -> void:
	if scenario_dropdown != null:
		scenario_dropdown.focus_mode = Control.FOCUS_ALL
		var callable = Callable(self, "_on_scenario_selected")
		if not scenario_dropdown.is_connected("item_selected", callable):
			scenario_dropdown.item_selected.connect(_on_scenario_selected)
	if scenario_apply_button != null:
		scenario_apply_button.focus_mode = Control.FOCUS_ALL
		var apply_callable = Callable(self, "_on_scenario_apply_pressed")
		if not scenario_apply_button.is_connected("pressed", apply_callable):
			scenario_apply_button.pressed.connect(_on_scenario_apply_pressed)
	if scenario_regen_toggle != null:
		scenario_regen_toggle.button_pressed = true

func _refresh_scenario_dropdown() -> void:
	if scenario_dropdown == null:
		return
	_suppress_scenario_signal = true
	scenario_dropdown.clear()
	for idx in range(_campaign_profiles.size()):
		var profile: Dictionary = _campaign_profiles[idx]
		var label := _scenario_profile_label(profile)
		scenario_dropdown.add_item(label)
		scenario_dropdown.set_item_metadata(idx, profile)
	_suppress_scenario_signal = false
	_refresh_scenario_selection()

func _refresh_scenario_selection() -> void:
	if scenario_dropdown == null:
		return
	var count := scenario_dropdown.get_item_count()
	if count == 0:
		_refresh_scenario_description()
		return
	var desired_id: String = _active_profile_id if _active_profile_id != "" else _selected_profile_id
	var applied: bool = false
	if desired_id != "":
		for idx in range(count):
			var metadata: Variant = scenario_dropdown.get_item_metadata(idx)
			if metadata is Dictionary and String(metadata.get("id", "")) == desired_id:
				if scenario_dropdown.get_selected() != idx:
					_suppress_scenario_signal = true
					scenario_dropdown.select(idx)
					_suppress_scenario_signal = false
				_selected_profile_id = desired_id
				applied = true
				break
	if not applied:
		if scenario_dropdown.get_selected() < 0:
			_suppress_scenario_signal = true
			scenario_dropdown.select(0)
			_suppress_scenario_signal = false
		var metadata: Variant = scenario_dropdown.get_item_metadata(scenario_dropdown.get_selected())
		if metadata is Dictionary:
			_selected_profile_id = String(metadata.get("id", _selected_profile_id))
	_refresh_scenario_description()

func _refresh_scenario_description() -> void:
	if scenario_description_label == null:
		return
	var profile: Dictionary = _current_selected_profile()
	if profile.is_empty():
		scenario_description_label.text = "Select a start profile to see its description."
		return
	var title: String = String(profile.get("title", profile.get("id", "")))
	var subtitle: String = String(profile.get("subtitle", "")).strip_edges()
	var lines: Array = []
	if title != "":
		lines.append(title)
	if subtitle != "":
		lines.append(subtitle)

	var detail_lines: Array = []
	var units_variant: Variant = profile.get("starting_units", null)
	if units_variant is Array:
		var units_array: Array = (units_variant as Array)
		var unit_summaries: Array = []
		for unit in units_array:
			if not (unit is Dictionary):
				continue
			var count: int = int(unit.get("count", 0))
			var kind: String = String(unit.get("kind", "")).strip_edges()
			if kind == "":
				continue
			var unit_label: String = "%dx %s" % [count, kind] if count > 0 else kind
			unit_summaries.append(unit_label)
		if unit_summaries.size() > 0:
			detail_lines.append("Units: %s" % _join_profile_strings(unit_summaries))

	var profile_inventory_variant: Variant = profile.get("inventory", null)
	if profile_inventory_variant is Array:
		var inventory_array: Array = (profile_inventory_variant as Array)
		var inventory_lines: Array = []
		for entry in inventory_array:
			if not (entry is Dictionary):
				continue
			var item: String = String(entry.get("item", "")).strip_edges()
			if item == "":
				continue
			var quantity: int = int(entry.get("quantity", 0))
			if quantity != 0:
				inventory_lines.append("%d %s" % [quantity, item])
			else:
				inventory_lines.append(item)
		if inventory_lines.size() > 0:
			detail_lines.append("Inventory: %s" % _join_profile_strings(inventory_lines))

	var knowledge_variant: Variant = profile.get("knowledge_tags", null)
	if knowledge_variant is Array:
		var knowledge_array: Array = (knowledge_variant as Array)
		var tags: Array = []
		for tag in knowledge_array:
			var label := String(tag).strip_edges()
			if label != "":
				tags.append(label)
		if tags.size() > 0:
			detail_lines.append("Knowledge: %s" % _join_profile_strings(tags))

	var fog_parts: Array = []
	var fog_mode: String = String(profile.get("fog_mode", "")).strip_edges()
	if fog_mode != "":
		fog_parts.append(fog_mode.capitalize())
	var survey_radius: int = int(profile.get("survey_radius", -1))
	if survey_radius >= 0:
		fog_parts.append("radius %d" % survey_radius)
	if fog_parts.size() > 0:
		detail_lines.append("Fog: %s" % _join_profile_strings(fog_parts))

	if _faction_inventory_state.size() > 0:
		var runtime_lines: Array = []
		for faction_entry in _faction_inventory_state:
			if not (faction_entry is Dictionary):
				continue
			var faction_inventory_variant: Variant = faction_entry.get("inventory", [])
			if not (faction_inventory_variant is Array):
				continue
			var entries: Array = faction_inventory_variant
			var rendered_entries: Array = []
			for entry in entries:
				if not (entry is Dictionary):
					continue
				var item_name: String = String(entry.get("item", "")).strip_edges()
				if item_name == "":
					continue
				var quantity: int = int(entry.get("quantity", 0))
				rendered_entries.append("%d %s" % [quantity, item_name])
			if rendered_entries.is_empty():
				continue
			var faction_id: int = int(faction_entry.get("faction", 0))
			runtime_lines.append("Faction %d: %s" % [faction_id, _join_profile_strings(rendered_entries)])
		if runtime_lines.size() > 0:
			if detail_lines.size() > 0:
				detail_lines.append("")
			detail_lines.append("Runtime stockpiles:")
			for runtime_line in runtime_lines:
				detail_lines.append(runtime_line)

	if detail_lines.size() > 0:
		if lines.size() > 0:
			lines.append("")
		for detail_line in detail_lines:
			lines.append(detail_line)

	if lines.size() == 0:
		scenario_description_label.text = "Select a start profile to see its description."
	else:
		scenario_description_label.text = "\n".join(lines)

func _current_selected_profile() -> Dictionary:
	if scenario_dropdown == null:
		return {}
	var index := scenario_dropdown.get_selected()
	if index < 0 or index >= scenario_dropdown.get_item_count():
		return {}
	var metadata: Variant = scenario_dropdown.get_item_metadata(index)
	if metadata is Dictionary:
		return metadata
	return {}

func _join_profile_strings(parts: Array, separator: String = ", ") -> String:
	if parts.is_empty():
		return ""
	var packed := PackedStringArray()
	for part in parts:
		packed.append(String(part))
	var buffer := ""
	for idx in range(packed.size()):
		if idx > 0:
			buffer += separator
		buffer += packed[idx]
	return buffer

func _scenario_profile_label(profile: Dictionary) -> String:
	var id: String = String(profile.get("id", ""))
	var title: String = String(profile.get("title", id))
	var subtitle: String = String(profile.get("subtitle", "")).strip_edges()
	if subtitle == "":
		return title
	return "%s — %s" % [title, subtitle]

func _on_scenario_selected(index: int) -> void:
	if _suppress_scenario_signal:
		return
	if scenario_dropdown == null:
		return
	var metadata: Variant = scenario_dropdown.get_item_metadata(index)
	if metadata is Dictionary:
		_selected_profile_id = String(metadata.get("id", _selected_profile_id))
	_refresh_scenario_description()

func _on_scenario_apply_pressed() -> void:
	if _selected_profile_id == "":
		_append_log_sink.call("Select a start profile before applying.")
		return
	var profile: Dictionary = _current_selected_profile()
	var display_name: String = _scenario_profile_label(profile)
	var line := "start_profile %s" % _selected_profile_id
	var message := "Start profile '%s' requested." % display_name
	if _send.call(line, message):
		if scenario_regen_toggle != null and scenario_regen_toggle.button_pressed:
			_on_map_generate_button_pressed()

func _custom_map_size_label(dimensions: Vector2i) -> String:
	if dimensions.x <= 0 or dimensions.y <= 0:
		return "Custom"
	return "Custom (%dx%d)" % [dimensions.x, dimensions.y]

func _active_map_label() -> String:
	if _map_size_key == "" or _map_size_key == "custom":
		return "Custom"
	for option in MAP_SIZE_OPTIONS:
		if String(option.get("key", "")) == _map_size_key:
			return String(option.get("label", _map_size_key.capitalize()))
	return _map_size_key.capitalize()

func _populate_map_size_dropdown() -> void:
	if map_size_dropdown == null:
		return
	var previous := _suppress_map_size_signal
	_suppress_map_size_signal = true
	map_size_dropdown.clear()
	var index := 0
	for option in MAP_SIZE_OPTIONS:
		var label: String = "%s (%dx%d)" % [
			String(option.get("label", "")),
			int(option.get("width", 0)),
			int(option.get("height", 0))
		]
		map_size_dropdown.add_item(label)
		map_size_dropdown.set_item_metadata(index, option)
		if String(option.get("key", "")) == _map_size_key:
			map_size_dropdown.select(index)
		index += 1
	_map_size_custom_index = index
	map_size_dropdown.add_item(_custom_map_size_label(_map_dimensions))
	map_size_dropdown.set_item_metadata(_map_size_custom_index, {
		"key": "custom",
		"label": "Custom",
		"width": _map_dimensions.x,
		"height": _map_dimensions.y
	})
	if _map_size_key == "custom":
		map_size_dropdown.select(_map_size_custom_index)
	_suppress_map_size_signal = previous

func _set_map_size_selection_from_dimensions(width: int, height: int) -> void:
	if width <= 0 or height <= 0:
		return
	_map_dimensions = Vector2i(width, height)
	var matched_key := ""
	for option in MAP_SIZE_OPTIONS:
		if int(option.get("width", 0)) == width and int(option.get("height", 0)) == height:
			matched_key = String(option.get("key", ""))
			break
	if matched_key == "":
		_map_size_key = "custom"
		if map_size_dropdown != null:
			if _map_size_custom_index < 0 or _map_size_custom_index >= map_size_dropdown.get_item_count():
				_populate_map_size_dropdown()
			var previous := _suppress_map_size_signal
			_suppress_map_size_signal = true
			map_size_dropdown.set_item_text(_map_size_custom_index, _custom_map_size_label(_map_dimensions))
			map_size_dropdown.set_item_metadata(_map_size_custom_index, {
				"key": "custom",
				"label": "Custom",
				"width": width,
				"height": height
			})
			map_size_dropdown.select(_map_size_custom_index)
			_suppress_map_size_signal = previous
	else:
		_map_size_key = matched_key
		_populate_map_size_dropdown()

func _on_map_size_selected(index: int) -> void:
	if map_size_dropdown == null or _suppress_map_size_signal:
		return
	if index < 0 or index >= map_size_dropdown.get_item_count():
		return
	var metadata: Variant = map_size_dropdown.get_item_metadata(index)
	if typeof(metadata) != TYPE_DICTIONARY:
		return
	var descriptor: Dictionary = metadata
	var key: String = String(descriptor.get("key", ""))
	# If the selected key is empty or "custom", do not process further.
	# Custom map sizes are set programmatically and not directly selectable from the dropdown.
	# Provide user feedback to avoid confusion.
	if key == "" or key == "custom":
		push_warning("Custom map sizes must be set via the map size controls, not directly from the dropdown.")
		return
	var width: int = int(descriptor.get("width", 0))
	var height: int = int(descriptor.get("height", 0))
	if width <= 0 or height <= 0:
		return
	if key == _map_size_key and _map_dimensions.x == width and _map_dimensions.y == height:
		_append_log_sink.call("Selected map size '%s' (%dx%d) is already active. No change made." % [key, width, height])
		return
	_map_size_key = key
	_map_dimensions = Vector2i(width, height)
	var label: String = String(descriptor.get("label", key.capitalize()))
	if not _send_map_size_command(width, height, label):
		_append_log_sink.call("Failed to request map size change.")

func _send_map_size_command(width: int, height: int, label: String) -> bool:
	if width <= 0 or height <= 0:
		return false
	var descriptor: String = label if label.strip_edges() != "" else "%dx%d" % [width, height]
	return _send.call(
		"map_size %d %d" % [width, height],
		"%s map (%dx%d) requested." % [descriptor, width, height]
	)

func _on_map_generate_button_pressed() -> void:
	var width: int = _map_dimensions.x
	var height: int = _map_dimensions.y
	if width <= 0 or height <= 0:
		_append_log_sink.call("Map dimensions unavailable; cannot generate a new map.")
		return
	var descriptor := "%s (regenerate)" % _active_map_label()
	if not _send_map_size_command(width, height, descriptor):
		_append_log_sink.call("Failed to request map generation.")
