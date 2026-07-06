extends VBoxContainer
class_name OverlayInspectorPanel

## The "Map Overlays" section (nested inside the Map tab). Owns the overlay-channel
## selector (built at runtime), the channel metadata, and the culture/military overlay
## readouts, and drives the MapView overlay channel.
##
## Command/tab collaborator, not a snapshot-fan-out panel: the coordinator routes the
## overlay payload here via ingest() (after re-homing the palette → Terrain and
## crisis_annotations → Crisis side-routes that share the `overlays` key), and pushes the
## MapView handle via set_map_view(). The tag-overlay channel depends on Terrain's tag
## labels, so the coordinator passes them into ingest().
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

@onready var terrain_overlay_section_label: Label = $OverlaySectionLabel
@onready var terrain_overlay_tabs: TabContainer = $OverlayTabs
@onready var terrain_overlay_culture_placeholder: RichTextLabel = $OverlayTabs/Culture/CulturePlaceholder
@onready var terrain_overlay_military_placeholder: RichTextLabel = $OverlayTabs/Military/MilitaryPlaceholder

var _overlay_selector: OptionButton = null
var _overlay_channel_labels: Dictionary = {}
var _overlay_channel_descriptions: Dictionary = {}
var _overlay_channel_order: Array = []
var _overlay_placeholder_flags: Dictionary = {}
var _selected_overlay_key: String = ""
## Pushed by the coordinator; the overlay channel is applied to it.
var _map_view: Node = null
## Terrain's tag labels (Terrain-owned), passed in via ingest() so the tag overlay can
## report availability.
var _terrain_tag_labels: Dictionary = {}

func _ready() -> void:
	_ensure_overlay_selector()

## Coordinator collaborator: the map view the overlay channel is pushed to.
func set_map_view(view: Node) -> void:
	_map_view = view
	_apply_overlay_selection_to_map()
	_refresh_overlay_panels()

## Coordinator collaborator: ingest the overlay payload (channels + metadata). The
## coordinator passes Terrain's tag labels so the tag overlay reports availability.
func ingest(overlay_dict: Dictionary, terrain_tag_labels: Dictionary) -> void:
	# Deep-copy: the coordinator owns this Dictionary, so we must not hold a live
	# reference into it (reset() clears our copy, which would otherwise wipe
	# Terrain-owned state through the shared reference).
	_terrain_tag_labels = terrain_tag_labels.duplicate(true)
	_update_overlay_channels(overlay_dict)

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_overlay_channel_labels.clear()
	_overlay_channel_descriptions.clear()
	_overlay_channel_order.clear()
	_overlay_placeholder_flags.clear()
	_selected_overlay_key = ""
	# Fresh Dictionary rather than clearing in place — see ingest(): this is our own
	# copy, but replacing keeps the "never mutate shared state" invariant obvious.
	_terrain_tag_labels = {}
	_refresh_overlay_selector()
	_update_overlay_section_text()
	# Push the cleared channel to the map so a disconnect/reset drops the stale
	# overlay instead of leaving it painted on the map.
	_apply_overlay_selection_to_map()
	_refresh_overlay_panels()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if terrain_overlay_culture_placeholder != null:
		Typography.apply(terrain_overlay_culture_placeholder, Typography.STYLE_BODY)
	if terrain_overlay_military_placeholder != null:
		Typography.apply(terrain_overlay_military_placeholder, Typography.STYLE_BODY)
	if terrain_overlay_section_label != null:
		Typography.apply(terrain_overlay_section_label, Typography.STYLE_HEADING)
	if terrain_overlay_tabs != null:
		Typography.apply(terrain_overlay_tabs, Typography.STYLE_CONTROL)
	if _overlay_selector != null:
		Typography.apply(_overlay_selector, Typography.STYLE_CONTROL)

func _ensure_overlay_selector() -> void:
	if _overlay_selector != null:
		return
	if terrain_overlay_section_label == null:
		return
	var container: Node = terrain_overlay_section_label.get_parent()
	if container == null:
		return
	_overlay_selector = OptionButton.new()
	_overlay_selector.name = "OverlaySelector"
	_overlay_selector.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_overlay_selector.focus_mode = Control.FOCUS_ALL
	container.add_child(_overlay_selector)
	if terrain_overlay_tabs != null:
		var children: Array = container.get_children()
		var target_index: int = children.find(terrain_overlay_tabs)
		if target_index >= 0:
			container.move_child(_overlay_selector, target_index)
	if not _overlay_selector.is_connected("item_selected", Callable(self, "_on_overlay_channel_selected")):
		_overlay_selector.item_selected.connect(_on_overlay_channel_selected)
	_overlay_selector.visible = false

