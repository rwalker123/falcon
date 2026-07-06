extends ScrollContainer
class_name CultureInspectorPanel

## Inspector "Culture" tab. Owns the culture layers, divergence list + detail, and the
## tension readout, and drives MapView's culture-layer highlight.
##
## Snapshot-driven (in _tab_panels): apply_update() ingests culture_layers /
## culture_layer_updates / culture_layer_removed / culture_tensions. Rendering is driven
## by the coordinator via render(resonance) — the influencer-resonance "pushes" line is
## coordinator-mediated (InfluencerPanel.aggregate_resonance(), passed in) so the panels
## stay decoupled. Collaborators: set_map_view (layer highlight) and set_log_hook (new
## tensions are logged to the Logs feed).
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

const CULTURE_TOP_TRAIT_LIMIT = 6
const CULTURE_MAX_DIVERGENCES = 6

@onready var culture_summary_text: RichTextLabel = $CultureVBox/CultureSummarySection/CultureSummaryText
@onready var culture_divergence_list: ItemList = $CultureVBox/CultureDivergenceSection/CultureDivergenceList
@onready var culture_divergence_detail: RichTextLabel = $CultureVBox/CultureDivergenceSection/CultureDivergenceDetail
@onready var culture_tension_text: RichTextLabel = $CultureVBox/CultureTensionSection/CultureTensionText

var _selected_culture_layer_id: int = -1
var _culture_layers: Dictionary = {}
var _culture_tensions: Array[Dictionary] = []
var _culture_tension_tracker: Dictionary = {}
## Influencer-resonance summary pushed in at render time (coordinator-mediated).
var _influencer_resonance: Dictionary = {}
## Pushed by the coordinator; the culture-layer highlight is applied to it.
var _map_view: Node = null
## Logs sink for newly-escalated tensions: (entry: String) -> void.
var _log_hook: Callable = Callable()

func _ready() -> void:
	if culture_divergence_list != null:
		var divergence_callable = Callable(self, "_on_culture_divergence_selected")
		if not culture_divergence_list.is_connected("item_selected", divergence_callable):
			culture_divergence_list.item_selected.connect(_on_culture_divergence_selected)
		if not culture_divergence_list.is_connected("item_activated", divergence_callable):
			culture_divergence_list.item_activated.connect(_on_culture_divergence_selected)

## Coordinator contract: ingest culture snapshot keys. Rendering is driven separately by
## render() so the coordinator can supply the influencer-resonance summary.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	if full_snapshot and data.has("culture_layers"):
		_rebuild_culture_layers(data["culture_layers"])
	elif data.has("culture_layer_updates"):
		_apply_culture_layer_updates(data["culture_layer_updates"])
	if data.has("culture_layer_removed"):
		_remove_culture_layers(data["culture_layer_removed"])
	if data.has("culture_tensions"):
		_update_culture_tensions(data["culture_tensions"], full_snapshot)

## Coordinator driver: render with the current influencer-resonance summary (pulled from
## InfluencerPanel by the coordinator so the panels stay decoupled).
func render(resonance: Dictionary) -> void:
	_influencer_resonance = resonance
	_render_impl()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_culture_layers.clear()
	_culture_tensions.clear()
	_culture_tension_tracker.clear()
	_selected_culture_layer_id = -1
	_influencer_resonance = {}
	_render_impl()

## Coordinator collaborator: the map view the culture-layer highlight is pushed to.
func set_map_view(view: Node) -> void:
	_map_view = view
	if _selected_culture_layer_id >= 0 and _culture_layers.has(_selected_culture_layer_id):
		var layer_variant: Variant = _culture_layers.get(_selected_culture_layer_id)
		if layer_variant is Dictionary:
			_publish_culture_layer_highlight_from_layer(layer_variant as Dictionary)
		else:
			_publish_culture_layer_highlight_from_layer({})
	else:
		_publish_culture_layer_highlight_from_layer({})

## Coordinator collaborator: Logs sink for newly-escalated tensions.
func set_log_hook(hook: Callable) -> void:
	_log_hook = hook

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if culture_summary_text != null:
		Typography.apply(culture_summary_text, Typography.STYLE_BODY)
	if culture_divergence_detail != null:
		Typography.apply(culture_divergence_detail, Typography.STYLE_BODY)
	if culture_tension_text != null:
		Typography.apply(culture_tension_text, Typography.STYLE_BODY)
	if culture_divergence_list != null:
		Typography.apply(culture_divergence_list, Typography.STYLE_BODY)

