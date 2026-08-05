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
        floor: float, species: String = "",
        improvement: String = SourceForecast.IMPROVEMENT_NONE,
        kit_id: String = KitRoster.NO_KIT_ID) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, floor, species, improvement,
        kit_id)

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
##
## **`improvement` IS THE CREW'S OWN DIP** — while a Tame or a Corral runs the sim pays
## `workers × per_worker × build_dip`, so a take priced without it quotes ~4× what the herd will hand
## over. It is the caller's already-live verb (`SourceForecast.live_improvement`), never a raw composed
## one: a rung this herd has already climbed dips nothing.
## `holding` asks the same question of the steady state — the ceiling becomes one turn's regrowth at
## this floor instead of the room above it. Same swap, same reason, as `_hunt_delivered_and_waste`'s.
func _hunt_take_rate(herd: Dictionary, floor: float, workers: int, improvement: String,
        holding: bool = false) -> Dictionary:
    var rates := SourceForecast.herd_axis_rates(herd, floor, improvement)
    var per_worker_rate := float(rates["per_worker"])
    var ceiling := float(rates["hold_ceiling" if holding else "ceiling"])
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return {"available": false}
    return {
        "available": true,
        "rate": maxf(minf(float(workers) * per_worker_rate, ceiling), 0.0),
        "axis": String(rates["axis"]),
    }


## The averaging WINDOW (turns) for the whole-animal disclaimer — a STABLE, worker-independent property
## derived from the SELECTED floor's raw ceiling (NOT the crew's current delivered rate, which
## moves as workers change and made the old line blink out). Keyed on the FLOOR because a deeper
## floor frees more standing stock and so delivers lumpy whole animals over a different span. `g` =
## animals/turn that floor's ceiling buys: slow/big game (`g < 1`) lands one animal every ~`1/g` turns; fast game (`g >= 1`) delivers
## the "extra" fractional animal every ~`1/frac` turns. Returns 0 when `food_per_animal` / the ceiling is
## unknown (caller then skips the line). NEVER scaled by `output_multiplier` — it's a pure herd property.
func _hunt_avg_window_turns(herd: Dictionary, floor: float, improvement: String) -> int:
    # On the component the species pays: an inedible quarry's `food_per_animal` is honestly 0, so a
    # food-only derivation returns 0 and the disclaimer silently disappears from a wolf's picker even
    # though its delivery is every bit as lumpy. The animal COUNT is identical on either component.
    #
    # **THE VERB TRAVELS FOR THE AXIS, NOT FOR THE SPAN.** The two terms below — the ceiling and the
    # one-animal quantum — are undipped by construction (the dip rides the crew term alone), so the
    # window this returns is unchanged by a build, which is correct: it is a property of the FLOOR and
    # the species, deliberately not of the crew. What the verb buys is that the axis is chosen from the
    # same dipped vector the take's is, so the span can never end up quoting the rhythm of a product the
    # take beside it is not measured in.
    var rates := SourceForecast.herd_axis_rates(herd, floor, improvement)
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
##
## **`holding` ASKS THE SAME QUESTION OF THE STEADY STATE** — the take once the herd sits at its floor
## and only regrowth is on offer. It swaps ONLY the ceiling (the room becomes one turn's regrowth) and
## leaves the crew, the dip and the quantisation exactly where they are, so the burst and the steady
## rate are the same computation asked twice. A separate steady-state formula would be free to drop
## the whole-animal branch and print a smooth number beside a bodies-per-turn one.
func _hunt_delivered_and_waste(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        improvement: String, holding: bool = false) -> Dictionary:
    # PER COMPONENT, on the one this species pays (issue #337). The three terms must come from the SAME
    # axis or the arithmetic is nonsense: a wolf's per-animal FOOD quantum is 0 (divide by zero) while
    # its per-animal TRADE quantum is real. `herd_axis_rates` is the single place that choice is made,
    # and it reads the HERD's species-aware per-worker rates — never the cohort's species-blind
    # `hunt_per_worker_provisions`, which is what would re-introduce phantom food here.
    #
    # **THE DIP MULTIPLIES THE COLLECTION, AND THE QUANTISATION HAPPENS AFTER IT** — the sim's own
    # order (`hunt_take` composes `workers × per_worker × build_dip`, THEN
    # `fauna::quantise_animal_take`). It arrives here on `per_worker`, so `collection` below carries it
    # and the whole-animal branch is taken against the dipped throughput. That is not a scaling of the
    # answer: below one body the crew falls off the carryable branch entirely, still kills one animal
    # (the `max(1, …)` this function's else-branch mirrors) and wastes most of it — so a build moves the
    # WASTE line, not merely the take. Dipping the ceiling, or the delivered figure after quantisation,
    # produces a number that is wrong in a way that still looks plausible.
    var rates := SourceForecast.herd_axis_rates(herd, floor, improvement)
    var fpa := float(rates["per_animal"])
    var per_worker := float(rates["per_worker"])
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var ceiling := float(rates["hold_ceiling" if holding else "ceiling"])
    if fpa <= 0.0 or per_worker <= 0.0 or ceiling < 0.0 or workers <= 0:
        return {"available": false}
    ceiling *= output
    var collection := float(workers) * per_worker * output   # crew's raw food throughput /turn
    # WHOLE ANIMALS /TURN THE CREW CAN CARRY — **AND NEVER MORE THAN IT CAN REACH**
    # (`docs/plan_hunt_through_combat.md` §2). The sim's take is
    # `min(stock above the floor, collection, engaged × body_mass)` with the quantiser running on that
    # min, and the engagement arm was the one this sheet had never had: one hunter's 40 biomass of
    # carry read **307 Wild Fowl a turn** against a take of ten — the sheet promising 30× what the sim
    # pays, for the whole life of the wire field's absence.
    #
    # **THE BOUND IS APPLIED TO THE ANIMAL COUNT, NOT TO `collection`**, and the two are the same
    # arithmetic taken in different orders: `floor(min(carry, engaged × fpa) / fpa)` is
    # `min(floor(carry / fpa), engaged)` — but the first divides a product of `fpa` BY `fpa` and can
    # land a whole engagement one animal short on a rounding, while the second is exact. It also keeps
    # the partial-body branch below reading the RAW carry, which is right: engagement is never the
    # binding arm there (a party that exists reaches at least one animal), so the waste it reports
    # stays a carry story.
    #
    # `animals_engaged` answers UNBOUNDED for a pen and for a species with no engagement stage, so the
    # `min` is a no-op on every managed-herd and plant-web frame.
    var carryable := minf(floorf(collection / fpa),
        SourceForecast.animals_engaged(workers, float(rates["engage_rate"]), float(rates["dip"])))
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


## Each FLOOR PRESET's button metric on a LOCAL hunt, keyed preset -> a `{compact, full}` pair (compact
## for the button face's second line, full for the tooltip). The plant twin is `_forage_floor_takes`;
## both wear the same shape, only the source of the ceiling differs.
##
## The metric is the herd's worker-independent CEILING at that preset's floor — `max(0, B - f*K) x the
## species' per-biomass vector`, composed by `SourceForecast.forecast_inputs`. Composed, not looked up:
## the per-stance ceiling rows are retired `(deprecated)` wire slots that read zero, and four rows
## could not answer a continuous dial anyway.
##
## **THE HUNT SIDE ALSO FILLS THE PAIR'S OPTIONAL `note`** — the averaging-window disclaimer
## (`HudComposeVocab.HUNT_AVG_WINDOW_FORMAT`), which the picker appends under the preset's tooltip
## metric line. It is a caveat on THIS floor's rate (a hunt lands whole animals, so a per-turn figure
## is a long-run average), so it rides the preset's own take pair rather than a body line: keyed per
## preset, since the span differs with the floor, and omitted when the window is unknown. The forage
## twin fills no note — a patch's take is smooth, and there is nothing to average.
##
## **THE PRESET METRICS THEMSELVES ARE UNDIPPED, AND THAT IS THE RULE RATHER THAN AN OVERSIGHT** — a
## ceiling is what the herd offers above the floor whether the crew is hunting it or building on it
## (§3.1). `improvement` is threaded in for the WINDOW alone, which resolves its axis through
## `herd_axis_rates` and must pick the same component the sheet's take does.
##
## Empty when the wire does not describe this herd (older snapshot / non-huntable).
func _hunt_floor_takes(herd: Dictionary, improvement: String) -> Dictionary:
    var takes := {}
    var zero_account := SourceForecast.zero_account_of(herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        var floor_value := SourceForecast.floor_for_preset(preset)
        var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
            HudComposeVocab.BARE_FORECAST_PREFIX, floor_value)
        if not bool(forecast["known"]):
            continue
        # BOTH products (issue #337): each preset's cap is a pair, each half rendered only when
        # non-zero. A wolf's presets therefore read as trade caps rather than `+0.00`s — the false
        # reading that said an inedible species was worth nothing at every floor.
        var pair := SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]), float(forecast["ceiling_trade"]), 0.0, zero_account)
        var window_turns := _hunt_avg_window_turns(herd, floor_value, improvement)
        if window_turns > 0:
            pair["note"] = HudComposeVocab.HUNT_AVG_WINDOW_FORMAT % window_turns
        takes[preset] = pair
    return takes


## The LOCAL hunt's live per-turn yield preview, or "" when the snapshot lacks the levers/ceilings
## (graceful degrade — no line, panel otherwise unchanged). A resident band applies its
## `output_multiplier` (morale/discontent productivity) at payout, so the preview is the take rate
## scaled by it. Reads income-green when the take is within the herd's sustainable yield (the Sustain
## ceiling), WARN-amber with the shared ⚠ when it overdraws — the same flag the allocation rows carry.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        improvement: String = SourceForecast.IMPROVEMENT_NONE) -> String:
    return _yield_preview_bbcode(_hunt_yield_model(band, herd, floor, workers, improvement),
        HudComposeVocab.LOCAL_HUNT_OVERDRAW_SUFFIX)

