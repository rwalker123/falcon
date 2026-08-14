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

## The RETIRED threshold NOTE's own stem — the prose that sat beside the BUILDERS stepper restating
## what the build line's colour says. A LITERAL for the reason above: the const is gone, and the rate
## it carried lives in that row label's tooltip now, which the claim beside this needle reads.
const RETIRED_BUILD_FLOOR_NEEDLE := "the surplus is progress"

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
## **THREE HANDS, BECAUSE THE RUNG'S OWN RATE COMES OFF THE TOP** (issue #545). This patch has nothing
## built on it, so its SOURCE demand is honestly zero — and a client netting against that quoted a
## finish date for a crew the rung can never let advance. `plant:tended` asks 2 work a turn whether or
## not anything is at risk yet, so the pace is `3 − 2` and the estimate is ⌈50 ÷ 1⌉. At the ONE hand
## this frame used to staff the honest answer is `∞`, which is a different frame's subject
## (`improvement_never_finishes_unstarted`) and not this one's.
const BUILD_ROOM_BUILDERS := 3

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

## **A NEGATIVE THIS CLIENT DOES NOT RECOGNISE** — one past the two the wire spells
## (`sim_schema::{NO_BUILD_TURNS_ESTIMATE, BUILD_NEVER_FINISHES}`). It stands for whatever the
## field grows next, and the claim it pins is that an unknown answer renders as NO answer rather
## than falling into whichever branch happens to be last.
const UNKNOWN_BUILD_TURNS_SENTINEL := -3

## ---- THE REPAIR (`improvement_rung_slipped`) --------------------------------------------------
## Where a slipped tended patch's meter sits: below its cost, so the rung is BUILDING again, and
## above `plant:tended`'s own `retain_fraction` (0.75), so the ground is still tended. The gap
## between those two numbers is the whole state, and a fixture that missed it on either side would
## be staging a completed rung or a lost one.
const SLIPPED_METER_PROGRESS := 0.9

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

## ---- THE REPRO (`improvement_never_finishes_unstarted`) ---------------------------------------
## **A WILD PATCH, CULTIVATE DECLARED, ONE BUILDER — AND THE SHEET PROMISED ≈50 TURNS.** Reported
## from play: commit it, wait four turns, and the meter still reads `0 / 50 (0%)` because the rung's
## rate is 2 and one hand nets −1. The sim was right the whole time (it publishes `-2` here); the
## client's own stepper arithmetic subtracted the SOURCE's `upkeepDemand`, which is `0` on a patch
## with no progress — nothing is at risk yet — so the rate vanished from the form at exactly the
## moment the sheet is quoting a rung nobody has started, and it computed `50 ÷ 1`.
const UNSTARTED_BUILD_CREW := 1

## What the broken arithmetic quoted, so the NEGATIVE can name the defect rather than merely denying
## a shape: `PLANT_CULTIVATE_WORK_COST ÷ UNSTARTED_BUILD_CREW`, with the rung's own rate netted at
## nothing.
const UNSTARTED_BUILD_WRONG_TURNS := 50

## ---- THE TURN-ESTIMATE A/B (`improvement_turns_lone_crew` / `_full_crew`) ---------------------
## **TWO CREWS ON ONE PATCH AT ONE FLOOR**, which is the only shape that can show the estimate moving.
## Both are well under the frame's own worker cap, so what differs between the frames is the count and
## nothing else.
## **THE PUBLISHED THRESHOLD ON THE REFERENCE PATCH, IN WORK PER TURN** — `plant:tended`'s own
## `upkeep.work_per_turn`, which the fixture states as `patch_cultivation_upkeep_demand`. It is what
## the BUILDERS row names, and at the shipped one-work-per-worker-turn output it brackets the A/B:
## `TURNS_LONE_CREW` banks less than it and `TURNS_FULL_CREW` more.
const TURNS_MIN_BUILD_WORK := BaseFx.PLANT_TENDED_UPKEEP_PER_TURN

const TURNS_LONE_CREW := 1

const TURNS_FULL_CREW := 4

