extends RefCounted

## WORKINGS — the three surfaces the two deposit branches are read and worked from (arc #583,
## issue #650, `docs/plan_extraction.md` §7, `.claude/rules/client/extraction-workings.md`).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. It is appended LAST for exactly that reason. See
## `.claude/rules/client/test-harnesses.md`.
##
## ⛔ **THE `Workings ▸` POPUP IS GONE, AND SO IS EVERY FRAME OF IT.** It was a fourth UX pattern for a
## branch whose three surfaces already ship, and what this chapter walks now is those three: the tile
## card's per-material ROWS, the two COMPOSE SHEETS, and the shared `RungLadder` TRACK.
##
## ⛔ **THE CLAIMS ARE THE §7 FORK, THE TWO-MATERIAL KEY AND THE OPENING STATE, and none of the three
## can be shown by one frame.** `regrowth_rate > 0` decides which sentence a working publishes — the
## over-cut pair against the runway — and a flint scatter and a quarry are both `extraction`, so a
## chapter that staged one renewing and one finite working would pass on a client forking on `branch`.
## A tile holds up to TWO deposits on one `(tile, material)` key each, which is the one claim a
## tile-keyed surface cannot make. And a ROW IS NOT A WORKING: the section stands a row on every
## discovered deposit-bearing tile, so untouched ground is the state a player meets first.
##
## ⛔ **A RENDERED FIXTURE FRAME IS EVIDENCE ABOUT LAYOUT.** It says nothing about what a player can
## reach in a real game; every claim here is about what a surface SAYS, made against the shipped
## composers and the real controls.

## The shared readers this chapter walks the surfaces with — the same set every other chapter uses, so
## a control is found the same way here as there.
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 118

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The hex every state in this chapter stands on. Its own coordinates, so a working staged here can
## never be confused with the road frames' tile.
const WORKING_TILE_X := 21
const WORKING_TILE_Y := 14

## ⛔ **EVERY BAND IN THIS CHAPTER STANDS ON THE WORKING'S OWN HEX, AND SINCE issue #650 IT HAS TO.**
## `BandFx.band_fixture()` camps at (71,18) with a `work_range` of 2, some fifty tiles from this
## chapter's ground — which cost nothing while the deposit sheets measured no distance, and refuses
## every one of them now that they do. A sheet whose commit is dead for a reason no state here is
## about would make each of the claims below a claim about the refusal instead.
##
## **A DISTANCE OF ZERO rather than one inside the range**, because the sheet states no distance when
## it is happy and there is nothing to be gained from a number the frames cannot see. The one state
## that IS about the range stands its band far away on purpose (`_band_beyond_reach`).
func _band_at_the_working() -> Dictionary:
	var band := BandFx.band_fixture()
	band["current_x"] = WORKING_TILE_X
	band["current_y"] = WORKING_TILE_Y
	return band

## …and the same band left where the shared fixture camps it: out of reach of this chapter's hex, so
## the refusal is the sheet's own arithmetic on the shipped `work_range` rather than a distance this
## file asserts. Only `workings_out_of_range` uses it.
func _band_beyond_reach() -> Dictionary:
	return BandFx.band_fixture()

## **A WOODED HIGHLAND'S TWO SEAMS**, at the shipped `extraction.json` proportions: mixed woodland
## carries 600 units of wood and renews 3% of it a turn, a karst body 2200 units of rock and renews
## none at all. Transcribed rather than rounded, so the frames state the real ground.
const WOOD_CAPACITY := 600.0
const WOOD_REGROWTH := 0.03
const STONE_CAPACITY := 2200.0
const STONE_REGROWTH := 0.0

## What is standing on each seam — drawn down, so the row states the `stock of capacity` PAIR rather
## than the single figure a full seam reads as.
const WOOD_STOCK := 412.0
const STONE_STOCK := 2100.0

## ⛔ **THE PATH THE ORDER FRAMES STAND A ROAD ON — the FLOOR rung, part-worn toward the next.**
## `0.14` is the meter Ray's own card read (`Path · 14% to trail`), so the frame states the readout he
## was looking at when he asked for the row to move.
##
## **IT HELPS NOBODY AND OWES NOBODY, DELIBERATELY.** At `ROAD_FRICTION_NO_HELP` the block emits no
## payoff row and with a zero bill no `Upkeep` or `Reverting` row either, so the road is exactly ONE
## line — which is what lets these frames assert *the road is the LAST line* by index rather than by
## a tail-scan whose own correctness would need arguing. The road block's other four rows are already
## walked by `land_readouts.gd`'s eleven road frames; what is under test here is only WHERE the block
## lands.
const ROAD_PATH_METER := 0.14

## No band has graded this path, which is the whole free floor's normal state — the `has_keeper` bool
## beside it is what the client actually reads, `0` being a real `BandId`.
const ROAD_NO_KEEPER := -1

## ⛔ **`reachable` IS WHAT THE CURRENT RUNG CAN GET AT, and on the stone seam it is a FRACTION of the
## body** — `extraction:gathering`'s `recovery_fraction` is 0.15, so a surface picker on a 2200-unit
## rock body reaches 330 of it and no more. **That gap left the tile card with the popup** and is the
## compose sheet's VERDICT now, which is where this fixture's numbers are asserted.
const WOOD_REACHABLE := 412.0
const STONE_REACHABLE := 330.0

## …and the same body once a quarry stands on it: `recovery_fraction` 0.85 of 2200.
const QUARRY_STOCK := 1870.0
const QUARRY_REACHABLE := 1870.0

## ⛔ **A QUARRY WORKED DOWN PAST THE OMITTED-TOKEN DEFAULT, WHICH IS THE ONE GROUND THE CLIENT'S
## COMPOSITION USED TO DIVERGE ON** (issue #650). A finite seam is offered no dial, so the sheet
## composes at `SourceForecast.DEFAULT_HARVEST_FLOOR` — 0.5, the value an omitted command token
## resolves to sim-side — and an unconditional `max` let that bind ABOVE `extraction:quarry`'s own
## 0.15 rung floor. The sim stopped composing the crew's half on ground that never renews; the client
## composes it in ONE place (`HudDepositVocab.composed_floor`) and had to follow.
##
## **THE STOCK IS CHOSEN TO LAND BETWEEN THE TWO FLOORS**, which is what makes the divergence
## VISIBLE rather than merely wrong: `0.15 × 2200` = 330 of rock stands below it and `0.5 × 2200`
## = 1100 stands above it. Composed at the rung's own floor the seam has 370 units left to cut;
## composed at the default it has NONE, and the sheet quotes a take of nothing and a cap of nobody
## on a quarry the sim will happily work for another fifty turns.
const WORKED_QUARRY_STOCK := 700.0
## `stock − rung_floor_fraction × capacity`, the sim's own `deposit_reachable` at a crew that named
## no floor — and the figure `room_next_turn` must reproduce, rock's curve being all zeros so the
## growth term is nothing.
const WORKED_QUARRY_REACHABLE := 370.0
## `ceil(370 / 2.2)` — the most cutters the seam can use, `max_useful_cutters`' own division.
const WORKED_QUARRY_MAX_CUTTERS := 169
## The crew the band has on it, and the take it lifts: `3 × QUARRY_PER_WORKER`, comfortably inside
## the room above, so what the sheet quotes is the CREW's arithmetic rather than a clamp.
const WORKED_QUARRY_CUTTERS := 3
const WORKED_QUARRY_TAKE := 6.6
## `floor(370 / 6.6)` — the sim's own runway at that crew, passed through verbatim like `STONE_RUNWAY`.
const WORKED_QUARRY_RUNWAY := 56

## **A DIAL DRIVEN BELOW THE GATHERING RUNG'S OWN FLOOR AND ABOVE IT** — the pair that pins the fix
## NARROW. On the renewing scatter the `max` must still compose exactly as it did: the rung's 0.85
## wins at the default, and a player floor of 0.9 wins over the rung.
const SCATTER_DEEP_FLOOR := 0.9

## **THE OVER-CUT PAIR.** `sustainable_take` is the deposit's MSY — the growth term at the PEAK of its
## curve, never at today's stock — which is why one figure serves a drawn-down seam and a full one
## alike; `actual_take` is what the crews paid out last turn. The wood seam is being cut HARDER than
## it renews, which is the state §7's renewable warning exists for.
const WOOD_SUSTAINABLE := 4.5
const WOOD_OVER_CUT := 7.2

## …and the same seam cut inside its own renewal, which must say NOTHING. A clause on every renewing
## working is a row spent on the absence of news.
const WOOD_SAFE_TAKE := 2.0

## **A FINITE WORKING'S RUNWAY**, `floor(reachable / actual_take)` sim-side and passed through
## verbatim. Stated rather than derived here: the client reads the sim's own answer, so a fixture that
## computed it would be asserting the client against a second derivation of the same quantity.
const STONE_TAKE := 4.4
const STONE_RUNWAY := 75

## The wire's own two runway negatives (`sim_schema::{DEPOSIT_RUNWAY_NOT_APPLICABLE,
## DEPOSIT_RUNWAY_NO_TAKE}`), read back off the vocab rather than typed, so a fixture cannot state a
## sentinel the reader does not know.
const RUNWAY_IDLE := HudDepositVocab.RUNWAY_NO_TAKE
const RUNWAY_RENEWS := HudDepositVocab.RUNWAY_NOT_APPLICABLE

# ---- THE ESCAPEMENT FLOOR'S FOUR APPENDED FIELDS (issue #650) ------------------------------------
#
# Every figure here is `intensification_ladder.json` / `extraction.json` at the values that ship, so a
# claim made about the sheet is a claim about the wire and not about a table this chapter invented.

## `1 − recovery_fraction`, per shipped rung. **EVERY FORESTRY RUNG REACHES THE WHOLE SEAM** (recovery
## 1.0), which is what makes the composition's `max` a no-op on the branch that gets the dial and
## load-bearing on the renewing EXTRACTION ground beside it — the scatter below, whose gathering rung
## strands 85% of the stone before the player's dial is consulted at all.
const FORESTRY_RUNG_FLOOR := 0.0
const GATHERING_RUNG_FLOOR := 0.85
const QUARRY_RUNG_FLOOR := 0.15

## `yield_per_worker_turn` per shipped rung — what ONE cutter moves in a turn, which the wire
## publishes on the working as `perWorkerBiomass` rather than leaving the client to read off the
## catalog.
const FELLING_PER_WORKER := 2.0
const COPPICE_PER_WORKER := 2.5
const GATHERING_PER_WORKER := 0.4
const QUARRY_PER_WORKER := 2.2

## `extraction.json`'s `seed_fraction` and the wire's curve resolution — the two terms the sampled
## curve below is built from, so a fixture curve is the sim's own arithmetic rather than a shape.
const DEPOSIT_SEED_FRACTION := 0.02
const REGROWTH_SAMPLE_COUNT := 11

## **A FLINT SCATTER: `extraction` AND RENEWING.** Rock's rate is zero and a loose-stone scatter's is
## not, so this row shares the quarry's branch and must take the OVER-CUT arm — the one fixture that
## fails a client forking on `branch` and passes every claim a same-branch pair would make.
##
## ⛔ **ITS CAPACITY IS BELOW `extraction:quarry`'s SITE THRESHOLD**, which is the second thing it is
## for: a 70-unit scatter is exactly the ground the placement rule refuses a quarry on.
const SCATTER_CAPACITY := 70.0
const SCATTER_STOCK := 48.0
const SCATTER_REGROWTH := 0.02
const SCATTER_SUSTAINABLE := 0.9
const SCATTER_OVER_CUT := 2.1

## THE STANDING BILL of a felling working at home — `intensification_ladder.json`'s
## `forestry:felling.upkeep.work_per_turn`, transcribed, against a pool that is not covering it.
const WOOD_UPKEEP_DEMAND := 1.0
const WOOD_UPKEEP_SUPPLIED := 0.4
const WOOD_UPKEEP_SHORTFALL := 0.6
const WOOD_UPKEEP_KEEPERS := 1

## …and the neglect countdown that rides beside it. `forestry:felling` forgives three turns.
const WOOD_GRACE_LEFT := 2

## A free floor owes NOTHING, which is the whole of what makes it free — so its keeping figures must
## not reach any surface rather than reaching one as a bill of zero.
const FREE_FLOOR_NO_UPKEEP := 0.0

## The meter on the rung being RAISED. `1.0` is the wire's own *nothing is rising* — published exactly
## for a rung just finished AND for the top of a branch — so a fixture uses it wherever it wants no
## climb stated.
const METER_NOTHING_RISING := 1.0

## …and a real climb, part way to the coppice above.
const METER_CLIMBING := 0.42

## The build countdown for a working nobody has ordered. **`-1`, never `0`** — a zero renders as a
## finished build.
const BUILD_NO_ESTIMATE := -1

## …and the sim's chained countdown for one that IS queued, which is what the ladder's mid-build row
## quotes instead of a price.
const BUILD_TURNS_QUEUED := 18

## **WHAT A FREE FLOOR REACHES OF AN UNTOUCHED SEAM**: `forestry:deadfall`'s `recovery_fraction` is 1.0
## (fallen wood is all there is to gather) and `extraction:gathering`'s is 0.15.
const UNOPENED_WOOD_REACHABLE := WOOD_CAPACITY
const UNOPENED_STONE_REACHABLE := 330.0

## **NOTHING CAME OUT OF UNTOUCHED GROUND LAST TURN** — `extraction::NO_TAKE_THIS_TURN`, and the one
## take figure an unopened row really does zero. ⛔ `sustainable_take` is NOT zeroed with it: it is the
## deposit's MSY, read at the peak of the curve rather than at today's stock.
const UNOPENED_NO_TAKE := 0.0

## The crew this chapter dials on a compose sheet, so the deal row and the take are quoted at a real
## number rather than at the seeded one.
const SHEET_CREW := 3

## The faction knowledge this chapter's ladder states are gated against — `Quarrying` unlearned, which
## is what puts the CRAFT gate on the quarry row, and Woodcraft in hand so the felling row is priced
## and pressable rather than refused for a second reason.
const KNOWLEDGE_LEARNED := 1.0
const KNOWLEDGE_UNLEARNED := 0.35

## ⛔ **THE CREW EVERY OTHER LADDER CLAIM IS MADE AT, and it is stated rather than defaulted.** The CREW
## gate refuses every ordered rung on a working nobody holds, so a track asked at zero would render
## `no crew` on the very rows the site, craft, ground and price claims below are about — each of them
## would then pass or fail for a reason that has nothing to do with what it names. One hand is enough:
## the gate forks on `> 0` and nothing else here divides by it.
const LADDER_CUTTERS := 1

## …and the state that gate is FOR — a working the roster lists at nobody, which is the shape
## `_workings_band_fixture`'s far row already stages on the band panel.
const LADDER_NO_CUTTERS := 0