## The hunt web's yield model — the animal twin of `_forage_yield_model`, in the same shape.
##
## **ITS ROWS ARE ACCOUNTS, LIKE EVERY OTHER PER-TURN READING — one per account the take PAYS.** The
## readout answers what a turn of this hunt puts in the band's stores, so it is stated in every
## account the take credits (an edible species that also sells its hide pays food AND trade; a wolf
## pays trade alone) through `SourceForecast.rescaled_accounts` → `yield_rows` and the account
## table's units, exactly as the plant web's is and exactly as the raid's payload
## (`_trip_yield_rows`) already was. **The WHOLE-ANIMAL reading belongs to the CHART above it** (the
## escapement curve and its handle, which count bodies) and to the whole-trip payload of a raid; a
## per-turn row wearing the quarry's name in place of an account states a rate in a currency the
## stores do not keep, and the header over it (`per turn · now → after`) then keys nothing the
## number beside it can be spent as.
##
## **`improvement` IS THE CREW'S OWN DIP, and the two forecasts here take it DIFFERENTLY** — the plant
## twin's rule, on the animal web. The TAKE carries it (the sim pays a building crew
## `workers × per_worker × build_dip`, and it is the crew's collection that is then quantised into whole
## animals); the SUSTAIN reference below must not.
func _hunt_yield_model(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        improvement: String, reaches: bool = false) -> Dictionary:
    # **THE SUSTAINABILITY BAR IS THE FOOD PEAK'S CEILING**, on the SAME axis the take is measured on
    # (comparing a trade take against a food ceiling would flag every wolf hunt as an overdraw, or
    # none of them). It is the floor at which the herd settles on its most productive biomass, so a
    # take above it is one the herd cannot pay forever — which is exactly what the verdict claims.
    #
    # **AND IT IS RESOLVED AT `IMPROVEMENT_NONE` DELIBERATELY — the one call site where the undipped
    # rates are the correct ones.** This is the LINE THE TAKE IS JUDGED AGAINST, not a take: it is the
    # herd's own renewable yield, a fact about the animals rather than about the crew. Dipping it would
    # move the bar down in step with the take it is compared to, the two dips would cancel, and a
    # building crew could never trip the flag at all — the ⚠ would become vacuous exactly where a
    # quarter-throughput crew is least able to explain itself.
    var sustain_rates := SourceForecast.herd_axis_rates(herd, SourceForecast.FLOOR_FOOD_PEAK,
        SourceForecast.IMPROVEMENT_NONE)
    var sustain_ceiling := float(sustain_rates["ceiling"])
    if sustain_ceiling < 0.0:
        return {}
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var sustainable := sustain_ceiling * output
    var dw := _hunt_delivered_and_waste(band, herd, floor, workers, improvement)
    if not bool(dw.get("available", false)):
        # Graceful degrade — the per-animal quantum (or a lever) is unknown on BOTH components, so fall
        # back to the smoothed per-turn line rather than regress the readout. **It credits the SAME
        # account set the quantised path does**, through the same rescale: the two paths differ in
        # whether the take is quantised, never in what a take pays, and a model whose two branches
        # stated different currencies for one herd is the defect one branch above records.
        var take := _hunt_take_rate(herd, floor, workers, improvement)
        if not bool(take.get("available", false)):
            return {}
        var actual := float(take["rate"]) * output
        var smooth := SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            String(take["axis"]), actual)
        var trade_axis: bool = String(take["axis"]) == SourceForecast.YIELD_AXIS_TRADE
        var account := SourceForecast.YIELD_ACCOUNT_TRADE if trade_axis \
            else SourceForecast.YIELD_ACCOUNT_FOOD
        var smooth_after := {}
        var smooth_hold := _hunt_take_rate(herd, floor, workers, improvement, true)
        if reaches and bool(smooth_hold.get("available", false)):
            smooth_after = SourceForecast.rescaled_accounts(herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, String(smooth_hold["axis"]),
                float(smooth_hold["rate"]) * output)
        return {
            YIELD_MODEL_ROWS: SourceForecast.yield_rows(
                float(smooth[SourceForecast.YIELD_ACCOUNT_FOOD]),
                float(smooth[SourceForecast.YIELD_ACCOUNT_TRADE]),
                float(smooth[SourceForecast.YIELD_ACCOUNT_FODDER]),
                account, smooth_after),
            # The SENTENCE states the same vector the rows do — `yield_components` is the joiner the
            # plant twin already uses, and it obeys the same render-only-when-non-zero rule, so this
            # line cannot quote one account beside a row set carrying two.
            YIELD_MODEL_TEXT: HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % (
                SourceForecast.yield_components(
                    float(smooth[SourceForecast.YIELD_ACCOUNT_FOOD]),
                    float(smooth[SourceForecast.YIELD_ACCOUNT_TRADE]),
                    float(smooth[SourceForecast.YIELD_ACCOUNT_FODDER]), account)),
            YIELD_MODEL_OVERDRAW: _is_overdraw(actual, sustainable) \
                and _herd_take_draws_down(herd, floor, workers, improvement),
            YIELD_MODEL_WASTE: "",
        }
    # The crew's honest carry-aware delivered take. `delivered` is already carry-quantized, so this
    # credits no throughput the crew can't haul home — and it is a take in an ACCOUNT, which is what
    # the readout row states. The animal RATE derived beside it is the SENTENCE's (`YIELD_MODEL_TEXT`,
    # the one-line preview), where the whole-animal rhythm is the whole point of the line.
    var fpa := float(dw["per_animal"])
    var delivered := float(dw["delivered"])
    var animal_rate := delivered / fpa if fpa > 0.0 else 0.0
    var rate_text := _format_animal_rate(animal_rate)
    var quarry := SourceForecast.herd_display_name(herd)
    # Overdraw and waste are DIFFERENT flags and may co-occur — render both. Overdraw = the delivered take
    # exceeds the herd's food-peak ceiling; waste = a kill the crew couldn't carry.
    var waste_pct := float(dw["waste_pct"])
    # **THE COUNT IS TAKEN ON ONE AXIS AND VALUED IN EVERY ACCOUNT** — the sim's own order
    # (`forecast_production_and_take`: quantise on `ratio_axis()`, then `YieldPair::rescaled_to`), and
    # the half this readout used to drop. A boar's take genuinely credits meat AND hide; stopping at
    # the axis the quantiser happened to divide by rendered its `PER TURN` row as food alone, while
    # the very same species raided by an expedition (`_trip_yield_rows`) stated both. `yield_rows` is
    # still the one place the "render only where the vector pays" rule lives, so a wolf — whose
    # provisions rate is a structural 0 — rescales to a zero food component that renders NO row, and
    # the zero account below keeps that answer for an all-zero take.
    var take := SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
        String(dw["axis"]), delivered)
    # THE ZERO ACCOUNT IS THE AXIS THE TAKE WAS MEASURED ON — the same choice the degrade branch above
    # makes, so one model's two paths can never state an empty take in two different currencies.
    var trade_axis: bool = String(dw["axis"]) == SourceForecast.YIELD_AXIS_TRADE
    var account := SourceForecast.YIELD_ACCOUNT_TRADE if trade_axis \
        else SourceForecast.YIELD_ACCOUNT_FOOD
    # THE STEADY STATE RIDES EACH ACCOUNT'S OWN `after`, so `build_yields_row` composes the arrow from
    # the two magnitudes it formats itself and its header keys them. It RESCALES THE SAME WAY the take
    # does — an arrowed row must key both accounts consistently, and a hold rate credited on one axis
    # beside a take credited on two would arrow only half the reading. `yield_rows` drops an `after`
    # equal to its take, which is the same "an arrow to itself is noise" test this used to make here.
    var after := {}
    var held := _hunt_delivered_and_waste(band, herd, floor, workers, improvement, true)
    if reaches and bool(held.get("available", false)):
        after = SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            String(held["axis"]), float(held["delivered"]))
    return {
        YIELD_MODEL_ROWS: SourceForecast.yield_rows(
            float(take[SourceForecast.YIELD_ACCOUNT_FOOD]),
            float(take[SourceForecast.YIELD_ACCOUNT_TRADE]),
            float(take[SourceForecast.YIELD_ACCOUNT_FODDER]),
            account, after),
        YIELD_MODEL_TEXT: HudComposeVocab.HUNT_DELIVERED_FORMAT % [rate_text, quarry],
        YIELD_MODEL_OVERDRAW: _is_overdraw(delivered, sustainable) \
            and _herd_take_draws_down(herd, floor, workers, improvement),
        YIELD_MODEL_WASTE: SourceForecast.HUNT_WASTE_NOTE_FORMAT % int(round(waste_pct * 100.0)) \
            if waste_pct > 0.0 else "",
    }

## The hunt web's half of the overdraw GATE — `SourceForecast.take_draws_down` on a herd, asked at the
## crew's LIVE improvement.
##
## **A GATE MUST WALK THE SAME CREW THE TAKE IT GATES IS PRICED FOR.** It was asked at
## `IMPROVEMENT_NONE` "to match its undipped takes" — a premise that held only while `herd_axis_rates`
## silently dropped the verb. Now that the take carries the dip, an undipped projection would walk a
## crew four times the one being quoted, so it would report a herd falling that this crew is nowhere
## near able to draw down, and the ⚠ would fire over a take that takes less than the herd regrows. The
## verb is the caller's already-live one; `take_draws_down` runs it through `build_dip`, so a rung this
## herd has already climbed still dips nothing.
func _herd_take_draws_down(herd: Dictionary, floor: float, workers: int,
        improvement: String) -> bool:
    return SourceForecast.take_draws_down(herd, SourceForecast.SOURCE_KIND_HERD, "", floor, workers,
        improvement)

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
##
## **AND ALL OF IT IS GATED ON THE PROJECTION ACTUALLY FALLING** (`SourceForecast.take_draws_down`).
## The per-account test above is a take against the FOOD-PEAK ceiling, which on a source standing at
## or below that peak is just `take > 0` — a fact about the floor, not about the stock — so the ⚠
## could sit two lines above a verdict reading *it settles at 53% and holds there*. Nothing is
## overdrawn while the stock climbs, whatever the floor says. The gate is purely subtractive: a source
## with no curve to walk keeps the flag it always had.
##
## **`improvement` IS THE CREW'S OWN DIP, and the two forecasts here take it DIFFERENTLY.** The take
## must carry it — while a build runs the sim pays `min(crew × per_worker, ceiling × dip)`, and this
## line quoting the undipped ceiling is what made it disagree with both the deal line's middle term and
## the worked row it becomes next turn. The SUSTAIN reference must NOT: it is the patch's regrowth
## rate, a property of the land, and dipping it would move the sustainability bar down in step with the
## take and let a genuinely overdrawing build read green.
func _local_forage_preview_bbcode(band: Dictionary, tile_info: Dictionary, floor: float,
        workers: int, improvement: String = SourceForecast.IMPROVEMENT_NONE) -> String:
    return _yield_preview_bbcode(_forage_yield_model(band, tile_info, floor, workers, improvement),
        HudComposeVocab.LOCAL_FORAGE_OVERDRAW_SUFFIX)

## The structured half of the line above — and the SHARED shape both webs answer in, so the readout's
## yields row is built from the same numbers the sentence quotes rather than from a second derivation.
## `{}` is the graceful degrade (an unknown forecast, or a source that pays into no account at all);
## `rows` is `SourceForecast.yield_rows`' answer verbatim, so the render-only-where-the-vector-pays
## rule is obeyed by construction and no zero can be reintroduced by a caller.
const YIELD_MODEL_ROWS := "rows"
const YIELD_MODEL_TEXT := "text"
const YIELD_MODEL_OVERDRAW := "overdraw"
const YIELD_MODEL_WASTE := "waste"
## WHY one of this model's accounts renders as a dash instead of a number — `""` when every account
## this take pays is bankable. It rides the MODEL rather than being resolved at the render, so the
## muted row and the sentence explaining it are two readings of one model dict: whoever evaluates this
## model at a given floor and crew gets both, and neither can be composed without the other.
const YIELD_MODEL_LOCKED_REASON := "locked_reason"
func _forage_yield_model(band: Dictionary, tile_info: Dictionary, floor: float,
        workers: int, improvement: String = SourceForecast.IMPROVEMENT_NONE,
        reaches: bool = false) -> Dictionary:
    # The FOOD-PEAK ceiling is the patch's sustainable yield (what it will pay forever), so a take
    # above it draws the patch down — the same bar the hunt version uses, for the same reason.
    var sustain := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK)
    if not bool(sustain["known"]):
        return {}
    var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, improvement)
    if not bool(forecast["known"]):
        return {}
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var actual := SourceForecast.expected_yield(forecast, workers, band)
    # The trade account names its own whole-animal quantum so the engagement arm can price it; a PATCH
    # publishes none, so the arm drops out and the plant web reads exactly as it did. Fodder names
    # none anywhere — no animal pays it — which is why those two calls leave the key defaulted.
    var actual_trade := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_trade", "ceiling_trade",
        SourceForecast.FORECAST_TRADE_PER_ANIMAL_KEY)
    var actual_fodder := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_fodder", "ceiling_fodder")
    var zero_account := String(forecast["zero_account"])
    # THE STEADY-STATE TAKE, one `min` against a different ceiling — the SAME `expected_yield_account`,
    # reached by key, so the burst and the hold rate cannot be computed two ways. Composed only when
    # the crew actually reaches the floor: a crew that settles short never enters the holding state,
    # and promising it a rate it never attains is the failure this whole reading exists to fix.
    var after := {}
    if reaches:
        after = {
            SourceForecast.YIELD_ACCOUNT_FOOD: SourceForecast.expected_yield_account(
                forecast, workers, band, "per_worker", "hold_ceiling",
                SourceForecast.FORECAST_FOOD_PER_ANIMAL_KEY),
            SourceForecast.YIELD_ACCOUNT_TRADE: SourceForecast.expected_yield_account(
                forecast, workers, band, "per_worker_trade", "hold_ceiling_trade",
                SourceForecast.FORECAST_TRADE_PER_ANIMAL_KEY),
            SourceForecast.YIELD_ACCOUNT_FODDER: SourceForecast.expected_yield_account(
                forecast, workers, band, "per_worker_fodder", "hold_ceiling_fodder"),
        }
    var rows := SourceForecast.yield_rows(actual, actual_trade, actual_fodder, zero_account, after)
    if rows.is_empty():
        # The patch pays in NO account at all — there is no line to draw rather than a zero to print.
        return {}
    # **THE FODDER ACCOUNT MAY BE REAL AND UNBANKABLE AT ONCE** (issue #485). The sim credits a WILD
    # patch's hay only to a faction that has learned Foddering (a committed patch is paid
    # unconditionally — committing is the bid), and Foddering is taught by KEEPING A PEN, so a forager
    # band reads a live `fodder_per_biomass` off the meadow and banks nothing from it. The take is not
    # recomputed: it is what the crew MOVES, and only the credit is refused, so the row keeps its unit
    # and loses its number while the sentence beside it goes to `0.0`.
    var locked := _wild_fodder_lock(tile_info)
    var banked_fodder := actual_fodder
    if locked != "" and SourceForecast.has_component(actual_fodder):
        for row in rows:
            if String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, "")) \
                    != SourceForecast.YIELD_ACCOUNT_FODDER:
                continue
            row[HudWidgets.YIELD_ROW_NUMBER] = HudComposeVocab.YIELD_LOCKED_GLYPH
            row[HudWidgets.YIELD_ROW_MUTED] = true
            # An arrow to a rate nobody banks is noise — and its presence alone would key the row
            # header's `now → after` off an account that has neither reading.
            row.erase(SourceForecast.YIELD_ROW_AFTER)
        banked_fodder = 0.0
    else:
        locked = ""
    return {
        YIELD_MODEL_ROWS: rows,
        # The joined sentence has no room for the reason, so it must not promise the account at all.
        YIELD_MODEL_TEXT: SourceForecast.yield_components(
            actual, actual_trade, banked_fodder, zero_account),
        # **THE FODDER CEILING COMPARISON STAYS, LOCK OR NO LOCK, and deleting it is the plausible
        # wrong move.** The take draws the same biomass down whether or not the crew banks the hay, so
        # the drawdown is unchanged — and on a hay-only patch this comparison is the only drawdown
        # signal there is.
        YIELD_MODEL_OVERDRAW: (_is_overdraw(actual, float(sustain["ceiling"]) * output) \
            or _is_overdraw(actual_trade, float(sustain["ceiling_trade"]) * output) \
            or _is_overdraw(actual_fodder, float(sustain["ceiling_fodder"]) * output)) \
            and SourceForecast.take_draws_down(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
                HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, workers, improvement),
        YIELD_MODEL_WASTE: "",
        YIELD_MODEL_LOCKED_REASON: locked,
    }

