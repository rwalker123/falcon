extends RefCounted

## Hunting: crews, raids, forecasts and the whole-animal cap.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 345

## The countdown verdict's opening, as a needle — the precondition every claim about that sentence
## rests on ("this model reached the reaching branch at all").
const REACHES_FLOOR_NEEDLE := "Reaches the floor"

## **A NEEDLE FOR A RETIRED CLAUSE, KEPT SO IT STAYS RETIRED.** The countdown used to close by
## promising the equilibrium it was counting down to, on sources that had one; the readout states that
## itself, in `VERDICT_HOLDS_AT_FLOOR`, the moment it is true. Spelled out because there is no const
## left to compose it from.
const RETIRED_AFTERMATH_NEEDLE := "then holds it"

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")
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

## **THE THREE DESTINATION CAPACITIES THE FLOOR FLAG IS PROBED AT**, all against the taming herd's own
## live `K` of 2150 (≈11 Red Deer at the best-harvest floor, which is what the flag flies today).
##
## The first is an ordinary pen — richer than the range the herd walks, so the threshold it quotes
## (≈15) cannot be mistaken for a restatement of the live one. The second is the LIVENESS probe: only
## the published field changes between them, so a clause composed from anything else renders the same
## string twice. The third is the case the `-1` sentinel exists to keep apart from *nothing queued* —
## a pen struck on ground that would carry nothing at all.
const FLOOR_FLAG_DESTINATION_CAPACITY := 3000.0
const FLOOR_FLAG_DESTINATION_CAPACITY_RICHER := 4000.0
const FLOOR_FLAG_DESTINATION_CAPACITY_BARREN := 0.0

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
	# **THE SOURCE-CONDITIONAL ROWS COME OUT FIRST** (`docs/plan_standing_upkeep.md` §2.2). The keeping
	# row renders iff the source has a rung that can be lost — a Tended Patch does, a wild herd does
	# not — so leaving it in would turn a fact about the two FIXTURES into a claim about the two webs'
	# grammar, which is the one thing this assertion exists to be.
	var forage_spine: Array = _spine_grammar(h._compose_spines[forage_key])
	var hunt_spine: Array = _spine_grammar(h._compose_spines[hunt_key])
	h._assert_hud(("the forage and local-hunt sheets read in the SAME control order — forage %s, hunt %s"
		% [str(forage_spine), str(hunt_spine)]), forage_spine == hunt_spine)

## A captured spine with its source-conditional rows dropped — what is left is the sheet's GRAMMAR,
## which is what the two webs must share.
func _spine_grammar(spine: Array) -> Array:
	return spine.filter(func(tag: Variant) -> bool:
		return not Spine.COMPOSE_SPINE_SOURCE_CONDITIONAL.has(String(tag)))

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
		# **THE THREE TERMS THE QUANTISER NEEDS, AND THE WHOLE OF WHAT THIS PACK WAS MISSING.** A take
		# is `min(room, crew carry, what stays) ÷ one body` and every one of those is a BIOMASS, so a
		# species that pays no food is quantised exactly as a deer is — once it states a body. The
		# harness used to DERIVE `body_mass` from `food_per_animal ÷ provisions_per_biomass`, which is
		# `0/0` here, so the wolf reached the sheet with no quantum, both food paths bailed, and its
		# material rows fell through to a crew-throughput line that scaled with hands the pack never
		# reached. The carry is stated too rather than left to the fixture default, since these three
		# and `WOLF_ROOM_AT_PEAK` are one arithmetic and a defaulted term is one nobody can check.
		"body_mass": WOLF_BODY_MASS,
		"per_worker_biomass": WOLF_CARRY,
		"engage_rate": WOLF_ENGAGE_RATE,
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
		# **THE TWO INVESTMENT RUNGS' PAYOFFS, AS VECTORS.** `pastoral_yield` / `corral_yield` are
		# PROVISIONS, so a wolf's are honestly `0` and both rungs advertised `0.00 food` or nothing —
		# the two rungs a player would actually take on such a species offering no reason to take
		# them. Corral above Tame, so the ladder still reads as ascending in the account it pays.
		"pastoral_yield": 0.0,
		"corral_yield": 0.0,
		# **THE TWO RUNGS' BUILD PRICES, without which `improvement_forecast` declines to quote the
		# deal at all** and the payoff below would be asserted against a `{}`. A cost of zero is the
		# wire saying it prices no such job on this source (`docs/plan_standing_upkeep.md` §2.2 — the
		# retired dip fraction used to be this gate), so the frame states real ones and exercises the
		# quotable path rather than the refusal one.
		"tame_work_cost": HerdFx.ANIMAL_TAME_WORK_COST,
		"corral_work_cost": HerdFx.ANIMAL_CORRAL_WORK_COST,
		"pastoral_material": [
			{"material_id": WOLF_MATERIAL_ID, "amount": WOLF_PASTORAL_HIDE},
		],
		"corral_material": [
			{"material_id": WOLF_MATERIAL_ID, "amount": WOLF_CORRAL_HIDE},
		],
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
## The crew-throughput rate the RETIRED expression clamped against. It is still on the fixture and
## still read by the LAST-DITCH arm (a herd the wire describes too thinly to quantise), and it is
## deliberately NOT the rate the quantised take implies — a fixture whose two answers coincide cannot
## tell the fix from the bug.
const WOLF_MATERIAL_PER_WORKER := 0.11

## ---- THE PACK'S OWN BIOMASS ARITHMETIC ---------------------------------------------------------
## Stated so the assertions can be read by eye, and sized so the REACH is what binds across every crew
## the sheet can compose — which is the regime the defect lived in.
##
##   room at the food peak = 240 − 0.5 × 400 = 40 biomass  ⇒  2 whole wolves affordable
##   one hunter carries      40 biomass                    ⇒  2 bodies haulable per hunter
##   one hunter REACHES      0.25 wolves                   ⇒  `w × 0.25`, UNROUNDED
##
## So `killed` is the REACH at every crew up to 7 (1.75 wolves, still under the room's 2) and rises
## with every hand — a tenth of a hide per hunter against the retired crew-throughput line's 0.11,
## two linear readings a tenth apart that only a magnitude claim can tell apart. That is the same
## shape the edible Wild Boar pair proved the food side with, which is why the wolf's claim can be
## stated the same way.
const WOLF_BODY_MASS := 20.0
const WOLF_CARRY := 40.0
const WOLF_ENGAGE_RATE := 0.25
## The largest crew the REACH is still the binding arm at: `7 × 0.25` = 1.75 wolves, under both the
## room's 2 whole bodies and the 14 the crew could haul. Every claim below is made inside that regime,
## because it is the only one where the take is a reading of the reach alone.
const WOLF_REACH_BOUND_CREW := 7
## `max(0, B − f·K)` at the food peak, restated so the oracle below divides by a number this file owns
## rather than by one it recomposes out of the client's own `escapement_room`.
const WOLF_ROOM_AT_PEAK := WOLF_BIOMASS - 0.5 * WOLF_CAPACITY
## What each INVESTMENT rung pays once it stands, per turn, in hides. Ascending Tame < Corral, in the
## only account this species has, so the ladder reads as a ladder rather than as two equal offers.
const WOLF_PASTORAL_HIDE := 0.34
const WOLF_CORRAL_HIDE := 0.52
## What ONE hauled wolf is worth in hides on a RAID. The trip line rounds its payload to whole units
## (a trip is not a rate), so this is sized to clear 1.0 at the frame's kill counts — a payload that
## rounded to `~0` would render a clause the reader could not tell from a suppressed one.
const WOLF_RAID_HIDE_PER_ANIMAL := 0.55

