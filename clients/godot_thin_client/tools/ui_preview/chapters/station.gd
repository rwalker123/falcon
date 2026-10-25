extends RefCounted

## THE STATIONED PARTY — the Peoples tab's `Station people` link, its compose sheet, the Peoples row
## of a band that has people posted, the Parties row of a station party, and the event-dock lines
## (issue #774).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. See `.claude/rules/client/test-harnesses.md`. It is appended LAST and
## hands the reference band back, so no earlier frame moves.
##
## **THE SHEET IS DRIVEN THROUGH THE ROSTER'S OWN LINK**, never by opening the verb directly: the link
## is the only way in, so a link that did not open it would be a defect no direct call could show.
## The command it ends in is asserted by EQUALITY through the real `Main.format_station_expedition`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 50

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
const EVENT_DOCK_SCENE := preload("res://src/ui/EventDockPanel.tscn")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The SENDING band and the bands its ties name. Their own entities, so these frames cannot be
## confused with the reference band the rest of the run uses.
const HOME_ENTITY := 1181
const HOME_NAME := "Ashfell"
const HOME_TILE := Vector2i(71, 18)
## A band the player also holds, with a party already walking to it.
const WALKING_ENTITY := 1182
const WALKING_NAME := "Elmgate"
## Bands the player does NOT hold: remembered by the name their tie carries.
const STATIONED_ENTITY := 1183
const STATIONED_NAME := "Rushford"
const OPEN_ENTITY := 1184
const OPEN_NAME := "Brackwater"
const PARKED_ENTITY := 1185
const PARKED_NAME := "Thornwick"

## Where each was last seen. Rushford is far enough off that the walk home is a real number of turns.
const STATIONED_SEEN := Vector2i(60, 24)
const OPEN_SEEN := Vector2i(63, 21)
const WALKING_SEEN := Vector2i(66, 20)
const PARKED_SEEN := Vector2i(44, 9)
## A band the HOME band holds NO row to: only its sibling (the walking band) has met it. The people
## knows it, so its row lists under `Known through <sibling>` with both links enabled.
const SIBLING_SUBJECT_ENTITY := 1186
const SIBLING_SUBJECT_NAME := "Sedgemoor"
const SIBLING_SEEN := Vector2i(52, 14)
const SIBLING_SEEN_AGE := 4

## The turn the frames are rendered on, and how stale the stationed row's news is.
const TURN := 40
## Contact stamps run one behind the frame turn (`HudConnectionsVocab.RESOLVED_TURN_LAG`).
const RESOLVED := TURN - HudConnectionsVocab.RESOLVED_TURN_LAG
const NEWS_AGE_TURNS := 3

## Tiles per turn the band publishes — without it the sheet cannot price a walk and says `a walk`.
const MOVE_TILES_PER_TURN := 2.0
## How far inside the shipped reach the near-host frame stands its sender from the host.
const NEAR_HOST_OFFSET_TILES := 1
## The shipped messenger reach: a host this close reports the same turn.
const COMM_RANGE_TILES := 2

## The four party missions the Parties row marks, and the first entity of the fixture parties.
const MARK_MISSIONS := ["scout", "deny", "trade", "station"]
const MARK_PARTY_ENTITY_BASE := 7390

const TIE_LIVE := 0.75
const TIE_PARKED := 0.0

## The bands' free workers, and the people in each party.
const HOME_IDLE := 6
const STATIONED_PARTY_SIZE := 2
const WALKING_PARTY_SIZE := 1
const STATIONED_PARTY_ENTITY := 7281
const WALKING_PARTY_ENTITY := 7282

## The people the sheet's stepper is driven to, and the launch it must produce.
const SHEET_PEOPLE := 3

const PANEL_RESERVER := &"band_panel"

var _sent: Array = []


