extends Control
class_name ComposeSheet

## The COMPOSE SHEET — the write state of the selection card (docs/plan_tile_panel_layout.md §10-§15).
##
## WHY IT EXISTS. Part 1 bounded the selection card by capping its drawer; it did not make the
## drawer SMALL. The two compose blocks (`%ForageAssignControls` / `%HerdAssignControls`) are ~270px
## of always-expanded picker living permanently in a column that also has to show the land, the
## roster and the detail rows. Composing is MODAL BY NATURE — you open it, decide, commit, done — so
## the read state keeps the column and the write state borrows space only while in use.
##
## THREE STRUCTURAL RULES, each of which this repo has already got wrong once:
##
## 1. THE CARD IS AN `AutoSizingPanel`, NOT A `DockScrollFit` CARD. The root CLAUDE.md rule: a
##    FREE-FLOATING panel measured against the VIEWPORT is `AutoSizingPanel.fit_to_content`;
##    `DockScrollFit` is for a card inside a dock's `VBoxContainer`, whose size the container
##    overwrites every layout pass and whose ceiling is the DOCK's remaining height. This sheet
##    floats, so it is the AutoSizingPanel case — the opposite of what Part 1's drawer needed.
##    Picking the wrong one misbehaves SILENTLY rather than failing.
##
## 2. THE CATCHER IS NESTED, NOT SIBLINGED. This node IS the full-screen dismiss catcher
##    (`MOUSE_FILTER_STOP`), with the card as its CHILD — reusing `NarrativeForkPanel` exactly. As
##    siblings the ordering is ambiguous and the catcher swallows the card's own clicks. And the
##    catcher is pinned to the viewport EXPLICITLY (`_sync_to_viewport`): the node is hidden until a
##    compose opens, and a hidden Control's full-rect anchors do not settle, leaving a zero-size rect.
##
## 3. NO SCRIM. `NarrativeForkPanel` dims because a fork is a story beat demanding attention. An
##    assignment is a working action composed AGAINST the map — the band's work-range ring, the
##    herd's position and the hunt reach are all live context you are reading while you dial the
##    party. This is the one place the sheet deliberately departs from the fork panel.
##
## The sheet knows nothing about foraging or hunting: the caller opens it with a title and fills
## `content()` with whatever it likes (see `Hud._open_forage_compose` / `_open_herd_compose`). The
## builders are handed that container as an explicit target — the compose blocks are NEVER
## reparented into it, because reparenting silently clears `unique_name_in_owner` and breaks every
## `%Name` lookup in the owner script (`PanelCard`'s contract note).

## Emitted whenever an open sheet closes, for any reason (✕, catcher click, Esc, or the caller's
## own `close()` on commit / selection change). The caller drops its compose state here so the two
## can never disagree about whether a sheet is open.
signal closed

const CARD_WIDTH := 340.0
const CARD_MIN_HEIGHT := 120.0
const CARD_MAX_HEIGHT := 560.0
## Clearance kept between the card and the viewport edges (top/bottom margin, and the gap between
## the anchor card and this sheet).
const VIEWPORT_MARGIN := 16.0
const ANCHOR_GAP := 12.0
## Extra room beyond the measured content height, so the last control never sits on the border.
const CARD_EXTRA_PADDING := 12.0
const BODY_SEPARATION := 6
const HEADER_SEPARATION := 8
const CLOSE_GLYPH := "✕"
const CLOSE_TOOLTIP := "Close (Esc)"
## Matches `PanelCard`'s header: an 11pt cyan eyebrow before the 14pt subject title.
const TITLE_FONT_SIZE := 14
const EYEBROW_FONT_SIZE := 11
const CLOSE_BUTTON_WIDTH := 26.0
const HEADER_FORMAT := "[color=#%s][font_size=%d]%s[/font_size][/color]  %s"

var _card: AutoSizingPanel = null
var _header: RichTextLabel = null
var _body: VBoxContainer = null
var _scroll: ScrollContainer = null
## The global rect of the card that summoned this sheet; the sheet opens BESIDE it (see `_place_card`).
var _anchor_rect: Rect2 = Rect2()
## Identifies the subject being composed, so the owner can tell "the same source, refreshed" from
## "a different source" on a per-snapshot re-render. Opaque to this node.
var _subject_key: String = ""
var _fit_pending: bool = false

func _ready() -> void:
	# The catcher: full-screen, STOP, so a click anywhere outside the card dismisses. No scrim —
	# see rule 3 above. Deliberately NOT `PRESET_FULL_RECT`: the preset's non-equal opposite anchors
	# make the engine overwrite any size we set (it warns as much), and since the node is hidden
	# until a compose opens its layout never settles anyway — `_sync_to_viewport` is the pin, so the
	# anchors stay top-left and the explicit size is authoritative.
	mouse_filter = Control.MOUSE_FILTER_STOP
	gui_input.connect(_on_catcher_input)
	resized.connect(_place_card)
	visible = false

	_card = AutoSizingPanel.new()
	_card.name = "ComposeCard"
	_card.target_width = CARD_WIDTH
	_card.min_height = CARD_MIN_HEIGHT
	_card.max_height = CARD_MAX_HEIGHT
	_card.bottom_margin = VIEWPORT_MARGIN
	add_child(_card)

	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	_card.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", HEADER_SEPARATION)
	panel.add_child(column)

	column.add_child(_build_header_row())

	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "ComposeBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", BODY_SEPARATION)
	_scroll.add_child(_body)
	# The caller rebuilds the body on every stepper tick / policy click; refit from the content
	# itself rather than asking every builder to remember to call us.
	_body.minimum_size_changed.connect(refit)

