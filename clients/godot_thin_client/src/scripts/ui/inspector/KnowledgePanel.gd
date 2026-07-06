extends ScrollContainer
class_name KnowledgeInspectorPanel

## Inspector "Knowledge" tab. Owns the espionage/knowledge vertical slice end to end:
## discovery-progress ledger, knowledge & timeline event feeds, counter-intelligence
## policy/budget state, and the espionage mission queue + debug controls. The Inspector
## coordinator forwards snapshot updates via apply_update(), clears via reset(), reports
## capability via set_available(), reports command connectivity via set_command_connected(),
## wires command issuing via set_command_hooks(), and pushes log-stream telemetry via
## ingest_log_entry(). Trade diffusion records arrive through the public append_events().
##
## Follows the tab-panel contract established by PowerPanel/CrisisPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

const COUNTERINTEL_POLICY_OPTIONS := [
	{"key": "lenient", "label": "Lenient"},
	{"key": "standard", "label": "Standard"},
	{"key": "hardened", "label": "Hardened"},
	{"key": "crisis", "label": "Crisis"}
]
const KNOWLEDGE_EVENT_HISTORY_LIMIT = 24
const KNOWLEDGE_TIMELINE_HISTORY_LIMIT = 48
const KNOWLEDGE_TIMELINE_KIND_LABELS := {
	0: "Leak progress",
	1: "Spy probe",
	2: "Counter-intel",
	3: "Exposure",
	4: "Treaty",
	5: "Cascade",
	6: "Digest"
}

@onready var _summary_text: RichTextLabel = %KnowledgeSummaryText
@onready var _progress_list: ItemList = %DiscoveryProgressList
@onready var _events_text: RichTextLabel = %KnowledgeEventsText
@onready var _counter_faction_spin: SpinBox = %KnowledgeCounterFactionSpin
@onready var _policy_dropdown: OptionButton = %KnowledgePolicyDropdown
@onready var _policy_apply_button: Button = %KnowledgePolicyApplyButton
@onready var _budget_reserve_spin: SpinBox = %KnowledgeBudgetReserveSpin
@onready var _budget_set_button: Button = %KnowledgeBudgetSetButton
@onready var _budget_delta_spin: SpinBox = %KnowledgeBudgetDeltaSpin
@onready var _budget_adjust_button: Button = %KnowledgeBudgetAdjustButton
@onready var _counterintel_status_text: RichTextLabel = %KnowledgeCounterintelStatusText
@onready var _mission_dropdown: OptionButton = %KnowledgeMissionDropdown
@onready var _owner_spin: SpinBox = %KnowledgeOwnerSpin
@onready var _target_spin: SpinBox = %KnowledgeTargetSpin
@onready var _discovery_spin: SpinBox = %KnowledgeDiscoverySpin
@onready var _tier_spin: SpinBox = %KnowledgeTierSpin
@onready var _agent_auto_toggle: CheckButton = %KnowledgeAgentAutoToggle
@onready var _agent_spin: SpinBox = %KnowledgeAgentSpin
@onready var _schedule_spin: SpinBox = %KnowledgeScheduleSpin
@onready var _queue_mission_button: Button = %KnowledgeQueueMissionButton
@onready var _mission_details_text: RichTextLabel = %KnowledgeMissionDetailsText
@onready var _queue_list: ItemList = %KnowledgeQueueList

var _discovery_progress: Dictionary = {}
var _events: Array[Dictionary] = []
var _timeline_events: Array[Dictionary] = []
var _metrics: Dictionary = {}
var _policy_states: Dictionary = {}
var _budget_states: Dictionary = {}
var _missions: Array[Dictionary] = []
var _mission_lookup: Dictionary = {}
var _mission_queue: Array[Dictionary] = []
## Latest known turn, mirrored from snapshot data so log-stream ingestion between
## snapshots stamps records with the current tick (as the coordinator's own _last_turn did).
var _last_turn: int = 0
## Whether the espionage (T2) capability is unlocked. Tab stays clickable; locked → explains.
var _available: bool = true
## Whether the command socket is connected. Gates the debug controls.
var _command_connected: bool = false
## Coordinator-supplied command hooks: (line, message) -> bool, and (text) -> void.
var _send_command: Callable = Callable()
var _append_log: Callable = Callable()

func _ready() -> void:
	if _agent_auto_toggle != null:
		_agent_auto_toggle.toggled.connect(_on_agent_auto_toggled)
	if _queue_mission_button != null:
		_queue_mission_button.pressed.connect(_on_queue_mission_pressed)
	if _mission_dropdown != null:
		if not _mission_dropdown.is_connected("item_selected", Callable(self, "_on_mission_selected")):
			_mission_dropdown.item_selected.connect(_on_mission_selected)
	_init_counterintel_controls()
	if _agent_auto_toggle != null:
		_on_agent_auto_toggled(_agent_auto_toggle.button_pressed)
	_render()

## Coordinator contract: ingest a full snapshot or delta; re-render if anything changed.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	if data.has("turn"):
		_last_turn = int(data["turn"])
	var dirty := false
	if full_snapshot and data.has("discovery_progress"):
		_discovery_progress.clear()
		_merge_discovery_progress(data["discovery_progress"])
		dirty = true
	elif data.has("discovery_progress_updates"):
		_merge_discovery_progress(data["discovery_progress_updates"])
		dirty = true
	if dirty:
		_render()

