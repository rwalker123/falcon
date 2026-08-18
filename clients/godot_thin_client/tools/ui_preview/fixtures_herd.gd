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

# **THE DENIAL RAID's carry** (`docs/plan_denial_raid.md`) — food ONE party member hauls home over the
# whole raid. Deliberately tiny beside what the raid kills, because that ratio IS the mission: a party
# that never stops engaging kills far more than its pack holds, so `delivered_food` is a rounding error
# and `wasted_food` is the take. A fixture that hauled its whole kill would be a hunting raid wearing a
# denial outcome, and the waste readout the mission exists to show would have nothing to state.
const DENIAL_CARRY_PER_WORKER := 2.0
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

## **WHAT HOLDING A BUILT PEN COSTS, PER TURN, IN WORK** — `intensification_ladder.json`'s
## `animal:pen` upkeep (`1.0` per keeper-load) over this herd's two loads. Its supplied half is ONE
## keeper's worth, so the fixture sits at a live shortfall: half the bill unmet, which is exactly the
## state the keeping row exists to warn about.
const ANIMAL_PEN_UPKEEP_DEMAND := 2.0

const ANIMAL_PEN_UPKEEP_SUPPLIED := 1.0

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
## `food_per_animal` 2.0 through `raid_estimate_table`, so the payload is `animals × 2` food.
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
## not "this rung is a denial mission", and a boar is edible on every rung). A `delivers_trade` /
## `delivered_trade` twin rode every cell until arc #527 retired that account.
## The per-policy bumps are illustrative fixture data; the live sim exports the real per-floor counts.
static func raid_estimate_table(turns_row: Array, animals_row: Array, fpa: float,
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
				"turns_to_fill": turns, "delivers_food": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * fpa, "wasted_food": 0.0,
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
## **IT IS THEREFORE UNIT-FREE, AND AN INEDIBLE QUARRY'S CALLER STATES IT IN BIOMASS.** Nothing here
## names an account: pass a room, a carry and a quantum all in biomass and `delivered` comes back in
## biomass, which is the only form a species with a structural zero food rate has. That is what lets
## the wolf's claim be a cross-check against the SAME oracle the deer's is, rather than a second one.
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
##       `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, …}`. An
##       expedition's trip is NOT a rate division (on Surplus/Deplete the ceiling is a *stock* the party
##       strips in a turn or two, then it crawls at the regrowth trickle), so the client looks the answer
##       up and does no math. `turns_to_fill == 0` → the projection ran out with the raid still going,
##       which is `TRIP_BOUND_HORIZON` and nothing else — a raid that ends by emptying the range
##       reports the turn it ended on, like any other; `delivers_food == false` says the QUARRY IS
##       INEDIBLE (#337) and, since arc #527 retired the `delivers_trade` sibling, is on its own what
##       makes a raid a denial mission.
## **A ROW THAT DELIVERS NOTHING CANNOT WEAR A PARTY-SIDE BOUND.** `pack_full` requires a LOAD, and a
## load is a delivery — so the sim never pairs it with an empty payload, and
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

## **A STRIP-BARE RAID FINISHES BY EMPTYING THE RANGE, so it reports the turn it finished on.**
##
## The floor-`0` row used to carry `turns_to_fill == 0` beside `TRIP_BOUND_HORIZON`, i.e. the wire's
## "still going when the projection ran out" — which the sheet then read on three surfaces at once as a
## raid that never completes, for the one mission whose whole purpose is to finish. The sim reserves
## that sentinel for `horizon` alone now: a raid that drives the herd under its extinction floor ends
## on `herd_lost`, on a real turn, because the live arm's lost-herd guard turns the party for home in
## that same turn. This is that turn — longer than the surplus raid on the same herd, since the party
## keeps killing until there is nothing left rather than stopping at a floor.
const STRIP_BARE_TRIP_TURNS := 11

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
		"hunt_trip_estimates": {
			"sustain:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": trip_turns, "delivers_food": true,
				"animals_taken": sustain_animals,
				"delivered_food": sustain_delivered, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(sustain_animals, "sustain",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			"surplus:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "surplus",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			"deplete:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered, "wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "deplete",
					SourceForecast.TRIP_BOUND_PACK_FULL),
			},
			# Eradicate DELIVERS (issue #337): `delivers_food` says the quarry is EDIBLE, not that the
			# rung is a denial mission, and an Eradicate raid banks the whole-stock windfall.
			"eradicate:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": STRIP_BARE_TRIP_TURNS,
				"delivers_food": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered, "wasted_food": 0.0,
				# **THE FLOOR-`0` ROW COMPLETES, and its turn count is what says so.** It used to pair
				# `turns_to_fill == 0` with `TRIP_BOUND_HORIZON` — the wire's "still going when the
				# projection ran out" — which the sheet read on three surfaces at once as a raid that
				# never completes (`over many turns` / `still delivering at the end of the forecast` /
				# `Send Anyway (long raid)`), for the one mission whose whole purpose is to finish.
				# `herd_lost` beside a REAL turn is the sim's own pairing; the horizon pairing lives on
				# `hunt.gd`'s `_horizon_raid_herd`, where it is genuinely what the projection found.
				SourceForecast.TRIP_BOUND_KEY: clean_raid_bound(surplus_animals, "eradicate",
					SourceForecast.TRIP_BOUND_HERD_LOST),
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
	price_animal_build(fixture)
	fixture["ecology_phase"] = "thriving"
	fixture["tile_info"] = compact_herd_tile_fixture()
	return fixture

## THE INVESTMENT PAYOFF FACES (issue #397) — a Wild Boar on a pen-ceiling species, so BOTH investment
## rungs are offered and both wear the payoff the faces render:
##   Tame   → `pastoral_yield` 1.48 food  ⇒ `→ 1.48 food`
##   Corral → `corral_yield`   2.95 food  ⇒ `→ 2.95 food`
## These are the boar figures from the issue's own report. A `pastoral_trade` / `corral_trade` half
## rode beside each until arc #527 retired that account, so the faces state food alone again — and a
## herd's non-food product has no per-rung quote on the wire at all. Ordering is the ladder's:
## Sustain (0.90) < Tame (1.48) < Corral (2.95).
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
	fixture["corral_yield"] = 2.95
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
	price_animal_build(fixture)
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

## ---- THE BUILD, PRICED IN WORK (`docs/plan_unit_costed_work.md` §8) ---------------------------
## `intensification_ladder.json`'s own `work_cost` for the two ANIMAL rungs, times this species' own
## cost multiplier for the Tame (penning is flat for every species — a fence is a fence). One unit is
## one worker-turn at the food peak with no gear.
const ANIMAL_TAME_WORK_COST := 50.0

const ANIMAL_CORRAL_WORK_COST := 75.0

## The SIM's own estimate of what is left on the running build, at this fixture's keeper crew, floor
## and kit. The client computes none of it; `SourceForecast.BUILD_TURNS_NO_ESTIMATE` is the other
## reading and renders as no line at all.
const ANIMAL_BUILD_TURNS_REMAINING := 6

## **WHAT THIS HERD'S BUILDERS ADD WITH THE HANDLING GEAR, PER TURN** — the shipped `hurdles` flint
## tier at `BandFx.KIT_BUILD_WORK_HANDLING` (0.5) per equipped worker, over the reference two-keeper
## crew, so `2 × 0.5` = 1.0. **The animal web is where this readout is judged**, no plant item
## declaring the stat yet (issue #539).
##
## ⛔ **IT WAS `17.0` — `2 × 8.5` UNDER THE RETIRED SUBTRACTION** (`docs/plan_standing_upkeep.md`
## §4.8). `buildWorkFromGear` was *what the tools took OFF the job* and is *what the pool's kits ADD
## per turn*; the magnitude moved with the meaning, and 17 work a turn from two keepers would be a
## band out-building its own 50-unit Tame twice over in a single turn.
const ANIMAL_BUILD_WORK_FROM_GEAR := 1.0

## Price a herd's two rungs in WORK, deriving each meter's `work_done` from the fraction the fixture
## already states — so a fixture that re-dials a meter cannot end up with a percentage and an
## absolute that disagree, which is the one thing this readout exists to make visible.
## `upkeep` is the rung RATE both animal rungs quote, and it is a PARAMETER because the rate scales
## with the herd's own keeper load: the reference herd is two loads, a warren is one. A fixture whose
## load differs states it rather than inheriting a rate its own size would never produce.
static func price_animal_build(fixture: Dictionary,
		turns: int = ANIMAL_BUILD_TURNS_REMAINING,
		gear: float = ANIMAL_BUILD_WORK_FROM_GEAR,
		upkeep: float = ANIMAL_TAME_UPKEEP_DEMAND) -> Dictionary:
	fixture["tame_work_cost"] = ANIMAL_TAME_WORK_COST
	fixture["tame_work_done"] = \
		float(fixture.get("domestication", 0.0)) * ANIMAL_TAME_WORK_COST
	fixture["corral_work_cost"] = ANIMAL_CORRAL_WORK_COST
	fixture["corral_work_done"] = \
		float(fixture.get("corral_progress", 0.0)) * ANIMAL_CORRAL_WORK_COST
	fixture["build_turns_remaining"] = turns
	fixture["build_work_from_gear"] = gear
	# The compose sheet's own SOURCE term, beside the sim's answer — see `BaseFx` for why the
	# per-worker output is stated rather than assumed. The gear half is not here: it is the BAND's
	# ledger, so it rides `BandFx.kit_tiers_rows`.
	fixture["build_work_per_worker_turn"] = BaseFx.BUILD_WORK_PER_WORKER_TURN
	# **THE PER-RUNG RATE, PUBLISHED UNCONDITIONALLY BESIDE THE PER-RUNG COST** — the plant twin's own
	# note says why `upkeep_demand` cannot serve: it is what the herd is BILLED today, so a herd with
	# nothing started reads `0` and a sheet netting against it quotes a finish date on a rung its crew
	# can never advance. Both animal rungs quote per keeper-load, so these carry this herd's own load.
	fixture["tame_upkeep_demand"] = upkeep
	fixture["corral_upkeep_demand"] = upkeep
	# **AND THE ROT, WHICH ON THIS WEB IS STRUCTURALLY NOTHING** — see `ANIMAL_METER_ROT` below.
	fixture["meter_rot_per_turn"] = ANIMAL_METER_ROT
	# **AND WHERE IT SITS IN THE BAND'S QUEUE** (`docs/plan_standing_upkeep.md` §4.6b), the plant
	# twin's rule: the sim publishes a countdown only for a source some band has queued, so a priced
	# build with the sentinel position would stage a countdown belonging to no entry.
	fixture["build_queue_position"] = SourceForecast.BUILD_QUEUE_HEAD
	return fixture

## **NO ANIMAL RUNG CAN LOSE METER, SO THIS IS A CONSTANT AND NOT A PARAMETER**
## (`docs/plan_standing_upkeep.md` §2.4). Neither `animal:pastoral` nor `animal:pen` declares a
## `meter_decay`: their penalty for an unpaid keeping is a **shed** — the flock loses animals — and
## `domestication_progress` is monotone-up, so an animal meter never goes backwards.
##
## It is stated rather than omitted because the build's closed form NETS it (`net = crew work − rot`),
## so an absent field and a stated nothing are the same arithmetic and only one of them says the
## nothing is a fact about the web. The consequence worth knowing: on this web every staffed build
## crew climbs, and the two never-finishing sentinels are unreachable from an animal source.
const ANIMAL_METER_ROT := 0.0

## **WHAT HOLDING A TAMED HERD COSTS, PER TURN, IN WORK.** `intensification_ladder.json` declares
## `1.0 × source_load` on **both** animal rungs — a penned animal is not less work to mind than an
## unfenced one, and what the fence buys is GRACE, not rate — so this is the pen's own figure by
## construction rather than by coincidence, over the reference herd's two keeper-loads. Spelled as an
## alias so a retune of one animal rung cannot silently leave the other behind.
const ANIMAL_TAME_UPKEEP_DEMAND := ANIMAL_PEN_UPKEEP_DEMAND

## The world's herd list (Main pushes snapshot["herds"]). Named because the turn-orb starving-pen
## state swaps in its own list and must restore this one.
static func world_herds_fixture() -> Array:
	return [
		{
			"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120,
			"ecology_phase": "stressed", "food_per_animal": 2.0,
			# **THE DENIAL TABLE, so an IN-FLIGHT denial party has a verdict to read**
			# (`docs/plan_denial_raid.md` §3). The sim publishes no per-party collapse field — its
			# answer lives on the TARGET herd, one row per party size — so a launched raid resolves
			# its own row out of this list exactly as its launch sheet did. A world-herd entry
			# without one leaves the party's readout blank, which is the state no live server can be
			# in and the one in which every claim about that line passes vacuously.
			"denial_estimates": denial_estimate_table(SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY,
				DENIAL_COLLAPSE_TURNS, DENIAL_COLLAPSE_LOW, DENIAL_COLLAPSE_HIGH,
				DENIAL_COLLAPSE_KILLS, 2.0),
		},
	]

# The in-flight denial party's own table — parties 1..8 against the reference Red Deer. **More hands
# break the herd SOONER and that is the mission's only lever**, so the rows fall monotonically; the
# band widens where the retreat is chanciest. The party the frames render is `HUNT_EXPEDITION_PARTY`
# (5), whose row is `4` with a `3–5` band — the plan's own worked example.
const DENIAL_COLLAPSE_TURNS := [12, 8, 6, 5, 4, 4, 3, 3]

const DENIAL_COLLAPSE_LOW := [10, 7, 5, 4, 3, 3, 2, 2]

const DENIAL_COLLAPSE_HIGH := [15, 10, 8, 6, 5, 5, 4, 4]

const DENIAL_COLLAPSE_KILLS := [30, 48, 60, 70, 78, 86, 92, 98]

## The DENIAL raid's pre-launch table (`docs/plan_denial_raid.md` §1.1) — an ARRAY with ONE row per
## party size and **no other axis**, which is the whole shape difference from `raid_estimate_table`
## above: denial carries no floor and no fill target, so party size is the only thing there is to
## sample and a row's own `party_workers` is its identity.
##
## `outcome` is the sim's verdict and the client renders NOTHING numeric without it, so every row
## carries one. A `repelled` / `horizon` table passes all-zero turn rows: `0` means "not within the
## horizon on that end", never "immediately".
static func denial_estimate_table(outcome: String, turns_row: Array, low_row: Array,
		high_row: Array, kills_row: Array, fpa: float,
		carry_per_worker: float = DENIAL_CARRY_PER_WORKER) -> Array:
	var rows: Array = []
	for i in kills_row.size():
		var party := i + 1
		var killed := int(kills_row[i])
		var killed_food := float(killed) * fpa
		# What the pack holds, never what it killed — the raid banks a rounding error on the way home
		# and leaves the rest standing dead on the range.
		var hauled := minf(killed_food, float(party) * carry_per_worker)
		rows.append({
			"party_workers": party,
			"turns_to_collapse": int(turns_row[i]),
			"turns_to_collapse_low": int(low_row[i]),
			"turns_to_collapse_high": int(high_row[i]),
			"outcome": outcome,
			"animals_killed": killed,
			"delivered_food": hauled,
			"wasted_food": killed_food - hauled,
		})
	return rows

static func herd_fixture() -> Dictionary:
	# **BOTH RUNGS ARE PRICED, because the wire prices them** — `workCost` is published whether or not
	# a build is in flight, which is what lets the compose sheet quote a Tame before the player
	# commits to it. `price_animal_build` derives each meter's `work_done` from the fraction stated
	# below, so the two halves of a row can never disagree.
	return price_animal_build({
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
		# **THE STANDING UPKEEP** (`docs/plan_standing_upkeep.md` §2). The animal rungs quote their
		# rate per KEEPER-LOAD (`head count / animals_per_herder`), which is what lets one number say
		# *a shepherd minds 300 sheep and a cowherd 80*. This reference herd is WILD, so nothing is
		# built on it, nothing is owed, and the pair below reads the honest zero rather than a
		# sentinel — the same reading `has_neglect_grace: false` states one field along.
		"upkeep_demand": 0.0,
		"upkeep_supplied": 0.0,
		"upkeep_shortfall": 0.0,
		"upkeep_workers_needed": 0,
		"tile_info": BaseFx.food_tile_fixture(),
	})

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
	price_animal_build(fixture)
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
	price_animal_build(fixture)
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
	price_animal_build(fixture)
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
	price_animal_build(fixture)
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
	price_animal_build(fixture)
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
	# **THE STANDING UPKEEP, UNDERPAID** (`docs/plan_standing_upkeep.md` §2, §2.4). The `animal:pen`
	# rung asks `1.0` work per KEEPER-LOAD and this herd is two loads over; one keeper is on it, so
	# half the bill goes unmet — and the shortfall IS the decay, so the drawer must say the rung is
	# being lost and how long it has. It is the ANIMAL web's copy of the reading the plant card makes,
	# and the one fixture in the corpus that renders the warning rather than the reassurance.
	fixture["upkeep_demand"] = ANIMAL_PEN_UPKEEP_DEMAND
	fixture["upkeep_supplied"] = ANIMAL_PEN_UPKEEP_SUPPLIED
	fixture["upkeep_shortfall"] = ANIMAL_PEN_UPKEEP_DEMAND - ANIMAL_PEN_UPKEEP_SUPPLIED
	# `ceil(2 / 1)` — two keepers meet the whole bill, against the one that is on it.
	fixture["upkeep_workers_needed"] = 2
	# The `animal:pen` rung's own `upkeep.grace_turns` (6), counted down by four turns of shortfall —
	# so the row reads "lost in 2 turns" rather than the full grace, which is the state a player has
	# to be able to see coming.
	fixture["has_neglect_grace"] = true
	fixture["neglect_grace_remaining"] = 2
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
