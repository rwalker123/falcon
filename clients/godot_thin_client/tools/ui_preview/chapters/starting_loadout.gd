extends RefCounted

## THE OUTFITTING PICKER (issue #629) — a band's outfitting window: ONE carry meter (#732), a pick
## list, and the readout that says what the pile is worth.
##
## **IT WALKS BOTH WINDOWS.** The spawned band's GRANT (picks that MINT) comes first and is most of the
## chapter; the TAKE a split opens on a splinter (picks that MOVE gear out of the home band, capped per
## ITEM as well as by the carry) is appended at the end, after the orb states, so nothing before it
## moves.
##
## ⛔ **BOTH ARE CAPPED BY THE BAND'S CARRY** — an order's load is `item weight × expanded item units +
## material weight × material units` against the window's `carry_capacity`. The fixture weights are the
## shipped 1.0 each, so a `big_game` kit (spears + sled) weighs 2 and a `gathering` kit 1.
##
## ⛔ **THE CARD DRAWS WHAT THE BAND HOLDS, AND EVERY PRESS ORDERS.** The sim applies a band's default
## outfit when it makes the band, so `loadout_window.kits` / `.materials` are gear in hands rather than
## a suggestion — and there is no deferred commit for the card to lose: a stepper press emits the whole
## allocation on the spot, and the footer control closes the card and sends nothing.
##
## ⛔ **EVERY FIXTURE BAND PUBLISHES THE LAST ORDER THIS CARD SENT FOR IT** (`_held_kits`), falling
## back to its default outfit. That is what a server does — it applies the order and republishes the
## band — and a fixture that re-stated a stale allocation instead would be a server that ignored the
## player, which the card would rightly (and confusingly) adopt.
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
const EXPECTED_CHECKPOINTS := 190

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

## The working-age hands of the fixture band. Nothing on the card reads it any more — the carry is
## the sim's answer, published on the window — but a cohort reaching `update_band_alerts` is shaped
## like one.
const WORKING_AGE := 17
## ⛔ **THE GRANT'S CARRY, IN LOAD.** The defaults weigh 48 (20 items + 28 units), so the meter opens
## with 12 left; three `big_game` presses take it to 54, and three more fill it EXACTLY — every step
## is 2, so the pack closes on the unit rather than leaving a fraction no press can spend.
const CARRY_CAPACITY := 60.0
## ⛔ **THE OPENING BAND'S LARDER, IN LOAD — a FIXED larder** (`food_fixed`): it counts against the
## carry without yielding to goods, so the window publishes the WHOLE carry (goods room + larder) and
## the goods allowance is that less the larder — `CARRY_CAPACITY` above, unchanged. The card states it
## as the bar's food segment and `Food 18`, which is what says why the goods room is smaller.
const GRANT_FOOD := 18.0
const GRANT_WHOLE_CARRY := CARRY_CAPACITY + GRANT_FOOD
const GRANT_FOOD_LINE := "Food 18"
## The shipped `trade.item_carry_weight` / `trade.material_carry_weight`.
const ITEM_CARRY_WEIGHT := 1.0
const MATERIAL_CARRY_WEIGHT := 1.0
## The shipped pick list, in the profile's own order — which is also the draw order of the resources
## column and of the legend above the recipe list.
const PICKABLE := ["bone", "fibre", "hide", "wood", "stone"]
## …and the pile the band already holds, the profile's own spread as the sim applied it. With the kits
## it weighs 48 of 60, deliberately NOT the whole carry: a fixture holding its whole carry could not
## tell a working meter from one stuck at zero.
const DEFAULT_BONE := 3
const DEFAULT_FIBRE := 17
const DEFAULT_HIDE := 8
const DEFAULTS_TOTAL := DEFAULT_BONE + DEFAULT_FIBRE + DEFAULT_HIDE

## The KITS THE BAND HOLDS — the shipped spread. **The sim fitted these to the carry when it applied
## them**; the fixture states them as the sim would and the picker must draw them unchanged.
const DEFAULT_STALKING := 4
const DEFAULT_TRAPPING := 4
const DEFAULT_GATHERING := 4
## What one kit of each weighs at the shipped item weight — the items it EXPANDS to.
const KIT_STALKING_ITEMS := 2
const KIT_TRAPPING_ITEMS := 2
const KIT_GATHERING_ITEMS := 1
## The opening load: 4×2 + 4×2 + 4×1 items and 28 units — spelled as the sum the server weighs.
const DEFAULT_LOAD := (DEFAULT_STALKING * KIT_STALKING_ITEMS + DEFAULT_TRAPPING * KIT_TRAPPING_ITEMS
	+ DEFAULT_GATHERING * KIT_GATHERING_ITEMS) * ITEM_CARRY_WEIGHT + DEFAULTS_TOTAL * MATERIAL_CARRY_WEIGHT
## ⛔ **WHAT ONE MORE `big_game` COSTS, SAID ON ITS ROW** — a kit that puts two items in hands weighs
## 2, which is not obvious. A LITERAL, the needle rule: a needle composed through the format under test
## moves with it. The one-item `gathering` row must carry no such clause.
const KIT_CARRY_COST_NEEDLE := "· 2 carry"
const CARRY_COST_WORD := "carry"

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
## …and the SECOND kit, which only the adoption states move — a row pressed on a band nothing above
## has touched, so "the sim's rows replaced the card's" is a claim about a row with a history.
const KIT_GATHERING := "gathering"
## The roster entry that must NEVER be offered — it grants nothing, which is how the picker knows.
const KIT_NONE := "none"

# ---- the OVER-BUDGET band (the reported screen) --------------------------------------------------

const OVER_BAND_ENTITY := 6203
## Its carry is the one the split left it with; its accepted rows are what it still claims — 7
## `big_game` (14 items) and 16 bone against 24, i.e. 6 over. ⛔ **THE BAND IS NOT MOVING, SO NOTHING
## ON THE CARD OR THE ORB SAYS SO** (the maintainer's rule — the long-move targeting warning is the one
## place over-carry is stated): the meter reads `0 /` in its calm ink and the orb calls it outfitted.
const OVER_CARRY_CAPACITY := 24.0
const OVER_KITS_HELD := 7
const OVER_UNITS_HELD := 16
## What the card's own meter must read — the free room FLOORED AT ZERO, spelled as a LITERAL rather
## than composed through `CARRY_REMAINING_FORMAT`, since an expectation built from the format under
## test can only agree with itself.
const OVER_METER_NEEDLE := "0 / 24 carry left"
## …and the row's whole detail, by equality: the band leads, then the READY arm's own words.
const OVER_DETAIL := "Windmere — everything is picked"
## The retired over-carry wording, asserted ABSENT from the row: `6 carry over`.
const RETIRED_CARRY_OVER_NEEDLE := "carry over"

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
## **The supply lists only items some kit carries** — the sim filters the three bench tools out, shop
## equipment staying with the workshop that built it — so a fixture naming one would be a supply no
## server can send.
##
## ⛔ **THE TAKE'S CARRY**, the splinter's TOTAL — goods load first and its food fills what they leave,
## so this is not net of food. 18 is above the sled-bound
## `big_game` walk (10 items + 2 hide = 12), so the SUPPLY is what stops that walk — and below what
## fibre's supply would take on top of the released kits, so the CARRY is what stops fibre. One window,
## both caps, each binding where the fixture aims it.
const TAKE_CARRY_CAPACITY := 18.0
## ⛔ **THE SPLINTER'S FOOD SHARE**, in load — its full proportional larder share. 14 is ABOVE the room
## the standing take leaves (18 − 6 = 12), so the card opens with the food UNDER its share and the
## amber hint up, and every goods press visibly costs food. A share inside that room would put the
## hint nowhere and make "a press moves the food line" a claim about nothing.
const TAKE_FOOD_SHARE := 14.0
## The food line the standing take opens on: the room it leaves, 12 of the 14.
const TAKE_FOOD_OPENING := "Food 12 of 14"
## …after ONE more fibre, before the server answers: the card's own preview, 11 of the 14.
const TAKE_FOOD_PREVIEW := "Food 11 of 14"
## ⛔ **THE WIRE'S FIGURE WINS ONCE THE ORDER IS ECHOED.** Staged off the card's own `min()` (which reads
## 11 for the echoed order) so the two readings are distinguishable — a card that kept previewing after
## the echo reads 11 here, one that reads the wire reads 10. No other property of this fixture moves.
const TAKE_FOOD_ECHOED := 10.0
## What every food line begins with, for reporting the line that WAS drawn when a claim fails.
const FOOD_LINE_NEEDLE := "Food "
const TAKE_FOOD_ECHOED_LINE := "Food 10 of 14"## ⛔ **THE DEFAULT TAKE THE SPLIT ALREADY MOVED, kit-denominated and published on the window** — and
## the card OPENS on it. It is what makes an untouched `Set out` an exact no-op instead of an order to
## take nothing, which is what an empty card would order the moment a stepper is pressed.
## Non-zero and unequal to the stepper's floor, so a card that opened at zero — or at one — fails on
## the count rather than on a coincidence.
const TAKE_DEFAULT_BIG_GAME := 2
const TAKE_DEFAULT_HIDE := 2

## `big_game` is sled-bound, not spear-bound — the whole point of the fixture.
const TAKE_BIG_GAME_CEILING := TAKE_SLED
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
## Presses aimed past the hide cap, so the clamp is what stops the stepper rather than the loop.
const TAKE_HIDE_OVERPRESSES := TAKE_HIDE + 2
## ⛔ **WHERE THE CARRY, NOT THE SUPPLY, STOPS A TAKE ROW.** After the release the order is 3
## `big_game` (6) + 4 hide = 10 of 18, so fibre stops at 8 with 9 still at home.
const TAKE_FIBRE_CARRY_CEILING := 8
## The take's opening load: 2 `big_game` (4 items) + 2 hide.
const TAKE_DEFAULT_LOAD := 6.0

## The words a TAKE card must carry, as needles rather than composed formats — an expectation taken
## from the const under test moves with it and passes on the very rename it exists to catch.
## ⛔ **WHAT IS FREE, NOT WHAT THE GOODS LEAVE**: 18 − 6 goods − 12 food = 0. The goods leave 12, and
## the food fills exactly that, so a meter reading `12 /` would offer room nothing can use.
const TAKE_METER_OPENING := "0 / 18 carry left"
const TAKE_METER_NEEDLE := "/ 18 carry left"
const TAKE_SUBTITLE_NEEDLE := "take from"
## …and the GRANT's carry, asserted ABSENT from a take card and present on the grant again — one
## meter, two bands, two capacities, so the denominator is what tells the cards apart.
const GRANT_METER_NEEDLE := "/ 78 carry left"
## …and the grant's opening meter, what is actually free: 78 − 48 goods − 18 food.
const GRANT_METER_OPENING := "12 / 78 carry left"
## The orb noun a take may never use, for the same reason. Supply left at home is lost by nobody.
const TAKE_FORBIDDEN_NOUN := "unspent"

# ---- the band whose allocation the SIM moves, and the one that proves two presses hold ------------

## ⛔ **ITS OWN BAND, because adoption is a claim about a band whose card has a HISTORY.** Every other
## band here has been edited by the states above and would confuse "the sim's rows replaced mine" with
## "nothing happened". It opens LAST, after every orb claim has been made, and the chapter's closing
## shut-window push simply omits it.
const ADOPT_BAND_ENTITY := 6204

