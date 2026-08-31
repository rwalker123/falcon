class_name HudRouteVocab

## ROAD vocabulary — the intensification ladder's third branch (arc #532,
## `.claude/rules/core_sim/routes.md`). The rung names, the road readout's row keys and formats, and
## the one composer behind each of its rows.
##
## ⛔ **A ROAD IS ONE TILE.** Each tile carries its own rung, its own meter, its own keeper and its
## own decay (`.claude/rules/core_sim/routes.md`), which is what makes *"one band keeps half the tiles
## between two camps and another keeps the rest"* a state the world can hold at all. There is no
## stored path on the row and nothing here reads one.
##
## ⛔ **A ROAD IS NOT AN ORDER PATH.** `AnnotationRenderer._routes` and `map_preview`'s `"routes"`
## annotation state are the per-faction ORDER-PATH overlay — waypoints a player's own movement orders
## are following, which vanish when the order does. A road is a WORLD OBJECT in the ground that
## outlives every band that walks it, and it does not follow a camp. The obvious name was taken by
## the other thing first, so the client noun for this one is **road** throughout
## (`MapView.road_network`, `AnnotationRenderer.draw_road_network`, `ui_preview`'s `road_*` states).
##
## ⛔ **NOTHING HERE RE-DERIVES A NUMBER THE SIM ALREADY ANSWERED.** The bill, its shortfall, the
## keeper count and the neglect countdown are four published fields, and the composers below read
## them and nothing else — no `demand − supplied`, no `ceil(demand / rate)`, no counting turns. The
## sim publishes all four precisely so the client does not compute them, and this branch is the most
## exposed to that defect (the demand moves WITHIN a turn as bands walk on and off a road).

# ---- THE FOUR RUNGS --------------------------------------------------------------------------
#
# The wire's own `<branch>:<id>` spelling (`RungKey::wire_key`), the same grammar `plant:tended` and
# `animal:pen` use one branch over.
#
# ⛔ **THE RUNG STRING IS THE BOOL.** `build_fraction` beside it on the wire is the meter on the rung
# being RAISED, which is a DIFFERENT rung — a reader that thresholded the float would call a
# fully-worn trail a dirt road on the turn its first traffic banked.
const RUNG_KEY_PATH := "route:path"
const RUNG_KEY_TRAIL := "route:trail"
const RUNG_KEY_DIRT_ROAD := "route:dirt_road"
const RUNG_KEY_PAVED_ROAD := "route:paved_road"

## The branch in climb order, floor first. **The order is the BRANCH's, not this file's** — it is
## `path → trail → dirt road → paved road` in `intensification_ladder.json`, restated here for
## the same reason `SourceForecast.RUNG_KEY_*` restates the plant and animal ladders: the client is
## sent the rung a road HOLDS and must be able to name the one traffic is wearing in above it.
##
## A rung this list does not know renders its meter without a destination name rather than guessing
## one — see `progress_clause`.
const RUNG_ORDER := [
	RUNG_KEY_PATH,
	RUNG_KEY_TRAIL,
	RUNG_KEY_DIRT_ROAD,
	RUNG_KEY_PAVED_ROAD,
]

## Player-facing names. The floor is *"Path"* rather than *"None"* because it IS a thing — a rung a
## road really holds and the row therefore renders, not the absence of one (issue #215).
##
## ⛔ **IT IS A PATH AND NOT A *"GAME TRAIL"*, AND THAT IS A CORRECTION.** The old name asserted an
## ORIGIN the sim does not model: exactly one pass banks route work — the pooling-link pass in
## `core_sim/src/supply.rs` — and no animal has ever worn a step of any road. So a path worn in by
## the player's own trade traffic was shown to him as *"Game trail"*. Same reason the row's retired
## `nothing — a path the animals made` clause went (see RETIRED `ROAD_BUYS_NOTHING` below); this is
## the second half of that fix, on the rung the sentence was hung off.
const RUNG_LABELS := {
	RUNG_KEY_PATH: "Path",
	RUNG_KEY_TRAIL: "Trail",
	RUNG_KEY_DIRT_ROAD: "Dirt road",
	RUNG_KEY_PAVED_ROAD: "Paved road",
}

# RETIRED — `ROAD_GLYPH` (🛣), the badge the rung row used to lead with (issue #566). It sat in the
# value cell of a row whose KEY already reads `Road`, so it said the word twice and bought the card
# nothing; the plant/animal badges it was copied from lead rows whose keys name a rung
# (`Cultivation`, `Corral`) rather than the thing itself.

# ---- THE TILE CARD'S ROAD BLOCK --------------------------------------------------------------
#
# ⛔ **EVERY ROW HERE IS CONDITIONAL, AND A FREE PATH IS ONE ROW.** The block used to print five
# rows whatever the road was, two of which were sentences saying *no* — `Keeping: free — nobody keeps
# a path` over `Buys: nothing — a path the animals made`, which was four lines of prose to say
# that the commonest road in the game costs nothing and does nothing. **A row that would say "none"
# is not rendered at all** (issue #566): the rung row is the only unconditional one, and the other
# three appear exactly when the road has something to say with them.
#
# **THE ROWS READ LIKE THE ROWS ABOVE THEM ON THE CARD** — `label · value · qualifier`, the shape
# `Foraging  90 / 100 · Thriving` and `Grazing  9 / 10 · Thriving` already use, joined by
# `ROAD_CLAUSE_SEPARATOR`. A road-specific style on a card of ecology rows reads as a different
# card's row.
#
# Keys stay inside `DetailFormat.DETAIL_KEY_MAX_LENGTH` so they align in the card's two-column table
# with every other row on it.

## What this ground carries, and how far the traffic on it has got toward the next rung. **Always
## rendered where a road crosses the hex** — it is the row that says a road is here at all.
const ROAD_ROW := "Road"

## ⛔ **THE PAYOFF ROW, AND IT IS DELIBERATELY UNLABELLED.** `Buys:` was a key doing no work: the
## value (*40% less loss*) already reads as a benefit, so the label only narrowed the
## column the sentence had to live in.
##
## ⛔ **THE KEY IS A BLANK RATHER THAN ABSENT, AND THAT IS STRUCTURAL.** `DetailFormat.detail_bbcode`
## renders a colon-free line FULL WIDTH and closes the open `[table=2]` to do it — so a keyless payoff
## in the middle of the block would split the card's one table in two and the road's keys would stop
## sharing a column with `Foraging` / `Grazing` below them. A blank key keeps the row in the table and
## the value in the value column, which is what "bare" has to mean here.
const ROAD_BONUS_ROW := " "

## What holding it costs, per turn, and how many `roadwork` keepers that bill wants. **Rendered only
## where the road actually owes something**, which today is neither free rung.
##
## ⛔ **THE WORD IS `Upkeep`, WHICH IS THE CLIENT'S AND THE WIRE'S.** It read `Keeping` for a slice —
## a second word for the thing `HudDisclosureVocab.DETAIL_ROW_UPKEEP` names on the band's own card and
## `upkeep_demand` / `upkeep_supplied` name on the wire. **Sharing that key is deliberate**: one
## concept, one word, one row label everywhere in the client. `DetailFormat._value_hex` therefore
## carries a single `Upkeep` arm that answers for both the band's material bill and this.
const ROAD_UPKEEP_ROW := "Upkeep"

# ⛔ **RETIRED — `ROAD_KEEPER_ROW` (`Kept by`), AS A VISIBLE ROW.** It said WHOSE JOB the road is on
# a line directly beneath a bill that said how much and how many, and the pair read
# `Upkeep: 0.0 work a turn · wants 1 keepers` / `Kept by: Band 3` — two rows, three figures, and the
# only thing a player can act on (*who is on the hook, and are they covering it*) spread across both.
#
# **`ROAD_UPKEEP_ROW` ABSORBED IT.** The keeper's NAME is the bill's headline now
# (`Upkeep: Band 3`), the shortfall rides it as a clause in WORKERS (`(short 1 worker)`), and the
# figures went to the block's hover. Reported from play: *"We don't need both upkeep and kept by. We
# can show upkeep warnings, like `Upkeep: Band 3 (short 1 worker)`."*
#
# **The keeper is still the band that BUILT it, wherever that band now stands** — `route_keeping_claims`
# walks the roads a band keeps and never reads that band's position, so a band four tiles away goes on
# paying. That fact did not move; only the row it is stated on did.

## The neglect COUNTDOWN, rendered only while the road is actually at risk.
const ROAD_REVERTING_ROW := "Reverting"

## The separator between a row's value and its qualifiers — the card's own middot, so a road row
## breaks into clauses the same way the ecology rows above it do. One const for the rung row's
## qualifiers and the payoff row's clauses, because they are the same punctuation doing the same job.
const ROAD_CLAUSE_SEPARATOR := " · "

## ⛔ **`Road:` — THE RUNG IS THE FACT AND THE PERCENTAGE IS THE NEXT RUNG'S APPROACH.** A path
## at 25% is a COMPLETE path a quarter of the way to becoming a trail — it is not a road that is
## a quarter built, and the row must not be readable that way. The rung stands alone as the value and
## the meter arrives as a qualifier that names where it is GOING (`25% to trail`), which is the only
## phrasing that cannot be read as a progress bar on the rung being held.
##
## **`build_fraction` is a DIFFERENT rung's meter** (`RungKey::wire_key`'s rule) — a reader that
## thresholded it would call a fully-worn trail a dirt road on the turn its first traffic banked — so
## it can never be printed as the rung row's own value.
##
## The destination reads in LOWER CASE because it is mid-sentence there rather than heading a row.
## Every rung name is plain English with no proper noun in it (`RUNG_LABELS`), so the fold is safe and
## needs no second label table beside the first.
const ROAD_PROGRESS_FORMAT := "%d%% to %s"

