extends RefCounted

## Compose-sheet rhythm, crew nouns and the rung gates.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 104

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")

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
	return RungFx.stamp_herd(fixture)

## The same herd one turn later, taming under way and owned — see `_reopen_wild_herd_fixture`.
func _reopen_taming_herd_fixture() -> Dictionary:
	var fixture := _reopen_wild_herd_fixture()
	fixture["domestication"] = REOPEN_TAMING_DOMESTICATION
	HerdFx.price_animal_build(fixture)
	HerdFx.set_managed_herders(fixture, REOPEN_TAMING_HERDERS)
	return RungFx.stamp_herd(fixture)

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
	var fresh_meter := _tame_meter_face(REOPEN_TAMING_DOMESTICATION)
	# **THE PRECONDITION IS THE ABSENCE OF THE FRESH METER, NOT THE PRESENCE OF A `0%` ONE.** A wild
	# herd with Tame declared and nobody on it has nothing in flight, so the control is the DECLARED
	# checkbox and quotes no meter at all — the whole point of that state, and what makes a stale
	# declaration re-tickable. What the baseline has to establish is only that the sheet is NOT already
	# quoting the taming herd's 4%, or the claim below would pass by coincidence.
	h._assert_hud("precondition: the WILD herd's sheet is a DECLARED choice, nothing in flight",
		String(ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).get_meta(HudWidgets.IMPROVEMENT_STATE_META, ""))
			== HudWidgets.IMPROVEMENT_STATE_DECLARED)
	h._assert_hud("…and it quotes no meter at all, least of all the taming herd's",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			HudConst.LABOR_POLICY_TAME).contains(fresh_meter))
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
	# **THE SECOND WITNESS IS THE BUTTON'S NOUN, asserted above** — a different field entirely
	# (`herders_needed` 0 → 4, through `SourceForecast.is_managed_hunt_source`), so the two cannot both
	# pass off one stale-or-fresh dict by coincidence.
	#
	# **IT WAS THE `Keepers:` ROW, AND THAT ROW IS RETIRED** (issue #545). It stated a standing demand
	# every turn on a herd where nothing was wrong, beside a `Keeping:` row saying the same number
	# again, and reported from play neither could be read. What a head count is FOR is *am I short*,
	# which the shed sentence and the rung row's own ⚠ now carry — so this asserts the retirement
	# instead, on the one drawer in the corpus where a calm keeper demand used to render.
	h._assert_hud("…and the drawer states no standing keeper bill at all — that row is retired",
		not Q.has_label_containing(h._hud.occupant_detail, "drawn from the band")
			and not Q.has_label_containing(h._hud.occupant_detail, "the pool covers"))
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
	#
	# **THE AXIS IS THIS HERD'S OWN `Tame`, NOT AN EMPTY SLOT**, and the difference is the derivation
	# (`docs/plan_standing_upkeep.md` §2.4): the wild fixture is 40% tamed, and a meter between zero
	# and its cost IS a build in flight whoever declared it. What the claim has always been about is
	# that the slot carries THIS herd's answer rather than the pen-ready herd's leftover `Corral`, so
	# it is stated that way rather than as an emptiness the model no longer has.
	h._assert_hud("…and the stepper it agrees with is built on THIS herd's own improvement axis",
		h._hud._compose.hunt_improvement() == SourceForecast.IMPROVEMENT_TAME
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

	_assert_the_hint_states_each_kits_own_items()
	_assert_the_appended_axes_read_the_band()
	_assert_a_pen_prices_on_the_hunters_carry()

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
# Reported from play on the expanded roster. The compose sheet offered the trapping kit and a
# weaponless handling kit against a Red Deer as ordinary choices — pricing the trap's `dispersion 0` (nothing flees, so the take looks
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
## and the SYNTHETIC handling kit (the pen axis, `HANDLING_KIT_ID`; no shipped kit has that shape since
## §4.9 item 12b, and its docstring is where the reason lives). Both are absent from
## `BandFx.kit_roster_fixture()`, and adding them there would change what every hunt picker in both
## harnesses lists.
func _offer_roster() -> Array:
	var kits := _pen_axis_roster()
	kits.insert(kits.size() - 1, {
		"id": KIT_ID_TRAPPING, "display_name": "Trapping kit", "jobs": ["hunt"],
		"attack": BandFx.KIT_ATTACK_EQUIPPED,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_BARE,
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
	return RungFx.stamp_herd(herd)

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
	# **THE SYNTHETIC ENTRY, not a shipped kit** — since §4.9 item 12b deleted the `husbandry` kit no
	# shipped roster carries this shape (a hunt kit that hauls and cannot hurt anything), and the two
	# claims it is here for are still live rules. See `HANDLING_KIT_ID`.
	var deer_handling := _picker_entry(deer_sheet, HANDLING_KIT_LABEL)
	# **A RED DEER NEVER CLIMBS**, so neither of the handling kit's axes can reach it: its pen tier
	# wants a pen this species can never have, and its build axis wants a rung it can never stand on.
	# That is what keeps the withholding honest now that the build axis exists — see the warren below,
	# where the same kit on the same roster is offered.
	#
	# **AND THE REASON IT STATES IS THE WEAPON'S, not the pen's.** The kit carries a sled, so it
	# supplies the haul a wild hunt reads and the pen rule declines it (`kit_reaches_a_wild_hunt`);
	# what refuses it here is that it carries no weapon, and a wild herd is FOUGHT — the sim's
	# `max(0, attack − defense)` refuses the hunt outright, so the row must grey and say so.
	h._assert_hud("…and the handling kit is greyed on a wild-ceiling herd, for the WEAPON's reason — \"%s\""
			% String(deer_handling.get("text", "")),
		bool(deer_handling.get("disabled", false))
			and String(deer_handling.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % "Red Deer"))
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
	h._assert_hud("…while the handling kit is OFFERED on a warren, whose rungs its gear can still speed",
		not bool(_picker_entry(rabbit_sheet, HANDLING_KIT_LABEL).get("disabled", true)))

	_assert_a_closed_gate_quotes_zero(deer)
	await _assert_the_greying_follows_the_animal()

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

## The quarry the sled roster is judged on. A distinct id from the offer block's deer, because the
## composed kit is dropped on a source CHANGE and re-showing the same animal would not be one.
const SLED_ROSTER_DEER_ID := "game_deer_sled_roster"

## Its penned twin, on its own id for the same reason — and for a second one the wild sheet does not
## have: the sheet re-reads its quarry from the SELECTION by id, so a corralled twin sharing another
## fixture's id renders against that herd instead.
const SLED_ROSTER_PEN_ID := "game_boar_sled_roster_pen"

## The penned twin's own CONTROL — a corralled Rabbit Warren, on its own id for the same two reasons.
## It is what stops "a pen greys the weaponless kits" from being read as "a pen greys everything": the
## warren's `defense 0` is cleared by the bare hand, so the identical roster greys nothing on it.
const SLED_ROSTER_PEN_WARREN_ID := "game_rabbit_sled_roster_pen"

## The penned twin's species terms, at the shipped `fauna_config.json` numbers — a **Wild Boar**
## (12 kg, `defense 2`, husbandry ceiling `pen`). Both halves are load-bearing: the defence is what
## makes the weapon rule grey the trap and the handling kit out on the range, and the ceiling is what
## makes a pen a place this animal can actually be.
const PEN_BOAR_BODY_MASS := 12.0

const PEN_BOAR_DEFENSE := 2.0

## The four entries a hunt picker lists off `_sled_roster()` — Stalking, Trapping, the synthetic
## handling kit and the null kit. Named because both sheets below assert it as a PRECONDITION: every claim about a greyed
## row or an absent reason passes on a picker that listed nothing at all.
const SLED_ROSTER_HUNT_KITS := 4

## **THE SHIPPED ROSTER** the pen/wild contrast below is asked against. It lifted the pen tier onto
## every sled-carrying kit while `EquipmentStat::PenCarry` existed; the axis is deleted (issue #543),
## so it is the offer roster unchanged and kept under its own name because three frames are staged
## from it and their names are the harness's stable handles.
func _sled_roster() -> Array:
	return _offer_roster()

## **THE GREYING FOLLOWS THE ANIMAL, NEVER THE FENCE.**
##
## ⛔ **IT WAS `_assert_a_sled_does_not_make_a_hunt_kit_pen_only`, AND THE RULE IT GUARDED IS GONE**
## (issue #543). Reported from play: the ASSIGN HUNTERS sheet on a wild Red Deer greyed **all three**
## hunt kits with one sentence — *"what it adds is only used on a penned herd"* — leaving the null kit
## as the only selectable entry, so a wild hunt could not be equipped at all. The pen rule read
## `kit_uses(pen_carry) and not penned`, a PROXY that held only while the sole declarer of `pen_carry`
## was pen-only gear; `hurdles` left the roster as EQUIPMENT, both sides of the axis moved onto the
## sled that every hunt kit carries, and the proxy became true of SPEARS. `EquipmentStat::PenCarry` is
## now deleted outright, so no kit can be pen-only and the rule with it — the claims about that reason
## went where the reason did.
##
## **WHAT THE THREE FRAMES STILL PROVE IS THE WEAPON RULE AT A PEN**, which is the live question:
## a fence does not kill the boar. The wild sheet, its corralled twin and a corralled Rabbit Warren are
## staged on ONE roster in ONE run, so a picker that greyed everything and one that greyed nothing both
## fail.
func _assert_the_greying_follows_the_animal() -> void:
	var kits := _sled_roster()
	h._hud.update_kit_roster(kits, BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	# **THE PRECONDITION IS THAT THERE IS A HAUL TIER TO TELL KITS APART BY AT ALL** — read off the
	# fixture dicts rather than through `kit_uses`, which is a term of the predicates under test. The
	# stalking kit must beat the null kit on the ONE carry axis, or every claim below is asked on a
	# roster whose entries are interchangeable.
	#
	# **THE ROSTER'S OWN `none`, not `KitRoster.NO_KIT_ID`** — that constant is the EMPTY id a caller
	# passes to mean "no selection", where the null kit is an ordinary member spelled `none`.
	var stalking_carry := float(KitRoster.kit_by_id(kits, BandFx.KIT_ID_BIG_GAME).get(
		KitRoster.KIT_HUNT_CARRY_KEY, 0.0))
	var null_carry := float(KitRoster.kit_by_id(kits, BandFx.KIT_ID_NONE).get(
		KitRoster.KIT_HUNT_CARRY_KEY, 0.0))
	h._assert_hud("precondition: the sled-carrying stalking kit declares the EQUIPPED haul (%s)"
		% str(stalking_carry), is_equal_approx(stalking_carry, BandFx.KIT_HUNT_CARRY_EQUIPPED))
	h._assert_hud("…while the null kit declares the BARE one, so the axis has a tier to beat (%s)"
		% str(null_carry), is_equal_approx(null_carry, BandFx.KIT_HUNT_CARRY_BARE))

	# --- THE WILD RED DEER: the sheet from the report ---------------------------------------------
	var deer := _offer_quarry(SLED_ROSTER_DEER_ID, "Red Deer", "big", OFFER_DEER_BODY_MASS,
		OFFER_DEER_DEFENSE, SourceForecast.HUSBANDRY_CEILING_WILD)
	h._hud._compose.reset_hunt_source()
	h._show_herd(deer)
	h._compose_herd(deer, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	# **THE SHEET IS REACHED FOR AFTER A SETTLE, NEVER STRAIGHT OFF THE COMPOSE CALL.** The one the
	# controller holds is replaced on the frame after the source changes, so a picker read in the same
	# breath is the PREVIOUS quarry's — entries about the wrong animal, passing or failing for reasons
	# that have nothing to do with the code under test.
	await h._settle()
	var sheet: Control = h._hud._drawercompose._compose_sheet
	# Open, because the greying lives in the POPUP — placed by hand for the reason the deer state
	# above records (`show_popup()` grabs input and can move focus mid-run).
	var picker := Q.find_meta_node(sheet, KitRoster.KIT_PICKER_META) as OptionButton
	if picker != null:
		picker.get_popup().position = Vector2i(
			picker.get_screen_position() + Vector2(0.0, picker.size.y))
		picker.get_popup().popup()
	await h._settle()
	await h._save("herd_kit_offer_sled_roster_wild")
	if picker != null:
		picker.get_popup().hide()
	var rows := _picker_entries(sheet)
	h._assert_hud("precondition: the sled roster's Red Deer sheet lists every hunt kit (%d)"
		% rows.size(), rows.size() == SLED_ROSTER_HUNT_KITS)
	var stalking_row := _picker_entry(sheet, "Stalking kit")
	h._assert_hud("a sled does not make the spear line pen-only: Stalking is SELECTABLE — \"%s\""
			% String(stalking_row.get("text", "")),
		not bool(stalking_row.get("disabled", true)))
	h._assert_hud("…and the sheet OPENS on it, so the selection did not collapse to the null kit (%s)"
		% h._hud._compose.hunt_kit_id(), h._hud._compose.hunt_kit_id() == BandFx.KIT_ID_BIG_GAME)
	h._assert_hud("…carrying the `(default)` mark the picker never stopped drawing — \"%s\""
			% String(stalking_row.get("text", "")),
		String(stalking_row.get("text", "")).ends_with(HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX))
	# The two that stay greyed, each for the rule that actually applies to it: a trap clears no
	# defence, and the handling kit carries no weapon at all.
	var trapping_row := _picker_entry(sheet, "Trapping kit")
	h._assert_hud("Trapping is still greyed on a Red Deer, and for the WEAPON's reason — \"%s\""
			% String(trapping_row.get("text", "")),
		bool(trapping_row.get("disabled", false))
			and String(trapping_row.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % "Red Deer"))
	var handling_row := _picker_entry(sheet, HANDLING_KIT_LABEL)
	h._assert_hud("…and so is the handling kit, which carries a sled and no weapon — \"%s\""
			% String(handling_row.get("text", "")),
		bool(handling_row.get("disabled", false))
			and String(handling_row.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % "Red Deer"))
	# ⛔ A claim stood here counting how many entries of a WILD sheet stated the pen reason
	# (*"what it adds is only used on a penned herd"*) and requiring zero. Both the rule and the string
	# are deleted (issue #543), so the claim could only assert that a constant no longer exists.

	# --- THE PENNED TWIN: the same roster, the place the axis IS read ------------------------------
	# **A WILD BOAR, because the penned claim needs an animal that is BOTH defended and pennable**
	# (`fauna_config.json`: `defense 2`, `body_mass 12`, ceiling `pen`). A penned rabbit would grey
	# nothing whatever the code did — it greys nothing in the wild either — so the contrast would be
	# empty; out on the range this boar greys the trap and the handling kit exactly as the deer does.
	#
	# **IT CARRIES ITS OWN ID.** The sheet resolves its quarry through `DrawerComposeController.
	# _live_herd`, which re-reads the SELECTION by id, so a twin sharing the shared fixture's id
	# renders against the herd the prologue selected and its `corralled` flag never arrives.
	var pen := _offer_quarry(SLED_ROSTER_PEN_ID, "Wild Boar", "big", PEN_BOAR_BODY_MASS,
		PEN_BOAR_DEFENSE, SourceForecast.HUSBANDRY_CEILING_PEN)
	pen[KitRoster.QUARRY_CORRALLED_KEY] = true
	# ⛔ **`NO_ENGAGEMENT_STAGE` IS WHAT THE WIRE ACTUALLY PUBLISHES FOR A PEN, and this fixture keeps
	# it deliberately.** `core_sim/src/snapshot/subsistence.rs` still filters the field on
	# `is_corralled()`, against the sim's own behaviour, until issue #572 closes it — so a fixture that
	# quietly stated a real reach here would prove the greying below against a frame no live client
	# ever receives. `KitRoster.quarry_is_fought` reads the CORRALLED flag for exactly this reason.
	pen["engage_rate"] = SourceForecast.NO_ENGAGEMENT_STAGE
	h._hud._compose.reset_hunt_source()
	h._show_herd(pen)
	h._compose_herd(pen, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	var pen_sheet: Control = h._hud._drawercompose._compose_sheet
	var pen_picker := Q.find_meta_node(pen_sheet, KitRoster.KIT_PICKER_META) as OptionButton
	if pen_picker != null:
		pen_picker.get_popup().position = Vector2i(
			pen_picker.get_screen_position() + Vector2(0.0, pen_picker.size.y))
		pen_picker.get_popup().popup()
	await h._settle()
	await h._save("herd_kit_offer_sled_roster_pen")
	if pen_picker != null:
		pen_picker.get_popup().hide()
	var pen_rows := _picker_entries(pen_sheet)
	h._assert_hud("precondition: the corralled twin lists the same kits (%d)" % pen_rows.size(),
		pen_rows.size() == SLED_ROSTER_HUNT_KITS)
	# ⛔ **THIS BLOCK INVERTED AT §4.9 item 12b, and the old claim was that a corralled herd greys
	# NOTHING.** It was true of a sim that quoted a fence-holding band a take whatever it carried; the
	# take resolves engage → retreat → fight at every rung now, at the species' own `defense`, so a
	# bare-handed party at a pen is quoted nothing and paid nothing
	# (`core_sim/tests/hunt_useful_crew_on_the_wire.rs`). **Containment solves the catching, weapons
	# solve the killing** — and the picker has to say so before the crew is sent.
	var pen_trapping := _picker_entry(pen_sheet, "Trapping kit")
	h._assert_hud("a fence does not kill the boar: Trapping is greyed at the PEN too, for the WEAPON's reason — \"%s\""
			% String(pen_trapping.get("text", "")),
		bool(pen_trapping.get("disabled", false))
			and String(pen_trapping.get("text", "")).contains(
				HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % "Wild Boar"))
	# **THE SPEAR LINE IS THE OTHER HALF OF THE CLAIM.** A rule that greyed everything on a pen would
	# satisfy the line above on its own, and it would be the old defect pointing the other way.
	var pen_stalking := _picker_entry(pen_sheet, "Stalking kit")
	h._assert_hud("…while the spear line clears the same defence and stays selectable — \"%s\""
			% String(pen_stalking.get("text", "")),
		not bool(pen_stalking.get("disabled", true)))
	# **AND THE HANDLING KIT IS STILL OFFERED, on the BUILD axis rather than on the weapon.** It
	# carries no weapon and could not bring the boar down, but this pen has a rung left to climb and
	# the axis that speeds that climb is asked BEFORE the fight — a crook does not have to kill a beast
	# to be the right thing to carry while you are gentling one. Out on the range the same kit on the
	# same roster IS greyed (the wild twin above), because a wild boar offers no rung to build.
	var pen_handling := _picker_entry(pen_sheet, HANDLING_KIT_LABEL)
	h._assert_hud("…and the handling kit stays offered on the rung its gear can still speed — \"%s\""
			% String(pen_handling.get("text", "")),
		not bool(pen_handling.get("disabled", true))
			and RungGates.hunt_rung_remains(pen, HudComposeVocab.BARE_FORECAST_PREFIX))
	# ⛔ **THE PEN AND THE RANGE ARE PRICED ON ONE AXIS** (issue #543). This pair asserted the opposite
	# — *"the PEN is the axis it is priced on … where the wild twin is priced on the hunt haul"* — off a
	# `carry_axis_for(job, src)` that forked on `corralled`. The fork is gone with
	# `EquipmentStat::PenCarry`, so what is claimed now is that the fence moves NOTHING: the axis is the
	# job's, and both twins answer it.
	h._assert_hud("a pen is collected on the hunt's own haul, like the range (%s)"
		% KitRoster.carry_axis_for(KitRoster.JOB_HUNT),
		KitRoster.carry_axis_for(KitRoster.JOB_HUNT) == KitRoster.KIT_HUNT_CARRY_KEY)

	# --- THE PENNED ANIMAL THE PARTY CAN ACTUALLY KILL: nothing is greyed -------------------------
	# **THE PAIRING IS THE CLAIM, and without this half the boar's greying proves only that a pen
	# greys things.** A Rabbit Warren states `defense 0`, so the bare hand's `attack 1` clears it,
	# `max(0, 1 − 0)` is positive and the gate is open for every kit on the roster — penned exactly as
	# it is wild. What decides the greying is the ANIMAL, never the fence.
	var pen_warren := _offer_quarry(SLED_ROSTER_PEN_WARREN_ID, "Rabbit Warren", "small",
		OFFER_RABBIT_BODY_MASS, OFFER_RABBIT_DEFENSE, SourceForecast.HUSBANDRY_CEILING_PEN)
	pen_warren[KitRoster.QUARRY_CORRALLED_KEY] = true
	pen_warren["engage_rate"] = SourceForecast.NO_ENGAGEMENT_STAGE
	h._hud._compose.reset_hunt_source()
	h._show_herd(pen_warren)
	h._compose_herd(pen_warren, OFFER_HUNTERS, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	var warren_sheet: Control = h._hud._drawercompose._compose_sheet
	var warren_picker := Q.find_meta_node(warren_sheet, KitRoster.KIT_PICKER_META) as OptionButton
	if warren_picker != null:
		warren_picker.get_popup().position = Vector2i(
			warren_picker.get_screen_position() + Vector2(0.0, warren_picker.size.y))
		warren_picker.get_popup().popup()
	await h._settle()
	await h._save("herd_kit_offer_sled_roster_pen_warren")
	if warren_picker != null:
		warren_picker.get_popup().hide()
	var warren_rows := _picker_entries(warren_sheet)
	h._assert_hud("precondition: the penned warren lists the same kits (%d)" % warren_rows.size(),
		warren_rows.size() == SLED_ROSTER_HUNT_KITS)
	var warren_greyed := 0
	for row_variant in warren_rows:
		if bool((row_variant as Dictionary)["disabled"]):
			warren_greyed += 1
	h._assert_hud("a penned warren greys NOTHING — every kit clears a `defense 0` (%d greyed)"
		% warren_greyed, warren_greyed == 0)

## **THE HINT LINE STATES THE KIT'S OWN ATTACK, HAUL AND ITEMS — and the two kits differ in all three.**
##
## ⛔ **IT WAS `_assert_handling_hint_states_the_pen`, A 2×2 OVER (kit × fence)** — *"a handling kit's
## hint NAMES the pen and an ordinary hunt kit's does not … the pen column keeps the SLED's condition
## and drops the attack, which is the sim's own split: a penned beast is slaughtered rather than
## stalked."* Both halves of that died: §4.9 item 12b made a pen resolve the ordinary FIGHT (so the
## weapon clause belongs there), and issue #543 deleted `EquipmentStat::PenCarry` (so the haul clause
## is the sled's there). `tier_hint` no longer takes a quarry at all.
##
## ⛔ **IT WAS THEN `_assert_the_hint_reads_the_same_at_a_pen`, AND ITS PEN COLUMN WAS A TAUTOLOGY.**
## That version claimed *"THE 2×2 IS KEPT AND ITS EXPECTATION INVERTED … asserting the wild and penned
## readings are EQUAL and that the two KITS are not is what keeps it honest."* It did no such thing.
## `tier_hint` had already lost its quarry parameter, so the two "penned" readings were the
## BYTE-IDENTICAL call the wild pair makes, and the corralled twins were built and handed nowhere —
## the trailing precondition compared the two fixture dicts only against each other. Two claims that
## cannot fail unless the two above them already have assert that a control is PRESENT, not RIGHT.
##
## **SO "THE FENCE MOVES NOTHING" IS STRUCTURAL HERE NOW, NOT TESTED.** `tier_hint(kits, kit, band,
## job, crew)` takes no source at all: there is no argument through which a pen could reach it, so a
## penned READING of the hint is not expressible, and the invariant holds by construction. Restoring
## one would mean re-adding the source parameter issue #543 deleted — which is the very change the
## claim was about. **If `tier_hint` ever takes a source again, this block is where the wild/penned
## equality has to be rebuilt**, and until then the arc's live pen claim is the PRICING one:
## `_assert_a_pen_prices_on_the_hunters_carry` below drives `DrawerComposeController._hunt_priced_herd`,
## which does take the herd, over `_corral_twin`'s pair.
##
## **DRIVEN OVER A LOCALLY-BUILT ROSTER, and it has to be**: the entry it stages is the SYNTHETIC
## `HANDLING_KIT_ID`, a hunt kit with a haul and no weapon, no shipped kit having had that shape since
## §4.9 item 12b deleted the `husbandry` one.
##
## Each of the two readings is an EQUALITY against a string composed here, never recomposed through
## `KitRoster.tier_hint`: an expectation re-derived through the function under test asserts nothing,
## and a `contains` would pass on a hint that stated every tier it knows.
func _assert_the_hint_states_each_kits_own_items() -> void:
	var kits := _pen_axis_roster()
	# The shared fixture states a condition for EVERY item the roster ships, handling gear included,
	# so this no longer grafts one on — a second row for the same item would shadow the fixture's.
	var band := _pen_axis_band({})
	var stalking := KitRoster.kit_by_id(kits, BandFx.KIT_ID_BIG_GAME)
	var handling := KitRoster.kit_by_id(kits, HANDLING_KIT_ID)
	var sep := HudComposeVocab.KIT_HINT_SEPARATOR
	var attack_equipped := HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS)
	var attack_bare := HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(
		BandFx.KIT_ATTACK_BARE, HudComposeVocab.KIT_TIER_DECIMALS)
	var hunt_carry := HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(
		BandFx.KIT_HUNT_CARRY_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS)
	# **THE ITEM NAMES ITSELF** — the clause takes the wire's own `item_ids` entry, so there is no
	# axis→item table left for an expectation to borrow (nor for the hint to guess through).
	var spears := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_SPEARS, int(BandFx.KIT_CONDITION_SPEARS)]
	var sled := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_SLED, int(BandFx.KIT_CONDITION_SLED)]
	var handling_gear := HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
		BandFx.KIT_ITEM_CROOK, int(BandFx.KIT_CONDITION_CROOK)]
	# The hint is source-blind, so there is one column and it is the only one there can be — see the
	# struck claim above for why a second, "penned" column here was a copy of this call, not a test.
	var stalking_hint := KitRoster.tier_hint(kits, stalking, band, KitRoster.JOB_HUNT)
	var handling_hint := KitRoster.tier_hint(kits, handling, band, KitRoster.JOB_HUNT)
	h._assert_hud("a stalking kit states attack, the haul and its own items — \"%s\""
		% stalking_hint, stalking_hint == sep.join([attack_equipped, hunt_carry, spears, sled]))
	# The handling kit carries no spears, so it takes the bare-handed attack and names the crook it does
	# carry — the ITEM half of the claim, which is what stops the two readings being one reading twice.
	h._assert_hud("…and a handling kit states the BARE attack and names its crook — \"%s\"" % handling_hint,
		handling_hint == sep.join([attack_bare, hunt_carry, handling_gear, sled]))
	h._assert_hud("…so the KIT moves the line, and neither reading is a constant the other could be",
		stalking_hint != handling_hint)

## **THE APPENDED AXES STEP DOWN WITH THE BAND'S OWN WEAR — a pair per axis, and neither half proves
## anything alone.**
##
## They reached `KitOption` (the FRESH roster) and the cohort's flat fields long before they reached
## `BandKitTiers`, so for a while a picker asking *what would the kit under the cursor grant me* had
## nowhere to read them but the roster: a Scout card quoted `2-tile sight per vantage` while
## `calculate_visibility` revealed at 1. **Wrong in the reassuring direction**, which is the direction
## nobody reports.
##
## **EACH AXIS IS A PAIR BECAUSE EITHER READING ALONE IS SATISFIABLE BY A CONSTANT.** A client stuck on
## the roster's fresh tier passes the equipped half; one that had stopped resolving anything passes the
## worn half. Only the two together say the number FOLLOWS the band. The worn fixtures differ from
## their fresh twins in the one item that supplies the axis and in nothing else, so a step-down
## reaching for any other item's condition moves the wrong number.
##
## ⛔ **THE PEN WAS THE OTHER AXIS OF THIS PAIR AND IS GONE** (issue #543). Its half read *"a pen
## compose sheet quoted `pen 40.0 per keeper` for a band whose handling gear was dry while the sim
## collected 12"*, and it was made on the whole HINT because that sentence carries the dry clause
## beside the rate. The HINT half is kept and re-aimed at the axis that survived the deletion: the
## handling kit's CROOK still runs dry, the hint still has to drop its condition clause for the dry
## face, and the haul it states is the sled's throughout — which is the whole point of the deletion.
func _assert_the_appended_axes_read_the_band() -> void:
	var pen_kits := _pen_axis_roster()
	var handling := KitRoster.kit_by_id(pen_kits, HANDLING_KIT_ID)
	var fresh_carry := float(KitRoster.effective_tiers(pen_kits, handling,
		_pen_axis_band({}))[KitRoster.KIT_HUNT_CARRY_KEY])
	h._assert_hud("a keeper hauls at the EQUIPPED tier, the sled being what carries a pen (%s)"
		% str(fresh_carry), is_equal_approx(fresh_carry, BandFx.KIT_HUNT_CARRY_EQUIPPED))
	var worn_hint := KitRoster.tier_hint(pen_kits, handling, _pen_axis_band({}, true),
		KitRoster.JOB_HUNT)
	# **THE HAUL DOES NOT MOVE AND THE CROOK'S CLAUSE DOES.** The dry band differs from the fresh one in
	# the CROOK alone, so a hint that stepped the carry down here would be reading the wrong item's
	# condition — the exact class of bug `BandKitTiers` exists to remove.
	var want_worn := HudComposeVocab.KIT_HINT_SEPARATOR.join([
		HudComposeVocab.KIT_HINT_ATTACK_FORMAT % String.num(
			BandFx.KIT_ATTACK_BARE, HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % String.num(
			BandFx.KIT_HUNT_CARRY_EQUIPPED, HudComposeVocab.KIT_TIER_DECIMALS),
		HudComposeVocab.KIT_HINT_DRY_FORMAT % BandFx.KIT_ITEM_CROOK,
		HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [
			BandFx.KIT_ITEM_SLED, int(BandFx.KIT_CONDITION_SLED)]])
	h._assert_hud("…and a DRY crook shows as dry beside an untouched sled's own haul — \"%s\"" % worn_hint,
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

## **A PEN IS COLLECTED ON THE HUNTER'S CARRY — the same one, at the same number** — the pricing half
## of the rule, and the one that moves a number rather than a sentence.
##
## ⛔ **THIS BLOCK ASSERTED THE OPPOSITE AND IS INVERTED** (issue #543). It was
## `_assert_a_pen_prices_on_the_keepers_carry`: *"a corralled herd is worked from a Hunt row, so an
## axis keyed by JOB priced a pen on the SLED's tier while the sim collects one on
## `EquipmentStat::PenCarry`"*, and its headline was *"the handling gear is worth MORE at a pen than
## the sled is."* That stat is deleted — what a worker can carry is a fact about the people and their
## gear, never about the ground they stand on — so the fence must move NOTHING and the old claim is
## now the defect.
##
## **THE FENCE MOVING NOTHING IS SATISFIED BY A CLIENT THAT PRICES NOTHING**, so it is asked with two
## contrasts that a dead pricing seam fails: the herd's own published rate is the reference the fully
## equipped kits must hit, and the NULL kit — a bare-handed party, the roster's own `none` — must come
## in strictly under it at the pen exactly as it does in the wild. Equality without that pair would
## pass on a seam returning `src` untouched.
##
## **DRIVEN THROUGH `DrawerComposeController._hunt_priced_herd`, the real seam**, for the reason the
## kit-liveness block above records: the two deaths this feature has had were both in the FEED, and a
## direct `KitRoster` call exercises the arithmetic without ever reaching it. The roster is installed
## and restored around the block, so no frame after it renders a picker this block put there.
func _assert_a_pen_prices_on_the_hunters_carry() -> void:
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
	h._hud._compose.set_hunt_kit_id(HANDLING_KIT_ID)
	var handling_at_the_pen := float(h._hud._drawercompose._hunt_priced_herd(pen, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	# **THE BARE-HANDED PARTY IS THE CONTRAST.** The roster's own `none` hauls `KIT_HUNT_CARRY_BARE`
	# against the reference's equipped tier, so it must come in strictly under — at the pen and in the
	# wild alike. Without it, "the fence moves nothing" passes on a seam that has stopped repricing.
	h._hud._compose.set_hunt_kit_id(BandFx.KIT_ID_NONE)
	var bare_in_the_wild := float(h._hud._drawercompose._hunt_priced_herd(wild, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	var bare_at_the_pen := float(h._hud._drawercompose._hunt_priced_herd(pen, band).get(
		SourceForecast.FORECAST_PER_WORKER_KEY, 0.0))
	h._hud._compose.set_hunt_kit_id(kit_before)
	h._hud.update_kit_roster(kits_before, BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	h._assert_hud("precondition: the herd states a per-worker rate at all (%s)" % str(published),
		published > 0.0)
	# THE WILD READING IS UNCHANGED — a sled still hauls a carcass in off the range at the reference.
	h._assert_hud("a stalking kit on the WILD twin still prices at the SLED's tier (%s of %s)"
		% [str(sled_in_the_wild), str(published)], is_equal_approx(sled_in_the_wild, published))
	# …AND THE FENCE MOVES NOTHING. The same kit on the same herd, corralled, prices at the same number:
	# that IS the decision, stated as an equality on the seam that carries it.
	h._assert_hud("…and prices the CORRALLED twin at exactly the same number (%s against %s)"
		% [str(sled_at_the_pen), str(sled_in_the_wild)],
		is_equal_approx(sled_at_the_pen, sled_in_the_wild))
	# The handling kit hauls on the same sled tier, so it collects that pen at the reference too — two
	# kits, one carry. The old claim here was that this one beat the stalking kit at a pen.
	h._assert_hud("the handling kit, whose gear differs in everything but the haul, collects the same pen at the reference (%s of %s)"
		% [str(handling_at_the_pen), str(published)],
		is_equal_approx(handling_at_the_pen, published))
	h._assert_hud("…so no kit is worth more at a pen than on the range (%s against %s)"
		% [str(handling_at_the_pen), str(sled_at_the_pen)],
		is_equal_approx(handling_at_the_pen, sled_at_the_pen))
	# **THE LIVENESS HALF.** A bare-handed party is quoted LESS than the reference — in the wild and at
	# the pen — so the equalities above cannot be passing on a seam that reprices nothing.
	h._assert_hud("a bare-handed party is quoted UNDER the reference in the wild (%s of %s)"
		% [str(bare_in_the_wild), str(published)], bare_in_the_wild < published)
	h._assert_hud("…and under it by the same margin at the pen (%s against %s)"
		% [str(bare_at_the_pen), str(bare_in_the_wild)],
		bare_at_the_pen < published and is_equal_approx(bare_at_the_pen, bare_in_the_wild))
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
	on_handling[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = HANDLING_KIT_ID
	var geared_line := _gear_row(on_handling)
	h._assert_hud("the handling gear's row states the build it speeds (%s) — \"%s\""
		% [clause, geared_line], geared_line.contains(clause))
	var on_stalking := band.duplicate(true)
	on_stalking[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = BandFx.KIT_ID_BIG_GAME
	var bare_line := _gear_row(on_stalking)
	h._assert_hud("…and NOT on a band whose hunt job left the gear at camp — \"%s\"" % bare_line,
		not bare_line.contains(clause))
	var dry := _pen_axis_band(BandFx.hunt_preview_local_band(), true)
	dry[DetailFormat.BAND_QUOTED_KIT_ID_KEY] = HANDLING_KIT_ID
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
		if String(line).contains(DetailFormat.KIT_LABEL_CROOK):
			return String(line)
	return ""

## The herd the PRICING block is asked against, in its two states. `corralled` is the only difference
## between the two dicts, so anything the sheet says differently about them is the pen's. It is used
## by `_assert_a_pen_prices_on_the_hunters_carry` alone: the hint block above no longer takes a penned
## reading, because `KitRoster.tier_hint` has no source parameter to hand this to.
func _corral_twin(corralled: bool) -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd[KitRoster.QUARRY_CORRALLED_KEY] = corralled
	return herd

## ⛔ **A SYNTHETIC KIT, NAMED SO THAT IT CANNOT BE MISTAKEN FOR A SHIPPED ONE.** No id in
## `equipment.json` is `handling`, and that is deliberate: `docs/plan_standing_upkeep.md` §4.9 item 12b
## deleted the `husbandry` kit, which was the last shipped entry of this SHAPE — a hunt kit that
## supplies a haul and no attack. The picker rules that shape still has to obey did not go with it (a
## weaponless hunt kit is greyed on a wild-ceiling herd for the WEAPON's reason, and offered on a herd
## with a rung left to climb, issue #515), and no shipped roster can prove them any more. So this
## chapter stages the shape itself.
##
## The entry exists FOR that proof — it is not stale content left behind by the deletion, and deleting
## it takes the only coverage those two rules have with it.
const HANDLING_KIT_ID := "handling"

## What the synthetic entry's row reads in a picker. Spelled once, because every claim below finds its
## row by this prefix and a second spelling is how a lookup comes to miss silently and assert `{}`.
const HANDLING_KIT_LABEL := "Handling kit"

## The shared roster plus the synthetic handling kit the harness's own picker states must not see: a
## hunt kit with a full HAUL, a full BUILD axis and a bare-handed ATTACK, which is the shape no shipped
## roster has carried since §4.9 item 12b deleted `husbandry`. Every other axis on it is the roster's
## own bare tier, the wire's shape.
##
## ⛔ It was *"the ONE entry that equips the pen axis, so `KitRoster.equipped_tier` answers 40"*; the
## pen axis is deleted (issue #543) and what the entry stages now is the weaponless SHAPE, which the
## offer test's weapon and build rules are still asked about.
func _pen_axis_roster() -> Array:
	var kits := BandFx.kit_roster_fixture()
	kits.insert(kits.size() - 1, {
		"id": HANDLING_KIT_ID, "display_name": HANDLING_KIT_LABEL, "jobs": ["hunt"],
		"attack": BandFx.KIT_ATTACK_BARE,
		"hunt_carry_per_worker_biomass": BandFx.KIT_HUNT_CARRY_EQUIPPED,
		"forage_carry_per_worker_biomass": BandFx.KIT_FORAGE_CARRY_BARE,
		"scout_vantage_range": BandFx.KIT_SCOUT_VANTAGE_BARE,
		# **AND THE BUILD AXIS, which is what makes this kit applicable before a pen exists.** Its
		# pen tier above is read on a corralled herd and nowhere else; this one is read on any herd
		# with a rung left to climb, which is the work hurdles and halters are physically for.
		"build_work_per_worker": BandFx.KIT_BUILD_WORK_HANDLING,
		# **AND THE WEB THAT WORTH IS FOR.** Hurdles serve the ANIMAL branch, so this kit takes work
		# off a Tame and nothing at all off a Cultivate; an entry stating the worth without the branch
		# reads as serving NO web and its gear silently disappears from every build estimate.
		"build_work_branch": KitRoster.BUILD_BRANCH_ANIMAL,
		# Handling gear, then the sled it also carries — config order, and the list the hint's condition
		# clauses are read off. See the trapping entry above for why an entry without one is inert.
		"item_ids": [BandFx.KIT_ITEM_CROOK, BandFx.KIT_ITEM_SLED],
	})
	return kits

## The band both pen blocks are asked about: the shared kitted fixture PLUS a `kit_tiers` row for the
## synthetic handling kit, which `BandFx` cannot state because no roster it ships offers that kit.
##
## **A KIT WITH NO ROW READS AS `stated == false`**, and then `KitRoster.effective_tiers` falls back to
## the roster's fresh tiers and the hint prints NO condition clause — so without this the handling
## kit's whole gear half would be silently absent and both hint expectations would be asserting a
## line the client had stopped building.
##
## **THE ROW STATES EVERY AXIS THE WIRE'S ROW DOES.** It did not while `BandKitTiers` carried three,
## and an axis therefore came off the ROSTER's fresh tier. `handling_gear_dry` is the other half of
## that pair: the same band with the CROOK worn out, its build axis stepped down the way the sim steps
## it down, so a client that went back to reading the roster reads a live tool against a fixture that
## says spent.
##
## ⛔ It read *"ALL FIVE AXES, THE PEN INCLUDED … how a keeper with dry handling gear was quoted 40"*.
## `EquipmentStat::PenCarry` is deleted (issue #543); the haul row above is what a pen reads now, and
## it rides the SLED, which drying the crook does not touch.
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
		KitRoster.BAND_KIT_TIERS_ID_KEY: HANDLING_KIT_ID,
		KitRoster.KIT_ATTACK_KEY: BandFx.KIT_ATTACK_BARE,
		KitRoster.KIT_HUNT_CARRY_KEY: BandFx.KIT_HUNT_CARRY_EQUIPPED,
		KitRoster.KIT_FORAGE_CARRY_KEY: BandFx.KIT_FORAGE_CARRY_BARE,
		# The handling gear buys the BUILD axis and nothing else, so this kit's vantage is the bare one
		# whatever state that gear is in — a keeper's tools do not help a scout see further, and the haul
		# above rides the SLED, which `handling_gear_dry` deliberately leaves untouched.
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
		# **THE BRANCH RIDES THE ROW TOO, and it does NOT step down with the gear.** Which web a tool
		# serves is a fact about the tool; spent hurdles are still animal handling gear, and the worth
		# above is what goes to zero.
		KitRoster.KIT_BUILD_BRANCH_KEY: KitRoster.BUILD_BRANCH_ANIMAL,
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
		if String(row.get(KitRoster.ITEM_CONDITION_ID_KEY, "")) == BandFx.KIT_ITEM_CROOK:
			row[KitRoster.ITEM_CONDITION_REMAINING_KEY] = KitRoster.CONDITION_DRY
		out.append(row)
	return out

# =====================================================================================
#  THE TURN ESTIMATE MOVES WITH THE KIT, NOT ONLY WITH THE CREW
# =====================================================================================
# The compose sheet evaluates `turns(workers)` itself, and its GEAR term is
# `min(workers, the kit's saturating crew) × that kit's per-worker worth` — both halves off a kit ROW,
# which is what makes a kit swap re-price the whole estimate. The crew A/B in
# `chapters/improvements.gd` exercises the ungeared arm and nothing else, so this is the half of the
# form no frame in that set can reach.
#
# ⛔ **THE KIT IS THE BUILDERS', NOT THE SHEET'S OWN PICKER** (the builders-kits arc). This A/B moved
# the HUNT picker for a release, which was the defect: the picker under the crew stepper chooses what
# the TAKE crew carries, and what speeds a build is what the BUILDERS carry — two different rows. The
# sheet reads the band's `builders` row through `KitRoster.builders_kit_for` now, so the frames dial
# THAT row's kit and the hunt picker is held fixed across both.
#
# **THE PAIR IS THE CLAIM, at ONE crew on ONE herd.** A gear term read off a WORKED SOURCE — or off
# the take crew's kit — answers the same number for both, a perfectly plausible sheet quoting the
# wrong crew's tools, and only the second frame can tell that from a working one.

## The crew both kit frames staff. **Above the handling gear's saturating crew on purpose**, so the
## `min` is doing real work in the geared frame rather than being inert.
const KIT_SWAP_KEEPERS := 3

## What that crew owes on an UNSTARTED Tame under each kit, derived HERE from the fixtures rather
## than through the producer under test: the rung costs `HerdFx.ANIMAL_TAME_WORK_COST` (50) with
## nothing banked, the floor sits at the food peak (×1.0) and one keeper banks one work unit a turn.
##
## **NOTHING COMES OFF THE CREW, and slice 6a is what removed the term** (`docs/plan_standing_upkeep.md`
## §2.4). The rung's rate was netted here — the build crew supplied it while the meter was below its
## cost — and the keeping pool owes it at every fullness now, so a builder's whole output is progress.
## What the form nets instead is the meter's ROT, which on this web is structurally nothing
## (`HerdFx.ANIMAL_METER_ROT`: no animal rung declares a `meter_decay`, their penalty being a shed).
## So three builders bank 3: with the builders row naming nothing the sheet derives the roster's
## ANIMAL kit and this band holds no row for it, so the crew is bare and owes ⌈50 ÷ 3⌉; with the
## handling kit named on that row it arms two of the three at 0.5 apiece, so the pool banks `3 + 1.0`
## a turn and owes ⌈50 ÷ 4⌉.
##
## ⛔ **RE-DERIVED WITH THE GEAR TERM'S MOVE** (`docs/plan_standing_upkeep.md` §4.8). The geared count
## was ⌈(50 − 17) ÷ 3⌉ = 11 while the kit was subtracted from the JOB; it divides the job by a faster
## crew now. **The bare count is unmoved at 17, and that is the tell rather than a coincidence** — a
## crew carrying nothing has no term in either form, so a re-derivation that moved it would be
## describing something other than the gear.
const KIT_SWAP_TURNS_BARE := 17

const KIT_SWAP_TURNS_GEARED := 13

## …and what ONE MORE keeper owes under the handling gear: the gear term is unmoved at `2 × 0.5` = 1.0
## because the fourth keeper finds no hurdles left to carry, so the pool banks `4 + 1.0` and owes
## ⌈50 ÷ 5⌉. Beside it, what a `min` dropped from the head count would quote that crew instead
## (`4 × 0.5` = 2.0, so ⌈50 ÷ 6⌉): stated so the negative names a number rather than merely differing.
##
## **THE SATURATION IS WORTH LESS UNDER THE NEW MODEL AND STILL SEPARATES THE TWO ANSWERS**, which is
## the point of restating both — a term worth 0.5 a head cannot move a count as far as one worth 8.5
## did, so a pair that had collapsed to one number would have quietly stopped testing the `min`.
const KIT_SWAP_TURNS_SATURATED := 10

const KIT_SWAP_TURNS_UNCAPPED := 9

## The herd both frames are composed on — a warren, which is the ceiling that keeps the handling kit
## OFFERED (a wild-ceiling herd greys it, see `_kit_offer_states`), with its Tame priced and unstarted
## so the OFFERED face carries the quote rather than a running meter.
func _kit_swap_herd() -> Dictionary:
	var herd := _offer_quarry(KIT_SWAP_HERD_ID, "Rabbit Warren", "small", OFFER_RABBIT_BODY_MASS,
		OFFER_RABBIT_DEFENSE, SourceForecast.HUSBANDRY_CEILING_PEN)
	herd["domestication"] = KIT_SWAP_UNSTARTED_TAME
	return RungFx.stamp_herd(HerdFx.price_animal_build(herd, HerdFx.ANIMAL_BUILD_TURNS_REMAINING,
		HerdFx.ANIMAL_BUILD_WORK_FROM_GEAR, KIT_SWAP_UPKEEP_PER_TURN))

const KIT_SWAP_HERD_ID := "game_warren_kitswap"

## Nothing banked on the Tame, so the quote is the whole job and the two frames differ by the gear
## alone rather than by where a part-built meter happened to stand.
const KIT_SWAP_UNSTARTED_TAME := 0.0

## **THE RUNG'S OWN RATE ON THIS HERD** (issue #545) — `animal:pastoral`'s `1.0 × source_load` over a
## warren's ONE keeper-load, against the reference herd's two. A warren inheriting the reference
## herd's 2.0 would be quoting a rate this herd's own size can never produce.
##
## **IT IS THE OFFERED FACE'S STANDING PRICE NOW, NOT A TERM OF THE PACE**
## (`docs/plan_standing_upkeep.md` §2.4) — what holding this Tame will cost every turn, quoted beside
## what building it costs. So it still has to be this herd's own figure, and for a sharper reason than
## before: the number is on screen.
const KIT_SWAP_UPKEEP_PER_TURN := 1.0

## **THE FULLY-ARMED CREW — six keepers, every one of them carrying hurdles.** The crew and the gear's
## saturating crew are EQUAL on purpose: the `min` on the head count is already pinned by the
## saturation claim above, and a mismatch here would leave this state asserting that term a second
## time instead of the arithmetic at full coverage.
##
## ⛔ **IT WAS THE OVER-GEARED CASE AND THERE IS NO SUCH CASE ANY MORE**
## (`docs/plan_standing_upkeep.md` §4.8). Six keepers at the retired `8.5` took `51` off a 50-unit
## Tame, so the bar went to zero and the job finished on the first worked turn — a *lump against the
## pile*. A kit raises the RATE now, so no crew size makes a job vanish: these six bank
## `6 × 1.0 + 6 × 0.5` = 9.0 a turn against 50 units of work. The ONE-TURN answer is still a live
## branch and is asserted where it is now honest — a meter standing at its cost.
const OVER_GEARED_KEEPERS := 6

const OVER_GEARED_ARMS_CREW := OVER_GEARED_KEEPERS

## What that crew owes: ⌈50 ÷ (6 × 1.0 + min(6, 6) × 0.5)⌉ = ⌈50 ÷ 9⌉. Every term is a stated fixture
## constant, which is what makes this an assertion about the ARITHMETIC rather than about a number
## having appeared.
const ARMED_CREW_TURNS := 6

## …and what the SAME terms answer with the gear left in the numerator — ⌈(50 − 3) ÷ 6⌉. It is close
## enough to the right answer to pass any *is it smaller than bare* test, which is exactly why the
## negative names it (`docs/plan_standing_upkeep.md` §4.8).
const ARMED_CREW_TURNS_GEAR_IN_NUMERATOR := 8

## The two zeros the equality rests on, named because each is a PRECONDITION rather than a default: a
## meter with work already banked or a source bleeding work would both move the count.
const ARMED_NOTHING_BANKED := 0.0
const ARMED_NOTHING_ROTTING := 0.0

## The noun every rendered count carries. It is what an ABSENT clause must not contain — and a clause
## that had lost only its NUMBER would still carry it, which is the shape this needle is aimed at.
const BUILD_TURNS_NOUN_NEEDLE := "turns"

## The band this state stands up has to field `OVER_GEARED_KEEPERS` on BOTH of the sheet's crews at
## once — the take and the build — since they draw on one pool.
const OVER_GEARED_CREWS := 2

## Hands the fixture keeps OUT of the idle pool, so `working_age > idle_workers` the way a real
## cohort's does and nothing here reads as a band with every soul standing free.
const OVER_GEARED_SPARE_NON_IDLE := 4

## The OFFERED face's price CLAUSE alone — `50 work, ≈17 turns · 1 work a turn to hold` — composed
## through the shipped formats, so the assertion pins the counts this chapter derived and not the
## wording.
##
## **THE STANDING PRICE IS PART OF THE CLAUSE** (`docs/plan_standing_upkeep.md` §2.4): a rung costs a
## pile once and a rate forever, and an `ends_with` claim that stopped at the turns would pass on a
## face that had dropped the half a player is being asked to commit to.
func _kit_swap_price_clause(turns: int) -> String:
	return _kit_swap_held_price(HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [
		HudComposeVocab.BUILD_PRICE_WORK_FORMAT % DetailFormat.format_work_units(
			HerdFx.ANIMAL_TAME_WORK_COST),
		HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % turns])

## …and its SINGULAR twin — `50 work, ≈1 turn · 1 work a turn to hold`. Spelled from the count
## vocabulary's own singular rather than through `DetailFormat.build_turns_clause`, which is the fork
## under test.
func _kit_swap_price_clause_one() -> String:
	return _kit_swap_held_price(HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [
		HudComposeVocab.BUILD_PRICE_WORK_FORMAT % DetailFormat.format_work_units(
			HerdFx.ANIMAL_TAME_WORK_COST),
		HudComposeVocab.BUILD_TURNS_COUNT_ONE])

## The standing half, appended to whichever one-off price its caller composed — this warren's own
## `KIT_SWAP_UPKEEP_PER_TURN`, stated in WORK, and naming the ROLE that pays it. The warren is an
## animal source, so that is Husbandry; the role word is composed through the shipped picker rather
## than spelled here, so a re-worded pair moves the expectation with the sheet.
func _kit_swap_held_price(price: String) -> String:
	return HudComposeVocab.BUILD_PRICE_UPKEEP_FORMAT % [price,
		DetailFormat.format_work_units(KIT_SWAP_UPKEEP_PER_TURN),
		HudWorkVocab.keeping_role_name(SourceForecast.SOURCE_KIND_HERD)]

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
	#
	# **THE ROW NAMES NO KIT HERE**, so the sheet derives the roster's answer for this entry's web —
	# the `hurdling` kit, which this band publishes no resolved row for — and the crew builds bare.
	BandFx.staff_builders(h._hud._band_labor, KIT_SWAP_KEEPERS)
	h._compose_herd(warren)
	await h._settle()
	await h._save("herd_kit_swap_bare_build")
	var bare_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)
	# **CAPTURED HERE, NOT WITH ITS ASSERTION.** The estimate is a function of the band's LIVE
	# `builders` row, and the geared frame below re-staffs it — so a reading taken beside the claim
	# would be the geared answer under the bare name, and the two counts would agree for a reason that
	# has nothing to do with the kit. (Measured: it read 11 against 11.)
	var bare_turns := SourceForecast.build_turns_at(warren, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, KIT_SWAP_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK,
		h._hud._drawercompose._build_gear_for(h._hud._band_labor.player_band(),
			SourceForecast.LABOR_KIND_HUNT))

	#   (b) THE HANDLING KIT — the SAME herd, the SAME crew, the SAME floor, the SAME hunt kit under
	# the stepper. Only the BUILDERS row moved, which is the row a build's gear comes off.
	BandFx.staff_builders(h._hud._band_labor, KIT_SWAP_KEEPERS, HANDLING_KIT_ID)
	h._compose_herd(warren)
	await h._settle()
	await h._save("herd_kit_swap_geared_build")
	var geared_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)
	var geared_turns := SourceForecast.build_turns_at(warren, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, KIT_SWAP_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK,
		h._hud._drawercompose._build_gear_for(h._hud._band_labor.player_band(),
			SourceForecast.LABOR_KIND_HUNT))
	print("ui_preview: kit swap  bare=%s (%d turns)  geared=%s (%d turns)" % [
		bare_face, bare_turns, geared_face, geared_turns])

	# **THE ESTIMATE IS OFF THE FACE, so the claims are made on the PRODUCER** (§4.7a ①). Ray took the
	# price and the turn count off the compose sheet — *"That information should be on the work tab.
	# No need to have it here, it is useless."* — and the arithmetic did not move with the rendering,
	# so `build_turns_at` is asked directly at the two kits' own resolved gear. The two FRAMES stay:
	# they are what shows the sheet is otherwise identical under both kits.
	h._assert_hud("a crew whose kit helps no build is quoted the whole job — %d turns" % bare_turns,
		bare_turns == KIT_SWAP_TURNS_BARE)
	h._assert_hud("…and the handling gear takes work off it, at the SAME crew — %d turns" % geared_turns,
		geared_turns == KIT_SWAP_TURNS_GEARED)
	# The negative that names the defect: a gear term read off the SOURCE rather than off the kit row
	# answers one number for both kits, which is what the two claims above spell as two counts.
	h._assert_hud("…so the estimate cannot read the same under both kits",
		KIT_SWAP_TURNS_BARE != KIT_SWAP_TURNS_GEARED and bare_turns != geared_turns)
	# **THE `min` IS ON THE HEAD COUNT, and it is asked of the PRODUCER** — a crew above the gear's own
	# saturating crew cannot be staged on a frame without putting the claim at the mercy of the
	# stepper's cap. A fourth keeper carries no hurdles, so the gear term does not grow with them.
	var geared := KitRoster.build_gear(keepers, HANDLING_KIT_ID, KitRoster.BUILD_BRANCH_ANIMAL)
	var overstaffed := SourceForecast.build_turns_at(warren, HudComposeVocab.BARE_FORECAST_PREFIX,
		SourceForecast.IMPROVEMENT_TAME, KIT_SWAP_KEEPERS + 1, SourceForecast.FLOOR_FOOD_PEAK,
		geared)
	h._assert_hud("a keeper past the gear's own crew adds no gear, the term having saturated",
		overstaffed == KIT_SWAP_TURNS_SATURATED)
	h._assert_hud("…and NOT the shorter job an uncapped gear line would credit that crew with",
		KIT_SWAP_TURNS_SATURATED != KIT_SWAP_TURNS_UNCAPPED
			and overstaffed != KIT_SWAP_TURNS_UNCAPPED)

	#   (c) **A FULLY-ARMED CREW, AND THE ARITHMETIC AT FULL COVERAGE — asserted by EQUALITY.** The same
	# warren and the same handling kit, over a band holding a PARTY'S worth of hurdles: six armed
	# keepers bank `6 × 1.0` from their hands and `min(6, 6) × 0.5` from their tools, so the pool banks
	# 9.0 a turn against a 50-unit Tame with nothing on it and owes ⌈50 ÷ 9⌉.
	#
	# ⛔ **THIS FRAME USED TO CLAIM THE GEAR PAID THE JOB OFF OUTRIGHT** — `6 × 8.5` = 51 off a 50-unit
	# Tame, one turn — and that claim retired with the subtraction (§4.8). Every term here is known, so
	# the count is asserted as a NUMBER rather than as *smaller than the bare one*, and the two
	# negatives beside it name the two ways the term can be put back wrong: in the NUMERATOR, and
	# uncapped.
	var stocked := _pen_axis_band(BandFx.hunt_preview_local_band(), false, OVER_GEARED_ARMS_CREW)
	# **AND HANDS FOR BOTH CREWS, because the sheet's two steppers share ONE pool** — the take is
	# capped at `pool − builders` and the build at `pool − take`
	# (`HudBandLaborState.source_crew_pool_hunt`). The shared fixture's ten idle workers cannot field
	# six hunters AND six keepers, so the take stepper clamped to four and the state below stopped
	# being about an over-geared BUILD at all. Staged rather than worked around: this frame's claim is
	# that six armed keepers pay a 50-unit Tame off outright, and a band that cannot field six of them
	# beside its hunters is not the band that claim is about.
	stocked["idle_workers"] = OVER_GEARED_KEEPERS * OVER_GEARED_CREWS
	stocked["working_age"] = int(stocked["idle_workers"]) + OVER_GEARED_SPARE_NON_IDLE
	h._hud._band_labor._player_band = stocked
	h._hud._band_labor._player_bands = [stocked]
	h._hud._compose.reset_hunt_source()
	var stocked_warren := _kit_swap_herd()
	h._show_herd(stocked_warren)
	h._compose_herd(stocked_warren, OVER_GEARED_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK)
	# The BUILD's crew AND its kit, dialled after the open — see (a) above. The gear is resolved over
	# these hands, so the "gear alone pays the job off" regime is a claim about the BUILDERS' coverage.
	BandFx.staff_builders(h._hud._band_labor, OVER_GEARED_KEEPERS, HANDLING_KIT_ID)
	h._compose_herd(stocked_warren)
	await h._settle()
	await h._save("herd_kit_swap_armed_crew")
	var over_geared_face := ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_TAME)
	print("ui_preview: over-geared build  face=%s  crew=%d" % [
		over_geared_face, Readout.stepper_value(h._hud._drawercompose._compose_sheet)])
	# THE PRECONDITIONS, without which the count below is about some other build: the crew the sheet
	# actually composed is the armed one, the gear it resolved is the shipped pair, and the job really
	# is the whole 50 units with nothing banked and nothing rotting.
	var armed_gear: Dictionary = h._hud._drawercompose._build_gear_for(
		h._hud._band_labor.player_band(), SourceForecast.LABOR_KIND_HUNT)
	h._assert_hud("the sheet really staffs the armed crew, at the shipped gear (%s)" % armed_gear,
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == OVER_GEARED_KEEPERS
			and is_equal_approx(float(armed_gear.get(SourceForecast.BUILD_GEAR_PER_WORKER, 0.0)),
				BandFx.KIT_BUILD_WORK_HANDLING)
			and int(armed_gear.get(SourceForecast.BUILD_GEAR_SATURATING_CREW, 0))
				== OVER_GEARED_ARMS_CREW)
	h._assert_hud("…and the job is the whole Tame, with nothing banked and nothing rotting",
		is_equal_approx(SourceForecast.build_work_cost(stocked_warren,
				HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_TAME),
			HerdFx.ANIMAL_TAME_WORK_COST)
		and SourceForecast.build_work_done(stocked_warren, HudComposeVocab.BARE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_TAME) == ARMED_NOTHING_BANKED
		and SourceForecast.meter_rot_per_turn(stocked_warren,
			HudComposeVocab.BARE_FORECAST_PREFIX) == ARMED_NOTHING_ROTTING)
	# **THE EQUALITY, ON THE PRODUCER** — every term above is known, so this is the arithmetic and not
	# the observation that a number appeared.
	var over_geared_turns := SourceForecast.build_turns_at(stocked_warren,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_TAME, OVER_GEARED_KEEPERS,
		SourceForecast.FLOOR_FOOD_PEAK, armed_gear)
	print("ui_preview: armed crew build turns = %d" % over_geared_turns)
	h._assert_hud("a fully-armed crew owes ⌈50 ÷ (6 + 3)⌉ — got %d" % over_geared_turns,
		over_geared_turns == ARMED_CREW_TURNS)
	# **THE NEGATIVE THAT PINS WHICH SIDE OF THE DIVIDE THE GEAR IS ON.** Left in the NUMERATOR the
	# same terms answer ⌈(50 − 3) ÷ 6⌉ = 8 — a plausible number, close to the right one, and wrong on
	# every job. Stated rather than merely "not equal", so the failure names the retired model.
	h._assert_hud("…and NOT the ⌈(50 − 3) ÷ 6⌉ a gear term left in the numerator would answer",
		ARMED_CREW_TURNS != ARMED_CREW_TURNS_GEAR_IN_NUMERATOR
			and over_geared_turns != ARMED_CREW_TURNS_GEAR_IN_NUMERATOR)
	# …and never the retired *the gear pays the job off* reading, which is what this frame claimed
	# while the kit was a lump against the pile. No crew size makes a job vanish now.
	h._assert_hud("…nor the ONE TURN the retired subtraction quoted this very crew",
		over_geared_turns != SourceForecast.BUILD_FINISHES_IN_ONE_TURN)
	# **THE ONE-TURN BRANCH IS STILL LIVE, and this is where it is honest now**: a meter standing AT
	# its cost has nothing left to work off, and the sim answers `1` there
	# (`docs/plan_unit_costed_work.md` §6.2). Asked of the producer on the same warren with its Tame
	# meter filled, so the pair differs by the METER and by nothing else.
	var finished := stocked_warren.duplicate(true)
	finished["tame_work_done"] = HerdFx.ANIMAL_TAME_WORK_COST
	h._assert_hud("a meter already at its cost finishes on the first worked turn",
		SourceForecast.build_turns_at(finished, HudComposeVocab.BARE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_TAME, OVER_GEARED_KEEPERS, SourceForecast.FLOOR_FOOD_PEAK,
			armed_gear) == SourceForecast.BUILD_FINISHES_IN_ONE_TURN)
	# **AND THE PAIRED NO-ESTIMATE CASE, ASSERTED THROUGH TO THE CLAUSE.** A rung with nothing banked
	# and nobody on it is the DECLARED state — the sim's own `None` — and it must render NO CLAUSE at
	# all. **This is the pair that was missing**: every assertion in this family read a count, so a
	# clause rendering a number-less `≈ turns` for a sentinel it had no face for would have passed all
	# of them. `build_turns_clause` answers `""` for it now, which is what the two call sites test.
	var unstarted := stocked_warren.duplicate(true)
	unstarted["tame_work_done"] = ARMED_NOTHING_BANKED
	var no_estimate := SourceForecast.build_turns_at(unstarted,
		HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_TAME,
		SourceForecast.BUILD_CREW_NONE, SourceForecast.FLOOR_FOOD_PEAK, armed_gear)
	h._assert_hud("a rung with nothing banked and nobody on it has NO estimate — got %d"
			% no_estimate, no_estimate == SourceForecast.BUILD_TURNS_NO_ESTIMATE)
	h._assert_hud("…and the clause for it is ABSENT, never a `≈ turns` with the number missing (\"%s\")"
			% DetailFormat.build_turns_clause(no_estimate),
		DetailFormat.build_turns_clause(no_estimate) == ""
			and not DetailFormat.build_price_clause(HerdFx.ANIMAL_TAME_WORK_COST, no_estimate,
				ARMED_NOTHING_ROTTING, SourceForecast.SOURCE_KIND_HERD).contains(
					BUILD_TURNS_NOUN_NEEDLE)
			and DetailFormat.build_turns_clause(ARMED_CREW_TURNS).contains(BUILD_TURNS_NOUN_NEEDLE))
	h._hud._band_labor._player_band = keepers
	h._hud._band_labor._player_bands = [keepers]
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
