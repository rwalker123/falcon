class_name HudRouteVocab

## ROAD vocabulary — the intensification ladder's third branch (arc #532,
## `.claude/rules/core_sim/routes.md`). The rung names, the road readout's row keys and formats, and
## the one composer behind each of its rows.
##
## ⛔ **A ROAD IS NOT AN ORDER PATH.** `AnnotationRenderer._routes` and `map_preview`'s `"routes"`
## annotation state are the per-faction ORDER-PATH overlay — waypoints a player's own movement orders
## are following, which vanish when the order does. A road is a WORLD OBJECT with a fixed stamped
## path that outlives every band that walks it, and it does not follow a camp. The obvious name was
## taken by the other thing first, so the client noun for this one is **road** throughout
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
const RUNG_KEY_GAME_TRAIL := "route:game_trail"
const RUNG_KEY_TRAIL := "route:trail"
const RUNG_KEY_DIRT_ROAD := "route:dirt_road"
const RUNG_KEY_PAVED_ROAD := "route:paved_road"

## The branch in climb order, floor first. **The order is the BRANCH's, not this file's** — it is
## `game trail → trail → dirt road → paved road` in `intensification_ladder.json`, restated here for
## the same reason `SourceForecast.RUNG_KEY_*` restates the plant and animal ladders: the client is
## sent the rung a road HOLDS and must be able to name the one traffic is wearing in above it.
##
## A rung this list does not know renders its meter without a destination name rather than guessing
## one — see `wearing_in_value`.
const RUNG_ORDER := [
	RUNG_KEY_GAME_TRAIL,
	RUNG_KEY_TRAIL,
	RUNG_KEY_DIRT_ROAD,
	RUNG_KEY_PAVED_ROAD,
]

## Player-facing names. The floor is *"Game trail"* rather than *"None"* because it IS a thing —
## the first roads are the ones the animals made (issue #215), and it is a rung a road really holds.
const RUNG_LABELS := {
	RUNG_KEY_GAME_TRAIL: "Game trail",
	RUNG_KEY_TRAIL: "Trail",
	RUNG_KEY_DIRT_ROAD: "Dirt road",
	RUNG_KEY_PAVED_ROAD: "Paved road",
}

## The mark a road wears on the tile card, in the same slot the plant/animal rung badges wear theirs.
const ROAD_GLYPH := "🛣"

# ---- THE TILE CARD'S ROAD BLOCK --------------------------------------------------------------
#
# Five rows, each `Key: value`, keys inside `DetailFormat.DETAIL_KEY_MAX_LENGTH` so they align in the
# card's two-column table with every other row on it.

## What this ground carries. Always rendered where a road crosses the hex.
const ROAD_ROW := "Road"

## The meter traffic is banking into the rung ABOVE the one held — the route twin of `Cultivation` /
## `Field`. **Rendered only while something is rising**: a full meter means the held rung is complete
## and nothing is being worn in, which is a row with nothing to say rather than a `100%` one.
const ROAD_WEARING_ROW := "Wearing in"

## What holding it costs, per turn, and how many `roadwork` keepers that bill wants.
const ROAD_KEEPING_ROW := "Keeping"

## The neglect COUNTDOWN, rendered only while the road is actually at risk.
const ROAD_REVERTING_ROW := "Reverting"

## ⛔ **WHAT THE RUNG IS BUYING — the row this whole readout exists for.** The route ladder is
## deliberately NOT a straight upgrade path: a road is cheaper to travel and dearer to keep, and the
## player is meant to pave only where the traffic pays for the upkeep. Without a visible statement of
## what a rung buys, every road reads as pure cost and the decision the branch exists to create is
## invisible — §4.9 item 12's *"a tax, not a ladder"* trap, on the client side of the wire.
const ROAD_BUYS_ROW := "Buys"

