class_name DrawerComposeController
extends RefCounted

## The DRAWER'S COMPOSE HALF (HUD decomposition Phase 2c-2b, docs/plan_hud_decomposition.md): the
## compose-sheet lifecycle, the two drawer-action builders that stand in front of it, the two big
## compose builders behind it (`_build_forage_assign_controls` / `_build_herd_assign_controls`), and
## the compose-only forecast / gate / crop-picker layer they rest on. It is the second half of the
## selection card — `SelectionCardController` took the identity/list half; the DRAWER RENDER DISPATCH
## (`_render_land_drawer` / `_render_occupant_drawer`) stays on `HudLayer` and calls IN here.
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
## The word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the
## same convention `HudWidgets` / `HudFormat` / `TopBarReadouts` / `SelectionCardController` follow —
## so a phrase is still typed in exactly one place.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# A hunting party was dispatched — relayed to HudLayer.send_hunt_expedition_requested.
signal send_hunt_expedition_requested(payload: Dictionary)
# Another ring was fenced around a pen — relayed to HudLayer.extend_pen_requested.
signal extend_pen_requested(payload: Dictionary)

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
        policy: String, species: String = "") -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, policy, species)

## The per-turn provisions `workers` from `band` take off `herd` under `policy` — the sim's LOCAL/band
## hunt take before the output multiplier: `min(workers × hunt_per_worker_provisions, band_ceiling)`.
## Resident-band only: an EXPEDITION's trip is never a rate division (see `SourceForecast.hunt_trip_forecast`).
## Returns `HUNT_RATE_UNAVAILABLE` when the levers/ceiling are absent.
func _hunt_take_rate(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> float:
    var per_worker_rate := float(band.get("hunt_per_worker_provisions", 0.0))
    var ceiling := SourceForecast.hunt_policy_ceiling(herd, policy)
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return SourceForecast.HUNT_RATE_UNAVAILABLE
    return maxf(minf(float(workers) * per_worker_rate, ceiling), 0.0)


## The averaging WINDOW (turns) for the whole-animal disclaimer — a STABLE, worker-independent property
## derived from the SELECTED policy's raw flow ceiling (NOT the crew's current delivered rate, which
## moves as workers change and made the old line blink out). Keyed on `policy` because a faster policy
## (Surplus/Market) delivers lumpy whole animals over a different span. `g` = animals/turn the policy's
## flow buys: slow/big game (`g < 1`) lands one animal every ~`1/g` turns; fast game (`g >= 1`) delivers
## the "extra" fractional animal every ~`1/frac` turns. Returns 0 when `food_per_animal` / the ceiling is
## unknown (caller then skips the line). NEVER scaled by `output_multiplier` — it's a pure herd property.
func _hunt_avg_window_turns(herd: Dictionary, policy: String) -> int:
    var fpa := float(herd.get("food_per_animal", 0.0))
    var ceiling := SourceForecast.hunt_policy_ceiling(herd, policy)
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
    var fpa := float(herd.get("food_per_animal", 0.0))
    var per_worker := float(band.get("hunt_per_worker_provisions", 0.0))
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var ceiling := SourceForecast.hunt_policy_ceiling(herd, policy)
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
    return {"available": true, "delivered": delivered, "waste": waste, "waste_pct": waste_pct}

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
##   EXTRACTIVE (Sustain/Surplus/Market/Eradicate) → the herd's worker-independent CAP for the policy
##       (`hunt_policy_ceilings`): a bare signed rate on the face, framed "up to X/turn" in the tooltip
##       — the ceiling it is, distinct from the crew's carry-aware delivered line below the picker. Read
##       straight off the sim; never re-derived.
##   INVESTMENT (Tame/Corral) → the PAYOFF the rung builds toward (`pastoral_yield` / `corral_yield`),
##       `→+Y` on the face / "builds toward Y/turn" in the tooltip. NOT the during-building dip: that dip
##       reads BELOW Sustain and is identical
##       for both rungs, so quoting it made taming/penning look strictly worse than hunting. Shown even
##       when the rung is gated/greyed — informative — with the gate-reason line explaining the lock; a
##       0/absent payoff leaves the button bare.
## Empty when the herd carries no ceilings (older snapshot / non-huntable).
func _hunt_policy_takes(herd: Dictionary) -> Dictionary:
    var takes := {}
    var ceilings_variant: Variant = herd.get(SourceForecast.HERD_BAND_CEILINGS_KEY, {})
    if not (ceilings_variant is Dictionary):
        return takes
    for policy in (ceilings_variant as Dictionary):
        # The INVESTMENT rungs are skipped here — their during-build dip rides this list too, but they
        # wear the PAYOFF (the second loop), not the dip. Mirrors `_forage_policy_takes`.
        if String(policy) in HudComposeVocab.INVESTMENT_POLICIES:
            continue
        var rate := float((ceilings_variant as Dictionary)[policy])
        if rate < 0.0:
            continue
        takes[String(policy)] = SourceForecast.extractive_take(rate)
    for policy in [HudConst.LABOR_POLICY_TAME, SourceForecast.LABOR_POLICY_CORRAL]:
        var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX, policy)
        if not bool(forecast["known"]) or not bool(forecast["investment"]):
            continue
        var payoff := float(forecast["payoff"])
        if payoff > 0.0:
            takes[policy] = _payoff_take(payoff)
    return takes


## A `{compact, full}` metric pair for an INVESTMENT rung's PAYOFF — the arrow-led rate on the button
## face (`→+1.20`), the "builds toward X/turn" wording in the tooltip. Shared by hunt + forage.
func _payoff_take(payoff: float) -> Dictionary:
    var signed := SourceForecast.format_signed(payoff)
    return {"compact": HudComposeVocab.POLICY_PAYOFF_COMPACT % signed, "full": HudComposeVocab.POLICY_PAYOFF_FULL_FORMAT % signed}

