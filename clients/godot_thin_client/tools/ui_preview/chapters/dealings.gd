extends RefCounted

## EARLY DIPLOMACY, THE CLIENT'S HALF (issue #233, slice C1) — the merged Trade tab's waiting list and
## history (the Dealings tab is gone), the Peoples tab's treaty proposals, the answer sheet they open, the trade sheet's gift / offer / propose modes, the scout sheet's pack and strangers order,
## the faction page's gift toggle, the orb's "a people awaits your answer" row, and the event dock's
## dealing and treaty lines.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. It is appended LAST and hands the reference band back, so no earlier
## frame moves. See `.claude/rules/client/harness-ui-preview.md`.
##
## **EVERY FIXTURE IS SHAPED EXACTLY AS THE NATIVE DECODER WRITES IT** (`dict/dealings.rs`), and every
## command is asserted by EQUALITY through the real `Main` builders — a payload can be perfectly good
## and be spelled into a line the server's parser refuses.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 242

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
const EVENT_DOCK_SCENE := preload("res://src/ui/EventDockPanel.tscn")

var h

const PANEL_RESERVER := &"band_panel"

## The turn the frames render on.
const TURN := 52
const RESOLVED := TURN - HudConnectionsVocab.RESOLVED_TURN_LAG

## The band the player sees the world through, and its holdings.
const HOME_ENTITY := 1281
const HOME_NAME := "Ashfell"
const HOME_TILE := Vector2i(71, 18)
const HOME_PROVISIONS := 84.0
const HOME_FODDER := 41.0
const HOME_IDLE := 8
const CARRY_PER_WORKER := 10.0
const MATERIAL_WEIGHT := 2.0
const FODDER_WEIGHT := 0.5

## Three other peoples, one band each. The first is the player's rival with a kept road to the home
## band (a standing link), the second a people with no road, the third a people whose envoy came.
const BRACK_FACTION := 1
const BRACK_ENTITY := 1284
const BRACK_NAME := "Brackwater"
const BRACK_PEOPLE := "Brackfolk"
const BRACK_SEEN := Vector2i(63, 21)
const THORN_FACTION := 2
const THORN_ENTITY := 1285
const THORN_NAME := "Thornwick"
const THORN_PEOPLE := "Thornfolk"
const THORN_SEEN := Vector2i(52, 12)
const MOOR_FACTION := 3
const MOOR_ENTITY := 1286
const MOOR_NAME := "Gorsemoor"
const MOOR_PEOPLE := "Moorfolk"
const MOOR_SEEN := Vector2i(44, 9)
## The strength Moorfolk's tie is parked at for the "no live tie" treaty-sheet frame (the sim refuses an
## envoy to a band we hold no live tie with).
const MOOR_TIE_PARKED := 0.0
## A fourth people met ONLY through a remembered tie -- no goodwill row, no band in sight.
const ASH_FACTION := 4
const ASH_ENTITY := 1287
const ASH_NAME := "Fenmere"
const ASH_PEOPLE := "Fenfolk"
const ASH_SEEN := Vector2i(38, 15)

## The dealings' ids.
const OFFER_IN := 11
const PROPOSAL_IN := 12
const OFFER_OUT := 13
const TREATY_IN := 14
const COUNTER_IN := 15
## A REPLY addressed to the home band (Brackwater named a return for our cargo) and one the home band
## named (Thornwick's party waits on our return).
const REPLY_IN := 16
const REPLY_OUT := 17
## A GIFT addressed to the home band (Gorsemoor sent it) and one the home band gave Gorsemoor: offer-kind
## dealings whose `terms` is `gift`, which take nothing back.
const GIFT_IN := 18
const GIFT_OUT := 19
const GIFT_FOOD := 7.0
const GIFT_HIDE := 2.0

## What an offer carries, and the party that waits with it.
const OFFER_FOOD := 12.0
const OFFER_HIDE := 4.0
const OFFER_PARTY := 3
## A proposal asks for the player's food in return for flint.
const PROPOSAL_FLINT := 5.0
const PROPOSAL_FOOD := 10.0
const PROPOSAL_PARTY := 2
## The return the answer sheet is driven to.
const RETURN_FOOD := 3.0
## What a reply carries: the cargo it is for, and the return named for it.
const REPLY_CARGO_FOOD := 9.0
const REPLY_RETURN_FLINT := 5.0
## The home band's parties (issue #233): four trade parties and the two that are NOT. The waiting one
## stands in Brackwater's camp, so the pending offer we sent and the reply addressed to us (both between
## the home band and Brackwater) are its dealing rows; the other two shipments have none.
const PARTY_WAITING := 7301
## A SECOND shipment camped on the very same pair (Brackwater): the reply addressed to us is ITS dealing.
const PARTY_WAITING_REPLY := 7306
const PARTY_HEADING_OUT := 7302
const PARTY_COMING_HOME := 7303
const PARTY_ENVOY := 7304
const PARTY_SCOUT := 7305
const PARTY_SIZE := 2
const PARTY_ENVOY_SIZE := 2
const PARTY_SCOUT_SIZE := 3
## The envoy's destination band is named by the wire itself (a published name wins over the roster's).
const ENVOY_DESTINATION := "Fenmere"
const ROAD_CARGO_FOOD := 5.0
const ROAD_CARGO_HIDE := 2.0
## The trade history's turns, oldest first (the newest is the turn before the one the frames render on).
const HISTORY_TURN_TRADED := 38
const HISTORY_TURN_GIFT_GIVEN := 41
const HISTORY_TURN_DECLINED := 43
const HISTORY_TURN_REFUSED := 44
const HISTORY_TURN_EXPIRED := 46
const HISTORY_TURN_GIFT_RECEIVED := 47
const HISTORY_TURN_WITHDRAWN := 48
const HISTORY_TURN_FAILED := 50
## A record of ANOTHER band of the player's, which this band's page must not show.
const HISTORY_TURN_OTHER_BAND := 45
const OTHER_BAND_ENTITY := 1299
## A turn added to prove the history re-fills in place.
const HISTORY_TURN_ADDED := 51

var _commands: Array = []


func run(harness) -> void:
	h = harness
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	# A panel built after another chapter's inherits that chapter's persisted edge, and `set_dock` on the
	# edge it already holds emits no reservation -- so the HUD would not yield. Leave it, then arrive.
	panel.set_dock(SIDE_RIGHT)
	panel.set_dock(SIDE_LEFT)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	h._hud.set_band_city_panel(panel)
	h._hud.diplomacy_command_requested.connect(func(payload: Dictionary) -> void: _commands.append(payload))

	var prior_turn: int = h._hud._band_labor.current_turn()
	FactionNames.update([
		{"faction": 0, "name": "Ashfolk"}, {"faction": BRACK_FACTION, "name": BRACK_PEOPLE},
		{"faction": THORN_FACTION, "name": THORN_PEOPLE}, {"faction": MOOR_FACTION, "name": MOOR_PEOPLE}])
	h._hud.update_overlay(TURN, {})
	h._hud.update_connections(_ties())
	h._hud.update_band_alerts([_home_band()] + _parties())
	h._hud.update_treaties(_treaties())
	h._hud.update_goodwill(_goodwill())
	h._hud.update_dealings(_dealings())
	h._hud.update_trade_history(_history())
	h._hud.show_unit_selection(_home_band())
	await h._settle()

	# The dock reads in full on the LEFT (the right dock runs off the harness's 1500px canvas); the SHEETS
	# live in the tile drawer, which the left dock covers, so they are judged with the dock on the right.
	await _tab_states(panel)
	await _road_states(panel)
	await _gift_states(panel)
	await _reply_states(panel)
	await _history_states(panel)
	await _peoples_treaty_states(panel)
	panel.set_dock(SIDE_RIGHT)
	await h._settle()
	await _answer_sheet_states(panel)
	await _trade_sheet_states(panel)
	await _scout_sheet_states(panel)
	panel.set_dock(SIDE_LEFT)
	await h._settle()
	await _faction_page_states(panel)
	await _orb_states()
	await _feed_states()
	_command_states()

	# Hand the reference band back, so a chapter appended after this one starts where the rest do.
	h._hud.reset_world_state()
	FactionNames.reset()
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(PANEL_RESERVER, SIDE_LEFT, 0.0)
	h._hud.set_reserved_inset(PANEL_RESERVER, SIDE_RIGHT, 0.0)
	panel.queue_free()
	await h.get_tree().process_frame
	h._hud.update_overlay(prior_turn, {})
	h._hud.update_connections([])
	h._hud.update_band_alerts([BandFx.band_fixture()])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()


# ---- (a) THE MERGED TRADE TAB ----------------------------------------------------------------------

func _tab_states(panel: BandCityPanel) -> void:
	await h._save("dealings_tab")
	var block := _block(panel)
	h._assert_hud("dealings: the band page's Trade tab carries the waiting list", block != null)
	h._assert_hud("dealings: …there is NO Dealings tab any more (the zone key is gone)",
		not BandCityPanel.ZONE_KEYS.has(&"dealings"))
	var tab_text := _text_of(panel).to_lower()
	var at_waiting := tab_text.find(HudDealingsVocab.HEAD.to_lower())
	var at_turn := tab_text.find(HudTradeVocab.THIS_TURN_HEAD.to_lower())
	var at_history := tab_text.find(HudDealingsVocab.HISTORY_HEAD.to_lower())
	h._assert_hud("dealings: the tab holds three sections in order: waiting on an answer, this turn, history",
		at_waiting >= 0 and at_turn > at_waiting and at_history > at_turn)
	var scrolls: Array = []
	_collect_scrolls(panel._zones.get(BandCityPanel.ZONE_TRADE), scrolls)
	h._assert_hud("dealings: the whole tab is ONE sanctioned scroll (%d found)" % scrolls.size(),
		scrolls.size() == 1 and String((scrolls[0] as Node).name) == TradeZoneController.TAB_SCROLL_NAME)
	h._assert_hud("dealings: …and the SHORT tier is gone (the full tier is always drawn)",
		panel.find_child("TradeShortTier", true, false) == null
			and panel.find_child(TradeZoneController.FULL_TIER_NAME, true, false) != null)
	var rows := _rows(block)
	h._assert_hud("dealings: the six TRADE items are listed, the treaty proposal is not (%d)" % rows.size(),
		rows.size() == 6)
	h._assert_hud("dealings: what waits on this band's answer is listed FIRST, ids ascending",
		_row_ids(rows) == [OFFER_IN, PROPOSAL_IN, COUNTER_IN, REPLY_IN, OFFER_OUT, REPLY_OUT])
	h._assert_hud("dealings: …a treaty proposal is NOT in the Trade tab", _row(rows, TREATY_IN) == null)
	var offer := _row(rows, OFFER_IN)
	var offer_text := _text_of(offer)
	h._assert_hud("dealings: an offer names the other PEOPLE and the band of theirs (\"%s\")" % offer_text.left(60),
		offer_text.contains(BRACK_PEOPLE) and offer_text.contains(BRACK_NAME))
	h._assert_hud("dealings: …what is carried, per account and per material with its reading, never summed",
		offer_text.contains("12.0 food") and offer_text.contains("4.0 hide (tough: fair, supple: good)"))
	h._assert_hud("dealings: …that it names no return",
		offer_text.contains("you name what comes back"))
	h._assert_hud("dealings: …and the window in amber beside the number it explains",
		offer_text.contains(HudDealingsVocab.TURNS_LEFT_ONE)
			and _find_label(offer, HudDealingsVocab.TURNS_LEFT_ONE).get_theme_color("font_color") == HudStyle.WARN)
	h._assert_hud("dealings: an offer is accepted or refused but not countered (it names no terms)",
		_action(offer, HudDealingsVocab.ANSWER_ACCEPT) != null
			and _action(offer, HudDealingsVocab.ANSWER_REFUSE) != null
			and _action(offer, HudDealingsVocab.ANSWER_COUNTER) == null)
	var proposal := _row(rows, PROPOSAL_IN)
	var proposal_text := _text_of(proposal)
	h._assert_hud("dealings: a proposal reads from the SHIPPER's seat: they send flint and want food (\"%s\")" % proposal_text.left(70),
		proposal_text.contains("They send 5.0 flint") and proposal_text.contains("they want 10.0 food from you"))
	h._assert_hud("dealings: …and offers accept, refuse AND counter",
		_action(proposal, HudDealingsVocab.ANSWER_ACCEPT) != null
			and _action(proposal, HudDealingsVocab.ANSWER_COUNTER) != null)
	var counter := _row(rows, COUNTER_IN)
	h._assert_hud("dealings: a counter to OUR proposal reads from our seat as the shipper",
		_text_of(counter).contains("You send 8.0 food") and _text_of(counter).contains("you get 2.0 hide"))
	var outgoing := _row(rows, OFFER_OUT)
	var out_text := _text_of(outgoing)
	h._assert_hud("dealings: an outgoing item waits: \"awaiting their answer · N turns left\" (\"%s\")" % out_text.left(80),
		out_text.contains("Awaiting their answer · 1 turn left"))
	h._assert_hud("dealings: …and offers no action", _action(outgoing, HudDealingsVocab.ANSWER_ACCEPT) == null)
	h._assert_hud("dealings: every row is two lines of copy (the strip, where there is one, is controls)",
		_copy_lines(offer) == 2 and _copy_lines(outgoing) == 2)
	for row in rows:
		h._assert_hud("dealings: no row prints a raw id (%s)" % _text_of(row).left(30),
			not _text_of(row).contains(str(BandFx.FIXTURE_BAND_ID_OFFSET)))
	h._assert_hud("dealings: the Trade tab badge counts the TRADE items that wait on this band, hot",
		String(panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {}).get("text", "")) == "4"
			and bool(panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {}).get("hot", false)))


