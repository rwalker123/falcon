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
## **DOMAINS ARE ROWS AND IT IS NOT A GRAPH** (`docs/plan_knowledge_rows.md`). They were COLUMNS, and
## the measurement in §1 is why they are not any more: **domains are the axis that GROWS, and the
## shipped layout put them on the axis that cannot scroll.** A column carried a 210px floor and a 20px
## separation beside a 300px pinned detail pane, so the card's content minimum was
## `230 × domains + 336` — ~1,716px at today's six, and 2,636–3,096 at the ten to twelve
## `docs/plan_civilization_steps.md` commits, on a 1,920 viewport docked panels have already taken a
## bite out of.
##
## Laid out in rows, **width is a function of ladder DEPTH** — which the design caps at ~4 rungs and
## forbids growing — so one fixed card width fits every domain count, and a new branch costs one row
## of HEIGHT, on an axis that already scrolls.
##
## ⛔ **AND NO CLAMP WOULD HAVE DONE IT.** `refit` raised `max_width` to the room and `fit_width`
## clamped to it correctly — but Godot will not render a Control below its
## `get_combined_minimum_size()`, and that minimum ran straight through `KnowledgeScroll`, whose
## `horizontal_scroll_mode` was `SCROLL_MODE_DISABLED`: **a ScrollContainer that cannot scroll an axis
## propagates its child's full minimum on that axis rather than absorbing it.** The clamp was computed
## and then overruled by the layout. The content minimum itself had to come down, which is what the
## rows do — and the horizontal axis is `SCROLL_MODE_AUTO` now so the card is genuinely shrinkable.
##
## A LADDER domain draws the rail BETWEEN its rungs (its nodes are ordered: each is earned by
## practising the one below); the CRAFT fan draws none, because a craft is learned by working its
## material and gates recipes rather than a next step. **That is a property of the domain descriptor,
## not a branch in this renderer** — the same rule the column rail stated, rotated ninety degrees.
##
## **WHAT EXISTS IS RENDERED, AND NOTHING ELSE.** Routes / War / Telling have no nodes, so they have
## no rows — see `KnowledgeRoster.build_domains`, and never draw an empty domain row.
##
## **THE DETAIL IS INLINE AND THE SELECTION IS A TOGGLE** (§4). The reading opens beneath the row
## whose chip was pressed, pressing the open chip closes it, and only one is ever open. The block is
## mounted in BOTH states at `DETAIL_BLOCK_MIN_HEIGHT`, which is what stops the card breathing as
## readings open and close.
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
## The open reading's `✕` was pressed. **Not `node_selected` with the open key** — that would route a
## close through the controller's TOGGLE, which is a coincidence of the current state rather than the
## thing being asked for; this says *close the reading* whatever is open.
signal detail_closed

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
## The domain rows and the detail block, interleaved — the detail sits immediately after the row that
## owns the selected node, or last when nothing is selected.
var _rows: VBoxContainer = null
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
	# than the ladder rows need.
	#
	# ⛔ **THE HORIZONTAL AXIS IS `AUTO`, AND THAT IS REQUIRED RATHER THAN COSMETIC.** A
	# ScrollContainer with an axis DISABLED propagates its child's full minimum on that axis instead
	# of absorbing it, which is exactly what overruled the old width clamp (see the class docstring).
	# With it `AUTO` the card can genuinely be bounded, and a room narrower than one reading scrolls
	# rather than forcing the card wide.
	_scroll = ScrollContainer.new()
	_scroll.name = "KnowledgeScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "KnowledgeBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_body)

	_rows = VBoxContainer.new()
	_rows.name = "DomainRows"
	_rows.add_theme_constant_override("separation", 0)
	_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(_rows)

# ---- public API -------------------------------------------------------------