## …and the same clause where `RUNG_ORDER` does not know the destination: the meter still means
## *something above this is rising*, and naming a rung this client cannot vouch for would be worse
## than not naming one.
const ROAD_PROGRESS_UNNAMED_FORMAT := "%d%% to the next rung"

## ⛔ **THE METER AT WHICH NOTHING HAS BEEN BANKED, AND IT IS THE ROW'S ONE FORK.** A MEANING — *this
## climb has not started* — rather than a rounding tolerance.
##
## **AT ZERO THE ROW STATES A PERCENTAGE IFF SOMETHING IS DECLARED.** A road nobody has ordered
## anything on has no climb to report, and a `0% to dirt road` there would invent one; a road with a
## live BUILD QUEUE entry has a real climb standing at zero, and printing it is the whole of fix 3.
## Ray: *"when it is in the queue, it should show a % complete in the road panel instead of continuing
## to make it look like it isn't queued."*
##
## **THERE IS NO SECOND PHRASING FOR IT.** `ROAD_PROGRESS_FORMAT` is the spelling above zero and it is
## the spelling at zero; a `queued for dirt road` clause beside it would be two wordings for one fact
## and would have to be told apart from the percentage it replaces.
const ROAD_METER_UNSTARTED := 0.0

## The hazard qualifier on the rung row — the branch's own consequence word behind the shared mark.
## **`washing out` is the ROUTE web's word**, beside the plant web's `slipping` and the animal web's
## `drifting`: a road nobody keeps is not abandoned, it is eroding. It rides the row as ONE MORE
## MIDDOT CLAUSE, which is the ecology rows' own shape (`205 / 205 · ⚠ Stressed`) — the space-joined
## `HudSelectionVocab.RUNG_UNDER_KEPT_FORMAT` would run the word onto the end of the progress clause
## and read as the NEXT rung washing out.
const ROAD_HAZARD_CLAUSE_FORMAT := "%s %s"
const ROAD_UNDER_KEPT_WORD := "washing out"

## **THE KEEPER'S NAME, WHICH IS NOW THE `Upkeep:` ROW'S HEADLINE.** The band's own label, as this
## client names every band (`HudBandLaborState.band_label_for_id` → `HudFormat.band_display_name`), so
## a road's keeper and the same band on the dock cannot be called two different things.
const ROAD_KEEPER_FORMAT := "%s"

## …and what a band OUTSIDE the player's roster reads as. A road may be kept by a people you merely
## know of, and naming them by a raw id would state a fact the player cannot use. **`RungGates` reads
## this too**, for the *another band keeps it* refusal, so the two surfaces name a stranger alike.
const ROAD_KEEPER_FOREIGN := "another people"

## …and a road that OWES a bill and has nobody paying it: the keeping band is gone, so the road is
## decaying towards nobody. **Re-issuing `grade` / `pave` is how it is picked up — adoption is the
## same act as building**, which is what the clause says rather than inventing an adopt verb.
const ROAD_KEEPER_NOBODY := "nobody — grade it again to take it on"

## ⛔ **WHAT DISTANCE DID TO THE PRICE — ON THE HOVER NOW, not on the row.** `keeper_remoteness` is the
## multiple the sim quoted when the keeper took the tile on, applied to BOTH the build pile and the
## standing upkeep, so a road beyond the base keeping range is dearer for exactly one reason and this
## is the only surface that says which. **It is kept because it has no other home** — distance is a
## cost and never a wall, so nothing refuses the road and nothing else explains a bill larger than the
## rung's — but it is rare and long, and the row it used to trail is the one Ray asked to shorten.
const ROAD_KEEPER_REMOTE_FORMAT := "far from them — ×%s the rung's price"

## The remoteness at which distance costs nothing — the reading inside the base keeping range, and on
## every road nobody keeps. Named because it is the TEST the clause above is gated on rather than a
## rounding tolerance.
const ROAD_REMOTENESS_AT_HOME := 1.0

## How the multiple is printed. One decimal, because the shipped dial is `2.0` and a future one need
## not be whole — and `%s` above rather than `%.1f` inline so the two cannot drift.
const ROAD_REMOTENESS_FORMAT := "%.1f"

## ⛔ **`Upkeep:` NAMES A BAND, NOT A NUMBER — AND THAT IS THE FIX, NOT A REWORDING.** The row read
## `0.0 work a turn · wants 1 keepers`, which is gibberish assembled out of two individually correct
## figures: a trail holding 2% of the dirt road banked owes ~`0.009` work a turn, which
## `DetailFormat.format_work_units` rounds DOWN to `0.0` while the sim's own `ceil` rounds the same
## number UP to one keeper. Two roundings in opposite directions on one quantity, printed side by side.
##
## **SO THE FIGURES LEFT THE ROW ENTIRELY** (`upkeep_tooltip`, registered on the block's hover) and the
## row states the decision: whose job this is, and whether they are covering it. The exact bill is one
## hover away, so it can never be the headline and `0.0 work a turn` can never render as one.
##
## **THERE IS NO "free" READING HERE.** The floor declares no upkeep at all, so the row is simply not
## emitted (see `upkeep_value`) — a sentence saying the bill does not exist is a row spent on nothing,
## and it was on every road in a current game.
const ROAD_UPKEEP_FORMAT := "%s"

## ⛔ **THE SHORTFALL IS STATED IN WORKERS AND DERIVED FROM `upkeepShortfall`, NEVER FROM
## `wants − assigned`.** The client does not know what the keeper has on `roadwork` — the assignment is
## a band-wide pool and its per-road share is the sim's answer — so the only honest conversion is
## `ceil(shortfall / PER_WORKER_OUTPUT)` off the sim's stamped field. This branch has shipped the
## subtraction as a defect twice; the shortfall is published precisely so nobody has to take it.
##
## **AND IT PLURALIZES.** The shipped row said `wants 1 keepers`.
##
## ⛔ **`ROAD_UPKEEP_SHORT_MARK` IS ONE SPELLING SERVING TWO READERS** — the composer that writes the
## clause and `upkeep_value_hex`, which forks the row's ink on it. Two spellings is how a row comes to
## say *short* in plain ink.
const ROAD_UPKEEP_SHORT_MARK := "(short "
const ROAD_UPKEEP_SHORT_FORMAT := "%s %s%d %s)"
const ROAD_UPKEEP_WORKER_SINGULAR := "worker"
const ROAD_UPKEEP_WORKER_PLURAL := "workers"

## …and the figures the row stopped printing, for the block's HOVER. **Both are still exactly the
## sim's**: the bill is `upkeepDemand` as published and the keeper count is `upkeepWorkersNeeded`, the
## sim's own `ceil` against the same stamped basis. Nothing here is derived, and nothing here is
## rounded twice.
const ROAD_UPKEEP_TIP_FORMAT := "%s work a turn · %d keeper%s"
const ROAD_UPKEEP_PLURAL_SUFFIX := "s"

## The head count at which both readouts above read SINGULAR. Named because it is the pluralisation
## test and the shortfall's own floor, not a sentinel: the shipped row said `wants 1 keepers`.
const ROAD_ONE_WORKER := 1

## `Reverting:` values — the COUNTDOWN, never the counter. `0` means it is reverting NOW, which is
## why zero gets its own word rather than printing `in 0 turns`.
const ROAD_REVERTING_NOW := "%s now"
const ROAD_REVERTING_FORMAT := "%s in %d turns"
const ROAD_REVERTING_ONE := "%s next turn"

## The countdown's own zero, named because it is a MEANING (biting now) and not a sentinel.
const ROAD_REVERTING_IMMINENT := 0

## The payoff's friction clause, stated as what it SAVES. `friction_multiplier` is the fraction of the
## base loss a network bound by this road pays, so `0.6` is *40% less lost* — the conversion is a
## presentation of the published multiplier, never a re-derivation of a sim answer.
const ROAD_BONUS_FRICTION_FORMAT := "%d%% less loss"

## The sight clause — `grants_sight` is the RESOLVED answer, because a client cannot re-derive
## *"is the bill met"* (that is a comparison against the stamped basis with the sim's own epsilon).
const ROAD_BONUS_SIGHT := "you can see along it"

## …and its other half: a BUILT road whose bill is unpaid goes dark BEFORE it decays, which is the
## honest early warning that it is being lost. Said out loud, because a clause that merely vanished
## would read as a rung that never lit anything. **It says `upkeep`, the row above it's own word.**
const ROAD_BONUS_DARK := "dark until upkeep is paid"

## ⛔ **THE LINK SPAN IS A LIVE EFFECT, AS OF SLICE 13b.** `balance_supply_networks` forms a pooling
## link at `distance <= max(reach_tiles, the weakest tile of the run)`, so this sentence states
## something the player can act on: two camps too far apart to share a larder can be joined by a road.
##
## It was authored a slice before it was consumed, and the wording was chosen then to survive that —
## *links camps up to N tiles apart* describes what the rung does rather than when it starts doing it,
## so the tense did not have to move when the sim caught up. **Keep it that way**: a rung's payoff is
## published from the config, so a new rung's line must read correctly with no client edit.
const ROAD_BONUS_LINK_FORMAT := "links camps up to %d tiles apart"