# ---- (a1) A TRADE PARTY BELONGS TO THE TRADE TAB, END TO END ----------------------------------------------

func _road_states(panel: BandCityPanel) -> void:
	var labor: HudBandLaborState = h._hud._band_labor
	var parties_zone: Control = panel._zones.get(BandCityPanel.ZONE_PARTIES)
	# The Parties tab drops every trade party: its head and badge count the scout and the envoy alone.
	panel.set_active_tab(BandCityPanel.ZONE_PARTIES)
	await h._settle()
	var parties_text := _text_of(parties_zone)
	h._assert_hud("road: the Parties head counts the scout and the envoy only (\"%s\")" % parties_text.left(60),
		parties_text.contains(HudComposeVocab.PARTIES_HEADER_FORMAT % [2, PARTY_ENVOY_SIZE + PARTY_SCOUT_SIZE]))
	h._assert_hud("road: …no trade party is a row there (Gorsemoor, Thornwick and Brackwater are the shipments' bands)",
		not parties_text.contains(MOOR_NAME) and not parties_text.contains(THORN_NAME)
			and not parties_text.contains(BRACK_NAME))
	h._assert_hud("road: …the envoy IS a row there, reading Waiting on an answer",
		parties_text.contains(ENVOY_DESTINATION)
			and parties_text.contains(HudExpeditionVocab.EXPEDITION_PHASE_WAITING_ON_ANSWER_LABEL))
	h._assert_hud("road: …the scout keeps its own wording, Awaiting orders",
		parties_text.contains("Awaiting orders"))
	h._assert_hud("road: …and the Parties badge counts 2, not the four shipments as well",
		String(panel._tab_badges.get(BandCityPanel.ZONE_PARTIES, {}).get("text", "")) == "2")
	h._assert_hud("road: …the band's own roster still has all five parties (the Workforce 'away' clause reads them)",
		labor.band_parties(_home_band()).size() == 6 and labor.band_zone_parties(_home_band()).size() == 2)
	await h._save("dealings_parties_no_shipments")
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	await h._settle()
	# The Trade tab gains the On the road section between waiting and this turn.
	var block := panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false) as Control
	h._assert_hud("road: the Trade tab has an On the road section", block != null)
	var tab_text := _text_of(panel).to_lower()
	var at_waiting := tab_text.find(HudDealingsVocab.HEAD.to_lower())
	var at_road := tab_text.find(HudDealingsVocab.ROAD_HEAD.to_lower())
	var at_turn := tab_text.find(HudTradeVocab.THIS_TURN_HEAD.to_lower())
	h._assert_hud("road: …between Waiting on an answer and This turn",
		at_waiting >= 0 and at_road > at_waiting and at_turn > at_road)
	var rows := _road_rows(block)
	h._assert_hud("road: …one row for each shipment no dealing stands for, and not the one standing in a camp (%d)" % rows.size(),
		rows.size() == 2 and _road_row(rows, PARTY_HEADING_OUT) != null and _road_row(rows, PARTY_COMING_HOME) != null
			and _road_row(rows, PARTY_WAITING) == null)
	var out := _road_row(rows, PARTY_HEADING_OUT)
	var out_text := _text_of(out)
	h._assert_hud("road: a party leaving reads its destination, Heading out and its cargo (\"%s\")" % out_text.left(70),
		out_text.contains(MOOR_NAME) and out_text.contains(HudDealingsVocab.PHASE_HEADING_OUT)
			and out_text.contains("%.1f food" % ROAD_CARGO_FOOD))
	var home := _road_row(rows, PARTY_COMING_HOME)
	var home_text := _text_of(home)
	h._assert_hud("road: …one walking home reads Coming home and its cargo (\"%s\")" % home_text.left(70),
		home_text.contains(THORN_NAME) and home_text.contains(HudDealingsVocab.PHASE_COMING_HOME)
			and home_text.contains("%.1f hide" % ROAD_CARGO_HIDE))
	h._assert_hud("road: …every road row is two lines (the links ride the second)",
		_copy_lines(out) == 2 and _copy_lines(home) == 2)
	for row in rows:
		var jump := _party_link(row, HudDealingsVocab.PARTY_LINK_JUMP)
		var recall := _party_link(row, HudDealingsVocab.PARTY_LINK_RECALL)
		h._assert_hud("road: …each row offers Jump to party and Recall",
			jump != null and recall != null and jump.text == HudComposeVocab.PARTY_INSPECT_JUMP
				and recall.text == HudComposeVocab.PARTY_RECALL_VERB)
	# The shipment standing in the camp shows once: on its dealing rows, with the same two links.
	var waiting := _block(panel)
	var dealing_rows := _rows(waiting)
	var offer_out := _row(dealing_rows, OFFER_OUT)
	var reply_in := _row(dealing_rows, REPLY_IN)
	h._assert_hud("road: the offer we sent, standing in a camp, carries the party's Jump and Recall",
		_party_link(offer_out, HudDealingsVocab.PARTY_LINK_JUMP) != null
			and _party_link(offer_out, HudDealingsVocab.PARTY_LINK_RECALL) != null)
	h._assert_hud("road: …and so does the reply addressed to us, beside Take it and Reject it",
		_party_link(reply_in, HudDealingsVocab.PARTY_LINK_JUMP) != null
			and _party_link(reply_in, HudDealingsVocab.PARTY_LINK_RECALL) != null
			and _action(reply_in, HudDealingsVocab.ANSWER_ACCEPT) != null)
	h._assert_hud("road: …a dealing with no party of ours behind it offers neither (the offer addressed to us)",
		_party_link(_row(dealing_rows, OFFER_IN), HudDealingsVocab.PARTY_LINK_JUMP) == null)
	h._assert_hud("road: …the offer row is still two lines of copy with the links on it",
		_copy_lines(offer_out) == 2)
	h._assert_hud("road: …the Trade badge still counts the TRADE items waiting on the band (unchanged)",
		String(panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {}).get("text", "")) == "4")
	# The links are the Parties row's own actions: Recall asks the same confirm, Jump selects the party.
	var focused: Array = []
	h._hud.alert_focus_requested.connect(func(x: int, y: int) -> void: focused.append(Vector2i(x, y)))
	var link := _party_link(offer_out, HudDealingsVocab.PARTY_LINK_JUMP)
	if link != null:
		link.emit_signal("pressed")
	var reply_link := _party_link(reply_in, HudDealingsVocab.PARTY_LINK_JUMP)
	if reply_link != null:
		reply_link.emit_signal("pressed")
	h._assert_hud("road: two parties camped on ONE pair each get their OWN links (offer -> x+1, reply -> x+2: %s)" % str(focused),
		focused.has(Vector2i(HOME_TILE.x + 1, HOME_TILE.y)) and focused.has(Vector2i(HOME_TILE.x + 2, HOME_TILE.y)))
	h._assert_hud("road: …pressing Jump to party focuses THAT party's tile, as the Parties row's did (%s)" % str(focused),
		focused.has(Vector2i(HOME_TILE.x + 1, HOME_TILE.y)))
	await h._save("dealings_road")
	# A frame that moves only the dealings re-fills the road section in place: the waiting party's offer is
	# withdrawn and its row goes, so the party is on the road as a row of its own.
	var scroll := panel.find_child(TradeZoneController.TAB_SCROLL_NAME, true, false) as ScrollContainer
	var kept: Array = _dealings().filter(func(dealing: Dictionary) -> bool:
		return int(dealing["id"]) != OFFER_OUT and int(dealing["id"]) != REPLY_IN)
	h._hud.update_dealings(kept)
	await h._settle()
	h._assert_hud("road: a party whose dealing is gone is on the road as a row of its own, in the same scroll",
		panel.find_child(TradeZoneController.TAB_SCROLL_NAME, true, false) == scroll
			and _road_rows(panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false)).size() == 4
			and _road_row(_road_rows(panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false)),
				PARTY_WAITING) != null)
	var waiting_row := _road_row(_road_rows(panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false)),
		PARTY_WAITING)
	h._assert_hud("road: …reading Waiting on an answer, not Awaiting orders (\"%s\")" % _text_of(waiting_row).left(60),
		_text_of(waiting_row).contains(HudExpeditionVocab.EXPEDITION_PHASE_WAITING_ON_ANSWER_LABEL)
			and not _text_of(waiting_row).contains("Awaiting orders"))
	await h._save("dealings_road_waiting")
	h._hud.update_dealings(_dealings())
	await h._settle()
	h._assert_hud("road: …and with the offer back it goes again, the dealing row carrying it",
		_road_row(_road_rows(panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false)), PARTY_WAITING) == null)
	# With no party on the road at all, the section is not drawn.
	h._hud.update_band_alerts([_home_band()])
	await h._settle()
	h._assert_hud("road: no trade party anywhere -> no On the road section (an empty head is furniture)",
		panel.find_child(HudDealingsVocab.ROAD_BLOCK_NAME, true, false) == null)
	h._hud.update_band_alerts([_home_band()] + _parties())
	await h._settle()


# ---- (a1b) A GIFT TAKES NOTHING BACK ---------------------------------------------------------------------

