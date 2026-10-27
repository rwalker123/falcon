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
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const DoorQ := preload("res://tools/ui_preview/door_query.gd")
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
## Some dead, but short of the catalog's minimum.
const BELIEF_FEW := 0.5

## The crew the step card is asked at: nobody, and two hands.
const CREW_NONE := 0
const CREW_TWO := 2
## A turn past any the harness stages an assign on, so `reconcile_pending` retires every un-acknowledged crew.
const PENDING_RETIRED_TURN := 1000000
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

## A band HOLDING a tile row on each branch at ZERO staffed (Max 1) -- the sim's state before any build is
## declared, since a tile improvement has no take and the builders are dealt only to a declared build.
func _band_held_unstaffed() -> Dictionary:
	var band := _band(CREW_NONE, CREW_NONE)
	var assignments: Array = (band.get("labor_assignments", []) as Array).duplicate()
	for branch in [BELIEF_BRANCH, STORES_BRANCH]:
		var row := _tile_assignment(branch, CREW_NONE)
		row["max_workers"] = 1
		assignments.append(row)
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

## The card row that states a branch, off the CATALOG exactly as the producer reads it (the dead's own
## `Ancestors` row for the belief site term, the branch's display name for the other).
func _row_key(branch: String) -> String:
	var ladder := HudTileVocab.tile_ladder(_catalog())
	return HudTileVocab.readout_row_key(HudTileVocab.first_buildable(ladder, branch),
		h._hud._topbar.ladder_branches())

## Is the branch's row on the card at all (door or readout)?
func _row_present(branch: String) -> bool:
	return String(h._hud.tile_detail.text).contains("[color=#%s]%s[/color]" % [
		HudStyle.INK_DIM_HEX, _row_key(branch)])

func _is_door(branch: String) -> bool:
	return DoorQ.is_door(h._hud, _row_key(branch))

func _press_door(branch: String) -> bool:
	return DoorQ.press(h._hud, _row_key(branch))

## The row's hover (what the greyed amber reason line used to say).
func _reason_text(branch: String) -> String:
	var meta := DoorQ.meta_for(h._hud, _row_key(branch))
	if meta != "":
		return DoorQ.hover_text(h._hud, _row_key(branch))
	return String(h._hud._drawer._land_ctx.row_tooltips.get(_row_key(branch), ""))

func _door_count() -> int:
	return DoorQ.all_metas(h._hud).size()

func _sheet() -> Node:
	return h._hud._drawercompose._compose_sheet

func _close_sheet() -> void:
	h._hud._drawercompose.close_compose_sheet()

## Open the crew sheet from the tile card's own button, set the stepper to `plus_presses` more than the
## seeded crew and commit; returns the command line the real builder makes of the emitted payload.
func _commit_crew(branch: String, plus_presses: int) -> String:
	if not _is_door(branch):
		return "<no crew door>"
	var sent: Array = []
	var sink := func(payload: Dictionary) -> void: sent.append(payload)
	h._hud.assign_labor_requested.connect(sink)
	_press_door(branch)
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
	_open_step_card(branch)
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

