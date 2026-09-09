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

# ⛔ **RETIRED — `RUNG_ORDER_FORESTRY`, `RUNG_ORDER_EXTRACTION` AND `RUNG_LABELS`** (issue #650). They
# were the client's own reading vocabulary for branches the wire published no catalog for, and the
# wire publishes one now: `SubsistenceSection.depositRungs`, one row per rung of BOTH branches,
# carrying the climb order, the sim's own `display_name`, the price, the payoff and every gate. Every
# surface reads it through the `catalog_*` / `ladder_*` accessors below, so **a rung added to
# `intensification_ladder.json` appears with no client edit** — the property the route branch's own
# catalog bought, arriving here for the same reason.
#
# The five `RUNG_KEY_*` consts above SURVIVE: they are the wire's own join keys, spelled once for the
# harnesses that stage a catalog and a working standing on one of its rungs.

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

## **NOTHING BANKED ON THE LADDER** — `intensification::RUNG_UNSTARTED`, the position
## `DepositSource::opening` starts a deposit at. Work banked into RAISING a rung moves it and cutting
## never does, so it is the one field that separates a seam somebody has invested in from one nobody
## has.
const LADDER_UNSTARTED := 0.0

## **A TAKE OF NOTHING** — `extraction::NO_TAKE_THIS_TURN`, and it is what `last_take` is cleared to
## every turn. ⛔ On its own it says only *nothing came out last turn*, which is true of a working in
## a dead season and of a stalled crew as well as of ground nobody has touched: see `is_unopened`.
const TAKE_NONE := 0.0

# ---- READERS — one per wire field, so a key is spelled once ------------------------------------

## The working's TILE half. `(-1, -1)` for a row missing either coordinate, which every consumer
## drops rather than drawing at the origin.
static func tile_of(deposit: Dictionary) -> Vector2i:
	return Vector2i(int(deposit.get("tile_x", -1)), int(deposit.get("tile_y", -1)))

## …and its MATERIAL half — `"wood"`, `"stone"`. The other half of the key, never optional.
## The other half of a working's key when the wire states none — a row this client cannot address, which
## every caller drops rather than addressing by tile alone. Named because the DECLARATION tests it: a
## verb carrying no material would raise the wrong ladder on a hex holding two workings.
const MATERIAL_NONE := ""

static func material_of(deposit: Dictionary) -> String:
	return String(deposit.get("material", MATERIAL_NONE))

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

## **HOW FAR UP ITS BRANCH THIS DEPOSIT HAS BEEN RAISED**, in cumulative work units — the absolute
## position the rung and the meter above are both read out of. It is banked by BUILDING, never by
## cutting, so a seam worked for a hundred turns at its free floor still reads `LADDER_UNSTARTED`.
static func ladder_position_of(deposit: Dictionary) -> float:
	return float(deposit.get("ladder_position", LADDER_UNSTARTED))

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

# ---- HAS ANYBODY OPENED THIS GROUND? -----------------------------------------------------------

## **TRUE WHERE THE ROW IS THE GROUND'S OPENING STATE AND NOTHING MORE** — the deposit a tile HOLDS,
## with no working standing on it (issue #650).
##
## ⛔ **A ROW NO LONGER MEANS A LIVE WORKING.** The `deposits` section publishes one row for every
## DISCOVERED tile that holds a deposit and merges the registry's working in where a band has opened
## one, so most rows on the map describe untouched ground. The card would otherwise render every one
## of them as a working whose bill is met, whose build is done and whose seam nobody is cutting —
## four quiet readings that together say *this is being kept*, which is false.
##
## ⛔ **THE PREDICATE IS NOT `actual_take == 0`.** A working with a real crew honestly takes nothing
## in a dead season, behind a stalled build, or with its stock at the rung's floor — and rendering
## THAT as untouched ground hides a working the player is paying for. What this tests instead is the
## whole fingerprint of `DepositSource::opening`, every term of which a live working moves the moment
## anybody does anything to it:
##
## - **it owes no keeping** — every rung above a free floor bills `quarrywork` every turn;
## - **nothing is banked on its ladder** — raising a rung moves `ladder_position` and cutting cannot;
## - **its seam is FULL** — a crew that has taken anything leaves `stock` below `capacity` until the
##   ground has grown every unit of it back;
## - **nobody has ordered a rung on it** (`is_queued`), which is the stalled build's own field;
## - **and nothing came out of it last turn.**
##
## **What it cannot separate is a working standing at its branch's FREE FLOOR, on a full seam, with
## nothing queued and nothing taken** — which is field-for-field the opening state on the wire, costs
## the player nothing, cuts nothing and builds nothing. The wire carries no *has a band opened this*
## flag to ask instead; adding one is sim-side work.
static func is_unopened(deposit: Dictionary) -> bool:
	return not owes_keeping(deposit) \
		and ladder_position_of(deposit) <= LADDER_UNSTARTED \
		and stock_of(deposit) >= capacity_of(deposit) \
		and not is_queued(deposit) \
		and actual_take_of(deposit) <= TAKE_NONE

# ---- COMPOSERS -------------------------------------------------------------------------------

# ⛔ **RETIRED — `rung_label`, `next_rung_key` and `next_rung_label`.** All three read the client-side
# tables above; the catalog answers all three now (`ladder_rung_name`, `ladder_next_entry`), and it
# answers them for a rung this client has never heard of as well.

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
static func progress_clause(deposit: Dictionary, ladder: Array[Dictionary] = []) -> String:
	var meter := build_fraction_of(deposit)
	if meter >= METER_COMPLETE:
		return ""
	# **A ZERO METER IS A CLIMB ONLY WHERE ONE HAS BEEN ORDERED.** A working queued behind another job
	# banks nothing for dozens of turns; with an entry declared the `0%` is a true reading of a real
	# climb, and without one there is no climb to state.
	if meter <= METER_UNSTARTED and not is_queued(deposit):
		return ""
	var percent := percent_of(meter)
	var destination := catalog_display_name(ladder_next_entry(ladder, deposit))
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

## ⛔ **THE WORD FOR GROUND NOBODY HAS OPENED, AND IT IS NOT A WARNING.** `not being worked` is the
## IDLE WORKING's sentence — a seam somebody opened and walked away from, which will run out and
## which the card keeps a runway for — and this is the state before that one: there is no working
## here yet. Lower-case, and it lands after the rung the way every other qualifier does.
const DEPOSIT_UNOPENED_WORD := "unopened"

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
static func deposit_row_value(deposit: Dictionary, ladder: Array[Dictionary] = []) -> String:
	var clauses: Array[String] = [ladder_rung_name(ladder, rung_of(deposit))]
	# ⛔ **UNTOUCHED GROUND STATES WHAT IT IS AND STOPS.** Every clause below this line describes
	# something being DONE to a working — a rung rising, a seam being cut faster than it grows, a
	# runway burning down, a bill going unpaid — and on ground nobody has opened each of them would
	# be a reading of an event that has not happened. The RUNG stays: the free floor is what a crew
	# put here today would work at, which is the honest half of the line.
	if is_unopened(deposit):
		clauses.append(DEPOSIT_UNOPENED_WORD)
		return DEPOSIT_CLAUSE_SEPARATOR.join(clauses)
	var progress := progress_clause(deposit, ladder)
	if progress != "":
		clauses.append(progress)
	var supply := supply_clause(deposit)
	if supply != "":
		clauses.append(supply)
	var hazard := hazard_clause(deposit)
	if hazard != "":
		clauses.append(hazard)
	return DEPOSIT_CLAUSE_SEPARATOR.join(clauses)

