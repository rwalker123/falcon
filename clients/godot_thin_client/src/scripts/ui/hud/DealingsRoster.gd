class_name DealingsRoster
extends RefCounted

## **"DEALINGS" -- THE PENDING ITEMS ONE BAND TAKES PART IN, ON ITS OWN PAGE** (early diplomacy,
## issue #233, `docs/plan_early_diplomacy.md` §7).
##
## All-`static`, stateless: the Dealings tab, its wide-shell section, the in-place `refill`, the badge
## and the one ordering. The labor model is threaded in as a PARAMETER (the `ConnectionsRoster` idiom),
## and the three answers are Callables the caller hands in -- this file builds rows and emits nothing.
##
## **AN UNBOUNDED LIST GETS ITS OWN TAB.** A band can hold any number of dealings, so the tab scrolls
## with no row cap and no `+N more`, exactly as the Peoples tab does.
##
## **THE ROW IS THE SIM'S, NOT THE CLIENT'S.** The turns left, the kind, the legs and which side is
## shipper are published; nothing here re-derives a window, a link or a leg. The two things this file
## decides are wording (`HudDealingsVocab`) and which actions a row offers, and the second is a
## statement about the row's DIRECTION alone: an item addressed to this band is answered, one this band
## sent is waited on.

## The row directions. **Incoming** is addressed to this band (it is the receiver); **outgoing** is the
## rest -- the band sent it, or its people did.
const DIRECTION_INCOMING := 0
const DIRECTION_OUTGOING := 1

## Which way a dealing runs from `band_id`'s page. The receiver band answers; anything else is waited on.
static func direction_of(dealing: Dictionary, band_id: int) -> int:
	if int(dealing.get("receiver_band", HudConst.NO_BAND_ID)) == band_id:
		return DIRECTION_INCOMING
	return DIRECTION_OUTGOING

## **THE ROSTER'S ORDER**: what waits on this band first (ids ascending: the sim's monotonic ids make
## it a pure function of the rows), then what this band waits on.
static func ordered(dealings: Array, band_id: int) -> Array:
	var incoming: Array = []
	var outgoing: Array = []
	for dealing_variant in dealings:
		var dealing: Dictionary = dealing_variant
		if direction_of(dealing, band_id) == DIRECTION_INCOMING:
			incoming.append(dealing)
		else:
			outgoing.append(dealing)
	incoming.sort_custom(_id_before)
	outgoing.sort_custom(_id_before)
	incoming.append_array(outgoing)
	return incoming

static func _id_before(a: Dictionary, b: Dictionary) -> bool:
	return int(a.get("id", 0)) < int(b.get("id", 0))

## How many dealings wait on THIS band's answer.
static func awaiting_count(band_id: int, band_labor: HudBandLaborState) -> int:
	var count := 0
	for dealing_variant in band_labor.dealings_for_band(band_id):
		if direction_of(dealing_variant as Dictionary, band_id) == DIRECTION_INCOMING:
			count += 1
	return count

## The tab's badge: how many items wait on this band's answer (the hot case), else how many it waits
## on, and nothing at none. Returns `{text, hot}`.
static func badge(band_id: int, band_labor: HudBandLaborState) -> Dictionary:
	var rows := band_labor.dealings_for_band(band_id)
	var awaiting := awaiting_count(band_id, band_labor)
	if awaiting > 0:
		return {"text": str(awaiting), "hot": true}
	return {"text": str(rows.size()) if not rows.is_empty() else "", "hot": false}

## The tab's declared scroll viewport: the zone box less the head and the gap under it, floored at the
## parties list's own floor so an unknown or tiny box never collapses the list into a bare scrollbar.
static func list_viewport_height(box_height: float) -> float:
	return maxf(HudWorkVocab.PARTIES_LIST_MIN_HEIGHT,
		box_height - HudWorkVocab.ZONE_HEAD_HEIGHT - float(HudWorkVocab.ZONE_BLOCK_SEPARATION))