func run(harness) -> void:
	h = harness
	# ⛔ **THE CHAPTER STAGES ITS OWN BAND, AND WITHOUT ONE EVERY SHEET CLAIM IS ABOUT A CREW OF
	# ZERO.** The compose sheet caps its stepper at the acting band's own hands, and this chapter runs
	# LAST — after twenty-five others, whichever roster the previous one left standing. A band with no
	# idle worker clamps the count to 0, which renders a perfectly ordinary sheet: no take, no deal
	# row, and the pointer line's *send crews here first* arm instead of its live one.
	h._hud.update_band_alerts([_band_at_the_working()])
	# **THE CATALOG IS PER WORLD AND EVERY SURFACE HERE READS IT** — the rung's name, its price, what
	# it buys and every gate. Pushed through the real ingest so a claim made below is a claim about
	# the wire rather than about a table this chapter holds.
	h._hud.update_deposit_rungs(_deposit_catalog())
	# **AND THE KNOWLEDGE ROSTER BESIDE IT** — the `{knowledge_id: display_name}` map every craft on
	# these branches is NAMED from, both in a gate's refusal and in the sheet's teaching line. It is
	# pushed through the real ingest for the catalog's reason: a name this chapter typed for itself
	# would prove nothing about the wire.
	h._hud.update_ladder_knowledge(_deposit_knowledge_roster())
	await h._settle()

	# ⛔ **STATE workings-tile-card — ONE ROW PER MATERIAL, AND THAT IS THE KEY SPEAKING.** The
	# registry is keyed `(tile, material)`: a wooded highland carries timber AND rock, and working one
	# is not working the other. A surface that rendered one row per TILE is the defect the whole
	# `material` field exists to prevent.
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	var ctx := DetailFormat.Context.new()
	var lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info(), ctx)
	h._assert_hud("a hex with two seams draws ONE ROW PER MATERIAL, each keyed by its own material",
		Readout.detail_row_index(lines, "Wood") >= 0
			and Readout.detail_row_index(lines, "Stone") >= 0)
	# ⛔ **THE ROW LEADS WITH THE STOCK AND THE RUNG IS A QUALIFIER** — the card's subject is the
	# GROUND, so what is standing here comes first and what stands on it follows.
	# ⛔ **AND THE CREW SITS BETWEEN THEM, AT THE STATED ZERO** (issue #650). This band holds the
	# working and has taken its hands off it, which is a different fact from untouched ground and the
	# one the sim charges for: the bill goes on coming out of `quarrywork` either way. The zero form
	# is parallel to the staffed one for the forage land row's own reason — *nobody is on this* reads
	# at a glance instead of needing a comparison with a row that has a number.
	h._assert_hud("…the wood row reads `stock of capacity · rung · crew · hazard` (%s)"
			% Readout.detail_row_value(lines, "Wood"),
		Readout.detail_row_value(lines, "Wood") == HudDepositVocab.DEPOSIT_CLAUSE_SEPARATOR.join([
			HudDepositVocab.DEPOSIT_STOCK_DRAWN_FORMAT % [
				DetailFormat.format_trimmed(WOOD_STOCK, HudDepositVocab.CARD_STOCK_DECIMALS),
				DetailFormat.format_trimmed(WOOD_CAPACITY, HudDepositVocab.CARD_STOCK_DECIMALS)],
			CATALOG_FELLING_NAME,
			HudDepositVocab.crew_clause(_wood_working(WOOD_OVER_CUT), HudDepositVocab.CUTTERS_NONE),
			HudDepositVocab.DEPOSIT_HAZARD_CLAUSE_FORMAT % [
				HudSelectionVocab.RUNG_HAZARD_GLYPH, HudDepositVocab.DEPOSIT_UNDER_KEPT_WORD]]))
	# ⛔ **THE RUNG'S NAME IS THE CATALOG'S OWN WORD**, never a client table: `RUNG_LABELS` is retired,
	# so a rung added to `intensification_ladder.json` names itself here with no client edit. The
	# negative is what makes the claim mean something — the old table spelled the quarry rung
	# `Quarry face`, which no catalog carries.
	h._assert_hud("…and the rung is named by the WIRE, not by a retired client table",
		Readout.detail_row_value(lines, "Wood").contains(CATALOG_FELLING_NAME))
	# ⛔ **NO UPKEEP ROW, NO COUNTDOWN AND NO SHORTFALL FIGURE ON THIS CARD.** The hazard is a WORD;
	# the figures ride the block's hover, which is the whole of what "retired" means here.
	h._assert_hud("…and no bill, countdown or shortfall FIGURE reaches the card at all",
		Readout.detail_row_index(lines, HudRouteVocab.ROAD_UPKEEP_ROW) < 0
			and not "\n".join(lines).contains(UPKEEP_FACE_NEEDLE)
			and not "\n".join(lines).contains(HudDepositVocab.reverting_value(
				_wood_working(WOOD_OVER_CUT))))
	h._assert_hud("…while the block's hover carries the bill AND the §7 figures (%s)"
			% DetailFormat.block_tooltip(ctx),
		DetailFormat.block_tooltip(ctx).contains(DetailFormat.format_work_units(
				WOOD_UPKEEP_SHORTFALL))
			and DetailFormat.block_tooltip(ctx).contains(
				DetailFormat.format_trimmed(WOOD_SUSTAINABLE, HudDepositVocab.CARD_STOCK_DECIMALS)))
	# ⛔ **THE COUNTDOWN LIVES ON THE WORK BOARD'S HOVER AND NOWHERE ELSE.**
	h._assert_hud("…and the neglect countdown is on the roster's hover and not on this one",
		HudDepositVocab.deposit_roster_tooltip(_wood_working(WOOD_OVER_CUT)).contains(
				HudDepositVocab.reverting_value(_wood_working(WOOD_OVER_CUT)))
			and not DetailFormat.block_tooltip(ctx).contains(
				HudDepositVocab.reverting_value(_wood_working(WOOD_OVER_CUT))))
	# **A ROW THAT WOULD SAY "none" IS NOT RENDERED.** Neither of these rungs buys anything over its
	# branch's free floor, so the payoff row is absent — at the free floor a material is ONE row.
	h._assert_hud("…and neither rung buys anything over its floor, so no payoff row is drawn",
		Readout.detail_row_index(lines, HudDepositVocab.DEPOSIT_PAYOFF_ROW) < 0)
	await h._save("workings_tile_card")

	# ⛔ **STATE workings-payoff-rows — THE BLANK-KEY ROW, AND ITS KEY IS BLANK RATHER THAN ABSENT.**
	# A quarry reaches 85% of a body a surface picker reaches 15% of, and a coppice renews twice as
	# fast; those are the arguments the ladder exists to make, and this is the one row on the card
	# that states a payoff.
	h._show_tile(_workings_tile([_coppiced_wood(), _quarried_stone()]))
	await h._settle()
	var payoff_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	h._assert_hud("a built quarry states its stock against the BODY, under the catalog's own name (%s)"
			% Readout.detail_row_value(payoff_lines, "Stone"),
		Readout.detail_row_value(payoff_lines, "Stone")
			== HudDepositVocab.DEPOSIT_CLAUSE_SEPARATOR.join([
				HudDepositVocab.DEPOSIT_STOCK_DRAWN_FORMAT % [
					DetailFormat.format_trimmed(QUARRY_STOCK, HudDepositVocab.CARD_STOCK_DECIMALS),
					DetailFormat.format_trimmed(STONE_CAPACITY,
						HudDepositVocab.CARD_STOCK_DECIMALS)],
				CATALOG_QUARRY_NAME,
				HudDepositVocab.crew_clause(_quarried_stone(),
					HudDepositVocab.CUTTERS_NONE)]))
	# ⛔ **THE BLANK KEY IS SEARCHED FOR ACROSS THE BLOCK, NOT TAKEN AS THE FIRST MATCH.** A hex
	# carrying two RAISED deposits emits two payoff rows, both keyed `" "` — which is exactly what this
	# state stages — so a reader that took `detail_row_value`'s first hit would answer the wood's
	# sentence for the stone's row and pass or fail for the wrong reason.
	h._assert_hud("…and the payoff row beneath it says what the rung REACHES (%s)"
			% str(_payoff_values(payoff_lines)),
		_payoff_values(payoff_lines).has(HudDepositVocab.DEPOSIT_PAYOFF_REACH_FORMAT % int(round(
			CATALOG_QUARRY_RECOVERY * HudDepositVocab.DEPOSIT_RECOVERY_PERCENT_SCALE))))
	# **THE COPPICE'S PAYOFF IS THE OTHER AXIS ENTIRELY** — a finite seam's ladder buys REACH and a
	# renewing one's buys RENEWAL, and a composer that stated one for both would be describing a
	# mechanism the branch does not have.
	h._assert_hud("…and a coppiced wood's payoff is its RENEWAL rather than its reach",
		HudDepositVocab.deposit_payoff_clause(
				_catalog_entry(HudDepositVocab.RUNG_KEY_COPPICE),
				_catalog_entry(HudDepositVocab.RUNG_KEY_DEADFALL))
			== HudDepositVocab.DEPOSIT_PAYOFF_REGROWTH_TWICE)
	# ⛔ **THE PAYOFF ROW'S KEY IS A BLANK, NOT ABSENT, AND THAT IS STRUCTURAL.** A colon-free line is
	# rendered FULL WIDTH and CLOSES the open `[table=2]` to do it — so a keyless payoff in the middle
	# of the block would split the card's one table in two and every key below it would stop sharing a
	# column with `Foraging` / `Grazing`. Asserted on the RENDERED markup, which is the only place the
	# split is visible at all.
	h._assert_hud("…and it is a TABLE ROW rather than a full-width line that splits the card",
		DetailFormat.detail_bbcode(payoff_lines).count(TABLE_OPEN_TAG) == 1)
	await h._save("workings_payoff_rows")

	# ⛔ **STATE workings-forestry-sheet — `Assign foresters ▸`, THE FORAGE SHEET'S SPINE WITH THE
	# ELEMENTS A DEPOSIT HAS NO CONCEPT FOR ABSENT.** No floor presets, no chart, no species chips: a
	# deposit has no escapement floor, and the chart IS the floor dial.
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	var foresters := _assign_button(h._hud.forestry_assign_controls,
		HudDepositVocab.BRANCH_FORESTRY)
	h._assert_hud("a hex carrying timber offers `%s`"
			% (HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT
				% HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower()),
		foresters != null)
	if foresters != null:
		# **DIAL AFTER THE FIRST OPEN, THEN RE-OPEN** — the compose walk's own contract: the first open
		# re-seeds the crew off the band's standing row, so a count set before it is thrown away.
		foresters.pressed.emit()
		await h._settle()
		h._hud._compose.set_deposit_count(SHEET_CREW)
		h._hud._drawercompose.open_deposit_compose(_wood_working(WOOD_OVER_CUT))
		await h._settle()
		var sheet: Node = h._hud._drawercompose._compose_sheet
		# The crew the sheet SETTLED on, read back rather than assumed: the stepper's cap is the
		# working's own max-useful against the band's hands, and asserting a deal at a count the sheet
		# refused would be asserting a number nothing on screen shows.
		var crew: int = h._hud._compose.deposit_count()
		h._assert_hud("…and its eyebrow names the BRANCH's crew, not a rung's (%s)"
				% sheet._header.text,
			sheet._header.text.contains((HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT
				% HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower()).to_upper()))
		h._assert_hud("…and the crew row is that same noun (%s)" % Readout.crew_row_label(sheet),
			Readout.crew_row_label(sheet)
				== HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_upper())
		# ⛔ **THE DIAL IS OFFERED HERE BECAUSE THE GROUND GROWS BACK** (issue #650) — the three intent
		# presets over the draggable chart, through the SAME builders the forage sheet uses. The
		# negative that makes this mean something is the digger sheet below, on rock, which draws
		# neither.
		h._assert_hud("…and a RENEWING seam is offered the floor presets AND the chart",
			_first_meta(sheet, HudWidgets.POLICY_RUNG_META) != null
				and _first_meta(sheet, HudWidgets.FLOOR_CHART_META) != null)
		# **THE POINTER LINE — the sheet emits no improvement verb, so it NAMES the board that does.**
		h._assert_hud("…and the pointer line names the next rung and links the Work tab (%s)"
				% _offer_text(sheet),
			_offer_text(sheet).contains(HudDepositVocab.offer_label(
					_catalog_entry(HudDepositVocab.RUNG_KEY_COPPICE),
					HudDepositVocab.BRANCH_FORESTRY))
				and _offer_text(sheet).contains(HudComposeVocab.WORK_TAB_LINK_TEXT))
		# **THE DEAL — its own block, quoting the NEXT rung's rate at the crew being composed.**
		h._assert_hud("…and the deal row states what a coppice would pay at this crew (%s | %s)"
				% [Readout.improvement_deal_text(sheet), Readout.improvement_deal_value(sheet)],
			Readout.improvement_deal_text(sheet).contains(
					HudDepositVocab.deal_label(_catalog_entry(HudDepositVocab.RUNG_KEY_COPPICE)).to_upper())
				and crew > 0
				and Readout.improvement_deal_value(sheet).contains(
					DetailFormat.format_trimmed(CATALOG_COPPICE_YIELD * float(crew),
						HudDepositVocab.CARD_STOCK_DECIMALS)))
		# **THE RENEWING ARM OF §7's FORK, in the readout's own note and verdict.**
		# **THE NOTE RIDES THE ROW, IN THE READOUT'S SMALL-PRINT UPPERCASE** — `_readout_unit_label`
		# upper-cases every annotation it draws, so the needle is the vocabulary's own word in the
		# case the row states it, never a second spelling.
		#
		# ⛔ **AND IT IS THE COMPOSE SHEET'S WORD, NOT THE TILE CARD'S** (issue #650). A sheet states
		# the consequence in the source's own noun — *overdraws the patch* / *the herd* / *the seam*,
		# `HudComposeVocab.LOCAL_OVERDRAW_NOTES` — while the card's one-line clause states the bare
		# adjective (`HudDepositVocab.over_cut_word`). Two registers of one idea, and the deposit
		# readout takes the register its two neighbours take because it is now the same widget.
		h._assert_hud("…and a renewing seam cut over its renewal wears the SHEET's overdraw note (%s)"
				% Readout.yields_text(sheet),
			Readout.yields_text(sheet).contains(
				HudComposeVocab.LOCAL_EXTRACT_OVERDRAW_NOTE.to_upper()))
		# ⛔ **AND ITS VERDICT IS THE SHARED REACHING ONE, WHICH IS WHAT THE FLOOR BOUGHT** (issue
		# #650). It was `Cutting 7.2 a turn against 4.5 that grows back` — true, and an observation
		# rather than a decision. With a dial there is a question to answer instead: does THIS crew get
		# the stand down to where you told it to stop, and if not how many hands would. The over-cut
		# fact is not lost — it is the ⚠ on the row above, asserted a line up.
		#
		# **`deposit_verdict`'s renewing arm still ships and is still reached** — a renewing working
		# the wire sent no curve for has no walk to read, and that arm is what it falls back to.
		var over_cut_sentence := HudDepositVocab.DEPOSIT_VERDICT_OVER_CUT_FORMAT % [
			DetailFormat.format_trimmed(WOOD_OVER_CUT, HudDepositVocab.CARD_STOCK_DECIMALS),
			DetailFormat.format_trimmed(WOOD_SUSTAINABLE, HudDepositVocab.CARD_STOCK_DECIMALS)]
		h._assert_hud("…and its verdict is the SHARED harvest one, off the projection walk (%s)"
				% Readout.verdict_text(sheet),
			Readout.verdict_text(sheet) != ""
				and not Readout.verdict_text(sheet).contains(over_cut_sentence))
		# **AND THE ARM IT REPLACED IS STILL REACHED** — a renewing working the wire published no
		# curve for has no walk, and `deposit_verdict` is what the readout falls back to there. The
		# claim is the PRODUCER rather than a frame: no fixture can show both verdicts at once.
		h._assert_hud("…while a curveless working still composes the over-cut sentence (%s)"
				% String(HudDepositVocab.deposit_verdict(_curveless_wood(), _ladder()).get("text", "")),
			String(HudDepositVocab.deposit_verdict(_curveless_wood(), _ladder()).get("text", ""))
				== over_cut_sentence)
		h._assert_hud("…and its commit button is the branch's own verb (%s)" % _commit_face(sheet),
			_commit_face(sheet) == HudDepositVocab.commit_verb(HudDepositVocab.BRANCH_FORESTRY))
		await h._save("workings_forestry_sheet")
		h._hud._drawercompose.close_compose_sheet()
		await h._settle()

	# ⛔ **STATE workings-extraction-sheet — `Assign diggers ▸`, AND THE VERDICT THE BRANCH TURNS ON.**
	# `Gathering reaches 330 of 2,200. A quarry would reach 1,870.` is the sentence the whole finite
	# ladder exists to make, and it is composed from the CATALOG's reach against the ground's capacity
	# — never from `reachable / capacity`, which falls as the rock is worked.
	var diggers := _assign_button(h._hud.extraction_assign_controls,
		HudDepositVocab.BRANCH_EXTRACTION)
	h._assert_hud("…and the same hex offers `%s` for the rock beside it"
			% (HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT
				% HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_EXTRACTION).to_lower()),
		diggers != null)
	if diggers != null:
		diggers.pressed.emit()
		await h._settle()
		h._hud._compose.set_deposit_count(SHEET_CREW)
		h._hud._drawercompose.open_deposit_compose(_stone_working(STONE_TAKE))
		await h._settle()
		var sheet: Node = h._hud._drawercompose._compose_sheet
		h._assert_hud("the digger sheet's crew row is the OTHER branch's noun (%s)"
				% Readout.crew_row_label(sheet),
			Readout.crew_row_label(sheet)
				== HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_EXTRACTION).to_upper())
		# ⛔ **AND ROCK IS OFFERED NO DIAL AT ALL — the fork is the CLIENT'S, and it is
		# `regrowth_rate > 0`.** The sim publishes a floor on every `extract` row; what a finite seam
		# must never be offered is the CHOICE, because *leave half the seam* on ground that never grows
		# back means never getting half the seam. A disabled or empty chart would be furniture
		# explaining an absence, so the sheet keeps exactly the shape it had.
		h._assert_hud("…and a FINITE seam is offered neither a floor preset nor a chart",
			_first_meta(sheet, HudWidgets.POLICY_RUNG_META) == null
				and _first_meta(sheet, HudWidgets.FLOOR_CHART_META) == null)
		# ⛔ **NOR ANY CREW-TARGET PILL** — both pills are answers about a FLOOR, so the empty chart
		# model is what drops them rather than a branch in the deposit builder.
		h._assert_hud("…and neither crew-target pill, both being answers about a floor",
			Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_CLEAR)
					== Readout.CREW_TARGET_ABSENT
				and Readout.crew_target_count(sheet, HudWidgets.CREW_TARGET_HOLD)
					== Readout.CREW_TARGET_ABSENT)
		h._assert_hud("…and its verdict states what this rung reaches AND what the next would (%s)"
				% Readout.verdict_text(sheet),
			Readout.verdict_text(sheet).contains((HudDepositVocab.DEPOSIT_VERDICT_REACH_FORMAT % [
					CATALOG_GATHERING_NAME,
					DetailFormat.format_trimmed(STONE_REACHABLE,
						HudDepositVocab.CARD_STOCK_DECIMALS),
					DetailFormat.format_trimmed(STONE_CAPACITY,
						HudDepositVocab.CARD_STOCK_DECIMALS)])
				+ HudDepositVocab.DEPOSIT_VERDICT_REACH_NEXT_FORMAT % [
					CATALOG_QUARRY_NAME.to_lower(),
					DetailFormat.format_trimmed(QUARRY_REACHABLE,
						HudDepositVocab.CARD_STOCK_DECIMALS)]))
		# ⛔ **THE ASIDE HONOURS BOTH SENTINELS.** `-2` is *nobody is cutting it* and never `0 turns`.
		h._assert_hud("…and the aside states the runway at this rate (%s)"
				% Readout.readout_aside_text(sheet),
			Readout.readout_aside_text(sheet).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_FORMAT % STONE_RUNWAY))
		h._assert_hud("…while an idle seam says nobody is cutting it, never `%s` (%s)"
				% [HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_FORMAT % 0,
					HudDepositVocab.runway_aside(_idle_stone_working())],
			HudDepositVocab.runway_aside(_idle_stone_working())
					== HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_IDLE
				and HudDepositVocab.runway_aside(_idle_stone_working())
					!= HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_FORMAT % 0)
		h._assert_hud("…and its commit button is `%s`, the other branch's verb (%s)"
				% [HudDepositVocab.commit_verb(HudDepositVocab.BRANCH_EXTRACTION),
					_commit_face(sheet)],
			_commit_face(sheet) == HudDepositVocab.commit_verb(HudDepositVocab.BRANCH_EXTRACTION))
		await h._save("workings_extraction_sheet")
		h._hud._drawercompose.close_compose_sheet()
		await h._settle()

	# ⛔ **THE LADDER'S ROWS, ASSERTED WITHOUT A FRAME.** The track is opened from the WORK BOARD's
	# workings roster, which this HUD-only harness does not stand up — `band_panel_preview` renders it.
	# What is asserted here is the PRODUCER: the four states a deposit branch can reach, each of which
	# a rendered frame could only show one of.
	var scatter_rows := RungLadder.deposit_track(_scatter_working(), _ladder(),
		_knowledge(KNOWLEDGE_UNLEARNED), _knowledge_labels(), LADDER_CUTTERS)
	# ⛔ **THE SITE GATE IS NEW TO THIS BRANCH** — the route branch has no placement rule at all — and
	# it OUTRANKS the craft, because no amount of learning ever makes a 70-unit scatter big enough.
	h._assert_hud("a quarry on a scatter too small for it is refused for its SIZE, not its craft (%s)"
			% _row_face(scatter_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(scatter_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_SHORT_TOO_SMALL))
	h._assert_hud("…and its hover names the threshold AND this ground's own capacity (%s)"
			% _row_tooltip(scatter_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_tooltip(scatter_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_LONG_TOO_SMALL_FORMAT % [
				DetailFormat.format_trimmed(CATALOG_QUARRY_MIN_CAPACITY,
					HudDepositVocab.CARD_STOCK_DECIMALS),
				DetailFormat.format_trimmed(SCATTER_CAPACITY,
					HudDepositVocab.CARD_STOCK_DECIMALS)]))
	var stone_rows := RungLadder.deposit_track(_stone_working(STONE_TAKE), _ladder(),
		_knowledge(KNOWLEDGE_UNLEARNED), _knowledge_labels(), LADDER_CUTTERS)
	h._assert_hud("…while a body big enough for one is refused on the CRAFT instead (%s)"
			% _row_face(stone_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(stone_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_SHORT_NEEDS_CRAFT_FORMAT % CATALOG_QUARRYING_LABEL))
	# **THE REMEDY NAMES THE RUNG THAT *TEACHES* THE CRAFT**, looked up through `earns_knowledge` and
	# never inferred from `requires_rung`.
	h._assert_hud("…and the remedy names the rung that TEACHES it (%s)"
			% _row_tooltip(stone_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_tooltip(stone_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_LONG_KNOWLEDGE_REMEDY_FORMAT % CATALOG_GATHERING_NAME.to_lower()))
	# ⛔ **THE FREE FLOOR RENDERS AS A FACT, NOT A PRICE OF ZERO** — the rung this working stands on
	# takes `where you are`, never a `0 work` button.
	h._assert_hud("…and the free floor it stands on is a FACT, never a price of zero (%s)"
			% _row_state(stone_rows, HudDepositVocab.RUNG_KEY_GATHERING),
		_row_state(stone_rows, HudDepositVocab.RUNG_KEY_GATHERING) == RungLadder.STATE_STANDING
			and not _row_selectable(stone_rows, HudDepositVocab.RUNG_KEY_GATHERING))
	var open_rows := RungLadder.deposit_track(_stone_working(STONE_TAKE), _ladder(),
		_knowledge(KNOWLEDGE_LEARNED), _knowledge_labels(), LADDER_CUTTERS)
	# ⛔ **A PRICED ROW QUOTES ITS PILE AND ITS STANDING BILL, AND NO TURNS.** The estimate would be
	# divided by a builders pool that may be on another job and would ignore the queue the press joins.
	h._assert_hud("a rung within reach leads with its price and its upkeep, and quotes no turns (%s)"
			% _row_face(open_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(open_rows, HudDepositVocab.RUNG_KEY_QUARRY)
				== HudDepositVocab.deposit_ladder_price_face(
					_catalog_entry(HudDepositVocab.RUNG_KEY_QUARRY))
			and _row_selectable(open_rows, HudDepositVocab.RUNG_KEY_QUARRY))
	h._assert_hud("…and the material it eats rides beneath it as the SHARED price aside (%s)"
			% str(_row_asides(open_rows, HudDepositVocab.RUNG_KEY_QUARRY)),
		"\n".join(_row_asides(open_rows, HudDepositVocab.RUNG_KEY_QUARRY)).contains(
			CATALOG_QUARRY_MATERIAL_ID))
	# ⛔ **A ROW BEING BUILT QUOTES ITS METER AND THE SIM'S OWN CHAINED ESTIMATE**, never a price: a
	# rung already ordered is not a purchase being weighed, and `0%` is the receipt that the press
	# landed.
	var building_rows := RungLadder.deposit_track(_queued_stone_working(), _ladder(),
		_knowledge(KNOWLEDGE_LEARNED), _knowledge_labels(), LADDER_CUTTERS)
	h._assert_hud("…and a rung already ordered quotes its meter and the sim's date instead (%s)"
			% _row_face(building_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(building_rows, HudDepositVocab.RUNG_KEY_QUARRY)
			== HudDepositVocab.build_value(_queued_stone_working(), 0))
	# ⛔ **THE CREW GATE — a working nobody holds refuses its whole ladder, and the card says so.** The
	# sim reaches a deposit verb only through a band's staffed `extract` row
	# (`queue_build_on_working_bands` filters `workers > 0`), and the roster lists a 0-crew working, so
	# the ladder opens on one in a click. The A/B is the claim: the SAME working, the SAME learned
	# craft, only the crew moving — a gate that refused unconditionally would satisfy the locked half
	# on its own.
	var idle_rows := RungLadder.deposit_track(_stone_working(STONE_TAKE), _ladder(),
		_knowledge(KNOWLEDGE_LEARNED), _knowledge_labels(), LADDER_NO_CUTTERS)
	h._assert_hud("a working with nobody on it refuses the rung above it for want of a CREW (%s)"
			% _row_face(idle_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(idle_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_SHORT_NO_CREW)
			and not _row_selectable(idle_rows, HudDepositVocab.RUNG_KEY_QUARRY))
	# **THE HOVER NAMES THE BRANCH'S OWN CREW, which is the whole of the remedy** — *diggers* on a rock
	# and *foresters* on a wood, never one word for both.
	h._assert_hud("…and its hover names the remedy in this branch's own crew noun (%s)"
			% _row_tooltip(idle_rows, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_tooltip(idle_rows, HudDepositVocab.RUNG_KEY_QUARRY).contains(
			HudDepositVocab.GATE_LONG_NO_CREW_FORMAT % HudDepositVocab.crew_noun(
				HudDepositVocab.BRANCH_EXTRACTION).to_lower()))
	# ⛔ **AND THE SITE GATE STILL OUTRANKS IT.** A 70-unit scatter will never take a quarry however
	# many diggers stand on it, so *put diggers on it* is wrong advice there — the row states its size
	# and keeps the crew refusal for the hover.
	var idle_scatter := RungLadder.deposit_track(_scatter_working(), _ladder(),
		_knowledge(KNOWLEDGE_LEARNED), _knowledge_labels(), LADDER_NO_CUTTERS)
	h._assert_hud("…while ground too small for a quarry still leads with its SIZE, crew or no crew (%s)"
			% _row_face(idle_scatter, HudDepositVocab.RUNG_KEY_QUARRY),
		_row_face(idle_scatter, HudDepositVocab.RUNG_KEY_QUARRY).contains(
				HudDepositVocab.GATE_SHORT_TOO_SMALL)
			and not _row_face(idle_scatter, HudDepositVocab.RUNG_KEY_QUARRY).contains(
				HudDepositVocab.GATE_SHORT_NO_CREW))
	# **AND THE FREE FLOOR IS UNTOUCHED BY IT** — nobody declares that rung, so there is no order a
	# crew could be wanted for, and gate 1 is still stated ALONE.
	h._assert_hud("…and the rung nobody declares still states its own word alone (%s)"
			% _row_face(idle_rows, HudDepositVocab.RUNG_KEY_GATHERING),
		_row_state(idle_rows, HudDepositVocab.RUNG_KEY_GATHERING) == RungLadder.STATE_STANDING)

	# ⛔ **AND UNTOUCHED GROUND IS STILL A ROW, WITH NO WARNING ON IT.** The section stands a row on
	# every discovered deposit-bearing tile, so most rows on a revealed map describe ground nobody has
	# opened — and rendering that as a working being kept is the reading `is_unopened` exists to stop.
	h._show_tile(_workings_tile([_unopened_wood(), _unopened_stone()]))
	await h._settle()
	var unopened_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	h._assert_hud("untouched ground states its full seam as ONE figure, under the floor's own name (%s)"
			% Readout.detail_row_value(unopened_lines, "Wood"),
		Readout.detail_row_value(unopened_lines, "Wood") == "%s%s%s" % [
			HudDepositVocab.DEPOSIT_STOCK_FULL_FORMAT % DetailFormat.format_trimmed(
				WOOD_CAPACITY, HudDepositVocab.CARD_STOCK_DECIMALS),
			HudDepositVocab.DEPOSIT_CLAUSE_SEPARATOR, CATALOG_DEADFALL_NAME])
	h._assert_hud("…and carries no hazard word of either kind (%s | %s)"
			% [Readout.detail_row_value(unopened_lines, "Wood"),
				Readout.detail_row_value(unopened_lines, "Stone")],
		not "\n".join(unopened_lines).contains(HudSelectionVocab.RUNG_HAZARD_GLYPH))
	# ⛔ **AND THE PREDICATE IS NOT `actual_take == 0`.** The idle quarry takes nothing either, and it
	# is a working somebody opened and walked away from — a drawn-down seam that will run out.
	h._assert_hud("…yet a WORKING that merely took nothing still reads as one (%s)"
			% HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder()),
		HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder()).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_IDLE)
			and not HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder()).contains(
				HudDepositVocab.DEPOSIT_UNOPENED_WORD))
	# ⛔ **AND THE THIRD OF THE THREE SILENCES — a working THIS BAND has taken its hands off** (issue
	# #650). `unopened` is ground with no working, `not being worked` is the SOURCE's reading (nobody
	# at all is cutting it, so there is no rate to carry a runway forward on), and the crew clause is
	# the BAND's. **They are asserted on ONE row asked two ways**, which is the only shape that says
	# the crew is an argument rather than a field: the same idle quarry reads the source's word with
	# no crew stated and the band's word with a crew of nobody.
	var walked_away := HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder(),
		LADDER_NO_CUTTERS)
	h._assert_hud("…and the same working asked about THIS BAND's hands says so in its own word (%s)"
			% walked_away,
		walked_away.contains(HudDepositVocab.DEPOSIT_IDLE_WORD))
	# ⛔ **AND THE BAND'S WORD REPLACES THE SOURCE'S RATHER THAN JOINING IT.** Both describe one
	# silence, and a row carrying both spends two of its clauses saying one thing — so the reading the
	# player can act on from the roster is the one that stays.
	h._assert_hud("…in place of the source's own, never beside it (%s)" % walked_away,
		not walked_away.contains(HudDepositVocab.DEPOSIT_RUNWAY_IDLE))
	# **AND `CUTTERS_UNSTATED` IS NOT A CREW OF ZERO**, which is the tile card and every other reader
	# with no band in hand: not knowing whose hands are on a working must not announce that nobody's
	# are. The first claim above is asked at the default and would pass vacuously without this.
	h._assert_hud("…while a surface that cannot state a crew says nothing about one (%s)"
			% HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder()),
		not HudDepositVocab.deposit_row_value(_idle_stone_working(), _ladder()).contains(
			HudDepositVocab.DEPOSIT_IDLE_WORD))
	await h._save("workings_unopened")

	# ⛔ **STATE workings-road-last — THE ROAD CLOSES THE CARD.** Ray, reading a live Alluvial Plain
	# card: *"Road should go last in the list"*. The deposits and the two food webs are what the ground
	# IS; the road is what has been built across it, so it closes the card rather than splitting the
	# rivers from the seams. This is the first frame that stands all four families on ONE hex — the
	# seams, the human web with its basket, the animal web, and the road — which is the only shape the
	# ORDER is a claim about.
	h._show_tile(_full_land_tile(TileFx.VIS_ACTIVE))
	await h._settle()
	var order_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	h._assert_hud("the road closes the live card — its row is the LAST line on it (%s)"
			% str(order_lines.slice(maxi(0, order_lines.size() - 2))),
		Readout.detail_row_index(order_lines, HudRouteVocab.ROAD_ROW) == order_lines.size() - 1)
	# ⛔ **AND THE DEPOSITS DID NOT MOVE WITH IT.** Ray asked about the road alone, so the seams stay
	# between the rivers and the two webs — a claim that has to be made HERE, because the road's move
	# is a reordering of the one producer both blocks are emitted from.
	h._assert_hud("…while the seams still sit ABOVE the two food webs, where they already were",
		Readout.detail_row_index(order_lines, "Wood")
				< Readout.detail_row_index(order_lines, HudFloraVocab.FORAGING_KEY)
			and Readout.detail_row_index(order_lines, "Stone")
				< Readout.detail_row_index(order_lines, HudFloraVocab.FORAGING_KEY)
			and Readout.detail_row_index(order_lines, HudFloraVocab.GRAZING_KEY)
				< Readout.detail_row_index(order_lines, HudRouteVocab.ROAD_ROW))
	await h._save("workings_road_last")

	# ⛔⛔ **STATE workings-road-remembered — THE REGRESSION THIS PAIR EXISTS TO PREVENT.** The road
	# block is COMPOSED above the discovered early-return, because the sim publishes a road to any
	# faction that has seen the TILE — a road does not wander off, so remembering one is remembering
	# something true. Moving the block below that return to put it last would have dropped the road
	# from every remembered hex the sim went to the trouble of sending it for, which is exactly the
	# class of loss `_assert_fog_stock_parity` was built for one arc over. It is HELD in a local and
	# appended at the end of BOTH branches instead, and these two claims are what says so.
	h._show_tile(_full_land_tile(TileFx.VIS_DISCOVERED))
	await h._settle()
	var remembered_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	h._assert_hud("a REMEMBERED hex still carries its road (%s)"
			% Readout.detail_row_value(remembered_lines, HudRouteVocab.ROAD_ROW),
		Readout.detail_row_index(remembered_lines, HudRouteVocab.ROAD_ROW) >= 0)
	# **AND LAST THERE TOO** — the remembered branch returns early, so its append is a SECOND site and
	# would be the one forgotten. The webs above it state a capacity with no stock, which is the other
	# half of that branch and is asserted in `land_readouts.gd`; what is claimed here is the position.
	h._assert_hud("…and it is the LAST line there too, as it is on the live card (%s)"
			% str(remembered_lines.slice(maxi(0, remembered_lines.size() - 2))),
		Readout.detail_row_index(remembered_lines, HudRouteVocab.ROAD_ROW)
				== remembered_lines.size() - 1
			and Readout.detail_row_index(remembered_lines, HudFloraVocab.GRAZING_KEY)
				< Readout.detail_row_index(remembered_lines, HudRouteVocab.ROAD_ROW))
	await h._save("workings_road_remembered")

	# ⛔ **STATE workings-worked-buttons — THE STANDING SUMMARY IS THE BUTTON'S SECOND LINE.** Ray, on
	# a Rolling Hills card whose `♻ 2 foresters · +0.60 wood` sat in a row of its own above the button:
	# *"That looks strange there … I think that would look more at home inside the button. We should
	# make it the second line on the button."*
	#
	# ⛔ **IT IS THE PAIR THAT IS THE CLAIM, ON ONE HEX.** This band works the WOOD and not the rock, so
	# `Assign foresters ▸` carries a second line and `Assign diggers ▸` carries none — a control that
	# grew a blank second line on every source would satisfy the presence half on its own, and the
	# blank gap is exactly what Ray's *"a source nobody works keeps its single line"* forbids.
	h._hud.update_band_alerts([_working_band_fixture()])
	h._show_tile(_full_land_tile(TileFx.VIS_ACTIVE))
	await h._settle()
	await h._save("workings_worked_buttons")
	var worked := _assign_button(h._hud.forestry_assign_controls, HudDepositVocab.BRANCH_FORESTRY)
	var unworked := _assign_button(h._hud.extraction_assign_controls,
		HudDepositVocab.BRANCH_EXTRACTION)
	h._assert_hud("both branches still offer their `Assign … ▸` control",
		worked != null and unworked != null)
	if worked != null and unworked != null:
		var summary := Q.stacked_action_summary(worked)
		# **THE LABEL IS STILL THE FIRST LINE**, which is what says the summary joined the control
		# rather than replacing its face.
		h._assert_hud("…the worked branch's button still reads `%s` on its first line (%s)"
				% [HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT
					% HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower(),
					Q.action_button_face(worked)],
			Q.action_button_face(worked) == HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT
				% HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower())
		h._assert_hud("…and its SECOND line is the standing summary, inside the control",
			summary != null and summary.get_child_count() > 0
				and (summary.get_child(0) as Label).text.contains(
					HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower()))
		# ⛔ **AND THE PRESS STILL LANDS.** Every control in that face is `MOUSE_FILTER_IGNORE`, or the
		# summary's own note labels — which have been through `set_label_tooltip`, i.e. `STOP` — would
		# be dead patches over the button. A picture cannot show a swallowed click.
		var pressable := true
		for control in [summary]:
			if control != null and control.mouse_filter != Control.MOUSE_FILTER_IGNORE:
				pressable = false
		h._assert_hud("…with its whole face inert, so the button stays pressable", pressable)
		# **THE NEGATIVE, on the branch beside it**: no standing row, so no second line at all.
		h._assert_hud("…while the branch this band does NOT work keeps its single line",
			Q.stacked_action_summary(unworked) == null)
		# ⛔ **AND THE TAKE IS STATED IN THE MATERIAL, NEVER IN THE FOOD UNIT** (issue #650). This is
		# the RESOLVED half of the pair the next state opens: a working that has paid out reads
		# `+0.60 wood`, off the row's `material_yield`, and the food zero beside it is suppressed by
		# `SourceForecast.yield_rows`' own rule — a source paying a material pays SOMETHING.
		h._assert_hud("…and its take is stated in WOOD, with no food unit beside it (%s)"
				% _summary_text(worked),
			_summary_text(worked).contains(WOOD_MATERIAL_ID)
				and not _summary_text(worked).contains(SourceForecast.YIELD_PER_TURN_SUFFIX))
		# ⛔ **AND THE HOVER SAYS THE SAME THING THE FACE DOES.** The face and the tooltip are two
		# spellings of ONE row, so a button reading `+0.60 wood` over a hover reading
		# `+0.00 a turn on average · Sustainable +0.00 /turn` is worse than either being wrong alone —
		# the player is handed two accounts for one working and no way to tell which is the source's.
		# **Asserted by EQUALITY against the composed material clause**, which is the only form that
		# proves BOTH halves at once: the material clause is present, and no food clause survives beside
		# it. A `contains` would pass on the very string this exists to forbid.
		h._assert_hud("…and its HOVER is that same take and nothing else (%s)" % worked.tooltip_text,
			worked.tooltip_text == SourceForecast.POLICY_CAP_MATERIAL_FORMAT % [
				SourceForecast.format_signed(WORKED_BUTTON_TAKE), WOOD_MATERIAL_ID])

	# ⛔⛔ **STATE workings-just-assigned — THE CREW IS ON, NOTHING HAS BEEN TAKEN YET, AND THE TWO
	# WEBS ARE READ SIDE BY SIDE.** Issue #650, reported from play: two foresters on a floodplain wood
	# whose button read `♻ 2 foresters · +0.00 /turn`, while a forage crew assigned the same way read a
	# real number. **Every other frame in this chapter was authored with a RESOLVED take**, which is
	# exactly why the harness rendered a figure where the live game rendered `+0.00 /turn` — the broken
	# state was the one state nothing staged.
	#
	# ⛔ **THE CLAIM IS THE UNIT, NOT THE FIGURE, AND THAT IS DELIBERATE.** The NUMBER on the deposit
	# line comes from the sim's assign-time forecast seed (`core_sim/src/bin/server.rs` →
	# `seed_source_yield`), which seeds `Forage` and `Hunt` and returns early on `Extract` — so the
	# forage line here carries a seeded rate and the deposit line carries the zero the wire actually
	# sends. **The FIGURE is the sim's to state and the UNIT is the client's**, so this frame claims only
	# the client's half: a working states its MATERIAL and never the food unit, which is true at
	# `+0.00 wood` and at `+0.60 wood` alike. A frame pinning the zero would be asserting the sim's half
	# through the client, and would break on a seeded row that is more correct than the one it froze.
	h._hud.update_band_alerts([_just_assigned_band_fixture()])
	h._show_tile(_full_land_tile(TileFx.VIS_ACTIVE))
	await h._settle()
	await h._save("workings_just_assigned")
	var fresh_forage := _forage_summary_text()
	var fresh_wood := _summary_text(
		_assign_button(h._hud.forestry_assign_controls, HudDepositVocab.BRANCH_FORESTRY))
	# **THE FORAGE HALF — A REAL RATE IN THE FOOD UNIT**, which is the counterexample that made this a
	# bug rather than a timing artefact: a crew assigned this same turn, on the same card, states a
	# number.
	h._assert_hud("a crew put on the PATCH this turn states a seeded food rate (%s)" % fresh_forage,
		fresh_forage.contains(SourceForecast.YIELD_PER_TURN_SUFFIX)
			and not fresh_forage.contains(SourceForecast.format_signed(0.0)))
	# **THE DEPOSIT HALF — THE SAME LINE IN THE WORKING'S OWN ACCOUNT, AND A REAL FIGURE IN IT.** The
	# unit was the whole claim while the sim declined to seed an `Extract` row; it seeds one now, so
	# the frame states both halves: the working's own material, and never the food unit beside it.
	h._assert_hud("…and a crew put on the WORKING states its material at a seeded rate (%s)"
			% fresh_wood,
		fresh_wood.contains(WOOD_MATERIAL_ID)
			and fresh_wood.contains(SourceForecast.format_signed(FRESH_WOOD_SEED))
			and not fresh_wood.contains(SourceForecast.YIELD_PER_TURN_SUFFIX))
	# **AND BOTH ARE SUMMARIES AT ALL**, which is what makes the pair a comparison rather than two
	# separate readings: a missing second line would satisfy the two negatives above on its own.
	# ⛔ **AND THE HOVER STATES THE SEEDED TAKE AND NOTHING ELSE.** Every clause it used to carry — the
	# average, this turn's figure, the sustainable ceiling — is the FOOD account, and
	# `systems/labor.rs`' `Extract` arm leaves that at `SourceYield::ZERO` by construction, so the
	# sentence a player read there was three zeros in an account the ground does not pay. **Asserted by
	# EQUALITY against the composed material clause**, which is the only form that proves both halves
	# at once: the material clause is present, and no food clause survives beside it.
	var fresh_wood_hover := _assign_button(h._hud.forestry_assign_controls,
		HudDepositVocab.BRANCH_FORESTRY).tooltip_text
	h._assert_hud("…and its hover is that seeded take and nothing else (%s)" % fresh_wood_hover,
		fresh_wood_hover == SourceForecast.POLICY_CAP_MATERIAL_FORMAT % [
			SourceForecast.format_signed(FRESH_WOOD_SEED), WOOD_MATERIAL_ID])
	h._assert_hud("…with both controls carrying a second line on the same card (%s | %s)"
			% [fresh_forage, fresh_wood],
		fresh_forage.contains(HudComposeVocab.HARVEST_CREW_LABEL.to_lower())
			and fresh_wood.contains(
				HudDepositVocab.crew_noun(HudDepositVocab.BRANCH_FORESTRY).to_lower()))

	# ⛔⛔ **STATE workings-floor-* — THE ESCAPEMENT FLOOR ON THE FORESTRY SHEET** (issue #650). The
	# three intent presets over the draggable chart, above the crew row, through the SAME
	# `HudWidgets.build_floor_picker` / `build_floor_chart` / `SourceForecast.floor_chart_model` the
	# forage sheet uses. What the dial buys a WOOD is what it buys a patch: the take now, the take once
	# the stand settles, and a faster lesson for leaving more standing.
	#
	# ⛔ **THE WALK RUNS ON A NEARLY-FULL STAND, AND THAT IS NOT COSMETIC.** The chapter's working
	# fixture stands at 412 of 600 — BELOW the top preset's own floor of 480 — so at `Learn from it`
	# it honestly takes nothing until the stand grows past the line, and the frame would show an empty
	# readout for a true reason that has nothing to do with the dial. A stand above every preset's
	# floor is what makes the three frames a comparison of the DIAL rather than of the stock.
	h._hud.update_band_alerts([_working_band_fixture()])
	h._show_tile(_workings_tile([_standing_wood(), _stone_working(STONE_TAKE)]))
	await h._settle()
	h._hud._drawercompose.open_deposit_compose(_standing_wood())
	await h._settle()
	h._hud._compose.set_deposit_count(SHEET_CREW)
	for preset_variant in SourceForecast.FLOOR_PRESETS:
		var preset := String(preset_variant)
		# **DRIVEN AS THE PRESET BUTTON DRIVES IT** — the dial, then the autofill one-shot, then the
		# rebuild. Not through the button's face, which is the picker's own text and would make this a
		# claim about a label.
		h._hud._compose.set_deposit_floor(SourceForecast.floor_for_preset(preset))
		h._hud._compose.set_deposit_count(SHEET_CREW)
		h._hud._drawercompose.open_deposit_compose(_standing_wood())
		await h._settle()
		var floor_sheet: Node = h._hud._drawercompose._compose_sheet
		h._assert_hud("the wood sheet at `%s` lights that preset and draws its chart" % preset,
			SourceForecast.floor_preset_for(h._hud._compose.deposit_floor()) == preset
				and _first_meta(floor_sheet, HudWidgets.FLOOR_CHART_META) != null)
		# **THE TAKE IS STATED IN THE WORKING'S OWN MATERIAL, at every position on the dial** — a
		# working pays no food, so a `/turn` suffix here would be the account defect this arc already
		# corrected once on the button's second line.
		h._assert_hud("…and states its take in %s, never in the food unit (%s)"
				% [WOOD_MATERIAL_ID, Readout.yields_text(floor_sheet)],
			Readout.yields_text(floor_sheet).contains(WOOD_MATERIAL_ID.to_upper())
				and not Readout.yields_text(floor_sheet).contains(
					SourceForecast.YIELD_PER_TURN_SUFFIX))
		# **THE `now → after` PAIR IS WHAT THE FLOOR MAKES POSSIBLE** — the take now against the take
		# once the stand settles where the crew was told to stop. It exists only because there is a
		# floor to settle AT.
		#
		# ⛔ **THE CLAIM IS THAT THE ARROW AND THE SENTENCE CANNOT DISAGREE**, not a per-preset table
		# of which frames have one: both are gated on the SAME walk (`_live_reaches` and
		# `harvest_verdict`'s `reached_turn`), so a crew that settles short must be promised no holding
		# rate. On this stand only the top preset is close enough to reach, which is a fact about the
		# stock and would be re-authored every time the fixture moved — the pairing is not.
		h._assert_hud("…and its `now → after` arrow agrees with its own verdict (%s | %s)"
				% [Readout.yields_text(floor_sheet), Readout.verdict_text(floor_sheet)],
			Readout.yields_show_a_transition(floor_sheet)
				== Readout.verdict_text(floor_sheet).contains(
					SourceForecast.VERDICT_REACHES_FORMAT.get_slice("%", 0)))
		await h._save("workings_floor_%s" % preset)
	# ⛔ **THE TEACHING LINE IS PRICED AT THE PLAYER'S FLOOR, THROUGH THE SHARED `learn_multiplier`** —
	# and at the top preset that is `0.80 / 0.50`, the same ×1.60 a patch reads there. The rung's own
	# floor reaches the WORKABILITY predicate and not this one (`systems::labor`'s `Extract` arm passes
	# the ROW's floor to `intensification::learn_multiplier`), which is why the composition's `max` is
	# deliberately not applied to it.
	var learn_sheet: Node = h._hud._drawercompose._compose_sheet
	h._assert_hud("…and the aside teaches at the shared multiplier for that floor (%s)"
			% Readout.teaching_line(learn_sheet),
		Readout.teaching_line(learn_sheet).contains(SourceForecast.TEACHING_RATE_FORMAT % [
			CATALOG_CONSERVATIONISM, SourceForecast.learn_multiplier(
				SourceForecast.floor_for_preset(SourceForecast.FLOOR_PRESET_LEARN))]))

	# ⛔⛔ **STATE workings-floor-held — A WOOD STANDING EXACTLY AT ITS FLOOR.** The state the dial
	# exists to produce, and the one where the `now → after` pair collapses: the crew takes what the
	# stand puts back and nothing more, so the two readings coincide and `yield_rows` drops the arrow
	# itself rather than drawing one between two identical numbers.
	# **THE CARD BENEATH SHOWS THE SAME WORKING THE SHEET IS COMPOSED ON.** A sheet opened on a
	# fixture the selected hex does not carry renders coherently and reads as two pieces of ground.
	h._show_tile(_workings_tile([_wood_at_its_floor(), _stone_working(STONE_TAKE)]))
	await h._settle()
	h._hud._compose.set_deposit_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_deposit_count(SHEET_CREW)
	h._hud._drawercompose.open_deposit_compose(_wood_at_its_floor())
	await h._settle()
	var held_sheet: Node = h._hud._drawercompose._compose_sheet
	h._assert_hud("a wood standing at its floor reads the SHARED at-the-floor verdict (%s)"
			% Readout.verdict_text(held_sheet),
		Readout.verdict_text(held_sheet).contains(SourceForecast.VERDICT_HOLDS_AT_FLOOR))
	# **THE TAKE IS THE REGROWTH AT THAT FLOOR, capped by what the crew can lift** — the curve's own
	# value at `floor × capacity`, which is the peak this wood renews at and the figure the tile card
	# has been calling `sustainable_take` all along. Nothing more is reachable: the stock IS the floor,
	# so the only wood on offer next turn is the wood that grows between now and then.
	#
	# ⛔ **THE CREW IS READ BACK RATHER THAN ASSUMED**, the forestry sheet's own rule one state up: a
	# reopened sheet seeds from the band's standing row, so the count is the band's and asserting a
	# take at a count the sheet refused would be asserting a number nothing on screen shows.
	var held_crew: int = h._hud._compose.deposit_count()
	h._assert_hud("…and its take is `min(crew × rate, what the stand puts back)` at %d (%s)"
			% [held_crew, Readout.yields_text(held_sheet)],
		held_crew > 0 and Readout.yields_text(held_sheet).contains(
			SourceForecast.format_magnitude(
				minf(float(held_crew) * FELLING_PER_WORKER, WOOD_SUSTAINABLE))))
	h._assert_hud("…and draws no `now → after` arrow, the two readings being one number",
		not Readout.yields_show_a_transition(held_sheet))
	await h._save("workings_floor_held")

	# ⛔⛔ **STATE workings-floor-stripped — THE DIAL DRAGGED TO THE BOTTOM ON AN OVER-CUT WOOD.** The
	# drag is LIVE (`committed = false`), so the readings refill in place and the chart the pointer is
	# holding is never freed — a rebuild mid-drag ends the drag on the first pixel of movement.
	h._show_tile(_workings_tile([_over_cut_standing_wood(), _stone_working(STONE_TAKE)]))
	await h._settle()
	h._hud._drawercompose.open_deposit_compose(_over_cut_standing_wood())
	await h._settle()
	var strip_sheet: Node = h._hud._drawercompose._compose_sheet
	var strip_chart = Q.find_meta_node(strip_sheet, HudWidgets.FLOOR_CHART_META)
	h._assert_hud("the over-cut wood draws a chart to drag at all", strip_chart != null)
	if strip_chart != null:
		strip_chart.emit_signal("floor_changed", SourceForecast.FLOOR_MIN, false)
		await h._settle()
		h._assert_hud("…and the drag did not rebuild the sheet out from under itself",
			is_instance_valid(strip_chart))
		# ⛔ **THE OVER-CUT WORD IS THE FOOD WEBS' OWN**, in the working's own noun — one idea, one
		# word, wherever it is stated.
		h._assert_hud("…a seam cut over its renewal wears the shared overdraw mark (%s)"
				% Readout.yields_text(strip_sheet),
			Readout.yields_text(strip_sheet).contains(
				HudComposeVocab.LOCAL_EXTRACT_OVERDRAW_NOTE.to_upper()))
		# **AND FLOOR 0 TEACHES NOTHING**, which is the sim's own self-limit read back: the multiplier
		# is `floor / the food peak`, so a crew told to leave nothing standing learns at ×0.
		h._assert_hud("…and at floor 0 the aside says the lesson is not being earned (%s)"
				% Readout.teaching_line(strip_sheet),
			Readout.teaching_line(strip_sheet) == SourceForecast.TEACHING_NOTHING_STRIPPED)
		await h._save("workings_floor_stripped")
	h._hud._drawercompose.close_compose_sheet()
	await h._settle()

	# ⛔⛔ **STATE workings-fresh-runway — A CREW COMMITTED THIS TURN, ON A SEAM THAT HAS CUT NOTHING.**
	# `DepositState.actualTake` is written at turn resolution and at no other time, so the working
	# publishes `0` and `RUNWAY_NO_TAKE` for the whole frame between the press and the turn — and the
	# sheet read *Nobody is cutting it* over a headline stating the rate that same press committed to.
	# The sim declined to seed `actualTake` (a `+=` accumulator across bands doubles under a re-assign
	# and clobbers under a second band), so the three states are told apart from the ASSIGNMENT ROW.
	h._hud.update_band_alerts([_fresh_stone_band_fixture()])
	h._show_tile(_workings_tile([_idle_stone_working()]))
	await h._settle()
	h._hud._compose.set_deposit_count(FRESH_STONE_CUTTERS)
	h._hud._drawercompose.open_deposit_compose(_idle_stone_working())
	await h._settle()
	var fresh_sheet: Node = h._hud._drawercompose._compose_sheet
	h._assert_hud("a freshly-crewed seam states the FORECAST runway, not `%s` (%s)"
			% [HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_IDLE, Readout.readout_aside_text(fresh_sheet)],
		Readout.readout_aside_text(fresh_sheet).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_FORMAT % FRESH_STONE_RUNWAY)
			and not Readout.readout_aside_text(fresh_sheet).contains(
				HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_IDLE))
	await h._save("workings_fresh_runway")
	# **AND THE THIRD STATE IS STILL ITS OWN SENTENCE** — the same seam with nobody on it reads *not
	# being worked*, which is what makes the forecast above a distinction rather than a replacement.
	h._assert_hud("…while the SAME seam with no crew keeps the idle sentence (%s)"
			% HudDepositVocab.runway_aside(_idle_stone_working()),
		HudDepositVocab.runway_aside(_idle_stone_working())
			== HudDepositVocab.DEPOSIT_RUNWAY_ASIDE_IDLE)
	# ⛔ **AND THE VERDICT STOPS SAYING `Cutting 0`.** A renewing seam's over-cut sentence reads the
	# same three states: the realized take where a turn has resolved, this crew's seeded rate where one
	# has not, and the wire's own zero only where nobody is on it.
	h._assert_hud("…and a renewing seam's verdict quotes the SEEDED take, never a zero (%s)"
			% String(HudDepositVocab.deposit_verdict(_fresh_scatter_working(), _ladder(),
				_fresh_scatter_assignment()).get("text", "")),
		String(HudDepositVocab.deposit_verdict(_fresh_scatter_working(), _ladder(),
				_fresh_scatter_assignment()).get("text", "")).contains(
			DetailFormat.format_trimmed(FRESH_SCATTER_SEED, HudDepositVocab.CARD_STOCK_DECIMALS)))
	h._hud._drawercompose.close_compose_sheet()
	await h._settle()

	# ⛔⛔ **STATE workings-quarry-reach — THE CREW'S FLOOR IS DISCARDED WHERE THE GROUND NEVER RENEWS.**
	# The sim's `extraction::deposit_effective_floor` returns the rung's floor alone at `NEVER_RENEWS`;
	# `HudDepositVocab.composed_floor` mirrors it, and this is the state where the two used to disagree.
	# A finite seam is offered no dial, so the sheet composes at `DEFAULT_HARVEST_FLOOR` — and an
	# unconditional `max` let that 0.5 bind above `extraction:quarry`'s 0.15, under-reporting the take,
	# the cap and the runway on every stone sheet in the game.
	#
	# ⛔ **THE FORK IS TAKEN ON THE PUBLISHED `regrowth_rate`, WHICH IS THE RUNG-SCALED ONE, AND THAT
	# IS EXACT RATHER THAN LUCKY.** The sim forks on the GROUND's un-scaled rate; the wire carries it
	# multiplied by the rung's `regrowthMultiplier`, which `intensification`'s config validation floors
	# at `REGROWTH_UNCHANGED` (1.0). A never-zero multiplier makes `ground × multiplier > 0` true
	# exactly when `ground > 0`, so the two predicates cannot part company — see `renews()`.
	h._hud.update_band_alerts([_worked_quarry_band_fixture()])
	h._show_tile(_workings_tile([_worked_down_quarry()]))
	await h._settle()
	# **THE PRECONDITION, WITHOUT WHICH EVERY CLAIM BELOW IS VACUOUS** — the sheet's default really
	# must stand ABOVE this rung's own floor, or the `max` was a no-op here and nothing is being
	# tested. It is the whole reason the quarry rung is the one that diverged and gathering was not.
	h._assert_hud("the sheet's default floor stands ABOVE the quarry rung's own (%.2f > %.2f)"
			% [SourceForecast.DEFAULT_HARVEST_FLOOR, QUARRY_RUNG_FLOOR],
		SourceForecast.DEFAULT_HARVEST_FLOOR > QUARRY_RUNG_FLOOR)
	h._assert_hud("…and a working that NEVER RENEWS composes at the rung's floor alone, never %.2f (%.2f)"
			% [SourceForecast.DEFAULT_HARVEST_FLOOR,
				HudDepositVocab.composed_floor(_worked_down_quarry(),
					SourceForecast.DEFAULT_HARVEST_FLOOR)],
		is_equal_approx(HudDepositVocab.composed_floor(_worked_down_quarry(),
			SourceForecast.DEFAULT_HARVEST_FLOOR), QUARRY_RUNG_FLOOR))
	# ⛔ **AND THE ROOM REPRODUCES THE WIRE'S OWN `reachable` BY ARITHMETIC** — rock's curve is all
	# zeros, so the growth term is nothing and the room is `stock − rung floor × capacity`, which is
	# `extraction::deposit_reachable` at a crew that named no floor. Asserted against the PUBLISHED
	# field rather than against a second copy of that subtraction.
	h._assert_hud("…so the room above it reproduces the sim's published `reachable` (%.1f of %.1f)"
			% [HudDepositVocab.room_next_turn(_worked_down_quarry(),
					SourceForecast.DEFAULT_HARVEST_FLOOR),
				WORKED_QUARRY_REACHABLE],
		is_equal_approx(HudDepositVocab.room_next_turn(_worked_down_quarry(),
			SourceForecast.DEFAULT_HARVEST_FLOOR), WORKED_QUARRY_REACHABLE))
	h._assert_hud("…and the cap is the rung's own recovery over the rate, %d cutters (%d)"
			% [WORKED_QUARRY_MAX_CUTTERS,
				HudDepositVocab.max_useful_cutters(_worked_down_quarry(),
					SourceForecast.DEFAULT_HARVEST_FLOOR)],
		HudDepositVocab.max_useful_cutters(_worked_down_quarry(),
			SourceForecast.DEFAULT_HARVEST_FLOOR) == WORKED_QUARRY_MAX_CUTTERS)
	h._hud._drawercompose.open_deposit_compose(_worked_down_quarry())
	await h._settle()
	var reach_sheet: Node = h._hud._drawercompose._compose_sheet
	# ⛔ **THE CREW IS READ BACK RATHER THAN ASSUMED**, this chapter's rule: the sheet seeds from the
	# band's own `extract` row, and a take asserted at a count the sheet refused is a claim about a
	# number nothing on screen shows. Composed at the 0.5 default the cap was ZERO, so the stepper
	# clamped the crew away and the sheet quoted a take of nothing — which is what this reads back.
	var reach_crew: int = h._hud._compose.deposit_count()
	h._assert_hud("…so the sheet staffs the band's own %d diggers rather than clamping them away (%d)"
			% [WORKED_QUARRY_CUTTERS, reach_crew],
		reach_crew == WORKED_QUARRY_CUTTERS)
	h._assert_hud("…and quotes their whole take, the room being far above it (%s)"
			% Readout.yields_text(reach_sheet),
		Readout.yields_text(reach_sheet).contains(SourceForecast.format_magnitude(
			minf(float(reach_crew) * QUARRY_PER_WORKER, WORKED_QUARRY_REACHABLE))))
	await h._save("workings_quarry_reach")
	h._hud._drawercompose.close_compose_sheet()
	await h._settle()
	# ⛔ **AND THE FIX IS NARROW: ON GROUND THAT RENEWS THE `max` IS EXACTLY WHAT IT WAS.** The scatter
	# is the one fixture here where the composition is not a no-op — it renews, so it keeps its dial,
	# and it stands on the gathering rung whose own floor strands 85% of the stone. Both directions,
	# because either alone passes on a composition that has stopped taking a maximum at all.
	h._assert_hud("a RENEWING working still composes the greater of the two — the rung's %.2f over the dial's %.2f (%.2f)"
			% [GATHERING_RUNG_FLOOR, SourceForecast.DEFAULT_HARVEST_FLOOR,
				HudDepositVocab.composed_floor(_scatter_working(),
					SourceForecast.DEFAULT_HARVEST_FLOOR)],
		is_equal_approx(HudDepositVocab.composed_floor(_scatter_working(),
			SourceForecast.DEFAULT_HARVEST_FLOOR), GATHERING_RUNG_FLOOR))
	h._assert_hud("…and the player's floor still BINDS where it is asked deeper than the rung (%.2f)"
			% HudDepositVocab.composed_floor(_scatter_working(), SCATTER_DEEP_FLOOR),
		is_equal_approx(HudDepositVocab.composed_floor(_scatter_working(), SCATTER_DEEP_FLOOR),
			SCATTER_DEEP_FLOOR))

	# ⛔⛔ **STATE workings-out-of-range — A DEPOSIT CREW COULD BE SENT ANY DISTANCE** (issue #650).
	# Ray, from play: *"Diggers have no range, we apparently can go as far away as we want. Given this
	# involves bringing back the material, the initial dig sites should be limited to the same as
	# foraging. I'm assuming wood harvesting has the same bug."* He was right about the second half
	# too — both branches go through ONE builder, and it measured no distance at all.
	#
	# ⛔ **THE SIM WAS NEVER THE PROBLEM, AND NOTHING SIM-SIDE MOVED.** `systems::labor`'s `Extract`
	# arm lapses an out-of-range crew against `band_work_range`, the same value its `Forage` arm
	# uses, so the limit Ray asked for was already the rule. What was missing was the REFUSAL: the
	# client took the order, sent it, and the sim abandoned the crew on the next turn with nothing but
	# an event-log line — which from the player's seat reads as *no range limit* right up until the
	# crew vanishes. A refusal is strictly kinder than a silent lapse.
	#
	# ⛔ **AND IT IS A PLAIN REFUSAL, NOT THE HUNT SHEET'S OFFER.** A herd beyond reach is offered a
	# detached party (`"…Detach a party to follow it."`); the expedition missions are
	# `scout` / `hunt` / `deny` / `trade`, none of which works ground, so a seam has no such
	# alternative and the forage sheet's plain *no* is the honest answer.
	h._hud.update_band_alerts([_band_beyond_reach()])
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	h._hud._drawercompose.open_deposit_compose(_wood_working(WOOD_OVER_CUT))
	await h._settle()
	await h._save("workings_out_of_range")
	var far_sheet: Node = h._hud._drawercompose._compose_sheet
	# ⛔ **ASSERTED ON THE WHOLE SENTENCE, WITH THE DISTANCE IN IT.** A presence test would pass on a
	# gate that refused every sheet, and the number is the half a player acts on.
	h._assert_hud("a forester sheet on ground beyond the band's reach states the refusal (%s)"
			% BEYOND_REACH_SENTENCE,
		Q.has_label_containing(far_sheet, BEYOND_REACH_SENTENCE))
	# ⛔ **THE SENTENCE IS THE FORAGE SHEET'S OWN, ONE STRING FOR ONE NUMBER.** Both webs are judged
	# against `band_work_range`, so a second spelling would describe one limit as two.
	h._assert_hud("…in the very words the forage sheet refuses in",
		BEYOND_REACH_SENTENCE == HudComposeVocab.WORK_RANGE_REFUSAL_FORMAT % [
			WORKING_TILE_X, WORKING_TILE_Y, BEYOND_REACH_DISTANCE, BandFx.band_fixture()["work_range"]])
	# **AND THE COMMIT IS DEAD, which is the half that stops the order.** The sentence alone would be
	# a warning beside a live button.
	var far_commit := Q.compose_commit_button(far_sheet)
	h._assert_hud("…and the commit it would have sent is refused",
		far_commit != null and far_commit.disabled)
	# ⛔ **AND THE ROCK BESIDE IT IS REFUSED THE SAME WAY** — Ray's *"I'm assuming wood harvesting has
	# the same bug"*, tested rather than assumed. One builder serves both branches, so a gate written
	# on one arm would be the same defect one branch over.
	h._hud._drawercompose.open_deposit_compose(_stone_working(STONE_TAKE))
	await h._settle()
	var far_digger: Node = h._hud._drawercompose._compose_sheet
	var far_digger_commit := Q.compose_commit_button(far_digger)
	h._assert_hud("…and the DIGGER sheet on the same hex refuses in the same sentence",
		Q.has_label_containing(far_digger, BEYOND_REACH_SENTENCE)
			and far_digger_commit != null and far_digger_commit.disabled)
	h._hud.close_compose_sheet()
	await h._settle()

	# ⛔⛔ **STATE workings-tile-crews — THE TILE NOW SAYS THE DIGGING IS HAPPENING** (issue #650).
	# Ray: *"The tile also has no indication the activity is taking place. We probably can do the same
	# that we do for forage sites where we have a minimal display when the owning band is not selected
	# and more details when it is."*
	#
	# **THE FORAGE MECHANISM HE MEANS IS THE `<count> <mark>` PAIR**, and its band-independence is
	# what makes it the minimal level: `SelectionCardController._forage_workers_on_tile` sums the
	# foragers on the hex across EVERY player band, so the land row states what the faction has on
	# this ground whichever subject is picked. `SubjectDrawerController._cutters_on_working` is that
	# function for a working, and `HudDepositVocab.crew_clause` is the pair.
	#
	# ⛔ **TWO WORKINGS ON ONE HEX STAY TWO, AND THE COUNTS DIFFER SO THAT IT CAN BE PROVEN.** Equal
	# crews would pass a card that composed one number and printed it on both rows — the tile-keyed
	# collapse the whole `material` field exists to prevent.
	h._hud.update_band_alerts([_both_seams_band_fixture()])
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	var crew_ctx := DetailFormat.Context.new()
	var crew_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info(), crew_ctx)
	await h._save("workings_tile_crews")
	h._assert_hud("the wood row states its own crew (%s)"
			% Readout.detail_row_value(crew_lines, "Wood"),
		Readout.detail_row_value(crew_lines, "Wood").contains(
			HudDepositVocab.crew_clause(_wood_working(WOOD_OVER_CUT), CARD_WOOD_CUTTERS)))
	h._assert_hud("…and the rock states a DIFFERENT one on the same hex (%s)"
			% Readout.detail_row_value(crew_lines, "Stone"),
		Readout.detail_row_value(crew_lines, "Stone").contains(
			HudDepositVocab.crew_clause(_stone_working(STONE_TAKE), CARD_STONE_CUTTERS)))
	# **THE DISTINCTNESS, STATED AS A NEGATIVE TOO**: neither row may carry the other's count.
	h._assert_hud("…and neither row wears the other's crew",
		not Readout.detail_row_value(crew_lines, "Wood").contains(
				HudDepositVocab.crew_clause(_stone_working(STONE_TAKE), CARD_STONE_CUTTERS))
			and not Readout.detail_row_value(crew_lines, "Stone").contains(
				HudDepositVocab.crew_clause(_wood_working(WOOD_OVER_CUT), CARD_WOOD_CUTTERS)))
	# ⛔ **A MARK AND A COUNT — NEVER A BILL.** The keeping figure and the neglect countdown are
	# retired from this card, and staffing it must not smuggle either back on.
	h._assert_hud("…and the crew arrives with no bill and no countdown beside it",
		not "\n".join(crew_lines).contains(UPKEEP_FACE_NEEDLE)
			and not "\n".join(crew_lines).contains(HudDepositVocab.reverting_value(
				_wood_working(WOOD_OVER_CUT))))

	# ⛔⛔ **STATE workings-tile-crews-other-band — THE COUNT IS THE HEX'S, NOT THE PICKED BAND'S.**
	# The same two workings, held by a band that is NOT the faction's default actor: the rows read
	# exactly as they did above, which is the whole of what "minimal display when the owning band is
	# not selected" buys. A count taken off the selected band would go to zero here.
	h._hud.update_band_alerts(_both_seams_two_bands())
	h._show_tile(_workings_tile([_wood_working(WOOD_OVER_CUT), _stone_working(STONE_TAKE)]))
	await h._settle()
	var other_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	await h._save("workings_tile_crews_other_band")
	# **PRECONDITION: the default actor really does hold nothing here**, or the claim below is met by
	# the wrong band and proves the opposite of what it says.
	h._assert_hud("the faction's default band holds neither working",
		h._hud._band_labor.workers_for_extract(
				h._hud._band_labor.player_band(), WORKING_TILE_X, WORKING_TILE_Y,
				WOOD_MATERIAL_ID) == 0
			and h._hud._band_labor.workers_for_extract(
				h._hud._band_labor.player_band(), WORKING_TILE_X, WORKING_TILE_Y,
				STONE_MATERIAL_ID) == 0)
	h._assert_hud("…and both rows still state the crews the OTHER band has on them (%s | %s)"
			% [Readout.detail_row_value(other_lines, "Wood"),
				Readout.detail_row_value(other_lines, "Stone")],
		Readout.detail_row_value(other_lines, "Wood").contains(
				HudDepositVocab.crew_clause(_wood_working(WOOD_OVER_CUT), CARD_WOOD_CUTTERS))
			and Readout.detail_row_value(other_lines, "Stone").contains(
				HudDepositVocab.crew_clause(_stone_working(STONE_TAKE), CARD_STONE_CUTTERS)))
	# ⛔ **AND UNTOUCHED GROUND SAYS NOTHING ABOUT A CREW AT ALL** — `deposit_row_value`'s rule: every
	# clause about a working being worked is, on ground nobody has opened, a reading of an event that
	# has not happened. The stated ZERO is for a working someone HAS opened, which is the forage land
	# row's own parallel-to-the-staffed-form rule.
	#
	# **THE CREWS HAVE TO GO FOR THIS ONE, and that is the rule in the other direction.** A crew put
	# on a full seam this turn has taken nothing yet, which is field-for-field `is_unopened` — so the
	# mark is drawn for ANY crew and suppressed only where there is no crew and nothing has happened.
	# Staging untouched ground under the band that still holds both workings would therefore assert
	# the opposite of the rule and fail for being right.
	h._hud.update_band_alerts([_band_at_the_working()])
	h._show_tile(_workings_tile([_unopened_wood(), _unopened_stone()]))
	await h._settle()
	var untouched_lines: Array[String] = h._hud._drawer._tile_terrain_lines(
		h._hud._selection.tile_info())
	h._assert_hud("untouched ground carries no crew mark of any count (%s | %s)"
			% [Readout.detail_row_value(untouched_lines, "Wood"),
				Readout.detail_row_value(untouched_lines, "Stone")],
		not Readout.detail_row_value(untouched_lines, "Wood").contains(
				HudSelectionVocab.SOURCE_CREW_MARK)
			and not Readout.detail_row_value(untouched_lines, "Stone").contains(
				HudSelectionVocab.SOURCE_CREW_MARK))

	# **THE HEX IS HANDED BACK BARE**, so a chapter appended after this one starts where every other
	# one does. **An empty `deposits` array means the GROUND HOLDS NOTHING** — not *nobody has worked
	# it*, which is a row like any other.
	h._show_tile(_workings_tile([]))
	await h._settle()
	h._assert_hud("a hex whose ground holds no deposit offers neither assign action",
		not h._hud.forestry_assign_controls.visible
			and not h._hud.extraction_assign_controls.visible)

