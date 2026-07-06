extends ScrollContainer
class_name GreatDiscoveriesInspectorPanel

## Inspector "Great Discoveries" tab. Owns the constellation ledger, per-faction
## progress, the definition catalog, and their detail views. Fully self-contained:
## the only coordinator collaborators are the contract methods apply_update()/reset(),
## set_available() (capability gating), and apply_typography(). No command, log, or
## MapView coupling.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

@onready var _summary_label: Label = %GreatDiscoverySummaryLabel
@onready var _summary_text: RichTextLabel = %GreatDiscoverySummaryText
@onready var _definitions_list: ItemList = %GreatDiscoveryDefinitionsList
@onready var _ledger_label: Label = %GreatDiscoveryLedgerLabel
@onready var _ledger_list: ItemList = %GreatDiscoveryLedgerList
@onready var _ledger_detail: RichTextLabel = %GreatDiscoveryLedgerDetail
@onready var _progress_label: Label = %GreatDiscoveryProgressLabel
@onready var _progress_list: ItemList = %GreatDiscoveryProgressList
@onready var _progress_detail: RichTextLabel = %GreatDiscoveryProgressDetail

var _records: Dictionary = {}
var _progress_map: Dictionary = {}
var _telemetry: Dictionary = {}
var _selected_key: String = ""
var _selected_progress_key: String = ""
var _definitions: Dictionary = {}
var _selected_definition_id: int = -1
var _suppress_definition_signal: bool = false
var _definitions_warned: bool = false
## Whether the Megaprojects capability is unlocked. The tab stays clickable; when
## locked it explains how it unlocks instead of being disabled.
var _available: bool = true

func _ready() -> void:
	if _ledger_list != null:
		_ledger_list.item_selected.connect(_on_ledger_selected)
		_ledger_list.item_activated.connect(_on_ledger_selected)
	if _progress_list != null:
		_progress_list.item_selected.connect(_on_progress_selected)
		_progress_list.item_activated.connect(_on_progress_selected)
	if _definitions_list != null:
		_definitions_list.item_selected.connect(_on_definition_selected)
		_definitions_list.item_activated.connect(_on_definition_selected)
	_render()

## Coordinator contract: ingest a full snapshot or delta; re-render if anything changed.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	var dirty := false
	if full_snapshot and data.has("great_discovery_definitions"):
		_set_definitions(data["great_discovery_definitions"])
		dirty = true
	if full_snapshot and data.has("great_discoveries"):
		_rebuild_records(data["great_discoveries"])
		dirty = true
	elif data.has("great_discovery_updates"):
		_merge_updates(data["great_discovery_updates"])
		dirty = true
	if full_snapshot and data.has("great_discovery_progress"):
		_rebuild_progress(data["great_discovery_progress"])
		dirty = true
	elif data.has("great_discovery_progress_updates"):
		_merge_progress(data["great_discovery_progress_updates"])
		dirty = true
	if data.has("great_discovery_telemetry"):
		_set_telemetry(data["great_discovery_telemetry"])
		dirty = true
	if dirty:
		_render()

## Coordinator contract: drop all state to the pre-snapshot placeholder view.
func reset() -> void:
	_records.clear()
	_progress_map.clear()
	_telemetry.clear()
	_selected_key = ""
	_selected_progress_key = ""
	_selected_definition_id = -1
	if not _available:
		# Capability gating can run before the static-section reset; keep the locked
		# explanation instead of overwriting it with the placeholder view.
		_render_locked()
		return
	if _summary_text != null:
		_summary_text.text = "[b]Great Discoveries[/b]\n[i]Awaiting snapshot data.[/i]"
	if _definitions_list != null:
		_definitions_list.clear()
		_definitions_list.add_item("All Discoveries")
		_definitions_list.set_item_metadata(0, -1)
		_definitions_list.select(0)
	if _ledger_list != null:
		_ledger_list.clear()
	if _ledger_detail != null:
		_ledger_detail.text = "[i]Select a resolved discovery to inspect its details.[/i]"
	if _progress_list != null:
		_progress_list.clear()
	if _progress_detail != null:
		_progress_detail.text = "[i]Pending constellations will appear here once telemetry arrives.[/i]"

