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
## value (*40% less lost between bands*) already reads as a benefit, so the label only narrowed the
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

## ⛔ **WHOSE JOB THIS TILE IS** — the row that says who is on the hook for the bill above it.
##
## **The keeper is the band that BUILT it, wherever that band now stands.** It is not *"whoever is
## standing here"*: `route_keeping_claims` walks the roads a band keeps and never reads that band's
## position, so a band four tiles away goes on paying. Rendered only where there is something to say.
const ROAD_KEEPER_ROW := "Kept by"

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

## The hazard qualifier on the rung row — the branch's own consequence word behind the shared mark.
## **`washing out` is the ROUTE web's word**, beside the plant web's `slipping` and the animal web's
## `drifting`: a road nobody keeps is not abandoned, it is eroding. It rides the row as ONE MORE
## MIDDOT CLAUSE, which is the ecology rows' own shape (`205 / 205 · ⚠ Stressed`) — the space-joined
## `HudSelectionVocab.RUNG_UNDER_KEPT_FORMAT` would run the word onto the end of the progress clause
## and read as the NEXT rung washing out.
const ROAD_HAZARD_CLAUSE_FORMAT := "%s %s"
const ROAD_UNDER_KEPT_WORD := "washing out"

## `Kept by:` values. The band's own label, as this client names every band
## (`HudBandLaborState.band_label_for_id` → `HudFormat.band_display_name`), so a road's keeper and the
## same band on the dock cannot be called two different things.
const ROAD_KEEPER_FORMAT := "%s"

## …and what a band OUTSIDE the player's roster reads as. A road may be kept by a people you merely
## know of, and naming them by a raw id would state a fact the player cannot use.
const ROAD_KEEPER_FOREIGN := "another people"

## …and a road that OWES a bill and has nobody paying it: the keeping band is gone, so the road is
## decaying towards nobody. **Re-issuing `grade` / `pave` is how it is picked up — adoption is the
## same act as building**, which is what the clause says rather than inventing an adopt verb.
const ROAD_KEEPER_NOBODY := "nobody — grade it again to take it on"

## ⛔ **WHAT DISTANCE DID TO THE PRICE**, appended to the keeper's name. `keeper_remoteness` is the
## multiple the sim quoted when the keeper took the tile on, applied to BOTH the build pile and the
## standing upkeep — so a road beyond the base keeping range is dearer for exactly one reason and the
## row says which. **A real decision with no other surface**: distance is a cost and never a wall, so
## nothing refuses the road and nothing else on the card explains a bill larger than the rung's.
const ROAD_KEEPER_REMOTE_FORMAT := "%s · far from them — ×%s the rung's price"

## The remoteness at which distance costs nothing — the reading inside the base keeping range, and on
## every road nobody keeps. Named because it is the TEST the clause above is gated on rather than a
## rounding tolerance.
const ROAD_REMOTENESS_AT_HOME := 1.0

## How the multiple is printed. One decimal, because the shipped dial is `2.0` and a future one need
## not be whole — and `%s` above rather than `%.1f` inline so the two cannot drift.
const ROAD_REMOTENESS_FORMAT := "%.1f"

## `Upkeep:` values. The bill and the keeper count are one sentence because they are one decision —
## *"wants 4, you have 0"* is the readout that makes a standing cost legible.
##
## **THERE IS NO "free" READING HERE ANY MORE.** The floor declares no upkeep at all, so the row is
## simply not emitted (see `upkeep_value`) — a sentence saying the bill does not exist is a row spent
## on nothing, and it was on every road in a current game.
const ROAD_UPKEEP_FORMAT := "%s work a turn · wants %d keepers"
const ROAD_UPKEEP_SHORT_FORMAT := "%s short of %s work a turn · wants %d keepers"

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
const ROAD_BONUS_FRICTION_FORMAT := "%d%% less lost between bands"

## The sight clause — `grants_sight` is the RESOLVED answer, because a client cannot re-derive
## *"is the bill met"* (that is a comparison against the stamped basis with the sim's own epsilon).
const ROAD_BONUS_SIGHT := "lights its own tiles"