func _gift_states(panel: BandCityPanel) -> void:
	var gift_in := _dealing(GIFT_IN, HudDealingsVocab.KIND_OFFER, MOOR_FACTION, MOOR_ENTITY, 0, HOME_ENTITY,
		_goods(GIFT_FOOD), _goods(), 2, 0)
	gift_in["terms"] = HudDealingsVocab.TERMS_GIFT
	var gift_out := _dealing(GIFT_OUT, HudDealingsVocab.KIND_OFFER, 0, HOME_ENTITY, MOOR_FACTION, MOOR_ENTITY,
		_goods(0.0, 0.0, [_flat_hide(GIFT_HIDE)]), _goods(), 2, 0)
	gift_out["terms"] = HudDealingsVocab.TERMS_GIFT
	h._hud.update_dealings(_dealings() + [gift_in, gift_out])
	await h._settle()
	var rows := _rows(_block(panel))
	var incoming := _row(rows, GIFT_IN)
	var in_text := _text_of(incoming)
	h._assert_hud("gift: an incoming gift reads as a gift, not an offer (\"%s\")" % in_text.left(70),
		in_text.contains("They send %.1f food as a gift" % GIFT_FOOD) and not in_text.contains("you name what comes back"))
	var accept := _action(incoming, HudDealingsVocab.ANSWER_ACCEPT)
	h._assert_hud("gift: …it offers Accept and Refuse and NO counter",
		accept != null and _action(incoming, HudDealingsVocab.ANSWER_REFUSE) != null
			and _action(incoming, HudDealingsVocab.ANSWER_COUNTER) == null
			and accept.tooltip_text == HudDealingsVocab.ACCEPT_GIFT_TOOLTIP)
	var outgoing := _row(rows, GIFT_OUT)
	var out_text := _text_of(outgoing)
	h._assert_hud("gift: the band's own waiting gift reads `You gave …`, awaiting their answer (\"%s\")" % out_text.left(70),
		out_text.contains("You gave %.1f hide" % GIFT_HIDE) and out_text.contains("Awaiting their answer")
			and not out_text.contains("they name what comes back"))
	await h._save("dealings_gift_rows")
	_commands.clear()
	if accept != null:
		accept.emit_signal("pressed")
	h._assert_hud("gift: Accept sends ONE command and opens NO return sheet", _commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var built := MAIN_SCRIPT.format_diplomacy_command(_commands[0])
		h._assert_hud("gift: …the bare `answer_dealing 0 %d accept` (got `%s`)" % [GIFT_IN, built.get("line", "")],
			String(built.get("line", "")) == "answer_dealing 0 %d accept" % GIFT_IN)
	# The pair: an incoming OFFER still opens the return sheet.
	_commands.clear()
	var offer_accept := _action(_row(rows, OFFER_IN), HudDealingsVocab.ANSWER_ACCEPT)
	if offer_accept != null:
		offer_accept.emit_signal("pressed")
	h._assert_hud("gift: …while an incoming OFFER still opens the return sheet and sends nothing yet",
		_form() != null and _commands.is_empty())
	h._hud._bandpanel.close_verb_form()
	await h._settle()
	h._hud.update_dealings(_dealings())
	await h._settle()


func _road_rows(block: Node) -> Array:
	var rows: Array = []
	if block == null:
		return rows
	var box := block.find_child(HudDealingsVocab.ROAD_ROWS_NAME, true, false)
	if box == null:
		return rows
	for child in box.get_children():
		if child is Control and (child as Control).has_meta(HudDealingsVocab.ROAD_ROW_META):
			rows.append(child)
	return rows

func _road_row(rows: Array, entity: int) -> Control:
	for row in rows:
		if int((row as Control).get_meta(HudDealingsVocab.ROAD_ROW_META)) == entity:
			return row
	return null

## A party link (`jump` / `recall`) on a road row or a dealing row, by its meta.
func _party_link(root: Node, which: String) -> Button:
	if root == null:
		return null
	if root is Button and root.has_meta(HudDealingsVocab.PARTY_LINK_META) \
			and String(root.get_meta(HudDealingsVocab.PARTY_LINK_META)) == which:
		return root as Button
	for child in root.get_children():
		var found := _party_link(child, which)
		if found != null:
			return found
	return null


# ---- (a2) A REPLY ---------------------------------------------------------------------------------------

func _reply_states(panel: BandCityPanel) -> void:
	var rows := _rows(_block(panel))
	var incoming := _row(rows, REPLY_IN)
	var in_text := _text_of(incoming)
	h._assert_hud("reply: the one this band answers reads \"<band> would give back <return> for your <cargo>\" (\"%s\")" % in_text.left(90),
		in_text.contains("%s would give back 5.0 flint for your 9.0 food" % BRACK_NAME))
	h._assert_hud("reply: …with its turns left in amber",
		_find_label(incoming, HudDealingsVocab.TURNS_LEFT_ONE) != null
			and _find_label(incoming, HudDealingsVocab.TURNS_LEFT_ONE).get_theme_color("font_color") == HudStyle.WARN)
	var take := _action(incoming, HudDealingsVocab.ANSWER_ACCEPT)
	var walk := _action(incoming, HudDealingsVocab.ANSWER_REFUSE)
	h._assert_hud("reply: …offers `Take it` and `Reject it`, and NO counter",
		take != null and take.text == HudDealingsVocab.ACTION_TAKE_IT
			and walk != null and walk.text == HudDealingsVocab.ACTION_WALK_HOME
			and _action(incoming, HudDealingsVocab.ANSWER_COUNTER) == null)
	h._assert_hud("reply: …in two lines of copy", _copy_lines(incoming) == 2)
	var outgoing := _row(rows, REPLY_OUT)
	var out_text := _text_of(outgoing)
	h._assert_hud("reply: the one this band named waits: \"Waiting on <band>'s party to take your return of <return>\" (\"%s\")" % out_text.left(100),
		out_text.contains("Waiting on %s's party to take your return of 10.0 food" % THORN_NAME))
	h._assert_hud("reply: …offers no action, in two lines",
		_action(outgoing, HudDealingsVocab.ANSWER_ACCEPT) == null and _copy_lines(outgoing) == 2)
	await h._save("dealings_reply")
	# Take it: a bare accept, no return and no sheet.
	_commands.clear()
	if take != null:
		take.emit_signal("pressed")
	h._assert_hud("reply: Take it emits ONE command and opens no sheet", _commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var built := MAIN_SCRIPT.format_diplomacy_command(_commands[0])
		h._assert_hud("reply: …`answer_dealing 0 %d accept` with NO return list (got `%s`)" % [REPLY_IN, built.get("line", "")],
			String(built.get("line", "")) == "answer_dealing 0 %d accept" % REPLY_IN)
		h._assert_hud("reply: …whose receipt says the return was taken, not a gift",
			String(built.get("message", "")) == HudDealingsVocab.ANSWER_MESSAGE_TAKE_RETURN)
	_commands.clear()
	if walk != null:
		walk.emit_signal("pressed")
	h._assert_hud("reply: Reject it emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var built := MAIN_SCRIPT.format_diplomacy_command(_commands[0])
		h._assert_hud("reply: …`answer_dealing 0 %d refuse` (got `%s`)" % [REPLY_IN, built.get("line", "")],
			String(built.get("line", "")) == "answer_dealing 0 %d refuse" % REPLY_IN)
		h._assert_hud("reply: …whose receipt says the party walked home",
			String(built.get("message", "")) == HudDealingsVocab.ANSWER_MESSAGE_WALK_HOME)
	# Take the offer's own receipts unchanged: an ordinary refuse is still "Refused the dealing."
	var ordinary := MAIN_SCRIPT.format_answer_dealing({"dealing_id": 3, "answer": "refuse"})
	h._assert_hud("reply: …and an ordinary refuse keeps its own receipt",
		String(ordinary.get("message", "")) == HudDealingsVocab.ANSWER_MESSAGE_REFUSE)


# ---- (a3) THE HISTORY -----------------------------------------------------------------------------------

func _history_states(panel: BandCityPanel) -> void:
	var scroll := panel.find_child(TradeZoneController.TAB_SCROLL_NAME, true, false) as ScrollContainer
	# Scroll the tab to its foot, where the history sits.
	scroll.scroll_vertical = int(scroll.get_v_scroll_bar().max_value)
	await h._settle()
	await h._save("dealings_history")
	var block := panel.find_child(HudDealingsVocab.HISTORY_BLOCK_NAME, true, false) as Control
	h._assert_hud("history: the tab carries a History section", block != null)
	var rows := _history_rows(block)
	h._assert_hud("history: every record of THIS band is listed, none of another's (%d of 8)" % rows.size(),
		rows.size() == 8)
	var turns: Array = []
	for row in rows:
		turns.append(int((row as Control).get_meta(HudDealingsVocab.HISTORY_ROW_META)))
	h._assert_hud("history: …newest first (%s)" % str(turns), _is_non_increasing(turns))
	var traded := _history_row(rows, HISTORY_TURN_TRADED)
	var traded_text := _text_of(traded)
	h._assert_hud("history: a trade reads `Turn n · <counterparty> · Traded` (\"%s\")" % traded_text.left(80),
		traded_text.contains("Turn %d · %s · %s" % [HISTORY_TURN_TRADED, "%s · %s" % [BRACK_PEOPLE, BRACK_NAME],
			"Traded"]) or (traded_text.contains("Turn %d" % HISTORY_TURN_TRADED)
			and traded_text.contains("%s · %s" % [BRACK_PEOPLE, BRACK_NAME]) and traded_text.contains("Traded")))
	h._assert_hud("history: …and what moved each way, `gave 5.0 food · got 2.0 hide`",
		traded_text.contains("gave 5.0 food · got 2.0 hide"))
	var declined_text := _text_of(_history_row(rows, HISTORY_TURN_DECLINED))
	h._assert_hud("history: a deal that did not happen says what was OFFERED for what (\"%s\")" % declined_text.left(90),
		declined_text.contains("Declined") and declined_text.contains("offered 3.0 flint for 4.0 food"))
	var refused_text := _text_of(_history_row(rows, HISTORY_TURN_REFUSED))
	h._assert_hud("history: …an empty side is left out (`offered 8.0 food`)",
		refused_text.contains("Refused") and refused_text.contains("offered 8.0 food")
			and not refused_text.contains(" for "))
	var gift_text := _text_of(_history_row(rows, HISTORY_TURN_GIFT_GIVEN))
	h._assert_hud("history: a gift given says `gave`, and nothing came back (\"%s\")" % gift_text.left(70),
		gift_text.contains("Gift given") and gift_text.contains("gave 6.0 hay") and not gift_text.contains("got"))
	var received_text := _text_of(_history_row(rows, HISTORY_TURN_GIFT_RECEIVED))
	h._assert_hud("history: a gift received says `got`",
		received_text.contains("Gift received") and received_text.contains("got 2.0 hide")
			and not received_text.contains("gave"))
	h._assert_hud("history: the other outcome words read Lapsed, Withdrawn and Failed",
		_text_of(_history_row(rows, HISTORY_TURN_EXPIRED)).contains("Lapsed")
			and _text_of(block).contains("Withdrawn") and _text_of(block).contains("Failed"))
	for row in rows:
		h._assert_hud("history: a row is at most two lines (%s)" % _text_of(row).left(30),
			_copy_lines(row) <= 2)
	var word := _find_meta_all(traded, HudDealingsVocab.HISTORY_ROW_META).filter(
		func(n: Node) -> bool: return n is Label)
	var danger := _find_meta_all(_history_row(rows, HISTORY_TURN_DECLINED), HudDealingsVocab.HISTORY_ROW_META).filter(
		func(n: Node) -> bool: return n is Label)
	h._assert_hud("history: the outcome word wears its ink (a trade healthy, a refusal danger)",
		word.size() == 1 and (word[0] as Label).get_theme_color("font_color") == HudStyle.HEALTHY
			and danger.size() == 1 and (danger[0] as Label).get_theme_color("font_color") == HudStyle.DANGER)
	# The receiving end of a SAME-PEOPLE shipment reads the one record from its own seat.
	var record := {"turn": 40, "own_band": 5, "other_faction": HudConst.PLAYER_FACTION_ID, "other_band": 9,
		"outcome": HudDealingsVocab.OUTCOME_GIFT_GIVEN, "goods_out": _goods(4.0), "goods_in": _goods()}
	var sender_view := TradeHistoryRoster.view_of(record, 5)
	var receiver_view := TradeHistoryRoster.view_of(record, 9)
	h._assert_hud("history: a same-people shipment is ONE record, read from both ends (sender gives, receiver gets)",
		int(sender_view["band"]) == 9 and sender_view["outcome"] == HudDealingsVocab.OUTCOME_GIFT_GIVEN
			and TradeHistoryRoster.detail_text(sender_view) == "gave 4.0 food"
			and int(receiver_view["band"]) == 5 and receiver_view["outcome"] == HudDealingsVocab.OUTCOME_GIFT_RECEIVED
			and TradeHistoryRoster.detail_text(receiver_view) == "got 4.0 food")
	# A frame that moves only the history re-fills the list in place.
	var scroll_id := scroll.get_instance_id()
	var offset := scroll.scroll_vertical
	var history := _history()
	history.append(_record(HISTORY_TURN_ADDED, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
		HudDealingsVocab.OUTCOME_EXCHANGED, _goods(1.0), _goods(0.0, 0.0, [_flat_hide(1.0)])))
	h._hud.update_trade_history(history)
	await h._settle()
	var after := panel.find_child(TradeZoneController.TAB_SCROLL_NAME, true, false) as ScrollContainer
	h._assert_hud("history: a history-only frame refills the list IN PLACE (same scroll, same offset, one more row)",
		after != null and after.get_instance_id() == scroll_id and after.scroll_vertical == offset
			and _history_rows(panel.find_child(HudDealingsVocab.HISTORY_BLOCK_NAME, true, false)).size() == 9)
	# Empty states: no history says so, under its head; so does an empty waiting list.
	h._hud.update_trade_history([])
	h._hud.update_dealings(_dealings().filter(func(d: Dictionary) -> bool: return int(d["id"]) == TREATY_IN))
	await h._settle()
	await h._save("dealings_tab_empty")
	var empty_history := panel.find_child(HudDealingsVocab.HISTORY_BLOCK_NAME, true, false) as Control
	h._assert_hud("history: with no records the section keeps its head and says `No trades yet`",
		empty_history != null and _text_of(empty_history).contains(HudDealingsVocab.HISTORY_EMPTY)
			and _history_rows(empty_history).is_empty())
	var empty_waiting := _block(panel)
	h._assert_hud("dealings: with nothing waiting the section keeps its head and says `Nothing waiting`",
		empty_waiting != null and _text_of(empty_waiting).contains(HudDealingsVocab.EMPTY)
			and _rows(empty_waiting).is_empty() and _has(_text_of(empty_waiting), HudDealingsVocab.HEAD))
	h._assert_hud("dealings: …and the Trade badge is gone (a treaty proposal is not a trade)",
		String(panel._tab_badges.get(BandCityPanel.ZONE_TRADE, {}).get("text", "x")) == "")
	h._hud.update_trade_history(_history())
	h._hud.update_dealings(_dealings())
	await h._settle()


# ---- (a4) TREATY PROPOSALS, ON THE PEOPLES TAB ------------------------------------------------------

func _peoples_treaty_states(panel: BandCityPanel) -> void:
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	await h._save("dealings_peoples_treaty")
	var block := panel.find_child(HudDealingsVocab.TREATY_BLOCK_NAME, true, false) as Control
	h._assert_hud("treaty: the Peoples tab carries the treaty proposal under `Waiting on an answer`",
		block != null and _has(_text_of(block), HudDealingsVocab.HEAD))
	var rows := _rows(block)
	h._assert_hud("treaty: …just the one treaty proposal (%d)" % rows.size(),
		_row_ids(rows) == [TREATY_IN])
	var treaty := _row(rows, TREATY_IN)
	var treaty_text := _text_of(treaty)
	h._assert_hud("treaty: it reads its terms in words (\"%s\")" % treaty_text.left(90),
		treaty_text.contains("Road rights") and treaty_text.contains("Open borders")
			and treaty_text.contains("Agreed trade: 5.0 flint for 10.0 food"))
	h._assert_hud("treaty: …says an envoy brought it on its hover",
		treaty.tooltip_text.contains(HudDealingsVocab.TREATY_ENVOY_TOOLTIP))
	h._assert_hud("treaty: …and keeps accept, refuse AND counter",
		_action(treaty, HudDealingsVocab.ANSWER_ACCEPT) != null
			and _action(treaty, HudDealingsVocab.ANSWER_REFUSE) != null
			and _action(treaty, HudDealingsVocab.ANSWER_COUNTER) != null)
	h._assert_hud("treaty: it is the FIRST thing on the tab, above the regard block",
		block.get_index() == 0 or block.get_parent().get_child(0) == block)
	h._assert_hud("treaty: the Peoples badge counts the proposals awaiting this band's answer, hot",
		String(panel._tab_badges.get(BandCityPanel.ZONE_PEOPLES, {}).get("text", "")) == "1"
			and bool(panel._tab_badges.get(BandCityPanel.ZONE_PEOPLES, {}).get("hot", false)))
	_commands.clear()
	var accept := _action(treaty, HudDealingsVocab.ANSWER_ACCEPT)
	if accept != null:
		accept.emit_signal("pressed")
	h._assert_hud("treaty: Accept emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("treaty: …`answer_dealing 0 %d accept` (got `%s`)" % [TREATY_IN, line],
			line == "answer_dealing 0 %d accept" % TREATY_IN)
	# With none pending the block is absent and the badge falls back to the tie count.
	h._hud.update_dealings(_dealings().filter(func(d: Dictionary) -> bool: return int(d["id"]) != TREATY_IN))
	await h._settle()
	h._assert_hud("treaty: with none pending the block is not drawn",
		panel.find_child(HudDealingsVocab.TREATY_BLOCK_NAME, true, false) == null)
	h._assert_hud("treaty: …and the Peoples badge is the tie count again, not hot",
		String(panel._tab_badges.get(BandCityPanel.ZONE_PEOPLES, {}).get("text", "")) == "3"
			and not bool(panel._tab_badges.get(BandCityPanel.ZONE_PEOPLES, {}).get("hot", false)))
	h._hud.update_dealings(_dealings())
	await h._settle()
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	h._hud.show_unit_selection(_home_band())
	await h._settle()


# ---- (b) THE ANSWER SHEET ------------------------------------------------------------------------

func _answer_sheet_states(panel: BandCityPanel) -> void:
	# Refuse is one press and no sheet.
	_commands.clear()
	var refuse := _action(_row(_rows(_block(panel)), PROPOSAL_IN), HudDealingsVocab.ANSWER_REFUSE)
	if refuse != null:
		refuse.emit_signal("pressed")
	h._assert_hud("answer: refusing a proposal emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("answer: …spelled `answer_dealing 0 12 refuse` (got `%s`)" % line,
			line == "answer_dealing 0 %d refuse" % PROPOSAL_IN)
	# Accept on a proposal is one press and no sheet either: the terms are written.
	_commands.clear()
	var accept_proposal := _action(_row(_rows(_block(panel)), PROPOSAL_IN), HudDealingsVocab.ANSWER_ACCEPT)
	if accept_proposal != null:
		accept_proposal.emit_signal("pressed")
	h._assert_hud("answer: accepting a proposal as written emits ONE command and opens no sheet",
		_commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("answer: …spelled `answer_dealing 0 12 accept` (got `%s`)" % line,
			line == "answer_dealing 0 %d accept" % PROPOSAL_IN)

	# ---- the OFFER's sheet: pick a return from the holdings -----------------------------------------
	_commands.clear()
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var accept := _action(_row(_rows(_block(panel)), OFFER_IN), HudDealingsVocab.ANSWER_ACCEPT)
	h._assert_hud("answer: the offer's Accept is live", accept != null and not accept.disabled)
	if accept != null:
		accept.emit_signal("pressed")
	await h._settle()
	await h._save("dealings_answer_offer")
	var form := _form()
	h._assert_hud("answer: Accept on an offer opens the answer sheet in the receiving band's drawer",
		form != null and String(form.get_meta(HudWidgets.VERB_FORM_META, ""))
			== HudDealingsVocab.COMPOSE_MISSION_DEALING)
	var sheet := _text_of(form)
	h._assert_hud("answer: …titled with the people (\"%s\")" % (HudDealingsVocab.SHEET_TITLE_OFFER_FORMAT % BRACK_PEOPLE),
		sheet.contains(HudDealingsVocab.SHEET_TITLE_OFFER_FORMAT % BRACK_PEOPLE))
	h._assert_hud("answer: …saying what they offer, rated", sheet.contains("12.0 food")
		and sheet.contains("hide (tough: fair, supple: good)"))
	h._assert_hud("answer: …with the SHIPMENT'S OWN picker over this band's holdings (food, hay, materials)",
		sheet.contains(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) and sheet.contains("Hay")
			and sheet.contains("Hide"))
	var cap := float(OFFER_PARTY) * CARRY_PER_WORKER
	h._assert_hud("answer: …bounded by the waiting party's carry (%.1f)" % cap,
		sheet.contains(HudDealingsVocab.SHEET_LIMIT_FORMAT % (HudCraftingVocab.BATCH_AMOUNT_FORMAT % cap)))
	var gift := Q.find_meta_node(form, HudDealingsVocab.SHEET_ACTION_META) as Button
	h._assert_hud("answer: an EMPTY return is allowed and the button says it reads as a gift",
		gift != null and gift.text == HudDealingsVocab.SHEET_ACCEPT_GIFT_BUTTON and not gift.disabled)
	_load(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, RETURN_FOOD)
	await h._settle()
	await h._save("dealings_answer_offer_return")
	var paid := Q.find_meta_node(_form(), HudDealingsVocab.SHEET_ACTION_META) as Button
	h._assert_hud("answer: loading a return renames the button plain Accept",
		paid != null and paid.text == HudDealingsVocab.SHEET_ACCEPT_BUTTON)
	_commands.clear()
	if paid != null:
		paid.emit_signal("pressed")
	await h._settle()
	h._assert_hud("answer: the sheet emits ONE answer and closes", _commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		var expected := "answer_dealing 0 %d accept food %s" % [OFFER_IN,
			MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(RETURN_FOOD))]
		h._assert_hud("answer: …`%s` (got `%s`)" % [expected, line], line == expected)

	# ---- a proposal's COUNTER sheet: both legs editable ----------------------------------------------
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var counter := _action(_row(_rows(_block(panel)), PROPOSAL_IN), HudDealingsVocab.ANSWER_COUNTER)
	if counter != null:
		counter.emit_signal("pressed")
	await h._settle()
	await h._save("dealings_counter_proposal")
	var counter_form := _form()
	var counter_text := _text_of(counter_form)
	h._assert_hud("counter: the sheet opens on the proposal",
		counter_form != null and counter_text.contains(HudDealingsVocab.SHEET_TITLE_COUNTER_FORMAT % THORN_PEOPLE))
	h._assert_hud("counter: …the leg this band holds is the picker, prefilled with the 10.0 food asked of it",
		_cargo_amount(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == PROPOSAL_FOOD)
	h._assert_hud("counter: …and their leg is named, prefilled with the 5.0 flint they send",
		_has(counter_text, HudDealingsVocab.SHEET_THEIR_LEG_SECTION)
			and counter_text.contains("flint") and counter_text.contains("5.0"))
	h._assert_hud("counter: …editable: each good steps, removes, and `Add a good` offers the rest",
		_find_meta_all(counter_form, HudDealingsVocab.LEG_STEP_META).size() == 2
			and Q.find_meta_node(counter_form, HudDealingsVocab.LEG_ADD_META) != null)
	# One more flint, through the editor's own `+`.
	var more := _leg_step(counter_form, "material:flint", HudWorkVocab.STEPPER_PLUS_FACE)
	if more != null:
		more.emit_signal("pressed")
	await h._settle()
	await h._save("dealings_counter_edited")
	_commands.clear()
	var send := Q.find_meta_node(_form(), HudDealingsVocab.SHEET_ACTION_META) as Button
	h._assert_hud("counter: the edited counter has a live send", send != null and not send.disabled)
	if send != null:
		send.emit_signal("pressed")
	await h._settle()
	h._assert_hud("counter: the send emits ONE counter and closes the sheet",
		_commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		# `give` is the SHIPPER's leg whoever speaks: they are the shipper, so give = their flint.
		var food := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(PROPOSAL_FOOD))
		var flint := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(PROPOSAL_FLINT + 1.0))
		var expected := "answer_dealing 0 %d counter give material flint %s ask food %s" % [
			PROPOSAL_IN, flint, food]
		h._assert_hud("counter: …`%s` (got `%s`)" % [expected, line], line == expected)

	# ---- a counter to OUR proposal: we are the shipper, the legs swap seats -------------------------
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var counter_in := _action(_row(_rows(_block(panel)), COUNTER_IN), HudDealingsVocab.ANSWER_COUNTER)
	if counter_in != null:
		counter_in.emit_signal("pressed")
	await h._settle()
	h._assert_hud("counter: answering a counter to OUR proposal, the picker holds the GIVE leg (8.0 food)",
		_cargo_amount(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL) == 8.0
			and _has(_text_of(_form()), HudDealingsVocab.SHEET_YOUR_LEG_SECTION))
	h._hud.close_verb_form()
	await h._settle()

	# ---- the TREATY counter: tick the terms (the proposal waits on the Peoples tab) ----------------------
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var treaty_block := panel.find_child(HudDealingsVocab.TREATY_BLOCK_NAME, true, false) as Control
	var treaty_counter := _action(_row(_rows(treaty_block), TREATY_IN), HudDealingsVocab.ANSWER_COUNTER)
	if treaty_counter != null:
		treaty_counter.emit_signal("pressed")
	await h._settle()
	await h._save("dealings_treaty_counter")
	var treaty_form := _form()
	var treaty_text := _text_of(treaty_form)
	h._assert_hud("treaty: the counter lists each proposed term to tick (\"%s\")" % treaty_text.left(80),
		treaty_text.contains(HudDealingsVocab.SHEET_TREATY_ROAD)
			and treaty_text.contains(HudDealingsVocab.SHEET_TREATY_BORDERS)
			and treaty_text.contains("Keep: Agreed trade"))
	var road := _check_named(treaty_form, HudDealingsVocab.SHEET_TREATY_ROAD)
	if road != null:
		road.button_pressed = false
	await h._settle()
	_commands.clear()
	var treaty_send := Q.find_meta_node(_form(), HudDealingsVocab.SHEET_ACTION_META) as Button
	if treaty_send != null:
		treaty_send.emit_signal("pressed")
	await h._settle()
	h._assert_hud("treaty: the counter emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		var sent := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(PROPOSAL_FLINT))
		var returned := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(PROPOSAL_FOOD))
		var expected := "answer_dealing 0 %d counter open_borders bundle give material flint %s ask food %s" % [
			TREATY_IN, sent, returned]
		h._assert_hud("treaty: …`%s` (got `%s`)" % [expected, line], line == expected)
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	# A sheet for a dealing the server answered closes itself.
	var again := _action(_row(_rows(_block(panel)), OFFER_IN), HudDealingsVocab.ANSWER_ACCEPT)
	if again != null:
		again.emit_signal("pressed")
	await h._settle()
	h._assert_hud("answer: the offer's sheet is open again", _form() != null)
	var remaining := _dealings()
	remaining = remaining.filter(func(d: Dictionary) -> bool: return int(d["id"]) != OFFER_IN)
	h._hud.update_dealings(remaining)
	await h._settle()
	h._assert_hud("answer: a sheet open on a dealing that has since lapsed closes itself", _form() == null)
	h._hud.update_dealings(_dealings())
	await h._settle()


# ---- (c) THE TRADE SHEET -------------------------------------------------------------------------

func _trade_sheet_states(panel: BandCityPanel) -> void:
	panel.set_active_tab(BandCityPanel.ZONE_PARTIES)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var trade_btn := _verb_button(HudComposeVocab.VERB_TRADE)
	h._assert_hud("trade: the drawer offers the Trade verb", trade_btn != null)
	if trade_btn != null:
		trade_btn.emit_signal("pressed")
	await h._settle()
	var modes := _choice_buttons()
	h._assert_hud("trade: the sheet offers Gift, Offer and Propose, in that order",
		modes.size() == 3 and String(modes[0].get_meta(HudDealingsVocab.CHOICE_META)) == "gift"
			and String(modes[2].get_meta(HudDealingsVocab.CHOICE_META)) == "propose")
	h._assert_hud("trade: Gift is the default", modes.size() == 3 and modes[0].button_pressed)
	h._assert_hud("trade: Propose is GREYED with its reason until a destination is picked (\"%s\")" % (
			modes[2].tooltip_text if modes.size() == 3 else ""),
		modes.size() == 3 and modes[2].disabled
			and modes[2].tooltip_text == HudDealingsVocab.TRADE_PROPOSE_NEEDS_DESTINATION)
	await h._save("dealings_trade_modes")
	# Offer mode, on a destination with NO standing link (the Gorsemoor tie).
	_choice_button("offer").emit_signal("pressed")
	await h._settle()
	_load(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, OFFER_HIDE)
	h._hud.notify_targeting_click(_tile_info(MOOR_SEEN))
	await h._settle()
	h._assert_hud("trade: a destination with no published standing link keeps Propose greyed, saying why",
		_choice_button("propose").disabled
			and _choice_button("propose").tooltip_text == HudDealingsVocab.TRADE_MODE_PROPOSE_NO_LINK)
	var offer_send := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_TRADE_CONFIRM_META) as Button
	h._assert_hud("trade: Offer mode names its button", offer_send != null
		and offer_send.text == HudDealingsVocab.TRADE_SEND_OFFER_BUTTON)
	var shipped: Array = []
	var record := func(payload: Dictionary) -> void: shipped.append(payload)
	h._hud.send_trade_expedition_requested.connect(record)
	if offer_send != null:
		offer_send.emit_signal("pressed")
	await h._settle()
	h._hud.send_trade_expedition_requested.disconnect(record)
	h._assert_hud("trade: Offer mode ships ONE shipment", shipped.size() == 1)
	if shipped.size() == 1:
		var line := String(MAIN_SCRIPT.format_send_trade_expedition(shipped[0]).get("line", ""))
		h._assert_hud("trade: …whose line ends with the `offer` word (got `%s`)" % line,
			line.ends_with(" offer") and line.contains("food "))
		var gift_payload: Dictionary = shipped[0].duplicate()
		gift_payload["mode"] = HudDealingsVocab.TRADE_MODE_GIFT
		var gift_line := String(MAIN_SCRIPT.format_send_trade_expedition(gift_payload).get("line", ""))
		h._assert_hud("trade: …and a gift is the same line with nothing after the manifest (`%s`)" % gift_line,
			not gift_line.ends_with(" offer") and gift_line == line.trim_suffix(" offer"))

	# ---- the PROPOSE mode, on the destination the sim published a standing link to ----------------------
	trade_btn = _verb_button(HudComposeVocab.VERB_TRADE)
	if trade_btn != null:
		trade_btn.emit_signal("pressed")
	await h._settle()
	h._hud.notify_targeting_click(_tile_info(BRACK_SEEN))
	await h._settle()
	var propose := _choice_button("propose")
	h._assert_hud("trade: with a published link Propose is LIVE", propose != null and not propose.disabled)
	if propose != null:
		propose.emit_signal("pressed")
	await h._settle()
	_set_party(OFFER_PARTY)
	await h._settle()
	_load(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, 6.0)
	await h._settle()
	var sheet := _text_of(h._hud.allocation_panel)
	h._assert_hud("trade: propose mode splits the sheet into what we give and what we ask for",
		_has(sheet, HudDealingsVocab.TRADE_YOU_GIVE_SECTION) and _has(sheet, HudDealingsVocab.TRADE_YOU_ASK_SECTION))
	h._assert_hud("trade: …the ask list starts empty and says what to do",
		sheet.contains(HudDealingsVocab.TRADE_ASK_EMPTY))
	var blocked := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_TRADE_CONFIRM_META) as Button
	await h._save("dealings_trade_propose")
	# Name an ask through the editor's menu, then step it up.
	_add_leg_good(HudDealingsVocab.LEG_KEY_MATERIAL_PREFIX + "flint")
	await h._settle()
	var step_up := _leg_step(h._hud.allocation_panel, "material:flint", HudWorkVocab.STEPPER_PLUS_FACE)
	h._assert_hud("trade: Add a good puts flint on the ask list, with its own stepper", step_up != null)
	if step_up != null:
		step_up.emit_signal("pressed")
	await h._settle()
	await h._save("dealings_trade_propose_ask")
	var propose_send := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_TRADE_CONFIRM_META) as Button
	h._assert_hud("trade: the proposal's send is live and named for what it does",
		propose_send != null and not propose_send.disabled
			and propose_send.text == HudDealingsVocab.TRADE_PROPOSE_BUTTON)
	_commands.clear()
	if propose_send != null:
		propose_send.emit_signal("pressed")
	await h._settle()
	h._assert_hud("trade: it emits ONE proposal and closes the sheet", _commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		var expected := "propose_trade 0 %d %d %d give food %s ask material flint %s" % [
			BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY, OFFER_PARTY,
			BandFx.FIXTURE_BAND_ID_OFFSET + BRACK_ENTITY,
			MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(6.0)),
			MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(2.0))]
		h._assert_hud("trade: …`%s` (got `%s`)" % [expected, line], line == expected)
	# The link going away drops the mode back to a gift rather than leaving a mode it cannot send.
	trade_btn = _verb_button(HudComposeVocab.VERB_TRADE)
	if trade_btn != null:
		trade_btn.emit_signal("pressed")
	await h._settle()
	h._hud.notify_targeting_click(_tile_info(BRACK_SEEN))
	await h._settle()
	_choice_button("propose").emit_signal("pressed")
	await h._settle()
	h._hud.update_treaties({"treaties": [], "standing_links": []})
	await h._settle()
	h._assert_hud("trade: when the sim stops publishing the link the sheet falls back to a gift",
		_choice_button("gift") != null and _choice_button("gift").button_pressed
			and _choice_button("propose").disabled)
	h._hud.update_treaties(_treaties())
	h._hud.close_verb_form()
	await h._settle()