func _render_impl() -> void:
	if culture_summary_text == null or culture_divergence_list == null or culture_tension_text == null:
		return

	if _culture_layers.is_empty():
		culture_summary_text.text = "[b]Culture[/b]\n[i]No culture data received yet.[/i]"
		culture_divergence_list.clear()
		if culture_divergence_detail != null:
			culture_divergence_detail.text = "[i]Awaiting regional or local layers.[/i]"
		culture_tension_text.text = "[i]No active tensions.[/i]"
		# No layers means no valid selection: clear any stale MapView highlight so a
		# reset()/disconnect or a removal that empties the layer set doesn't leave the
		# previous culture-layer highlight active on the map.
		_publish_culture_layer_highlight_from_layer({})
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
		var divergence_val: float = float(global_layer.get("divergence", 0.0))
		var soft_threshold: float = float(global_layer.get("soft_threshold", 0.0))
		var hard_threshold: float = float(global_layer.get("hard_threshold", 0.0))
		var ticks_soft: int = int(global_layer.get("ticks_above_soft", 0))
		var ticks_hard: int = int(global_layer.get("ticks_above_hard", 0))
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
		summary_lines.append("")
		summary_lines.append("Δ %+.2f | soft %.2f · hard %.2f" % [divergence_val, soft_threshold, hard_threshold])
		summary_lines.append("Ticks above soft: %d · hard: %d" % [ticks_soft, ticks_hard])
	var resonance_summary: Dictionary = _influencer_resonance
	var scope_sequence: Array[String] = ["Global", "Regional", "Local"]
	for scope_key in scope_sequence:
		if not resonance_summary.has(scope_key):
			continue
		var entries_variant: Variant = resonance_summary[scope_key]
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant as Array
		if entries.is_empty():
			continue
		var limit_scope: int = min(entries.size(), 2)
		var fragments: Array[String] = []
		for idx in range(limit_scope):
			var entry_variant: Variant = entries[idx]
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant as Dictionary
			var axis_label: String = str(entry.get("label", entry.get("axis", "Axis")))
			var output_val: float = float(entry.get("output", 0.0))
			fragments.append("%s %+.3f" % [axis_label, output_val])
		if fragments.size() > 0:
			summary_lines.append("%s pushes: %s" % [scope_key, ", ".join(fragments)])
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
		if selection_index == -1 and int(layer_dict.get("id", -1)) == previous_selection:
			selection_index = item_index
	if selection_index >= 0:
		culture_divergence_list.select(selection_index)
	elif culture_divergence_list.get_item_count() > 0:
		if previous_selection >= 0 and _culture_layers.has(previous_selection):
			var ref_dict: Dictionary = _culture_layers[previous_selection]
			_selected_culture_layer_id = int(ref_dict.get("id", -1))
			_publish_culture_layer_highlight_from_layer(ref_dict)
		else:
			culture_divergence_list.select(0)
			var first_meta: Variant = culture_divergence_list.get_item_metadata(0)
			if first_meta is Dictionary:
				var first_dict: Dictionary = first_meta as Dictionary
				_selected_culture_layer_id = int(first_dict.get("id", -1))
				_publish_culture_layer_highlight_from_layer(first_dict)
	else:
		_selected_culture_layer_id = -1
		_publish_culture_layer_highlight_from_layer({})
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
		_publish_culture_layer_highlight_from_layer({})
		return
	var index: int = selected_items[0]
	var meta: Variant = culture_divergence_list.get_item_metadata(index)
	if not (meta is Dictionary):
		culture_divergence_detail.text = "[i]Select a regional or local layer to inspect divergence.[/i]"
		_publish_culture_layer_highlight_from_layer({})
		return
	var layer: Dictionary = meta as Dictionary
	_selected_culture_layer_id = int(layer.get("id", -1))
	_publish_culture_layer_highlight_from_layer(layer)
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

func _publish_culture_layer_highlight_from_layer(layer: Dictionary) -> void:
	if _map_view == null:
		return
	if not _map_view.has_method("set_culture_layer_highlight"):
		return
	if layer.is_empty():
		var empty_ids := PackedInt32Array()
		_map_view.call("set_culture_layer_highlight", empty_ids, "")
		return
	var scope_str := str(layer.get("scope", ""))
	var layer_ids: Array[int] = []
	var context_label: String = ""
	if scope_str == "Regional":
		var region_id: int = int(layer.get("id", -1))
		for entry_variant in _culture_layers.values():
			if entry_variant is Dictionary:
				var entry_dict: Dictionary = entry_variant as Dictionary
				if String(entry_dict.get("scope", "")) != "Local":
					continue
				if int(entry_dict.get("parent", -1)) == region_id:
					layer_ids.append(int(entry_dict.get("id", -1)))
		var local_count: int = layer_ids.size()
		context_label = "Region #%03d (%d locals)" % [region_id, local_count]
	elif scope_str == "Local":
		var local_id: int = int(layer.get("id", -1))
		var parent_id: int = int(layer.get("parent", -1))
		layer_ids.append(local_id)
		if parent_id >= 0:
			context_label = "Local #%03d (Region #%03d)" % [local_id, parent_id]
		else:
			context_label = "Local #%03d" % local_id
	else:
		for entry_variant in _culture_layers.values():
			if entry_variant is Dictionary:
				var entry_dict: Dictionary = entry_variant as Dictionary
				if String(entry_dict.get("scope", "")) == "Local":
					layer_ids.append(int(entry_dict.get("id", -1)))
		context_label = "Global culture selection"
	var packed_ids := PackedInt32Array(layer_ids)
	_map_view.call("set_culture_layer_highlight", packed_ids, context_label)

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

func _rebuild_culture_layers(array_data) -> void:
	var prev_selected: int = _selected_culture_layer_id
	_culture_layers.clear()
	if array_data is Array:
		for entry in array_data:
			var layer_dict: Dictionary = _normalize_culture_layer(entry)
			if layer_dict.is_empty():
				continue
			var id = int(layer_dict.get("id", 0))
			_culture_layers[id] = layer_dict
	if prev_selected >= 0 and _culture_layers.has(prev_selected):
		_selected_culture_layer_id = prev_selected
	else:
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
		if id == _selected_culture_layer_id:
			_publish_culture_layer_highlight_from_layer(layer_dict)

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
			if _log_hook.is_valid():
				_log_hook.call("[color=#ffd166]%s[/color] layer #%03d [%s] severity %.2f (timer %d)" % [
					kind_label,
					layer_id,
					scope_label,
					severity,
					timer_val
				])
			_culture_tension_tracker[key] = timer_val
		else:
			_culture_tension_tracker[key] = max(previous, timer_val)
