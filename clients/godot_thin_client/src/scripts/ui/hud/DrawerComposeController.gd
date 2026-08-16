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
## Built on the LegendController / FactionReadouts / TurnOrbController / SelectionCardController idiom:
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

## **RETIRED — `improvement_requested` AND `unqueue_requested`, THIS SHEET'S WHOLE COMMITTING HALF**
## (`docs/plan_standing_upkeep.md` §4.7a ①). The pair was the second axis's command
## (`cultivate` / `sow` / `tame` / `corral`) and its withdrawal, sent by the improvement checkbox's
## tick and untick.
##
## **THE CHECKBOX WAS NOT THE COMMIT, and that was the defect.** Only the sheet's own action button —
## reading **`Forage`** — sent the verb, so ticking the box and closing the sheet did nothing at all.
## Reported from play repeatedly. The declaration is one press of the WORK ROW's `⌃`, on the tab that
## owns the builders pool and the build queue, and the withdrawal is that queue's own row `✕`;
## **both signals survive on `BandPanelController`**, whose payloads were already byte-identical to
## these, so `Main.format_improvement` / `format_unqueue` did not change at all.
##
## What this sheet keeps is the FORECAST — the rung, what it would PAY once built, and its refusals —
## which is what a 28px work row could never hold and what makes it the place a rung is JUDGED. (Its
## COSTS went to the Work tab too; see `_improvement_offer_phrase`.)

## **THE PLAYER ASKED FOR THE WORK TAB** — the `Work tab` link on an OFFERED rung's line
## (`docs/plan_standing_upkeep.md` §4.7a ①). `HudLayer` relays it to `BandPanelController.show_work_tab`;
## this controller must not reach the dock itself, panel-to-panel coupling being the coordinator's.
##
## **IT CARRIES NO PAYLOAD, and deliberately does not name the source.** Focusing this patch's ROW on
## that tab would need a public focus seam on the board plus the panel to be showing this very band —
## two things this signal cannot assert — so it asks for the TAB and stops there. The board's own
## `⌃ N ready` chip is what finds the row once the player is on it.
signal work_tab_requested()

## **THERE IS NO KEEPING SIGNAL HERE** (`docs/plan_standing_upkeep.md` §2.5). `maintain_requested`
## retired with the `maintain` command it carried: maintenance left the tile, so the keeping is
## staffed as a band-wide standing role from the Band panel's WORKFORCE zone and this sheet has no
## keeping crew to commit.

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _compose: ComposeState = null
var _band_labor: HudBandLaborState = null
var _selection: HudSelectionState = null
# Read for `faction_knowledge` ONLY — the knowledge half of the investment-rung gates.
var _topbar: FactionReadouts = null
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
## **RETIRED — `_pen_extend_crew`, the ring's own dialled crew** (`docs/plan_standing_upkeep.md`
## §2.5). It held the number a stepper beside the Extend-pen button had dialled, because
## `extend_pen` took a trailing worker count; the verb DECLARES now — it appends a queue entry and the
## band's `builders` pool raises it at the head — so there is nothing left to hold between restates.

## **THE FORECAST QUERY SEAM**, injected by `HudLayer` after construction (`set_forecast_query`).
## The expedition branch's every number comes through it; the LOCAL hunt branch never touches it,
## being priced from the herd's own per-biomass vector and the band's ceilings.
var _forecast_query: ForecastQuery = null

func set_forecast_query(query: ForecastQuery) -> void:
    _forecast_query = query

func _init(compose: ComposeState, band_labor: HudBandLaborState, selection: HudSelectionState,
        topbar: FactionReadouts, selectioncard: SelectionCardController, host: Node,
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


## **THE SHEET OPENS ON THE BAND ALREADY WORKING THE SOURCE** — one rung AHEAD of
## `_resolve_assign_band`'s ladder, and it lives HERE rather than in that resolver because the resolver
## is shared with move-band and targeting, neither of which has a source in hand to ask about.
##
## Reported from play: a tile worked by Band 3 opened a sheet composing for Band 1, a band four tiles
## away with no idle crew and the patch outside its forage range. Every live reading on the sheet moved
## when the picker was corrected; the composed crew did not, and a composed 0 against a standing 2 turns
## the commit button into `Unassign`.
##
## `works_source` is asked of every player band in ROSTER order and must be PENDING-AWARE (the
## `effective_*_workers` readers), so a just-issued assign counts rather than the sheet bouncing to
## another band for the turn before the snapshot confirms it.
##
## **THE EXISTING LADDER WINS A TIE.** Where its answer is itself one of the crews, nothing moves — the
## band the player is reading is the right subject when it works the source. Only when the ladder names
## a band that is NOT working it does the first worker in roster order take it (roster order, so the
## answer is deterministic), and only when nobody works it does the ladder answer alone.
func _band_working_source(works_source: Callable) -> Dictionary:
    var resolved := _resolve_assign_band()
    var resolved_entity := int(resolved.get("entity", ComposeState.NO_BAND_ENTITY))
    var first_worker := {}
    for band_variant in _band_labor.current_player_bands():
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        if not bool(works_source.call(band)):
            continue
        var entity := int(band.get("entity", ComposeState.NO_BAND_ENTITY))
        if entity != ComposeState.NO_BAND_ENTITY and entity == resolved_entity:
            return resolved
        if first_worker.is_empty():
            first_worker = band
    return resolved if first_worker.is_empty() else first_worker

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

## **RETIRED — `_emit_improvement`, THE DECLARE/WITHDRAW EMITTER**
## (`docs/plan_standing_upkeep.md` §4.7a ①). It turned the improvement checkbox's tick into the SET
## verb (`cultivate` / `sow` / `tame` / `corral`) and its untick into `unqueue <faction> <source>`,
## and it went with the checkbox: the box was not the commit — the action button reading **`Forage`**
## was — so ticking it and closing the sheet did nothing, reported from play repeatedly.
##
## **BOTH ORDERS SURVIVE, on the surface that owns the queue.** `BandPanelController` emits the
## declaration off a WORK ROW's `⌃` and the withdrawal off a BUILD QUEUE row's `✕`, and its payloads
## were already byte-identical to these — which is what let this whole function go without
## `Main.format_improvement` / `format_unqueue` changing by a character.
##
## **WHAT WENT WITH IT AND MUST NOT COME BACK:** the *"an unchanged declaration is not an order"*
## comparison against `standing`, and the two-command commit. This sheet sends `assign_labor` and
## nothing else now, so the ORDERING rule that made a verb follow the staffing command has no second
## command left to order — and the sim's constraint it existed for (a verb reaches only bands already
## working the source) is satisfied by construction where the `⌃` lives, a work row existing only for
## a source the band already works.

## **RETIRED — `_commit_source`, the shrinking-crew-goes-first ordering**
## (`docs/plan_standing_upkeep.md` §2.5). The sheet composed a source's TAKE and its BUILD as one
## transaction over one crew pool, and the two commands had to be ordered so the band was solvent at
## every step. A verb states no hands now, so the improvement command spends nothing and cannot be
## refused for want of workers: the commit is `assign_labor` and then the verb, in the order the sim
## requires anyway — an improvement command only ever reaches bands ALREADY working the source, so
## the row has to exist before the verb names it.

## The per-turn take `workers` from `band` get off `herd` under `policy` — the sim's LOCAL/band hunt
## take before the output multiplier, `min(workers × per-worker, band_ceiling)`, in PROVISIONS.
## Returns `{available, rate}` (`available` false when the levers/ceiling are absent).
##
## **The per-worker rate is the HERD's own `per_worker_yield`, never the cohort's
## `hunt_per_worker_provisions`** (issue #337). That cohort field is a species-BLIND echo of the global
## `hunt.provisions_per_biomass` — it has no herd in scope, so it cannot know an inedible quarry pays no
## meat, and clamping a per-herd preview with it quotes a positive food rate against such a herd's
## all-zero food ceilings. The sim's own doc comments now say exactly this. Since arc #527 retired the
## trade axis an inedible quarry answers `available: false` here, and the sheet states no rate at all
## rather than a rate in an account nothing keeps.
## Resident-band only: an EXPEDITION's trip is never a rate division (see `SourceForecast.hunt_trip_forecast`).
##
## **IT TAKES NO IMPROVEMENT** (`docs/plan_standing_upkeep.md` §2.2). While a Tame or a Corral ran,
## the sim used to pay a gentling crew `workers × per_worker × build_dip`, and a take priced without
## the verb quoted ~4× what the herd handed over. The build has its own crew now, so the hunters' take
## is the plain one whether or not a rung is going up.
## `holding` asks the same question of the steady state — the ceiling becomes one turn's regrowth at
## this floor instead of the room above it. Same swap, same reason, as `_hunt_delivered_and_waste`'s.
func _hunt_take_rate(herd: Dictionary, floor: float, workers: int,
        holding: bool = false) -> Dictionary:
    var rates := SourceForecast.herd_axis_rates(herd, floor)
    var per_worker_rate := float(rates["per_worker"])
    var ceiling := float(rates["hold_ceiling" if holding else "ceiling"])
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return {"available": false}
    return {
        "available": true,
        "rate": maxf(minf(float(workers) * per_worker_rate, ceiling), 0.0),
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
    var rates := SourceForecast.herd_axis_rates(herd, floor)
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
## hunt takes WHOLE animals via a kill-credit bank, so the take is ONE quantised expression — the sim's
## `killed = min(the stock above the floor, max(1, whole bodies haulable), animals brought down)`,
## in bodies per turn — and delivery is `killed × min(one body, the crew's carry)`, the pack's hold
## charged PER BODY. Fractional carry capacity is idle (NOT waste, no animal having been dropped); a
## body killed and left behind is. Returns
## `{available, delivered, waste, waste_pct}` (all food/turn; `waste_pct` 0..1) or `{available=false}`
## when a lever/ceiling is absent (caller degrades to the old food/turn line). NEVER re-derives the
## ecology model — `food_per_animal` and the flow ceiling are sim exports.
##
## **`holding` ASKS THE SAME QUESTION OF THE STEADY STATE** — the take once the herd sits at its floor
## and only regrowth is on offer. It swaps ONLY the ceiling (the room becomes one turn's regrowth) and
## leaves the crew, the dip and the quantisation exactly where they are, so the burst and the steady
## rate are the same computation asked twice. A separate steady-state formula would be free to drop
## the whole-animal quantum and print a smooth number beside a bodies-per-turn one.
##
## **`herd` ARRIVES ALREADY KIT-PRICED** — `_hunt_yield_model` is its only caller and prices at its own
## top. Pricing again here would apply the ratio twice (`KitRoster.repriced_source` is not idempotent),
## and reaching for a raw dict instead would quote the equipped reference to a bare-handed crew.
func _hunt_delivered_and_waste(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        improvement: String, holding: bool = false) -> Dictionary:
    # ON THE PROVISIONS AXIS, through `herd_axis_rates` — the single place the quantised take's terms
    # are resolved, reading the HERD's species-aware per-worker rate rather than the cohort's
    # species-blind `hunt_per_worker_provisions`, which is what would re-introduce phantom food here.
    # An inedible quarry's per-animal food quantum is honestly `0`, so the guard below answers
    # `available: false` for it rather than dividing by zero; the trade quantum that used to stand in
    # went with the axis (arc #527).
    #
    # **THE DIP MULTIPLIES THE COLLECTION, AND THE QUANTISATION HAPPENS AFTER IT** — the sim's own
    # order (`hunt_take` composes `workers × per_worker`, THEN
    # `fauna::quantise_animal_take`). It arrives here on `per_worker`, so `collection` below carries it
    # and the quantisation runs against the dipped throughput. That is not a scaling of the answer:
    # below one body of carry the crew still kills one animal (the `max(1.0)` on the haul arm) and
    # wastes most of it — so a build moves the WASTE line, not merely the take. Dipping the ceiling, or
    # the delivered figure after quantisation, produces a number that is wrong in a way that still
    # looks plausible.
    var rates := SourceForecast.herd_axis_rates(herd, floor)
    var fpa := float(rates["per_animal"])
    var per_worker := float(rates["per_worker"])
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var ceiling := float(rates["hold_ceiling" if holding else "ceiling"])
    if fpa <= 0.0 or per_worker <= 0.0 or ceiling < 0.0 or workers <= 0:
        return {"available": false}
    ceiling *= output
    var collection := float(workers) * per_worker * output   # crew's raw food throughput /turn
    # THE KILL IS ONE EXPRESSION — the sim's own `fauna::quantise_animal_take`,
    # `killed = affordable.min(carryable.max(1.0)).min(brought_down)`, restated in food. The three
    # bounds are the stock above the floor, what the crew can HAUL, and what it brings down
    # (`docs/plan_hunt_through_combat.md` §2). The engagement arm was the one this sheet had never
    # had: one hunter's 40 biomass of carry read **307 Wild Fowl a turn** against a take of ten — the
    # sheet promising 30× what the sim pays, for the whole life of the wire field's absence.
    #
    # **THE `max(1.0)` BELONGS TO THE CARRY ARM ALONE, INSIDE THE `min`, AND THAT IS THE WHOLE OF WHY
    # THIS IS ONE EXPRESSION RATHER THAN TWO BRANCHES.** A party that cannot carry one whole animal
    # still kills one and wastes the rest — a fact about the PACK. A party that brings down three
    # quarters of an animal has brought down three quarters of an animal, and floors at nothing. While
    # the only bound below one body was the carry quotient the two were indistinguishable, so a
    # `carryable < 1` branch could price delivery as the crew's whole raw `collection`; with the
    # engagement arm in the same `min` that stopped being true, and a Wild Boar crew of six
    # (engaged 1, stayed 0.75) read **4.80 food/turn** — its entire carry throughput, twenty boar —
    # for a take of 0.18, then FELL to 0.36 at seven hunters, the readout dropping as the crew grew.
    #
    # **THE BOUND IS APPLIED TO THE ANIMAL COUNT, NOT TO `collection`**, and the two are the same
    # arithmetic taken in different orders: `floor(min(carry, engaged × fpa) / fpa)` is
    # `min(floor(carry / fpa), engaged)` — but the first divides a product of `fpa` BY `fpa` and can
    # land a whole engagement one animal short on a rounding, while the second is exact.
    #
    # `animals_engaged` answers UNBOUNDED for a pen and for a species with no engagement stage, and
    # `animals_stayed` passes an unbounded reach straight through — so the `min` is a no-op and every
    # managed-herd and plant-web frame reads exactly what the carry quotient alone gave it.
    #
    # **AND THE ARM IS WHAT STAYS, NOT WHAT IS REACHED.** The sim runs engage → retreat → fight and
    # hands the SURVIVOR count to the quantiser, so the bound here is `animals_stayed` — which is also
    # the ONE thing the kit's `dispersion` moves on this line. A trapping party is not there to be seen
    # and keeps everything it reaches; a spear party on the same warren keeps one animal in four. Read
    # off `engage_rate` instead, Big-game and Trapping quote the identical hunt.
    #
    # **AND THE CARRY CLAMP IS PER BODY, NOT PER TURN** — which is the other half of why one expression
    # replaces two branches. A body lands WHOLE on the turn it drops and the crew hauls `collection`
    # that turn; the remainder rots where it fell. Below one body per turn `killed` is a CADENCE (a
    # body every `1/killed` turns), so clamping the AVERAGED kill by the carry credits the crew a whole
    # body's worth of meat no single turn could hold: a party whose collection, whose ceiling and whose
    # 0.6-of-a-body cadence all coincide really lands `0.6 × collection` and wastes 40% of what it
    # kills, where the averaged-then-clamped form reads the full ceiling with no waste at all — 1.67×
    # too high, and silent about the meat left on the ground.
    var haulable := maxf(floorf(collection / fpa), 1.0)
    var brought_down := SourceForecast.animals_stayed(
        SourceForecast.animals_engaged(workers, float(rates["engage_rate"])),
        float(rates["stay"]))
    # BODIES PER TURN — the herd's own offer, the crew's haul, and what the party puts on the ground,
    # whichever is least. Fractional below one: a body every `1/killed` turns.
    var killed := minf(minf(ceiling / fpa, haulable), brought_down)
    var delivered := killed * minf(fpa, collection)
    var killed_food := killed * fpa
    var waste := maxf(killed_food - delivered, 0.0)
    var waste_pct := (waste / killed_food) if killed_food > 0.0 else 0.0
    return {"available": true, "delivered": delivered, "waste": waste, "waste_pct": waste_pct,
        "per_animal": fpa}

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
## (§3.1). `improvement` is threaded in for the WINDOW alone, which resolves its terms through
## `herd_axis_rates` and must divide by the same quantum the sheet's take does.
##
## Empty when the wire does not describe this herd (older snapshot / non-huntable).
func _hunt_floor_takes(herd: Dictionary, band: Dictionary, improvement: String) -> Dictionary:
    var takes := {}
    var zero_account := SourceForecast.zero_account_of(herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        var floor_value := SourceForecast.floor_for_preset(preset)
        var forecast := _hunt_forecast(herd, band, floor_value)
        if not bool(forecast["known"]):
            continue
        # Each preset's cap, rendered only when non-zero — **per account, materials included**. The
        # material ceiling composes at THIS preset's floor by the same
        # `max(0, B − floor·K) × rate` rule the food one does (`material_per_biomass`), which is why an
        # inedible quarry's presets state `0.22 hide` rather than the blank they read while the
        # retired trade cap was the only thing that could stand in for its food zeros.
        var pair := SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]), 0.0, zero_account,
            forecast["material_ceiling"])
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
## account the take credits through `SourceForecast.rescaled_accounts` → `yield_rows` and the account
## table's units, exactly as the plant web's is and exactly as the raid's payload
## (`_trip_yield_rows`) already was. **The WHOLE-ANIMAL reading belongs to the CHART above it** (the
## escapement curve and its handle, which count bodies) and to the whole-trip payload of a raid; a
## per-turn row wearing the quarry's name in place of an account states a rate in a currency the
## stores do not keep, and the header over it (`per turn · now → after`) then keys nothing the
## number beside it can be spent as.
##
## **`improvement` IS THE CREW'S OWN DIP, and the two forecasts here take it DIFFERENTLY** — the plant
## twin's rule, on the animal web. The TAKE carries it (the sim pays a building crew
## `workers × per_worker`, and it is the crew's collection that is then quantised into whole
## animals); the SUSTAIN reference below must not.
## **THE SOURCE AS THE CHOSEN KIT PRICES IT** — the one seam the compose sheet's numbers move
## through when the player switches kits, and the reason neither yield model below knows a kit
## exists.
##
## `carry_key` names which of the kit's two carry tiers this source is measured in; everything else
## is arithmetic `KitRoster.repriced_source` does on the wire's own terms. A source that publishes no
## retreat (a patch, a pen) is unaffected by the second half of that substitution, so the same call
## serves both webs.
##
## **The tiers are the BAND's, not the kit's fresh ones** — `KitRoster.effective_tiers` steps a tier
## down when the band has worn that item out, so a dry-basketed band is quoted bare-handed even while
## the picker shows what a fresh gathering kit would grant.
## **THE HERD, PRICED AT THE CHOSEN KIT — and the only door onto this sheet's hunt arithmetic.**
##
## Reported from play: the take moved with the kit but *clear it now*, *hold it after* and
## *max N workers useful* did not. Repricing used to live inside the two yield models, and this sheet
## reads a source in FOUR shapes — a forecast (`forecast_inputs`), the chart model that renders both
## crew-target pills, the axis rates the quantised take is composed from, and the degrade path's smooth
## rate. Every one of them is a function of the crew's carry, so every one of them takes the priced
## herd; a call site that reached for the raw dict is exactly how three of the four were missed.
##
## **PRICE ONCE PER PRODUCER.** `KitRoster.repriced_source` is no longer idempotent (its reference is
## the roster's tier rather than a field the substitution overwrites), so a producer prices at its own
## top and never hands a priced dict to another producer that prices too.
func _hunt_priced_herd(herd: Dictionary, band: Dictionary) -> Dictionary:
    return _kit_priced_source(herd, HudComposeVocab.BARE_FORECAST_PREFIX, band, KitRoster.JOB_HUNT,
        _compose.hunt_kit_id())