## Coordinator contract (capability-gated): stay clickable; render a locked
## explanation while the Megaprojects capability is unavailable.
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	_style(_summary_text, Typography.STYLE_BODY)
	_style(_ledger_detail, Typography.STYLE_BODY)
	_style(_progress_detail, Typography.STYLE_BODY)
	_style(_summary_label, Typography.STYLE_HEADING)
	_style(_ledger_label, Typography.STYLE_HEADING)
	_style(_progress_label, Typography.STYLE_HEADING)
	_style(_definitions_list, Typography.STYLE_BODY)
	_style(_ledger_list, Typography.STYLE_BODY)
	_style(_progress_list, Typography.STYLE_BODY)

func _style(control: Control, style: StringName) -> void:
	if control != null:
		Typography.apply(control, style)

# --- ingestion --------------------------------------------------------------

func _rebuild_records(array_data) -> void:
	_records.clear()
	_merge_updates(array_data)

func _merge_updates(array_data) -> void:
	if array_data == null:
		return
	if array_data is Array:
		for entry in array_data:
			if entry is Dictionary:
				_apply_entry(entry as Dictionary)
	_ensure_selection_valid()

func _apply_entry(entry: Dictionary) -> void:
	var info: Dictionary = entry.duplicate(true)
	var faction: int = int(info.get("faction", -1))
	var discovery_id: int = int(info.get("id", -1))
	if faction < 0 or discovery_id < 0:
		return
	var key := "%d:%d" % [faction, discovery_id]
	_records[key] = info

func _ensure_selection_valid() -> void:
	if _selected_key != "" and not _records.has(_selected_key):
		_selected_key = ""

func _rebuild_progress(array_data) -> void:
	_progress_map.clear()
	_merge_progress(array_data)

func _merge_progress(array_data) -> void:
	if array_data == null:
		return
	if array_data is Array:
		for entry in array_data:
			if entry is Dictionary:
				_apply_progress_entry(entry as Dictionary)
	_ensure_progress_selection_valid()

func _apply_progress_entry(entry: Dictionary) -> void:
	var info: Dictionary = entry.duplicate(true)
	var faction: int = int(info.get("faction", -1))
	var discovery_id: int = int(info.get("discovery", -1))
	if faction < 0 or discovery_id < 0:
		return
	if not _progress_map.has(faction):
		_progress_map[faction] = {}
	var faction_dict: Dictionary = _progress_map[faction]
	faction_dict[discovery_id] = info

func _ensure_progress_selection_valid() -> void:
	if _selected_progress_key == "":
		return
	var entry := _get_progress_entry_by_key(_selected_progress_key)
	if entry.is_empty():
		_selected_progress_key = ""

func _set_telemetry(value) -> void:
	if value is Dictionary:
		_telemetry = (value as Dictionary).duplicate(true)

func _ensure_definition_selection_valid() -> void:
	if _selected_definition_id >= 0 and not _definitions.has(_selected_definition_id):
		_selected_definition_id = -1

func _set_definitions(definitions_variant: Variant) -> void:
	_definitions.clear()
	if definitions_variant is Array:
		for entry in definitions_variant:
			if entry is Dictionary:
				var info: Dictionary = (entry as Dictionary).duplicate(true)
				var discovery_id: int = int(info.get("id", -1))
				if discovery_id >= 0:
					_definitions[discovery_id] = info
	elif definitions_variant is Dictionary:
		var definitions_dict: Dictionary = definitions_variant
		for key in definitions_dict.keys():
			var value: Variant = definitions_dict[key]
			if value is Dictionary:
				var info: Dictionary = (value as Dictionary).duplicate(true)
				var discovery_id: int = int(info.get("id", int(key)))
				if discovery_id >= 0:
					_definitions[discovery_id] = info
	if _definitions.is_empty():
		if not _definitions_warned:
			push_warning("Great Discovery definition catalog is empty; awaiting server metadata.")
			_definitions_warned = true
	else:
		_definitions_warned = false
	_ensure_definition_selection_valid()

func _on_definition_selected(index: int) -> void:
	if _suppress_definition_signal:
		return
	if _definitions_list == null:
		return
	if index < 0 or index >= _definitions_list.get_item_count():
		_selected_definition_id = -1
		_render()
		return
	var meta: Variant = _definitions_list.get_item_metadata(index)
	if typeof(meta) == TYPE_INT:
		_selected_definition_id = int(meta)
	else:
		_selected_definition_id = -1
	_render()