## `Road:` value — the rung it holds, badged. `RUNG_BUILT_FORMAT`'s sibling: a road always HOLDS its
## rung whole (the meter beside it belongs to the rung above), so there is no percentage here.
const ROAD_RUNG_FORMAT := "%s %s"

## …and the same face with the branch's own consequence word when the keeping is short, through the
## shared `RUNG_UNDER_KEPT_FORMAT` the plant and animal rows use. **`washing out` is the ROUTE web's
## word**, beside the plant web's `slipping` and the animal web's `drifting`: a road nobody keeps is
## not abandoned, it is eroding.
const ROAD_UNDER_KEPT_WORD := "washing out"

## `Wearing in:` value — the destination rung and the meter, or the bare meter where the destination
## is a rung this client does not know.
const ROAD_WEARING_FORMAT := "%s %d%%"
const ROAD_WEARING_UNNAMED_FORMAT := "%d%%"

## `Keeping:` values. The bill and the keeper count are one sentence because they are one decision —
## *"wants 4, you have 0"* is the readout that makes a standing cost legible.
const ROAD_KEEPING_FORMAT := "%s work a turn · wants %d keepers"
const ROAD_KEEPING_SHORT_FORMAT := "%s short of %s work a turn · wants %d keepers"

## …and the FLOOR's own answer, which is not "0" but "nobody maintains one". The game trail declares
## no upkeep at all, and that is the whole of what makes it free — a `0 work a turn` row would read
## as a bill that happens to be empty this turn.
const ROAD_KEEPING_FREE := "free — nobody keeps a game trail"

## `Reverting:` values — the COUNTDOWN, never the counter. `0` means it is reverting NOW, which is
## why zero gets its own word rather than printing `in 0 turns`.
const ROAD_REVERTING_NOW := "%s now"
const ROAD_REVERTING_FORMAT := "%s in %d turns"
const ROAD_REVERTING_ONE := "%s next turn"

## The countdown's own zero, named because it is a MEANING (biting now) and not a sentinel.
const ROAD_REVERTING_IMMINENT := 0

## `Buys:` clauses. Joined with the card's own middot separator; each is omitted where the rung buys
## nothing on that axis, so a rung that buys nothing at all falls through to `ROAD_BUYS_NOTHING`.
const ROAD_BUYS_SEPARATOR := " · "

## The friction payoff, stated as what it SAVES. `friction_multiplier` is the fraction of the base
## loss a network bound by this road pays, so `0.6` is *40% less lost* — the conversion is a
## presentation of the published multiplier, never a re-derivation of a sim answer.
const ROAD_BUYS_FRICTION_FORMAT := "%d%% less lost between bands"

## The sight payoff — `grants_sight` is the RESOLVED answer, because a client cannot re-derive
## *"is the bill met"* (that is a comparison against the stamped basis with the sim's own epsilon).
const ROAD_BUYS_SIGHT := "lights its own tiles"

## …and its other half: a BUILT road whose bill is unpaid goes dark BEFORE it decays, which is the
## honest early warning that it is being lost. Said out loud, because a clause that merely vanished
## would read as a rung that never lit anything.
const ROAD_BUYS_DARK := "dark until its keeping is paid"

## ⛔ **THE LINK SPAN IS FUTURE TENSE, AND THAT IS NOT A STYLE CHOICE.** `holds_link_to_tiles` is
## authored on every route rung and **not yet read by the sim** — nothing in `balance_supply_networks`
## consumes it (that is slice 13b). It is published now because it is half of this line, and an honest
## *"authored, not yet consumed"* beats a field the client has to guess at; rendering it in the
## present tense would state an effect that is not in play.
const ROAD_BUYS_LINK_FORMAT := "will hold a link %d tiles out"

## A rung that buys nothing on any axis — the game trail, stated rather than left as an empty row.
## Both its terms are their own neutral (multiplier `1.0`, span `0`), and that is a LIVE reading:
## *"this rung is worth nothing"* is precisely what the branch's floor means.
const ROAD_BUYS_NOTHING := "nothing — a path the animals made"

