class_name RungLadder
extends RefCounted

## **All-`static`, stateless** shared LADDER-TRACK layer — the one answer to *"what does this source's
## branch hold, where does it stand on it, and how far may the player send it?"*
##
## **WHY IT EXISTS AT ALL.** `RungGates` answers *"may this source climb its NEXT rung"*, which is the
## right question for a MARK. A destination picker asks a different one: the sim stores **one position
## per source** in cumulative work units and a queue entry names a **destination rung**, climbing every
## rung between where the source stands and where it was sent
## (`docs/plan_standing_upkeep.md` §2.8). So the surface has to state the WHOLE branch — the rungs
## already paid for, the one the source stands on, the ones it could be taken to, and the ones it
## cannot — and `next_rung_ready` structurally cannot say any of that.
##
## **WHY ITS OWN FILE.** It is called by the work board's row (the `⌃` mark's track) and by nothing
## else *yet*, and it would have been a private method there — except that `BandPanelController` is a
## controller and this is arithmetic over a wire dict, which is the split
## `.claude/rules/client/hud-modules.md` draws between a controller and a shared layer. The same
## measurement that produced `SourceForecast`, `RungGates` and `KitRoster`.
##
## **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache. The one piece of HUD state
## the gates need, **faction knowledge**, is threaded in as a `knowledge` PARAMETER exactly as
## `RungGates` threads it, and the ROW BUILDER takes its press handler as a `Callable`.
##
## ⛔ **IT RE-DERIVES NEITHER THE WORK NOR THE TURNS.** A leg's owing and its chained date are the
## sim's (`SourceForecast.build_legs`), read where the queued entry publishes them. A rung NO entry
## covers has no leg, and its owing is the per-rung `workCost − workDone` pair the wire publishes for
## exactly this purpose — the same two numbers `forage::plant_build_legs` subtracts, which the sim's
## own `patch_rung_work_done` calls *"a readout, not a second authority"*. **The TURNS have no such
## pair**: they are chained against a build queue this client cannot see, so a rung with no leg states
## no date at all rather than a number this surface has no right to.

# ---- THE FIVE STATES ------------------------------------------------------------------------------
#
# Read them as one enumeration: every rung of a branch is in exactly one, and the track's whole job is
# to make which one obvious at a glance.

## Already paid for. **It contributes NOTHING to the figures**, which is the property the readout most
## has to make obvious: a previous improvement is a RECEIPT, NOT A DISCOUNT — the player is never
## asked to buy work they have already bought, and is never offered it back either.
const STATE_BANKED := "banked"
## The rung the source stands on today — the highest banked one, or the branch's wild floor.
const STATE_STANDING := "standing"
## Above the standing rung and BELOW the destination a queued entry names: a leg the climb will lay on
## its way, not a thing the player chose for itself.
const STATE_PATH := "path"
## The destination a queued entry names. The climb ends here and the entry leaves the queue.
const STATE_TARGET := "target"
## Reachable on this branch and refused — by knowledge, by this source's own state, or outright by
## what the species or the ground will admit. **A locked rung stays VISIBLE**: a track exists to say
## what the branch holds, and a rung silently missing from it reads as a shorter ladder rather than as
## one this source cannot climb.
const STATE_LOCKED := "locked"
## Above the standing rung, ungated, and not on any chosen path — a destination the player may pick.
const STATE_OPEN := "open"

## The row keys `track` publishes and `build_track` renders. **Named**, because producer and reader are
## different scripts and a typo in a `get` here is a silent empty row.
const ROW_RUNG_KEY := "rung"
const ROW_IMPROVEMENT_KEY := "improvement"
const ROW_NAME_KEY := "name"
const ROW_STATE_KEY := "state"
const ROW_WORK_KEY := "work_remaining"
const ROW_TURNS_KEY := "turns_remaining"
const ROW_REASONS_KEY := "reasons"
const ROW_SELECTABLE_KEY := "selectable"

## ⛔ **THE THREE KEYS THE ROUTE BRANCH ADDS, AND EVERY ONE IS OPTIONAL.** A row that carries none of
## them renders exactly as it always did, which is what keeps the plant and animal tracks untouched by
## a cut made for roads.
##
## `ROW_FACE_KEY` is a FINISHED right-hand face. The other two branches let `_row_face` derive one
## from the state and the figures; a road row states `<figure> · <nearest refusal>`, which is a
## composition only its producer can make — it alone knows which refusal is nearest.
##
## `ROW_TOOLTIP_KEY` is the row's hover, and it is where the payoff, the standing bill, the
## remoteness and EVERY refusal went when the row was cut to one line. **Nothing was deleted; it
## moved here**, which is why the harness asserts hovers and not only visible text.
##
## `ROW_NAME_WIDTH_KEY` overrides the shared name column. See `HudRouteVocab.ROAD_LADDER_NAME_WIDTH`:
## the route faces are wider and its rung names are shorter, so the column moves the pixels across
## rather than widening a card the other two branches share.
const ROW_FACE_KEY := "face"
const ROW_TOOLTIP_KEY := "tooltip"
const ROW_NAME_WIDTH_KEY := "name_width"

## **THE TWO PRICE-ASIDE KEYS, BESIDE `ROW_REASONS_KEY` AND DELIBERATELY NOT INSIDE IT.** That array
## means *why this rung is refused*, and a price is not a refusal — a LOCKED rung carries both, which
## is the whole reason they could not share one key.
##
## `ROW_BUILD_ASIDES_KEY` is what raising the rung EATS — the pile, and beneath it the stall warning
## when the band's shelf cannot cover it. `ROW_HOLD_ASIDES_KEY` is what holding it costs every turn.
## Both are arrays of `{RUNG_ASIDE_TEXT_KEY, RUNG_ASIDE_WARN_KEY}` rather than of bare strings,
## because the stall line is the one aside on this card that is not quiet: it is the reading that
## should stop the player, and an aside array of strings has nowhere to say so.
const ROW_BUILD_ASIDES_KEY := "build_asides"
const ROW_HOLD_ASIDES_KEY := "hold_asides"

## One aside's two fields. `warn` picks the ink at render (`build_aside`), so the producer decides
## severity and the renderer never sniffs the text for it.
const RUNG_ASIDE_TEXT_KEY := "text"
const RUNG_ASIDE_WARN_KEY := "warn"

## A rung whose owing this layer cannot state at all — the wire prices no such job on this source.
## Distinct from `0.0`, which is a real reading meaning *nothing left to pay*.
const WORK_UNKNOWN := -1.0

# ---- THE CROP STEP'S KEYS ------------------------------------------------------------------------
#
# The two SOURCE keys this layer reads for a plant rung's crop, spelled once because they take the
# caller's `prefix` and a bare read on a `patch_`-prefixed `tile_info` silently answers nothing —
# the trap `hud_compose_vocab.gd` → `BARE_FORECAST_PREFIX` carries the long form of.
const CROP_COMPOSITION_KEY := "composition"
const CROP_COMMITTED_SPECIES_KEY := "committed_species"

## The row keys `crop_choices` publishes and `build_crop_step` renders, `track`'s own convention.
const CROP_SPECIES_KEY := "species"
const CROP_LABEL_KEY := "label"
## What this crop would COST and what it would PAY, as one aside beneath the row. It was
## `CROP_PAYOFF_KEY` while a crop row carried payoffs alone; a key named for the payoffs while
## holding a price is how a reader comes to believe the work figure is one.
const CROP_FACE_KEY := "face"

## **THE PER-CROP SOW PRICE, AND ITS ABSENCE IS A REFUSAL** (`FloraShareInfo.sowWorkCost`, carried
## onto the basket entry by `SourceForecast.flora_basket_entries` with its own presence key). A plant
## the wire prices no Sow for is one that cannot climb to a Field on this ground — the tile-specific
## legality the species-global `can_sow` ceiling flag structurally cannot express — so it is the same
## predicate the sim's own `default_species_for_rung` filters on.
const CROP_SOW_WORK_COST_KEY := "sow_work_cost"
const CROP_HAS_SOW_WORK_COST_KEY := "has_sow_work_cost"

## **THE WIRE'S OWN SPELLING OF *let the sim choose*** — a real instruction rather than an absent one,
## which is why it is a named empty string and not a sentinel. `Main.format_assign_labor` omits the
## species token for it, and the sim then commits to the tile's dominant legal plant.
const CROP_SIM_PICKS := ""