## The LOCAL hunt's live per-turn yield preview, or "" when the snapshot lacks the levers/ceilings
## (graceful degrade — no line, panel otherwise unchanged). A resident band applies its
## `output_multiplier` (morale/discontent productivity) at payout, so the preview is the take rate
## scaled by it. Reads income-green when the take is within the herd's sustainable yield (the Sustain
## ceiling), WARN-amber with the shared ⚠ when it overdraws — the same flag the allocation rows carry.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, policy: String, workers: int) -> String:
    var sustain_ceiling := SourceForecast.hunt_policy_ceiling(herd, SourceForecast.DEFAULT_HUNT_POLICY)
    if sustain_ceiling < 0.0:
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var sustainable := sustain_ceiling * output
    var dw := _hunt_delivered_and_waste(band, herd, policy, workers)
    if not bool(dw.get("available", false)):
        # Graceful degrade — `food_per_animal` (or a lever) is unknown, so fall back to the old smoothed
        # food/turn line unchanged rather than regress the readout.
        var rate := _hunt_take_rate(band, herd, policy, workers)
        if rate < 0.0:
            return ""
        var actual := rate * output
        var text: String = HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % SourceForecast.format_yield(actual)
        if _is_overdraw(actual, sustainable):
            return "[color=#%s]%s %s%s[/color]" % [
                HudStyle.WARN_HEX, HudComposeVocab.OVERHUNT_FLAG, text, HudComposeVocab.LOCAL_HUNT_OVERDRAW_SUFFIX]
        return "[color=#%s]%s%s[/color]" % [HudStyle.HEALTHY_HEX, text, SourceForecast.YIELD_TOOLTIP_RENEWABLE]
    # ANIMALS-FIRST: the crew's honest carry-aware delivered take, as a per-turn animal rate (one
    # consistent format — no fast/slow flip). `delivered` is already carry-quantized, so this credits no
    # throughput the crew can't haul home.
    var fpa := float(herd.get("food_per_animal", 0.0))
    var delivered := float(dw["delivered"])
    var animal_rate := delivered / fpa if fpa > 0.0 else 0.0
    var primary := HudComposeVocab.HUNT_DELIVERED_FORMAT % [_format_animal_rate(animal_rate), SourceForecast.herd_display_name(herd)]
    # Overdraw and waste are DIFFERENT flags and may co-occur — render both. Overdraw = the delivered take
    # exceeds the herd's Sustain ceiling (Surplus/Market draw it down); waste = a kill the crew couldn't
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
## Forage is SMOOTH food (no whole-animal rhythm — no lumpy carry, no waste), so the line is just the
## per-turn take + a sustainability verdict: income-green `+2.74 /turn · renewable` when the take is
## within the patch's Sustain ceiling, WARN-amber `⚠ … — overdraws the patch` when a Surplus/Market/
## Eradicate policy draws it down. Both scaled by the acting band's output multiplier, like the hunt
## line. "" (no line) when the forecast levers are unknown, so the panel degrades gracefully.
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
    var sustainable := float(sustain["ceiling"]) * output
    var actual := SourceForecast.expected_yield(forecast, workers, band)
    var text := SourceForecast.format_yield(actual)
    if _is_overdraw(actual, sustainable):
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
func _forecast_worker_cap(forecast: Dictionary, assignable: int, useful_floor: int = 0) -> Dictionary:
    var useful := SourceForecast.max_useful_workers(forecast)
    # A managed herd's maintenance crew raises the usefulness ceiling above what the take/prepare side
    # reports: a Corral rung's prep forecast says "1 worker suffices to prepare", but a growing pen needs
    # `herders_needed` hands EVERY turn to hold its tameness. Fold that floor in (callers pass it via
    # `useful_floor`) so the player can always staff the herders the herd requires. An UNBOUNDED forecast
    # stays unbounded — the floor is a RAISE, never a new cap — and a wild herd passes 0, so it's a no-op.
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

