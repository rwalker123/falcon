extends Control
class_name DisclosureTriangle

## **A FILLED DISCLOSURE TRIANGLE, DRAWN RATHER THAN SET IN TYPE** — `▼` while its section is open, `▶`
## while it is folded, its bounding box centred in this control's rect on both axes.
##
## ⛔ **IT IS DRAWN BECAUSE A GLYPH CANNOT BE CENTRED.** A font places `⌄` / `›` / `▼` on its baseline
## with the glyph's own side bearings, so a glyph centred in a Button's text box sits off-centre in the
## button by whatever the font's metrics say — and differently at every interface scale. A polygon in
## this control's own coordinates is centred by arithmetic, and the whole-UI scale transforms it with
## the button, so it stays centred at every scale.
##
## It takes no input (`MOUSE_FILTER_IGNORE`): it is laid over its button full-rect, and the BUTTON is
## the hit area.

## An equilateral triangle's height per unit of side.
const EQUILATERAL_HEIGHT_RATIO := 0.8660254

## The triangle's SIDE in this control's units — the base of `▼`, the height of `▶`.
var side: float = 8.0:
	set(value):
		side = value
		queue_redraw()

## `true` draws `▼` (open), `false` draws `▶` (folded).
var expanded: bool = true:
	set(value):
		expanded = value
		queue_redraw()

var color: Color = Color.WHITE:
	set(value):
		color = value
		queue_redraw()

func _init() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE

## The triangle's bounding box in this control's local coordinates — centred in `size` by
## construction. Public so a harness can assert the centring off the drawn geometry.
func drawn_rect() -> Rect2:
	var depth := side * EQUILATERAL_HEIGHT_RATIO
	var box := Vector2(side, depth) if expanded else Vector2(depth, side)
	return Rect2((size - box) * 0.5, box)

func _draw() -> void:
	var r := drawn_rect()
	var points: PackedVector2Array
	if expanded:
		points = PackedVector2Array([r.position, Vector2(r.end.x, r.position.y),
			Vector2(r.position.x + r.size.x * 0.5, r.end.y)])
	else:
		points = PackedVector2Array([r.position, Vector2(r.end.x, r.position.y + r.size.y * 0.5),
			Vector2(r.position.x, r.end.y)])
	draw_colored_polygon(points, color)
