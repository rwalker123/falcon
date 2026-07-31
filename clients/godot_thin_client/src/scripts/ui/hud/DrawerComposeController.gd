class_name DrawerComposeController
extends RefCounted

## The DRAWER'S COMPOSE HALF (HUD decomposition Phase 2c-2b, docs/plan_hud_decomposition.md): the
## compose-sheet lifecycle, the two drawer-action builders that stand in front of it, the two big
## compose builders behind it (`_build_forage_assign_controls` / `_build_herd_assign_controls`), and
## the compose-only forecast / gate / crop-picker layer they rest on. It is the second half of the
## selection card — `SelectionCardController` took the identity/list half; the DRAWER RENDER DISPATCH
## (`_render_land_drawer` / `_render_occupant_drawer`) lives on `SubjectDrawerController` (Phase 2c-3)
## now and calls IN here.
##
## Built on the LegendController / TopBarReadouts / TurnOrbController / SelectionCardController idiom:
## `HudLayer` holds one as `_drawercompose`, hands it the shared `RefCounted` state models BY REFERENCE
## (the SAME `ComposeState` / `HudBandLaborState` / `HudSelectionState` instances), keeps thin
## delegators for the two methods reached BY NAME (`is_compose_sheet_open` / `close_compose_sheet` —
## `Main._unhandled_input`'s Esc precedence and the preview harness probe them on the HUD node), and
## RELAYS this controller's own two signals onto the `HudLayer` signals `Main` connects to. The
## controller never emits a `HudLayer` signal directly.
##
## THE WHOLE BOUNDARY BACK TO `HudLayer` IS THREE CALLABLES, and each is retained there because it has
## callers on the other side too:
##   • `_resolve_assign_band` — the acting band, also resolved by move-band / quick-assign / targeting.
##   • `_herd_label_for_id`   — the herd vocabulary, also read by the targeting banner + command feed.
##   • `_emit_assign_labor`   — owns the `assign_labor_requested` emit, the optimistic pending write and
##     `_after_pending_change()`, all of which are HudLayer's. So `assign_labor` stays INDIRECT here,
##     while the two commands with no other emitter (`send_hunt_expedition` / `extend_pen`) are signals.
##
## Everything else arrives as a collaborator: the state models, the top bar (for `faction_knowledge`,
## which the rung gates read), the selection card (for `tile_contents_unseen`), the two drawer-action
## containers it fills, the selection card panel it anchors the sheet beside (read-only), and a HOST
## node — a `RefCounted` cannot `add_child`, so the `ComposeSheet` it creates is parented into the HUD
## CanvasLayer exactly as `TurnOrbController` parents its fork panel.
##
## The word tables, formats and thresholds live in the topic vocab modules (`HudConst` / the matching
## `Hud*Vocab`) and the shared `DetailFormat` layer, read as `Module.X` — so a phrase is still typed in
## exactly one place.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# A hunting party was dispatched — relayed to HudLayer.send_hunt_expedition_requested.
# The deal line's FIRST term is the stance's own take, undipped — the baseline the dip is measured
# against. Named rather than a bare `1.0` beside `deal["build_fraction"]`, so the two arguments to
# `_account_products` read as what they are: no dip, and this rung's dip.
const DEAL_STANCE_UNDIPPED := 1.0

signal send_hunt_expedition_requested(payload: Dictionary)
# Another ring was fenced around a pen — relayed to HudLayer.extend_pen_requested.
signal extend_pen_requested(payload: Dictionary)

## The SECOND AXIS's command (issue #442) — `cultivate` / `sow` / `tame` / `corral`. A signal like
## `extend_pen_requested` rather than a HudLayer callable, for the same reason: this controller is its
## only emitter. `HudLayer` relays it to `Main`, which formats the verb.
signal improvement_requested(payload: Dictionary)

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _compose: ComposeState = null
var _band_labor: HudBandLaborState = null
var _selection: HudSelectionState = null
# Read for `faction_knowledge` ONLY — the knowledge half of the investment-rung gates.
var _topbar: TopBarReadouts = null
# Read for `tile_contents_unseen` ONLY — a redacted hex offers no forage compose.
var _selectioncard: SelectionCardController = null
# The HUD CanvasLayer, so the RefCounted controller has a node to parent the compose sheet into.
var _host: Node = null

# --- Scene nodes (handed in by HudLayer) ---
# The two drawer-action containers this controller FILLS. They keep their names and their place in
# the drawer — the compose block moved out of them, the nodes did not move.
var _herd_assign_controls: VBoxContainer = null
var _forage_assign_controls: VBoxContainer = null
# The selection card, READ-ONLY: the rect the sheet floats beside (`_compose_anchor_rect`).
var _tile_panel: PanelCard = null

# --- The three retained HudLayer helpers, injected as Callables (see the class header) ---
# Each is reached through a typed adapter below rather than called raw: `Callable.call` returns
# `Variant`, which would push an untyped value into every consumer here.
var _resolve_assign_band_fn: Callable
var _herd_label_for_id_fn: Callable
var _emit_assign_labor_fn: Callable

# --- Owned state (moved off HudLayer) ---
# The floating compose sheet NODE. Which source it is composing is pure data and lives on `_compose`
# (`kind()` / `subject()`); this handle is a scene node, so it is owned here beside its lifecycle.
var _compose_sheet: ComposeSheet = null
# The drawer-actions diff caches: the shape signature last rendered for each drawer, so an unchanged
# per-snapshot restate PATCHES the existing nodes instead of freeing + rebuilding them (the reflow
# that flashes). Zero readers outside this controller, so they travelled with the builders.
var _forage_drawer_shape: Array = []
var _herd_drawer_shape: Array = []

func _init(compose: ComposeState, band_labor: HudBandLaborState, selection: HudSelectionState,
        topbar: TopBarReadouts, selectioncard: SelectionCardController, host: Node,
        herd_assign_controls: VBoxContainer, forage_assign_controls: VBoxContainer,
        tile_panel: PanelCard,
        resolve_assign_band: Callable, herd_label_for_id: Callable, emit_assign_labor: Callable) -> void:
    _compose = compose
    _band_labor = band_labor
    _selection = selection
    _topbar = topbar
    _selectioncard = selectioncard
    _host = host
    _herd_assign_controls = herd_assign_controls
    _forage_assign_controls = forage_assign_controls
    _tile_panel = tile_panel
    _resolve_assign_band_fn = resolve_assign_band
    _herd_label_for_id_fn = herd_label_for_id
    _emit_assign_labor_fn = emit_assign_labor

# ---- Typed adapters over the three injected HudLayer helpers -----------------------------------

## The band an assignment targets — the selected player band, else the faction's single band. Retained
## on HudLayer because move-band, quick-assign and the targeting flows resolve the same band.
## The player faction's `{track: progress}` knowledge row, threaded into every `RungGates` call.
## `_topbar` is the only place the HUD holds it, and `RungGates` is stateless by design — so the
## sheet reads the row here and passes it as a value rather than the gate layer reaching back.
func _player_knowledge() -> Dictionary:
    return _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID)

func _resolve_assign_band() -> Dictionary:
    return _resolve_assign_band_fn.call()

## A friendlier label for a herd id. Retained on HudLayer, which also feeds the targeting banner and
## the command feed from it.
func _herd_label_for_id(herd_id: String) -> String:
    return _herd_label_for_id_fn.call(herd_id)

## Issue a labor assignment. Retained on HudLayer because it owns the `assign_labor_requested` emit,
## the optimistic pending-labor write and `_after_pending_change()` — so this stays INDIRECT rather
## than becoming a third signal on this controller.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String,
        policy: String, species: String = "",
        improvement: String = SourceForecast.IMPROVEMENT_NONE) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, policy, species, improvement)

## Send the improvement command when the composed second axis differs from what the source is already
## building — the SET verb (`cultivate` / `sow` / `tame` / `corral`) when one is composed, and
## `abandon_improvement` when the player has unchecked a running build. Either way it is its OWN
## command, never a token on `assign_labor` — that is what lets a crew-size edit stop re-asserting the
## improvement, and with it the re-staffing gap where changing the crew of a PAUSED build re-ran the
## build's own gates and was refused (issue #442 §6).
##
## **The two commands name their target DIFFERENTLY, and the payload carries both spellings for that
## reason.** A set verb is targeted by the VERB (`tame` names a herd; `cultivate`/`sow`/`corral` name a
## tile — `corral` is the case that proves it, a herd's rung addressed by the pen's place), while
## `abandon_improvement` is targeted by the WEB (`forage` → tile, `hunt` → herd), because it names a
## SOURCE rather than a verb. `Main` keeps the two grammars in separate builders; nothing here
## flattens them into one.
##
## **The abandon is UNGATED and is never suppressed.** It is legal on a stalled build, on unhealthy
## ground and at any knowledge level — that is the case it exists for. The only thing that stops it is
## the source not building anything, which `standing` already answers.
func _emit_improvement(band: Dictionary, kind: String, composed: String, standing: String,
        x: int, y: int, herd_id: String) -> void:
    if composed == standing:
        return
    emit_signal("improvement_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        # "" here IS the abandon — the wire's own spelling of "building nothing", so the formatter
        # branches on the same value the compose state holds rather than on a second flag.
        "improvement": composed,
        "kind": kind,
        "x": x,
        "y": y,
        "herd_id": herd_id,
    })

## The per-turn take `workers` from `band` get off `herd` under `policy` — the sim's LOCAL/band hunt
## take before the output multiplier, `min(workers × per-worker, band_ceiling)`, ON THE COMPONENT THE
## SPECIES PAYS. Returns `{available, rate, axis}` (`available` false when the levers/ceiling are absent).
##
## **The per-worker rate is the HERD's `per_worker_yield` / `per_worker_trade`, never the cohort's
## `hunt_per_worker_provisions`** (issue #337). That cohort field is a species-BLIND echo of the global
## `hunt.provisions_per_biomass` — it has no herd in scope, so it cannot know an inedible quarry pays no
## meat, and clamping a per-herd preview with it quotes a positive food rate against a wolf's all-zero
## food ceilings. The sim's own doc comments now say exactly this.
## Resident-band only: an EXPEDITION's trip is never a rate division (see `SourceForecast.hunt_trip_forecast`).
func _hunt_take_rate(herd: Dictionary, policy: String, workers: int) -> Dictionary:
    var rates := SourceForecast.herd_axis_rates(herd, policy)
    var per_worker_rate := float(rates["per_worker"])
    var ceiling := float(rates["ceiling"])
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return {"available": false}
    return {
        "available": true,
        "rate": maxf(minf(float(workers) * per_worker_rate, ceiling), 0.0),
        "axis": String(rates["axis"]),
    }


## The averaging WINDOW (turns) for the whole-animal disclaimer — a STABLE, worker-independent property
## derived from the SELECTED policy's raw flow ceiling (NOT the crew's current delivered rate, which
## moves as workers change and made the old line blink out). Keyed on `policy` because a faster policy
## (Surplus/Deplete) delivers lumpy whole animals over a different span. `g` = animals/turn the policy's
## flow buys: slow/big game (`g < 1`) lands one animal every ~`1/g` turns; fast game (`g >= 1`) delivers
## the "extra" fractional animal every ~`1/frac` turns. Returns 0 when `food_per_animal` / the ceiling is
## unknown (caller then skips the line). NEVER scaled by `output_multiplier` — it's a pure herd property.
func _hunt_avg_window_turns(herd: Dictionary, policy: String) -> int:
    # On the component the species pays: an inedible quarry's `food_per_animal` is honestly 0, so a
    # food-only derivation returns 0 and the disclaimer silently disappears from a wolf's picker even
    # though its delivery is every bit as lumpy. The animal COUNT is identical on either component.
    var rates := SourceForecast.herd_axis_rates(herd, policy)
    var fpa := float(rates["per_animal"])
    var ceiling := float(rates["ceiling"])
    if fpa <= 0.0 or ceiling <= 0.0:
        return 0
    var g: float = ceiling / fpa
    var x: int
    if g < 1.0:
        x = int(ceil(1.0 / g))
    else:
        var frac: float = g - floor(g)
        x = 1 if frac < 0.01 else int(ceil(1.0 / frac))
    return clampi(x, 1, HudComposeVocab.HUNT_WINDOW_MAX_TURNS)