## The live INVESTMENT-rung forecast row on the assign controls — it states the DEAL: "Preparing:
## +0.09 /turn → then +1.20 /turn", so the up-front dip AND the payoff are visible BEFORE the player
## commits. Both halves scaled by the acting band's output multiplier. This row is INVESTMENT-only now:
## every extractive rung (hunt AND forage) renders its own bare-rate + verdict preview
## (`_local_hunt_preview_bbcode` / `_local_forage_preview_bbcode`) instead, so the old non-investment
## "Expected yield:" branch was unreachable and was removed. Callers gate on the investment rung.
##
## The Corral payoff is GROSS (the pen's feed is a separate debit on the keeper's larder), so its row
## never shows the payoff bare — it subtracts the herd's own exported `pen_upkeep` (which the sim now
## projects for an un-penned herd too, on the same biomass basis). The feed is NEVER folded away, and
## a **zero payoff is rendered, loudly** (see INVESTMENT_FORECAST_DEPLETED_NOTE) — a depleted herd
## below the escapement point pays nothing, and that is the row's most important reading.
func _forecast_yield_row(forecast: Dictionary, workers: int, band: Dictionary,
        crew_label: String = HudComposeVocab.FORAGE_CREW_LABEL) -> Label:
    var row := Label.new()
    var expected := SourceForecast.format_yield(SourceForecast.expected_yield(forecast, workers, band))
    var hex := HudStyle.HEALTHY
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var payoff := float(forecast.get("payoff", 0.0)) * output
    var feed := float(forecast.get("feed", 0.0)) * output
    var has_feed := bool(forecast.get("feed_rung", false)) and feed >= SourceForecast.FOOD_FLOW_MIN
    # UNSTAFFED: state the payoff as a condition, never as a sequence already under way — see
    # INVESTMENT_FORECAST_UNSTAFFED_FORMAT. The depleted-payoff note below still applies either way.
    var crew := crew_label.to_lower()
    if workers <= 0:
        if has_feed:
            row.text = HudComposeVocab.INVESTMENT_FORECAST_UNSTAFFED_FEED_FORMAT % [
                crew, SourceForecast.format_yield(payoff), SourceForecast.format_magnitude(feed)]
        else:
            row.text = HudComposeVocab.INVESTMENT_FORECAST_UNSTAFFED_FORMAT % [crew, SourceForecast.format_yield(payoff)]
    elif has_feed:
        row.text = HudComposeVocab.INVESTMENT_FORECAST_FEED_FORMAT % [
            expected, SourceForecast.format_yield(payoff), SourceForecast.format_magnitude(feed)]
    else:
        row.text = HudComposeVocab.INVESTMENT_FORECAST_FORMAT % [expected, SourceForecast.format_yield(payoff)]
    # A prepared source that pays NOTHING is a trap, and one that pays nothing while EATING every
    # turn is a net loss. Say so — amber, in words, without hiding the zeros that prove it.
    if has_feed and payoff < SourceForecast.FOOD_FLOW_MIN:
        row.text += "\n%s" % HudComposeVocab.INVESTMENT_FORECAST_DEPLETED_NOTE
        hex = HudStyle.WARN
    row.add_theme_color_override("font_color", hex)
    return row

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
    if source_changed:
        var staffed := _band_labor.workers_for_hunt(band, herd_id)
        _compose.seed_hunt(staffed if staffed > 0 else HudConst.WORKER_STEP, _band_labor.policy_for_hunt(band, herd_id))
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
        _build_herd_assign_controls(herd, target)))
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
    # Policy options: the Corral INVESTMENT rung is offered on a LOCAL hunt only — a detached party
    # follows the herd and hauls food home; it builds no pen. An expedition keeps the extractive four.
    var hunt_options: Array = SourceForecast.LABOR_HUNT_POLICIES if is_expedition else HudBandLaborState.HUNT_POLICY_OPTIONS
    # Grazing 2d-δ + the ladder's rung-2 verb: BOTH husbandry rungs are husbandry-ceiling affordances,
    # and the ceiling says how far up the ladder THIS SPECIES can climb ("wild" hunt-only / "pastoral"
    # tameable-but-never-pennable / "pen" the full ladder). An out-of-ceiling rung is HIDDEN OUTRIGHT,
    # never greyed: greying it would imply a reachable prerequisite, and no amount of knowledge or
    # work will ever let you pen an aurochs whose ceiling is "pastoral". Knowledge = "I know how";
    # ceiling = "this animal allows it" — decoupled (§4.2), so the gates above are orthogonal to this.
    #   • Corral needs a "pen" ceiling.
    #   • Tame needs anything ABOVE "wild" — and is pointless once the herd is fully tamed, so it
    #     retires from the picker at that point (its per-source meter is full; Corral is what's next).
    # `.filter` copies, so the HUNT_POLICY_OPTIONS const is untouched.
    if not is_expedition:
        var ceiling := SourceForecast.husbandry_ceiling(herd)
        if ceiling != SourceForecast.HUSBANDRY_CEILING_PEN:
            hunt_options = hunt_options.filter(func(policy: String) -> bool: return policy != SourceForecast.LABOR_POLICY_CORRAL)
        if ceiling == SourceForecast.HUSBANDRY_CEILING_WILD \
                or float(herd.get("domestication", 0.0)) >= SourceForecast.DOMESTICATION_COMPLETE:
            hunt_options = hunt_options.filter(func(policy: String) -> bool: return policy != HudConst.LABOR_POLICY_TAME)
    var hunt_gates := {} if is_expedition else _hunt_policy_gates(herd)
    # A gated rung can never be the composed policy (the herd may still be taming under a standing
    # Corral selection), so re-validate every render — not just when the selected herd changes.
    if not (_compose.hunt_policy() in hunt_options) \
            or not HudWidgets.gate_reasons(hunt_gates, _compose.hunt_policy()).is_empty():
        _compose.set_hunt_policy(SourceForecast.DEFAULT_HUNT_POLICY)
    # Pre-commit forecast — LOCAL hunt only. An expedition travels for several turns and accumulates
    # toward a carry cap, so the herd's per-turn take ceiling is NOT the bound on its party size;
    # forecasting a per-turn yield for it would be a lie. On a local hunt the ceiling caps the
    # stepper (no over-assigning) and drives the live expected-yield row; both recompute here on
    # every stepper/policy change, since both re-render these controls.
    var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_policy())
    # ONE yield row per rung — each rung gets the row that actually informs ITS decision:
    #   INVESTMENT (Corral) → `_forecast_yield_row` states the DEAL ("Preparing: +0.23 → then +1.05"):
    #       what you give up, for how long, to get what. That IS the Corral decision, and the local
    #       preview below structurally cannot express it (a dip/payoff pair is not a single rate).
    #       Corral draws sustainably by design, so no overdraw verdict is lost by using this row.
    #   EXTRACTIVE (the four) → `_local_hunt_preview_bbcode` below, which carries the same per-turn
    #       number PLUS the sustainability verdict (`· renewable` / `⚠ overdraws the herd`).
    # Rendering both was the merge's mistake: the two paths are independently computed but agree
    # numerically (verified — the flat `per_worker_yield`/`ceiling_*` scalars and the
    # `hunt_policy_ceilings` list are two views of ONE sim hunt model, both yielding +0.54 on a Market
    # take), so the second row added no information and, worse, argued with the first — a HEALTHY-green
    # "Expected yield" sitting directly above a WARN-amber "⚠ overdraws the herd" for the same number.
    var forecast_active := not is_expedition and bool(forecast["known"]) \
        and bool(forecast["investment"])
    # The party stepper caps at the max-useful count on BOTH branches — a raid's haul (`animals_taken`)
    # PLATEAUS with party size once the herd's surplus binds, so extra hunters past the plateau raid no
    # more animals and should be flagged idle exactly as an over-staffed local hunt is (the silent-idle-
    # hunter gap this pass closes). The local branch caps at the source's max-useful ceiling.
    # A managed (corralling/pastoral) herd needs `herders_needed` hands every turn to hold its tameness,
    # but the take/prepare max-useful ignores that — a Corral rung's prep says "1 worker useful", pinning
    # the player at 1 even when a growing herd needs 2 herders (an unwinnable trap: the corral slips and is
    # lost). Fold the herding crew into the LOCAL-hunt cap's usefulness ceiling so the maintenance crew is
    # always staffable. `herders_needed == 0` on a wild herd, so max(take-useful, 0) is a no-op there. The
    # expedition party has no herding crew, so `SourceForecast.expedition_useful_cap` is left alone.
    var herd_floor := int(herd.get("herders_needed", 0))
    var capped := SourceForecast.expedition_useful_cap(band, herd, _compose.hunt_policy(), assignable) if is_expedition \
        else _forecast_worker_cap(forecast, assignable, herd_floor)
    var cap := int(capped["cap"])
    # Auto-max on policy select — "give me everything this herd sustains": the max-useful for the policy
    # (clamped to idle below), which guarantees zero waste + the full rate. Only ever set by a policy
    # click (both branches), never by a −/+ tick, so manual counts survive the rebuild.
    if _compose.consume_hunt_autofill():
        _compose.set_hunt_count(cap)
    _compose.clamp_hunt_count(cap)
    # A managed herd's local crew are HERDERS/keepers (workersNeeded scales with the herd), not a hunt
    # party — so a pen needing several keepers doesn't read as a hunt-party bug (fix #6).
    var crew_label := HudComposeVocab.HERD_CREW_LABEL if SourceForecast.is_managed_hunt_source(herd, _compose.hunt_policy()) \
        else HudComposeVocab.HUNT_CREW_LABEL
    target.add_child(HudWidgets.build_worker_stepper(
        "Party" if is_expedition else crew_label, _compose.hunt_count(), _compose.hunt_count() < cap,
        func(n: int) -> void:
            _compose.set_hunt_count(clampi(n, 0, cap))
            _build_herd_assign_controls(herd, target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # Ascending per-policy takes under BOTH pickers so all three (forage / local hunt / expedition) wear
    # the same "up to X/turn" button metric: each policy's MAX obtainable food/turn (Sustain < Surplus <
    # Market < Eradicate). Worker-independent on both branches (the expedition's is the max over party
    # sizes of delivered_food / trip_turns, so it never changes as the Party stepper steps).
    var policy_takes := SourceForecast.expedition_policy_takes(band, herd, _band_labor.grid_width(), _band_labor.wrap_horizontal()) if is_expedition \
        else _hunt_policy_takes(herd)
    target.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
        _compose.set_hunt_policy(policy)
        # Picking a policy auto-fills the crew to that policy's max-useful (consumed next rebuild).
        _compose.arm_hunt_autofill()
        _build_herd_assign_controls(herd, target), _compose.hunt_policy(), hunt_options, hunt_gates, policy_takes))
    # The policy hint is rendered per BRANCH below, never here: a resident band and a detached party
    # earn DIFFERENT payoffs from the same policy word (the band tames the herd and trades the take;
    # an expedition's Hunting arm credits food only), so one shared hint line under the picker would
    # promise the expedition player a payoff the sim never pays.
    if forecast_active:
        target.add_child(
            _forecast_yield_row(forecast, _compose.hunt_count(), band, crew_label))
    if is_expedition:
        target.add_child(HudWidgets.alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
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
        # What this policy DOES for a resident band (the forecast line below carries the number; this
        # carries the consequence — above all what Sustain actually teaches, which is otherwise
        # invisible). Deliberately NOT the expedition hints: a party earns neither.
        target.add_child(HudWidgets.alloc_hint_label(
            String(HudComposeVocab.LOCAL_HUNT_POLICY_HINTS.get(_compose.hunt_policy(), ""))))
        # Averaging-window disclaimer — the delivered rate above is a long-run average of lumpy
        # whole-animal delivery (you take WHOLE animals, so per-turn delivery varies). ALWAYS shown on
        # an extractive rung (an investment rung shows a dip→payoff, not an animal cadence, so it's
        # skipped), as a STABLE herd-level statement: the span is keyed off the selected policy's flow
        # ceiling (`_hunt_avg_window_turns`), so it never moves as the Hunters count steps up and never
        # blinks out. Skipped only when the window is unknown (missing food_per_animal / ceiling).
        if not (_compose.hunt_policy() in HudComposeVocab.INVESTMENT_POLICIES):
            var window_turns := _hunt_avg_window_turns(herd, _compose.hunt_policy())
            if window_turns > 0:
                target.add_child(HudWidgets.alloc_hint_label(
                    HudComposeVocab.HUNT_AVG_WINDOW_FORMAT % window_turns))
        # "Why isn't my Tame progressing?" — the ONE silent rule left on this rung, surfaced rather
        # than left to be guessed. See `_tame_stalled_hint`.
        var stalled := _tame_stalled_hint(herd)
        if stalled != "":
            var stalled_label := HudWidgets.alloc_hint_label(stalled)
            stalled_label.add_theme_color_override("font_color", HudStyle.WARN)
            target.add_child(stalled_label)
        # LIVE per-turn yield for the standing assignment being composed (no carry cap on a local
        # hunt, so turns-to-fill is meaningless — food/turn is the number that decides it).
        # EXTRACTIVE rungs ONLY — an INVESTMENT rung is answered by the dip→payoff row above
        # (`forecast_active`) or by Tame's row, and rendering both put two rows with the same number
        # on the panel. See the ONE-yield-row-per-rung note there. Tested against the named rung set,
        # NOT `forecast["investment"]` (which is really "has a payoff key" and so misses Tame).
        if not (_compose.hunt_policy() in HudComposeVocab.INVESTMENT_POLICIES):
            var yield_line := _local_hunt_preview_bbcode(
                band, herd, _compose.hunt_policy(), _compose.hunt_count())
            if yield_line != "":
                target.add_child(HudWidgets.forecast_label(yield_line))
        assign_btn.text = HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON
        HudStyle.apply_button(assign_btn, "primary")
    if is_expedition:
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
                "band": int(band.get("entity", -1)),
                "party_workers": _compose.hunt_count(),
                "fauna_id": herd_id,
                "fauna_label": SourceForecast.herd_display_name(herd),
                "policy": _compose.hunt_policy() if _compose.hunt_policy() in SourceForecast.LABOR_HUNT_POLICIES else SourceForecast.DEFAULT_HUNT_POLICY,
            })
            # Committing is the end of the compose act — return to the read state (§15).
            close_compose_sheet())
    else:
        assign_btn.pressed.connect(func() -> void:
            _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, _compose.hunt_count(),
                herd_x, herd_y, herd_id, _compose.hunt_policy())
            close_compose_sheet())
    target.add_child(assign_btn)







