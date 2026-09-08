extends RefCounted

## WORKINGS — the tile card's readout for a live deposit (arc #583, `docs/plan_extraction.md` §7,
## `.claude/rules/client/extraction-workings.md`).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. It is appended LAST for exactly that reason. See
## `.claude/rules/client/test-harnesses.md`.
##
## ⛔ **THE STATES ARE THE §7 FORK, THE TWO-WORKINGS KEY AND THE OPENING STATE, and none of the three
## can be shown by one frame.** `regrowth_rate > 0` decides which sentence a working publishes — the
## over-cut pair against the runway — and a flint scatter and a quarry are both `extraction`, so a
## chapter that staged one renewing and one finite working would pass on a client forking on `branch`.
## A tile holds up to TWO workings on one `(tile, material)` key each, which is the one claim a
## single-block card cannot fail. And since issue #650 a ROW NO LONGER MEANS A WORKING: the section
## stands a row on every discovered deposit-bearing tile, so `workings_unopened` is the state a player
## meets first and the one an idle working must not be flattened into.
##
## ⛔ **A RENDERED FIXTURE FRAME IS EVIDENCE ABOUT LAYOUT.** It says nothing about what a player can
## reach in a real game; every claim here is about what the card SAYS, made against the shipped
## composers.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 26

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The hex every state in this chapter stands on. Its own coordinates, so a working staged here can
## never be confused with the road frames' tile.
const WORKING_TILE_X := 21
const WORKING_TILE_Y := 14

## **A WOODED HIGHLAND'S TWO SEAMS**, at the shipped `extraction.json` proportions: mixed woodland
## carries 600 units of wood and renews 3% of it a turn, a karst highland 3000 units of rock and
## renews none at all. Transcribed rather than rounded, so the frames state the real ground.
const WOOD_CAPACITY := 600.0
const WOOD_REGROWTH := 0.03
const STONE_CAPACITY := 3000.0
const STONE_REGROWTH := 0.0

## What is standing on each seam — drawn down, so `stock`, `capacity` and `reachable` are three
## different numbers and a card that collapsed any two of them fails visibly.
const WOOD_STOCK := 412.0
const STONE_STOCK := 2200.0

## ⛔ **`reachable` IS WHAT THE CURRENT RUNG CAN GET AT, and on the stone seam it is a FRACTION of the
## stock** — `extraction:gathering`'s `recovery_fraction` is 0.15, so a surface picker on a 2200-unit
## rock body reaches 330 of it and no more. That gap is the whole argument for climbing the ladder,
## and a fixture whose reachable equalled its stock could not show it.
const WOOD_REACHABLE := 412.0
const STONE_REACHABLE := 330.0

## **THE OVER-CUT PAIR.** `sustainable_take` is the deposit's MSY — the growth term at the PEAK of its
## curve, never at today's stock (issue #650), which is why one figure serves a drawn-down seam and a
## full one alike; `actual_take` is what the crews paid out last turn. The wood seam is being cut
## HARDER than it renews, which is the state §7's renewable warning exists for.
const WOOD_SUSTAINABLE := 4.5
const WOOD_OVER_CUT := 7.2

## …and the same seam cut inside its own renewal, which must say NOTHING. A clause on every renewing
## working is a row spent on the absence of news.
const WOOD_SAFE_TAKE := 2.0

## **A FINITE WORKING'S RUNWAY**, `floor(reachable / actual_take)` sim-side and passed through
## verbatim. Stated rather than derived here: the client reads the sim's own answer, so a fixture that
## computed it would be asserting the client against a second derivation of the same quantity.
const STONE_TAKE := 4.4
const STONE_RUNWAY := 75

## The wire's own two runway negatives (`sim_schema::{DEPOSIT_RUNWAY_NOT_APPLICABLE,
## DEPOSIT_RUNWAY_NO_TAKE}`), read back off the vocab rather than typed, so a fixture cannot state a
## sentinel the reader does not know.
const RUNWAY_IDLE := HudDepositVocab.RUNWAY_NO_TAKE
const RUNWAY_RENEWS := HudDepositVocab.RUNWAY_NOT_APPLICABLE

## The two branches, `RungBranch`'s wire form. **They decide the knowledge track and NOT the readout**
## — which is exactly what the flint-scatter frame below is for.
const BRANCH_FORESTRY := "forestry"
const BRANCH_EXTRACTION := "extraction"