## Coordinator contract: drop all state so the coordinator can re-seed from a clean slate.
func reset() -> void:
	_discovery_progress.clear()
	_events.clear()
	_timeline_events.clear()
	_metrics.clear()
	_policy_states.clear()
	_budget_states.clear()
	_missions.clear()
	_mission_lookup.clear()
	_mission_queue.clear()
	_render()
	# _render() repaints the ledger/events/queue but not these two on-demand widgets;
	# refresh them so reset() is a true clean slate (matters when reset is wired to a
	# mid-session reconnect — at startup they are already empty).
	_refresh_counterintel_status()
	_refresh_mission_options()

## Coordinator contract (capability-gated): the tab stays clickable; when locked the panel
## explains how it unlocks and its debug controls are disabled.
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

## Coordinator collaborator: command connectivity. Gates the debug controls.
func set_command_connected(connected: bool) -> void:
	_command_connected = connected
	_apply_command_enabled()

## Coordinator collaborator: command sink. send(line, message) -> bool issues a runtime
## command; append_log(text) writes a line to the command log.
func set_command_hooks(send_command: Callable, append_log: Callable) -> void:
	_send_command = send_command
	_append_log = append_log

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	_style(_summary_text, Typography.STYLE_BODY)
	_style(_events_text, Typography.STYLE_BODY)
	_style(_progress_list, Typography.STYLE_BODY)

func _style(control: Control, style: StringName) -> void:
	if control != null:
		Typography.apply(control, style)

## Public feeder: trade-diffusion records cross-fed from the Trade tab appear in the
## knowledge "Recent Events" list. Enforces the shared history limit and re-renders.
func append_events(records: Array) -> void:
	for record in records:
		if record is Dictionary:
			# Public cross-panel boundary: own a copy so a caller mutating its
			# records later can't retroactively alter the knowledge history.
			_events.append((record as Dictionary).duplicate(true))
	while _events.size() > KNOWLEDGE_EVENT_HISTORY_LIMIT:
		_events.pop_front()
	_render()

## Coordinator collaborator: feed a log-stream entry through the knowledge ingesters
## (telemetry / counter-intel / espionage). Each matches at most one entry type.
func ingest_log_entry(entry: Dictionary) -> void:
	_maybe_ingest_knowledge_telemetry(entry)
	_maybe_ingest_counterintel_log(entry)
	_maybe_ingest_espionage_log(entry)

func _init_counterintel_controls() -> void:
	if _policy_dropdown != null:
		_policy_dropdown.clear()
		for option in COUNTERINTEL_POLICY_OPTIONS:
			var label: String = String(option.get("label", option.get("key", "")))
			var key: String = String(option.get("key", label.to_lower()))
			_policy_dropdown.add_item(label)
			_policy_dropdown.set_item_metadata(_policy_dropdown.get_item_count() - 1, key)
		var option_count: int = _policy_dropdown.get_item_count()
		if option_count > 0:
			_policy_dropdown.select(min(option_count - 1, 1))
	if _policy_apply_button != null:
		_policy_apply_button.pressed.connect(_on_policy_apply_pressed)
	if _budget_set_button != null:
		_budget_set_button.pressed.connect(_on_budget_set_pressed)
	if _budget_adjust_button != null:
		_budget_adjust_button.pressed.connect(_on_budget_adjust_pressed)
	if _counterintel_status_text != null:
		_counterintel_status_text.text = "[i]No counter-intel activity recorded yet.[/i]"

func _call_send(line: String, message: String) -> bool:
	if _send_command.is_valid():
		return bool(_send_command.call(line, message))
	return false

func _call_log(text: String) -> void:
	if _append_log.is_valid():
		_append_log.call(text)

func _merge_discovery_progress(array) -> void:
	if array is Array:
		for entry in array:
			if not (entry is Dictionary):
				continue
			_apply_discovery_progress_entry(entry as Dictionary)

func _apply_discovery_progress_entry(entry: Dictionary) -> void:
	var faction: int = int(entry.get("faction", -1))
	var discovery: int = int(entry.get("discovery", -1))
	if faction < 0 or discovery < 0:
		return
	var progress_value: float = float(entry.get("progress", entry.get("progress_raw", 0.0)))
	if not _discovery_progress.has(faction):
		_discovery_progress[faction] = {}
	var faction_dict: Dictionary = _discovery_progress[faction]
	faction_dict[discovery] = progress_value

func _render() -> void:
	_apply_command_enabled()
	if not _available:
		_render_locked()
		return
	_render_ledger()

func _render_locked() -> void:
	if _summary_text != null:
		_summary_text.text = "[b]Knowledge Ledger[/b]\n[i]🔒 Locked — the espionage & counter-intelligence ledger comes online once your civilization unlocks Tier II espionage.[/i]"
	if _events_text != null:
		_events_text.text = "[i]Knowledge telemetry appears here after espionage is unlocked.[/i]"
	if _progress_list != null:
		_progress_list.clear()

