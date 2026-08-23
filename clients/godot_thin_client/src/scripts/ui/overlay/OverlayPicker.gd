extends Control
class_name OverlayPicker

## THE MAP-OVERLAY PICKER — the `◐` button docked on the minimap's top border and the popover it
## opens: the channel list, the selected channel's description and its legend.
##
## **IT KNOWS NO CHANNEL BY NAME.** The list is `OverlayChannels.roster(view)`, the legend is
## `OverlayLegend.render(...)` against whichever kind the descriptor declares, and the selection is
## pushed straight through `MapView.set_overlay_channel`. Adding a channel touches neither this file
## nor the renderer — see `OverlayChannels`, which is the only place a channel is declared.
##
## **WHY IT LIVES ON THE MINIMAP AND NOT IN THE INSPECTOR** (`docs/plan_knowledge_screen.md` §6a).
## The overlay selector was `Inspector → Map → OverlaySection`, and the Inspector is a modding tool
## that ships hidden behind `I` — so the map's own channel picker was somewhere a player would never
## look. The button is docked ON the border rather than floated beside the panel, so it costs the nav
## cluster no width and cannot be mistaken for a zoom control.
##
## **THE POPOVER IS A `Control`, NOT A `PopupPanel`, and that is deliberate.** The `PopupPanel`s in
## this HUD are Windows, which is what makes them unable to change a docked zone's height — a problem
## this widget does not have, since it floats over the map. What it would cost is the render
## harnesses: a Window renders to its own surface, so a popover opened in `map_preview` would be
## absent from the captured frame and the state could not be judged at all. So it follows `TurnOrb`'s
## shape instead — a full-screen catcher with the popover nested INSIDE it, so the popover's own
## buttons consume their clicks and only a click in the catcher area OUTSIDE it dismisses. It differs
## from `TurnOrb`'s in one way, for the reason `POPOVER_CANVAS_LAYER` gives: the catcher lives on a
## `CanvasLayer` of its own, so it needs no `top_level` to escape the picker's small rect.
##
## **THE PICKER OWNS THE CHANNEL ACROSS A SNAPSHOT AND NOWHERE ELSE, and getting that backwards
## silently erases every overlay in the client.** Two rules, one per signal:
##
## - **`overlay_channels_ingested` — RE-ASSERT.** `MapView._ingest_overlay_channels` clears
##   `active_overlay_key` on every frame it ingests, so a channel the player chose would be painted
##   for exactly one turn and then revert to bare terrain. The Inspector panel this replaces did the
##   re-push from its own ingest; nothing else will now.
## - **`overlay_legend_changed` — ADOPT.** That signal ALSO fires on every ordinary channel change,
##   and a picker that re-asserted on it would overwrite whatever any other caller had just set —
##   `MapView.set_terrain_mode`, `set_fow_enabled`'s deliberate clear, and every offline harness that
##   drives a channel with no picker in the loop. Measured: re-asserting on the legend signal
##   rendered SEVEN `map_preview` overlay states as bare terrain, each a perfectly plausible frame.
##   So outside an ingest `MapView` is the authority for what is painted, and the picker follows it.
##
## `_syncing` is what keeps the re-assert from recursing — its own `set_overlay_channel` emits the
## legend signal, and the adopt branch must not run on the picker's own echo.

const BUTTON_SIZE := 22.0
const BUTTON_GLYPH := "◐"
const BUTTON_FONT_SIZE := 13
const BUTTON_TOOLTIP := "Map overlay"

## **THE POPOVER GETS ITS OWN `CanvasLayer`, ABOVE EVERY DOCKED SURFACE.** The picker is mounted on
## the minimap, which in the shipped client is EMBEDDED in the HUD's bottom bar — so its popover
## inherited the HUD's layer (`Main`: `hud.layer = 101`) and the Band/City dock
## (`BandCityPanel.LAYER_INDEX` 103), the Workbench (`Main.WORKBENCH_LAYER` 103) and the event dock
## (`EventDockPanel.LAYER_INDEX` 104) all drew straight over it. Reported from play as "the menu shows
## up under the band panel", and invisible to `map_preview`, which stands up no HUD and therefore has
## none of those layers — which is why the assertion guarding this compares against those files' own
## constants rather than looking at a picture.
##
## It stays BELOW `Main.LOADING_OVERLAY_LAYER` (150): a world being built must cover everything.
const POPOVER_CANVAS_LAYER := 105

