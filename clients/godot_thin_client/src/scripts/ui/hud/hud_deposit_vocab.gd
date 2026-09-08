class_name HudDepositVocab

## WORKINGS vocabulary — the two deposit branches, forestry and extraction (arc #583,
## `docs/plan_extraction.md`, `.claude/rules/core_sim/extraction.md`). The rung names, the readers for
## every field on a `deposits` row, and the one composer behind the working's state.
##
## ⛔ **THE PLAYER-FACING NOUN IS "WORKING", NEVER "QUARRY".** `Quarry` in this client already means
## the HUNTED ANIMAL — it is one of the compose sheet's own field rows (`Band:` · `Kit` · `Quarry`) —
## so a second meaning on a second surface would put one word on two unrelated things. The sim's own
## word for a live deposit a band has opened is a **working**, and `quarrywork` survives only as the
## server's command token, which no player reads.
##
## ⛔ **A WORKING IS KEYED `(tile, material)`, AND THE PAIR IS INDIVISIBLE.** One tile can hold two —
## a wooded highland holds timber and rock, and working the timber is not working the rock — so every
## reader here that joins a row to anything joins on both halves. A surface keyed on the tile alone
## draws one of the two workings and silently loses the other, which is the defect the `material`
## field exists to prevent.
##
## ⛔ **WHICH READOUT A WORKING PUBLISHES IS DECIDED BY `regrowth_rate > 0`, NEVER BY `branch`.** A
## flint scatter and a quarry are both `extraction` and read differently — same skill, same ladder,
## and only one of them runs out. `renews()` below is that fork, it is the ONLY fork, and it lives
## inside `deposit_row_value` so the card and the roster cannot answer it two ways.
##
## ⛔ **NOTHING HERE RE-DERIVES A NUMBER THE SIM ALREADY ANSWERED.** The bill, its shortfall, the
## keeper count, the neglect countdown, the runway and the sustainable take are all published fields;
## the composers read them and nothing else. `HudRouteVocab`'s rule one branch over, and it is the
## same rule for the same reason.
##
## It reads `SourceForecast` / `DetailFormat` / `HudSelectionVocab` / `HudLoadoutVocab` / `HudStyle`
## **inside functions only, never in a `const`** — the vocab modules' shared contract, so this leaf
## adds no load cycle.

# ---- THE FIVE RUNGS --------------------------------------------------------------------------
#
# The wire's own `<branch>:<id>` spelling (`RungKey::wire_key`), the same grammar `plant:tended`,
# `animal:pen` and `route:dirt_road` use.
#
# ⛔ **THE RUNG STRING IS THE ANSWER — never threshold `build_fraction` to infer one.** That meter
# belongs to the rung being RAISED, which is a different rung: a reader that thresholded the float
# would call a working half way to a coppice a coppice already.

## **THE FORESTRY FLOOR** — gathering fallen wood. Free: nobody built it, so there is nothing to hold.
const RUNG_KEY_DEADFALL := "forestry:deadfall"

## Cutting standing timber, and the rung at which over-cutting a wood becomes possible at all.
const RUNG_KEY_FELLING := "forestry:felling"

## A managed cut-and-regrow wood — the branch's top, and the one rung that raises the ground's own
## rate of renewal rather than the rate of take.
const RUNG_KEY_COPPICE := "forestry:coppice"

## **THE EXTRACTION FLOOR** — picking loose stone off the surface. Free, for the deadfall's reason.
const RUNG_KEY_GATHERING := "extraction:gathering"

## An open face cut into the rock body beneath the scatter.
const RUNG_KEY_QUARRY := "extraction:quarry"

## The forestry branch's rungs in climb order — the order `next_rung_key` steps through.
const RUNG_ORDER_FORESTRY := [RUNG_KEY_DEADFALL, RUNG_KEY_FELLING, RUNG_KEY_COPPICE]

## …and the extraction branch's. **Two ladders, never one**: a working climbs the branch its material
## belongs to, and a shared order would offer a coppice above a quarry.
const RUNG_ORDER_EXTRACTION := [RUNG_KEY_GATHERING, RUNG_KEY_QUARRY]

