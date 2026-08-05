extends RefCounted

## Hunting: crews, raids, forecasts and the whole-animal cap.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The armed hunt party for the pre-launch forecast states (4 workers, matching the spec's worked
# example: a 4-worker party fills in ~6 turns on a mammoth but ~54 on red deer).
const LOCAL_HUNT_HUNTERS := 6

## What the stepper actually renders once `LOCAL_HUNT_HUNTERS` is dialed in: the sheet's own
## whole-animal carry cap ("max 3 workers useful here"), Red Deer food_per_animal 2.0 ÷ the band's
## 0.8 per-worker carry = 3 carriers to haul one body. The dial is clamped to it exactly as it is
## for the player, so this — not 6 — is what a guard on those frames can assert.
const LOCAL_HUNT_CAPPED_CREW := 3

## The crews the two BOUND frames are composed at, and the numbers their steppers must SHOW: one
## bound by the band's idle labor, one by the maximum party size. Named because each is asserted
## against the rendered value, so the dial and the expectation are one number rather than two.
const LABOR_BOUND_CREW := 3

const PARTY_SIZE_BOUND_CREW := 2

## The crew the TWO-PRODUCT frames (issue #337) are composed with — the wolf's pelts-only pair and the
## oracle deer's food+trade control. Two hunters is the oracle's own no-waste point (food_per_animal
## 1.23 ÷ the band's 0.8 per-worker carry ⇒ 2 carriers haul one whole body), so the frame the trade
## components are read on carries no waste term to argue with; the wolf rides the same crew so the
## inedible quarry and the both-products control are compared at ONE party size.
const PELT_FRAME_HUNTERS := 2

# **BELOW `ecology.collapse_fraction` (0.15).** The herd is past its Allee threshold, so the sampled
# curve is NEGATIVE here and the projection must show a decline the crew did not cause.
const FLOOR_CHART_ALLEE_STOCK_FRACTION := 0.08

# A crew big enough to bite, small enough that "clear it now" and "hold it after" stay different
# numbers — a frame where the two targets coincide cannot show that there are two of them.
## The two INVESTMENT-rung payoff terms the Wild Boar frame is judged on (issue #397), spelled out as
## literal strings rather than rebuilt from `SourceForecast.picker_products` — an assertion that
## re-derives the terms through the very formatter under test asserts nothing. Food leads, and each
## half appears because the boar pays both; the pre-fix face was the food clause alone.
##
## They moved off the PICKER's rung face onto the IMPROVEMENT control's own (issue #442), which is why
## the payoff ARROW is gone from them: the control's face already reads
## `◎ Tame this herd · then <terms>`, so a second arrow inside the terms said "then → 1.48" twice.
const BOAR_TAME_PAYOFF_FACE := "1.48 food · 0.37 trade"

const BOAR_CORRAL_PAYOFF_FACE := "2.95 food · 0.74 trade"

# The sim's forward-SIMULATED turns-to-fill for the 4-worker party in these states (it exports the
# answer; the client never divides). Sustain is a small renewable flow → slow; Surplus/Deplete strip the
# herd's stock headroom first → fast. The deer's Sustain trip (54) blows past the 20-turn viability
# threshold; its Surplus trip (6) does not — same herd, same party, opposite verdicts.
const MAMMOTH_SUSTAIN_TRIP_TURNS := 6

const DEER_SUSTAIN_TRIP_TURNS := 54

const DEER_SURPLUS_TRIP_TURNS := 6

const MAMMOTH_SURPLUS_TRIP_TURNS := 3

# The whole animals the 4-worker RAID delivers (HuntTripEstimate.animalsTaken) — the payload the readout
# headlines. A viable/slow raid lands a positive count; a herd at/below its policy floor lands 0 (the
# no-surplus state). Surplus/Deplete raid deeper than Sustain, so a deeper policy lands MORE animals.
const MAMMOTH_SUSTAIN_ANIMALS := 8

const DEER_SUSTAIN_ANIMALS := 6

const DEER_SURPLUS_ANIMALS := 12

const NO_SURPLUS_ANIMALS := 0

# The server's measured Wild Boar raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter → 5 animals /
# 7 turns, 2 → 8 / 8, 3 → 8 / 4. animalsTaken PLATEAUS at 8 (party 2), so max-useful = 2 hunters — the
# frame the "delivers ≈5 boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged on.
const MAMMOTH_FOOD_PER_ANIMAL := 16.0

# One animal's worth of TRADE GOODS on an EDIBLE quarry (issue #337) — a hunt pays a vector, so every
# raid cell carries a trade payload beside its food one and the readout names both. Deliberately much
# smaller than the food quantum: a deer/boar is meat first, hide second (the INEDIBLE case, where trade
# is the whole payload, is the wolf fixture below).
const RAID_TRAVEL_TURNS := 8

const RAID_TRAVEL_HUNT_TURNS := 8

# 0 = the raid ran the whole forecast horizon still delivering (a long raid), used by the no-surplus /
# collapsed fixtures where the raid also lands 0 animals.
const NEVER_FILLS_TRIP_TURNS := 0

## `floor_chart_model`'s `lesson_known` for a probe reading the VERDICT rather than the aside: the
## faction has NOT learned this source's lesson, so the teaching line is the one it always carried.
const LESSON_NOT_YET_LEARNED := false

## The TRIP-READOUT claims that live on the `_hunt_assign_forecast_states` frames, dispatched by state
## name so each fixture is asserted on the ONE thing it was built to show. They ride here rather than
## after the loop because the loop is where each state is actually staged, and re-staging one to assert
## it would risk asserting a sheet the frame never rendered.
##
## **EACH IS ONE HALF OF A PAIR**, the other half being `herd_hunt_expedition`'s block (a clean raid
## paying BOTH accounts, no waste, a brisk OK verdict): a lone "the waste note is here" passes on a
## readout that always prints one, and a lone "there is no trade row" passes on a readout that can no
## longer print any account at all.
func _assert_trip_readout(state_name: String) -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	match state_name:
		"herd_hunt_forecast_viable":
			# A party of 4 kills a 16-food mammoth and hauls 4 of it — the WASTE half, and the ZERO
			# ACCOUNT half in one fixture: this cell carries no `delivers_trade` at all, so the trade
			# row must not render. The trade-paying deer in `herd_hunt_expedition` is its twin.
			var wasted := Readout.yields_text(sheet)
			var waste_pct := int(round((MAMMOTH_FOOD_PER_ANIMAL - HerdFx.HUNT_FORECAST_PARTY)
				/ MAMMOTH_FOOD_PER_ANIMAL * 100.0))
			h._assert_hud("a partial kill states its WASTE on the trip's yields row",
				wasted.contains((SourceForecast.HUNT_WASTE_NOTE_FORMAT % waste_pct).to_upper()))
			h._assert_hud("…and an account the quarry does not pay renders NO row",
				wasted.contains("FOOD") and not wasted.contains("TRADE"))
		"herd_hunt_forecast_slow":
			# 54 turns past the band's 20-turn warn line — the verdict carries the severity the Send
			# button and the one-line form already carry, so the box cannot disagree with either.
			h._assert_hud("a raid past the band's warn line reads SLOW in the trip verdict",
				Readout.verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and Readout.verdict_text(sheet).contains(str(DEER_SUSTAIN_TRIP_TURNS)))
		"herd_hunt_forecast_eradicate":
			# `turns_to_fill == 0` — the raid ran the whole forecast horizon still delivering, so there
			# is no total to quote and the verdict says so instead of printing a bare 0.
			h._assert_hud("an unbounded raid states no total, and still reads SLOW",
				Readout.verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and Readout.verdict_text(sheet).contains(
						SourceForecast.EXPEDITION_TRIP_LONG_VERDICT))
		"herd_hunt_forecast_no_surplus":
			# **A REFUSED RAID RENDERS NO BOX AT ALL.** It has no payload to lay out in rows, and an
			# empty well would read as a raid delivering nothing measurable rather than one the panel
			# is declining — so the branch keeps the one-line refusal it always had.
			h._assert_hud("a raid with no surplus renders the refusal, never an empty readout box",
				Q.find_meta_node(sheet, HudWidgets.YIELDS_ROW_META) == null
					and Q.has_label_containing(sheet, "too lean to raid"))

const COMPOSE_SPINE_KEY_HUNT := "local hunt"

const COMPOSE_SPINE_KEY_EXPEDITION := "hunt expedition"

## THE PARITY ASSERTION: the two LOCAL compose sheets must read in the same control order, start to
## finish. Both keys must have been recorded — comparing two missing spines would pass while proving
## nothing, which is the failure mode a frame-only check already has.
func _assert_compose_order_parity(forage_key: String, hunt_key: String) -> void:
	var have_both = h._compose_spines.has(forage_key) and h._compose_spines.has(hunt_key)
	h._assert_hud("both compose spines were captured before the parity check (%s, %s)"
		% [forage_key, hunt_key], have_both)
	if not have_both:
		return
	var forage_spine: Array = h._compose_spines[forage_key]
	var hunt_spine: Array = h._compose_spines[hunt_key]
	h._assert_hud(("the forage and local-hunt sheets read in the SAME control order — forage %s, hunt %s"
		% [str(forage_spine), str(hunt_spine)]), forage_spine == hunt_spine)

## Two player bands (multi-band split is deferred, but the assign controls' band-picker must
## handle N). Different idle_workers so switching the dropdown visibly re-caps the worker
## stepper; neither hunts the deer herd, so the cap for a fresh source == idle_workers.
func _two_player_bands() -> Array:
	# hunt_reach 6 keeps both bands WITHIN local reach of the (66,10) herd (distances 0 and 3), so the
	# band-picker states test the LOCAL-hunt re-cap (the distance-aware expedition path is exercised by
	# BandFx.hunt_distance_bands, in `fixtures_band.gd`).
	return [
		{"entity": 801, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 12, "hunt_reach": 6, "activity": "forage", "labor_assignments": []},
		{"entity": 802, "faction": 0, "size": 40, "current_x": 68, "current_y": 12,
			"working_age": 6, "idle_workers": 2, "hunt_reach": 6, "activity": "hunt", "labor_assignments": []},
	]

## A raid herd whose max-useful party DIFFERS BY POLICY, to prove the labor-bound note's "of M" tracks
## the selected policy: Sustain's animalsTaken keeps rising through a party of 4 (then plateaus), Deplete's
## through a party of 7. A band that can field only 3 hunters is labor-bound under BOTH — so the note reads
## "3 of 4 useful" on Sustain and "3 of 7 useful" on Deplete, the same herd, only the policy changed.
func _labor_bound_raid_herd() -> Dictionary:
	var herd := HerdFx.assign_preview_herd("game_bison_09", "Steppe Bison", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = 4.0
	var sustain_animals := [3, 5, 7, 9, 9, 9, 9, 9]     # plateau at party 4
	var surplus_animals := [4, 6, 8, 10, 12, 12, 12, 12] # plateau at party 5
	var deplete_animals := [5, 7, 9, 11, 13, 15, 17, 17]  # plateau at party 7
	var fpa := 4.0    # matches food_per_animal above; clean raid → delivered = animals × fpa, waste 0
	var table := {}
	for i in sustain_animals.size():
		var w := i + 1
		# Every rung DELIVERS, Eradicate included (issue #337 — `delivers_food` is about the species,
		# and a bison is edible on every rung). Both products on every cell.
		for entry in [["sustain", sustain_animals[i], 8], ["surplus", surplus_animals[i], 6],
				["deplete", deplete_animals[i], 5], ["eradicate", int(deplete_animals[i]) + 2, 4]]:
			var animals: int = int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": int(entry[2]), "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals, "delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * HerdFx.RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
				# A clean raid that hauls its whole kill: the PACK is what stops it.
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_PACK_FULL}
	herd["hunt_trip_estimates"] = table
	return herd