# --- render ------------------------------------------------------------------

func _render() -> void:
	if not _available:
		_render_locked()
		return
	if _summary_text == null:
		return

	var summary_lines: Array[String] = ["[b]Great Discoveries[/b]"]
	var resolved_count: int = int(_telemetry.get("total_resolved", _records.size()))
	var pending_candidates: int = int(_telemetry.get("pending_candidates", 0))
	var active_constellations: int = int(_telemetry.get("active_constellations", 0))
	summary_lines.append("Resolved discoveries: %d" % resolved_count)
	summary_lines.append("Pending candidates: %d" % pending_candidates)
	summary_lines.append("Active constellations: %d" % active_constellations)
	var definition_filter := _selected_definition_id
	var faction_overview := _summarize_progress_by_faction(definition_filter)
	if faction_overview.is_empty():
		summary_lines.append("[i]No factions are actively pursuing Great Discoveries.[/i]")
	else:
		for faction_line in faction_overview:
			summary_lines.append(faction_line)

	var records := _collect_sorted_records()
	if records.is_empty():
		summary_lines.append("[i]No discoveries have been resolved yet.[/i]")
	else:
		var preview: Array[String] = []
		for record in records:
			var record_id: int = int(record.get("id", -1))
			if definition_filter >= 0 and record_id != definition_filter:
				continue
			preview.append(_format_record(record))
			if preview.size() >= 3:
				break
		if not preview.is_empty():
			summary_lines.append("Latest: %s" % ", ".join(preview))

	_summary_text.text = "\n".join(summary_lines)

	if _definitions_list != null:
		_suppress_definition_signal = true
		var previous_definition := _selected_definition_id
		_definitions_list.clear()
		_definitions_list.add_item("All Discoveries")
		_definitions_list.set_item_metadata(0, -1)
		var selected_definition_index := 0
		var sorted_definition_ids: Array = _definitions.keys()
		sorted_definition_ids.sort()
		var list_index := 1
		for id in sorted_definition_ids:
			var int_id: int = int(id)
			var label := _definition_label_for_id(int_id)
			_definitions_list.add_item(label)
			_definitions_list.set_item_metadata(list_index, int_id)
			if int_id == previous_definition:
				selected_definition_index = list_index
			list_index += 1
		_definitions_list.select(selected_definition_index)
		var meta: Variant = _definitions_list.get_item_metadata(selected_definition_index)
		_selected_definition_id = int(meta) if typeof(meta) == TYPE_INT else -1
		_suppress_definition_signal = false

	if _ledger_list != null:
		var previous_key := _selected_key
		_ledger_list.clear()
		var selected_index: int = -1
		var row_index := 0
		for record in records:
			var discovery_id: int = int(record.get("id", -1))
			if definition_filter >= 0 and discovery_id != definition_filter:
				continue
			var label := _format_record(record)
			_ledger_list.add_item(label)
			_ledger_list.set_item_metadata(row_index, record)
			if String(record.get("_key", "")) == previous_key:
				selected_index = row_index
			row_index += 1
		if selected_index >= 0:
			_ledger_list.select(selected_index)
			_on_ledger_selected(selected_index)
		else:
			_selected_key = ""
			_update_ledger_detail()

	var progress_entries := _collect_sorted_progress()
	if _progress_list != null:
		var previous_progress_key := _selected_progress_key
		_progress_list.clear()
		var selected_progress_index: int = -1
		var progress_row_index := 0
		for entry in progress_entries:
			var discovery_id: int = int(entry.get("discovery", -1))
			if definition_filter >= 0 and discovery_id != definition_filter:
				continue
			var label := _format_progress_entry(entry)
			_progress_list.add_item(label)
			_progress_list.set_item_metadata(progress_row_index, entry)
			if String(entry.get("_key", "")) == previous_progress_key:
				selected_progress_index = progress_row_index
			progress_row_index += 1
		if selected_progress_index >= 0:
			_progress_list.select(selected_progress_index)
			_on_progress_selected(selected_progress_index)
		else:
			_selected_progress_key = ""
			_update_progress_detail()