# ---- THE RUNG CATALOG -------------------------------------------------------------------------
#
# Shaped exactly as `native/src/dict/deposits.rs` writes a `DepositRungState` row, at the shipped
# `intensification_ladder.json` figures — so every name, price, payoff and gate a surface states here
# is the one the wire carries.

const CATALOG_DEADFALL_NAME := "Deadfall"
const CATALOG_FELLING_NAME := "Felling"
const CATALOG_COPPICE_NAME := "Coppice"
const CATALOG_GATHERING_NAME := "Gathering"
const CATALOG_QUARRY_NAME := "Quarry"

## The two knowledge tracks this chapter's gates read, and the roster's own display name for the one
## it names out loud.
const CATALOG_WOODCRAFT := "woodcraft"
const CATALOG_CONSERVATIONISM := "conservationism"
const CATALOG_QUARRYING := "quarrying"

## …and the roster's display name for each — the word a gate's refusal and the sheet's teaching line
## both take the craft from. **The lesson line lower-cases it**, so `Conservationism` here is
## `conservationism` in the sentence, which is the shared composer's own register.
const CATALOG_WOODCRAFT_LABEL := "Woodcraft"
const CATALOG_CONSERVATIONISM_LABEL := "Conservationism"
const CATALOG_QUARRYING_LABEL := "Quarrying"

