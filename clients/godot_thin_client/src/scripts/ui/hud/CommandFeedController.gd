class_name CommandFeedController
extends RefCounted

## Owns the left-dock command feed card: the rolling list of command/event
## entries, its de-duplication, and the internal-scroll sizing. Extracted from
## HudLayer (composition — Hud holds one of these and delegates). Behaviour is
## unchanged; only the ownership moved.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

const COMMAND_FEED_LIMIT := 6
const COMMAND_FEED_MIN_HEIGHT := 72.0
const COMMAND_FEED_BOTTOM_MARGIN := 12.0

# ---- per-kind styling (The Telling, docs/plan_the_telling.md) ---------------
# The feed was kind-agnostic, so `narrative_beat` rendered as the literal bold string "Narrative
# beat" — the wire kind capitalized. A narrative line is PROSE, not a command echo: it carries no
# `Turn N` prefix and no bold-label/italic-detail split, just the line itself with its gloss as the
# dim detail. Anything not listed here falls through to the original capitalize behaviour.
const KIND_NARRATIVE_BEAT := "narrative_beat"
const KIND_NARRATIVE_FORK := "narrative_fork"
# Both glyphs are LINE ART, not pictographic emoji — the same rule MagnifierButton and the policy
# icons were forced into: an emoji-presentation glyph (❞ / ❔) renders as tofu or a featureless
# blob at feed size. Verified at true size in `narrative_feed.png`.
const KIND_STYLE := {
	KIND_NARRATIVE_BEAT: {"glyph": "»", "color": HudStyle.INK_DIM_HEX},
	# A fork is a question put to the people — the same mark the orb's decision row wears.
	KIND_NARRATIVE_FORK: {"glyph": "?", "color": HudStyle.SIGNAL_HEX},
}
const NARRATIVE_LINE_FORMAT := "[color=#%s]%s[/color]  %s"
const COMMAND_TURN_COLOR_HEX := "8fd4ff"

var _panel: PanelCard = null
var _scroll: ScrollContainer = null
var _label: RichTextLabel = null
var _dock_scroll: ScrollContainer = null

var _entries: Array = []
var _signatures: Dictionary = {}

func _init(panel: PanelCard, scroll: ScrollContainer, label: RichTextLabel, dock_scroll: ScrollContainer) -> void:
	_panel = panel
	_scroll = scroll
	_label = label
	_dock_scroll = dock_scroll

## Merge a batch of server command-event dicts (`{tick, kind, label, detail}`),
## de-duplicated by their signature, then re-render.
func ingest_events(events_variant: Variant) -> void:
	if _label == null or not (events_variant is Array):
		return
	var events_array: Array = events_variant
	for entry_variant in events_array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var tick: int = int(entry.get("tick", -1))
		var kind: String = String(entry.get("kind", "")).strip_edges()
		var label: String = String(entry.get("label", "")).strip_edges()
		var detail: String = String(entry.get("detail", "")).strip_edges()
		var signature := "%d|%s|%s|%s" % [tick, kind, label, detail]
		if _signatures.has(signature):
			continue
		_signatures[signature] = true
		_append_entry(tick, kind, label, detail)
	render()

## Push a client-side note (no turn tick) and re-render.
func note(label: String, detail: String) -> void:
	_append_entry(-1, "", label, detail)
	render()

func reset() -> void:
	_entries.clear()
	_signatures.clear()
	render()

func _append_entry(tick: int, kind: String, label: String, detail: String) -> void:
	if KIND_STYLE.has(kind):
		_append_narrative_entry(kind, label, detail)
		return
	var prefix := kind.capitalize() if kind != "" else "Command"
	var summary := label if label != "" else prefix
	var turn_fragment := ""
	if tick >= 0:
		turn_fragment = "[color=#%s]Turn %d[/color]  " % [COMMAND_TURN_COLOR_HEX, tick]
	var message := "%s[b]%s[/b]" % [turn_fragment, prefix]
	if summary != "" and summary != prefix:
		message += " — %s" % summary
	if detail != "":
		message += "\n[i]%s[/i]" % detail
	_entries.append(message)
	_trim()

## A narrative line: the prose itself behind a kind glyph, with the gloss as the dim detail. No
## `Turn N` prefix and no bold kind label — a beat is the world speaking, not a command receipt.
func _append_narrative_entry(kind: String, label: String, detail: String) -> void:
	var style: Dictionary = KIND_STYLE[kind]
	var line := label if label != "" else detail
	var message: String = NARRATIVE_LINE_FORMAT % [String(style["color"]), String(style["glyph"]), line]
	if detail != "" and detail != line:
		message += "\n[i][color=#%s]%s[/color][/i]" % [HudStyle.INK_DIM_HEX, detail]
	_entries.append(message)
	_trim()

func _trim() -> void:
	while _entries.size() > COMMAND_FEED_LIMIT:
		_entries.pop_front()

func render() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = true
	if _entries.is_empty():
		_label.text = "[i]No command activity yet.[/i]"
	else:
		_label.text = "\n\n".join(_entries)
	# The feed grows to fit but stays within the dock so only it scrolls, not the
	# whole stack; the label needs a frame to re-lay out before its content height
	# and position are accurate.
	call_deferred("_resize")

## Grow the feed's scroll region to fit its entries, capped to the space
## remaining in the dock below the panels above it (so the feed scrolls
## internally rather than dragging the fixed panels through the dock scroll),
## then scroll to the newest (bottom) entry.
func _resize() -> void:
	if _scroll == null or _label == null:
		return
	var cap: float = _label.get_content_height()
	if _dock_scroll != null and _dock_scroll.size.y > 0.0:
		var top_in_dock: float = _scroll.global_position.y - _dock_scroll.global_position.y
		var available: float = _dock_scroll.size.y - top_in_dock - COMMAND_FEED_BOTTOM_MARGIN
		cap = min(cap, max(available, COMMAND_FEED_MIN_HEIGHT))
	_scroll.custom_minimum_size.y = max(cap, 0.0)
	_scroll.set_deferred("scroll_vertical", 1000000)
