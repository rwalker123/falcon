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
	fixture[HerdFx.HERDERS_NEEDED_KEY] = 0
	fixture[HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY] = REOPEN_WILD_WOULD_BE_HERDERS
	return fixture

## The same herd one turn later, taming under way and owned — see `_reopen_wild_herd_fixture`.
func _reopen_taming_herd_fixture() -> Dictionary:
	var fixture := _reopen_wild_herd_fixture()
	fixture["domestication"] = REOPEN_TAMING_DOMESTICATION
	HerdFx.set_managed_herders(fixture, REOPEN_TAMING_HERDERS)
	return fixture

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
	# **BUILT FROM THE METER FORMAT AND MATCHED AS A PREFIX**, because the face now carries the rung's
	# payoff after the percent (`🐾 Taming — 4% · then 1.20 food`) and the payoff is not what this pair
	# is about. The percent is followed by `%` in the format, so one meter's face can never be a prefix
	# of the other's — `— 0%` does not lead `— 34%` — and the claim stays as exact as the `==` was.
	var stale_meter := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
		FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME),
		String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[HudConst.LABOR_POLICY_TAME]), 0]
	var fresh_meter := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
		FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME),
		String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS[HudConst.LABOR_POLICY_TAME]),
		HudFormat.progress_percent(REOPEN_TAMING_DOMESTICATION)]
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