## **THE UNDER-KEPT CLAUSE, AND IT IS ONE SPELLING FOR BOTH SURFACES.** The roster's value cell states
## it after the runway; the tile card's material row states it after the rung. Composing it here is
## what stops the two rows wearing two different words for one working sliding back down its ladder.
## `""` where nothing is at risk, which is both free floors and every working whose bill is met.
static func hazard_clause(deposit: Dictionary) -> String:
	if not is_at_risk(deposit):
		return ""
	return DEPOSIT_HAZARD_CLAUSE_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH, DEPOSIT_UNDER_KEPT_WORD]

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
static func deposit_value_color(deposit: Dictionary, ladder: Array[Dictionary] = []) -> Color:
	return HudStyle.WARN if deposit_row_value(deposit, ladder).contains(
		HudSelectionVocab.RUNG_HAZARD_GLYPH) else HudStyle.INK_DIM

# ---- THE FIGURES, AND WHERE THEY GO NOW --------------------------------------------------------
#
# ⛔ **RETIRED — THE `Workings ▸` POPUP AND EVERY CONST THAT ONLY IT READ** (issue #650): `CARD_TITLE`,
# `CARD_BLOCK_HEAD_FORMAT`, `CARD_STOCK_ROW` / `CARD_STOCK_FORMAT` / `stock_value`, `CARD_CREW_ROW`,
# `CARD_BUILD_ROW`, `CARD_UPKEEP_ROW` and `CARD_REVERTING_ROW`. That card was a FOURTH UX pattern for
# a branch whose three surfaces already ship, and it has been replaced by them: the tile card's
# per-material ROW, the `Assign foresters ▸` / `Assign diggers ▸` COMPOSE SHEETS, and the shared
# `RungLadder` TRACK on the Work board.
#
# ⛔ **THE THREE-NUMBER STOCK ROW WENT WITH IT, AND ITS ARGUMENT DID NOT.** `stock`, `capacity` and
# `reachable` on one line were the case for climbing the ladder, and on a `label · value · qualifier`
# card there is no room for three figures. The argument moved to the two surfaces that can carry it:
# the tile card's PAYOFF row (*reaches 85% of the seam*, off the catalog rather than off the working)
# and the compose sheet's VERDICT (*Gathering reaches 330 of 2,200. A quarry would reach 1,870.*).
# **`reachable` is still never drawn as *what you can have***; what changed is which surface says so.
#
# The composers that survive are the ones with a reader on the new surfaces: `upkeep_value` and
# `reverting_value` state the FIGURES on a hover (`deposit_card_tooltip` / `deposit_roster_tooltip`),
# `build_value` is the ladder's face for the row being built, and `supply_tooltip` is unchanged.

## How many decimals a stock reads at. Wood and stone are counted in whole units at the scale the
## deposits table authors them (a mixed woodland is 600, a karst highland 3000), so a trailing `.0`
## would dress a config integer up as a measured figure — `DetailFormat.format_trimmed`'s own reason.
const CARD_STOCK_DECIMALS := 1

## The take pair, for the hover on a renewing working's state line: what it is paying out now against
## what it could pay out for ever. **Both figures published**; the client subtracts nothing.
const CARD_TAKE_TIP_FORMAT := "Taking %s a turn · it renews %s a turn"

## …and the hover on ground nobody has opened, which is the state a player meets FIRST on almost
## every deposit on the map. It names the one thing that changes it — the crew stepper on this same
## block — because the stock row above has already said what is here and how much of it the free
## floor can reach.
const CARD_UNOPENED_TIP := "Nobody is working this ground. Put cutters on it and it opens " \
	+ "at this rung."

## …and the finite working's hover, which has no sustainable take to quote (rock's rate is zero, so
## the sustainable figure is `0` by arithmetic) and quotes the seam instead.
const CARD_RUNWAY_TIP_FORMAT := "%s left within this rung's reach, at the take it is running at now."

## **THE CREW SECTION'S HOVER, ON THE COMPOSE SHEET** — the one place a working differs from a road:
## a road is not worked, a working is, and this stepper is the TAKE crew. It says so because a player
## who staffed it expecting the bill to be met would watch the working go back anyway; the hands that
## HOLD a working are the band-wide `Workings` pool, whose only control is the roster head's stepper.
const CARD_CREW_HINT := "Hands taking material out of this ground. The hands that HOLD it are the " \
	+ "band's Groundwork pool, on the Work tab."

## The bill's face: what the working owes a turn and how many keepers that is.
const CARD_UPKEEP_FORMAT := "%s work a turn · %d keeper%s"

const CARD_UPKEEP_PLURAL_SUFFIX := "s"

## …and the shortfall, appended where the pool is not covering it.
const CARD_UPKEEP_SHORT_FORMAT := "%s (short %s)"

## The countdown's three faces — *when this working goes back*, composed only while the working is
## actually short. **The countdown, not the counter**: `0` is *it is sliding now*. It has no ROW of its
## own on any surface: it rides the WORK BOARD roster's hover and nowhere else.
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

## The state line's hover — the §7 figures the one-line clause above it cannot carry. Forked on
## `renews()`, the same single fork, so the words and the numbers cannot describe two workings.
##
## **Untouched ground takes neither arm**: the renewing hover would quote a take of nothing, and the
## finite one a runway *"at the take it is running at now"*, which is no take at all. What it says
## instead is what the crew stepper below it would do.
static func supply_tooltip(deposit: Dictionary) -> String:
	if is_unopened(deposit):
		return CARD_UNOPENED_TIP
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

# ==================================================================================================
#  THE RUNG CATALOG — `SubsistenceSection.depositRungs`, one row per rung of BOTH branches
# ==================================================================================================
#
# ⛔ **EVERY LABEL, PRICE, PAYOFF AND GATE ON THIS BRANCH COMES OFF THE WIRE** (issue #650), exactly as
# the route branch's do. A rung added to `intensification_ladder.json` appears on the tile card, in the
# compose sheet's pointer line and as a ladder row **with no client edit at all** — which is the whole
# reason the client-side `RUNG_LABELS` table above was retired rather than extended.
#
# ⛔ **ONE ARRAY CARRIES TWO LADDERS.** `order` is a climb order WITHIN a branch, so every walk over
# the catalog either filters on `branch` first or is a bug: a shared order would offer a coppice above
# a quarry. `branch_ladder` is the filter, and nothing here walks the raw array.

## The row's join key with a working's own `rung`, spelled `"<branch>:<id>"`.
const RUNG_CATALOG_KEY := "rung_key"

## Which ladder the row is on — `"forestry"` | `"extraction"`, the same vocabulary `branch_of` reads
## off a working.
const RUNG_CATALOG_BRANCH := "branch"

const RUNG_CATALOG_ORDER := "order"
const RUNG_CATALOG_DISPLAY_NAME := "display_name"

## The TILE COMMAND that raises this rung. `""` on both free floors, which nobody declares.
const RUNG_CATALOG_VERB := "verb"

const RUNG_CATALOG_UNLOCK_KNOWLEDGE := "unlock_knowledge"
const RUNG_CATALOG_REQUIRES_RUNG := "requires_rung"

## ⛔ **WHAT STANDING ON THIS RUNG TEACHES — the craft gate's REMEDY.** `unlock_knowledge` says what a
## rung WAITS ON; this says what a rung EARNS, and they are different rungs. See `ladder_rung_teaching`
## for why the pairing may not be inferred from `requires_rung`.
const RUNG_CATALOG_EARNS_KNOWLEDGE := "earns_knowledge"

