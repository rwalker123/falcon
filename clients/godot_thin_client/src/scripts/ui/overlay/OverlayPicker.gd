extends HBoxContainer
class_name OverlayPicker

## THE MAP-OVERLAY PICKER — the two buttons docked on the minimap's top border, and the popover each
## one opens.
##
## | button | opens | face |
## |---|---|---|
## | `◐` | the CHANNEL MENU — the roster, one row each | fixed |
## | legend | the LEGEND for whatever channel is on | the channel's `icon`, else its ramp COLOUR |
##
## **ONE RULE FOR THE WHOLE CLUSTER: a button opens its own popover, attached to itself.** The first
## cut had one button and put the legend inside its menu, then tried making that legend a standing
## panel above the minimap — which pushed the menu away from the button that opened it by however
## tall the legend happened to be. A menu that does not touch its own button is the tell. Splitting
## them is also what makes the menu a FIXED height: the roster only changes when the world does,
## while a legend is three rows for a scalar ramp and twenty for the biome key.
##
## **THE LEGEND BUTTON'S FACE IS THE AMBIENT READOUT.** It says which channel is painted without
## costing a panel — and it is never in a dead state, because the bare map has a legend too (the
## biome key; see `OverlayChannels.CHANNELS`). That is where the right dock's retired `L` card went.
##
## **IT KNOWS NO CHANNEL BY NAME.** The list is `OverlayChannels.roster(view)`, the legend is
## `OverlayLegend.render(...)` against whichever kind the descriptor declares, and the selection is
## pushed straight through `MapView.set_overlay_channel`. Adding a channel touches neither this file
## nor the renderer — see `OverlayChannels`, the only place a channel is declared.
##
## **WHY IT LIVES ON THE MINIMAP AND NOT IN THE INSPECTOR** (`docs/plan_knowledge_screen.md` §6a).
## The overlay selector was `Inspector → Map → OverlaySection`, and the Inspector is a modding tool
## that ships hidden behind `I` — so the map's own channel picker was somewhere a player would never
## look. The buttons dock ON the border rather than floating beside the panel, so they cost the nav
## cluster no width and cannot be mistaken for zoom controls.
##
## **A POPOVER IS A `Control`, NOT A `PopupPanel`, and that is deliberate.** The `PopupPanel`s in this
## HUD are Windows, which is what makes them unable to change a docked zone's height — a problem a
## widget floating over the map does not have. What a Window WOULD cost is the render harnesses: it
## draws to its own surface, so a popover opened in `map_preview` would be absent from the captured
## frame and the state could not be judged at all. So it follows `TurnOrb`'s shape — a full-screen
## catcher with the popover nested INSIDE it, so the popover's own buttons consume their clicks and
## only a click in the catcher area OUTSIDE it dismisses. It differs from `TurnOrb`'s in one way, for
## the reason `POPOVER_CANVAS_LAYER` gives: the catcher lives on a `CanvasLayer` of its own, so it
## needs no `top_level` to escape the picker's small rect.
##
## **ONE POPOVER AT A TIME.** Opening either closes the other. Two small cards stacked over a corner
## of the map would fight for the same space and for the same dismiss click.
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
const BUTTON_GAP := 4
const CHANNEL_GLYPH := "◐"
const BUTTON_FONT_SIZE := 13
const CHANNEL_TOOLTIP := "Map overlay"
const LEGEND_TOOLTIP_FORMAT := "Legend — %s"
## The whole bar's width, which is what `MinimapPanel` reserves on the border for it.
const BAR_WIDTH := 2.0 * BUTTON_SIZE + float(BUTTON_GAP)

## The legend button's face when the channel names no `icon`: a filled square, TINTED with that
## channel's own map colour. A glyph rather than a `ColorRect` child for two reasons — it makes the
## two buttons siblings by construction (both are one-glyph text buttons, so they come out the same
## width under the same stylebox padding, which an empty button does not), and it makes the eventual
## `icon` a straight swap of the same property rather than a second face mechanism. U+25A0, the same
## Geometric Shapes block as `◐` and as `WorkbenchPages`' covered `▣` / `◧`.
const LEGEND_SWATCH_GLYPH := "■"

