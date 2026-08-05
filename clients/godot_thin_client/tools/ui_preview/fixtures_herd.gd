## Herd, hunt and raid fixtures.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")

const HUNT_FORECAST_PARTY := 4
# The dialed-in hunter count for the LOCAL hunt preview states — deliberately dialed PAST every
# ceiling in them, so the stepper clamps it back to the sheet's own whole-animal carry cap exactly as
# it would for the player (`LOCAL_HUNT_CAPPED_CREW`; these frames render 3 hunters, not 6). The point
# survives the clamp: even the clamped crew out-carries every policy ceiling here, so the HERD (not
# the hunters) is still the binding constraint — which is exactly the case where the per-turn yield
# preview earns its keep.

const BOAR_RAID_ANIMALS := [5, 8, 8, 8, 8, 8, 8, 8]

const BOAR_RAID_TURNS := [7, 8, 4, 3, 3, 3, 3, 3]

const BOAR_FOOD_PER_ANIMAL := 4.0
# The Thunder Mammoth's food quantum — big enough that no fieldable party can carry a whole one, which
# is what makes `_partial_waste_mammoth` the WASTE fixture: a party of `w` hauls ~`w` of the 16 and
# rots the rest. Named here rather than left a local because the waste assertion computes the same
# percentage the readout prints, and two spellings of one quantum would drift.

const RAID_TRADE_PER_ANIMAL := 0.5
# The DISTANCE frames' raid (`hunt_distance_herd`, the reference Red Deer at 2.0 food/animal): a party
# of `i+1` lands `DISTANCE_RAID_ANIMALS[i]` animals in `DISTANCE_RAID_TURNS[i]` HUNTING turns. Those
# frames open at the seeded party of 1, so the first cell is the one they render; the plateau at 3
# animals-taken keeps the party stepper's max-useful cap meaningful rather than unbounded. The turns sit
# well inside a band's `expedition_viability_warn_turns`, so the trip verdict reads OK there and the
# slow/long raids stay the business of the fixtures built for them.

const DISTANCE_RAID_ANIMALS := [3, 5, 6, 6, 6, 6, 6, 6]

const DISTANCE_RAID_TURNS := [9, 7, 6, 6, 6, 6, 6, 6]
# The `herd_hunt_raid_travel` frame's two halves, named so the split assertion states the arithmetic
# rather than a pair of literals: `_raid_travel_band` sits 8 tiles from the (66,10) boar and moves 2
# tiles a turn, so the round trip is ceil(2 × 8 / 2), and the boar's own 2-party cell fills in 8
# hunting turns (`BOAR_RAID_TURNS[1]`). 16 total, inside the band's 20-turn warn line.

const PEN_UPKEEP_RED_DEER := 1.74

const PEN_FED_STARVING := 0.40
# The herder-deficit state's staffing PAIR (`herd_corral_under_herded`). The growing corral needs 2
# herders every turn to hold its tameness while only 1 is staffed, and the two numbers must DISAGREE
# for the deficit to render at all — so the fixture's `herders_needed`, the band's dialed-down hunt
# assignment and the auto-max assertion all read them from here rather than repeating bare 1s and 2s.

## **The three "already built" remedy needles went with the gate reasons they pinned** (issue #442):
## a completed rung is a static DONE LABEL now, not a greyed picker button, so there is no dead end to
## explain and nothing for a needle to find. `IMPROVEMENT_DONE_LABELS` is what those frames assert on.
## The herder crew on the fully-tamed herd: the herd's `herders_needed` pair AND the workers the
## standing Tame assignment staffs, so the two cannot disagree about how many hands are on it.
const TAMED_HERD_CREW := 4

const HERDERS_NEEDED_KEY := "herders_needed"

const HERDERS_NEEDED_IF_MANAGED_KEY := "herders_needed_if_managed"

## Deep-scan bound. Fixtures are trees, but a bound turns a future self-referencing one into a stop
## rather than an infinite walk.
const HERD_SCAN_MAX_DEPTH := 8

