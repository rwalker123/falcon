extends ScrollContainer
class_name TradeInspectorPanel

## Inspector "Trade" tab. Owns the trade-diffusion vertical slice: trade links,
## metrics, and the diffusion-event history; renders its own widgets; and drives the
## map's trade overlay. The Inspector coordinator forwards updates via apply_update(),
## clears via reset(), reports capability via set_available(), hands it the map via
## set_map_view(), and feeds it log telemetry via ingest_log_entry().
##
## Trade diffusion records also surface in the Knowledge tab's event feed. The two
## panels never reference each other — this panel emits knowledge_events_produced and
## the coordinator forwards the batch to KnowledgePanel.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const TOP_LINK_LIMIT := 10
const EVENT_HISTORY_LIMIT := 24
const HERD_TRADE_DIFFUSION_BONUS := 0.25

const Typography = preload("res://src/scripts/Typography.gd")

## Emitted after trade telemetry ingestion with the batch of diffusion records that
## also belong in the Knowledge event feed. The coordinator wires this to
## KnowledgePanel.append_events; the panels stay decoupled.
signal knowledge_events_produced(records: Array)

@onready var _summary_text: RichTextLabel = %TradeSummaryText
@onready var _links_list: ItemList = %TradeLinksList
@onready var _events_text: RichTextLabel = %TradeEventsText
## The trade overlay toggle physically lives under the Map tab, but its only purpose
## is driving the trade overlay, so this panel owns it (resolved scene-wide via %).
@onready var _overlay_toggle: CheckButton = %LogisticsOverlayToggle

var _links: Dictionary = {}
var _metrics: Dictionary = {}
var _history: Array[Dictionary] = []
var _selected_entity: int = -1
## Whether an Industry-tier capability is unlocked. The tab stays clickable; when
## locked it explains how it unlocks instead of being disabled.
var _available: bool = true
## Set by the coordinator; the trade overlay is pushed onto it.
var _map_view: Node = null
## Mirrors the coordinator's current tick (for metric/event fallbacks).
var _last_turn: int = 0

func _ready() -> void:
	if _overlay_toggle != null:
		_overlay_toggle.toggled.connect(_on_overlay_toggled)
	if _links_list != null:
		_links_list.item_selected.connect(_on_link_selected)
		_links_list.item_activated.connect(_on_link_selected)
	_render()

## Coordinator contract: ingest a full snapshot or delta; re-render + resync the map
## overlay if anything changed.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	if data.has("turn"):
		_last_turn = int(data["turn"])
	var dirty := false
	if full_snapshot and data.has("trade_links"):
		_links.clear()
		_merge_links(data["trade_links"])
		_selected_entity = -1
		dirty = true
	elif data.has("trade_link_updates"):
		_merge_links(data["trade_link_updates"])
		dirty = true
	if data.has("trade_link_removed"):
		_remove_links(data["trade_link_removed"])
		dirty = true
	if dirty:
		_render()
		_sync_map_overlay()

## Coordinator contract: drop all state (new snapshot or disconnect).
func reset() -> void:
	_links.clear()
	_metrics.clear()
	_history.clear()
	_selected_entity = -1
	_render()
	_sync_map_overlay()

## Coordinator contract (capability-gated): the tab stays clickable; when locked the
## panel explains how it unlocks. The Map-tab overlay toggle is intentionally left
## enabled (Map is not gated).
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

## Coordinator collaborator: the map view the trade overlay is pushed to.
func set_map_view(view: Node) -> void:
	_map_view = view
	_sync_map_overlay()

## Coordinator collaborator: feed a streamed log entry; trade telemetry is parsed out.
func ingest_log_entry(entry: Dictionary) -> void:
	_maybe_ingest_telemetry(entry)

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	_style(_summary_text, Typography.STYLE_BODY)
	_style(_events_text, Typography.STYLE_BODY)
	_style(_links_list, Typography.STYLE_BODY)
	_style(_overlay_toggle, Typography.STYLE_CONTROL)

func _style(control: Control, style: StringName) -> void:
	if control != null:
		Typography.apply(control, style)

func _merge_links(array) -> void:
	if array is Array:
		for entry in array:
			if not (entry is Dictionary):
				continue
			var info: Dictionary = (entry as Dictionary).duplicate(true)
			var id: int = int(info.get("entity", info.get("id", 0)))
			_links[id] = info