# RETIRED — `ROAD_BUYS_NOTHING` (`nothing — a path the animals made`), the sentence a rung buying
# nothing on every axis used to print (issue #566).
#
# ⛔ **IT WAS FACTUALLY WRONG, NOT MERELY WORDY.** It asserted an ORIGIN the sim does not model: a
# path is a rung a tile HOLDS, and the commonest way a tile comes to hold it is the player's own
# bands walking the same ground and banking traffic into the meter — nothing about it is a path
# animals made. The rung buying nothing is now said by the row's ABSENCE, which states the same fact
# and cannot state a false one beside it.

## The friction multiplier at which a rung takes nothing off the loss. Named because it is the
## PATH's own reading and the test the friction clause is gated on, not a rounding tolerance.
const ROAD_FRICTION_NO_HELP := 1.0

## The link span a rung holding nothing open reads. Same rule as above: a live `0`, not a parked dial.
const ROAD_LINK_NONE := 0

## The meter at which a rung is complete and nothing is rising above it. The wire states exactly
## `1.0` for a road that has just finished a rung AND for one at the top of the ladder, precisely so
## no reader has to derive it by subtraction — see `ROAD_PROGRESS_FORMAT`.
const ROAD_METER_COMPLETE := 1.0

## Percent scale for the meter. Named for the same reason `HudFormat.progress_percent` exists.
const ROAD_PERCENT_SCALE := 100.0

# ---- FIELD READERS ---------------------------------------------------------------------------
#
# One reader per wire field, so the key is spelled once. A typo in a `get` is a silent zero here, and
# on the shortfall side a silent zero would report every road as kept.

static func rung_of(road: Dictionary) -> String:
	return String(road.get("rung", "")).strip_edges()

static func build_fraction_of(road: Dictionary) -> float:
	return float(road.get("build_fraction", 0.0))

## **THE TILE IS THE ROW'S IDENTITY** — it replaced the retired `RouteId`, because with one record per
## tile there is nothing left for a separate id to name. Both consumers join on it: the map stamps a
## hex here, and the tile card cross-refs the hex under the cursor.
static func tile_of(road: Dictionary) -> Vector2i:
	return Vector2i(int(road.get("tile_x", -1)), int(road.get("tile_y", -1)))

## ⛔ **READ THIS BEFORE `keeper_band_id_of`.** `0` is a real `BandId`, so the bool is the field that
## answers *"does anybody keep this"*. `false` across the whole free floor, which is the commonest
## road in the game rather than an edge case.
static func has_keeper(road: Dictionary) -> bool:
	return bool(road.get("has_keeper", false))

static func keeper_band_id_of(road: Dictionary) -> int:
	return int(road.get("keeper_band_id", HudConst.NO_BAND_ID))

## **WHAT DISTANCE DID TO THIS ROAD'S PRICE**, as a multiple of the rung's own — quoted once, when the
## keeper took the tile on, and held for the whole job. `1.0` inside the base keeping range and on
## every road nobody keeps.
static func keeper_remoteness_of(road: Dictionary) -> float:
	return float(road.get("keeper_remoteness", ROAD_REMOTENESS_AT_HOME))

## Is this road being held at a distance the sim charged extra for? The one gate the keeper row's
## second clause forks on — and a presentation of a published multiple, never a distance this client
## measured.
static func is_remote(road: Dictionary) -> bool:
	return keeper_remoteness_of(road) > ROAD_REMOTENESS_AT_HOME

static func upkeep_demand_of(road: Dictionary) -> float:
	return float(road.get("upkeep_demand", SourceForecast.NO_UPKEEP_DEMAND))

static func upkeep_shortfall_of(road: Dictionary) -> float:
	return float(road.get("upkeep_shortfall", SourceForecast.NO_UPKEEP_DEMAND))

## **THE SIM'S OWN KEEPER COUNT.** `ceil(demand / per-worker output)`, struck against the same stamped
## basis the bill is — never re-derived here, because the client holds neither the per-worker rate nor
## the epsilon the sim rounds with.
static func upkeep_workers_needed_of(road: Dictionary) -> int:
	return int(road.get("upkeep_workers_needed", SourceForecast.NO_UPKEEP_CREW))

## **READ THIS BEFORE THE NUMBER BESIDE IT.** `false` means there is NOTHING AT RISK on this road —
## it holds only the path, which declares no upkeep and so has no meter to lose — and the
## countdown then reuses the "biting now" `0` rather than inventing a sentinel.
static func has_neglect_grace(road: Dictionary) -> bool:
	return bool(road.get("has_neglect_grace", false))

static func neglect_grace_remaining_of(road: Dictionary) -> int:
	return int(road.get("neglect_grace_remaining", ROAD_REVERTING_IMMINENT))

static func grants_sight(road: Dictionary) -> bool:
	return bool(road.get("grants_sight", false))

static func friction_multiplier_of(road: Dictionary) -> float:
	return float(road.get("friction_multiplier", ROAD_FRICTION_NO_HELP))

static func holds_link_to_tiles_of(road: Dictionary) -> int:
	return int(road.get("holds_link_to_tiles", ROAD_LINK_NONE))

## Does this road owe anything at all? The one gate the keeping row forks on, at the SAME floor every
## work rate in the client is stated at — so a `0.00 work` row can never be printed by one readout
## and suppressed by another.
static func owes_keeping(road: Dictionary) -> bool:
	return upkeep_demand_of(road) >= SourceForecast.UPKEEP_WORK_MIN

## Is this road's keeping being underpaid THIS turn? The one test the hazard mark, the map's at-risk
## styling and the countdown row all fork on, so the three cannot disagree about one road.
## `SourceForecast.upkeep_is_short`'s rule and its floor, read off the road's own field names.
static func is_short(road: Dictionary) -> bool:
	return upkeep_shortfall_of(road) >= SourceForecast.UPKEEP_WORK_MIN

## Is the road actually losing its rung — short AND past nothing left to forgive? **The bool comes
## first**: `has_neglect_grace == false` is *"nothing at risk here"*, and reading the countdown
## without it would put a `Reverting: now` on every path in the world.
static func is_at_risk(road: Dictionary) -> bool:
	return is_short(road) and has_neglect_grace(road)

# ---- COMPOSERS -------------------------------------------------------------------------------

## One rung's player-facing name; the raw wire key for a rung this client has never heard of, which
## is the honest answer rather than a blank.
static func rung_label(rung: String) -> String:
	return String(RUNG_LABELS.get(rung, rung))

## The rung traffic is wearing in ABOVE the one held — `""` at the top of the branch and for a rung
## `RUNG_ORDER` does not know. Callers state the meter without a destination in that case rather than
## naming a rung they cannot vouch for.
static func next_rung_label(rung: String) -> String:
	return rung_label(next_rung_key(rung))

## …and the same step as a KEY, for a caller that needs the CATALOG's row rather than this table's
## word — the queue block, which names a road by the rung it is being raised to and prices it from the
## sim's own catalog entry. `""` at the top of the branch and for a rung `RUNG_ORDER` does not know.
##
## ⛔ **`RUNG_LABELS` MAY NOT NAME A LADDER ROW** (its own note), so a surface holding the catalog
## resolves the name through `ladder_rung_name` and only the tile card's readout reads the four-rung
## table above.
static func next_rung_key(rung: String) -> String:
	var at := RUNG_ORDER.find(rung)
	if at < 0 or at + 1 >= RUNG_ORDER.size():
		return ""
	return String(RUNG_ORDER[at + 1])

## ⛔ **THE APPROACH TO THE NEXT RUNG, NEVER THE STATE OF THIS ONE** — `25% to trail`, not `Trail 25%`.
## `""` where nothing is rising: **`1.0` is the complete reading**, published exactly rather than
## derived by subtraction, so the test is a plain comparison and never a tolerance, and it covers both
## a rung just finished and the top of the ladder.
static func progress_clause(road: Dictionary, queued: bool = false) -> String:
	var meter := build_fraction_of(road)
	if meter >= ROAD_METER_COMPLETE:
		return ""
	# ⛔ **A ZERO METER IS A CLIMB ONLY WHERE ONE HAS BEEN ORDERED.** A road queued behind another job
	# banks nothing for dozens of turns and the row read as a bare `Trail` throughout — identical to
	# ground nobody has touched, which is what made a working `grade` indistinguishable from a failed
	# one. With an entry declared the `0%` is a true reading of a real climb; without one there is no
	# climb, and printing a percentage would invent one.
	if meter <= ROAD_METER_UNSTARTED and not queued:
		return ""
	var percent := int(floor(meter * ROAD_PERCENT_SCALE))
	var destination := next_rung_label(rung_of(road))
	if destination == "":
		return ROAD_PROGRESS_UNNAMED_FORMAT % percent
	return ROAD_PROGRESS_FORMAT % [percent, destination.to_lower()]

## ⛔ **IS THIS ROAD IN A BAND'S BUILD QUEUE? — the ONE predicate, and the join is the CALLER'S.**
##
## `queued_tiles` is `{Vector2i: true}` over every road tile the player's bands have queued, derived
## once by `HudBandLaborState.road_queue_tiles` and threaded in exactly as `keeper_label` and the
## branch's bare work rate already are. **This leaf holds no band roster and no queue**, and teaching
## it to walk one would give it both; what it can do is join a ROAD ROW to a set of tiles, which is
## arithmetic on the road's own identity.
##
## **The tile IS the join.** A queue entry publishes `kind: "roadwork"` with the road's `target_x` /
## `target_y`, and the road row's identity is that same pair — the retired `RouteId` having gone with
## the stored path — so the two lists meet on one spelling and no id has to be invented.
##
## **ONE PREDICATE, TWO SURFACES.** The tile card's `Road` row and the BUILD QUEUE block's road row
## both ask it, so the card cannot say a road is un-queued while the queue is drawing a row for it.
static func is_queued(road: Dictionary, queued_tiles: Dictionary) -> bool:
	return queued_tiles.has(tile_of(road))

