extends RefCounted

## The forage accounts, the build dip and the floor chart.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 187

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")

## **RETIRED — `COMPOSE_PLAIN_STEPPERS_PER_SHEET`** (`docs/plan_standing_upkeep.md` §2.5). It counted
## the UNTAGGED steppers, because the build crew's row carried a meta of its own and only the keeping
## row was retired. Both rows are gone now, so there is nothing left to tag and nothing to exclude:
## `Readout.COMPOSE_STEPPERS_PER_SHEET` counts every stepper on the sheet, which is the stronger
## claim and the one a hypothetical build slider cannot slip past.

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## **A PLANT SOURCE HAS NO BODY TO COUNT.** The crew terms take a whole-animal quantum beside their
## engagement pair, and a patch answers `0` for it — grain is gathered by the handful — so every
## plant-side recomposition below spells this and `SourceForecast.NO_ENGAGEMENT_STAGE` rather than
## passing two unexplained zeros. It is what makes those calls read as *this web has no reach arm*.
const PLANT_NO_BODY := 0.0

# "leave the floor alone" for `_compose_herd`'s optional argument — a sentinel OUTSIDE the legal
# `0..1` range, since every real floor including `0` is a value a frame may want to dial.
const HAY_MEADOW_FODDER_PER_BIOMASS := 0.005

# ---- THE FLOOR CHART's five cases (docs/plan_harvest_floor.md §7.3) -----------------------------
# A floor ABOVE a nearly-full patch's stock, so nothing stands above the line and the flag has to flip
# below it — the two things `floor_chart_full` is judged on.
const FLOOR_CHART_ABOVE_STOCK := 0.95

## A SECOND live-drag floor for the teaching line, and it has to sit BELOW this state's standing
## stock. The drag before it parks the floor ABOVE the stock, where the aside correctly reads
## "Teaching nothing: nothing is being taken" — and any other floor still above the stock reads the
## SAME sentence, so the assertion would compare a string with itself and pass on a line that never
## re-read. This value crosses the sim's work predicate, so the drag moves the aside from that end of
## the non-degeneracy rule to a live rate.
const FLOOR_CHART_TEACHING_DRAG_FLOOR := 0.10

## The faction's Cultivation while the chart block renders — part-learned, so its WILD patches still
## have a lesson to teach and the aside's teaching line exists to be dragged and compared at all. The
## frames above this block complete every track, and a source teaches nothing once its lesson is
## known; `forage_lesson_known` flips it back to 1.0 and asserts exactly that.
const FLOOR_CHART_CULTIVATION_LEARNING := 0.55

# A stock already drawn well below the food peak but comfortably above a plant's reseed floor: low
# enough that the projection's descent to the floor is legible, high enough that the curve has room to
# flatten rather than bottoming out in the first turn.
const FLOOR_CHART_DRAWN_STOCK_FRACTION := 0.35

# The floor that patch is worked to — under its stock, clear of the plot's baseline, so the curve's
# descent and the FLAT it holds afterwards are both legible.
const FLOOR_CHART_HELD_FLOOR := 0.20

## The three forage-rung faces `_hay_meadow_tile_fixture` / `_dead_season_tile_fixture` are judged on
## (issue #426), spelled out as literals for the same reason as the boar pair above. The first two are
## the TWO-account line the plant web grew a column for, in wire order (provisions · fodder) and
## ascending on both between them. The third is the one surviving zero: a rung whose ceiling EXISTS
## and is empty says so, which is the whole difference between "pays nothing this season" and "the
## wire never described this patch". (A trade-goods clause stood between the two until arc #527.)
# The preset metric as the TOOLTIP spells it (`SourceForecast.extractive_take_pair`'s `full`), which
# is where it lives now that the button face states the intent alone. The face's compact spelling
# (`0.60 food · 0.20 fodder`) has no surface left to be read from.
const HAY_PEAK_TOOLTIP := "up to +0.60/turn · +0.20 fodder/turn"

const HAY_STRIP_TOOLTIP := "up to +1.35/turn · +0.45 fodder/turn"


const DEAD_SEASON_TOOLTIP := "up to +0.00/turn"

## The crew the hay meadow's overdraw frame is composed at — the smallest that puts the FODDER take
## past its Sustain ceiling (3 × 0.13 = 0.39 against 0.20) while the FOOD take (0.24) is still inside
## the patch's 0.60. One forager overdraws nothing at all, so a smaller crew would pass that state's
## claim vacuously.
## **THE YIELDS CAPTION, AND THE ONE IT REPLACED.** The readings are what the crew banks NEXT TURN
## (`SourceForecast.expected_next_turn_yield`), so the caption names a turn rather than a rate; the
## retired needle is asserted ABSENT beside it, since a header that had stopped rendering entirely
## would satisfy the positive on its own. Uppercased because `alloc_section_label` uppercases what it
## is given.
const YIELDS_HEADER_NEEDLE := "NEXT TURN"
const RETIRED_PER_TURN_HEADER_NEEDLE := "PER TURN"

## The two arrowed readings on `floor_chart_drawn_down` — a burst falling to the rate the patch holds.
## Literals, because the CLAIM is the pair of numbers: the two come from one function asked with two
## ceilings, and composing them here from that same function would assert only that it agrees with
## itself.
const BURST_FOOD_READING := "0.24 → 0.06"
const BURST_FODDER_READING := "0.08 → 0.02"

## **THE FORWARD-HEADLINE PAIR'S OWN TERMS** (`forage_at_floor` / `forage_below_floor`). The crew is
## sized so the ROOM binds rather than the hands — a crew-bound take would read the same number
## whichever ceiling the sheet quoted, and the claim would be vacuous.
const AT_FLOOR_FORAGERS := 6

## …and the stock the second state stands at, as a fraction of K: far enough under the food peak that
## one turn's regrowth does not carry it back over, which is the one state that still pays nothing.
const BELOW_FLOOR_STOCK_FRACTION := 0.2

## The account the headline must still be stating, and the zero it must not be. `0.00` is what the
## empty room used to print, so it is asserted absent at the floor and PRESENT below it.
const AT_FLOOR_ACCOUNT_NEEDLE := "FOOD"
const EMPTY_TAKE_NEEDLE := "0.00 FOOD"

## …and the refusal's own words, which become true exactly when they are shown.
const AT_FLOOR_REFUSAL_NEEDLE := "until it grows past"

## The countdown verdict's two ends, as needles rather than as the format string — the claim is about
## the SHAPE of the sentence (it opens with the count and stops there), and a `%d` cannot be matched.
const REACHES_FLOOR_HEAD := "Reaches the floor in "
const REACHES_FLOOR_TAIL := " turns."

## **A NEEDLE FOR A RETIRED CLAUSE, KEPT SO IT STAYS RETIRED.** The countdown used to close by
## promising the equilibrium it was counting down to; the readout says that itself, in
## `VERDICT_HOLDS_AT_FLOOR`, the moment it is true. This is deliberately spelled out here rather than
## composed from a const, because there is no const left to compose it from.
const RETIRED_REACHES_AFTERMATH := ", then holds it"

const HAY_OVERDRAW_FORAGERS := 8

## ---- THE LOCKED FODDER ACCOUNT (issue #485) ---------------------------------------------------
## The crew the three fodder-lock states are composed at. It has to put a NON-ZERO fodder take on the
## row — the lock mutes a reading, so a crew taking no hay would make every state pass vacuously —
## while staying below `HAY_OVERDRAW_FORAGERS`, since the take is what draws the patch down and the
## overdraw verdict is deliberately unmoved by the lock.
const FODDER_LOCK_FORAGERS := 3

## ---- THE NO-FOOD BASKET (the tile the worker cap was reported on) -----------------------------
## What one gatherer moves off this ground per turn, in BIOMASS — the term every crew answer divides
## by. Stated here rather than left to the seeder's food-derived recovery, which cannot run on a patch
## whose food rate is zero.
const NO_FOOD_BASKET_CARRY := 4.0
## Hay pays feed; tobacco and hay's stalks pay two materials. Deliberately UNEQUAL amounts, the
## cash-crop tile's rule: a vector summed into one figure is visibly neither of them.
const NO_FOOD_BASKET_FODDER_PER_BIOMASS := 0.012
const NO_FOOD_BASKET_TOBACCO_PER_BIOMASS := 0.020
const NO_FOOD_BASKET_FIBRE_PER_BIOMASS := 0.008
## Real `materials.json` ids, and the catalogue ships no display name — so the id IS the display word,
## exactly as it is on the crop picker's basket rows and on the wolf's readout.
const NO_FOOD_BASKET_TOBACCO_ID := "tobacco"
const NO_FOOD_BASKET_FIBRE_ID := "fibre"
## The crew the frame is composed at — deliberately BELOW the crew that clears the patch's room in one
## turn, so the take's `min` binds on the CREW arm. At the clearing crew the two arms are equal by
## construction and a readout that never read the per-worker rate would print the same number; the
## frame asserts the relation rather than trusting this comment.
const NO_FOOD_BASKET_QUOTE_CREW := 4

## The faction's Foddering, dialed as a PART-LEARNED meter rather than a bare 0 in the locked state:
## it is a 0..1 learning track like every other, and only `KNOWLEDGE_COMPLETE` opens the credit, so a
## fixture at 0 could not tell "unlearned" from "partly learned but still refused".
const FODDER_LOCK_PROGRESS := 0.42

## The peak preset's TOOLTIP with the hay locked — `HAY_PEAK_TOOLTIP` minus its fodder clause, and
## NOTHING in its place: a tooltip is one flat string with nowhere to hang the reason, so a refused
## ceiling is dropped rather than dashed or zeroed. Written out rather than sliced off that constant,
## for the reason every literal in this file is: a derived needle passes on whatever the code emits.
const HAY_PEAK_TOOLTIP_FODDER_LOCKED := "up to +0.60/turn"


## Which line of a rung's two-line face carries the metric: line 0 is the rung NAME
## (`HudFormat.policy_face`), line 1 the products (`HudWidgets._policy_rung_cell` builds them in that
## order). A rung with no metric wears line 0 alone.
const POLICY_RUNG_METRIC_LINE := 1

# ---- LEGACY FIXTURE ADAPTER: the four stances -> the escapement floor ---------------------------
# Every fixture in this file states a source's take as the retired per-STANCE ceiling table, because
# that is what the wire carried when they were written. The wire carries the per-biomass yield VECTOR
# now (`docs/plan_harvest_floor.md` §5) and the client composes `max(0, B - floor*K) x rate` at any
# floor, so the tables are converted HERE, in one place, rather than by rewriting ~50 literals.
#
# **THE CONVERSION PINS THE OLD `sustain` ROW TO THE FOOD PEAK**, which is the honest mapping: Sustain
# took the herd's renewable yield and the food peak is the floor that pays the most forever. So every
# frame's headline number at the DEFAULT floor is the number these fixtures were tuned to show, and
# what changes is that the other two presets now read off one curve instead of four authored rows.
#
# `B` and `K` come from the fixture when it carries a usable pair; otherwise they are seeded, because
# a fixture written before the floor existed had no reason to state a stock the client would divide
# by. The seeded pair leaves a real spread across the presets (strip 2.25x the peak, learn 0.25x).
const STALE_VERB_THROUGHPUT_EPSILON := 0.01

# ---- THE BUILDING PATCH: the regime where the REGROWTH beats the ROOM ---------------------------
# Reported from play, and the frame three separate defects appear in AT ONCE — none of them visible
# on any other fixture, because all three need the same narrow regime: a crew whose whole-turn carry
# is a shade UNDER the patch's own regrowth. There the standing room is a puddle, the regrowth is a
# river, and the sheet's four numbers stop agreeing with one another:
#
#   • `clear it now` was `room ÷ carry` = 5 — a crew that provably clears nothing, since the patch
#     regrows more each turn than those five hands can lift, printed two lines above a verdict saying
#     seven are needed. It is now floored on the reaching crew.
#   • `⚠ OVERDRAWS THE PATCH` fired beside a verdict reading *it settles at 54% and holds there*: the
#     take-vs-food-peak test is `take > 0` on a patch standing at the peak, i.e. a fact about the
#     FLOOR. Gated on the projection now.
#
# **THE DIP THAT ORIGINALLY PRODUCED THIS REGIME IS RETIRED** (`docs/plan_standing_upkeep.md` §2.2),
# and the regime is not: what it needs is a crew whose throughput sits just under the patch's peak
# regrowth, which the fixture now states OUTRIGHT as its own `perWorkerBiomass` instead of getting it
# by multiplying a full carry by a rung's fraction. Same numbers, one authority.
#
# **THE ARITHMETIC WAS NEVER WRONG — the numbers only disagreed with each other**, and the assertions
# below are RELATIONS between the rendered numbers, not literals: a fixture that drifts must fail
# rather than quietly re-baseline.
const BUILD_DIP_CAPACITY := 195.0

# Just under the food peak (97.5), so the room above the 45% floor is ~9 biomass while the patch
# regrows ~12 — the inversion the whole frame rests on. Also makes the food-peak ceiling ZERO, which
# is what let the overdraw test degenerate into "the floor is below the peak".
const BUILD_DIP_STOCK := 97.0

const BUILD_DIP_FLOOR := 0.45

# Six foragers × 2.0 = 12.0 biomass/turn — a shade under the ~12.19 the patch regrows at its peak, so
# the stock RISES under this crew and settles above the floor. One more hand reverses it, which is
# what makes the pair an A/B on the overdraw gate rather than one frame's say-so.
const BUILD_DIP_CREW := 6

const BUILD_DIP_DECLINE_CREW := 7

# **THE THROUGHPUT THAT PUTS THE CREW IN THE REGIME**, stated by the fixture rather than composed from
# a retired dip. 2.0 biomass a forager: six of them move 12.0, which is the whole point of the frame.
const BUILD_PATCH_PER_WORKER_BIOMASS := 2.0

# The band's hands. It must be able to REACH the reaching crew, or the cap — not the fix — is what the
# frame would be measuring.
const BUILD_DIP_IDLE_WORKERS := 9

## The METRIC line of a policy rung's two-line face — `0.24 food · 0.40 fodder`, the products line
## the payoff/cap assertions read. The rung is found by `HudWidgets.POLICY_RUNG_META`, its identity,
## and NEVER by button text: the face lives on a two-Label stack beside an empty-`text` Button, so
## `Q.find_button_by_text` finds nothing at all here. "" when the rung is absent from the picker or
## wears its name alone (no metric).
## A preset button's TOOLTIP — where the floor's metric lives now that the face carries only the
## intent. Reached by the rung's meta like everything else here, never by its face.
func _policy_rung_tooltip(root: Node, policy: String) -> String:
	var btn := Q.find_policy_rung(root, policy)
	return btn.tooltip_text if btn != null else ""

func _policy_rung_metric(root: Node, policy: String) -> String:
	var btn := Q.find_policy_rung(root, policy)
	if btn == null:
		return ""
	# The face's Labels are siblings of the Button under the rung's CELL, not children of it.
	var lines := Readout.face_lines(btn.get_parent())
	return lines[POLICY_RUNG_METRIC_LINE] if lines.size() > POLICY_RUNG_METRIC_LINE else ""

## **UNCHECK THE RUNNING BOX AND CHECK WHAT THE CLIENT WOULD TRANSMIT** — driven through the REAL
## control and the REAL formatter, not through the handler each would have called.
##
## The chain is the player's: move the BUILDERS stepper, press the sheet's own commit button, capture
## the payload off `HudLayer.improvement_requested`, and run it through `Main.format_improvement` —
## the pure static `Main._on_hud_improvement` dispatches to. Asserting the LINE rather than the payload is what makes this test the shipped representation:
## the payload could carry a perfectly good herd id and still be formatted into the tile-targeted
## grammar, which is exactly the mistake the two webs' differing targeting rules invite.
##
## Restores the composed improvement afterwards, so the frame that just rendered is not disturbed for
## whatever asserts against it next.
## The compose sheet's EYEBROW, as rendered — the header is one BBCode `RichTextLabel` holding
## `<EYEBROW>  <subject>`, so `get_parsed_text` is what a player actually reads off it. "" when no
## sheet is open, which fails a `begins_with` rather than satisfying it.
func _compose_sheet_eyebrow() -> String:
	var sheet: Control = h._hud._drawercompose._compose_sheet
	if sheet == null or sheet._header == null:
		return ""
	return (sheet._header as RichTextLabel).get_parsed_text().strip_edges()

## **THE PLANT WEB'S CREW NOUN, ON ALL FOUR SURFACES OF ONE FRAME.** Stages a patch, builds the drawer's
## read state and opens its sheet, then asserts the sheet's eyebrow, the crew-row label, the commit
## button and the drawer's open button all name `want_label` — plus, independently of `want_label`,
## that the eyebrow and the stepper AGREE. That last one is the point: the reported failure mode is a
## header saying one noun over a stepper saying another, and it is expressible whenever the two resolve
## separately, so it is asserted as a RELATION between two rendered strings rather than against a
## constant either could drift from.
##
## `improvement` composes a build IN FLIGHT (`""` for none). It must not move the noun — a crew clearing
## ground is still foraging the stand — which is exactly what the `plant_crew_wild_building` /
## `plant_crew_wild_sowing` states pass it for.
## **THE COLLAPSE ITSELF, ASKED OF THE RESOLVER** (`docs/plan_standing_upkeep.md` §4.9 item 12c) —
## PNG-less, because it is a claim about a STRING and every one of these rungs renders a perfectly
## plausible sheet whichever noun it drew.
##
## The five rendered states above each pin the four surfaces of ONE rung; they cannot say that the
## rungs AGREE, since five separate equalities against one const are also satisfied by five separate
## consts that happen to match. This asks `plant_crew_label` directly over the three rungs the retired
## fork split — wild ground, a Tended Patch, and a Field sown straight from wild ground — plus the
## non-empty guard without which "they all answer the same" is satisfied by "they all answer nothing".
func _assert_plant_crew_noun_is_rung_blind() -> void:
	var wild := HudFormat.plant_crew_label(BaseFx.food_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var tended := HudFormat.plant_crew_label(TileFx.tended_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var field := HudFormat.plant_crew_label(_wild_sown_field_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._assert_hud("the plant crew noun is the SAME at every rung — wild `%s`, tended `%s`, field `%s`"
			% [wild, tended, field],
		wild != "" and wild == tended and tended == field)
	h._assert_hud("…and it is the one the vocabulary declares (`%s`)"
			% HudComposeVocab.HARVEST_CREW_LABEL,
		wild == HudComposeVocab.HARVEST_CREW_LABEL)

func _assert_plant_crew_noun(state_name: String, tile: Dictionary, want_label: String,
		improvement: String = SourceForecast.IMPROVEMENT_NONE) -> void:
	h._hud._compose.reset_forage_source()
	h._show_tile(tile)
	# Drop the previous tile's button so this state gets a FRESH drawer build rather than the
	# same-shape patch path — the noun must be right on both, and the patch path is covered by
	# `forage_assign_button_targets_selected_tile`.
	h._hud._drawercompose._clear_forage_drawer()
	await h._settle()
	h._hud._drawercompose.build_forage_drawer_actions(
		ForageFx.floorify(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX))
	h._compose_forage(tile)
	if improvement != SourceForecast.IMPROVEMENT_NONE:
		# **DIAL THE VERB AFTER THE FIRST OPEN, THEN RE-OPEN — the herd sheet's contract, and the
		# forage sheet keeps it too.** Opening on a DIFFERENT source re-seeds the composition off the
		# band's own standing build (`_build_forage_assign_controls`' `source_changed` branch →
		# `seed_forage`), so a verb set BEFORE the first open is silently thrown away: measured, the
		# Cultivate and Sow frames came back BYTE-IDENTICAL, both rendering whatever the re-seed
		# produced rather than the build under test.
		h._hud._compose.set_forage_improvement(improvement)
		h._compose_forage(tile)
	await h._settle()
	if improvement != SourceForecast.IMPROVEMENT_NONE:
		# The fixture must actually REACH the state being claimed — a build the sheet quietly dropped
		# would leave this whole state asserting the no-build case twice under two names.
		h._assert_hud("%s: the sheet really is composing a live `%s`" % [state_name, improvement],
			h._hud._compose.forage_improvement() == improvement)
	await h._save(state_name)
	var sheet: Control = h._hud._drawercompose._compose_sheet
	var eyebrow := _compose_sheet_eyebrow()
	var stepper_label := Readout.crew_row_label(sheet)
	var commit := Q.compose_commit_button(sheet)
	var open_btn: Button = h._forage_open_button()
	var want_eyebrow := (HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % want_label.to_lower()).to_upper()
	# ⛔ **THE LIVENESS HALF, and it is what keeps this set worth anything now that all five states
	# expect ONE noun** (§4.9 item 12c). Every claim below is an EQUALITY against strings resolved from
	# the vocabulary, so a resolver answering `""` — and a `PLANT_ASSIGN_BUTTONS` lookup missing its one
	# key, which also answers `""` — would be satisfied by controls that drew nothing at all.
	var want_verb := String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(want_label, ""))
	h._assert_hud("%s: the crew NOUN and its VERB both resolve to something (`%s` / `%s`)"
			% [state_name, want_label, want_verb],
		want_label != "" and want_verb != "")
	h._assert_hud("%s: the sheet's eyebrow reads `%s`" % [state_name, want_eyebrow],
		eyebrow.begins_with(want_eyebrow))
	h._assert_hud("%s: the crew row is labelled `%s`" % [state_name, want_label.to_upper()],
		stepper_label == want_label.to_upper())
	h._assert_hud("%s: the commit button reads `%s`" % [state_name,
			String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(want_label, ""))],
		commit != null and commit.text == String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(want_label, "")))
	h._assert_hud("%s: the drawer opens with `%s`" % [state_name,
			HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % want_label.to_lower()],
		open_btn != null
			and open_btn.text == HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % want_label.to_lower())
	# THE CONSISTENCY CLAIM, stated without naming the noun — a header and a stepper that resolve
	# through one function cannot disagree, and a frame where they do is the defect itself.
	h._assert_hud("%s: the eyebrow and the stepper name the SAME crew on one frame" % state_name,
		stepper_label != "" and eyebrow.begins_with("%s %s" % [
			(HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % "").strip_edges().to_upper(),
			stepper_label]))