## The herd the distance-aware states select — the same (66,10) herd but a NON-food tile_info, so the
## Tile card drops its "Assign foragers" block and the hunt button + distance hint sit in-frame.
##
## **IT CARRIES A RAID TABLE, and without one the expedition frames judge nothing about the trip.**
## `herd_fixture` publishes the BAND's flow ceilings and no `hunt_trip_estimates`, so every expedition
## sheet opened on it answered `available: false` and rendered no forecast at all — a state a live herd
## cannot be in (the sim exports an estimate row for every huntable herd) and the one state in which
## every claim about the trip readout would pass vacuously. The counts are the reference deer's own
## `food_per_animal` 2.0 through `raid_estimate_table`, so the payload is `animals × 2` food beside
## `animals × RAID_TRADE_PER_ANIMAL` trade — both accounts positive, which is what makes the
## zero-account frame beside it (`_partial_waste_mammoth`, no trade at all) a real contrast.
static func hunt_distance_herd() -> Dictionary:
	var herd := herd_fixture()
	herd["tile_info"] = plain_herd_tile_info()
	herd["hunt_trip_estimates"] = raid_estimate_table(
		DISTANCE_RAID_TURNS, DISTANCE_RAID_ANIMALS, float(herd["food_per_animal"]))
	return herd

## A Wild Boar carrying the server's MEASURED raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter →
## 5 animals / 7 turns, 2 → 8 / 8, 3 → 8 / 4. `animalsTaken` plateaus at 8 (party 2), so max-useful = 2.
## The frame the "delivers ≈5 Wild Boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged
## on. `food_per_animal` = 4 so the readout appends the food total (~20 at 5 animals, ~32 at 8).
static func raid_boar_herd() -> Dictionary:
	var herd := assign_preview_herd("game_boar_04", "Wild Boar", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = BOAR_FOOD_PER_ANIMAL
	herd["hunt_trip_estimates"] = raid_estimate_table(
		BOAR_RAID_TURNS, BOAR_RAID_ANIMALS, BOAR_FOOD_PER_ANIMAL)
	return herd

## A raid estimate TABLE from a per-party Sustain (turns, animals) pair (index i = a party of i+1). The
## deeper policies raid to a lower floor, so they take MORE animals (Surplus < Deplete < Eradicate) — the
## per-policy ASCENDING the picker buttons read. **Eradicate DELIVERS** — it takes the most animals and
## banks the whole-stock windfall (issue #337 redefined `delivers_food`: it means the QUARRY IS EDIBLE,
## not "this rung is a denial mission", and a boar is edible on every rung). Every cell also carries the
## trade twin, since a hunt pays a VECTOR: `delivers_trade` + `delivered_trade = animals × tpa`.
## The per-policy bumps are illustrative fixture data; the live sim exports the real per-floor counts.
static func raid_estimate_table(turns_row: Array, animals_row: Array, fpa: float,
		tpa: float = RAID_TRADE_PER_ANIMAL,
		bound: String = SourceForecast.TRIP_BOUND_PACK_FULL) -> Dictionary:
	var table := {}
	for i in animals_row.size():
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × fpa, waste 0.
		# delivered_food is the PRIMARY payload the client headlines + the field the max-useful scan and
		# "too lean" test read — every cell must carry it.
		for entry in [["sustain", 0], ["surplus", 2], ["deplete", 3], ["eradicate", 5]]:
			var animals: int = base + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * tpa, "wasted_food": 0.0,
				# **WHICH STOP ENDS THIS SAMPLED TRIP** (`docs/plan_hunt_through_combat.md` §5.2). The
				# sim writes it on every row, so a fixture omitting it would be a herd no live server can
				# produce — and every bound-clause assertion would pass vacuously against the one state
				# the client renders for a snapshot that predates the field. `pack_full` is the honest
				# default for a CLEAN raid (whole kill hauled, nothing left standing at the floor); the
				# floor-bound contrast is `band_expedition.gd`'s `_floor_bound_raid_herd`.
				SourceForecast.TRIP_BOUND_KEY: bound,
			}
	return table