## **THE WHOLE BRANCH, BOTTOM RUNG FIRST — one row per rung, in CLIMB order.**
##
## The order is the wire's own (`SourceForecast.rung_branch_for_kind`) and it is load-bearing twice
## over: the standing rung is the HIGHEST done one, so a branch walked out of order marks the wrong
## rung as banked; and a destination's path is every rung between standing and it, which is a range
## over this list.
##
## `kind` is a LABOR kind (`forage` / `hunt`), `source` the RAW wire source dict, `prefix` the spelling
## its keys are in, `improvement` the assignment's declared verb (the second axis) and `knowledge` the
## faction's `{track: progress}` row.
## `band` is the band the `⌃` was opened from, read for ONE thing — its `material_store`, which is
## what the stall warning weighs a rung's pile against. `{}` (the default) simply drops that aside: a
## caller with no band in hand states the price and not the shortfall, which is the honest half.
static func track(kind: String, source: Dictionary, prefix: String, improvement: String,
        knowledge: Dictionary, band: Dictionary = {}) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var source_kind := SourceForecast.source_kind_for_labor(kind)
    var branch := SourceForecast.rung_branch_for_kind(source_kind)
    var standing := _standing_index(branch, source, prefix)
    # THE DESTINATION A QUEUED ENTRY NAMES, as an index into this branch. `-1` when nothing is queued
    # here, which is the state the picker is usually opened in — and then no rung is `path` or
    # `target`, because nothing has been chosen.
    var destination := _index_of_improvement(branch,
        SourceForecast.build_destination_rung(source, prefix))
    # …and the legs that entry still owes, keyed by rung so a row can ask for its own. The list is
    # EMPTY on an unqueued source, which is why every figure below has a no-leg answer.
    var legs := _legs_by_rung(source, prefix)
    var gates := _gates(kind, source, prefix, knowledge)
    # **THE PILE THE WIRE QUOTES BELONGS TO EXACTLY ONE ROW** — the rung directly above where the
    # source stands, which is the only one `buildMaterialCost` prices (see
    # `SourceForecast.FORECAST_BUILD_MATERIAL_COST_KEY`). Held as an index rather than re-tested per
    # row so a track can never quote one rung's pile against another rung's name.
    var priced_index := standing + 1
    var pile := SourceForecast.build_material_cost(source, prefix)
    var store := _material_store(band)
    # **THE PATH IS BLOCKED FROM BELOW.** A climb lays every leg between where the source stands and
    # where it was sent, so a barred rung bars everything above it — and the honest thing to state on
    # the rung above is the refusal that actually happened, further down.
    var blocking_row := {}
    for index in range(branch.size()):
        var rung_key := String(branch[index])
        var verb := String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(rung_key,
            SourceForecast.IMPROVEMENT_NONE))
        var row := {
            ROW_RUNG_KEY: rung_key,
            ROW_IMPROVEMENT_KEY: verb,
            ROW_NAME_KEY: _rung_name(verb),
            ROW_WORK_KEY: WORK_UNKNOWN,
            ROW_TURNS_KEY: SourceForecast.BUILD_TURNS_NO_ESTIMATE,
            ROW_REASONS_KEY: [] as Array[String],
            ROW_BUILD_ASIDES_KEY: [] as Array[Dictionary],
            ROW_HOLD_ASIDES_KEY: [] as Array[Dictionary],
            ROW_SELECTABLE_KEY: false,
        }
        if index < standing:
            row[ROW_STATE_KEY] = STATE_BANKED
            rows.append(row)
            continue
        if index == standing:
            row[ROW_STATE_KEY] = STATE_STANDING
            rows.append(row)
            continue
        # A rung ABOVE the standing one. Its own refusal first, then whatever refused below it.
        var reasons := _rung_refusals(kind, source, prefix, verb, gates)
        if reasons.is_empty() and not blocking_row.is_empty():
            reasons = [HudFloraVocab.GATE_REASON_PATH_BLOCKED_FORMAT % [
                String(blocking_row[ROW_NAME_KEY]),
                String((blocking_row[ROW_REASONS_KEY] as Array)[0])]]
        row[ROW_WORK_KEY] = _work_remaining(source, prefix, verb, legs, rung_key)
        row[ROW_TURNS_KEY] = _leg_turns(legs, rung_key)
        # **THE PRICE ASIDES GO ON EVERY ROW ABOVE THE STANDING RUNG, LOCKED ONES INCLUDED.** A rung
        # the branch refuses is still a rung the player may plan toward, and a price hidden behind a
        # refusal is a price nobody can plan against — which is exactly the state a player who has
        # never woven a hurdle is in when they look at a pen.
        if index == priced_index:
            row[ROW_BUILD_ASIDES_KEY] = _build_price_asides(pile, store)
        row[ROW_HOLD_ASIDES_KEY] = _hold_price_asides(source, prefix, verb)
        if not reasons.is_empty():
            row[ROW_STATE_KEY] = STATE_LOCKED
            row[ROW_REASONS_KEY] = reasons
            if blocking_row.is_empty():
                blocking_row = row
        elif index == destination:
            row[ROW_STATE_KEY] = STATE_TARGET
            row[ROW_SELECTABLE_KEY] = true
        elif destination > index:
            row[ROW_STATE_KEY] = STATE_PATH
            row[ROW_SELECTABLE_KEY] = true
        else:
            row[ROW_STATE_KEY] = STATE_OPEN
            row[ROW_SELECTABLE_KEY] = true
        rows.append(row)
    return rows

## **IS THERE ANY DESTINATION TO OFFER ON THIS SOURCE?** — the one test a caller asks before opening a
## track, so an empty card is never floated. It is deliberately NOT `next_rung_ready`: that answer is
## the highest UNGATED rung and the track is worth opening for a locked one too, which is the whole of
## why a barred rung stays visible.
static func has_track(rows: Array[Dictionary]) -> bool:
    for row in rows:
        var state := String(row.get(ROW_STATE_KEY, ""))
        if state != STATE_BANKED and state != STATE_STANDING:
            return true
    return false

# ---- THE ROUTE BRANCH — a SIBLING of `track`, never a widening of it -------------------------------
#
# ⛔ **`track` TAKES A LABOR `kind` AND A WIRE SOURCE DICT, AND A ROAD HAS NEITHER.** There is no
# `forage`/`hunt` crew on a road, no per-source forecast row, no queued entry publishing legs, and no
# `prefix` to spell its keys under. Widening that signature would push every plant and animal call
# site through a branch it cannot use, so the route branch gets its own producer — and hands the
# result to the SAME `build_track` renderer, which is what keeps one rung reading one way on every
# card in the client.
#
# ⛔ **EVERY LABEL, PRICE, PAYOFF AND GATE COMES OFF THE CATALOG** (`HudRouteVocab.route_ladder`, the
# wire's `routeRungs`). Nothing here reads `HudRouteVocab.RUNG_LABELS` and nothing hard-codes the
# four shipped rungs: a rung added to `intensification_ladder.json` appears as a row with no client
# edit at all, which is the whole reason the tile card's action opens a LADDER rather than a button
# per verb.

## A rung nobody declares — the free floor, worn in by traffic. **The seventh state, and the route
## branch is the only web that can produce it**: it is not refused (which is what `STATE_LOCKED`
## says) and it is not pressable, because there is no order to give. A rung in this state still
## states its reason as an aside, so the row says why rather than merely being inert.
const STATE_UNORDERED := "unordered"