## Each extractive policy's per-turn take on this forage patch — the policy ceiling from the shared
## `SourceForecast.forecast_inputs` (food/turn at output 1.0, like the hunt band ceiling), for the FORAGE picker's
## ascending per-policy readout. The plant twin of `_hunt_policy_takes`, so all three pickers wear the
## same "+X /turn" button metric. Empty entries (dead-season patch / older snapshot) are skipped.
func _forage_policy_takes(tile_info: Dictionary) -> Dictionary:
    var takes := {}
    for policy in SourceForecast.LABOR_HUNT_POLICIES:
        var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, String(policy))
        if not bool(forecast["known"]):
            continue
        takes[String(policy)] = SourceForecast.extractive_take(float(forecast["ceiling"]))
    # The two forage INVESTMENT rungs wear the PAYOFF they build toward, not a per-turn take (the prep
    # dip is lower than Sustain and would make Cultivate look strictly worse than idling). A locked rung
    # may still show its payoff — informative ("this is what it'd give"), and the gate-reason line under
    # the picker already explains the lock. Absent/zero payoff → no entry, so the button stays bare.
    for policy in [HudConst.LABOR_POLICY_CULTIVATE, HudConst.LABOR_POLICY_SOW]:
        var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, policy)
        if not bool(forecast["known"]) or not bool(forecast["investment"]):
            continue
        var payoff := float(forecast["payoff"])
        if payoff > 0.0:
            takes[policy] = _payoff_take(payoff)
    return takes

