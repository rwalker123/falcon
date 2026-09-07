extends RefCounted

## THE OUTFITTING PICKER (issue #629) — a band's outfitting window: two meters, a pick list, and the
## readout that says what the pile is worth.
##
## **IT WALKS BOTH WINDOWS.** The spawned band's GRANT (two budgets, picks that MINT) comes first and
## is most of the chapter; the TAKE a split opens on a splinter (picks that MOVE gear out of the home
## band, capped per ITEM) is appended at the end, after the orb states, so nothing before it moves.
##
## ⛔ **THE TAKE CARD OPENS ON THE SPLIT'S OWN DEFAULT TAKE, NOT AT ZERO.** That allocation is
## kit-denominated and published on the window, so an untouched `Set out` re-sends it unchanged — a
## no-op. It is asserted twice over, on the rendered steppers AND on the composed order, because a
## card that shows the right numbers and a commit that sends them are two different claims.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists
## it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter
## moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`. It runs LAST and
## ends by publishing a SHUT window, so the surface it stands up is gone before anything else could
## inherit it.
##
## ## MOST OF THIS CHAPTER IS ASSERTIONS, AND THAT IS THE POINT OF IT
##
## Every claim the third column makes renders as a perfectly plausible picture whatever it says. A
## row reading `×3`, a dash on a row that should read `×1`, a knowledge-gated bench tool quietly
## present — a screenshot cannot tell a correct one from a wrong one. So the arithmetic is asked of
## the rendered CONTROLS (by meta, never by scraping a subtree's text) and the frames carry the
## layout.
##
## ## THE THREE THINGS ONLY A FRAME CAN SHOW
##
## That the three columns fit side by side at the shipped width; that the swatch legend and the
## recipe rows draw the SAME five inks; and that the dismissed state leaves a reopen control on
## screen rather than nothing at all.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 112