## **THE QUEUE ROW'S NAME FOR A ROAD — the rung it is being RAISED TO, and the tile.** A patch and a
## road can be queued on one hex, so the coordinates alone would draw two rows a player cannot tell
## apart; the rung is what says which job this is. `HudWorkVocab.BUILD_QUEUE_PLANT_FACE_FORMAT`'s
## shape, one branch over, so the three faces read as one list.
const ROAD_QUEUE_FACE_FORMAT := "%s (%d, %d)"

## `Road:` — **the rung this road HOLDS**, plus what traffic is wearing in above it and the branch's
## hazard word where its upkeep is short, as middot clauses in that order.
##
## ⛔ **THE RUNG IS THE VALUE AND EVERYTHING ELSE IS A QUALIFIER.** A path at 25% is a COMPLETE
## path a quarter of the way to a trail; the row this replaced said `Trail 25%` on a second
## `Wearing in` row, which reads as a road that is 25% built and is the one thing this row must never
## be readable as.
static func road_row_value(road: Dictionary, queued_tiles: Dictionary = {}) -> String:
	var clauses: Array[String] = [rung_label(rung_of(road))]
	# **ONE CLAUSE, ONE SPELLING, AND THE DECLARATION IS WHAT OPENS IT AT ZERO** — see
	# `ROAD_METER_UNSTARTED`. The queued road's `0% to dirt road` and the working road's
	# `12% to dirt road` are the same sentence about the same climb, which is what stops the card
	# needing a second phrasing that then has to be told apart from the first.
	var progress := progress_clause(road, is_queued(road, queued_tiles))
	if progress != "":
		clauses.append(progress)
	if is_short(road):
		clauses.append(ROAD_HAZARD_CLAUSE_FORMAT % [
			HudSelectionVocab.RUNG_HAZARD_GLYPH, ROAD_UNDER_KEPT_WORD])
	return ROAD_CLAUSE_SEPARATOR.join(clauses)

## ⛔ **THE PAYOFF, UNLABELLED — and it is now ONE CLAUSE, the loss figure.**
##
## The row printed all three axes (`40% less lost hauling · lights its tiles · links 10 tiles out`),
## which on a 292px card wrapped to three lines under a value that is itself one word. Reported from
## play: *"the description under Road: trail is way too much, maybe just 15% less loss is enough, the
## rest can be a tool tip."*
##
## **THE LOSS FIGURE IS THE ONE THAT MOVES A DECISION** — it is the reason to pave rather than to
## leave a trail — so it stays on the card and the other two go to `bonus_tooltip` below. Nothing is
## lost: the block's hover carries them, through the same `Context.row_tooltips` seam the rung rows'
## own hazards use.
##
## ⛔ **A RUNG THAT SAVES NOTHING RENDERS NO ROW.** `friction_multiplier` at its neutral is both free
## rungs, and the row's absence states that better than a sentence could.
static func bonus_value(road: Dictionary) -> String:
	var multiplier := friction_multiplier_of(road)
	if multiplier >= ROAD_FRICTION_NO_HELP:
		return ""
	return ROAD_BONUS_FRICTION_FORMAT % int(round(
		(ROAD_FRICTION_NO_HELP - multiplier) * ROAD_PERCENT_SCALE))

## …and the rest of the payoff, for the block's HOVER — what the row above stopped saying.
##
## - whether the road is lighting its tiles right now (`grants_sight`, the RESOLVED answer, because a
##   client cannot re-derive *is the bill met*), and, on a BUILT road whose bill is unpaid, that it
##   has gone dark — which happens BEFORE the rung decays and is the honest early warning;
## - the link span the rung holds open (`holds_link_to_tiles`), a **live** effect since slice 13b:
##   the pooling pass joins two camps at `max(reach_tiles, the weakest tile of the run between them)`.
##
## `""` where there is nothing further to say, which leaves the hover empty rather than blank-lined.
static func bonus_tooltip(road: Dictionary) -> String:
	var clauses: Array[String] = []
	if grants_sight(road):
		clauses.append(ROAD_BONUS_SIGHT)
	elif is_short(road):
		# A road only goes dark for a reason, and the reason is the unpaid bill one row up. Gated on
		# the shortfall rather than printed for every unlit road, because the PATH lights nothing even
		# with its bill paid in full — a rung nobody keeps is not a road going dark — and telling the
		# player to pay a bill that does not exist would be a wrong remedy.
		clauses.append(ROAD_BONUS_DARK)
	var span := holds_link_to_tiles_of(road)
	if span > ROAD_LINK_NONE:
		clauses.append(ROAD_BONUS_LINK_FORMAT % span)
	return ROAD_CLAUSE_SEPARATOR.join(clauses)

## `Upkeep:` — the bill, the shortfall and the keeper count, every figure straight off the wire.
## **The shortfall is the SIM'S field, never `demand − supplied`**: all three read one stamped basis,
## and this branch has shipped that defect twice.
##
## ⛔ **`""` — NO ROW — WHERE THE ROAD OWES NOTHING.** Both free rungs declare no upkeep at all, and a
## sentence saying so (`free — nobody keeps a path`) was a row spent on the absence of a bill,
## on every road a current game can contain.
static func upkeep_value(road: Dictionary, label: String = "",
		per_worker_turn: float = RUNG_CATALOG_NO_BUILD_RATE) -> String:
	if not owes_keeping(road):
		return ""
	if not has_keeper(road):
		return ROAD_KEEPER_NOBODY
	var named := label.strip_edges()
	var face: String = ROAD_UPKEEP_FORMAT % (named if named != "" else ROAD_KEEPER_FOREIGN)
	if not is_short(road):
		return face
	# ⛔ **NO RATE, NO CLAUSE — AND THE NAME STILL STANDS.** A catalog this client has not been sent
	# leaves the gap unpriceable in hands, and `(short 0 workers)` would read as a bill that is met.
	# The keeper is still on the hook and is still named, which is the row's whole subject.
	var short := workers_short_of(road, per_worker_turn)
	if short <= SourceForecast.BUILD_CREW_NONE:
		return face
	return ROAD_UPKEEP_SHORT_FORMAT % [face, ROAD_UPKEEP_SHORT_MARK, short,
		ROAD_UPKEEP_WORKER_SINGULAR if short == ROAD_ONE_WORKER else ROAD_UPKEEP_WORKER_PLURAL]

## ⛔ **HOW MANY HANDS THE GAP IS WORTH — off `upkeepShortfall` and NOTHING ELSE.**
## `ceil(shortfall / per_worker_turn)`, the same arithmetic the sim runs to publish
## `upkeepWorkersNeeded` from the demand, so the row's *short N* and the hover's *N keepers* are two
## readings of one rate rather than two derivations.
##
## ⛔ **THE RATE IS THE BRANCH'S, PUBLISHED, AND THE CALLER RESOLVES IT.** It is
## `RouteRungState.buildWorkPerWorkerTurn` off the rung catalog
## (`branch_build_work_per_worker_turn`), threaded in exactly as `keeper_label` is and for the
## identical reason: this leaf holds no catalog and joining road→catalog here would give it one.
## `SourceForecast.BUILD_CREW_NONE` where no rate has arrived — a state, not a failure, and the row
## then states the keeper without a gap clause.
##
## **NEVER `wants − assigned`.** `roadwork` is a band-wide POOL and its share of any one road is the
## sim's answer; the client holds no per-road head count to subtract from, and inventing one is the
## defect this branch has already shipped twice.
##
## **THE FLOOR IS ONE, ONCE THERE IS A RATE AT ALL.** A road that is short is short by at least one
## whole hand — a `0` beside the word *short* would read as a bill that is met — but that floor may
## not manufacture an answer where there is no rate to strike one against.
static func workers_short_of(road: Dictionary,
		per_worker_turn: float = RUNG_CATALOG_NO_BUILD_RATE) -> int:
	var hands := SourceForecast.workers_for_work(upkeep_shortfall_of(road), per_worker_turn)
	if hands <= SourceForecast.BUILD_CREW_NONE:
		return SourceForecast.BUILD_CREW_NONE
	return maxi(hands, ROAD_ONE_WORKER)

## …and the figures, for the block's HOVER: the exact bill, the keepers it wants, and — where the sim
## quoted one — what distance is charging the keeper for it. **Every number is published**; the client
## multiplies nothing and subtracts nothing.
##
## `""` on a road that owes nothing, which is both free rungs and leaves the hover empty rather than
## blank-lined.
static func upkeep_tooltip(road: Dictionary) -> String:
	if not owes_keeping(road):
		return ""
	var wants := upkeep_workers_needed_of(road)
	var clauses: Array[String] = [ROAD_UPKEEP_TIP_FORMAT % [
		DetailFormat.format_work_units(upkeep_demand_of(road)), wants,
		"" if wants == ROAD_ONE_WORKER else ROAD_UPKEEP_PLURAL_SUFFIX]]
	if is_remote(road):
		clauses.append(ROAD_KEEPER_REMOTE_FORMAT % (
			ROAD_REMOTENESS_FORMAT % keeper_remoteness_of(road)))
	return ROAD_CLAUSE_SEPARATOR.join(clauses)