## `forestry:coppice`'s take rate, which the forestry sheet's deal row quotes at the composed crew.
const CATALOG_COPPICE_YIELD := 2.0

## `extraction:quarry`'s three defining terms: what it reaches, what it eats, and the ground it wants.
const CATALOG_QUARRY_RECOVERY := 0.85
const CATALOG_QUARRY_MATERIAL_ID := "wood"
const CATALOG_QUARRY_MIN_CAPACITY := 100.0

## ⛔ **THE BILL'S OWN SPELLING, WHICH IS WHAT THE CARD MUST NOT CARRY** — the tail of
## `HudDepositVocab.CARD_UPKEEP_FORMAT`, i.e. the words that make a work rate a BILL. Asserted on the
## phrase rather than on the figure, because `0.6` is a number half the card's rows could honestly
## contain and a negative satisfied by coincidence proves nothing.
const UPKEEP_FACE_NEEDLE := "work a turn"

## The `[table=` tag `DetailFormat.detail_bbcode` opens a two-column block with. **The payoff row's
## whole claim is that the card keeps ONE of them**, so the needle is spelled here rather than
## inferred from a rendered string.
const TABLE_OPEN_TAG := "[table="

func _deposit_catalog() -> Array:
	return [
		_rung(HudDepositVocab.RUNG_KEY_DEADFALL, HudDepositVocab.BRANCH_FORESTRY, 1,
			CATALOG_DEADFALL_NAME, "", "", "", CATALOG_WOODCRAFT, 0.0, 0.0, 0.5, 1.0, 1.0, 0.0),
		_rung(HudDepositVocab.RUNG_KEY_FELLING, HudDepositVocab.BRANCH_FORESTRY, 2,
			CATALOG_FELLING_NAME, "fell", CATALOG_WOODCRAFT, HudDepositVocab.RUNG_KEY_DEADFALL,
			CATALOG_CONSERVATIONISM, 60.0, 1.0, 1.0, 1.0, 1.0, 0.0),
		_rung(HudDepositVocab.RUNG_KEY_COPPICE, HudDepositVocab.BRANCH_FORESTRY, 3,
			CATALOG_COPPICE_NAME, "coppice", CATALOG_CONSERVATIONISM,
			HudDepositVocab.RUNG_KEY_FELLING, "", 150.0, 2.0, CATALOG_COPPICE_YIELD, 1.0, 2.0, 0.0),
		_rung(HudDepositVocab.RUNG_KEY_GATHERING, HudDepositVocab.BRANCH_EXTRACTION, 1,
			CATALOG_GATHERING_NAME, "", "", "", CATALOG_QUARRYING, 0.0, 0.0, 0.6, 0.15, 1.0, 0.0),
		_rung(HudDepositVocab.RUNG_KEY_QUARRY, HudDepositVocab.BRANCH_EXTRACTION, 2,
			CATALOG_QUARRY_NAME, "quarry", CATALOG_QUARRYING,
			HudDepositVocab.RUNG_KEY_GATHERING, "", 250.0, 1.5, 1.5, CATALOG_QUARRY_RECOVERY, 1.0,
			CATALOG_QUARRY_MIN_CAPACITY, 8.0, CATALOG_QUARRY_MATERIAL_ID),
	]

