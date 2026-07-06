extends ScrollContainer
class_name SentimentInspectorPanel

## Inspector "Sentiment" tab. Renders turn/axis-bias/axis-totals. Axis bias is owned
## by the coordinator (the Commands axis controls mutate it optimistically), so it is
## pushed in via set_axis_bias(); everything else the panel reads from the snapshot.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const Typography = preload("res://src/scripts/Typography.gd")

@onready var _text: RichTextLabel = %SentimentText

var _sentiment: Dictionary = {}
var _axis_bias: Dictionary = {}
var _last_turn: int = 0

func _ready() -> void:
	_render()

## Coordinator contract: read the sentiment key and the current turn; re-render.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	var dirty := false
	if data.has("turn"):
		_last_turn = int(data["turn"])
		dirty = true
	if data.has("sentiment"):
		var sentiment_variant: Variant = data["sentiment"]
		if sentiment_variant is Dictionary:
			_sentiment = (sentiment_variant as Dictionary).duplicate(true)
			dirty = true
	if dirty:
		_render()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_sentiment.clear()
	_render()

## Coordinator push: axis bias is coordinator-owned (Commands axis controls mutate it
## optimistically); forwarded here so the sentiment view reflects it immediately.
func set_axis_bias(bias: Dictionary) -> void:
	_axis_bias = bias.duplicate(true)
	_render()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if _text != null:
		Typography.apply(_text, Typography.STYLE_BODY)

func _render() -> void:
	if _text == null:
		return
	var lines: Array[String] = []
	lines.append("[b]Turn[/b] %d" % _last_turn)

	if not _axis_bias.is_empty():
		lines.append("[b]Axis Bias[/b]")
		for key in ["knowledge", "trust", "equity", "agency"]:
			var bias_value = float(_axis_bias.get(key, 0.0))
			lines.append(" • %s: %.3f" % [key.capitalize(), bias_value])

	if not _sentiment.is_empty():
		lines.append("")
		lines.append("[b]Axis Totals[/b]")
		for key in ["knowledge", "trust", "equity", "agency"]:
			if not _sentiment.has(key):
				continue
			var axis: Dictionary = _sentiment[key]
			var total = float(axis.get("total", 0.0))
			var policy = float(axis.get("policy", 0.0))
			var incidents = float(axis.get("incidents", 0.0))
			var influencer_val = float(axis.get("influencers", 0.0))
			lines.append(" • %s: %.3f (policy %.3f | incidents %.3f | influencers %.3f)"
				% [key.capitalize(), total, policy, incidents, influencer_val])

			var drivers = axis.get("drivers", [])
			var count = 0
			for driver in drivers:
				if count >= 3:
					break
				if not (driver is Dictionary):
					continue
				var driver_dict: Dictionary = driver
				var label: String = str(driver_dict.get("label", "Unnamed"))
				var category = str(driver_dict.get("category", ""))
				var value = float(driver_dict.get("value", 0.0))
				var weight = float(driver_dict.get("weight", 0.0))
				lines.append("    · [%s] %s: %.3f × %.3f" % [category, label, value, weight])
				count += 1

	_text.text = "\n".join(lines)
