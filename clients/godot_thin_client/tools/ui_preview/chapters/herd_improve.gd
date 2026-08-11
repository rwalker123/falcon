extends RefCounted

## The animal web's improvement rungs and its build dip.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# ---- THE BUILDING HERD: the regime where the dip drops the crew BELOW ONE BODY ------------------
# **THE ANIMAL WEB'S HALF OF THE SAME DEFECT**, and it fails DIFFERENTLY from the plant one, which is
# why it needs its own frame. A hunt take is quantised to whole animals AFTER the crew's collection is
# dipped (`hunt_take` → `quantise_animal_take`), and that rule's `max(1, carryable)` means a crew the
# build drops below one body does NOT simply take a fraction less: it still kills one animal and
# WASTES what it cannot haul. So the dip moves the waste line, not merely the take, and a fixture whose
# crew stays above one body the whole way through cannot see it.
#
# Every constant is a SHIPPED one. The species is the roster's heaviest TAMEABLE animal — a Steppe
# Runner (`fauna_config` `body_mass` 120, `husbandry_ceiling` "pastoral") — because the regime needs a
# body heavier than a few hands can carry AND a rung the sheet will actually offer: the heavier
# mammoth is `wild`-ceilinged and can never be tamed, so composing a Tame on one would stage a build
# the sim refuses.
const HERD_DIP_BODY_MASS := 120.0

# `fauna_config` `hunt.provisions_per_biomass` and `labor_config`'s
# `hunt.per_worker_biomass_capacity` — the two rates that turn biomass into this sheet's numbers.
const HERD_DIP_PROVISIONS_PER_BIOMASS := 0.02

const HERD_DIP_PER_WORKER_BIOMASS := 40.0

# A migratory herd's own scale (`fauna_config` mammoth/steppe-runner `biomass` 4000–12000). The stock
# sits a WHISKER above half its capacity, which is what puts the food-peak ceiling (0.4 food/turn) far
# below the take and leaves the ⚠ free to fire — while the herd's regrowth there (~112 biomass/turn)
# sits BETWEEN the dipped crew's carry (80) and the undipped one's (160), so the same crew draws this
# herd down while hunting it and does not while gentling it. That is the whole A/B.
const HERD_DIP_CAPACITY := 9000.0

const HERD_DIP_STOCK := 4520.0

# Below the food peak, so the take is judged against a ceiling it can exceed — the only way the
# overdraw flag can testify about anything on this frame.
const HERD_DIP_FLOOR := 0.30

# Four hunters carry 3.2 food undipped — one whole 2.4-food body, with change — and 1.6 under the
# build, which is two thirds of a body: the forced-partial branch, and a third of the kill left behind.
const HERD_DIP_CREW := 4

# `intensification_ladder.json`, animal:pastoral `yield_fraction_while_building`. The animal rungs dip
# to a HALF where the plant rungs dip to a quarter — do not carry the plant number across.
const HERD_DIP_BUILD_FRACTION := 0.5

# The crew this herd WOULD owe once managed (`herders_needed_if_managed`); its ownership-gated twin is
# 0, because a herd mid-Tame is not owned yet. Under the composed crew, so the investment rung's cap
# floor is not what the frame ends up measuring.
const HERD_DIP_WOULD_BE_HERDERS := 3

const HERD_DIP_IDLE_WORKERS := 12

## The Corral done-label's upkeep clause — asserted PRESENT on the penned frame and ABSENT on the
## pastoral one, which is the only way to pin an asymmetry rather than merely one side of it.
const UPKEEP_NEEDLE := "fodder/turn upkeep"

## The invariant TAIL of `SourceForecast.HUNT_WASTE_NOTE_FORMAT` (`⚠ %d%% wasted`) — the only part of
## that note a percentage-free ABSENCE test can name. The present-case assertion uses the whole
## formatted note instead, so the pair cannot both be satisfied by a note that lost its number.
const HUNT_WASTE_NEEDLE := "wasted"