## What each of those crews owes, derived HERE from the fixture rather than from the producer under
## test: the reference tile's Cultivate costs `BaseFx.PLANT_CULTIVATE_WORK_COST` (50) and its meter
## stands at `patch_cultivation_progress` 0.6, so 20 work units are left; no plant item declares the
## build stat yet, so the crew's gear takes nothing off that; and one worker banks one work unit a
## turn.
##
## **AND THE MAINTENANCE RATE COMES OFF THE CREW BEFORE ANY OF IT IS PROGRESS**
## (`docs/plan_standing_upkeep.md` §2.4). The reference patch owes `plant:tended`'s
## `BaseFx.PLANT_TENDED_UPKEEP_PER_TURN` (2.0) every turn while its meter is going up, and the
## BUILD crew is what supplies it there — so the pace is `crew − 2` and not the crew.
##
## **THAT MAKES THE LONE BUILDER A CREW THAT NEVER FINISHES, and the A/B is sharper for it.** One
## hand against a two-hand rate is a NEGATIVE net: the meter slides back rather than creeping
## forward, so there is no turn count to quote and the face states `∞` in the warning ink
## (`SourceForecast.BUILD_TURNS_NEVER`). Four hands clear the rate with two to spare: ⌈20 ÷ 2⌉.
## Under the retired `work_cost / crew` these read 20 and 5 — a lone builder was merely slow, which
## is the promise this arc exists to stop the sheet making.
const TURNS_AT_FULL_CREW := 10

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
	}, HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_TAME,
		SourceForecast.SOURCE_KIND_HERD, DetailFormat.husbandry_built_label(), false,
		REVERTING_METER_PROGRESS, true, false)

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
		DetailFormat.husbandry_built_label(), false, 0.0, false, true)
	# (2) work banked and nobody on it.
	var sliding := DetailFormat.rung_row_value({}, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, SourceForecast.SOURCE_KIND_HERD,
		DetailFormat.husbandry_built_label(), false, REVERTING_METER_PROGRESS, false, false)
	# (3) staffed at or under the rate, and (4) staffed with nothing accruing anyway.
	return unstarted.contains(mark) and sliding.contains(mark) \
		and _rung_value_for_turns(SourceForecast.BUILD_TURNS_NEVER).contains(mark) \
		and _rung_value_for_turns(SourceForecast.BUILD_TURNS_NO_ESTIMATE).contains(mark)

## The reverting row's expected value at this chapter's meter — the hazard mark and the percentage.
func _reverting_value() -> String:
	return HudSelectionVocab.RUNG_REVERTING_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH,
		HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## …and the BUILDING row's, which leads with the sim's own count.
func _building_value(turns: int) -> String:
	return HudSelectionVocab.RUNG_TURNS_FORMAT % [
		turns, HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## …and the NEVER row's, which leads with the ∞ under the same mark.
func _never_value() -> String:
	return HudSelectionVocab.RUNG_NEVER_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH, DetailFormat.BUILD_TURNS_NEVER_GLYPH,
		HudFormat.progress_percent(REVERTING_METER_PROGRESS)]

