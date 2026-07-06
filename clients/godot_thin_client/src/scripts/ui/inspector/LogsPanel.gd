extends VBoxContainer
class_name LogsInspectorPanel

## Inspector "Logs" tab. Fully owns the log stream: the LogStreamClient, the
## per-frame polling, the record buffer, level/target/text filters, target registry,
## and the tick-duration sparkline. Each raw stream entry is emitted via
## log_entry_received so the coordinator can dispatch it to the Knowledge/Trade
## ingesters (the panel never references those panels). Synthetic entries (command
## log, culture tensions, script logs) are pushed in by the coordinator via
## append_entry().
##
## Follows the tab-panel contract (see clients/godot_thin_client/CLAUDE.md).

const LogStreamClientScript = preload("res://src/scripts/LogStreamClient.gd")
const Typography = preload("res://src/scripts/Typography.gd")

const LOG_ENTRY_LIMIT = 60
const LOG_HOST_DEFAULT = "127.0.0.1"
const LOG_PORT_DEFAULT = 41003
const LOG_POLL_INTERVAL = 0.1
const LOG_RECONNECT_INTERVAL = 2.0
const LOG_TARGET_FALLBACK = "(general)"
const LOG_LEVEL_FILTER_OPTIONS = [
	{"label": "All", "threshold": 0},
	{"label": "Debug+", "threshold": 1},
	{"label": "Info+", "threshold": 2},
	{"label": "Warn+", "threshold": 3},
	{"label": "Error", "threshold": 4}
]
const LOG_LEVEL_SEVERITY = {
	"TRACE": 0,
	"DEBUG": 1,
	"INFO": 2,
	"WARN": 3,
	"WARNING": 3,
	"ERROR": 4,
	"COMMAND": 2,
	"SCRIPT": 2
}
const TICK_SAMPLE_LIMIT = 48

## Emitted once per raw stream entry; the coordinator dispatches it to the
## Knowledge/Trade log ingesters. The panel keeps no reference to those panels.
signal log_entry_received(entry: Dictionary)

@onready var _logs_text: RichTextLabel = %LogsText
@onready var _status_label: Label = %SparklineStatusLabel
@onready var _sparkline_graph: Control = %SparklineGraph
@onready var _sparkline_stats_label: Label = %SparklineStatsLabel
@onready var _level_label: Label = %LevelLabel
@onready var _level_dropdown: OptionButton = %LogLevelDropdown
@onready var _target_label: Label = %TargetLabel
@onready var _target_dropdown: OptionButton = %LogTargetDropdown
@onready var _filter_label: Label = %FilterLabel
@onready var _filter_line: LineEdit = %LogFilterLine
@onready var _clear_button: Button = %ClearButton
@onready var _copy_button: Button = %CopyButton

var _entries: Array = []
var _filtered_records: Array = []
var _render_dirty: bool = true
var _targets_dirty: bool = true
var _target_counts: Dictionary = {}
var _target_list: Array[String] = []
var _selected_target: String = ""
var _level_threshold: int = 0
var _search_query: String = ""
var _search_query_lower: String = ""
var _client: RefCounted = null
var _host: String = ""
var _port: int = 0
var _connected: bool = false
var _poll_timer: float = 0.0
var _retry_timer: float = 0.0
var _tick_samples: Array[Dictionary] = []
var _status_message: String = "Log stream offline."

func _ready() -> void:
	_initialize_filters()
	_initialize_channel()
	_render()
	_update_sparkline()

func _process(delta: float) -> void:
	_poll_stream(delta)

## Coordinator contract: Logs has no snapshot keys (it is stream/synthetic fed).
func apply_update(_data: Dictionary, _full_snapshot: bool) -> void:
	pass

## Coordinator contract: re-render on static-section (re)init. The running log buffer
## is intentionally preserved (logs are a stream, not per-snapshot state — matching the
## pre-extraction behavior, which only marked dirty + rendered here). The Clear button
## is what wipes the buffer.
func reset() -> void:
	_mark_dirty()
	_render()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	_style(_logs_text, Typography.STYLE_BODY)
	_style(_status_label, Typography.STYLE_CAPTION)
	_style(_sparkline_stats_label, Typography.STYLE_CAPTION)
	_style(_level_label, Typography.STYLE_CAPTION)
	_style(_target_label, Typography.STYLE_CAPTION)
	_style(_filter_label, Typography.STYLE_CAPTION)
	_style(_level_dropdown, Typography.STYLE_CONTROL)
	_style(_target_dropdown, Typography.STYLE_CONTROL)
	_style(_filter_line, Typography.STYLE_CONTROL)
	_style(_clear_button, Typography.STYLE_CONTROL)
	_style(_copy_button, Typography.STYLE_CONTROL)

