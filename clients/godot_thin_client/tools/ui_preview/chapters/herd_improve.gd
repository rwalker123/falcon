extends RefCounted

## The animal web's improvement rungs and its build dip.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 39

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## **WHAT THE PEN WOULD CARRY** — the destination capacity the climbing-floor state is staged at, well
## above the taming herd's own live `K` of 2150 so the two figures on the flag cannot be confused for
## one another. A pen is a different piece of land from the range the herd walks, which is exactly why
## the sim quotes it separately.
const PENNED_DESTINATION_CAPACITY := 3000.0

## …and the same herd on ground that would carry NOTHING once fenced. `0` is a real reading — a pen
## struck on rock — and it is the one the `-1` sentinel exists to keep apart from *no band has queued
## this herd*. Asserted, not rendered: it is a sentence in a hover, not a frame.
const BARREN_DESTINATION_CAPACITY := 0.0

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
	return RungFx.stamp_herd({
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
	})

## The band gentling it: standing ON the herd (distance 0 ≤ reach → the LOCAL branch, the only one
## that carries an improvement), `output_multiplier` 1.0 so the rendered numbers ARE the model's, and
## idle hands well clear of the composed crew so the frame measures the dip and not the labor ceiling.
## **STAMPED WITH A `band_id`** (`BandFx.with_band_id`) — a band holding `NO_BAND_ID` asks NOTHING on
## the query channel, so a sheet composed over one renders the crew-take PENDING line in place of its
## whole readout, on a HUD that is behaving correctly. It was the raid forecasts' rule and it is the
## resident take's now, which is what brought this locally-authored band under it.
func _building_herd_band_fixture() -> Dictionary:
	return BandFx.with_band_id({
		"id": "Band 1", "entity": 846, "faction": 0, "size": 90,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": HERD_DIP_IDLE_WORKERS,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	})

## The band STANDING on Tame on that herd — the fixture the re-admission frame turns on. Everything
## else about `BandFx.band_fixture` is kept; only the assignment list is replaced, by the single hunt
## assignment whose `fauna_id` matches `HerdFx.fully_tamed_herd_fixture`'s and whose policy is the rung the
## ceiling pass has since hidden.
## The BAND'S `builders` POOL — the animal twin of `BandFx.CULTIVATING_BAND_BUILDERS`, and staffed
## for the same reason: a running build is STAFFED, so a fixture that puts nobody on the pool stages a
## build nobody is paying for (`docs/plan_standing_upkeep.md` §2.5).
const TAME_STANDING_BUILDERS := 3

func _tame_standing_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["labor_assignments"] = [{
		"kind": "hunt", "workers": HerdFx.TAMED_HERD_CREW, "fauna_id": HerdFx.taming_herd_fixture()["id"],
		"floor": 0.5, "improvement": "tame", "target_x": 70, "target_y": 17,
		"actual_yield": 0.45, "sustainable_yield": 0.45,
		"workers_needed": HerdFx.TAMED_HERD_CREW, "overdraws": false,
	}, BandFx.builders_role_row(TAME_STANDING_BUILDERS)]
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
	# **A RUNNING BUILD IS A STATE, as Cultivate is** (`docs/plan_standing_upkeep.md` §2.4). It was a
	# checked, live `CheckBox` whose uncheck sent `abandon_improvement`; the verb is derived from the
	# meter now, so there is no stored intent to clear. **Asserted by STATE, not by type** — every
	# state of this control is a `Label` since §4.7a ①, so the type says nothing on its own.
	h._assert_hud("a running Tame renders the RUNNING state, as Cultivate does",
		tame_box is Label
			and String(tame_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
				== HudWidgets.IMPROVEMENT_STATE_RUNNING)
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
	# **THE BUILDERS STEPPER IS THE LEVER NOW, ON BOTH WEBS**, so what has to be present is the row —
	# the running control states the meter and the stepper beneath it is the whole of what a player can
	# change about the build.
	h._assert_hud("a running Tame mounts its BUILDERS stepper — the lever the uncheck used to be",
		Readout.stepper_count(h._hud._drawercompose._compose_sheet)
			== Readout.COMPOSE_STEPPERS_PER_SHEET)
	# **THE GEAR HALF OF THE ESTIMATE IS THE KIT'S, so its claims live with the kit** — the frames and
	# the saturation assertions are `chapters/compose_rungs.gd`'s `_kit_swap_turn_estimate_states`,
	# which is where a roster carrying the handling kit is staged. Nothing here states a gear term:
	# this chapter's band is on the stalking kit, which arms nobody for a build.
	# **THE SUPPRESSED FLOOR WALK, on the web whose model composes its `after` a different way.** The
	# hunt model rescales a quantised take into every account it pays, so its holding rate is built by
	# code the plant model shares none of — a gate added to one web only would leave this sheet
	# stacking the floor walk under the `ONCE TAMED` row exactly as the reported plant frame did.
	# Caption AND readings, for the reason the plant twin states.
	# **THE HUNT WEB'S CAPTION CARRIES ONE MORE CLAUSE, AND IT IS NOT THE DIP'S.** The take estimate
	# above these readings states a BAND; the readings themselves are single numbers, being fixed
	# conversions of one carried biomass, so the caption is what says which point of that band they are
	# quoted at (`HudComposeVocab.YIELD_HEADER_AT_LIKELY_SUFFIX`). The retired third state — *per turn ·
	# while building* — is still absent, which is what this claim is about, and the equality is what
	# keeps it a claim: a caption that grew the dip clause back would fail it as loudly as ever.
	h._assert_hud("a composed Tame's caption states the plain per-turn unit — there is no dip left to key",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== (SourceForecast.YIELD_ROW_HEADER
				+ HudComposeVocab.YIELD_HEADER_AT_LIKELY_SUFFIX).to_upper())
	h._assert_hud("…over readings that draw no arrow either",
		not Readout.yields_show_a_transition(h._hud._drawercompose._compose_sheet))
	# **THE HERD FORM, which is the one a shared branch gets wrong.** `unqueue` names a SOURCE, and its
	# two shapes are told apart exactly as the sim's parser tells them apart — two integer tokens are a
	# TILE, one token is a HERD id — so a formatter that reused the tile rule would send coordinates
	# here. The split outlived both of the walk-away forms this probe has had.
	await h._assert_walk_away_emits(SourceForecast.LABOR_KIND_HUNT, HudConst.LABOR_POLICY_TAME,
		"unqueue %d %s" % [HudConst.PLAYER_FACTION_ID,
			String(HerdFx.taming_herd_fixture()["id"])],
		{"faction": HudConst.PLAYER_FACTION_ID, "x": 70, "y": 17,
			"herd_id": String(HerdFx.taming_herd_fixture()["id"])})

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
	h._assert_hud("a finished Tame renders the DONE state",
		pastoral_label is Label and String(pastoral_label.get_meta(
			HudWidgets.IMPROVEMENT_STATE_META, "")) == HudWidgets.IMPROVEMENT_STATE_DONE)
	h._assert_hud("…and carries NO upkeep — a pastoral herd still grazes (the asymmetry, held)",
		pastoral_label != null
		and not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).contains(UPKEEP_NEEDLE))
	h._assert_hud("…with the next rung, Corral, OFFERED beneath it",
		String(ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, "corral"))
			== HudWidgets.IMPROVEMENT_STATE_OFFERED)

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
	h._assert_hud("a finished Corral renders the DONE state",
		penned_label is Label and String(penned_label.get_meta(
			HudWidgets.IMPROVEMENT_STATE_META, "")) == HudWidgets.IMPROVEMENT_STATE_DONE)
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
	# (1) A BUILD REALLY IS IN FLIGHT — the control in its RUNNING state, not a stale verb, which is
	# a different bug wearing the same numbers. Read off the state meta rather than the node type: a
	# running control and a done one are both Labels now, and this claim is about which.
	var dip_box = ForageFx.find_improvement_control(dip_sheet, SourceForecast.IMPROVEMENT_TAME)
	h._assert_hud("the sheet is visibly BUILDING — a running Tame, not a stale verb",
		dip_box != null and String(dip_box.get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
			== HudWidgets.IMPROVEMENT_STATE_RUNNING)
	h._assert_hud("…staffed by the composed TAKE crew (%d), which is the crew the take is priced for"
		% HERD_DIP_CREW, Readout.stepper_value(dip_sheet) == HERD_DIP_CREW)
	# (2) **THE TAKE IS THE UNDIPPED ONE.** Four hunters land the whole body they would land with no
	# build running at all — that is what "the build has its own crew" MEANS at the readout.
	h._assert_hud("the take is the plain one (%s food/turn) — the build takes nothing off it" % bare_face,
		Readout.yields_text(dip_sheet).contains(bare_face))
	# (3) **THE BUILD STATES ITS OWN CREW, ON ITS OWN CONTROL.** The stepper is the second allocation;
	# without it the verb would carry no count and the sim would be told to staff nobody.
	h._assert_hud("…and the build carries a crew row of its own beneath the verb",
		Readout.stepper_count(dip_sheet) == Readout.COMPOSE_STEPPERS_PER_SHEET)
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
	# (5) **THE OVERDRAW GATE IS GONE — the ⚠ is `LaborAssignment.overdraws` and nothing else.** This
	# used to assert that the client's own drawdown walk answered alike with a build and without one;
	# there is no client-side walk left to answer, so the claim is retired rather than restated. What
	# replaced it is `chapters/hunt.gd`'s three-surface agreement block.
	h._hud._band_labor._player_band = prior_dip_band
	h._hud._band_labor._player_bands = prior_dip_bands
	h._hud._compose.reset_hunt_source()   # the states after this one open on their own herd

	# State herd_floor_destination — **THE CLIMBING FLOOR, WITH SOMEWHERE TO CLIMB TO.** The flag has
	# marked a moving threshold with a bare `↑` since the build dip landed; the wire now states the
	# capacity the source will have at the rung the entry was sent to, so the flag can say what the
	# climb ends at instead of only that it is under way (`buildDestinationCapacity`).
	var penned_herd := HerdFx.taming_herd_fixture()
	penned_herd[SourceForecast.FORECAST_BUILD_DESTINATION_KEY] = SourceForecast.RUNG_KEY_PEN
	penned_herd[SourceForecast.FORECAST_BUILD_DESTINATION_CAPACITY_KEY] = \
		PENNED_DESTINATION_CAPACITY
	h._hud._band_labor._player_band = _tame_standing_band_fixture()
	h._hud._band_labor._player_bands = [_tame_standing_band_fixture()]
	h._show_herd(penned_herd)
	h._compose_herd(penned_herd)
	await h._settle()
	await h._save("herd_floor_destination")
	# **AND THE DRAWER'S OWN COMPANION TO IT.** `What taming is buying` states this herd's LIVE ceiling
	# at the top of the hover; the destination capacity is that same reading at the rung the build is
	# heading for, so it belongs in the clause that already says the ceiling is climbing rather than on
	# a line of its own. The wording carries the sim's one honesty constraint — the figure is struck on
	# TODAY'S land, the rung moving while the ground does not — so it says *as it stands today* and
	# quotes no date.
	var penned_hover := DetailFormat.husbandry_payoff_hover(penned_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	h._assert_hud("the payoff hover names the ceiling the taming is heading for, and the rung",
		penned_hover.contains("Corralled would carry ≈30 Red Deer"))
	h._assert_hud("…and states the ground it was struck on rather than promising a future",
		penned_hover.contains("on this ground as it stands today"))
	# **AND THE BREEDING LINE IS A RATE, ROUNDED AS ONE, WEARING ONE `≈`.** It filled a format that
	# carries its own `≈` with `SourceForecast.stock_face`, which carries a second — so every herd this
	# hover has ever appeared on rendered `≈≈`. The doubled glyph is asserted separately from the
	# figure because the two are independent failures of one line.
	h._assert_hud("the payoff hover wears ONE ≈ per figure, never the doubled `≈≈` (got '%s')"
		% penned_hover, not penned_hover.contains("≈≈"))
	# …and the FIGURE. `stock_face` floors its count at one body, which is right for a standing herd
	# and a lie about a per-turn curve: this range's peak regrowth is a QUARTER of a Red Deer a turn
	# and the line read `≈1`. The claim is that the fraction survives — `rate_face` carrying a decimal
	# point is the precondition, since the whole-animal rounding it replaced could not produce one.
	var payoff_prefix := HudComposeVocab.BARE_FORECAST_PREFIX
	var payoff_samples := SourceForecast.regrowth_samples(penned_herd, payoff_prefix)
	var payoff_peak := SourceForecast.regrowth_at(payoff_samples,
		SourceForecast.growth_peak_fraction(payoff_samples))
	var payoff_body := float(penned_herd.get(
		payoff_prefix + SourceForecast.FORECAST_BODY_MASS_KEY, 0.0))
	var breeding_line := DetailFormat.HUSBANDRY_PAYOFF_BREEDING_FORMAT % [
		DetailFormat.animal_rate_face(payoff_peak / payoff_body),
		SourceForecast.herd_display_name(penned_herd)]
	h._assert_hud("…and states the breeding RATE unrounded to whole animals (`%s`)" % breeding_line,
		payoff_body > 0.0 and breeding_line.contains(".")
			and penned_hover.contains(breeding_line))
	# **THE PRECONDITION, THEN THE ABSENCE.** An unqueued herd's row really does carry the `< 0`
	# sentinel — asserted first, or the silence below would pass just as well on a row that carried a
	# real destination the hover simply declined to state.
	var unqueued_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	h._assert_hud("the precondition — an unqueued herd's row carries the no-destination sentinel",
		SourceForecast.build_destination_capacity(unqueued_herd,
			HudComposeVocab.BARE_FORECAST_PREFIX) == SourceForecast.NO_BUILD_DESTINATION_CAPACITY)
	var unqueued_hover := DetailFormat.husbandry_payoff_hover(unqueued_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	h._assert_hud("…so the hover keeps the bare climbing line and quotes no ceiling at all (got '%s')"
		% unqueued_hover, unqueued_hover.contains(DetailFormat.HUSBANDRY_PAYOFF_CLIMBING))
	# **AND IT IS THE SENTINEL DOING IT, not the missing rung beside it** — this row names where it is
	# heading and prices that destination at `< 0`, so the capacity's own sentinel is the only thing
	# left to hold the ceiling back. Without this line the pair above passes on a hover that never
	# looked at the capacity at all.
	var unpriced_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	unpriced_herd[SourceForecast.FORECAST_BUILD_DESTINATION_KEY] = SourceForecast.RUNG_KEY_PEN
	var unpriced_hover := DetailFormat.husbandry_payoff_hover(unpriced_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	h._assert_hud("a NAMED destination the wire prices at the sentinel quotes no ceiling either (got '%s')"
		% unpriced_hover, unpriced_hover.contains(DetailFormat.HUSBANDRY_PAYOFF_CLIMBING))
	# **A DESTINATION THAT WOULD CARRY NOTHING IS STILL A DESTINATION** — the distinction the sentinel
	# is out of range for. A hover that swallowed this as *nothing queued* would hide the one pen the
	# player most needs talking out of.
	var barren_herd := ForageFx.floorify(HerdFx.taming_herd_fixture())
	barren_herd[SourceForecast.FORECAST_BUILD_DESTINATION_KEY] = SourceForecast.RUNG_KEY_PEN
	barren_herd[SourceForecast.FORECAST_BUILD_DESTINATION_CAPACITY_KEY] = BARREN_DESTINATION_CAPACITY
	var barren_hover := DetailFormat.husbandry_payoff_hover(barren_herd,
		HudComposeVocab.BARE_FORECAST_PREFIX)
	h._assert_hud("a pen that would carry nothing states its zero, a reading and not an absence (got '%s')"
		% barren_hover, barren_hover.contains("Corralled would carry 0 on this ground"))
	h._hud._compose.reset_hunt_source()
