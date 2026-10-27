extends RefCounted

## **THE `Dig stone` ROW** (#874) — `band_panel_preview`'s states for a MINED material's keep-on-hand order,
## in a file of their own so the fixtures do not grow the 25k-line harness. Driven from the harness's run
## order (`h`), rendering into the same long-lived panel, and APPENDED after the workings roster so no
## earlier frame moves.
##
## The five states the row has (`Main.dc.html` → the `stock` job): refilling, at target, no target at all,
## no rock known, and a pile below target with nobody free to dig. Each is a band carrying a
## `keep_on_hand` entry (the shape `native/src/dict/population.rs` writes) over the harness's workings
## ground; the dig crew, where there is one, is the sim-owned `extract` row at the entry's site.

## The harness node — `_hud`, `_panel`, `_settle`, `_save`, `_assert_band_panel`, …
var h

const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

const STONE := "stone"
## The picked site: the workings roster's near hex, one tile from the band's camp.
const SITE := Vector2i(72, 18)
const TARGET := 60
const ON_HAND_BELOW := 42
## What the band's builds and upkeep draw a turn, and the dig crew's take.
const DRAW := 1.0
const DIG_TAKE := 1.2
## The dig crew: one hand holding the draw (with the kit), two refilling bare-handed.
const CREW_KITTED := 1
const CREW_BARE := 2
const CREW_HOLD := 1
## A pile the target has already been met by, and a stray pile under a target of none.
const ON_HAND_FULL := 60
const ON_HAND_NONE_TARGET := 12

const NAME := "Dig stone"
const DETAIL_NO_SITE := "No rock known yet"

## The band: the harness's, over the workings ground, with a stone pile and a keep-on-hand entry.
func _band(target: int, on_hand: int, has_site: bool, crew_kitted: int, crew_bare: int,
		level: String = "normal") -> Dictionary:
	var band: Dictionary = h._band_fixture()
	band["material_store"] = [{"material_id": STONE, "amount": float(on_hand)}]
	band["keep_on_hand"] = [{
		"material": STONE, "target": target, "priority": level, "draw": DRAW,
		"has_site": has_site, "site_x": SITE.x if has_site else 0, "site_y": SITE.y if has_site else 0,
	}]
	var crew := crew_kitted + crew_bare
	if crew > 0:
		(band["labor_assignments"] as Array).append({
			"kind": HudConst.LABOR_KIND_EXTRACT, "workers": crew,
			"target_x": SITE.x, "target_y": SITE.y, "fauna_id": "", "material": STONE,
			"priority": level, "has_yield": true,
			"material_yield": [{"material_id": STONE, "amount": DIG_TAKE}],
			"keepers_staffed": 0, "kitted_staffed": crew_kitted, "bare_staffed": crew_bare,
			"hold_crew": CREW_HOLD, "reach_crew": crew,
		})
		band["working_age"] = int(band["working_age"]) + crew
	return band

func _stage(band: Dictionary) -> void:
	h._hud.update_deposits(h._workings_rows())
	h._push_bands([band])
	h._hud._bandpanel.rerender()
	await h._settle()

## The Dig row drawn on the live panel, or null.
func _row() -> Control:
	return h._find_meta_control(h._panel, HudKeepVocab.DIG_ROW_META)

func _text_of(meta: StringName) -> String:
	var node: Control = h._find_meta_control(h._panel, String(meta))
	return "" if node == null else (node as Label).text

func _line_of(row: Control, meta: StringName) -> String:
	var node: Control = h._find_meta_control(row, String(meta))
	return "" if node == null else (node as Label).text

