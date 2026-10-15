extends RefCounted

## EARLY DIPLOMACY, THE CLIENT'S SECOND HALF (issue #233, slice C2) — the Peoples tab's regard words and
## treaty rows, End treaty, the per-people border toggle and its breach confirm, the treaty sheet (over a
## road and by an envoy), a counter that names new agreed trade, and an envoy's row in the Parties zone.
##
## One chapter of the `ui_preview` state walk, appended after `dealings` and handing the reference band
## back like it. Its fixtures are the Dealings chapter's own (`DEALINGS`): the same three peoples, the
## same ties and dealings, shaped as the native decoder writes them, plus the goodwill / treaty / policy
## rows this slice reads. Every command is asserted by EQUALITY through the real `Main` builders.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 85

const DEALINGS := preload("res://tools/ui_preview/chapters/dealings.gd")
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

var h
var _fx

const PANEL_RESERVER := &"band_panel"

## What each people thinks of the player (`known`, as of a turn) and what the player thinks of it (`own`).
## The cuts are the sim's: below -0.2 wary, above 0.2 friendly.
const CUT_WARY := -0.2
const CUT_FRIENDLY := 0.2
const BRACK_THINKS := 0.6
const THORN_THINKS := -0.5
const BRACK_WE_THINK := 0.1
const THORN_WE_THINK := 0.5
const MOOR_WE_THINK := 0.0
## Brackwater's reasons: gifts and a road warm it, a broken treaty chills it.
const BRACK_GIFTS := 0.8
const BRACK_LINK := 0.2
const BRACK_BREACH := -0.4
const THORN_BREACH := -0.6
## The turn the treaty with Brackfolk was formed.
const TREATY_FORMED := 44

## A bundle the sheets compose: a shipment of flint, paid in food.
const BUNDLE_FLINT := 2.0
const BUNDLE_FOOD := 5.0
const COUNTER_BUNDLE_HIDE := 3.0
const COUNTER_BUNDLE_FOOD := 1.0
## The envoy the Thornfolk sheet sends, and the party that walks one in the Parties zone.
const ENVOY_PEOPLE := 2
const ENVOY_PARTY_ENTITY := 7391
## How long a tooltip needs to appear (the engine's own delay) and the margin the harness waits past it.
const TOOLTIP_WAIT_SECONDS := 1.0
## The widest a tab badge reads (two digits), for the tab-bar fit claim.
const TAB_WIDEST_BADGE := "88"

var _commands: Array = []


func run(harness) -> void:
	h = harness
	_fx = DEALINGS.new()
	_fx.h = harness
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	panel.set_dock(SIDE_RIGHT)
	panel.set_dock(SIDE_LEFT)
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.set_band_city_panel(panel)
	h._hud.diplomacy_command_requested.connect(func(payload: Dictionary) -> void: _commands.append(payload))

	var prior_turn: int = h._hud._band_labor.current_turn()
	FactionNames.update([
		{"faction": 0, "name": "Ashfolk"}, {"faction": DEALINGS.BRACK_FACTION, "name": DEALINGS.BRACK_PEOPLE},
		{"faction": DEALINGS.THORN_FACTION, "name": DEALINGS.THORN_PEOPLE},
		{"faction": DEALINGS.MOOR_FACTION, "name": DEALINGS.MOOR_PEOPLE}])
	h._hud.update_overlay(DEALINGS.TURN, {})
	h._hud.update_connections(_fx._ties())
	h._hud.update_band_alerts([_fx._home_band()])
	h._hud.update_faction_policies([_policy([{"other_faction": DEALINGS.THORN_FACTION, "open": false}])])
	h._hud.update_treaties(_treaties(true))
	h._hud.update_goodwill(_goodwill())
	h._hud.update_dealings(_fx._dealings())
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()

	await _peoples_states(panel)
	await _action_states(panel)
	panel.set_dock(SIDE_RIGHT)
	await h._settle()
	await _sheet_states(panel)
	await _counter_states(panel)
	panel.set_dock(SIDE_LEFT)
	await h._settle()
	await _envoy_row_states(panel)
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


# ---- (a) THE PEOPLES TAB: regard and treaty rows --------------------------------------------------

