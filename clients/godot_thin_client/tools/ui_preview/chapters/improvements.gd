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

const IMPROVEMENT_PAUSED_NEEDLE := "ease off and it resumes"

## The offer wording that must NOT appear while the rung is gated — the imperative the gated state
## exists to remove. Kept as a literal so a reworded offer cannot silently pass this assertion.
const GATED_OFFER_NEEDLE := "Cultivate this patch"

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

const REVERTING_METER_PERCENT := 96

## The tile the band works INSTEAD in the reverting frame — any tile that is not the one being judged.
## The patch under test is then improved, owned and unworked, which is the whole condition.
const METER_AWAY_TILE_X := 64

## The tile card's cultivation ROW key, for the run-log excerpt. Not an assertion input: the assertions
## match the rendered VALUE markup, which no other row can produce.
const CULTIVATION_ROW_KEY := "Cultivation"

## The take that crew is paid — `min(2 × 0.32, 0.96 × 0.25)` = the DIPPED ceiling, 0.24 food/turn. It is
## the number the green forecast line, the deal's middle term and the sim's own `actual_yield` must all
## carry; before the dip reached the forecast the green line quoted 0.64 (the undipped labour take) while
## the deal beside it said 0.24 — the same patch, the same crew, two different answers on one sheet.
# The take the sheet quotes on `improvement_build_crew`: the crew clamps to the sim's own
# `workers_needed` (12), and 12 x 0.32 x 0.25 = 0.96 — exactly the food-peak ceiling, i.e. the
# saturation point where the dip costs nothing at all. That coincidence IS the frame's subject.
const BUILD_CREW_DIPPED_TAKE := "0.96"

## One rung-meter row's rendered VALUE CELL — `[color=#HEX]<verb> 96%[/color]`, exactly as
## `DetailFormat.detail_bbcode` emits it. Word and tint in ONE needle, because the decaying state was a
## failure of BOTH and an assertion that pinned only one of them would pass on half a fix.
func _meter_value_markup(verb: String, hex: String) -> String:
	return "[color=#%s]%s %d%%[/color]" % [hex, verb, REVERTING_METER_PERCENT]