## **THE ONE SPELLING OF THE WILD-FODDER LOCK ON THIS SHEET** — `""` when this crew's hay is bankable,
## the reason when it is not. Two surfaces answer to it and they sit one control apart: the yields
## row's muted `—` (with the aside sentence explaining it) and the FLOOR PRESETS' dropped fodder
## ceiling. A second predicate over one gate — even one spelled identically today — is exactly how the
## presets came to quote a ceiling the row below them was already refusing (issue #485), so both go
## through this call and through `RungGates.wild_fodder_reason` behind it.
##
## **The PUBLISHED commitment, never the composed improvement**: a Cultivate the player has ticked and
## not yet committed is not a bid the sim has accepted.
func _wild_fodder_lock(tile_info: Dictionary) -> String:
    return RungGates.wild_fodder_reason(
        String(tile_info.get("patch_committed_species", "")).strip_edges(), _player_knowledge())

## One yield model → the one-line BBCode preview, for both webs. Green + `· renewable` inside the
## source's own regrowth, WARN-amber with the shared ⚠ and the web's own overdraw clause outside it;
## the waste note, where a web has one, is ALWAYS amber even when the line around it is green,
## because a kill the crew could not carry is its own concern.
func _yield_preview_bbcode(model: Dictionary, overdraw_suffix: String) -> String:
    if model.is_empty():
        return ""
    var text := String(model[YIELD_MODEL_TEXT])
    var body := ""
    if bool(model[YIELD_MODEL_OVERDRAW]):
        body = "[color=#%s]%s %s%s[/color]" % [
            HudStyle.WARN_HEX, HudComposeVocab.OVERHUNT_FLAG, text, overdraw_suffix]
    else:
        body = "[color=#%s]%s%s[/color]" % [
            HudStyle.HEALTHY_HEX, text, SourceForecast.YIELD_TOOLTIP_RENEWABLE]
    var waste := String(model[YIELD_MODEL_WASTE])
    if waste != "":
        body += "[color=#%s]%s%s[/color]" % [
            HudStyle.WARN_HEX, SourceForecast.TRADE_COMPONENT_SEPARATOR, waste]
    return body

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
## disable the `+` that fixes it. The *hold it after* crew is a third floor and is applied INSIDE
## `SourceForecast.max_useful_workers`, so it reaches both twins without either being told about it.
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

## **THE IMPROVEMENT CONTROL** (issue #442 §3) — the second axis, in whichever ONE of its states this
## source is in, plus the deal it offers. Shared verbatim by both webs: the plant ladder
## (Cultivate → Sow) and the animal one (Tame → Corral) get the same control, the same states and the
## same forecast, because they are the same decision about different stock.
##
## The states and their precedence (see `HudWidgets.build_improvement_control` for the shape):
##   RUNNING first — something is being built here, so nothing else is on offer. Its face carries the
##       meter, and a WARN pause line appears when the source has left Thriving, which is the ONE
##       silent rule on this axis: the meter accrues only while the source is Thriving, and that is
##       deliberately NOT a gate (a source's phase swings as it is worked, so refusing the verb would
##       be un-actionable churn). The sim just PAUSES, losing nothing — and saying nothing here would
##       recreate exactly the hidden rule this whole arc exists to kill. It was animal-only
##       (`_tame_stalled_hint`) because the plant web had no control to hang it on.
##   DONE next — the source stands on a built rung, so the state gets a static label, and the NEXT
##       rung's checkbox renders beneath it if there is one.
##   OFFERED last — an unchecked box naming the next rung and its terms. When that rung is GATED the
##       box is not built at all: the reason takes the control's slot as a plain label (the fourth
##       state, `IMPROVEMENT_STATE_GATED` — see the gated branch below for why).
##
## `payoff_face` is the caller's per-rung terms Callable (`improvement -> String`), because the plant
## web substitutes the CROP the rung would commit to and the animal web quotes the herd. `extra_rows`
## is the same idea for whole controls: the plant web drops its CROP PICKER beneath the box, since
## which crop this rung commits to is part of the same decision. Passing both in rather than branching
## keeps this function free of flora knowledge.
func _build_improvement_control(kind: String, source: Dictionary, prefix: String, floor: float,
        composed: String, band: Dictionary,
        on_toggle: Callable, target: VBoxContainer,
        payoff_face: Callable = Callable(), extra_rows: Callable = Callable()) -> void:
    # RUNNING — a composed improvement that is not yet built. `composed` covers both the wire's
    # standing value and a box the player just ticked, so a fresh commitment reads as running
    # immediately rather than waiting a turn to stop looking like an offer.
    if composed != SourceForecast.IMPROVEMENT_NONE \
            and not SourceForecast.improvement_is_done(source, prefix, composed):
        var glyph := FoodIcons.for_policy(composed)
        var participle := String(
            HudComposeVocab.IMPROVEMENT_RUNNING_LABELS.get(composed, composed.capitalize()))
        var percent := HudFormat.progress_percent(
            SourceForecast.improvement_progress(source, prefix, composed))
        # **THE PAYOFF ON THE RUNNING FACE, IN THE OFFERED BOX'S OWN `· then` GRAMMAR.** It is the one
        # term of the deleted deal line that was unique to it: the line's middle term restated the
        # readout's PER TURN headline verbatim, and its first term is the price of building, which the
        # crew row states as a factor. So the two states of one control now read alike — the box that
        # offers the rung and the box that is running it quote the same promise.
        #
        # `kind` here is the LABOR kind (`hunt`/`forage`); the forecast layer speaks SOURCE kinds
        # (`herd`/`forage`). They differ on the animal web, so this conversion is not optional.
        var deal := SourceForecast.improvement_forecast(
            source, SourceForecast.source_kind_for_labor(kind), prefix, floor, composed)
        var notes := _improvement_paused_note(source, prefix)
        var running_face := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
            glyph, participle, percent]
        if not deal.is_empty():
            var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
            # The payoff terms follow the CROP on the plant web, exactly as the offered box's do — one
            # Callable, both states, so a box that starts a rung and a box running it can never quote
            # different crops.
            var payoff_terms := String(payoff_face.call(composed)) if payoff_face.is_valid() \
                else _payoff_terms(deal, band)
            var feed := float(deal["feed"]) * output
            var has_feed := bool(deal["feed_rung"]) and feed >= SourceForecast.FOOD_FLOW_MIN
            if payoff_terms != "":
                running_face = HudComposeVocab.IMPROVEMENT_RUNNING_FEED_FORMAT % [
                    glyph, participle, percent, payoff_terms,
                    SourceForecast.format_magnitude(feed)] if has_feed \
                    else HudComposeVocab.IMPROVEMENT_RUNNING_FORMAT % [
                        glyph, participle, percent, payoff_terms]
            # **A ZERO PAYOFF UNDER A RUNNING FEED IS A PURE LOSS, and the note that says so outlived
            # the line it was written for.** The pen harvests by constant escapement, so a herd at or
            # below the MSY point pays 0.00 while still eating feed every turn. It rides the control's
            # own note slot (WARN-inked, beside the paused line) rather than a forecast row, because
            # it is a warning about the rung the player is committing to.
            if has_feed and float(deal["payoff"]) * output < SourceForecast.FOOD_FLOW_MIN:
                notes.append(HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE)
        target.add_child(HudWidgets.build_improvement_control(composed,
            HudWidgets.IMPROVEMENT_STATE_RUNNING, running_face,
            _improvement_running_tooltip(kind, composed), on_toggle, notes, true))
        if extra_rows.is_valid():
            extra_rows.call(composed, target)
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
    # **A KNOWLEDGE gate renders NOTHING ON ITS OWN, and that is not the same as hiding it.** When it
    # is the SOLE reason, the aside two rows up already states the lesson live and quantified
    # ("Teaching cultivation at ×1.38 — a higher floor teaches faster") and the reason's remedy —
    # *forage a wild patch to learn it* — names the very work this sheet is composing, so the line told
    # the player to do what they were in the middle of doing, under a sentence that had said it
    # better. The control does not render either: dropping the reason alone would leave an unchecked,
    # live, clickable box (with its crop picker beneath it on the plant web) for a build the sim
    # rejects outright — strictly worse than the sentence removed. Suppressing the reason and
    # suppressing the control are ONE change, not a change plus a consequence.
    #
    # **BUT IT IS DROPPED ONLY WHEN IT IS ALONE, because deleting it beside a SOURCE gate changes what
    # the survivor MEANS.** Reported from play: a tended patch at Seed Selection 77% on ground with no
    # fresh water rendered only *"This ground is rich but too dry to farm"* — a lone reason reads as
    # THE reason, so the sheet claimed the knowledge was in hand and the water was all that stood in
    # the way. That is the message for a player who HAS Seed Selection.
    #
    # The suppression rested on a premise that is conditional: the aside states the lesson only while
    # the crew is genuinely working the source (`teaching_note`'s own `taking` test). On that frame
    # the floor sat at the stock, the crew took nothing, and the aside read "Teaching nothing" — so
    # the reason was deleted in favour of a sentence that then said nothing. Nothing is saved by
    # suppressing it beside a source gate anyway: the control renders regardless, so the choice is
    # only WHICH fact to show, and the one being deleted is the actionable one.
    #
    # Both then render — the knowledge reason leads (it is `reasons[0]`, the near-term one a player
    # can move on) and the source gate keeps the note treatment below. They are different decisions:
    # *you do not know how yet* means wait, *this ground will never take seed* means move on, and
    # collapsing them to one loses whichever the player needed. `RungGates.knowledge_gate_unmet` is
    # the same `track < KNOWLEDGE_COMPLETE` test the gate builders make, asked structurally rather
    # than by matching the reason's words; the builders append the knowledge reason FIRST on both
    # webs, so `size() == 1` under an unmet gate is exactly "the knowledge reason, alone".
    if reasons.size() == 1 and RungGates.knowledge_gate_unmet(rung, _player_knowledge()):
        return
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
## Quoted at the FOOD PEAK, because a payoff is a property of the finished rung and not of the floor
## the crew happens to hold while building it. The floor reaches the deal only through the crew's dip.
func _improvement_payoff_terms(source: Dictionary, kind: String, prefix: String, rung: String,
        band: Dictionary) -> String:
    return _payoff_terms(SourceForecast.improvement_forecast(source,
        SourceForecast.source_kind_for_labor(kind), prefix, SourceForecast.FLOOR_FOOD_PEAK, rung),
        band)

