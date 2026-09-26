class_name TradeZoneController
extends RefCounted

## **THE BAND DOCK'S TRADE TAB** (issue #731; the spec is `docs/band_trade_tab_ux_proposal.html`,
## layout C) — what crossed this band's store with SOMEBODY
## ELSE this turn, by the link it crossed. A controller of its own rather than more of
## `BandPanelController` (the HUD decomposition invariant, `hud-modules.md`): the panel controller
## asks it for the zone's content and pushes the band, and this owns everything the tab draws — the
## zone, its list popover and its hover card.
##
## **TWO TIERS, CHOSEN BY MEASUREMENT.** The FULL tier is the network line, then `⇄ Local exchange`
## (one line per good, its net) and `⇄ Trade route` (Imports / Exports, one line per shipment). The
## SHORT tier is the network line and one row per arm, each opening its list. Which one is drawn is
## the full tier's MEASURED content height against the zone's own `zone_size()` height — never a
## hard-coded number — so a side dock draws the full tab and a bottom dock's 418px body, which the
## full tab overruns at its busiest, draws the short one. The panel never grows for it.
##
## **NOTHING EXPANDS IN PLACE.** Every list too long for the zone — the camps, a folded direction, the
## SHORT tier's arms — opens in the LIST POPOVER, so the zone's height never depends on what the
## player opened, which is the whole reason it can have a fixed budget. The zone itself owns NO
## `ScrollContainer` (`band_panel_preview._assert_scroll_only_where_sanctioned`).
##
## **THE POPOVER IS THE BAND TAB'S DISCLOSURE IDIOM** (`DisclosureController._open_popover`): a
## `PopupPanel` — a WINDOW, so it changes no zone's height — in `HudStyle.card_stylebox()`, anchored
## at the bottom-left of the row that opened it and as wide as the Trade column, so it reads as part
## of the card rather than a separate window. It opens below when the whole list fits there, and
## otherwise on whichever side of the row has more room, capped to that room and scrolling inside
## itself past it. Dismissal is the popup's own — a click away or ESC — which is also why it
## is on no ESC chain and needs no exclusion against the work inspector: a click anywhere else closes
## it.
##
## **WHAT COUNTS AS TRADE** is `TradeLedger.is_trade`: pooling (Local) and shipments (Route). A band's
## own party coming home, a party's rations and a split's dowry never reach this tab.

## List popover keys. A `camps` key may carry a commodity scope after `KIND_SCOPE_SEPARATOR`.
const KIND_CAMPS := "camps"
const KIND_LOCAL := "local"
const KIND_ROUTE_IN := "route_in"
const KIND_ROUTE_OUT := "route_out"
const KIND_ROUTE_BOTH := "route_both"
const KIND_SCOPE_SEPARATOR := "|"

## Node names a harness finds the tab's pieces by.
const FULL_TIER_NAME := "TradeFullTier"
const SHORT_TIER_NAME := "TradeShortTier"
const NETWORK_LINE_NAME := "TradeNetworkLine"
## The meta a row carries naming what it opens / what it shows, for a harness to drive.
const ROW_OPENS_META := &"trade_opens"
const ROW_GOOD_META := &"trade_good"
const ROW_SHIPMENT_META := &"trade_shipment"

## The rule between the parties list and the Trade section on a wide shell.
const SECTION_RULE_HEIGHT := 1.0

var _band_labor: HudBandLaborState = null
var _topbar: FactionReadouts = null
var _panel: BandCityPanel = null
var _host: Node = null

## The list popover and its pieces, built lazily on the first open.
var _popover: PopupPanel = null
var _popover_title: Label = null
var _popover_count: Label = null
var _popover_sub: Label = null
var _popover_scroll: ScrollContainer = null
var _popover_body: VBoxContainer = null
## The open list's key (`""` = closed), the row kind it is anchored under, whether it took the room
## above, and the anchor row's screen rect as of the last placement.
var _open_kind: String = ""
var _anchor_kind: String = ""
var _opened_above: bool = false
var _anchor_screen: Rect2 = Rect2()
## The frame the popover last hid on, and what it showed. A click on the row that opened it closes
## it TWICE over — the popup's own click-away, then the row's press — so a press landing on the same
## frame as that hide, for the same list, is the close and must not reopen it.
var _closed_frame: int = -1
var _closed_kind: String = ""
var _hover: TradeHoverCard = null
## The band the zone was last built for, and the column it was built into — kept so a resize can
## re-author the tier in place.
var _band: Dictionary = {}
var _column: VBoxContainer = null
var _available: Vector2 = Vector2.ZERO
## Which tier the last build drew — read by the harness and by nothing that decides anything.
var _short_tier: bool = false
## The band the open list belongs to; a different band closes it.
var _list_band_id: int = HudConst.NO_BAND_ID

func _init(band_labor: HudBandLaborState, host: Node) -> void:
	_band_labor = band_labor
	_host = host

func set_topbar(topbar: FactionReadouts) -> void:
	_topbar = topbar

func set_panel(panel: BandCityPanel) -> void:
	_panel = panel

func is_short_tier() -> bool:
	return _short_tier

## The open list's rows, for the harness.
func list_body() -> VBoxContainer:
	return _popover_body

## The popover's rect and its anchor row's, both in SCREEN space (what `Popup` is placed in) — the
## harness asserts the two are adjacent.
func list_screen_rect() -> Rect2:
	if not is_list_open():
		return Rect2()
	return _card_rect_of(Rect2(Vector2(_popover.position), Vector2(_popover.size)))

## **THE CARD A POPUP WINDOW DRAWS IS INSET FROM THE WINDOW** by its panel's shadow: a `PopupPanel`
## reserves room for the shadow inside its own rect (`shadow_size`, shifted by `shadow_offset`). So the
## placement below sizes the WINDOW to put the drawn CARD where it belongs, and every rect a reader is
## given is the card's.
func _shadow_insets() -> Array[float]:
	var sb := HudStyle.card_stylebox()
	var size := float(sb.shadow_size)
	return [size - sb.shadow_offset.x, size - sb.shadow_offset.y,
		size + sb.shadow_offset.x, size + sb.shadow_offset.y]