# ⛔ **RETIRED — `keeper_value`, the `Kept by:` ROW'S COMPOSER.** Its three readings are all still
# stated and all on the row above: a named keeper and a stranger are `upkeep_value`'s own face, and a
# road that owes a bill nobody pays is that composer's first branch. The remoteness clause is the one
# thing that moved surface rather than composer — it is on `upkeep_tooltip` now.

## `Reverting:` — the countdown, or `""` where nothing is at risk. **The countdown, not the counter**:
## `0` is *it is reverting now*, and a road whose bill is met reads its rung's full grace + 1, which
## is why this row renders only while the road is actually short.
static func reverting_value(road: Dictionary) -> String:
	if not is_at_risk(road):
		return ""
	var left := neglect_grace_remaining_of(road)
	if left <= ROAD_REVERTING_IMMINENT:
		return ROAD_REVERTING_NOW % HudSelectionVocab.RUNG_HAZARD_GLYPH
	if left == 1:
		return ROAD_REVERTING_ONE % HudSelectionVocab.RUNG_HAZARD_GLYPH
	return ROAD_REVERTING_FORMAT % [HudSelectionVocab.RUNG_HAZARD_GLYPH, left]

## **THE WHOLE ROAD BLOCK FOR ONE ROAD**, as `Key: value` detail lines. One composer, so the tile
## card and any later surface state one road the same way.
##
## ⛔ **ONLY THE RUNG ROW IS UNCONDITIONAL, AND A FREE PATH IS ONE LINE.** Every other row is
## emitted iff its composer has something to say — the payoff iff the rung buys something, the bill
## iff the road owes something, the keeper iff there is a job, the countdown iff the rung is at risk.
## The block used to be five rows on every road in the world, two of them prose saying *no*.
##
## The rows in order: what it is · what it buys · what it costs · whose job it is · when it is lost.
## The payoff sits directly under the rung because it is what the rung IS FOR, and the keeper follows
## the bill because it is the answer to *who pays that*.
## `ctx` is the render's tint/hover context, and the payoff row's SPARE CLAUSES are registered on it
## (`Context.row_tooltips`) rather than printed. A caller with none — the harness asserting the WORDS,
## and any surface with no host label — simply gets the visible lines, which is the honest half.
## `per_worker_turn` is the BRANCH's bare work rate, resolved by the caller from the rung catalog
## (`branch_build_work_per_worker_turn`) — threaded in for `keeper_label`'s own reason: this leaf holds
## no catalog, and a road→catalog join here would give it one. `RUNG_CATALOG_NO_BUILD_RATE` is a legal
## reading and means *the shortfall cannot be stated in hands*, which the row answers by not stating it.
## `queued_tiles` is the set of road tiles the player's bands have QUEUED, threaded in for
## `keeper_label`'s own reason — the leaf holds no queue, and the road→queue join lives at the call
## site. `{}` is a legal reading and means *nothing is queued*, which the rung row answers by stating
## no approach clause rather than a `0%`.
static func road_lines(road: Dictionary, keeper_label: String = "",
		ctx: DetailFormat.Context = null,
		per_worker_turn: float = RUNG_CATALOG_NO_BUILD_RATE,
		queued_tiles: Dictionary = {}) -> Array[String]:
	var lines: Array[String] = []
	lines.append("%s: %s" % [ROAD_ROW, road_row_value(road, queued_tiles)])
	var bonus := bonus_value(road)
	if bonus != "":
		lines.append("%s: %s" % [ROAD_BONUS_ROW, bonus])
	# **THE REST OF THE PAYOFF GOES TO THE BLOCK'S HOVER**, keyed on the row that stopped saying it.
	# `DetailFormat.block_tooltip` joins every registered hover into the one `tooltip_text` a
	# `RichTextLabel` block can carry, `[hint=…]` not being parsed by this Godot build.
	if ctx != null:
		var spare := bonus_tooltip(road)
		if spare != "":
			ctx.row_tooltips[ROAD_BONUS_ROW] = spare
	# ⛔ **ONE ROW FOR THE STANDING COST, AND IT NAMES THE BAND.** The bill and the keeper were two
	# rows and three figures; the row states the decision now (*whose job, and are they covering it*)
	# and the figures ride the hover beside the payoff's.
	var upkeep := upkeep_value(road, keeper_label, per_worker_turn)
	if upkeep != "":
		lines.append("%s: %s" % [ROAD_UPKEEP_ROW, upkeep])
		if ctx != null:
			var bill := upkeep_tooltip(road)
			if bill != "":
				ctx.row_tooltips[ROAD_UPKEEP_ROW] = bill
	var reverting := reverting_value(road)
	if reverting != "":
		lines.append("%s: %s" % [ROAD_REVERTING_ROW, reverting])
	return lines

## The `Road:` row's ink — the shared rung palette, so a road at risk reads in the same amber a
## slipping patch does and a kept one in the same signal a built rung does.
static func road_value_hex(value: String) -> String:
	if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
		return HudStyle.WARN_HEX
	return HudStyle.SIGNAL_HEX

## The payoff row's ink. **SIGNAL, unconditionally, and it takes no value** — the row is emitted only
## where the rung buys something (`bonus_value` returns `""` otherwise), so there is no second reading
## for it to fork on. It is the one row on the card that states a PAYOFF, and the branch fails as a
## ladder if it does not stand out from the cost below it.
static func bonus_value_hex() -> String:
	return HudStyle.SIGNAL_HEX

## The `Upkeep:` / `Reverting:` ink. Amber where the bill is unmet, plain ink otherwise —
## `ecology_value_hex`'s shape, keyed on the hazard mark the composers above put there rather than on
## a list of known sentences. **The road's `Upkeep` row shares its key with the BAND's material bill**
## (`HudDisclosureVocab.DETAIL_ROW_UPKEEP` — one word for one concept), so `DetailFormat._value_hex`
## reaches this only after the band's runway tint has declined the row, and a band bill carries no
## hazard mark and therefore reads the same plain ink it always did.
static func upkeep_value_hex(value: String) -> String:
	if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
		return HudStyle.WARN_HEX
	# ⛔ **THE ROAD'S BILL CARRIES NO GLYPH ANY MORE, so its two alarming readings are named.** The row
	# is words now (`Band 3 (short 1 worker)` / `nobody — grade it again to take it on`), and both say
	# the keeping is not being paid — which is the state the amber is for. `ROAD_UPKEEP_SHORT_MARK` is
	# the composer's own spelling, so the words and the tint cannot drift apart.
	if value == ROAD_KEEPER_NOBODY or value.contains(ROAD_UPKEEP_SHORT_MARK):
		return HudStyle.WARN_HEX
	return HudStyle.INK_HEX

# ⛔ **RETIRED — `keeper_value_hex`, the `Kept by:` ROW'S INK.** Both of its readings are on the
# `Upkeep` row now and `upkeep_value_hex` answers for them: a road nobody keeps reads amber there
# (the same news, one row earlier), and a kept road reads plain ink, remoteness included — distance
# is a price, not an alarm, which is why it went to the hover rather than to a tint.

# ---- THE ROUTE RUNG CATALOG (`SubsistenceSection.routeRungs`) ----------------------------------
#
# ⛔ **THE UNIT THE PLAYER PRESSES IS THE LADDER, NOT THE VERB, AND THIS CATALOG IS WHAT MAKES THAT
# FREE.** One row per rung of the route branch, published ONCE PER SNAPSHOT beside `ladderKnowledge`
# — every rung's name, its price, what it buys and what gates it, all resolved sim-side out of
# `intensification_ladder.json`. A rung added to that config therefore appears in the ladder sheet
# **with no client edit at all**, which is the whole reason the sheet is not a row of buttons.
#
# ⛔ **THE SHEET MUST NOT READ `RUNG_LABELS` FOR ITS ROW NAMES.** That table exists so the tile
# card's readout can name a rung the wire sends it on a ROAD ROW; it is a hard-coded four and would
# silently render a fifth rung as its raw wire key. The sheet names every row from
# `catalog_display_name`, which is the sim's own word for it.
#
# The catalog is per WORLD and carries no faction and no tile — exactly `ladder_knowledge`'s shape and
# for its reason. What a PARTICULAR road stands at is the `routes` row above; the two are joined on
# the rung key.

## One catalog row's fields, spelled once. A typo in a `get` here is a silent zero, and on the price
## side a silent zero would advertise a free road.
const RUNG_CATALOG_KEY := "rung_key"
const RUNG_CATALOG_ORDER := "order"
const RUNG_CATALOG_DISPLAY_NAME := "display_name"
const RUNG_CATALOG_VERB := "verb"
const RUNG_CATALOG_UNLOCK_KNOWLEDGE := "unlock_knowledge"
const RUNG_CATALOG_REQUIRES_RUNG := "requires_rung"
const RUNG_CATALOG_WORK_COST := "work_cost"
const RUNG_CATALOG_UPKEEP := "upkeep_work_per_turn"
const RUNG_CATALOG_FRICTION := "friction_multiplier"
const RUNG_CATALOG_LINK := "holds_link_to_tiles"
const RUNG_CATALOG_GRANTS_SIGHT := "grants_sight"
## ⛔ **WHAT STANDING ON THIS RUNG TEACHES — the gate's REMEDY, and the reason it is a wire field.**
## `unlock_knowledge` says what a rung WAITS ON; this says what a rung EARNS, and they are different
## rungs. See `ladder_rung_teaching` for why the pairing may not be inferred.
const RUNG_CATALOG_EARNS_KNOWLEDGE := "earns_knowledge"