const RUNG_CATALOG_WORK_COST := "work_cost"
const RUNG_CATALOG_UPKEEP := "upkeep_work_per_turn"

## The rung's declared build PILE and the noun for it — 8 wood on `extraction:quarry`, nothing
## anywhere else. **Read as a pair or not at all** (`catalog_material_pile`): an amount with no word
## cannot be rendered into a sentence and a word with no amount says nothing.
const RUNG_CATALOG_MATERIAL_COST := "build_material_cost"
const RUNG_CATALOG_MATERIAL_ID := "build_material_id"

## ⛔ **THE SIM'S OWN BARE WORKER OUTPUT, IDENTICAL ON EVERY ROW.** It rides the catalog because the
## catalog is exactly the set of numbers that are the same for every working in the world, and it is
## READ rather than transcribed: the sim writes worker output as a sum of terms, so a client copy goes
## stale in silence the day a second term lands.
const RUNG_CATALOG_BUILD_PER_WORKER_TURN := "build_work_per_worker_turn"

## What ONE cutter takes in a turn at this rung, before the reachable stock caps it. **Both free
## floors carry a real bare-handed rate** — the whole material economy bootstraps through them.
const RUNG_CATALOG_YIELD_PER_WORKER_TURN := "yield_per_worker_turn"

## ⛔ **HOW MUCH OF THE SEAM THIS RUNG REACHES, 0..1 — AND IT IS NOT `reachable / capacity`.** That
## ratio clamps to the STOCK, so it falls as the rock is worked while the rung's reach never moves.
const RUNG_CATALOG_RECOVERY := "recovery_fraction"

## What this rung multiplies the GROUND's own `regrowth_rate` by. No rung raises capacity and there is
## no field for one, so *stone's rate is zero* survives as arithmetic rather than as a rule.
const RUNG_CATALOG_REGROWTH_MULTIPLIER := "regrowth_multiplier"

## ⛔ **WHAT THE GROUND MUST HOLD FOR THIS RUNG TO STAND ON IT** — the one placement rule on either
## branch, and the whole of *you cannot quarry just anywhere*. The SITE gate reads it against the
## working's own `capacity`.
const RUNG_CATALOG_MIN_CAPACITY := "min_deposit_capacity"

## ⛔ **THE WIRE'S OWN SPELLING OF *there is none*, A NAMED EMPTY STRING RATHER THAN A SENTINEL.**
## `verb` is `""` on a rung nobody declares, `unlock_knowledge` on one nothing gates, `requires_rung`
## at a branch's floor and `earns_knowledge` on a rung that teaches nothing. Four real, distinct facts.
const RUNG_CATALOG_NONE := ""

## The order a rung with no published position falls to — **below either floor**, so a row this client
## cannot place sorts beneath the ladder rather than silently ahead of the rung it needs.
const RUNG_CATALOG_NO_ORDER := -1

## A rung nobody pays for — the two free floors. Named because it is the TEST a verbless row's face is
## gated on, not a rounding tolerance: `0 work` would read as a defect on a row with no price to state.
const RUNG_CATALOG_NO_WORK_COST := 0.0

## The rate a catalog this client has not been sent reads. **A measured nothing, not a sentinel**: it
## flows into the readers as *there is no rate*, which each answers by stating nothing.
const RUNG_CATALOG_NO_BUILD_RATE := 0.0

## …and the take rate's own version of that, for the identical reason.
const RUNG_CATALOG_NO_YIELD := 0.0

## A rung that asks nothing of the ground — every shipped rung but `extraction:quarry`. The SITE gate
## is skipped outright at this value rather than compared against a capacity.
const RUNG_CATALOG_NO_SITE_REQUIREMENT := 0.0

## A rung that eats no material, which is every rung of either branch but `extraction:quarry`.
const RUNG_CATALOG_MATERIAL_NONE := 0.0

## …and the noun for one. `""` is *this rung eats nothing*, never *nothing is known*.
const RUNG_CATALOG_MATERIAL_NO_ID := ""

## The multiplier at which a rung leaves the ground's renewal exactly as it found it — every
## extraction rung, and both floors. Above it is the coppice's payoff.
const RUNG_CATALOG_REGROWTH_UNCHANGED := 1.0

# ---- CATALOG READERS ---------------------------------------------------------------------------

static func catalog_rung_key(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_KEY, "")).strip_edges()

static func catalog_branch(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_BRANCH, "")).strip_edges()

static func catalog_order(entry: Dictionary) -> int:
	return int(entry.get(RUNG_CATALOG_ORDER, RUNG_CATALOG_NO_ORDER))

## The sim's own word for this rung. **The raw wire key where the catalog carries none**, which is the
## honest answer rather than a blank row.
static func catalog_display_name(entry: Dictionary) -> String:
	var name := String(entry.get(RUNG_CATALOG_DISPLAY_NAME, "")).strip_edges()
	return name if name != "" else catalog_rung_key(entry)

## ⛔ **`""` MEANS NOBODY DECLARES THIS RUNG** — the ground already offers it. A state, not a missing
## field, and it is the two commonest rungs on either branch.
static func catalog_verb(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_VERB, RUNG_CATALOG_NONE)).strip_edges()

static func catalog_unlock_knowledge(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_UNLOCK_KNOWLEDGE, RUNG_CATALOG_NONE)).strip_edges()

static func catalog_requires_rung(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_REQUIRES_RUNG, RUNG_CATALOG_NONE)).strip_edges()

static func catalog_earns_knowledge(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_EARNS_KNOWLEDGE, RUNG_CATALOG_NONE)).strip_edges()

static func catalog_work_cost(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_WORK_COST, RUNG_CATALOG_NO_WORK_COST))

static func catalog_upkeep(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_UPKEEP, SourceForecast.NO_UPKEEP_DEMAND))

static func catalog_material_cost(entry: Dictionary) -> float:
	return maxf(float(entry.get(RUNG_CATALOG_MATERIAL_COST, RUNG_CATALOG_MATERIAL_NONE)),
		RUNG_CATALOG_MATERIAL_NONE)

static func catalog_material_id(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_MATERIAL_ID, RUNG_CATALOG_MATERIAL_NO_ID)).strip_edges()

## ⛔ **THE PAIR, OR NOTHING — the one reader every caller goes through**, `HudRouteVocab`'s own rule
## one branch over. `{}` unless the rung declares BOTH a noun and an amount above zero, so *this rung
## eats nothing* and *this rung eats 8 wood* are the only two answers a caller can get. Shaped as a
## `MaterialPayoff` row, so `RungLadder`'s SHARED price-aside composer takes it unchanged.
static func catalog_material_pile(entry: Dictionary) -> Dictionary:
	var wanted := catalog_material_cost(entry)
	var id := catalog_material_id(entry)
	if wanted <= RUNG_CATALOG_MATERIAL_NONE or id == RUNG_CATALOG_MATERIAL_NO_ID:
		return {}
	return {
		SourceForecast.MATERIAL_PAYOFF_ID_KEY: id,
		SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY: wanted,
	}

static func catalog_build_work_per_worker_turn(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_BUILD_PER_WORKER_TURN, RUNG_CATALOG_NO_BUILD_RATE))

static func catalog_yield_per_worker_turn(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_YIELD_PER_WORKER_TURN, RUNG_CATALOG_NO_YIELD))

static func catalog_recovery_fraction(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_RECOVERY, RUNG_CATALOG_NO_YIELD))

static func catalog_regrowth_multiplier(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_REGROWTH_MULTIPLIER, RUNG_CATALOG_REGROWTH_UNCHANGED))

