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

var _header: RichTextLabel
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
	if _header != null:
		_header.add_theme_color_override("default_color", _title_color)

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

	# RichTextLabel header so the title can carry a colored "kind" eyebrow while
	# still behaving like a single-line label inside the content VBox.
	_header = RichTextLabel.new()
	_header.name = "CardHeader"
	_header.bbcode_enabled = true
	_header.fit_content = true
	_header.scroll_active = false
	_header.autowrap_mode = TextServer.AUTOWRAP_OFF
	_header.add_theme_font_size_override("normal_font_size", 14)
	_header.add_theme_color_override("default_color", _title_color)
	_header.add_theme_stylebox_override("normal", HudStyle.header_stylebox())
	_content.add_child(_header)
	_content.move_child(_header, 0)
	_refresh_header()

func _refresh_header() -> void:
	if _header == null:
		return
	var title := card_title
	if not hotkey_hint.is_empty():
		title = "%s (%s)" % [card_title, hotkey_hint]
	if card_kind.is_empty():
		_header.text = title
	else:
		_header.text = "[color=#%s][font_size=11]%s[/font_size][/color]  %s" % [
			HudStyle.SIGNAL_HEX, card_kind.to_upper(), title,
		]