func _render_locked() -> void:
	if _summary_text != null:
		_summary_text.text = "[b]Great Discoveries[/b]\n[i]🔒 Locked — the Great Discovery ledger comes online once your civilization reaches the Megaprojects tier.[/i]"
	if _definitions_list != null:
		_definitions_list.clear()
	if _ledger_list != null:
		_ledger_list.clear()
	if _ledger_detail != null:
		_ledger_detail.text = "[i]The constellation ledger unlocks with the Megaprojects capability.[/i]"
	if _progress_list != null:
		_progress_list.clear()
	if _progress_detail != null:
		_progress_detail.text = "[i]Constellation progress appears here once Megaprojects unlocks.[/i]"

func _collect_sorted_records() -> Array:
	var records: Array = []
	for key in _records.keys():
		var record_variant: Variant = _records[key]
		if record_variant is Dictionary:
			var record: Dictionary = (record_variant as Dictionary).duplicate(true)
			record["_key"] = String(key)
			records.append(record)
	records.sort_custom(Callable(self, "_compare_records"))
	return records

func _compare_records(a: Dictionary, b: Dictionary) -> bool:
	var tick_a: int = int(a.get("tick", 0))
	var tick_b: int = int(b.get("tick", 0))
	if tick_a == tick_b:
		var faction_a: int = int(a.get("faction", 0))
		var faction_b: int = int(b.get("faction", 0))
		if faction_a == faction_b:
			return int(a.get("id", 0)) < int(b.get("id", 0))
		return faction_a < faction_b
	return tick_a > tick_b

func _collect_sorted_progress() -> Array:
	var entries: Array = []
	for faction_key in _progress_map.keys():
		var faction_int: int = int(faction_key)
		var faction_variant: Variant = _progress_map[faction_key]
		if not (faction_variant is Dictionary):
			continue
		var faction_dict: Dictionary = faction_variant
		for discovery_key in faction_dict.keys():
			var info_variant: Variant = faction_dict[discovery_key]
			if not (info_variant is Dictionary):
				continue
			var entry: Dictionary = (info_variant as Dictionary).duplicate(true)
			entry["faction"] = faction_int
			entry["discovery"] = int(discovery_key)
			entry["_key"] = "%d:%d" % [faction_int, int(discovery_key)]
			entries.append(entry)
	entries.sort_custom(Callable(self, "_compare_progress"))
	return entries

func _compare_progress(a: Dictionary, b: Dictionary) -> bool:
	var progress_a: float = float(a.get("progress", 0.0))
	var progress_b: float = float(b.get("progress", 0.0))
	if is_equal_approx(progress_a, progress_b):
		var deficit_a: int = int(a.get("observation_deficit", 0))
		var deficit_b: int = int(b.get("observation_deficit", 0))
		if deficit_a == deficit_b:
			return int(a.get("eta_ticks", 0)) < int(b.get("eta_ticks", 0))
		return deficit_a < deficit_b
	return progress_a > progress_b

func _format_record(record: Dictionary) -> String:
	var faction: int = int(record.get("faction", -1))
	var discovery_id: int = int(record.get("id", -1))
	var tick: int = int(record.get("tick", 0))
	var label: String = _definition_label_for_id(discovery_id)
	var deployment_tag: String = "Public" if bool(record.get("publicly_deployed", false)) else "Classified"
	return "%s — F%d (T%s, %s)" % [
		label,
		faction,
		str(tick),
		deployment_tag
	]

func _format_progress_entry(entry: Dictionary) -> String:
	var faction: int = int(entry.get("faction", -1))
	var discovery_id: int = int(entry.get("discovery", -1))
	var progress_percent: float = float(entry.get("progress", 0.0)) * 100.0
	var deficit: int = int(entry.get("observation_deficit", 0))
	var eta: int = int(entry.get("eta_ticks", 0))
	var covert_label: String = "Covert" if bool(entry.get("covert", false)) else "Visible"
	return "%s — F%d :: %.1f%% (obs-%d, ETA %s, %s)" % [
		_definition_label_for_id(discovery_id),
		faction,
		progress_percent,
		deficit,
		"—" if eta <= 0 else str(eta),
		covert_label
	]

# --- definition catalog helpers ---------------------------------------------

func _definition_name_for_id(discovery_id: int) -> String:
	if _definitions.has(discovery_id):
		var info: Dictionary = _definitions[discovery_id]
		return String(info.get("name", "Discovery %d" % discovery_id))
	return "Discovery %d" % discovery_id

