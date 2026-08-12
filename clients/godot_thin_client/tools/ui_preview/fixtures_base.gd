## Fixture primitives the land, herd and forage fixtures all build on.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const BARREN_PATCH_PER_BIOMASS := 0.01

# **THE HAY MEADOW'S TWO NON-FOOD RATES.** Sized so the meadow's two accounts BIND DIFFERENTLY, which
# is the fixture's whole job: at the seeded stock (room 40 above the food peak) the fodder ceiling is
# 0.20/turn against a crew that gathers 0.13 fodder/worker, so the CEILING binds on fodder; food
# gathers at 0.08/worker against a 0.60 ceiling, so LABOR binds there. A crew can therefore sit
# comfortably inside the patch's food regrowth while stripping its hay — which is only expressible
# because `min(w x per_worker, ceiling)` and the overdraw verdict are both applied PER ACCOUNT.

const FIXTURE_CAPACITY := 100.0

const FIXTURE_STOCK_FRACTION := 0.9

# ---- THE GROWTH TERMS THE FIXTURES PREDATE (slice 4b) -------------------------------------------
# `perWorkerBiomass` and `regrowthSamples` are wire fields no fixture written before them can carry,
# and the chart needs BOTH — without a curve it renders nothing at all, which would silently drop the
# instrument out of ~50 frames. So the adapter seeds them, in the SAME one place it converts the
# stances, and it is careful about which of the two webs it is standing in for.
#
# **THE HARNESS IS STANDING IN FOR THE SIM HERE, and that is the one place a growth model may be
# written in GDScript.** These constants are the shipped config's (`labor_config.forage.ecology` /
# `fauna_config.ecology`) and the shapes are the two the sim publishes: a patch is logistic lifted to
# its reseed floor and therefore NEVER negative, a herd declines at `collapse_rate` below its Allee
# threshold and therefore IS. A fixture that flattened that asymmetry would let the chart clamp a
# herd's crash to zero and still look right.

## Is this dict a HERD? A herd carries `species`; a forage patch carries `committed_species` and never
## a bare one, and the `patch_` prefix settles the tile_info case outright. It decides which growth
## SHAPE the seeded curve takes, so guessing wrong would hand a patch a herd's crash.
static func fixture_is_herd(src: Dictionary, prefix: String) -> bool:
	return prefix == "" and src.has("species")

## The FLOOR each retired stance stood for, so a converted raid table lands on the sim's own sampled
## floors (`snapshot::RAID_FORECAST_FLOOR_SAMPLES` = 0.0, 0.15, 0.30, 0.50, 0.80). Sustain is the food
## peak; the other three are the successively deeper draws they named.
const LEGACY_STANCE_FLOORS := {
	"sustain": 0.5, "surplus": 0.3, "deplete": 0.15, "eradicate": 0.0,
}