## **THE SIM'S `fauna::quantise_animal_take`, RESTATED IN FOOD** — the harness's oracle for what a
## hunting crew is actually paid, so the assertions compare the sheet against the SIM's composition
## rather than against itself (both halves of the sheet dipped together would satisfy any test the
## sheet makes of its own numbers).
##
## Food and biomass differ only by the species' constant provisions rate, which divides out of every
## comparison the sim makes — `collection / body_mass` is `collection_food / food_per_animal` — so this
## is the same arithmetic in cheaper units. `max(1.0, carryable)` is the load-bearing line: a crew that
## cannot carry one whole animal still kills one and wastes the difference.
##
## **`engaged` IS THE THIRD BOUND** (`docs/plan_hunt_through_combat.md` §2) — the whole animals the
## party can bring into CONTACT, which `quantise_animal_take` mins in beside the affordable and the
## carryable. It defaults to `INF`, the reading the sim itself passes for a pen and the one the wire's
## `NO_ENGAGEMENT_STAGE` stands for, so every caller that predates the engagement stage is unchanged.
static func hunt_take_oracle(collection: float, ceiling: float, food_per_animal: float,
		engaged: float = INF) -> Dictionary:
	var affordable := floorf(ceiling / food_per_animal)
	if affordable < 1.0:
		return {"delivered": 0.0, "wasted": 0.0}
	var killed := minf(minf(affordable, maxf(1.0, floorf(collection / food_per_animal))), engaged)
	var killed_food := killed * food_per_animal
	var carried := minf(killed_food, collection)
	return {"delivered": carried, "wasted": killed_food - carried}

## A NON-food hex under the herd, so the Tile card drops its "Assign foragers" block and the herd's
## assign controls (stepper + policy + forecast + button) sit fully in-frame.
static func plain_herd_tile_info() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A forecast herd (carrying BOTH sim-exported per-policy ceiling tables) as a SELECTED herd — i.e. on
## a plain tile, the way `show_herd_selection` receives it — rather than as a hovered hex.
static func assign_preview_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int, surplus_trip_turns: int,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	var herd := forecast_herd(id, species, phase, sustain_ceiling, trip_turns, surplus_trip_turns,
		sustain_animals, surplus_animals)
	herd["huntable"] = true
	herd["tile_info"] = plain_herd_tile_info()
	return herd

## A herd carrying the two DIFFERENT things the sim exports for the two DIFFERENT actors:
##   `hunt_policy_ceilings` — the BAND's renewable FLOW ceiling {policy → provisions/turn}. The local
##       hunt preview is pure arithmetic over it (Sustain's entry IS the herd's sustainable yield).
##   `hunt_trip_estimates` — the sim's forward-SIMULATED expedition trip answers, keyed
##       `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, delivers_trade, …}`. An
##       expedition's trip is NOT a rate division (on Surplus/Deplete the ceiling is a *stock* the party
##       strips in a turn or two, then it crawls at the regrowth trickle), so the client looks the answer
##       up and does no math. `turns_to_fill == 0` → won't fill within the horizon; `delivers_food ==
##       false` says the QUARRY IS INEDIBLE (#337), and only `delivers_food AND delivers_trade` both
##       false is a denial mission — the raid banks whichever half the species pays.
## **A ROW THAT DELIVERS NOTHING CANNOT WEAR A PARTY-SIDE BOUND.** `pack_full` and `fill_target` both
## require a LOAD, and a load is a delivery — so the sim never pairs either with an empty payload, and
## a fixture that did would be a herd no live server can produce. It would also be invisible: the
## sheet's empty-raid refusal is keyed off `bound`, so such a row falls to the UNATTRIBUTED entry and
## every assertion about *which* refusal is rendered testifies about nothing.
##
## What a zero row in these families means is the herd standing AT ITS FLOOR — except at a floor of
## `0`, where the sim's own `surplus_spent` test cannot fire (it is gated on `floor > 0`) and a raid
## that lands nothing is one whose quarry dies out under it. **The party-side empty raid — a herd with
## real surplus a party cannot kill — is a different fixture entirely** (`hunt.gd`'s
## `_unkillable_aurochs_herd`), because it is a different fact about a different actor.
static func clean_raid_bound(animals: int, stance: String, delivering: String) -> String:
	if animals > 0:
		return delivering
	return SourceForecast.TRIP_BOUND_FLOOR \
		if float(BaseFx.LEGACY_STANCE_FLOORS.get(stance, 0.0)) > 0.0 \
		else SourceForecast.TRIP_BOUND_HERD_LOST