func _card_rect_of(window: Rect2) -> Rect2:
	var inset := _shadow_insets()
	return Rect2(window.position + Vector2(inset[0], inset[1]),
		window.size - Vector2(inset[0] + inset[2], inset[1] + inset[3]))

func anchor_screen_rect() -> Rect2:
	return _anchor_screen

func list_opened_above() -> bool:
	return _opened_above

func hover_card() -> TradeHoverCard:
	return _hover

## The tab's badge — this turn's shipments, both ways; `""` on a turn with none. Pooling never counts.
static func badge_text(band: Dictionary) -> String:
	var count := TradeLedger.shipment_count(band)
	return str(count) if count > 0 else ""

# ---- THE ZONE --------------------------------------------------------------------------------------

## The Trade content for `band`, authored against `available` (the zone's `zone_size()`; the parties
## zone's on a wide shell). The FULL tier where it fits, else the SHORT one.
func build(band: Dictionary, available: Vector2) -> VBoxContainer:
	_band = band
	_available = available
	_column = HudWidgets.make_zone_column()
	_column.add_theme_constant_override("separation", HudTradeVocab.SECTION_SEPARATION)
	_column.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	_fill(_column)
	sync_list(band)
	return _column

## The same content as a section appended under the Parties zone's list — the wide shell's home for
## it (option iii): a rule, the `Trade` head, then the tier.
func build_section(band: Dictionary, available: Vector2) -> VBoxContainer:
	var section := HudWidgets.make_zone_block()
	section.name = "TradeSection"
	section.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	var rule := ColorRect.new()
	rule.color = HudStyle.LINE_SOFT
	rule.custom_minimum_size = Vector2(0.0, SECTION_RULE_HEIGHT)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	section.add_child(rule)
	section.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_TAB_TRADE, ""))
	section.add_child(build(band, available))
	return section

## Re-author the tier in place against a new box — the resize path, which must not rebuild the zones.
func refill(available: Vector2) -> void:
	if _column == null or not is_instance_valid(_column) or _band.is_empty():
		return
	_available = available
	HudWidgets.clear_children(_column)
	_fill(_column)

func _fill(col: VBoxContainer) -> void:
	var full := _build_full_tier(_band)
	var needed := full.get_combined_minimum_size().y
	_short_tier = TradeLedger.has_trade(_band) and needed > _available.y
	if _short_tier:
		full.free()
		col.add_child(_build_short_tier(_band))
	else:
		col.add_child(full)

func _build_full_tier(band: Dictionary) -> VBoxContainer:
	var tier := _tier_column(FULL_TIER_NAME)
	tier.add_child(_build_network_line(band))
	if not TradeLedger.has_trade(band):
		_add_empty_state(tier)
		return tier
	# ⇄ LOCAL EXCHANGE — one line per good, its net this turn.
	var goods := TradeLedger.goods_net(band)
	tier.add_child(HudWidgets.zone_head(DetailFormat.TRANSFER_LABEL_LOCAL, _goods_count_text(goods.size())))
	var local_rows := _rows_column()
	for good in goods:
		local_rows.add_child(_build_good_row(band, good))
	tier.add_child(local_rows)
	# ⇄ TRADE ROUTE — Imports then Exports, each counted in shipments and folded past the trigger.
	var shipments := TradeLedger.shipments(band)
	tier.add_child(HudWidgets.zone_head(DetailFormat.TRANSFER_LABEL_ROUTE,
		_shipment_count_text(shipments.size())))
	var route_rows := _rows_column()
	for direction in [HudTradeVocab.DIRECTION_IN, HudTradeVocab.DIRECTION_OUT]:
		var half := TradeLedger.shipments_in_direction(band, direction)
		if half.is_empty():
			continue
		route_rows.add_child(_build_direction_head(direction, half.size()))
		var keep := half.size()
		if half.size() > HudTradeVocab.SHIPMENT_FOLD_TRIGGER:
			keep = HudTradeVocab.SHIPMENT_FOLD_KEEP
		for i in range(keep):
			route_rows.add_child(_build_shipment_row(band, half[i]))
		if half.size() > keep:
			var rest := half.size() - keep
			var more_format := HudTradeVocab.MORE_SHIPMENT_SINGULAR_FORMAT if rest == 1 \
				else HudTradeVocab.MORE_SHIPMENT_PLURAL_FORMAT
			var kind := KIND_ROUTE_IN if direction == HudTradeVocab.DIRECTION_IN else KIND_ROUTE_OUT
			route_rows.add_child(_build_opens_row("%s %s" % [HudTradeVocab.OPENS_CARET, more_format % rest],
				HudTradeVocab.SHIPMENT_INDENT, kind))
	tier.add_child(route_rows)
	return tier

## **THE SHORT TIER — the zone becomes a way in, not a readout.** The network line, then one row per
## arm stating what happened, each opening its full list over the map.
func _build_short_tier(band: Dictionary) -> VBoxContainer:
	var tier := _tier_column(SHORT_TIER_NAME)
	tier.add_child(_build_network_line(band))
	var goods := TradeLedger.goods_net(band)
	tier.add_child(_build_arm_row(DetailFormat.TRANSFER_LABEL_LOCAL,
		_goods_count_text(goods.size()), KIND_LOCAL if not goods.is_empty() else ""))
	var imports := TradeLedger.shipments_in_direction(band, HudTradeVocab.DIRECTION_IN).size()
	var exports := TradeLedger.shipments_in_direction(band, HudTradeVocab.DIRECTION_OUT).size()
	tier.add_child(_build_arm_row(DetailFormat.TRANSFER_LABEL_ROUTE,
		_shipment_count_text(imports + exports), KIND_ROUTE_BOTH if imports + exports > 0 else ""))
	if imports + exports > 0:
		var split := _faint_label(HudTradeVocab.SPLIT_JOIN.join([
			(HudTradeVocab.IMPORT_SINGULAR_FORMAT if imports == 1 else HudTradeVocab.IMPORT_PLURAL_FORMAT) % imports,
			(HudTradeVocab.EXPORT_SINGULAR_FORMAT if exports == 1 else HudTradeVocab.EXPORT_PLURAL_FORMAT) % exports,
		]))
		tier.add_child(split)
	return tier

