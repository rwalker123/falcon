extends ScrollContainer
class_name CrisisInspectorPanel

## Inspector "Crisis" tab. Owns the crisis-telemetry vertical slice: ingests the
## snapshot keys it cares about, holds its telemetry/annotation state, renders its
## widgets, and issues the seed/spawn debug commands. The Inspector coordinator
## forwards updates via apply_update(), clears via reset(), reports capability via
## set_available(), and wires command sending via set_command_hooks().
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

@onready var _summary_label: Label = %CrisisSummaryLabel
@onready var _summary_text: RichTextLabel = %CrisisSummaryText
@onready var _alerts_label: Label = %CrisisAlertsLabel
@onready var _alerts_text: RichTextLabel = %CrisisAlertsText
@onready var _auto_seed_check: CheckButton = %CrisisAutoSeedCheck
@onready var _archetype_field: LineEdit = %CrisisArchetypeField
@onready var _faction_spin: SpinBox = %CrisisFactionSpin
@onready var _spawn_button: Button = %CrisisSpawnButton

var _telemetry: Dictionary = {}
var _annotations: Array[Dictionary] = []
## Whether the Megaprojects/Crisis capability is unlocked. The tab stays clickable;
## when locked it explains how it unlocks instead of being disabled.
var _available: bool = true
## Coordinator-supplied command hooks: (line, message) -> bool, and (text) -> void.
var _send_command: Callable = Callable()
var _append_log: Callable = Callable()

func _ready() -> void:
	if _auto_seed_check != null:
		_auto_seed_check.toggled.connect(_on_auto_seed_toggled)
	if _spawn_button != null:
		_spawn_button.pressed.connect(_on_spawn_pressed)
	_render()

## Coordinator contract: ingest a full snapshot or delta; re-render if anything changed.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	var dirty := false
	if data.has("crisis_telemetry"):
		var telemetry_variant: Variant = data["crisis_telemetry"]
		if telemetry_variant is Dictionary:
			_telemetry = (telemetry_variant as Dictionary).duplicate(true)
			dirty = true
	if data.has("crisis_overlay"):
		_store_annotations_from_overlay(data["crisis_overlay"])
		dirty = true
	if dirty:
		_render()

## Coordinator contract: drop all state (new snapshot or disconnect).
func reset() -> void:
	_telemetry.clear()
	_annotations.clear()
	_render()

## Coordinator contract (capability-gated): the tab stays clickable; when locked the
## panel explains how it unlocks and its debug controls are disabled.
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

## Coordinator collaborator: command sink. send(line, message) -> bool issues a
## runtime command; append_log(text) writes a line to the command log.
func set_command_hooks(send_command: Callable, append_log: Callable) -> void:
	_send_command = send_command
	_append_log = append_log

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	_style(_summary_text, Typography.STYLE_BODY)
	_style(_alerts_text, Typography.STYLE_BODY)
	_style(_summary_label, Typography.STYLE_HEADING)
	_style(_alerts_label, Typography.STYLE_HEADING)

func _style(control: Control, style: StringName) -> void:
	if control != null:
		Typography.apply(control, style)

## Public feeder for the overlays payload's raw `crisis_annotations` array (the
## alternate encoding to the `crisis_overlay` key). Renders immediately since it is
## called outside apply_update().
func ingest_annotations(source: Variant) -> void:
	_replace_annotations(source)
	_render()

## From the `crisis_overlay` key: a dict wrapping an "annotations" array. No render
## (apply_update renders once after all its keys are applied).
func _store_annotations_from_overlay(data: Variant) -> void:
	var source: Variant = []
	if data is Dictionary:
		source = (data as Dictionary).get("annotations", [])
	_replace_annotations(source)

func _replace_annotations(source: Variant) -> void:
	_annotations.clear()
	if source is Array:
		for entry in (source as Array):
			if entry is Dictionary:
				_annotations.append((entry as Dictionary).duplicate(true))

