extends RefCounted

## The forage accounts, the build dip and the floor chart.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## **HOW MANY UNTAGGED STEPPERS A COMPOSE SHEET CARRIES** — one, the TAKE crew's
## (`docs/plan_standing_upkeep.md` §2.2, §2.5). The build crew's row is tagged by its own meta and
## the keeping row is retired, so a second plain stepper on a sheet is a third allocation nobody
## should be able to compose.
const COMPOSE_PLAIN_STEPPERS_PER_SHEET := 1

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
	return {
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
	}

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
	return {
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
	}

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
	return tile

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
func _hay_meadow_tile_fixture() -> Dictionary:
	var tile := ForageFx.fodder_basket_tile_fixture()
	tile["x"] = 65
	tile["y"] = 9
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

## THE SAME MEADOW, COMMITTED TO ITS HAY — the half that pins `patch.species.is_some()`. The sim pays a
## committed patch's fodder whatever the faction knows (committing IS the bid), so without this state
## the whole set would pass as "gated on knowledge alone". Same ground, same rates, same coordinates:
## only the commitment moves, which is what makes it a controlled comparison with the locked frame.
##
## `hay_grass` is a member of this basket, and the display name rides with it because a committed
## species with no display name is a shape the wire never ships (the crop picker's locked readout
## reads both).
func _committed_hay_meadow_tile_fixture() -> Dictionary:
	var tile := _hay_meadow_tile_fixture()
	tile["patch_committed_species"] = "hay_grass"
	tile["patch_committed_display_name"] = "Hay Grass"
	return tile

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
## readout's own (it joins the suffix to the crew clause), and the fodder magnitude is UNSIGNED, which
## is `yield_components`' rule for this account and not an omission.
const FODDER_STANDING_SUFFIX := " 0.40 fodder"
## The same clause as the rendered drawer line carries it.
const FODDER_STANDING_CLAUSE := "0.40 fodder"
## The tooltip's fodder clause, which reuses the rung tooltips' own wording rather than spelling the
## account a third way — hence the sign and the unit that the compact face does without.
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
		# **THE CREW THAT WAS WRITE-ONLY.** A reader that treated its `0` as "unknown" would be caught
		# by the OTHER frames in this chapter, where it genuinely is zero; here it is a distinct
		# positive, so a reader that dropped it is caught too.
		"improvement_workers": REOPEN_BUILD_CREW,
		"target_x": 68, "target_y": 12, "floor": BUILD_DIP_FLOOR,
		"improvement": "cultivate",
		"species": REOPEN_SPECIES,
		"actual_yield": 0.0, "sustainable_yield": 0.0, "realized_yield": 0.0,
		"overdraws": false,
	}, {"kind": "agriculture", "workers": REOPEN_KEEP_CREW}]
	return band

