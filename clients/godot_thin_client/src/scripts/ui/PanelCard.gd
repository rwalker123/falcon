extends PanelContainer
class_name PanelCard

## Reusable dock panel base.
##
## A PanelCard is the single building block for every HUD dock panel. It owns
## the chrome (styled background + header + collapse affordance) and hosts
## caller-supplied content in a plain VBoxContainer. Because it is a
## PanelContainer with container-sized children, it always reports a correct
## minimum size: the owning dock's VBoxContainer reflows automatically when a
## card is shown, hidden, or collapsed. There is no manual height math and no
## anchor-positioned children inside a card — that mixing is what caused the
## legacy panels to paint on top of one another.
##
## Content contract: author a single child VBoxContainer named "CardContent"
## in the scene (or leave the card empty and call `get_content()` at runtime).
## Widgets placed under CardContent keep their identity when the card wraps
## them in its header/body scaffold, so reference them with unique names (`%`).

signal collapsed_changed(collapsed: bool)

const HEADER_COLLAPSED_ARROW := "▸"  # ▸
const HEADER_EXPANDED_ARROW := "▾"   # ▾
const CONTENT_NODE_NAME := "CardContent"

@export var card_title: String = "Panel":
	set(value):
		card_title = value
		_refresh_header()
@export var collapsible: bool = true:
	set(value):
		collapsible = value
		_refresh_header()
@export var start_collapsed: bool = false

var _body: VBoxContainer
var _header: Button
var _content: VBoxContainer
var _collapsed: bool = false
var _built: bool = false

func _ready() -> void:
	_build()
	set_collapsed(start_collapsed)

## Returns the VBoxContainer callers add their widgets to. Safe before _ready.
func get_content() -> VBoxContainer:
	_build()
	return _content

func set_card_title(value: String) -> void:
	card_title = value

func is_collapsed() -> bool:
	return _collapsed

func set_collapsed(value: bool) -> void:
	_build()
	_collapsed = value and collapsible
	if _content != null:
		_content.visible = not _collapsed
	_refresh_header()
	if _collapsed != value:
		return
	collapsed_changed.emit(_collapsed)

func _build() -> void:
	if _built:
		return
	_built = true

	# Adopt an authored CardContent node if present, otherwise create one and
	# absorb any loose children the scene left directly under the card.
	_content = get_node_or_null(CONTENT_NODE_NAME) as VBoxContainer
	var loose_children: Array[Node] = []
	if _content == null:
		_content = VBoxContainer.new()
		_content.name = CONTENT_NODE_NAME
		for child in get_children():
			loose_children.append(child)
	_content.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	_header = Button.new()
	_header.name = "CardHeader"
	_header.flat = true
	_header.focus_mode = Control.FOCUS_NONE
	_header.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_header.pressed.connect(_on_header_pressed)

	_body = VBoxContainer.new()
	_body.name = "CardBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", 6)

	# Reparent adopted/loose content under the generated body.
	if _content.get_parent() == self:
		remove_child(_content)
	for child in loose_children:
		if child.get_parent() != null:
			child.get_parent().remove_child(child)
		_content.add_child(child)

	_body.add_child(_header)
	_body.add_child(_content)
	add_child(_body)

	_refresh_header()

func _refresh_header() -> void:
	if _header == null:
		return
	_header.disabled = not collapsible
	if collapsible:
		var arrow := HEADER_EXPANDED_ARROW if not _collapsed else HEADER_COLLAPSED_ARROW
		_header.text = "%s  %s" % [arrow, card_title]
	else:
		_header.text = card_title

func _on_header_pressed() -> void:
	if not collapsible:
		return
	set_collapsed(not _collapsed)