## The plant twin. A patch publishes no retreat, so only the carry half of the substitution bites.
func _forage_priced_patch(tile_info: Dictionary, band: Dictionary) -> Dictionary:
    return _kit_priced_source(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX, band,
        KitRoster.JOB_FORAGE, _compose.forage_kit_id())

## The hunt forecast, priced — and the ONLY way this sheet builds one. Pairing the repricing with the
## construction is what makes "some call sites were missed" unrepresentable rather than a thing to
## remember.
func _hunt_forecast(herd: Dictionary, band: Dictionary, floor: float) -> Dictionary:
    return SourceForecast.forecast_inputs(_hunt_priced_herd(herd, band),
        SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX, floor)

## …and the plant one.
func _forage_forecast(tile_info: Dictionary, band: Dictionary, floor: float) -> Dictionary:
    return SourceForecast.forecast_inputs(_forage_priced_patch(tile_info, band),
        SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, floor)

## The two SNAPSHOT reads `KitRoster.priced_source` must not make for itself (it is stateless), and
## nothing else. The resolve, the axis and the arithmetic all live there, so the dock's raid sheet
## prices its chart through the identical code rather than a second copy of this.
func _kit_priced_source(src: Dictionary, prefix: String, band: Dictionary, job: String,
        kit_id: String) -> Dictionary:
    return KitRoster.priced_source(src, prefix, _band_labor.kits(), job,
        _band_labor.default_kit_id(job), kit_id, band)

## **DOES THIS READOUT STATE THE FLOOR WALK AT ALL?** — the one gate on every `after` reading both
## webs compose, and therefore on the row's arrow and on the caption's `now → after` alike.
##
## A crew reaching its floor is only half of it. The other half is that a COMPOSED BUILD suppresses
## the walk entirely, because the readout would otherwise stack two unrelated "laters" with nothing
## marking them apart: the row's `now → after` is this rung's burst falling to its steady rate over
## a handful of turns, while the `ONCE TENDED` row directly beneath it is the NEXT rung's payoff
## after a ~25-turn build. Reported from play — the caption sat one line above the labelled row and
## was read as naming it. One transition on screen, labelled, and it is the one being decided; the
## walk is not lost, the verdict two lines down still narrates it in prose ("Reaches the floor in 13
## turns, then holds it").
##
## **IT IS ASKED AT THE MODEL, and one seam serves both webs.** The caption's `has_after` is derived
## from the rows the model emits, so gating here is what makes it impossible for the arrow and the
## key over it to disagree — which is the failure the caption resolver has already paid for once.
func _walks_to_the_floor(reaches: bool, improvement: String) -> bool:
    return reaches and improvement == SourceForecast.IMPROVEMENT_NONE

func _hunt_yield_model(band: Dictionary, herd_raw: Dictionary, floor: float, workers: int,
        improvement: String, reaches: bool = false) -> Dictionary:
    # **PRICED AT THE KIT THE CREW WILL BE SENT WITH, ONCE, BEFORE A SINGLE TERM IS READ** — so the
    # sustainability bar, the take, the waste and the degrade path are all one kit's story. Every read
    # below is off `herd`; the raw dict is not in scope again, which is the point of shadowing it here
    # rather than pricing at each `herd_axis_rates` call (this model makes three).
    var herd := _hunt_priced_herd(herd_raw, band)
    # **WHAT THIS CREW BRINGS HOME IN MATERIALS, AT THIS FLOOR** — composed from the herd's two RATES
    # rather than from the assignment's resolved `material_yield`, which a pre-commit sheet has not
    # got: the sim seeds it empty by design because projecting materials needs the take in biomass
    # while the forecast resolves in currency space. So this arm is `min(workers × per_worker,
    # ceiling(floor))` per material — the food side's own clamp, one account further out.
    #
    # **IT IS INDEPENDENT OF THE FOOD AXIS, and that independence is the whole point.** Both food
    # paths below bail on an inedible quarry (its per-animal quantum is honestly 0, so there is
    # nothing to quantise and nothing to smooth), and a model that returned `{}` there is what left a
    # wolf's sheet quoting no rate at all.
    var materials := _hunt_material_rows(herd, band, floor, workers)
    # **THE SUSTAINABILITY BAR IS THE FOOD PEAK'S CEILING**, in the same account the take is measured
    # in. It is the floor at which the herd settles on its most productive biomass, so a take above it
    # is one the herd cannot pay forever — which is exactly what the verdict claims.
    #
    # **AND IT IS RESOLVED AT `IMPROVEMENT_NONE` DELIBERATELY — the one call site where the undipped
    # rates are the correct ones.** This is the LINE THE TAKE IS JUDGED AGAINST, not a take: it is the
    # herd's own renewable yield, a fact about the animals rather than about the crew. Dipping it would
    # move the bar down in step with the take it is compared to, the two dips would cancel, and a
    # building crew could never trip the flag at all — the ⚠ would become vacuous exactly where a
    # quarter-throughput crew is least able to explain itself.
    var sustain_rates := SourceForecast.herd_axis_rates(herd, SourceForecast.FLOOR_FOOD_PEAK)
    var sustain_ceiling := float(sustain_rates["ceiling"])
    if sustain_ceiling < 0.0:
        return {}
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var sustainable := sustain_ceiling * output
    var dw := _hunt_delivered_and_waste(band, herd, floor, workers, improvement)
    if not bool(dw.get("available", false)):
        # Graceful degrade — the per-animal quantum (or a lever) is unknown, so fall back to the
        # smoothed per-turn line rather than regress the readout. **It credits the SAME account set
        # the quantised path does**, through the same rescale: the two paths differ in whether the
        # take is quantised, never in what a take pays, and a model whose two branches stated
        # different accounts for one herd is the defect one branch above records.
        var take := _hunt_take_rate(herd, floor, workers)
        if not bool(take.get("available", false)):
            # **NO FOOD PATH AT ALL — but a material one may still stand.** An inedible quarry reaches
            # here every time, and its materials are the whole of what the hunt pays; a `{}` model
            # renders no readout, which is the `+0.00` this arm exists to close.
            if SourceForecast.signed_material_components(materials) == "":
                return {}
            return {
                YIELD_MODEL_ROWS: SourceForecast.yield_rows(
                    0.0, 0.0, SourceForecast.YIELD_ACCOUNT_NONE, {}, materials),
                YIELD_MODEL_TEXT: HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % (
                    SourceForecast.yield_components(
                        0.0, 0.0, SourceForecast.YIELD_ACCOUNT_NONE, materials)),
                # **NO OVERDRAW VERDICT ON A MATERIAL-ONLY TAKE.** The sustainability bar is the food
                # peak's ceiling in the account the take is measured in, and this take is measured in
                # none of it — the drawdown a material take causes is real, but the client has no
                # material sustainable-yield to judge it against and must not invent one.
                YIELD_MODEL_OVERDRAW: false,
                YIELD_MODEL_WASTE: "",
            }
        var actual := float(take["rate"]) * output
        var smooth := SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            actual)
        var account := SourceForecast.YIELD_ACCOUNT_FOOD
        var smooth_after := {}
        if _walks_to_the_floor(reaches, improvement):
            var smooth_hold := _hunt_take_rate(herd, floor, workers, true)
            if bool(smooth_hold.get("available", false)):
                smooth_after = SourceForecast.rescaled_accounts(herd,
                    HudComposeVocab.BARE_FORECAST_PREFIX, float(smooth_hold["rate"]) * output)
        return {
            YIELD_MODEL_ROWS: SourceForecast.yield_rows(
                float(smooth[SourceForecast.YIELD_ACCOUNT_FOOD]),
                float(smooth[SourceForecast.YIELD_ACCOUNT_FODDER]),
                account, smooth_after, materials),
            # The SENTENCE states the same vector the rows do — `yield_components` is the joiner the
            # plant twin already uses, and it obeys the same render-only-when-non-zero rule, so this
            # line cannot quote one account beside a row set carrying two.
            YIELD_MODEL_TEXT: HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % (
                SourceForecast.yield_components(
                    float(smooth[SourceForecast.YIELD_ACCOUNT_FOOD]),
                    float(smooth[SourceForecast.YIELD_ACCOUNT_FODDER]), account, materials)),
            YIELD_MODEL_OVERDRAW: _is_overdraw(actual, sustainable) \
                and _herd_take_draws_down(herd, floor, workers),
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
    # **THE COUNT IS TAKEN ON THE PROVISIONS AXIS AND VALUED IN EVERY ACCOUNT** — the sim's own order
    # (`forecast_production_and_take`: quantise, then `YieldPair::rescaled_to`). `yield_rows` is the
    # one place the "render only where the vector pays" rule lives, so an inedible quarry — whose
    # provisions rate is a structural 0 — rescales to a zero food component that renders NO row, and
    # the zero account below keeps that answer for an all-zero take.
    var take := SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
        delivered)
    # THE ZERO ACCOUNT IS THE ACCOUNT THE TAKE WAS MEASURED IN — the same choice the degrade branch
    # above makes, so one model's two paths can never state an empty take in two different accounts.
    var account := SourceForecast.YIELD_ACCOUNT_FOOD
    # THE STEADY STATE RIDES EACH ACCOUNT'S OWN `after`, so `build_yields_row` composes the arrow from
    # the two magnitudes it formats itself and its header keys them. It RESCALES THE SAME WAY the take
    # does — an arrowed row must key both accounts consistently, and a hold rate credited on one axis
    # beside a take credited on two would arrow only half the reading. `yield_rows` drops an `after`
    # equal to its take, which is the same "an arrow to itself is noise" test this used to make here.
    var after := {}
    if _walks_to_the_floor(reaches, improvement):
        var held := _hunt_delivered_and_waste(band, herd, floor, workers, improvement, true)
        if bool(held.get("available", false)):
            after = SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
                float(held["delivered"]))
    return {
        YIELD_MODEL_ROWS: SourceForecast.yield_rows(
            float(take[SourceForecast.YIELD_ACCOUNT_FOOD]),
            float(take[SourceForecast.YIELD_ACCOUNT_FODDER]),
            account, after, materials),
        YIELD_MODEL_TEXT: HudComposeVocab.HUNT_DELIVERED_FORMAT % [rate_text, quarry],
        YIELD_MODEL_OVERDRAW: _is_overdraw(delivered, sustainable) \
            and _herd_take_draws_down(herd, floor, workers),
        YIELD_MODEL_WASTE: SourceForecast.HUNT_WASTE_NOTE_FORMAT % int(round(waste_pct * 100.0)) \
            if waste_pct > 0.0 else "",
    }

## **THE CREW'S MATERIAL TAKE AT THIS FLOOR** — `min(workers × per_worker_material, material_ceiling)`
## per material, then the band's output multiplier, which is the food side's own composition one
## account further out (arc #527 follow-up).
##
## `herd` arrives ALREADY KIT-PRICED (`_hunt_yield_model` prices at its own top and is the only
## caller), so this reaches `SourceForecast.forecast_inputs` directly rather than through
## `_hunt_forecast`, which would price a second time. That is this file's "price once per producer"
## rule, and the reason the parameter is named for what it already is.
##
## **NEVER read the assignment's `material_yield` here.** That is the RESOLVED take and is empty
## pre-commit by design; a compose sheet asks what a crew WOULD bring home, which is a question only
## the rates can answer.
func _hunt_material_rows(herd: Dictionary, band: Dictionary, floor: float, workers: int) -> Array:
    if workers <= 0:
        return []
    var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, floor)
    if not bool(forecast["known"]):
        return []
    return SourceForecast.scaled_material_rows(
        SourceForecast.expected_materials(float(workers), forecast),
        float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL)))