## **A FLINT SCATTER: `extraction` AND RENEWING.** Rock's rate is zero and a loose-stone scatter's is
## not, so this row shares the quarry's branch and must take the OVER-CUT arm — the one fixture that
## fails a client forking on `branch` and passes every claim a same-branch pair would make.
const SCATTER_CAPACITY := 70.0
const SCATTER_STOCK := 48.0
const SCATTER_REGROWTH := 0.02
const SCATTER_SUSTAINABLE := 0.9
const SCATTER_OVER_CUT := 2.1

## THE STANDING BILL of a felling working at home — `intensification_ladder.json`'s
## `forestry:felling.upkeep.work_per_turn`, transcribed, against a pool that is not covering it.
const WOOD_UPKEEP_DEMAND := 1.0
const WOOD_UPKEEP_SUPPLIED := 0.4
const WOOD_UPKEEP_SHORTFALL := 0.6
const WOOD_UPKEEP_KEEPERS := 1

## …and the neglect countdown that rides beside it. `forestry:felling` forgives three turns.
const WOOD_GRACE_LEFT := 2

## A free floor owes NOTHING, which is the whole of what makes it free — so its keeping rows must not
## render at all rather than rendering a bill of zero.
const FREE_FLOOR_NO_UPKEEP := 0.0

## The meter on the rung being RAISED. `1.0` is the wire's own *nothing is rising* — published exactly
## for a rung just finished AND for the top of a branch — so a fixture uses it wherever it wants no
## climb stated.
const METER_NOTHING_RISING := 1.0

## …and a real climb, part way to the coppice above.
const METER_CLIMBING := 0.42

## The build countdown for a working nobody has ordered. **`-1`, never `0`** — a zero renders as a
## finished build.
const BUILD_NO_ESTIMATE := -1

## **WHAT A FREE FLOOR REACHES OF AN UNTOUCHED SEAM**, at the shipped `intensification_ladder.json`
## fractions: `forestry:deadfall`'s `recovery_fraction` is 1.0 (fallen wood is all of it there is to
## gather), and `extraction:gathering`'s is 0.15 — so an unopened stone seam shows a 3000-unit body
## with 450 of it in reach, which is the gap the quarry rung above it buys.
const UNOPENED_WOOD_REACHABLE := WOOD_CAPACITY
const UNOPENED_STONE_REACHABLE := 450.0

## **NOTHING CAME OUT OF UNTOUCHED GROUND LAST TURN** — `extraction::NO_TAKE_THIS_TURN`, and the one
## take figure an unopened row really does zero.
##
## ⛔ **`sustainable_take` IS NOT ZEROED WITH IT.** It is the deposit's MSY, read at the PEAK of the
## curve rather than at today's stock (issue #650), so a mature untouched wood publishes its full
## `WOOD_SUSTAINABLE` — and a fixture that zeroed it would state a row no server can send AND make
## the over-cut arm's `actual > sustainable` trivially false for the wrong reason.
const UNOPENED_NO_TAKE := 0.0

## The builders on the band whose card is open, passed to `build_value` — **the crew is not what
## makes a build line render**, the sentinels are, so untouched ground states nothing here whatever
## this number is.
const UNOPENED_BUILD_CREW := 0

