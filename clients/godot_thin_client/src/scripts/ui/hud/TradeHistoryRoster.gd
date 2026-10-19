class_name TradeHistoryRoster
extends RefCounted

## **THE TRADE TAB'S THIRD SECTION -- EVERY TRADE THIS BAND TOOK PART IN, NEWEST FIRST** (early
## diplomacy, issue #233, `docs/plan_early_diplomacy.md` §1). The sim keeps a per-people record of every
## shipment and dealing that ended (the event feed only keeps twenty turns), and publishes the viewer's
## people's records; `HudBandLaborState.trade_history_for_band` picks this band's.
##
## All-`static`, stateless, the `DealingsRoster` idiom. **No cap and no `+N more`**: the Trade tab is one
## scroll, which is why it can hold two unbounded lists.
##
## **A RECORD IS THE SIM'S, NOT THE CLIENT'S.** The outcome, the goods each way and the turn are
## published; this file only words them. The one thing it decides is which END of a same-people
## shipment the viewing band is: that shipment writes ONE record (`own_band` the sender's band,
## `other_band` the receiver's), so the receiving band reads it from its own seat -- what `goods_out`
## says left is what this band got.

## A record as the viewing band reads it: `{faction, band, outcome, goods_out, goods_in, turn}`.
static func view_of(record: Dictionary, band_id: int) -> Dictionary:
	var own_band := int(record.get("own_band", HudConst.NO_BAND_ID))
	var other_band := int(record.get("other_band", HudConst.NO_BAND_ID))
	var same_people := int(record.get("other_faction", HudConst.NO_FACTION_ID)) == HudConst.PLAYER_FACTION_ID
	var out_goods := Dictionary(record.get("goods_out", {}))
	var in_goods := Dictionary(record.get("goods_in", {}))
	var outcome := String(record.get("outcome", ""))
	var band := other_band
	if same_people and own_band != band_id and other_band == band_id:
		# The receiving end of a shipment between two of this people's bands: the other end is the sender,
		# and what left them is what came here.
		band = own_band
		out_goods = Dictionary(record.get("goods_in", {}))
		in_goods = Dictionary(record.get("goods_out", {}))
		if outcome == HudDealingsVocab.OUTCOME_GIFT_GIVEN:
			outcome = HudDealingsVocab.OUTCOME_GIFT_RECEIVED
		elif outcome == HudDealingsVocab.OUTCOME_GIFT_RECEIVED:
			outcome = HudDealingsVocab.OUTCOME_GIFT_GIVEN
	return {"faction": int(record.get("other_faction", HudConst.NO_FACTION_ID)), "band": band,
		"same_people": same_people, "outcome": outcome, "goods_out": out_goods, "goods_in": in_goods,
		"turn": int(record.get("turn", 0))}

## The counterparty as the dealings rows name it (`Ashfell · Brackwater`), and for a band of the viewer's
## own people the band alone -- "your people" in front of one's own band says nothing.
static func counterparty_text(view: Dictionary, band_labor: HudBandLaborState) -> String:
	if bool(view["same_people"]):
		return DealingsRoster.band_name_of(int(view["band"]), band_labor)
	return DealingsRoster.who_text_for(int(view["faction"]), int(view["band"]), band_labor)

## The outcome in the tab's words; an unknown token reads as itself rather than as nothing.
static func outcome_word(outcome: String) -> String:
	return String(HudDealingsVocab.OUTCOME_WORDS.get(outcome, outcome))

## The outcome word's ink: HEALTHY where goods moved, DANGER where a deal was turned down or broke,
## faint where it ran out unanswered or was taken back. Palette entries the dealing rows already wear.
static func outcome_ink(outcome: String) -> Color:
	if HudDealingsVocab.OUTCOMES_DANGER.has(outcome):
		return HudStyle.DANGER
	if HudDealingsVocab.OUTCOMES_FAINT.has(outcome):
		return HudStyle.INK_FAINT
	return HudStyle.HEALTHY