## **THE DEALINGS TAB** -- the head counting the rows, then EVERY row in a sanctioned `ScrollContainer`
## that fills the tab. `actions` carries `accept` / `refuse` / `counter`, each a Callable taking the
## decoded dealing.
static func build_tab(band_id: int, band_labor: HudBandLaborState, box_height: float,
		actions: Dictionary = {}, policy: Dictionary = {}) -> VBoxContainer:
	var column := HudWidgets.make_zone_column()
	column.name = HudDealingsVocab.BLOCK_NAME
	column.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
	var rows_data := ordered(band_labor.dealings_for_band(band_id), band_id)
	column.add_child(_build_head(rows_data.size()))
	var gifts := _build_gift_row(policy, actions)
	if gifts != null:
		column.add_child(gifts)
	var scroll := ScrollContainer.new()
	scroll.name = HudDealingsVocab.LIST_NAME
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	scroll.custom_minimum_size = Vector2(0.0, list_viewport_height(box_height) \
		- (HudDealingsVocab.GIFT_ROW_HEIGHT if gifts != null else 0.0))
	var gutter := MarginContainer.new()
	gutter.name = HudDealingsVocab.GUTTER_NAME
	gutter.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	gutter.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	gutter.add_theme_constant_override("margin_right",
		int(scroll.get_v_scroll_bar().get_combined_minimum_size().x))
	scroll.add_child(gutter)
	var rows := _make_rows_box()
	rows.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	gutter.add_child(rows)
	column.add_child(scroll)
	_fill_rows(rows, rows_data, band_labor, band_id, actions)
	return column

## **THE WIDE SHELL'S SECTION** -- the same head and rows with no scroll of its own: it rides at the
## foot of the Parties zone's sanctioned list, after the Peoples section.
static func build_section(band_id: int, band_labor: HudBandLaborState,
		actions: Dictionary = {}, policy: Dictionary = {}) -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.name = HudDealingsVocab.BLOCK_NAME
	var rows_data := ordered(band_labor.dealings_for_band(band_id), band_id)
	block.add_child(_build_head(rows_data.size()))
	var gifts := _build_gift_row(policy, actions)
	if gifts != null:
		block.add_child(gifts)
	var rows := _make_rows_box()
	block.add_child(rows)
	_fill_rows(rows, rows_data, band_labor, band_id, actions)
	return block

## **RE-FILL A BUILT TAB OR SECTION IN PLACE** -- the head's count and the rows, nothing else, so the
## tab's scroll keeps its node and offset when a frame moves only the dealings.
static func refill(block: Control, band_id: int, band_labor: HudBandLaborState,
		actions: Dictionary = {}, policy: Dictionary = {}) -> void:
	var rows := rows_box(block)
	if block == null or rows == null or block.get_child_count() == 0:
		return
	var rows_data := ordered(band_labor.dealings_for_band(band_id), band_id)
	var old_head := block.get_child(0)
	block.remove_child(old_head)
	old_head.queue_free()
	var head := _build_head(rows_data.size())
	block.add_child(head)
	block.move_child(head, 0)
	# The setting row is rebuilt in place too: its state is the snapshot's, and a refused command must put
	# the box back.
	var old_gifts := block.find_child(HudDealingsVocab.GIFT_ROW_NAME, true, false)
	if old_gifts != null:
		var index := old_gifts.get_index()
		old_gifts.get_parent().remove_child(old_gifts)
		old_gifts.queue_free()
		var gifts := _build_gift_row(policy, actions)
		if gifts != null:
			block.add_child(gifts)
			block.move_child(gifts, index)
	_fill_rows(rows, rows_data, band_labor, band_id, actions)

static func rows_box(block: Node) -> VBoxContainer:
	if block == null:
		return null
	return block.find_child(HudDealingsVocab.ROWS_NAME, true, false) as VBoxContainer

static func _build_head(count: int) -> Control:
	return HudWidgets.zone_head(HudDealingsVocab.HEAD,
		HudDealingsVocab.HEAD_COUNT_FORMAT % count if count > 0 else "")

## **THE STANDING ANSWER TO A GIFT FROM THE PLAYER'S OWN PEOPLE**, at the top of the tab it governs:
## on (the sim's default), such a shipment lands on arrival; off, it waits here to be answered. It is a
## setting of the WHOLE people (every band), which the hover says. The state is the snapshot's
## (`accept_own_gifts`), every render; a press flips the box locally and emits, and the server's
## recapture puts it back if refused. `null` when the snapshot carried no policy row.
static func _build_gift_row(policy: Dictionary, actions: Dictionary) -> CheckBox:
	if policy.is_empty():
		return null
	var box := CheckBox.new()
	box.name = HudDealingsVocab.GIFT_ROW_NAME
	box.text = HudDealingsVocab.ACCEPT_GIFTS_LABEL
	box.focus_mode = Control.FOCUS_NONE
	box.button_pressed = bool(policy.get("accept_own_gifts", true))
	box.tooltip_text = HudDealingsVocab.ACCEPT_GIFTS_HINT
	box.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	HudStyle.apply_checkbox(box)
	box.set_meta(HudDealingsVocab.GIFT_TOGGLE_META, true)
	var callback: Callable = actions.get(HudDealingsVocab.ACTION_ACCEPT_GIFTS, Callable())
	if callback.is_valid():
		box.toggled.connect(func(pressed: bool) -> void: callback.call(pressed))
	return box

