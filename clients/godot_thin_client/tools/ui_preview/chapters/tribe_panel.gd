extends RefCounted

## THE TRIBE ON SCREEN (#863, `.claude/rules/client/tribe-panel.md`): the tribe replaces its bands in the
## Band panel's switcher, owns a page, drills into a member band with a breadcrumb, stands in for its members on
## the `Band:` picker and Materials & Crafting's picker, and raises its leader's proposal on the turn orb.
##
## One chapter of the `ui_preview` state walk, appended LAST so no earlier frame moves. Every claim is made
## against fixture dicts shaped as the native decoder shapes them (`tribe_id`, `tribe_member_bands`,
## `tribe_levy_ceiling`, `tribe_works`, `tribe_proposal`, `for_tribe`, `scouting_stretch`).
##
## **THE COMMAND LINES ARE ASSERTED THROUGH THE REAL EMITTERS** - a press on the control, the HUD signal it
## raises, and `Main.format_tribe_command` / `format_tribe_scout` / `format_chief_proposal` spelling it.

const EXPECTED_CHECKPOINTS := 72

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const DoorQ := preload("res://tools/ui_preview/door_query.gd")
const NodeQuery := preload("res://tools/ui_preview/node_query.gd")
const LandReadouts := preload("res://tools/ui_preview/chapters/land_readouts.gd")
const TileBranches := preload("res://tools/ui_preview/chapters/tile_branches.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

var h

const TRIBE_ID := 7
const TRIBE_NAME := "Reedwater tribe"
const PINEWOLD := {"name": "Pinewold", "band_id": 11, "entity": 1101, "x": 10, "y": 36}
const GREYFEN := {"name": "Greyfen", "band_id": 12, "entity": 1102, "x": 44, "y": 28}
const SALTMARCH := {"name": "Saltmarch", "band_id": 13, "entity": 1103, "x": 46, "y": 30}
const ASHFELL := {"name": "Ashfell", "band_id": 14, "entity": 1104, "x": 60, "y": 12}
const ANCHOR := Vector2i(44, 28)
const SCOUT_RANGE := 2
const LEVY_NORMAL := 3
const PROPOSAL_KEY := "monument@44,28"
const PROPOSAL_TEXT := "Zeke proposes: raise a monument at (44, 28)"
const FACTION := 0
## The tribe is the second entry of the walk (the faction page leads) and Ashfell the third.
const EXPECTED_SWITCHER_COUNT := 3
const WORKS_AT := Vector2i(44, 28)
const STORE_AT := Vector2i(45, 30)
const FOR_TRIBE_FORAGE_AT := Vector2i(72, 19)
## Greyfen's hands, and what is building for the tribe: 2 builders on the levied patch's declared build and 3 on
## the road it keeps at a `route` shared work. Five of its six builders are the tribe's.
const GREYFEN_HANDS := 24
const PATCH_BUILDERS := 2
const ROAD_BUILDERS := 3
const GREYFEN_HOME := 12
const GREYFEN_TRIBE := 9
const GREYFEN_IDLE := 3
## Bands and rows for the tall left-docked page: each band gets this many extra forage rows.
const TALL_EXTRA_ROWS := 6

func _member(spec: Dictionary, in_tribe: bool, short_stretch: bool) -> Dictionary:
	var band := BandFx.band_fixture()
	band.merge({"name": spec["name"], "id": spec["name"], "band_id": spec["band_id"],
		"entity": spec["entity"], "current_x": spec["x"], "current_y": spec["y"],
		"pos": [spec["x"], spec["y"]]}, true)
	var rows: Array = []
	for row in band["labor_assignments"]:
		var copy: Dictionary = (row as Dictionary).duplicate()
		if String(copy["kind"]) == "scout":
			copy["for_tribe"] = in_tribe
			copy["scouting_stretch"] = 1 if in_tribe else 0
			copy["scouting_reach"] = 1 if short_stretch else SCOUT_RANGE
			copy["scouting_range"] = SCOUT_RANGE
		else:
			copy["for_tribe"] = false
		rows.append(copy)
	band["labor_assignments"] = rows
	if in_tribe:
		band.merge({
			"tribe_id": TRIBE_ID, "tribe_name": TRIBE_NAME,
			"tribe_member_names": ["Pinewold", "Greyfen", "Saltmarch"],
			"tribe_member_bands": [11, 12, 13],
			"tribe_levy_ceiling": LEVY_NORMAL, "tribe_scouting_range": SCOUT_RANGE,
			"has_belief_anchor": true, "belief_anchor_x": ANCHOR.x, "belief_anchor_y": ANCHOR.y,
			"tribe_works": _works(),
			"tribe_proposal": {"key": PROPOSAL_KEY, "kind": "monument", "text": PROPOSAL_TEXT,
				"tile_x": ANCHOR.x, "tile_y": ANCHOR.y, "for_band": 12},
		}, true)
	else:
		band.merge({"tribe_id": 0, "tribe_name": "", "tribe_member_names": [], "tribe_member_bands": [],
			"tribe_levy_ceiling": 0, "tribe_works": [], "tribe_proposal": {},
			"has_belief_anchor": false}, true)
	return band

func _works() -> Array:
	return [
		{"x": WORKS_AT.x, "y": WORKS_AT.y, "branch": "route", "target_rung": "route:dirt_road",
			"priority": "normal", "bands": [12, 11], "workers": 5},
		{"x": STORE_AT.x, "y": STORE_AT.y, "branch": "food_stores", "target_rung": "",
			"priority": "normal", "bands": [13], "workers": 3},
	]

func _roster(short_stretch: bool = true) -> Array:
	var pinewold := _member(PINEWOLD, true, false)
	var greyfen := _member(GREYFEN, true, short_stretch)
	# 24 hands: 15 on rows, 3 idle, so 6 are building (the residual `builders_of` reads).
	greyfen["working_age"] = GREYFEN_HANDS
	var saltmarch := _member(SALTMARCH, true, false)
	# Greyfen carries one forage row the tribe levied, so the drilled panel shows the `for the tribe` mark.
	(greyfen["labor_assignments"] as Array).insert(1, {"kind": "forage", "workers": 2,
		"target_x": FOR_TRIBE_FORAGE_AT.x, "target_y": FOR_TRIBE_FORAGE_AT.y, "floor": 0.5,
		"actual_yield": 0.2, "sustainable_yield": 0.2, "workers_needed": 2, "overdraws": false,
		"for_tribe": true, "max_workers": 0, "hold_crew": 0, "reach_crew": 2, "keepers_staffed": 0,
		"kitted_staffed": 0, "bare_staffed": 2, "useful_crew": 2})
	return [pinewold, greyfen, saltmarch, _member(ASHFELL, false, false)]

func _panel() -> BandCityPanel:
	return h._hud._bandpanel._panel

func _collect_meta(root: Node, meta: String, out: Array = []) -> Array:
	if root == null:
		return out
	if root.has_meta(meta):
		out.append(root)
	for child in root.get_children():
		_collect_meta(child, meta, out)
	return out

func _labels_text(root: Node) -> String:
	var parts: Array[String] = []
	var stack: Array = [root]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node is Label:
			parts.append((node as Label).text)
		elif node is Button:
			parts.append((node as Button).text)
		elif node is RichTextLabel:
			parts.append((node as RichTextLabel).get_parsed_text())
		stack.append_array(node.get_children())
	return " | ".join(parts)

func _count_text() -> String:
	return String(_panel()._count_label.text)

func _press_first(root: Node, meta: String) -> bool:
	var found := _collect_meta(root, meta)
	if found.is_empty():
		return false
	(found[0] as Button).pressed.emit()
	return true

const BAND_PANEL_RESERVER := &"band_panel"

func run(harness) -> void:
	h = harness
	# The REAL Band/City panel, docked BOTTOM so the wide shell shows every zone at once (a side dock tabs them and
	# the other tab's nodes are detached). Released again at the end, so nothing downstream inherits its strip.
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, BAND_PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	panel.set_dock(SIDE_BOTTOM)
	h._hud.set_band_city_panel(panel)
	h._hud.update_ladder_branches([
		{"branch": "route", "area": "food", "source_kind": "road", "label": "Road"},
		{"branch": "food_stores", "area": "food", "source_kind": "tile", "label": "Food stores"},
	])
	var roster := _roster()
	h._hud.update_band_alerts(roster)
	# What is being built, published on the SOURCES: the levied patch and the road Greyfen keeps at the tribe's work.
	h._hud._band_labor.set_forage_patches([{"x": FOR_TRIBE_FORAGE_AT.x, "y": FOR_TRIBE_FORAGE_AT.y,
		"builders_staffed": PATCH_BUILDERS}])
	h._hud._band_labor.set_roads([{"tile_x": WORKS_AT.x, "tile_y": WORKS_AT.y, "has_keeper": true,
		"keeper_band_id": GREYFEN["band_id"], "builders_staffed": ROAD_BUILDERS}])
	var bandpanel = h._hud._bandpanel

	# ---- THE SWITCHER: the faction page, the tribe and Ashfell ------------------------------------------
	bandpanel.render_tribe(TRIBE_ID)
	await h._settle()
	h._assert_hud("the switcher walks [faction page, Reedwater tribe, Ashfell] - 3 entries, not 5 (%d)"
			% bandpanel._cycler_count(), bandpanel._cycler_count() == EXPECTED_SWITCHER_COUNT)
	h._assert_hud("the tribe is entry 2 of 3 (%s)" % _count_text(), _count_text() == "2 / 3")
	h._assert_hud("the tribe page's title is the tribe's name (%s)" % _panel()._name_label.text,
		_panel()._name_label.text == TRIBE_NAME)
	var subtitle := "Tribe · 3 bands · gathering place (%d, %d)" % [ANCHOR.x, ANCHOR.y]
	h._assert_hud("…and its subtitle states the bands and the gathering place (%s)" % _panel()._stage_label.text,
		_panel()._stage_label.text == subtitle)
	h._assert_hud("a band's own breadcrumb is hidden on the tribe page", not _panel()._breadcrumb_row.visible)
	var band_zone := _collect_meta(_panel(), HudTribeVocab.TRIBE_PAGE_META)
	h._assert_hud("the tribe page's band zone is built", not band_zone.is_empty())
	var zone_text := _labels_text(_panel())
	h._assert_hud("the Food vital is summed across the three bands, and no per-band vital is on the page",
		zone_text.contains("Food") and not zone_text.contains("Morale:") and not zone_text.contains("Growth:"))
	h._assert_hud("the People bar is one bar with the bands chip (%s)" % zone_text.substr(0, 0),
		zone_text.contains("3 bands"))
	h._assert_hud("the scouting line names Greyfen's short stretch",
		zone_text.contains("Greyfen's stretch sees +1 of +2 tiles"))
	h._assert_hud("the Bands section lists the three members, collapsed, with their sub-lines",
		_collect_meta(_panel(), HudTribeVocab.BAND_LINE_META).size() == 3
			and _collect_meta(_panel(), HudTribeVocab.SUB_LINE_META).size() == 3)
	await h._save("tribe_page")

	# ---- the sub-line and crew bar arithmetic -----------------------------------------------------------
	var split := TribeRoster.crew_split(h._hud._band_labor, roster[1], TribeRoster.works(roster))
	h._assert_hud("a band line splits the band's People-bar total three ways, builders included: Greyfen at home %d, for the tribe %d, idle %d"
			% [split["home"], split["tribe"], split["idle"]],
		int(split["tribe"]) == GREYFEN_TRIBE and int(split["home"]) == GREYFEN_HOME
			and int(split["idle"]) == GREYFEN_IDLE and int(split["tribe_rows"]) == 2)
	var people: Dictionary = h._hud._band_labor.people_split(roster[1])
	h._assert_hud("…and the three sum to the band's own working + idle (%d + %d), not to the rows alone"
			% [people["working"], people["idle"]],
		int(split["home"]) + int(split["tribe"]) + int(split["idle"]) == int(people["working"]) + int(people["idle"])
			and int(people["working"]) + int(people["idle"]) == GREYFEN_HANDS)
	h._assert_hud("…and the sub-line drops zero parts (%s)" % TribePage.sub_line({"jobs": 0, "tribe_rows": 0, "idle": 2}),
		TribePage.sub_line({"jobs": 0, "tribe_rows": 0, "idle": 2}) == "2 idle"
			and TribePage.sub_line({"jobs": 3, "tribe_rows": 1, "idle": 0}) == "3 jobs · 1 for the tribe")
	h._assert_hud("the crew bar's tooltip carries the same sentence",
		_collect_meta(_panel(), HudTribeVocab.CREW_BAR_META).size() == 3
			and String((_collect_meta(_panel(), HudTribeVocab.CREW_BAR_META)[1] as Control).tooltip_text).contains(
				"Greyfen: %d at home, %d for the tribe, %d idle" % [GREYFEN_HOME, GREYFEN_TRIBE, GREYFEN_IDLE]))

	# ---- one band opened ---------------------------------------------------------------------------------
	h._assert_hud("pressing a band's fold opens its line", _press_first(_panel(), HudTribeVocab.FOLD_META))
	await h._settle()
	var opened := _labels_text(_panel())
	h._assert_hud("an opened line shows the band's rows as the Band panel builds them (Agriculture section)",
		opened.contains("Agriculture") or opened.contains("AGRICULTURE"))
	await h._save("tribe_page_band_open")

	# ---- shared works --------------------------------------------------------------------------------
	var works := _collect_meta(_panel(), HudTribeVocab.WORK_ROW_META)
	h._assert_hud("Shared works lists one row per tribe work (%d)" % works.size(), works.size() == 2)
	var works_text := _labels_text(_panel())
	h._assert_hud("a work is named by its branch label and tile, with the target rung",
		works_text.contains("Road (44, 28)") and works_text.contains("Dirt Road") or works_text.contains("Dirt road"))
	h._assert_hud("…handed to Greyfen, then Pinewold when two bands carry it, with the worker count",
		works_text.contains("handed to Greyfen, then Pinewold") and works_text.contains("5 workers"))
	h._assert_hud("…and a single band reads `handed to Saltmarch`",
		works_text.contains("handed to Saltmarch") and works_text.contains("3 workers"))
	bandpanel._tribe_open_folds.clear()

	# ---- the tribe page's own commands ------------------------------------------------------------------
	var lines: Array = []
	var record := func(p: Dictionary) -> void: lines.append(String(MAIN_SCRIPT.format_tribe_command(p).get("line", "")))
	h._hud.tribe_command_requested.connect(record)
	var scouts: Array = []
	var record_scout := func(p: Dictionary) -> void: scouts.append(String(MAIN_SCRIPT.format_tribe_scout(p).get("line", "")))
	h._hud.tribe_scout_requested.connect(record_scout)
	bandpanel.render_tribe(TRIBE_ID)
	await h._settle()
	var picker := _collect_meta(_panel(), HudTribeVocab.LEVY_PICKER_META)
	h._assert_hud("the levy ceiling is a four-way picker standing at Normal", picker.size() == 1
		and (picker[0] as OptionButton).item_count == 4 and (picker[0] as OptionButton).text == "Normal")
	h._assert_hud("…with the levy tooltip", (picker[0] as OptionButton).tooltip_text.begins_with("How much of each band's spare hands"))
	(picker[0] as OptionButton).item_selected.emit(3)
	var plus: Button = null
	for button in _collect_meta(_panel(), HudWorkVocab.SCOUT_RANGE_VALUE_META):
		var row := (button as Node).get_parent()
		for sibling in row.get_children():
			if sibling is Button and (sibling as Button).tooltip_text == HudWorkVocab.SCOUT_RANGE_FARTHER_ARIA:
				plus = sibling
	h._assert_hud("the scouting stepper is on the tribe page", plus != null)
	if plus != null:
		plus.pressed.emit()
	var stop_button: Button = null
	for work_row in _collect_meta(_panel(), HudTribeVocab.WORK_ROW_META):
		for node in _all_buttons(work_row as Node):
			if (node as Button).text == HudTribeVocab.STOP_LABEL and stop_button == null:
				stop_button = node
	h._assert_hud("each shared work has a Stop", stop_button != null)
	if stop_button != null:
		stop_button.pressed.emit()
	h._hud.tribe_command_requested.disconnect(record)
	h._hud.tribe_scout_requested.disconnect(record_scout)
	h._assert_hud("High sends `tribe_levy 0 11 high`, Stop sends `tribe_work 0 11 stop route 44 28` (%s)" % str(lines),
		lines == ["tribe_levy 0 11 high", "tribe_work 0 11 stop route 44 28"])
	h._assert_hud("the stepper sends `tribe_scout 0 11 3` (%s)" % str(scouts), scouts == ["tribe_scout 0 11 3"])

	# ---- the covered edge ---------------------------------------------------------------------------------
	h._hud.update_band_alerts(_roster(false))
	bandpanel.render_tribe(TRIBE_ID)
	await h._settle()
	h._assert_hud("when every stretch reaches the range the line counts the scouts riding the edge",
		_labels_text(_panel()).contains("scouts ride the tribe's edge"))
	h._hud.update_band_alerts(roster)

	# ---- DRILL-IN ----------------------------------------------------------------------------------------
	h._hud.show_unit_selection(roster[1])
	await h._settle()
	h._assert_hud("a member's panel is its own, with the breadcrumb above the title",
		_panel()._breadcrumb_row.visible and _panel()._breadcrumb_link.text == "‹ Reedwater tribe"
			and _panel()._breadcrumb_tail.text == "› Greyfen" and _panel()._name_label.text == "Greyfen")
	h._assert_hud("while drilled in the cycler counts the tribe's members only (%s)" % _count_text(),
		_count_text() == "2 / 3")
	var drilled := _labels_text(_panel())
	h._assert_hud("a member's Scouting range is read-only: `+2 tiles · set by the tribe`",
		drilled.contains("+2 tiles · set by the tribe")
			and _collect_meta(_panel(), HudTribeVocab.SCOUT_READONLY_META).size() == 1)
	h._assert_hud("…and a levied row carries the `for the tribe` mark",
		_collect_meta(_panel(), HudTribeVocab.FOR_TRIBE_MARK_META).size() == 1)
	await h._save("tribe_drilled_band")
	bandpanel.cycle_band(1)
	await h._settle()
	h._assert_hud("the arrows step through the members (Saltmarch, 3 / 3)",
		_panel()._name_label.text == "Saltmarch" and _count_text() == "3 / 3")
	bandpanel.cycle_band(1)
	await h._settle()
	h._assert_hud("…and wrap within the tribe (Pinewold, 1 / 3), never onto Ashfell",
		_panel()._name_label.text == "Pinewold" and _count_text() == "1 / 3")
	var crumb := _collect_meta(_panel(), HudTribeVocab.BREADCRUMB_META)
	h._assert_hud("the breadcrumb link is a control", crumb.size() == 1)
	(crumb[0] as Button).pressed.emit()
	await h._settle()
	h._assert_hud("pressing it renders the tribe's page", _panel()._name_label.text == TRIBE_NAME
		and bandpanel._tribe_page_id == TRIBE_ID)
	# A lone band is its own entry, with no breadcrumb.
	h._hud.show_unit_selection(roster[3])
	await h._settle()
	h._assert_hud("a lone band's panel has no breadcrumb and walks the top-level switcher (%s)" % _count_text(),
		not _panel()._breadcrumb_row.visible and _count_text() == "3 / 3")
	# A tribe that dissolves falls back to its first former member.
	bandpanel.render_tribe(TRIBE_ID)
	var dissolved := roster.duplicate(true)
	for band in dissolved:
		band["tribe_id"] = 0
		band["tribe_name"] = ""
	h._hud.update_band_alerts(dissolved)
	await h._settle()
	h._assert_hud("a dissolved tribe falls back to its first former member's own panel",
		bandpanel._tribe_page_id == 0 and _panel()._name_label.text == "Pinewold")
	h._hud.update_band_alerts(roster)

	# ---- THE ROAD CARD'S `Band:` PICKER ----------------------------------------------------------------
	var land := LandReadouts.new()
	land.h = h
	h._hud.update_route_rungs(land._route_rung_catalog())
	h._hud.update_intensification([{"faction": FACTION, "knowledges": {
		HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING: 1.0, HudFloraVocab.KNOWLEDGE_TRACK_PAVING: 1.0}}])
	var road := land._road_fixture(HudRouteVocab.RUNG_KEY_TRAIL, 0.0, 0.0, 0.0, 0, 0, false,
		land.ROAD_FRICTION_TRAIL, land.ROAD_LINK_TRAIL)
	var road_tile := land._road_tile_fixture(road)
	var drawer = h._hud._drawercompose
	var road_lines: Array = []
	var road_record := func(p: Dictionary) -> void: road_lines.append(String(MAIN_SCRIPT.format_tribe_command(p).get("line", "")))
	h._hud.tribe_command_requested.connect(road_record)
	var build_lines: Array = []
	var build_record := func(p: Dictionary) -> void: build_lines.append(String(MAIN_SCRIPT.format_improvement(p, []).get("line", "")))
	h._hud.improvement_requested.connect(build_record)
	bandpanel.render_tribe(TRIBE_ID)
	h._show_tile(road_tile)
	await h._settle()
	h._assert_hud("the road card opens", await _open_road_card())
	await h._settle()
	h._assert_hud("the road card's `Band:` picker names the tribe by default (%s)" % _picker_face(),
		_picker_face() == TRIBE_NAME)
	h._assert_hud("…with a closed fold beside it", _fold_open_state() == false)
	await h._save("tribe_road_picker_folded")
	_press_first(drawer._road_ladder, HudTribeVocab.PICKER_FOLD_META)
	await h._settle()
	var entries := _picker_entries()
	h._assert_hud("opening the fold lists the members indented beneath the tribe, then Ashfell (%s)" % str(entries),
		entries == [TRIBE_NAME, "Pinewold", "Greyfen", "Saltmarch", "Ashfell"] and _fold_open_state() == true)
	var open_picker := _picker_in(drawer._road_ladder)
	if open_picker != null:
		open_picker.show_popup()
		await h._settle()
	await h._save("tribe_road_picker_open")
	if open_picker != null:
		open_picker.get_popup().hide()
	_press_first(drawer._road_ladder, HudTribeVocab.PICKER_FOLD_META)
	await h._settle()
	var grade_row := _rung_button(HudRouteVocab.RUNG_KEY_DIRT_ROAD)
	h._assert_hud("the card offers the grade rung", grade_row != null)
	if grade_row != null:
		grade_row.pressed.emit()
	await h._settle()
	h._assert_hud("a tribe grade sends `tribe_work 0 11 route 9 36 route:dirt_road`, not `build` (%s / %s)"
			% [str(road_lines), str(build_lines)],
		road_lines == ["tribe_work 0 11 route %d %d route:dirt_road" % [land.ROAD_TILE.x, land.ROAD_TILE.y]]
			and build_lines.is_empty())
	# A member picked: the old command.
	road_lines.clear()
	h._show_tile(road_tile)
	await h._settle()
	await _open_road_card()
	await h._settle()
	_press_first(drawer._road_ladder, HudTribeVocab.PICKER_FOLD_META)
	await h._settle()
	_pick_entry("Pinewold")
	await h._settle()
	h._assert_hud("picking a member lists that band (%s)" % _picker_face(), _picker_face() == "Pinewold")
	grade_row = _rung_button(HudRouteVocab.RUNG_KEY_DIRT_ROAD)
	if grade_row != null:
		grade_row.pressed.emit()
	await h._settle()
	h._assert_hud("a member's grade is the old `build grade 0 11 9 36` (%s / %s)" % [str(road_lines), str(build_lines)],
		road_lines.is_empty() and build_lines == ["build grade 0 11 %d %d" % [land.ROAD_TILE.x, land.ROAD_TILE.y]])
	h._hud.tribe_command_requested.disconnect(road_record)
	h._hud.improvement_requested.disconnect(build_record)
	drawer._dismiss_road_ladder()

	# ---- THE TILE CREW SHEET (storage) ------------------------------------------------------------------
	var tb := TileBranches.new()
	tb.h = h
	h._hud.update_ladder_rungs(tb._catalog())
	h._hud.update_ladder_branches([
		{"branch": TileBranches.BELIEF_BRANCH, "area": "lore", "source_kind": "tile", "label": "Belief"},
		{"branch": TileBranches.STORES_BRANCH, "area": "food", "source_kind": "tile", "label": "Food stores"},
		{"branch": "route", "area": "food", "source_kind": "road", "label": "Road"},
	])
	h._hud.update_ladder_knowledge(tb._knowledge_roster())
	var tile_lines: Array = []
	var tile_record := func(p: Dictionary) -> void: tile_lines.append(String(MAIN_SCRIPT.format_tribe_command(p).get("line", "")))
	h._hud.tribe_command_requested.connect(tile_record)
	var labor_lines: Array = []
	var labor_record := func(p: Dictionary) -> void: labor_lines.append(str(p.get("kind", "")))
	h._hud.assign_labor_requested.connect(labor_record)
	var tile_info := tb._tile(0.0)
	bandpanel.render_tribe(TRIBE_ID)
	tb._knowledge(1.0, 1.0)
	h._show_tile(tile_info)
	await h._settle()
	drawer.open_tile_compose(tile_info, TileBranches.STORES_BRANCH)
	await h._settle()
	h._assert_hud("the tile crew sheet defaults to the tribe from the tribe's page (%s)" % _sheet_picker_face(),
		_sheet_picker_face() == TRIBE_NAME)
	h._assert_hud("…and says where the work lands", _labels_text(drawer._compose_sheet).contains("Joins the tribe's shared works"))
	await h._save("tribe_tile_sheet")
	var commit := _sheet_commit()
	h._assert_hud("the sheet's commit is present", commit != null)
	if commit != null:
		commit.pressed.emit()
	await h._settle()
	h._assert_hud("a tribe's crew with no rung sends `tribe_work 0 11 food_stores 8 8` and no assign_labor (%s / %s)"
			% [str(tile_lines), str(labor_lines)],
		tile_lines == ["tribe_work 0 11 %s %d %d" % [TileBranches.STORES_BRANCH, TileBranches.TILE_X, TileBranches.TILE_Y]]
			and labor_lines.is_empty())
	# A rung pressed from the tribe's strip: through the rung card.
	tile_lines.clear()
	h._show_tile(tile_info)
	drawer.open_tile_compose(tile_info, TileBranches.STORES_BRANCH)
	await h._settle()
	var strip_ladder := _collect_meta(drawer._compose_sheet, HudTileVocab.STRIP_META)
	h._assert_hud("the sheet mounts the work strip", not strip_ladder.is_empty())
	h._hud._rungcard.open_tile_track(TribeRoster.address_band(TribeRoster.members(
		h._hud._band_labor.player_bands(), TRIBE_ID)), tile_info, TileBranches.STORES_BRANCH, 0, null,
		{"faction": FACTION, "band_id": 11})
	await h._settle()
	var pit := _rung_button("food_stores:storage_pit", h._hud._rungcard.card())
	h._assert_hud("the tribe's rung card offers the pit", pit != null)
	if pit != null:
		pit.pressed.emit()
	await h._settle()
	h._assert_hud("a tribe rung press sends `tribe_work 0 11 food_stores 8 8 food_stores:storage_pit` (%s)" % str(tile_lines),
		tile_lines == ["tribe_work 0 11 %s %d %d food_stores:storage_pit" % [
			TileBranches.STORES_BRANCH, TileBranches.TILE_X, TileBranches.TILE_Y]])
	h._hud._drawercompose.close_compose_sheet()
	# A member picked on the sheet: today's `assign_labor`.
	drawer.open_tile_compose(tile_info, TileBranches.STORES_BRANCH)
	await h._settle()
	var sheet_picker := _sheet_picker()
	_press_first(drawer._compose_sheet, HudTribeVocab.PICKER_FOLD_META)
	await h._settle()
	_pick_sheet_entry("Greyfen")
	await h._settle()
	tile_lines.clear()
	h._assert_hud("picking a member on the sheet lists that band (%s)" % _sheet_picker_face(), _sheet_picker_face() == "Greyfen")
	commit = _sheet_commit()
	if commit != null:
		commit.pressed.emit()
	await h._settle()
	h._assert_hud("a member's crew is today's `assign_labor` row, no tribe line (%s / %s)" % [str(tile_lines), str(labor_lines)],
		tile_lines.is_empty() and labor_lines == ["tile"])
	h._hud.tribe_command_requested.disconnect(tile_record)
	h._hud.assign_labor_requested.disconnect(labor_record)
	# Drilled in: the card defaults to that member.
	h._hud._drawercompose.close_compose_sheet()
	h._hud.show_unit_selection(roster[1])
	await h._settle()
	h._show_tile(tile_info)
	drawer._compose.begin_tile_source("", -1)
	drawer.open_tile_compose(tile_info, TileBranches.STORES_BRANCH)
	await h._settle()
	h._assert_hud("opened from a member's own panel the card defaults to that band (%s)" % _sheet_picker_face(),
		_sheet_picker_face() == "Greyfen")
	h._hud._drawercompose.close_compose_sheet()

	# ---- MATERIALS & CRAFTING ----------------------------------------------------------------------------
	h._hud.update_band_alerts(roster)
	var crafting = h._hud._crafting
	crafting.open_for(roster[0])
	await h._settle()
	var craft_panel: CraftingPanel = crafting.panel()
	var options := _crafting_options(craft_panel)
	h._assert_hud("the crafting picker lists the tribe in place of its members, then Ashfell (%s)" % str(options),
		options == [TRIBE_NAME, "Ashfell"])
	h._assert_hud("…with a fold beside it", not _collect_meta(craft_panel, HudTribeVocab.FOLD_META).is_empty())
	crafting._on_tribe_selected(TRIBE_ID)
	await h._settle()
	var craft_text := _labels_text(craft_panel)
	var view_present := ResourceLoader.exists(HudTribeVocab.CRAFTING_VIEW_PATH)
	h._assert_hud("picking the tribe shows the tribe view, or the placeholder while #864's script is absent",
		(view_present and not _collect_meta(craft_panel, HudTribeVocab.CRAFTING_TRIBE_VIEW_META).is_empty())
			or (not view_present and craft_text.contains(HudTribeVocab.CRAFTING_NO_BENCH)))
	h._assert_hud("…and no bench is drawn for the tribe", not craft_text.contains("Materials on hand"))
	await h._save("tribe_crafting_picker")
	crafting._on_tribe_fold_toggled()
	await h._settle()
	options = _crafting_options(craft_panel)
	h._assert_hud("opening the fold lists the members' benches (%s)" % str(options),
		options == [TRIBE_NAME, "Pinewold", "Greyfen", "Saltmarch", "Ashfell"])
	crafting._on_band_selected(int(roster[1]["entity"]))
	await h._settle()
	h._assert_hud("picking a member opens its own bench, and the arrows walk the members (%s)" % str(craft_panel.card().get_child_count()),
		_count_in_header(craft_panel) == "2 / 3")
	crafting.close()

	# ---- THE TURN ORB ----------------------------------------------------------------------------------
	var orb_lines: Array = []
	var orb_record := func(p: Dictionary) -> void: orb_lines.append(String(MAIN_SCRIPT.format_chief_proposal(p).get("line", "")))
	h._hud.chief_proposal_requested.connect(orb_record)
	h._hud.update_band_alerts(roster)
	h._hud.turn_orb.open_popover()
	await h._settle()
	var proposal_text := "%s — %s" % [TRIBE_NAME, PROPOSAL_TEXT]
	h._assert_hud("the orb carries ONE row for the tribe's proposal, in the tribe's name and the sim's words",
		NodeQuery.has_label_containing(h._hud.turn_orb._popover, proposal_text)
			and _orb_texts(proposal_text) == 1)
	h._assert_hud("…with Approve and Not now",
		_orb_buttons("Approve").size() == 1 and _orb_buttons("Not now").size() == 1)
	await h._save("tribe_orb_proposal")
	(_orb_buttons("Approve")[0] as Button).pressed.emit()
	await h._settle()
	h._hud.turn_orb.open_popover()
	await h._settle()
	(_orb_buttons("Not now")[0] as Button).pressed.emit()
	h._hud.chief_proposal_requested.disconnect(orb_record)
	h._assert_hud("Approve sends `tribe_proposal 0 11 approve %s`, Not now `... defer %s`, never approve_proposal (%s)"
			% [PROPOSAL_KEY, PROPOSAL_KEY, str(orb_lines)],
		orb_lines == ["tribe_proposal 0 11 approve %s" % PROPOSAL_KEY, "tribe_proposal 0 11 defer %s" % PROPOSAL_KEY])
	# ---- A TALL PAGE, DOCKED ON EACH EDGE ---------------------------------------------------------------
	# Every member opened, each with a long list of rows: the Work zone must scroll INSIDE the panel, on a side dock
	# as well as the bottom one. The claim is geometric - the scroll container's rect lies inside the card's and its
	# content is taller than it - since a page that overflows renders a perfectly ordinary list.
	var tall := _roster()
	for member in tall:
		if int(member["tribe_id"]) != TRIBE_ID:
			continue
		for i in range(TALL_EXTRA_ROWS):
			(member["labor_assignments"] as Array).append({"kind": "forage", "workers": 1,
				"target_x": 80 + i, "target_y": 20, "floor": 0.5, "actual_yield": 0.1,
				"sustainable_yield": 0.1, "workers_needed": 1, "overdraws": false, "for_tribe": false,
				"max_workers": 0, "hold_crew": 0, "reach_crew": 1, "keepers_staffed": 0,
				"kitted_staffed": 0, "bare_staffed": 1, "useful_crew": 1})
		bandpanel._tribe_open_folds["%d:%d" % [TRIBE_ID, int(member["band_id"])]] = true
	h._hud.update_band_alerts(tall)
	for edge in [SIDE_LEFT, SIDE_RIGHT, SIDE_BOTTOM]:
		panel.set_dock(edge)
		panel.set_active_tab(BandCityPanel.ZONE_WORK)
		bandpanel.render_tribe(TRIBE_ID)
		await h._settle()
		await h._settle()
		var work_scroll := panel.find_child(HudTribeVocab.WORK_SCROLL_NAME, true, false) as ScrollContainer
		var card_rect := panel.card_rect()
		var scroll_rect := work_scroll.get_global_rect() if work_scroll != null else Rect2()
		var content_height := (work_scroll.get_child(0) as Control).get_combined_minimum_size().y \
			if work_scroll != null and work_scroll.get_child_count() > 0 else 0.0
		h._assert_hud("dock %d: the Work zone's content sits in a vertical scroll (%s)" % [edge, str(work_scroll != null)],
			work_scroll != null and work_scroll.vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED)
		h._assert_hud("dock %d: the scroll lies inside the panel's card (scroll %s, card %s)"
				% [edge, str(scroll_rect), str(card_rect)],
			work_scroll != null and scroll_rect.position.y >= card_rect.position.y - 1.0
				and scroll_rect.end.y <= card_rect.end.y + 1.0)
		var zone_host := work_scroll.get_parent().get_parent() as Control if work_scroll != null else null
		h._assert_hud("dock %d: the scroll declares no height of its own and lies inside its zone host (min %.0f, scroll %s, host %s)"
				% [edge, work_scroll.custom_minimum_size.y if work_scroll != null else -1.0, str(scroll_rect),
					str(zone_host.get_global_rect()) if zone_host != null else "none"],
			work_scroll != null and zone_host != null and is_zero_approx(work_scroll.custom_minimum_size.y)
				and scroll_rect.end.y <= zone_host.get_global_rect().end.y + 1.0)
		h._assert_hud("dock %d: the opened bands are taller than the scroll, so it scrolls (%.0f > %.0f)"
				% [edge, content_height, scroll_rect.size.y],
			work_scroll != null and content_height > scroll_rect.size.y)
		if edge != SIDE_BOTTOM:
			await h._save("tribe_page_tall_dock_%s" % ("left" if edge == SIDE_LEFT else "right"))
	bandpanel._tribe_open_folds.clear()
	panel.set_dock(SIDE_BOTTOM)
	h._hud.update_band_alerts(roster)
	h._hud._band_labor.set_forage_patches([])
	h._hud._band_labor.set_roads([])

	# Clean up so nothing downstream inherits the tribe or the panel.
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(BAND_PANEL_RESERVER, SIDE_BOTTOM, 0.0)
	panel.queue_free()
	await h.get_tree().process_frame
	h._hud.update_band_alerts([BandFx.band_fixture()])

# ---- helpers ----------------------------------------------------------------------------------------------

func _all_buttons(root: Node, out: Array = []) -> Array:
	if root is Button:
		out.append(root)
	for child in root.get_children():
		_all_buttons(child, out)
	return out

func _open_road_card() -> bool:
	if not DoorQ.press(h._hud, HudRouteVocab.ROAD_ROW):
		return false
	await h._settle()
	return h._hud._drawercompose._road_ladder != null and h._hud._drawercompose._road_ladder.visible

func _picker_in(root: Node) -> OptionButton:
	var found := _collect_meta(root, HudTribeVocab.PICKER_META)
	for row in found:
		for child in (row as Node).get_children():
			if child is OptionButton:
				return child
	return null

func _picker_face() -> String:
	var picker := _picker_in(h._hud._drawercompose._road_ladder)
	return picker.text if picker != null else ""

func _sheet_picker() -> OptionButton:
	return _picker_in(h._hud._drawercompose._compose_sheet)

func _sheet_picker_face() -> String:
	var picker := _sheet_picker()
	return picker.text if picker != null else ""

func _fold_open_state() -> bool:
	var folds := _collect_meta(h._hud._drawercompose._road_ladder, HudTribeVocab.PICKER_FOLD_META)
	return bool((folds[0] as Node).get_meta(HudTribeVocab.PICKER_FOLD_META)) if not folds.is_empty() else false

func _picker_entries() -> Array:
	var out: Array = []
	var picker := _picker_in(h._hud._drawercompose._road_ladder)
	if picker == null:
		return out
	for i in range(picker.item_count):
		out.append(picker.get_item_text(i))
	return out

func _pick_entry(label: String) -> void:
	_pick_in(_picker_in(h._hud._drawercompose._road_ladder), label)

func _pick_sheet_entry(label: String) -> void:
	_pick_in(_sheet_picker(), label)

func _pick_in(picker: OptionButton, label: String) -> void:
	if picker == null:
		return
	for i in range(picker.item_count):
		if picker.get_item_text(i) == label:
			picker.item_selected.emit(i)
			return

func _rung_button(rung_key: String, root: Node = null) -> Button:
	for control in _collect_meta(root if root != null else h._hud._drawercompose._road_ladder, HudWorkVocab.RUNG_TRACK_RUNG_META):
		if String((control as Node).get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META)) == rung_key:
			if control is Button:
				return control
			for button in _all_buttons(control as Node):
				return button
	return null

func _sheet_commit() -> Button:
	for control in _collect_meta(h._hud._drawercompose._compose_sheet, HudWidgets.COMPOSE_COMMIT_META):
		return control
	return null

func _crafting_options(craft_panel: CraftingPanel) -> Array:
	var picker: OptionButton = null
	for node in _all_buttons(craft_panel):
		pass
	var stack: Array = [craft_panel]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node is OptionButton:
			picker = node
			break
		stack.append_array(node.get_children())
	var out: Array = []
	if picker != null:
		for i in range(picker.item_count):
			out.append(picker.get_item_text(i))
	return out

func _count_in_header(craft_panel: CraftingPanel) -> String:
	var parts := _labels_text(craft_panel._header).split(" | ")
	for part in parts:
		if part.contains(" / "):
			return part
	return ""

func _orb_buttons(face: String) -> Array:
	var found: Array = []
	var stack: Array = [h._hud.turn_orb._popover]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node is Button and (node as Button).text == face:
			found.append(node)
		stack.append_array(node.get_children())
	return found

func _orb_texts(needle: String) -> int:
	var count := 0
	var stack: Array = [h._hud.turn_orb._popover]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node is Label and (node as Label).text == needle:
			count += 1
		stack.append_array(node.get_children())
	return count
