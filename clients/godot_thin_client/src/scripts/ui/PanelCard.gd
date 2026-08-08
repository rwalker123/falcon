extends PanelContainer
class_name PanelCard

## Reusable dock panel base.
##
## A PanelCard is the single building block for every HUD dock panel. It owns
## the chrome (styled background + title header) and hosts caller-supplied
## content in a plain VBoxContainer. Because it is a PanelContainer with
## container-sized children, it always reports a correct minimum size: the
## owning dock's VBoxContainer reflows automatically when a card is shown or
## hidden. There is no manual height math and no anchor-positioned children
## inside a card — that mixing is what caused the legacy panels to paint on top
## of one another.
##
## Content contract: author a single child VBoxContainer named "CardContent" in
## the scene (or leave the card empty and call `get_content()` at runtime). The
## card inserts its title header as the first row of that container and never
## reparents the authored widgets, so `unique_name_in_owner` (`%`) references to
## them keep resolving from the scene owner.

const CONTENT_NODE_NAME := "CardContent"

## Header type scale — the title at the card's own weight, the kind eyebrow smaller beside it,
## the two sizes the header carried when it was a single bbcode line.
const TITLE_FONT_SIZE := 14
const KIND_FONT_SIZE := 11
## The gap between the kind eyebrow and the title, replacing the two literal spaces the bbcode used.
const HEADER_GAP := 8

@export var card_title: String = "Panel":
	set(value):
		card_title = value
		_refresh_header()
## Optional toggle key shown in the header, e.g. "L" renders "Terrain Types (L)".
## Only set it on panels that actually have a show/hide hotkey.
@export var hotkey_hint: String = "":
	set(value):
		hotkey_hint = value
		_refresh_header()
## Optional cyan eyebrow rendered before the title, e.g. "Tile" -> "TILE (5, 3)".
## Used by the selection panel to label what kind of thing is selected.
@export var card_kind: String = "":
	set(value):
		card_kind = value
		_refresh_header()

## The header is a PanelContainer (it carries the hairline stylebox) around a row of two Labels:
## the optional kind eyebrow and the title. See `_build` for why it is not one RichTextLabel.
var _header: PanelContainer
var _header_row: HBoxContainer
var _kind_label: Label
var _title_label: Label
var _content: VBoxContainer
var _built: bool = false
## Header ink. Defaults to the shared INK; `set_title_color` re-tints it for a card whose title
## itself carries meaning (the Telling panel's title ages with the narrator's medium).
var _title_color: Color = HudStyle.INK

func _ready() -> void:
	_build()

## Returns the VBoxContainer callers add their widgets to. Safe before _ready.
func get_content() -> VBoxContainer:
	_build()
	return _content

func set_card_title(value: String) -> void:
	card_title = value

func set_card_kind(value: String) -> void:
	card_kind = value

## Tint the header ink. For cards where the TITLE is itself a signal rather than just a name —
## today only the Telling panel, whose title and accent age together with the narrator's medium.
## Most cards should leave this alone and stay on the shared INK.
func set_title_color(color: Color) -> void:
	_title_color = color
	_build()
	if _title_label != null:
		_title_label.add_theme_color_override("font_color", _title_color)

func _build() -> void:
	if _built:
		return
	_built = true

	# Card chrome: dark translucent surface, hairline border, rounded corners.
	add_theme_stylebox_override("panel", HudStyle.card_stylebox())

	# Adopt the authored CardContent node if present, otherwise create an empty
	# one as the card's sole child. Either way the content container stays put —
	# we never reparent authored widgets.
	_content = get_node_or_null(CONTENT_NODE_NAME) as VBoxContainer
	if _content == null:
		_content = VBoxContainer.new()
		_content.name = CONTENT_NODE_NAME
		add_child(_content)
	_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# THE TITLE MUST NEVER DICTATE THE CARD'S WIDTH, which is why the header is a row of two
	# Labels and not the one bbcode RichTextLabel it used to be. A RichTextLabel's `fit_content`
	# reports its full unwrapped content size as a MINIMUM on both axes with no per-axis switch,
	# so with AUTOWRAP_OFF a long title was a hard minimum on its card and widened the entire dock
	# column: a 58-character legend title measured the right column at 489 against the 352 that
	# `Hud.RIGHT_COLUMN_CEILING` promises is an upper bound — and a column wider than that bound
	# gets drawn through by the Band/City card. A Label can trim, a RichTextLabel cannot (it has
	# neither `clip_text` nor `text_overrun_behavior`), so the title is a Label that reports a
	# ~zero width minimum and ellipsises when the card is narrower than its text.
	_header = PanelContainer.new()
	_header.name = "CardHeader"
	_header.add_theme_stylebox_override("panel", HudStyle.header_stylebox())

	_header_row = HBoxContainer.new()
	_header_row.name = "CardHeaderRow"
	_header_row.add_theme_constant_override("separation", HEADER_GAP)
	_header.add_child(_header_row)

	# The kind eyebrow keeps its natural width, deliberately: it is a one-word authored vocabulary
	# ("TILE"), and an HBox pays every child its minimum before the expanding one gets anything —
	# so trimming the eyebrow too would collapse BOTH halves to slivers on a narrow card instead of
	# spending the shortfall on the one string that can be arbitrarily long.
	_kind_label = Label.new()
	_kind_label.name = "CardKind"
	_kind_label.add_theme_font_size_override("font_size", KIND_FONT_SIZE)
	_kind_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
	_kind_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_kind_label.visible = false
	_header_row.add_child(_kind_label)

	_title_label = Label.new()
	_title_label.name = "CardTitle"
	_title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_title_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	# Both halves of the bound, and they are not the same property doing the same job: the overrun
	# behaviour is what draws the ellipsis, `clip_text` is what stops the label reporting — and
	# drawing — its full text width. Either one alone shrinks the reported minimum in Godot 4.7,
	# but only the pair both bounds the card and shows the player that the title was trimmed.
	_title_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_title_label.clip_text = true
	# A Label ignores the mouse by default and a tooltip is only found on a control the mouse hits.
	# PASS rather than STOP so the card underneath still sees the event.
	_title_label.mouse_filter = Control.MOUSE_FILTER_PASS
	_title_label.add_theme_font_size_override("font_size", TITLE_FONT_SIZE)
	_title_label.add_theme_color_override("font_color", _title_color)
	_title_label.resized.connect(_update_title_tooltip)
	_header_row.add_child(_title_label)

	_content.add_child(_header)
	_content.move_child(_header, 0)
	_refresh_header()

func _refresh_header() -> void:
	if _title_label == null:
		return
	var title := card_title
	if not hotkey_hint.is_empty():
		title = "%s (%s)" % [card_title, hotkey_hint]
	_title_label.text = title
	_kind_label.text = card_kind.to_upper()
	_kind_label.visible = not card_kind.is_empty()
	_update_title_tooltip()

## A trimmed title stays reachable: when — and only when — the ellipsis is engaged, the header
## carries the untrimmed string as its tooltip. Godot exposes no "is this label trimmed" flag, so
## the test is the shaped text width against the width the label was actually laid out at; an
## untrimmed title clears the tooltip so a card never explains a line the player can already read.
func _update_title_tooltip() -> void:
	if _title_label == null:
		return
	var font := _title_label.get_theme_font("font")
	if font == null:
		return
	var natural := font.get_string_size(_title_label.text, HORIZONTAL_ALIGNMENT_LEFT, -1,
		_title_label.get_theme_font_size("font_size")).x
	_title_label.tooltip_text = _title_label.text if natural > _title_label.size.x else ""