## THE BUILDING HERD — a Steppe Runner mid-TAME, at the shipped rates (see the `HERD_DIP_*` block).
## It is the ONLY fixture on either web where the build dip changes the SHAPE of the take rather than
## its size: four hunters carry one whole body, the same four gentling the herd carry two thirds of
## one, and `quantise_animal_take`'s `max(1, carryable)` turns that shortfall into a kill they cannot
## haul home.
##
## It states its terms in the MODERN wire vocabulary (stock, capacity, the per-biomass vector) rather
## than as a legacy per-stance table, so `ForageFx.floorify_ceilings` leaves every number exactly as authored —
## which is what lets the assertions recompose the sim's own take from them.
func _building_herd_fixture() -> Dictionary:
	return {
		"id": "game_runner_09", "label": "Steppe Runners (game_runner_09)",
		"species": "Steppe Runners", "size_class": "migratory",
		"huntable": true, "ecology_phase": "thriving",
		# Pastoral ceiling + a part-filled meter: Tame is the rung on offer and it is RUNNING, which is
		# the only shape that can dip a crew at all.
		"husbandry_ceiling": "pastoral",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": HERD_DIP_STOCK,
		"carrying_capacity": HERD_DIP_CAPACITY,
		"graze_range_radius": 2,
		"provisions_per_biomass": HERD_DIP_PROVISIONS_PER_BIOMASS,
		"per_worker_biomass": HERD_DIP_PER_WORKER_BIOMASS,
		"per_worker_yield": HERD_DIP_PER_WORKER_BIOMASS * HERD_DIP_PROVISIONS_PER_BIOMASS,
		# One body, in food and in biomass — two statements of the same animal, so the whole-animal
		# quantum the sheet divides by cannot disagree with the curve beside it.
		"food_per_animal": HERD_DIP_BODY_MASS * HERD_DIP_PROVISIONS_PER_BIOMASS,
		"body_mass": HERD_DIP_BODY_MASS,
		"tame_build_fraction": HERD_DIP_BUILD_FRACTION,
		# The pastoral rung's payoff (its own MSY: the pastoral r over this K, through the hunt rate),
		# so the improvement control states a real deal rather than a zero.
		"pastoral_yield": 3.6,
		"herders_needed": 0,
		"herders_needed_if_managed": HERD_DIP_WOULD_BE_HERDERS,
		"tile_info": HerdFx.plain_herd_tile_info(),
	}

