extends RefCounted

## TILE-SOURCED BRANCHES -- the client path to start the monument (`belief`) and the storage pit
## (`food_stores`), and every branch the catalog adds with `source_kind == "tile"`
## (`.claude/rules/client/extraction-workings.md` -> "THE TILE-SOURCED BRANCHES").
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it; it
## is appended LAST so no earlier frame moves.
##
## ⛔ **NOTHING IN THIS CHAPTER IS CLAIMED OF ONE BRANCH ALONE.** The catalog below transcribes both shipped
## branches and every claim is made twice, once per branch, because the property under test is that the
## client reads names, verbs, costs and gates off the catalog -- a client hard-coding `belief` passes the
## monument's claims and fails the pit's.
##
## ⛔ **THE COMMAND LINES ARE ASSERTED AS THE STRINGS THE SERVER PARSES** (`assign_labor <f> <band> tile
## <branch> <x> <y> <n>` and `build <verb> <f> <x> <y>`), composed through the real `Main` builders from
## the payloads the real controls emit -- never through a copy of the grammar.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

## The checkpoints this chapter owes the walk (a FLOOR; see `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS`).
const EXPECTED_CHECKPOINTS := 35

var h

const TILE_X := 33
const TILE_Y := 9
const BELIEF_BRANCH := "belief"
const STORES_BRANCH := "food_stores"
const RITUAL := "ritual"
const STORAGE := "storage"
const RITUAL_LABEL := "Ritual"
const STORAGE_LABEL := "Storage"

## The monument's site requirement and the belief that meets / misses it. `min_belief` is the catalog's
## 1.0; the tile that meets it holds three dead-equivalents, the other none at all.
const MIN_BELIEF := 1.0
const BELIEF_PRESENT := 3.0
const BELIEF_NONE := 0.0

## The crew the step card is asked at: nobody, and two hands.
const CREW_NONE := 0
const CREW_TWO := 2
const STONE_PILE := 20.0

func _catalog() -> Array:
	return [
		_rung("belief:gathering_ground", BELIEF_BRANCH, 1, "Gathering ground", "a gathering ground",
			"", "", "", 0.0, 0.0, "", 0.0),
		_rung("belief:monument", BELIEF_BRANCH, 2, "Monument", "a monument", "raise", RITUAL,
			"belief:gathering_ground", 120.0, STONE_PILE, "stone", MIN_BELIEF),
		_rung("food_stores:unstored", STORES_BRANCH, 1, "No store", "no store",
			"", "", "", 0.0, 0.0, "", 0.0),
		_rung("food_stores:storage_pit", STORES_BRANCH, 2, "Storage pit", "a storage pit", "dig", STORAGE,
			"food_stores:unstored", 40.0, 0.0, "", 0.0),
	]

func _rung(key: String, branch: String, order: int, display: String, fallback: String, verb: String,
		unlock: String, requires: String, work: float, pile: float, pile_id: String,
		min_belief: float) -> Dictionary:
	return {
		"rung_key": key, "branch": branch, "order": order, "display_name": display,
		"label": display, "label_fallback": fallback, "verb": verb, "source_kind": "tile",
		"unlock_knowledge": unlock, "requires_rung": requires, "earns_knowledge": "",
		"work_cost": work, "build_material_cost": pile, "build_material_id": pile_id,
		"max_builders": 3, "upkeep_work_per_turn": 0.5 if verb != "" else 0.0,
		"min_belief": min_belief,
	}

func _knowledge_roster() -> Array:
	return [
		{"knowledge_id": RITUAL, "display_name": RITUAL_LABEL, "branch": BELIEF_BRANCH,
			"order": 1, "is_step": true},
		{"knowledge_id": STORAGE, "display_name": STORAGE_LABEL, "branch": STORES_BRANCH,
			"order": 1, "is_step": true},
	]

func _band(crew_on_belief: int, crew_on_pit: int) -> Dictionary:
	var band := BandFx.band_fixture()
	band["current_x"] = TILE_X
	band["current_y"] = TILE_Y
	var assignments: Array = (band.get("labor_assignments", []) as Array).duplicate()
	if crew_on_belief > 0:
		assignments.append(_tile_assignment(BELIEF_BRANCH, crew_on_belief))
	if crew_on_pit > 0:
		assignments.append(_tile_assignment(STORES_BRANCH, crew_on_pit))
	band["labor_assignments"] = assignments
	return band