static func catalog_min_capacity(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_MIN_CAPACITY, RUNG_CATALOG_NO_SITE_REQUIREMENT))

# ---- THE LADDER: ONE BRANCH'S ROWS, IN CLIMB ORDER ---------------------------------------------

## **THE WHOLE CATALOG AS TYPED ROWS**, unfiltered. Non-Dictionary entries are dropped rather than
## defaulted: a row this client cannot read has no rung key to join a working on. `[]` before any
## snapshot has arrived, which every caller renders as *no ladder to show*.
static func deposit_ladder(catalog: Array) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for entry_variant in catalog:
		if entry_variant is Dictionary:
			rows.append(entry_variant as Dictionary)
	return rows

## ⛔ **ONE BRANCH'S ROWS, BOTTOM RUNG FIRST — the only walk any surface may make.** The wire's `order`
## is per branch, so sorting the whole catalog by it interleaves two ladders; every consumer asks for
## the branch it is about and gets a real climb order.
static func branch_ladder(ladder: Array[Dictionary], branch: String) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for entry in ladder:
		if catalog_branch(entry) == branch:
			rows.append(entry)
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return catalog_order(a) < catalog_order(b))
	return rows

## …and the branch a WORKING climbs, which is the branch its material belongs to.
static func working_ladder(ladder: Array[Dictionary], deposit: Dictionary) -> Array[Dictionary]:
	return branch_ladder(ladder, branch_of(deposit))

## One rung's whole catalog row, `{}` for a rung the catalog does not carry (a client that has not been
## sent one, or a rung above the top of its branch).
static func ladder_entry_of(ladder: Array[Dictionary], rung_key: String) -> Dictionary:
	if rung_key == RUNG_CATALOG_NONE:
		return {}
	for entry in ladder:
		if catalog_rung_key(entry) == rung_key:
			return entry
	return {}

## Where a rung sits on its branch — `RUNG_CATALOG_NO_ORDER` for one the catalog does not carry, which
## sorts it below the floor and so above nothing.
static func ladder_order_of(ladder: Array[Dictionary], rung_key: String) -> int:
	return catalog_order(ladder_entry_of(ladder, rung_key))

## One rung's name as every surface says it — the catalog's own word, and the raw wire key for a rung
## the catalog does not carry. **This is what replaced `RUNG_LABELS`**, so the tile card, the roster,
## the sheet and the ladder cannot spell one rung four ways.
static func ladder_rung_name(ladder: Array[Dictionary], rung_key: String) -> String:
	var entry := ladder_entry_of(ladder, rung_key)
	return catalog_display_name(entry) if not entry.is_empty() else rung_key

## The branch's FREE FLOOR — its lowest row. `{}` for a branch the catalog does not carry. It is what
## a rung's payoff is measured AGAINST: *reaches 85% of the seam* is a claim about the gap between this
## rung's reach and the floor's.
static func branch_floor_entry(ladder: Array[Dictionary], branch: String) -> Dictionary:
	var rows := branch_ladder(ladder, branch)
	return rows[0] if not rows.is_empty() else {}

## ⛔ **THE RUNG DIRECTLY ABOVE THE ONE THIS WORKING HOLDS**, as a catalog row — `{}` at the top of a
## branch and for a working standing on a rung the catalog does not carry. The walk is within ONE
## branch, so a wood is never offered a quarry.
static func ladder_next_entry(ladder: Array[Dictionary], deposit: Dictionary) -> Dictionary:
	var rows := working_ladder(ladder, deposit)
	var standing := ladder_order_of(rows, rung_of(deposit))
	for entry in rows:
		if catalog_order(entry) > standing:
			return entry
	return {}

## ⛔ **WHICH RUNG TEACHES THIS CRAFT — the gate's remedy, LOOKED UP AND NEVER INFERRED.** The obvious
## shortcut is *the rung beneath the gated one* (`requires_rung`), and it holds for the five rungs that
## ship: deadfall teaches Woodcraft, which opens the felling above it. **It is a coincidence of this
## config and not a property of the ladder** — and it matters because it is a REMEDY, so a wrong answer
## sends the player to stand on the wrong ground.
##
## `""` where no rung on this branch teaches it, which is a real state: a deposit rung may be gated on
## a craft another branch earns. The caller then states the craft and its progress with no remedy.
static func ladder_rung_teaching(ladder: Array[Dictionary], branch: String,
		knowledge_id: String) -> String:
	if knowledge_id == RUNG_CATALOG_NONE:
		return ""
	for entry in branch_ladder(ladder, branch):
		if catalog_earns_knowledge(entry) == knowledge_id:
			return catalog_display_name(entry)
	return ""

# ==================================================================================================
#  THE TILE CARD'S ROWS — one `Key: value` per material, and every one conditional
# ==================================================================================================
#
# ⛔ **THE ROWS READ LIKE THE ROWS ABOVE THEM ON THE CARD** — `label · value · qualifier`, the shape
# `Foraging 90 / 100 · Thriving` and `Road Path · 25% to trail` already use, joined by
# `DEPOSIT_CLAUSE_SEPARATOR`. A deposit-specific style on a card of ecology rows reads as a different
# card's row.
#
# ⛔ **A ROW THAT WOULD SAY "none" IS NOT RENDERED**, the road block's rule (issue #566): at the free
# floor a material is exactly ONE row, and the payoff row appears only where the rung buys something.
#
# ⛔ **NO UPKEEP ROW, NO COUNTDOWN, NO SHORTFALL FIGURE.** `land-readouts.md` records those as retired
# from the plant web on Ray's own instruction. The hazard is a WORD in the row's clause list; the
# figures ride the BLOCK's `tooltip_text` (`deposit_card_tooltip`), and the neglect countdown survives
# on the Work board's own hover and nowhere else.
#
# ⛔ **KEYS STAY INSIDE `DetailFormat.DETAIL_KEY_MAX_LENGTH`** so they align with every other row on
# the card. The material row's key IS the material (`Wood`, `Stone`), which is short by construction.

## ⛔ **THE PAYOFF ROW'S KEY IS A BLANK, NOT ABSENT, AND THAT IS STRUCTURAL.**
## `DetailFormat.detail_bbcode` renders a colon-free line FULL WIDTH and **closes the open `[table=2]`
## to do it** (`_split_kv` refuses `idx <= 0`, so a genuinely keyless line is unreachable as a table
## row) — and this row sits in the MIDDLE of the card, so a keyless payoff would split the card's one
## table in two and every key below it would stop sharing a column with `Foraging` / `Grazing`.
##
## **`HudRouteVocab.ROAD_BONUS_ROW` IS THE SAME BLANK, AND THE SHARING IS THE POINT** rather than a
## collision: one unlabelled payoff row, one ink, whichever branch emitted it — `DetailFormat._value_hex`
## dispatches both to `bonus_value_hex()`, which takes no value because the row is emitted only where
## the rung buys something.
##
## **THE `Foraging` BASKET ROWS ARE NOT THE PRECEDENT.** They indent with `MORALE_BREAKDOWN_INDENT` and
## are routed to the full-width sub-row branch, which closes the table — which is exactly what a row in
## the middle of this block may not do.
const DEPOSIT_PAYOFF_ROW := " "

## `35` — the seam at full, and the free floor's whole reading. **A stock at capacity states ONE
## number**: `35 of 35` is a ratio spent on the absence of a drawdown.
const DEPOSIT_STOCK_FULL_FORMAT := "%s"

## …and `1,870 of 2,200` once anything has been taken. The pair is the argument: what is standing
## against what this GROUND holds when full, which no rung may raise.
const DEPOSIT_STOCK_DRAWN_FORMAT := "%s of %s"

