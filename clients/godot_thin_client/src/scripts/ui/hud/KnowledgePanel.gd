extends AutoSizingPanel
class_name KnowledgePanel

## **THE KNOWLEDGE SCREEN** (`docs/plan_knowledge_screen.md` §3) — what your people know, what they
## are learning, and what they have earned and are not using. Its own free-floating surface, launched
## from the Band/City panel header's action bar beside the `⚒`.
##
## **NOTHING HERE IS CLICKABLE IN THE TECH-TREE SENSE.** No queue, no research order, no pathing, no
## "next" button. A discovery is earned by PRACTICE — you get Penning by keeping tamed herds, not by
## spending a currency — so a screen that offered a plan would teach the exact opposite of how the
## game works. Selecting a node opens a READING of it and nothing else. If it reads as a planner it
## has taught the wrong thing, and that is the one review question to ask of any change here.
##
## **DOMAINS ARE COLUMNS AND IT IS NOT A GRAPH.** The rung engine models ~4 steps per web and grows by
## adding BRANCHES, so the screen never needs pan, zoom or edge routing — and a graph view would spend
## its whole budget drawing eight nodes' worth of empty space. A LADDER domain draws a rail down its
## left edge (its nodes are ordered: each is earned by practising the one below); the CRAFT column
## draws none, because a craft is learned by working its material and gates recipes rather than a next
## step. **That is a property of the domain descriptor, not a branch in this renderer.**
##
## **WHAT EXISTS IS RENDERED, AND NOTHING ELSE.** The prototype
## (`docs/knowledge_screen_ux_proposal.html`) shows 36 nodes to prove the layout survives the tree it
## will one day have; the game has EIGHT. Routes / War / Telling have no nodes, so they have no
## columns — see `KnowledgeRoster.build_domains`, and never draw an empty domain column.
##
## **A TRACK AT `0.0` IS DRAWN, GREYED.** See `KnowledgeRoster`'s docstring for why the old skip was
## the bug rather than the economy.
##
## **THE FILTERS DIM, THEY DO NOT HIDE.** The shape of the tree — two short ladders and a fan — is most
## of what this screen teaches, and a filter that removed rows would take that away every time it was
## used. So a non-matching node keeps its place at `FILTERED_OUT_ALPHA`, and the pill's count and the
## dimming are ONE predicate (`KnowledgeRoster.matches`), because a separate count and a separate dim
## both look right on their own while disagreeing.
##
## **THIS IS THE FREE-FLOATING CASE, hence `AutoSizingPanel`**
## (`.claude/rules/client/panel-framework.md`): the card is measured against the ROOM — the viewport
## MINUS every reserved edge strip — rather than against a dock's remaining height, so
## `PanelCard` + `DockScrollFit` is the wrong half of the pair and would misbehave silently. Both axes
## are fitted explicitly because this node is a plain `Control` and no child minimum ever reaches it.
##
## The words, the domain descriptors and the measured geometry live in `HudKnowledgeVocab`; the
## derivation lives in `KnowledgeRoster`; this file holds no const block of its own beyond the payload
## contract below.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

## The ✕ was pressed.
signal closed
## A node was selected — its key. The controller does nothing with it but re-render, which is what
## keeps the selection a payload field rather than a second source of truth inside the panel.
signal node_selected(key: String)
## A filter pill was pressed — its key.
signal filter_selected(key: StringName)

# ---- the render payload's keys (this panel's contract with its controller) ----------------------
## `[{key, label, shape, nodes}]` — `KnowledgeRoster.build_domains`' answer, already pruned of empty
## domains.
const PAYLOAD_DOMAINS := "domains"
## The selected node's key, `""` when nothing is selected (the placeholder detail pane).
const PAYLOAD_SELECTED := "selected"
## The live filter's key. `FILTER_ALL` dims nothing.
const PAYLOAD_FILTER := "filter"