## One account's `now → after` pair, parsed off the RENDERED face — `[now, after]`, or `[0, 0]` when
## that account states no transition. Parsed rather than recomputed: a helper that asked
## `expected_yield_account` twice would agree with the widget by construction and testify to nothing.
func _yield_now_after(yields_text: String, account: String) -> Array:
	# The face reads `<now> → <after> <ACCOUNT>`, so the pair is the three tokens before the account.
	var upto := yields_text.split(account)[0].strip_edges().split(" ", false)
	if upto.size() < 3 or upto[upto.size() - 2] != "→":
		return [0.0, 0.0]
	return [float(upto[upto.size() - 3]), float(upto[upto.size() - 1])]

## **THE PLAYED TILE — a FINISHED Tended Patch whose crew a stale `Cultivate` was still dipping.**
##
## Every term is the SHIPPED one, because the whole point is that this is the arithmetic a LIVE patch
## produces and the preview fixtures could not: `per_worker_biomass_capacity` 8.0 × the tile's seasonal
## weight, which worldgen fixes at `INITIAL_SEASONAL_WEIGHT` 1.0 and nothing ever moves; the plant
## rungs' `yield_fraction_while_building` 0.25; and a basket of Wild Tubers 35% · Cotton 30% · Flax 20%
## · Wild Rice 15%, of which only the two staples pay food — 0.35 × 0.065 + 0.15 × 0.070 — so the patch
## converts at `STALE_VERB_FOOD_PER_BIOMASS`, the two cash crops paying materials rather than meals.
##
## **It states its own stock and capacity, so it deliberately does NOT go through `BaseFx.seed_forage_rows`**,
## which pins every fixture it touches to one `FIXTURE_CAPACITY`/`FIXTURE_STOCK_FRACTION` pair. This
## frame is about a particular `B / K` — a patch standing just above the floor it is worked at, where
## the crew is bound by the REGROWTH rather than by the room — and the per-biomass vector states the
## ceiling directly anyway. `ForageFx.floorify` still seeds the growth curve and the phase cuts from it.
func _stale_verb_tile_fixture() -> Dictionary:
	return RungFx.stamp_patch({
		"x": 68, "y": 12,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.72,
		"temperature": 18.0,
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": ForageFx.STALE_VERB_STOCK,
		"patch_carrying_capacity": ForageFx.STALE_VERB_CAPACITY,
		# THE RUNG THE VERB NAMES IS BUILT. `is_cultivated` is what the improvement control reads to
		# render its DONE label instead of a running meter — and, since this fix, what tells the crew
		# terms that the Cultivate still sitting in the compose state is a stale verb.
		"patch_is_cultivated": true,
		"patch_cultivation_progress": 1.0,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		"patch_provisions_per_biomass": ForageFx.STALE_VERB_FOOD_PER_BIOMASS,
		"patch_fodder_per_biomass": 0.0,
		"patch_per_worker_biomass": ForageFx.STALE_VERB_PER_WORKER_BIOMASS,
		"patch_per_worker_yield": ForageFx.STALE_VERB_PER_WORKER_BIOMASS * ForageFx.STALE_VERB_FOOD_PER_BIOMASS,
		# The two plant dips, as the wire carries them: `BuildDips::for_branch` publishes BOTH rungs'
		# fractions whatever the patch has already climbed, which is exactly why the fraction alone
		# cannot say "nothing left to build here" and the done flag above has to.
		# The ground is rich but away from fresh water, so the next rung is offered and REFUSED — the
		# sheet's improvement row is a done label over a site gate, with no running build anywhere on it.
		"patch_sow_site_refusal": "too_dry",
		"patch_tended_yield": 1.20,
		"patch_field_yield": 2.40,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.35,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "cotton", "role": "cash", "display_name": "Cotton Fields", "share": 0.30,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 1.40, "sow_yield_ratio": 2.60,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
			{"species": "flax", "role": "cash", "display_name": "Flax Fields", "share": 0.20,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.30, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
			{"species": "wild_rice", "role": "staple", "display_name": "Wild Rice", "share": 0.15,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
	}, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## The band working that patch — 2 foragers, NO improvement (the sim cleared the assignment's verb the
## turn the Cultivate completed), no idle hands, and its rate filled in by the caller from the tile's
## own wire terms so the drawer's standing summary and the sheet's crew targets cannot state two
## different throughputs by fixture drift.
func _stale_verb_band_fixture(rate: float) -> Dictionary:
	return {
		"id": "Band 1",
		"size": 30,
		"entity": 821,
		"faction": 0,
		"pos": [67, 11],
		"current_x": 67, "current_y": 11,
		"activity": "forage",
		"working_age": 16,
		"idle_workers": 0,
		"work_range": 3,
		"turns_of_food": 12.0,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": ForageFx.STALE_VERB_CREW,
				"target_x": 68, "target_y": 12, "floor": ForageFx.STALE_VERB_FLOOR,
				"improvement": "",
				"actual_yield": rate, "sustainable_yield": rate, "realized_yield": rate,
				"overdraws": false},
		],
	}

## **THE PATCH BEING CULTIVATED — a WILD stand with the rung's build genuinely in flight.**
##
## The stale-verb patch one screen up is its exact opposite and the pair is the point: there the
## `Cultivate` was a leftover verb that must dip NOTHING, here it is a real build that must dip
## everything, and the same fields (`is_cultivated` / `cultivation_progress`) decide which. So this
## one is UNCULTIVATED with a part-filled meter — `_build_improvement_control`'s RUNNING branch, a
## live 25% carry, and no knowledge gate anywhere near it (the running branch is chosen before the
## offer is looked up).
##
## Its stock and capacity are its own (no `BaseFx.seed_forage_rows`), for the reason the stale-verb fixture
## gives: this frame is about a particular `B / K` — a hair under the food peak — and a shared
## capacity/stock pair would round the whole regime away.
func _building_patch_tile_fixture() -> Dictionary:
	return RungFx.stamp_patch({
		"x": 68, "y": 12,
		"terrain_label": "Alluvial Plain",
		"tags_text": "Fertile, Fresh Water",
		"visibility_state": "active",
		"habitability": 0.72,
		"temperature": 18.0,
		"food_module": "riverine_delta",
		"food_module_label": "Riverine Delta",
		"site_name": "",
		"patch_ecology_phase": "thriving",
		"patch_biomass": BUILD_DIP_STOCK,
		"patch_carrying_capacity": BUILD_DIP_CAPACITY,
		# WILD ground with the rung under construction — the two fields `improvement_is_done` reads,
		# stated the opposite way round from the stale-verb patch.
		"patch_is_cultivated": false,
		"patch_cultivation_progress": 0.35,
		"patch_is_field": false,
		"patch_field_progress": 0.0,
		# The stale-verb patch's basket, verbatim: only the two staples pay food, so the patch converts
		# at well under a pure-staple rate and the ⚠ has a real take to fire on.
		"patch_provisions_per_biomass": ForageFx.STALE_VERB_FOOD_PER_BIOMASS,
		"patch_fodder_per_biomass": 0.0,
		"patch_per_worker_biomass": BUILD_PATCH_PER_WORKER_BIOMASS,
		"patch_per_worker_yield": BUILD_PATCH_PER_WORKER_BIOMASS * ForageFx.STALE_VERB_FOOD_PER_BIOMASS,
		# **THE STANDING UPKEEP the Cultivate under construction will owe once it stands.** The demand
		# is `0` while the meter is still going up — those hands are the BUILD's — which is exactly the
		# reading the keeping row has to render as words rather than as "wants 0".
		"patch_upkeep_demand": 0.0,
		"patch_upkeep_supplied": 0.0,
		"patch_upkeep_shortfall": 0.0,
		"patch_upkeep_workers_needed": 0,
		"patch_has_neglect_grace": true,
		"patch_neglect_grace_remaining": 3,
		"patch_sow_site_refusal": "too_dry",
		"patch_tended_yield": 1.20,
		"patch_field_yield": 2.40,
		"patch_composition": [
			{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.65,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.10, "sow_yield_ratio": 3.60,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "flax", "role": "cash", "display_name": "Flax Fields", "share": 0.35,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.30, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
	}, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## The band cultivating it — enough idle hands that the STEPPER, not the roster, is what bounds the
## crew. The reaching crew is the number the *clear it now* target now names, and a band that cannot
## staff it would make every assertion about that target a claim about labor scarcity instead.
##
## **IT CARRIES THE STANDING ASSIGNMENT, and that is what makes the build LIVE rather than LAPSED.** A
## part-filled cultivation meter with nobody on the tile is a patch REVERTING, which is what the tile
## card would say — a different state from the one this frame is about, rendered beside a sheet
## composing the opposite. `rate` is filled in by the caller from the tile's own wire terms, the
## stale-verb band's rule: the card's standing rate and the sheet's crew targets must be answering
## about one patch by construction.
func _building_patch_band_fixture(rate: float) -> Dictionary:
	return {
		"id": "Band 1",
		"size": 34,
		"entity": 823,
		"faction": 0,
		"pos": [67, 11],
		"current_x": 67, "current_y": 11,
		"activity": "forage",
		"working_age": 20,
		"idle_workers": BUILD_DIP_IDLE_WORKERS,
		"work_range": 3,
		"turns_of_food": 12.0,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": BUILD_DIP_CREW,
				"target_x": 68, "target_y": 12, "floor": BUILD_DIP_FLOOR,
				"improvement": "cultivate",
				"actual_yield": rate, "sustainable_yield": rate, "realized_yield": rate,
				# The stock RISES under this crew, so the sim's own flag is false here — the fact the
				# sheet's ⚠ was contradicting.
				"overdraws": false},
		],
	}

## A COMPLETED Field — the top of the plant ladder. The row must read "▦ Field" (SIGNAL), a visibly
## DIFFERENT THING from "🌾 Tended Patch", not a bigger percentage.
## **A FIELD SOWN STRAIGHT FROM WILD GROUND — the state `ForageFx.field_tile_fixture` cannot reach.** That one
## climbs the ladder rung by rung (`ForageFx.sowing_tile_fixture` sets `patch_is_cultivated`), so on it a
## Field is also cultivated and the retire test passes for the wrong reason. `Sow` needs no prior
## patch, so this is the shipped shape too: rung 3 built, rung 2's meter at ZERO and staying there.
## It is the frame the "a completed Field offers Cultivate" defect lived in.
func _wild_sown_field_tile_fixture() -> Dictionary:
	var tile := ForageFx.field_tile_fixture()
	tile["patch_cultivation_progress"] = 0.0
	tile["patch_is_cultivated"] = false
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## **A TILE THAT PAYS BOTH ACCOUNTS — the frame face treatment A is judged on (#426).** A hay meadow:
## thin human food and real FODDER, which is the account the whole plant web grew a column for. Every
## other forage fixture pays provisions alone, so until this one existed the picker's two-account face
## and the column ceiling it triggers had NO frame at all. (It paid a third, trade-goods account until
## arc #527 retired that axis.)
##
## **The rows are written out rather than derived.** `BaseFx.seed_forage_rows` seeds fodder to 0 by
## design (so a reseeding pass leaves every existing frame byte-identical), which is exactly the thing
## under test here — so this fixture overwrites the account dicts afterwards, the "genuinely
## non-derivable row" case that helper's docstring names.
##
## **EVERY ACCOUNT DESCENDS WITH THE FLOOR, and that is a real simplification.** A non-food column used
## to be non-monotone: `Deplete` alone carried `market.trade_goods_multiplier` (x4), a POLICY markup on
## stripping the patch for sale, so its cell sat above Eradicate's. The harvest-floor arc retired that
## markup — a deeper floor earns more only because it takes more BIOMASS — so both accounts are one
## stock through fixed rates and no column can invert.
## The hay meadow's own STANDING forage row, carrying the sim's verdict and nothing that could be
## mistaken for a second one. `actual_yield` deliberately sits ABOVE `sustainable_yield` on BOTH
## halves of the A/B: that pair is the retired client-side comparison's own inputs, so a row whose
## flag disagrees with it is the one shape that can tell the field from the derivation.
func _hay_standing_row(overdraws: bool) -> Dictionary:
	return {
		"kind": SourceForecast.LABOR_KIND_FORAGE,
		"workers": HAY_OVERDRAW_FORAGERS,
		"target_x": HAY_MEADOW_X, "target_y": HAY_MEADOW_Y,
		"floor": SourceForecast.FLOOR_FOOD_PEAK,
		"actual_yield": 0.63, "sustainable_yield": 0.20, "realized_yield": 0.63,
		"overdraws": overdraws,
	}

## The meadow's coordinates, named because the tile fixture and its standing row must agree on them —
## `forage_assignment_of` matches by tile, so a row one hex out silently answers "nobody works this".
const HAY_MEADOW_X := 65
const HAY_MEADOW_Y := 9

func _hay_meadow_tile_fixture() -> Dictionary:
	var tile := ForageFx.fodder_basket_tile_fixture()
	tile["x"] = HAY_MEADOW_X
	tile["y"] = HAY_MEADOW_Y
	tile["terrain_label"] = "Prairie Steppe"
	tile["food_module"] = "savanna_grassland"
	tile["food_module_label"] = "Savanna Grassland"
	tile["site_name"] = ""
	# **THE TWO ACCOUNTS BIND DIFFERENTLY, and that is the fixture's real job** — see
	# `HAY_MEADOW_FODDER_PER_BIOMASS` for the sizing. Food is slow to GATHER off ground that carries
	# plenty of it, so LABOR binds on provisions; hay comes in fast off a meadow that regrows little of
	# it, so the CEILING binds on fodder.
	tile["patch_per_worker_yield"] = 0.08
	tile["patch_ceiling_sustain"] = 0.60
	tile["patch_ceiling_surplus"] = 0.90
	tile["patch_ceiling_deplete"] = 1.35
	tile["patch_ceiling_eradicate"] = 2.10
	tile["patch_ceiling_cultivate"] = 0.06
	tile["patch_ceiling_sow"] = 0.02
	# The species-BLIND patch payoffs. A crop the player picks substitutes its own (Hay Grass pays 0.72
	# fodder at rung 2, 1.80 at rung 3), so these are what a COMMITTED patch quotes.
	tile["patch_tended_yield"] = 0.30
	tile["patch_tended_fodder"] = 0.72
	tile["patch_field_yield"] = 0.60
	tile["patch_field_fodder"] = 1.80
	# **THE FODDER ACCOUNT IS THE PATCH'S OWN RATE, stated directly.** `BaseFx.seed_forage_rows` derives
	# each account's per-biomass rate from the food-peak ceiling the fixture names, which is the right
	# reversal for a food account; the non-food one is an independent fact about what GROWS here, so it
	# is authored as the rate the wire actually carries and the seeder is told the peak ceiling it
	# stands for. A patch's per-worker term for it is NOT on the wire at all — the client composes it
	# from the one biomass throughput both accounts share — so there is nothing per-account left to
	# author here.
	tile["patch_fodder_per_biomass"] = HAY_MEADOW_FODDER_PER_BIOMASS
	tile = BaseFx.seed_forage_rows(tile)
	return tile

## **THE TILE FROM THE REPORT: Tobacco 56% + Hay Grass 44%, and it pays NO FOOD AT ALL.** Tobacco pays
## its own material; hay pays fodder and fibre. Neither pays a calorie, which is a perfectly ordinary
## thing for a wild patch to be and which nothing else in this harness stages.
##
## **IT IS THE CONTRAST TO `_dead_season_tile_fixture`, and the pair is the whole claim.** Both answer
## `0` on the food axis; one is barren and one is a stand of two crops the band actually wants. The cap
## read `max 1 worker useful here` on BOTH — `max_useful_workers` divided by the food term alone, and
## once arc #527 made the axis triple a plain alias of that term, the widening that used to rescue an
## inedible source was gone. So the sheet printed that 1 beneath its own `13 clear it now` and
## `2 hold it after`, with the `+` dead at 1.
##
## **THE ACCOUNTS ARE AUTHORED AS THE WIRE STATES THEM** — a per-biomass rate per account, and a
## per-worker term that is the crew's biomass throughput through that same rate. Holding that relation
## is what makes every account's saturating crew agree on a WILD patch, which is the property the cap's
## off-axis arm leans on; a fixture that broke it would be testing arithmetic no sim produces.
func _no_food_basket_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["x"] = 65
	tile["y"] = 10
	# **THE FOOD ACCOUNT IS A STRUCTURAL ZERO, NOT A SEASONAL ONE** — these two plants pay no calories
	# at any stock, so the rate goes to zero while the stock and the throughput stay real. That is
	# exactly the opposite of the dead season, which keeps its rates and loses its crop.
	tile["patch_provisions_per_biomass"] = 0.0
	tile["patch_per_worker_yield"] = 0.0
	# The two rungs' food payoffs go with it: committing to tobacco does not make it edible.
	tile["patch_tended_yield"] = 0.0
	tile["patch_field_yield"] = 0.0
	# **STATED, NOT SEEDED.** `ForageFx.seed_growth_terms` recovers the throughput from the food pair
	# where it can and falls back to a config number where it cannot — which is here — so the term the
	# whole crew side divides by would be an implementation detail of the seeder rather than a fact
	# this frame authored.
	tile["patch_per_worker_biomass"] = NO_FOOD_BASKET_CARRY
	tile["patch_fodder_per_biomass"] = NO_FOOD_BASKET_FODDER_PER_BIOMASS
	tile["patch_material_per_biomass"] = [
		{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": NO_FOOD_BASKET_TOBACCO_PER_BIOMASS},
		{"material_id": NO_FOOD_BASKET_FIBRE_ID, "amount": NO_FOOD_BASKET_FIBRE_PER_BIOMASS},
	]
	tile["patch_per_worker_material"] = [
		{"material_id": NO_FOOD_BASKET_TOBACCO_ID,
			"amount": NO_FOOD_BASKET_CARRY * NO_FOOD_BASKET_TOBACCO_PER_BIOMASS},
		{"material_id": NO_FOOD_BASKET_FIBRE_ID,
			"amount": NO_FOOD_BASKET_CARRY * NO_FOOD_BASKET_FIBRE_PER_BIOMASS},
	]
	tile["patch_composition"] = [
		{"species": "tobacco", "role": "cash", "display_name": "Tobacco", "share": 0.56,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [
				{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": 0.31}],
			"sow_material_payoff": [{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": 0.78}]},
		{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.44,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.72, "sow_fodder_payoff": 1.80,
			"cultivate_material_payoff": [{"material_id": NO_FOOD_BASKET_FIBRE_ID, "amount": 0.09}],
			"sow_material_payoff": [{"material_id": NO_FOOD_BASKET_FIBRE_ID, "amount": 0.22}]},
	]
	return tile

## **THE COMPLETED CASH-CROP FIELD FROM THE 2026-08-22 REPORT — a 100% tobacco Field with two tenders
## on it, reading `TENDERS 0` and `max 0 workers useful here`.**
##
## It is the same basket one rung up: the wire's `is_field`, a commitment to the crop, and a
## composition the Sow has narrowed to that one plant. Every account rate is the no-food basket's, so
## the FOOD term is a structural zero and the tobacco is the only thing this ground pays.
##
## **THE PATCH IS DRAWN DOWN LIKE ANY OTHER STAND, which is what the client had stopped believing.**
## `forage.rs`'s retired rung-3 managed harvest says so in as many words — *"A Field is now foraged
## through the ordinary `forage_take` path exactly as a tended patch and a wild stand are"* — so the
## fixture states a real stock above a real floor and expects an escapement composition, not a payoff
## figure.
func _cash_crop_field_tile_fixture() -> Dictionary:
	var tile := _no_food_basket_tile_fixture()
	tile["x"] = 64
	tile["y"] = 11
	tile["patch_is_field"] = true
	tile["patch_is_cultivated"] = true
	tile["patch_committed_species"] = FIELD_CROP_SPECIES
	tile["patch_committed_display_name"] = FIELD_CROP_NAME
	# **BOTH METERS ARE FULL, and that is the frame rather than tidiness.** The chapter's base tile
	# carries a part-built Cultivate, and `build_verb` reads a meter — so a Field inheriting one reads
	# as a patch still being tended, which is a different state with a different readout. This frame is
	# the rung DONE.
	tile["patch_cultivation_progress"] = 1.0
	tile["patch_field_progress"] = 1.0
	tile["patch_cultivation_work_done"] = float(tile.get("patch_cultivation_work_cost", 0.0))
	tile["patch_field_work_done"] = float(tile.get("patch_field_work_cost", 0.0))
	# **A SOWN FIELD IS 100% ITS CROP** (#433), so the basket narrows with the commitment and the
	# fodder the wild meadow paid goes with the hay grass that paid it.
	tile["patch_fodder_per_biomass"] = 0.0
	tile["patch_material_per_biomass"] = [
		{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": NO_FOOD_BASKET_TOBACCO_PER_BIOMASS}]
	tile["patch_per_worker_material"] = [{"material_id": NO_FOOD_BASKET_TOBACCO_ID,
		"amount": NO_FOOD_BASKET_CARRY * NO_FOOD_BASKET_TOBACCO_PER_BIOMASS}]
	tile["patch_composition"] = [
		{"species": FIELD_CROP_SPECIES, "role": "cash", "display_name": FIELD_CROP_NAME,
			"share": 1.0, "can_cultivate": true, "can_sow": true,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"cultivate_fodder_payoff": 0.0, "sow_fodder_payoff": 0.0,
			"cultivate_material_payoff": [
				{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": 0.31}],
			"sow_material_payoff": [{"material_id": NO_FOOD_BASKET_TOBACCO_ID, "amount": 0.78}]},
	]
	return RungFx.stamp_patch(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)

## The crop the Field is committed to, and the crew standing on it. **TWO, not one**: the reported
## defect staged the cap at `0`, and a floor asserted against a crew of one cannot tell a cap that
## reaches the standing crew from `MAX_USEFUL_BARREN`'s own one worker.
const FIELD_CROP_SPECIES := "tobacco"
const FIELD_CROP_NAME := "Tobacco"
const FIELD_TENDERS := 2

## …and the band working it, with no hands to spare. The pool the stepper is capped against is
## `idle + this source's own crew`, so a band with idle workers would let a labor-bound cap stand in
## for the usefulness one and the claim would be about the wrong ceiling.
func _cash_crop_field_band_fixture() -> Dictionary:
	var tile := _cash_crop_field_tile_fixture()
	var band: Dictionary = BandFx.forage_range_bands()[0]
	band["working_age"] = FIELD_TENDERS
	band["idle_workers"] = 0
	band["labor_assignments"] = [{
		"kind": "forage", "workers": FIELD_TENDERS,
		"target_x": int(tile["x"]), "target_y": int(tile["y"]),
		"floor": SourceForecast.FLOOR_FOOD_PEAK,
		"improvement": "", "species": FIELD_CROP_SPECIES,
		"actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0,
		"overdraws": false,
	}]
	return band

## **A DESCRIBED PATCH THAT PAYS NOTHING — the state issue #426 is named after.** Deep winter on the
## same meadow: the wire carries a full per-policy row for every rung and every cell in it is zero.
##
## This is NOT `_barren_tile_fixture`, and the difference is the whole issue: that tile has no food
## module, so there is no patch to forecast and the sheet correctly shows no compose block at all.
## Here there IS a patch, the sim HAS answered, and the answer is "nothing this season". The forecast
## must therefore read as KNOWN — the sheet stays loud, states the zeros, and keeps the worker cap
## live at `MAX_USEFUL_BARREN` — rather than falling through the "the wire said nothing" branch, which
## went silent and switched the cap off entirely.
func _dead_season_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["x"] = 66
	tile["y"] = 9
	tile["patch_ecology_phase"] = "collapsing"
	tile["patch_biomass"] = 0.0
	# Nothing grows, so nothing is worth committing to either — the investment rungs' payoffs go with
	# the harvest. The basket stays: which plants LIVE here is not a seasonal fact.
	tile["patch_tended_yield"] = 0.0
	tile["patch_tended_fodder"] = 0.0
	tile["patch_field_yield"] = 0.0
	tile["patch_field_fodder"] = 0.0
	tile["patch_per_worker_yield"] = 0.0
	# **THE CREW THROUGHPUT IS HONESTLY ZERO, AND IT IS STATED RATHER THAN SEEDED.** The wire's
	# `perWorkerBiomass` folds in the tile's seasonal weight, so a dead season really does move no
	# biomass per gatherer — and this is the one fixture that must say so, because it is the case the
	# panel's crew arithmetic must not divide by. `ForageFx.seed_growth_terms` would otherwise fall back to the
	# config's throughput here, since a zero food rate makes its exact recovery unavailable.
	tile["patch_per_worker_biomass"] = 0.0
	for policy in ["sustain", "surplus", "deplete", "eradicate", "cultivate", "sow"]:
		tile["patch_ceiling_%s" % policy] = 0.0
	tile = BaseFx.seed_forage_rows(tile)
	return tile

## THE SAME MEADOW, COMMITTED TO ITS HAY — the half that pins the COMMITMENT arm. The sim pays the
## fodder of a patch committed to a fodder-bearing crop whatever the faction knows (committing to hay
## IS the bid), so without this state the whole set would pass as "gated on knowledge alone". Same
## ground, same rates, same coordinates: only the commitment moves, which is what makes it a
## controlled comparison with the locked frame.
##
## `hay_grass` is a member of this basket, and the display name rides with it because a committed
## species with no display name is a shape the wire never ships (the crop picker's locked readout
## reads both).
func _committed_hay_meadow_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["patch_committed_species"] = "hay_grass"
	tile["patch_committed_display_name"] = "Hay Grass"
	return tile

## **THE SAME MEADOW, COMMITTED TO ITS GRAIN — THE REPORTED CASE, and the one the commitment arm used
## to get wrong.** `wild_emmer` is 70% of this basket and pays `0.0` fodder at BOTH rungs; `hay_grass`
## is the other 30% and pays 0.72 / 1.80. A player with no pens and no Foddering, whose one cultivated
## patch is the grain, banked hay off the volunteers standing beside it — hay nothing in that game
## could eat — because the bid was read as a commitment to ANYTHING.
##
## **IT IS THE CONTROLLED TWIN OF `_committed_hay_meadow_tile_fixture`**: same ground, same rates, same
## coordinates, same basket, and the committed SPECIES is the only thing that moves. That is what makes
## the pair say something about the species test rather than about the patch.
func _committed_grain_meadow_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["patch_committed_species"] = GRAIN_CROP_SPECIES
	tile["patch_committed_display_name"] = GRAIN_CROP_NAME
	return tile

## The grain in that basket, spelled once. It is the fixture's own key rather than a literal at the
## use site because the assertion below turns on this species having `cultivate_fodder_payoff` and
## `sow_fodder_payoff` BOTH at zero in `ForageFx.fodder_basket_tile_fixture` — the two the client's
## test reads.
const GRAIN_CROP_SPECIES := "wild_emmer"
const GRAIN_CROP_NAME := "Wild Emmer"

## ---- THE FODDER-ONLY WORKED SOURCE (issue #449) ------------------------------------------------
## What the sim pays this crew per turn. It is the ONLY account the assignment carries, which is the
## whole point: a sown hay Field publishes zero provisions, and the compact readouts
## rendered that as `+0.00 /turn` — a tile that reads as dead while it fills the band's fodder store.
const FODDER_STANDING_RATE := 0.40
## The crew on it. Three rather than one, so the summary's `N foragers` is plural and the row cannot be
## mistaken for the single-worker degenerate case.
const FODDER_STANDING_FORAGERS := 3
## The readout's whole `label_suffix`, spelled out rather than composed through `yield_components` —
## a needle built by the code under test agrees with whatever that code emits. The LEADING SPACE is the
## readout's own (it joins the suffix to the crew clause).
##
## **THE MAGNITUDE IS SIGNED, and it read `0.40 fodder` until the work board put a source's accounts on
## a line of their own.** `yield_components` signed its FOOD arm and no other, so one list mixed
## `+0.20 /turn` with a bare `0.40 fodder` — a difference a reader can only take as meaningful. Every
## account on that line is per-turn income and every one of them carries the sign that says so; the
## tooltip clause below had been signing this same reading all along.
const FODDER_STANDING_SUFFIX := " +0.40 fodder"
## The same clause as the rendered drawer line carries it.
const FODDER_STANDING_CLAUSE := "+0.40 fodder"
## The tooltip's fodder clause, which reuses the rung tooltips' own wording rather than spelling the
## account a third way — hence the `/turn` unit the compact face does without.
const FODDER_STANDING_TOOLTIP_CLAUSE := "+0.40 fodder/turn"

## THE ASSIGNMENT ITSELF, so the band fixture and the readout assertion cannot describe two different
## crews. `has_yield` is the one key `source_yield_readout` reads that is not on the wire assignment;
## the drawer derives it from `actual_yield`'s presence, so the zero has to be PRESENT rather than
## absent — a source that produced no food is not a source the wire never described.
func _fodder_field_assignment() -> Dictionary:
	return {
		"kind": SourceForecast.LABOR_KIND_FORAGE, "workers": FODDER_STANDING_FORAGERS,
		"workers_needed": FODDER_STANDING_FORAGERS,
		"target_x": int(_hay_meadow_tile_fixture()["x"]), "target_y": int(_hay_meadow_tile_fixture()["y"]),
		"floor": SourceForecast.FLOOR_FOOD_PEAK,
		"actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0,
		"fodder_yield": FODDER_STANDING_RATE,
		"overdraws": false, "has_yield": true,
	}

## The player band WORKING that Field, and nothing else — so the drawer's standing summary on the hay
## ---- THE REOPENED SHEET, on a band with NOTHING SPARE -----------------------------------------
## **THE STATE THAT WAS UNREACHABLE UNTIL THE BUILD CREW SHIPPED** (`docs/plan_standing_upkeep.md`
## §2.2). Every one of this band's hands is committed — `idle_workers` is **0** — and they are
## committed to THIS patch: gatherers and builders. Until the wire published the second, the sheet
## could only clamp that stepper at `idle`, so it opened at nobody with a maximum of nobody: the
## player could take the build crew to zero and could never put it back.
##
## **THE KEEPING IS NOT ONE OF THEM ANY MORE** (§2.5). Maintenance left the tile, so the band's third
## commitment is its `agriculture` ROLE and this sheet composes no keeping crew at all — which this
## state also asserts, an absence being the half a frame cannot show on its own.
##
## The two crew counts are deliberately DISTINCT, so a seed that read the wrong field cannot pass by
## coincidence, and their sum with the keeping role is the whole working-age pool — which is what
## makes `idle_workers = 0` a fact about the fixture rather than a number typed beside it.
const REOPEN_TAKE_CREW := 4

const REOPEN_BUILD_CREW := 3

## The hands this band has on its `agriculture` role — off the patch entirely, and part of why it has
## nothing idle. The patch's keeping is paid out of that pool rather than by a crew standing on it.
const REOPEN_KEEP_CREW := 2

## What the patch's keeping is worth in hands. The pool pays it `REOPEN_KEEP_CREW`, so the patch is
## short — the honest under-funded state, and the one the land card's `At risk:` row is about.
const REOPEN_UPKEEP_WANTS := 3

## The crop this band asked for, stated on the ASSIGNMENT. **The patch carries no `committed_species`**
## — no crew has finished a turn on this ground — so this is the only place the selection exists, and a
## sheet that re-resolved to the tile's dominant plant would silently re-point a 25-turn commitment.
## **The SPECIES KEY, never the display name** — the wire's own identity, which is what the picker
## marks its committed row by. `flax` is deliberately NOT this basket's highest share (`wild_tubers`,
## 0.65), so it is a choice the default resolver would overwrite: a seed that lost it lands on a
## different plant and this assertion names which.
const REOPEN_SPECIES := "flax"

func _reopened_patch_tile_fixture() -> Dictionary:
	var tile := _building_patch_tile_fixture()
	# The patch's keeping as a SHARE of the band's agriculture pool: the demand is what the land card
	# states and the shortfall is what the sim publishes against it.
	tile["patch_upkeep_demand"] = float(REOPEN_UPKEEP_WANTS)
	tile["patch_upkeep_supplied"] = float(REOPEN_KEEP_CREW)
	tile["patch_upkeep_shortfall"] = float(REOPEN_UPKEEP_WANTS - REOPEN_KEEP_CREW)
	tile["patch_upkeep_workers_needed"] = REOPEN_UPKEEP_WANTS
	tile["patch_has_neglect_grace"] = true
	tile["patch_neglect_grace_remaining"] = 1
	# **NO `committed_species`** — the ground is not committed to anything yet, which is precisely why
	# the assignment's own `species` has to be what the crop picker opens on.
	tile["patch_committed_species"] = ""
	tile["patch_committed_display_name"] = ""
	return tile

## The band that has staffed both of that patch's crews AND its agriculture role, with nothing left
## over. The keeping role is an ORDINARY ROW of the same list (`docs/plan_standing_upkeep.md` §2.5),
## exactly as scout and warrior are — which is why the sheet has no keeping crew to seed.
func _reopened_patch_band_fixture() -> Dictionary:
	var band := _building_patch_band_fixture(0.0)
	band["working_age"] = REOPEN_TAKE_CREW + REOPEN_BUILD_CREW + REOPEN_KEEP_CREW
	band["idle_workers"] = 0
	band["labor_assignments"] = [{
		"kind": "forage", "workers": REOPEN_TAKE_CREW,
		"target_x": 68, "target_y": 12, "floor": BUILD_DIP_FLOOR,
		"improvement": "cultivate",
		"species": REOPEN_SPECIES,
		"actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0,
		"overdraws": false,
	}, {"kind": "agriculture", "workers": REOPEN_KEEP_CREW},
		# **THE BUILD POOL IS A ROW OF THIS SAME LIST** (`docs/plan_standing_upkeep.md` §2.5) — a
		# standing role like the keeping one beside it, not a second count on the forage assignment.
		BandFx.builders_role_row(REOPEN_BUILD_CREW)]
	return band

## meadow is the frame's whole subject.
func _fodder_field_band_fixture() -> Dictionary:
	var band: Dictionary = BandFx.forage_range_bands()[0]
	band["labor_assignments"] = [_fodder_field_assignment()]
	return band

# ---- THE ROW'S TWO RATES, AND WHY EVERY OTHER FIXTURE IS BLIND TO THEM -------------------------
# **THE PLAYTEST NUMBERS.** `realizedYield` is a 40-turn forward projection of this source's take and
# `actualYield` is the take the sim resolved this turn; a patch sitting at its MSY inflection reads
# `+1.96` against `+1.91`, which is the width of the projection window and NOT an error. The row's
# face is the first and its hover carried the second under the word *Actual*, which asserted the face
# was something other than actual while naming neither quantity.
#
# **EVERY OTHER FIXTURE IN BOTH HARNESSES SETS THE TWO EQUAL** — measured, every `realized_yield`
# literal in `ui_preview` and `band_panel_preview` matches its own `actual_yield` — which is exactly
# why the word survived: on every canned row the tooltip's number WAS the face's number, so no frame
# and no assertion could tell a labelled pair from an unlabelled one. These two must DIFFER, or this
# whole block passes with the defect fully restored.
const READOUT_REALIZED_RATE := 1.96

const READOUT_ACTUAL_RATE := 1.91

## What the hunt half sustains — deliberately unlike either rate above, so a tooltip that quoted the
## sustainable figure in one of the two named slots lands on a number this block can name.
const READOUT_SUSTAINABLE_RATE := 1.20

## The retired word, as a LITERAL. It is spelled here rather than read off a constant because the
## constant is gone: the claim is that this string appears nowhere in a rendered readout, and a needle
## composed from the code under test could not make it.
const RETIRED_ACTUAL_NEEDLE := "Actual"

## One worked row in the shape `source_yield_readout` reads, with the two rates SPREAD. `has_yield` is
## the one key the readout needs that is not on a wire assignment, which is the same local seeding the
## drawer's own standing summary does.
func _spread_rate_assignment(kind: String, overdraws: bool) -> Dictionary:
	return {
		"kind": kind, "workers": 2, "workers_needed": 2, "has_yield": true,
		"actual_yield": READOUT_ACTUAL_RATE,
		"sustainable_yield": READOUT_SUSTAINABLE_RATE,
		"realized_yield": READOUT_REALIZED_RATE,
		"overdraws": overdraws, "wasted_yield": 0.0,
	}

## **THE ROW AND ITS HOVER ARE ONE CALL'S TWO KEYS, AND THE HOVER MUST NAME BOTH QUANTITIES.**
## PNG-LESS AND DRIVEN, because a tooltip is not in any picture and because both readings render a
## perfectly ordinary hover — `Actual +1.91` beside a face of `+1.96` is plausible, correctly
## formatted, and wrong only in the word.
func _assert_readout_names_both_rates() -> void:
	var forage := SourceForecast.source_yield_readout(
		_spread_rate_assignment(SourceForecast.LABOR_KIND_FORAGE, false),
		SourceForecast.LABOR_KIND_FORAGE)
	# (0) THE FIXTURE REALLY IS THE REGIME. Every other canned row in this harness collapses the two
	# onto one number, and on such a row every claim below passes with the word restored.
	h._assert_hud("the fixture's two published rates really do differ (%s vs %s)"
			% [SourceForecast.format_yield(READOUT_REALIZED_RATE),
				SourceForecast.format_yield(READOUT_ACTUAL_RATE)],
		not is_equal_approx(READOUT_REALIZED_RATE, READOUT_ACTUAL_RATE))
	# (1) THE FACE IS THE PROJECTION, which is what makes the hover's first slot the face's own number.
	h._assert_hud("the row's face is the forward-projected average (%s)"
			% SourceForecast.format_yield(float(forage["rate"])),
		is_equal_approx(float(forage["rate"]), READOUT_REALIZED_RATE))
	# (2) EQUALITY, never `contains`: half the claim is the LABELLING, and a containment test on either
	# number passes on a line that quotes it anonymously.
	var forage_tooltip := SourceForecast.YIELD_TOOLTIP_RATES_FORMAT % [
		SourceForecast.format_signed(READOUT_REALIZED_RATE),
		SourceForecast.format_signed(READOUT_ACTUAL_RATE)] + SourceForecast.YIELD_TOOLTIP_RENEWABLE
	h._assert_hud("…and its hover names BOTH quantities — \"%s\"" % String(forage["tooltip"]),
		String(forage["tooltip"]) == forage_tooltip)
	# (3) THE OTHER BRANCH, because the renewable one appends nothing after the pair and an overdrawing
	# row appends two more clauses — so a producer that composed the pair only on the quiet path fails
	# here rather than above.
	var hunt := SourceForecast.source_yield_readout(
		_spread_rate_assignment(SourceForecast.LABOR_KIND_HUNT, true),
		SourceForecast.LABOR_KIND_HUNT)
	var hunt_tooltip := SourceForecast.YIELD_TOOLTIP_RATES_FORMAT % [
		SourceForecast.format_signed(READOUT_REALIZED_RATE),
		SourceForecast.format_signed(READOUT_ACTUAL_RATE)] \
		+ " · Sustainable %s" % SourceForecast.format_yield(READOUT_SUSTAINABLE_RATE) \
		+ SourceForecast.YIELD_TOOLTIP_OVERDRAW
	h._assert_hud("an overdrawing hunt row names the same pair ahead of its own clauses — \"%s\""
			% String(hunt["tooltip"]),
		String(hunt["tooltip"]) == hunt_tooltip)
	# (4) AND THE WORD IS GONE, asserted on BOTH webs by literal needle. It is the negative that names
	# the defect: the equalities above would also pass on a line that had merely been reworded around
	# it, and this one would not.
	h._assert_hud("…and the word `%s` appears in neither hover" % RETIRED_ACTUAL_NEEDLE,
		not String(forage["tooltip"]).contains(RETIRED_ACTUAL_NEEDLE)
			and not String(hunt["tooltip"]).contains(RETIRED_ACTUAL_NEEDLE))

func run(harness) -> void:
	h = harness

	# State forage_stale_verb — **THE TWO PUBLISHED NUMBERS MUST IMPLY ONE THROUGHPUT.** The state above
	# proved the finished patch stops OFFERING Cultivate; this one proves it stops being PRICED as one.
	# Reported from play: a tended patch reading `Forage biomass 111 / 195` with `2 foragers · +0.41
	# /turn` on the card, and a sheet beside it asking for **6 hold it after** — a crew that can only be
	# right if a forager carries ~2 biomass, while the sim's own rate for the crew already working it
	# says ~6. Nothing on screen could explain the gap: the improvement control read `🌾 Tended Patch`,
	# a DONE label, so no build was visibly in flight. The stale `Cultivate` in the compose state was.
	#
	# `seed_forage` only runs when the SOURCE changes, so a composition outlives the build it named —
	# and the sim clears the assignment's `improvement` the turn the rung completes, which is precisely
	# when the two halves of the panel start dividing by different throughputs. Staged the way play
	# reaches it: open the sheet (seeding crew + floor off the standing assignment, improvement ""),
	# then dial in the verb the finished build left behind and re-open.
	var stale_tile := ForageFx.floorify(_stale_verb_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_carry := SourceForecast.per_worker_biomass(stale_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_samples := SourceForecast.regrowth_samples(stale_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var stale_growth := SourceForecast.regrowth_at(stale_samples, ForageFx.STALE_VERB_FLOOR)
	# **THE CARD'S NUMBER, COMPOSED THE WAY THE SIM COMPOSES IT** — regrow, then take what stands above
	# the floor, capped by what the crew can carry (`forage_take`'s `min(worker_cap, ceiling)`). Derived
	# from the tile's own wire terms rather than written as a literal, so the standing rate and the crew
	# targets are answering about the SAME patch by construction and this assertion cannot be satisfied
	# by a fixture that drifted.
	var stale_standing_rate := minf(float(ForageFx.STALE_VERB_CREW) * stale_carry, stale_growth) \
		* ForageFx.STALE_VERB_FOOD_PER_BIOMASS
	# Captured rather than restored to a named fixture: the band in force here is whatever the state
	# before this one left, and re-seeding it from a guess is how a later state's crew quietly moves.
	var prior_player_band = h._hud._band_labor.player_band()
	var prior_player_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _stale_verb_band_fixture(stale_standing_rate)
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._show_tile(stale_tile)
	h._compose_forage(stale_tile)
	h._hud._compose.set_forage_improvement("cultivate")
	h._compose_forage(stale_tile)
	await h._settle()
	await h._save("forage_stale_verb")
	var stale_sheet = h._hud._drawercompose._compose_sheet
	var stale_hold = Readout.crew_target_count(stale_sheet, HudWidgets.CREW_TARGET_HOLD)
	# (1) THE CREW TARGETS DIVIDE BY THE THROUGHPUT THE WIRE PUBLISHED. Compared against the crew terms
	# recomposed here from the source's own fields at NO dip — the answer a patch with nothing left to
	# build must give. With the stale verb pricing the crew this reads 6 against 2.
	h._assert_hud("a finished rung's verb dips no crew — HOLD divides by the wire's own throughput (%d)"
		% stale_hold,
		stale_hold == SourceForecast.crew_to_hold(stale_samples, ForageFx.STALE_VERB_FLOOR, stale_carry,
			PLANT_NO_BODY, SourceForecast.NO_ENGAGEMENT_STAGE,
			SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF))
	h._assert_hud("…and so does CLEAR, the other half of the same division",
		Readout.crew_target_count(stale_sheet, HudWidgets.CREW_TARGET_CLEAR)
			== SourceForecast.crew_to_clear(SourceForecast.escapement_room(stale_tile,
				HudComposeVocab.FORAGE_FORECAST_PREFIX, ForageFx.STALE_VERB_FLOOR), stale_carry,
				SourceForecast.crew_that_reaches(stale_samples, ForageFx.STALE_VERB_STOCK,
					ForageFx.STALE_VERB_CAPACITY, ForageFx.STALE_VERB_FLOOR, stale_carry,
					PLANT_NO_BODY, SourceForecast.NO_ENGAGEMENT_STAGE),
				PLANT_NO_BODY, SourceForecast.NO_ENGAGEMENT_STAGE))
	# (2) **THE INVARIANT THAT BROKE** — the sheet's crew target and the card's rate must imply the SAME
	# biomass per forager. The card's is a LOWER bound (its take may be bound by the room rather than by
	# the crew), so a crew target may never price a forager BELOW it: that is exactly the contradiction
	# played — 12.3 biomass moved by 2 foragers, beside a target saying a forager carries 2.
	var stale_from_card := (stale_standing_rate / ForageFx.STALE_VERB_FOOD_PER_BIOMASS) / float(ForageFx.STALE_VERB_CREW)
	var stale_from_hold = stale_growth / float(maxi(stale_hold, 1))
	h._assert_hud("the card's rate and the sheet's crew imply ONE throughput (%.2f vs %.2f biomass/forager)"
		% [stale_from_card, stale_from_hold],
		stale_hold > 0 and stale_from_hold >= stale_from_card - STALE_VERB_THROUGHPUT_EPSILON)
	# (3) …and the frame really is a FINISHED patch rather than a build in flight, which is what makes
	# the two assertions above claims about a STALE verb rather than about a legitimate dip. A RUNNING
	# Cultivate reads its own RUNNING label; this one is the DONE state's, naming the rung the patch is
	# standing on — which the face's own words are what separate, both states being Labels now.
	var stale_control = ForageFx.find_improvement_control(stale_sheet, "cultivate")
	h._assert_hud("the finished rung reads as a DONE label, so no build in flight can explain a dip",
		String(ForageFx.improvement_state(stale_sheet, "cultivate"))
				== HudWidgets.IMPROVEMENT_STATE_DONE
			and ForageFx.improvement_face(stale_sheet, "cultivate").contains(
				String(HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"])))
	h._hud._band_labor._player_band = prior_player_band
	h._hud._band_labor._player_bands = prior_player_bands
	h._hud._compose.reset_forage_source()   # the states after this one open on their own patch

	# ---- THE BUILDING PATCH: WHEN THE REGROWTH BEATS THE ROOM ------------------------------------
	# **THE FRAME THREE DEFECTS SHARE, and no other fixture reaches it.** Reported from play: a patch
	# at `K 195` with ~9 biomass standing above its floor and ~12 growing back every turn, worked by
	# six foragers at a live Cultivate's quarter carry. It rendered `5 clear it now` · `6 hold it
	# after` · `⚠ OVERDRAWS THE PATCH` over a verdict reading *this crew can't draw it that low. It
	# settles at 54% and holds there — 7 foragers would reach the floor.* Four numbers, no two of which
	# agree, and every one of them individually correct arithmetic.
	# THE ARITHMETIC WAS NOT THE DEFECT — the numbers contradicting each other was.
	var building_tile := ForageFx.floorify(_building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var build_samples := SourceForecast.regrowth_samples(building_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	# The crew term the sheet divides by, recomposed HERE from the tile's own wire fields — the carry
	# and the rung's dip, exactly as `floor_chart_model` composes it. Every relation below is stated
	# against it rather than against a literal, so a fixture that drifts fails instead of re-baselining.
	var build_carry := SourceForecast.per_worker_biomass(building_tile,
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var build_reaching := SourceForecast.crew_that_reaches(build_samples, BUILD_DIP_STOCK,
		BUILD_DIP_CAPACITY, BUILD_DIP_FLOOR, build_carry, PLANT_NO_BODY,
		SourceForecast.NO_ENGAGEMENT_STAGE)
	# THE CARD'S STANDING RATE, composed the way the sim composes it (`forage_take`'s `min(crew carry,
	# ceiling)` through the patch's food rate) — derived from the tile's own wire terms rather than
	# written down, so the card and the sheet cannot drift apart by fixture edit.
	var build_standing_rate := minf(float(BUILD_DIP_CREW) * build_carry,
		SourceForecast.escapement_room(building_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			BUILD_DIP_FLOOR)) * ForageFx.STALE_VERB_FOOD_PER_BIOMASS
	var prior_build_band = h._hud._band_labor.player_band()
	var prior_build_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _building_patch_band_fixture(build_standing_rate)
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._show_tile(building_tile)
	h._compose_forage(building_tile)
	h._hud._compose.set_forage_floor(BUILD_DIP_FLOOR)
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_count(BUILD_DIP_CREW)
	h._compose_forage(building_tile)
	await h._settle()
	await h._save("forage_build_crew")
	h._assert_compose_sheet_fits("forage_build_crew")
	var build_sheet = h._hud._drawercompose._compose_sheet
	# Captured while a build IS composed, so the no-build frame below can compare against it rather
	# than against a recomposition of the same oracle.
	var build_yields_before := Readout.yields_text(build_sheet)
	var build_clear = Readout.crew_target_count(build_sheet, HudWidgets.CREW_TARGET_CLEAR)
	# (0) THE FRAME REALLY IS THE REGIME. Without this every assertion below is about an ordinary
	# patch: the whole point is a crew that CANNOT out-take the regrowth, so the crew that can must be
	# strictly larger than the one-turn quotient the target used to state.
	var build_quotient := SourceForecast.crew_to_clear(SourceForecast.escapement_room(
		building_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX, BUILD_DIP_FLOOR), build_carry, 0,
		PLANT_NO_BODY, SourceForecast.NO_ENGAGEMENT_STAGE)
	h._assert_hud("the fixture reaches the regime — the reaching crew (%d) exceeds the one-turn quotient (%d)"
		% [build_reaching, build_quotient], build_reaching > build_quotient)
	# (1) **THE INVARIANT, stated as a RELATION between the two rendered numbers** rather than as the
	# pair of literals it happens to produce: a target offering to *clear it now* may never name fewer
	# hands than the verdict beside it names as merely REACHING the floor. Those five foragers cleared
	# nothing in any number of turns.
	h._assert_hud("clear-it-now (%d) is never below the crew the verdict names as reaching the floor (%d)"
		% [build_clear, build_reaching],
		build_clear >= build_reaching and build_reaching > 0)
	# (2) …AND THE STEPPER CAN REACH IT (§7.6). Flooring the target without flooring the cap trades one
	# contradiction for another — a pill naming a crew the `+` refuses. Driven through the REAL button,
	# because the clamp lives in the press handler and not in the arithmetic.
	Q.find_crew_target(build_sheet, HudWidgets.CREW_TARGET_CLEAR).pressed.emit()
	h._assert_hud("…and the stepper reaches that crew rather than clamping it to a smaller cap",
		h._hud._compose.forage_count() == build_clear)
	h._hud._compose.set_forage_count(BUILD_DIP_CREW)
	h._compose_forage(building_tile)
	# (3) **THE ⚠ IS THE SIM'S, AND THIS PATCH IS WORKED BY NOBODY.** The sheet used to derive the flag
	# here — a take against the food-peak ceiling (zero on a patch standing at the peak, so the test
	# degenerated into "the floor is below 0.5") gated on a client-side drawdown walk. Both are gone:
	# `LaborAssignment.overdraws` carries the whole verdict, and a patch with no standing row has no
	# crew for the sim to have answered about, so the composed sheet claims no drawdown at either crew.
	var build_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK, BUILD_DIP_CAPACITY,
		BUILD_DIP_FLOOR, float(BUILD_DIP_CREW) * build_carry)
	h._assert_hud("the projection this crew produces RISES — there is nothing being overdrawn (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(build_walk["settled_fraction"])],
		float(build_walk["settled_fraction"]) > BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	h._assert_hud("…and no overdraw flag fires on a patch nobody works",
		not h._hud._drawercompose._local_forage_preview_bbcode(h._hud._band_labor.player_band(),
			building_tile, BUILD_DIP_FLOOR, BUILD_DIP_CREW, "cultivate").contains(HudStyle.WARN_HEX))
	# (4) **THE BUILD STATES NO CREW AT ALL** (`docs/plan_standing_upkeep.md` §2.5). The retired dip
	# note used to stand on the take crew's row explaining why those foragers carried a quarter; a
	# second stepper stood there for one slice, for the hands doing the build; and a verb names no hands
	# now, so the sheet is back to ONE stepper. Asserted by COUNT — which is what catches a hypothetical
	# build slider re-added under any name — and by the ABSENCE of any carry claim on the take's label.
	h._assert_hud("a live build adds no second stepper — the builders are a band-level role",
		Readout.stepper_count(build_sheet) == Readout.COMPOSE_STEPPERS_PER_SHEET)
	h._assert_hud("…and the take crew's row claims no carry penalty beside its label",
		not Readout.crew_row_label(build_sheet).contains("%"))

	# State forage_build_crew_decline — **THE PROJECTION'S OTHER HALF, one hand apart.** Seven foragers
	# out-carry the patch's fastest regrowth, so the same patch at the same floor genuinely falls to
	# the line. **The ⚠ must NOT come back**, and that is the claim now: the flag answers for the crew
	# the sim resolved, not for the one being dialled, so a projection that has flipped between two
	# renders of an UNWORKED patch may not move it. Sabotage that re-derives the flag fails HERE.
	h._hud._compose.set_forage_count(BUILD_DIP_DECLINE_CREW)
	h._compose_forage(building_tile)
	await h._settle()
	await h._save("forage_build_crew_decline")
	var decline_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK,
		BUILD_DIP_CAPACITY, BUILD_DIP_FLOOR, float(BUILD_DIP_DECLINE_CREW) * build_carry)
	h._assert_hud("one more hand out-carries the regrowth, and the projection FALLS (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(decline_walk["settled_fraction"])],
		float(decline_walk["settled_fraction"]) < BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	h._assert_hud("…and the flag STILL does not fire — the projection is not one of its inputs",
		not h._hud._drawercompose._local_forage_preview_bbcode(h._hud._band_labor.player_band(),
			building_tile, BUILD_DIP_FLOOR, BUILD_DIP_DECLINE_CREW, "cultivate")
			.contains(HudStyle.WARN_HEX))
	h._assert_hud("…and the verdict agrees with it — this crew reaches the floor",
		Readout.verdict_severity(h._hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_OK)

	# State forage_build_crew_none — THE SAME PATCH WITH NO BUILD IN FLIGHT, which is the only way to
	# read the build crew row as a CLAIM: a row that renders on every sheet says nothing. It must be
	# absent here — there is no build to staff — while every take-side number is unmoved, which is the
	# retired dip's absence made visible.
	h._hud._compose.set_forage_improvement(SourceForecast.IMPROVEMENT_NONE)
	h._hud._compose.set_forage_count(BUILD_DIP_CREW)
	h._compose_forage(building_tile)
	await h._settle()
	await h._save("forage_build_crew_none")
	var bare_build_sheet = h._hud._drawercompose._compose_sheet
	# **THE BUILD CREW ROW IS STILL THERE, and that is correct rather than a miss**: the improvement
	# control has dropped to its OFFERED state, and staffing the build is part of accepting the offer.
	# What the pair actually claims is that the TAKE is unmoved — the retired dip's whole absence.
	h._assert_hud("the take is unmoved by the build beside it, which is the retired dip's whole absence",
		Readout.yields_text(bare_build_sheet) == build_yields_before)
	h._hud._band_labor._player_band = prior_build_band
	h._hud._band_labor._player_bands = prior_build_bands
	h._hud._compose.reset_forage_source()   # the states after this one open on their own patch

	# State 6b-sow-done — a COMPLETED Field with a standing Sow selection: ▦ Sow greys with "Already a
	# Field — ♻ Sustain-forage it to harvest", mirroring the finished-patch case one rung up (Cultivate is
	# greyed here too — the ground is both tended AND a Field).
	h._show_tile(ForageFx.field_tile_fixture())
	h._hud._compose.set_forage_improvement("sow")
	h._compose_forage(ForageFx.field_tile_fixture())
	await h._settle()
	await h._save("forage_sow_done")

	# State forage_field_from_wild — **A FIELD SOWN STRAIGHT FROM WILD GROUND**, which the frame above
	# cannot be: its fixture climbs rung by rung, so a Field is also cultivated there and the retire
	# test passes for the wrong reason. `Sow` needs no prior patch, so `cultivation_progress` is 0 and
	# stays 0 — and the client asked "is Cultivate built?" by reading `is_cultivated`, got a truthful
	# false, and OFFERED the lower rung on a finished Field. Reported from play. The sim has never
	# agreed: `forage_rung_already_built` matches `Cultivate => patch.is_managed()`, so the box was
	# live for a build the server treats as already built.
	var wild_sown := _wild_sown_field_tile_fixture()
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_improvement("")
	h._show_tile(wild_sown)
	h._compose_forage(wild_sown)
	await h._settle()
	await h._save("forage_field_from_wild")
	h._assert_hud("the fixture really is the state at issue — rung 3 built on an UNcultivated patch",
		SourceForecast.improvement_is_done(wild_sown, HudComposeVocab.FORAGE_FORECAST_PREFIX,
				SourceForecast.IMPROVEMENT_SOW)
			and not bool(wild_sown["patch_is_cultivated"]))
	h._assert_hud("…so a completed Field retires Cultivate, as the sim's own rung test does",
		SourceForecast.improvement_is_done(wild_sown, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.IMPROVEMENT_CULTIVATE))
	h._assert_hud("…and the sheet offers no Cultivate on it",
		String(ForageFx.improvement_state(h._hud._drawercompose._compose_sheet, "cultivate"))
			!= HudWidgets.IMPROVEMENT_STATE_OFFERED)
	# **THE PAIR THAT STOPS THIS BECOMING "CULTIVATE IS NEVER OFFERED".** A retire test that answered
	# true unconditionally would satisfy every line above; a wild patch with the knowledge in hand must
	# still offer the rung.
	h._assert_hud("…while a WILD patch still offers Cultivate — the rung is retired, not deleted",
		not SourceForecast.improvement_is_done(BaseFx.food_tile_fixture(),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE))
	h._hud._compose.reset_forage_source()

	# ---- THE PLANT WEB'S CREW NOUN IS ONE WORD AT EVERY RUNG --------------------------------------
	# ⛔ **THESE FIVE STATES USED TO ASSERT A FORK AND NOW ASSERT ITS COLLAPSE**
	# (`docs/plan_standing_upkeep.md` §4.9 item 12c). The dead claim, verbatim: *"every surface for a
	# sown Field still said forage / Foragers … a managed source's crew are TENDERS and only a wild
	# stand's are FORAGERS"*, with the two upper rungs expecting `Tenders` and the wild rung and both
	# in-flight builds expecting `Foragers`. Item 12c retired the second word — a Field's sheet read
	# `ASSIGN TENDERS` and then offered the *Gathering* kit, the tending being the Agriculture pool's —
	# so every state below expects `HARVEST_CREW_LABEL`.
	#
	# **THAT IS A WEAKER SET, so `_assert_plant_crew_noun` gained a LIVENESS half**: the resolved noun
	# and its verb must both be NON-EMPTY as well as equal, or a resolver answering `""` would satisfy
	# five states all expecting `""`. What the states still discriminate is the FOUR SURFACES and their
	# agreement on one frame; `_assert_plant_crew_noun_is_rung_blind` below is the collapse itself,
	# asked of the resolver over rungs that used to disagree.
	await _assert_plant_crew_noun("plant_crew_wild", BaseFx.food_tile_fixture(),
		HudComposeVocab.HARVEST_CREW_LABEL)
	await _assert_plant_crew_noun("plant_crew_tended", TileFx.tended_tile_fixture(),
		HudComposeVocab.HARVEST_CREW_LABEL)
	# **BOTH UPPER RUNGS, NOT ONE.** A Tended Patch stands on `plant:tended` and a Field sown from wild
	# ground on `plant:field` — two different rungs, and they were the two the retired fork reached
	# through one at-or-above test (`_wild_sown_field_tile_fixture` is the Field that was never
	# cultivated). They are kept because they are still the two rungs a re-introduced fork would split.
	await _assert_plant_crew_noun("plant_crew_field", _wild_sown_field_tile_fixture(),
		HudComposeVocab.HARVEST_CREW_LABEL)
	# **THE CASE A NAIVE "IS AN IMPROVEMENT COMPOSED?" TEST GOT WRONG.** `_building_patch_tile_fixture`
	# is wild ground with `cultivation_progress` part-way and `is_cultivated` false, and the compose
	# carries the verb — the state in which the retired resolver had to keep the WILD noun.
	await _assert_plant_crew_noun("plant_crew_wild_building", _building_patch_tile_fixture(),
		HudComposeVocab.HARVEST_CREW_LABEL, SourceForecast.IMPROVEMENT_CULTIVATE)
	# …and its Sow twin, on the same wild ground: `Sow` needs no prior patch, so a Sow in flight is the
	# other half of "a build is running here" and must read identically.
	await _assert_plant_crew_noun("plant_crew_wild_sowing", _building_patch_tile_fixture(),
		HudComposeVocab.HARVEST_CREW_LABEL, SourceForecast.IMPROVEMENT_SOW)
	_assert_plant_crew_noun_is_rung_blind()
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_improvement("")

	# ---- ALL THREE ACCOUNTS ON A FORAGE FACE (issue #426, face treatment A) -----------------------
	# **THE FACTION IS GIVEN FODDERING FOR THE WHOLE HAY-MEADOW BLOCK, and it is a fixture repair
	# rather than a convenience.** Every meadow below is a WILD patch, and since #485 a wild patch's
	# fodder credit is refused to a band that has not learned Foddering — so at the ladder's default
	# dial the readout's fodder row would read `—` on exactly the frames that exist to show three live
	# accounts, and `floor_chart_drawn_down`'s `now → after` claim would have one account left to make
	# it on. The lock has its own three states at the end of this chapter, where it is the subject.
	#
	# **Penning goes with it**, because Foddering is what a PEN teaches: a strip reading `Foddering ✔`
	# over an unstarted Penning is a pair the sim cannot produce, and a fixture may not state one.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": 1.0}}])
	# State forage_three_accounts — THE FRAME THIS PASS IS JUDGED ON. Every other forage fixture pays
	# provisions alone, so the picker's multi-account face had no frame at all and a hay meadow was
	# indistinguishable from barren prairie. The extractive rungs must read `0.24 food · 0.40 fodder`
	# and ascend on both. (A trade clause stood between them, non-monotone because `Deplete` alone
	# carried the ×4 market markup; the account and the markup are both retired — arc #527.)
	# **THE PICKER STAYS THREE ABREAST, and this frame is why that is a measurement and not a guess.**
	# A wide-face column ceiling of 2 was built for exactly this face and then refuted here: at three
	# columns the sheet comes out 555px — against the deer hunt picker's long-standing 546 — nothing
	# clips, and 3 + 3 reads better than the 2 + 2 + 2 the ceiling produced. The frame is what a future
	# change to that ceiling has to argue with.
	var hay_meadow := _hay_meadow_tile_fixture()
	h._show_tile(hay_meadow)
	h._compose_forage(hay_meadow)   # settle the source key first (it changed)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_species("")
	h._compose_forage(hay_meadow)
	await h._settle()
	await h._save("forage_three_accounts")
	h._assert_compose_sheet_fits("forage_three_accounts")
	# **THE NUMBERS MOVED TO THE TOOLTIP AND THESE ASSERTIONS FOLLOWED THEM.** The claim is unchanged
	# — a three-account patch states all three, in wire order, and every one rises as the floor drops
	# because they are one stock through three fixed rates. What changed is where a player reads it:
	# the face carries the intent alone now (a preset metric is the ROOM above that floor, a one-off,
	# and it stood in food units directly over a biomass chart), so a face assertion would testify to
	# the wrong surface. The pair below is what proves the move rather than a deletion.
	h._assert_hud("a forage rung names all three accounts, in wire order",
		_policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(HAY_PEAK_TOOLTIP))
	h._assert_hud("every account rises together as the floor drops — one stock, three fixed rates",
		_policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_STRIP).contains(HAY_STRIP_TOOLTIP))
	h._assert_hud("…and the FACE states no number at all, on any preset",
		_policy_rung_metric(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK) == ""
			and _policy_rung_metric(h._hud._drawercompose._compose_sheet,
				SourceForecast.FLOOR_PRESET_STRIP) == "")
	# **THE NEGATIVE HALF OF THE `now → after` READING.** A crew that never reaches the floor never
	# enters the holding state, so promising it a held rate is the same class of lie as the burst
	# wearing `/TURN`.
	#
	# **ASKED AT A FLOOR BELOW THE GROWTH PEAK, AND THAT IS THE WHOLE ASSERTION.** Written first
	# against this frame's own `FLOOR_FOOD_PEAK` sheet, it was VACUOUS: at the peak the floor SITS ON
	# the fastest regrowth, so any crew that can out-carry the regrowth there can also reach it, and
	# `now == after` suppresses the arrow whether or not the gate exists — deleting the gate changed
	# no pixel. Below the peak the crew must cross faster regrowth than it will meet at the floor, so
	# settling short and having a different held rate are finally possible at once. The
	# `ungated != gated` line is what proves this crew WOULD have been shown a second number; the line
	# after it proves it was not; and `reach_crew` above them proves it is the settling crew we mean.
	var rows_key = h._hud._drawercompose.YIELD_MODEL_ROWS
	var settles_crew := 1
	var gated: Dictionary = h._hud._drawercompose._forage_yield_model(h._hud._band_labor.player_band(),
		hay_meadow, FLOOR_CHART_HELD_FLOOR, settles_crew, SourceForecast.IMPROVEMENT_NONE, false)
	var ungated: Dictionary = h._hud._drawercompose._forage_yield_model(h._hud._band_labor.player_band(),
		hay_meadow, FLOOR_CHART_HELD_FLOOR, settles_crew, SourceForecast.IMPROVEMENT_NONE, true)
	h._assert_hud("this crew genuinely settles SHORT of the floor it is being priced against",
		settles_crew < SourceForecast.reach_crew(hay_meadow, SourceForecast.SOURCE_KIND_FORAGE,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, FLOOR_CHART_HELD_FLOOR))
	h._assert_hud("…and genuinely HAS a different held rate, so the gate is what hides it",
		str(ungated.get(rows_key, [])) != str(gated.get(rows_key, [])))
	h._assert_hud("…so a crew that settles short is promised NO held rate",
		not str(gated.get(rows_key, [])).contains(SourceForecast.YIELD_ROW_AFTER))
	# **THE HEADER READS `NEXT TURN`, and the retired `PER TURN` is asserted ABSENT beside it.** The
	# readings are composed from the room next turn's take actually has, so the old caption named a
	# RATE for a figure that is one turn's answer — and at equilibrium the two coincide, which is what
	# let it stand for so long. Both halves, because a header that had stopped rendering satisfies
	# either alone.
	h._assert_hud("…and a row with no transition is given a header with no arrow to key",
		Readout.yields_header(h._hud._drawercompose._compose_sheet).contains(YIELDS_HEADER_NEEDLE)
			and not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains(
				RETIRED_PER_TURN_HEADER_NEEDLE)
			and not Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("→"))
	# **THE PEAK ZONE CONTRIBUTES NOTHING, ANYWHERE.** Its line — "the most food this source can pay,
	# turn after turn, forever" — restated the definition of the preset the player had just clicked and
	# named no consequence they could act on, so it is struck from `FLOOR_ZONE_HINTS` itself rather
	# than suppressed per surface: an empty entry silences it on all five consumers, which is the
	# intent for copy worth nothing on any of them. Both halves are asserted, the TABLE's and the
	# ASIDE's, because a suppression at either level would satisfy only one.
	#
	# **PAIRED WITH THE STRIP ZONE, which must still warn.** A lone negative is satisfied by emptying
	# the whole table, and `strip`'s line is the one that may never go: it is the only place the sheet
	# says floor 0 is irreversible on the animal web, and the reaching verdict drops its own "then
	# holds it" clause there on the understanding that this line carries the consequence.
	h._assert_hud("the hint TABLE carries nothing for the peak zone — the sentence said nothing",
		HudFormat.floor_hint(SourceForecast.FLOOR_FOOD_PEAK, SourceForecast.LABOR_KIND_FORAGE) == "")
	h._assert_hud("…so the readout's aside states no peak hint either",
		not Readout.readout_aside_text(h._hud._drawercompose._compose_sheet).contains("turn after turn"))
	h._assert_hud("…and the STRIP zone still warns, on the web whose floor 0 is permanent",
		HudFormat.floor_hint(SourceForecast.FLOOR_MIN, SourceForecast.LABOR_KIND_HUNT)
			.contains("gone for good"))

	# State forage_three_accounts_overdraw — THE SAME meadow at floor 0 with a crew big enough to bite.
	#
	# **THE PER-ACCOUNT DIVERGENCE THIS FRAME WAS BUILT ON IS GONE, and its absence is a fact about the
	# model rather than a lost capability.** It used to author a fast fodder throughput beside a slow
	# food one, so a crew could sit inside the patch's food regrowth while stripping its hay — and the
	# verdict had to be ANY-account. The plant take is one BIOMASS quantity through three fixed rates
	# now (`forage::forage_take`'s own note: "both operands are the same biomass through the same
	# rates, so the two components agree on which side binds"), so every account overdraws or none
	# does. The `or` in the verdict is therefore inert on the plant web — kept because it costs
	# nothing and the animal web's quantised take is not obliged to stay that way.
	#
	# **WHAT THE FRAME PINS IS THAT THE VERDICT IS THE WIRE'S, AND IT IS A 2x2.** It used to claim the
	# verdict tracked the FLOOR — amber below the food peak, green at it — which was a claim about a
	# predicate the client no longer owns and which was wrong in both directions: the food-peak ceiling
	# is ZERO on a patch standing at the peak, so `take > ceiling` degenerated into "the dial is below
	# 0.5", and the drawdown walk beside it was a private copy of curves the sim owns.
	#
	# `LaborAssignment.overdraws` carries the whole verdict now, so the flag must move with the FIELD
	# and not with the dial. The four claims below are that, stated as a grid: the retired comparison
	# fails the *warns at the peak too* claim, and the retired gate-and-comparison pair fails the *does
	# not warn at floor 0* one. The crew size is load-bearing and deliberately not the auto-max — below
	# ~7 foragers LABOR binds under every ceiling, so a small-crew frame would move no number at all.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_MIN)
	h._hud._compose.set_forage_count(HAY_OVERDRAW_FORAGERS)
	h._compose_forage(hay_meadow)
	await h._settle()
	await h._save("forage_three_accounts_overdraw")
	# The band is swapped for a COPY carrying a standing row on this meadow, and handed back below, so
	# the frame above and every state after it see the band they always saw.
	var prior_meadow_band: Dictionary = h._hud._band_labor.player_band()
	var prior_meadow_bands: Array = h._hud._band_labor._player_bands
	for wire_answer in [true, false]:
		var wire_band := prior_meadow_band.duplicate(true)
		wire_band["labor_assignments"] = [_hay_standing_row(bool(wire_answer))]
		h._hud._band_labor._player_band = wire_band
		h._hud._band_labor._player_bands = [wire_band]
		for floor_value in [SourceForecast.FLOOR_MIN, SourceForecast.FLOOR_FOOD_PEAK]:
			var line: String = h._hud._drawercompose._local_forage_preview_bbcode(
				wire_band, hay_meadow, float(floor_value), HAY_OVERDRAW_FORAGERS)
			var flagged := line.contains(HudStyle.WARN_HEX)
			h._assert_hud("the ⚠ is the wire's `overdraws` (%s) whatever the dial says (floor %.2f)"
				% [str(wire_answer), float(floor_value)], flagged == bool(wire_answer))
	h._hud._band_labor._player_band = prior_meadow_band
	h._hud._band_labor._player_bands = prior_meadow_bands

	# State forage_dead_season — THE STATE THE ISSUE IS NAMED FOR. A patch the wire fully DESCRIBES
	# and whose every cell is zero: deep winter on the same meadow. It must not be confused with
	# `tile_panel_no_forage` (no food module at all, hence no patch and correctly no compose block) —
	# here the sim has answered, and the answer is "nothing this season". So the sheet stays LOUD: the
	# rungs render, they state their zeros as `0.00 food` (the one surviving zero — an empty ceiling
	# that EXISTS is a fact worth reading), and the worker cap stays live rather than switching off.
	var dead_season := _dead_season_tile_fixture()
	h._show_tile(dead_season)
	h._compose_forage(dead_season)   # settle the source key first (it changed)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._compose_forage(dead_season)
	await h._settle()
	await h._save("forage_dead_season")
	h._assert_compose_sheet_fits("forage_dead_season")
	# THE "GOES SILENT" HALF OF THE ISSUE, and it needs the PREVIEW line rather than the rungs: a rung
	# renders whether or not it has a metric (name-only is a legal face), so asserting the picker
	# exists passes even with the bug restored. The preview line is what actually disappeared — it
	# returns "" on an unknown forecast — so it is the only witness that can testify here.
	h._assert_hud("a fully-zero forecast still states its take rather than going silent",
		h._hud._drawercompose._local_forage_preview_bbcode(
			h._hud._band_labor.player_band(), dead_season, SourceForecast.FLOOR_FOOD_PEAK, 1) != "")
	h._assert_hud("a zero rung states its zero rather than going blank",
		_policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(DEAD_SEASON_TOOLTIP))
	# The cap is the half a PNG cannot testify to: `known` is a PRESENCE test, so a described-but-empty
	# patch is capped at `MAX_USEFUL_BARREN` (1) — NOT left UNBOUNDED, which is what an undescribed one
	# gets and what the old rate-based `known` wrongly handed this state.
	h._assert_hud("a described-but-empty patch caps workers rather than going unbounded",
		SourceForecast.max_useful_workers(SourceForecast.forecast_inputs(
			dead_season, SourceForecast.SOURCE_KIND_FORAGE,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK))
			== SourceForecast.MAX_USEFUL_BARREN)

	# `forage_dead_season` is ALSO the CHART's dead-season case (below), so it carries that pair of
	# assertions rather than a second identical PNG: `perWorkerBiomass` is honestly 0 in deep winter,
	# so the two crew targets have no denominator and must never be rendered as a zero saying "nobody
	# is needed" — while the chart still draws, the patch's stock, its floor and its growth curve all
	# being real facts about the ground.
	#
	# **THE PILL STAYS AND SAYS `✕`.** It used to be dropped, and a missing pill reads as the sheet
	# having nothing to say about clearing at all. The COUNT reading is the precondition — the model
	# really did answer the unpriceable sentinel — and the FACE is the claim, since a pill carrying
	# the sentinel on its meta while printing `-1` would satisfy the first alone.
	h._assert_hud("a dead-season patch prices no crew target rather than dividing by a zero throughput",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR)
			== SourceForecast.NO_CREW_ANSWER)
	h._assert_hud("…and says so on a DISABLED pill reading %s, rather than vanishing (got \"%s\")"
			% [Readout.CREW_TARGET_UNREACHABLE_FACE,
				Readout.crew_target_face(h._hud._drawercompose._compose_sheet,
					HudWidgets.CREW_TARGET_CLEAR)],
		Readout.crew_target_face(h._hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_CLEAR) == Readout.CREW_TARGET_UNREACHABLE_FACE
			and Readout.crew_target_is_disabled(h._hud._drawercompose._compose_sheet,
				HudWidgets.CREW_TARGET_CLEAR))
	h._assert_hud("…and still draws its chart, the stock and the curve being facts about the ground",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META) != null)

	# ---- THE CHART, THE TARGETS AND THE VERDICT (docs/plan_harvest_floor.md §7.1/§7.3/§7.6) --------
	# Five fixtures, each breaking the instrument a DIFFERENT way — a chart is exactly the kind of
	# thing that compiles, runs, exits 0 and is visibly wrong, so each is rendered AND looked at.
	# Three are here (the two patches and the dead season above); the herd pair rides beside the wolf.
	#
	# **THE FACTION IS PUT BACK TO STILL-LEARNING CULTIVATION FOR THIS BLOCK, and that is a fixture
	# repair rather than a convenience.** These patches are WILD, so the lesson they teach is
	# Cultivation — and a source teaches nothing once the faction knows its lesson, so at the
	# all-complete dial the frames above leave behind, the aside's teaching line is correctly ABSENT
	# and the live-drag assertion below (that the line RE-READS on a drag) would be asserting nothing.
	# The pair at the end of the block flips the dial back and asserts the absence deliberately.
	# Foddering rides along with the rest of the hay-meadow block (see its own note above): these are
	# the same wild meadows, and without it their fodder readings mute.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": FLOOR_CHART_CULTIVATION_LEARNING, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": 1.0}}])

	# State floor_chart_full — A FULL PATCH WITH THE FLOOR ABOVE ITS STOCK. Nothing stands above the
	# line, so there is nothing to clear (that target reads 0, not a crew) and the verdict reports
	# exactly that. **The CAP does not collapse with it, and this frame is the limit case that proves
	# why** (§7.2): the room is 0, but the patch still grows a little every turn, so the crew that TAKES
	# that growth is 1 — and `max_useful_workers` floors on it rather than telling the player to drop a
	# gatherer they need on the very next turn. The chart's own subject is the
	# GEOMETRY: a nearly-full stock band under a floor line at the very top of the plot, with the
	# floor's flag FLIPPED BELOW its line, the case that would otherwise draw off the plot's edge.
	# (The *at-or-below-the-floor* verdict is stated with a real crew by `forage_dead_season` and
	# `floor_chart_herd_allee`, whose caps leave one; it cannot also be shown here, because a source
	# with no room admits no useful workers at all.)
	var full_patch := ForageFx.floorify(_hay_meadow_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._show_tile(full_patch)
	h._compose_forage(full_patch)
	h._hud._compose.set_forage_floor(FLOOR_CHART_ABOVE_STOCK)
	h._hud._compose.set_forage_count(ForageFx.FLOOR_CHART_CREW)
	h._compose_forage(full_patch)
	await h._settle()
	await h._save("floor_chart_full")
	h._assert_compose_sheet_fits("floor_chart_full")
	h._assert_hud("a floor above the stock is BLOCKED — the source binds, not the crew",
		Readout.verdict_severity(h._hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_BLOCKED)
	h._assert_hud("…and there is nothing to clear, so that target reads zero rather than a crew",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR) == 0)
	# THE HALF A PNG CANNOT SHOW: the chart, the targets and the verdict are read against the SAME
	# crew the stepper renders. They were composed before the cap clamped it once, so the panel stated
	# a verdict for a crew it then refused to staff; this is what pins the order that fixed it.
	var full_hold = Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
	h._assert_hud("a source with no room still admits the crew that HOLDS it — the cap floors on the hold number",
		full_hold > 0)
	h._assert_hud("the verdict reads the crew the stepper shows, not one the cap is about to clamp away",
		Readout.stepper_value(h._hud._drawercompose._compose_sheet) == mini(ForageFx.FLOOR_CHART_CREW, full_hold))

	# State floor_chart_drawn_down — THE SAME PATCH ALREADY DRAWN DOWN, worked below the food peak.
	# The stock band is amber (the patch reports Stressed), the floor sits under it, and the projection
	# must fall to the line and then run FLAT along it: a plant curve never goes negative, so a patch
	# held at a low floor is held, not lost. That is the frame the herd pair `floor_chart_herd_allee`
	# (`chapters/hunt.gd`) is read against.
	# **NOBODY IS BUILDING THIS PATCH, and that is now something a fixture has to SAY.** The walk is
	# suppressed under a composed build (`_walks_to_the_floor`), and a patch carrying progress is
	# building it whether or not anything is declared — so the reference tile's own 0.6 meter would
	# suppress the very arrow this state exists to assert.
	h._floor_chart_drawn_patch = BaseFx.unbuilt(
		ForageFx.floorify(_hay_meadow_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	h._floor_chart_drawn_patch["x"] = 67
	h._floor_chart_drawn_patch["patch_ecology_phase"] = "stressed"
	h._floor_chart_drawn_patch["patch_biomass"] = FLOOR_CHART_DRAWN_STOCK_FRACTION \
		* float(h._floor_chart_drawn_patch["patch_carrying_capacity"])
	h._show_tile(h._floor_chart_drawn_patch)
	h._compose_forage(h._floor_chart_drawn_patch)
	# **A FLOOR BELOW THE STOCK BUT ABOVE THE BASELINE**, deliberately not `strip`: at floor 0 the
	# projection lands on the plot's own bottom edge and the "descends, then RUNS FLAT along the line"
	# reading — the whole contrast with the herd frame `floor_chart_herd_allee` (`chapters/hunt.gd`) —
	# is indistinguishable from the axis.
	h._hud._compose.set_forage_floor(FLOOR_CHART_HELD_FLOOR)
	h._hud._compose.set_forage_count(ForageFx.FLOOR_CHART_CREW)
	h._compose_forage(h._floor_chart_drawn_patch)
	await h._settle()
	await h._save("floor_chart_drawn_down")
	h._assert_hud("a patch drawn toward a reachable floor states a HOLD crew, not just a clearing one",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD)
			!= Readout.CREW_TARGET_ABSENT)
	# **THE BURST AND THE STEADY RATE, ON THE SAME READING.** The headline take is capped by the ROOM
	# above the floor, so a crew big enough to clear that room in a turn or two had its one-off burst
	# labelled `/TURN` — the misreading this pair exists to end. Asserted as `now → after` per account
	# rather than as a second row: the three accounts are one biomass flow through a fixed vector, so
	# a second row would carry one new fact three times.
	var burst_text = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: burst readings  %s" % burst_text)
	h._assert_hud("a crew that reaches the floor states what it takes NOW and what it holds AFTER",
		burst_text.contains(BURST_FOOD_READING) and burst_text.contains(BURST_FODDER_READING))
	# The `after` must be strictly SMALLER, or the reading would be claiming a drawdown pays less than
	# it does — and the two numbers coming from one function with two ceilings is exactly what could
	# silently swap them. Both parsed off the rendered face, never recomputed here.
	h._assert_hud("…and the held rate is the LOWER of the two, on every account it states",
		_yield_now_after(burst_text, "FOOD")[1] < _yield_now_after(burst_text, "FOOD")[0]
			and _yield_now_after(burst_text, "FODDER")[1] < _yield_now_after(burst_text, "FODDER")[0])
	# **THE ASIDE NO LONGER NARRATES WHAT THE NUMBERS ABOVE IT ALREADY SAY.** Two lines went:
	#   • the idle-crew note (`2 of your 3 foragers go idle once it is holding — only 1 can carry what
	#     grows back`) was arithmetic over the stepper's count and the `hold it after` pill, both a
	#     centimetre above it — and that pill is a BUTTON that sets the count, so the remedy was never
	#     a sentence away either. THIS frame is the one that carried it (3 foragers, hold crew 1), so
	#     it is the frame that can testify it is gone.
	#   • the PEAK zone's hint, asserted below where a peak-floor sheet is on screen.
	# The idle needle is the whole rendered clause, not the bare count: `1` appears in the crew targets
	# and in the stepper, so a digit search would pass with the line restored.
	h._assert_hud("the aside does not narrate the idle count the crew row already states twice",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_HOLD) == 1
			and not Readout.readout_aside_text(h._hud._drawercompose._compose_sheet).contains("go idle"))
	# **THE UNIT IS SAID ONCE, IN THE HEADER, AND THE HEADER KEYS THE ARROW.** Three `/TURN`s were the
	# widest thing on the row and it could not afford them once each account stated two numbers; the
	# header also stops `→` being a glyph the player has to guess. `NOW → AFTER` is the crew buttons'
	# own two words, which sit directly above it.
	var burst_header = Readout.yields_header(h._hud._drawercompose._compose_sheet)
	h._assert_hud("the row states its unit once in a header, not per account",
		burst_header.contains(YIELDS_HEADER_NEEDLE)
			and not burst_header.contains(RETIRED_PER_TURN_HEADER_NEEDLE)
			and not burst_text.contains("/TURN"))
	h._assert_hud("…and the header keys the arrow while there is one to key",
		burst_header.contains("NOW → AFTER"))
	# **THE DRAG CONTRACT, which no frame can show.** A LIVE floor change must refill the readings that
	# follow the floor WITHOUT rebuilding the controls — because the rebuild `queue_free`s the chart,
	# and Godot routes motion to the node that took the press, so a rebuilt chart ends the drag on the
	# first pixel of movement. Driving the signal directly is the only way to test it headlessly: the
	# chart must SURVIVE, and the verdict must have re-read against the new floor.
	var live_chart = Q.find_meta_node(h._hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META)
	# **AND SO MUST THE YIELDS — the reading the drag is AIMED at.** Reported from play: the verdict
	# followed the drag while the account numbers sat frozen, catching up only on release when the
	# rebuild lands. Captured BEFORE the emit, because the only assertion that can see that bug is a
	# CHANGE: the stale row is a perfectly valid, perfectly findable node, so "the yields row is still
	# there" passes with the defect fully restored.
	var yields_before = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	live_chart.emit_signal("floor_changed", FLOOR_CHART_ABOVE_STOCK, false)
	# **THE FRAME IS LOAD-BEARING.** `queue_free` is DEFERRED, so a rebuild leaves the old chart both
	# valid and findable for the rest of the frame it happened on — every same-frame form of this
	# assertion passes with the bug restored (measured, twice). Settling first is what makes the free
	# land, and `is_instance_valid` then answers the question actually being asked: is the node that
	# took the press still there to receive the motion?
	await h._settle()
	h._assert_hud("a LIVE drag leaves the chart alive — a rebuilt one would end the drag it is serving",
		is_instance_valid(live_chart))
	h._assert_hud("…and the verdict has re-read against the dragged floor, without that rebuild",
		Readout.verdict_severity(h._hud._drawercompose._compose_sheet) == SourceForecast.VERDICT_BLOCKED)
	var yields_after = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	h._assert_hud("…and so have the YIELDS, which are what the player is dragging TOWARD (%s → %s)"
		% [yields_before, yields_after],
		yields_before != "" and yields_after != "" and yields_after != yields_before)
	# **THE DRAG'S ONLY AFFORDANCE, which no frame can show either.** The whole plot is the drag
	# target — grabbing a 1px line would be unusable — so nothing about the chart's SHAPE says it can
	# be dragged, and a screenshot cannot carry a cursor. Reported from play: the pointer stayed an
	# arrow over the chart where the prototype showed the up/down resize cursor across the whole chart
	# area. Asserted on the control for the same reason as the pair above.
	h._assert_hud("the chart wears the vertical-resize cursor, so the drag has an affordance at all",
		live_chart.mouse_default_cursor_shape == Control.CURSOR_VSIZE)
	# **THE TEACHING RATE FOLLOWS THE DRAG TOO.** `learn_multiplier` is `floor / the food peak`, so
	# the aside's cyan line is a function of the floor exactly as the yields and the crew targets are
	# — and it is the line that tells the player what the top half of the dial is FOR, so a stale one
	# is the worst of the three to leave behind. Compared before/after rather than against a literal:
	# the fixture's floor is free to move without silently retargeting this at a number.
	var teaching_before = Readout.teaching_line(h._hud._drawercompose._compose_sheet)
	live_chart.emit_signal("floor_changed", FLOOR_CHART_TEACHING_DRAG_FLOOR, false)
	await h._settle()
	h._assert_hud("the teaching rate re-reads on a LIVE drag, like the numbers it sits under",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet) != teaching_before
			and Readout.teaching_line(h._hud._drawercompose._compose_sheet) != "")
	# Put the sheet back where the frame above left it (a live change deliberately does not re-render).
	h._hud._compose.set_forage_floor(FLOOR_CHART_HELD_FLOOR)
	h._compose_forage(h._floor_chart_drawn_patch)

	# State forage_lesson_known — **A LESSON THE FACTION HAS ALREADY LEARNED IS NOT TAUGHT AGAIN**, and
	# the claim is only meaningful as an A/B: this is the SAME patch, the same crew and the same floor
	# as the frame above, with the faction's Cultivation as the only thing that moves. `rung_lesson`
	# keys off the SOURCE's standing rung alone, so a wild patch went on reading `Teaching cultivation
	# at ×1.00` for the rest of the game (reported from play) — and asserting only the empty half would
	# pass on a line blanked unconditionally, which is why the learning half is captured first.
	var teaching_learning = Readout.teaching_line(h._hud._drawercompose._compose_sheet)
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": 1.0}}])
	h._compose_forage(h._floor_chart_drawn_patch)
	await h._settle()
	await h._save("forage_lesson_known")
	h._assert_hud("a lesson still being earned IS named, so the pair is not vacuous",
		teaching_learning.contains(Readout.TEACHING_LESSON_NEEDLE)
			and teaching_learning.contains(String(SourceForecast.RUNG_LESSONS[
				SourceForecast.SOURCE_KIND_FORAGE][SourceForecast.IMPROVEMENT_NONE])))
	# NO LINE AT ALL rather than an empty one: with no build in flight there is no second half to keep.
	h._assert_hud("…and the same patch teaches nothing once the faction knows it — no line, not a blank",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet) == "")

	# Reset so the states after this render their usual staple patch + Sustain rung.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_species("")

	# ---- THE LOCKED FODDER ACCOUNT (issue #485) --------------------------------------------------
	# THREE STATES ON ONE PATCH, JUDGED AS A SET. The sim credits a WILD patch's fodder take only to a
	# faction that has learned Foddering — which is what KEEPING A PENNED HERD teaches, so a forager
	# band structurally cannot have it — while a patch COMMITTED to a crop is paid unconditionally,
	# committing being the bid. The sheet composed `actual_fodder` off `fodderPerBiomass` regardless,
	# so a forager band on a hay meadow read a fodder rate it banked none of, with no feedback anywhere.
	#
	# **A LONE NEGATIVE HERE IS SATISFIED BY SILENCING THE ACCOUNT EVERYWHERE**, which is precisely the
	# hidden gate this row's surviving UNIT exists to refuse. So the set is: locked (the `—`), known
	# (the credit, knowledge alone moving), and committed (the credit, the COMMITMENT alone moving) —
	# and without the third the whole thing passes as "gated on knowledge".
	var wild_hay := _hay_meadow_tile_fixture()

	# State forage_fodder_locked — a WILD hay meadow worked by a band that cannot bank hay. The fodder
	# row keeps its FODDER unit and loses its number; the food row beside it stays a live number, which
	# is what scopes the lock to ONE account rather than to the readout.
	#
	# **Foddering is dialed PART-LEARNED, not 0.** It is a 0..1 track like every other and only
	# `KNOWLEDGE_COMPLETE` opens the credit, so a fixture at 0 could not tell "unlearned" from "partly
	# learned and still refused" — and the reason line's live percent would be untestable besides.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": FODDER_LOCK_PROGRESS}}])
	h._show_tile(wild_hay)
	h._compose_forage(wild_hay)   # settle the source key first (it changed)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(FODDER_LOCK_FORAGERS)
	h._compose_forage(wild_hay)
	await h._settle()
	await h._save("forage_fodder_locked")
	h._assert_compose_sheet_fits("forage_fodder_locked")
	var locked_fodder := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
	h._assert_hud("an unbankable account reads the locked glyph, and still states its unit",
		locked_fodder == HudComposeVocab.YIELD_LOCKED_GLYPH)
	# THE LOCK IS SCOPED TO ONE ACCOUNT. Without this the frame passes on a readout that went silent —
	# which is exactly the hidden gate the surviving unit exists to refuse. Asked of FOOD, the account
	# this crew genuinely banks.
	var locked_food := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FOOD)
	h._assert_hud("…while the account this band CAN bank still reads as a live number",
		locked_food != HudComposeVocab.YIELD_LOCKED_GLYPH
			and locked_food != Readout.YIELDS_ACCOUNT_ABSENT
			and float(locked_food) > 0.0)
	# BY META, never by text: the aside's other two lines move with the floor and this one does not, so
	# a whole-aside comparison testifies about it in neither direction.
	h._assert_hud("…and the aside says WHY, with the live percent and both remedies",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet)
			== HudFloraVocab.GATE_REASON_WILD_FODDER_FORMAT % [
				HudFormat.progress_percent(FODDER_LOCK_PROGRESS),
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CORRAL),
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE)])
	# The joined sentence has no room for a reason, so it must not promise the account at all — the
	# other half of "the row keeps its unit": the readout can qualify a number, a sentence cannot.
	h._assert_hud("…and the joined sentence promises no fodder either",
		not h._hud._drawercompose._local_forage_preview_bbcode(h._hud._band_labor.player_band(),
			wild_hay, SourceForecast.FLOOR_FOOD_PEAK, FODDER_LOCK_FORAGERS)
			.contains(SourceForecast.YIELD_ACCOUNT_FODDER))
	# **NOR DO THE FLOOR PRESETS ONE CONTROL ABOVE THE ROW.** Their tooltips are the OTHER surface on
	# this sheet composing a fodder ceiling, and a `♻ Best harvest` reading `+0.40 fodder/turn` directly
	# over a readout marked `— FODDER` is the sheet contradicting itself — the very defect #485 is
	# about. A tooltip is one flat string with nowhere to hang a reason, so the clause is DROPPED: the
	# lock is already stated once, in the register built to explain it. Asserted as a PAIR with the
	# line below, or a tooltip blanked outright would satisfy the negative half.
	var locked_tooltip := _policy_rung_tooltip(
		h._hud._drawercompose._compose_sheet, SourceForecast.FLOOR_PRESET_PEAK)
	h._assert_hud("a preset quotes no fodder ceiling the sim would refuse — no clause, not a zero",
		not locked_tooltip.contains(SourceForecast.YIELD_ACCOUNT_FODDER))
	h._assert_hud("…while the ceilings this crew CAN bank survive, so the tooltip is not merely blanked",
		locked_tooltip.contains(HAY_PEAK_TOOLTIP_FODDER_LOCKED))
	# **THE LOCK LINE IS IN THE LIVE SET, and only a DRIVEN CHANGE can say so.** Its text does not move
	# with the floor — it states what the FACTION is missing — but its PRESENCE does: raise the floor
	# above the stock and the fodder take goes to nothing, the muted row leaves with it, and a sentence
	# resolved once before the render would go on explaining a `—` that is no longer on screen. That
	# stale line is a perfectly valid, perfectly findable node, so "the line is there" passes with the
	# defect fully restored — which is why this is asserted as a disappearance under a live drag, the
	# same shape as `floor_chart_drawn_down`'s frozen-yields triple.
	var lock_chart = Q.find_meta_node(h._hud._drawercompose._compose_sheet, HudWidgets.FLOOR_CHART_META)
	var lock_line_before = Readout.locked_account_line(h._hud._drawercompose._compose_sheet)
	lock_chart.emit_signal("floor_changed", FLOOR_CHART_ABOVE_STOCK, false)
	await h._settle()
	h._assert_hud("a LIVE drag leaves the chart alive, so this is the drag path and not a rebuild",
		is_instance_valid(lock_chart))
	h._assert_hud("the lock line was there to lose, so its absence below is a change and not a blank",
		lock_line_before != "")
	h._assert_hud("…and a floor that takes no hay drops the lock line with the row it explains",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet) == ""
			and Readout.yields_account_number(h._hud._drawercompose._compose_sheet,
				SourceForecast.YIELD_ACCOUNT_FODDER) == Readout.YIELDS_ACCOUNT_ABSENT)
	# Put the sheet back where the frame above left it (a live change deliberately does not re-render).
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._compose_forage(wild_hay)

	# State forage_fodder_known — THE SAME PATCH with Foddering complete. Nothing about the ground
	# moves between this frame and the one above; only what these people know how to do with hay. It is
	# also the FIVE-TRACK strip's frame — every track is non-zero here, so the top-bar readout renders
	# the whole ladder plus the capability that is not a rung of it.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": 1.0}}])
	h._compose_forage(wild_hay)
	await h._settle()
	await h._save("forage_fodder_known")
	var known_fodder := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
	h._assert_hud("the same hay reads as a live number once the band knows Foddering",
		known_fodder != HudComposeVocab.YIELD_LOCKED_GLYPH
			and known_fodder != Readout.YIELDS_ACCOUNT_ABSENT
			and float(known_fodder) > 0.0)
	h._assert_hud("…and the aside drops the lock line entirely — no line, not a blank",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet) == "")
	h._assert_hud("…and the preset tooltips quote the hay ceiling again, all three clauses",
		_policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(HAY_PEAK_TOOLTIP))
	# The strip that used to name it is retired (issue #450), so this asks the cache the faction page's
	# KNOWLEDGE zone reads: the fifth track is CARRIED, which is what makes it a capability the ladder
	# tracks rather than a rung nobody records.
	h._assert_hud("…and the faction carries the fifth track, which is a capability and not a rung",
		h._hud._topbar.faction_tracks(HudConst.PLAYER_FACTION_ID).has(
			HudFloraVocab.KNOWLEDGE_TRACK_FODDERING))

	# State forage_fodder_committed — THE SAME PATCH COMMITTED to its hay, with Foddering back at 0.
	# THIS is the half that pins `species.is_some()`: the credit is open with the knowledge fully
	# absent, so the gate cannot be read as knowledge alone.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0}}])
	var committed_hay := _committed_hay_meadow_tile_fixture()
	h._show_tile(committed_hay)
	h._compose_forage(committed_hay)
	await h._settle()
	await h._save("forage_fodder_committed")
	var committed_fodder := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
	h._assert_hud("a COMMITTED patch pays its hay with the knowledge still unlearned",
		committed_fodder != HudComposeVocab.YIELD_LOCKED_GLYPH
			and committed_fodder != Readout.YIELDS_ACCOUNT_ABSENT
			and float(committed_fodder) > 0.0)
	h._assert_hud("…so the commitment closes the gate on its own — no lock line either",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet) == "")
	# The presets follow the same gate from its OTHER end: the credit is open on the commitment alone,
	# so the ceiling they quote is the full three-account one with the knowledge still unlearned.
	h._assert_hud("…and a committed patch's presets quote its hay ceiling, knowledge or no knowledge",
		_policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(HAY_PEAK_TOOLTIP))
	# State forage_fodder_grain_committed — THE REPORTED CASE. The SAME meadow committed to its GRAIN,
	# knowledge still unlearned. `wild_emmer` pays 0.0 fodder at both rungs, so committing to it is not
	# a bid for hay and the credit stays shut: the sim's arm is
	# `committed_to_a_fodder_crop(patch.species, &flora) || knows(faction, FODDERING)`, and this client
	# mirrors it species-for-species. It is the CONTROLLED TWIN of the frame directly above — same
	# ground, same basket, same rates, only the committed species moves — so the pair testifies about
	# the species test and not about the patch.
	var committed_grain := _committed_grain_meadow_tile_fixture()
	h._show_tile(committed_grain)
	h._compose_forage(committed_grain)
	await h._settle()
	await h._save("forage_fodder_grain_committed")
	var grain_fodder := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
	h._assert_hud("a patch committed to a GRAIN banks no hay — the commitment is not a bid for it",
		grain_fodder == HudComposeVocab.YIELD_LOCKED_GLYPH)
	h._assert_hud("…and the lock line is back, so the sheet says why rather than going quiet",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet)
			== HudFloraVocab.GATE_REASON_WILD_FODDER_FORMAT % [
				HudFormat.progress_percent(0.0),
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CORRAL),
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE)])
	# **THE TWO COMMITMENTS DIFFER, and asserted as a DIFFERENCE rather than two absolutes.** A gate
	# that had gone back to refusing every commitment would pass the grain claim above on its own; only
	# putting it beside the hay reading says the test is about the SPECIES.
	h._assert_hud("…and it differs from the HAY commitment on the same ground, which is the species test",
		grain_fodder != committed_fodder)
	# **THE PRESETS FOLLOW THE SAME GATE, one control above the row.** A tooltip quoting a hay ceiling
	# over a row marked `—` is the self-contradicting sheet #485 removed, arriving again through the
	# commitment arm.
	h._assert_hud("…and no preset quotes a hay ceiling the sim would refuse this commitment",
		not _policy_rung_tooltip(h._hud._drawercompose._compose_sheet,
			SourceForecast.FLOOR_PRESET_PEAK).contains(SourceForecast.YIELD_ACCOUNT_FODDER))

	# State forage_fodder_grain_known — THE SAME GRAIN COMMITMENT with Foddering complete. The two arms
	# are an OR: tightening the commitment one must not have narrowed the knowledge one, and a band that
	# has learned to make hay banks it off any ground whatever its patch is committed to.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0, "foddering": 1.0}}])
	h._compose_forage(committed_grain)
	await h._settle()
	await h._save("forage_fodder_grain_known")
	var grain_known_fodder := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
	h._assert_hud("Foddering opens the credit on a grain commitment too — the arms are an OR",
		grain_known_fodder != HudComposeVocab.YIELD_LOCKED_GLYPH
			and grain_known_fodder != Readout.YIELDS_ACCOUNT_ABSENT
			and float(grain_known_fodder) > 0.0)
	h._assert_hud("…so no lock line survives the knowledge arm",
		Readout.locked_account_line(h._hud._drawercompose._compose_sheet) == "")
	# Put the knowledge back where the committed-hay frame left it, so the drawdown comparison below
	# is asked with the fodder credit shut, exactly as it was written.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0}}])
	h._show_tile(committed_hay)
	h._compose_forage(committed_hay)
	await h._settle()

	# The DRAWDOWN is a fact about the biomass the crew moves, not about which accounts it banks, so
	# the fodder ceiling comparison is unchanged by the lock — on a hay-only patch it is the only
	# drawdown signal there is. Asked of the two fixtures at one floor and one crew.
	h._assert_hud("the overdraw verdict is the same locked and unlocked — the take is the take",
		h._hud._drawercompose._forage_yield_model(h._hud._band_labor.player_band(), wild_hay,
			SourceForecast.FLOOR_MIN, HAY_OVERDRAW_FORAGERS).get(
				h._hud._drawercompose.YIELD_MODEL_OVERDRAW)
			== h._hud._drawercompose._forage_yield_model(h._hud._band_labor.player_band(),
				committed_hay, SourceForecast.FLOOR_MIN, HAY_OVERDRAW_FORAGERS).get(
					h._hud._drawercompose.YIELD_MODEL_OVERDRAW))

	# Put the faction's knowledge back where this chapter's earlier blocks left it, so the states after
	# this one render the ladder they were written against.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0}}])
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_species("")

	# State forage_fodder_standing — THE COMPACT READOUT ON A FODDER-ONLY SOURCE (issue #449), i.e. the
	# state every surface in this arc exists for: a WORKED hay meadow whose take is feed and not
	# provisions. The drawer's closed standing summary composes from the SAME
	# `SourceForecast.source_yield_readout` the Band panel's work rows do, so this frame and that board
	# cannot state different products for one assignment — and before this it read `+0.00 /turn` on a
	# tile that was filling the band's fodder store every turn.
	var standing_band: Dictionary = h._hud._band_labor._player_band
	var standing_roster: Array = h._hud._band_labor._player_bands
	h._hud.close_compose_sheet()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = _fodder_field_band_fixture()
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(wild_hay)
	await h._settle()
	await h._save("forage_fodder_standing")
	var fodder_readout := SourceForecast.source_yield_readout(
		_fodder_field_assignment(), SourceForecast.LABOR_KIND_FORAGE)
	# EQUALITY, not `contains`: half the claim is what the suffix must NOT also say. A `+0.00 /turn`
	# leading it is exactly the reading this arc removed, and a containment test passes with it there.
	h._assert_hud("a fodder-only source's readout states its feed rate ALONE (got \"%s\")"
		% String(fodder_readout["label_suffix"]),
		String(fodder_readout["label_suffix"]) == FODDER_STANDING_SUFFIX)
	h._assert_hud("…and its tooltip names the account it credits (got \"%s\")"
		% String(fodder_readout["tooltip"]),
		String(fodder_readout["tooltip"]).contains(FODDER_STANDING_TOOLTIP_CLAUSE))
	h._assert_hud("…and the rendered drawer summary carries it",
		Q.has_label_containing(h._hud, FODDER_STANDING_CLAUSE))
	# Hand the chapter's own subject and roster back, so the states after this one render against the
	# band and the tile this block borrowed rather than against a hay Field nobody after it works.
	h._hud._band_labor._player_bands = standing_roster
	h._hud._band_labor._player_band = standing_band
	h._hud._compose.reset_forage_source()
	h._show_tile(committed_hay)
	await h._settle()

	# State forage_no_food_basket — **THE TILE FROM THE REPORT: Tobacco 56% + Hay Grass 44%, and not a
	# calorie between them.** The sheet read `max 1 worker useful here — more would be idle` with 17
	# workers standing idle, directly beneath its own `13 clear it now` / `2 hold it after` and a
	# verdict naming 2 foragers as the remedy — and the `+` was dead at 1, so none of those three
	# numbers could be acted on. `max_useful_workers` divides by the FOOD term, and arc #527 made the
	# axis triple an alias of it, so the barren branch came to fire on every source that pays no food.
	var no_food := _no_food_basket_tile_fixture()
	h._hud._compose.reset_forage_source()
	h._show_tile(no_food)
	h._compose_forage(no_food)
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(NO_FOOD_BASKET_QUOTE_CREW)
	h._compose_forage(no_food)
	await h._settle()
	await h._save("forage_no_food_basket")
	h._assert_compose_sheet_fits("forage_no_food_basket")
	var no_food_sheet = h._hud._drawercompose._compose_sheet
	# (0) THE FIXTURE REALLY IS THE REGIME. Without this every claim below is about an ordinary patch:
	# the whole point is a source whose FOOD account is a structural zero while two other accounts pay.
	var no_food_forecast := SourceForecast.forecast_inputs(
		ForageFx.floorify(no_food, HudComposeVocab.FORAGE_FORECAST_PREFIX),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK)
	h._assert_hud("the fixture pays no food and still pays its other accounts",
		float(no_food_forecast["per_worker"]) == 0.0
			and float(no_food_forecast["per_worker_fodder"]) > 0.0
			and not (no_food_forecast[SourceForecast.FORECAST_PER_WORKER_MATERIAL_KEY] as Array).is_empty())
	# (1) **THE CAP IS NOT THE BARREN ONE.** Stated as the relation rather than as the number it
	# happens to produce, and paired below with the dead season so the widening cannot have been a
	# deletion of the guard.
	var no_food_cap := SourceForecast.max_useful_workers(no_food_forecast)
	h._assert_hud("a patch paying fodder and materials is not barren (cap %d)" % no_food_cap,
		no_food_cap > SourceForecast.MAX_USEFUL_BARREN)
	# (2) **AND THE PAIRING**: a genuinely dead-season patch still caps at one worker. Asked of the
	# fixture the issue is named after, in the same frame, because "not barren" is trivially satisfied
	# by a cap that stopped answering at all.
	h._assert_hud("…while a patch that pays nothing anywhere still caps at one worker",
		SourceForecast.max_useful_workers(SourceForecast.forecast_inputs(
			ForageFx.floorify(_dead_season_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX),
			SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.FLOOR_FOOD_PEAK)) == SourceForecast.MAX_USEFUL_BARREN)
	# (3) **THE CAP REACHES THE CREW THE SHEET'S OWN TARGETS NAME** (§7.6) — read off the rendered
	# pills, not recomputed, since the defect was the panel disagreeing with itself. Both must be
	# PRESENT: an absent target passes a `>=` for free.
	var no_food_clear := Readout.crew_target_count(no_food_sheet, HudWidgets.CREW_TARGET_CLEAR)
	var no_food_hold := Readout.crew_target_count(no_food_sheet, HudWidgets.CREW_TARGET_HOLD)
	h._assert_hud("the sheet prices a crew at all — both targets render (%d / %d)"
		% [no_food_clear, no_food_hold],
		no_food_clear > 0 and no_food_hold > 0)
	h._assert_hud("…and the cap (%d) reaches both of them" % no_food_cap,
		no_food_cap >= no_food_clear and no_food_cap >= no_food_hold)
	# (4) **THE MATERIAL ROW RENDERS** — the other half of the report: the PER TURN box showed
	# `— FODDER` and no material row at all on a tile 56% tobacco. Each material as its OWN row, never
	# summed, and the FODDER row still reading beside them.
	var no_food_yields := Readout.yields_text(no_food_sheet)
	var no_food_crew: int = h._hud._compose.forage_count()
	# **THE CREW ARM OF THE `min` IS WHAT THIS FRAME READS**, which is why the composed crew sits below
	# the saturating one: at the clearing crew the two arms are equal by construction, and a producer
	# that never read the per-worker rate would render the same string. The band's own
	# `output_multiplier` is folded in because it is a FIXTURE fact (0.9 on this roster's band) rather
	# than part of the model under test.
	var no_food_output := float(h._hud._band_labor.player_band().get(
		"output_multiplier", SourceForecast.OUTPUT_FULL))
	var tobacco_take := float(no_food_crew) * NO_FOOD_BASKET_CARRY \
		* NO_FOOD_BASKET_TOBACCO_PER_BIOMASS * no_food_output
	h._assert_hud("the crew stays below the saturating one, so the CREW arm is what binds (%d < %d)"
		% [no_food_crew, no_food_clear], no_food_crew > 0 and no_food_crew < no_food_clear)
	h._assert_hud("the sheet quotes the tobacco this gather banks (got \"%s\")" % no_food_yields,
		no_food_yields.contains(NO_FOOD_BASKET_TOBACCO_ID.to_upper()))
	h._assert_hud("…and the fibre beside it, as two SEPARATE rows",
		Readout.yields_account_number(no_food_sheet, NO_FOOD_BASKET_TOBACCO_ID)
			!= Readout.YIELDS_ACCOUNT_ABSENT
		and Readout.yields_account_number(no_food_sheet, NO_FOOD_BASKET_FIBRE_ID)
			!= Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…at the CREW arm of the clamp, so the per-worker rate is read (%.2f tobacco)"
		% tobacco_take,
		tobacco_take > 0.0 and no_food_yields.contains(SourceForecast.format_magnitude(tobacco_take)))
	h._assert_hud("…and the FODDER row still reads, so the materials did not replace it",
		Readout.yields_account_number(no_food_sheet, SourceForecast.YIELD_ACCOUNT_FODDER)
			!= Readout.YIELDS_ACCOUNT_ABSENT)
	# (5) …AND NO `0.00 FOOD`. A patch that pays no calories states no food row rather than an empty
	# one — the §7.7 rule, which this tile is the plant-web case of.
	h._assert_hud("…and states NO food row, these two plants being no one's dinner",
		not no_food_yields.contains(SourceForecast.YIELD_ACCOUNT_UNITS[
			SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()))
	# (6) **THE STEPPER GOES WHERE THE TARGET SAYS**, driven through the REAL button, because the clamp
	# lives in the press handler rather than in the arithmetic. This is the claim the reported
	# screenshot fails: the press landed on 1. Taken LAST, after the frame is saved, since it moves the
	# crew off the one the readout claims above were composed at.
	Q.find_crew_target(no_food_sheet, HudWidgets.CREW_TARGET_CLEAR).pressed.emit()
	h._assert_hud("…so pressing *clear it now* lands the stepper on the crew it names",
		h._hud._compose.forage_count() == no_food_clear)

	# State forage_cash_crop_field — **THE SAME BASKET ONE RUNG UP, and the reported 2026-08-22
	# defect.** A completed 100%-tobacco Field with TWO tenders committed read `TENDERS 0` and
	# `max 0 workers useful here — more would be idle`, beside a tile card and a Work board both
	# stating the two.
	#
	# **THE CHAIN, and every link is client-side.** `FORECAST_MANAGED_FLAG_KEYS` still carried
	# `is_field`, so a Field took `forecast_inputs`' MANAGED branch — whose material ceiling comes from
	# `FORECAST_PAYOFF_MATERIAL_KEYS`, which has no plant entry by design — so the ceiling was
	# structurally `[]`; `off_axis_useful_workers` divided a `0` room by a live tobacco rate and
	# answered `0`, while `hold_crew`/`reach_crew` returned `0` for a "managed" source and §7.2's crew
	# floors could not rescue it. The plant web's managed rung is retired sim-side
	# (`forage.rs`, "RETIRED: the whole rung-3 MANAGED HARVEST"), and the client now follows.
	#
	# ⛔ **THE DANGEROUS HALF IS THE STAGED COUNT, NOT THE NOTE.** The cap clamps
	# `ComposeState.clamp_forage_count`, so the sheet staged `0` while two tenders were committed —
	# and confirming it sends `assign_labor` with `0`, silently dropping both, from a panel that never
	# showed the player a `2`.
	var field_tile := _cash_crop_field_tile_fixture()
	var prior_field_band = h._hud._band_labor.player_band()
	var prior_field_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _cash_crop_field_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud._compose.reset_forage_source()
	h._show_tile(field_tile)
	h._compose_forage(field_tile)
	await h._settle()
	await h._save("forage_cash_crop_field")
	h._assert_compose_sheet_fits("forage_cash_crop_field")
	var field_sheet = h._hud._drawercompose._compose_sheet
	var field_forecast := SourceForecast.forecast_inputs(
		ForageFx.floorify(field_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK)
	# (0) **THE FIXTURE REALLY IS THE REGIME**: a BUILT Field, paying no food and a live material rate.
	# Without this the claims below are about an ordinary wild patch, which never had the defect.
	h._assert_hud("the fixture is a built Field that pays no food and does pay a material",
		bool(field_tile["patch_is_field"])
			and float(field_forecast["per_worker"]) == 0.0
			and not (field_forecast[SourceForecast.FORECAST_PER_WORKER_MATERIAL_KEY]
				as Array).is_empty())
	# (1) **THE MECHANISM**: the material ceiling is composed rather than structurally empty. Asserted
	# on the ROOM because that is the term the divide answered `0` on — a cap assertion alone passes
	# on a client that floors the cap and still has no ceiling.
	h._assert_hud("a Field composes a material ceiling from its own stand, not an empty payoff vector",
		not (field_forecast["material_ceiling"] as Array).is_empty())
	# (2) …so the cap is a real ceiling rather than the reported zero, AND it is not the barren one —
	# `MAX_USEFUL_BARREN` is what a source that pays nothing at all answers, and this one pays tobacco.
	var field_cap := SourceForecast.max_useful_workers(field_forecast)
	h._assert_hud("a Field paying 1 material/worker reports a real ceiling (cap %d)" % field_cap,
		field_cap > SourceForecast.MAX_USEFUL_BARREN)
	# (3) ⛔ **AND THE STAGED COUNT NEVER FALLS BELOW THE COMMITTED ONE** — the assertion the silent
	# drop needs, and the one thing a frame of this sheet cannot show, a `TENDERS 0` stepper being a
	# perfectly ordinary control. Read off the compose model the COMMIT sends rather than off the
	# arithmetic above it, because `clamp_forage_count` sits between the two and is where the drop
	# happened. **It is an assertion rather than a runtime clamp on purpose** — see
	# `DrawerComposeController._forecast_worker_cap`, which records the floor that was tried here and
	# the legitimate cap-fall it broke.
	h._assert_hud("the sheet stages at least the %d tenders already committed (staged %d)"
		% [FIELD_TENDERS, h._hud._compose.forage_count()],
		h._hud._compose.forage_count() >= FIELD_TENDERS)
	# (4) **THE SCOPE, and it is what stops (3) reading as *never cap anything*.** The claim is made
	# of a source that PAYS — `pays_any_account`, the predicate it is worded in terms of — and the
	# pairing is a patch that pays into nothing at all, which must still cap at one worker however
	# many hands are standing on it.
	h._assert_hud("…and this Field is a source that pays into some account, which is the claim's scope",
		SourceForecast.pays_any_account(field_forecast))
	var dead_forecast := SourceForecast.forecast_inputs(
		ForageFx.floorify(_dead_season_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX),
		SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
		SourceForecast.FLOOR_FOOD_PEAK)
	h._assert_hud("…while a patch that pays into no account at all is still capped at one worker",
		not SourceForecast.pays_any_account(dead_forecast)
			and SourceForecast.max_useful_workers(dead_forecast)
				== SourceForecast.MAX_USEFUL_BARREN)
	# (5) …and the sheet quotes the tobacco those tenders bring home, so the cap was widened rather
	# than the readout silenced.
	h._assert_hud("…and the Field's readout names the tobacco it pays",
		Readout.yields_account_number(field_sheet, NO_FOOD_BASKET_TOBACCO_ID)
			!= Readout.YIELDS_ACCOUNT_ABSENT)
	h._hud._band_labor._player_band = prior_field_band
	h._hud._band_labor._player_bands = prior_field_bands

	# Hand the chapter's subject back, so the states after this one open on the tile they were written
	# against rather than on a patch nobody after them works.
	h._hud._compose.reset_forage_source()
	h._show_tile(committed_hay)
	await h._settle()

	# ---- THE REOPENED SHEET, ON A BAND WITH ZERO IDLE ------------------------------------------
	# **THE PROOF THAT A FULLY-ALLOCATED BAND CAN STILL RESTATE ITS TAKE** (`docs/plan_standing_upkeep.md`
	# §2.5). A band with every hand committed publishes `idle_workers == 0`, so a stepper clamped at
	# `idle` alone would open at nobody with a maximum of nobody — the player could take a crew to
	# nothing and never put it back. `source_crew_pool_forage`'s standing term is what answers that, and
	# it is the half of the retired shared-pool pair that survives.
	#
	# **THE SHEET COMPOSES ONE CREW.** It carried three at the arc's widest — take, build and keeping —
	# and both the keeping and the build have left the tile for band-level standing roles, so what this
	# frame now proves about the other two is an ABSENCE: exactly one stepper is mounted.
	var reopened_tile := ForageFx.floorify(_reopened_patch_tile_fixture(),
		HudComposeVocab.FORAGE_FORECAST_PREFIX)
	var prior_reopen_band = h._hud._band_labor.player_band()
	var prior_reopen_bands = h._hud._band_labor._player_bands
	h._hud._band_labor._player_band = _reopened_patch_band_fixture()
	h._hud._band_labor._player_bands = [h._hud._band_labor.player_band()]
	h._hud._compose.reset_forage_source()
	h._show_tile(reopened_tile)
	h._compose_forage(reopened_tile)
	await h._settle()
	await h._save("forage_reopened_crews")
	var reopened_sheet = h._hud._drawercompose._compose_sheet
	# (0) THE FRAME REALLY IS THE REGIME. Without this every claim below is about an ordinary band
	# with hands to spare, which is the case that already worked.
	h._assert_hud("the band really has nothing idle, which is the state that was unreachable",
		int(h._hud._band_labor.player_band().get("idle_workers", -1)) == 0)
	# (1) **THE TAKE STEPPER OPENED ON THE BAND'S OWN CREW.** Its build twin went with the per-source
	# build allocation (§2.5), and the two counts are still deliberately unlike, so a seed that read
	# some other field lands on a number this assertion names.
	h._assert_hud("the take stepper opened on the band's take crew (%d)" % REOPEN_TAKE_CREW,
		h._hud._compose.forage_count() == REOPEN_TAKE_CREW)
	# (2) **AND IT CAN STILL BE RESTATED AT ZERO IDLE**, which is the whole point: the ceiling is
	# `idle + this source's own take crew`, so a band that has committed everything can be taken DOWN
	# and back UP to what it already has. Driven through the model the stepper writes — down one hand,
	# then back — because a clamp at `idle` alone would refuse the return and strand the crew at 3.
	h._hud._compose.set_forage_count(REOPEN_TAKE_CREW - 1)
	h._compose_forage(reopened_tile)
	await h._settle()
	h._hud._compose.set_forage_count(REOPEN_TAKE_CREW)
	h._compose_forage(reopened_tile)
	await h._settle()
	h._assert_hud("the take stepper returns to its seeded crew on a band with 0 idle",
		h._hud._compose.forage_count() == REOPEN_TAKE_CREW)
	# (3) **AND THE SHEET COMPOSES NEITHER A KEEPING NOR A BUILD CREW** (§2.5). This patch is short of
	# its keeping — the fixture states a live shortfall — and its rung is being built, so a sheet that
	# still mounted either stepper would mount it HERE if anywhere; both levers are band-level role
	# cards, and a stepper on the sheet would point the player at a command the sim no longer accepts.
	# The claim is an ABSENCE, so it is asserted rather than shown: the frame renders identically
	# either way except for the missing rows.
	h._assert_hud("the patch really is under-funded, which is where a keeping stepper would appear",
		SourceForecast.upkeep_is_short(SourceForecast.upkeep_state(
			reopened_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)))
	h._assert_hud("…and its rung really is being built, which is where a build stepper would appear",
		SourceForecast.build_is_in_flight(reopened_tile,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE,
			SourceForecast.IMPROVEMENT_CULTIVATE))
	# It COUNTS the steppers rather than looking for a retired tag: both rows' metas went with the
	# rows, so a restored one would carry no tag and a tag search would pass vacuously. A count cannot.
	h._assert_hud("…and the sheet still offers exactly ONE stepper — the take crew's",
		Readout.stepper_count(reopened_sheet) == Readout.COMPOSE_STEPPERS_PER_SHEET)
	# (4) **THE CROP SURVIVED THE REOPEN, over ground nobody has worked.** The patch carries no
	# `committed_species` — there is nothing for the ground to be committed to yet — so the
	# assignment's own `species` is the only record of what the player chose, and re-resolving to the
	# tile's dominant plant would silently re-point a 25-turn commitment.
	h._assert_hud("the composed crop is the one the band ASKED for, not the tile's dominant plant",
		h._hud._compose.forage_species() == REOPEN_SPECIES)
	h._assert_hud("…and the patch itself is committed to nothing, so the assignment is the only source",
		String(reopened_tile.get("patch_committed_species", "")) == "")
	h._hud._band_labor._player_band = prior_reopen_band
	h._hud._band_labor._player_bands = prior_reopen_bands
	h._hud._compose.reset_forage_source()
	h._show_tile(committed_hay)
	await h._settle()

	# ---------------------------------------------------------------------------------------------
	# States forage_at_floor / forage_below_floor — **THE SHEET QUOTES NEXT TURN'S TAKE.**
	#
	# Reported from play: a patch sitting on its floor, regrowing and being harvested back to it every
	# turn, read `PER TURN 0.00 FOOD` under *"takes nothing until it grows past N"* while the WORK
	# BOARD showed `+0.96 /turn` for the same tile. Both were right about different questions — the
	# board quotes the sim's forward projection and the sheet quoted the room standing RIGHT NOW,
	# `B − floor·K`, which on such a patch is empty by construction.
	#
	# **THE SIM REGROWS A WHOLE STAGE BEFORE IT HARVESTS** (`advance_forage_regrowth` is in Logistics,
	# `advance_labor_allocation` in Population), so what the crew banks next turn is
	# `min(crew, (B + growth) − floor·K)` — and at equilibrium that IS the regrowth, which is why the
	# headline reconciles with the board with nothing keeping the two in step by hand.
	#
	# **THE PAIR IS THE CLAIM.** A headline that had simply stopped answering zero would satisfy the
	# first state alone; the second is a patch far enough below its floor that next turn's growth does
	# not reach it, which really does pay nothing — and only there may the sheet say so.
	# **THE FACTION IS PUT BACK TO STILL-LEARNING CULTIVATION FOR THESE THREE STATES.** A wild patch
	# teaches Cultivation, and a source teaches nothing once the faction knows its lesson — so at the
	# all-complete dial the frames above leave behind, the aside's teaching line is correctly ABSENT
	# and the claim below (that it may not deny the take) would be asserting about nothing. Restored
	# at the end of the block, exactly as the chart block above restores it.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": FLOOR_CHART_CULTIVATION_LEARNING, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0}}])
	var at_floor := BaseFx.food_tile_fixture()
	at_floor["patch_biomass"] = SourceForecast.FLOOR_FOOD_PEAK \
		* float(at_floor["patch_carrying_capacity"])
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(AT_FLOOR_FORAGERS)
	h._compose_forage(at_floor)
	await h._settle()
	await h._save("forage_at_floor")
	# THE PRECONDITION, and it is the reported bug stated as a fact: the room standing now is EMPTY, so
	# the old headline had nothing to quote. Read through `ForageFx.floorify`'s own output, since that
	# is the dict the sheet was composed from.
	var at_floor_priced := ForageFx.floorify(at_floor, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._assert_hud("the patch is standing exactly ON its floor, so the room above it is empty",
		SourceForecast.escapement_room(at_floor_priced, HudComposeVocab.FORAGE_FORECAST_PREFIX,
			SourceForecast.FLOOR_FOOD_PEAK) <= 0.0
		and SourceForecast.escapement_room_next_turn(at_floor_priced,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK) > 0.0)
	# …and the headline states a LIVE number rather than that zero. Asserted as *not the zero* AND as
	# the presence of the account, because a readout that had stopped rendering rows at all satisfies
	# the negative on its own.
	var at_floor_text = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: at the floor  %s | %s" % [at_floor_text,
		Readout.verdict_text(h._hud._drawercompose._compose_sheet)])
	h._assert_hud("…so the headline states what NEXT turn pays, not the empty room (%s)"
			% at_floor_text,
		at_floor_text.contains(AT_FLOOR_ACCOUNT_NEEDLE)
			and not at_floor_text.contains(EMPTY_TAKE_NEEDLE))
	# **AND IT RECONCILES WITH THE HOLDING RATE, which is the whole reason the number is trustworthy.**
	# At equilibrium next turn's room IS the regrowth, so the take equals what the source pays while
	# held at this floor — the `hold_ceiling` reading the `after` half of an arrowed row is composed
	# from. The two are computed from different ceilings and must land on one number here.
	var held_take := SourceForecast.expected_yield_account(
		SourceForecast.forecast_inputs(at_floor_priced, SourceForecast.SOURCE_KIND_FORAGE,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK),
		AT_FLOOR_FORAGERS, h._hud._band_labor.player_band(), "per_worker", "hold_ceiling",
		SourceForecast.FORECAST_FOOD_PER_ANIMAL_KEY)
	h._assert_hud("…and it IS the rate this patch holds at that floor (%s)"
			% SourceForecast.format_magnitude(held_take),
		at_floor_text.contains(SourceForecast.format_magnitude(held_take)))
	# …and the verdict says the source is HOLDING rather than that the crew takes nothing. The refusal
	# is the sentence this state used to carry, so its absence here is half the claim.
	h._assert_hud("…while the verdict states it is holding, not that it is empty (%s)"
			% Readout.verdict_text(h._hud._drawercompose._compose_sheet),
		Readout.verdict_text(h._hud._drawercompose._compose_sheet).contains(
				SourceForecast.VERDICT_HOLDS_AT_FLOOR)
			and Readout.verdict_severity(h._hud._drawercompose._compose_sheet)
				== SourceForecast.VERDICT_OK)

	# **AND THE TEACHING LINE MAY NOT DENY THE TAKE THE HEADLINE IS QUOTING.** Reported from play
	# beside the pair above: a patch publishing `+0.71 /turn` rendered *"Teaching nothing: nothing is
	# being taken."* The predicate was `crew_is_taking`, which tests the room standing right now
	# against the wire's POST-take biomass — false by construction on any source held at its floor,
	# which is the intended steady state of a Sustain policy rather than an edge case. The sim was
	# meanwhile crediting the lesson at full multiplier off its pre-take stock, so BOTH halves of the
	# sentence were false.
	#
	# **THE PRECONDITIONS ARE THE CLAIM'S OTHER HALF, and there are three.** Without them this passes
	# on a source that is genuinely idle (nothing taken, so the sentence is true), on a rung that
	# teaches nothing at all (no line, so nothing to be wrong), and on a lesson already learned (also
	# no line). The at-floor precondition is asserted above; these add the take and the line.
	var at_floor_teaching := Readout.teaching_line(h._hud._drawercompose._compose_sheet)
	print("ui_preview: at the floor  teaching | %s" % at_floor_teaching)
	h._assert_hud("precondition: this patch is publishing a live take, %s foragers standing on its floor"
			% AT_FLOOR_FORAGERS,
		held_take > 0.0 and AT_FLOOR_FORAGERS > 0)
	h._assert_hud("precondition: the rung teaches something and the faction has not learned it, so a line renders",
		at_floor_teaching != "")
	h._assert_hud("…and it does NOT claim nothing is being taken (%s)" % at_floor_teaching,
		not at_floor_teaching.contains(SourceForecast.TEACHING_NOTHING_UNWORKED))
	# …and the two sentences agree, which is the structural half: both are keyed on
	# `escapement_room_next_turn` now, so a teaching line saying "nothing is taken" beside a verdict
	# saying "holding it — taking only what grows back" is no longer expressible.
	h._assert_hud("…it states the lesson it is earning, at the floor's own multiplier (%s)"
			% at_floor_teaching,
		at_floor_teaching.contains(SourceForecast.TEACHING_RATE_FORMAT % [
			SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_FORAGE][
				SourceForecast.IMPROVEMENT_NONE],
			SourceForecast.learn_multiplier(SourceForecast.FLOOR_FOOD_PEAK)]))

	# **THE HALF THAT KEEPS ZERO REACHABLE.** The same patch drawn far enough below its floor that one
	# turn's growth does not carry it back over: that crew really does bank nothing, and the sentence
	# it earns is the one the state above must never show.
	var below_floor := BaseFx.food_tile_fixture()
	below_floor["patch_biomass"] = BELOW_FLOOR_STOCK_FRACTION \
		* float(below_floor["patch_carrying_capacity"])
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(AT_FLOOR_FORAGERS)
	h._compose_forage(below_floor)
	await h._settle()
	await h._save("forage_below_floor")
	var below_priced := ForageFx.floorify(below_floor, HudComposeVocab.FORAGE_FORECAST_PREFIX)
	h._assert_hud("a patch a turn's growth cannot lift over its floor really has no room next turn",
		SourceForecast.escapement_room_next_turn(below_priced,
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK) <= 0.0)
	var below_text = Readout.yields_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: below the floor  %s | %s" % [below_text,
		Readout.verdict_text(h._hud._drawercompose._compose_sheet)])
	h._assert_hud("…so the headline states its zero (%s)" % below_text,
		below_text.contains(EMPTY_TAKE_NEEDLE))
	h._assert_hud("…and the *until it grows past* sentence is true exactly where it is shown (%s)"
			% Readout.verdict_text(h._hud._drawercompose._compose_sheet),
		Readout.verdict_text(h._hud._drawercompose._compose_sheet).contains(AT_FLOOR_REFUSAL_NEEDLE)
			and Readout.verdict_severity(h._hud._drawercompose._compose_sheet)
				== SourceForecast.VERDICT_BLOCKED)

	# **STATE forage_reaches_floor — THE COUNTDOWN STATES THE TURN COUNT AND NOTHING ELSE.**
	# It read *"Reaches the floor in N turns, then holds it — taking only what grows back."* The
	# aftermath clause is the `VERDICT_HOLDS_AT_FLOOR` sentence's own job, said by this same readout
	# the moment the source arrives, so a countdown that also narrated it answered a question the
	# player had not reached — and a STRIPPED twin of the sentence existed purely to drop that clause
	# where there was no aftermath to promise. Both twins went with it (`VERDICT_REACHES_FORMAT`).
	#
	# The precondition is that this really is the COUNTDOWN branch: the at-floor and holding verdicts
	# above carry "grows back" legitimately, so an absence claim made without knowing which branch
	# rendered is satisfied by every other state in this chapter.
	var descending := BaseFx.food_tile_fixture()
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_count(AT_FLOOR_FORAGERS)
	h._compose_forage(descending)
	await h._settle()
	await h._save("forage_reaches_floor")
	var descending_text := Readout.verdict_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: descending  %s" % descending_text)
	h._assert_hud("precondition: this crew is walking the patch DOWN to its floor (%s)"
			% descending_text,
		descending_text.contains(REACHES_FLOOR_HEAD))
	h._assert_hud("…and the sentence is the turn count alone — no aftermath clause (%s)"
			% descending_text,
		descending_text.ends_with(REACHES_FLOOR_TAIL)
			and not descending_text.contains(RETIRED_REACHES_AFTERMATH))

	# Put the faction's knowledge back where the block before this one left it.
	h._hud.update_intensification([{"faction": 0, "knowledges": {"cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0}}])

	# ---- THE ROW'S TWO RATES ARE NAMED, BOTH OF THEM (PNG-less) --------------------------------
	# Appended last, and it renders nothing: the claim is about a HOVER, which no frame carries, and
	# about a WORD, which every canned row in this harness was structurally unable to expose (see
	# `_assert_readout_names_both_rates`).
	_assert_readout_names_both_rates()