## The HONEST carry-aware delivery model for a local hunt: what a crew of `workers` from `band` actually
## lands off `herd` under `policy` per turn, and how much of the kill they can't carry (which rots). A
## hunt takes WHOLE animals via a kill-credit bank, so the crew's raw food throughput is quantized to the
## whole bodies it can haul — fractional carry capacity is idle (NOT waste), but a crew too small to carry
## even one whole animal loses the surplus meat. Returns `{available, delivered, waste, waste_pct}` (all
## food/turn; `waste_pct` 0..1) or `{available=false}` when a lever/ceiling is absent (caller degrades to
## the old food/turn line). NEVER re-derives the ecology model — `food_per_animal` and the flow ceiling
## are sim exports.
func _hunt_delivered_and_waste(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> Dictionary:
    # PER COMPONENT, on the one this species pays (issue #337). The three terms must come from the SAME
    # axis or the arithmetic is nonsense: a wolf's per-animal FOOD quantum is 0 (divide by zero) while
    # its per-animal TRADE quantum is real. `herd_axis_rates` is the single place that choice is made,
    # and it reads the HERD's species-aware per-worker rates — never the cohort's species-blind
    # `hunt_per_worker_provisions`, which is what would re-introduce phantom food here.
    var rates := SourceForecast.herd_axis_rates(herd, policy)
    var fpa := float(rates["per_animal"])
    var per_worker := float(rates["per_worker"])
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var ceiling := float(rates["ceiling"])
    if fpa <= 0.0 or per_worker <= 0.0 or ceiling < 0.0 or workers <= 0:
        return {"available": false}
    ceiling *= output
    var collection := float(workers) * per_worker * output   # crew's raw food throughput /turn
    var carryable := floorf(collection / fpa)                # whole animals /turn the crew can carry
    var delivered := 0.0
    var waste := 0.0
    if carryable >= 1.0:
        # Carry quantized to whole bodies; the flow ceiling still caps it. Leftover carry capacity is
        # idle, NOT waste (no animal was killed and dropped).
        delivered = minf(ceiling, carryable * fpa)
        waste = 0.0
    else:
        # Can't carry even one whole animal → the meat that can't be hauled rots.
        var kills_per_turn := minf(1.0, ceiling / fpa)
        delivered = collection * kills_per_turn
        waste = (fpa - collection) * kills_per_turn
    var killed_food := delivered + waste
    var waste_pct := (waste / killed_food) if killed_food > 0.0 else 0.0
    return {"available": true, "delivered": delivered, "waste": waste, "waste_pct": waste_pct,
        "axis": String(rates["axis"]), "per_animal": fpa}

## An animals-per-turn rate string: up to 2 decimals with trailing zeros AND a trailing dot stripped
## (1.90→"1.9", 1.00→"1", 0.65→"0.65", 0.15→"0.15"). `String.num` keeps a lone ".0", so format fixed and
## strip the tail ourselves (rstrip stops at the first non-matching char, so integer zeros survive).
func _format_animal_rate(value: float) -> String:
    var text := ("%." + str(HudComposeVocab.HUNT_ANIMAL_RATE_DECIMALS) + "f") % value
    if "." in text:
        text = text.rstrip("0")
        if text.ends_with("."):
            text = text.rstrip(".")
    return text


## Each hunt policy's button metric, keyed policy → a `{compact, full}` pair (compact for the one-line
## button face, full for the tooltip). The plant twin of this is `_forage_policy_takes`; both wear the
## same shape, only the metric differs:
##   the herd's worker-independent CAP for the stance (`hunt_policy_ceilings`): a bare signed rate on
##   the face, framed "up to X/turn" in the tooltip — the ceiling it is, distinct from the crew's
##   carry-aware delivered line below the picker. Read straight off the sim; never re-derived.
##
## **THE HUNT SIDE ALSO FILLS THE PAIR'S OPTIONAL `note`** — the averaging-window disclaimer
## (`HudComposeVocab.HUNT_AVG_WINDOW_FORMAT`), which the picker appends under the rung's tooltip metric
## line. It is a caveat on THIS rung's rate (a hunt lands whole animals, so a per-turn figure is a
## long-run average), so it rides the rung's own take pair rather than a body line: keyed per rung,
## since `_hunt_avg_window_turns` spans differ by stance, and omitted when the window is unknown. The
## forage twin fills no note — a patch's take is smooth, and there is nothing to average.
##
## **The build verbs' PAYOFF faces left this function with them** (issue #442). Tame and Corral were a
## second loop here, wearing `→ 1.48 food · 0.37 trade` because a build verb was a rung of this picker;
## the improvement control states the same payoff as its own terms now, and the list this reads is
## exactly the four stances.
## Empty when the herd carries no ceilings (older snapshot / non-huntable).
func _hunt_policy_takes(herd: Dictionary) -> Dictionary:
    var takes := {}
    var ceilings_variant: Variant = herd.get(SourceForecast.HERD_BAND_CEILINGS_KEY, {})
    if not (ceilings_variant is Dictionary):
        return takes
    for policy in (ceilings_variant as Dictionary):
        var rate := float((ceilings_variant as Dictionary)[policy])
        if rate < 0.0:
            continue
        # BOTH products (issue #337): each rung's cap is a pair, and each half is rendered only when
        # non-zero. A wolf's four rungs therefore read as four ascending TRADE caps rather than four
        # `+0.00`s — the false reading that said an inedible species was worth nothing on every rung.
        var trade_rate := SourceForecast.hunt_policy_trade_ceiling(herd, String(policy))
        var pair := SourceForecast.extractive_take_pair(rate, maxf(trade_rate, 0.0))
        # The averaging window this rung's rate is an average OVER, as the pair's tooltip `note`. Only
        # for a STANCE the picker actually offers — the sim exports a ceiling row per `HUNT_POLICIES`,
        # which still includes the two build verbs, and those are the improvement control's now.
        var window_turns := _hunt_avg_window_turns(herd, String(policy)) \
            if String(policy) in SourceForecast.LABOR_HUNT_POLICIES else 0
        if window_turns > 0:
            pair["note"] = HudComposeVocab.HUNT_AVG_WINDOW_FORMAT % window_turns
        takes[String(policy)] = pair
    return takes


## The LOCAL hunt's live per-turn yield preview, or "" when the snapshot lacks the levers/ceilings
## (graceful degrade — no line, panel otherwise unchanged). A resident band applies its
## `output_multiplier` (morale/discontent productivity) at payout, so the preview is the take rate
## scaled by it. Reads income-green when the take is within the herd's sustainable yield (the Sustain
## ceiling), WARN-amber with the shared ⚠ when it overdraws — the same flag the allocation rows carry.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    # The Sustain ceiling on the SAME axis the take is measured on — comparing a trade take against a
    # food ceiling would flag every wolf hunt as an overdraw (or none of them).
    var sustain_rates := SourceForecast.herd_axis_rates(herd, SourceForecast.DEFAULT_HUNT_POLICY)
    var sustain_ceiling := float(sustain_rates["ceiling"])
    if sustain_ceiling < 0.0:
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var sustainable := sustain_ceiling * output
    var dw := _hunt_delivered_and_waste(band, herd, policy, workers)
    if not bool(dw.get("available", false)):
        # Graceful degrade — the per-animal quantum (or a lever) is unknown on BOTH components, so fall
        # back to the smoothed per-turn line rather than regress the readout. It is stated in whichever
        # currency the take is actually in: a trade take never reads as food.
        var take := _hunt_take_rate(herd, policy, workers)
        if not bool(take.get("available", false)):
            return ""
        var actual := float(take["rate"]) * output
        var trade_axis: bool = String(take["axis"]) == SourceForecast.YIELD_AXIS_TRADE
        var text: String = HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % (
            SourceForecast.format_trade(actual) if trade_axis else SourceForecast.format_yield(actual))
        if _is_overdraw(actual, sustainable):
            return "[color=#%s]%s %s%s[/color]" % [
                HudStyle.WARN_HEX, HudComposeVocab.OVERHUNT_FLAG, text, HudComposeVocab.LOCAL_HUNT_OVERDRAW_SUFFIX]
        return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, SourceForecast.YIELD_TOOLTIP_RENEWABLE]
    # ANIMALS-FIRST: the crew's honest carry-aware delivered take, as a per-turn animal rate (one
    # consistent format — no fast/slow flip). `delivered` is already carry-quantized, so this credits no
    # throughput the crew can't haul home. The animal rate is UNIT-FREE — a ratio of a take to one
    # animal's worth of the same product — so this line reads identically for a deer and a wolf without
    # naming either currency.
    var fpa := float(dw["per_animal"])
    var delivered := float(dw["delivered"])
    var animal_rate := delivered / fpa if fpa > 0.0 else 0.0
    var primary := HudComposeVocab.HUNT_DELIVERED_FORMAT % [_format_animal_rate(animal_rate), SourceForecast.herd_display_name(herd)]
    # Overdraw and waste are DIFFERENT flags and may co-occur — render both. Overdraw = the delivered take
    # exceeds the herd's Sustain ceiling (Surplus/Deplete draw it down); waste = a kill the crew couldn't
    # carry. The Sustain reading stays green + "· renewable".
    var body := ""
    if _is_overdraw(delivered, sustainable):
        body = "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, HudComposeVocab.OVERHUNT_FLAG, primary, HudComposeVocab.LOCAL_HUNT_OVERDRAW_SUFFIX]
    else:
        body = "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, primary, SourceForecast.YIELD_TOOLTIP_RENEWABLE]
    var waste_pct := float(dw["waste_pct"])
    if waste_pct > 0.0:
        # Waste is its OWN concern — always WARN-tinted, even when the main line is green.
        body += "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, SourceForecast.HUNT_WASTE_SUFFIX_FORMAT % int(round(waste_pct * 100.0))]
    return body

## The LOCAL forage patch's live per-turn yield preview — the plant twin of `_local_hunt_preview_bbcode`.
## Forage is SMOOTH (no whole-animal rhythm — no lumpy carry, no waste), so the line is just the
## per-turn take + a sustainability verdict: income-green `+2.74 /turn · renewable` when the take is
## within the patch's Sustain ceiling, WARN-amber `⚠ … — overdraws the patch` when a Surplus/Deplete/
## Eradicate policy draws it down. Both scaled by the acting band's output multiplier, like the hunt
## line. "" (no line) when the forecast levers are unknown, so the panel degrades gracefully.
##
## **THE WHOLE VECTOR, EACH ACCOUNT ONLY WHEN NON-ZERO (#426).** This read the food account alone,
## which is the same lie the picker face above it told: a flax patch previewed `+0.00 /turn ·
## renewable` — "staff this and get nothing, sustainably" — for a rung that pays real trade goods, and
## a hay meadow said the same of its fodder. `SourceForecast.yield_components` is the joiner the worked
## rows already use, so the composed preview and the row it becomes next turn word the vector alike.
##
## **The overdraw verdict is likewise PER ACCOUNT, and ANY account overdrawing carries the line.** The
## comparison used to be food-against-food, so a fodder crop's Eradicate rung — which strips the
## meadow bare — read green: both sides of the test were 0. The accounts have independent ceilings and
## a take can sit inside one while blowing through another, so a single scalar cannot answer this; ANY
## rather than ALL, because the warning is about the patch, and one account drawn past its regrowth
## draws down the same patch.
func _local_forage_preview_bbcode(band: Dictionary, tile_info: Dictionary, policy: String, workers: int) -> String:
    # The Sustain ceiling IS the patch's sustainable yield (its regrowth take), so a take above it draws
    # the patch down — mirrors how the hunt version derives `sustainable` from the Sustain ceiling.
    var sustain := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.DEFAULT_HUNT_POLICY)
    if not bool(sustain["known"]):
        return ""
    var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, policy)
    if not bool(forecast["known"]):
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var actual := SourceForecast.expected_yield(forecast, workers, band)
    var actual_trade := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_trade", "ceiling_trade")
    var actual_fodder := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_fodder", "ceiling_fodder")
    var text := SourceForecast.yield_components(actual, actual_trade, actual_fodder)
    var overdraws := _is_overdraw(actual, float(sustain["ceiling"]) * output) \
        or _is_overdraw(actual_trade, float(sustain["ceiling_trade"]) * output) \
        or _is_overdraw(actual_fodder, float(sustain["ceiling_fodder"]) * output)
    if overdraws:
        return "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, HudComposeVocab.OVERHUNT_FLAG, text, HudComposeVocab.LOCAL_FORAGE_OVERDRAW_SUFFIX]
    return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, SourceForecast.YIELD_TOOLTIP_RENEWABLE]