func _rung(key: String, branch: String, order: int, display_name: String, verb: String,
		unlock: String, requires: String, earns: String, work_cost: float, upkeep: float,
		yield_rate: float, recovery: float, regrowth: float, min_capacity: float,
		material_cost: float = 0.0, material_id: String = "") -> Dictionary:
	return {
		"rung_key": key,
		"branch": branch,
		"order": order,
		"display_name": display_name,
		"verb": verb,
		"unlock_knowledge": unlock,
		"requires_rung": requires,
		"earns_knowledge": earns,
		"work_cost": work_cost,
		"upkeep_work_per_turn": upkeep,
		"build_material_cost": material_cost,
		"build_material_id": material_id,
		# The sim's own bare worker output, identical on every row — read, never transcribed.
		"build_work_per_worker_turn": 1.0,
		"yield_per_worker_turn": yield_rate,
		"recovery_fraction": recovery,
		"regrowth_multiplier": regrowth,
		"min_deposit_capacity": min_capacity,
	}

func _ladder() -> Array[Dictionary]:
	return HudDepositVocab.deposit_ladder(_deposit_catalog())

func _catalog_entry(rung_key: String) -> Dictionary:
	return HudDepositVocab.ladder_entry_of(_ladder(), rung_key)

## The faction's knowledge row, with `Quarrying` at whatever this state wants and every other track
## learned — so a claim about the quarry's gate cannot be satisfied by a second unmet track.
func _knowledge(quarrying: float) -> Dictionary:
	return {
		CATALOG_WOODCRAFT: KNOWLEDGE_LEARNED,
		CATALOG_CONSERVATIONISM: KNOWLEDGE_LEARNED,
		CATALOG_QUARRYING: quarrying,
	}