func run(harness) -> void:
	h = harness

	# ⛔ **STATE workings-two-seams — ONE HEX, TWO WORKINGS, AND THAT IS THE KEY SPEAKING.** The
	# registry is keyed `(tile, material)`: a wooded highland carries timber AND rock, and working one
	# is not working the other. A card that rendered one block per TILE is the defect the whole
	# `material` field exists to prevent, so this frame is judged on TWO blocks and on the two
	# materials naming themselves.
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	h._assert_hud("a hex with two seams offers the `%s` action" % HudDepositVocab.CARD_TITLE,
		_workings_action() != null)
	if not await _open_workings_card():
		h._assert_hud("the workings card opens from the tile card's action", false)
	else:
		var blocks := _workings_blocks()
		h._assert_hud("…and it draws ONE BLOCK PER WORKING, not one per tile (%d)" % blocks.size(),
			blocks.size() == 2)
		h._assert_hud("…each block keyed by its own (tile, material) pair (%s)" % str(blocks),
			blocks.has(_block_key("wood")) and blocks.has(_block_key("stone")))
		# ⛔ **THREE NUMBERS ON THE STOCK ROW, AND `reachable` IS NOT A RESTATEMENT OF EITHER.** The
		# stone seam's low rung stands on a full body and reaches a fraction of it; collapsing the pair
		# hides the argument for climbing.
		var card := _workings_card_text()
		h._assert_hud("…and the stone block states stock, capacity AND reach as three numbers (%s)"
				% HudDepositVocab.stock_value(_stone_working(STONE_TAKE)),
			card.contains(HudDepositVocab.stock_value(_stone_working(STONE_TAKE))))
		await h._save("workings_two_seams")
	_dismiss_workings_card()

	# ⛔ **STATE workings-over-cut — THE RENEWING ARM OF §7's FORK.** The wood is being cut harder than
	# it grows, so the working states the take's own hazard word — the two food webs' word, because
	# taking more than a source renews is one idea — and the FIGURES ride the hover, which a PNG
	# cannot show and this claim can.
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT)]))
	await h._settle()
	if not await _open_workings_card():
		h._assert_hud("the workings card opens on a single seam", false)
	else:
		var over_cut := _wood_working(WOOD_OVER_CUT)
		h._assert_hud("a renewing working being over-cut says so on its state line (%s)"
				% HudDepositVocab.deposit_row_value(over_cut),
			HudDepositVocab.deposit_row_value(over_cut).contains(
				SourceForecast.YIELD_OVERDRAW_WORD))
		# ⛔ **AND THE SAME SEAM CUT INSIDE ITS RENEWAL SAYS NOTHING.** A negative alone is satisfied
		# by a composer that never warns; the pair is what makes the claim mean anything.
		h._assert_hud("…while the same seam cut inside its own renewal states no warning (%s)"
				% HudDepositVocab.deposit_row_value(_wood_working(WOOD_SAFE_TAKE)),
			not HudDepositVocab.deposit_row_value(_wood_working(WOOD_SAFE_TAKE)).contains(
				SourceForecast.YIELD_OVERDRAW_WORD))
		# ⛔ **THE RUNWAY IS NOT ON A RENEWING WORKING AT ALL.** `-1` means *this deposit renews*, and
		# rendering a runway here would be the flattening the two sentinels exist to prevent.
		h._assert_hud("…and quotes no runway, the seam renewing",
			not HudDepositVocab.deposit_row_value(over_cut).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_FORMAT % STONE_RUNWAY))
		h._assert_hud("…and its hover carries the pair the one-line clause cannot (%s)"
				% HudDepositVocab.supply_tooltip(over_cut),
			HudDepositVocab.supply_tooltip(over_cut).contains(
				DetailFormat.format_trimmed(WOOD_SUSTAINABLE,
					HudDepositVocab.CARD_STOCK_DECIMALS)))
		# **THE KEEPING IS THE ROAD CARD'S TREATMENT VERBATIM**, bill then countdown, and the
		# countdown reads the BOOL before the number.
		h._assert_hud("…and the working's bill names the keepers it wants (%s)"
				% HudDepositVocab.upkeep_value(over_cut),
			HudDepositVocab.upkeep_value(over_cut).contains(
				DetailFormat.format_work_units(WOOD_UPKEEP_SHORTFALL)))
		h._assert_hud("…and counts down to it going back (%s)"
				% HudDepositVocab.reverting_value(over_cut),
			HudDepositVocab.reverting_value(over_cut) == HudDepositVocab.CARD_REVERTING_FORMAT
				% [HudSelectionVocab.RUNG_HAZARD_GLYPH, WOOD_GRACE_LEFT])
		await h._save("workings_over_cut")
	_dismiss_workings_card()

	# ⛔ **STATE workings-runway — THE FINITE ARM, AND THE FLINT SCATTER BESIDE IT.** A quarry runs out
	# and states how long; a loose-stone scatter shares its BRANCH and renews, so it takes the other
	# arm. The pair is what fails a client forking on `branch` — either claim alone passes one.
	h._show_tile(_workings_tile([_stone_working(STONE_TAKE), _scatter_working()]))
	await h._settle()
	if not await _open_workings_card():
		h._assert_hud("the workings card opens on the quarry", false)
	else:
		var quarry := _stone_working(STONE_TAKE)
		h._assert_hud("a finite working states its runway (%s)"
				% HudDepositVocab.deposit_row_value(quarry),
			HudDepositVocab.deposit_row_value(quarry).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_FORMAT % STONE_RUNWAY))
		h._assert_hud("…and a RENEWING working of the SAME branch takes the over-cut arm instead (%s)"
				% HudDepositVocab.deposit_row_value(_scatter_working()),
			HudDepositVocab.deposit_row_value(_scatter_working()).contains(
					SourceForecast.YIELD_OVERDRAW_WORD)
				and not HudDepositVocab.deposit_row_value(_scatter_working()).contains(
					HudDepositVocab.DEPOSIT_RUNWAY_IDLE))
		# **A FREE FLOOR OWES NOTHING**, so its keeping rows do not render — a sentence saying *free*
		# is a row spent on the absence of a bill.
		h._assert_hud("…and neither free floor states a bill at all",
			HudDepositVocab.upkeep_value(quarry) == ""
				and HudDepositVocab.reverting_value(quarry) == "")
		await h._save("workings_runway")
	_dismiss_workings_card()

	# ⛔ **STATE workings-idle — `-2` IS *NOT BEING WORKED*, NEVER `0 turns left`.** Nobody is cutting
	# the quarry, so there is no rate to carry forward; the working WILL run out, just not while it
	# stands idle, and a zero there would announce an exhaustion that has not happened.
	h._show_tile(_workings_tile([_idle_stone_working()]))
	await h._settle()
	if not await _open_workings_card():
		h._assert_hud("the workings card opens on an unworked quarry", false)
	else:
		var idle := _idle_stone_working()
		h._assert_hud("an unworked finite working reads `%s` (%s)"
				% [HudDepositVocab.DEPOSIT_RUNWAY_IDLE,
					HudDepositVocab.deposit_row_value(idle)],
			HudDepositVocab.deposit_row_value(idle).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_IDLE))
		h._assert_hud("…and never `%s`" % (HudDepositVocab.DEPOSIT_RUNWAY_FORMAT % 0),
			not HudDepositVocab.deposit_row_value(idle).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_FORMAT % 0))
		# **THE CREW STEPPER IS THE ONE PLACE A WORKING DIFFERS FROM A ROAD**, so its presence is
		# asserted rather than left to the frame — a card with no stepper renders perfectly plausibly.
		h._assert_hud("…and the block carries the take crew's own stepper",
			not _crew_steppers().is_empty())
		await h._save("workings_idle")
	_dismiss_workings_card()

	# ⛔ **STATE workings-unopened — THE GROUND A PLAYER MEETS FIRST, AND IT MUST NOT READ AS A
	# WORKING** (issue #650). The section publishes a row for every discovered deposit-bearing tile,
	# so most rows on the map are ground nobody has opened: full seam, the branch's free floor, no
	# bill, no build, no take. Rendered as an ordinary working that is a card of quiet zeroes — a bill
	# met, a build done, a quarry idling — which together say *this is being kept*, and none of it has
	# happened. Both branches' floors are staged at once because the FINITE one is the row that would
	# otherwise quote the idle working's runway sentence on ground that has no working.
	h._show_tile(_workings_tile([_unopened_wood(), _unopened_stone()]))
	await h._settle()
	h._assert_hud("untouched deposit-bearing ground offers the `%s` action at all"
			% HudDepositVocab.CARD_TITLE,
		_workings_action() != null)
	if not await _open_workings_card():
		h._assert_hud("the workings card opens on ground nobody has opened", false)
	else:
		var wood := _unopened_wood()
		var stone := _unopened_stone()
		h._assert_hud("…and the block says it is UNOPENED, under the free floor's own name (%s)"
				% HudDepositVocab.deposit_row_value(wood),
			HudDepositVocab.deposit_row_value(wood).begins_with(
					HudDepositVocab.rung_label(HudDepositVocab.RUNG_KEY_DEADFALL))
				and HudDepositVocab.deposit_row_value(wood).contains(
					HudDepositVocab.DEPOSIT_UNOPENED_WORD))
		# ⛔ **NO WARNING ON GROUND NOBODY IS WORKING** — neither the over-cut word (the renewing arm)
		# nor the idle runway (the finite one), and the row's ink stays the calm one.
		h._assert_hud("…and carries no hazard, no over-cut word and no runway (%s | %s)"
				% [HudDepositVocab.deposit_row_value(wood),
					HudDepositVocab.deposit_row_value(stone)],
			not HudDepositVocab.deposit_row_value(wood).contains(
					HudSelectionVocab.RUNG_HAZARD_GLYPH)
				and not HudDepositVocab.deposit_row_value(wood).contains(
					SourceForecast.YIELD_OVERDRAW_WORD)
				and not HudDepositVocab.deposit_row_value(stone).contains(
					HudDepositVocab.DEPOSIT_RUNWAY_IDLE)
				and HudDepositVocab.deposit_value_color(stone) == HudStyle.INK_DIM)
		# **THE THREE CLAUSES THAT ONLY MEAN SOMETHING ONCE SOMEBODY IS CUTTING STATE NOTHING.** A
		# zero bill rendered as a met bill says *this is being kept*, which is the false reading.
		h._assert_hud("…and states no bill, no countdown and no build line",
			HudDepositVocab.upkeep_value(stone) == ""
				and HudDepositVocab.reverting_value(stone) == ""
				and HudDepositVocab.build_value(stone, UNOPENED_BUILD_CREW) == "")
		# **THE STOCK ROW IS THE USEFUL PART AND IT STAYS** — what is here, and how much of it the
		# free floor can reach, which on the stone floor is 15% of the body.
		h._assert_hud("…while the stock row still states all three numbers (%s)"
				% HudDepositVocab.stock_value(stone),
			_workings_card_text().contains(HudDepositVocab.stock_value(stone)))
		# **THE CREW STEPPER IS THE AFFORDANCE THAT OPENS IT**, so its absence would strand the state.
		h._assert_hud("…and both blocks carry the crew stepper that opens the working (%d)"
				% _crew_steppers().size(),
			_crew_steppers().size() == 2)
		# ⛔ **AND THE PREDICATE IS NOT `actual_take == 0`.** The idle quarry two states up takes
		# nothing either, and it is a working somebody opened and walked away from — a drawn-down seam
		# that will run out. It must keep the runway's own sentence rather than reading as untouched.
		h._assert_hud("…yet a WORKING that merely took nothing still reads as one (%s)"
				% HudDepositVocab.deposit_row_value(_idle_stone_working()),
			HudDepositVocab.deposit_row_value(_idle_stone_working()).contains(
					HudDepositVocab.DEPOSIT_RUNWAY_IDLE)
				and not HudDepositVocab.deposit_row_value(_idle_stone_working()).contains(
					HudDepositVocab.DEPOSIT_UNOPENED_WORD))
		await h._save("workings_unopened")
	_dismiss_workings_card()

	# **THE HEX IS HANDED BACK BARE**, so a chapter appended after this one starts where every other
	# one does rather than on a tile carrying two workings. **An empty `deposits` array now means the
	# GROUND HOLDS NOTHING** — not *nobody has worked it*, which is a row like any other.
	h._show_tile(_workings_tile([]))
	await h._settle()
	h._assert_hud("a hex whose ground holds no deposit offers no workings action at all",
		_workings_action() == null and not h._hud.workings_controls.visible)