## Rebuild the whole panel against `payload` (see the `PAYLOAD_*` keys) and show it.
##
## **NO SCROLL OFFSET IS CARRIED**, unlike the crafting ledger's. This body is three short columns and
## a reading — it does not scroll at any ordinary window size — so there is no place for a player to
## be scrolled to that a rebuild could cost them.
func render(payload: Dictionary) -> void:
	_payload = payload
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_rows)
	var nodes := KnowledgeRoster.flatten(payload.get(PAYLOAD_DOMAINS, []))
	_build_header(payload, nodes)
	_build_rows(payload, nodes)
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
	HudWidgets.clear_children(_rows)

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
	# ⛔ **FIXED WIDTH.** The card is `PANEL_WIDTH` and narrows only when the ROOM itself is narrower —
	# it never grows or shrinks with its content, so opening and closing a reading cannot move it
	# (`docs/plan_knowledge_rows.md` §4). `target_width` is therefore the panel's ACTUAL width rather
	# than the nominal floor it used to be, and `fit_width(0, 0)` has nothing left to fit: it applies
	# exactly `target_width`.
	target_width = clampf(room.size.x, HudKnowledgeVocab.PANEL_MIN_WIDTH, HudKnowledgeVocab.PANEL_WIDTH)
	max_width = target_width
	fit_width(0.0, 0.0)
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
	_append_filter_note(payload, nodes, filter_row)
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


# ---- the domain rows --------------------------------------------------------

## **ONE ROW PER DOMAIN, AND THE READING INTERLEAVED.** The hairline between two rows, then the row,
## then — when the selected node belongs to THIS domain — the reading, immediately after it.
##
## **THE BLOCK IS ALWAYS MOUNTED.** With nothing selected it goes last, holding its placeholder, at
## the same `DETAIL_BLOCK_MIN_HEIGHT` reserve it takes when open: the body's minimum height does not
## change when a knowledge is opened or closed, so the card cannot breathe (see the class docstring).
func _build_rows(payload: Dictionary, nodes: Array) -> void:
	var filter := StringName(payload.get(PAYLOAD_FILTER, HudKnowledgeVocab.FILTER_ALL))
	var selected := String(payload.get(PAYLOAD_SELECTED, ""))
	var open_node := _find_node(nodes, selected)
	var placed := false
	var first := true
	for domain_variant in payload.get(PAYLOAD_DOMAINS, []):
		if not (domain_variant is Dictionary):
			continue
		var domain := domain_variant as Dictionary
		# **NO RULE BEFORE THE FIRST ROW** — the header already draws one, and a second hairline under
		# it reads as an empty band rather than as a separator.
		if not first:
			_rows.add_child(_rule(HudStyle.LINE_SOFT))
		first = false
		_rows.add_child(_build_domain_row(domain, filter, selected))
		if not open_node.is_empty() and not placed \
				and String(open_node.get(HudKnowledgeVocab.NODE_DOMAIN, "")) \
					== String(domain[HudKnowledgeVocab.DOMAIN_KEY]):
			_rows.add_child(_build_detail_block(open_node))
			placed = true
	if not placed:
		_rows.add_child(_build_detail_block({}))

## ONE DOMAIN'S ROW: `NAME   ●chip ── ◐chip ── ○chip`, wrapping when it runs out of width.
##
## **THE RUNGS RIDE IN AN `HFlowContainer`, AND THAT IS WHAT KEEPS THE CARD BOUNDED.** It wraps, so
## its own minimum width is only its WIDEST CHILD — one chip — rather than the sum of the ladder. A
## row of `HBoxContainer`s would put the whole ladder back into the card's minimum, which is the
## column layout's defect on the other axis.
func _build_domain_row(domain: Dictionary, filter: StringName, selected: String) -> Control:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_left", HudKnowledgeVocab.ROW_PADDING_H)
	host.add_theme_constant_override("margin_right", HudKnowledgeVocab.ROW_PADDING_H)
	host.add_theme_constant_override("margin_top", HudKnowledgeVocab.ROW_PADDING_V)
	host.add_theme_constant_override("margin_bottom", HudKnowledgeVocab.ROW_PADDING_V)
	host.set_meta(HudKnowledgeVocab.DOMAIN_META, String(domain[HudKnowledgeVocab.DOMAIN_KEY]))

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudKnowledgeVocab.ROW_NAME_GUTTER)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	host.add_child(line)

	# The domain's name, right-aligned in a fixed gutter so every row's first chip starts on one
	# vertical — and pinned to the TOP of the row, so a row whose chips have wrapped onto two lines
	# still reads as belonging to the name beside its first line.
	var name_label := Label.new()
	name_label.text = String(domain[HudKnowledgeVocab.DOMAIN_LABEL]).to_upper()
	name_label.custom_minimum_size = Vector2(HudKnowledgeVocab.ROW_NAME_WIDTH, 0.0)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	name_label.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	name_label.add_theme_font_size_override("font_size", HudKnowledgeVocab.DOMAIN_HEAD_FONT_SIZE)
	name_label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	line.add_child(name_label)

	# **THE RAIL IS THE DOMAIN'S SHAPE, DRAWN.** A ladder's nodes are ordered — each earned by
	# practising the one below — and the connector between two chips is what says so; the craft fan
	# has no order to state, so it draws a bare gap. ONE `if` on the DESCRIPTOR, never on a name.
	var is_ladder := String(domain[HudKnowledgeVocab.DOMAIN_SHAPE]) \
		== HudKnowledgeVocab.DOMAIN_SHAPE_LADDER
	var rungs := HFlowContainer.new()
	rungs.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# Both separations are zero: the CONNECTORS carry the spacing, so a wrapped chip lands at the
	# same pitch as one that did not wrap.
	rungs.add_theme_constant_override("h_separation", 0)
	rungs.add_theme_constant_override("v_separation", 0)
	line.add_child(rungs)

	var first := true
	for node_variant in domain[HudKnowledgeVocab.DOMAIN_NODES]:
		if not (node_variant is Dictionary):
			continue
		if not first:
			rungs.add_child(_build_connector(is_ladder))
		first = false
		rungs.add_child(_build_node_chip(node_variant as Dictionary, filter, selected))
	return host

