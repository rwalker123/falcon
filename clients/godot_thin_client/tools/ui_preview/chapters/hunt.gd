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

## The crew the LABOR-BOUND frame is composed at, and the number its stepper must SHOW. Named because
## it is asserted against the rendered value, so the dial and the expectation are one number rather
## than two.
##
## **ITS PARTY-SIZE TWIN IS GONE, and that is the point of the pair having become a single.** The other
## frame staged `idle 6 >= max party 2` so `PARTY_SIZE_BOUND_NOTE_FORMAT` would render — but
## `max_expedition_party_size` is the estimate tables' SAMPLING AXIS and never was a rules cap, so
## `expedition_party_cap` no longer reads it, that note has no reachable branch left, and a frame
## staging a cap the client does not apply would render the max-useful note under a party-size name.
const LABOR_BOUND_CREW := 3

## The crew the INEDIBLE-QUARRY pair is composed with — the wolf and the oracle deer that contrasts
## with it. Two hunters is the oracle's own no-waste point (food_per_animal 1.23 ÷ the band's 0.8
## per-worker carry ⇒ 2 carriers haul one whole body), so the frame the accounts are read on carries no
## waste term to argue with; the wolf rides the same crew so the two are compared at ONE party size.
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
## They have moved twice — off the PICKER's rung face onto the IMPROVEMENT control's own (issue #442),
## and then off that face into the READOUT's payoff row, which is where they are asserted now. The
## payoff ARROW went at the first move (the face already read `· then <terms>`, so an arrow inside the
## terms said "then → 1.48" twice) and stays gone: the row's key states the condition instead.
##
## **THEY LOST THEIR SECOND CLAUSE WITH THE TRADE AXIS** (arc #527). Each face read `… · 0.37 trade`
## beside its food half; a herd's non-food product is materials now, and the wire quotes none per rung,
## so a prepared herd's payoff is a food figure alone again.
const BOAR_TAME_PAYOFF_FACE := "1.48 food"

const BOAR_CORRAL_PAYOFF_FACE := "2.95 food"

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

const RAID_TRAVEL_TURNS := 8

const RAID_TRAVEL_HUNT_TURNS := 8

# 0 = the raid ran the whole forecast horizon still delivering (a long raid), used by the no-surplus /
# collapsed fixtures where the raid also lands 0 animals.
const NEVER_FILLS_TRIP_TURNS := 0

## The quarry `_horizon_raid_herd` builds — a slow breeder a big party can neither fill nor exhaust. Named
## because the unbounded-raid copy assertions quote it by EQUALITY and a hand-typed second spelling is
## how a rename turns a real claim into a comparison of two wrong strings.
const HORIZON_QUARRY_NAME := "Steppe Bison"

## `floor_chart_model`'s `lesson_known` for a probe reading the VERDICT rather than the aside: the
## faction has NOT learned this source's lesson, so the teaching line is the one it always carried.
const LESSON_NOT_YET_LEARNED := false

## The TRIP-READOUT claims that live on the `_hunt_assign_forecast_states` frames, dispatched by state
## name so each fixture is asserted on the ONE thing it was built to show. They ride here rather than
## after the loop because the loop is where each state is actually staged, and re-staging one to assert
## it would risk asserting a sheet the frame never rendered.
##
## **EACH IS ONE HALF OF A PAIR**, the other half being `herd_hunt_expedition`'s block (a clean raid,
## no waste, a brisk OK verdict): a lone "the waste note is here" passes on a readout that always
## prints one.
func _assert_trip_readout(state_name: String) -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	match state_name:
		"herd_hunt_forecast_viable":
			# A party of 4 kills a 16-food mammoth and hauls 4 of it — the WASTE half. The deer in
			# `herd_hunt_expedition` is its clean-raid twin.
			var wasted := Readout.yields_text(sheet)
			var waste_pct := int(round((MAMMOTH_FOOD_PER_ANIMAL - HerdFx.HUNT_FORECAST_PARTY)
				/ MAMMOTH_FOOD_PER_ANIMAL * 100.0))
			h._assert_hud("a partial kill states its WASTE on the trip's yields row",
				wasted.contains((SourceForecast.HUNT_WASTE_NOTE_FORMAT % waste_pct).to_upper()))
			h._assert_hud("…beside the FOOD the party actually lands",
				wasted.contains("FOOD"))
		"herd_hunt_forecast_slow":
			# 54 turns past the band's 20-turn warn line — the verdict carries the severity the Send
			# button and the one-line form already carry, so the box cannot disagree with either.
			h._assert_hud("a raid past the band's warn line reads SLOW in the trip verdict",
				Readout.verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and Readout.verdict_text(sheet).contains(str(DEER_SUSTAIN_TRIP_TURNS)))
		"herd_hunt_forecast_eradicate":
			# **A STRIP-BARE RAID COMPLETES, AND THE SHEET MUST SAY WHEN.** This assertion used to pin
			# the OPPOSITE — `an unbounded raid states no total, and still reads SLOW` — against a
			# fixture whose floor-`0` row carried the wire's no-turn-count sentinel. That pairing was a
			# sim defect, so the assertion was pinning a bug: a floor-`0` raid ends by emptying the
			# range and comes home on a real turn. The claim it pinned lives on
			# `herd_hunt_forecast_horizon`, where the projection genuinely does run out.
			var strip_verdict := Readout.verdict_text(sheet)
			h._assert_hud("a strip-bare raid quotes a REAL total, never the unbounded sentence",
				strip_verdict.contains(str(HerdFx.STRIP_BARE_TRIP_TURNS))
					and not strip_verdict.contains(SourceForecast.EXPEDITION_TRIP_LONG_VERDICT))
			# The stop is what makes the total legible: the party comes home because the range is empty,
			# not because its pack filled. Asserted BESIDE the total — a verdict quoting a turn count
			# under the wrong stop is the same defect one clause along.
			h._assert_hud("…and names the stop that ended it — the herd running out, not the clock",
				strip_verdict.contains(SourceForecast.TRIP_BOUND_CLAUSES[
					SourceForecast.TRIP_BOUND_HERD_LOST]))
			# A raid the sim bounds inside the band's warn line is not a warning, and the Send says so:
			# `Send Anyway (long raid)` was the third spelling of "never completes" on this frame.
			var strip_send := Q.find_meta_node(sheet, HudWidgets.SEND_HUNT_CONFIRM_META) as Button
			h._assert_hud("…and the Send is the ordinary one, not the long-raid warning",
				strip_send != null and not strip_send.disabled
					and strip_send.text == SourceForecast.SEND_HUNTING_EXPEDITION_BUTTON
					and Readout.verdict_severity(sheet) == SourceForecast.VERDICT_OK)
		"herd_hunt_forecast_horizon":
			# `turns_to_fill == RAID_TURNS_UNBOUNDED` — the raid ran the whole forecast horizon still
			# delivering, so there is no total to quote and the verdict says so instead of printing a
			# bare 0. **The pairing half of the strip-bare claims above**: without a frame that still
			# reaches this branch, "never says `many turns`" would pass on a client that could no longer
			# say it at all.
			# **THIS BAND CARRIES NO MOVE RATE, so its trip is all hunting and the floor IS the
			# horizon** — which is exactly why it cannot tell `horizon` from `horizon + travel`.
			# `herd_hunt_horizon_travel` is the frame that can; this one pins that the branch is still
			# reachable and that it no longer hedges.
			h._assert_hud("an unbounded raid bounds itself with the horizon, and still reads SLOW",
				Readout.verdict_severity(sheet) == SourceForecast.VERDICT_SLOW
					and Readout.verdict_text(sheet) == "%s Away more than %d turns. Still delivering at the end of the forecast." % [
						HudWidgets.VERDICT_DOT, BandFx.FORECAST_HORIZON_TURNS])
			var horizon_send := Q.find_meta_node(sheet, HudWidgets.SEND_HUNT_CONFIRM_META) as Button
			h._assert_hud("…and its Send names that same floor rather than the word \"long\"",
				horizon_send != null
					and horizon_send.text == "Send Anyway (more than %d turns)"
						% BandFx.FORECAST_HORIZON_TURNS)
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
		BandFx.with_band_id({"entity": 801, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 12, "hunt_reach": 6, "activity": "forage", "labor_assignments": []}),
		BandFx.with_band_id({"entity": 802, "faction": 0, "size": 40, "current_x": 68, "current_y": 12,
			"working_age": 6, "idle_workers": 2, "hunt_reach": 6, "activity": "hunt", "labor_assignments": []}),
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
		# and a bison is edible on every rung).
		for entry in [["sustain", sustain_animals[i], 8], ["surplus", surplus_animals[i], 6],
				["deplete", deplete_animals[i], 5], ["eradicate", int(deplete_animals[i]) + 2, 4]]:
			var animals: int = int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": int(entry[2]), "delivers_food": true,
				"animals_taken": animals, "delivered_food": float(animals) * fpa,
				"wasted_food": 0.0,
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
		# The species is EDIBLE — it is the HERD that has nothing left, so `delivers_food` is true on
		# every rung and the payload is 0. That is what makes this the "too lean" case rather than the
		# "denial mission" one (issue #337).
		for policy in ["sustain", "surplus", "deplete", "eradicate"]:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": 0, "delivers_food": true,
				"animals_taken": 0, "delivered_food": 0.0,
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
			# **AND IT COMPLETES.** A floor-`0` raid ends by emptying the range, so its row is
			# `herd_lost` beside a real turn count — the frame the "never completes" fix is judged on.
			"name": "herd_hunt_forecast_eradicate",
			"floor": 0.0,
			"herd": HerdFx.assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# **THE RAID THAT GENUINELY DOES NOT FINISH** — and it is a floor at the food PEAK, not a
			# floor of 0. It is the pairing half of the state above: with the strip-bare row moved onto
			# a real turn count, this is the corpus's only delivering `horizon` row, so without it the
			# three "many turns" surfaces would have no fixture at all and a regression that spelled
			# every raid as unbounded would render no frame differently.
			"name": "herd_hunt_forecast_horizon",
			"floor": SourceForecast.FLOOR_FOOD_PEAK,
			"herd": _horizon_raid_herd(),
		},
	]

