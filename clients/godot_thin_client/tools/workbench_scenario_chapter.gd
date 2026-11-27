extends RefCounted
class_name WorkbenchScenarioChapter

## The Scenario page's chapter of `workbench_preview` — kept out of that harness for the reason the
## ui_preview chapters are: one arc, one file. `run(h)` is handed the harness node and uses its shell,
## `_settle`, `_save`, `_fail` and `_assert_equipment_fits`.
##
## **What it takes from the real thing**: the schema is the server's own `scenario_schema()` output
## (`tools/fixtures/scenario_schema.json`, dumped from the shipped build), and the document is a checked-in
## scenario (`tools/fixtures/scenario_doc.json` = `core_sim/scenarios/herds_pens_playtest.json`). The
## blind-walker keys below are INVENTED here, in the fixture, and named by no shipped script.

const SCHEMA_PATH := "res://tools/fixtures/scenario_schema.json"
const DOC_PATH := "res://tools/fixtures/scenario_doc.json"
const PAGE_ID := &"scenario"
const SCRIPT_ROOT := "res://src"

## A key added to a band (the array item schema) and a whole new top-level section, neither of which any
## shipped script names — the blind-walker property.
const INVENTED_BAND_KEY := "windbreak_rating"
const INVENTED_BAND_VALUE := 7
const INVENTED_SECTION := "hearth_ledger"
const INVENTED_SECTION_KEY := "ember_count"
const INVENTED_SECTION_VALUE := 31

const FILE_PATH := "core_sim/scenarios/herds_pens_playtest.json"
const EXPORT_PATH := "scenarios/exported_world.json"
const EXPORT_FIXTURE_PATH := "core_sim/scenarios/exported_world.json"
const OTHER_FILE_PATH := "core_sim/scenarios/kept_roads.json"

## Paths into the fixture document the assertions drive.
const BAND_PATH := "factions[0].bands[0]"
const LABOR_PATH := "factions[0].labor"
const SIZE_PATH := BAND_PATH + ".size"
const WORKERS_PATH := "factions[0].labor[1].assign_labor.workers"
const LARDER_CLASS_PATH := BAND_PATH + ".larder[0].class"

const OPTIONAL_KEY := "floor"

const REFUSAL_SIZE := "the band is larger than the cap"
const REFUSAL_WORKERS := "more workers than the band has"
const REFUSAL_CLASS := "no such keeping class"
const REFUSAL_FILE_LINE := "<file>: scenario could not be built"
const REFUSAL_SERDE_LINE := "line 3, column 9: expected `,` or `}`"

const PICKED_TILE := [5, 6]
const PICKED_LABOR_TILE := [9, 8]

var _h: Node = null
var _page: ScenarioPage = null
var _schema: Dictionary = {}
var _requests: Array = []
var _picks: Array = []
var _next_save_ok := true
var _asserts := 0


func run(h: Node) -> void:
	_h = h
	_schema = _read_json(SCHEMA_PATH)
	_invent_blind_walker_keys()
	h._shell.show_page(PAGE_ID)
	await h._settle()
	_page = _find_page()
	if _page == null:
		h._fail("scenario page not found")
		return

	# (e) DEGRADES WITH NO SERVICES — rendered before anything is installed.
	_assert_degrades_without_services()
	await h._settle()
	await h._save("workbench_scenario_empty")

	_install_fake_services()
	_page.adopt_schema(_schema, [FILE_PATH, OTHER_FILE_PATH])
	var doc_text := _doc_text()
	_assert(_page.adopt_text(FILE_PATH, doc_text), "the fixture document parses")
	_assert_clean_after_load()

	_page.form().set_all_open(false)
	await h._settle()
	await h._save("workbench_scenario_collapsed")

	_page.form().set_all_open(true)
	await h._settle()
	await h._save("workbench_scenario_expanded")
	h._assert_equipment_fits(_page, PAGE_ID)
	_assert_blind_walker()

	h._shell._scroll.scroll_vertical = int(h._shell._scroll.get_v_scroll_bar().max_value / 2.0)
	await h._settle()
	await h._save("workbench_scenario_expanded_mid")
	h._shell._scroll.scroll_vertical = 0

	await _assert_edits()
	await h._settle()
	await h._save("workbench_scenario_edited")

	await _assert_save_and_load(doc_text)
	await _assert_refusals()
	await h._settle()
	await h._save("workbench_scenario_refused")

	_assert_switch_while_dirty()

	await _assert_export_opens_as_a_fixture_target()
	await h._settle()
	await h._save("workbench_scenario_export")

	h._shell.set_rail_collapsed(true)
	await h._settle()
	await h._save("workbench_scenario_rail")
	h._shell.set_rail_collapsed(false)
	print("workbench_preview: scenario chapter — %d assertions" % _asserts)


