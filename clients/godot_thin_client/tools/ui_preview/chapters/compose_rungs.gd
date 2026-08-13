extends RefCounted

## Compose-sheet rhythm, crew nouns and the rung gates.
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

# The stale-DATA reopen guard's herd (`herd_compose_reopen_fresh`), staged on ONE id across two turns.
# Turn N it is still WILD — not owned, so the ownership-gated crew is 0 and only the would-be crew is
# real. Turn N+1 the sim has taken ownership: a real crew of 4 (both halves equal, as the sim exports
# them on a managed herd) and a meter that has just left zero. Every number the sheet quotes moves
# between the two, and none of them is in the drawer's shape signature — which is the whole test.
const REOPEN_HERD_ID := "game_deer_reopen_01"

const REOPEN_WILD_WOULD_BE_HERDERS := 3

const REOPEN_TAMING_HERDERS := 4

const REOPEN_TAMING_DOMESTICATION := 0.04

# Idle workers comfortably above BOTH max-useful caps (3 wild / 4 taming), so the stepper is bound by
# USEFULNESS and renders the "max N workers useful here" note rather than the labor-bound one.
const REOPEN_IDLE_WORKERS := 12

const REOPEN_WORKING_AGE := 24

# The CREW-NOUN guard's pen-ready herd (`herd_compose_crew_noun_after_pen`) — the one the player ticks
# Corral on FIRST, so it is the source of the improvement the NEXT herd's header must not inherit. Its
# id is deliberately distinct from `HerdFx.herd_fixture`'s, since a same-id re-open is not a source change and
# would not stage the bug.
## The crew the kit-liveness block prices its two takes at. Deliberately small enough that LABOUR is
## the binding arm of `min(workers × per_worker, ceiling)` — at a crew that saturates the patch's own
## ceiling both kits quote the ceiling and the take stops moving, which would make the claim pass on a
## dead repricing again.
const KIT_LIVENESS_FORAGERS := 2

## **THE TRAPPING KIT, WHICH NO FIXTURE ROSTER STAGES.** `BandFx.kit_roster_fixture()` ships the three
## kits the picker's frames are judged on; a fourth entry would change the picker's contents on every
## rendered kit state, and what the hint SAYS is a string rather than a picture anyway — the sheet
## renders a plausible line whichever item it names. So the hint claim below is DRIVEN over this entry
## appended to the shipped roster, the `_assert_denial_party_needed_skips_horizon` idiom.
##
## It is the one kit that can see the defect: it supplies `attack` from `traps` where `big_game`
## supplies it from `spears`, so a hint that resolves the item from the AXIS names gear this kit does
## not carry and quotes the wrong band row's wear.
## Read off `BandFx` rather than restated: the shared band fixtures state this kit's row on their own
## `kit_tiers` answer sheet, and a second spelling of the id here is how the roster entry below and the
## row the client looks it up in come apart.
const KIT_ID_TRAPPING := BandFx.KIT_ID_TRAPPING

const KIT_TRAPPING_DISPLAY_NAME := "Trapping kit"

const CREW_NOUN_PEN_HERD_ID := "game_aurochs_crewnoun"

## The crew the WILD herd of that pair would owe if it were ever tamed — its ownership-gated count is 0.
const CREW_NOUN_WILD_WOULD_BE_HERDERS := 3

## THE STALE-DATA REOPEN PAIR (`herd_compose_reopen_fresh`) — ONE herd id on two consecutive turns.
##
## Turn N, still WILD: the meter has not moved (`domestication` 0) and nothing is owned, so the
## ownership-gated `herders_needed` is 0 while the would-be crew is a real 3. Turn N+1, TAMING has
## started: the sim has taken the herd, so the crew is real (4, both halves equal — the sim exports
## them that way on a managed herd) and the meter has just left zero.
##
## EVERYTHING ELSE IS DELIBERATELY IDENTICAL — same id, same species, same ceilings, same tile — so
## `_herd_actions_shape` cannot tell the two apart and the restate takes the same-shape PATCH path.
## What the sheet then quotes is decided entirely by whether the retained compose-open closure kept a
## captured dict or re-resolves the live one. With the base fixture's Tame ceiling (0.23) over its
## per-worker yield (0.30) the take-side max-useful is 1, so the Tame rung's cap is the herder floor
## outright: 3 on the wild herd, 4 on the taming one.
func _reopen_wild_herd_fixture() -> Dictionary:
	var fixture := HerdFx.taming_herd_fixture()
	fixture["id"] = REOPEN_HERD_ID
	fixture["label"] = "Red Deer (%s)" % REOPEN_HERD_ID
	fixture["domestication"] = 0.0
	HerdFx.price_animal_build(fixture)
	fixture[HerdFx.HERDERS_NEEDED_KEY] = 0
	fixture[HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY] = REOPEN_WILD_WOULD_BE_HERDERS
	return fixture

## The same herd one turn later, taming under way and owned — see `_reopen_wild_herd_fixture`.
func _reopen_taming_herd_fixture() -> Dictionary:
	var fixture := _reopen_wild_herd_fixture()
	fixture["domestication"] = REOPEN_TAMING_DOMESTICATION
	HerdFx.price_animal_build(fixture)
	HerdFx.set_managed_herders(fixture, REOPEN_TAMING_HERDERS)
	return fixture

## The Tame rung's running FACE at a given meter, composed through the shipped formats — the leading
## run of what the checked box says, which the stale-vs-fresh claim matches as a prefix.
##
## **THE WORK PAIR IS DERIVED FROM THE METER, exactly as the fixture derives it** — both reopen herds
## price their Tame at the ladder's own `work_cost`, so the face carries `2 / 50 work (4%)` and the
## two candidate faces differ in BOTH halves. The claim is unchanged: it is about which HERD's meter
## reached the face.
func _tame_meter_face(domestication: float) -> String:
	return HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
		FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME),
		DetailFormat.build_meter_value(
			String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[HudConst.LABOR_POLICY_TAME]),
			domestication, domestication * HerdFx.ANIMAL_TAME_WORK_COST,
			HerdFx.ANIMAL_TAME_WORK_COST)]

## Two herds a band works at once — a FAST animal (several a turn) and a BIG one (one every several
## turns) — so the Current-actions rows can show both kill-RHYTHMs. `food_per_animal` is in PROVISIONS
## (`HerdTelemetryState.foodPerAnimal`, the decoded key), matched to the assignment's food rate:
## mammoth 16 food/animal ÷ 2.4 food/turn ≈ 7 turns; fowl 2.0 ÷ 2.6 ≈ 1.3/turn.
func _hunt_rhythm_herds_fixture() -> Array:
	return [
		{"id": "game_fowl_01", "species": "Marsh Fowl", "x": 71, "y": 18, "food_per_animal": 2.0},
		{"id": "game_mammoth_01", "species": "Woolly Mammoth", "x": 70, "y": 17, "food_per_animal": 16.0},
	]

## A band worked on TWO hunt sources — the render-honesty frame for the summary row's honest per-turn
## FOOD rate (fix #1) and the under-crewed `wastedYield` note (fix #5). Row 1 is a FAST animal; row 2 a
## BIG animal whose `actualYield` is 0.00 THIS turn — the "+0.00 /turn" lie the row used to headline —
## and which is under-crewed, so the muted "· N wasted" note shows. Neither row shows a `≈… /turn`
## animals-per-turn cadence: on a summary row the sustainable food rate is enough.
func _hunt_actions_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["labor_assignments"] = [
		# Fast: honest rate 2.60/turn. A Sustain animal → the sim-answered `overdraws` is false (no ⚠).
		{"kind": "hunt", "workers": 3, "fauna_id": "game_fowl_01", "floor": 0.5,
			"target_x": 71, "target_y": 18, "actual_yield": 2.60, "sustainable_yield": 2.60,
			"workers_needed": 3, "overdraws": false},
		# Big: honest rate 2.40/turn (the sim's measured Mammoth Sustain). actual_yield 0.00 = a wait turn
		# of the kill pulse (the old lie the row used to headline). Under-crewed → the muted "· 1.9 wasted".
		# Sustain → overdraws false, so no ⚠.
		{"kind": "hunt", "workers": 2, "fauna_id": "game_mammoth_01", "floor": 0.5,
			"target_x": 70, "target_y": 17, "actual_yield": 0.00, "sustainable_yield": 2.40,
			"workers_needed": 5, "wasted_yield": 1.9, "overdraws": false},
	]
	return band