func run(harness) -> void:
	h = harness
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	panel.set_dock(SIDE_RIGHT)
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.set_band_city_panel(panel)
	h._hud.station_expedition_requested.connect(func(payload: Dictionary) -> void: _sent.append(payload))

	var prior_turn: int = h._hud._band_labor.current_turn()
	h._hud.update_overlay(TURN, {})
	h._hud.update_connections(_ties())
	h._hud.update_band_alerts([_home_band(), _walking_band(), _party_stationed(), _party_walking()])
	h._hud.show_unit_selection(_home_band())
	await h._settle()

	# ---- (b) THE PEOPLES TAB: stationed, walking, offered, parked ---------------------------------
	await h._save("station_peoples")
	var block := panel.find_child(HudConnectionsVocab.BLOCK_NAME, true, false) as Control
	h._assert_hud("station: the band page carries a Peoples roster", block != null)
	var rows := _roster_rows(block)
	h._assert_hud("station: the roster holds the four ties plus the band known through a sibling (%d)" % rows.size(),
		rows.size() == 5)
	var stationed_row := _row_for(rows, STATIONED_NAME)
	var walking_row := _row_for(rows, WALKING_NAME)
	var open_row := _row_for(rows, OPEN_NAME)
	var parked_row := _row_for(rows, PARKED_NAME)
	h._assert_hud("station: each tie is named, none by a raw id",
		stationed_row != null and walking_row != null and open_row != null and parked_row != null)
	h._assert_hud("station: a row with people posted reads `stationed` in place of growing",
		stationed_row != null and _text_of(stationed_row).contains(
			HudConnectionsVocab.STATE_WORDS[HudConnectionsVocab.STATE_STATIONED])
		and not _text_of(stationed_row).contains(
			HudConnectionsVocab.STATE_WORDS[HudConnectionsVocab.STATE_GROWING]))
	var hover := stationed_row.tooltip_text if stationed_row != null else ""
	h._assert_hud("station: its hover adds how many are posted and how old their news is (\"%s\")" % hover,
		hover.contains(HudConnectionsVocab.STATION_ROW_HOVER_FORMAT % [
			STATIONED_PARTY_SIZE, NEWS_AGE_TURNS, HudConnectionsVocab.STATION_TURN_MANY]))
	h._assert_hud("station: …and stays two lines on its face (the link rides line 2)",
		stationed_row != null and stationed_row.get_child_count() == 2)
	var stationed_link := _link_of(stationed_row)
	h._assert_hud("station: a posted host's link reads `Stationed` and cannot be pressed",
		stationed_link != null and stationed_link.text == HudConnectionsVocab.STATION_LINK_DONE
			and stationed_link.disabled)
	var walking_link := _link_of(walking_row)
	h._assert_hud("station: a host people are still WALKING to reads `Stationed` too",
		walking_link != null and walking_link.text == HudConnectionsVocab.STATION_LINK_DONE
			and walking_link.disabled)
	h._assert_hud("station: …but its state word is still the tie's own (the party has not arrived)",
		walking_row != null and not _text_of(walking_row).contains(
			HudConnectionsVocab.STATE_WORDS[HudConnectionsVocab.STATE_STATIONED]))
	var open_link := _link_of(open_row)
	h._assert_hud("station: a live tie with nobody posted offers `Station people`",
		open_link != null and open_link.text == HudConnectionsVocab.STATION_LINK
			and not open_link.disabled)
	var parked_link := _link_of(parked_row)
	h._assert_hud("station: a PARKED tie's `Station people` link is ENABLED — the tie does not gate it",
		parked_link != null and not parked_link.disabled
			and parked_link.tooltip_text == HudConnectionsVocab.STATION_LINK_TOOLTIP)
	for row in rows:
		h._assert_hud("station: no row prints a raw band id (%s)" % _text_of(row).left(40),
			not _text_of(row).contains(str(BandFx.FIXTURE_BAND_ID_OFFSET)))

	# ---- (b2) THE `Trade` LINK, beside `Station people` on the same rows ----------------------------
	for row in rows:
		var trade_link := _trade_link_of(row)
		h._assert_hud("station: every row carries a `Trade` link beside its `Station people` one (%s)"
				% _text_of(row).left(24),
			trade_link != null and _link_of(row) != null)
	var open_trade := _trade_link_of(open_row)
	var trade_verb := HudComposeVocab.verb_for_mission(HudComposeVocab.COMPOSE_MISSION_TRADE)
	h._assert_hud("station: a live tie's link reads `Trade`, is pressable, and wears the Trade verb's own mark",
		open_trade != null and not open_trade.disabled
			and open_trade.text == HudConnectionsVocab.TRADE_LINK
			and open_trade.icon == HudSprites.for_mark(String(trade_verb[HudComposeVocab.VERB_KEY_MARK]))
			and open_trade.icon != null)
	var parked_trade := _trade_link_of(parked_row)
	h._assert_hud("station: a PARKED tie's `Trade` link is ENABLED too",
		parked_trade != null and not parked_trade.disabled
			and parked_trade.tooltip_text == HudConnectionsVocab.TRADE_LINK_TOOLTIP)
	# **A BAND KNOWN ONLY THROUGH A SIBLING HAS A ROW**, naming the sibling and the sighting, with both links
	# enabled; its Trade link opens the sheet with To already set (clicked like a player would).
	var sibling_row := _row_for(rows, SIBLING_SUBJECT_NAME)
	h._assert_hud("station: a band only a sibling has met appears in the roster",
		sibling_row != null)
	var through_line: String = HudConnectionsVocab.KNOWN_THROUGH_FORMAT % [WALKING_NAME,
		SIBLING_SEEN.x, SIBLING_SEEN.y, RESOLVED - SIBLING_SEEN_AGE]
	h._assert_hud("station: …reading `%s`" % through_line,
		sibling_row != null and _text_of(sibling_row).contains(through_line))
	var sibling_trade := _trade_link_of(sibling_row)
	var sibling_station := _link_of(sibling_row)
	h._assert_hud("station: …with BOTH links enabled",
		sibling_trade != null and not sibling_trade.disabled
			and sibling_station != null and not sibling_station.disabled)
	h._assert_hud("station: …and it stays two lines",
		sibling_row != null and sibling_row.get_child_count() == 2)
	await h._save("station_peoples_known_through")
	if sibling_trade != null:
		await _real_click(sibling_trade)
	var sibling_form := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) as Control
	var sibling_to := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.READ_ONLY_FIELD_META)
	h._assert_hud("station: the sibling-known row's Trade link opens the sheet with To = %s" % SIBLING_SUBJECT_NAME,
		sibling_form != null and String(sibling_form.get_meta(HudWidgets.VERB_FORM_META, ""))
			== HudComposeVocab.COMPOSE_MISSION_TRADE
			and sibling_to != null and _text_of(sibling_to).contains(SIBLING_SUBJECT_NAME))
	h._hud.close_verb_form()
	await h._settle()
	# **THE STATION SEND IS ENABLED FOR A PARKED-ONLY HOST.**
	var live_parked_link := _find_link(panel, BandFx.FIXTURE_BAND_ID_OFFSET + PARKED_ENTITY)
	if live_parked_link != null:
		live_parked_link.emit_signal("pressed")
		await h._settle()
	var parked_send := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_STATION_CONFIRM_META) as Button
	h._assert_hud("station: the Station send for a parked-only host is enabled",
		parked_send != null and not parked_send.disabled)
	h._hud.close_verb_form()
	await h._settle()
	# A REAL pointer click on the row's link, not a synthesised `pressed`: the Trade sheet opens on the
	# sender's drawer with the row's band already its `To`, and a `✕` to clear it.
	# The rows were rebuilt by the sheets opened above: find the live tie's link again.
	var fresh_block := panel.find_child(HudConnectionsVocab.BLOCK_NAME, true, false) as Control
	open_trade = _trade_link_of(_row_for(_roster_rows(fresh_block), OPEN_NAME))
	if open_trade != null:
		await _real_click(open_trade)
	var trade_form := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) as Control
	h._assert_hud("station: a click on the row's Trade link opens the Trade sheet in the sender's drawer",
		trade_form != null and String(trade_form.get_meta(HudWidgets.VERB_FORM_META, ""))
			== HudComposeVocab.COMPOSE_MISSION_TRADE)
	var to_row := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.READ_ONLY_FIELD_META)
	h._assert_hud("station: …with `To` already set to that row's band (%s)" % OPEN_NAME,
		to_row != null and _text_of(to_row).contains(OPEN_NAME))
	h._assert_hud("station: …carrying the ✕ that clears it",
		Q.find_meta_node(h._hud.allocation_panel, HudWidgets.FIELD_CLEAR_META) != null)
	await h._save("station_trade_from_peoples")
	h._hud.close_verb_form()
	await h._settle()

	# ---- (c) THE PARTIES ZONE: an outbound and a stationed party -----------------------------------
	panel.set_active_tab(BandCityPanel.ZONE_PARTIES)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	await h._save("station_parties")
	var parties_text := _text_of(panel)
	h._assert_hud("station: a stationed party reads `Stationed with Rushford`",
		parties_text.contains(HudExpeditionVocab.STATION_ROW_STATIONED_FORMAT % STATIONED_NAME))
	h._assert_hud("station: an outbound one reads `Going to stay with Elmgate`",
		parties_text.contains(HudExpeditionVocab.STATION_ROW_OUTBOUND_FORMAT % WALKING_NAME))
	h._assert_hud("station: neither row prints a raw band id",
		not parties_text.contains(str(BandFx.FIXTURE_BAND_ID_OFFSET + STATIONED_ENTITY))
			and not parties_text.contains(str(BandFx.FIXTURE_BAND_ID_OFFSET + WALKING_ENTITY)))
	h._assert_hud("station: …and no food runway or cargo readout on either",
		not parties_text.contains("999") and not parties_text.to_lower().contains("carried"))
	# ---- (c2) ALL FOUR MISSION MARKS ARE ART, on the Parties row ------------------------------------
	h._hud.update_band_alerts([_home_band(), _walking_band()] + _four_mission_parties())
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	await h._save("station_party_marks")
	var row_icons: Array = []
	_collect_icons(panel, row_icons)
	for mission in MARK_MISSIONS:
		var wanted: Texture2D = HudSprites.for_mark(HudFormat.expedition_mission_mark(mission))
		h._assert_hud("station: the `%s` party's Parties row draws its hud/ art, not the glyph" % mission,
			wanted != null and row_icons.has(wanted))
	h._assert_hud("station: the station mark is not the scout's (an absent mission is the scout's)",
		HudSprites.for_mark(HudFormat.expedition_mission_mark("station"))
			!= HudSprites.for_mark(HudFormat.expedition_mission_mark("scout")))
	h._assert_hud("station: the map marker for a stationed party resolves art too",
		ExpeditionSprites.for_mission(HudExpeditionVocab.EXPEDITION_MISSION_STATION) != null)
	h._hud.update_band_alerts([_home_band(), _walking_band(), _party_stationed(), _party_walking()])
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	# The party's own detail, selected: a Host row, no Provisions row.
	h._hud.show_unit_selection(_party_stationed())
	await h._settle()
	var roster_icons: Array = []
	_collect_icons(h._hud, roster_icons)
	h._assert_hud("station: the selection card's party row draws the station art, not the glyph",
		roster_icons.has(HudSprites.for_mark("station")))
	await h._save("station_party_detail")
	var party_text: String = h._hud.occupant_detail.text
	h._assert_hud("station: the party's detail names its host and mission",
		Readout.detail_excerpt(party_text, HudExpeditionVocab.STATION_HOST_ROW).contains(
			STATIONED_NAME)
			and party_text.contains(HudExpeditionVocab.EXPEDITION_MISSION_LABELS[
				HudExpeditionVocab.EXPEDITION_MISSION_STATION]))
	h._assert_hud("station: …and states no food (the home band feeds it)",
		not party_text.contains("Provisions") and not party_text.contains("Carried"))

	# ---- (a) THE SHEET, opened by the roster's own link --------------------------------------------
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_home_band())
	await h._settle()
	var link := _find_link(panel, BandFx.FIXTURE_BAND_ID_OFFSET + OPEN_ENTITY)
	h._assert_hud("station: the live host's link is in the Peoples tab", link != null)
	if link != null:
		link.emit_signal("pressed")
	await h._settle()
	await h._save("station_sheet")
	var form := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) as Control
	h._assert_hud("station: pressing the link opens the station sheet in the sender's drawer",
		form != null and String(form.get_meta(HudWidgets.VERB_FORM_META, ""))
			== HudComposeVocab.COMPOSE_MISSION_STATION)
	var sheet_text := _text_of(form)
	h._assert_hud("station: …titled with the host (\"%s\")" % (HudComposeVocab.STATION_TITLE_FORMAT % OPEN_NAME),
		sheet_text.contains(HudComposeVocab.STATION_TITLE_FORMAT % OPEN_NAME))
	h._assert_hud("station: …naming where they were last seen, as the shipment sheet's To row does",
		sheet_text.contains(HudComposeVocab.STATION_FIELD_WITH) and sheet_text.contains(OPEN_NAME))
	h._assert_hud("station: …pricing the news lag as a walk in turns",
		sheet_text.contains("turns late") and not sheet_text.contains(HudComposeVocab.STATION_NEWS_SAME_TURN))
	h._assert_hud("station: …and the far host's lag is the sim's rule (%d turns)" % _far_delay(),
		sheet_text.contains(HudComposeVocab.STATION_NEWS_LATE_FORMAT % _far_delay()))
	h._assert_hud("station: …with no map pick armed", not h._hud.is_targeting_active())
	# Raise the party through its own stepper, the shipment sheet's way.
	_set_people(SHEET_PEOPLE)
	await h._settle()
	await h._save("station_sheet_people")
	var send := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.SEND_STATION_CONFIRM_META) as Button
	h._assert_hud("station: the sheet carries a live send", send != null and not send.disabled)
	h._assert_hud("station: the stepper's count is what the sheet will send",
		_stepper_count() == SHEET_PEOPLE)
	_sent.clear()
	if send != null:
		send.emit_signal("pressed")
	await h._settle()
	h._assert_hud("station: the send emits exactly one order", _sent.size() == 1)
	if _sent.size() == 1:
		var line := String(MAIN_SCRIPT.format_station_expedition(_sent[0]).get("line", ""))
		var expected := "station_expedition 0 %d %d %d" % [BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY,
			SHEET_PEOPLE, BandFx.FIXTURE_BAND_ID_OFFSET + OPEN_ENTITY]
		h._assert_hud("station: …and Main spells it `%s` (got `%s`)" % [expected, line], line == expected)
	h._assert_hud("station: the order closes the sheet",
		Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) == null)
	h._assert_hud("station: a payload with no host builds no command",
		MAIN_SCRIPT.format_station_expedition({"band_id": 1, "party_workers": 2}).is_empty())

	# ---- (c) THE NEAR HOST: inside messenger reach the news is the same turn ---------------------
	_pin_news_delay_rule()
	var near_band := _home_band()
	near_band["pos"] = [OPEN_SEEN.x - NEAR_HOST_OFFSET_TILES, OPEN_SEEN.y]
	near_band["current_x"] = OPEN_SEEN.x - NEAR_HOST_OFFSET_TILES
	near_band["current_y"] = OPEN_SEEN.y
	h._hud.update_band_alerts([near_band, _walking_band(), _party_stationed(), _party_walking()])
	h._hud.show_unit_selection(near_band)
	await h._settle()
	var near_link := _find_link(panel, BandFx.FIXTURE_BAND_ID_OFFSET + OPEN_ENTITY)
	h._assert_hud("station: the near host's link is in the Peoples tab", near_link != null)
	if near_link != null:
		near_link.emit_signal("pressed")
	await h._settle()
	await h._save("station_sheet_near")
	var near_form := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.VERB_FORM_META) as Control
	var near_text := _text_of(near_form) if near_form != null else ""
	h._assert_hud("station: a host inside messenger reach says the news arrives the same turn",
		near_text.contains(HudComposeVocab.STATION_NEWS_SAME_TURN) and not near_text.contains("turns late"))

	# ---- (d) THE EVENT DOCK ---------------------------------------------------------------------
	await _event_dock_states()

	# Hand the reference band back, so a chapter appended after this one starts where the rest do.
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(PANEL_RESERVER, SIDE_RIGHT, 0.0)
	panel.queue_free()
	await h.get_tree().process_frame
	h._hud.update_overlay(prior_turn, {})
	h._hud.update_connections([])
	h._hud.update_band_alerts([BandFx.band_fixture()])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()


