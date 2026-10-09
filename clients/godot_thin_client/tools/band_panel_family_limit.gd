extends RefCounted

## **THE FAMILY LIMIT** (issue #691) - `band_panel_preview`'s states for the breeding ceiling, in a file of
## their own so the fixtures do not grow the 25k-line harness. Driven from the harness's run order
## (`h`), rendering into the same long-lived panel, and APPENDED LAST so no earlier frame moves.
##
## The frames: the Band tab's vitals with the `Family limit` row in each of its readings - room to grow,
## near the limit with a fading member, at the limit (births stopped, kin), in touch with another people
## (one fading), over the limit, starving (births stopped by FOOD, the row calm), and a people that is
## free for good (the row ABSENT) - each with the popover that opens off it where it has something to
## say; the Growth breakdown at the limit (the fourth row); and the split sheet twice - with the FAMILY
## LIMIT block and the home-at-its-limit line, and on a free people (Families rows, no block).

const SIDE_CANVAS := Vector2i(1500, 900)

## The world constants, as the campaign section echoes them: K people per founding line, and the
## head-count at which a people breeds freely for good.
const PEOPLE_PER_LINE := 100
const FREE_BREEDING_AT := 500

## The subject band (the harness's shared fixture keeps entity 904) and the other band it breeds with.
const SUBJECT_ENTITY := 904
const KIN_BAND_NAME := "Barrowmere"
const KIN_BAND_ENTITY := 911
const SUBJECT_NAME := "Ashfell"
## Two other peoples, by the faction ids the name table below names.
const FOREIGN_FACTION_A := 1
const FOREIGN_FACTION_B := 2
const FOREIGN_NAMES := [
	{"faction": 1, "name": "Marrow Clan"},
	{"faction": 2, "name": "Reedfolk"},
]

## The split sheet's stepper: half of the band's 16 workers, so the share is exactly 50%.
const SPLIT_HALF_WORKERS := 8
## Far past any sheet's height; the container clamps it to its own end.
const SCROLL_TO_END := 100000

var h

func _limited(overrides: Dictionary) -> Dictionary:
	var band: Dictionary = h._band_fixture()
	band["name"] = SUBJECT_NAME
	band["fertility_hunger"] = 1.0
	band["fertility_reserve"] = 1.5
	band["fertility_trend"] = 1.0
	band["fertility_ceiling"] = 1.0
	for key in overrides:
		band[key] = overrides[key]
	return band

func _member(entity: int, lines: int, people: int, fading: bool = false) -> Dictionary:
	return {"band_id": entity + h.FIXTURE_BAND_ID_OFFSET, "lines": lines, "people": people,
		"fading": fading}

func _people(faction: int, lines: int, fading: bool = false) -> Dictionary:
	return {"faction": faction, "lines": lines, "fading": fading}

func _kin_band() -> Dictionary:
	var kin: Dictionary = h._band_fixture()
	kin["entity"] = KIN_BAND_ENTITY
	kin["name"] = KIN_BAND_NAME
	kin["pos"] = [73, 18]
	kin["current_x"] = 73
	kin["current_y"] = 18
	return kin

## Stage one band as the panel's subject on the Band tab and render it.
func _show(band: Dictionary) -> void:
	h._push_bands([band, _kin_band()])
	h._panel.set_dock(SIDE_LEFT)
	h._panel.set_active_tab(BandCityPanel.ZONE_BAND)
	# The band selected on its own hex, as the live game has it - a popover click re-renders the hosts
	# off the SELECTION, which would otherwise still name the band an earlier chapter left lit.
	h._select_panel_band_on_its_hex()
	await h._settle()

func _vitals_text() -> String:
	var label: RichTextLabel = h._first_rich_text(h._panel._zones.get(BandCityPanel.ZONE_BAND))
	return "" if label == null else label.get_parsed_text()

func _popover_text() -> String:
	var label: RichTextLabel = h._hud._disclosures._breakdown_popover_label
	return "" if label == null else label.get_parsed_text()

func _open(kind: String, frame: String) -> void:
	h._click_disclosure(DetailFormat.breakdown_key(kind, {"entity": SUBJECT_ENTITY}))
	await h._settle()
	await h._save(frame)

func _close() -> void:
	h._hud._disclosures._close_popover()