## ⛔ **WHAT ONE BARE-HANDED WORKER BANKS IN A TURN, AND IT IS THE SIM'S RATE RATHER THAN THE RUNG'S.**
## `intensification::PER_WORKER_OUTPUT`, unscaled — before gear and before any multiplier. Every field
## beside it describes ONE rung; this one is identical on every row, and it rides the catalog because
## the catalog is exactly the set of numbers that are the same for every road in the world.
##
## ⛔ **IT REPLACED A TRANSCRIBED CONSTANT, WHICH IS THE POINT OF THE FIELD.** Every SOURCE publishes
## its own `build_work_per_worker_turn`; a road has no source row, so for one slice the client spelled
## `PER_WORKER_OUTPUT` itself — and the sim writes worker output as a SUM OF TERMS, so that copy goes
## stale in silence the day a second term lands. **A missing or `0` reading is answered as *no
## estimate* and *no clause*, never with a rate of the client's own** (`SourceForecast.workers_for_work`,
## `RungLadder._route_turns`).
const RUNG_CATALOG_BUILD_PER_WORKER_TURN := "build_work_per_worker_turn"

## The rate a catalog this client has not been sent reads. **A measured nothing, not a sentinel**: it
## flows into the two readers above as *there is no rate*, which each answers by stating nothing.
const RUNG_CATALOG_NO_BUILD_RATE := 0.0

## ⛔ **THE WIRE'S OWN SPELLING OF *there is none*, AND IT IS A NAMED EMPTY STRING RATHER THAN A
## SENTINEL.** `verb` is `""` on a rung nobody declares, `unlock_knowledge` is `""` on one nothing
## gates, and `requires_rung` is `""` on the floor. All three are real, distinct facts about a rung
## and each reads as its own gate below.
const RUNG_CATALOG_NONE := ""

## A rung nobody pays for — the two free ones, worn in by traffic. Named because it is the TEST the
## verbless rungs' face is gated on, not a rounding tolerance: a `0 work` price would read as a
## defect on a row that has no price to state at all.
const RUNG_CATALOG_NO_WORK_COST := 0.0

## The order a rung with no published position falls to. **Below the floor**, so a row this client
## cannot place sorts to the bottom of the ladder rather than silently ahead of the rung it needs.
const RUNG_CATALOG_NO_ORDER := -1

# ---- CATALOG READERS --------------------------------------------------------------------------

static func catalog_rung_key(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_KEY, "")).strip_edges()

static func catalog_order(entry: Dictionary) -> int:
	return int(entry.get(RUNG_CATALOG_ORDER, RUNG_CATALOG_NO_ORDER))

## The sim's own word for this rung. **The raw wire key where the catalog carries none**, which is the
## honest answer rather than a blank row — the same rule `rung_label` follows one section up.
static func catalog_display_name(entry: Dictionary) -> String:
	var name := String(entry.get(RUNG_CATALOG_DISPLAY_NAME, "")).strip_edges()
	return name if name != "" else catalog_rung_key(entry)

## ⛔ **`""` MEANS NOBODY DECLARES THIS RUNG — it is worn in by traffic.** It is a state, not a
## missing field: both free rungs carry it, and they are the commonest road in the game.
static func catalog_verb(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_VERB, RUNG_CATALOG_NONE)).strip_edges()

static func catalog_unlock_knowledge(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_UNLOCK_KNOWLEDGE, RUNG_CATALOG_NONE)).strip_edges()

## The rung this one has to stand on to be raised — `""` for the floor. ⛔ **This is why a road cannot
## be built on bare ground**: `dirt_road` requires `trail`, and a trail is reached only by traffic, so
## roads are upgraded where people already walk.
static func catalog_requires_rung(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_REQUIRES_RUNG, RUNG_CATALOG_NONE)).strip_edges()

## ⛔ **THE RUNG'S BASE PRICE, QUOTED AS PUBLISHED.** Remoteness multiplies it sim-side and is stated
## as its OWN clause beside this figure — see `ROAD_LADDER_REMOTE_FORMAT`. The client never multiplies
## the two, because that would put a copy of the sim's pricing formula where it can drift.
static func catalog_work_cost(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_WORK_COST, RUNG_CATALOG_NO_WORK_COST))

static func catalog_upkeep(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_UPKEEP, SourceForecast.NO_UPKEEP_DEMAND))

## One catalog row's bare per-worker work rate — the reader for a caller that already holds an entry
## (`RungLadder._route_turns`, pricing one rung).
static func catalog_build_work_per_worker_turn(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_BUILD_PER_WORKER_TURN, RUNG_CATALOG_NO_BUILD_RATE))

## ⛔ **THE BRANCH'S RATE, OFF THE CATALOG'S FIRST ROW — and reading one row is HONEST here rather
## than sloppy.**
##
## **The rate is one number for the whole branch, not a per-rung figure.** It is the sim's own worker
## output (`intensification::PER_WORKER_OUTPUT`), identical on every row, and
## `core_sim/tests/route_wire.rs` asserts exactly that — every rung publishes the same value. It rides
## the per-rung table only because the catalog is the wire structure the client already has in hand;
## a reader that walked all four rows looking for agreement would be checking the sim's own test.
##
## **It exists for the callers that hold a ROAD but no rung** — the tile card's `Upkeep` row, which
## needs the rate to state a shortfall in workers and has no reason to know which rung the road is
## climbing toward.
##
## `RUNG_CATALOG_NO_BUILD_RATE` on an empty catalog, which is every frame before the first snapshot
## and every fixture that stages no ladder: the callers state no clause there rather than inventing
## one.
static func branch_build_work_per_worker_turn(ladder: Array[Dictionary]) -> float:
	if ladder.is_empty():
		return RUNG_CATALOG_NO_BUILD_RATE
	return catalog_build_work_per_worker_turn(ladder[0])

static func catalog_friction(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_FRICTION, ROAD_FRICTION_NO_HELP))

static func catalog_link_span(entry: Dictionary) -> int:
	return int(entry.get(RUNG_CATALOG_LINK, ROAD_LINK_NONE))

static func catalog_grants_sight(entry: Dictionary) -> bool:
	return bool(entry.get(RUNG_CATALOG_GRANTS_SIGHT, false))

## ⛔ **`""` MEANS THIS RUNG TEACHES NOTHING** — the branch's floor, and its top, which has nothing
## above it to open. A state, not a missing field.
static func catalog_earns_knowledge(entry: Dictionary) -> String:
	return String(entry.get(RUNG_CATALOG_EARNS_KNOWLEDGE, RUNG_CATALOG_NONE)).strip_edges()

## **THE WHOLE BRANCH, BOTTOM RUNG FIRST**, as typed rows — the wire's `order` is the climb order and
## this is the one place it is applied. Non-Dictionary entries are dropped rather than defaulted: a
## row this client cannot read has no rung key to join a road on.
##
## `[]` before any snapshot has arrived, which every caller renders as *no ladder to show* rather than
## as a branch with nothing on it.
static func route_ladder(catalog: Array) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for entry_variant in catalog:
		if entry_variant is Dictionary:
			rows.append(entry_variant as Dictionary)
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return catalog_order(a) < catalog_order(b))
	return rows

## Where a rung sits in the branch — `RUNG_CATALOG_NO_ORDER` for one the catalog does not carry, which
## sorts it below the floor and so above nothing.
static func ladder_order_of(ladder: Array[Dictionary], rung_key: String) -> int:
	for entry in ladder:
		if catalog_rung_key(entry) == rung_key:
			return catalog_order(entry)
	return RUNG_CATALOG_NO_ORDER

## One rung's name as the LADDER SHEET says it — the catalog's own word. The raw key for a rung the
## catalog does not carry, which is what a road standing on an unknown rung reads as.
static func ladder_rung_name(ladder: Array[Dictionary], rung_key: String) -> String:
	for entry in ladder:
		if catalog_rung_key(entry) == rung_key:
			return catalog_display_name(entry)
	return rung_key

## One rung's whole catalog row, `{}` for a rung the catalog does not carry (a client that has not
## been sent one, or a rung above the top of the branch). Its callers read the price and the standing
## bill off the row rather than off a second table, so a rung added to the config is priced with no
## client edit.
static func ladder_entry_of(ladder: Array[Dictionary], rung_key: String) -> Dictionary:
	if rung_key == RUNG_CATALOG_NONE:
		return {}
	for entry in ladder:
		if catalog_rung_key(entry) == rung_key:
			return entry
	return {}

## ⛔ **WHICH RUNG TEACHES THIS CRAFT — the gate's remedy, LOOKED UP AND NEVER INFERRED.**
##
## The obvious shortcut is *"the rung beneath the gated one"* (`requires_rung`), and it holds for the
## four rungs that ship today: a trail teaches Roadbuilding, which opens the dirt road above it. **It
## is a coincidence of this config and not a property of the ladder.** `intensification_ladder.json`
## is free to have a rung teach a craft that opens something two rungs up, and the moment it does the
## inference names the WRONG rung — in a REMEDY, which is to say it sends the player to go and stand
## on the wrong ground. That is the one place a wrong answer costs something, so the pairing rides
## the wire (`earns_knowledge`) and this is the lookup.
##
## `""` where no rung on this branch teaches it, which is a real state: a route rung may be gated on a
## craft another branch earns. The caller then states the craft and its progress with no remedy rather
## than naming a rung that does not teach it.
static func ladder_rung_teaching(ladder: Array[Dictionary], knowledge_id: String) -> String:
	if knowledge_id == RUNG_CATALOG_NONE:
		return ""
	for entry in ladder:
		if catalog_earns_knowledge(entry) == knowledge_id:
			return catalog_display_name(entry)
	return ""

