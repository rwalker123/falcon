class_name AttentionController
extends RefCounted

## THE BAND/EXPEDITION ATTENTION PRODUCERS + THEIR JUMP ROUTING (HUD decomposition,
## docs/plan_hud_decomposition.md).
##
## WHAT THIS IS — and the domain split with `TurnOrbController`. The turn orb's attention registry is
## assembled from two halves. `TurnOrbController` owns the orb WIDGET, the snapshot-driven fork
## producer, and the registry ASSEMBLY (`set_band_attention` / `_push_attention` fold the band half +
## the fork half into ONE `TurnOrb.set_attention` and severity-sort). This controller owns the OTHER
## half: PRODUCING the band/expedition attention rows, and ROUTING their "Jump →". `HudLayer.update_band_alerts`
## builds the array through `build_band_attention` and hands it to `_turnorb.set_band_attention(...)`.
##
## THE ATTENTION BUILD MUST RUN BEFORE INGEST — a read-before-write on the previous sizes. Producer 2
## (losing-population) compares each band's current `size` to `_band_labor.prev_band_sizes()`, which
## `_band_labor.ingest_snapshot_bands(...)` OVERWRITES for next turn. So `build_band_attention` runs the
## PURE producers only and reads the pre-ingest `prev_band_sizes`; `update_band_alerts` calls it BEFORE
## it ingests. `build_band_attention` deliberately does NOT ingest — that stays on `HudLayer`.
##
## THE INJECTION SURFACE IS ONE CALLABLE — `_herd_label_for_id` (the pen/expedition rows name a herd by
## species). It stays on `HudLayer` because resolving a herd id reads THREE collaborators (the roster,
## the current selection, and the snapshot herd list), so it cannot fold onto `HudBandLaborState`.
## Reached through the typed adapter below rather than called raw: `Callable.call` returns `Variant`,
## which trips warnings-as-errors. `HudBandLaborState.labor_assignments_of` is a public `static func`, so
## the pen producers call it as a class-name static (exactly how `DetailFormat` reaches it) — no injection.
##
## IT EMITS ITS OWN `alert_focus_requested`; `HudLayer` RELAYS it (the `TurnOrbController` pattern — the
## controller never emits a HudLayer signal). The direct `_bandpanel.select_expedition` /
## `focus_labor_source` calls in `on_turn_orb_focus` stay direct (the controller holds `_bandpanel`).

signal alert_focus_requested(x: int, y: int)

## Sort key for an unworked rung whose source reports NO neglect grace (`has_neglect_grace == false`
## — nothing at risk). It must sort BEHIND every real countdown, and it cannot be the honest `0`,
## which is the wire's "the penalty is biting NOW" and therefore the most urgent value there is. A
## deliberately unreachable turn count rather than `INT_MAX`, so the number reads as what it means.
const NEGLECT_GRACE_ABSENT_SORT := 9999

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
# The digested per-snapshot player world: `prev_band_sizes` (Producer 2), `player_bands` (the pen jump),
# `player_expeditions` (the awaiting jump), and `find_world_herd` (the pen producers).
var _band_labor: HudBandLaborState = null
# The Band/City panel — the orb's expedition/pen jumps reuse its own focus paths.
var _bandpanel: BandPanelController = null

# --- The one retained HudLayer helper, injected as a Callable (see the class header) ---
var _herd_label_for_id_fn: Callable

## **THE BUILD HAND-OFFS THIS TURN** (`docs/plan_standing_upkeep.md` §2.3) — `{label, carried}` rows
## ingested off the command-event stream, held until the turn moves on.
##
## **THEY ARE HELD, not read straight out of the events, because the two arrive on different
## dispatches.** `Main` feeds `command_events` and `populations` separately and the attention array is
## rebuilt from the populations pass, so a producer that read the event array would answer empty on
## every frame but the one the events landed on — and the rows would flicker away mid-turn, which is
## exactly when the player is deciding what to do with those hands.
var _handoffs: Array[Dictionary] = []