var _card: PanelContainer = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
var _header: VBoxContainer = null
var _columns: HBoxContainer = null
var _detail: VBoxContainer = null
var _fit_pending: bool = false

## The last payload rendered, so a re-fit after a viewport change has something to measure.
var _payload: Dictionary = {}

func _ready() -> void:
	super()
	name = "KnowledgePanel"
	# The panel eats its own clicks and only its own: a press on a node must never also select the hex
	# behind it, and a press one pixel outside must still reach `MapView._unhandled_input`.
	mouse_filter = Control.MOUSE_FILTER_STOP
	target_width = HudKnowledgeVocab.PANEL_WIDTH
	min_height = HudKnowledgeVocab.PANEL_MIN_HEIGHT
	bottom_margin = HudKnowledgeVocab.VIEWPORT_MARGIN
	# `_place()` CENTRES this card in its room, so the height fit's ceiling is the room's whole height
	# and is taken off the room rect — the card is never moved in order to be measured. See `refit`.
	centred_in_room = true
	visible = false

	_card = PanelContainer.new()
	_card.name = "KnowledgeCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	add_child(_card)

	var column := VBoxContainer.new()
	column.name = "KnowledgeColumn"
	column.add_theme_constant_override("separation", 0)
	_card.add_child(column)

	_header = VBoxContainer.new()
	_header.name = "KnowledgeHeader"
	_header.add_theme_constant_override("separation", HudKnowledgeVocab.FILTER_ROW_SEPARATION)
	column.add_child(_wrap_padded(_header,
		HudKnowledgeVocab.HEADER_PADDING_H, HudKnowledgeVocab.HEADER_PADDING_V))

	column.add_child(_rule(HudStyle.LINE))

	# ONE scroll around the whole body, for `CraftingPanel`'s reason: this card is measured against
	# the viewport, so its ceiling is real room — and a short window genuinely can leave less of it
	# than three columns of nodes need.
	_scroll = ScrollContainer.new()
	_scroll.name = "KnowledgeScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "KnowledgeBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_body)

	var zones := HBoxContainer.new()
	zones.name = "KnowledgeZones"
	zones.add_theme_constant_override("separation", 0)
	zones.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(zones)

	_columns = HBoxContainer.new()
	_columns.name = "DomainColumns"
	_columns.add_theme_constant_override("separation", HudKnowledgeVocab.COLUMN_SEPARATION)
	_columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var columns_host := _wrap_padded(_columns,
		HudKnowledgeVocab.COLUMNS_PADDING_H, HudKnowledgeVocab.COLUMNS_PADDING_V)
	columns_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	zones.add_child(columns_host)

	var seam := Panel.new()
	seam.custom_minimum_size = Vector2(HudKnowledgeVocab.COLUMN_SEPARATOR_THICKNESS, 0.0)
	seam.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	zones.add_child(seam)

	_detail = VBoxContainer.new()
	_detail.name = "KnowledgeDetail"
	_detail.custom_minimum_size = Vector2(HudKnowledgeVocab.DETAIL_WIDTH, 0.0)
	_detail.size_flags_horizontal = Control.SIZE_FILL
	_detail.add_theme_constant_override("separation", HudKnowledgeVocab.DETAIL_SECTION_SEPARATION)
	zones.add_child(_wrap_padded(_detail,
		HudKnowledgeVocab.DETAIL_PADDING_H, HudKnowledgeVocab.DETAIL_PADDING_V))

# ---- public API -------------------------------------------------------------