## The rail between two rungs — a drawn hairline for a LADDER, and `FAN_GAP` of nothing for a fan.
## **Only the ladder's carries `RAIL_META`**, so *"a ladder draws its rail and the craft fan draws
## none"* stays a claim a harness can make about the tree rather than about a pixel.
func _build_connector(is_ladder: bool) -> Control:
	if not is_ladder:
		var gap := Control.new()
		gap.custom_minimum_size = Vector2(HudKnowledgeVocab.FAN_GAP, 0.0)
		gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
		return gap
	var rail := Panel.new()
	rail.custom_minimum_size = Vector2(HudKnowledgeVocab.CONNECTOR_LENGTH,
		HudKnowledgeVocab.RAIL_THICKNESS)
	rail.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	rail.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	rail.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rail.set_meta(HudKnowledgeVocab.RAIL_META, true)
	return rail

## ONE NODE'S CHIP: `● Cultivation`, plus a percent while it is being learned, plus the `◇` when
## nothing is using it, plus the capability capsule when it gates nothing.
##
## **A `PanelContainer` WITH `gui_input`, NOT A `Button`, and that is the panel's own documented
## rule.** A Button is not a Container, so a face parented to one is NEVER LAID OUT — the children
## pile up at the origin and the chip's height stops being a function of its content — and a `flat`
## Button ignores its `normal` stylebox outright, so the SELECTED state would be an override reaching
## nothing the widget draws. Both were shipped here first and both are invisible to a bounds
## assertion. `BandCityPanel._make_tab_button` records the same finding for the same reason.
##
## **PRESSING IT IS A READING, not a queue.** There is nothing to order and nothing to spend — see
## the class docstring. It carries `NODE_META` so a harness finds it by the node it IS rather than by
## whatever text it happens to be showing.
func _build_node_chip(node: Dictionary, filter: StringName, selected: String) -> Control:
	var key := String(node[HudKnowledgeVocab.NODE_KEY])
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	var ink: Color = HudKnowledgeVocab.NODE_INKS.get(state, HudStyle.INK)
	var is_selected := key == selected

	var chip := PanelContainer.new()
	chip.mouse_filter = Control.MOUSE_FILTER_STOP
	chip.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	chip.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	chip.tooltip_text = _chip_tooltip(node)
	chip.set_meta(HudKnowledgeVocab.NODE_META, key)
	chip.add_theme_stylebox_override("panel", _node_chip_stylebox(is_selected))
	chip.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
				and event.pressed:
			node_selected.emit(key))

	# A mouse-transparent face inside the panel, so the glyph, the name and whatever hangs off them
	# read (and click) as ONE chip.
	var face := HBoxContainer.new()
	face.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_theme_constant_override("separation", HudKnowledgeVocab.CHIP_SEPARATION)
	chip.add_child(face)

	face.add_child(_chip_label(String(HudKnowledgeVocab.NODE_GLYPHS.get(state, "")), ink,
		HudKnowledgeVocab.NODE_NAME_FONT_SIZE))
	face.add_child(_chip_label(String(node[HudKnowledgeVocab.NODE_LABEL]), ink,
		HudKnowledgeVocab.NODE_NAME_FONT_SIZE))

	# **THE BLOCK METER IS DROPPED FROM THE CHIP — it does not fit one.** It survives in the reading's
	# state line, which is why `METER_CELLS` is still live; the chip states the percent alone.
	if state == HudKnowledgeVocab.NODE_STATE_LEARNING:
		face.add_child(_chip_label(HudKnowledgeVocab.CHIP_PERCENT_FORMAT
			% HudFormat.progress_percent(float(node[HudKnowledgeVocab.NODE_PROGRESS])),
			HudStyle.WARN, HudKnowledgeVocab.NODE_VALUE_FONT_SIZE))

	# **THE UNSPENT STATE HAS TO BE LEGIBLE WITHOUT A CLICK**, and the clause row it used to ride on
	# has nowhere to go under a chip. So the MARK rides here, the clause rides in the tooltip
	# (`_chip_tooltip`) and the sentence rides in the reading's state line — three carriers, all three
	# wired. `WARN`, the same tint the tally's unspent clause takes.
	if bool(node.get(HudKnowledgeVocab.NODE_UNSPENT, false)):
		face.add_child(_chip_label(HudKnowledgeVocab.UNSPENT_MARK, HudStyle.WARN,
			HudKnowledgeVocab.NODE_NAME_FONT_SIZE))

	# **A KNOWLEDGE THAT GATES NOTHING SAYS SO ON ITS FACE.** `foddering` hangs off the end of its
	# ladder, and the capsule is what stops it reading as one more step. Off `NODE_UNSPENT_TESTABLE`,
	# which is the ladder's own `is_step` — never a client list of exceptions. Crafts publish it
	# `true`, so the capsule cannot land on one.
	if not bool(node.get(HudKnowledgeVocab.NODE_UNSPENT_TESTABLE, false)):
		face.add_child(_build_capability_capsule())

	# **DIM, NEVER HIDE** — see the class docstring. `modulate` on the CHIP itself: there is no row
	# host any more, and a per-Label tint would leave the capsule bright over a faded name.
	if not KnowledgeRoster.matches(node, filter):
		chip.modulate = Color(1.0, 1.0, 1.0, HudKnowledgeVocab.FILTERED_OUT_ALPHA)
	return chip