## **Seed the per-policy forage ROWS from the flat scalars this fixture already states** (#426).
##
## The wire now carries the tile's whole yield vector as one row per rung — six dicts keyed by policy,
## both the ceiling and the per-worker term, in all three accounts — and the six flat `patch_ceiling_*`
## scalars are deprecated slots. `SourceForecast.forecast_is_known` reads the ROW's PRESENCE as its
## "does the wire describe this source" witness, so a fixture that seeds only the scalars now correctly
## reads as *undescribed* and renders no forecast at all.
##
## Deriving the rows here rather than hand-writing them at ~30 fixture sites keeps each fixture naming
## its numbers ONCE, in the readable scalar form its comments explain, and makes the two
## representations unable to disagree. A state that wants a genuinely NON-derivable row (a
## fodder-paying tile) overwrites the relevant dict after calling this.
##
## Fodder defaults to 0 — the render-only-when-non-zero rule means every existing frame is then
## byte-identical, which is exactly what a reseeding pass must not disturb.
##
## **THE `patch_ceiling_*` KEYS IT READS ARE A FIXTURE-AUTHORING SHORTHAND, NOT WIRE KEYS, AND THIS
## ERASES THEM** (#426). The six flat scalars they are named after are retired `(deprecated)` wire
## slots that `MapView` no longer cross-refs and `SourceForecast` no longer reads — so a tile dict
## left carrying them would be a wire-shaped key with no wire behind it, and the next fixture author
## to reach for one would get silence rather than an error. Consuming them here keeps ~30 fixtures
## naming their numbers once, in the readable form their comments explain, while guaranteeing no
## fixture can hand the HUD a representation the sim stopped sending.
static func seed_forage_rows(tile: Dictionary) -> Dictionary:
	var per_worker := float(tile.get("patch_per_worker_yield", 0.0))
	# **A RE-SEED FALLS BACK TO WHAT IS ALREADY THERE**, and that is what makes the layered fixtures
	# work: most of them are `food_tile_fixture()` (already seeded) plus a few overrides plus a second
	# `seed_forage_rows`. Reading only the scalars would silently zero every account the second caller
	# did NOT restate.
	var peak_food := float(tile.get("patch_ceiling_sustain", 0.0))
	var peak_fodder := 0.0
	if tile.has("patch_provisions_per_biomass"):
		var prior_room := float(tile.get("patch_biomass", 0.0)) \
			- SourceForecast.FLOOR_FOOD_PEAK * float(tile.get("patch_carrying_capacity", 0.0))
		if peak_food <= 0.0:
			peak_food = float(tile["patch_provisions_per_biomass"]) * prior_room
		peak_fodder = float(tile.get("patch_fodder_per_biomass", 0.0)) * prior_room
	# **THE STOCK THE CEILING IS COMPOSED FROM.** A fixture states a ceiling; the wire states the terms
	# a ceiling is built out of, so this reverses the arithmetic the client now does — pinning each
	# fixture's authored `sustain` number to the FOOD PEAK, which is the honest mapping (Sustain took
	# the renewable yield; the peak is the floor that pays the most forever). At the seeded stock the
	# other two presets fall out at 2.25x and 0.25x of it.
	var capacity := FIXTURE_CAPACITY
	var biomass := FIXTURE_STOCK_FRACTION * FIXTURE_CAPACITY
	var room := biomass - SourceForecast.FLOOR_FOOD_PEAK * capacity
	tile["patch_carrying_capacity"] = capacity
	tile["patch_biomass"] = biomass
	# **A BARREN PATCH KEEPS ITS RATES AND LOSES ITS STOCK** — the dead-season case, and the whole of
	# what issue #426 turns on. Its per-biomass vector is a property of what GROWS there and stays
	# positive; what a dead season zeroes is the crew's throughput and the standing crop. Zeroing the
	# RATE instead would make the patch read as one the wire never described, which is the opposite of
	# the state.
	if peak_food <= 0.0 and peak_fodder <= 0.0:
		tile["patch_biomass"] = SourceForecast.FLOOR_FOOD_PEAK * capacity
		tile["patch_provisions_per_biomass"] = BARREN_PATCH_PER_BIOMASS
		tile["patch_fodder_per_biomass"] = 0.0
	else:
		tile["patch_provisions_per_biomass"] = peak_food / room
		tile["patch_fodder_per_biomass"] = peak_fodder / room
	# **THE TWO BUILD DIPS ARE FRACTIONS** (issue #442). `patch_ceiling_cultivate` /
	# `patch_ceiling_sow` remain the fixture-authoring shorthand — a fixture states the dip as the
	# absolute rate its comments explain — and this converts each to the wire's fraction form by
	# dividing by the food-peak ceiling, which is exactly what the old row was. A fixture that states a
	# fraction outright wins; a barren patch leaves it 0, i.e. "no build described here".
	for rung in SourceForecast.FORAGE_IMPROVEMENTS:
		var key := "patch_%s_build_fraction" % rung
		if not tile.has(key):
			var dip := float(tile.get("patch_ceiling_%s" % rung, 0.0))
			tile[key] = (dip / peak_food) if peak_food > 0.0 else 0.0
		tile.erase("patch_ceiling_%s" % rung)
	for policy in LEGACY_STANCE_FLOORS:
		tile.erase("patch_ceiling_%s" % policy)
	return tile