const POPOVER_GAP := 8.0
const POPOVER_PADDING := 10
const POPOVER_MARGIN_SIDES: Array[String] = ["left", "top", "right", "bottom"]
## How far the popover is kept from the viewport edge once it has been clamped inside it.
const POPOVER_SCREEN_INSET := 8.0
const POPOVER_TITLE := "MAP OVERLAY"
const POPOVER_TITLE_FONT_SIZE := 11
const POPOVER_SECTION_SEPARATION := 8
const LIST_SEPARATION := 1
const ROW_FONT_SIZE := 12

var _map_view: Node = null
var _button: Button = null
var _popover_layer: CanvasLayer = null
var _catcher: Control = null
var _popover: PanelContainer = null
var _list: VBoxContainer = null
var _legend_body: VBoxContainer = null

## The channel the PLAYER chose. `MapView.active_overlay_key` is what is painted, and between an
## ingest and the re-apply below the two differ — which is the whole reason this is held here.
var _selected_key: String = OverlayChannels.NO_OVERLAY_KEY
var _roster: Array[Dictionary] = []
var _legend: Dictionary = {}
## True while the picker's own `set_overlay_channel` is running, so the legend signal that call emits
## does not run the ADOPT branch against the picker's own echo.
var _syncing: bool = false

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	custom_minimum_size = Vector2(BUTTON_SIZE, BUTTON_SIZE)
	_button = Button.new()
	_button.name = "OverlayPickerButton"
	_button.text = BUTTON_GLYPH
	_button.tooltip_text = BUTTON_TOOLTIP
	_button.focus_mode = Control.FOCUS_NONE
	_button.mouse_filter = Control.MOUSE_FILTER_STOP
	_button.set_anchors_preset(Control.PRESET_FULL_RECT)
	_button.add_theme_font_size_override("font_size", BUTTON_FONT_SIZE)
	HudStyle.apply_button(_button, "ghost")
	_button.pressed.connect(toggle_popover)
	add_child(_button)

## The map the picker drives. Also its data source: the channel roster and the legend both come off
## `MapView`, so nothing has to route the `overlays` payload here.
func set_map_view(view: Node) -> void:
	if _map_view == view:
		return
	if _map_view != null and is_instance_valid(_map_view):
		if _map_view.is_connected("overlay_legend_changed", _on_overlay_legend_changed):
			_map_view.disconnect("overlay_legend_changed", _on_overlay_legend_changed)
		if _map_view.is_connected("overlay_channels_ingested", _on_overlay_channels_ingested):
			_map_view.disconnect("overlay_channels_ingested", _on_overlay_channels_ingested)
	_map_view = view
	if _map_view == null:
		_roster = []
		_legend = {}
		return
	if _map_view.has_signal("overlay_legend_changed"):
		_map_view.connect("overlay_legend_changed", _on_overlay_legend_changed)
	if _map_view.has_signal("overlay_channels_ingested"):
		_map_view.connect("overlay_channels_ingested", _on_overlay_channels_ingested)
	if _map_view.has_method("current_overlay_legend"):
		_legend = _map_view.call("current_overlay_legend")
	# ADOPT on attach, never re-assert: the MapView may already be painting a channel (a harness
	# drives one before the minimap exists), and the picker has no player choice to defend yet.
	_adopt_painted_key()
	_rebuild_roster()
	_render_popover()

func is_popover_open() -> bool:
	return _popover != null and is_instance_valid(_popover)

## The channel the player has chosen — what the picker will re-assert after the next snapshot.
func selected_key() -> String:
	return _selected_key

## The merged roster the popover is listing, for a caller that wants to assert over it.
func roster() -> Array[Dictionary]:
	return _roster.duplicate(true)

func toggle_popover() -> void:
	if is_popover_open():
		close_popover()
	else:
		open_popover()