## One rung's player-facing name.
##
## ⛔ **THERE IS NO RUNG CATALOG ON THE WIRE FOR EITHER DEPOSIT BRANCH**, unlike the route branch's
## `SubsistenceSection.routeRungs` — so this table is the client's own reading vocabulary, exactly as
## `HudRouteVocab.RUNG_LABELS` is for the tile card. A rung this table does not know renders as its
## raw wire key, which is the honest answer rather than a blank.
const RUNG_LABELS := {
	RUNG_KEY_DEADFALL: "Deadfall",
	RUNG_KEY_FELLING: "Felling",
	RUNG_KEY_COPPICE: "Coppice",
	RUNG_KEY_GATHERING: "Gathering",
	RUNG_KEY_QUARRY: "Quarry face",
}

# ---- THE WIRE'S OWN VALUES -------------------------------------------------------------------

## **THE RUNWAY'S TWO NEGATIVES, PASSED THROUGH VERBATIM** from `sim_schema::{DEPOSIT_RUNWAY_NOT_
## APPLICABLE, DEPOSIT_RUNWAY_NO_TAKE}` — two different sentences, and flattening them into one
## "no runway" is the defect the split exists to prevent.
##
## `-1` is NOT APPLICABLE: **this deposit RENEWS**, so it does not run out and the over-cut pair is
## its warning instead. **By construction it cannot reach the runway arm at all** — that arm is
## entered only where `regrowth_rate == 0` — so a `-1` seen there is a sim bug rather than a state,
## and `runway_clause` states nothing rather than inventing a reading for it.
const RUNWAY_NOT_APPLICABLE := -1

## `-2` is NO TAKE THIS TURN: nobody is cutting it, so there is no rate to carry forward. **It is not
## `-1`** — this working WILL run out, just not while it stands idle — and it renders as *not being
## worked*, never as `0 turns left`.
const RUNWAY_NO_TAKE := -2

## The ground's own renewal rate at which a working is FINITE. Rock's rate is exactly this, which is
## what makes `sustainable_take` read `0` on a quarry by arithmetic rather than by a branch check.
const REGROWTH_NEVER_RENEWS := 0.0

## `build_fraction`'s complete reading — **published exactly, never derived by subtraction**, so the
## test is a plain comparison and never a tolerance. It covers BOTH a rung just finished and a
## working at the top of its ladder with nothing left to raise: either way *nothing is rising*.
const METER_COMPLETE := 1.0

## …and its other end. A zero meter is a climb only where one has been ORDERED (`is_queued`);
## without an entry there is no climb and printing a percentage would invent one.
const METER_UNSTARTED := 0.0

## The scale a meter is stated at, shared with the percentage the card's build line prints.
const PERCENT_SCALE := 100.0

# ---- READERS — one per wire field, so a key is spelled once ------------------------------------

## The working's TILE half. `(-1, -1)` for a row missing either coordinate, which every consumer
## drops rather than drawing at the origin.
static func tile_of(deposit: Dictionary) -> Vector2i:
	return Vector2i(int(deposit.get("tile_x", -1)), int(deposit.get("tile_y", -1)))

## …and its MATERIAL half — `"wood"`, `"stone"`. The other half of the key, never optional.
static func material_of(deposit: Dictionary) -> String:
	return String(deposit.get("material", ""))

## WHICH LADDER works this deposit — `"forestry"` | `"extraction"`, `RungBranch`'s wire form.
##
## ⛔ **IT DOES NOT DECIDE THE READOUT.** `renews()` does. `branch` answers *which knowledge track*
## and nothing else.
static func branch_of(deposit: Dictionary) -> String:
	return String(deposit.get("branch", ""))

## What is standing here right now, in the material's own units — the only SAVED thing about a
## working.
static func stock_of(deposit: Dictionary) -> float:
	return float(deposit.get("stock", 0.0))

## What this GROUND holds when full, read live off the tile. **No rung may raise it** — it is the
## terrain's.
static func capacity_of(deposit: Dictionary) -> float:
	return float(deposit.get("capacity", 0.0))

## ⛔ **WHAT THE CURRENT RUNG CAN ACTUALLY GET AT — never simply `stock`.** A rung that cannot reach
## the whole seam leaves stock it cannot take, and climbing the ladder is precisely how you reach
## deeper; a readout that drew `stock` as *what you can have* promises the player rock the crew
## cannot cut. It is also the numerator of the runway.
static func reachable_of(deposit: Dictionary) -> float:
	return float(deposit.get("reachable", 0.0))

## The GROUND's own renewal rate, already scaled by what this working's rung bought.
static func regrowth_rate_of(deposit: Dictionary) -> float:
	return float(deposit.get("regrowth_rate", REGROWTH_NEVER_RENEWS))