## A herd stripped to its policy floor: EVERY (policy, party) cell delivers 0 animals, so the raid comes
## home empty at any size — the one non-viable case (surplus is a property of the HERD, not the party, so
## no party size fixes it). The button must be DISABLED with the "too lean to raid" reason.
func _no_surplus_herd() -> Dictionary:
	var herd := HerdFx.assign_preview_herd("game_rabbit_02", "Rabbit Warren", "thriving", 0.05, 0, 0)
	herd["size_class"] = "small"
	# The herd is at its floor: no surplus at ANY party size → delivered_food 0 everywhere, so the raid
	# comes home empty and the button DISABLES ("too lean — no surplus above this policy's floor").
	var table := {}
	for w in range(1, 9):
		# The species is edible and its pelts sell — it is the HERD that has nothing left, so both
		# `delivers_*` flags are true on every rung and BOTH payloads are 0. That is what makes this
		# the "too lean" case rather than the "denial mission" one (issue #337).
		for policy in ["sustain", "surplus", "deplete", "eradicate"]:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": 0, "delivered_food": 0.0, "delivered_trade": 0.0,
				"wasted_food": 0.0,
				# The herd IS at its floor, so the raid's stop is the floor — reached on turn one, with
				# nothing taken. The client never renders this clause (the readout takes the refusal
				# sentence branch), which is exactly why the fixture must still carry the sim's answer.
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_FLOOR,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## The herd-panel EXPEDITION forecast states (herd beyond hunt_reach), each also naming the composed
## POLICY — because the policy is half the key (`"<policy>:<party_workers>"`) the forecast looks up in
## the herd's `hunt_trip_estimates`. Re-deriving a Surplus trip from the BAND's flow ceiling instead of
## reading the sim's row was the bug these cover.
func _hunt_assign_forecast_states() -> Array:
	return [
		{
			# THE PARTIAL-WITH-WASTE case: a Thunder Mammoth is big game (16 food/animal), and a party of
			# 4 can't carry a whole one — it kills the 1-animal surplus and hauls only 4 food, wasting 12.
			# So the line reads a brisk-but-lossy "delivers ≈1 Thunder Mammoth over ≈6 turns · ~4 food ·
			# ⚠ 75% wasted" (cyan headline + amber waste), and the button STAYS ENABLED (a partial is a
			# real delivery, the waste % is just informative). This is the case the whole pass exists for.
			"name": "herd_hunt_forecast_viable",
			"floor": 0.5,
			"herd": _partial_waste_mammoth(),
		},
		{
			# A SLOW raid: Sustain on a Red Deer still delivers ≈6 animals, but over 54 turns — past the
			# band's warn threshold (20) → amber "⚠ … — a slow raid" + "Send Anyway (≈54 turns)".
			"name": "herd_hunt_forecast_slow",
			"floor": 0.5,
			"herd": HerdFx.assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# The SAME Red Deer on Surplus: a Surplus raid strips deeper (≈12 animals) and comes home in
			# ~6 turns — a brisk, richer raid. Reading the sim's row, never re-deriving it.
			"name": "herd_hunt_forecast_surplus",
			"floor": 0.3,
			"herd": HerdFx.assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# No surplus: a collapsing Wild Fowl flock is at/below its floor → animalsTaken = 0, the raid
			# returns empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button.
			"name": "herd_hunt_forecast_no_surplus",
			"floor": 0.5,
			"herd": HerdFx.assign_preview_herd("game_fowl_03", "Wild Fowl", "collapsing", 0.0,
				NEVER_FILLS_TRIP_TURNS, NEVER_FILLS_TRIP_TURNS,
				NO_SURPLUS_ANIMALS, NO_SURPLUS_ANIMALS),
		},
		{
			# Eradicate DELIVERS (#337): every rung is paid the species' yield vector, so this row carries
			# a real payload and the client must NOT read a denial off the policy string. A denial is a
			# quarry that pays neither product — see `_pelt_only_wolf_raid_herd` for the inedible case.
			"name": "herd_hunt_forecast_eradicate",
			"floor": 0.0,
			"herd": HerdFx.assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
	]

## The partial-with-waste raid herd: a Thunder Mammoth (16 food/animal) whose standing surplus is ONE
## animal. Any fieldable party kills that 1 animal but cannot carry a whole mammoth — a party of `w` hauls
## ~`w` food and wastes the rest — so `delivered_food` rises with party size while `animals_taken` stays 1.
## At the composed party of 4: delivered 4, wasted 12 → 75% wasted, button ENABLED. The per-policy turns
## descend Sustain(6) > Surplus(4) > Deplete(3) > Eradicate(2) so the picker's max-food/turn caps read
## ASCENDING. This is
## exactly the case the old `animals_taken`-based "too lean" test and plateau scan got wrong (a leading 1).
func _partial_waste_mammoth() -> Dictionary:
	var herd := HerdFx.assign_preview_herd("game_mammoth_11", "Thunder Mammoth", "thriving", 2.7,
		MAMMOTH_SUSTAIN_TRIP_TURNS, MAMMOTH_SURPLUS_TRIP_TURNS,
		MAMMOTH_SUSTAIN_ANIMALS, MAMMOTH_SUSTAIN_ANIMALS)
	var fpa := MAMMOTH_FOOD_PER_ANIMAL
	herd["food_per_animal"] = fpa
	# Eradicate rides the SAME loop as the other three (#337): it is paid the species' yield vector like
	# every rung, so a mammoth is edible on Eradicate too. It merely raids fastest (2 turns), which keeps
	# the picker's max-food/turn caps ascending. It used to carry a hand-built `delivers_food = false`
	# cell — a denial state the sim can no longer produce for an edible quarry.
	var policy_turns := {"sustain": 6, "surplus": 4, "deplete": 3, "eradicate": 2}
	var table := {}
	for w in range(1, 9):
		var delivered := minf(float(w), fpa)     # each hunter hauls ~1 food of the 16-food kill
		for policy in policy_turns:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": int(policy_turns[policy]), "delivers_food": true,
				"animals_taken": 1, "delivered_food": delivered, "wasted_food": fpa - delivered,
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_PACK_FULL,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## The band the herd-panel EXPEDITION preview states staff: it carries the forecast levers (the global
## config values echoed on every cohort) and sits at (86,24) — ~27 tiles from the (66,10) herd, beyond
## its hunt_reach 7, so every herd resolves to the expedition branch.
func _hunt_preview_far_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 831, "faction": 0, "size": 80,
		"current_x": 86, "current_y": 24, "pos": [86, 24],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0) → the forecast's HAUL = party × this.
		"expedition_per_worker_carry": 4.0,
		"activity": "forage", "labor_assignments": [],
	}

## A band 8 tiles from the (66,10) herd (beyond hunt_reach 7 → expedition) carrying a MOVE RATE, so the
## raid forecast's round-trip travel is exercised: ceil(2 × 8 / 2) = 8 travel turns added to the hunting
## turns. `band_move_tiles_per_turn` now ships on the wire (schema slot 124) and is decoded onto the band;
## this carries the same value the decoder surfaces.
func _raid_travel_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 833, "faction": 0, "size": 80,
		"current_x": 66, "current_y": 18, "pos": [66, 18],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		"expedition_per_worker_carry": 4.0,
		"band_move_tiles_per_turn": 2,
		"activity": "forage", "labor_assignments": [],
	}

## The oracle band for the carry-aware delivered/waste preview: per-worker 0.8, output 1.0 (so the
## rendered numbers match the spec oracle EXACTLY — no morale modifier muddying them), sitting ON the
## herd (local branch), with plenty of idle workers so the big-game auto-max (20 carriers) isn't
## labor-bound.
func _delivered_oracle_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 840, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": 26,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	}

# ---- THE ENGAGEMENT-BOUND FOWL (docs/plan_hunt_through_combat.md §2) ----------------------------
# A LIGHT-BODIED quarry at the shipped hunt conversion, dialed so the two bounds on a take are an
# order of magnitude apart and neither frame can be passed by the wrong one. Every number below is a
# term the wire really carries; the derived ones are composed in the builder from these rather than
# restated, so the fixture cannot describe a bird that could not exist.
#   room above the food peak = 161 − 0.5 × 200 = 61 biomass = ~470 birds
#   one hunter's CARRY        = 0.80 food = 40 biomass = 307 birds
#   one hunter's REACH        = 10 birds
const FOWL_PROVISIONS_PER_BIOMASS := 0.02

const FOWL_BODY_MASS := 0.13

const FOWL_CAPACITY := 200.0

const FOWL_BIOMASS := 161.0

const FOWL_PER_WORKER_YIELD := 0.80

const FOWL_ENGAGE_RATE := 10.0

## ONE hunter, which is the party the defect was reported on: the two bounds are furthest apart there,
## and it is the smallest crew that can hold `animals_engaged`'s "a party that exists reaches one".
const FOWL_HUNTERS := 1

## The engagement A/B's herd. `engage_rate` is the ONLY field that moves between the pair, and its
## absent half publishes `NO_ENGAGEMENT_STAGE` — the literal wire value a PEN carries, and the reading
## the whole plant web gets by never publishing the field at all — so the twin pins that the arm DROPS
## rather than merely shrinking, and that forage and corrals are untouched by this arc.
func _engagement_fowl_herd(engage_rate: float) -> Dictionary:
	return {
		"id": "game_fowl_11", "label": "Wild Fowl (game_fowl_11)", "species": "Wild Fowl",
		"size_class": "small", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10,
		"husbandry_ceiling": "wild",
		"biomass": FOWL_BIOMASS,
		"carrying_capacity": FOWL_CAPACITY,
		"body_mass": FOWL_BODY_MASS,
		# The sim's own identity, composed rather than restated: `food_per_animal = body_mass ×
		# provisions_per_biomass`. A fixture that states both freely can claim a bird whose meat and
		# whose mass disagree, which is precisely the arithmetic these frames are judging.
		"food_per_animal": FOWL_BODY_MASS * FOWL_PROVISIONS_PER_BIOMASS,
		"provisions_per_biomass": FOWL_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": FOWL_PER_WORKER_YIELD,
		"engage_rate": engage_rate,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}

## One account's take — the `now` magnitude, whether or not the row also states a holding rate. The
## face reads `<now> → <after> <ACCOUNT>` when it arrows and `<now> <ACCOUNT>` when it does not, so
## the token to read is three back past an arrow and one back without one. Asked so an assertion
## about WHAT A TAKE PAYS is independent of whether this particular crew also reaches the floor —
## which is a different claim, and one `forage_accounts`' `_yield_now_after` already carries.
## Callers test the account's PRESENCE separately (the unit word), since an account with no row leaves
## no number to parse and the fall-through would read whatever token precedes it.
func _yield_take(yields_text: String, account: String) -> float:
	var upto := yields_text.split(account)[0].strip_edges().split(" ", false)
	if upto.is_empty():
		return 0.0
	if upto.size() >= 3 and upto[upto.size() - 2] == "→":
		return float(upto[upto.size() - 3])
	return float(upto[upto.size() - 1])

## ---- THE SPEC-ORACLE DEER'S OWN TERMS ----------------------------------------------------------
## The four wire numbers the both-products assertion recomposes this herd's take from, named here and
## spent in the fixture below so the expectation and the fixture cannot drift into two stories. The
## two per-animal quanta are the SIM's own reference mix (`SourceYieldForecast::body_mass_yield`), so
## an assertion that scales one account into the other through them is arriving at the answer by a
## different route than the client, which rescales through the per-biomass vector.
const ORACLE_DEER_FOOD_PER_ANIMAL := 1.23

const ORACLE_DEER_TRADE_PER_ANIMAL := 0.18

const ORACLE_DEER_PER_WORKER := 0.8

const ORACLE_DEER_SUSTAIN_CEILING := 2.33

## The spec oracle deer: food_per_animal 1.23, Sustain flow ceiling 2.33, per-worker 0.8, output 1.0.
##   1 worker  → can't carry one whole 1.23 deer → delivered 0.80, ≈0.65 deer/turn · ⚠ 35% wasted
##   2 workers → lands exactly one whole deer/turn, no waste → ≈1 deer/turn · renewable
##   4 workers → the Sustain-max cap, delivered 2.33 → ≈1.89 deer/turn, no waste
## Ascending `hunt_policy_ceilings` so the "up to X/turn" cap buttons read Sustain < Surplus < Deplete <
## Eradicate; husbandry ceiling "wild" keeps the picker to the four extractive rungs.
func _delivered_oracle_herd() -> Dictionary:
	return {
		"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "species": "Red Deer",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 820.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": ORACLE_DEER_FOOD_PER_ANIMAL,
		"per_worker_yield": ORACLE_DEER_PER_WORKER,
		"hunt_policy_ceilings": {
			"sustain": ORACLE_DEER_SUSTAIN_CEILING,
			"surplus": 3.5, "deplete": 5.0, "eradicate": 7.0,
		},
		# THE SECOND PRODUCT (issue #337). A deer is edible AND its hide sells, so it pays BOTH: the
		# picker's four rungs must read food-then-trade (food leading), never food alone. The trade
		# ceilings are the food ones times the species' hide-to-meat ratio, so they ascend together.
		"trade_per_animal": ORACLE_DEER_TRADE_PER_ANIMAL,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.34, "surplus": 0.51, "deplete": 0.73, "eradicate": 1.02,
		},
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## THE INEDIBLE QUARRY (issue #337) — a wolf pack: `provisions == 0` on every rung, a real TRADE yield
## on all four. It is the frame the whole arc is judged on. Before the fix the client read only food, so
## this herd rendered `+0.00` on every picker button and a source "worth nothing"; it must now read four
## ASCENDING trade numbers, NO food line anywhere, and no zeros. Every food-denominated field is
## deliberately 0/absent — `food_per_animal` too — so anything that still divides by a food quantum
## divides by zero and shows up in the frame rather than hiding.
func _pelt_only_wolf_herd() -> Dictionary:
	return {
		"id": "game_wolf_03", "label": "Grey Wolf (game_wolf_03)", "species": "Grey Wolf",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 240.0,
		"husbandry_ceiling": "wild",
		"prey_sense_radius": 4,
		"food_per_animal": 0.0,
		"per_worker_yield": 0.0,
		"hunt_policy_ceilings": {
			"sustain": 0.0, "surplus": 0.0, "deplete": 0.0, "eradicate": 0.0,
		},
		"trade_per_animal": 1.40,
		"per_worker_trade": 0.45,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.90, "surplus": 1.35, "deplete": 1.95, "eradicate": 2.70,
		},
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## The wolf's RAID table: `delivers_food = false` (an INEDIBLE quarry, NOT a denial policy) beside
## `delivers_trade = true` on every rung, so the expedition line must read a real delivery in trade
## goods rather than the "denial mission" the old `delivers_food`-only branch would have called it.
func _pelt_only_wolf_raid_herd() -> Dictionary:
	var herd := _pelt_only_wolf_herd()
	var table := {}
	var animals_row := [3, 5, 6, 6, 6, 6, 6, 6]
	for i in animals_row.size():
		for entry in [["sustain", 0, 9], ["surplus", 1, 7], ["deplete", 2, 6], ["eradicate", 4, 5]]:
			var animals: int = int(animals_row[i]) + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": int(entry[2]),
				"delivers_food": false, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": 0.0, "wasted_food": 0.0,
				"delivered_trade": float(animals) * 1.40,
				# **AN INEDIBLE QUARRY NEVER FILLS A *FOOD* PACK**, so the pack is inert on a wolf raid
				# and the herd's own floor is what ends it — the sim's `raid_load` rule, stated on the
				# fixture rather than left for the client to infer from `delivers_food`.
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_FLOOR,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## A big-game herd for the averaging-WINDOW hint: food_per_animal 16, Sustain flow ceiling 2.4 → one whole
## mammoth lands only every ceil(16/2.4)=7 turns, so the delivered ≈0.15/turn rate carries the "≈1 … every
## ~7 turns" span line. The whole-animal cap needs 20 carriers to haul one 16-food body, and auto-max
## staffs them (band idle 26).
func _big_game_window_herd() -> Dictionary:
	return {
		"id": "game_mammoth_01", "label": "Woolly Mammoth (game_mammoth_01)",
		"species": "Woolly Mammoth",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 3200.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": 16.0,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {
			"sustain": 2.4, "surplus": 3.6, "deplete": 5.0, "eradicate": 7.0,
		},
		# Ivory sells (issue #337) — a live herd carries the trade half of its vector too.
		"trade_per_animal": 2.4,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.36, "surplus": 0.54, "deplete": 0.75, "eradicate": 1.05,
		},
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## A BIG-GAME wild herd whose WHOLE-ANIMAL body outweighs one hunter's carry — the frame the peak-turn
## carry cap is judged on. An aurochs is one 80-biomass body dropped whole by the kill-credit bank;
## `food_per_animal` 1.6 is that body in food, and one hunter carries only `per_worker_yield` 0.80. So a
## lone hunter carrying an aurochs WASTES half — the panel must say TWO hunters are useful, not one.
##   Sustain ceiling 0.74: old cap = ceil(0.74 / 0.80) = 1 (the bug); new cap =
##     ceil((floor(0.74 / 1.6) + 1) × 1.6 / 0.80) = ceil(1.6 / 0.80) = 2 → "max 2 workers useful".
##   Deplete ceiling 1.86: two bodies drop on the peak turn → ceil((floor(1.86/1.6)+1) × 1.6 / 0.80) =
##     ceil(3.2 / 0.80) = 4 → the cap tracks the selected policy's ceiling upward.
func _aurochs_big_game_fixture() -> Dictionary:
	var fixture := HerdFx.herd_fixture()
	fixture["id"] = "game_aurochs_04"
	fixture["label"] = "Wild Aurochs (game_aurochs_04)"
	fixture["species"] = "Wild Aurochs"
	fixture["husbandry_ceiling"] = "wild"
	fixture["food_per_animal"] = 1.6
	fixture["per_worker_yield"] = 0.80
	fixture["hunt_policy_ceilings"] = {
		"sustain": 0.74, "surplus": 1.20, "deplete": 1.86, "eradicate": 2.60,
	}
	fixture["trade_per_animal"] = 0.24
	fixture["per_worker_trade"] = 0.12
	fixture["hunt_policy_trade_ceilings"] = {
		"sustain": 0.11, "surplus": 0.18, "deplete": 0.28, "eradicate": 0.39,
	}
	fixture["tile_info"] = HerdFx.compact_herd_tile_fixture()
	return fixture