## The turn `_handoffs` describes. A hand-off is news about ONE turn, so the list is dropped whole
## when the turn changes rather than accumulating a session's worth of finished builds.
var _handoff_turn: int = -1

func _init(band_labor: HudBandLaborState, bandpanel: BandPanelController,
        herd_label_for_id: Callable) -> void:
    _band_labor = band_labor
    _bandpanel = bandpanel
    _herd_label_for_id_fn = herd_label_for_id

## A friendlier label for a herd id. Retained on HudLayer, which resolves it from the roster, the
## current selection AND the snapshot herd list. Typed adapter: `Callable.call` returns `Variant`.
func _herd_label_for_id(herd_id: String) -> String:
    return String(_herd_label_for_id_fn.call(herd_id))

## Build the band/expedition attention array for THIS snapshot — Producers 1–5, in the exact append
## order the old `update_band_alerts` loop produced. `player_bands` / `player_expeditions` are the
## already-split rosters; this runs BEFORE `HudBandLaborState.ingest_snapshot_bands`, so Producer 2
## still reads the pre-ingest `prev_band_sizes` (the load-bearing ordering — see the class header).
## MUST NOT ingest; that stays in `update_band_alerts`.
##
## The bands-only counter is `i + 1`: the old loop incremented `band_number` once per resident band,
## right after `player_bands.append`, so the Nth resident band's number is its index + 1 — matching
## the band-picker (`i + 1`) and the panel header, all numbered positionally within `player_bands`.
## **EVERY ENTRY NAMES ITS `owner`**, the entity the alert is ABOUT — the band for the five band-scoped
## producers, the party for the awaiting-orders one. The turn orb never needed it (it renders one flat
## list and jumps by `x`/`y`), but the faction page's Work and Parties tabs GROUP by it, and an alert
## that cannot say whose it is cannot be put on a row.
##
## **`x`/`y` is NOT a substitute and must not be used as one.** Two bands can stand on one tile, and a
## starving pen's coordinates are deliberately the HERD's rather than the keeper's — so locating an
## alert and attributing it are different questions with different answers.
##
## The UNWORKED-RUNG producer stamps no owner, and that is a fact about the sim rather than an
## omission: a patch is owned by the FACTION (`patch.owner == PLAYER_FACTION_ID`), never by a band, so
## there is no band whose row it could sit on. `OWNER_NONE` is what it reads as.
const OWNER_NONE := -1

## The two tokens a hand-off event is recognized by, on the sim's own `status=... action=...` detail
## line (`systems::labor`'s completion pass). Matched structurally rather than by the label's words,
## because the label is a sentence written for the player and the detail is the contract.
const HANDOFF_ACTION_TOKEN := "action=build_complete"

const HANDOFF_STATUS_CARRIED := "status=carried_to_upkeep"

const HANDOFF_STATUS_FREED := "status=freed"

## **INGEST THE TURN'S BUILD HAND-OFFS** (`docs/plan_standing_upkeep.md` §2.3) — the completions whose
## crew moved, off the same `command_events` array the Telling and the event dock read.
##
## **THE SIM'S OWN SENTENCE IS KEPT VERBATIM.** It knows which rung finished, how many hands moved and
## where they went (*"3 of your cultivate crew stay on (31, 18) to keep it"*); recomposing it here
## would be a second description of one event, and the two would drift.
##
## The `status=` token is what separates the two outcomes: `carried_to_upkeep` means those hands are
## now paying the rung's keeping, `freed` means they are idle. Anything else on the stream is not a
## hand-off and is left to the surfaces that own it.
func ingest_command_events(events_variant: Variant, turn: int) -> void:
    if not (events_variant is Array):
        return
    if turn != _handoff_turn:
        _handoffs.clear()
        _handoff_turn = turn
    for entry_variant in events_variant:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var detail := String(entry.get("detail", ""))
        if not detail.contains(HANDOFF_ACTION_TOKEN):
            continue
        var carried := detail.contains(HANDOFF_STATUS_CARRIED)
        if not carried and not detail.contains(HANDOFF_STATUS_FREED):
            continue
        _handoffs.append({
            "label": String(entry.get("label", "")).strip_edges(),
            "carried": carried,
        })

