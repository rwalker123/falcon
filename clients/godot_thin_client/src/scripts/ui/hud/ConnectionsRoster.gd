class_name ConnectionsRoster
extends RefCounted

## **"PEOPLES WE KNOW" — THE TIES ONE BAND HOLDS, ON ITS OWN PAGE** (arc #527, issue #549).
##
## All-`static`, stateless: the Peoples tab and its wide-shell section, the tie-state rule it reads, the
## roster's order, and **the one subject-naming rule** the shipment picker shares
## (`subject_label`). The labor model is threaded in as a PARAMETER, never held — the `HudWidgets` /
## `SourceForecast` idiom.
##
## **A CONNECTION IS THE PRIMITIVE, TRADE IS ONE RIDER ON IT**, so this is the surface that says who a
## band knows without pretending to send goods. It renders the published row and nothing more:
##   * **ties are per OBSERVER band**, so the block lists `connections_for_band(<this band>)` and lives
##     on a band's page only, never the faction page;
##   * **the position is REMEMBERED, never live** — a connection only ever grants `Discovered`
##     (`.claude/rules/core_sim/connections.md` → the keystone), so the line under every row is worded
##     as a sighting with its turn;
##   * **a PARKED tie (strength at `HudConst.TIE_STRENGTH_NONE`) is SHOWN**, its state in amber and its
##     second line saying what it needs — hiding it would hide that the tie is what gates every rider.

## The three states, read off the published row and the current turn alone. **Growing** is contact
## THIS turn (`last_contact_turn == current_turn`, which `record_contact` stamps sim-side). Contact
## always raises strength above zero, so a zero-strength row is parked whatever its stamps say. An
## unknown turn (`NO_TURN`, before the first frame) never reads as contact.
static func tie_state(tie: Dictionary, current_turn: int) -> int:
	if not HudBandLaborState.tie_is_live(tie):
		return HudConnectionsVocab.STATE_PARKED
	if current_turn != HudConnectionsVocab.NO_TURN \
			and int(tie.get("last_contact_turn", HudConnectionsVocab.NO_TURN)) == current_turn:
		return HudConnectionsVocab.STATE_GROWING
	return HudConnectionsVocab.STATE_FADING

## **THE ROSTER'S ORDER**: live ties by strength, strongest first; then parked ties by their last
## contact, most recent first. Ties on either key break by `subject_band_id`, so the order is a pure
## function of the rows and two frames of the same section cannot reshuffle.
static func ordered(ties: Array) -> Array:
	var rows: Array = ties.duplicate()
	rows.sort_custom(_tie_before)
	return rows

static func _tie_before(a_variant: Variant, b_variant: Variant) -> bool:
	var a: Dictionary = a_variant
	var b: Dictionary = b_variant
	var a_live := HudBandLaborState.tie_is_live(a)
	var b_live := HudBandLaborState.tie_is_live(b)
	if a_live != b_live:
		return a_live
	if a_live:
		var a_strength := float(a.get("strength", HudConst.TIE_STRENGTH_NONE))
		var b_strength := float(b.get("strength", HudConst.TIE_STRENGTH_NONE))
		if not is_equal_approx(a_strength, b_strength):
			return a_strength > b_strength
	else:
		var a_contact := int(a.get("last_contact_turn", 0))
		var b_contact := int(b.get("last_contact_turn", 0))
		if a_contact != b_contact:
			return a_contact > b_contact
	return int(a.get("subject_band_id", HudConst.NO_BAND_ID)) \
		< int(b.get("subject_band_id", HudConst.NO_BAND_ID))

## **THE NAME A TIE'S SUBJECT IS SHOWN UNDER — one rule for the roster AND the shipment picker.**
##   1. a band this faction still holds is named exactly as the cycler, the band picker and the event
##      dock name it (`band_label_for_id`) — one band, one name across every surface;
##   2. else the name the tie REMEMBERS (`subject_name`, clock 1: what they answered to when last
##      seen), which is how a foreign, dead or split-off subject is still named;
##   3. else where they were (`Band near (x, y)`).
## The raw `BandId` is a database key and never reaches a player-facing label.
static func subject_label(tie: Dictionary, band_labor: HudBandLaborState) -> String:
	var label := band_labor.band_label_for_id(
		int(tie.get("subject_band_id", HudConst.NO_BAND_ID)))
	if label != "":
		return label
	var remembered := String(tie.get("subject_name", ""))
	if remembered != "":
		return remembered
	return HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_LABEL_FORMAT % [
		int(tie.get("last_seen_x", -1)), int(tie.get("last_seen_y", -1))]