## **LINE 2.** Where goods moved: `gave 5.0 food · got 2.0 hide`, a side that is empty left out. Where
## nothing moved: `offered 5.0 food for 2.0 hide`, `offered 5.0 food` or `asked for 2.0 hide`. `""` when
## the record states no goods at all (the row is then one line).
static func detail_text(view: Dictionary) -> String:
	var out_goods: Dictionary = view["goods_out"]
	var in_goods: Dictionary = view["goods_in"]
	var gave := "" if HudDealingsVocab.goods_are_empty(out_goods) else HudDealingsVocab.goods_text(out_goods)
	var got := "" if HudDealingsVocab.goods_are_empty(in_goods) else HudDealingsVocab.goods_text(in_goods)
	if HudDealingsVocab.OUTCOMES_MOVED.has(String(view["outcome"])):
		var parts := PackedStringArray()
		if gave != "":
			parts.append(HudDealingsVocab.HISTORY_GAVE_FORMAT % gave)
		if got != "":
			parts.append(HudDealingsVocab.HISTORY_GOT_FORMAT % got)
		return HudDealingsVocab.HISTORY_SIDES_SEPARATOR.join(parts)
	if gave != "" and got != "":
		return HudDealingsVocab.HISTORY_OFFERED_FOR_FORMAT % [gave, got]
	if gave != "":
		return HudDealingsVocab.HISTORY_OFFERED_FORMAT % gave
	if got != "":
		return HudDealingsVocab.HISTORY_ASKED_FORMAT % got
	return ""

## **THE SECTION** -- the head counting the records, then every record newest first, or the one quiet
## line. No scroll of its own: it rides the Trade tab's.
static func build_section(band_id: int, band_labor: HudBandLaborState) -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.name = HudDealingsVocab.HISTORY_BLOCK_NAME
	var records := band_labor.trade_history_for_band(band_id)
	block.add_child(_build_head(records.size()))
	var rows := HudWidgets.make_zone_block()
	rows.name = HudDealingsVocab.HISTORY_ROWS_NAME
	block.add_child(rows)
	_fill_rows(rows, records, band_labor, band_id)
	return block

## Re-fill a built section in place: the head's count and the rows.
static func refill(block: Control, band_id: int, band_labor: HudBandLaborState) -> void:
	var rows := block.find_child(HudDealingsVocab.HISTORY_ROWS_NAME, true, false) as VBoxContainer \
		if block != null else null
	if rows == null or block.get_child_count() == 0:
		return
	var records := band_labor.trade_history_for_band(band_id)
	var old_head := block.get_child(0)
	block.remove_child(old_head)
	old_head.queue_free()
	var head := _build_head(records.size())
	block.add_child(head)
	block.move_child(head, 0)
	_fill_rows(rows, records, band_labor, band_id)

static func _build_head(count: int) -> Control:
	return HudWidgets.zone_head(HudDealingsVocab.HISTORY_HEAD,
		HudDealingsVocab.HEAD_COUNT_FORMAT % count if count > 0 else "")

static func _fill_rows(rows: VBoxContainer, records: Array, band_labor: HudBandLaborState,
		band_id: int) -> void:
	HudWidgets.clear_children(rows)
	if records.is_empty():
		rows.add_child(HudWidgets.alloc_hint_label(HudDealingsVocab.HISTORY_EMPTY))
		return
	for record_variant in records:
		rows.add_child(_build_row(record_variant as Dictionary, band_labor, band_id))

## One record, two lines at most: `Turn 12 · Ashfell · Brackwater · Traded` (the outcome in its ink) then
## what moved. Both are single-line labels that ellipsise, so a row cannot wrap to a third; the hover
## carries the whole of both.
static func _build_row(record: Dictionary, band_labor: HudBandLaborState, band_id: int) -> VBoxContainer:
	var view := view_of(record, band_id)
	var who := counterparty_text(view, band_labor)
	var outcome := String(view["outcome"])
	var word := outcome_word(outcome)
	var detail := detail_text(view)
	var row := VBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudDealingsVocab.ROW_LINE_SEPARATION)
	row.set_meta(HudDealingsVocab.HISTORY_ROW_META, int(view["turn"]))
	row.mouse_filter = Control.MOUSE_FILTER_PASS
	var lead := HudDealingsVocab.HISTORY_LEAD_FORMAT % [int(view["turn"]), who]
	var hover := PackedStringArray([HudDealingsVocab.HISTORY_LINE_FORMAT % [int(view["turn"]), who, word]])
	if detail != "":
		hover.append(detail)
	row.tooltip_text = "\n".join(hover)

	var head := HBoxContainer.new()
	head.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_theme_constant_override("separation", 0)
	head.add_child(_line_label(lead, HudStyle.INK, HudWorkVocab.WORK_ROW_FONT_SIZE))
	head.add_child(_cell(HudDealingsVocab.HISTORY_OUTCOME_JOINER, HudStyle.INK_FAINT))
	var outcome_label := _cell(word, outcome_ink(outcome))
	outcome_label.set_meta(HudDealingsVocab.HISTORY_ROW_META, outcome)
	head.add_child(outcome_label)
	row.add_child(head)
	if detail != "":
		var second := _line_label(detail, HudStyle.INK_DIM, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		HudWidgets.set_label_tooltip(second, "\n".join(hover))
		row.add_child(second)
	return row

static func _line_label(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.clip_text = true
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", font_size)
	return label

static func _cell(text: String, ink: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	return label