## The chip's hover. The unlock note is what a player wants off a name they do not recognise; the
## unspent clause is APPENDED to it rather than replacing it, because the two say different things
## and the clause has lost its own row on the face.
func _chip_tooltip(node: Dictionary) -> String:
	var note := String(node.get(HudKnowledgeVocab.NODE_NOTE, ""))
	if not bool(node.get(HudKnowledgeVocab.NODE_UNSPENT, false)):
		return note
	var clause := "%s %s" % [HudKnowledgeVocab.UNSPENT_MARK, HudKnowledgeVocab.UNSPENT_CLAUSE]
	return clause if note == "" else "%s\n%s" % [note, clause]

## The `gates nothing` tag — a fully-rounded outline around a faint caption, so it reads as something
## hanging off the chip rather than as another word in the knowledge's name.
func _build_capability_capsule() -> Control:
	var capsule := PanelContainer.new()
	capsule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	capsule.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.0)
	sb.set_border_width_all(HudKnowledgeVocab.CHIP_BORDER_THICKNESS)
	sb.border_color = HudStyle.LINE
	sb.set_corner_radius_all(HudStyle.PILL_CORNER_RADIUS)
	sb.content_margin_left = HudKnowledgeVocab.CAPSULE_PADDING_H
	sb.content_margin_right = HudKnowledgeVocab.CAPSULE_PADDING_H
	sb.content_margin_top = HudKnowledgeVocab.CAPSULE_PADDING_V
	sb.content_margin_bottom = HudKnowledgeVocab.CAPSULE_PADDING_V
	capsule.add_theme_stylebox_override("panel", sb)
	capsule.add_child(_chip_label(HudKnowledgeVocab.CAPABILITY_CAPSULE, HudStyle.INK_FAINT,
		HudKnowledgeVocab.CAPSULE_FONT_SIZE))
	return capsule