## ⛔ **THE ONE FORK IN THIS FILE.** `true` → the OVER-CUT pair is this working's warning; `false` →
## the RUNWAY is. **Never `branch`**: forking on the branch string paints a renewing flint scatter
## with a runway that never moves, and leaves a quarry quoting a sustainable take of zero as though
## it were a bill met.
static func renews(deposit: Dictionary) -> bool:
	return regrowth_rate_of(deposit) > REGROWTH_NEVER_RENEWS

## The rung this working HOLDS.
static func rung_of(deposit: Dictionary) -> String:
	return String(deposit.get("rung", ""))

## The meter on the rung being RAISED, 0..1 — never the rung above's fullness and never this rung's.
static func build_fraction_of(deposit: Dictionary) -> float:
	return float(deposit.get("build_fraction", METER_UNSTARTED))

## **THE OVER-CUT PAIR, first half** — what a crew could take EVERY TURN AT THIS STOCK and leave the
## working where it stands. `0` on a finite working by arithmetic (rock's rate is zero, so the
## difference is exactly zero), which is why `renews()` and not this field is the fork.
static func sustainable_take_of(deposit: Dictionary) -> float:
	return float(deposit.get("sustainable_take", 0.0))

## …and its second half — what the working actually paid out last turn, summed over every band
## cutting it.
static func actual_take_of(deposit: Dictionary) -> float:
	return float(deposit.get("actual_take", 0.0))

## **THE RUNWAY** — how many turns this working lasts at the current take, `floor(reachable /
## actual_take)` sim-side. A FORWARD projection, never a trailing average, so it moves the turn the
## crew does. Two negatives, two sentences — see `RUNWAY_NOT_APPLICABLE` / `RUNWAY_NO_TAKE`.
static func turns_remaining_of(deposit: Dictionary) -> int:
	return int(deposit.get("turns_remaining", RUNWAY_NO_TAKE))

## **THE STANDING BILL — the patch / herd / road quad, verbatim**, drawn from the band's `quarrywork`
## pool. `0` on both free floors, which declare no upkeep at all: nobody built them, so there is
## nothing to hold, and that is the whole of what makes a floor free.
static func upkeep_demand_of(deposit: Dictionary) -> float:
	return float(deposit.get("upkeep_demand", 0.0))

static func upkeep_supplied_of(deposit: Dictionary) -> float:
	return float(deposit.get("upkeep_supplied", 0.0))

## ⛔ **THE SIM'S FIELD, NEVER `demand − supplied`.** All three read one stamped basis at the
## post-decay position; a bill struck on one side of the accrual against a payment on the other are
## two readings of two different workings.
static func upkeep_shortfall_of(deposit: Dictionary) -> float:
	return float(deposit.get("upkeep_shortfall", 0.0))

## The whole `quarrywork` keepers the bill wants — the readout that makes a standing cost legible
## (*"wants 2, you have 0"*). Published, so nothing here divides a demand by a rate.
static func upkeep_workers_needed_of(deposit: Dictionary) -> int:
	return int(deposit.get("upkeep_workers_needed", 0))

## ⛔ **READ THIS BEFORE THE COUNTDOWN.** `false` means there is NOTHING AT RISK here — a working on
## either free floor, which declares no upkeep and so has no meter to lose — and the countdown beside
## it reuses the "biting now" `0` rather than inventing a sentinel a client could mistake for a real
## count.
static func has_neglect_grace(deposit: Dictionary) -> bool:
	return bool(deposit.get("has_neglect_grace", false))

## THE COUNTDOWN, NOT THE COUNTER: `0` means it is sliding NOW, and a working whose bill is met reads
## its rung's full grace + 1 (*"walk away and you have this long"*).
static func neglect_grace_remaining_of(deposit: Dictionary) -> int:
	return int(deposit.get("neglect_grace_remaining", 0))

## **THE BUILD IN FRONT OF THIS WORKING — the CHAINED countdown**, and the SAME quantity with the
## SAME sentinels a patch, a herd and a road publish. There is deliberately no deposit dialect, so
## `DetailFormat.build_sentinel_value` renders a working through the identical fork.
static func build_turns_remaining_of(deposit: Dictionary) -> int:
	return int(deposit.get("build_turns_remaining", SourceForecast.BUILD_TURNS_NO_ESTIMATE))