## **THE POPOVERS GET THEIR OWN `CanvasLayer`, ABOVE EVERY DOCKED SURFACE.** The picker is mounted on
## the minimap, which in the shipped client is EMBEDDED in the HUD's bottom bar — so its popover
## inherited `Main.HUD_LAYER` (101) and the Band/City dock (`BandCityPanel.LAYER_INDEX` 103), the
## Workbench (`Main.WORKBENCH_LAYER` 103) and the event dock (`EventDockPanel.LAYER_INDEX` 104) all
## drew straight over it. Reported from play as "the menu shows up under the band panel", and
## invisible to `map_preview`, which stands up no HUD and therefore has none of those layers — which
## is why the assertion guarding this compares against those files' own constants rather than looking
## at a picture.
##
## It stays BELOW `Main.LOADING_OVERLAY_LAYER` (150): a world being built must cover everything.
const POPOVER_CANVAS_LAYER := 105

const POPOVER_GAP := 8.0
const POPOVER_PADDING := 10
const POPOVER_MARGIN_SIDES: Array[String] = ["left", "top", "right", "bottom"]
## How far a popover is kept from the play area's edge once it has been clamped inside it.
const POPOVER_SCREEN_INSET := 8.0
const CHANNELS_TITLE := "MAP OVERLAY"
const LEGEND_TITLE := "LEGEND"
const POPOVER_TITLE_FONT_SIZE := 11
const POPOVER_SECTION_SEPARATION := 8
const LIST_SEPARATION := 1
const ROW_FONT_SIZE := 12

## How tall the legend body may grow before it scrolls. The biome key runs to every biome present on
## the map — twenty-odd rows on an earthlike — and a popover taller than the map it explains is not a
## legend. Sized to sit comfortably above the minimap in the nav cluster.
const LEGEND_MAX_HEIGHT := 320.0

## Daylight between the legend's rows and the scrollbar that shares their right edge. A
## `ScrollContainer`'s vertical bar is drawn OVER the content rather than beside it, so without a
## reserved gutter the value column runs under the bar — reported from play with `2059 tiles` touching
## it. Measured off the real bar rather than guessed, so a theme change moves the gutter with it.
const LEGEND_SCROLL_GAP := 6.0

## Which popover is open, for a caller that needs to tell them apart.
const POPOVER_NONE := &""
const POPOVER_CHANNELS := &"channels"
const POPOVER_LEGEND := &"legend"

## Every button text-colour state the swatch face paints, so it reads as one colour in every state.
const LEGEND_FACE_COLOR_STATES: Array[StringName] = [
	&"font_color", &"font_hover_color", &"font_pressed_color", &"font_focus_color",
]

var _map_view: Node = null
var _channel_button: Button = null
var _legend_button: Button = null

var _popover_layer: CanvasLayer = null
var _catcher: Control = null
var _popover: PanelContainer = null
var _popover_kind: StringName = POPOVER_NONE
## The button the open popover hangs off — what `_position_popover` anchors to.
var _anchor_button: Button = null
var _list: VBoxContainer = null
var _legend_body: VBoxContainer = null
var _legend_scroll: ScrollContainer = null

## The channel the PLAYER chose. `MapView.active_overlay_key` is what is painted, and between an
## ingest and the re-assert the two differ — which is the whole reason this is held here.
var _selected_key: String = OverlayChannels.NO_OVERLAY_KEY
var _roster: Array[Dictionary] = []
var _legend: Dictionary = {}
## True while the picker's own `set_overlay_channel` is running, so the legend signal that call emits
## does not run the ADOPT branch against the picker's own echo.
var _syncing: bool = false

func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_theme_constant_override("separation", BUTTON_GAP)
	custom_minimum_size = Vector2(BAR_WIDTH, BUTTON_SIZE)

	_channel_button = _build_button("OverlayChannelButton", CHANNEL_TOOLTIP)
	_channel_button.text = CHANNEL_GLYPH
	_channel_button.pressed.connect(toggle_channels)
	add_child(_channel_button)

	_legend_button = _build_button("OverlayLegendButton", LEGEND_TOOLTIP_FORMAT % "")
	_legend_button.pressed.connect(toggle_legend)
	add_child(_legend_button)
	_paint_legend_face()

func _build_button(node_name: String, tooltip: String) -> Button:
	var button := Button.new()
	button.name = node_name
	button.tooltip_text = tooltip
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_filter = Control.MOUSE_FILTER_STOP
	button.custom_minimum_size = Vector2(BUTTON_SIZE, BUTTON_SIZE)
	button.add_theme_font_size_override("font_size", BUTTON_FONT_SIZE)
	HudStyle.apply_button(button, "ghost")
	return button

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
		_paint_legend_face()
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
	_refresh_surfaces()