## ⛔ **WHAT THE RUNG BUYS OVER ITS BRANCH'S FREE FLOOR, IN ONE CLAUSE.** It is the one thing on this
## card that states a PAYOFF, which is why it is tinted rather than left in plain ink — without it a
## working reads as pure cost and the decision the ladder exists to create is invisible.
const DEPOSIT_PAYOFF_REACH_FORMAT := "reaches %d%% of the seam"

## The coppice's payoff, and the one point on the scale English has a word for. Any other multiplier
## states the factor itself rather than inventing a word for it.
const DEPOSIT_REGROWTH_TWICE := 2.0
const DEPOSIT_PAYOFF_REGROWTH_TWICE := "grows back twice as fast"
const DEPOSIT_PAYOFF_REGROWTH_FORMAT := "grows back %s× as fast"

## The scale a recovery fraction is stated at.
const DEPOSIT_RECOVERY_PERCENT_SCALE := 100.0

## How many decimals the regrowth multiplier reads at, so `2.5` states itself and `2.0` does not read
## as `2.00`.
const DEPOSIT_REGROWTH_DECIMALS := 1

## **THE WHOLE DEPOSIT BLOCK FOR ONE MATERIAL**, as `Key: value` detail lines — `HudRouteVocab.road_lines`'
## twin, and it keeps that composer's two rules: only the material row is unconditional, and every join
## (the catalog) is resolved at the CALL SITE and threaded in, so this leaf holds no catalog.
##
## `ctx` is the render's tint/hover context. The FIGURES the row's one line cannot carry are registered
## on it against the MATERIAL row's key — never the payoff row's, which two materials on one hex would
## both claim.
static func deposit_lines(deposit: Dictionary, ladder: Array[Dictionary],
		ctx: DetailFormat.Context = null) -> Array[String]:
	var lines: Array[String] = []
	var key := material_label(deposit)
	if key == "":
		return lines
	lines.append("%s: %s" % [key, deposit_land_value(deposit, ladder)])
	if ctx != null:
		ctx.deposit_rows[key] = true
		var figures := deposit_card_tooltip(deposit)
		if figures != "":
			ctx.row_tooltips[key] = figures
	var payoff := deposit_payoff_clause(
		ladder_entry_of(ladder, rung_of(deposit)),
		branch_floor_entry(ladder, branch_of(deposit)))
	if payoff != "":
		lines.append("%s: %s" % [DEPOSIT_PAYOFF_ROW, payoff])
	return lines

## `1,870 of 2,200 · Quarry · ⚠ going back` — **THE MATERIAL ROW'S VALUE**, and it leads with the STOCK
## because the card's subject is the GROUND: what is standing here, then what stands on it.
##
## ⛔ **IT IS NOT `deposit_row_value`, AND THE TWO PART ON PURPOSE.** That composer is the WORKINGS
## ROSTER's, whose subject is a working the band already holds — so it leads with the rung and carries
## the runway and the climb. This one describes a hex the player is looking at, most of which is ground
## nobody has opened. **The clauses they share are shared functions** (`ladder_rung_name`,
## `hazard_clause`), so the rung's word and the hazard's cannot drift between them.
static func deposit_land_value(deposit: Dictionary, ladder: Array[Dictionary]) -> String:
	var clauses: Array[String] = [deposit_stock_clause(deposit),
		ladder_rung_name(ladder, rung_of(deposit))]
	var hazard := hazard_clause(deposit)
	if hazard == "" and renews(deposit) and actual_take_of(deposit) > sustainable_take_of(deposit):
		# **THE OVER-CUT WORD IS THE SECOND HAZARD THIS ROW CAN CARRY**, and it is the food webs' own
		# word (`SourceForecast.YIELD_OVERDRAW_WORD`) rather than a second spelling of one idea. The
		# under-kept clause outranks it: a working sliding back down its ladder is the louder fact and
		# the row holds one qualifier.
		hazard = DEPOSIT_HAZARD_CLAUSE_FORMAT % [
			HudSelectionVocab.RUNG_HAZARD_GLYPH, over_cut_word()]
	if hazard != "":
		clauses.append(hazard)
	return DEPOSIT_CLAUSE_SEPARATOR.join(clauses)

## The stock as the tile card states it. ⛔ **NEVER `0 / 0`, and never a ratio on a full seam**: a
## deposit at capacity has had nothing taken out of it, and the second figure would be a comparison
## with itself.
static func deposit_stock_clause(deposit: Dictionary) -> String:
	var stock := stock_of(deposit)
	var capacity := capacity_of(deposit)
	var standing := DetailFormat.format_trimmed(stock, CARD_STOCK_DECIMALS)
	if stock >= capacity:
		return DEPOSIT_STOCK_FULL_FORMAT % standing
	return DEPOSIT_STOCK_DRAWN_FORMAT % [standing,
		DetailFormat.format_trimmed(capacity, CARD_STOCK_DECIMALS)]

## ⛔ **WHAT THE HELD RUNG BUYS OVER ITS BRANCH'S FREE FLOOR — `""` where it buys nothing**, which is
## both floors and every extraction rung that neither reaches deeper nor renews faster.
##
## The two axes are the branches' own: `recovery_fraction` is what a finite seam's ladder is FOR (a
## surface picker reaches 15% of a rock body and a quarry 85%), and `regrowth_multiplier` is what a
## renewing one's is (a coppice is *more per turn for ever*, not more per turn). **Both are read off
## the catalog and neither is derived from the working**, so a drawn-down seam cannot make its own
## rung's reach appear to shrink.
static func deposit_payoff_clause(entry: Dictionary, floor_entry: Dictionary) -> String:
	if entry.is_empty():
		return ""
	var clauses: Array[String] = []
	var reach := catalog_recovery_fraction(entry)
	if reach > catalog_recovery_fraction(floor_entry):
		clauses.append(DEPOSIT_PAYOFF_REACH_FORMAT % int(round(
			reach * DEPOSIT_RECOVERY_PERCENT_SCALE)))
	var renewal := catalog_regrowth_multiplier(entry)
	if renewal > catalog_regrowth_multiplier(floor_entry):
		clauses.append(DEPOSIT_PAYOFF_REGROWTH_TWICE if is_equal_approx(
				renewal, DEPOSIT_REGROWTH_TWICE) \
			else DEPOSIT_PAYOFF_REGROWTH_FORMAT % DetailFormat.format_trimmed(
				renewal, DEPOSIT_REGROWTH_DECIMALS))
	return DEPOSIT_CLAUSE_SEPARATOR.join(clauses)

## **THE FIGURES THAT LEFT THE ROW, ON THE BLOCK'S HOVER** — the §7 take pair or the runway, and the
## standing bill where the working owes one. ⛔ **Nothing was deleted; it MOVED here**, which is the
## whole of what "no upkeep row on this card" means.
##
## ⛔ **NO NEGLECT COUNTDOWN.** That reading survives on the Work board's own roster hover
## (`deposit_roster_tooltip`) and nowhere else — a countdown on the land card is a figure the player
## cannot act on from there.
static func deposit_card_tooltip(deposit: Dictionary) -> String:
	var lines: Array[String] = [supply_tooltip(deposit)]
	var bill := upkeep_value(deposit)
	if bill != "":
		lines.append(DEPOSIT_UPKEEP_TIP_FORMAT % bill)
	return HudFormat.join_tooltip_lines(lines)

