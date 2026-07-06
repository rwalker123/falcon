extends ScrollContainer
class_name PowerInspectorPanel

## Inspector "Power" tab. Owns the power-grid vertical slice end to end: it ingests
## the snapshot/delta keys it cares about, holds its own node/metric state, and
## renders its own widgets. The Inspector coordinator forwards every update via
## apply_update() and clears the panel via reset(); it knows nothing about the
## power schema or these widgets.
##
## This is the reference implementation of the tab-panel contract other Inspector
## tabs are being migrated onto (see clients/godot_thin_client/CLAUDE.md).

const NODE_LIST_LIMIT := 16
const STABILITY_WARN := 0.4
const STABILITY_CRITICAL := 0.2

@onready var _summary_text: RichTextLabel = %PowerSummaryText
@onready var _node_list: ItemList = %PowerNodeList
@onready var _node_detail_text: RichTextLabel = %PowerNodeDetailText

var _nodes: Dictionary = {}
var _metrics: Dictionary = {}
var _selected_node_id: int = -1
## Whether the Power capability is unlocked. Gated tabs stay clickable and explain
## how they unlock instead of being disabled (see set_available()).
var _available: bool = true

func _ready() -> void:
	if _node_list != null:
		_node_list.item_selected.connect(_on_node_selected)
		_node_list.item_activated.connect(_on_node_selected)
	_render()

## Coordinator contract: ingest a full snapshot or an incremental delta. The panel
## reads only the keys it owns and re-renders once if anything changed.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	var dirty := false
	if full_snapshot and data.has("power_nodes"):
		_nodes.clear()
		_merge_nodes(data["power_nodes"])
		dirty = true
	elif data.has("power_updates"):
		_merge_nodes(data["power_updates"])
		dirty = true
	if data.has("power_removed"):
		_remove_nodes(data["power_removed"])
		dirty = true
	if data.has("power_metrics"):
		var metrics_variant: Variant = data["power_metrics"]
		if metrics_variant is Dictionary:
			_metrics = (metrics_variant as Dictionary).duplicate(true)
			dirty = true
	if dirty:
		_render()

## Coordinator contract: drop all state so the coordinator can re-seed from a clean
## slate (called during static-section (re)init; the hook for a future disconnect flow).
func reset() -> void:
	_nodes.clear()
	_metrics.clear()
	_selected_node_id = -1
	_render()

## Coordinator contract (capability-gated tabs only): report whether this tab's
## capability is unlocked. The tab stays clickable either way — when locked the
## panel renders an explanation of how it unlocks rather than being disabled.
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

func _merge_nodes(array) -> void:
	if array is Array:
		for entry in array:
			if not (entry is Dictionary):
				continue
			var info: Dictionary = (entry as Dictionary).duplicate(true)
			var node_id: int = int(info.get("node_id", info.get("entity", 0)))
			_nodes[node_id] = info

func _remove_nodes(ids) -> void:
	if ids is PackedInt64Array:
		for value in (ids as PackedInt64Array):
			_nodes.erase(int(value))
	elif ids is Array:
		for value in ids:
			_nodes.erase(int(value))
	if not _nodes.has(_selected_node_id):
		_selected_node_id = -1

func _render() -> void:
	if not _available:
		_render_locked()
		return
	if _summary_text != null:
		if _metrics.is_empty():
			_summary_text.text = "[b]Power Grid[/b]\n[i]Awaiting telemetry.[/i]"
		else:
			var supply: float = float(_metrics.get("total_supply", _metrics.get("total_supply_raw", 0.0)))
			var demand: float = float(_metrics.get("total_demand", _metrics.get("total_demand_raw", 0.0)))
			var storage: float = float(_metrics.get("total_storage", _metrics.get("total_storage_raw", 0.0)))
			var capacity: float = float(_metrics.get("total_capacity", _metrics.get("total_capacity_raw", 0.0)))
			var stress: float = float(_metrics.get("grid_stress_avg", 0.0))
			var margin: float = float(_metrics.get("surplus_margin", 0.0))
			var alerts: int = int(_metrics.get("instability_alerts", 0))
			var lines: Array[String] = []
			lines.append("[b]Power Grid[/b]")
			lines.append("Supply %.2f | Demand %.2f" % [supply, demand])
			lines.append("Storage %.2f / %.2f" % [storage, capacity])
			lines.append("Stress %.2f | Margin %.2f | Alerts %d" % [stress, margin, alerts])
			var incidents_variant: Variant = _metrics.get("incidents", [])
			if incidents_variant is Array and not (incidents_variant as Array).is_empty():
				var warn_count := 0
				var critical_count := 0
				for entry in (incidents_variant as Array):
					if not (entry is Dictionary):
						continue
					var severity := String((entry as Dictionary).get("severity", "warning"))
					if severity == "critical":
						critical_count += 1
					else:
						warn_count += 1
				lines.append("Incidents: %d critical, %d warning" % [critical_count, warn_count])
			_summary_text.text = "\n".join(lines)

	if _node_list != null:
		_node_list.clear()
		var entries: Array = Array(_nodes.values())
		entries.sort_custom(Callable(self, "_compare_nodes"))
		var selection_index: int = -1
		var limit: int = min(entries.size(), NODE_LIST_LIMIT)
		for idx in range(limit):
			var info_variant: Variant = entries[idx]
			if not (info_variant is Dictionary):
				continue
			var info: Dictionary = info_variant
			var label := _format_node_entry(info)
			var item_index := _node_list.add_item(label)
			_node_list.set_item_metadata(item_index, info)
			var node_id := int(info.get("node_id", info.get("entity", 0)))
			if node_id == _selected_node_id:
				selection_index = item_index
		if selection_index >= 0:
			_node_list.select(selection_index)
		elif _node_list.get_item_count() > 0:
			_node_list.select(0)
			var first_meta: Variant = _node_list.get_item_metadata(0)
			if first_meta is Dictionary:
				_selected_node_id = int((first_meta as Dictionary).get("node_id", (first_meta as Dictionary).get("entity", -1)))
	_update_node_detail()

