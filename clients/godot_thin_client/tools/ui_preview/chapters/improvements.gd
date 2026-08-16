extends RefCounted

## The improvement control and the tile meters.
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
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The RETIRED pause note's own stem — the line that told a player to ease workers off a build the
## floor was pacing. Spelled as a LITERAL because the vocabulary const is gone: a needle recomposed
## from a live format could only ever describe whatever the code still says.
const RETIRED_PAUSED_NOTE_NEEDLE := "ease off and it resumes"

## The RETIRED threshold's own stem — the prose that sat beside the BUILDERS stepper, and then rode
## that row label's tooltip, naming the work a build crew had to beat. A LITERAL for the reason above,
## and the claim it serves is now an outright ABSENCE: the keeping pool owes the rate at every
## fullness, so no rung declares a build-crew threshold and nothing on this sheet may state one
## (`docs/plan_standing_upkeep.md` §2.4).
const RETIRED_BUILD_FLOOR_NEEDLE := "surplus is progress"

## **THE FULLY-COMMITTED BAND'S TAKE CREW — the patch's OWN max-useful, not a number picked here.**
## The take stepper is capped at `min(pool − builders, max-useful)`, so a crew above this one is
## clamped away by the usefulness ceiling and the state stops being about the POOL at all. Reading it
## off the sim's published `workers_needed` for this patch is what keeps the two in step if the
## fixture is ever re-dialled.
const POOL_TAKE_CREW := ForageFx.CULTIVATE_SIM_WORKERS_NEEDED

## What the take is dropped to inside the sheet. Two below the crew above, so the hands the builders
## gain are COUNTABLE rather than a single step that a stepper reading either end would satisfy.
const POOL_REDUCED_TAKE := POOL_TAKE_CREW - 2

## A meter that has never moved — the state a DECLARED build with no builders sits in, and the one a
## `0%` badge and a suppressed rung row made indistinguishable from a build that had just begun.
const POOL_UNSTARTED_METER := 0.0

## **ANY TURN ESTIMATE AT ALL, on either compose face.** Both count forms open with the approximation
## mark (`HudComposeVocab.BUILD_TURNS_COUNT_FORMAT` / `_ONE`) and nothing else on those faces carries
## one, so this finds an estimate without naming a count — which is what an ABSENCE claim needs: a
## needle built from a specific number passes straight over a face quoting a different one.
const ANY_TURN_ESTIMATE_NEEDLE := "≈"

## The floor `improvement_stressed_advances` composes at — **beneath the stressed patch's own 22 / 100
## stock**, so `max(0, B − floor·K)` is positive and the crew is genuinely working it, and above
## `FLOOR_MIN`, where `learn_multiplier` is 0 and nothing would accrue for the other reason. Both ends
## matter: the frame exists to show a build advancing on a NON-Thriving source.
const BUILD_ROOM_FLOOR := 0.15

## The crew on that build, and what it owes — derived HERE from the fixture: the Cultivate costs
## `BaseFx.PLANT_CULTIVATE_WORK_COST` (50) with nothing banked (the stressed fixture re-prices its
## meter to 0), no plant gear takes anything off it, and one worker banks one work unit a turn.
##
## **THE FLOOR NO LONGER PACES IT** (`docs/plan_standing_upkeep.md` §2.2). `learn_multiplier(0.15)`
## used to scale the accrual to ×0.30, which read as 167 turns; a build crew is not pulling on the
## source, so the rate is the builders' plain output and what the floor still decides is only whether
## there is any room to work in at all.
##
## **ONE HAND, WHICH IS THE WHOLE STAFFING THIS CLAIM NEEDS** (`docs/plan_standing_upkeep.md` §2.4).
## It was three for one slice, because the rung's own rate came off the build crew and a lone builder
## on a `plant:tended` rung honestly answered `∞`. **The keeping pool owes that rate at every
## fullness now and a builder's whole output is progress**, and this patch has nothing banked on it,
## so nothing can rot: one hand banks one work unit a turn and ⌈50 ÷ 1⌉ is a real count again.
const BUILD_ROOM_BUILDERS := 1

const TURNS_AT_ROOM_FLOOR := 50

## The one count that takes the singular clause — a build one turn from done — and the smallest one
## that does not. The pair is what makes the claim about the FORK rather than about one branch.
const TURNS_SINGULAR_JOB := 1

const TURNS_PLURAL_JOB := 2

## The offer wording that must NOT appear while the rung is gated — the imperative the gated state
## exists to remove. Kept as a literal so a reworded offer cannot silently pass this assertion.
## It names SOW because Sow's site refusal is the only SOURCE gate the compose sheet can still
## render: the plant web's other rung, Cultivate, gates on knowledge alone.
const GATED_OFFER_NEEDLE := "Sow a field here"

## **THE REPORTED PAIR, in numbers** — a burst and a holding rate the MODEL can tell apart and the
## READOUT cannot: both render `0.26` at `SourceForecast.YIELD_DECIMALS`, so a gate asked of the raw
## floats drew an arrow between two identical strings.
const ARROW_ROUNDING_NOW := 0.2612
const ARROW_ROUNDING_HOLD := 0.2588
## The FODDER account standing beside it on that same frame, and correctly arrowed: a pair that
## differs at the readout's own resolution keeps its transition.
const ARROW_VISIBLE_NOW := 0.90
const ARROW_VISIBLE_HOLD := 0.87

## Dialed past every plausible cap on that frame, so what the stepper renders IS the cap.
const BUILD_CREW_DIALED_FORAGERS := 14

## Idle workers handed to the WORKED-ROW cap twin, so IDLE never becomes the binding term and the two
## probes differ only by the count under test. Any number above the cap does; this one is not the band's.
const BUILD_CREW_IDLE_ON_HAND := 14

## **THE METER VALUE THE BUILDING/REVERTING PAIR IS JUDGED AT — deliberately NEAR COMPLETE.** "Preparing
## 96%" beside "Reverting 96%" is the exact ambiguity the third state exists to remove: at a high
## percentage the two states are most alike and the stakes are highest, since what is nearly finished is
## also what there is most to lose. Both frames render this ONE number, so the word and the tint are the
## only things that can differ between them.
const REVERTING_METER_PROGRESS := 0.96

## The tile the band works INSTEAD in the reverting frame — any tile that is not the one being judged.
## The patch under test is then improved, owned and unworked, which is the whole condition.
const METER_AWAY_TILE_X := 64

## The tile card's cultivation ROW key, for the run-log excerpt. Not an assertion input: the assertions
## match the rendered VALUE markup, which no other row can produce.
const CULTIVATION_ROW_KEY := "Cultivation"

## **A CREW ON THE BUILD** — `rung_row_value`'s `build_crew` argument wherever a state wants *somebody
## is on it* without the count mattering. Named because a bare `1` at a call site says nothing about
## which of the two `BUILD_METER_HOLDS` readings the row is being asked for: a crew treading water
## (marked, amber `∞`) or a build parked on purpose (unmarked, neutral `Held at N%`).
const STAFFED_CREW := 1

## **THE ABANDONED CULTIVATE'S OWN METER** — banked work on a rung nobody is building, under a Sow that
## IS declared. Any value strictly between empty and full stages it; this one is the reference tile's.
const ABANDONED_CULTIVATE_METER := 0.6

## **A NEGATIVE THIS CLIENT DOES NOT RECOGNISE** — one past the THREE the wire spells
## (`sim_schema::{NO_BUILD_TURNS_ESTIMATE, BUILD_METER_HOLDS, BUILD_METER_ROTS}`). It stands for
## whatever the field grows next, and the claim it pins is that an unknown answer renders as the
## STALLED hazard rather than falling into whichever branch happens to be last.
##
## **IT HAS BEEN RE-AIMED TWICE NOW, AND EACH TIME IT WAS PINNING A DEFECT UNTIL IT WAS.** It was
## `-3` while the sim split `BUILD_METER_ROTS` out of `BUILD_METER_HOLDS`, and `-4` while §4.6b added
## `BUILD_QUEUE_BLOCKED` — each time the harness went on asserting that the newly-spelled value
## "renders as NO answer", holding the client's failure to follow in place, green. **A
## sentinel-is-unknown claim has to be re-aimed the day the wire spells that value**; the value below
## is one past the LAST one the schema defines, and it moves again the next time the schema grows.
const UNKNOWN_BUILD_TURNS_SENTINEL := -5

## ---- THE REPAIR (`improvement_rung_slipped`) --------------------------------------------------
## Where a slipped tended patch's meter sits: below its cost, so the rung is BUILDING again, and
## above `plant:tended`'s own `retain_fraction` (0.75), so the ground is still tended. The gap
## between those two numbers is the whole state, and a fixture that missed it on either side would
## be staging a completed rung or a lost one.
const SLIPPED_METER_PROGRESS := 0.9

## **WHAT THAT SLIPPED METER IS LOSING PER TURN** — the bleed the unpaid keeping above produces, and
## the reason the meter fell below its cost in the first place. It is the rung's own
## `meter_decay.per_turn` at a WHOLLY unsupplied keeping, which is exactly this patch's state
## (`upkeep_supplied` 0 against a full demand), so the figure is the ladder's rather than the
## fixture's.
const SLIPPED_METER_ROT_PER_TURN := 0.5

## The retired pooled `Keeping:` wording, as a LITERAL — what the land card must NOT say about a rung
## no pool is paying for. Spelled out rather than composed from the format, because the format is gone
## (issue #545) and a needle built from a live const could only ever describe what the code says now.
const UPKEEP_POOL_NEEDLE := "the pool covers"

## **THE CARD'S TURN COUNT, AS A NEEDLE THAT NAMES NO NUMBER.** Every building row opens with the
## estimate's `≈`, so asserting its absence tells a WITHDRAWN count from a shortened one — where a
## needle built from a specific count sails past a row rendering any other, `≈-1 turns` included.
const RUNG_TURNS_NEEDLE := "≈"

## The work-unit word the COMPOSE SHEET's meter spells and the CARD no longer does (issue #545).
## Asserting its absence on the card is how "the absolutes moved to the sheet" is checkable.
const BUILD_WORK_UNIT_NEEDLE := "work ("

## ---- THE PRE-COMMIT QUOTE (`improvement_unstarted_standing_price`) ---------------------------
## **A WILD PATCH, CULTIVATE DECLARED, ONE BUILDER — AND THE ANSWER IS ≈50 TURNS**
## (`docs/plan_standing_upkeep.md` §2.4). Nothing is banked, so nothing can rot, so the whole of that
## one hand's output is progress and the quote is `workCost ÷ crew`.
##
## **THIS FRAME USED TO ASSERT THE OPPOSITE, AND BOTH READINGS WERE RIGHT IN THEIR OWN SLICE.** It was
## issue #545's repro: the build crew supplied the rung's maintenance rate while the meter was below
## its cost, so one hand against `plant:tended`'s 2 netted −1 and could never advance — and the client
## quoted 50 turns for it because it netted the SOURCE's `upkeepDemand`, which is `0` on a patch with
## no progress. Slice 6a deleted the mechanism, not the arithmetic: **the keeping pool owes the rate at
## every fullness**, so the same `50 ÷ 1` is now the honest answer and the 2.0 is a standing BILL the
## face quotes as a price. The frame is re-aimed at that price rather than deleted, because the
## pre-commit quote is still exactly where a wrong answer would be invisible.
const UNSTARTED_BUILD_CREW := 1

## What the sheet must now quote — `PLANT_CULTIVATE_WORK_COST ÷ UNSTARTED_BUILD_CREW`, derived here
## rather than through the producer under test.
const UNSTARTED_BUILD_TURNS := 50

## ---- THE TURN-ESTIMATE A/B (`improvement_turns_lone_crew` / `_full_crew`) ---------------------
## **TWO CREWS ON ONE PATCH AT ONE FLOOR**, which is the only shape that can show the estimate moving.
## Both are well under the frame's own worker cap, so what differs between the frames is the count and
## nothing else.
## **WHAT THIS PATCH'S METER IS LOSING PER TURN, and it is the ONLY term that can stop a build**
## (`docs/plan_standing_upkeep.md` §2.4). The A/B is staged on a patch whose keeping pool has fallen
## far enough behind to bleed it faster than one builder banks, which brackets the pair: `TURNS_LONE_
## CREW` banks less than this and `TURNS_FULL_CREW` more.
##
## **IT REPLACED THE RUNG'S RATE, AND THE A/B IS ABOUT A SHORT POOL NOW RATHER THAN A SMALL CREW.**
## The rate used to come off the build crew, so one hand against `plant:tended`'s 2 was a negative
## net; the keeping pool owes that rate at every fullness now and a builder's whole output is
## progress. What eats a build is the rot, so a lone builder reaches `∞` only where the KEEPING is
## short — which is what this fixture stages, `patch_upkeep_shortfall` beside it saying why.
##
## **IT IS `plant:tended`'S OWN SHIPPED RATE**, not a figure picked to reach a state. The rung's
## `meter_decay.per_turn` is 0.5 and that is what a WHOLLY unsupplied keeping bleeds, which is exactly
## this patch (`patch_upkeep_supplied` 0 against a full demand). **It was staged at 2.0 for one pass**,
## above what any plant rung can bleed, because the `∞` states were thought unreachable — they are
## reached at ZERO builders instead, which this same 0.5 produces honestly.
const TURNS_METER_ROT_PER_TURN := SLIPPED_METER_ROT_PER_TURN

## What the band's keeping pool is short by on that patch, which is WHY it rots. A rot with no
## shortfall beside it is a state no sim can produce, so the two are stated together — and the pair
## is what makes the `At risk:` row on the card beneath the sheet honest.
const TURNS_KEEPING_SHORTFALL := BaseFx.PLANT_TENDED_UPKEEP_PER_TURN

const TURNS_LONE_CREW := 1

const TURNS_FULL_CREW := 4

## What each of those crews owes, derived HERE from the fixture rather than from the producer under
## test: the reference tile's Cultivate costs `BaseFx.PLANT_CULTIVATE_WORK_COST` (50) and its meter
## stands at `patch_cultivation_progress` 0.6, so 20 work units are left; no plant item declares the
## build stat yet, so the crew's gear takes nothing off that; and one worker banks one work unit a
## turn.
##
## **AND THE ROT COMES OFF THE CREW'S OUTPUT** (`docs/plan_standing_upkeep.md` §2.4). This patch's
## keeping is wholly unpaid, so its meter bleeds `TURNS_METER_ROT_PER_TURN` (0.5) every turn and the
## pace is `crew − 0.5`: ⌈20 ÷ 0.5⌉ at one hand and ⌈20 ÷ 3.5⌉ at four.
##
## **BOTH CREWS FINISH, and that is the shipped plant web being honest.** No plant rung can bleed
## faster than one worker banks, so a lone builder is SLOW rather than doomed — which is what the pair
## demonstrates, the estimate moving with the stepper. **The `∞` states are reached at ZERO builders**,
## where the whole bleed is the net, and the frame asserts them there.
##
## Under the retired `crew − rate` these read `∞` and 10, a lone builder having been unable to clear
## the rung's 2-work maintenance rate — the tax on building that §4.6a deleted.
const TURNS_AT_LONE_CREW := 40

const TURNS_AT_FULL_CREW := 6

## The mark every turn clause carries, whatever its count — the `≈` both `BUILD_TURNS_COUNT_*`
## formats open with. Asserting its ABSENCE is how a withdrawn estimate is told from a shortened one,
## without pinning the claim to a particular number.
const BUILD_TURNS_CLAUSE_MARK := "≈"

## The floor a LIVE drag lands on for the third frame — the `Learning` preset, above the peak. Read
## off the preset table rather than restated, so the drag lands where the sheet's own mark is.
const TURNS_DRAG_FLOOR := SourceForecast.FLOOR_PRESET_VALUES[SourceForecast.FLOOR_PRESET_LEARN]

## **THE PLANT WEB'S GEAR TERM, and it is empty for a structural reason**: no plant item declares the
## build stat yet (issue #539), so no forage kit arms anybody for a build and every frame in this
## chapter exercises the ungeared arm. The kit half of the form is
## `chapters/compose_rungs.gd`'s `_kit_swap_turn_estimate_states`, on the web whose gear exists.
const NO_BUILD_GEAR := {}

## The take the sheet quotes on `improvement_build_crew`: the crew clamps to the sim's own
## `workers_needed` (3), and 3 × 0.32 = 0.96 — exactly the food-peak ceiling, i.e. the crew that
## saturates the patch. **It is the PLAIN take, and that is the frame's subject now**: a Cultivate is
## running beside it and takes nothing off what these gatherers carry (`docs/plan_standing_upkeep.md`
## §2.2), where the retired dip would have quoted a quarter of it.
const BUILD_CREW_UNDIPPED_TAKE := "0.96"

## One rung row's rendered VALUE CELL — `[color=#HEX]<value>[/color]`, exactly as
## `DetailFormat.detail_bbcode` emits it. Word and tint in ONE needle, because a rung's failure states
## are a failure of BOTH and an assertion that pinned only one of them would pass on half a fix.
##
## **THE VALUE IS THE WHOLE ROW NOW** (issue #545): the card states `≈11 turns (96%)` rather than a
## verb over an indented estimate, so a needle built from a verb would be asserting a readout that no
## longer exists. The caller spells the expected value from the shipped format, so the claim stays
## about which of the four hazard states this is and what colour it wears.
func _rung_value_markup(value: String, hex: String) -> String:
	return "[color=#%s]%s[/color]" % [hex, value]