func _tier_column(tier_name: String) -> VBoxContainer:
	var tier := VBoxContainer.new()
	tier.name = tier_name
	tier.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tier.add_theme_constant_override("separation", HudTradeVocab.SECTION_SEPARATION)
	return tier

func _rows_column() -> VBoxContainer:
	var rows := VBoxContainer.new()
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rows.add_theme_constant_override("separation", HudTradeVocab.ROWS_SEPARATION)
	return rows

## `NETWORK  15 camps ›   within 6 tiles` — the camp count opens the camps panel; the span is the one
## the links actually formed at (`supply_network_span_tiles`), never the config's free reach.
func _build_network_line(band: Dictionary) -> PanelContainer:
	var box := PanelContainer.new()
	box.name = NETWORK_LINE_NAME
	box.add_theme_stylebox_override("panel", _network_stylebox())
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	box.add_child(row)
	var key := HudWidgets.alloc_section_label(HudTradeVocab.NETWORK_KEY_WORD)
	key.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(key)
	var members := _members(band)
	if members.is_empty():
		row.add_child(_row_label(HudTradeVocab.NETWORK_NONE, HudStyle.INK_DIM))
		return box
	var camps := HudWidgets.build_inline_link(HudTradeVocab.NETWORK_CAMPS_FORMAT % members.size(),
		HudStyle.SIGNAL, func() -> void: open_list(KIND_CAMPS, box))
	camps.add_theme_font_size_override("font_size", HudTradeVocab.ROW_FONT_SIZE)
	camps.tooltip_text = HudTradeVocab.NETWORK_CAMPS_TOOLTIP
	camps.set_meta(ROW_OPENS_META, KIND_CAMPS)
	row.add_child(camps)
	row.add_child(_spacer())
	var span := TradeLedger.network_span(band)
	row.add_child(_row_label(HudTradeVocab.NETWORK_SPAN_FORMAT % span if span > 0
		else HudTradeVocab.NETWORK_SPAN_SHARED, HudStyle.SIGNAL))
	return box

func _add_empty_state(tier: VBoxContainer) -> void:
	var head := _row_label(HudTradeVocab.EMPTY_HEAD, HudStyle.INK_DIM)
	tier.add_child(head)
	tier.add_child(HudWidgets.alloc_hint_label(HudTradeVocab.EMPTY_BODY))

## One good's row: `Bone  6 ratings ····· ▲ 0.8`. Clicks open the camps panel scoped to the good;
## a material hovers to its rating piles.
func _build_good_row(band: Dictionary, good: Dictionary) -> Control:
	var commodity := String(good[TradeLedger.GOOD_COMMODITY])
	var piles: Array = good[TradeLedger.GOOD_PILES]
	var shell := _row_shell(0, _camps_kind(commodity))
	shell.set_meta(ROW_GOOD_META, commodity)
	var row: HBoxContainer = shell.get_child(0)
	row.add_child(_row_label(TradeLedger.commodity_label(commodity), HudStyle.INK))
	if piles.size() > 1:
		var ratings := _faint_label(HudTradeVocab.RATINGS_FORMAT % piles.size())
		row.add_child(ratings)
	row.add_child(_spacer())
	row.add_child(_net_label(float(good[TradeLedger.GOOD_NET])))
	if TradeLedger.is_material(commodity):
		_attach_hover(shell, func() -> Array[Control]: return _good_card_lines(good))
	return shell

## A direction's sub-head under TRADE ROUTE: `▲ Imports ····· 2 shipments`.
func _build_direction_head(direction: int, count: int) -> Control:
	var margin := _indent(HudTradeVocab.DIRECTION_INDENT)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	margin.add_child(row)
	var inbound := direction == HudTradeVocab.DIRECTION_IN
	row.add_child(_row_label(HudTradeVocab.IN_GLYPH if inbound else HudTradeVocab.OUT_GLYPH,
		HudStyle.HEALTHY if inbound else HudStyle.WARN))
	row.add_child(_row_label(HudTradeVocab.IMPORTS_WORD if inbound else HudTradeVocab.EXPORTS_WORD,
		HudStyle.INK))
	row.add_child(_spacer())
	row.add_child(_faint_label(_shipment_count_text(count)))
	return margin

## One shipment: `← Barrowmere ⚑ ····· 1.5 hide`. No date — every row on the tab crossed THIS turn.
## Route rows do not click through to camps: a party carried it, from a band that need not pool here.
func _build_shipment_row(band: Dictionary, shipment: Dictionary) -> Control:
	var shell := _row_shell(HudTradeVocab.SHIPMENT_INDENT, "")
	shell.set_meta(ROW_SHIPMENT_META, TradeLedger.counterparty_label(shipment))
	var row: HBoxContainer = shell.get_child(0)
	_add_counterparty(row, band, shipment)
	# The cargo takes the rest of the row, right-aligned where every amount on the tab sits, and is
	# the part that gives way (an ellipsis, the whole of it on the hover card) when a name is long.
	var cargo := _faint_label(_cargo_text(shipment))
	cargo.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	cargo.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	cargo.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	row.add_child(cargo)
	_attach_hover(shell, func() -> Array[Control]: return _shipment_card_lines(band, shipment))
	return shell