## WHY THE POOL IS STUCK ON THIS WORKING — the shared free-form `BuildGate` vocabulary, never an
## enum, so one reader answers for every branch.
##
## ⛔ **`""` IS NOT *FINE*.** It is *nothing is being built here*, which is a different sentence from
## *nothing is wrong* — and it is every working on the map until a player orders a rung.
static func build_blocked_reason_of(deposit: Dictionary) -> String:
	return String(deposit.get("build_blocked_reason", ""))

## Is this working in some band's build queue right now? — the MEMBERSHIP flag, and the term
## `build_turns_remaining`'s `-5` is separated from `-1` by.
##
## ⛔ **IT CANNOT BE REPLACED BY A `build_kit_id != ""` TEST**: a resolved builders kit is never
## empty, the bare-handed kit being a roster entry like any other.
static func is_queued(deposit: Dictionary) -> bool:
	return bool(deposit.get("is_queued", false))

## **THE POSITION `DetailFormat.build_sentinel_value` FORKS `-1` ON, answered by the flag the deposit
## row actually publishes.** That fork asks exactly one question — *does any band still have this
## source queued* — and `is_queued` is that question's own field; a deposit row carries membership
## rather than a rank, so the rank is never read on this path and never invented here.
static func queue_position_of(deposit: Dictionary) -> int:
	return SourceForecast.BUILD_QUEUE_HEAD if is_queued(deposit) \
		else SourceForecast.NOT_IN_ANY_BUILD_QUEUE

# ---- THE KEEPING VERDICT ---------------------------------------------------------------------

## Does this working owe anything to hold at all? `false` on both free floors, where the whole keeping
## block renders as nothing rather than as a bill of zero.
static func owes_keeping(deposit: Dictionary) -> bool:
	return upkeep_demand_of(deposit) >= SourceForecast.UPKEEP_WORK_MIN

## Is the `quarrywork` pool failing to cover this working? — **the sim's own shortfall and no
## subtraction here.**
static func is_short(deposit: Dictionary) -> bool:
	return upkeep_shortfall_of(deposit) >= SourceForecast.UPKEEP_WORK_MIN

## Is the working actually LOSING its rung? — short **and** carrying a meter to lose. Reads the bool
## before the number, which is the whole of `has_neglect_grace`'s rule.
static func is_at_risk(deposit: Dictionary) -> bool:
	return is_short(deposit) and has_neglect_grace(deposit)

# ---- COMPOSERS -------------------------------------------------------------------------------

## One rung's player-facing name; the raw wire key for a rung this client has never heard of.
static func rung_label(rung: String) -> String:
	return String(RUNG_LABELS.get(rung, rung))

## The rung DIRECTLY above the one held, as a key — `""` at the top of a branch and for a rung
## neither order knows. The two branches are walked separately, so a working never offers a rung from
## the other ladder.
static func next_rung_key(rung: String) -> String:
	for order in [RUNG_ORDER_FORESTRY, RUNG_ORDER_EXTRACTION]:
		var at: int = (order as Array).find(rung)
		if at >= 0 and at + 1 < (order as Array).size():
			return String((order as Array)[at + 1])
	return ""

## …and as a word. Callers state a meter without a destination rather than naming a rung they cannot
## vouch for.
static func next_rung_label(rung: String) -> String:
	var next_key := next_rung_key(rung)
	return "" if next_key == "" else rung_label(next_key)

## The material as a person reads it — `Wood`, `Stone`. **The wire carries no display name for a
## material**, and `HudLoadoutVocab.material_label` is this client's one idiom for that, so the
## working's head and the crafting bench's shelf cannot capitalize one material two ways.
static func material_label(deposit: Dictionary) -> String:
	return HudLoadoutVocab.material_label(material_of(deposit))

## The meter as a whole percent. **`floor` rather than `round`**, because a hair short of a rung must
## never read as the rung — `HudRouteVocab.road_percent_of`'s rule, and the same reason.
static func percent_of(meter: float) -> int:
	return int(floor(meter * PERCENT_SCALE))