func _render() -> void:
	_set_controls_enabled(_available)
	if not _available:
		_render_locked()
		return
	_render_summary()
	_render_alerts()

func _set_controls_enabled(enabled: bool) -> void:
	if _auto_seed_check != null:
		_auto_seed_check.disabled = not enabled
	if _archetype_field != null:
		_archetype_field.editable = enabled
	if _faction_spin != null:
		_faction_spin.editable = enabled
	if _spawn_button != null:
		_spawn_button.disabled = not enabled

func _render_locked() -> void:
	if _summary_text != null:
		_summary_text.text = "[b]Crisis Telemetry[/b]\n[i]🔒 Locked — the crisis dashboard comes online once your civilization completes a Crisis-field Great Discovery.[/i]"
	if _alerts_text != null:
		_alerts_text.text = "[i]Crisis alerts appear here after the Crisis capability unlocks.[/i]"

func _render_summary() -> void:
	if _summary_text == null:
		return
	if _telemetry.is_empty():
		_summary_text.text = "[b]Crisis Telemetry[/b]\n[i]Awaiting telemetry.[/i]"
		return
	var lines: Array[String] = []
	lines.append("[b]Crisis Telemetry[/b]")
	var warnings: int = int(_telemetry.get("warnings_active", 0))
	var criticals: int = int(_telemetry.get("criticals_active", 0))
	var modifiers: int = int(_telemetry.get("modifiers_active", 0))
	lines.append("Warnings %d · Criticals %d · Modifiers %d" % [warnings, criticals, modifiers])
	var foreshock: int = int(_telemetry.get("foreshock_incidents", 0))
	var containment: int = int(_telemetry.get("containment_incidents", 0))
	if foreshock > 0 or containment > 0:
		lines.append("Incidents %d foreshock · %d containment" % [foreshock, containment])
	var gauges_variant: Variant = _telemetry.get("gauges", [])
	var gauge_lines: Array[String] = []
	if gauges_variant is Array:
		for gauge_entry in (gauges_variant as Array):
			if not (gauge_entry is Dictionary):
				continue
			var gauge: Dictionary = gauge_entry
			var label := String(gauge.get("label", gauge.get("kind", "Metric")))
			var band := String(gauge.get("band", "safe"))
			var raw := float(gauge.get("raw", 0.0))
			var ema := float(gauge.get("ema", 0.0))
			var trend := float(gauge.get("trend_5t", 0.0))
			var warn_threshold := float(gauge.get("warn_threshold", 0.0))
			var crit_threshold := float(gauge.get("critical_threshold", 0.0))
			var stale := int(gauge.get("stale_ticks", 0))
			var status := _format_band(band)
			var gauge_text := "%s: raw %.3f · ema %.3f · trend %+0.3f [i](warn %.3f / crit %.3f)[/i] — %s" % [
				label,
				raw,
				ema,
				trend,
				warn_threshold,
				crit_threshold,
				status
			]
			if stale > 0:
				gauge_text += " [i](stale %dt)[/i]" % stale
			gauge_lines.append(gauge_text)
	if gauge_lines.is_empty():
		gauge_lines.append("[i]No gauges reported.[/i]")
	lines.append_array(gauge_lines)
	_summary_text.text = "\n".join(lines)