## …and its other half: a BUILT road whose bill is unpaid goes dark BEFORE it decays, which is the
## honest early warning that it is being lost. Said out loud, because a clause that merely vanished
## would read as a rung that never lit anything. **It says `upkeep`, the row above it's own word.**
const ROAD_BONUS_DARK := "dark until its upkeep is paid"

## ⛔ **THE LINK SPAN IS FUTURE TENSE, AND THAT IS NOT A STYLE CHOICE.** `holds_link_to_tiles` is
## authored on every route rung and **not yet read by the sim** — nothing in `balance_supply_networks`
## consumes it (that is slice 13b). It is published now because it is half of this line, and an honest
## *"authored, not yet consumed"* beats a field the client has to guess at; rendering it in the
## present tense would state an effect that is not in play.
const ROAD_BONUS_LINK_FORMAT := "will hold a link %d tiles out"

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
	var at := RUNG_ORDER.find(rung)
	if at < 0 or at + 1 >= RUNG_ORDER.size():
		return ""
	return rung_label(String(RUNG_ORDER[at + 1]))

## ⛔ **THE APPROACH TO THE NEXT RUNG, NEVER THE STATE OF THIS ONE** — `25% to trail`, not `Trail 25%`.
## `""` where nothing is rising: **`1.0` is the complete reading**, published exactly rather than
## derived by subtraction, so the test is a plain comparison and never a tolerance, and it covers both
## a rung just finished and the top of the ladder.
static func progress_clause(road: Dictionary) -> String:
	var meter := build_fraction_of(road)
	if meter >= ROAD_METER_COMPLETE:
		return ""
	var percent := int(floor(meter * ROAD_PERCENT_SCALE))
	var destination := next_rung_label(rung_of(road))
	if destination == "":
		return ROAD_PROGRESS_UNNAMED_FORMAT % percent
	return ROAD_PROGRESS_FORMAT % [percent, destination.to_lower()]

## `Road:` — **the rung this road HOLDS**, plus what traffic is wearing in above it and the branch's
## hazard word where its upkeep is short, as middot clauses in that order.
##
## ⛔ **THE RUNG IS THE VALUE AND EVERYTHING ELSE IS A QUALIFIER.** A path at 25% is a COMPLETE
## path a quarter of the way to a trail; the row this replaced said `Trail 25%` on a second
## `Wearing in` row, which reads as a road that is 25% built and is the one thing this row must never
## be readable as.
static func road_row_value(road: Dictionary) -> String:
	var clauses: Array[String] = [rung_label(rung_of(road))]
	var progress := progress_clause(road)
	if progress != "":
		clauses.append(progress)
	if is_short(road):
		clauses.append(ROAD_HAZARD_CLAUSE_FORMAT % [
			HudSelectionVocab.RUNG_HAZARD_GLYPH, ROAD_UNDER_KEPT_WORD])
	return ROAD_CLAUSE_SEPARATOR.join(clauses)

## ⛔ **THE PAYOFF, UNLABELLED — what this rung is BUYING, and the point of the whole readout.** Three
## clauses, each off a published field:
##
## - the friction it saves, as a percentage of the base loss it takes off (`friction_multiplier`);
## - whether it is lighting its own tiles right now (`grants_sight`, the RESOLVED answer) — and, on a
##   built road whose bill is unpaid, that it has gone dark, which happens BEFORE the rung decays;
## - the link span the rung will hold open (`holds_link_to_tiles`), in **future tense**, because the
##   sim does not read that field yet.
##
## ⛔ **A RUNG THAT BUYS NOTHING RENDERS NO ROW.** It used to print `nothing — a path the animals
## made`, which was a row spent on saying no AND a false claim about where the tile's rung came from
## (nothing in the sim models an animal's path; the meter is banked by whoever walks the ground). Both
## free rungs land here, so this is the commonest road in the game rendering one row shorter.
static func bonus_value(road: Dictionary) -> String:
	var clauses: Array[String] = []
	var multiplier := friction_multiplier_of(road)
	if multiplier < ROAD_FRICTION_NO_HELP:
		clauses.append(ROAD_BONUS_FRICTION_FORMAT % int(round(
			(ROAD_FRICTION_NO_HELP - multiplier) * ROAD_PERCENT_SCALE)))
	if grants_sight(road):
		clauses.append(ROAD_BONUS_SIGHT)
	elif is_short(road):
		# A road only goes dark for a reason, and the reason is the unpaid bill one row up. Gated on
		# the shortfall rather than printed for every unlit road, because the PATH lights
		# nothing even with its bill paid in full — a rung nobody keeps is not a road going dark —
		# and telling the player to pay a bill that does not exist would be a wrong remedy.
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
static func upkeep_value(road: Dictionary) -> String:
	if not owes_keeping(road):
		return ""
	var demand := DetailFormat.format_work_units(upkeep_demand_of(road))
	var wants := upkeep_workers_needed_of(road)
	if is_short(road):
		return ROAD_UPKEEP_SHORT_FORMAT % [
			DetailFormat.format_work_units(upkeep_shortfall_of(road)), demand, wants]
	return ROAD_UPKEEP_FORMAT % [demand, wants]

