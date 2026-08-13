extends RefCounted

## The improvement control and the tile meters.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The RETIRED pause note's own stem — the line that told a player to ease workers off a build the
## floor was pacing. Spelled as a LITERAL because the vocabulary const is gone: a needle recomposed
## from a live format could only ever describe whatever the code still says.
const RETIRED_PAUSED_NOTE_NEEDLE := "ease off and it resumes"

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
## there is any room to work in at all. ⌈50 ÷ 1⌉.
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

## ---- THE TURN-ESTIMATE A/B (`improvement_turns_lone_crew` / `_full_crew`) ---------------------
## **TWO CREWS ON ONE PATCH AT ONE FLOOR**, which is the only shape that can show the estimate moving.
## Both are well under the frame's own worker cap, so what differs between the frames is the count and
## nothing else.
const TURNS_LONE_CREW := 1

const TURNS_FULL_CREW := 4

## What each of those crews owes, derived HERE from the fixture rather than from the producer under
## test: the reference tile's Cultivate costs `BaseFx.PLANT_CULTIVATE_WORK_COST` (50) and its meter
## stands at `patch_cultivation_progress` 0.6, so 20 work units are left; no plant item declares the
## build stat yet, so the crew's gear takes nothing off that; the floor sits at the food peak, so the
## learn multiplier is exactly ×1.0; and one worker banks one work unit a turn. 20 ÷ 1 and ⌈20 ÷ 4⌉.
const TURNS_AT_LONE_CREW := 20

const TURNS_AT_FULL_CREW := 5

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