## …and the roster's display names for them, **read back off the HUD's own ingest** rather than
## restated here: the gate's refusal and the sheet's teaching line must name a craft with one word,
## and a second table in this file is exactly how they would come to name it with two.
func _knowledge_labels() -> Dictionary:
	return h._hud._topbar.knowledge_labels()

## **THE `ladder_knowledge` ROSTER FOR THE TWO DEPOSIT BRANCHES**, in `native/src/dict/knowledge.rs`'
## own row shape. The shared `fixtures_knowledge.gd` roster carries the plant, animal and route
## ladders and none of these, so this chapter states its own three: a knowledge roster is per WORLD,
## and this chapter runs last.
func _deposit_knowledge_roster() -> Array:
	return [
		_knowledge_row(CATALOG_WOODCRAFT, CATALOG_WOODCRAFT_LABEL,
			HudDepositVocab.BRANCH_FORESTRY, 1),
		_knowledge_row(CATALOG_CONSERVATIONISM, CATALOG_CONSERVATIONISM_LABEL,
			HudDepositVocab.BRANCH_FORESTRY, 2),
		_knowledge_row(CATALOG_QUARRYING, CATALOG_QUARRYING_LABEL,
			HudDepositVocab.BRANCH_EXTRACTION, 1),
	]

func _knowledge_row(id: String, display: String, branch: String, order: int) -> Dictionary:
	return {
		"knowledge_id": id, "display_name": display, "branch": branch,
		"order": order, "is_step": true,
	}