func _remove_links(ids) -> void:
	if ids is PackedInt64Array:
		var packed: PackedInt64Array = ids
		for value in packed:
			_links.erase(int(value))
	elif ids is Array:
		for value in ids:
			_links.erase(int(value))
	if _links.is_empty():
		_selected_entity = -1

func _render() -> void:
	if not _available:
		_render_locked()
		return
	if _summary_text == null:
		return

	if _links.is_empty():
		_summary_text.text = "[b]Trade Diffusion[/b]\n[i]Awaiting trade link telemetry.[/i]"
		if _links_list != null:
			_links_list.clear()
		if _events_text != null:
			_events_text.text = "[i]No diffusion events recorded yet.[/i]"
		return

	var lines: Array[String] = []
	lines.append("[b]Trade Diffusion[/b]")
	lines.append("Tracked links: %d" % _links.size())

	if not _metrics.is_empty():
		var metric_tick: int = int(_metrics.get("tick", _last_turn))
		var diffusion_count: int = int(_metrics.get("tech_diffusion_applied", 0))
		var migration_count: int = int(_metrics.get("migration_transfers", 0))
		var truncated: int = int(_metrics.get("records_truncated", 0))
		lines.append("Last tick %d → leaks %d (migration %d, extra %d)"
			% [metric_tick, diffusion_count, migration_count, truncated])

	var total_open: float = 0.0
	var total_flow: float = 0.0
	for value in _links.values():
		if value is Dictionary:
			total_open += _extract_openness(value)
			total_flow += abs(float((value as Dictionary).get("throughput", 0.0)))
	var avg_open: float = total_open / max(1, _links.size())
	var avg_flow: float = total_flow / max(1, _links.size())
	lines.append("Avg openness %.2f | avg flow %.2f" % [avg_open, avg_flow])
	var wildlife_line := _wildlife_summary_line()
	if wildlife_line != "":
		lines.append(wildlife_line)

	_summary_text.text = "\n".join(lines)

	if _links_list != null:
		_links_list.clear()
		var entries: Array = Array(_links.values())
		entries.sort_custom(Callable(self, "_compare_links"))
		var limit: int = min(entries.size(), TOP_LINK_LIMIT)
		for idx in range(limit):
			var info_variant: Variant = entries[idx]
			if not (info_variant is Dictionary):
				continue
			var info: Dictionary = info_variant
			var entity_id: int = int(info.get("entity", info.get("id", 0)))
			var openness: float = _extract_openness(info)
			var throughput: float = float(info.get("throughput", 0.0))
			var knowledge_variant: Variant = info.get("knowledge", {})
			var leak_timer: int = 0
			if knowledge_variant is Dictionary:
				leak_timer = int((knowledge_variant as Dictionary).get("leak_timer", 0))
			var from_faction: int = int(info.get("from_faction", -1))
			var to_faction: int = int(info.get("to_faction", -1))
			var label: String = "ID %d :: F%d→F%d | open %.2f | τ %d | flow %.2f" % [
				entity_id,
				from_faction,
				to_faction,
				openness,
				leak_timer,
				throughput
			]
			_links_list.add_item(label)
			_links_list.set_item_metadata(_links_list.get_item_count() - 1, entity_id)
			if entity_id == _selected_entity:
				_links_list.select(_links_list.get_item_count() - 1)

	if _events_text != null:
		if _history.is_empty():
			_events_text.text = "[i]No diffusion events recorded yet.[/i]"
		else:
			var event_lines: Array[String] = []
			for record in _history:
				if record is Dictionary:
					event_lines.append(_format_event_line(record))
			_events_text.text = "\n".join(event_lines)

func _render_locked() -> void:
	if _summary_text != null:
		_summary_text.text = "[b]Trade Diffusion[/b]\n[i]🔒 Locked — trade routes and diffusion telemetry come online once your civilization reaches the Industry tier.[/i]"
	if _links_list != null:
		_links_list.clear()
	if _events_text != null:
		_events_text.text = "[i]Trade link and diffusion telemetry appear here after the Industry capability unlocks.[/i]"

func _compare_links(a: Dictionary, b: Dictionary) -> bool:
	var a_open: float = _extract_openness(a)
	var b_open: float = _extract_openness(b)
	if is_equal_approx(a_open, b_open):
		var a_flow: float = abs(float(a.get("throughput", 0.0)))
		var b_flow: float = abs(float(b.get("throughput", 0.0)))
		return a_flow > b_flow
	return a_open > b_open