const Q := preload("res://tools/ui_preview/node_query.gd")
## The walk's shared band fixtures — `with_band_id` is what stamps a cohort's durable id and its name,
## and every fixture cohort in every chapter goes through it.
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## Where this chapter's two cohorts stand and how many people they are. Nothing on the card reads
## either — the picker draws a window, not a band's tile — but `update_band_alerts` is a real ingest
## and a cohort reaching it has to be shaped like one.
const BAND_FIXTURE_SIZE := 30
const BAND_FIXTURE_X := 44
const BAND_FIXTURE_Y := 9
## The four shipped HUD palettes, read as DATA — the ready ink's separation claim is made
## against every one of them rather than against whichever the harness happens to be pinned to.
const PaletteScript := preload("res://src/scripts/ui/HudPalette.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# ---- the window the sim publishes ---------------------------------------------------------------

## One kit per working-age hand of the shipped 30-person band — the sim DERIVES this, so a fixture
## states it rather than inventing a dial.
const KIT_BUDGET := 17
## `start_profiles.json` `opening_loadout.material_points`.
const MATERIAL_BUDGET := 30
## The shipped pick list, in the profile's own order — which is also the draw order of the resources
## column and of the legend above the recipe list.
const PICKABLE := ["bone", "fibre", "hide", "wood", "stone"]
## …and the pre-fill it opens on. Sums to 28 of 30, deliberately NOT to the budget: a fixture that
## opened with nothing left could not tell a working meter from one stuck at zero.
const DEFAULT_BONE := 3
const DEFAULT_FIBRE := 17
const DEFAULT_HIDE := 8
const DEFAULTS_TOTAL := DEFAULT_BONE + DEFAULT_FIBRE + DEFAULT_HIDE

## The KIT column's own pre-fill — the shipped spread, and 12 of the 17-kit budget so the meter opens
## with something left for the same reason the material one does. **The wire sends these ALREADY
## CLAMPED**; the fixture states them as the sim would and the picker must draw them unchanged.
const DEFAULT_STALKING := 4
const DEFAULT_TRAPPING := 4
const DEFAULT_GATHERING := 4
const KIT_DEFAULTS_TOTAL := DEFAULT_STALKING + DEFAULT_TRAPPING + DEFAULT_GATHERING

## **THE GATED BENCH TOOL, PRESENT IN THE RECIPE BOOK AND ABSENT FROM `craftable_recipe_ids`.** It is
## the whole reason the list of ids is published: a client that filtered on anything else — a refusal
## sentence, a `requires_knowledge` array it re-tested itself — would show this row.
const RECIPE_GATED := "tanning_frame"

const RECIPE_SPEARS := "spears"
const RECIPE_BASKETS := "baskets"
const RECIPE_TRAPS := "traps"
const RECIPE_SLED := "sled"
const RECIPE_CROOK := "crook"
## Costs 3 WOOD, and the default pile holds none — the unreachable row, which must be DIMMED and
## present rather than filtered away.
const RECIPE_EARTHMOVING := "earthmoving"

## What each row must read against the default pile (bone 3, fibre 17, hide 8, wood 0):
## `floor(min over inputs of held / required)`.
const EXPECTED_SPEARS := 3     # bone 1 → 3, fibre 2 → 8, hide 1 → 8
const EXPECTED_BASKETS := 3    # fibre 5 → 3, hide 1 → 8
const EXPECTED_TRAPS := 2      # fibre 6 → 2, bone 1 → 3
const EXPECTED_SLED := 1       # hide 6 → 1, fibre 2 → 8
const EXPECTED_CROOK := 3      # bone 1 → 3, fibre 2 → 8
const EXPECTED_EARTHMOVING := 0

## The kit the chapter buys, and how many of it.
const KIT_STALKING := "big_game"
const KIT_PRESSES := 3
## The roster entry that must NEVER be offered — it grants nothing, which is how the picker knows.
const KIT_NONE := "none"

# ---- the OVER-BUDGET band (the reported screen) --------------------------------------------------

const OVER_BAND_ENTITY := 6203
## Its budgets are the ones the split left it with; its accepted rows are what it still claims. The
## two overspends are DIFFERENT sizes and in different currencies, so a detail that reported one for
## the other lands on the wrong number rather than on a coincidence.
const OVER_KIT_BUDGET := 12
const OVER_KITS_HELD := 14
const OVER_MATERIAL_BUDGET := 22
const OVER_UNITS_HELD := 28
## What the card's own material meter must read — the reported `-6 / 22 left`, spelled as a LITERAL
## rather than composed through `BUDGET_REMAINING_FORMAT`, since an expectation built from the format
## under test can only agree with itself.
const OVER_METER_NEEDLE := "-6 / 22 left"
## …and the row's whole detail, by equality: the band leads, then both overspends.
const OVER_DETAIL := "Windmere — 2 kits, 6 resources over budget"
## The words the READY arm uses, asserted ABSENT from an over-budget row — the defect was those exact
## words on this exact band.
const ORB_DETAIL_READY_NEEDLE := "everything is picked"

## A stepper loop's ceiling. It is a GUARD, not the expected count: a `+` that stopped working would
## otherwise spin this chapter until the watchdog killed the whole run.
const STEPPER_PRESS_LIMIT := 64

const STEPPER_PLUS_FACE := "+"
const STEPPER_MINUS_FACE := "−"

# ---- the TAKE window a split opens ---------------------------------------------------------------

## The two cohorts this chapter pushes. The home band holds the GRANT; the splinter's window is a
## TAKE on it, which is what a split from turn two onward opens.
const HOME_BAND_ENTITY := 6201
const SPLINTER_BAND_ENTITY := 6202

## ⛔ **THE CAP THAT CANNOT BE DRAWN PER KIT ROW.** `big_game` uses `spears + sled` and `trapping` uses
## `traps + sled`, so the two rows are NOT independent — five of each needs ten sleds. The supply is
## aimed at exactly that: sleds are the scarcest line, so `big_game` runs out at the SLED rather than
## at its own spears, and `trapping` is then capped at zero with four traps still sitting at home.
##
## **Each line is `the home band's holdings + this take's standing units`**, which is the cap the sim
## refuses on — so the two `big_game` kits the split already moved are IN these numbers.
const TAKE_SPEARS := 6
const TAKE_SLED := 5
const TAKE_TRAPS := 4
const TAKE_BASKETS := 3
## What the kit meter reads against on a take: the whole published supply. **It lists only items some
## kit carries** — the sim filters the three bench tools out, shop equipment staying with the workshop
## that built it — so no client re-derives that filter and a fixture naming one would be a supply no
## server can send.
const TAKE_ITEM_TOTAL := TAKE_SPEARS + TAKE_SLED + TAKE_TRAPS + TAKE_BASKETS
## ⛔ **THE DEFAULT TAKE THE SPLIT ALREADY MOVED, kit-denominated and published on the window** — and
## the card OPENS on it. It is what makes an untouched `Set out` an exact no-op instead of an order to
## take nothing, which is what an empty card committed while the take was a bare per-item manifest.
## Non-zero and unequal to the stepper's floor, so a card that opened at zero — or at one — fails on
## the count rather than on a coincidence.
const TAKE_DEFAULT_BIG_GAME := 2
const TAKE_DEFAULT_HIDE := 2

## `big_game` is sled-bound, not spear-bound — the whole point of the fixture.
const TAKE_BIG_GAME_CEILING := TAKE_SLED
## What one `big_game` kit puts in hands (`spears` + `sled`), which is what the take's meter counts:
## its currency is ITEM UNITS, not kit slots, because that is the currency the cap is denominated in.
const KIT_STALKING_ITEMS := 2
## …and how far it is walked BACK, so `trapping` has sleds again.
const TAKE_BIG_GAME_RELEASED := 2

## **THE TAKE'S MATERIALS ARE WHAT THE HOME BAND HOLDS, NOT THE PROFILE'S PICK LIST.** The pick list
## binds the grant and deliberately not a take: a material a band crafted for itself must still be
## transferable to its own splinter. `clay` is on neither `PICKABLE` nor the recipe book, so a card
## drawing the pick list here fails on the row list alone.
const TAKE_MATERIALS := ["hide", "fibre", "clay"]
## The hide line: 2 already taken plus 2 still at home, which is the cap the card may raise to.
const TAKE_HIDE := 4
const TAKE_FIBRE := 9
const TAKE_CLAY := 2
const TAKE_MATERIAL_TOTAL := TAKE_HIDE + TAKE_FIBRE + TAKE_CLAY
## Presses aimed past the hide cap, so the clamp is what stops the stepper rather than the loop.
const TAKE_HIDE_OVERPRESSES := TAKE_HIDE + 2

## The words a TAKE card must carry, as needles rather than composed formats — an expectation taken
## from the const under test moves with it and passes on the very rename it exists to catch.
const TAKE_METER_NEEDLE := "left at home"
const TAKE_SUBTITLE_NEEDLE := "take from"
## …and the grant's own meter wording, asserted ABSENT from a take card: what a grant leaves unspent
## is forfeited on the advance and what a take leaves is not, so one meter must not wear the other's
## words.
const GRANT_METER_NEEDLE := "/ 30 left"
## The orb noun a take may never use, for the same reason. Supply left at home is lost by nobody.
const TAKE_FORBIDDEN_NOUN := "unspent"

## The word every retired forfeiture claim was built on — the commit control's old conditional face
## and the subtitle's old second clause alike. Asserted ABSENT from the whole card: the unspent
## warning is the turn orb's, and a second copy here is what this needle exists to catch coming back.
const FORFEIT_NEEDLE := "forfeit"
## …and the refusal notice that used to be posted on a still-open frame after a commit.
const REFUSAL_NEEDLE := "refused"

## **THE FLOOR A `ready` INK MUST CLEAR AGAINST ITS OWN PALETTE'S OTHER ACCENTS**, as a straight RGB
## distance normalised so 1.0 is black-to-white. It exists because "blue means done" is worth nothing
## if the blue reads as the accent beside it — and because loam's `SIGNAL` is already a blue and
## console's already a cyan, which is exactly where one hex pasted into four palettes lands on top of
## something. **Both of those inks were retuned when this assertion first ran** (loam 0.20 → 0.25,
## console 0.17 → 0.25 against their own `SIGNAL`) rather than the bar being lowered to admit them.
##
## MEASURED, and the floor sits under the true worst with room rather than on it: the tightest of the
## sixteen pairs is ember's blue against its sage `HEALTHY` at **0.239**, then loam and console at
## 0.252/0.254 against their own `SIGNAL`; the widest is console's 0.568 against `WARN`.
const READY_MIN_SEPARATION := 0.20

## The palette keys the ready ink is measured against — the three that can share the orb's face with
## it, plus the `HEALTHY` green a "done" colour is most likely to be confused with.
const READY_RIVAL_KEYS := ["SIGNAL", "WARN", "DANGER", "HEALTHY"]

## The affordance a non-locating row that opens a panel wears. Asserted rather than assumed, because
## the failure it catches is a row that WEARS it and does nothing — and this row is the only
## guaranteed way back to a dismissed card.
const ORB_OPEN_AFFORDANCE := "Open ▸"

## **THE ORB ROW'S DETAIL, SPELLED OUT RATHER THAN COMPOSED THROUGH `HudLoadoutVocab`.** The remainder
## must be named whatever the PICKER names it — its second column is headed `RESOURCES`, and the orb
## read `2 units unspent` beside it until a player asked what a unit was. An expectation taken from
## the const under test moves with it, so both sides of the comparison change together and the claim
## passes on the very rename it exists to catch; measured, sabotaging the const failed this claim not
## at all. These are literals for the same reason `_assert_horizon_floor_is_the_whole_trip`'s are.
const ORB_DETAIL_ONE_RESOURCE := "Brackwater — 1 resource unspent"
const ORB_DETAIL_EVERYTHING_PICKED := "Brackwater — everything is picked"
## …and the noun it must never go back to, asserted ABSENT so the rename cannot quietly revert.
const ORB_DETAIL_RETIRED_NOUN := "unit"

## **HOW MUCH TALLER THAN THE BODY THE SCROLL REGION MAY BE**, in pixels. The card is fitted to a
## measured minimum, so the honest tolerance is rounding, not a design allowance — even a row of slack
## is the dead-space defect, and the whole point of this bound is that it cannot be satisfied by a
## card that is merely "about right".
const CARD_DEAD_SPACE_TOLERANCE := 2.0

## The face the commit control wears while a budget is unspent, captured so the fully-spent state can
## be compared against it. The claim is that the two are EQUAL.
var _unspent_commit_face := ""

func run(harness) -> void:
	h = harness
	# The two catalogues the picker JOINS onto, pushed through the seams `Main` really uses. Neither
	# is part of the loadout section: the kit roster rides the equipment config blob and a recipe's
	# input costs ride the recipe book, and the picker reads both rather than a second copy.
	h._hud.update_equipment_config(JSON.stringify(_equipment_config()))
	h._hud.update_crafting_catalogues(null, null, _recipes(), [])

	# **THE WINDOW OPENS ITSELF.** Nothing below asks it to — the first frame carrying a band whose
	# window is open is what stands the card up, which is the behaviour a player meets after watching
	# a world generate.
	#
	# **TWO SEAMS, IN THIS ORDER.** The campaign half (the pick list and the two pre-fills) rides the
	# campaign section; the WINDOW itself — `open`, both budgets, the accepted rows and a take's caps
	# — rides the COHORT, so it arrives through the same roster push `update_band_alerts` already
	# makes. The campaign half has to land first or the pre-fill has no pick list to be filtered
	# against, which is the order `Main` dispatches them in.
	h._hud.update_opening_loadout(_campaign())
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	_assert_opened_itself()
	_assert_kits_open_on_the_published_prefill()
	_assert_defaults_seeded()
	_assert_gated_recipe_is_absent()
	_assert_recipe_counts()
	_assert_reachable_first()
	_assert_orb_row()
	_assert_the_footer_says_how_to_get_back()
	_assert_no_dead_space("opened")
	await h._save("starting_loadout")

	await _pick_a_kit()
	_assert_no_dead_space("picked")
	await h._save("starting_loadout_picked")

	await _spend_both_budgets()
	_assert_no_dead_space("spent")
	await h._save("starting_loadout_spent")

	await _dismiss_and_reopen()
	await _commit_is_revisable()
	_assert_the_ready_ink_is_separable_in_every_palette()
	await _orb_states()
	# **THE TAKE ARC IS APPENDED, never interleaved.** Every state above renders with exactly one
	# window open, and the orb states assert the loadout row is the orb's ONLY entry — a second window
	# opened earlier would make those two frames evidence of somebody else's row.
	await _take_window_opens_on_the_splinter()
	await _an_untouched_commit_re_sends_the_standing_take()
	await _the_take_cap_is_the_expanded_item_list()
	await _the_switcher_reaches_the_other_band()
	await _every_row_names_its_band_and_opens_it()
	await _an_over_budget_band_is_not_outfitted()
	_assert_window_shuts()

# ---- the opening state ------------------------------------------------------

func _controller() -> StartingLoadoutController:
	return h._hud.starting_loadout_panel()

func _panel() -> StartingLoadoutPanel:
	return _controller().panel()

func _assert_opened_itself() -> void:
	h._assert_hud("loadout — the picker opens ITSELF on the first frame the window is open",
		_controller().is_expanded())

## **THE KIT COLUMN OPENS ON THE PUBLISHED PRE-FILL**, not on zeros — the material column's rule, one
## column over. A roster row is found by its own meta rather than by its face: the face is a config
## string, so a text match would only confirm the fixture back to itself.
##
## ⛔ **THE COUNTS ARE ASSERTED AS PUBLISHED, which is what catches a second clamp.** The wire's spread
## is already fitted to `kit_budget` sim-side, so a client that re-fitted it would render a different
## pre-fill from the one that was sent — and with a spread comfortably inside the budget (12 of 17)
## that re-fit would be INVISIBLE unless the individual counts are checked, since the total would
## still look reasonable.
func _assert_kits_open_on_the_published_prefill() -> void:
	var rows := _rows(HudLoadoutVocab.KIT_ROW_META)
	h._assert_hud("loadout — the `%s` kit is not offered (it grants nothing)" % KIT_NONE,
		not rows.is_empty() and not rows.has(KIT_NONE))
	for expectation in [[KIT_STALKING, DEFAULT_STALKING], ["trapping", DEFAULT_TRAPPING],
			["gathering", DEFAULT_GATHERING]]:
		var kit_id := String(expectation[0])
		var want := int(expectation[1])
		var got := _stepper_count(HudLoadoutVocab.KIT_ROW_META, kit_id)
		h._assert_hud("loadout — %s opens on the published %d (got %d)" % [kit_id, want, got],
			got == want)
	# …and a kit the pre-fill does not name really does open at zero, without which "opens on the
	# defaults" passes on a column that put the same number on every row.
	h._assert_hud("loadout — a kit the pre-fill does not name opens at 0 (got %d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, "warrior"),
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, "warrior") == 0)
	h._assert_hud("loadout — the meter accounts for the pre-fill (%d spent, %d of %d left)"
			% [_controller().kits_spent(), _controller().kits_left(), KIT_BUDGET],
		_controller().kits_spent() == KIT_DEFAULTS_TOTAL
			and _controller().kits_left() == KIT_BUDGET - KIT_DEFAULTS_TOTAL)