## ⛔ **THE APPROACH TO THE NEXT RUNG, NEVER THE STATE OF THIS ONE** — `42% to coppice`, not
## `Coppice 42%`. `""` where nothing is rising: `METER_COMPLETE` is the complete reading and covers
## both a rung just finished and the top of a branch.
static func progress_clause(deposit: Dictionary) -> String:
	var meter := build_fraction_of(deposit)
	if meter >= METER_COMPLETE:
		return ""
	# **A ZERO METER IS A CLIMB ONLY WHERE ONE HAS BEEN ORDERED.** A working queued behind another job
	# banks nothing for dozens of turns; with an entry declared the `0%` is a true reading of a real
	# climb, and without one there is no climb to state.
	if meter <= METER_UNSTARTED and not is_queued(deposit):
		return ""
	var percent := percent_of(meter)
	var destination := next_rung_label(rung_of(deposit))
	if destination == "":
		return DEPOSIT_PROGRESS_UNNAMED_FORMAT % percent
	return DEPOSIT_PROGRESS_FORMAT % [percent, destination.to_lower()]

## `42% to coppice` — the climb, named by where it is going.
const DEPOSIT_PROGRESS_FORMAT := "%d%% to %s"

## …and the same climb where the destination cannot be named, which is a rung neither order knows.
const DEPOSIT_PROGRESS_UNNAMED_FORMAT := "%d%% to the next rung"

## The clause separator — the tile card's own middot, so a working's line reads like the rows above it.
const DEPOSIT_CLAUSE_SEPARATOR := " · "

## `⚠ overdrawing` / `⚠ going back` — the glyph and the state word, the road block's own
## `ROAD_HAZARD_CLAUSE_FORMAT` shape. Every hazard clause on this branch leads with the mark, which is
## what the tint registry keys the amber off rather than off a list of known words.
const DEPOSIT_HAZARD_CLAUSE_FORMAT := "%s %s"

## ⛔ **THE WORD IS THE TWO FOOD WEBS' OWN WORD** (`SourceForecast.YIELD_OVERDRAW_WORD`), and it is
## read from there rather than spelled here: taking more than a source renews is ONE idea, and one
## idea gets one word wherever it is stated. Resolved in a function because a cross-class `const` is a
## GDScript parse error.
static func over_cut_word() -> String:
	return SourceForecast.YIELD_OVERDRAW_WORD

## …and the §7 clause for a FINITE working: the runway.
const DEPOSIT_RUNWAY_FORMAT := "%d turns left"

## The singular, because `1 turns left` is the tell that a list was built by a format string.
const DEPOSIT_RUNWAY_ONE := "1 turn left"

## ⛔ **`RUNWAY_NO_TAKE` RENDERS AS THIS, NEVER AS `0 turns left`.** Nobody is cutting it, so there is
## no rate to carry forward — the working WILL run out, just not while it stands idle, and a `0` there
## would announce an exhaustion that has not happened.
const DEPOSIT_RUNWAY_IDLE := "not being worked"

## The hazard word for a working whose keeping is short — **its own consequence rather than a shared
## adjective**, exactly as the plant web's *slipping* and the animal web's *drifting* are theirs. An
## unheld working slides back down its ladder, so what is happening to it is that it is **going back**.
## Lower-case: it lands mid-value, after the state clause.
const DEPOSIT_UNDER_KEPT_WORD := "going back"

## `Felling · 42% to coppice · 8 turns left · ⚠ going back` — **THE ONE COMPOSER, and both surfaces
## use it.**
##
## ⛔ **THE CARD'S STATE LINE AND THE ROSTER'S VALUE CELL ARE THIS FUNCTION**, exactly as
## `HudRouteVocab.road_row_value` is shared one branch over. One composer, one answer — so the card
## and the roster cannot disagree about a working's state, and the §7 fork lives here ONCE rather
## than once per surface.
##
## ⛔ **THE RUNG IS THE VALUE AND EVERYTHING ELSE IS A QUALIFIER.** A felling working 42% of the way
## to a coppice is a COMPLETE felling working, not a coppice 42% built.
static func deposit_row_value(deposit: Dictionary) -> String:
	var clauses: Array[String] = [rung_label(rung_of(deposit))]
	var progress := progress_clause(deposit)
	if progress != "":
		clauses.append(progress)
	var supply := supply_clause(deposit)
	if supply != "":
		clauses.append(supply)
	if is_at_risk(deposit):
		clauses.append(DEPOSIT_HAZARD_CLAUSE_FORMAT % [
			HudSelectionVocab.RUNG_HAZARD_GLYPH, DEPOSIT_UNDER_KEPT_WORD])
	return DEPOSIT_CLAUSE_SEPARATOR.join(clauses)