# ---- FIXTURES ---------------------------------------------------------------------------------
#
# Shaped exactly as `native/src/dict/deposits.rs` writes a row — one per `(tile, material)`, every
# derived number read live off the tile sim-side — so a claim made here is a claim about the wire.

## The tile card's payload, carrying whatever workings this state stages. `[]` is the ordinary case:
## the registry is sparse and lazy, so an untouched map publishes no rows at all — which is *nobody
## has worked this ground* and never *there is no deposit here*.
func _workings_tile(workings: Array) -> Dictionary:
	return {
		"x": WORKING_TILE_X, "y": WORKING_TILE_Y,
		"terrain_label": "Karst Highland",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.21,
		"temperature": 11.0,
		"deposits": workings,
	}

## A FELLING working on mixed woodland, cut at whatever rate the caller names. Its keeping is SHORT,
## which is what puts the bill and the countdown on the card.
func _wood_working(actual_take: float) -> Dictionary:
	return {
		"tile_x": WORKING_TILE_X, "tile_y": WORKING_TILE_Y,
		"material": "wood",
		"branch": BRANCH_FORESTRY,
		"stock": WOOD_STOCK,
		"capacity": WOOD_CAPACITY,
		"reachable": WOOD_REACHABLE,
		"regrowth_rate": WOOD_REGROWTH,
		"rung": HudDepositVocab.RUNG_KEY_FELLING,
		"build_fraction": METER_CLIMBING,
		"ladder_position": 60.0,
		"sustainable_take": WOOD_SUSTAINABLE,
		"actual_take": actual_take,
		"turns_remaining": RUNWAY_RENEWS,
		"upkeep_demand": WOOD_UPKEEP_DEMAND,
		"upkeep_supplied": WOOD_UPKEEP_SUPPLIED,
		"upkeep_shortfall": WOOD_UPKEEP_SHORTFALL,
		"upkeep_workers_needed": WOOD_UPKEEP_KEEPERS,
		"has_neglect_grace": true,
		"neglect_grace_remaining": WOOD_GRACE_LEFT,
		"build_turns_remaining": BUILD_NO_ESTIMATE,
		"build_blocked_reason": "",
		"is_queued": false,
		"build_kit_id": "",
		"upkeep_kit_id": "",
		"upkeep_kit_named": false,
	}