## **THE ROUTE BRANCH FOR ONE ROAD, BOTTOM RUNG FIRST — ONE LINE PER RUNG.**
##
## `road` is the raw `routes` row for the tile (the `HudRouteVocab` field readers' own shape),
## `ladder` the ordered catalog, `knowledge` the faction's `{track: progress}` row, `labels` the
## `{knowledge_id: display_name}` lookup off the ladder's knowledge roster, and `band` the acting band
## — `{}` meaning none is picked, which is one of the keeper gates.
##
## `keeper_label` is the name of whichever band ALREADY keeps this tile, resolved by the caller
## through this client's one band-naming rule — passed straight to `RungGates.route_gates`, which
## names it in the *one band keeps a road tile, never two* refusal. This layer resolves no names, the
## tile card's own `Kept by:` row having established that the drawer does it.
##
## ⛔ **THE ROW CARRIES A FINISHED FACE AND A TOOLTIP, AND NO ASIDES AT ALL.** The plant and animal
## branches stack their price, their stall warning and their standing bill BENEATH the row; this one
## printed the same way and came out six lines deep per rung, which is the wordiness this cut removed.
## What a road row states is `<figure> · <nearest refusal>` and everything else is on the hover, so
## `ROW_BUILD_ASIDES_KEY` / `ROW_HOLD_ASIDES_KEY` / `ROW_REASONS_KEY` stay EMPTY and `build_track`'s
## aside loops emit nothing for these rows without needing to know the difference.
##
## **THE ROWS ARE OTHERWISE `track`'s OWN `ROW_*` SHAPE**, so the renderer is unchanged. Three of the
## six original states are unreachable here and that is structural rather than unfinished: `path` and
## `target` name legs of a QUEUED entry, and no road publishes one.
## `builders` is the ACTING BAND'S OWN POOL and `kit_gear` what that pool carries, which is what makes
## the turns estimate a fact about the band the picker names rather than about the road — a different
## band with a different pool is a different answer, so both move when the picker does.
##
## `queue` is that same band's BUILD QUEUE, as `{ahead, head}` — how many entries a press would land
## behind and what the first of them is called. It moves with the picker for the identical reason: a
## different band is a different line to stand in.
static func route_track(road: Dictionary, ladder: Array[Dictionary], knowledge: Dictionary,
        labels: Dictionary, band: Dictionary,
        keeper_label: String = "", builders: int = SourceForecast.BUILD_CREW_NONE,
        kit_gear: Dictionary = {}, queue: Dictionary = {}) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var gates := RungGates.route_gates(road, ladder, knowledge, labels, band, keeper_label)
    var standing_order := HudRouteVocab.ladder_order_of(ladder, HudRouteVocab.rung_of(road))
    # **THE REMOTENESS IS THE KEEPER'S PUBLISHED MULTIPLE, AND IT IS NEVER MULTIPLIED IN.** The sim
    # quoted it when this road's keeper took the tile on; the client states it in the hover, apart
    # from the base price, because folding the two would put the sim's pricing formula here.
    var remote := HudRouteVocab.is_remote(road)
    var meter := HudRouteVocab.build_fraction_of(road)
    # **THE METER BELONGS TO THE FIRST ROW ABOVE THE STANDING RUNG AND TO NO OTHER** — it is the rung
    # being RAISED, which is a DIFFERENT rung from the one the road holds. Tracked as a latch rather
    # than an `order + 1` test, because the catalog's orders need not be contiguous.
    var approach_placed := false
    for entry in ladder:
        var verb := HudRouteVocab.catalog_verb(entry)
        var rung_key := HudRouteVocab.catalog_rung_key(entry)
        var row := {
            ROW_RUNG_KEY: rung_key,
            ROW_IMPROVEMENT_KEY: verb,
            ROW_NAME_KEY: HudRouteVocab.catalog_display_name(entry),
            ROW_NAME_WIDTH_KEY: HudRouteVocab.ROAD_LADDER_NAME_WIDTH,
            ROW_WORK_KEY: WORK_UNKNOWN,
            ROW_TURNS_KEY: SourceForecast.BUILD_TURNS_NO_ESTIMATE,
            ROW_REASONS_KEY: [] as Array[String],
            ROW_BUILD_ASIDES_KEY: [] as Array[Dictionary],
            ROW_HOLD_ASIDES_KEY: [] as Array[Dictionary],
            ROW_SELECTABLE_KEY: false,
        }
        var order := HudRouteVocab.catalog_order(entry)
        if order < standing_order:
            row[ROW_STATE_KEY] = STATE_BANKED
            rows.append(row)
            continue
        if order == standing_order:
            row[ROW_STATE_KEY] = STATE_STANDING
            rows.append(row)
            continue
        # THE METER is CLAIMED by the first row above the standing rung whether or not that row
        # shows it, so a later row can never pick it up.
        var is_approach_row := not approach_placed
        approach_placed = true
        var refusals := RungGates.route_gates_for(gates, rung_key)
        # **THE ESTIMATE IS PER ROW, because the pile is** — only the row directly above the standing
        # rung has anything banked against it, and every row above that is priced whole.
        var turns := _route_turns(entry, meter if is_approach_row else NOTHING_BANKED,
            builders, kit_gear)
        row[ROW_TURNS_KEY] = turns
        var turns_clause := DetailFormat.build_turns_clause(turns, builders)
        row[ROW_TOOLTIP_KEY] = _route_tooltip(entry, refusals, road, remote, turns_clause)
        if verb == HudRouteVocab.RUNG_CATALOG_NONE:
            # **A RUNG NOBODY ORDERS LEADS WITH ITS METER**, that being the only figure it has; with
            # nothing rising it states the cause alone.
            row[ROW_STATE_KEY] = STATE_UNORDERED
            # ⛔ **THE METER RIDES THIS ROW AND NO OTHER.** A rung nobody orders has no figure of its
            # own, so without the meter the line states only a static fact about traffic. Every other
            # row already leads with a price — and the tile card's `Road` line one block up states the
            # same percentage, so repeating it beside a price would be the duplication this cut is
            # removing rather than a second useful reading.
            var meter_clause := _route_meter_clause(meter) if is_approach_row else ""
            row[ROW_FACE_KEY] = _route_face(meter_clause, HudWorkVocab.RUNG_TRACK_STATE_WORN_IN)
            rows.append(row)
            continue
        # **THE BASE PRICE, AS PUBLISHED**, and it is the row's figure on every ordered rung —
        # refused or not. A rung the player may plan toward has to be one they can plan against.
        row[ROW_WORK_KEY] = HudRouteVocab.catalog_work_cost(entry)
        var price := HudWorkVocab.RUNG_TRACK_COST_UNDATED_FORMAT % DetailFormat.format_work_units(
            HudRouteVocab.catalog_work_cost(entry))
        if refusals.is_empty():
            # Buildable: the price IS the button, and the one clause beside it is **how long** — the
            # question the card could not answer at all until now. Where the band has nobody on
            # `builders` there is no duration to state, so the row states the REMEDY instead, in the
            # aside shape: a blank column reads as a client that failed rather than as a missing crew.
            row[ROW_STATE_KEY] = STATE_OPEN
            row[ROW_SELECTABLE_KEY] = true
            row[ROW_FACE_KEY] = price if turns_clause == "" \
                else HudWorkVocab.RUNG_TRACK_COST_FORMAT % [
                    DetailFormat.format_work_units(HudRouteVocab.catalog_work_cost(entry)),
                    turns_clause]
            # ⛔ **WHERE THE PRESS LANDS, STATED AT THE MOMENT OF THE DECISION.** The press DECLARES —
            # it appends to the acting band's build queue, and the head of that queue takes every
            # builder — so a card that stated only a price and a duration promised work starting on
            # the spot. It also says what the duration is measured from, which is why the estimate is
            # kept rather than cut: the number is right and it is not a completion date.
            var asides: Array[Dictionary] = []
            var placement := _route_queue_aside(queue)
            if placement != "":
                asides.append({
                    RUNG_ASIDE_TEXT_KEY: placement,
                    RUNG_ASIDE_WARN_KEY: false,
                })
            if builders <= SourceForecast.BUILD_CREW_NONE:
                asides.append({
                    RUNG_ASIDE_TEXT_KEY: HudRouteVocab.ROAD_LADDER_NO_BUILDERS_ASIDE,
                    RUNG_ASIDE_WARN_KEY: true,
                })
            row[ROW_BUILD_ASIDES_KEY] = asides
            rows.append(row)
            continue
        # ⛔ **ONE REFUSAL ON THE ROW AND ALL OF THEM IN THE HOVER**, and never the word `locked`
        # beside a reason that already is the state.
        row[ROW_STATE_KEY] = STATE_LOCKED
        row[ROW_FACE_KEY] = HudRouteVocab.ROAD_LADDER_FACE_FORMAT % [
            price, RungGates.route_row_refusal(refusals)]
        rows.append(row)
    return rows

## One row's face, with the meter clause in front of it where this row owns one. `""` for the meter is
## the ordinary case, and then the face is exactly what was passed in — no stray separator.
static func _route_face(meter_clause: String, face: String) -> String:
    if meter_clause == "":
        return face
    return HudRouteVocab.ROAD_LADDER_FACE_FORMAT % [meter_clause, face]

## **HOW FAR TRAFFIC HAS GOT TOWARD THE ROW THIS SITS ON** — `""` where nothing is rising.
##
## The wire states exactly `1.0` for a rung just finished AND for the top of the ladder, so the upper
## test is a plain comparison rather than a tolerance; the lower one keeps a road that has banked
## nothing from stating a `0%` that reads as a stalled build.
static func _route_meter_clause(meter: float) -> String:
    if meter <= HudRouteVocab.RUNG_CATALOG_NO_WORK_COST \
            or meter >= HudRouteVocab.ROAD_METER_COMPLETE:
        return ""
    return HudRouteVocab.ROAD_LADDER_METER_FORMAT % int(
        floor(meter * HudRouteVocab.ROAD_PERCENT_SCALE))

## **WHAT A PRESS ON THIS ROW WOULD JOIN, AS ONE ASIDE.** `""` for a caller that states no queue at
## all — a harness, or a card opened with no band on the roster — which draws no line rather than
## claiming the queue is empty. **An empty queue and an unknown one are different facts**, and the
## first of them is the reassuring one, so it is never the fall-back.
static func _route_queue_aside(queue: Dictionary) -> String:
    if not queue.has(HudRouteVocab.ROAD_LADDER_QUEUE_AHEAD_KEY):
        return ""
    var ahead := int(queue[HudRouteVocab.ROAD_LADDER_QUEUE_AHEAD_KEY])
    if ahead <= QUEUE_NOTHING_AHEAD:
        return HudRouteVocab.ROAD_LADDER_QUEUE_EMPTY_ASIDE
    # **THE HEAD IS NAMED AND THE REST ARE COUNTED**, which is the shape of the decision: the head is
    # what this road is waiting behind, and the count is what a reorder is measured against.
    var more := ""
    if ahead > QUEUE_HEAD_ALONE:
        more = HudRouteVocab.ROAD_LADDER_QUEUE_MORE_FORMAT % (ahead - QUEUE_HEAD_ALONE)
    return HudRouteVocab.ROAD_LADDER_QUEUE_BEHIND_FORMAT % [
        String(queue.get(HudRouteVocab.ROAD_LADDER_QUEUE_HEAD_KEY, "")), more,
        HudRouteVocab.ROAD_LADDER_QUEUE_ESTIMATE_NOTE]

## The two counts the aside above forks on, named because each is a MEANING: nothing is in the way, and
## the only thing in the way is the head this sentence has just named.
const QUEUE_NOTHING_AHEAD := 0
const QUEUE_HEAD_ALONE := 1

## **THE METER READING FOR A ROW WITH NOTHING BANKED AGAINST IT** — every row but the one directly
## above the standing rung. Named because it is a MEANING (this rung has not been started) rather
## than a sentinel: `build_fraction` belongs to exactly one rung and the rest are priced whole.
const NOTHING_BANKED := 0.0

