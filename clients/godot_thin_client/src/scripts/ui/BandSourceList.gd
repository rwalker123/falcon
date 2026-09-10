class_name BandSourceList
extends Control

## **THE SELECTED BAND'S SOURCES, AS A LIST DOCKED BESIDE IT ON THE MAP** (issue #650).
##
## ⛔ **IT EXISTS BECAUSE RATES CANNOT LIVE OVER THEIR OWN MARKERS, AND THAT IS ARITHMETIC.**
## Selecting a band put ~20 floating elements over four hexes; a rate pill is ~90px wide, two markers
## in one hex's edge slots sit ~55px apart, and a hex holds three slots. A collision/lift pass shipped
## and was reverted — Ray, on the live frame: *"having 1 way up there is worse then letting them
## overlapp a bit"* — because a pill moved off its marker has lost the whole of what tied the rate to
## its source. So the rates left the map and became rows, each joined to its hex by a LEADER LINE,
## which is the association a pill's position used to carry and the one thing a list still needs.
##
## **SELECTION ADDS; NOTHING IS DIMMED OR REMOVED.** Everything the selected view showed before is
## exactly where it was — the range borders, the rings, the badges, the pending overlay. This is one
## more surface, and it is the only one that leaves the map plane.
##
## **THE MODEL IS `BandOverlayRenderer.compute_source_rows`, AND THIS PANEL DERIVES NOTHING.** Every
## figure on a row — the `⚒N`, the rate, the build countdown, the attention state — is produced once,
## by the pass that already resolved it for the marker, and read here. A row's icon is the MARKER's
## own face through `SecondaryMarkerRenderer.face_for_*`. The leader line makes any disagreement
## visible in a single glance, which is why there is no second producer anywhere in this file.
##
## ⛔ **THE GEOMETRY IS ARITHMETIC, IN A PUBLIC FUNCTION — NEVER READ BACK OFF CONTROL LAYOUT.**
## Control layout is deferred a frame, and the leader lines need the row anchors in the SAME frame or
## they lag visibly under a pan. `place()` measures and `row_anchor()` reads that measurement, the
## `_name_pill_rects` rule from `map-markers.md`: one function measures, two consumers read it.
## **This is also why the panel is not an `AutoSizingPanel`** — that helper fits a card to its
## CONTENT, measured after a layout pass, and this panel's height is a pure function of the row count
## known before one.
##
## **THE WHOLE WIDGET TREE IS BUILT ONCE, IN `setup`.** Rows are `PAGE_SIZE` fixed slots whose text
## and visibility are re-set each frame; nothing is added or removed while the map is drawing.
##
## Modelled on `MinimapPanel.setup(parent, layer_index)` — a `CanvasLayer` of its own, made and owned
## by `MapView`.

## Emitted when a row is clicked — the map PANS to that hex.
signal tile_focus_requested(x: int, y: int)
## Emitted by the footer's `Work tab ▸` link. `MapView` re-emits it with the band's entity.
signal work_tab_requested()
## Emitted when the pager moves. **`MapView` has to REDRAW on it**: the leader lines are drawn by the
## map, from `page_rows()`, so a page turned without a redraw leaves every line still pointing at the
## previous page's hexes. A button press touches no map state and would otherwise queue nothing.
signal page_changed()

## **A MAP ANNOTATION, SO IT SITS ABOVE THE MAP AND BELOW EVERY DOCKED HUD SURFACE.** The map is a
## `Node2D` at layer 0; `Main.HUD_LAYER` is 101, `MinimapPanel.MINIMAP_CANVAS_LAYER` 102 and
## `OverlayPicker.POPOVER_CANVAS_LAYER` 105. 100 is the last index under all of them: this list
## belongs to the map's own plane and must never cover a panel the player docked.
const LAYER_INDEX := 100

