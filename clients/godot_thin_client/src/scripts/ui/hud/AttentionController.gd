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

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
# The digested per-snapshot player world: `prev_band_sizes` (Producer 2), `player_bands` (the pen jump),
# `player_expeditions` (the awaiting jump), and `find_world_herd` (the pen producers).
var _band_labor: HudBandLaborState = null
# The Band/City panel — the orb's expedition/pen jumps reuse its own focus paths.
var _bandpanel: BandPanelController = null

# --- The one retained HudLayer helper, injected as a Callable (see the class header) ---
var _herd_label_for_id_fn: Callable

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
    # Producer 4 — awaiting orders: a detached party parked at its objective, burning provisions
    # until the player acts (amber/warn, same class as idle labor). Runs over the EXPEDITIONS split
    # out above, not the bands — an expedition is never "Band N", so it never enters the band loop.
    attention.append_array(_awaiting_orders_attention(player_expeditions))
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
    # A starving-pen row jumps to the HERD, not just its hex: `focus_labor_source` (the very path
    # the Band panel's Hunt row uses) recenters AND pins the herd, so the drawer that explains the
    # alert — the "⚠ Starving" Corral row + the Pen feed cost — is what actually opens.
    var pen_herd := _starving_pen_at(x, y)
    if pen_herd != "":
        _bandpanel.focus_labor_source(x, y, pen_herd)
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
## The pens are found through the band's OWN Corral labor assignments: the client has no owner field
## on a herd, so scanning `_band_labor.world_herds()` would happily alarm on a RIVAL's starving pen.
func _starving_pen_attention(band: Dictionary) -> Array:
    var items: Array = []
    for a_variant in HudBandLaborState.labor_assignments_of(band):
        if not (a_variant is Dictionary):
            continue
        var a: Dictionary = a_variant
        if String(a.get("kind", "")).to_lower() != SourceForecast.LABOR_KIND_HUNT:
            continue
        if String(a.get("policy", "")).to_lower() != SourceForecast.LABOR_POLICY_CORRAL:
            continue
        var herd_id := String(a.get("fauna_id", ""))
        var herd := _band_labor.find_world_herd(herd_id)
        if herd.is_empty() or not PenStatus.herd_is_starving(herd):
            continue
        var fed := PenStatus.fed_fraction(herd)
        items.append({
            "kind": HudAttentionVocab.ATTENTION_KIND_STARVING_PEN,
            "severity": HudAttentionVocab.ATTENTION_SEVERITY_WARN,
            "label": HudAttentionVocab.ATTENTION_PEN_LABEL_FORMAT % _herd_label_for_id(herd_id),
            "detail": HudAttentionVocab.ATTENTION_PEN_DETAIL_FORMAT % int(round(fed * HudConst.PROGRESS_PERCENT_SCALE)),
            # The HERD's live tile — a penned herd is pinned, but the jump must still land on the
            # animals (that is where the drawer with the fed fraction and the feed cost opens),
            # not on the keeper band.
            "x": int(herd.get("x", -1)), "y": int(herd.get("y", -1)),
        })
    return items

## The starving pen (if any) standing on `(x, y)`, for the orb's jump routing — the herd twin of
## `_awaiting_expedition_at`. Only pens the player's own bands keep, via the same producer path.
func _starving_pen_at(x: int, y: int) -> String:
    for band_variant in _band_labor.player_bands():
        if not (band_variant is Dictionary):
            continue
        for a_variant in HudBandLaborState.labor_assignments_of(band_variant):
            if not (a_variant is Dictionary):
                continue
            var a: Dictionary = a_variant
            if String(a.get("kind", "")).to_lower() != SourceForecast.LABOR_KIND_HUNT:
                continue
            if String(a.get("policy", "")).to_lower() != SourceForecast.LABOR_POLICY_CORRAL:
                continue
            var herd_id := String(a.get("fauna_id", ""))
            var herd := _band_labor.find_world_herd(herd_id)
            if herd.is_empty() or not PenStatus.herd_is_starving(herd):
                continue
            if int(herd.get("x", -1)) == x and int(herd.get("y", -1)) == y:
                return herd_id
    return ""

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
