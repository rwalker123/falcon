extends Node

## Dev-only verifier for the turn-orb popover click fix (the full-screen dismiss catcher used to
## swallow the popover's button clicks). Real synthetic mouse routing via `Viewport.push_input` is
## unreliable in this headless/script environment (an isolated button-in-backdrop also fails to
## route), so this asserts the two things that DO determine correctness:
##
##  1. STRUCTURE — the popover is a DESCENDANT of the catcher (a child, not a sibling). A child
##     renders + picks ABOVE its parent, so the catcher can no longer sit on top of the buttons.
##     This is the actual fix; the old code added catcher + popover as sibling top_level nodes.
##  2. WIRING — each popover button's `pressed` reaches its signal: the footer Advance emits
##     `advance_requested`, a reason row emits `focus_requested`.
##
## The remaining OS-input → button link is guaranteed by the canonical nested-modal structure (1)
## and is confirmed live in-app by a human. Prints PASS/FAIL, exits non-zero on failure.
##
##   godot --headless --path . res://tools/turn_orb_click_probe.tscn

const TURN_ORB_SCENE := preload("res://src/ui/TurnOrb.tscn")

var _orb: TurnOrb
var _advance_fired := false
var _focus_fired := false

func _ready() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	_orb = TURN_ORB_SCENE.instantiate()
	layer.add_child(_orb)
	_orb.advance_requested.connect(func() -> void: _advance_fired = true)
	_orb.focus_requested.connect(func(_x: int, _y: int) -> void: _focus_fired = true)
	await _frames(2)

	var ok := true

	# --- Test 1: all-clear popover — structure + Advance wiring ---
	_orb.set_attention([])
	_orb.open_popover()
	await _frames(2)
	var catcher: Control = _orb._catcher
	var popover: Control = _orb._popover
	var nested := catcher != null and popover != null and popover.get_parent() == catcher
	var catcher_stops := catcher != null and catcher.mouse_filter == Control.MOUSE_FILTER_STOP and catcher.top_level
	print("turn_orb_click_probe: popover nested in catcher? ", nested, " ; catcher is full-screen STOP layer? ", catcher_stops)
	ok = ok and nested and catcher_stops

	var advance := _find_button_with_text(catcher, "Advance")
	if advance == null:
		push_error("turn_orb_click_probe: Advance button not found under the catcher")
		ok = false
	else:
		advance.pressed.emit()
		await _frames(1)
	print("turn_orb_click_probe: advance_requested fired from Advance.pressed? ", _advance_fired)
	ok = ok and _advance_fired

	# --- Test 2: attention popover — a reason row emits focus_requested ---
	_orb.set_attention([
		{"kind": "starving", "severity": "critical", "label": "Band 1 starving", "detail": "3 days", "x": 12, "y": 8},
	])
	_orb.open_popover()
	await _frames(2)
	# The reason row is re-nested under the catcher too.
	var re_nested := _orb._popover != null and _orb._popover.get_parent() == _orb._catcher
	ok = ok and re_nested
	var reason := _find_reason_button(_orb._catcher)
	if reason == null:
		push_error("turn_orb_click_probe: reason row button not found under the catcher")
		ok = false
	else:
		reason.pressed.emit()
		await _frames(1)
	print("turn_orb_click_probe: focus_requested fired from reason-row.pressed? ", _focus_fired)
	ok = ok and _focus_fired

	if ok:
		print("turn_orb_click_probe: PASS — popover nested in catcher (picks above it) + button wiring intact")
		get_tree().quit(0)
	else:
		push_error("turn_orb_click_probe: FAIL")
		get_tree().quit(1)

func _frames(n: int) -> void:
	for _i in range(n):
		await get_tree().process_frame

func _find_button_with_text(node: Node, needle: String) -> Button:
	if node == null:
		return null
	if node is Button and String((node as Button).text).findn(needle) >= 0:
		return node
	for child in node.get_children():
		var found := _find_button_with_text(child, needle)
		if found != null:
			return found
	return null

## The first Button that is NOT the Advance footer (reason rows carry their text in child labels,
## so their own `text` is empty).
func _find_reason_button(node: Node) -> Button:
	if node == null:
		return null
	if node is Button and String((node as Button).text).findn("Advance") < 0:
		return node
	for child in node.get_children():
		var found := _find_reason_button(child)
		if found != null:
			return found
	return null