## **ONE RUNG ROW'S VALUE AT A GIVEN PUBLISHED TURN ANSWER**, asked of the producer over a source
## dict carrying nothing but that answer and this chapter's meter — the shape that reaches the states
## no frame here stages (a stall, an unrecognised sentinel) without building a fixture for each.
func _rung_value_for_turns(turns: int) -> String:
	return DetailFormat.rung_row_value({
		SourceForecast.FORECAST_BUILD_TURNS_KEY: turns,
		SourceForecast.FORECAST_BUILD_WORK_COST_KEYS[SourceForecast.IMPROVEMENT_TAME]:
			BaseFx.PLANT_CULTIVATE_WORK_COST,
		# **THE METER RIDES THE DICT, not only the `progress` argument** — `rung_row_value` asks
		# `build_verb` whether this rung is the one in flight, and a dict with no meter answers NO, so
		# the row would take the not-in-flight branch and never reach the sentinel under test.
		SourceForecast.FORECAST_BUILD_METER_KEYS[SourceForecast.IMPROVEMENT_TAME]:
			REVERTING_METER_PROGRESS,
	}, HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_TAME,
		SourceForecast.SOURCE_KIND_HERD, DetailFormat.husbandry_built_label(), false,
		REVERTING_METER_PROGRESS, STAFFED_CREW, SourceForecast.IMPROVEMENT_NONE)

## **ARE ALL FOUR HAZARD STATES MARKED?** With the standing `Keeping:` row retired, a bare rung row is
## the ONLY thing that says a rung is fine — so a failure state rendering without
## `RUNG_HAZARD_GLYPH` reads as success, which is precisely the defect the unstaffed build was. Asked
## as ONE conjunction because the claim is about the SET: any state escaping the mark is the bug, and
## a per-state frame would sample rather than close it.
func _hazard_states_all_marked() -> bool:
	var mark := HudSelectionVocab.RUNG_HAZARD_GLYPH
	# (1) declared, nobody assigned, nothing banked — the row states a sentence, not a meter.
	var unstarted := DetailFormat.rung_row_value({}, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, SourceForecast.SOURCE_KIND_HERD,
		DetailFormat.husbandry_built_label(), false, 0.0, SourceForecast.BUILD_CREW_NONE,
		SourceForecast.IMPROVEMENT_TAME)
	# **(2) WORK BANKED AND NOBODY ON IT LEFT THIS SET, AND ITS ABSENCE IS ASSERTED HERE** (§4.6a). It
	# was `⚠ Reverting`, on the premise that a parked meter must be bleeding; the keeping pool holds it
	# at any fullness now, so with the keeping covered it is a decision the player made, and marking it
	# would teach them to ignore the mark. The negative rides in this conjunction rather than in a
	# frame because this is where the SET is decided.
	var held := DetailFormat.rung_row_value({
		SourceForecast.FORECAST_BUILD_TURNS_KEY: SourceForecast.BUILD_TURNS_HOLDS,
		SourceForecast.FORECAST_BUILD_METER_KEYS[SourceForecast.IMPROVEMENT_TAME]:
			REVERTING_METER_PROGRESS,
	}, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, SourceForecast.SOURCE_KIND_HERD,
		DetailFormat.husbandry_built_label(), false, REVERTING_METER_PROGRESS,
		SourceForecast.BUILD_CREW_NONE, SourceForecast.IMPROVEMENT_NONE)
	# (3) a crew banking exactly the ROT, (4) a crew under it — the meter going backwards — and
	# (5) staffed with nothing accruing anyway. The rotting one is in this conjunction rather than
	# only on its own frame because it is the newest sentinel, i.e. the one most likely to be added
	# to the wire and forgotten on the way in.
	# **(6) A BUILT RUNG THAT IS NOT THE AT-RISK ONE — the routing negative** (§4.6a). Only one meter on
	# a source is ever at risk, so a patch mid-Sow must not mark the tended rung under it. It rides this
	# conjunction rather than a frame alone because the SET is what decides whether a mark means
	# anything: a false one costs every true one its meaning, exactly as a missing one does.
	var not_at_risk := DetailFormat.rung_row_value(_two_live_meters_short_of_keeping(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
		SourceForecast.SOURCE_KIND_FORAGE, DetailFormat.cultivation_built_label(), true,
		SourceForecast.BUILD_METER_FULL, SourceForecast.BUILD_CREW_NONE,
		SourceForecast.IMPROVEMENT_NONE)
	# **(7) THE SAME ROUTING ON THE *UNBUILT* ARM, which is where review found it in the countdown.**
	# A Cultivate abandoned mid-build with a `Sow` declared over it is not the rung in flight, so it
	# states its own condition — marked here, the keeping being short — and never the Field's count.
	# The two arms are asserted separately because `built` forks before the routing does, so a fix to
	# one says nothing about the other.
	var abandoned := _an_abandoned_cultivate_under_a_declared_sow()
	var stale_count := DetailFormat.rung_row_value(abandoned,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
		SourceForecast.SOURCE_KIND_FORAGE, DetailFormat.cultivation_built_label(), false,
		ABANDONED_CULTIVATE_METER, SourceForecast.BUILD_CREW_NONE,
		SourceForecast.IMPROVEMENT_SOW)
	return unstarted.contains(mark) and not held.contains(mark) \
		and not not_at_risk.contains(mark) \
		and stale_count.contains(mark) \
		and not stale_count.contains(RUNG_TURNS_NEEDLE) \
		and _rung_value_for_turns(SourceForecast.BUILD_TURNS_HOLDS).contains(mark) \
		and _rung_value_for_turns(SourceForecast.BUILD_TURNS_ROTS).contains(mark) \
		and _rung_value_for_turns(SourceForecast.BUILD_TURNS_NO_ESTIMATE).contains(mark)

## **THE REVIEWER'S OWN WALK — a Cultivate abandoned mid-build with a `Sow` declared over it.**
## `build_verb` honours the Sow (its meter is at zero, which is the one state a declaration answers
## for), so the source publishes the FIELD's countdown — and the Cultivation row, which is not the rung
## in flight, must not print it. The keeping is short, so what that row states instead is its own
## condition: `⚠ Reverting 60%`.
##
## **THE TWO PER-SOURCE NUMBERS NAME DIFFERENT RUNGS HERE**, which is the whole reason the routing is
## per-number: `at_risk_rung` answers CULTIVATE (the newest meter carrying work) while `build_verb`
## answers SOW. A row asking either question of the other gets the wrong answer.
func _an_abandoned_cultivate_under_a_declared_sow() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_is_cultivated"] = false
	tile["patch_cultivation_progress"] = ABANDONED_CULTIVATE_METER
	tile["patch_is_field"] = false
	tile["patch_field_progress"] = 0.0
	tile["patch_upkeep_supplied"] = 0.0
	tile["patch_upkeep_shortfall"] = BaseFx.PLANT_TENDED_UPKEEP_PER_TURN
	return BaseFx.price_plant_build(tile, BOTH_METERS_FIELD_TURNS)

## **A PATCH HOLDING A TENDED RUNG WHILE ITS FIELD GOES UP, WITH THE BAND'S KEEPING SHORT** — the one
## shape on which the at-risk mark has a choice of rows to land on, and the shape a mark on the wrong
## row is invisible in. The Field is the newest meter carrying work, so it is what the published
## shortfall is resolved through, and the tended rung under it is not being billed at all.
func _two_live_meters_short_of_keeping() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_is_cultivated"] = true
	tile["patch_cultivation_progress"] = SourceForecast.BUILD_METER_FULL
	tile["patch_is_field"] = false
	tile["patch_field_progress"] = BOTH_METERS_FIELD_PROGRESS
	tile["patch_upkeep_supplied"] = 0.0
	tile["patch_upkeep_shortfall"] = BaseFx.PLANT_FIELD_UPKEEP_PER_TURN
	return BaseFx.price_plant_build(tile, BOTH_METERS_FIELD_TURNS)

## **A METER PARKED ON PURPOSE — the one rung state that is NOT a failure** (§4.6a). It replaced
## `_reverting_value`, whose row (`⚠ Reverting 96%`) fired on *work banked and nobody on it*: an
## inference that a parked meter must be bleeding, true only while an unbuilt rung was billed to its
## build crew. The keeping pool holds it at any fullness now, so the wire's `-2` at zero builders is a
## player's decision, and the row wears **no mark and no `∞`**.
func _held_value() -> String:
	return HudSelectionVocab.RUNG_HELD_FORMAT % HudFormat.progress_percent(REVERTING_METER_PROGRESS)

## …and what a rung that is NOT the one in flight reads when its keeping is short. It is the surviving
## half of the retired reverting row: the sim's `-2`/`-3` answer for the at-risk meter, and this row is
## not the one the countdown is about, so it states its condition rather than a number that is not its.
func _reverting_value() -> String:
	return HudSelectionVocab.RUNG_REVERTING_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH,
		HudFormat.progress_percent(ABANDONED_CULTIVATE_METER)]

