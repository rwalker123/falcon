class_name OpeningCardController
extends RefCounted

## THE OPENING HAND-OFF — the controller half of `OpeningCardPanel`. Once per world it puts the
## Telling's opening lines (every tick-0 `narrative_beat`) on a modal card IN PLACE OF the outfitting
## card's own first auto-open, and the card's one way off opens that outfitting card.
##
## `HudLayer` holds one as `_opening`, hands it a HOST `Node` to parent the card into (a `RefCounted`
## cannot `add_child`) and the `StartingLoadoutController` it hands off to — a typed collaborator,
## the `TurnOrbController.set_starting_loadout_panel` shape — and wires that controller's
## `opening_grant_held` signal to `hold_opening_grant`.
##
## ## ⛔ THE TWO INPUTS ARRIVE IN EITHER ORDER, SO NOTHING IS DECIDED WHERE EITHER LANDS
##
## Within one snapshot `Main` dispatches the roster (`update_band_alerts` → the outfitting window)
## BEFORE `command_events` (the beats), so on the opening frame the window is held before the story
## has been read. And across snapshots either may come first. So both inputs only RECORD, and one
## deferred `_resolve` — run at the end of the frame, after every dispatch of that snapshot — decides:
##
## - **held band, story present** → the opening card, the outfitting card left at its pill under it.
## - **held band, no story** (a save loaded mid-opening, beats absent) → the outfitting card opens as
##   it always did. If the story then arrives in a LATER snapshot while that band's window is still
##   open, the card is raised over the outfitting card and hands back to it.
## - **story, no held band yet** → nothing; the hold will resolve it.
##
## ⛔ **THE OUTFITTING CARD IS NEVER LOST.** The held card is already rendered at its reopen pill (see
## `StartingLoadoutController._holds_for_opening`), and every way off the opening card — the button,
## a catcher click, ESC — opens it (`_on_handed_off`).
##
## ONCE PER WORLD: `reset_world_state` (the world boundary) re-arms it, and nothing on an ordinary
## snapshot does. Splinter windows later in the game never reach here — the hold is the world's first
## auto-open only.

## The world's opening tick — the tick the sim says its opening beats on.
const OPENING_TICK := 0
## What an event with no `tick` reads as — never the opening tick, so it is never taken.
const UNKNOWN_TICK := -1
## The card's node name under the HUD layer.
const PANEL_NODE_NAME := "OpeningCardPanel"
## The command-event kind this card reads — the Telling's own definition, not a second spelling.
const KIND_NARRATIVE_BEAT := TellingPanel.KIND_NARRATIVE_BEAT

# --- Collaborators handed in by HudLayer ---
var _host: Node = null
var _loadout: StartingLoadoutController = null
var _panel: OpeningCardPanel = null

# --- One world's opening ---
## The opening lines, in arrival order. The beat's rendered prose (`label`); its gloss is not shown.
var _story: Array[String] = []
## `tick|label|detail` of every beat already taken — a full snapshot re-sends the whole ring, so a
## re-ingest must add nothing.
var _signatures: Dictionary = {}
## The band whose grant window was held for the opening. `HudConst.NO_BAND_ID` until the hold.
var _held_band: int = HudConst.NO_BAND_ID
## The outfitting card has been released without a story — the late-story case may still raise one.
var _released_without_story: bool = false
## The card has been shown this world. Once true, nothing raises it again until the world changes.
var _shown: bool = false
## A `_resolve` is already queued for the end of this frame.
var _resolve_queued: bool = false

func setup(host: Node, loadout: StartingLoadoutController) -> void:
	_host = host
	_loadout = loadout

# ---- ingest -----------------------------------------------------------------

## The opening beats, off the same command-event array the Telling panel reads. Only tick-0
## `narrative_beat` rows are kept; a resolve is queued only when one is new.
func ingest_command_events(events_variant: Variant) -> void:
	if not (events_variant is Array):
		return
	var appended := false
	for entry_variant in (events_variant as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if String(entry.get("kind", "")).strip_edges() != KIND_NARRATIVE_BEAT:
			continue
		var tick := int(entry.get("tick", UNKNOWN_TICK))
		if tick != OPENING_TICK:
			continue
		var label := String(entry.get("label", "")).strip_edges()
		if label.is_empty():
			continue
		var signature := "%d|%s|%s" % [tick, label, String(entry.get("detail", "")).strip_edges()]
		if _signatures.has(signature):
			continue
		_signatures[signature] = true
		_story.append(label)
		appended = true
	if appended:
		_queue_resolve()

## `StartingLoadoutController.opening_grant_held` — the world's first auto-open was a grant, and it is
## waiting at its pill.
func hold_opening_grant(band_id: int) -> void:
	_held_band = band_id
	_queue_resolve()

## A world rebuild: the previous world's opening is not this one's.
func reset_world_state() -> void:
	_story = []
	_signatures = {}
	_held_band = HudConst.NO_BAND_ID
	_released_without_story = false
	_shown = false
	if _panel != null and is_instance_valid(_panel):
		_panel.close()

# ---- queries / ESC ----------------------------------------------------------

func is_open() -> bool:
	return _panel != null and is_instance_valid(_panel) and _panel.is_open()

## ESC on the card — the same hand-off as its button.
func dismiss() -> void:
	if is_open():
		_panel.dismiss()

## The card node, for the harnesses.
func panel() -> OpeningCardPanel:
	return _panel

## The opening lines held this world, for the harnesses.
func story() -> Array[String]:
	return _story.duplicate()

# ---- resolve ----------------------------------------------------------------

func _queue_resolve() -> void:
	if _resolve_queued:
		return
	_resolve_queued = true
	call_deferred("_resolve")

func _resolve() -> void:
	_resolve_queued = false
	if _shown or _held_band == HudConst.NO_BAND_ID:
		return
	if _story.is_empty():
		if not _released_without_story:
			_released_without_story = true
			_loadout.open_band(_held_band)
		return
	# The story came late: only while the opening is still on, i.e. the held band's window is open.
	if not _loadout.has_window(_held_band):
		return
	_shown = true
	_loadout.collapse()
	_ensure_panel()
	_panel.show_lines(_story)

func _ensure_panel() -> void:
	if _panel != null and is_instance_valid(_panel):
		return
	_panel = OpeningCardPanel.new()
	_panel.name = PANEL_NODE_NAME
	_panel.handed_off.connect(_on_handed_off)
	_host.add_child(_panel)

func _on_handed_off() -> void:
	_loadout.open_band(_held_band)
