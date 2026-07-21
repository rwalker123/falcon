extends Control
class_name FoodOutlookChart

## The band's FOOD OUTLOOK — one merged projection of the larder over the arrivals horizon.
##
## The per-source tick strips (`ArrivalStrip`) say WHEN each source delivers; this says whether the
## band survives the gaps between those deliveries — "do I make it to the next haul?". The sim
## deliberately emits per-source schedules and no merged projection, so the merge is composed here:
## it is presentation arithmetic over sim-supplied numbers (arrivals, consumption, pen feed) and
## re-derives no yield and no ecology.
##
## The walk, per turn i of the horizon:
##     food = max(0, food + Σ arrival_schedule[i] over the band's sources − drain)
## with `drain` (consumption + pen feed) held FLAT across the projection — this is a "if nothing
## changes" readout, not a forecast of the player's future decisions.

## Chart box: wide enough to give 20 turns a legible tick, short enough to stay a glanceable readout
## rather than an analytics chart. The width sits inside `BandCityPanel.SECTION_COLUMN_WIDTH` so the
## wide-column packer never has to grow a column for it.
const CHART_MIN_WIDTH := 300.0
const CHART_HEIGHT := 84.0
## Inner padding of the plot area (top leaves room for the peak, bottom for the baseline + label).
const PLOT_MARGIN_LEFT := 2.0
const PLOT_MARGIN_RIGHT := 2.0
const PLOT_MARGIN_TOP := 6.0
const PLOT_MARGIN_BOTTOM := 12.0
## Floor on the y-axis so a band with a near-empty larder still draws a readable curve instead of a
## flat line pinned to the top of the box.
const MIN_Y_SCALE := 1.0
## Headroom above the peak so the line never touches the top edge.
const Y_HEADROOM := 1.12

const LINE_WIDTH := 2.0
const AREA_ALPHA := 0.18
const BASELINE_WIDTH := 1.0
const HAUL_DOT_RADIUS := 2.5
## Dashed "runs out here" vertical: dash + gap length, in pixels.
const EMPTY_DASH_LENGTH := 3.0
const EMPTY_GAP_LENGTH := 3.0
const EMPTY_MARKER_WIDTH := 1.0
const EMPTY_LABEL_FONT_SIZE := 9
const EMPTY_LABEL_OFFSET := Vector2(3.0, -1.0)
const EMPTY_LABEL_FORMAT := "empty ~turn %d"
const EMPTY_LABEL_RELATIVE_FORMAT := "empty in %d turns"
## `current_turn` sentinel — see `ArrivalStrip.UNKNOWN_TURN`.
const UNKNOWN_TURN := -1
## No turn in the horizon empties the larder.
const NO_EMPTY_TURN := -1

var _series: PackedFloat32Array = PackedFloat32Array()
var _arrivals: PackedFloat32Array = PackedFloat32Array()
var _empty_index: int = NO_EMPTY_TURN
var _current_turn: int = UNKNOWN_TURN

func _init() -> void:
	custom_minimum_size = Vector2(CHART_MIN_WIDTH, CHART_HEIGHT)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	mouse_filter = Control.MOUSE_FILTER_IGNORE

## Compose + store the projection. `start_food` is the band's current larder (provisions),
## `arrivals[i]` the merged food landing i+1 turns from now, `drain` the flat per-turn cost
## (consumption + pen feed). `current_turn` labels the empty marker (`UNKNOWN_TURN` → relative).
func set_projection(start_food: float, arrivals: PackedFloat32Array, drain: float, current_turn: int) -> void:
	_arrivals = arrivals
	_current_turn = current_turn
	_empty_index = NO_EMPTY_TURN
	_series = PackedFloat32Array()
	# Point 0 is NOW (before any arrival), so point i+1 is the larder after turn i resolves.
	var food: float = maxf(start_food, 0.0)
	_series.push_back(food)
	for i in range(arrivals.size()):
		food = maxf(food + arrivals[i] - drain, 0.0)
		_series.push_back(food)
		if _empty_index == NO_EMPTY_TURN and food <= 0.0:
			_empty_index = i
	queue_redraw()