## An already-resolved deal's payoff VECTOR as products, scaled by the acting band's output
## multiplier — "" for a deal the wire does not quote. Both states of the control read it: the OFFERED
## box through `_improvement_payoff_terms` (which resolves the deal itself) and the RUNNING box from
## the deal it has already resolved for the feed term, so one payoff is composed one way.
func _payoff_terms(deal: Dictionary, band: Dictionary) -> String:
    if deal.is_empty():
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    return SourceForecast.picker_products(float(deal["payoff"]) * output,
        float(deal["payoff_trade"]) * output, float(deal["payoff_fodder"]) * output)

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

# ---- THE FLOOR'S LIVE READINGS (docs/plan_harvest_floor.md §7.3, §7.1, §7.6) --------------------
#
# **A DRAG CANNOT REBUILD THE SHEET.** Every other control here answers a click by re-running the
# whole builder, which `queue_free`s the children — including the control that was clicked. That is
# harmless for a button and fatal for a DRAG: Godot routes motion to the node that took the press
# until the button comes up, so freeing the chart mid-drag ends the drag on the first pixel of
# movement. The chart therefore reports live motion separately from its commit, and every reading that
# follows the floor lives in its own small host that a live change REFILLS in place. On release the
# ordinary rebuild runs and everything re-reads.
#
# **THE SET IS A REGISTRY, NOT A FIXED PAIR OF KEYS, AND WHAT IT MISSED WAS THE POINT OF THE PANEL.**
# It held the crew targets and the verdict; the YIELDS ROW — the food and trade numbers the player is
# dragging TOWARD — was outside it, so the one reading the gesture is aimed at was the one frozen
# while the gesture ran, catching up only on release. Reported from play. The rule the registry
# encodes: anything whose value depends on the floor belongs in it (the yields, both crew targets, the
# verdict, the idle-crew note and the teaching line), and anything that does not must stay out, or the
# drag pays for work it does not need.
#
# Each entry is the HOST to refill plus the `fill(host, model, workers)` that refills it, so adding a
# reading is one `_register_live` call rather than a new key, a new type test and a new branch in
# `_refresh_floor_live`. The registry is passed around rather than held as a member because two sheets
# build one each and every rebuild makes new nodes; a member would outlive the nodes it names.
const FLOOR_LIVE_NODE := "node"
const FLOOR_LIVE_FILL := "fill"

## Register a host that FOLLOWS THE FLOOR, and fill it once. The CALLER owns the node — a crew target
## sits inline in the crew row, a readout register inside the readout box — so the host is passed in
## rather than made here.
func _register_live(hosts: Array, host: Container, model: Dictionary, workers: int,
        fill: Callable) -> void:
    hosts.append({FLOOR_LIVE_NODE: host, FLOOR_LIVE_FILL: fill})
    fill.call(host, model, workers)

## Refill every live host against a model composed at the floor the drag is currently on. Each host is
## re-checked for validity: a snapshot can rebuild the sheet under a drag, and a stale reference here
## would be a freed node.
func _refresh_floor_live(hosts: Array, model: Dictionary, workers: int) -> void:
    if not bool(model.get("known", false)):
        return
    for entry in hosts:
        var host: Variant = entry.get(FLOOR_LIVE_NODE, null)
        if not (host is Container and is_instance_valid(host)):
            continue
        HudWidgets.clear_children(host as Container)
        (entry[FLOOR_LIVE_FILL] as Callable).call(host, model, workers)

## **THE CREW ROW** (§7.6) — a quiet row-label, then the stepper and BOTH crew targets on one wrapping
## line beneath it. It used to be three rows: a body-size heading with the stepper flung to the far
## right by a spacer, and the two targets as full-width boxed buttons of their own. That is three rows
## and two competing edges for one statement, and it made the panel's second-loudest thing a control
## the chart above it had already decided most of.
##
## A source with NO chart model gets the stepper alone: an expedition, a managed rung-3 source, a
## source the wire published no curve for — none of them has a floor axis, so there is nothing to
## target. That is a different answer from `NO_CREW_ANSWER`, which is a source that HAS one and whose
## crew cannot be priced (a dead-season patch); `build_crew_targets` drops those, one target each.
func _mount_crew_row(parent: VBoxContainer, hosts: Array, crew_label: String, count: int,
        plus_enabled: bool, on_change: Callable, model: Dictionary, on_pick: Callable) -> void:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_LABEL_SEPARATION)
    var row_label := HudWidgets.alloc_section_label(crew_label)
    row_label.set_meta(HudWidgets.CREW_ROW_LABEL_META, true)
    # THE ROW LABEL AND THE BUILD-DIP NOTE ARE ONE PHRASE — `FORAGERS — while building, each carries
    # 25% as much` — so they share a line above the stepper, and the note renders only where a build
    # is actually dipping this crew. It is static for the life of the sheet (a floor drag moves every
    # number the dip multiplies, never the dip), so it stays OUT of the live-refresh registry below.
    var label_line := HBoxContainer.new()
    label_line.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_NOTE_SEPARATION)
    label_line.add_child(row_label)
    var dip_note := HudWidgets.build_crew_dip_note(
        float(model.get("build_dip", SourceForecast.NO_BUILD_DIP)))
    if dip_note != null:
        label_line.add_child(dip_note)
    block.add_child(label_line)
    var line := HFlowContainer.new()
    line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line.add_theme_constant_override("h_separation", HudComposeVocab.CREW_ROW_SEPARATION)
    line.add_theme_constant_override("v_separation", HudComposeVocab.CREW_ROW_SEPARATION)
    var stepper := HBoxContainer.new()
    stepper.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    HudWidgets.add_stepper_controls(stepper, count, plus_enabled, on_change)
    line.add_child(stepper)
    if bool(model.get("known", false)):
        var targets := HBoxContainer.new()
        # The pills are shorter than the stepper's boxed buttons, so they centre against it rather
        # than hanging off the flow row's top edge.
        targets.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        line.add_child(targets)
        _register_live(hosts, targets, model, count,
            func(host: Container, live: Dictionary, workers: int) -> void:
                host.add_child(HudWidgets.build_crew_targets(live, workers, on_pick)))
    block.add_child(line)
    parent.add_child(block)

## **THE READOUT** (§7.1, §7.2, §7.7) — the sheet's bottom half as ONE bounded box with three
## deliberately different registers, loudest first because that is the reading order:
##
##   a. THE YIELDS — what this crew, at this floor, brings home. The answer, and therefore the largest
##      type on the sheet. `yields_at(floor, crew)` recomposes it, which is what lets it follow a drag.
##   b. THE VERDICT — which of the crew and the floor is binding, with its severity dot.
##   c. THE ASIDE — the idle-crew note and the floor's own teaching line, under a dashed rule at the
##      quietest size on the sheet. The teaching line used to stand between the chart and the stepper
##      as a two-line paragraph, which made the panel's least urgent information one of its loudest
##      elements.
##
## No box at all when there is nothing to put in it — a source with no floor axis AND no priceable
## take has no readout, rather than an empty well.
##
## **EVERY REGISTER IN THIS BOX IS LIVE**, including the aside's locked-account line. Anything whose
## value — or whose PRESENCE — depends on the floor belongs in the live set: the lock's sentence
## explains a `—` in the register above it, and raising the floor takes the fodder row away, so a
## sentence resolved once before the render would outlive the mark it answers. The row and its
## explanation cannot disagree in either direction, and what guarantees that is NOT a single call —
## this function calls `yields_at` three times per refresh (the emptiness probe, the yields host, the
## aside). It is that the yield models are PURE and every one of those calls passes IDENTICAL
## arguments, which `_live_floor` / `_live_reaches` enforce by having one definition apiece.
func _mount_readout(parent: VBoxContainer, hosts: Array, model: Dictionary, workers: int,
        yields_at: Callable, labor_kind: String) -> void:
    var known := bool(model.get("known", false))
    var floor_value := float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))
    if not known and (yields_at.call(floor_value, workers, false) as Dictionary).is_empty():
        return
    var column := HudWidgets.build_readout_box(parent)
    var yields_host := VBoxContainer.new()
    yields_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    column.add_child(yields_host)
    # **THE `after` READING IS GATED ON THE SAME WALK THE VERDICT READS**, not on a closed form beside
    # it. `reached_turn` is what the sentence one line down says out loud ("Reaches the floor in 3
    # turns"), so a row promising a holding rate under a verdict saying the crew never gets there is
    # not possible. A source with no chart has no walk and therefore no holding state to promise.
    _register_live(hosts, yields_host, model, workers,
        func(host: Container, live: Dictionary, crew: int) -> void:
            _fill_yields_host(host, yields_at.call(
                _live_floor(live), crew, _live_reaches(live)), labor_kind))
    if known:
        var verdict_host := VBoxContainer.new()
        verdict_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.add_child(verdict_host)
        _register_live(hosts, verdict_host, model, workers,
            func(host: Container, live: Dictionary, _crew: int) -> void:
                host.add_child(HudWidgets.build_verdict_line(live.get("verdict", {}))))
    var aside_host := VBoxContainer.new()
    aside_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    column.add_child(aside_host)
    _register_live(hosts, aside_host, model, workers,
        func(host: Container, live: Dictionary, crew: int) -> void:
            var lines: Array[Dictionary] = []
            # THE LOCKED-ACCOUNT REASON LEADS, because it is the only aside line answering a mark the
            # player is already looking at. Its own meta: the two lines below it move with the floor,
            # so an assertion on "the aside changed" says nothing about this sentence.
            #
            # **READ OFF THE SAME `yields_at` ANSWER THE ROW ABOVE IS BUILT FROM, at this floor and
            # this crew.** Its PRESENCE is floor-dependent — raise the floor (or step the crew to 0)
            # and the fodder take goes to nothing, the row leaves, and a sentence resolved once before
            # the render would go on explaining a `—` no longer on screen. This is its OWN `yields_at`
            # call, not the row's — what makes it impossible for the two to disagree is that the yield
            # models are PURE and both calls pass the same arguments, which is why the floor and the
            # reaches flag are spelled `_live_floor` / `_live_reaches` and nowhere else.
            # The hunt web needs no branch here: its model carries no such key, so this reads `""`.
            var locked_reason := String((yields_at.call(
                _live_floor(live), crew, _live_reaches(live)) as Dictionary).get(
                    YIELD_MODEL_LOCKED_REASON, ""))
            if locked_reason != "":
                lines.append(HudWidgets.readout_aside_line(locked_reason, HudStyle.INK_FAINT,
                    HudWidgets.READOUT_LOCKED_ACCOUNT_META))
            # The floor's own teaching line, whatever zone it is in. A zone with nothing to say
            # answers `""` in `FLOOR_ZONE_HINTS` and `build_readout_aside` drops an empty line, so
            # there is no zone to test for here.
            lines.append(HudWidgets.readout_aside_line(HudFormat.floor_hint(
                _live_floor(live), labor_kind)))
            # **THE TEACHING RATE, and the one aside line that can be CYAN.** It states what
            # `learn_multiplier` actually buys — the chart's gradient rail only gestures at it — so
            # it wears `SIGNAL` while the crew is genuinely earning it, and the aside's own faint ink
            # when it is naming one of the two ends where nothing is learned. An EMPTY note means
            # this rung teaches nothing at all, which is a reason to render no line, not a blank one.
            var teaching: Dictionary = live.get("teaching_note", {})
            var teaching_text := String(teaching.get("text", ""))
            if teaching_text != "":
                lines.append(HudWidgets.readout_aside_line(teaching_text,
                    HudStyle.SIGNAL if bool(teaching.get("teaching", false))
                        else HudStyle.INK_FAINT, HudWidgets.READOUT_TEACHING_META))
            host.add_child(HudWidgets.build_readout_aside(lines)))

## The floor a live refresh is reading at, and whether its walk REACHES that floor — the two arguments
## every `yields_at` call in the readout is made with. One definition apiece, because the yields row and
## the aside's locked-account line must be composed at the SAME point on the dial: the row shows the
## mark, the line explains it, and a second spelling of either argument is all it would take for one to
## outlive the other.
func _live_floor(live: Dictionary) -> float:
    return float(live.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))

func _live_reaches(live: Dictionary) -> bool:
    return int(live.get("reached_turn", SourceForecast.PROJECTION_REACHED_NONE)) \
        != SourceForecast.PROJECTION_REACHED_NONE