# ---- the popovers ---------------------------------------------------------------------------

func is_popover_open() -> bool:
	return _popover != null and is_instance_valid(_popover)

## Which popover is open — `POPOVER_CHANNELS`, `POPOVER_LEGEND`, or `POPOVER_NONE`.
func open_popover_kind() -> StringName:
	return _popover_kind if is_popover_open() else POPOVER_NONE

func toggle_channels() -> void:
	if open_popover_kind() == POPOVER_CHANNELS:
		close_popover()
	else:
		open_channels()

func toggle_legend() -> void:
	if open_popover_kind() == POPOVER_LEGEND:
		close_popover()
	else:
		open_legend()

func open_channels() -> void:
	_open(POPOVER_CHANNELS, _channel_button)

func open_legend() -> void:
	_open(POPOVER_LEGEND, _legend_button)

func close_popover() -> void:
	# Freeing the layer frees the catcher and the popover nested under it.
	if _popover_layer != null and is_instance_valid(_popover_layer):
		_popover_layer.queue_free()
	_popover_layer = null
	_catcher = null
	_popover = null
	_popover_kind = POPOVER_NONE
	_anchor_button = null
	_list = null
	_legend_body = null
	_legend_scroll = null

## The `CanvasLayer` index the open popover is drawing on, for a caller asserting it clears the
## docked surfaces. `-1` when nothing is open.
func popover_layer_index() -> int:
	if _popover_layer == null or not is_instance_valid(_popover_layer):
		return -1
	return _popover_layer.layer

## Where the open popover landed, for a caller asserting it cleared a docked edge or kept its height.
## An empty `Rect2` when nothing is open.
func popover_rect() -> Rect2:
	if not is_popover_open():
		return Rect2()
	return _popover.get_global_rect()

## The button the open popover hangs off, for a caller asserting the two touch.
func anchor_rect() -> Rect2:
	if _anchor_button == null or not is_instance_valid(_anchor_button):
		return Rect2()
	return _anchor_button.get_global_rect()

## The legend button's own rect, so a caller can prove which button a popover is hanging off — and so
## a harness can press it where a player would.
func legend_button_rect() -> Rect2:
	if _legend_button == null or not is_instance_valid(_legend_button):
		return Rect2()
	return _legend_button.get_global_rect()

## The channel button's own rect, the `◐`'s twin of the above.
func channel_button_rect() -> Rect2:
	if _channel_button == null or not is_instance_valid(_channel_button):
		return Rect2()
	return _channel_button.get_global_rect()

## The colour the legend button's face is currently drawn in, for a caller asserting it tracks the
## painted channel.
func legend_face_color() -> Color:
	if _legend_button == null:
		return Color()
	return _legend_button.get_theme_color(&"font_color")

## The channel the player has chosen — what the picker will re-assert after the next snapshot.
func selected_key() -> String:
	return _selected_key

## The merged roster the menu is listing, for a caller that wants to assert over it.
func roster() -> Array[Dictionary]:
	return _roster.duplicate(true)

## Choose a channel, exactly as clicking its row does. The one entry point: the row handler, the
## re-assert after a snapshot and any caller driving the picker all land here.
func select_channel(key: String) -> void:
	_selected_key = key
	_apply_to_map()
	_refresh_surfaces()

func _open(kind: StringName, anchor: Button) -> void:
	close_popover()
	if _map_view != null and _map_view.has_method("refresh_overlay_legend"):
		# The legend is pushed on change; a popover opening long after the last one needs the pull.
		_map_view.call("refresh_overlay_legend")
	_popover_kind = kind
	_anchor_button = anchor

	_popover_layer = CanvasLayer.new()
	_popover_layer.name = "OverlayPickerLayer"
	_popover_layer.layer = POPOVER_CANVAS_LAYER
	add_child(_popover_layer)

	# A direct child of a `CanvasLayer` already sits in that layer's own space, so this needs no
	# `top_level` — the one thing that differs from `TurnOrb`'s otherwise identical catcher.
	_catcher = Control.new()
	_catcher.name = "OverlayPickerCatcher"
	_catcher.mouse_filter = Control.MOUSE_FILTER_STOP
	_catcher.position = Vector2.ZERO
	_catcher.size = get_viewport_rect().size
	_catcher.gui_input.connect(_on_catcher_input)
	_popover_layer.add_child(_catcher)

	_popover = _build_popover(kind)
	_popover.resized.connect(_position_popover)
	_catcher.add_child(_popover)
	_render_popover()
	_position_popover()