## ---- THE BUILD, PRICED IN WORK (`docs/plan_unit_costed_work.md` §8) ---------------------------
## `intensification_ladder.json`'s own `work_cost` for the two PLANT rungs. One unit is one
## worker-turn at the food peak with no gear, so the shipped 50 and 75 read themselves — and the
## pair is what makes the two rungs visibly different jobs rather than one meter filling at
## unexplained speeds. Stated here so every plant fixture prices its rungs from one place.
const PLANT_CULTIVATE_WORK_COST := 50.0

const PLANT_SOW_WORK_COST := 75.0

## The SIM's own estimate of what is left on the running build, at this fixture's crew, floor and
## kit. **The client computes none of it** — it holds neither the crew's output nor the floor
## multiplier nor the kit's contribution — so a fixture states an ANSWER here exactly as it states a
## yield forecast. `SourceForecast.BUILD_TURNS_NO_ESTIMATE` is the other reading, and it renders as
## no line at all.
const BUILD_TURNS_REMAINING := 11

## **NO PLANT ITEM DECLARES THE BUILD STAT YET** (issue #539 is the hoe), so a plant build's gear
## contribution is honestly `0` and the tile card renders no gear line. The ANIMAL fixtures carry a
## real one — `husbandry_gear` ships 8.5 per equipped keeper — which is where that readout is judged.
const PLANT_BUILD_WORK_FROM_GEAR := 0.0

## Price a tile's two plant rungs in WORK, deriving each meter's `work_done` from the fraction the
## fixture already states — so a fixture that re-dials its progress cannot end up with a percentage
## and an absolute that disagree, which is the one thing this readout exists to make visible.
static func price_plant_build(tile: Dictionary, turns: int = BUILD_TURNS_REMAINING) -> Dictionary:
	tile["patch_cultivation_work_cost"] = PLANT_CULTIVATE_WORK_COST
	tile["patch_cultivation_work_done"] = \
		float(tile.get("patch_cultivation_progress", 0.0)) * PLANT_CULTIVATE_WORK_COST
	tile["patch_field_work_cost"] = PLANT_SOW_WORK_COST
	tile["patch_field_work_done"] = \
		float(tile.get("patch_field_progress", 0.0)) * PLANT_SOW_WORK_COST
	tile["patch_build_turns_remaining"] = turns
	tile["patch_build_work_from_gear"] = PLANT_BUILD_WORK_FROM_GEAR
	return tile