func _extract_openness(info: Dictionary) -> float:
	var knowledge_variant: Variant = info.get("knowledge", {})
	if knowledge_variant is Dictionary:
		return float((knowledge_variant as Dictionary).get("openness", 0.0))
	return 0.0

func _sync_map_overlay() -> void:
	if _map_view == null:
		return
	var links_array: Array = []
	for value in _links.values():
		if value is Dictionary:
			links_array.append((value as Dictionary).duplicate(true))
	var enabled: bool = _overlay_toggle != null and _overlay_toggle.button_pressed
	if _map_view.has_method("update_trade_overlay"):
		_map_view.call("update_trade_overlay", links_array, enabled)
	if _map_view.has_method("set_trade_overlay_enabled"):
		_map_view.call("set_trade_overlay_enabled", enabled)
	if _map_view.has_method("set_trade_overlay_selection"):
		_map_view.call("set_trade_overlay_selection", _selected_entity)

func _on_overlay_toggled(_pressed: bool) -> void:
	_sync_map_overlay()

func _on_link_selected(index: int) -> void:
	if _links_list == null:
		return
	if index < 0 or index >= _links_list.get_item_count():
		_selected_entity = -1
		_sync_map_overlay()
		return
	var meta = _links_list.get_item_metadata(index)
	if typeof(meta) in [TYPE_INT, TYPE_FLOAT]:
		_selected_entity = int(meta)
	else:
		_selected_entity = -1
	_sync_map_overlay()

func _push_record(record: Dictionary, tick: int) -> Dictionary:
	var entry: Dictionary = record.duplicate(true)
	entry["tick"] = tick
	_history.append(entry.duplicate(true))
	while _history.size() > EVENT_HISTORY_LIMIT:
		_history.pop_front()
	return entry

func _format_event_line(record: Dictionary) -> String:
	var tick: int = int(record.get("tick", _last_turn))
	var from_faction: int = int(record.get("from", -1))
	var to_faction: int = int(record.get("to", -1))
	var discovery: int = int(record.get("discovery", -1))
	var delta_percent: float = float(record.get("delta", 0.0)) * 100.0
	var via_migration: bool = bool(record.get("via_migration", false))
	var tag: String = "migration" if via_migration else "trade"
	var herd_density: float = float(record.get("herd_density", 0.0))
	var wildlife_suffix := ""
	if herd_density > 0.0:
		var bonus_pct := herd_density * HERD_TRADE_DIFFUSION_BONUS * 100.0
		wildlife_suffix = " ρ=%.2f (+%.1f%%)" % [herd_density, bonus_pct]
	return "[%03d] F%d→F%d discovery %d +%.2f%% (%s)%s" % [
		tick,
		from_faction,
		to_faction,
		discovery,
		delta_percent,
		tag,
		wildlife_suffix
	]

func _maybe_ingest_telemetry(entry: Dictionary) -> bool:
	var message: String = String(entry.get("message", ""))
	if not message.begins_with("trade.telemetry "):
		return false
	var payload := message.substr("trade.telemetry ".length())
	var parsed: Variant = JSON.parse_string(payload)
	if typeof(parsed) != TYPE_DICTIONARY:
		return false
	var info: Dictionary = parsed
	_metrics = info.duplicate(true)
	var tick_value: int = int(info.get("tick", _last_turn))
	var records_variant: Variant = info.get("records", [])
	var batch: Array = []
	if records_variant is Array:
		for record_variant in records_variant:
			if record_variant is Dictionary:
				batch.append(_push_record(record_variant as Dictionary, tick_value))
	# Emit the whole batch once so the Knowledge feed re-renders a single time.
	if not batch.is_empty():
		knowledge_events_produced.emit(batch)
	_render()
	return true

func _wildlife_summary_line() -> String:
	if _history.is_empty():
		return ""
	var last_index := _history.size() - 1
	if last_index < 0:
		return ""
	var record: Dictionary = _history[last_index]
	var herd_density: float = float(record.get("herd_density", 0.0))
	if herd_density <= 0.0:
		return ""
	var bonus_pct := herd_density * HERD_TRADE_DIFFUSION_BONUS * 100.0
	return "Wildlife density %.0f%% → +%.1f%% leak bonus" % [herd_density * 100.0, bonus_pct]