## Turn-orb items for the builds that finished this turn and moved their crew (Producer 8) — the
## next-turn attention half of §2.3's *"either way it is announced"*.
##
## **NON-LOCATING** (`x < 0`): the event names its source in words and carries no coordinates, so the
## row reads `Open` rather than promising a jump it would have to guess. Capped like the awaiting
## rows, for the same off-screen-popover reason.
func _crew_handoff_attention() -> Array:
    var items: Array = []
    for i in _handoffs.size():
        var handoff: Dictionary = _handoffs[i]
        if i >= HudAttentionVocab.ATTENTION_HANDOFF_MAX_ROWS:
            items.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_CREW_HANDOFF,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_INFO,
                "label": HudAttentionVocab.ATTENTION_HANDOFF_OVERFLOW_LABEL_FORMAT % (
                    _handoffs.size() - i),
                "detail": HudAttentionVocab.ATTENTION_HANDOFF_OVERFLOW_DETAIL,
                "x": HudAttentionVocab.ATTENTION_NON_LOCATING,
                "y": HudAttentionVocab.ATTENTION_NON_LOCATING,
            })
            break
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_CREW_HANDOFF,
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_INFO,
            "label": String(handoff["label"]),
            "detail": HudAttentionVocab.ATTENTION_HANDOFF_DETAIL_CARRIED \
                if bool(handoff["carried"]) \
                else HudAttentionVocab.ATTENTION_HANDOFF_DETAIL_FREED,
            "x": HudAttentionVocab.ATTENTION_NON_LOCATING,
            "y": HudAttentionVocab.ATTENTION_NON_LOCATING,
        })
    return items

func build_band_attention(player_bands: Array, player_expeditions: Array) -> Array:
    var attention: Array = []
    for i in player_bands.size():
        if not (player_bands[i] is Dictionary):
            continue
        var entry: Dictionary = player_bands[i]
        var band_number := i + 1
        var entity := int(entry.get("entity", -1))
        var size := int(entry.get("size", 0))
        var turns := float(entry.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
        var morale := float(entry.get("morale", 1.0))
        var morale_cause := int(entry.get("morale_cause", DetailFormat.MORALE_CAUSE_NONE))
        var last_emigrated := int(entry.get("last_emigrated", 0))
        var x := int(entry.get("current_x", -1))
        var y := int(entry.get("current_y", -1))
        var band_name := HudFormat.band_display_name(entry, band_number)
        # Producer 1 — starving: larder below the critical threshold (red/critical).
        if BandFoodStatus.is_critical(turns):
            attention.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_STARVING,
                "owner": entity,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_CRITICAL,
                "label": "%s starving" % band_name,
                "detail": DetailFormat.food_turns_text(turns),
                "x": x, "y": y,
            })
        # Producer 2 — losing population: shrank vs the previous snapshot (amber/warn). Reads the
        # PRE-INGEST `prev_band_sizes` — this runs before `update_band_alerts` ingests.
        if _band_labor.prev_band_sizes().has(entity) and size < int(_band_labor.prev_band_sizes()[entity]):
            attention.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_LOSING_POPULATION,
                "owner": entity,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
                "label": "%s losing population" % band_name,
                "detail": _decline_reason(turns, morale, morale_cause, last_emigrated),
                "x": x, "y": y,
            })
        # Producer 3 — idle labor: working-age workers unassigned (amber/warn). Supersedes
        # the old activity==idle alert (a worker count is more actionable than a state flag).
        var idle_workers := int(entry.get("idle_workers", 0))
        if idle_workers > 0:
            attention.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_IDLE_WORKERS,
                "owner": entity,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
                "label": "%d idle worker%s" % [idle_workers, "" if idle_workers == 1 else "s"],
                "detail": band_name,
                "x": x, "y": y,
            })
        # Producer 5 — a starving pen this band keeps (amber/warn; see HudAttentionVocab.ATTENTION_KIND_STARVING_PEN
        # for why it is not critical). Keyed off the band's OWN Corral assignments, never a scan of
        # every herd on the wire: that is what makes it the PLAYER's pen (a herd carries no owner
        # field client-side) and what lets the row name the keeper who has to fix it.
        attention.append_array(_starving_pen_attention(entry))
        # Producer 7 — a MANAGED herd this band keeps with fewer keepers than it demands (amber/warn).
        # Keyed off the band's own hunt assignments for the `_starving_pen_attention` reason: a herd
        # carries no owner field client-side, so a scan of `world_herds()` would alarm on a rival's.
        attention.append_array(_under_crewed_herd_attention(entry, player_bands))
    # Producer 4 — awaiting orders: a detached party parked at its objective, burning provisions
    # until the player acts (amber/warn, same class as idle labor). Runs over the EXPEDITIONS split
    # out above, not the bands — an expedition is never "Band N", so it never enters the band loop.
    attention.append_array(_awaiting_orders_attention(player_expeditions))
    # Producer 6 — a BUILT plant rung with nobody on it. Runs over the PATCHES, outside the band loop,
    # and that is structural rather than stylistic: the whole point is that there is no assignment to
    # hang it on, so there is no band whose roster it could be found through. Patches carry an owner on
    # the wire (herds do not), which is what makes an assignment-free scan attributable here.
    attention.append_array(_unworked_rung_attention(player_bands))
    # Producer 8 — a finished build's crew moved (§2.3). Outside the band loop and fed by the command
    # stream rather than by the roster: a hand-off is an EVENT, and no band field records that it
    # happened.
    attention.append_array(_crew_handoff_attention())
    return attention