## `trip_turns` is the simulated turns-to-fill for the 4-worker party these states dial in.
static func forecast_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int = 0, surplus_trip_turns: int = 0,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × food_per_animal
	# and nothing rots. `delivered_food` is now the PRIMARY payload the client headlines (and the field
	# the "too lean" test / max-useful scan read), so every fixture cell must carry it; a partial-with-
	# waste cell is built explicitly (see `_partial_waste_mammoth`).
	var fpa := 2.0
	var sustain_delivered := float(sustain_animals) * fpa
	var surplus_delivered := float(surplus_animals) * fpa
	return {
		"id": id,
		"label": "%s (%s)" % [species, id],
		"species": species,
		"size_class": "big",
		"huntable": true,
		"ecology_phase": phase,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# One animal's worth of FOOD (provisions), `HerdTelemetryState.foodPerAnimal` — drives the
		# kill-rhythm on the local-hunt preview (food ÷ food). Matches `fpa` above (the clean delivered).
		"food_per_animal": fpa,
		# A LIVE herd carries BOTH forecast field sets, so this fixture must too (they were split
		# across two disjoint fixtures once, which hid every interaction between them):
		#   • `per_worker_yield` + the `hunt_policy_ceilings` table, which drive the shared
		#     `SourceForecast.forecast_inputs` → cap + "Expected yield" / "Preparing → then" row, and
		#   • `hunt_trip_estimates` below (the sim's forward-simulated EXPEDITION trip answers).
		# Per-worker matches the band's `hunt_per_worker_provisions` (0.8) and the ceilings ARE the
		# band ceilings, because the sim exports one hunt model — the two paths must agree.
		"per_worker_yield": 0.8,
		# Eradicate's ceiling was `0.0` — the retired "denial yields nothing" premise written as a number,
		# which rendered the rung's picker face as `💀 +0.00` and its local preview as a zero take. #337
		# pays every rung the species' vector, and Eradicate empties the standing stock, so it is the
		# DEEPEST floor and frees the most: 8× the Sustain flow here.
		"hunt_policy_ceilings": {
			"sustain": sustain_ceiling,
			"surplus": sustain_ceiling * 4.0,
			"deplete": sustain_ceiling * 2.0,
			"eradicate": sustain_ceiling * 8.0,
		},
		# The trade half of each rung's ceiling (issue #337), a fixed fraction of the food one.
		"trade_per_animal": fpa * 0.15,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": sustain_ceiling * 0.15,
			"surplus": sustain_ceiling * 4.0 * 0.15,
			"deplete": sustain_ceiling * 2.0 * 0.15,
			"eradicate": sustain_ceiling * 8.0 * 0.15,
		},
		"hunt_trip_estimates": {
			"sustain:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": sustain_animals,
				"delivered_food": sustain_delivered,
				"delivered_trade": float(sustain_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(sustain_animals, "sustain",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			"surplus:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "surplus",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			"deplete:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "deplete",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			# Eradicate DELIVERS (issue #337): `delivers_food` says the quarry is EDIBLE, not that the
			# rung is a denial mission, and an Eradicate raid banks the whole-stock windfall.
			"eradicate:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
				# `turns_to_fill == 0` IS the horizon case — the raid was still delivering when the
				# projection ran out — and the pairing is the sim's own: the two must move together, or
				# the verdict names a stop the turn count denies.
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "eradicate",
					SourceForecast.TRIP_BOUND_HORIZON),
			},
		},
	}