## …and the resources column opens on the PROFILE'S DEFAULTS rather than on zero. It is the one
## column that does, and a picker that opened both at zero would look identical in a screenshot.
func _assert_defaults_seeded() -> void:
	var rows := _rows(HudLoadoutVocab.MATERIAL_ROW_META)
	h._assert_hud("loadout — one row per pickable material, in the profile's order (%s)"
			% str(rows),
		rows == PICKABLE)
	h._assert_hud("loadout — the pile opens on the published defaults (%d of %d spent)"
			% [_controller().materials_spent(), MATERIAL_BUDGET],
		_controller().materials_spent() == DEFAULTS_TOTAL)

## **THE KNOWLEDGE-GATED BENCH TOOL IS NOT DRAWN AT ALL** — not greyed, not in a locked group. It is
## in the recipe book this chapter pushed and off the published craftable list, so a client filtering
## on anything but that list fails here.
func _assert_gated_recipe_is_absent() -> void:
	var rows := _rows(HudLoadoutVocab.RECIPE_ROW_META)
	h._assert_hud("loadout — the gated `%s` has no row at all (%d rows drawn)"
			% [RECIPE_GATED, rows.size()],
		not rows.has(RECIPE_GATED))

## The column's whole arithmetic, read off the rendered count labels. `×N` answers *"how many if the
## WHOLE pile went on this one thing"*, so the rows do not have to sum to anything.
func _assert_recipe_counts() -> void:
	for expectation in [
		[RECIPE_SPEARS, EXPECTED_SPEARS], [RECIPE_BASKETS, EXPECTED_BASKETS],
		[RECIPE_TRAPS, EXPECTED_TRAPS], [RECIPE_SLED, EXPECTED_SLED],
		[RECIPE_CROOK, EXPECTED_CROOK], [RECIPE_EARTHMOVING, EXPECTED_EARTHMOVING],
	]:
		var recipe_id := String(expectation[0])
		var want := int(expectation[1])
		var got := _recipe_count(recipe_id)
		h._assert_hud("loadout — %s reads ×%d against the default pile (got %d)"
				% [recipe_id, want, got], got == want)
	# …and the unreachable row is DIMMED rather than dropped: it is the row that says the pile is
	# short of wood, which is the most useful thing the column can tell a player.
	var row := _row_node(HudLoadoutVocab.RECIPE_ROW_META, RECIPE_EARTHMOVING)
	h._assert_hud("loadout — the unreachable %s row is present and dimmed" % RECIPE_EARTHMOVING,
		row != null and row.modulate.a < 1.0)

func _assert_reachable_first() -> void:
	var rows := _rows(HudLoadoutVocab.RECIPE_ROW_META)
	var last_seen := -1
	var ordered := true
	for recipe_id in rows:
		var count := _recipe_count(String(recipe_id))
		if last_seen >= 0 and count > last_seen:
			ordered = false
		last_seen = count
	h._assert_hud("loadout — reachable rows sort first (%s)" % str(rows), ordered)

## The orb carries the window as a NON-BLOCKING warn row. Blocking it would make the client hold a
## turn the sim is perfectly willing to advance.
func _assert_orb_row() -> void:
	var rows := _controller().attention_rows()
	var row: Dictionary = rows[0] if not rows.is_empty() else {}
	h._assert_hud("loadout — the orb carries one non-blocking row for the open window (%s)"
			% str(row.get("detail", "")),
		rows.size() == 1 and not bool(row.get("blocking", false))
			and String(row.get("kind", "")) == HudAttentionVocab.ATTENTION_KIND_OPENING_LOADOUT)

# ---- driving the steppers ---------------------------------------------------

## Press the Stalking kit's `+` through the REAL button. The panel rebuilds on every press, so the
## button is re-found each time — a cached node here would be pressing a freed control.
func _pick_a_kit() -> void:
	for _i in range(KIT_PRESSES):
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	await h._settle()
	var want_spent := KIT_DEFAULTS_TOTAL + KIT_PRESSES
	h._assert_hud("loadout — %d presses buy %d more kits, and the meter says so (%d left of %d)"
			% [KIT_PRESSES, _controller().kits_spent(), _controller().kits_left(), KIT_BUDGET],
		_controller().kits_spent() == want_spent
			and _controller().kits_left() == KIT_BUDGET - want_spent)
	# **THE COMMIT CONTROL'S FACE IS UNCONDITIONAL, and this is the half of that claim taken with a
	# budget still holding something.** Its pair rides the fully-spent state below, and neither is
	# worth anything alone: a face reading `Set out` here alone passes on a control that renames
	# itself once the budgets clear, and there alone on one that renames itself while they do not.
	_unspent_commit_face = _commit_face()
	h._assert_hud("loadout — the commit control reads `%s` with 14 kits and 2 resources unspent"
			% _unspent_commit_face,
		_unspent_commit_face == HudLoadoutVocab.COMMIT_CLEAR_LABEL)
	# …and it makes NO forfeiture claim, which is the orb's to make. Asked of the whole card, because
	# the sentence that used to say it was the SUBTITLE as well as the button.
	h._assert_hud("loadout — nothing on the card says anything is forfeited",
		not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))

## Spend BOTH budgets to the last unit, then press once more. The extra press is the point: the
## controller clamps against the budget, so the `+` is disabled and nothing can be overspent — the
## sim rejects an overspent order WHOLE, so a client that let one be composed would throw the picks
## away on send.
func _spend_both_budgets() -> void:
	var presses := 0
	while _controller().kits_left() > 0 and presses < STEPPER_PRESS_LIMIT:
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
		presses += 1
	while _controller().materials_left() > 0 and presses < STEPPER_PRESS_LIMIT:
		_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
		presses += 1
	await h._settle()
	h._assert_hud("loadout — both budgets are spent to the unit (%d kits, %d units left)"
			% [_controller().kits_left(), _controller().materials_left()],
		_controller().kits_left() == 0 and _controller().materials_left() == 0)
	var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	h._assert_hud("loadout — `+` is disabled once the budget is gone",
		plus != null and plus.disabled)
	h._assert_hud("loadout — the commit control's face did not move when the budgets cleared (%s → %s)"
			% [_unspent_commit_face, _commit_face()],
		_commit_face() == _unspent_commit_face
			and _commit_face() == HudLoadoutVocab.COMMIT_CLEAR_LABEL)

# ---- dismiss, reopen, refuse ------------------------------------------------

## **THE WINDOW IS DISMISSIBLE AND THE PICKS SURVIVE IT.** A player has to be able to pan, zoom and
## read a tile before committing, and the reopen pill is what brings the card back.
func _dismiss_and_reopen() -> void:
	_controller().collapse()
	await h._settle()
	var pill := _panel().reopen_pill()
	h._assert_hud("loadout — dismissed leaves a live reopen control on screen",
		not _controller().is_expanded() and pill != null and pill.visible)
	await h._save("starting_loadout_dismissed")
	# ⛔ **THE SNAPSHOT MUST NOT PUT THE CARD BACK.** Every frame carries the window's section while it
	# is open, so a re-render gated on *is the surface up* rather than *is the CARD up* re-expands the
	# picker the player just dismissed, on the very next snapshot and every one after it — which makes
	# the window undismissible while looking, in any single frame, exactly right.
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	h._assert_hud("loadout — a snapshot does NOT re-open a card the player dismissed",
		not _controller().is_expanded() and _controller().is_open())
	var spent := _controller().kits_spent()
	pill.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — reopening keeps every pick (%d kits)" % _controller().kits_spent(),
		_controller().is_expanded() and _controller().kits_spent() == spent)

