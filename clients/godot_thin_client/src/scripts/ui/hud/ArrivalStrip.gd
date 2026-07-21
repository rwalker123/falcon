extends Control
class_name ArrivalStrip

## The per-source ARRIVAL TICK STRIP on a Current-actions row.
##
## The Band panel's "Food /turn" is a STEADY average (the honest long-run rate), which fixed the
## swinging number but hid the cadence: a whole-animal hunt hands over nothing for several turns and
## then a lump. This strip answers "when does it actually land?" — one cell per upcoming turn, read
## straight off the sim's `arrival_schedule` (index i = food delivered i+1 turns from now), so it is
## pinned to what the sim will actually pay rather than re-derived here.
##
## It renders ONLY when the schedule has a gap (`has_gap`): a source that delivers every turn has no
## lumpiness to explain, so a forage row stays clean. That gap test is the whole rule — deliberately
## NOT a kind check, because "lumpy" is a property of the projection, not of the word "hunt".
##
## An EMPTY schedule means "not projected" (Scout/Warrior, a rehydrated save), never famine — such a
## row simply gets no strip.

## Vertical size of the strip: about the height of a row's secondary (status) line, so it reads as
## part of that line rather than as a chart.
const STRIP_HEIGHT := 8.0
## Gap between adjacent turn cells.
const CELL_SEPARATION := 2.0
## Narrowest a cell may draw; below this the cells fuse into an unreadable bar, so the strip stops
## drawing rather than lying about the cadence.
const MIN_CELL_WIDTH := 1.0
## `current_turn` sentinel: the HUD has not seen an overlay update yet, so cells are labelled by
## OFFSET ("in N turns") instead of by absolute turn number.
const UNKNOWN_TURN := -1

const TOOLTIP_DELIVERY := "Turn %d — +%.2f food"
const TOOLTIP_EMPTY := "Turn %d — nothing lands"
const TOOLTIP_DELIVERY_RELATIVE := "In %d turns — +%.2f food"
const TOOLTIP_EMPTY_RELATIVE := "In %d turns — nothing lands"

var _schedule: PackedFloat32Array = PackedFloat32Array()
var _current_turn: int = UNKNOWN_TURN

## True when this schedule is worth drawing: it carries data AND at least one turn inside the horizon
## delivers nothing. A continuous source (every slot positive) returns false and gets no strip.
static func has_gap(schedule: PackedFloat32Array) -> bool:
	if schedule.is_empty():
		return false
	for amount in schedule:
		if amount <= 0.0:
			return true
	return false

func _init() -> void:
	custom_minimum_size = Vector2(0.0, STRIP_HEIGHT)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# Cells carry their own tooltip, so the strip must receive mouse events (and must not swallow
	# them for the row beneath it — STOP is what makes `_get_tooltip` fire at all).
	mouse_filter = Control.MOUSE_FILTER_STOP

## Feed the strip one source's projected arrivals. `current_turn` is the sim turn the snapshot is on
## (`UNKNOWN_TURN` before the first overlay update), used only to label the cells.
func set_schedule(schedule: PackedFloat32Array, current_turn: int) -> void:
	_schedule = schedule
	_current_turn = current_turn
	queue_redraw()

func _draw() -> void:
	var count := _schedule.size()
	if count <= 0:
		return
	var cell_width := _cell_width(count)
	if cell_width < MIN_CELL_WIDTH:
		return
	for i in range(count):
		var color: Color = HudStyle.HEALTHY if _schedule[i] > 0.0 else HudStyle.LINE_SOFT
		draw_rect(Rect2(_cell_left(i, cell_width), 0.0, cell_width, size.y), color)

func _get_tooltip(at_position: Vector2) -> String:
	var count := _schedule.size()
	if count <= 0:
		return ""
	var cell_width := _cell_width(count)
	if cell_width < MIN_CELL_WIDTH:
		return ""
	var index := clampi(int(at_position.x / (cell_width + CELL_SEPARATION)), 0, count - 1)
	var amount := _schedule[index]
	var offset := index + 1
	if _current_turn <= UNKNOWN_TURN:
		return TOOLTIP_DELIVERY_RELATIVE % [offset, amount] if amount > 0.0 \
			else TOOLTIP_EMPTY_RELATIVE % offset
	var turn := _current_turn + offset
	return TOOLTIP_DELIVERY % [turn, amount] if amount > 0.0 else TOOLTIP_EMPTY % turn

## Width of one cell so that `count` cells plus the separations exactly span the strip.
func _cell_width(count: int) -> float:
	return (size.x - CELL_SEPARATION * float(count - 1)) / float(count)

func _cell_left(index: int, cell_width: float) -> float:
	return float(index) * (cell_width + CELL_SEPARATION)
