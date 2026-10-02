extends Control
class_name ProseOverlay

## The Telling's MODAL PROSE CARD skeleton — the look the narrative fork established, shared by every
## surface that stops the game to SAY something: `NarrativeForkPanel` (a question with choices) and
## `OpeningCardPanel` (the world's opening lines, handing off to the outfitting card).
##
## Structure follows the two patterns this HUD already has:
##   • the targeting banner's centered-overlay skeleton (TargetingController._ensure_targeting_banner), and
##   • the TurnOrb popover's CATCHER NESTING (TurnOrb._open_popover) — the card is a CHILD of the
##     full-screen dismiss layer, never its sibling. A child renders and picks ABOVE its parent, so
##     the card's own buttons consume their clicks and only clicks OUTSIDE the card dismiss. As
##     siblings, the ordering is ambiguous and the catcher swallows the buttons ("clicking the
##     choice did nothing") — a bug that has already been paid for once.
##
## This node IS the catcher; `_card` (an AutoSizingPanel, per the project rule against bespoke height
## logic) is the nested card that grows to fit the prose. A subclass fills `_body` in `_build_body`
## and answers a click outside the card in `_on_dismiss`; everything about geometry lives here.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

# ---- geometry / typography (named constants; no magic literals) ------------
# A dim scrim over the rest of the HUD. The card is modal in intent — it must read as the only thing
# on screen — and the card's own stylebox is translucent, so without this the tile card and the docks
# show THROUGH the narration and make the prose hard to read.
const SCRIM_COLOR := Color(0.0, 0.0, 0.0, 0.55)
const CARD_WIDTH := 660.0
const CARD_MIN_HEIGHT := 220.0
const CARD_MAX_HEIGHT := 720.0
# The card is pinned this far below the top edge, and keeps the same clearance at the bottom —
# which is exactly the `bottom_margin` AutoSizingPanel measures its available height against.
const CARD_TOP_MARGIN := 96.0
# Breathing room below the last row, on top of the card stylebox's own margins — a card whose footer
# sits flush on its border reads rushed.
const CARD_EXTRA_PADDING := 12.0
const BODY_SEPARATION := 16
# The narration is prose at paragraph length, so it is set noticeably larger than UI copy and
# given real leading — cramped 14px body text is what makes a story beat read like a tooltip.
const NARRATION_FONT_SIZE := 19
const NARRATION_LINE_SPACING := 7
const NARRATION_MIN_HEIGHT := 76.0
## The node name the nested card carries — a subclass names its own, so a scene-tree dump says which
## of the prose cards is up.
const DEFAULT_CARD_NAME := "ProseCard"

var _card: AutoSizingPanel = null
var _body: VBoxContainer = null
var _scroll: ScrollContainer = null
var _scrim: ColorRect = null


func _ready() -> void:
	# The catcher: full-screen, STOP, so a click anywhere outside the card dismisses.
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	gui_input.connect(_on_catcher_input)
	resized.connect(_reposition_card)
	visible = false

	_scrim = ColorRect.new()
	_scrim.name = "Scrim"
	_scrim.color = SCRIM_COLOR
	_scrim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_scrim)

	_card = AutoSizingPanel.new()
	_card.name = _card_name()
	_card.target_width = CARD_WIDTH
	_card.min_height = CARD_MIN_HEIGHT
	_card.max_height = CARD_MAX_HEIGHT
	_card.bottom_margin = CARD_TOP_MARGIN
	add_child(_card)

	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	_card.add_child(panel)

	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	panel.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", BODY_SEPARATION)
	_scroll.add_child(_body)

# ---- public API ------------------------------------------------------------

func close() -> void:
	visible = false

func is_open() -> bool:
	return visible

# ---- subclass hooks --------------------------------------------------------

## The nested card's node name.
func _card_name() -> String:
	return DEFAULT_CARD_NAME

## Fill `body` top-to-bottom. Called on every `_render`, into an emptied body.
func _build_body(_body_box: VBoxContainer) -> void:
	pass

## A click landed OUTSIDE the card.
func _on_dismiss() -> void:
	close()

# ---- rendering -------------------------------------------------------------

## Show the overlay and (re)build the card's content.
func _show_and_render() -> void:
	visible = true
	_sync_to_viewport()
	_render()

func _render() -> void:
	if _body == null:
		return
	for child in _body.get_children():
		child.queue_free()
		_body.remove_child(child)
	_build_body(_body)
	# The card grows to fit its prose (which varies a lot in length), so the fit needs a frame for
	# the wrapped narration label to report its real height.
	call_deferred("_fit_card")

## One paragraph of narration — the hero element: large, generous leading, wrapped, never truncated.
func _build_narration(text: String) -> Label:
	var narration := Label.new()
	narration.text = text
	narration.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	narration.custom_minimum_size = Vector2(0, NARRATION_MIN_HEIGHT)
	narration.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	narration.add_theme_font_size_override("font_size", NARRATION_FONT_SIZE)
	narration.add_theme_constant_override("line_spacing", NARRATION_LINE_SPACING)
	narration.add_theme_color_override("font_color", HudStyle.INK)
	return narration

## Grow the card to its content and keep it pinned top-centre. AutoSizingPanel owns the height
## math (and turns the scroll on when the prose outruns the available space).
func _fit_card() -> void:
	if _card == null or _body == null:
		return
	_card.position = Vector2(maxf((_available_width() - CARD_WIDTH) * 0.5, 0.0), CARD_TOP_MARGIN)
	var card_style := HudStyle.card_stylebox()
	var chrome := card_style.content_margin_top + card_style.content_margin_bottom + CARD_EXTRA_PADDING
	_card.fit_to_content(_body.get_combined_minimum_size().y, chrome, _scroll)
	_reposition_card()

## Centre the card horizontally. Measured against the VIEWPORT, not this node's `size`: the catcher
## is anchored full-rect but its size only settles on the next layout pass, so reading `size` in the
## same frame the panel is built centres it against 0 and pins the card to the left edge.
func _available_width() -> float:
	var viewport := get_viewport()
	if viewport != null:
		return viewport.get_visible_rect().size.x
	return size.x

## Pin the catcher (and its scrim) to the viewport EXPLICITLY rather than trusting the full-rect
## anchors: this node is hidden until it has something to say, and a hidden Control's layout does not
## settle — leaving the scrim a zero-size rect that silently never darkens anything.
func _sync_to_viewport() -> void:
	var rect := Rect2(Vector2.ZERO, Vector2(size))
	var viewport := get_viewport()
	if viewport != null:
		rect = viewport.get_visible_rect()
	position = Vector2.ZERO
	size = rect.size
	if _scrim != null:
		_scrim.position = Vector2.ZERO
		_scrim.size = rect.size

func _reposition_card() -> void:
	if _card == null:
		return
	_sync_to_viewport()
	_card.position = Vector2(maxf((_available_width() - _card.size.x) * 0.5, 0.0), CARD_TOP_MARGIN)

# ---- input -----------------------------------------------------------------

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		_on_dismiss()