## A "Band: [▼]" dropdown row for the assign controls: lists every player band (positional
## "Band N" names, matching the roster) and selects `selected_band`; `on_pick` fires with the
## chosen band dict. The actor band is always explicit — shown even with one band (single-item
## dropdown). NOTE: lists ALL player bands; in-range filtering (Forage within work_range / Hunt
## within work_range + leash) is deferred to the multi-band slice (needs the hunt-leash reach in
## the snapshot, and can't be exercised until a 2nd band can exist).
func _build_band_picker(selected_band: Dictionary, on_pick: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var name_label := Label.new()
    name_label.text = HudWorkVocab.BAND_PICKER_LABEL
    name_label.add_theme_color_override("font_color", HudStyle.INK)
    row.add_child(name_label)
    var picker := OptionButton.new()
    picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var bands := _band_labor.current_player_bands()
    var selected_entity := int(selected_band.get("entity", -1))
    var selected_idx := 0
    for i in bands.size():
        var b: Dictionary = bands[i]
        picker.add_item(HudFormat.band_display_name(b, i + 1))
        picker.set_item_metadata(i, int(b.get("entity", -1)))
        if int(b.get("entity", -1)) == selected_entity:
            selected_idx = i
    picker.select(selected_idx)
    picker.item_selected.connect(func(idx: int) -> void:
        on_pick.call(_band_labor.player_band_by_entity(int(picker.get_item_metadata(idx)))))
    row.add_child(picker)
    return row

## Cap the worker stepper at what the source can absorb: min(the band's assignable workers,
## max-useful). Returns `{cap, note}` — `note` is set ONLY when max-useful is the binding cap, so a
## dead `+` button is always explained rather than mysterious (the idle-worker cap explains itself).
##
## THE WORKED-ROW TWIN IS `SourceForecast.source_worker_cap_state`, which takes the SAME `useful_floor`
## and applies it the same way — the compose sheet and the Band panel's work board must gate at one
## ceiling, and a floor that reached only one of them let the board flag a herd under-herded and then
## disable the `+` that fixes it.
func _forecast_worker_cap(forecast: Dictionary, assignable: int, useful_floor: int = 0) -> Dictionary:
    var useful := SourceForecast.max_useful_workers(forecast)
    # A managed herd's maintenance crew raises the usefulness ceiling above what the take/prepare side
    # reports: a Corral rung's prep forecast says "1 worker suffices to prepare", but a growing pen needs
    # its herding crew EVERY turn to hold its tameness. Callers pass that crew as `useful_floor` —
    # `SourceForecast.herd_crew_floor` is its one definition, and it is where the ownership-gated vs
    # would-be field split is explained. An UNBOUNDED forecast stays unbounded — the floor is a RAISE,
    # never a new cap — and a wild herd passes 0, so it's a no-op there.
    if useful != SourceForecast.MAX_USEFUL_UNBOUNDED:
        useful = maxi(useful, useful_floor)
    if useful == SourceForecast.MAX_USEFUL_UNBOUNDED or useful >= assignable:
        # Labor-bound below the usefulness ceiling: the `+` capped at idle workers, not at
        # usefulness — name the reason so the cap doesn't read as a silent bug. Exactly staffed
        # (useful == assignable) and no-forecast (UNBOUNDED) stay noteless.
        var labor_note := ""
        if useful != SourceForecast.MAX_USEFUL_UNBOUNDED and useful > assignable:
            labor_note = SourceForecast.LABOR_BOUND_NOTE_FORMAT % [assignable, useful]
        return {"cap": assignable, "note": labor_note}
    var noun := SourceForecast.MAX_USEFUL_NOUN_ONE if useful == 1 else SourceForecast.MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": SourceForecast.MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## **THE IMPROVEMENT CONTROL** (issue #442 §3) — the second axis, in whichever ONE of its three states
## this source is in, plus the deal it offers. Shared verbatim by both webs: the plant ladder
## (Cultivate → Sow) and the animal one (Tame → Corral) get the same control, the same three states
## and the same forecast, because they are the same decision about different stock.
##
## The three states and their precedence (see `HudWidgets.build_improvement_control` for the shape):
##   RUNNING first — something is being built here, so nothing else is on offer. Its face carries the
##       meter, and a WARN pause line appears when the source has left Thriving, which is the ONE
##       silent rule on this axis: the meter accrues only while the source is Thriving, and that is
##       deliberately NOT a gate (a source's phase swings as it is worked, so refusing the verb would
##       be un-actionable churn). The sim just PAUSES, losing nothing — and saying nothing here would
##       recreate exactly the hidden rule this whole arc exists to kill. It was animal-only
##       (`_tame_stalled_hint`) because the plant web had no control to hang it on.
##   DONE next — the source stands on a built rung, so the state gets a static label, and the NEXT
##       rung's checkbox renders beneath it if there is one.
##   OFFERED last — an unchecked box naming the next rung and its terms, its gate reasons beneath it
##       when it has any (shown, unchecked, explained — a greyed control alone does not teach).
##
## `payoff_face` is the caller's per-rung terms Callable (`improvement -> String`), because the plant
## web substitutes the CROP the rung would commit to and the animal web quotes the herd. `extra_rows`
## is the same idea for whole controls: the plant web drops its CROP PICKER between the box and the
## deal, since which crop this rung commits to is part of the same decision. Passing both in rather
## than branching keeps this function free of flora knowledge.
func _build_improvement_control(kind: String, source: Dictionary, prefix: String, stance: String,
        composed: String, band: Dictionary, workers: int, crew_label: String,
        on_toggle: Callable, target: VBoxContainer,
        payoff_face: Callable = Callable(), extra_rows: Callable = Callable()) -> void:
    # RUNNING — a composed improvement that is not yet built. `composed` covers both the wire's
    # standing value and a box the player just ticked, so a fresh commitment reads as running
    # immediately rather than waiting a turn to stop looking like an offer.
    if composed != SourceForecast.IMPROVEMENT_NONE \
            and not SourceForecast.improvement_is_done(source, prefix, composed):
        var percent := HudFormat.progress_percent(
            SourceForecast.improvement_progress(source, prefix, composed))
        var running_face := HudComposeVocab.IMPROVEMENT_RUNNING_FORMAT % [
            FoodIcons.for_policy(composed),
            String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS.get(composed, composed.capitalize())),
            percent]
        var paused := _improvement_paused_note(source, prefix)
        target.add_child(HudWidgets.build_improvement_control(composed,
            HudWidgets.IMPROVEMENT_STATE_RUNNING, running_face,
            _improvement_running_tooltip(kind, composed), on_toggle, paused, true))
        if extra_rows.is_valid():
            extra_rows.call(composed, target)
        _build_improvement_deal(source, kind, prefix, stance, composed, band, workers, crew_label,
            payoff_face, target)
        return
    # DONE — the highest rung this source has actually built, as a state label. Highest first, for the
    # reason the work board's rung mark tests highest first: a Field is ALSO cultivated and a penned
    # herd is ALSO fully tamed, so testing the lower rung first would collapse the distinction.
    var ladder: Array = SourceForecast.FORAGE_IMPROVEMENTS if kind == SourceForecast.LABOR_KIND_FORAGE \
        else SourceForecast.HUNT_IMPROVEMENTS
    for i in range(ladder.size() - 1, -1, -1):
        var rung := String(ladder[i])
        if not SourceForecast.improvement_is_done(source, prefix, rung):
            continue
        target.add_child(HudWidgets.build_improvement_control(rung,
            HudWidgets.IMPROVEMENT_STATE_DONE, _improvement_done_face(source, prefix, rung, band),
            String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")), on_toggle))
        break
    # OFFERED — the ONE rung on offer, ordered by `RungGates` so the sheet, the work board and the map
    # can never disagree about which rung is next. Renders BENEATH a done label when there is one.
    var offer := RungGates.next_rung_offered(kind, source, composed, _player_knowledge(), prefix)
    if offer.is_empty():
        return
    var rung := String(offer["policy"])
    var terms := String(payoff_face.call(rung)) if payoff_face.is_valid() \
        else _improvement_payoff_terms(source, kind, prefix, rung, band)
    var offer_face := HudComposeVocab.IMPROVEMENT_OFFER_FORMAT % [
        FoodIcons.for_policy(rung),
        String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS.get(rung, rung.capitalize())), terms] \
        if terms != "" else HudComposeVocab.IMPROVEMENT_OFFER_BARE_FORMAT % [
            FoodIcons.for_policy(rung),
            String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS.get(rung, rung.capitalize()))]
    var reasons := RungGates.gate_reasons_for({rung: offer.get("reasons", [])}, rung)
    # **GATED — THE REASON IS THE CONTROL, and the offer text is not shown at all.** This used to
    # render the full offer ("🌱 Cultivate this patch · then 0.04 food · 2.74 trade · 0.81 fodder") as
    # a greyed checkbox with "Your people know Cultivation 0% — ♻ Sustain-forage a wild patch to learn
    # it" on a line beneath. That is an OFFER the player cannot accept sitting directly above the
    # sentence explaining that they cannot accept it — the card arguing with itself, and an imperative
    # ("Cultivate this patch") aimed at someone who has no way to obey it.
    #
    # So the reason moves UP into the control's own slot, keeping the rung's glyph so the improvement
    # axis is still visibly present and still identifiable. What is lost is the payoff terms as
    # motivation ("here is what it would pay"); that is deliberate — a number you cannot act on is
    # noise at the moment you are told you cannot act, and the rung's tooltip still carries its hint.
    if not reasons.is_empty():
        target.add_child(HudWidgets.build_improvement_control(rung,
            HudWidgets.IMPROVEMENT_STATE_GATED,
            HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
                FoodIcons.for_policy(rung), String(reasons[0])],
            String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")), on_toggle,
            # A second and later reason keeps the note treatment beneath — the lead line can only
            # carry one, and dropping the rest would hide half of what the rung costs to unlock.
            reasons.slice(1)))
        return
    target.add_child(HudWidgets.build_improvement_control(rung,
        HudWidgets.IMPROVEMENT_STATE_OFFERED, offer_face,
        String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")), on_toggle))
    # The crop picker rides an OFFER only: "which crop do I commit this patch to?" is part of
    # committing, so it has no meaning where committing is refused — the gated branch above returns
    # before reaching it.
    if extra_rows.is_valid():
        extra_rows.call(rung, target)

## The RUNNING control's tooltip: the rung's own hint ("what does this buy?") plus what UNCHECKING
## does ("what happens if I stop?"). Two different questions, so two lines rather than one run-on.
##
## The abandon clause is PER WEB because the sim's answer is: a plant meter bleeds away once nobody is
## improving the patch, an animal meter is kept. Neither promises progress BACK — the command does not
## touch the meter at all, it hands the source to the rule that already governs an unimproved one.
## Putting it in the tooltip rather than in a note or a confirm is the judgement call: unchecking is
## always legal and, on the animal web, fully reversible, so a modal would be ceremony over a
## decision the player can simply re-make.
func _improvement_running_tooltip(kind: String, improvement: String) -> String:
    return HudComposeVocab.IMPROVEMENT_TOOLTIP_SEPARATOR.join([
        String(HudComposeVocab.IMPROVEMENT_HINTS.get(improvement, "")),
        String(HudComposeVocab.IMPROVEMENT_ABANDON_HINTS.get(kind, "")),
    ])

## The WARN pause line for a running build, as the note array the control renders beneath its box —
## empty when the source is Thriving (nothing to explain). Both webs read the same `ecology_phase`.
func _improvement_paused_note(source: Dictionary, prefix: String) -> Array:
    var phase := String(source.get(prefix + "ecology_phase", "")).strip_edges().to_lower()
    if phase == "" or phase == HudFloraVocab.ECOLOGY_PHASE_THRIVING:
        return []
    return [HudComposeVocab.IMPROVEMENT_PAUSED_FORMAT % phase.capitalize()]

## The done-state label's face. **The Corral rung carries the pen's per-turn upkeep and the Tame rung
## does not, and that asymmetry is deliberate and permanent** (spec §4): a penned herd cannot graze,
## so someone feeds it every turn, and a standing obligation belongs with the standing state. Do not
## make the two webs match here.
func _improvement_done_face(source: Dictionary, prefix: String, rung: String,
        band: Dictionary) -> String:
    var glyph := FoodIcons.for_policy(rung)
    var noun := String(HudComposeVocab.IMPROVEMENT_DONE_LABELS.get(rung, rung.capitalize()))
    if SourceForecast.FORECAST_FEED_KEYS.has(rung):
        var feed := float(source.get(prefix + String(SourceForecast.FORECAST_FEED_KEYS[rung]), 0.0)) \
            * float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
        if feed >= SourceForecast.FOOD_FLOW_MIN:
            return HudComposeVocab.IMPROVEMENT_DONE_UPKEEP_FORMAT % [
                glyph, noun, SourceForecast.format_magnitude(feed)]
    return HudComposeVocab.IMPROVEMENT_DONE_FORMAT % [glyph, noun]

## The "then <X>" terms on an offered box — the payoff VECTOR the built rung pays, each account only
## when non-zero, so a hay meadow reads `1.80 fodder` and a pelt species `0.37 trade`. "" when the
## wire quotes no payoff at all, which the caller renders as the bare verb rather than "· then +0.00".
##
## Quoted at SUSTAIN, because a payoff is a property of the finished rung and not of the stance the
## crew happens to hold while building it — only the DIP rides the stance.
func _improvement_payoff_terms(source: Dictionary, kind: String, prefix: String, rung: String,
        band: Dictionary) -> String:
    var deal := SourceForecast.improvement_forecast(source,
        SourceForecast.source_kind_for_labor(kind), prefix, SourceForecast.DEFAULT_HUNT_POLICY, rung)
    if deal.is_empty():
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    return SourceForecast.picker_products(float(deal["payoff"]) * output,
        float(deal["payoff_trade"]) * output, float(deal["payoff_fodder"]) * output)

## **THE WHOLE DEAL, in three terms** — `+0.96 → +0.24 while building → +1.20 /turn`, the middle term
## WARN-amber because it is the dip. Every term is scaled by the acting band's output multiplier.
##
## The FIRST term is what the two-term "Preparing: +X → then +Y" line structurally could not show: a
## build verb WAS the policy, so committing vacated the stance and there was no baseline left to
## quote. It is also the number that makes the trade legible — the dip only means something against
## what you are giving up.
##
## The Corral payoff is GROSS (the pen's feed is a separate debit on the keeper's larder), so its line
## subtracts the herd's own exported `pen_upkeep` — which the sim projects for an un-penned herd too,
## on the same biomass basis, so the row quotes the real running cost at the moment the player
## decides. The feed is NEVER folded away, and a **zero payoff is rendered, loudly** (see
## `IMPROVEMENT_DEAL_DEPLETED_NOTE`): a depleted herd below the escapement point pays nothing, and
## that is the line's most important reading.
func _build_improvement_deal(source: Dictionary, kind: String, prefix: String, stance: String,
        improvement: String, band: Dictionary, workers: int, crew_label: String,
        payoff_face: Callable, target: VBoxContainer) -> void:
    # `kind` here is the LABOR kind (`hunt`/`forage`); the forecast layer speaks SOURCE kinds
    # (`herd`/`forage`). They differ on the animal web, so this conversion is not optional.
    var deal := SourceForecast.improvement_forecast(
        source, SourceForecast.source_kind_for_labor(kind), prefix, stance, improvement)
    if deal.is_empty():
        return
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    # The payoff terms follow the CROP on the plant web, exactly as the offered box's do — one
    # Callable, both places, so the box and the line beneath it can never quote different crops.
    var payoff_terms := String(payoff_face.call(improvement)) if payoff_face.is_valid() \
        else SourceForecast.picker_products(float(deal["payoff"]) * output,
            float(deal["payoff_trade"]) * output, float(deal["payoff_fodder"]) * output)
    var feed := float(deal["feed"]) * output
    var has_feed := bool(deal["feed_rung"]) and feed >= SourceForecast.FOOD_FLOW_MIN
    var row := HudWidgets.forecast_label("")
    var hex := HudStyle.HEALTHY
    if workers <= 0:
        # UNSTAFFED: state the payoff as a CONDITION, never as a sequence already under way. Both the
        # stance term and the dip are staffing-scaled while the payoff is not, so an unstaffed deal
        # would otherwise read as a plan the player is emphatically not on track for.
        var crew := crew_label.to_lower()
        row.text = HudComposeVocab.IMPROVEMENT_DEAL_UNSTAFFED_FEED_FORMAT % [
            crew, payoff_terms, SourceForecast.format_magnitude(feed)] if has_feed \
            else HudComposeVocab.IMPROVEMENT_DEAL_UNSTAFFED_FORMAT % [crew, payoff_terms]
    else:
        # Each of the three terms is a full account VECTOR capped per account against its own ceiling,
        # never a scalar — the render-only-when-non-zero rule reaching the deal line.
        var stance_terms := _account_products(deal, workers, band, DEAL_STANCE_UNDIPPED)
        var preparing_terms := _account_products(deal, workers, band, float(deal["build_fraction"]))
        var warn_hex := HudStyle.WARN.to_html(false)
        row.text = HudComposeVocab.IMPROVEMENT_DEAL_FEED_FORMAT % [
            stance_terms, warn_hex, preparing_terms, payoff_terms,
            SourceForecast.format_magnitude(feed)] if has_feed \
            else HudComposeVocab.IMPROVEMENT_DEAL_FORMAT % [
                stance_terms, warn_hex, preparing_terms, payoff_terms]
    if has_feed and float(deal["payoff"]) * output < SourceForecast.FOOD_FLOW_MIN:
        row.text += "\n%s" % HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE
        hex = HudStyle.WARN
    row.add_theme_color_override("default_color", hex)
    target.add_child(row)

## One of the deal's terms as a products string, CLAMPED BY THE CREW. The deal carries CEILINGS, and a
## crew below max-useful does not reach them — so each account is priced through
## `SourceForecast.expected_yield_account`, the same `min(workers x per_worker, ceiling)` the sim
## applies per component, and only then scaled by `dip` (1.0 for the stance term, the rung's build
## fraction for the "while building" one). Scaling the CAPPED take rather than the ceiling is what
## keeps the two terms in exact proportion at every crew size.
##
## Each account renders only when non-zero (`picker_products`), so a staple reads `+0.96 food`, flax
## `+0.24 trade`, and hay ground both plus its fodder.
func _account_products(deal: Dictionary, workers: int, band: Dictionary, dip: float) -> String:
    var stance_forecast: Dictionary = deal["stance_forecast"]
    return SourceForecast.picker_products(
        SourceForecast.expected_yield_account(
            stance_forecast, workers, band, "per_worker", "ceiling") * dip,
        SourceForecast.expected_yield_account(
            stance_forecast, workers, band, "per_worker_trade", "ceiling_trade") * dip,
        SourceForecast.expected_yield_account(
            stance_forecast, workers, band, "per_worker_fodder", "ceiling_fodder") * dip)

## THE overdraw test: a take above the source's renewable-sustainable ceiling (by more than the
## epsilon) draws the source down. One definition, shared by the confirmed allocation rows
## (`SourceForecast.source_yield_readout`) and the local hunt's pre-assign yield preview.
func _is_overdraw(actual: float, sustainable: float) -> bool:
    return actual > sustainable + HudComposeVocab.OVERHUNT_EPSILON

## The Extend-pen affordance on a selected PENNED herd (Grazing 2d-γ). While no ring is in flight
## (`pen_extend_progress == 0`) it offers an "Extend pen" button that issues `extend_pen <faction>
## <x> <y>` at the pen's anchor (a penned herd sits AT `corralled_at`, so the herd's own tile is the
## anchor). While a ring is being worked off (`pen_extend_progress > 0`) the button is replaced by a
## WARN-amber "Fencing N%" badge — the pen twin of the corral-build "Building N%" meter. The server
## rejects an extend at max radius / unowned / Herding-unknown with a feed message; the client does
## not pre-gate on those (max radius is not on the wire).
func _build_extend_pen_control(herd: Dictionary, target: VBoxContainer) -> void:
    var extend_progress := float(herd.get("pen_extend_progress", 0.0))
    if extend_progress > 0.0:
        var badge := Label.new()
        badge.text = HudComposeVocab.PEN_FENCING_LABEL % int(round(extend_progress * HudConst.PROGRESS_PERCENT_SCALE))
        badge.add_theme_color_override("font_color", HudStyle.WARN)
        target.add_child(badge)
        return
    var x := int(herd.get("x", -1))
    var y := int(herd.get("y", -1))
    if x < 0 or y < 0:
        return
    var extend_btn := Button.new()
    extend_btn.text = HudComposeVocab.PEN_EXTEND_LABEL
    extend_btn.tooltip_text = HudComposeVocab.PEN_EXTEND_TOOLTIP
    HudStyle.apply_button(extend_btn, "ghost")
    extend_btn.pressed.connect(_emit_extend_pen.bind(x, y))
    target.add_child(extend_btn)

## Emit the extend-pen request for the pen anchored at (x, y). Main formats `extend_pen <faction> <x> <y>`.
func _emit_extend_pen(x: int, y: int) -> void:
    emit_signal("extend_pen_requested", {
        "faction": HudConst.PLAYER_FACTION_ID,
        "x": x,
        "y": y,
    })

## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _herd_compose_available(herd):
        return
    var resolved := _resolve_assign_band()
    var herd_id := String(herd.get("id", ""))
    # When the selected herd changes, default the actor band to the resolved band (and re-seed
    # the compose count/policy from its staffing); otherwise preserve the picked band + count
    # across per-snapshot re-renders of the same herd.
    var source_changed := _compose.hunt_key() != herd_id
    if source_changed:
        _compose.begin_hunt_source(herd_id, int(resolved.get("entity", -1)))
    # The actor is the band-picker selection; fall back to the resolved band if it has vanished.
    var band := _band_labor.player_band_by_entity(_compose.hunt_band())
    if band.is_empty():
        band = resolved
        _compose.set_hunt_band(int(band.get("entity", -1)))
    # THE BAND'S STANDING RUNG ON THIS HERD, or "" when it does not hunt this herd at all. The staffing
    # test is what makes it meaningful: `policy_for_hunt` answers with the DEFAULT for an unstaffed
    # source, so calling it blind would make every fresh sheet look as though the band were standing on
    # Sustain — and the reset below would then never fire on a genuinely stale composition.
    var standing_hunt := _band_labor.policy_for_hunt(band, herd_id) \
        if _band_labor.workers_for_hunt(band, herd_id) > 0 else ""
    # THE SECOND AXIS's standing value (issue #442) — what the band is already BUILDING here. It seeds
    # the improvement control so a herd mid-Tame opens with its box checked rather than looking
    # untouched, and it is what the commit compares against to decide whether a verb needs sending.
    var standing_improvement := _band_labor.improvement_for_hunt(band, herd_id)
    if source_changed:
        var staffed := _band_labor.workers_for_hunt(band, herd_id)
        _compose.seed_hunt(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.policy_for_hunt(band, herd_id), standing_improvement)
    # Show the effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _band_labor.effective_hunt_workers(band, herd_id)
    var pending := _band_labor.pending_assigns_for(int(band.get("entity", -1))).has(_band_labor.pending_key(SourceForecast.LABOR_KIND_HUNT, -1, -1, herd_id))
    # The sheet's own header already names the verb ("ASSIGN HERDERS") and the herd, so this line
    # carries only what the header cannot: the standing staffing being edited.
    if current > 0 or pending:
        var title := Label.new()
        title.text = HudComposeVocab.COMPOSE_NOW_STAFFED_FORMAT % [current, HudComposeVocab.COMPOSE_PENDING_SUFFIX if pending else ""]
        title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
        target.add_child(title)
    # Which band supplies the hunters (above the worker/party stepper, so it reads "which band →
    # how many workers"). Switching bands re-runs the distance-aware branch below for that band.
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _compose.set_hunt_band(int(picked.get("entity", -1)))
        _build_herd_assign_controls(_live_herd(herd_id, herd), target)))
    # Distance-aware: a LOCAL hunt when the herd is within the SELECTED band's hunt_reach, a hunting
    # EXPEDITION when it's beyond. Distance is wrap-aware from the picked band's OWN tile — every part
    # of the decision (distance, reach, and the command's band target) keys off `band` explicitly, so
    # the right band drives it even with multiple bands (single-band playtest can't surface a mixup).
    var herd_x := int(herd.get("x", -1))
    var herd_y := int(herd.get("y", -1))
    var band_tile := SourceForecast.band_tile(band)
    var reach := int(band.get("hunt_reach", 0))
    var distance := SourceForecast.hex_distance_wrapped(
        band_tile.x, band_tile.y, herd_x, herd_y, _band_labor.grid_width(), _band_labor.wrap_horizontal())
    # Beyond reach → expedition. Unknown distance (missing tiles) falls back to the local hunt.
    var is_expedition := distance >= 0 and distance > reach
    # Local hunt caps at the band's assignable hunt workers; an expedition caps at the party ceiling.
    var assignable := SourceForecast.expedition_party_cap(band) if is_expedition else _band_labor.assignable_hunt_workers(band, herd_id)
    # **THE STANCE ROW IS THE SAME FOUR RUNGS ON BOTH BRANCHES NOW** (issue #442). It was six on a
    # local hunt and four on an expedition, and every line of the ceiling-filtering, standing-rung
    # re-admitting machinery that produced that difference existed to cram the build verbs in here.
    # Corral being local-only is still true — it is an IMPROVEMENT, and the improvement control is
    # simply not built on the expedition branch, because a detached party builds no pen.
    var hunt_options: Array = SourceForecast.LABOR_HUNT_POLICIES
    # THE SHEET NEVER RENDERS A STANCE THE BAND IS NOT ON. A stance is never gated and never retires,
    # so this can now only fire on a malformed composition (a harness staging a bogus rung).
    if not (_compose.hunt_policy() in hunt_options):
        _compose.set_hunt_policy(SourceForecast.DEFAULT_HUNT_POLICY)
    # Pre-commit forecast — LOCAL hunt only. An expedition travels for several turns and accumulates
    # toward a carry cap, so the herd's per-turn take ceiling is NOT the bound on its party size;
    # forecasting a per-turn yield for it would be a lie. On a local hunt the ceiling caps the
    # stepper (no over-assigning) and drives the live expected-yield row; both recompute here on
    # every stepper/policy change, since both re-render these controls.
    var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_policy())
    # THE COMPOSED IMPROVEMENT — the second axis, LOCAL hunt only (a detached party builds no pen).
    # The deal it states rides the SELECTED stance, which is the whole point of the split: a Deplete
    # builder's dip is a fraction of Deplete's larger ceiling, and it defeats itself through the
    # ecology (the meter accrues only while the herd is Thriving) rather than through a gate.
    var composed_improvement := SourceForecast.IMPROVEMENT_NONE if is_expedition \
        else _compose.hunt_improvement()
    # The party stepper caps at the max-useful count on BOTH branches — a raid's haul (`animals_taken`)
    # PLATEAUS with party size once the herd's surplus binds, so extra hunters past the plateau raid no
    # more animals and should be flagged idle exactly as an over-staffed local hunt is (the silent-idle-
    # hunter gap this pass closes). The local branch caps at the source's max-useful ceiling.
    # A managed herd needs a herding crew EVERY turn to hold its tameness, which the take/prepare
    # max-useful knows nothing about — so the LOCAL-hunt cap's usefulness ceiling is floored on
    # `SourceForecast.herd_crew_floor`, the ONE definition of that number, shared with the Band panel's
    # worked-row twin (`source_worker_cap_state`) so the sheet and the board can never gate differently.
    # It reads the IMPROVEMENT axis to pick the ownership-gated vs would-be crew field; the rationale
    # for that split lives on the helper. The expedition party has no herding crew, so
    # `SourceForecast.expedition_useful_cap` is left alone.
    var capped := SourceForecast.expedition_useful_cap(band, herd, _compose.hunt_policy(), assignable) if is_expedition \
        else _forecast_worker_cap(forecast, assignable, SourceForecast.herd_crew_floor(
            herd, composed_improvement != SourceForecast.IMPROVEMENT_NONE))
    var cap := int(capped["cap"])
    # Auto-max on policy select — "give me everything this herd sustains": the max-useful for the policy
    # (clamped to idle below), which guarantees zero waste + the full rate. Only ever set by a policy
    # click (both branches), never by a −/+ tick, so manual counts survive the rebuild.
    if _compose.consume_hunt_autofill():
        _compose.set_hunt_count(cap)
    _compose.clamp_hunt_count(cap)
    # A managed herd's local crew are HERDERS/keepers (workersNeeded scales with the herd), not a hunt
    # party — so a pen needing several keepers doesn't read as a hunt-party bug (fix #6).
    var crew_label := HudComposeVocab.HERD_CREW_LABEL \
        if SourceForecast.is_managed_hunt_source(herd, composed_improvement) \
        else HudComposeVocab.HUNT_CREW_LABEL
    # Ascending per-policy takes under BOTH pickers so all three (forage / local hunt / expedition) wear
    # the same "up to X/turn" button metric: each policy's MAX obtainable food/turn (Sustain < Surplus <
    # Deplete < Eradicate). Worker-independent on both branches (the expedition's is the max over party
    # sizes of delivered_food / trip_turns, so it never changes as the Party stepper steps).
    var policy_takes := SourceForecast.expedition_policy_takes(band, herd, _band_labor.grid_width(), _band_labor.wrap_horizontal()) if is_expedition \
        else _hunt_policy_takes(herd)
    # **STANCE FIRST, THEN THE CREW — the SAME vertical grammar the forage sheet reads in.** The stepper
    # used to sit directly under the band picker, so the hunt sheet asked "how many hunters?" before it
    # asked what they would be doing, and the two compose sheets disagreed about the order of the one
    # decision they both make. You choose the action, then staff it. Nothing about the mechanics moved:
    # the cap is still recomputed from the composed stance (a policy click re-renders and may auto-fill
    # the crew) and the forecast below still reads the current crew — only the position changed.
    target.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
        _compose.set_hunt_policy(policy)
        # Picking a policy auto-fills the crew to that policy's max-useful (consumed next rebuild).
        _compose.arm_hunt_autofill()
        _build_herd_assign_controls(_live_herd(herd_id, herd), target), _compose.hunt_policy(), hunt_options, policy_takes))
    # The hint under the picker is per BRANCH, never shared: a resident band and a detached party earn
    # DIFFERENT payoffs from the same policy word (both trade the take since #337, but only the band's
    # Sustain builds husbandry — an expedition accrues none), so one shared line would promise the
    # expedition player a payoff the sim never pays. The expedition's slot carries the distance refusal
    # instead — it is that branch's answer to "what does this stance mean here?".
    if is_expedition:
        target.add_child(HudWidgets.alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
    else:
        # What this stance DOES for a resident band (the forecast line further down carries the number;
        # this carries the consequence for the herd). Deliberately NOT the expedition hints.
        target.add_child(HudWidgets.alloc_hint_label(
            String(HudComposeVocab.LOCAL_HUNT_POLICY_HINTS.get(_compose.hunt_policy(), ""))))
    # THE CREW, beneath the stance it staffs — with its cap note, which explains THIS stepper's dead `+`
    # and therefore travels with it.
    target.add_child(HudWidgets.build_worker_stepper(
        "Party" if is_expedition else crew_label, _compose.hunt_count(), _compose.hunt_count() < cap,
        func(n: int) -> void:
            _compose.set_hunt_count(clampi(n, 0, cap))
            _build_herd_assign_controls(_live_herd(herd_id, herd), target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # WOULD THIS SUBMIT CHANGE ANYTHING? — the forage sheet's rule, on the hunt web, because
    # `workers == 0` means the SAME two different things here (the sim's `assign_labor` skips validation
    # entirely at 0, so the unassign is always legal). `current` is the pending-aware standing crew on
    # this herd for THIS band, so:
    #   • 0 on a herd this band does NOT hunt → the command would do nothing. Dead button, still the
    #     verb, and the reason spelled out beside it.
    #   • 0 on a herd it DOES hunt → the sim's unassign. Live button, renamed, and NO improvement
    #     control (below) — a panel offering to start a build in the act of abandoning the source
    #     argues with itself.
    # Gating on the raw count instead would fix the no-op and break the unassign the Work zone needs.
    # EXPEDITION IS NOT IN THIS FAMILY: a raid is a launch, not an edit of a standing assignment, so
    # there is no crew to hand back and a party of 0 is simply refused (the disable below).
    var is_unassign := not is_expedition and _compose.hunt_count() <= 0 and current > 0
    var is_noop := not is_expedition and _compose.hunt_count() <= 0 and current <= 0
    var assign_btn := Button.new()
    if is_expedition:
        # LIVE turns-to-fill for the party + policy currently dialed. This block re-renders on every
        # stepper tick and policy click, so the forecast tracks the compose state instead of arriving
        # as a confirmation — and it comes from the SAME helpers the targeting banner uses, so the two
        # entry points can never quote different numbers.
        # `trip`, NOT `forecast`: the outer `forecast` is the LOCAL hunt's per-turn ceiling inputs
        # (client arithmetic over the BAND flow ceiling). This one is the sim's forward-simulated TRIP
        # estimate — a pure table lookup, zero client arithmetic. The two must never be confused.
        var trip := SourceForecast.hunt_trip_forecast(band, herd, _compose.hunt_policy(), _compose.hunt_count(),
            _band_labor.grid_width(), _band_labor.wrap_horizontal())
        var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip, _herd_label_for_id(herd_id))
        if forecast_line != "":
            target.add_child(HudWidgets.forecast_label(forecast_line))
        # The no-surplus refusal — computed ONCE and used for both the button tooltip and the reason
        # line, and identical to what the targeting flow posts to the command feed.
        var no_surplus := SourceForecast.hunt_trip_no_surplus(trip)
        var reason := SourceForecast.hunt_no_surplus_reason(herd) if no_surplus else ""
        SourceForecast.style_send_hunt_button(assign_btn, trip, reason)
        # The reason is spelled out beside the button too — a disabled control's tooltip is easy to miss.
        if no_surplus:
            target.add_child(HudWidgets.alloc_hint_label(reason))
    else:
        # The averaging-window disclaimer USED TO STAND HERE, as a wrapped body line under the hint: the
        # delivered rate is a long-run average of lumpy whole-animal delivery. It is a caveat on ONE
        # number, so it now rides the RUNG's tooltip beside the metric it qualifies (`_hunt_policy_takes`
        # fills the take pair's `note`) — the panel is where the hunt sheet could least afford a sentence
        # the forage sheet has no counterpart for. The window computation is unchanged.
        # LIVE per-turn yield for the STANCE being composed (no carry cap on a local hunt, so
        # turns-to-fill is meaningless — food/turn is the number that decides it). Every stance renders
        # it now: the "one yield row per rung" split existed because a build verb occupied the same
        # control, and a dip→payoff pair and a bare rate cannot share one line. They no longer do —
        # the stance states the take, the improvement control below states the deal.
        var yield_line := _local_hunt_preview_bbcode(
            band, herd, _compose.hunt_policy(), _compose.hunt_count())
        if yield_line != "":
            target.add_child(HudWidgets.forecast_label(yield_line))
        # THE IMPROVEMENT ROW — the second axis, beneath the stance it multiplies. Nothing is offered on
        # an UNASSIGN, for the reason the forage sheet already records: what abandoning costs is stated
        # in the rung's own hint ("It must stay staffed or the herd goes wild again"), so a second
        # warning at the moment of unassigning states one fact twice.
        if not is_unassign:
            _build_improvement_control(SourceForecast.LABOR_KIND_HUNT, herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_policy(), composed_improvement,
                band, _compose.hunt_count(), crew_label,
                func(improvement: String) -> void:
                    _compose.set_hunt_improvement(improvement)
                    _build_herd_assign_controls(_live_herd(herd_id, herd), target),
                target)
        # A dead button is always explained (the `+` stepper's cap note is the precedent) — but only
        # when the cap note has not already said it, so the panel never states one fact twice.
        if is_noop and cap_note == "":
            target.add_child(HudWidgets.alloc_hint_label(
                String(HudComposeVocab.HUNT_NOOP_HINTS.get(crew_label, ""))))
        assign_btn.text = HudComposeVocab.UNASSIGN_BUTTON if is_unassign \
            else HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON
        HudStyle.apply_button(assign_btn, "primary")
        assign_btn.disabled = is_noop
    if is_expedition:
        assign_btn.set_meta(HudWidgets.SEND_HUNT_CONFIRM_META, true)
        # A hunting expedition needs a positive party; a local hunt allows 0 (removes the assignment).
        # `SourceForecast.style_send_hunt_button` already disabled it when the raid returns empty (no surplus); a
        # positive party is the other precondition. (`or` — never clear a disable the style step set.)
        assign_btn.disabled = assign_btn.disabled or _compose.hunt_count() <= 0
        assign_btn.pressed.connect(func() -> void:
            if _compose.hunt_count() <= 0 or SourceForecast.hunt_trip_no_surplus(
                    SourceForecast.hunt_trip_forecast(band, herd, _compose.hunt_policy(), _compose.hunt_count(),
            _band_labor.grid_width(), _band_labor.wrap_horizontal())):
                return
            emit_signal("send_hunt_expedition_requested", {
                "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
                "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
                "party_workers": _compose.hunt_count(),
                "fauna_id": herd_id,
                "fauna_label": SourceForecast.herd_display_name(herd),
                "policy": _compose.hunt_policy() if _compose.hunt_policy() in SourceForecast.LABOR_HUNT_POLICIES else SourceForecast.DEFAULT_HUNT_POLICY,
            })
            # Committing is the end of the compose act — return to the read state (§15).
            close_compose_sheet())
    else:
        assign_btn.pressed.connect(func() -> void:
            # ORDER IS LOAD-BEARING: `assign_labor` first, the improvement verb second. The sim's
            # improvement commands act on the bands ALREADY WORKING the source, so a verb sent to an
            # unstaffed herd is rejected outright — the crew has to land first.
            _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, _compose.hunt_count(),
                herd_x, herd_y, herd_id, _compose.hunt_policy(), "", composed_improvement)
            _emit_improvement(band, SourceForecast.LABOR_KIND_HUNT, composed_improvement,
                standing_improvement, herd_x, herd_y, herd_id)
            close_compose_sheet())
    target.add_child(assign_btn)







## Each STANCE's per-turn take on this forage patch — the stance ceiling from the shared
## `SourceForecast.forecast_inputs` (per turn at output 1.0, like the hunt band ceiling), for the
## FORAGE picker's ascending per-rung readout. The plant twin of `_hunt_policy_takes`, so both pickers
## wear the same button metric. Empty entries (dead-season patch / older snapshot) are skipped.
##
## **ALL THREE ACCOUNTS (#426).** This used to hand the shared joiner an explicit `0.0` for trade, on
## the standing claim that the plant web projected no trade rate — so a flax patch, which pays trade
## and no food, rendered `0.00 food` on every rung and read exactly like the worthless-source lie
## #337 removed from the hunt picker. A patch's per-policy ROW now carries provisions, trade goods and
## fodder, and each is rendered only when non-zero, so a staple reads `0.96 food`, flax `0.24 trade`,
## and hay ground `0.08 food · 0.40 fodder`.
##
## **The Cultivate/Sow PAYOFF faces left with the build verbs** (issue #442): they were a second loop
## here, wearing the crop-substituted payoff because a build verb was a rung of this picker. The
## improvement control states the same terms now, against the same crop, through the same
## `_crop_payoff_terms`.
func _forage_policy_takes(tile_info: Dictionary) -> Dictionary:
    var takes := {}
    for policy in SourceForecast.LABOR_HUNT_POLICIES:
        var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, String(policy))
        if not bool(forecast["known"]):
            continue
        takes[String(policy)] = SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]),
            maxf(float(forecast["ceiling_trade"]), 0.0),
            maxf(float(forecast["ceiling_fodder"]), 0.0))
    return takes