func _update_overlay_channels(overlay_dict: Dictionary) -> void:
	_ensure_overlay_selector()
	_overlay_channel_labels.clear()
	_overlay_channel_descriptions.clear()
	_overlay_channel_order.clear()
	_overlay_placeholder_flags.clear()

	if overlay_dict.has("channels"):
		var channels_variant: Variant = overlay_dict["channels"]
		if channels_variant is Dictionary:
			var channels: Dictionary = channels_variant
			for raw_key in channels.keys():
				var key := String(raw_key)
				var info_variant: Variant = channels[raw_key]
				if not (info_variant is Dictionary):
					continue
				var info: Dictionary = info_variant
				_overlay_channel_labels[key] = String(info.get("label", key.capitalize()))
				_overlay_channel_descriptions[key] = String(info.get("description", ""))
				_overlay_placeholder_flags[key] = bool(info.get("placeholder", false))

	var placeholder_variant: Variant = overlay_dict.get("placeholder_channels", PackedStringArray())
	if placeholder_variant is PackedStringArray:
		var placeholder_array: PackedStringArray = placeholder_variant
		for raw_placeholder_key in placeholder_array:
			var placeholder_key := String(raw_placeholder_key)
			_overlay_placeholder_flags[placeholder_key] = true

	var order_variant: Variant = overlay_dict.get("channel_order", PackedStringArray())
	_overlay_channel_order.clear()
	if order_variant is PackedStringArray:
		var order_array: PackedStringArray = order_variant
		for raw_channel_key in order_array:
			_overlay_channel_order.append(String(raw_channel_key))
	if _overlay_channel_order.is_empty():
		var keys: Array = _overlay_channel_labels.keys()
		keys.sort()
		_overlay_channel_order = keys

	var tag_overlay_available: bool = overlay_dict.has("terrain_tags") or not _terrain_tag_labels.is_empty()
	if tag_overlay_available:
		_overlay_channel_labels["terrain_tags"] = "Terrain Tags"
		_overlay_channel_descriptions["terrain_tags"] = "Colors tiles by environmental tags (blends when multiple tags apply)."
		_overlay_placeholder_flags["terrain_tags"] = false
		var has_tag_channel := false
		for raw_key in _overlay_channel_order:
			if String(raw_key) == "terrain_tags":
				has_tag_channel = true
				break
		if not has_tag_channel:
			_overlay_channel_order.append("terrain_tags")

	# Always provide a "No Overlay" entry so users can clear overlays without special keys.
	_overlay_channel_labels[""] = "No Overlay"
	_overlay_channel_descriptions[""] = "Base map without overlays."
	_overlay_placeholder_flags[""] = false
	if _overlay_channel_order.find("") == -1:
		_overlay_channel_order.push_front("")

	var default_variant: Variant = overlay_dict.get("default_channel", _selected_overlay_key)
	var default_key: String = String(default_variant)
	if not _overlay_channel_labels.has(_selected_overlay_key):
		if _overlay_channel_labels.has(default_key):
			_selected_overlay_key = default_key
		elif _overlay_channel_order.size() > 0:
			_selected_overlay_key = String(_overlay_channel_order[0])
		else:
			var keys_array: Array = _overlay_channel_labels.keys()
			_selected_overlay_key = String(keys_array[0])

	_refresh_overlay_selector()
	_update_overlay_section_text()
	_apply_overlay_selection_to_map()
	_refresh_overlay_panels()