## ⛔ `Kept by:` — **WHOSE JOB THIS ROAD IS**, and what distance is charging them for it.
##
## `label` is the band's own name as this client resolves it (`""` for a band outside the player's
## roster — a road really can be kept by a people you merely know of), so this composer never invents
## one from an id.
##
## `""` — no row at all — for a road that owes nothing and nobody keeps: that is the free floor, where
## there is no job to be on the hook for. A road that OWES a bill with no keeper is the opposite case
## and says so out loud, because it is decaying towards nobody.
static func keeper_value(road: Dictionary, label: String) -> String:
	if not has_keeper(road):
		return ROAD_KEEPER_NOBODY if owes_keeping(road) else ""
	var named := label.strip_edges()
	var face: String = ROAD_KEEPER_FORMAT % (named if named != "" else ROAD_KEEPER_FOREIGN)
	if not is_remote(road):
		return face
	return ROAD_KEEPER_REMOTE_FORMAT % [face,
		ROAD_REMOTENESS_FORMAT % keeper_remoteness_of(road)]

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
static func road_lines(road: Dictionary, keeper_label: String = "") -> Array[String]:
	var lines: Array[String] = []
	lines.append("%s: %s" % [ROAD_ROW, road_row_value(road)])
	var bonus := bonus_value(road)
	if bonus != "":
		lines.append("%s: %s" % [ROAD_BONUS_ROW, bonus])
	var upkeep := upkeep_value(road)
	if upkeep != "":
		lines.append("%s: %s" % [ROAD_UPKEEP_ROW, upkeep])
	var keeper := keeper_value(road, keeper_label)
	if keeper != "":
		lines.append("%s: %s" % [ROAD_KEEPER_ROW, keeper])
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
	return HudStyle.INK_HEX

## The `Kept by:` ink. A road that owes a bill with NOBODY paying it reads in the same amber the
## unmet bill above it does — it is the same news one row late — and a kept road reads in plain ink,
## remoteness included: distance is a price, not an alarm.
static func keeper_value_hex(value: String) -> String:
	return HudStyle.WARN_HEX if value == ROAD_KEEPER_NOBODY else HudStyle.INK_HEX

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

static func catalog_friction(entry: Dictionary) -> float:
	return float(entry.get(RUNG_CATALOG_FRICTION, ROAD_FRICTION_NO_HELP))

static func catalog_link_span(entry: Dictionary) -> int:
	return int(entry.get(RUNG_CATALOG_LINK, ROAD_LINK_NONE))

static func catalog_grants_sight(entry: Dictionary) -> bool:
	return bool(entry.get(RUNG_CATALOG_GRANTS_SIGHT, false))

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

## …and the sheet's heading, in `HudWorkVocab.RUNG_TRACK_TITLE`'s register because it is the same kind
## of card one branch over. **`RAISE`, not `TAKE`** — a road is not carried anywhere, and the plant
## branch's verb read as movement on a surface whose whole subject is a piece of ground.
const ROAD_LADDER_TITLE := "RAISE IT TO…"

## What a rung BUYS, as the aside beneath its price — `HudWorkVocab.RUNG_TRACK_BUILD_MATERIAL_FORMAT`'s
## shape, and it wraps the same three clauses the tile card's payoff row states.
const ROAD_LADDER_BUYS_FORMAT := "buys %s"