## What the frame must read, composed at assertion time from the crew the sheet actually landed on
## (see above): the QUANTISED delivery in biomass, valued through the pack's per-biomass hide rate.
##
## **THE ORACLE IS `HerdFx.hunt_take_oracle`, WHICH IS UNIT-FREE.** It restates the sim's
## `quantise_animal_take`, and that arithmetic never mentions an account — it is a room, a carry, a
## quantum and a reach, all in whatever unit its caller states them in. Passing the pack's biomass
## terms is therefore the same cross-check the deer's food terms get, not a second oracle.
func _wolf_material_take(crew: int) -> float:
	# The reach is `crew × engage_rate`, UNROUNDED, exactly as `fauna::animals_engaged` states it: a
	# reach is a rate, and a lone hunter reaching a quarter of a wolf is a body every fourth turn
	# rather than the whole one the retired floor-of-one quoted.
	return float(HerdFx.hunt_take_oracle(float(crew) * WOLF_CARRY, WOLF_ROOM_AT_PEAK,
		WOLF_BODY_MASS, float(crew) * WOLF_ENGAGE_RATE)["delivered"]) \
		* WOLF_MATERIAL_PER_BIOMASS

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

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the actor band changing
	# RE-SEEDS the composition from that band's own standing staffing, so the dialed 8 is gone and the
	# stepper opens on the `WORKER_STEP` floor (Band 2 hunts nothing here). The note beneath it is the
	# newly-selected band's own labour bound — `2 of 7 useful` — which is the "selection → actor band →
	# stepper" chain this pair has always demonstrated, now stated by the seed rather than by a clamp.
	var second_band: Dictionary = _two_player_bands()[1]
	h._hud._compose.set_hunt_band(int(second_band["entity"]))
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
	# (3) **THE CLIENT-SIDE ⚠ GATE IS RETIRED.** A pair of assertions stood here holding that the gate
	# and the verdict read ONE projection — which was the right invariant while the client answered the
	# ⚠ at all. `LaborAssignment.overdraws` carries the whole verdict now (intent AND ability), so the
	# projection is no longer one of the flag's inputs and there is nothing left for it to agree with.
	# The claim that replaced it is `_overdraw_is_the_wires_answer` at the end of this chapter.

	# 3r-b — THE SAME BIRD WITH NO ENGAGEMENT STAGE. This is the pen's wire value and the plant web's
	# silence, and it must read exactly as the sheet always did: carry-bound, and capped at the two
	# haulers who can carry the peak drop.
	var fowl_unreached := _engagement_fowl_herd(SourceForecast.NO_ENGAGEMENT_STAGE)
	# **THE SEAM IS RESET, BECAUSE THIS PAIR RESTAGES ONE HERD ID WITH DIFFERENT BIOLOGY.** A crew-take
	# answer is keyed on band + herd id + (kit, cap, floor) — and every one of those is identical across
	# the two fowls, only the `engage_rate` differing, which is a SPECIES CONSTANT the live wire cannot
	# flip under a standing id. So the seam correctly serves the bound bird's curve to the unbound one's
	# sheet (measured: a take of `≈10 Wild Fowl` where nothing stalks the quarry), and the artifact is
	# the restaging rather than the caching. It is the world-boundary reset used for what it is for.
	h._hud.forecast_query().reset()
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
	# **THE TWO INVESTMENT RUNGS QUOTE THEIR PAYOFF NOW, AND THEY QUOTED NOTHING BEFORE.** `Tame` and
	# `Corral` pay `pastoral_yield` / `corral_yield`, which are PROVISIONS — honestly `0` on a wolf —
	# so the two rungs a player would take on such a species advertised no reason to take them.
	#
	# **DRIVEN THROUGH THE PRODUCER, PNG-LESS, AND THAT IS FORCED RATHER THAN CHOSEN.** This wolf's
	# `husbandry_ceiling` is `wild`, so the sheet renders NO Tame or Corral rung to read a face off —
	# which is correct behaviour and is precisely why the claim cannot be made on the render. The
	# chain asserted is the real one (`improvement_forecast` → `payoff_material` → `_payoff_terms`),
	# and it is asserted as a PAIR with the ascending claim: a payoff appearing on both rungs at one
	# number would satisfy "quotes something" and still misread the ladder.
	var tame_face: String = h._hud._drawercompose._improvement_payoff_terms(
		wolf, SourceForecast.LABOR_KIND_HUNT, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, h._hud._band_labor._player_band)
	var corral_face: String = h._hud._drawercompose._improvement_payoff_terms(
		wolf, SourceForecast.LABOR_KIND_HUNT, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_CORRAL, h._hud._band_labor._player_band)
	h._assert_hud("the wolf's Tame rung quotes the hides it would pay (got \"%s\")" % tame_face,
		tame_face.contains(WOLF_MATERIAL_ID)
			and tame_face.contains(SourceForecast.format_magnitude(WOLF_PASTORAL_HIDE)))
	h._assert_hud("…and its Corral rung quotes MORE of them, so the ladder still ascends (got \"%s\")"
			% corral_face,
		corral_face.contains(WOLF_MATERIAL_ID)
			and corral_face.contains(SourceForecast.format_magnitude(WOLF_CORRAL_HIDE)))
	h._assert_hud("…and neither quotes a food figure a wolf does not pay",
		not tame_face.contains(SourceForecast.YIELD_ACCOUNT_FOOD)
			and not corral_face.contains(SourceForecast.YIELD_ACCOUNT_FOOD))
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

	# **A VERDICT MAY NOT PROMISE AN AFTERMATH AT ALL — IT STATES THE COUNTDOWN AND STOPS.** Reported
	# from play: a Rabbit Warren at `Take everything` read `0 hold it after` beside "Reaches the floor
	# in 2 turns, then holds it — taking only what grows back". The herd is GONE at floor 0; there is
	# nothing to hold and nothing that grows back, and the panel was contradicting its own crew target.
	#
	# That was first fixed by FORKING the sentence on the regrowth at the floor, so a stripped herd
	# dropped the clause and a stripped patch — which really does reseed from bare ground — kept it.
	# The clause is now off BOTH readings (`VERDICT_REACHES_FORMAT`): what a source does once it
	# arrives is the `VERDICT_HOLDS_AT_FLOOR` sentence's own job, said the moment it is true. With
	# nothing left for the fork to choose between, `harvest_verdict` takes no `regrows` term.
	#
	# **THE THREE MODELS ARE KEPT, and they are what stops the fork coming back**: a stripped herd (no
	# aftermath), a stripped patch (a real one) and a HEALTHY herd above its floor (also a real one).
	# Any re-added branch — on the web, on floor 0, or on the regrowth — puts the clause back on at
	# least one of the three, and the single claim below covers all three at once.
	var strip_crew := 64
	var stripped_herd := SourceForecast.floor_chart_model(allee_herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_MIN, strip_crew, "hunters", LESSON_NOT_YET_LEARNED)
	var stripped_patch = SourceForecast.floor_chart_model(h._floor_chart_drawn_patch,
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_MIN, strip_crew, "foragers",
		LESSON_NOT_YET_LEARNED)
	var stripped_herd_text := String((stripped_herd.get("verdict", {}) as Dictionary).get("text", ""))
	var stripped_patch_text = String((stripped_patch.get("verdict", {}) as Dictionary).get("text", ""))
	# The healthy herd is the third model, and it is the one a WEB branch gets wrong: it regrows at
	# its floor like anything else, so "is a herd" and "cannot regrow" stop coinciding here.
	var held_herd := SourceForecast.floor_chart_model(
		ForageFx.floorify(HerdFx.grazing_healthy_herd_fixture()), SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED)
	var held_herd_text := String((held_herd.get("verdict", {}) as Dictionary).get("text", ""))
	# The PRECONDITION: all three really are stating the countdown. Without it the absence claim below
	# passes on three models that reached some other verdict entirely.
	h._assert_hud("all three sources REACH their floor, so all three are stating the reaching verdict",
		stripped_herd_text.contains(REACHES_FLOOR_NEEDLE)
			and stripped_patch_text.contains(REACHES_FLOOR_NEEDLE)
			and held_herd_text.contains(REACHES_FLOOR_NEEDLE))
	h._assert_hud(("…and NONE of them promises an aftermath — not the stripped herd, not the patch"
			+ " that reseeds, not the healthy herd (%s / %s / %s)")
			% [stripped_herd_text, stripped_patch_text, held_herd_text],
		not stripped_herd_text.contains(RETIRED_AFTERMATH_NEEDLE)
			and not stripped_patch_text.contains(RETIRED_AFTERMATH_NEEDLE)
			and not held_herd_text.contains(RETIRED_AFTERMATH_NEEDLE))

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
	# **AND THE FLAG SAYS WHEN THAT COUNT IS MOVING.** A build raises the source's capacity every turn,
	# so `floor x K` — the animal count this flag flies — CLIMBS while the percentage beside it sits
	# still, and the take falls under it. A player who cannot see the threshold move reads that fall as
	# the herd being poor. The mark is a DIRECTION and never a magnitude: nothing on the wire states next
	# turn's capacity, so a figure here would be invented.
	#
	# **THE PAIR IS THE CLAIM** — a flag that always carried the mark satisfies the first alone and one
	# that never did satisfies the second, and the second is the ordinary sheet in every other frame.
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK,
		"capacity": 2150.0, "body_mass": 100.0, "quarry": "Red Deer", "floor_climbing": true})
	h._assert_hud("a floor rising under a build is flagged as moving, with no magnitude invented",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer" + HarvestFloorChart.FLOOR_FLAG_CLIMBING_SUFFIX)
	flag_probe.set_model({"known": true, "floor": SourceForecast.FLOOR_FOOD_PEAK,
		"capacity": 2150.0, "body_mass": 100.0, "quarry": "Red Deer", "floor_climbing": false})
	h._assert_hud("…and a herd with no build in flight carries no mark at all",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer")
	# **AND WHERE THE CLIMB STOPS.** The mark above says the threshold is moving and the wire now says
	# what it is moving TOWARD: `buildDestinationCapacity` is the source's `K` at the rung its build
	# was sent to, so `floor x` it is the same threshold at the destination's standing. Everything
	# below runs through the REAL model (`floor_chart_model` over a wire-shaped herd), not a hand-built
	# one, because the claim is that a WIRE FIELD reaches the flag — a hand-built model would assert
	# the formatting and nothing about the decode.
	#
	# **THE TWO CAPACITIES ARE DELIBERATELY DIFFERENT** (2150 standing, 3000 penned ⇒ ≈11 and ≈15). An
	# expectation struck where they coincide is satisfied by a clause that merely restates the live
	# figure, which is the shape of a passing-but-blind equality; here only the published destination
	# produces the rendered number.
	var destination_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	destination_herd[SourceForecast.FORECAST_BUILD_DESTINATION_KEY] = SourceForecast.RUNG_KEY_PEN
	destination_herd[SourceForecast.FORECAST_BUILD_DESTINATION_CAPACITY_KEY] = \
		FLOOR_FLAG_DESTINATION_CAPACITY
	flag_probe.set_model(SourceForecast.floor_chart_model(destination_herd,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED))
	var destination_flag := flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
	h._assert_hud("a climbing floor states the threshold it is climbing to, and names the rung (got '%s')"
		% destination_flag, destination_flag == "leave 50% · ≈11 Red Deer ↑ ≈15 at Corralled")
	# **THE LIVENESS HALF: the figure MOVES WITH THE WIRE.** Same herd, same live capacity, same rung,
	# one published destination changed — so an implementation that recomposed the live number, or
	# hard-wired a gain of its own, renders the same string twice and fails here while passing above.
	destination_herd[SourceForecast.FORECAST_BUILD_DESTINATION_CAPACITY_KEY] = \
		FLOOR_FLAG_DESTINATION_CAPACITY_RICHER
	flag_probe.set_model(SourceForecast.floor_chart_model(destination_herd,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED))
	var richer_flag := flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
	h._assert_hud("…and it is the WIRE's number: a richer destination renders a higher threshold (got '%s')"
		% richer_flag, richer_flag == "leave 50% · ≈11 Red Deer ↑ ≈20 at Corralled")
	# **A CAPACITY OF ZERO IS A READING, NOT AN ABSENCE** — the case the sentinel exists to keep apart.
	# A pen struck on rock really would hold nothing, and swallowing that as *nothing queued* would
	# quietly hide the one destination a player most needs talking out of.
	destination_herd[SourceForecast.FORECAST_BUILD_DESTINATION_CAPACITY_KEY] = \
		FLOOR_FLAG_DESTINATION_CAPACITY_BARREN
	flag_probe.set_model(SourceForecast.floor_chart_model(destination_herd,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED))
	h._assert_hud("a destination that would hold NOTHING still states itself — zero is not absent",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer ↑ 0 at Corralled")
	# **AND A HERD HEADING NOWHERE STATES NO CLAUSE AT ALL — no dash, no zero, no empty `at`.** The
	# PRECONDITION is asserted first and it is the whole point of the pair: without it this passes on a
	# row that carried a perfectly good destination the flag simply declined to render, which is the
	# same silence and a different bug.
	var unqueued_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	h._assert_hud("the precondition — this row really does carry the no-destination sentinel",
		SourceForecast.build_destination_capacity(unqueued_herd,
			HudComposeVocab.BARE_FORECAST_PREFIX) == SourceForecast.NO_BUILD_DESTINATION_CAPACITY)
	flag_probe.set_model(SourceForecast.floor_chart_model(unqueued_herd,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED))
	h._assert_hud("…so its flag keeps the bare mark and quotes nothing",
		flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
			== "leave 50% · ≈11 Red Deer" + HarvestFloorChart.FLOOR_FLAG_CLIMBING_SUFFIX)
	# **AND THE SENTINEL IS WHAT DOES THE SUPPRESSING, not the missing rung beside it.** The pair above
	# states neither, so it is satisfied by a flag that only ever checks the rung — this row NAMES its
	# destination and prices it at the sentinel, which leaves the capacity's own `< 0` as the only
	# thing that can hold the clause back.
	var unpriced_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	unpriced_herd[SourceForecast.FORECAST_BUILD_DESTINATION_KEY] = SourceForecast.RUNG_KEY_PEN
	flag_probe.set_model(SourceForecast.floor_chart_model(unpriced_herd,
		SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, strip_crew, "hunters", LESSON_NOT_YET_LEARNED))
	var unpriced_flag := flag_probe._floor_flag_text(SourceForecast.FLOOR_FOOD_PEAK, 1075.0)
	h._assert_hud("a NAMED destination the wire prices at the sentinel still quotes nothing (got '%s')"
		% unpriced_flag, unpriced_flag
			== "leave 50% · ≈11 Red Deer" + HarvestFloorChart.FLOOR_FLAG_CLIMBING_SUFFIX)
	# …and the MODEL is what decides it, off the wire's own `buildTurnsRemaining` rather than off any
	# reading the client composes. The sentinels are NEGATIVE (`BUILD_TURNS_HOLDS` / `_ROTS` /
	# `_QUEUE_BLOCKED`), so a stalled or parked build correctly states no climb: nothing is rising.
	h._assert_hud("the model reads the climb off the wire's own build countdown",
		bool(SourceForecast.floor_chart_model(
			ForageFx.floorify(HerdFx.grazing_healthy_herd_fixture()), SourceForecast.SOURCE_KIND_HERD,
			HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, strip_crew,
			"hunters", LESSON_NOT_YET_LEARNED).get("floor_climbing", true)) == false)
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
	# **THE REFUSAL IS FORWARD-LOOKING NOW, so it is staged by the trailing `takes_next_turn`.** The
	# headline above it states what the crew banks NEXT turn, so the sentence may only claim the crew
	# takes nothing when next turn's growth really does not cross the floor — `false` here is a source
	# far enough under it that it does not.
	var at_floor := SourceForecast.harvest_verdict({"reached_turn": SourceForecast.PROJECTION_REACHED_NONE,
		"settled_fraction": 0.0, "series": []}, ForageFx.FLOOR_CHART_CREW, 96.0, 2150.0,
		SourceForecast.FLOOR_FOOD_PEAK, 0, "hunters", 100.0, "Red Deer", false)
	h._assert_hud("the at-floor verdict quotes the threshold in the SAME unit the flag flies",
		String(at_floor.get("text", "")).contains("≈11 Red Deer")
			and not String(at_floor.get("text", "")).contains("1075"))
	# **AND THE PAIR THAT MAKES `takes_next_turn` MEAN SOMETHING: the same standing stock, the same
	# empty room, and a growth that DOES cross the floor.** That source is holding at its floor and
	# living off the regrowth — the state whose headline used to read `0.00 FOOD` under *"takes nothing
	# until it grows past 1075"* — so it states neither the refusal nor a settling verdict, both of
	# which would be about a source that is not there yet.
	var holding := SourceForecast.harvest_verdict({"reached_turn": SourceForecast.PROJECTION_REACHED_NONE,
		"settled_fraction": 0.0, "series": []}, ForageFx.FLOOR_CHART_CREW, 96.0, 2150.0,
		SourceForecast.FLOOR_FOOD_PEAK, 0, "hunters", 100.0, "Red Deer", true)
	h._assert_hud("…while a source held AT its floor states that it is holding, not that it is empty",
		String(holding.get("text", "")) == SourceForecast.VERDICT_HOLDS_AT_FLOOR
			and String(holding.get("severity", "")) == SourceForecast.VERDICT_OK)

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
	HerdFx.price_animal_build(penned_boar)
	h._hud._compose.reset_hunt_source()
	h._show_herd(penned_boar)
	h._compose_herd(penned_boar, PELT_FRAME_HUNTERS)
	await h._settle()
	await h._save("herd_investment_corral_offer")
	var corral_offer = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "corral")
	h._assert_hud("Corral's OFFERED payoff names BOTH products too, food leading",
		corral_offer != null
		and String(corral_offer.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
			== HudWidgets.IMPROVEMENT_STATE_OFFERED
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

	# ---- …AND THE MATERIAL ROWS ARE ROWS OF THAT SAME DELIVERY ------------------------------------
	_material_take_tracks_delivery_assertions()

	# ---- …AND SO ARE AN INEDIBLE QUARRY'S, NOW THAT THE QUANTISER IS STATED IN BIOMASS ------------
	_wolf_material_take_assertions()

	# ---- …AND THE EDIBLE QUARRY LANDS EXACTLY WHAT IT LANDED IN FOOD -----------------------------
	_edible_take_is_unchanged_assertions()

	# ---- THE ⚠ HAS ONE PRODUCER, AND THREE SURFACES READ IT ---------------------------------------
	_overdraw_is_the_wires_answer()

	# ---- THE RETREAT PRICES THE CREW, NOT ONLY THE TAKE ------------------------------------------
	_retreat_crew_assertions()

	# ---- …AND ONE FRAME OF IT, SO THE THREE ACCOUNTS CAN BE READ TOGETHER -------------------------
	await _material_take_state()

	# ---- THE TAKE IS THE SIM'S ANSWER, AND THE READOUT SAYS SO ------------------------------------
	await _crew_take_readout_assertions()

	# ---- …AND SO IS EVERY CREW ANSWER BESIDE IT, DOWN TO A FRACTION OF AN ANIMAL -----------------
	await _subone_take_assertions()

	# ---- …AND A TARGET NO CREW REACHES SAYS SO, RATHER THAN VANISHING ----------------------------
	await _unreachable_target_state()

	# ---- …AND IT IS RE-ASKED AS THE HARVEST FLOOR MOVES, RATE-LIMITED -----------------------------
	await _crew_take_follows_the_drag_assertions()


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
	# **THE UNIFORM CONTROL for the split line below**, and without it that state's claim passes on a
	# sheet that states a split for every band. This band publishes ONE crew — the shipped case — so
	# there is no division to report and the sheet says nothing about who can and who cannot.
	h._assert_hud("…and a UNIFORMLY equipped party states no split either",
		Readout.hunt_crew_split_line(speared_sheet) == "")

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

	# State gate-d — **THE SAME MAMMOTH, TEN SPEARS AMONG SEVENTEEN HUNTERS** (issue #520). The gate
	# clears, because it is asked at ONE tier and that tier is the best-equipped crew's — which is the
	# reassuring half. Ten of these hunters can take the animal and seven cannot at any headcount, and
	# until this the sheet said nothing about the seven.
	await _combat_gate_split_state()

	# Reset for whatever renders next.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)


## **THE SENTENCE, SPELLED OUT rather than recomposed through `SourceForecast`'s own format.** An
## expectation built from the code under test can only agree with itself, and both candidate readings
## here — the armed count against the barred one, "bare-handed" against "hold too little gear" —
## differ by a word or a digit, which a `contains` would not separate.
##
## **IT COUNTS THE PARTY, NOT THE BAND.** `LOCAL_HUNT_HUNTERS` (6) drawn from an armed run of 4 is
## `4 of your 6`; the band-level reading of the same rows would be `4 of your 17`, which over a
## `HUNTERS 6` stepper names more bare hands than the party has people.
const GATE_SPLIT_LINE := "⚠ 4 of your 6 hunters can take Woolly Mammoth; the other 2 hold too little gear and land nothing on it at any headcount."

## A party that fits INSIDE the armed run — every hunter sent is holding a spear, so this hunt has no
## split whatever the rest of the band is carrying.
const GATE_SPLIT_COVERED_HUNTERS := 3

## **THE KIT LINE'S OWN SENTENCE ON THE SAME PARTLY-ARMED BAND**, spelled out rather than recomposed
## through `KitRoster.tier_hint` — an expectation built from the function under test agrees only with
## itself, and the whole finding here is a clause of four small numbers.
##
## **THE TIERS ARE THE EQUIPPED ONES AND THE COVERAGE IS NOT, WHICH IS THE DEFECT IN ONE LINE.**
## `with_short_spears` holds four spears; `effective_tiers` resolves `attack` through the band's best
## live item, so the line quoted `attack 20.0` to a party of six while the sim priced two of the six
## bare-handed inside its take curve. The take was right and the line was wrong about why. The fix
## STATES the coverage — it does not blend the attack, which would describe nobody and would be a
## third number for a division `huntCrews` has already published.
const GATE_SPLIT_KIT_HINT := "attack 20.0 · carry 40.0 per hunter · 4 of 6 equipped · spears 87 · sled 54"

## **THE SAME BAND, THE SAME KIT, A PARTY THE GEAR COVERS — AND THE CLAUSE STILL PRINTS.** It is the
## half that makes the assertion above a claim about the NUMBERS rather than about a clause existing:
## `4 of 6` and `3 of 3` differ in every digit, so a client that hardcoded either, or that only ever
## printed on a shortfall, fails one of the pair.
##
## **FULL COVERAGE IS STATED, NOT WITHHELD.** A clause that appeared only when the band was short
## would be a warning glyph in words: the player would have no baseline to watch `6 of 6` become
## `5 of 6` against, and a clause POPPING INTO EXISTENCE as the stepper crosses the gear's reach reads
## as the step having broken something. It is the same rule the condition clauses beside it follow —
## `spears 74` prints every frame, not only once the spears are nearly gone.
const GATE_SPLIT_COVERED_KIT_HINT := "attack 20.0 · carry 40.0 per hunter · 3 of 3 equipped · spears 87 · sled 54"

## The partly-equipped party's own frame. Its band is `with_short_spears`, which differs from the
## speared band of gate-a in NOTHING a condition readout can see — every item is live and at the same
## wear — so the only thing that can move this line is the crew division itself.
func _combat_gate_split_state() -> void:
	var mammoth := _combat_gate_mammoth()
	var split := BandFx.with_short_spears(BandFx.hunt_preview_local_band())
	h._hud._band_labor._player_bands = [split]
	h._hud._band_labor._player_band = split
	h._hud._compose.reset_hunt_source()
	h._show_herd(mammoth)
	h._compose_herd(mammoth, LOCAL_HUNT_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_gate_split")
	var sheet: Control = h._hud._drawercompose._compose_sheet
	# **THE GATE AND THE SPLIT ARE COMPLEMENTS, so both halves are asserted.** A refusal here would
	# mean the sheet had decided nobody can take it, which is the OTHER sentence; the split line only
	# ever stands where the fight is winnable by somebody.
	h._assert_hud("a split party clears the gate — the refusal does not render",
		Readout.hunt_gate_blocked(sheet) == Readout.HUNT_GATE_ABSENT)
	h._assert_hud("…and the sheet says WHICH of them can take it: %s" % GATE_SPLIT_LINE,
		Readout.hunt_crew_split_line(sheet) == GATE_SPLIT_LINE)
	# **AND THE KIT LINE NO LONGER CLAIMS AN ATTACK THE PARTY DOES NOT HAVE.** The two lines answer
	# different questions off different wire terms — the split says who can beat THIS quarry's
	# defence (`huntCrews` against `defense`), the kit line says how far the gear reaches into the
	# party at all — so a band whose spears simply ran short would state the second and not the first.
	h._assert_hud("…and the Kit line states the coverage beside the tiers: \"%s\""
		% Readout.kit_hint_line(sheet), Readout.kit_hint_line(sheet) == GATE_SPLIT_KIT_HINT)
	# **THE SAME BAND AND THE SAME QUARRY, A SMALLER PARTY — AND NOW THERE IS NOTHING TO SAY.** The
	# gear covers a prefix of whoever is sent, so three hunters drawn from four spears are all armed;
	# a line here would be the band's shortfall reported as this party's, which reads as "2 of my 3
	# are bare-handed" over a stepper the player just set. PNG-less: the claim is an absence, and the
	# frame above is the picture.
	h._hud._compose.reset_hunt_source()
	h._show_herd(mammoth)
	h._compose_herd(mammoth, GATE_SPLIT_COVERED_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	h._assert_hud("…and a party that fits inside the armed run states NO split",
		Readout.hunt_crew_split_line(h._hud._drawercompose._compose_sheet) == "")
	# **THE KIT LINE STILL SPEAKS, and it says everybody.** The split line above goes quiet because
	# there is no division in THIS party to report; the coverage clause is not a warning and stays,
	# which is what gives the player a `3 of 3` to watch turn into `3 of 4` on the very next step.
	h._assert_hud("…while the Kit line states FULL coverage rather than falling silent: \"%s\""
		% Readout.kit_hint_line(h._hud._drawercompose._compose_sheet),
		Readout.kit_hint_line(h._hud._drawercompose._compose_sheet) == GATE_SPLIT_COVERED_KIT_HINT)


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

	# ---- …AND IT OPENS ON THE BAND ALREADY WORKING THE SOURCE, AND RE-SEEDS ON A SWITCH -----------
	# The block's rationale, its fixture and its four claims are documented beside `_actor_band_roster`
	# at the foot of this chapter. The panel band stays CLEARED and no unit is selected, so
	# `_resolve_assign_band` answers roster[0] — a band that works neither source — and the working-band
	# rung is the only thing that can move the answer.
	var actor_roster := _actor_band_roster()
	h._hud._band_labor.set_panel_band({})
	h._hud._band_labor._player_bands = actor_roster
	h._hud._band_labor._player_band = actor_roster[0]
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(ComposeState.NO_BAND_ENTITY)
	var actor_tile := BaseFx.food_tile_fixture()
	h._show_tile(actor_tile)
	h._compose_forage(actor_tile)
	await h._settle()
	await h._save("compose_working_band_forage")
	_assert_actor_band("compose_working_band_forage", ACTOR_FIRST_WORKER_INDEX,
		ACTOR_FIRST_WORKER_CREW, ACTOR_FORAGE_VERB, ACTOR_FORAGE_RUNG)

	# Drive the picker to the OTHER working band, whose standing crew differs — the composed count,
	# the verb and the improvement control must all re-read from it. With the bare band write the
	# stepper stays on the band being left.
	await _pick_actor_band(ACTOR_SECOND_WORKER_INDEX - 1)
	await h._settle()
	await h._save("compose_band_switch_forage")
	_assert_actor_band("compose_band_switch_forage", ACTOR_SECOND_WORKER_INDEX,
		ACTOR_SECOND_WORKER_CREW, ACTOR_FORAGE_VERB, ACTOR_FORAGE_RUNG)

	# The vacuity guard, then the played defect itself: dial this band's crew to 0 (a real unassign,
	# which the sheet must SAY), then switch back to the first worker. Without the re-seed the composed
	# 0 survives the switch and one press would strip that band's two foragers off the tile.
	h._hud._compose.set_forage_count(0)
	h._compose_forage(actor_tile)
	await h._settle()
	_assert_actor_unassign("compose_band_switch_forage", ACTOR_FORAGE_RUNG)
	await _pick_actor_band(ACTOR_FIRST_WORKER_INDEX - 1)
	await h._settle()
	_assert_actor_band("compose_band_switch_forage (back)", ACTOR_FIRST_WORKER_INDEX,
		ACTOR_FIRST_WORKER_CREW, ACTOR_FORAGE_VERB, ACTOR_FORAGE_RUNG)

	# ---- THE HUNT TWINS — a SECOND compose builder, so the plant half says nothing about it -------
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(ComposeState.NO_BAND_ENTITY)
	var actor_herd := HerdFx.herd_fixture()
	h._show_herd(actor_herd)
	h._compose_herd(actor_herd)
	await h._settle()
	await h._save("compose_working_band_hunt")
	_assert_actor_band("compose_working_band_hunt", ACTOR_FIRST_WORKER_INDEX,
		ACTOR_FIRST_WORKER_CREW, ACTOR_HUNT_VERB, ACTOR_HUNT_RUNG)

	await _pick_actor_band(ACTOR_SECOND_WORKER_INDEX - 1)
	await h._settle()
	await h._save("compose_band_switch_hunt")
	_assert_actor_band("compose_band_switch_hunt", ACTOR_SECOND_WORKER_INDEX,
		ACTOR_SECOND_WORKER_CREW, ACTOR_HUNT_VERB, ACTOR_HUNT_RUNG)

	h._hud._compose.set_hunt_count(0)
	h._compose_herd(actor_herd)
	await h._settle()
	_assert_actor_unassign("compose_band_switch_hunt", ACTOR_HUNT_RUNG)
	await _pick_actor_band(ACTOR_FIRST_WORKER_INDEX - 1)
	await h._settle()
	_assert_actor_band("compose_band_switch_hunt (back)", ACTOR_FIRST_WORKER_INDEX,
		ACTOR_FIRST_WORKER_CREW, ACTOR_HUNT_VERB, ACTOR_HUNT_RUNG)

	# ---- …AND ONE FRAME OF THE ⚠, SO THE TWO HUD SURFACES CAN BE READ TOGETHER --------------------
	await _overdraw_agreement_state()

	# Reset the roster, the panel band and BOTH compose spines for whatever renders after this chapter.
	h._hud._band_labor.set_panel_band({})
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(ComposeState.NO_BAND_ENTITY)
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(ComposeState.NO_BAND_ENTITY)
	h._hud.clear_selection()

## **THE PICTURE THE DRIVEN BLOCK CANNOT TAKE** — the herd drawer's standing summary and the compose
## sheet floating beside it, on ONE source, both flying the ⚠ the sim answered. Appended LAST in this
## chapter so no earlier frame moves, and it hands the roster back the way the block above it does.
##
## **It is EVIDENCE, not the claim.** `_overdraw_is_the_wires_answer` is where the agreement is
## asserted, because the third surface — the map's on-tile label — is painted into MapView's canvas
## and no assertion can read a glyph back off one. What this frame adds is that a reader can see the
## two HUD surfaces saying it, which a list of `PASS` lines cannot show.
func _overdraw_agreement_state() -> void:
	var band := _delivered_oracle_band()
	band["labor_assignments"] = [_overdraw_row(true)]
	var herd := ForageFx.floorify(_delivered_oracle_herd())
	herd["id"] = OVERDRAW_HERD_ID
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(ComposeState.NO_BAND_ENTITY)
	h._show_herd(herd)
	h._compose_herd(herd, OVERDRAW_ROW_CREW, SourceForecast.FLOOR_MIN)
	await h._settle()
	await h._save("herd_overdraw_agrees")
	# The two surfaces, read back off the RENDER rather than off the model — the drawer's standing
	# summary is the tile card's tooltip producer, and the sheet's is the compose model.
	h._assert_hud("the drawer's standing summary flies the ⚠ on this source",
		Q.has_label_containing(h._hud.herd_assign_controls, HudComposeVocab.OVERHUNT_FLAG))
	h._assert_hud("…and so does the sheet beside it, on the same source",
		Readout.yields_text(h._hud._drawercompose._compose_sheet)
			.to_upper().contains(HudComposeVocab.LOCAL_HUNT_OVERDRAW_NOTE.to_upper()))


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
# joined the same `min` that premise was false: a crew that hauls twenty boar can still bring down
# less than ONE, so the branch quoted twenty boar for a take of a fraction of one. (The report names
# crews of six and seven because the reach was FLOORED then; it is a plain `workers × engage_rate`
# now, so the same regime sits at a smaller crew and the constants below say so.)
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

## A crew that brings down LESS THAN ONE BODY a turn, and its exact double — the pair that says a
## reach is a rate. `4 × 0.33` = 1.32 reached, of which 0.99 stays: under one body, while the same
## four hunters could HAUL thirteen. `8 × 0.33` reaches exactly twice that, because an unrounded reach
## is linear in the crew where the retired `floor(w × 0.33).max(1)` was a staircase.
const BOAR_CREW_SUB_ONE_ANIMAL := 4

const BOAR_CREW_DOUBLE := BOAR_CREW_SUB_ONE_ANIMAL * 2

## The sweep, chosen to CROSS the sub-one-animal region rather than sit above it: one hunter reaches
## 0.33 of a boar and keeps a quarter of one, and the crew is still the binding arm at twelve (2.97
## brought down, against 40 haulable and 11.67 affordable). A sweep starting above the crossing would
## pass with the defect fully restored.
const BOAR_SWEEP_MIN_CREW := 1

const BOAR_SWEEP_MAX_CREW := 12

## ---- WHAT THE SAME BOAR PAYS BESIDE THE MEAT ---------------------------------------------------
## The reported defect's OTHER half. One herder against five, changing nothing else, read
## `FOOD 0.18 · BONE 0.08 · HIDE 0.56` and `FOOD 0.18 · BONE 0.40 · HIDE 2.80`: the food was the
## model working as it was written then — the reach was FLOORED, `floor(workers × 0.33)` never below
## one, pinning every crew from 1 to 6 at exactly ONE animal reached — and the materials were a pure
## crew-throughput line
## (`min(workers × per_worker_material, escapement ceiling)`) that had met neither the
## engagement→retreat arm nor the whole-animal quantiser, promising ~5× the truth at five herders.
##
## **THE PER-WORKER TERMS ARE COMPOSED, NEVER RESTATED.** One hunter moves
## `per_worker_yield ÷ provisions_per_biomass` = 40 biomass a turn, so what they bring home in a
## material is that throughput times the material's own per-biomass rate. A fixture that typed both
## halves could describe a boar whose hide and whose mass disagree — which is exactly the
## disagreement this readout exists to make visible.
const BOAR_PER_WORKER_BIOMASS := BOAR_PER_WORKER_YIELD / BOAR_PROVISIONS_PER_BIOMASS

## TWO materials, not one, at deliberately unequal rates: a one-material fixture passes just as well
## against a producer that summed the vector into a single materials/turn figure — the retired trade
## axis under a new name. Both are real `materials.json` ids, and the catalogue ships no display
## name, so the id IS the display word.
const BOAR_BONE_ID := "bone"

const BOAR_HIDE_ID := "hide"

const BOAR_BONE_PER_BIOMASS := 0.01

const BOAR_HIDE_PER_BIOMASS := 0.05

## The reported pair. Both crews reach ONE animal (`max(floor(1 × 0.33), 1)` and `floor(5 × 0.33)`),
## so every account they bring home must read the SAME — while the retired crew line moved 5×.
const BOAR_MATERIAL_LEAN_CREW := 1

const BOAR_MATERIAL_FULL_CREW := 5

## …and the full crew DOUBLED, which is what the coupling claim is made across: an unrounded reach is
## linear in the crew, so twice the hands bring down exactly twice as much and every account that is a
## conversion of the delivery must double with it. The reach is still the binding arm here (2.48
## brought down, against 33 haulable and 11.67 affordable).
const BOAR_MATERIAL_DOUBLE_CREW := BOAR_MATERIAL_FULL_CREW * 2

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
		# **THE CARRY IN BIOMASS, which is the unit the quantiser divides by.** Stated rather than left
		# to `ForageFx.seed_growth_terms` because two of the three blocks that read this herd do NOT
		# floorify it, and a fixture whose take depends on which caller reached it first is a fixture
		# that can disagree with itself. It is the same identity the seeder would apply.
		"per_worker_biomass": BOAR_PER_WORKER_BIOMASS,
		# **WHAT ONE UNIT OF THIS BOAR IS MADE OF, and what one hunter therefore hauls of it.** The
		# per-worker vector is the throughput times the per-biomass one, so the two halves of the
		# pair cannot describe different animals.
		"material_per_biomass": [
			{"material_id": BOAR_BONE_ID, "amount": BOAR_BONE_PER_BIOMASS},
			{"material_id": BOAR_HIDE_ID, "amount": BOAR_HIDE_PER_BIOMASS},
		],
		"per_worker_material": [
			{"material_id": BOAR_BONE_ID,
				"amount": BOAR_PER_WORKER_BIOMASS * BOAR_BONE_PER_BIOMASS},
			{"material_id": BOAR_HIDE_ID,
				"amount": BOAR_PER_WORKER_BIOMASS * BOAR_HIDE_PER_BIOMASS},
		],
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

## …and the same carry in BIOMASS, the unit the quantiser divides by. Composed from the pair above
## rather than written down, so a re-dial of either cannot leave the two describing different hunters.
const CADENCE_PER_WORKER_BIOMASS := CADENCE_PER_WORKER_YIELD / CADENCE_PROVISIONS_PER_BIOMASS

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
		# The quantiser's carry, in its own unit — the boar fixture's note, for the same reason.
		"per_worker_biomass": CADENCE_PER_WORKER_BIOMASS,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}

## **THE SIM’S ANSWER, AS THIS HARNESS STANDS IN FOR IT** — the crew-take curve
## (`ForecastQuery.KIND_HUNT_CREW_TAKE`) the sheet now looks its third bound up in, composed by the
## same canned answerer the live sheet is served by (`fixtures_forecast.gd`). The producers below are
## called DIRECTLY, outside any sheet, so nothing has asked the seam for them and the curve is handed
## in the way the builder hands it in.
##
## It is generous by a wide margin rather than sized to each claim: a row past the crew asked about is
## never read, and a curve that stopped one short of a sweep would fail as “no answer” rather than as
## the magnitude the sweep is about.
const CREW_CURVE_MAX_WORKERS := 24

## `floor` is the floor the PRODUCER under test is being called at, and it is not optional in spirit:
## the stand-in clamps its rows by the room at that floor exactly as the sim does, so a curve composed
## at one floor and spent against another states a reach the herd cannot afford.
func _crew_curve(herd: Dictionary, floor: float = SourceForecast.FLOOR_FOOD_PEAK) -> Array:
	return ForecastFx.crew_take_rows(herd, CREW_CURVE_MAX_WORKERS, floor)

## What the party brings down at `workers`, composed the way the sim composes it (engage → retreat)
## rather than read back off the producer under test.
func _boar_brought_down(workers: int) -> float:
	return SourceForecast.animals_stayed(
		SourceForecast.animals_engaged(workers, BOAR_ENGAGE_RATE),
		BOAR_STAY_FRACTION)

## The producer's delivered take for one crew, at the food peak with no build in flight — **read in
## BIOMASS and stated in FOOD**, which is the unit every claim below is written in. The quantiser
## answers in the unit a take is actually taken in; the species' own per-biomass rate is what values
## it, and it is one published number rather than a second derivation.
func _boar_delivered(band: Dictionary, herd: Dictionary, workers: int) -> float:
	var take: Dictionary = h._hud._drawercompose._hunt_delivered_and_waste(
		band, herd, SourceForecast.FLOOR_FOOD_PEAK, workers, SourceForecast.IMPROVEMENT_NONE,
		_crew_curve(herd))
	if not bool(take.get("available", false)):
		return -1.0
	return float(take["delivered_biomass"]) * BOAR_PROVISIONS_PER_BIOMASS

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

	# (1) THE REPORTED PAIR. The sub-one-animal crew brings down 0.99 of a boar, so it lands
	#     0.99 × 0.24 food — NOT the crew's whole carry throughput, which is what the retired
	#     `carryable < 1` branch quoted: thirteen boar, for one.
	var six := _boar_delivered(band, herd, BOAR_CREW_SUB_ONE_ANIMAL)
	var want_six := stayed_six * fpa
	var carry_six := float(BOAR_CREW_SUB_ONE_ANIMAL) * BOAR_PER_WORKER_YIELD
	h._assert_hud(("%d hunters land what they bring DOWN (%.2f food/turn), not what they could carry"
			+ " (%.2f) — got %.2f") % [BOAR_CREW_SUB_ONE_ANIMAL, want_six, carry_six, six],
		is_equal_approx(six, want_six))
	# Twice the crew brings down exactly twice as much, the reach being a RATE — where the retired
	# staircase answered one animal for every crew from one to six and then jumped.
	var seven := _boar_delivered(band, herd, BOAR_CREW_DOUBLE)
	var want_seven := _boar_brought_down(BOAR_CREW_DOUBLE) * fpa
	h._assert_hud("…and %d hunters land %.2f food/turn — got %.2f"
			% [BOAR_CREW_DOUBLE, want_seven, seven],
		is_equal_approx(seven, want_seven) and is_equal_approx(seven, six * 2.0))

	# (1a) THE REACH IS THE SIM'S `fauna::animals_engaged`, UNROUNDED — `workers × engage_rate`,
	#      asserted as the NUMBER at a crew whose product is FRACTIONAL. This is the mirror the sim
	#      un-floored and the client did not: on the shipped boar the retired `floor(w × 0.33).max(1)`
	#      bounded a lone hunter's row at ONE animal reached while the turn brought down 0.33, a ~3×
	#      over-quote that held for every crew under `1 / engage_rate`. A presence check passes on
	#      either expression, so the claim is the value.
	var lone_reach := SourceForecast.animals_engaged(1, BOAR_ENGAGE_RATE)
	h._assert_hud(("one hunter reaches %.2f boar — the product, not the retired floor-of-one's 1.00"
			+ " — got %.2f") % [BOAR_ENGAGE_RATE, lone_reach],
		is_equal_approx(lone_reach, BOAR_ENGAGE_RATE) and lone_reach < 1.0)
	# (1b) AND IT RISES WITH EVERY HAND, at the product, across the whole sweep. Stated as one claim
	#      over the sweep rather than as twelve literals, so a re-dialed fixture moves the numbers and
	#      not the claim; the first crew that breaks either half is the one reported.
	var reach_broke_at := 0
	var previous_reach := 0.0
	for workers in range(BOAR_SWEEP_MIN_CREW, BOAR_SWEEP_MAX_CREW + 1):
		var reach := SourceForecast.animals_engaged(workers, BOAR_ENGAGE_RATE)
		if reach_broke_at == 0 and (reach <= previous_reach
				or not is_equal_approx(reach, float(workers) * BOAR_ENGAGE_RATE)):
			reach_broke_at = workers
		previous_reach = reach
	h._assert_hud(("the reach is `workers × %.2f` and rises with every hand (%d..%d) — broke at %d")
			% [BOAR_ENGAGE_RATE, BOAR_SWEEP_MIN_CREW, BOAR_SWEEP_MAX_CREW, reach_broke_at],
		reach_broke_at == 0)

	# (2) MONOTONICITY — the PROPERTY the defect violated, and the one that catches its return in any
	#     other species' numbers. Every arm of the `min` is non-decreasing in the crew, so the take
	#     must be too; the played pair was 4.80 → 0.36, an order of magnitude LOST to one more hunter.
	#     **STRICTLY rising across this sweep**, since the reach is the binding arm at every crew in it
	#     and an unrounded reach has no treads — a non-decreasing claim would pass on the staircase the
	#     un-flooring removed.
	var previous := -1.0
	var broke_at := 0
	var broke_from := 0.0
	var broke_to := 0.0
	for workers in range(BOAR_SWEEP_MIN_CREW, BOAR_SWEEP_MAX_CREW + 1):
		var delivered := _boar_delivered(band, herd, workers)
		if broke_at == 0 and previous >= 0.0 and delivered <= previous:
			broke_at = workers
			broke_from = previous
			broke_to = delivered
		previous = delivered
	h._assert_hud(("the delivered take RISES with every hand (%d..%d hunters)"
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
		SourceForecast.IMPROVEMENT_NONE, _crew_curve(cadence))
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
	# The producer answers in BIOMASS — see `_boar_delivered` — so the claim is valued through the
	# species' own per-biomass rate rather than restated in a second unit.
	var cadence_delivered := float(cadence_take["delivered_biomass"]) \
		* CADENCE_PROVISIONS_PER_BIOMASS
	h._assert_hud("a crew that cannot carry a whole body lands %.4f food/turn, not the room's %.4f — got %.4f"
			% [want_cadence, cadence_ceiling, cadence_delivered],
		is_equal_approx(cadence_delivered, want_cadence))
	h._assert_hud("…and the body it cannot finish carrying is WASTE — %d%%, got %d%%"
			% [int(round(CADENCE_WASTE_FRACTION * 100.0)),
				int(round(float(cadence_take["waste_pct"]) * 100.0))],
		is_equal_approx(float(cadence_take["waste_pct"]), CADENCE_WASTE_FRACTION))


# =====================================================================================
#  THE MATERIAL ROWS ARE ROWS OF THE SAME DELIVERY
# =====================================================================================
# Reported from play on the SAME Wild Boar herd, one herder against five with nothing else moved:
#
#   herders │ FOOD │ BONE │ HIDE
#         1 │ 0.18 │ 0.08 │ 0.56
#         5 │ 0.18 │ 0.40 │ 2.80
#
# **The food was the model working as it was written then.** The reach was FLOORED —
# `floor(workers × 0.33)`, never below one — so every crew from one to six reached exactly ONE animal
# and one boar came home at both sizes. (The floor is retired; the reach is `workers × engage_rate`
# and rises with every hand, so the two crews below differ in the delivery and the claims are about
# what the accounts are a CONVERSION of.) **The materials were a second expression** —
# `min(workers × per_worker_material, escapement ceiling)`, a pure crew-throughput line that had met
# neither the engagement→retreat arm nor the whole-animal quantiser — so the sheet promised roughly
# 5× the truth at five herders while quoting the honest meat one line above it.
#
# The sim banks both accounts off ONE quantity (`systems/labor.rs`: `hunt_yield.apply(take.carried,
# …)` beside `credit_material_yield(…, take.carried, …)`), so quote and payout provably disagreed.
# The material rows are composed off the delivered biomass now — `SourceForecast.rescaled_accounts`,
# the same crossing the fodder account already took — and the drift is unrepresentable rather than
# merely unlikely.
#
# PNG-LESS AND DRIVEN, for the block above's reason: these are numbers, and a sheet quoting the wrong
# ones renders a perfectly plausible readout.

## The one FORAGE control this block needs — two foragers on the cash-crop patch, whose materials must
## go on scaling LINEARLY with the crew. The plant web has no engagement stage and no whole-animal
## quantum, so its food row and its material rows are both `min(workers × rate, ceiling)` against
## matching ceilings and already track; the animal web's food row has three more arms, which is the
## whole of why the shared expression was right there and wrong here.
const FORAGE_LINEARITY_LEAN_CREW := 1

const FORAGE_LINEARITY_FULL_CREW := 2

## A yield model's rows, read back as `account -> per-turn value`. `yield_rows` keys a material row by
## the material's own id, so one reader answers for all three accounts.
func _model_accounts(model: Dictionary) -> Dictionary:
	var out := {}
	for row_variant in model.get(h._hud._drawercompose.YIELD_MODEL_ROWS, []):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		out[String(row[SourceForecast.YIELD_ROW_ACCOUNT])] = float(row[SourceForecast.YIELD_ROW_VALUE])
	return out

## A per-material vector, read back the same way — for the RETIRED expression, which is still the
## plant web's and is what the vacuity guard below quotes.
func _material_amounts(rows: Array) -> Dictionary:
	var out := {}
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		out[String(row[SourceForecast.MATERIAL_PAYOFF_ID_KEY])] = \
			float(row[SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY])
	return out

## Every account this crew brings home off the herd, through the REAL producer — so the claim is about
## the sheet's own wiring rather than about a helper called in isolation.
func _boar_accounts(band: Dictionary, herd: Dictionary, workers: int) -> Dictionary:
	return _model_accounts(h._hud._drawercompose._hunt_yield_model(
		band, herd, SourceForecast.FLOOR_FOOD_PEAK, workers, SourceForecast.IMPROVEMENT_NONE, false,
		_crew_curve(herd)))

## …and its plant twin.
func _forage_accounts(band: Dictionary, tile: Dictionary, workers: int) -> Dictionary:
	return _model_accounts(h._hud._drawercompose._forage_yield_model(
		band, tile, SourceForecast.FLOOR_FOOD_PEAK, workers))

## **THE ONE FRAME OF THE REPORTED SHEET** — the engagement-bound Wild Boar at the crew the report
## names, so the three accounts can be read beside each other rather than only asserted apart. It is
## the corrected reading: `0.18 FOOD · 0.09 BONE · 0.45 HIDE` at five herders, the same three numbers
## one herder brings home, because both crews reach the same single animal.
##
## **A FRAME CANNOT CARRY THE CLAIM** — a sheet quoting 5× the hide renders a perfectly plausible
## readout, which is why the block above is PNG-less and driven — but it can carry the SHAPE, and a
## reader who has never seen the three accounts on one hunt sheet has no picture to check against.
func _material_take_state() -> void:
	var band := _delivered_oracle_band()
	h._hud._band_labor._player_bands = [band]
	h._hud._band_labor._player_band = band
	var herd := _quantisation_boar_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(herd)
	h._compose_herd(herd, BOAR_MATERIAL_FULL_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_hunt_material_take")
	h._assert_compose_sheet_fits("herd_hunt_material_take")

func _material_take_tracks_delivery_assertions() -> void:
	var band := _delivered_oracle_band()   # output_multiplier 1.0, so no morale factor muddies it
	var herd := ForageFx.floorify(_quantisation_boar_herd())
	var fpa := BOAR_BODY_MASS * BOAR_PROVISIONS_PER_BIOMASS
	var accounts: Array[String] = [SourceForecast.YIELD_ACCOUNT_FOOD, BOAR_BONE_ID, BOAR_HIDE_ID]
	# The producer prices its own herd at the composed kit, and every claim below is a RELATION
	# between rendered numbers — but a repricing that moved `per_worker_yield` under the fixture's
	# `food_per_animal` would put the lean crew's carry below one body and break the equality for a
	# reason that has nothing to do with materials. So the no-op is asserted rather than assumed.
	h._hud._compose.reset_hunt_source()
	var priced: Dictionary = h._hud._drawercompose._hunt_priced_herd(herd, band)
	h._assert_hud("the kit prices this fixture as-is, so the crews below are the fixture's own",
		is_equal_approx(float(priced["per_worker_yield"]), BOAR_PER_WORKER_YIELD)
			and is_equal_approx(float(priced["stay_fraction"]), BOAR_STAY_FRACTION))

	# (0) THE PRECONDITION THE WHOLE PAIR RESTS ON — the ENGAGEMENT arm is what binds at BOTH crews,
	#     and both therefore reach the SAME single animal. Without it the equality below is satisfied
	#     by every account collapsing to zero, or by a herd whose arms happen to coincide.
	var room_bodies := (BOAR_BIOMASS - SourceForecast.FLOOR_FOOD_PEAK * BOAR_CAPACITY) \
		/ BOAR_BODY_MASS
	for workers in [BOAR_MATERIAL_LEAN_CREW, BOAR_MATERIAL_FULL_CREW]:
		var stayed := _boar_brought_down(workers)
		var haulable := maxf(floorf(float(workers) * BOAR_PER_WORKER_YIELD / fpa), 1.0)
		h._assert_hud(("the engagement arm is what binds at %d herders — %.2f brought down against"
				+ " %d haulable and %.2f affordable") % [workers, stayed, int(haulable), room_bodies],
			stayed < haulable and stayed < room_bodies)
	h._assert_hud(("…and the full crew brings down %d× what the lean one does, the reach being a rate")
			% (BOAR_MATERIAL_FULL_CREW / BOAR_MATERIAL_LEAN_CREW),
		is_equal_approx(_boar_brought_down(BOAR_MATERIAL_FULL_CREW),
			_boar_brought_down(BOAR_MATERIAL_LEAN_CREW)
				* float(BOAR_MATERIAL_FULL_CREW) / float(BOAR_MATERIAL_LEAN_CREW)))

	# (0b) VACUITY GUARD — the RETIRED expression really did move between these two crews, so the
	#      equality below is a claim about the fix rather than about a fixture that cannot tell them
	#      apart. It is the real `expected_materials`, still the plant web's, asked of this herd.
	var boar_forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	var lean_crew_line := _material_amounts(SourceForecast.expected_materials(
		float(BOAR_MATERIAL_LEAN_CREW), boar_forecast,
		SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY))
	var full_crew_line := _material_amounts(SourceForecast.expected_materials(
		float(BOAR_MATERIAL_FULL_CREW), boar_forecast,
		SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY))
	h._assert_hud(("the retired crew-throughput line DID move between these crews (%.2f → %.2f hide),"
			+ " so this fixture can tell the two expressions apart")
			% [float(lean_crew_line[BOAR_HIDE_ID]), float(full_crew_line[BOAR_HIDE_ID])],
		not is_equal_approx(float(lean_crew_line[BOAR_HIDE_ID]),
			float(full_crew_line[BOAR_HIDE_ID])))

	# (1) EVERY ACCOUNT IS A CONVERSION OF THE ONE DELIVERED BIOMASS — its own published per-biomass
	#     rate times what the crew actually lands, which is what "rows of the same delivery" means and
	#     what the retired crew-throughput line was not. **The claim is the MAGNITUDE**: with the reach
	#     unrounded both expressions are linear in the crew, so a shape claim can no longer tell them
	#     apart — the retired line quotes an order of magnitude more hide, and only a number says so.
	var lean := _boar_accounts(band, herd, BOAR_MATERIAL_LEAN_CREW)
	var full := _boar_accounts(band, herd, BOAR_MATERIAL_FULL_CREW)
	var rates := {
		SourceForecast.YIELD_ACCOUNT_FOOD: BOAR_PROVISIONS_PER_BIOMASS,
		BOAR_BONE_ID: BOAR_BONE_PER_BIOMASS,
		BOAR_HIDE_ID: BOAR_HIDE_PER_BIOMASS,
	}
	for workers in [BOAR_MATERIAL_LEAN_CREW, BOAR_MATERIAL_FULL_CREW]:
		var landed := _boar_delivered_biomass(band, herd, workers)
		var quoted := _boar_accounts(band, herd, workers)
		for account in accounts:
			var want := landed * float(rates[account])
			h._assert_hud("%s is a live reading at %d herders (got %.4f) — not a collapse to zero"
					% [account, workers, float(quoted.get(account, 0.0))],
				float(quoted.get(account, 0.0)) > 0.0)
			h._assert_hud("%s at %d herders is %.4f — the %.4f biomass landed at %.2f/biomass"
					% [account, workers, want, landed, float(rates[account])],
				is_equal_approx(float(quoted.get(account, 0.0)), want))
	h._assert_hud(("…and the hide is NOT the retired throughput line's %.4f at %d herders — got %.4f")
			% [float(full_crew_line[BOAR_HIDE_ID]), BOAR_MATERIAL_FULL_CREW,
				float(full.get(BOAR_HIDE_ID, 0.0))],
		not is_equal_approx(float(full.get(BOAR_HIDE_ID, 0.0)),
			float(full_crew_line[BOAR_HIDE_ID])))

	# (2) AND ALL THREE MOVE TOGETHER WITH THE DELIVERY. This is the half that proves they are COUPLED
	#     rather than three separately-composed lines that happen to agree at one crew: twice the crew
	#     brings down twice as much — the reach is a rate — so every account doubles with it.
	var stepped := _boar_accounts(band, herd, BOAR_MATERIAL_DOUBLE_CREW)
	var step := _boar_brought_down(BOAR_MATERIAL_DOUBLE_CREW) \
		/ _boar_brought_down(BOAR_MATERIAL_FULL_CREW)
	h._assert_hud("%d herders really bring down twice what %d do (×%.2f)"
			% [BOAR_MATERIAL_DOUBLE_CREW, BOAR_MATERIAL_FULL_CREW, step],
		is_equal_approx(step, 2.0))
	for account in accounts:
		h._assert_hud("%s doubles with the animal count — %.4f against %.4f"
				% [account, float(stepped.get(account, 0.0)), float(full.get(account, 0.0)) * step],
			is_equal_approx(float(stepped.get(account, 0.0)), float(full.get(account, 0.0)) * step))

	# (3) THE PLANT WEB MUST NOT HAVE MOVED, and the claim is made in two ways because either alone
	#     is weak. Its materials are still `expected_materials`' own answer to the digit — the shared
	#     clamp is right there, where the food row is the same linear `min` against a matching
	#     ceiling — and they still scale LINEARLY with the crew, which is the property the animal
	#     web's engagement bound is what breaks.
	var tile := ForageFx.floorify(ForageFx.cash_crop_gather_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var patch_materials: Array[String] = [ForageFx.CASH_PATCH_FIBRE_ID,
		ForageFx.CASH_PATCH_TOBACCO_ID]
	var patch_forecast := SourceForecast.forecast_inputs(tile, SourceForecast.SOURCE_KIND_FORAGE,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	var lean_patch := _forage_accounts(band, tile, FORAGE_LINEARITY_LEAN_CREW)
	var full_patch := _forage_accounts(band, tile, FORAGE_LINEARITY_FULL_CREW)
	var lean_clamp := _material_amounts(SourceForecast.expected_materials(
		float(FORAGE_LINEARITY_LEAN_CREW), patch_forecast,
		SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY))
	for material in patch_materials:
		h._assert_hud("the patch's %s is a live reading (got %.4f)"
				% [material, float(lean_patch.get(material, 0.0))],
			float(lean_patch.get(material, 0.0)) > 0.0)
		h._assert_hud("…and it is still the crew-throughput clamp to the digit — %.4f against %.4f"
				% [float(lean_patch.get(material, 0.0)), float(lean_clamp.get(material, 0.0))],
			is_equal_approx(float(lean_patch.get(material, 0.0)),
				float(lean_clamp.get(material, 0.0))))
		h._assert_hud("…and it still scales LINEARLY with the crew — %.4f against %.4f at %d foragers"
				% [float(full_patch.get(material, 0.0)),
					float(lean_patch.get(material, 0.0)) * 2.0, FORAGE_LINEARITY_FULL_CREW],
			is_equal_approx(float(full_patch.get(material, 0.0)),
				float(lean_patch.get(material, 0.0)) * 2.0))


# =====================================================================================
#  AN INEDIBLE QUARRY'S MATERIALS ARE ROWS OF THE SAME DELIVERY
# =====================================================================================
# The half of the arc above the boar pair could not reach. A wolf's provisions rate is a structural
# `0`, so the FOOD-keyed quantiser divided by nothing, both food paths bailed, and its material rows
# fell through to `min(workers × per_worker_material, ceiling)` — a pure crew-throughput line carrying
# ONE of the take's four bounds. It was therefore over-quoted at every crew the reach arm pinned, on
# the one quarry whose materials are the entire point of hunting it.
#
# The quantiser is stated in BIOMASS now (`min(room, carry, what stays) ÷ one body`, every term a
# biomass), so a pack that publishes a body is priced by exactly the `min` a deer is.
#
# PNG-LESS AND DRIVEN, for the boar block's reason: a sheet quoting seven times the hide renders a
# perfectly plausible readout, and only a relation between two crews' numbers can say otherwise.
func _wolf_material_take_assertions() -> void:
	var band := _delivered_oracle_band()   # output_multiplier 1.0, so no morale factor muddies it
	var wolf := ForageFx.floorify(_pelt_only_wolf_herd())
	# (0) THE PRECONDITION — the REACH is what binds at both crews, and both therefore reach the SAME
	#     single animal. Without it the equality below is satisfied by a pack whose arms coincide.
	var haulable := maxf(floorf(float(WOLF_REACH_BOUND_CREW) * WOLF_CARRY / WOLF_BODY_MASS), 1.0)
	var affordable := WOLF_ROOM_AT_PEAK / WOLF_BODY_MASS
	var reach := SourceForecast.animals_engaged(WOLF_REACH_BOUND_CREW, WOLF_ENGAGE_RATE)
	h._assert_hud(("the reach arm binds at %d hunters — %.2f brought down against %d haulable and"
			+ " %.1f affordable") % [WOLF_REACH_BOUND_CREW, reach, int(haulable), affordable],
		reach < haulable and reach < affordable)
	# (0b) VACUITY GUARD — the RETIRED expression really did move across this crew range, so the
	#      equality below is a claim about the fix rather than about a pack that cannot tell the two
	#      expressions apart. It is the real `expected_materials`, still the plant web's.
	var wolf_forecast := SourceForecast.forecast_inputs(wolf, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	var retired_one := _material_amounts(SourceForecast.expected_materials(
		1.0, wolf_forecast, SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY))
	var retired_many := _material_amounts(SourceForecast.expected_materials(
		float(WOLF_REACH_BOUND_CREW), wolf_forecast, SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY))
	h._assert_hud(("the retired crew-throughput line DID move across this range (%.2f → %.2f hide),"
			+ " so the pack can tell the two expressions apart")
			% [float(retired_one[WOLF_MATERIAL_ID]), float(retired_many[WOLF_MATERIAL_ID])],
		not is_equal_approx(float(retired_one[WOLF_MATERIAL_ID]),
			float(retired_many[WOLF_MATERIAL_ID])))
	# (1) THE QUOTED HIDE IS THE QUANTISER'S, AT EVERY CREW THE REACH BINDS AT — equal to the harness's
	#     own oracle (a cross-check against the sim's arithmetic rather than a restatement of the
	#     client's) and NOT the retired crew-throughput line beside it. The two are a tenth apart per
	#     hunter now that the reach is unrounded, both of them linear, so only the MAGNITUDE separates
	#     them: a shape claim would pass on either.
	var one_hunter := _wolf_material_take(1)
	h._assert_hud("one hunter's take is a live reading (got %.4f hide) — not a collapse to zero"
		% one_hunter, one_hunter > 0.0)
	var wolf_broke_at := 0
	var wolf_previous := -1.0
	for crew in range(1, WOLF_REACH_BOUND_CREW + 1):
		var quoted := float(_wolf_accounts(band, wolf, crew).get(WOLF_MATERIAL_ID, 0.0))
		var oracle := _wolf_material_take(crew)
		h._assert_hud(("%d hunter(s) bring home the quantiser's %.4f hide, not the throughput line's"
				+ " %.4f — got %.4f")
				% [crew, oracle, float(crew) * WOLF_MATERIAL_PER_WORKER, quoted],
			is_equal_approx(quoted, oracle)
				and not is_equal_approx(quoted, float(crew) * WOLF_MATERIAL_PER_WORKER))
		if wolf_broke_at == 0 and quoted <= wolf_previous:
			wolf_broke_at = crew
		wolf_previous = quoted
	# (2) …AND IT RISES WITH EVERY HAND, which is what proves the row is COUPLED to the delivery rather
	#     than frozen — an equality claim alone passes on a readout that has stopped moving at all. It
	#     is also the property the reach's retired floor destroyed: seven hunters used to bring home
	#     exactly what one did.
	h._assert_hud("the hide rises with every hand from 1 to %d hunters (broke at %d)"
			% [WOLF_REACH_BOUND_CREW, wolf_broke_at], wolf_broke_at == 0)
	h._assert_hud("…and %d hunters bring home %d× what one does, the reach being a rate — %.4f hide"
			% [WOLF_REACH_BOUND_CREW, WOLF_REACH_BOUND_CREW,
				float(_wolf_accounts(band, wolf, WOLF_REACH_BOUND_CREW).get(WOLF_MATERIAL_ID, 0.0))],
		is_equal_approx(float(_wolf_accounts(band, wolf, WOLF_REACH_BOUND_CREW)
			.get(WOLF_MATERIAL_ID, 0.0)), one_hunter * float(WOLF_REACH_BOUND_CREW)))
	# (3) AND IT STILL STATES NO FOOD. The crossing is stated in biomass now, so the plausible wrong
	#     fix is one that credits every account from it — which would put `0.00 FOOD` back on a wolf.
	h._assert_hud("…and the pack still pays into no food account at all",
		not _wolf_accounts(band, wolf, WOLF_REACH_BOUND_CREW).has(SourceForecast.YIELD_ACCOUNT_FOOD))

## What the crew actually LANDS off the boar, in biomass — the quantity every account beside it is a
## fixed conversion of, read from the same producer the accounts are read from rather than recomposed.
func _boar_delivered_biomass(band: Dictionary, herd: Dictionary, workers: int) -> float:
	var take: Dictionary = h._hud._drawercompose._hunt_delivered_and_waste(
		band, herd, SourceForecast.FLOOR_FOOD_PEAK, workers, SourceForecast.IMPROVEMENT_NONE,
		_crew_curve(herd))
	return float(take["delivered_biomass"]) if bool(take.get("available", false)) else -1.0

## Every account this crew brings home off the pack, through the REAL producer — the wolf twin of
## `_boar_accounts`, so the two quarries' claims are made about the same seam.
func _wolf_accounts(band: Dictionary, herd: Dictionary, workers: int) -> Dictionary:
	return _model_accounts(h._hud._drawercompose._hunt_yield_model(
		band, herd, SourceForecast.FLOOR_FOOD_PEAK, workers, SourceForecast.IMPROVEMENT_NONE, false,
		_crew_curve(herd)))

## **AN EDIBLE QUARRY'S TAKE IS UNCHANGED BY THE BIOMASS RE-EXPRESSION, AND NOTHING ELSE IN THE CORPUS
## SAYS SO.** Measured: pointing the quantiser's quantum at a DIFFERENT pairing of the wire's own
## fields moved FORTY-ODD frames and failed not one assertion — every claim on those sheets is a
## relation, a presence or a word, so a take quietly quoting the wrong number renders a perfectly
## plausible readout. This is the magnitude claim that closes it.
##
## **The expectation is the RETIRED expression, in the RETIRED units** — the food-keyed quantiser this
## arc replaced, restated here — because the whole claim is that the two forms are one answer. The
## ceiling it divides is the composed one (`herd_axis_rates`), which is not what is under test; the
## quantum and the crew term are the FIXTURE's own published food pair.
##
## **THE FIXTURE IS THE REFERENCE HERD FOR A REASON**: `ForageFx.floorify` rewrites
## `provisions_per_biomass` from the authored peak ceiling, and this herd also STATES `body_mass`
## outright, so the two pairings genuinely disagree on it — which is the one shape that can tell the
## quantum's two candidate readings apart. That disagreement is asserted first, or the claim below is
## satisfied by any herd whose fixture happens to close.
func _edible_take_is_unchanged_assertions() -> void:
	var band := _delivered_oracle_band()   # output_multiplier 1.0
	var herd := ForageFx.floorify(HerdFx.herd_fixture())
	var fpa := float(herd["food_per_animal"])
	var rate := float(herd["provisions_per_biomass"])
	var per_worker := float(herd["per_worker_yield"])
	var stated_body := float(herd[SourceForecast.FORECAST_BODY_MASS_KEY])
	h._assert_hud(("precondition: this fixture's two pairings disagree — stated body %.1f against the"
			+ " food pair's %.1f") % [stated_body, fpa / rate],
		not is_equal_approx(stated_body, fpa / rate))
	var ceiling := float(SourceForecast.herd_axis_rates(herd,
		SourceForecast.FLOOR_FOOD_PEAK)["next_ceiling"])
	for crew in [EDIBLE_UNCHANGED_LEAN_CREW, EDIBLE_UNCHANGED_FULL_CREW]:
		var collection := float(crew) * per_worker
		# The retired form: `killed = min(room ÷ one body, whole bodies haulable)` — this herd states
		# no engagement stage, so the third arm is unbounded and drops out — then
		# `delivered = killed × min(one body, the crew's carry)`, the pack's hold charged PER BODY.
		var killed := minf(ceiling / fpa, maxf(floorf(collection / fpa), 1.0))
		var want := killed * minf(fpa, collection)
		var got := float(_wolf_accounts(band, herd, crew).get(
			SourceForecast.YIELD_ACCOUNT_FOOD, 0.0))
		h._assert_hud("%d hunter(s) land the food-keyed quantiser's own %.4f — got %.4f"
			% [crew, want, got], is_equal_approx(got, want))

## Two crews on opposite sides of one body of carry, so the claim covers the `min(one body, the crew's
## carry)` clamp in BOTH directions rather than only the arm that happens to bind.
const EDIBLE_UNCHANGED_LEAN_CREW := 1
const EDIBLE_UNCHANGED_FULL_CREW := 8


# =====================================================================================
#  THE ⚠ HAS ONE PRODUCER, AND THREE SURFACES READ IT
# =====================================================================================
# Reported from play as two surfaces disagreeing about ONE source: the tile card's tooltip read
# *"Sustainable +0.63/turn — overdrawing"* while the compose sheet three inches away read *"This crew
# can't draw it that low. It settles at 92%."* The tooltip read the wire; the sheet computed a fourth
# predicate of its own — `actual > sustainable` (the comparison `snapshot.fbs` forbids outright, since
# a first harvest of a stocked source exceeds one turn's regrowth at EVERY floor) gated on a
# client-side reachability walk.
#
# `LaborAssignment.overdraws` carries the whole verdict — intent AND ability — so the claim here is
# that every surface flying the mark reads that one field and nothing else.
#
# PNG-LESS AND DRIVEN: a sheet flying the wrong ⚠ renders a perfectly ordinary readout, and the map
# badge is painted into a canvas no assertion can read a glyph back off.

## The reported shape, stated as a row rather than derived: a crew the sim says IS drawing the herd
## down, whose `actual` sits BELOW its `sustainable` — the first-turn reading the retired comparison
## would have called clean — and, on the other half of the A/B, a kill turn's spike with the sim
## saying nothing is being overdrawn, which is what that comparison cried wolf on.
const OVERDRAW_ROW_ACTUAL_UNDER := 0.63
const OVERDRAW_ROW_SUSTAINABLE := 1.40
const OVERDRAW_ROW_ACTUAL_SPIKE := 4.10
const OVERDRAW_ROW_CREW := 3

func _overdraw_row(overdraws: bool) -> Dictionary:
	return {
		"kind": SourceForecast.LABOR_KIND_HUNT,
		"workers": OVERDRAW_ROW_CREW,
		"fauna_id": OVERDRAW_HERD_ID,
		"floor": SourceForecast.FLOOR_MIN,
		"has_yield": true,
		"actual_yield": OVERDRAW_ROW_ACTUAL_UNDER if overdraws else OVERDRAW_ROW_ACTUAL_SPIKE,
		"realized_yield": OVERDRAW_ROW_ACTUAL_UNDER if overdraws else OVERDRAW_ROW_ACTUAL_SPIKE,
		"sustainable_yield": OVERDRAW_ROW_SUSTAINABLE,
		"overdraws": overdraws,
	}

## The reported herd — the oracle deer under another id, so the sheet composes a real take over it and
## the ⚠ is the only thing the A/B moves.
const OVERDRAW_HERD_ID := "game_deer_63"

func _overdraw_is_the_wires_answer() -> void:
	var band := _delivered_oracle_band()
	var herd := ForageFx.floorify(_delivered_oracle_herd())
	herd["id"] = OVERDRAW_HERD_ID
	var prior_band: Dictionary = h._hud._band_labor.player_band()
	var prior_bands: Array = h._hud._band_labor._player_bands
	for wire_answer in [true, false]:
		var row := _overdraw_row(bool(wire_answer))
		var worked_band := band.duplicate(true)
		worked_band["labor_assignments"] = [row]
		h._hud._band_labor._player_band = worked_band
		h._hud._band_labor._player_bands = [worked_band]
		# (0) THE PRECONDITION — on BOTH halves the retired comparison disagrees with the wire, so
		#     neither claim below can pass on a client that is still deriving. `true` rides an actual
		#     BELOW its sustainable (the first harvest the schema names) and `false` a kill turn's
		#     spike above it.
		var derived: bool = float(row["actual_yield"]) > float(row["sustainable_yield"])
		h._assert_hud("precondition: `actual > sustainable` says %s where the wire says %s"
				% [str(derived), str(wire_answer)], derived != bool(wire_answer))
		# (1) THE TILE CARD'S TOOLTIP AND THE DRAWER'S STANDING SUMMARY — one producer, the one the
		#     reported tooltip came out of.
		var readout := SourceForecast.source_yield_readout(row, SourceForecast.LABOR_KIND_HUNT)
		h._assert_hud("the worked-row readout flies the wire's ⚠ (%s)" % str(wire_answer),
			bool(readout["warn"]) == bool(wire_answer))
		# (2) THE MAP BADGE — the same row, through the renderer's own reader, since a plate is drawn
		#     to a canvas and cannot be read back.
		h._assert_hud("…and so does the map's on-tile yield label (%s)" % str(wire_answer),
			BandOverlayRenderer.yield_label_overdraw(row) == bool(wire_answer))
		# (3) THE COMPOSE SHEET — the surface that used to disagree, asked through the real producer.
		var model: Dictionary = h._hud._drawercompose._hunt_yield_model(worked_band, herd,
			SourceForecast.FLOOR_MIN, OVERDRAW_ROW_CREW, SourceForecast.IMPROVEMENT_NONE, false,
			_crew_curve(herd, SourceForecast.FLOOR_MIN))
		h._assert_hud("…and so does the compose sheet, which is where the two surfaces parted (%s)"
			% str(wire_answer),
			bool(model[DrawerComposeController.YIELD_MODEL_OVERDRAW]) == bool(wire_answer))
		# (4) THE AGREEMENT ITSELF — the pairing IS the claim, so it is asserted rather than left to
		#     three separate readings that happen to coincide.
		h._assert_hud("…and all three surfaces say the SAME thing about this one source",
			bool(readout["warn"]) == BandOverlayRenderer.yield_label_overdraw(row)
				and bool(readout["warn"])
					== bool(model[DrawerComposeController.YIELD_MODEL_OVERDRAW]))
	h._hud._band_labor._player_band = prior_band
	h._hud._band_labor._player_bands = prior_bands


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
		src, kind, HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK))

## The two crew-target pills a hunt sheet renders, off the chart model that renders them.
func _herd_crew_targets(herd: Dictionary) -> Dictionary:
	var model := SourceForecast.floor_chart_model(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, RETREAT_CHART_CREW, "hunters", LESSON_NOT_YET_LEARNED)
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
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	var raw_engage := SourceForecast.engage_workers(float(speared_forecast["axis_ceiling"]),
		float(speared_forecast["axis_per_animal"]), float(speared_forecast["engage_rate"]),
		SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF)

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
	var picker := _band_picker_control(root)
	return PANEL_BAND_PICKER_ABSENT if picker == null else picker.text

## The `Band:` picker CONTROL itself, found by the same structural rule — the actor-band block below
## drives it with real pointer input, which needs the node rather than its face.
func _band_picker_control(root: Node) -> OptionButton:
	if root == null:
		return null
	var key_seen := false
	for child in root.get_children():
		if child is Label and (child as Label).text == HudWorkVocab.BAND_PICKER_LABEL:
			key_seen = true
		elif key_seen and child is OptionButton:
			return child as OptionButton
	for child in root.get_children():
		var found := _band_picker_control(child)
		if found != null:
			return found
	return null

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


# ---- THE SHEET OPENS ON THE BAND ALREADY WORKING THE SOURCE, AND A BAND SWITCH RE-SEEDS ----------
# Reported from play, on a tile worked by Band 3 with two foragers while the panel showed Band 3: the
# sheet opened on Band 1 — four tiles away, out of forage range, no idle crew — and switching the
# picker to Band 3 moved every LIVE reading (`2 of 4 useful`, the standing-crew line) while the
# COMPOSED count stayed at Band 1's. With a composed 0 against a standing 2 the commit button became
# `Unassign`, one press from stripping the two real foragers off the tile, and the improvement
# checkbox vanished with it (the crew-0-on-a-worked-source rule, correctly applied to the wrong state).
#
# Both halves are guarded here because both are decisions about the ACTOR BAND, and both webs are
# guarded because the two compose builders are separate code.

## The band `Hud._resolve_assign_band` answers for — roster[0], with no panel band set and no unit
## selected. It works NEITHER source, which is what makes the working-band rung observable: with the
## ladder's own answer among the workers the tie goes to the ladder and nothing moves.
const ACTOR_LADDER_ENTITY := 861

## The FIRST band in roster order that works both sources — what the sheet must open on.
const ACTOR_FIRST_WORKER_ENTITY := 862

## A SECOND worker, so the band switch has somewhere to go that is also a working band. Its crew
## differs from the first's, which is the whole of the re-seed claim.
const ACTOR_SECOND_WORKER_ENTITY := 863

## The two standing crews. **Neither is `HudConst.WORKER_STEP`**: a stepper reading 1 cannot tell a
## re-seed from the no-standing-assignment fallback, so a 1 here would make the claim unfalsifiable.
## Both sit under either source's max-useful ceiling, so the stepper renders the seed rather than a
## clamp of it.
const ACTOR_FIRST_WORKER_CREW := 2
const ACTOR_SECOND_WORKER_CREW := 3

## 1-based roster positions — the picker labels bands positionally (`HudFormat.band_display_name`), so
## each is both the popup entry a press must land on (one less, the popup being 0-based) and the
## `Band N` the face must read.
const ACTOR_FIRST_WORKER_INDEX := 2
const ACTOR_SECOND_WORKER_INDEX := 3

## Idle crew on every band in the roster, well above both standing crews so no stepper is labour-bound
## and the rendered number is decided by the SEED alone.
const ACTOR_IDLE_WORKERS := 10

## The reference food tile (`BaseFx.food_tile_fixture`) and the reference herd, both at (66,10) — every
## band in the roster stands on that hex, so distance can move neither sheet's branch.
const ACTOR_TILE_X := 66
const ACTOR_TILE_Y := 10
const ACTOR_HERD_ID := "game_deer_07"

## The ordinary commit verbs, spelled out here rather than read back through `HudComposeVocab`: an
## expectation composed from the vocabulary under test can only ever agree with itself. `Unassign` is
## the one face taken from the vocabulary, because its ABSENCE is what is being claimed.
const ACTOR_FORAGE_VERB := "Forage"
const ACTOR_HUNT_VERB := "Hunt Here"

## The rung each web's improvement control offers on these fixtures — a wild Thriving patch offers
## Cultivate, a part-tamed herd offers Tame. Reached by `HudWidgets.IMPROVEMENT_CONTROL_META`, never by
## face, so the claim is about the control's PRESENCE rather than about its wording.
const ACTOR_FORAGE_RUNG := "cultivate"
const ACTOR_HUNT_RUNG := "tame"

## Where in a popup row a press is aimed, as a fraction of the row's own height and of the popup's width.
const ACTOR_POPUP_ROW_CENTRE := 0.5

## How long the driven press waits for the picker's popup to ANNOUNCE itself (`about_to_popup`).
## **A BOUND rather than an `await` on the signal**: a popup that never opens has to fail this
## chapter's own assertion, naming the press that failed, instead of parking the coroutine until
## `preview_watchdog` kills the run three minutes later with nothing to say about which press it was.
const ACTOR_POPUP_WAIT_FRAMES := 30

## The popup reported no entry under the press at all — a failure, never a skip.
const ACTOR_NO_ENTRY_PRESSED := -1

## Which popup entry the last driven press actually landed on. **A MEMBER, not a local**: a GDScript
## lambda captures a local by VALUE, so a witness assigning to one reports that nothing ever happened.
var _actor_entry_pressed := ACTOR_NO_ENTRY_PRESSED

## THREE player bands where the ladder's answer works neither source and the other two work both. The
## roster is the assertion: with one band, or with the ladder's own band among the workers, every rung
## of the resolution agrees and a state passes for free.
func _actor_band_roster() -> Array:
	return [
		_actor_band(ACTOR_LADDER_ENTITY, []),
		_actor_band(ACTOR_FIRST_WORKER_ENTITY, _actor_assignments(ACTOR_FIRST_WORKER_CREW)),
		_actor_band(ACTOR_SECOND_WORKER_ENTITY, _actor_assignments(ACTOR_SECOND_WORKER_CREW)),
	]

func _actor_band(entity: int, assignments: Array) -> Dictionary:
	return BandFx.with_band_id({
		"entity": entity, "faction": 0, "size": 90,
		"current_x": ACTOR_TILE_X, "current_y": ACTOR_TILE_Y,
		"working_age": 16, "idle_workers": ACTOR_IDLE_WORKERS,
		"work_range": 2, "hunt_reach": 7, "max_expedition_party_size": 8,
		"activity": "forage", "labor_assignments": assignments,
	})

## One standing crew on EACH web, so a single roster serves both sheets. Both carry no `improvement`,
## so the improvement control is an OFFERED box on either band and the presence claim below is about
## the crew-0 suppression rather than about which rung is being built.
func _actor_assignments(crew: int) -> Array:
	return [
		{"kind": "forage", "workers": crew, "target_x": ACTOR_TILE_X, "target_y": ACTOR_TILE_Y,
			"floor": SourceForecast.FLOOR_FOOD_PEAK, "actual_yield": 0.0, "sustainable_yield": 0.0,
			"workers_needed": crew, "overdraws": false},
		{"kind": "hunt", "workers": crew, "fauna_id": ACTOR_HERD_ID,
			"target_x": ACTOR_TILE_X, "target_y": ACTOR_TILE_Y,
			"floor": SourceForecast.FLOOR_FOOD_PEAK, "actual_yield": 0.0, "sustainable_yield": 0.0,
			"workers_needed": crew, "overdraws": false},
	]

## The four claims an open compose sheet owes the band it is composing for. They fail apart, which is
## why they are four: the picker's FACE is what the player reads, the STEPPER is the composition the
## commit would send, the VERB is what one press would do, and the improvement CONTROL is the
## affordance the played defect took away.
func _assert_actor_band(state: String, index: int, crew: int, verb: String, rung: String) -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var want_face := HudFormat.band_display_name(_actor_band_roster()[index - 1], index)
	var got_face := _band_picker_face(sheet)
	h._assert_hud("%s: the Band: picker names the band working the source (%s, got %s)"
			% [state, want_face, got_face],
		got_face == want_face)
	var got_crew := Readout.stepper_value(sheet)
	h._assert_hud("%s: the crew stepper opens on THAT band's standing crew (%d, got %d)"
			% [state, crew, got_crew],
		got_crew == crew)
	var commit := Q.compose_commit_button(sheet)
	var got_verb := "" if commit == null else commit.text
	h._assert_hud("%s: the commit button is the ordinary verb, not %s (%s, got %s)"
			% [state, HudComposeVocab.UNASSIGN_BUTTON, verb, got_verb],
		got_verb == verb)
	h._assert_hud("%s: the improvement control is rendered (%s)" % [state, rung],
		ForageFx.find_improvement_control(sheet, rung) != null)

## The VACUITY GUARD for the "not Unassign" claims above: the same sheet, on the same band, with the
## crew dialed to 0 — which really is an unassign, so the button really does change its face and the
## improvement control really does go. Without it a sheet that had lost the ability to say `Unassign`
## at all, or one that never renders an improvement control, would satisfy every claim above.
func _assert_actor_unassign(state: String, rung: String) -> void:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var commit := Q.compose_commit_button(sheet)
	var got_verb := "" if commit == null else commit.text
	h._assert_hud("%s: a composed 0 against a standing crew really does read %s (got %s)"
			% [state, HudComposeVocab.UNASSIGN_BUTTON, got_verb],
		got_verb == HudComposeVocab.UNASSIGN_BUTTON)
	h._assert_hud("%s: …and the improvement control is suppressed with it" % state,
		ForageFx.find_improvement_control(sheet, rung) == null)

## Drive the open sheet's `Band:` picker to `entry` (0-based) with REAL POINTER INPUT — the face, then
## the popup row — rather than emitting `item_selected` by hand. A faked signal calls the connected
## lambda directly and so passes on a picker whose popup never opens and, worse, on the branch where
## the engine DECLINES to report a pick at all (`labor-ui.md` → "A PICKER STATES ITS OWN SELECTION").
##
## **The popup is FREED under this function, and that is the control behaving correctly**: the pick runs
## `on_pick`, which rebuilds the compose controls and takes the picker row with it. The teardown is
## `is_instance_valid`-guarded, because an unguarded `disconnect` raises, which ABORTS the call — and an
## aborted GDScript call answers with its return type's default, which for an entry index is a legal 0.
##
## **IT SETTLES FIRST, ASSERTS ITS AIM, WAITS FOR THE POPUP AND CHECKS THE SHEET SURVIVED — and every
## one of those four is the same failure seen from a different side.** `compose_band_switch_forage`
## failed and passed clean three times: the press landed on the full-viewport dismiss CATCHER rather
## than on the picker, the sheet closed, and FIVE assertions failed downstream as a cascade from one
## bad press — reading as five independent problems. `ComposeSheet.refit` re-arms itself and
## `_place_card` has two boundary flips that move the card by hundreds of pixels, so the picker's rect
## is not final until the sheet has stopped moving.
func _pick_actor_band(entry: int) -> void:
	# THE SETTLE — this was the only geometry-sensitive path in the early chapters without one.
	await h._settle()
	var sheet: ComposeSheet = h._hud._drawercompose._compose_sheet
	var picker := _band_picker_control(sheet)
	h._assert_hud("the open sheet renders a Band: picker to drive", picker != null)
	if picker == null:
		return
	var viewport: Viewport = h.get_viewport()
	var picker_centre := picker.get_global_rect().get_center()
	# THE AIM — a press outside the card is a press on the catcher, and the frame it is aimed from is
	# the frame the assertion can still name.
	var card_rect := sheet._card.get_global_rect()
	h._assert_hud("the Band: picker's face is INSIDE the compose card before it is pressed (%s in %s)"
			% [picker_centre, card_rect],
		card_rect.has_point(picker_centre))
	var face := InputProbe.canvas_to_window(viewport, h.get_window(), picker_centre)
	InputProbe.hover(viewport, face)
	var popup := picker.get_popup()
	# THE POPUP — armed BEFORE the press, so a popup that opens on the press itself cannot be missed
	# between the connect and the wait. Polled against a BOUND rather than `await`ed on the signal: a
	# popup that never opens must fail this function's own assertion, not hand the run to the watchdog
	# 180 seconds later.
	var popped := [false]
	var on_popup := func() -> void:
		popped[0] = true
	popup.about_to_popup.connect(on_popup)
	# An `OptionButton` fires at ACTION_MODE_BUTTON_PRESS, so the popup is up before the release
	# exists — the two halves have to be driven apart.
	InputProbe.press_left(viewport, face)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, face)
	var waited := 0
	while not popped[0] and waited < ACTOR_POPUP_WAIT_FRAMES:
		await h.get_tree().process_frame
		waited += 1
	if is_instance_valid(popup):
		popup.about_to_popup.disconnect(on_popup)
	# THE SURVIVAL — one line, and it is what turns the whole cascade into a single self-explaining
	# failure naming the press that caused it.
	h._assert_hud("…and the press left the compose sheet OPEN rather than dismissing it",
		h._hud.is_compose_sheet_open())
	h._assert_hud("a press on the Band: picker's face opens its popup (announced=%s, visible=%s)"
			% [popped[0], popup.visible], popped[0] and popup.visible)
	if not popup.visible:
		return
	_actor_entry_pressed = ACTOR_NO_ENTRY_PRESSED
	var witness := func(index: int) -> void:
		_actor_entry_pressed = index
	popup.index_pressed.connect(witness)
	var row_height := float(popup.size.y) / float(maxi(popup.item_count, 1))
	var point := InputProbe.canvas_to_window(viewport, h.get_window(), Vector2(
		float(popup.position.x) + float(popup.size.x) * ACTOR_POPUP_ROW_CENTRE,
		float(popup.position.y) + row_height * (float(entry) + ACTOR_POPUP_ROW_CENTRE)))
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h.get_tree().process_frame
	if is_instance_valid(popup):
		popup.index_pressed.disconnect(witness)
	# The point is DERIVED from the popup's own rect (an item's rect is not published), so the popup
	# itself is asked which row the press hit rather than the derivation being trusted.
	h._assert_hud("…and the press lands on the band's own entry (%d, got %d)"
			% [entry, _actor_entry_pressed],
		_actor_entry_pressed == entry)

# =====================================================================================
#  THE PRE-COMMIT TAKE IS THE SIM'S ANSWER (`ForecastQuery.KIND_HUNT_CREW_TAKE`)
# =====================================================================================
# **PNG-LESS AND DRIVEN, because the defect renders a perfectly ordinary readout.** The panel lets the
# player move the crew before committing, so it re-derived the take itself — and its third bound was
# `animals_stayed(animals_engaged(w, rate), stay)`, the engagement and the retreat with NO FIGHT. On a
# Wild Aurochs with four hunters it printed **1.92 food** where the herd paid **0.84**, and the bone,
# the fibre and the hide were over by the same 2.3x, all four being fixed conversions of one biomass.
# Nothing about that frame looks wrong.
#
# **THE SENDER IS REPLACED FOR THE LENGTH OF THIS BLOCK**, so the curve the sheet is served DISAGREES
# with the client's two stages by a known factor. That is the only staging in which the two candidate
# implementations answer differently: `fixtures_forecast.gd`'s stand-in composes the same two stages
# the client used to, deliberately — so every magnitude the chapter above pins survives the move, and
# equally, a sheet still doing its own arithmetic passes every claim made against it. The seam and the
# canned answerer are put back at the end.

## What the FIGHT leaves of what the party brings to bay — a factor no client term can see, since
## `combat_config.hit_chance` is unpublished and the damage-over-durability division is the sim's. A
## HALF is chosen because it separates the two candidate readings at every magnitude the sheet renders
## (`0.75` against `1.50` animals, `0.18` against `0.36` food) rather than at one lucky crew.
const CREW_TAKE_FIGHT_SURVIVAL := 0.5

## The crew the claims are made at. It is sized so the CREW is the smallest of the take's FOUR limits
## rather than the herd's own regrowth: three hunters bring 0.74 boar to bay against 9 haulable and
## 11.67 affordable, and the fight halves that to 0.37 — under the 0.42 a turn this herd breeds back,
## which is the arm that would otherwise answer the binding-limit sentence claim (4) is about.
##
## **IT MOVED FROM SIX WHEN THE REACH STOPPED ROUNDING.** It used to sit inside the engagement
## staircase's longest flat run (`max(floor(w x 0.33), 1)`, one animal for crews one through six);
## with the reach a plain `workers x engage_rate` there is no run to sit in, and six hunters now
## out-take the regrowth — which put the herd's own breeding on the sentence instead of the crew.
const CREW_TAKE_CLAIM_CREW := 3

## The band's own spread, as `HuntCrewTakeRow.animals_low` / `_high` relative to the likely take. Both
## published stochastic stages are binomials, so a real band is asymmetric and narrow; what this needs
## is only that it is a BAND, wide enough that the rendered ends are distinct faces.
const CREW_TAKE_LOW_FRACTION := 0.6

const CREW_TAKE_HIGH_FRACTION := 1.4

## The stock a below-floor herd is dialled to, as a fraction of its own `floor x K`. Under 1 by
## construction, and far enough under that no rounding can put the sheet back above the line.
const CREW_TAKE_BELOW_FLOOR_STOCK := 0.6

## Whether the answer this block's sender composes carries a BAND at all. Flipped between the two
## halves of the degenerate-band pair; a member rather than a parameter because the sender is
## installed once and the claims are made against re-renders of it.
var _crew_take_band_is_real := false

## The curve this block answers with: the client's own two stages, then the FIGHT the client cannot
## see. Composed from the herd the ask names, so it is a stand-in server rather than a table.
func _fight_reply(request_id: int, ask: Dictionary) -> Dictionary:
	var herd := _quantisation_boar_herd()
	var rows: Array = []
	for workers in range(1, int(ask.get("max_workers", 0)) + 1):
		var likely := _boar_brought_down(workers) * CREW_TAKE_FIGHT_SURVIVAL
		rows.append({
			SourceForecast.CREW_TAKE_WORKERS_KEY: workers,
			SourceForecast.CREW_TAKE_LOW_KEY: likely * (CREW_TAKE_LOW_FRACTION \
				if _crew_take_band_is_real else 1.0),
			SourceForecast.CREW_TAKE_LIKELY_KEY: likely,
			SourceForecast.CREW_TAKE_HIGH_KEY: likely * (CREW_TAKE_HIGH_FRACTION \
				if _crew_take_band_is_real else 1.0),
		})
	# **A BARE `assert` WOULD HALT THE WHOLE HARNESS** rather than report (see
	# `.claude/rules/client/test-harnesses.md`), so the quarry check is a reported claim: this stand-in
	# answers for ONE herd, and a curve composed for a different one would be a plausible answer to the
	# wrong question.
	h._assert_hud("the crew-take stand-in is answering for %s" % String(herd["id"]),
		String(ask.get("herd_id", "")) == String(herd["id"]))
	return {"request_id": request_id, "ok": true,
		"kind": ForecastQuery.KIND_HUNT_CREW_TAKE, "per_crew": rows}

## Open this block's sheet at the composed crew, having reset the seam first: the two halves of the
## band pair restage ONE herd id with a different ANSWER, which is exactly the collision
## `ForecastQuery` is keyed to ignore (band + herd + kit + cap + floor are identical across them).
func _open_crew_take_sheet(herd: Dictionary) -> void:
	h._hud.forecast_query().reset()
	h._hud._compose.reset_hunt_source()
	h._show_herd(herd)
	h._compose_herd(herd, CREW_TAKE_CLAIM_CREW, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()

func _crew_take_readout_assertions() -> void:
	var prior_band = h._hud._band_labor.player_band()
	var prior_bands: Array = h._hud._band_labor._player_bands
	var band := _delivered_oracle_band()
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	var query: ForecastQuery = h._hud.forecast_query()
	_crew_take_band_is_real = false
	query.set_sender(func(request_id: int, ask: Dictionary) -> bool:
		# Anything that is not the crew take falls through to the harness's ordinary answerer, so this
		# block cannot silently starve a raid readout of its reply.
		if String(ask.get("kind", "")) != ForecastQuery.KIND_HUNT_CREW_TAKE:
			query.deliver.call_deferred([ForecastFx.answer(h._hud, request_id, ask)])
			return true
		query.deliver.call_deferred([_fight_reply(request_id, ask)])
		return true)

	var herd := _quantisation_boar_herd()
	await _open_crew_take_sheet(herd)
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var fpa := BOAR_BODY_MASS * BOAR_PROVISIONS_PER_BIOMASS
	var two_stage := _boar_brought_down(CREW_TAKE_CLAIM_CREW)
	var with_fight := two_stage * CREW_TAKE_FIGHT_SURVIVAL

	# (0) THE PRECONDITION — the two candidate answers really are different faces here. Without it every
	#     claim below is satisfied by a staging in which the fight happens to change nothing.
	h._assert_hud("precondition: with the fight this crew takes %s animals, without it %s"
			% [_animal_face(with_fight), _animal_face(two_stage)],
		_animal_face(with_fight) != _animal_face(two_stage))
	# …and the sheet really composed the crew the figures above are for. A cap below it would leave every
	# claim below describing a party the player is not staffing.
	h._assert_hud("precondition: the sheet composes %d hunters" % CREW_TAKE_CLAIM_CREW,
		Readout.stepper_value(sheet) == CREW_TAKE_CLAIM_CREW)

	# (1) THE TAKE SENTENCE IS THE SIM'S FIGURE. The claim the whole channel exists for: the client may
	#     not compose this number, so the readout must state the one that came back and no other.
	# **IT IS THE BINDING-LIMIT SENTENCE BELOW THE ROWS, and that is now the sheet's ONLY statement of
	#     the take** — the estimate line that used to lead the readout said the same rate a second
	#     time, with the four accounts in between, and was retired for it.
	var take_line := Readout.verdict_text(sheet)
	h._assert_hud("the take sentence states the SIM's %s animals/turn, not the fightless %s — got %s"
			% [_animal_face(with_fight), _animal_face(two_stage), take_line],
		take_line.contains(HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % _animal_face(with_fight))
			and not take_line.contains(
				HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % _animal_face(two_stage)))

	# (2) …AND SO DOES THE YIELD BENEATH IT, which is the other half of the reported defect: the four
	#     accounts are fixed conversions of ONE carried biomass, so a take over by the fight's factor
	#     puts every one of them over by it too.
	var food_face := SourceForecast.format_magnitude(with_fight * fpa)
	var fightless_food := SourceForecast.format_magnitude(two_stage * fpa)
	h._assert_hud("…and the FOOD row reads %s, not the fightless %s — got %s"
			% [food_face, fightless_food, Readout.yields_text(sheet)],
		Readout.yields_text(sheet).contains(food_face)
			and not Readout.yields_text(sheet).contains(fightless_food))

	# (3) A DEGENERATE BAND PRINTS NO RANGE. This is every reading at the shipped tuning, and range
	#     chrome that always renders manufactures doubt the model does not have.
	h._assert_hud("a degenerate band prints the bare figure with no range — got %s" % take_line,
		not take_line.contains(CREW_TAKE_RANGE_NEEDLE))

	# (4) THE BINDING LIMIT NAMES THE CREW, at the crew's own unclamped figure — the remedy the retired
	#     advisory got wrong, since it sized "12 herders would reach the floor" without the fight.
	h._assert_hud("the binding limit names the hunters and their %s — got %s"
			% [_animal_face(with_fight), Readout.verdict_text(sheet)],
		Readout.verdict_text(sheet).contains(_crew_limit_head(
			HudComposeVocab.HUNT_CREW_LABEL.to_lower(), with_fight, String(herd["species"]))))
	# …AND NAMES NO REMEDY. It closed *"— add hands to take more"*: a clause naming no count, two lines
	# under the stepper's own `max N useful here — more would be idle`, and wrong at any crew inside one
	# of that cap. The needle is LITERAL, since the constant that used to carry it is gone.
	h._assert_hud("…and offers no \"%s\" remedy under a stepper already stating the useful crew — got %s"
			% [RETIRED_CREW_REMEDY_NEEDLE, Readout.verdict_text(sheet)],
		not Readout.verdict_text(sheet).contains(RETIRED_CREW_REMEDY_NEEDLE))

	# (5) THE WIDEST READOUT THIS SHEET DRAWS STILL FITS. The Work zone's height and width budgets are
	#     full, and the take sentence is the longest thing in the box — so the fit is asserted on the
	#     state that carries the figure, its band AND its cadence in one sentence rather than trusted.
	_crew_take_band_is_real = true
	await _open_crew_take_sheet(herd)
	sheet = h._hud._drawercompose._compose_sheet
	h._assert_compose_sheet_fits("herd_crew_take_band")

	# (6) …AND A REAL BAND IS PRINTED. The pair is the claim: a rule that never rendered the range
	#     satisfies (3) alone and one that always rendered it satisfies this alone.
	take_line = Readout.verdict_text(sheet)
	h._assert_hud("a real band prints %s - %s beside the figure — got %s"
			% [_animal_face(with_fight * CREW_TAKE_LOW_FRACTION),
				_animal_face(with_fight * CREW_TAKE_HIGH_FRACTION), take_line],
		take_line.contains(HudComposeVocab.HUNT_TAKE_BAND_FORMAT % [
			_animal_face(with_fight * CREW_TAKE_LOW_FRACTION),
			_animal_face(with_fight * CREW_TAKE_HIGH_FRACTION)]))

	# (7) BELOW THE BREEDING FLOOR THE SAME SLOT SAYS SO, and says nothing else: the player is never in
	#     both states, so a second block would be a second producer of one verdict.
	var starved := herd.duplicate(true)
	starved["biomass"] = BOAR_CAPACITY * SourceForecast.FLOOR_FOOD_PEAK * CREW_TAKE_BELOW_FLOOR_STOCK
	await _open_crew_take_sheet(ForageFx.floorify(starved))
	sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("below the floor the limit line states the breeding floor — got %s"
			% Readout.verdict_text(sheet),
		Readout.verdict_text(sheet).contains(HudComposeVocab.HUNT_LIMIT_BELOW_FLOOR))

	# Back to the seam every other chapter runs on: an empty seam with the canned answerer installed.
	h._hud.forecast_query().reset()
	ForecastFx.install(h._hud)
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_band = prior_band
	h._hud._band_labor._player_bands = prior_bands

## An animals-per-turn face as the readout spells it — the SHIPPED formatter, so the needle and the
## rendered number cannot round differently.
func _animal_face(animals: float) -> String:
	return DetailFormat.animal_rate_face(animals)

## **THE CREW-LIMIT SENTENCE'S HEAD** — its own format with an EMPTY band-and-cadence tail and the full
## stop trimmed, i.e. everything up to where that tail begins. A claim about the sentence is written
## against this so it survives a fixture whose take gains or loses a range or a cadence clause; the
## tail is the business of the band and cadence claims, which assert it themselves.
func _crew_limit_head(crew_noun: String, animals: float, quarry: String) -> String:
	return (HudComposeVocab.HUNT_LIMIT_CREW_FORMAT % [
		crew_noun, _animal_face(animals), quarry, ""]).trim_suffix(".")

## **THE TWO WAYS THIS SENTENCE COULD SPELL AN ANIMALS-PER-TURN RATE, and the one it must.** Both are
## LITERAL rather than composed from `HudComposeVocab`: a claim about the FORM a constant takes cannot
## be written in terms of that constant, which would pass whatever the constant happened to say. The
## reported line read `≈0.75 Wild Boar a turn` — prose, where every other rate on this sheet is a rate.
const TAKE_RATE_UNIT_NEEDLE := "/turn"
const TAKE_PROSE_UNIT_NEEDLE := " a turn"

## **THE REMEDY THIS SENTENCE NO LONGER OFFERS.** Reported from play: it sat two lines under the
## stepper's `max 7 workers useful here — more would be idle`, named no count, and told a crew of six
## to add hands the control above had already capped. Literal, because the constant that carried it is
## deleted — a needle read out of the vocabulary could only assert that the vocabulary agrees with
## itself.
const RETIRED_CREW_REMEDY_NEEDLE := "add hands to take more"

## The mark a RANGE clause cannot be drawn without — the EN DASH `HudComposeVocab.HUNT_TAKE_BAND_FORMAT`
## separates its two ends with. Spelled as an escape rather than typed, so it cannot be confused at a
## glance with the hyphen or the em dash this file uses elsewhere; the POSITIVE half of the pair
## asserts the whole formatted clause, so a format that changed its separator fails there.
const CREW_TAKE_RANGE_NEEDLE := "\u2013"


# =====================================================================================
#  A FRACTIONAL ANIMAL IS THE NORMAL CASE, AND ONE CURVE ANSWERS EVERY CREW QUESTION
# =====================================================================================
# **REPORTED FROM PLAY, on a Wild Aurochs with eight hunters.** The sheet read
# `≈0 WILD AUROCHS/TURN · 0.00 FOOD` and *"These hunters bring down ≈0 Wild Aurochs a turn — add hands
# to take more"* beside `32 clear it now`, `8 hold it after` and `13 of 37 useful`, while the work
# board published `0.84 food/turn` for the same herd. Two defects in one frame:
#
#   1. The curve published `floor(damage ÷ durability)` — whole bodies THIS TURN — which is `0` for
#      every aurochs crew a stepper can reach, because the blow is capped by the one body in front of
#      the party. The sim publishes the RATE now; the panel's remaining half was that it may not round
#      that rate away, and that a sub-one take has to read as a WAIT rather than as a nothing.
#   2. Only the take headline had moved onto the curve. The four crew readouts beside it were still
#      `engagement_carry` quotients — the engagement and the retreat, with no attack, no defense and
#      no durability — so they answered *"how fast could this stock be drawn down"* while never asking
#      whether the animals can be killed at all. That is what put `13 of 37 useful` two lines above a
#      take of zero.
#
# The fixture is that aurochs, at terms chosen so the FIGHT is the binding arm across the whole crew
# range the claims are made over — which is the only staging in which the two candidate models answer
# differently at every size rather than at one lucky crew.

## The quarry's own terms. `durability 150` and the stand-in's 14 damage a hunter put ONE hunter at
## `0.0933` animals a turn, so eight take `0.747` — the figure the report quotes — and no crew from one
## to ten finishes a body inside a turn. Eleven is the first that does, which is what makes the
## "rising and still sub-one" sweep below a claim about a PLATEAU of zeros rather than about one crew.
const SUBONE_PROVISIONS_PER_BIOMASS := 0.4
const SUBONE_BODY_MASS := 6.0
const SUBONE_DURABILITY := 150.0
const SUBONE_ENGAGE_RATE := 0.17
const SUBONE_CAPACITY := 400.0

## `0.5 × 400 = 200` stands under the food peak, so **1.2 animals** of room stand above it — small
## enough that the curve reaches it inside the band's own pool, which is what makes *clear it now* a
## LOOKUP here rather than a fall-back to the closed form.
const SUBONE_BIOMASS := 207.2

## One hunter's food throughput: `4.0 ÷ 0.4 = 10` biomass, comfortably over one body, so the carry arm
## never binds and the take on screen is the fight's answer and nothing else.
const SUBONE_PER_WORKER_YIELD := 4.0

## The reported party.
const SUBONE_HUNTERS := 8

## How far the "every crew takes something, and more hands take more" sweep runs. Ten, because eleven
## is the first crew whose take reaches a whole animal — see `SUBONE_DURABILITY`.
const SUBONE_PLATEAU_PROBE := 10

## The herd the report was made on. Every derived term is composed from the constants above rather than
## restated, so the fixture cannot describe an animal whose meat and whose mass disagree.
func _subone_aurochs_herd() -> Dictionary:
	return {
		"id": "game_aurochs_21", "label": "Wild Aurochs (game_aurochs_21)",
		"species": "Wild Aurochs",
		"size_class": "large", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10,
		"husbandry_ceiling": "wild",
		"biomass": SUBONE_BIOMASS,
		"carrying_capacity": SUBONE_CAPACITY,
		"body_mass": SUBONE_BODY_MASS,
		"food_per_animal": SUBONE_BODY_MASS * SUBONE_PROVISIONS_PER_BIOMASS,
		"provisions_per_biomass": SUBONE_PROVISIONS_PER_BIOMASS,
		"per_worker_yield": SUBONE_PER_WORKER_YIELD,
		"engage_rate": SUBONE_ENGAGE_RATE,
		"defense": AUROCHS_DEFENSE,
		"durability": SUBONE_DURABILITY,
		"tile_info": HerdFx.compact_herd_tile_fixture(),
	}

## **IDLE HANDS STANDING BY WHILE THE `+` GATE IS PROBED.** The gate closes for two different
## reasons — the band has nobody left, or this source has all the hands it can use — and only the
## second is under test, so the band is deliberately not the thing that runs out.
const BOARD_IDLE_HANDS := 4

## **THE WORK BOARD'S `+` GATE AND THE COMPOSE SHEET, ON ONE HERD AND ONE CEILING.**
##
## The board renders many rows a frame and cannot round-trip a crew-take query for each, so it prices
## a worked row from the snapshot alone — and `max_useful_workers` then fell through to the closed
## form `take_workers`, which divides by an engagement reach carrying **no attack, no defense and no
## durability**. On a fight-bound quarry that is a different ceiling from the one the sheet's own
## curve plateaus at, for the same herd, two panels apart.
##
## **THE PRECONDITION IS ASSERTED, NOT ASSUMED.** The board's own fightless quotient is checked to
## DISAGREE with the plateau first: on a quarry where the two models happen to land together the
## equality below would pass against the defect itself.
##
## It walks the real seams rather than handing a forecast a literal — the worker map's
## presence-sensitive copy, then the board's injection — because a dropped copy is exactly the shape
## that would quietly leave the board back on the quotient.
func _board_cap_matches_the_sheet(herd: Dictionary, band: Dictionary, floor_value: float,
		plateau: int) -> void:
	var herd_id := String(herd["id"])
	var board_base := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, floor_value)
	var quotient := SourceForecast.max_useful_workers(board_base)
	h._assert_hud(("precondition: this quarry is FIGHT-bound — the board's fightless quotient says"
			+ " %d hunters are useful where the sheet's curve stops rising at %d")
			% [quotient, plateau], quotient != plateau)
	# The row as the decoder hands it over: a confirmed hunt assignment carrying the sim's own
	# `huntUsefulWorkers`, staffed one hand BELOW the ceiling so the `+` has somewhere to go.
	var below_cap := plateau - 1
	var worked := band.duplicate(true)
	worked["labor_assignments"] = [{
		"kind": SourceForecast.LABOR_KIND_HUNT, "workers": below_cap,
		"target_x": int(herd["x"]), "target_y": int(herd["y"]), "fauna_id": herd_id,
		"floor": floor_value,
		SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY: plateau,
	}]
	var merged: Dictionary = h._hud._band_labor.effective_worker_map(worked)
	var row: Dictionary = merged.get(h._hud._band_labor.pending_key(
		SourceForecast.LABOR_KIND_HUNT, int(herd["x"]), int(herd["y"]), herd_id), {})
	h._assert_hud("the worked row carries the sim's own crew ceiling through the worker map (%s)"
			% str(row.get(SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY, "absent")),
		int(row.get(SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY,
			SourceForecast.NO_CREW_ANSWER)) == plateau)
	var board := SourceForecast.with_published_useful_crew(board_base, row)
	h._assert_hud("the Work board quotes the sheet's ceiling (%d), not the fightless crew (%d)"
			% [plateau, quotient], SourceForecast.max_useful_workers(board) == plateau)
	# …and the gate the player actually presses moves with it: dead AT the ceiling, live one below,
	# with idle hands standing by either way so the source is the only thing that can be refusing.
	var at_cap := SourceForecast.source_worker_cap_state(board, plateau, BOARD_IDLE_HANDS)
	var under := SourceForecast.source_worker_cap_state(board, below_cap, BOARD_IDLE_HANDS)
	h._assert_hud("…and the row's `+` is dead at %d and live at %d, idle hands to spare on both"
			% [plateau, below_cap],
		not bool(at_cap["can_add"]) and bool(under["can_add"])
			and String(at_cap["note"]) != "")

	# **A NON-HUNT ROW KEEPS ITS OWN ANSWER, and the `0` on it is not the hunt's `0`.** The wire
	# publishes this field on every row — `0` meaning *no crew is useful here* on a hunt and *does not
	# apply* everywhere else — so the two readings must not collapse. The forage row really does carry
	# the zero (else the guard below is guarding nothing), and its cap is untouched by it.
	var patch := _retreat_crew_patch(UNCHANGED_PROBE_STAY)
	var patch_band := band.duplicate(true)
	patch_band["labor_assignments"] = [{
		"kind": SourceForecast.LABOR_KIND_FORAGE, "workers": below_cap,
		"target_x": int(patch["x"]), "target_y": int(patch["y"]), "fauna_id": "",
		"floor": floor_value,
		SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY: SourceForecast.PUBLISHED_NO_USEFUL_CREW,
	}]
	var patch_row: Dictionary = h._hud._band_labor.effective_worker_map(patch_band).get(
		h._hud._band_labor.pending_key(SourceForecast.LABOR_KIND_FORAGE,
			int(patch["x"]), int(patch["y"]), ""), {})
	h._assert_hud("a forage row carries the structural `0` every non-hunt row publishes",
		int(patch_row.get(SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY,
			SourceForecast.NO_CREW_ANSWER)) == SourceForecast.PUBLISHED_NO_USEFUL_CREW)
	var patch_cap := _source_worker_cap(patch, SourceForecast.SOURCE_KIND_FORAGE)
	var patch_forecast := SourceForecast.forecast_inputs(patch, SourceForecast.SOURCE_KIND_FORAGE,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	h._assert_hud("…and its cap stays its own %d rather than collapsing to that zero" % patch_cap,
		patch_cap > 0 and SourceForecast.max_useful_workers(
			SourceForecast.with_published_useful_crew(patch_forecast, patch_row)) == patch_cap)
	# **A PEN IS THE SAME REFUSAL ONE STEP OVER.** It IS a hunt row, so the wire publishes the field
	# on it — but a penned beast is collected rather than stalked (`NO_ENGAGEMENT_STAGE`), and its cap
	# was never the fightless quotient this replaces. A stalking curve's plateau must not bind it.
	var pen := _retreat_crew_pen(UNCHANGED_PROBE_STAY)
	var pen_cap := _source_worker_cap(pen, SourceForecast.SOURCE_KIND_HERD)
	var pen_forecast := SourceForecast.forecast_inputs(pen, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
	h._assert_hud("a PEN keeps its own cap (%d) — no engagement stage, no published plateau" % pen_cap,
		pen_cap > 0 and SourceForecast.max_useful_workers(
			SourceForecast.with_published_useful_crew(pen_forecast, {
				SourceForecast.ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY:
					SourceForecast.PUBLISHED_NO_USEFUL_CREW})) == pen_cap)

func _subone_take_assertions() -> void:
	var prior_band = h._hud._band_labor.player_band()
	var prior_bands: Array = h._hud._band_labor._player_bands
	var band := _delivered_oracle_band()
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	var herd := _subone_aurochs_herd()
	var floor_value := SourceForecast.FLOOR_FOOD_PEAK
	h._hud.forecast_query().reset()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(herd)
	h._compose_herd(herd, SUBONE_HUNTERS, floor_value)
	await h._settle()
	await h._save("herd_hunt_subone_take")
	var sheet: Control = h._hud._drawercompose._compose_sheet

	# **THE CURVE THE SHEET WAS SERVED, RE-READ RATHER THAN RE-DERIVED.** Every expectation below is a
	# lookup in these rows: an assertion written against a second copy of the client's arithmetic can
	# only agree with itself, and this arc is precisely about a panel whose readouts were a second copy.
	var pool: int = h._hud._band_labor.source_crew_pool_hunt(band, String(herd["id"]))
	var rows := ForecastFx.crew_take_rows(herd, pool, floor_value)
	var take := SourceForecast.crew_take_likely(rows, SUBONE_HUNTERS)
	var quarry := String(herd["species"])

	# (0) THE PRECONDITION THIS WHOLE BLOCK RESTS ON — the row really is a fraction of an animal, and
	#     the whole-animal reading of it really is zero. Without it every claim below passes on a
	#     fixture where the rounding never bit, which is exactly how the sim's own sweeps missed this.
	h._assert_hud("precondition: %d hunters take %.4f animals a turn — a fraction, and %d whole"
			% [SUBONE_HUNTERS, take, int(floorf(take))],
		take > 0.0 and take < 1.0 and floorf(take) == 0.0)
	# …and the two client-side arms are NOT what is binding, so the number on screen is the fight's.
	var priced: Dictionary = h._hud._drawercompose._hunt_priced_herd(herd, band)
	var carry := SourceForecast.per_worker_biomass(priced, "")
	var room_animals := SourceForecast.escapement_room(herd, "", floor_value) / SUBONE_BODY_MASS
	h._assert_hud(("precondition: neither the room (%.2f animals) nor the carry (%.2f bodies) binds"
			+ " at %d hunters") % [room_animals, float(SUBONE_HUNTERS) * carry / SUBONE_BODY_MASS,
			SUBONE_HUNTERS],
		room_animals > take and float(SUBONE_HUNTERS) * carry >= SUBONE_BODY_MASS)

	# (1) THE TAKE SENTENCE STATES THE FRACTION. The reported face is the one thing it may never print.
	# **IT IS THE BINDING-LIMIT SENTENCE BELOW THE ROWS.** The estimate line that used to lead the
	# readout carried this claim and was retired for restating a rate that sentence already quoted, so
	# the whole take reading — figure, band, cadence — is read out of the verdict register now.
	# The forbidden face is the WHOLE reported line rather than the bare `≈0`, which is a prefix of
	# `≈0.75` and would fail every correct render — the assertion passing for the wrong reason in the
	# other direction.
	var take_line := Readout.verdict_text(sheet)
	var zero_line: String = HudComposeVocab.HUNT_DELIVERED_FORMAT % ["0", quarry]
	h._assert_hud("the take sentence states %s a turn, never the reported \"%s\" — got %s"
			% [_animal_face(take), zero_line, take_line],
		take_line.contains(HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % _animal_face(take))
			and not take_line.contains(zero_line))
	# (2) …AND SAYS WHAT THE FRACTION MEANS. A decimal alone still reads as "not quite one", which is
	#     the same conclusion the `≈0` produced; the cadence is the half that makes the wait legible.
	var cadence: String = HudComposeVocab.HUNT_TAKE_CADENCE_FORMAT % \
		DetailFormat.format_trimmed(1.0 / take, HudComposeVocab.HUNT_CADENCE_DECIMALS)
	h._assert_hud("…and states the cadence a sub-one take is actually felt as (%s) — got %s"
			% [cadence.strip_edges(), take_line], take_line.contains(cadence))
	# (2a) …SPELLED AS A RATE. `≈0.75 Wild Aurochs a turn` was prose on a sheet whose every other
	#      reading is a rate, and it is the form the retired estimate line already used.
	h._assert_hud("…as \"%s%s\", never the reported \"%s%s\" — got %s"
			% [quarry, TAKE_RATE_UNIT_NEEDLE, quarry, TAKE_PROSE_UNIT_NEEDLE, take_line],
		take_line.contains(quarry + TAKE_RATE_UNIT_NEEDLE)
			and not take_line.contains(quarry + TAKE_PROSE_UNIT_NEEDLE))
	# (2b) **AND IT IS STATED ONCE.** The reported frame said the rate twice — an estimate line above
	#      `NEXT TURN` and the sentence below the rows — with the four accounts sandwiched between.
	#      Asked STRUCTURALLY, of the readout's first register: anything mounted above the caption
	#      takes index 0 and pushes the rows to 1, so a replacement line worded differently fails here
	#      too. The precondition is half the claim — on a sheet that drew no rows at all the index is
	#      `-1` and an "is it 0" test alone would be answering about nothing.
	h._assert_hud("precondition: the readout drew its rows — got \"%s\"" % Readout.yields_text(sheet),
		Readout.yields_text(sheet) != "")
	h._assert_hud("nothing stands above the NEXT TURN caption — the rows lead the readout (index %d)"
			% Readout.yields_block_index(sheet), Readout.yields_block_index(sheet) == 0)

	# (3) THE FOOD ROW IS THE SAME TAKE, VALUED. The four accounts are fixed conversions of one carried
	#     biomass, so a take rounded away takes every one of them to zero with it — which is the
	#     reported `0.00 FOOD` beside a work board quoting a live rate.
	var food_face := SourceForecast.format_magnitude(
		take * SUBONE_BODY_MASS * SUBONE_PROVISIONS_PER_BIOMASS)
	h._assert_hud("…and the FOOD row reads %s rather than the reported 0.00 — got %s"
			% [food_face, Readout.yields_text(sheet)],
		Readout.yields_text(sheet).contains(food_face)
			and not Readout.yields_text(sheet).contains(SourceForecast.format_magnitude(0.0)))

	# (4) THE BINDING-LIMIT SENTENCE QUOTES THE SAME NUMBER. It named the crew at a take of zero, which
	#     is a remedy attached to a claim that hands cannot help.
	h._assert_hud("the binding limit names the hunters at %s, the curve's own figure — got %s"
			% [_animal_face(take), Readout.verdict_text(sheet)],
		Readout.verdict_text(sheet).contains(_crew_limit_head(
			HudComposeVocab.HUNT_CREW_LABEL.to_lower(), take, quarry)))

	# (5) **THE PLATEAU READS HONESTLY.** Every crew from one to ten takes something, and each takes
	#     more than the one below it — the published curve's whole shape, and the exact span the old
	#     whole-animal reading flattened to a row of zeros.
	var rising := true
	var all_positive := true
	var all_sub_one := true
	for workers in range(1, SUBONE_PLATEAU_PROBE + 1):
		var row_take := SourceForecast.crew_take_likely(rows, workers)
		all_positive = all_positive and row_take > 0.0
		all_sub_one = all_sub_one and row_take < 1.0
		if workers > 1:
			rising = rising and row_take > SourceForecast.crew_take_likely(rows, workers - 1)
	h._assert_hud(("crews 1 to %d each take a non-zero, RISING share of an animal (%s … %s) —"
			+ " the span the whole-animal reading published as a row of zeros")
			% [SUBONE_PLATEAU_PROBE, _animal_face(SourceForecast.crew_take_likely(rows, 1)),
				_animal_face(SourceForecast.crew_take_likely(rows, SUBONE_PLATEAU_PROBE))],
		all_positive and rising and all_sub_one)

	# (6) ***CLEAR IT NOW* IS A LOOKUP IN THOSE ROWS.** The pill's crew takes at least the room, and the
	#     crew below the first that does, does not — asserted against the CURVE, never against a
	#     re-derived quotient, since re-deriving it is the defect.
	var clear := Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_CLEAR)
	var clear_by_curve := SourceForecast.crew_take_reaching(rows, room_animals)
	h._assert_hud(("*clear it now* (%d) takes at least the room's %.2f animals, and the crew below the"
			+ " curve's own answer (%d) does not")
			% [clear, room_animals, clear_by_curve],
		clear != Readout.CREW_TARGET_ABSENT and clear >= clear_by_curve
			and SourceForecast.crew_take_likely(rows, clear) >= room_animals
			and SourceForecast.crew_take_likely(rows, clear_by_curve - 1) < room_animals)

	# (7) …AND SO IS ***HOLD IT AFTER***, against what the herd breeds back at this floor.
	var growth_animals := SourceForecast.regrowth_at(
		SourceForecast.regrowth_samples(herd, ""), floor_value) / SUBONE_BODY_MASS
	var hold := Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_HOLD)
	h._assert_hud(("*hold it after* (%d) takes at least the %.2f animals a turn this herd breeds back,"
			+ " and one hand fewer does not") % [hold, growth_animals],
		hold != Readout.CREW_TARGET_ABSENT and growth_animals > 0.0
			and SourceForecast.crew_take_likely(rows, hold) >= growth_animals
			and SourceForecast.crew_take_likely(rows, hold - 1) < growth_animals)

	# (8) **AND THE STEPPER'S CAP IS WHERE THE CURVE STOPS RISING**, which is the readout that printed
	#     `13 of 37 useful` about a herd whose fourteenth hunter was still buying take. The fightless
	#     `take_workers` quotient is asserted to be a DIFFERENT number, or this claim is satisfied by a
	#     staging in which the two models happen to agree.
	var forecast: Dictionary = h._hud._drawercompose._hunt_forecast(herd, band, floor_value, rows)
	var fightless: Dictionary = h._hud._drawercompose._hunt_forecast(herd, band, floor_value)
	var plateau := SourceForecast.crew_take_plateau(rows)
	h._assert_hud("the worker cap is the curve's plateau (%d), not the fightless take crew (%d)"
			% [plateau, SourceForecast.max_useful_workers(fightless)],
		SourceForecast.max_useful_workers(forecast) == plateau
			and SourceForecast.max_useful_workers(fightless) != plateau)

	# (9) **AND THE WORK BOARD QUOTES THE SAME CEILING FOR THE SAME HERD.** The board prices a worked
	#     row with no crew-take reply in hand, so it holds `fightless` above — and `fightless` is
	#     asserted one line up to be a DIFFERENT number from the plateau, which is the precondition
	#     that makes this pair a finding rather than a coincidence on a quarry where both models
	#     agree. The sim publishes the plateau of its OWN curve on every assigned hunt row; this walks
	#     the two client seams that carry it (the worker map's presence-sensitive copy, then the
	#     board's injection) rather than handing the forecast a literal, because a broken copy is
	#     exactly the shape that would leave the board on the quotient again.
	_board_cap_matches_the_sheet(herd, band, floor_value, plateau)

	# Back to the seam and the band every other block runs on.
	h._hud.forecast_query().reset()
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_band = prior_band
	h._hud._band_labor._player_bands = prior_bands


# =====================================================================================
#  A TARGET NO CREW REACHES — THE `✕` PILL
# =====================================================================================
# ***CLEAR IT NOW* HAS AN ANSWER ON A QUARRY THAT SCATTERS, AND THE ANSWER IS "NOBODY".** The pill
# used to be DROPPED on `NO_CREW_ANSWER`, so the sheet read as having nothing to say about clearing at
# all — when what it has is a definite refusal. It renders disabled now, leading with `✕`, and the
# reason rides its tooltip.
#
# **`✕` AND DELIBERATELY NOT `∞`.** An infinity is a QUANTITY: it invites the player to keep adding
# hunters, and they do not help. The take curve plateaus at `room × stayFraction` — the animals that
# stay to be fought — so a wary quarry can never be cleared in one turn by any crew at any size.
#
# The fixture is the sub-one aurochs with its RETREAT turned up and nothing else touched: at
# `stayFraction 0.1` one animal in ten stays, the curve settles an order of magnitude below the room,
# and the two preconditions below say so out loud. Without them the render claim passes on a quarry
# whose curve reaches the target perfectly well and whose pill happens to be missing for some other
# reason.

## The retreat that puts the target out of reach. One animal in ten stays to be fought, so the curve
## settles at a tenth of the room and no crew in the band's pool clears the herd in a turn. Every
## other term is the sub-one aurochs', unchanged, so the fixture differs from the reachable one in
## exactly the field the claim is about.
const SCATTER_STAY_FRACTION := 0.1

func _scatter_aurochs_herd() -> Dictionary:
	var herd := _subone_aurochs_herd()
	herd["id"] = "game_aurochs_22"
	herd["label"] = "Wild Aurochs (game_aurochs_22)"
	herd[SourceForecast.FORECAST_STAY_FRACTION_KEY] = SCATTER_STAY_FRACTION
	return herd

func _unreachable_target_state() -> void:
	var prior_band = h._hud._band_labor.player_band()
	var prior_bands: Array = h._hud._band_labor._player_bands
	var band := _delivered_oracle_band()
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	var herd := _scatter_aurochs_herd()
	var floor_value := SourceForecast.FLOOR_FOOD_PEAK
	h._hud.forecast_query().reset()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(herd)
	h._compose_herd(herd, SUBONE_HUNTERS, floor_value)
	await h._settle()
	await h._save("herd_hunt_unreachable_target")
	var sheet: Control = h._hud._drawercompose._compose_sheet

	# (0) THE PRECONDITION — the CURVE genuinely fails, and fails permanently. `crew_take_reaching`
	#     answering the sentinel is "no crew in the band's pool gets there"; `crew_take_curve_settled`
	#     is what makes that a property of the quarry rather than of the pool's size, and it is the
	#     term `crew_to_clear` consults before it declines to price the target at all.
	var pool: int = h._hud._band_labor.source_crew_pool_hunt(band, String(herd["id"]))
	var rows := ForecastFx.crew_take_rows(herd, pool, floor_value)
	var room_animals := SourceForecast.escapement_room(herd, "", floor_value) / SUBONE_BODY_MASS
	var plateau_take := SourceForecast.crew_take_likely(rows,
		SourceForecast.crew_take_plateau(rows))
	h._assert_hud(("precondition: %.2f animals stand above the floor and the curve settles at %.2f —"
			+ " no crew in a pool of %d reaches it") % [room_animals, plateau_take, pool],
		room_animals > 0.0
			and SourceForecast.crew_take_reaching(rows, room_animals)
				== SourceForecast.NO_CREW_ANSWER
			and SourceForecast.crew_take_curve_settled(rows))
	# (1) …AND THE SENTINEL REACHES THE PILL. Read off the meta, which is the model's own answer
	#     carried onto the control — the half that says the builder was asked the question at all.
	h._assert_hud("…so the sheet's *clear it now* target is the unpriceable sentinel (%d)"
			% Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_CLEAR),
		Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_CLEAR)
			== SourceForecast.NO_CREW_ANSWER)
	# (2) **THE CLAIM: THE PILL IS THERE AND IT SAYS `✕`.** The face, not the meta — a builder that
	#     kept the sentinel on the meta and printed `-1` in the pill satisfies (1) on its own. `""` is
	#     what an ABSENT pill answers, which is the state this whole block exists to rule out.
	h._assert_hud("…and it renders a pill reading %s rather than vanishing (got \"%s\")"
			% [Readout.CREW_TARGET_UNREACHABLE_FACE,
				Readout.crew_target_face(sheet, HudWidgets.CREW_TARGET_CLEAR)],
		Readout.crew_target_face(sheet, HudWidgets.CREW_TARGET_CLEAR)
			== Readout.CREW_TARGET_UNREACHABLE_FACE)
	# (3) …AND IT IS NOT CLICKABLE, BY BOTH HALVES. `disabled` is what the player meets; the handler
	#     count is the half a driven press cannot see, since Godot swallows a click on a disabled
	#     Button — a pill left connected reads as correct in every press assertion and is one
	#     `disabled = false` from calling `on_pick` with `-1` as a worker count.
	h._assert_hud("…disabled, and wired to nothing (%d handlers)"
			% Readout.crew_target_press_handlers(sheet, HudWidgets.CREW_TARGET_CLEAR),
		Readout.crew_target_is_disabled(sheet, HudWidgets.CREW_TARGET_CLEAR)
			and Readout.crew_target_press_handlers(sheet, HudWidgets.CREW_TARGET_CLEAR) == 0)

	h._hud.forecast_query().reset()
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_band = prior_band
	h._hud._band_labor._player_bands = prior_bands


# =====================================================================================
#  THE CURVE IS RE-ASKED AS THE FLOOR MOVES (`ForecastQuery.KIND_HUNT_CREW_TAKE`)
# =====================================================================================
# **PNG-LESS AND DRIVEN, because the defect renders a perfectly ordinary readout** — the take for a
# floor the player has already dragged past, in the sheet's ordinary type, beside a chart drawn at the
# floor they are actually on. The curve is FLOOR-DEPENDENT (every row is bounded by the room standing
# above the escapement floor) and the sheet asked at the COMMITTED floor alone, so the number settled
# onto the dragged floor a frame after the drag was RELEASED. Nothing about that frame looks wrong.
#
# **THE STAND-IN MAKES THE FLOOR THE ONLY THING THAT CAN MOVE THE TAKE.** Its rows are the harness's
# ordinary two-stage-plus-fight curve scaled by a factor read off the ASKED floor — a stand-in for the
# sim's own floor-dependence, and deliberately one the client cannot reproduce: the room clamps the
# ENGAGEMENT, before the retreat and before the fight, so a floor-shifted row cannot be recovered from
# a row already in hand by scaling it. The herd is staged so that NEITHER client-side arm binds at
# either floor (asserted below), which is what makes the figure on screen the curve's answer and
# nothing else — without that precondition every claim here would pass on a sheet whose own room arm
# was quietly doing the work.

## The quarry. Three quarters of its capacity stands, so the room is ample at every floor these claims
## touch and the ROOM never becomes the binding arm; the fight does, at
## `workers × ForecastFx.FIGHT_DAMAGE_PER_HUNTER ÷ durability`.
const DRAG_CAPACITY := 400.0

const DRAG_BIOMASS := 300.0

const DRAG_BODY_MASS := 6.0

const DRAG_PROVISIONS_PER_BIOMASS := 0.4

## One hunter's food throughput: `4.0 ÷ 0.4 = 10` biomass, well over one body, so the CARRY arm never
## binds either and the two client-side bounds are both out of the way at once.
const DRAG_PER_WORKER_YIELD := 4.0

const DRAG_ENGAGE_RATE := 0.17

const DRAG_DURABILITY := 150.0

## The crew every figure below is quoted at.
const DRAG_HUNTERS := 8

## Where the sheet opens, and the floor the defect would go on quoting for the whole drag.
const DRAG_COMMITTED_FLOOR := SourceForecast.FLOOR_FOOD_PEAK

## Where the drag goes. Above the peak (so it is a floor a player reaches by pulling the line UP, the
## direction that shrinks the room) and stated as a whole percent, which is the resolution
## `HarvestFloorChart` quantises a drag to — a fixture floor finer than the control could emit would be
## testing a gesture that cannot happen.
const DRAG_LIVE_FLOOR := 0.62

## **THE FIGHT'S SURVIVAL AS A FUNCTION OF THE FLOOR** — `1 − floor`, a stand-in for the sim's own
## floor-dependence. The SHAPE is not the claim; what matters is that it is monotone, that it separates
## the two floors at the resolution the take line renders at, and that the client holds no term it
## could be reconstructed from. See the block header.
const DRAG_SURVIVAL_AT_FLOOR_ZERO := 1.0

## A sweep of the plot, as the chart would emit it: a run of DISTINCT quantised floors delivered in one
## burst with no frame between them. Twelve because the claim is a RATIO — a rate limit that suppresses
## is visible against a dozen steps and unfalsifiable against two.
const DRAG_SWEEP_STEPS := 12

const DRAG_SWEEP_FIRST_FLOOR := 0.63

const DRAG_SWEEP_FLOOR_STEP := 0.01

## What that burst is allowed to put on the socket. ONE, and only because the leading edge is allowed
## to fire if the interval happened to have elapsed before the sweep began: the burst itself runs in
## microseconds, so no second interval can pass inside it. Restore the defect and this reads twelve.
const DRAG_SWEEP_ASK_CEILING := 1

## The floor the drag moves to once the rate limit's interval has passed — the other half of the
## suppression pair, since a limiter that never asks again satisfies the ceiling above on its own.
const DRAG_REOPEN_FLOOR := 0.55

## …and the one it moves to with the answerer WITHHOLDING, where the sheet must say it is waiting
## rather than state the figure it still holds for another floor.
const DRAG_PENDING_FLOOR := 0.58

## Every floor this block's stand-in has been ASKED at, in order — the debounce claim's evidence, and
## the reason it is a count rather than a "did it eventually ask".
var _drag_asked_floors: Array = []

## Whether the stand-in answers at all. Flipped for the pending claim: the question reaches the socket
## and no reply is composed, which is exactly *an ask is outstanding*.
var _drag_withhold := false

## The fixture, made ONCE and reused. `_show_herd` / `_compose_herd` floorify in place (the regrowth
## curve, the phase cuts), and the stand-in reads the same terms the sheet does — so a fresh dict per
## call would answer for an un-floorified twin of the herd on screen.
var _drag_herd_fixture: Dictionary = {}

func _drag_herd() -> Dictionary:
	if _drag_herd_fixture.is_empty():
		_drag_herd_fixture = {
			"id": "game_aurochs_44", "label": "Wild Aurochs (game_aurochs_44)",
			"species": "Wild Aurochs",
			"size_class": "large", "huntable": true, "ecology_phase": "thriving",
			"x": 66, "y": 10,
			"husbandry_ceiling": "wild",
			"biomass": DRAG_BIOMASS,
			"carrying_capacity": DRAG_CAPACITY,
			"body_mass": DRAG_BODY_MASS,
			"food_per_animal": DRAG_BODY_MASS * DRAG_PROVISIONS_PER_BIOMASS,
			"provisions_per_biomass": DRAG_PROVISIONS_PER_BIOMASS,
			"per_worker_yield": DRAG_PER_WORKER_YIELD,
			"engage_rate": DRAG_ENGAGE_RATE,
			"defense": AUROCHS_DEFENSE,
			"durability": DRAG_DURABILITY,
			"tile_info": HerdFx.compact_herd_tile_fixture(),
		}
	return _drag_herd_fixture

func _drag_survival(floor_value: float) -> float:
	return DRAG_SURVIVAL_AT_FLOOR_ZERO - floor_value

## The curve this block answers with, and the one every expectation below is READ OUT OF. One
## definition serving both is the point: an expectation written against a second copy of the stand-in's
## arithmetic could only ever agree with itself.
func _drag_rows(max_workers: int, floor_value: float) -> Array:
	var survival := _drag_survival(floor_value)
	var scaled: Array = []
	for row in ForecastFx.crew_take_rows(_drag_herd(), max_workers, floor_value):
		var scaled_row: Dictionary = (row as Dictionary).duplicate()
		for key in [SourceForecast.CREW_TAKE_LOW_KEY, SourceForecast.CREW_TAKE_LIKELY_KEY,
				SourceForecast.CREW_TAKE_HIGH_KEY]:
			scaled_row[key] = float(scaled_row[key]) * survival
		scaled.append(scaled_row)
	return scaled

## The needle the take sentence states a rate with — the whole `≈N` clause rather than the bare digits,
## so a figure that merely APPEARS somewhere on the sheet (a crew pill, the stepper) cannot satisfy it.
## Read out of the VERDICT register: the take, its band and its cadence are the binding-limit
## sentence's since the estimate line above the yields was retired for saying the rate twice.
func _drag_take_needle(animals: float) -> String:
	return HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % _animal_face(animals)

func _crew_take_follows_the_drag_assertions() -> void:
	var prior_band = h._hud._band_labor.player_band()
	var prior_bands: Array = h._hud._band_labor._player_bands
	var band := _delivered_oracle_band()
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	var query: ForecastQuery = h._hud.forecast_query()
	_drag_asked_floors = []
	_drag_withhold = false
	query.reset()
	query.set_sender(func(request_id: int, ask: Dictionary) -> bool:
		# Anything that is not the crew take falls through to the harness's ordinary answerer, so this
		# block cannot silently starve another readout of its reply.
		if String(ask.get("kind", "")) != ForecastQuery.KIND_HUNT_CREW_TAKE:
			query.deliver.call_deferred([ForecastFx.answer(h._hud, request_id, ask)])
			return true
		_drag_asked_floors.append(float(ask.get("floor", 0.0)))
		# **`true` EVEN WHEN WITHHOLDING**, deliberately: the question reached the socket, and
		# answering `false` would be a TRANSPORT failure, which is a different sentence on the sheet
		# from the one under test.
		if not _drag_withhold:
			query.deliver.call_deferred([{"request_id": request_id, "ok": true,
				"kind": ForecastQuery.KIND_HUNT_CREW_TAKE,
				"per_crew": _drag_rows(int(ask.get("max_workers", 0)),
					float(ask.get("floor", 0.0)))}])
		return true)

	var herd := _drag_herd()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(herd)
	h._compose_herd(herd, DRAG_HUNTERS, DRAG_COMMITTED_FLOOR)
	await h._settle()
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var pool: int = h._hud._band_labor.source_crew_pool_hunt(band, String(herd["id"]))
	var committed_take := SourceForecast.crew_take_likely(
		_drag_rows(pool, DRAG_COMMITTED_FLOOR), DRAG_HUNTERS)
	var dragged_take := SourceForecast.crew_take_likely(
		_drag_rows(pool, DRAG_LIVE_FLOOR), DRAG_HUNTERS)

	# (0) THE PRECONDITION EVERY CLAIM BELOW RESTS ON — the two floors really do produce different
	#     curve rows, at the resolution the take line renders them at. Without it the whole block
	#     passes on a herd whose floor does not bind, which is the shape a re-ask test fails silently
	#     in.
	h._assert_hud("precondition: the two floors' curve rows read differently (%s at %d%%, %s at %d%%)"
			% [_animal_face(committed_take), SourceForecast.floor_percent(DRAG_COMMITTED_FLOOR),
				_animal_face(dragged_take), SourceForecast.floor_percent(DRAG_LIVE_FLOOR)],
		_animal_face(committed_take) != _animal_face(dragged_take))
	# …and the sheet composed the crew those two figures are for.
	h._assert_hud("precondition: the sheet composes %d hunters" % DRAG_HUNTERS,
		Readout.stepper_value(sheet) == DRAG_HUNTERS)
	# …and NEITHER client-side arm binds at the dragged floor, so the only thing that can move the
	#     number on screen is the curve. This is what makes the block blind-proof: with the room arm
	#     binding, a sheet still quoting the committed floor's rows would print the right answer for
	#     the wrong reason and every claim below would pass against the defect.
	var priced: Dictionary = h._hud._drawercompose._hunt_priced_herd(herd, band)
	var carry := SourceForecast.per_worker_biomass(priced, "")
	var room_animals := SourceForecast.escapement_room(herd, "", DRAG_LIVE_FLOOR) / DRAG_BODY_MASS
	h._assert_hud(("precondition: at %d%% neither the room (%.2f animals) nor the carry (%.2f bodies)"
			+ " binds on a take of %s") % [SourceForecast.floor_percent(DRAG_LIVE_FLOOR),
			room_animals, float(DRAG_HUNTERS) * carry / DRAG_BODY_MASS,
			_animal_face(dragged_take)],
		room_animals > dragged_take and float(DRAG_HUNTERS) * carry >= DRAG_BODY_MASS)

	# (1) THE SHEET OPENS ON THE COMMITTED FLOOR'S ANSWER. The state the drag starts from, asserted so
	#     that (2) is a MOVE rather than a lucky match.
	h._assert_hud("the sheet opens stating the committed floor's %s — got %s"
			% [_animal_face(committed_take), Readout.verdict_text(sheet)],
		Readout.verdict_text(sheet).contains(_drag_take_needle(committed_take)))

	# (2) …AND THE DRAG MOVES IT. The defect: only the two client-side arms recomposed under a drag,
	#     so the take line went on stating the floor the sheet opened at until the drag was released.
	var chart = Q.find_meta_node(sheet, HudWidgets.FLOOR_CHART_META)
	h._assert_hud("the sheet draws a floor chart to drag at all", chart != null)
	var asks_at_open := _drag_asked_floors.size()
	chart.emit_signal("floor_changed", DRAG_LIVE_FLOOR, false)
	await h._settle()
	var dragged_line := Readout.verdict_text(sheet)
	h._assert_hud("a LIVE drag re-states the take at the DRAGGED floor's %s, not the committed %s — got %s"
			% [_animal_face(dragged_take), _animal_face(committed_take), dragged_line],
		dragged_line.contains(_drag_take_needle(dragged_take))
			and not dragged_line.contains(_drag_take_needle(committed_take)))
	# (3) …WITHOUT REBUILDING THE SHEET. The answer arrives with no snapshot behind it and `answered`
	#     lands on `refresh_compose_sheet`, which is a rebuild — and a rebuild frees the chart the
	#     pointer is holding, so the fix would have ended every drag it served on the first reply.
	h._assert_hud("…and the chart the drag is on is still alive — the answer refilled, it did not rebuild",
		is_instance_valid(chart))
	# (4) …and the move is a fresh QUESTION at the new floor, which is the mechanism rather than the
	#     symptom: `ForecastQuery.key_of` carries the floor, so a moved floor must show up as an ask.
	h._assert_hud("the drag asked the curve at %d%% — asked %s"
			% [SourceForecast.floor_percent(DRAG_LIVE_FLOOR),
				str(_drag_asked_floors.slice(asks_at_open))],
		_drag_asked_floors.size() - asks_at_open == 1
			and is_equal_approx(float(_drag_asked_floors[-1]), DRAG_LIVE_FLOOR))

	# (5) A DRAG IS NOT ONE ASK PER EMITTED STEP. Each ask is a socket round trip and a slider emits on
	#     every pixel of motion, so flooding the command socket would be a worse defect than the
	#     staleness this closes. Asserted as a COUNT against a burst of distinct floors: "an ask
	#     eventually happens" is satisfied by a rate limit that does not limit.
	var asks_before_sweep := _drag_asked_floors.size()
	for step in range(DRAG_SWEEP_STEPS):
		chart.emit_signal("floor_changed",
			DRAG_SWEEP_FIRST_FLOOR + float(step) * DRAG_SWEEP_FLOOR_STEP, false)
	var sweep_asks := _drag_asked_floors.size() - asks_before_sweep
	h._assert_hud("a %d-step sweep may put at most %d question(s) on the socket — it put %d"
			% [DRAG_SWEEP_STEPS, DRAG_SWEEP_ASK_CEILING, sweep_asks],
		sweep_asks <= DRAG_SWEEP_ASK_CEILING)
	# (6) …AND THE LIMIT REOPENS. The other half of the pair: a limiter that simply stopped asking
	#     satisfies (5) on its own and would leave the drag stale again, one interval in.
	await _drag_wait_out_the_interval()
	var asks_before_reopen := _drag_asked_floors.size()
	chart.emit_signal("floor_changed", DRAG_REOPEN_FLOOR, false)
	h._assert_hud("…and once the interval has passed the next motion asks again (at %d%%)"
			% SourceForecast.floor_percent(DRAG_REOPEN_FLOOR),
		_drag_asked_floors.size() - asks_before_reopen == 1
			and is_equal_approx(float(_drag_asked_floors[-1]), DRAG_REOPEN_FLOOR))

	# (7) WHILE AN ASK IS OUTSTANDING THE SHEET SAYS SO. The rule this arc keeps: an unanswered query
	#     renders as PENDING rather than falling back to a smoothed client derivation — and, now that a
	#     drag re-asks, rather than to the answer it still holds for a floor the player has left. The
	#     stand-in stops answering, which is the state the claim is about.
	_drag_withhold = true
	await _drag_wait_out_the_interval()
	var asks_before_pending := _drag_asked_floors.size()
	chart.emit_signal("floor_changed", DRAG_PENDING_FLOOR, false)
	await h._settle()
	h._assert_hud("the drag asked the curve at %d%% and nothing answered"
			% SourceForecast.floor_percent(DRAG_PENDING_FLOOR),
		_drag_asked_floors.size() - asks_before_pending == 1)
	h._assert_hud("…so the sheet states NO take at all — got \"%s\""
			% Readout.verdict_text(sheet),
		not Readout.verdict_text(sheet).contains(_drag_take_needle(dragged_take))
			and not Readout.yields_text(sheet).contains(_drag_take_needle(dragged_take)))
	h._assert_hud("…and says which it is waiting on rather than leaving the slot blank",
		Readout.face_lines(sheet).has(HudComposeVocab.HUNT_TAKE_PENDING))

	# Release the drag and put the seam, the composition and the band back the way every other block
	# runs on. The release is a real commit, so the sheet ends this block rebuilt rather than refilled.
	_drag_withhold = false
	chart.emit_signal("floor_changed", DRAG_COMMITTED_FLOOR, true)
	await h._settle()
	query.reset()
	ForecastFx.install(h._hud)
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_band = prior_band
	h._hud._band_labor._player_bands = prior_bands

## Let the rate limit's window close, in FRAMES rather than in a sleep the harness has no primitive
## for. Each settle is at least one frame, so this is a handful of them; the loop reads the same clock
## the limiter does, so the two cannot drift apart when the constant moves.
func _drag_wait_out_the_interval() -> void:
	var until := Time.get_ticks_msec() + HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC
	while Time.get_ticks_msec() < until:
		await h._settle()