## A slow breeder a big party can neither fill nor exhaust — the sim's own words for the ONE stop that
## reports no turn count. Built from the shared forecast herd and then re-stamped at the PEAK floor's
## cell, because `HerdFx.forecast_herd` pairs every delivering row with a stop that HAS a turn, which
## is now true of its floor-`0` row too.
func _horizon_raid_herd() -> Dictionary:
	var herd := HerdFx.assign_preview_herd("game_bison_44", HORIZON_QUARRY_NAME, "thriving", 0.30,
		DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
		DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS)
	# The stance-keyed table has not been floorified yet (`_show_herd` does that), so the peak floor is
	# still spelled `sustain` — `BaseFx.LEGACY_STANCE_FLOORS` maps it to `FLOOR_FOOD_PEAK`.
	var cell: Dictionary = (herd["hunt_trip_estimates"] as Dictionary)[
		"sustain:%d" % HerdFx.HUNT_FORECAST_PARTY]
	cell["turns_to_fill"] = SourceForecast.RAID_TURNS_UNBOUNDED
	cell[SourceForecast.TRIP_BOUND_KEY] = SourceForecast.TRIP_BOUND_HORIZON
	return herd

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
	return BandFx.with_band_id({
		"id": "Band 1", "entity": 831, "faction": 0, "size": 80,
		"current_x": 86, "current_y": 24, "pos": [86, 24],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		"expedition_forecast_horizon_turns": BandFx.FORECAST_HORIZON_TURNS,
		# Per-worker carry (shipped 4.0) → the forecast's HAUL = party × this.
		"expedition_per_worker_carry": 4.0,
		"activity": "forage", "labor_assignments": [],
	})

## A band 8 tiles from the (66,10) herd (beyond hunt_reach 7 → expedition) carrying a MOVE RATE, so the
## raid forecast's round-trip travel is exercised: ceil(2 × 8 / 2) = 8 travel turns added to the hunting
## turns. `band_move_tiles_per_turn` now ships on the wire (schema slot 124) and is decoded onto the band;
## this carries the same value the decoder surfaces.
func _raid_travel_band() -> Dictionary:
	return BandFx.with_band_id({
		"id": "Band 1", "entity": 833, "faction": 0, "size": 80,
		"current_x": 66, "current_y": 18, "pos": [66, 18],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		"expedition_forecast_horizon_turns": BandFx.FORECAST_HORIZON_TURNS,
		"expedition_per_worker_carry": 4.0,
		"band_move_tiles_per_turn": 2,
		"activity": "forage", "labor_assignments": [],
	})