# ---- (d) THE SCOUT SHEET -------------------------------------------------------------------------

func _scout_sheet_states(panel: BandCityPanel) -> void:
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var scout_btn := _verb_button(HudComposeVocab.VERB_SCOUT)
	h._assert_hud("scout: the drawer offers the Scout verb", scout_btn != null)
	if scout_btn != null:
		scout_btn.emit_signal("pressed")
	await h._settle()
	var orders := _choice_buttons()
	h._assert_hud("scout: the sheet offers Keep, Give and Offer, with Keep the default",
		orders.size() == 3 and orders[0].button_pressed
			and String(orders[1].get_meta(HudDealingsVocab.CHOICE_META)) == "give")
	h._assert_hud("scout: …and the pack is the shipment's own picker",
		_has(_text_of(h._hud.allocation_panel), HudDealingsVocab.SCOUT_PACK_SECTION)
			and _has(_text_of(h._hud.allocation_panel), HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL))
	await h._save("dealings_scout_sheet")
	# Give needs a pack: greyed with its reason when empty.
	_choice_button("give").emit_signal("pressed")
	await h._settle()
	var blocked := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_TRADE_CONFIRM_META) as Button
	h._assert_hud("scout: Give with an empty pack cannot be sent, and says why",
		blocked != null and blocked.disabled
			and blocked.tooltip_text == HudDealingsVocab.SCOUT_STRANGERS_NEEDS_PACK)
	await h._save("dealings_scout_give_empty")
	_load(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL, RETURN_FOOD)
	await h._settle()
	await h._save("dealings_scout_pack")
	var send := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_EXPEDITION_CONFIRM_META) as Button
	h._assert_hud("scout: with a pack the send is live", send != null and not send.disabled)
	var scouted: Array = []
	var record := func(payload: Dictionary) -> void: scouted.append(payload)
	h._hud.send_expedition_requested.connect(record)
	if send != null:
		send.emit_signal("pressed")
	await h._settle()
	h._hud.notify_targeting_click(_tile_info(THORN_SEEN))
	await h._settle()
	h._hud.send_expedition_requested.disconnect(record)
	h._assert_hud("scout: the armed pick's click sends ONE expedition", scouted.size() == 1)
	if scouted.size() == 1:
		var line := String(MAIN_SCRIPT.format_send_expedition(scouted[0]).get("line", ""))
		var expected := "send_expedition 0 %d 1 %d %d strangers give pack food %s" % [
			BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY, THORN_SEEN.x, THORN_SEEN.y,
			MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(RETURN_FOOD))]
		h._assert_hud("scout: …`%s` (got `%s`)" % [expected, line], line == expected)
		var plain: Dictionary = scouted[0].duplicate()
		plain["strangers"] = HudDealingsVocab.STRANGERS_KEEP
		plain["pack"] = []
		h._assert_hud("scout: …and a scout that touched neither row emits the line it always did",
			String(MAIN_SCRIPT.format_send_expedition(plain).get("line", ""))
				== "send_expedition 0 %d 1 %d %d" % [BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY,
					THORN_SEEN.x, THORN_SEEN.y])
	h._hud.close_verb_form()
	await h._settle()


