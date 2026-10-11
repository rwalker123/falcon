extends RefCounted

## MIGRATION MODE on the Assign hunters sheet and the herd card (`docs/plan_roaming_bands.md` §Migration
## mode is a choice on the hunt, not its own order).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is a
## set of frames changed. It is appended LAST, so no existing frame moves. The Work tab's row half lives
## in `band_panel_preview` (`_follow_row_states`), which is the harness that renders that board.
##
## What the frames judge: the box exists on a MIGRATORY herd's sheet and on no other, it sits ABOVE the
## WORK PARTY section, it is unchecked by default and seeds from the band's own row, the commit sends the
## bare `follow` token last, and the herd card's worked line says `moving with the herd`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 27

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The band: 8 tiles from the herd with an apron of 2 (the far-herd geometry), so the sheet mounts the
## WORK PARTY section the box has to sit above. A migratory herd's catching-up case IS this one.
const BAND_ENTITY := 871
const BAND_TILE := Vector2i(66, 18)
const HERD_TILE := Vector2i(66, 10)
const APRON := 2
const IDLE_WORKERS := 6
const CREW := 2

## The caravan reply the fixture's answerer hands back, so the section really renders (the numbers are
## not what this chapter judges; the section's PRESENCE is what gives "above" something to be above).
const PARTY_RATE_HOME := 0.30
const PARTY_WALK_TILES := 6
const PARTY_WALK_TURNS := 6
const PARTY_ON_ROAD := 2
const PARTY_FIRST_LOAD := 12

## **THE MEAL THE BAND CAN EAT BEFORE A KILL SPOILS** (`hunt_meals`), in food, and the keeping class the
## herd's meat resolves to. The meal is a few animals so the muster the need block quotes lands BELOW the
## herd's useful crew — the gap the frame exists to show.
const KEEPING_CLASS := "fresh"
const MEAL_ANIMALS := 2
## The herd's authored crew-take curve: each hunter brings down NEED_ANIMALS_PER_HUNTER animals a turn
## until the herd's room caps the take at NEED_CURVE_CAP_ANIMALS. The meal (MEAL_ANIMALS) is reached
## before the cap, so the meal-sized muster sits BELOW the useful crew (the plateau).
const NEED_ANIMALS_PER_HUNTER := 0.5
const NEED_CURVE_CAP_ANIMALS := 3.0

const MIGRATORY := "migratory"
const RESIDENT := "medium"

## The line a commit sends, captured off the real signal so the text under test is what `Main` formats.
var _sent_lines: Array[String] = []

func _band(followed: bool, herd_id: String) -> Dictionary:
	var assignments: Array = []
	if followed:
		assignments.append({"kind": "hunt", "workers": CREW, "workers_needed": CREW, "floor": 0.5,
			"fauna_id": herd_id, "target_x": HERD_TILE.x, "target_y": HERD_TILE.y,
			"actual_yield": 0.9, "sustainable_yield": 0.9, "move_with_herd": true})
	return BandFx.with_band_id({
		"name": "Ashfell", "id": "Ashfell", "entity": BAND_ENTITY, "faction": 0, "size": 80,
		"current_x": BAND_TILE.x, "current_y": BAND_TILE.y, "pos": [BAND_TILE.x, BAND_TILE.y],
		"hunt_meals": [{"keeping": KEEPING_CLASS,
			"food": (float(MEAL_ANIMALS) - 0.5) * HerdFx.BOAR_FOOD_PER_ANIMAL}],
		"working_age": 12, "idle_workers": IDLE_WORKERS,
		"work_range": APRON, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_forecast_horizon_turns": BandFx.FORECAST_HORIZON_TURNS,
		"band_move_tiles_per_turn": 1,
		"activity": "hunt" if followed else "forage", "labor_assignments": assignments})

func _herd(size_class: String) -> Dictionary:
	var herd := HerdFx.raid_boar_herd()
	herd["size_class"] = size_class
	herd["keeping"] = KEEPING_CLASS
	herd["x"] = HERD_TILE.x
	herd["y"] = HERD_TILE.y
	herd[ForecastFx.WORK_PARTY_FORECAST_KEY] = {
		"posts_a_party": true, "rate_home": PARTY_RATE_HOME,
		"walk_tiles": PARTY_WALK_TILES, "walk_turns": PARTY_WALK_TURNS,
		"hunters_on_the_road": PARTY_ON_ROAD, "first_load_turn": PARTY_FIRST_LOAD,
	}
	return herd

func _use_band(band: Dictionary) -> void:
	# A commit leaves its optimistic row behind, and the sheet seeds from the pending-aware map — so the
	# previous state's `follow` would seed this one. Each state starts from the band's wire row alone.
	h._hud._band_labor._pending_labor = {}
	h._hud._band_labor._player_bands = [band]
	h._hud._band_labor._player_band = band
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)

func _open(herd: Dictionary) -> void:
	h._show_herd(herd)
	h._compose_herd(herd, CREW)