## One yields model into the readout's first register. **The overdraw state moves the NUMBER, not just
## a suffix**: the row is the loudest thing in the box, so a take the source cannot pay forever has to
## read amber where the player is already looking. The waste note is always amber, even under a green
## take — a kill the crew could not carry is its own concern.
func _fill_yields_host(host: Container, model: Dictionary, labor_kind: String) -> void:
    if model.is_empty():
        return
    var overdraws := bool(model[YIELD_MODEL_OVERDRAW])
    var note := HudComposeVocab.OVERHUNT_FLAG + " " + String(
        HudComposeVocab.LOCAL_OVERDRAW_NOTES.get(labor_kind, "")) if overdraws         else SourceForecast.YIELD_RENEWABLE_NOTE
    host.add_child(HudWidgets.build_yields_row(
        model[YIELD_MODEL_ROWS],
        HudStyle.WARN if overdraws else HudStyle.INK,
        note,
        HudStyle.WARN if overdraws else HudStyle.HEALTHY,
        String(model[YIELD_MODEL_WASTE])))

## **THE EXPEDITION'S READOUT — the same box, the same three registers, a different question.** The
## branch used to answer with one wrapped bbcode sentence carrying five facts (the animals, the
## turns, the split, the food, the trade and the waste), beside a local sheet that laid the same
## kinds of fact out in a bounded well. Two sheets on one panel, reading nothing alike.
##
## What must NOT carry over is the local readout's PER-TURN framing: the header
## (`EXPEDITION_TRIP_ROW_HEADER`), the absent `now → after` on every row, and a verdict about the
## trip's length rather than about which of the crew and the floor binds — all three because a raid
## is one bounded errand, not a rate a resident crew settles into.
##
## Only a DELIVERING trip reaches here (`SourceForecast.hunt_trip_delivers`); the refused states keep
## their sentence, an empty box being worse than the line it replaced.
func _mount_trip_readout(parent: VBoxContainer, trip: Dictionary, quarry: String,
        floor_value: float) -> void:
    var column := HudWidgets.build_readout_box(parent)
    # The waste rides the yields row's own `waste` slot, exactly as the local hunt's does — a kill the
    # party could not haul is the animal web's concern on both branches, and it is amber either way.
    var waste_pct := float(trip.get("waste_pct", 0.0))
    column.add_child(HudWidgets.build_yields_row(
        _trip_yield_rows(trip, quarry),
        HudStyle.INK,
        "",
        HudStyle.HEALTHY,
        SourceForecast.HUNT_WASTE_NOTE_FORMAT % int(round(waste_pct * 100.0)) \
            if waste_pct > 0.0 else "",
        SourceForecast.EXPEDITION_TRIP_ROW_HEADER))
    column.add_child(HudWidgets.build_verdict_line(SourceForecast.hunt_trip_verdict(trip)))
    # THE ASIDE IS THE FLOOR HINT AND NOTHING ELSE. The local readout's other line — the live teaching
    # rate — has no counterpart here: an expedition accrues no husbandry (the gap
    # `FLOOR_LEARNING_HINT_EXPEDITION` already names in the learning zone), so a teaching line would
    # quote a multiplier this party never earns. A zone with nothing to say renders no aside at all,
    # rather than a dashed rule over empty space.
    # The COMPOSED floor, not the estimate row's nearest sample: the hint explains the preset the
    # player is holding, and the sampling is a fact about the forecast table rather than about them.
    var hint := HudFormat.floor_hint(floor_value, SourceForecast.LABOR_KIND_HUNT, true)
    if hint != "":
        column.add_child(HudWidgets.build_readout_aside(
            [HudWidgets.readout_aside_line(hint)]))