## **A REFUSAL, in the sim's own words** — the wire's `detail`, lowercase and with no full stop, which
## the card quotes verbatim. Nothing here names a kit; the card must not either. It is the shape of
## `LoadoutRejection::OverCarry`'s message — the budget refusal it replaced no longer exists (#732).
## How far a measured edge may sit past the card's own before the claim calls it off the card — the
## sub-pixel slack a fractional canvas scale leaves on a rect's edge, never a layout allowance.
const CARD_EDGE_TOLERANCE := 0.5
const REFUSAL_DETAIL := "the order weighs 15.0 against a carry of 14.0"
## …and one too long for two lines at the card's width, which must ellipsize and keep the whole text on
## its tooltip. Ends in a full stop the card must not double.
const REFUSAL_LONG_DETAIL := ("the order names more than the band can carry: every allocation is "
	+ "checked against both budgets at once, and this one was over the kit budget and over the "
	+ "resource budget, so the whole order was turned down and nothing in it was applied to the band "
	+ "this turn, which keeps exactly what it held before the order arrived and nothing else besides.")
## The word only a refusal that RESET the picks may say.
const REFUSAL_RESET_NEEDLE := "reset"
## The label the sim gives every refused outfit order.
const REFUSAL_LABEL := "Outfit failed"

## The window it opens on, and the one the sim re-publishes a frame later. **The re-fit is what a split
## does to the PARENT** (`fission::rebalance_partitioned_grant`): the carry shrinks and the standing
## allocation is restated against it — an allocation this card never sent, so the card must take it.
## The held rows weigh 19 of 20 (one press to spare); the re-fit weighs 10 of 14, room for the three
## presses the next two blocks make.
const ADOPT_CARRY_CAPACITY := 20.0
const ADOPT_REFIT_CARRY_CAPACITY := 14.0
## What the band holds when the card opens.
const ADOPT_HELD_BIG_GAME := 3
const ADOPT_HELD_GATHERING := 3
const ADOPT_BONE := 4
const ADOPT_FIBRE := 6
## …and what the SIM re-fits it to. **Every row moves**, and `gathering` moves to a value the player's
## own presses below never reach, so "the published rows won" cannot be satisfied by a card that simply
## kept what it had.
const ADOPT_REFIT_BIG_GAME := 1
const ADOPT_REFIT_GATHERING := 2
const ADOPT_REFIT_BONE := 2
const ADOPT_REFIT_FIBRE := 4
## What the player does to it afterwards: two presses on `gathering`, one after the other, with the
## FIRST press's echo landing between the second press and its own. Neither may pull the card back.
const ADOPT_PRESSES := 2

## The word every retired forfeiture claim was built on — the old commit control's conditional face
## and the subtitle's old second clause alike. Asserted ABSENT from the whole card: the unspent
## warning is the turn orb's, and a second copy here is what this needle exists to catch coming back.
const FORFEIT_NEEDLE := "forfeit"
## …and the refusal notice that used to be posted on a still-open frame after a commit.
const REFUSAL_NEEDLE := "refused"
## ⛔ **THE FACE THE FOOTER CONTROL MAY NEVER WEAR AGAIN.** It closes the card and sends nothing, so a
## button reading `Set out` would promise the one thing this card no longer defers — and a player who
## never pressed it would think they had lost what they picked. A LITERAL, not
## `HudLoadoutVocab.CLOSE_LABEL`'s old value: a needle taken from the const under test moves with it.
const RETIRED_SEND_FACE := "Set out"

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
## must be named whatever the PICKER names it — the card's one meter says `carry` — and the orb read
## `2 units unspent` beside a `RESOURCES` column until a player asked what a unit was. An expectation taken from
## the const under test moves with it, so both sides of the comparison change together and the claim
## passes on the very rename it exists to catch; measured, sabotaging the const failed this claim not
## at all. These are literals for the same reason `_assert_horizon_floor_is_the_whole_trip`'s are.
const ORB_DETAIL_ONE_CARRY := "Brackwater — 1 carry unspent"
const ORB_DETAIL_EVERYTHING_PICKED := "Brackwater — everything is picked"
## …and the noun it must never go back to, asserted ABSENT so the rename cannot quietly revert.
const ORB_DETAIL_RETIRED_NOUN := "unit"

## **HOW MUCH TALLER THAN THE BODY THE SCROLL REGION MAY BE**, in pixels. The card is fitted to a
## measured minimum, so the honest tolerance is rounding, not a design allowance — even a row of slack
## is the dead-space defect, and the whole point of this bound is that it cannot be satisfied by a
## card that is merely "about right".
const CARD_DEAD_SPACE_TOLERANCE := 2.0

## **EVERY COMMAND THE CARD HAS EMITTED, in order** — the sink is connected for the whole chapter, so
## a press is visible to the state that made it AND to the fixtures, which republish the last order a
## band was sent as that band's holdings. See `_held_kits`.
var _orders: Array = []

func run(harness) -> void:
	h = harness
	# **THE SINK IS CONNECTED FOR THE WHOLE CHAPTER**, because every press sends now: a state that
	# connected only for its own block would leave the fixtures unable to say what a band holds. See
	# `_orders`.
	h._hud.set_starting_loadout_requested.connect(_on_order)
	# The two catalogues the picker JOINS onto, pushed through the seams `Main` really uses. Neither
	# is part of the loadout section: the kit roster rides the equipment config blob and a recipe's
	# input costs ride the recipe book, and the picker reads both rather than a second copy.
	h._hud.update_equipment_config(JSON.stringify(_equipment_config()))
	h._hud.update_crafting_catalogues(null, null, _recipes(), [])

	# **THE WINDOW OPENS ITSELF.** Nothing below asks it to — the first frame carrying a band whose
	# window is open is what stands the card up, which is the behaviour a player meets after watching
	# a world generate.
	#
	# **TWO SEAMS, IN THIS ORDER.** The campaign half (the pick list, the craftable ids) rides the
	# campaign section; the WINDOW itself — `open`, both budgets, the allocation the band HOLDS and a
	# take's caps — rides the COHORT, so it arrives through the same roster push `update_band_alerts`
	# already makes. The campaign half has to land first or the allocation has no pick list to be
	# filtered against, which is the order `Main` dispatches them in.
	h._hud.update_opening_loadout(_campaign())
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	_assert_opened_itself()
	_assert_the_kits_the_band_holds()
	_assert_the_pile_the_band_holds()
	_assert_gated_recipe_is_absent()
	_assert_recipe_counts()
	_assert_reachable_first()
	_assert_orb_row()
	_assert_the_footer_says_how_to_get_back()
	_assert_the_footer_control_does_not_read_as_a_send()
	await _assert_no_dead_space("opened")
	await h._save("starting_loadout")

	await _a_press_sends_the_whole_allocation()
	await _assert_no_dead_space("picked")
	await h._save("starting_loadout_picked")

	await _a_failed_send_rolls_the_press_back()

	await _spend_the_carry()
	await _assert_no_dead_space("spent")
	await h._save("starting_loadout_spent")

	await _dismiss_and_reopen()
	await _closing_the_card_sends_nothing()
	_assert_the_ready_ink_is_separable_in_every_palette()
	await _orb_states()
	# **THE TAKE ARC IS APPENDED, never interleaved.** Every state above renders with exactly one
	# window open, and the orb states assert the loadout row is the orb's ONLY entry — a second window
	# opened earlier would make those two frames evidence of somebody else's row.
	await _take_window_opens_on_the_splinter()
	await _a_press_on_a_take_sends_the_whole_standing_order()
	await _the_take_cap_is_the_expanded_item_list()
	await _the_switcher_reaches_the_other_band()
	await _every_row_names_its_band_and_opens_it()
	await _an_over_budget_band_is_not_outfitted()
	await _a_moved_allocation_is_adopted()
	await _two_presses_do_not_flicker_back()
	await _a_refused_order_resets_the_picks()
	await _the_card_prints_whole_numbers()
	await _a_splinters_food_is_not_unspent_room()
	_assert_window_shuts()
	h._hud.set_starting_loadout_requested.disconnect(_on_order)

# ---- the opening state ------------------------------------------------------

func _controller() -> StartingLoadoutController:
	return h._hud.starting_loadout_panel()

func _panel() -> StartingLoadoutPanel:
	return _controller().panel()

func _assert_opened_itself() -> void:
	h._assert_hud("loadout — the picker opens ITSELF on the first frame the window is open",
		_controller().is_expanded())