## ⛔ **§7's FORK, AND IT IS THE ONLY PLACE IT IS TAKEN.** A renewing working warns that you are
## over-cutting it; a finite one warns that it runs out. `""` on a renewing working cut inside its own
## renewal — nothing is wrong, and a clause saying so would be a row spent on the absence of news.
static func supply_clause(deposit: Dictionary) -> String:
	if renews(deposit):
		if actual_take_of(deposit) > sustainable_take_of(deposit):
			return DEPOSIT_HAZARD_CLAUSE_FORMAT % [
				HudSelectionVocab.RUNG_HAZARD_GLYPH, over_cut_word()]
		return ""
	return runway_clause(deposit)

## The finite working's runway as a sentence. **`RUNWAY_NOT_APPLICABLE` cannot reach here by
## construction** — it means *this deposit renews*, which is the arm above — so it states nothing
## rather than rendering a reading for a value that would be a sim bug.
static func runway_clause(deposit: Dictionary) -> String:
	var turns := turns_remaining_of(deposit)
	if turns == RUNWAY_NO_TAKE:
		return DEPOSIT_RUNWAY_IDLE
	if turns < 0:
		return ""
	if turns == 1:
		return DEPOSIT_RUNWAY_ONE
	return DEPOSIT_RUNWAY_FORMAT % turns

## The row's INK, forked on the hazard mark the composer above puts there rather than on a second
## reading of the working — `HudRouteVocab.road_value_hex`'s shape, so a working at risk reads in the
## same amber a slipping patch does.
static func deposit_value_color(deposit: Dictionary) -> Color:
	return HudStyle.WARN if deposit_row_value(deposit).contains(
		HudSelectionVocab.RUNG_HAZARD_GLYPH) else HudStyle.INK_DIM

# ---- THE CARD'S OWN ROWS ----------------------------------------------------------------------
#
# The working card is a READOUT with a crew stepper, and deliberately NOT a rung ladder: a deposit has
# no stance, no escapement floor, no policy ceiling, no take-species and no projection chart, so the
# two food webs' compose sheet is the wrong shape for it and giving it one would be inventing parity
# the simulation does not have.

## The card's own title — the noun, not the command token.
const CARD_TITLE := "WORKINGS"

## `Wood` — one BLOCK per working, headed by its material, because the registry key is
## `(tile, material)` and a wooded highland really does carry two.
const CARD_BLOCK_HEAD_FORMAT := "%s"

## ⛔ **THREE NUMBERS, AND `reachable` IS NOT A RESTATEMENT OF EITHER.** `stock` is what is standing,
## `capacity` is what this ground holds when full, and `reachable` is what the CURRENT rung can get
## at — so a low rung on a full seam shows a large capacity and a small reachable, and that gap is the
## whole argument for climbing the ladder. Collapsing any two of them hides the argument.
const CARD_STOCK_FORMAT := "%s of %s · %s within reach"

## The stock row's key, kept inside `DetailFormat.DETAIL_KEY_MAX_LENGTH` so it aligns with the card's
## other rows.
const CARD_STOCK_ROW := "Stock"

## How many decimals a stock reads at. Wood and stone are counted in whole units at the scale the
## deposits table authors them (a mixed woodland is 600, a karst highland 3000), so a trailing `.0`
## would dress a config integer up as a measured figure — `DetailFormat.format_trimmed`'s own reason.
const CARD_STOCK_DECIMALS := 1

## The take pair, for the hover on a renewing working's state line: what it is paying out now against
## what it could pay out for ever. **Both figures published**; the client subtracts nothing.
const CARD_TAKE_TIP_FORMAT := "Taking %s a turn · it renews %s a turn"

## …and the finite working's hover, which has no sustainable take to quote (rock's rate is zero, so
## the sustainable figure is `0` by arithmetic) and quotes the seam instead.
const CARD_RUNWAY_TIP_FORMAT := "%s left within this rung's reach, at the take it is running at now."

## The crew row's key and its hint. **The one place a working differs from a road**: a road is not
## worked, a working is, and this stepper is the take crew — never the keepers, which are the
## band-wide pool one panel over.
const CARD_CREW_ROW := "Cutters"

const CARD_CREW_HINT := "Hands taking material out of this working. The hands that HOLD it are the " \
	+ "band's Workings pool, on the Work tab."

## The build line's key — the rung being raised, its countdown and its percentage.
const CARD_BUILD_ROW := "Building"

## The keeping line's key. **It shares its word with the road's bill and the band's material bill** —
## one word for one concept.
const CARD_UPKEEP_ROW := "Upkeep"