## The trip's payload as yields rows: the ANIMALS the party brings back, then whatever accounts those
## bodies pay.
##
## **THE ANIMAL COUNT LEADS, IN THE LOCAL HUNT ROW'S OWN IDIOM** — its `YIELD_ROW_NUMBER` /
## `YIELD_ROW_UNIT` overrides, the quarry as the unit and `YIELD_ACCOUNT_NONE` as the account,
## because a body is not an account. It borrows the `≈` FACE vocabulary and deliberately not the
## `/turn` UNIT one: this is a whole-trip count, and the header above already says so.
##
## The food and trade rows go through `SourceForecast.yield_rows`, so the render-only-where-the-
## vector-pays rule keeps one definition — a wolf raid states pelts and no `0 food`, an edible quarry
## with no trade states food alone. `YIELD_ACCOUNT_NONE` as the zero account means NO row is
## synthesised when both are empty; that state cannot arrive here anyway (it is `empty`, and the
## caller took the sentence branch), so a fabricated zero would be a reading of nothing.
##
## No `after` on any row: a trip has no holding state to arrow toward.
func _trip_yield_rows(trip: Dictionary, quarry: String) -> Array[Dictionary]:
    var animals := int(trip.get("animals", 0))
    var rows: Array[Dictionary] = [{
        SourceForecast.YIELD_ROW_ACCOUNT: SourceForecast.YIELD_ACCOUNT_NONE,
        SourceForecast.YIELD_ROW_VALUE: float(animals),
        HudWidgets.YIELD_ROW_NUMBER: HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % animals,
        HudWidgets.YIELD_ROW_UNIT: quarry,
    }]
    rows.append_array(SourceForecast.yield_rows(
        float(trip.get("food", 0.0)), float(trip.get("trade", 0.0)), 0.0,
        SourceForecast.YIELD_ACCOUNT_NONE))
    return rows

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
    # THE SECOND AXIS's standing value (issue #442) — what the band is already BUILDING here. It seeds
    # the improvement control so a herd mid-Tame opens with its box checked rather than looking
    # untouched, and it is what the commit compares against to decide whether a verb needs sending.
    var standing_improvement := _band_labor.improvement_for_hunt(band, herd_id)
    if source_changed:
        var staffed := _band_labor.workers_for_hunt(band, herd_id)
        _compose.seed_hunt(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.floor_for_hunt(band, herd_id), standing_improvement)
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
    # **THE KIT, RESOLVED HERE AND MOUNTED UNDER THE CREW ROW.** Its selection decides whether the
    # sim's `huntTripEstimates` table applies to this raid at all, and every reading below is priced
    # against that answer — so the resolve leads and the ROW lands beside the crew it describes.
    var kits := _band_labor.kits()
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_HUNT, default_kit,
        _compose.hunt_kit_id())
    _compose.set_hunt_kit_id(kit_id)
    # **THE HONESTY GATE, EXPEDITION BRANCH ONLY.** A LOCAL hunt is priced from the herd's own
    # per-biomass vector and the band's ceilings — no estimate table, so no kit mismatch to have. The
    # raid's every figure is a lookup into `huntTripEstimates`, which is quoted for ONE kit; where the
    # selection differs the sheet must not present that table as the answer. Compare the ids — never
    # assume the default is selected.
    var trip_quoted := not is_expedition or KitRoster.estimates_apply_to(herd,
        KitRoster.HERD_TRIP_ESTIMATES_KIT_KEY, default_kit, kit_id)
    # **THE HARVEST ROW IS THE SAME CONTROL ON BOTH BRANCHES** — three floor presets plus the slider
    # between them, since a floor is a number and there is no per-branch option list left to differ
    # about. Corral being local-only is still true: it is an IMPROVEMENT, and the improvement control
    # is simply not built on the expedition branch, because a detached party builds no pen.
    # Pre-commit forecast — LOCAL hunt only. An expedition travels for several turns and accumulates
    # toward a carry cap, so the herd's per-turn take ceiling is NOT the bound on its party size;
    # forecasting a per-turn yield for it would be a lie. On a local hunt the ceiling caps the
    # stepper (no over-assigning) and drives the live expected-yield row; both recompute here on
    # every stepper/policy change, since both re-render these controls.
    # THE COMPOSED IMPROVEMENT — the second axis, LOCAL hunt only (a detached party builds no pen).
    # The deal it states rides the SELECTED stance, which is the whole point of the split: a Deplete
    # builder's dip is a fraction of Deplete's larger ceiling, and it defeats itself through the
    # ecology (the meter accrues only while the herd is Thriving) rather than through a gate.
    # RESOLVED BEFORE THE FORECAST, because the forecast now takes it: while a build runs the sim's
    # ceiling is `stance × <rung>BuildFraction`, so a stance-only forecast caps the stepper on a
    # ceiling this crew will not be paid.
    # A verb this herd has already built is RETIRED rather than carried — the animal twin of the
    # forage sheet's rule, and for the same reason: `seed_hunt` runs only on a source change, so a
    # composition outlives its build and would keep dipping the crew for a Tame that finished.
    var composed_improvement := SourceForecast.IMPROVEMENT_NONE if is_expedition \
        else SourceForecast.live_improvement(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            _compose.hunt_improvement())
    if not is_expedition and composed_improvement != _compose.hunt_improvement():
        _compose.set_hunt_improvement(composed_improvement)
    var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_floor(), composed_improvement)
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
    # **THE DEMAND-SIDE CAP IS A READING OF THE TABLE TOO**, so under a kit mismatch the party falls
    # back to supply alone: with no table for this kit the payload's plateau is unknown, and clamping
    # to another kit's plateau would refuse a party this one may well need.
    var capped := {"cap": assignable, "note": ""}
    if is_expedition and trip_quoted:
        capped = SourceForecast.expedition_useful_cap(band, herd, _compose.hunt_floor(), assignable)
    elif not is_expedition:
        capped = _forecast_worker_cap(forecast, assignable, SourceForecast.herd_crew_floor(
            herd, composed_improvement != SourceForecast.IMPROVEMENT_NONE))
    var cap := int(capped["cap"])
    # Auto-max on a FLOOR click — "give me everything this herd can spare at this floor": the
    # max-useful for that floor (clamped to idle below), which guarantees zero waste + the full rate.
    # Only ever set by a preset/slider click, never by a −/+ tick, so manual counts survive a rebuild.
    if _compose.consume_hunt_autofill():
        _compose.set_hunt_count(cap)
    _compose.clamp_hunt_count(cap)
    # A managed herd's local crew are HERDERS/keepers (workersNeeded scales with the herd), not a hunt
    # party — so a pen needing several keepers doesn't read as a hunt-party bug (fix #6).
    var crew_label := HudComposeVocab.HERD_CREW_LABEL \
        if SourceForecast.is_managed_hunt_source(herd, composed_improvement) \
        else HudComposeVocab.HUNT_CREW_LABEL
    # Per-PRESET takes under both pickers so all three (forage / local hunt / expedition) wear the same
    # "up to X/turn" button metric, DESCENDING as the floor rises (take everything > best harvest >
    # learn from it). Worker-independent on both branches (the expedition's is the max over party sizes
    # of delivered / trip_turns, so it never changes as the Party stepper steps).
    # **THE PRESET METRICS GO WITH THE TABLE THEY COME FROM.** `expedition_policy_takes` is a reading
    # of `huntTripEstimates`, so under a kit mismatch it would put a figure priced at another kit on a
    # sheet whose own note says none are quoted. `{}` is the picker's supported degrade (a herd the
    # wire does not describe), so the rungs render bare rather than wrong.
    var floor_takes := {}
    if is_expedition:
        if trip_quoted:
            floor_takes = SourceForecast.expedition_policy_takes(band, herd,
                _band_labor.grid_width(), _band_labor.wrap_horizontal())
    else:
        floor_takes = _hunt_floor_takes(herd, composed_improvement)
    # **THE FLOOR FIRST, THEN THE CREW — the SAME vertical grammar the forage sheet reads in.** You
    # choose how hard to pull, then staff it. The cap is recomputed from the composed floor before the
    # stepper renders (a preset click re-renders and may auto-fill the crew) and the forecast below
    # reads the current crew.
    var on_floor_picked := func(floor: float) -> void:
        _compose.set_hunt_floor(floor)
        # Picking a floor auto-fills the crew to that floor's max-useful (consumed next rebuild).
        _compose.arm_hunt_autofill()
        _build_herd_assign_controls(_live_herd(herd_id, herd), target)
    target.add_child(HudWidgets.build_floor_picker(
        on_floor_picked, _compose.hunt_floor(), floor_takes))
    # **THE CHART — the stock and the floor in ONE picture, and the floor is the panel's primary
    # control** (`docs/plan_harvest_floor.md` §7.3). It is the same setter the presets use, so a
    # dragged value and a clicked preset are one state — which is what lets the picker honestly show
    # NO preset selected between two of them. It replaced the plain slider slice 4a shipped as a
    # placeholder.
    #
    # **IT RENDERS ON THE EXPEDITION BRANCH TOO NOW** (`docs/plan_hunt_through_combat.md` §5.2), and
    # the note that used to stand here saying it must not was written before the party had a stop of
    # its own. The curve IS one a raid follows: a party's per-turn take is the same
    # `min(room, carry, engagement)` a resident crew's is, so the drawdown it draws is the raid's. What
    # the picture cannot show is where the trip ENDS, and the fill target plus the bound clause in the
    # readout are exactly what now say that — the herd-side half and the party-side half of one
    # decision, which is why the graph had to come back rather than the target arriving alone.
    var chart_model: Dictionary = {}
    var live_hosts: Array[Dictionary] = []
    # **WHETHER THE FACTION ALREADY KNOWS WHAT THIS HERD TEACHES**, bound once and captured by the
    # live-drag closure below: a drag is a gesture inside one snapshot, and a snapshot that moves the
    # knowledge rebuilds the sheet outright. The aside's teaching line is the only reading that wants
    # it — see `SourceForecast.teaching_note`.
    var lesson_known := SourceForecast.rung_lesson_known(SourceForecast.SOURCE_KIND_HERD, herd,
        HudComposeVocab.BARE_FORECAST_PREFIX, _player_knowledge())
    chart_model = SourceForecast.floor_chart_model(herd, SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_floor(), _compose.hunt_count(),
        composed_improvement, crew_label.to_lower(), lesson_known)
    if bool(chart_model.get("known", false)):
        target.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _compose.set_hunt_floor(floor)
                if committed:
                    _compose.arm_hunt_autofill()
                    _build_herd_assign_controls(_live_herd(herd_id, herd), target)
                else:
                    # A LIVE drag must not rebuild these controls — the rebuild frees the chart
                    # and the drag dies with it. Refill only the readings that follow the floor.
                    # **On the EXPEDITION branch `live_hosts` is empty and that is deliberate**: the
                    # raid's numbers are a lookup into a table SAMPLED at five floors, so most of a
                    # drag moves nothing, and the release rebuilds the sheet against the sample the
                    # player landed on. The drag itself still survives, which is the contract.
                    _refresh_floor_live(live_hosts, SourceForecast.floor_chart_model(
                        _live_herd(herd_id, herd), SourceForecast.SOURCE_KIND_HERD,
                        HudComposeVocab.BARE_FORECAST_PREFIX, floor, _compose.hunt_count(),
                        composed_improvement, crew_label.to_lower(), lesson_known),
                        _compose.hunt_count())))
    # The expedition branch spends this slot on the distance refusal — it is that branch's answer to
    # "why is this a party rather than a hunt?" — and the local branch on what the floor means for the
    # herd. **ONE hint table serves both webs and both branches now** (`HudFormat.floor_hint`): a
    # floor's meaning is its position relative to the food peak, which is the same fact for a patch and
    # a herd. The two things that genuinely differ are composed in, not tabulated: what stripping
    # COSTS (a patch reseeds; a herd is gone for good) and the fact that a detached party learns no
    # craft, so the above-peak trade is not one an expedition can make.
    # **THE TRIP, RESOLVED BEFORE THE CREW ROW** — the same `_compose.hunt_count()` the stepper below
    # renders (the cap clamp is already done above, and nothing between here and the button moves it),
    # so the readout at the bottom and the floor hint at the top branch on ONE lookup rather than two.
    var trip: Dictionary = {}
    # The fill target's axis, composed from the UNTARGETED raid so it does not move under the handle.
    # Empty on the local branch: a resident crew works a source turn after turn and has no trip to end.
    var fill_target_model: Dictionary = {}
    if is_expedition:
        target.add_child(HudWidgets.alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
    if is_expedition and trip_quoted:
        # **THE TARGET IS FOLDED BACK ONTO ITS AXIS BEFORE THE TRIP IS LOOKED UP.** The axis is a
        # function of the party and the floor, both of which the player has just been moving, so a
        # target held from a bigger party could otherwise ask for more animals than this raid brings
        # home — which `raid_load` answers by handing the pack back, i.e. a lever that silently does
        # nothing. `raid_fill_target_model` returns the clamped value; writing it straight back is what
        # makes the control, the readout and the launch payload one number.
        fill_target_model = SourceForecast.raid_fill_target_model(band, herd, _compose.hunt_floor(),
            _compose.hunt_count(), _band_labor.grid_width(), _band_labor.wrap_horizontal(),
            _compose.hunt_fill_target())
        _compose.set_hunt_fill_target(int(fill_target_model.get(
            "target", SourceForecast.NO_FILL_TARGET)))
        trip = SourceForecast.hunt_trip_forecast(band, herd, _compose.hunt_floor(),
            _compose.hunt_count(), _band_labor.grid_width(), _band_labor.wrap_horizontal(),
            _compose.hunt_fill_target())
        # **THE FLOOR HINT TRAVELS INTO THE TRIP READOUT'S ASIDE**, where the local sheet keeps its
        # own — the two branches now read alike. It stays HERE only for the raids that get no readout
        # box (no estimate, a denial quarry, a herd with nothing above the floor): those state one
        # sentence in place of the box, so the hint has nowhere else to go. Empty hints render no
        # label at all; a zone with nothing to say must not leave a blank line behind it.
        if not SourceForecast.hunt_trip_delivers(trip):
            var refused_hint := HudFormat.floor_hint(
                _compose.hunt_floor(), SourceForecast.LABOR_KIND_HUNT, true)
            if refused_hint != "":
                target.add_child(HudWidgets.alloc_hint_label(refused_hint))
    # THE CREW, on ONE line with both targets (§7.6) — with its cap note, which explains THIS stepper's
    # dead `+` and therefore travels with it. Clicking a target staffs it, clamped to the same cap the
    # `+` obeys: a target is a shortcut to a count, never a way past the ceiling.
    #
    # **THE CREW TARGETS STAY OFF THE EXPEDITION BRANCH even now that it has a chart.** They answer
    # *clear it now* and *hold it after*, and the second is a promise about a crew that STAYS — a
    # detached party leaves. So the crew row is handed an empty model there (which is also what drops
    # the build-dip note, correctly: a party builds nothing).
    _mount_crew_row(target, live_hosts,
        HudComposeVocab.COMPOSE_FIELD_PARTY if is_expedition else crew_label,
        _compose.hunt_count(), _compose.hunt_count() < cap,
        func(n: int) -> void:
            _compose.set_hunt_count(clampi(n, 0, cap))
            _build_herd_assign_controls(_live_herd(herd_id, herd), target),
        {} if is_expedition else chart_model,
        func(count: int) -> void:
            _compose.set_hunt_count(clampi(count, 0, cap))
            _build_herd_assign_controls(_live_herd(herd_id, herd), target))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # **THE KIT ROW, directly under the crew stepper and above every forecast** — a kit describes the
    # crew, and it moves the fight (the attack tier) and the haul (the carry tier) alike. Both branches
    # get it: a local hunt sends `assign_labor … kit <id>` exactly as a raid sends
    # `send_hunt_expedition … kit <id>`.
    _mount_kit_row(target, kits, KitRoster.JOB_HUNT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_hunt_kit_id(picked)
            _build_herd_assign_controls(_live_herd(herd_id, herd), target))
    # **THE FIGHT, STATED BEFORE THE PARTY LEAVES** (`docs/plan_hunt_through_combat.md` §2.1 / §6.5),
    # directly under the crew that will fight it — both lines answer "is this crew the right size, and
    # can it win at all", which is what the stepper one row up has just posed.
    #
    # **THE ENGAGEMENT STAGE IS THE GATE ON BOTH, and that is what keeps a PEN and the whole PLANT web
    # byte-identical**: both publish `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked and a berry
    # does not fight back — so neither line renders and neither sheet moves. The dip rides the reach
    # exactly as it rides every crew target (hands gentling a herd are hands not stalking it), so a
    # Tame in progress raises the hunters-per-animal figure honestly rather than quoting a wild reach.
    var engage_rate := float(herd.get(
        HudComposeVocab.BARE_FORECAST_PREFIX + SourceForecast.FORECAST_ENGAGE_RATE_KEY,
        SourceForecast.NO_ENGAGEMENT_STAGE))
    var engage_dip := SourceForecast.build_dip(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
        composed_improvement)
    if SourceForecast.has_engagement_stage(engage_rate, engage_dip):
        var quarry := _herd_label_for_id(herd_id)
        # HOW MANY HUNTERS ONE ANIMAL TAKES — `1 / engageRate`, off the SAME `engagement_per_worker`
        # every crew target divides by. "Twenty hunters to take a mammoth" is a number a player can
        # size a party against; `0.05` is not.
        var reach_label := HudWidgets.alloc_hint_label(
            SourceForecast.hunters_per_animal_face(engage_rate, engage_dip, quarry))
        reach_label.set_meta(HudWidgets.HUNTERS_PER_ANIMAL_META, true)
        target.add_child(reach_label)
        # …AND WHETHER THEY CAN HURT IT AT ALL. A sub-gate party kills nothing at any headcount and
        # still takes casualties, which reads as a bug unexplained. The refusal is DANGER-inked; above
        # the gate the same line states the effort instead, so the sheet always says what the fight
        # costs rather than only when it is hopeless.
        #
        # **IT IS ASKED AT THE SELECTED KIT'S EFFECTIVE ATTACK, NOT THE BAND'S DEFAULT-KIT TIER.** The
        # picker one row up decides what these hunters carry, so a gate quoting the band's default kit
        # would refuse — or clear — a fight the composed party is not having.
        var gate_tiers := KitRoster.effective_tiers(kits, KitRoster.kit_by_id(kits, kit_id), band)
        var gate := SourceForecast.hunt_gate_model_at(float(gate_tiers["attack"]), herd, quarry)
        if bool(gate["stated"]):
            var gate_label := HudWidgets.forecast_label("[color=#%s]%s[/color]" % [
                HudStyle.DANGER_HEX if bool(gate["blocked"]) else HudStyle.INK_DIM_HEX,
                String(gate["text"])])
            gate_label.set_meta(HudWidgets.HUNT_GATE_META, bool(gate["blocked"]))
            target.add_child(gate_label)
    # **THE FILL TARGET, DIRECTLY UNDER THE PARTY IT IS PRICED BY** (§5.2). It reads *how long you will
    # wait*, and both terms of that — the animals and the turns they cost — are functions of the party
    # size one row up, so the two controls belong adjacent and in that order. Only a DELIVERING raid
    # gets one: a refused trip has no length to shorten.
    if is_expedition and bool(fill_target_model.get("available", false)):
        target.add_child(HudWidgets.build_fill_target_control(fill_target_model,
            func(new_target: int) -> void:
                _compose.set_hunt_fill_target(new_target)
                _build_herd_assign_controls(_live_herd(herd_id, herd), target)))
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
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    if is_expedition and not trip_quoted:
        # **THE KIT-MISMATCH RAID.** The table is not an answer for this party, so nothing derived from
        # it renders — no trip readout, no one-line verdict, no refusal. The combat gate two rows above
        # already stated what this kit can and cannot hurt (it is composed from wire terms and stays
        # honest at any tier), so what is added here is the sentence naming whose numbers were
        # withheld. The send stays live and plainly styled: the raid launches, we simply cannot quote
        # its length.
        target.add_child(HudWidgets.alloc_hint_label(KitRoster.estimates_quoted_note(kits, herd,
            KitRoster.HERD_TRIP_ESTIMATES_KIT_KEY, default_kit, kit_id,
            HudComposeVocab.KIT_TRIP_ESTIMATES_QUOTED_FORMAT)))
        SourceForecast.style_send_hunt_button(assign_btn, {}, "")
    elif is_expedition:
        # **THE TRIP READOUT** — the raid's answer in the SAME bounded box the local sheet uses, so a
        # player moving between the two branches reads one layout: the payload as a yields row, the
        # trip's length as the verdict, the floor's meaning as the aside. It re-renders with this
        # whole block on every stepper tick and policy click, which is what keeps it a live forecast
        # rather than a confirmation — and there is no chart on this branch, hence no drag to keep
        # alive, so it stays out of `_register_live` deliberately.
        # `trip`, NOT `forecast`: the outer `forecast` is the LOCAL hunt's per-turn ceiling inputs
        # (client arithmetic over the BAND flow ceiling). This one is the sim's forward-simulated TRIP
        # estimate — a pure table lookup, zero client arithmetic. The two must never be confused.
        if SourceForecast.hunt_trip_delivers(trip):
            _mount_trip_readout(target, trip, _herd_label_for_id(herd_id), _compose.hunt_floor())
        else:
            # A raid with nothing to lay out in rows — no estimate, a denial quarry, a herd at its
            # floor — keeps the ONE-LINE form, which is also the send-hunt banner's, so the two entry
            # points still cannot quote different refusals.
            var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip, _herd_label_for_id(herd_id))
            if forecast_line != "":
                target.add_child(HudWidgets.forecast_label(forecast_line))
        # The empty-raid refusal — computed ONCE and used for both the button tooltip and the reason
        # line, and identical to what the Band panel's dock sheet renders. It takes the TRIP as well as
        # the herd: whether the culprit is the herd's spent surplus or a party that cannot make the
        # kill comes off the sim's own `bound`, and the two remedies are opposites.
        var returns_empty := SourceForecast.hunt_trip_returns_empty(trip)
        var reason := SourceForecast.hunt_empty_refusal_reason(trip, herd) if returns_empty else ""
        SourceForecast.style_send_hunt_button(assign_btn, trip, reason)
        # The reason is spelled out beside the button too — a disabled control's tooltip is easy to miss.
        if returns_empty:
            target.add_child(HudWidgets.alloc_hint_label(reason))
    else:
        # The averaging-window disclaimer USED TO STAND HERE, as a wrapped body line under the hint: the
        # delivered rate is a long-run average of lumpy whole-animal delivery. It is a caveat on ONE
        # number, so it now rides the RUNG's tooltip beside the metric it qualifies (`_hunt_floor_takes`
        # fills the take pair's `note`) — the panel is where the hunt sheet could least afford a sentence
        # the forage sheet has no counterpart for. The window computation is unchanged.
        # **THE READOUT** — the LIVE per-turn take for the floor being composed (no carry cap on a
        # local hunt, so turns-to-fill is meaningless — the delivered rate is the number that decides
        # it), then the verdict (§7.1: which of the two independent statements is binding, the crew or
        # the floor), then the idle-crew note (§7.2 — reported, never acted on) and the teaching line.
        # The take is recomposed from the LIVE floor, so the numbers the player is dragging toward
        # move while the drag runs.
        _mount_readout(target, live_hosts, chart_model, _compose.hunt_count(),
            func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
                return _hunt_yield_model(band, herd, floor_value, crew,
                    composed_improvement, reaches),
            SourceForecast.LABOR_KIND_HUNT)
        # THE IMPROVEMENT ROW — the second axis, beneath the stance it multiplies. Nothing is offered on
        # an UNASSIGN, for the reason the forage sheet already records: what abandoning costs is stated
        # in the rung's own hint ("It must stay staffed or the herd goes wild again"), so a second
        # warning at the moment of unassigning states one fact twice.
        if not is_unassign:
            _build_improvement_control(SourceForecast.LABOR_KIND_HUNT, herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_floor(), composed_improvement,
                band,
                func(improvement: String) -> void:
                    _compose.set_hunt_improvement(improvement)
                    _build_herd_assign_controls(_live_herd(herd_id, herd), target),
                target)
        # A dead button is always explained (the `+` stepper's cap note is the precedent) — but only
        # when the cap note has not already said it, so the panel never states one fact twice.
        if is_noop and cap_note == "":
            target.add_child(HudWidgets.alloc_hint_label(
                String(HudComposeVocab.HUNT_NOOP_HINTS.get(crew_label, ""))))
        # THE VERB FOLLOWS THE CREW NOUN, off the SAME `crew_label` the stepper and the noop hint
        # above already read. It was hard-coded to `Hunt Here`, so an `ASSIGN HERDERS` sheet over a
        # `Herders` stepper committed with `Hunt Here` — reported from play. A penned or fully-tamed
        # herd is not hunted.
        assign_btn.text = HudComposeVocab.UNASSIGN_BUTTON if is_unassign \
            else String(HudComposeVocab.HUNT_ASSIGN_BUTTONS.get(crew_label,
                HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON))
        HudStyle.apply_button(assign_btn, "primary")
        assign_btn.disabled = is_noop
    if is_expedition:
        assign_btn.set_meta(HudWidgets.SEND_HUNT_CONFIRM_META, true)
        # A hunting expedition needs a positive party; a local hunt allows 0 (removes the assignment).
        # `SourceForecast.style_send_hunt_button` already disabled it when the raid returns empty (no surplus); a
        # positive party is the other precondition. (`or` — never clear a disable the style step set.)
        assign_btn.disabled = assign_btn.disabled or _compose.hunt_count() <= 0
        assign_btn.pressed.connect(func() -> void:
            if _compose.hunt_count() <= 0:
                return
            # **THE EMPTY-RAID GUARD IS ALSO A READING OF THE TABLE**, so it is skipped where the table
            # is not quoted for this kit: refusing a launch on another kit's projection would be the
            # same lie as quoting one, cast as a silent no-op.
            if trip_quoted and SourceForecast.hunt_trip_returns_empty(
                    SourceForecast.hunt_trip_forecast(band, herd, _compose.hunt_floor(), _compose.hunt_count(),
            _band_labor.grid_width(), _band_labor.wrap_horizontal(), _compose.hunt_fill_target())):
                return
            emit_signal("send_hunt_expedition_requested", {
                "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
                "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
                "party_workers": _compose.hunt_count(),
                "fauna_id": herd_id,
                "fauna_label": SourceForecast.herd_display_name(herd),
                # THE PARTY'S ORDERS: where the raid stops, as a fraction of the herd's capacity.
                # `send_hunt_expedition` takes it as its optional trailing token.
                "floor": _compose.hunt_floor(),
                # …and the party-side half of the same sentence (§5.2): the whole animals it waits
                # for. `NO_FILL_TARGET` = fill the pack, which is what the command sent before this
                # lever existed and what `Main` omits the token for.
                "fill_target": _compose.hunt_fill_target(),
                # The kit the party walks out with, and the job default `Main` omits the token for.
                "kit_id": kit_id,
                "default_kit_id": default_kit,
            })
            # Committing is the end of the compose act — return to the read state (§15).
            close_compose_sheet())
    else:
        assign_btn.pressed.connect(func() -> void:
            # ORDER IS LOAD-BEARING: `assign_labor` first, the improvement verb second. The sim's
            # improvement commands act on the bands ALREADY WORKING the source, so a verb sent to an
            # unstaffed herd is rejected outright — the crew has to land first.
            _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, _compose.hunt_count(),
                herd_x, herd_y, herd_id, _compose.hunt_floor(), "", composed_improvement, kit_id)
            _emit_improvement(band, SourceForecast.LABOR_KIND_HUNT, composed_improvement,
                standing_improvement, herd_x, herd_y, herd_id)
            close_compose_sheet())
    target.add_child(assign_btn)