func _style(control: Control, style: StringName) -> void:
	if control != null:
		Typography.apply(control, style)

## Coordinator push: append a synthetic (non-stream) log line — command log, culture
## tensions, script logs. Mirrors the old _append_log_entry.
func append_entry(entry: String, level: String = "INFO", target: String = "inspector", timestamp_ms: int = -1) -> void:
	var trimmed: String = entry.strip_edges(true, true)
	if trimmed == "":
		return
	var resolved_timestamp: int = timestamp_ms
	if resolved_timestamp < 0:
		# UNIX-epoch ms, matching the server log stream (log_stream.rs) so
		# synthetic lines and stream entries share the same time base and
		# _format_timestamp renders both correctly.
		resolved_timestamp = int(Time.get_unix_time_from_system() * 1000.0)
	var normalized_level: String = _normalize_level(level)
	var formatted: String = "[%s] %s" % [_format_timestamp(resolved_timestamp), trimmed]
	_record(formatted, normalized_level, target, trimmed, resolved_timestamp, {}, true)

func _initialize_filters() -> void:
	if _level_dropdown != null:
		_level_dropdown.clear()
		for idx in range(LOG_LEVEL_FILTER_OPTIONS.size()):
			var option: Dictionary = LOG_LEVEL_FILTER_OPTIONS[idx]
			_level_dropdown.add_item(String(option.get("label", "All")))
			_level_dropdown.set_item_metadata(idx, int(option.get("threshold", 0)))
		var default_index: int = 0
		if _level_dropdown.get_item_count() > 0:
			default_index = min(2, _level_dropdown.get_item_count() - 1)
			_level_dropdown.select(default_index)
			var meta_variant: Variant = _level_dropdown.get_item_metadata(default_index)
			_level_threshold = int(meta_variant) if typeof(meta_variant) == TYPE_INT else 0
		if not _level_dropdown.item_selected.is_connected(_on_level_filter_changed):
			_level_dropdown.item_selected.connect(_on_level_filter_changed)

	if _filter_line != null:
		_filter_line.text = ""
		_filter_line.placeholder_text = "Search text or fields"
		if not _filter_line.text_changed.is_connected(_on_search_changed):
			_filter_line.text_changed.connect(_on_search_changed)

	if _target_dropdown != null:
		_target_dropdown.clear()
		_target_dropdown.add_item("All targets")
		_target_dropdown.set_item_metadata(0, "")
		_target_dropdown.select(0)
		if not _target_dropdown.item_selected.is_connected(_on_target_filter_changed):
			_target_dropdown.item_selected.connect(_on_target_filter_changed)

	if _clear_button != null and not _clear_button.pressed.is_connected(_on_clear_pressed):
		_clear_button.pressed.connect(_on_clear_pressed)

	if _copy_button != null and not _copy_button.pressed.is_connected(_on_copy_pressed):
		_copy_button.pressed.connect(_on_copy_pressed)

func _initialize_channel() -> void:
	_client = LogStreamClientScript.new()
	_host = _determine_host()
	_port = _determine_port()
	_connected = false
	_poll_timer = 0.0
	_retry_timer = 0.0
	_update_status("Connecting to log stream (%s:%d)..." % [_host, _port])
	var err: Error = ERR_UNAVAILABLE
	if _client != null and _client.has_method("connect_to"):
		err = _client.call("connect_to", _host, _port)
	if err != OK:
		_update_status("Log stream connection failed (%s)." % error_string(err))
		_retry_timer = LOG_RECONNECT_INTERVAL

func _determine_host() -> String:
	var env_host: String = OS.get_environment("LOG_HOST")
	if env_host != "":
		return env_host
	env_host = OS.get_environment("STREAM_HOST")
	if env_host != "":
		return env_host
	env_host = OS.get_environment("COMMAND_HOST")
	if env_host != "":
		return env_host
	return LOG_HOST_DEFAULT

func _determine_port() -> int:
	var env_port: String = OS.get_environment("LOG_PORT")
	if env_port != "":
		var parsed: int = int(env_port)
		if parsed > 0:
			return parsed
	return LOG_PORT_DEFAULT