# ---- GEOMETRY (see the ⛔ above: every one of these is spent arithmetically) ---------------------
## Fixed width. A list whose width tracked its content would move its own leader lines every turn a
## rate gained a digit.
const PANEL_WIDTH := 300.0
const ROW_HEIGHT := 22.0
## Rows per page. Ten is what fits beside a band without the panel becoming the screen; past that the
## pager carries the rest.
const PAGE_SIZE := 10
const FOOTER_HEIGHT := 22.0
## ⛔ **THE TOTAL GETS A LINE OF ITS OWN, AND THE ARITHMETIC IS WHY.** It states EVERY account the
## band earns — summing them would be the retired trade axis under a new name — so it is structurally
## the widest string this panel ever draws, and it grows with the band rather than with the page. On
## the six-source fixture it is already `+0.95 /turn · +0.40 fodder · +0.30 wood`, ~250px beside a
## 36px pager, and in the shipped client the `Work tab ▸` link takes another 74px off the same strip:
## rendered on one line it elided to `+…`, losing the third account outright. A number the player
## cannot read is not the number whose INVARIANCE across pages is this footer's whole justification.
## A second 22px line is cheaper than the ~80px of extra width that would fix it sideways, and the
## panel covers map either way.
const FOOTER_TOTAL_HEIGHT := 18.0
## Interior padding, all four sides.
const PAD := 8.0
## The divider under the attention block: 3px of gap plus a 1px line, drawn only on a page holding
## BOTH an attention row and an ordinary one.
const ATTENTION_RULE_H := 5.0
const ATTENTION_RULE_LINE_H := 1.0
## From the band token to the panel's near corner.
const BAND_GAP := 28.0
## Of the unreserved rect, per axis. Inside this band around the centre the panel keeps the side it
## already holds — **this is what stops the list flipping side to side while the player pans a few
## pixels.**
const DEAD_ZONE_FRACTION := 0.12

# ---- ROW COLUMNS ---------------------------------------------------------------------------------
## `icon · ⚒N · rate · build/attention`, left to right. The four widths and the gap between them sum
## to `PANEL_WIDTH`, which is what makes the detail column's width arithmetic rather than a guess.
const COL_GAP := 6.0
const ICON_WIDTH := 18.0
const CREW_WIDTH := 30.0
const RATE_WIDTH := 96.0
const ROW_FONT_SIZE := 12
const FOOTER_FONT_SIZE := 11
## The emoji fallback face is drawn a little larger than the row's text so it reads as a mark rather
## than as a character in the sentence.
const ICON_FONT_SIZE := 14

# ---- FOOTER --------------------------------------------------------------------------------------
const PAGE_BUTTON_WIDTH := 18.0
const PREV_PAGE_GLYPH := "‹"
const NEXT_PAGE_GLYPH := "›"
const FOOTER_SEPARATOR := " · "
const FOOTER_PAGE_FORMAT := "%d/%d"
## Singular is spelled out rather than pluralised by a rule: one row is the common case on an early
## band, and `1 sources` is the kind of thing a player reads as a bug in the count.
const FOOTER_COUNT_ONE := "1 source"
const FOOTER_COUNT_FORMAT := "%d sources"
const WORK_TAB_LABEL := "Work tab ▸"
const WORK_TAB_WIDTH := 74.0
## Between the row's two elided cells on its hover. The footer's own separator is a different width
## because it joins whole clauses; this joins two cells of one row.
const TOOLTIP_SEPARATOR := "  ·  "

## The page a fresh subject opens on.
const FIRST_PAGE := 0

var canvas_layer: CanvasLayer = null

var _background: Panel = null
var _rule: ColorRect = null
var _rows: Array[Button] = []
var _row_icons: Array[TextureRect] = []
var _row_glyphs: Array[Label] = []
var _row_crews: Array[Label] = []
var _row_rates: Array[Label] = []
var _row_details: Array[Label] = []
var _prev_button: Button = null
var _next_button: Button = null
var _footer_label: Label = null
var _total_label: Label = null
var _work_tab_button: Button = null

## The whole model, every page of it.
var _source_rows: Array = []
var _page := FIRST_PAGE
## Which band these rows belong to — a change resets the page and the placement sides, so a newly
## selected band picks fresh rather than inheriting the last one's quadrant.
var _band_entity := -1
## The placement sides, held ACROSS frames: `true` = open right / open below. See `DEAD_ZONE_FRACTION`.
var _side_x := true
var _side_y := true
## How many rows this page holds, and whether it holds both classes — measured by `place`, read by
## `row_anchor`, never re-derived.
var _rows_on_page := 0
var _page_has_rule := false
## The slot index of the first ORDINARY row on this page, `-1` when the page holds none. Cached by
## `_render_page` because the row layout and the rule both spend it per slot.
var _first_ordinary_on_page := -1
## The band's whole income, as `MapView` last composed it. Held because the PAGER re-renders the
## footer and the total is not the pager's to recompute — it is the same string on every page, which
## is the entire reason it is worth reading (see `_render_footer`).
var _total_text := ""