## A herd mid-TAME on a pen-ceiling species: the 🐾 Tame rung is available and selected, and the herd's
## OWN meter reads 40% (`domestication`). It is the base of the taming family below; the TWO-METER
## SPLIT is staged on its fully-tamed variant, since only a retired Tame lets the gated Corral — the
## bridge between the two meters — render at all (see `two_meter_split`).
static func taming_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["husbandry_ceiling"] = "pen"
	fixture["domestication"] = 0.4
	fixture["ecology_phase"] = "thriving"
	fixture["tile_info"] = compact_herd_tile_fixture()
	return fixture

## THE BOTH-PRODUCTS INVESTMENT PAYOFF (issue #397) — a Wild Boar, edible AND worth its hide/bristles,
## on a pen-ceiling species so BOTH investment rungs are offered. Its four extractive rungs already pay
## a pair; the numbers here are the OTHER pair, the one the payoff faces render:
##   Tame   → `pastoral_yield` 1.48 food + `pastoral_trade` 0.37 trade  ⇒ `→ 1.48 food · 0.37 trade`
##   Corral → `corral_yield`   2.95 food + `corral_trade`   0.74 trade  ⇒ `→ 2.95 food · 0.74 trade`
## The food halves are the boar figures from the issue's own report (where the faces read `→ 1.48 food`
## and `→ 2.95 food` and the trade halves were dropped); each trade half is a quarter of its food half,
## the boar's hide-to-meat ratio, so the pair ascends together up the ladder exactly as the extractive
## caps do. Ordering is the ladder's: Sustain (0.90) < Tame (1.48) < Corral (2.95).
## `domestication` stays mid-ladder (0.4) because Tame RETIRES from the picker at full domestication
## while Corral is gated below it — the only way both rungs appear at once, with Corral greyed and
## still wearing its payoff.
static func investment_pair_boar_herd() -> Dictionary:
	var fixture := taming_herd_fixture()
	fixture["id"] = "game_boar_11"
	fixture["label"] = "Wild Boar (game_boar_11)"
	fixture["species"] = "Wild Boar"
	fixture["size_class"] = "medium"
	fixture["pastoral_yield"] = 1.48
	fixture["pastoral_trade"] = 0.37
	fixture["corral_yield"] = 2.95
	fixture["corral_trade"] = 0.74
	return fixture

## A still-WILD but tameable herd (pen ceiling) for the taming-startup-lag guard. It is NOT yet managed,
## so its OWNERSHIP-GATED `herders_needed` is 0 — but its ownership-INDEPENDENT would-be herder crew
## (`herders_needed_if_managed`, from biomass) is 10, set DELIBERATELY ABOVE this herd's Sustain
## take-useful (7, driven by the carry model) so the "no leak" companion is meaningful: composing Tame
## floors the cap UP to the 10-crew, while composing the extractive Sustain must stay at its own 7 — a
## crew-floor leak into Sustain would instead bump it to 10, which the companion asserts does NOT happen.
## A herd whose TAMING IS FINISHED — `domestication` at the sim's completion threshold, which RETIRES
## ◎ Tame (its per-source meter is full, so the improvement control shows it as the DONE state) and
## makes 🐄 Corral the rung on offer. It is managed at that point, so it carries a real herder crew
## through `set_managed_herders` — the field pair every herd fixture owes the frame guard.
##
## **It is also the only shape on which a Corral GATE can render**, which is why `two_meter_split`
## stages it: a gate reason needs the rung to be the one on offer, and Corral only ever is once Tame
## has retired.
static func fully_tamed_herd_fixture() -> Dictionary:
	var fixture := taming_herd_fixture()
	fixture["domestication"] = SourceForecast.DOMESTICATION_COMPLETE
	set_managed_herders(fixture, TAMED_HERD_CREW)
	return fixture