## Unmet prerequisites for the FORAGE investment rungs (Cultivate = rung 2, Sow = rung 3), keyed
## policy → Array[String] of reasons (each already carrying its own remedy). Empty when every rung is
## available. Mirrors the sim's `assign_labor` validation.
##
## The two rungs gate on DIFFERENT things, which is the ladder made legible:
##   • Cultivate — Cultivation knowledge + a Thriving patch (you improve what is already there).
##   • Sow — Seed Selection knowledge + ground that will take seed. It needs NO prior patch and no
##     Thriving gate (seed travels, and sown ground starts at the reseed floor — i.e. Collapsing — so
##     a health gate would forbid the very case the rung exists for). What it needs instead is the
##     LAND: `patch_sow_site_refusal` is the sim's verdict on this ground, and it is the only gate
##     reason on either web that the player answers by MOVING rather than by working.
func _forage_policy_gates(tile_info: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(SourceForecast.LABOR_POLICY_SUSTAIN)
    var gates := {}
    var cultivate_reasons: Array[String] = []
    var cultivation := _topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID, HudFloraVocab.KNOWLEDGE_TRACK_CULTIVATION)
    if cultivation < HudConst.KNOWLEDGE_COMPLETE:
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(cultivation), sustain_icon])
    var phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if phase != HudFloraVocab.ECOLOGY_PHASE_THRIVING:
        var phase_label := phase.capitalize() if phase != "" else HudFloraVocab.GATE_PHASE_UNKNOWN_LABEL
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_PATCH_THRIVING_FORMAT % phase_label)
    # A finished patch retires Cultivate outright: the build is DONE (Sustain harvests it, and Sow is the
    # next rung if unlocked). This SUPERSEDES the prep prerequisites — a tended patch's Thriving/knowledge
    # gates are moot — so it replaces the reason list rather than piling on.
    if bool(tile_info.get("is_cultivated", false)):
        cultivate_reasons.clear()
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_ALREADY_TENDED_FORMAT % sustain_icon)
    if not cultivate_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_CULTIVATE] = cultivate_reasons
    var sow_reasons: Array[String] = []
    var seed_selection := _topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID, HudFloraVocab.KNOWLEDGE_TRACK_SEED_SELECTION)
    if seed_selection < HudConst.KNOWLEDGE_COMPLETE:
        sow_reasons.append(HudFloraVocab.GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(seed_selection), sustain_icon])
    var refusal := _sow_site_refusal_reason(tile_info)
    if refusal != "":
        sow_reasons.append(refusal)
    # A finished Field retires Sow, same as a finished patch retires Cultivate.
    if bool(tile_info.get("patch_is_field", false)):
        sow_reasons.clear()
        sow_reasons.append(HudFloraVocab.GATE_REASON_ALREADY_FIELD_FORMAT % sustain_icon)
    if not sow_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_SOW] = sow_reasons
    return gates

## WHY this ground will not take seed, in the manual's voice — "" when it will. Reads the sim's
## `patch_sow_site_refusal` verdict; the client never re-derives it (it has neither the per-biome
## capacity table nor the hydrology). An unknown key still refuses: the sim gates the command on the
## same seam, so offering the button anyway would only produce a failure the player cannot read.
func _sow_site_refusal_reason(tile_info: Dictionary) -> String:
    var key := String(tile_info.get("patch_sow_site_refusal", "")).strip_edges()
    if key == "":
        return ""
    return String(HudFloraVocab.SOW_REFUSAL_REASONS.get(key, HudFloraVocab.SOW_REFUSAL_FALLBACK))

## Unmet prerequisites for the HUNT investment rungs (Tame = rung 2, Corral = rung 3), keyed policy →
## Array[String] of reasons. The herd twin of `_forage_policy_gates`.
##
## The §4.3 GATE RESHUFFLE is what this function encodes: ONE knowledge per transition. **Herding
## gates Tame** (it no longer gates Corral, and taming is no longer ungated), and the **new Penning
## gates Corral**. Corral additionally needs THIS herd tamed — the per-source half of the split.
##
## Deliberately NOT gated: the herd being Thriving. Taming a herd whose phase swings under hunting
## would be un-actionable, so the sim just PAUSES the meter instead — see `_tame_stalled_hint`, which
## is how the player is told rather than left to guess.
##
## Known gap (pre-existing): no ownership check — the sim's tracks are per-faction, so a herd tamed by
## ANOTHER faction reads as available here while the sim rejects the assign.
func _hunt_policy_gates(herd: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(SourceForecast.LABOR_POLICY_SUSTAIN)
    var gates := {}
    var domestication := float(herd.get("domestication", 0.0))
    var tame_reasons: Array[String] = []
    var herding := _topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID, HudFloraVocab.KNOWLEDGE_TRACK_HERDING)
    if herding < HudConst.KNOWLEDGE_COMPLETE:
        tame_reasons.append(HudFloraVocab.GATE_REASON_HERDING_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(herding), sustain_icon])
    if not tame_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_TAME] = tame_reasons
    var corral_reasons: Array[String] = []
    var penning := _topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID, HudFloraVocab.KNOWLEDGE_TRACK_PENNING)
    if penning < HudConst.KNOWLEDGE_COMPLETE:
        corral_reasons.append(HudFloraVocab.GATE_REASON_PENNING_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(penning), sustain_icon])
    if domestication < SourceForecast.DOMESTICATION_COMPLETE:
        corral_reasons.append(HudFloraVocab.GATE_REASON_HERD_DOMESTICATED_FORMAT % [
            HudFormat.progress_percent(domestication), FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME)])
    if not corral_reasons.is_empty():
        gates[SourceForecast.LABOR_POLICY_CORRAL] = corral_reasons
    return gates