func _render_ledger() -> void:
	if _summary_text == null:
		return

	var lines: Array[String] = []
	lines.append("[b]Knowledge Ledger[/b]")
	var faction_keys: Array = _discovery_progress.keys()
	faction_keys.sort()
	if _discovery_progress.is_empty():
		lines.append("[i]Awaiting discovery progress telemetry.[/i]")
	else:
		for key in faction_keys:
			var faction: int = int(key)
			var progress_variant: Variant = _discovery_progress[key]
			if not (progress_variant is Dictionary):
				continue
			var progress_dict: Dictionary = progress_variant
			var entries: Array[Dictionary] = []
			for discovery_key in progress_dict.keys():
				var entry_dict: Dictionary = {
					"discovery": int(discovery_key),
					"progress": float(progress_dict[discovery_key])
				}
				entries.append(entry_dict)
			entries.sort_custom(Callable(self, "_compare_discovery_entries"))
			var limit: int = min(entries.size(), 3)
			var fragments: Array[String] = []
			for idx in range(limit):
				var entry = entries[idx]
				var percent: float = entry.get("progress", 0.0) * 100.0
				fragments.append("D%d %.1f%%" % [entry.get("discovery", -1), percent])
			if fragments.is_empty():
				fragments.append("No visible research")
			lines.append("Faction %d: %s" % [faction, ", ".join(fragments)])

	if not _metrics.is_empty():
		lines.append("")
		lines.append("[b]Telemetry Alerts[/b]")
		lines.append(
			"Warnings: %d | Criticals: %d | Countermeasures Active: %d | Common Knowledge: %d" % [
				int(_metrics.get("leak_warnings", 0)),
				int(_metrics.get("leak_criticals", 0)),
				int(_metrics.get("countermeasures_active", 0)),
				int(_metrics.get("common_knowledge_total", 0))
			]
		)

	_summary_text.text = "\n".join(lines)

	if _progress_list != null:
		_progress_list.clear()
		if not _discovery_progress.is_empty():
			for faction_key in faction_keys:
				var faction_int: int = int(faction_key)
				var inner_variant: Variant = _discovery_progress[faction_key]
				if not (inner_variant is Dictionary):
					continue
				var inner_dict: Dictionary = inner_variant
				var discoveries: Array = inner_dict.keys()
				discoveries.sort()
				for discovery_key in discoveries:
					var progress_val: float = float(inner_dict[discovery_key]) * 100.0
					var row: String = "F%d :: Discovery %d — %.1f%%" % [
						faction_int,
						int(discovery_key),
						progress_val
					]
					_progress_list.add_item(row)

	if _events_text != null:
		var lines_events: Array[String] = []
		if not _timeline_events.is_empty():
			for event_record in _timeline_events:
				if event_record is Dictionary:
					lines_events.append(_format_timeline_line(event_record))
		if not _events.is_empty():
			if not lines_events.is_empty():
				lines_events.append("")
			for record in _events:
				if record is Dictionary:
					lines_events.append(_format_event_line(record))
		if lines_events.is_empty():
			_events_text.text = "[i]No knowledge telemetry received.[/i]"
		else:
			_events_text.text = "\n".join(lines_events)

	_prune_expired_missions(_last_turn)
	_render_mission_queue()
	_update_selected_mission_details()

func _apply_command_enabled() -> void:
	# Effective enable: connected AND capability unlocked. Locked → all controls disabled.
	var connected: bool = _command_connected and _available
	var missions_available: bool = _mission_dropdown != null and _mission_dropdown.get_item_count() > 0
	if _queue_mission_button != null:
		_queue_mission_button.disabled = not (connected and missions_available)
	if _mission_dropdown != null:
		_mission_dropdown.disabled = _missions.is_empty() or not _available
	if _owner_spin != null:
		_owner_spin.editable = connected
	if _target_spin != null:
		_target_spin.editable = connected
	if _discovery_spin != null:
		_discovery_spin.editable = connected
	if _tier_spin != null:
		_tier_spin.editable = connected
	if _schedule_spin != null:
		_schedule_spin.editable = connected
	if _agent_auto_toggle != null:
		_agent_auto_toggle.disabled = not connected
	if _agent_spin != null:
		_agent_spin.editable = connected and (_agent_auto_toggle == null or not _agent_auto_toggle.button_pressed)
	if _counter_faction_spin != null:
		_counter_faction_spin.editable = connected
	if _policy_dropdown != null:
		_policy_dropdown.disabled = not connected
	if _policy_apply_button != null:
		_policy_apply_button.disabled = not connected
	if _budget_reserve_spin != null:
		_budget_reserve_spin.editable = connected
	if _budget_set_button != null:
		_budget_set_button.disabled = not connected
	if _budget_delta_spin != null:
		_budget_delta_spin.editable = connected
	if _budget_adjust_button != null:
		_budget_adjust_button.disabled = not connected

func _refresh_mission_options() -> void:
	if _mission_dropdown == null:
		return
	var previous_id: String = ""
	if _mission_dropdown.get_item_count() > 0:
		var current_index: int = _mission_dropdown.get_selected()
		if current_index >= 0:
			var meta = _mission_dropdown.get_item_metadata(current_index)
			if typeof(meta) == TYPE_STRING:
				previous_id = String(meta)

	_mission_dropdown.clear()
	if _missions.is_empty():
		_mission_dropdown.disabled = true
		if _mission_details_text != null:
			_mission_details_text.text = "[i]Select a mission template to view statistics.[/i]"
		return

	_mission_dropdown.disabled = false
	var entries: Array = []
	for mission in _missions:
		if mission is Dictionary:
			entries.append((mission as Dictionary).duplicate(true))
	entries.sort_custom(Callable(self, "_compare_mission_entries"))

	var selected_index: int = 0
	for idx in range(entries.size()):
		var entry: Dictionary = entries[idx]
		var mission_id: String = String(entry.get("id", ""))
		var label: String = _format_mission_label(entry)
		_mission_dropdown.add_item(label)
		_mission_dropdown.set_item_metadata(idx, mission_id)
		if mission_id == previous_id:
			selected_index = idx

	_mission_dropdown.select(selected_index)
	_apply_command_enabled()
	_update_selected_mission_details()