## ⛔ **HOW MANY TURNS THIS RUNG WOULD TAKE AT THE ACTING BAND'S `builders` POOL** — the answer the
## card never gave, and the reason `ROW_TURNS_KEY` stopped being a slot nothing filled.
##
## ⛔ **IT IS THE CLOSED FORM'S OWN SUPPLY SEAM, NOT A SECOND ESTIMATOR.**
## `SourceForecast.pool_work_supply` is exactly what `build_turns_at` divides by — `crew × bare rate
## + min(crew, saturating crew) × gear` — so a road and a Cultivate are paced by one expression. What
## cannot be reused is `build_turns_at` itself: it reads its cost, its banked work and its RATE off a
## prefixed SOURCE dict, and a road has no source row at all (which is why `buildTurnsRemaining` is a
## no-op for roads sim-side).
##
## ⛔ **BOTH TERMS COME OFF THE CATALOG ENTRY, AND THE RATE IS READ RATHER THAN ASSUMED.** The cost is
## `workCost`; the bare rate is `buildWorkPerWorkerTurn`, which the sim publishes on every rung
## precisely so no client transcribes `PER_WORKER_OUTPUT` — this one did, for a slice, and a copied
## constant goes stale in silence the day the sim writes worker output as a sum of more terms.
## **A missing or zero rate is `BUILD_TURNS_NO_ESTIMATE` before any division, never a substituted
## `1.0`.** The published rate is at BARE HANDS, so the kit addend is still the client's to add and
## `pool_work_supply` still does it.
##
## **THE PILE IS THE RUNG'S BASE PRICE, AS PUBLISHED**, matching the figure the row states beside it.
## The remoteness multiple is quoted apart from both, exactly as it is on the face — a client that
## folded it in here would hold a copy of the sim's pricing formula.
##
## **THERE IS NO ROT TERM, AND THAT IS NOT AN OMISSION.** `meter_rot_per_turn` is a SOURCE field; a
## road publishes its shortfall and its grace instead, and the rung being raised here is one nobody
## is yet keeping. `BUILD_TURNS_NO_ESTIMATE` — no clause at all — for a rung the catalog prices at
## nothing and for a pool that supplies nothing.
static func _route_turns(entry: Dictionary, banked_fraction: float, builders: int,
        kit_gear: Dictionary) -> int:
    var cost := HudRouteVocab.catalog_work_cost(entry)
    if cost <= HudRouteVocab.RUNG_CATALOG_NO_WORK_COST:
        return SourceForecast.BUILD_TURNS_NO_ESTIMATE
    # **NO PUBLISHED RATE, NO ANSWER — and the test is BEFORE the division rather than after it.** A
    # zero rate reaching `pool_work_supply` would answer whatever the KIT alone pays, which on a
    # branch no kit serves is `0` and would look like the same refusal for a different reason.
    var per_worker_turn := HudRouteVocab.catalog_build_work_per_worker_turn(entry)
    if per_worker_turn <= SourceForecast.BUILD_WORK_NONE:
        return SourceForecast.BUILD_TURNS_NO_ESTIMATE
    var supply := SourceForecast.pool_work_supply(builders, per_worker_turn, kit_gear)
    if supply <= SourceForecast.BUILD_WORK_NONE:
        return SourceForecast.BUILD_TURNS_NO_ESTIMATE
    # ⛔ **`1.0` MEANS *NOTHING IS RISING*, NOT *THIS RUNG IS PAID FOR*.** The wire states exactly that
    # for a rung just finished AND for the top of the ladder, so a reader that netted it off the pile
    # would quote `≈1 turn` for a 260-work paving nobody has started — `_route_meter_clause` draws the
    # same boundary one line up, and for the same reason.
    var banked := banked_fraction
    if banked >= HudRouteVocab.ROAD_METER_COMPLETE:
        banked = NOTHING_BANKED
    var remaining := cost * maxf(HudRouteVocab.ROAD_METER_COMPLETE - banked, NOTHING_BANKED)
    if remaining <= SourceForecast.BUILD_WORK_NONE:
        return SourceForecast.BUILD_FINISHES_IN_ONE_TURN
    return ceili(remaining / supply)

## **EVERYTHING THE ROW STOPPED SAYING, as one hover.** In reading order: what it costs to build AND
## to keep, what it does, what distance adds, then every refusal in the gate layer's own order.
##
## ⛔ **THE PRICE LINE RENDERS ONLY WHERE THE RUNG OWES UPKEEP.** On a rung that is free to hold it
## would restate the face and add nothing, and a line that says nothing is what this cut removed.
static func _route_tooltip(entry: Dictionary, refusals: Array, road: Dictionary,
        remote: bool, turns_clause: String = "") -> String:
    var lines: Array[String] = []
    var upkeep := HudRouteVocab.catalog_upkeep(entry)
    if upkeep >= SourceForecast.UPKEEP_WORK_MIN:
        lines.append(HudRouteVocab.ROAD_LADDER_TIP_PRICE_FORMAT % [
            DetailFormat.format_work_units(HudRouteVocab.catalog_work_cost(entry)),
            DetailFormat.format_work_units(upkeep)])
    # **HOW LONG, AND IT IS ON EVERY ORDERED ROW.** A buildable row states it on the face as well; a
    # REFUSED one has spent its one face clause on the refusal, and this is the surface that keeps the
    # duration available on a rung the player is planning toward rather than pressing today.
    if turns_clause != "":
        lines.append(HudRouteVocab.ROAD_LADDER_TIP_TURNS_FORMAT % turns_clause)
    var payoff := HudRouteVocab.rung_payoff_clause(entry)
    if payoff != "":
        lines.append(payoff)
    if remote:
        lines.append(HudRouteVocab.ROAD_LADDER_TIP_REMOTE_FORMAT % (
            HudRouteVocab.ROAD_REMOTENESS_FORMAT % HudRouteVocab.keeper_remoteness_of(road)))
    lines.append_array(RungGates.route_tooltip_refusals(refusals))
    return HudRouteVocab.ROAD_LADDER_TIP_SEPARATOR.join(lines)

## **THE TRACK AS CONTROLS** — one row per rung, a `Button` where the rung may be picked and a `Label`
## where it may not.
##
## **THE SHAPE IS THE STATEMENT**, this client's standing rule for the improvement control: a button
## is a CHOICE, and a banked rung, the rung you stand on and an unmet prerequisite are all FACTS. A
## greyed button on a locked rung would offer an act the sim refuses, one press away from a job that
## queues and then blocks.
##
## `on_pick` takes the row's improvement VERB — the destination, which is what the command carries.
## **THE RENDERER IS SHARED WITH THE ROUTE BRANCH, AND ONLY THE HEADING IS WIDENED.** `track` was not:
## it takes a labor `kind` and a wire SOURCE dict, and a road has neither, so widening the PRODUCER
## would force every plant and animal call site through a branch it cannot use. A row is a row on any
## branch, though — same name, same face, same asides in the same order — so `route_track` builds the
## same `ROW_*` shape and hands it here rather than growing a second render loop that could drift.
##
## `title` defaults to the plant/animal heading, so every existing call site is untouched.
static func build_track(rows: Array[Dictionary], on_pick: Callable,
        title_text: String = HudWorkVocab.RUNG_TRACK_TITLE) -> VBoxContainer:
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    column.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    var title := Label.new()
    title.text = title_text
    title.add_theme_color_override("font_color", HudStyle.INK_DIM)
    title.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_TITLE_FONT_SIZE)
    column.add_child(title)
    for row in rows:
        column.add_child(_build_row(row, on_pick))
        # **THE ORDER IS THE SENTENCE**: why it is refused (when it is), what it eats to raise, what
        # the shelf will not cover, then what it costs to hold — the refusal first because a player
        # reads *may I* before *what does it cost*, and the standing bill last because it is the half
        # of the commitment that outlives the build.
        for reason in (row.get(ROW_REASONS_KEY, []) as Array):
            column.add_child(build_aside(String(reason)))
        for aside in (row.get(ROW_BUILD_ASIDES_KEY, []) as Array):
            column.add_child(build_aside(String((aside as Dictionary).get(RUNG_ASIDE_TEXT_KEY, "")),
                bool((aside as Dictionary).get(RUNG_ASIDE_WARN_KEY, false))))
        for aside in (row.get(ROW_HOLD_ASIDES_KEY, []) as Array):
            column.add_child(build_aside(String((aside as Dictionary).get(RUNG_ASIDE_TEXT_KEY, "")),
                bool((aside as Dictionary).get(RUNG_ASIDE_WARN_KEY, false))))
    return column

## **THE RING'S OWN SMALL CARD — what another ring eats to raise, what it costs to hold, and where it
## will stall** (`docs/plan_standing_upkeep.md` §4.9 item 12c).
##
## ⛔ **A RING IS NOT A TRACK ROW, AND GIVING IT ONE WOULD BE A LIE ABOUT THE LADDER.** The track is
## ONE POSITION ON A BRANCH — every row is a rung you are standing on, have banked, or may climb to.
## A ring is a **repeatable increment with no position**: it widens the `animal:pen` rung the herd
## already stands on, so it has no place in that list and `has_track` is correctly FALSE on a
## corralled herd (`animal:pen` being the top of the animal branch). That falsity is the mechanical
## reason extending a pen ended up as a button on the tile card in the first place.
##
## **IT REUSES THE TRACK'S OWN ASIDE COMPOSERS, which is the whole point of it living here.** The
## pile, the stall warning and the standing bill are composed by `_build_price_asides` /
## `_hold_price_asides` — the same three sentences in the same order the `⌃` opens on every other
## rung — so the caret means ONE thing on every mark that wears it.
##
## **THE PRICE IS `animal:pen`'s OWN, IN BOTH CURRENCIES**, because that is what a ring is: the same
## rung, again. The work half is `corral_work_cost`; the pile is
## `SourceForecast.corral_build_material_cost` — the pen rung's own, NOT `build_material_cost`, which
## prices the rung DIRECTLY ABOVE the source and is therefore empty on every corralled herd
## (`animal:pen` is the top of the animal branch, and the sim publishes nothing there on purpose).
## The card shipped quoting `build_material_cost` and so stated no pile at all, which made this
## paragraph's claim aspirational for a whole slice; the pen-rung key is what makes it true.
##
## The herd's `pen_extend_cost` is NOT read here — the sim stamps it only once a ring is accruing, so
## it is the in-flight meter's denominator and says nothing about a ring nobody has declared.
##
## `band` rides in for the shelf, exactly as `track` takes it: the stall warning weighs the pile
## against what this band actually holds, which the SOURCE cannot answer for.
static func ring_row(source: Dictionary, prefix: String, band: Dictionary) -> Dictionary:
    var verb := SourceForecast.IMPROVEMENT_CORRAL
    return {
        ROW_IMPROVEMENT_KEY: verb,
        ROW_NAME_KEY: HudWorkVocab.RING_CARD_ROW_NAME,
        ROW_WORK_KEY: SourceForecast.build_work_cost(source, prefix, verb),
        ROW_TURNS_KEY: SourceForecast.BUILD_TURNS_NO_ESTIMATE,
        ROW_BUILD_ASIDES_KEY: _build_price_asides(
            SourceForecast.corral_build_material_cost(source, prefix), _material_store(band)),
        ROW_HOLD_ASIDES_KEY: _hold_price_asides(source, prefix, verb),
    }