## The tile "Assign foragers" controls (compose a count, then Assign). Shown only for a
## tile with a food module while a player band exists to staff it — and only on a hex the player can
## actually SEE (a workable patch is live state, redacted from a remembered tile like its occupants;
## MapView already strips `food_module*` there, and this holds the line if anything ever feeds a
## non-redacted dict).
## May this basket entry be committed under `policy`? Species-GLOBAL legality ONLY (`can_cultivate` /
## `can_sow` = "can this plant ever climb this rung"). `share` answers the other question — whether a
## legal crop is a WISE one here — and it must never disable anything.
func _flora_entry_allows(entry: Dictionary, policy: String) -> bool:
    if policy == HudConst.LABOR_POLICY_SOW:
        return bool(entry.get("can_sow", false))
    return bool(entry.get("can_cultivate", false))

## What committing this entry under `policy` pays relative to gathering it wild. `FLORA_CROP_RATIO_NONE`
## on a rung the species cannot climb — the sentinel, never printed as a number.
func _flora_entry_ratio(entry: Dictionary, policy: String) -> float:
    if policy == HudConst.LABOR_POLICY_SOW:
        return float(entry.get("sow_yield_ratio", SourceForecast.FLORA_CROP_RATIO_NONE))
    return float(entry.get("cultivate_yield_ratio", SourceForecast.FLORA_CROP_RATIO_NONE))