## The hunt web's half of the overdraw GATE — `SourceForecast.take_draws_down` on a herd, asked at the
## crew's LIVE improvement.
##
## **A GATE WALKS THE CREW THE TAKE IT GATES IS PRICED FOR, and with the dip retired that is simply
## the take crew.** The verb used to be a term here, because the take carried the rung's build
## fraction and an undipped projection would have walked a crew four times the one being quoted. It
## carries none now — the builders are their own allocation — so the projection and the take divide by
## one throughput with nothing to keep in step.
func _herd_take_draws_down(herd: Dictionary, floor: float, workers: int) -> bool:
    return SourceForecast.take_draws_down(herd, SourceForecast.SOURCE_KIND_HERD, "", floor,
        workers)

## The LOCAL forage patch's live per-turn yield preview — the plant twin of `_local_hunt_preview_bbcode`.
## Forage is SMOOTH (no whole-animal rhythm — no lumpy carry, no waste), so the line is just the
## per-turn take + a sustainability verdict: income-green `+2.74 /turn · renewable` when the take is
## within the patch's Sustain ceiling, WARN-amber `⚠ … — overdraws the patch` when a Surplus/Deplete/
## Eradicate policy draws it down. Both scaled by the acting band's output multiplier, like the hunt
## line. "" (no line) when the forecast levers are unknown, so the panel degrades gracefully.
##
## **THE WHOLE VECTOR, EACH ACCOUNT ONLY WHEN NON-ZERO (#426).** This read the food account alone,
## which is the same lie the picker face above it told: a hay meadow previewed `+0.00 /turn ·
## renewable` — "staff this and get nothing, sustainably" — for a rung that fills the band's fodder
## store every turn. `SourceForecast.yield_components` is the joiner the worked rows already use, so the composed preview and the row it becomes next turn word the vector alike.
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
    var sustain := _forage_forecast(tile_info, band, SourceForecast.FLOOR_FOOD_PEAK)
    if not bool(sustain["known"]):
        return {}
    var forecast := _forage_forecast(tile_info, band, floor)
    if not bool(forecast["known"]):
        return {}
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var actual := SourceForecast.expected_yield(forecast, workers, band)
    # Fodder names no whole-animal quantum anywhere — no animal pays it — which is why these calls
    # leave the engagement arm's key defaulted and the arm drops out.
    var actual_fodder := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_fodder", "ceiling_fodder")
    # **WHAT THE CREW BANKS IN MATERIALS, AT THIS FLOOR** — `min(workers × per_worker, ceiling(floor))`
    # per material, the SAME clamp the food and fodder accounts take and the same one the hunt sheet
    # already applied. **This is the argument the plant web never got**: a tile 32% cotton and 26%
    # tobacco composed a sheet reading `0.24 → 0.18 FOOD · — FODDER` and never mentioned the fibre and
    # tobacco the gather actually banks, because this model passed FOUR arguments to `yield_rows`
    # where its hunt twin passed five.
    #
    # `expected_materials` reads the forecast's own two vectors, both of which the patch publishes
    # under the same prefixed keys the herd does — which is why one composition serves both webs and
    # this is a call rather than a second derivation.
    var materials := SourceForecast.scaled_material_rows(
        SourceForecast.expected_materials(float(workers), forecast), output)
    var zero_account := String(forecast["zero_account"])
    # THE STEADY-STATE TAKE, one `min` against a different ceiling — the SAME `expected_yield_account`,
    # reached by key, so the burst and the hold rate cannot be computed two ways. Composed only when
    # this sheet states the floor walk at all: a crew that settles short never enters the holding state
    # (promising it a rate it never attains is the failure this whole reading exists to fix), and a
    # sheet composing a BUILD suppresses the walk outright — see `_walks_to_the_floor`.
    var after := {}
    if _walks_to_the_floor(reaches, improvement):
        after = {
            SourceForecast.YIELD_ACCOUNT_FOOD: SourceForecast.expected_yield_account(
                forecast, workers, band, "per_worker", "hold_ceiling",
                SourceForecast.FORECAST_FOOD_PER_ANIMAL_KEY),
            SourceForecast.YIELD_ACCOUNT_FODDER: SourceForecast.expected_yield_account(
                forecast, workers, band, "per_worker_fodder", "hold_ceiling_fodder"),
        }
    var rows := SourceForecast.yield_rows(actual, actual_fodder, zero_account, after, materials)
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
            actual, banked_fodder, zero_account, materials),
        # **THE FODDER CEILING COMPARISON STAYS, LOCK OR NO LOCK, and deleting it is the plausible
        # wrong move.** The take draws the same biomass down whether or not the crew banks the hay, so
        # the drawdown is unchanged — and on a hay-only patch this comparison is the only drawdown
        # signal there is.
        YIELD_MODEL_OVERDRAW: (_is_overdraw(actual, float(sustain["ceiling"]) * output) \
            or _is_overdraw(actual_fodder, float(sustain["ceiling_fodder"]) * output)) \
            and SourceForecast.take_draws_down(tile_info, SourceForecast.SOURCE_KIND_FORAGE,
                HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, workers),
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
            HudStyle.WARN_HEX, SourceForecast.COMPONENT_SEPARATOR, waste]
    return body

