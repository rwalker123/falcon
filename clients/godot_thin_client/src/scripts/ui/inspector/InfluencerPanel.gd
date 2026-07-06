extends ScrollContainer
class_name InfluencerInspectorPanel

## Inspector "Influencers" tab. Owns the influencer roster: ingests full/delta roster
## data, renders the top influencers, and exposes the culture-resonance aggregate the
## coordinator feeds into the Culture tab.
##
## Display-only within this tab — the influencer *command* controls (support/suppress/
## channel/spawn) are not part of this panel; they remain coordinator-owned and read
## the roster back via get_influencers().
##
## Capability-gated (Industry tier): the tab stays clickable; when locked it explains
## how it unlocks rather than being disabled.
##
## Follows the tab-panel contract established by PowerPanel (see
## clients/godot_thin_client/CLAUDE.md).

const TOP_INFLUENCER_LIMIT := 8
const TOP_RESONANCE_LIMIT := 2

const Typography = preload("res://src/scripts/Typography.gd")

@onready var _text: RichTextLabel = %InfluencersText

var _influencers: Dictionary = {}
## Whether an Industry-tier capability is unlocked. The tab stays clickable; when
## locked it explains how it unlocks instead of being disabled.
var _available: bool = true

func _ready() -> void:
	_render()

## Coordinator contract: ingest a full roster snapshot or delta; re-render if changed.
func apply_update(data: Dictionary, full_snapshot: bool) -> void:
	var dirty := false
	if full_snapshot and data.has("influencers"):
		_rebuild_influencers(data["influencers"])
		dirty = true
	elif data.has("influencer_updates"):
		_merge_influencers(data["influencer_updates"])
		dirty = true
	if data.has("influencer_removed"):
		_remove_influencers(data["influencer_removed"])
		dirty = true
	if dirty:
		_render()

## Coordinator contract: drop state (new snapshot / disconnect).
func reset() -> void:
	_influencers.clear()
	_render()

## Coordinator contract (capability-gated): the tab stays clickable; when locked the
## panel explains how it unlocks.
func set_available(available: bool) -> void:
	if _available == available:
		return
	_available = available
	_render()

## Coordinator contract: (re)apply typography to this panel's styled widgets.
func apply_typography() -> void:
	if _text != null:
		Typography.apply(_text, Typography.STYLE_BODY)

## Coordinator collaborator: the roster, keyed by influencer id. Read by the
## coordinator-owned influencer command controls (dropdown/selection).
func get_influencers() -> Dictionary:
	return _influencers

## Coordinator collaborator: culture-resonance totals grouped by scope (Global /
## Regional / Local). The coordinator feeds this into the Culture tab render.
func aggregate_resonance() -> Dictionary:
	var totals := {
		"Global": {},
		"Regional": {},
		"Local": {}
	}
	for value in _influencers.values():
		if not (value is Dictionary):
			continue
		var info: Dictionary = value as Dictionary
		var scope_text := str(info.get("scope", ""))
		if scope_text == "Generation":
			scope_text = "Global"
		if not totals.has(scope_text):
			totals[scope_text] = {}
		var resonance_variant: Variant = info.get("culture_resonance", null)
		var entries: Array = []
		if resonance_variant is Array:
			entries = resonance_variant
		if entries.is_empty():
			continue
		var axis_map: Dictionary = totals[scope_text]
		for entry_variant in entries:
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant as Dictionary
			var axis_key: String = str(entry.get("axis", entry.get("label", "")))
			if axis_key == "":
				continue
			var label: String = str(entry.get("label", axis_key))
			var output_val: float = float(entry.get("output", 0.0))
			if absf(output_val) < 0.0001:
				continue
			if not axis_map.has(axis_key):
				axis_map[axis_key] = {
					"axis": axis_key,
					"label": label,
					"output": 0.0
				}
			axis_map[axis_key]["output"] += output_val
	var result := {}
	for scope_key in totals.keys():
		var axis_map: Dictionary = totals[scope_key]
		var entries: Array = axis_map.values()
		entries.sort_custom(Callable(self, "_compare_resonance_total"))
		result[scope_key] = entries
	return result

func _rebuild_influencers(array_data) -> void:
	_influencers.clear()
	if not (array_data is Array):
		return
	for entry in array_data:
		if not (entry is Dictionary):
			continue
		var info: Dictionary = entry.duplicate(true)
		var id = int(info.get("id", 0))
		_influencers[id] = info