## The step card (Monument / Storage pit) opens from the Work tab's tile row chip -- the same
## `RungCardController.open_tile_track` that chip calls -- not from the tile card.
## The gate's input is the REAL `tile_rows_held` (what the chip passes), so a held row at zero staffed and
## a hex with no row are told apart by the band staged, not by a literal here.
func _open_step_card(branch: String) -> void:
	var held: int = h._hud._band_labor.tile_rows_held(TILE_X, TILE_Y, branch)
	h._hud._rungcard.open_tile_track(_band(CREW_TWO, CREW_TWO), _tile(BELIEF_PRESENT), branch, held,
		h._hud.tile_detail)

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

	# ---- THE TILE CARD ROWS ------------------------------------------------------------------------
	# Before either lesson is known, and with no improvement standing, no tile-branch row is a door: the
	# dead are a plain readout and the store has no row at all.
	_stage(band, [], 0.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("before the lessons, the Ancestors row is a plain readout and nothing on the card is a door",
		_row_present(BELIEF_BRANCH) and not _is_door(BELIEF_BRANCH) and _door_count() == 0)
	h._assert_hud("…and the store has no row at all",
		not _row_present(STORES_BRANCH))
	await h._save("tile_branches_hidden")

	# Lessons known: BOTH branches' rows are doors, valued off the catalog (the rung the tile stands at).
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("with both lessons known the card carries one door per tile branch", _door_count() == 2)
	h._assert_hud("…the dead's row is the belief branch's door",
		_row_key(BELIEF_BRANCH) == DetailFormat.ANCESTORS_ROW and _is_door(BELIEF_BRANCH))
	h._assert_hud("…the store row is keyed by the catalog's branch name and reads the standing rung",
		_row_key(STORES_BRANCH) == "Food stores" and _is_door(STORES_BRANCH)
			and String(h._hud.tile_detail.text).contains("No store"))
	h._assert_hud("…the retired foot buttons are gone from the scene",
		not _foot_stack_exists())
	await h._save("tile_branches_rows")

	# One lesson only: just that branch's row is a door; the other has no row (neither known nor held).
	_stage(band, [], 1.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("with only Ritual known, only the Ancestors row is a door and the store has no row",
		_door_count() == 1 and _is_door(BELIEF_BRANCH) and not _row_present(STORES_BRANCH))
	# …and an improvement already standing shows its row with no lesson at all (the held-here arm).
	_stage(band, [_improvement_row(STORES_BRANCH, "food_stores:storage_pit")], 1.0, 0.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("a tile already holding a pit shows the store row (a door) without the lesson, valued at the pit",
		_is_door(STORES_BRANCH) and String(h._hud.tile_detail.text).contains("Storage pit"))

	# The site term fails. A tile with no dead has no Ancestors row at all; one with too few keeps the
	# row as a plain readout whose hover carries the reason the greyed amber line used to.
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_NONE))
	await h._settle()
	h._assert_hud("a tile with no dead has no Ancestors row at all", not _row_present(BELIEF_BRANCH))
	h._assert_hud("…while the store row on the same tile is untouched by the belief term",
		_is_door(STORES_BRANCH))
	h._show_tile(_tile(BELIEF_FEW))
	await h._settle()
	h._assert_hud("a tile with too few dead keeps the Ancestors row as a plain readout, not a door",
		_row_present(BELIEF_BRANCH) and not _is_door(BELIEF_BRANCH))
	h._assert_hud("…and its hover says why in plain words (%s)" % _reason_text(BELIEF_BRANCH),
		_reason_text(BELIEF_BRANCH).contains(HudTileVocab.SITE_FEW_BELIEF_SHORT)
			and _reason_text(BELIEF_BRANCH).contains(HudTileVocab.SITE_BELIEF_HAVE_SOME))
	h._assert_hud("…and pressing nothing opens no sheet (a readout row carries no door meta)",
		not _press_door(BELIEF_BRANCH) and not h._hud.is_compose_sheet_open())
	await h._save("tile_branches_site_greyed")
	# ⛔ A refused branch is a readout, but a crew standing on it is still a stated fact: full-ink figure + count,
	# outside any url. No sim state produces it (belief never decays and the site term is checked when a crew is
	# put on), so the claim forces ONLY the model's crew on the REAL refused model and drives the real producer.
	var refused: Dictionary = {}
	for model in h._hud._drawercompose.tile_branch_models(h._hud._selection.tile_info()):
		if String(model["branch"]) == BELIEF_BRANCH:
			refused = model.duplicate()
	var refused_ctx := DetailFormat.Context.new()
	var refused_lines: Array[String] = []
	refused["crew"] = CREW_TWO
	h._hud._drawer._append_tile_row(refused_lines, refused_ctx, "%s: 1 dead" % DetailFormat.ANCESTORS_ROW, refused)
	var refused_markup := DetailFormat.detail_bbcode(refused_lines, refused_ctx)
	h._assert_hud("a refused (readout) branch with a crew states it: full-ink figure and the count, outside any url (%s)"
			% refused_markup.substr(0, 40),
		not refused["door"] and refused_ctx.door_metas.is_empty()
			and refused_markup.count(HudSprites.SPRITE_PATHS["workers"]) == 1
			and refused_markup.contains("color=#%sff" % HudStyle.INK_HEX)
			and refused_markup.contains("\u00a0%d" % CREW_TWO)
			and not refused_markup.contains("[url=door:"))
	refused["crew"] = CREW_NONE
	var bare_lines: Array[String] = []
	var bare_ctx := DetailFormat.Context.new()
	h._hud._drawer._append_tile_row(bare_lines, bare_ctx, "%s: 1 dead" % DetailFormat.ANCESTORS_ROW, refused)
	h._assert_hud("…and a refused readout with no crew carries no mark",
		DetailFormat.detail_bbcode(bare_lines, bare_ctx).count(HudSprites.SPRITE_PATHS["workers"]) == 0)

	# ---- THE CREW SHEET ---------------------------------------------------------------------------
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	_press_door(BELIEF_BRANCH)
	await h._settle()
	h._assert_hud("the Ancestors door opens the crew sheet on its branch",
		h._hud.is_compose_sheet_open() and h._hud._compose.kind() == ComposeState.KIND_TILE
			and h._hud._compose.subject() == "%d,%d:%s" % [TILE_X, TILE_Y, BELIEF_BRANCH])
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
					HudTileVocab.GATE_LONG_NO_CREW))
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
	_open_step_card(BELIEF_BRANCH)
	await h._settle()
	h._assert_hud("the Work tab chip's call opens the shared rung card",
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

	# ⛔ A row HELD at zero staffed (the sim's state until a build is declared) is a live press: the gate asks
	# whether a band holds the row, never the staffed count -- else the press that declares the build can
	# never be made.
	# The crew sheets above left their un-acknowledged crews in the overlay; a turn's snapshot retires them.
	h._hud._band_labor.reconcile_pending(PENDING_RETIRED_TURN)
	_stage(_band_held_unstaffed(), [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	var band_probe: Dictionary = h._hud._band_labor.current_player_bands()[0]
	h._assert_hud("a held tile row at zero staffed counts as held (rows held = 1, staffed = 0)",
		h._hud._band_labor.tile_rows_held(TILE_X, TILE_Y, STORES_BRANCH) == 1
			and (h._hud._band_labor.effective_tile_workers(band_probe, TILE_X, TILE_Y, STORES_BRANCH)) == 0)
	var held_dig := await _press_step(STORES_BRANCH)
	h._assert_hud("a held-but-unstaffed row: the Storage pit press emits the build line (%s)" % held_dig,
		held_dig == "build dig %d %d %d" % [HudConst.PLAYER_FACTION_ID, TILE_X, TILE_Y])
	var held_raise := await _press_step(BELIEF_BRANCH)
	h._assert_hud("…and the monument press too (%s)" % held_raise,
		held_raise == "build raise %d %d %d" % [HudConst.PLAYER_FACTION_ID, TILE_X, TILE_Y])
	_open_step_card(STORES_BRANCH)
	await h._settle()
	await h._save("tile_branches_step_card_held_unstaffed")
	h._hud._rungcard.dismiss()
	await h._settle()

	# No row at all keeps the refusal.
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	var no_row_dig := await _press_step(STORES_BRANCH)
	h._assert_hud("a hex with no tile row still refuses the press (%s)" % no_row_dig,
		no_row_dig == "<nothing emitted>")

	# Without a crew the card still opens and explains: the press is available, the rung is not.
	_stage(band, [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	_open_step_card(BELIEF_BRANCH)
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

	await _door_state_claims()
	await _crew_sheet_chip_claims()
	await _tile_row_art_claims()

	# Leave the HUD as the next chapter found it.
	_stage(_band(CREW_NONE, CREW_NONE), [], 0.0, 0.0)
	h._hud.update_tile_improvements([])

## Does any of the retired foot-stack containers or buttons exist? Looked for by node name and by meta,
## anywhere under the HUD.
func _foot_stack_exists() -> bool:
	for name in ["ForageAssignControls", "ForestryAssignControls", "ExtractionAssignControls",
			"TileBranchControls", "RoadLadderControls"]:
		if h._hud.find_child(name, true, false) != null:
			return true
	return not _collect_meta(h._hud, HudRouteVocab.ROAD_LADDER_ACTION_META, []).is_empty()

## The wash, hover and selected claims, on the Ancestors and store doors (the rows of this chapter's card).
func _door_state_claims() -> void:
	_stage(_band(CREW_NONE, CREW_NONE), [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	var ancestors := _row_key(BELIEF_BRANCH)
	var stores := _row_key(STORES_BRANCH)
	var meta_a := DoorQ.meta_for(h._hud, ancestors)
	var meta_s := DoorQ.meta_for(h._hud, stores)
	var drawer = h._hud._drawer
	# -- at rest: no wash anywhere
	h._assert_hud("at rest no door row wears a wash",
		DoorQ.washed(h._hud, HudStyle.DOOR_HOVER_WASH_HEX).is_empty()
			and DoorQ.washed(h._hud, HudStyle.DOOR_SELECTED_WASH_HEX).is_empty())
	DoorQ.assert_marks(h, "tile_branches", [ancestors, stores])
	# -- hover: starts washes that row on BOTH cells and only that row; ended clears it
	h._hud.tile_detail.meta_hover_started.emit(meta_a)
	await h._settle()
	h._assert_hud("hovering a door washes that row, both cells, with the hover wash (and keeps it a frame later)",
		DoorQ.washed(h._hud, HudStyle.DOOR_HOVER_WASH_HEX) == [meta_a]
			and DoorQ.cell_tags(h._hud, ancestors).size() == 2
			and DoorQ.cell_tags(h._hud, ancestors)[0].contains("bg=#" + HudStyle.DOOR_HOVER_WASH_HEX)
			and DoorQ.cell_tags(h._hud, ancestors)[1].contains("bg=#" + HudStyle.DOOR_HOVER_WASH_HEX))
	h._assert_hud("…and the table is still ONE table",
		String(h._hud.tile_detail.text).count("[table=") == 1)
	await h._save("tile_branches_door_hovered")
	h._hud.tile_detail.meta_hover_ended.emit(meta_a)
	await h._settle()
	h._assert_hud("ending the hover clears the wash",
		DoorQ.washed(h._hud, HudStyle.DOOR_HOVER_WASH_HEX).is_empty())
	# -- churn guard: ended -> started for ONE meta re-renders at most once
	h._hud.tile_detail.meta_hover_started.emit(meta_a)
	var before: int = drawer.land_markup_writes()
	h._hud.tile_detail.meta_hover_ended.emit(meta_a)
	h._hud.tile_detail.meta_hover_started.emit(meta_a)
	await h._settle()
	h._assert_hud("an ended -> started pair for the same door re-renders nothing (%d writes)" % (
			drawer.land_markup_writes() - before),
		drawer.land_markup_writes() - before == 0
			and DoorQ.washed(h._hud, HudStyle.DOOR_HOVER_WASH_HEX) == [meta_a])
	h._hud.tile_detail.meta_hover_ended.emit(meta_a)
	await h._settle()
	# -- selected: the open sheet washes its row, stronger, and only that row; closing takes it down
	DoorQ.press(h._hud, ancestors)
	await h._settle()
	h._assert_hud("an open crew sheet washes its own row with the SELECTED wash and no other",
		DoorQ.washed(h._hud, HudStyle.DOOR_SELECTED_WASH_HEX) == [meta_a])
	h._hud.tile_detail.meta_hover_started.emit(meta_a)
	h._assert_hud("selected + hovered shows selected",
		DoorQ.washed(h._hud, HudStyle.DOOR_SELECTED_WASH_HEX) == [meta_a]
			and DoorQ.washed(h._hud, HudStyle.DOOR_HOVER_WASH_HEX).is_empty())
	await h._save("tile_branches_door_selected")
	h._hud.tile_detail.meta_hover_ended.emit(meta_a)
	h._hud.close_compose_sheet()
	await h._settle()
	h._assert_hud("closing the sheet takes the selected wash down",
		DoorQ.washed(h._hud, HudStyle.DOOR_SELECTED_WASH_HEX).is_empty())
	# -- the second door opens ITS sheet, not the first's
	DoorQ.press(h._hud, stores)
	await h._settle()
	h._assert_hud("the store door opens the store's sheet and washes the store row",
		h._hud._compose.kind() == ComposeState.KIND_TILE
			and h._hud._compose.subject() == "%d,%d:%s" % [TILE_X, TILE_Y, STORES_BRANCH]
			and DoorQ.washed(h._hud, HudStyle.DOOR_SELECTED_WASH_HEX) == [meta_s])
	h._hud.close_compose_sheet()
	await h._settle()
	# -- standing facts: the crew rides the row, and the hover carries the refusal-free row sentence
	_stage(_band(CREW_TWO, CREW_NONE), [], 1.0, 1.0)
	h._show_tile(_tile(BELIEF_PRESENT))
	await h._settle()
	h._assert_hud("a crew on the belief branch shows its count after the full-ink figure on the Ancestors row (%s | %s)"
			% [DoorQ.mark_tint(h._hud, ancestors), DoorQ.mark_text(h._hud, ancestors)],
		DoorQ.mark_text(h._hud, ancestors) == str(CREW_TWO)
			and DoorQ.mark_tint(h._hud, ancestors) == HudStyle.INK_HEX + "ff")
	h._assert_hud("…and the store row, with nobody on it, shows the dim figure alone and no count (%s | %s)"
			% [DoorQ.mark_tint(h._hud, stores), DoorQ.mark_text(h._hud, stores)],
		DoorQ.mark_text(h._hud, stores) == ""
			and DoorQ.mark_images(h._hud, stores) == 1
			and DoorQ.mark_tint(h._hud, stores) == HudStyle.INK_DIM_HEX + "ff")
	await h._save("tile_branches_door_crews")
	_stage(_band(CREW_NONE, CREW_NONE), [], 0.0, 0.0)

## **THE WORK TAB TILE ROW'S ART AND ITS PROGRESS** (issue #709). A queued dig carrying the wire's
## `build_fraction` 0.25 reads 25% on the chip -- asserted on the fill layer's stamped amount and on the chip's
## own label, found by meta, not by a text search of the row -- and the icon slot and the chip wear the
## `HudSprites` art of the branch's built rung. A HELD row with nothing queued offers the rung with the same art.
const QUEUED_BUILD_FRACTION := 0.25
const STORED_FOOD := 14.49
const STORE_CAPACITY := 400.0
const ROW_HOST_POS := Vector2(24.0, 24.0)
const ROW_HOST_WIDTH := 360.0

func _chip_label(chip: Node) -> Label:
	for node in chip.find_children("*", "Label", true, false):
		return node as Label
	return null

func _chip_texture(chip: Node) -> Texture2D:
	for node in chip.find_children("*", "TextureRect", true, false):
		return (node as TextureRect).texture
	return null

func _tile_row_art_claims() -> void:
	var host := PanelContainer.new()
	var box := StyleBoxFlat.new()
	box.bg_color = HudStyle.PANEL_SOLID
	host.add_theme_stylebox_override("panel", box)
	host.position = ROW_HOST_POS
	host.custom_minimum_size = Vector2(ROW_HOST_WIDTH, 0.0)
	var column := VBoxContainer.new()
	host.add_child(column)
	h._hud.add_child(host)
	h._hud.update_tile_beliefs({Vector2i(TILE_X, TILE_Y): BELIEF_PRESENT})
	var band := _band_held_unstaffed()
	var queued := _improvement_row(STORES_BRANCH, "food_stores:unstored")
	queued["is_queued"] = true
	queued["build_fraction"] = QUEUED_BUILD_FRACTION
	queued["builders_staffed"] = 1
	queued["builders_cap"] = 3
	queued["build_turns_remaining"] = 3
	_stage(band, [queued], 1.0, 1.0)
	var bp = h._hud._bandpanel
	var assignment := _tile_assignment(STORES_BRANCH, CREW_NONE)
	var building_row: Control = bp._build_tile_row(band, bp._tile_model(band, TILE_X, TILE_Y, STORES_BRANCH, assignment))
	column.add_child(building_row)
	var icon := Q.find_meta_node(building_row, HudWorkVocab.TILE_ROW_MARK_META)
	var pit_art := HudSprites.for_mark("storage_pit")
	h._assert_hud("tile row: the pit art is bundled and loads", pit_art != null)
	h._assert_hud("tile row (food_stores): the icon slot is the built rung's art, a TextureRect",
		icon is TextureRect and (icon as TextureRect).texture == pit_art
			and String(icon.get_meta(HudWorkVocab.TILE_ROW_MARK_META)) == "storage_pit")
	var chip := Q.find_meta_node(building_row, HudWorkVocab.WORK_STRIP_LADDER_META)
	var fill := Q.find_meta_node(chip, HudWorkVocab.WORK_STRIP_FILL_META) if chip != null else null
	h._assert_hud("tile row: a queued dig with build_fraction 0.25 stamps the chip fill at 0.25 (%s)"
			% [fill.get_meta(HudWorkVocab.WORK_STRIP_FILL_META) if fill != null else "no fill"],
		fill != null and is_equal_approx(float(fill.get_meta(HudWorkVocab.WORK_STRIP_FILL_META)), QUEUED_BUILD_FRACTION))
	var chip_label := _chip_label(chip) if chip != null else null
	h._assert_hud("…and the chip reads 25%% with the pit art, not the dig glyph (%s)"
			% [chip_label.text if chip_label != null else "no label"],
		chip_label != null and chip_label.text == "25%" and _chip_texture(chip) == pit_art)
	h._assert_hud("…and the chip's model carries the mark id",
		String(ForageFx.strip_model(building_row).get(WorkStrip.M_LADDER_MARK, "")) == "storage_pit")
	h._assert_hud("…and the Work tab row's chip carries NO rung name (the name is the crew sheet's alone)",
		String(ForageFx.strip_model(building_row).get(WorkStrip.M_LADDER_NAME, "x")) == ""
			and chip_label != null and not chip_label.text.contains("Storage pit"))
	# The crew sheet's strip row: the same chip, named. Built through the sheet's own producer + WorkStrip.
	var sheet_strip := WorkStrip.build(WorkStrip.model_from_row(
		bp.tile_strip_row(band, TILE_X, TILE_Y, STORES_BRANCH)), {}, true)
	column.add_child(sheet_strip)
	var sheet_chip := Q.find_meta_node(sheet_strip, HudWorkVocab.WORK_STRIP_LADDER_META)
	var sheet_label := _chip_label(sheet_chip) if sheet_chip != null else null
	h._assert_hud("crew sheet chip, queued: reads `Storage pit 25%%` beside the art, label clipped with an ellipsis (%s)"
			% [sheet_label.text if sheet_label != null else "no label"],
		sheet_label != null and sheet_label.text == "Storage pit 25%"
			and sheet_label.text_overrun_behavior == TextServer.OVERRUN_TRIM_ELLIPSIS
			and _chip_texture(sheet_chip) == pit_art
			and (sheet_chip as Control).tooltip_text.begins_with("Storage pit"))
	await h._settle()
	await h._save("tile_branches_row_art")
	sheet_strip.queue_free()

	# A held row with nothing queued: the rung is OFFERED, wearing the same art; the belief branch's is the dolmen.
	_stage(band, [], 1.0, 1.0)
	for branch in [STORES_BRANCH, BELIEF_BRANCH]:
		var mark := "storage_pit" if branch == STORES_BRANCH else "monument"
		var offer_row: Control = bp._build_tile_row(band,
			bp._tile_model(band, TILE_X, TILE_Y, branch, _tile_assignment(branch, CREW_TWO)))
		column.add_child(offer_row)
		var offer_chip := Q.find_meta_node(offer_row, HudWorkVocab.WORK_STRIP_LADDER_META)
		var slot := Q.find_meta_node(offer_row, HudWorkVocab.TILE_ROW_MARK_META)
		h._assert_hud("tile row (%s), offered: icon slot is %s art and the chip wears it" % [branch, mark],
			slot is TextureRect and (slot as TextureRect).texture == HudSprites.for_mark(mark)
				and offer_chip != null and _chip_texture(offer_chip) == HudSprites.for_mark(mark))
		var offer_name := "Storage pit" if branch == STORES_BRANCH else "Monument"
		var offer_strip := WorkStrip.build(WorkStrip.model_from_row(
			bp.tile_strip_row(band, TILE_X, TILE_Y, branch)), {}, true)
		column.add_child(offer_strip)
		var named_chip := Q.find_meta_node(offer_strip, HudWorkVocab.WORK_STRIP_LADDER_META)
		var named_label := _chip_label(named_chip) if named_chip != null else null
		h._assert_hud("crew sheet chip (%s), offered: reads `%s ⌃` with the art (%s)" % [branch, offer_name,
				named_label.text if named_label != null else "no label"],
			named_label != null and named_label.text == offer_name + " ⌃"
				and _chip_texture(named_chip) == HudSprites.for_mark(mark))
		var plain_label := _chip_label(offer_chip) if offer_chip != null else null
		h._assert_hud("crew sheet chip (%s): the Work tab row's chip beside it has no name (%s)" % [branch,
				plain_label.text if plain_label != null else "no label"],
			plain_label != null and not plain_label.text.contains(offer_name))
	await h._settle()
	await h._save("tile_branches_row_art_offered")

	# THE STANDING-RUNG MARK: a bare tile has none; a tile standing on its BUILT rung wears that rung's art in
	# the strip's rung slot (the harvest rows' slot), hovering the rung's name.
	for child in column.get_children():
		column.remove_child(child)
		child.queue_free()
	await h._settle()
	for branch in [STORES_BRANCH, BELIEF_BRANCH]:
		var bare_row: Control = bp._build_tile_row(band,
			bp._tile_model(band, TILE_X, TILE_Y, branch, _tile_assignment(branch, CREW_TWO)))
		column.add_child(bare_row)
		# An empty slot stamps the marks run itself with a blank rung (`_add_rung_mark`), so "no mark" is a blank stamp
		# on a non-art node, never a TextureRect.
		h._assert_hud("tile row (%s), bare tile: no stored line" % branch,
			Q.find_meta_node(bare_row, HudWorkVocab.WORK_ROW_STORED_META) == null)
		var bare_stamp := Q.find_meta_node(bare_row, HudWorkVocab.WORK_ROW_RUNG_META)
		h._assert_hud("tile row (%s), bare tile: no standing-rung mark" % branch,
			bare_stamp != null and not (bare_stamp is TextureRect)
				and String(bare_stamp.get_meta(HudWorkVocab.WORK_ROW_RUNG_META)) == "")
	var built_rungs := {STORES_BRANCH: ["food_stores:storage_pit", "storage_pit", "Storage pit"],
		BELIEF_BRANCH: ["belief:monument", "monument", "Monument"]}
	for branch in [STORES_BRANCH, BELIEF_BRANCH]:
		var built := _improvement_row(branch, String(built_rungs[branch][0]))
		if branch == STORES_BRANCH:
			built["stored_food"] = STORED_FOOD
			built["store_capacity"] = STORE_CAPACITY
		_stage(band, [built], 1.0, 1.0)
		var built_row: Control = bp._build_tile_row(band,
			bp._tile_model(band, TILE_X, TILE_Y, branch, _tile_assignment(branch, CREW_TWO)))
		column.add_child(built_row)
		var standing := Q.find_meta_node(built_row, HudWorkVocab.WORK_ROW_RUNG_META)
		h._assert_hud("tile row (%s), built: the standing mark is a TextureRect wearing the %s art, hovering `%s`"
				% [branch, built_rungs[branch][1], built_rungs[branch][2]],
			standing is TextureRect
				and (standing as TextureRect).texture == HudSprites.for_mark(String(built_rungs[branch][1]))
				and (standing as TextureRect).tooltip_text.begins_with(String(built_rungs[branch][2]))
				and (standing as TextureRect).get_combined_minimum_size().y >= HudWorkVocab.WORK_ROW_RUNG_WIDTH)
		var stored_node := Q.find_meta_node(built_row, HudWorkVocab.WORK_ROW_STORED_META)
		if branch == STORES_BRANCH:
			h._assert_hud("tile row (food_stores), built store: line three reads exactly `14 / 400 stored` (%s)"
					% [(stored_node as Label).text if stored_node is Label else "no line"],
				stored_node is Label and (stored_node as Label).text == "14 / 400 stored"
					and (stored_node as Label).tooltip_text == HudWorkVocab.WORK_ROW_STORED_TOOLTIP)
		else:
			h._assert_hud("tile row (%s), built monument: no stored line" % branch, stored_node == null)
	# A loss this turn: the figure rides the line, in warn ink.
	var spoiling := _improvement_row(STORES_BRANCH, "food_stores:storage_pit")
	spoiling["stored_food"] = STORED_FOOD
	spoiling["store_capacity"] = STORE_CAPACITY
	spoiling["store_spoiled"] = 3.0
	_stage(band, [spoiling], 1.0, 1.0)
	var spoil_row: Control = bp._build_tile_row(band,
		bp._tile_model(band, TILE_X, TILE_Y, STORES_BRANCH, _tile_assignment(STORES_BRANCH, CREW_TWO)))
	column.add_child(spoil_row)
	var spoil_node := Q.find_meta_node(spoil_row, HudWorkVocab.WORK_ROW_STORED_META)
	h._assert_hud("tile row, spoiling store: `14 / 400 stored · 3 spoiled` in warn ink",
		spoil_node is Label and (spoil_node as Label).text == "14 / 400 stored · 3 spoiled"
			and (spoil_node as Label).get_theme_color("font_color") == HudStyle.WARN)
	# A pit whose contents are not published (capacity 0) draws no line, never `0 / 0`.
	var unpublished := _improvement_row(STORES_BRANCH, "food_stores:storage_pit")
	_stage(band, [unpublished], 1.0, 1.0)
	var hidden_row: Control = bp._build_tile_row(band,
		bp._tile_model(band, TILE_X, TILE_Y, STORES_BRANCH, _tile_assignment(STORES_BRANCH, CREW_TWO)))
	column.add_child(hidden_row)
	h._assert_hud("tile row, store with nothing published: no stored line",
		Q.find_meta_node(hidden_row, HudWorkVocab.WORK_ROW_STORED_META) == null)
	await h._settle()
	await h._save("tile_branches_row_standing_mark")
	host.queue_free()
	h._hud.update_tile_beliefs({})
	await h._settle()

## The crew sheet's work-strip LADDER CHIP: on every state of the sheet, the same face as the Work tab's row
## for the same tile + branch, and a press opens the rung card for THAT branch.
func _crew_sheet_chip_claims() -> void:
	var rung_names := {BELIEF_BRANCH: "Monument", STORES_BRANCH: "Storage pit"}
	var rung_keys := {BELIEF_BRANCH: "belief:monument", STORES_BRANCH: "food_stores:storage_pit"}
	# The Work tab's chip reads the belief off the map's lookup (a tile with none offers no monument).
	h._hud.update_tile_beliefs({Vector2i(TILE_X, TILE_Y): BELIEF_PRESENT})
	for crewed in [false, true]:
		var band := _band(CREW_TWO if crewed else CREW_NONE, CREW_TWO if crewed else CREW_NONE)
		_stage(band, [], 1.0, 1.0)
		h._show_tile(_tile(BELIEF_PRESENT))
		await h._settle()
		for branch in [BELIEF_BRANCH, STORES_BRANCH]:
			var label := "%s sheet, %s" % [branch, "crewed" if crewed else "no crew yet"]
			_press_door(branch)
			await h._settle()
			var sheet = _sheet()
			var strip_model := ForageFx.strip_model(sheet)
			h._assert_hud("%s: the sheet mounts the work strip with a ladder chip (%s)" % [label,
					ForageFx.describe(sheet)],
				not strip_model.is_empty()
					and String(strip_model.get(WorkStrip.M_LADDER_KIND, WorkStrip.LADDER_NONE))
						!= WorkStrip.LADDER_NONE)
			# The Work tab's row for the same tile + branch, built by the board's own builder.
			var board_row: Control = h._hud._bandpanel._build_tile_row(band,
				h._hud._bandpanel._tile_model(band, TILE_X, TILE_Y, branch, _tile_assignment(branch, CREW_TWO)
					if crewed else {}))
			var board_model := ForageFx.strip_model(board_row)
			h._assert_hud("%s: the chip's face matches the Work tab row's (%s vs %s)" % [label,
					ForageFx.describe(sheet), ForageFx.describe(board_row)],
				String(strip_model.get(WorkStrip.M_LADDER_KIND, "")) == String(board_model.get(WorkStrip.M_LADDER_KIND, "x"))
					and strip_model.get(WorkStrip.M_LADDER_GLYPH) == board_model.get(WorkStrip.M_LADDER_GLYPH))
			board_row.free()
			var chip_button: Button = null
			var strip := ForageFx.find_strip(sheet)
			if strip != null:
				var chip := Q.find_meta_node(strip, HudWorkVocab.WORK_STRIP_LADDER_META)
				for child in (chip.get_children() if chip != null else []):
					if child is Button:
						chip_button = child as Button
			h._assert_hud("%s: the chip is a pressable button" % label, chip_button != null)
			if chip_button != null:
				if crewed and branch == BELIEF_BRANCH:
					await h._save("tile_branches_crew_sheet_chip")
				chip_button.pressed.emit()
				await h._settle()
				h._assert_hud("%s: pressing the chip opens the rung card" % label,
					h._hud._rungcard.is_open())
				var named := false
				for control in _collect_meta(h._hud, HudWorkVocab.RUNG_TRACK_RUNG_META):
					named = named or String(control.get_meta(HudWorkVocab.RUNG_TRACK_RUNG_META)) == rung_keys[branch]
				h._assert_hud("%s: …and its track carries the branch's own rung (%s)" % [label, rung_names[branch]],
					named and Q.has_label_containing(h._hud._rungcard.card(), rung_names[branch]))
				h._hud._rungcard.dismiss()
				await h._settle()
			h._hud.close_compose_sheet()
			await h._settle()
	h._hud.update_tile_beliefs({})
	_stage(_band(CREW_NONE, CREW_NONE), [], 0.0, 0.0)