## The sim's own lines for a station party's life — its label in the sim's `band <id>` spelling, and
## the `host=<id>` token the dock swaps the roster's name in by.
func _event_dock_states() -> void:
	var host_id := BandFx.FIXTURE_BAND_ID_OFFSET + STATIONED_ENTITY
	var dock: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	h.add_child(dock)
	await h.get_tree().process_frame
	dock.set_perpendicular_insets(h._hud.left_column_width(), h._hud.right_column_width())
	dock.reset()
	dock.set_band_labels({str(host_id): STATIONED_NAME})
	var sim_host := "band %d" % host_id
	var launch_label := "%s party -> stationed with %s" % [HOME_NAME, sim_host]
	var arrived_label := "Party stationed with %s" % sim_host
	var borders_label := "%s closed its borders — party returning home" % sim_host
	var gone_label := "Party lost %s — returning home" % sim_host
	var recalled_label := "%s recalled — returning home" % HOME_NAME
	var home_gone_label := "Party's home band is gone — returning"
	var not_found_label := "Party found no sign of %s where they were last seen — coming home" % sim_host
	var dest_not_found_label := "Shipment found no sign of %s where they were last seen — coming home" % sim_host
	dock.ingest_events([
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": not_found_label,
			"detail": "status=returning reason=host_not_found host=%d expedition=7" % host_id,
			"seq": 707},
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": dest_not_found_label,
			"detail": "status=returning reason=destination_not_found destination=%d expedition=8" % host_id,
			"seq": 708},
		{"tick": TURN, "kind": "expedition_sent", "faction": 0, "label": launch_label,
			"detail": "status=applied mission=station workers=2 host=%d distance=9 expedition=7" % host_id,
			"seq": 701},
		{"tick": TURN, "kind": "expedition_arrived", "faction": 0, "label": arrived_label,
			"detail": "status=stationed host=%d expedition=7" % host_id, "seq": 702},
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": borders_label,
			"detail": "status=returning reason=borders_closed host=%d expedition=7" % host_id,
			"seq": 703},
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": gone_label,
			"detail": "status=returning reason=host_gone host=%d expedition=7" % host_id, "seq": 705},
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": home_gone_label,
			"detail": "status=returning reason=home_gone host=%d expedition=7" % host_id, "seq": 709},
		{"tick": TURN, "kind": "expedition_recalled", "faction": 0, "label": recalled_label,
			"detail": "status=returning reason=recalled host=%d expedition=7" % host_id, "seq": 706},
	])
	dock.set_expanded(true)
	await h._settle()
	await h._save("station_event_dock")
	for label in [launch_label, arrived_label, borders_label, gone_label, home_gone_label, not_found_label, dest_not_found_label]:
		var swapped := String(label).replace(sim_host, STATIONED_NAME)
		h._assert_hud("station dock: `%s` names the host by the roster's name" % swapped,
			_label_count(dock, swapped) == 1)
	for event in dock._events:
		var phrase := EventDockPanel.detail_phrase(String(event["detail"]))
		h._assert_hud("station dock: no raw host id in the detail (\"%s\")" % phrase,
			not phrase.contains(str(host_id)))
	h._assert_hud("station dock: a post ended by the world is NOTABLE (visible at the default floor)",
		_rung(dock, borders_label) == HudEventVocab.RUNG_NOTABLE
			and _rung(dock, gone_label) == HudEventVocab.RUNG_NOTABLE
			and _rung(dock, home_gone_label)
			and _rung(dock, not_found_label) == HudEventVocab.RUNG_NOTABLE
			and _rung(dock, dest_not_found_label) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("station dock: …the player's own recall stays a ROUTINE receipt",
		_rung(dock, recalled_label) == HudEventVocab.RUNG_ROUTINE)
	dock.set_band_labels({})
	dock.queue_free()
	await h.get_tree().process_frame
	await h._settle()
	await _remembered_hover_state()


## The map hover for a remembered band (a tied people the viewer cannot see): the hex tooltip gains a
## line naming the band and when, in the Peoples tab's words. Fed the tile-info shape `MapView` builds.
func _remembered_hover_state() -> void:
	var tie := {"subject_band_id": 77, "subject_name": "Goosefen", "strength": 0.2,
		"last_seen_x": 71, "last_seen_y": 8, "last_seen_turn": 21}
	h._hud.show_tooltip({"x": 71, "y": 8, "terrain_label": "Grassland", "visibility_state": "discovered",
		"remembered_bands": [tie]})
	await h._settle()
	await h._save("remembered_band_hover")
	var text: String = h._hud.tooltip_label.text
	h._assert_hud("remembered hover: the tooltip names the band and when",
		text.contains("Goosefen \u2014 last seen turn 21, may have moved"))
	h._hud.show_tooltip({})


# ---- finders ----------------------------------------------------------------------------------

func _label_count(dock: EventDockPanel, rendered_label: String) -> int:
	var count := 0
	for event in dock._events:
		if dock._row_label(event) == rendered_label:
			count += 1
	return count

func _rung(dock: EventDockPanel, sim_label: String) -> String:
	for event in dock._events:
		if String(event["label"]) == sim_label:
			return String(event["rung"])
	return ""

func _roster_rows(block: Node) -> Array:
	var rows: Array = []
	var box := ConnectionsRoster.rows_box(block)
	if box == null:
		return rows
	for child in box.get_children():
		if child is Control and (child as Control).has_meta(HudConnectionsVocab.ROW_META):
			rows.append(child)
	return rows

func _row_for(rows: Array, name: String) -> Control:
	for row in rows:
		if _text_of(row).contains(name):
			return row
	return null

## A real pointer click on a control: hover, press, release through the engine's own dispatch.
func _real_click(control: Control) -> void:
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(), control.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h.get_tree().process_frame
	await h._settle()

## The `Trade` link under `row`.
func _trade_link_of(row: Node) -> Button:
	if row is Button and (row as Button).has_meta(HudConnectionsVocab.TRADE_LINK_META):
		return row as Button
	for child in row.get_children() if row != null else []:
		var found := _trade_link_of(child)
		if found != null:
			return found
	return null

func _link_of(row: Control) -> Button:
	if row == null:
		return null
	return _find_link(row, -1)

## The `Station people` link under `root`: the first one when `host_id` is -1, else the one whose
## meta names that host.
func _find_link(root: Node, host_id: int) -> Button:
	if root is Button and (root as Button).has_meta(HudConnectionsVocab.STATION_LINK_META):
		if host_id < 0 or int((root as Button).get_meta(HudConnectionsVocab.STATION_LINK_META)) == host_id:
			return root as Button
	for child in root.get_children():
		var found := _find_link(child, host_id)
		if found != null:
			return found
	return null

## Every texture a `Button.icon` or `TextureRect` under `root` carries.
func _collect_icons(root: Node, out: Array) -> void:
	if root is Button and (root as Button).icon != null:
		out.append((root as Button).icon)
	elif root is TextureRect and (root as TextureRect).texture != null:
		out.append((root as TextureRect).texture)
	for child in root.get_children():
		_collect_icons(child, out)

## One party per mission on the home band, so the Parties zone draws all four marks at once.
func _four_mission_parties() -> Array:
	var out: Array = []
	var entity := MARK_PARTY_ENTITY_BASE
	for mission in ["scout", "deny", "trade", "station"]:
		var party := _party(entity, 2, "outbound", STATIONED_ENTITY, HOME_TILE)
		party["expedition_mission"] = mission
		out.append(party)
		entity += 1
	return out

func _text_of(root: Node) -> String:
	var text := ""
	if root is Label:
		text += (root as Label).text + " "
	elif root is Button:
		text += (root as Button).text + " "
	elif root is RichTextLabel:
		text += (root as RichTextLabel).text + " "
	for child in root.get_children():
		text += _text_of(child)
	return text

func _stepper_count() -> int:
	var row := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.PARTY_STEPPER_COUNT_META)
	return int(row.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META)) if row != null else -1