func _tile_assignment(branch: String, workers: int) -> Dictionary:
	return {"kind": "tile", "branch": branch, "target_x": TILE_X, "target_y": TILE_Y,
		"workers": workers, "fauna_id": "", "floor": 0.5, "improvement": "", "kit_id": ""}

func _tile(belief: float) -> Dictionary:
	var tile := {
		"x": TILE_X, "y": TILE_Y, "terrain_label": "Alluvial Plain", "tags_text": "none",
		"visibility_state": "active", "habitability": 0.5, "temperature": 12.0,
	}
	if belief > 0.0:
		tile["belief"] = belief
	return tile

func _improvement_row(branch: String, rung: String) -> Dictionary:
	return {"tile_x": TILE_X, "tile_y": TILE_Y, "branch": branch, "current_rung": rung,
		"is_queued": false, "build_turns_remaining": -1, "builders_staffed": 0, "builders_cap": 0}

func _knowledge(ritual: float, storage: float) -> void:
	h._hud.update_intensification([{"faction": 0, "knowledges": {RITUAL: ritual, STORAGE: storage}}])

func _stage(band: Dictionary, rows: Array, ritual: float, storage: float) -> void:
	h._hud.update_band_alerts([band])
	h._hud.update_tile_improvements(rows)
	_knowledge(ritual, storage)

func _collect_meta(root: Node, meta: String, out: Array[Control] = []) -> Array[Control]:
	if root == null:
		return out
	if root is Control and (root as Control).has_meta(meta):
		out.append(root as Control)
	for child in root.get_children():
		_collect_meta(child, meta, out)
	return out

func _step_button(branch: String) -> Button:
	for control in _collect_meta(h._hud.tile_branch_controls, HudTileVocab.STEP_META):
		if String(control.get_meta(HudTileVocab.STEP_META)).ends_with(":" + branch):
			return control as Button
	return null

func _crew_button(branch: String) -> Button:
	for control in _collect_meta(h._hud.tile_branch_controls, HudTileVocab.CREW_META):
		if String(control.get_meta(HudTileVocab.CREW_META)).ends_with(":" + branch):
			return control as Button
	return null

func _reason_text(branch: String) -> String:
	for control in _collect_meta(h._hud.tile_branch_controls, HudTileVocab.REASON_META):
		if String(control.get_meta(HudTileVocab.REASON_META)).ends_with(":" + branch):
			return (control as Label).text
	return ""

func _row_count() -> int:
	return _collect_meta(h._hud.tile_branch_controls, HudTileVocab.ROW_META).size()

func _sheet() -> Node:
	return h._hud._drawercompose._compose_sheet

func _close_sheet() -> void:
	h._hud._drawercompose.close_compose_sheet()

## Open the crew sheet from the tile card's own button, set the stepper to `plus_presses` more than the
## seeded crew and commit; returns the command line the real builder makes of the emitted payload.
func _commit_crew(branch: String, plus_presses: int) -> String:
	var button := _crew_button(branch)
	if button == null:
		return "<no crew button>"
	var sent: Array = []
	var sink := func(payload: Dictionary) -> void: sent.append(payload)
	h._hud.assign_labor_requested.connect(sink)
	button.pressed.emit()
	await h._settle()
	for _i in range(plus_presses):
		var row := Q.find_meta_node(_sheet(), HudTileVocab.CREW_ROW_META)
		if row == null:
			break
		((row as Node).get_child((row as Node).get_child_count() - 1) as Button).pressed.emit()
		await h._settle()
	var commit := Q.find_meta_node(_sheet(), HudWidgets.COMPOSE_COMMIT_META) as Button
	if commit != null:
		commit.pressed.emit()
	await h._settle()
	h._hud.assign_labor_requested.disconnect(sink)
	return String((MAIN_SCRIPT.format_assign_labor(sent[0]) as Dictionary).get("line", "")) \
		if not sent.is_empty() else "<nothing emitted>"