## Set BOTH herder counts on a MANAGED herd fixture. The sim exports them EQUAL there (see the
## field-pair guard `_guard_herd_fields` in `ui_preview.gd`), and setting them one at a time is
## precisely the
## mistake the guard exists to catch — so managed fixtures set them together, through this.
## A still-WILD but tameable herd is the one case where they differ and writes them by hand
## (`_tame_worker_cap_herd_fixture`: gated 0, would-be 10).
static func set_managed_herders(fixture: Dictionary, needed: int) -> void:
	fixture[HERDERS_NEEDED_KEY] = needed
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = needed

## The world's herd list (Main pushes snapshot["herds"]). Named because the turn-orb starving-pen
## state swaps in its own list and must restore this one.
static func world_herds_fixture() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120, "ecology_phase": "stressed", "food_per_animal": 2.0},
	]

static func herd_fixture() -> Dictionary:
	return {
		"id": "game_deer_07",
		"label": "Red Deer (game_deer_07)",
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# Ecological carrying capacity + grazing range (Grazing Phase 2b-iii): the numbers that explain
		# the herd's size. Big game roams a radius-1 range (7 tiles); on good steppe it caps ~2150, well
		# above this herd's 820 biomass, so the drawer reads the healthy "Herd: 8 / 22" pair with no
		# overgrazing warning. The dedicated grazing states below dial in overgrazed / small-game.
		"carrying_capacity": 2150.0,
		# ONE animal's biomass — what turns both numbers above into the ANIMAL counts the drawer and the
		# floor flag state. **Pinned to the fixture's own `food_per_animal`**, not chosen freely: the
		# sim's identity is `food_per_animal = body_mass × provisions_per_biomass`, so at the deer's
		# 0.02 this must be 2.0 / 0.02 = 100 or the fixture asserts against a herd that could not exist.
		"body_mass": 100.0,
		"graze_range_radius": 1,
		"route_length": 3,
		# One animal's worth of FOOD (provisions) — `HerdTelemetryState.foodPerAnimal`, the exact key the
		# decoder now emits. The kill-rhythm divides it by the food rate (both provisions): 2.0
		# food/animal vs a 0.90/turn Sustain take reads "≈1 Red Deer / 3 turns".
		"food_per_animal": 2.0,
		# Pre-commit yield forecast (food/turn at this herd's biomass, at output_multiplier 1.0).
		# Sustain admits ceil(0.90 / 0.30) = 3 useful hunters, below the reference band's 7 assignable
		# (3 idle + the 4 it already has on this herd), so the Hunters stepper caps at 3 with the
		# "max 3 workers useful here" note.
		"per_worker_yield": 0.30,
		# The two INVESTMENT rungs' PAYOFFS — the food/turn each rung pays ONCE prepared (the pastoral
		# MSY after taming, the pen's sustained rate once built), NOT the during-build dip. Ordered
		# Sustain (0.90) < Tame (1.20) < Corral (1.50) so the picker's `→ +Y/turn` payoff buttons read
		# as an ascending ladder, both clearly above Sustain's `up to +0.90/turn` cap.
		"corral_yield": 1.50,
		"pastoral_yield": 1.20,
		"corral_progress": 0.0,
		# EVERY ceiling — the four extractive rungs plus the Tame/Corral DIPS — rides this ONE list;
		# the herd has no flat `ceiling*` scalars on the wire any more (deprecated schema slots). The
		# sim exports a row for every one of the six `FollowPolicy::HUNT_POLICIES`, so this is the
		# shape the decoder produces and where `SourceForecast.forecast_inputs` reads every herd ceiling.
		"hunt_policy_ceilings": {
			"sustain": 0.90,
			"surplus": 1.80,
			"deplete": 2.70,
			"eradicate": 4.50,
		},
		# **THE TWO BUILD DIPS, AS FRACTIONS** (issue #442) — they were `tame` / `corral` ROWS of the
		# list above, each the 0.23 a builder took because Sustain (0.90) was the only stance a builder
		# could hold. 0.23 / 0.90 is that same dip stated as the factor it always was, and it now
		# multiplies WHICHEVER stance the crew holds: a Deplete builder takes 2.70 x 0.256 = 0.69.
		"tame_build_fraction": 0.23 / 0.90,
		"corral_build_fraction": 0.23 / 0.90,
		# The TRADE half of the same list (issue #337) — the decoder fills both dicts in one pass over
		# the one wire list, so a fixture that carries the food rows must carry these or the picker
		# under-reports what the rung pays.
		"trade_per_animal": 0.30,
		"per_worker_trade": 0.05,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.14,
			"surplus": 0.27,
			"deplete": 0.41,
			"eradicate": 0.68,
		},
		"tile_info": BaseFx.food_tile_fixture(),
	}