# ---- FIXTURES ---------------------------------------------------------------------------------
#
# Shaped exactly as `native/src/dict/deposits.rs` writes a row — one per `(tile, material)`, every
# derived number read live off the tile sim-side — so a claim made here is a claim about the wire.

## The tile card's payload, carrying whatever deposits this state stages. **`[]` means the GROUND
## HOLDS NOTHING**: the section stands a row on every discovered deposit-bearing tile, so an absent
## row is absent ground rather than untouched ground.
## **THE FULL LAND CARD — a hex carrying both seams, both food webs AND a road**, in the given sight
## state. It is the shape Ray was reading when he asked for the road row to move, which is why it is
## composed from the SHARED `BaseFx.food_tile_fixture()` (the forage patch, the pasture and the food
## module) rather than from this chapter's bare `_workings_tile`, which states deposits and nothing
## else and so could not show what the road is being ordered against.
##
## The deposits and the road are both re-homed onto this chapter's own hex, so every row on the card
## is about one piece of ground.
func _full_land_tile(visibility_state: String) -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["x"] = WORKING_TILE_X
	tile["y"] = WORKING_TILE_Y
	tile["visibility_state"] = visibility_state
	tile["deposits"] = [_unopened_wood(), _unopened_stone()]
	tile["roads"] = [_worn_path()]
	return tile

## **A BAND WORKING THE WOOD AND NOT THE ROCK** — the one shape that puts a standing summary on ONE of
## the hex's two deposit buttons, which is what makes `workings_worked_buttons` a pair rather than a
## sample. The `extract` row names its MATERIAL because `(tile, material)` is the assignment's whole
## identity and a row without it stages an assignment `LaborTarget::Extract` cannot produce.
func _working_band_fixture() -> Dictionary:
	var band := _band_at_the_working()
	var rows: Array = band["labor_assignments"]
	rows.append({
		"kind": HudConst.LABOR_KIND_EXTRACT,
		"workers": WORKED_BUTTON_CUTTERS,
		"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
		"material": WOOD_MATERIAL_ID,
		# ⛔ **A WORKING'S TAKE IS A MATERIAL, AND THIS FIXTURE USED TO STATE IT AS FOOD** (issue #650).
		# `actual_yield` is the FOOD account, which `systems/labor.rs`' `Extract` arm leaves at
		# `SourceYield::ZERO` on purpose so a deposit cannot pollute the band's `food_income`; what a
		# working pays rides `material_yield`. Carrying `0.60` in the food slot made this frame render
		# `+0.60 /turn` — the food unit — while claiming in its own comment to show `+0.60 wood`, which
		# is exactly how a harness authored against the wrong field passed a client that was wrong the
		# same way. The row now states what the wire states.
		"actual_yield": 0.0,
		"sustainable_yield": 0.0,
		"material_yield": [{"material_id": WOOD_MATERIAL_ID, "amount": WORKED_BUTTON_TAKE}],
		"workers_needed": WORKED_BUTTON_CUTTERS,
	})
	return band

## Ray's own numbers, so the frame is the card he was reading: two foresters and the take beside them.
const WORKED_BUTTON_CUTTERS := 2
const WORKED_BUTTON_TAKE := 0.60

## The wire's own id for the wood seam's material — the `(tile, material)` pair's second half on the
## deposit row, on the `extract` labor row and on the material payoff the row pays. Spelled once, so a
## fixture cannot key three surfaces apart with three spellings of one material.
const WOOD_MATERIAL_ID := "wood"

## …and the rock's, for the same reason. It was spelled inline while nothing joined on it; the crew
## clauses' band-independence claim looks a working up by the PAIR, and a typo there would answer
## `0` and pass a negative assertion.
const STONE_MATERIAL_ID := "stone"

## **THE TWO CREWS THE TILE CARD STATES, AND THEY DIFFER ON PURPOSE** (issue #650). Equal counts
## would be satisfied by a card that composed one number and printed it on both of the hex's rows,
## which is the tile-keyed collapse the `(tile, material)` key exists to prevent.
const CARD_WOOD_CUTTERS := 2
const CARD_STONE_CUTTERS := 4

## A band cutting BOTH seams on this hex, each at its own crew — the fixture the tile card's two crew
## clauses are read against.
func _both_seams_band_fixture() -> Dictionary:
	var band := _band_at_the_working()
	var rows: Array = band["labor_assignments"]
	for held in [[WOOD_MATERIAL_ID, CARD_WOOD_CUTTERS], [STONE_MATERIAL_ID, CARD_STONE_CUTTERS]]:
		rows.append({
			"kind": HudConst.LABOR_KIND_EXTRACT, "workers": int(held[1]),
			"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
			"material": String(held[0]),
		})
	return band

## …and the pair that makes the card's count BAND-INDEPENDENT rather than merely correct: a second
## band listed FIRST, so it is the faction's default actor (`HudBandLaborState.player_band`) and the
## panel's subject, holding nothing on this hex at all. If the rows read off the picked band they go
## to zero here; they do not, because the count is the GROUND's.
func _both_seams_two_bands() -> Array:
	var idle := _band_at_the_working()
	idle["entity"] = IDLE_BAND_ENTITY
	idle["name"] = IDLE_BAND_NAME
	idle["id"] = IDLE_BAND_NAME
	idle["labor_assignments"] = []
	return [idle, _both_seams_band_fixture()]

## The second band's identity. Its own entity — a duplicate would collapse the two rosters into one
## band and the claim would be about nothing.
const IDLE_BAND_ENTITY := 907
const IDLE_BAND_NAME := "Coldhollow"

## **HOW FAR `BandFx.band_fixture()`'s CAMP IS FROM THIS CHAPTER'S HEX** — (71,18) to (21,14) by the
## client's own odd-r cube distance, transcribed rather than computed here so the frame pins the
## arithmetic instead of restating it. Against the fixture's shipped `work_range` of 2 it is the
## refusal's whole reason.
const BEYOND_REACH_DISTANCE := 52

## `BandFx.band_fixture()`'s own `work_range`, transcribed: a const expression cannot read a fixture
## dictionary, and the state that uses it compares the two so a drift in the fixture fails loudly.
const BEYOND_REACH_WORK_RANGE := 2

## …and the sentence that refusal reads, composed from the SHARED format so this file cannot freeze a
## wording the client has moved on from.
const BEYOND_REACH_SENTENCE := HudComposeVocab.WORK_RANGE_REFUSAL_FORMAT % [
	WORKING_TILE_X, WORKING_TILE_Y, BEYOND_REACH_DISTANCE, BEYOND_REACH_WORK_RANGE]

## The road those two frames stand on — see `ROAD_PATH_METER` for why it helps nobody and owes
## nobody. Shaped as `native/src/dict/routes.rs` writes a road row, with the wire's own
## `demand − supplied == shortfall` identity held on both currencies so the fixture stays inside what
## the sim can emit.
func _worn_path() -> Dictionary:
	return {
		"tile_x": WORKING_TILE_X,
		"tile_y": WORKING_TILE_Y,
		"has_keeper": false,
		"keeper_band_id": ROAD_NO_KEEPER,
		"keeper_remoteness": 0.0,
		"rung": HudRouteVocab.RUNG_KEY_PATH,
		"build_fraction": ROAD_PATH_METER,
		"upkeep_demand": 0.0,
		"upkeep_supplied": 0.0,
		"upkeep_shortfall": 0.0,
		"upkeep_workers_needed": 0,
		"has_neglect_grace": false,
		"neglect_grace_remaining": 0,
		"grants_sight": false,
		"friction_multiplier": HudRouteVocab.ROAD_FRICTION_NO_HELP,
		"holds_link_to_tiles": HudRouteVocab.ROAD_LINK_NONE,
		"build_material_demand": 0.0,
		"build_material_supplied": 0.0,
		"upkeep_material_demand": 0.0,
		"upkeep_material_supplied": 0.0,
		"build_blocked_reason": "",
	}

func _workings_tile(workings: Array) -> Dictionary:
	return {
		"x": WORKING_TILE_X, "y": WORKING_TILE_Y,
		"terrain_label": "Karst Highland",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.21,
		"temperature": 11.0,
		"deposits": workings,
	}

## **THE SAMPLED GROWTH CURVE, BUILT THE WAY THE SIM BUILDS IT** — `deposit_regrowth(s) − s` at
## `s = i/(n−1) × capacity`, with the seed read INSIDE the logistic term rather than lifted onto the
## stock. That is the whole of why rock stays at zero: the seed is multiplied by the deposit's own
## rate, so a rate of `0` seeds exactly nothing and every sample comes back `0` — a live reading of
## *this does not grow*, which is a different claim from the EMPTY vector meaning *no curve was sent*.
func _deposit_regrowth_samples(capacity: float, rate: float) -> PackedFloat32Array:
	var samples := PackedFloat32Array()
	for index in range(REGROWTH_SAMPLE_COUNT):
		var stock := float(index) / float(REGROWTH_SAMPLE_COUNT - 1) * capacity
		var seeded := maxf(stock, DEPOSIT_SEED_FRACTION * capacity)
		var delta := maxf(rate * seeded * (1.0 - seeded / capacity), 0.0)
		samples.push_back(minf(stock + delta, capacity) - stock)
	return samples

## A FELLING working on mixed woodland, cut at whatever rate the caller names. Its keeping is SHORT,
## which is what puts the hazard word on the row and the bill on the hover.
func _wood_working(actual_take: float) -> Dictionary:
	return {
		"tile_x": WORKING_TILE_X, "tile_y": WORKING_TILE_Y,
		"material": "wood",
		"branch": HudDepositVocab.BRANCH_FORESTRY,
		"stock": WOOD_STOCK,
		"capacity": WOOD_CAPACITY,
		"reachable": WOOD_REACHABLE,
		"regrowth_rate": WOOD_REGROWTH,
		"rung": HudDepositVocab.RUNG_KEY_FELLING,
		"build_fraction": METER_CLIMBING,
		"ladder_position": 60.0,
		"sustainable_take": WOOD_SUSTAINABLE,
		"actual_take": actual_take,
		"turns_remaining": RUNWAY_RENEWS,
		"upkeep_demand": WOOD_UPKEEP_DEMAND,
		"upkeep_supplied": WOOD_UPKEEP_SUPPLIED,
		"upkeep_shortfall": WOOD_UPKEEP_SHORTFALL,
		"upkeep_workers_needed": WOOD_UPKEEP_KEEPERS,
		"has_neglect_grace": true,
		"neglect_grace_remaining": WOOD_GRACE_LEFT,
		"build_turns_remaining": BUILD_NO_ESTIMATE,
		"build_blocked_reason": "",
		"is_queued": false,
		"build_kit_id": "",
		"upkeep_kit_id": "",
		"upkeep_kit_named": false,
		"rung_floor_fraction": FORESTRY_RUNG_FLOOR,
		"per_worker_biomass": FELLING_PER_WORKER,
		"regrowth_samples": _deposit_regrowth_samples(WOOD_CAPACITY, WOOD_REGROWTH),
	}

## …and the same wood raised to its branch's top, which is the rung whose payoff is RENEWAL.
func _coppiced_wood() -> Dictionary:
	var working := _wood_working(WOOD_SAFE_TAKE)
	working["rung"] = HudDepositVocab.RUNG_KEY_COPPICE
	working["build_fraction"] = METER_NOTHING_RISING
	working["regrowth_rate"] = WOOD_REGROWTH * 2.0
	working["per_worker_biomass"] = COPPICE_PER_WORKER
	# **THE CURVE IS THE ONE THE RUNG BOUGHT** — the coppice doubles the ground's own rate, and the
	# samples are what the client draws, so a fixture leaving the felling curve here would render a
	# managed wood as an unmanaged one on the only surface that shows the payoff.
	working["regrowth_samples"] = _deposit_regrowth_samples(WOOD_CAPACITY, WOOD_REGROWTH * 2.0)
	return working

## A GATHERING working on the rock body beneath — the extraction branch's FREE FLOOR, so it owes
## nothing and has no meter to lose. Finite by arithmetic: rock's rate is zero, which is what makes
## `sustainable_take` honestly `0` and the runway its warning instead.
func _stone_working(actual_take: float) -> Dictionary:
	return {
		"tile_x": WORKING_TILE_X, "tile_y": WORKING_TILE_Y,
		"material": "stone",
		"branch": HudDepositVocab.BRANCH_EXTRACTION,
		"stock": STONE_STOCK,
		"capacity": STONE_CAPACITY,
		"reachable": STONE_REACHABLE,
		"regrowth_rate": STONE_REGROWTH,
		"rung": HudDepositVocab.RUNG_KEY_GATHERING,
		"build_fraction": METER_NOTHING_RISING,
		"ladder_position": 0.0,
		"sustainable_take": 0.0,
		"actual_take": actual_take,
		"turns_remaining": STONE_RUNWAY,
		"upkeep_demand": FREE_FLOOR_NO_UPKEEP,
		"upkeep_supplied": FREE_FLOOR_NO_UPKEEP,
		"upkeep_shortfall": FREE_FLOOR_NO_UPKEEP,
		"upkeep_workers_needed": 0,
		"has_neglect_grace": false,
		"neglect_grace_remaining": 0,
		"build_turns_remaining": BUILD_NO_ESTIMATE,
		"build_blocked_reason": "",
		"is_queued": false,
		"build_kit_id": "",
		"upkeep_kit_id": "",
		"upkeep_kit_named": false,
		"rung_floor_fraction": GATHERING_RUNG_FLOOR,
		"per_worker_biomass": GATHERING_PER_WORKER,
		# **ALL ZEROS, AND THAT IS A READING RATHER THAN AN ABSENCE.** Rock's rate is zero, so every
		# sample is exactly `0` — and the client's dial forks on `regrowth_rate` rather than on this,
		# which is what keeps a renewing flint scatter of the SAME branch on the other side of the fork.
		"regrowth_samples": _deposit_regrowth_samples(STONE_CAPACITY, STONE_REGROWTH),
	}

## …and the same body once a quarry stands on it, drawn down: the row that states the reach the ladder
## bought and the payoff row that says so.
func _quarried_stone() -> Dictionary:
	var working := _stone_working(STONE_TAKE)
	working["rung"] = HudDepositVocab.RUNG_KEY_QUARRY
	working["stock"] = QUARRY_STOCK
	working["reachable"] = QUARRY_REACHABLE
	working["rung_floor_fraction"] = QUARRY_RUNG_FLOOR
	working["per_worker_biomass"] = QUARRY_PER_WORKER
	return working

## ⛔ **THE SAME QUARRY WORKED DOWN BETWEEN THE TWO FLOORS** — the regression fixture for issue #650's
## client half, and the one working in this chapter on which `composed_floor`'s *"only where it
## renews"* condition changes an answer. See `WORKED_QUARRY_STOCK` for why this stock and no other.
## Its `reachable` is the sim's own `deposit_reachable` at the rung's floor, so the room the sheet
## composes and the reach the wire published are one number rather than two.
func _worked_down_quarry() -> Dictionary:
	var working := _quarried_stone()
	working["stock"] = WORKED_QUARRY_STOCK
	working["reachable"] = WORKED_QUARRY_REACHABLE
	working["actual_take"] = WORKED_QUARRY_TAKE
	working["turns_remaining"] = WORKED_QUARRY_RUNWAY
	return working