static func _make_rows_box() -> VBoxContainer:
	var rows := HudWidgets.make_zone_block()
	rows.name = HudDealingsVocab.ROWS_NAME
	return rows

static func _fill_rows(rows: VBoxContainer, dealings: Array, band_labor: HudBandLaborState,
		band_id: int, actions: Dictionary) -> void:
	HudWidgets.clear_children(rows)
	if dealings.is_empty():
		rows.add_child(HudWidgets.alloc_hint_label(HudDealingsVocab.EMPTY))
		return
	for dealing_variant in dealings:
		rows.add_child(_build_row(dealing_variant as Dictionary, band_labor, band_id, actions))

# ---- ONE ROW ------------------------------------------------------------------------------------------

## The other people's faction id: the sender's on an item addressed here, the receiver's on one we sent.
static func other_faction_of(dealing: Dictionary, direction: int) -> int:
	return int(dealing.get("sender_faction", HudConst.NO_FACTION_ID)) \
		if direction == DIRECTION_INCOMING \
		else int(dealing.get("receiver_faction", HudConst.NO_FACTION_ID))

## The other band: the sender band on an incoming item, the receiver band on an outgoing one.
static func other_band_of(dealing: Dictionary, direction: int) -> int:
	return int(dealing.get("sender_band", HudConst.NO_BAND_ID)) \
		if direction == DIRECTION_INCOMING \
		else int(dealing.get("receiver_band", HudConst.NO_BAND_ID))

## `Ashfell · Brackwater` -- the other people's name then the band of theirs it concerns, through the
## roster's own naming rule (`HudBandLaborState.host_label_for_id`: the roster, else the name a tie
## remembers, else where it was last seen). A raw id never reaches the label.
static func who_text(dealing: Dictionary, direction: int, band_labor: HudBandLaborState) -> String:
	var people := FactionMark.faction_name(other_faction_of(dealing, direction))
	var band := band_labor.host_label_for_id(other_band_of(dealing, direction))
	if band == "":
		band = HudDealingsVocab.WHO_UNKNOWN_BAND
	return HudDealingsVocab.WHO_FORMAT % [people, band]

## **WHAT THE ROW SAYS** in one sentence from the viewing band's seat: a treaty reads its terms in
## words; an offer says what is carried and that the return is unnamed; a proposal or counter is stated
## from the SHIPPER's point of view on the wire, so the seat decides the pronouns.
static func what_text(dealing: Dictionary, direction: int, band_id: int) -> String:
	var kind := String(dealing.get("kind", ""))
	var incoming := direction == DIRECTION_INCOMING
	if kind == HudDealingsVocab.KIND_TREATY_PROPOSAL:
		var terms := HudDealingsVocab.terms_text(Dictionary(dealing.get("treaty", {})))
		return (HudDealingsVocab.TREATY_IN_FORMAT if incoming \
			else HudDealingsVocab.TREATY_OUT_FORMAT) % terms
	var sent := HudDealingsVocab.goods_text(Dictionary(dealing.get("goods_sent", {})))
	if kind == HudDealingsVocab.KIND_OFFER:
		return (HudDealingsVocab.OFFER_IN_FORMAT if incoming \
			else HudDealingsVocab.OFFER_OUT_FORMAT) % sent
	var asked := HudDealingsVocab.goods_text(Dictionary(dealing.get("goods_asked", {})))
	if int(dealing.get("shipper_band", HudConst.NO_BAND_ID)) == band_id:
		return HudDealingsVocab.SHIPPER_FORMAT % [sent, asked]
	return (HudDealingsVocab.CONSIGNEE_IN_FORMAT if incoming \
		else HudDealingsVocab.CONSIGNEE_OUT_FORMAT) % [sent, asked]