## The friction multiplier at which a rung takes nothing off the loss. Named because it is the
## GAME TRAIL's own reading and the test the friction clause is gated on, not a rounding tolerance.
const ROAD_FRICTION_NO_HELP := 1.0

## The link span a rung holding nothing open reads. Same rule as above: a live `0`, not a parked dial.
const ROAD_LINK_NONE := 0

## The meter at which a rung is complete and nothing is rising above it. The wire states exactly
## `1.0` for a road that has just finished a rung AND for one at the top of the ladder, precisely so
## no reader has to derive it by subtraction — see `ROAD_WEARING_ROW`.
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
## it holds only the game trail, which declares no upkeep and so has no meter to lose — and the
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
## without it would put a `Reverting: now` on every game trail in the world.
static func is_at_risk(road: Dictionary) -> bool:
	return is_short(road) and has_neglect_grace(road)

# ---- COMPOSERS -------------------------------------------------------------------------------

## One rung's player-facing name; the raw wire key for a rung this client has never heard of, which
## is the honest answer rather than a blank.
static func rung_label(rung: String) -> String:
	return String(RUNG_LABELS.get(rung, rung))

## The rung traffic is wearing in ABOVE the one held — `""` at the top of the branch and for a rung
## `RUNG_ORDER` does not know. Callers render the bare meter in that case rather than naming a rung
## they cannot vouch for.
static func next_rung_label(rung: String) -> String:
	var at := RUNG_ORDER.find(rung)
	if at < 0 or at + 1 >= RUNG_ORDER.size():
		return ""
	return rung_label(String(RUNG_ORDER[at + 1]))

## `Road:` — the rung this road HOLDS, with the branch's hazard word where its keeping is short. The
## shared `RUNG_UNDER_KEPT_FORMAT` and `RUNG_HAZARD_GLYPH`, so a washing-out road wears the same mark
## in the same place a slipping patch and a drifting flock do.
static func road_row_value(road: Dictionary) -> String:
	var face: String = ROAD_RUNG_FORMAT % [ROAD_GLYPH, rung_label(rung_of(road))]
	if is_short(road):
		return HudSelectionVocab.RUNG_UNDER_KEPT_FORMAT % [face,
			HudSelectionVocab.RUNG_HAZARD_GLYPH, ROAD_UNDER_KEPT_WORD]
	return face

## `Wearing in:` — the meter on the rung being raised, or `""` where nothing is. **`1.0` is the
## complete reading**, published exactly rather than derived by subtraction, so the test is a plain
## comparison and never a tolerance.
static func wearing_in_value(road: Dictionary) -> String:
	var meter := build_fraction_of(road)
	if meter >= ROAD_METER_COMPLETE:
		return ""
	var percent := int(floor(meter * ROAD_PERCENT_SCALE))
	var destination := next_rung_label(rung_of(road))
	if destination == "":
		return ROAD_WEARING_UNNAMED_FORMAT % percent
	return ROAD_WEARING_FORMAT % [destination, percent]

## `Keeping:` — the bill, the shortfall and the keeper count, every figure straight off the wire.
## **The shortfall is the SIM'S field, never `demand − supplied`**: all three read one stamped basis,
## and this branch has shipped that defect twice.
static func keeping_value(road: Dictionary) -> String:
	if not owes_keeping(road):
		return ROAD_KEEPING_FREE
	var demand := DetailFormat.format_work_units(upkeep_demand_of(road))
	var wants := upkeep_workers_needed_of(road)
	if is_short(road):
		return ROAD_KEEPING_SHORT_FORMAT % [
			DetailFormat.format_work_units(upkeep_shortfall_of(road)), demand, wants]
	return ROAD_KEEPING_FORMAT % [demand, wants]

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