## **THE CLAMPED REGIME, WHICH IS THE ONE THAT WAS REPORTED FROM PLAY — and no PNG in this harness
## renders it.** Every state above runs in a viewport far taller than its sheet, so all nine assert the
## roomy case, where the under-measured chrome cost the card's rect 9px and nothing visible. The defect
## bit on a SHORT window: `fit_to_content` decides "must this scroll?" by comparing the same
## chrome-derived desired height against the room below the card, so a chrome 21px short understated
## the content, left the scroll DISABLED on a sheet that genuinely did not fit, and the
## `PanelContainer` ran out of the card and off the bottom of the screen with `Hunt Here` sliced.
##
## The squeeze is `bottom_margin`, deliberately: it is the one term of `max_available` that `refit`
## does not re-declare per fit (it re-reads `max_height` from the live viewport every pass), so it can
## force the clamp while the REAL `refit` runs with the REAL chrome. Shrinking the canvas instead would
## re-render every frame after it and cost this harness its bit-identity reference.
##
## **THE ROOM LEFT IS THE PANEL'S OWN MINIMUM, and that is the whole discrimination.** A generous
## squeeze proves nothing — clamp a sheet to 200px and it scrolls whether the chrome is right or not,
## which is measured: an aggressive first cut of this check PASSED with the bug fully restored. The
## window in which the two answers differ is exactly the size of the error, so the room has to be
## pinned to an independent measure of what the sheet truly needs. At `panel_min` the correct chrome
## asks for `panel_min + CARD_EXTRA_PADDING` and must clamp; the short one asks for `panel_min − 9` and
## sails under, leaving the scroll off and the card smaller than what it draws.
##
## Three assertions, in order: the squeeze actually clamped (without it the other two are vacuous), the
## scroll came ON, and the card still contains the panel it draws.
func _assert_compose_sheet_scrolls_when_clamped(state: String) -> void:
	var sheet: ComposeSheet = h._hud._drawercompose._compose_sheet
	if sheet == null or not sheet.visible or sheet._panel == null:
		h._assert_hud("%s has an open compose sheet to squeeze" % state, false)
		return
	var roomy: float = sheet._card.size.y
	var room: float = sheet._panel.get_combined_minimum_size().y
	var restore: float = sheet._card.bottom_margin
	sheet._card.bottom_margin = maxf(
		sheet.size.y - sheet._card.global_position.y - room, restore)
	sheet.refit()
	await h._settle(false)
	var clamped: float = sheet._card.size.y
	var demanded: float = sheet._panel.get_combined_minimum_size().y
	var scrolling := sheet._scroll.vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED
	h._assert_hud("%s squeezed to %.0f: the card really is clamped short of its roomy height (%.0f, was %.0f)"
		% [state, room, clamped, roomy], clamped < roomy)
	h._assert_hud("%s squeezed to %.0f: a sheet that no longer fits turns its internal scroll ON"
		% [state, room], scrolling)
	h._assert_hud("%s squeezed to %.0f: …and the card still holds the panel it draws (%.0f demanded, %.0f card)"
		% [state, room, demanded, clamped], clamped >= demanded - Spine.COMPOSE_FIT_SLACK)
	sheet._card.bottom_margin = restore
	sheet.refit()
	await h._settle(false)