func _add_counterparty(row: HBoxContainer, band: Dictionary, shipment: Dictionary) -> void:
	var inbound := int(shipment[TradeLedger.SHIPMENT_DIRECTION]) == HudTradeVocab.DIRECTION_IN
	var name := _row_label("%s %s" % [HudTradeVocab.IMPORT_ARROW if inbound else HudTradeVocab.EXPORT_ARROW,
		TradeLedger.counterparty_label(shipment)], HudStyle.INK_DIM)
	row.add_child(name)
	row.add_child(FactionMark.make(int(shipment[TradeLedger.SHIPMENT_COUNTERPARTY_FACTION]),
		_own_faction(band)))

## The SHORT tier's arm row: `⇄ LOCAL EXCHANGE ····· 8 goods ›`, opening `kind` (none when empty).
func _build_arm_row(title: String, count: String, kind: String) -> Control:
	var shell := _row_shell(0, kind)
	var row: HBoxContainer = shell.get_child(0)
	var label := HudWidgets.alloc_section_label(title)
	label.add_theme_color_override("font_color", HudStyle.SIGNAL if kind != "" else HudStyle.INK_FAINT)
	row.add_child(label)
	row.add_child(_spacer())
	row.add_child(_row_label(count, HudStyle.INK_DIM))
	if kind != "":
		row.add_child(_row_label(HudTradeVocab.OPENS_CARET, HudStyle.INK_FAINT))
	return shell

## A row that only opens a list — `› 2 more shipments`.
func _build_opens_row(text: String, indent: int, kind: String) -> Control:
	var shell := _row_shell(indent, kind)
	var row: HBoxContainer = shell.get_child(0)
	row.add_child(_row_label(text, HudStyle.SIGNAL))
	return shell

# ---- ROW PLUMBING ----------------------------------------------------------------------------------

## A row: a `PanelContainer` (so it can take the pointer and draw a wash) around an indented `HBox`.
## `opens` names the list a click opens, `""` for a row that does not click.
func _row_shell(indent: int, opens: String) -> PanelContainer:
	var shell := PanelContainer.new()
	shell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	shell.add_theme_stylebox_override("panel", _row_stylebox(indent, false))
	shell.mouse_filter = Control.MOUSE_FILTER_PASS
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	shell.add_child(row)
	if opens != "":
		shell.set_meta(ROW_OPENS_META, opens)
		shell.mouse_filter = Control.MOUSE_FILTER_STOP
		shell.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		shell.gui_input.connect(func(event: InputEvent) -> void:
			if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
					and event.pressed:
				open_list(opens, shell))
		shell.mouse_entered.connect(func() -> void:
			shell.add_theme_stylebox_override("panel", _row_stylebox(indent, true)))
		shell.mouse_exited.connect(func() -> void:
			shell.add_theme_stylebox_override("panel", _row_stylebox(indent, false)))
	return shell