## Rebuild the whole panel against `payload` (see the `PAYLOAD_*` keys) and show it.
##
## **NO SCROLL OFFSET IS CARRIED**, unlike the crafting ledger's. This body is three short columns and
## a reading — it does not scroll at any ordinary window size — so there is no place for a player to
## be scrolled to that a rebuild could cost them.
func render(payload: Dictionary) -> void:
	_payload = payload
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_columns)
	HudWidgets.clear_children(_detail)
	var nodes := KnowledgeRoster.flatten(payload.get(PAYLOAD_DOMAINS, []))
	_build_header(payload, nodes)
	_build_columns(payload)
	_build_detail(payload, nodes)
	# **VISIBLE BEFORE THE FIT, and that is load-bearing**: `Container._sort_children` early-returns on
	# a hidden subtree, so a card kept hidden until it had been measured would never lay its content
	# out and would measure the unwrapped lower bound forever.
	visible = true
	# …and on its FIRST mount it shows at its NOMINAL width, so the height read a frame from now is a
	# function of the width the content was actually laid out at. A card that has already been fitted
	# is left where it is — snapping it back to the nominal would draw one whole frame at a width the
	# card is about to leave.
	if not has_fitted_width():
		fit_width(0.0, 0.0)
	refit()

func dismiss() -> void:
	visible = false
	_payload = {}
	if _scroll != null:
		_scroll.scroll_vertical = 0
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_columns)
	HudWidgets.clear_children(_detail)

func is_open() -> bool:
	return visible

## The `PanelContainer` that DRAWS the card. A real Container, so its combined minimum is the honest
## measure of whether the card is holding its content or quietly growing out of itself.
func card() -> PanelContainer:
	return _card

## Re-fit to content and re-place. Coalesced across one frame: the content's height is a function of
## the card's width, so a measurement taken in the same frame the body was rebuilt reports the
## PREVIOUS content's wrapping. `CraftingPanel.refit`'s contract, for its reasons.
func refit() -> void:
	if not visible or _fit_pending or _body == null:
		return
	_fit_pending = true
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	var room := _room()
	var chrome := HudStyle.card_stylebox().get_minimum_size()
	max_width = maxf(room.size.x, target_width)
	fit_width(_body.get_combined_minimum_size().x, chrome.x + _scroll_gutter())
	# The height fit's ceiling is the WHOLE room and the card does not move to be measured —
	# `centred_in_room` is how the base class is told so. Fitting a centred card against the room
	# BELOW it throws away everything above it.
	max_height = room.size.y
	fit_to_content(_body.get_combined_minimum_size().y + _header_height(), chrome.y, _scroll)
	_place()

# ---- header: the title, the tally, the filter pills -------------------------