## **THE CATCHER MUST NOT SWALLOW A PRESS AIMED AT THE PICKER'S OWN BUTTONS.** It is a full-screen
## `STOP` on a layer ABOVE the bar, so with either popover open the OTHER button never receives its
## click — pressing it read as "dismiss" instead of "switch", and the player had to click twice to get
## to the other card. The two buttons are resolved here instead, in the same terms their own `pressed`
## handlers use: the open one's button toggles it shut, the other one's swaps to it. Everything else
## dismisses, which is what the catcher is for.
func _on_catcher_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton and event.pressed):
		return
	var at: Vector2 = _catcher.global_position + (event as InputEventMouseButton).position
	if _channel_button != null and _channel_button.get_global_rect().has_point(at):
		toggle_channels()
		return
	if _legend_button != null and _legend_button.get_global_rect().has_point(at):
		toggle_legend()
		return
	close_popover()

# ---- channel state --------------------------------------------------------------------------

## A snapshot landed: the roster may have changed and `active_overlay_key` has just been cleared, so
## this is the one moment the picker RE-ASSERTS rather than follows. See the header's two rules.
func _on_overlay_channels_ingested() -> void:
	_rebuild_roster()
	_apply_to_map()
	_refresh_surfaces()

## A channel changed — ours or somebody else's. ADOPT, never re-assert; the header says why.
func _on_overlay_legend_changed(legend: Dictionary) -> void:
	_legend = legend.duplicate(true) if legend is Dictionary else {}
	if not _syncing:
		_adopt_painted_key()
	_refresh_surfaces()

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

# ---- rendering ------------------------------------------------------------------------------

## Restate everything the current channel decides: the legend button's face, and whichever popover is
## open. The face is repainted on EVERY channel change, open popover or not — it is the standing
## readout, so it must be current when nothing is open at all.
func _refresh_surfaces() -> void:
	_paint_legend_face()
	_render_popover()

## The legend button's face: the channel's `icon` when the registry names one, else the colour the
## channel paints the map in. See `OverlayChannels` for why it is a table with a live fallback.
func _paint_legend_face() -> void:
	if _legend_button == null:
		return
	var descriptor := _descriptor_for_selection()
	var label := String(descriptor.get("label", ""))
	_legend_button.tooltip_text = LEGEND_TOOLTIP_FORMAT % label if label != "" else LEGEND_TITLE
	var icon := String(descriptor.get("icon", ""))
	_legend_button.text = icon if icon != "" else LEGEND_SWATCH_GLYPH
	var face: Color = HudStyle.INK
	if icon == "" and _map_view != null and _map_view.has_method("overlay_color_for"):
		face = _map_view.call("overlay_color_for", _selected_key)
	# All four live states, not just `font_color`: a swatch is a READOUT, so it must not change hue
	# under the pointer the way a label would. The stylebox still gives the button its hover.
	for state in LEGEND_FACE_COLOR_STATES:
		_legend_button.add_theme_color_override(state, face)

func _build_popover(kind: StringName) -> PanelContainer:
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
	title.text = CHANNELS_TITLE if kind == POPOVER_CHANNELS else LEGEND_TITLE
	title.add_theme_font_size_override("font_size", POPOVER_TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK_DIM)
	body.add_child(title)

	if kind == POPOVER_CHANNELS:
		_list = VBoxContainer.new()
		_list.name = "OverlayChannelList"
		_list.add_theme_constant_override("separation", LIST_SEPARATION)
		body.add_child(_list)
	else:
		# The biome key runs to every biome on the map, so the legend body scrolls past a cap rather
		# than growing a popover taller than the thing it explains.
		_legend_scroll = ScrollContainer.new()
		_legend_scroll.name = "OverlayLegendScroll"
		_legend_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
		body.add_child(_legend_scroll)
		# The rows sit inside a margin, not directly in the scroll: the vertical bar is drawn OVER the
		# content, so the gutter has to come out of the rows' own width. See `LEGEND_SCROLL_GAP`.
		var gutter := MarginContainer.new()
		gutter.name = "OverlayLegendGutter"
		gutter.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		gutter.add_theme_constant_override("margin_right", int(_legend_scroll_gutter()))
		_legend_scroll.add_child(gutter)
		_legend_body = VBoxContainer.new()
		_legend_body.name = "OverlayChannelLegend"
		_legend_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		gutter.add_child(_legend_body)
	return panel