## ⛔ **COMMITTING DOES NOT SHUT THE WINDOW, AND THE FRAME AFTER ONE IS NOT A REFUSAL.** An apply is a
## REPLACEMENT, so the sim leaves `open` true and the player may revise and re-send as often as they
## like. This block walks that whole loop: commit, take the still-open frame the sim really sends,
## reopen, change the allocation, and commit the same control again.
##
## The claim it exists for is the one a client reading `open` as a success signal fails: the card must
## come back CLEAN. An earlier cut posted *"that order was refused"* on exactly this frame, which
## under replacement semantics fires after every successful order.
func _commit_is_revisable() -> void:
	var commit := Q.find_meta_node(_panel(), HudLoadoutVocab.COMMIT_BUTTON_META) as Button
	h._assert_hud("loadout — the card carries a commit control", commit != null)
	commit.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — committing puts the card away so the player can look at the map",
		not _controller().is_expanded() and _controller().is_open())
	# The frame the sim really sends after an accepted order: the window is STILL OPEN.
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	h._assert_hud("loadout — a still-open window after a commit is not treated as a refusal",
		not _controller().is_expanded())
	_panel().reopen_pill().pressed.emit()
	await h._settle()
	h._assert_hud("loadout — the card comes back CLEAN — no refusal, no forfeiture claim",
		_controller().is_expanded()
			and not Q.has_label_containing(_panel(), REFUSAL_NEEDLE)
			and not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))
	_assert_no_dead_space("reopened")
	await h._save("starting_loadout_resent")
	# **AND THE SAME CONTROL SENDS AGAIN.** A revised allocation, then a re-send — the ordinary act
	# under replacement semantics, and one a client that latched "already committed" would refuse.
	_press_minus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
	await h._settle()
	var revised := _controller().materials_spent()
	var resend := Q.find_meta_node(_panel(), HudLoadoutVocab.COMMIT_BUTTON_META) as Button
	resend.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — a revised allocation re-sends and survives the send (%d units)" % revised,
		_controller().materials_spent() == revised and _controller().is_open())

## ⛔ **EVERY ROW NAMES ITS OWN BAND, AND ITS `Open ▸` REACHES THAT BAND.**
##
## Reported from a live run: two windows, two orb rows, both reading *"Band outfitted / everything is
## picked"* — identical and unattributable, directly beneath two idle-worker rows that DID name their
## bands. And the affordance was worse than the wording: a row carried a KIND and no band, so pressing
## either one opened whichever band the card happened to be showing.
##
## **The press is the half a rendered claim cannot make.** Both rows wear `Open ▸` whatever it reaches,
## so the row is PRESSED — through the real button, which runs `_on_reason_pressed` →
## `panel_requested` → `TurnOrbController` → `open_band` — and the SUBJECT is read back off the card.
## The card is left on the HOME band by the block above, so a press that ignored the row's subject
## would leave it there and pass every wording claim on this screen.
func _every_row_names_its_band_and_opens_it() -> void:
	await _open_orb_popover()
	var rendered := Q.turn_orb_popover_rows(h._hud.turn_orb)
	var home_row := _popover_row_for(rendered, _band_name(HOME_BAND_ENTITY))
	var take_row := _popover_row_for(rendered, _band_name(SPLINTER_BAND_ENTITY))
	h._assert_hud("loadout — each open window's row LEADS with its own band (%d rows drawn)"
			% rendered.size(),
		not home_row.is_empty() and not take_row.is_empty()
			and String(home_row["detail"]) != String(take_row["detail"]))
	h._assert_hud("loadout — …and both still wear `%s` (`%s` / `%s`)"
			% [ORB_OPEN_AFFORDANCE, home_row.get("jump", ""), take_row.get("jump", "")],
		String(home_row.get("jump", "")) == ORB_OPEN_AFFORDANCE
			and String(take_row.get("jump", "")) == ORB_OPEN_AFFORDANCE)
	# **THE PICTURE OF THE REPORTED SCREEN, FIXED.** Two windows, two rows, each leading with its own
	# band — where the report showed two rows reading `Band outfitted / everything is picked`, twice.
	# The orb's own accent is not this frame's claim (the band producers' rows are up beside these).
	await h._save("starting_loadout_orb_bands")
	# The precondition without which the press below proves nothing: the card is on the OTHER band.
	h._assert_hud("loadout — the card is on the home band before the press (subject %d)"
			% _controller().subject_band_id(),
		_controller().subject_band_id() == _band_id(HOME_BAND_ENTITY))
	(take_row["button"] as Button).pressed.emit()
	await h._settle()
	h._assert_hud("loadout — pressing the SPLINTER's row opens the SPLINTER's card (subject %d)"
			% _controller().subject_band_id(),
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(SPLINTER_BAND_ENTITY))
	_close_orb_popover()

## One popover row by the band its detail leads with. `begins_with`, because the band LEADS — a
## `contains` would also match a row that merely mentioned the band somewhere in its fact.
func _popover_row_for(rendered: Array, band_name: String) -> Dictionary:
	for row_variant in rendered:
		var row: Dictionary = row_variant
		if String(row.get("detail", "")).begins_with(band_name):
			return row
	return {}

## ⛔ **OVER BUDGET IS NOT FULLY SPENT, AND THE ORB MUST NOT PAINT IT GREEN.**
##
## The completeness test was `remaining <= 0` over a remainder clamped at zero, so a band holding MORE
## than its budget allows passed it: a live run showed a card reading `-6 / 22 left` beside a row
## calling that band outfitted. The sim bug behind the `-6` is fixed and **nothing here relies on
## that** — a state the row cannot word is exactly the state it must not paint as done.
##
## The state is staged the only way a client can reach it: the SIM publishes an allocation over the
## band's budget. The card draws a published allocation as-is (a second clamp here would disagree with
## the sim's own), so the meter goes negative exactly as it did on the screen that was reported.
func _an_over_budget_band_is_not_outfitted() -> void:
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band()])
	await h._settle()
	h._assert_hud("loadout/over — the over-budget band's card opens (subject %d)"
			% _controller().subject_band_id(),
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(OVER_BAND_ENTITY))
	# **THE CARD REALLY DOES READ NEGATIVE.** Without this the row's claim is about a band that is
	# merely unspent, which the `warn` arm has always handled.
	h._assert_hud("loadout/over — the card's meter reads `%s`" % OVER_METER_NEEDLE,
		Q.has_label_containing(_panel(), OVER_METER_NEEDLE))
	var row := _band_attention_row(_band_id(OVER_BAND_ENTITY))
	h._assert_hud("loadout/over — the row is `%s`, not `%s` (got `%s`)"
			% [HudAttentionVocab.ATTENTION_SEVERITY_WARN,
				HudAttentionVocab.ATTENTION_SEVERITY_READY, row.get("severity", "")],
		String(row.get("severity", "")) == HudAttentionVocab.ATTENTION_SEVERITY_WARN)
	h._assert_hud("loadout/over — …and it reads `%s`, never `%s` (got `%s`)"
			% [HudLoadoutVocab.ATTENTION_LABEL_OVER, HudLoadoutVocab.ATTENTION_LABEL_READY,
				row.get("label", "")],
		String(row.get("label", "")) == HudLoadoutVocab.ATTENTION_LABEL_OVER)
	# …and its detail says which way it is wrong, in both currencies, behind its own band's name.
	h._assert_hud("loadout/over — the detail names the band and both overspends (`%s`)"
			% row.get("detail", ""),
		String(row.get("detail", "")) == OVER_DETAIL)
	h._assert_hud("loadout/over — …and never claims everything is picked",
		not String(row.get("detail", "")).contains(ORB_DETAIL_READY_NEEDLE))
	_assert_no_dead_space("over")
	await h._save("starting_loadout_over_budget")

## One producer row by the band it is about, read off the CONTROLLER — the popover holds every
## producer's rows and this claim is about which one this band got.
func _band_attention_row(band_id: int) -> Dictionary:
	for row_variant in _controller().attention_rows():
		var row: Dictionary = row_variant
		if int(row.get(HudAttentionVocab.ATTENTION_PANEL_SUBJECT, HudConst.NO_BAND_ID)) == band_id:
			return row
	return {}

## The turn advanced: the sim says every window has shut, and the whole surface goes with it. **The
## BANDS are still there** — it is their windows that closed, which is the frame the sim really sends
## and not the same thing as a roster going empty.
func _assert_window_shuts() -> void:
	var home := _grant_band()
	(home[HudLoadoutVocab.WINDOW_KEY] as Dictionary)[HudLoadoutVocab.OPEN_KEY] = false
	var splinter := _splinter_band()
	(splinter[HudLoadoutVocab.WINDOW_KEY] as Dictionary)[HudLoadoutVocab.OPEN_KEY] = false
	var over := _over_budget_band()
	(over[HudLoadoutVocab.WINDOW_KEY] as Dictionary)[HudLoadoutVocab.OPEN_KEY] = false
	h._hud.update_band_alerts([home, splinter, over])
	h._assert_hud("loadout — a shut window takes the whole surface off screen",
		not _controller().is_open())
	h._assert_hud("loadout — …and the orb's rows go with it",
		_controller().attention_rows().is_empty())

# ---- the TAKE a split opens ---------------------------------------------------------------------