func _poll_stream(delta: float) -> void:
	if _client == null:
		return
	_poll_timer += delta
	if _poll_timer < LOG_POLL_INTERVAL:
		return
	_poll_timer = 0.0
	if not _client.has_method("poll"):
		return
	var entries_variant: Variant = _client.call("poll")
	if typeof(entries_variant) != TYPE_ARRAY:
		entries_variant = []
	var entries: Array = entries_variant
	var status_code_variant: Variant = _client.call("status") if _client.has_method("status") else StreamPeerTCP.STATUS_NONE
	var status_code: int = int(status_code_variant)
	match status_code:
		StreamPeerTCP.STATUS_CONNECTING:
			var connecting_message: String = "Log stream connecting (%s:%d)..." % [_host, _port]
			if _status_message != connecting_message:
				_update_status(connecting_message)
			_connected = false
			return
		StreamPeerTCP.STATUS_CONNECTED:
			if not _connected:
				_update_status("Log stream connected (%s:%d)." % [_host, _port])
			_connected = true
			_retry_timer = 0.0
		_:
			if _connected:
				_update_status("Log stream disconnected; retrying...")
			_connected = false

	if not _connected:
		_retry_timer += LOG_POLL_INTERVAL
		if _retry_timer >= LOG_RECONNECT_INTERVAL:
			_retry_timer = 0.0
			var retry_err: Error = ERR_UNAVAILABLE
			if _client.has_method("connect_to"):
				retry_err = _client.call("connect_to", _host, _port)
			if retry_err != OK:
				_update_status("Log stream retry failed (%s)." % error_string(retry_err))
			else:
				_update_status("Reconnecting to log stream (%s:%d)..." % [_host, _port])
		return

	var updated: bool = false
	for entry in entries:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		_ingest_stream_entry(entry)
		updated = true
	if updated:
		_update_sparkline()

func _update_status(message: String) -> void:
	if _status_message == message:
		return
	_status_message = message
	if _status_label != null:
		_status_label.text = message
	_render()

## A raw stream entry: broadcast it (coordinator dispatches to Knowledge/Trade), then
## record it locally for the sparkline + display.
func _ingest_stream_entry(entry: Dictionary) -> void:
	log_entry_received.emit(entry)
	_record_tick_sample(entry)
	var level: String = _normalize_level(String(entry.get("level", "INFO")))
	var raw_target: String = String(entry.get("target", ""))
	var timestamp_ms: int = int(entry.get("timestamp_ms", 0))
	var message: String = String(entry.get("message", ""))
	var fields_variant: Variant = entry.get("fields", {})
	var fields: Dictionary = {}
	if typeof(fields_variant) == TYPE_DICTIONARY:
		fields = (fields_variant as Dictionary).duplicate(true)
	var formatted: String = _format_entry(timestamp_ms, level, raw_target, message, fields)
	if formatted != "":
		_record(formatted, level, raw_target, message, timestamp_ms, fields, false)

func _format_entry(timestamp_ms: int, level: String, target: String, message: String, fields: Dictionary) -> String:
	var time_str: String = _format_timestamp(timestamp_ms)
	var level_label: String = _normalize_level(level)
	var colored_level: String = "[%s]" % level_label
	var level_color: String = _level_color(level_label)
	if level_color != "":
		colored_level = "[color=%s][%s][/color]" % [level_color, level_label]
	var target_segment: String = ""
	var trimmed_target: String = target.strip_edges()
	if trimmed_target != "":
		target_segment = " (%s)" % trimmed_target
	var suffix: String = _format_field_suffix(fields)
	return "[%s] %s%s %s%s" % [time_str, colored_level, target_segment, message, suffix]

func _stringify_field(name: String, value) -> String:
	match typeof(value):
		TYPE_BOOL:
			return "true" if value else "false"
		TYPE_INT:
			return str(value)
		TYPE_FLOAT:
			if name == "duration_ms":
				return "%.1fms" % float(value)
			return "%.2f" % float(value)
		TYPE_STRING:
			return String(value)
		TYPE_ARRAY:
			return "[%d]" % (value as Array).size()
		TYPE_DICTIONARY:
			return "{...}"
		TYPE_NIL:
			return "null"
		_:
			return str(value)

func _format_timestamp(ms: int) -> String:
	if ms <= 0:
		return "--:--:--"
	var seconds: int = ms / 1000
	var millis: int = ms % 1000
	var datetime: Dictionary = Time.get_datetime_dict_from_unix_time(float(seconds))
	var hour: int = int(datetime.get("hour", 0))
	var minute: int = int(datetime.get("minute", 0))
	var second: int = int(datetime.get("second", 0))
	return "%02d:%02d:%02d.%03d" % [hour, minute, second, millis]

