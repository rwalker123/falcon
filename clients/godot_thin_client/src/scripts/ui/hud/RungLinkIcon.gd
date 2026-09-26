class_name RungLinkIcon
extends Control

## **A POOLING LINK'S RUNG, AS A GLYPH IN THE RUNG'S OWN WEIGHT** (issue #731) — the camps panel's link
## column. A path is a dotted line, a trail a dashed one, a dirt road a solid stroke in pigment and a
## paved road a double line; open ground (`""`, no kept road on the run) is a faint dot. The weight
## rises with the rung, so a list of links reads as a list of how well each is held without a legend.
##
## The icon is keyed on the rung id's NAME (`trail` off `route:trail`); a rung this table does not
## know draws the plain solid stroke, so a fifth rung gets an honest line and its catalog word
## (`TradeZoneController`) rather than nothing.

const ICON_SIZE := Vector2(24.0, 10.0)
## The stroke runs the box's width, inset so a round cap is not clipped.
const STROKE_INSET := 1.0
const PATH_DOT_WIDTH := 1.8
const PATH_DOT_STEP := 3.5
const TRAIL_DASH := 4.0
const TRAIL_GAP := 2.5
const TRAIL_WIDTH := 1.4
const ROAD_WIDTH := 2.6
const PAVED_WIDTH := 1.3
## The paved road's two rails, above and below the centre line.
const PAVED_RAIL_OFFSET := 2.0
const OPEN_GROUND_DOT_RADIUS := 1.5

var _rung_name: String = ""

static func make(rung_id: String) -> RungLinkIcon:
	var icon := RungLinkIcon.new()
	icon._rung_name = TradeLedger.rung_name(rung_id)
	icon.custom_minimum_size = ICON_SIZE
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return icon

func rung_name() -> String:
	return _rung_name

## The ink the rung is drawn in — faint for the free rungs, pigment for the built road, full ink for
## the paved one.
static func ink_for(rung_name: String) -> Color:
	match rung_name:
		HudTradeVocab.RUNG_ICON_PATH, HudTradeVocab.OPEN_GROUND_RUNG:
			return HudStyle.INK_FAINT
		HudTradeVocab.RUNG_ICON_TRAIL:
			return HudStyle.INK_DIM
		HudTradeVocab.RUNG_ICON_DIRT_ROAD:
			return HudStyle.VOICE_PIGMENT
	return HudStyle.INK

func _draw() -> void:
	var ink := ink_for(_rung_name)
	var mid := ICON_SIZE.y * 0.5
	var left := STROKE_INSET
	var right := ICON_SIZE.x - STROKE_INSET
	match _rung_name:
		HudTradeVocab.OPEN_GROUND_RUNG:
			draw_circle(Vector2(ICON_SIZE.x * 0.5, mid), OPEN_GROUND_DOT_RADIUS, ink)
		HudTradeVocab.RUNG_ICON_PATH:
			var x := left
			while x <= right:
				draw_circle(Vector2(x, mid), PATH_DOT_WIDTH * 0.5, ink)
				x += PATH_DOT_STEP
		HudTradeVocab.RUNG_ICON_TRAIL:
			var x := left
			while x < right:
				draw_line(Vector2(x, mid), Vector2(minf(x + TRAIL_DASH, right), mid), ink, TRAIL_WIDTH)
				x += TRAIL_DASH + TRAIL_GAP
		HudTradeVocab.RUNG_ICON_PAVED_ROAD:
			draw_line(Vector2(left, mid - PAVED_RAIL_OFFSET), Vector2(right, mid - PAVED_RAIL_OFFSET),
				ink, PAVED_WIDTH)
			draw_line(Vector2(left, mid + PAVED_RAIL_OFFSET), Vector2(right, mid + PAVED_RAIL_OFFSET),
				ink, PAVED_WIDTH)
		_:
			draw_line(Vector2(left, mid), Vector2(right, mid), ink, ROAD_WIDTH)