## The bill's face: what the working owes a turn and how many keepers that is.
const CARD_UPKEEP_FORMAT := "%s work a turn · %d keeper%s"

const CARD_UPKEEP_PLURAL_SUFFIX := "s"

## …and the shortfall, appended where the pool is not covering it.
const CARD_UPKEEP_SHORT_FORMAT := "%s (short %s)"

## The countdown row — *when this working goes back*, and it renders only while the working is
## actually short. **The countdown, not the counter**: `0` is *it is sliding now*.
const CARD_REVERTING_ROW := "Going back"

const CARD_REVERTING_NOW := "%s now"
const CARD_REVERTING_ONE := "%s next turn"
const CARD_REVERTING_FORMAT := "%s in %d turns"

## `0` on the countdown is *it is happening this turn*, not *there is no countdown* — the bool above
## is what answers the second question.
const REVERTING_IMMINENT := 0

## The bill, as the card's `Upkeep` row says it. `""` on a free floor, which then renders NO row —
## a sentence saying *free* is a row spent on the absence of a bill.
static func upkeep_value(deposit: Dictionary) -> String:
	if not owes_keeping(deposit):
		return ""
	var wants := upkeep_workers_needed_of(deposit)
	var face: String = CARD_UPKEEP_FORMAT % [
		DetailFormat.format_work_units(upkeep_demand_of(deposit)), wants,
		"" if wants == 1 else CARD_UPKEEP_PLURAL_SUFFIX]
	if not is_short(deposit):
		return face
	return CARD_UPKEEP_SHORT_FORMAT % [face,
		DetailFormat.format_work_units(upkeep_shortfall_of(deposit))]

## The countdown, or `""` where nothing is at risk — which is every working on either free floor and
## every working whose bill is met.
static func reverting_value(deposit: Dictionary) -> String:
	if not is_at_risk(deposit):
		return ""
	var left := neglect_grace_remaining_of(deposit)
	if left <= REVERTING_IMMINENT:
		return CARD_REVERTING_NOW % HudSelectionVocab.RUNG_HAZARD_GLYPH
	if left == 1:
		return CARD_REVERTING_ONE % HudSelectionVocab.RUNG_HAZARD_GLYPH
	return CARD_REVERTING_FORMAT % [HudSelectionVocab.RUNG_HAZARD_GLYPH, left]

## The stock row's three figures, in the material's own units.
static func stock_value(deposit: Dictionary) -> String:
	return CARD_STOCK_FORMAT % [
		DetailFormat.format_trimmed(stock_of(deposit), CARD_STOCK_DECIMALS),
		DetailFormat.format_trimmed(capacity_of(deposit), CARD_STOCK_DECIMALS),
		DetailFormat.format_trimmed(reachable_of(deposit), CARD_STOCK_DECIMALS)]

## The state line's hover — the §7 figures the one-line clause above it cannot carry. Forked on
## `renews()`, the same single fork, so the words and the numbers cannot describe two workings.
static func supply_tooltip(deposit: Dictionary) -> String:
	if renews(deposit):
		return CARD_TAKE_TIP_FORMAT % [
			DetailFormat.format_trimmed(actual_take_of(deposit), CARD_STOCK_DECIMALS),
			DetailFormat.format_trimmed(sustainable_take_of(deposit), CARD_STOCK_DECIMALS)]
	return CARD_RUNWAY_TIP_FORMAT % DetailFormat.format_trimmed(
		reachable_of(deposit), CARD_STOCK_DECIMALS)

## The build line's value — **the shared sentinel fork, not a deposit dialect.** A working publishes
## the identical five sentinels a patch, a herd and a road publish, so it renders through
## `DetailFormat.build_countdown_value` with no branch of its own.
##
## `""` where no rung is in flight: `build_turns_remaining` answers the no-estimate sentinel and the
## meter is empty and nothing is queued, which is every working until a player orders a rung.
static func build_value(deposit: Dictionary, crew: int) -> String:
	var turns := build_turns_remaining_of(deposit)
	var meter := build_fraction_of(deposit)
	if turns == SourceForecast.BUILD_TURNS_NO_ESTIMATE and not is_queued(deposit) \
			and (meter <= METER_UNSTARTED or meter >= METER_COMPLETE):
		return ""
	return DetailFormat.build_countdown_value(turns, crew, percent_of(meter),
		queue_position_of(deposit))