func _merge_influencers(array_data) -> void:
	if not (array_data is Array):
		return
	for entry in array_data:
		if not (entry is Dictionary):
			continue
		var info: Dictionary = entry.duplicate(true)
		var id = int(info.get("id", 0))
		_influencers[id] = info

func _remove_influencers(ids) -> void:
	if ids is PackedInt64Array:
		for value in (ids as PackedInt64Array):
			_influencers.erase(int(value))
	elif ids is Array:
		for value in ids:
			_influencers.erase(int(value))

func _render() -> void:
	if _text == null:
		return
	if not _available:
		_text.text = "[b]Influencers[/b]\n[i]🔒 Locked — the influencer roster and culture-resonance telemetry come online once your civilization reaches the Industry tier.[/i]"
		return
	if _influencers.is_empty():
		_text.text = "[b]Influencers[/b]\nNo roster data received yet."
		return

	var entries: Array = _influencers.values()
	entries.sort_custom(Callable(self, "_compare_influencers"))

	var lines: Array[String] = []
	lines.append("[b]Influencers[/b] (%d tracked)" % entries.size())
	var limit: int = min(entries.size(), TOP_INFLUENCER_LIMIT)
	for index in range(limit):
		var info: Dictionary = entries[index]
		var id = int(info.get("id", 0))
		var name: String = str(info.get("name", "Unnamed"))
		var lifecycle = str(info.get("lifecycle", ""))
		var influence = float(info.get("influence", 0.0))
		var growth = float(info.get("growth_rate", 0.0))
		var support = float(info.get("support_charge", 0.0))
		var suppress = float(info.get("suppress_pressure", 0.0))
		lines.append("%d. %s [ID %d] — %s" % [index + 1, name, id, lifecycle])
		lines.append("    influence %.3f | growth %.3f | support %.3f | suppress %.3f"
			% [influence, growth, support, suppress])

		var domains_variant = info.get("domains")
		if domains_variant is PackedStringArray:
			var domain_str = _join_strings(domains_variant)
			if domain_str != "":
				lines.append("    domains: %s" % domain_str)

		var resonance_variant: Variant = info.get("culture_resonance", null)
		var resonance_entries: Array = []
		if resonance_variant is Array:
			resonance_entries = resonance_variant
		if resonance_entries.size() > 0:
			resonance_entries.sort_custom(Callable(self, "_compare_culture_resonance"))
			var resonance_limit: int = min(resonance_entries.size(), TOP_RESONANCE_LIMIT)
			var fragments: Array[String] = []
			for ridx in range(resonance_limit):
				var entry_variant: Variant = resonance_entries[ridx]
				if not (entry_variant is Dictionary):
					continue
				var entry: Dictionary = entry_variant as Dictionary
				var axis_label: String = str(entry.get("label", entry.get("axis", "Axis")))
				var weight_val: float = float(entry.get("weight", 0.0))
				var output_val: float = float(entry.get("output", 0.0))
				fragments.append("%s w%+.2f Δ%+.3f" % [axis_label, weight_val, output_val])
			if fragments.size() > 0:
				lines.append("    culture: %s" % ", ".join(fragments))

	_text.text = "\n".join(lines)

func _compare_influencers(a: Dictionary, b: Dictionary) -> bool:
	var a_score = float(a.get("influence", 0.0))
	var b_score = float(b.get("influence", 0.0))
	return a_score > b_score

func _compare_culture_resonance(a: Dictionary, b: Dictionary) -> bool:
	var a_out = abs(float(a.get("output", 0.0)))
	var b_out = abs(float(b.get("output", 0.0)))
	if is_equal_approx(a_out, b_out):
		var a_weight = abs(float(a.get("weight", 0.0)))
		var b_weight = abs(float(b.get("weight", 0.0)))
		return a_weight > b_weight
	return a_out > b_out

func _compare_resonance_total(a: Dictionary, b: Dictionary) -> bool:
	var a_out: float = absf(float(a.get("output", 0.0)))
	var b_out: float = absf(float(b.get("output", 0.0)))
	return a_out > b_out

func _join_strings(values: PackedStringArray) -> String:
	var parts: Array[String] = []
	for value in values:
		parts.append(String(value))
	var result = ""
	for i in range(parts.size()):
		result += parts[i]
		if i < parts.size() - 1:
			result += ", "
	return result