func open_popover() -> void:
	close_popover()
	if _map_view != null and _map_view.has_method("refresh_overlay_legend"):
		# The legend is pushed on change; a popover opening long after the last one needs the pull.
		_map_view.call("refresh_overlay_legend")
	_popover_layer = CanvasLayer.new()
	_popover_layer.name = "OverlayPickerLayer"
	_popover_layer.layer = POPOVER_CANVAS_LAYER
	add_child(_popover_layer)

	# A direct child of a `CanvasLayer` already sits in that layer's own space, so this needs no
	# `top_level` — which is the one thing that differs from `TurnOrb`'s otherwise identical catcher.
	_catcher = Control.new()
	_catcher.name = "OverlayPickerCatcher"
	_catcher.mouse_filter = Control.MOUSE_FILTER_STOP
	_catcher.position = Vector2.ZERO
	_catcher.size = get_viewport_rect().size
	_catcher.gui_input.connect(_on_catcher_input)
	_popover_layer.add_child(_catcher)

	_popover = _build_popover()
	_popover.resized.connect(_position_popover)
	_catcher.add_child(_popover)
	_render_popover()
	_position_popover()

func close_popover() -> void:
	# Freeing the layer frees the catcher and the popover nested under it.
	if _popover_layer != null and is_instance_valid(_popover_layer):
		_popover_layer.queue_free()
	_popover_layer = null
	_catcher = null
	_popover = null
	_list = null
	_legend_body = null

## The `CanvasLayer` index the open popover is drawing on, for a caller asserting it clears the
## docked surfaces. `-1` when the popover is closed.
func popover_layer_index() -> int:
	if _popover_layer == null or not is_instance_valid(_popover_layer):
		return -1
	return _popover_layer.layer

## Where the open popover landed, for a caller asserting it cleared a docked edge. An empty `Rect2`
## when the popover is closed.
func popover_rect() -> Rect2:
	if not is_popover_open():
		return Rect2()
	return _popover.get_global_rect()

## Choose a channel, exactly as clicking its row does. The one entry point: the row handler, the
## re-apply after a snapshot and any caller driving the picker all land here.
func select_channel(key: String) -> void:
	_selected_key = key
	_apply_to_map()
	_render_popover()

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		close_popover()

## A snapshot landed: the roster may have changed and `active_overlay_key` has just been cleared, so
## this is the one moment the picker RE-ASSERTS rather than follows. See the header's two rules.
func _on_overlay_channels_ingested() -> void:
	_rebuild_roster()
	_apply_to_map()
	_render_popover()

## A channel changed — ours or somebody else's. ADOPT, never re-assert; the header says why.
func _on_overlay_legend_changed(legend: Dictionary) -> void:
	_legend = legend.duplicate(true) if legend is Dictionary else {}
	if not _syncing:
		_adopt_painted_key()
	_render_popover()

## Follow `MapView`'s own `active_overlay_key`. A key outside the roster is still adopted — the
## channel really is painted, and a picker that showed a different row would be lying about the map.
func _adopt_painted_key() -> void:
	if _map_view == null:
		return
	var painted: Variant = _map_view.get(&"active_overlay_key")
	if painted is String:
		_selected_key = String(painted)

func _rebuild_roster() -> void:
	_roster = OverlayChannels.roster(_map_view)
	if not _has_key(_selected_key):
		_selected_key = _roster[0]["key"] if not _roster.is_empty() else OverlayChannels.NO_OVERLAY_KEY

func _has_key(key: String) -> bool:
	for descriptor in _roster:
		if String(descriptor.get("key", "")) == key:
			return true
	return false

func _descriptor_for_selection() -> Dictionary:
	for descriptor in _roster:
		if String(descriptor.get("key", "")) == _selected_key:
			return descriptor
	return {}

func _apply_to_map() -> void:
	if _map_view == null or not _map_view.has_method("set_overlay_channel"):
		return
	_syncing = true
	_map_view.call("set_overlay_channel", _selected_key)
	_syncing = false
	# `set_overlay_channel` SILENTLY REFUSES a key it holds no raster for, so read the result back
	# rather than assume the push took — otherwise the lit row claims a channel the map is not
	# painting, and the next snapshot re-asserts the same rejected key forever.
	_adopt_painted_key()
	if _map_view.has_method("current_overlay_legend"):
		_legend = _map_view.call("current_overlay_legend")

