extends Button
class_name MagnifierButton

## A zoom-rail button that draws a crisp magnifier icon in `_draw` (font magnifier
## glyphs render as desaturated blobs / tofu in Godot's default font). The lens
## carries a small `+` (zoom in) or `−` (zoom out) so the two buttons read
## distinctly. Monochrome, `HudStyle` ink → `SIGNAL` on hover, so it matches the
## console look; the ghost stylebox (bg/border) is applied separately by the HUD.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

## +1 draws a magnifier with a "+" (zoom in); -1 draws a "−" (zoom out).
@export var zoom_sign: int = 1

# Geometry, relative to the button's shorter side (named — no magic literals).
const LENS_CENTER_FACTOR := 0.42   # lens center, of the button's min side
const LENS_RADIUS := 6.5
const LENS_WIDTH := 1.6
const HANDLE_LEN := 6.0
const HANDLE_WIDTH := 2.0
const SIGN_HALF := 3.0             # half-length of the +/− strokes inside the lens
const SIGN_WIDTH := 1.6
const HANDLE_DIR := Vector2(0.7071, 0.7071)   # 45° down-right (normalized)

var _hovered := false

func _ready() -> void:
	mouse_entered.connect(_on_hover_changed.bind(true))
	mouse_exited.connect(_on_hover_changed.bind(false))

func _on_hover_changed(hovered: bool) -> void:
	_hovered = hovered
	queue_redraw()

func _draw() -> void:
	var color := HudStyle.SIGNAL if _hovered else HudStyle.INK
	var side := minf(size.x, size.y)
	var center := Vector2(side, side) * LENS_CENTER_FACTOR
	# Lens ring.
	draw_arc(center, LENS_RADIUS, 0.0, TAU, 32, color, LENS_WIDTH, true)
	# Handle: a short diagonal off the lens's lower-right toward the button corner.
	var handle_start := center + HANDLE_DIR * LENS_RADIUS
	draw_line(handle_start, handle_start + HANDLE_DIR * HANDLE_LEN, color, HANDLE_WIDTH, true)
	# Sign inside the lens: always the horizontal stroke; add the vertical for "+".
	draw_line(center - Vector2(SIGN_HALF, 0.0), center + Vector2(SIGN_HALF, 0.0), color, SIGN_WIDTH, true)
	if zoom_sign > 0:
		draw_line(center - Vector2(0.0, SIGN_HALF), center + Vector2(0.0, SIGN_HALF), color, SIGN_WIDTH, true)