## A GATHERING working on the rock body beneath — the extraction branch's FREE FLOOR, so it owes
## nothing and has no meter to lose. Finite by arithmetic: rock's rate is zero, which is what makes
## `sustainable_take` honestly `0` and the runway its warning instead.
func _stone_working(actual_take: float) -> Dictionary:
	return {
		"tile_x": WORKING_TILE_X, "tile_y": WORKING_TILE_Y,
		"material": "stone",
		"branch": BRANCH_EXTRACTION,
		"stock": STONE_STOCK,
		"capacity": STONE_CAPACITY,
		"reachable": STONE_REACHABLE,
		"regrowth_rate": STONE_REGROWTH,
		"rung": HudDepositVocab.RUNG_KEY_GATHERING,
		"build_fraction": METER_NOTHING_RISING,
		"ladder_position": 0.0,
		"sustainable_take": 0.0,
		"actual_take": actual_take,
		"turns_remaining": STONE_RUNWAY,
		"upkeep_demand": FREE_FLOOR_NO_UPKEEP,
		"upkeep_supplied": FREE_FLOOR_NO_UPKEEP,
		"upkeep_shortfall": FREE_FLOOR_NO_UPKEEP,
		"upkeep_workers_needed": 0,
		"has_neglect_grace": false,
		"neglect_grace_remaining": 0,
		"build_turns_remaining": BUILD_NO_ESTIMATE,
		"build_blocked_reason": "",
		"is_queued": false,
		"build_kit_id": "",
		"upkeep_kit_id": "",
		"upkeep_kit_named": false,
	}