## Build the CanvasLayer, the panel and every row slot. Once — see the class note.
func setup(parent: Node, layer_index: int = LAYER_INDEX) -> void:
	canvas_layer = CanvasLayer.new()
	canvas_layer.layer = layer_index
	canvas_layer.name = "BandSourceListLayer"
	parent.add_child(canvas_layer)
	canvas_layer.add_child(self)
	name = "BandSourceList"
	# A click on the list must not fall through to the map: `MapView` reads clicks in
	# `_unhandled_input`, so a Control that CONSUMES the press is the whole of the guard.
	mouse_filter = Control.MOUSE_FILTER_STOP
	size = Vector2(PANEL_WIDTH, ROW_HEIGHT)
	visible = false

	_background = Panel.new()
	# The card's own chrome with its content margins stripped: this panel positions its children
	# arithmetically, so a stylebox that also inset them would double the padding `PAD` already owns.
	# `card_stylebox()` returns a fresh box per call, so zeroing it here touches nothing else.
	var box: StyleBoxFlat = HudStyle.card_stylebox()
	box.content_margin_left = 0
	box.content_margin_right = 0
	box.content_margin_top = 0
	box.content_margin_bottom = 0
	_background.add_theme_stylebox_override("panel", box)
	_background.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_background)

	_rule = ColorRect.new()
	_rule.color = HudStyle.LINE
	_rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_rule.visible = false
	add_child(_rule)

	for i in range(PAGE_SIZE):
		_build_row(i)
	_build_footer()

## One row slot: a flat `Button` carrying the hover highlight, the pointing cursor and the tooltip,
## with four IGNORE-filtered labels laid over it. The button draws no text of its own — the columns
## have to be positioned, and a Button's own label cannot be.
func _build_row(index: int) -> void:
	var row := Button.new()
	HudStyle.apply_link_button(row, HudStyle.INK)
	row.mouse_filter = Control.MOUSE_FILTER_STOP
	row.pressed.connect(_on_row_pressed.bind(index))
	add_child(row)
	_rows.append(row)

	var icon := TextureRect.new()
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	_row_icons.append(icon)

	_row_glyphs.append(_row_label(row, ICON_FONT_SIZE, HudStyle.INK,
		HORIZONTAL_ALIGNMENT_CENTER))
	_row_crews.append(_row_label(row, ROW_FONT_SIZE, HudStyle.INK_DIM,
		HORIZONTAL_ALIGNMENT_LEFT))
	# **THE RATE COLUMN ELIDES TOO, AND FOR A REASON THE RETIRED PILL DID NOT HAVE.** That plate sized
	# itself to its MEASURED run, so a rate could not overflow it however many materials it stated; a
	# row's cell is a FIXED `RATE_WIDTH` column with the detail cell hard against it. The material arm
	# states EVERY material by design (`_yield_label_rate_text`'s ⛔), and a two-material take like
	# `+0.22 hide · +0.10 sinew` measures ~138px at `ROW_FONT_SIZE` — half again the column — so
	# without this it draws straight through its neighbour.
	#
	# ⛔ **THE STRING IS NEVER SHORTENED, ONLY CLIPPED.** Stating fewer materials than the source pays
	# is the one thing this cell may not do; the whole run stays on the row's tooltip, so an elided
	# figure is still reachable.
	_row_rates.append(_elided(_row_label(row, ROW_FONT_SIZE, HudStyle.HEALTHY,
		HORIZONTAL_ALIGNMENT_LEFT)))
	# **THE DETAIL COLUMN ELIDES AND KEEPS THE WHOLE STRING ON ITS TOOLTIP** — a build sentinel or a
	# hazard clause can outrun 122px, and nothing on this surface may become unreachable.
	_row_details.append(_elided(_row_label(row, ROW_FONT_SIZE, HudStyle.SIGNAL_DEEP,
		HORIZONTAL_ALIGNMENT_LEFT)))

## Trim a cell that outruns its fixed column to an ellipsis. One spelling for the two cells that can
## overflow, so a third column added later cannot pick a different overrun behaviour by accident.
func _elided(label: Label) -> Label:
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.clip_text = true
	return label