## A "Band: [▼]" dropdown row for the assign controls: lists every player band (positional
## "Band N" names, matching the roster) and selects `selected_band`; `on_pick` fires with the
## chosen band dict. The actor band is always explicit — shown even with one band (single-item
## dropdown). NOTE: lists ALL player bands; in-range filtering (Forage within work_range / Hunt
## within work_range + leash) is deferred to the multi-band slice (needs the hunt-leash reach in
## the snapshot, and can't be exercised until a 2nd band can exist).
##
## **It goes through the SHARED field-row builders** (`HudWidgets.build_field_key` /
## `build_option_picker`), which is what makes it, the Kit row beneath it and the Band panel's Quarry
## row one family: one declared key width, one ghost chrome, one height. It used to be a bare
## `OptionButton` on a natural-width label — the only off-palette control on the sheet, and the one
## whose value box started at a different x from every other row's.
func _build_band_picker(selected_band: Dictionary, on_pick: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.add_child(HudWidgets.build_field_key(HudWorkVocab.BAND_PICKER_LABEL))
    var bands := _band_labor.current_player_bands()
    var selected_entity := int(selected_band.get("entity", -1))
    var selected_index := 0
    var entries: Array = []
    for i in bands.size():
        var band: Dictionary = bands[i]
        var entity := int(band.get("entity", -1))
        if entity == selected_entity:
            selected_index = i
        entries.append({
            "label": HudFormat.band_display_name(band, i + 1),
            # Resolved through the labor model at PRESS time rather than captured: the picker outlives
            # a snapshot, and the band dict this row was built from is a copy of a stale turn's.
            "on_pick": func() -> void: on_pick.call(_band_labor.player_band_by_entity(entity)),
        })
    # The face is the selected entry's own label — there is no glyph and no marker to hold apart here,
    # unlike the Kit row — so it is stated from the same list rather than composed.
    var face := "" if entries.is_empty() \
        else String((entries[selected_index] as Dictionary).get("label", ""))
    # No tooltip: the row's own `Band:` key says what the control is, and this picker has never
    # carried one. Inventing copy here would be a change of content behind a change of chrome.
    row.add_child(HudWidgets.build_option_picker(entries, selected_index, face, ""))
    return row

## Cap the worker stepper at what the source can absorb: min(the band's assignable workers,
## max-useful). Returns `{cap, note}` — `note` is set ONLY when max-useful is the binding cap, so a
## dead `+` button is always explained rather than mysterious (the idle-worker cap explains itself).
##
## THE WORKED-ROW TWIN IS `SourceForecast.source_worker_cap_state`, and the two must gate at ONE
## ceiling. Neither carries a keeper floor any more — that crew has its own stepper
## (`docs/plan_standing_upkeep.md` §2.2) — and the *hold it after* crew, which is a fact about this
## take rather than a demand a kind of source makes, is applied INSIDE
## `SourceForecast.max_useful_workers`, so it reaches both twins without either being told about it.
##
## **`assignable` IS THE SOURCE'S CREW POOL** — idle plus what this band already has on it
## (`HudBandLaborState.source_crew_pool_forage`), which is the ceiling `assign_labor` is judged
## against. It briefly had the sheet's own proposed BUILDERS subtracted from it, and the note took a
## `build_crew` argument so it could name that nearer lever; the build is a band-level role now
## (`docs/plan_standing_upkeep.md` §2.5), so there is one stepper and one remedy.
func _forecast_worker_cap(forecast: Dictionary, assignable: int) -> Dictionary:
    # **NO KEEPER FLOOR UNDER THE TAKE CAP** (`docs/plan_standing_upkeep.md` §2.2). This used to be
    # raised to a managed herd's `herdersNeeded`, because one crew both hunted the animals and held
    # them: a cap sized on the take alone went dead below the count the sim asked for. Those keepers
    # are the MAINTAIN allocation now, with their own stepper and their own ceiling, so flooring the
    # TAKE stepper on them would demand hands that belong to another crew.
    var useful := SourceForecast.max_useful_workers(forecast)
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
## source is in. Shared verbatim by both webs: the plant ladder
## (Cultivate → Sow) and the animal one (Tame → Corral) get the same control, the same states and the
## same forecast, because they are the same decision about different stock.
##
## **WHICH STATE IT IS IN IS DERIVED FROM THE METERS, NOT FROM THE COMPOSED VERB**
## (`docs/plan_standing_upkeep.md` §2.4). `SourceForecast.build_verb` is the one derivation and the
## composed value reaches it as a pending DECLARATION — honoured only where the meter it names is at
## zero, which is exactly the case the sim cannot guess (a wild patch could climb to tended *or* be
## sown). Everything else the meters answer for themselves, newest first.
##
## **THAT IS WHAT PUTS A REPAIR ON SCREEN.** A completed rung whose meter has eroded back below its
## cost is BUILDING again with nothing declared — the state the player has to see, since the remedy is
## hands — and the old test (`composed != NONE and not improvement_is_done`) rendered the DONE label
## there and offered no way back. **The meter's fullness and the rung's achievement stay orthogonal**:
## this reads fullness, `improvement_is_done` reads the stamped retention bar, so a patch at 90% shows
## as *Cultivating* while the ground it stands on is still tended.
##
## The states and their precedence (see `HudWidgets.build_improvement_control` for the shape):
##   RUNNING first — something is being built here, so nothing else is on offer. Its face carries
##       the meter and, where the crew's proposed floor leaves anything for them to work, the turn
##       estimate. **It is a STATE** — as every state of this control is since §4.7a ①. What puts a
##       build with work on it down is `abandon`, command line only in this slice; what withdraws a
##       DECLARATION is the BUILD QUEUE row's `✕`.
##       **It states no PAUSE**: the phase-keyed WARN line this state used to carry
##       (`_improvement_paused_note`, animal-only `_tame_stalled_hint` before that) described a sim
##       that stopped a build outside Thriving, and `docs/plan_harvest_floor.md` §3.2 replaced that
##       cliff with a rate — so it warned of a pause beside a meter its own face showed advancing. The
##       rule it existed to make audible is stated better one register up, live and quantified, by the
##       aside's teaching line.
##   DONE next — the source stands on a built rung, so the state gets a static label, and the NEXT
##       rung's line renders beneath it if there is one.
##   OFFERED last — the next rung, its price, and the REMEDY naming the control that takes it. When
##       that rung is GATED the offer is not rendered at all: the reason takes the control's slot
##       instead (`IMPROVEMENT_STATE_GATED` — see the gated branch below for why).
##
## **THE FACES CARRY NO PAYOFF, AND THE `payoff_face` CALLABLE THAT FED THEM IS GONE.** Both states
## state the CHOICE and nothing else — the verb, or the verb and its meter. The terms moved one
## register down into the readout, where they sit beside the take they are meant to be compared with;
## see `_improvement_deal_row`. The one number this control still resolves for itself is the pen's
## zero-payoff-under-a-feed test, which is a warning about the RUNG and belongs in its note slot.
##
## **THERE IS NO `extra_rows` HOOK ANY MORE** (`docs/plan_standing_upkeep.md` §4.7a ③). The plant web
## dropped its CROP PICKER through it, beneath the box, on the reasoning that which crop the rung
## commits to is part of the same decision — and the decision left this sheet, so the crop went with
## it to the BUILD QUEUE row. Nothing is mounted beneath this control on any state.
##
## **`workers`, `floor` AND `kit_gear` ARE THE PROPOSAL, not the standing assignment** — the stepper's
## count, the slider's floor and the gear terms of the kit THIS ENTRY'S BUILDERS would carry (never
## the sheet's own picker — see `_build_gear_for`). Both faces price
## their turn estimate against them through `SourceForecast.build_turns_at`, which is the whole reason
## this control is rebuilt by the live-refresh registry at its call sites: the sim's own
## `buildTurnsRemaining` answers for the crew already there and cannot move under any of the three.
## **`build_crew` IS THE BAND'S `builders` POOL, AND IT IS READ RATHER THAN COMPOSED**
## (`docs/plan_standing_upkeep.md` §2.5). It was a stepper ON this control — the source's own build
## allocation, which a verb carried as a trailing worker count — and both the stepper and the count
## are retired: a verb DECLARES, appending an entry to the band's build queue, and the hands stand on
## `assign_labor <faction> <band> builders <n>`. So this control quotes the estimate at the pool the
## band already has (`HudBandLaborState.build_crew_forage` / `_hunt`) and offers nothing to dial.
##
## ⛔ **DO NOT GIVE IT A HYPOTHETICAL CREW SLIDER.** With the pool at zero the sim honestly publishes
## *no estimate*, and the tempting repair is a proposed crew to re-price it — which is exactly the
## per-source build staffing this slice deletes, re-implied by a control.
## **THE GEAR TERM OF THIS SHEET'S BUILD, AT THE KIT THE QUEUE ENTRY IMPLIES** — never at the kit the
## sheet's own picker is showing.
##
## The picker above this control chooses what the TAKE crew carries; what speeds a build is what the
## BUILDERS carry, and those are two different rows. Both sheets used to pass their own selection —
## so a Cultivate was priced at the Gathering kit's build axis (nothing at all) and a Tame at
## whichever hunt kit was picked — which is the whole of §1.2's defect.
##
## **The kit is derived per ENTRY, and the entry's web is the sheet's own** (`KitRoster.builders_kit_for`
## → `equipment.md` → "THE BUILDERS' KIT IS DERIVED PER QUEUE ENTRY"): a patch is a plant build and a
## herd an animal one, so the sheet knows its branch from the job it composes. `build_gear` then zeroes
## a row whose branch disagrees, exactly as the sim's own `serves_branch` does.
func _build_gear_for(band: Dictionary, kind: String) -> Dictionary:
    var branch := KitRoster.build_branch_for_kind(kind)
    var builders_kit := KitRoster.builders_kit_for(_band_labor.kits(),
        HudBandLaborState.role_kit_id(band, HudConst.LABOR_KIND_BUILDERS), branch,
        _band_labor.head_build_branch(band))
    return KitRoster.build_gear(band, builders_kit, branch)

func _build_improvement_control(kind: String, source: Dictionary, prefix: String, floor: float,
        composed: String, band: Dictionary, workers: int, kit_gear: Dictionary,
        works_the_ground: bool, target: VBoxContainer, build_crew: int = 0) -> void:
    # THE RUNG IN FLIGHT — whatever the METERS say is going up, with `composed` reaching the derivation
    # as a declaration for a meter at zero. That covers the two cases no stored verb can: a rung eroded
    # back below its cost, and a build the player never re-declared.
    var source_kind := SourceForecast.source_kind_for_labor(kind)
    var running_verb := SourceForecast.build_verb(source, prefix, source_kind, composed)
    # **A DECLARATION IS NOT A BUILD, AND THE CONTROL MUST NOT RENDER ONE AS THE OTHER.** A build is
    # ACTUALLY in flight when there are builders on it or work banked on its meter; a rung with neither
    # has been *declared* and nothing more. Rendering that as RUNNING was a one-way door — a player who
    # declared `cultivate` on a band with no free hands got `Cultivating 0 / 50 work (0%)` and no way
    # back off it. Reported from play.
    #
    # It is the DECLARED state instead: the offer's own face plus `◷ Queued`, whose withdrawal is
    # `unqueue <faction> <source>` on the BUILD QUEUE row's `✕` (`docs/plan_standing_upkeep.md` §2.5,
    # §4.7a ①). The *not started* warning travels with it and stays useful, a band that SHRINKS
    # shedding its builders while the declaration stands.
    #
    # **THE CREW TEST IS *ARE THEY ON THIS ONE*, WHICH IS THE QUEUE'S HEAD** (§4.6b). The whole
    # `builders` pool goes on the head entry until its meter fills, so a staffed pool says nothing
    # about an entry waiting third in line — and reading it as *in flight* would put the one-way
    # `Cultivating 0 / 50 work (0%)` Label straight back on every queued-and-waiting rung.
    if running_verb != SourceForecast.IMPROVEMENT_NONE \
            and not (build_crew > SourceForecast.BUILD_CREW_NONE \
                and SourceForecast.build_is_queue_head(source, prefix)) \
            and SourceForecast.improvement_progress(source, prefix, running_verb) \
                <= SourceForecast.BUILD_METER_UNSTARTED:
        _mount_declared_control(source, prefix, source_kind, running_verb, floor, band, kit_gear,
            target, build_crew)
        return
    if running_verb != SourceForecast.IMPROVEMENT_NONE:
        var glyph := FoodIcons.for_policy(running_verb)
        var participle := String(
            HudComposeVocab.IMPROVEMENT_RUNNING_LABELS.get(running_verb,
                running_verb.capitalize()))
        # **THE METER IS THE WHOLE FACE, AND IT NOW STATES THE JOB'S SIZE.** The payoff that used to
        # close it (`· then 1.39 food`) is a readout row now — it stood one line above a PER TURN box
        # quoting a different number for the same source, with nothing on either to say which
        # question each was answering. The work absolutes come from the SAME composer the tile card
        # and the herd drawer use, so one build never reads two ways on one screen.
        #
        # `kind` here is the LABOR kind (`hunt`/`forage`); the forecast layer speaks SOURCE kinds
        # (`herd`/`forage`). They differ on the animal web, so this conversion is not optional.
        var deal := SourceForecast.improvement_forecast(
            source, source_kind, prefix, floor, running_verb)
        # **THE ONLY NOTE A RUNNING BUILD CAN CARRY IS THE PEN'S ZERO PAYOFF, and the phase-keyed
        # PAUSE line that used to lead it is gone** (`docs/plan_harvest_floor.md` §3.2). No rung on
        # either web stops on `EcologyPhase`: the sim replaced that cliff with a RATE, so a crew
        # pulling hard on the ground it is clearing builds SLOWLY. The line therefore printed
        # "⚠ Paused … this only advances while Thriving" beside a meter its own face showed advancing,
        # and prescribed the opposite of the remedy — the FLOOR is what paces the build, which the
        # aside states live ("Building at ×1.60 — a higher floor builds faster"). What genuinely stops
        # a build is an empty escapement room, and the honest statement of THAT is the estimate
        # dropping out (`SourceForecast.build_turns_at`'s work predicate), not a note about the phase.
        var notes: Array = []
        # **THE BLEEDING METER NEEDS NO NOTE HERE ANY MORE, because the LINE says it**
        # (`docs/plan_standing_upkeep.md` §4.6a). `BUILD_SLIDING_NOTE` filled a silence: at a builders
        # crew of zero `build_turns_at` answered `BUILD_TURNS_NO_ESTIMATE` and the turns clause was
        # dropped, so the face read `🌱 Cultivating 30 / 50 work (60%)` and stopped. **That form now
        # answers at zero** — `∞ turns` in the losing red where the keeping is short, and the neutral
        # held reading where it is covered — so a note would restate the line above it, and would
        # additionally claim a loss on a meter the player has parked on purpose.
        #
        # **`BUILD_UNSTAFFED_UNSTARTED` CANNOT REACH THIS BRANCH** — it is no crew AND no work banked,
        # i.e. exactly `not in_flight`, so it renders on the DECLARED state below with its own note.
        # That is what keeps the *not started* warning on a withdrawable rung, not a one-way door.
        var running_face := HudComposeVocab.IMPROVEMENT_RUNNING_BARE_FORMAT % [
            glyph, DetailFormat.build_meter_value(participle,
                SourceForecast.improvement_progress(source, prefix, running_verb),
                SourceForecast.build_work_done(source, prefix, running_verb),
                SourceForecast.build_work_cost(source, prefix, running_verb))]
        # **"ADD HANDS AND WATCH IT DROP" IS THE WHOLE POINT, so it goes on the face beside the crew
        # stepper that moves it — and it is priced at THIS stepper's crew and THIS slider's floor.**
        # It read the sim's `buildTurnsRemaining` for a release, which is the answer for the crew
        # ALREADY on the source: the sheet quoted `≈32 turns` at one worker and went on quoting it as
        # the player stepped to three, freezing the one readout this arc exists to make legible on the
        # one panel where the decision is being made. `build_turns_at` evaluates the closed form the
        # sim publishes the terms for; `BUILD_TURNS_NO_ESTIMATE` still renders as no clause at all
        # rather than as a `0` that would promise the build is about to land.
        #
        # **AND BOTH NEVER-FINISHING SENTINELS RENDER AS `∞ turns`, IN A WARNING INK** — a crew at or
        # below what this meter is ROTTING by nets nothing, so there is no finish date and the sheet
        # must not imply one by staying silent beside a meter that is visibly full of work. It is the
        # one reading on this control that should stop the player, which is why it never takes the
        # neutral ink the larder's own ∞ gets. `BUILD_TURNS_HOLDS` is amber and `BUILD_TURNS_ROTS` is
        # red — the pace below decides which, off the same fork the tile card's row uses.
        var running_turns := SourceForecast.build_turns_at(
            source, prefix, running_verb, build_crew, floor, kit_gear)
        # **THE CLAUSE IS QUOTED AT THE CREW, for the reason the pace below is** (§4.6a): the same
        # `BUILD_METER_HOLDS` is a crew treading water at `∞ turns` and a build parked on purpose at
        # `held`, and only the stepper's own count tells them apart.
        if running_turns != SourceForecast.BUILD_TURNS_NO_ESTIMATE:
            running_face = HudComposeVocab.IMPROVEMENT_RUNNING_TURNS_FORMAT % [
                running_face, DetailFormat.build_turns_clause(running_turns, build_crew)]
        if _rung_pays_nothing_under_its_feed(deal, band):
            notes.append(HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE)
        # **THE STATE OF THE METER IS THE FACE'S COLOUR, AND IT HAS THREE VALUES** — green while the
        # surplus is positive and a real turn count is quoted, amber while the crew banks exactly the
        # rate and holds it at `∞`, red while the rate goes unpaid and the meter loses ground. It
        # replaced the prose note beside the BUILDERS stepper (`2 work a turn holds it — the surplus is
        # progress`), which was a sentence doing a colour's job on the line above it.
        # `SourceForecast.build_pace` owns the classification and the rule that the client reads the
        # sim's answer rather than deriving the sign of a surplus for itself.
        # **THE PACE IS ASKED WITH THE CREW, because one wire value covers two states**
        # (`SourceForecast.BUILD_PACE_HELD`): `BUILD_METER_HOLDS` is a crew treading water when there
        # is a crew, and a deliberate park when there is not.
        var pace := SourceForecast.build_pace(running_turns, build_crew)
        target.add_child(HudWidgets.build_improvement_control(running_verb,
            HudWidgets.IMPROVEMENT_STATE_RUNNING, running_face,
            _improvement_running_tooltip(running_verb), notes, true, pace))
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
            String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, ""))))
        break
    # OFFERED — the ONE rung on offer, ordered by `RungGates` so the sheet, the work board and the map
    # can never disagree about which rung is next. Renders BENEATH a done label when there is one.
    var offer := RungGates.next_rung_offered(kind, source, composed, _player_knowledge(), prefix)
    if offer.is_empty():
        return
    var rung := String(offer["policy"])
    # The verb and its PRICE — the payoff this used to close on reads in the readout, one register
    # down, and what closes it now is what the job COSTS. **A verb with no price was survivable only
    # while every verb cost the same 25 turns**: rungs declare their own size in work units now, so a
    # sheet that offers `Sow a field here` without saying it is half again the Cultivate below it
    # hides the one number the choice turns on. `workCost` is published whether or not a build runs —
    # that is what makes the quote available pre-commit — and the turns half is quoted for the crew,
    # floor and kit the player is COMPOSING (the running face above carries why), dropped entirely
    # where that has no finite answer. A rung nobody has started is exactly the state the sim's own
    # estimate cannot speak to: there is no crew on it yet to have been measured.
    # **THE VERDICT IS VISIBLE BEFORE THE COMMITMENT, NOT AFTER IT.** A crew at or below what this
    # source's meter is rotting by never finishes, so the offer's own price quotes `∞ turns` in the
    # warning ink as the stepper below is dragged past it — the answer arriving while the decision is
    # being made rather than as a blank line once it has been taken. On a rung nobody has started
    # nothing is at risk, so the rot is `0` and every staffed crew gets a real count.
    # **THE PACE STILL NEEDS THE ESTIMATE, and it is the only thing left that does.** The count is off
    # the face (see `_improvement_offer_phrase`), but *can this band's pool ever finish it* is a
    # verdict rather than a number, and it inks the line.
    var offer_turns := SourceForecast.build_turns_at(
        source, prefix, rung, build_crew, floor, kit_gear)
    var offer_face := _improvement_offer_phrase(kind, rung, works_the_ground)
    var reasons := RungGates.gate_reasons_for({rung: offer.get("reasons", [])}, rung)
    # **GATED — THE REASON IS THE CONTROL, and the offer text is not shown at all.** This used to
    # render the full offer ("🌱 Cultivate this patch · then 0.04 food · 0.81 fodder") as
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
            String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")),
            # A second and later reason keeps the note treatment beneath — the lead line can only
            # carry one, and dropping the rest would hide half of what the rung costs to unlock.
            reasons.slice(1)))
        return
    # **THE OFFER IS NEVER DEAD FOR WANT OF HANDS** (`docs/plan_standing_upkeep.md` §2.5). It greyed
    # out on an empty build pool, because declaring used to mean declaring a build WITH a crew that
    # the sim refused outright. A verb names no crew: declaring APPENDS a queue entry, which is legal
    # and costs nothing whether or not anybody is on the `builders` role — and the note that says
    # nobody is comes from the DECLARED state's own *not started* warning.
    #
    # **AND THE OFFER NAMES THE CONTROL THAT TAKES IT, because it is no longer that control itself**
    # (§4.7a ①). A priced offer with no visible way to accept it is exactly what a sheet that merely
    # stopped committing would leave behind.
    target.add_child(HudWidgets.build_improvement_control(rung,
        HudWidgets.IMPROVEMENT_STATE_OFFERED, offer_face,
        String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")), [], false,
        SourceForecast.build_pace(offer_turns, build_crew),
        func(_meta: String) -> void: emit_signal("work_tab_requested")))


## **THE DECLARATION `build_verb` SHOULD HONOUR — the overlay's if it has one, else the sheet's own.**
## One expression, so the two webs cannot resolve the precedence differently
## (`docs/plan_standing_upkeep.md` §4.7a ①).
##
## The overlay wins because it is the NEWER statement: it is written the instant the Work board's `⌃`
## is pressed, while the composition is whatever this sheet was last seeded or dialled to. Where the
## overlay carries nothing — every frame on which the player has not just declared — this is exactly
## the composition, which is what it has always been.
func _declared_or_composed(pending: String, composed: String) -> String:
    return composed if pending == SourceForecast.IMPROVEMENT_NONE else pending

## **THE OFFERED LINE — one sentence, in one of two forms** (`docs/plan_standing_upkeep.md` §4.7a ①,
## ③). It was a FACT line plus a REMEDY note beneath it, and reported from play the fact line read as
## an imperative — as the button it used to be — with a second sentence under it explaining that it is
## not one. One line says both, and its `Work tab` is a live link.
##
## **THE LIMIT IS STATED, NOT RELAXED, AND THAT WAS RAY'S DECISION.** The sim refuses an improvement
## verb aimed at a source the band does not work; the alternative to saying so was relaxing that rule.
## So a band with no crew on the ground AND none composed on this sheet is told to send people first,
## and everyone else is told which control to press — two sentences, both single-line.
##
## **NO PRICE.** The pile, the rate and the turn count left this sheet entirely — see
## `HudComposeVocab.BUILD_OFFER_WORKED_FORMAT` for where each went and why.
##
## `works_the_ground` is the caller's already-resolved `not is_noop` — the same standing-plus-composed
## test the dead commit button forks on, so the sheet cannot tell the player to press a button it has
## itself disabled.
func _improvement_offer_phrase(kind: String, rung: String, works_the_ground: bool) -> String:
    var verb := String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS.get(rung, rung.capitalize()))
    var link := HudFormat.bbcode_link(HudComposeVocab.WORK_TAB_LINK_TEXT,
        HudComposeVocab.WORK_TAB_LINK_META, HudStyle.SIGNAL_HEX)
    var sentence := HudComposeVocab.BUILD_OFFER_WORKED_FORMAT % [verb, link]
    if not works_the_ground:
        sentence = (HudComposeVocab.BUILD_OFFER_UNWORKED_PLANT_FORMAT             if kind == SourceForecast.LABOR_KIND_FORAGE             else HudComposeVocab.BUILD_OFFER_UNWORKED_ANIMAL_FORMAT) % [verb, link]
    return HudComposeVocab.IMPROVEMENT_OFFER_BARE_FORMAT % [FoodIcons.for_policy(rung), sentence]

## **THE RUNG, NAMED AND NOTHING ELSE** — `🌱 Cultivate this patch`, which is what the DECLARED state
## puts its `◷ Queued` beside. It is `IMPROVEMENT_OFFER_BARE_FORMAT` over the same label the offered
## sentence embeds, so a queued rung and an offered one cannot come to be called two different things.
func _improvement_rung_face(rung: String) -> String:
    return HudComposeVocab.IMPROVEMENT_OFFER_BARE_FORMAT % [FoodIcons.for_policy(rung),
        String(HudComposeVocab.IMPROVEMENT_OFFER_LABELS.get(rung, rung.capitalize()))]

## **RETIRED — `_mount_build_crew_row`, the BUILDERS stepper on the improvement control**
## (`docs/plan_standing_upkeep.md` §2.5). It stated the source's own build crew, because the verb
## carried one as a trailing worker count; the verb DECLARES now — `cultivate <f> <x> <y>` appends an
## entry to the band's build queue and the extra token is a PARSE ERROR — and the hands stand on
## `assign_labor <faction> <band> builders <n>`, a standing role card on the Band panel.
##
## **WHAT THE SHEET SHOWS INSTEAD IS ALREADY ON THE CONTROL**: the rung's `<rung>WorkCost` (the pile),
## its `<rung>UpkeepDemand` (the rate it costs to hold, forever) and the chained
## `buildTurnsRemaining` this source would get if queued now — all three published before anything is
## queued. With the pool at zero the sim answers *no estimate*, and the face says so plainly rather
## than rendering nothing.
##