## The one silent rule left on the Tame rung, said out loud. Taming accrues only while the herd is
## **Thriving**, but that is deliberately NOT a gate on selecting Tame (`_hunt_policy_gates`): a
## herd's phase swings as it is hunted, so refusing the verb would be un-actionable churn. The sim
## instead just PAUSES the meter — progress is neither lost nor switched — and resumes when the herd
## recovers.
##
## Saying nothing here would recreate the exact failure this whole arc exists to kill: a hidden rule
## the player can only learn by being told. So whenever Tame is the composed policy on a herd that
## is not Thriving, state the pause, name the cause (its live phase), and name the remedy — which is
## the opposite of "work harder" (ease off and let it recover), the same shape as the patch-ecology
## gate's advice. Returns "" when Tame is not selected or the herd is Thriving (nothing to explain).
func _tame_stalled_hint(herd: Dictionary) -> String:
    if _compose.hunt_policy() != HudConst.LABOR_POLICY_TAME:
        return ""
    var phase := String(herd.get("ecology_phase", "")).strip_edges().to_lower()
    if phase == "" or phase == HudFloraVocab.ECOLOGY_PHASE_THRIVING:
        return ""
    return HudComposeVocab.TAME_STALLED_HINT_FORMAT % phase.capitalize()

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

## The FODDER (hay) this entry would pay per turn as a sown field — >0 marks a fodder crop, whose
## provisions ratio reads 0. Routed to the fodder account, so the picker shows it in place of the 0×
## ratio. `FLORA_CROP_RATIO_NONE` (0) for a normal provisions crop. Fodder is a Field payoff only.
func _flora_entry_fodder_payoff(entry: Dictionary) -> float:
    return float(entry.get("sow_fodder_payoff", SourceForecast.FLORA_CROP_RATIO_NONE))

## Provisions/turn this rung pays once complete, committed to THIS species — the sim's own number, in
## the same units and output-multiplier convention as the forecast `payoff` it replaces. 0 (never
## substituted) on a rung the species cannot climb.
func _flora_entry_payoff(entry: Dictionary, policy: String) -> float:
    if policy == HudConst.LABOR_POLICY_SOW:
        return float(entry.get("sow_payoff", 0.0))
    return float(entry.get("cultivate_payoff", 0.0))

## The forecast, with its species-BLIND payoff replaced by the selected crop's own. Without this the
## "→ then" term quotes one number no matter which crop is picked, so the picker appears to change
## nothing above it — the player commits to Reeds and is shown Wild Emmer's payoff. A SUBSTITUTION,
## not a calculation: the client does no arithmetic on the sim's figure. Returns the forecast untouched
## when nothing is committed (no selection, a non-committing rung, or a species with no payoff on it).
func _forecast_for_selected_crop(forecast: Dictionary, entries: Array[Dictionary], policy: String,
        species: String) -> Dictionary:
    if species == "" or not (policy in HudFloraVocab.FLORA_COMMITTING_POLICIES):
        return forecast
    for entry in entries:
        if String(entry["species"]) != species:
            continue
        var payoff := _flora_entry_payoff(entry, policy)
        if payoff <= 0.0:
            return forecast
        var adjusted := forecast.duplicate()
        adjusted["payoff"] = payoff
        return adjusted
    return forecast