## The oracle band for the carry-aware delivered/waste preview: per-worker 0.8, output 1.0 (so the
## rendered numbers match the spec oracle EXACTLY — no morale modifier muddying them), sitting ON the
## herd (local branch), with plenty of idle workers so the big-game auto-max (20 carriers) isn't
## labor-bound.
func _delivered_oracle_band() -> Dictionary:
	return BandFx.with_band_id({
		"id": "Band 1", "entity": 840, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": 26,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	})

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
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## THE INEDIBLE QUARRY (issue #337, arc #527) — a wolf pack: `provisions == 0` on every rung. Every
## food-denominated field is deliberately 0/absent, `food_per_animal` included, so anything that still
## divides by a food quantum divides by zero and shows up in the frame rather than hiding.
##
## **WHAT THIS FRAME PROVES, AND HOW ITS SUBJECT MOVED TWICE.** It began as the two-product arc's
## judge: four ascending TRADE numbers, no food line, no zeros. Arc #527 retired that account, and for
## one release the honest reading was that the sheet stated no rate at all — the wire quoted a herd no
## material figure. The follow-up closed that: a herd now publishes `material_per_biomass` and
## `per_worker_material`, so this sheet composes `min(workers × per_worker, ceiling(floor))` per
## material exactly as the food side does, and the wolf quotes what it actually pays.
##
## **The claim that never changed is still the one that matters**: it must not print a `0.00 FOOD`
## saying a wolf's pelts are worth no meat. `herd_hunt_both_products`' deer beside it is what keeps
## that negative from passing on a readout that has simply lost every account — and the positive
## claim (a live `hide` rate) is what keeps it from passing on a readout that says nothing.
##
## **THE FOOD SIDE IS ALL ZEROS ON PURPOSE.** `provisions_per_biomass`, `food_per_animal` and
## `per_worker_yield` are the structural zeros of an inedible species; they are what make every food
## path in `_hunt_yield_model` bail, which is the state the material arm has to stand up alone in.
func _pelt_only_wolf_herd() -> Dictionary:
	return {
		"id": "game_wolf_03", "label": "Grey Wolf (game_wolf_03)", "species": "Grey Wolf",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": WOLF_BIOMASS,
		# The capacity the escapement room is measured against — a wolf pack composes its material
		# ceiling by the SAME `max(0, B − floor·K) × rate` rule a deer composes its food one, so the
		# fixture has to state it or the room is the whole standing stock.
		"carrying_capacity": WOLF_CAPACITY,
		"husbandry_ceiling": "wild",
		"prey_sense_radius": 4,
		"food_per_animal": 0.0,
		"per_worker_yield": 0.0,
		"provisions_per_biomass": 0.0,
		# **WHAT ONE UNIT OF THIS PACK IS MADE OF** — the material twin of `provisions_per_biomass`,
		# and the term the floor presets scale by the room at whatever floor is dragged.
		"material_per_biomass": [
			{"material_id": WOLF_MATERIAL_ID, "amount": WOLF_MATERIAL_PER_BIOMASS},
		],
		# …and what ONE HUNTER brings home per turn, the twin of `per_worker_yield`. Deliberately the
		# BINDING term at this frame's crew: see `WOLF_MATERIAL_PER_WORKER`.
		"per_worker_material": [
			{"material_id": WOLF_MATERIAL_ID, "amount": WOLF_MATERIAL_PER_WORKER},
		],
		"hunt_policy_ceilings": {
			"sustain": 0.0, "surplus": 0.0, "deplete": 0.0, "eradicate": 0.0,
		},
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## A floor preset's TOOLTIP — where its per-turn cap is stated now that the face carries the intent
## alone. Reached by the rung's own meta (`HudWidgets.POLICY_RUNG_META`) and never by button text:
## the face is a two-Label stack beside an empty-`text` Button, so a text search finds nothing here.
func _floor_preset_tooltip(root: Node, preset: String) -> String:
	var btn := Q.find_policy_rung(root, preset)
	return btn.tooltip_text if btn != null else ""

## ---- THE INEDIBLE QUARRY'S OWN ARITHMETIC -----------------------------------------------------
## Stated rather than restated, because two frames and a work-board row read the same pack and a
## number typed twice is a fixture that can disagree with itself.
##
##   room at the food-peak floor = 240 − 0.5 × 400 = 40 biomass
##   ceiling                     = 40 × 0.02       = 0.80 hide/turn
##   per hunter                  = 0.11 hide/turn                    ← the binding term
##
## **THE CREW BINDS, NOT THE CEILING, AND THAT IS THE POINT OF THE NUMBERS.** A frame whose ceiling
## bound would render the same string whether or not the per-worker rate were read at all, so the
## `min` it is supposed to prove would be decorative. At any crew this sheet can compose, the crew
## arm is the one that answers.
##
## **THE CREW IS READ BACK, NEVER ASSUMED.** The sheet's own worker cap lands this frame at ONE
## hunter — `max_useful_workers` divides by the quantised FOOD axis, which an inedible quarry has
## none of — so a `PELT_FRAME_HUNTERS`-shaped expectation would assert against a crew the stepper
## refuses. The claim is composed from `hunt_count()` at assertion time instead, which is also what
## keeps it true the day that cap learns about materials.
const WOLF_BIOMASS := 240.0
const WOLF_CAPACITY := 400.0
## `hide` is a real `materials.json` id, and the catalogue ships no display name — so the id IS the
## display word, exactly as `fibre` is on the crop picker's basket rows.
const WOLF_MATERIAL_ID := "hide"
const WOLF_MATERIAL_PER_BIOMASS := 0.02
const WOLF_MATERIAL_PER_WORKER := 0.11
## What ONE hauled wolf is worth in hides on a RAID. The trip line rounds its payload to whole units
## (a trip is not a rate), so this is sized to clear 1.0 at the frame's kill counts — a payload that
## rounded to `~0` would render a clause the reader could not tell from a suppressed one.
const WOLF_RAID_HIDE_PER_ANIMAL := 0.55

## What the frame must read, composed at assertion time from the crew the sheet actually landed on
## (see above) times the per-worker rate, at the reference band's full output.
func _wolf_material_take(crew: int) -> float:
	return float(crew) * WOLF_MATERIAL_PER_WORKER

## The wolf's RAID table: `delivers_food = false` on every rung — an INEDIBLE quarry, not a denial
## POLICY. It read as a DENIAL MISSION for the release between the trade axis's retirement and
## `delivered_material` reaching the reply, because with no payload on the wire "brings nothing home"
## was all the client could honestly say. **It is a real delivery now**: the rows carry the hides the
## party hauls, and the denial branch tests `delivers_food == false` AND no material — a raid that
## brings something home is a delivery whatever account that something is in.
##
## **THE PAYLOAD GROWS WITH THE PARTY, exactly as a food payload does**, so the picker's per-preset
## metric and the party stepper both move on it; a flat table would let a stepper that reads nothing
## back pass every claim made here.
func _pelt_only_wolf_raid_herd() -> Dictionary:
	var herd := _pelt_only_wolf_herd()
	var table := {}
	var animals_row := [3, 5, 6, 6, 6, 6, 6, 6]
	for i in animals_row.size():
		for entry in [["sustain", 0, 9], ["surplus", 1, 7], ["deplete", 2, 6], ["eradicate", 4, 5]]:
			var animals: int = int(animals_row[i]) + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": int(entry[2]),
				"delivers_food": false,
				"animals_taken": animals,
				"delivered_food": 0.0, "wasted_food": 0.0,
				# **THE ENTIRE PAYLOAD**, since `delivered_food` is honestly 0 here — which is exactly
				# why a wolf is the fixture this half of the arc needs: a missing material row cannot
				# hide behind a food number. Composed from the kill count so it moves with the party.
				SourceForecast.TRIP_DELIVERED_MATERIAL_KEY: [
					{"material_id": WOLF_MATERIAL_ID,
						"amount": float(animals) * WOLF_RAID_HIDE_PER_ANIMAL},
				],
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
	# ~6 food"), beside a local sheet that laid the same kinds of fact out in a
	# bounded well — two sheets on one panel reading nothing alike. What must NOT carry over is the
	# per-turn framing, and the header is where that shows: a trip has no steady state, so
	# `THIS TRIP` and not `PER TURN`, and no `now → after` arrow to key.
	h._assert_hud("the expedition sheet's readout is headed for a TRIP, not for a rate",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.EXPEDITION_TRIP_ROW_HEADER.to_upper())
	h._assert_hud("…so it states no PER TURN header and no now → after arrow",
		not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("PER TURN")
			and not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("→"))
	# THE PAYLOAD, BOTH TERMS. The animal count leads in the local hunt row's own idiom (the `≈` face,
	# the quarry as the unit, no account), then the account those bodies pay. Both are named, because
	# matching one survives losing the other — and this quarry PAYS FOOD, which is the positive half of
	# the render-only-where-the-vector-pays pair whose negative is the wolf frame further down.
	var trip_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("the ANIMAL count leads the row, in the quarry's own name",
		trip_yields.contains("≈%d" % HerdFx.DISTANCE_RAID_ANIMALS[0]) and trip_yields.contains("RED DEER"))
	h._assert_hud("…with the trip's FOOD beside it",
		trip_yields.contains(SourceForecast.format_magnitude(HerdFx.DISTANCE_RAID_ANIMALS[0] * 2.0))
			and trip_yields.contains("FOOD"))
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
	var expedition_sheet: Control = h._hud._drawercompose._compose_sheet
	# **THERE IS NO PARTY-SIDE LEVER ON THE TRIP'S LENGTH, and that absence is deliberate.** A fill
	# target ("come home with N animals") was offered here and is retired (issue #491): trip length is
	# `carry ÷ (engage_rate × stay_chance × body_mass)`, in which PARTY SIZE CANCELS, so the control
	# moved a species-and-kit constant and the spread it existed to escape is a TUNING problem. What the
	# readout quotes is the sim's own untargeted raid, and the verdict below is the whole of what the
	# sheet says about the trip's end.
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
	#   3o eradicate   — a healthy Red Deer at the STRIP-BARE floor: it DELIVERS like every other rung
	#                    (#337 pays each rung the species' yield vector) AND it COMPLETES — the raid ends
	#                    by emptying the range on turn 11, so the line quotes a real total and the Send is
	#                    the ordinary one. NOT a denial: denial is now a property of the QUARRY (pays
	#                    neither product), not of the rung.
	#   3o2 horizon    — a Steppe Bison at the food PEAK the party can neither fill nor exhaust: the one
	#                    row whose projection genuinely ran out → amber LONG-RAID line + "Send Anyway
	#                    (long raid)". It is the PAIR to 3o, since with the strip-bare row bounded this is
	#                    the corpus's only delivering `horizon` cell.
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
	#   (The PARTY-SIZE-BOUND frame that stood here is DELETED with the cap it staged: a band with
	#    `idle 6 >= max party 2` no longer binds on anything, because `expedition_party_cap` reads the
	#    idle workforce alone. Its note had no reachable branch left, so the frame could only have
	#    rendered the max-useful note under a party-size name.)
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

	# 3w — THE INEDIBLE QUARRY (issue #337, arc #527). A wolf pays PELTS AND NO MEAT: `provisions == 0`
	# on every rung. It read four ascending TRADE numbers until that account was retired, then nothing
	# at all for one release; the follow-up gave a herd `material_per_biomass` / `per_worker_material`,
	# so the sheet composes `min(workers × per_worker, ceiling(floor))` per material and the wolf
	# quotes what it actually pays. TWO claims ride the frame, and they are a pair: it must never print
	# a `0.00 FOOD` saying a wolf's pelts are worth no meat, AND it must state the hide rate — the
	# negative alone is satisfied by a readout that prints nothing.
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
	# food row — never the `0.00 FOOD` that says its pelts are worth no meat. **Asserted as a PAIR
	# WITH `herd_hunt_both_products`**, whose deer states a live FOOD row off the same producer: on its
	# own this negative is satisfied by a readout that has stopped printing any account at all, which
	# is exactly the failure mode a retired account invites.
	var wolf_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("an inedible quarry states NO food row — its pelts are not meat",
		not wolf_yields.contains(SourceForecast.YIELD_ACCOUNT_UNITS[
			SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()))
	# **AND THE POSITIVE HALF: it quotes the hide it pays**, at the CREW arm of the `min` rather than
	# at the ceiling — see `WOLF_MATERIAL_TAKE` for why that distinction is what makes the claim bite.
	# The unit is the material's own id, uppercased by the readout exactly as `FOOD` is, because the
	# catalogue ships no display name and the id IS the display word.
	var wolf_take := _wolf_material_take(h._hud._compose.hunt_count())
	h._assert_hud("the wolf's crew is one the sheet will actually compose", wolf_take > 0.0)
	h._assert_hud("…and it QUOTES the hide, which is the whole of what the hunt pays",
		wolf_yields.contains(SourceForecast.format_magnitude(wolf_take))
			and wolf_yields.contains(WOLF_MATERIAL_ID.to_upper()))
	# **THE FLOOR PRESETS QUOTE IT TOO, and at the PRESET's own floor rather than the composed one.**
	# A preset's cap is the room above ITS floor through the same per-biomass rate, so `strip` (floor
	# 0, the whole standing stock) must quote strictly more hide than the food peak's half-capacity
	# room — which is the claim that the material ceiling composes at a floor at all rather than being
	# a constant the picker repeats four times.
	var peak_cap := _floor_preset_tooltip(h._hud._drawercompose._compose_sheet,
		SourceForecast.FLOOR_PRESET_PEAK)
	var strip_cap := _floor_preset_tooltip(h._hud._drawercompose._compose_sheet,
		SourceForecast.FLOOR_PRESET_STRIP)
	h._assert_hud("the wolf's floor presets quote hide rather than nothing",
		peak_cap.contains(WOLF_MATERIAL_ID) and strip_cap.contains(WOLF_MATERIAL_ID))
	h._assert_hud("…and a deeper floor quotes MORE of it — the ceiling composes at the floor",
		strip_cap != peak_cap)
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
	# cell says THE QUARRY IS INEDIBLE. This frame read as a DENIAL MISSION for the release in which
	# the trade axis was gone and `delivered_material` had not landed — "brings nothing home" was all
	# the client could honestly say. **It is a real delivery now**, and the line quotes the hides: the
	# frame's whole job is that an inedible quarry's raid is legible, and its food readings being
	# honest zeros is what stops a missing material clause hiding behind a food number.
	var wolf_raid := _pelt_only_wolf_raid_herd()
	h._hud._band_labor._player_bands = [_hunt_preview_far_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(wolf_raid)
	h._compose_herd(wolf_raid, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_pelts_raid")
	# **THE RAID IS NO LONGER A DENIAL MISSION, AND IT QUOTES WHAT IT HAULS.** Driven through the REAL
	# producer on the fixture's own row — the compose sheet renders the readout BOX rather than the
	# one-line form, so the sentence is asserted where it is composed (the idiom `herd_hunt_horizon`
	# uses one state along).
	#
	# **THREE claims, and the third is the one a lazy fix fails.** Not a denial mission (the branch that
	# owned this frame for a release); the line names the hide; and it is not read as a raid that
	# RETURNS EMPTY — which is where a food-only `delivered_food <= 0` test sends every raid whose
	# payload is material, printing a refusal at a party that is walking home loaded.
	var raid_forecast := SourceForecast.hunt_trip_forecast(
		h._hud._band_labor._player_band, h._hud._selection.herd(),
		_raid_row_for(h._hud._selection.herd(), SourceForecast.FLOOR_FOOD_PEAK,
			PELT_FRAME_HUNTERS),
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal())
	h._assert_hud("an inedible quarry's raid is not a denial mission once it hauls something",
		not bool(raid_forecast.get("denial", true)))
	h._assert_hud("…nor a raid that returns empty, which a food-only test would call it",
		not bool(raid_forecast.get("empty", true)))
	var raid_line := SourceForecast.hunt_forecast_line_bbcode(raid_forecast,
		String(h._hud._selection.herd().get("species", "")))
	h._assert_hud("…and the raid line names the hide it brings home (got \"%s\")" % raid_line,
		raid_line.contains(WOLF_MATERIAL_ID))

	# 3y — THE PAYING CONTROL: the same oracle deer, which pays real food. Each picker button's product
	# line must carry that food (`2.33 food`), which is the half of the rule the wolf frame cannot
	# prove — its all-zero readout is only meaningful beside a readout that still prints. Rendered right
	# after the wolf so the two are compared directly. Both frames also judge the TWO-LINE FACE itself:
	# the rung's name over its products, so `which rung` and `what it pays` stop competing in one line
	# of glyphs.
	h._hud._band_labor._player_bands = [_delivered_oracle_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	var oracle_pair := _delivered_oracle_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(oracle_pair)
	h._compose_herd(oracle_pair, PELT_FRAME_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_both_products")
	# **THE READOUT'S HALF OF THE PAIR — the quantised animal count, valued in the account it pays.**
	# Reported from play, a Wild Boar's compose sheet read `0.00 FOOD` while an expedition on the same
	# species read a real take: a quantised take is COUNTED on one axis, but the count is unit-free, and
	# the sim values it through `YieldPair::rescaled_to` while the client had stopped at the axis it had
	# quantised on. **This is the LIVE half of the wolf frame's negative** — with the trade account
	# retired, "the readout prints nothing" and "the readout is correctly silent" are the same picture,
	# and only a frame that still prints tells them apart.
	var pair_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	var food_unit: String = SourceForecast.YIELD_ACCOUNT_UNITS[
		SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()
	h._assert_hud("a local hunt's PER TURN row names the account this take pays",
		pair_yields.contains(food_unit))
	# THE MAGNITUDE, recomposed from the sim's own step: `quantise_animal_take` for the count (the
	# harness's `HerdFx.hunt_take_oracle`, in food). The client composes it through the per-biomass
	# vector instead, so the two arrive at one answer by different routes rather than by construction.
	var pair_food := float(HerdFx.hunt_take_oracle(PELT_FRAME_HUNTERS * ORACLE_DEER_PER_WORKER,
		ORACLE_DEER_SUSTAIN_CEILING, ORACLE_DEER_FOOD_PER_ANIMAL)["delivered"])
	h._assert_hud("…and it is the crew's quantised take (%s)"
		% SourceForecast.format_magnitude(pair_food),
		is_equal_approx(_yield_take(pair_yields, food_unit),
			float(SourceForecast.format_magnitude(pair_food))))

	# 3z — THE INVESTMENT-RUNG TWIN of 3y (issue #397). A prepared herd pays what a hunted one does, so
	# the payoff obeys the same render-only-when-non-zero rule and both faces must name their FOOD.
	# The pair briefly carried a trade clause each (`pastoralTrade` / `corralTrade`); arc #527 retired
	# both wire fields with the axis, and the wire quotes a herd no per-rung material figure to put in
	# their place — so a payoff face is a food figure again.
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
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet)
			.ends_with(BOAR_TAME_PAYOFF_FACE))
	h._assert_hud("…and states them ONCE, the box above carrying no terms of its own",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
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
	h._assert_hud("Corral's OFFERED payoff names BOTH products too, food leading",
		corral_offer is CheckBox
		and Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet)
			.ends_with(BOAR_CORRAL_PAYOFF_FACE))
	h._assert_hud("…as the block's ONLY row, an offered rung stating its payoff and nothing else",
		Readout.improvement_deal_rows(h._hud._drawercompose._compose_sheet) == 1)

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

	# ---- THE QUANTISED TAKE IS ONE EXPRESSION, AND IT NEVER FALLS AS THE CREW GROWS ---------------
	_engagement_quantisation_assertions()

	# ---- THE RETREAT PRICES THE CREW, NOT ONLY THE TAKE ------------------------------------------
	_retreat_crew_assertions()


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
	# State gate-a — A SPEARED PARTY, above the gate. **THE SHEET SAYS NOTHING ABOUT THE FIGHT**, and
	# that is the claim: both pre-launch lines this state used to carry are retired (reported from
	# playtest). The hunters-per-animal figure and the hunter-turns effort figure were species
	# constants that never moved with anything the player was dialling, printed between the kit they
	# had just chosen and the forecast that already prices the whole trip. What survives is the
	# REFUSAL, asserted in gate-b.
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
	# **`HUNT_GATE_ABSENT`, NOT `HUNT_GATE_WINNABLE`.** The two are different findings — "the sheet
	# says the fight is winnable" and "the sheet says nothing about the fight" — and the whole point of
	# the removal is that a winnable fight now renders NO line, so the winnable state no longer exists
	# on screen. Asserting the absence is what stops the effort face creeping back.
	h._assert_hud("a party above the gate is told nothing about the fight — no line at all",
		Readout.hunt_gate_blocked(speared_sheet) == Readout.HUNT_GATE_ABSENT
			and Readout.hunt_gate_line(speared_sheet) == "")

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
	# **THE NEGATIVE, AND IT IS THE HALF THE ARC KEEPS BREAKING.** A PEN publishes no engagement
	# stage, so the refusal may not render on one — and this fixture carries a real `defense` and
	# `durability`, so the silence is the ENGAGEMENT GATE's doing rather than a fixture that omitted
	# the terms. **That gate is the reason `has_engagement_stage` survives the removal above**: a
	# penned animal is not fought, and without it a pen would wear the refusal. PNG-less: the claim is
	# an absence, which a picture states only by not showing something, and the frame set's byte-diff
	# is where a regression would actually surface.
	var pen := _combat_gate_pen()
	h._hud._compose.reset_hunt_source()
	h._show_herd(pen)
	h._compose_herd(pen, LOCAL_HUNT_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	var pen_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a PEN is not stalked and not fought — the refusal does not render on one",
		Readout.hunt_gate_blocked(pen_sheet) == Readout.HUNT_GATE_ABSENT)

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)


# =====================================================================================
#  THE FORECAST REPORTS A RANGE (`docs/plan_hunt_through_combat.md` §6.4)
# =====================================================================================
# `actualYield` became the take's EXPECTATION over the retreat seed, and the pair around it says how
# far the real take can land from it. **A LIVE BAND IS THE SHIPPED CASE ON A HUNT** — wariness is
# authored across the roster (0.10–0.85), so the retreat binomial spreads a raid's take and a range
# on a herd row is the feature, not a defect to go hunting for. What is still degenerate is the plant
# web (a patch publishes no retreat stage), a resolved row, and a spread too narrow to survive the
# formatter's own rounding.
#
# **SO THE PAIR OF STATES IS THE CLAIM.** A band renders where the bounds differ, and NO band renders
# where they agree; the second is what stops the first passing on a readout that decorates every row.
# Both halves are CONSTRUCTED here — the fixtures state the two bounds directly — so neither frame
# depends on what the roster happens to author this month.

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

	# State range-b — **THE DEGENERATE CASE, PINNED.** A forage patch and every resolved row report it,
	# and without this half the state above passes just as well on a readout that draws a range
	# unconditionally. The bounds are collapsed onto the expectation HERE rather than staged off a
	# wariness-free species, so the claim is about the readout and not about the roster.
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
				"turns_to_fill": 0, "delivers_food": true,
				"animals_taken": 0, "delivered_food": 0.0,
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

	# **THE CONTRADICTION ITSELF.** It was a relation between two RENDERED lines — the reach line and
	# the ceiling — until the reach line was retired; the ceiling is still the half that has to hold,
	# and it must be at least the crew one animal takes or the cap is back below the engagement arm
	# that floors it.
	h._assert_hud("…and the ceiling can no longer sit BELOW the hunters one animal takes (%d >= %d)"
			% [AUROCHS_ENGAGE_CREW, AUROCHS_HUNTERS_PER_ANIMAL],
		AUROCHS_ENGAGE_CREW >= AUROCHS_HUNTERS_PER_ANIMAL)

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)

	# ---- THE UNBOUNDED RAID QUOTES A FLOOR, AND THE FLOOR INCLUDES THE WALK ----------------------
	# **The pairing half of `herd_hunt_forecast_horizon`, and the only frame that can tell the fix from
	# the bug it replaces.** That state's band carries no move rate, so its trip is all hunting and
	# `horizon` and `horizon + travel` are the same number — a client quoting the bare horizon renders
	# it identically. Here the SAME never-completing Steppe Bison is raided from the travel band (8
	# tiles out at 2 tiles a turn → a round trip of `RAID_TRAVEL_TURNS`), so the two answers differ by
	# the whole walk and the copy has to say which one it means.
	#
	# The rule the sim's own field states: the horizon bounds the HUNTING alone, so the floor on the
	# TRIP is `horizon + round-trip travel`. A number wrong in the reassuring direction is worse than
	# the "many turns" it replaces, which is why every claim below is an EQUALITY.
	h._hud._band_labor._player_bands = [_raid_travel_band()]
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	var horizon_travel_herd := _horizon_raid_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(horizon_travel_herd)
	h._compose_herd(horizon_travel_herd, HerdFx.HUNT_FORECAST_PARTY,
		SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_horizon_travel")
	_assert_horizon_floor_is_the_whole_trip()

	# Reset again for whatever renders after this chapter.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)

	# ---- A SHEET COMPOSES FOR THE BAND THE PLAYER IS LOOKING AT (issue #510) ---------------------
	# **The two-band regression founding a colony exposed.** `Hud._resolve_assign_band` fell through to
	# the FIRST player band whenever no unit was selected — and selecting a herd or a tile, which is
	# how a compose sheet is opened in the first place, leaves the selection's unit empty. Once a
	# colony existed the sheet composed for the PARENT while the Band/City panel read the colony: the
	# picker said `Band 1` under a header saying `Band 2`, and the crew stepper capped at the parent's
	# spent idle workers. Every number under it was honest and about the wrong band.
	#
	# **THE ROSTER IS THE ASSERTION.** Every other compose fixture in this harness is single-band, which
	# is exactly why nothing here caught it: with one band the three rungs of the resolver agree, so a
	# state that does not stage a SECOND band as the panel's subject passes for free.
	#
	# Three wrong answers, three distinct numbers, so a failure names WHICH rung misfired — the parent
	# holds `PANEL_BAND_PARENT_IDLE` (its crew walked out with the expedition), the colony
	# `PANEL_BAND_COLONY_IDLE`, and the panel's stored render-time COPY of the colony
	# `PANEL_BAND_STALE_IDLE`.
	var panel_roster := _panel_band_roster()
	var panel_colony: Dictionary = panel_roster[PANEL_BAND_COLONY_INDEX - 1]
	h._hud._band_labor._player_bands = panel_roster
	h._hud._band_labor._player_band = panel_roster[0]
	h._hud._band_labor.set_panel_band(_stale_panel_band())
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	var panel_band_herd := HerdFx.herd_fixture()
	h._show_herd(panel_band_herd)
	h._compose_herd(panel_band_herd, PANEL_BAND_DIALED_CREW)
	await h._settle()
	await h._save("compose_panel_band_hunt")
	_assert_composes_for_panel_band("compose_panel_band_hunt", panel_colony,
		h._hud._compose.hunt_band())

	# ---- …AND SO DOES THE FORAGE SHEET ----------------------------------------------------------
	# The report says the forage sheet behaved identically, and it is a SECOND call site of the same
	# resolver — one sheet passing says nothing about the other. Same roster, same stale panel copy,
	# and the (66,10) reference patch both bands stand within work range of, so nothing but the band
	# resolution can move the answer.
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	var panel_band_tile := BaseFx.food_tile_fixture()
	h._show_tile(panel_band_tile)
	h._compose_forage(panel_band_tile)
	h._hud._compose.set_forage_count(PANEL_BAND_DIALED_CREW)
	h._compose_forage(panel_band_tile)
	await h._settle()
	await h._save("compose_panel_band_forage")
	_assert_composes_for_panel_band("compose_panel_band_forage", panel_colony,
		h._hud._compose.forage_band())

	# Reset the roster, the panel band and BOTH compose spines for whatever renders after this chapter.
	h._hud._band_labor.set_panel_band({})
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._hud.clear_selection()


## The three surfaces that used to say "many turns", each asserted by EQUALITY against a sentence
## spelled out HERE rather than re-composed through `SourceForecast`'s own formats — a claim about copy
## that borrows the copy under test can only ever agree with itself. A `contains` would not do either:
## the failure being guarded is a line quoting the HORIZON where it owes the horizon PLUS the walk, and
## those two lines share every word.
func _assert_horizon_floor_is_the_whole_trip() -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var trip_floor := BandFx.FORECAST_HORIZON_TURNS + RAID_TRAVEL_TURNS
	# 1. THE TRIP VERDICT — the sentence the defect was reported against ("Away many turns — still
	#    delivering at the end of the forecast, after 18 turns of travel"), now a bound in the same span
	#    and the same shape its bounded twin ("Away ≈36 turns — 18 hunting, 18 travel") states.
	var expected_verdict := ("%s Away more than %d turns — more than %d hunting, %d travel. "
		+ "Still delivering at the end of the forecast.") % [
			HudWidgets.VERDICT_DOT, trip_floor, BandFx.FORECAST_HORIZON_TURNS, RAID_TRAVEL_TURNS]
	var got_verdict := Readout.verdict_text(sheet)
	h._assert_hud("an unbounded raid's verdict bounds the WHOLE trip — want \"%s\", got \"%s\""
			% [expected_verdict, got_verdict],
		got_verdict == expected_verdict)
	h._assert_hud("…and still reads SLOW, the severity the line and the button already carry",
		Readout.verdict_severity(sheet) == SourceForecast.VERDICT_SLOW)
	# 2. THE SEND BUTTON — the last control the player looks at before committing, so it names the same
	#    figure rather than the word "long".
	var send := Q.find_meta_node(sheet, HudWidgets.SEND_HUNT_CONFIRM_META) as Button
	var expected_send := "Send Anyway (more than %d turns)" % trip_floor
	h._assert_hud("…and the Send names the same floor — want \"%s\", got \"%s\""
			% [expected_send, "" if send == null else send.text],
		send != null and not send.disabled and send.text == expected_send)
	# 3. THE ONE-LINE FORM (the targeting banner's and the dock sheet's sentence — the compose sheet
	#    renders the box instead, so it is asserted on the producer). The payload tail is the fixture's
	#    and is not the claim, so the equality is on the whole HEAD through the travel split.
	# **THE ROW IS HANDED IN NOW, not looked up.** `hunt_trip_forecast` took a (floor, party) and read
	# the herd's snapshot table; the sim answers a QUERY for the exact pair instead, so the producer
	# takes the answered ROW. The fixture's table stands in for that answer here — this claim is about
	# how the producer SHAPES a row into a sentence, which is unchanged.
	var forecast := SourceForecast.hunt_trip_forecast(
		h._hud._band_labor._player_band, h._hud._selection.herd(),
		_raid_row_for(h._hud._selection.herd(), SourceForecast.FLOOR_FOOD_PEAK,
			HerdFx.HUNT_FORECAST_PARTY),
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal())
	var expected_head := ("[color=#%s]delivers ≈%d %s over more than %d turns "
		+ "(more than %d hunting + %d travel)") % [
			HudStyle.WARN_HEX, int(forecast.get("animals", 0)), HORIZON_QUARRY_NAME,
			trip_floor, BandFx.FORECAST_HORIZON_TURNS, RAID_TRAVEL_TURNS]
	var got_line := SourceForecast.hunt_forecast_line_bbcode(forecast, HORIZON_QUARRY_NAME)
	h._assert_hud("…and the one-line form opens on the same floor and split — want \"%s\", got \"%s\""
			% [expected_head, got_line],
		got_line.begins_with(expected_head))

## The fixture raid table's row for one (floor, party), standing in for a query answer. It SCANS on
## the row's own two fields rather than rebuilding the `"<floor>:<party>"` key, which renders the
## floor through Rust's float Display and cannot be reproduced exactly from GDScript.
func _raid_row_for(herd: Dictionary, floor: float, party: int) -> Dictionary:
	var estimates: Variant = herd.get("hunt_trip_estimates", {})
	if not (estimates is Dictionary):
		return {}
	for key in (estimates as Dictionary):
		var row_variant: Variant = (estimates as Dictionary)[key]
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if is_equal_approx(float(row.get("floor", -1.0)), floor) \
				and int(row.get("party_workers", 0)) == party:
			return row
	return {}


# =====================================================================================
#  THE QUANTISED TAKE IS ONE EXPRESSION (`fauna::quantise_animal_take`)
# =====================================================================================
# Reported from play on a Wild Boar herd: **six hunters quoted 4.80 food/turn and seven quoted
# 0.36** — the readout falling off a cliff as the crew GREW. `_hunt_delivered_and_waste` carried two
# branches, and the `carryable < 1` one priced delivery as the crew's whole raw `collection` on the
# premise that the only way below one body is a pack too small to hold one. Once the ENGAGEMENT arm
# joined the same `min` that premise was false: at six hunters the crew hauls twenty boar and brings
# down three quarters of ONE, so the branch quoted twenty boar for a take of three quarters of one.
#
# PNG-LESS AND DRIVEN, for the reason `compose_rungs.gd`'s kit-repricing liveness block is: this is
# arithmetic, and a sheet quoting the wrong number renders a perfectly plausible frame. The producer
# is called directly with a BARE fixture — `_hunt_delivered_and_waste` takes an already-kit-priced
# herd, and the reference here is the unpriced one whose terms the constants below state.

## The reported quarry's own wire terms. Every derived figure is composed FROM these rather than
## restated beside them, so the fixture cannot describe a boar whose meat and whose mass disagree.
##   food_per_animal    = 12 × 0.02 = 0.24
##   one hunter CARRIES 0.80 food = 40 biomass = 3.3 boar
##   one hunter REACHES 0.33 boar, of which three in four stay (wariness 0.25)
const BOAR_BODY_MASS := 12.0

const BOAR_PROVISIONS_PER_BIOMASS := 0.02

const BOAR_PER_WORKER_YIELD := 0.80

const BOAR_ENGAGE_RATE := 0.33

## `1 − wariness`, the wire's own `stayFraction`: a quarter of what the party reaches breaks off.
const BOAR_STAY_FRACTION := 0.75

## Dialed so the flow ceiling is NEVER the binding arm across the whole sweep — the room above the
## food peak is 340 − 0.5 × 400 = 140 biomass = 2.80 food, against a top take of 0.54. The claim is
## about the carry and engagement arms, so a ceiling that clipped the sweep would answer elsewhere.
const BOAR_CAPACITY := 400.0

const BOAR_BIOMASS := 340.0

## The two crews the played report names, and the one animal-count step between them:
## 6 → `floor(6 × 0.33)` = 1 engaged, 0.75 stayed; 7 → `floor(7 × 0.33)` = 2 engaged, 1.50 stayed.
const BOAR_CREW_SUB_ONE_ANIMAL := 6

const BOAR_CREW_ONE_ANIMAL := 7

## The sweep, chosen to CROSS the sub-one-animal region rather than sit above it: at one hunter the
## party engages `ENGAGED_AT_LEAST`'s single animal and keeps 0.75 of it, and only at 13 would a
## third body drop. A sweep starting above the crossing would pass with the defect fully restored.
const BOAR_SWEEP_MIN_CREW := 1

const BOAR_SWEEP_MAX_CREW := 12

## The engagement-bound Wild Boar of the played report: wild, un-penned and food-paying, so the axis
## is provisions and the whole-animal quantum is real.
func _quantisation_boar_herd() -> Dictionary:
	return {
		"id": "game_boar_03", "label": "Wild Boar (game_boar_03)", "species": "Wild Boar",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10,
		"husbandry_ceiling": "wild",
		"biomass": BOAR_BIOMASS,
		"carrying_capacity": BOAR_CAPACITY,
		"body_mass": BOAR_BODY_MASS,
		# The sim's own identity, composed rather than restated, for the reason
		# `_engagement_fowl_herd` composes it: `food_per_animal = body_mass × provisions_per_biomass`.
		"food_per_animal": BOAR_BODY_MASS * BOAR_PROVISIONS_PER_BIOMASS,
		"provisions_per_biomass": BOAR_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": BOAR_PER_WORKER_YIELD,
		"engage_rate": BOAR_ENGAGE_RATE,
		"stay_fraction": BOAR_STAY_FRACTION,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}

## ---- THE CADENCE HERD: collection == ceiling == 0.6 OF ONE BODY -------------------------------
## Dialed so all three arms of the `min` coincide BELOW one body and the engagement is not one of
## them, which is the one shape that separates a per-BODY carry clamp from a per-TURN one. Every
## figure is composed from these; the coincidence itself is asserted before the claims that rest on
## it, since a fixture that drifts off it satisfies them for the wrong reason.
##   food_per_animal  = 10 × 0.02 = 0.20
##   one hunter carries 0.12 food = 0.6 of a body
##   room above the food peak = 56 − 0.5 × 100 = 6 biomass = 0.12 food = 0.6 bodies a turn
const CADENCE_BODY_MASS := 10.0

const CADENCE_PROVISIONS_PER_BIOMASS := 0.02

const CADENCE_PER_WORKER_YIELD := 0.12

const CADENCE_CAPACITY := 100.0

const CADENCE_BIOMASS := 56.0

## ONE hunter — the crew the coincidence is dialed for, and the smallest that can be below one body.
const CADENCE_HUNTERS := 1

## The fraction of a body the three arms agree on, named so the expectation and the fixture are one
## number: `delivered = bodies × min(fpa, collection)` = `0.6 × 0.6` of a body's food.
const CADENCE_BODIES_PER_TURN := 0.6

## What rots: the party kills 0.6 of a body's worth per turn and hauls 0.6 of each body it drops, so
## `1 − 0.6` of every kill is left where it fell.
const CADENCE_WASTE_FRACTION := 0.4

## The cadence herd. It publishes NO `engage_rate`, so the engagement arm is unbounded and the claim
## is about the carry clamp alone — the same isolation `_engagement_fowl_herd`'s unbounded twin makes
## in the other direction.
func _cadence_herd() -> Dictionary:
	return {
		"id": "game_hare_02", "label": "Snow Hare (game_hare_02)", "species": "Snow Hare",
		"size_class": "small", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10,
		"husbandry_ceiling": "wild",
		"biomass": CADENCE_BIOMASS,
		"carrying_capacity": CADENCE_CAPACITY,
		"body_mass": CADENCE_BODY_MASS,
		"food_per_animal": CADENCE_BODY_MASS * CADENCE_PROVISIONS_PER_BIOMASS,
		"provisions_per_biomass": CADENCE_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": CADENCE_PER_WORKER_YIELD,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}

## What the party brings down at `workers`, composed the way the sim composes it (engage → retreat)
## rather than read back off the producer under test.
func _boar_brought_down(workers: int) -> float:
	return SourceForecast.animals_stayed(
		SourceForecast.animals_engaged(workers, BOAR_ENGAGE_RATE, SourceForecast.NO_BUILD_DIP),
		BOAR_STAY_FRACTION)

## The producer's delivered take for one crew, at the food peak with no build in flight.
func _boar_delivered(band: Dictionary, herd: Dictionary, workers: int) -> float:
	var take: Dictionary = h._hud._drawercompose._hunt_delivered_and_waste(
		band, herd, SourceForecast.FLOOR_FOOD_PEAK, workers, SourceForecast.IMPROVEMENT_NONE)
	if not bool(take.get("available", false)):
		return -1.0
	return float(take["delivered"])

func _engagement_quantisation_assertions() -> void:
	var band := _delivered_oracle_band()   # output_multiplier 1.0, so no morale factor muddies it
	var herd := _quantisation_boar_herd()
	var fpa := BOAR_BODY_MASS * BOAR_PROVISIONS_PER_BIOMASS

	# (0) THE FIXTURE REALLY STAGES THE SUB-ONE-ANIMAL CASE, and it is the ENGAGEMENT arm that puts it
	#     there. Without this the two claims below are satisfied by any herd whose bounds happen to
	#     coincide, and they would say nothing about the branch that shipped.
	var stayed_six := _boar_brought_down(BOAR_CREW_SUB_ONE_ANIMAL)
	var haulable_six := floorf(float(BOAR_CREW_SUB_ONE_ANIMAL) * BOAR_PER_WORKER_YIELD / fpa)
	h._assert_hud("the fixture separates the arms — %d hunters HAUL %d boar and bring down %.2f"
			% [BOAR_CREW_SUB_ONE_ANIMAL, int(haulable_six), stayed_six],
		stayed_six < 1.0 and haulable_six > 1.0)

	# (1) THE REPORTED PAIR. Six hunters bring down 0.75 of a boar, so they land 0.75 × 0.24 = 0.18
	#     food — NOT the 4.80 the crew's whole carry throughput would be, which is what the retired
	#     `carryable < 1` branch quoted: twenty boar, for three quarters of one.
	var six := _boar_delivered(band, herd, BOAR_CREW_SUB_ONE_ANIMAL)
	var want_six := stayed_six * fpa
	var carry_six := float(BOAR_CREW_SUB_ONE_ANIMAL) * BOAR_PER_WORKER_YIELD
	h._assert_hud(("%d hunters land what they bring DOWN (%.2f food/turn), not what they could carry"
			+ " (%.2f) — got %.2f") % [BOAR_CREW_SUB_ONE_ANIMAL, want_six, carry_six, six],
		is_equal_approx(six, want_six))
	# The seventh hunter tips the engagement to two animals, so 1.50 stay and the take DOUBLES.
	var seven := _boar_delivered(band, herd, BOAR_CREW_ONE_ANIMAL)
	var want_seven := _boar_brought_down(BOAR_CREW_ONE_ANIMAL) * fpa
	h._assert_hud("…and %d hunters land %.2f food/turn — got %.2f"
			% [BOAR_CREW_ONE_ANIMAL, want_seven, seven],
		is_equal_approx(seven, want_seven))

	# (2) MONOTONICITY — the PROPERTY the defect violated, and the one that catches its return in any
	#     other species' numbers. Every arm of the `min` is non-decreasing in the crew, so the take
	#     must be too; the played pair was 4.80 → 0.36, an order of magnitude LOST to one more hunter.
	#     Asserted as a relation over the sweep rather than as twelve literals, so a re-dialed fixture
	#     moves the numbers and not the claim.
	var previous := -1.0
	var broke_at := 0
	var broke_from := 0.0
	var broke_to := 0.0
	for workers in range(BOAR_SWEEP_MIN_CREW, BOAR_SWEEP_MAX_CREW + 1):
		var delivered := _boar_delivered(band, herd, workers)
		if broke_at == 0 and previous >= 0.0 and delivered < previous \
				and not is_equal_approx(delivered, previous):
			broke_at = workers
			broke_from = previous
			broke_to = delivered
		previous = delivered
	h._assert_hud(("the delivered take never falls as the crew grows (%d..%d hunters)"
			+ " — %d hunters read %.2f food/turn after %.2f")
			% [BOAR_SWEEP_MIN_CREW, BOAR_SWEEP_MAX_CREW, broke_at, broke_to, broke_from],
		broke_at == 0)

	# (3) **THE CARRY CLAMP IS CHARGED PER BODY, NOT PER TURN** — the half of the expression the
	#     engagement pair above cannot see, because it never puts the crew below one body of CARRY.
	#     On the cadence herd the three terms coincide at 0.6 of a body: the crew's collection is
	#     0.6 × fpa, the room offers 0.6 bodies a turn, and nothing breaks off. A body still lands
	#     WHOLE on the turn it drops, so the crew hauls its 0.6 × fpa of it and the rest rots —
	#     `0.6 × 0.6 = 0.36 fpa` delivered against `0.6 fpa` killed, i.e. 40% wasted. Averaging the
	#     kill first and clamping THAT by the carry credits the crew the full 0.6 fpa with no waste
	#     at all: 1.67× too high, and silent about the meat on the ground.
	var cadence := _cadence_herd()
	var cadence_fpa := CADENCE_BODY_MASS * CADENCE_PROVISIONS_PER_BIOMASS
	var cadence_take: Dictionary = h._hud._drawercompose._hunt_delivered_and_waste(
		band, cadence, SourceForecast.FLOOR_FOOD_PEAK, CADENCE_HUNTERS,
		SourceForecast.IMPROVEMENT_NONE)
	# The vacuity guard: the fixture must really sit at the coincidence, or the two numbers below are
	# satisfied by any herd whose carry happens not to bind.
	var cadence_collection := float(CADENCE_HUNTERS) * CADENCE_PER_WORKER_YIELD
	var cadence_ceiling := (CADENCE_BIOMASS - SourceForecast.FLOOR_FOOD_PEAK * CADENCE_CAPACITY) \
		* CADENCE_PROVISIONS_PER_BIOMASS
	h._assert_hud(("the fixture sits at the coincidence — collection %.4f, ceiling %.4f,"
			+ " both %.2f of one %.4f body")
			% [cadence_collection, cadence_ceiling, CADENCE_BODIES_PER_TURN, cadence_fpa],
		is_equal_approx(cadence_collection, CADENCE_BODIES_PER_TURN * cadence_fpa)
			and is_equal_approx(cadence_ceiling, CADENCE_BODIES_PER_TURN * cadence_fpa))
	var want_cadence := CADENCE_BODIES_PER_TURN * CADENCE_BODIES_PER_TURN * cadence_fpa
	h._assert_hud("a crew that cannot carry a whole body lands %.4f food/turn, not the room's %.4f — got %.4f"
			% [want_cadence, cadence_ceiling, float(cadence_take["delivered"])],
		is_equal_approx(float(cadence_take["delivered"]), want_cadence))
	h._assert_hud("…and the body it cannot finish carrying is WASTE — %d%%, got %d%%"
			% [int(round(CADENCE_WASTE_FRACTION * 100.0)),
				int(round(float(cadence_take["waste_pct"]) * 100.0))],
		is_equal_approx(float(cadence_take["waste_pct"]), CADENCE_WASTE_FRACTION))


# =====================================================================================
#  THE RETREAT PRICES THE CREW, NOT ONLY THE TAKE
# =====================================================================================
# Reported from play on the same Wild Boar herd: **the stepper capped at 82 while *clear it now* named
# 108** — the sheet offering a crew target the panel then refused to let the player assign. The two
# arms had divided by different reaches, `engagement_carry` cutting by the retreat and
# `engage_workers` sizing on the raw one, on the reasoning that the second mirrors the sim's
# `hunt_take_workers`. A party that keeps one animal in four brings down a quarter as much per hand
# and therefore needs four times the hands to draw the same stock down, so every crew answer divides
# by what STAYS now and 108 is the honest number.
#
# PNG-LESS AND DRIVEN, the reason the block above is: a crew count is a number, and a sheet capping
# at the wrong one renders a perfectly ordinary stepper.

## The played herd's own room — 670 − 0.5 × 700 = 320 biomass — chosen because it is where the two
## arms were measured disagreeing: 320 ÷ (12 × 0.33 × 0.75) = 108 hands to clear, against a raw-reach
## engagement crew of 82. Every other term is the boar's, shared with the quantisation block above.
const RETREAT_CAPACITY := 700.0

const RETREAT_BIOMASS := 670.0

## The crew the chart is composed against. Any crew answers the same two targets — they are functions
## of the floor and the source — so this is the smallest one that exists.
const RETREAT_CHART_CREW := 1

## The retreat a TRAP LINE leaves: `dispersion 0` means nothing is there to be seen, so every animal
## the party reaches stands. It is the wire's own identity, and the A/B's other half.
const TRAP_LINE_STAY := SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF

## A wary quarry's retreat, used only as the B side of the UNCHANGED A/Bs — a pen, an unstalked herd
## and a patch must answer the same crew whatever is substituted here, having no engagement to cut.
const UNCHANGED_PROBE_STAY := 0.25

## The pen's managed per-turn payoff, so its cap is a crew of some size rather than a crew of none.
const PEN_CORRAL_YIELD := 6.4

## The played Wild Boar at the room the two arms were measured on. `stay` is a parameter so the spear
## line and the trap line are ONE fixture differing in one field — the A/B that shows the retreat is
## what moves the cap, rather than something else about the animal.
func _retreat_crew_boar(stay: float) -> Dictionary:
	var herd := {
		"id": "game_boar_09", "label": "Wild Boar (game_boar_09)", "species": "Wild Boar",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10,
		"husbandry_ceiling": "wild",
		"biomass": RETREAT_BIOMASS,
		"carrying_capacity": RETREAT_CAPACITY,
		"body_mass": BOAR_BODY_MASS,
		"food_per_animal": BOAR_BODY_MASS * BOAR_PROVISIONS_PER_BIOMASS,
		"provisions_per_biomass": BOAR_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": BOAR_PER_WORKER_YIELD,
		"engage_rate": BOAR_ENGAGE_RATE,
		"stay_fraction": stay,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}
	ForageFx.floorify(herd)
	return herd

## A PENNED boar: no engagement stage (`NO_ENGAGEMENT_STAGE`, the pen's own wire value) and corralled,
## so the crew is the haul alone and no retreat can reach it. It carries a `corral_yield`, or the
## managed ceiling reads zero and the A/B below compares two crews of none — true, and about nothing.
func _retreat_crew_pen(stay: float) -> Dictionary:
	var pen := _retreat_crew_boar(stay)
	pen["id"] = "game_boar_pen"
	pen["engage_rate"] = SourceForecast.NO_ENGAGEMENT_STAGE
	pen["corralled"] = true
	pen["corral_yield"] = PEN_CORRAL_YIELD
	return pen

## A WILD herd that publishes `NO_ENGAGEMENT_STAGE` — the pen's value on the engagement field alone,
## and the reading the whole plant web gets by never publishing it. This is the case that really
## exercises the arm: it is a whole-animal source, so `take_workers` IS consulted and its engage half
## has to answer "no crew" rather than divide by an unbounded reach. The corralled pen above cannot
## make that claim — a managed source never reaches the whole-animal branch at all.
func _retreat_crew_unstalked(stay: float) -> Dictionary:
	var herd := _retreat_crew_boar(stay)
	herd["id"] = "game_boar_unstalked"
	herd["engage_rate"] = SourceForecast.NO_ENGAGEMENT_STAGE
	return herd

## A wild forage patch carrying a `stay_fraction` no plant web would ever publish, purely so the
## substitution has something to move. The whole claim is that it moves nothing.
func _retreat_crew_patch(stay: float) -> Dictionary:
	var patch := {
		"x": 66, "y": 10,
		"biomass": RETREAT_BIOMASS,
		"carrying_capacity": RETREAT_CAPACITY,
		"provisions_per_biomass": BOAR_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": BOAR_PER_WORKER_YIELD,
		"stay_fraction": stay,
	}
	ForageFx.floorify(patch)
	return patch

## The stepper's own ceiling for a source, through the two real layers the sheet uses.
func _source_worker_cap(src: Dictionary, kind: String) -> int:
	return SourceForecast.max_useful_workers(SourceForecast.forecast_inputs(
		src, kind, HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK,
		SourceForecast.IMPROVEMENT_NONE))

## The two crew-target pills a hunt sheet renders, off the chart model that renders them.
func _herd_crew_targets(herd: Dictionary) -> Dictionary:
	var model := SourceForecast.floor_chart_model(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, RETREAT_CHART_CREW,
		SourceForecast.IMPROVEMENT_NONE, "hunters", LESSON_NOT_YET_LEARNED)
	return {
		"clear": int(model.get("crew_to_clear", SourceForecast.NO_CREW_ANSWER)),
		"hold": int(model.get("crew_to_hold", SourceForecast.NO_CREW_ANSWER)),
	}

func _retreat_crew_assertions() -> void:
	var speared := _retreat_crew_boar(BOAR_STAY_FRACTION)
	var speared_cap := _source_worker_cap(speared, SourceForecast.SOURCE_KIND_HERD)
	var speared_targets := _herd_crew_targets(speared)
	var speared_clear: int = speared_targets["clear"]
	# The RAW-REACH sizing, restated here rather than asked of the layer under test: the engagement
	# crew the cap used to be floored on, with the retreat term left out.
	var speared_forecast := SourceForecast.forecast_inputs(speared, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK,
		SourceForecast.IMPROVEMENT_NONE)
	var raw_engage := SourceForecast.engage_workers(float(speared_forecast["axis_ceiling"]),
		float(speared_forecast["axis_per_animal"]), float(speared_forecast["engage_rate"]),
		float(speared_forecast["dip"]), SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF)

	# (0) THE FRAME REALLY STAGES THE DISAGREEMENT. Without it the claim below is satisfied by any
	#     herd whose two arms happen to coincide, and says nothing about the defect.
	h._assert_hud("the fixture stages the played gap — the raw reach sizes %d hands where *clear it now* names %d"
			% [raw_engage, speared_clear],
		raw_engage < speared_clear)

	# (1) **THE CAP IS NO LONGER BELOW THE TARGET BESIDE IT**, which is the whole of the report.
	h._assert_hud("the stepper caps at %d — at or above the %d *clear it now* names, where the raw reach capped at %d"
			% [speared_cap, speared_clear, raw_engage],
		speared_cap >= speared_clear)

	# (2) **THE INVARIANT, over several species rather than the boar alone** — no crew target on a
	#     hunt sheet may name a crew the stepper refuses to reach. It is the property the whole
	#     change exists to restore, and the one that catches its return in another species' numbers.
	#     `NO_CREW_ANSWER` is a target that renders no pill at all, so it bounds nothing.
	var sweep := {
		"Wild Boar (speared)": speared,
		"Wild Boar (trap line)": _retreat_crew_boar(TRAP_LINE_STAY),
		"Wild Fowl": ForageFx.floorify(_engagement_fowl_herd(FOWL_ENGAGE_RATE)),
		"Wild Aurochs": ForageFx.floorify(_aurochs_big_game_fixture()),
		"Snow Hare": ForageFx.floorify(_cadence_herd()),
	}
	for quarry in sweep:
		var src: Dictionary = sweep[quarry]
		var cap := _source_worker_cap(src, SourceForecast.SOURCE_KIND_HERD)
		var targets := _herd_crew_targets(src)
		var clear: int = targets["clear"]
		var hold: int = targets["hold"]
		h._assert_hud("%s — the cap (%d) reaches both targets it renders (clear %d, hold %d)"
				% [quarry, cap, clear, hold],
			(clear == SourceForecast.NO_CREW_ANSWER or cap >= clear)
				and (hold == SourceForecast.NO_CREW_ANSWER or cap >= hold))

	# (3) **A TRAP LINE IS UNMOVED, and the A/B is what makes (1) a claim about the RETREAT.** One
	#     herd, one room, one crew; the only field that differs is `stay_fraction`. A device that is
	#     not there to be seen keeps everything it reaches, so its cap is the raw-reach sizing exactly.
	var trapped_cap := _source_worker_cap(_retreat_crew_boar(TRAP_LINE_STAY),
		SourceForecast.SOURCE_KIND_HERD)
	h._assert_hud("a trap line keeps what it reaches, so its cap is the raw-reach one (%d) and the spear line's is larger (%d)"
			% [trapped_cap, speared_cap],
		trapped_cap == raw_engage and speared_cap > trapped_cap)

	# (4) **A SOURCE WITH NO ENGAGEMENT STAGE IS BYTE-IDENTICAL WHATEVER IS SUBSTITUTED**, the
	#     byte-identity this arc holds each time the arm reaches a new consumer. Asserted as an A/B on
	#     ONE source rather than as a literal, so a re-dialed fixture moves the number and not the
	#     claim, and over all THREE shapes: the unstalked herd is the one that reaches the whole-animal
	#     branch and so the only one that can testify about `engage_workers`' unbounded reading, while
	#     the pen (managed) and the patch (no body) never get that far and are each unchanged for their
	#     own reason. Any one of them alone would leave two of the three untested.
	var unchanged := {
		"an unstalked herd": [_retreat_crew_unstalked(TRAP_LINE_STAY),
			_retreat_crew_unstalked(UNCHANGED_PROBE_STAY), SourceForecast.SOURCE_KIND_HERD],
		"a pen": [_retreat_crew_pen(TRAP_LINE_STAY), _retreat_crew_pen(UNCHANGED_PROBE_STAY),
			SourceForecast.SOURCE_KIND_HERD],
		"a forage patch": [_retreat_crew_patch(TRAP_LINE_STAY),
			_retreat_crew_patch(UNCHANGED_PROBE_STAY), SourceForecast.SOURCE_KIND_FORAGE],
	}
	for subject in unchanged:
		var pair: Array = unchanged[subject]
		var kept := _source_worker_cap(pair[0], String(pair[2]))
		var cut := _source_worker_cap(pair[1], String(pair[2]))
		h._assert_hud("%s prices the same crew whatever breaks off (%d against %d)"
				% [subject, kept, cut],
			kept == cut and kept > 0)


# ---- THE PANEL BAND IS THE ACTING BAND (issue #510) ----------------------------------------------

## The PARENT band's idle crew. Zero: a colony founding off it took the workers with it, which is the
## state the playtest report was in and the reason the wrongly-resolved sheet capped at nothing.
const PANEL_BAND_PARENT_IDLE := 0

## The COLONY's LIVE idle crew — the number the compose steppers must cap at, since the colony is what
## the Band/City panel is showing.
const PANEL_BAND_COLONY_IDLE := 2

## The idle crew on the panel's STORED copy of the colony. Deliberately unlike either of the other two:
## `set_panel_band` keeps a deep copy taken at render time, so a resolver that hands that copy back
## instead of re-resolving the entity against the live roster answers a third, equally distinct number
## rather than coinciding with the parent's and hiding inside its failure.
const PANEL_BAND_STALE_IDLE := 9

## Where the colony sits in the roster, 1-based — the picker labels bands positionally
## (`HudFormat.band_display_name`), so this is both its index and the `Band 2` its face must read.
## Named because the frame's whole point is that the SECOND band is the one in focus.
const PANEL_BAND_COLONY_INDEX := 2

## The crew both sheets are dialed to before the cap is read. Above every candidate idle count
## including the stale one, so the rendered number is decided by the CAP and never by the dial.
const PANEL_BAND_DIALED_CREW := 12

## What `_band_picker_face` answers when the open sheet has no `Band:` row at all — a SENTINEL rather
## than "", so a sheet that never built the picker fails the equality instead of quietly satisfying it.
const PANEL_BAND_PICKER_ABSENT := "<no Band: picker>"

## TWO player bands where the SECOND is the one the player is looking at. Both stand within hunt reach
## and work range of the (66,10) reference herd and food tile, so neither sheet's answer can turn on
## distance — the ONLY thing that differs between them is the idle crew, which is precisely what makes
## composing for the wrong band visible in the frame.
func _panel_band_roster() -> Array:
	return [
		BandFx.with_band_id({"entity": 841, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": PANEL_BAND_PARENT_IDLE, "hunt_reach": 7, "work_range": 2,
			"max_expedition_party_size": 8, "activity": "forage", "labor_assignments": []}),
		BandFx.with_band_id({"entity": 842, "faction": 0, "size": 40, "current_x": 67, "current_y": 10,
			"working_age": 6, "idle_workers": PANEL_BAND_COLONY_IDLE, "hunt_reach": 7, "work_range": 2,
			"max_expedition_party_size": 8, "activity": "forage", "labor_assignments": []}),
	]

## The colony as the Band/City panel STORED it — the deep copy `set_panel_band` keeps, here stale about
## the one field the compose steppers cap against.
func _stale_panel_band() -> Dictionary:
	var stale: Dictionary = (_panel_band_roster()[PANEL_BAND_COLONY_INDEX - 1] as Dictionary).duplicate(true)
	stale["idle_workers"] = PANEL_BAND_STALE_IDLE
	return stale

## The `Band:` picker's rendered FACE. Found STRUCTURALLY — `_build_band_picker` is the only row that
## pairs the `BAND_PICKER_LABEL` field key with an `OptionButton` — because the face is the very thing
## under test, so reaching the control by the text it is claimed to show would assert nothing.
func _band_picker_face(root: Node) -> String:
	if root == null:
		return PANEL_BAND_PICKER_ABSENT
	var key_seen := false
	for child in root.get_children():
		if child is Label and (child as Label).text == HudWorkVocab.BAND_PICKER_LABEL:
			key_seen = true
		elif key_seen and child is OptionButton:
			return (child as OptionButton).text
	for child in root.get_children():
		var found := _band_picker_face(child)
		if found != PANEL_BAND_PICKER_ABSENT:
			return found
	return PANEL_BAND_PICKER_ABSENT

## The three claims an open compose sheet owes the band the player has in FOCUS, made on BOTH sheets
## because `_resolve_assign_band` is injected into each of them separately.
##
## They are three because they fail apart: the picker's face is what the player READ (the report's
## screenshot showed `Band 1` under a `Band 2` header), the composed entity is what the commit would
## NAME, and the stepper's cap is what actually stopped the player staffing the hunt. A sheet could get
## any one of them right while the resolver is wrong.
func _assert_composes_for_panel_band(state: String, colony: Dictionary, composed_entity: int) -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var want_face := HudFormat.band_display_name(colony, PANEL_BAND_COLONY_INDEX)
	var got_face := _band_picker_face(sheet)
	h._assert_hud("%s: the Band: picker names the band the panel is on (%s, got %s)"
			% [state, want_face, got_face],
		got_face == want_face)
	var want_entity := int(colony.get("entity", -1))
	h._assert_hud("%s: the sheet composes for that band's entity (%d, got %d)"
			% [state, want_entity, composed_entity],
		composed_entity == want_entity)
	var got_crew := Readout.stepper_value(sheet)
	h._assert_hud(("%s: the crew stepper caps at its LIVE idle workers (%d, got %d) — not the parent's"
			+ " %d and not the panel's stale %d")
			% [state, PANEL_BAND_COLONY_IDLE, got_crew, PANEL_BAND_PARENT_IDLE, PANEL_BAND_STALE_IDLE],
		got_crew == PANEL_BAND_COLONY_IDLE)