## `Holding it: 1.5 work a turn · 2 keepers` — the bill as a HOVER sentence, so the figures read as a
## fact about the working rather than as a row the card is spending a line on.
const DEPOSIT_UPKEEP_TIP_FORMAT := "Holding it: %s"

## …and the WORK BOARD's version, which is the card's plus the neglect COUNTDOWN. The roster is the one
## surface that states when a working goes back, because it is the surface whose own head staffs the
## pool that would stop it.
static func deposit_roster_tooltip(deposit: Dictionary) -> String:
	var lines: Array[String] = [deposit_card_tooltip(deposit)]
	var countdown := reverting_value(deposit)
	if countdown != "":
		lines.append(DEPOSIT_REVERTING_TIP_FORMAT % countdown)
	return HudFormat.join_tooltip_lines(lines)

const DEPOSIT_REVERTING_TIP_FORMAT := "Going back %s"

## The material row's INK, forked on the hazard mark the composer put there rather than on a second
## reading of the working — `deposit_value_color`'s rule, and the same one.
static func deposit_land_value_hex(value: String) -> String:
	if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
		return HudStyle.WARN_HEX
	return HudStyle.INK_HEX

# ==================================================================================================
#  THE TWO COMPOSE SHEETS — `Assign foresters ▸` and `Assign diggers ▸`
# ==================================================================================================
#
# ⛔ **THE CREW NOUN IS PER BRANCH, NEVER PER RUNG** — the `Harvesters` rule
# (`labor-ui.md` → "The plant web's crew is `Harvesters`"): *a build in flight does not move the noun*.
# A crew cutting a coppice is still foresters, and a second word would be the plant web's retired
# `Foragers`/`Tenders` fork arriving on a third branch.

## The wire's own branch spellings — `RungBranch`'s, and the same strings `branch_of` reads off a
## working and `catalog_branch` off a rung.
const BRANCH_FORESTRY := "forestry"
const BRANCH_EXTRACTION := "extraction"

## The crew each branch staffs, and the verb its commit button carries. Two tables rather than one
## keyed record, the `IMPROVEMENT_*_LABELS` idiom: each answers one question and a caller reads one.
const BRANCH_CREW_NOUNS := {
	BRANCH_FORESTRY: "Foresters",
	BRANCH_EXTRACTION: "Diggers",
}

const BRANCH_COMMIT_VERBS := {
	BRANCH_FORESTRY: "Cut",
	BRANCH_EXTRACTION: "Dig",
}

## …and the NOUN the pointer line names the ground with — *this stand* for a wood, *this rock* for a
## seam. It is per BRANCH for the crew noun's reason: the rung's own verb already says which rung.
const BRANCH_GROUND_NOUNS := {
	BRANCH_FORESTRY: "this stand",
	BRANCH_EXTRACTION: "this rock",
}

## The crew this branch staffs — `""` for a branch this client has never heard of, which every caller
## renders as no sheet rather than as an unnamed one.
static func crew_noun(branch: String) -> String:
	return String(BRANCH_CREW_NOUNS.get(branch, ""))

static func commit_verb(branch: String) -> String:
	return String(BRANCH_COMMIT_VERBS.get(branch, ""))

static func ground_noun(branch: String) -> String:
	return String(BRANCH_GROUND_NOUNS.get(branch, ""))

## ⛔ **THE SHEET EMITS NO IMPROVEMENT VERB — `assign_labor` is the only command it sends.** The rung
## is declared from the Work board, exactly as `cultivate` and `sow` are, and this line is the pointer
## that says so. `Work tab` is a live `[url]`; the verb and the ground's noun come off the NEXT rung's
## catalog entry and this branch's own table.
const DEPOSIT_OFFER_LABEL_FORMAT := "%s %s"

## …and the form for a band that does not work this ground yet. The sim's rule is that an improvement
## verb reaches only bands ALREADY working the source, so the sheet says so where the player meets it
## rather than offering a link that would land on a board with no row to press.
const DEPOSIT_OFFER_UNWORKED_FORMAT := "Send %s here first, then %s from the %s."

## The rung a pointer line names — `Quarry this rock`. `""` where the working is at the top of its
## branch or the next rung declares no verb, and the sheet then states no pointer at all.
static func offer_label(entry: Dictionary, branch: String) -> String:
	var verb := catalog_verb(entry)
	if verb == RUNG_CATALOG_NONE:
		return ""
	var noun := ground_noun(branch)
	if noun == "":
		return ""
	return DEPOSIT_OFFER_LABEL_FORMAT % [verb.capitalize(), noun]

# ---- THE READOUT BOX ---------------------------------------------------------------------------

## `ONCE QUARRIED` — the deal row's label, in the readout's own small-print register: the rung's own
## verb in the past tense, so the row names the rung rather than restating the material.
##
## ⛔ **THREE SUFFIX RULES, AND EACH IS A RULE RATHER THAN A SPECIAL CASE.** The shipped verbs are
## `fell` → `felled`, `coppice` → `coppiced` (a silent `e` takes `d` alone) and `quarry` →
## `quarried` (a consonant + `y` becomes `ied`). A verb whose participle is genuinely irregular
## falls through to the plain suffix, which reads as a mangled word — the honest failure, and visible
## the first time a config adds one.
const DEPOSIT_DEAL_LABEL_FORMAT := "once %sed"
const DEPOSIT_DEAL_LABEL_SILENT_E_FORMAT := "once %sd"
const DEPOSIT_DEAL_LABEL_Y_FORMAT := "once %sied"

## The two endings those rules fork on, named because each IS the rule.
const DEPOSIT_DEAL_SILENT_E := "e"
const DEPOSIT_DEAL_CONSONANT_Y := "y"

## `6.60 stone a turn` — what the next rung would pay at the crew being composed, off its own
## `yieldPerWorkerTurn`. **Not a client-side projection of the take**: it is the catalog's rate times
## the stepper's count, which is the sim's own arithmetic before the reachable stock caps it.
const DEPOSIT_DEAL_VALUE_FORMAT := "%s %s a turn"

## The deal row's label for one rung — `once quarried`. `""` for a rung with no verb, which has no
## deal to state.
static func deal_label(entry: Dictionary) -> String:
	var verb := catalog_verb(entry)
	if verb == RUNG_CATALOG_NONE:
		return ""
	if verb.ends_with(DEPOSIT_DEAL_SILENT_E):
		return DEPOSIT_DEAL_LABEL_SILENT_E_FORMAT % verb
	if verb.ends_with(DEPOSIT_DEAL_CONSONANT_Y):
		return DEPOSIT_DEAL_LABEL_Y_FORMAT % verb.left(verb.length() - 1)
	return DEPOSIT_DEAL_LABEL_FORMAT % verb

## …and its value, at the crew the stepper is on. `""` at a crew of zero or for a rung the catalog
## prices no take on — a deal quoted at nobody is a promise of nothing.
static func deal_value(entry: Dictionary, deposit: Dictionary, crew: int) -> String:
	var rate := catalog_yield_per_worker_turn(entry)
	if rate <= RUNG_CATALOG_NO_YIELD or crew <= 0:
		return ""
	return DEPOSIT_DEAL_VALUE_FORMAT % [
		DetailFormat.format_trimmed(rate * float(crew), CARD_STOCK_DECIMALS),
		material_of(deposit)]

## ⛔ **THE SENTENCE THE WHOLE BRANCH TURNS ON, ON A FINITE SEAM** — `Gathering reaches 330 of 2,200. A
## quarry would reach 1,870.` Composed from the CATALOG's `recovery_fraction` and the working's own
## `capacity`, never from `reachable / capacity`: that ratio clamps to the stock, so it would fall as
## the rock is worked and quietly restate the rung's reach as something it is not.
const DEPOSIT_VERDICT_REACH_FORMAT := "%s reaches %s of %s."
const DEPOSIT_VERDICT_REACH_NEXT_FORMAT := " A %s would reach %s."