# ---- fixtures ------------------------------------------------------------------

func _read_json(path: String) -> Dictionary:
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
	if not (parsed is Dictionary):
		_h._fail("fixture %s unreadable" % path)
		return {}
	return parsed


## The checked-in scenario, with the two invented keys carried in it (their values are what (a) reads).
func _doc_text() -> String:
	var doc := _read_json(DOC_PATH)
	(doc["factions"][0]["bands"][0] as Dictionary)[INVENTED_BAND_KEY] = INVENTED_BAND_VALUE
	doc[INVENTED_SECTION] = {INVENTED_SECTION_KEY: INVENTED_SECTION_VALUE}
	return JSON.stringify(doc, "  ", false, true) + "\n"


## Add `INVENTED_BAND_KEY` to the band schema and `INVENTED_SECTION` to the file's — in the FIXTURE schema,
## exactly as a loader-struct change would add them server-side.
func _invent_blind_walker_keys() -> void:
	var band: Dictionary = _schema["properties"]["factions"]["items"]["properties"]["bands"]["items"]
	band["properties"][INVENTED_BAND_KEY] = {"type": "integer", "format": "uint32", "minimum": 0,
		"description": "Invented by the preview fixture."}
	_schema["properties"][INVENTED_SECTION] = {
		"type": "object", "additionalProperties": false,
		"properties": {INVENTED_SECTION_KEY: {"type": "integer", "format": "uint32", "minimum": 0}},
		"required": [INVENTED_SECTION_KEY]}


func _find_page() -> ScenarioPage:
	for node in _h._shell.find_children("*", "", true, false):
		if node is ScenarioPage:
			return node
	return null


func _install_fake_services() -> void:
	var request := func(kind: String, args: Dictionary, on_reply: Callable) -> bool:
		_requests.append({"kind": kind, "args": args})
		match kind:
			WorkbenchVocab.SCENARIO_KIND_WRITE:
				if _next_save_ok:
					on_reply.call({"ok": true, "refusals": [], "error": ""})
				else:
					on_reply.call({"ok": false, "error": "",
						"refusals": [WorkbenchVocab.SCENARIO_REPLY_FILES]})
			WorkbenchVocab.SCENARIO_KIND_LOAD:
				on_reply.call({"ok": true, "refusals": [], "error": ""})
			WorkbenchVocab.SCENARIO_KIND_LIST:
				on_reply.call({"ok": true, "refusals": [], "error": "", "files": [FILE_PATH, OTHER_FILE_PATH],
					"schema": JSON.stringify(_schema)})
		return true
	var pick := func(on_pick: Callable, on_cancel: Callable = Callable()) -> bool:
		_picks.append({"pick": on_pick, "cancel": on_cancel})
		return true
	_h._shell.set_services({
		WorkbenchVocab.SERVICE_SCENARIO_REQUEST: request,
		WorkbenchVocab.SERVICE_PICK_TILE: pick,
		WorkbenchVocab.SERVICE_APPEND_LOG: func(_text: String) -> void: pass,
	})


# ---- helpers -------------------------------------------------------------------

func _assert(condition: bool, what: String) -> void:
	_asserts += 1
	if not condition:
		_h._fail("scenario: %s" % what)


func _labels(text: String) -> Array[Label]:
	var out: Array[Label] = []
	for node in _page.find_children("*", "Label", true, false):
		if (node as Label).text == text and not node.is_queued_for_deletion():
			out.append(node)
	return out