## One rung-meter row's rendered VALUE CELL — `[color=#HEX]<verb> 48 / 50 work (96%)[/color]`, exactly
## as `DetailFormat.detail_bbcode` emits it. Word and tint in ONE needle, because the decaying state
## was a failure of BOTH and an assertion that pinned only one of them would pass on half a fix.
##
## The value itself goes through `DetailFormat.build_meter_value`, so the needle states the job's SIZE
## the way the row does — and the claim stays about the verb and the ink, which is what moves between
## these two frames.
func _meter_value_markup(verb: String, hex: String) -> String:
	return "[color=#%s]%s[/color]" % [hex, DetailFormat.build_meter_value(verb,
		REVERTING_METER_PROGRESS,
		REVERTING_METER_PROGRESS * BaseFx.PLANT_CULTIVATE_WORK_COST,
		BaseFx.PLANT_CULTIVATE_WORK_COST)]

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
	h._assert_hud("a running Cultivate renders a CHECKED improvement box",
		running_box is CheckBox and (running_box as CheckBox).button_pressed)
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
	# **THE RUNNING BOX IS LIVE, AND IS NEVER GATED.** Unchecking abandons the build, and the abandon
	# path asks for nothing — no knowledge, no ceiling, no site, no Thriving — because abandoning a
	# STALLED build is the case it exists for. A disabled box here would be the regression the split
	# introduced by accident (under the old model, picking another policy always walked a build away).
	h._assert_hud("a running improvement's box is LIVE — unchecking is always allowed",
		running_box is CheckBox and not (running_box as CheckBox).disabled)
	# **THE NON-VACUITY HALF OF THE SUPPRESSION CLAIM, and it is not optional here.** Both caption
	# claims above are satisfied for free by a crew that never reaches its floor — there would be no
	# walk to suppress — so the SAME patch, crew and floor are re-composed with the box UNTICKED,
	# where the walk must be back in full. Only the BUILD moves between the two readings, which is
	# what makes the suppression attributable to it rather than to the fixture.
	#
	# PNG-less, and placed AFTER every assertion that holds a node from the saved frame: a re-compose
	# frees the sheet's children, so a captured `CheckBox` read afterwards is a freed instance. The
	# composed Cultivate is restored immediately, leaving the sheet as the saved frame had it.
	h._hud._compose.set_forage_improvement(SourceForecast.IMPROVEMENT_NONE)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	h._assert_hud("…while the SAME crew with the box unticked walks to its floor and says so",
		Readout.yields_show_a_transition(h._hud._drawercompose._compose_sheet)
			and Readout.yields_header(h._hud._drawercompose._compose_sheet)
				== SourceForecast.YIELD_ROW_HEADER_WITH_AFTER.to_upper())
	h._hud._compose.set_forage_improvement(SourceForecast.IMPROVEMENT_CULTIVATE)
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
		and building_box is CheckBox and (building_box as CheckBox).button_pressed)
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

	# THE ABANDON, plant side — driven here rather than on the frame above because committing CLOSES
	# the sheet and writes a pending assign, which the Deplete frame beside it reads.
	await h._assert_abandon_emits(SourceForecast.LABOR_KIND_FORAGE, "cultivate",
		"abandon_improvement %d forage %d %d" % [HudConst.PLAYER_FACTION_ID,
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
	h._assert_hud("a stalled build keeps its box CHECKED — progress is not lost",
		no_room_box is CheckBox and (no_room_box as CheckBox).button_pressed)
	# **THE SHARPEST CASE FOR THE UNGATED RULE.** A STALLED build is exactly when a player reaches for
	# the abandon, so its box must stay LIVE — and this is the one frame where greying it would look
	# defensible (the source has left Thriving, which is what USED to gate the build's own start).
	h._assert_hud("a stalled build's box is still live — abandoning it is the whole point",
		no_room_box is CheckBox and not (no_room_box as CheckBox).disabled)

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
	h._assert_hud("a meter a crew IS building reads as a BUILD, in neutral ink",
		building_row.contains(_meter_value_markup(
			HudFloraVocab.CULTIVATION_PREPARING_LABEL, HudStyle.INK_HEX)))
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
		reverting_row.contains(_meter_value_markup(
			HudFloraVocab.RUNG_REVERTING_LABEL, HudStyle.WARN_HEX)))
	# THE NEGATIVE, with the positive above it as its companion (a whole-text search alone would also
	# pass on a card that rendered no cultivation row at all): the build's word must be REPLACED, not
	# merely joined — a row reading both would be the same ambiguity in longer form.
	h._assert_hud("…and the build's own word is GONE from the row, not merely joined by another",
		not reverting_row.contains(HudFloraVocab.CULTIVATION_PREPARING_LABEL))
	# **THE TURN ESTIMATE IS A PAIR, and the negative is the whole of `-1`'s contract**
	# (`docs/plan_unit_costed_work.md` §11): a build under way states what it has left, and one nobody
	# is working states NOTHING — never `0 turns`, which reads as a build about to land on ground that
	# is going back to wild. The positive is what stops the negative passing on a card that lost the
	# line entirely; the whole ROW is the needle, so a stray digit elsewhere on the card cannot satisfy
	# either half.
	var turns_row := HudSelectionVocab.BUILD_TURNS_ROW_FORMAT % BaseFx.BUILD_TURNS_REMAINING
	h._assert_hud("a running build states the sim's own turn estimate beside its meter",
		building_row.contains(turns_row))
	# The reverting half states no estimate for the honest reason: nobody works this ground, so the sim
	# answers `-1`. **The needle is the row's shared TAIL, never a specific count** — one shaped like a
	# count sails past a row rendering any other, the sentinel's own `≈-1 turns` included.
	h._assert_hud("…and a meter nobody is building states NO estimate, not a zero",
		not reverting_row.contains(HudSelectionVocab.BUILD_TURNS_ROW_TAIL))
	# **THE SENTINEL'S OWN RULE IS SEPARATE, and only a DRIVEN claim reaches it.** The frame above is
	# silent because its fixture states `-1`, so it would stay silent for a producer that RENDERED the
	# sentinel — `≈-1 turns at this crew` under a STALLED build (a crew whose output is zero, which the
	# sim answers `-1` for). Asked of the producer directly, over one source dict in its two states,
	# since no frame in the corpus stages a stall and the failure is a string either way.
	var stalled := {SourceForecast.FORECAST_BUILD_TURNS_KEY: SourceForecast.BUILD_TURNS_NO_ESTIMATE}
	var running := {SourceForecast.FORECAST_BUILD_TURNS_KEY: BaseFx.BUILD_TURNS_REMAINING}
	h._assert_hud("a STALLED build's `-1` renders no estimate line at all",
		not "\n".join(DetailFormat.build_estimate_lines(stalled, HudComposeVocab.BARE_FORECAST_PREFIX))
			.contains(HudSelectionVocab.BUILD_TURNS_ROW_TAIL))
	h._assert_hud("…while a live one does, so the silence above is the sentinel and not the producer",
		"\n".join(DetailFormat.build_estimate_lines(running, HudComposeVocab.BARE_FORECAST_PREFIX))
			.contains(turns_row))
	# **THE GEAR LINE'S NEGATIVE.** No plant item declares the build stat yet (issue #539 is the hoe),
	# so a plant build's contribution is honestly `0` and the row must not appear at all — a
	# `−0 work off this job` advertises a tool that did nothing. Its positive twin is the animal web's
	# (`herd_corral`), where the shipped handling gear really does take work off the job.
	h._assert_hud("a build no tool helps states NO gear line, not a zero one",
		not building_row.contains(HudSelectionVocab.BUILD_GEAR_WORK_ROW_FORMAT.split("%s")[0]))

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
	h._hud._compose.set_forage_build_count(TURNS_FULL_CREW)
	h._compose_forage(_seeded_food_tile())
	await h._settle()
	await h._save("improvement_turns_full_crew")
	var full_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "cultivate")
	# Captured before the drag, so the drag frame's negative compares against a rendered reading rather
	# than a recomposition.
	var full_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build turns  lone=%s  full=%s" % [lone_face, full_face])
	# **EQUALITY ON THE WHOLE CLAUSE, and the two counts are derived in this chapter rather than
	# through the producer under test** — an expectation composed from `build_turns_at` could only
	# agree with itself. `ends_with` because the clause closes the face the meter opens.
	h._assert_hud("one forager's estimate is the whole remaining job, one turn per work unit",
		lone_face.ends_with(_turns_clause(TURNS_AT_LONE_CREW)))
	h._assert_hud("…and four hands quarter it — the estimate moves with the stepper",
		full_face.ends_with(_turns_clause(TURNS_AT_FULL_CREW)))
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