## …and the RENEWING seam's verdict, which is the over-cut sentence rather than a reach.
const DEPOSIT_VERDICT_OVER_CUT_FORMAT := "Cutting %s a turn against %s that grows back."
const DEPOSIT_VERDICT_WITHIN_FORMAT := "Cutting %s a turn, inside the %s that grows back."

## The readout's verdict — `{severity, text}` as `HudWidgets.build_verdict_line` takes it.
##
## ⛔ **THE FORK IS `regrowth_rate > 0`, NEVER THE BRANCH** — `renews()`, the one fork, so a renewing
## flint scatter and a quarry of the same branch read differently and neither borrows the other's
## sentence.
static func deposit_verdict(deposit: Dictionary, ladder: Array[Dictionary]) -> Dictionary:
	if renews(deposit):
		var actual := DetailFormat.format_trimmed(actual_take_of(deposit), CARD_STOCK_DECIMALS)
		var sustainable := DetailFormat.format_trimmed(
			sustainable_take_of(deposit), CARD_STOCK_DECIMALS)
		if actual_take_of(deposit) > sustainable_take_of(deposit):
			return {
				"severity": SourceForecast.VERDICT_BLOCKED,
				"text": DEPOSIT_VERDICT_OVER_CUT_FORMAT % [actual, sustainable],
			}
		return {
			"severity": SourceForecast.VERDICT_OK,
			"text": DEPOSIT_VERDICT_WITHIN_FORMAT % [actual, sustainable],
		}
	var branch := branch_of(deposit)
	var standing := ladder_entry_of(ladder, rung_of(deposit))
	var capacity := capacity_of(deposit)
	var text := DEPOSIT_VERDICT_REACH_FORMAT % [
		ladder_rung_name(ladder, rung_of(deposit)),
		DetailFormat.format_trimmed(
			catalog_recovery_fraction(standing) * capacity, CARD_STOCK_DECIMALS),
		DetailFormat.format_trimmed(capacity, CARD_STOCK_DECIMALS)]
	var next_entry := ladder_next_entry(ladder, deposit)
	if not next_entry.is_empty() \
			and catalog_recovery_fraction(next_entry) > catalog_recovery_fraction(standing):
		text += DEPOSIT_VERDICT_REACH_NEXT_FORMAT % [
			catalog_display_name(next_entry).to_lower(),
			DetailFormat.format_trimmed(
				catalog_recovery_fraction(next_entry) * capacity, CARD_STOCK_DECIMALS)]
	return {
		"severity": SourceForecast.VERDICT_OK if next_entry.is_empty() \
			else SourceForecast.VERDICT_SLOW,
		"text": text,
	}

## `Runs out in 275 turns at this rate.` — the aside under the dashed rule, and it honours BOTH
## sentinels: `RUNWAY_NO_TAKE` is *nobody is cutting it* and never `0 turns`, and
## `RUNWAY_NOT_APPLICABLE` cannot occur on a finite deposit by construction. `""` on a renewing one,
## which has no runway to state.
const DEPOSIT_RUNWAY_ASIDE_FORMAT := "Runs out in %d turns at this rate."
const DEPOSIT_RUNWAY_ASIDE_ONE := "Runs out next turn at this rate."
const DEPOSIT_RUNWAY_ASIDE_IDLE := "Nobody is cutting it, so it is running out at no rate at all."

static func runway_aside(deposit: Dictionary) -> String:
	if renews(deposit):
		return ""
	var turns := turns_remaining_of(deposit)
	if turns == RUNWAY_NO_TAKE:
		return DEPOSIT_RUNWAY_ASIDE_IDLE
	if turns < 0:
		return ""
	if turns == 1:
		return DEPOSIT_RUNWAY_ASIDE_ONE
	return DEPOSIT_RUNWAY_ASIDE_FORMAT % turns

## ⛔ **THE MOST CUTTERS THIS WORKING CAN USE — `reachable / yieldPerWorkerTurn`, rounded UP.** A crew
## takes `min(crew × rate, reachable)` in a turn, so hands beyond that quotient take nothing and the
## `+` states so rather than offering them. `CUTTERS_UNCAPPED` where the catalog prices no rate, which
## is a client that has not been sent one — the cap is then the band's own pool and nothing else.
const CUTTERS_UNCAPPED := -1

## **NOBODY IS ON THIS WORKING**, and it is a real and common state rather than an absence: the roster
## lists a working the moment the band holds an `extract` row on it, at ZERO cutters as readily as at
## five. Distinct from `CUTTERS_UNCAPPED`, which is a SENTINEL — *the catalog prices no rate here* —
## rather than a measured nothing. The CREW gate forks on it; see `GATE_KIND_CREW`.
const CUTTERS_NONE := 0

static func max_useful_cutters(deposit: Dictionary, entry: Dictionary) -> int:
	var rate := catalog_yield_per_worker_turn(entry)
	if rate <= RUNG_CATALOG_NO_YIELD:
		return CUTTERS_UNCAPPED
	return int(ceil(reachable_of(deposit) / rate))

## The dead commit button's explanation — a crew of zero on a working nobody holds, where the command
## would do nothing at all. **A dead button is always explained**, the `+` stepper's cap note being
## this client's precedent.
const DEPOSIT_NOOP_HINT_FORMAT := "Put %s on it to open this ground."

## The note under the stepper where the working itself is the ceiling — the compose sheets' own
## `alloc_hint_label` register, so it reads like the forage sheet's cap note one card over.
const CUTTERS_CAP_NOTE_FORMAT := "%d %s is all this ground can use — the rest would take nothing."

# ==================================================================================================
#  THE LADDER — the deposit branches' two tracks on the Work board
# ==================================================================================================

## ⛔ **THE NAME COLUMN IS NARROWER ON THIS BRANCH, and the wrapping is why** — the route branch's own
## measurement, arrived at for the same reason. The shared `HudWorkVocab.RUNG_TRACK_NAME_WIDTH` (150px)
## leaves 142px of a 292px card for the face, and `250 work · 1.50/turn upkeep` does not fit in it.
## Deposit rung names are short (`Gathering` is the longest the shipped ladders hold), so the column
## gives the width back to the face. It rides the ROW (`RungLadder.ROW_NAME_WIDTH_KEY`) rather than
## widening `build_track`'s signature, the plant and animal tracks wanting the wider column they have.
const DEPOSIT_LADDER_NAME_WIDTH := 96.0

## The row's two-clause face. **The figure leads**, because it is what two rows of one ladder differ by
## most and because the eye that came here for a price should not have to read past a sentence.
const DEPOSIT_LADDER_FACE_FORMAT := "%s · %s"

## ⛔ **THE RUNG'S OWN STANDING BILL, BESIDE THE PILE IT WOULD COST TO RAISE** — the second half of what
## the press commits to, and the half a one-off figure cannot state. It is the CATALOG's
## `upkeepWorkPerTurn`, never a progress-scaled figure: a rung nobody has started has no live bill to
## scale, and the number being weighed is what holding it will cost for ever.
const DEPOSIT_LADDER_UPKEEP_FORMAT := "%s/turn upkeep"

## ⛔ **A RATE IS PRINTED TO TWO DECIMALS AND THE BUILD PILE IS NOT** — the route branch's finding
## verbatim: `DetailFormat.format_work_units` rounds to one, which prints a 1.50 rate as `1.5` and
## would print a 0.45 one as `0.5`, an 11% lie about the number the player is deciding against. `250`
## does not care.
const DEPOSIT_LADDER_RATE_DECIMALS := 2