## The FODDER (hay) this entry would pay per turn under `policy` — >0 marks a crop whose vector feeds
## animals. `FLORA_CROP_RATIO_NONE` (0) where the vector pays no fodder or the plant cannot climb this
## rung.
##
## **PER RUNG, exactly like `_flora_entry_ratio` above** (#419). It read `sow_fodder_payoff`
## unconditionally — a *Field* payoff — so the Cultivate row stated what a sown field would pay, on a
## rung that pays a drawn-down MSY skim off a merely-weeded basket. The sim ships both rungs' figures.
func _flora_entry_fodder_payoff(entry: Dictionary, policy: String) -> float:
    if policy == HudConst.LABOR_POLICY_SOW:
        return float(entry.get("sow_fodder_payoff", SourceForecast.FLORA_CROP_RATIO_NONE))
    return float(entry.get("cultivate_fodder_payoff", SourceForecast.FLORA_CROP_RATIO_NONE))

## The TRADE this entry would credit to the faction trade_goods stockpile per turn under `policy` —
## the exact twin of the fodder payoff above, per rung for the same reason. `FLORA_CROP_RATIO_NONE` (0)
## where the vector pays no trade or the plant cannot climb this rung.
##
## **A NON-ZERO VALUE HERE DOES NOT MEAN "CASH CROP".** Every staple carries the flat
## `trade_goods_per_biomass: 0.005` token, so all 27 of them quote a small real number — which is why
## the row states each account it finds rather than branching on one of them to pick a single account.
func _flora_entry_trade_payoff(entry: Dictionary, policy: String) -> float:
    if policy == HudConst.LABOR_POLICY_SOW:
        return float(entry.get("sow_trade_payoff", SourceForecast.FLORA_CROP_RATIO_NONE))
    return float(entry.get("cultivate_trade_payoff", SourceForecast.FLORA_CROP_RATIO_NONE))

## The rung noun the payoff tooltips name — "a tended patch" under Cultivate, "a sown field" under Sow.
## These payoffs are per-rung, so a tooltip that named the wrong rung would restate the very bug the
## per-rung split fixed.
func _flora_rung_noun(policy: String) -> String:
    return String(HudFloraVocab.FLORA_CROP_RUNG_NOUNS.get(
        policy, HudFloraVocab.FLORA_CROP_RUNG_NOUN_FALLBACK))

## **The row face for one basket entry: every account this plant actually pays, none it does not.**
## The base share row, then a ratio / hay / trade clause each gated by whether its component is really
## there — food leading, the shared render-only-when-non-zero rule (`SourceForecast.has_component` is
## THE gate, so the two non-food accounts are judged exactly as the hunt faces judge trade).
##
## The ratio keeps its own `> FLORA_CROP_RATIO_NONE` test rather than `has_component`: `0` there is the
## **cannot-climb sentinel**, not a small rate, and the sentinel must never print as `0.0×`.
func _flora_row_face(crop_name: String, percent: int, ratio: float, fodder: float,
        trade: float) -> String:
    var face := HudFloraVocab.FLORA_SHARE_FORMAT % [crop_name, percent]
    if ratio > SourceForecast.FLORA_CROP_RATIO_NONE:
        face += HudFloraVocab.FLORA_CROP_RATIO_CLAUSE_FORMAT % ratio
    if SourceForecast.has_component(fodder):
        face += HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT % fodder
    if SourceForecast.has_component(trade):
        face += HudFloraVocab.FLORA_CROP_TRADE_CLAUSE_FORMAT % trade
    return face

## Provisions/turn this rung pays once complete, committed to THIS species — the sim's own number, in
## the same units and output-multiplier convention as the forecast `payoff` it replaces. 0 (never
## substituted) on a rung the species cannot climb.
func _flora_entry_payoff(entry: Dictionary, policy: String) -> float:
    if policy == HudConst.LABOR_POLICY_SOW:
        return float(entry.get("sow_payoff", 0.0))
    return float(entry.get("cultivate_payoff", 0.0))

## The "then <X>" terms for a plant rung, with the patch's species-BLIND payoff replaced by the
## SELECTED crop's own. Without this the deal's payoff term quotes one number no matter which crop is
## picked, so the picker appears to change nothing above it — the player commits to Reeds and is shown
## Wild Emmer's payoff. A SUBSTITUTION, not a calculation: the client does no arithmetic on the sim's
## figures. Falls back to the patch quote when nothing is being committed (no selection, or an
## already-committed patch, for which the patch quote IS the right answer — the sim takes
## `tendedYield`/`fieldYield` against the patch's own species).
##
## **A ZERO PAYOFF IS SUBSTITUTED, NOT SKIPPED** (#419). An earlier version bailed out on
## `payoff <= 0.0` and left the *previous* crop's number standing — so picking a crop that pays no food
## on this rung left the line asserting food it will never deliver. The case is real: a **sown Field**
## is 100% its crop, so a cash crop's `sow_payoff` is exactly `0`. Zero is the honest answer there and
## the line must say so; quoting a different crop is the one thing it must never do. Every account is
## substituted TOGETHER, so the face can never mix one crop's food with another's fodder.
func _crop_payoff_terms(tile_info: Dictionary, entries: Array[Dictionary], species: String,
        band: Dictionary, rung: String) -> String:
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var deal := SourceForecast.improvement_forecast(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.DEFAULT_HUNT_POLICY, rung)
    if deal.is_empty():
        return ""
    var payoff := float(deal["payoff"])
    var trade := float(deal["payoff_trade"])
    var fodder := float(deal["payoff_fodder"])
    if species != "":
        for entry in entries:
            if String(entry["species"]) != species:
                continue
            payoff = _flora_entry_payoff(entry, rung)
            trade = _flora_entry_trade_payoff(entry, rung)
            fodder = _flora_entry_fodder_payoff(entry, rung)
            break
    return SourceForecast.picker_products(payoff * output, trade * output, fodder * output)

## The crop this compose will SEND: the player's pick while it is still legal on this tile+rung, else
## the HIGHEST-SHARE legal entry — which is the sim's own `default_species_for_rung`, so picking
## nothing and accepting the default behave identically. Returns "" (send nothing, still valid) for a
## non-committing rung, an already-committed patch, or a basket with no legal plant.
func _resolve_crop_selection(entries: Array[Dictionary], policy: String, committed: bool, picked: String) -> String:
    if committed or not (policy in SourceForecast.FORAGE_IMPROVEMENTS):
        return ""
    var default_species := ""
    for entry in entries:
        if not _flora_entry_allows(entry, policy):
            continue
        var species := String(entry["species"])
        if species == picked:
            return picked
        if default_species == "":
            # Wire order is share-DESC, so the FIRST legal entry is the highest-share legal one.
            default_species = species
    return default_species