func _compare_mission_entries(a: Dictionary, b: Dictionary) -> bool:
	var a_label: String = String(a.get("name", a.get("id", "")))
	var b_label: String = String(b.get("name", b.get("id", "")))
	if a_label == "":
		a_label = String(a.get("id", ""))
	if b_label == "":
		b_label = String(b.get("id", ""))
	return a_label < b_label

func _on_mission_selected(_index: int) -> void:
	_update_selected_mission_details()

func _selected_mission_id() -> String:
	if _mission_dropdown == null:
		return ""
	if _mission_dropdown.get_item_count() == 0:
		return ""
	var index: int = _mission_dropdown.get_selected()
	if index < 0:
		index = 0
	var meta: Variant = _mission_dropdown.get_item_metadata(index)
	if typeof(meta) == TYPE_STRING:
		return String(meta)
	return String(_mission_dropdown.get_item_text(index)).strip_edges()

func _update_selected_mission_details() -> void:
	if _mission_details_text == null:
		return
	var mission_id: String = _selected_mission_id()
	if mission_id == "":
		_mission_details_text.text = "[i]Select a mission template to view statistics.[/i]"
		return
	var mission_variant: Variant = _mission_lookup.get(mission_id, null)
	if typeof(mission_variant) != TYPE_DICTIONARY:
		_mission_details_text.text = "[i]Awaiting telemetry for %s.[/i]" % mission_id
		return
	var mission: Dictionary = mission_variant
	var lines: Array[String] = []
	var name: String = String(mission.get("name", mission_id))
	lines.append("[b]%s[/b] (%s)" % [name, mission_id])
	var tags: Array[String] = []
	var kind: String = String(mission.get("kind", "")).strip_edges()
	if kind != "":
		tags.append(kind.capitalize())
	if bool(mission.get("generated", false)):
		tags.append("Generated")
	var misinfo_flag: bool = absf(float(mission.get("fidelity_suppression", 0.0))) > 0.0001
	if misinfo_flag:
		tags.append("Misinformation")
	if not tags.is_empty():
		lines.append("Tags: %s" % ", ".join(tags))
	var resolution: int = max(int(mission.get("resolution_ticks", 0)), 1)
	lines.append("Resolution: %d turn%s" % [resolution, "s" if resolution != 1 else ""])
	var success_pct: float = float(mission.get("base_success", 0.0)) * 100.0
	var threshold_pct: float = float(mission.get("success_threshold", 0.0)) * 100.0
	lines.append("Base success %.1f%% · Threshold %.1f%%" % [success_pct, threshold_pct])
	var fidelity_gain: float = float(mission.get("fidelity_gain", 0.0))
	var suspicion_success: float = float(mission.get("suspicion_on_success", 0.0))
	var suspicion_failure: float = float(mission.get("suspicion_on_failure", 0.0))
	var cell_gain: int = int(mission.get("cell_gain_on_success", 0))
	lines.append("Fidelity %+0.2f | Suspicion %+0.2f / %+0.2f | Cells +%d" % [
		fidelity_gain,
		suspicion_success,
		suspicion_failure,
		cell_gain
	])
	var detail_parts: Array[String] = []
	var suspicion_relief: float = float(mission.get("suspicion_relief", 0.0))
	if absf(suspicion_relief) > 0.0001:
		var relief_value: float = -absf(suspicion_relief) if suspicion_relief >= 0.0 else absf(suspicion_relief)
		detail_parts.append("Relief %+.2f" % relief_value)
	var fidelity_suppression: float = float(mission.get("fidelity_suppression", 0.0))
	if absf(fidelity_suppression) > 0.0001:
		var suppression_value: float = -absf(fidelity_suppression) if fidelity_suppression >= 0.0 else absf(fidelity_suppression)
		detail_parts.append("Suppression %+.2f" % suppression_value)
	if not detail_parts.is_empty():
		lines.append(", ".join(detail_parts))
	var note_variant: Variant = mission.get("note", "")
	var note_text: String = ""
	if note_variant != null:
		note_text = String(note_variant).strip_edges()
	if note_text != "":
		lines.append("[i]%s[/i]" % note_text)
	_mission_details_text.text = "\n".join(lines)

func _enrich_mission_queue_entries() -> void:
	if _mission_queue.is_empty():
		return
	var updated: bool = false
	for idx in range(_mission_queue.size()):
		var entry_variant = _mission_queue[idx]
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if _apply_mission_metadata(entry):
			updated = true
	if updated:
		_render_mission_queue()

