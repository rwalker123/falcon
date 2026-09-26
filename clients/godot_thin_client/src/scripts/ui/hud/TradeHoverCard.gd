extends PanelContainer
class_name TradeHoverCard

## **WHAT A TRADE ROW IS MADE OF** (issue #731) — a good's rating piles, each with its own signed
## amount, or a shipment's cargo pile by pile. The row states what a player scans for; this states
## what they check.
##
## **BESIDE THE ROW, NEVER UNDER THE POINTER, AND ON THE SIDE AWAY FROM AN OPEN OVERFLOW PANEL** — so
## it can never land on the list the player just opened. With no panel open it prefers the row's
## right and flips where there is no room. It takes no clicks (`MOUSE_FILTER_IGNORE` all the way down)
## and so can never steal the hover that keeps it up.
##
## A plain `PanelContainer` on the overflow panel's own `CanvasLayer`, not a Godot tooltip: a tooltip
## places itself at the pointer and cannot be told which side to take.

var _column: VBoxContainer = null

func _ready() -> void:
	name = "TradeHoverCard"
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	visible = false
	add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
	_column = VBoxContainer.new()
	_column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_column.add_theme_constant_override("separation", HudTradeVocab.ROWS_SEPARATION)
	add_child(_column)

## Show `lines` beside `row_rect` (global), on the side away from `avoid` (an open panel's rect, or an
## empty one).
func show_for(lines: Array[Control], row_rect: Rect2, avoid: Rect2) -> void:
	for child in _column.get_children():
		_column.remove_child(child)
		child.queue_free()
	for line in lines:
		_ignore_mouse(line)
		_column.add_child(line)
	custom_minimum_size = Vector2(HudTradeVocab.HOVER_MIN_WIDTH, 0.0)
	reset_size()
	visible = true
	size = get_combined_minimum_size()
	var viewport := get_viewport().get_visible_rect()
	var right_x := row_rect.end.x + HudTradeVocab.HOVER_GAP
	var left_x := row_rect.position.x - HudTradeVocab.HOVER_GAP - size.x
	var prefer_left := false
	if avoid.size.x > 0.0 and not avoid.encloses(row_rect):
		prefer_left = avoid.get_center().x > row_rect.get_center().x
	var x := left_x if prefer_left else right_x
	if not prefer_left and x + size.x > viewport.end.x - HudTradeVocab.HOVER_EDGE_MARGIN:
		x = left_x
	if x < viewport.position.x + HudTradeVocab.HOVER_EDGE_MARGIN:
		x = right_x
	var y := clampf(row_rect.position.y, viewport.position.y + HudTradeVocab.HOVER_EDGE_MARGIN,
		maxf(viewport.end.y - size.y - HudTradeVocab.HOVER_EDGE_MARGIN,
			viewport.position.y + HudTradeVocab.HOVER_EDGE_MARGIN))
	position = Vector2(x, y)

func dismiss() -> void:
	visible = false

func _ignore_mouse(node: Node) -> void:
	if node is Control:
		(node as Control).mouse_filter = Control.MOUSE_FILTER_IGNORE
	for child in node.get_children():
		_ignore_mouse(child)