func run(harness) -> void:
	h = harness

	# ---- THE TWO ZERO-CREW SUBMITS, HUNT SIDE ----------------------------------------------------
	# The forage pair `forage_unstaffed` / `forage_unassign` (`chapters/forage_crop.gd`) is one half of
	# a rule that belongs
	# to BOTH sheets: `workers == 0` means two different things depending on whether this band already
	# works the source, and the sim skips validation entirely at 0 — so the unassign is always legal.
	# The hunt sheet had ONE state for both, a live button that sent a command changing nothing, and no
	# rename on the source it does work. These two frames are judged as a pair, exactly as the forage
	# ones are.
	#
	# State hunt-unstaffed (A) — 0 hunters on a herd this band does NOT hunt. Pressing would send a
	# no-op, so the button is DEAD and still wears the verb.
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.investment_pair_boar_herd())
	h._compose_herd(HerdFx.investment_pair_boar_herd(), ForageFx.ZERO_CREW)
	await h._settle()
	await h._save("herd_hunt_unstaffed")
	var hunt_noop_btn = Q.find_button_by_text(h._hud._drawercompose._compose_sheet,
		HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON)
	h._assert_hud("0 hunters on a herd this band does not hunt disables the submit (it would be a no-op)",
		hunt_noop_btn != null and hunt_noop_btn.disabled)

	# State hunt-unassign (B) — the SAME 0 on a herd this band DOES hunt (4 standing hunters on
	# `game_deer_07`): that is the sim's unassign, not a no-op. The button stays live and is RENAMED,
	# and the improvement control is GONE — what abandoning costs is already on the card in the rung's
	# own hint, so offering to START a build in the act of abandoning the source says two opposite
	# things at once. The positive-crew open below it is what makes that absence a CHANGE and not a
	# sheet that simply never offers this herd a rung.
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.taming_herd_fixture())
	h._compose_herd(HerdFx.taming_herd_fixture())
	await h._settle()
	h._assert_hud("precondition: at its standing crew the same herd IS offered its next rung",
		ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME) != null)
	h._compose_herd(HerdFx.taming_herd_fixture(), ForageFx.ZERO_CREW)
	await h._settle()
	await h._save("herd_hunt_unassign")
	var hunt_unassign_btn = Q.find_button_by_text(h._hud._drawercompose._compose_sheet,
		HudComposeVocab.UNASSIGN_BUTTON)
	h._assert_hud("0 hunters on a herd this band hunts stays live, renamed Unassign",
		hunt_unassign_btn != null and not hunt_unassign_btn.disabled)
	h._assert_hud("…and offers no improvement to start in the act of abandoning the source",
		ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME) == null
		and ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL) == null)

	# Back to a plain Sustain compose for the band-picker / distance states below.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = []
	h._hud._compose.set_hunt_count(1)
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_hunt_improvement("")
	h._hud._compose.reset_hunt_source()

	# State 3f — TWO player bands: the "Assign hunters" controls' "Band:" dropdown lists both
	# (positional "Band 1" / "Band 2"). Default selection is the resolved band (Band 1, 12 idle).
	# The Hunters count is dialed to 8 and CLAMPS to 7 with `+` disabled, because the binding cap here
	# is USEFULNESS ("max 7 workers useful here"), not the band's 12 idle — the frame shows the stepper
	# answering to the sheet's own ceiling while the picker's default selection resolves.
	h._hud._band_labor._player_bands = _two_player_bands()
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()   # force a fresh seed so the default selection = resolved band
	h._show_herd(HerdFx.herd_fixture())
	h._compose_herd(HerdFx.herd_fixture(), 8)
	await h._settle()
	await h._save("herd_band_picker")

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the picker path
	# re-caps the Hunters count to the newly-selected band's assignable workers (8 → 2, + now
	# disabled), demonstrating selection → actor band → stepper re-cap.
	var second_band: Dictionary = _two_player_bands()[1]
	h._hud._compose.set_hunt_band(int(second_band["entity"]))
	h._hud._compose.set_hunt_count(clampi(
		h._hud._compose.hunt_count(), 0, h._hud._band_labor.assignable_hunt_workers(second_band, HerdFx.herd_fixture()["id"])))
	h._compose_herd(HerdFx.herd_fixture())
	await h._settle()
	await h._save("herd_band_picker_b")
	# Reset so later states render their usual single-band dropdown.
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_hunt_source()

	# State 3h — distance-aware herd-hunt, SINGLE far band: a lone band ~27 tiles from the herd (beyond
	# its hunt_reach 7). The affordance fully replaces the local option — the button reads "Send
	# Expedition", a distance hint shows, the stepper reads "Party", and Assign emits
	# send_hunt_expedition (party = the stepper), NOT assign_labor.
	h._hud._band_labor._player_bands = [BandFx.hunt_distance_bands()[1]]   # only the FAR band
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(HerdFx.hunt_distance_herd())
	h._compose_herd(HerdFx.hunt_distance_herd())
	await h._settle()
	await h._save("herd_hunt_expedition")
	h._assert_compose_sheet_fits("herd_hunt_expedition")
	# The EXPEDITION branch reads in the same grammar as the two local sheets as far as it goes — it
	# builds no improvement control (a detached party builds nothing), so only the shared HEAD is
	# claimed here, which is exactly what `_record_compose_spine` asserts.
	h._record_compose_spine(COMPOSE_SPINE_KEY_EXPEDITION)
	# **THE TRIP READOUT — the expedition's answer in the SAME box the local sheet uses.** The branch
	# used to render one wrapped sentence carrying five facts ("delivers ≈3 Red Deer over ≈9 turns ·
	# ~6 food · ⇄ ~2 trade goods"), beside a local sheet that laid the same kinds of fact out in a
	# bounded well — two sheets on one panel reading nothing alike. What must NOT carry over is the
	# per-turn framing, and the header is where that shows: a trip has no steady state, so
	# `THIS TRIP` and not `PER TURN`, and no `now → after` arrow to key.
	h._assert_hud("the expedition sheet's readout is headed for a TRIP, not for a rate",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.EXPEDITION_TRIP_ROW_HEADER.to_upper())
	h._assert_hud("…so it states no PER TURN header and no now → after arrow",
		not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("PER TURN")
			and not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("→"))
	# THE PAYLOAD, ALL THREE TERMS. The animal count leads in the local hunt row's own idiom (the `≈`
	# face, the quarry as the unit, no account), then the accounts those bodies pay. Every term is
	# named, because matching one survives losing either of the others — and this quarry pays BOTH
	# accounts, which is the positive half of the render-only-where-the-vector-pays pair asserted on
	# the zero-trade mammoth below.
	var trip_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("the ANIMAL count leads the row, in the quarry's own name",
		trip_yields.contains("≈%d" % HerdFx.DISTANCE_RAID_ANIMALS[0]) and trip_yields.contains("RED DEER"))
	h._assert_hud("…with the trip's FOOD beside it",
		trip_yields.contains(SourceForecast.format_magnitude(HerdFx.DISTANCE_RAID_ANIMALS[0] * 2.0))
			and trip_yields.contains("FOOD"))
	h._assert_hud("…and its TRADE, since this quarry pays both",
		trip_yields.contains(SourceForecast.format_magnitude(
			HerdFx.DISTANCE_RAID_ANIMALS[0] * HerdFx.RAID_TRADE_PER_ANIMAL)) and trip_yields.contains("TRADE"))
	h._assert_hud("a raid that hauls its whole kill states NO waste note",
		not trip_yields.contains("wasted".to_upper()))
	# THE VERDICT states the trip's length. This band carries no move rate, so travel is 0 and there is
	# no split to spell out — the pair that DOES is `herd_hunt_raid_travel` below.
	h._assert_hud("the verdict states how long the party is away",
		Readout.verdict_text(h._hud._drawercompose._compose_sheet).contains(str(HerdFx.DISTANCE_RAID_TURNS[0])))
	h._assert_hud("…and a brisk raid reads OK",
		Readout.verdict_severity(h._hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)
	# **THE ESCAPEMENT GRAPH IS BACK ON THIS BRANCH** (`docs/plan_hunt_through_combat.md` §5.2) — and
	# this assertion once claimed the opposite, on the premise that a raid does not draw the herd down
	# the way a resident crew does. It does: a party's per-turn take is the same
	# `min(room, carry, engagement)`, so the curve IS the raid's. The half of the old pair that
	# SURVIVES is the crew targets, and the two must be asserted apart or "the expedition sheet reads
	# like the local one" quietly comes to mean "it grew a *hold it after* promise" — which a detached
	# party cannot keep, because it leaves.
	h._assert_hud("the expedition sheet draws the escapement graph — the herd-side half of the pair",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) != null)
	h._assert_hud("…but still NO crew targets: a party has no *hold it after*, it goes home",
		Q.find_crew_target(h._hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_CLEAR) == null
			and Q.find_crew_target(h._hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_HOLD) == null)
	# **THE PARTY-SIDE HALF: the fill target, offered and OFF.** Its two states are one glance apart
	# (an unticked box beside a ticked one), so the state is read off the control rather than off the
	# frame: no value Label exists while the box is clear, and the turns note prices the WHOLE
	# untargeted raid — the same `turnsToFill` the verdict above quotes.
	var expedition_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("the expedition sheet offers a fill target — the party-side half of the pair",
		Q.find_meta_node(expedition_sheet, HudWidgets.FILL_TARGET_META) != null)
	h._assert_hud("…and it opens with NO target set: the pack is what the raid fills",
		Readout.fill_target_value(expedition_sheet) == SourceForecast.NO_FILL_TARGET)
	h._assert_hud("…so its turns note prices the untargeted raid, not a target",
		Readout.fill_target_turns(expedition_sheet) == HerdFx.DISTANCE_RAID_TURNS[0])
	# **THE VERDICT NAMES WHICH STOP ENDS THE TRIP** — the point of §5.2, and the thing a turn count
	# alone cannot say. This raid fills its pack, so it must read the pack clause and NOT the floor
	# one; asserting only "some clause is present" would pass on the wrong stop.
	h._assert_hud("…and the verdict names the stop that ends the trip — the PACK, not the floor",
		Readout.verdict_text(expedition_sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_PACK_FULL])
			and not Readout.verdict_text(expedition_sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_FLOOR]))
	# The peak zone's half of change A, on the surface that has the least redundancy: this sheet sits
	# at `FLOOR_FOOD_PEAK`, the zone now says nothing, and the aside renders NOT AT ALL rather than a
	# dashed rule over empty space. Paired with the strip-zone assertion on `forage_three_accounts`,
	# which is what keeps "empty the whole table" from passing.
	h._assert_hud("the peak zone contributes no aside to the trip readout either",
		Readout.readout_aside_text(h._hud._drawercompose._compose_sheet) == "")

	# State 3i — TWO bands at DIFFERENT distances from ONE herd, NEAR band selected: band 811 sits ON
	# the herd (distance 0 ≤ reach 7) → "Hunt Here" + assign_labor. The band-picker selection —
	# not the herd — drives it (the resolved/default band is the near one here).
	h._hud._band_labor._player_bands = BandFx.hunt_distance_bands()
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(HerdFx.hunt_distance_herd())
	h._compose_herd(HerdFx.hunt_distance_herd())
	await h._settle()
	await h._save("herd_hunt_band_near")

	# State 3j — same two bands, FAR band selected via the picker (entity 812, ~27 tiles away): the SAME
	# herd now offers "Send Expedition" (party cap = min(idle 6, max party 8) = 6), proving that
	# WHICH band is selected flips the label + command + band-entity target, not the herd.
	h._hud._compose.set_hunt_band(int(BandFx.hunt_distance_bands()[1]["entity"]))   # FAR band
	h._compose_herd(HerdFx.hunt_distance_herd())
	await h._settle()
	await h._save("herd_hunt_band_far")
	# Reset so later states render their usual single-band dropdown + default band.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)

	# States 3k–3o — the HERD-PANEL hunt forecast, EXPEDITION branch. This is the second entry point
	# into a hunting expedition (herd-first): the herd is beyond the band's hunt_reach, so the panel
	# composes party + policy and sends immediately — no targeting step, so the banner's forecast never
	# appears. The forecast therefore renders LIVE above the button (the block re-renders on every
	# stepper tick / policy click) from the SAME helpers the banner uses: a PURE LOOKUP into the herd's
	# `hunt_trip_estimates` cell for (policy, party size). The client does no arithmetic here — the sim
	# forward-simulated each trip and exported the turns. Party 4:
	#   3k viable      — Sustain on a Thunder Mammoth: the sim's cell says 6 turns → cyan line, normal
	#                    primary "Send Expedition" button.
	#   3l not viable  — Sustain on Red Deer: 54 turns > warn 20 → amber line + the button itself goes
	#                    "armed" and names the cost: "Send Anyway (≈54 turns)".
	#   3m surplus     — the SAME Red Deer on Surplus: a Surplus party strips the herd's stock headroom
	#                    rather than living off its renewable flow, so the sim's cell says ~6 turns —
	#                    VIABLE. (The old bug re-derived the trip from the band's flow ceiling and scared
	#                    the player off a perfectly good trip; only the sim's own row knows.)
	#   3n never fills — a collapsing Wild Fowl flock: every cell is `turns_to_fill = 0` → red line +
	#                    the DISABLED "Herd too lean to raid" button, exactly as 3r below (the HERD has
	#                    nothing left to give, and no party size can fix a herd with no surplus).
	#   3o eradicate   — a healthy Red Deer on Eradicate: it DELIVERS like every other rung (#337 pays each
	#                    rung the species' yield vector), and its cell ran the whole horizon still
	#                    delivering → amber LONG-RAID line + "Send Anyway (long raid)". NOT a denial:
	#                    denial is now a property of the QUARRY (pays neither product), not of the rung.
	# WARNED, not BLOCKED — and never a confirm dialog: a slow raid and a long one are real tradeoffs, so
	# they read as a price tag and stay ENABLED. The ONE blocked case is 3n's, a herd with no surplus
	# left: it would return empty at every party size, so there is no price to pay.
	h._hud._band_labor._player_bands = [_hunt_preview_far_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	for state: Dictionary in _hunt_assign_forecast_states():
		var far_herd: Dictionary = state["herd"]
		h._hud._compose.reset_hunt_source()    # force a fresh seed (band = resolved, policy = the herd's current)
		h._hud._compose.set_hunt_band(-1)
		h._show_herd(far_herd)
		# The policy-picker click, without the click.
		h._compose_herd(far_herd, HerdFx.HUNT_FORECAST_PARTY, float(state["floor"]))
		await h._settle()
		await h._save(String(state["name"]))
		_assert_trip_readout(String(state["name"]))

	# AUTO-MAX on a policy click (expedition branch): picking a policy fills the Party to that policy's
	# max-useful cap. The mammoth's Sustain payload keeps rising to the fieldable ceiling, so a Sustain
	# click sets the party to 6 (min(plateau, idle 6)) — the "give me everything, zero idle hunters"
	# default. The compose hunt autofill is the one-shot a policy CLICK arms; the rebuild consumes it.
	var automax_herd := _partial_waste_mammoth()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(automax_herd)
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.arm_hunt_autofill()
	h._compose_herd(automax_herd)
	await h._settle()
	await h._save("herd_hunt_expedition_automax")

	# States 3p–3s — the RAID readout (delivered payload + waste) + the party stepper capped at max-useful.
	# A hunting expedition is a greedy raid: it grabs the herd's standing surplus in a burst and comes home,
	# so the headline is the delivered PAYLOAD, and `deliveredFood` PLATEAUS with party size once the surplus
	# (not the pack) binds — that plateau IS max-useful. The clean Wild Boar carries the server's measured
	# raid (hauls its whole kill, no waste). The picker buttons read each policy's MAX food/turn, ascending.
	#   3p boar raid   — a 1-hunter raid: "delivers ≈5 Wild Boar over ≈7 turns · ~20 food" (no waste), cyan +
	#                    primary "Send Expedition"; picker "up to +10.67 / +13.33 / +14.67 /turn".
	#   3q max useful  — 2 hunters: "delivers ≈8 Wild Boar over ≈8 turns · ~32 food"; a 3rd delivers NO more
	#                    food (the surplus binds), so the stepper caps at 2 and the `+` note reads
	#                    "max 2 workers useful here — more would be idle". The silent-idle-hunter gap, closed.
	#   3r no surplus  — a herd stripped to its floor: deliveredFood = 0 at EVERY size → the raid returns
	#                    empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button (party
	#                    size can't fix it — surplus is a property of the herd, not the party).
	#   3s eradicate   — the boar on Eradicate: the whole-stock windfall comes home (#337), so the raid line
	#                    quotes its payload in BOTH products and the Send button is the ordinary one. What
	#                    Eradicate costs is the herd itself, permanently — never the payload.
	var boar := HerdFx.raid_boar_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(boar)
	h._compose_herd(boar)   # source_changed seeds party = 1
	await h._settle()
	await h._save("herd_hunt_boar_raid")

	h._hud._compose.set_hunt_count(2)               # key unchanged → no re-seed; caps at the plateau (2)
	h._compose_herd(boar)
	await h._settle()
	await h._save("herd_hunt_max_useful")

	# State 3q-travel — the SAME boar raid, staffed by a band the herd is 8 tiles away from (beyond
	# hunt_reach 7 → expedition) and carrying a move rate. `turnsToFill` is HUNTING turns only, so the
	# client adds the round-trip TRAVEL the band-agnostic estimate table can't (ceil(2 × 8 / 2) = 8): at
	# party 2 the readout reads "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food",
	# and the stepper still caps at the animalsTaken plateau (2). `band_move_tiles_per_turn` now ships on the
	# wire (schema slot 124) and is decoded onto the band; this fixture carries it exactly as the decoder does.
	h._hud._band_labor._player_bands = [_raid_travel_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(boar)
	h._compose_herd(boar, 2)
	await h._settle()
	await h._save("herd_hunt_raid_travel")
	# **THE SPLIT — the half of the trip verdict `herd_hunt_expedition` structurally cannot show.**
	# That band carries no move rate, so its trip is all hunting and the verdict states one number;
	# this one walks 8 tiles each way at 2 tiles a turn, so the total is 8 hunting + 8 travel and the
	# verdict has to spell out where those turns go. Asserted as a PAIR with the total, because a
	# verdict quoting the split alone would leave the player adding it up themselves.
	var travel_verdict = Readout.verdict_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("a raid with travel states the TOTAL and the split it is made of",
		travel_verdict.contains(str(RAID_TRAVEL_HUNT_TURNS + RAID_TRAVEL_TURNS))
			and travel_verdict.contains("%d hunting, %d travel" % [
				RAID_TRAVEL_HUNT_TURNS, RAID_TRAVEL_TURNS]))
	h._assert_hud("…and a trip inside the band's warn line still reads OK",
		Readout.verdict_severity(h._hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)
	# Restore the far band (no move rate) for the remaining raid states.
	h._hud._band_labor._player_bands = [_hunt_preview_far_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]

	var lean := _no_surplus_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(lean)
	h._compose_herd(lean, HerdFx.HUNT_FORECAST_PARTY)
	await h._settle()
	await h._save("herd_hunt_no_surplus")
	# **THE HERD-SIDE HALF OF THE REFUSAL A/B.** Every cell here is `TRIP_BOUND_FLOOR` — the standing
	# surplus really is spent — so this frame is what stops the party-side claims in
	# `_unkillable_quarry_states` passing on a sheet that blames the party for everything. The two
	# fixtures deliver the identical zero and differ only in the sim's `bound`.
	var lean_sheet: Control = h._hud._drawercompose._compose_sheet
	var lean_quarry := SourceForecast.herd_display_name(lean)
	h._assert_hud("a herd genuinely at its floor is still refused in the HERD's name",
		Q.has_label_containing(lean_sheet,
				_empty_refusal(SourceForecast.TRIP_BOUND_FLOOR, "line", lean_quarry))
			and not Q.has_label_containing(lean_sheet,
				_empty_refusal(SourceForecast.TRIP_BOUND_HORIZON, "line", lean_quarry)))
	# **A PAYLOAD FLAT AT ZERO IS NOT A PLATEAU.** This herd publishes no `engage_rate`, so nothing
	# floors its ceiling — which is exactly the shape where the rise/break scan used to read the
	# flatness as "the first hunter was all that was useful" and cap the party at one.
	h._assert_hud("…and a raid that lands nothing at EVERY size names no max-useful party at all",
		not Q.has_label_containing(lean_sheet, SourceForecast.MAX_USEFUL_NOTE_FORMAT % [
			1, SourceForecast.MAX_USEFUL_NOUN_ONE]))

	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(boar)
	h._compose_herd(boar, 2, SourceForecast.FLOOR_MIN)
	await h._settle()
	await h._save("herd_hunt_eradicate")
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# States 3t–3v — the LABOR-BOUND note. When the herd's max-useful party exceeds the hunters you can
	# field, the `+` caps at LABOR (not usefulness), and the note names the reason AND the ceiling you're
	# working toward — "N of M useful — free up idle workers to send more". The Steppe Bison's plateau
	# DIFFERS BY POLICY (Sustain 4, Deplete 7), which is how the "of M" is shown to track the policy.
	var bison := _labor_bound_raid_herd()
	var bound_band: Dictionary = _hunt_preview_far_band().duplicate(true)
	bound_band["idle_workers"] = 3           # below Sustain's plateau of 4 AND Deplete's of 7 → labor-bound
	h._hud._band_labor._player_bands = [bound_band]
	h._hud._band_labor._player_band = bound_band
	#   3t Sustain — idle 3 < plateau 4 → "3 of 4 useful — free up idle workers to send more", + dead at 3.
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(bison)
	h._compose_herd(bison, LABOR_BOUND_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_labor_bound")
	h._assert_hud("the labor-bound frame renders the 3-hunter crew idle labor caps it at",
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == LABOR_BOUND_CREW)
	#   3u Deplete — SAME herd + band, policy flipped: the plateau rises to 7 → "3 of 7 useful", proving the
	#              ceiling tracks the selected policy. Key unchanged so the policy override sticks.
	h._hud._compose.set_hunt_floor(ForageFx.DEEP_DRAW_FLOOR)
	h._compose_herd(bison)
	await h._settle()
	await h._save("herd_hunt_labor_bound_deplete")
	#   3v Party-size-bound — the SUB-CASE where freeing idle workers would NOT help: idle 6 >= max party 2,
	#              so the party-SIZE cap binds, not idle. The note reads "2 of 4 useful — at the max party
	#              size" instead of the free-up-workers advice.
	var party_capped: Dictionary = _hunt_preview_far_band().duplicate(true)
	party_capped["idle_workers"] = 6
	party_capped["max_expedition_party_size"] = 2
	h._hud._band_labor._player_bands = [party_capped]
	h._hud._band_labor._player_band = party_capped
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(bison)
	h._compose_herd(bison, PARTY_SIZE_BOUND_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_party_size_bound")
	h._assert_hud("the party-size-bound frame renders the 2-hunter crew the max party size caps it at",
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == PARTY_SIZE_BOUND_CREW)
	# Restore the far band + sustain for the states that follow.
	h._hud._band_labor._player_bands = [_hunt_preview_far_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# States 3n–3o — the same panel's LOCAL branch (herd within hunt_reach). The preview line reads the
	# crew's HONEST carry-aware delivered take in ANIMALS (delivered ÷ food_per_animal), not the
	# unquantized food rate. Red Deer fpa 2.0, band per-worker 0.8, output 0.9; Sustain ceiling 0.30,
	# Deplete 0.60. `LOCAL_HUNT_HUNTERS` is dialed in, but the stepper clamps the crew to 3 carriers
	# (`LOCAL_HUNT_CAPPED_CREW`) — and the clamp is immaterial to what these two frames show: a 3-hunter
	# crew collects 2.16 food/turn and a 6-hunter one 4.32, both far above the ceilings below, so the
	# HERD's flow ceiling is what binds and the quantized take is the same either way:
	#   3n Sustain — delivered = min(0.30×0.9, …) = 0.27 → ≈0.14 Red Deer/turn · renewable (green).
	#   3o Deplete  — delivered 0.54 > Sustain 0.27 → WARN-amber "⚠ ≈0.27 Red Deer/turn — overdraws the
	#                herd" (the same ⚠ the allocation rows use). No waste (a whole deer is carryable).
	# (The herd's `hunt_trip_estimates` ride along but are IGNORED here — a trip table answers an
	# EXPEDITION's question; a local hunt is carry arithmetic over the band's flow ceilings. Band = flow
	# arithmetic; expedition = lookup.)
	var local_herd := HerdFx.assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
		DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS)
	h._hud._band_labor._player_bands = [BandFx.hunt_preview_local_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(local_herd)
	h._compose_herd(local_herd, LOCAL_HUNT_HUNTERS)
	await h._settle()
	await h._save("herd_hunt_local_sustain")
	h._assert_compose_sheet_fits("herd_hunt_local_sustain")
	await _assert_compose_sheet_scrolls_when_clamped("herd_hunt_local_sustain")
	# THE HUNT HALF, and the parity check itself: both sheets must ask WHICH STANCE before HOW MANY
	# PEOPLE, and in the same order throughout. The hunt sheet staffed first until the consistency pass;
	# a frame cannot hold that claim, which is why it is asserted rather than eyeballed.
	h._record_compose_spine(COMPOSE_SPINE_KEY_HUNT)
	_assert_compose_order_parity(Spine.COMPOSE_SPINE_KEY_FORAGE, COMPOSE_SPINE_KEY_HUNT)
	h._assert_hud("the local-hunt frames render the dialed-in crew (capped), not the re-seeded 1",
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == LOCAL_HUNT_CAPPED_CREW)

	# Flip the policy picker to Deplete — the same click path the player takes; the preview line
	# re-computes live off the new ceiling.
	h._hud._compose.set_hunt_floor(ForageFx.DEEP_DRAW_FLOOR)
	h._compose_herd(local_herd)
	await h._settle()
	await h._save("herd_hunt_local_overdraw")

	# The SAME local picker flipped to Eradicate — the frame the rung's HINT is judged on (issue #337).
	# Its text must describe the whole-stock windfall + the permanent end state, and must NOT claim the
	# rung yields nothing: the sim pays every rung its species' yield vector, Eradicate included.
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_MIN)
	h._compose_herd(local_herd)
	await h._settle()
	await h._save("herd_hunt_local_eradicate")

	# States 3p–3q — the WHOLE-ANIMAL carry cap. A big-game aurochs drops as one 80-biomass body via the
	# kill-credit bank; food_per_animal 1.6 outweighs one hunter's carry (per_worker 0.80), so the cap is
	# the CARRIERS needed to haul the peak-turn drop, not ceil(smoothed-rate / per_worker). Sustain
	# (ceiling 0.74) used to read "max 1 useful" (the bug: ceil(0.74/0.80)=1) — it must now read "max 2".
	h._hud._band_labor._player_bands = [BandFx.hunt_preview_local_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	var aurochs := _aurochs_big_game_fixture()
	h._show_herd(aurochs)
	h._compose_herd(aurochs, 1, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_whole_animal_cap")

	# Flip to Deplete — two bodies drop on the peak turn, so the cap climbs to 4: it tracks the selected
	# policy's ceiling, exactly as the smoothed-rate cap did.
	h._hud._compose.set_hunt_floor(ForageFx.DEEP_DRAW_FLOOR)
	h._compose_herd(aurochs)
	await h._settle()
	await h._save("herd_hunt_whole_animal_cap_deplete")

	# States 3r-a / 3r-b — THE ENGAGEMENT BOUND, AS AN A/B ON ONE HERD
	# (`docs/plan_hunt_through_combat.md` §2). A party can only kill what it can get NEAR, and until
	# `engageRate` reached the wire this sheet composed its preview from the crew's CARRY and the stock
	# above the floor alone. Measured in play on a live Wild Fowl herd with ONE hunter: the sheet read
	# **≈307 birds/turn** where the sim pays **ten**, and told the player "max 2 workers useful here —
	# more would be idle" while ~470 birds stood above the floor and each hunter reached ten of them.
	# The number was 30× out and the advice was exactly backwards.
	#
	# **THE PAIR IS THE CLAIM.** One herd, one crew, one floor; the ONLY thing that moves between the
	# two frames is `engage_rate`, and the second publishes `NO_ENGAGEMENT_STAGE` — a pen's own wire
	# value, and what the plant web gets by never publishing the field — so it pins that the arm DROPS
	# rather than merely shrinking, and that forage and corrals are untouched. A lone bounded frame
	# would pass just as well on a sheet that had simply got quieter.
	#
	# Both takes are stated by the harness's own oracle (`HerdFx.hunt_take_oracle`, the sim's
	# `quantise_animal_take` restated in food), never by asking the sheet what it thinks — a check
	# written out of `SourceForecast` would agree with the sheet by construction.
	h._hud._band_labor._player_bands = [_delivered_oracle_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	var fowl_fpa := FOWL_BODY_MASS * FOWL_PROVISIONS_PER_BIOMASS
	var fowl_room := FOWL_BIOMASS - SourceForecast.FLOOR_FOOD_PEAK * FOWL_CAPACITY
	var fowl_ceiling := fowl_room * FOWL_PROVISIONS_PER_BIOMASS
	var fowl_collection := float(FOWL_HUNTERS) * FOWL_PER_WORKER_YIELD
	var reach_take := HerdFx.hunt_take_oracle(fowl_collection, fowl_ceiling, fowl_fpa,
		float(FOWL_HUNTERS) * FOWL_ENGAGE_RATE)
	var carry_take := HerdFx.hunt_take_oracle(fowl_collection, fowl_ceiling, fowl_fpa)
	var reach_face := SourceForecast.format_magnitude(float(reach_take["delivered"]))
	var carry_face := SourceForecast.format_magnitude(float(carry_take["delivered"]))
	# THE TWO CREW TERMS, restated from the sim's own `hunt_haul_workers` / `hunt_engage_workers`: the
	# peak animal drop a ceiling allows is `floor(ceiling / body) + 1`, and the crew is that drop
	# divided by what ONE worker carries / reaches. Composed here, not asked of `SourceForecast`, for
	# the reason the take oracle is.
	var fowl_peak_drop := floorf(fowl_ceiling / fowl_fpa) + 1.0
	var fowl_haul_crew := int(ceilf(fowl_peak_drop * fowl_fpa / FOWL_PER_WORKER_YIELD))
	var fowl_engage_crew := int(ceilf(fowl_peak_drop / FOWL_ENGAGE_RATE))
	var fowl_assignable := int(_delivered_oracle_band()["idle_workers"])
	# The two notes, spelled through the SHIPPED formats so a reworded note fails loudly here rather
	# than silently matching nothing.
	var idle_advice: String = SourceForecast.MAX_USEFUL_NOTE_FORMAT % [fowl_haul_crew,
		SourceForecast.MAX_USEFUL_NOUN_ONE if fowl_haul_crew == 1 \
			else SourceForecast.MAX_USEFUL_NOUN_MANY]
	var wanted_advice: String = SourceForecast.LABOR_BOUND_NOTE_FORMAT % [fowl_assignable,
		fowl_engage_crew]

	var fowl_reaching := _engagement_fowl_herd(FOWL_ENGAGE_RATE)
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(fowl_reaching)
	h._compose_herd(fowl_reaching, FOWL_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_engagement_bound")
	var reaching_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("the take is what one hunter can REACH (%s food/turn), not what it could carry (%s)"
		% [reach_face, carry_face],
		Readout.yields_text(reaching_sheet).contains(reach_face)
			and not Readout.yields_text(reaching_sheet).contains(carry_face))
	# THE ADVICE, WHICH IS THE OTHER HALF OF THE DEFECT. With the reach bound live the herd can use far
	# more hunters than this band HAS, so the stepper is labor-bound and says so — and the sentence
	# claiming more hands would be idle must be nowhere on the sheet.
	h._assert_hud("more hunters are wanted, not idle — the cap reads \"%s\"" % wanted_advice,
		Q.has_label_containing(reaching_sheet, wanted_advice)
			and not Q.has_label_containing(reaching_sheet, idle_advice))
	# **THE THIRD SURFACE THE SAME BOUND HAS TO REACH — the CREW TARGETS and the verdict beneath them.**
	# The take and the cap became engagement-aware while the chart's targets went on dividing the room
	# by the CARRY alone, so this sheet offered `2 clear it now` for a herd two hunters would take ~47
	# turns to clear, and the sentence under it promised the floor next turn. Reported from play on a
	# Red Deer herd, where the same sheet read `6 clear it now` beside a take of six deer a turn.
	#
	# Both terms are composed from the FIXTURE's own wire numbers rather than asked of
	# `SourceForecast` — a target checked against the layer that produces it agrees by construction.
	var fowl_carry_biomass := FOWL_PER_WORKER_YIELD / FOWL_PROVISIONS_PER_BIOMASS
	var fowl_clear_by_carry := int(ceilf(fowl_room / fowl_carry_biomass))
	var fowl_clear_by_reach := int(ceilf(fowl_room / (FOWL_BODY_MASS * FOWL_ENGAGE_RATE)))
	# (0) THE FRAME REALLY SEPARATES THE TWO BOUNDS. Without this the assertion below passes on any
	# sheet whose two answers happen to coincide, and says nothing about which one it read.
	h._assert_hud("the fixture separates the bounds — reaching the room takes %d hands, carrying it %d"
		% [fowl_clear_by_reach, fowl_clear_by_carry],
		fowl_clear_by_reach > fowl_clear_by_carry)
	var fowl_clear := Readout.crew_target_count(reaching_sheet, HudWidgets.CREW_TARGET_CLEAR)
	h._assert_hud("*clear it now* names the crew that can REACH the room in a turn (%d), not carry it (%d)"
		% [fowl_clear_by_reach, fowl_clear_by_carry], fowl_clear == fowl_clear_by_reach)
	# (1) **THE TARGET AND THE READOUT BESIDE IT AGREE**, which is the class of defect this pins: at the
	# crew the pill names, the SIM's own take empties the room, and one hand short of it does not. Both
	# takes come from `HerdFx.hunt_take_oracle` (`quantise_animal_take` restated in food), so the claim
	# is a cross-check against the sim's arithmetic rather than a restatement of the client's.
	var fowl_room_food := floorf(fowl_ceiling / fowl_fpa) * fowl_fpa
	var clear_take := float(HerdFx.hunt_take_oracle(float(fowl_clear) * FOWL_PER_WORKER_YIELD,
		fowl_ceiling, fowl_fpa, floorf(float(fowl_clear) * FOWL_ENGAGE_RATE))["delivered"])
	var short_take := float(HerdFx.hunt_take_oracle(float(fowl_clear - 1) * FOWL_PER_WORKER_YIELD,
		fowl_ceiling, fowl_fpa, floorf(float(fowl_clear - 1) * FOWL_ENGAGE_RATE))["delivered"])
	h._assert_hud("…and at that crew the take really does empty the room (%.4f of %.4f food), one hand short does not"
		% [clear_take, fowl_room_food],
		clear_take >= fowl_room_food and short_take < fowl_room_food)
	# (2) **AND THE VERDICT READS THE SAME PROJECTION.** One hunter reaching ten birds a turn cannot
	# out-take this herd's regrowth, so the crew binds and the sentence must say so; carry-bound, the
	# same hunter moved 40 biomass a turn and the sheet promised the floor. The twin below is what
	# makes this a claim about the ARM rather than about the fixture.
	h._assert_hud("the verdict is the crew's, not a promise of a floor one hunter cannot reach",
		Readout.verdict_severity(reaching_sheet) == SourceForecast.VERDICT_SLOW)
	# (3) **THE ⚠ GATE AND THE VERDICT ARE TWO READINGS OF ONE PROJECTION**, which is the invariant
	# `take_draws_down` was introduced to hold and the one an engagement-blind gate quietly breaks:
	# here the party reaches 1.3 biomass of bird a turn against ~2.5 of regrowth, so the stock RISES —
	# nothing is being overdrawn — while the carry alone (40 a turn) says it falls. Left carry-only the
	# sheet could fly `⚠ OVERDRAWS THE HERD` directly above *it settles at 84% and holds there*.
	#
	# Asserted as the EQUALITY of the two answers, never as the literal `false`: the pairing is the
	# claim, so a fixture that stops rising fails nothing while a gate that stops agreeing fails here.
	var bound_model := SourceForecast.floor_chart_model(fowl_reaching,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, FOWL_HUNTERS, SourceForecast.IMPROVEMENT_NONE, "hunters",
		LESSON_NOT_YET_LEARNED)
	var verdict_falls: bool = float(bound_model["settled_fraction"]) \
		< float(bound_model["stock_fraction"]) - SourceForecast.STOCK_FRACTION_EPSILON
	var gate_falls := SourceForecast.take_draws_down(fowl_reaching,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, FOWL_HUNTERS, SourceForecast.IMPROVEMENT_NONE)
	# The precondition, without which the equality is satisfied by two blind answers agreeing: the
	# CARRY-ONLY walk (`project_stock`'s unbounded default — the pre-fix reading) must say the opposite.
	var carry_only_walk := SourceForecast.project_stock(
		SourceForecast.regrowth_samples(fowl_reaching, HudComposeVocab.BARE_FORECAST_PREFIX),
		FOWL_BIOMASS, FOWL_CAPACITY, SourceForecast.FLOOR_FOOD_PEAK,
		float(FOWL_HUNTERS) * fowl_carry_biomass)
	h._assert_hud("precondition: carry alone would call this herd drawn down (%.3f → %.3f), so the pair is not vacuous"
		% [FOWL_BIOMASS / FOWL_CAPACITY, float(carry_only_walk["settled_fraction"])],
		float(carry_only_walk["settled_fraction"])
			< FOWL_BIOMASS / FOWL_CAPACITY - SourceForecast.STOCK_FRACTION_EPSILON)
	h._assert_hud("the ⚠ gate and the verdict read ONE projection — both say the stock %s"
		% ("falls" if verdict_falls else "rises"), gate_falls == verdict_falls)

	# 3r-b — THE SAME BIRD WITH NO ENGAGEMENT STAGE. This is the pen's wire value and the plant web's
	# silence, and it must read exactly as the sheet always did: carry-bound, and capped at the two
	# haulers who can carry the peak drop.
	var fowl_unreached := _engagement_fowl_herd(SourceForecast.NO_ENGAGEMENT_STAGE)
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(fowl_unreached)
	h._compose_herd(fowl_unreached, FOWL_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_engagement_unbounded")
	var unreached_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("no engagement stage → the take is carry-bound (%s food/turn) as it always was"
		% carry_face,
		Readout.yields_text(unreached_sheet).contains(carry_face)
			and not Readout.yields_text(unreached_sheet).contains(reach_face))
	h._assert_hud("…and the cap is the haul crew again, so \"%s\" is the honest advice here"
		% idle_advice,
		Q.has_label_containing(unreached_sheet, idle_advice)
			and not Q.has_label_containing(unreached_sheet, wanted_advice))
	# **THE CREW TARGETS AND THE VERDICT DROP THE ARM TOO, and this half is the regression that matters
	# most**: a pen and the whole PLANT web publish exactly this value, so a target or a projection that
	# read `NO_ENGAGEMENT_STAGE` as "reaches nothing" would move every forage sheet in the game. The
	# same hunter carrying 40 biomass a turn clears this room in one, so the pill reads the carry
	# quotient and the verdict promises the floor — precisely as it did before the arm existed.
	h._assert_hud("no engagement stage → *clear it now* is the carry quotient (%d) again"
		% fowl_clear_by_carry,
		Readout.crew_target_count(unreached_sheet, HudWidgets.CREW_TARGET_CLEAR) == fowl_clear_by_carry)
	h._assert_hud("…and the verdict reaches the floor again, so the arm DROPS rather than merely shrinking",
		Readout.verdict_severity(unreached_sheet) == SourceForecast.VERDICT_OK)

	# States 3s–3v — the CARRY-AWARE ANIMALS-FIRST local-hunt preview (spec oracle: deer fpa 1.23, band
	# per-worker 0.8, output 1.0, Sustain ceiling 2.33). The preview line reads the crew's HONEST
	# delivered take in animals, not the unquantized food rate the crew could never carry; the policy
	# buttons read "up to X/turn" (the herd's cap, worker-independent).
	h._hud._band_labor._player_bands = [_delivered_oracle_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]

	# 3s — 2 hunters land exactly one whole 1.23 deer/turn, no waste → "≈1 Red Deer/turn · renewable",
	# and the four ascending "up to +2.33 / +3.50 / +5.00 / +7.00 /turn" cap buttons.
	var oracle_clean := _delivered_oracle_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(oracle_clean)
	h._compose_herd(oracle_clean, 2, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_delivered_clean")

	# 3t — 1 hunter can't carry even one whole deer (0.80 < 1.23), so 35% of the kill rots →
	# "≈0.65 Red Deer/turn · ⚠ 35% wasted" (green line, amber waste suffix).
	var oracle_waste := _delivered_oracle_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(oracle_waste)
	h._compose_herd(oracle_waste, 1, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_delivered_waste")

	# 3u — AUTO-MAX on policy select: simulate the picker click path (autofill flag + policy set) starting
	# from a count of 1; the rebuild fills the crew to the Sustain max-useful cap (4 carriers), so the
	# stepper sits at 4 and the line reads the full ≈1.89 deer/turn with zero waste.
	var oracle_automax := _delivered_oracle_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(oracle_automax)
	h._compose_herd(oracle_automax, 1, SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.arm_hunt_autofill()
	h._compose_herd(oracle_automax)
	await h._settle()
	await h._save("herd_hunt_automax")

	# 3v — big game (mammoth fpa 16, Sustain ceiling 2.4): auto-max staffs the 20 carriers, delivered
	# 2.4 → ≈0.15 mammoth/turn, and the averaging-WINDOW hint appears: "≈1 Woolly Mammoth every ~7
	# turns — the rate above is averaged over that span."
	var window_herd := _big_game_window_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(window_herd)
	h._compose_herd(window_herd, 1, SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.arm_hunt_autofill()
	h._compose_herd(window_herd)
	await h._settle()
	await h._save("herd_hunt_big_game_window")
	h._assert_compose_sheet_fits("herd_hunt_big_game_window")

	# 3w — THE INEDIBLE QUARRY (issue #337). A wolf pays PELTS AND NO MEAT: `provisions == 0` on every
	# rung, a real trade ceiling on all four. This is the frame the whole arc is judged on. The picker's
	# four buttons must read FOUR ASCENDING TRADE numbers on their second line — `0.90 / 1.35 / 1.95 /
	# 2.70 trade` — with NO food term and NO zeros anywhere; before the fix the client read only food, so all four read `+0.00`
	# and the pack rendered as a source worth nothing. The preview line below the picker must still show a
	# per-turn ANIMAL rate (the ratio is unit-free — it divides by the TRADE quantum, since the food one
	# is honestly 0), and the averaging-window disclaimer must still appear.
	var wolf := _pelt_only_wolf_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(wolf)
	# Crew + rung go through `_compose_herd`, which dials them in AFTER the source-change re-seed.
	h._compose_herd(wolf, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_pelts_only")
	h._assert_compose_sheet_fits("herd_hunt_pelts_only")
	# **THE REGRESSION THAT MATTERS MOST once the readout credits every account a take pays.** The
	# rule is render-only-where-the-vector-PAYS, not "render every account": a wolf's provisions rate
	# is a structural 0, so the crossing into food answers a structural zero and `yield_rows` emits NO
	# food row — never the `0.00 FOOD` that says its pelts are worth no meat. Asserted as a PAIR, since
	# the negative alone is satisfied by a readout that lost both accounts.
	var wolf_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("an inedible quarry's PER TURN row states the TRADE it pays…",
		wolf_yields.contains(SourceForecast.YIELD_ACCOUNT_UNITS[
			SourceForecast.YIELD_ACCOUNT_TRADE].to_upper())
			and _yield_take(wolf_yields, SourceForecast.YIELD_ACCOUNT_UNITS[
				SourceForecast.YIELD_ACCOUNT_TRADE].to_upper()) > 0.0)
	h._assert_hud("…and NO food row beside it — a wolf pays pelts and no meat, ever",
		not wolf_yields.contains(SourceForecast.YIELD_ACCOUNT_UNITS[
			SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()))
	# **THE CHART ON AN INEDIBLE QUARRY** (the wolf half of the five chart cases). The readout above it
	# carries no food line at all, and the chart must not care: a floor is a fraction of BIOMASS, and
	# the crew targets divide by `perWorkerBiomass`, which is positive on a wolf where both the food
	# rate and `perWorkerYield` are honestly `0`. That is precisely why the field exists — the old
	# `perWorkerYield / provisionsPerBiomass` recovery is `0/0` on this animal.
	h._assert_hud("a wolf's chart draws — a floor is biomass, and biomass is what this species has",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) != null)
	h._assert_hud("…and its crew targets are priced off the biomass throughput, not the absent food one",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR)
			> Readout.CREW_TARGET_ABSENT)
	# **A CLICKABLE TARGET THE STEPPER BESIDE IT CANNOT REACH IS THE PANEL ARGUING WITH ITSELF** (§7.2),
	# and the wolf is where that was found: `5 hold it after` sat under `max 4 workers useful here`,
	# because the cap answered "hands that clear what stands THIS turn" and the target answered "hands
	# that take the regrowth EVERY turn" — and the cap was the one that was wrong (a source AT its floor
	# has no room, so it capped at 0 while a positive crew was needed next turn). `max_useful_workers`
	# now floors on the hold crew, so the press below lands the stepper on exactly the number the button
	# offered. Driven through the REAL button, since the clamp that used to swallow it lives in the press
	# handler rather than in the arithmetic.
	var wolf_hold = Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
	h._assert_hud("the wolf states a hold-it-after crew to click at all", wolf_hold > 0)
	Q.find_crew_target(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD).pressed.emit()
	h._assert_hud("…and the stepper reaches that crew instead of clamping it to a smaller cap",
		h._hud._compose.hunt_count() == wolf_hold)

	# State floor_chart_herd_allee — **THE HERD BELOW ITS ALLEE POINT, and the frame the whole sampled
	# curve exists for.** Under `collapse_fraction` a herd's regrowth samples are NEGATIVE: it declines
	# every turn whether or not anyone hunts it. The projection must therefore fall AWAY from the floor
	# toward extinction. Clamping those samples to zero is the instinctive thing to do with a chart and
	# it would draw this herd sitting still — the exact asymmetry that makes floor 0 end a herd and
	# only set a patch back (compare `floor_chart_drawn_down`, whose curve flattens onto its floor).
	var allee_herd := ForageFx.floorify(HerdFx.collapsing_herd_fixture())
	allee_herd["biomass"] = FLOOR_CHART_ALLEE_STOCK_FRACTION * float(allee_herd["carrying_capacity"])
	# The band is the wolf state's, deliberately — this frame is about the HERD's curve, and swapping
	# the actor would put a second variable in a comparison the reader is meant to make against it.
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(allee_herd)
	h._compose_herd(allee_herd, ForageFx.FLOOR_CHART_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("floor_chart_herd_allee")
	# The PNG shows the decline; this is the half it cannot testify to — that the samples themselves
	# are negative down there, which is what the projection reads and what a clamp would erase.
	h._assert_hud("the herd's curve is NEGATIVE below its Allee point — decline, not stillness",
		SourceForecast.regrowth_at(SourceForecast.regrowth_samples(allee_herd,
			HudComposeVocab.BARE_FORECAST_PREFIX), FLOOR_CHART_ALLEE_STOCK_FRACTION) < 0.0)
	h._assert_hud("…while the plant curve never is, at the same fraction of its own capacity",
		SourceForecast.regrowth_at(SourceForecast.regrowth_samples(h._floor_chart_drawn_patch,
			HudComposeVocab.FORAGE_FORECAST_PREFIX), FLOOR_CHART_ALLEE_STOCK_FRACTION) >= 0.0)

	# **A VERDICT MAY NOT PROMISE AN AFTERMATH THE SOURCE HAS NO WAY TO REACH.** Reported from play: a
	# Rabbit Warren at `Take everything` read `0 hold it after` beside "Reaches the floor in 2 turns,
	# then holds it — taking only what grows back". The herd is GONE at floor 0; there is nothing to
	# hold and nothing that grows back, and the panel was contradicting its own crew target.
	#
	# **The discriminator is the REGROWTH at that floor, not the web and not floor 0**, and this pair
	# is what pins that: the same floor on a PATCH keeps the full sentence, because a stripped patch
	# reseeds from bare ground and genuinely does hold at 0 paying what grows back. A fix that branched
	# on "fauna" or on "floor == 0" would pass the herd line below and fail the patch line under it.
	var strip_crew := 64
	var stripped_herd := SourceForecast.floor_chart_model(allee_herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_MIN, strip_crew,
		SourceForecast.IMPROVEMENT_NONE, "hunters", LESSON_NOT_YET_LEARNED)
	var stripped_patch = SourceForecast.floor_chart_model(h._floor_chart_drawn_patch,
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_MIN, strip_crew, SourceForecast.IMPROVEMENT_NONE, "foragers",
		LESSON_NOT_YET_LEARNED)
	var stripped_herd_text := String((stripped_herd.get("verdict", {}) as Dictionary).get("text", ""))
	var stripped_patch_text = String((stripped_patch.get("verdict", {}) as Dictionary).get("text", ""))
	h._assert_hud("both stripped sources REACH their floor, so both are stating the reaching verdict",
		stripped_herd_text.contains("Reaches the floor")
			and stripped_patch_text.contains("Reaches the floor"))
	h._assert_hud("a herd taken to nothing is not promised that it holds what grows back",
		not stripped_herd_text.contains("grows back"))
	h._assert_hud("…while a patch at the same floor still is — it reseeds, so the clause is TRUE there",
		stripped_patch_text.contains("grows back"))
	# **THE LINE THAT RULES OUT THE PLAUSIBLE WRONG FIX.** Branching on `kind != SOURCE_KIND_HERD`
	# passes both assertions above — the two fixtures there make "is a herd" and "cannot regrow"
	# coincide, so the sabotage changed no output and the pair testified to nothing. A HEALTHY herd
	# above its floor regrows at that floor like anything else and must KEEP the clause; that is the
	# case a web branch gets wrong, and the only one of the three that can see the difference.
	var held_herd := SourceForecast.floor_chart_model(
		ForageFx.floorify(HerdFx.grazing_healthy_herd_fixture()), SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, strip_crew,
		SourceForecast.IMPROVEMENT_NONE, "hunters", LESSON_NOT_YET_LEARNED)
	var held_herd_text := String((held_herd.get("verdict", {}) as Dictionary).get("text", ""))
	h._assert_hud("a HERD that still regrows at its floor keeps the clause — it is the growth, not the web",
		held_herd_text.contains("Reaches the floor") and held_herd_text.contains("grows back"))

	# **THE FLOOR FLAG'S UNIT AND ITS ORDER**, which no PNG can testify to at 10px. Asserted against
	# hand-built models rather than the live sheet so both webs are reachable from one place and the
	# expected strings are computable by eye: 1075 ÷ 100 = 10.75 → 11 animals at floor 0.50.
	#
	# The ORDER is the assertion that matters, and it is now the SAME on both. An animal count over a K
	# of ~21 has ~21 states where biomass had one per FLOOR_STEP, so an animal-FIRST flag would sit
	# unmoved across a tenth of the drag and read as a stuck control; the percent leads to keep the flag
	# responsive, and once it must lead on fauna the patch follows it so one control cannot swap its
	# terms mid-session. `==` (not `contains`) is what pins the order — a `contains` passes on either.
	var flag_probe := HarvestFloorChart.new()
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK,
		"capacity": 2150.0, "body_mass": 100.0, "quarry": "Red Deer"})
	h._assert_hud("a HERD's floor flag counts animals, after the percent",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer")
	# THE OTHER WEB: a patch has no body, so its quantity stays biomass — no `≈`, no species — while
	# the ORDER around it is identical. Without this the suite could not tell "fauna converted" from
	# "everything converted", and could not see the patch's percent silently moving back to the tail.
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK, "capacity": 195.0})
	h._assert_hud("…and a PATCH's states biomass, in the same order and with no animal count",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 97.5) == "leave 50% · 98")
	flag_probe.free()
	# The conversion itself, on literals. `animal_count` is the ONE place biomass becomes a head count
	# (the drawer row and the flag both read it), so its two edges are worth stating outright: a
	# species with no `body_mass` on the wire yields no count at all, and a herd holding a FIFTH of a
	# body counts ONE, never the rounded zero — it is alive on the map and the sim's kill step floors
	# at one body too.
	h._assert_hud("body mass turns biomass into animals",
		SourceForecast.animal_count(820.0, 100.0) == 8)
	h._assert_hud("…a herd under one body still counts one, never zero",
		SourceForecast.animal_count(19.0, 100.0) == 1)
	h._assert_hud("…and a species with no body mass has no count to state",
		SourceForecast.animal_count(820.0, 0.0) == SourceForecast.ANIMAL_COUNT_NONE)
	# **THE FLAG AND THE VERDICT NAME ONE THRESHOLD, so they must name it in one unit.** Caught in a
	# frame, not in review: this sheet read `leave 50% · ≈11 Red Deer` over "grows past 1075". Both now
	# render the quantity through `stock_face`, and this is the assertion that says so — the verdict's
	# sentence must CONTAIN what the flag flies, on the same model.
	var at_floor := SourceForecast.harvest_verdict({"reached_turn": SourceForecast.PROJECTION_REACHED_NONE,
		"settled_fraction": 0.0, "series": []}, ForageFx.FLOOR_CHART_CREW, 96.0, 2150.0,
		SourceForecast.FLOOR_FOOD_PEAK, 0, "hunters", 100.0, "Red Deer")
	h._assert_hud("the at-floor verdict quotes the threshold in the SAME unit the flag flies",
		String(at_floor.get("text", "")).contains("≈11 Red Deer")
			and not String(at_floor.get("text", "")).contains("1075"))

	# 3x — the same wolf as an EXPEDITION target (band 27 tiles off). `delivers_food = false` on every
	# cell now means THE QUARRY IS INEDIBLE, not "a denial mission", so the raid line must read a real
	# delivery whose payload is trade goods — `delivers ≈5 Grey Wolf over ≈9 turns · ⇄ ~7 trade goods` —
	# and the Send button must be the ordinary primary send, NOT "brings nothing home".
	var wolf_raid := _pelt_only_wolf_raid_herd()
	h._hud._band_labor._player_bands = [_hunt_preview_far_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(wolf_raid)
	h._compose_herd(wolf_raid, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_pelts_raid")

	# 3y — THE BOTH-PRODUCTS CONTROL: the same oracle deer, whose hide sells beside its meat. Each picker
	# button's product line must carry BOTH components with FOOD LEADING (`2.33 food · 0.34 trade`), which
	# is the half of the rule the wolf frame cannot prove. Rendered right after the wolf so the two are
	# compared directly. Both frames also judge the TWO-LINE FACE itself: the rung's name over its
	# products, so `which rung` and `what it pays` stop competing in one line of glyphs.
	h._hud._band_labor._player_bands = [_delivered_oracle_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	var oracle_pair := _delivered_oracle_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(oracle_pair)
	h._compose_herd(oracle_pair, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_both_products")
	# **THE READOUT'S HALF OF THE PAIR — one animal count, valued in BOTH accounts.** The picker faces
	# above have named the pair since #337; the `PER TURN` row beneath them did not, and reported from
	# play a Wild Boar's compose sheet read `0.00 FOOD` with no trade row while an expedition on the
	# same species read `20.00 FOOD  2.50 TRADE`. A quantised take must be COUNTED on one axis (a
	# wolf's food quantum is honestly 0, so nothing else may divide), but the count is unit-free: the
	# sim quantises on `ratio_axis()` and then values that count in every currency the species pays
	# (`YieldPair::rescaled_to`). The client stopped at the axis it had quantised on.
	var pair_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	var food_unit: String = SourceForecast.YIELD_ACCOUNT_UNITS[
		SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()
	var trade_unit: String = SourceForecast.YIELD_ACCOUNT_UNITS[
		SourceForecast.YIELD_ACCOUNT_TRADE].to_upper()
	h._assert_hud("a local hunt's PER TURN row names BOTH accounts this take pays",
		pair_yields.contains(food_unit) and pair_yields.contains(trade_unit))
	# THE MAGNITUDES, recomposed from the sim's own two steps: `quantise_animal_take` for the count
	# (the harness's `HerdFx.hunt_take_oracle`, in food), then the species' whole-animal quanta for the
	# crossing into trade. The client rescales through the per-biomass VECTOR instead, so the two
	# arrive at the same pair by different routes rather than by construction — and a fix that valued
	# the second account off the per-worker rates (0.12 / 0.80, a mix the crew's carry has no business
	# supplying) misses it.
	var pair_food := float(HerdFx.hunt_take_oracle(PELT_FRAME_HUNTERS * ORACLE_DEER_PER_WORKER,
		ORACLE_DEER_SUSTAIN_CEILING, ORACLE_DEER_FOOD_PER_ANIMAL)["delivered"])
	var pair_trade := pair_food * ORACLE_DEER_TRADE_PER_ANIMAL / ORACLE_DEER_FOOD_PER_ANIMAL
	h._assert_hud("…the FOOD reading is the crew's quantised take (%s)"
		% SourceForecast.format_magnitude(pair_food),
		is_equal_approx(_yield_take(pair_yields, food_unit),
			float(SourceForecast.format_magnitude(pair_food))))
	h._assert_hud("…and the TRADE reading is that SAME take in the other currency (%s)"
		% SourceForecast.format_magnitude(pair_trade),
		is_equal_approx(_yield_take(pair_yields, trade_unit),
			float(SourceForecast.format_magnitude(pair_trade))))

	# 3z — THE INVESTMENT-RUNG TWIN of 3y (issue #397). The extractive rungs above have paid a pair since
	# #337, but Tame and Corral rendered a FOOD-ONLY payoff face — a Wild Boar read `→ 1.48 food` beside
	# its own extractive rungs' `0.74 food · 0.18 trade`, silently dropping a trade half the sim exports
	# (`pastoralTrade` / `corralTrade`). A prepared herd pays the same two products a hunted one does, so
	# the payoff obeys the same render-only-when-non-zero rule: both faces must name FOOD THEN TRADE.
	# Domestication is mid-ladder on purpose — Tame retires from the picker once the herd is fully tamed,
	# and Corral is knowledge-gated below that, so a frame carrying BOTH rungs necessarily has one greyed.
	# A gated rung still wears its payoff (that is the point of showing it), which this frame also proves.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	var payoff_boar := HerdFx.investment_pair_boar_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(payoff_boar)
	# TAME RUNNING: its own payoff rides the checked box's own face, exactly as the offered box's does
	# below — which is what makes the pair of assertions here a comparison of two STATES of one control
	# rather than of two different widgets.
	h._compose_herd(payoff_boar, PELT_FRAME_HUNTERS, ForageFx.COMPOSE_FLOOR_UNSET, "tame")
	await h._settle()
	await h._save("herd_investment_both_products")
	h._assert_hud("Tame's payoff names BOTH products, food leading",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, HudConst.LABOR_POLICY_TAME)
			.ends_with(BOAR_TAME_PAYOFF_FACE))
	# CORRAL OFFERED: the boar is fully tamed here, so Tame is DONE and Corral is the rung on offer —
	# its payoff quoted on the checkbox's own face, which is where a not-yet-started rung states terms.
	var penned_boar := HerdFx.investment_pair_boar_herd()
	penned_boar["domestication"] = 1.0
	h._hud._compose.reset_hunt_source()
	h._show_herd(penned_boar)
	h._compose_herd(penned_boar, PELT_FRAME_HUNTERS)
	await h._settle()
	await h._save("herd_investment_corral_offer")
	var corral_offer = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "corral")
	h._assert_hud("Corral's offered face names BOTH products, food leading",
		corral_offer is CheckBox
		and ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL).ends_with(BOAR_CORRAL_PAYOFF_FACE))

	# Reset so later states render their usual single-band dropdown + default band/policy.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# ---- THE FIGHT, SAID BEFORE THE PARTY LEAVES (§2.1 / §6.5) -----------------------------------
	await _combat_gate_states()

	# ---- THE FORECAST REPORTS A RANGE (§6.4) -----------------------------------------------------
	await _yield_range_states()

	# ---- AN EMPTY RAID IS EMPTY FOR ONE OF TWO REASONS --------------------------------------------
	await _unkillable_quarry_states()


# =====================================================================================
#  THE PRE-LAUNCH FIGHT (`docs/plan_hunt_through_combat.md` §2.1, §4.2, §6.5)
# =====================================================================================
# The gate produces a real outcome that reads as a bug unexplained: **hunters die and nothing is
# killed.** The client holds every term it needs — the band's resolved `hunterAttack`, the herd's
# `defense` and its newly-exported `durability` — so the sim exports no verdict and the sheet asks
# itself the question. Beside it, `1 / engageRate` turns `0.05` into *"twenty hunters to take a
# mammoth"*, which is a number a player can size a party against.

## The mammoth's own two defensive axes, at the roster's settled values (§4.2). They must not be
## blurred: **`defense` is whether a hit counts at all, `durability` is how many counting hits it
## takes** — so the first decides the refusal and the second the effort figure, and a fixture that
## moved one would move only one of the two assertions below.
const GATE_MAMMOTH_DEFENSE := 12.0

const GATE_MAMMOTH_DURABILITY := 500.0

## §2.1's own row: twenty hunters can surround one mammoth. Stated as the wire's `engageRate` and
## INVERTED by the assertion rather than restated, so the harness and the readout arrive at 20 from
## opposite ends.
const GATE_MAMMOTH_ENGAGE_RATE := 0.05

## A herd carrying ALL THREE of the fight's terms — the two above plus the engagement stage that gates
## whether either line is spoken at all. Built on the deadly-herd mammoth, which already carries the
## `defense 12` the refusal is judged on, so this fixture adds the two fields the arc appended and
## changes nothing else about the animal.
func _combat_gate_mammoth() -> Dictionary:
	var herd := HerdFx.deadly_herd_fixture()
	herd["defense"] = GATE_MAMMOTH_DEFENSE
	herd["durability"] = GATE_MAMMOTH_DURABILITY
	herd["engage_rate"] = GATE_MAMMOTH_ENGAGE_RATE
	return herd

## **A PEN CARRYING THE FIGHT'S TERMS AND STILL SAYING NOTHING.** The herd is corralled, so it
## publishes `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked — while keeping a real `defense`
## and `durability`. That is what makes the negative below a claim about the ENGAGEMENT GATE rather
## than about a fixture that simply omitted the fields: strip the gate and this sheet grows both
## lines, which is the byte-identity this arc has to hold for the pen and the whole plant web.
func _combat_gate_pen() -> Dictionary:
	var herd := HerdFx.domesticated_herd_fixture()
	herd["defense"] = GATE_MAMMOTH_DEFENSE
	herd["durability"] = GATE_MAMMOTH_DURABILITY
	herd["engage_rate"] = SourceForecast.NO_ENGAGEMENT_STAGE
	return herd

func _combat_gate_states() -> void:
	var quarry := String(HerdFx.deadly_herd_fixture()["species"])
	var mammoth := _combat_gate_mammoth()
	# **THE HUNTERS-PER-ANIMAL FIGURE, COMPOSED FROM THE OTHER SIDE OF THE QUOTIENT.** The sheet
	# divides `1` by `engageRate`; the harness multiplies back up, so the two agree only if the
	# readout really is the inverse rather than a number that happens to look right.
	var hunters_per_animal := ceili(1.0 / GATE_MAMMOTH_ENGAGE_RATE)
	var reach_line: String = SourceForecast.HUNTERS_PER_ANIMAL_FORMAT % [hunters_per_animal, quarry]

	# State gate-a — A SPEARED PARTY, above the gate. The sheet states what the fight COSTS rather
	# than only whether it is hopeless: `durability / (attack − defense)` hunter-turns per kill, which
	# is what makes a mammoth (62.5) and a rabbit comparable at all.
	var speared := BandFx.with_equipped_kit(BandFx.hunt_preview_local_band())
	h._hud._band_labor._player_bands = [speared]
	h._hud._band_labor._player_band = speared
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(mammoth)
	h._compose_herd(mammoth, LOCAL_HUNT_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_gate_effort")
	var speared_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("twenty hunters reach one mammoth — \"%s\"" % reach_line,
		Readout.hunters_per_animal_line(speared_sheet) == reach_line)
	# **THE EFFORT FIGURE IS NOT DIVIDED BY THE PARTY**, deliberately: the herd's accumulated wounds
	# are not exported, so a per-party turn count here would be a second duration model competing
	# with the sim's own `huntTripEstimates`. It is hunter-turns for ONE hunter, and it is what turns
	# a bare refusal into something a player can plan against.
	var hunter_turns := GATE_MAMMOTH_DURABILITY / (BandFx.KIT_ATTACK_EQUIPPED - GATE_MAMMOTH_DEFENSE)
	h._assert_hud("a party above the gate is quoted the EFFORT (%s hunter-turns), not a refusal"
		% String.num(hunter_turns, SourceForecast.HUNT_GATE_EFFORT_DECIMALS),
		Readout.hunt_gate_blocked(speared_sheet) == Readout.HUNT_GATE_WINNABLE
			and Readout.hunt_gate_line(speared_sheet).contains(
				String.num(hunter_turns, SourceForecast.HUNT_GATE_EFFORT_DECIMALS)))

	# State gate-b — THE SAME MAMMOTH, THE SAME PARTY SIZE, BARE HANDS. `max(0, 1 − 12)` is zero, so
	# no headcount kills anything and the party takes casualties for nothing. **The only thing that
	# moves between the two frames is the band's kit**, which is what makes this an A/B on the WEAPON
	# rather than on the animal — §4.8's whole claim that the first spear should feel like a different
	# game.
	var bare := BandFx.with_bare_hands(BandFx.hunt_preview_local_band())
	h._hud._band_labor._player_bands = [bare]
	h._hud._band_labor._player_band = bare
	h._hud._compose.reset_hunt_source()
	h._show_herd(mammoth)
	h._compose_herd(mammoth, LOCAL_HUNT_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_gate_blocked")
	var bare_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("bare hands against a mammoth is refused IN WORDS, before the party is sent",
		Readout.hunt_gate_blocked(bare_sheet) == Readout.HUNT_GATE_BLOCKED)
	h._assert_hud("…and the refusal names BOTH terms, so the lesson is the weapon and not the headcount",
		Readout.hunt_gate_line(bare_sheet).contains(
				String.num(BandFx.KIT_ATTACK_BARE, SourceForecast.HUNT_GATE_SCALAR_DECIMALS))
			and Readout.hunt_gate_line(bare_sheet).contains(
				String.num(GATE_MAMMOTH_DEFENSE, SourceForecast.HUNT_GATE_SCALAR_DECIMALS)))
	# **CONTACT IS NOT THE GATE** (§2.1): twenty bare-handed hunters DO walk up to a mammoth, and the
	# fight is where they fail. So the reach line must be unchanged by the kit — a sheet that hid it
	# alongside the refusal would teach that the party cannot find the animal.
	h._assert_hud("…while the engagement figure is untouched — contact is not the gate",
		Readout.hunters_per_animal_line(bare_sheet) == reach_line)

	# **THE NEGATIVE, AND IT IS THE HALF THE ARC KEEPS BREAKING.** A PEN publishes no engagement
	# stage, so neither line may render on one — and this fixture carries a real `defense` and
	# `durability`, so the silence is the ENGAGEMENT GATE's doing rather than a fixture that omitted
	# the terms. PNG-less: the claim is an absence, which a picture states only by not showing
	# something, and the frame set's byte-diff is where a regression would actually surface.
	var pen := _combat_gate_pen()
	h._hud._compose.reset_hunt_source()
	h._show_herd(pen)
	h._compose_herd(pen, LOCAL_HUNT_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	var pen_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a PEN is not stalked and not fought — neither pre-launch line renders on one",
		Readout.hunters_per_animal_line(pen_sheet) == ""
			and Readout.hunt_gate_blocked(pen_sheet) == Readout.HUNT_GATE_ABSENT)

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)


# =====================================================================================
#  THE FORECAST REPORTS A RANGE (`docs/plan_hunt_through_combat.md` §6.4)
# =====================================================================================
# `actualYield` became the take's EXPECTATION over the retreat seed, and the pair around it says how
# far the real take can land from it. **It ships DEGENERATE** — wariness is `0` across the roster and
# `hit_chance` is `1.0`, so every stage takes its exact identity at every quantile and `low == actual
# == high` bit-for-bit — which means the range UI has to be correct and currently near-invisible.
# Slice 7 authors wariness and it turns on with no further client work.
#
# **SO THE PAIR OF STATES IS THE CLAIM.** A band renders where the bounds differ, and NO band renders
# where they agree; the second is what stops the first passing on a readout that decorates every row,
# and it is the case every source in the game is in today.

## A real band on this turn's take — the spec's own worked example, *"6–11, likely 9"*, in the food
## units a deer hunt actually pays. `actual` sits between them because it is the EXPECTATION, which is
## what `forecast == actual` is restated on.
const RANGE_ACTUAL := 0.90

const RANGE_LOW := 0.60

const RANGE_HIGH := 1.10

## The band's own hunt assignment, with the pair either SPREAD or collapsed onto the expectation.
## One builder for both states, so the only thing that differs between them is the two bounds —
## a second fixture could drift in a term the assertion does not name.
func _range_band(low: float, high: float) -> Dictionary:
	var band := BandFx.hunt_preview_local_band()
	band["labor_assignments"] = [{
		"kind": "hunt", "workers": 2, "fauna_id": HerdFx.herd_fixture()["id"],
		"target_x": 66, "target_y": 10, "floor": SourceForecast.FLOOR_FOOD_PEAK,
		"actual_yield": RANGE_ACTUAL, "sustainable_yield": RANGE_ACTUAL,
		"realized_yield": RANGE_ACTUAL, "workers_needed": 2, "overdraws": false,
		SourceForecast.YIELD_RANGE_LOW_KEY: low,
		SourceForecast.YIELD_RANGE_HIGH_KEY: high,
	}]
	return band

func _yield_range_states() -> void:
	var herd := HerdFx.herd_fixture()
	var band_clause: String = SourceForecast.YIELD_RANGE_CLAUSE_FORMAT \
		% SourceForecast.format_yield_range(RANGE_LOW, RANGE_HIGH)

	# State range-a — A REAL BAND. The row's headline stays the steady realized rate (the band is
	# about this turn's expectation, which is a different number), and the spread rides beside it in
	# the muted register the wasted note already uses.
	var spread := _range_band(RANGE_LOW, RANGE_HIGH)
	h._hud._band_labor._player_bands = [spread]
	h._hud._band_labor._player_band = spread
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	await h._settle()
	await h._save("herd_hunt_yield_range")
	h._assert_hud("a stochastic take states its band beside the expectation — \"%s\"" % band_clause,
		Q.has_label_containing(h._hud, band_clause))

	# State range-b — **THE DEGENERATE CASE, PINNED.** Every source in the game reports this today, so
	# a band appearing here is a defect rather than a rarity — and without this half the state above
	# passes just as well on a readout that draws a range unconditionally.
	var point := _range_band(RANGE_ACTUAL, RANGE_ACTUAL)
	h._hud._band_labor._player_bands = [point]
	h._hud._band_labor._player_band = point
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	await h._settle()
	await h._save("herd_hunt_yield_point")
	h._assert_hud("…and where the distribution is a POINT no band is drawn at all",
		not Q.has_label_containing(h._hud, SourceForecast.YIELD_RANGE_CLAUSE_FORMAT % ""))

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()


# =====================================================================================
#  AN EMPTY RAID IS EMPTY FOR ONE OF TWO REASONS, AND THEY ARE OPPOSITES
# =====================================================================================
# Reported from play. A THRIVING Wild Aurochs herd — ten of eleven animals standing, four of them
# affordable above a 50% floor — was refused to a party of one as *"too lean to raid — its surplus is
# spent"*, over a line two rows up saying it takes several hunters to bring ONE aurochs into contact.
# The herd was not the problem; the party was. Since the take started resolving through the fight
# (`docs/plan_hunt_through_combat.md` §4) a raid delivers nothing for two unrelated reasons with
# OPPOSITE remedies — wait for the herd to rebuild, against send more hunters — and the refusal named
# the wrong one, which is worse than naming no one.
#
# **THE SIM ALREADY TELLS THEM APART**, so the frames are an A/B on `HuntTripBound` alone: this herd's
# every cell is `horizon` (the projection ran out with the party still empty-handed), while
# `_no_surplus_herd`'s every cell is `floor` (the standing surplus is spent). Both deliver ZERO in
# both currencies, so a client reading the numbers cannot separate them and every claim below is
# really a claim about reading the sim's answer instead.

## The reported herd's own terms — `B/K` a hair over 0.9, i.e. unmistakably NOT at its floor, which is
## the whole premise the old refusal contradicted. Ten of eleven animals, so `body_mass` and the pair
## agree by construction rather than by two roundings landing together.
const AUROCHS_CAPACITY := 1320.0

const AUROCHS_BIOMASS := 1200.0

const AUROCHS_BODY_MASS := 120.0

## A clean binary rate, so the sheet's `1 / engageRate` inversion lands on a whole number rather than
## on `6.000000000000001` — the reported herd's own figure was ~6, and what the frame has to hold is
## the SHAPE (several hunters per animal), not that particular quotient.
const AUROCHS_ENGAGE_RATE := 0.25

## The two readings of that rate, stated from the harness's side so the sheet and the fixture arrive
## at the same number from opposite ends.
const AUROCHS_HUNTERS_PER_ANIMAL := 4

## **THE SIM'S `fauna::hunt_engage_workers`, RESTATED FROM THIS FIXTURE'S OWN NUMBERS** — the crew that
## brings the herd's peak animal drop into contact in one turn, which is the ceiling the party stepper
## must report. `room = 1200 − 0.5 × 1320 = 540`; `peak drop = floor(540 / 120) + 1 = 5`;
## `crew = ceil(5 / 0.25) = 20`. Written out rather than asked of `SourceForecast`, because a ceiling
## read back from the code under test could only ever confirm itself.
const AUROCHS_ENGAGE_CREW := 20

## A per-biomass rate that keeps the herd's food quantum a round number
## (`food_per_animal = body_mass × this`), so nothing in the readout reads as noise.
const AUROCHS_PROVISIONS_PER_BIOMASS := 0.02

## The band's kit against the aurochs' two defensive axes. **The fight is WINNABLE and slow**, which is
## the case the arc is about: a blocked gate is already covered by `herd_hunt_gate_blocked`, and a
## refusal that named the party would be unsurprising there. `durability / (attack − defense)` =
## `160 / (20 − 5)` ≈ 10.7 hunter-turns for ONE hunter — enough that a party of one lands nothing
## inside the projection's horizon, and nowhere near "cannot hurt it".
const AUROCHS_DEFENSE := 5.0

const AUROCHS_DURABILITY := 160.0

## The party the case was reported at, and the smallest one that can hold the claim: one hunter, four
## animals standing affordable, and a herd that needs twenty hands to be reached at all.
const AUROCHS_PARTY := 1

## **THE HERD WITH REAL SURPLUS AND A PARTY THAT CANNOT TOUCH IT.** Every sampled cell delivers zero in
## both currencies while the herd stands at 0.9 of its capacity, and every cell carries
## `TRIP_BOUND_HORIZON` — the sim's own statement that the projection ran its whole length without the
## party filling a pack, spending the surplus or losing the herd. `delivers_food` stays TRUE: the
## quarry is edible (that flag says so since #337), and a fixture that cleared both flags would stage a
## DENIAL mission instead, which is a different branch entirely.
func _unkillable_aurochs_herd() -> Dictionary:
	var herd := HerdFx.assign_preview_herd("game_aurochs_05", "Wild Aurochs", "thriving",
		AUROCHS_PROVISIONS_PER_BIOMASS * (AUROCHS_BIOMASS
			- SourceForecast.FLOOR_FOOD_PEAK * AUROCHS_CAPACITY), 0, 0)
	herd["biomass"] = AUROCHS_BIOMASS
	herd["carrying_capacity"] = AUROCHS_CAPACITY
	# Stated rather than derived, so the room, the peak drop and the engagement crew above are one
	# arithmetic chain the adapter cannot re-seed underneath.
	herd["body_mass"] = AUROCHS_BODY_MASS
	herd["food_per_animal"] = AUROCHS_BODY_MASS * AUROCHS_PROVISIONS_PER_BIOMASS
	herd["engage_rate"] = AUROCHS_ENGAGE_RATE
	herd["defense"] = AUROCHS_DEFENSE
	herd["durability"] = AUROCHS_DURABILITY
	var table := {}
	for w in range(1, 9):
		for stance in BaseFx.LEGACY_STANCE_FLOORS:
			table["%s:%d" % [String(stance), w]] = {
				# `turns_to_fill == 0` and `bound == horizon` are ONE statement of the same fact and
				# must move together: the projection never completed.
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": false,
				"animals_taken": 0, "delivered_food": 0.0, "delivered_trade": 0.0,
				"wasted_food": 0.0,
				SourceForecast.TRIP_BOUND_KEY: SourceForecast.TRIP_BOUND_HORIZON,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## The three faces of one refusal, read out of the ONE table the client composes them from — so the
## assertions cannot drift from the copy, and a reworded sentence moves the expectation with it.
func _empty_refusal(bound: String, face: String, quarry: String) -> String:
	var entry: Dictionary = SourceForecast.HUNT_EMPTY_REFUSALS[bound]
	return String(entry[face]) if face == "button" else String(entry[face]) % quarry

func _unkillable_quarry_states() -> void:
	var aurochs := _unkillable_aurochs_herd()
	var quarry := String(aurochs["species"])
	# A band beyond `hunt_reach` (so the sheet takes the EXPEDITION branch) carrying a real
	# `hunter_attack`, without which the combat gate's own line — the one the refusal used to
	# contradict — would not render at all.
	var band := BandFx.with_equipped_kit(_hunt_preview_far_band())
	h._hud._band_labor._player_bands = [band]
	h._hud._band_labor._player_band = band
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(aurochs)
	h._compose_herd(aurochs, AUROCHS_PARTY, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_party_cannot_kill")
	var sheet: Control = h._hud._drawercompose._compose_sheet

	# **THE REFUSAL NAMES THE PARTY, AND SAYS THE HERD IS NOT THE PROBLEM.** Asserted as a PAIR with
	# the herd-side sentence's absence, because a table that answered one refusal for every bound
	# would satisfy a lone positive — and that table is exactly what shipped.
	var party_line := _empty_refusal(SourceForecast.TRIP_BOUND_HORIZON, "line", quarry)
	var herd_line := _empty_refusal(SourceForecast.TRIP_BOUND_FLOOR, "line", quarry)
	h._assert_hud("a raid this party cannot kill is refused in the PARTY's name — \"%s\"" % party_line,
		Q.has_label_containing(sheet, party_line)
			and not Q.has_label_containing(sheet, herd_line))

	# **THE SEND BUTTON AGREES WITH THE LINE**, off the same entry. A button reading "Herd too lean to
	# raid" under a sentence naming the party is the same misattribution one control further on, and
	# a face resolved separately is free to be exactly that.
	var send := Q.find_meta_node(sheet, HudWidgets.SEND_HUNT_CONFIRM_META) as Button
	var party_button := _empty_refusal(SourceForecast.TRIP_BOUND_HORIZON, "button", quarry)
	h._assert_hud("…and the disabled Send wears the SAME culprit's face — \"%s\"" % party_button,
		send != null and send.disabled and send.text == party_button)

	# **THE SPELLED-OUT REASON CARRIES THE PARTY'S REMEDIES AND NONE OF THE HERD'S.** This is the line
	# that actually sends the player somewhere, so the negative half is the load-bearing one: "wait for
	# the herd to rebuild" is advice that would never come true here.
	var party_reason := _empty_refusal(SourceForecast.TRIP_BOUND_HORIZON, "reason", quarry)
	var herd_reason := _empty_refusal(SourceForecast.TRIP_BOUND_FLOOR, "reason", quarry)
	h._assert_hud("…and the reason beside it sends the player to the PARTY, not to the herd",
		Q.has_label_containing(sheet, party_reason)
			and not Q.has_label_containing(sheet, herd_reason))

	# **THE CREW CEILING IS THE ENGAGEMENT CREW, NOT ONE.** The plateau scan can only report a bind it
	# watches the payload run into, and a payload flat at zero never rises — so the sheet said "max 1
	# worker useful here" two lines above the crew ONE animal takes. Both halves are asserted: the
	# ceiling the note now names, and the absence of the advice that contradicted the line above it.
	var expected_note: String = SourceForecast.LABOR_BOUND_NOTE_FORMAT % [
		SourceForecast.expedition_party_cap(band), AUROCHS_ENGAGE_CREW]
	var idle_note: String = SourceForecast.MAX_USEFUL_NOTE_FORMAT % [
		AUROCHS_PARTY, SourceForecast.MAX_USEFUL_NOUN_ONE]
	h._assert_hud("the party ceiling is the crew that REACHES the herd — \"%s\"" % expected_note,
		Q.has_label_containing(sheet, expected_note)
			and not Q.has_label_containing(sheet, idle_note))

	# **THE CONTRADICTION ITSELF, as a relation between two RENDERED lines.** The reach line and the
	# ceiling are the two numbers that disagreed; the ceiling must be at least the crew one animal
	# takes, or the panel is arguing with itself again.
	var reach_line: String = SourceForecast.HUNTERS_PER_ANIMAL_FORMAT % [
		AUROCHS_HUNTERS_PER_ANIMAL, quarry]
	h._assert_hud("…and it can no longer sit BELOW the hunters one animal takes — \"%s\"" % reach_line,
		Readout.hunters_per_animal_line(sheet) == reach_line
			and AUROCHS_ENGAGE_CREW >= AUROCHS_HUNTERS_PER_ANIMAL)

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