# ⛔ **THE MATERIAL ASIDE IS THE SHARED ONE** (`HudWorkVocab.RUNG_TRACK_BUILD_MATERIAL_FORMAT`,
# `+ 8 wood to raise it`), composed by `RungLadder._build_price_asides` from
# `catalog_material_pile` — so a deposit rung's pile, a pen's hurdles and a paved road's stone all
# read in one format and in one order. A branch-local spelling of it was written here first and
# deleted: it would have been the same sentence typed a fourth time.

## **WHAT A RUNG NOBODY HAS STARTED COSTS — the pile AND the bill, and NO DURATION.** A priced row
## quotes no turns: the estimate would be divided by a builders pool that may be on another job and
## would ignore the queue the press joins. The two figures that do not move with a crew are what the
## row states.
##
## **THE UPKEEP CLAUSE IS DROPPED WHERE THE RUNG DECLARES NONE** — both free floors hold for nothing,
## so `0/turn upkeep` would be a bill where there is no bill.
static func deposit_ladder_price_face(entry: Dictionary) -> String:
	var pile := HudWorkVocab.RUNG_TRACK_COST_UNDATED_FORMAT % DetailFormat.format_work_units(
		catalog_work_cost(entry))
	var upkeep := catalog_upkeep(entry)
	if upkeep < SourceForecast.UPKEEP_WORK_MIN:
		return pile
	return DEPOSIT_LADDER_FACE_FORMAT % [pile,
		DEPOSIT_LADDER_UPKEEP_FORMAT % DetailFormat.format_trimmed(
			upkeep, DEPOSIT_LADDER_RATE_DECIMALS)]

## The ladder row's HOVER — what it costs to build AND to keep, what it buys, and then every refusal.
## **The hover's order is the sentence.**
const DEPOSIT_LADDER_TIP_PRICE_FORMAT := "%s work to raise, %s work a turn to keep."
const DEPOSIT_LADDER_TIP_SEPARATOR := "\n"

# ---- THE GATES: A SHORT FORM FOR THE ROW, A SENTENCE FOR THE HOVER ------------------------------
#
# ⛔ **A GATED ROW IS SHOWN AND EXPLAINED, NEVER HIDDEN.** The track exists to say what the branch
# HOLDS; a rung silently missing reads as a shorter ladder rather than as one this working cannot climb.
#
# ⛔ **AND THE WORD `locked` IS NOT USED.** A row reading `locked` above a reason says it twice — the
# reason alone IS the state, and the row stays disabled by its ink and by being a `Label`.
#
# **The RECORD's own field names are `HudRouteVocab.GATE_KIND_KEY` / `GATE_SHORT_KEY` / `GATE_LONG_KEY`,
# shared deliberately**: one refusal shape for every branch, so `RungGates`' row-pick and hover-join
# read one spelling. What is per branch is the PRIORITY below.

## The gate KINDS this branch can state, in the order a ROW prefers them — first match wins, and the
## tooltip keeps the rest.
##
## ⛔ **THE GROUND GATE SINKS TO LAST**, the route branch's own finding: *needs a felling* names a rung
## the ladder is already displaying one line up under the rung it stands on, so it earns least on a
## line that holds one clause. **The SITE gate outranks the craft** because it is the only refusal here
## that no amount of learning or standing will ever close — this ground will never take a quarry — and
## telling a player to go and learn Quarrying for a 35-unit scatter is wrong advice.
const GATE_KIND_WORN_IN := "worn_in"
const GATE_KIND_SITE := "site"
const GATE_KIND_CREW := "crew"
const GATE_KIND_CRAFT := "craft"
const GATE_KIND_GROUND := "ground"
const GATE_ROW_PRIORITY := [
	GATE_KIND_WORN_IN,
	GATE_KIND_SITE,
	GATE_KIND_CREW,
	GATE_KIND_CRAFT,
	GATE_KIND_GROUND,
]

## The rung nobody declares — the free floor. It is not refused for want of anything; the ground
## already offers it and there is no order to give. **Stated ALONE**, so a craft or a site rule beside
## it cannot read as a prerequisite for something that is not on offer.
##
## **The row's own word is `HudWorkVocab.RUNG_TRACK_STATE_WORN_IN`** — one spelling, in the state table
## with the other six — so this carries the hover's sentence alone and no short form.
const GATE_LONG_WORN_IN := "The ground already offers this. There is nothing to order."

## The GROUND gate — `requires_rung`.
const GATE_SHORT_NEEDS_RUNG_FORMAT := "needs a %s"
const GATE_LONG_NEEDS_RUNG_FORMAT := "Needs a %s first."

## The CRAFT gate. **The discovery is named from the ladder's own knowledge roster**, so a rung added to
## `intensification_ladder.json` names its unlock with no client edit; a roster carrying no name for it
## yet says so plainly rather than printing a blank.
const GATE_SHORT_NEEDS_CRAFT_FORMAT := "needs %s"
const GATE_SHORT_NEEDS_CRAFT_UNNAMED := "needs a craft"
const GATE_LONG_KNOWLEDGE_HEAD_FORMAT := "%s known %d%%."
const GATE_LONG_KNOWLEDGE_HEAD_UNNAMED_FORMAT := "This craft is known %d%%."
const GATE_LONG_KNOWLEDGE_REMEDY_FORMAT := " Learn it by holding a %s."

## ⛔ **THE SITE GATE, AND IT IS NEW TO THIS BRANCH** — the route branch has no placement rule at all.
## `min_deposit_capacity` against the working's own `capacity` is the whole of *you cannot quarry just
## anywhere*, and it is what refuses a quarry on a 35-unit periglacial scatter. **Both figures are
## published**: the threshold rides the catalog and the capacity rides the working, so nothing here
## transcribes a rule the config owns.
const GATE_SHORT_TOO_SMALL := "too small"
const GATE_LONG_TOO_SMALL_FORMAT := "Wants ground holding %s; this one holds %s."

## ⛔ **THE CREW GATE — the one refusal on this card whose CAUSE is nowhere on the surface it is read
## from.** `queue_build_on_working_bands` filters `workers > 0`, so a working held at a rung with its
## cutters pulled off refuses every verb on its ladder — `cultivate`'s shipped rule, applied unchanged.
## And the roster lists a 0-crew working (a working with nobody on it is still held and still owes,
## which is the whole reason the pool exists), so the ladder opens on one in ONE CLICK.
##
## ⛔ **THE ROW CANNOT STATE THE CREW, so this gate has to.** roads.md's per-row prohibition forbids a
## crew count on a roster row, and the sheet that staffs the working is on another surface — so a
## player who has pulled the cutters off sees a card of refusals with nothing anywhere near it saying
## why. A refusal the player cannot explain is the defect the gate records exist to prevent.
##
## **IT SITS BELOW THE SITE GATE AND ABOVE THE CRAFT**, which is the SITE gate's own argument read
## twice: a scatter will never take a quarry however many diggers stand on it, so *put diggers on it*
## is wrong advice there — but a craft is the branch's long game while this bites TODAY and closes in
## one gesture, so it leads where both are unmet.
const GATE_SHORT_NO_CREW := "no crew"
const GATE_LONG_NO_CREW_FORMAT := "Nobody is on this ground. Put %s on it before you order a rung."
const GATE_LONG_NO_CREW_UNNAMED := "Nobody is on this ground. Put a crew on it before you order a rung."