func _build_header(payload: Dictionary, nodes: Array) -> void:
	var title_row := HBoxContainer.new()
	title_row.add_theme_constant_override("separation", HudKnowledgeVocab.HEADER_SEPARATION)
	var title := Label.new()
	title.text = HudKnowledgeVocab.PANEL_TITLE.to_upper()
	title.add_theme_font_size_override("font_size", HudKnowledgeVocab.TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	title_row.add_child(title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	title_row.add_child(spacer)

	var tally := Label.new()
	tally.text = _tally_text(nodes)
	tally.set_meta(HudKnowledgeVocab.TALLY_META, true)
	tally.add_theme_font_size_override("font_size", HudKnowledgeVocab.TALLY_FONT_SIZE)
	tally.add_theme_color_override("font_color", HudStyle.INK_DIM)
	title_row.add_child(tally)

	var close := Button.new()
	close.text = HudKnowledgeVocab.CLOSE_GLYPH
	close.tooltip_text = HudKnowledgeVocab.CLOSE_TOOLTIP
	close.focus_mode = Control.FOCUS_NONE
	HudStyle.apply_button(close, "ghost")
	close.pressed.connect(func(): closed.emit())
	title_row.add_child(close)
	_header.add_child(title_row)

	var filter_row := HBoxContainer.new()
	filter_row.add_theme_constant_override("separation", HudKnowledgeVocab.FILTER_ROW_SEPARATION)
	filter_row.add_child(_caption(HudKnowledgeVocab.FILTER_ROW_LABEL, HudStyle.INK_FAINT,
		HudKnowledgeVocab.FILTER_FONT_SIZE))
	var live := StringName(payload.get(PAYLOAD_FILTER, HudKnowledgeVocab.FILTER_ALL))
	for spec in HudKnowledgeVocab.FILTERS:
		var key := StringName(spec[HudKnowledgeVocab.FILTER_SPEC_KEY])
		var pill := Button.new()
		pill.text = HudKnowledgeVocab.FILTER_PILL_FORMAT % [
			String(spec[HudKnowledgeVocab.FILTER_SPEC_LABEL]),
			KnowledgeRoster.count_matching(nodes, key)]
		pill.focus_mode = Control.FOCUS_NONE
		pill.set_meta(HudKnowledgeVocab.FILTER_META, String(key))
		pill.add_theme_font_size_override("font_size", HudKnowledgeVocab.FILTER_FONT_SIZE)
		HudStyle.apply_pill_toggle(pill, key == live)
		pill.pressed.connect(func(): filter_selected.emit(key))
		filter_row.add_child(pill)
	_header.add_child(filter_row)

## **THE TALLY IS THREE STATES PLUS THE NUDGE.** `unspent` rides last and in `WARN` because it is the
## only one of the four that is asking for something rather than reporting.
func _tally_text(nodes: Array) -> String:
	var counts := KnowledgeRoster.tally(nodes)
	var parts: Array[String] = [
		HudKnowledgeVocab.TALLY_KNOWN_FORMAT % int(counts[HudKnowledgeVocab.NODE_STATE_KNOWN]),
		HudKnowledgeVocab.TALLY_LEARNING_FORMAT % int(counts[HudKnowledgeVocab.NODE_STATE_LEARNING]),
		HudKnowledgeVocab.TALLY_NOT_BEGUN_FORMAT % int(counts[HudKnowledgeVocab.NODE_STATE_NOT_BEGUN]),
	]
	# The unspent clause appears only when there IS one. A standing `0 unspent` is a nudge about
	# nothing, and it is beside three real readings where it would read as a fourth.
	var unspent := int(counts[KnowledgeRoster.TALLY_UNSPENT])
	if unspent > 0:
		parts.append(HudKnowledgeVocab.TALLY_UNSPENT_FORMAT % unspent)
	return HudKnowledgeVocab.TALLY_SEPARATOR.join(parts)

# ---- the domain columns -----------------------------------------------------

func _build_columns(payload: Dictionary) -> void:
	var filter := StringName(payload.get(PAYLOAD_FILTER, HudKnowledgeVocab.FILTER_ALL))
	var selected := String(payload.get(PAYLOAD_SELECTED, ""))
	for domain_variant in payload.get(PAYLOAD_DOMAINS, []):
		if domain_variant is Dictionary:
			_columns.add_child(_build_domain(domain_variant as Dictionary, filter, selected))

func _build_domain(domain: Dictionary, filter: StringName, selected: String) -> Control:
	var col := VBoxContainer.new()
	col.custom_minimum_size = Vector2(HudKnowledgeVocab.COLUMN_MIN_WIDTH, 0.0)
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", HudKnowledgeVocab.DOMAIN_SEPARATION)
	col.set_meta(HudKnowledgeVocab.DOMAIN_META, String(domain[HudKnowledgeVocab.DOMAIN_KEY]))

	var head := Label.new()
	head.text = String(domain[HudKnowledgeVocab.DOMAIN_LABEL]).to_upper()
	head.add_theme_font_size_override("font_size", HudKnowledgeVocab.DOMAIN_HEAD_FONT_SIZE)
	head.add_theme_color_override("font_color", HudStyle.INK_DIM)
	col.add_child(head)

	var shape := String(domain[HudKnowledgeVocab.DOMAIN_SHAPE])
	col.add_child(_caption(String(HudKnowledgeVocab.DOMAIN_SHAPE_NOTES.get(shape, "")),
		HudStyle.INK_FAINT, HudKnowledgeVocab.DOMAIN_SHAPE_FONT_SIZE))

	# **THE RAIL IS THE DOMAIN'S SHAPE, DRAWN.** A ladder's nodes are ordered — each earned by
	# practising the one below — and the rail is what says so; the craft fan has no order to state, so
	# it draws none and its rows sit at the column's own left edge. One `if` on the DESCRIPTOR, never
	# on a domain's name.
	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 0)
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for node_variant in domain[HudKnowledgeVocab.DOMAIN_NODES]:
		if node_variant is Dictionary:
			rows.add_child(_build_node_row(node_variant as Dictionary, filter, selected))
	if shape == HudKnowledgeVocab.DOMAIN_SHAPE_LADDER:
		col.add_child(_with_rail(rows))
	else:
		col.add_child(rows)
	return col

## The vertical hairline plus its gutter, with the rows to its right.
func _with_rail(rows: Control) -> Control:
	var host := HBoxContainer.new()
	host.add_theme_constant_override("separation", HudKnowledgeVocab.RAIL_GUTTER)
	host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var rail := Panel.new()
	rail.custom_minimum_size = Vector2(HudKnowledgeVocab.RAIL_THICKNESS, 0.0)
	rail.size_flags_vertical = Control.SIZE_EXPAND_FILL
	rail.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	rail.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rail.set_meta(HudKnowledgeVocab.RAIL_META, true)
	host.add_child(rail)
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	host.add_child(rows)
	return host

## ONE NODE'S ROW: `glyph name ……… value`, with the unspent clause under it when it has one.
##
## **A `PanelContainer` WITH `gui_input`, NOT A `Button`, and that is the panel's own documented
## rule.** A Button is not a Container, so a `glyph + name + value` row parented to one is NEVER LAID
## OUT — the children pile up at the origin and the row's height stops being a function of its
## content — and a `flat` Button ignores its `normal` stylebox outright, so the SELECTION would have
## been an override reaching nothing the widget draws. Both were shipped here first and both are
## invisible to a bounds assertion: the row renders, at the wrong height, with no selected state.
## `BandCityPanel._make_tab_button` records the same finding for the same reason.
##
## **PRESSING IT IS A READING, not a queue.** There is nothing to order and nothing to spend — see the
## class docstring. It carries `NODE_META` so a harness finds it by the node it IS rather than by
## whatever text it happens to be showing.
func _build_node_row(node: Dictionary, filter: StringName, selected: String) -> Control:
	var key := String(node[HudKnowledgeVocab.NODE_KEY])
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	var ink: Color = HudKnowledgeVocab.NODE_INKS.get(state, HudStyle.INK)
	var is_selected := key == selected

	var host := VBoxContainer.new()
	host.add_theme_constant_override("separation", 0)
	host.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var row := PanelContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_STOP
	row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	row.tooltip_text = String(node.get(HudKnowledgeVocab.NODE_NOTE, ""))
	row.set_meta(HudKnowledgeVocab.NODE_META, key)
	row.add_theme_stylebox_override("panel", _node_row_stylebox(is_selected))
	row.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
				and event.pressed:
			node_selected.emit(key))

	# A mouse-transparent row inside the panel, so the glyph, the name and the value read (and click)
	# as ONE row.
	var face := HBoxContainer.new()
	face.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_theme_constant_override("separation", HudKnowledgeVocab.NODE_ROW_SEPARATION)
	row.add_child(face)

	var glyph := Label.new()
	glyph.text = String(HudKnowledgeVocab.NODE_GLYPHS.get(state, ""))
	glyph.add_theme_font_size_override("font_size", HudKnowledgeVocab.NODE_NAME_FONT_SIZE)
	glyph.add_theme_color_override("font_color", ink)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_child(glyph)

	var name_label := Label.new()
	name_label.text = String(node[HudKnowledgeVocab.NODE_LABEL])
	name_label.add_theme_font_size_override("font_size", HudKnowledgeVocab.NODE_NAME_FONT_SIZE)
	name_label.add_theme_color_override("font_color", ink)
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_child(name_label)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_child(spacer)

	var value := Label.new()
	value.text = _node_value_text(node)
	value.add_theme_font_size_override("font_size", HudKnowledgeVocab.NODE_VALUE_FONT_SIZE)
	value.add_theme_color_override("font_color", HudStyle.INK_FAINT \
		if state == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN else HudStyle.INK_DIM)
	value.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_child(value)
	host.add_child(row)

	# **THE UNSPENT CLAUSE IS ON THE ROW, not only in the pane.** The whole point of the state is that
	# the player has not noticed it, so it has to be legible without a click. `WARN`, the same tint the
	# tally's unspent clause takes.
	if bool(node.get(HudKnowledgeVocab.NODE_UNSPENT, false)):
		var clause := _caption("%s %s" % [HudKnowledgeVocab.UNSPENT_MARK,
			HudKnowledgeVocab.UNSPENT_CLAUSE], HudStyle.WARN,
			HudKnowledgeVocab.NODE_CLAUSE_FONT_SIZE)
		# Indented onto the NAME's own column, so it reads as a note about this row rather than as a
		# row of its own. The glyph's width plus the face's separation, derived rather than measured.
		var clause_host := MarginContainer.new()
		clause_host.add_theme_constant_override("margin_left",
			HudKnowledgeVocab.NODE_CLAUSE_INDENT)
		clause_host.add_child(clause)
		host.add_child(clause_host)

	# **DIM, NEVER HIDE** — see the class docstring. `modulate` on the whole row host so the glyph, the
	# name, the value AND the clause fade together; a per-Label tint would leave the clause bright over
	# a faded name.
	if not KnowledgeRoster.matches(node, filter):
		host.modulate = Color(1.0, 1.0, 1.0, HudKnowledgeVocab.FILTERED_OUT_ALPHA)
	return host