func _refresh_overlay_selector() -> void:
	if _overlay_selector == null:
		return
	_overlay_selector.clear()
	if _overlay_channel_labels.is_empty():
		_overlay_selector.hide()
		return
	_overlay_selector.show()
	var index := 0
	var selected := false
	for key in _overlay_channel_order:
		if not _overlay_channel_labels.has(key):
			continue
		var label: String = _overlay_channel_labels[key]
		if bool(_overlay_placeholder_flags.get(key, false)):
			label += " (stub)"
		_overlay_selector.add_item(label)
		_overlay_selector.set_item_metadata(index, key)
		if _overlay_channel_descriptions.has(key):
			var tooltip: String = String(_overlay_channel_descriptions[key])
			if tooltip != "":
				_overlay_selector.set_item_tooltip(index, tooltip)
		if key == _selected_overlay_key:
			_overlay_selector.select(index)
			selected = true
		index += 1
	if index == 0:
		_overlay_selector.hide()
		return
	if not selected:
		var fallback_index := -1
		var index_empty := -1
		for i in range(_overlay_selector.get_item_count()):
			var metadata: Variant = _overlay_selector.get_item_metadata(i)
			var key := String(metadata)
			if key == "":
				index_empty = i
				break
			if fallback_index == -1:
				fallback_index = i
		var choose_index := index_empty if index_empty >= 0 else fallback_index
		if choose_index >= 0:
			_overlay_selector.select(choose_index)
			var metadata: Variant = _overlay_selector.get_item_metadata(choose_index)
			_selected_overlay_key = String(metadata)

func _apply_overlay_selection_to_map() -> void:
	if _map_view == null:
		return
	if _map_view.has_method("set_overlay_channel"):
		_map_view.call("set_overlay_channel", _selected_overlay_key)

func _update_overlay_section_text() -> void:
	if terrain_overlay_section_label == null:
		return
	if _overlay_channel_labels.is_empty():
		terrain_overlay_section_label.text = "Future Overlays"
		terrain_overlay_section_label.tooltip_text = ""
		return
	var text := "Map Overlays"
	var tooltip := ""
	if _overlay_channel_labels.has(_selected_overlay_key):
		text += " — %s" % _overlay_channel_labels[_selected_overlay_key]
		if bool(_overlay_placeholder_flags.get(_selected_overlay_key, false)):
			text += " (stub data)"
		if _overlay_channel_descriptions.has(_selected_overlay_key):
			tooltip = String(_overlay_channel_descriptions[_selected_overlay_key])
	terrain_overlay_section_label.text = text
	terrain_overlay_section_label.tooltip_text = tooltip

func _on_overlay_channel_selected(index: int) -> void:
	if _overlay_selector == null:
		return
	var metadata: Variant = _overlay_selector.get_item_metadata(index)
	var key := String(metadata)
	if key == _selected_overlay_key:
		return
	_selected_overlay_key = key
	_update_overlay_section_text()
	_apply_overlay_selection_to_map()

func _overlay_stats_for_key(key: String) -> Dictionary:
	if _map_view == null:
		return {}
	if not _map_view.has_method("overlay_stats_for_key"):
		return {}
	var result: Variant = _map_view.call("overlay_stats_for_key", key)
	if result is Dictionary:
		return result as Dictionary
	return {}

func _overlay_panel_text(key: String, title: String, description: String) -> String:
	var lines: Array[String] = []
	lines.append("[b]%s[/b]" % title)
	if description != "":
		lines.append(description)
	if _map_view == null:
		lines.append("[i]Overlay data unavailable.[/i]")
		return "\n".join(lines)
	if bool(_overlay_placeholder_flags.get(key, false)):
		lines.append("[i]Channel awaiting telemetry.[/i]")
		return "\n".join(lines)
	var stats: Dictionary = _overlay_stats_for_key(key)
	if stats.is_empty() or not bool(stats.get("has_values", false)):
		lines.append("[i]No samples received yet.[/i]")
		return "\n".join(lines)
	var raw_min: float = float(stats.get("raw_min", 0.0))
	var raw_avg: float = float(stats.get("raw_avg", 0.0))
	var raw_max: float = float(stats.get("raw_max", 0.0))
	var normalized_avg: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
	lines.append("Raw min %.3f · avg %.3f · max %.3f" % [raw_min, raw_avg, raw_max])
	lines.append("Normalized avg %.1f%%" % (normalized_avg * 100.0))
	return "\n".join(lines)

func _refresh_overlay_panels() -> void:
	if terrain_overlay_culture_placeholder != null:
		terrain_overlay_culture_placeholder.text = _overlay_panel_text(
			"culture",
			"Culture Divergence",
			"Highlights divergence pressure relative to schism thresholds.",
		)
	if terrain_overlay_military_placeholder != null:
		terrain_overlay_military_placeholder.text = _overlay_panel_text(
			"military",
			"Force Readiness",
			"Composite of garrison morale, manpower, and supply margin.",
		)