## ⛔ `Buys:` — **WHAT THIS RUNG IS BUYING, and the point of the whole readout.** Three clauses, each
## off a published field:
##
## - the friction it saves, as a percentage of the base loss it takes off (`friction_multiplier`);
## - whether it is lighting its own tiles right now (`grants_sight`, the RESOLVED answer) — and, on a
##   built road whose bill is unpaid, that it has gone dark, which happens BEFORE the rung decays;
## - the link span the rung will hold open (`holds_link_to_tiles`), in **future tense**, because the
##   sim does not read that field yet.
##
## A rung buying nothing on every axis says so out loud rather than rendering an empty row.
static func buys_value(road: Dictionary) -> String:
	var clauses: Array[String] = []
	var multiplier := friction_multiplier_of(road)
	if multiplier < ROAD_FRICTION_NO_HELP:
		clauses.append(ROAD_BUYS_FRICTION_FORMAT % int(round(
			(ROAD_FRICTION_NO_HELP - multiplier) * ROAD_PERCENT_SCALE)))
	if grants_sight(road):
		clauses.append(ROAD_BUYS_SIGHT)
	elif is_short(road):
		# A road only goes dark for a reason, and the reason is the unpaid bill one row up. Gated on
		# the shortfall rather than printed for every unlit road, because the GAME TRAIL lights
		# nothing even with its bill paid in full — a path the animals made is not a road somebody
		# keeps — and telling the player to pay a bill that does not exist would be a wrong remedy.
		clauses.append(ROAD_BUYS_DARK)
	var span := holds_link_to_tiles_of(road)
	if span > ROAD_LINK_NONE:
		clauses.append(ROAD_BUYS_LINK_FORMAT % span)
	if clauses.is_empty():
		return ROAD_BUYS_NOTHING
	return ROAD_BUYS_SEPARATOR.join(clauses)

## **THE WHOLE ROAD BLOCK FOR ONE ROAD**, as `Key: value` detail lines. One composer, so the tile
## card and any later surface state one road the same way.
##
## The rows in order: what it is · what traffic is wearing in · what it costs · when it is lost ·
## what it buys. The last is deliberately LAST — it is the answer the player weighs the four above
## against, so it reads as the conclusion rather than as one more property.
static func road_lines(road: Dictionary) -> Array[String]:
	var lines: Array[String] = []
	lines.append("%s: %s" % [ROAD_ROW, road_row_value(road)])
	var wearing := wearing_in_value(road)
	if wearing != "":
		lines.append("%s: %s" % [ROAD_WEARING_ROW, wearing])
	lines.append("%s: %s" % [ROAD_KEEPING_ROW, keeping_value(road)])
	var reverting := reverting_value(road)
	if reverting != "":
		lines.append("%s: %s" % [ROAD_REVERTING_ROW, reverting])
	lines.append("%s: %s" % [ROAD_BUYS_ROW, buys_value(road)])
	return lines

## The `Road:` row's ink — the shared rung palette, so a road at risk reads in the same amber a
## slipping patch does and a kept one in the same signal a built rung does.
static func road_value_hex(value: String) -> String:
	if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
		return HudStyle.WARN_HEX
	return HudStyle.SIGNAL_HEX

## The `Keeping:` / `Reverting:` ink. Amber where the bill is unmet, plain ink otherwise —
## `ecology_value_hex`'s shape, keyed on the hazard mark the composers above put there rather than on
## a list of known sentences.
static func keeping_value_hex(value: String) -> String:
	if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
		return HudStyle.WARN_HEX
	return HudStyle.INK_HEX

## The `Buys:` ink. A rung that buys something reads in SIGNAL — it is the one row on the card that
## states a PAYOFF, and the branch fails as a ladder if it does not stand out from the cost above it.
## A rung that buys nothing reads dim, which is the truthful weight of the floor.
static func buys_value_hex(value: String) -> String:
	return HudStyle.INK_DIM_HEX if value == ROAD_BUYS_NOTHING else HudStyle.SIGNAL_HEX