## ⛔ **A SPLIT OPENS A WINDOW ON THE SPLINTER, AND IT IS A TAKE ON THE HOME BAND.** The picks MOVE
## gear rather than minting it, so three things on the card have to change and nothing else may: the
## meters read against what the home band can supply, the subtitle says where the gear comes from,
## and the resources column lists what that band HOLDS rather than the profile's pick list.
##
## **The card stands ITSELF up on the splinter**, exactly as it did on the spawned band — a player who
## has just split a band should not have to find the screen.
func _take_window_opens_on_the_splinter() -> void:
	h._hud.update_band_alerts([_grant_band(), _splinter_band()])
	await h._settle()
	h._assert_hud("loadout/take — the splinter's window opens the card on the SPLINTER (subject %d)"
			% _controller().subject_band_id(),
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(SPLINTER_BAND_ENTITY)
			and _controller().is_take())
	# **THE COPY SAYS WHERE THE GEAR COMES FROM.** A take that read like a grant would tell the player
	# they are minting kit for the splinter when they are taking it off the band next door.
	h._assert_hud("loadout/take — the subtitle says the gear comes from the home band",
		Q.has_label_containing(_panel(), TAKE_SUBTITLE_NEEDLE)
			and Q.has_label_containing(_panel(), _band_name(HOME_BAND_ENTITY)))
	# …and it still claims nothing is forfeited, which on a take would be false twice over: supply
	# left at home is lost by nobody.
	h._assert_hud("loadout/take — the card makes no forfeiture claim",
		not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))
	h._assert_hud("loadout/take — the meters read against the home band's supply, not a budget",
		Q.has_label_containing(_panel(), TAKE_METER_NEEDLE)
			and not Q.has_label_containing(_panel(), GRANT_METER_NEEDLE))
	# **THE PICK LIST DOES NOT BIND A TAKE.** The rows are what the home band holds — `clay` included,
	# which the profile never offered — so a card drawing `PICKABLE` here fails on the row list alone.
	var rows := _rows(HudLoadoutVocab.MATERIAL_ROW_META)
	h._assert_hud("loadout/take — the resources column lists what the HOME BAND holds (%s)" % str(rows),
		rows == TAKE_MATERIALS and rows != PICKABLE)
	# **THE METERS COUNT WHAT THE SPLIT ALREADY MOVED.** The kit meter's currency is ITEM UNITS, so the
	# two standing `big_game` kits read as four; the material meter counts the two hide.
	var spent_items := TAKE_DEFAULT_BIG_GAME * KIT_STALKING_ITEMS
	h._assert_hud("loadout/take — the kit meter reads the standing take against the supply (%d of %d)"
			% [_controller().kits_spent(), TAKE_ITEM_TOTAL],
		_controller().kits_spent() == spent_items
			and _controller().kits_left() == TAKE_ITEM_TOTAL - spent_items)
	h._assert_hud("loadout/take — …and the material meter does the same (%d of %d)"
			% [_controller().materials_spent(), TAKE_MATERIAL_TOTAL],
		_controller().materials_spent() == TAKE_DEFAULT_HIDE
			and _controller().materials_left() == TAKE_MATERIAL_TOTAL - TAKE_DEFAULT_HIDE)
	# ⛔ **THE CARD OPENS ON THE STANDING TAKE, NOT AT ZERO — the whole point of the fix.** The split's
	# default take is kit-denominated and published on the window, so the splinter's card draws the
	# allocation it is already standing on. Asserted ROW BY ROW rather than on the meter: a card that
	# opened at zero and a card that opened on somebody else's spread both move a meter.
	h._assert_hud("loadout/take — %s opens on the standing %d (got %d)"
			% [KIT_STALKING, TAKE_DEFAULT_BIG_GAME,
				_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == TAKE_DEFAULT_BIG_GAME)
	h._assert_hud("loadout/take — …and hide opens on the standing %d (got %d)"
			% [TAKE_DEFAULT_HIDE,
				_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[0])],
		_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[0]) == TAKE_DEFAULT_HIDE)
	# …and a row the take does NOT name really does open at zero, without which "opens on the standing
	# take" passes on a column that put the same number on every row.
	h._assert_hud("loadout/take — a kit the take does not name opens at 0 (got %d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, "trapping"),
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, "trapping") == 0)
	_assert_no_dead_space("take")
	await h._save("starting_loadout_take")

## ⛔ **AN UNTOUCHED `Set out` RE-SENDS WHAT IS SHOWN, WHICH IS AN EXACT NO-OP.** That is the fix this
## whole fixture exists for: an apply is a REPLACEMENT, so while the card opened EMPTY on a splinter
## this same press ordered *take nothing* and handed the band's whole dowry back to its parent.
##
## **The claim is the composed ORDER, not the rendered steppers.** The card showing the right numbers
## and the commit sending them are two different things, and only the payload can say the second —
## which is why this presses the real control and reads what the HUD emitted.
##
## **Nothing special-cases an empty tail.** An empty order still means *take nothing* here exactly as
## it does on a grant; what makes the press safe is that the card is not empty.
func _an_untouched_commit_re_sends_the_standing_take() -> void:
	var sent: Array = []
	var sink := func(payload: Dictionary) -> void: sent.append(payload)
	h._hud.set_starting_loadout_requested.connect(sink)
	var commit := Q.find_meta_node(_panel(), HudLoadoutVocab.COMMIT_BUTTON_META) as Button
	commit.pressed.emit()
	await h._settle()
	h._hud.set_starting_loadout_requested.disconnect(sink)
	var order: Dictionary = sent[0] if not sent.is_empty() else {}
	h._assert_hud("loadout/take — the commit names the SPLINTER by its durable band id (got %d)"
			% int(order.get("band_id", HudConst.NO_BAND_ID)),
		sent.size() == 1
			and int(order.get("band_id", HudConst.NO_BAND_ID)) == _band_id(SPLINTER_BAND_ENTITY))
	h._assert_hud("loadout/take — an untouched commit re-sends the standing kits (%s)"
			% str(_order_rows(order, "kits", "count")),
		_order_rows(order, "kits", "count") == {KIT_STALKING: TAKE_DEFAULT_BIG_GAME})
	h._assert_hud("loadout/take — …and the standing materials, unchanged (%s)"
			% str(_order_rows(order, "materials", "units")),
		_order_rows(order, "materials", "units") == {TAKE_MATERIALS[0]: TAKE_DEFAULT_HIDE})
	# The commit collapses the card so the player can look at the map; the pill brings it back with
	# every pick intact, which is what the cap walk below then moves.
	_panel().reopen_pill().pressed.emit()
	await h._settle()
	h._assert_hud("loadout/take — the card comes back on the same band, picks intact",
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(SPLINTER_BAND_ENTITY)
			and _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == TAKE_DEFAULT_BIG_GAME)

## One half of a composed order as `id -> amount`. A DICT rather than the emitted array, so the claim
## is about the rows and not about the order a Dictionary happened to hand back its keys in.
func _order_rows(order: Dictionary, key: String, amount_key: String) -> Dictionary:
	var rows: Dictionary = {}
	for row_variant in order.get(key, []):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		rows[String(row.get("id", ""))] = int(row.get(amount_key, 0))
	return rows

## ⛔ **THE CAP IS THE EXPANDED ITEM LIST, WHOLE — a kit row cannot be capped on its own.** `sled` is
## used by both `big_game` and `trapping`, so the two rows ADD against one supply line. This walks
## exactly that: `big_game` runs out at the SLED (five) rather than at its own spears (six), `trapping`
## is then capped at zero with four traps still at home, and giving two sleds back frees it again.
##
## A per-row cap would pass every claim above and fail here — which is the whole reason this block
## exists rather than a bound on one row.
func _the_take_cap_is_the_expanded_item_list() -> void:
	var presses := 0
	while presses < STEPPER_PRESS_LIMIT:
		var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
		if plus == null or plus.disabled:
			break
		plus.pressed.emit()
		presses += 1
		await h._settle()
	var taken := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	h._assert_hud("loadout/take — %s stops at %d, bound by the SLED and not by its %d spears (got %d)"
			% [KIT_STALKING, TAKE_BIG_GAME_CEILING, TAKE_SPEARS, taken],
		taken == TAKE_BIG_GAME_CEILING)
	var trapping_plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, "trapping")
	h._assert_hud("loadout/take — …and `trapping` is capped at 0 with %d traps still at home"
			% TAKE_TRAPS,
		trapping_plus != null and trapping_plus.disabled)
	h._assert_hud("loadout/take — the meter counts ITEM UNITS, the currency the cap is in (%d spent)"
			% _controller().kits_spent(),
		_controller().kits_spent() == TAKE_BIG_GAME_CEILING * KIT_STALKING_ITEMS)
	_assert_no_dead_space("take_capped")
	await h._save("starting_loadout_take_capped")
	# **GIVE THE SLEDS BACK AND THE OTHER ROW OPENS.** The two rows share one supply line, so this is
	# the same claim from the other side — and it is what a per-row cap gets wrong in both directions.
	for _i in range(TAKE_BIG_GAME_RELEASED):
		_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	await h._settle()
	trapping_plus = _plus_button(HudLoadoutVocab.KIT_ROW_META, "trapping")
	h._assert_hud("loadout/take — releasing %d sleds lets `trapping` be taken again"
			% TAKE_BIG_GAME_RELEASED,
		trapping_plus != null and not trapping_plus.disabled)
	# The MATERIAL cap is per material and needs no expansion — pressed past the pile to prove the
	# clamp is what stops it.
	for _i in range(TAKE_HIDE_OVERPRESSES):
		_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[0])
	await h._settle()
	h._assert_hud("loadout/take — hide clamps at the %d the supply names (%d)"
			% [TAKE_HIDE, _stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[0])],
		_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[0]) == TAKE_HIDE)