## Mount the kit row where a sheet wants it — a no-op when the roster offers this job no kit at all,
## so a sheet rendered before the first snapshot (or against a world whose roster does not cover the
## verb) is byte-identical to what it was before the picker existed. The Band panel's dock sheets keep
## the identical helper; the two controllers share no base, and one Callable to reach the other's copy
## would be an injection that buys nothing.
func _mount_kit_row(target: VBoxContainer, kits: Array, job: String, kit_id: String,
        default_kit: String, band: Dictionary, on_pick: Callable) -> void:
    var row := KitRoster.build_kit_row(kits, job, kit_id, default_kit, band, on_pick)
    if row != null:
        target.add_child(row)







## Each FLOOR PRESET's per-turn take on this forage patch — the ceiling at that floor, composed by the
## shared `SourceForecast.forecast_inputs` (per turn at output 1.0, exactly as the hunt twin), for the
## FORAGE picker's preset readout. The plant twin of `_hunt_floor_takes`, so both pickers wear the
## same button metric. A patch the wire does not describe is skipped.
##
## **ALL THREE ACCOUNTS (#426), and the ZERO now lands in the right one (§7.7).** This once handed the
## shared joiner an explicit `0.0` for trade, on the standing claim that the plant web projected no
## trade rate — so a flax patch, which pays trade and no food, rendered `0.00 food` at every floor and
## read exactly like the worthless-source lie #337 removed from the hunt picker. Each account now
## comes off the patch's own per-biomass vector and renders only when non-zero, and when the take is
## empty in ALL of them the surviving zero is the account the patch actually pays.
##
## **The Cultivate/Sow PAYOFF faces left with the build verbs** (issue #442): they were a second loop
## here, wearing the crop-substituted payoff because a build verb was a rung of this picker. The
## improvement control states the same terms now, against the same crop, through the same
## `_crop_payoff_terms`.
##
## **A LOCKED FODDER ACCOUNT IS QUOTED IN NO PRESET TOOLTIP AT ALL** (issue #485) — not a dash, not a
## zero, no clause. A tooltip is one flat string with nowhere to hang a reason, and this sheet already
## states the lock ONCE, in the register built to explain it: the muted `—` row that keeps its FODDER
## unit, plus the aside sentence directly below these very buttons. So dropping the clause hides
## nothing that is not still on screen — the account's EXISTENCE is stated by that surviving unit —
## whereas quoting the ceiling states a quantity the sim will refuse, one control above the readout
## refusing it. That self-contradicting sheet is the defect this scoping fixes.
##
## **The lock is resolved through `_wild_fodder_lock`, the same call the yields row is muted by.** Two
## predicates over one gate is how these two surfaces started disagreeing in the first place.
##
## **Scoped to the FORAGE presets, and to the WILD take.** The hunt picker has no fodder account to
## drop; and neither the crop picker's rows nor the improvement control's payoff faces (`Hay Grass 30%
## · 1.80 hay`, `→ … fodder`) are touched by the lock — they quote what COMMITTING to the crop would
## pay, and a committed patch's hay is credited unconditionally, committing being the bid.
func _forage_floor_takes(tile_info: Dictionary) -> Dictionary:
    var takes := {}
    var zero_account := SourceForecast.zero_account_of(
        tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX)
    var locked := _wild_fodder_lock(tile_info) != ""
    # **THE ACCOUNT'S OWN ZERO GOES WITH ITS CEILING.** On a hay-ONLY patch fodder is the
    # `zero_account`, so merely zeroing the term would print `up to +0.00 fodder/turn` on every preset
    # — the refused quantity again, now at a number the ground itself contradicts. `YIELD_ACCOUNT_NONE`
    # renders no line, leaving the preset its name-only tooltip.
    if locked and zero_account == SourceForecast.YIELD_ACCOUNT_FODDER:
        zero_account = SourceForecast.YIELD_ACCOUNT_NONE
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.floor_for_preset(preset))
        if not bool(forecast["known"]):
            continue
        takes[preset] = SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]), float(forecast["ceiling_trade"]),
            0.0 if locked else float(forecast["ceiling_fodder"]), zero_account)
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
        HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.FLOOR_FOOD_PEAK, rung)
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
    # THE SECOND AXIS's standing value (issue #442) — what the band is already BUILDING on this patch.
    # Unlike the stance it needs no staffing test: `improvement_for_forage` reads the assignment's own
    # field and answers "" when there is no assignment at all.
    var standing_improvement := _band_labor.improvement_for_forage(band, x, y)
    if source_changed:
        # `seed_forage` also clears the crop: a crop pick belongs to the PATCH it was made on, and a
        # new tile has a different basket.
        var staffed := _band_labor.workers_for_forage(band, x, y)
        _compose.seed_forage(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.floor_for_forage(band, x, y), standing_improvement)
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
    # The forage ESCAPEMENT FLOOR — the same control the hunt sheet builds (three intent presets plus
    # the slider between them), default the food peak. Persisted across re-renders; re-seeded from the
    # band's standing assignment when the tile changes. There is nothing to validate against a list any
    # more: `ComposeState` clamps the number on the way in, which is the whole of what a floor can be
    # wrong about.
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
    # **AND A VERB THIS PATCH HAS ALREADY BUILT IS RETIRED HERE, NOT CARRIED.** `seed_forage` runs
    # only when the SOURCE changes, so a composition outlives the build it named: the turn a Cultivate
    # completes the sim clears the assignment's `improvement`, the improvement control below drops to
    # its DONE label — and the composed verb sat on unread, dipping every crew term to 25% and
    # re-issuing itself on the next commit. `SourceForecast.live_improvement` is the same
    # `improvement_is_done` test that control already makes, so the numbers and the label can no
    # longer say different things about the same rung.
    var composed_improvement := SourceForecast.live_improvement(tile_info,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_improvement())
    if composed_improvement != _compose.forage_improvement():
        _compose.set_forage_improvement(composed_improvement)
    # THE CREW NOUN, resolved ONCE for the whole sheet — `Foragers` on wild ground, `Tenders` on a
    # Tended Patch or a Field. Every surface below reads THIS local (section label, verdict/idle
    # sentences, commit button, dead-button hint), and the header two rows up reads the same
    # `HudFormat.plant_crew_label`, so the eyebrow and the stepper cannot name two different crews on
    # one sheet. Note it takes `tile_info`, NOT `composed_improvement`: a build in flight keeps the
    # wild noun (see that function).
    var crew_label := HudFormat.plant_crew_label(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX)
    var crop_rung := composed_improvement if composed_improvement != SourceForecast.IMPROVEMENT_NONE \
        else String(RungGates.next_rung_offered(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            composed_improvement, _player_knowledge(),
            HudComposeVocab.FORAGE_FORECAST_PREFIX).get("policy", ""))
    _compose.resolve_forage_species(func(current: String) -> String:
        return _resolve_crop_selection(basket, crop_rung, is_committed, current))
    # Per-preset per-turn takes on the buttons, so the forage picker wears the SAME metric the
    # local-hunt picker does. The two build verbs do not ride this picker at all — they wear their
    # payoff on the improvement control below (issue #442).
    # **THE CAP IS RESOLVED BEFORE THE CHART, and the order is load-bearing** — the chart, the two
    # crew targets and the verdict are all read against a CREW, and reading them against a count the
    # stepper below is about to clamp away made the panel state a verdict for a crew it then refused
    # to show (a full patch reading "already at the floor" beside a stepper the same pass had just
    # zeroed). The hunt sheet has always resolved its cap here; this is the forage twin of that order.
    # Pre-commit forecast: the patch's per-worker yield + the SELECTED stance's ceiling — DIPPED by
    # whatever this crew is building — cap the stepper at max-useful workers, so the player CAN'T
    # over-assign while composing. Both the stepper and the stance picker re-render these controls, so
    # the cap and the preview below recompute on every change (a Deplete/Eradicate ceiling is higher
    # than Sustain's, so switching stance moves the cap; ticking the improvement box moves it too).
    var forecast := SourceForecast.forecast_inputs(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_floor(), composed_improvement)
    # …and floored on the rung's OWN build crew, the plant twin of a managed herd's herding crew. The
    # dip and the cap otherwise fight: dividing the dipped ceiling collapses the count, so committing
    # to a 25-turn improvement would ask for fewer hands than gathering the same ground — and the sim,
    # which takes `max(build crew, take crew)`, would then report the row overstaffed at the very count
    # this sheet capped it to.
    var capped := _forecast_worker_cap(forecast, _band_labor.assignable_forage_workers(band, x, y),
        SourceForecast.plant_crew_floor(
            tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX, composed_improvement))
    var cap := int(capped["cap"])
    # Auto-max on stance select — "give me everything this patch sustains": jump to the max-useful for
    # the stance (clamped to available below). Only ever set by a stance click, never by a −/+ tick.
    if _compose.consume_forage_autofill():
        _compose.set_forage_count(cap)
    _compose.clamp_forage_count(cap)
    var forage_takes := _forage_floor_takes(tile_info)
    var on_floor_picked := func(floor: float) -> void:
        _compose.set_forage_floor(floor)
        # Picking a floor auto-fills the foragers to its max-useful (consumed next rebuild).
        _compose.arm_forage_autofill()
        _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target)
    target.add_child(HudWidgets.build_floor_picker(on_floor_picked, _compose.forage_floor(),
        forage_takes, HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS))
    # **THE CHART** — the plant twin of the hunt sheet's, and the same node in the same slot, because
    # a floor means the same thing on both webs. What differs is what the curve DOES: a patch's
    # sampled regrowth never goes negative (it reseeds from bare ground), so its projection at floor 0
    # bottoms out and climbs where a herd's crashes. That asymmetry comes off the wire, not from here.
    var live_hosts: Array[Dictionary] = []
    # The plant twin of the hunt sheet's binding, and captured by the drag closure for the same
    # reason: a lesson the faction has already learned is not taught again in the aside.
    var lesson_known := SourceForecast.rung_lesson_known(SourceForecast.SOURCE_KIND_FORAGE,
        tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX, _player_knowledge())
    var chart_model := SourceForecast.floor_chart_model(tile_info,
        SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        _compose.forage_floor(), _compose.forage_count(), composed_improvement,
        crew_label.to_lower(), lesson_known)
    if bool(chart_model.get("known", false)):
        target.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _compose.set_forage_floor(floor)
                if committed:
                    _compose.arm_forage_autofill()
                    _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target)
                else:
                    _refresh_floor_live(live_hosts, SourceForecast.floor_chart_model(
                        _live_tile_info(subject_key, tile_info), SourceForecast.SOURCE_KIND_FORAGE,
                        HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, _compose.forage_count(),
                        composed_improvement, crew_label.to_lower(),
                        lesson_known),
                        _compose.forage_count())))
    # THE CREW, on ONE line with both targets (§7.6) — each clamped to the same cap the `+` obeys, so
    # a target is a shortcut to a count and never a way past the ceiling. The floor's TEACHING LINE
    # used to stand here, between the chart and the stepper; it reads in the readout's aside now,
    # where the panel's quietest information belongs.
    _mount_crew_row(target, live_hosts, crew_label, _compose.forage_count(),
        _compose.forage_count() < cap,
        func(n: int) -> void:
            _compose.set_forage_count(clampi(n, 0, cap))
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
        chart_model,
        func(count: int) -> void:
            _compose.set_forage_count(clampi(count, 0, cap))
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target))
    var cap_note := String(capped["note"])
    if cap_note != "":
        target.add_child(HudWidgets.alloc_hint_label(cap_note))
    # **THE KIT ROW, directly under the crew stepper and above the readout** — the forage web's half of
    # the picker. There is no honesty gate here: the plant web has no estimate table quoted at one kit,
    # so the readout below is composed from the patch's own terms at any selection. What the kit DOES
    # move is the gatherer's carry tier, which the hint line under the picker states at this band's
    # real basket condition rather than at the roster's fresh number.
    var forage_kits := _band_labor.kits()
    var forage_default_kit := _band_labor.default_kit_id(KitRoster.JOB_FORAGE)
    var forage_kit_id := KitRoster.resolve_selection(forage_kits, KitRoster.JOB_FORAGE,
        forage_default_kit, _compose.forage_kit_id())
    _compose.set_forage_kit_id(forage_kit_id)
    _mount_kit_row(target, forage_kits, KitRoster.JOB_FORAGE, forage_kit_id, forage_default_kit, band,
        func(picked: String) -> void:
            _compose.set_forage_kit_id(picked)
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target))
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
    # **THE READOUT** — the take, the verdict, and the idle-crew note + teaching line, in one bounded
    # box at three registers (§7.1, §7.2). The take is recomposed from the LIVE floor, which is what
    # lets the numbers the player is dragging toward move while the drag runs.
    _mount_readout(target, live_hosts, chart_model, _compose.forage_count(),
        func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
            return _forage_yield_model(band, tile_info, floor_value, crew, composed_improvement,
                reaches),
        SourceForecast.LABOR_KIND_FORAGE)
    # THE IMPROVEMENT ROW — the second axis, beneath the stance it multiplies. Nothing is forecast for
    # an UNASSIGN: what abandoning costs is already on the card in the rung's own hint ("It must stay
    # staffed or it goes feral"), so a second warning here would state one fact twice.
    if not is_unassign:
        _build_improvement_control(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, _compose.forage_floor(), composed_improvement,
            band,
            func(improvement: String) -> void:
                _compose.set_forage_improvement(improvement)
                _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
            target,
            # THE PAYOFF TERMS FOLLOW THE CROP, on the OFFERED box and the RUNNING one alike — one
            # Callable, both states, so the two can never quote different crops (issue #419).
            func(rung: String) -> String:
                return _crop_payoff_terms(tile_info, basket, _compose.forage_species(), band, rung),
            # WHICH CROP this rung commits the patch to (flora roster S1), beneath the box
            # because it is part of the same decision. Re-resolved every render (the rung can
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
        target.add_child(HudWidgets.alloc_hint_label(
            String(HudComposeVocab.PLANT_NOOP_HINTS.get(crew_label, ""))))
    var assign_btn := Button.new()
    # The commit verb follows the crew noun the stepper above just asked for — `Forage` for foragers,
    # `Tend` for tenders — keyed off the ONE resolved label, exactly as the hunt web's noop hint is.
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    assign_btn.text = HudComposeVocab.UNASSIGN_BUTTON if is_unassign \
        else String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(crew_label, ""))
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range or is_noop
    assign_btn.pressed.connect(func() -> void:
        # ORDER IS LOAD-BEARING: `assign_labor` first (it carries the crop), the improvement verb
        # second. The sim's improvement commands act on the bands ALREADY WORKING the tile, so a verb
        # sent to an unworked patch is rejected outright — the crew has to land first.
        _emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, _compose.forage_count(), x, y, "",
            _compose.forage_floor(), _compose.forage_species(), composed_improvement, forage_kit_id)
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
    return DetailFormat.tile_is_gathering_site(tile_info) \
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
        if SourceForecast.is_managed_hunt_source(herd, _herd_improvement_axis(herd)) \
        else HudComposeVocab.HUNT_CREW_LABEL

