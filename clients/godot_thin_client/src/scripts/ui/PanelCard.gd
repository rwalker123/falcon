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

var _header: Label
var _content: VBoxContainer
var _built: bool = false

func _ready() -> void:
	_build()

## Returns the VBoxContainer callers add their widgets to. Safe before _ready.
func get_content() -> VBoxContainer:
	_build()
	return _content

func set_card_title(value: String) -> void:
	card_title = value

func _build() -> void:
	if _built:
		return
	_built = true

	# Adopt the authored CardContent node if present, otherwise create an empty
	# one as the card's sole child. Either way the content container stays put —
	# we never reparent authored widgets.
	_content = get_node_or_null(CONTENT_NODE_NAME) as VBoxContainer
	if _content == null:
		_content = VBoxContainer.new()
		_content.name = CONTENT_NODE_NAME
		add_child(_content)
	_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	_header = Label.new()
	_header.name = "CardHeader"
	_header.add_theme_font_size_override("font_size", 16)
	_header.add_theme_color_override("font_color", Color(0.9, 0.95, 1.0, 1.0))
	_content.add_child(_header)
	_content.move_child(_header, 0)
	_refresh_header()

func _refresh_header() -> void:
	if _header == null:
		return
	if hotkey_hint.is_empty():
		_header.text = card_title
	else:
		_header.text = "%s (%s)" % [card_title, hotkey_hint]