## **TWO OPEN WINDOWS, TWO ORB ROWS, AND A SWITCHER — the way to the other card for a player who never
## opens the popover.** The row's own `Open ▸` is the other, and it is asserted by the block below;
## while a row carried a KIND and no band this switcher was the only one.
func _the_switcher_reaches_the_other_band() -> void:
	var rows := _controller().attention_rows()
	h._assert_hud("loadout — the orb carries one row per open window (%d)" % rows.size(),
		rows.size() == 2)
	var take_row: Dictionary = {}
	for row_variant in rows:
		var row: Dictionary = row_variant
		if String(row.get("detail", "")).begins_with(_band_name(SPLINTER_BAND_ENTITY)):
			take_row = row
	# ⛔ **A TAKE'S ROW NEVER SAYS `unspent`.** That word is on the orb because a grant's remainder is
	# GONE on the turn advance; a take's supply simply stays with the home band.
	h._assert_hud("loadout/take — the orb row names its OWN band and never says `%s` (`%s`)"
			% [TAKE_FORBIDDEN_NOUN, take_row.get("detail", "")],
		not take_row.is_empty()
			and not String(take_row.get("detail", "")).contains(TAKE_FORBIDDEN_NOUN))
	h._assert_hud("loadout/take — …and it is not blocking, like every loadout row",
		not bool(take_row.get("blocking", false)))
	var tabs := _rows(HudLoadoutVocab.BAND_TAB_META)
	h._assert_hud("loadout — the card carries one tab per open window (%s)" % str(tabs),
		tabs.size() == 2)
	var home_tab := _row_node(HudLoadoutVocab.BAND_TAB_META, str(_band_id(HOME_BAND_ENTITY)))
	h._assert_hud("loadout — the home band has a tab of its own", home_tab != null)
	(home_tab as Button).pressed.emit()
	await h._settle()
	h._assert_hud("loadout — pressing it renders the HOME band's grant (subject %d, take %s)"
			% [_controller().subject_band_id(), str(_controller().is_take())],
		_controller().subject_band_id() == _band_id(HOME_BAND_ENTITY)
			and not _controller().is_take())
	h._assert_hud("loadout — …and that card is a GRANT again, meters and all",
		Q.has_label_containing(_panel(), GRANT_METER_NEEDLE)
			and not Q.has_label_containing(_panel(), TAKE_METER_NEEDLE))
	_assert_no_dead_space("switched")
	await h._save("starting_loadout_bands")

## ⛔ **NO BAND OF EMPTY SPACE UNDER THE COLUMNS.**
##
## Reported from the real client: picking a single kit grew the card by roughly 400px, all of it dead
## space between the bottom of the kit list and the footer rule, with the three columns rendering
## identically. Nothing in the CONTENT can do that; only the FIT can, by measuring a body full of
## `AUTOWRAP_WORD_SMART` labels against a width they were not laid out at — see
## `StartingLoadoutPanel.refit`.
##
## **This walk never reproduced it** (see the rule file), so this is a BOUND rather than a repro.
##
## ⛔ **IT IS ASKED OF THE SCROLL REGION, NOT OF THE CARD, and the first version asked the card and was
## VACUOUS IN EXACTLY THE REPORTED CASE.** That one compared the panel against
## `PanelContainer.get_combined_minimum_size()` and skipped itself when the internal scroll was on —
## but a card that has grown past the room's ceiling turns the scroll ON, so the skip fired precisely
## when the defect was present. Proven: with 400px of dead space injected, the card-based form printed
## nothing at all and the run stayed green.
##
## The scroll region is where the space would actually be, and the question has one honest form in
## both regimes: **the region may be SHORTER than the body wants (the room's ceiling doing its job,
## with the internal scrollbar carrying the rest) but never TALLER.** One-sided, so it needs no skip
## and has nowhere to hide.
##
## Asked in EVERY card state the chapter renders, because the defect appeared on an INTERACTION rather
## than on a mount and a bound checked only on the opening frame would not have seen it.
func _assert_no_dead_space(arm: String) -> void:
	var card := _panel().card()
	var body: Control = card.find_child("LoadoutBody", true, false)
	var scroll: Control = card.find_child("LoadoutScroll", true, false)
	if body == null or scroll == null:
		h._assert_hud("loadout/%s — the card still has a body and a scroll to measure" % arm, false)
		return
	var slack := scroll.size.y - body.get_combined_minimum_size().y
	h._assert_hud("loadout/%s — no dead space under the columns (%.0f px of slack in the scroll)"
			% [arm, slack],
		slack <= CARD_DEAD_SPACE_TOLERANCE)

## **THE FOOTER TELLS THE PLAYER HOW TO GET THIS CARD BACK, AND WHEN THEY CANNOT.** Two facts, and
## both are asserted, because either alone leaves the player stuck: the orb is the only way back to a
## dismissed card, and the turn is the thing that ends it.
##
## ⛔ **THE RETIRED FORFEITURE CLAIM STAYS ABSENT BESIDE THEM.** That one said COMMITTING shuts the
## window, which is false — an apply is a replacement. This names the TURN, which is true. They are one
## `forfeit` apart in the source and must not drift back together, so the negative rides here.
func _assert_the_footer_says_how_to_get_back() -> void:
	var note := HudLoadoutVocab.FOOTER_NOTE
	h._assert_hud("loadout — the footer says the ORB brings this back (\"%s\")" % note,
		Q.has_label_containing(_panel(), FOOTER_ORB_NEEDLE)
			and note.contains(FOOTER_ORB_NEEDLE))
	h._assert_hud("loadout — …and that ENDING THE TURN is what closes it",
		Q.has_label_containing(_panel(), FOOTER_TURN_NEEDLE)
			and note.contains(FOOTER_TURN_NEEDLE))
	h._assert_hud("loadout — …while still claiming nothing is forfeited by committing",
		not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))

## The two facts, as the words only this line says. Needles rather than the whole sentence, so a
## reworded second clause does not silently stop being asserted — what must survive is the FACT.
const FOOTER_ORB_NEEDLE := "turn orb"
const FOOTER_TURN_NEEDLE := "Ending the turn"

# ---- the orb, in both of its states ------------------------------------------

## **THE TWO ORB STATES, AND THEY ARE JUDGED AS A PAIR.** Yellow with something unspent, blue with
## everything picked — either claim alone passes on an orb whose accent never moves, so both are made
## on the same registry with only the allocation between them.
##
## ⛔ **ALL THREE OTHER HALVES OF THE REGISTRY ARE CLEARED FIRST, AND HANDED BACK AFTER.** The orb's
## accent is the colour of the HIGHEST-ranked entry and `ready` ranks below everything, so ANY other
## row present paints these two frames instead and they become evidence of nothing. Clearing the band
## half alone was not enough — measured: the orb came back `DANGER` on both arms, off a pending
## narrative fork this long-lived HUD was still holding from the `telling` chapter.
##
## Cleared at the CACHE rather than at the node, the `turn_orb` chapter's own rule:
## `TurnOrb.set_attention([])` empties only the node and the next `_push_attention` resurrects
## everything. The fork and knowledge halves are written directly because their public setters do more
## than set — `update_pending_forks` also AUTO-OPENS the fork panel, which would put a card over these
## frames — and one `set_band_attention` at the end pushes all three.
func _orb_states() -> void:
	var held_bands: Array = h._hud._turnorb._band_attention
	var held_knowledge: Array = h._hud._turnorb._knowledge_attention
	var held_forks: Array = h._hud._turnorb._pending_forks
	h._hud._turnorb._knowledge_attention = []
	h._hud._turnorb._pending_forks = []
	h._hud._turnorb.set_band_attention([])
	# **UNSPENT — the warn arm.** `_commit_is_revisable` left one unit off the pile, so the window is
	# open with something still to pick.
	await h._settle()
	# The precondition without which every accent claim below is about somebody else's row.
	h._assert_hud("loadout — the loadout row is the orb's ONLY entry for these two frames (%d)"
			% h._hud.turn_orb._entries.size(),
		h._hud.turn_orb._entries.size() == 1)
	_assert_orb_state("unspent", HudAttentionVocab.ATTENTION_SEVERITY_WARN, HudStyle.WARN)
	await _open_orb_popover()
	_assert_orb_row_reads("unspent", HudLoadoutVocab.ATTENTION_LABEL_UNSPENT,
		ORB_DETAIL_ONE_RESOURCE)
	await h._save("starting_loadout_orb_unspent")
	_close_orb_popover()

	# **COMPLETE — the ready arm.** One press puts the last unit back, and nothing else changes.
	_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
	await h._settle()
	h._assert_hud("loadout — the last unit really is spent (%d kits, %d units left)"
			% [_controller().kits_left(), _controller().materials_left()],
		_controller().kits_left() == 0 and _controller().materials_left() == 0)
	_assert_orb_state("complete", HudAttentionVocab.ATTENTION_SEVERITY_READY, HudStyle.READY)
	await _open_orb_popover()
	_assert_orb_row_reads("complete", HudLoadoutVocab.ATTENTION_LABEL_READY,
		ORB_DETAIL_EVERYTHING_PICKED)
	await h._save("starting_loadout_orb_ready")
	_close_orb_popover()
	h._hud._turnorb._knowledge_attention = held_knowledge
	h._hud._turnorb._pending_forks = held_forks
	h._hud._turnorb.set_band_attention(held_bands)