## A DEADLY-TO-HUNT herd (Predators Phase 0): a woolly mammoth — high attack (8) and high ferocity
## (0.9, it fights back), but aggression 0 (a grazer never attacks unprovoked). Its drawer shows high
## Attack + Fights back bars and an EMPTY Aggressive bar — the split that proves strength ≠ danger.
## Compact tile so the component/husbandry rows land in-frame.
static func deadly_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["id"] = "game_mammoth_02"
	fixture["label"] = "Woolly Mammoth (game_mammoth_02)"
	fixture["species"] = "Woolly Mammoth"
	fixture["husbandry_ceiling"] = "wild"
	fixture["attack"] = 8.0
	fixture["defense"] = 12.0
	fixture["ferocity"] = 0.9
	fixture["aggression"] = 0.0
	fixture["tile_info"] = compact_herd_tile_fixture()
	return fixture

## A WILD-ceiling herd (Grazing 2d-δ): hunt-only. The drawer shows NO husbandry track (no
## domestication / corral / pen rows) — just the "Wild game — hunt only" hint — and the hunt policy
## picker drops the Corral rung.
static func wild_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["husbandry_ceiling"] = "wild"
	fixture["tile_info"] = compact_herd_tile_fixture()
	return fixture

## A compact NON-food tile_info (like the domesticated/hunt-distance herds) so the tile card stays
## short and the herd drawer's husbandry rows land in-frame rather than below the dock scroll fold.
static func compact_herd_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## The stressed herd sharing the occupied hex (amber ecology dot).
static func occupied_herd_only() -> Dictionary:
	return {
		"id": "game_bison_02",
		"label": "Steppe Bison (game_bison_02)",
		"species": "Steppe Bison",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "stressed",
		"domestication": 0.0,
		"biomass": 240.0,
		"x": 58, "y": 24,
	}

static func collapsing_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["biomass"] = 96.0
	fixture["ecology_phase"] = "collapsing"
	fixture["domestication"] = 0.0
	return fixture

## A compact NON-food tile_info (like the corral fixtures) so the Tile card stays short and the herd
## drawer's Biomass (current/max) / Range (+ overgrazing) rows land in-frame rather than below the fold.
static func compact_herd_tile() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A HEALTHY grazing herd (Grazing Phase 2b-iii): big game (radius-1 range → "Range: 7 tiles") whose
## biomass sits below the K its range supports, so the merged "Biomass: 1480 / 2150" current/max pair
## reads current < max with NO overgrazing warning. domestication 0 keeps the frame focused on the rows.
static func grazing_healthy_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 1480.0
	fixture["carrying_capacity"] = 2150.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = compact_herd_tile()
	return fixture