func _definition_label_for_id(discovery_id: int) -> String:
	var name := _definition_name_for_id(discovery_id)
	return "%s (D%d)" % [name, discovery_id]

func _definition_metadata_for_id(discovery_id: int) -> Dictionary:
	if _definitions.has(discovery_id):
		var entry_variant: Variant = _definitions[discovery_id]
		if entry_variant is Dictionary:
			return entry_variant
	return {}

func _definition_tags_text(discovery_id: int) -> String:
	var metadata := _definition_metadata_for_id(discovery_id)
	if metadata.is_empty():
		return ""
	var tags_variant: Variant = metadata.get("tags", [])
	var tags: Array[String] = []
	if tags_variant is Array:
		for tag in tags_variant:
			tags.append(String(tag))
	elif tags_variant is PackedStringArray:
		var packed: PackedStringArray = tags_variant
		for tag in packed:
			tags.append(String(tag))
	return ", ".join(tags)

func _definition_int(metadata: Dictionary, key: String) -> int:
	if not metadata.has(key):
		return -1
	var value: Variant = metadata[key]
	var value_type := typeof(value)
	if value_type == TYPE_INT or value_type == TYPE_FLOAT:
		return int(value)
	if value_type == TYPE_STRING:
		return int(value)
	return -1

func _definition_bool(metadata: Dictionary, key: String, default_value: bool = false) -> bool:
	if not metadata.has(key):
		return default_value
	var value: Variant = metadata[key]
	var value_type := typeof(value)
	if value_type == TYPE_BOOL:
		return bool(value)
	if value_type == TYPE_INT:
		return int(value) != 0
	if value_type == TYPE_STRING:
		var text := String(value).to_lower()
		return text == "true" or text == "1" or text == "yes"
	return default_value

func _format_definition_requirements(discovery_id: int) -> Array[String]:
	var metadata := _definition_metadata_for_id(discovery_id)
	var requirements_variant: Variant = metadata.get("requirements", [])
	var lines: Array[String] = []
	if requirements_variant is Array:
		for req_variant in requirements_variant:
			if not (req_variant is Dictionary):
				continue
			var req: Dictionary = req_variant
			var req_id: int = int(req.get("discovery_id", -1))
			var req_name: String = String(req.get("name", "Discovery %d" % req_id))
			var min_progress: float = float(req.get("minimum_progress", 0.0))
			var weight: float = float(req.get("weight", 1.0))
			var summary_text: String = String(req.get("summary", ""))
			var min_percent := min_progress * 100.0
			var id_label := "unknown"
			if req_id >= 0:
				id_label = "d%d" % req_id
			var entry := "• %s (%s) — min %.0f%%, weight %.2f" % [
				req_name,
				id_label,
				min_percent,
				weight
			]
			lines.append(entry)
			if not summary_text.is_empty():
				lines.append("    %s" % summary_text)
	return lines

# --- selection + detail ------------------------------------------------------

func _on_ledger_selected(index: int) -> void:
	if _ledger_list == null:
		return
	if index < 0 or index >= _ledger_list.get_item_count():
		_selected_key = ""
		_update_ledger_detail()
		return
	var meta: Variant = _ledger_list.get_item_metadata(index)
	if meta is Dictionary:
		_selected_key = String((meta as Dictionary).get("_key", ""))
	else:
		_selected_key = ""
	_update_ledger_detail()

func _on_progress_selected(index: int) -> void:
	if _progress_list == null:
		return
	if index < 0 or index >= _progress_list.get_item_count():
		_selected_progress_key = ""
		_update_progress_detail()
		return
	var meta: Variant = _progress_list.get_item_metadata(index)
	if meta is Dictionary:
		_selected_progress_key = String((meta as Dictionary).get("_key", ""))
	else:
		_selected_progress_key = ""
	_update_progress_detail()