## **A BAND WITH THREE DIGGERS STANDING ON IT** — so the sheet SEEDS at that crew rather than at the
## `WORKER_STEP` floor, and the take it quotes is a figure this fixture states rather than one the
## stepper happened to land on.
func _worked_quarry_band_fixture() -> Dictionary:
	var band := _band_at_the_working()
	var rows: Array = band["labor_assignments"]
	rows.append({
		"kind": HudConst.LABOR_KIND_EXTRACT,
		"workers": WORKED_QUARRY_CUTTERS,
		"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
		"material": "stone",
		# ⛔ **THE FLOOR THE SHEET WILL COMPOSE AT, and it is the whole defect in one field.** A finite
		# seam is offered no dial, so the row carries the sim's own omitted-token default — inert
		# sim-side since #650, and what the client used to compose above the rung's floor anyway.
		"floor": SourceForecast.DEFAULT_HARVEST_FLOOR,
		"actual_yield": 0.0,
		"sustainable_yield": 0.0,
		"material_yield": [{"material_id": "stone", "amount": WORKED_QUARRY_TAKE}],
		"workers_needed": WORKED_QUARRY_CUTTERS,
	})
	return band

## …the same quarry with NOBODY cutting it: `-2` on the runway and a take of nothing.
func _idle_stone_working() -> Dictionary:
	var working := _stone_working(0.0)
	working["turns_remaining"] = RUNWAY_IDLE
	return working

## …and the gathering seam with a QUARRY ORDERED on it, part-raised. **The queue flag is what admits
## the row to the building arm** — `build_fraction` measures the rung at RISK, so on a working with
## nothing banked above it the meter reads the rung already held.
func _queued_stone_working() -> Dictionary:
	var working := _stone_working(STONE_TAKE)
	working["is_queued"] = true
	working["build_fraction"] = METER_CLIMBING
	working["build_turns_remaining"] = BUILD_TURNS_QUEUED
	return working

## **UNTOUCHED WOODLAND — the row the sim derives for a tile nobody has opened**, and the ordinary
## case on a revealed map. `DepositSource::opening`'s own fingerprint: the seam FULL, the branch's
## free floor, nothing banked, no bill, nothing queued and nothing taken.
func _unopened_wood() -> Dictionary:
	var deposit := _wood_working(UNOPENED_NO_TAKE)
	deposit["stock"] = WOOD_CAPACITY
	deposit["reachable"] = UNOPENED_WOOD_REACHABLE
	deposit["rung"] = HudDepositVocab.RUNG_KEY_DEADFALL
	deposit["build_fraction"] = HudDepositVocab.METER_UNSTARTED
	deposit["ladder_position"] = HudDepositVocab.LADDER_UNSTARTED
	deposit["upkeep_demand"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_supplied"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_shortfall"] = FREE_FLOOR_NO_UPKEEP
	deposit["upkeep_workers_needed"] = 0
	deposit["has_neglect_grace"] = false
	deposit["neglect_grace_remaining"] = 0
	return deposit

## …and the FINITE half of the same hex, on the extraction floor. ⛔ **Its runway sentinel is `-2`**,
## which on a working somebody opened reads *not being worked* — an honest sentence there and a false
## one here, where there is no working.
func _unopened_stone() -> Dictionary:
	var deposit := _stone_working(UNOPENED_NO_TAKE)
	deposit["stock"] = STONE_CAPACITY
	deposit["reachable"] = UNOPENED_STONE_REACHABLE
	deposit["build_fraction"] = HudDepositVocab.METER_UNSTARTED
	deposit["turns_remaining"] = RUNWAY_IDLE
	return deposit

## **THE SAME WOOD WITH NO CURVE ON THE WIRE** — an EMPTY sample vector, which is the claim *no curve
## was sent* rather than *this does not grow*. It is what has no projection to walk, and therefore the
## one state where the sheet's verdict is `deposit_verdict`'s own renewing arm.
func _curveless_wood() -> Dictionary:
	var working := _wood_working(WOOD_OVER_CUT)
	working["regrowth_samples"] = PackedFloat32Array()
	return working

## **A WOOD STANDING ABOVE EVERY PRESET'S FLOOR** — the stand the dial is compared ON, so the three
## preset frames differ by the FLOOR and by nothing else. Its take is inside its own renewal, which is
## also what puts the shared `renewable` note on the row where the over-cut fixture wears the ⚠.
const STANDING_WOOD_STOCK := 552.0

func _standing_wood() -> Dictionary:
	var working := _wood_working(WOOD_SAFE_TAKE)
	working["stock"] = STANDING_WOOD_STOCK
	working["reachable"] = STANDING_WOOD_STOCK
	return working

## …and the same stand being cut over its renewal, which is what puts the ⚠ on the readout's row while
## the dial is dragged. The stock is the standing one so the drag has room to move in; only the TAKE
## differs, which is the term the over-cut predicate reads.
func _over_cut_standing_wood() -> Dictionary:
	var working := _standing_wood()
	working["actual_take"] = WOOD_OVER_CUT
	return working

## **THE SAME WOOD STANDING EXACTLY AT THE FOOD PEAK** — `floor × capacity` of stock, which is the
## steady state a Sustain dial produces and the one the `now → after` pair collapses at. The stand is
## stated as a STOCK and not as a floor field: `DepositState` publishes none (PR #651 review — no
## GDScript was allowed to read one, so it left the wire), and where this sheet composes at is the
## BAND's own `extract` row.
func _wood_at_its_floor() -> Dictionary:
	var working := _wood_working(WOOD_SUSTAINABLE)
	working["stock"] = SourceForecast.FLOOR_FOOD_PEAK * WOOD_CAPACITY
	working["reachable"] = 0.0
	return working

## **A BAND THAT HAS JUST PUT DIGGERS ON THE ROCK AND CUT NOTHING YET** — the `extract` row carries
## the crew and the SEEDED rate, which is all three of the terms the runway is told apart by.
func _fresh_stone_band_fixture() -> Dictionary:
	var band := _band_at_the_working()
	var rows: Array = band["labor_assignments"]
	rows.append({
		"kind": HudConst.LABOR_KIND_EXTRACT,
		"workers": FRESH_STONE_CUTTERS,
		"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
		"material": "stone",
		"floor": SourceForecast.DEFAULT_HARVEST_FLOOR,
		"actual_yield": 0.0,
		"sustainable_yield": 0.0,
		"material_yield": [{"material_id": "stone", "amount": FRESH_STONE_SEED}],
		"workers_needed": 0,
	})
	return band

## Two diggers at `extraction:gathering`'s shipped `0.4` a worker-turn, and the runway that buys on a
## seam with 330 units inside the rung's reach: `floor(330 / 0.8)`. Stated rather than computed, so the
## frame asserts against the sim's own division rather than against a second copy of it.
const FRESH_STONE_CUTTERS := 2
const FRESH_STONE_SEED := 0.8
const FRESH_STONE_RUNWAY := 412

## …and the RENEWING half of the same state: a scatter nobody has cut yet, with a crew committed to it
## this turn. It is what makes the over-cut verdict's three states testable — the wire's `actual_take`
## is `0` here, and the sentence must quote the seeded rate instead.
const FRESH_SCATTER_SEED := 1.2

func _fresh_scatter_working() -> Dictionary:
	var working := _scatter_working()
	working["actual_take"] = HudDepositVocab.TAKE_NONE
	working["turns_remaining"] = RUNWAY_RENEWS
	return working

func _fresh_scatter_assignment() -> Dictionary:
	return {
		"kind": HudConst.LABOR_KIND_EXTRACT,
		"workers": FRESH_STONE_CUTTERS,
		"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
		"material": "flint",
		"material_yield": [{"material_id": "flint", "amount": FRESH_SCATTER_SEED}],
	}

## **A LOOSE-STONE SCATTER — `extraction` AND RENEWING, AND TOO SMALL FOR A QUARRY.** The one fixture
## that tells a `regrowth_rate` fork from a `branch` fork, and the ground the SITE gate refuses.
func _scatter_working() -> Dictionary:
	var working := _stone_working(SCATTER_OVER_CUT)
	working["material"] = "flint"
	working["stock"] = SCATTER_STOCK
	working["capacity"] = SCATTER_CAPACITY
	working["reachable"] = SCATTER_STOCK
	working["regrowth_rate"] = SCATTER_REGROWTH
	working["sustainable_take"] = SCATTER_SUSTAINABLE
	working["turns_remaining"] = RUNWAY_RENEWS
	# ⛔ **THE ONE FIXTURE WHERE THE COMPOSITION'S `max` IS NOT A NO-OP.** It renews, so it is offered
	# the dial — and it stands on `extraction:gathering`, whose own floor strands 85% of the scatter.
	# A client that ADDED the two floors instead of taking the greater draws this crew stopping 85% of
	# the ground short of where it really stops, on every dial position.
	working["regrowth_samples"] = _deposit_regrowth_samples(SCATTER_CAPACITY, SCATTER_REGROWTH)
	return working

# ---- READING THE SURFACES ---------------------------------------------------------------------

## EVERY payoff row's value on the card, in the order they were emitted — the blank key's own reader,
## since a hex can carry one per material and `Readout.detail_row_value` answers with the first.
func _payoff_values(lines: Array[String]) -> Array[String]:
	var prefix := HudDepositVocab.DEPOSIT_PAYOFF_ROW + DetailFormat.DETAIL_KV_SEPARATOR
	var out: Array[String] = []
	for line in lines:
		if line.begins_with(prefix):
			out.append(line.substr(prefix.length()))
	return out

## **A BAND THAT HAS JUST COMMITTED BOTH CREWS AND RESOLVED NEITHER** — the state issue #650 was
## reported in, on ONE hex so the two webs are read side by side.
##
## The FORAGE row carries what the sim's assign-time seed writes (`seed_source_yield`): the patch's
## own published take at this crew, `min(crew × patch_per_worker_yield, patch_ceiling_sustain)` off
## `BaseFx.food_tile_fixture()`, so the fixture quotes the tile card's own numbers rather than a
## figure invented here.
##
## The EXTRACT row carries what the wire sends for a working nobody has RESOLVED but somebody has
## committed to: a zero food account (`systems/labor.rs`' `Extract` arm leaves `SourceYield::ZERO`
## there deliberately, so a deposit cannot pollute `food_income`) and a SEEDED material vector.
##
## ⛔ **THE SEED IS NEW AND THIS FIXTURE STATED ITS ABSENCE** (issue #650). `seed_source_yield`
## returned early on `Extract` when this state was authored, so the row here carried an EMPTY material
## vector and the frame's whole claim was the UNIT. The sim's `Extract` arm now prices the take
## through the very seam the turn takes — `min(hands, the reach above the composed floor)` on a
## regrown clone of the working — so the honest fixture states a FIGURE, and what the frame proves is
## that both webs answer a crew committed this turn with a real rate in their own account.
func _just_assigned_band_fixture() -> Dictionary:
	var band := _band_at_the_working()
	band["labor_assignments"] = [
		{
			"kind": SourceForecast.LABOR_KIND_FORAGE,
			"workers": FRESH_FORAGE_CREW,
			"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
			"floor": SourceForecast.DEFAULT_HARVEST_FLOOR,
			"actual_yield": FRESH_FORAGE_SEED,
			"sustainable_yield": FRESH_FORAGE_SEED,
			"realized_yield": FRESH_FORAGE_SEED,
			"workers_needed": FRESH_FORAGE_CREW,
			"overdraws": false,
		},
		{
			"kind": HudConst.LABOR_KIND_EXTRACT,
			"workers": WORKED_BUTTON_CUTTERS,
			"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
			"material": WOOD_MATERIAL_ID,
			"floor": SourceForecast.DEFAULT_HARVEST_FLOOR,
			"actual_yield": 0.0,
			"sustainable_yield": 0.0,
			"material_yield": [{"material_id": WOOD_MATERIAL_ID, "amount": FRESH_WOOD_SEED}],
			"workers_needed": 0,
		},
	]
	return band

## What the seed pays those two cutters on the hex's UNOPENED wood: `2 × 0.3` at
## `forestry:deadfall`'s shipped bare-handed rate, which the whole material economy bootstraps
## through. Stated rather than computed so the frame asserts against the sim's arithmetic.
const FRESH_WOOD_SEED := 0.6

## The gatherers on that patch and what the seed pays them: `min(3 × 0.32, 0.96)` at the shipped
## `food_tile_fixture` rates, i.e. the patch's whole Sustain ceiling. Stated rather than computed here
## so the frame asserts against the sim's arithmetic rather than against a second copy of it.
const FRESH_FORAGE_CREW := 3
const FRESH_FORAGE_SEED := 0.96

## The forage drawer's standing summary text — **the `Assign … ▸` button's SECOND LINE**, read
## through the stacked cell exactly as the deposit reader below does, so the pair is compared through
## one shape.
func _forage_summary_text() -> String:
	var controls = h._hud.forage_assign_controls
	if controls == null or controls.get_child_count() == 0:
		return ""
	return _summary_text(controls.get_child(0))

## …and the same read for any stacked action cell (or the `Button` inside one). `""` where the control
## carries no second line at all, which is the "nobody works this source" state.
func _summary_text(node: Node) -> String:
	if node == null:
		return ""
	var flow := Q.stacked_action_summary(node)
	if flow == null or flow.get_child_count() == 0:
		return ""
	var label := flow.get_child(0) as Label
	return label.text if label != null else ""

## One branch's `Assign … ▸` button, or `null` where the hex carries no deposit of that branch.
func _assign_button(host: Node, branch: String) -> Button:
	return Q.find_button_by_text(host, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT
		% HudDepositVocab.crew_noun(branch).to_lower())

## The sheet's commit button, by the shared meta every compose sheet marks it with.
func _commit_face(sheet: Node) -> String:
	var node := Q.find_meta_node(sheet, HudWidgets.COMPOSE_COMMIT_META)
	return (node as Button).text if node is Button else ""

## ⛔ **ANY control under `root` carrying `meta`, whatever its VALUE.** `node_query.find_meta_node`
## answers the same question, and these two claims are ABSENCES: what has to be true is that no floor
## preset and no chart were built at all, which is a question about the meta's presence rather than
## about a particular preset's key.
func _first_meta(root: Node, meta: String) -> Node:
	if root == null:
		return null
	if root is Control and (root as Control).has_meta(meta):
		return root
	for child in root.get_children():
		var found := _first_meta(child, meta)
		if found != null:
			return found
	return null

## The pointer line's parsed text. It is the ONE state built as a `RichTextLabel` (the `Work tab`
## `[url]` has to flow with the sentence), so its words are read through `get_parsed_text`.
func _offer_text(sheet: Node) -> String:
	var node := _first_meta(sheet, HudWidgets.IMPROVEMENT_CONTROL_META)
	return (node as RichTextLabel).get_parsed_text() if node is RichTextLabel else ""

# ---- READING THE LADDER'S ROWS ----------------------------------------------------------------
#
# The track is produced here and rendered on the Work board, so these read the PRODUCER's rows. A
# frame could show one state; the branch has four, and each is a different sentence.

func _row(rows: Array[Dictionary], rung_key: String) -> Dictionary:
	for row in rows:
		if String(row.get(RungLadder.ROW_RUNG_KEY, "")) == rung_key:
			return row
	return {}

func _row_face(rows: Array[Dictionary], rung_key: String) -> String:
	return String(_row(rows, rung_key).get(RungLadder.ROW_FACE_KEY, ""))

func _row_tooltip(rows: Array[Dictionary], rung_key: String) -> String:
	return String(_row(rows, rung_key).get(RungLadder.ROW_TOOLTIP_KEY, ""))

func _row_state(rows: Array[Dictionary], rung_key: String) -> String:
	return String(_row(rows, rung_key).get(RungLadder.ROW_STATE_KEY, ""))

func _row_selectable(rows: Array[Dictionary], rung_key: String) -> bool:
	return bool(_row(rows, rung_key).get(RungLadder.ROW_SELECTABLE_KEY, false))

func _row_asides(rows: Array[Dictionary], rung_key: String) -> Array[String]:
	var out: Array[String] = []
	for aside in Array(_row(rows, rung_key).get(RungLadder.ROW_BUILD_ASIDES_KEY, [])):
		if aside is Dictionary:
			out.append(String((aside as Dictionary).get(RungLadder.RUNG_ASIDE_TEXT_KEY, "")))
	return out