# ---- (e) THE FACTION PAGE ------------------------------------------------------------------------

func _faction_page_states(panel: BandCityPanel) -> void:
	h._hud.update_faction_policies([{"faction": 0, "open_borders": true, "accept_own_gifts": true,
		"border_overrides": []}])
	panel.set_active_tab(BandCityPanel.ZONE_BAND)
	h._hud._bandpanel.render_faction()
	await h._settle()
	await h._save("dealings_faction_page")
	h._assert_hud("faction: the gift setting is NOT on the faction page (it is a dealings setting)",
		Q.find_meta_node(panel, HudDealingsVocab.GIFT_TOGGLE_META) == null
			and Q.find_meta_node(panel, FactionRollup.OPEN_BORDERS_TOGGLE_META) != null)
	# ---- the setting, at the top of the Trade tab's waiting section --------------------------------
	panel.set_active_tab(BandCityPanel.ZONE_TRADE)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	await h._save("dealings_gift_setting")
	var toggle := Q.find_meta_node(panel, HudDealingsVocab.GIFT_TOGGLE_META) as CheckBox
	h._assert_hud("gifts: the Trade tab carries the setting, labelled in the player's words, on by default",
		toggle != null and toggle.text == HudDealingsVocab.ACCEPT_GIFTS_LABEL and toggle.button_pressed)
	h._assert_hud("gifts: …above the waiting list (the head, then the setting, then the rows)",
		toggle != null and toggle.get_index() == 1)
	h._assert_hud("gifts: …and its hover says what turning it on does",
		toggle != null and toggle.tooltip_text == HudDealingsVocab.ACCEPT_GIFTS_HINT)
	_commands.clear()
	if toggle != null:
		toggle.button_pressed = false
	h._assert_hud("gifts: turning it off emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("gifts: …`set_accept_own_gifts 0 off` (got `%s`)" % line,
			line == "set_accept_own_gifts 0 off")
	h._assert_hud("gifts: a payload with no state builds no command",
		MAIN_SCRIPT.format_diplomacy_command({"command": HudDealingsVocab.COMMAND_ACCEPT_OWN_GIFTS}).is_empty())
	h._hud.update_faction_policies([{"faction": 0, "open_borders": true, "accept_own_gifts": false,
		"border_overrides": []}])
	await h._settle()
	var off := Q.find_meta_node(panel, HudDealingsVocab.GIFT_TOGGLE_META) as CheckBox
	h._assert_hud("gifts: the box follows the SNAPSHOT, refilled in place (off when the sim says off)",
		off != null and not off.button_pressed and _rows(_block(panel)).size() == 6)
	await h._save("dealings_gift_setting_off")
	h._hud.update_faction_policies([{"faction": 0, "open_borders": true, "accept_own_gifts": true,
		"border_overrides": []}])
	await h._settle()