## **THE KIT COLUMN DRAWS THE KITS THE BAND HOLDS**, not zeros — the material column's rule, one
## column over. A roster row is found by its own meta rather than by its face: the face is a config
## string, so a text match would only confirm the fixture back to itself.
##
## ⛔ **THE COUNTS ARE ASSERTED AS PUBLISHED, which is what catches a second clamp.** The sim fitted
## the spread to the band's carry when it applied it, so a client that re-fitted it would render a
## different allocation from the one the band is carrying — and with a spread comfortably inside the
## carry (48 of 60) that re-fit would be INVISIBLE unless the individual counts are checked, since the
## total would still look reasonable.
##
## ⛔ **AND THE CAMPAIGN PRE-FILL IS NOT DRAWN HERE AT ALL.** `_campaign()` no longer states it, so a
## client that seeded its kits from the campaign section would render an EMPTY kit column on this frame
## rather than a doubled one — which these same counts catch, at `got 0`.
func _assert_the_kits_the_band_holds() -> void:
	var rows := _rows(HudLoadoutVocab.KIT_ROW_META)
	h._assert_hud("loadout — the `%s` kit is not offered (it grants nothing)" % KIT_NONE,
		not rows.is_empty() and not rows.has(KIT_NONE))
	for expectation in [[KIT_STALKING, DEFAULT_STALKING], ["trapping", DEFAULT_TRAPPING],
			[KIT_GATHERING, DEFAULT_GATHERING]]:
		var kit_id := String(expectation[0])
		var want := int(expectation[1])
		var got := _stepper_count(HudLoadoutVocab.KIT_ROW_META, kit_id)
		h._assert_hud("loadout — %s draws the published %d (got %d)" % [kit_id, want, got],
			got == want)
	# …and a kit the band does not hold really does read zero, without which "draws what it holds"
	# passes on a column that put the same number on every row.
	h._assert_hud("loadout — a kit the band does not hold reads 0 (got %d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, "warrior"),
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, "warrior") == 0)
	h._assert_hud("loadout — the carry meter weighs what is held (%.1f of %.1f, want %.1f)"
			% [_controller().carry_spent(), _controller().carry_capacity(), DEFAULT_LOAD],
		is_equal_approx(_controller().carry_spent(), DEFAULT_LOAD)
			and is_equal_approx(_controller().carry_capacity(), GRANT_WHOLE_CARRY))
	# ⛔ **A FIXED LARDER IS PART OF THE BAR, AND THE METER SAYS WHAT IS ACTUALLY FREE.** The window's
	# carry is the WHOLE carry; the meter is that less the goods AND the food, and the food states
	# itself on its legend line. A meter of `30 /` (goods only) passes the claim above and fails this.
	h._assert_hud("loadout — the meter reads what is free with the larder in it (`%s`)"
			% GRANT_METER_OPENING,
		Q.has_label_containing(_panel(), GRANT_METER_OPENING))
	h._assert_hud("loadout — the opening band's larder is the bar's food segment, `%s` (got `%s`)"
			% [GRANT_FOOD_LINE, Q.label_containing(_panel(), FOOD_LINE_NEEDLE)],
		_food_line_text() == GRANT_FOOD_LINE)
	# ⛔ **WHAT ONE MORE COSTS IS ON THE ROW WHERE IT IS NOT OBVIOUS** — a two-item kit says `2 carry`,
	# and a one-item kit says nothing. The pair is the claim: a row that always or never said it passes
	# one half.
	var stalking := _row_node(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	var gathering := _row_node(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	h._assert_hud("loadout — the two-item %s row says what one more weighs (`%s`)"
			% [KIT_STALKING, KIT_CARRY_COST_NEEDLE],
		Q.has_label_containing(stalking, KIT_CARRY_COST_NEEDLE))
	h._assert_hud("loadout — …and the one-item %s row says nothing about carry" % KIT_GATHERING,
		gathering != null and not Q.has_label_containing(gathering, CARRY_COST_WORD))

## …and the resources column draws the pile the band holds rather than zero. A picker that opened
## both columns at zero would look identical in a screenshot.
func _assert_the_pile_the_band_holds() -> void:
	var rows := _rows(HudLoadoutVocab.MATERIAL_ROW_META)
	h._assert_hud("loadout — one row per pickable material, in the profile's order (%s)"
			% str(rows),
		rows == PICKABLE)
	h._assert_hud("loadout — the pile draws what the band holds (%d units, want %d)"
			% [_controller().materials_spent(), DEFAULTS_TOTAL],
		_controller().materials_spent() == DEFAULTS_TOTAL)
	# **ONE METER, ON THE HEADER** — the kit meter and the material meter are gone, so a card still
	# drawing either fails on the count.
	h._assert_hud("loadout — the card carries exactly ONE meter, the carry (%s)"
			% str(_rows(HudLoadoutVocab.BUDGET_METER_META)),
		_rows(HudLoadoutVocab.BUDGET_METER_META) == [HudLoadoutVocab.BUDGET_CARRY])

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

## ⛔ **A PRESS IS AN ORDER, AND IT CARRIES THE WHOLE ALLOCATION.**
##
## Proven from a recorded session: a player filled a card in, never pressed `Set out`, ended the turn
## and the band got NOTHING — the server had received exactly one `set_starting_loadout` all game, for
## the other band, while the card and the orb both read *outfitted* off the card's own local copy. The
## draft is deleted, so the claim is that the press itself sends.
##
## **THREE THINGS, AND THEY FAIL APART**: the number under the finger moved; a command went out on the
## press with no further control touched; and that command names the WHOLE allocation, every row the
## band holds, since the verb is a replacement and a line naming only the pressed row would order the
## rest away.
##
## Pressed through the REAL button, re-found on every press — the panel rebuilds each time, so a
## cached node here is a freed control.
func _a_press_sends_the_whole_allocation() -> void:
	var before := _orders.size()
	for _i in range(KIT_PRESSES):
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
		await h._settle()
	var want_load := DEFAULT_LOAD + KIT_PRESSES * KIT_STALKING_ITEMS * ITEM_CARRY_WEIGHT
	h._assert_hud("loadout — %d presses weigh %d each, and the meter says so (%.1f of %.1f)"
			% [KIT_PRESSES, KIT_STALKING_ITEMS, _controller().carry_spent(), CARRY_CAPACITY],
		is_equal_approx(_controller().carry_spent(), want_load))
	h._assert_hud("loadout — …and each press sent ONE order, with no `%s` pressed (%d for %d presses)"
			% [RETIRED_SEND_FACE, _orders.size() - before, KIT_PRESSES],
		_orders.size() - before == KIT_PRESSES)
	var order: Dictionary = _orders.back()
	h._assert_hud("loadout — the order names the band by its durable id (got %d)"
			% int(order.get("band_id", HudConst.NO_BAND_ID)),
		int(order.get("band_id", HudConst.NO_BAND_ID)) == _band_id(HOME_BAND_ENTITY))
	# **THE WHOLE ALLOCATION, not the row that moved.** A replacement naming only `big_game` would order
	# the other two kits and the whole pile away, which is a line that looks perfectly correct.
	h._assert_hud("loadout — …and it carries every kit the band holds (%s)"
			% str(_order_rows(order, "kits", "count")),
		_order_rows(order, "kits", "count") == {
			KIT_STALKING: DEFAULT_STALKING + KIT_PRESSES,
			"trapping": DEFAULT_TRAPPING,
			KIT_GATHERING: DEFAULT_GATHERING,
		})
	h._assert_hud("loadout — …and every resource too (%s)"
			% str(_order_rows(order, "materials", "units")),
		_order_rows(order, "materials", "units") == {
			"bone": DEFAULT_BONE, "fibre": DEFAULT_FIBRE, "hide": DEFAULT_HIDE,
		})
	# …and it makes NO forfeiture claim, which is the orb's to make. Asked of the whole card, because
	# the sentence that used to say it was the SUBTITLE as well as the button.
	h._assert_hud("loadout — nothing on the card says anything is forfeited",
		not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))

## ⛔ **A SEND THAT DID NOT GO TAKES THE PRESS WITH IT.** The write is optimistic — the number has to
## move on the frame the stepper was pressed — and its warrant is the command beside it, so a line the
## transport refused would otherwise leave a card showing gear the sim has never heard of.
##
## **Driven the way `Main` drives it**: the `has_method` name is asserted first (that probe fails
## SILENTLY, so a rename takes the rollback out of the client without a word), then the very payload
## the HUD emitted is handed back — `band_panel_preview._assert_pending_assign_rollback`'s shape, and
## for its reason: a card showing one more kit than it ordered is a perfectly ordinary card.
func _a_failed_send_rolls_the_press_back() -> void:
	var before := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	var before_spent := _controller().carry_spent()
	h._assert_hud("loadout/rollback — the HUD carries the name `Main` probes for",
		h._hud.has_method("revert_starting_loadout"))
	_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	await h._settle()
	h._assert_hud("loadout/rollback — the press moved the row first (%d → %d)"
			% [before, _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == before + 1)
	# The line did not go. This is the branch `Main._on_hud_set_starting_loadout` takes on `false`.
	h._hud.revert_starting_loadout(_orders.back())
	# **AND NO SERVER SAW IT**, so it is taken off the record the fixtures publish from — a frame
	# carrying an order that never arrived would be a server inventing one.
	_orders.pop_back()
	await h._settle()
	h._assert_hud("loadout/rollback — …and the refused send takes the row back (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING), before],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == before)
	h._assert_hud("loadout/rollback — …and the meter with it (%.1f carried, want %.1f)"
			% [_controller().carry_spent(), before_spent],
		is_equal_approx(_controller().carry_spent(), before_spent))
	# **AND THE ROLLED-BACK ORDER IS NOT LEFT WAITING FOR AN ECHO.** The card un-sent it, so the next
	# frame — which restates the band as it stood BEFORE the press, that being the last order that
	# really went — must leave the rolled-back row exactly where the rollback put it.
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	h._assert_hud("loadout/rollback — …and the next frame leaves the rolled-back row alone (%d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING),
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == before)

## Fill the CARRY to the last unit with `big_game` presses, then press once more. The extra press is
## the point: the controller clamps against the carry, so every `+` is disabled and nothing can be
## overloaded — the sim refuses an over-carry order WHOLE (`OverCarry`), so a client that let one be
## composed would throw the picks away on send.
func _spend_the_carry() -> void:
	var presses := 0
	while presses < STEPPER_PRESS_LIMIT:
		var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
		if plus == null or plus.disabled:
			break
		plus.pressed.emit()
		presses += 1
		await h._settle()
	h._assert_hud("loadout — the carry is spent to the unit (%.1f left)" % _controller().carry_left(),
		is_zero_approx(_controller().carry_left()))
	var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	var material_plus := _plus_button(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
	h._assert_hud("loadout — every `+` is disabled once the carry is full, kit and resource alike",
		plus != null and plus.disabled and material_plus != null and material_plus.disabled)
	# **THE FOOTER CONTROL'S FACE IS UNCONDITIONAL**, taken here with the carry full against the
	# opening state's reading with room left. Neither half is worth anything alone: a face asserted
	# only where something is unspent passes on a control that renames itself once the pack fills, and
	# only where it is full on one that renames itself while it is not.
	h._assert_hud("loadout — the footer control still reads `%s` with the carry full (got `%s`)"
			% [HudLoadoutVocab.CLOSE_LABEL, _close_face()],
		_close_face() == HudLoadoutVocab.CLOSE_LABEL)

# ---- dismiss, reopen, refuse ------------------------------------------------

## **THE WINDOW IS DISMISSIBLE AND THE PICKS SURVIVE IT.** A player has to be able to pan, zoom and
## read a tile while outfitting, and the reopen pill is what brings the card back.
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

## ⛔ **THE FOOTER CONTROL CLOSES THE CARD AND SENDS NOTHING, AND NOTHING IS LOST BY PRESSING IT.**
##
## It was `Set out` and it was the only thing that sent; a player who filled the card in and never
## pressed it lost the lot. Every press orders now, so what is left for this button is the map: it
## puts the card away, and the reopen pill and the orb's row both stay live.
##
## **The pair is the claim** — pressing it must send NOTHING (a control that still ordered would make
## every press's send a double) and must cost nothing (a card that came back empty would be the old
## defect wearing new words).
##
## The frame after it is the one a client reading `open` as a success signal fails: the sim leaves the
## window open whatever it does with an order, so the card must come back CLEAN. An earlier cut posted
## *"that order was refused"* on exactly this frame.
func _closing_the_card_sends_nothing() -> void:
	var close_button := Q.find_meta_node(_panel(), HudLoadoutVocab.CLOSE_BUTTON_META) as Button
	h._assert_hud("loadout — the card carries a footer control", close_button != null)
	var before := _orders.size()
	var spent := _controller().kits_spent()
	close_button.pressed.emit()
	await h._settle()
	h._assert_hud("loadout — it puts the card away so the player can look at the map",
		not _controller().is_expanded() and _controller().is_open())
	h._assert_hud("loadout — …and it sends NOTHING (%d orders)" % (_orders.size() - before),
		_orders.size() == before)
	# The frame the sim really sends: the window is STILL OPEN.
	h._hud.update_band_alerts([_grant_band()])
	await h._settle()
	h._assert_hud("loadout — a still-open window is not treated as a refusal",
		not _controller().is_expanded())
	_panel().reopen_pill().pressed.emit()
	await h._settle()
	h._assert_hud("loadout — the card comes back CLEAN, every kit intact (%d), no refusal, no"
				% _controller().kits_spent() + " forfeiture claim",
		_controller().is_expanded() and _controller().kits_spent() == spent
			and not Q.has_label_containing(_panel(), REFUSAL_NEEDLE)
			and not Q.has_label_containing(_panel(), FORFEIT_NEEDLE))
	await _assert_no_dead_space("reopened")
	await h._save("starting_loadout_reopened")
	# **AND A PRESS STILL ORDERS AFTER A CLOSE AND A REOPEN** — an ordinary act under replacement
	# semantics, and one a client that latched "already sent" would refuse. It also leaves ONE resource
	# unspent, which is the warn arm the orb states below are taken on.
	var sends := _orders.size()
	_press_minus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
	await h._settle()
	h._assert_hud("loadout — a revised allocation sends again (%.1f carry left, %d order)"
			% [_controller().carry_left(), _orders.size() - sends],
		_orders.size() - sends == 1 and is_equal_approx(_controller().carry_left(), MATERIAL_CARRY_WEIGHT))

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

## ⛔ **OVER ITS CARRY IS NOT FULLY SPENT, AND THE ORB MUST NOT PAINT IT GREEN.**
##
## The completeness test was `remaining <= 0` over a remainder clamped at zero, so a band holding MORE
## than its window allows passed it: a live run showed a card reading `-6 / 22 left` beside a row
## calling that band outfitted. The sim bug behind the `-6` is fixed and **nothing here relies on
## that** — a state the row cannot word is exactly the state it must not paint as done.
##
## The state is staged the only way a client can reach it: the SIM publishes an allocation over the
## band's carry. The card draws a published allocation as-is (a second clamp here would disagree with
## the sim's own), so the meter goes negative exactly as it did on the screen that was reported.
func _an_over_budget_band_is_not_outfitted() -> void:
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band()])
	await h._settle()
	h._assert_hud("loadout/over — the over-budget band's card opens (subject %d)"
			% _controller().subject_band_id(),
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(OVER_BAND_ENTITY))
	# ⛔ **AN OVER-FULL BAND THAT IS NOT MOVING IS NOT WARNED.** The meter floors at zero in its calm
	# ink, no label on the card wears the warning ink, and the orb's row is the READY arm — there is no
	# over-carry row any more. Each half alone passes on a card that has merely changed its wording.
	h._assert_hud("loadout/over — the card's meter reads `%s`, never negative (got `%s`)"
			% [OVER_METER_NEEDLE, _meter_label_text()],
		_meter_label_text() == OVER_METER_NEEDLE)
	h._assert_hud("loadout/over — …in the calm ink, not the warning one",
		_meter_label_color() != HudStyle.WARN)
	h._assert_hud("loadout/over — NO label on the card wears the warning ink (%d did)"
			% _warn_labels_on_card(),
		_warn_labels_on_card() == 0)
	# …and the `+` is shut (there is no room) while the `−` stays open — the band can still put back.
	var plus := _plus_button(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	h._assert_hud("loadout/over — `+` is shut and `−` is open on the over-full row",
		plus != null and plus.disabled and _minus_enabled(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING))
	var row := _band_attention_row(_band_id(OVER_BAND_ENTITY))
	h._assert_hud("loadout/over — the orb row is `%s`, never a warning (got `%s` / `%s`)"
			% [HudLoadoutVocab.ATTENTION_LABEL_READY, row.get("label", ""), row.get("severity", "")],
		String(row.get("label", "")) == HudLoadoutVocab.ATTENTION_LABEL_READY
			and String(row.get("severity", "")) == HudAttentionVocab.ATTENTION_SEVERITY_READY)
	h._assert_hud("loadout/over — the detail is `%s`, and says nothing of carry over (`%s`)"
			% [OVER_DETAIL, row.get("detail", "")],
		String(row.get("detail", "")) == OVER_DETAIL
			and not String(row.get("detail", "")).contains(RETIRED_CARRY_OVER_NEEDLE))
	await _assert_no_dead_space("over")
	await h._save("starting_loadout_over_budget")

## One producer row by the band it is about, read off the CONTROLLER — the popover holds every
## producer's rows and this claim is about which one this band got.
func _band_attention_row(band_id: int) -> Dictionary:
	for row_variant in _controller().attention_rows():
		var row: Dictionary = row_variant
		if int(row.get(HudAttentionVocab.ATTENTION_PANEL_SUBJECT, HudConst.NO_BAND_ID)) == band_id:
			return row
	return {}

## ⛔ **AN ALLOCATION THIS CARD NEVER SENT IS THE SIM'S, AND THE CARD TAKES IT.**
##
## A split re-fits the PARENT's standing allocation down to its reduced budget
## (`fission::rebalance_partitioned_grant`) and re-materializes the band from it, so the published
## rows genuinely move under a card that is already up. The wire is the authority on what a band
## holds: a card that kept its own copy would draw the pre-split rows against the post-split carry,
## which is a negative meter reproduced client-side out of stale state.
##
## **EVERY ROW MOVES, and the one the player pressed moves to a value the presses never reach** — so
## "the published rows won" cannot be satisfied by a card that simply kept what it had. The meter is
## read off the ORB, whose `over` arm is the one reader of the unclamped remainder, because a stale
## allocation carried past a shrunken carry is exactly what would light it.
##
## **Its own band** (see `ADOPT_BAND_ENTITY`), and last, so no earlier state's edits are in its state
## and no orb claim above it sees a fourth row.
func _a_moved_allocation_is_adopted() -> void:
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band(),
		_adopt_band(false)])
	await h._settle()
	h._assert_hud("loadout/adopt — the new band's card stands itself up (subject %d)"
			% _controller().subject_band_id(),
		_controller().is_expanded()
			and _controller().subject_band_id() == _band_id(ADOPT_BAND_ENTITY))
	h._assert_hud("loadout/adopt — …on the kits the band HOLDS (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), ADOPT_HELD_GATHERING],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == ADOPT_HELD_GATHERING)
	# The player orders one more, so the card is standing on something IT sent rather than on the rows
	# it was handed — without which "the published rows won" is a claim about a card that never moved.
	_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	var ordered := ADOPT_HELD_GATHERING + HudConst.WORKER_STEP
	h._assert_hud("loadout/adopt — the player's own order stands at %d (got %d)"
			% [ordered, _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == ordered)
	# **THE FRAME THE SIM SENDS FOR ITS OWN REASONS**: the carry shrinks and every row is restated.
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band(),
		_adopt_band(true)])
	await h._settle()
	h._assert_hud("loadout/adopt — the re-fit replaces the row the player ordered (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), ADOPT_REFIT_GATHERING],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == ADOPT_REFIT_GATHERING)
	h._assert_hud("loadout/adopt — …and the rows beside it (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING), ADOPT_REFIT_BIG_GAME],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING) == ADOPT_REFIT_BIG_GAME)
	h._assert_hud("loadout/adopt — …and the resources column too (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, "fibre"), ADOPT_REFIT_FIBRE],
		_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, "fibre") == ADOPT_REFIT_FIBRE)
	# **THE PROPERTY ADOPTION EXISTS FOR.** The `over` arm is the one that reads the UNCLAMPED
	# remainder, so a card still holding the pre-refit allocation against the post-refit carry lights it.
	var row := _band_attention_row(_band_id(ADOPT_BAND_ENTITY))
	h._assert_hud("loadout/adopt — the re-fit is not a supply overdraw — the orb says `%s`, never `%s`"
			% [row.get("label", ""), HudLoadoutVocab.ATTENTION_LABEL_OVER_SUPPLY],
		String(row.get("label", "")) != HudLoadoutVocab.ATTENTION_LABEL_OVER_SUPPLY)
	# **AND THE NEXT ORDER AGREES WITH THE CARD.** The rendered steppers and the composed command are
	# two different claims — a card that adopted only on screen would send the rows it no longer shows.
	var before := _orders.size()
	_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	h._assert_hud("loadout/adopt — the order carries the adopted rows (%s)"
			% str(_order_rows(_orders.back(), "kits", "count")),
		_orders.size() - before == 1
			and _order_rows(_orders.back(), "kits", "count") == {
				KIT_GATHERING: ADOPT_REFIT_GATHERING + HudConst.WORKER_STEP,
				KIT_STALKING: ADOPT_REFIT_BIG_GAME,
			})
	await _assert_no_dead_space("adopt")
	await h._save("starting_loadout_adopted")

## ⛔ **TWO PRESSES IN A ROW, AND THE FIRST ONE'S ECHO MUST NOT PULL THE CARD BACK.**
##
## The sim recaptures after every dispatched command, so a pair of quick presses is answered by a pair
## of frames — and the FIRST arrives when the card is already showing the SECOND. A card comparing a
## published allocation against what it last SAW reads that frame as a change and steps backwards on
## it, visibly, on every pair of presses; comparing against what it has SENT is what makes the echo
## recognisable as its own.
##
## **Both frames are pushed, in the order a server sends them**, and the claim after each is that the
## card still reads the SECOND press. The second frame is not decoration: without it the block passes
## on a card that ignores published allocations altogether, which the state above would then be the
## only thing to catch.
func _two_presses_do_not_flicker_back() -> void:
	var standing := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	var want := standing + ADOPT_PRESSES * HudConst.WORKER_STEP
	for _i in range(ADOPT_PRESSES):
		_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
		await h._settle()
	h._assert_hud("loadout/flicker — two presses stand at %d (got %d)"
			% [want, _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want)
	# The FIRST press's own frame, arriving after the second press was made.
	h._hud.update_band_alerts([_adopt_band_holding(_orders[_orders.size() - 2])])
	await h._settle()
	h._assert_hud("loadout/flicker — the first press's echo does not pull the card back (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), want],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want)
	# …and the second press's own frame, which settles it.
	h._hud.update_band_alerts([_adopt_band_holding(_orders.back())])
	await h._settle()
	h._assert_hud("loadout/flicker — …and its own echo leaves it exactly there (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), want],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want)

## ⛔ **THE SIM REFUSED THE ORDER, AND THE ONLY WORD OF IT IS ONE EVENT ROW.** A refused order moves
## no band row (populations ship as diffs), so no frame republishes the band — the card has to put its
## optimistic pick back off the `starting_loadout` row alone, and say why.
##
## Also asserted: a row from an EARLIER turn is moot and ignored; the same row re-sent (a full
## snapshot's ring) is not applied twice; the next press clears the line; and a reason too long for two
## lines is ellipsized there and carried whole on the tooltip.
func _a_refused_order_resets_the_picks() -> void:
	var band_id := _band_id(ADOPT_BAND_ENTITY)
	var turn: int = h._hud._band_labor.current_turn()
	var holds := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	var holds_spent := _controller().carry_spent()
	# A `−`, because the flicker pair above leaves this band's carry with too little room for a `+` to
	# be sure of landing. Either direction is an order the sim can refuse.
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	h._assert_hud("loadout/refused — the press moved the row first (%d → %d)"
			% [holds, _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == holds - HudConst.WORKER_STEP)
	# **THE SERVER REFUSED IT**, so the band does not hold it and no fixture may publish it.
	_orders.pop_back()
	var seq: int = _controller()._event_seq_cursor + 1
	# An EARLIER turn's refusal: that window is shut, so the row says nothing about this one.
	_next_snapshot()
	h._hud.ingest_command_events([_refusal_event(seq, turn - 1, band_id, REFUSAL_DETAIL)])
	await h._settle()
	h._assert_hud("loadout/refused — a refusal from an earlier turn moves nothing (%d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING),
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == holds - HudConst.WORKER_STEP
			and _refusal_line() == null)
	var refusal := _refusal_event(seq + 1, turn, band_id, REFUSAL_DETAIL)
	_next_snapshot()
	h._hud.ingest_command_events([refusal])
	await h._settle()
	h._assert_hud("loadout/refused — the refusal puts the row back on what the band holds (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), holds],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == holds)
	h._assert_hud("loadout/refused — …and the carry meter with it (%.1f spent, want %.1f)"
			% [_controller().carry_spent(), holds_spent],
		is_equal_approx(_controller().carry_spent(), holds_spent))
	var line := _refusal_line()
	var want_text := HudLoadoutVocab.REFUSAL_FORMAT % REFUSAL_DETAIL
	h._assert_hud("loadout/refused — the card says why, in the sim's words (%s)"
			% (line.text if line != null else "<no line>"),
		line != null and line.visible and line.text == want_text)
	h._assert_hud("loadout/refused — …in warning ink",
		line != null and line.get_theme_color("font_color").is_equal_approx(HudStyle.WARN))
	await _assert_no_dead_space("refused")
	await h._save("starting_loadout_refused")
	# The next press supersedes the refused order — and its line.
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	h._assert_hud("loadout/refused — the next press clears the line",
		_refusal_line() == null)
	# A full snapshot re-sends the ring: the same row again must not reset the new press.
	_next_snapshot()
	h._hud.ingest_command_events([refusal])
	await h._settle()
	h._assert_hud("loadout/refused — the same row re-sent is not applied twice (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING),
				holds - HudConst.WORKER_STEP],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == holds - HudConst.WORKER_STEP
			and _refusal_line() == null)
	# That press is refused too, with a reason too long for two lines.
	_orders.pop_back()
	_next_snapshot()
	h._hud.ingest_command_events([_refusal_event(seq + 2, turn, band_id, REFUSAL_LONG_DETAIL)])
	await h._settle()
	line = _refusal_line()
	var full := HudLoadoutVocab.REFUSAL_FORMAT % REFUSAL_LONG_DETAIL.trim_suffix(
		HudLoadoutVocab.REFUSAL_DETAIL_TRAILING_STOP)
	h._assert_hud("loadout/refused — a long reason takes two lines and no more (%d of %d)"
			% [line.get_visible_line_count() if line != null else -1,
				line.get_line_count() if line != null else -1],
		line != null and line.get_line_count() > HudLoadoutVocab.REFUSAL_MAX_LINES
			and line.get_visible_line_count() == HudLoadoutVocab.REFUSAL_MAX_LINES)
	h._assert_hud("loadout/refused — …and the tooltip carries all of it, the stop not doubled",
		line != null and line.tooltip_text == full and not full.contains(".."))
	await _assert_no_dead_space("refused_long")
	await h._save("starting_loadout_refused_long")
	await _a_refusal_leaves_a_later_order_in_flight(band_id, turn, seq + 3)
	await _a_refusal_older_than_the_published_state_says_nothing(band_id, turn, seq + 4)

## ⛔ **A IS REFUSED WHILE B IS STILL IN FLIGHT: ONLY A IS FORGOTTEN.** The sim runs a band's orders in
## sequence, so A's refusal is about the OLDEST unechoed order. B is a whole replacement still on its
## way, so the card keeps B's pick on screen, says A was not taken, and lets B's own echo settle it.
## Resetting every in-flight order dragged the card back past B and left the line over B's accepted
## picks.
func _a_refusal_leaves_a_later_order_in_flight(band_id: int, turn: int, seq: int) -> void:
	_next_snapshot()
	var holds := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	var order_b: Dictionary = _orders.back()
	# A is the order BEFORE B, and no server holds it.
	_orders.remove_at(_orders.size() - 2)
	var want_b := holds - 2 * HudConst.WORKER_STEP
	_next_snapshot()
	h._hud.ingest_command_events([_refusal_event(seq, turn, band_id, REFUSAL_DETAIL)])
	await h._settle()
	var line := _refusal_line()
	h._assert_hud("loadout/refused-in-flight — A's refusal shows its line (%s)"
			% (line.text if line != null else "<no line>"),
		line != null and line.text == HudLoadoutVocab.REFUSAL_IN_FLIGHT_FORMAT % REFUSAL_DETAIL)
	# Nothing was reset — B's pick is still standing — so the line must not say it was.
	h._assert_hud("loadout/refused-in-flight — …and does not claim a reset (%s)"
			% (line.text if line != null else "<no line>"),
		line != null and not line.text.to_lower().contains(REFUSAL_RESET_NEEDLE))
	h._assert_hud("loadout/refused-in-flight — …and B's pick stays on screen (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), want_b],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want_b)
	var unechoed := _unechoed(band_id)
	h._assert_hud("loadout/refused-in-flight — …and only B is still awaiting its echo (%d in flight)"
			% unechoed.size(),
		unechoed.size() == 1 and (unechoed[0] as Dictionary).get(HudLoadoutVocab.WINDOW_KITS_KEY, {})
			== _order_rows(order_b, "kits", "count"))
	await _assert_no_dead_space("refused_in_flight")
	await h._save("starting_loadout_refused_in_flight")
	# B's own frame: the sim accepted it, so the line over it goes.
	_next_snapshot()
	h._hud.update_band_alerts([_adopt_band_holding(order_b)])
	await h._settle()
	h._assert_hud("loadout/refused-in-flight — B's echo clears the line and keeps B (%d)"
			% _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING),
		_refusal_line() == null
			and _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want_b
			and _unechoed(band_id).is_empty())

## ⛔ **ONE SNAPSHOT CARRIES B'S ECHO AND A'S REFUSAL, AND THE REFUSAL IS THE OLDER FACT.** `Main` hands
## the card populations BEFORE command_events, so B's echo is already on screen when A's refusal is
## read — and the capture carrying both was taken after the sim ran both orders. A "Not taken" line
## there would sit over picks the sim accepted afterwards.
func _a_refusal_older_than_the_published_state_says_nothing(band_id: int, turn: int,
		seq: int) -> void:
	_next_snapshot()
	var holds := _stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING)
	await h._settle()
	var order_b: Dictionary = _orders.back()
	_orders.remove_at(_orders.size() - 2)
	var want_b := holds - 2 * HudConst.WORKER_STEP
	# ONE snapshot, in `Main`'s order: the band first, then the event rows.
	_next_snapshot()
	h._hud.update_band_alerts([_adopt_band_holding(order_b)])
	h._hud.ingest_command_events([_refusal_event(seq, turn, band_id, REFUSAL_DETAIL)])
	await h._settle()
	h._assert_hud("loadout/refused-same-frame — no line over B's accepted picks",
		_refusal_line() == null)
	h._assert_hud("loadout/refused-same-frame — …which stay on screen, nothing in flight (%d, want %d)"
			% [_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING), want_b],
		_stepper_count(HudLoadoutVocab.KIT_ROW_META, KIT_GATHERING) == want_b
			and _unechoed(band_id).is_empty())

## The `command_events` row the sim pushes for a refused `set_starting_loadout`.
func _refusal_event(seq: int, tick: int, band_id: int, detail: String) -> Dictionary:
	return {
		"seq": seq,
		"tick": tick,
		"kind": HudLoadoutVocab.REFUSAL_EVENT_KIND,
		"faction": HudConst.PLAYER_FACTION_ID,
		"label": REFUSAL_LABEL,
		"detail": detail,
		"band": band_id,
	}

## The orders the card has sent for one band and not yet seen echoed.
func _unechoed(band_id: int) -> Array:
	return (_controller()._bands.get(band_id, {}) as Dictionary).get(
		StartingLoadoutController.BAND_UNECHOED, [])

## Start a snapshot the way `Main` does — `update_overlay` comes first in every one, and it is the
## boundary the card uses to tell a refusal OLDER than a state it has just been handed.
func _next_snapshot() -> void:
	h._hud.update_overlay(h._hud._band_labor.current_turn(), {})

func _refusal_line() -> Label:
	return Q.find_meta_node(_panel(), HudLoadoutVocab.REFUSAL_LINE_META) as Label

## ⛔ **THE CARD PRINTS WHOLE NUMBERS, AND ROUNDS THE WAY THAT KEEPS IT HONEST.** Reported from play:
## `Food 77.7`, `0.2 / 124.9 carry left`. The adopt band is re-published with the reported carry and a
## FIXED larder that leaves exactly 0.2 free, so the meter must read `0 / 125` (the room FLOORS, the
## total rounds) and the larder `Food 115`. Then one more unit of larder puts the band 0.8 over, and
## the meter reads `0 /` again, in the calm ink — a band that is not moving is never told it is over
## its carry. Only the printing moves: the goods precondition is read off the controller's floats.
const WHOLE_CARRY := 124.9
const WHOLE_FREE := 0.2
const WHOLE_OVER_BY := 1.0
const WHOLE_METER := "0 / 125 carry left"
const WHOLE_METER_OVER := "0 / 125 carry left"
const WHOLE_FOOD_LINE := "Food 115"
## The adopt band's re-fitted allocation, weighed as the server weighs it.
const ADOPT_REFIT_LOAD := (ADOPT_REFIT_BIG_GAME * KIT_STALKING_ITEMS
		+ ADOPT_REFIT_GATHERING * KIT_GATHERING_ITEMS) * ITEM_CARRY_WEIGHT \
		+ (ADOPT_REFIT_BONE + ADOPT_REFIT_FIBRE) * MATERIAL_CARRY_WEIGHT

func _the_card_prints_whole_numbers() -> void:
	var larder := WHOLE_CARRY - ADOPT_REFIT_LOAD - WHOLE_FREE
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band(),
		_fractional_adopt_band(larder)])
	await h._settle()
	h._assert_hud("loadout/whole — the card is on the re-fitted allocation (%.1f, want %.1f)"
			% [_controller().carry_spent(), ADOPT_REFIT_LOAD],
		_controller().subject_band_id() == _band_id(ADOPT_BAND_ENTITY)
			and is_equal_approx(_controller().carry_spent(), ADOPT_REFIT_LOAD))
	h._assert_hud("loadout/whole — the meter reads `%s` (got `%s`)" % [WHOLE_METER, _meter_label_text()],
		_meter_label_text() == WHOLE_METER)
	h._assert_hud("loadout/whole — the larder reads `%s` (got `%s`)" % [WHOLE_FOOD_LINE, _food_line_text()],
		_food_line_text() == WHOLE_FOOD_LINE)
	await h._save("starting_loadout_whole_numbers")
	h._hud.update_band_alerts([_grant_band(), _splinter_band(), _over_budget_band(),
		_fractional_adopt_band(larder + WHOLE_OVER_BY)])
	await h._settle()
	h._assert_hud("loadout/whole — 0.8 over reads `%s` in the calm ink, never negative (got `%s`)"
			% [WHOLE_METER_OVER, _meter_label_text()],
		_meter_label_text() == WHOLE_METER_OVER and _meter_label_color() != HudStyle.WARN)

## The adopt band at its re-fitted allocation, with a fractional WHOLE carry and a FIXED larder.
func _fractional_adopt_band(larder: float) -> Dictionary:
	var band := _adopt_band(true)
	var window: Dictionary = band[HudLoadoutVocab.WINDOW_KEY]
	window[HudLoadoutVocab.CARRY_CAPACITY_KEY] = WHOLE_CARRY
	window[HudLoadoutVocab.FOOD_SHARE_KEY] = larder
	window[HudLoadoutVocab.FOOD_CARRIED_KEY] = larder
	window[HudLoadoutVocab.FOOD_FIXED_KEY] = true
	return band

## ⛔ **A SPLINTER'S FOOD IS NOT UNSPENT ROOM** — reported from play: two splinters reading
## `0 / 35 carry left` and `Food 22 of 22` while the orb said *"22 carry unspent"*. A GRANT window on a
## band whose food is NOT fixed (a splinter of a band whose grant was still open), staged twice on one
## band: full (13 goods + 22 food = 35) must read OUTFITTED with no "carry unspent", and genuinely
## roomy (5 goods + 22 food, 8 free) must still read *"8 carry unspent"* beside a meter reading 8 —
## the pair, since either half alone passes on an arm that always or never fires.
const FOOD_SPLINTER_ENTITY := 6205
const FOOD_SPLINTER_CARRY := 35.0
const FOOD_SPLINTER_SHARE := 22.0
const FOOD_SPLINTER_FULL_GOODS := 13
const FOOD_SPLINTER_ROOMY_GOODS := 5
const FOOD_SPLINTER_FULL_METER := "0 / 35 carry left"
const FOOD_SPLINTER_ROOMY_METER := "8 / 35 carry left"
const FOOD_SPLINTER_ROOMY_FACT := "8 carry unspent"
const CARRY_UNSPENT_NEEDLE := "carry unspent"

func _a_splinters_food_is_not_unspent_room() -> void:
	h._hud.update_band_alerts([_grant_band(), _food_splinter_band(FOOD_SPLINTER_FULL_GOODS)])
	await h._settle()
	_controller().open_band(_band_id(FOOD_SPLINTER_ENTITY))
	await h._settle()
	h._assert_hud("loadout/splinter food — the full card reads `%s` (got `%s`)"
			% [FOOD_SPLINTER_FULL_METER, _meter_label_text()],
		_meter_label_text() == FOOD_SPLINTER_FULL_METER)
	var row := _band_attention_row(_band_id(FOOD_SPLINTER_ENTITY))
	h._assert_hud("loadout/splinter food — …and the orb calls it `%s`, never unspent (`%s` / `%s`)"
			% [HudLoadoutVocab.ATTENTION_LABEL_READY, row.get("label", ""), row.get("detail", "")],
		String(row.get("label", "")) == HudLoadoutVocab.ATTENTION_LABEL_READY
			and not String(row.get("detail", "")).contains(CARRY_UNSPENT_NEEDLE))
	await h._save("starting_loadout_splinter_food_full")
	h._hud.update_band_alerts([_grant_band(), _food_splinter_band(FOOD_SPLINTER_ROOMY_GOODS)])
	await h._settle()
	h._assert_hud("loadout/splinter food — the roomy card reads `%s` (got `%s`)"
			% [FOOD_SPLINTER_ROOMY_METER, _meter_label_text()],
		_meter_label_text() == FOOD_SPLINTER_ROOMY_METER)
	row = _band_attention_row(_band_id(FOOD_SPLINTER_ENTITY))
	h._assert_hud("loadout/splinter food — …and the orb says `%s`, the meter's own figure (`%s` / `%s`)"
			% [FOOD_SPLINTER_ROOMY_FACT, row.get("label", ""), row.get("detail", "")],
		String(row.get("label", "")) == HudLoadoutVocab.ATTENTION_LABEL_UNSPENT
			and String(row.get("detail", "")).ends_with(FOOD_SPLINTER_ROOMY_FACT))

## A GRANT window on a splinter: its food yields to its goods (`food_fixed` false), the window's food
## carried being the room its goods leave, capped by the share — what the sim answers.
func _food_splinter_band(goods: int) -> Dictionary:
	return _band(FOOD_SPLINTER_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.CARRY_CAPACITY_KEY: FOOD_SPLINTER_CARRY,
		HudLoadoutVocab.FOOD_SHARE_KEY: FOOD_SPLINTER_SHARE,
		HudLoadoutVocab.FOOD_CARRIED_KEY: minf(FOOD_SPLINTER_SHARE,
			FOOD_SPLINTER_CARRY - goods * MATERIAL_CARRY_WEIGHT),
		HudLoadoutVocab.FOOD_FIXED_KEY: false,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		HudLoadoutVocab.WINDOW_KITS_KEY: [],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: PICKABLE[0],
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: goods},
		],
	})