## Open the step card on one branch and press its first ORDERED rung; returns the `build` line, or the
## reason none was sent.
func _press_step(branch: String) -> String:
	var sent: Array = []
	var sink := func(payload: Dictionary) -> void: sent.append(payload)
	h._hud.improvement_requested.connect(sink)
	_step_button(branch).pressed.emit()
	await h._settle()
	var pressed := false
	for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META):
		# Only THIS branch's rungs: an earlier state's road card may still be standing in the tree.
		if not String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META)).begins_with(branch + ":"):
			continue
		for child in control.get_children():
			if child is Button and not (child as Button).disabled and not pressed:
				(child as Button).pressed.emit()
				pressed = true
	await h._settle()
	h._hud.improvement_requested.disconnect(sink)
	if sent.is_empty():
		return "<nothing emitted>"
	return String((MAIN_SCRIPT.format_improvement(sent[0], h._hud.ladder_rungs()) as Dictionary).get("line", ""))

func _site(branch: String, row: Dictionary = {}) -> Dictionary:
	var ladder := HudTileVocab.tile_ladder(_catalog())
	return HudTileVocab.site_of(row, ladder, branch, TILE_X, TILE_Y)

func _track(branch: String, knowledge: Dictionary, crew: int, belief: float,
		store: Array = []) -> Array[Dictionary]:
	var ladder := HudTileVocab.tile_ladder(_catalog())
	var labels := {RITUAL: RITUAL_LABEL, STORAGE: STORAGE_LABEL}
	var band := {DetailFormat.BAND_MATERIAL_STORE_KEY: store}
	return RungLadder.tile_track(_site(branch), ladder, knowledge, labels, crew, _tile(belief),
		SourceForecast.BUILD_CREW_NONE, band)

func _target_row(rows: Array[Dictionary]) -> Dictionary:
	for row in rows:
		if String(row.get(RungLadder.ROW_STATE_KEY, "")) not in [RungLadder.STATE_BANKED, RungLadder.STATE_STANDING]:
			return row
	return {}