## An orb row's "Jump →". A row that locates an AWAITING EXPEDITION routes through the SAME path the
## Band panel's Active-expeditions row click uses (`BandPanelController.select_expedition`: recenter + pin the
## exact expedition so its drawer opens and the panel band isn't hijacked) rather than a second,
## weaker jump that would only recenter the hex and auto-select whatever occupant sits on it. Every
## other producer (band-located) keeps the plain recenter.
func on_turn_orb_focus(x: int, y: int) -> void:
    var exp := _awaiting_expedition_at(x, y)
    if not exp.is_empty():
        _bandpanel.select_expedition(int(exp.get("entity", -1)), x, y)
        return
    # A starving-pen or under-crewed-herd row jumps to the HERD, not just its hex: `focus_labor_source`
    # (the very path the Band panel's Hunt row uses) recenters AND pins the herd, so the drawer that
    # explains the alert — the "⚠ Starving" Corral row, the Pen feed cost, the Herders count — is what
    # actually opens.
    var alerted_herd := _alerted_herd_at(x, y)
    if alerted_herd != "":
        _bandpanel.focus_labor_source(x, y, alerted_herd)
        return
    # An unworked-rung row names the LAND, the forage twin of the herd branch above: the patch's rung
    # rows and its improvement control live on the land card, so a bare recentre would land on the hex
    # and then let its auto-pick open whichever band or herd happens to stand there.
    if _unworked_rung_at(x, y):
        _bandpanel.focus_labor_source(x, y)
        return
    alert_focus_requested.emit(x, y)

## The expedition's OBJECTIVE in words — the herd it follows (hunt) or the tile it is parked on
## (scout) — the "where do I have to go / what is this about" half of an attention row's context.
func _expedition_objective(exp: Dictionary) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        return _herd_label_for_id(String(exp.get("expedition_target_herd", "")).strip_edges())
    return HudAttentionVocab.ATTENTION_TILE_FORMAT % [int(exp.get("current_x", -1)), int(exp.get("current_y", -1))]