func _buttons(text: String) -> Array[Button]:
	var out: Array[Button] = []
	for node in _page.find_children("*", "Button", true, false):
		if (node as Button).text == text and not node.is_queued_for_deletion():
			out.append(node)
	return out


func _block(path: String) -> Dictionary:
	return _page.form().blocks().get(path, {})


func _block_header(path: String) -> HBoxContainer:
	var indent: Control = _block(path).get("body", null)
	return indent.get_parent().get_child(0) if indent != null else null


func _notes_text(path: String) -> String:
	var box: VBoxContainer = _page.form().notes().get(path, null)
	if box == null:
		return ""
	var out := ""
	for child in box.get_children():
		if child is Label:
			out += (child as Label).text + "\n"
	return out


## The page's current document, deep-copied for before/after comparison.
func _doc() -> Dictionary:
	return (_page.document() as Dictionary)


# ---- (e) degrade ---------------------------------------------------------------

func _assert_degrades_without_services() -> void:
	_assert(_page._services_caption.visible and _page._services_caption.text == WorkbenchVocab.SCENARIO_NO_SERVICES,
		"with no services the page says why in one caption")
	_assert(_page._file_picker.disabled and _page._refresh_button.disabled,
		"with no services the file list and Refresh are disabled")
	_assert(_page._save_button.disabled and _page._load_button.disabled,
		"with no services Save and Load are disabled")
	# A standalone page (its own tree) holds the same line when built with nothing at all.
	var lone := ScenarioPage.new()
	_h.add_child(lone)
	lone.build()
	var footer := lone.build_actions()
	_h.add_child(footer)
	_assert(lone._services_caption.visible, "a page built with no service dictionary at all still builds")
	lone.adopt_schema(_schema, [])
	_assert(lone.adopt_text(FILE_PATH, _doc_text()), "a service-less page still takes a document")
	for pick in lone.find_children("*", "Button", true, false):
		if (pick as Button).text == WorkbenchVocab.FORM_PICK_LABEL:
			_assert((pick as Button).disabled, "with no pick service every pick button is disabled")
			break
	lone.queue_free()
	footer.queue_free()


# ---- a clean load --------------------------------------------------------------

## The document the page just read must NOT read as edited: the dirty check is against the same
## canonical form, so integers parsed as floats and key order cannot make a fresh load dirty.
func _assert_clean_after_load() -> void:
	_assert(not _page._is_dirty(), "a freshly loaded file is clean")
	_assert(_page._save_button.disabled, "Save is dead on a clean file")
	_assert(not _page._load_button.disabled, "Load is live on a clean, saved file")
	_assert(not (_doc()["factions"][0]["bands"][0]["size"] is float), "integer fields are written back as integers")


# ---- (a) the blind walker ------------------------------------------------------

func _assert_blind_walker() -> void:
	var key_rows := _labels(INVENTED_BAND_KEY)
	_assert(key_rows.size() == 1, "the invented band key renders as a row (found %d)" % key_rows.size())
	if key_rows.size() == 1:
		var spins := key_rows[0].get_parent().find_children("*", "SpinBox", true, false)
		_assert(spins.size() == 1 and int((spins[0] as SpinBox).value) == INVENTED_BAND_VALUE,
			"the invented band key renders with its own value")
	var section_rows := _labels(INVENTED_SECTION_KEY)
	_assert(section_rows.size() == 1, "the invented section's key renders (found %d)" % section_rows.size())
	if section_rows.size() == 1:
		var spins := section_rows[0].get_parent().find_children("*", "SpinBox", true, false)
		_assert(spins.size() == 1 and int((spins[0] as SpinBox).value) == INVENTED_SECTION_VALUE,
			"the invented section's key renders with its own value")
	_assert(_block(INVENTED_SECTION).size() > 0, "the invented section is a block of its own")
	# …and no shipped script names any of the three, so the rows above came from the schema alone.
	for name in [INVENTED_BAND_KEY, INVENTED_SECTION, INVENTED_SECTION_KEY]:
		var hits := _scripts_naming(String(name))
		_assert(hits.is_empty(), "no shipped script names '%s' (found in %s)" % [name, hits])