func _render_alerts() -> void:
	if _alerts_text == null:
		return
	if _annotations.is_empty():
		if _telemetry.is_empty():
			_alerts_text.text = "[i]Awaiting crisis telemetry.[/i]"
		else:
			var warnings := int(_telemetry.get("warnings_active", 0))
			var criticals := int(_telemetry.get("criticals_active", 0))
			if warnings == 0 and criticals == 0:
				_alerts_text.text = "[i]No active crisis alerts.[/i]"
			else:
				_alerts_text.text = "Warnings %d · Criticals %d — awaiting annotations." % [warnings, criticals]
		return
	var alert_lines: Array[String] = ["[b]Active Crisis Alerts[/b]"]
	for annotation in _annotations:
		if not (annotation is Dictionary):
			continue
		var entry: Dictionary = annotation
		var label := String(entry.get("label", ""))
		if label == "":
			label = "Unlabelled vector"
		var severity := String(entry.get("severity", "safe"))
		var severity_text := _format_band(severity)
		var path_summary := _summarize_path(entry.get("path", PackedInt32Array()))
		if path_summary == "":
			alert_lines.append("%s — %s" % [severity_text, label])
		else:
			alert_lines.append("%s — %s %s" % [severity_text, label, path_summary])
	_alerts_text.text = "\n".join(alert_lines)

func _format_band(band: String) -> String:
	var normalized := band.to_lower()
	match normalized:
		"critical":
			return "[color=#ff4d6a]CRITICAL[/color]"
		"warn":
			return "[color=#f2c94c]Warning[/color]"
		_:
			return "[color=#7ce7ff]Stable[/color]"

func _summarize_path(path_variant: Variant) -> String:
	if path_variant is PackedInt32Array:
		var packed: PackedInt32Array = path_variant
		var length := packed.size()
		if length < 2:
			return ""
		var start_col := int(packed[0])
		var start_row := int(packed[1])
		var tiles: int = max(int(length / 2), 1)
		if length >= 4:
			var end_col := int(packed[length - 2])
			var end_row := int(packed[length - 1])
			if start_col == end_col and start_row == end_row:
				return "[i](%d,%d · %d tiles)[/i]" % [start_col, start_row, tiles]
			return "[i](%d,%d → %d,%d · %d tiles)[/i]" % [start_col, start_row, end_col, end_row, tiles]
		return "[i](%d,%d · %d tiles)[/i]" % [start_col, start_row, tiles]
	elif path_variant is Array:
		var arr: Array = path_variant
		if arr.is_empty():
			return ""
		var tiles: int = max(int(arr.size()), 1)
		var start_step = arr[0]
		var end_step = arr[arr.size() - 1]
		if start_step is Array and start_step.size() >= 2:
			var start_col := int(start_step[0])
			var start_row := int(start_step[1])
			if end_step is Array and end_step.size() >= 2:
				var end_col := int(end_step[0])
				var end_row := int(end_step[1])
				if start_col == end_col and start_row == end_row:
					return "[i](%d,%d · %d tiles)[/i]" % [start_col, start_row, tiles]
				return "[i](%d,%d → %d,%d · %d tiles)[/i]" % [start_col, start_row, end_col, end_row, tiles]
		return "[i](path length %d)[/i]" % tiles
	return ""

func _on_auto_seed_toggled(pressed: bool) -> void:
	var line := "crisis_autoseed %s" % ("on" if pressed else "off")
	var message := "Crisis auto-seed %s." % ("enabled" if pressed else "disabled")
	if not _call_send(line, message) and _auto_seed_check != null:
		_auto_seed_check.button_pressed = not pressed

func _on_spawn_pressed() -> void:
	if _archetype_field == null:
		return
	var archetype_id := _archetype_field.text.strip_edges()
	if archetype_id.is_empty():
		_call_log("Provide a crisis archetype id to spawn.")
		return
	var normalized_id := archetype_id.to_lower()
	var faction := 0
	if _faction_spin != null:
		faction = int(_faction_spin.value)
	var line := "spawn_crisis %s %d" % [normalized_id, faction]
	var message := "Spawn request for crisis '%s' (faction %d)." % [normalized_id, faction]
	if _call_send(line, message):
		_archetype_field.clear()

func _call_send(line: String, message: String) -> bool:
	if _send_command.is_valid():
		return bool(_send_command.call(line, message))
	return false

func _call_log(text: String) -> void:
	if _append_log.is_valid():
		_append_log.call(text)