## The band whose allocation the SIM moves. `refit` is the second frame: the carry shrunk and every
## row restated against it, which is what a split does to the parent.
func _adopt_band(refit: bool) -> Dictionary:
	return _band(ADOPT_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.CARRY_CAPACITY_KEY: ADOPT_REFIT_CARRY_CAPACITY if refit \
			else ADOPT_CARRY_CAPACITY,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		HudLoadoutVocab.WINDOW_KITS_KEY: [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY:
					ADOPT_REFIT_BIG_GAME if refit else ADOPT_HELD_BIG_GAME},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_GATHERING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY:
					ADOPT_REFIT_GATHERING if refit else ADOPT_HELD_GATHERING},
		],
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "bone",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY:
					ADOPT_REFIT_BONE if refit else ADOPT_BONE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "fibre",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY:
					ADOPT_REFIT_FIBRE if refit else ADOPT_FIBRE},
		],
	})

## …and the same band publishing ONE PARTICULAR ORDER back — the frame a server sends after applying
## it. Taken from the emitted payload rather than composed, so the echo is byte-for-byte what the card
## asked for; anything composed here could differ from the order by a rounding of the fixture's own.
func _adopt_band_holding(order: Dictionary) -> Dictionary:
	var band := _adopt_band(true)
	var window: Dictionary = band[HudLoadoutVocab.WINDOW_KEY]
	window[HudLoadoutVocab.WINDOW_KITS_KEY] = _window_rows(order.get("kits", []), "count",
		HudLoadoutVocab.KIT_DEFAULT_ID_KEY, HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY)
	window[HudLoadoutVocab.WINDOW_MATERIALS_KEY] = _window_rows(order.get("materials", []), "units",
		HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY, HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY)
	return band

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
	# ⛔ **A FIXED LARDER STATES ITS MASS AND NO CHOICE** — taken before the splinter's window stands
	# its own card up, while the grant is still the subject. The opening band cannot trade its food for
	# tools, so its line carries no "fewer tools" clause; the splinter's below does. Paired, or "the
	# clause appears" passes on a card that prints it on every window.
	h._assert_hud("loadout/take — the GRANT's food line is its fixed larder alone (`%s`)"
			% _food_line_text(),
		_food_line_text() == GRANT_FOOD_LINE and not _controller().is_take())
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
	# **ONE CARRY METER, THE SPLINTER'S OWN.** Its denominator is the take's carry, never the grant's.
	h._assert_hud("loadout/take — the meter reads the splinter's carry (`%s`), not the grant's"
			% TAKE_METER_OPENING,
		Q.has_label_containing(_panel(), TAKE_METER_OPENING)
			and not Q.has_label_containing(_panel(), GRANT_METER_NEEDLE))
	# **THE PICK LIST DOES NOT BIND A TAKE.** The rows are what the home band holds — `clay` included,
	# which the profile never offered — so a card drawing `PICKABLE` here fails on the row list alone.
	var rows := _rows(HudLoadoutVocab.MATERIAL_ROW_META)
	h._assert_hud("loadout/take — the resources column lists what the HOME BAND holds (%s)" % str(rows),
		rows == TAKE_MATERIALS and rows != PICKABLE)
	# **THE METER WEIGHS WHAT THE SPLIT ALREADY MOVED** — the two standing `big_game` kits expand to
	# four items, plus the two hide.
	h._assert_hud("loadout/take — the meter weighs the standing take (%.1f of %.1f, want %.1f)"
			% [_controller().carry_spent(), _controller().carry_capacity(), TAKE_DEFAULT_LOAD],
		is_equal_approx(_controller().carry_spent(), TAKE_DEFAULT_LOAD)
			and _controller().items_spent() == TAKE_DEFAULT_BIG_GAME * KIT_STALKING_ITEMS
			and _controller().materials_spent() == TAKE_DEFAULT_HIDE)
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
	# ⛔ **THE FOOD A SPLIT BRINGS IS THE ROOM THE GOODS LEAVE** — 18 − 6 = 12 of a 14 share, and the
	# choice that would bring more, on ONE line.
	h._assert_hud("loadout/take — the food line reads `%s`, with the choice beside it (`%s`)"
			% [TAKE_FOOD_OPENING + HudLoadoutVocab.FOOD_ROOM_CLAUSE, _food_line_text()],
		_food_line_text() == TAKE_FOOD_OPENING + HudLoadoutVocab.FOOD_ROOM_CLAUSE)
	# ⛔ **AND IT IS A CHOICE, NOT A FAULT: QUIET INK, NEVER AMBER.** It was a separate amber line; the
	# maintainer read a warning into a trade-off.
	h._assert_hud("loadout/take — …in the quiet ink, never the warning one",
		_food_line_ink() == HudStyle.INK_DIM and _food_line_ink() != HudStyle.WARN)
	await _assert_no_dead_space("take")
	await h._save("starting_loadout_take")