## The staple tile as the COMPOSE SHEET sees it — `BaseFx.food_tile_fixture` already runs through
## `BaseFx.seed_forage_rows`, so this is simply the named handle the dip-comparison assertion reads its
## forecast from. Naming it keeps that assertion from re-stating which fixture it is judging.
func _seeded_food_tile() -> Dictionary:
	return BaseFx.food_tile_fixture()

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
	# **A RUNNING BUILD IS A STATE LABEL, NOT A CHECKED BOX** (`docs/plan_standing_upkeep.md` §2.4).
	# The checkbox existed so it could be UNCHECKED, which sent `abandon_improvement`; the verb is
	# derived from the meter now, so there is nothing stored for an uncheck to clear and the control's
	# TYPE carries the CHOICE/FACT distinction the GATED and DONE states already used.
	h._assert_hud("a running Cultivate renders a STATE label, not a box to uncheck",
		running_box is Label and not (running_box is CheckBox)
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
	# **THE LEVER ON A RUNNING BUILD IS ITS BUILDERS STEPPER, and it is never gated.** The ungated rule
	# survives the control it was written for: a STALLED build is exactly when a player reaches for the
	# remedy, and the remedy is hands. So the row has to be mounted here, on a build the sheet is
	# reporting rather than offering.
	h._assert_hud("a running improvement mounts its BUILDERS stepper — the lever that replaced the uncheck",
		ForageFx.build_crew_row(h._hud._drawercompose._compose_sheet) != null)
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
	var idle_band := BandFx.cultivating_forage_band_fixture()
	idle_band["labor_assignments"][0].erase("improvement")
	idle_band["labor_assignments"][0].erase("improvement_workers")
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
	# **AND THE BUILD'S OWN CREW ROW IS THERE**, which is the control this frame gained: the verb is
	# staffed by a stepper of its own, not by the gatherers above it.
	h._assert_hud("…and the running Cultivate carries a build crew row of its own",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet,
			HudWidgets.BUILD_CREW_ROW_META) != null)

	# WALKING AWAY, plant side — driven here rather than on the frame above because committing CLOSES
	# the sheet and writes a pending assign, which the Deplete frame beside it reads. It is the SET
	# verb at zero builders now: `abandon_improvement` cleared a stored intent and there is no stored
	# intent left, so unstaffing the crew is the whole of walking away.
	await h._assert_walk_away_emits(SourceForecast.LABOR_KIND_FORAGE, "cultivate",
		"cultivate %d %d %d 0" % [HudConst.PLAYER_FACTION_ID,
			int(BaseFx.food_tile_fixture()["x"]), int(BaseFx.food_tile_fixture()["y"])])

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
	h._assert_hud("a stalled build still reads as RUNNING — progress is not lost",
		no_room_box is Label
			and String(no_room_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_RUNNING)
	# **THE SHARPEST CASE FOR THE UNGATED RULE.** A STALLED build is exactly when a player reaches for
	# the remedy, so the lever must be there — and this is the one frame where withholding it would
	# look defensible (the source has left Thriving, which is what USED to gate the build's own start).
	h._assert_hud("a stalled build still offers its BUILDERS stepper — staffing it is the whole point",
		ForageFx.build_crew_row(h._hud._drawercompose._compose_sheet) != null)

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
	h._hud._compose.set_forage_build_count(BUILD_ROOM_BUILDERS)
	h._compose_forage(no_room_tile)
	await h._settle()
	await h._save("improvement_stressed_advances")
	var advancing_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: stressed advancing  face=%s" % advancing_face)
	h._assert_hud("a floor beneath the stock leaves room, so the SAME patch quotes its turns",
		advancing_face.ends_with(_turns_clause(TURNS_AT_ROOM_FLOOR)))
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
	h._assert_hud("a finished Cultivate is a static LABEL, not a checkbox",
		done_label is Label and not (done_label is CheckBox))
	h._assert_hud("…naming the state the build left the patch in",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate").contains(
			String(HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"])))
	# The ladder CONTINUES: an offerable next rung is a live checkbox, which is also what separates the
	# done state from a dead end. A gated next rung would be a Label — see `forage_sow_locked` — so
	# this assertion only means something on ground that will take seed.
	var next_rung = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "sow")
	h._assert_hud("…and the NEXT rung's LIVE checkbox sits beneath it",
		next_rung is CheckBox and not (next_rung as CheckBox).disabled)

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
	h._assert_hud("…as a LABEL rather than a checkbox, because it is a state and not a choice",
		not (gated_box is CheckBox))
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
	h._assert_hud("…and offers no crop to commit to, committing being what is refused",
		ForageFx.find_crop_row(h._hud._drawercompose._compose_sheet, ForageFx.GATED_CROP_NEEDLE) == null)

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
	#   (b) REVERTING — the SAME patch at the SAME percentage with nobody building it. The band is
	#   working a different tile, so the patch is improved, unworked and bleeding.
	BaseFx.price_plant_build(meter_tile, SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture(
		METER_AWAY_TILE_X, int(meter_tile["y"]))
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_reverting")
	var reverting_row = h._hud.tile_detail.text
	print("ui_preview: meter rows  building=%s  reverting=%s" % [
		Readout.detail_excerpt(building_row, CULTIVATION_ROW_KEY),
		Readout.detail_excerpt(reverting_row, CULTIVATION_ROW_KEY)])
	h._assert_hud("the SAME meter with nobody on it reads as a LOSS, in WARN ink — not a build",
		reverting_row.contains(_rung_value_markup(_reverting_value(), HudStyle.WARN_HEX)))
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

	#   (c) NEVER — **the third answer, and the one this card was silent about**
	#   (`docs/plan_standing_upkeep.md` §2.4). The SAME patch with the SAME crew ON it, whose
	#   staffing is at or below the maintenance rate: the meter is not bleeding — somebody is
	#   working it — so the row keeps its BUILD verb and its neutral ink, and what changes is the
	#   estimate beneath it. `buildTurnsRemaining` publishes three answers now, and `-2` is *this
	#   staffing never finishes*: a standing fact about a commitment the player has already made,
	#   where `-1`'s states are a transient absence of information. Folded into one sentinel it
	#   rendered as NO LINE here — visible only to a compose sheet that redid the comparison
	#   itself — which is the reassuring direction on the one state that should stop them.
	BaseFx.price_plant_build(meter_tile, SourceForecast.BUILD_TURNS_NEVER)
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud.clear_selection()
	h._show_tile(meter_tile)
	await h._settle()
	await h._save("tile_meter_never")
	var never_row = h._hud.tile_detail.text
	print("ui_preview: meter never  %s" % Readout.detail_excerpt(never_row, CULTIVATION_ROW_KEY))
	# **THE WORD AND THE INK, in one needle, exactly as the A/B above pins its two.** The indented
	# branch tints the WHOLE line, so the markup is the claim: silence and neutral ink are both
	# failures here, and only asserting the colour separates ∞-the-warning from the larder runway's
	# ∞, which draws the identical glyph for the opposite news.
	h._assert_hud("a committed crew below the rate reads ∞ on the card's own row, in WARN ink",
		never_row.contains(_rung_value_markup(_never_value(), HudStyle.WARN_HEX)))
	# …and the row still carries its METER, because the crew is ON this one. Without the percentage
	# the frame is satisfied by the unstarted state, which is a different fact with a different
	# remedy: nobody is there, against somebody is there and it is not enough.
	h._assert_hud("…and it still states how far the meter got — somebody IS on this one",
		never_row.contains("(%d%%)" % HudFormat.progress_percent(REVERTING_METER_PROGRESS)))
	# **THE UNRECOGNISED SENTINEL, ASKED OF THE PRODUCER.** A negative this client does not know is
	# *no answer*, never a guess — and with the row now leading on the count, "no answer" has to
	# render as the STALLED hazard rather than as a bare percentage, which is the silence the whole
	# redesign exists to remove. `-3` stands for whatever the wire grows next.
	h._assert_hud("…and an unrecognised negative is marked as STALLED, never rendered bare",
		_rung_value_for_turns(UNKNOWN_BUILD_TURNS_SENTINEL)
			== HudSelectionVocab.RUNG_STALLED_FORMAT % [
				HudSelectionVocab.RUNG_HAZARD_GLYPH,
				HudFormat.progress_percent(REVERTING_METER_PROGRESS)])

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
	h._show_tile(_seeded_food_tile())
	h._compose_forage(_seeded_food_tile())
	# Dialled AFTER the first open, for the reason `_compose_herd`'s docstring gives: the source change
	# re-seeds the composition, so a build crew set before it is silently thrown away.
	h._hud._compose.set_forage_build_count(TURNS_LONE_CREW)
	h._compose_forage(_seeded_food_tile())
	await h._settle()
	await h._save("improvement_turns_lone_crew")
	var lone_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	# Captured off the LONE-crew sheet, because the next line recomposes it and frees these nodes.
	var _face_warned_at_lone_crew := ForageFx.improvement_face_is_warned(
		h._hud._drawercompose._compose_sheet, "cultivate")
	var _floor_at_lone_crew := ForageFx.build_work_floor(h._hud._drawercompose._compose_sheet)
	var _floor_tip_at_lone_crew := ForageFx.build_work_floor_tooltip(
		h._hud._drawercompose._compose_sheet)
	# The face's INK, as a colour rather than a bool: the pace has three states and the two `∞` ones
	# would read alike through a warned/not-warned reader.
	var _face_ink_at_lone_crew := ForageFx.improvement_face_color(
		h._hud._drawercompose._compose_sheet, "cultivate")
	h._hud._compose.set_forage_build_count(TURNS_FULL_CREW)
	h._compose_forage(_seeded_food_tile())
	await h._settle()
	await h._save("improvement_turns_full_crew")
	var full_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	var _face_was_warned_at_full_crew := ForageFx.improvement_face_is_warned(
		h._hud._drawercompose._compose_sheet, "cultivate")
	# Captured before the drag, so the drag frame's negative compares against a rendered reading rather
	# than a recomposition.
	var full_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build turns  lone=%s  full=%s" % [lone_face, full_face])
	# **EQUALITY ON THE WHOLE CLAUSE, and the counts are derived in this chapter rather than through
	# the producer under test** — an expectation composed from `build_turns_at` could only agree with
	# itself. `ends_with` because the clause closes the face the meter opens.
	#
	# **THE LONE BUILDER NEVER FINISHES, AND THE SHEET SAYS SO WITH AN `∞` RATHER THAN A SILENCE.**
	# It is below this rung's maintenance rate, so the meter holds or slides and no turn count is
	# ever reached; a large number there would read as a promise, and no clause at all would read as
	# a rendering fault beside a meter the same face shows part-full.
	h._assert_hud("one hand under the rate never finishes, and the face states ∞ rather than a number",
		lone_face.ends_with(_never_clause()))
	# …**IN THE WARNING INK, which is the half a wording assertion cannot see.** The larder's runway
	# draws the same glyph in neutral ink for the opposite news, so ∞ alone does not say which way
	# this one points. Read as the RESOLVED colour, `get_theme_color` answering the stock default
	# where no override is set — an "an override exists" test would pass on the very bug.
	h._assert_hud("…and it is inked as a WARNING, where the larder's own ∞ is good news",
		_face_warned_at_lone_crew)
	h._assert_hud("…while four hands clear the rate and quote a real count — the estimate moves with the stepper",
		full_face.ends_with(_turns_clause(TURNS_AT_FULL_CREW)))
	# The PAIR's other half: the crew that DOES finish must not be wearing the warning, or the ink is
	# decoration rather than a verdict.
	h._assert_hud("…and that face is NOT warned — the amber is the verdict, not the control's livery",
		not _face_was_warned_at_full_crew)
	# **AND THE THRESHOLD IS STILL REACHABLE FROM THE STEPPER, IN WORK** (issue #545). It makes `∞`
	# actionable rather than merely alarming — it names the rate to beat — and it is read off the meta
	# rather than any wording. **Work, not hands**: the model is denominated in work units end to end,
	# and the head count it replaced reads `0` before a build starts.
	h._assert_hud("the BUILDERS row states the WORK that holds the rung (%s), on both sides of it"
		% TURNS_MIN_BUILD_WORK,
		is_equal_approx(_floor_at_lone_crew, TURNS_MIN_BUILD_WORK)
			and is_equal_approx(ForageFx.build_work_floor(h._hud._drawercompose._compose_sheet),
				TURNS_MIN_BUILD_WORK))
	# **THE VISIBLE NOTE IS RETIRED AND THE RATE IS A HOVER, so the claim is REACHABILITY.** It was a
	# sentence beside the stepper (`2 work a turn holds it — the surplus is progress`) doing the job the
	# build line's own colour does one row up, so it went; the NUMBER did not, which is what the pair of
	# claims here says — the tooltip names it, and no visible Label repeats it.
	h._assert_hud("…and the rate is still reachable, as the row label's tooltip",
		_floor_tip_at_lone_crew.contains(
			DetailFormat.format_work_units(TURNS_MIN_BUILD_WORK)))
	h._assert_hud("…while the retired NOTE is on no label anywhere on the sheet",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, RETIRED_BUILD_FLOOR_NEEDLE))
	# **THE PACE IS A COLOUR, AND THE PAIR IS THE CLAIM.** A lone builder under the rate holds the meter
	# (amber, `∞`); four clear it and the meter climbs (green, a real count). A face pinned to either
	# ink passes one half and fails the other, which is what makes this a verdict rather than livery.
	h._assert_hud("a HOLDING build line is amber and a GROWING one is green",
		_face_ink_at_lone_crew == HudStyle.WARN
			and ForageFx.improvement_face_color(h._hud._drawercompose._compose_sheet,
				"cultivate") == HudStyle.HEALTHY)
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
	h._assert_hud("a crew of nobody states NO estimate, never a number",
		SourceForecast.build_turns_at(_seeded_food_tile(),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE,
			SourceForecast.BUILD_CREW_NONE, SourceForecast.FLOOR_FOOD_PEAK,
			NO_BUILD_GEAR) == SourceForecast.BUILD_TURNS_NO_ESTIMATE)
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
	await _the_sheets_two_crews_share_one_pool()
	await _a_reopened_sheet_shows_the_LIVE_crew()
	await _a_declared_build_with_no_builders_says_so()
	await _an_unstarted_rung_is_priced_at_its_own_rate()
	await _both_live_meters_get_their_own_row()
	await _a_band_with_no_free_hands_is_offered_a_dead_box()

## **THE SHEET IS ONE TRANSACTION OVER A SOURCE'S TWO CREWS, AND IT IS CLAMPED AS ONE.** Reported
## from play: a band with every hand on a herd, HUNTERS dropped 4 → 2, and BUILDERS still dead at a
## maximum of 0 — the two hands freed inside the sheet were invisible to the other stepper until the
## player committed, closed and reopened.
##
## **THE PRECONDITION IS `idle_workers == 0`, and every claim here is worthless without it.** With
## spare hands in the band both steppers are live for reasons that have nothing to do with each other,
## so the frame would pass against the pre-fix per-activity ceilings.
##
## **THE TWO HALVES ARE THE CLAIM, not either one.** A `+` that is dead at a full take passes on a
## builders row that is dead always; a `+` that is live at a reduced take passes on one that never
## clamps at all. So the state asks both, and finishes by pressing the `+` through the sheet's own
## handler until the pool is spent — which is the third claim, that the clamp still refuses to offer
## a crew this band does not have.
func _the_sheets_two_crews_share_one_pool() -> void:
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
	await h._save("compose_pool_take_full")
	var sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("the band has NO idle hands — every claim below is about the pool, not about slack",
		int(band["idle_workers"]) == 0)
	h._assert_hud("…and the sheet opens on the crew the band really has — %d" % POOL_TAKE_CREW,
		Readout.stepper_value(sheet) == POOL_TAKE_CREW)
	h._assert_hud("…with the BUILDERS stepper at nobody",
		Readout.build_crew_value(sheet) == SourceForecast.BUILD_CREW_NONE)
	h._assert_hud("…and its `+` dead, the take holding every hand the pool has",
		not Readout.build_crew_can_add(sheet))
	# DROP THE TAKE, IN THE SHEET, WITHOUT COMMITTING — which is the whole gesture under test.
	h._hud._compose.set_forage_count(POOL_REDUCED_TAKE)
	h._compose_forage(tile)
	await h._settle()
	await h._save("compose_pool_take_freed")
	sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("dropping the take to %d frees hands INSIDE the sheet" % POOL_REDUCED_TAKE,
		Readout.stepper_value(sheet) == POOL_REDUCED_TAKE)
	h._assert_hud("…so the BUILDERS `+` comes live with no commit, close and reopen",
		Readout.build_crew_can_add(sheet))
	# …AND STOPS EXACTLY WHERE THE BAND DOES. Pressed rather than written, so the ceiling clamp in the
	# sheet's own handler is what answers. **Each press rebuilds the controls and the old row is
	# `queue_free`d, i.e. still in the tree until the frame ends** — so the settle between presses is
	# load-bearing: without it the second press lands on the freed row and the count never moves.
	for _i in range(POOL_TAKE_CREW - POOL_REDUCED_TAKE):
		Readout.build_crew_plus(h._hud._drawercompose._compose_sheet).pressed.emit()
		await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	print("ui_preview: shared pool  take=%d  builders=%d  can_add=%s" % [
		Readout.stepper_value(sheet), Readout.build_crew_value(sheet),
		Readout.build_crew_can_add(sheet)])
	h._assert_hud("…up to the %d the take gave back" % (POOL_TAKE_CREW - POOL_REDUCED_TAKE),
		Readout.build_crew_value(sheet) == POOL_TAKE_CREW - POOL_REDUCED_TAKE)
	h._assert_hud("…and no further — the sheet never offers a crew the sim would refuse",
		not Readout.build_crew_can_add(sheet))

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
	# **AND THE PLAYER CAN GET BACK OFF IT.** The declaration used to render as RUNNING, which is a
	# `Label`: the box vanished, so a build ticked on a band with no free hands could not be unticked.
	# Reported from play. It is the DECLARED state now — the same checkbox, TICKED and LIVE — and the
	# three claims are one claim each about the three ways that used to fail: the wrong node type, an
	# unticked box that would read as no declaration at all, and a disabled one that cannot be undone.
	var declared_box = ForageFx.find_improvement_control(
		h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
	h._assert_hud("a declaration nobody is building is a CHECKBOX, not the running state label",
		declared_box is CheckBox
			and String(declared_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_DECLARED)
	h._assert_hud("…ticked, because the declaration stands",
		declared_box is CheckBox and (declared_box as CheckBox).button_pressed)
	h._assert_hud("…and LIVE, so unticking it is the walk-away — even on a band with no free hands",
		declared_box is CheckBox and not (declared_box as CheckBox).disabled
			and h._hud._band_labor.effective_idle(band) == 0)
	# THE NEGATIVE that names the defect: the word a rung nobody is building must not wear anywhere on
	# the card, in any ink — that word is what read as work in progress.
	h._assert_hud("…and never a turn count, which is what a started build states and this is not",
		not h._hud.tile_detail.text.contains(RUNG_TURNS_NEEDLE))
	# THE MAP's own fork, asked directly — all three answers, or "always warn" passes the first two.
	h._assert_hud("the map badge reads an unstarted rung as NOT WORKING",
		SourceForecast.unstaffed_build_of(POOL_UNSTARTED_METER, SourceForecast.BUILD_CREW_NONE)
			== SourceForecast.BUILD_UNSTAFFED_UNSTARTED)
	h._assert_hud("…a meter with work banked and nobody on it as SLIDING BACK",
		SourceForecast.unstaffed_build_of(REVERTING_METER_PROGRESS, SourceForecast.BUILD_CREW_NONE)
			== SourceForecast.BUILD_UNSTAFFED_SLIDING)
	h._assert_hud("…and a staffed one as neither, so the plate keeps its percentage",
		not SourceForecast.build_is_unstaffed(SourceForecast.unstaffed_build_of(
			POOL_UNSTARTED_METER, BandFx.CULTIVATING_BAND_BUILDERS)))
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
		"improvement": SourceForecast.IMPROVEMENT_CULTIVATE, "improvement_workers": builders,
		"workers_needed": ForageFx.CULTIVATE_SIM_WORKERS_NEEDED, "overdraws": false,
	}]
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
	BaseFx.price_plant_build(slipped)
	# The keeping is UNPAID, because that is what let it slip in the first place — and while the
	# meter is down it is the BUILD crew that owes the rate, which is what the card's row must say.
	slipped["patch_upkeep_supplied"] = 0.0
	slipped["patch_upkeep_shortfall"] = BaseFx.PLANT_TENDED_UPKEEP_PER_TURN
	# **A BAND THAT DECLARED NOTHING.** Its forage assignment carries no `improvement` and no
	# builders, so every reading below is the METER's answer and nothing else's.
	var repair_band := BandFx.cultivating_forage_band_fixture(
		int(slipped["x"]), int(slipped["y"]))
	repair_band["labor_assignments"][0].erase("improvement")
	repair_band["labor_assignments"][0].erase("improvement_workers")
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
	# from the BUILDERS stepper, and the threshold beside it names the crew that stops the slide.
	h._assert_hud("…and offers the BUILDERS stepper, whose threshold is stated in WORK",
		is_equal_approx(ForageFx.build_work_floor(h._hud._drawercompose._compose_sheet),
			BaseFx.PLANT_TENDED_UPKEEP_PER_TURN))
	# **THE LAND CARD STATES WHAT IS AT STAKE AND NOT WHAT IS OWED** (issue #545). The standing
	# `Keeping:` bill is retired — it read as noise on every source that owed anything — so what
	# survives is the row that only exists when the rate is going UNPAID, which is this patch.
	var slipped_risk := "\n".join(DetailFormat.at_risk_lines(slipped,
		HudComposeVocab.FORAGE_FORECAST_PREFIX))
	h._assert_hud("…and the land card states what the shortfall costs, never the pooled bill",
		slipped_risk.contains(DetailFormat.UPKEEP_RISK_ROW)
			and not slipped_risk.contains(UPKEEP_POOL_NEEDLE))

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
## **IT IS THE UNSTARTED CASE THAT MATTERS, and the existing turn A/B cannot reach it.** That pair
## runs on the reference patch, whose meter is already at 60% — so its `upkeepDemand` is live and the
## old arithmetic happened to be right there. The defect only shows where nothing is at risk yet,
## which is every rung the player is about to commit to.
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
	var band := _fully_committed_forage_band(wild, POOL_TAKE_CREW, UNSTARTED_BUILD_CREW)
	h._hud._band_labor._player_band = band
	h._hud._band_labor._player_bands = [band]
	h._hud._compose.reset_forage_source()
	h._show_tile(wild)
	h._compose_forage(wild)
	h._hud._compose.set_forage_build_count(UNSTARTED_BUILD_CREW)
	h._compose_forage(wild)
	await h._settle()
	await h._save("improvement_never_finishes_unstarted")
	var face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	print("ui_preview: unstarted cultivate face  %s" % face)
	h._assert_hud("a lone builder on an UNSTARTED rung reads ∞, in the warning ink",
		face.contains(DetailFormat.BUILD_TURNS_NEVER_GLYPH)
			and ForageFx.improvement_face_is_warned(
				h._hud._drawercompose._compose_sheet, "cultivate"))
	# **THE NEGATIVE THAT NAMES THE DEFECT.** `≈50 turns` is what the source-demand arithmetic
	# produced, and it is the reading the player acted on; denying the ∞'s absence alone would pass on
	# a sheet quoting any other wrong number.
	h._assert_hud("…and never the %d turns the source-demand arithmetic promised"
		% UNSTARTED_BUILD_WRONG_TURNS,
		not face.contains(_turns_clause(UNSTARTED_BUILD_WRONG_TURNS)))
	# **AND THE THRESHOLD BESIDE IT IS STATED IN WORK, on the one state where the head count it
	# replaced reads zero** — `upkeepWorkersNeeded` is `0` on an unstarted source, so the note that
	# tells the player what to beat fell silent exactly where it was needed.
	h._assert_hud("the BUILDERS row names the RATE to beat, on a rung with no progress",
		is_equal_approx(ForageFx.build_work_floor(h._hud._drawercompose._compose_sheet),
			BaseFx.PLANT_TENDED_UPKEEP_PER_TURN))

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

## The Field meter and the sim's answer for it on `tile_two_meters_live`. Deliberately DIFFERENT from
## every other build reading in this chapter, so a card rendering one rung's numbers on both rows
## cannot pass.
const BOTH_METERS_FIELD_PROGRESS := 0.12

const BOTH_METERS_FIELD_TURNS := 30

## **A BAND WITH NO FREE HANDS IS OFFERED A DEAD BOX WITH ITS REASON, NEVER A LIVE ONE.** Reported
## from play: a player ticked `cultivate` on a site their band could not staff, and the sheet went
## straight to the running state — a `Label`, so there was nothing left to untick. The declaration
## itself is now re-tickable (`tile_build_unstaffed`), and this is the other half: the click that
## creates that state is refused BEFORE it happens, with the reason where the offer was.
##
## **THE FIXTURE IS A SECOND PATCH, and that is what makes the pool empty.** The band's every hand is
## on tile A, so a sheet opened over an UNWORKED tile B has `crew_pool == 0` — the sheet's build pool
## being the SOURCE's pool less the composed take, not the band's idle count. Reusing tile A would
## stage a source whose own crew is in the pool and the box would rightly stay live.
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
	# has nothing free, so the offer really is unstaffable.
	h._assert_hud("the band has no idle hands at all — the pool the offer draws on is empty",
		h._hud._band_labor.effective_idle(band) == 0)
	h._assert_hud("the rung is still OFFERED — it is refused, not hidden",
		box is CheckBox
			and String(box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_OFFERED)
	h._assert_hud("…and DISABLED, so the click that made the un-undoable state cannot happen",
		box is CheckBox and (box as CheckBox).disabled)
	h._assert_hud("…with the reason in its own slot, never a dead control that says nothing",
		Q.has_label_containing(sheet, HudComposeVocab.BUILD_NO_HANDS_REASON))
	# The NEGATIVE that names what a dead offer must not grow: a BUILDERS stepper for a build nobody
	# can staff would be the same live-but-useless control one row down.
	h._assert_hud("…and no BUILDERS stepper beneath it — there is nothing to staff it with",
		ForageFx.build_crew_row(sheet) == null)