func _scripts_naming(needle: String) -> Array:
	var hits: Array = []
	var stack: Array = [SCRIPT_ROOT]
	while not stack.is_empty():
		var dir_path: String = stack.pop_back()
		var dir := DirAccess.open(dir_path)
		if dir == null:
			continue
		for sub in dir.get_directories():
			stack.append(dir_path.path_join(sub))
		for file in dir.get_files():
			if file.ends_with(".gd") and FileAccess.get_file_as_string(dir_path.path_join(file)).contains(needle):
				hits.append(dir_path.path_join(file))
	return hits


# ---- (d) edits -----------------------------------------------------------------

func _assert_edits() -> void:
	var doc := _doc()
	var labor: Array = doc["factions"][0]["labor"]
	var before := labor.size()
	var second_before: Variant = labor[1].duplicate(true)

	# + add: the picker lists the variant keys and the button appends that variant's skeleton.
	var labor_body: Control = _block(LABOR_PATH)["body"].get_child(0)
	var add_row: Control = labor_body.get_child(labor_body.get_child_count() - 1)
	var picker: OptionButton = add_row.find_children("*", "OptionButton", false, false)[0]
	var add: Button = null
	for node in add_row.find_children("*", "Button", false, false):
		if not (node is OptionButton):
			add = node
	var names: Array = []
	for i in picker.item_count:
		names.append(picker.get_item_text(i))
	_assert(names == ScenarioSchema.variant_keys(_schema["properties"]["factions"]["items"]["properties"]["labor"]["items"]),
		"the picker offers the schema's variant keys, in order (%s)" % [names])
	picker.select(names.find("work_priority"))
	add.pressed.emit()
	await _h._settle()
	doc = _doc()
	labor = doc["factions"][0]["labor"]
	_assert(labor.size() == before + 1, "+ add appends one labor item (%d -> %d)" % [before, labor.size()])
	_assert(labor.back() == {"work_priority": {"band": "", "level": ""}},
		"+ add appends the picked variant's required-keys skeleton (got %s)" % [labor.back()])

	# × removes that item and only that item.
	var remove_header := _block_header("factions[0].labor[0]")
	var removed := false
	for node in remove_header.find_children("*", "Button", false, false):
		if (node as Button).text == WorkbenchVocab.FORM_REMOVE_GLYPH:
			(node as Button).pressed.emit()
			removed = true
	await _h._settle()
	labor = _doc()["factions"][0]["labor"]
	_assert(removed and labor.size() == before and labor[0] == second_before,
		"× removes exactly the item it sits on")

	# A string field writes through.
	var id_edits: Array[LineEdit] = []
	for node in _block(BAND_PATH)["body"].find_children("*", "LineEdit", true, false):
		if (node as LineEdit).text == "home":
			id_edits.append(node)
	_assert(id_edits.size() >= 1, "the band id has a text field")
	if id_edits.size() >= 1:
		id_edits[0].text = "base"
		id_edits[0].text_changed.emit("base")
		id_edits[0].focus_exited.emit()
	await _h._settle()
	_assert(_doc()["factions"][0]["bands"][0]["id"] == "base", "a text edit writes the document")
	# A reference to an id that is no longer declared is KEPT and shown as it is — never replaced.
	var kept := false
	for node in _page.find_children("*", "OptionButton", true, false):
		var option: OptionButton = node
		if option.is_queued_for_deletion() or option.item_count == 0:
			continue
		if option.get_item_text(option.selected) == "home" + WorkbenchVocab.FORM_UNDECLARED_SUFFIX:
			kept = true
	_assert(kept, "a band reference to a no-longer-declared id shows as 'home (not declared)'")
	var labor_band: Variant = _doc()["factions"][0]["labor"][0]
	_assert(str(labor_band).contains("\"home\""), "…and the document still holds 'home' (got %s)" % [labor_band])

	# An optional key: + key inserts the skeleton, x removes the key again.
	var adders := _buttons(WorkbenchVocab.FORM_ADD_FORMAT % OPTIONAL_KEY)
	_assert(adders.size() >= 1, "an absent optional key offers '+ %s'" % OPTIONAL_KEY)
	if adders.size() >= 1:
		adders[0].pressed.emit()
		await _h._settle()
		var assign: Dictionary = _doc()["factions"][0]["labor"][0]["assign_labor"]
		_assert(assign.has(OPTIONAL_KEY) and assign[OPTIONAL_KEY] == 0.0,
			"+ key inserts the schema's skeleton for it (got %s)" % [assign.get(OPTIONAL_KEY, "<absent>")])
		var row_labels := _labels(OPTIONAL_KEY)
		var cleared := false
		for label in row_labels:
			for node in label.get_parent().find_children("*", "Button", false, false):
				if (node as Button).text == WorkbenchVocab.FORM_REMOVE_GLYPH:
					(node as Button).pressed.emit()
					cleared = true
					break
			if cleared:
				break
		await _h._settle()
		assign = _doc()["factions"][0]["labor"][0]["assign_labor"]
		_assert(cleared and not assign.has(OPTIONAL_KEY), "x on an optional key removes the key from the document")

	# Picks: a tile array and a key-pair object.
	_picks.clear()
	var at_row := _labels("at")
	var tile_pick: Button = null
	for label in at_row:
		if label.get_parent().find_children("*", "SpinBox", true, false).size() == 2 \
				and _block(BAND_PATH)["body"].is_ancestor_of(label):
			for node in label.get_parent().find_children("*", "Button", true, false):
				if (node as Button).text == WorkbenchVocab.FORM_PICK_LABEL:
					tile_pick = node
	_assert(tile_pick != null, "the band's `at` row carries a pick button")
	if tile_pick != null:
		tile_pick.pressed.emit()
		_assert(_picks.size() == 1, "pressing pick arms the pick_tile service")
		_assert(_page._status.text == WorkbenchVocab.SCENARIO_PICK_ARMED, "an armed pick says what to click")
		(_picks[0]["pick"] as Callable).call(PICKED_TILE[0], PICKED_TILE[1])
		await _h._settle()
		_assert(_doc()["factions"][0]["bands"][0]["at"] == PICKED_TILE, "a picked hex writes [x, y]")
		_assert(_page._status.text != WorkbenchVocab.SCENARIO_PICK_ARMED, "the armed line clears when the pick lands")

	_picks.clear()
	var tile_keys_header := _block_header("factions[0].labor[0]")
	var key_pick: Button = null
	for node in tile_keys_header.find_children("*", "Button", false, false):
		if (node as Button).text == WorkbenchVocab.FORM_PICK_LABEL:
			key_pick = node
	_assert(key_pick != null, "an x-pick-tile block carries a pick button on its header")
	if key_pick != null:
		key_pick.pressed.emit()
		(_picks[0]["pick"] as Callable).call(PICKED_LABOR_TILE[0], PICKED_LABOR_TILE[1])
		await _h._settle()
		var inner: Dictionary = _doc()["factions"][0]["labor"][0]
		inner = inner[inner.keys()[0]]
		_assert(inner.get("target_x") == PICKED_LABOR_TILE[0] and inner.get("target_y") == PICKED_LABOR_TILE[1],
			"a picked hex writes both key-pair fields (got %s)" % [inner])
	# A second press of pick cancels (the service answers through on_cancel).
	_picks.clear()
	_page._arm_pick(func(_x: int, _y: int) -> void: pass)
	(_picks[0]["cancel"] as Callable).call()
	_assert(not _page._pick_armed, "a cancelled pick disarms the page")

	_assert(_page._is_dirty(), "after edits the page is dirty")
	_assert(_page._load_button.disabled, "Load is dead while dirty")
	_assert(_page._dot.get_theme_color("font_color").a > 0.0, "the modified dot is lit while dirty")