func _apply_mission_metadata(entry: Dictionary) -> bool:
	var mission_id: String = String(entry.get("mission_id", ""))
	if mission_id == "":
		return false
	var mission_variant: Variant = _mission_lookup.get(mission_id, null)
	if typeof(mission_variant) != TYPE_DICTIONARY:
		return false
	var mission: Dictionary = mission_variant
	entry["name"] = String(mission.get("name", mission_id))
	entry["kind"] = String(mission.get("kind", ""))
	entry["generated"] = bool(mission.get("generated", entry.get("generated", false)))
	entry["resolution_ticks"] = max(int(mission.get("resolution_ticks", entry.get("resolution_ticks", 1))), 1)
	entry["base_success"] = float(mission.get("base_success", entry.get("base_success", 0.0)))
	entry["success_threshold"] = float(mission.get("success_threshold", entry.get("success_threshold", 0.0)))
	entry["fidelity_gain"] = float(mission.get("fidelity_gain", entry.get("fidelity_gain", 0.0)))
	entry["suspicion_on_success"] = float(mission.get("suspicion_on_success", entry.get("suspicion_on_success", 0.0)))
	entry["suspicion_on_failure"] = float(mission.get("suspicion_on_failure", entry.get("suspicion_on_failure", 0.0)))
	entry["suspicion_relief"] = float(mission.get("suspicion_relief", entry.get("suspicion_relief", 0.0)))
	entry["fidelity_suppression"] = float(mission.get("fidelity_suppression", entry.get("fidelity_suppression", 0.0)))
	entry["cell_gain_on_success"] = int(mission.get("cell_gain_on_success", entry.get("cell_gain_on_success", 0)))
	var note_variant: Variant = mission.get("note", entry.get("note", ""))
	entry["note"] = "" if note_variant == null else String(note_variant)
	entry["has_misinformation"] = absf(float(entry.get("fidelity_suppression", 0.0))) > 0.0001
	return true

func _prune_expired_missions(current_tick: int) -> void:
	if _mission_queue.is_empty():
		return
	var removed: bool = false
	for idx in range(_mission_queue.size() - 1, -1, -1):
		var entry_variant = _mission_queue[idx]
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var scheduled_tick: int = int(entry.get("scheduled_tick", current_tick))
		var resolution: int = max(int(entry.get("resolution_ticks", 1)), 1)
		if current_tick > scheduled_tick + resolution:
			_mission_queue.remove_at(idx)
			removed = true
	if removed:
		_render_mission_queue()

func _render_mission_queue() -> void:
	if _queue_list == null:
		return
	_queue_list.clear()
	var entries: Array = []
	for entry_variant in _mission_queue:
		if entry_variant is Dictionary:
			entries.append(entry_variant)
	if entries.is_empty():
		_queue_list.add_item("No active missions queued.")
		var placeholder_index: int = _queue_list.get_item_count() - 1
		if placeholder_index >= 0:
			_queue_list.set_item_disabled(placeholder_index, true)
		return
	entries.sort_custom(Callable(self, "_compare_mission_queue_entries"))
	for entry in entries:
		var label: String = _format_mission_queue_entry(entry)
		var index: int = _queue_list.get_item_count()
		_queue_list.add_item(label)
		_queue_list.set_item_metadata(index, int(entry.get("instance", -1)))

func _compare_mission_queue_entries(a: Dictionary, b: Dictionary) -> bool:
	var a_tick: int = int(a.get("scheduled_tick", 0))
	var b_tick: int = int(b.get("scheduled_tick", 0))
	if a_tick != b_tick:
		return a_tick < b_tick
	return int(a.get("instance", 0)) < int(b.get("instance", 0))

func _format_mission_queue_entry(entry: Dictionary) -> String:
	var instance: int = int(entry.get("instance", -1))
	var instance_label: String = "#%03d" % instance if instance >= 0 else "#---"
	var mission_id: String = String(entry.get("mission_id", ""))
	var mission_name: String = String(entry.get("name", mission_id if mission_id != "" else "Mission"))
	var owner: int = int(entry.get("owner", -1))
	var target_owner: int = int(entry.get("target_owner", -1))
	var discovery_id: int = int(entry.get("discovery", -1))
	var tier_variant: Variant = entry.get("target_tier", null)
	var tier_segment: String = ""
	if tier_variant != null:
		tier_segment = " T%s" % str(tier_variant)
	var path_label: String = "F%d→F%d D%d%s" % [owner, target_owner, discovery_id, tier_segment]
	var tags: Array[String] = []
	if bool(entry.get("generated", false)):
		tags.append("GEN")
	if bool(entry.get("has_misinformation", false)):
		tags.append("MISINFO")
	if bool(entry.get("auto_agent", false)):
		tags.append("AUTO")
	var tag_segment: String = ""
	if not tags.is_empty():
		tag_segment = " [%s]" % ", ".join(tags)
	var scheduled_tick: int = int(entry.get("scheduled_tick", -1))
	var resolution: int = max(int(entry.get("resolution_ticks", 1)), 1)
	var status: String = "Queued"
	var eta_label: String = ""
	if scheduled_tick >= 0:
		if _last_turn < scheduled_tick:
			status = "Queued"
			eta_label = " (ETA %d)" % max(scheduled_tick - _last_turn, 0)
		else:
			status = "Resolving"
			var completion_tick: int = scheduled_tick + resolution
			if _last_turn < completion_tick:
				eta_label = " (ETA %d)" % max(completion_tick - _last_turn, 0)
	var success_pct: float = float(entry.get("base_success", 0.0)) * 100.0
	return "%s %s %s%s · Success %.1f%% · %s%s" % [
		instance_label,
		mission_name,
		path_label,
		tag_segment,
		success_pct,
		status,
		eta_label
	]