## Turn-orb attention items for every expedition parked in `awaiting` (Producer 4). ONE ROW PER
## PARTY — each is its own decision with its own place to go (unlike idle workers, which are
## genuinely one aggregate per band) — capped at HudAttentionVocab.ATTENTION_AWAITING_MAX_ROWS, with the remainder
## folded into a single overflow row that jumps to the first party beyond the cap (so even the
## aggregate row is actionable rather than a dead "Open ▸" stub).
func _awaiting_orders_attention(expeditions: Array) -> Array:
    var awaiting: Array = []
    for exp_variant in expeditions:
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if HudFormat.expedition_phase_key(exp) == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            awaiting.append(exp)
    var items: Array = []
    for i in awaiting.size():
        var exp: Dictionary = awaiting[i]
        var x := int(exp.get("current_x", -1))
        var y := int(exp.get("current_y", -1))
        if i >= HudAttentionVocab.ATTENTION_AWAITING_MAX_ROWS:
            # Overflow: one aggregate row for the rest, locating to this (the first uncapped) party.
            items.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_AWAITING_ORDERS,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
                "label": HudAttentionVocab.ATTENTION_AWAITING_OVERFLOW_LABEL_FORMAT % (awaiting.size() - i),
                "detail": HudAttentionVocab.ATTENTION_AWAITING_OVERFLOW_DETAIL,
                "x": x, "y": y,
            })
            break
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_AWAITING_ORDERS,
            "owner": int(exp.get("entity", -1)),
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
            # The demand headline reuses the phase words ("Awaiting orders"); the context line names
            # the mission + its objective, so the row is actionable without opening anything.
            "label": HudFormat.expedition_phase_label(HudExpeditionVocab.EXPEDITION_PHASE_AWAITING),
            "detail": HudAttentionVocab.ATTENTION_AWAITING_DETAIL_FORMAT % [
                DetailFormat.expedition_mission_label(String(exp.get("expedition_mission", ""))),
                _expedition_objective(exp)],
            "x": x, "y": y,
        })
    return items

## Turn-orb attention items for the STARVING PENS one band keeps (Producer 5). One row per pen — a
## pen is a distinct 25-turn investment with its own herd, its own tile and its own fed fraction, so
## (unlike idle workers) there is nothing meaningful to aggregate. Driven by `PenStatus`, the same
## test the herd drawer and the map badge ask, so the three surfaces cannot disagree.
##
## The pens are found through the band's OWN hunt labor assignments: the client has no owner field
## on a herd, so scanning `_band_labor.world_herds()` would happily alarm on a RIVAL's starving pen.
##
## **THE `policy == "corral"` FILTER IS GONE, AND IT HAD SILENTLY KILLED THIS PRODUCER** (issue #442).
## `policy` is always one of the four STANCES now and the build verb rides its own `improvement` field,
## so the old test could never again be true and no starving pen has been reported since the axes
## split. The filter was only ever the ATTRIBUTION half ("is this pen ours?"), which the assignment
## walk already answers; whether it is a starving PEN is `PenStatus.herd_is_starving`'s job, and that
## reads `pen_fed_fraction`, which only a penned herd carries.
func _starving_pen_attention(band: Dictionary) -> Array:
    var items: Array = []
    for entry in _hunted_herds_of(band):
        var herd_id: String = entry["herd_id"]
        var herd: Dictionary = entry["herd"]
        if not PenStatus.herd_is_starving(herd):
            continue
        var fed := PenStatus.fed_fraction(herd)
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_STARVING_PEN,
            "owner": int(band.get("entity", -1)),
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
            "label": HudAttentionVocab.ATTENTION_PEN_LABEL_FORMAT % _herd_label_for_id(herd_id),
            "detail": HudAttentionVocab.ATTENTION_PEN_DETAIL_FORMAT % int(round(fed * HudConst.PROGRESS_PERCENT_SCALE)),
            # The HERD's live tile — a penned herd is pinned, but the jump must still land on the
            # animals (that is where the drawer with the fed fraction and the feed cost opens),
            # not on the keeper band.
            "x": int(herd.get("x", -1)), "y": int(herd.get("y", -1)),
        })
    return items