## …and the ring card as CONTROLS — a title, one priced button, and the same asides in the same order
## the track puts them in. Deliberately NOT `build_track`: that walks a branch and states a position,
## and this states neither.
static func build_ring_card(row: Dictionary, on_declare: Callable) -> VBoxContainer:
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    column.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    var title := Label.new()
    title.text = HudWorkVocab.RING_CARD_TITLE
    title.add_theme_color_override("font_color", HudStyle.INK_DIM)
    title.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_TITLE_FONT_SIZE)
    column.add_child(title)
    # **THE ROW IS THE TRACK'S OWN ROW BUILDER**, given a selectable row, so the ring's line and a
    # rung's line cannot come out at two type sizes or two paddings on cards a player reads one after
    # the other.
    var line := row.duplicate()
    line[ROW_STATE_KEY] = STATE_OPEN
    line[ROW_SELECTABLE_KEY] = true
    line[ROW_REASONS_KEY] = [] as Array[String]
    column.add_child(_build_row(line, func(_verb: String) -> void: on_declare.call()))
    for aside in (row.get(ROW_BUILD_ASIDES_KEY, []) as Array):
        column.add_child(build_aside(String((aside as Dictionary).get(RUNG_ASIDE_TEXT_KEY, "")),
            bool((aside as Dictionary).get(RUNG_ASIDE_WARN_KEY, false))))
    for aside in (row.get(ROW_HOLD_ASIDES_KEY, []) as Array):
        column.add_child(build_aside(String((aside as Dictionary).get(RUNG_ASIDE_TEXT_KEY, "")),
            bool((aside as Dictionary).get(RUNG_ASIDE_WARN_KEY, false))))
    return column