func _box(sheet: Control) -> CheckBox:
	var node := Q.find_meta_node(sheet, HudComposeVocab.MOVE_CAMP_BOX_META)
	return node as CheckBox if node is CheckBox else null

## The box's and the party section's vertical positions in the sheet, so "above" is measured on the
## rendered layout and not assumed from the build order.
func _y_of(node: Control) -> float:
	return node.get_global_rect().position.y

## The curve length the fixture's crew-take answer is asked for when the harness reads it back.
const NEED_CURVE_WORKERS := 12

## The authored curve: whole-crew rows `1..max_workers`, rising per hunter and flat at the cap.
func _need_curve(max_workers: int) -> Array:
	var rows: Array = []
	for workers in range(1, max_workers + 1):
		var animals := minf(float(workers) * NEED_ANIMALS_PER_HUNTER, NEED_CURVE_CAP_ANIMALS)
		rows.append({
			SourceForecast.CREW_TAKE_WORKERS_KEY: workers,
			SourceForecast.CREW_TAKE_LOW_KEY: animals,
			SourceForecast.CREW_TAKE_LIKELY_KEY: animals,
			SourceForecast.CREW_TAKE_HIGH_KEY: animals,
			SourceForecast.HUNT_CREW_NEXT_RUNG_ANIMALS_KEY: 0.0,
			"next_rung_keep_hands": 0.0,
		})
	return rows

## `SourceForecast.hunt_need_crew` on a hand-made curve: smallest reaching row, cap at the useful crew,
## no row reaching it (the mammoth) -> useful, no meal for the class -> useful, sub-animal meal -> one.
func _check_hunt_need_crew() -> void:
	var likely := SourceForecast.CREW_TAKE_LIKELY_KEY
	var curve: Array = []
	for pair in [[1, 1.0], [2, 2.0], [3, 4.0], [4, 4.0]]:
		curve.append({"workers": pair[0], likely: pair[1]})
	var keeping := KEEPING_CLASS
	var meals: Array = [{"keeping": keeping, "food": 7.0}, {"keeping": "cured", "food": 100.0}]
	# food 7 / 3 per animal -> ceil = 3 animals -> the first row reaching 3 is the 3-worker row.
	h._assert_hud("need crew: the smallest row reaching the meal",
		SourceForecast.hunt_need_crew(meals, keeping, 3.0, curve, 4) == 3)
	h._assert_hud("need crew: capped at the useful crew",
		SourceForecast.hunt_need_crew(meals, keeping, 3.0, curve, 2) == 2)
	h._assert_hud("need crew: a meal no row reaches falls back to the useful crew",
		SourceForecast.hunt_need_crew(meals, "cured", 3.0, curve, 4) == 4)
	h._assert_hud("need crew: no meal for the class falls back to the useful crew",
		SourceForecast.hunt_need_crew(meals, "dried", 3.0, curve, 4) == 4)
	h._assert_hud("need crew: a meal under one animal still asks for one",
		SourceForecast.hunt_need_crew([{"keeping": keeping, "food": 0.1}], keeping, 3.0, curve, 4) == 1)
	h._assert_hud("need crew: an unbounded useful crew passes through when there is no meal",
		SourceForecast.hunt_need_crew([], keeping, 3.0, curve, SourceForecast.MAX_USEFUL_UNBOUNDED)
			== SourceForecast.MAX_USEFUL_UNBOUNDED)