## One arm of the pair: the registry's own row, the ORB'S PAINTED ACCENT, and the rendered row's words
## and affordance.
##
## **The accent is the claim a severity const cannot make.** A row can carry `ready` and paint nothing
## — that is exactly what a rank of 0 would have done — so `_accent_color` is read off the orb itself.
func _assert_orb_state(arm: String, severity: String, want: Color) -> void:
	var rows: Array = _controller().attention_rows()
	var row: Dictionary = rows[0] if not rows.is_empty() else {}
	h._assert_hud("loadout/%s — the orb carries exactly ONE loadout row, whether or not anything is left"
			% arm,
		rows.size() == 1
			and String(row.get("kind", "")) == HudAttentionVocab.ATTENTION_KIND_OPENING_LOADOUT)
	h._assert_hud("loadout/%s — it is `%s` and still not blocking (got `%s`)"
			% [arm, severity, row.get("severity", "")],
		String(row.get("severity", "")) == severity and not bool(row.get("blocking", false)))
	h._assert_hud("loadout/%s — the orb's face is painted the row's own ink (%s vs %s)"
			% [arm, h._hud.turn_orb._accent_color, want],
		h._hud.turn_orb._accent_color == want)

## …and the RENDERED row, which is what says the way back to a dismissed card is really on screen.
func _assert_orb_row_reads(arm: String, label: String, detail: String) -> void:
	var rendered := Q.turn_orb_popover_rows(h._hud.turn_orb)
	var found := {}
	for row_variant in rendered:
		var row: Dictionary = row_variant
		if String(row["label"]) == label:
			found = row
	h._assert_hud("loadout/%s — the popover row reads `%s` (%d rows drawn)"
			% [arm, label, rendered.size()],
		not found.is_empty())
	# **THE DETAIL NAMES THE BUDGET THE PICKER NAMES.** It read `2 units unspent` beside a column
	# headed `RESOURCES`, and a player asked what a unit was. The label alone cannot see that: both
	# arms carry the same label whatever the remainder is worded as. `detail` is a LITERAL from this
	# chapter — see `ORB_DETAIL_ONE_RESOURCE`.
	h._assert_hud("loadout/%s — …and its detail reads `%s` (got `%s`)"
			% [arm, detail, found.get("detail", "")],
		String(found.get("detail", "")) == detail)
	h._assert_hud("loadout/%s — …and never calls a resource a `%s` (got `%s`)"
			% [arm, ORB_DETAIL_RETIRED_NOUN, found.get("detail", "")],
		not String(found.get("detail", "")).contains(ORB_DETAIL_RETIRED_NOUN))
	h._assert_hud("loadout/%s — …and wears `%s`, the way back to a dismissed card (got `%s`)"
			% [arm, ORB_OPEN_AFFORDANCE, found.get("jump", "")],
		String(found.get("jump", "")) == ORB_OPEN_AFFORDANCE)

func _open_orb_popover() -> void:
	h._hud.turn_orb._open_popover()
	await h._settle()

func _close_orb_popover() -> void:
	if h._hud.turn_orb._popover_open:
		h._hud.turn_orb._close_popover()

## **THE READY INK IS ASSERTED IN ALL FOUR PALETTES, as DATA rather than through the one the harness
## is pinned to.** The token has to be authored per theme — three of the four put a blue or a cyan on
## `SIGNAL` already — so the failure worth catching is one hex pasted into four palettes, which lands
## on top of `SIGNAL` in at least two of them and reads as "nothing in particular" there.
##
## Two claims, and the second is what stops the first passing on four identical values that all happen
## to clear their own theme's accents.
func _assert_the_ready_ink_is_separable_in_every_palette() -> void:
	var seen: Array[Color] = []
	var worst := 1.0
	var worst_where := ""
	for theme_id in PaletteScript.THEMES.keys():
		var hud: Dictionary = (PaletteScript.THEMES[theme_id] as Dictionary)["hud"]
		if not hud.has("READY"):
			h._assert_hud("loadout — palette `%s` declares no READY ink" % theme_id, false)
			continue
		var ready: Color = hud["READY"]
		seen.append(ready)
		for key in READY_RIVAL_KEYS:
			var apart := _color_distance(ready, hud[key])
			if apart < worst:
				worst = apart
				worst_where = "%s.%s" % [theme_id, key]
	h._assert_hud("loadout — every palette's READY clears %.2f from its own accents (worst %.2f at %s)"
			% [READY_MIN_SEPARATION, worst, worst_where],
		worst >= READY_MIN_SEPARATION)
	var distinct := {}
	for color in seen:
		# Keyed by the inks own hex, String(Color) not being a constructor GDScript offers.
		distinct[color.to_html(false)] = true
	h._assert_hud("loadout — the four palettes author their OWN ready ink (%d distinct of %d)"
			% [distinct.size(), seen.size()],
		seen.size() == PaletteScript.THEMES.size() and distinct.size() == seen.size())

## Straight RGB distance, normalised so 1.0 is black-to-white. A perceptual metric would be better and
## is not needed: what is being caught is a token that landed ON another accent, not a subtle one.
func _color_distance(a: Color, b: Color) -> float:
	return sqrt((a.r - b.r) * (a.r - b.r) + (a.g - b.g) * (a.g - b.g) + (a.b - b.b) * (a.b - b.b)) \
		/ sqrt(3.0)

# ---- lookups ----------------------------------------------------------------

## Every row carrying `meta`, as the VALUES it was stamped with — the ids, in draw order. Asked of
## the rendered controls rather than of the payload, so the assertion is about what is on screen.
func _rows(meta: StringName) -> Array:
	var ids: Array = []
	_collect(_panel(), meta, ids)
	return ids

func _collect(node: Node, meta: StringName, into: Array) -> void:
	if node == null:
		return
	# ⛔ **`str()`, NEVER `String()`** — the latter is a constructor accepting only the string types and
	# RAISES on anything else, which ABORTS this chapter instead of failing a claim.
	if node is Control and (node as Control).has_meta(meta):
		into.append(str((node as Control).get_meta(meta)))
	for child in node.get_children():
		_collect(child, meta, into)

func _row_node(meta: StringName, id: String) -> Control:
	return _find_row(_panel(), meta, id)

func _find_row(node: Node, meta: StringName, id: String) -> Control:
	if node == null:
		return null
	if node is Control and (node as Control).has_meta(meta) \
			and str((node as Control).get_meta(meta)) == id:
		return node as Control
	for child in node.get_children():
		var found := _find_row(child, meta, id)
		if found != null:
			return found
	return null

## A recipe row's count, read off the label's own meta rather than off its text: the face is `×3` or
## a dash, and parsing either back into a number would be re-implementing the renderer to check it.
func _recipe_count(recipe_id: String) -> int:
	var row := _row_node(HudLoadoutVocab.RECIPE_ROW_META, recipe_id)
	if row == null:
		return -1
	var label := Q.find_meta_node(row, HudLoadoutVocab.RECIPE_COUNT_META)
	return int(label.get_meta(HudLoadoutVocab.RECIPE_COUNT_META)) if label != null else -1

## A row's stepper VALUE as rendered — the middle child of the `− n +` triple, found structurally
## rather than by text, since the text is the number under test.
func _stepper_count(meta: StringName, id: String) -> int:
	var row := _row_node(meta, id)
	if row == null:
		return -1
	var minus := Q.find_button_by_text(row, STEPPER_MINUS_FACE)
	if minus == null:
		return -1
	var parent := minus.get_parent()
	var value: Label = parent.get_child(minus.get_index() + 1) as Label
	return int(value.text) if value != null and value.text.is_valid_int() else -1

func _plus_button(meta: StringName, id: String) -> Button:
	var row := _row_node(meta, id)
	return Q.find_button_by_text(row, STEPPER_PLUS_FACE) if row != null else null

func _press_plus(meta: StringName, id: String) -> void:
	var plus := _plus_button(meta, id)
	if plus != null and not plus.disabled:
		plus.pressed.emit()

func _press_minus(meta: StringName, id: String) -> void:
	var row := _row_node(meta, id)
	var minus := Q.find_button_by_text(row, STEPPER_MINUS_FACE) if row != null else null
	if minus != null and not minus.disabled:
		minus.pressed.emit()

