extends RefCounted

## A FOURTH STEP ON EACH WEB, WITH NO CLIENT EDIT (issue #175, the generic `rung_quotes`).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it,
## appended LAST so no earlier frame moves. It pushes a ladder catalog carrying two steps the shipped
## `intensification_ladder.json` does not have -- `plant:terrace` (verb `terrace`, above the Farm) and
## `animal:stud` (verb `stud`, above the Pen) -- and stands a Farm and a Pen beneath them. Nothing in
## `src/` names either step: if they draw, price, gate and commit, it is because the catalog and the
## sources' `rung_quotes` said so. The fixtures author them through `RungFx`'s `extra_step` keys.
##
## **THE CLAIMS ARE ABOUT THE CONTROLS**, read by identity (`ForageFx.find_improvement_control`), never
## by a face needle alone: the control exists, is in the state the knowledge dictates, and names the
## step by the catalog's own label.

## The checkpoints this chapter owes the walk -- assertions made plus frames saved, as a FLOOR.
const EXPECTED_CHECKPOINTS := 16

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The knowledge each extra step is gated on in the fixture catalog, and the shipped ones beneath it.
const KNOWLEDGE_TERRACING := "terracing"
const KNOWLEDGE_STUDING := "studbooks"
const KNOWN := 1.0
const UNKNOWN := 0.0

## What the extra steps cost and pay in the fixtures: distinct figures, so a row reading the wrong
## rung's quote lands on a number this chapter names.
const TERRACE_WORK_COST := 120.0
const TERRACE_PROGRESS_PART := 0.4
const TERRACE_YIELD := 2.8
const STUD_WORK_COST := 150.0
const STUD_YIELD := 1.9

## The catalog labels the fixture gives the extra steps (`RungFx.ladder_rungs`).
const TERRACE_LABEL := "Terrace"
const STUD_LABEL := "Stud"
const TERRACE_VERB := "terrace"
const STUD_VERB := "stud"

func run(harness) -> void:
	h = harness
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(true))
	await _terrace_states()
	await _stud_states()
	# Hand the shipped catalog back so nothing appended after this chapter inherits the extra steps.
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(false))

func _knowledge(terracing: float, studing: float) -> void:
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		"cultivation": KNOWN, "herding": KNOWN, "seed_selection": KNOWN, "penning": KNOWN,
		KNOWLEDGE_TERRACING: terracing, KNOWLEDGE_STUDING: studing}}])

## A Farm standing on its own ground, with `plant:terrace` above it priced and unreached.
func _field_tile(terrace_progress: float) -> Dictionary:
	var tile := TileFx.tended_tile_fixture()
	tile["patch_is_field"] = true
	tile["patch_field_progress"] = 1.0
	tile["patch_extra_step"] = true
	tile["patch_terrace_progress"] = terrace_progress
	tile["patch_terrace_work_cost"] = TERRACE_WORK_COST
	tile["patch_terrace_work_done"] = TERRACE_WORK_COST * terrace_progress
	tile["patch_terrace_yield"] = TERRACE_YIELD
	for entry_variant in tile.get("patch_composition", []):
		var entry: Dictionary = entry_variant
		entry["can_terrace"] = true
		entry["terrace_work_cost"] = TERRACE_WORK_COST
		entry["terrace_payoff"] = TERRACE_YIELD
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

func _terrace_states() -> void:
	h._hud._band_labor._player_band = BandFx.band_fixture()
	_knowledge(KNOWN, KNOWN)
	var tile := _field_tile(0.0)
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._save("fourth_rung_terrace_offered")
	h._assert_hud("…while an UNSTARTED Terrace draws no card row, only the offer",
		not h._hud.tile_detail.get_parsed_text().contains(TERRACE_LABEL))
	var sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Farm with a catalog step above it OFFERS that step: the `terrace` control is live",
		ForageFx.improvement_state(sheet, TERRACE_VERB) == HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._assert_hud("…it is named by the catalog's own label, not a client table",
		ForageFx.improvement_face(sheet, TERRACE_VERB).contains(TERRACE_LABEL))
	h._assert_hud("…and the rung below it is a DONE state, not a second offer",
		ForageFx.improvement_state(sheet, "sow") == HudWidgets.IMPROVEMENT_STATE_DONE)
	var ladder: Array = SourceForecast.improvement_ladder(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._assert_hud("the source's ladder is the catalog's, in climb order (%s)" % str(ladder),
		ladder == ["cultivate", "sow", TERRACE_VERB])

	# The step's own price rides the card once work is banked on it: a row of its own beside Farm.
	var part_tile := _field_tile(TERRACE_PROGRESS_PART)
	h._show_tile(part_tile)
	await h._settle()
	await h._save("fourth_rung_terrace_part_built")
	var card_text: String = h._hud.tile_detail.get_parsed_text()
	h._assert_hud("a part-built Terrace draws its own row on the tile card, by the catalog's name",
		card_text.contains(TERRACE_LABEL))

	# Without the knowledge the catalog names, the step is not offered -- the gate comes off the quote.
	_knowledge(UNKNOWN, KNOWN)
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._save("fourth_rung_terrace_locked")
	h._assert_hud("the step's knowledge unlearned: no live control for it",
		ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, TERRACE_VERB)
			!= HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._hud._drawercompose.close_compose_sheet()
	_knowledge(KNOWN, KNOWN)

	# The catalog's `requires_held_rung_below` on Terrace: a patch holding only Tended is not offered it,
	# and the gate names the rung it waits on (the Farm), whatever branch the step is on.
	var tended_only := _field_tile(0.0)
	tended_only["patch_is_field"] = false
	tended_only["patch_field_progress"] = 0.0
	RungFx.stamp_patch(tended_only, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._hud._compose.reset_forage_source()
	h._show_tile(tended_only)
	h._compose_forage(tended_only)
	await h._settle()
	h._assert_hud("a patch holding only Tended is NOT offered Terrace",
		ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, TERRACE_VERB)
			!= HudWidgets.IMPROVEMENT_STATE_OFFERED)
	var gates := RungGates.source_gates(tended_only, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		{"cultivation": KNOWN, "seed_selection": KNOWN, KNOWLEDGE_TERRACING: KNOWN})
	h._assert_hud("…and the step's own gate says the rung below is not held (%s)" % str(gates),
		gates.has(TERRACE_VERB))
	h._hud._drawercompose.close_compose_sheet()

func _stud_states() -> void:
	_knowledge(KNOWN, KNOWN)
	var herd := HerdFx.herd_fixture()
	herd["domestication"] = RungFx.DOMESTICATION_COMPLETE
	herd["corralled"] = true
	herd["corral_progress"] = 1.0
	herd["extra_step"] = true
	herd["husbandry_ceiling"] = STUD_VERB
	herd["stud_work_cost"] = STUD_WORK_COST
	herd["stud_yield"] = STUD_YIELD
	RungFx.stamp_herd(herd)
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	h._compose_herd(herd)
	await h._settle()
	await h._save("fourth_rung_stud_offered")
	var sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Pen with a catalog step above it OFFERS that step: the `stud` control is live",
		ForageFx.improvement_state(sheet, STUD_VERB) == HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._assert_hud("…named by the catalog's own label",
		ForageFx.improvement_face(sheet, STUD_VERB).contains(STUD_LABEL))
	h._assert_hud("…and a penned herd is enclosed by the catalog's trait, not by a flag",
		RungQuotes.is_enclosed(herd, ""))
	h._hud._drawercompose.close_compose_sheet()