func run(harness) -> void:
	h = harness
	_sent_lines = []
	var query: ForecastQuery = h._hud.forecast_query()
	query.set_sender(func(request_id: int, ask: Dictionary) -> bool:
		var reply: Dictionary = ForecastFx.answer(h._hud, request_id, ask)
		if String(ask.get("kind", "")) == ForecastQuery.KIND_HUNT_CREW_TAKE:
			reply["per_crew"] = _need_curve(int(ask.get("max_workers", 0)))
		query.deliver.call_deferred([reply])
		return true)
	var recorder := func(p: Dictionary) -> void:
		_sent_lines.append(String(MAIN_SCRIPT.format_assign_labor(p).get("line", "")))
	h._hud.assign_labor_requested.connect(recorder)

	# State 1 — A MIGRATORY herd, unchecked by default: the box and its one dim line, directly above the
	# WORK PARTY section.
	var herd := _herd(MIGRATORY)
	_use_band(_band(false, String(herd["id"])))
	_open(herd)
	await h._settle()
	await h._save("herd_follow_unchecked")
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var box := _box(sheet)
	h._assert_hud("a migratory herd's sheet carries the Move camp box", box != null)
	h._assert_hud("…worded exactly `%s`" % HudComposeVocab.MOVE_CAMP_LABEL,
		box != null and box.text == HudComposeVocab.MOVE_CAMP_LABEL)
	h._assert_hud("…unchecked by default", box != null and not box.button_pressed)
	h._assert_hud("…with its one dim sub-line `%s`" % HudComposeVocab.MOVE_CAMP_HINT,
		Q.has_label_containing(sheet, HudComposeVocab.MOVE_CAMP_HINT))
	var party := Q.find_meta_node(sheet, HudWidgets.WORK_PARTY_SECTION_META)
	h._assert_hud("…and the WORK PARTY section is mounted (the 'above' claim has something to measure)",
		party != null)
	h._assert_hud("…the box sits ABOVE the WORK PARTY section",
		box != null and party is Control and _y_of(box) < _y_of(party as Control))

	# State 2 — CHECKED: the commit sends the bare `follow` token, LAST on the hunt line.
	box.button_pressed = true
	await h._settle()
	await h._save("herd_need_checked")
	h._assert_hud("ticking the box is composed state", h._hud._compose.hunt_move_with_herd())
	var ticked: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a ticked sheet swaps the Hunters stepper for `%s`" % HudComposeVocab.NEED_BLOCK_TITLE,
		Q.find_meta_node(ticked, HudComposeVocab.NEED_BLOCK_META) != null
			and Q.has_label_containing(ticked, HudComposeVocab.NEED_BLOCK_TITLE)
			and Readout.stepper_count(ticked) == 0)
	h._assert_hud("…and carries no per-turn yield box (no hands are standing on the herd)",
		Readout.yields_text(ticked) == "")
	# The muster is sized to the MEAL: `ceil(food / food_per_animal)` animals, the first curve row that
	# reaches them — read here off the fixture's own curve, not off the function under test.
	var meal_crew := 0
	for row in _need_curve(NEED_CURVE_WORKERS):
		if float((row as Dictionary)[SourceForecast.CREW_TAKE_LIKELY_KEY]) * 1.001 >= float(MEAL_ANIMALS):
			meal_crew = int((row as Dictionary)["workers"])
			break
	h._assert_hud("the need block quotes the MEAL-sized muster (%d), `%s`" % [meal_crew,
			HudComposeVocab.NEED_BLOCK_HINT_FORMAT % meal_crew],
		meal_crew > 0 and Q.has_label_containing(ticked, HudComposeVocab.NEED_BLOCK_HINT_FORMAT % meal_crew))
	var plateau := SourceForecast.crew_take_plateau(_need_curve(NEED_CURVE_WORKERS))
	h._assert_hud("…and it is below the useful crew (the curve's plateau, %d)" % plateau,
		meal_crew > 0 and meal_crew < plateau)
	_check_hunt_need_crew()
	_sent_lines.clear()
	var commit := Q.compose_commit_button(h._hud._drawercompose._compose_sheet)
	if commit != null:
		commit.pressed.emit()
	await h._settle()
	h._assert_hud("a ticked commit sends ONE hunt line ending in the bare `follow` token (got %s)"
			% str(_sent_lines),
		_sent_lines.size() == 1 and _sent_lines[0].contains(" hunt ")
			and _sent_lines[0].contains(" %d%s" % [HudComposeVocab.NEED_ROW_STANDING_HANDS,
				MAIN_SCRIPT.FOLLOW_TOKEN])
			and _sent_lines[0].ends_with(MAIN_SCRIPT.FOLLOW_TOKEN))

	# State 3 — UNTICKED commit sends no token (the byte-identical line it always sent).
	h._hud._compose.reset_hunt_source()
	_use_band(_band(false, String(herd["id"])))
	_open(herd)
	await h._settle()
	_sent_lines.clear()
	var plain_commit := Q.compose_commit_button(h._hud._drawercompose._compose_sheet)
	if plain_commit != null:
		plain_commit.pressed.emit()
	await h._settle()
	h._assert_hud("an unticked commit sends no `follow` token (got %s)" % str(_sent_lines),
		_sent_lines.size() == 1 and not _sent_lines[0].contains("follow"))

	# State 4 — A RESIDENT herd: no box, no sub-line.
	var resident := _herd(RESIDENT)
	_use_band(_band(false, String(resident["id"])))
	_open(resident)
	await h._settle()
	await h._save("herd_follow_resident")
	var resident_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a resident herd's sheet has no Move camp box", _box(resident_sheet) == null)
	h._assert_hud("…and none of the box's sub-line",
		not Q.has_label_containing(resident_sheet, HudComposeVocab.MOVE_CAMP_HINT))

	# State 5 — REOPENING a followed row: the box seeds from the band's own row, and the herd card's worked
	# line says so.
	var followed_band := _band(true, String(herd["id"]))
	_use_band(followed_band)
	_open(herd)
	await h._settle()
	await h._save("herd_follow_reopened")
	var reopened := _box(h._hud._drawercompose._compose_sheet)
	h._assert_hud("reopening a followed hunt row seeds the box checked",
		reopened != null and reopened.button_pressed)
	h._assert_hud("…and the herd card's worked line says `%s`"
			% HudComposeVocab.STANDING_SUMMARY_MOVING_CLAUSE.strip_edges(),
		Q.has_label_containing(h._hud, HudComposeVocab.STANDING_SUMMARY_MOVING_CLAUSE.strip_edges()))

	h._hud.assign_labor_requested.disconnect(recorder)
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_bands = []