## **EVERY HERD THIS BAND HAS HANDS ON**, as `{herd_id, herd}` pairs against the herd's LIVE dict —
## the one walk both herd-side producers and both of their jump lookups share, so "which herds are
## ours" is answered in exactly one place. A herd that has left the visible fauna set is dropped here
## rather than in four callers.
func _hunted_herds_of(band: Dictionary) -> Array:
    var found: Array = []
    for a_variant in HudBandLaborState.labor_assignments_of(band):
        if not (a_variant is Dictionary):
            continue
        var a: Dictionary = a_variant
        if String(a.get("kind", "")).to_lower() != SourceForecast.LABOR_KIND_HUNT:
            continue
        var herd_id := String(a.get("fauna_id", ""))
        # Herds MIGRATE, so the LIVE dict is the authority on where this one is — never the
        # assignment's launch-time target.
        var herd := _band_labor.find_world_herd(herd_id)
        if herd.is_empty():
            continue
        found.append({"herd_id": herd_id, "herd": herd})
    return found

## **THE COUNTDOWN CLAUSE, and the ONE place the wire's three readings are turned into words**
## (issue #442). `neglect_grace_remaining` is shipped as `(grace + 1) - neglect`, so:
##   • `has_neglect_grace == false` → NOTHING IS AT RISK. Render no countdown — that is why the bool
##     exists, since "nothing to lose" and "biting now" are both the number `0`.
##   • `0` → the penalty is biting THIS turn.
##   • `N > 0` → it bites in N more unworked turns.
## `soon` / `now` / `unknown` are the caller's own three phrasings (the plant meter reverts, the herd
## sheds), so the reading is written down once and the vocabulary stays per-web.
static func _neglect_clause(src: Dictionary, prefix: String,
        soon_format: String, now_text: String, unknown_text: String) -> String:
    if not bool(src.get(prefix + "has_neglect_grace", false)):
        return unknown_text
    var remaining := int(src.get(prefix + "neglect_grace_remaining", 0))
    if remaining <= 0:
        return now_text
    return soon_format % [remaining,
        "" if remaining == 1 else HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]

## Turn-orb items for every BUILT plant rung the player owns that nobody is working (Producer 6).
##
## **COMPLETED RUNGS ONLY, and that matches the sim's own announce rule**: `forage::announce_rung_lost`
## fires when a *finished* meter crosses back below complete and stays silent through the partial bleed
## after it, because "the thing that mattered has already happened". A half-built meter reverting is
## already stated where the player is looking at that patch — the tile card's WARN `Reverting N%` row —
## and alarming on every one of them would bury the rows that are about a real loss.
##
## Ownership is the patch's OWN `owner` field, so this scan cannot alarm on a rival's ground. Sorted
## most-urgent-first and capped like the awaiting-orders rows, for the same off-screen-popover reason.
##
## `player_bands` is THIS snapshot's roster, and passing it is load-bearing: the producers run BEFORE
## `ingest_snapshot_bands`, so `HudBandLaborState`'s own roster is still last turn's and a crew the
## client has not ingested yet would read as absent — every improved patch the player works would
## alarm as unworked on the first snapshot after a load.
func _unworked_rung_attention(player_bands: Array) -> Array:
    var candidates: Array = []
    for tile_key in _band_labor.forage_patch_lookup():
        var patch: Dictionary = _band_labor.forage_patch_lookup()[tile_key]
        if not bool(patch.get("has_owner", false)) \
                or int(patch.get("owner", -1)) != HudConst.PLAYER_FACTION_ID:
            continue
        # The HIGHEST built rung names the row, because that is the one the plant web unwinds first —
        # the same newest-first order the wire's own countdown describes.
        var rung := ""
        if bool(patch.get("is_field", false)):
            rung = SourceForecast.IMPROVEMENT_SOW
        elif bool(patch.get("is_cultivated", false)):
            rung = SourceForecast.IMPROVEMENT_CULTIVATE
        if rung == "":
            continue
        var x := int(patch.get("x", -1))
        var y := int(patch.get("y", -1))
        # UNWORKED means no crew from ANY of the player's bands, pending edits included — a patch two
        # bands can reach is worked if either one is on it.
        if int(_band_labor.forage_effort_at(x, y, player_bands).get("workers", 0)) > 0:
            continue
        candidates.append({
            "x": x, "y": y, "rung": rung,
            # Sort key: the wire's countdown, with "nothing at risk" pushed to the back rather than
            # colliding with the biting-now zero.
            "urgency": int(patch.get("neglect_grace_remaining", 0)) \
                if bool(patch.get("has_neglect_grace", false)) else NEGLECT_GRACE_ABSENT_SORT,
            "clause": _neglect_clause(patch, HudComposeVocab.BARE_FORECAST_PREFIX,
                HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT,
                HudAttentionVocab.ATTENTION_LAPSE_NOW,
                HudAttentionVocab.ATTENTION_LAPSE_UNKNOWN),
        })
    candidates.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
        return int(a["urgency"]) < int(b["urgency"]))
    var items: Array = []
    for i in candidates.size():
        var c: Dictionary = candidates[i]
        if i >= HudAttentionVocab.ATTENTION_UNWORKED_MAX_ROWS:
            items.append({
                "kind": HudAttentionVocab.ATTENTION_KIND_UNWORKED_RUNG,
                "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
                "label": HudAttentionVocab.ATTENTION_UNWORKED_OVERFLOW_LABEL_FORMAT % (
                    candidates.size() - i),
                "detail": HudAttentionVocab.ATTENTION_UNWORKED_OVERFLOW_DETAIL,
                "x": int(c["x"]), "y": int(c["y"]),
            })
            break
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_UNWORKED_RUNG,
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
            "label": HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
                String(HudComposeVocab.IMPROVEMENT_DONE_LABELS.get(String(c["rung"]), "")),
                int(c["x"]), int(c["y"])],
            "detail": String(c["clause"]),
            "x": int(c["x"]), "y": int(c["y"]),
        })
    return items