## **A ROW IS TWO LINES AND, WHERE IT IS ADDRESSED HERE, AN ACTION STRIP.**
##   * incoming -- line 1 `← Ashfell · Brackwater ……… 1 turn left` (the window in amber beside the
##     number it explains), line 2 what it says, then `Accept · Refuse · Counter`;
##   * outgoing -- line 1 `→ Ashfell · Brackwater`, line 2 `Awaiting their answer · 1 turn left`.
## Every line is one line, clipped with an ellipsis, so a row cannot wrap to a third; the hover carries
## the whole sentence.
static func _build_row(dealing: Dictionary, band_labor: HudBandLaborState, band_id: int,
		actions: Dictionary) -> VBoxContainer:
	var direction := direction_of(dealing, band_id)
	var incoming := direction == DIRECTION_INCOMING
	var turns := int(dealing.get("turns_left", 0))
	var who := who_text(dealing, direction, band_labor)
	var what := what_text(dealing, direction, band_id)
	var row := VBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudDealingsVocab.ROW_LINE_SEPARATION)
	row.set_meta(HudDealingsVocab.ROW_META, int(dealing.get("id", 0)))
	var hover := PackedStringArray([who, what])
	if String(dealing.get("carrier", "")) == HudDealingsVocab.CARRIER_ENVOY:
		hover.append(HudDealingsVocab.TREATY_ENVOY_TOOLTIP)
	elif String(dealing.get("carrier", "")) == HudDealingsVocab.CARRIER_ROAD:
		hover.append(HudDealingsVocab.TREATY_ROAD_TOOLTIP)
	row.tooltip_text = "\n".join(hover)
	row.mouse_filter = Control.MOUSE_FILTER_PASS

	var head := HBoxContainer.new()
	head.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
	head.add_child(_cell(HudDealingsVocab.INCOMING_GLYPH if incoming else HudDealingsVocab.OUTGOING_GLYPH,
		HudStyle.SIGNAL if incoming else HudStyle.INK_FAINT))
	var name_label := _line_label(who, HudStyle.INK, HudWorkVocab.WORK_ROW_FONT_SIZE)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(name_label)
	if incoming:
		var window := _cell(HudDealingsVocab.turns_left_text(turns), HudStyle.WARN)
		HudWidgets.set_label_tooltip(window, HudDealingsVocab.TURNS_LEFT_TOOLTIP)
		head.add_child(window)
	else:
		# What we sent rides line 1's right end, clipped first, so line 2 stays the one sentence the
		# row exists to say: that nothing is owed until they answer.
		var gives := _line_label(what, HudStyle.INK_DIM, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		gives.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		HudWidgets.set_label_tooltip(gives, what)
		head.add_child(gives)
	row.add_child(head)

	var second: Label
	if incoming:
		second = _line_label(what, HudStyle.INK_DIM, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		HudWidgets.set_label_tooltip(second, "\n".join(hover))
	else:
		second = _line_label(
			HudDealingsVocab.AWAITING_FORMAT % HudDealingsVocab.turns_left_text(turns),
			HudStyle.INK_FAINT, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		HudWidgets.set_label_tooltip(second, HudDealingsVocab.OUTGOING_TOOLTIP)
		row.tooltip_text = "\n".join(PackedStringArray([who, what, HudDealingsVocab.OUTGOING_TOOLTIP]))
	row.add_child(second)
	if not incoming:
		return row
	row.add_child(_build_actions(dealing, actions))
	return row

## The incoming row's strip: `Accept · Refuse · Counter`. An offer cannot be countered (the receiver
## names the return in its accept); every other kind can. Each is greyed with its reason where the
## caller supplied none.
static func _build_actions(dealing: Dictionary, actions: Dictionary) -> HBoxContainer:
	var strip := HBoxContainer.new()
	strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	strip.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
	strip.set_meta(HudDealingsVocab.ACTION_META, int(dealing.get("id", 0)))
	var is_offer := String(dealing.get("kind", "")) == HudDealingsVocab.KIND_OFFER
	var entries: Array[Dictionary] = [
		{"id": HudDealingsVocab.ANSWER_ACCEPT, "text": HudDealingsVocab.ACTION_ACCEPT,
			"tip": HudDealingsVocab.ACCEPT_OFFER_TOOLTIP if is_offer else HudDealingsVocab.ACCEPT_TOOLTIP,
			"ink": HudStyle.HEALTHY},
		{"id": HudDealingsVocab.ANSWER_REFUSE, "text": HudDealingsVocab.ACTION_REFUSE,
			"tip": HudDealingsVocab.REFUSE_TOOLTIP, "ink": HudStyle.DANGER},
	]
	if not is_offer:
		entries.append({"id": HudDealingsVocab.ANSWER_COUNTER, "text": HudDealingsVocab.ACTION_COUNTER,
			"tip": HudDealingsVocab.COUNTER_TOOLTIP, "ink": HudStyle.SIGNAL})
	for entry in entries:
		var callback: Callable = actions.get(entry["id"], Callable())
		var link := HudWidgets.build_inline_link(String(entry["text"]), entry["ink"] as Color,
			func() -> void:
				if callback.is_valid():
					callback.call(dealing))
		link.tooltip_text = String(entry["tip"])
		link.set_meta(HudDealingsVocab.ACTION_META, String(entry["id"]))
		link.disabled = not callback.is_valid()
		strip.add_child(link)
	return strip

## A single-line, ellipsis-clipped label -- a row's line can never wrap into a third.
static func _line_label(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.clip_text = true
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", font_size)
	return label

## A line-1 cell: never clipped, sized to its text.
static func _cell(text: String, ink: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	return label