func _summarize_progress_by_faction(filter_definition: int) -> Array[String]:
	var lines: Array[String] = []
	var faction_keys := _progress_map.keys()
	if faction_keys.is_empty():
		return lines
	faction_keys.sort()
	for faction_key in faction_keys:
		var faction_int: int = int(faction_key)
		var faction_variant: Variant = _progress_map[faction_key]
		if not (faction_variant is Dictionary):
			continue
		var faction_dict: Dictionary = faction_variant
		if faction_dict.keys().is_empty():
			continue
		var entries: Array[Dictionary] = []
		for discovery_key in faction_dict.keys():
			var info_variant: Variant = faction_dict[discovery_key]
			if info_variant is Dictionary:
				var info: Dictionary = (info_variant as Dictionary).duplicate(true)
				var discovery_id: int = int(discovery_key)
				if filter_definition >= 0 and discovery_id != filter_definition:
					continue
				info["discovery"] = discovery_id
				entries.append(info)
		if entries.is_empty():
			continue
		entries.sort_custom(Callable(self, "_compare_progress"))
		var fragments: Array[String] = []
		var limit: int = min(entries.size(), 3)
		for idx in range(limit):
			var entry := entries[idx]
			var discovery_id: int = int(entry.get("discovery", -1))
			var progress_percent: float = float(entry.get("progress", 0.0)) * 100.0
			var deficit: int = int(entry.get("observation_deficit", 0))
			var eta: int = int(entry.get("eta_ticks", 0))
			var flash: String = "ready" if eta <= 0 and deficit <= 0 else "eta %s" % ("now" if eta <= 0 else str(eta))
			fragments.append("%s %.0f%% (%s)" % [_definition_name_for_id(discovery_id), progress_percent, flash])
		lines.append("Faction %d: %s" % [faction_int, ", ".join(fragments)])
	return lines

func _update_ledger_detail() -> void:
	if _ledger_detail == null:
		return
	if _selected_key == "" or not _records.has(_selected_key):
		_ledger_detail.text = "[i]Select a resolved discovery to inspect its details.[/i]"
		return
	var record_variant: Variant = _records[_selected_key]
	if not (record_variant is Dictionary):
		_ledger_detail.text = "[i]Select a resolved discovery to inspect its details.[/i]"
		return
	var record: Dictionary = record_variant
	var id: int = int(record.get("id", -1))
	var faction: int = int(record.get("faction", -1))
	var tick: int = int(record.get("tick", 0))
	var field_label: String = String(record.get("field_label", ""))
	if field_label.is_empty():
		field_label = "Field %s" % String(record.get("field", ""))
	var definition_name := _definition_name_for_id(id)
	var deployed := bool(record.get("publicly_deployed", false))
	var effects_variant: Variant = record.get("effects", PackedStringArray())
	var effect_labels: Array[String] = []
	if effects_variant is PackedStringArray:
		for effect_label in (effects_variant as PackedStringArray):
			effect_labels.append(String(effect_label))
	var effect_text: String = ", ".join(effect_labels)
	if effect_text.is_empty():
		effect_text = "None"
	var lines: Array[String] = []
	lines.append("[b]%s[/b] — Faction %d" % [_definition_label_for_id(id), faction])
	lines.append("Name: %s" % definition_name)
	lines.append("Field: %s" % field_label)
	lines.append("Resolved on tick %d" % tick)
	lines.append("Deployment: %s" % ("Publicly deployed" if deployed else "Classified ledger entry"))
	lines.append("Effects: %s" % effect_text)

	var metadata := _definition_metadata_for_id(id)
	if not metadata.is_empty():
		var tag_text := _definition_tags_text(id)
		if not tag_text.is_empty():
			lines.append("Tags: %s" % tag_text)

		var gate_value := _definition_int(metadata, "observation_threshold")
		if gate_value >= 0:
			lines.append("Observation Gate: %d verified signals" % gate_value)

		var cadence_bits: Array[String] = []
		var cooldown_value := _definition_int(metadata, "cooldown_ticks")
		if cooldown_value >= 0:
			cadence_bits.append("cooldown %d ticks" % cooldown_value)
		var freshness_value := _definition_int(metadata, "freshness_window")
		if freshness_value > 0:
			cadence_bits.append("freshness window %d ticks" % freshness_value)
		if _definition_bool(metadata, "covert_until_public", false):
			cadence_bits.append("covert until public")
		if not cadence_bits.is_empty():
			lines.append("Cadence: %s" % ", ".join(cadence_bits))

		var summary_text := String(metadata.get("summary", ""))
		if not summary_text.is_empty():
			lines.append("")
			lines.append("[b]Summary[/b]")
			lines.append(summary_text)

		var catalog_effects_variant: Variant = metadata.get("effects_summary", [])
		if catalog_effects_variant is Array:
			var catalog_effects: Array = catalog_effects_variant
			if not catalog_effects.is_empty():
				lines.append("")
				lines.append("[b]Catalog Effects[/b]")
				for effect_entry in catalog_effects:
					lines.append("• %s" % String(effect_entry))

		var requirement_lines := _format_definition_requirements(id)
		if not requirement_lines.is_empty():
			lines.append("")
			lines.append("[b]Constellation Requirements[/b]")
			lines.append_array(requirement_lines)

		var observation_notes := String(metadata.get("observation_notes", ""))
		if not observation_notes.is_empty():
			lines.append("")
			lines.append("[b]Observation Notes[/b]")
			lines.append(observation_notes)

		var leak_profile := String(metadata.get("leak_profile", ""))
		if not leak_profile.is_empty():
			lines.append("")
			lines.append("[b]Leak Profile[/b]")
			lines.append(leak_profile)

	_ledger_detail.text = "\n".join(lines)