func run(harness) -> void:
	h = harness
	LineageWorld.update({"lineage_people_per_line": PEOPLE_PER_LINE,
		"lineage_free_breeding_at": FREE_BREEDING_AT})
	FactionNames.update(FOREIGN_NAMES)
	await h._pin_canvas(SIDE_CANVAS)
	var start_canvas: Vector2i = h._pinned_canvas
	var limit_kind := HudDisclosureVocab.BREAKDOWN_KIND_FAMILY_LIMIT

	# ---- 1. ROOM TO GROW: the row, calm ink, no line under it ------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 120, "breeding_ceiling": 200,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 120)], "breeding_peoples": []}))
	await h._save("family_limit_room")
	var text := _vitals_text()
	h._assert_band_panel("family limit: the row reads `population / ceiling` (120 / 200)",
		text.contains("Family limit") and text.contains("120 / 200"))
	h._assert_band_panel("family limit: …and says nothing under it while there is room",
		not text.contains(HudLineageVocab.BULLET_MARK) and not text.contains("Stay in touch"))

	# ---- 2. NEAR THE LIMIT, one member fading ----------------------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 190, "breeding_ceiling": 200,
		"fertility_ceiling": 0.4,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 120), _member(KIN_BAND_ENTITY, 2, 70, true)],
		"breeding_peoples": []}))
	text = _vitals_text()
	h._assert_band_panel("family limit: near the limit, the amber line counts the room (got `%s`)"
			% text.replace("\n", " | "),
		text.contains(HudLineageVocab.BULLET_ROOM_FORMAT % 10))
	await _open(limit_kind, "family_limit_near")
	var pop := _popover_text()
	h._assert_band_panel("family limit: the popover lists each member with its families and people",
		pop.contains(SUBJECT_NAME) and pop.contains("2 families · 120")
			and pop.contains(HudLineageVocab.MEMBER_VALUE_FORMAT % [2, 70]))
	h._assert_band_panel("family limit: …tags the fading member",
		pop.contains(HudLineageVocab.FADING_SUFFIX.strip_edges()))
	h._assert_band_panel("family limit: …and closes on the scale note",
		pop.contains(HudLineageVocab.SCALE_NOTE_FORMAT % [PEOPLE_PER_LINE, FREE_BREEDING_AT]))
	_close()

	# ---- 3. AT THE LIMIT: births stopped, kin ----------------------------------------------------
	var at_limit := _limited({"founding_lines": 2, "breeding_population": 200, "breeding_ceiling": 200,
		"fertility_ceiling": 0.0,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 200)], "breeding_peoples": []})
	await _show(at_limit)
	await h._save("family_limit_at")
	text = _vitals_text()
	h._assert_band_panel("family limit: at the limit the Growth row reads `Births stopped`",
		text.contains(DetailFormat.GROWTH_STOPPED_TEXT) and not text.contains("0% of normal"))
	h._assert_band_panel("family limit: …and the line under the limit is the kin sentence",
		text.contains(HudLineageVocab.BULLET_KIN))
	await _open(HudDisclosureVocab.BREAKDOWN_KIND_GROWTH, "family_limit_growth_breakdown")
	h._assert_band_panel("family limit: the Growth breakdown gains the fourth row (`too few families`)",
		_popover_text().contains(DetailFormat.FERTILITY_LABEL_CEILING))
	_close()

	# ---- 4. IN TOUCH WITH ANOTHER PEOPLE, one of them fading --------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 330, "breeding_ceiling": 500,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 140), _member(KIN_BAND_ENTITY, 2, 190)],
		"breeding_peoples": [_people(FOREIGN_FACTION_A, 3), _people(FOREIGN_FACTION_B, 2, true)]}))
	text = _vitals_text()
	h._assert_band_panel("family limit: a people one step from free gets the dim `stay in touch` line",
		text.contains(HudLineageVocab.STAY_IN_TOUCH_FORMAT % FREE_BREEDING_AT)
			and not text.contains(HudLineageVocab.BULLET_MARK))
	await _open(limit_kind, "family_limit_peoples")
	pop = _popover_text()
	h._assert_band_panel("family limit: the popover names the other peoples by their published names",
		pop.contains("Marrow Clan") and pop.contains("+3 families") and pop.contains("Reedfolk")
			and pop.contains("+2 families"))
	_close()

	# ---- 5. LIFTED: no row at all -----------------------------------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 620, "breeding_ceiling": 0,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 620)], "breeding_peoples": []}))
	await h._save("family_limit_lifted")
	h._assert_band_panel("family limit: a people free for good shows no Family limit row",
		not _vitals_text().contains("Family limit"))

	# ---- 6. OVER THE LIMIT ----------------------------------------------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 230, "breeding_ceiling": 200,
		"fertility_ceiling": 0.0,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 230)], "breeding_peoples": []}))
	await h._save("family_limit_over")
	h._assert_band_panel("family limit: past the ceiling the line says to find another people",
		_vitals_text().contains(HudLineageVocab.BULLET_OVER_LIMIT))

	# ---- 7. STARVING: births stopped by FOOD, the limit calm ---------------------------------------------
	await _show(_limited({"founding_lines": 2, "breeding_population": 100, "breeding_ceiling": 300,
		"fertility_hunger": 0.0,
		"breeding_members": [_member(SUBJECT_ENTITY, 2, 100)], "breeding_peoples": []}))
	await h._save("family_limit_starving")
	text = _vitals_text()
	h._assert_band_panel("family limit: a starving band reads `Births stopped` with the limit row calm",
		text.contains(DetailFormat.GROWTH_STOPPED_TEXT) and text.contains("100 / 300")
			and not text.contains(HudLineageVocab.BULLET_MARK))

	# ---- 8. THE SPLIT SHEET, with the FAMILY LIMIT block and the home-at-limit line ------------------------
	var splitting := _limited({"founding_lines": 4, "breeding_population": 430, "breeding_ceiling": 500,
		"fertility_ceiling": 0.5,
		"breeding_members": [_member(SUBJECT_ENTITY, 4, 30), _member(KIN_BAND_ENTITY, 2, 400)],
		"breeding_peoples": []})
	h._push_bands([splitting, _kin_band()])
	h._panel.set_dock(SIDE_LEFT)
	h._panel.set_active_tab(&"parties")
	await h._open_split_sheet(SPLIT_HALF_WORKERS)
	await h._settle()
	await _scroll_sheet_to_end()
	await h._save("family_limit_split")
	var sheet := _sheet_text()
	h._assert_band_panel("family limit: the split sheet states the lines each half holds (`4 → 2`; %s)"
			% sheet.replace("\n", " | "),
		sheet.contains(HudComposeVocab.SPLIT_ROW_FAMILIES) and sheet.contains("4 → 2"))
	h._assert_band_panel("family limit: …the FAMILY LIMIT block prices both halves (`200 · home 400`)",
		sheet.contains(HudComposeVocab.SPLIT_FAMILY_LIMIT_HEADER.to_upper())
			and sheet.contains("500, shared") and sheet.contains("200 · home 400"))
	h._assert_band_panel("family limit: …and the home half is told it would sit at its limit",
		sheet.contains(HudComposeVocab.SPLIT_LIMIT_HOME_AT))
	h._close_split_sheet()

	# ---- 9. THE SPLIT SHEET on a free people: Families rows, no block ----------------------------------------
	var free := _limited({"founding_lines": 4, "breeding_population": 620, "breeding_ceiling": 0,
		"breeding_members": [_member(SUBJECT_ENTITY, 4, 620)], "breeding_peoples": []})
	h._push_bands([free, _kin_band()])
	await h._open_split_sheet(SPLIT_HALF_WORKERS)
	await h._settle()
	await _scroll_sheet_to_end()
	await h._save("family_limit_split_free")
	sheet = _sheet_text()
	h._assert_band_panel("family limit: a free people's split still shows Families, with no block",
		sheet.contains("4 → 2") and not sheet.contains(HudComposeVocab.SPLIT_FAMILY_LIMIT_HEADER.to_upper()))
	h._close_split_sheet()

	# ---- 10. THE ARITHMETIC, PNG-less --------------------------------------------------------------------------
	_assert_split_arithmetic()

	# Put the world back: nothing here may leak into a frame after it.
	LineageWorld.reset()
	FactionNames.reset()
	await h._pin_canvas(start_canvas)

