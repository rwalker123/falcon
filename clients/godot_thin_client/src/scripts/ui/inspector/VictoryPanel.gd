extends ScrollContainer
class_name VictoryInspectorPanel

## Inspector "Victory" tab. Renders victory progress + per-mode meters and logs a
## one-shot "Victory achieved" line to the command log when a winner is first seen.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

@onready var _summary_text: RichTextLabel = %VictorySummaryText
@onready var _modes_list: ItemList = %VictoryModesList

var _victory_state: Dictionary = {}
var _log_signature: String = ""
## Coordinator-supplied command-log sink: (text) -> void.
var _append_log: Callable = Callable()

func _ready() -> void:
	_render()

## Coordinator contract: read the victory key, log a first-time winner once, re-render.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	if data.has("victory"):
		var victory_variant: Variant = data["victory"]
		if victory_variant is Dictionary:
			_victory_state = (victory_variant as Dictionary).duplicate(true)
			_log_victory()
			_render()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_victory_state.clear()
	_log_signature = ""
	_render()

## Coordinator collaborator: command-log sink for the one-shot victory announcement.
func set_log_hook(append_log: Callable) -> void:
	_append_log = append_log
	# Replay any victory that was ingested before the hook was wired (the signature is
	# only recorded once the log actually emits, so this can't double-announce).
	_log_victory()

func _log_victory() -> void:
	if _victory_state.is_empty():
		_log_signature = ""
		return
	var winner_variant: Variant = _victory_state.get("winner", {})
	if not (winner_variant is Dictionary):
		return
	var winner: Dictionary = winner_variant
	var mode: String = String(winner.get("mode", "")).strip_edges()
	if mode == "":
		return
	var tick: int = int(winner.get("tick", -1))
	var signature := "%s#%d" % [mode, tick]
	if signature == _log_signature:
		return
	if not _append_log.is_valid():
		# Don't record the signature until we actually emit, so a victory ingested
		# before the hook is wired still announces once the hook arrives.
		return
	var label: String = String(winner.get("label", mode)).strip_edges()
	if label == "":
		label = mode
	_log_signature = signature
	_append_log.call("Victory achieved: %s (tick %d)." % [label, tick])

func _render() -> void:
	if _summary_text != null:
		if _victory_state.is_empty():
			_summary_text.text = "[b]Victory Progress[/b]\n[i]Awaiting telemetry.[/i]"
		else:
			var lines: Array[String] = ["[b]Victory Progress[/b]"]
			var winner_variant: Variant = _victory_state.get("winner", {})
			if winner_variant is Dictionary and not (winner_variant as Dictionary).is_empty():
				var winner_dict: Dictionary = winner_variant
				var label_text := String(winner_dict.get("label", winner_dict.get("mode", "Victory")))
				var tick := int(winner_dict.get("tick", 0))
				lines.append("[color=gold]Winner locked:[/color] %s · Tick %d" % [label_text, tick])
			else:
				lines.append("[color=gray]No faction has secured a victory yet.[/color]")
			_summary_text.text = "\n".join(lines)
	if _modes_list == null:
		return
	_modes_list.clear()
	if _victory_state.is_empty():
		_modes_list.add_item("Awaiting telemetry.")
		return
	var modes_variant: Variant = _victory_state.get("modes", [])
	if not (modes_variant is Array) or (modes_variant as Array).is_empty():
		_modes_list.add_item("No victory modes reported.")
		return
	var sorted_modes: Array = _sorted_modes(modes_variant as Array)
	for mode in sorted_modes:
		if not (mode is Dictionary):
			continue
		var mode_dict: Dictionary = mode
		var label_text := String(mode_dict.get("label", mode_dict.get("id", mode_dict.get("kind", "Mode"))))
		if label_text.strip_edges() == "":
			label_text = _format_label(String(mode_dict.get("id", mode_dict.get("kind", "Mode"))))
		var pct: float = clamp(float(mode_dict.get("progress_pct", 0.0)), 0.0, 1.0) * 100.0
		var achieved := bool(mode_dict.get("achieved", false))
		var row_text := "%s — %.1f%%" % [label_text, pct]
		_modes_list.add_item(row_text)
		var row_index := _modes_list.get_item_count() - 1
		_modes_list.set_item_metadata(row_index, mode_dict)
		var progress_raw := float(mode_dict.get("progress", 0.0))
		var threshold := float(mode_dict.get("threshold", 0.0))
		var tooltip := "%s\nProgress %.2f / %.2f" % [
			("Achieved" if achieved else "In progress"),
			progress_raw,
			threshold
		]
		_modes_list.set_item_tooltip(row_index, tooltip)

func _sorted_modes(source: Array) -> Array:
	var entries: Array = []
	for entry in source:
		if entry is Dictionary:
			entries.append((entry as Dictionary).duplicate(true))
	entries.sort_custom(Callable(self, "_mode_sorter"))
	return entries

func _mode_sorter(a: Dictionary, b: Dictionary) -> bool:
	var pct_a := float(a.get("progress_pct", 0.0))
	var pct_b := float(b.get("progress_pct", 0.0))
	if is_equal_approx(pct_a, pct_b):
		var label_a := _format_label(String(a.get("label", a.get("id", ""))))
		var label_b := _format_label(String(b.get("label", b.get("id", ""))))
		return label_a < label_b
	return pct_a > pct_b

func _format_label(raw: String) -> String:
	var trimmed := raw.strip_edges()
	if trimmed == "":
		return "Victory Mode"
	var sanitized := trimmed.replace("_", " ").replace("-", " ").replace(".", " ")
	var parts: Array = sanitized.split(" ", false)
	for i in range(parts.size()):
		parts[i] = String(parts[i]).capitalize()
	return String(" ".join(parts)).strip_edges()