## A FULLY TAMED, not-yet-penned herd with no pen started, on the same compact tile as the corral-ready
## one — the ONE shape that can put a GATED 🐄 Corral on screen (issue #442).
##
## It was a still-wild herd (domestication 0.4) while a gated rung was a greyed button of the policy
## picker, and every rung showed at once. The improvement control offers the source's NEXT rung, so a
## part-tamed herd is offered 🐾 Tame and Corral is not rendered at all — which quietly emptied both
## corral-gate frames. Retiring Tame (a full meter) is what makes Corral the rung on offer; the only
## thing left that can gate it is the faction's PENNING, which is exactly the knowledge bridge the two
## frames document. The SOURCE half of `RungGates.hunt_gates`' Corral reason is consequently
## unreachable in this control now — the moment it would apply, Tame is offered instead, and a
## checkbox is a better remedy than a sentence.
static func corral_locked_herd_fixture() -> Dictionary:
	var fixture := corral_ready_herd_fixture()
	fixture["corral_progress"] = 0.0
	return fixture

## A fully-domesticated herd whose pen is HALF-BUILT (not yet corralled): the Corral investment rung
## is available (knowledge + domestication both satisfied) and under way, so the hunt picker offers
## 🐄 Corral and the drawer reads "Corral: Building 40%". Compact non-food tile_info (like the
## domesticated fixture) so the Tile card stays short and the drawer rows land in-frame.
static func corral_ready_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["domestication"] = 1.0
	fixture["corralled"] = false
	fixture["corral_progress"] = 0.4
	# `pen_upkeep` is the feed this pen WOULD demand once built (the sim projects it at the herd's
	# current biomass, on the same basis as `corral_yield`) — so the pre-commit row can quote the
	# real running cost at the moment the player decides, rather than saying "before feed".
	fixture["pen_upkeep"] = 0.34
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

static func domesticated_herd_fixture() -> Dictionary:
	var fixture := herd_fixture()
	fixture["domestication"] = 1.0
	# A fully-domesticated herd is penned: the drawer adds a "🐄 Corralled" row.
	fixture["corralled"] = true
	# A PENNED herd is a managed population — it eats from its keeper's larder every turn. Fully fed
	# here (`pen_fed_fraction` 1.0), so the drawer reads the healthy "🐄 Corralled" badge plus the
	# amber "Pen feed: -1.74 /turn" standing debit.
	fixture["pen_upkeep"] = PEN_UPKEEP_RED_DEER
	fixture["pen_fed_fraction"] = 1.0
	# Grazing 2d-γ — a radius-1 pen on POOR footprint: its fenced land covers NONE of the feed
	# (`pen_pasture_fraction` 0.0), so the whole GROSS demand falls on the FOOD larder as the net bill
	# (`pen_larder_bill` == gross, no hay). Feed-split reads "Fed by pasture 0% · larder 1.7 food/turn".
	# Invariant: gross × pasture(0) + hay(0) + larder(1.74) == gross(1.74).
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.0
	fixture["pen_larder_bill"] = PEN_UPKEEP_RED_DEER
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	# Compact NON-food tile_info (like the hunt-distance herd) so the tile card stays short and
	# the drawer's Husbandry + Corral rows land in-frame rather than below the dock scroll fold.
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## The SAME penned herd, STARVING: its keeper paid only 40% of the 1.74/turn feed, so the herd is
## shrinking (`pen.starve_shrink_rate × (1 − fed) × biomass`) every turn and its yield with it. The
## drawer must say so loudly — the Corral row drops its badge for a red "⚠ Starving — 40% fed", and
## the Pen feed row names the shortfall. Biomass is down from the fed fixture's 820 to show the herd
## has actually lost ground.
static func starving_pen_herd_fixture() -> Dictionary:
	var fixture := domesticated_herd_fixture()
	fixture["biomass"] = 310.0
	fixture["pen_fed_fraction"] = PEN_FED_STARVING
	return fixture