## The commit control's rendered FACE. Read off the button rather than off a producer, because the
## claim is about what the player is looking at.
func _commit_face() -> String:
	var commit := Q.find_meta_node(_panel(), HudLoadoutVocab.COMMIT_BUTTON_META) as Button
	return commit.text if commit != null else ""

# ---- fixtures ---------------------------------------------------------------

## **THE CAMPAIGN'S HALF — one per world.** The pick list, the two pre-fills and the craftable ids;
## `open` and the budgets are NOT here, they are facts about one band and ride the cohort below.
func _campaign() -> Dictionary:
	return {
		HudLoadoutVocab.PICKABLE_MATERIALS_KEY: PICKABLE.duplicate(),
		HudLoadoutVocab.MATERIAL_DEFAULTS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "bone",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_BONE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "fibre",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_FIBRE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "hide",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_HIDE},
		],
		HudLoadoutVocab.KIT_DEFAULTS_KEY: [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_STALKING},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: "trapping",
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_TRAPPING},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: "gathering",
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_GATHERING},
		],
		HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY: [
			RECIPE_SLED, RECIPE_CROOK, RECIPE_BASKETS, RECIPE_TRAPS, RECIPE_SPEARS,
			RECIPE_EARTHMOVING,
		],
	}

## The spawned band, carrying its own GRANT window. Stamped through `BandFx.with_band_id` like every
## fixture cohort in the walk — a `Band #<id>` on a frame means one reached the HUD without it.
func _grant_band() -> Dictionary:
	return _band(HOME_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.KIT_BUDGET_KEY: KIT_BUDGET,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: MATERIAL_BUDGET,
		# **A GRANT NAMES NO PARENT.** Its picks mint; nothing moves off another band.
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		# Nobody has ordered against it yet, which is what makes the campaign pre-fill the seed.
		HudLoadoutVocab.WINDOW_KITS_KEY: [],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [],
	})

## The splinter a split just made, carrying a TAKE on the home band. Both budgets are `0` and mean
## nothing; the cap is the two supplies, each already `the home band's holdings + this take's standing
## units`, which is the number the sim refuses on.
##
## ⛔ **ITS ACCEPTED ROWS ARE NOT EMPTY**, and a fixture that left them so would be staging the very
## state the split's kit-denominated default take exists to remove — a card opening at zero on a band
## standing on its dowry, whose untouched commit hands the dowry back.
func _splinter_band() -> Dictionary:
	return _band(SPLINTER_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.KIT_BUDGET_KEY: 0,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: 0,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: _band_id(HOME_BAND_ENTITY),
		# **THE DEFAULT TAKE, kit-denominated** — what the split already moved, which is what the card
		# opens on and what an untouched commit re-sends. Its expansion (2 spears, 2 sleds) is inside
		# the supply below by construction, the sim publishing `holdings + this take's standing units`.
		HudLoadoutVocab.WINDOW_KITS_KEY: [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: TAKE_DEFAULT_BIG_GAME},
		],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: TAKE_MATERIALS[0],
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: TAKE_DEFAULT_HIDE},
		],
		HudLoadoutVocab.PARENT_ITEM_SUPPLY_KEY: [
			_supply("spears", TAKE_SPEARS), _supply("sled", TAKE_SLED),
			_supply("traps", TAKE_TRAPS), _supply("baskets", TAKE_BASKETS),
		],
		HudLoadoutVocab.PARENT_MATERIAL_SUPPLY_KEY: [
			_supply(TAKE_MATERIALS[0], TAKE_HIDE), _supply(TAKE_MATERIALS[1], TAKE_FIBRE),
			_supply(TAKE_MATERIALS[2], TAKE_CLAY),
		],
	})

## **A BAND WHOSE PUBLISHED ALLOCATION IS OVER ITS BUDGET** — the reported screen, staged the one way
## a client can reach it. Its grant budgets are the post-split ones and its accepted rows are the
## pre-split allocation, which is the shape the duplication bug left behind; the card draws a
## published allocation as-is, so both meters read negative.
func _over_budget_band() -> Dictionary:
	return _band(OVER_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.KIT_BUDGET_KEY: OVER_KIT_BUDGET,
		HudLoadoutVocab.MATERIAL_BUDGET_KEY: OVER_MATERIAL_BUDGET,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		HudLoadoutVocab.WINDOW_KITS_KEY: [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: OVER_KITS_HELD},
		],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: PICKABLE[0],
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: OVER_UNITS_HELD},
		],
	})

## One player-faction cohort carrying one window — the shape `update_band_alerts` consumes.
func _band(entity: int, window: Dictionary) -> Dictionary:
	return BandFx.with_band_id({
		"entity": entity,
		"faction": HudConst.PLAYER_FACTION_ID,
		"size": BAND_FIXTURE_SIZE,
		"working_age": KIT_BUDGET,
		"current_x": BAND_FIXTURE_X,
		"current_y": BAND_FIXTURE_Y,
		"idle_workers": 0,
		HudLoadoutVocab.WINDOW_KEY: window,
	})

func _supply(id: String, units: int) -> Dictionary:
	return {HudLoadoutVocab.SUPPLY_ID_KEY: id, HudLoadoutVocab.SUPPLY_UNITS_KEY: units}

## The durable id `BandFx.with_band_id` stamps onto a cohort — the handle the command names and the
## key the controller holds a window under.
func _band_id(entity: int) -> int:
	return entity + BandFx.FIXTURE_BAND_ID_OFFSET

## …and the name it stamps beside it, which is what the take's copy and its orb row quote.
func _band_name(entity: int) -> String:
	return BandFx.FIXTURE_BAND_NAMES[_band_id(entity) % BandFx.FIXTURE_BAND_NAMES.size()]

## The shipped kit roster's shape, `none` included — the picker has to drop it, so the fixture has to
## offer it.
func _equipment_config() -> Dictionary:
	return {HudLoadoutVocab.CONFIG_KITS_KEY: [
		_kit(KIT_STALKING, "Stalking kit", ["hunt"], ["spears", "sled"]),
		_kit("trapping", "Trapping kit", ["hunt"], ["traps", "sled"]),
		_kit("gathering", "Harvesting kit", ["forage"], ["baskets"]),
		_kit("hurdling", "Hurdling kit", ["builders", "husbandry"], ["crook"]),
		_kit("tillage", "Tillage kit", ["builders", "agriculture"], ["hoes"]),
		_kit("roadbuilding", "Roadbuilding kit", ["builders", "roadwork"], ["earthmoving"]),
		_kit("paving", "Paving kit", ["builders", "roadwork"], ["stone_dressing"]),
		_kit("wayfinding", "Wayfinding kit", ["scout"], ["wayfinding"]),
		_kit("warrior", "Warrior kit", ["warrior"], ["clubs"]),
		_kit(KIT_NONE, "No kit", ["hunt", "forage"], []),
	]}

func _kit(id: String, display_name: String, jobs: Array, uses: Array) -> Dictionary:
	return {
		HudLoadoutVocab.KIT_ID_KEY: id,
		HudLoadoutVocab.KIT_DISPLAY_NAME_KEY: display_name,
		HudLoadoutVocab.KIT_JOBS_KEY: jobs,
		HudLoadoutVocab.KIT_USES_KEY: uses,
	}

## The recipe book, in the wire's own shape — the shipped costs, transcribed, plus the gated tool
## that must not be drawn.
func _recipes() -> Array:
	return [
		_recipe(RECIPE_SLED, "Sled", 8.0, {"hide": 6.0, "fibre": 2.0}),
		_recipe(RECIPE_CROOK, "Crook", 5.0, {"bone": 1.0, "fibre": 2.0}),
		_recipe(RECIPE_BASKETS, "Baskets", 6.0, {"fibre": 5.0, "hide": 1.0}),
		_recipe(RECIPE_TRAPS, "Traps", 6.0, {"fibre": 6.0, "bone": 1.0}),
		_recipe(RECIPE_SPEARS, "Spears", 6.0, {"bone": 1.0, "fibre": 2.0, "hide": 1.0}),
		_recipe(RECIPE_EARTHMOVING, "Earthmoving tools", 8.0, {"wood": 3.0, "bone": 2.0}),
		_recipe(RECIPE_GATED, "Tanning frame", 12.0, {"fibre": 8.0, "bone": 2.0}),
	]

func _recipe(id: String, display_name: String, work: float, inputs: Dictionary) -> Dictionary:
	var rows: Array = []
	for material_id in inputs.keys():
		rows.append({
			HudLoadoutVocab.RECIPE_INPUT_MATERIAL_ID_KEY: String(material_id),
			HudLoadoutVocab.RECIPE_INPUT_AMOUNT_KEY: float(inputs[material_id]),
		})
	return {
		HudLoadoutVocab.RECIPE_ID_KEY: id,
		HudLoadoutVocab.RECIPE_DISPLAY_NAME_KEY: display_name,
		HudLoadoutVocab.RECIPE_WORK_KEY: work,
		HudLoadoutVocab.RECIPE_INPUTS_KEY: rows,
	}