## The remembered-position sentence, or `""` when no tile was ever recorded. Shared wording with the
## shipment picker (`HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT`).
static func remembered_line(tie: Dictionary) -> String:
	var x := int(tie.get("last_seen_x", -1))
	var y := int(tie.get("last_seen_y", -1))
	if x < 0 or y < 0:
		return ""
	return HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT % [
		x, y, int(tie.get("last_seen_turn", 0))]

## **THE PEOPLES TAB** — the band page's narrow-shell zone of its own (`BandCityPanel.ZONE_PEOPLES`):
## the head counting the ties, then EVERY tie in a sanctioned `ScrollContainer`
## (`HudConnectionsVocab.LIST_NAME`) that fills the tab under the head. No row cap and no `+N more`:
## the list scrolls. The scroll reports no minimum on its axis; its declared viewport is
## `list_viewport_height(<the zone's box height>)`, so the list's own height never reaches the panel.
static func build_tab(band_id: int, band_labor: HudBandLaborState,
		box_height: float) -> VBoxContainer:
	var column := HudWidgets.make_zone_column()
	column.name = HudConnectionsVocab.BLOCK_NAME
	column.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
	var ties := ordered(band_labor.connections_for_band(band_id))
	column.add_child(_build_head(ties.size()))
	var scroll := ScrollContainer.new()
	scroll.name = HudConnectionsVocab.LIST_NAME
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	scroll.custom_minimum_size = Vector2(0.0, list_viewport_height(box_height))
	# The rows sit in a gutter the scrollbar's own width wide, reserved whether or not it shows —
	# Trade's list popover's gutter, the same measure — so the bar never touches a row's state word and
	# the rows do not jump sideways when the list starts to scroll.
	var gutter := MarginContainer.new()
	gutter.name = HudConnectionsVocab.GUTTER_NAME
	gutter.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# A scrolled child must not claim the viewport's height as its own, or a short list would stretch
	# its rows down the tab; the width still fills, since horizontal scrolling is disabled.
	gutter.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	gutter.add_theme_constant_override("margin_right",
		int(scroll.get_v_scroll_bar().get_combined_minimum_size().x))
	scroll.add_child(gutter)
	var rows := _make_rows_box()
	rows.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	gutter.add_child(rows)
	column.add_child(scroll)
	_fill_rows(rows, ties, band_labor)
	return column

## **THE WIDE SHELL'S SECTION** — the same head and rows, with no scroll of its own: it rides at the
## foot of the Parties zone's sanctioned list, after Trade's section, and that list scrolls.
static func build_section(band_id: int, band_labor: HudBandLaborState) -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.name = HudConnectionsVocab.BLOCK_NAME
	var ties := ordered(band_labor.connections_for_band(band_id))
	block.add_child(_build_head(ties.size()))
	var rows := _make_rows_box()
	block.add_child(rows)
	_fill_rows(rows, ties, band_labor)
	return block

## **RE-FILL A BUILT TAB OR SECTION IN PLACE** — the head's count and the rows, nothing else. The
## tab's scroll and its offset survive, which is what lets a connections-only frame refresh the list
## without throwing the player back to its top.
static func refill(block: Control, band_id: int, band_labor: HudBandLaborState) -> void:
	var rows := rows_box(block)
	if block == null or rows == null or block.get_child_count() == 0:
		return
	var ties := ordered(band_labor.connections_for_band(band_id))
	var old_head := block.get_child(0)
	block.remove_child(old_head)
	old_head.queue_free()
	var head := _build_head(ties.size())
	block.add_child(head)
	block.move_child(head, 0)
	_fill_rows(rows, ties, band_labor)

## The rows box inside a built tab or section, found by its node name.
static func rows_box(block: Node) -> VBoxContainer:
	if block == null:
		return null
	return block.find_child(HudConnectionsVocab.ROWS_NAME, true, false) as VBoxContainer

## The tab's badge: how many ties the band holds, parked ones included, and nothing at none — the
## Trade tab's own convention.
static func badge_text(band_id: int, band_labor: HudBandLaborState) -> String:
	var count := band_labor.connections_for_band(band_id).size()
	return HudConnectionsVocab.HEAD_COUNT_FORMAT % count if count > 0 else ""

## The tab's declared scroll viewport: the zone box less the head and the gap under it, floored at the
## parties list's own floor (`HudWorkVocab.PARTIES_LIST_MIN_HEIGHT`) so an unknown or tiny box never
## collapses the list into a bare scrollbar.
static func list_viewport_height(box_height: float) -> float:
	return maxf(HudWorkVocab.PARTIES_LIST_MIN_HEIGHT,
		box_height - HudWorkVocab.ZONE_HEAD_HEIGHT - float(HudWorkVocab.ZONE_BLOCK_SEPARATION))