## Press the stepper's `+` until it reads `target` — the player's own control, not a state poke.
func _set_people(target: int) -> void:
	var guard := 0
	while _stepper_count() < target and guard < target * 2:
		var row := Q.find_meta_node(h._hud.allocation_panel, HudWidgets.PARTY_STEPPER_COUNT_META)
		var plus: Button = null
		for child in row.get_children():
			if child is Button and (child as Button).text == "+":
				plus = child
		if plus == null or plus.disabled:
			return
		plus.emit_signal("pressed")
		guard += 1
		await h.get_tree().process_frame


## The sim's delay for the far host, worked from the same pieces the sheet reads.
func _far_delay() -> int:
	return SourceForecast.station_news_delay_turns(_home_band(), OPEN_SEEN.x, OPEN_SEEN.y,
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal())

## Pins `SourceForecast.station_news_delay_turns` on a flat row, where hex distance is the column gap:
## AT the reach the news is the same turn, one tile past it is a walk of ceil(distance / rate), no rate
## reads 0, and a row without the reach field reads 0 reach.
func _pin_news_delay_rule() -> void:
	var grid := 100
	var band := {"pos": [10, 10], "band_move_tiles_per_turn": MOVE_TILES_PER_TURN,
		SourceForecast.COMM_RANGE_KEY: COMM_RANGE_TILES}
	var at_reach := SourceForecast.station_news_delay_turns(band, 10 + COMM_RANGE_TILES, 10, grid, false)
	var past_reach := SourceForecast.station_news_delay_turns(band, 10 + COMM_RANGE_TILES + 1, 10, grid, false)
	h._assert_hud("station: news delay at distance = reach is 0 (got %d)" % at_reach, at_reach == 0)
	h._assert_hud("station: news delay at reach+1 is ceil(3/2) = 2 turns (got %d)" % past_reach,
		past_reach == 2)
	band.erase("band_move_tiles_per_turn")
	h._assert_hud("station: no published move rate reads 0 turns",
		SourceForecast.station_news_delay_turns(band, 40, 10, grid, false) == 0)
	band["band_move_tiles_per_turn"] = MOVE_TILES_PER_TURN
	band.erase(SourceForecast.COMM_RANGE_KEY)
	h._assert_hud("station: a row without the reach field reads 0 reach (distance 1 is late)",
		SourceForecast.station_news_delay_turns(band, 11, 10, grid, false) == 1)