## **THE DECLARED CONTROL — a rung this band has queued and that has neither reached the head of that
## queue nor banked any work.** The offer's own face (the verb and its price), the `◷ Queued` clause
## that tells it from an unqueued offer, and the *not started* warning beneath.
##
## **IT IS A `Label`, AND THE UNDO IS SOMEWHERE ELSE** (`docs/plan_standing_upkeep.md` §4.7a ①). It
## was a TICKED, live checkbox whose untick sent `unqueue <faction> <source>` — the withdrawal that
## really does drop the declaration, leaving the row, its take crew, its kit and the meter exactly as
## they are (§2.5). That verb is unchanged and so is its payload; what moved is the control, onto the
## BUILD QUEUE row's `✕`, because a declaration IS a queue entry and the list of entries is where one
## is dropped. This sheet commits nothing, so it withdraws nothing either.
##
## **IT STATES NO POSITION AND NO DATE.** The schedule belongs to the surface that can reorder it —
## see `HudComposeVocab.BUILD_QUEUED_CLAUSE`.
##
## **IT IS PRICED AT THE BAND'S POOL, exactly as the OFFER beside it is** (§4.6b). A declared entry
## that has not reached the head of the queue is not being worked THIS turn, and quoting it at nobody
## would state *no estimate* for a band that has builders — the date it wants is *what this will take
## once they reach it*, which is the closed form at the pool.
func _mount_declared_control(source: Dictionary, prefix: String, source_kind: String, rung: String,
        floor: float, band: Dictionary, kit_gear: Dictionary,
        target: VBoxContainer, build_crew: int) -> void:
    var turns := SourceForecast.build_turns_at(source, prefix, rung, build_crew, floor, kit_gear)
    # **THE *NOT STARTED* WARNING IS GATED ON THE POOL, and it was unconditional**
    # (`docs/plan_standing_upkeep.md` §2.5). It fired on every declared rung, which was right while
    # DECLARED meant *nobody is building this*; a declaration is queued now, so a band with builders
    # on the role really is going to raise it — the face beside this note quotes the date — and a
    # warning there would be telling the player nothing is happening on a job that is simply waiting
    # its turn. With the pool empty it is exactly as true as it ever was, and it LEADS, being the
    # loudest thing true of the rung; the pen's zero payoff rides beneath it as it does on a running
    # build, that note being about the RUNG rather than about the work in flight.
    var notes: Array = []
    if build_crew <= SourceForecast.BUILD_CREW_NONE:
        notes.append(HudComposeVocab.BUILD_UNSTARTED_NOTE)
    if _rung_pays_nothing_under_its_feed(
            SourceForecast.improvement_forecast(source, source_kind, prefix, floor, rung), band):
        notes.append(HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE)
    target.add_child(HudWidgets.build_improvement_control(rung,
        HudWidgets.IMPROVEMENT_STATE_DECLARED,
        HudComposeVocab.IMPROVEMENT_DECLARED_FORMAT % [
            _improvement_rung_face(rung), HudComposeVocab.BUILD_QUEUED_CLAUSE],
        String(HudComposeVocab.IMPROVEMENT_HINTS.get(rung, "")),
        notes, true, SourceForecast.build_pace(turns, build_crew)))

## **A ZERO PAYOFF UNDER A RUNNING FEED IS A PURE LOSS, and the note that says so has now outlived
## three homes for the zero itself.** The pen harvests by constant escapement, so a herd at or below
## the MSY point pays 0.00 while still eating feed every turn. It is a warning about the RUNG rather
## than about the work in flight, which is why it rides the DECLARED control as well as the RUNNING
## one — and why the test is ONE predicate, so those two cannot come to disagree about whether a
## commitment is a loss.
func _rung_pays_nothing_under_its_feed(deal: Dictionary, band: Dictionary) -> bool:
    if deal.is_empty() or not bool(deal["feed_rung"]):
        return false
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    return float(deal["feed"]) * output >= SourceForecast.FOOD_FLOW_MIN \
        and float(deal["payoff"]) * output < SourceForecast.FOOD_FLOW_MIN

## **RETIRED — `_improvement_offer_face`, the verb and its PRICE.** It composed
## `🌱 Cultivate this patch — 50 work, ≈25 turns · 2 work a turn from Agriculture to hold` and was
## shared by the OFFERED and DECLARED states so a rung could not be priced one way before it was
## queued and another after.
##
## **THE PRICE LEFT THE SHEET, not the client** (`docs/plan_standing_upkeep.md` §4.7a ①). Ray, from
## play: *"That information should be on the work tab. No need to have it here, it is useless."* The
## pile and the standing rate ride the WORK ROW's `⌃` tooltip — one hover from the control that spends
## them — and the turn count rides the BUILD QUEUE row's date, which is the sim's own chained answer
## and the one a player reorders against. `DetailFormat.build_price_clause` composes the first pair
## still and is unchanged; only its caller moved.
##
## What is left here is two much smaller composers: `_improvement_offer_phrase` (the pointer sentence)
## and `_improvement_rung_face` (the bare rung, which DECLARED puts `◷ Queued` beside).

## The RUNNING control's tooltip: the rung's own hint — *what does this buy?* — and nothing else.
##
## **The per-web abandon clause went with the command it described** (`docs/plan_standing_upkeep.md`
## §2.4). It answered *what happens if I stop?* about an uncheck that no longer exists, and the state
## it described — a build with nobody on it — is now stated live on the source itself, by the
## `Keeping:` row and the `At risk:` countdown beside it, which say what it is COSTING rather than
## what it would cost. The tooltip therefore takes no web, so it takes no `kind`.
func _improvement_running_tooltip(improvement: String) -> String:
    return String(HudComposeVocab.IMPROVEMENT_HINTS.get(improvement, ""))

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

## **THE RUNG THE READOUT'S DEAL BLOCK QUOTES** — the composed verb while a build is in flight, else
## the rung on OFFER, and `""` when there is no offer or the offer is GATED.
##
## The gated case is the one worth writing down: a gated control spends its whole slot on the unmet
## prerequisite and states no payoff at all, deliberately (a number you cannot act on is noise at the
## moment you are told you cannot act), so quoting the same payoff one register down would put back
## exactly what that branch removes. `RungGates.gate_reasons_for` is the same test the control makes.
func _improvement_deal_rung(kind: String, source: Dictionary, prefix: String,
        composed: String) -> String:
    if composed != SourceForecast.IMPROVEMENT_NONE:
        return composed
    var offer := RungGates.next_rung_offered(kind, source, composed, _player_knowledge(), prefix)
    if offer.is_empty():
        return ""
    var rung := String(offer["policy"])
    if not RungGates.gate_reasons_for({rung: offer.get("reasons", [])}, rung).is_empty():
        return ""
    return rung

## **THE DEAL, AS THE READOUT'S ONE LABELLED ROW** — what the rung on the table will pay ONCE TENDED
## / SOWN / TAMED / PENNED. `{}` where there is nothing to state, which `_mount_readout` renders as no
## block at all rather than an empty well.
##
## `payoff_terms` is the CALLER's, already resolved: the plant web substitutes the SELECTED CROP's
## own payoff and the animal web quotes the herd's, and both sheets resolve it exactly once so the
## crop picker and this row can never name different crops (issue #419). `""` means the wire quotes
## no deal for this rung, and no row is then rendered at all — never a fabricated `0.00`, the same
## rule the bare face formats follow.
##
## **THE FORECAST LOOKUP SURVIVED THE ROW THAT NEEDED A FLOOR.** Corral's feed debit rides this row's
## value, and `feed_rung` / `feed` are what say whether there is one — a structural test, so a rung
## the wire does not describe can never quote an upkeep. Everything read here is floor-independent,
## which is why the block is NOT in the live registry: `improvement_forecast` is asked at the FOOD
## PEAK, the floor `_improvement_payoff_terms` already quotes the payoff at, a rung's pay being a
## property of the finished rung rather than of the floor the crew holds while building it.
func _improvement_deal_row(kind: String, source: Dictionary, prefix: String, band: Dictionary,
        rung: String, payoff_terms: String) -> Dictionary:
    if rung == "" or payoff_terms == "":
        return {}
    var value := payoff_terms
    var deal := SourceForecast.improvement_forecast(source,
        SourceForecast.source_kind_for_labor(kind), prefix, SourceForecast.FLOOR_FOOD_PEAK, rung)
    # THE PEN'S UPKEEP RIDES THE PAYOFF ROW, Corral only. `corralYield` is GROSS, so the row would
    # otherwise promise a rate the pen never nets.
    if not deal.is_empty() and bool(deal["feed_rung"]):
        var feed := float(deal["feed"]) \
            * float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
        if feed >= SourceForecast.FOOD_FLOW_MIN:
            value = HudComposeVocab.IMPROVEMENT_DEAL_FEED_FORMAT % [
                payoff_terms, SourceForecast.format_magnitude(feed)]
    return {
        HudWidgets.IMPROVEMENT_DEAL_ROW_LABEL:
            String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS.get(rung, rung)),
        HudWidgets.IMPROVEMENT_DEAL_ROW_VALUE: value,
    }

## The payoff terms for a rung — the payoff VECTOR the built rung pays, each account only when
## non-zero, so a hay meadow reads `1.80 fodder`. "" when the wire quotes no payoff at all, which the
## readout renders as no payoff row rather than "0.00".
##
## Quoted at the FOOD PEAK, because a payoff is a property of the finished rung and not of the floor
## the crew happens to hold while building it. The floor reaches the deal only through the crew's dip.
func _improvement_payoff_terms(source: Dictionary, kind: String, prefix: String, rung: String,
        band: Dictionary) -> String:
    return _payoff_terms(SourceForecast.improvement_forecast(source,
        SourceForecast.source_kind_for_labor(kind), prefix, SourceForecast.FLOOR_FOOD_PEAK, rung),
        band)

## An already-resolved deal's payoff VECTOR as products, scaled by the acting band's output
## multiplier — "" for a deal the wire does not quote. Reached through `_improvement_payoff_terms`
## (which resolves the deal itself), so one payoff is composed one way whichever rung is being asked
## about; the plant web's crop substitution answers in the same grammar through `_crop_payoff_terms`.
func _payoff_terms(deal: Dictionary, band: Dictionary) -> String:
    if deal.is_empty():
        return ""
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    # **THE MATERIAL HALF OF THE PAYOFF, and it is the whole payoff on an inedible quarry.** Tame and
    # Corral quoted `0.00 food` or nothing at all for a wolf, because `pastoral_yield` / `corral_yield`
    # are provisions and a wolf's are honestly zero — so the two rungs a player would actually take on
    # such a species advertised no reason to take them. Scaled by the band's output exactly as the two
    # scalars are: it is the same take through the same crew.
    return SourceForecast.picker_products(float(deal["payoff"]) * output,
        float(deal["payoff_fodder"]) * output, SourceForecast.YIELD_ACCOUNT_FOOD,
        SourceForecast.scaled_material_rows(deal.get("payoff_material", []), output))

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
    # **THE RING QUEUES; IT DOES NOT STAFF** (`docs/plan_standing_upkeep.md` §2.5). A ring rides the
    # same `animal:pen` rung as the pen it widens, so it is funded exactly like every other build: the
    # press appends an entry to the build queue of every band keeping the pen, and the band's
    # `builders` pool raises it when it reaches the head. It carried a CREW STEPPER for one slice,
    # because the verb took a trailing worker count; that token is a parse error now, so the control
    # is back to the one-click standing action it always was.
    var extend_btn := Button.new()
    extend_btn.text = HudComposeVocab.PEN_EXTEND_LABEL
    extend_btn.tooltip_text = HudComposeVocab.PEN_EXTEND_TOOLTIP
    HudStyle.apply_button(extend_btn, "ghost")
    extend_btn.pressed.connect(func() -> void:
        _emit_extend_pen(x, y))
    target.add_child(extend_btn)

## Emit the extend-pen request for the pen anchored at (x, y). Main formats
## `extend_pen <faction> <x> <y>` — **no worker count**: the verb declares and the `builders` pool
## raises whatever is at the head of the band's queue.
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
# It held the crew targets and the verdict; the YIELDS ROW — the food and fodder numbers the player is
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
    # **THE ROW LABEL CARRIES NO BUILD NOTE ANY MORE.** It used to say *"— building this rung, each
    # carries 25% as much"*, which was the dip: one crew doing two jobs. The build has its own crew
    # below, so these hands carry a full load whether or not a rung is going up, and there is nothing
    # left for the label to qualify.
    var label_line := HBoxContainer.new()
    label_line.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_NOTE_SEPARATION)
    label_line.add_child(row_label)
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

## **THE READOUT** (§7.1, §7.2, §7.7) — the sheet's bottom half as ONE bounded box with four
## deliberately different registers, loudest first because that is the reading order:
##
##   a. THE YIELDS — what this crew, at this floor, brings home. The answer, and therefore the largest
##      type on the sheet. `yields_at(floor, crew)` recomposes it, which is what lets it follow a drag.
##   a2. THE DEAL — what the rung on the table will pay once it stands, as ONE labelled row. It
##      renders only where there is a rung to state, and it is deliberately OUTSIDE the live registry:
##      the payoff is quoted at the food peak, so nothing in it moves under a floor drag.
##   b. THE VERDICT — which of the crew and the floor is binding, with its severity dot.
##   c. THE ASIDE — the idle-crew note and the floor's own teaching line, under a dashed rule at the
##      quietest size on the sheet. The teaching line used to stand between the chart and the stepper
##      as a two-line paragraph, which made the panel's least urgent information one of its loudest
##      elements.
##
## No box at all when there is nothing to put in it — a source with no floor axis AND no priceable
## take has no readout, rather than an empty well.
##
## **EVERY REGISTER IN THIS BOX THAT MOVES WITH THE FLOOR IS LIVE** — which is all of them but the
## deal, whose exclusion is stated at its mount below. The aside's locked-account line is the case
## worth stating: anything whose value — or whose PRESENCE — depends on the floor belongs in the
## live set, and the lock's sentence explains a `—` in the register above it, so raising the floor
## takes the fodder row away and a sentence resolved once before the render would outlive the mark it
## answers. The row and its explanation cannot disagree in either direction, and what guarantees that
## is NOT a single call — this function calls `yields_at` three times per refresh (the emptiness
## probe, the yields host, the aside). It is that the yield models are PURE and every one of those
## calls passes IDENTICAL arguments, which `_live_floor` / `_live_reaches` enforce by having one
## definition apiece.
func _mount_readout(parent: VBoxContainer, hosts: Array, model: Dictionary, workers: int,
        yields_at: Callable, labor_kind: String,
        deal_row: Dictionary = {}) -> void:
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
    # **THE IMPROVEMENT DEAL — a SIBLING of the yields block, never a row inside its flow.** Two
    # harness contracts read that flow structurally (the caption is `parent.get_child(index - 1)`;
    # both webs' take assertions parse the flow's joined text by splitting on an account word), so a
    # deal term folded in would corrupt both — silently, which is why it has a block and a meta of
    # its own. See `HudWidgets.IMPROVEMENT_DEAL_META`.
    #
    # **IT IS DELIBERATELY OUT OF THE LIVE REGISTRY**, by that registry's own rule: a payoff is a
    # property of the finished rung, quoted at the food peak, so nothing in it moves under a floor
    # drag and a host in the set would pay for work it does not need. `{}` renders nothing at all —
    # a source with no rung on the table has no deal, not an empty one.
    if not deal_row.is_empty():
        column.add_child(HudWidgets.build_improvement_deal(
            String(deal_row.get(HudWidgets.IMPROVEMENT_DEAL_ROW_LABEL, "")),
            String(deal_row.get(HudWidgets.IMPROVEMENT_DEAL_ROW_VALUE, ""))))
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
##
## **NO `while_building` KEY REACHES THE CAPTION ANY MORE** (`docs/plan_standing_upkeep.md` §2.2).
## It said *these readings are the DIPPED take*; the build has its own crew, so these are the plain
## take whether or not a rung is going up. `build_yields_row` resolves the caption from the ROWS,
## which is the only thing left that can key it.
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


## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _herd_compose_available(herd):
        return
    var herd_id := String(herd.get("id", ""))
    # The band the sheet DEFAULTS to: whoever already hunts this herd, else the shared ladder's answer.
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.effective_hunt_workers(candidate, herd_id) > 0)
    # When the selected herd changes, default the actor band to the resolved band (and re-seed
    # the compose count/floor from its staffing); otherwise preserve the picked band + count
    # across per-snapshot re-renders of the same herd.
    var source_changed := _compose.hunt_key() != herd_id
    if source_changed:
        _compose.begin_hunt_source(herd_id, int(resolved.get("entity", ComposeState.NO_BAND_ENTITY)))
        # **AND THE KIT, because the default is a fact about the ANIMAL now** — every render writes
        # the resolved id back onto the compose state, so a choice resolved on the last herd would
        # otherwise read as the player's own choice here and hide this herd's own default.
        _compose.reset_hunt_kit()
    # The actor is the band-picker selection; fall back to the resolved band if it has vanished.
    var band := _band_labor.player_band_by_entity(_compose.hunt_band())
    if band.is_empty():
        band = resolved
        _compose.set_hunt_band(int(band.get("entity", ComposeState.NO_BAND_ENTITY)))
    # THE SECOND AXIS's standing value (issue #442) — what this herd is already BUILDING, DERIVED from
    # its meters with the assignment's own `improvement` reaching the derivation as a pending
    # declaration (`docs/plan_standing_upkeep.md` §2.4). It seeds the improvement control so a herd
    # mid-Tame opens on its running face rather than looking untouched, and it is what the commit
    # compares against to decide whether a verb needs sending. The plant sheet's twin carries why the
    # stored field alone cannot answer.
    var standing_improvement := SourceForecast.build_verb(herd,
        HudComposeVocab.BARE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_HERD,
        _band_labor.improvement_for_hunt(band, herd_id))
    # **A COMPOSITION IS SEEDED FOR ONE BAND, so the ACTOR changing re-seeds it exactly as the SOURCE
    # changing does.** The picker's callback writes the band and rebuilds, so at this point the composed
    # band IS the picked one — and the crew, the floor and the build it carries are still the previous
    # band's row. Evaluated AFTER `band` resolves, or it would compare against the band being left.
    var band_entity := int(band.get("entity", ComposeState.NO_BAND_ENTITY))
    if source_changed or _compose.hunt_seeded_band() != band_entity:
        var staffed := _band_labor.workers_for_hunt(band, herd_id)
        # **ONE CREW SEEDS FROM THE BAND'S ROW** (`docs/plan_standing_upkeep.md` §2.5). The build
        # count seeded here too while a verb carried one; a verb declares and names no hands now, so
        # the only crew this sheet composes is the take.
        _compose.seed_hunt(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.floor_for_hunt(band, herd_id), standing_improvement)
    # The effective (pending-aware) standing crew, which the commit's unassign/no-op test reads below.
    var current := _band_labor.effective_hunt_workers(band, herd_id)
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
    # **THE HANDS THIS SHEET MAY SPEND** — idle plus the crew this band already has on this herd
    # (`HudBandLaborState.source_crew_pool_hunt`), which is the ceiling `assign_labor` is judged
    # against. Without the standing term a fully-allocated band capped at `0` and the player could
    # take a crew to nothing and never put it back.
    #
    # **THE BUILDERS ARE NOT IN THIS TRANSACTION ANY MORE** (`docs/plan_standing_upkeep.md` §2.5).
    # The sheet composed two crews and clamped them against one pool while a verb carried a head
    # count; the build is a band-level role now, so this is the take's ceiling and nothing else.
    var assignable := SourceForecast.expedition_party_cap(band) if is_expedition \
        else _band_labor.source_crew_pool_hunt(band, herd_id)
    # **THE KIT, RESOLVED HERE AND MOUNTED UNDER THE CREW ROW.** It is part of the question the sim is
    # asked, so every reading below is priced for it — the resolve leads and the ROW lands beside the
    # crew it describes.
    var kits := _band_labor.kits()
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    # **THE HERD IS PASSED, so a kit this quarry cannot be worked with is never resolved onto.** A
    # trapping selection made against a warren must not survive into a Red Deer's sheet as the kit the
    # picker opens on — the offer test is asked at the FRESH tier, so the answer moves with the quarry
    # and never with the band's wear.
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_HUNT, default_kit,
        _compose.hunt_kit_id(), herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    _compose.set_hunt_kit_id(kit_id)
    # **THE RAID'S NUMBERS ARE ASKED FOR, EXPEDITION BRANCH ONLY.** A LOCAL hunt is priced from the
    # herd's own per-biomass vector and the band's ceilings — client arithmetic over wire terms, no
    # query. A raid's every figure is the sim's forward simulation of THIS band, kit, party and floor,
    # so the sheet asks and renders the answer; there is no table to mismatch against any more.
    #
    # The ask is idempotent on the composed key, so it costs nothing on the rebuilds that do not move
    # it — and every rebuild that DOES (a stepper tick, a kit switch, a committed floor) is exactly a
    # re-query. A floor DRAG never reaches here: only a committed change rebuilds the sheet.
    var raid_view := {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    if is_expedition:
        raid_view = _raid_forecast_view(band, herd_id, kit_id, _compose.hunt_count(),
            _compose.hunt_floor(), SourceForecast.expedition_party_cap(band))
    var raid_answer: Dictionary = raid_view["answer"]
    var raid_ready := String(raid_view["state"]) == ForecastQuery.STATE_READY
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
    # composition outlives its build and would keep quoting a Tame that finished. `build_verb` is that
    # retirement and its opposite in one derivation: it also ADOPTS a build the meters say is running
    # where nothing was declared.
    # **A DECLARATION MADE ELSEWHERE THIS FRAME OUTRANKS THE COMPOSITION** (§4.7a ①). The Work board's
    # `⌃` writes the rung to the OPTIMISTIC OVERLAY, and this sheet is what would otherwise go on
    # offering a rung the band has just queued — the sheet stays open across that press, so a stale
    # OFFERED here is a sheet arguing with the queue row beside it. See
    # `HudBandLaborState.pending_improvement_for` for why it reads the overlay and not the assignment.
    var composed_improvement := SourceForecast.IMPROVEMENT_NONE if is_expedition \
        else SourceForecast.build_verb(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            SourceForecast.SOURCE_KIND_HERD,
            _declared_or_composed(_band_labor.pending_improvement_for(
                band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id),
                _compose.hunt_improvement()))
    if not is_expedition and composed_improvement != _compose.hunt_improvement():
        _compose.set_hunt_improvement(composed_improvement)
    var forecast := _hunt_forecast(herd, band, _compose.hunt_floor())
    # The party stepper caps at the max-useful count on BOTH branches — a raid's haul (`animals_taken`)
    # PLATEAUS with party size once the herd's surplus binds, so extra hunters past the plateau raid no
    # more animals and should be flagged idle exactly as an over-staffed local hunt is (the silent-idle-
    # hunter gap this pass closes). The local branch caps at the source's max-useful ceiling.
    # **THE KEEPER FLOOR IS GONE FROM THIS CAP** (`docs/plan_standing_upkeep.md` §2.2). A managed herd
    # does need hands every turn to hold its tameness — and those hands are the MAINTAIN crew, which
    # has its own stepper on this very sheet. Raising the HUNT cap to `herdersNeeded` made the take
    # stepper demand a crew that is not the take's, and it is what the keeping row answers now.
    # **THE DEMAND-SIDE CAP RIDES THE ANSWER TOO**, so until one lands the party falls back to supply
    # alone: with no reply the payload's plateau is unknown, and clamping to a plateau nobody has quoted
    # would refuse a party this raid may well need.
    var capped := {"cap": assignable, "note": ""}
    if is_expedition:
        # The plateau is the reply's (`useful_cap`); the engagement-crew floor and the `assignable`
        # clamp stay client-side. With no answer yet the scan contributes 0, which reads exactly as
        # the old "the table carries no rows" case — supply alone binds.
        capped = SourceForecast.expedition_useful_cap(band, herd, _compose.hunt_floor(),
            int(raid_answer.get("useful_cap", 0)), assignable)
    elif not is_expedition:
        capped = _forecast_worker_cap(forecast, assignable)
    var cap := int(capped["cap"])
    # Auto-max on a FLOOR click — "give me everything this herd can spare at this floor": the
    # max-useful for that floor (clamped to idle below), which guarantees zero waste + the full rate.
    # Only ever set by a preset/slider click, never by a −/+ tick, so manual counts survive a rebuild.
    #
    #
    # **IT DOES NOT WAIT FOR THE REPLY, AND IT MUST NOT.** A raid's plateau is the reply's, so a fill
    # spent before the answer lands uses the supply-only fallback — but the line BELOW re-clamps the
    # count to the cap on every render, so the reply's real plateau still binds it a frame later and the
    # fill converges on `min(plateau, idle)` either way. Holding the one-shot until the reply DEADLOCKS
    # instead: the ask is skipped at a party of 0, which is exactly the state the fill exists to leave,
    # so the answer never comes and the sheet renders no forecast at all. (The DENIAL sheet's seed has
    # no such re-clamp and does wait — see `BandPanelController._fill_denial_compose_sheet`.)
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
    # **THE PRESET METRICS COME FROM THE SAME ANSWER AS EVERY OTHER FIGURE HERE** — `per_preset`, one
    # row per preset in the order they were asked for, priced for the party and kit this sheet is
    # composing. `{}` until the reply lands, which is the picker's supported degrade (as is a herd the
    # wire does not describe), so the rungs render bare rather than wrong.
    var floor_takes := {}
    if is_expedition:
        # One row per preset, answered in the SAME round trip as the composed row, so all three
        # buttons get a face without three sockets' worth of latency. `{}` until the answer lands —
        # the picker's supported degrade, so the rungs render bare rather than wrong.
        floor_takes = SourceForecast.expedition_policy_takes(band, herd,
            raid_answer.get("per_preset", []), _band_labor.grid_width(),
            _band_labor.wrap_horizontal())
    else:
        floor_takes = _hunt_floor_takes(herd, band, composed_improvement)
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
    # **THE CHART TAKES THE PRICED HERD, and it is the surface the kit report was ABOUT.** Both crew
    # pills — *clear it now* and *hold it after* — are this model's, and the stepper cap floors on the
    # `hold_crew`/`reach_crew` the FORECAST carries; priced on one side only, a pill would name a count
    # the `+` refuses, which is the panel arguing with itself.
    chart_model = SourceForecast.floor_chart_model(_hunt_priced_herd(herd, band),
        SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_floor(), _compose.hunt_count(), crew_label.to_lower(), lesson_known)
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
                        _hunt_priced_herd(_live_herd(herd_id, herd), band),
                        SourceForecast.SOURCE_KIND_HERD,
                        HudComposeVocab.BARE_FORECAST_PREFIX, floor, _compose.hunt_count(), crew_label.to_lower(), lesson_known),
                        _compose.hunt_count())))
    # The expedition branch spends this slot on the distance refusal — it is that branch's answer to
    # "why is this a party rather than a hunt?" — and the local branch on what the floor means for the
    # herd. **ONE hint table serves both webs and both branches now** (`HudFormat.floor_hint`): a
    # floor's meaning is its position relative to the food peak, which is the same fact for a patch and
    # a herd. The two things that genuinely differ are composed in, not tabulated: what stripping
    # COSTS (a patch reseeds; a herd is gone for good) and the fact that a detached party learns no
    # craft, so the above-peak bargain is not one an expedition can make.
    # **THE TRIP, RESOLVED BEFORE THE CREW ROW** — the same `_compose.hunt_count()` the stepper below
    # renders (the cap clamp is already done above, and nothing between here and the button moves it),
    # so the readout at the bottom and the floor hint at the top branch on ONE lookup rather than two.
    var trip: Dictionary = {}
    if is_expedition:
        target.add_child(HudWidgets.alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
    if is_expedition and raid_ready:
        trip = SourceForecast.hunt_trip_forecast(band, herd,
            raid_answer.get("at_composed", {}), _band_labor.grid_width(),
            _band_labor.wrap_horizontal())
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
    var on_crew_change := func(n: int) -> void:
        _compose.set_hunt_count(clampi(n, 0, cap))
        _build_herd_assign_controls(_live_herd(herd_id, herd), target)
    if is_expedition:
        # **THE EXPEDITION BRANCH TAKES THE INLINE `Party` ROW, the dock sheet's own control.** The
        # two hunting-party entry points were the same decision in two shapes — a `PARTY` section
        # heading with the stepper beneath it here, an inline labelled row there — so the herd
        # drawer's raid now reads Quarry / Party / Kit as one stack of labelled rows exactly as the
        # dock's does. **The LOCAL branches keep the section heading and their own crew NOUNS**
        # (`Hunters` / `Foragers` / `Herders`): that split is deliberate, so a managed herd's keepers
        # never read as a hunting party, and the crew targets that hang off the heading are a
        # resident crew's controls anyway.
        target.add_child(HudWidgets.build_party_stepper_row(_compose.hunt_count(), cap,
            on_crew_change))
    else:
        _mount_crew_row(target, live_hosts, crew_label,
            _compose.hunt_count(), _compose.hunt_count() < cap, on_crew_change, chart_model,
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
            _build_herd_assign_controls(_live_herd(herd_id, herd), target),
        herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    # **THE FIGHT, STATED BEFORE THE PARTY LEAVES** (`docs/plan_hunt_through_combat.md` §2.1 / §6.5),
    # directly under the crew that will fight it — both lines answer "is this crew the right size, and
    # can it win at all", which is what the stepper one row up has just posed.
    #
    # **THE ENGAGEMENT STAGE IS THE GATE ON BOTH, and that is what keeps a PEN and the whole PLANT web
    # byte-identical**: both publish `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked and a berry
    # does not fight back — so neither line renders and neither sheet moves. **The reach carries no
    # build term** (`docs/plan_standing_upkeep.md` §2.2): a Tame in flight is staffed by its own crew,
    # so the hunters beside it stalk exactly as they would with no build running.
    var engage_rate := float(herd.get(
        HudComposeVocab.BARE_FORECAST_PREFIX + SourceForecast.FORECAST_ENGAGE_RATE_KEY,
        SourceForecast.NO_ENGAGEMENT_STAGE))
    #
    # **THE HUNTERS-PER-ANIMAL LINE IS GONE** (reported from playtest). `One hunter brings 10 Wild Fowl
    # into contact.` is a fact about the SPECIES that never moved with anything the player was
    # dialling, sitting between the kit they had just chosen and the forecast they were reading; the
    # crew targets and the party cap already divide by the same reach, so the number it stated is
    # spent where it is actionable rather than announced where it is not.
    #
    # **AND THE GATE NOW RENDERS ONLY WHEN IT REFUSES.** `0.1 hunter-turns to bring one Wild Fowl
    # down` is the same removal: above the gate the effort figure is a species constant beside a
    # forecast that already prices the trip. The REFUSAL stays and is the whole point of keeping the
    # helper — a sub-gate party kills nothing at any headcount and still takes casualties, which reads
    # as a bug unexplained, and it is the honesty line a `none` kit depends on.
    if SourceForecast.has_engagement_stage(engage_rate):
        var quarry := _herd_label_for_id(herd_id)
        # **IT IS ASKED AT THE SELECTED KIT'S EFFECTIVE ATTACK, NOT THE BAND'S DEFAULT-KIT TIER.** The
        # picker one row up decides what these hunters carry, so a gate quoting the band's default kit
        # would refuse — or clear — a fight the composed party is not having.
        # **AND AT ITS ATTACK AGAINST *THIS ANIMAL*, NOT THE KIT'S BEST CASE.** A weapon bounded to a
        # size window grants nothing above it, so a snare reads the bare hand's attack against a Red
        # Deer — and the unbounded reading is what let a trapping sheet clear a gate the sim shuts.
        var gate := SourceForecast.hunt_gate_model_at(KitRoster.effective_attack_against(
            kits, KitRoster.kit_by_id(kits, kit_id), band,
            float(herd.get(KitRoster.QUARRY_BODY_MASS_KEY, 0.0))), herd, quarry)
        if bool(gate["blocked"]):
            var gate_label := HudWidgets.forecast_label("[color=#%s]%s[/color]" % [
                HudStyle.DANGER_HEX, String(gate["text"])])
            gate_label.set_meta(HudWidgets.HUNT_GATE_META, true)
            target.add_child(gate_label)
        else:
            # **THE FIGHT IS WINNABLE — BUT NOT BY EVERYBODY.** The gate above answers at ONE tier,
            # and on a partly-equipped band that tier is the best-armed crew's, so a cleared gate is
            # the reassuring half of a split party (issue #520). The complement, never the companion.
            #
            # **ASKED ABOUT THE COMPOSED PARTY, NOT THE BAND.** The gear covers a prefix of whoever
            # is sent, so a party small enough to fit inside the armed run has no split at all — and
            # a band-level sentence over this stepper would name more bare hands than there are
            # hunters in the party.
            HudWidgets.mount_hunt_crew_split(target, band, herd, quarry, kit_id,
                _compose.hunt_count())
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
    if is_expedition and not raid_ready:
        # **NO ANSWER YET, OR NONE COMING.** Where the kit-mismatch apology used to stand: the table
        # could not answer for this party, so the sheet said whose numbers it was withholding. The sim
        # answers for this party now — so the only reason to have no figures is that the round trip
        # has not landed (or has failed), and the line says which.
        #
        # Nothing derived renders either way. The combat gate two rows above still does: it is
        # composed from wire terms the band and herd already carry and stays honest at any tier and
        # with no reply at all. **The send stays LIVE and plainly styled** — the raid launches; we
        # simply cannot quote its length yet, and refusing a launch over a pending socket would be a
        # worse lie than the one this arc removed.
        target.add_child(HudWidgets.alloc_hint_label(
            HudComposeVocab.RAID_FORECAST_PENDING if String(raid_view["state"]) == ForecastQuery.STATE_PENDING
            else HudComposeVocab.FORECAST_FAILED_FORMAT % String(raid_view["error"])))
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
            HudWidgets.mount_trip_readout(target, trip, _herd_label_for_id(herd_id),
                _compose.hunt_floor())
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
        # THE IMPROVEMENT ROW — the second axis, ABOVE the readout that prices it. Nothing is offered
        # on an UNASSIGN, for the reason the forage sheet already records: what abandoning costs is
        # stated in the rung's own hint ("It must stay staffed or the herd goes wild again"), so a
        # second warning at the moment of unassigning states one fact twice.
        #
        # **THE CONTROL LEADS THE READOUT, and the order is the reading order**: the box is the last
        # thing composed, and the box's terms are stated in the readout beneath it. It used to follow
        # the readout, which put the payoff (then on the box's own face) BELOW the PER TURN box it
        # differs from and gave the two numbers no visible relationship at all.
        if not is_unassign:
            # **THE CONTROL IS IN THE LIVE SET, because its turn estimate is priced at the floor.**
            # The crew half tracks on its own — a stepper tick rebuilds the whole sheet — but a floor
            # DRAG must not, so the box is rebuilt in place by the registry exactly as the yields row
            # and the crew targets are. The registry's own rule decides it: anything whose value
            # depends on the floor belongs in it.
            var improvement_host := VBoxContainer.new()
            improvement_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
            target.add_child(improvement_host)
            _register_live(live_hosts, improvement_host, chart_model, _compose.hunt_count(),
                func(host: Container, live: Dictionary, crew: int) -> void:
                    _build_improvement_control(SourceForecast.LABOR_KIND_HUNT, herd,
                        HudComposeVocab.BARE_FORECAST_PREFIX, _live_floor(live),
                        composed_improvement, band, crew,
                        _build_gear_for(band, SourceForecast.LABOR_KIND_HUNT),
                        # **DOES ANYBODY WORK THIS HERD?** — `is_noop`, which is exactly *no standing
                        # crew and none composed*, and which the dead commit button already forks on.
                        # It picks WHICH remedy the offered rung states (§4.7a ③).
                        not is_noop, host as VBoxContainer,
                        # **THE ESTIMATE IS QUOTED AT THE ACTING BAND'S OWN `builders` POOL**
                        # (`docs/plan_standing_upkeep.md` §4). It was this sheet's retired BUILDERS
                        # stepper; it is the band's standing role now, so the reading moves when the
                        # player staffs that role and never when they touch this sheet. It is the
                        # PICKED band's, not a fold across the source's workers: this is the band the
                        # commit names, so it is the band whose hands would raise the entry.
                        #
                        # **AND IT IS PENDING-AWARE, like the Builders card beside it.** It read the
                        # CONFIRMED row, so a player who had just staffed the role read a card saying
                        # `2` next to a sheet saying *"nobody is on this band's Builders role"* until
                        # the turn resolved — two surfaces on one screen, and the stale one phrased as
                        # an accusation. A pending role edit cannot be refused (`assign_labor` clamps
                        # rather than rejects), so the optimistic read can never suppress a real
                        # warning.
                        int(_band_labor.effective_role_workers(
                            band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))))
        # THE ONE RESOLUTION OF THIS SHEET'S DEAL, spent by the readout below. An unassign quotes
        # none: the control above is not built either, so there would be no rung on the card for the
        # rows to be about.
        var deal_rung := "" if is_unassign else _improvement_deal_rung(
            SourceForecast.LABOR_KIND_HUNT, herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            composed_improvement)
        var deal_payoff := "" if deal_rung == "" else _improvement_payoff_terms(
            herd, SourceForecast.LABOR_KIND_HUNT, HudComposeVocab.BARE_FORECAST_PREFIX,
            deal_rung, band)
        # The averaging-window disclaimer USED TO STAND HERE, as a wrapped body line under the hint: the
        # delivered rate is a long-run average of lumpy whole-animal delivery. It is a caveat on ONE
        # number, so it now rides the RUNG's tooltip beside the metric it qualifies (`_hunt_floor_takes`
        # fills the take pair's `note`) — the panel is where the hunt sheet could least afford a sentence
        # the forage sheet has no counterpart for. The window computation is unchanged.
        # **THE READOUT** — the LIVE per-turn take for the floor being composed (no carry cap on a
        # local hunt, so turns-to-fill is meaningless — the delivered rate is the number that decides
        # it), then the rung's payoff, then the verdict (§7.1: which of the two
        # independent statements is binding, the crew or the floor), then the idle-crew note (§7.2 —
        # reported, never acted on) and the teaching line. The take is recomposed from the LIVE floor,
        # so the numbers the player is dragging toward move while the drag runs.
        _mount_readout(target, live_hosts, chart_model, _compose.hunt_count(),
            func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
                return _hunt_yield_model(band, herd, floor_value, crew,
                    composed_improvement, reaches),
            SourceForecast.LABOR_KIND_HUNT,
            _improvement_deal_row(SourceForecast.LABOR_KIND_HUNT, herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, band, deal_rung, deal_payoff))
        # **NO KEEPING ROW** (`docs/plan_standing_upkeep.md` §2.5) — a managed herd is held by the
        # band's `husbandry` role, not by a crew on this sheet, so there is no stepper here to point
        # at it. What this herd's share of that pool covers, and where it falls short, is stated on
        # the herd drawer's `Keeping:` / `At risk:` rows.
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
            # **THE EMPTY-RAID GUARD READS THE ANSWER ON SCREEN**, and only when there is one: with no
            # reply the button is plainly styled and enabled, and refusing the launch here would be a
            # silent no-op the player has no way to explain. It re-reads `trip` rather than re-asking,
            # so the guard and the readout above it can never disagree.
            if raid_ready and SourceForecast.hunt_trip_returns_empty(trip):
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
                # The kit the party walks out with, and the job default `Main` omits the token for.
                "kit_id": kit_id,
                "default_kit_id": default_kit,
            })
            # Committing is the end of the compose act — return to the read state (§15).
            close_compose_sheet())
    else:
        # **ONE COMMAND, AND IT IS `assign_labor`** (`docs/plan_standing_upkeep.md` §4.7a ①). The
        # improvement verb that used to follow it is the Work tab's now. `composed_improvement` still
        # travels — it is recorded on the OPTIMISTIC OVERLAY and never on the wire, so a crew edit
        # does not blank a build the herd is already running.
        assign_btn.pressed.connect(func() -> void:
            _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, _compose.hunt_count(),
                herd_x, herd_y, herd_id, _compose.hunt_floor(), "", composed_improvement, kit_id)
            close_compose_sheet())
    target.add_child(assign_btn)