## The crop this compose will SEND: the player's pick while it is still legal on this tile+rung, else
## the HIGHEST-SHARE legal entry — which is the sim's own `default_species_for_rung`, so picking
## nothing and accepting the default behave identically. Returns "" (send nothing, still valid) for a
## non-committing rung, an already-committed patch, or a basket with no legal plant.
func _resolve_crop_selection(entries: Array[Dictionary], policy: String, committed: bool, picked: String) -> String:
    if committed or not (policy in HudFloraVocab.FLORA_COMMITTING_POLICIES):
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
## legal-but-marginal one is fully pressable. A patch that has already committed gets a locked
## READOUT instead, since the commitment is one-way until it lapses. Returns null when there is
## nothing to render (a biome that carries no named forage), so no empty block appears.
func _build_crop_picker(
    entries: Array[Dictionary],
    policy: String,
    selected: String,
    committed_name: String,
    on_pick: Callable) -> Control:
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudFloraVocab.FLORA_CROP_BLOCK_SEPARATION)
    if committed_name != "":
        block.add_child(HudWidgets.alloc_section_label(HudFloraVocab.FLORA_CROP_COMMITTED_HEADER))
        var committed_label := Label.new()
        committed_label.text = committed_name
        committed_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
        block.add_child(committed_label)
        block.add_child(HudWidgets.alloc_hint_label(HudFloraVocab.FLORA_CROP_COMMITTED_HINT))
        return block
    if entries.is_empty():
        return null
    block.add_child(HudWidgets.alloc_section_label(HudFloraVocab.FLORA_CROP_PICKER_HEADER))
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
        # A fodder crop pays hay, not provisions: its ratio is 0, so its face states the hay value in
        # its own account instead of a worthless-looking "0.0×".
        var fodder_payoff := _flora_entry_fodder_payoff(entry)
        var is_fodder := fodder_payoff > SourceForecast.FLORA_CROP_RATIO_NONE
        var btn := Button.new()
        # The payoff rides the face ONLY where there is one: a fodder crop shows its hay value, a
        # provisions crop its ratio, and a row greyed by the climbability flags carries the 0 sentinel
        # (printing "0.0×" there would read as "a crop worth nothing" rather than "not a crop at this rung").
        if is_fodder:
            btn.text = HudFloraVocab.FLORA_CROP_FODDER_ROW_FORMAT % [crop_name, percent, fodder_payoff]
        elif ratio > SourceForecast.FLORA_CROP_RATIO_NONE:
            btn.text = HudFloraVocab.FLORA_CROP_ROW_FORMAT % [crop_name, percent, ratio]
        else:
            btn.text = HudFloraVocab.FLORA_SHARE_FORMAT % [crop_name, percent]
        btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        HudStyle.apply_button(btn, "primary" if legal and species == selected else "ghost")
        # A row must be EXACTLY `FLORA_CROP_ROW_HEIGHT` — the list's cap is derived from it, so a row
        # wearing the default button chrome would silently break that maths (the work board's rule).
        HudWidgets.compact(btn, HudFloraVocab.FLORA_CROP_ROW_FONT_SIZE, HudFloraVocab.FLORA_CROP_ROW_PADDING_V)
        btn.custom_minimum_size = Vector2(0.0, HudFloraVocab.FLORA_CROP_ROW_HEIGHT)
        btn.disabled = not legal
        if legal:
            any_legal = true
            # A fodder crop is valuable in the FODDER account, not the provisions one, so it never
            # takes the loss-warn ink its 0 provisions ratio would otherwise earn: its tooltip names
            # the hay it pays instead.
            if is_fodder:
                btn.tooltip_text = HudFloraVocab.FLORA_CROP_FODDER_TOOLTIP_FORMAT % [crop_name, fodder_payoff]
            # A LOSS-MAKING but legal crop: warn ink, FULLY pressable. Never hidden, clamped, sorted
            # by, or disabled — the ratio is there to stop a bad idea being invisible, not to forbid it.
            elif ratio > SourceForecast.FLORA_CROP_RATIO_NONE and ratio < HudFloraVocab.FLORA_CROP_BREAK_EVEN_RATIO:
                btn.add_theme_color_override("font_color", HudStyle.WARN)
                btn.add_theme_color_override("font_hover_color", HudStyle.WARN)
                btn.tooltip_text = HudFloraVocab.FLORA_CROP_LOSS_TOOLTIP_FORMAT % [crop_name, ratio]
            elif ratio >= HudFloraVocab.FLORA_CROP_STRONG_RATIO:
                btn.tooltip_text = HudFloraVocab.FLORA_CROP_STRONG_TOOLTIP_FORMAT % [crop_name, ratio]
            elif ratio > SourceForecast.FLORA_CROP_RATIO_NONE:
                btn.tooltip_text = HudFloraVocab.FLORA_CROP_MODEST_TOOLTIP_FORMAT % [crop_name, ratio]
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
    # The ONLY standing line under the list is the one that REPLACES content rather than adding to it:
    # a basket with nothing this rung can take has no pressable row to carry the explanation.
    if not any_legal:
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
    var key := "%d,%d" % [x, y]
    # When the selected tile changes, default the actor band to the resolved band (and re-seed
    # the count from its staffing); otherwise preserve the picked band + count across the
    # per-snapshot re-renders of the same tile.
    var source_changed := _compose.forage_key() != key
    if source_changed:
        _compose.begin_forage_source(key, int(resolved.get("entity", -1)))
    var band := _band_labor.player_band_by_entity(_compose.forage_band())
    if band.is_empty():
        band = resolved
        _compose.set_forage_band(int(band.get("entity", -1)))
    if source_changed:
        # `seed_forage` also clears the crop: a crop pick belongs to the PATCH it was made on, and a
        # new tile has a different basket.
        var staffed := _band_labor.workers_for_forage(band, x, y)
        _compose.seed_forage(staffed if staffed > 0 else HudConst.WORKER_STEP, _band_labor.policy_for_forage(band, x, y))
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
        _build_forage_assign_controls(tile_info, target)))
    # Forage take policy (Sustain/Surplus/Market/Eradicate, default Sustain) — reuses the hunt policy
    # radio + option set (LABOR_HUNT_POLICIES) but shows forage-appropriate behaviour hints. Persisted
    # across re-renders like the hunt policy; re-seeded from current staffing when the tile changes.
    var forage_gates := _forage_policy_gates(tile_info)
    # A gated rung can never be the composed policy — the patch may have left Thriving under a
    # standing Cultivate selection, so re-validate every render, not just on a tile change.
    if not (_compose.forage_policy() in HudBandLaborState.FORAGE_POLICY_OPTIONS) \
            or not HudWidgets.gate_reasons(forage_gates, _compose.forage_policy()).is_empty():
        _compose.set_forage_policy(SourceForecast.DEFAULT_HUNT_POLICY)
    # Ascending per-policy per-turn takes on the extractive buttons, so the forage picker wears the SAME
    # "+X /turn" button metric the local-hunt picker does (the investment rungs Cultivate/Sow carry none,
    # like Corral — their dip→payoff is stated by the forecast row below).
    var forage_takes := _forage_policy_takes(tile_info)
    target.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
        _compose.set_forage_policy(policy)
        # Picking a policy auto-fills the foragers to that policy's max-useful (consumed next rebuild).
        _compose.arm_forage_autofill()
        _build_forage_assign_controls(tile_info, target), _compose.forage_policy(), HudBandLaborState.FORAGE_POLICY_OPTIONS,
        forage_gates, forage_takes, HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS,
        # Collapse the OTHER rungs' reasons only while a committing rung is composed — that is the one
        # card that also carries the crop picker, and the only place the height is not there.
        _compose.forage_policy() in HudFloraVocab.FLORA_COMMITTING_POLICIES))
    target.add_child(HudWidgets.alloc_hint_label(String(HudComposeVocab.FORAGE_POLICY_HINTS.get(_compose.forage_policy(), ""))))
    # WHICH CROP this rung commits the patch to (flora roster S1). Only the two COMMITTING rungs show
    # it; the selection is re-resolved every render (a policy switch changes which plants are legal),
    # so the composed crop can never name a plant this tile+rung cannot take — and "" always
    # remains valid, meaning "take the sim's default".
    var basket := SourceForecast.flora_basket_entries(tile_info.get("patch_composition", []))
    var committed_crop := String(tile_info.get("patch_committed_display_name", "")).strip_edges()
    var is_committed := String(tile_info.get("patch_committed_species", "")).strip_edges() != "" \
        and committed_crop != ""
    _compose.resolve_forage_species(func(current: String) -> String:
        return _resolve_crop_selection(basket, _compose.forage_policy(), is_committed, current))
    if _compose.forage_policy() in HudFloraVocab.FLORA_COMMITTING_POLICIES:
        var crop_picker := _build_crop_picker(basket, _compose.forage_policy(), _compose.forage_species(),
            committed_crop if is_committed else "",
            func(species: String) -> void:
                _compose.set_forage_species(species)
                _build_forage_assign_controls(tile_info, target))
        if crop_picker != null:
            target.add_child(crop_picker)
    # Pre-commit forecast: the patch's per-worker yield + the SELECTED policy's ceiling cap the
    # stepper at max-useful workers, so the player CAN'T over-assign while composing. Both the
    # stepper and the policy picker re-render these controls, so the cap and the expected-yield row
    # below recompute on every change (a Market/Eradicate ceiling is higher than Sustain's, so
    # switching policy moves the cap).
    var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_policy())
    # THE "→ then" TERM FOLLOWS THE CROP. `SourceForecast.forecast_inputs` answers for the patch, which is species-
    # blind; once a crop is committed the payoff is that crop's. `basket` and the composed crop
    # are resolved above, and the picker's own handler rebuilds these whole controls, so changing the
    # selection moves this line on the same frame. Only `payoff` is substituted — the ceiling and the
    # per-worker rate still describe the PATCH, which is what caps the stepper.
    forecast = _forecast_for_selected_crop(forecast, basket, _compose.forage_policy(), _compose.forage_species())
    var capped := _forecast_worker_cap(forecast, _band_labor.assignable_forage_workers(band, x, y))
    var cap := int(capped["cap"])
    # Auto-max on policy select — "give me everything this patch sustains": jump to the max-useful for
    # the policy (clamped to available below). Only ever set by a policy click, never by a −/+ tick.
    if _compose.consume_forage_autofill():
        _compose.set_forage_count(cap)
    _compose.clamp_forage_count(cap)
    target.add_child(HudWidgets.build_worker_stepper(
        HudComposeVocab.FORAGE_CREW_LABEL, _compose.forage_count(), _compose.forage_count() < cap,
        func(n: int) -> void:
            _compose.set_forage_count(clampi(n, 0, cap))
            _build_forage_assign_controls(tile_info, target)))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # WOULD THIS SUBMIT CHANGE ANYTHING? `current` is the pending-aware standing staffing on this tile
    # for THIS band, so the two zero-worker cases are DIFFERENT SUBMITS, and the block below —
    # forecast line and button TOGETHER — has to read coherently for each:
    #   • 0 on a tile this band does NOT work → the command would do nothing. Dead button (still
    #     "Forage"), and the forecast states the payoff as a CONDITION ("Assign foragers to begin…").
    #   • 0 on a tile it DOES work → the sim's unassign (server.rs: "Unassigning (workers == 0) is
    #     always allowed"). Live button, renamed, and NO "assign to begin" line — a panel whose button
    #     says Unassign above a line reading "assign to begin" tells the player two opposite things.
    # Gating on the raw count instead would fix the no-op and break the unassign the Work zone needs.
    var is_unassign := _compose.forage_count() <= 0 and current > 0
    var is_noop := _compose.forage_count() <= 0 and current <= 0
    # ONE yield row per rung, mirroring the local hunt: an INVESTMENT rung (Cultivate/Sow) keeps
    # `_forecast_yield_row`'s dip→payoff deal ("Preparing: +X → then +Y"), which a single rate can't
    # express; an EXTRACTIVE rung renders the bare-rate + verdict preview (`+2.74 /turn · renewable` /
    # `⚠ … — overdraws the patch`) at the same font as the hunt line — which also surfaces the overdraw
    # warning an Eradicate/Market forage used to render silently.
    if _compose.forage_policy() in HudComposeVocab.INVESTMENT_POLICIES:
        # Nothing is forecast for an unassign — see is_unassign above. What abandoning costs is already
        # on the card in the rung's own policy hint ("It must stay staffed or it goes feral"), so a
        # second warning here would state one fact twice.
        if bool(forecast["known"]) and not is_unassign:
            target.add_child(
                _forecast_yield_row(forecast, _compose.forage_count(), band, HudComposeVocab.FORAGE_CREW_LABEL))
    else:
        var yield_line := _local_forage_preview_bbcode(
            band, tile_info, _compose.forage_policy(), _compose.forage_count())
        if yield_line != "":
            target.add_child(HudWidgets.forecast_label(yield_line))
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
    assign_btn.text = HudComposeVocab.FORAGE_UNASSIGN_BUTTON if is_unassign else HudComposeVocab.FORAGE_ASSIGN_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range or is_noop
    assign_btn.pressed.connect(func() -> void:
        _emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, _compose.forage_count(), x, y, "",
            _compose.forage_policy(), _compose.forage_species())
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