func _record_tick_sample(entry: Dictionary) -> void:
	var fields_variant: Variant = entry.get("fields", {})
	if typeof(fields_variant) != TYPE_DICTIONARY:
		return
	var fields: Dictionary = fields_variant
	var turn_id: int = int(fields.get("turn", -1))
	var duration_val: float = float(fields.get("duration_ms", 0.0))
	if duration_val <= 0.0:
		return
	var sample := {
		"turn": turn_id,
		"duration_ms": duration_val
	}
	_tick_samples.append(sample)
	while _tick_samples.size() > TICK_SAMPLE_LIMIT:
		_tick_samples.pop_front()

func _update_sparkline() -> void:
	if _sparkline_graph == null:
		return
	if _tick_samples.is_empty():
		if _sparkline_graph.has_method("clear_samples"):
			_sparkline_graph.call("clear_samples")
		if _sparkline_stats_label != null:
			_sparkline_stats_label.text = "Awaiting telemetry."
		return
	var durations: Array = []
	var total: float = 0.0
	for sample in _tick_samples:
		var value: float = float(sample.get("duration_ms", 0.0))
		durations.append(value)
		total += value
	if _sparkline_graph.has_method("set_samples"):
		_sparkline_graph.call("set_samples", durations)
	var latest: Dictionary = _tick_samples[_tick_samples.size() - 1]
	var turn_id: int = int(latest.get("turn", -1))
	var last_duration: float = float(latest.get("duration_ms", 0.0))
	var avg_duration: float = total / max(durations.size(), 1)
	if _sparkline_stats_label != null:
		_sparkline_stats_label.text = "Turn %d: %.1f ms (avg %.1f ms over %d turns)" % [
			turn_id,
			last_duration,
			avg_duration,
			durations.size()
		]

func _render() -> void:
	if _logs_text == null:
		return
	if _targets_dirty:
		_refresh_target_dropdown()
	var lines: Array[String] = []
	lines.append("[b]Logs[/b]")
	if _status_message != "":
		lines.append("[color=#a4c6ff]%s[/color]" % _status_message)
	var filtered: Array = _filtered_records_list()
	if filtered.is_empty():
		if _entries.is_empty():
			lines.append("No log entries yet.")
		else:
			lines.append("[i]No log entries match current filters.[/i]")
	else:
		for record_variant in filtered:
			if not (record_variant is Dictionary):
				continue
			var record: Dictionary = record_variant
			lines.append(String(record.get("formatted", "")))
	_logs_text.text = "\n".join(lines)
	if _logs_text.get_line_count() > 0:
		_logs_text.scroll_to_line(_logs_text.get_line_count() - 1)

func _filtered_records_list() -> Array:
	if not _render_dirty:
		return _filtered_records
	var filtered: Array = []
	for record_variant in _entries:
		if not (record_variant is Dictionary):
			continue
		var record: Dictionary = record_variant
		if _record_passes_filters(record):
			filtered.append(record)
	_filtered_records = filtered
	_render_dirty = false
	return _filtered_records

func _record_passes_filters(record: Dictionary) -> bool:
	var level: String = _normalize_level(String(record.get("level", "INFO")))
	if _severity_for_level(level) < _level_threshold:
		return false
	if _selected_target != "":
		var record_target: String = _normalize_target(String(record.get("target", "")))
		if record_target != _selected_target:
			return false
	if _search_query_lower != "":
		var haystack: String = String(record.get("formatted_lower", record.get("formatted", ""))).to_lower()
		if not haystack.contains(_search_query_lower):
			return false
	return true

func _severity_for_level(level: String) -> int:
	var upper: String = level.to_upper()
	if LOG_LEVEL_SEVERITY.has(upper):
		return int(LOG_LEVEL_SEVERITY[upper])
	return LOG_LEVEL_SEVERITY.get("INFO", 2)

func _normalize_level(level: String) -> String:
	var upper: String = level.to_upper()
	match upper:
		"WARNING":
			return "WARN"
		"ERR":
			return "ERROR"
		_:
			return upper

func _normalize_target(raw: String) -> String:
	var trimmed := raw.strip_edges()
	if trimmed == "":
		return LOG_TARGET_FALLBACK
	return trimmed

func _register_target(target_key: String, delta: int) -> void:
	var previous: int = int(_target_counts.get(target_key, 0))
	var updated: int = previous + delta
	if updated <= 0:
		_target_counts.erase(target_key)
		_target_list.erase(target_key)
		if _selected_target == target_key:
			_selected_target = ""
	else:
		_target_counts[target_key] = updated
		if previous == 0 and delta > 0:
			_target_list.append(target_key)
	_targets_dirty = true