## The crop picker — one row per plant in the tile's basket, in wire order, `Wild Emmer 56%`. An
## illegal entry is greyed WITH ITS REASON but never hidden (see FLORA_CROP_NO_CULTIVATE_FORMAT); a
## legal-but-marginal one is fully pressable. Returns null when there is nothing to render (a biome
## that carries no named forage), so no empty block appears.
##
## A COMMITTED PATCH SHOWS THE SAME BASKET, LOCKED — never a lone crop name. The commitment is still
## one-way until it lapses, so every row is non-interactive and the `Already committed …` line stays;
## what changed is that the rows are no longer REPLACED by the name. A bare readout beside a tile card
## listing three plants had the two panels of one tile disagreeing about what grows there, and it read
## as "this tile is Wild Emmer now" — the belief issue #433 deleted, since a commitment REWEIGHTS the
## basket over the build rather than emptying it (and moves it not at all until the build lands).
## The committed row renders SELECTED-and-locked via `HudStyle.apply_button`'s `selected_when_disabled`
## — plain disabled styling fades the border and the ink to `INK_FAINT`, erasing the one mark of which
## crop is current. **This is that flag's ONLY caller now**: the policy picker's standing-but-gated
## rung, which it was originally written for (#420), went with the stance/improvement split (#442).
## It is the basket's twin of the
## `HudStyle.SIGNAL` mark the tile card puts on the same species, so the two panels read as one fact.
func _build_crop_picker(
    entries: Array[Dictionary],
    policy: String,
    selected: String,
    committed_species: String,
    on_pick: Callable) -> Control:
    var committed := committed_species.strip_edges()
    var is_committed := committed != ""
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudFloraVocab.FLORA_CROP_BLOCK_SEPARATION)
    # A committed patch keeps its block even on the (theoretical) empty basket, so the standing
    # commitment is still stated; an uncommitted one with nothing to pick has nothing to say at all.
    if entries.is_empty() and not is_committed:
        return null
    block.add_child(HudWidgets.alloc_section_label(
        HudFloraVocab.FLORA_CROP_COMMITTED_HEADER if is_committed else HudFloraVocab.FLORA_CROP_PICKER_HEADER))
    var rows := VBoxContainer.new()
    rows.add_theme_constant_override("separation", HudFloraVocab.FLORA_CROP_BLOCK_SEPARATION)
    rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var any_legal := false
    for entry in entries:
        var species := String(entry["species"])
        var crop_name := String(entry["display_name"])
        var percent := int(entry["percent"])
        var legal := _flora_entry_allows(entry, policy)
        var ratio := _flora_entry_ratio(entry, policy)
        # ALL THREE ACCOUNTS OF THIS RUNG, read per rung. A plant pays into as many of them as its
        # yield vector has components — a staple food AND its trade token, a cash crop trade AND (at
        # rung 2, which weeds rather than replaces) the volunteers' calories — so nothing here picks
        # one account to state.
        var fodder_payoff := _flora_entry_fodder_payoff(entry, policy)
        var trade_payoff := _flora_entry_trade_payoff(entry, policy)
        var btn := Button.new()
        # The face states each account that is really there and nothing else — so a row greyed by the
        # climbability flags is a bare `Name 12%` (printing "0.0×" there would read as "a crop worth
        # nothing" rather than "not a crop at this rung").
        btn.text = _flora_row_face(crop_name, percent, ratio, fodder_payoff, trade_payoff)
        btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        # WHICH ROW IS MARKED depends on which question the block is asking: an open picker marks the
        # composed pick (and only if that pick is legal), a committed one marks the crop the patch is
        # already on, whose legality at this rung is moot — the commitment is already made.
        var marked := (species == committed) if is_committed else (legal and species == selected)
        # A committed block is a READOUT: every row locked, including the marked one.
        # `selected_when_disabled` is what keeps the mark visible through the disabled treatment
        # (see the header note) — and this is the last surface in the client that needs it.
        HudStyle.apply_button(btn, "primary" if marked else "ghost", marked)
        # A row must be EXACTLY `FLORA_CROP_ROW_HEIGHT` — the list's cap is derived from it, so a row
        # wearing the default button chrome would silently break that maths (the work board's rule).
        HudWidgets.compact(btn, HudFloraVocab.FLORA_CROP_ROW_FONT_SIZE, HudFloraVocab.FLORA_CROP_ROW_PADDING_V)
        btn.custom_minimum_size = Vector2(0.0, HudFloraVocab.FLORA_CROP_ROW_HEIGHT)
        # A committed patch locks EVERY row — a pressable one would imply a switch the sim will refuse.
        btn.disabled = is_committed or not legal
        if legal:
            any_legal = true
            # THE TOOLTIP IS COMPOSED THE SAME WAY THE FACE IS: the food verdict (which is about the
            # ratio) and then a clause per non-food account, each only where the component exists. It
            # used to be a five-way elif in which a hay or trade payoff SUPPRESSED the food verdict
            # entirely — the tooltip half of the one-account-per-row defect.
            var tooltip_lines: Array[String] = []
            # A LOSS-MAKING but legal crop: warn ink, FULLY pressable. Never hidden, clamped, sorted
            # by, or disabled — the ratio is there to stop a bad idea being invisible, not to forbid it.
            # **A cash crop earns this ink honestly at rung 2** and must not be exempted: weeding
            # cotton up through the basket really does pay less food than gathering the tile wild, and
            # that surrendered calorie is the cost the trade clause below is the benefit of.
            if ratio > SourceForecast.FLORA_CROP_RATIO_NONE and ratio < HudFloraVocab.FLORA_CROP_BREAK_EVEN_RATIO:
                btn.add_theme_color_override("font_color", HudStyle.WARN)
                btn.add_theme_color_override("font_hover_color", HudStyle.WARN)
                tooltip_lines.append(HudFloraVocab.FLORA_CROP_LOSS_TOOLTIP_FORMAT % [crop_name, ratio])
            elif ratio >= HudFloraVocab.FLORA_CROP_STRONG_RATIO:
                tooltip_lines.append(HudFloraVocab.FLORA_CROP_STRONG_TOOLTIP_FORMAT % [crop_name, ratio])
            elif ratio > SourceForecast.FLORA_CROP_RATIO_NONE:
                tooltip_lines.append(HudFloraVocab.FLORA_CROP_MODEST_TOOLTIP_FORMAT % [crop_name, ratio])
            var rung_noun := _flora_rung_noun(policy)
            if SourceForecast.has_component(fodder_payoff):
                tooltip_lines.append(HudFloraVocab.FLORA_CROP_FODDER_TOOLTIP_FORMAT
                    % [crop_name, fodder_payoff, rung_noun])
            if SourceForecast.has_component(trade_payoff):
                tooltip_lines.append(HudFloraVocab.FLORA_CROP_TRADE_TOOLTIP_FORMAT
                    % [crop_name, trade_payoff, rung_noun])
            if not tooltip_lines.is_empty():
                btn.tooltip_text = "\n".join(tooltip_lines)
            # The tooltips above still earn their keep on a locked row (what the plant pays is a fact
            # about the plant), but a committed row gets no handler at all — not merely a dead one.
            if not is_committed:
                btn.pressed.connect(func() -> void: on_pick.call(species))
        else:
            var reason_format := HudFloraVocab.FLORA_CROP_NO_SOW_FORMAT if policy == HudConst.LABOR_POLICY_SOW \
                else HudFloraVocab.FLORA_CROP_NO_CULTIVATE_FORMAT
            btn.tooltip_text = reason_format % crop_name
        rows.add_child(btn)
    # A basket longer than the sheet can spare scrolls WITHIN the picker, so the Forage button below
    # stays on screen. Container configuration only — the ScrollContainer's own minimum height is 0,
    # so the capped `custom_minimum_size` IS the height, and a short basket skips the wrapper entirely
    # rather than padding out to the cap.
    if entries.size() > HudFloraVocab.FLORA_CROP_LIST_VISIBLE_ROWS:
        var scroll := ScrollContainer.new()
        scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
        scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        # A ScrollContainer's own minimum height is 0, so this IS its height; a basket short enough to
        # fit skips the wrapper entirely rather than padding out to the cap.
        scroll.custom_minimum_size = Vector2(0.0, HudFloraVocab.FLORA_CROP_LIST_MAX_HEIGHT)
        scroll.add_child(rows)
        block.add_child(scroll)
    else:
        block.add_child(rows)
    # ONE standing line under the list, and only where the rows cannot carry the fact themselves.
    # A COMMITTED block states why nothing here is pressable — the line that used to stand in place of
    # the rows now stands under them. The rung-legality hint is moot there (the choice is behind you),
    # so the two are mutually exclusive rather than stacked.
    if is_committed:
        block.add_child(HudWidgets.alloc_hint_label(HudFloraVocab.FLORA_CROP_COMMITTED_HINT))
    elif not any_legal:
        block.add_child(HudWidgets.alloc_hint_label(HudFloraVocab.FLORA_CROP_NONE_LEGAL_HINT))
    return block