## A row's text cell. Font sizes are set with `add_theme_font_size_override` — `Typography.gd` is a
## no-op shim and styling through it fails silently.
func _row_label(parent: Control, font_size: int, color: Color,
		align: int) -> Label:
	var label := Label.new()
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", color)
	label.horizontal_alignment = align
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(label)
	return label

## `‹ › <page>/<pages> · <N> sources · <total> · Work tab ▸`.
##
## **PAGING IS THE CONTROL HERE BECAUSE THE WHEEL IS ALREADY ZOOM.** A scrolling overlay over the map
## would fight the one gesture a player uses constantly, so there is deliberately no scroll.
func _build_footer() -> void:
	_prev_button = _page_button(PREV_PAGE_GLYPH, -1)
	_next_button = _page_button(NEXT_PAGE_GLYPH, 1)
	# The CONTROLS line — the page readout and the source count, which are both short and fixed-shape.
	_footer_label = _elided(_row_label(self, FOOTER_FONT_SIZE, HudStyle.INK_DIM,
		HORIZONTAL_ALIGNMENT_LEFT))
	_footer_label.mouse_filter = Control.MOUSE_FILTER_PASS   # IGNORE would swallow its own tooltip
	# **THE TOTAL LINE — the band's WHOLE income, on its own full-width strip** (see
	# `FOOTER_TOTAL_HEIGHT`). It still elides as a last resort, because a band earning four accounts
	# outruns any width this panel can afford; what the line buys is that the ordinary two- and
	# three-account bands are read rather than truncated.
	_total_label = _elided(_row_label(self, FOOTER_FONT_SIZE, HudStyle.INK_DIM,
		HORIZONTAL_ALIGNMENT_LEFT))
	_total_label.mouse_filter = Control.MOUSE_FILTER_PASS
	_work_tab_button = Button.new()
	_work_tab_button.text = WORK_TAB_LABEL
	_work_tab_button.add_theme_font_size_override("font_size", FOOTER_FONT_SIZE)
	HudStyle.apply_link_button(_work_tab_button, HudStyle.SIGNAL)
	_work_tab_button.pressed.connect(func() -> void: work_tab_requested.emit())
	add_child(_work_tab_button)

func _page_button(glyph: String, step: int) -> Button:
	var button := Button.new()
	button.text = glyph
	button.add_theme_font_size_override("font_size", FOOTER_FONT_SIZE)
	HudStyle.apply_link_button(button, HudStyle.INK)
	button.pressed.connect(_on_page_step.bind(step))
	add_child(button)
	return button

## **A PAGE STEP RE-RENDERS THE FOOTER TOO, AND ASKS THE MAP TO REDRAW.** Neither is optional: the
## footer carries the `<page>/<pages>` readout and the pager's own disabled states, so re-rendering
## only the rows leaves `1/2` showing on page 2 with `‹` still greyed out; and the leader lines belong
## to the map, so without the redraw they go on pointing at the page that just left.
func _on_page_step(step: int) -> void:
	var next := clampi(_page + step, FIRST_PAGE, _page_count() - 1)
	if next == _page:
		return
	_page = next
	_render_page()
	_render_footer(_total_text)
	page_changed.emit()

## **A ROW CLICK PANS TO ITS HEX — `focus_on_tile`, never `focus_and_select_tile`.** That second one
## routes through `handle_hex_click`, which RE-SELECTS the clicked tile and therefore CLEARS
## `selected_unit_id` on any hex with no band on it — closing the very list the click was made in.
## Focusing is the whole of what a row click means; the source's ring and the tile outline already
## mark the hex once the view is there.
func _on_row_pressed(index: int) -> void:
	var rows := page_rows()
	if index < 0 or index >= rows.size():
		return
	var tile: Vector2i = (rows[index] as Dictionary).get("tile", Vector2i(-1, -1))
	if tile.x < 0 or tile.y < 0:
		return
	tile_focus_requested.emit(tile.x, tile.y)

