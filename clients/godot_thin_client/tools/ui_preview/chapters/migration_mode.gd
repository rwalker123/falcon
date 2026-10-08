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
const EXPECTED_CHECKPOINTS := 17

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
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
		"working_age": 12, "idle_workers": IDLE_WORKERS,
		"work_range": APRON, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_forecast_horizon_turns": BandFx.FORECAST_HORIZON_TURNS,
		"band_move_tiles_per_turn": 1,
		"activity": "hunt" if followed else "forage", "labor_assignments": assignments})

func _herd(size_class: String) -> Dictionary:
	var herd := HerdFx.raid_boar_herd()
	herd["size_class"] = size_class
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

func run(harness) -> void:
	h = harness
	_sent_lines = []
	var query: ForecastQuery = h._hud.forecast_query()
	query.set_sender(func(request_id: int, ask: Dictionary) -> bool:
		query.deliver.call_deferred([ForecastFx.answer(h._hud, request_id, ask)])
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
	await h._save("herd_follow_checked")
	h._assert_hud("ticking the box is composed state", h._hud._compose.hunt_move_with_herd())
	_sent_lines.clear()
	var commit := Q.compose_commit_button(h._hud._drawercompose._compose_sheet)
	if commit != null:
		commit.pressed.emit()
	await h._settle()
	h._assert_hud("a ticked commit sends ONE hunt line ending in the bare `follow` token (got %s)"
			% str(_sent_lines),
		_sent_lines.size() == 1 and _sent_lines[0].contains(" hunt ")
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