## The band gentling it: standing ON the herd (distance 0 ≤ reach → the LOCAL branch, the only one
## that carries an improvement), `output_multiplier` 1.0 so the rendered numbers ARE the model's, and
## idle hands well clear of the composed crew so the frame measures the dip and not the labor ceiling.
func _building_herd_band_fixture() -> Dictionary:
	return {
		"id": "Band 1", "entity": 846, "faction": 0, "size": 90,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": HERD_DIP_IDLE_WORKERS,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	}

## The band STANDING on Tame on that herd — the fixture the re-admission frame turns on. Everything
## else about `BandFx.band_fixture` is kept; only the assignment list is replaced, by the single hunt
## assignment whose `fauna_id` matches `HerdFx.fully_tamed_herd_fixture`'s and whose policy is the rung the
## ceiling pass has since hidden.
func _tame_standing_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["labor_assignments"] = [{
		"kind": "hunt", "workers": HerdFx.TAMED_HERD_CREW, "fauna_id": HerdFx.taming_herd_fixture()["id"],
		"floor": 0.5, "improvement": "tame", "target_x": 70, "target_y": 17,
		"actual_yield": 0.45, "sustainable_yield": 0.45,
		"workers_needed": HerdFx.TAMED_HERD_CREW, "overdraws": false,
	}]
	return band

func run(harness) -> void:
	h = harness

	# State 442-tame-running — THE ANIMAL WEB's running improvement, the exact twin of
	# `improvement_running_plant`. Same control, same three states, same forecast: the two ladders are
	# one grammar (spec §4), and rendering them together is what proves it.
	h._hud._band_labor._player_band = _tame_standing_band_fixture()
	h._hud._band_labor._player_bands = [_tame_standing_band_fixture()]
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.taming_herd_fixture())
	h._compose_herd(HerdFx.taming_herd_fixture())
	await h._settle()
	await h._save("improvement_running_animal")
	var tame_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "tame")
	h._assert_hud("a running Tame renders a CHECKED improvement box, as Cultivate does",
		tame_box is CheckBox and (tame_box as CheckBox).button_pressed)
	# **THE SAME PAIR ITS PLANT TWIN CARRIES, on the web that shares the control.** The payoff is off
	# both sheets' faces and in both readouts; asserting only the absence would pass on a sheet that
	# had lost the payoff too, which is why the second half names where it went.
	h._assert_hud("…with no payoff on its face, exactly as the plant web has none",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, "tame")
			.contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
	h._assert_hud("…and the terms in the PER TURN readout, under this rung's ONCE TAMED key",
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet).contains(
			String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS[
				SourceForecast.IMPROVEMENT_TAME]).to_upper()))
	# The animal web's half of the one-row claim — see its plant twin on `improvement_running_plant`
	# for why a `contains` cannot make it and the count must.
	h._assert_hud("…as the block's ONLY row on this web too",
		Readout.improvement_deal_rows(h._hud._drawercompose._compose_sheet) == 1)
	h._assert_hud("…repeating no magnitude the yields row already states",
		not Readout.deal_repeats_a_yields_number(h._hud._drawercompose._compose_sheet))
	# KNOWN LESSON + A BUILD IN FLIGHT, on the animal web: Herding is complete for this faction, so the
	# aside drops the craft and keeps the build the same multiplier paces. Both halves, for the reason
	# the plant twin states.
	h._assert_hud("a known lesson is not taught again on the hunt sheet either",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_LESSON_NEEDLE))
	h._assert_hud("…while its BUILD half still reads, as it does on the plant sheet",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_BUILD_NEEDLE))
	h._assert_hud("a running Tame's box is LIVE too — the abandon path is ungated on both webs",
		tame_box is CheckBox and not (tame_box as CheckBox).disabled)
	# **THE SUPPRESSED FLOOR WALK, on the web whose model composes its `after` a different way.** The
	# hunt model rescales a quantised take into every account it pays, so its holding rate is built by
	# code the plant model shares none of — a gate added to one web only would leave this sheet
	# stacking the floor walk under the `ONCE TAMED` row exactly as the reported plant frame did.
	# Caption AND readings, for the reason the plant twin states.
	h._assert_hud("a composed Tame's caption keys the dip alone, as the plant web's does",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.YIELD_ROW_HEADER_WHILE_BUILDING.to_upper())
	h._assert_hud("…over readings that draw no arrow either",
		not Readout.yields_show_a_transition(h._hud._drawercompose._compose_sheet))
	# **THE HERD FORM, which is the one a shared branch gets wrong.** `abandon_improvement` targets by
	# WEB (`hunt` → herd id) while the SET verbs target by VERB — and `corral` is a herd's rung
	# addressed by a TILE, so a formatter that reused the set-verb rule would send coordinates here.
	await h._assert_abandon_emits(SourceForecast.LABOR_KIND_HUNT, HudConst.LABOR_POLICY_TAME,
		"abandon_improvement %d hunt %s" % [HudConst.PLAYER_FACTION_ID,
			String(HerdFx.taming_herd_fixture()["id"])])

	# State 442-tame-done — the animal DONE state, and **THE ONE ASYMMETRY THAT SURVIVES** (spec §4):
	# a fully tamed herd's ◎ Pastoral label carries NO upkeep, because a pastoral herd still grazes.
	# Its Corral twin below does, because a penned one cannot. The next rung's box (🐄 Corral) sits
	# beneath the label, which is what the done state is for.
	var tamed_herd := HerdFx.fully_tamed_herd_fixture()
	h._hud._compose.reset_hunt_source()
	h._show_herd(tamed_herd)
	h._compose_herd(tamed_herd)
	await h._settle()
	await h._save("improvement_done_animal")
	var pastoral_label = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "tame")
	h._assert_hud("a finished Tame is a static LABEL, not a checkbox",
		pastoral_label is Label and not (pastoral_label is CheckBox))
	h._assert_hud("…and carries NO upkeep — a pastoral herd still grazes (the asymmetry, held)",
		pastoral_label != null
		and not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).contains(UPKEEP_NEEDLE))
	h._assert_hud("…with the next rung, Corral, offered beneath it",
		ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "corral") is CheckBox)

	# State 442-corral-done — the OTHER half of that asymmetry: a PENNED herd's 🐄 label DOES carry the
	# pen's per-turn fodder upkeep, because a penned herd cannot graze and someone feeds it every turn.
	# A standing obligation belongs with the standing state. The two frames must NOT be made to match.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = [BandFx.band_fixture()]
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.domesticated_herd_fixture())
	h._compose_herd(HerdFx.domesticated_herd_fixture())
	await h._settle()
	await h._save("improvement_done_penned")
	var penned_label = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "corral")
	h._assert_hud("a finished Corral is a static LABEL",
		penned_label is Label and not (penned_label is CheckBox))
	h._assert_hud("…and DOES carry the pen's upkeep — the one asymmetry between the two webs",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL).contains(UPKEEP_NEEDLE))

	# ---- THE BUILDING HERD: A DIPPED CREW THAT CANNOT CARRY ONE BODY ------------------------------
	# **THE ANIMAL WEB'S HALF OF THE DEFECT THE PLANT SHEET HAS ALREADY LOST.** `herd_axis_rates`
	# composed its forecast at the DEFAULT improvement, so every number the local-hunt preview quoted —
	# the take, the waste split, the animals-per-turn line — was priced as though nobody were building
	# anything, while the sim pays a gentling crew `workers × per_worker × build_dip`. The worker cap,
	# the chart and both crew targets beside them already carried the verb, so the sheet disagreed with
	# the sim AND with itself.
	#
	# It is staged at the ONE regime where the dip is not a scaling: the crew crosses BELOW one body
	# mass, so `quantise_animal_take`'s `max(1, carryable)` turns the shortfall into a kill it cannot
	# haul. The two frames are judged as a PAIR — this one and the same herd with no build in flight —
	# because "every number got smaller" is exactly what a wrong fix produces too.
	var dip_herd := ForageFx.floorify(_building_herd_fixture())
	var prior_dip_band = h._hud._band_labor.player_band()
	var prior_dip_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _building_herd_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	# THE SIM'S OWN COMPOSITION, recomposed from the herd's wire terms rather than written down, so a
	# fixture that drifts fails these assertions instead of quietly re-baselining them.
	var dip_fraction := SourceForecast.build_dip(dip_herd, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME)
	var dip_fpa := float(dip_herd["food_per_animal"])
	var dip_ceiling := SourceForecast.escapement_room(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR) \
		* float(dip_herd["provisions_per_biomass"])
	var bare_collection := float(HERD_DIP_CREW) * float(dip_herd["per_worker_yield"])
	var built_collection := bare_collection * dip_fraction
	var bare_take := HerdFx.hunt_take_oracle(bare_collection, dip_ceiling, dip_fpa)
	var built_take := HerdFx.hunt_take_oracle(built_collection, dip_ceiling, dip_fpa)
	# THE NEEDLE IS THE ACCOUNT MAGNITUDE THE ROW STATES. The readout's per-turn readings are accounts
	# like every other web's — the whole-animal count is the CHART's business above it, and the raid's
	# whole-trip payload's — so the needle is spelled through `format_magnitude`, exactly as
	# `HudWidgets._yield_reading` spells the number it is aimed at.
	var bare_face := SourceForecast.format_magnitude(float(bare_take["delivered"]))
	var built_face := SourceForecast.format_magnitude(float(built_take["delivered"]))
	var built_killed: float = float(built_take["delivered"]) + float(built_take["wasted"])
	var built_waste_pct := int(round(float(built_take["wasted"]) / built_killed * 100.0))
	h._hud._compose.reset_hunt_source()
	h._show_herd(dip_herd)
	h._compose_herd(dip_herd, HERD_DIP_CREW, HERD_DIP_FLOOR, SourceForecast.IMPROVEMENT_TAME)
	await h._settle()
	await h._save("herd_build_dip")
	var dip_sheet = h._hud._drawercompose._compose_sheet
	# (0) THE FRAME REALLY IS THE REGIME, and without this every assertion below is about an ordinary
	# hunt: the crew must carry a whole body undipped and less than one under the build, which is the
	# only place `max(1, carryable)` bites and therefore the only place the waste line can move.
	h._assert_hud(("the fixture reaches the regime — %d hunters carry a whole %.2f-food body (%.2f) and "
		+ "the same crew gentling the herd carries %.2f, less than one")
		% [HERD_DIP_CREW, dip_fpa, bare_collection, built_collection],
		dip_fraction < SourceForecast.NO_BUILD_DIP and bare_collection >= dip_fpa
			and built_collection < dip_fpa)
	# (1) …AND A BUILD REALLY IS IN FLIGHT. A dip with no visible build is the stale-verb defect, a
	# different bug wearing the same numbers, so the frame states which one it is: a LIVE ticked box.
	var dip_box = ForageFx.find_improvement_control(dip_sheet, SourceForecast.IMPROVEMENT_TAME)
	h._assert_hud("…and the sheet is visibly BUILDING — a live, ticked Tame, not a stale verb",
		dip_box is CheckBox and (dip_box as CheckBox).button_pressed)
	h._assert_hud("…staffed by the composed crew (%d), so the cap is not what the frame measures"
		% HERD_DIP_CREW, Readout.stepper_value(dip_sheet) == HERD_DIP_CREW)
	# (2) **THE TAKE IS THE SIM'S DIPPED ONE.** Stated as the sim's own composition of the herd's wire
	# terms and as a RELATION to the undipped take — never as a literal — so a config retune moves the
	# fixture rather than the claim. Undipped this crew lands a whole animal a turn; it must not say so
	# while it is gentling the herd instead.
	h._assert_hud("the take is the sim's DIPPED one (%s food/turn), not the undipped %s food/turn"
		% [built_face, bare_face],
		Readout.yields_text(dip_sheet).contains(built_face)
			and not Readout.yields_text(dip_sheet).contains(bare_face))
	h._assert_hud("…and it is strictly under the take the same crew would land hunting (%.2f < %.2f food/turn)"
		% [float(built_take["delivered"]), float(bare_take["delivered"])],
		float(built_take["delivered"]) < float(bare_take["delivered"]))
	# (3) **AND THE WASTE IS WHAT MOVED**, which is the half a scaled-down take cannot produce: the crew
	# still kills one animal and leaves the part it cannot haul. A build that merely shrank the take
	# would render no waste note at all.
	h._assert_hud("…because the dipped crew kills a body it cannot carry — %d%% wasted" % built_waste_pct,
		built_waste_pct > 0
			# The readout's small print is UPPERCASED by `HudWidgets._readout_unit_label`, so every
			# needle aimed at the note/waste labels is raised here. The NUMBER labels beside them are
			# not, which is why the rate needles above are compared as written.
			and Readout.yields_text(dip_sheet).contains(
				(SourceForecast.HUNT_WASTE_NOTE_FORMAT % built_waste_pct).to_upper()))
	# (4) **THE OVERDRAW GATE WALKS THE CREW THE TAKE IS PRICED FOR.** It was asked at
	# `IMPROVEMENT_NONE` to match takes that were themselves undipped; with the takes fixed, an undipped
	# projection walks a crew four times the one being quoted. This herd's regrowth sits BETWEEN the two
	# carries, so the two answers genuinely differ here and the argument is load-bearing rather than
	# decorative.
	h._assert_hud("the overdraw gate walks the DIPPED crew — this herd grows under it, though it falls under the undipped one",
		not SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
				HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW,
				SourceForecast.IMPROVEMENT_TAME)
			and SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
				HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW,
				SourceForecast.IMPROVEMENT_NONE))
	h._assert_hud("…so the row reads renewable rather than flagging a drawdown this crew is not committing",
		Readout.yields_text(dip_sheet).contains(SourceForecast.YIELD_RENEWABLE_NOTE.to_upper())
			and not Readout.yields_text(dip_sheet).contains(
				HudComposeVocab.LOCAL_HUNT_OVERDRAW_NOTE.to_upper()))
	# (5) THE CREW ROW SAYS IT. Every number above follows from a half carry, and the sheet has to say
	# so somewhere — the plant web's rule, on the animal sheet's own dip.
	h._assert_hud("a live build states its half carry on the crew row",
		Readout.crew_row_dip_note(dip_sheet).contains(
			str(HudFormat.progress_percent(HERD_DIP_BUILD_FRACTION))))
	# (6) **THE CREW TARGETS AND THE WORKER CAP WERE ALREADY RIGHT — MEASURED, NOT ASSUMED.** They read
	# `forecast_inputs` through the chart model, which the builder has always handed the composed verb,
	# so they divide by the DIPPED carry. Pinned both ways: the rendered target is the dipped answer AND
	# it differs from the undipped one, without which the claim would pass on a sheet that ignored the
	# dip entirely.
	var dip_carry := SourceForecast.per_worker_biomass(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX) * dip_fraction
	var dip_samples := SourceForecast.regrowth_samples(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	# This herd publishes no `engageRate` — it predates the engagement stage — so both recompositions
	# state `NO_ENGAGEMENT_STAGE` and the reach arm drops out, leaving the claim about the DIP alone.
	var dip_hold := SourceForecast.crew_to_hold(dip_samples, HERD_DIP_FLOOR, dip_carry,
		HERD_DIP_BODY_MASS, SourceForecast.NO_ENGAGEMENT_STAGE, dip_fraction,
		SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF)
	var bare_hold := SourceForecast.crew_to_hold(dip_samples, HERD_DIP_FLOOR,
		dip_carry / dip_fraction, HERD_DIP_BODY_MASS, SourceForecast.NO_ENGAGEMENT_STAGE,
		SourceForecast.NO_BUILD_DIP, SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF)
	h._assert_hud("the *hold it after* target divides by the DIPPED carry (%d, against %d undipped)"
		% [dip_hold, bare_hold],
		Readout.crew_target_count(dip_sheet, HudWidgets.CREW_TARGET_HOLD) == dip_hold
			and dip_hold != bare_hold)

	# State herd_build_dip_none — THE SAME HERD WITH NO BUILD IN FLIGHT, and the half that proves the
	# first is not simply a sheet scaled down. Nothing about the animal moves: the crew lands a whole
	# body again, wastes nothing, and the ⚠ comes back — because four hunters really do out-carry this
	# herd's regrowth when they are hunting it rather than gentling it.
	h._hud._compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
	h._hud._compose.set_hunt_count(HERD_DIP_CREW)
	h._hud._drawercompose.open_herd_compose(dip_herd)
	await h._settle()
	await h._save("herd_build_dip_none")
	var bare_sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("no build in flight, no dip claimed on the crew row",
		Readout.crew_row_dip_note(bare_sheet) == "")
	h._assert_hud("…the same crew lands the whole body again (%s food/turn)" % bare_face,
		Readout.yields_text(bare_sheet).contains(bare_face)
			and not Readout.yields_text(bare_sheet).contains(built_face))
	h._assert_hud("…wasting nothing, so the waste note is a claim about the BUILD and not about the herd",
		float(bare_take["wasted"]) == 0.0
			and not Readout.yields_text(bare_sheet).contains(HUNT_WASTE_NEEDLE.to_upper()))
	h._assert_hud("…and the ⚠ returns: hunting, this crew really does draw the herd down",
		Readout.yields_text(bare_sheet).contains(HudComposeVocab.LOCAL_HUNT_OVERDRAW_NOTE.to_upper()))
	h._hud._band_labor._player_band = prior_dip_band
	h._hud._band_labor._player_bands = prior_dip_bands
	h._hud._compose.reset_hunt_source()   # the states after this one open on their own herd