## ⛔ **ONE PRESS ON A TAKE ORDERS THE WHOLE STANDING TAKE, PLUS THE PRESS.** The splinter is already
## holding its dowry — the split moved it — so the row the player does not touch is exactly the row a
## replacement must still name: an order carrying only the pressed kit would hand the rest of the
## dowry back to the parent, which is the shape this fixture exists for and a line that reads as
## perfectly ordinary.
##
## **The claim is the composed ORDER, not the rendered steppers.** The card showing the right numbers
## and the command carrying them are two different things, and only the payload can say the second.
##
## **Nothing special-cases an empty tail**, here or on a grant: an empty order means *take nothing*.
func _a_press_on_a_take_sends_the_whole_standing_order() -> void:
	var before := _orders.size()
	_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	await h._settle()
	var order: Dictionary = _orders.back()
	h._assert_hud("loadout/take — the press sent ONE order, naming the SPLINTER by its durable id (%d)"
			% int(order.get("band_id", HudConst.NO_BAND_ID)),
		_orders.size() - before == 1
			and int(order.get("band_id", HudConst.NO_BAND_ID)) == _band_id(SPLINTER_BAND_ENTITY))
	h._assert_hud("loadout/take — …and it carries the standing kits the press never touched (%s)"
			% str(_order_rows(order, "kits", "count")),
		_order_rows(order, "kits", "count") == {KIT_STALKING: TAKE_DEFAULT_BIG_GAME})
	h._assert_hud("loadout/take — …and the standing hide beside the fibre just taken (%s)"
			% str(_order_rows(order, "materials", "units")),
		_order_rows(order, "materials", "units") == {
			TAKE_MATERIALS[0]: TAKE_DEFAULT_HIDE, TAKE_MATERIALS[1]: HudConst.WORKER_STEP,
		})
	# ⛔ **THE PRESS MOVES THE FOOD LINE BEFORE THE SERVER ANSWERS** — nothing has been re-pushed, so
	# the card is previewing `min(share, carry − goods)`, 11 of 14. A card that waited for the wire would
	# still read the opening 12 here.
	h._assert_hud("loadout/take — a goods press previews the food it costs (`%s`, got `%s`, %.2f)"
			% [TAKE_FOOD_PREVIEW, Q.label_containing(_panel(), FOOD_LINE_NEEDLE), _controller().food_brought()],
		Q.has_label_containing(_panel(), TAKE_FOOD_PREVIEW)
			and not Q.has_label_containing(_panel(), TAKE_FOOD_OPENING))
	# ⛔ **AND A `+` STAYS LIVE WHILE GOODS FIT, EVEN THOUGH IT COSTS FOOD.** Food is not a term of the
	# carry check; only the goods are. Fibre's next unit fits (8 of 18), so its `+` must be enabled.
	var fibre_plus := _plus_button(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	h._assert_hud("loadout/take — a `+` that would cost food stays enabled while the goods fit",
		fibre_plus != null and not fibre_plus.disabled)
	# ⛔ **ONCE ECHOED, THE WIRE'S FIGURE IS THE TRUTH** — the fixture republishes the order (so the card
	# has nothing out) with the food staged off the preview; the line must read the wire's.
	h._hud.update_band_alerts([_grant_band(), _splinter_band(TAKE_FOOD_ECHOED)])
	await h._settle()
	h._assert_hud("loadout/take — once the order is echoed the line reads the WIRE's food (`%s`)"
			% TAKE_FOOD_ECHOED_LINE,
		Q.has_label_containing(_panel(), TAKE_FOOD_ECHOED_LINE))
	# Put the fibre back, so the cap walk below starts from the standing take the fixture describes.
	_press_minus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	await h._settle()
	h._assert_hud("loadout/take — and a `−` orders too, back to the standing take (%s)"
			% str(_order_rows(_orders.back(), "materials", "units")),
		_order_rows(_orders.back(), "materials", "units")
			== {TAKE_MATERIALS[0]: TAKE_DEFAULT_HIDE})
	await _a_refused_take_rolls_the_food_back()

## ⛔ **A REFUSED ORDER ROLLS THE FOOD BACK WITH THE PICKS.** The `−` above is out and unanswered, so
## the line is previewing the room it leaves (12); the wire's last word was the staged echo (10, for
## the allocation still holding one fibre). Refusing the `−` must put the picks back on that held
## allocation AND the food back on the wire's 10 — a card that kept previewing reads 12 for an order
## that never happened. Then a second `−` restores the standing take the cap walk below starts from.
func _a_refused_take_rolls_the_food_back() -> void:
	h._assert_hud("loadout/take — the unanswered `−` previews its food (`%s`)" % TAKE_FOOD_OPENING,
		Q.has_label_containing(_panel(), TAKE_FOOD_OPENING))
	_orders.pop_back()
	var seq: int = _controller()._event_seq_cursor + 1
	_next_snapshot()
	h._hud.ingest_command_events([_refusal_event(seq, h._hud._band_labor.current_turn(),
		_band_id(SPLINTER_BAND_ENTITY), REFUSAL_DETAIL)])
	await h._settle()
	h._assert_hud("loadout/take — a refusal puts the fibre back on what the band holds (%d)"
			% _stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1]),
		_stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1]) == HudConst.WORKER_STEP
			and _refusal_line() != null)
	h._assert_hud("loadout/take — …and the food back on the WIRE's figure (`%s`, got `%s`)"
			% [TAKE_FOOD_ECHOED_LINE, Q.label_containing(_panel(), FOOD_LINE_NEEDLE)],
		Q.has_label_containing(_panel(), TAKE_FOOD_ECHOED_LINE)
			and not Q.has_label_containing(_panel(), TAKE_FOOD_OPENING))
	await _assert_no_dead_space("take_refused")
	await h._save("starting_loadout_take_refused")
	# ⛔ **THREE PRESSES, EACH LANDING ON THE FRAME THE PREVIOUS ONE'S FIT FINISHES** — the `−` that
	# clears the refusal line (the header loses a row), then a `+`/`−` pair on a kit row, one `_settle`
	# apart. `_settle` is two frames and so is `refit`, so each render lands right after the last fit,
	# rebuilds every autowrapping label, and the card's column is sorted against their unsized, inflated
	# minimums at an unchanged card height. That is the reported frame: a 1596px scroll in an 839px
	# card, the footer pushed off the bottom (`StartingLoadoutPanel._fit_expanded_height`). Pressing
	# with NO settle between does not reproduce it — the three renders share one frame and one sort.
	# The pair nets out, so the cap walk below still starts from the standing take.
	_press_minus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	await h._settle()
	_press_plus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	await h._settle()
	_press_minus(HudLoadoutVocab.KIT_ROW_META, KIT_STALKING)
	# The last render's fit spans two frames (`refit`), so the claim waits for it to land.
	await h._settle()
	await h._settle()
	_assert_card_holds_its_column("take_back_to_back")
	await _assert_no_dead_space("take_refusal_cleared")