func _update_progress_detail() -> void:
	if _progress_detail == null:
		return
	if _selected_progress_key == "":
		_progress_detail.text = "[i]Pending constellations will appear here once telemetry arrives.[/i]"
		return
	var entry := _get_progress_entry_by_key(_selected_progress_key)
	if entry.is_empty():
		_progress_detail.text = "[i]Pending constellations will appear here once telemetry arrives.[/i]"
		return
	var faction: int = int(entry.get("faction", -1))
	var discovery_id: int = int(entry.get("discovery", -1))
	var progress_percent: float = float(entry.get("progress", 0.0)) * 100.0
	var deficit: int = int(entry.get("observation_deficit", 0))
	var eta: int = int(entry.get("eta_ticks", 0))
	var covert := bool(entry.get("covert", false))
	var lines: Array[String] = []
	lines.append("[b]Constellation Readiness[/b]")
	lines.append("Discovery: %s" % _definition_label_for_id(discovery_id))
	lines.append("Faction: F%d" % faction)
	lines.append("Progress: %.2f%%" % progress_percent)
	lines.append("Observation deficit: %d" % deficit)
	lines.append("Estimated resolution: %s" % ("Now" if eta <= 0 else "%d ticks" % eta))
	lines.append("Posture: %s" % ("Covert" if covert else "Visible"))

	var metadata := _definition_metadata_for_id(discovery_id)
	if not metadata.is_empty():
		var gate_value := _definition_int(metadata, "observation_threshold")
		if gate_value >= 0:
			lines.append("Observation gate: %d verified signals" % gate_value)

		var cadence_bits: Array[String] = []
		var cooldown_value := _definition_int(metadata, "cooldown_ticks")
		if cooldown_value >= 0:
			cadence_bits.append("cooldown %d ticks" % cooldown_value)
		var freshness_value := _definition_int(metadata, "freshness_window")
		if freshness_value > 0:
			cadence_bits.append("freshness window %d ticks" % freshness_value)
		if _definition_bool(metadata, "covert_until_public", false):
			cadence_bits.append("covert until public")
		if not cadence_bits.is_empty():
			lines.append("Cadence: %s" % ", ".join(cadence_bits))

		var summary_text := String(metadata.get("summary", ""))
		if not summary_text.is_empty():
			lines.append("")
			lines.append("Summary: %s" % summary_text)

		var requirement_lines := _format_definition_requirements(discovery_id)
		if not requirement_lines.is_empty():
			lines.append("")
			lines.append("Requirements:")
			lines.append_array(requirement_lines)

	_progress_detail.text = "\n".join(lines)

func _get_progress_entry_by_key(key: String) -> Dictionary:
	var components := key.split(":")
	if components.size() != 2:
		return {}
	var faction := int(components[0])
	var discovery := int(components[1])
	if not _progress_map.has(faction):
		return {}
	var faction_variant: Variant = _progress_map[faction]
	if not (faction_variant is Dictionary):
		return {}
	var faction_dict: Dictionary = faction_variant
	if not faction_dict.has(discovery):
		return {}
	var entry_variant: Variant = faction_dict[discovery]
	if entry_variant is Dictionary:
		return (entry_variant as Dictionary).duplicate(true)
	return {}