## One rung's line: its name on the left, its state or its price on the right.
static func _build_row(row: Dictionary, on_pick: Callable) -> Control:
    var state := String(row.get(ROW_STATE_KEY, ""))
    var selectable := bool(row.get(ROW_SELECTABLE_KEY, false)) and on_pick.is_valid()
    var line := HBoxContainer.new()
    line.set_meta(HudWorkVocab.RUNG_TRACK_ROW_META, String(row.get(ROW_IMPROVEMENT_KEY, "")))
    line.set_meta(HudWorkVocab.RUNG_TRACK_STATE_META, state)
    # …and the rung KEY beside them, which is the only handle unique on the ROUTE branch: two of its
    # rungs declare no verb at all, so the meta above cannot tell them apart.
    line.set_meta(HudWorkVocab.RUNG_TRACK_RUNG_META, String(row.get(ROW_RUNG_KEY, "")))
    var tint := _state_color(state)
    var name := Label.new()
    name.text = String(row.get(ROW_NAME_KEY, ""))
    name.clip_text = true
    name.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    name.custom_minimum_size = Vector2(float(row.get(ROW_NAME_WIDTH_KEY,
        HudWorkVocab.RUNG_TRACK_NAME_WIDTH)), 0.0)
    name.add_theme_color_override("font_color", tint)
    name.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE)
    name.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(name)
    var face := _row_face(row, state)
    # **THE HOVER GOES ON THE LINE AND ON THE FACE BOTH.** A `Button` is `MOUSE_FILTER_STOP` and
    # answers a hover with its OWN tooltip, so a tooltip left only on the row would never show over
    # the one control the pointer is actually aiming at.
    var hover := String(row.get(ROW_TOOLTIP_KEY, ""))
    line.tooltip_text = hover
    if not selectable:
        var value := Label.new()
        value.text = face
        value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        value.add_theme_color_override("font_color", tint)
        value.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE)
        # ⛔ **IGNORE, so the hover falls through to the LINE that carries the tooltip.** A Label
        # that swallowed the pointer would answer with its own empty tooltip and show nothing.
        value.mouse_filter = Control.MOUSE_FILTER_IGNORE
        line.add_child(value)
        return line
    var pick := Button.new()
    pick.text = face
    pick.focus_mode = Control.FOCUS_NONE
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(pick, "ghost")
    # The board's own row treatment — the default button chrome is nearly two lines tall, which on a
    # three-rung track is half the card.
    HudWidgets.compact(pick, HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    # AFTER `apply_button`, which writes its own `font_color`: a rung already on the chosen path keeps
    # the cyan that says so, and the hover brightening stays as the affordance.
    pick.add_theme_color_override("font_color", tint)
    pick.tooltip_text = hover
    var verb := String(row.get(ROW_IMPROVEMENT_KEY, ""))
    pick.pressed.connect(func() -> void: on_pick.call(verb))
    line.add_child(pick)
    return line

## An aside beneath a row, in the quiet ink a gate reason takes everywhere else in this client — used
## for a locked rung's REASON, for a crop row's own price-and-payoff FACE, and for the two PRICE
## asides a rung above the standing one carries, which are four different statements in one shape
## rather than one statement with four meanings.
##
## `warn` is the ONE aside on this card that is not quiet — the stall warning, which says the band's
## shelf will not carry the build it is about to order. Defaulted false, so the reason and crop-face
## callers are untouched and the quiet register stays the rule rather than the exception.
## **ONE ASIDE — the small wrapped line beneath a row.** Public because the ROUTE card's abandon row
## states its consequence in exactly this register and must not draw a second small line of its own:
## an aside is an aside on any surface this ladder builds.
static func build_aside(text: String, warn: bool = false) -> Label:
    var label := Label.new()
    label.text = text
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    label.add_theme_color_override("font_color", HudStyle.WARN if warn else HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_REASON_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    return label

# ---- THE PRICE ASIDES — the material half of the ladder (`docs/plan_standing_upkeep.md` §2.7) -----

## **WHAT RAISING THIS RUNG EATS, AND WHETHER THE SHELF WILL CARRY IT** — one aside for the pile, a
## second in WARN ink when the store cannot cover it, and `[]` when the rung swallows no material at
## all (which is every rung on the shipped ladder but `animal:pen`).
##
## ⛔ **AN EMPTY PILE IS "THIS RUNG EATS NOTHING", NOT "NOTHING IS KNOWN"** — the `MaterialPayoff`
## contract this whole wire follows — so it renders NO aside rather than a `0` or a dash. A rung that
## eats nothing must not read as one that eats badly.
static func _build_price_asides(pile: Array[Dictionary],
        store: Dictionary) -> Array[Dictionary]:
    var asides: Array[Dictionary] = []
    if pile.is_empty():
        return asides
    asides.append({
        RUNG_ASIDE_TEXT_KEY: HudWorkVocab.RUNG_TRACK_BUILD_MATERIAL_FORMAT % _material_terms(pile),
        RUNG_ASIDE_WARN_KEY: false,
    })
    # **THE FRACTION IS THE WORST GOOD'S, AND THE CLAUSE NAMES THAT GOOD.** A build draws every good
    # in its pile at one coverage fraction, so the one the shelf runs out of first is what the whole
    # build stalls at — and a summed "you have 8 things" would be the currency this model refuses.
    var worst := ""
    var worst_share := 1.0
    for row in pile:
        var id := String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))
        var wanted := float(row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        if wanted < SourceForecast.MATERIAL_FLOW_MIN:
            continue
        var share := float(store.get(id, 0.0)) / wanted
        if share < worst_share:
            worst_share = share
            worst = id
    if worst == "":
        return asides
    asides.append({
        RUNG_ASIDE_TEXT_KEY: HudWorkVocab.RUNG_TRACK_STALL_FORMAT % [
            _material_term(worst, float(store.get(worst, 0.0))),
            _stall_fraction_word(worst_share)],
        RUNG_ASIDE_WARN_KEY: true,
    })
    return asides

## **WHAT HOLDING THIS RUNG COSTS EVERY TURN** — the work rate and, beside it, the goods. One aside on
## every rung above the standing one, materials or not: the row's own face states a one-off price, and
## the second half of what the player is agreeing to has nowhere else to be said.
##
## ⛔ **BOTH TERMS ARE THE OFFERED RUNG'S OWN RATE, and the SOURCE'S current bill is a different
## question.** `build_upkeep_demand` and `build_upkeep_material_demand` both answer per RUNG — what
## THIS rung would cost to hold, at every fullness, on a rung nobody has started. That is the only
## question a player deciding whether to commit can act on, so neither clause may read the stamped
## `upkeep_material_demand` / `upkeep_demand` pair, which says what the source was BILLED this turn
## through the rung it already stands on. **On a source mid-climb the two disagree, and that is
## correct** — do not reconcile them.
##
## Reading the stamp for the goods clause is what this aside used to do, and it silenced the clause on
## the one row in the game that has a good to state: a **pastoral herd looking at the Pen row**, whose
## current rung (`animal:pastoral`) declares no material at all. `animal:pen` is the top of its branch
## so no track ever offers it from a penned herd, which left the material half unreachable in play.
##
## ⛔ **EMPTY MEANS THE RUNG EATS NO MATERIAL, never zero of something** — an empty quote renders NO
## material clause at all and the aside still states its work term.
##
## `[]` where the rung is free to hold in both currencies — a `0 work a turn` bill reads as a defect,
## and a rung that costs nothing should say nothing rather than say nothing twice.
static func _hold_price_asides(source: Dictionary, prefix: String,
        improvement: String) -> Array[Dictionary]:
    var asides: Array[Dictionary] = []
    var terms := _price_terms(
        SourceForecast.build_upkeep_demand(source, prefix, improvement),
        SourceForecast.build_upkeep_material_demand(source, prefix, improvement))
    if terms.is_empty():
        return asides
    asides.append({
        RUNG_ASIDE_TEXT_KEY: HudWorkVocab.RUNG_TRACK_HOLD_FORMAT % HudWorkVocab
            .RUNG_TRACK_PRICE_SEPARATOR.join(terms),
        RUNG_ASIDE_WARN_KEY: false,
    })
    return asides

## **WHAT THIS SOURCE IS BILLED TO STAND, PER TURN, AS TERMS** — `["1 work", "0.05 hurdles"]`, `[]`
## for a source that owes nothing at all. **`[]` IS THE WILD ANSWER AND IT IS THE GATE**: a source on
## no standing rung has no improvement to keep, so the work board inspector draws no Upkeep picker and
## no bill rather than a picker over a job that does not exist.
##
## ⛔ **ONE PRODUCER FOR *DOES THIS SITE OWE UPKEEP* AND FOR *WHAT DOES IT OWE*, deliberately.** The
## gate is `is_empty()` on the very list the line renders, so the picker cannot draw beside a bill that
## says nothing, nor a bill sit under a picker that did not draw. A re-derived rung test (`is_field`,
## `corralled`, a `domestication` threshold) would be a SECOND authority over a question
## `SourceForecast` already answers — and would additionally get the mid-climb case wrong: a source
## raising its FIRST rung stands on `wild` and is already billed, which is exactly what the stamped
## terms say and a rung word cannot.
##
## ⛔ **BOTH CURRENCIES, AND EITHER ONE ALONE IS ENOUGH.** A rung may owe work, goods, or both; a
## keeping kit speeds the work half even where the material half is zero, so the disjunction is what
## decides whether the picker is worth offering. The material half is `[]` on every shipped rung but
## `animal:pen` (`SourceForecast.FORECAST_UPKEEP_MATERIAL_DEMAND_KEY`), which is why reading only the
## work account would still have been right today and wrong on the next rung that eats a good.
##
## ⛔ **THE STAMPED PAIR, NEVER THE PER-RUNG QUOTE.** `_hold_price_asides` above answers *what would
## this rung cost to hold* for a rung nobody has started, and `SourceForecast.upkeep_state`'s own ⛔
## forbids reading one as the other — on a source mid-climb the two disagree by design. This one
## answers *what is this source billed right now*, which is the present-tense question the inspector's
## line asks and the only one a site already standing can be asked.
static func upkeep_price_terms(source: Dictionary, prefix: String) -> Array[String]:
    return _price_terms(
        float(SourceForecast.upkeep_state(source, prefix).get(
            "demand", SourceForecast.NO_UPKEEP_DEMAND)),
        SourceForecast.upkeep_material_demand(source, prefix))

## The work term and the goods terms of ONE bill, in that order — the shape both price readings take,
## so a rate quoted on the track and the same rate quoted on the work board cannot be worded two ways.
##
## ⛔ **A ZERO WORK RATE STATES NO WORK TERM AND AN EMPTY GOODS LIST STATES NO GOODS TERM** — the
## `MaterialPayoff` contract's own rule, one scope up: `0 work a turn` reads as a defect, and a bill
## that costs nothing in a currency should say nothing rather than say nothing twice.
static func _price_terms(work: float, goods: Array[Dictionary]) -> Array[String]:
    var terms: Array[String] = []
    if work >= SourceForecast.UPKEEP_WORK_MIN:
        terms.append(HudWorkVocab.RUNG_TRACK_HOLD_WORK_TERM % DetailFormat.format_work_units(work))
    terms.append_array(_material_term_list(goods))
    return terms

## The band's shelf as `{material_id: amount}` — the ONE place `material_store` is turned into a
## lookup, so the stall test reads a good's own stock and never a total across goods.
static func _material_store(band: Dictionary) -> Dictionary:
    var held := {}
    for row in SourceForecast.material_payoff_rows(band.get(
            DetailFormat.BAND_MATERIAL_STORE_KEY, [])):
        held[String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))] = float(
            row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    return held

## A per-good list joined for one clause — `6 hurdles · 2 rope`. **Never a sum**: one term per good is
## the whole contract, and a total is the retired trade axis under a new name.
static func _material_terms(rows: Array[Dictionary]) -> String:
    return HudWorkVocab.RUNG_TRACK_PRICE_SEPARATOR.join(_material_term_list(rows))

static func _material_term_list(rows: Array[Dictionary]) -> Array[String]:
    var terms: Array[String] = []
    for row in rows:
        var amount := float(row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        if amount < SourceForecast.MATERIAL_FLOW_MIN:
            continue
        terms.append(_material_term(String(row.get(
            SourceForecast.MATERIAL_PAYOFF_ID_KEY, "")), amount))
    return terms

## ONE good's amount and its name. **A material names itself** — the catalogue ships no display word,
## so the id IS the noun, which is the rule every material readout in this client follows. The amount
## is trimmed rather than fixed, so a whole pile reads `6` and a mending rate reads `0.05`.
static func _material_term(material_id: String, amount: float) -> String:
    return HudWorkVocab.RUNG_TRACK_MATERIAL_TERM % [
        DetailFormat.format_trimmed(amount, HudWorkVocab.RUNG_TRACK_MATERIAL_DECIMALS),
        material_id]

## **HOW FAR THE SHELF GETS, IN WORDS** — the NEAREST of `RUNG_TRACK_STALL_STEPS`' own values, so the
## ladder is the words themselves and there is no threshold table beside them to drift. Under the
## lowest named fraction's own nearest-neighbour boundary the answer is `barely at all`: a store
## covering a fortieth of a pile is nearer to nothing than to a tenth.
static func _stall_fraction_word(share: float) -> String:
    var steps: Array = HudWorkVocab.RUNG_TRACK_STALL_STEPS
    var lowest := float((steps[0] as Array)[0])
    if share < lowest * 0.5:
        return HudWorkVocab.RUNG_TRACK_STALL_BARELY
    var best := String((steps[0] as Array)[1])
    var best_gap := absf(share - lowest)
    for step in steps:
        var gap := absf(share - float((step as Array)[0]))
        if gap < best_gap:
            best_gap = gap
            best = String((step as Array)[1])
    return best

# ---- THE CROP STEP — the second page of the same card --------------------------------------------

## **IS THIS RUNG'S DECLARATION INCOMPLETE WITHOUT A CROP?** — the ONE test a caller asks before
## sending a picked rung straight to the command builder.
##
## **PLANT RUNGS ONLY, and the asymmetry is the model rather than an unfinished half.** `tame` and
## `corral` commit no species at all: an animal rung names the herd it is raised on and there is
## nothing left to choose, so making them ask would be a click that answers nothing.
static func rung_commits_a_crop(improvement: String) -> bool:
    return RungGates.CROP_LEGALITY_FLAGS.has(improvement)

## **THE CROPS THIS RUNG MAY BE COMMITTED TO, EACH BESIDE WHAT IT PAYS — one row per legal plant,
## `Sim picks` LAST.**
##
## ⛔ **THE PAYOFF IS WHY THIS LIST EXISTS.** A list of names and shares relocates the trap rather than
## closing it: the player picks the dominant plant again, because it looks like the obvious answer,
## and a 100%-tobacco tile is exactly the ground where that is the worst choice available. Every row
## therefore states its FOOD at two decimals **including a zero** (`HudFloraVocab
## .FLORA_CROP_FOOD_CLAUSE_FORMAT`, the one clause here exempt from the render-only-when-non-zero
## rule), its fodder when it pays any, and ONE CLAUSE PER MATERIAL — never a summed materials figure,
## which is the retired trade axis under a new name.
##
## ⛔ **AND ON THE SOW RUNG EVERY ROW STATES ITS OWN PRICE.** The work half used to be one figure per
## PATCH — the patch's `field_work_cost`, struck against its commitment or the rung's auto-pick — so
## every crop in this list was quoted the default crop's price while the payoffs beside them moved,
## which defeats a list that exists to weigh work against payoff. The per-crop figure is on the wire
## (`CROP_SOW_WORK_COST_KEY`) and is quoted AS PUBLISHED: **neither number here is derived from the
## other, and neither is derived from `share`** — a committed patch's published share is its
## reweighted one while the price is struck on the tile's own basket, which is what the sim charges
## against.
##
## **A CROP THE WIRE PRICES NO SOW FOR RENDERS NO ROW AT ALL.** Absence is the sim saying this plant
## cannot climb to a Field here, so a row would offer a job that cannot be ordered — and a `0` in its
## place would read as a free Sow. It is also what keeps `Sim picks` honest: the rows left are exactly
## the plants `default_species_for_rung` chooses between, so the first of them IS what it resolves to.
##
## **THE FIGURES ARE THE RUNG'S OWN, read off the composition entry.** `sow_payoff` /
## `cultivate_payoff` and their fodder and material twins are what THIS rung would pay once it stands,
## per plant, on this ground; the per-biomass rates beside them describe the wild stand being gathered
## today and would answer a different question. The two rungs differ in KIND rather than by a factor
## (a sown Field is 100% its crop; a tended patch keeps its volunteers), so one never implies the
## other and the rung is passed in rather than guessed.
##
## `[]` where the rung commits no crop at all, or where the basket carries no plant it may legally
## take — the caller declares outright in both cases, which is what the sim does with an absent token.
static func crop_choices(source: Dictionary, prefix: String,
        improvement: String) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var flag := String(RungGates.CROP_LEGALITY_FLAGS.get(improvement, ""))
    if flag == "":
        return rows
    var sow := improvement == SourceForecast.IMPROVEMENT_SOW
    for entry in SourceForecast.flora_basket_entries(
            source.get(prefix + CROP_COMPOSITION_KEY, [])):
        if not bool(entry.get(flag, false)):
            continue
        # The wire's own refusal, and only the Sow rung has a price to be refused: a Cultivate's
        # build cost is unscaled, so no per-crop work figure exists for it and none is asked for.
        if sow and not bool(entry.get(CROP_HAS_SOW_WORK_COST_KEY, false)):
            continue
        rows.append({
            CROP_SPECIES_KEY: String(entry.get("species", "")),
            CROP_LABEL_KEY: _crop_label(entry),
            CROP_FACE_KEY: _crop_face(entry, improvement),
        })
    if rows.is_empty():
        return rows
    # **`Sim picks` IS LAST AND NAMES WHAT IT WOULD LAND ON.** Leading the list is what a hurried
    # player takes, and the whole defect was a default nobody had to look at; stating the plant it
    # resolves to (the first legal entry, the wire's share-DESC order being the sim's own auto-pick)
    # makes it exactly as loud about the consequence as the rows above it.
    rows.append({
        CROP_SPECIES_KEY: CROP_SIM_PICKS,
        CROP_LABEL_KEY: HudWorkVocab.RUNG_CROP_SIM_PICKS_LABEL,
        CROP_FACE_KEY: HudWorkVocab.RUNG_CROP_SIM_PICKS_NOTE_FORMAT % String(
            (rows[0] as Dictionary).get(CROP_LABEL_KEY, "")),
    })
    return rows

## `🌾 Wild Emmer 62%` — the plant written EXACTLY as the tile card and the compose sheet's chips
## write it, composed from those surfaces' own consts rather than a second spelling. One plant reads
## one way in this client, or the card and this list start disagreeing about the same stand.
static func _crop_label(entry: Dictionary) -> String:
    var name := HudFloraVocab.FLORA_SHARE_FORMAT % [
        String(entry.get("display_name", "")), int(entry.get("percent", 0))]
    var role := FoodIcons.for_crop_role(String(entry.get("role", "")))
    return name if role == "" else "%s %s" % [role, name]

## What committing THIS plant on THIS rung would COST, and what it would then pay, per turn, per
## account — `38 work · 0.00 food · 1.12 tobacco`.
##
## **THE COST LEADS**, because it is the figure two rows of one picker most differ by and because a
## price is read before the payoffs it is weighed against. It renders on the Sow rung alone: a
## Cultivate's build cost is unscaled, so the wire prices no per-crop figure for it and a clause here
## would have to invent one.
static func _crop_face(entry: Dictionary, improvement: String) -> String:
    var sow := improvement == SourceForecast.IMPROVEMENT_SOW
    var face := ""
    if sow:
        # Quoted, never re-derived. `crop_choices` has already refused every row without the key, so
        # this read can never fall back to a zero that would advertise a free Sow.
        face += HudFloraVocab.FLORA_CROP_WORK_CLAUSE_FORMAT % DetailFormat.format_work_units(
            float(entry.get(CROP_SOW_WORK_COST_KEY, 0.0)))
    # **THE FOOD CLAUSE RENDERS AT ZERO.** See `crop_choices` — a cash crop's payoff is exactly `0`,
    # and the unstated zero is the whole reason this step exists.
    face += HudFloraVocab.FLORA_CROP_FOOD_CLAUSE_FORMAT % float(
        entry.get("sow_payoff" if sow else "cultivate_payoff", 0.0))
    var fodder := float(entry.get("sow_fodder_payoff" if sow else "cultivate_fodder_payoff", 0.0))
    if SourceForecast.has_component(fodder):
        face += HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT % fodder
    for row_variant in (entry.get(
            "sow_material_payoff" if sow else "cultivate_material_payoff", []) as Array):
        var row: Dictionary = row_variant
        var amount := float(row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        if not SourceForecast.has_component(amount):
            continue
        face += HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT % [
            amount, String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))]
    # The leading separator belongs between clauses, not before the first one.
    return face.trim_prefix(HudFloraVocab.FLORA_CROP_CLAUSE_LEAD)

## **THE CROP STEP AS CONTROLS** — a pressable row per crop over its price-and-payoff aside, and a
## Back row.
##
## `on_pick` takes the row's SPECIES key, which is what the command carries; `on_back` returns to the
## rung list. Both are `Callable`s, `build_track`'s own treatment — this layer holds no state.
## **`improvement` NAMES THE RUNG BEING COMMITTED, and it picks the title.** A Cultivate weeds toward
## a plant already standing; only a Sow plants one. See `HudWorkVocab.RUNG_CROP_TITLE_TEND`.
static func build_crop_step(rows: Array[Dictionary], on_pick: Callable,
        on_back: Callable, improvement: String) -> VBoxContainer:
    var column := VBoxContainer.new()
    column.set_meta(HudWorkVocab.RUNG_CROP_STEP_META, true)
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    column.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    var title := Label.new()
    title.text = HudWorkVocab.RUNG_CROP_TITLE_GROW \
        if improvement == SourceForecast.IMPROVEMENT_SOW \
        else HudWorkVocab.RUNG_CROP_TITLE_TEND
    title.add_theme_color_override("font_color", HudStyle.INK_DIM)
    title.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_TITLE_FONT_SIZE)
    column.add_child(title)
    if rows.is_empty():
        column.add_child(build_aside(HudWorkVocab.RUNG_CROP_NONE_NOTE))
    for row in rows:
        column.add_child(_build_crop_row(row, on_pick))
        column.add_child(build_aside(String(row.get(CROP_FACE_KEY, ""))))
    column.add_child(_build_back_row(on_back))
    return column