func run(harness) -> void:
	h = harness
	h._hud.update_deposit_rungs(h._deposit_rung_catalog())
	h._panel.set_active_tab(&"work")

	# ---- REFILLING -------------------------------------------------------------------------------
	await _stage(_band(TARGET, ON_HAND_BELOW, true, CREW_KITTED, CREW_BARE, "high"))
	await h._save("band_panel_dig_stone_refilling")
	var row := _row()
	h._assert_band_panel("a band with a stone entry draws ONE `%s` row" % NAME, row != null)
	if row != null:
		var title := _line_of(row, HudWorkVocab.WORKINGS_ROW_NAME_META)
		h._assert_band_panel("…named `%s` (got `%s`)" % [NAME, title], title == NAME)
		var chip := _line_of(row, HudKeepVocab.KEEP_CHIP_META)
		var want_chip := "Keep %d · %d on hand" % [TARGET, ON_HAND_BELOW]
		h._assert_band_panel("…its keep chip reads `%s` (got `%s`)" % [want_chip, chip], chip == want_chip)
		h._assert_band_panel("…and the chip is a LABEL, not a button",
			h._find_meta_control(row, String(HudKeepVocab.KEEP_CHIP_META)) is Label)
		var status := _line_of(row, HudKeepVocab.KEEP_STATUS_META)
		h._assert_band_panel("…line three says it is refilling (got `%s`)" % status,
			status.contains("Refilling to %d" % TARGET))
		var detail := _line_of(row, HudWorkVocab.SITE_CREW_LINE_META)
		h._assert_band_panel("…its site line says the game picks the site (got `%s`)" % detail,
			detail.contains("(%d, %d) — the game picks the site" % [SITE.x, SITE.y]))
		var priority := h._find_meta_control(row, String(HudWorkVocab.WORK_ROW_PRIORITY_PILL_META)) as Button
		h._assert_band_panel("…and the Priority pill carries the entry's own level",
			priority != null and priority.text.to_lower().contains("high"))
	var old_rows: Array = h._workings_rows_drawn().filter(func(r: Control) -> bool:
		return String(r.get_meta(HudWorkVocab.WORKINGS_ROSTER_ROW_META)).ends_with(":" + STONE))
	h._assert_band_panel("a mined material draws no per-site stone rows beside the Dig row (got %d)"
			% old_rows.size(),
		old_rows.size() == 1)

	# ---- AT TARGET -------------------------------------------------------------------------------
	await _stage(_band(TARGET, ON_HAND_FULL, true, 0, 0))
	await h._save("band_panel_dig_stone_at_target")
	row = _row()
	if row != null:
		h._assert_band_panel("a pile at its target reads `%s` (got `%s`)"
				% [HudKeepVocab.STATUS_AT_TARGET, _line_of(row, HudKeepVocab.KEEP_STATUS_META)],
			_line_of(row, HudKeepVocab.KEEP_STATUS_META) == HudKeepVocab.STATUS_AT_TARGET)

	# ---- NO TARGET -------------------------------------------------------------------------------
	await _stage(_band(0, ON_HAND_NONE_TARGET, true, 0, 0))
	await h._save("band_panel_dig_stone_none")
	row = _row()
	if row != null:
		h._assert_band_panel("a target of none reads `Keep: none` (got `%s`)"
				% _line_of(row, HudKeepVocab.KEEP_CHIP_META),
			_line_of(row, HudKeepVocab.KEEP_CHIP_META) == HudKeepVocab.KEEP_CHIP_NONE)

	# ---- NO ROCK KNOWN ---------------------------------------------------------------------------
	await _stage(_band(TARGET, ON_HAND_BELOW, false, 0, 0))
	await h._save("band_panel_dig_stone_no_site")
	row = _row()
	if row != null:
		h._assert_band_panel("a band with no rock known says so (got `%s`)"
				% _line_of(row, HudWorkVocab.SITE_CREW_LINE_META),
			_line_of(row, HudWorkVocab.SITE_CREW_LINE_META) == DETAIL_NO_SITE)
		# ...and the row is NOT inert: a click opens the site-less Stone sheet.
		var click := InputEventMouseButton.new()
		click.button_index = MOUSE_BUTTON_LEFT
		click.pressed = true
		row.gui_input.emit(click)
		await h._settle()
		h._assert_band_panel("a click on the no-site row opens the Stone sheet",
			h._hud._drawercompose.is_compose_sheet_open())
		h._hud._drawercompose.close_compose_sheet()
		await h._settle()

	# ---- NOBODY FREE -----------------------------------------------------------------------------
	await _stage(_band(TARGET, ON_HAND_BELOW, true, 0, 0))
	await h._save("band_panel_dig_stone_nobody_free")
	row = _row()
	if row != null:
		h._assert_band_panel("a pile below target with no crew reads `%s` (got `%s`)"
				% [HudKeepVocab.STATUS_NOBODY_FREE, _line_of(row, HudKeepVocab.KEEP_STATUS_META)],
			_line_of(row, HudKeepVocab.KEEP_STATUS_META) == HudKeepVocab.STATUS_NOBODY_FREE)

	# ---- THE PRIORITY PILL SENDS `keep_on_hand` WITH THE TARGET --------------------------------------
	await _stage(_band(TARGET, ON_HAND_BELOW, true, CREW_KITTED, CREW_BARE))
	var sent: Array = []
	h._hud.keep_on_hand_requested.connect(func(p: Dictionary) -> void: sent.append(p))
	var pill := h._find_meta_control(_row(), String(HudWorkVocab.WORK_ROW_PRIORITY_PILL_META)) as Button
	if pill != null:
		pill.pressed.emit()
		var line: String = ""
		if not sent.is_empty():
			line = String(MAIN_SCRIPT.format_keep_on_hand(sent[0]).get("line", ""))
		h._assert_band_panel("cycling the pill sends `keep_on_hand` with BOTH trailing fields (got `%s`)" % line,
			line == "keep_on_hand %d %d %s %d high" % [HudConst.PLAYER_FACTION_ID,
				int(h._hud._band_labor.panel_band().get("band_id", -1)), STONE, TARGET])
	await h._put_the_workings_roster_back()