func _row_stylebox(indent: int, lit: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.GHOST_BG if lit else Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = indent
	sb.set_corner_radius_all(HudTradeVocab.CHIP_CORNER_RADIUS)
	return sb

func _attach_hover(control: Control, lines_fn: Callable) -> void:
	if control.mouse_filter == Control.MOUSE_FILTER_IGNORE:
		control.mouse_filter = Control.MOUSE_FILTER_PASS
	control.mouse_entered.connect(func() -> void: _show_hover(control, lines_fn.call()))
	control.mouse_exited.connect(func() -> void: _hide_hover())
	control.set_meta(&"trade_hover", lines_fn)

## Show the hover card for `control` — for a harness as much as for the pointer, so a frame can be
## taken of it without synthesising a mouse move.
func show_hover_for(control: Control) -> void:
	var lines_fn: Variant = control.get_meta(&"trade_hover", null)
	if lines_fn is Callable:
		_show_hover(control, (lines_fn as Callable).call())

func _show_hover(control: Control, lines: Array[Control]) -> void:
	var card := _ensure_hover()
	if card == null or not control.is_inside_tree():
		for line in lines:
			line.free()
		return
	# Both rects in the CARD's space (the hover layer's), since a row may live in the popover's own
	# window: through the screen, the one space every window shares.
	var avoid := Rect2()
	if is_list_open():
		avoid = _screen_to_layer(card, list_screen_rect())
	card.show_for(lines, _screen_to_layer(card, _screen_rect(control)), avoid)

func _hide_hover() -> void:
	if _hover != null and is_instance_valid(_hover):
		_hover.dismiss()

func _indent(pixels: int) -> MarginContainer:
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	margin.add_theme_constant_override("margin_left", pixels)
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return margin

func _row_label(text: String, ink: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", HudTradeVocab.ROW_FONT_SIZE)
	label.add_theme_color_override("font_color", ink)
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return label

func _faint_label(text: String) -> Label:
	var label := _row_label(text, HudStyle.INK_FAINT)
	label.add_theme_font_size_override("font_size", HudTradeVocab.SUB_FONT_SIZE)
	return label

func _spacer() -> Control:
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return spacer

## A good's NET, in the shared transfer tints: `▲ 0.8` green in, `▼ 0.4` amber out, `even` faint.
func _net_label(net: float) -> Label:
	if TradeLedger.is_even(net):
		return _row_label(HudTradeVocab.EVEN_WORD, HudStyle.INK_FAINT)
	var inbound := net > 0.0
	return _row_label(HudTradeVocab.AMOUNT_FORMAT % [
		HudTradeVocab.IN_GLYPH if inbound else HudTradeVocab.OUT_GLYPH, absf(net)],
		HudStyle.HEALTHY if inbound else HudStyle.WARN)

## A pile's SIGNED amount on a hover card: `+0.8` green, `-0.4` amber.
func _signed_label(net: float) -> Label:
	return _row_label(HudTradeVocab.SIGNED_AMOUNT_FORMAT % net,
		HudStyle.HEALTHY if net >= 0.0 else HudStyle.WARN)

## `axis: band` chips off a pile's readings, the Crafting panel's own spelling.
func _chips(readings: Array) -> HBoxContainer:
	var chips := HBoxContainer.new()
	chips.add_theme_constant_override("separation", HudTradeVocab.CHIP_SEPARATION)
	chips.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for reading_variant in readings:
		if not (reading_variant is Dictionary):
			continue
		var reading: Dictionary = reading_variant
		var chip := PanelContainer.new()
		chip.mouse_filter = Control.MOUSE_FILTER_IGNORE
		chip.add_theme_stylebox_override("panel", _chip_stylebox())
		var label := Label.new()
		label.text = HudCraftingVocab.CHARACTERISTIC_CHIP_FORMAT % [
			String(reading.get(HudCraftingVocab.READING_AXIS_KEY, "")),
			String(reading.get(HudCraftingVocab.READING_BAND_NAME_KEY, ""))]
		label.add_theme_font_size_override("font_size", HudTradeVocab.SUB_FONT_SIZE)
		label.add_theme_color_override("font_color", HudStyle.INK_DIM)
		chip.add_child(label)
		chips.add_child(chip)
	return chips

func _chip_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.GROUND_2
	sb.border_color = HudStyle.LINE
	sb.set_border_width_all(HudTradeVocab.CHIP_BORDER_WIDTH)
	sb.set_corner_radius_all(HudTradeVocab.CHIP_CORNER_RADIUS)
	sb.content_margin_left = HudTradeVocab.CHIP_PADDING_H
	sb.content_margin_right = HudTradeVocab.CHIP_PADDING_H
	sb.content_margin_top = HudTradeVocab.CHIP_PADDING_V
	sb.content_margin_bottom = HudTradeVocab.CHIP_PADDING_V
	return sb

func _network_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.GROUND_2
	sb.border_color = HudStyle.LINE_SOFT
	sb.set_border_width_all(HudTradeVocab.NETWORK_BORDER_WIDTH)
	sb.set_corner_radius_all(HudTradeVocab.NETWORK_CORNER_RADIUS)
	sb.content_margin_left = HudTradeVocab.NETWORK_PADDING_H
	sb.content_margin_right = HudTradeVocab.NETWORK_PADDING_H
	sb.content_margin_top = HudTradeVocab.NETWORK_PADDING_V
	sb.content_margin_bottom = HudTradeVocab.NETWORK_PADDING_V
	return sb

# ---- HOVER CARDS -----------------------------------------------------------------------------------

## A good's card: its name and pile count, then each pile — its rating chips and its own signed
## amount. No footer.
func _good_card_lines(good: Dictionary) -> Array[Control]:
	var lines: Array[Control] = []
	var piles: Array = good[TradeLedger.GOOD_PILES]
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	head.add_child(_row_label(TradeLedger.commodity_label(String(good[TradeLedger.GOOD_COMMODITY])),
		HudStyle.INK))
	head.add_child(_faint_label(HudTradeVocab.RATINGS_FORMAT % piles.size()))
	lines.append(head)
	for pile_variant in piles:
		var pile: Dictionary = pile_variant
		var line := HBoxContainer.new()
		line.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
		line.add_child(_chips(pile[TradeLedger.PILE_READINGS]))
		line.add_child(_spacer())
		line.add_child(_signed_label(float(pile[TradeLedger.PILE_NET])))
		lines.append(line)
	return lines

## A shipment's card: who, then everything it carried, pile by pile, each at its own rating.
func _shipment_card_lines(band: Dictionary, shipment: Dictionary) -> Array[Control]:
	var lines: Array[Control] = []
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	_add_counterparty(head, band, shipment)
	lines.append(head)
	lines.append_array(_cargo_lines(shipment, 0))
	return lines

## One line per pile of a shipment's cargo: `Hide [toughness: excellent] ····· 1.5`.
func _cargo_lines(shipment: Dictionary, indent: int) -> Array[Control]:
	var lines: Array[Control] = []
	var inbound := int(shipment[TradeLedger.SHIPMENT_DIRECTION]) == HudTradeVocab.DIRECTION_IN
	for good_variant in shipment[TradeLedger.SHIPMENT_CARGO]:
		var good: Dictionary = good_variant
		for pile_variant in good[TradeLedger.GOOD_PILES]:
			var pile: Dictionary = pile_variant
			var margin := _indent(indent)
			var line := HBoxContainer.new()
			line.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
			margin.add_child(line)
			line.add_child(_faint_label(TradeLedger.commodity_label(String(good[TradeLedger.GOOD_COMMODITY]))))
			line.add_child(_chips(pile[TradeLedger.PILE_READINGS]))
			line.add_child(_spacer())
			var amount := float(pile[TradeLedger.PILE_NET])
			line.add_child(_net_label(amount if inbound else -amount))
			lines.append(margin)
	return lines

## `12.0 food · 1.1 wood` — food and fodder first, then materials, each good summed across its ratings.
func _cargo_text(shipment: Dictionary) -> String:
	var terms: Array[String] = []
	for good_variant in shipment[TradeLedger.SHIPMENT_CARGO]:
		var good: Dictionary = good_variant
		terms.append(HudTradeVocab.CARGO_TERM_FORMAT % [float(good[TradeLedger.GOOD_NET]),
			TradeLedger.commodity_label(String(good[TradeLedger.GOOD_COMMODITY])).to_lower()])
	return HudTradeVocab.SPLIT_JOIN.join(terms)

# ---- THE LIST POPOVER ------------------------------------------------------------------------------

func _camps_kind(commodity: String) -> String:
	return KIND_CAMPS + KIND_SCOPE_SEPARATOR + commodity

## Open the list popover on `kind`, under `anchor` — or, when `anchor` is null, under the zone row
## that opens `kind` (the harness's way in). **A row INSIDE the popover swaps the content in place and
## keeps the anchor**; a row in the zone re-anchors. The row that opened the list closes it.
func open_list(kind: String, anchor: Control = null) -> void:
	if kind == "" or _band.is_empty():
		return
	if kind == _closed_kind and Engine.get_process_frames() == _closed_frame:
		return
	if is_list_open() and kind == _open_kind and (anchor == null or not _in_popover(anchor)):
		dismiss()
		return
	_hide_hover()
	var popover := _ensure_popover()
	if popover == null:
		return
	if anchor == null or not _in_popover(anchor):
		_anchor_kind = kind
	# **A POPUP HIDDEN THIS FRAME CANNOT BE RE-OPENED THIS FRAME.** A press on another Trade row while a
	# list is up first closes it (the popup's own click-away), and the main window's focus coming back
	# lands AFTER that press — so a popover re-opened in the same frame is hidden again by the very
	# click that opened it. One frame later it opens and stays.
	if Engine.get_process_frames() == _closed_frame and _host != null and _host.is_inside_tree():
		await _host.get_tree().process_frame
	_list_band_id = TradeLedger.band_id_of(_band)
	_mount(kind, _band)

## Re-mount the open list against a fresh `band` (a snapshot, a re-render) and re-anchor it under the
## rebuilt row; a different band closes it.
func sync_list(band: Dictionary) -> void:
	if not is_list_open():
		return
	if TradeLedger.band_id_of(band) != _list_band_id:
		dismiss()
		return
	_mount(_open_kind, band)

## Take the popover and the hover card down — a band switch, the tab leaving the screen, the panel
## hiding.
func dismiss() -> void:
	_hide_hover()
	if _popover != null and is_instance_valid(_popover) and _popover.visible:
		_popover.hide()
	_open_kind = ""
	_list_band_id = HudConst.NO_BAND_ID

func is_list_open() -> bool:
	return _popover != null and is_instance_valid(_popover) and _popover.visible and _open_kind != ""

func _mount(kind: String, band: Dictionary) -> void:
	var rows: Array[Control] = []
	var title := ""
	var count := ""
	var summary := ""
	if kind == KIND_CAMPS or kind.begins_with(KIND_CAMPS + KIND_SCOPE_SEPARATOR):
		var members := _members(band)
		var camps := TradeLedger.camp_rows(band, members, _band_labor.grid_width(),
			_band_labor.wrap_horizontal())
		count = HudTradeVocab.CAMPS_COUNT_FORMAT % camps.size()
		var scope := kind.substr(KIND_CAMPS.length() + KIND_SCOPE_SEPARATOR.length()) \
			if kind != KIND_CAMPS else ""
		if scope == "":
			title = HudTradeVocab.CAMPS_TITLE
			for camp in camps:
				rows.append(_build_camp_row(camp))
		else:
			var scoped := TradeLedger.good_across_network(band, scope, camps)
			title = HudTradeVocab.GOOD_SCOPE_TITLE_FORMAT % TradeLedger.commodity_label(scope)
			summary = HudTradeVocab.GOOD_SCOPE_SUMMARY_FORMAT % [int(scoped[TradeLedger.SCOPED_MOVED]),
				int(scoped[TradeLedger.SCOPED_EVEN])]
			for camp in scoped[TradeLedger.SCOPED_ROWS]:
				rows.append(_build_scoped_camp_row(camp))
			rows.append(_build_lost_row(float(scoped[TradeLedger.SCOPED_LOST])))
	elif kind == KIND_LOCAL:
		var goods := TradeLedger.goods_net(band)
		title = HudTradeVocab.LOCAL_TITLE
		count = _goods_count_text(goods.size())
		for good in goods:
			rows.append(_build_good_row(band, good))
	else:
		var directions: Array[int] = []
		match kind:
			KIND_ROUTE_IN:
				directions = [HudTradeVocab.DIRECTION_IN]
				title = HudTradeVocab.ROUTE_TITLE_IN
			KIND_ROUTE_OUT:
				directions = [HudTradeVocab.DIRECTION_OUT]
				title = HudTradeVocab.ROUTE_TITLE_OUT
			_:
				directions = [HudTradeVocab.DIRECTION_IN, HudTradeVocab.DIRECTION_OUT]
				title = HudTradeVocab.ROUTE_TITLE_BOTH
		var total := 0
		for direction in directions:
			var half := TradeLedger.shipments_in_direction(band, direction)
			total += half.size()
			if half.is_empty():
				continue
			if directions.size() > 1:
				rows.append(_build_direction_head(direction, half.size()))
			for shipment in half:
				var head := _row_shell(HudTradeVocab.DIRECTION_INDENT, "")
				head.set_meta(ROW_SHIPMENT_META, TradeLedger.counterparty_label(shipment))
				var head_row: HBoxContainer = head.get_child(0)
				_add_counterparty(head_row, band, shipment)
				rows.append(head)
				rows.append_array(_cargo_lines(shipment, HudTradeVocab.SHIPMENT_INDENT))
		count = _shipment_count_text(total)
	_open_kind = kind
	_popover_title.text = title
	_popover_count.text = count
	_popover_sub.text = summary
	_popover_sub.visible = summary != ""
	HudWidgets.clear_children(_popover_body)
	for row in rows:
		_popover_body.add_child(row)
	_place()
	# …and again once the rows have laid out: the first placement measured them unsorted.
	_place_after_layout()

## An unscoped camp: `Barrowmere ····· ┄┄ trail  4 tiles`, a relay `Alderfen ····· via Barrowmere  6 tiles`.
func _build_camp_row(camp: Dictionary) -> Control:
	var shell := _row_shell(0, "")
	var row: HBoxContainer = shell.get_child(0)
	var is_self := bool(camp[TradeLedger.CAMP_IS_SELF])
	row.add_child(_row_label(String(camp[TradeLedger.CAMP_NAME]), HudStyle.SIGNAL if is_self else HudStyle.INK))
	row.add_child(_spacer())
	if is_self:
		row.add_child(_faint_label(HudTradeVocab.THIS_BAND_WORD))
		return shell
	var link: Control
	if bool(camp[TradeLedger.CAMP_DIRECT]):
		link = _build_rung_mark(String(camp[TradeLedger.CAMP_RUNG_ID]), true)
	else:
		link = _faint_label(HudTradeVocab.VIA_FORMAT % String(camp[TradeLedger.CAMP_VIA]))
	# Two fixed columns, so the rungs and the distances line up down the list.
	link.custom_minimum_size.x = HudTradeVocab.CAMP_LINK_COLUMN_WIDTH
	row.add_child(link)
	var distance := _faint_label(HudTradeVocab.DISTANCE_FORMAT % int(camp[TradeLedger.CAMP_DISTANCE]))
	distance.custom_minimum_size.x = HudTradeVocab.CAMP_DISTANCE_COLUMN_WIDTH
	distance.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	row.add_child(distance)
	return shell

## A scoped camp: `Barrowmere ····· ┄┄  ▼ 3.4`, the band itself first, `even` last.
func _build_scoped_camp_row(camp: Dictionary) -> Control:
	var shell := _row_shell(0, "")
	var row: HBoxContainer = shell.get_child(0)
	var is_self := bool(camp[TradeLedger.CAMP_IS_SELF])
	row.add_child(_row_label(String(camp[TradeLedger.CAMP_NAME]), HudStyle.SIGNAL if is_self else HudStyle.INK))
	row.add_child(_spacer())
	if not is_self and bool(camp[TradeLedger.CAMP_DIRECT]):
		row.add_child(_build_rung_mark(String(camp[TradeLedger.CAMP_RUNG_ID]), false))
	row.add_child(_net_label(float(camp[TradeLedger.SCOPED_NET])))
	return shell

## The friction row — what the distance ate, the one figure on the tab nobody received.
func _build_lost_row(lost: float) -> Control:
	var shell := _row_shell(0, "")
	var row: HBoxContainer = shell.get_child(0)
	row.add_child(_faint_label(HudTradeVocab.LOST_IN_TRANSIT))
	row.add_child(_spacer())
	row.add_child(_row_label("%.1f" % lost, HudStyle.INK_FAINT))
	return shell

## A link's rung as icon + word, hovering to what it is, how far it holds a link and its friction —
## read off the PUBLISHED rung table, never a copy. `""` is open ground.
func _build_rung_mark(rung_id: String, with_word: bool) -> Control:
	var mark := HBoxContainer.new()
	mark.add_theme_constant_override("separation", HudTradeVocab.CHIP_SEPARATION)
	mark.mouse_filter = Control.MOUSE_FILTER_PASS
	mark.add_child(RungLinkIcon.make(rung_id))
	var word := HudTradeVocab.OPEN_GROUND_WORD
	var tip := HudTradeVocab.OPEN_GROUND_TIP
	if rung_id != HudTradeVocab.OPEN_GROUND_RUNG:
		var entry := TradeLedger.rung_entry(rung_id, _topbar.route_rungs() if _topbar != null else [])
		word = HudRouteVocab.catalog_display_name(entry).to_lower() if not entry.is_empty() \
			else TradeLedger.rung_name(rung_id)
		if entry.is_empty():
			tip = word
		else:
			tip = "\n".join([HudRouteVocab.catalog_display_name(entry),
				HudTradeVocab.RUNG_HOLDS_FORMAT % HudRouteVocab.catalog_link_span(entry),
				HudTradeVocab.RUNG_FRICTION_FORMAT % HudRouteVocab.catalog_friction(entry)])
	mark.tooltip_text = tip
	if with_word:
		mark.add_child(_faint_label(word))
	return mark

# ---- HELPERS ---------------------------------------------------------------------------------------

func _members(band: Dictionary) -> Array[Dictionary]:
	return TradeLedger.network_members(band, _band_labor.player_bands())

func _own_faction(band: Dictionary) -> int:
	return int(band.get("faction", HudConst.PLAYER_FACTION_ID))

func _goods_count_text(count: int) -> String:
	if count == 0:
		return HudTradeVocab.NONE_WORD
	return (HudTradeVocab.GOOD_SINGULAR_FORMAT if count == 1 else HudTradeVocab.GOOD_PLURAL_FORMAT) % count

func _shipment_count_text(count: int) -> String:
	if count == 0:
		return HudTradeVocab.NONE_WORD
	return (HudTradeVocab.SHIPMENT_SINGULAR_FORMAT if count == 1
		else HudTradeVocab.SHIPMENT_PLURAL_FORMAT) % count

## The popover: a `PopupPanel` parented on the HUD like the disclosure popover, so it is a WINDOW and
## changes no zone's height. A head (title, count), an optional summary line, then the rows in its own
## `ScrollContainer` — the one scroll the Trade list may have, since a window reserves nothing.
func _ensure_popover() -> PopupPanel:
	if _popover != null and is_instance_valid(_popover):
		return _popover
	if _host == null:
		return null
	var popover := PopupPanel.new()
	popover.name = HudTradeVocab.POPOVER_NAME
	popover.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	var margin := MarginContainer.new()
	for side in HudTradeVocab.POPOVER_MARGIN_SIDES:
		margin.add_theme_constant_override("margin_%s" % side, HudTradeVocab.POPOVER_PADDING)
	popover.add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	margin.add_child(column)
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", HudTradeVocab.ROW_SEPARATION)
	column.add_child(head)
	_popover_title = _row_label("", HudStyle.INK)
	_popover_title.add_theme_font_size_override("font_size", HudTradeVocab.TITLE_FONT_SIZE)
	head.add_child(_popover_title)
	_popover_count = _faint_label("")
	head.add_child(_popover_count)
	_popover_sub = _faint_label("")
	column.add_child(_popover_sub)
	_popover_scroll = ScrollContainer.new()
	_popover_scroll.name = HudTradeVocab.POPOVER_SCROLL_NAME
	_popover_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_popover_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_popover_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_popover_scroll)
	_popover_body = _rows_column()
	_popover_body.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	_popover_scroll.add_child(_popover_body)
	popover.popup_hide.connect(_on_popover_hidden)
	_host.add_child(popover)
	_popover = popover
	return popover

func _on_popover_hidden() -> void:
	# A hide that lands after the popover was already re-opened on another list is stale.
	if _popover != null and _popover.visible:
		return
	_hide_hover()
	_closed_frame = Engine.get_process_frames()
	_closed_kind = _open_kind
	_open_kind = ""

func _in_popover(control: Control) -> bool:
	return _popover != null and is_instance_valid(_popover) and _popover.is_ancestor_of(control)

## **WHERE THE POPOVER SITS**, measured off the live rects every time and never a hard-coded height.
## Its left edge and width are the Trade column's; its top is the anchor row's bottom plus
## `POPOVER_GAP`, when the whole list fits in the room from there to the bottom of the visible screen.
## When it does not, it opens on whichever side of the row has MORE room — above, for a bottom dock's
## row near the screen edge with a long list — capped to that room and scrolling past it. All in SCREEN space, the
## disclosure popover's `get_screen_transform` math, which folds in the window and the canvas stretch.
func _place() -> void:
	if _popover == null or not is_instance_valid(_popover):
		return
	var anchor := _anchor_row()
	if anchor == null or _column == null or not is_instance_valid(_column) or not _column.is_inside_tree():
		return
	_anchor_screen = _screen_rect(anchor)
	var column := _screen_rect(_column)
	var viewport := anchor.get_viewport()
	var visible: Rect2 = viewport.get_screen_transform() * viewport.get_visible_rect()
	var content := _popover_content_height()
	var room_below := visible.end.y - _anchor_screen.end.y - HudTradeVocab.POPOVER_GAP \
		- HudTradeVocab.POPOVER_EDGE_MARGIN
	var room_above := _anchor_screen.position.y - HudTradeVocab.POPOVER_GAP - visible.position.y \
		- HudTradeVocab.POPOVER_EDGE_MARGIN
	_opened_above = content > room_below and room_above > room_below
	var height := minf(content, room_above if _opened_above else room_below)
	var top := _anchor_screen.position.y - HudTradeVocab.POPOVER_GAP - height if _opened_above \
		else _anchor_screen.end.y + HudTradeVocab.POPOVER_GAP
	# The CARD goes at (column.x, top) and is (column width × height); the window around it is grown by
	# the shadow insets so the drawn card, not the window, lines up with the Trade column.
	var inset := _shadow_insets()
	var rect := Rect2i(Vector2i(roundi(column.position.x - inset[0]), roundi(top - inset[1])),
		Vector2i(roundi(column.size.x + inset[0] + inset[2]), roundi(height + inset[1] + inset[3])))
	if _popover.visible:
		_popover.position = rect.position
		_popover.size = rect.size
	else:
		_popover.popup(rect)

func _place_after_layout() -> void:
	if _host == null or not _host.is_inside_tree():
		return
	await _host.get_tree().process_frame
	if is_list_open():
		_place()

## The popover's full content height: everything around the rows plus the rows' own height. **Once
## it has laid out, "everything around the rows" is MEASURED** — the popover's height less its scroll
## viewport's — since the window adds chrome of its own that no stylebox reports; before that, the
## first placement estimates it from the card stylebox and the head (the scroll reports none of the
## rows while it can scroll), and the placement a frame later corrects it.
func _popover_content_height() -> float:
	var rows := _popover_body.get_combined_minimum_size().y
	if _popover.visible and _popover_scroll.size.y > 0.0:
		var inset := _shadow_insets()
		return float(_popover.size.y) - inset[1] - inset[3] - _popover_scroll.size.y + rows
	var chrome := HudStyle.card_stylebox().get_minimum_size().y
	var margin: Control = _popover.get_child(0)
	return chrome + margin.get_combined_minimum_size().y + rows

## The row the popover hangs from — the zone's row that opens the anchored kind. Looked up afresh
## each placement, because every render rebuilds the zone and frees the row it was opened from.
func _anchor_row() -> Control:
	if _column == null or not is_instance_valid(_column):
		return null
	return _find_opens(_column, _anchor_kind)

func _find_opens(node: Node, kind: String) -> Control:
	if node.has_meta(ROW_OPENS_META) and String(node.get_meta(ROW_OPENS_META)) == kind:
		# The network line's camps link names the camps list; the popover hangs from the LINE.
		if node is Button:
			var line := node.get_parent().get_parent()
			if line is PanelContainer:
				return line as Control
		return node as Control
	for child in node.get_children():
		var found := _find_opens(child, kind)
		if found != null:
			return found
	return null

## A control's rect in SCREEN space.
func _screen_rect(control: Control) -> Rect2:
	return control.get_screen_transform() * Rect2(Vector2.ZERO, control.size)

## A SCREEN rect in `on`'s canvas space — how the hover card, on a canvas layer of the main window,
## places itself beside a row that may live in the popover's own window.
func _screen_to_layer(on: Control, rect: Rect2) -> Rect2:
	return on.get_viewport().get_screen_transform().affine_inverse() * rect

func _ensure_hover() -> TradeHoverCard:
	if _hover != null and is_instance_valid(_hover):
		return _hover
	var layer := _layer()
	if layer == null:
		return null
	_hover = TradeHoverCard.new()
	layer.add_child(_hover)
	return _hover

## The layer the hover card lives on — the work inspector's, one above the event dock
## (`HudLayer.WORK_INSPECTOR_LAYER_INDEX`): it takes part in no zone's layout, and it sits over the bar.
func _layer() -> Node:
	if _host == null or not _host.has_method("work_inspector_host"):
		return null
	return _host.work_inspector_host()