# ---- THE LADDER SHEET'S OWN WORDS --------------------------------------------------------------
#
# ⛔ **THE ACTION IS NAMED FOR THE BRANCH, NEVER FOR A VERB.** One button per verb does not scale —
# highways and railways are rungs, not new controls — and a single verb-named button forces ONE
# refusal string, which cannot answer *"paving is out of reach but railroad is not"*. The action opens
# the whole branch and every rung carries its own price and its own gate.

## The tile card's road action. `Road` is the branch's noun and matches the readout row's key one
## section up, so the control and the row it acts on are called the same thing. The chevron is the
## card's own *opens something* mark, as `HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT` wears it.
const ROAD_LADDER_ACTION_LABEL := "Road ▸"

## …and the two stable HANDLES a harness reaches those nodes by. **Never by face text**: the action's
## label is one word a future rung could change, and the card carries no text of its own at all.
const ROAD_LADDER_ACTION_META := &"road_ladder_action"
const ROAD_LADDER_META := &"road_ladder"
## …and the abandon control's own handle, for the same reason: its face is a sentence and a harness
## that matched on the words would break the day the sentence is reworded.
const ROAD_LADDER_ABANDON_META := &"road_ladder_abandon"

## …and the sheet's heading, in `HudWorkVocab.RUNG_TRACK_TITLE`'s register because it is the same kind
## of card one branch over. **`RAISE`, not `TAKE`** — a road is not carried anywhere, and the plant
## branch's verb read as movement on a surface whose whole subject is a piece of ground.
const ROAD_LADDER_TITLE := "RAISE IT TO…"

# ---- THE LADDER'S ROWS: ONE LINE EACH, AND THE REST ONE HOVER AWAY ------------------------------
#
# ⛔ **A RUNG IS ONE LINE.** The card printed up to SIX per rung — a state word, a price aside, an
# approach, a remoteness clause, a payoff and a standing bill, plus one wrapped sentence per unmet
# gate. Reported from play as *"the most wordy dialog I think I've ever seen"*, and the diagnosis was
# right: none of it was wrong, all of it was at once, and the decision the row exists for (*can I
# afford this yet*) was the hardest thing on it to find.
#
# **NOTHING IS DELETED, IT MOVES TO THE ROW'S `tooltip_text`.** The payoff, the standing bill, the
# prerequisite chain and the *how do I learn this* sentence all survive one hover away. A cut that
# loses the detail instead of relocating it is the failure this arrangement is built to avoid, which
# is why the harness asserts the tooltips and not only the visible line.
#
# The face is `<figure> · <nearest refusal>`; where the rung is buildable the figure IS the button
# and there is no refusal to state.

## The row's two-clause face. **The figure leads** because it is what two rows of one ladder differ
## by most, and because the eye that came here for a price should not have to read past a sentence.
const ROAD_LADDER_FACE_FORMAT := "%s · %s"

## The meter on the row DIRECTLY ABOVE the standing rung, and on no other — `build_fraction` is the
## rung being RAISED. **It rides the `wearing in` row alone**: that row has no price of its own, so
## without it the line states only a static fact about traffic, while every other row already leads
## with a figure.
const ROAD_LADDER_METER_FORMAT := "%d%%"

# ⛔ RETIRED — **`ROAD_LADDER_BUYS_FORMAT`, `ROAD_LADDER_PRICE_FORMAT` and
# `ROAD_LADDER_PROGRESS_FORMAT`**, the three ASIDES a ladder row used to stack beneath itself
# (`buys …`, `costs 110 work`, `35% done`). The first is in the tooltip now, the second is the face,
# the third is a clause of the face. **The word `buys` went with them** — a payoff already reads as a
# benefit, so the label only narrowed the column the sentence had to live in.

## ⛔ **WHAT DISTANCE DOES TO THE PRICE — tooltip only now.** The row's figure is the rung's BASE
## price exactly as published and the multiple is stated apart from it, because multiplying the two
## here would put a copy of the sim's pricing formula in the client, where it can drift.
const ROAD_LADDER_TIP_REMOTE_FORMAT := "Far from your band, so it costs ×%s."

## **THE PRICE PAIRED WITH THE STANDING BILL, which is the decision** — one-off against per-turn, the
## two halves of what the player is agreeing to.
##
## ⛔ **RENDERED ONLY WHERE THE RUNG OWES UPKEEP.** On a rung that is free to hold the line would
## restate the face and add nothing, and a line that says nothing is the whole of what this cut
## removed.
const ROAD_LADDER_TIP_PRICE_FORMAT := "%s work to build, %s work a turn to keep."

## **HOW LONG, AT THE ACTING BAND'S OWN `builders` POOL.** `Dirt Road — 110 work` with one builder is
## ~110 turns, and nothing on the card said so; every other build surface in the game states a turns
## estimate. **It names the BAND because the answer is the band's** — a different pool is a different
## number, which is why the estimate re-renders with the picker above it.
##
## ⛔ **ON A BUILDABLE ROW IT RIDES THE FACE, and on a refused one it rides the HOVER.** The face holds
## one clause beside the price, and on a refused rung that clause is the refusal — a row reading
## `110 work · ≈110 turns · needs Roadbuilding` is the wordiness this card was cut down from.
const ROAD_LADDER_TIP_TURNS_FORMAT := "%s with this band's builders."

# ---- WHERE THE PRESS LANDS — the queue, said at the moment of the decision ----------------------
#
# ⛔ **THE PRESS QUEUES A JOB AND THE CARD NEVER SAID SO.** Reported from play: a graded tile showed
# nothing anywhere, and the ladder had not warned that the press was a DECLARATION taking its place in
# a line rather than work starting on the spot. Ray: *"it isn't obvious that the road will show up in
# the build queue, so we need something to indicate that when the job is selected."*
#
# ⛔ **AND THE TURNS ESTIMATE IMPLIED IT STARTS NOW.** `110 work · ≈37 turns` silently assumes the
# builders are free; in the reported game they were on a Tame until turn 132. The figure is RIGHT and
# it is not a completion date — so it is kept, and where anything is ahead of this road the row says
# what the number is measured from.

## An empty queue: the press really does start it, and saying so is what makes the other reading
## legible when it appears.
const ROAD_LADDER_QUEUE_EMPTY_ASIDE := "joins this band's build queue, and starts now"

## …and a queue with something in it. **The HEAD is named because it is the whole question** — the
## head takes every builder, so it alone decides when this road starts — and the rest are a count,
## which is what a player reorders against.
const ROAD_LADDER_QUEUE_BEHIND_FORMAT := "joins this band's build queue behind %s%s — %s"
const ROAD_LADDER_QUEUE_MORE_FORMAT := " and %d more"

## …and the qualifier on the estimate beside it. **It names what the count is measured FROM rather
## than hedging it**: the number is exact once the builders reach this entry, and a raid on the figure
## itself would throw away the one thing the row can tell the player about the price of the job.
const ROAD_LADDER_QUEUE_ESTIMATE_NOTE := "the estimate runs from when it starts"

## The queue model the ladder is handed — how many entries stand ahead of a press, and what the head
## is called. Named keys because a bare pair is two ints a caller can hand over the wrong way round.
const ROAD_LADDER_QUEUE_AHEAD_KEY := "ahead"
const ROAD_LADDER_QUEUE_HEAD_KEY := "head"

## ⛔ **ZERO BUILDERS IS AN ANSWER, NOT AN ABSENCE.** With nobody on the pool there is no estimate to
## state and the honest thing is the REMEDY, in the ladder's own aside shape — a blank column where
## every other row states a duration reads as a client that failed to work it out.
const ROAD_LADDER_NO_BUILDERS_ASIDE := "nobody on builders — staff the pool and it starts"

## The tooltip's lines, one fact each. **Newline rather than a middot**, because these are sentences
## and a hover has the room a 292px row does not.
const ROAD_LADDER_TIP_SEPARATOR := "\n"

# ---- PUTTING A ROAD DOWN — the undo a BUILT road had nowhere else ------------------------------
#
# ⛔ **`unqueue` IS NOT ENOUGH, AND THAT IS WHY THIS ROW EXISTS.** It withdraws a DECLARATION; the
# moment any work is banked the verb that releases a keeper is `abandon`, which was command-line
# only — so a road handed to the wrong band could not be taken back from the UI at all. With the band
# picker above making the keeper a real choice, this is what makes it a reversible one.

## The control, at the BOTTOM of the card — under the ladder, because putting a road down is the
## opposite end of the same decision the rows above it offer. **It names the ACT, not the verb**: the
## wire word is `abandon`, and *abandon* reads as walking away from the ground rather than as handing
## back a job.
const ROAD_LADDER_ABANDON_LABEL := "Stop keeping this road"

## …and what it actually does, on the control's own hover. **The rung STAYS** — the sim leaves the
## meter alone and the ground rots back down at the rung's own rate with nobody keeping it — which is
## the half a player must know before pressing, since it is not a demolition.
const ROAD_LADDER_ABANDON_TOOLTIP := "Drops this band's keeping of the road and its place in the " \
	+ "queue. The rung stays, and with nobody keeping it rots back down at its own rate."

## ⛔ **AND WHERE THE HEX ALSO CARRIES WORK OF YOURS, THE ROW SAYS SO.** `abandon <faction> <x> <y>`
## names a PLACE: `handle_abandon` drops the faction's labor rows on that tile as well as the road's
## keeper, because a tile may carry a road and a patch and putting one down without the other would be
## silently partial. **There is no road-only abandon** — the sim has no such verb — so the honest thing
## is to state the consequence rather than to narrow the command.
const ROAD_LADDER_ABANDON_ALSO := "…and the foraging your people do on this hex goes down with it"