## Push this frame's model. `band_entity` resets the page and the placement sides when it changes.
func update_rows(rows: Array, band_entity: int, total_text: String) -> void:
	if band_entity != _band_entity:
		_band_entity = band_entity
		_page = FIRST_PAGE
		# Default inside the dead zone with no prior: right / below.
		_side_x = true
		_side_y = true
	_source_rows = rows
	# A page the model shrank out from under keeps the player on a page that exists.
	_page = clampi(_page, FIRST_PAGE, _page_count() - 1)
	_total_text = total_text
	_render_page()
	_render_footer(_total_text)

## Whether the footer draws its `Work tab ▸` link. `MapView` passes whether anything is connected to
## its own re-emitted signal, so a harness with no HUD shows no dead control.
func set_work_tab_available(available: bool) -> void:
	_work_tab_button.visible = available

## Visible iff a player band is selected AND it works at least one source — see `MapView`, which owns
## the first half of that test.
func hide_list() -> void:
	visible = false

func _page_count() -> int:
	return maxi(1, int(ceil(float(_source_rows.size()) / float(PAGE_SIZE))))

## The rows on the CURRENT page, in order — what the row slots render and what `MapView` runs leader
## lines to, so the two cannot index different things.
func page_rows() -> Array:
	var first := _page * PAGE_SIZE
	if first >= _source_rows.size():
		return []
	return _source_rows.slice(first, mini(first + PAGE_SIZE, _source_rows.size()))

## Which slot on this page is the FIRST ordinary row, `-1` when the page holds none. The sort has
## already put every attention row ahead of every ordinary one, so this is a boundary rather than a
## search.
func _first_ordinary_index(rows: Array) -> int:
	for i in range(rows.size()):
		if int((rows[i] as Dictionary).get("attention",
				BandOverlayRenderer.ATTENTION_NONE)) == BandOverlayRenderer.ATTENTION_NONE:
			return i
	return -1

func _render_page() -> void:
	var rows := page_rows()
	_rows_on_page = rows.size()
	var first_ordinary := _first_ordinary_index(rows)
	# The rule separates the last attention row from the first ordinary one, and only on a page that
	# holds BOTH — a page of all-calm rows draws no divider under nothing.
	_first_ordinary_on_page = first_ordinary
	_page_has_rule = first_ordinary > 0
	_rule.visible = _page_has_rule
	for i in range(PAGE_SIZE):
		var row: Button = _rows[i]
		if i >= rows.size():
			row.visible = false
			continue
		row.visible = true
		var model: Dictionary = rows[i]
		var sprite: Texture2D = model.get("sprite")
		_row_icons[i].texture = sprite
		_row_icons[i].visible = sprite != null
		var glyph := String(model.get("glyph", ""))
		_row_glyphs[i].text = glyph
		_row_glyphs[i].visible = sprite == null and glyph != ""
		_row_crews[i].text = "%s%d" % [HudSelectionVocab.SOURCE_CREW_MARK,
			int(model.get("crew", 0))]
		_row_rates[i].text = String(model.get("rate_text", ""))
		_row_rates[i].add_theme_color_override("font_color",
			HudStyle.WARN if bool(model.get("overdraw", false)) else HudStyle.HEALTHY)
		# The attention state takes the cell where it has one; the build countdown otherwise. Only
		# one of the two can be true of a row — the rank is resolved from them.
		var attention_text := String(model.get("attention_text", ""))
		var detail := attention_text if attention_text != "" else String(model.get("build_text", ""))
		_row_details[i].text = detail
		_row_details[i].add_theme_color_override("font_color",
			HudStyle.WARN if attention_text != "" else HudStyle.SIGNAL_DEEP)
		# **THE HOVER CARRIES BOTH ELIDED CELLS**, so nothing on this row can become unreachable: the
		# rate first (its own column clips a multi-material take), then the detail. A row with no
		# detail states the rate alone rather than trailing a separator over nothing.
		var hover: Array[String] = []
		if _row_rates[i].text != "":
			hover.append(_row_rates[i].text)
		if detail != "":
			hover.append(detail)
		row.tooltip_text = TOOLTIP_SEPARATOR.join(PackedStringArray(hover))