## ⛔ **THE CARD'S COLUMN FITS INSIDE THE CARD, AND THE FOOTER IS ON IT.** The scroll region may not be
## taller than the card that holds it, and the footer (the `Done` row) must end inside the card's own
## rect. Both are geometry a dead-space bound reads only indirectly; a card whose column was laid out
## against stale minimums fails both at once, which is what the reported frame showed.
func _assert_card_holds_its_column(arm: String) -> void:
	var card := _panel().card()
	var scroll: Control = card.find_child("LoadoutScroll", true, false)
	var footer: Control = card.find_child("LoadoutFooter", true, false)
	if scroll == null or footer == null:
		h._assert_hud("loadout/%s — the card still has a scroll and a footer to measure" % arm, false)
		return
	h._assert_hud("loadout/%s — the scroll region fits inside the card (%.0f of %.0f)"
			% [arm, scroll.size.y, card.size.y],
		scroll.size.y <= card.size.y)
	var card_rect := card.get_global_rect()
	var footer_rect := footer.get_global_rect()
	h._assert_hud("loadout/%s — the footer is on the card (ends at %.0f, card ends at %.0f)"
			% [arm, footer_rect.end.y, card_rect.end.y],
		footer.is_visible_in_tree() and footer_rect.end.y <= card_rect.end.y + CARD_EDGE_TOLERANCE)

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
	h._assert_hud("loadout/take — the order expands to ITEM UNITS, the currency the cap is in (%d)"
			% _controller().items_spent(),
		_controller().items_spent() == TAKE_BIG_GAME_CEILING * KIT_STALKING_ITEMS)
	# …and the SUPPLY stopped it, not the carry: the pack still has room. Without this the sled claim
	# above could be a carry bind that happened to land on five.
	h._assert_hud("loadout/take — the carry still has room, so the SUPPLY is what stopped it (%.1f left)"
			% _controller().carry_left(),
		_controller().carry_left() > 0.0)
	await _assert_no_dead_space("take_capped")
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
	# ⛔ **AND THE CARRY BINDS A TAKE ROW TOO.** Fibre has 9 at home, but the pack has room for 8 — so
	# the row's own supply cap is not enough for its `+` to be live: `can_add` is BOTH caps.
	for _i in range(TAKE_FIBRE + 1):
		_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	await h._settle()
	var fibre := _stepper_count(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	var fibre_plus := _plus_button(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	h._assert_hud("loadout/take — fibre stops at %d, bound by the CARRY with %d still at home (got %d)"
			% [TAKE_FIBRE_CARRY_CEILING, TAKE_FIBRE, fibre],
		fibre == TAKE_FIBRE_CARRY_CEILING and fibre_plus != null and fibre_plus.disabled
			and is_zero_approx(_controller().carry_left()))
	# Hand the fibre back, so the switcher block below finds the take it describes.
	for _i in range(TAKE_FIBRE_CARRY_CEILING):
		_press_minus(HudLoadoutVocab.MATERIAL_ROW_META, TAKE_MATERIALS[1])
	await h._settle()

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
	h._assert_hud("loadout — …and that card is a GRANT again, its own carry and all",
		Q.has_label_containing(_panel(), GRANT_METER_NEEDLE)
			and not Q.has_label_containing(_panel(), TAKE_METER_NEEDLE))
	await _assert_no_dead_space("switched")
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
	# ⛔ **THE FIT LANDS OVER TWO FRAMES, so the bound waits for it.** A state that changes the subject
	# (a tab press, a re-push) re-renders a card whose header can be a different height — a split's
	# food lines — and one `_settle` returns before the second frame's height is applied. Measured
	# there, the bound reports the PREVIOUS content's height as dead space.
	await h._settle()
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

## ⛔ **THE FOOTER CONTROL DOES NOT READ AS A SEND, AND THIS IS HALF OF AN UNCONDITIONAL-FACE PAIR.**
## Taken here with both budgets unspent; its twin rides the fully-spent state, and neither is worth
## anything alone — a face asserted only where something is unspent passes on a control that renames
## itself once the budgets clear, and only where they are clear on one that renames itself while they
## do not.
##
## The negative is the one that matters: `Set out` was this button's face while it was the only thing
## that sent, and a player who read it and never pressed it lost the lot.
func _assert_the_footer_control_does_not_read_as_a_send() -> void:
	h._assert_hud("loadout — the footer control reads `%s`, never `%s` (got `%s`)"
			% [HudLoadoutVocab.CLOSE_LABEL, RETIRED_SEND_FACE, _close_face()],
		_close_face() == HudLoadoutVocab.CLOSE_LABEL and _close_face() != RETIRED_SEND_FACE)
	h._assert_hud("loadout — …and nothing else on the card says `%s` either" % RETIRED_SEND_FACE,
		not Q.has_label_containing(_panel(), RETIRED_SEND_FACE))

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
	# **UNSPENT — the warn arm.** `_closing_the_card_sends_nothing` left one unit off the pile, so the
	# window is open with something still to pick.
	await h._settle()
	# The precondition without which every accent claim below is about somebody else's row.
	h._assert_hud("loadout — the loadout row is the orb's ONLY entry for these two frames (%d)"
			% h._hud.turn_orb._entries.size(),
		h._hud.turn_orb._entries.size() == 1)
	_assert_orb_state("unspent", HudAttentionVocab.ATTENTION_SEVERITY_WARN, HudStyle.WARN)
	await _open_orb_popover()
	_assert_orb_row_reads("unspent", HudLoadoutVocab.ATTENTION_LABEL_UNSPENT,
		ORB_DETAIL_ONE_CARRY)
	await h._save("starting_loadout_orb_unspent")
	_close_orb_popover()

	# **COMPLETE — the ready arm.** One press puts the last unit back, and nothing else changes.
	_press_plus(HudLoadoutVocab.MATERIAL_ROW_META, PICKABLE[0])
	await h._settle()
	h._assert_hud("loadout — the last unit really is spent (%.1f carry left)"
			% _controller().carry_left(),
		is_zero_approx(_controller().carry_left()))
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
	# chapter — see `ORB_DETAIL_ONE_CARRY`.
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

## The carry meter's LABEL ink, read off the rendered label inside the meter block.
## Labels on the card drawn in the warning ink. The refusal line is the card's one legitimate warning
## and none is up where this is asked.
func _warn_labels_on_card() -> int:
	return _count_warn_labels(_panel())

func _count_warn_labels(node: Node) -> int:
	var count := 0
	if node is Label and (node as Label).is_visible_in_tree() \
			and (node as Label).get_theme_color("font_color") == HudStyle.WARN:
		count += 1
	for child in node.get_children():
		count += _count_warn_labels(child)
	return count

## Whether a row's `−` is enabled. The stepper's two buttons are found in the row as `+` is.
func _minus_enabled(meta: StringName, id: String) -> bool:
	var row := _row_node(meta, id)
	if row == null:
		return false
	var minus := Q.find_button_by_text(row, STEPPER_MINUS_FACE)
	return minus != null and not minus.disabled

func _meter_label_text() -> String:
	var meter := _row_node(HudLoadoutVocab.BUDGET_METER_META, HudLoadoutVocab.BUDGET_CARRY)
	if meter == null or meter.get_child_count() == 0 or not (meter.get_child(0) is Label):
		return ""
	return (meter.get_child(0) as Label).text

func _meter_label_color() -> Color:
	var meter := _row_node(HudLoadoutVocab.BUDGET_METER_META, HudLoadoutVocab.BUDGET_CARRY)
	if meter == null or meter.get_child_count() == 0 or not (meter.get_child(0) is Label):
		return Color()
	return (meter.get_child(0) as Label).get_theme_color("font_color")

## The footer control's rendered FACE. Read off the button rather than off a producer, because the
## claim is about what the player is looking at.
func _close_face() -> String:
	var close_button := Q.find_meta_node(_panel(), HudLoadoutVocab.CLOSE_BUTTON_META) as Button
	return close_button.text if close_button != null else ""

# ---- the orders this card has sent -------------------------------------------

## Every `set_starting_loadout` payload the HUD emits, in order. A METHOD rather than a lambda: a
## lambda captures a local by VALUE, so a witness assigning into one reports that nothing happened.
func _on_order(payload: Dictionary) -> void:
	_orders.append(payload)

## The last order sent for one band, or `{}` — what a server would have applied, and so what it
## publishes back.
func _last_order_for(entity: int) -> Dictionary:
	var band_id := _band_id(entity)
	for index in range(_orders.size() - 1, -1, -1):
		var order: Dictionary = _orders[index]
		if int(order.get("band_id", HudConst.NO_BAND_ID)) == band_id:
			return order
	return {}

## One band's published kits: the last order sent for it, else the default outfit the sim applied when
## it made the band.
## A window's goods load, computed here from the fixture's own item counts and weights — never asked
## of the controller, whose `_order_load` is what the food line is checked against.
const KIT_ITEM_COUNTS := {
	KIT_STALKING: KIT_STALKING_ITEMS, "trapping": KIT_TRAPPING_ITEMS, KIT_GATHERING: KIT_GATHERING_ITEMS,
}

func _window_goods_load(kits: Array, materials: Array) -> float:
	var load_total := 0.0
	for row in kits:
		load_total += ITEM_CARRY_WEIGHT * int(KIT_ITEM_COUNTS.get(
			String(row.get(HudLoadoutVocab.KIT_DEFAULT_ID_KEY, "")), 0)) \
			* int(row.get(HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY, 0))
	for row in materials:
		load_total += MATERIAL_CARRY_WEIGHT * int(row.get(HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY, 0))
	return load_total

## The food legend line's whole text, `""` where no line is drawn.
func _food_line_text() -> String:
	var line := Q.find_meta_node(_panel(), HudLoadoutVocab.FOOD_LINE_META) as Label
	return line.text if line != null else ""

## The food legend line's ink, or transparent where no line is drawn.
func _food_line_ink() -> Color:
	var line := Q.find_meta_node(_panel(), HudLoadoutVocab.FOOD_LINE_META) as Label
	if line == null:
		return Color(0, 0, 0, 0)
	return line.get_theme_color("font_color")

func _held_kits(entity: int, fallback: Array) -> Array:
	var order := _last_order_for(entity)
	if order.is_empty():
		return fallback
	return _window_rows(order.get("kits", []), "count",
		HudLoadoutVocab.KIT_DEFAULT_ID_KEY, HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY)

func _held_materials(entity: int, fallback: Array) -> Array:
	var order := _last_order_for(entity)
	if order.is_empty():
		return fallback
	return _window_rows(order.get("materials", []), "units",
		HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY, HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY)

## A command's `[{id, <amount>}]` rows as the WINDOW's `[{<id_key>, <amount_key>}]` ones — the two
## shapes the same allocation wears on the way out and on the way back.
func _window_rows(rows: Variant, order_amount_key: String, id_key: String,
		amount_key: String) -> Array:
	var out: Array = []
	if not (rows is Array):
		return out
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		out.append({
			id_key: String(row.get("id", "")),
			amount_key: int(row.get(order_amount_key, 0)),
		})
	return out

# ---- fixtures ---------------------------------------------------------------

## **THE CAMPAIGN'S HALF — one per world.** The pick list and the craftable ids.
##
## ⛔ **THE TWO PRE-FILLS ARE DELIBERATELY ABSENT.** The wire still carries them and the client reads
## neither: the SIM applies that spread when it makes the band, so it arrives on the window below, and
## a fixture stating it here as well would let a client that drew it a second time pass.
##
## `open` and the carry capacity are not here either — they are facts about one band and ride the
## cohort. The two carry WEIGHTS are, being one per world.
func _campaign() -> Dictionary:
	return {
		HudLoadoutVocab.PICKABLE_MATERIALS_KEY: PICKABLE.duplicate(),
		HudLoadoutVocab.CRAFTABLE_RECIPE_IDS_KEY: [
			RECIPE_SLED, RECIPE_CROOK, RECIPE_BASKETS, RECIPE_TRAPS, RECIPE_SPEARS,
			RECIPE_EARTHMOVING,
		],
		HudLoadoutVocab.ITEM_CARRY_WEIGHT_KEY: ITEM_CARRY_WEIGHT,
		HudLoadoutVocab.MATERIAL_CARRY_WEIGHT_KEY: MATERIAL_CARRY_WEIGHT,
	}

## The spawned band, carrying its own GRANT window. Stamped through `BandFx.with_band_id` like every
## fixture cohort in the walk — a `Band #<id>` on a frame means one reached the HUD without it.
##
## ⛔ **ITS ALLOCATION IS THE LAST ORDER THIS CARD SENT FOR IT**, and the profile's own spread before
## any — which is the sim: it applies the default outfit at the band's creation and applies each order
## as it arrives, republishing the band either way. A fixture that re-stated the default after a press
## would be a server that ignored the player, and the card would rightly adopt it.
func _grant_band() -> Dictionary:
	return _band(HOME_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		# The WHOLE carry, its larder FIXED inside it — the opening band, and the parent whose grant is
		# still open after the split below, are both this window.
		HudLoadoutVocab.CARRY_CAPACITY_KEY: GRANT_WHOLE_CARRY,
		HudLoadoutVocab.FOOD_SHARE_KEY: GRANT_FOOD,
		HudLoadoutVocab.FOOD_CARRIED_KEY: GRANT_FOOD,
		HudLoadoutVocab.FOOD_FIXED_KEY: true,
		# **A GRANT NAMES NO PARENT.** Its picks mint; nothing moves off another band.
		HudLoadoutVocab.PARENT_BAND_ID_KEY: HudLoadoutVocab.GRANT_PARENT_BAND_ID,
		HudLoadoutVocab.WINDOW_KITS_KEY: _held_kits(HOME_BAND_ENTITY, [
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_STALKING},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: "trapping",
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_TRAPPING},
			{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_GATHERING,
				HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: DEFAULT_GATHERING},
		]),
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: _held_materials(HOME_BAND_ENTITY, [
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "bone",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_BONE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "fibre",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_FIBRE},
			{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: "hide",
				HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: DEFAULT_HIDE},
		]),
	})