## ⛔ **THE PRICE OF A RUNG THE LADDER REFUSES, WHICH ITS FACE CANNOT CARRY.** A row's right-hand slot
## states the price where the rung may be ordered and the word `locked` where it may not — so on this
## branch, which eats no material and therefore has no pile aside to fall back on, a refused rung
## would state no price at all. **A rung a player may plan toward must be a rung they can plan
## against**, which is `RungLadder`'s own rule for the material pile, arriving one currency over.
##
## It renders ONLY where the face has been spent on a refusal: an open rung states its price once, on
## the face, and a row saying `110 work` above an aside saying `110 work to raise it` is one fact
## twice. `HudWorkVocab.RUNG_TRACK_BUILD_MATERIAL_FORMAT`'s shape, so the two prices read alike.
const ROAD_LADDER_PRICE_FORMAT := "%s work to raise it"

## ⛔ **HOW FAR TRAFFIC HAS GOT TOWARD THE ROW IT SITS ON.** `build_fraction` is the meter on the rung
## being RAISED, so it belongs to the row DIRECTLY ABOVE the standing one and to no other — the same
## rule the tile card's `25% to trail` follows, said from the destination's side rather than the
## holder's.
const ROAD_LADDER_PROGRESS_FORMAT := "%d%% of the way there already"

## ⛔ **WHAT DISTANCE DOES TO THIS ROW'S PRICE, AS ITS OWN CLAUSE.** The figure beside it is the rung's
## BASE price exactly as published; the multiple is stated separately because multiplying the two here
## would put a copy of the sim's pricing formula in the client, where it can drift. Same sentence the
## tile card's `Kept by:` row already carries, so the player meets one wording for one fact.
const ROAD_LADDER_REMOTE_FORMAT := "far from them — ×%s the rung's price"

# ---- THE GATES, EACH WITH ITS OWN REMEDY -------------------------------------------------------
#
# ⛔ **A GATED ROW IS SHOWN AND EXPLAINED, NEVER HIDDEN.** The sheet exists to say what the branch
# HOLDS; a rung silently missing reads as a shorter ladder rather than as one this road cannot climb
# — `RungLadder`'s own rule, and the reason the action is offered on every road rather than only on
# the ones that can be raised today.

## The GROUND gate. `requires_rung` is the rung this one has to stand on, and a road cannot be built
## on bare ground because of it: a dirt road wants a trail beneath it and a trail is worn in only by
## traffic. Both halves are stated — what it needs, and what this ground actually carries.
const GATE_REASON_ROAD_NEEDS_RUNG_FORMAT := "Needs a %s beneath it — this ground carries only a %s"

## The rung nobody declares. It is not refused for want of anything; there is simply no order to give,
## which the remedy says rather than leaving the row looking broken.
const GATE_REASON_ROAD_WORN_IN := "Worn in by traffic — nothing to order; it rises as your bands pool food across this hex"

## The KNOWLEDGE gate, in `HudFloraVocab.GATE_REASON_*_KNOWLEDGE_FORMAT`'s voice — **with the craft's
## name taken from the ladder's own knowledge roster** rather than spelled here, so a rung added to the
## config names its unlock with no client edit.
##
## **THE REMEDY IS THE RUNG BENEATH, WHICH THE CATALOG ALREADY NAMES.** A route knowledge is earned by
## holding the rung below the one it opens (`requires_rung`), so the sentence reads off the same field
## the ground gate does instead of a second table that could drift from it.
const GATE_REASON_ROAD_KNOWLEDGE_FORMAT := "Your people know %s %d%% — keep a %s carrying traffic to learn it"

## …and the same gate where the roster has no name for the craft yet. The PROGRESS is still the news,
## and naming a discovery this client cannot vouch for would be worse than not naming one.
const GATE_REASON_ROAD_KNOWLEDGE_UNNAMED_FORMAT := "Your people have not learned this craft yet — keep a %s carrying traffic"

## ⛔ **THE KEEPER GATE — a road really has to be somebody's job.** The band token IS the keeper:
## issuing the verb declares the work and names who is on the hook for the standing bill, which are one
## act. `Main.IMPROVEMENT_NO_BAND` refuses the command outright rather than guessing a band, so the
## row says so before the press rather than after it.
const GATE_REASON_ROAD_NO_KEEPER := "No band to keep it — pick one of your bands first; whoever raises a road keeps it"

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