func _refresh_target_dropdown() -> void:
	if _target_dropdown == null:
		_targets_dirty = false
		return
	if _selected_target != "" and not _target_counts.has(_selected_target):
		_selected_target = ""
	_target_dropdown.clear()
	_target_dropdown.add_item("All targets")
	_target_dropdown.set_item_metadata(0, "")
	var sorted_targets: Array = _target_list.duplicate()
	sorted_targets.sort()
	var index: int = 1
	var applied_selection: bool = false
	for target_key in sorted_targets:
		var count: int = int(_target_counts.get(target_key, 0))
		if count <= 0:
			continue
		_target_dropdown.add_item("%s (%d)" % [target_key, count])
		_target_dropdown.set_item_metadata(index, target_key)
		if target_key == _selected_target:
			_target_dropdown.select(index)
			applied_selection = true
		index += 1
	if not applied_selection:
		_target_dropdown.select(0)
	_targets_dirty = false

func _mark_dirty() -> void:
	_render_dirty = true

func _reset_state() -> void:
	_entries.clear()
	_filtered_records.clear()
	_render_dirty = true
	_targets_dirty = true
	_target_counts.clear()
	_target_list.clear()
	_selected_target = ""

func _record(formatted: String, level: String, target: String, message: String, timestamp_ms: int, fields: Dictionary, synthetic: bool) -> void:
	var target_key: String = _normalize_target(target)
	var stored_fields: Dictionary = {}
	if fields is Dictionary:
		stored_fields = (fields as Dictionary).duplicate(true)
	var record: Dictionary = {
		"formatted": formatted,
		"formatted_lower": formatted.to_lower(),
		"level": level,
		"target": target,
		"target_key": target_key,
		"message": message,
		"timestamp_ms": timestamp_ms,
		"fields": stored_fields,
		"synthetic": synthetic
	}
	_entries.append(record)
	_register_target(target_key, 1)
	while _entries.size() > LOG_ENTRY_LIMIT:
		var removed_variant: Variant = _entries.pop_front()
		if removed_variant is Dictionary:
			var removed: Dictionary = removed_variant
			var removed_key: String = _normalize_target(String(removed.get("target", "")))
			_register_target(removed_key, -1)
	_mark_dirty()
	_render()

func _level_color(level: String) -> String:
	match level:
		"ERROR":
			return "#ff6b6b"
		"WARN":
			return "#ffd166"
		"INFO":
			return "#a4c6ff"
		"DEBUG":
			return "#6ee7b7"
		"TRACE":
			return "#9aa5b1"
		"COMMAND":
			return "#d4bfff"
		"SCRIPT":
			return "#d4bfff"
		_:
			return ""

func _format_field_suffix(fields: Dictionary) -> String:
	if fields.is_empty():
		return ""
	var keys: Array = fields.keys()
	keys.sort()
	var parts: Array[String] = []
	for key in keys:
		var key_str: String = String(key)
		parts.append("%s=%s" % [key_str, _stringify_field(key_str, fields[key])])
	if parts.is_empty():
		return ""
	return " " + ", ".join(parts)

func _on_level_filter_changed(index: int) -> void:
	if _level_dropdown == null:
		return
	var metadata: Variant = _level_dropdown.get_item_metadata(index)
	var threshold: int = 0
	if typeof(metadata) == TYPE_INT:
		threshold = int(metadata)
	_level_threshold = threshold
	_mark_dirty()
	_render()

func _on_target_filter_changed(index: int) -> void:
	if _target_dropdown == null:
		return
	var metadata: Variant = _target_dropdown.get_item_metadata(index)
	if metadata == null:
		_selected_target = ""
	else:
		_selected_target = String(metadata)
	if _selected_target != "" and not _target_counts.has(_selected_target):
		_selected_target = ""
	_mark_dirty()
	_render()

func _on_search_changed(new_text: String) -> void:
	_search_query = new_text
	_search_query_lower = new_text.to_lower()
	_mark_dirty()
	_render()

func _on_clear_pressed() -> void:
	_reset_state()
	if _target_dropdown != null:
		_target_dropdown.select(0)
	if _filter_line != null and _filter_line.text != "":
		_filter_line.text = ""
	_search_query = ""
	_search_query_lower = ""
	_mark_dirty()
	_render()
	_update_status("Logs cleared")

func _on_copy_pressed() -> void:
	if _logs_text == null:
		return
	var contents := _logs_text.text
	if contents.strip_edges().is_empty():
		_update_status("No logs to copy")
		return
	DisplayServer.clipboard_set(contents)
	var line_count := _logs_text.get_line_count()
	if line_count > 0:
		_update_status("Copied %d log lines" % line_count)
	else:
		_update_status("Logs copied to clipboard")
