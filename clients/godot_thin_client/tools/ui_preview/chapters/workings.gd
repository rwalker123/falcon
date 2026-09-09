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
const EXPECTED_CHECKPOINTS := 59

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The hex every state in this chapter stands on. Its own coordinates, so a working staged here can
## never be confused with the road frames' tile.
const WORKING_TILE_X := 21
const WORKING_TILE_Y := 14

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
	h._hud.update_band_alerts([BandFx.band_fixture()])
	# **THE CATALOG IS PER WORLD AND EVERY SURFACE HERE READS IT** — the rung's name, its price, what
	# it buys and every gate. Pushed through the real ingest so a claim made below is a claim about
	# the wire rather than about a table this chapter holds.
	h._hud.update_deposit_rungs(_deposit_catalog())
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
	h._assert_hud("…the wood row reads `stock of capacity · rung · hazard` (%s)"
			% Readout.detail_row_value(lines, "Wood"),
		Readout.detail_row_value(lines, "Wood") == HudDepositVocab.DEPOSIT_CLAUSE_SEPARATOR.join([
			HudDepositVocab.DEPOSIT_STOCK_DRAWN_FORMAT % [
				DetailFormat.format_trimmed(WOOD_STOCK, HudDepositVocab.CARD_STOCK_DECIMALS),
				DetailFormat.format_trimmed(WOOD_CAPACITY, HudDepositVocab.CARD_STOCK_DECIMALS)],
			CATALOG_FELLING_NAME,
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
		Readout.detail_row_value(payoff_lines, "Stone") == "%s%s%s" % [
			HudDepositVocab.DEPOSIT_STOCK_DRAWN_FORMAT % [
				DetailFormat.format_trimmed(QUARRY_STOCK, HudDepositVocab.CARD_STOCK_DECIMALS),
				DetailFormat.format_trimmed(STONE_CAPACITY, HudDepositVocab.CARD_STOCK_DECIMALS)],
			HudDepositVocab.DEPOSIT_CLAUSE_SEPARATOR, CATALOG_QUARRY_NAME])
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
		# ⛔ **NO FLOOR DIAL AND NO CHART ON EITHER BRANCH**, and a disabled or empty one would be
		# furniture explaining an absence rather than an honest silence.
		h._assert_hud("…and neither a floor preset nor its chart is drawn at all",
			_first_meta(sheet, HudWidgets.POLICY_RUNG_META) == null
				and _first_meta(sheet, HudWidgets.FLOOR_CHART_META) == null)
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
		h._assert_hud("…and a renewing seam cut over its renewal reads the food webs' own word (%s)"
				% Readout.yields_text(sheet),
			Readout.yields_text(sheet).contains(HudDepositVocab.over_cut_word().to_upper()))
		h._assert_hud("…and its verdict is the over-cut sentence, not a reach (%s)"
				% Readout.verdict_text(sheet),
			Readout.verdict_text(sheet).contains(
				HudDepositVocab.DEPOSIT_VERDICT_OVER_CUT_FORMAT % [
				DetailFormat.format_trimmed(WOOD_OVER_CUT, HudDepositVocab.CARD_STOCK_DECIMALS),
				DetailFormat.format_trimmed(WOOD_SUSTAINABLE, HudDepositVocab.CARD_STOCK_DECIMALS)]))
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

## …and the roster's display names for them, which is where a gate reason takes the craft's word from.
func _knowledge_labels() -> Dictionary:
	return {CATALOG_QUARRYING: CATALOG_QUARRYING_LABEL}

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
	var band := BandFx.band_fixture()
	var rows: Array = band["labor_assignments"]
	rows.append({
		"kind": HudConst.LABOR_KIND_EXTRACT,
		"workers": WORKED_BUTTON_CUTTERS,
		"target_x": WORKING_TILE_X, "target_y": WORKING_TILE_Y, "fauna_id": "",
		"material": "wood",
		"actual_yield": WORKED_BUTTON_TAKE,
		"sustainable_yield": WORKED_BUTTON_TAKE,
		"workers_needed": WORKED_BUTTON_CUTTERS,
	})
	return band

## Ray's own numbers, so the frame is the card he was reading: two foresters and the take beside them.
const WORKED_BUTTON_CUTTERS := 2
const WORKED_BUTTON_TAKE := 0.60

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
	}

## …and the same wood raised to its branch's top, which is the rung whose payoff is RENEWAL.
func _coppiced_wood() -> Dictionary:
	var working := _wood_working(WOOD_SAFE_TAKE)
	working["rung"] = HudDepositVocab.RUNG_KEY_COPPICE
	working["build_fraction"] = METER_NOTHING_RISING
	working["regrowth_rate"] = WOOD_REGROWTH * 2.0
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
	}

## …and the same body once a quarry stands on it, drawn down: the row that states the reach the ladder
## bought and the payoff row that says so.
func _quarried_stone() -> Dictionary:
	var working := _stone_working(STONE_TAKE)
	working["rung"] = HudDepositVocab.RUNG_KEY_QUARRY
	working["stock"] = QUARRY_STOCK
	working["reachable"] = QUARRY_REACHABLE
	return working

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