## meadow is the frame's whole subject.
func _fodder_field_band_fixture() -> Dictionary:
	var band: Dictionary = BandFx.forage_range_bands()[0]
	band["labor_assignments"] = [_fodder_field_assignment()]
	return band

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
		stale_control is Label and not (stale_control is CheckBox)
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
	# (3) **THE ⚠ AND THE VERDICT NOW READ THE SAME PROJECTION.** The take is well past the food-peak
	# ceiling (which is zero on a patch standing at the peak), so the per-account test still fires and
	# the gate is the only thing suppressing it — and what the gate reads is the stock CLIMBING.
	var build_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK, BUILD_DIP_CAPACITY,
		BUILD_DIP_FLOOR, float(BUILD_DIP_CREW) * build_carry)
	h._assert_hud("the projection this crew produces RISES — there is nothing being overdrawn (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(build_walk["settled_fraction"])],
		float(build_walk["settled_fraction"]) > BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	h._assert_hud("…so no overdraw flag fires beside a verdict saying the patch grows",
		not h._hud._drawercompose._local_forage_preview_bbcode(h._hud._band_labor.player_band(),
			building_tile, BUILD_DIP_FLOOR, BUILD_DIP_CREW, "cultivate").contains(HudStyle.WARN_HEX))
	# (4) **THE BUILD STATES ITS OWN CREW** (`docs/plan_standing_upkeep.md` §2.2). The retired dip note
	# used to stand on the take crew's row explaining why those foragers carried a quarter; what stands
	# there now is a second stepper, on the improvement control, for the hands actually doing the
	# build. Asserted by PRESENCE, and by the ABSENCE of any carry claim on the take crew's label.
	h._assert_hud("a live build carries its own crew row under the verb",
		Q.find_meta_node(build_sheet, HudWidgets.BUILD_CREW_ROW_META) != null)
	h._assert_hud("…and the take crew's row claims no carry penalty beside its label",
		not Readout.crew_row_label(build_sheet).contains("%"))

	# State forage_build_crew_decline — **THE OTHER HALF OF THE GATE, one hand apart.** Seven foragers
	# out-carry the patch's fastest regrowth, so the same patch at the same floor now genuinely falls
	# to the line — and the ⚠ must come back. Without this frame the assertion above passes vacuously
	# on a gate that suppressed the flag everywhere.
	h._hud._compose.set_forage_count(BUILD_DIP_DECLINE_CREW)
	h._compose_forage(building_tile)
	await h._settle()
	await h._save("forage_build_crew_decline")
	var decline_walk := SourceForecast.project_stock(build_samples, BUILD_DIP_STOCK,
		BUILD_DIP_CAPACITY, BUILD_DIP_FLOOR, float(BUILD_DIP_DECLINE_CREW) * build_carry)
	h._assert_hud("one more hand out-carries the regrowth, and the projection FALLS (%.3f → %.3f)"
		% [BUILD_DIP_STOCK / BUILD_DIP_CAPACITY, float(decline_walk["settled_fraction"])],
		float(decline_walk["settled_fraction"]) < BUILD_DIP_STOCK / BUILD_DIP_CAPACITY)
	h._assert_hud("…and the overdraw flag fires there, so the gate subtracts rather than silences",
		h._hud._drawercompose._local_forage_preview_bbcode(h._hud._band_labor.player_band(),
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
	h._assert_hud("…and the sheet offers no Cultivate box on it",
		not (ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate") is CheckBox))
	# **THE PAIR THAT STOPS THIS BECOMING "CULTIVATE IS NEVER OFFERED".** A retire test that answered
	# true unconditionally would satisfy every line above; a wild patch with the knowledge in hand must
	# still offer the rung.
	h._assert_hud("…while a WILD patch still offers Cultivate — the rung is retired, not deleted",
		not SourceForecast.improvement_is_done(BaseFx.food_tile_fixture(),
			HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.IMPROVEMENT_CULTIVATE))
	h._hud._compose.reset_forage_source()

	# ---- THE PLANT WEB'S CREW NOUN FOLLOWS THE STANDING RUNG -------------------------------------
	# Reported from play: every surface for a sown Field still said *forage* / *Foragers*. The ladder
	# config is the authority — `wild` declares the harvest primitive `worker_take`, `tended` and
	# `field` both declare `worker_tend` — so a managed source's crew are TENDERS and only a wild
	# stand's are FORAGERS. `HudFormat.plant_crew_label` is the one resolver; these four states drive
	# the four surfaces it feeds (sheet eyebrow, crew-row label, commit button, drawer open button)
	# and, on every frame, assert the eyebrow and the stepper AGREE — the disagreement being the
	# failure the single resolver exists to make unexpressible.
	await _assert_plant_crew_noun("plant_crew_wild", BaseFx.food_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL)
	await _assert_plant_crew_noun("plant_crew_tended", TileFx.tended_tile_fixture(),
		HudComposeVocab.TEND_CREW_LABEL)
	# **BOTH UPPER RUNGS, NOT ONE.** A Tended Patch answers through `patch_is_cultivated` and a Field
	# sown from wild ground through `patch_is_field` + `FORECAST_RETIRED_BY_HIGHER_RUNG` — two
	# different flags reaching one noun, so a resolver that read only the first would pass above and
	# fail here (`_wild_sown_field_tile_fixture` is the Field that was never cultivated).
	await _assert_plant_crew_noun("plant_crew_field", _wild_sown_field_tile_fixture(),
		HudComposeVocab.TEND_CREW_LABEL)
	# **THE CASE A NAIVE "IS AN IMPROVEMENT COMPOSED?" TEST GETS WRONG.** These people are foraging the
	# wild stand AND clearing ground — which is exactly what the build dip charges them for — so the
	# noun must not move until the rung COMPLETES. `_building_patch_tile_fixture` is wild ground with
	# `cultivation_progress` part-way and `is_cultivated` false, and the compose carries the verb.
	await _assert_plant_crew_noun("plant_crew_wild_building", _building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL, SourceForecast.IMPROVEMENT_CULTIVATE)
	# …and its Sow twin, on the same wild ground: `Sow` needs no prior patch, so a Sow in flight is the
	# other half of "a build is running here" and must read identically.
	await _assert_plant_crew_noun("plant_crew_wild_sowing", _building_patch_tile_fixture(),
		HudComposeVocab.FORAGE_CREW_LABEL, SourceForecast.IMPROVEMENT_SOW)
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
		"foddering": 1.0,
	}])
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
	h._assert_hud("…and a row with no transition is given a header with no arrow to key",
		Readout.yields_header(h._hud._drawercompose._compose_sheet).contains("PER TURN")
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
	# What the frame still pins is that the verdict tracks the FLOOR: the same crew reads amber below
	# the food peak and green at it. The crew size is load-bearing and deliberately not the auto-max —
	# below ~7 foragers LABOR binds under every ceiling and the honest verdict is renewable at every
	# floor, so a small-crew frame would pass this state's claim vacuously.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_MIN)
	h._hud._compose.set_forage_count(HAY_OVERDRAW_FORAGERS)
	h._compose_forage(hay_meadow)
	await h._settle()
	await h._save("forage_three_accounts_overdraw")
	h._assert_hud("a crew past the food peak's room overdraws — the verdict tracks the floor",
		h._hud._drawercompose._local_forage_preview_bbcode(
			h._hud._band_labor.player_band(), hay_meadow, SourceForecast.FLOOR_MIN, HAY_OVERDRAW_FORAGERS)
			.contains(HudStyle.WARN_HEX))
	h._assert_hud("the same crew on the rung that protects the patch reads renewable",
		not h._hud._drawercompose._local_forage_preview_bbcode(
			h._hud._band_labor.player_band(), hay_meadow, SourceForecast.FLOOR_FOOD_PEAK, HAY_OVERDRAW_FORAGERS)
			.contains(HudStyle.WARN_HEX))

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
	# so the two crew targets have no denominator and must be ABSENT rather than rendered as a zero
	# saying "nobody is needed" — while the chart still draws, the patch's stock, its floor and its
	# growth curve all being real facts about the ground.
	h._assert_hud("a dead-season patch prices no crew target rather than dividing by a zero throughput",
		Readout.crew_target_count(h._hud._drawercompose._compose_sheet, HudWidgets.CREW_TARGET_CLEAR)
			== Readout.CREW_TARGET_ABSENT)
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": FLOOR_CHART_CULTIVATION_LEARNING, "herding": 1.0,
		"seed_selection": 1.0, "penning": 1.0, "foddering": 1.0,
	}])

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
	h._assert_hud("a crew that reaches the floor states what it takes NOW and what it holds AFTER",
		burst_text.contains("0.22 → 0.06") and burst_text.contains("0.07 → 0.02"))
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
		burst_header.contains("PER TURN") and not burst_text.contains("/TURN"))
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
		"foddering": 1.0,
	}])
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
		"foddering": FODDER_LOCK_PROGRESS,
	}])
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
		"foddering": 1.0,
	}])
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
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
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])
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
	# Hand the chapter's subject back, so the states after this one open on the tile they were written
	# against rather than on a patch nobody after them works.
	h._hud._compose.reset_forage_source()
	h._show_tile(committed_hay)
	await h._settle()

	# ---- THE REOPENED SHEET, ON A BAND WITH ZERO IDLE ------------------------------------------
	# **THE PROOF THAT BOTH OF A SOURCE'S CREWS ARE READABLE** (`docs/plan_standing_upkeep.md` §2.2).
	# Before `improvement_workers` reached the wire, this exact state was a dead end: a
	# fully-allocated band publishes `idle_workers == 0`, the build stepper could only clamp at
	# `idle`, and it opened at nobody with a maximum of nobody — the player could unstaff a build crew
	# and never restore it. The frame is that sheet, reopened, with its real crews and usable
	# steppers, and with NO keeping row on it at all (§2.5).
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
	# (1) **EACH STEPPER OPENED ON ITS OWN CREW.** Two distinct counts, so a seed that read the
	# wrong field lands on a number this assertion names.
	h._assert_hud("the take stepper opened on the band's take crew (%d)" % REOPEN_TAKE_CREW,
		h._hud._compose.forage_count() == REOPEN_TAKE_CREW)
	h._assert_hud("…and the BUILD stepper on its builders (%d), not on nobody" % REOPEN_BUILD_CREW,
		h._hud._compose.forage_build_count() == REOPEN_BUILD_CREW)
	# (2) **AND THE BUILD `+` IS STILL LIVE AT ZERO IDLE**, which is the whole point: the ceiling is
	# `idle + this source's own build crew`, so a restate is possible on a band that has committed
	# everything. Driven through the model the stepper writes, at one hand above the seed.
	h._hud._compose.set_forage_build_count(REOPEN_BUILD_CREW + 1)
	h._compose_forage(reopened_tile)
	await h._settle()
	h._assert_hud("the build stepper reaches past its seeded crew on a band with 0 idle",
		h._hud._compose.forage_build_count() == REOPEN_BUILD_CREW + 1)
	h._hud._compose.set_forage_build_count(REOPEN_BUILD_CREW)
	h._compose_forage(reopened_tile)
	await h._settle()
	# (3) **AND THE SHEET COMPOSES NO KEEPING CREW AT ALL** (§2.5). This patch is short of its keeping
	# — the fixture states a live shortfall — so a sheet that still mounted a keeping stepper would
	# mount it HERE if anywhere; the lever is the band's `agriculture` role, and a stepper on the
	# sheet would point the player at a command the sim no longer accepts. The claim is an ABSENCE, so
	# it is asserted rather than shown: the frame renders identically either way except for one row.
	h._assert_hud("the patch really is under-funded, which is where a keeping stepper would appear",
		SourceForecast.upkeep_is_short(SourceForecast.upkeep_state(
			reopened_tile, HudComposeVocab.FORAGE_FORECAST_PREFIX)))
	# It counts the STEPPERS structurally rather than looking for a retired label: the keeping row's
	# meta went with the row, so a restored one would carry no tag and a tag search would pass
	# vacuously. The spine walk tags the build row by its own meta and every other stepper as a plain
	# `stepper`, so a third allocation on this sheet shows up as a second plain tag.
	var reopened_spine := Spine.compose_spine(reopened_sheet)
	h._assert_hud("…and the sheet still offers exactly ONE plain stepper — the take crew's",
		reopened_spine.count(Spine.COMPOSE_SPINE_STEPPER) == COMPOSE_PLAIN_STEPPERS_PER_SHEET
		and reopened_spine.has(Spine.COMPOSE_SPINE_BUILDERS))
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