## One crop's pressable row. Full width — a crop name plus its share is the whole face and there is no
## second column to line up, unlike a rung's name-beside-its-price.
static func _build_crop_row(row: Dictionary, on_pick: Callable) -> Button:
    var pick := Button.new()
    pick.text = String(row.get(CROP_LABEL_KEY, ""))
    pick.set_meta(HudWorkVocab.RUNG_CROP_ROW_META, String(row.get(CROP_SPECIES_KEY, "")))
    pick.focus_mode = Control.FOCUS_NONE
    pick.alignment = HORIZONTAL_ALIGNMENT_LEFT
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(pick, "ghost")
    HudWidgets.compact(pick, HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    var species := String(row.get(CROP_SPECIES_KEY, ""))
    pick.pressed.connect(func() -> void: on_pick.call(species))
    return pick

static func _build_back_row(on_back: Callable) -> Button:
    var back := Button.new()
    back.text = HudWorkVocab.RUNG_CROP_BACK_LABEL
    back.focus_mode = Control.FOCUS_NONE
    back.alignment = HORIZONTAL_ALIGNMENT_LEFT
    back.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(back, "ghost")
    HudWidgets.compact(back, HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    back.add_theme_color_override("font_color", HudStyle.INK_DIM)
    back.pressed.connect(func() -> void: on_back.call())
    return back

## **WHAT THE RIGHT-HAND SLOT SAYS.** A rung at or below where the source stands states its STATE and
## no figure at all — that is the receipt-not-discount property, rendered: a banked rung is not a
## discount to be quoted, it is a thing already bought.
static func _row_face(row: Dictionary, state: String) -> String:
    # ⛔ **A FINISHED FACE WINS, and only the route branch supplies one.** That branch's face is
    # `<figure> · <nearest refusal>` — a composition its PRODUCER makes, since it alone knows which of
    # several refusals is nearest — so deriving one here from the state would overwrite it with the
    # single word this cut removed.
    var supplied := String(row.get(ROW_FACE_KEY, ""))
    if supplied != "":
        return supplied
    match state:
        STATE_BANKED: return HudWorkVocab.RUNG_TRACK_STATE_BANKED
        STATE_STANDING: return HudWorkVocab.RUNG_TRACK_STATE_STANDING
        STATE_LOCKED: return HudWorkVocab.RUNG_TRACK_STATE_LOCKED
        # **A ROUTE RUNG NOBODY DECLARES STATES THE CAUSE, NOT A PRICE.** It carries a real
        # `workCost` of zero and a `locked` face would read as a refusal; what is true is that
        # traffic raises it and there is no order to give.
        STATE_UNORDERED: return HudWorkVocab.RUNG_TRACK_STATE_WORN_IN
    var work := float(row.get(ROW_WORK_KEY, WORK_UNKNOWN))
    if work <= WORK_UNKNOWN:
        # The wire prices no such job on this source. The state word is the whole of what is known.
        return _state_word(state)
    var turns := DetailFormat.build_turns_clause(int(row.get(ROW_TURNS_KEY,
        SourceForecast.BUILD_TURNS_NO_ESTIMATE)))
    if turns == "":
        return HudWorkVocab.RUNG_TRACK_COST_UNDATED_FORMAT % DetailFormat.format_work_units(work)
    return HudWorkVocab.RUNG_TRACK_COST_FORMAT % [DetailFormat.format_work_units(work), turns]

## **EVERY STATE `_row_face` CAN REACH HAS AN ARM HERE, and the third one is why.** `open` had none —
## there was no `RUNG_TRACK_STATE_OPEN` to return — so an open rung the wire prices no job on rendered
## a **`Button` with no text that still emitted its `tame`/`corral` declaration on press**, which is
## the worst shape a control can take. Reachable through a herd the snapshot no longer carries: the
## source resolves to `{}`, `husbandry_ceiling` defaults to the pen so no outright bar fires, and
## `build_work_cost` reads nothing.
##
## **The word rather than a refusal to build the row**, because a track exists to state the WHOLE
## branch: dropping a selectable rung reads as a shorter ladder, which is the same failure a hidden
## locked rung would be, and the destination is real whether or not this client can price it. With all
## three arms present the `""` below is unreachable — it is the compile-time else, not a state.
static func _state_word(state: String) -> String:
    match state:
        STATE_PATH: return HudWorkVocab.RUNG_TRACK_STATE_PATH
        STATE_TARGET: return HudWorkVocab.RUNG_TRACK_STATE_TARGET
        STATE_OPEN: return HudWorkVocab.RUNG_TRACK_STATE_OPEN
    return ""

## The row's ink. `SIGNAL` is what this HUD spells a LIVE state in — the rung you stand on and the one
## you are heading for — and `SIGNAL_DEEP` is the one step down a rung under construction already
## wears on the work board, which is exactly what a leg on the way is.
static func _state_color(state: String) -> Color:
    match state:
        STATE_BANKED: return HudStyle.INK_FAINT
        STATE_STANDING: return HudStyle.SIGNAL
        STATE_PATH: return HudStyle.SIGNAL_DEEP
        STATE_TARGET: return HudStyle.SIGNAL
        STATE_LOCKED: return HudStyle.INK_DIM
        # A rung above you that you cannot press reads in the quiet ink, whether it is refused or
        # merely unorderable — the WORD beside it is what tells those two apart.
        STATE_UNORDERED: return HudStyle.INK_DIM
    return HudStyle.INK

## **WHERE THE SOURCE STANDS — the HIGHEST banked rung, `0` (the wild floor) when none is.** Walked
## top-down for the reason `BandPanelController._work_source_rung` tests the higher rung first: a
## Field is ALSO cultivated, so a bottom-up walk would stop at the rung beneath the one it stands on.
##
## **`improvement_is_done` IS THE TEST, and it compares the source's STANDING rung at-or-above** — a
## Field sown straight from wild ground reports `is_cultivated` false forever, and its Cultivate is
## still bought and paid for.
static func _standing_index(branch: Array, source: Dictionary, prefix: String) -> int:
    for index in range(branch.size() - 1, 0, -1):
        var verb := String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(String(branch[index]),
            SourceForecast.IMPROVEMENT_NONE))
        if verb != SourceForecast.IMPROVEMENT_NONE \
                and SourceForecast.improvement_is_done(source, prefix, verb):
            return index
    return 0

## Which rung of this branch an improvement VERB names — `-1` for none, which is what
## `IMPROVEMENT_NONE` answers and is why an unqueued source has no path and no target.
static func _index_of_improvement(branch: Array, improvement: String) -> int:
    if improvement == SourceForecast.IMPROVEMENT_NONE:
        return -1
    for index in range(branch.size()):
        if String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(String(branch[index]), "")) == improvement:
            return index
    return -1