## ⛔ **THE NAME COLUMN IS NARROWER ON THIS BRANCH, and the wrapping is why.** The shared
## `HudWorkVocab.RUNG_TRACK_NAME_WIDTH` (150px) leaves 142px of a 292px card for the value, and
## `110 work · needs Roadbuilding` does not fit in it — the row clipped, which was half of why the
## card read badly. Route rung names are short (`Paved Road` is the longest the shipped ladder holds),
## so the column gives the width back to the face. It rides the ROW rather than widening
## `build_track`'s signature: the plant and animal tracks want the wider name column they have.
const ROAD_LADDER_NAME_WIDTH := 96.0

# ---- THE GATES: A SHORT FORM FOR THE ROW, A SENTENCE FOR THE HOVER -----------------------------
#
# ⛔ **A GATED ROW IS SHOWN AND EXPLAINED, NEVER HIDDEN.** The sheet exists to say what the branch
# HOLDS; a rung silently missing reads as a shorter ladder rather than as one this road cannot climb.
#
# ⛔ **AND THE WORD `locked` IS GONE.** A row reading `locked` above a reason said it twice — the
# reason alone IS the state. The row stays visibly disabled by its ink (`INK_DIM`) and by being a
# `Label` rather than a `Button`, which is this client's standing rule for the improvement control.
#
# ⛔ **THE ROW SHOWS ONE GATE AND THE TOOLTIP SHOWS THEM ALL.** A rung two steps out of reach printed
# every refusal it had, which was most of the six lines. Each gate therefore ships a SHORT form (a row
# clause, lower case, no full stop) and a LONG form (a sentence for the hover).

## The gate KINDS, in the order a ROW prefers them — first match wins, and the tooltip keeps the rest.
##
## ⛔ **THE GROUND GATE SINKS TO LAST, and that is not an ordering whim.** *Needs a trail first* names
## a rung the ladder is already displaying two lines up under `where you are` — it is the one refusal
## the player cannot miss, so it earns least on a line that holds one clause. Everything above it
## names something that is NOT on screen.
##
## The two keeper gates outrank the craft because no amount of learning helps a tile that is already
## somebody else's job; and `pick a band` outranks the craft because it is the only gate on this card
## the player closes with a click rather than with a campaign.
const GATE_KIND_WORN_IN := "worn_in"
const GATE_KIND_ANOTHER_KEEPER := "another_keeper"
const GATE_KIND_NO_KEEPER := "no_keeper"
const GATE_KIND_CRAFT := "craft"
const GATE_KIND_GROUND := "ground"
const GATE_ROW_PRIORITY := [
	GATE_KIND_WORN_IN,
	GATE_KIND_ANOTHER_KEEPER,
	GATE_KIND_NO_KEEPER,
	GATE_KIND_CRAFT,
	GATE_KIND_GROUND,
]

## The three fields one gate carries. **Named**, because producer and reader are different scripts and
## a typo in a `get` here is a silently blank row rather than an error.
const GATE_KIND_KEY := "kind"
const GATE_SHORT_KEY := "short"
const GATE_LONG_KEY := "long"

## The GROUND gate — `requires_rung`. A road cannot be built on bare ground because of it: a dirt road
## wants a trail beneath it and a trail is worn in only by traffic.
const GATE_SHORT_ROAD_NEEDS_RUNG_FORMAT := "needs a %s"
const GATE_LONG_ROAD_NEEDS_RUNG_FORMAT := "Needs a %s first."

## The rung nobody declares. It is not refused for want of anything; there is simply no order to give.
## **The row's own word is `HudWorkVocab.RUNG_TRACK_STATE_WORN_IN`** — one spelling, in the state table
## with the other six — so this carries the hover's sentence alone.
const GATE_LONG_ROAD_WORN_IN := "Traffic wears this in. There is nothing to order."

## The CRAFT gate. **The discovery is named from the ladder's own knowledge roster**, so a rung added
## to `intensification_ladder.json` names its unlock with no client edit; a roster carrying no name for
## it yet says so plainly rather than printing a blank.
const GATE_SHORT_ROAD_NEEDS_CRAFT_FORMAT := "needs %s"
const GATE_SHORT_ROAD_NEEDS_CRAFT_UNNAMED := "needs a craft"

## …and the hover's two halves. The head states what is missing and how far along it is; the remedy
## names the rung that TEACHES it (`ladder_rung_teaching` — never the rung merely beneath, see there)
## and is appended only where a rung on this branch does.
const GATE_LONG_ROAD_KNOWLEDGE_HEAD_FORMAT := "%s known %d%%."
const GATE_LONG_ROAD_KNOWLEDGE_HEAD_UNNAMED_FORMAT := "This craft is known %d%%."
const GATE_LONG_ROAD_KNOWLEDGE_REMEDY_FORMAT := " Learn it from a busy %s."

# ⛔ RETIRED — **`GATE_REASON_ROAD_KNOWLEDGE_FORMAT` and `GATE_REASON_ROAD_KNOWLEDGE_UNNAMED_FORMAT`**,
# the whole-sentence forms an earlier cut replaced. Their remedy clause was fed the rung BENEATH the
# gated one (`requires_rung`), which is a different fact from the rung that TEACHES the craft; the
# sentences they produced on the shipped ladder were byte-identical to the pair above, which is
# precisely why the defect was invisible and why the fixture that catches it states a catalog where
# the two rungs differ.

## ⛔ **THE KEEPER GATE — a road really has to be somebody's job.** The band token IS the keeper:
## issuing the verb declares the work and names who is on the hook for the standing bill, which are one
## act. `Main.IMPROVEMENT_NO_BAND` refuses the command outright rather than guessing a band, so the
## row says so before the press rather than after it.
const GATE_SHORT_ROAD_NO_KEEPER := "pick a band"
const GATE_LONG_ROAD_NO_KEEPER := "Pick a band first. Whoever builds a road keeps it."

## ⛔ **AND ITS TWIN — THE TILE IS ALREADY SOMEBODY ELSE'S JOB.** `road_verb_refusal` rejects
## `grade`/`pave` outright when `Road::keeper` names a band other than the one issuing it: **ONE BAND
## KEEPS A ROAD TILE, NEVER TWO**, and the refusal is what makes co-payment unrepresentable rather
## than merely discouraged. Without this gate the row rendered READY, the press went out, and the
## player got a command-failure event where a greyed row with a reason belonged.
##
## **The keeper is NAMED, and the name is resolved by the caller** — a road carries a `band_id` and
## this client has exactly one band-naming rule (`HudBandLaborState.band_label_for_id`), so the label
## is threaded in exactly as the tile card's `Kept by:` row threads it. A band outside the player's
## roster reads `ROAD_KEEPER_FOREIGN`, which is a real state: a road may be kept by a people you
## merely know of.
##
## **The remedy is the sim's own** — the keeping band has to put the tile down before another may take
## it on. It deliberately does NOT name `abandon` as a control, that verb being command-line only in
## this slice; what the clause promises is the ACT, which is true however it is reached.
const GATE_SHORT_ROAD_ANOTHER_KEEPER_FORMAT := "%s keeps it"
const GATE_LONG_ROAD_ANOTHER_KEEPER_FORMAT := "%s keeps it. They must give it up first."

# ⛔ RETIRED — the five whole-sentence gate consts the SHORT/LONG pairs above replaced
# (`GATE_REASON_ROAD_NEEDS_RUNG_FORMAT`, `_WORN_IN`, `_KNOWLEDGE_HEAD*`, `_NO_KEEPER`,
# `_ANOTHER_KEEPER_FORMAT`). Each was one string doing two jobs at one length: long enough to wrap a
# 292px row, short enough that the remedy had to be dropped to fit. The row and the hover each take
# the form they have room for now.

## **WHAT A RUNG BUYS, OFF THE CATALOG RATHER THAN OFF A BUILT ROAD** — the same three clauses
## `bonus_value` composes for the tile card, asked of a rung nobody has raised yet.
##
## ⛔ **IT IS ALL FUTURE TENSE, INCLUDING THE SIGHT CLAUSE.** `bonus_value` reads a road's RESOLVED
## `grants_sight` — *is it lighting its tiles right now* — and there is no such answer for a rung that
## does not exist; what the catalog carries is whether the rung lights its tiles once it stands and its
## bill is paid. The `dark until its upkeep is paid` half has no place here either: it is news about a
## road being lost, and nothing is being lost on a rung nobody has built.
##
## `""` where the rung buys nothing on every axis — both free rungs — and the caller then renders no
## aside at all, exactly as the tile card renders no payoff row.
static func rung_payoff_clause(entry: Dictionary) -> String:
	var clauses: Array[String] = []
	var multiplier := catalog_friction(entry)
	if multiplier < ROAD_FRICTION_NO_HELP:
		clauses.append(ROAD_BONUS_FRICTION_FORMAT % int(round(
			(ROAD_FRICTION_NO_HELP - multiplier) * ROAD_PERCENT_SCALE)))
	if catalog_grants_sight(entry):
		clauses.append(ROAD_BONUS_SIGHT)
	var span := catalog_link_span(entry)
	if span > ROAD_LINK_NONE:
		clauses.append(ROAD_BONUS_LINK_FORMAT % span)
	return ROAD_CLAUSE_SEPARATOR.join(clauses)