func _chip_label(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", ink)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return label

## The chip's own box. **Transparent either way — a node is text, not a control — and the SELECTED one
## wears the faint wash this HUD gives a live selection inside a `SIGNAL` border.** Identical content
## margins in both states, so selecting a chip never moves the ones beside it; that is
## `BandCityPanel._tab_stylebox`'s rule, and it is what makes the stylebox the honest carrier of the
## state (a `flat` Button's `normal` override draws nothing at all).
##
## **A BORDER ON ALL FOUR SIDES rather than the column layout's leading bar.** A bar down one edge
## said *this row of a column* and the chips are a horizontal run, where a leading bar reads as a
## connector to whatever is left of it.
func _node_chip_stylebox(selected: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.SIGNAL_WASH if selected else Color(0.0, 0.0, 0.0, 0.0)
	sb.set_border_width_all(HudKnowledgeVocab.CHIP_BORDER_THICKNESS)
	sb.border_color = HudStyle.SIGNAL if selected else Color(0.0, 0.0, 0.0, 0.0)
	sb.set_corner_radius_all(HudStyle.READOUT_CORNER_RADIUS)
	sb.content_margin_left = HudKnowledgeVocab.CHIP_PADDING_H
	sb.content_margin_right = HudKnowledgeVocab.CHIP_PADDING_H
	sb.content_margin_top = HudKnowledgeVocab.CHIP_PADDING_V
	sb.content_margin_bottom = HudKnowledgeVocab.CHIP_PADDING_V
	return sb

# ---- the inline detail ------------------------------------------------------

## **A READING OF ONE NODE, BENEATH ITS OWN ROW: what it lets you do · where, now · how it is
## learned.** Nothing here is a control except the `✕`. See the class docstring.
##
## ⛔ **THE RESERVE IS THE POINT OF THIS BLOCK BEING MOUNTED IN BOTH STATES.** `DETAIL_BLOCK_MIN_HEIGHT`
## is claimed whether a knowledge is open or not, so the body's minimum height is the same either way
## and the card cannot narrow on a close and widen on an open — which, on a card centred in its room,
## is a lurch in both directions from the middle of the screen on every click
## (`docs/plan_knowledge_rows.md` §4).
func _build_detail_block(node: Dictionary) -> Control:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_left", HudKnowledgeVocab.DETAIL_INDENT)
	host.add_theme_constant_override("margin_right", HudKnowledgeVocab.ROW_PADDING_H)
	host.add_theme_constant_override("margin_top", HudKnowledgeVocab.ROW_PADDING_V)
	host.add_theme_constant_override("margin_bottom", HudKnowledgeVocab.ROW_PADDING_V)
	host.custom_minimum_size = Vector2(0.0, HudKnowledgeVocab.DETAIL_BLOCK_MIN_HEIGHT)
	host.set_meta(HudKnowledgeVocab.DETAIL_META,
		String(node.get(HudKnowledgeVocab.NODE_KEY, "")) if not node.is_empty() else "")

	var pane := PanelContainer.new()
	pane.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pane.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	pane.add_theme_stylebox_override("panel", _detail_stylebox(not node.is_empty()))
	host.add_child(pane)

	if node.is_empty():
		pane.add_child(_detail_placeholder())
		return host

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", HudKnowledgeVocab.DETAIL_SECTION_SEPARATION)
	pane.add_child(column)
	column.add_child(_detail_title_row(node))
	column.add_child(_detail_state_line(node))
	column.add_child(_detail_sections(node))
	return host

## The title and the way out. **The `✕` emits `detail_closed`, not `node_selected`** — see that
## signal for why a close must not route through the controller's toggle.
func _detail_title_row(node: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudKnowledgeVocab.HEADER_SEPARATION)

	var title := Label.new()
	title.text = String(node[HudKnowledgeVocab.NODE_LABEL])
	title.add_theme_font_size_override("font_size", HudKnowledgeVocab.DETAIL_TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	row.add_child(title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(spacer)

	var close := Button.new()
	close.text = HudKnowledgeVocab.CLOSE_GLYPH
	close.tooltip_text = HudKnowledgeVocab.CLOSE_TOOLTIP
	close.focus_mode = Control.FOCUS_NONE
	HudStyle.apply_button(close, "ghost")
	close.pressed.connect(func(): detail_closed.emit())
	row.add_child(close)
	return row

## `Known · nothing is using it` · `▰▰▱▱▱ 62%` · `Not begun` — the THIRD carrier of the unspent state,
## and the one that says it in words.
##
## **THE SCALE CONVERSION IS THE POINT OF THE LEARNING BRANCH.** `meter_bar` grades a `0..100` score
## and every node's progress is `0..1`, so a bare `progress` fills zero cells at every value under
## 0.5 — which is how the faction page's meters shipped EMPTY, indistinguishable from an unstarted
## track beside a live percent.
func _detail_state_line(node: Dictionary) -> Control:
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	var text := ""
	if state == HudKnowledgeVocab.NODE_STATE_KNOWN:
		text = HudKnowledgeVocab.NODE_VALUE_KNOWN
		if bool(node.get(HudKnowledgeVocab.NODE_UNSPENT, false)):
			text = HudKnowledgeVocab.TALLY_SEPARATOR.join(
				[text, HudKnowledgeVocab.UNSPENT_CLAUSE])
	elif state == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN:
		text = HudKnowledgeVocab.NODE_VALUE_NOT_BEGUN
	else:
		var progress := float(node[HudKnowledgeVocab.NODE_PROGRESS])
		text = HudKnowledgeVocab.LEARNING_VALUE_FORMAT % [
			HudFormat.meter_bar(progress * HudConst.PROGRESS_PERCENT_SCALE,
				HudKnowledgeVocab.METER_CELLS),
			HudFormat.progress_percent(progress)]
	return _caption(text, HudStyle.INK_DIM, HudKnowledgeVocab.DETAIL_BODY_FONT_SIZE)

## The three sections, side by side, in the prototype's order: **does · where · how**.
##
## ⛔ **A KNOWLEDGE WITH NO NOTE STILL DRAWS.** Absence leaves the column with less to say; it never
## removes the section and never removes the node. That is what keeps the panel wire-driven: if a
## missing sentence could suppress an entry, adding a knowledge would be a client edit again.
func _detail_sections(node: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudKnowledgeVocab.DETAIL_SECTION_GUTTER)

	# The unlock copy first, because it is the answer to the question that brought the player here.
	# Read from `FactionReadouts.KNOWLEDGE_UNLOCK_NOTES` via the roster — the same sentence any other
	# surface naming a discovery says, so the two cannot describe one differently.
	row.add_child(_detail_section(HudKnowledgeVocab.DETAIL_HEAD_UNLOCKS,
		String(node.get(HudKnowledgeVocab.NODE_NOTE, ""))))

	# **A NODE NOT YET LEARNED HAS NO "WHERE" AT ALL**, and saying "0 sources" about one would read as
	# a shortfall rather than as a thing not yet learned — so the kicker itself changes.
	var state := String(node[HudKnowledgeVocab.NODE_STATE])
	if state == HudKnowledgeVocab.NODE_STATE_KNOWN:
		row.add_child(_detail_section(HudKnowledgeVocab.DETAIL_HEAD_WHERE, _where_text(node)))
	else:
		row.add_child(_detail_section(HudKnowledgeVocab.DETAIL_NEEDS_HEAD,
			HudKnowledgeVocab.DETAIL_NEEDS_NOT_BEGUN \
			if state == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN \
			else HudKnowledgeVocab.DETAIL_NEEDS_LEARNING_FORMAT % HudFormat.progress_percent(
				float(node[HudKnowledgeVocab.NODE_PROGRESS]))))

	row.add_child(_detail_section(HudKnowledgeVocab.DETAIL_HEAD_PRACTISE,
		String(node.get(HudKnowledgeVocab.NODE_PRACTISE, ""))))
	return row

func _detail_section(kicker: String, body: String) -> Control:
	var column := VBoxContainer.new()
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.add_child(_detail_head(kicker))
	column.add_child(_detail_body(body))
	return column

func _detail_placeholder() -> Control:
	var label := _detail_body(HudKnowledgeVocab.DETAIL_PLACEHOLDER_BODY)
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	return label

## The reading's box. **The `SIGNAL` bar down the leading edge is what ties the block to the row above
## it** — it is the only thing on screen saying this paragraph belongs to that chip. The placeholder
## state draws no bar: it belongs to nothing.
func _detail_stylebox(open: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.0)
	if open:
		sb.border_width_left = HudKnowledgeVocab.DETAIL_BAR_THICKNESS
		sb.border_color = HudStyle.SIGNAL
		sb.content_margin_left = HudKnowledgeVocab.DETAIL_BAR_GUTTER
	return sb

## What `Where, now` says. Three shapes, and the third exists because a knowledge that unlocks
## nothing has no source to stand on it — the capsule on its chip says the same thing shorter.
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

## The caption a zero-match filter earns. **It rides ON the filter row, at its trailing end** — it is
## a note about the filter and it sits beside the pill that earned it. It used to hang in the pinned
## detail pane because a banner drawn ACROSS the columns would have read as a replacement for the list
## rather than as a note about it; there is no pinned pane to hang in now.
##
## ⛔ **ON THE ROW, NOT UNDER IT, AND THAT IS A SIZING RULE RATHER THAN A TASTE ONE.** `_header` is
## OUTSIDE the scroll and `_header_height()` feeds `fit_to_content`, so a caption mounted as a row of
## its own grows the card by its own height the moment a player presses a pill that matches nothing —
## measured at **477 → 499** on a centred card, which is the same lurch the reading's reserve exists
## to prevent, arriving through the other surface. A pill's own minimum height (a `Button` with
## `HudStyle.BUTTON_PADDING_V`) is taller than an `EMPTY_FONT_SIZE` Label, so riding the row costs
## NOTHING whether the note is there or not — no reserve, and no permanent dead band under the pills
## to pay for a caption that is usually absent.
##
## The width it adds is real but is not the binding term: the TITLE row (title + tally + `✕`) is wider
## than the pills plus this note, so the card's combined minimum is unmoved. The preview chapter
## asserts that, because it is the one way this placement could push the card past `PANEL_WIDTH`.
func _append_filter_note(payload: Dictionary, nodes: Array, filter_row: HBoxContainer) -> void:
	var filter := StringName(payload.get(PAYLOAD_FILTER, HudKnowledgeVocab.FILTER_ALL))
	if filter == HudKnowledgeVocab.FILTER_ALL:
		return
	if KnowledgeRoster.count_matching(nodes, filter) > 0:
		return
	var clause := String(HudKnowledgeVocab.FILTER_EMPTY_CLAUSES.get(filter, ""))
	if clause == "":
		return
	# The header's own gap between two clusters, so the note reads as a remark about the pills rather
	# than as a sixth one.
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(float(HudKnowledgeVocab.HEADER_SEPARATION), 0.0)
	gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	filter_row.add_child(gap)

	var label := _caption(HudKnowledgeVocab.FILTER_EMPTY_FORMAT % clause, HudStyle.INK_FAINT,
		HudKnowledgeVocab.EMPTY_FONT_SIZE)
	label.set_meta(HudKnowledgeVocab.EMPTY_NOTE_META, String(filter))
	filter_row.add_child(label)


func _find_node(nodes: Array, key: String) -> Dictionary:
	if key == "":
		return {}
	for node_variant in nodes:
		if node_variant is Dictionary \
				and String((node_variant as Dictionary).get(HudKnowledgeVocab.NODE_KEY, "")) == key:
			return node_variant as Dictionary
	return {}

# ---- leaves -----------------------------------------------------------------

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
	label.custom_minimum_size = Vector2(_detail_section_width(), 0.0)
	label.add_theme_font_size_override("font_size", HudKnowledgeVocab.DETAIL_BODY_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	return label

## One reading column's width, DERIVED rather than typed: the card's fixed width, less the card's own
## chrome and the scroll gutter, less the block's indent and right margin, less the leading bar and
## its gutter, less the gutters between the columns — shared out between them.
##
## **IT SUBTRACTS THE CHROME AND THE GUTTER DELIBERATELY.** A width derived from `PANEL_WIDTH` alone
## makes the reading's minimum exactly the card's outer width, which is wider than the card's
## INTERIOR — so the horizontal scrollbar would be showing on every frame of a card that fits.
func _detail_section_width() -> float:
	var interior := HudKnowledgeVocab.PANEL_WIDTH \
		- HudStyle.card_stylebox().get_minimum_size().x - _scroll_gutter()
	var gutters := float(HudKnowledgeVocab.DETAIL_SECTION_GUTTER) \
		* float(HudKnowledgeVocab.DETAIL_SECTION_COUNT - 1)
	var text_width := interior \
		- float(HudKnowledgeVocab.DETAIL_INDENT) \
		- float(HudKnowledgeVocab.ROW_PADDING_H) \
		- float(HudKnowledgeVocab.DETAIL_BAR_THICKNESS) \
		- float(HudKnowledgeVocab.DETAIL_BAR_GUTTER) \
		- gutters
	return maxf(text_width / float(HudKnowledgeVocab.DETAIL_SECTION_COUNT), 0.0)

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