func _find_mission_queue_index(instance_id: int) -> int:
	for idx in range(_mission_queue.size()):
		var entry_variant = _mission_queue[idx]
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if int(entry.get("instance", -1)) == instance_id:
			return idx
	return -1

func _remove_mission_queue_entry(instance_id: int) -> void:
	var index: int = _find_mission_queue_index(instance_id)
	if index < 0:
		return
	_mission_queue.remove_at(index)
	_render_mission_queue()

func _format_mission_label(entry: Dictionary) -> String:
	var mission_id: String = String(entry.get("id", "Mission"))
	var name: String = String(entry.get("name", mission_id))
	var kind: String = String(entry.get("kind", "")).strip_edges()
	var generated: bool = bool(entry.get("generated", false))
	var fragments: Array[String] = []
	if kind != "":
		fragments.append(kind)
	if generated:
		fragments.append("generated")
	if fragments.is_empty():
		return "%s (%s)" % [name, mission_id]
	return "%s (%s | %s)" % [name, mission_id, ", ".join(fragments)]

func _on_agent_auto_toggled(pressed: bool) -> void:
	if _agent_spin != null:
		_agent_spin.editable = not pressed and _command_connected
	_apply_command_enabled()

func _on_queue_mission_pressed() -> void:
	if _mission_dropdown == null or _mission_dropdown.get_item_count() == 0:
		_call_log("No espionage missions available to queue.")
		return
	var selected_index: int = _mission_dropdown.get_selected()
	if selected_index < 0:
		selected_index = 0
	var mission_meta: Variant = _mission_dropdown.get_item_metadata(selected_index)
	var mission_id: String = String(mission_meta) if typeof(mission_meta) == TYPE_STRING else String(_mission_dropdown.get_item_text(selected_index))
	mission_id = mission_id.strip_edges()
	if mission_id == "":
		_call_log("Mission template selection invalid.")
		return
	var owner: int = int(_owner_spin.value)
	var target: int = int(_target_spin.value)
	var discovery: int = int(_discovery_spin.value)
	if owner < 0 or target < 0 or discovery < 0:
		_call_log("Owner, target, and discovery must be non-negative.")
		return
	var agent_token: String = "auto"
	if _agent_auto_toggle != null and not _agent_auto_toggle.button_pressed:
		agent_token = str(int(_agent_spin.value))
	var tokens: Array[String] = [
		"queue_espionage_mission",
		mission_id,
		"owner",
		str(owner),
		"target",
		str(target),
		"discovery",
		str(discovery),
		"agent",
		agent_token
	]
	var tier_val: int = int(_tier_spin.value)
	if tier_val != -1:
		tokens.append("tier")
		tokens.append(str(tier_val))
	var schedule_val: int = int(_schedule_spin.value)
	if schedule_val != -1:
		var tick_value: int = max(_last_turn + schedule_val, 0)
		tokens.append("tick")
		tokens.append(str(tick_value))
	var command_line: String = " ".join(tokens)
	var summary: String = "Queued %s (F%d→F%d, discovery %d)" % [mission_id, owner, target, discovery]
	_call_send(command_line, summary)

func _on_policy_apply_pressed() -> void:
	if _policy_dropdown == null:
		return
	if _policy_dropdown.get_item_count() == 0:
		_call_log("No counter-intel policy options available.")
		return
	var faction: int = 0 if _counter_faction_spin == null else int(_counter_faction_spin.value)
	var index: int = _policy_dropdown.get_selected()
	if index < 0:
		index = 0
	var policy_variant: Variant = _policy_dropdown.get_item_metadata(index)
	var policy_key: String = String(policy_variant) if typeof(policy_variant) == TYPE_STRING else _policy_dropdown.get_item_text(index).to_lower()
	var summary: String = "Counter-intel policy set to %s for F%d" % [policy_key.capitalize(), faction]
	_call_send("counterintel_policy %d %s" % [faction, policy_key], summary)

func _on_budget_set_pressed() -> void:
	var faction: int = 0 if _counter_faction_spin == null else int(_counter_faction_spin.value)
	var reserve_value: float = 0.0 if _budget_reserve_spin == null else float(_budget_reserve_spin.value)
	var summary: String = "Counter-intel reserve set to %.2f for F%d" % [reserve_value, faction]
	_call_send("counterintel_budget %d reserve %.3f" % [faction, reserve_value], summary)

func _on_budget_adjust_pressed() -> void:
	var faction: int = 0 if _counter_faction_spin == null else int(_counter_faction_spin.value)
	var delta_value: float = 0.0 if _budget_delta_spin == null else float(_budget_delta_spin.value)
	if is_equal_approx(delta_value, 0.0):
		_call_log("Budget adjustment of 0 ignored.")
		return
	var summary: String = "Counter-intel reserve adjusted by %+.2f for F%d" % [delta_value, faction]
	_call_send("counterintel_budget %d delta %.3f" % [faction, delta_value], summary)

func _compare_discovery_entries(a: Dictionary, b: Dictionary) -> bool:
	var a_progress: float = float(a.get("progress", 0.0))
	var b_progress: float = float(b.get("progress", 0.0))
	return a_progress > b_progress