## …and the BUILDING row's, which leads with the sim's own count.
func _building_value(turns: int) -> String:
	return HudSelectionVocab.RUNG_TURNS_FORMAT % [
		turns, HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## …and the HOLDING row's, which leads with the ∞ under the same mark.
func _holding_value() -> String:
	return HudSelectionVocab.RUNG_HOLDING_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH, DetailFormat.BUILD_TURNS_NEVER_GLYPH,
		HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## …and the ROTTING row's, which is the same ∞ plus the words that say the meter is going BACKWARDS.
## Composed from the shipped format so the claim stays about WHICH hazard this is, not about wording.
func _rotting_value() -> String:
	return HudSelectionVocab.RUNG_ROTTING_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH, DetailFormat.BUILD_TURNS_NEVER_GLYPH,
		HudSelectionVocab.RUNG_ROTTING_PHRASE,
		HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## …and the BLOCKED row's — the fourth sentinel (`docs/plan_standing_upkeep.md` §4.6b). It carries the
## hazard mark like every other failure state and **no `∞`**: that glyph is a statement about a crew's
## arithmetic, and no crew size is the remedy for a queue standing on a refusing gate.
func _blocked_value() -> String:
	return HudSelectionVocab.RUNG_BLOCKED_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH,
		HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## **THE BLOCKED FRAME'S KEEPING**, in work per turn — a demand the band's pool covers only part of,
## which is the state `build_blocked_lines` pairs the block with. The two are stated apart rather than
## as a shortfall, because the sim publishes all three and a fixture that derived one of them would be
## making the client's own no-subtraction rule untestable.
const BLOCKED_UPKEEP_DEMAND := 6.1
const BLOCKED_UPKEEP_SUPPLIED := 4.1

## …and the turns of grace left on it, so the `At risk:` row states a countdown rather than the
## already-being-lost form.
const BLOCKED_GRACE_TURNS := 2

## A blocked build states ONE line where the keeping is not also short — the cause alone. Named
## because the count IS the claim: a second line there is the keeping remedy rendering on a cause it
## is not the lever for.
const BLOCKED_CAUSE_ONLY_LINES := 1

## The SECOND cause key the negatives assert by equality. Deliberately one whose remedy has nothing
## to do with either the keeping or the floor, so a table that answered one sentence for every key
## cannot satisfy both it and the escapement claim.
const BLOCKED_KNOWLEDGE_REASON := "knowledge"

## The staple tile as the COMPOSE SHEET sees it — `BaseFx.food_tile_fixture` already runs through
## `BaseFx.seed_forage_rows`, so this is simply the named handle the dip-comparison assertion reads its
## forecast from. Naming it keeps that assertion from re-stating which fixture it is judging.
func _seeded_food_tile() -> Dictionary:
	return BaseFx.food_tile_fixture()

## **THE REFERENCE PATCH WITH ITS KEEPING SHORT, so its half-built Cultivate is BLEEDING**
## (`docs/plan_standing_upkeep.md` §2.4) — the turn-estimate A/B's own tile, and the only shape on the
## plant web that can reach either `∞` from a staffed build crew.
##
## **THE ROT IS THE FIXTURE'S CLAIM AND THE SHORTFALL IS ITS REASON.** A meter loses ground because
## the band's keeping pool did not cover it past the rung's grace, so stating the bleed without the
## shortfall beside it would stage a state no sim can produce — and the `At risk:` row the card
## renders beneath the sheet reads the shortfall, not the rot.
##
## It is built rather than borrowed for the reason every fixture in this chapter is: `food_tile_fixture`
## is the corpus's reference patch and rots at nothing, which is what every OTHER frame here needs.
func _short_kept_food_tile() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_upkeep_supplied"] = 0.0
	tile["patch_upkeep_shortfall"] = TURNS_KEEPING_SHORTFALL
	return BaseFx.price_plant_build(tile, BaseFx.BUILD_TURNS_REMAINING,
		TURNS_METER_ROT_PER_TURN)

## The running face's TURN CLAUSE alone — ` — ≈20 turns` — composed through the shipped format with an
## empty meter half, so the assertion pins the clause and not the meter beside it. The format is the
## HUD's own; what this chapter states independently is the COUNT.
## A patch's standing stock as a share of its capacity, read off the FIXTURE — the quantity a floor is
## compared against when asking whether anything stands above it. Stated here rather than as a
## constant so a fixture that re-dials either term cannot leave the precondition asserting a stale
## ratio.
func _stock_fraction(tile: Dictionary) -> float:
	return float(tile["patch_biomass"]) / float(tile["patch_carrying_capacity"])

func _turns_clause(turns: int) -> String:
	return HudComposeVocab.IMPROVEMENT_RUNNING_TURNS_FORMAT % ["",
		HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % turns]

## …and the clause a crew that NEVER finishes earns, composed the same way. It wears no `≈`: every
## other reading here is an estimate, and this one is not — at or below the rate the meter does not
## advance at all, so there is no distribution to hedge.
## …and the clause a build PARKED on a kept meter earns — `held`, in words. It carries no count and no
## `≈` for the reason the `∞` does not: a parked meter is a standing fact rather than an estimate.
func _held_clause() -> String:
	return HudComposeVocab.IMPROVEMENT_RUNNING_TURNS_FORMAT % ["",
		HudComposeVocab.BUILD_TURNS_HELD]

func _never_clause() -> String:
	return HudComposeVocab.IMPROVEMENT_RUNNING_TURNS_FORMAT % ["",
		HudComposeVocab.BUILD_TURNS_NEVER_FORMAT % DetailFormat.BUILD_TURNS_NEVER_GLYPH]

func run(harness) -> void:
	h = harness

	# ---- THE IMPROVEMENT CONTROL: three states, one axis (issue #442 §3) ------------------------
	# State 442-cultivate-running — the RUNNING state. A patch with a standing Cultivate improvement
	# renders a CHECKED box carrying the build meter, with the stance row above it untouched. The stance
	# is Sustain here; the frame below it says Deplete, and the two are equally legal.
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._compose.reset_forage_source()
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	# **LABOUR-BOUND ON PURPOSE, and the frame beneath this one is read against that.** The readout's
	# take is `min(crew x per_worker x dip, ceiling)`, and the dip multiplies the CREW — so this crew
	# binds under both floors' ceilings and the take is the same at either. That is the deep-floor
	# frame's whole point: a deeper draw buys this crew nothing today and still stalls its meter.
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("improvement_running_plant")
	var sustain_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	var sustain_verdict = Readout.verdict_text(h._hud._drawercompose._compose_sheet)
	var running_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate")
	# **A RUNNING BUILD IS A STATE** (`docs/plan_standing_upkeep.md` §2.4). The checkbox existed so it
	# could be UNCHECKED, which sent `abandon_improvement`; the verb is derived from the meter now, so
	# there is nothing stored for an uncheck to clear. **Asserted by STATE, never by type** — §4.7a ①
	# made every state a `Label`, so a type test separates nothing.
	h._assert_hud("a running Cultivate renders the RUNNING state",
		running_box is Label
			and String(running_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_RUNNING)
	h._assert_hud("…carrying the build meter the sim reports (60%)",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate").contains("60%"))
	# READ OFF THE RENDERED PICKER, not off the compose model: the model is the input this frame sets,
	# so asserting it back proves only that the harness can write a field. What the frame claims is that
	# the stance ROW still shows Sustain lit beside a running build.
	h._assert_hud("…and the stance row is untouched, still on the band's own Sustain",
		Readout.rung_is_selected(Q.find_policy_rung(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK)))
	# **THE PAYOFF LEFT THE FACE AND LANDS IN THE READOUT — asserted as a PAIR**, because "gone from
	# the face" alone is satisfied by a sheet that lost the payoff altogether. The face used to close
	# on `· then 1.39 food` directly above a PER TURN box quoting a different number for the same
	# patch; the terms now read inside that box, beside the take they are meant to be compared with.
	h._assert_hud("…and the running box's face carries no payoff at all",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
			.contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
	h._assert_hud("…because it reads in the PER TURN readout, under the rung's own ONCE TENDED key",
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet).contains(
			String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS[
				SourceForecast.IMPROVEMENT_CULTIVATE]).to_upper()))
	# **THE BLOCK IS EXACTLY ONE ROW, AND THE COUNT IS HOW THAT IS PINNED.** It briefly carried a
	# second row stating the crew's undipped take; that went because the baseline is visible by
	# unticking the box, and because at a crew that saturates the source the dip costs nothing — so
	# the row printed the SAME figures as the headline directly above it, under a crew note saying
	# each carries half as much. No `contains` claim can see that return: a baseline row satisfies
	# every one of them and its numbers are legitimate.
	h._assert_hud("…as the block's ONLY row, the undipped baseline having been retired",
		Readout.improvement_deal_rows(h._hud._drawercompose._compose_sheet) == 1)
	# …and the general form of the same defect: nothing in the deal may restate a number the take
	# above it already prints, whatever row it arrives on.
	h._assert_hud("…repeating no magnitude the yields row already states",
		not Readout.deal_repeats_a_yields_number(h._hud._drawercompose._compose_sheet))
	# **A COMPOSED BUILD STATES ONE TRANSITION, AND IT IS THE LABELLED ONE.** This crew reaches its
	# floor, so the sheet has a floor walk it COULD state — and must not, because the `ONCE TENDED`
	# row directly beneath the caption is a different "later" (the next rung, after a ~25-turn build)
	# and the player read the labelled row as the caption's `after`. Reported from play. Nothing is
	# lost: the verdict two lines down still narrates the walk in prose.
	#
	# **ASSERTED AS A PAIR, caption AND readings**, because either alone is satisfied by the mismatch
	# this arc exists to prevent: a caption that dropped `now → after` over rows still drawing arrows
	# reads exactly like the fix to a header-only claim. Matched by EQUALITY, since a `contains` on
	# the dip half passes on any caption sharing that prefix.
	h._assert_hud("a composed build's yields caption states the plain per-turn unit, with no floor walk in it",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.YIELD_ROW_HEADER.to_upper())
	h._assert_hud("…and no reading under it draws an arrow for the caption to have keyed",
		not Readout.yields_show_a_transition(h._hud._drawercompose._compose_sheet))
	# **KNOWN LESSON + A BUILD IN FLIGHT — the teaching line goes SILENT** (`docs/plan_standing_upkeep.md`
	# §2.2). Cultivation completed several frames above, so `Teaching cultivation at ×1.00` would be
	# teaching a craft this faction finished learning; and the BUILDING half that used to survive it
	# went with the floor's term on the build rate. The dial buys this source nothing further, and
	# silence is the honest way to say so. Both halves asserted, so a line that merely dropped the
	# craft's word cannot pass.
	h._assert_hud("a lesson the faction already knows is not taught again beside a running build",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_LESSON_NEEDLE))
	h._assert_hud("…and no BUILDING half survives it — the floor paces no build now",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_BUILD_NEEDLE))
	# **THE LEVER ON A RUNNING BUILD IS THE BAND'S `builders` ROLE, so this sheet mounts none**
	# (`docs/plan_standing_upkeep.md` §2.5). It carried a BUILDERS stepper for one slice and this claim
	# asserted its presence; a verb declares and names no hands now, so the honest reading is that the
	# sheet still offers exactly ONE stepper — the take's.
	h._assert_hud("a running improvement mounts NO per-source builders stepper",
		Readout.stepper_count(h._hud._drawercompose._compose_sheet)
			== Readout.COMPOSE_STEPPERS_PER_SHEET)
	# **THE NON-VACUITY HALF OF THE SUPPRESSION CLAIM, and it is not optional here.** Both caption
	# claims above are satisfied for free by a crew that never reaches its floor — there would be no
	# walk to suppress — so the same crew and floor are re-composed over a patch NOBODY IS BUILDING,
	# where the walk must be back in full.
	#
	# **WHAT MOVES BETWEEN THE TWO READINGS IS THE METER, because the meter IS the build now**
	# (`docs/plan_standing_upkeep.md` §2.4). It used to be one line — untick the box — and that no
	# longer expresses anything: a patch carrying progress is building it whether or not a verb is
	# declared, so a declaration cleared over a live meter changes nothing on the sheet. The band
	# goes with it, or its own standing `improvement` is honoured against the zeroed meter and the
	# derivation puts the build straight back.
	#
	# PNG-less, and placed AFTER every assertion that holds a node from the saved frame: a re-compose
	# frees the sheet's children, so a node captured earlier is a freed instance. The running
	# composition is restored immediately, leaving the sheet as the saved frame had it.
	var prior_build_band: Dictionary = h._hud._band_labor._player_band
	var prior_build_bands: Array = h._hud._band_labor._player_bands
	var idle_band := BandFx.without_builders(BandFx.cultivating_forage_band_fixture())
	idle_band["labor_assignments"][0].erase("improvement")
	h._hud._band_labor._player_band = idle_band
	h._hud._band_labor._player_bands = [idle_band]
	h._hud._compose.reset_forage_source()
	h._compose_forage(BaseFx.unbuilt(BaseFx.food_tile_fixture()))
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.unbuilt(BaseFx.food_tile_fixture()))
	await h._settle()
	h._assert_hud("…while the SAME crew on a patch nobody is building walks to its floor and says so",
		Readout.yields_show_a_transition(h._hud._drawercompose._compose_sheet)
			and Readout.yields_header(h._hud._drawercompose._compose_sheet)
				== SourceForecast.YIELD_ROW_HEADER_WITH_AFTER.to_upper())
	h._hud._band_labor._player_band = prior_build_band
	h._hud._band_labor._player_bands = prior_build_bands
	h._hud._compose.reset_forage_source()
	h._compose_forage(BaseFx.food_tile_fixture())
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()

	# State 442-deplete-beside-cultivate — **THE FRAME THE WHOLE TWO-AXIS MODEL EXISTS TO MAKE SAYABLE.**
	# The same running Cultivate at a DEEP FLOOR: legal, un-gated, and self-defeating through the
	# ecology rather than through a rule. The deeper floor frees a larger ceiling, and on this
	# deliberately labour-bound crew that buys NOTHING — the take is the food-peak frame's to the
	# decimal — while the lesson the same crew is building at slows with the floor. That is the trap
	# stated at its sharpest: you drive the patch out of Thriving and stall your own meter for nothing.
	h._hud._compose.set_forage_floor(ForageFx.DEEP_DRAW_FLOOR)
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("improvement_deplete_while_building")
	# **THE RENDERED TAKE IS THE SAME AT BOTH FLOORS, and asserting that is what makes this pair agree
	# with the floor-independence claim forty lines down** — they are two readings of ONE number, the
	# crew's dipped take, and they used to contradict each other outright. The rendered claim said the
	# take MOVED and passed anyway, because it compared the whole yields string and the string carried
	# a `now → after` reading that did move; the take itself never did. That walk is suppressed while a
	# build is composed, so the string is now the take alone and the old claim fails honestly.
	#
	# **The non-vacuity companion is the VERDICT**, which really is floor-driven on these two frames —
	# the same crew that holds a patch at the food peak cannot draw it to a deep floor, and the
	# sentence says so — so without it "the take did not move" would pass on a sheet that had stopped
	# rendering. **The teaching line cannot be that companion any more**: this faction has already
	# learned Cultivation, and since the floor stopped pacing the build
	# (`docs/plan_standing_upkeep.md` §2.2) a known lesson leaves the aside silent at every floor.
	# The PAYOFF is deliberately floor-independent too (a property of the finished rung), so it can
	# stand in for neither.
	var deplete_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	var deplete_verdict = Readout.verdict_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: take  peak=%s  deep=%s" % [sustain_yields, deplete_yields])
	print("ui_preview: verdict  peak=%s  deep=%s" % [sustain_verdict, deplete_verdict])
	h._assert_hud("…and the RENDERED take does NOT move with the floor — this crew binds at both",
		sustain_yields != "" and deplete_yields != "" and sustain_yields == deplete_yields)
	h._assert_hud("…while the VERDICT this crew earns DOES move with the deeper draw",
		sustain_verdict != "" and deplete_verdict != "" and sustain_verdict != deplete_verdict)
	# BOTH AXES, READ OFF THEIR OWN CONTROLS. This asserted the two compose-model fields the frame had
	# just written, which is true whatever the sheet rendered — and "no gate, no repaint" is precisely a
	# claim about the rendering: the Deplete rung must be lit AND live, with the Cultivate box still
	# checked beside it.
	var deplete_rung = Q.find_policy_rung(
		h._hud._drawercompose._compose_sheet, SourceForecast.FLOOR_PRESET_STRIP)
	var building_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CULTIVATE)
	# A deep floor is NOT one of the three presets, so no preset lights — which is the honest reading
	# and the thing a picker of shortcuts must be able to say. What the frame claims is that the
	# improvement box is untouched by the floor beside it: no gate, no repaint.
	h._assert_hud("a deep floor stands beside a running Cultivate — no gate, no repaint",
		deplete_rung != null and not deplete_rung.disabled and not Readout.rung_is_selected(deplete_rung)
		and building_box is Label
		and String(building_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
			== HudWidgets.IMPROVEMENT_STATE_RUNNING)
	# **THE DEAL CARRIES ONE FORECAST NOW** (`docs/plan_standing_upkeep.md` §2.2). It used to carry two
	# — the take today and the *preparing* take beside it — and the pair existed only because one crew
	# did both jobs. What survives is the claim the pair was really about: the gatherers' take rises
	# with a deeper floor and the rung on the table pays what it pays whatever the floor is, so the
	# deal's payoff is floor-INDEPENDENT while the take under it is not.
	var band = h._hud._band_labor.player_band()
	var deep_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		ForageFx.DEEP_DRAW_FLOOR, SourceForecast.IMPROVEMENT_CULTIVATE)
	var peak_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.IMPROVEMENT_CULTIVATE)
	h._assert_hud("the rung's payoff is floor-INDEPENDENT — a deep draw buys the same Tended Patch",
		is_equal_approx(float(deep_deal["payoff"]), float(peak_deal["payoff"])))
	h._assert_hud("…while the take under it still rises with the floor, so the forecast is not floor-blind",
		SourceForecast.expected_yield(deep_deal["base_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
		>= SourceForecast.expected_yield(peak_deal["base_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band))

	# State 442-build-crew — **THE SHEET AND THE SIM, ON ONE NUMBER.** The control is the SIM's answer:
	# `workers_needed`, read back off the very assignment the sheet is composed over, so the harness is
	# never comparing a number it chose to a number it chose again.
	#
	# **IT IS THE TAKE ACTIVITY'S COUNT AND NOTHING ELSE** (`docs/plan_standing_upkeep.md` §2.2). It
	# used to blend the rung's `crew_needed` into a take inverted out of a DIPPED carry, so committing
	# to a 25-turn Cultivate quadrupled the hands the panel asked for. Both terms are retired: this is
	# `ceil(ceiling / per_worker)` and a build in flight does not move it.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(BUILD_CREW_DIALED_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("improvement_build_crew")
	var sim_workers_needed = int((HudBandLaborState.labor_assignments_of(
		h._hud._band_labor.player_band())[0] as Dictionary)["workers_needed"])
	var rendered_cap = Readout.stepper_value(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build crew  sim workers_needed=%d  rendered cap=%d" % [
		sim_workers_needed, rendered_cap])
	h._assert_hud("the compose stepper caps at the crew the SIM asks for (%d)"
		% sim_workers_needed, rendered_cap == sim_workers_needed)
	# THE WORKED-ROW TWIN, on the SAME forecast. `source_worker_cap_state` is the Band panel's gate, and
	# the two are only genuinely one ceiling if it goes dead at exactly that count — asserted on either
	# side of it so "always false" cannot pass.
	var build_forecast := SourceForecast.forecast_inputs(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK)
	var row_below: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed - 1, BUILD_CREW_IDLE_ON_HAND)["can_add"])
	var row_at: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed, BUILD_CREW_IDLE_ON_HAND)["can_add"])
	h._assert_hud("…and the WORK BOARD's `+` gates at the same count — live below it, dead at it",
		row_below and not row_at)
	# THE READOUT'S TAKE, read off the RENDERED sheet: `min(w × per_worker, ceiling)`, with no build
	# term anywhere in it. A Cultivate is running on this very patch, which is what makes the claim
	# worth making — the crew building it is a different crew.
	var build_green = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build crew  take=%s" % build_green)
	h._assert_hud("the green forecast line quotes the PLAIN take the sim pays (%s) beside a running build"
		% BUILD_CREW_UNDIPPED_TAKE, build_green.contains(BUILD_CREW_UNDIPPED_TAKE)
		and build_green.contains(SourceForecast.YIELD_RENEWABLE_NOTE.to_upper()))
	# **AND THERE IS NO BUILDERS STEPPER ON IT** (`docs/plan_standing_upkeep.md` §2.5, §3.1). The verb
	# carried a crew for one slice and this frame asserted the stepper's PRESENCE; a verb declares now
	# and the hands are a band-level role, so the claim is inverted. **It is the §3.1 guard**: with the
	# pool at zero the sim honestly publishes no estimate, and the tempting repair is a hypothetical
	# crew slider to re-price it — which is exactly the per-source build staffing this slice deletes.
	h._assert_hud("…and the running Cultivate offers NO per-source builders stepper",
		Readout.stepper_count(h._hud._drawercompose._compose_sheet)
			== Readout.COMPOSE_STEPPERS_PER_SHEET)

	# WALKING AWAY, plant side — driven here rather than on the frame above because committing CLOSES
	# the sheet and writes a pending assign, which the Deplete frame beside it reads. It is `unqueue`
	# now: the crew-zero form SET the declaration rather than clearing it, which is the live defect
	# slice 6b folded in (`docs/plan_standing_upkeep.md` §2.5).
	await h._assert_walk_away_emits(SourceForecast.LABOR_KIND_FORAGE, "cultivate",
		"unqueue %d %d %d" % [HudConst.PLAYER_FACTION_ID,
			int(BaseFx.food_tile_fixture()["x"]), int(BaseFx.food_tile_fixture()["y"])],
		{"faction": HudConst.PLAYER_FACTION_ID,
			"x": int(BaseFx.food_tile_fixture()["x"]),
			"y": int(BaseFx.food_tile_fixture()["y"]), "herd_id": ""})

	# State 442-cultivate-no-room — **THE FLOOR STANDS ABOVE THE STOCK, so the sheet quotes NOTHING.**
	# It was `improvement_paused_plant`, and it asserted the contradiction as a pass: the frame is a
	# Stressed patch, and the control carried "⚠ Paused — … this only advances while Thriving" because
	# the phase was not Thriving. No rung stops on the phase (`docs/plan_harvest_floor.md` §3.2 replaced
	# that cliff with a rate), so that line contradicted the meter beside it and prescribed the reverse
	# of the remedy. What DOES stop this build is the other reading of the same fixture: at the food
	# peak nothing stands above the floor on a patch holding 22 of 100, so `crew_is_working_the_source`
	# is false, the sim accrues nothing and publishes no estimate — and `build_turns_at` now carries
	# that predicate too, instead of quoting the fastest number on the axis for a build going nowhere.
	var no_room_tile := TileFx.stressed_tile_fixture()
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.reset_forage_source()
	h._show_tile(no_room_tile)
	h._compose_forage(no_room_tile)
	await h._settle()
	await h._save("improvement_no_room_plant")
	var no_room_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate")
	var no_room_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: no-room build  face=%s" % no_room_face)
	# The PRECONDITIONS, without which the two claims below are made about a frame that never staged
	# the case: the composed floor really is above this patch's own stock fraction, and the phase the
	# retired note keyed off really is non-Thriving.
	h._assert_hud("the composed floor really does stand above the patch's stock, so there is no room",
		SourceForecast.FLOOR_FOOD_PEAK > _stock_fraction(no_room_tile)
			and SourceForecast.escapement_room(no_room_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX,
				SourceForecast.FLOOR_FOOD_PEAK) <= SourceForecast.BUILD_NO_ESCAPEMENT_ROOM)
	h._assert_hud("…and the patch is not Thriving, so the retired pause line would have fired",
		String(no_room_tile["patch_ecology_phase"]) != HudFloraVocab.ECOLOGY_PHASE_THRIVING)
	h._assert_hud("a build with no room states NO estimate — the sheet cannot quote what the sim will not accrue",
		not no_room_face.contains(ANY_TURN_ESTIMATE_NEEDLE))
	h._assert_hud("…and no PAUSE line is stated anywhere on the sheet, the phase gating nothing",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, RETIRED_PAUSED_NOTE_NEEDLE))
	# **IT READS AS DECLARED, AND THAT CHANGED WITH THE QUEUE** (`docs/plan_standing_upkeep.md` §4.6b).
	# The claim was RUNNING while a per-source crew existed: this patch has NO work banked, so the only
	# thing that made it "in flight" was a build crew on the tile. The hands are a band-level pool now
	# and it funds the HEAD of the queue, so a wild patch nobody has queued is a DECLARATION — a live,
	# ticked checkbox the player can withdraw — which is the state the one-way door was closed for.
	h._assert_hud("a stalled rung nobody has queued reads as DECLARED, not as work in flight",
		no_room_box != null
			and String(no_room_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_DECLARED)
	# **THE SHARPEST CASE FOR THE UNGATED RULE.** A STALLED build is exactly when a player reaches for
	# the remedy, so the lever must be there — and this is the one frame where withholding it would
	# look defensible (the source has left Thriving, which is what USED to gate the build's own start).
	h._assert_hud("a stalled build still offers its BUILDERS stepper — staffing it is the whole point",
		Readout.stepper_count(h._hud._drawercompose._compose_sheet)
			== Readout.COMPOSE_STEPPERS_PER_SHEET)

	# State 442-cultivate-stressed-advances — **THE SAME STRESSED PATCH, BUILDING.** Only the floor
	# moved: dropped beneath the patch's own stock the crew is working it again, so the build accrues
	# and the face quotes the turns. This is the half that says the frame above is about the ROOM and
	# not about the phase — a client still keyed on `ecology_phase` renders the pause line here, over a
	# meter its own face shows advancing, which is the reported defect exactly.
	h._hud._compose.set_forage_floor(BUILD_ROOM_FLOOR)
	h._compose_forage(no_room_tile)
	# **THE BUILD CREW IS DIALLED AFTER THE FIRST OPEN**, the `_compose_herd` re-open contract in the
	# forage web's own form: a source change re-seeds the composition (`seed_forage`), so a count set
	# before the sheet is opened on this tile is thrown away. The estimate is the BUILD crew's now, so
	# without this the frame renders a face with no turns clause at all.
	BandFx.staff_builders(h._hud._band_labor, BUILD_ROOM_BUILDERS)
	h._compose_forage(no_room_tile)
	await h._settle()
	await h._save("improvement_stressed_advances")
	var advancing_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: stressed advancing  face=%s" % advancing_face)
	# **THE COUNT IS OFF THE FACE (§4.7a ①), so the claim is on the PRODUCER.** What the frame is about
	# is the ROOM — a floor beneath the stock makes the build accrue again — and the estimate is what
	# says so; where it RENDERS is the Work tab's now, and the face beside it is the pointer.
	var advancing_turns := SourceForecast.build_turns_at(no_room_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
		BUILD_ROOM_BUILDERS, BUILD_ROOM_FLOOR, {})
	h._assert_hud("a floor beneath the stock leaves room, so the SAME patch quotes %d turns (got %d)"
		% [TURNS_AT_ROOM_FLOOR, advancing_turns], advancing_turns == TURNS_AT_ROOM_FLOOR)
	h._assert_hud("…and the face still names the rung, so the frame is not simply empty",
		advancing_face.contains(String(
			HudComposeVocab.IMPROVEMENT_OFFER_LABELS[SourceForecast.IMPROVEMENT_CULTIVATE])))
	h._assert_hud("…and still says nothing about being paused, on a patch that is still Stressed",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, RETIRED_PAUSED_NOTE_NEEDLE))
	# Back to the peak for the states below, which state no floor of their own.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)

	# State 442-cultivate-done — the DONE state. A finished patch's rung becomes a static LABEL (no box
	# to uncheck, nothing to clear), and the NEXT rung's checkbox renders beneath it. This is the state
	# that retired #420 outright: a label cannot be selected-and-gated.
	# SOWABLE ground, not the reference patch. The claim below is that the ladder CONTINUES beneath a
	# done label — which needs a next rung that is genuinely on offer. `TileFx.tended_tile_fixture` is built
	# on the reference tile, whose `sow_site_refusal` is "too_dry", so Sow there can only ever be
	# gated: the assertion would be testing the gated shape while claiming to test the offered one.
	var tended_tile := ForageFx.sowable_tile_fixture()
	# `patch_`-PREFIXED, like every other key on a tile_info dict — the unprefixed spellings are the
	# RAW wire patch's, and setting those here leaves `improvement_is_done` reading nothing at all.
	tended_tile["patch_cultivation_progress"] = 1.0
	tended_tile["patch_is_cultivated"] = true
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture(
		int(tended_tile["x"]), int(tended_tile["y"]))
	h._hud._compose.reset_forage_source()
	h._show_tile(tended_tile)
	h._compose_forage(tended_tile)
	await h._settle()
	await h._save("improvement_done_plant")
	var done_label = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate")
	h._assert_hud("a finished Cultivate renders the DONE state",
		done_label is Label and String(done_label.get_meta(
			HudWidgets.IMPROVEMENT_STATE_META, "")) == HudWidgets.IMPROVEMENT_STATE_DONE)
	h._assert_hud("…naming the state the build left the patch in",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate").contains(
			String(HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"])))
	# The ladder CONTINUES: the next rung reads as OFFERED, which is what separates the done state from
	# a dead end. A gated next rung reads GATED instead — see `forage_sow_locked` — so this assertion
	# only means something on ground that will take seed.
	h._assert_hud("…and the NEXT rung is OFFERED beneath it",
		String(ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, "sow"))
			== HudWidgets.IMPROVEMENT_STATE_OFFERED)

	# State 442-offered-gated — the OFFERED state with an unmet prerequisite. A SOURCE-gated improvement
	# is SHOWN, UNCHECKED and EXPLAINED: discovering the rung exists and what it costs to unlock must
	# not require already having unlocked it.
	#
	# **THE FIXTURE FOLLOWED THE ONLY SURVIVING SOURCE GATE, twice, and neither move weakened it.** It
	# first staged a wild Thriving patch gated on KNOWLEDGE ALONE — a state this sheet now renders no
	# control for at all — and then a Stressed patch, whose ecology gate is retired above. What is left
	# on either web is **`Sow`'s site refusal**: `Tame` gates on knowledge alone, and `Corral`'s
	# ownership half is unreachable here (only the source's NEXT rung is offered, so a part-tamed herd
	# is offered Tame). So the fixture is a tended patch on ground that will never take seed, with Seed
	# Selection KNOWN — the source gate standing alone, which is exactly this frame's subject.
	# `forage_sow_locked` is the neighbouring case where BOTH kinds of reason are live at once.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.reset_forage_source()
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])
	h._show_tile(TileFx.tended_tile_fixture())
	h._compose_forage(TileFx.tended_tile_fixture())
	await h._settle()
	await h._save("improvement_offered_gated")
	var gated_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "sow")
	# **A GATED RUNG IS A LABEL, NOT A DISABLED CHECKBOX** — the control's SHAPE says whether this is a
	# choice or a fact, and an unmet prerequisite is a fact. The greyed-checkbox form this replaced put
	# an offer the player cannot accept ("Sow a field here · then 2.40 food …") directly above the
	# sentence explaining that they cannot accept it.
	h._assert_hud("a gated improvement is SHOWN, never hidden — the rung stays discoverable",
		gated_box != null)
	h._assert_hud("…in the GATED state rather than as an offer, because it is a fact and not a choice",
		String(gated_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
			== HudWidgets.IMPROVEMENT_STATE_GATED)
	# Matched WHOLE, not by needle: this reason is the one the GROUND raises, and a `contains` on a
	# fragment would still pass if the remedy clause (the half that says what to DO) went missing.
	h._assert_hud("…whose own text is the REASON, so nothing offers what cannot be taken",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_SOW)
			== HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_SOW),
				String(HudFloraVocab.SOW_REFUSAL_REASONS[String(
					TileFx.tended_tile_fixture()["patch_sow_site_refusal"])])])
	h._assert_hud("…and the offer wording is gone entirely, not merely greyed",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, GATED_OFFER_NEEDLE))
	# THE CROP LIST IS PART OF COMMITTING, so a refused commitment offers none. Shipped once with the
	# picker rendered under the disabled box: four live, clickable crop rows beneath a checkbox whose
	# own note read "Your people know Cultivation 0%" — the card refusing the act and inviting the
	# player to configure it in the same breath. The gate NOTE stays (it answers "why not?"); the
	# CONFIGURATION goes. Found in play, not by the harness, which is why the assertion exists now.
	# **NO CROP ANYWHERE, which since §4.7a ③ is true of every state** — the crop is the BUILD QUEUE
	# row's setting now, so this has stopped separating a gated control from an offered one and is
	# kept as the sheet-wide negative it has become.
	h._assert_hud("…and offers no crop anywhere, the crop being the queue row's setting",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, ForageFx.GATED_CROP_NEEDLE))

	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])

	# ---- THE THIRD METER STATE: BUILDING vs REVERTING (issue #442) ------------------------------
	# **"Preparing 99%" WAS THE MOST MISLEADING LINE ON THE CARD.** A meter that is bleeding back toward
	# wild wore the build's own word in the build's own neutral ink, so gaining and losing read
	# identically — the two differ only in which DIRECTION the number is moving, which a percentage
	# cannot show. Judged as an A/B on ONE patch at ONE meter value, because the claim is precisely that
	# the same number reads differently depending on whether a crew is on it: the only thing that moves
	# between these two frames is who the player band is working.
	var meter_tile := BaseFx.food_tile_fixture()
	meter_tile["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	# **THE TURN ESTIMATE MOVES WITH THE CREW, because the sim only has one while somebody is
	# working**: the building half is priced with a live estimate and the reverting half with
	# `BUILD_TURNS_NO_ESTIMATE`, which renders as no line rather than a `0 turns` that would promise a
	# build about to land on ground nobody is touching. The WORK PAIR is identical across both, which
	# is what keeps the A/B's own claim — the verb and the ink — the thing that differs.
	BaseFx.price_plant_build(meter_tile)
	#   (a) BUILDING — the band's own Cultivate assignment is on this tile. Neutral ink, build verb.
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_building")
	var building_row = h._hud.tile_detail.text
	# **WORD AND TINT IN ONE NEEDLE.** `detail_bbcode` renders a row's value as
	# `[color=#HEX]<value>[/color]`, so asserting the whole tinted cell pins both halves at once — and it
	# has to, because the old row was not merely mis-WORDED, it was mis-COLOURED: a bleeding meter wore
	# the neutral ink of a build one turn from done. A bare hex search would match any INK row on the
	# card; this one can only be satisfied by the cultivation value itself.
	h._assert_hud("a meter a crew IS building leads with the sim's turn count, in neutral ink",
		building_row.contains(_rung_value_markup(
			_building_value(BaseFx.BUILD_TURNS_REMAINING), HudStyle.INK_HEX)))
	#   (b) **HELD ON PURPOSE — the SAME patch at the SAME percentage with nobody building it, and it
	#   is NOT a failure** (`docs/plan_standing_upkeep.md` §4.6a). The band is working a different tile,
	#   so the patch is improved and unworked — and the band's keeping pool is covering it, which is
	#   what the wire's `-2` at zero builders says. Take the builders off a Cultivate at 96%, staff the
	#   keeping, and the meter stays there indefinitely: a decision, not a loss.
	#
	#   **THIS FRAME ASSERTED THE OPPOSITE UNTIL 6a**, as `⚠ Reverting 96%` in WARN ink — the card's
	#   own producer of *work banked + nobody on it ⇒ bleeding*, which held only while an unbuilt rung
	#   was billed to its build crew. The sim answers it now, and the half that really IS a loss is
	#   `tile_meter_rotting` below.
	BaseFx.price_plant_build(meter_tile, SourceForecast.BUILD_TURNS_HOLDS)
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture(
		METER_AWAY_TILE_X, int(meter_tile["y"]))
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_held")
	var reverting_row = h._hud.tile_detail.text
	print("ui_preview: meter rows  building=%s  reverting=%s" % [
		Readout.detail_excerpt(building_row, CULTIVATION_ROW_KEY),
		Readout.detail_excerpt(reverting_row, CULTIVATION_ROW_KEY)])
	# **NO MARK, NEUTRAL INK, AND WORDS THAT SAY IT IS PARKED** — the markup is the claim, so amber and
	# a hazard glyph are both failures here. With the `Keeping:` row retired the ABSENCE of a mark is
	# the only signal a rung is fine, so marking a deliberate hold would teach the player to ignore the
	# mark on the three states that need it.
	h._assert_hud("the SAME meter with nobody on it and its keeping covered reads as HELD, in neutral ink",
		reverting_row.contains(_rung_value_markup(_held_value(), HudStyle.INK_HEX)))
	# THE NEGATIVE THAT NAMES THE RETIRED ROW: the hazard mark must be nowhere on this rung's value.
	h._assert_hud("…and carries no hazard mark at all — parking a build is a decision, not a warning",
		not Readout.detail_excerpt(reverting_row, CULTIVATION_ROW_KEY).contains(
			HudSelectionVocab.RUNG_HAZARD_GLYPH))
	# THE NEGATIVE, with the positive above it as its companion (a whole-text search alone would also
	# pass on a card that rendered no cultivation row at all): a build under way states a COUNT, and
	# one nobody is working must state none — never `0 turns`, which reads as a build about to land on
	# ground that is going back to wild. The needle is the count's own punctuation rather than a
	# specific number, so a row rendering any other count fails it too.
	h._assert_hud("…and a meter nobody is building states NO turn count, not a zero",
		not reverting_row.contains(RUNG_TURNS_NEEDLE))
	# **THE HAZARD MARK IS THE POINT OF THE REDESIGN, AND ITS NEGATIVE IS ITS OTHER HALF** (issue
	# #545). With the `Keeping:` row retired, the ABSENCE of a mark is the only thing that says a rung
	# is fine — so a healthy build must carry none, or the mark means nothing on the three states that
	# do carry it.
	h._assert_hud("…while the build a crew IS advancing wears NO hazard mark at all",
		not Readout.detail_excerpt(building_row, CULTIVATION_ROW_KEY).contains(
			HudSelectionVocab.RUNG_HAZARD_GLYPH))
	# **THE FOUR HAZARD STATES, ASKED OF THE PRODUCER, because two of them render as a state no frame
	# in this chapter stages and every one of them must be marked.** A state that renders bare reads as
	# success, which is the defect the unstaffed build was; a producer-level claim is what makes the
	# rule checkable rather than sampled.
	h._assert_hud("every rung hazard carries its mark: unstarted · sliding · never · stalled",
		_hazard_states_all_marked())
	# **THE GEAR LINE'S NEGATIVE.** No plant item declares the build stat yet (issue #539 is the hoe),
	# so a plant build's contribution is honestly `0` and the row must not appear at all — a
	# `−0 work off this job` advertises a tool that did nothing. Its positive twin is the animal web's
	# (`herd_corral`), where the shipped handling gear really does take work off the job.
	h._assert_hud("a build no tool helps states NO gear line, not a zero one",
		not building_row.contains(HudSelectionVocab.BUILD_GEAR_WORK_ROW_FORMAT.split("%s")[0]))
	# **AND THE WORK ABSOLUTES ARE OFF THE CARD**, which is the other half of the one-row redesign:
	# `30 / 50 work` is what you read while COMPOSING a build, beside the stepper that moves it, and
	# it stays on the compose sheet. The needle is the unit word the meter format spells.
	h._assert_hud("…and the card states no work absolutes at all — those are the sheet's",
		not building_row.contains(BUILD_WORK_UNIT_NEEDLE))

	#   (c) HOLDING — **the third answer, and the one this card was silent about**
	#   (`docs/plan_standing_upkeep.md` §2.4). The SAME patch with the SAME crew ON it, whose
	#   staffing exactly pays the maintenance rate: the meter is not bleeding — somebody is
	#   working it, and their whole output goes on the rate — so what changes is the estimate.
	#   `-2` is *this staffing never finishes*: a standing fact about a commitment the player has
	#   already made, where `-1`'s states are a transient absence of information. Folded into one
	#   sentinel it rendered as NO LINE here — visible only to a compose sheet that redid the
	#   comparison itself — which is the reassuring direction on the one state that should stop them.
	BaseFx.price_plant_build(meter_tile, SourceForecast.BUILD_TURNS_HOLDS)
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_never")
	var never_row = h._hud.tile_detail.text
	print("ui_preview: meter holding  %s" % Readout.detail_excerpt(never_row, CULTIVATION_ROW_KEY))
	# **THE WORD AND THE INK, in one needle, exactly as the A/B above pins its two.** The indented
	# branch tints the WHOLE line, so the markup is the claim: silence and neutral ink are both
	# failures here, and only asserting the colour separates ∞-the-warning from the larder runway's
	# ∞, which draws the identical glyph for the opposite news.
	h._assert_hud("a committed crew AT the rate reads ∞ on the card's own row, in WARN ink",
		never_row.contains(_rung_value_markup(_holding_value(), HudStyle.WARN_HEX)))
	# …and the row still carries its METER, because the crew is ON this one. Without the percentage
	# the frame is satisfied by the unstarted state, which is a different fact with a different
	# remedy: nobody is there, against somebody is there and it is not enough.
	h._assert_hud("…and it still states how far the meter got — somebody IS on this one",
		never_row.contains("(%d%%)" % HudFormat.progress_percent(REVERTING_METER_PROGRESS)))

	#   (d) ROTTING — **the FOURTH answer, split out of `-2` and swallowed by this client for a
	#   release.** The same patch, the same crew ON it, one step further down: the staffing is UNDER
	#   the rate, so past the rung's grace the decay pass bleeds work the player has already bought.
	#   `SourceForecast.build_turns_remaining` accepted `-1` and `-2` and mapped every other negative
	#   to *no estimate*, so this state — a real, staffed, priced build actively LOSING banked work —
	#   rendered as no line at all, indistinguishable from a source nobody has touched. **It is the
	#   most common early staffing there is**: one builder on a Cultivate against `plant:tended`'s
	#   rate of 2.0 nets −1.
	#
	#   Judged against (c) rather than alone, because the two are one step apart and the whole claim
	#   is that they read DIFFERENTLY: same ∞, same mark, different words and different INK.
	BaseFx.price_plant_build(meter_tile, SourceForecast.BUILD_TURNS_ROTS)
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_rotting")
	var rotting_row = h._hud.tile_detail.text
	print("ui_preview: meter rotting  %s" % Readout.detail_excerpt(rotting_row, CULTIVATION_ROW_KEY))
	# **WORD AND INK IN ONE NEEDLE, and the INK is the half that was missing.** The row led with the
	# hazard mark either way, so a claim about the mark alone passes on a rotting build painted the
	# holding row's amber — which is the schema's promised red/yellow split existing on the wire and
	# nowhere on screen.
	h._assert_hud("a committed crew UNDER the rate reads ∞ AND *losing ground*, in DANGER ink",
		rotting_row.contains(_rung_value_markup(_rotting_value(), HudStyle.DANGER_HEX)))
	# **THE NEGATIVE THAT NAMES THE DEFECT.** Silence is what shipped, and a positive claim above
	# passes on a card that renders BOTH rows — so the holding row's exact value must be absent from
	# this frame, or the two sentinels are still being rendered as one answer.
	h._assert_hud("…and NOT the holding row's amber ∞ — the two sentinels are two answers",
		not rotting_row.contains(_rung_value_markup(_holding_value(), HudStyle.WARN_HEX)))
	# **THE UNRECOGNISED SENTINEL, ASKED OF THE PRODUCER.** A negative this client does not know is
	# *no answer*, never a guess — and with the row now leading on the count, "no answer" has to
	# render as the STALLED hazard rather than as a bare percentage, which is the silence the whole
	# redesign exists to remove. `-5` stands for whatever the wire grows next; it was `-3`, then `-4`,
	# each until the wire spelled that value — at which point this claim was pinning a bug rather than
	# the rule.
	h._assert_hud("…and an unrecognised negative is marked as STALLED, never rendered bare",
		_rung_value_for_turns(UNKNOWN_BUILD_TURNS_SENTINEL)
			== HudSelectionVocab.RUNG_STALLED_FORMAT % [
				HudSelectionVocab.RUNG_HAZARD_GLYPH,
				HudFormat.progress_percent(REVERTING_METER_PROGRESS)])

	#   (d2) **BLOCKED — the FIFTH answer, and the one whose remedy is on a different line**
	#   (`docs/plan_standing_upkeep.md` §4.6b). The band's builders are STAFFED and standing on this
	#   entry, its own rung's gate refuses it, so nothing banks and nothing queued behind it moves
	#   either. It is neither of the two `∞` states beside it — those are answers about a crew's
	#   arithmetic, and MORE builders is their remedy — so it wears the hazard mark and no `∞` at all.
	#
	#   **THE PAIRING IS THE CLAIM, twice over.** Judged against (d) so the two hazards are seen to
	#   read differently, AND against the shortfall row beneath it: the build surface does not know
	#   WHY the gate refuses, so what it renders is the state plus the two facts the same row already
	#   publishes — `upkeepShortfall` and `neglectGraceRemaining` — with the remedy naming the keeping
	#   ROLE. A frame that stated the block and invented a cause would be worse than one that said
	#   nothing.
	var blocked := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	blocked["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	BaseFx.price_plant_build(blocked, SourceForecast.BUILD_TURNS_QUEUE_BLOCKED)
	# **THE KEEPING IS SHORT, which is what the remedy sub-row is PAIRED with.** The published
	# shortfall and its grace are the two facts already on this row; the block itself states no cause.
	# **AND THE SIM STATES WHY** (`buildBlockedReason`). Without it the card can only render the
	# hazard row, which is the state that shipped: the player covered the keeping the sub-row
	# named, and the block stayed with nothing on any surface saying what was refusing it.
	blocked["patch_build_blocked_reason"] = HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT
	blocked["patch_upkeep_demand"] = BLOCKED_UPKEEP_DEMAND
	blocked["patch_upkeep_supplied"] = BLOCKED_UPKEEP_SUPPLIED
	blocked["patch_upkeep_shortfall"] = BLOCKED_UPKEEP_DEMAND - BLOCKED_UPKEEP_SUPPLIED
	blocked["patch_has_neglect_grace"] = true
	blocked["patch_neglect_grace_remaining"] = BLOCKED_GRACE_TURNS
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture(
		int(blocked["x"]), int(blocked["y"]))
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(blocked)
	await h._settle()
	await h._save("tile_meter_blocked")
	var blocked_row = h._hud.tile_detail.text
	print("ui_preview: meter blocked  %s" % Readout.detail_excerpt(blocked_row, CULTIVATION_ROW_KEY))
	h._assert_hud("a queue standing on a refusing gate reads BLOCKED, marked, in WARN ink",
		blocked_row.contains(_rung_value_markup(_blocked_value(), HudStyle.WARN_HEX)))
	# **THE NEGATIVE THAT KEEPS THE FIVE ANSWERS FIVE.** A client that had not followed the fourth
	# sentinel maps it to *no answer* and renders the STALLED hazard — a plausible-looking marked row
	# naming the wrong state and the wrong remedy — so that value must be absent from this frame.
	h._assert_hud("…and NOT the STALLED hazard an unfollowed sentinel would render",
		not blocked_row.contains(_rung_value_for_turns(SourceForecast.BUILD_TURNS_NO_ESTIMATE)))
	# **AND THE CAUSE IS BENEATH IT, which is the row that did not exist.** The sim names the
	# conjunct that refused and the client words it; asserted by EQUALITY against the shipped
	# table, since the whole claim is WHICH sentence renders.
	h._assert_hud("…and the row beneath states the sim's own cause for the block",
		blocked_row.contains(String(HudSelectionVocab.BUILD_BLOCKED_REASONS[
			HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT])))
	# **AND THE REMEDY IS BESIDE IT, naming the KEEPING role rather than the builders.** The measured
	# escape is `assign_labor <f> <b> husbandry <n>` alone — staffing the keeping restores the
	# source's regrowth and it climbs back over its own gate — so the sub-row must name the web's
	# keeping role and must NOT hedge with a second half about the take crew.
	h._assert_hud("…and the row beneath names the keeping role that frees it",
		blocked_row.contains(HudSelectionVocab.RUNG_BLOCKED_REMEDY_FORMAT
			% HudWorkVocab.ROLE_NAME_AGRICULTURE))
	# …and the `At risk:` row still states what the shortfall COSTS, which is what makes the remedy
	# above an instruction rather than an unquantified nudge.
	h._assert_hud("…over the At risk: row the shortfall itself earns",
		blocked_row.contains(DetailFormat.UPKEEP_RISK_ROW))
	# **THE PAIRED NEGATIVES, all PNG-less — the frames differ by one sub-row apiece.**
	#
	#   (1) A BLOCKED source whose keeping is PAID states its CAUSE and NOT the keeping remedy. The
	#   keeping is a lever on the escapement stall and is not why any other gate refuses, so a card
	#   that named it whenever a build was stuck would send the player to staff a role that changes
	#   nothing — which is the failure this whole row was written for, pointed the other way.
	var blocked_paid := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	blocked_paid["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	blocked_paid["patch_build_blocked_reason"] = HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT
	BaseFx.price_plant_build(blocked_paid, SourceForecast.BUILD_TURNS_QUEUE_BLOCKED)
	var paid_lines := DetailFormat.build_blocked_lines(blocked_paid,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE)
	h._assert_hud("a blocked source whose keeping is PAID still states the cause (%d lines)"
			% paid_lines.size(), paid_lines.size() == BLOCKED_CAUSE_ONLY_LINES
		and String(paid_lines[0]).contains(String(HudSelectionVocab.BUILD_BLOCKED_REASONS[
			HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT])))
	h._assert_hud("…and states no keeping remedy, the keeping being paid",
		not String(paid_lines[0]).contains(HudSelectionVocab.RUNG_BLOCKED_REMEDY_FORMAT
			% HudWorkVocab.ROLE_NAME_AGRICULTURE))
	#   (2) A SECOND KEY, by EQUALITY. A table answering one sentence for every cause passes the
	#   escapement claim above on its own, so the knowledge refusal is asserted to render ITS words
	#   and not the escapement ones.
	var blocked_knowledge := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	blocked_knowledge["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	blocked_knowledge["patch_build_blocked_reason"] = BLOCKED_KNOWLEDGE_REASON
	BaseFx.price_plant_build(blocked_knowledge, SourceForecast.BUILD_TURNS_QUEUE_BLOCKED)
	var knowledge_lines := DetailFormat.build_blocked_lines(blocked_knowledge,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE)
	h._assert_hud("a build blocked on KNOWLEDGE states the knowledge cause (%s)"
			% str(knowledge_lines), knowledge_lines.size() == BLOCKED_CAUSE_ONLY_LINES
		and String(knowledge_lines[0]).contains(
			String(HudSelectionVocab.BUILD_BLOCKED_REASONS[BLOCKED_KNOWLEDGE_REASON])))
	h._assert_hud("…and NOT the escapement one, which is a different remedy entirely",
		not String(knowledge_lines[0]).contains(String(HudSelectionVocab.BUILD_BLOCKED_REASONS[
			HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT])))
	#   (3) A source that is NOT blocked states NO cause at all. A producer that always emitted a
	#   line passes every positive above, so this is the half that makes them worth anything.
	var building := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	building["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	building["patch_build_blocked_reason"] = ""
	BaseFx.price_plant_build(building, BaseFx.BUILD_TURNS_REMAINING)
	h._assert_hud("a source that is NOT blocked states no cause line at all",
		DetailFormat.build_blocked_lines(building,
			HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.SOURCE_KIND_FORAGE).is_empty())

	#   (e) **STALLED — and the frame exists to pin that the SHEET and the CARD agree about it**
	#   (§4.6a). A half-built Cultivate on a patch drawn to or below its own floor, with nobody on it:
	#   `RungDef::build_accrual`'s `eligible` carries `crew_is_working_the_source`, which reads the
	#   STOCK against the floor and takes **no crew count at all**, so the sim answers `-1` at any
	#   staffing and the card renders `⚠ Stalled`.
	#
	#   **THE CLIENT GATED THAT PREDICATE ON A CREW FOR ONE PASS**, reasoning that nothing accrues at
	#   zero builders anyway — true, and not this predicate's question. The gate made the sheet answer
	#   the neutral `held` where the card says `⚠ Stalled`: **two producers disagreeing about one
	#   meter**, which is the exact thing the closed-form equality exists to prevent.
	var starved := TileFx.stressed_tile_fixture()
	starved["patch_cultivation_progress"] = REVERTING_METER_PROGRESS
	BaseFx.price_plant_build(starved, SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture(
		METER_AWAY_TILE_X, int(starved["y"]))
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(starved)
	await h._settle()
	await h._save("tile_meter_stalled")
	var starved_row = h._hud.tile_detail.text
	print("ui_preview: meter stalled  %s" % Readout.detail_excerpt(starved_row, CULTIVATION_ROW_KEY))
	# THE PRECONDITION, or this frame is about a patch with room to work in: the food peak really does
	# stand above this patch's own stock, which is what makes the predicate refuse.
	h._assert_hud("the starved patch really has NO room above the floor — the predicate must refuse",
		SourceForecast.escapement_room(starved, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.FLOOR_FOOD_PEAK) <= SourceForecast.BUILD_NO_ESCAPEMENT_ROOM)
	h._assert_hud("…and the CARD states the sim's own STALLED hazard, never a bare percentage",
		starved_row.contains(_rung_value_markup(
			_rung_value_for_turns(SourceForecast.BUILD_TURNS_NO_ESTIMATE), HudStyle.WARN_HEX)))
	# **THE EQUALITY, at the committed crew of ZERO.** The sheet prices a proposal and the card reads
	# the sim, and on this source they have to answer the same thing — which they did not while the
	# predicate was gated on a crew.
	h._assert_hud("…and the SHEET answers no estimate for the same crew, as the sim does",
		SourceForecast.build_turns_at(starved, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE, SourceForecast.BUILD_CREW_NONE,
			SourceForecast.FLOOR_FOOD_PEAK, NO_BUILD_GEAR)
			== SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	# …and the NEGATIVE that names the defect: `held` is what the crew-gated form answered here, and it
	# is the reassuring direction on a build that is going nowhere.
	h._assert_hud("…and specifically NOT the neutral *held* the crew-gated predicate produced",
		SourceForecast.build_turns_at(starved, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE, SourceForecast.BUILD_CREW_NONE,
			SourceForecast.FLOOR_FOOD_PEAK, NO_BUILD_GEAR)
			!= SourceForecast.BUILD_TURNS_HOLDS)

	# ---- THE TURN ESTIMATE FOLLOWS THE STEPPER (docs/plan_unit_costed_work.md §11) ----------------
	# **ONE PATCH, ONE FLOOR, TWO CREWS — and a frame set that renders only one crew proves nothing
	# here.** The sheet read the sim's `buildTurnsRemaining` for a release, which is its answer for the
	# crew ALREADY on the source: the reported symptom was `Cultivating 0 / 50 work (0%) — ≈32 turns`
	# holding still as the forager stepper went 1 → 3, on the one panel where that number is the whole
	# decision. The A/B is the only shape that can tell a moving estimate from a frozen one.
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._band_labor._player_bands = [BandFx.cultivating_forage_band_fixture()]
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	# **THE STEPPER THAT MOVES THE ESTIMATE IS THE BUILD'S** (`docs/plan_standing_upkeep.md` §2.2).
	# The take crew no longer prices a build at all — it was never doing the work — so the A/B is run
	# on the improvement control's own crew row, which is where the player now answers *how many hands
	# on this job*. The take crew is held at a plain 1 throughout, so nothing but the builders moves.
	h._hud._compose.set_forage_count(1)
	# **THE PATCH IS SHORT-KEPT, WHICH IS WHAT MAKES EITHER `∞` REACHABLE AT ALL** — see
	# `_short_kept_food_tile`. The reference patch itself rots at nothing, and on a kept meter every
	# staffed builder climbs.
	var short_kept := _short_kept_food_tile()
	h._show_tile(short_kept)
	h._compose_forage(short_kept)
	# Dialled AFTER the first open, for the reason `_compose_herd`'s docstring gives: the source change
	# re-seeds the composition, so a build crew set before it is silently thrown away.
	BandFx.staff_builders(h._hud._band_labor, TURNS_LONE_CREW)
	h._compose_forage(short_kept)
	await h._settle()
	await h._save("improvement_turns_lone_crew")
	var lone_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	# Captured off the LONE-crew sheet, because the next line recomposes it and frees these nodes.
	var _face_stops_at_lone_crew := ForageFx.improvement_face_stops(
		h._hud._drawercompose._compose_sheet, "cultivate")
	# The face's INK, as a colour rather than a bool: the pace has three states and the two `∞` ones
	# would read alike through a warned/not-warned reader.
	var _face_ink_at_lone_crew := ForageFx.improvement_face_color(
		h._hud._drawercompose._compose_sheet, "cultivate")
	BandFx.staff_builders(h._hud._band_labor, TURNS_FULL_CREW)
	h._compose_forage(short_kept)
	await h._settle()
	await h._save("improvement_turns_full_crew")
	var full_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	var _face_stopped_at_full_crew := ForageFx.improvement_face_stops(
		h._hud._drawercompose._compose_sheet, "cultivate")
	# Captured before the drag, so the drag frame's negative compares against a rendered reading rather
	# than a recomposition.
	var full_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build turns  lone=%s  full=%s" % [lone_face, full_face])
	# **EQUALITY ON THE WHOLE CLAUSE, and the counts are derived in this chapter rather than through
	# the producer under test** — an expectation composed from `build_turns_at` could only agree with
	# itself. `ends_with` because the clause closes the face the meter opens.
	#
	# **A LONE BUILDER ON A BLEEDING METER IS SLOW, NOT DOOMED — and that is the shipped plant web**
	# (§4.6a). No plant rung bleeds faster than one worker banks, so one hand nets `1 − 0.5` and the
	# sheet quotes a real, long count; the `∞` lives at ZERO builders, asserted below.
	h._assert_hud("one hand outruns the bleed and is quoted a real count — slow, not doomed",
		lone_face.ends_with(_turns_clause(TURNS_AT_LONE_CREW)))
	h._assert_hud("…and it is NOT inked as a stop, the meter being ahead of its own rot",
		not _face_stops_at_lone_crew)
	h._assert_hud("…while four hands outrun it far faster — the estimate moves with the stepper",
		full_face.ends_with(_turns_clause(TURNS_AT_FULL_CREW)))
	# The PAIR's other half: the crew that finishes SOONER must not be wearing a warning either, or the
	# ink is decoration rather than a verdict.
	h._assert_hud("…and that face is NOT warned — the ink is a verdict, not the control's livery",
		not _face_stopped_at_full_crew)
	# **AND THE BUILDERS ROW STATES NO THRESHOLD AT ALL, WHICH IS THE RETIREMENT AS A CLAIM**
	# (`docs/plan_standing_upkeep.md` §2.4). The row named the rung's rate as the work a builder had to
	# beat — first as a note, then as that label's tooltip — and the keeping pool owes that rate at
	# every fullness now, so there is no bar to clear and the smallest useful build crew is one hand.
	# **The absence is asserted on both sides of the A/B**, or a row that states a threshold only for
	# the crew that clears it passes here.
	h._assert_hud("the BUILDERS row states no threshold — the rate is not a bar a builder clears",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, RETIRED_BUILD_FLOOR_NEEDLE))
	# **THE PACE IS A COLOUR, AND THE PAIR IS THE CLAIM.** Both these crews outrun the bleed, so both
	# lines are GREEN; the red one is the zero-builder reading below. A face pinned to either ink
	# passes one half and fails the other, which is what makes this a verdict rather than livery.
	h._assert_hud("both climbing build lines are green, whatever their count",
		_face_ink_at_lone_crew == HudStyle.HEALTHY
			and ForageFx.improvement_face_color(h._hud._drawercompose._compose_sheet,
				"cultivate") == HudStyle.HEALTHY)
	# **…AND THE STEPPER AT ZERO IS WHERE THE `∞` LIVES, reached at a SHIPPED rate** (§4.6a). PNG-less,
	# the same sheet one tick down: with nobody on it the whole 0.5 bleed is the net, so the meter goes
	# BACKWARDS and the face states `∞` in the LOSING red. **The boundary is *is there work banked*,
	# not *is anyone staffed*** — this proposal used to answer *no estimate* and render as a silence on
	# the one state that should stop the player.
	BandFx.staff_builders(h._hud._band_labor, SourceForecast.BUILD_CREW_NONE)
	h._compose_forage(short_kept)
	await h._settle()
	var empty_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: build turns  none=%s" % empty_face)
	h._assert_hud("nobody on a bleeding meter reads ∞, not a silence",
		empty_face.ends_with(_never_clause()))
	h._assert_hud("…in the LOSING red, because work already bought is going back",
		ForageFx.improvement_face_color(h._hud._drawercompose._compose_sheet,
			"cultivate") == HudStyle.DANGER)
	# **AND THE SAME STAFFING ON A KEPT METER IS *HELD*, WHICH IS THE HALF THAT IS NOT A FAILURE.**
	# The reference patch is the same tile at the same coordinates with its keeping covered, so the
	# composition survives the swap and the ONLY thing that moves is the shortfall — the sharpest shape
	# this pair has, since the crew, the meter and the stepper are all held still.
	h._compose_forage(_seeded_food_tile())
	await h._settle()
	var held_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: build turns  parked=%s" % held_face)
	# **THE FACE SAYS *held*, NOT `∞`** (§4.6a). That glyph is the larder runway's, shared on the
	# strength of a player learning a mark once and reading it everywhere — so spending it where
	# nothing is wrong teaches that it sometimes means nothing is wrong, which costs the two states
	# where it means a great deal. The card says the same word about the same state, which is what
	# makes the two producers trustworthy.
	h._assert_hud("a build PARKED on a kept meter reads *held*, in words rather than the arc's ∞",
		held_face.ends_with(_held_clause())
			and not held_face.contains(DetailFormat.BUILD_TURNS_NEVER_GLYPH))
	h._assert_hud("…in neutral ink and stopping nothing, because nothing is being lost",
		not ForageFx.improvement_face_stops(h._hud._drawercompose._compose_sheet, "cultivate"))
	# …and the PACE behind both, asked of the producer: the two states share ONE wire sentinel and only
	# the crew tells them apart, so the pair is what closes the fork rather than samples it.
	h._assert_hud("…while a PARKED build whose keeping is covered is held, not losing",
		SourceForecast.build_pace(SourceForecast.BUILD_TURNS_HOLDS,
			SourceForecast.BUILD_CREW_NONE) == SourceForecast.BUILD_PACE_HELD
		and not HudWidgets.improvement_pace_stops(SourceForecast.BUILD_PACE_HELD))
	h._assert_hud("…and a CREW banking exactly that much is still warned — its turn is being wasted",
		SourceForecast.build_pace(SourceForecast.BUILD_TURNS_HOLDS,
			TURNS_LONE_CREW) == SourceForecast.BUILD_PACE_HOLDING
		and HudWidgets.improvement_pace_stops(SourceForecast.BUILD_PACE_HOLDING)
		and DetailFormat.build_turns_clause(SourceForecast.BUILD_TURNS_HOLDS,
			TURNS_LONE_CREW).contains(DetailFormat.BUILD_TURNS_NEVER_GLYPH))
	# Back to the full crew for the floor-drag frame below, which is about the FLOOR and nothing else.
	BandFx.staff_builders(h._hud._band_labor, TURNS_FULL_CREW)
	h._compose_forage(short_kept)
	await h._settle()
	# The NEGATIVE that names the defect. Both faces reading the same number is exactly what a sheet
	# quoting the sim's committed-crew answer renders, and it satisfies neither claim above only
	# because those spell two different counts — this says so directly, and names the frozen value.
	h._assert_hud("…so the two crews cannot both read the sim's own committed-crew answer",
		lone_face != full_face and not full_face.contains(_turns_clause(BaseFx.BUILD_TURNS_REMAINING)))
	# **AND A LIVE FLOOR DRAG NO LONGER MOVES IT, which is the retirement stated as a rendered claim**
	# (`docs/plan_standing_upkeep.md` §2.2). The floor used to SCALE the build rate
	# (`learn_multiplier`), so dragging toward Learning quoted a faster build — *a crew pulling hard on
	# the source it is improving builds slowly*. **With separate crews the build crew is not pulling
	# anything**, so the rate is the builders' plain output and the estimate is floor-INDEPENDENT
	# wherever there is room to work in at all. What the floor still decides is the WORK PREDICATE, and
	# `improvement_stressed_advances` above is where that is pinned.
	#
	# The drag is driven the way this harness drives every other one — `floor_changed` with
	# `committed = false`, the signal the chart emits while the pointer is still down — and the frame
	# is what a gesture looks like mid-flight: the dial has moved and the sheet has not been rebuilt
	# around it, which is what the live-refresh registry buys.
	var live_chart = Q.find_meta_node(h._hud._drawercompose._compose_sheet,
		HudWidgets.FLOOR_CHART_META)
	h._assert_hud("the forage sheet mounts a floor chart to drag at all",
		live_chart != null)
	live_chart.emit_signal("floor_changed", TURNS_DRAG_FLOOR, false)
	await h._settle()
	await h._save("improvement_turns_learning_floor")
	var dragged_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	h._assert_hud("the same builders read the same estimate at a deeper floor — the floor paces no build now",
		dragged_face.ends_with(_turns_clause(TURNS_AT_FULL_CREW)))
	# The NEGATIVE that keeps it a claim about the DRAG rather than about a frozen box: the dial really
	# did move, and the sheet really did re-read it — the take beside the estimate follows the floor,
	# which is what a sheet ignoring the drag entirely could not produce.
	h._assert_hud("…while the drag really did land, the take under it having moved with the floor",
		TURNS_DRAG_FLOOR != SourceForecast.FLOOR_FOOD_PEAK
			and Readout.yields_text(h._hud._drawercompose._compose_sheet) != full_yields)
	# **THE DEGENERATE CREW, asked of the producer** — no frame can stage it, a crew of 0 on a tile
	# this band works being an UNASSIGN, which offers no improvement control at all. A `0` crew makes
	# the per-turn work zero, and the honest answer to `remaining / 0` is no clause rather than a huge
	# number or an infinity.
	h._assert_hud("a crew of nobody on a rung with NOTHING BANKED states no estimate, never a number",
		SourceForecast.build_turns_at(BaseFx.unbuilt(BaseFx.food_tile_fixture()),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
			SourceForecast.BUILD_CREW_NONE, SourceForecast.FLOOR_FOOD_PEAK,
			NO_BUILD_GEAR) == SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	# **…AND THE BOUNDARY IS *IS THERE WORK BANKED*, NOT *IS ANYONE STAFFED*** (§4.6a). A proposal of
	# nobody on a meter that already carries work is a real question with a real answer — the meter is
	# HELD where the keeping covers it — and it is this same form at zero, which is exactly what the sim
	# publishes for zero builders. Asserted as the PAIR, or "always no estimate" passes the claim above
	# on its own.
	h._assert_hud("…while a crew of nobody on a meter with work on it answers HELD, as the sim does",
		SourceForecast.build_turns_at(_seeded_food_tile(),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
			SourceForecast.BUILD_CREW_NONE, SourceForecast.FLOOR_FOOD_PEAK,
			NO_BUILD_GEAR) == SourceForecast.BUILD_TURNS_HOLDS)
	# **THE SINGULAR, asked of the producer** — no fixture in the corpus lands a one-turn job, and
	# staging one would pin a whole frame's arithmetic to a rounding. Both compose faces spell their
	# count through `DetailFormat.build_turns_clause`, so the pair is asked of it: a build one turn out
	# must not read `≈1 turns` on the sheet beside the tile card's own `≈1 turn at this crew`. The
	# PLURAL half is what stops "always singular" passing.
	h._assert_hud("a one-turn job reads in the singular on the compose face",
		DetailFormat.build_turns_clause(TURNS_SINGULAR_JOB)
			== HudComposeVocab.BUILD_TURNS_COUNT_ONE)
	h._assert_hud("…and every other count keeps the plural",
		DetailFormat.build_turns_clause(TURNS_PLURAL_JOB)
			== HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % TURNS_PLURAL_JOB)

	# Restore the unassigned near band + a plain Sustain compose for the range states below.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_count(1)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)

	_arrow_is_gated_on_what_is_SHOWN()
	await _a_rung_that_slipped_is_building_again()
	await _a_reopened_sheet_shows_the_LIVE_crew()
	await _a_declared_build_with_no_builders_says_so()
	await _an_unstarted_rung_is_priced_at_its_own_rate()
	await _both_live_meters_get_their_own_row()
	await _a_band_with_no_free_hands_is_offered_a_dead_box()

## **RETIRED — `compose_pool_take_full` / `compose_pool_take_freed`, the shared-pool pair**
## (`docs/plan_standing_upkeep.md` §2.5). They staged a band with every hand committed and asserted
## that dropping the TAKE stepper inside the sheet immediately freed hands for the BUILDERS stepper
## beside it — a claim about two steppers sharing one source pool. A verb states no crew now, so the
## sheet has one stepper and there is no second ceiling for the first to give way to;
## `source_crew_pool_forage` is a plain per-activity ceiling again.
##
## **WHAT SURVIVES OF IT IS ONE HALF, AND IT IS ASSERTED ELSEWHERE**: a fully-allocated band must
## still be able to restate the crew it already has, which is what the standing term in that pool is
## for and what `forage_reopened_crews` holds.

## **AN UNCOMMITTED EDIT MUST NOT OUTLIVE ITS SHEET.** Reported from play beside the pool bug: drop
## HUNTERS 4 → 2, close WITHOUT committing, reopen — and the sheet still showed 2 over a band that
## still had 4 on the herd. The composition was keyed on the source and the close only ever dropped
## *which sheet is open*, so the dialled number survived as a promise nothing was keeping.
##
## **THE CLOSE IS DRIVEN THROUGH THE REAL PATH** (`close_compose_sheet` → the sheet's `closed` →
## `_on_compose_sheet_closed`), because the reset rides that signal; poking `ComposeState` here would
## assert the harness's own write.
##
## The PAIR is the claim: the edit must be visible while the sheet is open, or "it shows 4 on reopen"
## passes on a sheet that never took the edit at all.
func _a_reopened_sheet_shows_the_LIVE_crew() -> void:
	var tile := BaseFx.food_tile_fixture()
	var band := _fully_committed_forage_band(tile, POOL_TAKE_CREW, SourceForecast.BUILD_CREW_NONE)
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	h._hud._compose.set_forage_count(POOL_REDUCED_TAKE)
	h._compose_forage(tile)
	await h._settle()
	h._assert_hud("the uncommitted edit really is on the open sheet — %d" % POOL_REDUCED_TAKE,
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == POOL_REDUCED_TAKE)
	h._hud._drawercompose.close_compose_sheet()
	h._compose_forage(tile)
	await h._settle()
	await h._save("compose_reopen_reseeds")
	h._assert_hud("…and reopening seeds from the band's own row again — %d" % POOL_TAKE_CREW,
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == POOL_TAKE_CREW)

## **A DECLARED BUILD WITH NOBODY ON IT LOOKED EXACTLY LIKE ONE THAT HAD JUST STARTED.** The sim
## publishes `buildTurnsRemaining = -1` for an unstaffed source — correctly, nobody having promised
## anything there — and every meter surface renders `-1` as no line, so the tile card printed no rung
## row at all (its rows are gated on `progress > 0`), the sheet quoted `Cultivating 0 / 50 work (0%)`
## and stopped, and the map drew a `0%` plate over it.
##
## **THE FRAME CARRIES THE TILE CARD AND THE SHEET; THE BADGE IS DRIVEN BESIDE IT.** A plate is drawn
## to a canvas and no assertion can read a glyph back off one, so the map half asks
## `SourceForecast.unstaffed_build_of` — the fork `BandOverlayRenderer._queue_source_badge` reads —
## for both of its answers, plus the staffed control that stops "always warn" passing.
func _a_declared_build_with_no_builders_says_so() -> void:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_cultivation_progress"] = POOL_UNSTARTED_METER
	BaseFx.price_plant_build(tile, SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	var band := _fully_committed_forage_band(tile, POOL_TAKE_CREW, SourceForecast.BUILD_CREW_NONE)
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	h._compose_forage(tile)
	await h._settle()
	await h._save("tile_build_unstaffed")
	# **WORD AND TINT IN ONE NEEDLE**, the treatment the building/reverting A/B takes: the row was not
	# merely missing, and a version of it in the build's own neutral ink would be the same lie.
	h._assert_hud("a declared build with no builders states its rung row at a meter of ZERO",
		h._hud.tile_detail.text.contains("[color=#%s]%s[/color]" % [
			HudStyle.WARN_HEX, DetailFormat.BUILD_UNSTARTED_VALUE]))
	h._assert_hud("…and the compose sheet says the same thing in its own register",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudComposeVocab.BUILD_UNSTARTED_NOTE))
	# **AND IT IS TOLD FROM AN OFFER IN WORDS.** The declaration used to render as RUNNING — a meter at
	# 0% with no way back off it, reported from play — and then as a TICKED checkbox, whose tick was
	# the only thing separating a queued rung from an unqueued one. §4.7a ① retired the box, so the
	# distinction is the `◷ Queued` clause on a face the OFFER composes identically; the two claims
	# are the STATE and that clause, on a band with no free hands.
	var declared_box = ForageFx.find_improvement_control(
		h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	h._assert_hud("a declaration nobody is building reads as DECLARED, not as work in flight",
		declared_box != null
			and String(declared_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_DECLARED)
	h._assert_hud("…and says so in words, since its face is otherwise the OFFER's own",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CULTIVATE).contains(HudComposeVocab.BUILD_QUEUED_CLAUSE)
			and h._hud._band_labor.effective_idle(band) == 0)
	# THE NEGATIVE that names the defect: the word a rung nobody is building must not wear anywhere on
	# the card, in any ink — that word is what read as work in progress.
	h._assert_hud("…and never a turn count, which is what a started build states and this is not",
		not h._hud.tile_detail.text.contains(RUNG_TURNS_NEEDLE))
	# THE MAP's own fork, asked directly — all three answers, or "always warn" passes the first one.
	h._assert_hud("the map badge reads an unstarted rung as NOT WORKING",
		SourceForecast.unstaffed_build_of(POOL_UNSTARTED_METER, SourceForecast.BUILD_CREW_NONE)
			== SourceForecast.BUILD_UNSTAFFED_UNSTARTED)
	# **AND A METER WITH WORK BANKED IS NO LONGER THIS FORK'S BUSINESS** (§4.6a). It answered
	# `BUILD_UNSTAFFED_SLIDING` — *nobody on it, so it must be bleeding* — an inference that the pooled
	# keeping made wrong half the time: a parked build whose keeping is covered simply holds. The wire
	# answers it (`-2` / `-3`) and `build_is_losing` is what the plate reads, so this returns the
	# no-warning state and the badge keeps its honest percentage.
	h._assert_hud("…a meter with work banked and nobody on it as NOT this fork's warning",
		SourceForecast.unstaffed_build_of(REVERTING_METER_PROGRESS, SourceForecast.BUILD_CREW_NONE)
			== SourceForecast.BUILD_STAFFED)
	h._assert_hud("…and a staffed one as neither, so the plate keeps its percentage",
		not SourceForecast.build_is_unstaffed(SourceForecast.unstaffed_build_of(
			POOL_UNSTARTED_METER, BandFx.CULTIVATING_BAND_BUILDERS)))
	# **THE PLATE'S OTHER FACE IS THE WIRE'S VERDICT, asked of the producer** — a losing meter drops
	# its percentage (a falling number is the same lie as a frozen one) and a HELD one keeps it. The
	# pair is the claim: a `build_is_losing` hard-wired either way satisfies one half and fails the
	# other.
	h._assert_hud("the badge reads a rotting meter as LOSING, whoever is on it",
		SourceForecast.build_is_losing({
			SourceForecast.FORECAST_BUILD_TURNS_KEY: SourceForecast.BUILD_TURNS_ROTS,
		}, SourceForecast.BUILD_CREW_NONE))
	h._assert_hud("…and a meter merely PARKED as not losing, so its percentage is honest and stays",
		not SourceForecast.build_is_losing({
			SourceForecast.FORECAST_BUILD_TURNS_KEY: SourceForecast.BUILD_TURNS_HOLDS,
		}, SourceForecast.BUILD_CREW_NONE))
	# **THE ANIMAL WEB'S OWN ROW, driven** — `herd_summary_lines` is pure, and no herd fixture in this
	# chapter is on screen, so the pair is asked of the producer. It is a PAIR because a Husbandry row
	# rendered unconditionally would satisfy the positive on its own: at a meter of zero with nothing
	# declared, that row must still be absent.
	var untamed := HerdFx.taming_herd_fixture()
	untamed["domestication"] = POOL_UNSTARTED_METER
	var promised := "\n".join(DetailFormat.herd_summary_lines(untamed,
		h._hud._band_labor.world_herds(), SourceForecast.IMPROVEMENT_TAME))
	var untouched := "\n".join(DetailFormat.herd_summary_lines(untamed,
		h._hud._band_labor.world_herds(), SourceForecast.IMPROVEMENT_NONE))
	h._assert_hud("the herd drawer states a Tame promised with no keepers on it",
		promised.contains(DetailFormat.BUILD_UNSTARTED_VALUE))
	h._assert_hud("…and says nothing at all about a herd nobody has promised anything",
		not untouched.contains(DetailFormat.BUILD_UNSTARTED_VALUE))

## **A BAND WITH EVERY HAND ON ONE PATCH** — `idle_workers == 0`, the take and the build stated
## explicitly, which is the only shape in which the two steppers' shared pool is observable at all.
func _fully_committed_forage_band(tile: Dictionary, take: int, builders: int) -> Dictionary:
	var band: Dictionary = BandFx.forage_range_bands()[0].duplicate(true)
	band["idle_workers"] = 0
	band["working_age"] = take + builders
	band["labor_assignments"] = [{
		"kind": "forage", "workers": take,
		"target_x": int(tile["x"]), "target_y": int(tile["y"]),
		"floor": SourceForecast.FLOOR_FOOD_PEAK,
		"improvement": SourceForecast.IMPROVEMENT_CULTIVATE,
		"workers_needed": ForageFx.CULTIVATE_SIM_WORKERS_NEEDED, "overdraws": false,
	}, BandFx.builders_role_row(builders)]
	return band

## **THE REPAIR — a completed rung whose meter has eroded, BUILDING again with nothing declared**
## (`docs/plan_standing_upkeep.md` §2.4). It is the state the derivation exists for, and the one no
## stored verb could reach: completion freed the declaration, so a patch that fell below its cost
## re-entered the building state with nothing set, accrued nothing, and could not be repaired until
## the player re-issued `cultivate` — an intent they had never withdrawn.
##
## **THE FIXTURE IS THE WHOLE ARGUMENT, so it is built rather than borrowed**: a patch that is
## STILL TENDED (`patch_is_cultivated` true, the rung standing above its retention bar) with a
## meter BELOW its cost, worked by a band whose assignment declares no improvement at all. Nothing
## in this state is a command anybody issued.
##
## **THE TWO FACTS ARE ORTHOGONAL AND THE FRAME ASSERTS BOTH.** Fullness decides who pays the rate
## and is what puts the control in its RUNNING state; the stamped retention bar decides whether the
## ground is still tended and is what `improvement_is_done` reads. Folding them would make a rung's
## LOSS and a rung's REPAIR the same edge — so the claim is that the sheet says *building* about a
## patch the ladder still calls cultivated.
func _a_rung_that_slipped_is_building_again() -> void:
	var slipped := TileFx.tended_tile_fixture()
	# Eroded below its cost but ABOVE the rung's retention bar, which is what keeps it tended.
	slipped["patch_cultivation_progress"] = SLIPPED_METER_PROGRESS
	# The keeping is UNPAID, because that is what let it slip in the first place — and the keeping POOL
	# owes it at every fullness now (`docs/plan_standing_upkeep.md` §2.4), a half-built meter being no
	# more the build crew's bill than a held rung is. So the shortfall states what the pool missed and
	# the rot states what the meter is losing for it; a shortfall with no bleed beside it would be a
	# patch that is short and somehow not slipping.
	slipped["patch_upkeep_supplied"] = 0.0
	slipped["patch_upkeep_shortfall"] = BaseFx.PLANT_TENDED_UPKEEP_PER_TURN
	BaseFx.price_plant_build(slipped, BaseFx.BUILD_TURNS_REMAINING, SLIPPED_METER_ROT_PER_TURN)
	# **A BAND THAT DECLARED NOTHING.** Its forage assignment carries no `improvement` and no
	# builders, so every reading below is the METER's answer and nothing else's.
	var repair_band := BandFx.without_builders(BandFx.cultivating_forage_band_fixture(
		int(slipped["x"]), int(slipped["y"])))
	repair_band["labor_assignments"][0].erase("improvement")
	h._hud._band_labor._player_band = repair_band
	h._hud._band_labor._player_bands = [repair_band]
	h._hud._compose.reset_forage_source()
	h._show_tile(slipped)
	h._compose_forage(slipped)
	await h._settle()
	await h._save("improvement_rung_slipped")
	# The PRECONDITIONS, without which the claims below describe some other patch: the rung really
	# still stands, and its meter really is short of the cost.
	h._assert_hud("the slipped patch is STILL tended — the rung is above its retention bar",
		SourceForecast.improvement_is_done(slipped, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE))
	h._assert_hud("…and its meter really is short of the cost",
		SourceForecast.build_work_done(slipped, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE)
			< SourceForecast.build_work_cost(slipped, HudComposeVocab.FORAGE_FORECAST_PREFIX,
				SourceForecast.IMPROVEMENT_CULTIVATE))
	# **AND NOBODY DECLARED IT** — the assignment carries no verb, so a client reading the stored
	# field would render this patch's DONE label and offer no way back.
	h._assert_hud("…with no verb on the band's own assignment — nothing here was commanded",
		h._hud._band_labor.improvement_for_forage(repair_band, int(slipped["x"]),
			int(slipped["y"])) == SourceForecast.IMPROVEMENT_NONE)
	var slipped_control = ForageFx.find_improvement_control(
		h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	h._assert_hud("a rung that slipped reads as BUILDING again, derived from its own meter",
		slipped_control != null
			and String(slipped_control.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_RUNNING)
	# **AND THE REMEDY IS ON SCREEN**, which is the whole point of showing it: the repair is staffed
	# from the BUILDERS stepper. **It names no threshold, and that is now half the claim** — the rate a
	# builder had to beat retired with the fullness test (`docs/plan_standing_upkeep.md` §2.4), so the
	# row offers hands and quotes no bar.
	h._assert_hud("…and offers no per-source builders stepper, nor any threshold beside one",
		Readout.stepper_count(h._hud._drawercompose._compose_sheet)
				== Readout.COMPOSE_STEPPERS_PER_SHEET
			and not Q.has_label_containing(h._hud._drawercompose._compose_sheet,
				RETIRED_BUILD_FLOOR_NEEDLE))
	# **THE LAND CARD STATES WHAT IS AT STAKE AND NOT WHAT IS OWED** (issue #545). The standing
	# `Keeping:` bill is retired — it read as noise on every source that owed anything — so what
	# survives is the row that only exists when the rate is going UNPAID, which is this patch.
	var slipped_risk := "\n".join(DetailFormat.at_risk_lines(slipped,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE))
	h._assert_hud("…and the land card states what the shortfall costs, never the pooled bill",
		slipped_risk.contains(DetailFormat.UPKEEP_RISK_ROW)
			and not slipped_risk.contains(UPKEEP_POOL_NEEDLE))
	# **…AND IT NAMES THE ROLE THAT PAYS, which is the sentence the map's `⚠` cannot carry** (§4.6b).
	# That badge is drawn with `draw_string` into `MapView`'s own canvas, so it can hold no tooltip;
	# the card is where a player interrogates it, and the words are the work board's own note, so the
	# two surfaces cannot phrase one hazard differently.
	h._assert_hud("…and names the ROLE that pays it, in the work board's own words",
		slipped_risk.contains(HudWorkVocab.under_kept_note_for_source(
			SourceForecast.SOURCE_KIND_FORAGE)))

## **AN ARROW BETWEEN TWO IDENTICAL NUMBERS** — `0.26 → 0.26 FOOD`, reported from play beside a
## second account correctly reading `0.90 → 0.87`. The `after` reading is attached where it DIFFERS from the
## take, and "differs" was asked of the raw floats while the reading renders through
## `format_magnitude` at two decimals — so any pair closer than the display's own resolution drew a
## transition from a number to itself.
##
## **DRIVEN, NOT RENDERED, and the pair is the claim.** No fixture in the corpus lands two accounts
## that close, and staging one would pin a whole frame's arithmetic to a rounding; `yield_rows` is
## pure, so the two cases are stated directly. The visible pair is what stops "never arrow"
## satisfying this — the gate exists to suppress a mark, and suppressing all of them passes any lone
## negative. The PRECONDITION is the other half: the two food values must be genuinely different
## floats that format alike, or the claim is about equality rather than about precision.
func _arrow_is_gated_on_what_is_SHOWN() -> void:
	var rows := SourceForecast.yield_rows(ARROW_ROUNDING_NOW, ARROW_VISIBLE_NOW,
		SourceForecast.YIELD_ACCOUNT_FOOD, {
			SourceForecast.YIELD_ACCOUNT_FOOD: ARROW_ROUNDING_HOLD,
			SourceForecast.YIELD_ACCOUNT_FODDER: ARROW_VISIBLE_HOLD,
		})
	h._assert_hud("the rounding pair is two DIFFERENT rates that print as one reading",
		not is_equal_approx(ARROW_ROUNDING_NOW, ARROW_ROUNDING_HOLD)
			and SourceForecast.format_magnitude(ARROW_ROUNDING_NOW)
				== SourceForecast.format_magnitude(ARROW_ROUNDING_HOLD))
	h._assert_hud("…so its account states ONE number and no arrow, the transition being invisible",
		not _row_for(rows, SourceForecast.YIELD_ACCOUNT_FOOD).has(SourceForecast.YIELD_ROW_AFTER))
	h._assert_hud("…while the account beside it, which the readout CAN tell apart, keeps its arrow",
		_row_for(rows, SourceForecast.YIELD_ACCOUNT_FODDER).has(SourceForecast.YIELD_ROW_AFTER))

## One account's row out of a `yield_rows` answer — `{}` when that account states none, which fails a
## `has()` claim rather than satisfying it.
func _row_for(rows: Array[Dictionary], account: String) -> Dictionary:
	for row in rows:
		if String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, "")) == account:
			return row
	return {}


## **THE REPRO, AS A FRAME.** A WILD patch — nothing built, so the source's own `upkeepDemand` is the
## honest `0` — with Cultivate declared and ONE builder on it. The rung asks 2 work a turn, so one
## hand nets −1 and the build can never advance; the sim publishes `-2` for exactly this, and the
## sheet must now agree rather than quoting `≈50 turns` off a rate it netted at nothing.
##
## **IT IS THE UNSTARTED CASE THAT MATTERS, and the turn A/B beside it cannot reach it.** That pair
## runs on a patch whose meter is already at 60% and whose keeping is short, so it has work at risk
## and a live bleed. This one has neither, which is the state EVERY rung is in at the moment the
## player is deciding to commit to it — and it is where a wrong quote is least visible.
func _an_unstarted_rung_is_priced_at_its_own_rate() -> void:
	var wild := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	# The PRECONDITION, without which this frame is about some other patch: the source-level demand
	# really is zero here, and the RUNG's rate really is not.
	h._assert_hud("the wild patch is billed nothing today — the source demand is honestly zero",
		SourceForecast.upkeep_state(wild, HudComposeVocab.FORAGE_FORECAST_PREFIX)["demand"]
			< SourceForecast.UPKEEP_WORK_MIN)
	h._assert_hud("…while the Cultivate RUNG still costs %s work a turn to hold"
		% BaseFx.PLANT_TENDED_UPKEEP_PER_TURN,
		is_equal_approx(SourceForecast.build_upkeep_demand(wild,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE),
			BaseFx.PLANT_TENDED_UPKEEP_PER_TURN))
	# …**AND NOTHING IS BANKED, SO NOTHING CAN ROT** — the third precondition, and the one that makes
	# the count below the honest answer rather than a rate that went missing again. The rot is the
	# closed form's only subtracted term now, so a fixture that carried one here would be staging the
	# very state this frame denies.
	h._assert_hud("…and the patch is losing nothing, having nothing banked to lose",
		SourceForecast.meter_rot_per_turn(wild, HudComposeVocab.FORAGE_FORECAST_PREFIX)
			< SourceForecast.UPKEEP_WORK_MIN)
	var band := _fully_committed_forage_band(wild, POOL_TAKE_CREW, UNSTARTED_BUILD_CREW)
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	h._hud._compose.reset_forage_source()
	h._show_tile(wild)
	h._compose_forage(wild)
	BandFx.staff_builders(h._hud._band_labor, UNSTARTED_BUILD_CREW)
	h._compose_forage(wild)
	await h._settle()
	await h._save("improvement_unstarted_standing_price")
	# **THE PRICE AND THE TURN COUNT ARE OFF THIS SHEET, so the claims are made on the PRODUCER**
	# (`docs/plan_standing_upkeep.md` §4.7a ①). Ray, from play: *"That information should be on the
	# work tab. No need to have it here, it is useless."* The arithmetic did not move — only its
	# rendering — so what a rendered face could pin is now pinned on `SourceForecast.build_turns_at`,
	# and the RENDERED home of each half is asserted where it landed: the pile and the standing rate on
	# the WORK ROW's `⌃` tooltip (`band_panel_preview`), the turn count on the BUILD QUEUE row's date.
	var priced := SourceForecast.build_turns_at(wild, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_CULTIVATE, UNSTARTED_BUILD_CREW,
		SourceForecast.DEFAULT_HARVEST_FLOOR, {})
	print("ui_preview: unstarted cultivate turns at %d builders = %d" % [UNSTARTED_BUILD_CREW, priced])
	# **THE RATE IS NOT A TAX ON BUILDING, stated as the number the closed form answers.** One hand
	# banks one work unit a turn against a 50-unit job, and the rung's 2 work a turn is owed to the
	# band's keeping pool rather than taken off this crew.
	h._assert_hud("a lone builder on an UNSTARTED rung is quoted %d turns — its whole output is progress"
		% UNSTARTED_BUILD_TURNS, priced == UNSTARTED_BUILD_TURNS)
	# **THE NEGATIVE THAT NAMES THE RETIRED MECHANISM.** `∞` is what the rate-as-a-build-term
	# arithmetic answered here, and it is the reading that would tell a player this build can never
	# advance when in fact it lands in fifty turns.
	h._assert_hud("…and never the ∞ the rate-as-a-build-term arithmetic answered",
		priced != SourceForecast.BUILD_TURNS_HOLDS and priced != SourceForecast.BUILD_TURNS_ROTS)
	# **AND THE SHEET QUOTES NEITHER PRICE.** Both halves, because dropping the pile while keeping the
	# rate — or the reverse — is the half-move that reads as done. The needles are the shipped formats'
	# own, so a re-worded clause still trips them.
	BandFx.staff_builders(h._hud._band_labor, SourceForecast.BUILD_CREW_NONE)
	h._compose_forage(wild)
	# The sheet rebuilds on the next frame, so the face is read AFTER a settle — without it this reads
	# the crew-of-one sheet still standing and the claim is about the wrong control.
	await h._settle()
	var offer_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: unstarted cultivate OFFER face  %s" % offer_face)
	h._assert_hud("the offered face quotes no standing rate — the ⌃'s tooltip does (got \"%s\")"
			% offer_face,
		not offer_face.contains(HudComposeVocab.BUILD_PRICE_UPKEEP_FORMAT % ["",
			DetailFormat.format_work_units(BaseFx.PLANT_TENDED_UPKEEP_PER_TURN),
			HudWorkVocab.keeping_role_name(SourceForecast.SOURCE_KIND_FORAGE)]))
	h._assert_hud("…and no work pile either, so it is the whole price that left rather than half",
		not offer_face.contains(HudComposeVocab.BUILD_PRICE_WORK_FORMAT
			% DetailFormat.format_work_units(BaseFx.PLANT_CULTIVATE_WORK_COST)))
	# …and it still SAYS something, or "quotes no price" is satisfied by a face that has gone blank.
	# **This rung is DECLARED rather than offered** — the fixture composes a Cultivate nobody has
	# queued — so what it must carry is the queued clause; the OFFERED pointer is asserted where a rung
	# is genuinely on offer (`forage_cultivate_stressed`, `compose_offer_no_hands`).
	h._assert_hud("…while still stating the rung and that it is queued, so the face is not blank",
		offer_face.contains(HudComposeVocab.BUILD_QUEUED_CLAUSE)
			and offer_face.contains(String(
				HudComposeVocab.IMPROVEMENT_OFFER_LABELS[SourceForecast.IMPROVEMENT_CULTIVATE])))

## **BOTH METERS LIVE, BOTH ROWS RENDERED.** A patch holding a Tended rung while a Field goes up is
## two facts — the rung you HAVE and the build in FLIGHT — and a single merged row would silently drop
## one of them. Each row is labelled by its own rung and carries its own state.
func _both_live_meters_get_their_own_row() -> void:
	var both := BaseFx.food_tile_fixture()
	both["patch_is_cultivated"] = true
	both["patch_cultivation_progress"] = 1.0
	both["patch_is_field"] = false
	both["patch_field_progress"] = BOTH_METERS_FIELD_PROGRESS
	BaseFx.price_plant_build(both, BOTH_METERS_FIELD_TURNS)
	# The keeping is PAID, so the tended row above must be bare — which is what makes the Field row's
	# own reading the only thing on this card that says anything is happening.
	both["patch_upkeep_supplied"] = float(both["patch_upkeep_demand"])
	both["patch_upkeep_shortfall"] = 0.0
	var sowing_band := BandFx.cultivating_forage_band_fixture(int(both["x"]), int(both["y"]))
	sowing_band["labor_assignments"][0]["improvement"] = SourceForecast.IMPROVEMENT_SOW
	h._hud._band_labor._player_band = sowing_band
	h._hud._band_labor._player_bands = [sowing_band]
	h._hud.clear_selection()
	h._show_tile(both)
	await h._settle()
	await h._save("tile_two_meters_live")
	var card: String = h._hud.tile_detail.text
	print("ui_preview: two meters  %s | %s" % [
		Readout.detail_excerpt(card, CULTIVATION_ROW_KEY),
		Readout.detail_excerpt(card, HudFloraVocab.FIELD_ROW)])
	h._assert_hud("the rung the patch HOLDS states its badge and how full it still is",
		card.contains(_rung_value_markup("%s %d%%" % [DetailFormat.cultivation_built_label(),
			HudConst.PROGRESS_PERCENT_SCALE], HudStyle.SIGNAL_HEX)))
	h._assert_hud("…and the build in FLIGHT gets its OWN row, leading with the sim's count",
		card.contains(_rung_value_markup(HudSelectionVocab.RUNG_TURNS_FORMAT % [
			BOTH_METERS_FIELD_TURNS,
			HudFormat.progress_percent(BOTH_METERS_FIELD_PROGRESS)], HudStyle.INK_HEX)))
	# **THE PAIR IS THE CLAIM**, and the silence is its third half: a card whose keeping is paid must
	# carry NO hazard mark on either row, or the mark that the four failure states rely on means
	# nothing.
	h._assert_hud("…and a patch whose keeping is paid carries no hazard mark on either row",
		not Readout.detail_excerpt(card, CULTIVATION_ROW_KEY).contains(
				HudSelectionVocab.RUNG_HAZARD_GLYPH)
			and not Readout.detail_excerpt(card, HudFloraVocab.FIELD_ROW).contains(
				HudSelectionVocab.RUNG_HAZARD_GLYPH))

	# **AND THE SAME PATCH WITH ITS KEEPING SHORT — the routing claim** (§4.6a). PNG-less, because only
	# the shortfall moves and the card's shape is the frame above. **Only one meter on a source is ever
	# at risk**: this patch is mid-Sow, so the FIELD is what is billed and the tended rung under it is
	# not billed at all. A `⚠` on the tended row would point the player at ground that is fine, and
	# with the `Keeping:` row retired a false mark costs every true one its meaning.
	#
	# **THE ROUTING USED TO BE ACCIDENTAL.** `is_under_kept` carried a `build_is_in_flight` gate, there
	# for a different reason entirely, which happened to suppress the built row on exactly this shape;
	# merging the two keeping warnings removed the gate and left nothing routing the mark.
	var short_kept_both := both.duplicate(true)
	short_kept_both["patch_upkeep_supplied"] = 0.0
	short_kept_both["patch_upkeep_shortfall"] = BaseFx.PLANT_FIELD_UPKEEP_PER_TURN
	h._hud.clear_selection()
	h._show_tile(short_kept_both)
	await h._settle()
	var short_card: String = h._hud.tile_detail.text
	print("ui_preview: two meters, keeping short  %s | %s" % [
		Readout.detail_excerpt(short_card, CULTIVATION_ROW_KEY),
		Readout.detail_excerpt(short_card, HudFloraVocab.FIELD_ROW)])
	# THE PRECONDITION, or the claim below is about a patch that owes nothing: the source really is
	# short, and the rung the shortfall belongs to really is the Field.
	h._assert_hud("the patch's keeping really is short, and the AT-RISK rung really is the Field",
		SourceForecast.is_under_kept(short_kept_both, HudComposeVocab.FORAGE_FORECAST_PREFIX)
			and SourceForecast.at_risk_rung(short_kept_both,
				HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE)
				== SourceForecast.IMPROVEMENT_SOW)
	# THE ROUTING, at the producer: the shortfall belongs to the Field's row and to no other.
	h._assert_hud("…so the shortfall is the FIELD's and not the tended rung's",
		SourceForecast.rung_is_under_kept(short_kept_both,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE,
			SourceForecast.IMPROVEMENT_SOW)
		and not SourceForecast.rung_is_under_kept(short_kept_both,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE,
			SourceForecast.IMPROVEMENT_CULTIVATE))
	# …AND AS RENDERED: the tended row keeps its badge, unmarked, on a card that IS short of keeping.
	h._assert_hud("…and the tended row renders its badge with no mark, the ground under it being fine",
		short_card.contains(_rung_value_markup("%s %d%%" % [DetailFormat.cultivation_built_label(),
			HudConst.PROGRESS_PERCENT_SCALE], HudStyle.SIGNAL_HEX)))
	# **AND THE SHORTFALL IS NOT SILENT FOR IT** — the routing withholds a mark from one ROW, never the
	# fact. The source-level `At risk:` row states what the shortfall costs and how long is left, which
	# is the half a per-rung mark was never carrying.
	h._assert_hud("…while the card still states what the shortfall costs, on its own At risk: row",
		short_card.contains(DetailFormat.UPKEEP_RISK_ROW))

	# **AND THE SAME ROUTING ON THE UNBUILT ARM — the reviewer's own walk, as a rendered card.** A
	# Cultivate ABANDONED at 60% with a `Sow` declared over it: `build_verb` honours the Sow, so the
	# source publishes the FIELD's countdown, and the Cultivation row printed it — `≈30 turns (60%)`
	# for a meter nobody is touching. `built` forks before the routing does, so the claim above says
	# nothing about this arm and it is asserted separately.
	var abandoned := _an_abandoned_cultivate_under_a_declared_sow()
	var abandoning_band := BandFx.without_builders(BandFx.cultivating_forage_band_fixture(
		int(abandoned["x"]), int(abandoned["y"])))
	abandoning_band["labor_assignments"][0]["improvement"] = SourceForecast.IMPROVEMENT_SOW
	h._hud._band_labor._player_band = abandoning_band
	h._hud._band_labor._player_bands = [abandoning_band]
	h._hud.clear_selection()
	h._show_tile(abandoned)
	await h._settle()
	var abandoned_card: String = h._hud.tile_detail.text
	print("ui_preview: abandoned rung under a declared Sow  %s | %s" % [
		Readout.detail_excerpt(abandoned_card, CULTIVATION_ROW_KEY),
		Readout.detail_excerpt(abandoned_card, HudFloraVocab.FIELD_ROW)])
	# THE PRECONDITION, and it is the whole shape: the two per-source numbers name DIFFERENT rungs
	# here, which is why routing them separately is not over-engineering.
	h._assert_hud("the rung IN FLIGHT is the Sow while the rung AT RISK is the Cultivate",
		SourceForecast.build_verb(abandoned, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.SOURCE_KIND_FORAGE, SourceForecast.IMPROVEMENT_SOW)
			== SourceForecast.IMPROVEMENT_SOW
		and SourceForecast.at_risk_rung(abandoned, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.SOURCE_KIND_FORAGE) == SourceForecast.IMPROVEMENT_CULTIVATE)
	# THE DEFECT, denied by name: the Field's own count must not appear on the Cultivation row.
	h._assert_hud("…so the Cultivation row states no turn count at all — the count is the Field's",
		not Readout.detail_excerpt(abandoned_card, CULTIVATION_ROW_KEY).contains(RUNG_TURNS_NEEDLE))
	# …and the POSITIVE beside it, or "prints nothing" would satisfy the negative: the row states its
	# own condition instead, marked, because this patch's keeping is short.
	h._assert_hud("…and states its OWN condition instead — reverting, marked, in WARN ink",
		abandoned_card.contains(_rung_value_markup(_reverting_value(), HudStyle.WARN_HEX)))

## The Field meter and the sim's answer for it on `tile_two_meters_live`. Deliberately DIFFERENT from
## every other build reading in this chapter, so a card rendering one rung's numbers on both rows
## cannot pass.
const BOTH_METERS_FIELD_PROGRESS := 0.12

const BOTH_METERS_FIELD_TURNS := 30

## **A BAND WITH NO FREE HANDS IS OFFERED A LIVE BOX, AND THAT REVERSES THIS FRAME'S OWN CLAIM**
## (`docs/plan_standing_upkeep.md` §2.5). The box was greyed with a reason for one slice, because
## ticking it declared a build WITH a crew and the sim refused a count the band could not staff —
## which was the click that made the un-undoable `Cultivating 0 / 50 work (0%)` state.
##
## **A VERB DECLARES NOW.** Ticking appends an entry to the band's build queue, which is legal and
## costs nothing whether or not anybody stands on the `builders` role, and unticking sends `unqueue`,
## which really does withdraw it. So there is nothing left to refuse in advance: what says nobody is
## building the rung is the *not started* warning on the declared control, and what fixes it is a role
## card on the Band panel.
##
## **THE FIXTURE IS A SECOND PATCH, and it is kept** — the band's every hand is on tile A, so a sheet
## opened over an UNWORKED tile B is the emptiest state the offer can be in. That is exactly where a
## re-added hands gate would fire, which is what makes this the frame for the negative.
func _a_band_with_no_free_hands_is_offered_a_dead_box() -> void:
	var worked := BaseFx.food_tile_fixture()
	var band := _fully_committed_forage_band(worked, POOL_TAKE_CREW,
		SourceForecast.BUILD_CREW_NONE)
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	# A DIFFERENT patch, unworked by this band and carrying no meter of its own — the state a player
	# reaches by opening the next forage site along.
	var fresh := BaseFx.unbuilt(BaseFx.food_tile_fixture())
	fresh["x"] = int(worked["x"]) + 1
	fresh["y"] = int(worked["y"]) + 1
	h._hud._compose.reset_forage_source()
	h._hud.clear_selection()
	h._show_tile(fresh)
	h._compose_forage(fresh)
	await h._settle()
	await h._save("compose_offer_no_hands")
	var sheet = h._hud._drawercompose._compose_sheet
	var box = ForageFx.find_improvement_control(sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	# THE PRECONDITION, without which every claim below is about an ordinary band: this band really
	# has nothing free, so a hands gate would fire here if one existed anywhere.
	h._assert_hud("the band has no idle hands at all — the state a hands gate would refuse",
		h._hud._band_labor.effective_idle(band) == 0)
	h._assert_hud("the rung is OFFERED — declaring costs no hands, so nothing is refused in advance",
		box != null
			and String(box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_OFFERED)
	# **AND IT NAMES THE CONTROL THAT TAKES IT** (§4.7a ①, ③). This sheet cannot declare, so the offer
	# would otherwise be a priced proposition with no visible way to accept it.
	#
	# **THIS IS THE UNWORKED HALF OF THE PAIR, and the fixture makes it so by accident of its own
	# premise**: the band's every hand is on another tile, so the crew pool here is zero, the stepper
	# clamps the seeded worker away, and the sheet composes NOBODY on ground nobody works. A `⌃` lives
	# on a WORK ROW and this band will have none for this patch, so the honest remedy is the two-step
	# one — which is §4.7a's stated limit, said on the surface where the player meets it.
	#
	# The other half (a crew on the ground, so the one-step remedy) is `forage_cultivate_stressed`.
	# **The PAIR is the claim** — a builder that always printed one sentence passes either alone.
	h._assert_hud("the band composes no crew here, which is what makes this the unworked half",
		h._hud._compose.forage_count() == 0)
	#
	# Read off the FACE, since the remedy is not a note beneath the offer any more — it IS the offer,
	# one line, with the `Work tab` in it as a live `[url]`.
	var unworked_face := ForageFx.improvement_face(sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	h._assert_hud("…so the offer leads with sending gatherers FIRST (got \"%s\")" % unworked_face,
		unworked_face == HudComposeVocab.IMPROVEMENT_OFFER_BARE_FORMAT % [
			FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE),
			HudComposeVocab.BUILD_OFFER_UNWORKED_PLANT_FORMAT % [
				String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS[SourceForecast.IMPROVEMENT_CULTIVATE]),
				HudComposeVocab.WORK_TAB_LINK_TEXT]])
	h._assert_hud("…and never the one-step form, which names a control this band cannot reach",
		not unworked_face.begins_with(HudComposeVocab.IMPROVEMENT_OFFER_BARE_FORMAT % [
			FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE),
			String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS[SourceForecast.IMPROVEMENT_CULTIVATE])]))
	# The NEGATIVE that names the retired mechanism: the sheet must offer no per-source build control
	# of any kind, which is §3.1's rule and the one this whole slice is most likely to walk back.
	h._assert_hud("…and no builders stepper beneath it — the hands are a band-level role",
		Readout.stepper_count(sheet) == Readout.COMPOSE_STEPPERS_PER_SHEET)
	_the_five_states_are_a_SET(band)

## **THE FIVE STATES ARE A SET, AND THE SET IS THE CLAIM** (`docs/plan_standing_upkeep.md` §4.7a ①).
##
## Every state of the improvement control is a `Label` now, so a builder that had stopped
## distinguishing them at all — rendering one state's shape and one state's meta for every input —
## passes any single positive check in this corpus. What cannot pass it is the whole set: five
## fixtures, five DIFFERENT answers, compared by EQUALITY and required to be five distinct values.
##
## **IT IS PNG-LESS AND BUILDS INTO A DETACHED HOST, on purpose.** `_build_improvement_control` is the
## resolver under test and it writes into whatever `VBoxContainer` it is handed, so this drives the
## real fork over real fixtures while touching neither the selection, the compose state nor the open
## sheet — which matters in this harness more than usual, states rendering into ONE long-lived
## `HudLayer` where a block that leaves anything behind moves every frame after it.
##
## **The knowledge in force is the chapter's own full ladder** (pushed above), which is what makes
## Cultivate offerable and leaves Sow gated on the GROUND alone.
func _the_five_states_are_a_SET(band: Dictionary) -> void:
	var compose = h._hud._drawercompose
	var gear: Dictionary = compose._build_gear_for(band, SourceForecast.LABOR_KIND_FORAGE)
	# RUNNING — the reference tile's meter stands at 60%, so the verb answers off the meter.
	var running := _improvement_state_for(BaseFx.food_tile_fixture(), "cultivate",
		SourceForecast.IMPROVEMENT_NONE, band, gear)
	# DECLARED — the same patch with nothing banked and the rung declared, nobody on the pool.
	var unstarted := BaseFx.food_tile_fixture()
	unstarted["patch_cultivation_progress"] = POOL_UNSTARTED_METER
	var declared := _improvement_state_for(unstarted, "cultivate",
		SourceForecast.IMPROVEMENT_CULTIVATE, band, gear)
	# OFFERED — that same patch with nothing declared either.
	var offered := _improvement_state_for(unstarted, "cultivate",
		SourceForecast.IMPROVEMENT_NONE, band, gear)
	# DONE and GATED come off ONE build: a tended patch renders the finished Cultivate as a state and
	# the next rung, Sow, as the ground's refusal — the two answers a single control mounts together.
	var tended := TileFx.tended_tile_fixture()
	tended["patch_cultivation_progress"] = 1.0
	tended["patch_is_cultivated"] = true
	var host := VBoxContainer.new()
	compose._build_improvement_control(SourceForecast.LABOR_KIND_FORAGE, tended,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.DEFAULT_HARVEST_FLOOR,
		SourceForecast.IMPROVEMENT_NONE, band, 1, gear, true, host,
		SourceForecast.BUILD_CREW_NONE)
	var done := ForageFx.improvement_state(host, "cultivate")
	var gated := ForageFx.improvement_state(host, SourceForecast.IMPROVEMENT_SOW)
	host.free()
	var seen := [running, declared, offered, done, gated]
	var want := [HudWidgets.IMPROVEMENT_STATE_RUNNING, HudWidgets.IMPROVEMENT_STATE_DECLARED,
		HudWidgets.IMPROVEMENT_STATE_OFFERED, HudWidgets.IMPROVEMENT_STATE_DONE,
		HudWidgets.IMPROVEMENT_STATE_GATED]
	h._assert_hud("the five improvement states are five DISTINCT answers (got %s)" % str(seen),
		seen == want)

## One `_build_improvement_control` run into a detached host, answering the state it resolved for
## `rung` — `""` where it built no control for that rung at all, which fails an equality rather than
## satisfying one.
func _improvement_state_for(source: Dictionary, rung: String, composed: String,
		band: Dictionary, gear: Dictionary) -> String:
	var host := VBoxContainer.new()
	h._hud._drawercompose._build_improvement_control(SourceForecast.LABOR_KIND_FORAGE, source,
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.DEFAULT_HARVEST_FLOOR,
		composed, band, 1, gear, true, host, SourceForecast.BUILD_CREW_NONE)
	var state := ForageFx.improvement_state(host, rung)
	host.free()
	return state
