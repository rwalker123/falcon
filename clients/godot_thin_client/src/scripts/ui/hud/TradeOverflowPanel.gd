extends AutoSizingPanel
class_name TradeOverflowPanel

## **THE TRADE TAB'S OVERFLOW PANEL** (issue #731) — where every list too long for the zone opens: the
## camps (the network line, or a good's row scoped to that good), a direction's folded shipments, and
## the SHORT tier's two arm rows. **A list NEVER grows the zone**: the zone's height is fixed, so it
## states a count and a way in, and the list opens here.
##
## **THE `WorkInspectorDialog` IDIOM, property for property** (`band-city-panel.md` → "THE WORK
## INSPECTOR IS A DIALOG"): a `Control` on its own `CanvasLayer`, NEVER a `Popup` (which auto-hides on
## an outside click — and a row press elsewhere on the tab is ordinary use, which RE-TARGETS this
## panel); non-modal, with no catcher and no scrim, so every pixel it does not draw stays live map;
## centred in the room the dock leaves, cut back off the card's MAP-FACING side so it never covers the
## card it came from; and carrying its own `ScrollContainer`, which is the one scroll the Trade zone
## itself may not have (`BandComposeFloat`'s exemption: this card reserves nothing, so what it holds
## can never reach the dock's reservation).
##
## **ONE INSTANCE.** Opening another list replaces this one's content rather than stacking a second.
##
## The free-floating case, hence `AutoSizingPanel` rather than `PanelCard` + `DockScrollFit`
## (`panel-framework.md`): it is measured against the viewport, not against a dock's remaining height.

## The viewport clearance and the gap off the card's map-facing edge — `WorkInspectorDialog`'s own
## reads of the same two quantities, and the same numbers for the same reasons.
const VIEWPORT_MARGIN := WorkInspectorDialog.VIEWPORT_MARGIN
const ANCHOR_GAP := WorkInspectorDialog.ANCHOR_GAP
## The column the lists are authored at: the narrow shell's zone width, the width the tab's own rows
## were measured at.
const CONTENT_WIDTH := BandCityPanel.ZONE_PARTY_WIDTH
const HEAD_SEPARATION := 8
const BODY_SEPARATION := 6
## The floor a mounted card may shrink to — a head and one row.
const MIN_CARD_HEIGHT := 60.0

signal closed

var _card: PanelContainer = null
var _column: VBoxContainer = null
var _title: Label = null
var _count: Label = null
var _sub: Label = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
var _anchor: Rect2 = Rect2()
var _edge: int = SIDE_TOP
var _fit_pending: bool = false
## A fit asked for while one was in flight — deferred, never discarded (`WorkInspectorDialog._fit_requested`).
var _fit_requested: bool = false
## Which list is mounted — the caller's own key, so a re-render can re-target the same list.
var _kind: String = ""

func _ready() -> void:
	super()
	name = "TradeOverflowPanel"
	mouse_filter = Control.MOUSE_FILTER_STOP
	bottom_margin = VIEWPORT_MARGIN
	centred_in_room = true
	min_height = MIN_CARD_HEIGHT
	visible = false

	_card = PanelContainer.new()
	_card.name = "TradeOverflowCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", BandCityPanel.panel_card_stylebox())
	add_child(_card)

	_column = VBoxContainer.new()
	_column.add_theme_constant_override("separation", BODY_SEPARATION)
	_card.add_child(_column)

	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", HEAD_SEPARATION)
	_column.add_child(head)
	_title = Label.new()
	_title.add_theme_font_size_override("font_size", HudTradeVocab.TITLE_FONT_SIZE)
	_title.add_theme_color_override("font_color", HudStyle.INK)
	head.add_child(_title)
	_count = Label.new()
	_count.add_theme_font_size_override("font_size", HudTradeVocab.SUB_FONT_SIZE)
	_count.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_count.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_count.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(_count)
	var close := Button.new()
	close.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
	close.tooltip_text = HudTradeVocab.CLOSE_TOOLTIP
	close.focus_mode = Control.FOCUS_NONE
	HudStyle.apply_button(close, "ghost")
	HudWidgets.compact(close, HudTradeVocab.ROW_FONT_SIZE, HudWorkVocab.INSPECTOR_CLOSE_PADDING_V)
	close.pressed.connect(dismiss)
	head.add_child(close)

	_sub = Label.new()
	_sub.add_theme_font_size_override("font_size", HudTradeVocab.SUB_FONT_SIZE)
	_sub.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_sub.visible = false
	_column.add_child(_sub)

	_scroll = ScrollContainer.new()
	_scroll.name = "TradeOverflowScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "TradeOverflowBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", HudTradeVocab.ROWS_SEPARATION)
	_body.minimum_size_changed.connect(refit)
	_scroll.add_child(_body)