static func food_tile_fixture() -> Dictionary:
	var tile := {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		# Fertile steppe: low drain → "Hospitable" (green Tile-card row).
		"habitability": 0.01,
		# Mid-latitude ~18° → "Temperate" climate band (neutral Tile-card row).
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		# A discovered Wondrous Site on this tile → the Tile card shows a "Site: …" line.
		"site_name": "Verdant Basin",
		# Forage patch being worked toward cultivation → the Tile card's "Cultivation 60%" row.
		"patch_cultivation_progress": 0.6,
		"patch_is_cultivated": false,
		"patch_has_owner": true,
		"patch_owner": 0,
		"patch_ecology_phase": "thriving",
		# Standing forage stock vs the patch ceiling (sim default capacity 120) → the Tile card's
		# "Forage biomass 84 / 120" row, the patch counterpart to a herd's Biomass row.
		"patch_biomass": 84.0,
		"patch_carrying_capacity": 120.0,
		# Pre-commit yield forecast (food/turn at THIS biomass, exported at output_multiplier 1.0).
		# Sustain's ceiling admits ceil(0.96 / 0.32) = 3 useful foragers — below band 821's 10 idle
		# workers, so the Foragers stepper caps at 3 and shows the "max 3 workers useful here" note.
		# The higher-policy ceilings admit 6 / 9 / 15, so switching policy visibly moves the cap.
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
		# The Cultivate INVESTMENT rung: while the patch is being prepared it pays only a fraction of
		# its Sustain ceiling (the dip the player is buying with), then flips to the tended yield.
		# Both are food/turn at output_multiplier 1.0, like the ceilings above.
		"patch_ceiling_cultivate": 0.24,
		"patch_tended_yield": 1.20,
		# THE BUILD CREWS (#442) — `intensification_ladder.json`'s own `crew_needed` for the two plant
		# rungs (tended 2, field 3), which is what the compose stepper FLOORS its cap on. Not decoration:
		# the dip shrinks the ceiling the cap divides, so without a crew a Cultivate composed here caps
		# at ONE forager while the sim asks for two — the exact disagreement the pair of them fixes.
		"patch_cultivate_crew_needed": 2,
		"patch_sow_crew_needed": 3,
		# THE NEGLECT GRACE (#442) — the countdown to this rung reverting. The reference patch IS being
		# worked (a crew is cultivating it), so it reads the plant:tended rung's full `grace + 1` = 3:
		# "walk away and you have this long". `has_neglect_grace` is what makes the number readable at
		# all — a wild patch would ship `false`, not a zero.
		"patch_has_neglect_grace": true,
		"patch_neglect_grace_remaining": 3,
		# Plant RUNG 3 — the Field + the Sow verb. This reference tile is ordinary prairie steppe:
		# rich enough to forage, but it will NOT take seed (rung 3 moves seed, it cannot fertilize or
		# irrigate), so the sim's `sow_site_refusal` verdict rides here and the Sow option is gated
		# with the reason. Only ~1% of a real map is sowable, so REFUSED is the common case and is
		# deliberately the default fixture; `ForageFx.sowable_tile_fixture` is the exception.
		"patch_field_progress": 0.0,
		"patch_is_field": false,
		"patch_ceiling_sow": 0.0,
		"patch_field_yield": 0.0,
		"patch_sow_site_refusal": "too_dry",
		# WHAT GROWS HERE (flora roster F1) — the named plants this tile's forage capacity decomposes
		# into. Wire order (share DESC, then species key ASC) is preserved verbatim by the card.
		# The shares are chosen so NAIVE rounding totals 101% (46 + 30 + 25): the card must absorb the
		# remainder into the largest share and render 45 / 30 / 25 — this fixture IS the rounding test.
		# `can_cultivate` / `can_sow` are SPECIES-GLOBAL rung legality (flora roster S1), deliberately
		# mixed here so the crop picker has a greyed row in every frame: Oak Mast climbs nothing (a wild
		# harvest forever) and Ground Nut tends but never sows. `*_yield_ratio` is what committing PAYS
		# relative to gathering wild, on the CORRECTED scale (the sim's ratio omitted
		# `tended_regrowth_gain`, understating every Cultivate figure by exactly 2×) — so above 1.0 is
		# now the NORM and these read: a strong crop (2.40×), an honest middle one (1.70×) and the 0
		# sentinel on the greyed rows. `*_payoff` is the same rung's provisions/turn committed to THAT
		# species, and it is what the compose sheet's "→ then" term quotes once a crop is picked: the two
		# rows differ (1.20 vs 0.85), which is what makes the selection visibly move the forecast.
		"patch_composition": [
			{"species": "wild_grain", "role": "staple", "display_name": "Wild Grain", "share": 0.455,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.295,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.85, "sow_payoff": 0.0},
			{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.25,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
		# The GRAZE (pasture) layer — the ANIMAL-edible twin of the forage patch above (Grazing Phase
		# 2a). Prairie steppe is the reference pasture: capacity 240, standing full, hence Thriving.
		# Rendered as the `Grazing` row directly under `Foraging` and its basket, so the card states the
		# two facts side by side: what HUMANS can eat here, and what ANIMALS can eat here.
		"graze_biomass": 240.0,
		"graze_capacity": 240.0,
		"graze_ecology_phase": "thriving",
	}
	# The reference plant tile prices BOTH its rungs, because the wire does: `workCost` is published
	# whether or not a build is in flight, which is what lets the compose sheet quote a rung before
	# the player commits to it.
	return seed_forage_rows(price_plant_build(tile))
