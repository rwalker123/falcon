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
const Q := preload("res://tools/ui_preview/node_query.gd")

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
	# KNOWN LESSON + A BUILD IN FLIGHT, on the animal web: Herding is complete for this faction, and the
	# BUILDING half that used to survive it retired with the floor's term on the build rate
	# (`docs/plan_standing_upkeep.md` §2.2). The aside goes silent. Both halves, for the reason the
	# plant twin states.
	h._assert_hud("a known lesson is not taught again on the hunt sheet either",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_LESSON_NEEDLE))
	h._assert_hud("…and no BUILD half survives it here either, as on the plant sheet",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(Readout.TEACHING_BUILD_NEEDLE))
	h._assert_hud("a running Tame's box is LIVE too — the abandon path is ungated on both webs",
		tame_box is CheckBox and not (tame_box as CheckBox).disabled)
	# **THE GEAR HALF OF THE ESTIMATE IS THE KIT'S, so its claims live with the kit** — the frames and
	# the saturation assertions are `chapters/compose_rungs.gd`'s `_kit_swap_turn_estimate_states`,
	# which is where a roster carrying the handling kit is staged. Nothing here states a gear term:
	# this chapter's band is on the stalking kit, which arms nobody for a build.
	# **THE SUPPRESSED FLOOR WALK, on the web whose model composes its `after` a different way.** The
	# hunt model rescales a quantised take into every account it pays, so its holding rate is built by
	# code the plant model shares none of — a gate added to one web only would leave this sheet
	# stacking the floor walk under the `ONCE TAMED` row exactly as the reported plant frame did.
	# Caption AND readings, for the reason the plant twin states.
	h._assert_hud("a composed Tame's caption states the plain per-turn unit — there is no dip left to key",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.YIELD_ROW_HEADER.to_upper())
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

	# ---- THE BUILDING HERD: THE TAKE THE BUILD DOES **NOT** MOVE ---------------------------------
	# **THE DIP IS RETIRED, AND THIS PAIR IS THE CLAIM THAT REPLACED IT**
	# (`docs/plan_standing_upkeep.md` §2.2). A gentling crew used to be paid
	# `workers x per_worker x build_dip` — one crew doing two jobs, with a fraction standing in for the
	# conflict. A source carries three independent allocations now, so the hunters beside a Tame carry
	# exactly what hunters carry and the build costs the hands standing on it.
	#
	# The two frames are still judged as a PAIR, for the inverse of the old reason: "nothing moved" is
	# only a claim if the same fixture at the same crew is rendered both ways, and every number below
	# is recomposed from the herd's own wire terms rather than written down.
	var dip_herd := ForageFx.floorify(_building_herd_fixture())
	var prior_dip_band = h._hud._band_labor.player_band()
	var prior_dip_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _building_herd_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	var dip_fpa := float(dip_herd["food_per_animal"])
	var dip_ceiling := SourceForecast.escapement_room(dip_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR) \
		* float(dip_herd["provisions_per_biomass"])
	var bare_collection := float(HERD_DIP_CREW) * float(dip_herd["per_worker_yield"])
	var bare_take := HerdFx.hunt_take_oracle(bare_collection, dip_ceiling, dip_fpa)
	# THE NEEDLE IS THE ACCOUNT MAGNITUDE THE ROW STATES, spelled through `format_magnitude` exactly as
	# `HudWidgets._yield_reading` spells the number it is aimed at.
	var bare_face := SourceForecast.format_magnitude(float(bare_take["delivered"]))
	h._hud._compose.reset_hunt_source()
	h._show_herd(dip_herd)
	h._compose_herd(dip_herd, HERD_DIP_CREW, HERD_DIP_FLOOR, SourceForecast.IMPROVEMENT_TAME)
	await h._settle()
	await h._save("herd_build_crew")
	var dip_sheet = h._hud._drawercompose._compose_sheet
	# (1) A BUILD REALLY IS IN FLIGHT — a LIVE ticked box, not a stale verb, which is a different bug
	# wearing the same numbers.
	var dip_box = ForageFx.find_improvement_control(dip_sheet, SourceForecast.IMPROVEMENT_TAME)
	h._assert_hud("the sheet is visibly BUILDING — a live, ticked Tame, not a stale verb",
		dip_box is CheckBox and (dip_box as CheckBox).button_pressed)
	h._assert_hud("…staffed by the composed TAKE crew (%d), which is the crew the take is priced for"
		% HERD_DIP_CREW, Readout.stepper_value(dip_sheet) == HERD_DIP_CREW)
	# (2) **THE TAKE IS THE UNDIPPED ONE.** Four hunters land the whole body they would land with no
	# build running at all — that is what "the build has its own crew" MEANS at the readout.
	h._assert_hud("the take is the plain one (%s food/turn) — the build takes nothing off it" % bare_face,
		Readout.yields_text(dip_sheet).contains(bare_face))
	# (3) **THE BUILD STATES ITS OWN CREW, ON ITS OWN CONTROL.** The stepper is the second allocation;
	# without it the verb would carry no count and the sim would be told to staff nobody.
	h._assert_hud("…and the build carries a crew row of its own beneath the verb",
		Q.find_meta_node(dip_sheet, HudWidgets.BUILD_CREW_ROW_META) != null)
	# (4) **AND THE CREW ROW CLAIMS NO CARRY PENALTY.** The retired note read
	# "— building this rung, each carries 50% as much"; a sheet that still said so would be describing
	# arithmetic the sim no longer does.
	h._assert_hud("the crew row states the plain label alone, with no carry penalty beside it",
		not Readout.crew_row_label(dip_sheet).contains("%"))

	# State herd_build_crew_none — THE SAME HERD AT THE SAME CREW WITH NO BUILD IN FLIGHT. Under the
	# dip this frame was the control that proved the take had moved; it is now the control that proves
	# it did NOT, which is the same pair asked the opposite question.
	h._hud._compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
	h._hud._compose.set_hunt_count(HERD_DIP_CREW)
	h._hud._drawercompose.open_herd_compose(dip_herd)
	await h._settle()
	await h._save("herd_build_crew_none")
	var bare_sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("the same crew lands the same %s food/turn with no build running" % bare_face,
		Readout.yields_text(bare_sheet).contains(bare_face))
	# **THE TWO SHEETS AGREE ON THE TAKE, WHICH A `contains` PAIR ALONE CANNOT SAY.** Compared as
	# rendered text rather than as two recompositions of one oracle: an implementation that dipped one
	# side would satisfy both `contains` claims above if the dip happened to round to the same face.
	h._assert_hud("…and the two readouts state the SAME take, which is the whole of the claim",
		Readout.yields_text(bare_sheet) == Readout.yields_text(dip_sheet))
	# (5) **THE OVERDRAW GATE WALKS ONE CREW NOW.** It used to have to carry the verb so the projection
	# and the take stayed in step; with nothing to keep in step, the same crew at the same floor gets
	# the same answer whether or not a rung is going up.
	h._assert_hud("the overdraw gate answers the same for the same crew, build or no build",
		SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
			HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW)
			== SourceForecast.take_draws_down(dip_herd, SourceForecast.SOURCE_KIND_HERD,
				HudComposeVocab.BARE_FORECAST_PREFIX, HERD_DIP_FLOOR, HERD_DIP_CREW))
	h._hud._band_labor._player_band = prior_dip_band
	h._hud._band_labor._player_bands = prior_dip_bands
	h._hud._compose.reset_hunt_source()   # the states after this one open on their own herd
