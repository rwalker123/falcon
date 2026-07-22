class_name CommandFeedController
extends RefCounted

## Owns the left-dock command feed card: the rolling list of command/event
## entries, its de-duplication, and the internal-scroll sizing. Extracted from
## HudLayer (composition — Hud holds one of these and delegates). Behaviour is
## unchanged; only the ownership moved.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")
const DockScrollFit := preload("res://src/scripts/ui/hud/DockScrollFit.gd")
const TellingPanelScript := preload("res://src/scripts/ui/TellingPanel.gd")

const COMMAND_FEED_LIMIT := 6
const COMMAND_FEED_MIN_HEIGHT := 72.0
const COMMAND_FEED_BOTTOM_MARGIN := 12.0
# THE FEED MAY NEVER OWN MORE THAN THIS SHARE OF ITS DOCK. It shares the left column with the
# selection card, which is where the VERBS are — and six receipts at full height are tall enough to
# push that card's compose block below the fold, which is exactly the failure the one-card layout
# exists to remove. The feed is a log you glance at: past this share it scrolls internally instead
# of growing. (The Telling panel bounds itself the same way, against the viewport.)
const COMMAND_FEED_MAX_DOCK_FRACTION := 0.4

const COMMAND_TURN_COLOR_HEX := "8fd4ff"

var _panel: PanelCard = null
var _scroll: ScrollContainer = null
var _label: RichTextLabel = null
var _dock_scroll: ScrollContainer = null

var _entries: Array = []
var _signatures: Dictionary = {}
# Hidden by default — six read-only receipts and NO verbs, so the selection card gets the dock
# height instead and the player opens the feed with `R` when they want the log. Hud overrides this
# from the persisted prefs on ready. It lives here rather than in Hud because `render()` runs on
# every ingest: a controller that unconditionally shows its panel would undo the dock's
# `set_relevant(false)` the moment any command receipt arrived.
var feed_suppressed: bool = true

func _init(panel: PanelCard, scroll: ScrollContainer, label: RichTextLabel, dock_scroll: ScrollContainer) -> void:
	_panel = panel
	_scroll = scroll
	_label = label
	_dock_scroll = dock_scroll

## Merge a batch of server command-event dicts (`{tick, kind, label, detail}`),
## de-duplicated by their signature, then re-render.
##
## NARRATIVE KINDS ARE SKIPPED — they belong to `TellingPanel` (see its header for why: a receipt
## and a telling want opposite retention and density, and two beats used to fill this whole card
## and push the receipts off). The test lives THERE, not here, so a kind can never be claimed by
## both surfaces or dropped by both.
func ingest_events(events_variant: Variant) -> void:
	if _label == null or not (events_variant is Array):
		return
	var events_array: Array = events_variant
	for entry_variant in events_array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var kind: String = String(entry.get("kind", "")).strip_edges()
		if TellingPanelScript.handles_kind(kind):
			continue
		var tick: int = int(entry.get("tick", -1))
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

func _trim() -> void:
	while _entries.size() > COMMAND_FEED_LIMIT:
		_entries.pop_front()

## Restore a persisted suppress state (Hud's prefs load) / flip it from the `R` hotkey. Mirrors
## `LegendController.set_suppressed` / `toggle_suppressed` exactly — same pattern, same contract.
func set_suppressed(suppressed: bool) -> void:
	feed_suppressed = suppressed
	render()

func toggle_suppressed() -> void:
	set_suppressed(not feed_suppressed)

## Re-cap the feed against the room the dock has left NOW. The left dock holds two cards that grow
## — the selection drawer and this feed — and they share one height budget, so whichever fits second
## must measure the other in its settled state or their sum overflows the dock. Hud fits the drawer
## first and then calls this. No-op while suppressed (a hidden card claims nothing).
func refit() -> void:
	if feed_suppressed:
		return
	_resize()

func render() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = not feed_suppressed
	if feed_suppressed:
		return
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
	var content: float = _label.get_content_height()
	if _dock_scroll != null and _dock_scroll.size.y > 0.0:
		content = min(content, _dock_scroll.size.y * COMMAND_FEED_MAX_DOCK_FRACTION)
	DockScrollFit.fit_height(
		_scroll, content, _dock_scroll, COMMAND_FEED_MIN_HEIGHT, COMMAND_FEED_BOTTOM_MARGIN)
	# A receipt is worthless once read, so the feed ALWAYS snaps to newest — no read-position
	# preservation here (that is the Telling panel's concern, where scrolling back is the point).
	_scroll.set_deferred("scroll_vertical", int(_label.get_content_height()))