func _build_forage_assign_controls(tile_info: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _forage_compose_available(tile_info):
        return
    var resolved := _resolve_assign_band()
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    # ONE key for this patch: the compose source key and the sheet's subject key are the same string
    # by definition, and the rebuild closures below re-resolve the LIVE tile through it (`_live_tile_info`).
    var subject_key := _forage_source_key(tile_info)
    # When the selected tile changes, default the actor band to the resolved band (and re-seed
    # the count from its staffing); otherwise preserve the picked band + count across the
    # per-snapshot re-renders of the same tile.
    var source_changed := _compose.forage_key() != subject_key
    if source_changed:
        _compose.begin_forage_source(subject_key, int(resolved.get("entity", -1)))
    var band := _band_labor.player_band_by_entity(_compose.forage_band())
    if band.is_empty():
        band = resolved
        _compose.set_forage_band(int(band.get("entity", -1)))
    # THE BAND'S STANDING RUNG ON THIS PATCH, or "" when it does not work this tile at all. The staffing
    # test is what makes it meaningful: `policy_for_forage` answers with the DEFAULT for an unstaffed
    # source, so calling it blind would make every fresh sheet look as though the band were standing on
    # Sustain — and the reset below would then never fire on a genuinely stale composition.
    var standing_forage := _band_labor.policy_for_forage(band, x, y) \
        if _band_labor.workers_for_forage(band, x, y) > 0 else ""
    # THE SECOND AXIS's standing value (issue #442) — what the band is already BUILDING on this patch.
    # Unlike the stance it needs no staffing test: `improvement_for_forage` reads the assignment's own
    # field and answers "" when there is no assignment at all.
    var standing_improvement := _band_labor.improvement_for_forage(band, x, y)
    if source_changed:
        # `seed_forage` also clears the crop: a crop pick belongs to the PATCH it was made on, and a
        # new tile has a different basket.
        var staffed := _band_labor.workers_for_forage(band, x, y)
        _compose.seed_forage(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.policy_for_forage(band, x, y), standing_improvement)
    # Effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _band_labor.effective_forage_workers(band, x, y)
    var pending := _band_labor.pending_assigns_for(int(band.get("entity", -1))).has(_band_labor.pending_key(SourceForecast.LABOR_KIND_FORAGE, x, y, ""))
    # The sheet's own header already names the verb and the subject ("ASSIGN FORAGERS  Nut Grove"),
    # so this line carries only what the header cannot: the standing staffing being edited.
    if current > 0 or pending:
        var title := Label.new()
        title.text = HudComposeVocab.COMPOSE_NOW_STAFFED_FORMAT % [current, HudComposeVocab.COMPOSE_PENDING_SUFFIX if pending else ""]
        title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
        target.add_child(title)
    # Which band supplies the foragers (above the stepper). Switching re-runs the range check below
    # for that band.
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _compose.set_forage_band(int(picked.get("entity", -1)))
        _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target)))
    # Forage harvest STANCE (Sustain/Surplus/Deplete/Eradicate, default Sustain) — the SAME four rungs
    # the hunt picker offers, with forage-appropriate behaviour hints. Persisted across re-renders;
    # re-seeded from current staffing when the tile changes.
    #
    # THE SHEET NEVER RENDERS A STANCE THE BAND IS NOT ON. A stance is never gated and never retires,
    # so this can now only fire on a malformed composition (a harness staging a bogus rung). The
    # gate-and-standing-rung dance this replaced existed for exactly one case — a patch that dropped
    # out of Thriving mid-Cultivate, whose build verb was its policy — and the improvement axis carries
    # that case now, pausing rather than repainting the row.
    if not (_compose.forage_policy() in SourceForecast.LABOR_HUNT_POLICIES):
        _compose.set_forage_policy(SourceForecast.DEFAULT_HUNT_POLICY)
    # THE BASKET IS RESOLVED BEFORE THE RUNG FACES, because the two committing rungs' faces quote the
    # crop they would commit to (issue #419) — a face computed off a species-blind patch reads the same
    # number whichever crop is lit, which is the "nothing above the list moves" half of that issue.
    var basket := SourceForecast.flora_basket_entries(tile_info.get("patch_composition", []))
    # The SPECIES KEY, not the display name: the picker marks the committed row by matching the wire
    # key its entries carry, exactly as the tile card's basket does — one identity, two panels.
    var committed_species := String(tile_info.get("patch_committed_species", "")).strip_edges()
    var is_committed := committed_species != "" \
        and String(tile_info.get("patch_committed_display_name", "")).strip_edges() != ""
    # THE COMPOSED IMPROVEMENT — the second axis. The crop belongs to it, not to the stance, so the
    # rung the crop is resolved against is the one the improvement control will render: the composed
    # verb where one is in flight, else the rung on offer.
    var composed_improvement := _compose.forage_improvement()
    var crop_rung := composed_improvement if composed_improvement != SourceForecast.IMPROVEMENT_NONE \
        else String(RungGates.next_rung_offered(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            composed_improvement, _player_knowledge(),
            HudComposeVocab.FORAGE_FORECAST_PREFIX).get("policy", ""))
    _compose.resolve_forage_species(func(current: String) -> String:
        return _resolve_crop_selection(basket, crop_rung, is_committed, current))
    # Ascending per-stance per-turn takes on the rung buttons, so the forage picker wears the SAME
    # "+X /turn" button metric the local-hunt picker does. The two build verbs no longer ride this
    # picker at all — they wear their payoff on the improvement control below (issue #442).
    var forage_takes := _forage_policy_takes(tile_info)
    target.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
        _compose.set_forage_policy(policy)
        # Picking a stance auto-fills the foragers to that stance's max-useful (consumed next rebuild).
        _compose.arm_forage_autofill()
        _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
        _compose.forage_policy(), SourceForecast.LABOR_HUNT_POLICIES,
        forage_takes, HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS))
    target.add_child(HudWidgets.alloc_hint_label(String(HudComposeVocab.FORAGE_POLICY_HINTS.get(_compose.forage_policy(), ""))))
    # Pre-commit forecast: the patch's per-worker yield + the SELECTED stance's ceiling cap the
    # stepper at max-useful workers, so the player CAN'T over-assign while composing. Both the
    # stepper and the stance picker re-render these controls, so the cap and the preview below
    # recompute on every change (a Deplete/Eradicate ceiling is higher than Sustain's, so switching
    # stance moves the cap).
    var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_policy())
    var capped := _forecast_worker_cap(forecast, _band_labor.assignable_forage_workers(band, x, y))
    var cap := int(capped["cap"])
    # Auto-max on stance select — "give me everything this patch sustains": jump to the max-useful for
    # the stance (clamped to available below). Only ever set by a stance click, never by a −/+ tick.
    if _compose.consume_forage_autofill():
        _compose.set_forage_count(cap)
    _compose.clamp_forage_count(cap)
    target.add_child(HudWidgets.build_worker_stepper(
        HudComposeVocab.FORAGE_CREW_LABEL, _compose.forage_count(), _compose.forage_count() < cap,
        func(n: int) -> void:
            _compose.set_forage_count(clampi(n, 0, cap))
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # WOULD THIS SUBMIT CHANGE ANYTHING? `current` is the pending-aware standing staffing on this tile
    # for THIS band, so the two zero-worker cases are DIFFERENT SUBMITS, and the block below —
    # forecast line and button TOGETHER — has to read coherently for each:
    #   • 0 on a tile this band does NOT work → the command would do nothing. Dead button (still
    #     "Forage"), and the improvement deal states the payoff as a CONDITION ("Assign foragers…").
    #   • 0 on a tile it DOES work → the sim's unassign (server.rs: "Unassigning (workers == 0) is
    #     always allowed"). Live button, renamed, and NO "assign to begin" line — a panel whose button
    #     says Unassign above a line reading "assign to begin" tells the player two opposite things.
    # Gating on the raw count instead would fix the no-op and break the unassign the Work zone needs.
    var is_unassign := _compose.forage_count() <= 0 and current > 0
    var is_noop := _compose.forage_count() <= 0 and current <= 0
    # THE STANCE's live per-turn take + sustainability verdict (`+2.74 /turn · renewable` /
    # `⚠ … — overdraws the patch`). EVERY stance renders it now: the "one yield row per rung" split
    # existed because a build verb occupied this same control and a dip→payoff pair cannot share a line
    # with a bare rate. They no longer share one — the improvement control states the deal below.
    var yield_line := _local_forage_preview_bbcode(
        band, tile_info, _compose.forage_policy(), _compose.forage_count())
    if yield_line != "":
        target.add_child(HudWidgets.forecast_label(yield_line))
    # THE IMPROVEMENT ROW — the second axis, beneath the stance it multiplies. Nothing is forecast for
    # an UNASSIGN: what abandoning costs is already on the card in the rung's own hint ("It must stay
    # staffed or it goes feral"), so a second warning here would state one fact twice.
    if not is_unassign:
        _build_improvement_control(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_policy(), composed_improvement,
            band, _compose.forage_count(), HudComposeVocab.FORAGE_CREW_LABEL,
            func(improvement: String) -> void:
                _compose.set_forage_improvement(improvement)
                _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
            target,
            # THE PAYOFF TERMS FOLLOW THE CROP, on the offered box AND the deal line alike — one
            # Callable, both places, so the two can never quote different crops (issue #419).
            func(rung: String) -> String:
                return _crop_payoff_terms(tile_info, basket, _compose.forage_species(), band, rung),
            # WHICH CROP this rung commits the patch to (flora roster S1), between the box and the
            # deal because it is part of the same decision. Re-resolved every render (the rung can
            # change), so the composed crop can never name a plant this tile+rung cannot take — and ""
            # always remains valid, meaning "take the sim's default".
            func(rung: String, host: VBoxContainer) -> void:
                var crop_picker := _build_crop_picker(basket, rung, _compose.forage_species(),
                    committed_species if is_committed else "",
                    func(species: String) -> void:
                        _compose.set_forage_species(species)
                        _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target))
                if crop_picker != null:
                    host.add_child(crop_picker))
    # Range-aware: foraging is stationary gathering (there is NO forage-expedition alternative), so a
    # tile beyond the SELECTED band's work_range DISABLES the button + shows an out-of-range hint,
    # rather than a fallback. Distance is wrap-aware from the picked band's OWN tile — distance,
    # work_range, and the target band all key off `band` explicitly (never the faction's default band).
    var band_tile := SourceForecast.band_tile(band)
    var work_range := int(band.get("work_range", 0))
    var distance := SourceForecast.hex_distance_wrapped(
        band_tile.x, band_tile.y, x, y, _band_labor.grid_width(), _band_labor.wrap_horizontal())
    var out_of_range := distance >= 0 and distance > work_range
    if out_of_range:
        target.add_child(HudWidgets.alloc_hint_label(
            "(%d,%d) is %d tiles away — beyond this band's forage range (%d)." % [x, y, distance, work_range]))
    # A dead button is always explained (the `+` stepper's cap note is the precedent) — but only when
    # the cap note has not already said it, so the panel never states one fact twice.
    if is_noop and cap_note == "":
        target.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.FORAGE_NOOP_HINT))
    var assign_btn := Button.new()
    assign_btn.text = HudComposeVocab.UNASSIGN_BUTTON if is_unassign else HudComposeVocab.FORAGE_ASSIGN_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range or is_noop
    assign_btn.pressed.connect(func() -> void:
        # ORDER IS LOAD-BEARING: `assign_labor` first (it carries the crop), the improvement verb
        # second. The sim's improvement commands act on the bands ALREADY WORKING the tile, so a verb
        # sent to an unworked patch is rejected outright — the crew has to land first.
        _emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, _compose.forage_count(), x, y, "",
            _compose.forage_policy(), _compose.forage_species(), composed_improvement)
        _emit_improvement(band, SourceForecast.LABOR_KIND_FORAGE, composed_improvement,
            standing_improvement, x, y, "")
        close_compose_sheet())
    target.add_child(assign_btn)

# ---- THE COMPOSE SHEET: the drawer's read state + the floating write state --------------------
#
# docs/plan_tile_panel_layout.md §10-§15. The drawer keeps the detail rows, gains a one-line
# standing-assignment summary, and ends in `Assign … ▸`; the sheet (`ui/hud/ComposeSheet.gd`) hosts
# the compose block itself. NOTHING is re-derived here — the summary's rate comes from the same
# `SourceForecast.source_yield_readout` the Band panel's Current-actions rows use, and every gate, forecast and
# ceiling in the sheet comes from the same call it came from when the block lived in the drawer.

## Build the compose sheet once. Like the fork panel it is a child of the HUD CanvasLayer itself,
## NOT of `layout_root`: it floats over the whole window and must not inset with the reserved docks.
func _ensure_compose_sheet() -> void:
    if _compose_sheet != null:
        return
    _compose_sheet = ComposeSheet.new()
    _compose_sheet.name = "ComposeSheet"
    _compose_sheet.closed.connect(_on_compose_sheet_closed)
    _host.add_child(_compose_sheet)

## Is a compose sheet open? `Main._unhandled_input` asks this FIRST on Esc — the sheet is the
## innermost surface, so it claims the key ahead of targeting-cancel and the pause menu (§15).
func is_compose_sheet_open() -> bool:
    return _compose_sheet != null and _compose_sheet.is_open()

## Close any open sheet and return to the read state. Idempotent, so every close reason (commit, ✕,
## catcher click, Esc, selection change, targeting) can call it unconditionally.
func close_compose_sheet() -> void:
    if _compose_sheet != null:
        _compose_sheet.close()

## The sheet reports itself closed (including when WE closed it) — drop the compose state so the two
## can never disagree, and restore the drawer's read state so its button un-presses.
func _on_compose_sheet_closed() -> void:
    _compose.clear_composing()
    refresh_drawer_actions()

## The rect the sheet floats beside: the selection card, so the subject list + standing summary it
## is editing stay readable. A zero rect (card hidden) makes the sheet hug the viewport margin.
func _compose_anchor_rect() -> Rect2:
    if _tile_panel == null or not _tile_panel.visible:
        return Rect2()
    return _tile_panel.get_global_rect()

## Can this LAND offer a forage compose at all? The gate the drawer's button and the sheet share, so
## the button can never open an empty sheet. (A workable patch is live state — redacted on a
## remembered hex like its occupants — and there must be a player band to staff it.)
func _forage_compose_available(tile_info: Dictionary) -> bool:
    return String(tile_info.get("food_module", "")).strip_edges() != "" \
        and not _resolve_assign_band().is_empty() \
        and not _selectioncard.tile_contents_unseen(tile_info)

## Can this HERD offer a hunt/herding compose? Huntable, with a player band to staff it. (A penned
## herd's Extend-pen action is NOT a compose — it stays in the drawer, see `build_herd_drawer_actions`.)
func _herd_compose_available(herd: Dictionary) -> bool:
    return bool(herd.get("huntable", false)) and not _resolve_assign_band().is_empty()

## The stable key identifying a composed source, so a per-snapshot refresh can tell "the same
## source, restated" from "a different source" (§15: a snapshot must NOT close the sheet).
func _forage_source_key(tile_info: Dictionary) -> String:
    return "%d,%d" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]

## The LIVE herd dict for `herd_id`, or `fallback` when the selection has moved on.
##
## A `pressed`/rebuild closure must NEVER capture a herd dict. The drawer-actions same-shape patch
## path deliberately keeps a button's connection intact across snapshots, and the shape signature
## carries the herd's IDENTITY but none of its live state — so a captured dict is frozen at whatever
## turn the drawer was last fully rebuilt, and the sheet opens against a pre-tame herd quoting a
## stale `domestication` and `herders_needed_if_managed` (the "max 3 workers useful" / "0% tamed"
## playtest report, one turn behind the drawer beside it). Re-resolving through the selection model
## at CALL time is what makes the open path and the per-snapshot `refresh_compose_sheet` path read
## the SAME dict by construction. The id guard keeps a harness that stages a herd without touching
## `_selection` working off its own fixture.
func _live_herd(herd_id: String, fallback: Dictionary) -> Dictionary:
    var live := _selection.herd()
    if live.is_empty() or String(live.get("id", "")) != herd_id:
        return fallback
    return live

## The forage twin of `_live_herd` — same stale-capture rule, keyed on the tile's compose subject key.
func _live_tile_info(subject_key: String, fallback: Dictionary) -> Dictionary:
    var live := _selection.tile_info()
    if live.is_empty() or _forage_source_key(live) != subject_key:
        return fallback
    return live

## The crew noun the sheet's stepper uses for this herd — herders on a MANAGED (corralled/pastoral)
## herd, hunters on a wild one. Read by the drawer button too, so the two always agree.
##
## **It reads the IMPROVEMENT axis, never the stance** (issue #442). `is_managed_hunt_source`'s second
## argument is the composed improvement — its `== corral` clause is what makes a herd the player is
## penning read as managed before the pen exists — and handing it `hunt_policy()` made that clause
## permanently false (no stance is ever spelled `corral`), so the drawer button said "Assign hunters"
## while the sheet's own stepper beside it said "Herders".
func _herd_crew_noun(herd: Dictionary) -> String:
    return HudComposeVocab.HERD_CREW_LABEL \
        if SourceForecast.is_managed_hunt_source(herd, _compose.hunt_improvement()) \
        else HudComposeVocab.HUNT_CREW_LABEL

func open_forage_compose(tile_info: Dictionary) -> void:
    if not _forage_compose_available(tile_info):
        return
    _ensure_compose_sheet()
    _compose.set_composing(ComposeState.KIND_FORAGE, _forage_source_key(tile_info))
    var subject := String(tile_info.get("food_module_label", "")).strip_edges()
    if subject == "":
        subject = HudFormat.food_module_label(String(tile_info.get("food_module", "")))
    var content := _compose_sheet.open(
        HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudComposeVocab.FORAGE_CREW_LABEL.to_lower(),
        subject, _compose.subject(), _compose_anchor_rect())
    _build_forage_assign_controls(tile_info, content)
    refresh_drawer_actions()

func open_herd_compose(herd: Dictionary) -> void:
    if not _herd_compose_available(herd):
        return
    _ensure_compose_sheet()
    _compose.set_composing(ComposeState.KIND_HERD, String(herd.get("id", "")))
    var content := _compose_sheet.open(
        HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % _herd_crew_noun(herd).to_lower(),
        SourceForecast.herd_display_name(herd), _compose.subject(), _compose_anchor_rect())
    _build_herd_assign_controls(herd, content)
    refresh_drawer_actions()

## A snapshot arrived: re-render the OPEN sheet in place against the fresh subject. It must NOT
## close — `reapply_selection` runs every turn and closing would make the sheet unusable under
## autoplay (§15). It closes only when the subject it is composing is actually GONE (a different
## source is now selected, or the source stopped offering the compose at all).
func refresh_compose_sheet() -> void:
    if not is_compose_sheet_open():
        return
    match _compose.kind():
        ComposeState.KIND_FORAGE:
            if _forage_source_key(_selection.tile_info()) != _compose.subject() \
                    or not _forage_compose_available(_selection.tile_info()):
                close_compose_sheet()
                return
            _build_forage_assign_controls(_selection.tile_info(), _compose_sheet.content())
        ComposeState.KIND_HERD:
            if String(_selection.herd().get("id", "")) != _compose.subject() \
                    or not _herd_compose_available(_selection.herd()):
                close_compose_sheet()
                return
            _build_herd_assign_controls(_selection.herd(), _compose_sheet.content())
        _:
            close_compose_sheet()

## Re-render whichever subject's drawer actions are showing (the standing summary + the `Assign … ▸`
## button), so a turn's staffing change lands in the read state as well as in the open sheet.
func refresh_drawer_actions() -> void:
    if not _selection.herd().is_empty():
        build_herd_drawer_actions(_selection.herd())
    elif not _selection.tile_info().is_empty() and _selection.unit().is_empty():
        build_forage_drawer_actions(_selection.tile_info())

