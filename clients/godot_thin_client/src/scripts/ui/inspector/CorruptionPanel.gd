extends ScrollContainer
class_name CorruptionInspectorPanel

## Inspector "Corruption" tab. Renders the corruption ledger (reputation modifier,
## audit capacity, and active incidents). Display-only and fully decoupled — the
## corruption *inject* command controls (dropdown/spins/button) are not part of this
## panel; they remain coordinator-owned. This tab is not capability-gated.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

@onready var _text: RichTextLabel = %CorruptionText

var _corruption: Dictionary = {}

func _ready() -> void:
	_render()

## Coordinator contract: read the corruption ledger key; re-render.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	if data.has("corruption"):
		var ledger_variant: Variant = data["corruption"]
		if ledger_variant is Dictionary:
			_corruption = (ledger_variant as Dictionary).duplicate(true)
			_render()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_corruption.clear()
	_render()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if _text != null:
		Typography.apply(_text, Typography.STYLE_BODY)

func _render() -> void:
	if _text == null:
		return
	if _corruption.is_empty():
		_text.text = "[b]Corruption[/b]\nNo ledger data received yet."
		return

	var lines: Array[String] = []
	lines.append("[b]Corruption[/b]")
	lines.append("Reputation modifier: %.3f" % float(_corruption.get("reputation_modifier", 0.0)))
	lines.append("Audit capacity: %d" % int(_corruption.get("audit_capacity", 0)))

	var entries_variant: Variant = _corruption.get("entries", [])
	var entries: Array = entries_variant if entries_variant is Array else []
	if entries.is_empty():
		lines.append("No active incidents.")
	else:
		lines.append("Active incidents:")
		for entry in entries:
			if not (entry is Dictionary):
				continue
			var info: Dictionary = entry
			var subsystem = str(info.get("subsystem", "Unknown"))
			var intensity = float(info.get("intensity", 0.0))
			var timer = int(info.get("exposure_timer", 0))
			var last_tick = int(info.get("last_update_tick", 0))
			lines.append(" • %s: intensity %.3f | τ=%d | updated %d"
				% [subsystem, intensity, timer, last_tick])

	_text.text = "\n".join(lines)