func _format_event_line(record: Dictionary) -> String:
	var tick: int = int(record.get("tick", _last_turn))
	var from_faction: int = int(record.get("from", -1))
	var to_faction: int = int(record.get("to", -1))
	var discovery: int = int(record.get("discovery", -1))
	var delta_percent: float = float(record.get("delta", 0.0)) * 100.0
	var via_migration: bool = bool(record.get("via_migration", false))
	var source_label: String = "migration" if via_migration else "trade"
	return "[%03d] F%d ← F%d discovery %d +%.2f%% (%s)" % [
		tick,
		to_faction,
		from_faction,
		discovery,
		delta_percent,
		source_label
	]

func _maybe_ingest_knowledge_telemetry(entry: Dictionary) -> bool:
	var message: String = String(entry.get("message", ""))
	if not message.begins_with("knowledge.telemetry "):
		return false
	var payload := message.substr("knowledge.telemetry ".length())
	var parsed: Variant = JSON.parse_string(payload)
	if typeof(parsed) != TYPE_DICTIONARY:
		return false
	var info: Dictionary = parsed
	var tick_value: int = int(info.get("tick", _last_turn))
	_metrics = {
		"tick": tick_value,
		"leak_warnings": int(info.get("leak_warnings", 0)),
		"leak_criticals": int(info.get("leak_criticals", 0)),
		"countermeasures_active": int(info.get("countermeasures_active", 0)),
		"common_knowledge_total": int(info.get("common_knowledge_total", 0))
	}
	var events_variant: Variant = info.get("events", [])
	_timeline_events.clear()
	if events_variant is Array:
		for event_variant in events_variant:
			if event_variant is Dictionary:
				var event_dict: Dictionary = _coerce_timeline_event(event_variant as Dictionary, tick_value)
				if not event_dict.is_empty():
					_timeline_events.append(event_dict)
	if _timeline_events.size() > KNOWLEDGE_TIMELINE_HISTORY_LIMIT:
		var start_index: int = max(0, _timeline_events.size() - KNOWLEDGE_TIMELINE_HISTORY_LIMIT)
		_timeline_events = _timeline_events.slice(start_index, _timeline_events.size())
	var missions_variant: Variant = info.get("missions", [])
	_missions.clear()
	_mission_lookup.clear()
	if missions_variant is Array:
		for mission_variant in missions_variant:
			if mission_variant is Dictionary:
				var mission_dict: Dictionary = (mission_variant as Dictionary).duplicate(true)
				_missions.append(mission_dict)
				var mission_id: String = String(mission_dict.get("id", ""))
				if mission_id != "":
					_mission_lookup[mission_id] = mission_dict
	_enrich_mission_queue_entries()
	_refresh_mission_options()
	_render()
	return true

func _maybe_ingest_espionage_log(entry: Dictionary) -> void:
	var target: String = String(entry.get("target", ""))
	if target != "shadow_scale::espionage":
		return
	var message: String = String(entry.get("message", ""))
	var fields_variant: Variant = entry.get("fields", {})
	if typeof(fields_variant) != TYPE_DICTIONARY:
		return
	var fields: Dictionary = fields_variant
	match message:
		"espionage.mission.queued":
			var mission_id: String = String(fields.get("mission_id", "")).strip_edges()
			if mission_id == "":
				return
			var instance_id: int = int(fields.get("instance", -1))
			var queue_entry: Dictionary = {
				"instance": instance_id,
				"mission_id": mission_id,
				"owner": int(fields.get("owner_faction", -1)),
				"target_owner": int(fields.get("target_owner", -1)),
				"discovery": int(fields.get("discovery_id", -1)),
				"agent_handle": int(fields.get("agent_handle", -1)),
				"scheduled_tick": int(fields.get("scheduled_tick", -1)),
				"target_tier": _to_optional_int(fields.get("target_tier", null)),
				"auto_agent": _coerce_bool(fields.get("auto_agent", false)),
				"queued_tick": _last_turn
			}
			_apply_mission_metadata(queue_entry)
			var existing_index: int = _find_mission_queue_index(instance_id)
			if existing_index >= 0:
				_mission_queue[existing_index] = queue_entry
			else:
				_mission_queue.append(queue_entry)
			_render_mission_queue()
		"espionage.mission.queue_failed":
			var failed_instance: int = int(fields.get("instance", -1))
			_remove_mission_queue_entry(failed_instance)
			var mission_label: String = String(fields.get("mission_id", "")).strip_edges()
			var error_text: String = String(fields.get("error", "")).strip_edges()
			if mission_label == "":
				mission_label = "mission"
			if error_text != "":
				_call_log("%s queue failed: %s." % [mission_label, error_text])
			else:
				_call_log("%s queue failed." % mission_label)
		_:
			pass

func _maybe_ingest_counterintel_log(entry: Dictionary) -> void:
	var target: String = String(entry.get("target", ""))
	if target != "shadow_scale::espionage":
		return
	var message: String = String(entry.get("message", ""))
	var fields_variant: Variant = entry.get("fields", {})
	if typeof(fields_variant) != TYPE_DICTIONARY:
		return
	var fields: Dictionary = fields_variant
	match message:
		"counter_intel.policy.updated":
			var faction: int = int(fields.get("faction", 0))
			var policy_label: String = String(fields.get("policy", "Unknown"))
			_policy_states[faction] = policy_label
			_refresh_counterintel_status()
		"counter_intel_budget.adjusted":
			var faction_id: int = int(fields.get("faction", 0))
			var reserve_val = _to_optional_float(fields.get("reserve", null))
			var delta_val = _to_optional_float(fields.get("delta", null))
			var available_val = _to_optional_float(fields.get("available", null))
			_budget_states[faction_id] = {
				"reserve": reserve_val,
				"delta": delta_val,
				"available": available_val
			}
			_refresh_counterintel_status()
		_:
			return