# ---- fixtures ---------------------------------------------------------------------------------

func _home_band() -> Dictionary:
	var band := BandFx.band_fixture()
	band["name"] = HOME_NAME
	band["id"] = HOME_NAME
	band["entity"] = HOME_ENTITY
	band = BandFx.with_band_id(band)
	band["pos"] = [HOME_TILE.x, HOME_TILE.y]
	band["idle_workers"] = HOME_IDLE
	band["working_age"] = HOME_IDLE + STATIONED_PARTY_SIZE + WALKING_PARTY_SIZE
	band["labor_assignments"] = []
	band["band_move_tiles_per_turn"] = MOVE_TILES_PER_TURN
	band[SourceForecast.COMM_RANGE_KEY] = COMM_RANGE_TILES
	return band

func _walking_band() -> Dictionary:
	var band := BandFx.band_fixture()
	band["name"] = WALKING_NAME
	band["id"] = WALKING_NAME
	band["entity"] = WALKING_ENTITY
	band = BandFx.with_band_id(band)
	band["pos"] = [WALKING_SEEN.x, WALKING_SEEN.y]
	band["labor_assignments"] = []
	return band

func _party(entity: int, size: int, phase: String, host_entity: int, pos: Vector2i) -> Dictionary:
	return {
		"id": "Posted %d" % entity,
		"name": HOME_NAME,
		"size": size,
		"entity": entity,
		"band_id": BandFx.FIXTURE_BAND_ID_OFFSET + entity,
		"faction": 0,
		"pos": [pos.x, pos.y],
		"current_x": pos.x,
		"current_y": pos.y,
		"home_band_entity": HOME_ENTITY,
		# **THE WIRE'S SHAPE FOR A STATION PARTY**: no larder of its own (the home band feeds it), so
		# 999 turns of food and an empty store, no carry cap, and the host named by key and by name.
		"turns_of_food": 999.0,
		"stores": {},
		"is_expedition": true,
		"expedition_mission": "station",
		"expedition_phase": phase,
		"expedition_destination_band": BandFx.FIXTURE_BAND_ID_OFFSET + host_entity,
		"expedition_destination_name": "",
		"expedition_carry_cap": 0.0,
		"tile_info": {
			"x": pos.x, "y": pos.y,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

func _party_stationed() -> Dictionary:
	return _party(STATIONED_PARTY_ENTITY, STATIONED_PARTY_SIZE, "stationed", STATIONED_ENTITY,
		STATIONED_SEEN)

func _party_walking() -> Dictionary:
	return _party(WALKING_PARTY_ENTITY, WALKING_PARTY_SIZE, "outbound", WALKING_ENTITY, HOME_TILE)

func _ties() -> Array:
	var stationed := _tie(STATIONED_ENTITY, TIE_LIVE, STATIONED_SEEN, RESOLVED - NEWS_AGE_TURNS, RESOLVED)
	stationed["subject_name"] = STATIONED_NAME
	var open := _tie(OPEN_ENTITY, TIE_LIVE, OPEN_SEEN, RESOLVED - 2, RESOLVED - 2)
	open["subject_name"] = OPEN_NAME
	var parked := _tie(PARKED_ENTITY, TIE_PARKED, PARKED_SEEN, 12, 12)
	parked["subject_name"] = PARKED_NAME
	var walking := _tie(WALKING_ENTITY, 0.5, WALKING_SEEN, RESOLVED - 1, RESOLVED - 1)
	var sibling := _tie(SIBLING_SUBJECT_ENTITY, 0.5, SIBLING_SEEN, RESOLVED - SIBLING_SEEN_AGE, RESOLVED - SIBLING_SEEN_AGE)
	sibling["subject_name"] = SIBLING_SUBJECT_NAME
	sibling["observer_band_id"] = BandFx.FIXTURE_BAND_ID_OFFSET + WALKING_ENTITY
	return [stationed, open, walking, parked, sibling]

func _tie(subject_entity: int, strength: float, seen: Vector2i, seen_turn: int,
		contact_turn: int) -> Dictionary:
	return {
		"observer_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + HOME_ENTITY,
		"subject_band_id": BandFx.FIXTURE_BAND_ID_OFFSET + subject_entity,
		"strength": strength,
		"last_seen_x": seen.x,
		"last_seen_y": seen.y,
		"last_seen_turn": seen_turn,
		"last_contact_turn": contact_turn,
		"first_contact_turn": 5,
	}