# ---- save / load ---------------------------------------------------------------

func _assert_save_and_load(original_text: String) -> void:
	_requests.clear()
	_page._on_save_pressed()
	var writes := _requests.filter(func(r: Dictionary) -> bool: return r["kind"] == WorkbenchVocab.SCENARIO_KIND_WRITE)
	_assert(writes.size() == 1 and writes[0]["args"]["path"] == FILE_PATH, "Save sends write_scenario for the open path")
	var text: String = writes[0]["args"]["text"] if writes.size() == 1 else ""
	_assert(text.ends_with("\n") and not text.ends_with("\n\n"), "the saved text ends in exactly one newline")
	_assert(not text.contains(": 150.0"), "integers are written without a fraction")
	var reparsed: Variant = JSON.parse_string(text)
	_assert(reparsed is Dictionary and (reparsed as Dictionary).keys() == _doc().keys(), "the saved text keeps key order")
	_assert(not _page._is_dirty(), "a saved file is clean")
	_assert(not _page._load_button.disabled, "Load is live once saved")
	var loads_before := _requests.size()
	_page._on_load_pressed()
	_assert(_requests.size() == loads_before + 1 and _requests.back()["kind"] == WorkbenchVocab.SCENARIO_KIND_LOAD,
		"Load sends load_scenario")
	# A refused save keeps the page dirty and shows the refusals.
	_page._doc["name"] = "renamed"
	_page._refresh_state()
	_next_save_ok = false
	_page._on_save_pressed()
	_next_save_ok = true
	_assert(_page._is_dirty(), "a refused save leaves the page dirty")
	_page._doc["name"] = JSON.parse_string(original_text)["name"]
	_page._refresh_state()
	await _h._settle()


