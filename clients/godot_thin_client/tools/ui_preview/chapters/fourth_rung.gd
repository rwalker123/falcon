extends RefCounted

## A FOURTH STEP ON EACH WEB, WITH NO CLIENT EDIT (issue #175, the generic `rung_quotes`).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it,
## appended LAST so no earlier frame moves. It pushes a ladder catalog carrying two steps the shipped
## `intensification_ladder.json` does not have -- `plant:farm` (verb `farm`, above the Field) and
## `animal:ranch` (verb `ranch`, above the Pen) -- and stands a Field and a Pen beneath them. Nothing in
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
const KNOWLEDGE_AGRONOMY := "agronomy"
const KNOWLEDGE_RANCHING := "ranching"
const KNOWN := 1.0
const UNKNOWN := 0.0

## What the extra steps cost and pay in the fixtures: distinct figures, so a row reading the wrong
## rung's quote lands on a number this chapter names.
const FARM_WORK_COST := 120.0
const FARM_PROGRESS_PART := 0.4
const FARM_YIELD := 2.8
const RANCH_WORK_COST := 150.0
const RANCH_YIELD := 1.9

## The catalog labels the fixture gives the extra steps (`RungFx.ladder_rungs`).
const FARM_LABEL := "Farm"
const RANCH_LABEL := "Ranch"
const FARM_VERB := "farm"
const RANCH_VERB := "ranch"

func run(harness) -> void:
	h = harness
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(true))
	await _farm_states()
	await _ranch_states()
	# Hand the shipped catalog back so nothing appended after this chapter inherits the extra steps.
	h._hud.update_ladder_rungs(RungFx.ladder_rungs(false))

func _knowledge(agronomy: float, ranching: float) -> void:
	h._hud.update_intensification([{"faction": 0, "knowledges": {
		"cultivation": KNOWN, "herding": KNOWN, "seed_selection": KNOWN, "penning": KNOWN,
		KNOWLEDGE_AGRONOMY: agronomy, KNOWLEDGE_RANCHING: ranching}}])

## A Field standing on its own ground, with `plant:farm` above it priced and unreached.
func _field_tile(farm_progress: float) -> Dictionary:
	var tile := TileFx.tended_tile_fixture()
	tile["patch_is_field"] = true
	tile["patch_field_progress"] = 1.0
	tile["patch_extra_step"] = true
	tile["patch_farm_progress"] = farm_progress
	tile["patch_farm_work_cost"] = FARM_WORK_COST
	tile["patch_farm_work_done"] = FARM_WORK_COST * farm_progress
	tile["patch_farm_yield"] = FARM_YIELD
	for entry_variant in tile.get("patch_composition", []):
		var entry: Dictionary = entry_variant
		entry["can_farm"] = true
		entry["farm_work_cost"] = FARM_WORK_COST
		entry["farm_payoff"] = FARM_YIELD
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

func _farm_states() -> void:
	h._hud._band_labor._player_band = BandFx.band_fixture()
	_knowledge(KNOWN, KNOWN)
	var tile := _field_tile(0.0)
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._save("fourth_rung_farm_offered")
	h._assert_hud("…while an UNSTARTED Farm draws no card row, only the offer",
		not h._hud.tile_detail.get_parsed_text().contains(FARM_LABEL))
	var sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Field with a catalog step above it OFFERS that step: the `farm` control is live",
		ForageFx.improvement_state(sheet, FARM_VERB) == HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._assert_hud("…it is named by the catalog's own label, not a client table",
		ForageFx.improvement_face(sheet, FARM_VERB).contains(FARM_LABEL))
	h._assert_hud("…and the rung below it is a DONE state, not a second offer",
		ForageFx.improvement_state(sheet, "sow") == HudWidgets.IMPROVEMENT_STATE_DONE)
	var ladder: Array = SourceForecast.improvement_ladder(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._assert_hud("the source's ladder is the catalog's, in climb order (%s)" % str(ladder),
		ladder == ["cultivate", "sow", FARM_VERB])

	# The step's own price rides the card once work is banked on it: a row of its own beside Field.
	var part_tile := _field_tile(FARM_PROGRESS_PART)
	h._show_tile(part_tile)
	await h._settle()
	await h._save("fourth_rung_farm_part_built")
	var card_text: String = h._hud.tile_detail.get_parsed_text()
	h._assert_hud("a part-built Farm draws its own row on the tile card, by the catalog's name",
		card_text.contains(FARM_LABEL))

	# Without the knowledge the catalog names, the step is not offered -- the gate comes off the quote.
	_knowledge(UNKNOWN, KNOWN)
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._save("fourth_rung_farm_locked")
	h._assert_hud("the step's knowledge unlearned: no live control for it",
		ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, FARM_VERB)
			!= HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._hud._drawercompose.close_compose_sheet()
	_knowledge(KNOWN, KNOWN)

	# The catalog's `requires_held_rung_below` on Farm: a patch holding only Tended is not offered it,
	# and the gate names the rung it waits on (the Field), whatever branch the step is on.
	var tended_only := _field_tile(0.0)
	tended_only["patch_is_field"] = false
	tended_only["patch_field_progress"] = 0.0
	RungFx.stamp_patch(tended_only, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._hud._compose.reset_forage_source()
	h._show_tile(tended_only)
	h._compose_forage(tended_only)
	await h._settle()
	h._assert_hud("a patch holding only Tended is NOT offered Farm",
		ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, FARM_VERB)
			!= HudWidgets.IMPROVEMENT_STATE_OFFERED)
	var gates := RungGates.source_gates(tended_only, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		{"cultivation": KNOWN, "seed_selection": KNOWN, KNOWLEDGE_AGRONOMY: KNOWN})
	h._assert_hud("…and the step's own gate says the rung below is not held (%s)" % str(gates),
		gates.has(FARM_VERB))
	h._hud._drawercompose.close_compose_sheet()

func _ranch_states() -> void:
	_knowledge(KNOWN, KNOWN)
	var herd := HerdFx.herd_fixture()
	herd["domestication"] = RungFx.DOMESTICATION_COMPLETE
	herd["corralled"] = true
	herd["corral_progress"] = 1.0
	herd["extra_step"] = true
	herd["husbandry_ceiling"] = RANCH_VERB
	herd["ranch_work_cost"] = RANCH_WORK_COST
	herd["ranch_yield"] = RANCH_YIELD
	RungFx.stamp_herd(herd)
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	h._compose_herd(herd)
	await h._settle()
	await h._save("fourth_rung_ranch_offered")
	var sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Pen with a catalog step above it OFFERS that step: the `ranch` control is live",
		ForageFx.improvement_state(sheet, RANCH_VERB) == HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._assert_hud("…named by the catalog's own label",
		ForageFx.improvement_face(sheet, RANCH_VERB).contains(RANCH_LABEL))
	h._assert_hud("…and a penned herd is enclosed by the catalog's trait, not by a flag",
		RungQuotes.is_enclosed(herd, ""))
	h._hud._drawercompose.close_compose_sheet()