## The LAND drawer's read state: the standing forage summary (when the player already works this
## patch) and the `Assign foragers ▸` button that opens the sheet. Fills `%ForageAssignControls`,
## which is why that node keeps its name and its place in the drawer — the compose block MOVED out
## of it, the node did not move.
func build_forage_drawer_actions(tile_info: Dictionary) -> void:
    if _forage_assign_controls == null:
        return
    var available := _forage_compose_available(tile_info)
    _forage_assign_controls.visible = available
    if not available:
        _clear_forage_drawer()
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var standing := _standing_assignment(SourceForecast.LABOR_KIND_FORAGE, x, y, "")
    var summary_model: Dictionary = {}
    if not standing.is_empty():
        summary_model = _standing_summary_model(standing, SourceForecast.LABOR_KIND_FORAGE, HudComposeVocab.FORAGE_CREW_LABEL.to_lower())
    var subject_key := _forage_source_key(tile_info)
    # THE SIGNATURE CARRIES IDENTITY ONLY, AND THE CLOSURE CARRIES NOTHING. Two halves:
    #   • The subject key LEADS the shape signature, so switching to a different tile — even one of
    #     identical structure — forces a full rebuild rather than a positional patch onto another
    #     tile's nodes.
    #   • The compose-open button's `pressed` closure does NOT capture `tile_info`; it re-resolves the
    #     live tile through `_live_tile_info(subject_key, …)` when pressed. The same-shape patch path
    #     deliberately keeps that connection intact across per-snapshot restates, and this signature
    #     cannot carry the patch's live state (its `patch_*` forecast/progress fields move every turn
    #     without changing the drawer's STRUCTURE) — nor should it try, since that would rebuild the
    #     drawer on every tick and reintroduce the reflow flash the patch path exists to remove.
    var shape := [subject_key] + _standing_actions_shape(summary_model)
    var expected_children := (1 if not summary_model.is_empty() else 0) + 1
    # Same shape (summary present + its warn/note structure) → patch the summary + compose button in
    # place, so the per-snapshot restate never tears down the drawer (the "worst around Forage" flash).
    # The compose button's primary/ghost flip lands in place too.
    if shape == _forage_drawer_shape and _forage_assign_controls.get_child_count() == expected_children:
        var idx := 0
        if not summary_model.is_empty():
            _update_standing_summary(_forage_assign_controls.get_child(idx) as HFlowContainer, summary_model)
            idx += 1
        _update_compose_open_button(_forage_assign_controls.get_child(idx) as Button, HudComposeVocab.FORAGE_CREW_LABEL, subject_key)
        return
    _clear_forage_drawer()
    if not summary_model.is_empty():
        _forage_assign_controls.add_child(_build_standing_summary_from_model(summary_model))
    _forage_assign_controls.add_child(_build_compose_open_button(
        HudComposeVocab.FORAGE_CREW_LABEL, subject_key,
        func() -> void: open_forage_compose(_live_tile_info(subject_key, tile_info))))
    _forage_drawer_shape = shape

## Free the forage drawer-actions and forget its shape, so the next build always rebuilds.
func _clear_forage_drawer() -> void:
    if _forage_assign_controls == null:
        return
    for child in _forage_assign_controls.get_children():
        child.queue_free()
    _forage_drawer_shape = []

## The HERD drawer's read state: the Extend-pen action (a one-click standing action on a built pen —
## NOT a compose, so it stays here rather than hiding behind a sheet), the standing hunt summary, and
## the `Assign hunters ▸` / `Assign herders ▸` button.
func build_herd_drawer_actions(herd: Dictionary) -> void:
    if _herd_assign_controls == null:
        return
    var corralled := bool(herd.get("corralled", false))
    var available := _herd_compose_available(herd)
    # A penned herd always offers Extend-pen, even if it is no longer huntable — so the container
    # stays visible for a pen OR a composable herd.
    _herd_assign_controls.visible = available or corralled
    if not (available or corralled):
        _clear_herd_drawer()
        return
    var extending := corralled and float(herd.get("pen_extend_progress", 0.0)) > 0.0
    var herd_id := String(herd.get("id", ""))
    var noun := _herd_crew_noun(herd)
    var summary_model: Dictionary = {}
    if available:
        var standing := _standing_assignment(SourceForecast.LABOR_KIND_HUNT, -1, -1, herd_id)
        if not standing.is_empty():
            summary_model = _standing_summary_model(standing, SourceForecast.LABOR_KIND_HUNT, noun.to_lower())
    var shape := _herd_actions_shape(herd_id, corralled, extending, available, summary_model)
    var expected_children := (1 if corralled else 0) + (1 if not summary_model.is_empty() else 0) + (1 if available else 0)
    # Same shape (extend kind + summary structure + compose button presence) → patch each part in
    # place, so a per-snapshot restate never tears the herd drawer down.
    if shape == _herd_drawer_shape and _herd_assign_controls.get_child_count() == expected_children:
        var idx := 0
        if corralled:
            _update_extend_pen_control(_herd_assign_controls.get_child(idx), herd)
            idx += 1
        if not summary_model.is_empty():
            _update_standing_summary(_herd_assign_controls.get_child(idx) as HFlowContainer, summary_model)
            idx += 1
        if available:
            _update_compose_open_button(_herd_assign_controls.get_child(idx) as Button, noun, herd_id)
        return
    _clear_herd_drawer()
    if corralled:
        _build_extend_pen_control(herd, _herd_assign_controls)
    if not summary_model.is_empty():
        _herd_assign_controls.add_child(_build_standing_summary_from_model(summary_model))
    if available:
        _herd_assign_controls.add_child(_build_compose_open_button(
            noun, herd_id, func() -> void: open_herd_compose(_live_herd(herd_id, herd))))
    _herd_drawer_shape = shape

## Free the herd drawer-actions and forget its shape, so the next build always rebuilds.
func _clear_herd_drawer() -> void:
    if _herd_assign_controls == null:
        return
    for child in _herd_assign_controls.get_children():
        child.queue_free()
    _herd_drawer_shape = []

## The STANDING-SUMMARY child-slot structure shared by both drawers: `[has_summary, warn, has_note,
## has_muted]` — the full set of optional summary child slots, so any structural change (summary
## appearing/disappearing, a warn/note/muted label appearing) moves the signature and forces a rebuild
## rather than a stale positional patch. Each caller PREPENDS its own subject key (the forage tile key /
## the herd id) so a subject change also forces a rebuild — see `build_forage_drawer_actions` /
## `_herd_actions_shape`.
func _standing_actions_shape(summary_model: Dictionary) -> Array:
    if summary_model.is_empty():
        return [false, false, false, false]
    return [true, bool(summary_model["warn"]),
        String(summary_model["note"]) != "", String(summary_model["muted_note"]) != ""]

## The herd drawer-actions shape: the herd SUBJECT KEY + the extend control's kind + the summary
## structure + whether the compose button is present. The subject key LEADS so a subject change (even
## to a herd of identical structure) moves the signature and forces a full rebuild, rather than a
## positional patch onto another herd's nodes; any other structural change forces one too.
##
## THAT IS IDENTITY, AND IDENTITY IS ALL IT CARRIES. It deliberately holds none of the herd's LIVE
## state (`domestication`, `herders_needed`, `herders_needed_if_managed`, biomass, ecology), which
## moves every turn without changing the drawer's STRUCTURE — folding those in would rebuild the
## drawer on every tick and reintroduce the reflow flash the patch path exists to remove. So the
## compose-open button's `pressed` closure must not capture a herd dict either: it re-resolves the
## live herd through `_live_herd(herd_id, …)` when pressed, which is what stops the sheet quoting a
## pre-tame herd the turn taming starts (see `_live_herd`).
func _herd_actions_shape(herd_id: String, corralled: bool, extending: bool, available: bool, summary_model: Dictionary) -> Array:
    return [herd_id, corralled, extending, available] + _standing_actions_shape(summary_model)

## Patch an extend-pen control in place. It is a Fencing-N% BADGE while a ring is in flight, else a
## plain button; WHICH one rides the shape signature (`extending`), so here it is only ever the same
## kind — only the badge carries a live number to refresh.
func _update_extend_pen_control(node: Node, herd: Dictionary) -> void:
    var badge := node as Label
    if badge != null:
        badge.text = HudComposeVocab.PEN_FENCING_LABEL % int(round(float(herd.get("pen_extend_progress", 0.0)) * HudConst.PROGRESS_PERCENT_SCALE))

## Patch the `Assign … ▸` button in place: its noun (herders vs hunters can flip as a herd is tamed)
## and its primary/ghost lit-while-composing state, without freeing the button (whose `pressed`
## connection we keep intact).
func _update_compose_open_button(button: Button, noun: String, subject_key: String) -> void:
    if button == null:
        return
    button.text = HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % noun.to_lower()
    var composing := is_compose_sheet_open() and _compose.subject() == subject_key
    HudStyle.apply_button(button, "primary" if composing else "ghost")

## The `Assign … ▸` button. It lights "primary" (SIGNAL cyan — this HUD's LIVE state, as on the
## Sight chip and the selection accent) while ITS sheet is the open one, so the drawer shows which
## source is being composed rather than looking idle behind the sheet; "ghost" at rest. NOT "armed"
## — that is the destructive/warned treatment (DANGER border), and an open sheet is not a warning.
func _build_compose_open_button(noun: String, subject_key: String, on_press: Callable) -> Button:
    var button := Button.new()
    button.text = HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % noun.to_lower()
    var composing := is_compose_sheet_open() and _compose.subject() == subject_key
    HudStyle.apply_button(button, "primary" if composing else "ghost")
    button.pressed.connect(on_press)
    return button

## The player faction's standing assignment on a source, across every player band — `{}` when
## nobody works it. Scans `_band_labor.player_bands()` (the full player-faction list) and falls back to the
## single `_band_labor.player_band()` the one-band case (and the HUD-only preview harness) carries.
func _standing_assignment(kind: String, x: int, y: int, herd_id: String) -> Dictionary:
    var bands: Array = _band_labor.player_bands() if not _band_labor.player_bands().is_empty() else [_band_labor.player_band()]
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        var found := _band_labor.hunt_assignment_of(band, herd_id) if kind == SourceForecast.LABOR_KIND_HUNT \
            else _band_labor.forage_assignment_of(band, x, y)
        if not found.is_empty():
            return found
    return {}

## The drawer's one-line standing-assignment summary: `♻ 3 foragers · +2.74 /turn`, with the SAME
## warn/overdraw and overstaff/wasted flags the Band panel's Current-actions rows render, from the
## SAME `SourceForecast.source_yield_readout` call. The rate is never recomputed here.
## The standing-summary's display model — the values `_build_standing_summary_from_model` renders,
## computed ONCE so the drawer-actions shape signature and the in-place patch read one computation.
func _standing_summary_model(assignment: Dictionary, kind: String, noun: String) -> Dictionary:
    # `has_yield` is the ONE key `SourceForecast.source_yield_readout` reads that is not on the wire assignment —
    # it gates the rate on a CONFIRMED source (`_band_labor.effective_worker_map` sets it false for a
    # pending, yield-less optimistic assign). Everything else — actual/sustainable/realized,
    # `overdraws`, `workers_needed`, `wasted_yield` — is read straight off the assignment the sim sent.
    var m := assignment.duplicate()
    m["has_yield"] = assignment.has("actual_yield")
    var readout := SourceForecast.source_yield_readout(m, kind)
    var text := HudComposeVocab.STANDING_SUMMARY_FORMAT % [
        FoodIcons.for_policy(String(assignment.get("policy", ""))),
        int(assignment.get("workers", 0)),
        noun,
    ]
    var suffix := String(readout["label_suffix"])
    if suffix != "":
        text += HudComposeVocab.STANDING_SUMMARY_SEPARATOR + suffix
    return {
        "text": text.strip_edges(),
        "tooltip": String(readout["tooltip"]),
        "warn": bool(readout["warn"]),
        "note": String(readout["note"]),
        "muted_note": String(readout["muted_note"]),
    }

## Build the drawer's one-line standing-assignment summary (`♻ 3 foragers · +2.74 /turn`) from a
## precomputed model. Same warn/overdraw + overstaff/wasted flags a Band-panel Current-actions row
## renders, same three colours.
func _build_standing_summary_from_model(model: Dictionary) -> Control:
    var tooltip := String(model["tooltip"])
    var flow := HFlowContainer.new()
    flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    flow.add_theme_constant_override("h_separation", HudWorkVocab.STATUS_LINE_SEPARATION)
    if tooltip != "":
        flow.tooltip_text = tooltip
    flow.add_child(HudWidgets.build_status_part(String(model["text"]), HudStyle.INK))
    # ⚠ = ecological (the take outruns regrowth); the notes = labor (extra workers idle here / the
    # crew could not carry what the source offered). Same three parts, same three colours as a row.
    if bool(model["warn"]):
        flow.add_child(HudWidgets.build_row_note_label(HudComposeVocab.OVERHUNT_FLAG, HudStyle.WARN, tooltip))
    var note := String(model["note"])
    if note != "":
        flow.add_child(HudWidgets.build_row_note_label(note, HudStyle.WARN, tooltip))
    var muted_note := String(model["muted_note"])
    if muted_note != "":
        flow.add_child(HudWidgets.build_row_note_label(muted_note, HudStyle.INK_FAINT, tooltip))
    return flow

## Patch an existing standing-summary flow in place. Child 0 is the main status part; the optional
## warn/note/muted labels follow in that order and their PRESENCE is fixed by the shape signature, so
## positions are stable here (their text/colour is constant per position, only the value moves).
func _update_standing_summary(flow: HFlowContainer, model: Dictionary) -> void:
    if flow == null:
        return
    var tooltip := String(model["tooltip"])
    flow.tooltip_text = tooltip
    var idx := 0
    (flow.get_child(idx) as Label).text = String(model["text"])
    idx += 1
    if bool(model["warn"]):
        HudWidgets.set_label_tooltip(flow.get_child(idx) as Label, tooltip)  # OVERHUNT_FLAG face is constant
        idx += 1
    var note := String(model["note"])
    if note != "":
        var note_label := flow.get_child(idx) as Label
        note_label.text = note
        HudWidgets.set_label_tooltip(note_label, tooltip)
        idx += 1
    var muted_note := String(model["muted_note"])
    if muted_note != "":
        var muted_label := flow.get_child(idx) as Label
        muted_label.text = muted_note
        HudWidgets.set_label_tooltip(muted_label, tooltip)