func _peoples_states(panel: BandCityPanel) -> void:
	await h._save("treaties_peoples")
	var block := panel.find_child(HudDealingsVocab.PEOPLES_BLOCK_NAME, true, false) as Control
	h._assert_hud("peoples: the Peoples tab carries a diplomacy block", block != null)
	var rows := _people_rows(block)
	h._assert_hud("peoples: one block per people the wire names, ascending (%s)" % [_row_factions(rows)],
		_row_factions(rows) == [DEALINGS.BRACK_FACTION, DEALINGS.THORN_FACTION, DEALINGS.MOOR_FACTION])
	var brack := _people_row(rows, DEALINGS.BRACK_FACTION)
	var thorn := _people_row(rows, DEALINGS.THORN_FACTION)
	var moor := _people_row(rows, DEALINGS.MOOR_FACTION)
	h._assert_hud("peoples: friendly Brackfolk read `They: friendly` and `We: neutral`",
		_text(brack).contains("They: friendly") and _text(brack).contains("We: neutral"))
	var wary := _cell(thorn, "known")
	h._assert_hud("peoples: wary Thornfolk read `They: wary`, in amber beside what it explains",
		wary != null and wary.text == "They: wary" and wary.get_theme_color("font_color") == HudStyle.WARN)
	h._assert_hud("peoples: …and we think well of them (`We: friendly`)", _text(thorn).contains("We: friendly"))
	h._assert_hud("peoples: a people nobody has reported on has NO `They:` word (never a default neutral)",
		_cell(moor, "known") == null and not _text(moor).contains("They:"))
	h._assert_hud("peoples: …but our own opinion of it still reads", _text(moor).contains("We: neutral"))
	var theirs: Label = _cell(brack, "known")
	h._assert_hud("peoples: the hover says as of which turn and WHY, in words (\"%s\")" % theirs.tooltip_text.replace("\n", " | "),
		theirs != null and theirs.tooltip_text.contains("as of turn %d" % (DEALINGS.RESOLVED - 2))
			and theirs.tooltip_text.contains("+0.8 gifts") and theirs.tooltip_text.contains("-0.4 a broken treaty")
			and theirs.tooltip_text.contains("+0.2 a road between us"))
	h._assert_hud("peoples: …strongest reason first", theirs != null
		and theirs.tooltip_text.find("gifts") < theirs.tooltip_text.find("a road between us"))
	h._assert_hud("peoples: …and no raw number stands on the row itself",
		not _text(brack).contains("0.6") and not _text(brack).contains("0.8"))
	h._assert_hud("peoples: a treaty in force reads its terms in words",
		_text(brack).contains("Road rights") and _text(brack).contains("Open borders")
			and _text(brack).contains("Agreed trade: 2.0 flint for 5.0 food"))
	h._assert_hud("peoples: …and says since when on its hover",
		_label_with(brack, "Road rights").tooltip_text.contains("In force since turn %d" % TREATY_FORMED))
	h._assert_hud("peoples: a people with no treaty reads `No treaty`", _text(thorn).contains("No treaty"))
	h._assert_hud("peoples: End treaty is offered where a treaty stands, and only there",
		_action(brack, HudDealingsVocab.PEOPLE_ACTION_END) != null
			and _action(thorn, HudDealingsVocab.PEOPLE_ACTION_END) == null
			and _action(moor, HudDealingsVocab.PEOPLE_ACTION_END) == null)
	h._assert_hud("peoples: every people can be proposed to",
		_action(brack, HudDealingsVocab.PEOPLE_ACTION_PROPOSE) != null
			and _action(thorn, HudDealingsVocab.PEOPLE_ACTION_PROPOSE) != null
			and _action(moor, HudDealingsVocab.PEOPLE_ACTION_PROPOSE) != null)
	var brack_borders := _action(brack, HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	var thorn_borders := _action(thorn, HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	var moor_borders := _action(moor, HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	h._assert_hud("peoples: the border toggle shows the sim's state: treaty-held open, override closed, default open",
		brack_borders.text == HudDealingsVocab.BORDERS_OPEN_LABEL
			and thorn_borders.text == HudDealingsVocab.BORDERS_CLOSED_LABEL
			and moor_borders.text == HudDealingsVocab.BORDERS_OPEN_LABEL)
	h._assert_hud("peoples: …and each hover says whose word it is",
		brack_borders.tooltip_text == HudDealingsVocab.BORDERS_TREATY_TOOLTIP
			and thorn_borders.tooltip_text == HudDealingsVocab.BORDERS_OVERRIDE_TOOLTIP
			and moor_borders.tooltip_text == HudDealingsVocab.BORDERS_DEFAULT_TOOLTIP)
	for row in rows:
		h._assert_hud("peoples: every block is two lines of copy and a strip (%s)" % _text(row).left(30),
			_copy_lines(row) == 2)
	h._assert_hud("peoples: the ties list follows under its own label",
		_text(panel).contains(HudDealingsVocab.BANDS_SECTION.to_upper())
			or _text(panel).contains(HudDealingsVocab.BANDS_SECTION))
	# A real hover: the engine's own tooltip, drawn over the row.
	await _hover(theirs)
	await h._save("treaties_peoples_hover")
	_unhover()


# ---- (b) END TREATY, THE BORDER TOGGLE, AND THEIR CONFIRMS ----------------------------------------

func _action_states(panel: BandCityPanel) -> void:
	var rows := _people_rows(panel.find_child(HudDealingsVocab.PEOPLES_BLOCK_NAME, true, false))
	# ---- End treaty confirms, and says what a breach costs --------------------------------------------
	_commands.clear()
	var end := _action(_people_row(rows, DEALINGS.BRACK_FACTION), HudDealingsVocab.PEOPLE_ACTION_END)
	end.emit_signal("pressed")
	await h._settle()
	var dialog := _dialog()
	h._assert_hud("end: End treaty asks first", dialog != null and _commands.is_empty())
	h._assert_hud("end: …the prompt names the people and says it costs their goodwill (\"%s\")" % (dialog.dialog_text if dialog != null else ""),
		dialog != null and dialog.dialog_text == HudDealingsVocab.TREATY_END_CONFIRM_FORMAT % DEALINGS.BRACK_PEOPLE
			and dialog.dialog_text.contains("goodwill"))
	await h._save("treaties_end_confirm")
	if dialog != null:
		dialog.emit_signal("confirmed")
	h._assert_hud("end: confirming emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("end: …spelled `end_treaty 0 1` (got `%s`)" % line,
			line == "end_treaty 0 %d" % DEALINGS.BRACK_FACTION)
	await _drop_dialog()

	# ---- the border toggle: a plain flip fires on the press ----------------------------------------------
	_commands.clear()
	var thorn_borders := _action(_people_row(rows, DEALINGS.THORN_FACTION), HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	thorn_borders.emit_signal("pressed")
	h._assert_hud("borders: opening to a people we had closed to asks nothing and emits ONE command",
		_commands.size() == 1 and _dialog() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("borders: …spelled `set_open_borders 0 open 2` (got `%s`)" % line,
			line == "set_open_borders 0 open %d" % DEALINGS.THORN_FACTION)
	_commands.clear()
	var moor_borders := _action(_people_row(rows, DEALINGS.MOOR_FACTION), HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	moor_borders.emit_signal("pressed")
	h._assert_hud("borders: closing to a people with no treaty asks nothing", _commands.size() == 1 and _dialog() == null)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("borders: …spelled `set_open_borders 0 closed 3` (got `%s`)" % line,
			line == "set_open_borders 0 closed %d" % DEALINGS.MOOR_FACTION)

	# ---- closing against a treaty that holds them open is a breach, and asks ----------------------------
	_commands.clear()
	var brack_borders := _action(_people_row(rows, DEALINGS.BRACK_FACTION), HudDealingsVocab.PEOPLE_ACTION_BORDERS)
	brack_borders.emit_signal("pressed")
	await h._settle()
	var breach := _dialog()
	h._assert_hud("borders: closing to the people whose treaty holds them open asks first",
		breach != null and _commands.is_empty())
	h._assert_hud("borders: …and says it ends the treaty, a breach that costs their goodwill (\"%s\")" % (breach.dialog_text if breach != null else ""),
		breach != null and breach.dialog_text == HudDealingsVocab.BORDERS_BREACH_CONFIRM_FORMAT % DEALINGS.BRACK_PEOPLE
			and breach.dialog_text.contains("breach") and breach.dialog_text.contains("goodwill"))
	await h._save("treaties_borders_breach_confirm")
	if breach != null:
		breach.emit_signal("confirmed")
	h._assert_hud("borders: confirming emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		h._assert_hud("borders: …spelled `set_open_borders 0 closed 1` (got `%s`)" % line,
			line == "set_open_borders 0 closed %d" % DEALINGS.BRACK_FACTION)
	await _drop_dialog()
	# Cancelling leaves nothing sent.
	_commands.clear()
	brack_borders.emit_signal("pressed")
	await h._settle()
	var again := _dialog()
	if again != null:
		again.emit_signal("canceled")
	h._assert_hud("borders: cancelling the breach sends nothing", _commands.is_empty())
	await _drop_dialog()


# ---- (c) THE TREATY SHEET: over a road, by an envoy, and with nobody to send to ---------------------

func _sheet_states(panel: BandCityPanel) -> void:
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()
	var rows := _people_rows(panel.find_child(HudDealingsVocab.PEOPLES_BLOCK_NAME, true, false))

	# ---- over the road: Brackwater has a published standing link ----------------------------------------
	_action(_people_row(rows, DEALINGS.BRACK_FACTION), HudDealingsVocab.PEOPLE_ACTION_PROPOSE).emit_signal("pressed")
	await h._settle()
	await h._save("treaties_sheet_road")
	var form := _form()
	h._assert_hud("sheet: Propose a treaty opens the treaty sheet in the band's drawer",
		form != null and String(form.get_meta(HudWidgets.VERB_FORM_META, "")) == HudDealingsVocab.COMPOSE_MISSION_TREATY)
	var text := _text(form)
	h._assert_hud("sheet: …titled with the people", text.contains(HudDealingsVocab.TREATY_SHEET_TITLE_FORMAT % DEALINGS.BRACK_PEOPLE))
	h._assert_hud("sheet: …saying it goes over the road, to the band the sim published (\"%s\")" % text.left(0),
		text.contains(HudDealingsVocab.TREATY_VIA_ROAD_FORMAT % DEALINGS.BRACK_NAME))
	h._assert_hud("sheet: …with no party picker (nobody walks)", not text.contains(HudDealingsVocab.TREATY_ENVOY_PEOPLE_LABEL))
	var send := Q.find_meta_node(form, HudDealingsVocab.TREATY_SEND_META) as Button
	h._assert_hud("sheet: Propose is greyed with no term, and the reason is in amber beside it",
		send != null and send.disabled and send.text == HudDealingsVocab.TREATY_PROPOSE_BUTTON
			and send.tooltip_text == HudDealingsVocab.TREATY_NEEDS_TERMS
			and _find_label(form, HudDealingsVocab.TREATY_NEEDS_TERMS) != null
			and _find_label(form, HudDealingsVocab.TREATY_NEEDS_TERMS).get_theme_color("font_color") == HudStyle.WARN)
	var road: CheckBox = _fx._check_named(form, HudDealingsVocab.SHEET_TREATY_ROAD)
	if road != null:
		road.button_pressed = true
	await h._settle()
	_press_meta(HudDealingsVocab.TREATY_ADD_BUNDLE_META)
	await h._settle()
	h._assert_hud("sheet: Add agreed trade adds a bundle block with both lists",
		_has(_form(), HudDealingsVocab.TREATY_BUNDLE_HEAD_FORMAT % 1)
			and _has(_form(), HudDealingsVocab.TREATY_BUNDLE_SENT_SECTION)
			and _has(_form(), HudDealingsVocab.TREATY_BUNDLE_PAID_SECTION))
	var empty_send := Q.find_meta_node(_form(), HudDealingsVocab.TREATY_SEND_META) as Button
	h._assert_hud("sheet: a bundle that carries nothing greys the send, reason beside it in amber",
		empty_send != null and empty_send.disabled
			and empty_send.tooltip_text == HudDealingsVocab.TREATY_BUNDLE_NEEDS_SENT
			and _find_label(_form(), HudDealingsVocab.TREATY_BUNDLE_NEEDS_SENT) != null)
	_add_leg_good(0, "material:flint")
	await h._settle()
	await _set_leg(0, "material:flint", BUNDLE_FLINT)
	await h._settle()
	_add_leg_good(1, "food")
	await h._settle()
	await _set_leg(0, "food", BUNDLE_FOOD)
	await h._settle()
	await h._save("treaties_sheet_road_filled")
	var filled := Q.find_meta_node(_form(), HudDealingsVocab.TREATY_SEND_META) as Button
	h._assert_hud("sheet: with a term and a bundle that carries something, Propose is live",
		filled != null and not filled.disabled and filled.tooltip_text == HudDealingsVocab.TREATY_PROPOSE_HINT)
	_commands.clear()
	if filled != null:
		filled.emit_signal("pressed")
	h._assert_hud("sheet: Propose emits ONE command and closes the sheet", _commands.size() == 1 and _form() == null)
	if _commands.size() == 1:
		var formatted := MAIN_SCRIPT.format_diplomacy_command(_commands[0])
		var home_id := BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.HOME_ENTITY
		var brack_id := BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.BRACK_ENTITY
		var flint := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(BUNDLE_FLINT))
		var food := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(BUNDLE_FOOD))
		var want := "propose_treaty 0 %d %d road_rights bundle give material flint %s ask food %s" % [
			home_id, brack_id, flint, food]
		h._assert_hud("sheet: …`%s` (got `%s`)" % [want, String(formatted.get("line", ""))],
			String(formatted.get("line", "")) == want)
		h._assert_hud("sheet: …and the receipt names the terms in words",
			String(formatted.get("message", "")).contains("Road rights")
				and String(formatted.get("message", "")).contains("Agreed trade: 2.0 flint for 5.0 food"))

	# ---- by an envoy: Thornfolk have no road, but a band of theirs is named ----------------------------
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()
	rows = _people_rows(panel.find_child(HudDealingsVocab.PEOPLES_BLOCK_NAME, true, false))
	_action(_people_row(rows, DEALINGS.THORN_FACTION), HudDealingsVocab.PEOPLE_ACTION_PROPOSE).emit_signal("pressed")
	await h._settle()
	await h._save("treaties_sheet_envoy")
	var envoy_text := _text(_form())
	h._assert_hud("sheet: with no road the sheet says an envoy walks it, and to which band",
		envoy_text.contains(HudDealingsVocab.TREATY_VIA_ENVOY_FORMAT % DEALINGS.THORN_NAME))
	h._assert_hud("sheet: …and asks for the party, like the trade party picker",
		envoy_text.contains(HudDealingsVocab.TREATY_ENVOY_PEOPLE_LABEL))
	var envoy_borders: CheckBox = _fx._check_named(_form(), HudDealingsVocab.SHEET_TREATY_BORDERS)
	if envoy_borders != null:
		envoy_borders.button_pressed = true
	await h._settle()
	var envoy_road: CheckBox = _fx._check_named(_form(), HudDealingsVocab.SHEET_TREATY_ROAD)
	if envoy_road != null:
		envoy_road.button_pressed = true
	await h._settle()
	_fx._set_party(ENVOY_PEOPLE)
	await h._settle()
	await h._save("treaties_sheet_envoy_filled")
	var envoy_send := Q.find_meta_node(_form(), HudDealingsVocab.TREATY_SEND_META) as Button
	h._assert_hud("sheet: the envoy's send reads `Send envoy` and is live",
		envoy_send != null and envoy_send.text == HudDealingsVocab.TREATY_ENVOY_BUTTON and not envoy_send.disabled)
	_commands.clear()
	if envoy_send != null:
		envoy_send.emit_signal("pressed")
	h._assert_hud("sheet: Send envoy emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		var want := "send_envoy 0 %d %d %d road_rights open_borders" % [
			BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.HOME_ENTITY, ENVOY_PEOPLE,
			BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.THORN_ENTITY]
		h._assert_hud("sheet: …`%s` (got `%s`)" % [want, line], line == want)

	# ---- nobody of theirs is known: the send is greyed with its reason ------------------------------------
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()
	rows = _people_rows(panel.find_child(HudDealingsVocab.PEOPLES_BLOCK_NAME, true, false))
	_action(_people_row(rows, DEALINGS.MOOR_FACTION), HudDealingsVocab.PEOPLE_ACTION_PROPOSE).emit_signal("pressed")
	await h._settle()
	var moor_road: CheckBox = _fx._check_named(_form(), HudDealingsVocab.SHEET_TREATY_ROAD)
	if moor_road != null:
		moor_road.button_pressed = true
	await h._settle()
	await h._save("treaties_sheet_no_target")
	var moor_send := Q.find_meta_node(_form(), HudDealingsVocab.TREATY_SEND_META) as Button
	h._assert_hud("sheet: with no band of theirs known the send is greyed, reason in amber beside it",
		moor_send != null and moor_send.disabled and moor_send.tooltip_text == HudDealingsVocab.TREATY_ENVOY_NO_TARGET
			and _find_label(_form(), HudDealingsVocab.TREATY_ENVOY_NO_TARGET) != null)
	h._hud.close_verb_form() if h._hud.has_method("close_verb_form") else null
	panel.set_active_tab(BandCityPanel.ZONE_PEOPLES)
	await h._settle()


# ---- (d) A COUNTER THAT NAMES NEW AGREED TRADE -----------------------------------------------------

func _counter_states(panel: BandCityPanel) -> void:
	panel.set_active_tab(BandCityPanel.ZONE_DEALINGS)
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()
	var block := panel.find_child(HudDealingsVocab.BLOCK_NAME, true, false) as Control
	var counter: Button = _fx._action(_fx._row(_fx._rows(block), DEALINGS.TREATY_IN), HudDealingsVocab.ANSWER_COUNTER)
	h._assert_hud("counter: a treaty proposal offers Counter", counter != null)
	if counter == null:
		return
	counter.emit_signal("pressed")
	await h._settle()
	await h._save("treaties_counter")
	var text := _text(_form())
	h._assert_hud("counter: the sheet keeps the published terms ticked and offers agreed trade of our own",
		text.contains(HudDealingsVocab.SHEET_TREATY_ROAD) and text.contains(HudDealingsVocab.TREATY_ADD_BUNDLE)
			and text.contains(HudDealingsVocab.TREATY_SHEET_ADD_HINT))
	_press_meta(HudDealingsVocab.TREATY_ADD_BUNDLE_META)
	await h._settle()
	_add_leg_good(0, "material:hide")
	await h._settle()
	await _set_leg(0, "material:hide", COUNTER_BUNDLE_HIDE)
	await h._settle()
	_add_leg_good(1, "food")
	await h._settle()
	await _set_leg(0, "food", COUNTER_BUNDLE_FOOD)
	await h._settle()
	await h._save("treaties_counter_bundle")
	var send: Array = _fx._find_meta_all(_form(), HudDealingsVocab.SHEET_ACTION_META)
	h._assert_hud("counter: the counter's send is live with a bundle that carries something",
		send.size() == 1 and not (send[0] as Button).disabled)
	_commands.clear()
	if send.size() == 1:
		(send[0] as Button).emit_signal("pressed")
	h._assert_hud("counter: Send counter emits ONE command", _commands.size() == 1)
	if _commands.size() == 1:
		var line := String(MAIN_SCRIPT.format_diplomacy_command(_commands[0]).get("line", ""))
		var flint := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(DEALINGS.PROPOSAL_FLINT))
		var food := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(DEALINGS.PROPOSAL_FOOD))
		var hide := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(COUNTER_BUNDLE_HIDE))
		var small_food := MAIN_SCRIPT.cargo_wire_text(MAIN_SCRIPT.cargo_wire_amount(COUNTER_BUNDLE_FOOD))
		var want := "answer_dealing 0 %d counter road_rights open_borders bundle give material flint %s ask food %s bundle give material hide %s ask food %s" % [
			DEALINGS.TREATY_IN, flint, food, hide, small_food]
		h._assert_hud("counter: …the kept terms then the new bundle, one grammar (got `%s`)" % line, line == want)


# ---- (e) AN ENVOY'S ROW IN THE PARTIES ZONE --------------------------------------------------------

func _envoy_row_states(panel: BandCityPanel) -> void:
	panel.set_active_tab(BandCityPanel.ZONE_PARTIES)
	h._hud.update_band_alerts([_fx._home_band(), _envoy_party()])
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()
	await h._save("treaties_envoy_party")
	var text := _text(panel)
	h._assert_hud("envoy: the Parties row names who it is for and what it carries (\"%s\")" % text.left(0),
		text.contains(DEALINGS.THORN_NAME) and text.contains("Road rights") and text.contains("Open borders"))
	var row_button := _find_party_button(panel)
	h._assert_hud("envoy: …and its hover states the proposal whole",
		row_button != null and row_button.tooltip_text.contains(
			HudDealingsVocab.ENVOY_TERMS_FORMAT % "Road rights · Open borders"))
	h._assert_hud("envoy: …under the envoy's own mark, not the scout's",
		row_button != null and HudFormat.expedition_mission_mark("envoy") == "envoy"
			and HudFormat.expedition_mission_glyph("envoy") == HudFormat.PANEL_EXPEDITION_ENVOY_GLYPH)
	# ---- the six tabs fit the strip, with EVERY badge showing at its widest -----------------------------
	# The Dealings tab made six; at the old padding the bar overran the 380px strip and the card widened
	# past its reserved edge (the dock's border then ran through the tab bar and the rows).
	var interior := BandCityPanel.PANEL_WIDTH - BandCityPanel.PANEL_CHROME_H
	h._assert_hud("tabs: the six-tab bar fits the strip's interior as the badges stand (%.0f of %.0f)" % [
		panel._tab_bar.get_combined_minimum_size().x, interior],
		panel._tab_bar.get_combined_minimum_size().x <= interior)
	for zone in [BandCityPanel.ZONE_WORK, BandCityPanel.ZONE_PARTIES, BandCityPanel.ZONE_TRADE,
			BandCityPanel.ZONE_PEOPLES, BandCityPanel.ZONE_DEALINGS]:
		panel.set_tab_badge(zone, TAB_WIDEST_BADGE, false)
	await h._settle()
	h._assert_hud("tabs: ...and with all five badges at two digits (%.0f of %.0f)" % [
		panel._tab_bar.get_combined_minimum_size().x, interior],
		panel._tab_bar.get_combined_minimum_size().x <= interior)
	h._hud.update_band_alerts([_fx._home_band()])
	h._hud.show_unit_selection(_fx._home_band())
	await h._settle()


# ---- (f) THE COMMAND BUILDERS' REFUSALS --------------------------------------------------------------

func _command_states() -> void:
	var terms := {"road_rights": true, "open_borders": false, "bundles": []}
	h._assert_hud("commands: a treaty proposal with no terms builds nothing",
		MAIN_SCRIPT.format_propose_treaty({"band_id": 1, "destination_band_id": 2,
			"terms": {"road_rights": false, "open_borders": false, "bundles": []}}).is_empty())
	h._assert_hud("commands: …nor one with no destination",
		MAIN_SCRIPT.format_propose_treaty({"band_id": 1, "terms": terms}).is_empty())
	h._assert_hud("commands: an envoy with no party builds nothing",
		MAIN_SCRIPT.format_send_envoy({"band_id": 1, "destination_band_id": 2, "party_workers": 0,
			"terms": terms}).is_empty())
	h._assert_hud("commands: ending a treaty with nobody builds nothing", MAIN_SCRIPT.format_end_treaty({}).is_empty())
	h._assert_hud("commands: a per-people border with no people builds nothing",
		MAIN_SCRIPT.format_set_borders({"open": true}).is_empty())
	h._assert_hud("commands: …nor with no state", MAIN_SCRIPT.format_set_borders({"other_faction": 2}).is_empty())


# ---- finders ---------------------------------------------------------------------------------------

func _people_rows(block: Node) -> Array:
	var rows: Array = []
	if block == null:
		return rows
	for child in block.get_children():
		if child is Control and (child as Control).has_meta(HudDealingsVocab.PEOPLE_ROW_META):
			rows.append(child)
	return rows

func _row_factions(rows: Array) -> Array:
	var ids: Array = []
	for row in rows:
		ids.append(int((row as Control).get_meta(HudDealingsVocab.PEOPLE_ROW_META)))
	return ids

func _people_row(rows: Array, faction: int) -> Control:
	for row in rows:
		if int((row as Control).get_meta(HudDealingsVocab.PEOPLE_ROW_META)) == faction:
			return row
	return null

func _text(root: Node) -> String:
	return _fx._text_of(root)

## Section labels are uppercased by their widget, so a needle is matched case-insensitively.
func _has(root: Node, needle: String) -> bool:
	return _text(root).to_lower().contains(needle.to_lower())

func _find_label(root: Node, needle: String) -> Label:
	return _fx._find_label(root, needle)

func _label_with(root: Node, needle: String) -> Label:
	return _fx._find_label(root, needle)

func _form() -> Control:
	return _fx._form()

## The regard cell of one row (`known` = They, `own` = We), or `null`.
func _cell(row: Control, which: String) -> Label:
	for node in _fx._find_meta_all(row, HudDealingsVocab.PEOPLE_GOODWILL_META):
		if String(node.get_meta(HudDealingsVocab.PEOPLE_GOODWILL_META)) == which:
			return node as Label
	return null

## One action link of a people's strip, by its action id.
func _action(row: Control, action_id: String) -> Button:
	for node in _fx._find_meta_all(row, HudDealingsVocab.PEOPLE_ACTION_META):
		if node is Button and String(node.get_meta(HudDealingsVocab.PEOPLE_ACTION_META)) == action_id:
			return node as Button
	return null

## How many LINES OF COPY a block carries: its labels, ignoring the strip's links (controls).
func _copy_lines(row: Control) -> int:
	var lines := 0
	for child in row.get_children():
		if child is Label:
			lines += 1
		elif child is HBoxContainer and not (child as Control).has_meta(HudDealingsVocab.PEOPLE_ACTION_META):
			lines += 1
	return lines

## The first confirm dialog parented on the HUD, or `null`.
func _dialog() -> ConfirmationDialog:
	for child in h._hud.get_children():
		if child is ConfirmationDialog and not (child as ConfirmationDialog).is_queued_for_deletion():
			return child as ConfirmationDialog
	return null

func _drop_dialog() -> void:
	var dialog := _dialog()
	if dialog != null:
		dialog.queue_free()
	await h.get_tree().process_frame

func _press_meta(meta: StringName) -> void:
	var node := Q.find_meta_node(_form(), meta) as Button
	h._assert_hud("treaties: the sheet has a %s control" % meta, node != null)
	if node != null:
		node.emit_signal("pressed")

## `Add a good` on the Nth leg editor of the open sheet (0-based, in tree order).
func _add_leg_good(editor_index: int, key: String) -> void:
	var menus: Array = _fx._find_meta_all(_form(), HudDealingsVocab.LEG_ADD_META)
	h._assert_hud("treaties: the sheet has leg editor %d (%d found)" % [editor_index, menus.size()],
		editor_index < menus.size())
	if editor_index >= menus.size():
		return
	var popup := (menus[editor_index] as MenuButton).get_popup()
	var wanted := HudDealingsVocab.leg_label(key)
	for index in popup.item_count:
		if popup.get_item_text(index) == wanted:
			popup.id_pressed.emit(popup.get_item_id(index))
			return
	h._assert_hud("treaties: `Add a good` lists %s" % wanted, false)

## Step the `occurrence`th leg row keyed `key` (tree order across the sheet's editors) up to `amount`
## through its own `+`. Each press rebuilds the sheet, so the button is re-found and the sheet settled.
func _set_leg(occurrence: int, key: String, amount: float) -> void:
	var presses := int(round(amount / HudDealingsVocab.LEG_STEP)) - 1
	for _i in presses:
		var plus: Button = null
		var seen := 0
		for node in _fx._find_meta_all(_form(), HudDealingsVocab.LEG_STEP_META):
			if node is Button and String(node.get_meta(HudDealingsVocab.LEG_STEP_META)) == key \
					and (node as Button).text == HudWorkVocab.STEPPER_PLUS_FACE:
				if seen == occurrence:
					plus = node
					break
				seen += 1
		if plus == null:
			break
		plus.emit_signal("pressed")
		await h._settle()

## Park the real mouse over `control` and wait out the engine's tooltip delay, so the frame carries the
## tooltip the player would see.
func _hover(control: Control) -> void:
	if control == null:
		return
	var point := InputProbe.canvas_to_window(h.get_viewport(), h.get_window(),
		control.get_global_rect().get_center())
	InputProbe.hover(h.get_viewport(), point)
	# The harness freezes `Engine.time_scale` at 0 for bit-identical frames, and the engine's tooltip
	# delay is a scaled timer: thaw the clock for the length of the hover and freeze it again, on a
	# timer that ignores the scale so the wait itself cannot hang.
	var frozen := Engine.time_scale
	Engine.time_scale = 1.0
	await h.get_tree().create_timer(TOOLTIP_WAIT_SECONDS, true, false, true).timeout
	Engine.time_scale = frozen
	await h._settle()

func _unhover() -> void:
	InputProbe.hover(h.get_viewport(), Vector2(-1.0, -1.0))

func _find_party_button(root: Node) -> Button:
	if root is Button and (root as Button).tooltip_text.contains(HudDealingsVocab.ENVOY_TERMS_FORMAT.split("%s")[0]):
		return root as Button
	for child in root.get_children():
		var found := _find_party_button(child)
		if found != null:
			return found
	return null


# ---- fixtures: shaped exactly as `dict/dealings.rs`, `dict/campaign.rs` and `dict/population.rs` write them

func _policy(overrides: Array) -> Dictionary:
	return {"faction": 0, "open_borders": true, "accept_own_gifts": true, "border_overrides": overrides}

func _goodwill_row(faction: int, total: float, reported: int, parts: Dictionary = {}) -> Dictionary:
	var row := {"other_faction": faction, "gifts": 0.0, "exchanges": 0.0, "stationed": 0.0, "link": 0.0,
		"treaty": 0.0, "breach": 0.0, "total": total, "reported_turn": reported}
	row.merge(parts, true)
	return row

func _goodwill() -> Dictionary:
	return {
		"known": [
			_goodwill_row(DEALINGS.BRACK_FACTION, BRACK_THINKS, DEALINGS.RESOLVED - 2,
				{"gifts": BRACK_GIFTS, "link": BRACK_LINK, "breach": BRACK_BREACH}),
			_goodwill_row(DEALINGS.THORN_FACTION, THORN_THINKS, DEALINGS.RESOLVED - 4,
				{"breach": THORN_BREACH}),
		],
		"own": [
			_goodwill_row(DEALINGS.BRACK_FACTION, BRACK_WE_THINK, DEALINGS.RESOLVED),
			_goodwill_row(DEALINGS.THORN_FACTION, THORN_WE_THINK, DEALINGS.RESOLVED),
			_goodwill_row(DEALINGS.MOOR_FACTION, MOOR_WE_THINK, DEALINGS.RESOLVED),
		],
		"cuts": {"wary_below": CUT_WARY, "friendly_above": CUT_FRIENDLY},
	}

func _treaties(with_treaty: bool) -> Dictionary:
	var flint := {"material_id": "flint", "amount": BUNDLE_FLINT, "readings": [], "variety_name": ""}
	var treaties: Array = []
	if with_treaty:
		treaties.append({"other_faction": DEALINGS.BRACK_FACTION, "formed_turn": TREATY_FORMED, "terms": {
			"road_rights": true, "open_borders": true, "bundles": [{
				"sent": {"food": 0.0, "fodder": 0.0, "materials": [flint]},
				"returned": {"food": BUNDLE_FOOD, "fodder": 0.0, "materials": []}}]}})
	return {"treaties": treaties, "standing_links": [{
		"viewer_band": BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.HOME_ENTITY,
		"other_band": BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.BRACK_ENTITY,
		"other_faction": DEALINGS.BRACK_FACTION}]}

func _envoy_party() -> Dictionary:
	return {
		"id": "Envoy %d" % ENVOY_PARTY_ENTITY,
		"name": DEALINGS.HOME_NAME,
		"size": ENVOY_PEOPLE,
		"entity": ENVOY_PARTY_ENTITY,
		"band_id": BandFx.FIXTURE_BAND_ID_OFFSET + ENVOY_PARTY_ENTITY,
		"faction": 0,
		"pos": [DEALINGS.HOME_TILE.x, DEALINGS.HOME_TILE.y],
		"current_x": DEALINGS.HOME_TILE.x,
		"current_y": DEALINGS.HOME_TILE.y,
		"home_band_entity": DEALINGS.HOME_ENTITY,
		"turns_of_food": 999.0,
		"stores": {},
		"is_expedition": true,
		"expedition_mission": "envoy",
		"expedition_phase": "outbound",
		"expedition_destination_band": BandFx.FIXTURE_BAND_ID_OFFSET + DEALINGS.THORN_ENTITY,
		"expedition_destination_name": "",
		"expedition_carry_cap": 0.0,
		"envoy_terms": {"road_rights": true, "open_borders": true, "bundles": []},
		"tile_info": {"x": DEALINGS.HOME_TILE.x, "y": DEALINGS.HOME_TILE.y, "terrain_label": "Prairie Steppe",
			"tags_text": "Fertile", "visibility_state": "active", "food_module": "", "food_module_label": "None"},
	}