## The crew noun the sheet's stepper uses for this herd — herders on a MANAGED (corralled/pastoral)
## herd, hunters on a wild one. Read by the drawer button too, so the two always agree.
func _herd_crew_noun(herd: Dictionary) -> String:
    return HudComposeVocab.HERD_CREW_LABEL if SourceForecast.is_managed_hunt_source(herd, _compose.hunt_policy()) else HudComposeVocab.HUNT_CREW_LABEL

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
    var shape := _standing_actions_shape(summary_model)
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
        func() -> void: open_forage_compose(tile_info)))
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
    var shape := _herd_actions_shape(corralled, extending, available, summary_model)
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
            noun, herd_id, func() -> void: open_herd_compose(herd)))
    _herd_drawer_shape = shape

## Free the herd drawer-actions and forget its shape, so the next build always rebuilds.
func _clear_herd_drawer() -> void:
    if _herd_assign_controls == null:
        return
    for child in _herd_assign_controls.get_children():
        child.queue_free()
    _herd_drawer_shape = []

## The forage drawer-actions shape: `[has_summary, warn, has_note, has_muted]` — the full set of
## optional child slots, so any structural change (summary appearing/disappearing, a warn/note/muted
## label appearing) moves the signature and forces a rebuild rather than a stale positional patch.
func _standing_actions_shape(summary_model: Dictionary) -> Array:
    if summary_model.is_empty():
        return [false, false, false, false]
    return [true, bool(summary_model["warn"]),
        String(summary_model["note"]) != "", String(summary_model["muted_note"]) != ""]

## The herd drawer-actions shape: the extend control's kind + the summary structure + whether the
## compose button is present. Any change forces a rebuild rather than a positional patch.
func _herd_actions_shape(corralled: bool, extending: bool, available: bool, summary_model: Dictionary) -> Array:
    return [corralled, extending, available] + _standing_actions_shape(summary_model)

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