func run(harness) -> void:
	h = harness

	# ---- Hunt/husbandry render-honesty pass (intensification ladder client UX) ----------------------
	# Fix #1 + #5 — CURRENT ACTIONS rows: a summary row headlines the honest per-turn FOOD rate
	# (sustainable, not the 0.00 pulse) + the policy/status glyphs, with NO `≈… /turn` animals-per-turn
	# cadence (that lives on the compose-preview line). Both rows must read `Hunt <species> +X /turn ♻ ●`;
	# the big-game (under-crewed) row also keeps its muted "· 1.9 wasted" note (yld.muted_note, not cadence).
	h._set_world_herds(_hunt_rhythm_herds_fixture())
	h._hud.show_unit_selection(_hunt_actions_band_fixture())
	await h._settle()
	await h._save("hunt_actions_rhythm")
	h._set_world_herds(HerdFx.world_herds_fixture())
	# **THE WASTED NOTE IS THE ANIMAL WEB'S, AND THE SAME NUMBER MEANS THE OPPOSITE ON A PATCH.** One
	# wire field, two facts: on a herd `wasted_yield` is `killed − carried`, meat that really rotted;
	# on a patch it is `room − take`, stock the crew did not reach, which the sim's own note says
	# "stays in the stock and regrows". Reported from play as `0.75 wasted` sitting permanently on a
	# well-run Alluvial Plain — and permanent is the word, because `room > take` is the state the
	# compose sheet RECOMMENDS (its `hold it after` target is far below its `clear it now` one).
	#
	# **Asserted as a PAIR against ONE readout call**, not on a rendered frame: no forage fixture
	# carries a non-zero `wasted_yield`, so a frame assertion would pass with the bug fully present.
	# The hunt half is what stops the fix from being "silence the note everywhere", and the tooltip is
	# checked beside the note because the wasted text was appended to both.
	var wasted_model := {"has_yield": true, "workers": 2, "workers_needed": 0,
		"actual_yield": 0.30, "sustainable_yield": 0.30, "wasted_yield": 0.75, "overdraws": false}
	var wasted_forage := SourceForecast.source_yield_readout(
		wasted_model, SourceForecast.LABOR_KIND_FORAGE)
	var wasted_hunt := SourceForecast.source_yield_readout(
		wasted_model, SourceForecast.LABOR_KIND_HUNT)
	# **THE CLAIM IS ABOUT THE WASTE NOTE, NOT ABOUT AN EMPTY CHANNEL.** `muted_note` is a shared
	# small-print slot — the forecast's BAND rides it too since §6.4 — so an `== ""` here would start
	# failing on a patch that merely reports a stochastic take, i.e. for a reason this assertion has
	# nothing to say about. It reads the note the same way the tooltip half beside it already does.
	h._assert_hud("a PATCH states no waste — the stock it did not reach is still standing",
		not String(wasted_forage.get("muted_note", "")).contains("wasted")
			and not String(wasted_forage.get("tooltip", "")).contains("wasted"))
	h._assert_hud("…while a HERD still does, where the meat really rotted",
		String(wasted_hunt.get("muted_note", "")).contains("wasted")
			and String(wasted_hunt.get("tooltip", "")) != "")

	# Fix #2 + #1(forecast) + #6 — the LOCAL hunt compose view: the policy picker shows each rung's
	# per-turn take so Sustain < Surplus < Deplete < Eradicate reads as ASCENDING, and the live preview
	# pairs its rate with the kill-rhythm. (The stepper on a WILD herd reads "Hunters".)
	# A compact NON-food tile so the herd drawer (not a full forage tile card) lands in-frame.
	var picker_herd := HerdFx.herd_fixture()
	picker_herd["tile_info"] = HerdFx.compact_herd_tile_fixture()
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._show_herd(picker_herd)
	h._compose_herd(picker_herd, 3, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("hunt_picker_ascending")

	# Fix #6 — a MANAGED (corralled) herd's local crew are HERDERS, not a hunt party: the stepper reads
	# "Herders" so a pen whose workersNeeded scales with the herd doesn't look like a hunt-party bug.
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.domesticated_herd_fixture())
	h._compose_herd(HerdFx.domesticated_herd_fixture())
	await h._settle()
	await h._save("hunt_crew_herders")

	# Fix #4 — LEARNING knowledge visibility: Penning at 34% (0 < value < 1) must climb WITH its % in
	# the top-bar strip, not be absent-until-100. Seed Selection mid-climb too; Cultivation/Herding ✔.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "seed_selection": 0.6, "herding": 1.0, "penning": 0.34}])
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	await h._save("knowledge_penning_climbing")
	# Restore the default strip for any later frame.
	h._hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# STALE-CLOSURE GUARD (herd) — the drawer diff-cache patches a same-SHAPE restate in place and
	# DELIBERATELY keeps the compose-open button's `pressed` closure intact. Before the fix
	# `_herd_actions_shape` omitted the herd id, so switching to a DIFFERENT herd of identical structure
	# took the PATCH path and left "Assign hunters ▸" opening the PREVIOUS herd's compose (playtest: the
	# rabbit's button opened the boar's Tame sheet). Two wild huntable herds share the "assign-button, no
	# summary" shape, so the buggy patch path fires; pressing the button must open herd B's compose, not A's.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_hunt_source()
	var stale_herd_a := HerdFx.wild_herd_fixture()
	var stale_herd_b := HerdFx.wild_herd_fixture()
	stale_herd_b["id"] = "game_deer_stale_99"
	stale_herd_b["species"] = "Roe Deer"
	stale_herd_b["label"] = "Roe Deer (game_deer_stale_99)"
	# Drive the REAL drawer-actions path (`refresh_drawer_actions` calls these), settling a frame between
	# each so the diff-cache's deferred `queue_free` completes: without the settles, stale buttons linger
	# in-tree, the child-count patch test misreads, and `Q.find_button_by_text` grabs the wrong node.
	h._hud._drawercompose._clear_herd_drawer()   # drop any prior-state button so A gets a FRESH closure
	await h._settle()
	h._hud._drawercompose.build_herd_drawer_actions(stale_herd_a)   # full rebuild → button opens A
	await h._settle()
	h._hud._drawercompose.build_herd_drawer_actions(stale_herd_b)   # same shape → the patch path under test
	await h._settle()
	var stale_herd_btn = Q.find_button_by_text(
		h._hud.herd_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower())
	assert(stale_herd_btn != null)
	stale_herd_btn.pressed.emit()
	await h._settle()
	await h._save("herd_assign_button_targets_selected_herd")
	# The opened compose must be herd B (the herd now shown), never the herd A it was first wired against.
	assert(h._hud._compose.kind() == ComposeState.KIND_HERD)
	assert(h._hud._compose.subject() == String(stale_herd_b["id"]))
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()

	# STALE-CLOSURE GUARD (forage) — the identical diff-cache pattern on the forage drawer. Before the fix
	# the forage-actions shape omitted the tile subject key, so switching between two food tiles of the same
	# shape kept "Assign foragers ▸" opening the PREVIOUS tile's forage compose. Same drive, other drawer.
	h._hud._compose.reset_forage_source()
	var stale_tile_a := BaseFx.food_tile_fixture()
	var stale_tile_b := BaseFx.food_tile_fixture()
	stale_tile_b["x"] = 70
	stale_tile_b["y"] = 20
	h._hud._drawercompose._clear_forage_drawer()   # drop any prior-state button so tile A gets a FRESH closure
	await h._settle()
	h._hud._drawercompose.build_forage_drawer_actions(stale_tile_a)   # full rebuild → button opens tile A
	await h._settle()
	h._hud._drawercompose.build_forage_drawer_actions(stale_tile_b)   # same shape → the patch path under test
	await h._settle()
	# STRUCTURALLY, not by face: the open button's noun follows the patch's rung, and the bare `assert`
	# this replaces BREAKS THE HEADLESS RUN INTO THE DEBUGGER rather than reporting — measured, it hung
	# the suite the first time the noun moved under it.
	var stale_forage_btn: Button = h._forage_open_button()
	h._assert_hud("the forage drawer's open button survives a same-shape restate", stale_forage_btn != null)
	if stale_forage_btn != null:
		stale_forage_btn.pressed.emit()
	await h._settle()
	await h._save("forage_assign_button_targets_selected_tile")
	# The opened compose must be tile B (subject key "70,20"), never tile A ("66,10") it was first wired to.
	h._assert_hud("the forage drawer's button opens a FORAGE compose",
		h._hud._compose.kind() == ComposeState.KIND_FORAGE)
	h._assert_hud("…on the tile now SHOWN (70,20), not the one it was first wired against",
		h._hud._compose.subject() == "70,20")
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_forage_source()

	# STALE-DATA GUARD (herd) — herd_compose_reopen_fresh. The two guards above prove the retained
	# compose-open closure re-targets when the SUBJECT changes; this one proves it re-reads when only
	# the herd's STATE moves, which the shape signature deliberately cannot see. `_herd_actions_shape`
	# carries the herd's IDENTITY and none of its live numbers (folding `domestication` /
	# `herders_needed` in would rebuild the drawer every tick and bring back the reflow flash the patch
	# path exists to remove), so the turn taming starts the drawer PATCHES in place and the button keeps
	# its connection. A closure that CAPTURED the herd dict then feeds the sheet a pre-tame herd — the
	# playtest report where the drawer read "Domesticating 4% · Herders 3 / 4" while the sheet beside it
	# still said "This herd is 0% tamed" and "max 3 workers useful here", one whole turn behind.
	# `_live_herd` re-resolves through the selection model at PRESS time, so both surfaces read one dict.
	# A PNG alone cannot carry this claim: a fresh sheet and a stale one differ only in their numbers.
	# Knowledge is complete on both rungs, so Tame is ungated (a gated Tame would be reset to Sustain
	# and the frame would test nothing) and Corral's ONLY gate reason is the herd's own tameness — the
	# number under test, rendered as the compact one-liner rather than a bulleted list.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	var reopen_band := BandFx.band_fixture()
	reopen_band["idle_workers"] = REOPEN_IDLE_WORKERS
	reopen_band["working_age"] = REOPEN_WORKING_AGE
	h._hud._band_labor._player_band = reopen_band
	h._hud._band_labor._player_bands = [reopen_band]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_floor(SourceForecast.DEFAULT_HARVEST_FLOOR)
	var reopen_wild := _reopen_wild_herd_fixture()
	var reopen_taming := _reopen_taming_herd_fixture()
	# TURN N — select the wild herd through the real path, which fully rebuilds the drawer and wires a
	# FRESH closure onto the compose-open button.
	h._show_herd(reopen_wild)
	await h._settle()
	var reopen_btn = Q.find_button_by_text(
		h._hud.herd_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower())
	assert(reopen_btn != null)
	# Open the sheet by PRESSING the real button, then dial Tame in and press again — the second open
	# finds `hunt_key` unchanged, so the rung survives the source-changed re-seed (`_compose_herd`'s
	# double-open, done here through the button because the button's closure is what is under test).
	reopen_btn.pressed.emit()
	await h._settle()
	h._hud._compose.set_hunt_improvement(HudConst.LABOR_POLICY_TAME)
	reopen_btn.pressed.emit()
	await h._settle()
	# BASELINE — the pre-tame numbers, so the assertions below are proven to be a CHANGE and not a
	# coincidence. Both sentences are built from the shipped formats, so they read what the player reads.
	# **THE RUNNING IMPROVEMENT'S METER IS THE WITNESS** (issue #442). It was the gated Corral rung's
	# "This herd is N% tamed" reason line, which only existed while a build verb was a picker rung; the
	# meter on the checked Tame box states the SAME number, is the thing the player actually reads, and
	# is unambiguously per-herd — so a stale captured dict shows through it just as plainly.
	# **BUILT FROM THE METER FORMAT AND MATCHED AS A PREFIX.** The face carried the rung's payoff after
	# the percent for a while (`🐾 Taming — 4% · then 1.20 food`) and no longer does — the payoff
	# reads in the readout — so the prefix form is now equivalent to an `==`. It stays because the
	# claim is about the METER and nothing else, whatever else a face may later gain. The percent is
	# followed by `%` in the format, so one meter's face can never be a prefix of the other's — `0%`
	# does not lead `34%` — and the claim is as exact as the `==` was. **The face may close on the
	# sim's turn estimate now** (`— ≈ N turns`), which is another reason the match stays a prefix.
	var stale_meter := _tame_meter_face(0.0)
	var fresh_meter := _tame_meter_face(REOPEN_TAMING_DOMESTICATION)
	h._assert_hud("precondition: the WILD herd's sheet quotes its own untamed meter",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).begins_with(stale_meter))
	# The player closes the sheet and ends the turn. Closing matters: with the sheet OPEN the snapshot's
	# `refresh_compose_sheet` rebuilds it against `_selection.herd()` and self-heals, which is exactly
	# why the bug reads as "one turn behind" rather than as a permanent lie.
	h._hud._drawercompose.close_compose_sheet()
	await h._settle()
	var reopen_btn_id = reopen_btn.get_instance_id()
	# TURN N+1 — the SAME herd id restated with taming under way, through the real per-snapshot path.
	h._hud.reapply_selection("herd", reopen_taming)
	await h._settle()
	h._assert_hud("the same-herd restate PATCHES the drawer in place (the button node survives)",
		h._hud.herd_assign_controls.get_child_count() == 1
		and h._hud.herd_assign_controls.get_child(0).get_instance_id() == reopen_btn_id)
	# The crew NOUN is the second half of the report: the sim now demands keepers (`herders_needed` 4),
	# so `SourceForecast.is_managed_hunt_source` reads managed and the button — patched in place, not
	# rebuilt — flips to "Assign herders ▸", agreeing with the drawer's own "Herders: A / 4" row.
	h._assert_hud("…and its noun flips to herders, the sim having asked for keepers",
		reopen_btn.text == HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HERD_CREW_LABEL.to_lower())
	reopen_btn.pressed.emit()
	await h._settle()
	await h._save("herd_compose_reopen_fresh")
	h._assert_hud("the reopened sheet quotes the FRESH meter (4% tamed), not the captured 0%",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).begins_with(fresh_meter))
	# The HERDERS row is the second witness, and a different field entirely (`herders_needed` 0 -> 4),
	# so the two cannot both pass off one stale-or-fresh dict by coincidence.
	#
	# **IT HAS TO BE THE ROW, NOT THE NUMBER.** This searched the drawer OR the sheet for the digit "4"
	# — which the fresh meter beside it ("Tame — 4%") already contains, so it passed off the very
	# witness it was meant to be independent of, and would have passed off a coordinate or a yield just
	# as happily. The Herders row's whole rendered value is the only text that can testify: it names the
	# demand, and it names it in the row the claim is about. Assigned is READ rather than assumed — this
	# band hunts a different herd, so the deficit form is what renders, and hardcoding it would pin the
	# fixture's staffing instead of the herd's demand.
	h._assert_hud("…and the drawer's herder demand is the live one (4), not the pre-tame 0",
		Q.has_label_containing(h._hud.occupant_detail, DetailFormat.herders_label(
			h._hud._band_labor.assigned_herders_for(REOPEN_HERD_ID), REOPEN_TAMING_HERDERS)))
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_floor(SourceForecast.DEFAULT_HARVEST_FLOOR)
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = []
	h._hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# ---- THE CREW NOUN AND THE PREVIOUS HERD'S IMPROVEMENT ---------------------------------------
	# `ComposeState._hunt_improvement` is ONE slot shared by every herd, and neither `begin_hunt_source`
	# nor `reset_hunt_source` clears it — so a noun resolved from it names the crew after whichever herd
	# was composed LAST. Tick Corral on a pen-ready herd, then select a WILD one: `is_managed_hunt_source`
	# read true against the leftover, the header said `ASSIGN HERDERS`, and the stepper built by the very
	# same render — from the improvement `_build_herd_assign_controls` had just RE-SEEDED — said `Hunters`.
	# That is the disagreement `_herd_crew_noun` was written to remove, with the sides swapped.
	#
	# The two herds must differ in ID (a same-id re-open is not a source change and stages nothing) and
	# the second must be genuinely UNMANAGED — `HerdFx.herd_fixture` is 40% tamed, unpenned, owing no keepers,
	# so `is_managed_hunt_source` is false on its own axis and can only read true off the leftover.
	# A PNG carries the header; the assertions carry the stepper AGREEING with it, since a header alone
	# cannot show a disagreement.
	var crew_noun_pen := HerdFx.corral_ready_herd_fixture()
	crew_noun_pen["id"] = CREW_NOUN_PEN_HERD_ID
	crew_noun_pen["label"] = "Aurochs (%s)" % CREW_NOUN_PEN_HERD_ID
	crew_noun_pen["species"] = "Aurochs"
	var crew_noun_wild := HerdFx.herd_fixture()
	# It DECLARES the unmanaged half of the herder pair — owed no keepers (the ownership-gated 0) while
	# naming the crew it WOULD owe if tamed. That is the still-wild tameable shape, the one case the
	# field-pair guard admits an unequal pair in, and it is what makes the claim precise: this herd's own
	# axis says "hunters", so a header reading HERDERS can only have got it from the previous herd.
	crew_noun_wild[HerdFx.HERDERS_NEEDED_KEY] = 0
	crew_noun_wild[HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY] = CREW_NOUN_WILD_WOULD_BE_HERDERS
	h._set_world_herds([crew_noun_pen, crew_noun_wild])
	h._show_herd(crew_noun_pen)
	await h._settle()
	# Tick Corral on the pen-ready herd — `_compose_herd` opens, sets the axis (what the checkbox's
	# `on_toggle` writes) and re-opens, so the sheet really is composing a pen when we leave it.
	h._compose_herd(crew_noun_pen, Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, SourceForecast.IMPROVEMENT_CORRAL)
	await h._settle()
	h._assert_hud("precondition: the pen-ready herd's sheet really is composing a Corral",
		h._hud._compose.hunt_improvement() == SourceForecast.IMPROVEMENT_CORRAL)
	h._hud._drawercompose.close_compose_sheet()
	# Now the WILD herd, selected through the real path so its drawer actions rebuild.
	h._show_herd(crew_noun_wild)
	await h._settle()
	h._assert_hud("the wild herd's drawer button asks for hunters, not the penned herd's herders",
		Q.find_button_by_text(h._hud.herd_assign_controls,
			HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower()) != null)
	h._hud._drawercompose.open_herd_compose(crew_noun_wild)
	await h._settle()
	await h._save("herd_compose_crew_noun_after_pen")
	h._assert_hud("…and the sheet's eyebrow reads ASSIGN HUNTERS, not the previous herd's HERDERS",
		h._hud._drawercompose._compose_sheet._header.text.contains(
			(HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower()).to_upper()))
	# The independent half: the STEPPER names itself from the axis the sheet re-seeded, so reading that
	# axis back proves the header agrees with the stepper rather than the two being wrong together.
	h._assert_hud("…and the stepper it agrees with is built on THIS herd's own (empty) improvement axis",
		h._hud._compose.hunt_improvement() == SourceForecast.IMPROVEMENT_NONE
		and Readout.crew_row_label(h._hud._drawercompose._compose_sheet)
			== HudComposeVocab.HUNT_CREW_LABEL.to_upper())
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
	# RESTORE the roster this block replaced rather than clearing it: `_guard_frame_herd_fields` scans
	# every herd the HUD holds as each later frame renders, so emptying it here would quietly retire
	# those scans from the last states of the run.
	h._set_world_herds(HerdFx.world_herds_fixture())

	# --- **THE KIT MOVES THE SHEET'S NUMBERS — the LIVENESS half of the repricing** ---------------
	#
	# Reported from play, twice over: `Gathering kit` and `No kit` rendered IDENTICAL per-turn takes
	# and identical *clear it now* / *hold it after* pills, with only the hint line above them moving.
	# Both times the repricing was silently DEAD, and both times every existing assertion stayed green
	# — because a dead repricing returns the source unchanged, which is exactly what the fixtures were
	# tuned against. **A frame cannot see this either**: a sheet quoting one kit's numbers under
	# another kit's name is a perfectly plausible sheet.
	#
	# `band_panel_preview._assert_kit_reprices_the_source` cannot see it: it calls
	# `KitRoster.repriced_source` DIRECTLY with numeric arguments, so it exercises the arithmetic and
	# never the seam that feeds it. The second death was in that feed — `_kit_priced_source` read the
	# effective tier under one spelling and the roster's reference under another, so the reference came
	# back `0` and the substitution short-circuited. So the claim here is deliberately made through
	# `DrawerComposeController`'s own producers, at the REAL roster, and it is a claim that the numbers
	# MOVE rather than that they equal anything: what a kit is worth is the roster's business.
	var kit_band: Dictionary = h._hud._band_labor.player_band()
	var kit_patch := ForageFx.floorify(BaseFx.food_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var forage_kit_before: String = h._hud._compose.forage_kit_id()
	h._hud._compose.set_forage_kit_id(BandFx.KIT_ID_GATHERING)
	var basketed: Dictionary = h._hud._drawercompose._forage_priced_patch(kit_patch, kit_band)
	var basketed_take: Dictionary = h._hud._drawercompose._forage_yield_model(kit_band, kit_patch,
		SourceForecast.FLOOR_FOOD_PEAK, KIT_LIVENESS_FORAGERS)
	var basketed_cap := SourceForecast.max_useful_workers(
		h._hud._drawercompose._forage_forecast(kit_patch, kit_band, SourceForecast.FLOOR_FOOD_PEAK))
	h._hud._compose.set_forage_kit_id(BandFx.KIT_ID_NONE)
	var bare: Dictionary = h._hud._drawercompose._forage_priced_patch(kit_patch, kit_band)
	var bare_take: Dictionary = h._hud._drawercompose._forage_yield_model(kit_band, kit_patch,
		SourceForecast.FLOOR_FOOD_PEAK, KIT_LIVENESS_FORAGERS)
	var bare_cap := SourceForecast.max_useful_workers(
		h._hud._drawercompose._forage_forecast(kit_patch, kit_band, SourceForecast.FLOOR_FOOD_PEAK))
	h._hud._compose.set_forage_kit_id(forage_kit_before)
	# **THE SUBSTITUTION ITSELF**, by the ratio the ROSTER states — the basket's tier over the bare
	# hand's. Asserted as a ratio rather than as two magnitudes so a re-tuned `equipment.json` moves the
	# fixture and the expectation together.
	var basketed_carry := float(basketed.get(HudComposeVocab.FORAGE_FORECAST_PREFIX
		+ SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var bare_carry := float(bare.get(HudComposeVocab.FORAGE_FORECAST_PREFIX
		+ SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._assert_hud("precondition: the basketed patch states a per-worker rate at all (%s)"
		% str(basketed_carry), basketed_carry > 0.0)
	h._assert_hud("a bare-handed crew is repriced to the roster's own bare tier (%s against %s)"
			% [str(bare_carry), str(basketed_carry)],
		is_equal_approx(bare_carry * BandFx.KIT_FORAGE_CARRY_EQUIPPED,
			basketed_carry * BandFx.KIT_FORAGE_CARRY_BARE))
	# **AND IT REACHES BOTH SURFACES THE REPORT NAMED** — the per-turn readout and the crew targets.
	# The pair is the claim: the first death moved neither, the second moved neither, and a fix that
	# repriced the forecast while leaving the take on the raw patch would satisfy only the second.
	h._assert_hud("…so the PER TURN take moves with the kit (%s against %s)"
			% [String(bare_take.get(h._hud._drawercompose.YIELD_MODEL_TEXT, "")),
				String(basketed_take.get(h._hud._drawercompose.YIELD_MODEL_TEXT, ""))],
		String(bare_take.get(h._hud._drawercompose.YIELD_MODEL_TEXT, ""))
			!= String(basketed_take.get(h._hud._drawercompose.YIELD_MODEL_TEXT, "")))
	h._assert_hud("…and so does the crew the sheet asks for (%d against %d)"
		% [bare_cap, basketed_cap], bare_cap != basketed_cap)
	# **THE HUNT TWIN, on the same seam** — one `_kit_priced_source`, so a spelling that dies on one web
	# dies on both, and only a per-web assertion can say which.
	# `herd_fixture` rather than a `world_herds_fixture` row: those rows are ROSTER entries (id, species,
	# position) and state no per-worker rate, so the ratio would be `0` against `0` — which the
	# precondition beside the claim caught on the first run, and which is the whole reason it is there.
	var kit_herd: Dictionary = HerdFx.herd_fixture()
	var hunt_kit_before: String = h._hud._compose.hunt_kit_id()
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_BIG_GAME)
	var sledded: Dictionary = h._hud._drawercompose._hunt_priced_herd(kit_herd, kit_band)
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_NONE)
	var sledless: Dictionary = h._hud._drawercompose._hunt_priced_herd(kit_herd, kit_band)
	h._hud._compose.set_hunt_kit_id(hunt_kit_before)
	var sledded_carry := float(sledded.get(SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var sledless_carry := float(sledless.get(SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._assert_hud("precondition: the sledded herd states a per-worker rate at all (%s)"
		% str(sledded_carry), sledded_carry > 0.0)
	h._assert_hud("a sledless party is repriced on the hunt web too (%s against %s)"
			% [str(sledless_carry), str(sledded_carry)],
		is_equal_approx(sledless_carry * BandFx.KIT_HUNT_CARRY_EQUIPPED,
			sledded_carry * BandFx.KIT_HUNT_CARRY_BARE))

	# --- **THE HINT NAMES THE KIT'S OWN GEAR** -----------------------------------------------------
	#
	# Reported from play: selecting the Trapping kit read `attack 20.0 · carry 40.0 per hunter ·
	# spears 100 · sled 100`. It named an item that kit does not carry AND quoted the SPEARS' remaining
	# condition, so a band with fresh traps and worn-out spears read exactly backwards. The client was
	# resolving the item from the display AXIS (`attack → spears`), which cannot tell two kits apart
	# when both grant `attack` at the same tier — and `KitOption.item_ids` is the wire field that can.
	#
	# **THE PAIR IS THE CLAIM.** `big_game` alone passes under the old guess (its attack really does
	# come from spears), and `trapping` alone would pass on a hint that named every item in the world.
	# Both are asserted by EQUALITY against the vocabulary's own formats rather than by `contains`,
	# because half of what the trapping line must get right is what it does NOT say.
	_assert_kit_hint_names_the_kits_own_items()

	_assert_husbandry_hint_states_the_pen()
	_assert_the_appended_axes_read_the_band()
	_assert_a_pen_prices_on_the_keepers_carry()

	await _kit_offer_states()
	await _herd_default_kit_states()
	await _kit_swap_turn_estimate_states()

## The two hints, driven at the roster + band the sim publishes: a band carrying all four items at four
## DIFFERENT conditions, so a clause reading the wrong row quotes a visibly wrong number rather than a
## coincidentally equal one.
func _assert_kit_hint_names_the_kits_own_items() -> void:
	var roster := BandFx.kit_roster_fixture()
	roster.append({
		"id": KIT_ID_TRAPPING, "display_name": KIT_TRAPPING_DISPLAY_NAME, "jobs": [KitRoster.JOB_HUNT],
		"attack": BandFx.KIT_ATTACK_EQUIPPED,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_BARE,
		"pen_carry_per_worker_biomass": BandFx.KIT_PEN_CARRY_BARE,
		"scout_vantage_range": BandFx.KIT_SCOUT_VANTAGE_BARE,
		# The passive device, then the haul aid it shares with `big_game` — config order, weapon first.
		"item_ids": [BandFx.KIT_ITEM_TRAPS, BandFx.KIT_ITEM_SLED],
	})
	var band := BandFx.with_equipped_kit(BandFx.band_fixture())
	# The tier half of both lines is identical (the two kits grant the same numbers), which is exactly
	# why the item clauses are the only thing that can tell them apart.
	var tiers := [
		HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(BandFx.KIT_ATTACK_EQUIPPED,
			HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(BandFx.KIT_HUNT_CARRY_EQUIPPED,
			HudComposeVocab.KIT_TIER_DECIMALS),
	]
	var sled_clause := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [BandFx.KIT_ITEM_SLED,
		int(BandFx.KIT_CONDITION_SLED)]
	var big_game_want := HudComposeVocab.KIT_HINT_SEPARATOR.join(tiers + [
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [BandFx.KIT_ITEM_SPEARS,
			int(BandFx.KIT_CONDITION_SPEARS)],
		sled_clause])
	var trapping_want := HudComposeVocab.KIT_HINT_SEPARATOR.join(tiers + [
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [BandFx.KIT_ITEM_TRAPS,
			int(BandFx.KIT_CONDITION_TRAPS)],
		sled_clause])
	var big_game_got := KitRoster.tier_hint(roster,
		KitRoster.kit_by_id(roster, BandFx.KIT_ID_BIG_GAME), band, KitRoster.JOB_HUNT)
	var trapping_got := KitRoster.tier_hint(roster,
		KitRoster.kit_by_id(roster, KIT_ID_TRAPPING), band, KitRoster.JOB_HUNT)
	h._assert_hud("the big-game hint is UNCHANGED — spears then sled, at their own conditions (\"%s\")"
		% big_game_got, big_game_got == big_game_want)
	h._assert_hud("…and the trapping hint names TRAPS at the traps' condition (wanted \"%s\", got \"%s\")"
		% [trapping_want, trapping_got], trapping_got == trapping_want)
	h._assert_hud("…naming no gear it does not carry — the reported defect (\"%s\")"
		% trapping_got, not trapping_got.contains(BandFx.KIT_ITEM_SPEARS))
	# The empty list is a real answer, not a missing field: `none` wears nothing, so it states its bare
	# tiers and STOPS. Without this the whole claim is satisfiable by a hint that prints every item
	# there is — and `none` is the entry that would show it, being in the same roster as both others.
	var none_want := HudComposeVocab.KIT_HINT_SEPARATOR.join([
		HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(BandFx.KIT_ATTACK_BARE,
			HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(BandFx.KIT_HUNT_CARRY_BARE,
			HudComposeVocab.KIT_TIER_DECIMALS),
	])
	var none_got := KitRoster.tier_hint(roster, KitRoster.kit_by_id(roster, BandFx.KIT_ID_NONE),
		band, KitRoster.JOB_HUNT)
	h._assert_hud("…while a kit that carries nothing states no condition clause at all (\"%s\")"
		% none_got, none_got == none_want)

# =====================================================================================
#  A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED, AND THE TAKE IT WOULD HAVE QUOTED IS ZERO
# =====================================================================================
# Reported from play on the expanded roster. The compose sheet offered Trapping and Husbandry against
# a Red Deer as ordinary choices — pricing the trap's `dispersion 0` (nothing flees, so the take looks
# BETTER) while never applying its `attackMaxBodyMass 1.0` — and quoted a real take for a hunt that
# brings home exactly nothing: above the bound the snare grants no attack, the party falls back to the
# bare hand's 1, and the sim's `max(0, attack − defense)` refuses the hunt.
#
# **A FRAME CANNOT JUDGE THE SECOND HALF AND A NUMBER CANNOT JUDGE THE FIRST**, which is why this
# block does both. A sheet quoting a take for a kit that takes nothing is a perfectly plausible sheet;
# an entry's DISABLED flag and the reason on its face are not in the pixels a reader can check either.

## The shipped `trapping` kit's two distinguishing declarations (`equipment.json`): the snare is rated
## to hold quarry up to `attack_max_body_mass` and it scares nothing on the way in. Its ID is
## `KIT_ID_TRAPPING` above, off `BandFx` — one spelling, because the hint block and this one stage the
## same kit on two different rosters and a second copy is how they come to stage two different kits.
const TRAPPING_MAX_BODY_MASS := 1.0

const TRAPPING_DISPERSION := 0.0

## The two quarries the offer test is judged on, at the shipped roster's own numbers — a **Red Deer**
## (15 kg, `defense 1`) and a **Rabbit Warren** (0.27 kg, `defense 0`). The pair is the claim: the
## trapping kit must be greyed on the first and selectable on the second, and a rule that greyed it
## everywhere would satisfy the deer half alone.
const OFFER_DEER_BODY_MASS := 15.0
const OFFER_DEER_DEFENSE := 1.0
const OFFER_RABBIT_BODY_MASS := 0.27
const OFFER_RABBIT_DEFENSE := 0.0

## How many counting hits either animal takes. Only its being ABOVE ZERO is load-bearing — that is the
## gate's `stated` test, and a quarry the roster cannot resolve publishes `0` and must grey nothing.
const OFFER_QUARRY_DURABILITY := 40.0

## The engagement stage both quarries publish. A wild herd is stalked, which is what makes the weapon
## rule apply at all; a pen publishes `NO_ENGAGEMENT_STAGE` and is deliberately exempt.
const OFFER_QUARRY_ENGAGE_RATE := 1.0

## The sim's own identity `food_per_animal = body_mass × provisions_per_biomass`, at the deer
## fixture's 0.02 — restated for each quarry so neither states a body it could not have.
const OFFER_PROVISIONS_PER_BIOMASS := 0.02

## The crew and floor both offer frames compose at, so the two sheets differ in the ANIMAL and in
## nothing else.
const OFFER_HUNTERS := 3

## The shared roster plus the two kits the offer test needs — the passive device (mass-bounded, silent)
## and the husbandry kit (the pen axis). Both are absent from `BandFx.kit_roster_fixture()`, and adding
## them there would change what every hunt picker in both harnesses lists.
func _offer_roster() -> Array:
	var kits := _pen_axis_roster()
	kits.insert(kits.size() - 1, {
		"id": KIT_ID_TRAPPING, "display_name": "Trapping kit", "jobs": ["hunt"],
		"attack": BandFx.KIT_ATTACK_EQUIPPED,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_BARE,
		"pen_carry_per_worker_biomass": BandFx.KIT_PEN_CARRY_BARE,
		"scout_vantage_range": BandFx.KIT_SCOUT_VANTAGE_BARE,
		"build_work_per_worker": BandFx.KIT_BUILD_WORK_NEUTRAL,
		"attack_max_body_mass": TRAPPING_MAX_BODY_MASS,
		"dispersion": TRAPPING_DISPERSION,
		# **THE OFFER TEST READS THIS LIST, not the tiers.** `KitRoster.kit_supplies_any` asks whether
		# the kit carries anything at all, so a roster entry with no `item_ids` reads as the null kit
		# and is never withheld — which would make every greying claim below pass vacuously.
		"item_ids": [BandFx.KIT_ITEM_TRAPS, BandFx.KIT_ITEM_SLED],
	})
	return kits

## One quarry, carrying the three terms the fight is composed from plus the mass the weapon's window
## is tested against. Built on the shared herd fixture so the sheet renders in full.
func _offer_quarry(id: String, species: String, size_class: String, body_mass: float,
		defense: float, ceiling: String) -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd["id"] = id
	herd["label"] = "%s (%s)" % [species, id]
	herd["species"] = species
	herd["size_class"] = size_class
	herd["body_mass"] = body_mass
	herd["food_per_animal"] = body_mass * OFFER_PROVISIONS_PER_BIOMASS
	herd["defense"] = defense
	herd["durability"] = OFFER_QUARRY_DURABILITY
	herd["engage_rate"] = OFFER_QUARRY_ENGAGE_RATE
	herd["corralled"] = false
	# **THE HUSBANDRY CEILING IS A PARAMETER NOW, because the offer rule turns on it** (issue #515):
	# a kit that speeds a build is applicable to any herd with a rung left to climb. Both shipped
	# values are the real species' — a Red Deer never climbs, a Rabbit Warren pens — so the pairing
	# below is a fact about the roster rather than two hand-picked flags.
	herd["husbandry_ceiling"] = ceiling
	return herd

## Every entry the mounted kit picker is showing, as `{text, disabled}` in roster order — read off the
## LIVE `OptionButton` the sheet mounted, never off `KitRoster.build_kit_row` called a second time: an
## expectation re-derived through the builder under test asserts nothing about what was rendered.
func _picker_entries(surface: Node) -> Array:
	var picker := Q.find_meta_node(surface, KitRoster.KIT_PICKER_META) as OptionButton
	var rows: Array = []
	if picker == null:
		return rows
	for index in picker.item_count:
		rows.append({
			"text": picker.get_item_text(index),
			"disabled": picker.is_item_disabled(index),
		})
	return rows

## The one entry whose face begins with this kit's display name, `{}` when the picker does not list it.
## Matched on the PREFIX because a withheld entry carries its reason after the name and the default
## carries its `(default)` mark — both of which are part of what is being asserted.
func _picker_entry(surface: Node, display_name: String) -> Dictionary:
	for row_variant in _picker_entries(surface):
		var row: Dictionary = row_variant
		if String(row["text"]).begins_with(display_name):
			return row
	return {}

func _kit_offer_states() -> void:
	var roster := _offer_roster()
	h._hud.update_kit_roster(roster, BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	var deer := _offer_quarry("game_deer_offer", "Red Deer", "big", OFFER_DEER_BODY_MASS,
		OFFER_DEER_DEFENSE, SourceForecast.HUSBANDRY_CEILING_WILD)
	var rabbit := _offer_quarry("game_rabbit_offer", "Rabbit Warren", "small",
		OFFER_RABBIT_BODY_MASS, OFFER_RABBIT_DEFENSE, SourceForecast.HUSBANDRY_CEILING_PEN)

	# --- THE RED DEER: two of the four kits take nothing, and the sheet now says which -------------
	h._hud._compose.reset_hunt_source()
	h._show_herd(deer)
	h._compose_herd(deer, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_kit_offer_red_deer")
	var deer_sheet: Control = h._hud._drawercompose._compose_sheet
	# **AND OPEN, because the greying LIVES IN THE POPUP.** The closed face names the selected kit
	# alone, so the frame above cannot show a withheld row or the reason on it — which is the whole
	# thing this state exists to make visible. Placed by hand rather than through `show_popup()`, which
	# grabs input and can move focus mid-run; the popup is an EMBEDDED subwindow, so positioning it and
	# calling `popup()` renders it into the same viewport the capture reads (`band_panel_preview`'s
	# `band_panel_compose_deny_kit_open` idiom).
	var deer_picker := Q.find_meta_node(deer_sheet, KitRoster.KIT_PICKER_META) as OptionButton
	if deer_picker != null:
		deer_picker.get_popup().position = Vector2i(
			deer_picker.get_screen_position() + Vector2(0.0, deer_picker.size.y))
		deer_picker.get_popup().popup()
	await h._settle()
	await h._save("herd_kit_offer_red_deer_open")
	if deer_picker != null:
		deer_picker.get_popup().hide()
	var deer_rows := _picker_entries(deer_sheet)
	# **THE PRECONDITION IS THE WHOLE ROSTER.** Every claim below is an absence or a flag, and all of
	# them pass on a picker that listed nothing at all.
	h._assert_hud("precondition: the Red Deer sheet lists every hunt kit the roster carries (%d)"
		% deer_rows.size(), deer_rows.size() == 4)
	var deer_trapping := _picker_entry(deer_sheet, "Trapping kit")
	h._assert_hud("a snare cannot hold a Red Deer, so Trapping is greyed AND says why — \"%s\""
			% String(deer_trapping.get("text", "")),
		bool(deer_trapping.get("disabled", false))
			and String(deer_trapping.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % "Red Deer"))
	var deer_husbandry := _picker_entry(deer_sheet, "Husbandry kit")
	# **A RED DEER NEVER CLIMBS**, so neither of the handling kit's axes can reach it: its pen tier
	# wants a pen this species can never have, and its build axis wants a rung it can never stand on.
	# That is what keeps the withholding honest now that the build axis exists — see the warren below,
	# where the same kit on the same roster is offered.
	h._assert_hud("…and Husbandry is greyed on a wild-ceiling herd, for its OWN reason — \"%s\""
			% String(deer_husbandry.get("text", "")),
		bool(deer_husbandry.get("disabled", false))
			and String(deer_husbandry.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_PEN_ONLY))
	# **THE TWO POSITIVES ARE NOT DECORATION.** A rule that greyed every kit satisfies both claims
	# above, and `none` staying selectable is what keeps the bare-handed comparison free to run.
	var deer_stalking := _picker_entry(deer_sheet, "Stalking kit")
	h._assert_hud("…the spear line is untouched and still the kit the sheet opens on — \"%s\""
			% String(deer_stalking.get("text", "")),
		not bool(deer_stalking.get("disabled", true))
			and h._hud._compose.hunt_kit_id() == BandFx.KIT_ID_BIG_GAME)
	h._assert_hud("…and the bare-handed comparison is NEVER withheld, whatever it can hurt",
		not bool(_picker_entry(deer_sheet, "No kit").get("disabled", true)))

	# --- THE RABBIT WARREN: the same trap, the animal it was made for ------------------------------
	h._hud._compose.reset_hunt_source()
	h._show_herd(rabbit)
	h._compose_herd(rabbit, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_kit_offer_rabbit")
	var rabbit_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("the SAME trapping kit is selectable on a warren — the bound is the animal, not the kit",
		not bool(_picker_entry(rabbit_sheet, "Trapping kit").get("disabled", true)))
	# **AND THE HANDLING KIT IS OFFERED ON A HERD THAT CAN STILL CLIMB** (issue #515) — the defect
	# the build axis was added to fix. It used to be greyed here, telling the player *"what it adds is
	# only used on a penned herd"* on the very warren they were about to tame, which is exactly the
	# work hurdles and halters do. The reason was not merely unhelpful, it was false.
	#
	# **THE PAIRING IS THE CLAIM.** The deer above is still withheld on the same roster in the same
	# run, so a rule that simply stopped greying anything fails there rather than passing here.
	h._assert_hud("…while Husbandry is OFFERED on a warren, whose rungs its gear can still speed",
		not bool(_picker_entry(rabbit_sheet, "Husbandry kit").get("disabled", true)))

	_assert_a_closed_gate_quotes_zero(deer)

	# RESTORE the shared roster and the compose axis — the states after this one price against the
	# roster the prologue seeded, and a chapter that left its own in place would re-list every later
	# picker.
	h._hud.update_kit_roster(BandFx.kit_roster_fixture(), BandFx.KIT_DEFAULT_HUNT,
		BandFx.KIT_DEFAULT_FORAGE, BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_kit_id(KitRoster.NO_KIT_ID)
	h._set_world_herds(HerdFx.world_herds_fixture())

# =====================================================================================
#  THE SHEET OPENS ON THE KIT **THIS QUARRY** WANTS, AND THE PICKER MARKS THE SAME ONE
# =====================================================================================
# `HerdTelemetryState.defaultKitId` is DERIVED per species (`equipment.md` → "Which kit a QUARRY
# wants is DERIVED"), so a Rabbit Warren's sheet must open on the trap and a Red Deer's on the spear.
# It was published for a whole release with nothing client-side reading it: every sheet opened on the
# hunt JOB's default, which on a warren is a spear party losing three animals in four to the retreat.
#
# **THE WARREN IS RENDERED AFTER THE DEER DELIBERATELY, WITH NO `reset_hunt_source` BETWEEN THEM.**
# Every render writes the resolved kit back onto `ComposeState`, so the deer's `big_game` is sitting
# there as "the player's own choice" when the warren opens — and the composed choice outranks any
# default. Only the drawer's own source-change reset (`ComposeState.reset_hunt_kit`) clears it, so
# taking that path here is what makes the claim about the SECOND sheet a real one; resetting by hand
# would prove the default reachable exactly once per session.

## The two ids this pair composes on. Distinct from the offer block's, because the whole mechanism
## under test fires on a source CHANGE and re-showing one of those would not be one.
const DEFAULT_KIT_DEER_ID := "game_deer_quarry_default"

const DEFAULT_KIT_WARREN_ID := "game_rabbit_quarry_default"

## A quarry that publishes its OWN default kit — the derived per-species winner
## (`HerdTelemetryState.defaultKitId`), which is what the hunt sheet opens on.
##
## **THE TWO `*EstimatesKitId` FIELDS ARE GONE and no fixture may put them back.** They disclaimed the
## pre-launch estimate tables, which the forecast query retired; a sheet asks the sim about the kit it
## composed now, so there is no other kit's numbers to refuse.
func _quarry_defaulting_to(id: String, species: String, size_class: String, body_mass: float,
		defense: float, default_kit_id: String) -> Dictionary:
	# **A `wild` ceiling, because this block is about the DEFAULT kit and nothing else.** A climbable
	# herd would additionally offer the handling kit (issue #515), which is a true statement about a
	# different question and would only add noise to the rows these states read.
	var herd := _offer_quarry(id, species, size_class, body_mass, defense,
		SourceForecast.HUSBANDRY_CEILING_WILD)
	herd[KitRoster.HERD_DEFAULT_KIT_KEY] = default_kit_id
	return herd

func _herd_default_kit_states() -> void:
	var roster := _offer_roster()
	h._hud.update_kit_roster(roster, BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	var deer := _quarry_defaulting_to(DEFAULT_KIT_DEER_ID, "Red Deer", "big",
		OFFER_DEER_BODY_MASS, OFFER_DEER_DEFENSE, BandFx.KIT_DEFAULT_HUNT)
	var warren := _quarry_defaulting_to(DEFAULT_KIT_WARREN_ID, "Rabbit Warren", "small",
		OFFER_RABBIT_BODY_MASS, OFFER_RABBIT_DEFENSE, KIT_ID_TRAPPING)

	# --- THE BIG-GAME CONTROL: a herd whose own default IS the job's changes nothing --------------
	h._hud._compose.reset_hunt_source()
	h._show_herd(deer)
	h._compose_herd(deer, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_quarry_default_red_deer")
	var deer_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Red Deer's sheet still opens on the stalking kit (%s)"
			% h._hud._compose.hunt_kit_id(),
		h._hud._compose.hunt_kit_id() == BandFx.KIT_ID_BIG_GAME)
	h._assert_hud("…and the picker marks THAT entry — \"%s\""
			% String(_picker_entry(deer_sheet, "Stalking kit").get("text", "")),
		String(_picker_entry(deer_sheet, "Stalking kit").get("text", "")).ends_with(
			HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX))

	# --- THE WARREN: the animal the trap was made for, opened straight after the deer --------------
	h._show_herd(warren)
	h._compose_herd(warren, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_quarry_default_rabbit_warren")
	var warren_sheet: Control = h._hud._drawercompose._compose_sheet
	h._assert_hud("a Rabbit Warren's sheet opens on the TRAP the wire named for it, not the job's spear (%s)"
			% h._hud._compose.hunt_kit_id(),
		h._hud._compose.hunt_kit_id() == KIT_ID_TRAPPING)
	# **THE MARK AND THE SELECTION ARE ONE CLAIM IN TWO HALVES.** A picker that opened on the trap and
	# printed `(default)` on the spear contradicts itself on every small-game herd, so the entry that
	# must NOT carry it is asserted beside the one that must.
	h._assert_hud("…and the `(default)` mark follows it — \"%s\""
			% String(_picker_entry(warren_sheet, "Trapping kit").get("text", "")),
		String(_picker_entry(warren_sheet, "Trapping kit").get("text", "")).ends_with(
			HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX))
	h._assert_hud("…leaving the job's own default unmarked here — \"%s\""
			% String(_picker_entry(warren_sheet, "Stalking kit").get("text", "")),
		not String(_picker_entry(warren_sheet, "Stalking kit").get("text", "")).ends_with(
			HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX))

	# RESTORE, as the offer block does — the states after this one price against the prologue's roster.
	h._hud.update_kit_roster(BandFx.kit_roster_fixture(), BandFx.KIT_DEFAULT_HUNT,
		BandFx.KIT_DEFAULT_FORAGE, BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_kit_id(KitRoster.NO_KIT_ID)
	h._set_world_herds(HerdFx.world_herds_fixture())

# **THE HONESTY BLOCK IS GONE WITH THE TABLES IT GUARDED.**
#
# `_assert_the_estimate_tables_still_apply` asserted that a warren opening on the trap did not then
# refuse its own figures for being priced at the job's spear. The pre-launch tables it read
# (`estimates_apply_to`, `HERD_*_ESTIMATES_KIT_KEY`) are retired: the sim is ASKED now and answers the
# exact kit the sheet composed, so there is no other kit's raid to disown and no refusal to keep from
# over-firing. What survives of that block is the claim above it — the sheet OPENS on the kit this
# quarry wants — which is what the per-quarry default was actually for.

## **THE SECOND HALF: THE NUMBER, NOT THE LIST.** Greying is not enough on its own — the Band panel's
## raid chart reprices with no picker in sight (`BandPanelController` calls `KitRoster.priced_source`
## directly) — so the quoted take has to be honest by itself.
##
## **WEAR IS WHAT MAKES THIS REACHABLE, and it is the case the whole design turns on.** The offer test
## resolves at the FRESH tier, so a withheld kit is never the one priced; what a band CAN reach is its
## own spears run dry against a Red Deer — `max(0, 1 − 1)` is zero, the party kills nothing, and the
## sheet must quote nothing. The kit stays listed, selectable and default throughout, because the list
## may not reshuffle as gear wears.
##
## Driven rather than rendered: a per-worker rate is a number, and a sheet quoting the wrong one
## renders a perfectly plausible forecast.
func _assert_a_closed_gate_quotes_zero(deer: Dictionary) -> void:
	var kit_before: String = h._hud._compose.hunt_kit_id()
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_BIG_GAME)
	var speared := BandFx.with_equipped_kit(BandFx.hunt_preview_local_band())
	var dry := BandFx.with_bare_hands(BandFx.hunt_preview_local_band())
	var speared_rate := float(h._hud._drawercompose._hunt_priced_herd(deer, speared).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var dry_rate := float(h._hud._drawercompose._hunt_priced_herd(deer, dry).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._hud._compose.set_hunt_kit_id(kit_before)
	h._assert_hud("precondition: a speared party is quoted a real take on the Red Deer (%s)"
		% str(speared_rate), speared_rate > 0.0)
	h._assert_hud("a party whose spears are dry cannot hurt it, so the sheet quotes ZERO (%s)"
		% str(dry_rate), is_zero_approx(dry_rate))

## **THE HUSBANDRY KIT'S HINT NAMES THE PEN, AND AN ORDINARY HUNT KIT'S DOES NOT.**
##
## The pen axis reached the roster with no hint-line reader, so a player selecting Husbandry on a hunt sheet
## read `attack 1.0 · carry 40.0 per hunter · sled NN` — the SLED's condition, no pen tier at all, and
## nothing about the one item the kit exists to carry.
##
## **IT IS A 2×2 NOW, BECAUSE THE PEN LINE IS GATED ON THE SOURCE RATHER THAN ON THE KIT.** Gating it
## on the KIT printed a pen tier for a husbandry kit against a WILD herd — a tier nothing would
## read — and withheld it from a sled-only kit at a PEN, which is the one place a player needs it. So
## both kits are asked against both sources, and each of the four is an EQUALITY: half of every claim
## is what the line must NOT also say, and a `contains` would pass on a hint that stated every tier.
##
## **The pen column keeps the SLED's condition and drops the attack**, which is the sim's own split: a
## penned beast is slaughtered rather than stalked (no weapon is charged), while the slaughter charges
## the handling gear for what it butchered AND the sled for what it hauled home.
##
## **DRIVEN OVER A LOCALLY-BUILT ROSTER, and it has to be** — `BandFx.kit_roster_fixture()` carries no
## `husbandry` kit (adding one would change what every hunt picker in both harnesses lists), so no
## entry there equips the pen axis and the roster's max would equal its bare tier: a kit compared
## against itself. It is also a SENTENCE, which a frame cannot judge — the sheet renders a perfectly
## plausible hint whichever component it quotes.
##
## The expectations are spelled out here rather than recomposed through `KitRoster.tier_hint`: an
## expectation re-derived through the function under test asserts nothing.
func _assert_husbandry_hint_states_the_pen() -> void:
	var kits := _pen_axis_roster()
	# The shared fixture states a condition for EVERY item the roster ships, handling gear included,
	# so this no longer grafts one on — a second row for the same item would shadow the fixture's.
	var band := _pen_axis_band({})
	var stalking := KitRoster.kit_by_id(kits, BandFx.KIT_ID_BIG_GAME)
	var handling := KitRoster.kit_by_id(kits, HUSBANDRY_KIT_ID)
	var wild := _corral_twin(false)
	var pen := _corral_twin(true)
	var sep := HudComposeVocab.KIT_HINT_SEPARATOR
	var attack_equipped := HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS)
	var attack_bare := HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_BARE, HudComposeVocab.KIT_TIER_DECIMALS)
	var hunt_carry := HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(
		BandFx.KIT_HUNT_CARRY_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS)
	var pen_carry_equipped := HudComposeVocab.KIT_HINT_PEN_CARRY_FORMAT % String.num(
		BandFx.KIT_PEN_CARRY_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS)
	var pen_carry_bare := HudComposeVocab.KIT_HINT_PEN_CARRY_FORMAT % String.num(
		BandFx.KIT_PEN_CARRY_BARE, HudComposeVocab.KIT_TIER_DECIMALS)
	# **THE ITEM NAMES ITSELF** — the clause takes the wire's own `item_ids` entry, so there is no
	# axis→item table left for an expectation to borrow (nor for the hint to guess through).
	var spears := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_SPEARS, int(BandFx.KIT_CONDITION_SPEARS)]
	var sled := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_SLED, int(BandFx.KIT_CONDITION_SLED)]
	var handling_gear := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_HUSBANDRY_GEAR, int(BandFx.KIT_CONDITION_HUSBANDRY_GEAR)]
	# --- the WILD column: byte-identical to what this line rendered before the pen axis existed ----
	var wild_stalking := KitRoster.tier_hint(kits, stalking, band, KitRoster.JOB_HUNT, wild)
	var wild_handling := KitRoster.tier_hint(kits, handling, band, KitRoster.JOB_HUNT, wild)
	h._assert_hud("a stalking kit against a WILD herd states attack and the sled — \"%s\""
		% wild_stalking, wild_stalking == sep.join([attack_equipped, hunt_carry, spears, sled]))
	# The husbandry kit carries no spears, so it takes the bare-handed attack and states no spear
	# condition — and states NO pen tier out here, the pen being what would read one.
	h._assert_hud("…and a husbandry kit out there states no pen tier at all — \"%s\"" % wild_handling,
		wild_handling == sep.join([attack_bare, hunt_carry, handling_gear, sled]))
	# --- the PEN column: the keeper's carry, and no fight ------------------------------------------
	var pen_stalking := KitRoster.tier_hint(kits, stalking, band, KitRoster.JOB_HUNT, pen)
	var pen_handling := KitRoster.tier_hint(kits, handling, band, KitRoster.JOB_HUNT, pen)
	h._assert_hud("a stalking kit at a PEN collects at the BARE keeper's tier — \"%s\"" % pen_stalking,
		pen_stalking == sep.join([pen_carry_bare, spears, sled]))
	h._assert_hud("…and the husbandry kit states the pen AND its handling gear — \"%s\"" % pen_handling,
		pen_handling == sep.join([pen_carry_equipped, handling_gear, sled]))

## **THE PEN AND THE VANTAGE STEP DOWN WITH THE BAND'S OWN WEAR — the pair that would have caught the
## bug, and neither half proves anything alone.**
##
## Those two axes reached `KitOption` (the FRESH roster) and the cohort's flat fields long before they
## reached `BandKitTiers`, so for a while a picker asking *what would the kit under the cursor grant
## me* had nowhere to read them but the roster: a pen compose sheet quoted `pen 40.0 per keeper` for a
## band whose handling gear was dry while the sim collected 12, and a Scout card quoted `2-tile sight
## per vantage` while `calculate_visibility` revealed at 1. **Both wrong in the reassuring direction**,
## which is the direction nobody reports.
##
## **EACH AXIS IS A PAIR BECAUSE EITHER READING ALONE IS SATISFIABLE BY A CONSTANT.** A client stuck on
## the roster's fresh tier passes the equipped half; one that had stopped resolving anything passes the
## worn half. Only the two together say the number FOLLOWS the band. The worn fixtures differ from
## their fresh twins in the one item that supplies the axis and in nothing else, so a step-down
## reaching for any other item's condition moves the wrong number.
##
## The pen's worn claim is made on the whole HINT rather than on the tier, because that sentence is
## what a keeper actually reads and it carries the dry clause beside the rate; the vantage's is made on
## `role_gear`'s tier, the value the Scout card is built from.
func _assert_the_appended_axes_read_the_band() -> void:
	var pen_kits := _pen_axis_roster()
	var handling := KitRoster.kit_by_id(pen_kits, HUSBANDRY_KIT_ID)
	var pen := _corral_twin(true)
	var fresh_pen := float(KitRoster.effective_tiers(pen_kits, handling,
		_pen_axis_band({}))[KitRoster.KIT_PEN_CARRY_KEY])
	h._assert_hud("a keeper's pen tier is the EQUIPPED one while the handling gear holds (%s)"
		% str(fresh_pen), is_equal_approx(fresh_pen, BandFx.KIT_PEN_CARRY_EQUIPPED))
	var worn_hint := KitRoster.tier_hint(pen_kits, handling, _pen_axis_band({}, true),
		KitRoster.JOB_HUNT, pen)
	var want_worn := HudComposeVocab.KIT_HINT_SEPARATOR.join([
		HudComposeVocab.KIT_HINT_PEN_CARRY_FORMAT % String.num(
			BandFx.KIT_PEN_CARRY_BARE, HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_DRY_FORMAT % BandFx.KIT_ITEM_HUSBANDRY_GEAR,
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
			BandFx.KIT_ITEM_SLED, int(BandFx.KIT_CONDITION_SLED)]])
	h._assert_hud("…and once it is DRY the same pen reads the BARE keeper's tier — \"%s\"" % worn_hint,
		worn_hint == want_worn)
	# The SCOUT's axis, on the shared roster: the wayfinding kit is the one entry that equips it, so a
	# band that has worn that gear out sees one tile where the roster still advertises two.
	var kits := BandFx.kit_roster_fixture()
	var wayfinding := KitRoster.kit_by_id(kits, BandFx.KIT_ID_WAYFINDING)
	var kitted_reach := float(KitRoster.role_gear(kits, wayfinding,
		BandFx.with_equipped_kit({}), KitRoster.JOB_SCOUT)[KitRoster.ROLE_GEAR_TIER_KEY])
	var bare_reach := float(KitRoster.role_gear(kits, wayfinding,
		BandFx.with_bare_hands({}), KitRoster.JOB_SCOUT)[KitRoster.ROLE_GEAR_TIER_KEY])
	h._assert_hud("a posted vantage sees the EQUIPPED range while the wayfinding gear holds (%s)"
		% str(kitted_reach), is_equal_approx(kitted_reach, BandFx.KIT_SCOUT_VANTAGE_EQUIPPED))
	h._assert_hud("…and the BARE range once it is spent, not the roster's fresh one (%s)"
		% str(bare_reach), is_equal_approx(bare_reach, BandFx.KIT_SCOUT_VANTAGE_BARE))

## **A PEN IS COLLECTED ON THE KEEPER'S CARRY, NOT THE HUNTER'S** — the pricing half of the same rule,
## and the one that moves a number rather than a sentence.
##
## Reported as a gap in the sim's own notes: a corralled herd is worked from a Hunt row, so an axis
## keyed by JOB priced a pen on the SLED's tier while the sim collects one on `EquipmentStat::PenCarry`.
## **On the shipped roster the two errors CANCEL** — husbandry and stalking both carry a sled, so both
## sat at the sled's equipped tier and every hunt kit quoted a pen the same number. That is why the
## claim is a TRIPLE and not a single: the pen pair alone would be satisfied by a fix that priced
## everything on the pen axis, and the wild reading alone by no fix at all.
##
## **DRIVEN THROUGH `DrawerComposeController._hunt_priced_herd`, the real seam**, for the reason the
## kit-liveness block above records: the two deaths this feature has had were both in the FEED, and a
## direct `KitRoster` call exercises the arithmetic without ever reaching it. The roster is installed
## and restored around the block, `BandFx.kit_roster_fixture()` carrying no husbandry kit, so no frame
## after it renders a picker this block put there.
func _assert_a_pen_prices_on_the_keepers_carry() -> void:
	var kits_before: Array = h._hud._band_labor.kits()
	h._hud.update_kit_roster(_pen_axis_roster(), BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	var band := _pen_axis_band(BandFx.hunt_preview_local_band())
	var wild := _corral_twin(false)
	var pen := _corral_twin(true)
	var published := float(pen.get(SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var kit_before: String = h._hud._compose.hunt_kit_id()
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_BIG_GAME)
	var sled_in_the_wild := float(h._hud._drawercompose._hunt_priced_herd(wild, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var sled_at_the_pen := float(h._hud._drawercompose._hunt_priced_herd(pen, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._hud._compose.set_hunt_kit_id(HUSBANDRY_KIT_ID)
	var handling_at_the_pen := float(h._hud._drawercompose._hunt_priced_herd(pen, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._hud._compose.set_hunt_kit_id(kit_before)
	h._hud.update_kit_roster(kits_before, BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	h._assert_hud("precondition: the herd states a per-worker rate at all (%s)" % str(published),
		published > 0.0)
	# THE WILD READING IS UNCHANGED — a sled still hauls a carcass in off the range at the reference.
	h._assert_hud("a stalking kit on the WILD twin still prices at the SLED's tier (%s of %s)"
		% [str(sled_in_the_wild), str(published)], is_equal_approx(sled_in_the_wild, published))
	# …AND THE PEN IS PRICED ON THE KEEPER'S. Stated as the roster's own ratio rather than as two
	# magnitudes, so a re-tuned `equipment.json` moves the fixture and the expectation together.
	h._assert_hud("the husbandry kit collects the PEN at the reference (%s of %s)"
		% [str(handling_at_the_pen), str(published)],
		is_equal_approx(handling_at_the_pen, published))
	h._assert_hud("…and a sled-only kit collects the same pen at the BARE keeper's tier (%s against %s)"
			% [str(sled_at_the_pen), str(handling_at_the_pen)],
		is_equal_approx(sled_at_the_pen * BandFx.KIT_PEN_CARRY_EQUIPPED,
			handling_at_the_pen * BandFx.KIT_PEN_CARRY_BARE))
	# The headline, in the direction the report named: the pen under-quoted the very kit it exists for.
	h._assert_hud("…so the handling gear is worth MORE at a pen than the sled is (%s against %s)"
		% [str(handling_at_the_pen), str(sled_at_the_pen)], handling_at_the_pen > sled_at_the_pen)
	_assert_the_gear_row_states_the_build_it_speeds(band)

## **THE HANDLING GEAR'S ROW SAYS BOTH THE JOBS IT DOES** (issue #515). It bounds a slaughter at a pen
## AND takes work off the `Tame` and `Corral` builds, and a row quoting only the pen rate describes the payoff
## at the top of the ladder while saying nothing about the climb that produces it — which is the whole
## complaint the build axis was added to answer.
##
## **ASKED THREE WAYS, because each alone passes on a broken renderer.** Present when the band's hunt
## kit carries the gear; ABSENT on the same band reading a kit that does not (or a suffix stamped on
## every row passes the first); and ABSENT again when the gear is DRY (or a clause read off the fresh
## ROSTER instead of the band's own worn row passes the first two).
func _assert_the_gear_row_states_the_build_it_speeds(band: Dictionary) -> void:
	var clause := DetailFormat.KIT_ROLE_BUILD_WORK_SUFFIX % String.num(
		BandFx.KIT_BUILD_WORK_HANDLING, DetailFormat.KIT_BUILD_WORK_DECIMALS)
	var on_handling := band.duplicate(true)
	on_handling[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = HUSBANDRY_KIT_ID
	var geared_line := _gear_row(on_handling)
	h._assert_hud("the handling gear's row states the build it speeds (%s) — \"%s\""
		% [clause, geared_line], geared_line.contains(clause))
	var on_stalking := band.duplicate(true)
	on_stalking[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = BandFx.KIT_ID_BIG_GAME
	var bare_line := _gear_row(on_stalking)
	h._assert_hud("…and NOT on a band whose hunt job left the gear at camp — \"%s\"" % bare_line,
		not bare_line.contains(clause))
	var dry := _pen_axis_band(BandFx.hunt_preview_local_band(), true)
	dry[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = HUSBANDRY_KIT_ID
	var dry_line := _gear_row(dry)
	h._assert_hud("…nor once the gear is spent, which takes no work off any job — \"%s\""
		% dry_line, not dry_line.contains(clause))
	# **LIVENESS**: every claim above but the first is an absence, and all three pass on a popover
	# that stopped rendering the row at all.
	h._assert_hud("…while all three really rendered the handling gear's row",
		geared_line != "" and bare_line != "" and dry_line != "")

## The handling gear's line out of the band's own gear breakdown, `""` when no row carries the label.
func _gear_row(band: Dictionary) -> String:
	for line in h._hud._disclosures.kit_breakdown_lines(band):
		if String(line).contains(DetailFormat.KIT_LABEL_HUSBANDRY_GEAR):
			return String(line)
	return ""

## The ONE herd both pen blocks are asked against, in its two states. `corralled` is the only
## difference between the two dicts, so anything the sheet says differently about them is the pen's.
func _corral_twin(corralled: bool) -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd[KitRoster.QUARRY_CORRALLED_KEY] = corralled
	return herd

## The shipped `husbandry` kit's id (`equipment.json`). The item behind the pen axis rides the shared
## band fixture now, so this chapter no longer names it.
const HUSBANDRY_KIT_ID := "husbandry"

## The shared roster plus the `husbandry` kit the harness's own picker states must not see: the ONE
## entry that equips the pen axis, so `KitRoster.equipped_tier` answers 40 and the offer test's own
## `kit_uses` axis-supply check can tell the two hunt kits apart. Every other axis on it is the roster's own bare tier, the wire's shape.
func _pen_axis_roster() -> Array:
	var kits := BandFx.kit_roster_fixture()
	kits.insert(kits.size() - 1, {
		"id": HUSBANDRY_KIT_ID, "display_name": "Husbandry kit", "jobs": ["hunt"],
		"attack": BandFx.KIT_ATTACK_BARE,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_BARE,
		"pen_carry_per_worker_biomass": BandFx.KIT_PEN_CARRY_EQUIPPED,
		"scout_vantage_range": BandFx.KIT_SCOUT_VANTAGE_BARE,
		# **AND THE BUILD AXIS, which is what makes this kit applicable before a pen exists.** Its
		# pen tier above is read on a corralled herd and nowhere else; this one is read on any herd
		# with a rung left to climb, which is the work hurdles and halters are physically for.
		"build_work_per_worker": BandFx.KIT_BUILD_WORK_HANDLING,
		# Handling gear, then the sled it also carries — config order, and the list the hint's condition
		# clauses are read off. See the trapping entry above for why an entry without one is inert.
		"item_ids": [BandFx.KIT_ITEM_HUSBANDRY_GEAR, BandFx.KIT_ITEM_SLED],
	})
	return kits

## The band both pen blocks are asked about: the shared kitted fixture PLUS a `kit_tiers` row for the
## husbandry kit, which `BandFx` cannot state because no roster it ships offers that kit.
##
## **A KIT WITH NO ROW READS AS `stated == false`**, and then `KitRoster.effective_tiers` falls back to
## the roster's fresh tiers and the hint prints NO condition clause — so without this the handling
## kit's whole gear half would be silently absent and both hint expectations would be asserting a
## line the client had stopped building.
##
## **THE ROW STATES ALL FIVE AXES, THE PEN INCLUDED, because the wire's row does.** It did not while
## `BandKitTiers` carried three, and the pen therefore came off the ROSTER's fresh tier — which is
## how a keeper with dry handling gear was quoted 40. `handling_gear_dry` is the other half of that
## pair: the same band with the gear worn out, its pen row stepped down to the bare rate the way the
## sim steps it down, so a client that went back to reading the roster reads 40 against a fixture
## that says 12.
##
## **`arms_crew` IS THE GEAR'S SATURATING CREW, and it is a parameter because a band's HOLDINGS are
## what it states.** The default is the shared fixture's two sets of hurdles; the over-geared frame
## hands it a party's worth, which is the only way a 50-unit Tame can be covered by the gear alone at
## a crew the stepper will admit. Dry gear arms nobody whatever this says — a worth with no crew
## behind it would credit a build the band cannot staff.
func _pen_axis_band(band: Dictionary, handling_gear_dry: bool = false,
		arms_crew: int = BandFx.KIT_BUILD_SATURATING_CREW_HANDLING) -> Dictionary:
	var kitted := BandFx.with_equipped_kit(band)
	var rows: Array = kitted.get(KitRoster.BAND_KIT_TIERS_KEY, [])
	rows.append({
		KitRoster.BAND_KIT_TIERS_ID_KEY: HUSBANDRY_KIT_ID,
		KitRoster.KIT_ATTACK_KEY: BandFx.KIT_ATTACK_BARE,
		KitRoster.KIT_HUNT_CARRY_KEY: BandFx.KIT_HUNT_CARRY_EQUIPPED,
		KitRoster.KIT_FORAGE_CARRY_KEY: BandFx.KIT_FORAGE_CARRY_BARE,
		KitRoster.KIT_PEN_CARRY_KEY: (BandFx.KIT_PEN_CARRY_BARE if handling_gear_dry
			else BandFx.KIT_PEN_CARRY_EQUIPPED),
		# The handling gear buys the PEN and nothing else, so this kit's vantage is the bare one
		# whatever state that gear is in — a keeper's tools do not help a scout see further.
		KitRoster.KIT_SCOUT_VANTAGE_KEY: BandFx.KIT_SCOUT_VANTAGE_BARE,
		# **AND THE BUILD AXIS, which steps down WITH the gear** (issue #515). Spent hurdles take no
		# work off a build, so dry gear reads exactly as bare hands do — which is why the readout
		# drops the clause entirely rather than printing `0 work`.
		KitRoster.KIT_BUILD_WORK_KEY: (BandFx.KIT_BUILD_WORK_NEUTRAL if handling_gear_dry
			else BandFx.KIT_BUILD_WORK_HANDLING),
		# **AND HOW MANY KEEPERS IT ARMS — the axis's other half**, which the compose sheet's turn
		# estimate saturates its gear term at. Dry gear arms nobody, so the pair steps down together:
		# a worth with no crew behind it would credit a build the band cannot staff.
		KitRoster.KIT_BUILD_SATURATING_CREW_KEY: (BandFx.KIT_BUILD_SATURATING_CREW_NONE
			if handling_gear_dry else arms_crew),
	})
	kitted[KitRoster.BAND_KIT_TIERS_KEY] = rows
	if handling_gear_dry:
		kitted[KitRoster.BAND_ITEM_CONDITIONS_KEY] = _dry_handling_gear_conditions(kitted)
	return kitted

## The shared fixture's condition list with the HANDLING GEAR alone run to `CONDITION_DRY`. Every
## other item keeps its own number, which is what makes the worn band's hint assertable: only the one
## clause changes, so a line that moved anything else is a line that read the wrong item.
func _dry_handling_gear_conditions(band: Dictionary) -> Array:
	var out: Array = []
	for row_variant in band.get(KitRoster.BAND_ITEM_CONDITIONS_KEY, []):
		var row: Dictionary = (row_variant as Dictionary).duplicate()
		if String(row.get(KitRoster.ITEM_CONDITION_ID_KEY, "")) == BandFx.KIT_ITEM_HUSBANDRY_GEAR:
			row[KitRoster.ITEM_CONDITION_REMAINING_KEY] = KitRoster.CONDITION_DRY
		out.append(row)
	return out

# =====================================================================================
#  THE TURN ESTIMATE MOVES WITH THE KIT, NOT ONLY WITH THE CREW
# =====================================================================================
# The compose sheet evaluates `turns(workers)` itself, and its GEAR term is
# `min(workers, the kit's saturating crew) × that kit's per-worker worth` — **both halves off the kit
# row the picker is OFFERING**, which is what makes a kit swap re-price the whole estimate. The crew
# A/B in `chapters/improvements.gd` exercises the ungeared arm and nothing else (no plant item
# declares the build stat yet), so this is the half of the form no frame in that set can reach.
#
# **THE PAIR IS THE CLAIM, at ONE crew on ONE herd.** A gear term read off a WORKED SOURCE rather
# than off the kit row answers the same number for both kits — a perfectly plausible sheet, quoting
# the committed crew's tools under a name the player has just changed — and only the second frame can
# tell that from a working one.

## The crew both kit frames staff. **Above the handling gear's saturating crew on purpose**, so the
## `min` is doing real work in the geared frame rather than being inert.
const KIT_SWAP_KEEPERS := 3

## What that crew owes on an UNSTARTED Tame under each kit, derived HERE from the fixtures rather
## than through the producer under test: the rung costs `HerdFx.ANIMAL_TAME_WORK_COST` (50) with
## nothing banked, the floor sits at the food peak (×1.0) and one keeper banks one work unit a turn.
## The stalking kit arms nobody for a build, so ⌈50 ÷ 3⌉; the handling gear arms two of the three at
## 8.5 apiece, so ⌈(50 − 17) ÷ 3⌉.
const KIT_SWAP_TURNS_BARE := 17

const KIT_SWAP_TURNS_GEARED := 11

## …and what ONE MORE keeper owes under the handling gear, ⌈(50 − 17) ÷ 4⌉ — the gear term unmoved,
## because the fourth keeper finds no hurdles left to carry. Beside it, what a `min` dropped from the
## head count would quote that crew instead (`4 × 8.5` off the job, so ⌈16 ÷ 4⌉): stated so the
## negative names a number rather than merely differing.
const KIT_SWAP_TURNS_SATURATED := 9

const KIT_SWAP_TURNS_UNCAPPED := 4

## The herd both frames are composed on — a warren, which is the ceiling that keeps the handling kit
## OFFERED (a wild-ceiling herd greys it, see `_kit_offer_states`), with its Tame priced and unstarted
## so the OFFERED face carries the quote rather than a running meter.
func _kit_swap_herd() -> Dictionary:
	var herd := _offer_quarry(KIT_SWAP_HERD_ID, "Rabbit Warren", "small", OFFER_RABBIT_BODY_MASS,
		OFFER_RABBIT_DEFENSE, SourceForecast.HUSBANDRY_CEILING_PEN)
	herd["domestication"] = KIT_SWAP_UNSTARTED_TAME
	return HerdFx.price_animal_build(herd)

const KIT_SWAP_HERD_ID := "game_warren_kitswap"

## Nothing banked on the Tame, so the quote is the whole job and the two frames differ by the gear
## alone rather than by where a part-built meter happened to stand.
const KIT_SWAP_UNSTARTED_TAME := 0.0

## **THE OVER-GEARED CREW, and both halves of it are the fixture's claim.** Six keepers at the handling
## gear's 8.5 apiece take 51 work off a 50-unit Tame — the shipped start-stock case, a band holding 26
## `husbandry_gear` units — so the bar is at or below zero and the job finishes on the first worked
## turn. The crew and the gear's saturating crew are EQUAL on purpose: the `min` on the head count is
## already pinned by the saturation claim above, and a mismatch here would leave this frame asserting
## that term a second time instead of the answer at the boundary.
const OVER_GEARED_KEEPERS := 6

const OVER_GEARED_ARMS_CREW := OVER_GEARED_KEEPERS

## The OFFERED face's price CLAUSE alone — `50 work, ≈17 turns` — composed through the shipped
## formats, so the assertion pins the count this chapter derived and not the wording.
func _kit_swap_price_clause(turns: int) -> String:
	return HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [
		HudComposeVocab.BUILD_PRICE_WORK_FORMAT % DetailFormat.format_work_units(
			HerdFx.ANIMAL_TAME_WORK_COST),
		HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % turns]

## …and its SINGULAR twin — `50 work, ≈1 turn`. Spelled from the count vocabulary's own singular
## rather than through `DetailFormat.build_turns_clause`, which is the fork under test.
func _kit_swap_price_clause_one() -> String:
	return HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [
		HudComposeVocab.BUILD_PRICE_WORK_FORMAT % DetailFormat.format_work_units(
			HerdFx.ANIMAL_TAME_WORK_COST),
		HudComposeVocab.BUILD_TURNS_COUNT_ONE]

func _kit_swap_turn_estimate_states() -> void:
	h._hud.update_kit_roster(_offer_roster(), BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	# The band has to publish a `kit_tiers` row for BOTH kits, or the geared frame reads the ungeared
	# answer for a reason that has nothing to do with the picker.
	var keepers := _pen_axis_band(BandFx.hunt_preview_local_band())
	h._hud._band_labor._player_band = keepers
	h._hud._band_labor._player_bands = [keepers]
	# Tame has to be OFFERED, which is a knowledge gate: an un-learned rung renders its reason instead
	# of its price, and there would be no clause to read on either frame.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
		"foddering": 1.0,
	}])
	var warren := _kit_swap_herd()

	#   (a) THE STALKING KIT — a drag harness takes no work off gentling an animal.
	h._hud._compose.reset_hunt_source()
	h._show_herd(warren)
	h._compose_herd(warren, KIT_SWAP_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_BIG_GAME)
	# **THE ESTIMATE IS QUOTED AT THE BUILD'S OWN CREW** (`docs/plan_standing_upkeep.md` §2.2), so the
	# frames dial the builders rather than the take crew — and dial them AFTER the first open, the
	# `_compose_herd` re-open contract, since a source change re-seeds the composition. The gear term
	# is resolved over these same hands, which is the whole of what the kit swap moves.
	h._hud._compose.set_hunt_build_count(KIT_SWAP_KEEPERS)
	h._compose_herd(warren)
	await h._settle()
	await h._save("herd_kit_swap_bare_build")
	var bare_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)

	#   (b) THE HANDLING KIT — the SAME herd, the SAME crew, the SAME floor. Only the picker moved.
	h._hud._compose.set_hunt_kit_id(HUSBANDRY_KIT_ID)
	h._compose_herd(warren)
	await h._settle()
	await h._save("herd_kit_swap_geared_build")
	var geared_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)
	print("ui_preview: kit swap  bare=%s  geared=%s" % [bare_face, geared_face])

	h._assert_hud("a crew whose kit helps no build is quoted the whole job — \"%s\"" % bare_face,
		bare_face.ends_with(_kit_swap_price_clause(KIT_SWAP_TURNS_BARE)))
	h._assert_hud("…and the handling gear takes work off it, at the SAME crew — \"%s\"" % geared_face,
		geared_face.ends_with(_kit_swap_price_clause(KIT_SWAP_TURNS_GEARED)))
	# The negative that names the defect: a gear term read off the SOURCE rather than off the kit row
	# answers one number for both kits, which is what the two claims above spell as two counts.
	h._assert_hud("…so the estimate cannot read the same under both kits",
		KIT_SWAP_TURNS_BARE != KIT_SWAP_TURNS_GEARED and bare_face != geared_face)
	# **THE `min` IS ON THE HEAD COUNT, and it is asked of the PRODUCER** — a crew above the gear's own
	# saturating crew cannot be staged on a frame without putting the claim at the mercy of the
	# stepper's cap. A fourth keeper carries no hurdles, so the gear term does not grow with them.
	var geared := KitRoster.build_gear(keepers, HUSBANDRY_KIT_ID)
	var overstaffed := SourceForecast.build_turns_at(warren, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, KIT_SWAP_KEEPERS + 1, SourceForecast.FLOOR_FOOD_PEAK,
		geared)
	h._assert_hud("a keeper past the gear's own crew adds no gear, the term having saturated",
		overstaffed == KIT_SWAP_TURNS_SATURATED)
	h._assert_hud("…and NOT the shorter job an uncapped gear line would credit that crew with",
		KIT_SWAP_TURNS_SATURATED != KIT_SWAP_TURNS_UNCAPPED
			and overstaffed != KIT_SWAP_TURNS_UNCAPPED)

	#   (c) **THE GEAR COVERS THE WHOLE JOB — and the answer is ONE TURN, not "no estimate".** The same
	# warren and the same handling kit, over a band holding a PARTY'S worth of hurdles: six armed
	# keepers take `6 × 8.5` = 51 off a 50-unit Tame, so the bar is already at or below zero and the
	# build completes on the first worked turn (`docs/plan_unit_costed_work.md` §6.2). Quoting
	# `BUILD_TURNS_NO_ESTIMATE` there blanked the clause at exactly the crew that demonstrates *add
	# hands and watch it drop* — the estimate fell 25 → 13 → 4 → 2 → nothing — while the tile card beside
	# it, reading the sim's own answer, said `≈1 turn at this crew`.
	var stocked := _pen_axis_band(BandFx.hunt_preview_local_band(), false, OVER_GEARED_ARMS_CREW)
	h._hud._band_labor._player_band = stocked
	h._hud._band_labor._player_bands = [stocked]
	h._hud._compose.reset_hunt_source()
	var stocked_warren := _kit_swap_herd()
	h._show_herd(stocked_warren)
	h._compose_herd(stocked_warren, OVER_GEARED_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_hunt_kit_id(HUSBANDRY_KIT_ID)
	# The BUILD's crew, dialled after the open — see (a) above. The gear is resolved over these hands,
	# so the "gear alone pays the job off" regime is a claim about the BUILDERS' coverage.
	h._hud._compose.set_hunt_build_count(OVER_GEARED_KEEPERS)
	h._compose_herd(stocked_warren)
	await h._settle()
	await h._save("herd_kit_swap_over_geared")
	var over_geared_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)
	print("ui_preview: over-geared build  face=%s  crew=%d" % [
		over_geared_face, Readout.stepper_value(h._hud._drawercompose._compose_sheet)])
	# The PRECONDITION: the crew the sheet actually composed is the one whose gear covers the job. A
	# stepper clamped below it would leave every claim below describing a different, ordinary build.
	h._assert_hud("the sheet really staffs the over-geared crew, so the bar is at or below zero",
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == OVER_GEARED_KEEPERS
			and float(OVER_GEARED_KEEPERS) * BandFx.KIT_BUILD_WORK_HANDLING
				>= HerdFx.ANIMAL_TAME_WORK_COST)
	h._assert_hud("a job the gear alone pays off quotes ONE turn — \"%s\"" % over_geared_face,
		over_geared_face.ends_with(_kit_swap_price_clause_one()))
	# The NEGATIVE that names the defect: withholding the clause renders the bare price, which is a
	# perfectly plausible face and the one this frame exists to refuse.
	h._assert_hud("…and never the bare price a withheld estimate would leave behind",
		not over_geared_face.ends_with(HudComposeVocab.BUILD_PRICE_WORK_FORMAT
			% DetailFormat.format_work_units(HerdFx.ANIMAL_TAME_WORK_COST)))
	h._hud._band_labor._player_band = keepers
	h._hud._band_labor._player_bands = [keepers]
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