## The staple tile as the COMPOSE SHEET sees it — `BaseFx.food_tile_fixture` already runs through
## `BaseFx.seed_forage_rows`, so this is simply the named handle the dip-comparison assertion reads its
## forecast from. Naming it keeps that assertion from re-stating which fixture it is judging.
func _seeded_food_tile() -> Dictionary:
	return BaseFx.food_tile_fixture()

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
	# CEILING-BOUND ON PURPOSE, and it is what makes the frame beneath this one legible. The readout's
	# take is `min(crew x per_worker, ceiling)`, so at a small crew LABOR binds and every floor quotes
	# the same number — the two frames would then differ only in which rung is lit. Saturating the
	# patch puts the CEILING in charge, which is the term the floor actually moves.
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("improvement_running_plant")
	var sustain_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
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
	# **THE CAPTION CARRIES BOTH KEYS AT ONCE, and this is the frame where both are true.** These
	# readings are the DIPPED take AND they arrow toward a holding rate, so a caption stating only
	# the dip leaves the row's `0.64 → 0.15` unkeyed — which shipped once, as an unconditional
	# override, and is a caption that has stopped explaining a mark still on screen. Matched by
	# EQUALITY against the both-true const: a `contains` on the dip half passes on exactly the
	# collapsed caption this pins against, the two strings sharing a prefix.
	h._assert_hud("…and the yields caption keys BOTH the dip and the row's own arrow",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.YIELD_ROW_HEADER_WHILE_BUILDING_WITH_AFTER.to_upper())
	# **KNOWN LESSON + A BUILD IN FLIGHT — the teaching line keeps the half that is still true.**
	# Cultivation completed several frames above, so `Teaching cultivation at ×1.00` would be teaching a
	# craft this faction finished learning; one multiplier paces the lesson and the build meter alike,
	# so what survives is the BUILDING half. Both halves asserted: the word must be gone AND the
	# building sentence present, or blanking the line entirely would pass.
	h._assert_hud("a lesson the faction already knows is not taught again beside a running build",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_LESSON_NEEDLE))
	h._assert_hud("…while the BUILD half, which one multiplier still paces, keeps its line",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_BUILD_NEEDLE))
	# **THE RUNNING BOX IS LIVE, AND IS NEVER GATED.** Unchecking abandons the build, and the abandon
	# path asks for nothing — no knowledge, no ceiling, no site, no Thriving — because abandoning a
	# STALLED build is the case it exists for. A disabled box here would be the regression the split
	# introduced by accident (under the old model, picking another policy always walked a build away).
	h._assert_hud("a running improvement's box is LIVE — unchecking is always allowed",
		running_box is CheckBox and not (running_box as CheckBox).disabled)

	# State 442-deplete-beside-cultivate — **THE FRAME THE WHOLE TWO-AXIS MODEL EXISTS TO MAKE SAYABLE.**
	# The same running Cultivate at a DEEP FLOOR: legal, un-gated, and self-defeating through the
	# ecology rather than through a rule. The deeper floor frees a larger ceiling, so the crew's take
	# TODAY is bigger than the food-peak frame's — which is exactly the trap: you take more now and
	# drive the patch out of Thriving, stalling your own meter.
	h._hud._compose.set_forage_floor(ForageFx.DEEP_DRAW_FLOOR)
	h._hud._compose.set_forage_count(ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("improvement_deplete_while_building")
	# THE READING THAT MOVES IS THE READOUT'S, and it is the one the deal's middle term used to restate
	# — which is why deleting that term lost no information. Compared against the food-peak frame above:
	# a deeper floor frees a larger ceiling, so the same crew on the same patch must quote a different
	# take. The PAYOFF deliberately does not move (it is a property of the finished rung, not of the
	# floor), so asserting the face here would assert a constant.
	var deplete_yields = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: take  peak=%s  deep=%s" % [sustain_yields, deplete_yields])
	h._assert_hud("…and the RENDERED take moves with the floor, not just the model behind it",
		sustain_yields != "" and deplete_yields != "" and sustain_yields != deplete_yields)
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
	# **THE DIP MOVED ONTO THE CREW** (`docs/plan_harvest_floor.md` §3.1), and this is the assertion
	# that pins it. The old claim here was that a deeper draw's "while building" term is BIGGER,
	# because the dip multiplied the ceiling — which is exactly the bug the move fixed: a fraction of a
	# bigger standing stock still filled the crew's baskets, so a deep floor built for free. The dip is
	# a factor on THROUGHPUT now, so the build term is floor-INDEPENDENT wherever the crew is the
	# binding side, and the two floors' build terms are EQUAL there. The crew is deliberately small
	# enough to bind under both ceilings; the take-today assertion beside it is what stops this from
	# passing vacuously on a forecast that ignores the floor altogether.
	var band = h._hud._band_labor.player_band()
	var deep_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		ForageFx.DEEP_DRAW_FLOOR, SourceForecast.IMPROVEMENT_CULTIVATE)
	var peak_deal := SourceForecast.improvement_forecast(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.IMPROVEMENT_CULTIVATE)
	var deep_building = SourceForecast.expected_yield(
		deep_deal["build_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
	var peak_building = SourceForecast.expected_yield(
		peak_deal["build_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
	h._assert_hud("the build term is floor-INDEPENDENT on a labour-bound crew — a deep floor builds no faster",
		is_equal_approx(deep_building, peak_building))
	h._assert_hud("…while the take TODAY still rises with the deeper draw, so the frame is not vacuous",
		SourceForecast.expected_yield(deep_deal["base_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band)
		>= SourceForecast.expected_yield(peak_deal["base_forecast"], ForageFx.IMPROVEMENT_STANCE_FRAME_FORAGERS, band))

	# State 442-build-crew — **THE SHEET AND THE SIM, ON ONE NUMBER.** `forecast_inputs` used to take a
	# STANCE ONLY, so while a build ran the sheet read the UNDIPPED ceiling and three surfaces went wrong
	# together: the stepper let the player dial workers the sim reports idle, the green line quoted a take
	# the sim does not pay, and the overdraw verdict compared an undipped take against the Sustain bar.
	# The two cap paths are documented as twins that "can never gate differently" — and they could not,
	# because they were wrong in the SAME way, agreeing with each other while contradicting the sim.
	# So the control here is the SIM's answer: `workers_needed`, read back off the very assignment the
	# sheet is composed over.
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
	# ONE equality, and the DIP is what carries it. Undipped the cap is ceil(0.96/0.32) = 3 — a quarter
	# of what the sim asks — and the rung's crew floor (2) sits below either, so only the dipped
	# inversion lands on the sim's 12.
	h._assert_hud("the compose stepper caps at the crew the SIM asks for (%d), not at an undipped ceiling"
		% sim_workers_needed, rendered_cap == sim_workers_needed)
	# THE WORKED-ROW TWIN, on the SAME forecast. `source_worker_cap_state` is the Band panel's gate, and
	# the two are only genuinely one ceiling if it goes dead at exactly that count — asserted on either
	# side of it so "always false" cannot pass.
	var build_forecast := SourceForecast.forecast_inputs(_seeded_food_tile(),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.IMPROVEMENT_CULTIVATE)
	var build_floor := SourceForecast.plant_crew_floor(_seeded_food_tile(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE)
	var row_below: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed - 1, BUILD_CREW_IDLE_ON_HAND, build_floor)["can_add"])
	var row_at: bool = bool(SourceForecast.source_worker_cap_state(build_forecast,
		sim_workers_needed, BUILD_CREW_IDLE_ON_HAND, build_floor)["can_add"])
	h._assert_hud("…and the WORK BOARD's `+` gates at the same count — live below it, dead at it",
		row_below and not row_at)
	# THE READOUT'S TAKE, read off the RENDERED sheet: it must be the sim's own
	# `min(w × per_worker × dip, ceiling)`, not the undipped labour take.
	#
	# **THE SECOND HALF OF THIS PAIR WAS THE DEAL'S "while building" TERM, and it is gone WITH the deal
	# line rather than merely untested.** The two were asserted to carry the same figure, and they did —
	# byte for byte, being the same crew through the same dipped forecast — which is precisely the
	# duplication that retired the line. What remains is the one producer.
	var build_green = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: build crew  take=%s" % build_green)
	h._assert_hud("the green forecast line quotes the DIPPED take the sim pays (%s)"
		% BUILD_CREW_DIPPED_TAKE, build_green.contains(BUILD_CREW_DIPPED_TAKE)
		and build_green.contains(SourceForecast.YIELD_RENEWABLE_NOTE.to_upper()))

	# THE ABANDON, plant side — driven here rather than on the frame above because committing CLOSES
	# the sheet and writes a pending assign, which the Deplete frame beside it reads.
	await h._assert_abandon_emits(SourceForecast.LABOR_KIND_FORAGE, "cultivate",
		"abandon_improvement %d forage %d %d" % [HudConst.PLAYER_FACTION_ID,
			int(BaseFx.food_tile_fixture()["x"]), int(BaseFx.food_tile_fixture()["y"])])

	# State 442-cultivate-paused — the PAUSED build. The sim deliberately leaves this alone: a patch that
	# drops out of Thriving mid-build KEEPS its improvement and merely pauses accrual
	# (`.claude/rules/core_sim/cultivation.md` — "neither lost nor silently switched"). The control has to
	# say the same thing: the box stays CHECKED and a WARN line states the pause, its cause and the
	# ease-off remedy. This is the `_tame_stalled_hint` treatment, now on the plant web too.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.reset_forage_source()
	h._show_tile(TileFx.stressed_tile_fixture())
	h._compose_forage(TileFx.stressed_tile_fixture())
	await h._settle()
	await h._save("improvement_paused_plant")
	var paused_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate")
	h._assert_hud("a paused build keeps its box CHECKED — progress is not lost",
		paused_box is CheckBox and (paused_box as CheckBox).button_pressed)
	h._assert_hud("…and the WARN line names the pause, its cause and the ease-off remedy",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet, IMPROVEMENT_PAUSED_NEEDLE))
	# **THE SHARPEST CASE FOR THE UNGATED RULE.** A STALLED build is exactly when a player reaches for
	# the abandon, so a paused box must stay LIVE — and this is the one frame where greying it would
	# look defensible (the source has left Thriving, which is what gates the build's own START). The
	# notes here are a loud WARN line, so this also pins that `notes` do not disable a RUNNING control
	# the way they disable an OFFERED one.
	h._assert_hud("a PAUSED build's box is still live — abandoning a stalled build is the whole point",
		paused_box is CheckBox and not (paused_box as CheckBox).disabled)

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
	# **THE FIXTURE MOVED FROM THE KNOWLEDGE GATE TO A SOURCE GATE, and that is the rule change rather
	# than a weakening.** It staged a wild Thriving patch with Cultivation 35% known, i.e. a rung gated
	# on KNOWLEDGE ALONE — and the compose sheet now renders NO control there at all (the aside two
	# rows up says the same lesson live and quantified, and the reason's remedy named the very work the
	# sheet was composing). A Stressed patch with Cultivation fully known keeps this frame's actual
	# subject — the gated control's SHAPE — on a gate that survives. The suppressed case is not lost
	# either: `forage_cultivate_locked` already staged exactly this fixture and is now the frame the
	# suppression rule is judged on.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.reset_forage_source()
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.0,
	}])
	h._show_tile(TileFx.stressed_tile_fixture())
	h._compose_forage(TileFx.stressed_tile_fixture())
	await h._settle()
	await h._save("improvement_offered_gated")
	var gated_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate")
	# **A GATED RUNG IS A LABEL, NOT A DISABLED CHECKBOX** — the control's SHAPE says whether this is a
	# choice or a fact, and an unmet prerequisite is a fact. The greyed-checkbox form this replaced put
	# an offer the player cannot accept ("Cultivate this patch · then 0.04 food …") directly above the
	# sentence explaining that they cannot accept it.
	h._assert_hud("a gated improvement is SHOWN, never hidden — the rung stays discoverable",
		gated_box != null)
	h._assert_hud("…as a LABEL rather than a checkbox, because it is a state and not a choice",
		not (gated_box is CheckBox))
	# Matched WHOLE, not by needle: this reason is the one the ecology raises, and a `contains` on a
	# fragment would still pass if the remedy clause (the half that says what to DO) went missing.
	h._assert_hud("…whose own text is the REASON, so nothing offers what cannot be taken",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
			== HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE),
				HudFloraVocab.GATE_REASON_PATCH_THRIVING_FORMAT % String(
					TileFx.stressed_tile_fixture()["patch_ecology_phase"]).capitalize()])
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

	# Restore the unassigned near band + a plain Sustain compose for the range states below.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_count(1)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