# ---- (c) refusals --------------------------------------------------------------

func _assert_refusals() -> void:
	# Re-seat the clean document so the paths below mean what they say.
	_page._refusals = []
	_page.adopt_text(FILE_PATH, _doc_text())
	_page.form().set_all_open(false)
	_page._on_save_reply({"ok": false, "error": "", "refusals": [
		"%s: %s" % [SIZE_PATH, REFUSAL_SIZE],
		"%s: %s" % [WORKERS_PATH, REFUSAL_WORKERS],
		"%s: %s" % [LARDER_CLASS_PATH, REFUSAL_CLASS],
		REFUSAL_FILE_LINE,
		REFUSAL_SERDE_LINE,
	]}, _doc_text(), FILE_PATH)
	await _h._settle()
	_assert(_notes_text(SIZE_PATH).contains(REFUSAL_SIZE), "a refusal attaches to the node its path names")
	_assert(_notes_text(WORKERS_PATH).contains(REFUSAL_WORKERS), "a deep refusal attaches to its own leaf")
	_assert(_notes_text(LARDER_CLASS_PATH).contains(REFUSAL_CLASS), "an array-indexed refusal attaches to its leaf")
	_assert(not _notes_text(SIZE_PATH).contains(REFUSAL_WORKERS), "a refusal attaches to ONE node")
	var open_ok := true
	for ancestor in ["factions", "factions[0]", BAND_PATH, "factions[0].labor", "factions[0].labor[1]"]:
		var button: Button = _block(ancestor).get("button", null)
		open_ok = open_ok and button != null and button.text.begins_with(WorkbenchVocab.FORM_OPEN_GLYPH)
	_assert(open_ok, "the ancestors of an attached refusal are expanded")
	_assert(not _block("deposits").is_empty() and not (_block("deposits")["body"] as Control).visible,
		"blocks with no refusal below them stay collapsed")
	var banner_text := ""
	for node in _page._banner_host.find_children("*", "Label", true, false):
		banner_text += (node as Label).text + "\n"
	_assert(banner_text.contains(REFUSAL_FILE_LINE) and banner_text.contains(REFUSAL_SERDE_LINE),
		"lines naming no node go to the banner")
	_assert(not banner_text.contains(REFUSAL_SIZE), "a refusal that attached is not repeated in the banner")
	# A path whose deepest node is not drawn lands on the deepest one that is.
	_page._on_save_reply({"ok": false, "error": "", "refusals": [BAND_PATH + ".no_such_key.deeper: gone"]},
		_doc_text(), FILE_PATH)
	await _h._settle()
	_assert(_notes_text(BAND_PATH).contains("gone"), "a refusal on an undrawn node attaches to the deepest drawn ancestor")
	_page._on_save_reply({"ok": false, "error": "", "refusals": [REFUSAL_FILE_LINE, "%s: %s" % [SIZE_PATH, REFUSAL_SIZE]]},
		_doc_text(), FILE_PATH)
	await _h._settle()
	# Refusals stay until the NEXT reply, and the next ok reply clears them.
	_page._on_form_edited()
	_assert(not _notes_text(SIZE_PATH).is_empty(), "refusals survive an edit")
	_page._on_save_reply({"ok": true, "error": "", "refusals": []}, _doc_text(), FILE_PATH)
	await _h._settle()
	_assert(_notes_text(SIZE_PATH).is_empty() and _page._banner_host.get_child_count() == 0,
		"the next accepted save clears every refusal")
	# …and leave a refusal showing for the PNG.
	_page._on_save_reply({"ok": false, "error": "", "refusals": [
		"%s: %s" % [SIZE_PATH, REFUSAL_SIZE],
		"%s: %s" % [WORKERS_PATH, REFUSAL_WORKERS],
		REFUSAL_FILE_LINE,
	]}, _doc_text(), FILE_PATH)