# ---- public API -------------------------------------------------------------

## Mount one list: its title, its count, an optional summary line, and its rows. **Re-mounting is the
## re-target** — the panel stays up and its content changes.
func mount(kind: String, title: String, count: String, summary: String, rows: Array[Control],
		card_rect: Rect2, map_facing: int) -> void:
	_kind = kind
	_anchor = card_rect
	_edge = map_facing
	target_width = CONTENT_WIDTH + _card_chrome_width()
	_title.text = title
	_count.text = count
	_sub.text = summary
	_sub.visible = summary != ""
	_clear_body()
	for row in rows:
		_body.add_child(row)
	# VISIBLE BEFORE THE FIT — a hidden subtree never sorts, so it would measure unwrapped forever.
	visible = true
	if not has_fitted_width():
		fit_width(0.0, 0.0)
	_place()
	refit()

func dismiss() -> void:
	var was_open := is_open()
	_clear_body()
	_kind = ""
	visible = false
	if was_open:
		closed.emit()

func is_open() -> bool:
	return visible and _kind != ""

## The mounted list's key, `""` when closed.
func kind() -> String:
	return _kind

func card() -> PanelContainer:
	return _card

func body() -> VBoxContainer:
	return _body

func room() -> Rect2:
	return _room()

## Re-fit to the content and re-centre, across two frames (`WorkInspectorDialog.refit`'s measured
## reasons: an unsorted body reports nonsense, and the width fit moves the wrap the height reads).
func refit() -> void:
	if not visible or _body == null:
		return
	if _fit_pending:
		_fit_requested = true
		return
	_fit_pending = true
	_fit_requested = false
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	var room_rect := _room()
	max_width = maxf(room_rect.size.x, target_width)
	fit_width(_column.get_combined_minimum_size().x, _card_chrome_width())
	await get_tree().process_frame
	if not visible or _body == null:
		return
	room_rect = _room()
	max_height = room_rect.size.y
	var head_height := _column.get_combined_minimum_size().y - _scroll.get_combined_minimum_size().y
	fit_to_content(head_height + _body.get_combined_minimum_size().y, _card_chrome().y, _scroll)
	_place()
	if _fit_requested:
		_fit_requested = false
		refit()

# ---- geometry ---------------------------------------------------------------

func _place() -> void:
	var room_rect := _room()
	position = Vector2(
		room_rect.position.x + maxf((room_rect.size.x - size.x) * 0.5, 0.0),
		room_rect.position.y + maxf((room_rect.size.y - size.y) * 0.5, 0.0))

## The viewport inside `VIEWPORT_MARGIN`, cut back to the MAP-FACING side of the panel card with
## `ANCHOR_GAP` of clearance — the one rect the fits and the placement read, so *"it never covers the
## card"* is structural. An unset anchor leaves the whole viewport.
func _room() -> Rect2:
	var room_rect := available_room(VIEWPORT_MARGIN)
	if _anchor.size.x <= 0.0 or _anchor.size.y <= 0.0:
		return room_rect
	match _edge:
		SIDE_RIGHT:
			return _cut_to(room_rect, _anchor.end.x + ANCHOR_GAP, room_rect.end.x, true)
		SIDE_LEFT:
			return _cut_to(room_rect, room_rect.position.x, _anchor.position.x - ANCHOR_GAP, true)
		SIDE_BOTTOM:
			return _cut_to(room_rect, _anchor.end.y + ANCHOR_GAP, room_rect.end.y, false)
		_:
			return _cut_to(room_rect, room_rect.position.y, _anchor.position.y - ANCHOR_GAP, false)

func _cut_to(room_rect: Rect2, lo: float, hi: float, horizontal: bool) -> Rect2:
	var low: float = maxf(lo, room_rect.position.x if horizontal else room_rect.position.y)
	var high: float = minf(hi, room_rect.end.x if horizontal else room_rect.end.y)
	var span: float = maxf(high - low, 0.0)
	if horizontal:
		return Rect2(Vector2(low, room_rect.position.y), Vector2(span, room_rect.size.y))
	return Rect2(Vector2(room_rect.position.x, low), Vector2(room_rect.size.x, span))

func _card_chrome() -> Vector2:
	return BandCityPanel.panel_card_stylebox().get_minimum_size()

## The card's chrome plus the scrollbar's gutter, reserved unconditionally so the width does not jump
## when a taller or shorter window turns the scroll on or off.
func _card_chrome_width() -> float:
	var gutter := 0.0
	if _scroll != null:
		gutter = _scroll.get_v_scroll_bar().get_combined_minimum_size().x
	return _card_chrome().x + gutter

func _clear_body() -> void:
	if _body == null:
		return
	for child in _body.get_children():
		_body.remove_child(child)
		child.queue_free()