## The row's own box. **Transparent either way — a node is text, not a control — and the SELECTED one
## wears a `SIGNAL` bar down its leading edge** plus the faint wash the rest of this HUD gives a live
## selection. Identical content margins in both states, so selecting a row never moves the column;
## that is `BandCityPanel._tab_stylebox`'s rule, and it is what makes the stylebox the honest carrier
## of the state (a `flat` Button's `normal` override draws nothing at all).
func _node_row_stylebox(selected: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.SIGNAL_WASH if selected else Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = HudKnowledgeVocab.NODE_ROW_PADDING_H
	sb.content_margin_right = HudKnowledgeVocab.NODE_ROW_PADDING_H
	sb.content_margin_top = HudKnowledgeVocab.NODE_ROW_PADDING_V
	sb.content_margin_bottom = HudKnowledgeVocab.NODE_ROW_PADDING_V
	if selected:
		sb.border_width_left = HudKnowledgeVocab.NODE_SELECTED_BAR_THICKNESS
		sb.border_color = HudStyle.SIGNAL
	return sb

## `known` · `▓▓▒░░ 62%` · `not begun`. The meter is `HudFormat.meter_bar`'s block glyphs, at
## `FactionReadouts.KNOWLEDGE_METER_CELLS`, so this screen and the faction page draw one track at one
## resolution.
##
## **THE SCALE CONVERSION IS THE POINT OF THE LEARNING BRANCH.** `meter_bar` grades a `0..100` score
## and every node's progress is `0..1`, so a bare `progress` fills zero cells at every value under
## 0.5 — which is how the faction page's meters shipped EMPTY, indistinguishable from an unstarted
## track beside a live percent.
func _node_value_text(node: Dictionary) -> String:
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	if state == HudKnowledgeVocab.NODE_STATE_KNOWN:
		return HudKnowledgeVocab.NODE_VALUE_KNOWN
	if state == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN:
		return HudKnowledgeVocab.NODE_VALUE_NOT_BEGUN
	var progress := float(node[HudKnowledgeVocab.NODE_PROGRESS])
	return HudKnowledgeVocab.LEARNING_VALUE_FORMAT % [
		HudFormat.meter_bar(progress * HudConst.PROGRESS_PERCENT_SCALE, HudKnowledgeVocab.METER_CELLS),
		HudFormat.progress_percent(progress)]

# ---- the detail pane --------------------------------------------------------

## **A READING OF ONE NODE: what it lets you do · how it is learned · where, now.** Nothing here is a
## control. See the class docstring.
func _build_detail(payload: Dictionary, nodes: Array) -> void:
	var selected := String(payload.get(PAYLOAD_SELECTED, ""))
	var node := _find_node(nodes, selected)
	if node.is_empty():
		_detail.add_child(_detail_title(HudKnowledgeVocab.DETAIL_PLACEHOLDER_HEAD))
		_detail.add_child(_detail_body(HudKnowledgeVocab.DETAIL_PLACEHOLDER_BODY))
		_append_filter_note(payload, nodes)
		return
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	_detail.add_child(_detail_title(String(node[HudKnowledgeVocab.NODE_LABEL])))

	# The unlock copy first, because it is the answer to the question that brought the player here.
	# Read from `FactionReadouts.KNOWLEDGE_UNLOCK_NOTES` via the roster — the same sentence the unlock
	# announcement says, so the two cannot describe one discovery differently.
	var note := String(node.get(HudKnowledgeVocab.NODE_NOTE, ""))
	if note != "":
		_detail.add_child(_detail_head(HudKnowledgeVocab.DETAIL_HEAD_UNLOCKS))
		_detail.add_child(_detail_body(note))

	var practise := String(node.get(HudKnowledgeVocab.NODE_PRACTISE, ""))
	if practise != "":
		_detail.add_child(_detail_head(HudKnowledgeVocab.DETAIL_HEAD_PRACTISE))
		_detail.add_child(_detail_body(practise))

	if state == HudKnowledgeVocab.NODE_STATE_KNOWN:
		_detail.add_child(_detail_head(HudKnowledgeVocab.DETAIL_HEAD_WHERE))
		_detail.add_child(_detail_body(_where_text(node)))
	else:
		# **A NODE NOT YET LEARNED HAS NO "WHERE" AT ALL**, and saying "0 sources" about one would read
		# as a shortfall rather than as a thing not yet learned.
		_detail.add_child(_detail_head(HudKnowledgeVocab.DETAIL_NEEDS_HEAD))
		_detail.add_child(_detail_body(
			HudKnowledgeVocab.DETAIL_NEEDS_NOT_BEGUN if state == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN
			else HudKnowledgeVocab.DETAIL_NEEDS_LEARNING_FORMAT % HudFormat.progress_percent(
				float(node[HudKnowledgeVocab.NODE_PROGRESS]))))
	_append_filter_note(payload, nodes)

## What `Where, now` says. Three shapes, and the third exists because a knowledge that unlocks
## nothing has no source to stand on it — see `HudKnowledgeVocab.UNLOCKLESS_TRACKS`.
func _where_text(node: Dictionary) -> String:
	if not bool(node.get(HudKnowledgeVocab.NODE_UNSPENT_TESTABLE, false)):
		return HudKnowledgeVocab.DETAIL_WHERE_UNLOCKLESS
	var in_use := int(node.get(HudKnowledgeVocab.NODE_IN_USE_COUNT, 0))
	if String(node.get(HudKnowledgeVocab.NODE_DOMAIN, "")) == HudKnowledgeVocab.DOMAIN_KEY_CRAFT:
		return HudKnowledgeVocab.DETAIL_WHERE_CRAFT_IN_USE if in_use > 0 \
			else HudKnowledgeVocab.DETAIL_WHERE_CRAFT_UNSPENT
	if in_use <= 0:
		return HudKnowledgeVocab.DETAIL_WHERE_UNSPENT_NONE
	if in_use == 1:
		return HudKnowledgeVocab.DETAIL_WHERE_IN_USE_ONE
	return HudKnowledgeVocab.DETAIL_WHERE_IN_USE_FORMAT % in_use

## The caption a zero-match filter earns. It rides in the DETAIL pane rather than over the columns
## because the columns still show every node (dimmed) and a banner across them would read as a
## replacement for the list rather than as a note about it.
func _append_filter_note(payload: Dictionary, nodes: Array) -> void:
	var filter := StringName(payload.get(PAYLOAD_FILTER, HudKnowledgeVocab.FILTER_ALL))
	if filter == HudKnowledgeVocab.FILTER_ALL:
		return
	if KnowledgeRoster.count_matching(nodes, filter) > 0:
		return
	var clause := String(HudKnowledgeVocab.FILTER_EMPTY_CLAUSES.get(filter, ""))
	if clause == "":
		return
	var label := _caption(HudKnowledgeVocab.FILTER_EMPTY_FORMAT % clause, HudStyle.INK_FAINT,
		HudKnowledgeVocab.EMPTY_FONT_SIZE)
	label.set_meta(HudKnowledgeVocab.EMPTY_NOTE_META, String(filter))
	_detail.add_child(label)

func _find_node(nodes: Array, key: String) -> Dictionary:
	if key == "":
		return {}
	for node_variant in nodes:
		if node_variant is Dictionary \
				and String((node_variant as Dictionary).get(HudKnowledgeVocab.NODE_KEY, "")) == key:
			return node_variant as Dictionary
	return {}

# ---- leaves -----------------------------------------------------------------

func _detail_title(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", HudKnowledgeVocab.DETAIL_TITLE_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK)
	return label

func _detail_head(text: String) -> Label:
	var label := Label.new()
	label.text = text.to_upper()
	label.add_theme_font_size_override("font_size", HudKnowledgeVocab.DETAIL_HEAD_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	return label

func _detail_body(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.custom_minimum_size = Vector2(HudKnowledgeVocab.DETAIL_WIDTH \
		- 2.0 * float(HudKnowledgeVocab.DETAIL_PADDING_H), 0.0)
	label.add_theme_font_size_override("font_size", HudKnowledgeVocab.DETAIL_BODY_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	return label

func _caption(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", ink)
	return label

func _rule(color: Color) -> Control:
	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(0.0, HudKnowledgeVocab.COLUMN_SEPARATOR_THICKNESS)
	var box := StyleBoxFlat.new()
	box.bg_color = color
	rule.add_theme_stylebox_override("panel", box)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rule

func _wrap_padded(content: Control, padding_h: int, padding_v: int) -> MarginContainer:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_left", padding_h)
	host.add_theme_constant_override("margin_right", padding_h)
	host.add_theme_constant_override("margin_top", padding_v)
	host.add_theme_constant_override("margin_bottom", padding_v)
	host.add_child(content)
	return host

# ---- geometry ---------------------------------------------------------------

## The room the card may use — the viewport MINUS every reserved edge strip and every overlay, which
## the controller hands over as `room_bounds`. `CraftingPanel._room`'s contract, for its reasons: a
## card measured against the whole window grows under a docked panel's strip and under the event bar.
func _room() -> Rect2:
	return available_room(HudKnowledgeVocab.VIEWPORT_MARGIN)

func _place() -> void:
	var room := _room()
	position = Vector2(
		room.position.x + maxf((room.size.x - size.x) * 0.5, 0.0),
		room.position.y + maxf((room.size.y - size.y) * 0.5, 0.0))

func _header_height() -> float:
	if _header == null:
		return 0.0
	return _header.get_parent().get_combined_minimum_size().y \
		+ HudKnowledgeVocab.COLUMN_SEPARATOR_THICKNESS

## The room the vertical scrollbar needs, whether or not it is currently shown. Reserved
## unconditionally: the ceiling here is the VIEWPORT, so a taller or shorter window turns the internal
## scrollbar on and off, and a gutter reserved only while scrolling would jump the card's width.
func _scroll_gutter() -> float:
	if _scroll == null:
		return 0.0
	return _scroll.get_v_scroll_bar().get_combined_minimum_size().x