## Turn-orb items for the MANAGED herds one band keeps with fewer keepers than the sim demands
## (Producer 7) — the animal half of "you built this and then walked away".
##
## **UNDER-CREWED, NOT UNWORKED, and the difference is what the client can attribute.** A herd carries
## no owner field, so a herd with NO assignment cannot be tied to the player at all (the same reason
## `_starving_pen_attention` walks assignments rather than `world_herds()`); a band that abandons its
## herd outright is told by the sim's own `too_few_workers` / lapse feed line instead. What is
## attributable — and is the common, quiet case — is a standing crew that has fallen below
## `herders_needed`, which is exactly when the shed clock starts.
##
## `player_bands` is THIS snapshot's roster, for the `_unworked_rung_attention` reason: the keeper
## count is folded over it rather than over the not-yet-ingested one, or the row quotes LAST turn's
## crew — and reads `0 of 4` for a band the client is seeing for the first time.
func _under_crewed_herd_attention(band: Dictionary, player_bands: Array) -> Array:
    var items: Array = []
    for entry in _hunted_herds_of(band):
        var herd_id: String = entry["herd_id"]
        var herd: Dictionary = entry["herd"]
        if not _herd_is_under_crewed(herd_id, herd, player_bands):
            continue
        var staffed := int(_band_labor.hunt_effort_on(herd_id, player_bands).get("workers", 0))
        var needed := int(herd.get("herders_needed", 0))
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_UNDER_CREWED_HERD,
            "owner": int(band.get("entity", -1)),
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
            "label": HudAttentionVocab.ATTENTION_UNDER_CREWED_LABEL_FORMAT % _herd_label_for_id(herd_id),
            "detail": HudAttentionVocab.ATTENTION_UNDER_CREWED_DETAIL_FORMAT % [
                staffed, needed,
                _neglect_clause(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
                    HudAttentionVocab.ATTENTION_SHED_SOON_FORMAT,
                    HudAttentionVocab.ATTENTION_SHED_NOW,
                    HudAttentionVocab.ATTENTION_LAPSE_UNKNOWN)],
            # The HERD's live tile — herds migrate, and the drawer that explains the alert (its Herders
            # row) opens on the animals, not on the keeper band.
            "x": int(herd.get("x", -1)), "y": int(herd.get("y", -1)),
        })
    return items