## The IMPROVEMENT axis **for this herd** — the composed value only while the compose state is keyed
## to it, else the band's own standing build on it.
##
## `_hunt_improvement` is ONE slot shared by every herd, and neither `begin_hunt_source` nor
## `reset_hunt_source` clears it, so it survives a source change. Reading it blind names the crew after
## whichever herd was composed LAST: tick Corral on a pen-ready herd, select a wild one, and its header
## read `ASSIGN HERDERS` over the stepper the SAME render labelled `Hunters` — the header/stepper
## disagreement `_herd_crew_noun` was written to remove, with the sides swapped. The stale read is not
## confined to the sheet either: `build_herd_drawer_actions` names the drawer's `Assign … ▸` button from
## the same call, in the read state, with no sheet open at all.
##
## Guarding on the KEY rather than clearing the slot is what keeps a same-source re-open honest: a box
## the player just ticked is still on this herd's compose, so it must survive. It is also ordering-proof
## — `open_herd_compose` resolves the eyebrow BEFORE `_build_herd_assign_controls` re-seeds, and on a
## source change this answers with exactly what that re-seed is about to write (`improvement_for_hunt`
## on the resolved band, which is the band a source change defaults the picker to).
func _herd_improvement_axis(herd: Dictionary) -> String:
    var herd_id := String(herd.get("id", ""))
    if _compose.hunt_key() == herd_id:
        return _compose.hunt_improvement()
    return _band_labor.improvement_for_hunt(_resolve_assign_band(), herd_id)

func open_forage_compose(tile_info: Dictionary) -> void:
    if not _forage_compose_available(tile_info):
        return
    _ensure_compose_sheet()
    _compose.set_composing(ComposeState.KIND_FORAGE, _forage_source_key(tile_info))
    var subject := String(tile_info.get("food_module_label", "")).strip_edges()
    if subject == "":
        subject = HudFormat.food_module_label(String(tile_info.get("food_module", "")))
    # The eyebrow names the crew the sheet is about to staff, through the SAME resolver the stepper
    # inside it uses — `ASSIGN FORAGERS` on wild ground, `ASSIGN TENDERS` on a Tended Patch or a Field.
    var content := _compose_sheet.open(
        HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudFormat.plant_crew_label(
            tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX).to_lower(),
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
    # The read state names the crew by the patch's own rung, exactly as the sheet it opens does.
    var crew_label := HudFormat.plant_crew_label(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX)
    if not standing.is_empty():
        summary_model = _standing_summary_model(standing, SourceForecast.LABOR_KIND_FORAGE, crew_label.to_lower())
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
        _update_compose_open_button(_forage_assign_controls.get_child(idx) as Button, crew_label, subject_key)
        return
    _clear_forage_drawer()
    if not summary_model.is_empty():
        _forage_assign_controls.add_child(_build_standing_summary_from_model(summary_model))
    _forage_assign_controls.add_child(_build_compose_open_button(
        crew_label, subject_key,
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
        FoodIcons.for_floor_zone(SourceForecast.floor_zone(
            float(assignment.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))),
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