## Mount the kit row where a sheet wants it — a no-op when the roster offers this job no kit at all,
## so a sheet rendered before the first snapshot (or against a world whose roster does not cover the
## verb) is byte-identical to what it was before the picker existed. The Band panel's dock sheets keep
## the identical helper; the two controllers share no base, and one Callable to reach the other's copy
## would be an injection that buys nothing.
##
## `quarry` / `prefix` are what the greying is resolved against and are omitted by the forage sheet,
## which has no animal to be inapplicable to.
func _mount_kit_row(target: VBoxContainer, kits: Array, job: String, kit_id: String,
        default_kit: String, band: Dictionary, on_pick: Callable, quarry: Dictionary = {},
        prefix: String = "") -> void:
    var row := KitRoster.build_kit_row(kits, job, kit_id, default_kit, band, on_pick, quarry, prefix)
    if row != null:
        target.add_child(row)







## Each FLOOR PRESET's per-turn take on this forage patch — the ceiling at that floor, composed by the
## shared `SourceForecast.forecast_inputs` (per turn at output 1.0, exactly as the hunt twin), for the
## FORAGE picker's preset readout. The plant twin of `_hunt_floor_takes`, so both pickers wear the
## same button metric. A patch the wire does not describe is skipped.
##
## **BOTH ACCOUNTS (#426), and the ZERO lands in the right one (§7.7).** This once handed the shared
## joiner an explicit `0.0` for its non-food account, so a patch that pays no food rendered
## `0.00 food` at every floor and read exactly like the worthless-source lie #337 removed from the
## hunt picker. Each account comes off the patch's own per-biomass vector and renders only when
## non-zero, and when the take is empty in BOTH the surviving zero is the account the patch actually
## pays. (A third, trade-goods account rode here until arc #527 retired it.)
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
func _forage_floor_takes(tile_info: Dictionary, band: Dictionary) -> Dictionary:
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
        var forecast := _forage_forecast(tile_info, band, SourceForecast.floor_for_preset(preset))
        if not bool(forecast["known"]):
            continue
        # **THE MATERIAL CEILING COMPOSES AT THIS PRESET'S OWN FLOOR**, by the same
        # `max(0, B − floor·K) × rate` rule the two scalars take — which is the whole reason
        # `material_per_biomass` is a per-biomass RATE rather than a pre-composed ceiling. **No lock
        # applies to it**: the wild-fodder gate is Foddering's, a claim about FEED and about the
        # faction's knowledge of penning, and a gatherer banks a cash crop's fibre whether or not it
        # has ever kept a pen.
        takes[preset] = SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]),
            0.0 if locked else float(forecast["ceiling_fodder"]), zero_account,
            forecast["material_ceiling"])
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

## **WHAT THIS CROP PAYS PER TURN, PER MATERIAL, under `policy`** (arc #527) — the replacement for the
## retired trade scalar, and deliberately NOT a restoration of it: `[{material_id, amount}]`, one row
## per material, never a total. `SourceForecast.flora_basket_entries` has already normalized the wire
## rows, so this only picks the rung.
##
## **PER RUNG, exactly like the fodder payoff above** — and here the two rungs differ in KIND rather
## than by a factor. A sown Field is 100% its crop (#433), so a grain Field quotes NOTHING; a tended
## patch is a weeded basket whose volunteers are still standing, so a tended grain honestly quotes the
## fibre its neighbours pay. Neither rung's answer may be inferred from the other's.
##
## **EMPTY IS "THIS PLANT PAYS NO MATERIAL", WHICH IS A REAL ANSWER** — the caller renders no clause,
## never a `0.00`, which would read as a cash crop that pays badly.
func _flora_entry_material_payoff(entry: Dictionary, policy: String) -> Array:
    if policy == HudConst.LABOR_POLICY_SOW:
        return entry.get("sow_material_payoff", []) as Array
    return entry.get("cultivate_material_payoff", []) as Array

## The rung noun the payoff tooltips name — "a tended patch" under Cultivate, "a sown field" under Sow.
## These payoffs are per-rung, so a tooltip that named the wrong rung would restate the very bug the
## per-rung split fixed.
func _flora_rung_noun(policy: String) -> String:
    return String(HudFloraVocab.FLORA_CROP_RUNG_NOUNS.get(
        policy, HudFloraVocab.FLORA_CROP_RUNG_NOUN_FALLBACK))

## **The row face for one basket entry: every account this plant actually pays, none it does not.**
## The base share row, then a ratio / hay clause and ONE CLAUSE PER MATERIAL, each gated by whether its
## component is really there — food leading, the shared render-only-when-non-zero rule
## (`SourceForecast.has_component` is THE gate, never a bespoke threshold).
##
## The ratio keeps its own `> FLORA_CROP_RATIO_NONE` test rather than `has_component`: `0` there is the
## **cannot-climb sentinel**, not a small rate, and the sentinel must never print as `0.0×`.
##
## **THE MATERIALS ARE ROWS, AND THEY STAY ROWS** — one clause each, in the wire's order. Summing them
## would be the retired trade scalar under a new name (`HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT`
## carries the long form), and an EMPTY list renders no clause at all rather than a `0.00`.
func _flora_row_face(crop_name: String, percent: int, ratio: float, fodder: float,
        materials: Array) -> String:
    var face := HudFloraVocab.FLORA_SHARE_FORMAT % [crop_name, percent]
    if ratio > SourceForecast.FLORA_CROP_RATIO_NONE:
        face += HudFloraVocab.FLORA_CROP_RATIO_CLAUSE_FORMAT % ratio
    if SourceForecast.has_component(fodder):
        face += HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT % fodder
    for row_variant in materials:
        var row: Dictionary = row_variant
        var amount := float(row[SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY])
        if not SourceForecast.has_component(amount):
            continue
        face += HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT % [
            amount, String(row[SourceForecast.MATERIAL_PAYOFF_ID_KEY])]
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
    var fodder := float(deal["payoff_fodder"])
    # **THE MATERIALS ARE THE CROP'S ALONE — the patch quote has no species-blind twin for them**, so
    # an uncommitted picker with no selection resolved states none. That is the honest answer rather
    # than a gap: the sim quotes a material payoff PER PLANT and there is no basket-wide figure to
    # fall back to, where `payoff` / `payoff_fodder` genuinely have one.
    var materials: Array = []
    if species != "":
        for entry in entries:
            if String(entry["species"]) != species:
                continue
            payoff = _flora_entry_payoff(entry, rung)
            fodder = _flora_entry_fodder_payoff(entry, rung)
            materials = _flora_entry_material_payoff(entry, rung)
            break
    # **THE FOOD ZERO SURVIVES BESIDE A MATERIAL CLAUSE, AND THAT IS THE READING.** A sown Field of
    # cotton pays exactly `0.00 food`, and stating it next to `0.29 fibre` is the whole land-use
    # bargain in one row — which is what the retired trade scalar was standing in for.
    var terms := SourceForecast.picker_products(payoff * output, fodder * output)
    for row_variant in materials:
        var row: Dictionary = row_variant
        var amount := float(row[SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY]) * output
        if not SourceForecast.has_component(amount):
            continue
        terms += HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT % [
            amount, String(row[SourceForecast.MATERIAL_PAYOFF_ID_KEY])]
    return terms

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