## The queued entry's published legs, keyed by their rung so a row can ask for its own. `{}` on an
## unqueued source.
static func _legs_by_rung(source: Dictionary, prefix: String) -> Dictionary:
    var by_rung := {}
    for leg in SourceForecast.build_legs(source, prefix):
        by_rung[String(leg.get(SourceForecast.BUILD_LEG_RUNG_KEY, ""))] = leg
    return by_rung

## **WHAT THIS RUNG STILL OWES, FROM WHERE THE SOURCE STANDS.** The queued entry's own leg where it
## publishes one — that figure is the sim's and is what the whole feature reads — and otherwise the
## per-rung `workCost − workDone` pair the wire publishes for exactly this pre-commit question.
##
## `WORK_UNKNOWN` where the wire prices no such job on this source, which every face renders as no
## figure rather than as a free rung.
static func _work_remaining(source: Dictionary, prefix: String, improvement: String,
        legs: Dictionary, rung_key: String) -> float:
    if legs.has(rung_key):
        return float((legs[rung_key] as Dictionary).get(SourceForecast.BUILD_LEG_WORK_KEY, 0.0))
    var cost := SourceForecast.build_work_cost(source, prefix, improvement)
    if cost <= SourceForecast.BUILD_WORK_COST_NONE:
        return WORK_UNKNOWN
    return maxf(cost - SourceForecast.build_work_done(source, prefix, improvement), 0.0)

## The leg's CHAINED date, and `BUILD_TURNS_NO_ESTIMATE` where no entry covers this rung. **There is
## no pair to fall back on here**: a date is chained against a build queue the client cannot see, so
## an unqueued rung honestly has none.
static func _leg_turns(legs: Dictionary, rung_key: String) -> int:
    if not legs.has(rung_key):
        return SourceForecast.BUILD_TURNS_NO_ESTIMATE
    return int((legs[rung_key] as Dictionary).get(SourceForecast.BUILD_LEG_TURNS_KEY,
        SourceForecast.BUILD_TURNS_NO_ESTIMATE))

## The rung's own NAME — `DetailFormat.rung_badge_word`'s word behind the glyph the work row's rung
## mark already draws, so the track and the board name one rung one way.
static func _rung_name(improvement: String) -> String:
    if improvement == SourceForecast.IMPROVEMENT_NONE:
        return HudWorkVocab.RUNG_TRACK_WILD_NAME
    return HudWorkVocab.RUNG_TRACK_NAME_FORMAT % [
        FoodIcons.for_policy(improvement), DetailFormat.rung_badge_word(improvement)]

## The KNOWLEDGE and SOURCE gates, through `RungGates` — the ONE definition, so the track, the `⌃`
## mark, the map badge and the compose sheet cannot disagree about what is climbable.
static func _gates(kind: String, source: Dictionary, prefix: String,
        knowledge: Dictionary) -> Dictionary:
    if kind == SourceForecast.LABOR_KIND_HUNT:
        return RungGates.hunt_gates(source, knowledge)
    return RungGates.forage_gates_from_patch(source, knowledge) \
        if prefix == HudComposeVocab.BARE_FORECAST_PREFIX else RungGates.forage_gates(source, knowledge)

## **EVERY REASON THIS RUNG IS REFUSED — `[]` when it is not.** Two kinds, and the ORDER is the point:
## an OUTRIGHT bar comes first, because *no aurochs will ever be penned* supersedes *your people know
## Penning 45%* — quoting the knowledge on a species that can never use it sends the player to earn a
## craft that buys them nothing here.
static func _rung_refusals(kind: String, source: Dictionary, prefix: String, improvement: String,
        gates: Dictionary) -> Array[String]:
    var outright := _outright_bar(kind, source, prefix, improvement)
    if outright != "":
        return [outright] as Array[String]
    return RungGates.gate_reasons_for(gates, improvement)

## **WHAT THE SPECIES OR THE GROUND WILL NEVER ADMIT** — `""` when the rung is admitted at all.
##
## `RungGates.hunt_rungs_admitted` / `any_crop_allows` WITHHOLD such a rung, which is right for a
## MARK: a mark promises the verb is available. A TRACK says what the branch holds, so the same answer
## has to arrive as a visible refusal instead — the three this function returns, which are
## `HudFloraVocab.GATE_REASON_SPECIES_NEVER_TAMED` / `_NEVER_PENNED` on the animal web and
## `GATE_REASON_CROP_CANNOT_CLIMB_FORMAT` on the plant one.
static func _outright_bar(kind: String, source: Dictionary, prefix: String,
        improvement: String) -> String:
    if kind == SourceForecast.LABOR_KIND_HUNT:
        # **ONLY THE PEN RUNG HAS A CEILING TO CLEAR.** `Tame` is admitted by anything above `wild`,
        # and a `wild`-ceiling herd's whole branch above the floor is barred — which the Corral row
        # states, the Tame row states through the same test, and neither invents a second wording for.
        var ceiling := SourceForecast.husbandry_ceiling(source)
        if improvement == SourceForecast.IMPROVEMENT_CORRAL:
            return "" if ceiling == SourceForecast.HUSBANDRY_CEILING_PEN \
                else HudFloraVocab.GATE_REASON_SPECIES_NEVER_PENNED
        if improvement == SourceForecast.IMPROVEMENT_TAME \
                and ceiling == SourceForecast.HUSBANDRY_CEILING_WILD:
            return HudFloraVocab.GATE_REASON_SPECIES_NEVER_TAMED
        return ""
    var flag := String(RungGates.CROP_LEGALITY_FLAGS.get(improvement, ""))
    if flag == "" or RungGates.any_crop_allows(source, prefix, flag):
        return ""
    return HudFloraVocab.GATE_REASON_CROP_CANNOT_CLIMB_FORMAT \
        % DetailFormat.rung_badge_word(improvement)