## …and the same quarry with NOBODY cutting it: `-2` on the runway and a take of nothing.
func _idle_stone_working() -> Dictionary:
	var working := _stone_working(0.0)
	working["turns_remaining"] = RUNWAY_IDLE
	return working

## **UNTOUCHED WOODLAND — the row the sim derives for a tile nobody has opened** (issue #650), and
## the ordinary case on a revealed map. It is `DepositSource::opening`'s own fingerprint: the seam
## FULL at the ground's capacity, the branch's FREE FLOOR, nothing banked on the ladder, no bill, no
## grace, nothing queued and nothing taken. Renewing ground, so its runway is `-1`.
func _unopened_wood() -> Dictionary:
	var deposit := _wood_working(UNOPENED_NO_TAKE)
	deposit["stock"] = WOOD_CAPACITY
	deposit["reachable"] = UNOPENED_WOOD_REACHABLE
	deposit["rung"] = HudDepositVocab.RUNG_KEY_DEADFALL
	deposit["build_fraction"] = HudDepositVocab.METER_UNSTARTED
	deposit["ladder_position"] = HudDepositVocab.LADDER_UNSTARTED
	# **A FREE FLOOR DECLARES NO UPKEEP AT ALL**, so the whole keeping quad goes to nothing with it —
	# the felling fixture this is derived from is the one that owes.
	deposit["upkeep_demand"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_supplied"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_shortfall"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_workers_needed"] = 0
	deposit["has_neglect_grace"] = false
	deposit["neglect_grace_remaining"] = 0
	return deposit

## …and the FINITE half of the same hex, on the extraction floor. ⛔ **This is the row that made the
## state worth a frame**: its runway sentinel is `-2`, which on a working somebody opened reads *not
## being worked* — an honest sentence there and a false one here, where there is no working.
func _unopened_stone() -> Dictionary:
	var deposit := _stone_working(UNOPENED_NO_TAKE)
	deposit["stock"] = STONE_CAPACITY
	deposit["reachable"] = UNOPENED_STONE_REACHABLE
	deposit["build_fraction"] = HudDepositVocab.METER_UNSTARTED
	deposit["turns_remaining"] = RUNWAY_IDLE
	return deposit

## **A LOOSE-STONE SCATTER — `extraction` AND RENEWING.** The one fixture that tells a `regrowth_rate`
## fork from a `branch` fork, which is why it carries the quarry's own branch string.
func _scatter_working() -> Dictionary:
	var working := _stone_working(SCATTER_OVER_CUT)
	working["material"] = "flint"
	working["stock"] = SCATTER_STOCK
	working["capacity"] = SCATTER_CAPACITY
	working["reachable"] = SCATTER_STOCK
	working["regrowth_rate"] = SCATTER_REGROWTH
	working["sustainable_take"] = SCATTER_SUSTAINABLE
	working["turns_remaining"] = RUNWAY_RENEWS
	return working

# ---- READING THE CARD -------------------------------------------------------------------------

## The tile card's `Workings ▸` action, or `null` on ground nobody has worked.
func _workings_action() -> Button:
	for control in _collect_meta(h._hud, DrawerComposeController.WORKINGS_ACTION_META, []):
		if control is Button:
			return control as Button
	return null

## Press the action and let the card land. `false` when there was nothing to press, which every caller
## reports as its own failure rather than going on to assert about an empty card.
func _open_workings_card() -> bool:
	var action := _workings_action()
	if action == null:
		return false
	action.pressed.emit()
	await h._settle()
	return not _workings_blocks().is_empty()

## Take the card down between states. A `PopupPanel` is a Window and outlives the render that opened
## it, so a frame saved with the previous state's card still up is the wrong picture with no tell.
func _dismiss_workings_card() -> void:
	h._hud._drawercompose._dismiss_workings_card()

## The open card's blocks, by their `(tile, material)` key — the roster's own identity, and the pair a
## tile-only handle could not tell apart.
func _workings_blocks() -> Array:
	var keys: Array = []
	for control in _collect_meta(h._hud, DrawerComposeController.WORKINGS_BLOCK_META, []):
		keys.append(String(control.get_meta(DrawerComposeController.WORKINGS_BLOCK_META)))
	return keys

## …and the crew steppers on them, the one control a road card has no counterpart for.
func _crew_steppers() -> Array:
	return _collect_meta(h._hud, DrawerComposeController.WORKINGS_CREW_STEPPER_META, [])

func _block_key(material: String) -> String:
	return "%d,%d:%s" % [WORKING_TILE_X, WORKING_TILE_Y, material]

## ⛔ **THE CARD IS A `Window`, SO A `Control`-ROOTED FINDER CANNOT SEE IT** — the road ladder's own
## trap, one branch over. The meta walk below follows the node kind rather than the class.
func _workings_card_text() -> String:
	var parts: Array[String] = []
	for control in _collect_meta(h._hud, DrawerComposeController.WORKINGS_BLOCK_META, []):
		for node in _all_nodes(control, []):
			if node is Label:
				parts.append((node as Label).text)
	return "\n".join(parts)

func _all_nodes(root: Node, out: Array) -> Array:
	out.append(root)
	for child in root.get_children():
		_all_nodes(child, out)
	return out

## ⛔ **IT WALKS EVERY `Node`, NOT EVERY `Control`** — the card is a `PopupPanel`, i.e. a `Window`, so
## `node_query.find_meta_node`'s `Control` gate walks straight past it and answers nothing. The road
## ladder's own trap, one branch over, and the reason this finder is local rather than shared.
func _collect_meta(root: Node, meta: StringName, out: Array = []) -> Array:
	if root == null:
		return out
	if root.has_meta(meta):
		out.append(root)
	for child in root.get_children():
		_collect_meta(child, meta, out)
	return out