func _render_footer(total_text: String) -> void:
	var pages := _page_count()
	_prev_button.disabled = _page <= FIRST_PAGE
	_next_button.disabled = _page >= pages - 1
	var count := _source_rows.size()
	var parts: Array[String] = [
		FOOTER_PAGE_FORMAT % [_page + 1, pages],
		FOOTER_COUNT_ONE if count == 1 else FOOTER_COUNT_FORMAT % count,
	]
	_footer_label.text = FOOTER_SEPARATOR.join(parts)
	_footer_label.tooltip_text = _footer_label.text
	# **THE TOTAL IS THE SAME STRING ON EVERY PAGE** — it is the BAND's income across every source,
	# not the page's, which is the only reason it is worth reading at all. It arrives already composed
	# and this function never recomputes it from the visible rows.
	_total_label.text = total_text
	_total_label.tooltip_text = total_text
	_total_label.visible = total_text != ""

## The BAND's whole income across every source, exactly as the total line states it — the string a
## probe compares across pages. Public because that invariance is this footer's whole justification
## and a harness asking it through the private label is how it came to be read off the wrong one.
func total_text() -> String:
	return _total_label.text if _total_label != null else ""

## The panel's height for the CURRENT page — pure arithmetic over the row count.
func _panel_height() -> float:
	return PAD * 2.0 + float(_rows_on_page) * ROW_HEIGHT \
		+ (ATTENTION_RULE_H if _page_has_rule else 0.0) + FOOTER_HEIGHT \
		+ (FOOTER_TOTAL_HEIGHT if _total_label != null and _total_label.visible else 0.0)

## The y of a row slot's TOP within the panel, the rule's gap included once the page has crossed it.
func _row_top(index: int) -> float:
	var y := PAD + float(index) * ROW_HEIGHT
	if _page_has_rule and index >= _first_ordinary_on_page:
		y += ATTENTION_RULE_H
	return y

## **PLACE THE PANEL BESIDE THE BAND — Ray's quadrant rule, with a dead zone.**
##
## Band LEFT of the unreserved rect's centre → open RIGHT; right → open LEFT. Above the centre → open
## BELOW; below → open ABOVE. So the panel always opens into the roomier half of the room the docked
## panels left.
##
## **THE DEAD ZONE IS THE WHOLE OF WHY THE SIDES ARE MEMBERS.** Within `DEAD_ZONE_FRACTION` of the
## centre on an axis the side does not change from the one already held, so a band parked near the
## middle does not flip the list from one side to the other while the player nudges the map.
##
## `bounds` is `MapView.unreserved_screen_rect()` — the viewport less every edge a docked panel has
## reserved. The quadrant rule points into the roomier half, but at an extreme zoom or a tiny window
## the panel can still exceed the room, so the rect is CLAMPED into `bounds` afterwards.
##
## ⛔ **`band_avoid` IS THE BAND'S WHOLE INKED FOOTPRINT, NOT ITS CENTRE, AND THAT IS A FIX.** The gap
## used to be measured from the token's centre, so a panel opening below-right landed squarely on the
## band's NAME PILL — which hangs BELOW the token and is wider than it. `MapView` unions the token's
## box with the nameplate footprint `BandMarkerRenderer` already measured (never a second measurement
## of a plate — `map-markers.md` → "A plate's half-extent is ONE expression"), and the gap is taken
## from that rect's EDGES. **The quadrant test still uses the rect's CENTRE**, so the rule itself is
## unchanged: a footprint's centre is the token's centre nudged by the plate hanging under it.
func place(band_avoid: Rect2, bounds: Rect2) -> void:
	var height := _panel_height()
	size = Vector2(PANEL_WIDTH, height)
	var centre := bounds.position + bounds.size * 0.5
	var band_centre := band_avoid.position + band_avoid.size * 0.5
	if absf(band_centre.x - centre.x) >= bounds.size.x * DEAD_ZONE_FRACTION:
		_side_x = band_centre.x < centre.x
	if absf(band_centre.y - centre.y) >= bounds.size.y * DEAD_ZONE_FRACTION:
		_side_y = band_centre.y < centre.y
	var x := band_avoid.end.x + BAND_GAP if _side_x \
		else band_avoid.position.x - BAND_GAP - PANEL_WIDTH
	var y := band_avoid.end.y + BAND_GAP if _side_y \
		else band_avoid.position.y - BAND_GAP - height
	position = Vector2(
		clampf(x, bounds.position.x, maxf(bounds.position.x, bounds.end.x - PANEL_WIDTH)),
		clampf(y, bounds.position.y, maxf(bounds.position.y, bounds.end.y - height)))
	_lay_out(height)