# ---- (f) THE TURN ORB ----------------------------------------------------------------------------

func _orb_states() -> void:
	var entries: Array = h._hud.turn_orb._entries
	var dealing_rows: Array = entries.filter(func(e: Dictionary) -> bool:
		return String(e.get("kind", "")) == HudDealingsVocab.ATTENTION_KIND_DEALING)
	h._assert_hud("orb: a pending item addressed to the player is ONE attention row per receiving band (%d)"
		% dealing_rows.size(), dealing_rows.size() == 1)
	if dealing_rows.size() == 1:
		var row: Dictionary = dealing_rows[0]
		h._assert_hud("orb: …reading `%s` (5 items wait on this band)" % (
				HudDealingsVocab.ATTENTION_LABEL_MANY_FORMAT % 5),
			String(row.get("label", "")) == HudDealingsVocab.ATTENTION_LABEL_MANY_FORMAT % 5)
		h._assert_hud("orb: …non-locating, carrying its band as the panel subject",
			int(row.get("x", 0)) < 0 and int(row.get(HudAttentionVocab.ATTENTION_PANEL_SUBJECT, 0))
				== BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY)
		h._assert_hud("orb: …warn, and NOT blocking (silence is the sim's refusal)",
			String(row.get("severity", "")) == HudAttentionVocab.ATTENTION_SEVERITY_WARN
				and not bool(row.get("blocking", false)))
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("dealings_orb")
	var popover_rows := Q.turn_orb_popover_rows(h._hud.turn_orb)
	var shown := popover_rows.filter(func(r: Dictionary) -> bool:
		return String(r.get("label", "")).contains("await your answer"))
	h._assert_hud("orb: the popover shows the row with an `Open` affordance (%d)" % shown.size(),
		shown.size() == 1 and not String(shown[0].get("jump", "")).is_empty())
	h._hud.turn_orb.toggle_popover()
	# Pressing the row brings the receiving band's Trade tab forward.
	h._hud._turnorb._on_turn_orb_panel_requested(HudDealingsVocab.ATTENTION_KIND_DEALING,
		BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY)
	await h._settle()
	h._assert_hud("orb: pressing the row focuses that band's Trade tab",
		h._hud._bandpanel._panel._active_tab == BandCityPanel.ZONE_TRADE)
	# Answered, the row goes.
	h._hud.update_dealings([])
	await h._settle()
	var after: Array = h._hud.turn_orb._entries.filter(func(e: Dictionary) -> bool:
		return String(e.get("kind", "")) == HudDealingsVocab.ATTENTION_KIND_DEALING)
	h._assert_hud("orb: once nothing waits the row is gone", after.is_empty())
	h._assert_hud("dealings: an empty list says so", _text_of(h._hud._bandpanel._panel).contains(HudDealingsVocab.EMPTY))
	h._hud.update_dealings(_dealings())
	await h._settle()


# ---- (g) THE EVENT DOCK --------------------------------------------------------------------------

func _feed_states() -> void:
	var dock: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	h.add_child(dock)
	await h.get_tree().process_frame
	dock.set_perpendicular_insets(h._hud.left_column_width(), h._hud.right_column_width())
	dock.reset()
	var brack_id := BandFx.FIXTURE_BAND_ID_OFFSET + BRACK_ENTITY
	var home_id := BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY
	dock.set_band_labels({str(home_id): HOME_NAME, str(brack_id): BRACK_NAME})
	var sim_brack := "band %d" % brack_id
	var offered := "A party from %s offers 12.00 food" % sim_brack
	var posted := "Your shipment is waiting on %s to answer (4.00 hide)" % sim_brack
	var accepted := "%s accepted your offer and sent back 3.00 food" % sim_brack
	var refused := "%s refused your proposal" % sim_brack
	var expired := "The offer from %s lapsed unanswered" % sim_brack
	var withdrawn := "The road to %s is broken — the proposal from them was withdrawn" % sim_brack
	var settled := "%s paid 10.00 food for the shipment" % sim_brack
	var failed := "%s no longer holds the agreed return — your party is coming home with its cargo" % sim_brack
	var reply_offered := "%s would give back 5.00 flint for your 9.00 food: take it or walk home" % sim_brack
	var reply_posted := "Your return of 5.00 flint is waiting on %s's party to take it" % sim_brack
	var reply_declined := "%s's party walked home with its cargo" % sim_brack
	var reply_withdrawn := "You took back your return of 5.00 flint"
	var treaty_offered := "A treaty proposal from %s: road rights, open borders" % sim_brack
	var treaty_formed := "%s accepted your treaty proposal" % sim_brack
	var breach_against := "Faction %d ended its treaty with you" % BRACK_FACTION
	var breach_by_us := "You ended your treaty with Faction %d" % BRACK_FACTION
	dock.ingest_events([
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": offered,
			"detail": "status=offered dealing=11 kind=offer from=%d sender=%d band=%d" % [
				BRACK_FACTION, brack_id, home_id], "seq": 801, "band": home_id},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": posted,
			"detail": "status=posted dealing=13 kind=offer destination=%d expedition=7" % brack_id,
			"seq": 802, "band": home_id},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": accepted,
			"detail": "status=accepted dealing=13 kind=offer destination=%d" % brack_id, "seq": 803},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": refused,
			"detail": "status=refused dealing=12 kind=proposal destination=%d" % brack_id, "seq": 804},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": expired,
			"detail": "status=expired dealing=15 kind=offer from=%d sender=%d band=%d" % [
				BRACK_FACTION, brack_id, home_id], "seq": 805},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": withdrawn,
			"detail": "status=withdrawn dealing=12 kind=proposal from=%d sender=%d band=%d reason=link_broken" % [
				BRACK_FACTION, brack_id, home_id], "seq": 806},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": settled,
			"detail": "status=settled dealing=16 kind=exchange destination=%d" % brack_id, "seq": 807},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": failed,
			"detail": "status=failed dealing=16 kind=exchange destination=%d reason=receiver_short" % brack_id,
			"seq": 808},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": reply_offered,
			"detail": "status=offered dealing=16 kind=reply from=%d sender=%d band=%d" % [
				BRACK_FACTION, brack_id, home_id], "seq": 813, "band": home_id},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": reply_posted,
			"detail": "status=posted dealing=17 kind=reply destination=%d" % brack_id, "seq": 814},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": reply_declined,
			"detail": "status=declined dealing=17 kind=reply destination=%d" % brack_id, "seq": 815},
		{"tick": TURN, "kind": "dealing", "faction": 0, "label": reply_withdrawn,
			"detail": "status=withdrawn dealing=17 kind=reply destination=%d" % brack_id, "seq": 816},
		{"tick": TURN, "kind": "treaty", "faction": 0, "label": treaty_offered,
			"detail": "status=offered dealing=14 kind=treaty_proposal carrier=envoy from=%d sender=%d band=%d" % [
				BRACK_FACTION, brack_id, home_id], "seq": 809},
		{"tick": TURN, "kind": "treaty", "faction": 0, "label": treaty_formed,
			"detail": "status=accepted dealing=14 kind=treaty_proposal destination=%d" % brack_id, "seq": 810},
		{"tick": TURN, "kind": "treaty", "faction": 0, "label": breach_against,
			"detail": "status=ended reason=ended with=%d breaker=%d" % [BRACK_FACTION, BRACK_FACTION], "seq": 811},
		{"tick": TURN, "kind": "treaty", "faction": 0, "label": breach_by_us,
			"detail": "status=ended reason=ended with=%d breaker=0" % BRACK_FACTION, "seq": 812},
	])
	dock.set_expanded(true)
	await h._settle()
	await h._save("dealings_event_dock")
	h._assert_hud("feed: an offer addressed to you is NOTABLE (visible at the default floor)",
		_rung(dock, offered) == HudEventVocab.RUNG_NOTABLE and _rung(dock, treaty_offered) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("feed: a treaty broken against you is NOTABLE",
		_rung(dock, breach_against) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("feed: …and the one YOU ended is a routine receipt",
		_rung(dock, breach_by_us) == HudEventVocab.RUNG_ROUTINE)
	# **THE ANSWER IS NOTABLE**: how a dealing ended (accepted, refused, declined, lapsed, failed) and a
	# reply addressed to us; a post, a withdrawal and a settled receipt stay routine.
	for label in [accepted, refused, expired, failed, reply_offered, reply_declined]:
		h._assert_hud("feed: `%s` is NOTABLE" % String(label).left(48),
			_rung(dock, label) == HudEventVocab.RUNG_NOTABLE)
	for label in [posted, withdrawn, settled, treaty_formed, reply_posted, reply_withdrawn]:
		h._assert_hud("feed: `%s` is a routine line" % String(label).left(48),
			_rung(dock, label) == HudEventVocab.RUNG_ROUTINE)
	h._assert_hud("feed: `declined` and the reply's phrases read in English, not as a raw token",
		EventDockPanel.detail_phrase("status=declined dealing=17 kind=reply destination=1").contains("turned down")
			and EventDockPanel.detail_phrase("status=failed dealing=17 kind=reply reason=receiver_short").contains(
				"they no longer held the return")
			and EventDockPanel.detail_phrase("status=accepted dealing=17 kind=reply").contains("accepted")
			and EventDockPanel.detail_phrase("status=withdrawn dealing=17 kind=reply").contains("withdrawn"))
	h._assert_hud("feed: the sender band is named by the tie's remembered name, not `band N`",
		_label_count(dock, offered.replace(sim_brack, BRACK_NAME)) == 1)
	h._assert_hud("feed: …and the treaty's people by their published name",
		_label_count(dock, "%s ended its treaty with you" % BRACK_PEOPLE) == 1)
	for event in dock._events:
		var phrase := EventDockPanel.detail_phrase(String(event["detail"]))
		h._assert_hud("feed: no raw id or token in the detail (\"%s\")" % phrase,
			not phrase.contains("dealing") and not phrase.contains("sender") and not phrase.contains("=")
			and not phrase.contains(str(brack_id)))
	dock.set_band_labels({})
	dock.queue_free()
	await h.get_tree().process_frame
	await h._settle()


# ---- (h) THE COMMAND BUILDERS' REFUSALS -------------------------------------------------------------

func _command_states() -> void:
	h._assert_hud("commands: an answer with no dealing id builds nothing",
		MAIN_SCRIPT.format_answer_dealing({"answer": "refuse"}).is_empty())
	h._assert_hud("commands: an unknown answer builds nothing",
		MAIN_SCRIPT.format_answer_dealing({"dealing_id": 3, "answer": "shrug"}).is_empty())
	h._assert_hud("commands: a proposal with no destination or party builds nothing",
		MAIN_SCRIPT.format_propose_trade({"band_id": 1, "party_workers": 2}).is_empty())
	h._assert_hud("commands: an unknown diplomacy verb builds nothing",
		MAIN_SCRIPT.format_diplomacy_command({"command": "declare_war"}).is_empty())
	var terms := {"road_rights": true, "open_borders": false, "bundles": [{
		"sent": {"food": 0.0, "fodder": 0.0, "materials": [{"material_id": "flint", "amount": 5.0, "readings": []}]},
		"returned": {"food": 10.0, "fodder": 0.0, "materials": []}}]}
	var line := String(MAIN_SCRIPT.format_answer_dealing({"dealing_id": 9, "answer": "counter",
		"terms": terms}).get("line", ""))
	var flint := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(5.0))
	var food := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(10.0))
	h._assert_hud("commands: a treaty counter spells road rights and the bundle (`%s`)" % line,
		line == "answer_dealing 0 9 counter road_rights bundle give material flint %s ask food %s" % [flint, food])