func _render_locked() -> void:
	if _summary_text != null:
		_summary_text.text = "[b]Power Grid[/b]\n[i]🔒 Locked — the power grid comes online once your civilization completes a Power-field Great Discovery.[/i]"
	if _node_list != null:
		_node_list.clear()
	if _node_detail_text != null:
		_node_detail_text.text = "[i]Grid metrics and node telemetry appear here after the Power capability unlocks.[/i]"

func _compare_nodes(a: Dictionary, b: Dictionary) -> bool:
	var stability_a: float = float(a.get("stability", a.get("stability_raw", 0.0)))
	var stability_b: float = float(b.get("stability", b.get("stability_raw", 0.0)))
	if not is_equal_approx(stability_a, stability_b):
		return stability_a < stability_b
	var deficit_a: float = float(a.get("deficit", a.get("deficit_raw", 0.0)))
	var deficit_b: float = float(b.get("deficit", b.get("deficit_raw", 0.0)))
	return deficit_a > deficit_b

func _format_node_entry(info: Dictionary) -> String:
	var node_id: int = int(info.get("node_id", info.get("entity", 0)))
	var stability: float = float(info.get("stability", info.get("stability_raw", 0.0)))
	var generation: float = float(info.get("generation", info.get("generation_raw", 0.0)))
	var demand: float = float(info.get("demand", info.get("demand_raw", 0.0)))
	var deficit: float = float(info.get("deficit", info.get("deficit_raw", 0.0)))
	var surplus: float = float(info.get("surplus", info.get("surplus_raw", 0.0)))
	var incidents: int = int(info.get("incident_count", 0))
	return "#%03d st %.2f | gen %.1f / dem %.1f | Δ-%.1f Δ+%.1f | incidents %d" % [
		node_id,
		stability,
		generation,
		demand,
		deficit,
		surplus,
		incidents
	]

func _update_node_detail() -> void:
	if _node_detail_text == null:
		return
	if _selected_node_id < 0 or not _nodes.has(_selected_node_id):
		_node_detail_text.text = "[i]Select a node to inspect output, demand, and stability.[/i]"
		return
	var info: Dictionary = _nodes[_selected_node_id]
	var lines: Array[String] = []
	lines.append("[b]Node #%03d[/b]" % _selected_node_id)
	var entity_id: int = int(info.get("entity", 0))
	lines.append("Entity %016X" % entity_id)
	var generation: float = float(info.get("generation", info.get("generation_raw", 0.0)))
	var demand: float = float(info.get("demand", info.get("demand_raw", 0.0)))
	var efficiency: float = float(info.get("efficiency", info.get("efficiency_raw", 0.0)))
	var storage_level: float = float(info.get("storage_level", info.get("storage_level_raw", 0.0)))
	var storage_capacity: float = float(info.get("storage_capacity", info.get("storage_capacity_raw", 0.0)))
	var stability: float = float(info.get("stability", info.get("stability_raw", 0.0)))
	var surplus: float = float(info.get("surplus", info.get("surplus_raw", 0.0)))
	var deficit: float = float(info.get("deficit", info.get("deficit_raw", 0.0)))
	var incidents: int = int(info.get("incident_count", 0))
	var stability_label := "[color=green]stable[/color]"
	if stability < STABILITY_CRITICAL:
		stability_label = "[color=red]critical[/color]"
	elif stability < STABILITY_WARN:
		stability_label = "[color=yellow]warning[/color]"
	lines.append("Generation %.2f -> Demand %.2f" % [generation, demand])
	lines.append("Efficiency %.2f" % efficiency)
	lines.append("Storage %.2f / %.2f" % [storage_level, storage_capacity])
	lines.append("Stability %.2f %s" % [stability, stability_label])
	lines.append("Surplus %.2f | Deficit %.2f" % [surplus, deficit])
	lines.append("Incidents %d" % incidents)
	_node_detail_text.text = "\n".join(lines)

func _on_node_selected(index: int) -> void:
	if _node_list == null:
		return
	var meta: Variant = _node_list.get_item_metadata(index)
	if meta is Dictionary:
		var info: Dictionary = meta
		_selected_node_id = int(info.get("node_id", info.get("entity", -1)))
	else:
		_selected_node_id = -1
	_update_node_detail()