## Position every child off the same arithmetic `place` just spent. A Control is not a Container, so
## nothing here is deferred and `row_anchor` can be asked in the same frame.
func _lay_out(height: float) -> void:
	_background.position = Vector2.ZERO
	_background.size = Vector2(PANEL_WIDTH, height)
	var detail_x := PAD + ICON_WIDTH + CREW_WIDTH + RATE_WIDTH + COL_GAP * 3.0
	var detail_w := PANEL_WIDTH - PAD - detail_x
	for i in range(PAGE_SIZE):
		if not _rows[i].visible:
			continue
		_rows[i].position = Vector2(PAD, _row_top(i))
		_rows[i].size = Vector2(PANEL_WIDTH - PAD * 2.0, ROW_HEIGHT)
		# Children are positioned RELATIVE to the row, so the row's own x drops out of each column.
		_row_icons[i].position = Vector2(0.0, (ROW_HEIGHT - ICON_WIDTH) * 0.5)
		_row_icons[i].size = Vector2(ICON_WIDTH, ICON_WIDTH)
		_row_glyphs[i].position = Vector2.ZERO
		_row_glyphs[i].size = Vector2(ICON_WIDTH, ROW_HEIGHT)
		_row_crews[i].position = Vector2(ICON_WIDTH + COL_GAP, 0.0)
		_row_crews[i].size = Vector2(CREW_WIDTH, ROW_HEIGHT)
		_row_rates[i].position = Vector2(ICON_WIDTH + CREW_WIDTH + COL_GAP * 2.0, 0.0)
		_row_rates[i].size = Vector2(RATE_WIDTH, ROW_HEIGHT)
		_row_details[i].position = Vector2(detail_x - PAD, 0.0)
		_row_details[i].size = Vector2(detail_w, ROW_HEIGHT)
	if _page_has_rule:
		_rule.position = Vector2(PAD, _row_top(_first_ordinary_on_page) - ATTENTION_RULE_H
			+ (ATTENTION_RULE_H - ATTENTION_RULE_LINE_H) * 0.5)
		_rule.size = Vector2(PANEL_WIDTH - PAD * 2.0, ATTENTION_RULE_LINE_H)
	# The TOTAL line sits under the controls line, so the controls line's y is measured from the
	# panel's bottom with the total's own strip already reserved (or not, when there is no total).
	var total_h := FOOTER_TOTAL_HEIGHT if _total_label.visible else 0.0
	var footer_y := height - PAD - total_h - FOOTER_HEIGHT
	_total_label.position = Vector2(PAD, footer_y + FOOTER_HEIGHT)
	_total_label.size = Vector2(PANEL_WIDTH - PAD * 2.0, total_h)
	_prev_button.position = Vector2(PAD, footer_y)
	_prev_button.size = Vector2(PAGE_BUTTON_WIDTH, FOOTER_HEIGHT)
	_next_button.position = Vector2(PAD + PAGE_BUTTON_WIDTH, footer_y)
	_next_button.size = Vector2(PAGE_BUTTON_WIDTH, FOOTER_HEIGHT)
	var footer_x := PAD + PAGE_BUTTON_WIDTH * 2.0 + COL_GAP
	_work_tab_button.position = Vector2(PANEL_WIDTH - PAD - WORK_TAB_WIDTH, footer_y)
	_work_tab_button.size = Vector2(WORK_TAB_WIDTH, FOOTER_HEIGHT)
	_footer_label.position = Vector2(footer_x, footer_y)
	_footer_label.size = Vector2(
		maxf(0.0, _work_tab_button.position.x - COL_GAP - footer_x)
			if _work_tab_button.visible else PANEL_WIDTH - PAD - footer_x,
		FOOTER_HEIGHT)

## **WHERE A ROW'S LEADER LINE LEAVES THE PANEL**, in CANVAS units — the y of the row's vertical
## centre, and the panel edge FACING the hex the line is going to. Leaving from the near side is what
## makes the leaders fan out instead of crossing back over the panel they came from.
func row_anchor(index_on_page: int, toward: Vector2) -> Vector2:
	var y := position.y + _row_top(index_on_page) + ROW_HEIGHT * 0.5
	var x := position.x if toward.x < position.x + PANEL_WIDTH * 0.5 else position.x + PANEL_WIDTH
	return Vector2(x, y)