func _draw() -> void:
	if _series.size() < 2:
		return
	var plot := Rect2(
		PLOT_MARGIN_LEFT,
		PLOT_MARGIN_TOP,
		maxf(size.x - PLOT_MARGIN_LEFT - PLOT_MARGIN_RIGHT, 0.0),
		maxf(size.y - PLOT_MARGIN_TOP - PLOT_MARGIN_BOTTOM, 0.0))
	if plot.size.x <= 0.0 or plot.size.y <= 0.0:
		return
	var y_max := MIN_Y_SCALE
	for value in _series:
		y_max = maxf(y_max, value)
	y_max *= Y_HEADROOM
	var points := PackedVector2Array()
	for i in range(_series.size()):
		points.push_back(_point(plot, i, _series[i], y_max))
	# Faint baseline first, so the filled area sits on top of it rather than hiding a seam.
	var baseline_y := plot.position.y + plot.size.y
	draw_line(Vector2(plot.position.x, baseline_y),
		Vector2(plot.position.x + plot.size.x, baseline_y), HudStyle.LINE_SOFT, BASELINE_WIDTH)
	# Filled area: the curve closed down to the baseline.
	var area := PackedVector2Array(points)
	area.push_back(Vector2(points[points.size() - 1].x, baseline_y))
	area.push_back(Vector2(points[0].x, baseline_y))
	var fill := HudStyle.SIGNAL
	fill.a = AREA_ALPHA
	draw_colored_polygon(area, fill)
	draw_polyline(points, HudStyle.SIGNAL, LINE_WIDTH, true)
	# A dot on every turn a haul actually lands (point i+1 is the larder AFTER turn i's delivery).
	for i in range(_arrivals.size()):
		if _arrivals[i] > 0.0 and i + 1 < points.size():
			draw_circle(points[i + 1], HAUL_DOT_RADIUS, HudStyle.HEALTHY)
	if _empty_index != NO_EMPTY_TURN:
		_draw_empty_marker(plot, points[_empty_index + 1].x)

## The dashed DANGER vertical + its label at the turn the larder first hits zero.
func _draw_empty_marker(plot: Rect2, x: float) -> void:
	var y := plot.position.y
	var bottom := plot.position.y + plot.size.y
	while y < bottom:
		var segment_end: float = minf(y + EMPTY_DASH_LENGTH, bottom)
		draw_line(Vector2(x, y), Vector2(x, segment_end), HudStyle.DANGER, EMPTY_MARKER_WIDTH)
		y = segment_end + EMPTY_GAP_LENGTH
	var text := EMPTY_LABEL_RELATIVE_FORMAT % (_empty_index + 1) if _current_turn <= UNKNOWN_TURN \
		else EMPTY_LABEL_FORMAT % (_current_turn + _empty_index + 1)
	var font: Font = ThemeDB.fallback_font
	var text_width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, EMPTY_LABEL_FONT_SIZE).x
	# Flip the label to the left of the marker when it would run off the right edge.
	var label_x := x + EMPTY_LABEL_OFFSET.x
	if label_x + text_width > size.x:
		label_x = x - EMPTY_LABEL_OFFSET.x - text_width
	draw_string(font, Vector2(label_x, size.y + EMPTY_LABEL_OFFSET.y), text,
		HORIZONTAL_ALIGNMENT_LEFT, -1, EMPTY_LABEL_FONT_SIZE, HudStyle.DANGER)

func _point(plot: Rect2, index: int, value: float, y_max: float) -> Vector2:
	var t := float(index) / float(_series.size() - 1)
	return Vector2(
		plot.position.x + t * plot.size.x,
		plot.position.y + (1.0 - clampf(value / y_max, 0.0, 1.0)) * plot.size.y)