func _refresh_counterintel_status() -> void:
	if _counterintel_status_text == null:
		return
	var faction_keys: Array = []
	for key in _policy_states.keys():
		if not faction_keys.has(key):
			faction_keys.append(key)
	for key in _budget_states.keys():
		if not faction_keys.has(key):
			faction_keys.append(key)
	if faction_keys.is_empty():
		_counterintel_status_text.text = "[i]No counter-intel activity recorded yet.[/i]"
		return
	faction_keys.sort()
	var lines: Array[String] = []
	for key in faction_keys:
		var faction: int = int(key)
		var policy_label: String = String(_policy_states.get(faction, "Unknown"))
		var budget: Dictionary = _budget_states.get(faction, {})
		var parts: Array[String] = []
		var reserve_val = budget.get("reserve", null)
		if reserve_val != null:
			parts.append("Reserve %.2f" % float(reserve_val))
		var available_val = budget.get("available", null)
		if available_val != null:
			parts.append("Available %.2f" % float(available_val))
		var delta_val = budget.get("delta", null)
		if delta_val != null:
			parts.append("Δ %+.2f" % float(delta_val))
		var budget_text: String = "No budget data" if parts.is_empty() else ", ".join(parts)
		lines.append("Faction %d — Policy %s | %s" % [faction, policy_label, budget_text])
	_counterintel_status_text.text = "\n".join(lines)

func _to_optional_float(value) -> Variant:
	match typeof(value):
		TYPE_NIL:
			return null
		TYPE_BOOL:
			return 1.0 if value else 0.0
		TYPE_INT, TYPE_FLOAT:
			return float(value)
		TYPE_STRING:
			var text: String = String(value).strip_edges()
			if text == "" or text.to_lower() == "null":
				return null
			return text.to_float()
		_:
			return null

func _to_optional_int(value) -> Variant:
	match typeof(value):
		TYPE_NIL:
			return null
		TYPE_BOOL:
			return 1 if value else 0
		TYPE_INT, TYPE_FLOAT:
			return int(value)
		TYPE_STRING:
			var text: String = String(value).strip_edges()
			if text == "" or text.to_lower() == "null":
				return null
			return text.to_int()
		_:
			return null

func _coerce_bool(value) -> bool:
	match typeof(value):
		TYPE_BOOL:
			return value
		TYPE_INT:
			return int(value) != 0
		TYPE_FLOAT:
			return not is_equal_approx(float(value), 0.0)
		TYPE_STRING:
			var lowered := String(value).strip_edges().to_lower()
			return lowered in ["true", "1", "yes", "on"]
		_:
			return false

func _coerce_timeline_event(raw_event: Dictionary, fallback_tick: int) -> Dictionary:
	var tick_value: int = fallback_tick
	var tick_variant: Variant = raw_event.get("tick", null)
	if typeof(tick_variant) in [TYPE_INT, TYPE_FLOAT]:
		tick_value = int(tick_variant)
	var kind_value: int = int(raw_event.get("kind", -1))
	var note_variant: Variant = raw_event.get("note", "")
	var note_text: String = ""
	if note_variant != null and note_variant != "":
		note_text = String(note_variant)
	var delta_variant: Variant = raw_event.get("delta_percent", null)
	var delta_value: Variant = null
	if typeof(delta_variant) in [TYPE_INT, TYPE_FLOAT]:
		delta_value = float(delta_variant)
	var source_variant: Variant = raw_event.get("source_faction", null)
	var source_value: Variant = null
	if typeof(source_variant) in [TYPE_INT, TYPE_FLOAT]:
		source_value = int(source_variant)
	return {
		"tick": tick_value,
		"kind": kind_value,
		"kind_label": _event_kind_label(kind_value),
		"note": note_text,
		"delta_percent": delta_value,
		"source_faction": source_value
	}

func _event_kind_label(kind_value: int) -> String:
	if KNOWLEDGE_TIMELINE_KIND_LABELS.has(kind_value):
		return KNOWLEDGE_TIMELINE_KIND_LABELS[kind_value]
	return "Event"

func _format_timeline_line(record: Dictionary) -> String:
	var tick: int = int(record.get("tick", _last_turn))
	var label: String = String(record.get("kind_label", record.get("kind", "Event")))
	var source_variant: Variant = record.get("source_faction", null)
	var source_text: String = ""
	if typeof(source_variant) in [TYPE_INT, TYPE_FLOAT]:
		source_text = "F%d" % int(source_variant)
	var note_text: String = String(record.get("note", "")).strip_edges()
	var fragments: Array[String] = []
	if source_text != "":
		fragments.append(source_text)
	if note_text != "":
		fragments.append(note_text)
	var detail_text: String = ""
	if not fragments.is_empty():
		detail_text = " — " + " · ".join(fragments)
	var delta_variant: Variant = record.get("delta_percent", null)
	var delta_text: String = ""
	if typeof(delta_variant) in [TYPE_INT, TYPE_FLOAT]:
		delta_text = " (Δ%+.1f%%)" % float(delta_variant)
	return "[%03d] %s%s%s" % [tick, label, detail_text, delta_text]