# ---- finders -------------------------------------------------------------------------------------

func _block(panel: BandCityPanel) -> Control:
	return panel.find_child(HudDealingsVocab.BLOCK_NAME, true, false) as Control

func _rows(block: Node) -> Array:
	var rows: Array = []
	var box := DealingsRoster.rows_box(block)
	if box == null:
		return rows
	for child in box.get_children():
		if child is Control and (child as Control).has_meta(HudDealingsVocab.ROW_META):
			rows.append(child)
	return rows

func _collect_scrolls(node: Node, into: Array) -> void:
	if node == null:
		return
	if node is ScrollContainer:
		into.append(node)
	for child in node.get_children():
		_collect_scrolls(child, into)

func _history_rows(block: Node) -> Array:
	var rows: Array = []
	if block == null:
		return rows
	var box := block.find_child(HudDealingsVocab.HISTORY_ROWS_NAME, true, false)
	if box == null:
		return rows
	for child in box.get_children():
		if child is Control and (child as Control).has_meta(HudDealingsVocab.HISTORY_ROW_META):
			rows.append(child)
	return rows

func _history_row(rows: Array, turn: int) -> Control:
	for row in rows:
		if int((row as Control).get_meta(HudDealingsVocab.HISTORY_ROW_META)) == turn:
			return row
	return null

func _is_non_increasing(values: Array) -> bool:
	for i in range(1, values.size()):
		if int(values[i]) > int(values[i - 1]):
			return false
	return true

func _row_ids(rows: Array) -> Array:
	var ids: Array = []
	for row in rows:
		ids.append(int((row as Control).get_meta(HudDealingsVocab.ROW_META)))
	return ids

func _row(rows: Array, dealing_id: int) -> Control:
	for row in rows:
		if int((row as Control).get_meta(HudDealingsVocab.ROW_META)) == dealing_id:
			return row
	return null

## One action link on a row, by its answer id, or `null`.
func _action(row: Control, answer: String) -> Button:
	if row == null:
		return null
	return _find_action(row, answer)

func _find_action(root: Node, answer: String) -> Button:
	if root is Button and String((root as Button).get_meta(HudDealingsVocab.ACTION_META, "")) == answer:
		return root as Button
	for child in root.get_children():
		var found := _find_action(child, answer)
		if found != null:
			return found
	return null

## How many TEXT lines a row carries: its direct Labels plus its header's, not the strip's links.
func _copy_lines(row: Control) -> int:
	var lines := 0
	for child in row.get_children():
		if child is Label:
			lines += 1
		elif child is HBoxContainer and not (child as Control).has_meta(HudDealingsVocab.ACTION_META):
			lines += 1
	return lines

func _find_label(root: Node, needle: String) -> Label:
	if root is Label and (root as Label).text.contains(needle):
		return root as Label
	for child in root.get_children():
		var found := _find_label(child, needle)
		if found != null:
			return found
	return null

func _text_of(root: Node) -> String:
	var text := ""
	if root is Label:
		text += (root as Label).text + " "
	elif root is Button:
		text += (root as Button).text + " "
	elif root is RichTextLabel:
		text += (root as RichTextLabel).text + " "
	elif root is CheckBox:
		text += (root as CheckBox).text + " "
	for child in root.get_children():
		text += _text_of(child)
	return text

## A section label is uppercased by its widget, so a needle is matched without regard to case.
func _has(text: String, needle: String) -> bool:
	return text.to_lower().contains(needle.to_lower())

func _form() -> Control:
	return Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) as Control

func _verb_button(verb_id: StringName) -> Button:
	return _find_verb_button(h._hud.allocation_panel, verb_id)

func _find_verb_button(root: Node, verb_id: StringName) -> Button:
	if root == null:
		return null
	if root is Button and StringName((root as Button).get_meta(HudWidgets.VERB_BUTTON_META, &"")) == verb_id:
		return root as Button
	for child in root.get_children():
		var found := _find_verb_button(child, verb_id)
		if found != null:
			return found
	return null

func _tile_info(tile: Vector2i) -> Dictionary:
	return {"x": tile.x, "y": tile.y, "visibility_state": "active"}

func _choice_buttons() -> Array:
	var out: Array = []
	_collect_choices(h._hud.allocation_panel, out)
	return out

func _collect_choices(root: Node, out: Array) -> void:
	if root is Button and (root as Button).has_meta(HudDealingsVocab.CHOICE_META):
		out.append(root)
	for child in root.get_children():
		_collect_choices(child, out)

func _choice_button(id: String) -> Button:
	for button in _choice_buttons():
		if String((button as Button).get_meta(HudDealingsVocab.CHOICE_META)) == id:
			return button
	return null

func _find_meta_all(root: Node, meta: StringName) -> Array:
	var out: Array = []
	_collect_meta(root, meta, out)
	return out

func _collect_meta(root: Node, meta: StringName, out: Array) -> void:
	if root.has_meta(meta):
		out.append(root)
	for child in root.get_children():
		_collect_meta(child, meta, out)

## One of a leg row's step buttons: the `+` or `−` of the row keyed `key`.
func _leg_step(root: Node, key: String, face: String) -> Button:
	for node in _find_meta_all(root, HudDealingsVocab.LEG_STEP_META):
		if node is Button and String(node.get_meta(HudDealingsVocab.LEG_STEP_META)) == key \
				and (node as Button).text == face:
			return node
	return null

## Choose a good from the leg editor's `Add a good` menu by its key, through the popup's own item
## selection (`id_pressed`), the way a click on the entry does.
func _add_leg_good(key: String) -> void:
	var menu := Q.find_meta_node(h._hud.allocation_panel, HudDealingsVocab.LEG_ADD_META) as MenuButton
	h._assert_hud("dealings: the ask editor offers `Add a good`", menu != null)
	if menu == null:
		return
	var popup := menu.get_popup()
	var wanted := HudDealingsVocab.leg_label(key)
	for index in popup.item_count:
		if popup.get_item_text(index) == wanted:
			popup.id_pressed.emit(popup.get_item_id(index))
			return
	h._assert_hud("dealings: `Add a good` lists %s" % wanted, false)

func _check_named(root: Node, text: String) -> CheckBox:
	if root is CheckBox and (root as CheckBox).text == text:
		return root as CheckBox
	for child in root.get_children():
		var found := _check_named(child, text)
		if found != null:
			return found
	return null

# ---- the shipment picker's controls (the trade chapter's finders, narrowed) -------------------------

func _load(needle: String, amount: float) -> void:
	var presses := int(round(amount / HudComposeVocab.COMPOSE_CARGO_STEP))
	var pressed := 0
	while pressed < presses:
		var plus := _cargo_control(h._hud.allocation_panel, needle, HudWidgets.CARGO_CONTROL_PLUS)
		if plus == null or plus.disabled:
			break
		plus.emit_signal("pressed")
		pressed += 1
	h._assert_hud("dealings: the cargo row for %s took the whole %.1f" % [needle, amount], pressed == presses)

func _cargo_amount(needle: String) -> float:
	var field := _cargo_control(h._hud.allocation_panel, needle, HudWidgets.CARGO_CONTROL_FIELD) as LineEdit
	return field.text.to_float() if field != null else -1.0

func _cargo_control(root: Node, needle: String, control: String) -> Control:
	if root is HBoxContainer and (root as HBoxContainer).has_meta(HudWidgets.CARGO_ROW_KEY_META) \
			and _line_names(root, needle):
		for child in root.get_children():
			if child is Control and String((child as Control).get_meta(
					HudWidgets.CARGO_CONTROL_META, "")) == control:
				return child as Control
	for child in root.get_children():
		var found := _cargo_control(child, needle, control)
		if found != null:
			return found
	return null

func _line_names(root: Node, needle: String) -> bool:
	for child in root.get_children():
		if child is Label and (child as Label).text.findn(needle) >= 0:
			return true
		if not (child is Button) and not (child is LineEdit) and _line_names(child, needle):
			return true
	return false

func _set_party(target: int) -> void:
	var guard := 0
	while guard < 30:
		var row := _party_stepper_row(h._hud.allocation_panel)
		if row == null:
			break
		var count := int(row.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META, 0))
		if count == target:
			return
		var step := row.get_child(row.get_child_count() - 1 if count < target else 1)
		if not (step is Button) or (step as Button).disabled:
			break
		(step as Button).emit_signal("pressed")
		guard += 1

func _party_stepper_row(root: Node) -> HBoxContainer:
	if root is HBoxContainer and (root as HBoxContainer).has_meta(HudWidgets.PARTY_STEPPER_COUNT_META):
		return root as HBoxContainer
	for child in root.get_children():
		var found := _party_stepper_row(child)
		if found != null:
			return found
	return null

func _rung(dock: EventDockPanel, sim_label: String) -> String:
	for event in dock._events:
		if String(event["label"]) == sim_label:
			return String(event["rung"])
	return ""

func _label_count(dock: EventDockPanel, rendered_label: String) -> int:
	var count := 0
	for event in dock._events:
		if dock._row_label(event) == rendered_label:
			count += 1
	return count

# ---- fixtures: shaped exactly as `dict/dealings.rs` and `dict/population.rs` write them --------------

func _home_band() -> Dictionary:
	var band := BandFx.band_fixture()
	band["name"] = HOME_NAME
	band["id"] = HOME_NAME
	band["entity"] = HOME_ENTITY
	band = BandFx.with_band_id(band)
	band["pos"] = [HOME_TILE.x, HOME_TILE.y]
	band["stores"] = {"provisions": HOME_PROVISIONS}
	band["fodder_store"] = HOME_FODDER
	band["idle_workers"] = HOME_IDLE
	band["working_age"] = HOME_IDLE + 4
	band["labor_assignments"] = []
	band["carry_per_worker"] = CARRY_PER_WORKER
	band["carry_material_weight"] = MATERIAL_WEIGHT
	band["carry_fodder_weight"] = FODDER_WEIGHT
	band["band_move_tiles_per_turn"] = 2.0
	band["material_batches"] = [
		_batch("hide", 14.2, [["tough", 0.45, "fair"], ["supple", 0.58, "good"]]),
		_batch("hide", 6.0, [["tough", 0.90, "excellent"], ["supple", 0.15, "poor"]]),
		_batch("flint", 9.0, [["sharp", 0.70, "good"]]),
	]
	return band