static func _build_head(count: int) -> Control:
	return HudWidgets.zone_head(HudConnectionsVocab.HEAD,
		HudConnectionsVocab.HEAD_COUNT_FORMAT % count if count > 0 else "")

static func _make_rows_box() -> VBoxContainer:
	var rows := HudWidgets.make_zone_block()
	rows.name = HudConnectionsVocab.ROWS_NAME
	return rows

## One two-line row per tie in `ordered` order, or the one-line empty state.
static func _fill_rows(rows: VBoxContainer, ties: Array, band_labor: HudBandLaborState) -> void:
	HudWidgets.clear_children(rows)
	if ties.is_empty():
		rows.add_child(HudWidgets.alloc_hint_label(HudConnectionsVocab.EMPTY))
		return
	var turn := band_labor.current_turn()
	for tie_variant in ties:
		rows.add_child(_build_row(tie_variant as Dictionary, band_labor, turn))

## One tie: **line 1** `Name ……… 75% · growing`, **line 2** where they were. Each line is ONE line,
## clipped with an ellipsis, so a row is two lines at any width; the row's hover carries the whole
## name and line 2 its whole sentence.
static func _build_row(tie: Dictionary, band_labor: HudBandLaborState, turn: int) -> VBoxContainer:
	var state := tie_state(tie, turn)
	var row := VBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudConnectionsVocab.ROW_LINE_SEPARATION)
	row.set_meta(HudConnectionsVocab.ROW_META,
		int(tie.get("subject_band_id", HudConst.NO_BAND_ID)))
	# The row's hover leads with the whole name, which the name label may have clipped.
	var subject := subject_label(tie, band_labor)
	row.tooltip_text = "\n".join(PackedStringArray([subject,
		HudConnectionsVocab.ROW_TOOLTIP_FORMAT % [
			int(tie.get("first_contact_turn", 0)), int(tie.get("last_contact_turn", 0))]]))
	row.mouse_filter = Control.MOUSE_FILTER_PASS

	var head := HBoxContainer.new()
	head.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
	var name_label := _line_label(subject, HudStyle.INK,
		HudWorkVocab.WORK_ROW_FONT_SIZE)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(name_label)
	head.add_child(_cell(HudConnectionsVocab.STRENGTH_FORMAT % HudFormat.progress_percent(
		float(tie.get("strength", HudConst.TIE_STRENGTH_NONE))), HudStyle.INK_DIM))
	head.add_child(_cell(HudConnectionsVocab.ROW_SEPARATOR, HudStyle.INK_FAINT))
	var word := _cell(String(HudConnectionsVocab.STATE_WORDS[state]), _state_ink(state))
	HudWidgets.set_label_tooltip(word, String(HudConnectionsVocab.STATE_TOOLTIPS[state]))
	head.add_child(word)
	row.add_child(head)

	var remembered := remembered_line(tie)
	var second: Label
	if state == HudConnectionsVocab.STATE_PARKED:
		var x := int(tie.get("last_seen_x", -1))
		var y := int(tie.get("last_seen_y", -1))
		var hint := HudConnectionsVocab.PARKED_HINT if remembered == "" \
			else HudConnectionsVocab.PARKED_HINT_FORMAT % [x, y, int(tie.get("last_seen_turn", 0))]
		second = _line_label(hint, HudStyle.WARN, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		HudWidgets.set_label_tooltip(second, "\n".join(
			PackedStringArray([hint, remembered]) if remembered != "" else PackedStringArray([hint])))
	elif remembered != "":
		second = _line_label(remembered, HudStyle.INK_FAINT, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
		HudWidgets.set_label_tooltip(second, remembered)
	if second != null:
		row.add_child(second)
	return row

## The state word's ink: growing reads healthy, fading dim, parked amber.
static func _state_ink(state: int) -> Color:
	match state:
		HudConnectionsVocab.STATE_GROWING:
			return HudStyle.HEALTHY
		HudConnectionsVocab.STATE_PARKED:
			return HudStyle.WARN
	return HudStyle.INK_DIM

## A single-line, ellipsis-clipped label — a row's line can never wrap into a third.
static func _line_label(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.clip_text = true
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", font_size)
	return label

## A line-1 cell after the name: never clipped, sized to its text.
static func _cell(text: String, ink: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	return label