## **RETIRED — `_build_crop_picker`, the compose sheet's CROP LIST**
## (`docs/plan_standing_upkeep.md` §4.7a ③). One pressable row per plant in the tile's basket, with an
## illegal entry greyed beside its reason and the committed one marked-and-locked.
##
## Ray, from play: *"The CROP TO TEND shouldn't be a selection here as the user can't do the
## cultivate here."* He is right, and §4.7a ③ had already decided it — **the queue row is the job's
## SETTINGS**, so the crop rides the entry it belongs to, beside the kit, on the tab that owns the
## queue. `BandPanelController._build_queue_crop_picker` is where it went.
##
## **THE CROP IS STILL COMMITTED BY THIS SHEET'S OWN `assign_labor`**, as its `species` token, exactly
## as before — that is why moving the CONTROL stranded nothing. `_resolve_crop_selection` still runs
## every render, so a composition can never name a plant this tile and rung cannot take, and the
## readout's `ONCE TENDED` terms still follow the selected crop through `_crop_payoff_terms`.
##
## **`HudStyle.apply_button`'s `selected_when_disabled` LOSES ITS LAST CALLER with it.** It was
## written for issue #420's standing-but-gated rung, which went with the stance/improvement split, and
## this picker's committed row was the second; it survives as a `HudStyle` flag because
## marked-and-locked is a real state, and the queue row's picker does not need it — an `OptionButton`
## marks its own selection natively.
##
## The `extra_rows` hook it rode retired with it: the improvement control mounts nothing beneath
## itself on any state now.

func _build_forage_assign_controls(tile_info: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _forage_compose_available(tile_info):
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    # The band the sheet DEFAULTS to: whoever already forages this patch, else the shared ladder's answer.
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.effective_forage_workers(candidate, x, y) > 0)
    # ONE key for this patch: the compose source key and the sheet's subject key are the same string
    # by definition, and the rebuild closures below re-resolve the LIVE tile through it (`_live_tile_info`).
    var subject_key := _forage_source_key(tile_info)
    # When the selected tile changes, default the actor band to the resolved band (and re-seed
    # the count from its staffing); otherwise preserve the picked band + count across the
    # per-snapshot re-renders of the same tile.
    var source_changed := _compose.forage_key() != subject_key
    if source_changed:
        _compose.begin_forage_source(subject_key, int(resolved.get("entity", ComposeState.NO_BAND_ENTITY)))
    var band := _band_labor.player_band_by_entity(_compose.forage_band())
    if band.is_empty():
        band = resolved
        _compose.set_forage_band(int(band.get("entity", ComposeState.NO_BAND_ENTITY)))
    # THE SECOND AXIS's standing value (issue #442) — what this patch is already BUILDING, DERIVED
    # from its meters with the assignment's own `improvement` reaching the derivation as a pending
    # declaration (`docs/plan_standing_upkeep.md` §2.4). The stored field alone cannot answer: a rung
    # that eroded back below its cost is building again with nothing declared, and a crew edit there
    # has to name the verb the sim is actually running or the commit sends nothing.
    var standing_improvement := SourceForecast.build_verb(tile_info,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE,
        _band_labor.improvement_for_forage(band, x, y))
    # **THE ACTOR BAND CHANGING RE-SEEDS THE COMPOSITION, exactly as the SOURCE changing does** — see
    # the hunt sheet's twin. Evaluated AFTER `band` resolves, so it compares against the band just
    # picked rather than the one being left.
    var band_entity := int(band.get("entity", ComposeState.NO_BAND_ENTITY))
    if source_changed or _compose.forage_seeded_band() != band_entity:
        # **BOTH CREWS AND THE CROP SEED FROM THE BAND'S ROW** (`docs/plan_standing_upkeep.md`
        # §2.2). `seed_forage` used to clear the crop outright — a crop pick belongs to the PATCH it
        # was made on, and a new tile has a different basket — but the assignment carries the player's
        # own `species` now, and that is the SELECTION rather than the ground's commitment: it exists
        # from the moment they chose, before any crew has worked the patch and therefore before
        # `patch_committed_species` says anything at all. Seeding from it is the only way a sheet
        # reopened over unworked ground shows the crop the player picked.
        var staffed := _band_labor.workers_for_forage(band, x, y)
        _compose.seed_forage(staffed if staffed > 0 else HudConst.WORKER_STEP,
            _band_labor.floor_for_forage(band, x, y), standing_improvement,
            _band_labor.species_for_forage(band, x, y))
    # The effective (pending-aware) standing crew, which the commit's unassign/no-op test reads below.
    var current := _band_labor.effective_forage_workers(band, x, y)
    # **THE HANDS THIS SHEET MAY SPEND** — idle plus the crew this band already has on this patch
    # (`HudBandLaborState.source_crew_pool_forage`), the ceiling `assign_labor` is judged against.
    # **THE BUILD NO LONGER SHARES IT** (`docs/plan_standing_upkeep.md` §2.5): the sheet had two
    # steppers each capped at what the other left, and a verb states no hands now.
    var crew_pool := _band_labor.source_crew_pool_forage(band, x, y)
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
    # completes, the improvement control below drops to its DONE label — and the composed verb sat on
    # unread and re-issued itself on the next commit. `SourceForecast.build_verb` is the same
    # derivation that control makes, so the numbers and the label can no longer say different things
    # about the same rung — and it works in the other direction too, adopting a build the METERS say
    # is in flight even where nothing was ever declared.
    # …and a declaration made from the Work board's `⌃` this frame outranks the composition — the
    # plant twin of the hunt sheet's note.
    var composed_improvement := SourceForecast.build_verb(tile_info,
        HudComposeVocab.FORAGE_FORECAST_PREFIX, SourceForecast.SOURCE_KIND_FORAGE,
        _declared_or_composed(_band_labor.pending_improvement_for(
            band, SourceForecast.LABOR_KIND_FORAGE, x, y, ""),
            _compose.forage_improvement()))
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
    # than Sustain's, so switching stance moves the cap; a rung going up moves it too).
    var forecast := _forage_forecast(tile_info, band, _compose.forage_floor())
    # …and floored on the rung's OWN build crew, the plant twin of a managed herd's herding crew. The
    # dip and the cap otherwise fight: dividing the dipped ceiling collapses the count, so committing
    # to a 25-turn improvement would ask for fewer hands than gathering the same ground — and the sim,
    # which takes `max(build crew, take crew)`, would then report the row overstaffed at the very count
    # this sheet capped it to.
    # **NO RUNG FLOOR UNDER THE TAKE CAP ANY MORE** (`docs/plan_standing_upkeep.md` §2.2). The plant
    # rungs published a `crew_needed` that had to RAISE this cap, because the ceiling it divides was
    # the DIPPED one and a 25-turn improvement therefore asked for fewer hands than gathering the same
    # ground. Both terms are retired: the take is undipped, so the quotient is the honest count, and
    # what a build costs is the builders' own stepper.
    var capped := _forecast_worker_cap(forecast, crew_pool)
    var cap := int(capped["cap"])
    # Auto-max on stance select — "give me everything this patch sustains": jump to the max-useful for
    # the stance (clamped to available below). Only ever set by a stance click, never by a −/+ tick.
    if _compose.consume_forage_autofill():
        _compose.set_forage_count(cap)
    _compose.clamp_forage_count(cap)
    var forage_takes := _forage_floor_takes(tile_info, band)
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
    # Priced at the chosen kit, exactly as the hunt sheet's is and for the same reason: the two crew
    # pills are this model's and the stepper cap is the forecast's, so both sides must know the basket.
    var chart_model := SourceForecast.floor_chart_model(_forage_priced_patch(tile_info, band),
        SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        _compose.forage_floor(), _compose.forage_count(),
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
                        _forage_priced_patch(_live_tile_info(subject_key, tile_info), band),
                        SourceForecast.SOURCE_KIND_FORAGE,
                        HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, _compose.forage_count(), crew_label.to_lower(),
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
    #     "Forage"), and the readout's deal block states the PAYOFF alone — the `now` row is dropped,
    #     since a staffing-scaled term beside a staffing-free one reads as a sequence at zero crew.
    #   • 0 on a tile it DOES work → the sim's unassign (server.rs: "Unassigning (workers == 0) is
    #     always allowed"). Live button, renamed, and NO "assign to begin" line — a panel whose button
    #     says Unassign above a line reading "assign to begin" tells the player two opposite things.
    # Gating on the raw count instead would fix the no-op and break the unassign the Work zone needs.
    var is_unassign := _compose.forage_count() <= 0 and current > 0
    var is_noop := _compose.forage_count() <= 0 and current <= 0
    # THE IMPROVEMENT ROW — the second axis, ABOVE the readout that prices it, so the box and the crop
    # beneath it are read first and the terms they buy immediately after. Nothing is offered on an
    # UNASSIGN: what abandoning costs is already on the card in the rung's own hint ("It must stay
    # staffed or it goes feral"), so a second warning here would state one fact twice.
    if not is_unassign:
        # **THE CROP PICKER IS NOT HERE ANY MORE** (`docs/plan_standing_upkeep.md` §4.7a ③). It rode
        # beneath the box on the reasoning that which crop the rung commits to is part of the same
        # decision — and the decision left this sheet, so the crop went with it to the job's own BUILD
        # QUEUE row. Ray, from play: *"The CROP TO TEND shouldn't be a selection here as the user can't
        # do the cultivate here."* The crop is still COMMITTED by this sheet's `assign_labor`, as its
        # `species` token, which is why moving the control stranded nothing.
        #
        # **IN THE LIVE SET, for the reason the hunt sheet's twin is** — the control's ink is priced at
        # the floor as well as at the crew, and a floor DRAG may not rebuild the sheet.
        var improvement_host := VBoxContainer.new()
        improvement_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        target.add_child(improvement_host)
        _register_live(live_hosts, improvement_host, chart_model, _compose.forage_count(),
            func(host: Container, live: Dictionary, crew: int) -> void:
                _build_improvement_control(SourceForecast.LABOR_KIND_FORAGE, tile_info,
                    HudComposeVocab.FORAGE_FORECAST_PREFIX, _live_floor(live), composed_improvement,
                    band, crew, _build_gear_for(band, SourceForecast.LABOR_KIND_FORAGE),
                    # The plant twin — see the hunt sheet's note (§4.7a ③).
                    not is_noop, host as VBoxContainer,
                    # The plant twin — the ACTING band's own pool, pending-aware; see the hunt
                    # sheet's note for both halves.
                    int(_band_labor.effective_role_workers(
                        band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))))
    # **THE PAYOFF FOLLOWS THE SELECTED CROP, AND IT IS RESOLVED EXACTLY ONCE** (issue #419). The
    # readout's payoff row and the crop picker one control up must read ONE seam or they quote
    # different crops — which is the whole defect that issue named, in its second home. `deal_rung` is
    # the rung the picker is rendered for wherever there is one to render (composed verb, else the
    # ungated offer), so the terms and the list can only ever be about the same rung.
    var deal_rung := "" if is_unassign else _improvement_deal_rung(
        SourceForecast.LABOR_KIND_FORAGE, tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        composed_improvement)
    var deal_payoff := "" if deal_rung == "" else _crop_payoff_terms(
        tile_info, basket, _compose.forage_species(), band, deal_rung)
    # **THE READOUT** — the take, the rung's payoff, the verdict, and the idle-crew
    # note + teaching line, in one bounded box (§7.1, §7.2). The take is recomposed from the LIVE
    # floor, which is what lets the numbers the player is dragging toward move while the drag runs.
    _mount_readout(target, live_hosts, chart_model, _compose.forage_count(),
        func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
            return _forage_yield_model(band, tile_info, floor_value, crew, composed_improvement,
                reaches),
        SourceForecast.LABOR_KIND_FORAGE,
        _improvement_deal_row(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, band, deal_rung, deal_payoff))
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
    # **NO KEEPING ROW** (`docs/plan_standing_upkeep.md` §2.5) — a tended patch or a Field is held by
    # the band's `agriculture` role, not by a crew on this sheet. What this patch's share of that pool
    # covers, and where it falls short, is stated on the land card's `Keeping:` / `At risk:` rows.
    var assign_btn := Button.new()
    # The commit verb follows the crew noun the stepper above just asked for — `Forage` for foragers,
    # `Tend` for tenders — keyed off the ONE resolved label, exactly as the hunt web's noop hint is.
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    assign_btn.text = HudComposeVocab.UNASSIGN_BUTTON if is_unassign \
        else String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(crew_label, ""))
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range or is_noop
    # **ONE COMMAND, AND IT IS `assign_labor`** — the plant twin of the hunt sheet's note. The CROP
    # rides it as its `species` token exactly as before, which is why moving the declaration out did
    # not strand the crop picker: the crop is part of the assignment, not part of the verb.
    assign_btn.pressed.connect(func() -> void:
        _emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, _compose.forage_count(),
            x, y, "", _compose.forage_floor(), _compose.forage_species(),
            composed_improvement, forage_kit_id)
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
##
## **THE COMPOSITION DIES WITH THE SHEET, and forgetting the SOURCE KEY is how it dies.** `clear_composing`
## only ever dropped the *which sheet is open* half; the *what is dialled* half — the crews, the floor,
## the crop, the declared rung — is keyed on the source and survived, so a player who dropped HUNTERS
## from 4 to 2, closed without committing and reopened was shown 2 over a band that still had 4 on the
## herd. An uncommitted number that outlives its sheet is a promise nothing in the game is keeping.
##
## **IT RE-ARMS THE EXISTING SEEDING PATH RATHER THAN ADDING A SECOND ONE.** `_build_*_assign_controls`
## already re-seeds from the band's own row whenever the source changes; `reset_*_source` clears the key
## that source test compares against, so the next open takes exactly that branch. There is deliberately
## no separate seed-on-open call — two seeding paths over one composition is how a sheet comes to open
## on a crew nobody has.
##
## **BOTH are reset, not just the kind that was open.** One sheet is on screen at a time and this is the
## only place either composition ends, so clearing the pair leaves no way for the other web's stale
## dial to survive into a later session.
func _on_compose_sheet_closed() -> void:
    _compose.clear_composing()
    _compose.reset_forage_source()
    _compose.reset_hunt_source()
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
## **THE RAID QUESTION, COMPOSED AND ASKED.** One place, so the ask and the read cannot describe two
## different raids: the key it asks under is the key it reads back, and both are built here.
##
## `max_party` is the band's own idle workforce — the stepper's ceiling — and it bounds the reply's
## `useful_cap` plateau scan, which walks `1..=max` CONTIGUOUSLY. It is deliberately not a rules cap:
## there is no such thing any more.
func _raid_forecast_view(band: Dictionary, herd_id: String, kit_id: String, party: int,
        floor: float, max_party: int) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_TRIP, band_id, herd_id)
    var key := ForecastQuery.key_of(subject, kit_id, party, floor)
    # A party of 0 is `invalid_party` server-side and there is no raid to project, so it is never
    # asked — the sheet's Send is already disabled there and the readout has nothing to say.
    if party > 0 and band_id != HudConst.NO_BAND_ID and herd_id != "":
        _forecast_query.ask(ForecastQuery.KIND_HUNT_TRIP, subject, key, {
            "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id,
            "herd_id": herd_id,
            "kit_id": kit_id,
            "party_workers": party,
            "floor": floor,
            "preset_floors": SourceForecast.preset_floors(),
            "max_party_workers": max_party,
        })
    return _forecast_query.view(subject, key)

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