## The home band's parties, shaped as the native decoder writes a cohort. The first three are the
## player's shipments; the envoy and the scout are the parties that stay on the Parties tab.
func _parties() -> Array:
	return [
		_party(PARTY_WAITING, "trade", "awaiting", BRACK_ENTITY, PARTY_SIZE, {
			"expedition_cargo_fodder": 3.0}),
		_party(PARTY_WAITING_REPLY, "trade", "awaiting", BRACK_ENTITY, PARTY_SIZE, {
			"current_x": HOME_TILE.x + 2, "expedition_cargo_food": REPLY_CARGO_FOOD}),
		_party(PARTY_HEADING_OUT, "trade", "outbound", MOOR_ENTITY, PARTY_SIZE, {
			"expedition_cargo_food": ROAD_CARGO_FOOD, "expedition_cargo_fodder": 2.0}),
		_party(PARTY_COMING_HOME, "trade", "returning", THORN_ENTITY, PARTY_SIZE, {
			"expedition_cargo_materials": [{"material_id": "hide", "amount": ROAD_CARGO_HIDE}]}),
		_party(PARTY_ENVOY, "envoy", "awaiting", ASH_ENTITY, PARTY_ENVOY_SIZE, {
			"expedition_destination_name": ENVOY_DESTINATION,
			"envoy_terms": {"road_rights": true, "open_borders": false, "bundles": []}}),
		_party(PARTY_SCOUT, "scout", "awaiting", 0, PARTY_SCOUT_SIZE, {}),
	]

func _party(entity: int, mission: String, phase: String, destination_entity: int, size: int,
		extra: Dictionary) -> Dictionary:
	var party := {
		"id": "Party %d" % entity, "size": size, "entity": entity, "faction": 0,
		"band_id": BandFx.FIXTURE_BAND_ID_OFFSET + entity,
		"pos": [HOME_TILE.x + 1, HOME_TILE.y], "current_x": HOME_TILE.x + 1, "current_y": HOME_TILE.y,
		"turns_of_food": 6.0, "stores": {"provisions": 5.0}, "is_expedition": true,
		"home_band_entity": HOME_ENTITY, "expedition_mission": mission, "expedition_phase": phase,
		"expedition_destination_band": BandFx.FIXTURE_BAND_ID_OFFSET + destination_entity
			if destination_entity > 0 else 0,
		"expedition_destination_name": "", "expedition_cargo_food": 0.0, "expedition_cargo_fodder": 0.0,
		"expedition_cargo_materials": [],
		"tile_info": {"x": HOME_TILE.x + 1, "y": HOME_TILE.y, "terrain_label": "Prairie Steppe",
			"tags_text": "Fertile", "visibility_state": "active", "food_module": "", "food_module_label": "None"},
	}
	party.merge(extra, true)
	return party

func _batch(material_id: String, amount: float, readings: Array) -> Dictionary:
	var reading_rows: Array = []
	for reading in readings:
		reading_rows.append({"axis": reading[0], "value": reading[1], "band_name": reading[2]})
	return {"material_id": material_id, "amount": amount, "readings": reading_rows, "variety_name": ""}

func _tie(subject_entity: int, name: String, seen: Vector2i, faction: int, strength: float = 0.7) -> Dictionary:
	return {
		"observer_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY,
		"subject_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + subject_entity,
		"strength": strength, "last_seen_x": seen.x, "last_seen_y": seen.y,
		"last_seen_turn": RESOLVED - 2, "last_contact_turn": RESOLVED - 2, "first_contact_turn": RESOLVED - 9,
		"subject_name": name, "subject_faction": faction,
	}

func _ties() -> Array:
	return [_tie(BRACK_ENTITY, BRACK_NAME, BRACK_SEEN, BRACK_FACTION),
		_tie(THORN_ENTITY, THORN_NAME, THORN_SEEN, THORN_FACTION),
		_tie(MOOR_ENTITY, MOOR_NAME, MOOR_SEEN, MOOR_FACTION)]

## Only the Brackwater road is a kept road all the way: the sim publishes one standing link, and the
## client offers Propose from that row alone.
func _treaties() -> Dictionary:
	return {"treaties": [], "standing_links": [{
		"viewer_band": BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY,
		"other_band": BandFx.FIXTURE_BAND_ID_OFFSET + BRACK_ENTITY, "other_faction": BRACK_FACTION}]}

func _goodwill() -> Dictionary:
	return {"known": [], "own": [], "cuts": {"wary_below": -0.2, "friendly_above": 0.2}}

func _goods(food: float = 0.0, fodder: float = 0.0, materials: Array = []) -> Dictionary:
	return {"food": food, "fodder": fodder, "materials": materials}

func _no_terms() -> Dictionary:
	return {"road_rights": false, "open_borders": false, "bundles": []}

func _dealing(id: int, kind: String, sender_faction: int, sender_entity: int, receiver_faction: int,
		receiver_entity: int, sent: Dictionary, asked: Dictionary, party: int, shipper_entity: int,
		treaty: Dictionary = {}, carrier: String = "") -> Dictionary:
	return {
		"id": id, "kind": kind,
		"sender_faction": sender_faction, "sender_band": BandFx.FIXTURE_BAND_ID_OFFSET + sender_entity,
		"receiver_faction": receiver_faction, "receiver_band": BandFx.FIXTURE_BAND_ID_OFFSET + receiver_entity,
		"goods_sent": sent, "goods_asked": asked, "turns_left": 1,
		"shipper_band": BandFx.FIXTURE_BAND_ID_OFFSET + shipper_entity if shipper_entity > 0 else 0,
		"party_workers": party, "treaty": treaty if not treaty.is_empty() else _no_terms(), "carrier": carrier,
	}

func _flat_hide(amount: float) -> Dictionary:
	return {"material_id": "hide", "amount": amount, "readings": [], "variety_name": ""}

## One trade record as `dict/dealings.rs` writes it: from the viewer's people's point of view, the goods
## lists unrated.
func _record(turn: int, own_entity: int, other_faction: int, other_entity: int, outcome: String,
		goods_out: Dictionary, goods_in: Dictionary) -> Dictionary:
	return {"turn": turn, "own_band": BandFx.FIXTURE_BAND_ID_OFFSET + own_entity,
		"other_faction": other_faction, "other_band": BandFx.FIXTURE_BAND_ID_OFFSET + other_entity,
		"dealing_id": 0, "outcome": outcome, "goods_out": goods_out, "goods_in": goods_in}

## The history, OLDEST FIRST as the sim publishes it: eight of this band's, and one of another band's.
func _history() -> Array:
	return [
		_record(HISTORY_TURN_TRADED, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			HudDealingsVocab.OUTCOME_EXCHANGED, _goods(5.0), _goods(0.0, 0.0, [_flat_hide(2.0)])),
		_record(HISTORY_TURN_GIFT_GIVEN, HOME_ENTITY, THORN_FACTION, THORN_ENTITY,
			HudDealingsVocab.OUTCOME_GIFT_GIVEN, _goods(0.0, 6.0), _goods()),
		_record(HISTORY_TURN_DECLINED, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			HudDealingsVocab.OUTCOME_DECLINED, _goods(0.0, 0.0, [{"material_id": "flint", "amount": 3.0,
				"readings": [], "variety_name": ""}]), _goods(4.0)),
		_record(HISTORY_TURN_REFUSED, HOME_ENTITY, MOOR_FACTION, MOOR_ENTITY,
			HudDealingsVocab.OUTCOME_REFUSED, _goods(8.0), _goods()),
		_record(HISTORY_TURN_OTHER_BAND, OTHER_BAND_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			HudDealingsVocab.OUTCOME_EXCHANGED, _goods(7.0), _goods(2.0)),
		_record(HISTORY_TURN_EXPIRED, HOME_ENTITY, THORN_FACTION, THORN_ENTITY,
			HudDealingsVocab.OUTCOME_EXPIRED, _goods(2.0), _goods()),
		_record(HISTORY_TURN_GIFT_RECEIVED, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			HudDealingsVocab.OUTCOME_GIFT_RECEIVED, _goods(), _goods(0.0, 0.0, [_flat_hide(2.0)])),
		_record(HISTORY_TURN_WITHDRAWN, HOME_ENTITY, THORN_FACTION, THORN_ENTITY,
			HudDealingsVocab.OUTCOME_WITHDRAWN, _goods(), _goods(PROPOSAL_FOOD)),
		_record(HISTORY_TURN_FAILED, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			HudDealingsVocab.OUTCOME_FAILED, _goods(4.0), _goods()),
	]

## Each dealing a camped party stands for names that party by its band id (`party_band`, the key the
## client joins on); every other dealing carries 0.
func _dealings() -> Array:
	var rows := _dealings_unjoined()
	for row in rows:
		match int(row["id"]):
			OFFER_OUT:
				row["terms"] = HudDealingsVocab.TERMS_OFFER
				row["party_band"] = BandFx.FIXTURE_BAND_ID_OFFSET + PARTY_WAITING
			REPLY_IN:
				row["party_band"] = BandFx.FIXTURE_BAND_ID_OFFSET + PARTY_WAITING_REPLY
			OFFER_IN:
				row["terms"] = HudDealingsVocab.TERMS_OFFER
				row["party_band"] = 0
			_:
				row["party_band"] = 0
	return rows

func _dealings_unjoined() -> Array:
	var hide := _batch("hide", OFFER_HIDE, [["tough", 0.45, "fair"], ["supple", 0.58, "good"]])
	var flint := {"material_id": "flint", "amount": PROPOSAL_FLINT, "readings": [], "variety_name": ""}
	return [
		_dealing(OFFER_IN, HudDealingsVocab.KIND_OFFER, BRACK_FACTION, BRACK_ENTITY, 0, HOME_ENTITY,
			_goods(OFFER_FOOD, 0.0, [hide]), _goods(), OFFER_PARTY, 0),
		_dealing(PROPOSAL_IN, HudDealingsVocab.KIND_PROPOSAL, THORN_FACTION, THORN_ENTITY, 0, HOME_ENTITY,
			_goods(0.0, 0.0, [flint]), _goods(PROPOSAL_FOOD), PROPOSAL_PARTY, THORN_ENTITY),
		_dealing(OFFER_OUT, HudDealingsVocab.KIND_OFFER, 0, HOME_ENTITY, BRACK_FACTION, BRACK_ENTITY,
			_goods(0.0, 6.0), _goods(), 2, 0),
		_dealing(TREATY_IN, HudDealingsVocab.KIND_TREATY_PROPOSAL, THORN_FACTION, THORN_ENTITY, 0, HOME_ENTITY,
			_goods(), _goods(), 0, 0, {"road_rights": true, "open_borders": true, "bundles": [{
				"sent": _goods(0.0, 0.0, [flint]), "returned": _goods(PROPOSAL_FOOD)}]},
			HudDealingsVocab.CARRIER_ENVOY),
		# We proposed (we are the shipper); they came back with a counter: eight food for two hides.
		_dealing(COUNTER_IN, HudDealingsVocab.KIND_COUNTER, BRACK_FACTION, BRACK_ENTITY, 0, HOME_ENTITY,
			_goods(8.0), _goods(0.0, 0.0, [{"material_id": "hide", "amount": 2.0, "readings": [],
				"variety_name": ""}]), 2, HOME_ENTITY),
		# A REPLY to our offer: Brackwater named a return of flint for our food (we answer it).
		_dealing(REPLY_IN, HudDealingsVocab.KIND_REPLY, BRACK_FACTION, BRACK_ENTITY, 0, HOME_ENTITY,
			_goods(REPLY_CARGO_FOOD), _goods(0.0, 0.0, [{"material_id": "flint", "amount": REPLY_RETURN_FLINT,
				"readings": [], "variety_name": ""}]), 2, HOME_ENTITY),
		# …and a reply WE named to Thornwick's offer: their party waits on our return of food.
		_dealing(REPLY_OUT, HudDealingsVocab.KIND_REPLY, 0, HOME_ENTITY, THORN_FACTION, THORN_ENTITY,
			_goods(0.0, 0.0, [_flat_hide(OFFER_HIDE)]), _goods(PROPOSAL_FOOD), 2, THORN_ENTITY),
	]