func _render_popover() -> void:
	if not is_popover_open():
		return
	if _popover_kind == POPOVER_CHANNELS:
		_render_list()
	else:
		var descriptor := _descriptor_for_selection()
		OverlayLegend.render(
			_legend_body,
			descriptor,
			_legend,
			OverlayChannels.facts_for(_map_view, descriptor))
		# The WIDTH comes from the content, not from `MIN_WIDTH`: horizontal scrolling is disabled, so
		# a scroll box narrower than its rows CLIPS them — which ate the last digit of every value in
		# the first cut. Only the height is capped.
		var wanted: Vector2 = _legend_body.get_combined_minimum_size()
		_legend_scroll.custom_minimum_size = Vector2(
			maxf(OverlayLegend.MIN_WIDTH, wanted.x + _legend_scroll_gutter()),
			minf(wanted.y, LEGEND_MAX_HEIGHT))
	# The popover is a `Control` under a plain `Control`, so nothing lays it out and a size it once
	# grew into is a size it keeps (see `_render_list`). Ask it back down to its own minimum after
	# every rebuild, which is what `reset_size` is for.
	_popover.reset_size()

## How much room the scrollbar needs on the legend's right edge: the bar's own minimum width plus a
## gap. The bar exists whether or not it is shown, so this answers the same number for a legend that
## scrolls and one that does not — which is what keeps the popover from changing width per channel.
func _legend_scroll_gutter() -> float:
	if _legend_scroll == null:
		return LEGEND_SCROLL_GAP
	var bar := _legend_scroll.get_v_scroll_bar()
	if bar == null:
		return LEGEND_SCROLL_GAP
	return bar.get_combined_minimum_size().x + LEGEND_SCROLL_GAP

func _render_list() -> void:
	# **`remove_child` BEFORE `queue_free`.** Same trap as `OverlayLegend.render`, and this is the one
	# that shipped: `queue_free` deletes at the END of the frame, so picking a row rebuilt the list
	# with the OUTGOING rows still counted, the popover's minimum height jumped for that frame, and a
	# `Control` that grows to satisfy a minimum WRITES THE GROWN SIZE BACK INTO ITS OFFSETS — so it
	# never shrank again. Reported as "every time a selection is made, the menu shows full height".
	for child in _list.get_children():
		_list.remove_child(child)
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

## Above its own button, right-aligned to it, then clamped into the PLAY AREA — the minimap sits at
## the bottom of the screen in both of its mounts (docked in the nav cluster, or floating
## bottom-right), so downward is off-screen and the un-clamped left edge runs under whatever is
## docked to the left.
##
## **THE BOUND IS THE UNRESERVED RECT, NOT THE VIEWPORT.** A left-docked Band/City panel is ~495px
## wide and a popover is ~290, so right-aligning it to a button in the nav cluster puts its far edge
## inside the dock — and drawing it *above* the dock instead of under it would only trade an
## unreadable popover for one covering the panel the player is reading. `MapView` already sums the
## docked edges for its own cover-fit, so the play area is a question it can answer; the viewport is
## the fallback for a mount with no MapView attached yet.
func _position_popover() -> void:
	if not is_popover_open() or _anchor_button == null:
		return
	var anchor := _anchor_button.get_global_rect()
	var size_now := _popover.size
	var play := _play_area()
	var pos := Vector2(anchor.end.x - size_now.x, anchor.position.y - size_now.y - POPOVER_GAP)
	var min_x: float = play.position.x + POPOVER_SCREEN_INSET
	var min_y: float = play.position.y + POPOVER_SCREEN_INSET
	pos.x = clampf(pos.x, min_x, maxf(min_x, play.end.x - size_now.x - POPOVER_SCREEN_INSET))
	pos.y = clampf(pos.y, min_y, maxf(min_y, play.end.y - size_now.y - POPOVER_SCREEN_INSET))
	_popover.global_position = pos

## The rect a popover may occupy: the map's unreserved area when a MapView is attached and offers
## one, else the whole viewport.
func _play_area() -> Rect2:
	if _map_view != null and _map_view.has_method("unreserved_screen_rect"):
		var rect: Variant = _map_view.call("unreserved_screen_rect")
		if rect is Rect2 and (rect as Rect2).has_area():
			return rect
	return Rect2(Vector2.ZERO, get_viewport_rect().size)