func run(harness) -> void:
	h = harness
	h._hud.update_ladder_rungs(_catalog())
	h._hud.update_ladder_branches([
		{"branch": BELIEF_BRANCH, "area": "lore", "source_kind": "tile", "label": "Belief"},
		{"branch": STORES_BRANCH, "area": "food", "source_kind": "tile", "label": "Food stores"},
	])
	h._hud.update_ladder_knowledge(_knowledge_roster())
	var band := _band(CREW_NONE, CREW_NONE)

	# ---- THE TILE CARD ROW ------------------------------------------------------------------------
	# Before either lesson is known, and with no improvement standing, the card carries no tile-branch row.
	_stage(band, [], 0.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("before the lessons, the tile card offers no tile-branch row at all",
		_row_count() == 0 and not h._hud.tile_branch_controls.visible)
	await h._save("tile_branches_hidden")

	# Lessons known: BOTH branches offer a row, worded off the catalog (verb + the rung's own noun).
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("with both lessons known the card carries one row per tile branch", _row_count() == 2)
	h._assert_hud("…the monument row reads the catalog's verb and noun",
		_step_button(BELIEF_BRANCH) != null
			and _step_button(BELIEF_BRANCH).text == "Raise a monument ▸")
	h._assert_hud("…the pit row reads the catalog's verb and noun",
		_step_button(STORES_BRANCH) != null
			and _step_button(STORES_BRANCH).text == "Dig a storage pit ▸")
	h._assert_hud("…on ground that meets the site term neither row is greyed or explained",
		not _step_button(BELIEF_BRANCH).disabled and _reason_text(BELIEF_BRANCH) == ""
			and _crew_button(BELIEF_BRANCH) != null and _crew_button(STORES_BRANCH) != null)
	await h._save("tile_branches_rows")

	# One lesson only: just that branch's row shows (the other is neither known nor held here).
	_stage(band, [], 1.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("with only Ritual known, only the monument row shows",
		_row_count() == 1 and _step_button(BELIEF_BRANCH) != null and _step_button(STORES_BRANCH) == null)
	# …and an improvement already standing shows its row with no lesson at all (the held-here arm).
	_stage(band, [_improvement_row(STORES_BRANCH, "food_stores:storage_pit")], 1.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("a tile already holding a pit shows the pit row without the lesson",
		_step_button(STORES_BRANCH) != null)

	# The site term fails: the monument row stays, greyed, with the reason in plain words.
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_NONE))
	await h._settle()
	h._assert_hud("a tile with no dead keeps the monument row, greyed",
		_step_button(BELIEF_BRANCH) != null and _step_button(BELIEF_BRANCH).disabled)
	h._assert_hud("…and says why in plain words (%s)" % _reason_text(BELIEF_BRANCH),
		_reason_text(BELIEF_BRANCH) == HudTileVocab.SITE_NO_BELIEF_SHORT)
	h._assert_hud("…while the pit row on the same tile is untouched by the belief term",
		not _step_button(STORES_BRANCH).disabled and _reason_text(STORES_BRANCH) == "")
	h._assert_hud("…and a greyed row offers no crew button (the site term no crew closes)",
		_crew_button(BELIEF_BRANCH) == null)
	await h._save("tile_branches_site_greyed")

	# ---- THE CREW SHEET ---------------------------------------------------------------------------
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	_crew_button(BELIEF_BRANCH).pressed.emit()
	await h._settle()
	h._assert_hud("the crew button opens a compose sheet", h._hud.is_compose_sheet_open())
	await h._save("tile_branches_crew_sheet")
	_close_sheet()
	await h._settle()
	var line := await _commit_crew(BELIEF_BRANCH, 1)
	h._assert_hud("the monument crew sheet emits the exact crew line (%s)" % line,
		line == "assign_labor %d %d tile %s %d %d %d" % [
			HudConst.PLAYER_FACTION_ID, band_id, BELIEF_BRANCH, TILE_X, TILE_Y, 2])
	var pit_line := await _commit_crew(STORES_BRANCH, 0)
	h._assert_hud("the pit crew sheet emits the same grammar on its own branch (%s)" % pit_line,
		pit_line == "assign_labor %d %d tile %s %d %d %d" % [
			HudConst.PLAYER_FACTION_ID, band_id, STORES_BRANCH, TILE_X, TILE_Y, 1])

	# ---- THE STEP CARD'S GATES (the producer, one claim per gate per branch) -----------------------
	var learned := {RITUAL: 1.0, STORAGE: 1.0}
	var unlearned := {RITUAL: 0.2, STORAGE: 0.2}
	for branch in [BELIEF_BRANCH, STORES_BRANCH]:
		var lesson_label := RITUAL_LABEL if branch == BELIEF_BRANCH else STORAGE_LABEL
		var lesson_row := _target_row(_track(branch, unlearned, CREW_TWO, BELIEF_PRESENT))
		h._assert_hud("%s: a missing lesson locks the rung and names the lesson (%s)" % [
				branch, lesson_row.get(RungLadder.ROW_FACE_KEY, "")],
			String(lesson_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_LOCKED
				and String(lesson_row.get(RungLadder.ROW_FACE_KEY, "")).contains(lesson_label))
		var crew_row := _target_row(_track(branch, learned, CREW_NONE, BELIEF_PRESENT))
		h._assert_hud("%s: no crew locks the rung with the crew remedy (%s)" % [
				branch, crew_row.get(RungLadder.ROW_FACE_KEY, "")],
			String(crew_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_LOCKED
				and String(crew_row.get(RungLadder.ROW_TOOLTIP_KEY, "")).contains(
					"Put a crew on this tile first"))
		var open_row := _target_row(_track(branch, learned, CREW_TWO, BELIEF_PRESENT,
			[{SourceForecast.MATERIAL_PAYOFF_ID_KEY: "stone",
				SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY: STONE_PILE}]))
		h._assert_hud("%s: lesson + crew + site opens the rung for a press" % branch,
			String(open_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_OPEN
				and bool(open_row.get(RungLadder.ROW_SELECTABLE_KEY, false)))
	var belief_row := _target_row(_track(BELIEF_BRANCH, learned, CREW_TWO, BELIEF_NONE))
	h._assert_hud("a tile with no dead locks the monument on the site term (%s)" % belief_row.get(
			RungLadder.ROW_FACE_KEY, ""),
		String(belief_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_LOCKED
			and String(belief_row.get(RungLadder.ROW_FACE_KEY, "")).contains(
				HudTileVocab.SITE_NO_BELIEF_SHORT))
	var pit_row := _target_row(_track(STORES_BRANCH, learned, CREW_TWO, BELIEF_NONE))
	h._assert_hud("…and the pit on the same bare ground is not refused for belief",
		String(pit_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_OPEN)
	# Stone short: the monument's 20 stone against an EMPTY store is an amber aside, not a refusal.
	var dry_row := _target_row(_track(BELIEF_BRANCH, learned, CREW_TWO, BELIEF_PRESENT, []))
	var amber := false
	for aside in (dry_row.get(RungLadder.ROW_BUILD_ASIDES_KEY, []) as Array):
		if bool((aside as Dictionary).get(RungLadder.RUNG_ASIDE_WARN_KEY, false)):
			amber = true
	h._assert_hud("a dry store stalls the monument: an AMBER aside beside the price, the rung still open",
		amber and String(dry_row.get(RungLadder.ROW_STATE_KEY, "")) == RungLadder.STATE_OPEN)

	# ---- THE STEP CARD, REAL POPUP, REAL PRESS ------------------------------------------------------
	var crewed := _band(CREW_TWO, CREW_TWO)
	_stage(crewed, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	_step_button(BELIEF_BRANCH).pressed.emit()
	await h._settle()
	h._assert_hud("the step button opens the shared rung card",
		h._hud._rungcard.is_open())
	await h._save("tile_branches_step_card")
	h._hud._rungcard.dismiss()
	await h._settle()
	var raise_line := await _press_step(BELIEF_BRANCH)
	h._assert_hud("pressing the monument rung emits the exact build line (%s)" % raise_line,
		raise_line == "build raise %d %d %d" % [HudConst.PLAYER_FACTION_ID, TILE_X, TILE_Y])
	var dig_line := await _press_step(STORES_BRANCH)
	h._assert_hud("pressing the pit rung emits the exact build line (%s)" % dig_line,
		dig_line == "build dig %d %d %d" % [HudConst.PLAYER_FACTION_ID, TILE_X, TILE_Y])

	# Without a crew the card still opens and explains: the press is available, the rung is not.
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	_step_button(BELIEF_BRANCH).pressed.emit()
	await h._settle()
	await h._save("tile_branches_step_card_no_crew")
	h._hud._rungcard.dismiss()
	await h._settle()

	# ---- THE WORK TAB'S TILE ROW GETS THE LADDER CHIP -----------------------------------------------
	h._hud.update_tile_beliefs({Vector2i(TILE_X, TILE_Y): BELIEF_PRESENT})
	var models: Array = h._hud._bandpanel._tile_source_models(crewed)
	h._assert_hud("the Work tab models both tile crews", models.size() == 2)
	var strip_row := {}
	var improvement := _improvement_row(BELIEF_BRANCH, "belief:gathering_ground")
	h._hud._bandpanel._put_tile_ladder_chip(strip_row, improvement, Vector2i(TILE_X, TILE_Y),
		BELIEF_BRANCH, CREW_TWO)
	h._assert_hud("…and a crewed tile row offers the monument's glyph on its ladder chip",
		String(strip_row.get("ready_glyph", "")) == FoodIcons.for_policy("raise"))
	h._hud.update_tile_beliefs({})
	var bare_row := {}
	h._hud._bandpanel._put_tile_ladder_chip(bare_row, improvement, Vector2i(TILE_X, TILE_Y),
		BELIEF_BRANCH, CREW_TWO)
	h._assert_hud("…and offers nothing on ground with no dead (the site term, read off the map's lookup)",
		not bare_row.has("ready_glyph"))

	# Leave the HUD as the next chapter found it.
	_stage(_band(CREW_NONE, CREW_NONE), [], 0.0, 0.0)
	h._hud.update_tile_improvements([])