## The drawer the sheet is mounted in is a height-capped scroll, and the FAMILY LIMIT block is the
## last thing on the sheet - so the frame that judges it scrolls to the end first.
func _scroll_sheet_to_end() -> void:
	var node: Node = h._verb_sheet()
	while node != null and not (node is ScrollContainer):
		node = node.get_parent()
	if node == null:
		h._fail("family limit: the split sheet is not inside a scroll container")
		return
	(node as ScrollContainer).scroll_vertical = SCROLL_TO_END
	await h._settle()

func _sheet_text() -> String:
	return _text_of(h._sheet_root())

func _text_of(node: Node) -> String:
	if node == null:
		return ""
	var parts: Array[String] = []
	if node is Label:
		parts.append((node as Label).text)
	elif node is Button:
		parts.append((node as Button).text)
	for child in node.get_children():
		parts.append(_text_of(child))
	return " | ".join(parts)

## The claims a frame cannot carry: the half-up rounding, the clamp, and the one-line band.
func _assert_split_arithmetic() -> void:
	h._assert_band_panel("family limit: 5 lines at 50% take 3 (round half up)",
		HudLineageVocab.split_taken(5, 0.5) == 3)
	h._assert_band_panel("family limit: a split never takes every line of a many-line band",
		HudLineageVocab.split_taken(4, 1.0) == 3 and HudLineageVocab.split_taken(4, 0.0) == 1)
	h._assert_band_panel("family limit: a one-line band copies its line (takes 1, keeps 1)",
		HudLineageVocab.split_taken(1, 0.9) == 1 and HudLineageVocab.split_home_lines(1, 1) == 1)
	var one_line := {"founding_lines": 1,
		"breeding_members": [_member(SUBJECT_ENTITY, 1, 80)], "breeding_peoples": [_people(1, 2)]}
	var limits := HudLineageVocab.split_limits(one_line, 1)
	h._assert_band_panel("family limit: a one-line band's halves are priced at half a line (%s)" % str(limits),
		int(limits["new_limit"]) == PEOPLE_PER_LINE / 2
			and int(limits["home_limit"]) == mini((3 - 1) * PEOPLE_PER_LINE + PEOPLE_PER_LINE / 2,
				FREE_BREEDING_AT))
