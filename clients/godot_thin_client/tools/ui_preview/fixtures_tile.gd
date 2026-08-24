## Tile, climate, river and visibility fixtures.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")

const VIS_ACTIVE := "active"

const VIS_DISCOVERED := "discovered"

## A hex in a given SIGHT state, deliberately carrying a herd in ALL THREE — including the unseen
## ones, where MapView would never have put one (it fog-gates `_herds_on_tile` at source). Feeding the
## HUD a "leaky" dict on purpose proves the HUD's own gate: on a Discovered/Unexplored hex it must
## refuse to list the herd and must say the contents are unknown, rather than showing an empty roster
## (which would read as "nothing here" — the exact lie this slice exists to kill).
static func sight_tile_fixture(visibility_state: String) -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["herds"] = [HerdFx.herd_fixture()]
	tile["herd_count"] = 1
	return tile

static func three_role_tile_fixture() -> Dictionary:
	return RungFx.stamp_patch({
		"x": 64, "y": 8,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.02,
		"temperature": 19.0,
		"height_display": "5 ▬▭▭▭▭▭▭▭▭▭",
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"food_kind": "river_garden",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": ForageFx.THREE_ROLE_STOCK,
		"patch_carrying_capacity": ForageFx.THREE_ROLE_CAPACITY,
		"patch_provisions_per_biomass": 0.012,
		"patch_fodder_per_biomass": 0.017,
		"patch_per_worker_biomass": 26.0,
		"patch_per_worker_yield": 0.31,
		"patch_is_cultivated": false,
		"patch_cultivation_progress": 0.0,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.38,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "cotton", "role": "cash", "display_name": "Cotton Fields", "share": 0.31,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 0.28, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.14, "sow_payoff": 0.0,
				"cultivate_material_payoff": [{"material_id": "fibre", "amount": 0.43}],
				"sow_material_payoff": [{"material_id": "fibre", "amount": 1.08}]},
			{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.31,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 0.25, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.12, "sow_payoff": 0.0,
				"cultivate_fodder_payoff": 0.72, "sow_fodder_payoff": 1.8},
		],
		"graze_biomass": ForageFx.THREE_ROLE_GRAZE_CAPACITY,
		"graze_capacity": ForageFx.THREE_ROLE_GRAZE_CAPACITY,
		"graze_ecology_phase": "thriving",
	}, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## An over-drawn, UNCULTIVATED forage patch: the Tile card's "Ecology" row must still render
## (the phase no longer gates cultivation, and the row shows on every patch regardless) as a
## WARN-amber "⚠ Stressed". Biomass is well below capacity, mirroring a patch foraged past its
## regrowth — **and well below the FOOD PEAK, which is what makes this the fixture the compose
## sheet's work predicate is judged on**: at any floor above `22 / 100` there is nothing standing
## above it, so a Cultivate composed here accrues nothing and states no turn estimate.
##
## **IT RE-PRICES ITS METER, because it re-dials the fraction.** Zeroing `cultivation_progress` off
## the reference tile's 0.6 without re-pricing left the absolutes behind at `30 / 50 work (0%)` —
## exactly the percentage-vs-absolute disagreement `BaseFx.price_plant_build` exists to make
## impossible.
static func stressed_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_ecology_phase"] = "stressed"
	tile["patch_biomass"] = 22.0
	# **THROUGH `BaseFx.unbuilt`, which drops the KEEPING as well as the meter.** It zeroed the
	# meter by hand and kept the reference tile's `0.5` keeping demand, which is a state the sim
	# cannot produce (`patch_unwinding_rung` answers `None` with both meters at zero). That helper
	# also restores the meter's ROT to nothing, which is the term the build's pace nets — a wild patch
	# holds no work, so there is none to lose (`docs/plan_standing_upkeep.md` §2.4).
	return BaseFx.unbuilt(tile)

## A fully-tended forage patch: the Tile card shows the "🌾 Tended Patch" badge (SIGNAL tint)
## plus an "Ecology" row, instead of the in-progress "Cultivation N%".
static func tended_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["x"] = 67
	tile["y"] = 11
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_is_cultivated"] = true
	tile["patch_ecology_phase"] = "thriving"
	# A TENDED patch reports every policy ceiling == per_worker_yield, so max-useful collapses to 1
	# worker regardless of policy — the stepper caps at 1 ("max 1 workers useful here").
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return RungFx.stamp_patch(BaseFx.seed_forage_rows(tile), HudComposeVocab.FORAGE_FORECAST_PREFIX)

## A hex with an occupant stack: 3 player bands + 1 herd, for the Occupants roster.
static func occupied_tile_fixture() -> Dictionary:
	return {
		"x": 58, "y": 24,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		"units": WorldFx.occupied_units_fixture(),
		"herds": [HerdFx.occupied_herd_only()],
	}

# ---- the compose sheet's FIT invariants -------------------------------------
