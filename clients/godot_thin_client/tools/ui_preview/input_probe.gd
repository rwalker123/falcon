extends RefCounted

## **DRIVING REAL POINTER INPUT THROUGH THE REAL DISPATCH.** All-`static`, no state — the shared
## layer a chapter reaches for when its claim is about ROUTING rather than about a rendered pixel.
##
## Everything here goes through `Viewport.push_input`, which runs the engine's own pass in order —
## GUI picking first, `_unhandled_input` after — so what a probe measures is what the live client
## would do with the same event. Reading a node's `mouse_filter` back instead would only say what the
## node was CONFIGURED as, never what the Viewport does with it.

## Canvas coordinates → WINDOW coordinates, which is what `push_input` takes. `project.godot`
## stretches `canvas_items` from a 1920 base with an `expand` aspect and a harness pins its own
## canvas, so a control's rect and an input position are not guaranteed to be in the same units.
static func canvas_to_window(viewport: Viewport, window: Window, canvas_point: Vector2) -> Vector2:
	var canvas: Vector2 = viewport.get_visible_rect().size
	if canvas.x <= 0.0 or canvas.y <= 0.0:
		return canvas_point
	var window_size := Vector2(window.size)
	return Vector2(
		canvas_point.x / canvas.x * window_size.x,
		canvas_point.y / canvas.y * window_size.y)

## Move the pointer there, and let the GUI pass re-pick what is under it. A gesture is routed to the
## HOVERED control, and `MapView._pointer_claimed_by_ui` reads that same hover, so a probe that
## teleports an event to a point the pointer was never over is testing nothing the client does.
static func hover(viewport: Viewport, window_point: Vector2) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position = window_point
	viewport.push_input(motion)

## A trackpad two-finger scroll. **This is what a macOS scroll actually is** — a Magic Mouse or
## trackpad delivers `InputEventPanGesture`, not a wheel button, which is why the reported defect
## PANNED the map rather than zooming it.
static func pan_gesture(viewport: Viewport, window_point: Vector2, delta: Vector2) -> void:
	var gesture := InputEventPanGesture.new()
	gesture.position = window_point
	gesture.delta = delta
	viewport.push_input(gesture)

## A trackpad pinch.
static func magnify_gesture(viewport: Viewport, window_point: Vector2, factor: float) -> void:
	var gesture := InputEventMagnifyGesture.new()
	gesture.position = window_point
	gesture.factor = factor
	viewport.push_input(gesture)

## One wheel notch, press and release. Godot delivers a wheel as a button PAIR and latches
## `gui.mouse_focus` on the press, so a probe that never releases routes every later press to
## whatever took this one, wherever it lands.
static func wheel(viewport: Viewport, window_point: Vector2, button_index: int) -> void:
	var press := InputEventMouseButton.new()
	press.button_index = button_index
	press.pressed = true
	press.position = window_point
	viewport.push_input(press)
	var release := InputEventMouseButton.new()
	release.button_index = button_index
	release.pressed = false
	release.position = window_point
	viewport.push_input(release)

## A left press ALONE, left latched. The half of a click a control that opens a POPUP answers on:
## `OptionButton` and `MenuButton` run at `ACTION_MODE_BUTTON_PRESS`, so the popup is up before the
## release exists, and a probe that must look at the popup — or click INTO it — needs the two halves
## driven apart with frames in between. Pair it with `release_left` at the point the gesture ends;
## leaving a press unreleased latches `gui.mouse_focus` and routes every later press to this control.
static func press_left(viewport: Viewport, window_point: Vector2) -> void:
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = window_point
	viewport.push_input(press)

## The release half of the pair above, delivered where the gesture ENDS — which is not always where it
## began: a popup entry is picked by a press-and-release over the entry, and a `BaseButton` that saw
## the pointer leave answers a release elsewhere by cancelling instead of firing.
static func release_left(viewport: Viewport, window_point: Vector2) -> void:
	var release := InputEventMouseButton.new()
	release.button_index = MOUSE_BUTTON_LEFT
	release.pressed = false
	release.position = window_point
	viewport.push_input(release)

## A left click, pressed AND released. The release is not optional: a press on its own latches
## `gui.mouse_focus`, and Godot then routes later presses to that control without re-picking, so
## probe 2 onwards would report probe 1's answer wherever it landed. The pointer is moved AWAY first
## so a `BaseButton` holding the press sees it leave and clears `pressing_inside` — the release then
## CANCELS the click instead of firing it, which is what keeps a probe over live HUD chrome from
## pressing something mid-assertion.
static func left_click(viewport: Viewport, window_point: Vector2, park: Vector2) -> void:
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = window_point
	viewport.push_input(press)
	var motion := InputEventMouseMotion.new()
	motion.position = park
	motion.relative = park - window_point
	viewport.push_input(motion)
	var release := InputEventMouseButton.new()
	release.button_index = MOUSE_BUTTON_LEFT
	release.pressed = false
	release.position = park
	viewport.push_input(release)