func _build_header_row() -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HEADER_SEPARATION)
	_header = RichTextLabel.new()
	_header.bbcode_enabled = true
	_header.fit_content = true
	_header.scroll_active = false
	_header.autowrap_mode = TextServer.AUTOWRAP_OFF
	_header.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_header.add_theme_font_size_override("normal_font_size", TITLE_FONT_SIZE)
	_header.add_theme_color_override("default_color", HudStyle.INK)
	row.add_child(_header)
	var close_btn := Button.new()
	close_btn.text = CLOSE_GLYPH
	close_btn.tooltip_text = CLOSE_TOOLTIP
	close_btn.custom_minimum_size = Vector2(CLOSE_BUTTON_WIDTH, 0)
	HudStyle.apply_button(close_btn, "ghost")
	close_btn.pressed.connect(close)
	row.add_child(close_btn)
	return row

# ---- public API ------------------------------------------------------------

## Open (or re-title) the sheet for `subject_key`, headed `<EYEBROW>  <title>`, floating beside
## `anchor` (the global rect of the card that summoned it). Returns the container the caller fills;
## the caller owns the content and is expected to clear + rebuild it.
func open(eyebrow: String, title: String, subject_key: String, anchor: Rect2) -> VBoxContainer:
	_subject_key = subject_key
	_anchor_rect = anchor
	set_header(eyebrow, title)
	visible = true
	_sync_to_viewport()
	refit()
	return _body

## The container the caller's builders render into. Never reparent authored `%Name` nodes here —
## pass this as an explicit build target instead (see the class docs).
func content() -> VBoxContainer:
	return _body

func set_header(eyebrow: String, title: String) -> void:
	if _header == null:
		return
	_header.text = HEADER_FORMAT % [HudStyle.SIGNAL_HEX, EYEBROW_FONT_SIZE, eyebrow.to_upper(), title]

func subject_key() -> String:
	return _subject_key

func is_open() -> bool:
	return visible

func close() -> void:
	if not visible:
		return
	visible = false
	_subject_key = ""
	for child in _body.get_children():
		child.queue_free()
	emit_signal("closed")

## Re-fit the card to its content and re-place it. Coalesced across one frame: the content height is
## a function of the card's width, so a measurement taken in the same frame the body was rebuilt
## reports the PREVIOUS content's wrapping (the same reason `Hud._fit_subject_drawer` waits a frame).
func refit() -> void:
	if not visible or _fit_pending or _card == null or _body == null:
		return
	_fit_pending = true
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _card == null or _body == null:
		return
	var card_style := HudStyle.card_stylebox()
	var chrome := card_style.content_margin_top + card_style.content_margin_bottom \
		+ _header.get_combined_minimum_size().y + HEADER_SEPARATION + CARD_EXTRA_PADDING
	_sync_to_viewport()
	_place_card()
	_card.fit_to_content(_body.get_combined_minimum_size().y, chrome, _scroll)
	_place_card()

# ---- geometry --------------------------------------------------------------

## Pin the catcher to the viewport EXPLICITLY rather than trusting the full-rect anchor preset: the
## node is hidden until a compose opens, and a hidden Control's layout does not settle — leaving a
## zero-size rect the card would then be positioned inside of (`NarrativeForkPanel._sync_to_viewport`).
func _sync_to_viewport() -> void:
	var rect := Rect2(Vector2.ZERO, Vector2(size))
	var viewport := get_viewport()
	if viewport != null:
		rect = viewport.get_visible_rect()
	position = Vector2.ZERO
	size = rect.size

## Float the card BESIDE the card that summoned it — the selection panel stays readable (it holds
## the subject list and the standing summary the sheet is editing), and the map stays uncovered
## where it can. Falls back to hugging the viewport's left margin when the anchor would push the
## sheet off-screen.
func _place_card() -> void:
	if _card == null:
		return
	var bounds := Vector2(size)
	var viewport := get_viewport()
	if viewport != null:
		bounds = viewport.get_visible_rect().size
	var x := _anchor_rect.end.x + ANCHOR_GAP
	if _anchor_rect.size == Vector2.ZERO or x + CARD_WIDTH > bounds.x - VIEWPORT_MARGIN:
		x = VIEWPORT_MARGIN
	var y := _anchor_rect.position.y if _anchor_rect.size != Vector2.ZERO else VIEWPORT_MARGIN
	y = clampf(y, VIEWPORT_MARGIN, maxf(bounds.y - _card.size.y - VIEWPORT_MARGIN, VIEWPORT_MARGIN))
	_card.position = Vector2(clampf(x, VIEWPORT_MARGIN, maxf(bounds.x - CARD_WIDTH - VIEWPORT_MARGIN, VIEWPORT_MARGIN)), y)

# ---- input -----------------------------------------------------------------

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		close()