func _build_popover() -> PanelContainer:
	var panel := PanelContainer.new()
	panel.name = "OverlayPickerPopover"
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	var margin := MarginContainer.new()
	for side in POPOVER_MARGIN_SIDES:
		margin.add_theme_constant_override("margin_%s" % side, POPOVER_PADDING)
	panel.add_child(margin)

	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", POPOVER_SECTION_SEPARATION)
	margin.add_child(body)

	var title := Label.new()
	title.text = POPOVER_TITLE
	title.add_theme_font_size_override("font_size", POPOVER_TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK_DIM)
	body.add_child(title)

	_list = VBoxContainer.new()
	_list.name = "OverlayChannelList"
	_list.add_theme_constant_override("separation", LIST_SEPARATION)
	body.add_child(_list)

	_legend_body = VBoxContainer.new()
	_legend_body.name = "OverlayChannelLegend"
	body.add_child(_legend_body)
	return panel

func _render_popover() -> void:
	if not is_popover_open():
		return
	_render_list()
	var descriptor := _descriptor_for_selection()
	OverlayLegend.render(
		_legend_body,
		descriptor,
		_legend,
		OverlayChannels.facts_for(_map_view, descriptor))

func _render_list() -> void:
	for child in _list.get_children():
		child.queue_free()
	for descriptor in _roster:
		_list.add_child(_channel_row(descriptor))

func _channel_row(descriptor: Dictionary) -> Button:
	var key := String(descriptor.get("key", ""))
	var label := String(descriptor.get("label", key))
	if bool(descriptor.get("placeholder", false)):
		label = "%s (%s)" % [label, OverlayLegend.STUB_MARKER]
	var row := Button.new()
	row.text = label
	row.alignment = HORIZONTAL_ALIGNMENT_LEFT
	row.focus_mode = Control.FOCUS_NONE
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_font_size_override("font_size", ROW_FONT_SIZE)
	var description := String(descriptor.get("description", "")).strip_edges()
	if description != "":
		row.tooltip_text = description
	HudStyle.apply_pill_toggle(row, key == _selected_key)
	row.add_theme_color_override("font_color",
		HudStyle.INK if key == _selected_key else HudStyle.INK_DIM)
	row.pressed.connect(select_channel.bind(key))
	return row

## Above the button, right-aligned to it, then clamped into the PLAY AREA — the minimap sits at the
## bottom of the screen in both of its mounts (docked in the nav cluster, or floating bottom-right),
## so downward is off-screen and the un-clamped left edge runs under whatever is docked to the left.
##
## **THE BOUND IS THE UNRESERVED RECT, NOT THE VIEWPORT.** A left-docked Band/City panel is ~495px
## wide and the popover is ~310, so right-aligning it to a button in the nav cluster puts its left
## edge inside the dock — and drawing it *above* the dock instead of under it would only trade an
## unreadable popover for one covering the panel the player is reading. `MapView` already sums the
## docked edges for its own cover-fit, so the play area is a question it can answer; the viewport is
## the fallback for a mount with no MapView attached yet.
func _position_popover() -> void:
	if not is_popover_open() or _button == null:
		return
	var anchor := _button.get_global_rect()
	var size_now := _popover.size
	var play := _play_area()
	var pos := Vector2(anchor.end.x - size_now.x, anchor.position.y - size_now.y - POPOVER_GAP)
	var min_x: float = play.position.x + POPOVER_SCREEN_INSET
	var min_y: float = play.position.y + POPOVER_SCREEN_INSET
	pos.x = clampf(pos.x, min_x, maxf(min_x, play.end.x - size_now.x - POPOVER_SCREEN_INSET))
	pos.y = clampf(pos.y, min_y, maxf(min_y, play.end.y - size_now.y - POPOVER_SCREEN_INSET))
	_popover.global_position = pos

## The rect the popover may occupy: the map's unreserved area when a MapView is attached and offers
## one, else the whole viewport.
func _play_area() -> Rect2:
	if _map_view != null and _map_view.has_method("unreserved_screen_rect"):
		var rect: Variant = _map_view.call("unreserved_screen_rect")
		if rect is Rect2 and (rect as Rect2).has_area():
			return rect
	return Rect2(Vector2.ZERO, get_viewport_rect().size)