## The splinter a split just made, carrying a TAKE on the home band. Capped by its own carry AND the
## two supplies, each already `the home band's holdings + this take's standing units`, which is the
## number the sim refuses on.
##
## ⛔ **ITS ROWS ARE NOT EMPTY**, and a fixture that left them so would be staging the very state the
## split's kit-denominated default take exists to remove — a card reading zero on a band standing on
## its dowry, whose next press would hand the dowry back.
func _splinter_band(food_carried: float = -1.0) -> Dictionary:
	var kits := _held_kits(SPLINTER_BAND_ENTITY, [
		{HudLoadoutVocab.KIT_DEFAULT_ID_KEY: KIT_STALKING,
			HudLoadoutVocab.KIT_DEFAULT_COUNT_KEY: TAKE_DEFAULT_BIG_GAME},
	])
	var materials := _held_materials(SPLINTER_BAND_ENTITY, [
		{HudLoadoutVocab.MATERIAL_DEFAULT_ID_KEY: TAKE_MATERIALS[0],
			HudLoadoutVocab.MATERIAL_DEFAULT_UNITS_KEY: TAKE_DEFAULT_HIDE},
	])
	# **THE FOOD THE SIM WOULD HAND BACK** for the allocation this fixture publishes — the room its goods
	# leave, capped by the share — unless a state stages it.
	if food_carried < 0.0:
		food_carried = clampf(TAKE_CARRY_CAPACITY - _window_goods_load(kits, materials),
			0.0, TAKE_FOOD_SHARE)
	return _band(SPLINTER_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.CARRY_CAPACITY_KEY: TAKE_CARRY_CAPACITY,
		HudLoadoutVocab.FOOD_SHARE_KEY: TAKE_FOOD_SHARE,
		HudLoadoutVocab.FOOD_CARRIED_KEY: food_carried,
		HudLoadoutVocab.FOOD_FIXED_KEY: false,
		HudLoadoutVocab.PARENT_BAND_ID_KEY: _band_id(HOME_BAND_ENTITY),
		# **THE DEFAULT TAKE, kit-denominated** — what the split already moved, which is what the card
		# draws and what every press re-sends beside itself. Its expansion (2 spears, 2 sleds) is inside
		# the supply below by construction, the sim publishing `holdings + this take's standing units`.
		HudLoadoutVocab.WINDOW_KITS_KEY: kits,
		HudLoadoutVocab.WINDOW_MATERIALS_KEY: materials,
		HudLoadoutVocab.PARENT_ITEM_SUPPLY_KEY: [
			_supply("spears", TAKE_SPEARS), _supply("sled", TAKE_SLED),
			_supply("traps", TAKE_TRAPS), _supply("baskets", TAKE_BASKETS),
		],
		HudLoadoutVocab.PARENT_MATERIAL_SUPPLY_KEY: [
			_supply(TAKE_MATERIALS[0], TAKE_HIDE), _supply(TAKE_MATERIALS[1], TAKE_FIBRE),
			_supply(TAKE_MATERIALS[2], TAKE_CLAY),
		],
	})

## **A BAND WHOSE PUBLISHED ALLOCATION IS OVER ITS CARRY** — the reported screen, staged the one way
## a client can reach it. Its carry is the post-split one and its accepted rows are the pre-split
## allocation, which is the shape the duplication bug left behind; the card draws a published
## allocation as-is, so the meter reads negative.
func _over_budget_band() -> Dictionary:
	return _band(OVER_BAND_ENTITY, {
		HudLoadoutVocab.OPEN_KEY: true,
		HudLoadoutVocab.CARRY_CAPACITY_KEY: OVER_CARRY_CAPACITY,
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
		"working_age": WORKING_AGE,
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