## Is this managed herd short of the keepers the sim demands? `herders_needed` is OWNERSHIP-GATED —
## `0` on a wild herd — so it doubles as the "this herd is managed at all" test and a wild hunt can
## never trip it. The staffed count is folded across every player band and is pending-aware, so a
## just-issued reinforcement clears the alert on the same frame. `bands` empty = the INGESTED roster,
## which is what the JUMP routing wants (it runs on a click, long after the ingest); the producer
## passes the incoming one.
func _herd_is_under_crewed(herd_id: String, herd: Dictionary, bands: Array = []) -> bool:
    var needed := int(herd.get("herders_needed", 0))
    if needed <= 0:
        return false
    return int(_band_labor.hunt_effort_on(herd_id, bands).get("workers", 0)) < needed

## The alerting herd (if any) standing on `(x, y)`, for the orb's jump routing — the herd twin of
## `_awaiting_expedition_at`. Only herds the player's own bands work, via the same `_hunted_herds_of`
## walk the producers use, so a row can never route to a herd no producer would have alerted on.
## Both herd producers share it: a starving pen and an under-crewed herd want the same jump (open the
## animals' own drawer, where the fed fraction and the Herders row explain the alert).
func _alerted_herd_at(x: int, y: int) -> String:
    for band_variant in _band_labor.player_bands():
        if not (band_variant is Dictionary):
            continue
        for entry in _hunted_herds_of(band_variant):
            var herd_id: String = entry["herd_id"]
            var herd: Dictionary = entry["herd"]
            if int(herd.get("x", -1)) != x or int(herd.get("y", -1)) != y:
                continue
            if PenStatus.herd_is_starving(herd) or _herd_is_under_crewed(herd_id, herd):
                return herd_id
    return ""

## Is there an unworked BUILT rung on `(x, y)` — i.e. did Producer 6 put a row here? Its jump names
## the LAND rather than merely recentring, because the patch's own rung rows and its improvement
## control live on the land card and that is what the player has to reach to fix it.
func _unworked_rung_at(x: int, y: int) -> bool:
    var patch: Dictionary = _band_labor.forage_patch_lookup().get(Vector2i(x, y), {})
    if patch.is_empty() or not bool(patch.get("has_owner", false)) \
            or int(patch.get("owner", -1)) != HudConst.PLAYER_FACTION_ID:
        return false
    if not (bool(patch.get("is_cultivated", false)) or bool(patch.get("is_field", false))):
        return false
    return int(_band_labor.forage_effort_at(x, y).get("workers", 0)) <= 0

## The awaiting expedition standing on (x, y), or {} — lets the orb's Jump reuse the panel's own
## expedition-focus path instead of a second, weaker one (see `on_turn_orb_focus`).
func _awaiting_expedition_at(x: int, y: int) -> Dictionary:
    for exp_variant in _band_labor.player_expeditions():
        if not (exp_variant is Dictionary):
            continue
        var exp: Dictionary = exp_variant
        if HudFormat.expedition_phase_key(exp) != HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            continue
        if int(exp.get("current_x", -1)) == x and int(exp.get("current_y", -1)) == y:
            return exp
    return {}

## Why a band is shrinking: a food crisis (larder below critical) reads "starving" first;
## then, since morale no longer kills (discontent relocates people — see
## docs/plan_civ_wellbeing.md), a shrink with emigrants last turn reads "people leaving".
## Otherwise the dominant morale cause names it in plain language ("harsh terrain" /
## "harsh climate" / "unrest"). When no cause is attributed (morale steady/rising — e.g.
## a rehydrated save, or shrinkage from cold deaths / an aging cohort at healthy morale)
## only say "low morale" if morale is actually low, else leave it plain rather than
## asserting a false reason.
func _decline_reason(turns: float, morale: float, morale_cause: int, last_emigrated: int) -> String:
    if BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.critical_turns():
        return HudAttentionVocab.DECLINE_REASON_STARVING
    if last_emigrated > 0:
        return HudAttentionVocab.DECLINE_REASON_PEOPLE_LEAVING
    var cause_label := DetailFormat.morale_cause_label(morale_cause)
    if cause_label != "":
        return cause_label
    if morale < BandFoodStatus.warn_morale():
        return HudAttentionVocab.DECLINE_REASON_LOW_MORALE
    return ""