# ---- switching while dirty -----------------------------------------------------

func _assert_switch_while_dirty() -> void:
	_page._refusals = []
	_page.adopt_text(FILE_PATH, _doc_text())
	_page._doc["name"] = "dirty"
	_page._refresh_state()
	_requests.clear()
	# choosing another file while dirty asks for a second press instead of reading it
	_page._file_picker.select(2)
	_page._on_file_chosen(2)
	_assert(_requests.is_empty() and _page._confirm_button.visible,
		"choosing a file while dirty asks to discard first")
	_page._on_new_pressed()
	_assert(_page._new_button.text == WorkbenchVocab.SCENARIO_DISCARD_LABEL, "New while dirty changes its label to the discard prompt")
	_page._name_edit.text = "fresh_scenario"
	_page._on_new_pressed()
	_assert(_page.current_path() == WorkbenchVocab.SCENARIO_DIR_PREFIX + "fresh_scenario" + WorkbenchVocab.SCENARIO_FILE_SUFFIX,
		"the second press creates the new file")
	_assert(_page._is_new and _page._is_dirty() and _page._load_button.disabled,
		"a new file is unsaved: dirty, and Load is dead until it is saved")
	var expected: Variant = ScenarioSchema.skeleton(_schema)
	_assert(_page.document() == expected, "New builds the schema's skeleton document")



# ---- an export opens as a fixture target ---------------------------------------

func _assert_export_opens_as_a_fixture_target() -> void:
	_page._refusals = []
	_page.adopt_schema(_schema, [FILE_PATH, OTHER_FILE_PATH, EXPORT_PATH])
	var export_item := -1
	for i in _page._file_picker.item_count:
		if _page._file_picker.get_item_metadata(i) == EXPORT_PATH:
			export_item = i
	_assert(export_item > 0 and _page._file_picker.get_item_text(export_item) == WorkbenchVocab.SCENARIO_EXPORT_ITEM_FORMAT % "exported_world.json",
		"an exported file is listed with its own label and keeps the real path as metadata")
	_assert(_page.adopt_text(EXPORT_PATH, _doc_text()), "an export's text parses")
	_assert(_page.current_path() == EXPORT_FIXTURE_PATH, "opening an export targets the fixture path (got %s)" % _page.current_path())
	_assert(_page._is_dirty(), "an opened export is dirty — nothing is saved at the fixture path yet")
	_assert(_page._load_button.disabled, "Load stays disabled until the export is saved as a fixture")
	_assert(not _page._save_button.disabled, "Save is live on an opened export")
	_assert(_page._status.text.contains(EXPORT_PATH.get_file()) and _page._status.text.contains(EXPORT_FIXTURE_PATH),
		"the footer says where it was opened from and where it saves")
	_assert(_page._status.get_line_count() <= 2, "the footer line is at most two lines")
	_requests.clear()
	_page._on_save_pressed()
	var writes := _requests.filter(func(r: Dictionary) -> bool: return r["kind"] == WorkbenchVocab.SCENARIO_KIND_WRITE)
	_assert(writes.size() == 1 and writes[0]["args"]["path"] == EXPORT_FIXTURE_PATH, "Save sends write_scenario with the fixture path")
	_assert(not _page._is_dirty() and not _page._load_button.disabled, "after Save the file is clean and Load is live")
