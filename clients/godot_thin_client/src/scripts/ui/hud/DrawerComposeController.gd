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
##     while a command with no other emitter would be a signal of this controller's own.
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
# ⛔ RETIRED — **`extend_pen_requested` on THIS controller.** The signal itself lives on
# `BandPanelController` now (`docs/plan_standing_upkeep.md` §4.9 item 12c): the ring is declared from
# the work row's standing-rung mark, not from a button on the tile card. `HudLayer` still carries the
# outbound signal of the same name and `Main.format_extend_pen` is untouched — only the emitter moved.

## **RETIRED — `improvement_requested` AND `unqueue_requested`, THIS SHEET'S WHOLE COMMITTING HALF**
## (`docs/plan_standing_upkeep.md` §4.7a ①). The pair was the second axis's command
## (`cultivate` / `sow` / `tame` / `corral`) and its withdrawal, sent by the improvement checkbox's
## tick and untick.
##
## What this sheet keeps is the FORECAST — the rung, what it would PAY once built, and its refusals —
## which is what a 28px work row could never hold and what makes it the place a rung is JUDGED. (Its
## COSTS went to the Work tab too; see `_improvement_offer_phrase`.)

## **THE PLAYER ASKED FOR THE WORK TAB** — the `Work tab` link on an OFFERED rung's line
## (`docs/plan_standing_upkeep.md` §4.7a ①). `HudLayer` relays it to `BandPanelController.show_work_tab`;
## this controller must not reach the dock itself, panel-to-panel coupling being the coordinator's.
##
## **IT CARRIES THE ACTING BAND'S `entity`, and for one release it carried nothing.** Reported from
## play: on the FACTION page the link switched to the FACTION's Work tab — a rollup, with no `⌃` on it
## — so the sentence sent the player somewhere that could not do what it promised. The acting band is
## the one the sheet's `Band:` picker names, which is the band whose `⌃` declares this rung;
## `ComposeState.NO_BAND_ENTITY` where none resolves, which the far end treats as
## *switch the tab and jump nowhere*.
##
## **`entity` rather than `band_id`**: nothing here builds a command, and the panel resolves a band by
## entity (`player_band_by_entity`) exactly as every overlay reader does.
##
## **WHAT IT STILL DOES NOT NAME IS THE SOURCE.** Focusing this patch's own ROW on that board needs a
## public focus seam keyed by the source, which this signal does not have; the board's `⌃ N ready`
## chip is what finds the row once the player is on it.
signal work_tab_requested(band_entity: int)

## ⛔ **THE ROUTE BRANCH'S DECLARATION — `grade|pave <faction> <band> <x> <y>`** (arc #532 slice 13),
## relayed by `HudLayer` straight onto `improvement_requested` and formatted by
## `Main.format_improvement`, both unchanged. It is a signal because the command has no other
## emitter, so there is no HudLayer helper to route it through.
##
## **IT IS DELIBERATELY NOT CALLED `improvement_requested`.** That name was RETIRED from this
## controller (see the block below) when the rung checkbox stopped being the commit, and reusing it
## would read as that pair coming back — this is a different act, from a different surface, carrying a
## band token none of those payloads had.
##
## **HudLayer relays it WITHOUT the optimistic overlay write** the work row's `⌃` gets: that write is
## keyed to a band's LABOR ROWS and a road has none, so there is nothing to record and nothing to roll
## back. The payload therefore carries no `pending_entity`.
signal road_improvement_requested(payload: Dictionary)

## ⛔ **PUT A ROAD DOWN — `abandon <faction> <x> <y>`, and it is the only undo a BUILT road has.**
## `unqueue` withdraws a DECLARATION and nothing else; the moment any work is banked the verb that
## releases a keeper is `abandon`, which was command-line only — so a road handed to the wrong band
## could not be taken back from the UI at all.
##
## Relayed by `HudLayer` onto its own `abandon_requested`, which `Main.format_abandon` builds. **No
## optimistic write**, exactly as the declaration beside it: a road has no labor row to shadow.
signal road_abandon_requested(payload: Dictionary)

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
# The HUD CanvasLayer, so this RefCounted controller has a node to reach the tree through. The
# compose sheet is NOT parented onto it directly — it goes into `HudLayer.compose_host()`, the
# dedicated compose CanvasLayer above the event dock's (see `_ensure_compose_sheet`).
var _host: Node = null

# --- Scene nodes (handed in by HudLayer) ---
# The HERD drawer-action container this controller FILLS. (The LAND drawer has no container: its sheets
# open from the door rows of `%TileDetail`, see `open_door`.)
var _herd_assign_controls: VBoxContainer = null
# The selection card, READ-ONLY: the rect the sheet floats beside (`_compose_anchor_rect`).
var _tile_panel: PanelCard = null
## The Band panel (the work strip's row producer) and the shared rung card, set by `Hud` once both exist.
var _bandpanel: BandPanelController = null
var _rung: RungCardController = null

# --- The three retained HudLayer helpers, injected as Callables (see the class header) ---
# Each is reached through a typed adapter below rather than called raw: `Callable.call` returns
# `Variant`, which would push an untyped value into every consumer here.
var _resolve_assign_band_fn: Callable
var _herd_label_for_id_fn: Callable
var _emit_assign_labor_fn: Callable

## The chart model the open sheet was last built from. Its two crew targets (`crew_to_clear` /
## `crew_to_hold`) are no longer drawn as pills - the sim staffs the crew from the floor (#871) - but the
## verdict's reaching crew and the forecast crew come from the same model, and the harness reads them
## here to pin that arithmetic.
var _last_chart_model: Dictionary = {}

## One of the open sheet's chart-model crew targets, `SourceForecast.NO_CREW_ANSWER` where the model prices
## none, and `ABSENT` where the sheet drew no chart at all.
const CHART_TARGET_ABSENT := -2
func chart_crew_target(key: String) -> int:
    if not _last_chart_model.has(key):
        return CHART_TARGET_ABSENT
    return int(_last_chart_model[key])

## A priority mark / a stop on a source the band works (#871), relayed through HudLayer to `Main`.
signal work_priority_requested(payload: Dictionary)
signal stop_labor_requested(payload: Dictionary)
## A declared build was STOPPED from the site's own sheet - the build crew control's `Stop building`.
## Relayed to HudLayer.unqueue_requested (`Main.format_unqueue`), the same edge the work row's in-flight
## chip uses. Payload `{ faction, x, y, herd_id, material, road, kind, pending_entity }`.
signal unqueue_requested(payload: Dictionary)

## The crew each sheet prices its source at: the first-pass guess while the forecast is being struck, the
## staffed crew once it has been. A member rather than a parameter because the priced source is reached
## from many readouts, and every one of them must price at the same crew.
var _hunt_priced_crew: int = KitRoster.KIT_CREW_UNCOMPOSED
var _forage_priced_crew: int = KitRoster.KIT_CREW_UNCOMPOSED

# --- Owned state (moved off HudLayer) ---
# The floating compose sheet NODE. Which source it is composing is pure data and lives on `_compose`
# (`kind()` / `subject()`); this handle is a scene node, so it is owned here beside its lifecycle.
var _compose_sheet: ComposeSheet = null
# The drawer-actions diff caches: the shape signature last rendered for each drawer, so an unchanged
# per-snapshot restate PATCHES the existing nodes instead of freeing + rebuilding them (the reflow
# that flashes). Zero readers outside this controller, so they travelled with the builders.
var _herd_drawer_shape: Array = []
## The road ladder's Window and its inner `MarginContainer`, built on the first press of a `Road` door
## and reused thereafter. **A Window renders over the dock and changes no layout**, which is why the
## height-capped selection card can host a ladder at all; the margin is held so a re-open clears the
## TRACK rather than the chrome that hosts it (`queue_free` is deferred, so freeing the margin renders
## correctly once and opens onto an empty card ever after).
var _road_ladder: PopupPanel = null
var _road_ladder_body: MarginContainer = null
## ⛔ **WHICH BAND THE OPEN CARD IS ACTING FOR, and it is the card's own state rather than the HUD's.**
## The keeper of a road is decided by the verb that raises it, so the ladder had to grow a picker (see
## `_open_road_ladder`); the pick lives only as long as the card, because the next road is a different
## question. `ComposeState.NO_BAND_ENTITY` between opens.
##
## **THE TILE AND THE ROW INDEX RIDE WITH IT** so a pick can re-render the rows in place: every gate on
## the track is resolved against the acting band, so a stale row would offer a rung the newly-chosen
## band cannot have.
var _road_ladder_band_entity: int = ComposeState.NO_BAND_ENTITY
var _road_ladder_tile: Dictionary = {}
var _road_ladder_index: int = -1


## **THE FORECAST QUERY SEAM**, injected by `HudLayer` after construction (`set_forecast_query`).
## Every sim-answered figure on the sheets comes through it — the crew-take curves and the work-party
## forecast.
var _forecast_query: ForecastQuery = null

# ---- WHAT A LIVE FLOOR DRAG HOLDS (see `_refresh_floor_live` and `_drag_crew_take`) --------------
#
# **A DRAG IS THE ONE GESTURE ON THIS PANEL THAT OUTLIVES THE RENDER THAT STARTED IT.** Every other
# control answers a click by rebuilding the sheet; a drag may not, because the rebuild frees the chart
# the pointer is holding. So the three things a drag changes — which floor is composed, which curve
# answers for it, and whether that answer has landed — cannot live in the builder's locals the way the
# rest of the composition does. They live here, for exactly as long as the gesture does.

## **THE CURVE THE HUNT SHEET'S LIVE READINGS COMPOSE AGAINST** (`ForecastQuery.KIND_HUNT_CREW_TAKE`).
## It is the committed floor's rows from the moment the sheet is built, and the DRAGGED floor's rows
## from the moment one is answered — which is the whole of the fix: the take line follows the floor
## being dragged rather than the floor the sheet opened at. Empty while an ask is in flight, which is
## what makes `_hunt_yield_model` state no take at all rather than a number for a floor already left.
var _hunt_live_crew_take: Array = []

## …and the seam's own verdict on that same key, so the sheet can say WHICH of waiting and refused it
## is in. Read by the live take-pending host; never derived from `_hunt_live_crew_take` being empty,
## since a failure and a round trip still in flight are different sentences.
var _hunt_live_crew_view: Dictionary = {}

## **THE DEPOSIT SHEET'S CREW CURVE VIEW** (`ForecastQuery.KIND_DEPOSIT_CREW_TAKE`, issue #663) — the
## seam's answer at the floor the sheet is composing at, whether that is the committed floor (set by
## the build) or a floor a live drag is on (set by `_drag_deposit_crew_take`). Every gear-bearing
## figure on the sheet is read out of it, so a drag's refill and a rebuild read one member.
var _deposit_live_crew_view: Dictionary = {}

## **THE PATCH SHEET'S CREW CURVE VIEW** (`ForecastQuery.KIND_FORAGE_CREW_TAKE`) — asked on a KEPT patch
## only, at the floor the sheet composes at (the build) or a floor a live drag is on. A kept patch's
## take, its cap and its keeping hands are read out of it; an UNKEPT patch leaves it empty and keeps the
## closed form, there being no keeping to take out of the crew.
var _forage_live_crew_view: Dictionary = {}

## **WHEN THE DRAG LAST PUT THE CURVE QUESTION ON THE SOCKET**, and the key it put — the rate limit's
## two terms (`HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC`). The key is held beside the
## clock so that a motion landing back on a floor already asked costs nothing AND does not restart the
## interval: the seam would drop that ask as a duplicate, so charging the budget for it would delay
## the next floor that is genuinely new.
var _crew_take_drag_asked_at_msec: int = 0
var _crew_take_drag_asked_key: String = ""

## **THE REFILL A LIVE FLOOR CHANGE RUNS, AND THE FLAG THAT SAYS ONE IS RUNNING.** The builder owns
## the live-host registry and the closures that fill it, so the refill is published here as a callable
## rather than reachable as a method; `refresh_compose_sheet` calls it INSTEAD of rebuilding while a
## drag is live, which is what lets a forecast answer land on a sheet the player is still holding.
## Empty between drags, and re-set by every rebuild — the nodes the closure names are that rebuild's.
var _floor_drag_refill: Callable = Callable()
var _floor_drag_live: bool = false

func set_forecast_query(query: ForecastQuery) -> void:
    _forecast_query = query

func _init(compose: ComposeState, band_labor: HudBandLaborState, selection: HudSelectionState,
        topbar: FactionReadouts, selectioncard: SelectionCardController, host: Node,
        herd_assign_controls: VBoxContainer, tile_panel: PanelCard,
        resolve_assign_band: Callable, herd_label_for_id: Callable, emit_assign_labor: Callable) -> void:
    _compose = compose
    _band_labor = band_labor
    _selection = selection
    _topbar = topbar
    _selectioncard = selectioncard
    _host = host
    _herd_assign_controls = herd_assign_controls
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
## away with no idle crew and the patch outside its work range. Every live reading on the sheet moved
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
        kit_id: String = KitRoster.NO_KIT_ID,
        take_species: PackedStringArray = PackedStringArray(),
        move_with_herd: bool = false) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, floor, species, improvement,
        kit_id, take_species, move_with_herd)

## **RETIRED — `_emit_improvement`, THE DECLARE/WITHDRAW EMITTER**
## (`docs/plan_standing_upkeep.md` §4.7a ①). It turned the improvement checkbox's tick into the SET
## verb (`cultivate` / `sow` / `tame` / `corral`) and its untick into `unqueue <faction> <source>`,
## and it went with the checkbox: the box was not the commit — the action button reading **`Forage`**
## was — so ticking it and closing the sheet did nothing, reported from play repeatedly.
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
##
## **IT TAKES NO IMPROVEMENT** (`docs/plan_standing_upkeep.md` §2.2). While a Tame or a Corral ran,
## the sim used to pay a gentling crew `workers × per_worker × build_dip`, and a take priced without
## the verb quoted ~4× what the herd handed over. The build has its own crew now, so the hunters' take
## is the plain one whether or not a rung is going up.
## `holding` asks the same question of the steady state — the ceiling becomes one turn's regrowth at
## this floor instead of the room above it. Same swap, same reason, as `_hunt_delivered`'s.
func _hunt_take_rate(herd: Dictionary, floor: float, workers: int,
        holding: bool = false, camp: bool = false) -> Dictionary:
    var rates := SourceForecast.herd_axis_rates(herd, floor)
    var per_worker_rate := float(rates["per_worker"])
    # **THE `now` READING IS NEXT TURN'S ROOM, NOT THE STANDING ONE** — the sim regrows before it
    # harvests (`advance_herds` / `advance_forage_regrowth` run a whole stage ahead of the labor
    # pass), so a herd held at its floor takes its regrowth rather than nothing, and the standing room
    # is what made this readout say `0.00` beside a work board quoting a live rate.
    var ceiling := float(rates["hold_ceiling" if holding else "next_ceiling"])
    if workers <= 0 or per_worker_rate <= 0.0 or ceiling < 0.0:
        return {"available": false}
    # A camp row has no carry cap (see `_hunt_delivered`): the herd's room is the only bound the
    # smoothed rate has, so the crew's throughput does not enter it.
    var crew_bound := ceiling if camp else minf(float(workers) * per_worker_rate, ceiling)
    return {
        "available": true,
        "rate": maxf(crew_bound, 0.0),
    }

## **IS THIS HUNT A POSTED ONE?** — the herd lies past the band's apron (`_is_past_apron`, the one
## threshold a caravan has), so a party is posted and the pack's kill-stop applies. Everything inside
## the work range is a CAMP kill. Distance, not the reply's `walk_tiles` (a posted row on a road can
## have a zero walk); an unknown distance reads as camp, like `_is_past_apron`'s own `-1`.
func _hunt_is_posted(band: Dictionary, herd: Dictionary) -> bool:
    return _is_past_apron(band, int(herd.get("x", -1)), int(herd.get("y", -1)))


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
## lands off `herd` under `policy` per turn. A
## hunt takes WHOLE animals via a kill-credit bank, so the take is ONE quantised expression — the sim's
## `killed = min(the stock above the floor, animals the pack seats, animals brought down)`,
## in bodies per turn — and delivery is `killed × body`: every animal killed lands WHOLE (the sim's
## `AnimalTake::killed_biomass()`), so carry sizes how many animals die, never what each one delivers. Returns
## `{available, delivered_biomass, body_mass}` or
## `{available=false}` when a term is absent (caller degrades to the smoothed per-turn line). NEVER
## re-derives the ecology model — `body_mass`, the curve and the room are sim exports.
##
## > #### ⛔ IT IS STATED IN **BIOMASS**, AND FOOD IS ONE VALUATION OF ITS ANSWER
## >
## > Every one of the three arms is a biomass: the room above the floor, the crew's carry, and the body
## > the count is quantised to. It used to be written in FOOD — the same three arms multiplied through
## > `provisionsPerBiomass` — which is arithmetically identical for a species that pays food and
## > **undefined for one that does not**: an inedible quarry's per-animal food quantum is a structural
## > `0`, so the guard below answered `available: false` for a wolf, both food paths bailed, and its
## > material rows fell through to a parallel crew-throughput line that carried neither the engagement
## > bound nor the whole-animal quantum. A wolf is exactly the quarry whose materials are the entire
## > point of hunting it, so it was over-quoted at every crew the reach arm pinned.
## >
## > **The caller crosses this one carried biomass into every account**
## > (`SourceForecast.rescaled_from_biomass`), which is the sim's own order — one `take.carried`, valued
## > twice.
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
func _hunt_delivered(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        improvement: String, per_crew: Array, holding: bool = false) -> Dictionary:
    # **WHAT THE CREW BRINGS DOWN IS THE SIM'S ANSWER, LOOKED UP** — one row per crew size off
    # `ForecastQuery.KIND_HUNT_CREW_TAKE`, with the engagement, the retreat and the FIGHT already
    # resolved. It is the WHOLE crew's animals per turn: **never multiplied by `workers`.**
    #
    # **NO ROW MEANS NO NUMBER, and that is why this refusal is spelled apart from the one below.** A
    # reply that has not landed is not a herd the wire describes too thinly — the degrade path under
    # it composes a SMOOTH crew-throughput rate with no fight and no engagement at all, which is a
    # bigger overstatement than the one this channel exists to remove. `_hunt_yield_model` states
    # nothing rather than reaching for it.
    var crew_row := SourceForecast.hunt_crew_take_row(per_crew, workers)
    if crew_row.is_empty():
        return {"available": false, CREW_TAKE_UNANSWERED: true}
    # IN BIOMASS, through `herd_axis_rates` — the single place the quantised take's terms are resolved,
    # reading the HERD's own body, carry and room rather than any cohort-level echo of them (the
    # species-blind `hunt_per_worker_provisions` is what would re-introduce phantom food here).
    #
    # **THE DIP MULTIPLIES THE COLLECTION, AND THE QUANTISATION HAPPENS AFTER IT** — the sim's own
    # order (`hunt_take` composes `workers × per_worker`, THEN
    # `fauna::quantise_animal_take`). It arrives here on `per_worker`, so `collection` below carries it
    # and the quantisation runs against the dipped throughput. That is not a scaling of the answer:
    # below one body of carry the crew still kills one animal (the `max(1.0)` on the haul arm), so
    # a build moves the SHAPE of the take, not merely its size. Dipping the ceiling, or
    # the delivered figure after quantisation, produces a number that is wrong in a way that still
    # looks plausible.
    var rates := SourceForecast.herd_axis_rates(herd, floor)
    var body := float(rates["body_mass"])
    var carry := float(rates["carry"])
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    # **THE `now` READING IS NEXT TURN'S ROOM, NOT THE STANDING ONE** — the sim regrows before it
    # harvests (`advance_herds` / `advance_forage_regrowth` run a whole stage ahead of the labor
    # pass), so a herd held at its floor takes its regrowth rather than nothing, and the standing room
    # is what made this readout say `0.00` beside a work board quoting a live rate.
    var room := float(rates["hold_room" if holding else "next_room"])
    if body <= 0.0 or carry <= 0.0 or room < 0.0 or workers <= 0:
        return {"available": false}
    room *= output
    var collection := float(workers) * carry * output   # crew's raw BIOMASS throughput /turn
    # THE KILL IS ONE EXPRESSION — the sim's own `fauna::quantise_animal_take`,
    # `killed = affordable.min(carryable.max(1.0)).min(brought_down)`, in bodies. The three
    # bounds are the stock above the floor, what the crew can HAUL, and what it brings down
    # (`docs/plan_hunt_through_combat.md` §2). The engagement arm was the one this sheet had never
    # had: one hunter's 40 biomass of carry read **307 Wild Fowl a turn** against a take of ten — the
    # sheet promising 30× what the sim pays, for the whole life of the wire field's absence.
    #
    # **THE CARRY ARM ROUNDS UP** — `fauna::animals_the_pack_seats`, `ceil(collection / body × (1 − ε))`
    # floored at one: a pack that seats 2.4 bodies stops the party at THREE kills, because the third
    # animal is killed whole and the pack only decides when the killing stops.
    # **THE `max(1.0)` BELONGS TO THE CARRY ARM ALONE, INSIDE THE `min`, AND THAT IS THE WHOLE OF WHY
    # THIS IS ONE EXPRESSION RATHER THAN TWO BRANCHES.** A party that cannot carry one whole animal
    # still kills one, landed whole — a fact about the PACK. A party that brings down three
    # quarters of an animal has brought down three quarters of an animal, and floors at nothing. While
    # the only bound below one body was the carry quotient the two were indistinguishable, so a
    # `carryable < 1` branch could price delivery as the crew's whole raw `collection`; with the
    # engagement arm in the same `min` that stopped being true, and a Wild Boar crew of six
    # (engaged 1, stayed 0.75) read **4.80 food/turn** — its entire carry throughput, twenty boar —
    # for a take of 0.18, then FELL to 0.36 at seven hunters, the readout dropping as the crew grew.
    #
    # **THE BOUND IS APPLIED TO THE ANIMAL COUNT, NOT TO `collection`**, and the two are the same
    # arithmetic taken in different orders: `floor(min(carry, engaged × body) / body)` is
    # `min(floor(carry / body), engaged)` — but the first divides a product of `body` BY `body` and can
    # land a whole engagement one animal short on a rounding, while the second is exact.
    #
    # `animals_engaged` answers UNBOUNDED for a pen and for a species with no engagement stage, and
    # `animals_stayed` passes an unbounded reach straight through — so the `min` is a no-op and every
    # managed-herd and plant-web frame reads exactly what the carry quotient alone gave it.
    #
    # **AND THE ARM IS THE SIM'S WHOLE THREE-STAGE ANSWER, NOT THE TWO STAGES THE CLIENT CAN SEE.**
    # It was `animals_stayed(animals_engaged(w, rate), stay)` — engagement and retreat, no fight — and
    # on a Wild Aurochs with four hunters that read **1.92 food** against the herd's **0.84**, with
    # bone, fibre and hide over by the same 2.3× because all four are fixed conversions of one
    # biomass. The client cannot close that gap: `combat_config.hit_chance` is unpublished and the
    # damage-over-durability division is one of the halves the schema keeps as the sim's answer.
    #
    # **CARRY SIZES THE KILL COUNT, NOT THE DELIVERY** — a body lands WHOLE (in the larder, or ferried
    # home by porters), so the crew's `collection` only bounds how many animals are brought down
    # (`haulable`); each one delivers its full body. Below one body per turn `killed` is a CADENCE (a
    # body every `1/killed` turns).
    #
    # **A CAMP KILL HAS NO CARRY ARM AT ALL** — a herd inside the band's work range lands whole in the
    # larder with no party posted, so only the fight (`brought_down`) and the room bound its kill
    # count and the sled changes nothing. Only a POSTED kill (past the apron) keeps the pack's
    # kill-stop. `workers_needed` is deliberately a different question: it plans the carriers.
    var haulable := INF
    if _hunt_is_posted(band, herd):
        haulable = maxf(ceilf(collection / body * (1.0 - ANIMAL_COUNT_EPSILON)), ONE_WHOLE_ANIMAL)
    var brought_down := float(crew_row[SourceForecast.CREW_TAKE_LIKELY_KEY])
    # BODIES PER TURN — the herd's own offer, the crew's haul, and what the party puts on the ground,
    # whichever is least. Fractional below one: a body every `1/killed` turns.
    #
    # **THE FIRST TWO ARMS ARE BOUND ONCE**, because the BAND below is the same take asked at two more
    # quantiles: only the sim's arm differs between the three readings, so a second spelling of the
    # room-and-haul pair is all it would take for the band to stop bracketing the figure it brackets.
    var room_and_haul := minf(room / body, haulable)
    var killed := minf(room_and_haul, brought_down)
    var delivered := killed * body
    return {"available": true, "delivered_biomass": delivered, "body_mass": body,
        # **THE TAKE AND ITS BAND, IN ANIMALS** — the readout's own line. The two client-side arms are
        # the caller's and are not stochastic, so they apply to every quantile unchanged; the spread
        # that survives them is the sim's.
        CREW_TAKE_ANIMALS: killed,
        CREW_TAKE_ANIMALS_LOW: minf(room_and_haul,
            float(crew_row[SourceForecast.CREW_TAKE_LOW_KEY])),
        CREW_TAKE_ANIMALS_HIGH: minf(room_and_haul,
            float(crew_row[SourceForecast.CREW_TAKE_HIGH_KEY])),
        # …and the sim's arm UNCLAMPED, which is what the binding-limit line compares the other two
        # against. Clamped, it could never be the smallest and the line could never name the crew.
        CREW_TAKE_BROUGHT_DOWN: brought_down}

## **HOW SURE THE TAKE IS AND HOW OFTEN IT LANDS, AS THE TAIL OF THE SENTENCE THAT QUOTES IT** —
## ` (0.21 – 0.48)` only where the band is genuinely a band, then `, about one every 2.9 turns` only
## where a body takes more than a turn to drop. `""` on a certain take of a whole animal or more,
## which leaves `HudComposeVocab.HUNT_LIMIT_CREW_FORMAT` a bare figure and a full stop.
##
## **IT IS A TAIL RATHER THAN A LINE OF ITS OWN, and that is what this arc changed.** The band and the
## cadence used to ride a take estimate ABOVE the yields, which restated a rate the binding-limit
## sentence below the yields already quoted — the same number twice, with the four accounts in
## between. One sentence carries all three readings now: figure → spread → wait.
##
## **THE CADENCE IS NOT CHROME.** On this web a fractional animal is the ORDINARY reading, not an edge
## case: the whole-animal quantum is a timing effect the sim's wound ledger carries between turns, so
## a party that cannot finish a body this turn still finishes one eventually. `≈0.75 Wild Aurochs/turn`
## is exact and a player still reads it as "not quite one, so nothing happens" — the same conclusion
## the reported `≈0` produced. Stating the wait in turns is what makes the fraction mean something.
##
## **A DEGENERATE BAND PRINTS NO RANGE**, which is every reading at the shipped tuning: both stochastic
## stages (`combat::attacks_landed_at`, `fauna::animals_that_stay`) are binomials that answer their
## degenerate identity at any quantile when `hit_chance` is 1 and the quarry's wariness is 0, so the
## three quantiles arrive bit-identical. Chrome that renders `0.35 – 0.35` would manufacture doubt the
## model does not have — and doing it by printing equal numbers rather than by suppressing the clause
## is the same lie with extra ink.
func _hunt_take_spread(dw: Dictionary) -> String:
    var likely := float(dw[CREW_TAKE_ANIMALS])
    var low := float(dw[CREW_TAKE_ANIMALS_LOW])
    var high := float(dw[CREW_TAKE_ANIMALS_HIGH])
    var tail := ""
    if not SourceForecast.hunt_take_band_is_degenerate(low, likely, high):
        tail = HudComposeVocab.HUNT_TAKE_BAND_FORMAT % [
            DetailFormat.animal_rate_face(low), DetailFormat.animal_rate_face(high)]
    # **AND THE CADENCE COMES LAST, after the band**, so the sentence reads figure → spread → wait:
    # the number, how sure it is, and what it feels like. It rides the LIKELY take, which is the
    # figure the sentence itself quotes. See `HudComposeVocab.HUNT_TAKE_CADENCE_FORMAT`.
    return tail + _hunt_take_cadence(likely)

## The crew NOUN this sheet calls these hands, lower-cased for a sentence — the same fork the stepper's
## row label makes (`SourceForecast.is_managed_hunt_source`), so the remedy line and the control it
## points at cannot call one crew two things.
func _hunt_crew_noun(herd: Dictionary, improvement: String) -> String:
    return (HudComposeVocab.HERD_CREW_LABEL \
        if SourceForecast.is_managed_hunt_source(herd, improvement) \
        else HudComposeVocab.HUNT_CREW_LABEL).to_lower()

## **WHICH OF THE THREE LIMITS ON THIS TAKE IS THE SMALLEST, AND WHAT TO DO ABOUT IT** — the herd's
## own regrowth, the room standing above the floor, and what the crew brings down. `{severity, text}`
## in the shared verdict's shape; `{}` when the herd publishes too little to compare them, which
## leaves the shared verdict standing.
##
## **THE THREE ARE COMPARED IN ONE UNIT — ANIMALS PER TURN — and every one of them is a term this
## panel already reads.** The regrowth is the growth curve sampled AT the floor
## (`HerdTelemetryState.regrowthSamples`); the room is the same forward room the take's first arm is
## quantised against; the crew's arm is the sim's own reply, UNCLAMPED, since the clamped figure is by
## construction never larger than the other two and could never be named.
##
## **THE BELOW-FLOOR STATE TAKES THE SLOT RATHER THAN ADDING A LINE.** A herd under
## `floor × carryingCapacity` has no room above the floor at all, so the room arm would win and say
## *"only ≈0 stand above your floor"* — true, and the wrong sentence: the player set that floor, and
## what they need to know is that the take is now the surplus alone.
func _hunt_binding_limit(herd: Dictionary, band: Dictionary, floor: float, dw: Dictionary,
        crew_noun: String) -> Dictionary:
    var prefix := HudComposeVocab.BARE_FORECAST_PREFIX
    var body := float(dw["body_mass"])
    var capacity := float(herd.get(prefix + SourceForecast.FORECAST_CAPACITY_KEY, 0.0))
    var biomass := float(herd.get(prefix + SourceForecast.FORECAST_BIOMASS_KEY, 0.0))
    if body <= 0.0 or capacity <= 0.0:
        return {}
    var quarry := SourceForecast.herd_display_name(herd)
    if biomass < SourceForecast.clamp_floor(floor) * capacity:
        return {"severity": SourceForecast.VERDICT_OK,
            "text": HudComposeVocab.HUNT_LIMIT_BELOW_FLOOR}
    # The band's productivity rides the herd-side arms exactly as it rides the take itself
    # (`_hunt_delivered` scales the room by it), or the comparison would weigh a full-rate
    # regrowth against a discounted crew.
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    var rates := SourceForecast.herd_axis_rates(herd, floor)
    var room_animals := maxf(float(rates["next_room"]), 0.0) * output / body
    var crew_animals := float(dw[CREW_TAKE_BROUGHT_DOWN])
    # **THE REGROWTH ARM IS DROPPED WHERE THERE IS NO CURVE TO SAMPLE**, rather than read as zero: a
    # herd the wire published no `regrowthSamples` for would otherwise bind at nothing and this line
    # would tell every such player their herd breeds back none.
    var samples := SourceForecast.regrowth_samples(herd, prefix)
    var sustainable := INF
    if SourceForecast.has_growth_curve(samples):
        sustainable = maxf(SourceForecast.regrowth_at(samples,
            SourceForecast.clamp_floor(floor)), 0.0) * output / body
    if crew_animals <= minf(room_animals, sustainable):
        # **THE COMPARISON ABOVE IS THE UNCLAMPED ARM; THE FIGURE QUOTED IS THE CLAMPED TAKE.** They
        # are one number in this branch save for the haul — the crew binding is exactly the room not
        # binding — and the take is what the band brackets, what the cadence divides and what the
        # accounts below are conversions of. Quoting the unclamped kill instead would put a figure in
        # the sentence that its own parenthetical band could contradict, on a party that kills more
        # than it can carry home — the state the waste note beside it already explains.
        var take := float(dw[CREW_TAKE_ANIMALS])
        return {"severity": SourceForecast.VERDICT_SLOW,
            "text": HudComposeVocab.HUNT_LIMIT_CREW_FORMAT % [
                crew_noun, DetailFormat.animal_rate_face(take), quarry, _hunt_take_spread(dw)]}
    if sustainable <= room_animals:
        return {"severity": SourceForecast.VERDICT_OK,
            "text": HudComposeVocab.HUNT_LIMIT_SUSTAINABLE_FORMAT % [
                DetailFormat.animal_rate_face(sustainable), quarry]}
    return {"severity": SourceForecast.VERDICT_OK,
        "text": HudComposeVocab.HUNT_LIMIT_ROOM_FORMAT % [
            DetailFormat.animal_rate_face(room_animals), quarry]}

## **HOW OFTEN A BODY ACTUALLY DROPS**, for a take under one animal a turn — `1 ÷ rate`, the cadence
## the wound ledger integrates the fractional take into. `""` at or above one a turn and for a rate of
## none, which is what keeps the clause off every ordinary line (see
## `HudComposeVocab.HUNT_TAKE_CADENCE_FORMAT`).
func _hunt_take_cadence(rate: float) -> String:
    if rate <= 0.0 or rate >= HudComposeVocab.HUNT_TAKE_CADENCE_THRESHOLD:
        return ""
    return HudComposeVocab.HUNT_TAKE_CADENCE_FORMAT % DetailFormat.format_trimmed(1.0 / rate,
        HudComposeVocab.HUNT_CADENCE_DECIMALS)


## Each FLOOR PRESET's button metric on a LOCAL hunt, keyed preset -> a `{compact, full}` pair (compact
## for the button face's second line, full for the tooltip). The plant twin is `_forage_floor_takes`;
## both wear the same shape, only the source of the ceiling differs.
##
## The metric is the herd's worker-independent CEILING at that preset's floor — `max(0, B - f*K) x the
## species' per-biomass vector`, composed by `SourceForecast.forecast_inputs`. Composed, not looked up:
## the wire carries no per-stance ceiling rows, and four rows could not answer a continuous dial
## anyway.
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
## `per_crew` is the crew-take reply's rows — required, not optional: the hunt yield model states
## nothing at all without them, so a caller that omitted them would render a blank line rather than a
## preview and nothing would say why.
func _local_hunt_preview_bbcode(band: Dictionary, herd: Dictionary, floor: float, workers: int,
        per_crew: Array, improvement: String = SourceForecast.IMPROVEMENT_NONE) -> String:
    return _yield_preview_bbcode(_hunt_yield_model(band, herd, floor, workers, improvement, false,
        per_crew), HudComposeVocab.LOCAL_HUNT_OVERDRAW_SUFFIX)

## The hunt web's yield model — the animal twin of `_forage_yield_model`, in the same shape.
##
## **ITS ROWS ARE ACCOUNTS, LIKE EVERY OTHER PER-TURN READING — one per account the take PAYS.** The
## readout answers what a turn of this hunt puts in the band's stores, so it is stated in every
## account the take credits through `SourceForecast.rescaled_accounts` → `yield_rows` and the account
## table's units, exactly as the plant web's is. **The WHOLE-ANIMAL reading belongs to the CHART above
## it** (the escapement curve and its handle, which count bodies); a
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
    # **THE COMPOSED PARTY IS WHAT THE GEAR HAS TO COVER.** A carry tier is per EQUIPPED worker, so
    # pricing without the crew hands the whole party a tier one unit buys — the defect this seam now
    # carries the stepper's own value to close.
    return _kit_priced_source(herd, HudComposeVocab.BARE_FORECAST_PREFIX, band, KitRoster.JOB_HUNT,
        _compose.hunt_kit_id(), _hunt_priced_crew)

## The plant twin. A patch publishes no retreat, so only the carry half of the substitution bites.
func _forage_priced_patch(tile_info: Dictionary, band: Dictionary) -> Dictionary:
    return _kit_priced_source(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX, band,
        KitRoster.JOB_FORAGE, _compose.forage_kit_id(), _forage_priced_crew)

## The hunt forecast, priced — and the ONLY way this sheet builds one. Pairing the repricing with the
## construction is what makes "some call sites were missed" unrepresentable rather than a thing to
## remember.
## **`per_crew` IS THE SIM'S TAKE CURVE, AND IT IS ONLY EVER PASSED AT THE COMMITTED FLOOR.** The
## reply is keyed on the floor it was asked at (`_crew_take_view`), so the per-PRESET forecasts
## `_hunt_floor_takes` builds deliberately pass none — quoting one floor's curve against another
## floor's room is the kind of borrowed answer this whole channel exists to stop. Their readings are
## worker-independent ceilings, which the curve does not enter anyway.
func _hunt_forecast(herd: Dictionary, band: Dictionary, floor: float,
        per_crew: Array = []) -> Dictionary:
    return SourceForecast.forecast_inputs(_hunt_priced_herd(herd, band),
        SourceForecast.SOURCE_KIND_HERD, HudComposeVocab.BARE_FORECAST_PREFIX, floor, per_crew)

## …and the plant one.
func _forage_forecast(tile_info: Dictionary, band: Dictionary, floor: float) -> Dictionary:
    return SourceForecast.forecast_inputs(_forage_priced_patch(tile_info, band),
        SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX, floor)

## The two SNAPSHOT reads `KitRoster.priced_source` must not make for itself (it is stateless), and
## nothing else. The resolve, the axis and the arithmetic all live there, so the dock's raid sheet
## prices its chart through the identical code rather than a second copy of this.
func _kit_priced_source(src: Dictionary, prefix: String, band: Dictionary, job: String,
        kit_id: String, crew: int = KitRoster.KIT_CREW_UNCOMPOSED) -> Dictionary:
    return KitRoster.priced_source(src, prefix, _band_labor.kits(), job,
        _band_labor.default_kit_id(job), kit_id, band, crew)

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
## turns").
##
## **IT IS ASKED AT THE MODEL, and one seam serves both webs.** The caption's `has_after` is derived
## from the rows the model emits, so gating here is what makes it impossible for the arrow and the
## key over it to disagree — which is the failure the caption resolver has already paid for once.
func _walks_to_the_floor(reaches: bool, improvement: String) -> bool:
    return reaches and improvement == SourceForecast.IMPROVEMENT_NONE

## `per_crew` is the sim's answer to *"what does a crew of N bring down off this herd per turn"* —
## `ForecastQuery.KIND_HUNT_CREW_TAKE`'s reply, one row per crew size, threaded in as a PARAMETER
## rather than looked up here because this producer is PURE and is called three times per refresh
## (`_mount_readout`'s emptiness probe, its yields host and its aside) plus once more for the holding
## rate. A lookup at each of those would be four asks for one answer, and the seam's idempotence would
## hide the fact that the sheet was composing four questions.
func _hunt_yield_model(band: Dictionary, herd_raw: Dictionary, floor: float, workers: int,
        improvement: String, reaches: bool = false, per_crew: Array = []) -> Dictionary:
    # **PRICED AT THE KIT THE CREW WILL BE SENT WITH, ONCE, BEFORE A SINGLE TERM IS READ** — so the
    # sustainability bar, the take, the waste and the degrade path are all one kit's story. Every read
    # below is off `herd`; the raw dict is not in scope again, which is the point of shadowing it here
    # rather than pricing at each `herd_axis_rates` call (this model makes three).
    var herd := _hunt_priced_herd(herd_raw, band)
    # **WHAT THIS CREW BRINGS HOME IN MATERIALS COMES OFF THE DELIVERED BIOMASS, NOT OFF A SECOND
    # CREW-THROUGHPUT LINE**, and it is composed at each of the three branches below rather than once
    # here. The sim banks food at `hunt_yield.apply(take.carried, …)` and materials at
    # `credit_material_yield(…, take.carried, …)` — the identical local — so the two accounts of one
    # readout must read one quantity or they can provably disagree with the payout. They did: a
    # `min(workers × per_worker_material, ceiling)` line skips the engagement→retreat arm and the
    # whole-animal quantiser the food row applies, so a pastoral Wild Boar herd quoted five herders
    # five times the bone and hide that one herder brings home while its FOOD row (correctly) sat
    # still, both crews reaching the same single animal.
    #
    # **`SourceForecast.rescaled_from_biomass` IS THAT ONE CROSSING** — food, fodder and materials out
    # of one carried biomass — so the material rows are a consequence of the delivered figure rather
    # than a parallel derivation of it, on EVERY quarry. The inedible one used to be the branch that
    # could not use it (its provisions rate is a structural zero, so a food-keyed crossing divides by
    # nothing and both food paths bailed); the quantiser is stated in biomass now, which is the unit a
    # take is taken in whether or not the species converts any of it to food.
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    # **THE ⚠ IS THE SIM'S ANSWER, READ OFF THE STANDING ROW** — intent AND ability, and the client
    # neither re-derives it nor gates it (`snapshot.fbs` → `LaborAssignment.overdraws`). See
    # `_source_overdraws`.
    var overdraws := _source_overdraws(SourceForecast.LABOR_KIND_HUNT, -1, -1,
        String(herd_raw.get("id", "")))
    var dw := _hunt_delivered(band, herd, floor, workers, improvement, per_crew)
    # **THE ANSWER HAS NOT LANDED — SO THIS SHEET STATES NO TAKE AT ALL.** Falling through to the
    # degrade branch below would answer with `_hunt_take_rate`, a smoothed `min(crew × per_worker,
    # ceiling)` carrying neither the engagement, the retreat nor the fight — a bigger overstatement
    # than the 2.3× this channel exists to remove, and one wearing an ordinary readout's face. The
    # sheet says it is waiting instead (`HudComposeVocab.HUNT_TAKE_PENDING`, mounted by the builder).
    if bool(dw.get(CREW_TAKE_UNANSWERED, false)):
        return {}
    if not bool(dw.get("available", false)):
        # Graceful degrade — the per-animal quantum (or a lever) is unknown, so fall back to the
        # smoothed per-turn line rather than regress the readout. **It credits the SAME account set
        # the quantised path does**, through the same rescale: the two paths differ in whether the
        # take is quantised, never in what a take pays, and a model whose two branches stated
        # different accounts for one herd is the defect one branch above records.
        var take := _hunt_take_rate(herd, floor, workers, false, not _hunt_is_posted(band, herd))
        if not bool(take.get("available", false)):
            # **NO TAKE THIS CLIENT CAN QUANTISE AND NO FOOD RATE EITHER — but a material one may still
            # stand.** A `{}` model renders no readout at all, which is the `+0.00` this arm exists to
            # close.
            #
            # **AN INEDIBLE QUARRY NO LONGER REACHES IT.** It did, every time, and this crew-throughput
            # line was then its only answer — carrying neither the engagement bound nor the whole-animal
            # quantum, so a wolf was over-quoted at any crew the reach arm pinned. The quantiser is
            # stated in BIOMASS now, so a pack that publishes a body and a carry is priced by the same
            # `min` a deer is. What is left here is a herd the wire describes too thinly to quantise at
            # all, where a crew-throughput rate is better than silence.
            var materials := _hunt_material_rows(herd, band, floor, workers)
            if SourceForecast.signed_material_components(materials) == "":
                return {}
            return {
                YIELD_MODEL_ROWS: SourceForecast.yield_rows(
                    0.0, 0.0, SourceForecast.YIELD_ACCOUNT_NONE, {}, materials),
                YIELD_MODEL_TEXT: HudComposeVocab.LOCAL_HUNT_YIELD_FORMAT % (
                    SourceForecast.yield_components(
                        0.0, 0.0, SourceForecast.YIELD_ACCOUNT_NONE, materials)),
                # The ⚠ is the wire's, on this branch as on every other: the sim answers it for the
                # crew that is standing here, whatever account the take is measured in, and a client
                # that suppressed it because IT could not judge the drawdown would be gating a field
                # the contract says not to gate.
                YIELD_MODEL_OVERDRAW: overdraws,
            }
        var actual := float(take["rate"]) * output
        # **ONE CROSSING, EVERY ACCOUNT** — food, fodder AND materials out of this one smoothed take,
        # so the degrade path cannot state a material rate its own food rate does not imply.
        var smooth := SourceForecast.rescaled_accounts(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            actual)
        var materials: Array = smooth[SourceForecast.RESCALED_MATERIALS_KEY]
        var account := SourceForecast.YIELD_ACCOUNT_FOOD
        var smooth_after := {}
        if _walks_to_the_floor(reaches, improvement):
            var smooth_hold := _hunt_take_rate(herd, floor, workers, true,
                not _hunt_is_posted(band, herd))
            if bool(smooth_hold.get("available", false)):
                smooth_after = SourceForecast.rescaled_accounts(herd,
                    HudComposeVocab.BARE_FORECAST_PREFIX, float(smooth_hold["rate"]) * output)
                # Scalars only — see the quantised branch's `after` for why a material states none.
                smooth_after.erase(SourceForecast.RESCALED_MATERIALS_KEY)
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
            YIELD_MODEL_OVERDRAW: overdraws,
        }
    # The crew's honest carry-aware delivered take. `delivered` is already carry-quantized, so this
    # credits no throughput the crew can't haul home — and it is a take in an ACCOUNT, which is what
    # the readout row states. The animal RATE derived beside it is the SENTENCE's (`YIELD_MODEL_TEXT`,
    # the one-line preview), where the whole-animal rhythm is the whole point of the line.
    #
    # **IT IS A BIOMASS, and the animal rate divides it by a BODY.** The quantiser counts bodies and
    # hauls biomass; converting either into food first is a step only an edible species can take, and
    # it is the step that left a wolf with no take at all.
    var body := float(dw["body_mass"])
    var delivered := float(dw["delivered_biomass"])
    var animal_rate := delivered / body if body > 0.0 else 0.0
    var rate_text := DetailFormat.animal_rate_face(animal_rate)
    var quarry := SourceForecast.herd_display_name(herd)
    # Overdraw = the delivered take exceeds the herd's food-peak ceiling.
    # **THE COUNT IS TAKEN IN BODIES, HAULED IN BIOMASS AND VALUED IN EVERY ACCOUNT** — the sim's own
    # order (`forecast_production_and_take`: quantise, then `YieldPair::rescaled_to`, both off one
    # `take.carried`). `yield_rows` is the one place the "render only where the vector pays" rule
    # lives, so an inedible quarry — whose provisions rate is a structural 0 — crosses to a zero food
    # component that renders NO row, and the zero account below keeps that answer for an all-zero take.
    var take := SourceForecast.rescaled_from_biomass(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
        delivered)
    # **AND THE MATERIALS ARE ROWS OF THAT SAME CROSSING** — one already-quantised `delivered`, so the
    # bone and hide a herder brings home step with the animal count exactly as the meat does, and a
    # crew the engagement bound pins cannot quote materials the sheet's own food row denies. Composing
    # them from `min(workers × per_worker_material, ceiling)` beside this is what quoted a Wild Boar's
    # five herders 5× the hide of one.
    var materials: Array = take[SourceForecast.RESCALED_MATERIALS_KEY]
    # THE ZERO ACCOUNT IS THE ACCOUNT THE TAKE WAS MEASURED IN — the same choice the degrade branch
    # above makes, so one model's two paths can never state an empty take in two different accounts.
    var account := SourceForecast.YIELD_ACCOUNT_FOOD
    # THE STEADY STATE RIDES EACH ACCOUNT'S OWN `after`, so `build_yields_row` composes the arrow from
    # the two magnitudes it formats itself and its header keys them. It RESCALES THE SAME WAY the take
    # does — an arrowed row must key both accounts consistently, and a hold rate credited on one axis
    # beside a take credited on two would arrow only half the reading. `yield_rows` drops an `after`
    # equal to its take, which is the same "an arrow to itself is noise" test this used to make here.
    #
    # **THE MATERIAL HALF OF THE CROSSING IS DROPPED FROM IT, and that is the standing rule rather than
    # an oversight** — the sheet's `now → after` is stated for food and fodder alone, so a material
    # arrow would be a reading no surface asks for. Dropping it also keeps `yield_rows`' `after.has()`
    # lookup facing scalars only, where a material id colliding with the key would otherwise be read
    # as a number.
    var after := {}
    if _walks_to_the_floor(reaches, improvement):
        var held := _hunt_delivered(band, herd, floor, workers, improvement, per_crew,
            true)
        if bool(held.get("available", false)):
            after = SourceForecast.rescaled_from_biomass(herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, float(held["delivered_biomass"]))
            after.erase(SourceForecast.RESCALED_MATERIALS_KEY)
    return {
        YIELD_MODEL_ROWS: SourceForecast.yield_rows(
            float(take[SourceForecast.YIELD_ACCOUNT_FOOD]),
            float(take[SourceForecast.YIELD_ACCOUNT_FODDER]),
            account, after, materials),
        YIELD_MODEL_TEXT: HudComposeVocab.HUNT_DELIVERED_FORMAT % [rate_text, quarry],
        # **THESE ROWS ARE QUOTED AT ONE POINT OF A BAND**, so their caption says which point — the
        # band itself rides the sentence below them (`_hunt_take_spread`).
        YIELD_MODEL_AT_LIKELY: true,
        # …and the sentence under them, which names the SMALLEST of the three limits rather than
        # re-walking a projection the fight is missing from — and, where that limit is the crew,
        # states this sheet's ONLY reading of the take in animals, its band and its cadence.
        YIELD_MODEL_LIMIT: _hunt_binding_limit(herd, band, floor, dw,
            _hunt_crew_noun(herd, improvement)),
        YIELD_MODEL_OVERDRAW: overdraws,
    }

## **THE INEDIBLE QUARRY'S MATERIAL TAKE, AND NOTHING ELSE'S** —
## `min(workers × per_worker_material, material_ceiling)` per material, then the band's output
## multiplier (arc #527 follow-up).
##
## > #### ⛔ THIS IS THE CREW-THROUGHPUT LINE, AND IT IS ONLY HONEST WHERE THERE IS NO TAKE TO CROSS FROM
## >
## > It served every hunt until it was measured against the payout: it carries the escapement ceiling
## > and NONE of the food row's other three bounds — the engagement→retreat arm and the whole-animal
## > quantiser — so on a herd whose reach binds it scales with the CREW while the animals reached, and
## > therefore the meat, do not. A pastoral Wild Boar herd (`engage_rate` 0.33, one animal reached at
## > every crew from one to six) quoted five herders 0.40 bone / 2.80 hide against one herder's
## > 0.08 / 0.56, beside a FOOD row correctly flat at 0.18 for both — while the sim banks both accounts
## > off one `take.carried`.
## >
## > **The quantised and degrade paths cross out of that delivered biomass now**
## > (`SourceForecast.rescaled_accounts`). What is left here is the branch where there is no food axis
## > at all: an inedible quarry's provisions rate is a structural zero, so no delivered figure exists
## > to cross from, and this remains the only answer the wire lets the client compose. It is therefore
## > still over-quoted at a crew the reach arm pins — closing it needs the quantiser expressed in
## > BIOMASS, i.e. a `body_mass` such a herd publishes and the harness fixtures do not derive.
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
    # The FORWARD room, like the food row beside it: the two accounts of one readout must describe one
    # turn, and this sheet's headline is what the crew banks next turn.
    return SourceForecast.scaled_material_rows(
        SourceForecast.expected_materials(float(workers), forecast,
            SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY),
        float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL)))

## > #### ⛔ THE ⚠ HAS ONE PRODUCER, AND IT IS THE SIM — this is the whole of the client's part
## >
## > `LaborAssignment.overdraws` carries the WHOLE verdict: the floor is below the food peak **AND**
## > this crew can actually draw the source down to it. The contract says so in as many words — read
## > this field, do not re-derive it, and do not gate it — so the compose sheet's answer is a lookup on
## > the source's own standing row and nothing else.
## >
## > **It used to compute a fourth, different predicate**, and both halves of it were wrong. The
## > comparison was `actual > sustainable`, which the schema forbids outright: a first harvest of a
## > stocked source exceeds one turn's regrowth at EVERY floor, so it cried wolf exactly where the mark
## > has to be trustworthy. Beside it sat a second, client-side reachability walk — a private copy of
## > curves the sim owns. Reported from play as two surfaces disagreeing about one source: the tile
## > card's tooltip read *"Sustainable +0.63/turn — overdrawing"* while the sheet three inches away read
## > *"This crew can't draw it that low. It settles at 92%."*
## >
## > **The row is the FACTION's, not the acting band's** (`_standing_assignment`) — the same scan the
## > drawer's standing summary and the map badge read, so a source worked by another of your bands
## > still answers with the crew that is actually on it rather than with the one the picker happens to
## > name.
## >
## > `false` on a source nobody works is the only reading available and is the right one: there is no
## > crew, so nothing is being drawn down. The sheet is then composing a hypothetical, and a warning
## > about a take that has not been ordered is a warning about nothing.
func _source_overdraws(kind: String, x: int, y: int, herd_id: String) -> bool:
    return bool(_standing_assignment(kind, x, y, herd_id).get("overdraws", false))

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
## **The overdraw verdict is NOT this line's to compose** — it is `LaborAssignment.overdraws`, read off
## the patch's own standing row. See `_source_overdraws` for what the client used to compute here and
## why every account of it was wrong.
func _local_forage_preview_bbcode(band: Dictionary, tile_info: Dictionary, floor: float,
        workers: int, improvement: String = SourceForecast.IMPROVEMENT_NONE) -> String:
    return _yield_preview_bbcode(_forage_yield_model(band, tile_info, floor, workers, improvement),
        HudComposeVocab.LOCAL_FORAGE_OVERDRAW_SUFFIX)

## The relative slop on a whole-animal count, mirroring `ANIMAL_COUNT_EPSILON` in `core_sim/src/fauna.rs`
## (used by `animals_the_pack_seats`, applied DOWNWARD because that rounding is upward: a collection of
## exactly three bodies seats three, not four on a last-mantissa-bit overshoot).
const ANIMAL_COUNT_EPSILON := 1e-6

## The fewest animals a pack ever seats — `ONE_WHOLE_ANIMAL` in `core_sim/src/fauna.rs`: a party that
## cannot carry one still kills one.
const ONE_WHOLE_ANIMAL := 1.0

## The structured half of the line above — and the SHARED shape both webs answer in, so the readout's
## yields row is built from the same numbers the sentence quotes rather than from a second derivation.
## `{}` is the graceful degrade (an unknown forecast, or a source that pays into no account at all);
## `rows` is `SourceForecast.yield_rows`' answer verbatim, so the render-only-where-the-vector-pays
## rule is obeyed by construction and no zero can be reintroduced by a caller.
const YIELD_MODEL_ROWS := "rows"
const YIELD_MODEL_TEXT := "text"
const YIELD_MODEL_OVERDRAW := "overdraw"
## **THE HONEST-SILENCE LINES THIS MODEL OWES THE PLAYER** — an `Array[String]` of asides rendered
## under the rows, `[]` on the ordinary sheet. A narrowed take earns one when the wire priced no
## per-species rate for something the player ticked, and another when the patch pays a MATERIAL (which
## is basket-averaged and has no per-species twin on the wire, so a narrowed crew's fibre is a number
## this client does not have). They ride the MODEL, so whoever evaluates it at a floor and a crew gets
## the rows and the reason they read that way together.
const YIELD_MODEL_NOTES := "notes"
## **ARE THESE ROWS ONE POINT OF A BAND?** — the caption's gate (`at the likely take`), `false` on
## every model whose readings carry no spread at all: the whole plant web, and the hunt web's degrade
## branch, whose smoothed rate is not a quantised take.
##
## **THE ROWS THEMSELVES STAY SINGLE NUMBERS.** Four bands would assert four independent rolls, which
## is false: the food, the bone, the fibre and the hide are fixed conversions of ONE carried biomass,
## so they move together and their spread is stated once — in the binding-limit sentence below them
## (`YIELD_MODEL_LIMIT`), which is where the take in animals is stated too.
##
## **IT USED TO BE THE TAKE ESTIMATE LINE ITSELF**, a `≈0.35 Wild Boar/turn · 0.21 – 0.48` label
## mounted above the rows. It said what the sentence below the rows already said, so it is gone and
## what survives of it is this flag: the caption still has to say which point of the band the four
## accounts are quoted at.
const YIELD_MODEL_AT_LIKELY := "at_likely"
## **WHICH OF THE THREE LIMITS IS BINDING, AND ITS REMEDY** — `{severity, text}` in the verdict's own
## shape, so the readout's third register renders it through `HudWidgets.build_verdict_line` exactly
## as it renders the shared harvest verdict. `{}` means "this model names no limit", which is the
## caller's cue to fall back to that shared verdict.
##
## **IT REPLACES THE SHARED VERDICT ON THE HUNT WEB, IT DOES NOT JOIN IT.** *"This crew can't draw it
## that low — 12 herders would reach the floor"* is composed from the projection walk, which carries
## the engagement and the retreat and NOT the fight; on the web where the fight is half the answer
## that sentence names the wrong remedy at the wrong size. The plant web has no fight and keeps it.
const YIELD_MODEL_LIMIT := "binding_limit"
## **DOES THIS SOURCE RENEW AT ALL?** — the gate on the yields row's `renewable` note, and the reason
## it is a model key rather than a test at the render: only the model knows, and `_fill_yields_host`
## draws for three webs.
##
## ⛔ **`RENEWABLE` ON A QUARRY IS A LIE, and it is the exact lie the deposit branch's whole readout
## fork exists to prevent** (`.claude/rules/client/extraction-workings.md` → the `regrowth_rate > 0`
## table). Rock's rate is zero, so a finite seam renews nothing and states NEITHER the note nor the
## overdraw flag; what it states instead is the runway.
##
## **ABSENT MEANS `true`, and that is a structural fact about the other two webs rather than a
## fallback**: a patch reseeds and a herd breeds, so neither food model has a `false` to state and
## every frame either drew before this key existed is unchanged.
const YIELD_MODEL_RENEWS := "renews"
## Set by `_with_home_rate` when the headline (food, or a working's material) is the caravan's steady
## rate home, so the caption can say
## so (`HudComposeVocab.YIELD_HEADER_ONCE_RUNNING`) instead of `next turn`. Absent everywhere else.
const YIELD_MODEL_HOME_RATE := "home_rate"
## Set by `_with_home_rate` alongside it: the caravan's rot sentence (`HudWorkVocab.rot_line`), or
## `""`. Rendered as an amber verdict-style bullet directly under the PER TURN numbers.
const YIELD_MODEL_ROT := "rot"

# ---- WHAT `_hunt_delivered` ANSWERS BESIDE THE DELIVERED BIOMASS ----------------------
## **THE REPLY HAS NOT LANDED**, told apart from an unavailable take so the caller can state nothing
## instead of degrading to a smoothed rate composed without the fight. See that function's guard.
const CREW_TAKE_UNANSWERED := "crew_take_unanswered"
## The quantised take and its band, in ANIMALS per turn — the sim's row through the caller's own two
## arms. `CREW_TAKE_BROUGHT_DOWN` is the sim's arm alone, unclamped, for the binding-limit comparison.
const CREW_TAKE_ANIMALS := "animals"
const CREW_TAKE_ANIMALS_LOW := "animals_low"
const CREW_TAKE_ANIMALS_HIGH := "animals_high"
const CREW_TAKE_BROUGHT_DOWN := "brought_down"
## **`tile_info` IS THE SOURCE THIS TAKE IS COMPOSED FROM, AND FOR A NARROWED CREW THE CALLER HAS
## ALREADY NARROWED IT** (`SourceForecast.narrowed_source`). That is the whole shape of the selective
## gather on this sheet: the stand, the rates and the crew throughput are substituted ONCE, upstream,
## so the take, the `now → after` walk, the worker cap, the crew targets and the chart all answer for
## the selection through the identical arithmetic — and ticking a chip moves the forecast exactly as
## stepping the crew does, which is the asymmetry this feature exists to remove.
##
## `take` is `{narrowed, quoted, materials}` — see `_selective_take_state`. It reaches this model for
## the two things the narrowed SOURCE cannot say for itself: that an unquotable narrowing must print
## no number at all, and which asides are owed.
func _forage_yield_model(band: Dictionary, tile_info: Dictionary, floor: float,
        workers: int, improvement: String = SourceForecast.IMPROVEMENT_NONE,
        reaches: bool = false, take: Dictionary = {}) -> Dictionary:
    var narrowed := bool(take.get(TAKE_SELECTION_NARROWED, false))
    var notes := _take_notes(take)
    if narrowed and not bool(take.get(TAKE_SELECTION_QUOTED, false)):
        # The wire priced no per-species rate for something the player ticked, so there is no number
        # this client has the right to print — and the whole basket's take under a narrowed heading is
        # the quote-vs-payout defect wearing a heading.
        return _wordless_take_model(notes)
    # **THE ⚠ IS THE SIM'S ANSWER, READ OFF THE STANDING ROW** — see `_source_overdraws`. There is no
    # sustainability BAR composed here any more: the food-peak ceiling this model used to measure the
    # take against was the client's own second opinion, and on a patch standing at or below that peak
    # it degenerated into "something is being taken at a floor below 0.5".
    var overdraws := _source_overdraws(SourceForecast.LABOR_KIND_FORAGE,
        int(tile_info.get("x", -1)), int(tile_info.get("y", -1)), "")
    var forecast := _forage_forecast(tile_info, band, floor)
    if not bool(forecast["known"]):
        return _wordless_take_model(notes)
    var output := float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    # ⛔ **A KEPT PATCH'S TAKE IS THE SIM'S** (`ForecastQuery.KIND_FORAGE_CREW_TAKE`). The keeping comes
    # out of the crew first and only the sim knows how many hands that is at each crew, so the take
    # is the curve's row and the other accounts are priced on the hands the row says gather. While the
    # reply is in flight the sheet states no take at all — there is no fallback arithmetic here.
    if _forage_is_kept(tile_info):
        return _kept_forage_yield_model(band, tile_info, floor, workers, improvement, reaches, notes,
            forecast, overdraws, output)
    # **THE HEADLINE IS WHAT LANDS NEXT TURN, NOT THE ROOM STANDING NOW.** The sim regrows a whole
    # stage before it harvests, so a patch held at its floor pays its regrowth while the standing room
    # is empty — which is what had this readout quoting `0.00 FOOD` beside a work board quoting
    # `+0.96 /turn` for the same tile. The presets above and the worker cap keep the ROOM: those ask
    # what is takeable ONCE and how many hands the standing stock can use.
    var actual := SourceForecast.expected_next_turn_yield(forecast, workers, band)
    # Fodder names no whole-animal quantum anywhere — no animal pays it — which is why these calls
    # leave the engagement arm's key defaulted and the arm drops out.
    var actual_fodder := SourceForecast.expected_yield_account(
        forecast, workers, band, "per_worker_fodder", "next_ceiling_fodder")
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
    # …and the MATERIALS through the SAME forward room, so every account on the row describes one
    # turn: a forward food figure beside a standing material one is two turns stated on one line.
    var materials := SourceForecast.scaled_material_rows(
        SourceForecast.expected_materials(float(workers), forecast,
            SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY), output)
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
    return _forage_model_rows(tile_info, actual, actual_fodder, zero_account, after, materials,
        overdraws, notes)

## **THE FORAGE READOUT'S SHARED TAIL** — the rows, the wild-fodder lock and the model — for the closed
## form and the kept patch's curve alike, so the lock and the joined sentence cannot differ by path.
func _forage_model_rows(tile_info: Dictionary, actual: float, actual_fodder: float,
        zero_account: String, after: Dictionary, materials: Array[Dictionary], overdraws: bool,
        notes: Array[String]) -> Dictionary:
    var rows := SourceForecast.yield_rows(actual, actual_fodder, zero_account, after, materials)
    if rows.is_empty():
        # The patch pays in NO account at all — there is no line to draw rather than a zero to print.
        # **A NARROWING TO CASH CROPS ALONE NO LONGER LANDS HERE**, and that is what the per-species
        # material rates bought: cotton pays `0.0` food and `0.0` hay honestly, and its fibre is now a
        # composed row like any other, so the readout states what the crew brings home instead of an
        # apology. What still reaches this line is a selection that genuinely pays into nothing.
        return _wordless_take_model(notes)
    return {
        YIELD_MODEL_ROWS: rows,
        # The joined sentence has no room for the reason, so it must not promise the account at all.
        YIELD_MODEL_TEXT: SourceForecast.yield_components(
            actual, actual_fodder, zero_account, materials),
        YIELD_MODEL_OVERDRAW: overdraws,
        YIELD_MODEL_NOTES: notes,
    }

## ---- THE SELECTIVE GATHER — which plants this crew carries home ---------------------------------
##
## A `Forage` assignment names the species it carries; naming none takes the whole basket, exactly as
## before. Two keys travel together because they answer one question in two halves — *is this crew
## narrowed*, and *can the wire price that narrowing*.
const TAKE_SELECTION_NARROWED := "narrowed"
const TAKE_SELECTION_QUOTED := "quoted"
## The composed rates themselves (`SourceForecast.selection_rates`' answer), carried on the state so
## the source is narrowed from the SAME composition the quoted/unquoted verdict was taken from.
const TAKE_SELECTION_RATES := "rates"

## Resolve the composed selection against the tile's basket — ONCE per render, above every reading.
##
## **THE CLIENT CAN PRICE A NARROWING NOW, AND IT PRICES IT FROM THE WIRE'S OWN PER-SPECIES RATES.**
## `provisionsPerBiomass` on the patch is the BASKET AVERAGE, which is why this sheet could once only
## sit still when a chip was ticked; `compositionProvisionsPerBiomass` and its fodder twin are the same
## quantity per plant, index-aligned with the basket, and `SourceForecast.selection_rates` is the one
## place they are composed (`Σ(share × rate) ÷ Σ share`, never a sum across species).
##
## **THE MATERIAL ACCOUNT COMPOSES THE SAME WAY, per material id.** It was the one silence this sheet
## still owed for a release — `material_per_biomass` on the patch is basket-averaged — and
## `compositionMaterialPerBiomass` closes it, which matters more than the two scalars did: baskets are
## made of fibre, baskets are what let a gatherer carry more food, so *tick cotton, see how much fibre*
## is the first thing a player tries.
func _selective_take_state(basket: Array[Dictionary],
        selection: PackedStringArray) -> Dictionary:
    if selection.is_empty():
        return {TAKE_SELECTION_NARROWED: false, TAKE_SELECTION_QUOTED: false,
            TAKE_SELECTION_RATES: {}}
    var rates := SourceForecast.selection_rates(basket, selection)
    return {
        TAKE_SELECTION_NARROWED: true,
        TAKE_SELECTION_QUOTED: bool(rates.get(SourceForecast.SELECTION_KNOWN, false)),
        TAKE_SELECTION_RATES: rates,
    }

## **THE PATCH AS THE TICKED PLANTS ALONE** — the ONE substitution the whole narrowed sheet is built
## on. Everything downstream (the forecast, the worker cap, the floor presets, the chart's two crew
## targets, the readout and its `now → after` walk) reads this dict through the code it already read
## the whole patch through, which is what makes a chip tick and a stepper tick move the same numbers
## the same way.
##
## **THE RAW PATCH SURVIVES FOR EVERYTHING THAT IS NOT A TAKE** — the basket itself, the commit crop,
## the improvement control and its deal row — because those are facts about the ground rather than
## about what this crew carries home.
func _forage_take_source(tile_info: Dictionary, take: Dictionary) -> Dictionary:
    return SourceForecast.narrowed_source(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        take.get(TAKE_SELECTION_RATES, {}) as Dictionary)

## The asides a narrowed take owes — `[]` for the whole basket AND for a fully priced narrowing, which
## is what keeps every other sheet on this controller unchanged.
##
## **ONE OF THE TWO UNQUOTABLE STATES SAYS NOTHING, AND THAT IS THE INTENT.** `selection_rates`
## returns its unquotable dict for two — the wire priced no per-species rate for a plant that IS in
## the basket, and the ticked plants are not in this tile's basket at all. Only the first is a fact
## the player can neither see nor fix, so only it gets a sentence; the second is a state the sim now
## prunes on commit, and where the game knows what happened it should behave correctly rather than
## narrate it. An empty `notes` there renders `_wordless_take_model`'s `{}`, i.e. nothing at all.
##
## ⛔ **THE TWO-STATE DISTINCTION IS STILL WHAT THIS BRANCH READS**, and dropping it would put the
## surviving sentence back on the state it is false about — a crew told the SERVER had not priced
## plants that had simply stopped growing there. `_selective_take_state` carries the same dict the
## quoted/unquoted verdict was taken from, so the sentence and the verdict cannot disagree about
## which state this is.
func _take_notes(take: Dictionary) -> Array[String]:
    var notes: Array[String] = []
    if not bool(take.get(TAKE_SELECTION_NARROWED, false)):
        return notes
    if not bool(take.get(TAKE_SELECTION_QUOTED, false)):
        var rates: Dictionary = take.get(TAKE_SELECTION_RATES, {}) as Dictionary
        var reason := String(rates.get(SourceForecast.SELECTION_REASON,
            SourceForecast.SELECTION_REASON_ABSENT))
        if reason == SourceForecast.SELECTION_REASON_UNPRICED:
            notes.append(HudFloraVocab.TAKE_UNQUOTED_NOTE)
    return notes

## **THE CREW A CURVE SAYS REACHES THE FLOOR** - `min(pool, useful)`. While the curve is in flight
## (`NO_CREW_ANSWER`) only the pool bounds it.
func _curve_worker_cap(useful: int, assignable: int) -> int:
    if useful == SourceForecast.NO_CREW_ANSWER:
        return assignable
    return mini(useful, assignable)

## **THE CEILING THE STANDING SUMMARY'S OVERSTAFF NOTE QUOTES** — the row's own ceiling where the
## wire published it, the Work board's number, so the drawer's second line and the board row read one
## figure: `worked_row_published_ceiling` over the LIVE source on a food row (the bare patch, or the
## herd from the world list — herds migrate), `published_useful_cutters` on a working.
func _overstaff_note_ceiling(assignment: Dictionary, kind: String) -> int:
    if kind == HudConst.LABOR_KIND_EXTRACT:
        return HudDepositVocab.published_useful_cutters(assignment)
    var src: Dictionary = _band_labor.forage_patch_lookup().get(Vector2i(
        int(assignment.get("target_x", -1)), int(assignment.get("target_y", -1))), {}) \
        if kind == SourceForecast.LABOR_KIND_FORAGE \
        else _band_labor.find_world_herd(String(assignment.get("fauna_id", "")))
    return SourceForecast.worked_row_published_ceiling(kind, assignment, src)

## **THE KIT THE BAND'S OWN ROW ON THIS SOURCE CARRIES**, or `NO_KIT_ID` where it has none — the
## sheet's kit SEED, beside the floor and take selection it already seeded from that row. Read off the
## EFFECTIVE worker map, so a pending re-kit counts. `NO_KIT_ID` resolves to the source's own default.
func _standing_kit_id(band: Dictionary, key: String) -> String:
    var row: Dictionary = _band_labor.effective_worker_map(band).get(key, {})
    return String(row.get(KitRoster.ROW_KIT_ID_KEY, KitRoster.NO_KIT_ID))

## **A KEPT PATCH'S CREW COUNTS NAME THE WHOLE CREW THE STEPPER SETS** — both pills. Each pill's take target
## (`crew_to_clear` / `crew_to_hold`, in gathering hands) converted through the patch curve's own
## `keep_hands` (`SourceForecast.forage_curve_crew_for`), the conversion the cap goes through too, so
## the pills, the cap note and the stepper are one count. While the curve is in flight neither pill
## names a crew. An unkept patch passes through: its take hands ARE its crew.
func _kept_crew_targets(model: Dictionary, kept: bool) -> Dictionary:
    if not kept or not bool(model.get("known", false)):
        return model
    var out := model.duplicate()
    var ready := String(_forage_live_crew_view.get("state", "")) == ForecastQuery.STATE_READY
    var per_crew: Array = (_forage_live_crew_view.get("answer", {}) as Dictionary).get("per_crew", [])
    for key in ["crew_to_clear", "crew_to_hold"]:
        out[key] = SourceForecast.forage_curve_crew_for(per_crew,
            int(model.get(key, SourceForecast.NO_CREW_ANSWER))) \
            if ready else SourceForecast.NO_CREW_ANSWER
    return out

## **THE HANDS OF `workers` THAT GATHER ON A KEPT PATCH** — `workers - keep_hands` off the curve's row,
## which `floor_chart_model` walks the stock with. `WHOLE_CREW_TAKES` on an unkept patch, and while the
## curve is in flight (the chart then walks the whole crew until the answer lands).
func _kept_take_crew(kept: bool, workers: int) -> float:
    if not kept or String(_forage_live_crew_view.get("state", "")) != ForecastQuery.STATE_READY:
        return SourceForecast.WHOLE_CREW_TAKES
    var row := SourceForecast.forage_crew_row((_forage_live_crew_view.get("answer", {})
        as Dictionary).get("per_crew", []), workers)
    if row.is_empty():
        return SourceForecast.WHOLE_CREW_TAKES
    return maxf(float(workers) - float(row.get(SourceForecast.FORAGE_CREW_KEEP_HANDS_KEY, 0.0)), 0.0)

## **THE DEAL OFF THE CREW CURVE** — the row at `crew`, whose `next_rung_*` fields are the whole
## `ONCE SOWN` / `ONCE TENDED` line. `{}` while the curve is pending or refused, and where it has no
## row for this crew, so the deal row is not drawn rather than drawn with a crew-blind number.
func _curve_next_rung_row(view: Dictionary, crew: int) -> Dictionary:
    if String(view.get("state", "")) != ForecastQuery.STATE_READY:
        return {}
    var row := SourceForecast.forage_crew_row((view.get("answer", {}) as Dictionary).get(
        "per_crew", []), crew)
    if row.is_empty() or not row.has(SourceForecast.FORAGE_CREW_NEXT_RUNG_TAKE_KEY):
        return {}
    return row


## Does this patch owe keeping? The gate on reading its take off the crew curve.
func _forage_is_kept(tile_info: Dictionary) -> bool:
    return SourceForecast.has_upkeep(SourceForecast.upkeep_state(tile_info,
        HudComposeVocab.FORAGE_FORECAST_PREFIX))

## **A KEPT PATCH'S READOUT, OFF THE CREW CURVE.** Food is the row's `take`; fodder, materials and the
## hold rate are priced on the gatherers the row names (`workers - keep_hands`), so every account on
## the line describes the same hands. Pending or refused, the readout states the seam's sentence.
func _kept_forage_yield_model(band: Dictionary, tile_info: Dictionary, floor: float, workers: int,
        improvement: String, reaches: bool, notes: Array[String], forecast: Dictionary,
        overdraws: bool, output: float) -> Dictionary:
    var state := String(_forage_live_crew_view.get("state", ForecastQuery.STATE_PENDING))
    if state != ForecastQuery.STATE_READY:
        var waiting: Array[String] = notes.duplicate()
        waiting.append(HudComposeVocab.HUNT_TAKE_PENDING if state == ForecastQuery.STATE_PENDING \
            else HudComposeVocab.FORECAST_FAILED_FORMAT % String(_forage_live_crew_view.get("error", "")))
        return _wordless_take_model(waiting)
    var per_crew: Array = (_forage_live_crew_view.get("answer", {}) as Dictionary).get("per_crew", [])
    var row := SourceForecast.forage_crew_row(per_crew, workers)
    var actual := float(row.get(SourceForecast.FORAGE_CREW_TAKE_KEY, 0.0))
    var gatherers := maxf(float(workers) - float(row.get(SourceForecast.FORAGE_CREW_KEEP_HANDS_KEY,
        0.0)), 0.0)
    var fodder := minf(gatherers * float(forecast.get("per_worker_fodder", 0.0)),
        float(forecast.get("next_ceiling_fodder", 0.0))) * output
    var materials := SourceForecast.scaled_material_rows(SourceForecast.expected_materials(
        gatherers, forecast, SourceForecast.MATERIAL_CEILING_NEXT_TURN_KEY), output)
    var after := {}
    if _walks_to_the_floor(reaches, improvement):
        after = {
            SourceForecast.YIELD_ACCOUNT_FOOD: minf(gatherers * float(forecast.get("per_worker", 0.0)),
                float(forecast.get("hold_ceiling", 0.0))) * output,
            SourceForecast.YIELD_ACCOUNT_FODDER: minf(gatherers * float(forecast.get(
                "per_worker_fodder", 0.0)), float(forecast.get("hold_ceiling_fodder", 0.0))) * output,
        }
    return _forage_model_rows(tile_info, actual, fodder, String(forecast["zero_account"]), after,
        materials, overdraws, notes)

## **THE PATCH CREW CURVE, COMPOSED AND ASKED** — the deposit curve's twin: the SUBJECT is band + tile,
## the key adds the kit, the floor, the take selection, the band's gear and the POOL, so a stepper tick
## re-reads rows the seam already holds. Read exactly, for the deposit curve's reason.
func _forage_crew_take_ask(band: Dictionary, tile_info: Dictionary, take_species: Array,
        kit_id: String, floor: float, max_workers: int, crop: String) -> Dictionary:
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var workers := _crew_take_workers(max_workers)
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_FORAGE_CREW_TAKE, band_id,
        "%d,%d" % [x, y])
    var key := ForecastQuery.key_of(subject, kit_id, workers, floor, band, _band_labor.kits())
    if not take_species.is_empty():
        key += ":" + ",".join(PackedStringArray(take_species))
    # **THE PICKED CROP IS A TERM OF THE QUESTION** — the rows' `next_rung_*` are priced on it, so a
    # different pick is a different answer and must be asked again.
    key += ":crop=" + crop
    # **THE STAND AND ITS BILL ARE TERMS OF THE QUESTION** — every row is bounded by next turn's room
    # above the floor and starts by taking the keeping out of the crew, so a turn that moves the
    # patch's biomass or its keeping is a new question rather than the previous turn's answer.
    key += ":%f:%f:%f" % [
        float(tile_info.get(HudComposeVocab.FORAGE_FORECAST_PREFIX + "biomass", 0.0)),
        float(tile_info.get(HudComposeVocab.FORAGE_FORECAST_PREFIX + "upkeep_demand", 0.0)),
        SourceForecast.upkeep_hands(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX)]
    return {
        "subject": subject, "key": key,
        "askable": workers > 0 and band_id != HudConst.NO_BAND_ID and x >= 0 and y >= 0,
        "params": {
            "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id, "x": x, "y": y,
            "take_species": take_species, "kit_id": kit_id, "floor": floor,
            "max_workers": workers, "crop": crop,
        },
    }

func _forage_crew_take_view(band: Dictionary, tile_info: Dictionary, take_species: Array,
        kit_id: String, floor: float, max_workers: int, crop: String) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var ask := _forage_crew_take_ask(band, tile_info, take_species, kit_id, floor, max_workers,
        crop)
    if not bool(ask["askable"]):
        return {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
    _forecast_query.ask(ForecastQuery.KIND_FORAGE_CREW_TAKE, String(ask["subject"]),
        String(ask["key"]), ask["params"])
    return _forecast_query.view_exact(String(ask["subject"]), String(ask["key"]))

## The same question while the floor is moving — `_drag_deposit_crew_take`'s twin, on the same rate
## limit and members.
func _drag_forage_crew_take(band: Dictionary, tile_info: Dictionary, take_species: Array,
        kit_id: String, floor: float, max_workers: int, crop: String) -> void:
    if _forecast_query == null:
        return
    var ask := _forage_crew_take_ask(band, tile_info, take_species, kit_id, floor, max_workers,
        crop)
    if not bool(ask["askable"]):
        _forage_live_crew_view = {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
        return
    var key := String(ask["key"])
    var now := Time.get_ticks_msec()
    if key != _crew_take_drag_asked_key \
            and now - _crew_take_drag_asked_at_msec \
                >= HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC:
        _crew_take_drag_asked_key = key
        _crew_take_drag_asked_at_msec = now
        _forecast_query.ask(ForecastQuery.KIND_FORAGE_CREW_TAKE, String(ask["subject"]), key,
            ask["params"])
    _forage_live_crew_view = _forecast_query.view_exact(String(ask["subject"]), key)

## A model with NO numbers in it — the shape every "there is nothing this sheet may state" path
## returns. `{}` when there is not even an aside to print, which is the whole-basket sheet's own
## unknown-source answer and leaves it byte-identical.
func _wordless_take_model(notes: Array[String]) -> Dictionary:
    if notes.is_empty():
        return {}
    return {
        YIELD_MODEL_ROWS: ([] as Array[Dictionary]),
        YIELD_MODEL_TEXT: "",
        YIELD_MODEL_OVERDRAW: false,
        YIELD_MODEL_NOTES: notes,
    }

## **THE SPECIES CHIP ROW, ITS CONSEQUENCE LINE AND ITS IDLE WARNING** — the whole selective-gather
## control, mounted as one block so the row and the sentence explaining it cannot be composed apart.
##
## `single_pick` is the VERB: foraging takes several plants, cultivating commits the ground to one.
## The two are the same chips in the same place, so the row's KEY and its consequence line fork on it.
## **The chips themselves do not**: both modes draw one selected pill per chosen plant, because that
## is the whole of what a chip has to say, and the difference between them — picking one clears the
## other — is a thing the pills already show as it happens.
##
## `ground_committed` is the crop the PATCH is already committed to (`patch_committed_species`), `""`
## on ground that is not; `rung_percent` is the composed rung's own meter. On committed ground the lit
## chip and the line are that crop: the composition sends none there, so `_compose.forage_species()`
## is `""` and would light nothing and name nothing.
func _mount_take_chips(target: VBoxContainer, basket: Array[Dictionary],
        selection: PackedStringArray, single_pick: bool,
        crop_is_default: bool, crop_rung: String, rebuild: Callable,
        ground_committed: String, rung_percent: int, offsite_label: String = "") -> void:
    if basket.is_empty():
        return
    var committed := ground_committed if ground_committed != "" else _compose.forage_species()
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_child(HudWidgets.alloc_section_label(
        HudFloraVocab.TAKE_ROW_LABEL_SINGLE if single_pick else HudFloraVocab.TAKE_ROW_LABEL))
    var chip_entries := _take_chip_entries(basket, selection, single_pick, committed)
    if ground_committed != "":
        var reason := HudFloraVocab.TAKE_CHIP_COMMITTED_REASON_FORMAT % _take_display_name(
            ground_committed, basket)
        for chip_entry in chip_entries:
            (chip_entry as Dictionary)[HudWidgets.SPECIES_CHIP_DISABLED_REASON_KEY] = reason
    block.add_child(HudWidgets.build_species_chips(chip_entries,
        func(species: String) -> void:
            if single_pick:
                # A commit crop is one plant, so a chip WRITES it rather than toggling a set —
                # re-picking the lit one is a no-op, exactly as re-picking a selected option is.
                _compose.set_forage_species(species)
            else:
                # **THE BASKET'S KEYS, NOT ITS SIZE.** The model needs them at BOTH ends: to expand an
                # implicit-all before removing the pressed plant (without which pressing one chip moves
                # every other), and to collapse a fully-ticked selection back to the empty default.
                _compose.toggle_forage_take_species(species, _take_basket_keys(basket))
            rebuild.call()))
    # **THE FORAGE SIDE MOUNTS NO LINE, and the empty answer must not become an empty Label.** A
    # zero-height hint still spends the block's separation, so the chips would sit over a gap that
    # nothing draws in. `""` is the whole of the forage answer now (`_take_consequence_note`).
    var consequence := _take_consequence_note(single_pick, crop_is_default, crop_rung, committed,
        basket, ground_committed != "")
    # Off the sites the crop IS picked (the default is a pick), and this order raises the farm of it.
    if offsite_label != "" and ground_committed == "" and committed != "":
        consequence = HudFloraVocab.OFFSITE_CREW_LINE_FORMAT % [offsite_label,
            _take_display_name(committed, basket)]
    if consequence != "":
        block.add_child(HudWidgets.alloc_hint_label(consequence))
    # **THE IDLE WARNING IS NOT WRITTEN HERE, AND NOT BECAUSE IT WAS DROPPED.** It is the crew
    # stepper's own `max N useful here — more would be idle`, one control up, and that note now moves
    # with the chips: the cap divides the NARROWED patch's ceiling
    # (`SourceForecast.max_useful_workers` over `_forage_take_source`), so ticking a scarce plant
    # lowers the useful count and the stepper says so in the words it already uses for every other way
    # of running out of useful hands. A second sentence down here would be a second producer of one
    # verdict — the shape this arc has shipped three defects of — and the two would drift the first
    # time either moved.
    target.add_child(block)

## One chip per named plant, written EXACTLY as the tile card writes it — the card's own icon, share
## and biomass-clause consts, so the two surfaces cannot come to spell one stand two ways.
func _take_chip_entries(basket: Array[Dictionary], selection: PackedStringArray, single_pick: bool,
        committed: String) -> Array:
    var entries: Array = []
    for entry in basket:
        var species := String(entry.get("species", ""))
        if species == "":
            continue
        var face := String(FoodIcons.for_crop_role(String(entry.get("role", ""))))
        if face != "":
            face += " "
        face += HudFloraVocab.FLORA_SHARE_FORMAT % [
            String(entry.get("display_name", "")), int(entry.get("percent", 0))]
        # **THE BRACKETED NUMBER IS THE WIRE'S OWN**, and an unstated one renders NO clause rather
        # than a `(0)` — a stand the server quoted no quantity for is not a stand drawn to nothing.
        if bool(entry.get("has_standing_biomass", false)):
            face += HudFloraVocab.FLORA_SHARE_BIOMASS_CLAUSE_FORMAT % int(round(
                float(entry.get("standing_biomass", 0.0))))
        else:
            face += HudFloraVocab.TAKE_CHIP_UNKNOWN_BIOMASS
        # **NO TOOLTIP.** A hover that restates the chip verbatim earns nothing and costs the row it
        # covers — measured in play, `Tobacco Fields 57% (117)` hovering over the row beneath itself.
        # The face is already the whole of what this control knows.
        entries.append({
            "species": species,
            "face": face,
            "state": _take_chip_state(species, selection, single_pick, committed),
        })
    return entries

## The take selection as the CHIPS render it, which is what the model toggles against — a `String`
## key per named plant, in the basket's own order.
func _take_basket_keys(basket: Array[Dictionary]) -> PackedStringArray:
    var keys := PackedStringArray()
    for entry in basket:
        var species := String(entry.get("species", ""))
        if species != "":
            keys.append(species)
    return keys

## **TWO STATES, AND AN EMPTY SELECTION IS EVERY PLANT SELECTED** — that is what it means on the wire,
## so that is what it draws. A third *default-included* state existed only while a click on one chip
## could move another; the toggle no longer can, so a settled selection and a picked one are one
## state. The single-pick side is the same rule with a set of at most one: the committed crop, whether
## the player named it or the resolver settled it, and nothing else.
func _take_chip_state(species: String, selection: PackedStringArray, single_pick: bool,
        committed: String) -> String:
    if single_pick:
        return HudFloraVocab.TAKE_STATE_SELECTED if species == committed \
            else HudFloraVocab.TAKE_STATE_UNSELECTED
    if selection.is_empty() or selection.has(species):
        return HudFloraVocab.TAKE_STATE_SELECTED
    return HudFloraVocab.TAKE_STATE_UNSELECTED

## What committing this ground COSTS, in one sentence — and the sentence exists on the CULTIVATE side
## only. With nothing picked it NAMES the crop the game would settle on, because silence there is the
## game choosing for the player without saying so; with a crop picked it says what committing does to
## the rest of the stand.
##
## **FORAGING ANSWERS `""`, and it is a deletion rather than a gap** (`HudFloraVocab`, the consequence
## block): the two selection sentences restated the chip row directly above them, and the refusal
## sentence — shown when the last remaining plant was unticked — was verbosity over a fact a player
## discovers by pressing the chip. The refusal is still ENFORCED in `ComposeState`; it is simply
## silent, and the chips are not greyed to announce it.
##
## ⛔ **AN EMPTY CROP NAMES NOTHING, AND NO LINE IS DRAWN AROUND IT.** Each branch formats a crop name
## into a sentence; a `""` there renders a sentence ending on a blank (`…committed to .`), so a branch
## with no name to state answers `""` and the sheet mounts no line.
func _take_consequence_note(single_pick: bool, crop_is_default: bool,
        crop_rung: String, committed: String, basket: Array[Dictionary],
        ground_is_committed: bool) -> String:
    if not single_pick or committed == "":
        return ""
    # Ground already committed to a crop states nothing: the lit (disabled) chip says which crop, and
    # "Nothing picked" would be false.
    if ground_is_committed:
        return ""
    if crop_is_default:
        return HudFloraVocab.TAKE_NOTE_CULTIVATE_DEFAULT_FORMAT % _take_display_name(
            committed, basket)
    return HudFloraVocab.TAKE_NOTE_CULTIVATE_NARROWED_FORMAT % String(
        HudComposeVocab.IMPROVEMENT_RUNNING_LABELS.get(crop_rung, crop_rung))

func _take_display_name(species: String, basket: Array[Dictionary]) -> String:
    for entry in basket:
        if String(entry.get("species", "")) == species:
            return String(entry.get("display_name", species))
    return species

## One yield model → the one-line BBCode preview, for both webs. Green + `· renewable` inside the
## source's own regrowth, WARN-amber with the shared ⚠ and the web's own overdraw clause outside it;
## no waste note rides it: a kill is kept whole, so a hunt has nothing to flag as wasted.
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
    return body

## A "Band: [▼]" dropdown row for the assign controls: lists every player band by the client's one
## naming rule (`HudFormat.band_name`, so the picker and the map cannot disagree) and selects
## `selected_band`; `on_pick` fires with the chosen band dict. The actor band is always explicit — shown even with one band (single-item
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
            "label": HudFormat.band_name(band),
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

## **THE CREW A NEW SOURCE WOULD BE STAFFED WITH** (#871): `min(the band's assignable workers, the
## forecast's max-useful crew)`. The hands that would reach the floor are the sim's own answer
## (`SourceForecast.max_useful_workers`, which already folds in the *hold it after* crew and, on a
## hunt, the plateau of the crew-take curve - bounded by the hands that can bring the animal down), so
## this only holds that figure to the pool. `assignable` is the source's crew pool - idle plus what the
## band already has on it (`HudBandLaborState.source_crew_pool_forage`).
##
## > #### ⛔ THE STAGED COUNT IS NO LONGER A CREW, AND THERE IS NO STANDING-CREW FLOOR HERE
## >
## > The cap used to clamp a count the commit sent as the crew, and a cap below the committed crew
## > silently dropped the difference. The commit sends a MAX now and a source the band already works
## > states the sim's own crew, so a cap can no longer unassign anybody; it only sizes the forecast of a
## > source nobody works yet.
func _forecast_worker_cap(forecast: Dictionary, assignable: int) -> int:
    var useful := SourceForecast.max_useful_workers(forecast)
    if useful == SourceForecast.MAX_USEFUL_UNBOUNDED:
        return assignable
    return mini(useful, assignable)

## **THE ROAD LADDER'S GEAR TERM, ONE ANSWER PER RUNG** — `{rung_key: build_gear}`, which
## `RungLadder.route_track` looks each row's answer up in.
##
## **A rung the roster serves with nothing answers `{}`**, which `SourceForecast.build_turns_at`
## reads as the ungeared case — the honest reading for a branch no tool on this roster helps.
func _road_ladder_kit_gear(band: Dictionary, ladder: Array[Dictionary]) -> Dictionary:
    var kits := _band_labor.kits()
    var by_rung: Dictionary = {}
    for entry in ladder:
        var rung_key := HudRouteVocab.catalog_rung_key(entry)
        if rung_key == "":
            continue
        by_rung[rung_key] = KitRoster.build_gear(band,
            KitRoster.build_kit_for_branch(kits, KitRoster.BUILD_BRANCH_ROUTE, rung_key),
            KitRoster.BUILD_BRANCH_ROUTE, rung_key)
    return by_rung


## Send the player to a band's Work board, **and put this sheet away on the way**.
##
## ⛔ **THE CLOSE IS NOT TIDINESS — WITHOUT IT THE SHEET SABOTAGES THE AFFORDANCE IT JUST OFFERED.**
## The board this link jumps to is `BandCityPanel`, on CanvasLayer 103; the sheet is on
## `HudLayer.COMPOSE_LAYER_INDEX` (105), above the event dock's 104, and its dismiss catcher covers
## the whole viewport at `MOUSE_FILTER_STOP`. So a sheet left open lands the player on a board whose
## first click — the `⌃` this very sentence told them to press — is swallowed as a dismissal.
## The sheet is a transient write surface and the panel is the persistent one; one click to put the
## sheet away is right when the player chose to leave, and simply wrong when the sheet itself sent
## them there.
##
## It is the ONE control inside the sheet that navigates to another surface — the sheet's other
## outward signals commit a command and close through their own `close_compose_sheet()` — so this is
## the whole of the exemption, not the first of a family.
##
## **THE ROAD LADDER'S RUNG PRESS IS A SECOND CALLER, and it is not a second exemption.** That card is
## its own Window and puts ITSELF away first (`_dismiss_road_ladder`); what it wants from here is the
## jump and the sheet-close beside it, both of which are exactly what a press landing on the Work
## board needs. A rung already being built calls this ALONE, no command being issued at all.
func _navigate_to_work_tab(band_entity: int) -> void:
    close_compose_sheet()
    emit_signal("work_tab_requested", band_entity)


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



## **WHAT THE SHEET SHOWS INSTEAD IS ALREADY ON THE CONTROL**: the rung's `<rung>WorkCost` (the pile),
## its `<rung>UpkeepDemand` (the rate it costs to hold, forever) and the chained
## `buildTurnsRemaining` this source would get if declared now — all three published before anything is
## declared. With nobody on the build the sim answers *no estimate*, and the face says so plainly
## rather than rendering nothing.



## **THE OFFER'S PRICE LEFT THE SHEET** (the Work row's `⌃` tooltip states it), so two small composers
## remain here: `_improvement_offer_phrase` (the pointer sentence) and `_improvement_rung_face` (the
## bare rung, which DECLARED puts `◷ Queued` beside).


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
## **THE ROW IS THE PAYOFF TERMS, AND NOTHING SUBTRACTED FROM THEM.** Corral used to append
## `− N.NN feed` here, quoting `pen_upkeep` against a GROSS `corralYield`; that food-unit pen bill is
## retired, so `corralYield` is what the pen actually pays and the row states it bare. There is no
## dangling separator and no empty slot — the term is gone, not blanked. The rung's standing price is
## WORK and is stated on the work row's `⌃` tooltip, the same place every other rung's is.
func _improvement_deal_row(kind: String, source: Dictionary, prefix: String, band: Dictionary,
        rung: String, payoff_terms: String, keeping_note: Dictionary = {}) -> Dictionary:
    if rung == "" or payoff_terms == "":
        return {}
    var label := String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS.get(rung, rung))
    var row := {
        HudWidgets.IMPROVEMENT_DEAL_ROW_LABEL: label,
        HudWidgets.IMPROVEMENT_DEAL_ROW_VALUE: payoff_terms,
    }
    if not keeping_note.is_empty():
        row[HudWidgets.IMPROVEMENT_DEAL_ROW_NOTE] = String(keeping_note["text"])
        # The rung's past-tense word comes off the SAME label the row wears, never a hard-coded one.
        row[HudWidgets.IMPROVEMENT_DEAL_ROW_NOTE_TIP] = HudComposeVocab.DEAL_ALL_KEEP_TOOLTIP_FORMAT \
            % [label.trim_prefix(HudComposeVocab.DEAL_LABEL_PAST_PREFIX), label]
    return row

## **WHY A DEAL READS ZERO WHEN THE CREW IS ALL KEEPERS** - `{text}` for the clause appended to the
## deal row, `{}` when it does not apply. `curve` is the crew curve's `per_crew` rows, `row` the one at
## the stepper's `crew`, `take_key` the row's next-rung take (`next_rung_take` on the forage curve,
## `next_rung_animals_likely` on the hunt curve - twins, both publishing `next_rung_keep_hands`). It
## applies when that take has no component AND the rung's keeping bill takes every hand of the crew.
## The second figure is the smallest crew on the curve that gathers, off the same rows.
func _next_rung_keeping_note(curve: Array, row: Dictionary, crew: int, take_key: String) -> Dictionary:
    if row.is_empty() or crew <= 0 or not row.has(take_key) \
            or not row.has(SourceForecast.NEXT_RUNG_KEEP_HANDS_KEY):
        return {}
    if SourceForecast.has_component(float(row[take_key])):
        return {}
    if float(row[SourceForecast.NEXT_RUNG_KEEP_HANDS_KEY]) \
            < float(crew) - SourceForecast.CREW_TAKE_REACH_TOLERANCE:
        return {}
    var gathers := 0
    for candidate in curve:
        if candidate is Dictionary and SourceForecast.has_component(float(
                (candidate as Dictionary).get(take_key, 0.0))):
            var workers := int((candidate as Dictionary).get(SourceForecast.CREW_TAKE_WORKERS_KEY, 0))
            if gathers == 0 or workers < gathers:
                gathers = workers
    return {"text": (HudComposeVocab.DEAL_ALL_KEEP_FORMAT % [crew, gathers]) if gathers > 0 \
        else HudComposeVocab.DEAL_ALL_KEEP_NONE_FORMAT % crew}

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

## **THE HUNT DEAL OFF THE CREW CURVE** — the row's `next_rung_animals_likely` at `crew`, valued in
## the herd's own accounts through the one biomass crossing the readout's take uses
## (`rescaled_from_biomass` over the body quantum), so the deal and the take above it are one
## register. `""` while the curve is pending or refused, or where it has no row for this crew.
func _hunt_curve_payoff_terms(herd: Dictionary, view: Dictionary, crew: int) -> String:
    if String(view.get("state", "")) != ForecastQuery.STATE_READY:
        return ""
    var row := SourceForecast.hunt_crew_take_row((view.get("answer", {}) as Dictionary).get(
        "per_crew", []), crew)
    if row.is_empty() or not row.has(SourceForecast.HUNT_CREW_NEXT_RUNG_ANIMALS_KEY):
        return ""
    var prefix := HudComposeVocab.BARE_FORECAST_PREFIX
    var accounts := SourceForecast.rescaled_from_biomass(herd, prefix,
        float(row[SourceForecast.HUNT_CREW_NEXT_RUNG_ANIMALS_KEY])
            * SourceForecast.body_quantum(herd, prefix))
    return SourceForecast.picker_products(float(accounts[SourceForecast.YIELD_ACCOUNT_FOOD]),
        float(accounts[SourceForecast.YIELD_ACCOUNT_FODDER]), SourceForecast.YIELD_ACCOUNT_FOOD,
        accounts[SourceForecast.RESCALED_MATERIALS_KEY])

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

##
## **THE MECHANICAL REASON IT ENDED UP HERE** was that `RungLadder.has_track` is FALSE when nothing
## sits above the standing rung, and `animal:pen` is the top of the animal branch — so a corralled
## herd's work row rendered **no `⌃` at all** and there was no affordance to hang it on. Extending a
## pen is precisely what you do *after* the ladder is finished. `selection-card.md` blamed it on being
## *"a one-click standing action, not a compose flow"*, which was true and was the second reason.
##
## **IT IS A `⌃` ON THE STANDING-RUNG MARK NOW** (`BandPanelController._open_ring_card`) — the mark
## sits on the thing the job acts on, a ring widening the pen the mark denotes, and the track stays
## one position on a ladder. It opens a PRICE rather than committing on the click, because a ring
## draws `animal:pen`'s own hurdle pile since §2.7 and a one-click button stated that cost nowhere.
##
## `PEN_EXTEND_LABEL` / `PEN_EXTEND_TOOLTIP` / `PEN_FENCING_LABEL` retired with the control; the
## `extend_pen_requested` signal and `Main.format_extend_pen` did NOT — the command is unchanged and
## only its entry point moved.

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

## **THE WORK STRIP, UNDER THE CREW LABEL** (issue #877) — the same control the Work tab's row of this
## source wears: the rung mark, the `⌃` / filling chip, the floor glyph, `⚠`, then the crew number and its
## bar. It replaces the crew line AND the improvement control this sheet used to carry, so the row and the
## sheet can never disagree: both are built from `BandPanelController.source_strip_row`. A source the band
## does not work yet shows the strip at a crew of zero.
##
## `label_tooltip` is a hover on the SECTION LABEL, `""` for every caller that wants none (the deposit
## sheets' crew is the TAKE crew while the hands that HOLD a working are a pool on another panel).
## `warn_line` is the hunt sheet's gear-short sentence. Under the strip, the details the retired
## inspector carried and this sheet did not: the row's notes, its arrival ticks and the site's bill.
func _mount_work_strip(parent: VBoxContainer, band: Dictionary, kind: String, x: int, y: int,
        herd_id: String, material: String, crew_label: String, label_tooltip: String,
        warn_line: String, priced_crew: int, wire_source: Dictionary = {},
        offers_rung: bool = true) -> void:
    var source := _bandpanel.source_strip_row(band, kind, x, y, herd_id, material, wire_source)
    if source.is_empty():
        return
    var row: Dictionary = source["row"]
    var deposit: Dictionary = source["deposit"]
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_LABEL_SEPARATION)
    var row_label := HudWidgets.alloc_section_label(crew_label)
    row_label.set_meta(HudWidgets.CREW_ROW_LABEL_META, true)
    HudWidgets.set_label_tooltip(row_label, label_tooltip)
    block.add_child(row_label)
    var model := WorkStrip.model_from_row(row)
    # A sheet that IS the order for a rung (an off-site farm order, whose commit raises the farm) offers no
    # second `⌃` beside it: the strip still draws the rung mark and the crew.
    if not offers_rung and String(model.get(WorkStrip.M_LADDER_KIND, "")) == WorkStrip.LADDER_OFFER:
        model[WorkStrip.M_LADDER_KIND] = WorkStrip.LADDER_NONE
    var strip := WorkStrip.build(model, _bandpanel.strip_handlers(band, row, deposit, wire_source), true)
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The crew the sheet's readout below is PRICED at (the sim's forecast crew on a new source, the
    # standing one on a worked source), stamped for the harness: the strip itself draws the committed crew.
    strip.set_meta(HudComposeVocab.SHEET_CREW_NUMBER_META, priced_crew)
    block.add_child(strip)
    _mount_row_details(block, row)
    if warn_line != "":
        var warn := HudWidgets.build_wrapping_status_part(warn_line, HudStyle.WARN)
        warn.set_meta(HudComposeVocab.GEAR_SHORT_LINE_META, warn_line)
        block.add_child(warn)
    parent.add_child(block)

## **WHAT THE WORK-TAB INSPECTOR SHOWED THAT THE SHEET DID NOT**, now under the strip: the row's own
## note (the slipping-ground or staffing sentence, inked by its severity), the muted note, the
## `ArrivalStrip` of upcoming deliveries where the schedule has a gap, and whose build it is where
## another people holds this source. (The site's tending bill is the crew bar's and the ⚠'s hover.)
func _mount_row_details(block: VBoxContainer, row: Dictionary) -> void:
    # ANOTHER PEOPLE'S BUILD on this source: whose it is, in warn ink - nothing here moves that meter and
    # the strip offers no `⌃` on it, so the sheet says why in the Work row's own words.
    if String(row.get("foreign_build", "")) != "":
        block.add_child(HudWidgets.build_wrapping_status_part(String(row.get("foreign_build", "")),
            HudStyle.WARN))
    if String(row.get("note", "")) != "":
        block.add_child(HudWidgets.build_wrapping_status_part(String(row.get("note", "")),
            HudWorkVocab.note_color(String(row.get("note_severity",
                HudWorkVocab.NOTE_SEVERITY_WARN)))))
    if String(row.get("muted_note", "")) != "":
        block.add_child(HudWidgets.build_wrapping_status_part(String(row.get("muted_note", "")),
            HudStyle.INK_FAINT))
    var schedule: PackedFloat32Array = row.get("schedule", PackedFloat32Array())
    if ArrivalStrip.has_gap(schedule):
        var arrivals := ArrivalStrip.new()
        arrivals.set_schedule(schedule, _band_labor.current_turn())
        block.add_child(arrivals)


## **THE CREW THE SIM WILL STAFF** (#871). On a source the band works it is the snapshot row's own
## `workers`; on a new one it is the client's forecast crew (`cap` - the crew that reaches the floor,
## already bounded by the hands that can bring the animal down and by the pool) held to the player's
## Max (`0` = no Max).
func _staffed_crew_of(is_working: bool, standing_crew: int, cap: int, max_workers: int) -> int:
    if is_working:
        return standing_crew
    var crew := cap if cap > 0 else 0
    return mini(crew, max_workers) if max_workers > 0 else crew

## **THE STAFFING PREVIEW, COMPOSED AND ASKED** (`ForecastQuery.KIND_STAFFING_PREVIEW`) - the sim's dry
## run of the real staffing with this not-yet-worked row added at the sheet's Priority, Max, floor and
## kit. `source` is the ask's address (`source_kind` plus its keys); `source_tag` is the subject's tail;
## `extra_key` is whatever else the source adds to the question (a patch's ticked plants). Read exactly,
## like the curves: a different Priority or Max is a different question, not a tick of the same one.
func _staffing_preview_view(band: Dictionary, source: Dictionary, source_tag: String, kit_id: String,
        floor: float, priority: String, max_workers: int, extra_key: String = "") -> Dictionary:
    var pending := {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    if _forecast_query == null or band_id == HudConst.NO_BAND_ID:
        return pending
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_STAFFING_PREVIEW, band_id, source_tag)
    var key := ForecastQuery.preview_key_of(subject, kit_id, max_workers, floor, priority, band,
        _band_labor.kits(), extra_key)
    var params := source.duplicate()
    params["faction_id"] = int(band.get("faction", HudConst.PLAYER_FACTION_ID))
    params["band_id"] = band_id
    params["kit_id"] = kit_id
    params["floor"] = floor
    params["priority"] = priority
    params["max_workers"] = max_workers
    _forecast_query.ask(ForecastQuery.KIND_STAFFING_PREVIEW, subject, key, params)
    return _forecast_query.view_exact(subject, key)

## **THE CREW A NEW ROW WOULD BE GIVEN**: the preview's `workers` once answered, else the idle-based
## estimate the caller already struck (`fallback`). A worked row never takes this - it reads its own
## `workers`, and its Priority / Max controls re-send live.
func _previewed_crew(is_working: bool, view: Dictionary, fallback: int) -> int:
    if not is_working and String(view.get("state", "")) == ForecastQuery.STATE_READY:
        return int((view["answer"] as Dictionary).get("workers", fallback))
    return fallback

## The curve ask's crew term must cover the previewed crew: the band's idle pool alone can be 0 while
## the real staffing hands the row a crew taken from lower-priority rows. Both ask paths (build and
## drag) take THIS number, so they share one key.
func _preview_curve_pool(pool: int, is_working: bool, view: Dictionary) -> int:
    return maxi(pool, _previewed_crew(is_working, view, 0))

## **THE PREVIEW LINE** under the crew strip: where the crew would come from, amber when no one would.
## Nothing while the answer is in flight (the idle estimate stands), the failure line if it was refused.
func _mount_staffing_preview(target: VBoxContainer, is_working: bool, view: Dictionary,
        verb: String, crew_noun: String, priority: String) -> void:
    var state := String(view.get("state", ""))
    if is_working or state == "" or state == ForecastQuery.STATE_PENDING:
        return
    if state == ForecastQuery.STATE_FAILED:
        target.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.FORECAST_FAILED_FORMAT
            % String(view.get("error", ""))))
        return
    var reply: Dictionary = view.get("answer", {})
    var line := HudComposeVocab.staffing_preview_line(reply, verb, crew_noun, priority,
        _herd_label_for_id_fn)
    var label := HudWidgets.build_wrapping_status_part(String(line["text"]),
        HudStyle.WARN if bool(line["warn"]) else HudStyle.INK_DIM)
    label.mouse_filter = Control.MOUSE_FILTER_STOP
    label.tooltip_text = HudComposeVocab.staffing_preview_tooltip(reply, _herd_label_for_id_fn)
    target.add_child(label)

## The crew the FIRST pricing pass assumes, before the forecast has said what it is: the whole pool, held
## to the Max.
func _first_pass_crew(pool: int, max_workers: int) -> int:
    return mini(pool, max_workers) if max_workers > 0 else pool

## The hands that would reach (or hold) the floor if gear were no limit, held to the pool and the Max -
## what the hunt sheet's gear-short sentence measures the staffed crew against. `0` where the forecast
## states none.
func _wanted_crew(forecast: Dictionary, pool: int, max_workers: int) -> int:
    var reach := maxi(int(forecast.get("reach_crew", 0)), int(forecast.get("hold_crew", 0)))
    var wanted := mini(reach, pool)
    return mini(wanted, max_workers) if max_workers > 0 else wanted


## **THE PRIORITY SEGMENT AND THE MAX STEPPER** (#871) - the two controls that are the player's whole say
## over a staffed source's crew. Priority is `High` / `Normal` / `Low` (the work inspector's own picker);
## the Max is `-` / `+` with `No max` at 0, bounded above by the band's working-age people.
func _mount_priority_and_max(parent: VBoxContainer, priority: String, max_workers: int, upper: int,
        on_priority: Callable, on_max: Callable, is_dig_crew: bool = false) -> void:
    var priority_block := VBoxContainer.new()
    priority_block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    priority_block.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_LABEL_SEPARATION)
    # The hint line is gone from the sheet; its sentence is the label's and every level's hover instead.
    var priority_label := HudWidgets.alloc_section_label(HudWorkVocab.WORK_INSPECT_PRIORITY)
    HudWidgets.set_label_tooltip(priority_label, HudWorkVocab.WORK_PRIORITY_HINT)
    priority_block.add_child(priority_label)
    priority_block.add_child(HudWidgets.build_work_priority_picker(on_priority, priority, false))
    parent.add_child(priority_block)
    # **MAX IS ONE LINE** — the key on the left, the `−  N  +` stepper beside it, the `Kit` field row's shape.
    var max_row := HBoxContainer.new()
    max_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    max_row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    max_row.add_child(HudWidgets.build_field_key(
        HudWorkVocab.DIG_CREW_STEPPER_LABEL if is_dig_crew else HudWorkVocab.MAX_STEPPER_LABEL))
    HudWidgets.add_max_stepper(max_row, max_workers, upper, on_max, false,
        HudDepositVocab.DIG_CREW_MIN if is_dig_crew else 0)
    parent.add_child(max_row)

## **A SOURCE THE BAND WORKS IS EDITED LIVE** (#871): its sheet's commit is `Stop working here`, so the
## Max, the floor, the kit (and a patch's take selection) re-send the assignment the moment they change,
## carrying everything the sheet composes. These three are the commit buttons' own emits, lifted so the
## live edit and the first commit cannot send two different lines.
func _send_hunt_assignment(band: Dictionary, herd_id: String, herd_x: int, herd_y: int,
        needs_hunt: bool) -> void:
    _note_applied()
    _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT,
        HudComposeVocab.NEED_ROW_STANDING_HANDS if needs_hunt else _compose.hunt_max(),
        herd_x, herd_y, herd_id, _compose.hunt_floor(), "", _compose.hunt_improvement(),
        _compose.hunt_kit_id(), PackedStringArray(), needs_hunt)

func _send_forage_assignment(band: Dictionary, x: int, y: int) -> void:
    _note_applied()
    _emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, _compose.forage_max(), x, y, "",
        _compose.forage_floor(), _compose.forage_species(), _compose.forage_improvement(),
        _compose.forage_kit_id(), _compose.forage_take_species())

func _send_deposit_assignment(band: Dictionary, deposit: Dictionary, tile: Vector2i,
        material: String) -> void:
    _note_applied()
    var named_floor := _compose.deposit_floor() if HudDepositVocab.renews(deposit) \
        else SourceForecast.FLOOR_UNNAMED
    _emit_assign_labor(band, HudConst.LABOR_KIND_EXTRACT, _compose.deposit_max(), tile.x, tile.y, "",
        named_floor, material, SourceForecast.IMPROVEMENT_NONE, _compose.deposit_kit_id())

## **THE COMMIT ROW.** A source the band does not work yet keeps its one commit button. A worked source's
## sheet edits live, so its row is `Done` (primary, closes the sheet and sends nothing) beside
## `Stop working here`, with `Applied` above it for a beat after a live edit went out.
func _mount_commit_row(target: VBoxContainer, assign_btn: Button, is_unassign: bool,
        on_show_map: Callable = Callable()) -> void:
    if not is_unassign:
        target.add_child(assign_btn)
        return
    var applied_left_sec := float(_applied_until_msec - Time.get_ticks_msec()) \
        / HudComposeVocab.MSEC_PER_SECOND
    if applied_left_sec > 0.0:
        var applied := Label.new()
        applied.text = HudComposeVocab.APPLIED_TEXT
        applied.add_theme_color_override("font_color", HudStyle.HEALTHY)
        applied.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
        applied.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        applied.set_meta(HudComposeVocab.APPLIED_META, true)
        target.add_child(applied)
        var fade := applied.create_tween()
        fade.tween_interval(applied_left_sec)
        fade.tween_property(applied, "modulate:a", 0.0, HudComposeVocab.APPLIED_FADE_SECONDS)
    # **`Show on map` AT THE LEFT, THE ACTIONS AT THE RIGHT** (issue #877): the camera jump the Work tab's
    # retired inspector called `Jump to source`. It moves the camera and nothing else, so the sheet
    # stays open. `Stop working here` (`stop_labor`) drops a working's holding too, so a working's sheet
    # carries no second put-down.
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_SEPARATION)
    if on_show_map.is_valid():
        var show_map := HudWidgets.build_inline_link(HudComposeVocab.SHOW_ON_MAP_LABEL, HudStyle.INK,
            on_show_map)
        show_map.set_meta(HudComposeVocab.SHOW_ON_MAP_META, true)
        show_map.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        row.add_child(show_map)
        var gap := Control.new()
        gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
        row.add_child(gap)
    var done := Button.new()
    done.text = HudComposeVocab.DONE_BUTTON
    done.set_meta(HudComposeVocab.DONE_BUTTON_META, true)
    HudStyle.apply_button(done, "primary")
    done.pressed.connect(func() -> void: close_compose_sheet())
    HudStyle.apply_button(assign_btn, "ghost")
    row.add_child(done)
    row.add_child(assign_btn)
    target.add_child(row)

## When the last live edit's command went out plus the hold: every commit row built before then shows
## `Applied` (a live edit rebuilds the controls more than once). Zeroed when the sheet closes, so a first
## commit (which closes the sheet) leaves nothing behind.
var _applied_until_msec := 0

func _note_applied() -> void:
    _applied_until_msec = Time.get_ticks_msec() \
        + int(HudComposeVocab.APPLIED_HOLD_SECONDS * HudComposeVocab.MSEC_PER_SECOND)

## Relay a priority mark for a source the band already works (`Main.format_work_priority`). The sheet's
## priority segment is LIVE on a working source and rides the commit on a new one.
func _emit_work_priority(band: Dictionary, _kind: String, x: int, y: int, herd_id: String,
        material: String, level: String) -> void:
    _note_applied()
    work_priority_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "x": x, "y": y, "herd_id": herd_id, "material": material,
        "level": HudWorkVocab.work_priority_of(level),
    })

## Relay `Stop working here` (`Main.format_stop_labor`).
func _emit_stop_labor(band: Dictionary, kind: String, x: int, y: int, herd_id: String,
        material: String = "") -> void:
    stop_labor_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "kind": kind,
        "x": x, "y": y, "herd_id": herd_id, "material": material,
        # Not a command token: `Hud._on_stop_labor_requested` drops THIS source's pending assign by it.
        "pending_entity": int(band.get("entity", -1)),
    })

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
            String(deal_row.get(HudWidgets.IMPROVEMENT_DEAL_ROW_VALUE, "")),
            String(deal_row.get(HudWidgets.IMPROVEMENT_DEAL_ROW_NOTE, "")),
            String(deal_row.get(HudWidgets.IMPROVEMENT_DEAL_ROW_NOTE_TIP, ""))))
    if known:
        var verdict_host := VBoxContainer.new()
        verdict_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.add_child(verdict_host)
        # **ONLY THE HUNT WEB STATES A SENTENCE HERE** — the smallest of the take's three real limits
        # (`YIELD_MODEL_LIMIT`). The plant web has no fight, publishes no such key, and this host
        # stays empty: the floor chart above carries where the stock goes.
        #
        # **READ OFF THE SAME `yields_at` ANSWER THE ROWS ARE BUILT FROM**, at this floor and this
        # crew — the aside's locked-account line's rule, and for its reason: the models are PURE and
        # both calls pass identical arguments, so the sentence and the numbers it explains cannot be
        # composed at two different points on the dial.
        _register_live(hosts, verdict_host, model, workers,
            func(host: Container, live: Dictionary, crew: int) -> void:
                var limit: Dictionary = (yields_at.call(_live_floor(live), crew,
                    _live_reaches(live)) as Dictionary).get(YIELD_MODEL_LIMIT, {})
                if not limit.is_empty():
                    host.add_child(HudWidgets.build_verdict_line(limit)))
    var aside_host := VBoxContainer.new()
    aside_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    column.add_child(aside_host)
    _register_live(hosts, aside_host, model, workers,
        func(host: Container, live: Dictionary, crew: int) -> void:
            var lines: Array[Dictionary] = []
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

## **ARRIVING AT THE FLOOR AND ALREADY STANDING ON IT ARE THE SAME ANSWER TO THIS QUESTION.** The walk
## only reports `reached_turn` on a DESCENT (`project_stock` requires the stock to fall below where it
## started), so a source held at its floor answered `false` and its `after` reading was suppressed —
## on the one source whose take IS the holding rate. That suppression and the instantaneous headline
## were ONE defect: with the headline forward-looking the two readings simply coincide there and
## `yield_rows` drops the arrow itself, while a source BELOW its floor states the pair honestly
## (`0.00 → 0.96`, beside the verdict saying what it is waiting for).
##
## A crew that settles SHORT of the floor still answers `false`, which is the gate's own reason for
## existing: promising it a holding rate it never attains is the failure this reading exists to fix.
func _live_reaches(live: Dictionary) -> bool:
    if int(live.get("reached_turn", SourceForecast.PROJECTION_REACHED_NONE)) \
            != SourceForecast.PROJECTION_REACHED_NONE:
        return true
    return float(live.get("stock_fraction", 1.0)) \
        <= float(live.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)) \
            + SourceForecast.STOCK_FRACTION_EPSILON

## One yields model into the readout's first register. **The overdraw state moves the NUMBER, not just
## a suffix**: the row is the loudest thing in the box, so a take the source cannot pay forever has to
## read amber where the player is already looking.
##
## **NO `while_building` KEY REACHES THE CAPTION ANY MORE** (`docs/plan_standing_upkeep.md` §2.2).
## It said *these readings are the DIPPED take*; the build has its own crew, so these are the plain
## take whether or not a rung is going up. `build_yields_row` resolves the caption from the ROWS,
## which is the only thing left that can key it.
func _fill_yields_host(host: Container, model: Dictionary, labor_kind: String) -> void:
    if model.is_empty():
        return
    # **WHAT THIS SHEET MAY NOT STATE, STATED IN WORDS** — a narrowed take whose per-species rates the
    # wire never sent, and the material account, which is basket-averaged with no per-species twin
    # behind it. Printing the whole basket's numbers under a narrowed heading is the quote-vs-payout
    # defect wearing a heading; printing nothing at all teaches that the sheet has no opinion.
    var notes: Array = model.get(YIELD_MODEL_NOTES, [])
    var rows: Array = model.get(YIELD_MODEL_ROWS, [])
    if rows.is_empty():
        # No numbers to draw — the asides ARE the readout here (a narrowing to cash crops alone, or an
        # unquotable one). The chart, the crew targets and the verdict above are facts about the LAND
        # and are unaffected by which plants are ticked.
        for line in notes:
            host.add_child(HudWidgets.alloc_hint_label(String(line)))
        return
    # **NOTHING LEADS THE BLOCK — THE ROWS DO.** A take estimate used to be mounted here, above the
    # caption, stating the animals this crew brings down per turn; the binding-limit sentence under
    # the rows quoted the same rate, so the sheet said it twice with the four accounts in between.
    # The take, its band and its cadence are that sentence's now. What survives here is the caption's
    # suffix: the accounts are fixed conversions of ONE carried biomass quoted at the LIKELY point of
    # the band, and the caption is what keeps them honest beside a sentence that carries a range.
    var at_likely := bool(model.get(YIELD_MODEL_AT_LIKELY, false))
    var overdraws := bool(model[YIELD_MODEL_OVERDRAW])
    var renews := bool(model.get(YIELD_MODEL_RENEWS, true))
    var home_rate := bool(model.get(YIELD_MODEL_HOME_RATE, false))
    var note := ""
    if overdraws:
        note = HudComposeVocab.OVERHUNT_FLAG + " " + String(
            HudComposeVocab.LOCAL_OVERDRAW_NOTES.get(labor_kind, ""))
    elif renews:
        note = SourceForecast.YIELD_RENEWABLE_NOTE
    host.add_child(HudWidgets.build_yields_row(
        model[YIELD_MODEL_ROWS],
        HudStyle.WARN if overdraws else HudStyle.INK,
        note,
        HudStyle.WARN if overdraws else HudStyle.HEALTHY,
        # **PAST THE APRON THE CAPTION NAMES THE RATE HOME**, and takes no likely-take suffix: that
        # figure is the caravan forecast's mean, not a point of the curve's band.
        HudComposeVocab.YIELD_HEADER_ONCE_RUNNING if home_rate else "",
        HudComposeVocab.YIELD_HEADER_AT_LIKELY_SUFFIX if at_likely and not home_rate else ""))
    var rot := String(model.get(YIELD_MODEL_ROT, ""))
    if rot != "":
        var rot_row := HudWidgets.build_verdict_line({"severity": SourceForecast.VERDICT_SLOW,
            "text": rot})
        rot_row.set_meta(HudWidgets.WORK_PARTY_ROT_META, rot)
        host.add_child(rot_row)


## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary, target: VBoxContainer) -> void:
    _build_herd_assign_controls_core(herd, target)
    if target == null or herd.is_empty():
        return
    var herd_id := String(herd.get("id", ""))
    var band := _band_labor.player_band_by_entity(_compose.hunt_band())
    _apply_chief_row_strip(target, band, SourceForecast.LABOR_KIND_HUNT, int(herd.get("x", -1)),
        int(herd.get("y", -1)), herd_id, {"herd_id": herd_id},
        func() -> void: _build_herd_assign_controls(_live_herd(herd_id, herd), target))

func _build_herd_assign_controls_core(herd: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    # A rebuild frees the chart, so whatever drag was on it is over — see `_end_floor_drag`.
    _end_floor_drag()
    for child in target.get_children():
        child.queue_free()
    if not _herd_compose_available(herd):
        return
    var herd_id := String(herd.get("id", ""))
    # The band the sheet DEFAULTS to: whoever already hunts this herd, else the shared ladder's answer.
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.has_hunt_row(candidate, herd_id))
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
        # **THE MAX SEEDS FROM THE BAND'S ROW** (#871): the crew is the sim's, so the one number this
        # sheet composes is the player's Max (`0` = no Max), and a new source opens on no Max.
        _compose.seed_hunt(_band_labor.effective_hunt_max(band, herd_id),
            _band_labor.floor_for_hunt(band, herd_id), standing_improvement,
            # MIGRATION MODE seeds from the band's own row (pending-aware, so a toggle pressed on the
            # Work tab is what a reopened sheet shows), like the crew and the floor.
            bool((_band_labor.effective_worker_map(band).get(_band_labor.pending_key(
                SourceForecast.LABOR_KIND_HUNT, -1, -1, herd_id), {}) as Dictionary).get(
                    "move_with_herd", false)))
        # **AND THE KIT SEEDS FROM THE ROW** — see `_standing_kit_id`.
        _compose.set_hunt_kit_id(_standing_kit_id(band,
            _band_labor.pending_key(SourceForecast.LABOR_KIND_HUNT, -1, -1, herd_id)))
        _compose.set_hunt_priority(HudWorkVocab.work_priority_of(
            _band_labor.hunt_assignment_of(band, herd_id).get("priority", "")))
    # The band's confirmed row on this herd - `{}` for a source it does not work yet. A WORKING source
    # states the crew the sim staffed; a new one states the client's forecast of it.
    var working_row := _band_labor.hunt_assignment_of(band, herd_id)
    var is_working := _band_labor.has_hunt_row(band, herd_id)
    # The effective (pending-aware) standing crew on this herd.
    var current := _band_labor.effective_hunt_workers(band, herd_id)
    # Which band supplies the hunters (above the worker stepper, so it reads "which band → how many
    # workers"). Switching bands re-measures the apron below for that band.
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _compose.set_hunt_band(int(picked.get("entity", -1)))
        _build_herd_assign_controls(_live_herd(herd_id, herd), target)))
    # ⛔ **EVERY HERD IS AN ORDINARY HUNT, WHATEVER THE DISTANCE** (`docs/plan_civilization_steps.md`
    # §One work party). The sim posts a party past the apron on its own, so the sheet composes one
    # `assign_labor` for every herd and only ADDS what distance costs.
    #
    # The one threshold a caravan has is the PICKED band's `band_work_range`, measured wrap-aware from
    # that band's own tile — the band picker above drives every part of it.
    var herd_x := int(herd.get("x", -1))
    var herd_y := int(herd.get("y", -1))
    var past_apron := _is_past_apron(band, herd_x, herd_y)
    # **THE HANDS THIS SHEET MAY SPEND** — idle plus the crew this band already has on this herd
    # (`HudBandLaborState.source_crew_pool_hunt`), which is the ceiling `assign_labor` is judged
    # against. Without the standing term a fully-allocated band capped at `0` and the player could
    # take a crew to nothing and never put it back.
    #
    # The sheet composed two crews and clamped them against one pool while a verb carried a head
    # count; the build is a band-level role now, so this is the take's ceiling and nothing else.
    var assignable := _band_labor.source_crew_pool_hunt(band, herd_id)
    # The crew the FIRST pass prices the herd at (the carry tier is per equipped worker): the standing
    # crew where the band works it, else the most the Max and the pool allow. The staffed crew, once
    # struck from that pass's forecast, replaces it for every later reading.
    _hunt_priced_crew = current if is_working else _first_pass_crew(assignable, _compose.hunt_max())
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
    # **THE MAX IS CLAMPED TO THE BAND'S WORKING-AGE PEOPLE** (#871) - before the preview, which asks at it.
    _compose.clamp_hunt_max(maxi(int(band.get("working_age", 0)), 0))
    var needs_hunt := _herd_is_migratory(herd) and _compose.hunt_move_with_herd()
    # **A SOURCE THE BAND DOES NOT WORK YET IS PRICED OFF THE SIM'S DRY RUN, not the idle pool**: the
    # command restaffs the whole band by Priority, taking hands from lower rows. A follow hunt musters
    # the whole band by its own rule, which the preview does not model - it asks nothing there.
    var preview_view: Dictionary = {}
    if not is_working and not needs_hunt:
        preview_view = _staffing_preview_view(band,
            {"source_kind": ForecastQuery.WORK_PARTY_SOURCE_HUNT, "herd_id": herd_id},
            "hunt:" + herd_id, kit_id, _compose.hunt_floor(), _compose.hunt_priority(),
            _compose.hunt_max())
    # The curve is asked wide enough to cover the previewed crew, on the build AND the drag path.
    var curve_pool := _preview_curve_pool(assignable, is_working, preview_view)
    # **THE SHEET'S OWN QUESTION — what each crew size actually BRINGS DOWN.** The sheet
    # lets the player move the stepper before committing, so the take has to be re-answered as they
    # move it, and the fight is not the client's to compute: composed here from the engagement and the
    # retreat alone it read 1.92 food against a Wild Aurochs' 0.84 at four hunters, with every yield
    # beside it over by the same 2.3×.
    #
    # **ONE ASK COVERS THE WHOLE STEPPER**, which is why the reply is a CURVE: it is keyed on the
    # band's pool rather than on the composed crew, so a `+` press reads a row out of an answer the
    # seam already holds instead of putting a fresh question on the socket.
    #
    # **THE FLOOR IS THE OTHER HALF OF THAT KEY, AND IT IS NOT COVERED BY ONE ASK.** The curve is
    # floor-dependent — every row is bounded by the room standing above the escapement floor — so this
    # ask answers for the COMMITTED floor and no other. A drag therefore re-asks as it moves
    # (`_drag_crew_take`, rate-limited); what is composed here is the sheet's opening state and the
    # state it returns to on release.
    var crew_take_view := _crew_take_view(band, herd_id, kit_id, _compose.hunt_floor(), curve_pool)
    var crew_take: Array = (crew_take_view["answer"] as Dictionary).get("per_crew", [])
    # **THE LIVE PAIR STARTS WHERE THE REBUILD LEFT IT.** A rebuild ends whatever drag preceded it, so
    # the committed floor's answer is the live answer until a drag replaces it; every live host reads
    # these two rather than the locals above, which is what lets one refill serve both the build and
    # the drag.
    _hunt_live_crew_take = crew_take
    _hunt_live_crew_view = crew_take_view
    var composed_improvement := SourceForecast.build_verb(herd, HudComposeVocab.BARE_FORECAST_PREFIX,
            SourceForecast.SOURCE_KIND_HERD,
            _declared_or_composed(_band_labor.pending_improvement_for(
                band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id),
                _compose.hunt_improvement()))
    if composed_improvement != _compose.hunt_improvement():
        _compose.set_hunt_improvement(composed_improvement)
    # **THE CURVE RIDES THE FORECAST, so the stepper's cap is a search of the same rows the take line
    # reads.** `max_useful_workers` was `take_workers` — reach the peak drop, carry it home — with no
    # fight in it, which is what printed `13 of 37 useful` on a herd whose 14th hunter was still
    # buying take.
    var forecast := _hunt_forecast(herd, band, _compose.hunt_floor(), crew_take)
    # **THE REPLY'S ARMED COUNT RIDES ALONG SO THE GEAR-SHORT LINE CAN NAME THE SHORTAGE.** It changes no
    # cap - the curve has already resolved that. Absent (pending, failed) it reads as the unarmed pair.
    var crew_take_answer: Dictionary = crew_take_view["answer"]
    var cap := _forecast_worker_cap(forecast, assignable)
    # **THE MAX IS CLAMPED TO THE BAND'S WORKING-AGE PEOPLE** (#871) - a Max past the band is meaningless.
    _compose.clamp_hunt_max(maxi(int(band.get("working_age", 0)), 0))
    # **THE CREW THE SIM WILL STAFF** (#871). Working: the snapshot row's own `workers`. New: the
    # client's forecast - the crew that reaches the floor (`cap`, which the sim's crew-take curve has
    # already bounded by the hands that can bring the animal down), held to the Max.
    var staffed_crew := _previewed_crew(is_working, preview_view, _staffed_crew_of(
        is_working and not needs_hunt, current, cap, _compose.hunt_max()))
    _hunt_priced_crew = staffed_crew
    # A managed herd's local crew are HERDERS/keepers (workersNeeded scales with the herd), not a hunt
    # party — so a pen needing several keepers doesn't read as a hunt-party bug (fix #6).
    var crew_label := HudComposeVocab.HERD_CREW_LABEL \
        if SourceForecast.is_managed_hunt_source(herd, composed_improvement) \
        else HudComposeVocab.HUNT_CREW_LABEL
    # Per-PRESET takes under the picker, so the forage and hunt sheets wear the same "up to X/turn"
    # button metric, DESCENDING as the floor rises. Worker-independent.
    var floor_takes := _hunt_floor_takes(herd, band, composed_improvement)
    # **THE FLOOR FIRST, THEN THE CREW — the SAME vertical grammar the forage sheet reads in.** You
    # choose how hard to pull, then staff it. The cap is recomputed from the composed floor before the
    # stepper renders (a preset click re-renders and may auto-fill the crew) and the forecast below
    # reads the current crew.
    var on_floor_picked := func(floor: float) -> void:
        _compose.set_hunt_floor(floor)
        if is_working:
            _send_hunt_assignment(band, herd_id, herd_x, herd_y, needs_hunt)
        _build_herd_assign_controls(_live_herd(herd_id, herd), target)
    target.add_child(HudWidgets.build_floor_picker(
        on_floor_picked, _compose.hunt_floor(), floor_takes))
    # **THE CHART — the stock and the floor in ONE picture, and the floor is the panel's primary
    # control** (`docs/plan_harvest_floor.md` §7.3). It is the same setter the presets use, so a
    # dragged value and a clicked preset are one state — which is what lets the picker honestly show
    # NO preset selected between two of them. It replaced the plain slider slice 4a shipped as a
    # placeholder.
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
        HudComposeVocab.BARE_FORECAST_PREFIX, _compose.hunt_floor(), staffed_crew, lesson_known,
        crew_take, SourceForecast.WHOLE_CREW_TAKES, _player_knowledge())
    _last_chart_model = chart_model
    if bool(chart_model.get("known", false)):
        # **THE DRAG'S REFILL, PUBLISHED RATHER THAN INLINE.** A forecast answer landing mid-drag has
        # to reach these hosts too — it is the whole point of asking during the drag — and the seam's
        # `answered` arrives at `refresh_compose_sheet`, which is a REBUILD and would free the chart
        # under the pointer. So the refill is handed to the member the rebuild path defers to while a
        # drag is live, and the chart's own live branch calls the same one.
        _floor_drag_refill = func(floor: float) -> void:
            # **THE CURVE IS RE-ASKED AS THE FLOOR MOVES** — rate-limited, and reading only an answer
            # asked at THIS floor. Both halves are the fix: without the ask the rows below are the
            # floor the sheet opened at, and without the exact read a superseded answer would stand in
            # for them under the seam's stale window for as long as the drag kept renewing it.
            _drag_crew_take(band, herd_id, kit_id, floor, curve_pool)
            _refresh_floor_live(live_hosts, SourceForecast.floor_chart_model(
                _hunt_priced_herd(_live_herd(herd_id, herd), band),
                SourceForecast.SOURCE_KIND_HERD,
                HudComposeVocab.BARE_FORECAST_PREFIX, floor, staffed_crew, lesson_known,
                _hunt_live_crew_take, SourceForecast.WHOLE_CREW_TAKES, _player_knowledge()),
                staffed_crew)
        target.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _compose.set_hunt_floor(floor)
                if committed:
                    # The release ends the gesture BEFORE the rebuild, so nothing downstream of it can
                    # still take the drag path against nodes this rebuild is about to free.
                    _floor_drag_live = false
                    if is_working:
                        _send_hunt_assignment(band, herd_id, herd_x, herd_y, needs_hunt)
                    _build_herd_assign_controls(_live_herd(herd_id, herd), target)
                else:
                    # A LIVE drag must not rebuild these controls — the rebuild frees the chart
                    # and the drag dies with it. Refill only the readings that follow the floor.
                    _floor_drag_live = true
                    _floor_drag_refill.call(floor)))
    # **HUNTING BY NEED — the box, ABOVE the Hunters section** (`docs/plan_roaming_bands.md` §Hunting by
    # need). Ticked, the row is a need row that holds no standing crew, so the stepper, its split, the
    # cap note, the work party and the per-turn readout below are replaced by one text block. Toggling
    # it REBUILDS the sheet. Present on a MIGRATORY herd's sheet only.
    if _herd_is_migratory(herd):
        _mount_move_camp_box(target, func() -> void:
            _build_herd_assign_controls(_live_herd(herd_id, herd), target))
    # **THE CREW THE SIM WILL STAFF, THEN THE PLAYER'S PRIORITY AND MAX** (#871). The crew is read-only
    # (a number and the keepers / gear / bare bar); the two controls under it are the player's whole say
    # over it. A ticked migration box replaces all of it with the need block.
    var gear_short_line := ""
    if needs_hunt:
        # N is the MEAL-sized muster (`SourceForecast.hunt_need_crew`): the hands the sim sends to bring
        # the band's `hunt_meals` figure for this herd's keeping class, capped at the useful crew and
        # falling back to it when the band publishes no meal. NOT the idle-clamped cap: the muster draws
        # on the band's other work too, not just its idle hands.
        var useful_crew := SourceForecast.hunt_need_crew(Array(band.get("hunt_meals", [])),
            String(herd.get("keeping", "")), float(herd.get("food_per_animal", 0.0)), crew_take,
            SourceForecast.max_useful_workers(forecast))
        _mount_work_strip(target, band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id, "",
            crew_label, "", "", 0, herd)
        _mount_need_block(target, cap if useful_crew == SourceForecast.MAX_USEFUL_UNBOUNDED \
            else useful_crew, crew_label)
    else:
        var hunt_armed := int(crew_take_answer.get(SourceForecast.CREW_TAKE_ARMED_KEY,
            SourceForecast.CREW_TAKE_NO_ARMED_CREW))
        var hunt_weapon := String(crew_take_answer.get(SourceForecast.CREW_TAKE_WEAPON_KEY, ""))
        # **GEAR SHORT, AND BARE HANDS CANNOT WOUND IT** (#871): the curve's plateau IS the armed count,
        # so hands that would reach the floor but hold no weapon stay home. Only a NEW source says it -
        # a working one states the crew the sim already struck.
        if not is_working:
            # The gear-free reach is the CLOSED-FORM forecast's (no crew-take curve): the curve is priced
            # with the band's gear, so its plateau cannot say how many more hands the herd could use.
            var wanted := _wanted_crew(_hunt_forecast(herd, band, _compose.hunt_floor()), assignable,
                _compose.hunt_max())
            if SourceForecast.quarry_is_fought(herd, HudComposeVocab.BARE_FORECAST_PREFIX) \
                    and hunt_armed > SourceForecast.CREW_TAKE_NO_ARMED_CREW \
                    and hunt_weapon != "" and staffed_crew <= hunt_armed and wanted > staffed_crew:
                gear_short_line = HudComposeVocab.HUNT_GEAR_SHORT_FORMAT % [staffed_crew, hunt_weapon,
                    String(herd.get("species", HudComposeVocab.HUNT_GEAR_SHORT_FALLBACK_NOUN)),
                    wanted - staffed_crew]
        _mount_work_strip(target, band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id, "",
            crew_label, "", gear_short_line, staffed_crew, herd)
        _mount_priority_and_max(target, _compose.hunt_priority(), _compose.hunt_max(),
            int(band.get("working_age", 0)),
            func(level: String) -> void:
                _compose.set_hunt_priority(level)
                if is_working:
                    _emit_work_priority(band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y,
                        herd_id, "", level)
                _build_herd_assign_controls(_live_herd(herd_id, herd), target),
            func(n: int) -> void:
                _compose.set_hunt_max(n)
                if is_working:
                    _send_hunt_assignment(band, herd_id, herd_x, herd_y, needs_hunt)
                _build_herd_assign_controls(_live_herd(herd_id, herd), target))
        _mount_staffing_preview(target, is_working, preview_view,
            HudComposeVocab.STAFFING_PREVIEW_VERB_HUNT, crew_label, _compose.hunt_priority())
    # **NO STALE ANSWER STANDS ON THIS PATH EITHER** — `_crew_take_view` reads `view_exact`, and the
    # note that used to stand here said the opposite. The stale window's bargain is about a STEPPER
    # tick, and a stepper tick moves nothing this question is keyed on (the key carries the band's
    # POOL, not the composed crew), so the indulgence bought the readout nothing and cost it the
    # release of a floor drag: the rebuild asked the new floor and read back the old floor's curve as
    # READY. See `_crew_take_view` for the whole of it, and `ForecastQuery.view_exact` for the rule.
    # **THE KIT ROW, directly under the crew stepper and above every forecast** — a kit describes the
    # crew, and it moves the fight (the attack tier) and the haul (the carry tier) alike; the commit
    # sends it as `assign_labor … kit <id>`.
    _mount_kit_row(target, kits, KitRoster.JOB_HUNT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_hunt_kit_id(picked)
            if is_working:
                _send_hunt_assignment(band, herd_id, herd_x, herd_y, needs_hunt)
            _build_herd_assign_controls(_live_herd(herd_id, herd), target),
        herd, HudComposeVocab.BARE_FORECAST_PREFIX, staffed_crew,
        _band_labor.hunt_assignment_of(band, herd_id))
    # **THE FIGHT, STATED BEFORE THE PARTY LEAVES** (`docs/plan_hunt_through_combat.md` §2.1 / §6.5),
    # directly under the crew that will fight it — both lines answer "is this crew the right size, and
    # can it win at all", which is what the stepper one row up has just posed.
    #
    # **THE FIGHT IS THE GATE ON BOTH** (`SourceForecast.quarry_is_fought`, the one predicate the kit
    # picker's greying and the sheet's own numbers already take), so the whole PLANT web renders
    # neither line and no forage sheet moves. **The reach carries no build term**
    # (`docs/plan_standing_upkeep.md` §2.2): a Tame in flight is staffed by its own crew, so the
    # hunters beside it stalk exactly as they would with no build running.
    #
    # ⛔ **A PEN USED TO BE ON THE PLANT WEB'S SIDE OF THAT GATE, AND IT NO LONGER IS.** This read
    # `has_engagement_stage`, on the stated ground that *"a PEN and the whole PLANT web are
    # byte-identical: both publish `NO_ENGAGEMENT_STAGE` — a penned animal is not stalked and a berry
    # does not fight back"*. The first half died with §4.9 item 12b: a penned herd resolves the
    # ORDINARY fight, so a bare-handed party at a fenced aurochs (`defense 6`) is refused outright and
    # this sheet rendered no refusal at all — a stepper, a kit row and a forecast over a take the sim
    # pays nothing for. **Containment solves the catching, weapons solve the killing.** The byte
    # identity with forage survives, because that was always the claim worth keeping.
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
    if SourceForecast.is_fought(engage_rate,
            RungQuotes.is_enclosed(herd)):
        var quarry := _herd_label_for_id(herd_id)
        # **IT IS ASKED AT THE SELECTED KIT'S EFFECTIVE ATTACK, NOT THE BAND'S DEFAULT-KIT TIER.** The
        # picker one row up decides what these hunters carry, so a gate quoting the band's default kit
        # would refuse — or clear — a fight the composed party is not having.
        # **AND AT ITS ATTACK AGAINST *THIS ANIMAL*, NOT THE KIT'S BEST CASE.** A weapon bounded to a
        # size window grants nothing above it, so a snare reads the bare hand's attack against a Red
        # Deer — and the unbounded reading is what let a trapping sheet clear a gate the sim shuts.
        # **THE REMEDY IS CHOSEN ON THE KIT, THE REFUSAL ON THE BAND'S RESOLVED ATTACK.** The two are
        # different questions and the second cannot answer the first: a Stalking kit with an empty
        # store resolves to the bare hand, so a remedy read off that reading would tell this player to
        # change kit — to the kit they already have.
        var selected_kit := KitRoster.kit_by_id(kits, kit_id)
        var gate := SourceForecast.hunt_gate_model_at(KitRoster.effective_attack_against(
            kits, selected_kit, band,
            float(herd.get(KitRoster.QUARRY_BODY_MASS_KEY, 0.0))), herd, quarry,
            KitRoster.kit_arms_the_party(kits, selected_kit))
        if bool(gate["blocked"]):
            var gate_label := HudWidgets.forecast_label("[color=#%s]%s[/color]" % [
                HudStyle.DANGER_HEX, String(gate["text"])])
            gate_label.set_meta(HudWidgets.HUNT_GATE_META, true)
            target.add_child(gate_label)
        # ⛔ **A CLEARED GATE NOW RENDERS NOTHING, and the `else` that stood here is RETIRED.** It mounted
        # `HudWidgets.mount_hunt_crew_split` — *"⚠ 1 of your 3 hunters can take Wild Horses; the other 2
        # hold too little gear and land nothing on it at any headcount."* — the gate's complement for a
        # partly-equipped band (issue #520). Reported from play, on a sheet showing it directly beneath
        # the kit line: *"The yellow message mentions the hunters. I'm not sure we need that message at
        # all, we have the stalking kit message, it seems the second is redundant, you can remove it."*
        #
        # **THE KIT LINE UNDER THE PICKER IS WHAT SURVIVES** (`KitRoster.shortfall_line`) — one sentence
        # about the gear, on the control that chose it. **The REFUSAL above is untouched**: a fight this
        # party cannot make at all must still say so, and that is the branch, not this one.
    # **THE WORK PARTY, PAST THE APRON** — what distance costs this crew, priced by the sim's own
    # caravan forecast at the crew, kit and floor composed above. Directly under the crew and its kit
    # because every figure in it is a property of that crew walking that distance.
    var party_view := {}
    if past_apron and not needs_hunt:
        party_view = _work_party_view(band, ForecastQuery.WORK_PARTY_SOURCE_HUNT, herd_id,
            herd_x, herd_y, [], kit_id, staffed_crew, _compose.hunt_floor())
        _mount_work_party_section(target, party_view)
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
    # **A NEED ROW COMMITS WITH 0 HANDS AND IS NEITHER OF THOSE**: `hunt 0 … follow` states the
    # standing order, and the sim keeps a zero-crew row that carries the flag.
    # **A WORKING SOURCE OFFERS `Stop working here`, A NEW ONE ITS VERB** (#871): the count slot is a Max
    # now, so zero is "no Max" and never an unassign - the only way a staffed row ends is `stop_labor`.
    var is_unassign := not needs_hunt and is_working
    var assign_btn := Button.new()
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
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
    # THE ONE RESOLUTION OF THIS SHEET'S DEAL, spent by the readout below. An unassign quotes
    # none: the control above is not built either, so there would be no rung on the card for the
    # rows to be about.
    var deal_rung := "" if needs_hunt else _improvement_deal_rung(
        SourceForecast.LABOR_KIND_HUNT, herd, HudComposeVocab.BARE_FORECAST_PREFIX,
        composed_improvement)
    # ⛔ **THE DEAL IS THE CREW CURVE'S, AT THE SHEET'S CREW** — the hunt twin of the forage sheet's
    # `ONCE SOWN`. `pastoral_yield` / `corral_yield` are crew-blind (what the finished rung pays a crew
    # big enough to take all of it); the curve row's `next_rung_animals_likely` is THIS crew's take on
    # the finished rung, its keeping netted. Pending, refused, or no row for the crew → no deal row.
    var deal_payoff := "" if deal_rung == "" else _hunt_curve_payoff_terms(herd,
        _hunt_live_crew_view, staffed_crew)
    var deal_keeping := {}
    if deal_payoff != "":
        var hunt_curve: Array = (_hunt_live_crew_view.get("answer", {}) as Dictionary).get(
            "per_crew", [])
        deal_keeping = _next_rung_keeping_note(hunt_curve,
            SourceForecast.hunt_crew_take_row(hunt_curve, staffed_crew), staffed_crew,
            SourceForecast.HUNT_CREW_NEXT_RUNG_ANIMALS_KEY)
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
    # **NO ANSWER YET, OR NONE COMING** — the query seam's rule, and for this reason: the numbers are the sim's, so the sheet says it is waiting rather than
    # composing a take out of the two stages it can see. Everything above this line is client
    # arithmetic over wire terms (the chart, the crew targets, the combat gate) and stands.
    # **AND IT IS IN THE LIVE SET, because the answer it is waiting on is the FLOOR's.** It used
    # to be resolved once, at build time, from the committed floor's view — which was sound while
    # a drag asked nothing and is a lie the moment one does: the sheet would drop its numbers for
    # the dragged floor (correctly) and carry no sentence saying why. The registry's own rule
    # decides it, as it decided the yields row: anything whose PRESENCE depends on the floor
    # belongs in it.
    #
    # **HIDDEN RATHER THAN ABSENT** when the answer is in hand. A `BoxContainer` skips invisible
    # children entirely, separation included, so an answered sheet lays out to the pixel it did
    # before this host existed.
    if not needs_hunt:
        var take_state_host := VBoxContainer.new()
        take_state_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        target.add_child(take_state_host)
        _register_live(live_hosts, take_state_host, chart_model, staffed_crew,
            func(host: Container, _live: Dictionary, _crew: int) -> void:
                var state := String(_hunt_live_crew_view.get("state", ForecastQuery.STATE_PENDING))
                host.visible = state != ForecastQuery.STATE_READY
                if not host.visible:
                    return
                host.add_child(HudWidgets.alloc_hint_label(
                    HudComposeVocab.HUNT_TAKE_PENDING \
                    if state == ForecastQuery.STATE_PENDING \
                    else HudComposeVocab.FORECAST_FAILED_FORMAT \
                        % String(_hunt_live_crew_view.get("error", "")))))
        # **THE ROWS COME OFF THE LIVE PAIR, NOT OFF THE BUILDER'S LOCAL.** The model is asked at
        # `_live_floor(live)` on every refill, so binding the committed floor's curve into the closure
        # would compose one floor's rows against another floor's room for the whole of a drag — the
        # defect this arc closes, restated in the one place it would be invisible. The two agree
        # exactly on a sheet nobody is dragging, which is why the substitution is safe.
        _mount_readout(target, live_hosts, chart_model, staffed_crew,
            func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
                return _with_home_rate(_hunt_yield_model(band, herd, floor_value, crew,
                    composed_improvement, reaches, _hunt_live_crew_take), party_view,
                    SourceForecast.YIELD_ACCOUNT_FOOD, true),
            SourceForecast.LABOR_KIND_HUNT,
            _improvement_deal_row(SourceForecast.LABOR_KIND_HUNT, herd,
                HudComposeVocab.BARE_FORECAST_PREFIX, band, deal_rung, deal_payoff, deal_keeping))
    # **NO KEEPING ROW** (`docs/plan_standing_upkeep.md` §2.5) — a managed herd is held by the
    # band's `husbandry` role, not by a crew on this sheet, so there is no stepper here to point
    # at it. What this herd's share of that pool covers, and where it falls short, is stated on
    # the herd drawer's `Keeping:` / `At risk:` rows.
    # A dead button is always explained (the `+` stepper's cap note is the precedent) — but only
    # when the cap note has not already said it, so the panel never states one fact twice.
    # THE VERB FOLLOWS THE CREW NOUN, off the SAME `crew_label` the stepper and the noop hint
    # above already read. It was hard-coded to `Hunt Here`, so an `ASSIGN HERDERS` sheet over a
    # `Herders` stepper committed with `Hunt Here` — reported from play. A penned or fully-tamed
    # herd is not hunted.
    assign_btn.text = HudComposeVocab.STOP_WORKING_BUTTON if is_unassign \
        else String(HudComposeVocab.HUNT_ASSIGN_BUTTONS.get(crew_label,
            HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON))
    HudStyle.apply_button(assign_btn, "primary")
    # **ONE COMMAND, AND IT IS `assign_labor`** (`docs/plan_standing_upkeep.md` §4.7a ①). The
    # improvement verb that used to follow it is the Work tab's now. `composed_improvement` still
    # travels — it is recorded on the OPTIMISTIC OVERLAY and never on the wire, so a crew edit
    # does not blank a build the herd is already running.
    assign_btn.pressed.connect(func() -> void:
        if is_unassign:
            _emit_stop_labor(band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id)
            close_compose_sheet()
            return
        # A NEED ROW STATES 0 STANDING HANDS: the sim musters the crew each turn it is wanted. Otherwise
        # the count slot is the player's MAX (`0` = no Max).
        _send_hunt_assignment(band, herd_id, herd_x, herd_y, needs_hunt)
        # …then the priority, when the player ranked it: a new row is created at Normal.
        if not needs_hunt and _compose.hunt_priority() != HudWorkVocab.WORK_PRIORITY_NORMAL:
            _emit_work_priority(band, SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id, "",
                _compose.hunt_priority())
        close_compose_sheet())
    _mount_commit_row(target, assign_btn, is_unassign, func() -> void:
        _bandpanel.show_source_on_map(SourceForecast.LABOR_KIND_HUNT, herd_x, herd_y, herd_id))

## Whether the herd migrates — the only kind that accepts the `follow` token (the server refuses it on a
## herd that stays where it is). Read off the wire's `size_class` word.
func _herd_is_migratory(herd: Dictionary) -> bool:
    return String(herd.get("size_class", "")) == HudComposeVocab.SIZE_CLASS_MIGRATORY

## The migration-mode box and its one dim sub-line. Toggling it REBUILDS the sheet (`on_toggle`): ticked,
## the Hunters section is replaced by the need block (`_mount_need_block`) and the per-turn readout goes.
func _mount_move_camp_box(target: VBoxContainer, on_toggle: Callable) -> void:
    var box := CheckBox.new()
    box.text = HudComposeVocab.MOVE_CAMP_LABEL
    box.button_pressed = _compose.hunt_move_with_herd()
    box.focus_mode = Control.FOCUS_NONE
    box.set_meta(HudComposeVocab.MOVE_CAMP_BOX_META, true)
    HudStyle.apply_checkbox(box)
    box.toggled.connect(func(on: bool) -> void:
        _compose.set_hunt_move_with_herd(on)
        on_toggle.call())
    target.add_child(box)
    target.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.MOVE_CAMP_HINT))

## **THE HUNTERS SECTION OF A TICKED SHEET** — in place of the stepper: the section label, then
## `When the band needs meat` over one dim line saying how many hands the muster sends (`useful`, the
## meal-sized crew from `SourceForecast.hunt_need_crew`) and from where.
func _mount_need_block(target: VBoxContainer, useful: int, crew_label: String) -> void:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_LABEL_SEPARATION)
    block.set_meta(HudComposeVocab.NEED_BLOCK_META, true)
    block.add_child(HudWidgets.alloc_section_label(crew_label))
    var title := Label.new()
    title.text = HudComposeVocab.NEED_BLOCK_TITLE
    title.add_theme_color_override("font_color", HudStyle.INK)
    title.mouse_filter = Control.MOUSE_FILTER_IGNORE
    block.add_child(title)
    block.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.NEED_BLOCK_HINT_FORMAT % useful))
    target.add_child(block)

## Mount the kit row where a sheet wants it — a no-op when the roster offers this job no kit at all,
## so a sheet rendered before the first snapshot (or against a world whose roster does not cover the
## verb) is byte-identical to what it was before the picker existed. The Band panel's dock sheets keep
## the identical helper; the two controllers share no base, and one Callable to reach the other's copy
## would be an injection that buys nothing.
##
## `quarry` / `prefix` are what the greying is resolved against and are omitted by the forage sheet,
## which has no animal to be inapplicable to.
## **`crew` IS THE STEPPER ONE ROW ABOVE**, handed on so the hint can say how far the band's gear
## reaches into the party being composed. A sheet that passes none keeps the pre-clause line.
## **`assignment` IS THE COMMITTED SOURCE ROW this sheet is composing**, `{}` for a source this band
## does not work yet. It is the raw WIRE row, so it carries the `kit_workers_holding` of `workers`
## pair the sim cut from the band's one ledger — which is what stops two 4-hunter sheets on four traps
## each reading *covered* while the rows behind them each arm two. `KitRoster.shortfall_line` carries
## the argument.
func _mount_kit_row(target: VBoxContainer, kits: Array, job: String, kit_id: String,
        default_kit: String, band: Dictionary, on_pick: Callable, quarry: Dictionary = {},
        prefix: String = "", crew: int = KitRoster.KIT_CREW_UNCOMPOSED,
        assignment: Dictionary = {}) -> void:
    var row := KitRoster.build_kit_row(kits, job, kit_id, default_kit, band, on_pick, quarry, prefix,
        HudComposeVocab.COMPOSE_FIELD_KIT, false, crew, assignment)
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
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        var forecast := _forage_forecast(tile_info, band, SourceForecast.floor_for_preset(preset))
        if not bool(forecast["known"]):
            continue
        # **THE MATERIAL CEILING COMPOSES AT THIS PRESET'S OWN FLOOR**, by the same
        # `max(0, B − floor·K) × rate` rule the two scalars take — which is the whole reason
        # `material_per_biomass` is a per-biomass RATE rather than a pre-composed ceiling.
        takes[preset] = SourceForecast.extractive_take_pair(
            float(forecast["ceiling"]),
            float(forecast["ceiling_fodder"]), zero_account,
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
    return RungQuotes.entry_can_climb(entry, policy)

## What committing this entry under `policy` pays relative to gathering it wild. `FLORA_CROP_RATIO_NONE`
## on a rung the species cannot climb — the sentinel, never printed as a number.
func _flora_entry_ratio(entry: Dictionary, policy: String) -> float:
    return float(RungQuotes.entry_quote(entry, policy).get(RungQuotes.KEY_YIELD_RATIO,
        SourceForecast.FLORA_CROP_RATIO_NONE))

## The FODDER (hay) this entry would pay per turn under `policy` — >0 marks a crop whose vector feeds
## animals. `FLORA_CROP_RATIO_NONE` (0) where the vector pays no fodder or the plant cannot climb this
## rung.
##
## **PER RUNG, exactly like `_flora_entry_ratio` above** (#419). It read `sow_fodder_payoff`
## unconditionally — a *Field* payoff — so the Cultivate row stated what a sown field would pay, on a
## rung that pays a drawn-down MSY skim off a merely-weeded basket. The sim ships both rungs' figures.
func _flora_entry_fodder_payoff(entry: Dictionary, policy: String) -> float:
    return float(RungQuotes.entry_quote(entry, policy).get(RungQuotes.KEY_FODDER_PAYOFF,
        SourceForecast.FLORA_CROP_RATIO_NONE))

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
    return SourceForecast.material_payoff_rows(
        RungQuotes.entry_quote(entry, policy).get(RungQuotes.KEY_MATERIAL_PAYOFF, []))

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
    return float(RungQuotes.entry_quote(entry, policy).get(RungQuotes.KEY_FOOD_PAYOFF, 0.0))

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
##
## `curve_row`, where given, is the crew curve's row at the sheet's crew, and its `next_rung_take` /
## `next_rung_fodder` / `next_rung_materials` REPLACE every account (the sim's own figures at this
## crew, output already applied). The crop quote then only names a refused fodder account.
func _crop_payoff_terms(tile_info: Dictionary, entries: Array[Dictionary], species: String,
        band: Dictionary, rung: String, curve_row: Dictionary = {}) -> String:
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
    var food := payoff * output
    fodder *= output
    if not curve_row.is_empty():
        food = float(curve_row[SourceForecast.FORAGE_CREW_NEXT_RUNG_TAKE_KEY])
        fodder = float(curve_row.get(SourceForecast.FORAGE_CREW_NEXT_RUNG_FODDER_KEY, 0.0))
        materials = curve_row.get(SourceForecast.FORAGE_CREW_NEXT_RUNG_MATERIALS_KEY, [])
        output = SourceForecast.OUTPUT_FULL
    var terms := SourceForecast.picker_products(food, fodder,
        SourceForecast.YIELD_ACCOUNT_FOOD, [])
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
    if committed or policy == SourceForecast.IMPROVEMENT_NONE:
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
## **THE CROP IS STILL COMMITTED BY THIS SHEET'S OWN `assign_labor`**, as its `species` token, exactly
## as before — that is why moving the CONTROL stranded nothing. `_resolve_crop_selection` still runs
## every render, so a composition can never name a plant this tile and rung cannot take, and the
## readout's `ONCE TENDED` terms still follow the selected crop through `_crop_payoff_terms`.
##
## The `extra_rows` hook it rode retired with it: the improvement control mounts nothing beneath
## itself on any state now.

func _build_forage_assign_controls(tile_info: Dictionary, target: VBoxContainer) -> void:
    _build_forage_assign_controls_core(tile_info, target)
    if target == null or tile_info.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var band := _band_labor.player_band_by_entity(_compose.forage_band())
    _apply_chief_row_strip(target, band, SourceForecast.LABOR_KIND_FORAGE, x, y, "", {"x": x, "y": y},
        func() -> void: _build_forage_assign_controls(_live_tile_info(_forage_source_key(tile_info), tile_info), target))

func _build_forage_assign_controls_core(tile_info: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    # A rebuild frees the chart, so whatever drag was on it is over — see `_end_floor_drag`.
    _end_floor_drag()
    for child in target.get_children():
        child.queue_free()
    if not _forage_compose_available(tile_info):
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    # The band the sheet DEFAULTS to: whoever already forages this patch, else the shared ladder's answer.
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.has_forage_row(candidate, x, y))
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
        # **THE TAKE SELECTION SEEDS WITH THEM**, and it must: re-issuing `assign_labor` without a
        # `take:` token CLEARS it sim-side exactly as an omitted floor or species token does, so a
        # sheet that opened on an empty selection over a band already gathering only emmer would
        # silently widen the crew back to the whole basket the moment anything else was committed.
        _compose.seed_forage(_band_labor.effective_forage_max(band, x, y),
            _band_labor.floor_for_forage(band, x, y), standing_improvement,
            _band_labor.species_for_forage(band, x, y),
            _band_labor.take_species_for_forage(band, x, y))
        # **AND THE KIT SEEDS FROM THE ROW** — see `_standing_kit_id`.
        _compose.set_forage_kit_id(_standing_kit_id(band,
            _band_labor.pending_key(SourceForecast.LABOR_KIND_FORAGE, x, y, "")))
        _compose.set_forage_priority(HudWorkVocab.work_priority_of(
            _band_labor.forage_assignment_of(band, x, y).get("priority", "")))
    # **THE TAKE SELECTION, resolved ONCE for the whole sheet** — the chips render it, the take side of
    # the sheet is composed against it and the commit sends it. Resolved here, above every reading, for
    # the reason the cap is: a readout composed against one selection beside chips drawn from another
    # is the sheet arguing with itself.
    var take_selection := _compose.forage_take_species()
    # The effective (pending-aware) standing crew, which the commit's unassign/no-op test reads below.
    var current := _band_labor.effective_forage_workers(band, x, y)
    var working_row := _band_labor.forage_assignment_of(band, x, y)
    var is_working := _band_labor.has_forage_row(band, x, y)
    # **THE HANDS THIS SHEET MAY SPEND** — idle plus the crew this band already has on this patch
    # (`HudBandLaborState.source_crew_pool_forage`), the ceiling `assign_labor` is judged against.
    # **THE BUILD NO LONGER SHARES IT** (`docs/plan_standing_upkeep.md` §2.5): the sheet had two
    # steppers each capped at what the other left, and a verb states no hands now.
    var crew_pool := _band_labor.source_crew_pool_forage(band, x, y)
    # The crew the FIRST pass prices the patch at - see the hunt sheet's twin.
    _forage_priced_crew = current if is_working else _first_pass_crew(crew_pool, _compose.forage_max())
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
    # **THE SELECTION IS PRICED HERE, AND WHAT COMES OUT IS A PATCH.** `take_tile` is `tile_info` with
    # the ticked plants' stand, conversion rates and crew throughput substituted in — so everything
    # below that answers about the TAKE reads it and nothing below has to know a narrowing happened:
    # the worker cap, the floor presets, the chart's two crew targets and the readout's `now → after`
    # walk all move when a chip is ticked, exactly as they move when the stepper is.
    #
    # **THE RAW `tile_info` SURVIVES FOR EVERYTHING THAT IS NOT A TAKE** — the basket above, the commit
    # crop, the improvement control and its deal row — because those are facts about the ground rather
    # than about what this crew carries home. Reading the wrong one of the two is the failure this
    # comment exists to prevent, so they are named apart.
    var take_state := _selective_take_state(basket, take_selection)
    var take_tile := _forage_take_source(tile_info, take_state)
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
    # **OFF THE GATHERING SITES THE CROP IS THE COMMIT**: the server takes a forage crew there only with a
    # crop named, so the chips are single-pick against the placing rung and the button carries it.
    var offsite_verb := "" if is_committed else DetailFormat.offsite_crop_verb(tile_info)
    var crop_rung := composed_improvement if composed_improvement != SourceForecast.IMPROVEMENT_NONE \
        else offsite_verb if offsite_verb != "" else String(RungGates.next_rung_offered(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            composed_improvement, _player_knowledge(),
            HudComposeVocab.FORAGE_FORECAST_PREFIX).get("policy", ""))
    _compose.resolve_forage_species(func(current: String) -> String:
        return _resolve_crop_selection(basket, crop_rung, is_committed, current))
    # **WHETHER THE CROP IS THE PLAYER'S OR THE GAME'S IS THE MODEL'S TO REMEMBER, not a comparison
    # made here.** The resolver writes its answer back, so from the second render on the pick and the
    # fall-back are the same string and a before-and-after comparison reads every settled crop as a
    # chosen one. The single-pick chip row needs the distinction: silence on a Cultivate is the game
    # choosing the crop without saying so, and the consequence line has to name the plant it settled on.
    var crop_is_default := not _compose.forage_species_chosen()
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
    var forecast := _forage_forecast(take_tile, band, _compose.forage_floor())
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
    # **A KEPT PATCH ASKS THE SIM FOR ITS CURVE**, and caps on the curve's plateau — the most hands
    # that still help, keeping included. The kit is resolved here for the ask exactly as the kit row
    # below resolves it, so the two cannot name different kits.
    # **A PATCH THE BAND DOES NOT WORK YET IS PRICED OFF THE SIM'S DRY RUN** (the hunt sheet's rule): the
    # curve below is asked wide enough to cover the previewed crew, and the strip reads it.
    _compose.clamp_forage_max(maxi(int(band.get("working_age", 0)), 0))
    var preview_kit := KitRoster.resolve_selection(_band_labor.kits(), KitRoster.JOB_FORAGE,
        _band_labor.default_kit_id(KitRoster.JOB_FORAGE), _compose.forage_kit_id())
    var preview_view: Dictionary = {}
    if not is_working:
        preview_view = _staffing_preview_view(band, {
            "source_kind": ForecastQuery.WORK_PARTY_SOURCE_FORAGE, "x": x, "y": y,
            "take_species": Array(_compose.forage_take_species())},
            "forage:%d,%d" % [x, y], preview_kit, _compose.forage_floor(),
            _compose.forage_priority(), _compose.forage_max(),
            ":" + ",".join(PackedStringArray(_compose.forage_take_species())))
    var curve_pool := _preview_curve_pool(crew_pool, is_working, preview_view)
    var kept := _forage_is_kept(take_tile)
    # **THE CROP THE CURVE PRICES ITS `next_rung_*` ON** — the sheet's picked (or settled) crop on
    # uncommitted ground; committed ground names none, the sim ignoring it there.
    var ask_crop := "" if is_committed else _compose.forage_species()
    var cap := 0
    if kept:
        var ask_kit := KitRoster.resolve_selection(_band_labor.kits(), KitRoster.JOB_FORAGE,
            _band_labor.default_kit_id(KitRoster.JOB_FORAGE), _compose.forage_kit_id())
        _forage_live_crew_view = _forage_crew_take_view(band, take_tile,
            Array(_compose.forage_take_species()), ask_kit, _compose.forage_floor(), curve_pool,
            ask_crop)
        # **THE CAP IS THE CREW WHOSE GATHERERS REACH THE TAKE'S USEFUL HANDS** — the take's own
        # count, plus the keeping hands the sim says that crew spends first. While the reply is in
        # flight, or no row gets there, only the pool caps the stepper.
        var useful := SourceForecast.NO_CREW_ANSWER
        if String(_forage_live_crew_view.get("state", "")) == ForecastQuery.STATE_READY:
            useful = SourceForecast.forage_curve_useful((_forage_live_crew_view.get("answer", {})
                as Dictionary).get("per_crew", []), SourceForecast.max_useful_workers(forecast))
        cap = _curve_worker_cap(useful, crew_pool)
    else:
        _forage_live_crew_view = {}
        cap = _forecast_worker_cap(forecast, crew_pool)
    # **THE MAX IS CLAMPED TO THE BAND'S WORKING-AGE PEOPLE** (#871), and the crew is the sim's: the
    # snapshot row's `workers` on a patch the band works, else the forecast's reach crew held to the Max.
    _compose.clamp_forage_max(maxi(int(band.get("working_age", 0)), 0))
    var staffed_crew := _previewed_crew(is_working, preview_view,
        _staffed_crew_of(is_working, current, cap, _compose.forage_max()))
    _forage_priced_crew = staffed_crew
    var forage_takes := _forage_floor_takes(take_tile, band)
    var on_floor_picked := func(floor: float) -> void:
        _compose.set_forage_floor(floor)
        if is_working:
            _send_forage_assignment(band, x, y)
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
    # **THE CHART IS PRICED ON THE NARROWED PATCH TOO, and that is what keeps its two crew pills
    # reachable.** Both targets are clickable and both are clamped to the stepper's cap; with the cap
    # divided from the ticked plants' stand and the pills from the whole basket's, a target would name
    # a count the `+` refuses — the panel arguing with itself, which this file already records once.
    # Uniform scaling leaves the stock FRACTION `B/K` untouched, so the curve's shape, the floor's
    # position on it and the phase bands behind it are exactly the whole patch's; what shrinks is the
    # absolute biomass, which is the selected plants' stand and is the number the chips state.
    var chart_model := _kept_crew_targets(SourceForecast.floor_chart_model(
        _forage_priced_patch(take_tile, band),
        SourceForecast.SOURCE_KIND_FORAGE, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        _compose.forage_floor(), staffed_crew,
        lesson_known, [], _kept_take_crew(kept, staffed_crew),
        _player_knowledge()),
        kept)
    _last_chart_model = chart_model
    if bool(chart_model.get("known", false)):
        # The plant twin of the hunt sheet's published refill, and it exists for the narrower half of
        # the same reason: this web asks the query channel nothing, but the seam's `answered` fans out
        # to EVERY open sheet, so a raid reply landing while a forager drags this chart would rebuild
        # it and end the drag. A drag is never rebuilt on either web.
        _floor_drag_refill = func(floor: float) -> void:
            # **A KEPT PATCH'S CURVE IS FLOOR-DEPENDENT, SO THE DRAG RE-ASKS IT** — the deposit
            # sheet's rate-limited drag ask; the refill then reads what the seam holds for that floor.
            if kept:
                _drag_forage_crew_take(band, take_tile, Array(_compose.forage_take_species()),
                    _compose.forage_kit_id(), floor, curve_pool, ask_crop)
            _refresh_floor_live(live_hosts, _kept_crew_targets(SourceForecast.floor_chart_model(
                _forage_priced_patch(_forage_take_source(
                    _live_tile_info(subject_key, tile_info), take_state), band),
                SourceForecast.SOURCE_KIND_FORAGE,
                HudComposeVocab.FORAGE_FORECAST_PREFIX, floor, staffed_crew,
                lesson_known, [], _kept_take_crew(kept, staffed_crew),
                _player_knowledge()), kept),
                staffed_crew)
        target.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _compose.set_forage_floor(floor)
                if committed:
                    _floor_drag_live = false
                    if is_working:
                        _send_forage_assignment(band, x, y)
                    _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target)
                else:
                    _floor_drag_live = true
                    _floor_drag_refill.call(floor)))
    # **THE CREW THE SIM WILL STAFF, THEN THE PLAYER'S PRIORITY AND MAX** (#871) - read-only crew number
    # and bar, then the two controls that are the player's whole say over it.
    _mount_work_strip(target, band, SourceForecast.LABOR_KIND_FORAGE, x, y, "", "", crew_label, "", "",
        staffed_crew, tile_info, offsite_verb == "")
    _mount_priority_and_max(target, _compose.forage_priority(), _compose.forage_max(),
        int(band.get("working_age", 0)),
        func(level: String) -> void:
            _compose.set_forage_priority(level)
            if is_working:
                _emit_work_priority(band, SourceForecast.LABOR_KIND_FORAGE, x, y, "", "", level)
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
        func(n: int) -> void:
            _compose.set_forage_max(n)
            if is_working:
                _send_forage_assignment(band, x, y)
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target))
    _mount_staffing_preview(target, is_working, preview_view,
        HudComposeVocab.STAFFING_PREVIEW_VERB_FORAGE, crew_label, _compose.forage_priority())
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
    # ⛔ **THE CREW IS HANDED ON HERE, AND ITS ABSENCE MADE THE FORAGE SHEET MUTE.** This call omitted
    # it, so `crew` defaulted to `KIT_CREW_UNCOMPOSED` and `shortfall_line` fell back to the published
    # `workersOnQuotedJob` — which is `0` on a sheet where nobody is assigned yet — and returned `""`
    # on EVERY forage sheet, however short the band was. The hunt row one screen up had always passed
    # `_compose.hunt_max()`, so the two webs looked identical in the producer and diverged at the
    # mount. Reported from play as *"in the forage image you say nothing."*
    _mount_kit_row(target, forage_kits, KitRoster.JOB_FORAGE, forage_kit_id, forage_default_kit, band,
        func(picked: String) -> void:
            _compose.set_forage_kit_id(picked)
            if is_working:
                _send_forage_assignment(band, x, y)
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
        {}, "", staffed_crew, _band_labor.forage_assignment_of(band, x, y))
    # **THE SPECIES CHIPS — what this crew carries home**, standing where the retired crop picker stood
    # and doing both of that control's jobs: on a plain gather it narrows the TAKE (multi-select, the
    # selective gather); with a rung composed it is the COMMIT crop (single-select), which is the same
    # decision the crop list used to make, back on the sheet because it is now inseparable from what
    # the crew is picking up this turn.
    #
    # It mounts BELOW the kit row and ABOVE the improvement control, so the sheet reads
    # band → floor → crew → kit → what we carry home → what we are building → the terms.
    var single_pick := composed_improvement != SourceForecast.IMPROVEMENT_NONE or offsite_verb != ""
    _mount_take_chips(target, basket, take_selection, single_pick,
        crop_is_default, crop_rung,
        func() -> void:
            if is_working:
                _send_forage_assignment(band, x, y)
            _build_forage_assign_controls(_live_tile_info(subject_key, tile_info), target),
        committed_species if is_committed else "",
        HudFormat.progress_percent(SourceForecast.improvement_progress(tile_info,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, crop_rung)),
        DetailFormat.offsite_crop_label(tile_info) if offsite_verb != "" else "")
    # **THE WORK PARTY, PAST THE APRON** — the hunt sheet's section on the plant web, asked with the
    # take selection the chips above just stated (empty = the whole basket), so narrowing the take
    # re-prices what arrives home. It is AFTER the chips because the take is part of its question.
    var party_view := {}
    if _is_past_apron(band, x, y):
        party_view = _work_party_view(band, ForecastQuery.WORK_PARTY_SOURCE_FORAGE, "", x, y,
            take_selection, forage_kit_id, staffed_crew, _compose.forage_floor())
        _mount_work_party_section(target, party_view)
    # **A WORKING PATCH OFFERS `Stop working here`, A NEW ONE ITS VERB** (#871): the count slot is a Max,
    # so zero is "no Max" and never an unassign - a staffed row ends only by `stop_labor`.
    var is_unassign := is_working
    # **THE PAYOFF FOLLOWS THE SELECTED CROP, AND IT IS RESOLVED EXACTLY ONCE** (issue #419). The
    # readout's payoff row and the crop picker one control up must read ONE seam or they quote
    # different crops — which is the whole defect that issue named, in its second home. `deal_rung` is
    # the rung the picker is rendered for wherever there is one to render (composed verb, else the
    # ungated offer), so the terms and the list can only ever be about the same rung.
    var deal_rung := _improvement_deal_rung(
        SourceForecast.LABOR_KIND_FORAGE, tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX,
        composed_improvement)
    # ⛔ **ON A KEPT OR RAISING PATCH THE WHOLE DEAL LINE IS THE CREW CURVE'S, AT THE SHEET'S CREW.**
    # `field_yield` / `tended_yield` are crew-blind — what the finished rung pays a crew big enough to
    # take all of it — so a four-hand crew on Rushford's Harvest read `ONCE SOWN 12.48` where the same
    # four hands, keeping netted, take 0.85. The curve row's `next_rung_take` / `next_rung_fodder` /
    # `next_rung_materials` are that crew's figures, priced on the crop the ask names (the picked one
    # on uncommitted ground), so the line follows the pick. While the curve is in flight the row
    # states no figure. A crew of zero has no curve row, so it states no deal. A wild patch merely
    # OFFERED a rung, with nothing kept and nothing being raised, keeps the crop quote.
    var curve_deal := kept or is_committed \
        or composed_improvement != SourceForecast.IMPROVEMENT_NONE
    var deal_payoff := ""
    var deal_keeping := {}
    if deal_rung != "" and curve_deal:
        var deal_view := _forage_live_crew_view
        if not kept:
            deal_view = _forage_crew_take_view(band, take_tile,
                Array(_compose.forage_take_species()), forage_kit_id, _compose.forage_floor(),
                curve_pool, ask_crop)
        var curve_row := _curve_next_rung_row(deal_view, staffed_crew)
        if not curve_row.is_empty():
            deal_payoff = _crop_payoff_terms(tile_info, basket, _compose.forage_species(), band,
                deal_rung, curve_row)
            deal_keeping = _next_rung_keeping_note((deal_view.get("answer", {}) as Dictionary).get(
                "per_crew", []), curve_row, staffed_crew,
                SourceForecast.FORAGE_CREW_NEXT_RUNG_TAKE_KEY)
    elif deal_rung != "":
        deal_payoff = _crop_payoff_terms(tile_info, basket, _compose.forage_species(), band,
            deal_rung)
    # **THE READOUT** — the take, the rung's payoff, the verdict, and the idle-crew
    # note + teaching line, in one bounded box (§7.1, §7.2). The take is recomposed from the LIVE
    # floor, which is what lets the numbers the player is dragging toward move while the drag runs.
    _mount_readout(target, live_hosts, chart_model, staffed_crew,
        func(floor_value: float, crew: int, reaches: bool) -> Dictionary:
            return _with_home_rate(_forage_yield_model(band, take_tile, floor_value, crew,
                composed_improvement, reaches, take_state), party_view,
                SourceForecast.YIELD_ACCOUNT_FOOD, true),
        SourceForecast.LABOR_KIND_FORAGE,
        _improvement_deal_row(SourceForecast.LABOR_KIND_FORAGE, tile_info,
            HudComposeVocab.FORAGE_FORECAST_PREFIX, band, deal_rung, deal_payoff, deal_keeping))
    # ⛔ **NO RANGE REFUSAL ON THIS SHEET ANY MORE** (`docs/plan_civilization_steps.md` §One work
    # party). A patch past the SELECTED band's work range used to disable the commit behind the
    # retired `_mount_work_range_refusal`, because the sim lapsed a far forage crew with no warning
    # (#650). The work party removed that lapse — the crew posts a party and walks the take home — so
    # the refusal would now forbid the very assignment the caravan exists to make. The deposit sheets
    # lost theirs for the same reason: a far working posts the same caravan.
    # A dead button is always explained (the `+` stepper's cap note is the precedent) — but only when
    # the cap note has not already said it, so the panel never states one fact twice.
    # **NO KEEPING ROW** (`docs/plan_standing_upkeep.md` §2.5) — a tended patch or a Field is held by
    # the band's `agriculture` role, not by a crew on this sheet. What this patch's share of that pool
    # covers, and where it falls short, is stated on the land card's `Keeping:` / `At risk:` rows.
    var assign_btn := Button.new()
    # The commit verb follows the crew noun the stepper above just asked for — `Forage` for foragers,
    # `Tend` for tenders — keyed off the ONE resolved label, exactly as the hunt web's noop hint is.
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    assign_btn.text = HudComposeVocab.STOP_WORKING_BUTTON if is_unassign \
        else HudFloraVocab.OFFSITE_COMMIT_FORMAT % [offsite_verb.capitalize(),
            DetailFormat.offsite_crop_label(tile_info)] if offsite_verb != "" \
        else String(HudComposeVocab.PLANT_ASSIGN_BUTTONS.get(crew_label, ""))
    HudStyle.apply_button(assign_btn, "primary")
    # Off the sites a crew with no crop to name is a refused command, so the button waits for one.
    var needs_crop := offsite_verb != "" and _compose.forage_species() == ""
    assign_btn.disabled = needs_crop and not is_unassign
    if needs_crop and not is_unassign:
        assign_btn.tooltip_text = HudFloraVocab.OFFSITE_NEEDS_CROP_HINT
    # **ONE COMMAND, AND IT IS `assign_labor`** — the plant twin of the hunt sheet's note. The CROP
    # rides it as its `species` token exactly as before, which is why moving the declaration out did
    # not strand the crop picker: the crop is part of the assignment, not part of the verb.
    assign_btn.pressed.connect(func() -> void:
        if is_unassign:
            _emit_stop_labor(band, SourceForecast.LABOR_KIND_FORAGE, x, y, "")
            close_compose_sheet()
            return
        # **THE SELECTION RIDES EVERY COMMIT, IN FULL, NEVER AS A DELTA.** Re-issuing `assign_labor`
        # without a `take:` token CLEARS the selection sim-side, exactly as it clears the floor and the
        # commit crop — so the sheet has to restate what it is composing on every press, and the empty
        # answer (the whole basket) is what makes a composition that never touched the chips emit the
        # byte-identical line it emitted before they existed.
        _send_forage_assignment(band, x, y)
        if _compose.forage_priority() != HudWorkVocab.WORK_PRIORITY_NORMAL:
            _emit_work_priority(band, SourceForecast.LABOR_KIND_FORAGE, x, y, "", "",
                _compose.forage_priority())
        close_compose_sheet())
    _mount_commit_row(target, assign_btn, is_unassign, func() -> void:
        _bandpanel.show_source_on_map(SourceForecast.LABOR_KIND_FORAGE, x, y, ""))

## ⛔ **RETIRED — `_mount_work_range_refusal`, THE STATIONARY WEBS' RANGE GATE** (issue #650). It
## refused a patch or a working past the band's `band_work_range`, because the sim lapsed such a crew
## with nothing but an event-log line. The work party retired that lapse web by web — forage first,
## then wood and stone — so every sheet now mounts `_mount_work_party_section` past the apron instead,
## and a refusal would forbid the assignment the caravan exists to make.

## Wrap-aware hex distance from `band`'s own tile to `(x, y)`, or `-1` where the grid cannot answer.
## The ONE measurement the work-party gate takes, on every sheet, so no two sheets can measure a
## source two ways.
func _apron_distance(band: Dictionary, x: int, y: int) -> int:
    var band_tile := SourceForecast.band_tile(band)
    return SourceForecast.hex_distance_wrapped(
        band_tile.x, band_tile.y, x, y, _band_labor.grid_width(), _band_labor.wrap_horizontal())

## **IS `(x, y)` PAST `band`'s APRON?** — the one threshold a caravan has (`band_work_range`, the
## sim's `work_party::party_begins_past`), for EVERY job: a hunt gets no longer apron than a gather.
##
## ⛔ **`-1` IS UNKNOWN, NOT FAR.** A frame that arrived before the grid did must not grow a party
## section on every sheet.
func _is_past_apron(band: Dictionary, x: int, y: int) -> bool:
    var distance := _apron_distance(band, x, y)
    return distance >= 0 and distance > int(band.get("work_range", 0))

## **THE WORK-PARTY QUESTION, COMPOSED AND ASKED** — the raid view's pattern on the ordinary sheets:
## one place builds both the key it asks under and the key it reads back.
##
## The subject is band + source (the herd id, or the patch's `x,y`); the key adds everything the
## answer depends on — kit, crew, floor, the band's gear and, on a patch, the take selection. It reads
## `view`, not `view_exact`: the only thing that moves this key under a player's hand is a stepper
## tick or a kit press, which is the bargain the stale window was struck for.
##
## A crew of 0 is `invalid_crew` server-side and there is no posting to price, so it is never asked;
## the section then renders nothing, the unassign/no-op commit below it already saying what happens.
##
## **A WORKING IS NAMED BY ITS TILE AND ITS `material`** — one hex can hold wood and stone both — so
## the material rides both the ask and the subject key. `""` on a herd or a patch.
func _work_party_view(band: Dictionary, source_kind: String, herd_id: String, x: int, y: int,
        take_species: Array, kit_id: String, workers: int, floor: float,
        material: String = "") -> Dictionary:
    if _forecast_query == null or workers <= 0:
        return {}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var source_key := herd_id if source_kind == ForecastQuery.WORK_PARTY_SOURCE_HUNT \
        else "%d,%d" % [x, y]
    if material != "":
        source_key = WORK_PARTY_DEPOSIT_KEY_FORMAT % [source_key, material]
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_WORK_PARTY, band_id, source_key)
    var key := ForecastQuery.key_of(subject, kit_id, workers, floor, band, _band_labor.kits())
    if not take_species.is_empty():
        key += ":" + ",".join(PackedStringArray(take_species))
    _forecast_query.ask(ForecastQuery.KIND_WORK_PARTY, subject, key, {
        "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": band_id,
        "source_kind": source_kind,
        "herd_id": herd_id,
        "x": x,
        "y": y,
        "take_species": take_species,
        "material": material,
        "kit_id": kit_id,
        "workers": workers,
        "floor": floor,
    })
    return _forecast_query.view(subject, key)

## The work-party subject key's material suffix — `x,y:wood` — so the two workings on one hex are two
## subjects and neither reads the other's reply.
const WORK_PARTY_DEPOSIT_KEY_FORMAT := "%s:%s"

## **THE WORK PARTY'S SECTION** — ONE line: how far the source is and when the first load gets home
## (`work_party_section_lines`). What arrives per turn is the PER TURN box's headline, and what rots
## on the walk is that box's amber bullet. Every figure is the reply's; nothing is re-derived here.
##
## **IT RENDERS NOTHING WHERE THE SIM SAYS THERE IS NO PARTY** (`posts_a_party: false`) even though
## the client measured this source past the apron — the sim's answer outranks the client's geometry.
## An empty view (a crew of 0, no seam) renders nothing either.
##
## It carries `HudWidgets.WORK_PARTY_SECTION_META` on its host, and each line
## `HudWidgets.WORK_PARTY_LINE_META` with its own text, so a harness reads what was drawn.
func _mount_work_party_section(target: VBoxContainer, view: Dictionary) -> void:
    if view.is_empty():
        return
    var state := String(view.get("state", ForecastQuery.STATE_PENDING))
    var answer: Dictionary = view.get("answer", {})
    if state == ForecastQuery.STATE_READY and not bool(answer.get("posts_a_party", false)):
        return
    var host := VBoxContainer.new()
    host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    host.set_meta(HudWidgets.WORK_PARTY_SECTION_META, true)
    host.add_child(HudWidgets.alloc_section_label(HudComposeVocab.WORK_PARTY_SECTION_LABEL))
    var lines: Array[String] = []
    if state == ForecastQuery.STATE_PENDING:
        lines.append(HudComposeVocab.WORK_PARTY_PENDING)
    elif state != ForecastQuery.STATE_READY:
        lines.append(HudComposeVocab.FORECAST_FAILED_FORMAT % String(view.get("error", "")))
    else:
        lines = work_party_section_lines(answer)
    for text in lines:
        var line := HudWidgets.alloc_hint_label(text)
        line.set_meta(HudWidgets.WORK_PARTY_LINE_META, text)
        host.add_child(line)
    target.add_child(host)

## **PAST THE APRON THE HEADLINE FOOD FIGURE IS THE RATE ARRIVING HOME** — the query's `rate_home`,
## which is the `netRateHome` the committed row will print the turn after commit. A compose sheet
## promises what the row it becomes will show, and a near row and a far row show one comparable food
## number per source; the take AT the source is not that number on a posting, the party eating some of
## it and some of its hands being on the road.
##
## ⛔ **ONLY THE FOOD ROW MOVES.** Every reading that describes the SOURCE — the chart, the crew
## targets, the verdict, the other accounts — keeps reading the source, and the food row drops its
## `now → after` reading (a steady rate home has no walk to the floor to key) and its waste note. Inside the apron
## `party_view` is `{}` and the model passes through untouched, which is the identity.
##
## **Until the reply lands** (or where it posts no party) the model is returned unchanged: there is no
## home rate to state yet, and a blank headline would read as a source that pays nothing.
##
## **`account` IS THE ROW THE RATE HOME IS IN** — `food` on a herd or a patch, the working's own
## MATERIAL on a deposit, whose reply's `rate_home` is in that material's units. A material row's
## account is its id (`SourceForecast.yield_rows`), so the headline reads `+1.4 wood` under the same
## `once running · per turn` caption and never as food. Nothing else forks: one caravan, one
## substitution, whatever the far job carries home.
##
## **A FAR PARTY'S PACKS CARRY ITS BY-PRODUCTS HOME TOO** (#706) — a forage party's hay and fibre, a
## hunt party's hide, bone and sinew — so on those two sheets (`carries_by_products`) the fodder and
## material rows read the reply's `fodder_rate_home` / `materials_rate_home`, the rates the committed
## row will publish, rather than the take at the source. A row the home rates no longer pay leaves; a
## muted (unbankable) fodder row keeps its glyph. A deposit's account IS its material, so the deposit
## sheet passes `false` and keeps its rows as they were.
func _with_home_rate(model: Dictionary, party_view: Dictionary,
        account: String = SourceForecast.YIELD_ACCOUNT_FOOD,
        carries_by_products: bool = false) -> Dictionary:
    if party_view.is_empty() or model.is_empty() \
            or String(party_view.get("state", "")) != ForecastQuery.STATE_READY:
        return model
    var answer: Dictionary = party_view.get("answer", {})
    if not bool(answer.get("posts_a_party", false)):
        return model
    var rate_home := float(answer.get("rate_home", 0.0))
    var out := model.duplicate()
    var rows: Array = (model.get(YIELD_MODEL_ROWS, []) as Array).duplicate(true)
    var found := false
    for row_variant in rows:
        var row: Dictionary = row_variant
        if String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, "")) == account:
            row[SourceForecast.YIELD_ROW_VALUE] = rate_home
            row.erase(SourceForecast.YIELD_ROW_AFTER)
            found = true
    if not found and SourceForecast.has_component(rate_home):
        rows.push_front({SourceForecast.YIELD_ROW_ACCOUNT: account,
            SourceForecast.YIELD_ROW_VALUE: rate_home})
    if carries_by_products:
        rows = _with_home_by_products(rows, account, answer)
    out[YIELD_MODEL_ROWS] = rows
    out[YIELD_MODEL_HOME_RATE] = true
    # **AND WHAT THE WALK ROTS, RIGHT UNDER THE NUMBER IT IS ALREADY NET OF** (#706) — the amber
    # bullet `_fill_yields_host` mounts. `""` when nothing rots.
    out[YIELD_MODEL_ROT] = HudWorkVocab.rot_line(float(answer.get("spoiled_rate_home", 0.0)),
        rate_home)
    return out

## The by-product rows of a far forage sheet, re-read off the reply's home rates (`_with_home_rate`).
## Every row but `account`'s is either the fodder row or a material row keyed by its id; each takes its
## home figure (0 where the reply carries none) and loses its `now → after`, a row left at nothing is
## dropped unless it is muted, and a home rate with no row yet gets one.
static func _with_home_by_products(rows: Array, account: String, answer: Dictionary) -> Array:
    var home := {SourceForecast.YIELD_ACCOUNT_FODDER: float(answer.get("fodder_rate_home", 0.0))}
    for material in SourceForecast.material_payoff_rows(answer.get("materials_rate_home", [])):
        home[String(material[SourceForecast.MATERIAL_PAYOFF_ID_KEY])] = \
            float(material[SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY])
    var out: Array = []
    var seen := {}
    for row_variant in rows:
        var row: Dictionary = row_variant
        var row_account := String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, ""))
        if row_account == account:
            out.append(row)
            continue
        var value := float(home.get(row_account, 0.0))
        seen[row_account] = true
        row[SourceForecast.YIELD_ROW_VALUE] = value
        row.erase(SourceForecast.YIELD_ROW_AFTER)
        if SourceForecast.has_component(value) or bool(row.get(HudWidgets.YIELD_ROW_MUTED, false)):
            out.append(row)
    for home_account in home:
        if not seen.has(home_account) and SourceForecast.has_component(float(home[home_account])):
            out.append({SourceForecast.YIELD_ROW_ACCOUNT: String(home_account),
                SourceForecast.YIELD_ROW_VALUE: float(home[home_account])})
    return out

## The section's ONE line, as text, off a READY reply — split out so the harness drives every branch
## of the copy without a sheet. How far the source is, and when the first load gets home once the
## forecast lands one; a road covering the run reads the no-walk line instead.
static func work_party_section_lines(answer: Dictionary) -> Array[String]:
    var walk_tiles := int(answer.get("walk_tiles", 0))
    if walk_tiles <= 0:
        return [HudComposeVocab.WORK_PARTY_NO_WALK]
    var line := HudComposeVocab.WORK_PARTY_AWAY_FORMAT % _counted(walk_tiles,
        HudComposeVocab.WORK_PARTY_TILES_FORMAT, HudComposeVocab.WORK_PARTY_TILES_ONE)
    var first_load := int(answer.get("first_load_turn", 0))
    if first_load > 0:
        line += HudComposeVocab.WORK_PARTY_FIRST_LOAD_CLAUSE_FORMAT % _counted(first_load,
            HudComposeVocab.WORK_PARTY_TURNS_FORMAT, HudComposeVocab.WORK_PARTY_TURNS_ONE)
    return [line]

## `n` in its counted phrase, singular at one.
static func _counted(n: int, plural_format: String, singular: String) -> String:
    return singular if n == HudComposeVocab.WORK_PARTY_COUNT_SINGULAR else plural_format % n

# ---- THE COMPOSE SHEET: the drawer's read state + the floating write state --------------------
#
# docs/plan_tile_panel_layout.md §10-§15. The drawer keeps the detail rows, gains a one-line
# standing-assignment summary, and ends in `Assign … ▸`; the sheet (`ui/hud/ComposeSheet.gd`) hosts
# the compose block itself. NOTHING is re-derived here — the summary's rate comes from the same
# `SourceForecast.source_yield_readout` the Band panel's Current-actions rows use, and every gate, forecast and
# ceiling in the sheet comes from the same call it came from when the block lived in the drawer.

## Build the compose sheet once. Like the fork panel it is NOT a child of `layout_root`: it floats
## over the whole window and must not inset with the reserved docks.
##
## **AND ITS PARENT IS THE COMPOSE LAYER, NOT THE HUD** (`HudLayer.compose_host()`). Parented onto
## the HUD it drew under a top-docked event bar — and, because that bar stops the pointer, the ✕ and
## the sheet's own rows under it could not be clicked at all. The compose layer sits one above the
## dock's; see `HudLayer.COMPOSE_LAYER_INDEX` for the ladder.
##
## **THE SHEET KEEPS ITS FULL-WINDOW DISMISS CATCHER, deliberately.** Now that the sheet is on top, a
## click on the bar behind it lands on the catcher and DISMISSES the sheet rather than reaching the
## bar. That is the wanted behaviour for a modal write surface: one click puts the sheet away, and
## the bar is still there for the second.
func _ensure_compose_sheet() -> void:
    if _compose_sheet != null:
        return
    _compose_sheet = ComposeSheet.new()
    _compose_sheet.name = "ComposeSheet"
    _compose_sheet.closed.connect(_on_compose_sheet_closed)
    _host.compose_host().add_child(_compose_sheet)

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
    _applied_until_msec = 0
    _compose.clear_composing()
    _compose.reset_forage_source()
    _compose.reset_hunt_source()
    # …and the deposit composition with them, for the identical reason: an uncommitted crew that
    # outlives its sheet is a promise nothing in the game is keeping.
    _compose.reset_deposit_source()
    _compose.reset_tile_source()
    _end_floor_drag()
    refresh_drawer_actions()

## **THE GESTURE IS OVER — and a drag flag that outlives its chart is worse than no flag at all**,
## because `refresh_compose_sheet` would then refill a set of freed hosts forever and the sheet would
## stop answering snapshots. So it is cleared at every point a drag can end WITHOUT a release: the
## sheet closing, and either builder running (the rebuild frees the chart, which is exactly what ends
## a drag). The release itself clears it inline, before the rebuild it triggers.
func _end_floor_drag() -> void:
    _floor_drag_live = false
    _floor_drag_refill = Callable()

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
    return DetailFormat.tile_is_workable(tile_info) \
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

## The Band panel and the shared rung card, handed over once both exist. The sheet's work strip is built
## by the panel's own producer (`source_strip_row`) so the sheet and the source's Work-tab row are one
## control; its `⌃` opens the SAME card (`RungCardController`) with the same commands.
## **THE BAND ORDERS DOOR** (#860), injected by `HudLayer`.
var _orders: BandOrdersController = null
## The band whose Food / Fodder sheet is open (entity), re-resolved every render.
var _order_sheet_band: int = -1

func set_band_orders(orders: BandOrdersController) -> void:
    _orders = orders

## The context the chief's sheet sections draw against: the band, the sends, and the three callbacks.
func _orders_ctx(band: Dictionary, rebuild: Callable) -> Dictionary:
    return {
        "band": band, "orders": _orders, "rebuild": rebuild,
        "close": func() -> void: close_compose_sheet(),
        "show_on_map": func(x: int, y: int) -> void:
            _bandpanel.show_source_on_map(HudConst.LABOR_KIND_EXTRACT, x, y, ""),
    }

## **OPEN A BAND ORDER'S SHEET FROM THE WORK TAB** (#860): Food and Fodder get their own sheet; a
## material's order opens the material's sheet on the site the chief is working.
func open_order_sheet(band_entity: int, kind: String) -> void:
    var band := _band_labor.player_band_by_entity(band_entity)
    var order := HudOrdersVocab.order_of(band, kind)
    if band.is_empty() or order.is_empty():
        return
    if not HudOrdersVocab.is_turns_kind(kind):
        if not bool(order.get("has_site", false)):
            return
        var site_x := int(order.get("site_x", -1))
        var site_y := int(order.get("site_y", -1))
        _compose.set_deposit_band(band_entity)
        # Through the panel, which selects the site's tile before the sheet opens.
        _bandpanel.open_site_sheet(site_x, site_y, kind)
        return
    _ensure_compose_sheet()
    _order_sheet_band = band_entity
    _compose.set_composing(ComposeState.KIND_ORDER, kind)
    var content := _compose_sheet.open(HudOrdersVocab.order_name(kind), HudFormat.band_name(band),
        "order:" + kind, _compose_anchor_rect())
    _build_order_sheet(kind, content)
    refresh_drawer_actions()

## **THE HERDS THE FODDER ORDER FEEDS**: every tamed or penned herd the band's people keep that draws
## fodder. A herd its own range feeds outright draws none and is not listed.
func _band_eating_herds(band: Dictionary) -> Array:
    var herds: Array = []
    for herd_variant in _band_labor.world_herds():
        var herd: Dictionary = herd_variant
        if not bool(herd.get("has_owner", false)) \
                or int(herd.get("owner", -1)) != int(band.get("faction", HudConst.PLAYER_FACTION_ID)):
            continue
        var kept := RungQuotes.is_enclosed(herd) \
                or RungQuotes.domestication_progress(herd) >= HudConst.KNOWLEDGE_COMPLETE
        var eats := float(herd.get("fodder_draw", 0.0)) + float(herd.get("pen_fodder_shortfall", 0.0))
        if kept and eats >= SourceForecast.FODDER_FLOW_MIN:
            herds.append(herd)
    return herds

func _build_order_sheet(kind: String, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    var band := _band_labor.player_band_by_entity(_order_sheet_band)
    var order := HudOrdersVocab.order_of(band, kind)
    if band.is_empty() or order.is_empty():
        return
    var ctx := _orders_ctx(band, func() -> void: _build_order_sheet(kind, target))
    BandOrdersSheets.build_order_sheet(target, ctx, order,
        _band_eating_herds(band) if kind == HudOrdersVocab.KIND_FODDER else [], _herd_label_for_id)

## The strip (and the disabled controls) a hunt / gather sheet wears for a row under the chief's order.
## `source_kind` is `hunt` / `forage`; `address` names the row for a release / claim.
func _apply_chief_row_strip(target: VBoxContainer, band: Dictionary, source_kind: String,
        x: int, y: int, herd_id: String, address: Dictionary, rebuild: Callable) -> void:
    if _orders == null or band.is_empty():
        return
    var info := _orders.order_for_source(band, source_kind, x, y, herd_id)
    if info.is_empty():
        return
    var ctx := _orders_ctx(band, rebuild)
    var strip := BandOrdersSheets.row_strip(ctx, info, source_kind, address)
    var live: Array = []
    for child in target.get_children():
        if not child.is_queued_for_deletion():
            live.append(child)
    if live.is_empty():
        return
    target.add_child(strip)
    # Under the `Band:` picker, which is the first live child.
    target.move_child(strip, (live[0] as Node).get_index() + 1)
    if bool(info["chief_owned"]):
        var keep: Array = [HudWorkVocab.WORK_STRIP_META, BandOrdersSheets.META_ROW_STRIP,
            HudComposeVocab.DONE_BUTTON_META, HudComposeVocab.SHOW_ON_MAP_META]
        for i in range(1, live.size()):
            BandOrdersSheets.disable_node(live[i], keep,
                HudOrdersVocab.CHIEF_ROW_DISABLED_TIP_FORMAT % HudOrdersVocab.who(band))
            # `Stop working here` is a hand command the sim refuses on his row.
            BandOrdersSheets.hide_meta(live[i], HudWidgets.COMPOSE_COMMIT_META)

func set_work_strip_sources(band_panel: BandPanelController, rung_card: RungCardController) -> void:
    _bandpanel = band_panel
    _rung = rung_card

## **OPEN THE SHEET OF A SOURCE THE PLAYER JUST CLICKED ON THE WORK TAB** (issue #877). The row has
## already selected the source (the land, or the herd), so the subject is resolved off the selection:
## the sheet then anchors beside the selection card exactly as the drawer's own button opens it, and the
## per-snapshot refresh finds the selected subject is the composed one and leaves it open.
func open_source_sheet(kind: String, x: int, y: int, herd_id: String, material: String) -> void:
    match kind:
        SourceForecast.LABOR_KIND_FORAGE:
            open_forage_compose(_selection.tile_info())
        SourceForecast.LABOR_KIND_HUNT:
            var herd := _selection.herd()
            if String(herd.get("id", "")) != herd_id:
                herd = _band_labor.find_world_herd(herd_id)
            open_herd_compose(herd)
        HudConst.LABOR_KIND_EXTRACT:
            var working := {}
            for candidate in _tile_workings(_selection.tile_info()):
                if HudDepositVocab.material_of(candidate) == material \
                        and HudDepositVocab.tile_of(candidate) == Vector2i(x, y):
                    working = candidate
            if working.is_empty():
                working = _band_labor.find_deposit(x, y, material)
            open_deposit_compose(working)
        HudConst.LABOR_KIND_TILE:
            # The branch rides the `material` slot, as it does in `pending_key`.
            open_tile_compose(_selection.tile_info(), material)

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
    var offsite_label := "" if String(tile_info.get("patch_committed_species", "")).strip_edges() != "" \
        else DetailFormat.offsite_crop_label(tile_info)
    var eyebrow := HudFloraVocab.OFFSITE_SHEET_EYEBROW_FORMAT % offsite_label if offsite_label != "" \
        else HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudFormat.plant_crew_label(
            tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX).to_lower()
    var content := _compose_sheet.open(
        eyebrow,
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
## **THE CREW TERM OF THE CREW-TAKE KEY, BOUNDED** — `HudComposeVocab.HUNT_CREW_TAKE_MAX_WORKERS`,
## which is `core_sim`'s own `MAX_CREW_TAKE_WORKERS`.
##
## **BOTH ASK PATHS GO THROUGH IT, and they have to go through the SAME one.** The number is half the
## question and half the key (`ForecastQuery.key_of`), so a clamp applied on one path and not the
## other would ask under one key and read back under another — the sheet would then wait forever on an
## answer that had already landed. Clamping here, before either the key or the payload is built, keeps
## the two spellings of the question identical by construction.
##
## The sim REFUSES an over-large ask rather than clamping it (`query_error::INVALID_CREW`), so an
## unclamped client would render `No forecast available (invalid_crew)` where a curve belongs. See the
## const for why nothing in play reaches the bound.
func _crew_take_workers(max_workers: int) -> int:
    return mini(max_workers, HudComposeVocab.HUNT_CREW_TAKE_MAX_WORKERS)

## **THE CREW-TAKE QUESTION, COMPOSED AND ASKED** — one
## place so the ask and the read cannot describe two different herds.
##
## **`max_workers` IS THE KEY'S CREW TERM, NOT THE COMPOSED CREW.** The reply is one row per crew from
## 1 to that cap, so the whole stepper is answered by one round trip and stepping it asks nothing
## new — see `ForecastQuery.key_of`. It is the band's own pool (`source_crew_pool_hunt`), which is
## also the ceiling the stepper is clamped to, so no reachable crew is off the end of the curve.
##
## **A CREW OF 0 IS STILL ASKED**, unlike the raid: the sheet is composing a curve rather than one
## party, the stepper is about to be filled from it, and a pool of zero is what answers nothing.
##
## **AND IT READS `view_exact`, FOR THE SAME REASON THE DRAG DOES.** The stale window is a bargain
## struck for a control whose motion is one tick of a stepper; the two terms of THIS key are the kit
## and the FLOOR, and neither is a tick — a different floor is a different room, a different kit a
## different fight. Read through `view`, the `ask` three lines above stamps `asked_key` with the very
## key being read back, so the seam answers `STATE_READY` carrying the PREVIOUS floor's curve: the
## release of a drag onto a floor the 120 ms rate limit never asked composed the take, the band, the
## cadence, the yields and the binding-limit sentence out of the floor the player had already left,
## with `take_state_host` hidden and nothing on the sheet saying so. Worse, `arm_hunt_autofill` then
## filled the stepper from that plateau and the real reply's `clamp_hunt_max` can only LOWER it, so
## a floor dragged down left a permanently under-staffed party.
##
## Exact, an answer stands only for the key it was actually asked at; everything else is
## `STATE_PENDING`, which is what puts `HudComposeVocab.HUNT_TAKE_PENDING` on the sheet until the real
## curve lands. The stepper itself is unaffected either way — the key carries the POOL, not the
## composed crew, so a `+` press re-reads an answer the seam already holds.
## **THE HERD'S STATE IS A TERM OF THE QUESTION** — the rows are bounded by the stock above the floor
## and their `next_rung_*` price the rung ABOVE the one the herd stands on, so a herd that grew, shrank
## or climbed a rung is a new question rather than the previous one's answer (the patch curve's
## stand terms, on the animal web).
func _hunt_curve_herd_terms(herd_id: String) -> String:
    var herd := _live_herd(herd_id, _band_labor.find_world_herd(herd_id))
    return ":%s:%f:%f:follow=%s" % [
        String(herd.get(SourceForecast.FORECAST_CURRENT_RUNG_KEY, "")),
        RungQuotes.domestication_progress(herd), float(herd.get("biomass", 0.0)),
        str(_hunt_curve_follows(herd_id))]

## **HUNTING BY NEED ASKS A DIFFERENT QUESTION** (#798): ticked on a migratory herd, the curve is priced
## over the gear the band's whole muster brings (donors lend their kits) rather than the idle crew's,
## so the ask carries `follow = true` and the key carries it too — a non-follow curve must never answer
## the follow sheet (nor the reverse). Same predicate as the sheet's `needs_hunt`.
func _hunt_curve_follows(herd_id: String) -> bool:
    var herd := _live_herd(herd_id, _band_labor.find_world_herd(herd_id))
    return _herd_is_migratory(herd) and _compose.hunt_move_with_herd()

## The ask's `max_workers` ceiling: the band's whole working-age workforce when following (the server
## clamps it to the musterable pool), else the caller's own pool. Run through `_crew_take_workers`.
func _hunt_curve_cap(band: Dictionary, herd_id: String, pool: int) -> int:
    if _hunt_curve_follows(herd_id):
        return _crew_take_workers(maxi(int(band.get("working_age", 0)), 0))
    return _crew_take_workers(pool)

## The hunt crew-take ask payload, one spelling for the build and drag paths. `follow` is present only
## when true, so an unticked ask is byte-identical to the pre-#798 one.
func _hunt_crew_take_params(band: Dictionary, band_id: int, herd_id: String, kit_id: String,
        floor: float, workers: int) -> Dictionary:
    var params := {
        "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": band_id,
        "herd_id": herd_id,
        "kit_id": kit_id,
        "floor": floor,
        "max_workers": workers,
    }
    if _hunt_curve_follows(herd_id):
        params["follow"] = true
    return params

func _crew_take_view(band: Dictionary, herd_id: String, kit_id: String, floor: float,
        max_workers: int) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var workers := _hunt_curve_cap(band, herd_id, max_workers)
    # **NO HANDS TO PRICE ASKS NOTHING AND IS NOT WAITING ON ANYTHING** - an answered, EMPTY curve (the
    # deposit sheet's rule): `PENDING` here read the "Costing..." line forever over a question never put.
    if workers <= 0:
        return {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_CREW_TAKE, band_id, herd_id)
    var key := ForecastQuery.key_of(subject, kit_id, workers, floor, band,
        _band_labor.kits()) + _hunt_curve_herd_terms(herd_id)
    if workers > 0 and band_id != HudConst.NO_BAND_ID and herd_id != "":
        _forecast_query.ask(ForecastQuery.KIND_HUNT_CREW_TAKE, subject, key,
            _hunt_crew_take_params(band, band_id, herd_id, kit_id, floor, workers))
    return _forecast_query.view_exact(subject, key)

## **THE SAME QUESTION, PUT WHILE THE FLOOR IS STILL MOVING** — and the reason it has to be put at all
## is that the curve is FLOOR-DEPENDENT: every row is bounded by the room standing above the
## escapement floor, so rows asked at the floor the drag started from describe a herd the player has
## already dialled past. `ForecastQuery.key_of` carries the floor, so a new floor is a new key and
## therefore a new question; what was missing was anyone asking it before the release.
##
## **THE CLIENT MAY NOT PATCH THE GAP ITSELF, and that is why the sheet waits instead.** The room
## clamps the ENGAGEMENT, before the retreat and before the fight (`fauna::resolve_hunt_engagement`),
## so a floor-shifted row cannot be recovered from a row in hand by scaling it — recomposing it means
## re-fighting the fight, which is the whole thing this channel exists to stop the client doing.
##
## **RATE-LIMITED, NOT DEBOUNCED** — see `HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC` for
## why the leading edge is the right one to keep and why the trailing edge needs no timer. The clock
## is charged only when a question actually goes out: a motion landing back on a floor already asked
## is one the seam would drop as a duplicate, and spending the budget on it would delay the next floor
## that is genuinely new.
##
## Writes the live pair the drag's hosts read; it never returns anything, because the refill that
## follows it reads those members and not a value threaded through the closure.
func _drag_crew_take(band: Dictionary, herd_id: String, kit_id: String, floor: float,
        max_workers: int) -> void:
    if _forecast_query == null:
        return
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    # **THE SAME CLAMP THE BUILD PATH APPLIES, and it must be the same one** — see
    # `_crew_take_workers`: the crew term is half the key, so two paths clamping differently would ask
    # under one key and read back under another.
    var workers := _hunt_curve_cap(band, herd_id, max_workers)
    if workers <= 0:
        _hunt_live_crew_view = {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
        _hunt_live_crew_take = []
        return
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_CREW_TAKE, band_id, herd_id)
    var key := ForecastQuery.key_of(subject, kit_id, workers, floor, band,
        _band_labor.kits()) + _hunt_curve_herd_terms(herd_id)
    var now := Time.get_ticks_msec()
    if key != _crew_take_drag_asked_key \
            and now - _crew_take_drag_asked_at_msec \
                >= HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC \
            and workers > 0 and band_id != HudConst.NO_BAND_ID and herd_id != "":
        _crew_take_drag_asked_key = key
        _crew_take_drag_asked_at_msec = now
        _forecast_query.ask(ForecastQuery.KIND_HUNT_CREW_TAKE, subject, key,
            _hunt_crew_take_params(band, band_id, herd_id, kit_id, floor, workers))
    # **READ EXACTLY, whether or not this motion asked.** A suppressed motion is still a floor the
    # sheet has no answer for, and the answer it does hold is the previous floor's — so the take line
    # says it is waiting rather than quoting a take for a floor the player has left. See
    # `ForecastQuery.view_exact`.
    _hunt_live_crew_view = _forecast_query.view_exact(subject, key)
    _hunt_live_crew_take = (_hunt_live_crew_view["answer"] as Dictionary).get("per_crew", [])

## **THE DEPOSIT CREW CURVE, COMPOSED AND ASKED** (`ForecastQuery.KIND_DEPOSIT_CREW_TAKE`, issue
## #663) — `_crew_take_view`'s twin for a working, keyed the same way: the SUBJECT is band + working
## (`x,y:material`, since one hex holds two), the key adds the kit, the floor, the band's gear and the
## POOL rather than the composed crew, so a stepper tick re-reads rows the seam already holds.
##
## **`view_exact`, for the hunt curve's reason** — the kit and the floor are the terms of this key and
## neither is a stepper tick, so the previous kit's or floor's rows must never stand in for this one's.
## A crew of 0 in the pool asks nothing; the sheet then has no curve and states no take.
func _deposit_crew_take_view(band: Dictionary, deposit: Dictionary, kit_id: String, floor: float,
        max_workers: int) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var ask := _deposit_crew_take_ask(band, deposit, kit_id, floor, max_workers)
    # **A BAND WITH NO HANDS TO SPEND ASKS NOTHING, AND IS NOT WAITING ON ANYTHING.** Nobody can be
    # put on the stepper, and a crew of nobody cuts nothing — a fact, not a forecast — so the view is
    # an answered, EMPTY curve: no row, no take, no gear line. `PENDING` here would read *costing…*
    # forever over a question that is never put.
    if not bool(ask["askable"]):
        return {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
    _forecast_query.ask(ForecastQuery.KIND_DEPOSIT_CREW_TAKE, String(ask["subject"]),
        String(ask["key"]), ask["params"])
    return _forecast_query.view_exact(String(ask["subject"]), String(ask["key"]))

## **THE SAME QUESTION WHILE THE FLOOR IS MOVING** — `_drag_crew_take`'s twin, on the SAME rate limit
## and the same two members (only one compose sheet is ever open, so the two drags cannot interleave).
## Writes `_deposit_live_crew_view`, which the live refill reads.
func _drag_deposit_crew_take(band: Dictionary, deposit: Dictionary, kit_id: String, floor: float,
        max_workers: int) -> void:
    if _forecast_query == null:
        return
    var ask := _deposit_crew_take_ask(band, deposit, kit_id, floor, max_workers)
    if not bool(ask["askable"]):
        _deposit_live_crew_view = {"state": ForecastQuery.STATE_READY, "answer": {}, "error": ""}
        return
    var key := String(ask["key"])
    var now := Time.get_ticks_msec()
    if key != _crew_take_drag_asked_key \
            and now - _crew_take_drag_asked_at_msec \
                >= HudComposeVocab.HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC:
        _crew_take_drag_asked_key = key
        _crew_take_drag_asked_at_msec = now
        _forecast_query.ask(ForecastQuery.KIND_DEPOSIT_CREW_TAKE, String(ask["subject"]), key,
            ask["params"])
    _deposit_live_crew_view = _forecast_query.view_exact(String(ask["subject"]), key)

## The ONE composition of the deposit curve question — subject, key and payload — so the build path
## and the drag path cannot spell it two ways (a different key on one path would wait forever on an
## answer the other had already received). The crew term goes through `_crew_take_workers`, the same
## clamp the hunt curve takes: the sim refuses above it (`invalid_crew`) rather than clamping.
func _deposit_crew_take_ask(band: Dictionary, deposit: Dictionary, kit_id: String, floor: float,
        max_workers: int) -> Dictionary:
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var workers := _crew_take_workers(max_workers)
    var tile := HudDepositVocab.tile_of(deposit)
    var material := HudDepositVocab.material_of(deposit)
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_DEPOSIT_CREW_TAKE, band_id,
        _deposit_source_key(deposit))
    return {
        "subject": subject,
        "key": ForecastQuery.key_of(subject, kit_id, workers, floor, band, _band_labor.kits()),
        "askable": workers > 0 and band_id != HudConst.NO_BAND_ID and tile.x >= 0
            and material != HudDepositVocab.MATERIAL_NONE,
        "params": {
            "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id,
            "x": tile.x,
            "y": tile.y,
            "material": material,
            "kit_id": kit_id,
            "floor": floor,
            "max_workers": workers,
        },
    }

## **A REFRESH ALONE IS NOT ENOUGH, AND THAT IS THE WHOLE REASON THIS IS ITS OWN METHOD.** Both
## builders ADOPT the overlay's declaration into the composition — `if composed_improvement !=
## _compose.<web>_improvement(): set_<web>_improvement(...)`, which is what lets a `⌃` press flip an
## open sheet on the same frame — so by the time the drop lands, the rung the player never ticked is
## the sheet's own composed verb and `_declared_or_composed` honours it forever. The adoption is
## withdrawn here and the sheet then re-renders into the OFFER.
##
## **IT IS WITHDRAWN ONLY WHERE IT IS THIS SHEET'S SUBJECT AND THIS RUNG**, so a rung the player
## ticked on the sheet themselves, or a sheet standing on another source, is untouched — the failed
## command names one entry and may undo exactly that one, which is `drop_pending_assign`'s own rule
## one layer up. A build the METERS say is running is re-adopted by the rebuild, correctly: that one
## is a fact about the source rather than a declaration anybody can withdraw.
##
## `improvement` empty (an ordinary crew rollback, which declares nothing) is a plain refresh.
func withdraw_declaration(kind: String, x: int, y: int, herd_id: String,
        improvement: String) -> void:
    if improvement != SourceForecast.IMPROVEMENT_NONE and is_compose_sheet_open():
        match _compose.kind():
            ComposeState.KIND_FORAGE:
                var tile_info := _selection.tile_info()
                if kind == SourceForecast.LABOR_KIND_FORAGE \
                        and _forage_source_key(tile_info) == _compose.subject() \
                        and int(tile_info.get("x", -1)) == x \
                        and int(tile_info.get("y", -1)) == y \
                        and _compose.forage_improvement() == improvement:
                    _compose.set_forage_improvement(SourceForecast.IMPROVEMENT_NONE)
            ComposeState.KIND_HERD:
                if herd_id != "" and herd_id == _compose.subject() \
                        and _compose.hunt_improvement() == improvement:
                    _compose.set_hunt_improvement(SourceForecast.IMPROVEMENT_NONE)
    refresh_compose_sheet()

## > #### ⛔ `may_close` IS FALSE FOR A FORECAST ANSWER, AND THAT IS A CORRECTNESS RULE
## >
## > This is the shared *"re-render in place, close only if the subject is gone"* path, and it decides
## > *gone* by comparing the sheet's subject against the SELECTION. That is the right authority for the
## > SNAPSHOT path — a herd can die, a patch can stop offering the compose — and it is no authority at
## > all for a QUERY REPLY: an answer says nothing about whether the subject still exists, it says what
## > the sheet asked about the subject it is already holding.
## >
## > **It became reachable the day the LOCAL branch started asking.** Only the expedition branch used
## > the query channel, and an expedition sheet is always opened from the selection, so the two could
## > not disagree; the crew-take question put every local hunt sheet on that channel, and an answer
## > landing on a sheet whose subject the drawer is not showing then TORE THE SHEET DOWN — measured in
## > `ui_preview`, where the drawer-actions path is driven directly and the selection is deliberately
## > another herd. Opening a sheet must not depend on a reply, and neither must keeping it open.
## >
## > **A mismatch under `may_close = false` re-renders NOTHING** rather than rebuilding against the
## > selection: the builders take the subject from `_selection`, so a rebuild there would compose the
## > sheet for the wrong herd — or, on an empty selection, for no herd at all. The sheet keeps the
## > frame it has, which is the honest answer to an answer it cannot place.
## > #### ⛔ A LIVE FLOOR DRAG IS REFILLED, NEVER REBUILT — including by a forecast answer
## >
## > A rebuild `queue_free`s the chart, and Godot routes motion to the node that took the press, so a
## > sheet rebuilt mid-drag ends the drag on the next pixel. That was harmless while a drag put no
## > questions on the socket: nothing could answer during one. **The drag-time re-ask made it
## > reachable on purpose** — the whole point is that an answer lands WHILE the player is still
## > holding the chart — and `answered` arrives here.
## >
## > So a live drag takes the refill the builder published instead (`_floor_drag_refill`), at the floor
## > the drag is currently on. Every reading that follows the floor is in the live registry, so the
## > refill lands the new answer in exactly the hosts a rebuild would have redrawn — minus the chart,
## > which is the one node that must survive.
func refresh_compose_sheet(may_close: bool = true) -> void:
    if not is_compose_sheet_open():
        return
    if _floor_drag_live and _floor_drag_refill.is_valid():
        var live_floor := _compose.forage_floor()
        if _compose.kind() == ComposeState.KIND_HERD:
            live_floor = _compose.hunt_floor()
        elif _compose.kind() == ComposeState.KIND_DEPOSIT:
            live_floor = _compose.deposit_floor()
        _floor_drag_refill.call(live_floor)
        return
    match _compose.kind():
        ComposeState.KIND_FORAGE:
            if _forage_source_key(_selection.tile_info()) != _compose.subject() \
                    or not _forage_compose_available(_selection.tile_info()):
                if may_close:
                    close_compose_sheet()
                return
            _build_forage_assign_controls(_selection.tile_info(), _compose_sheet.content())
        ComposeState.KIND_HERD:
            if String(_selection.herd().get("id", "")) != _compose.subject() \
                    or not _herd_compose_available(_selection.herd()):
                if may_close:
                    close_compose_sheet()
                return
            _build_herd_assign_controls(_selection.herd(), _compose_sheet.content())
        ComposeState.KIND_DEPOSIT:
            # **THE WORKING IS RE-RESOLVED OFF THE SELECTED TILE**, never held: a snapshot restates
            # every figure on this sheet, and a captured row would quote the turn the sheet opened.
            # A hex that no longer carries this material — the selection moved — is the sheet's
            # subject being GONE, which is exactly what `may_close` is for.
            var working := _live_deposit(_compose.subject(), {})
            if working.is_empty() or not _deposit_compose_available(working):
                if may_close:
                    close_compose_sheet()
                return
            _build_deposit_assign_controls(working, _compose_sheet.content())
        ComposeState.KIND_ORDER:
            var order_band := _band_labor.player_band_by_entity(_order_sheet_band)
            if order_band.is_empty() or HudOrdersVocab.order_of(order_band, _compose.subject()).is_empty():
                if may_close:
                    close_compose_sheet()
                return
            _build_order_sheet(_compose.subject(), _compose_sheet.content())
        ComposeState.KIND_TILE:
            var tile_branch := _tile_branch_of_key(_compose.subject())
            var live_tile := _selection.tile_info()
            if live_tile.is_empty() or _tile_subject_key(live_tile, tile_branch) != _compose.subject() \
                    or not _tile_compose_available(live_tile, tile_branch):
                if may_close:
                    close_compose_sheet()
                return
            _build_tile_assign_controls(live_tile, tile_branch, _compose_sheet.content())
        _:
            if may_close:
                close_compose_sheet()

## Re-render whichever subject's drawer actions are showing (the herd's standing summary + its
## `Assign … ▸` button), so a turn's staffing change lands in the read state as well as in the open sheet.
## The LAND has no actions to rebuild: its doors are rows of `%TileDetail`, and what this announces for
## it is whether the sheet a door opened has opened or closed (`door_selection_changed`).
func refresh_drawer_actions() -> void:
    if not _selection.herd().is_empty():
        build_herd_drawer_actions(_selection.herd())
    _note_door_selection()

# ---- THE LAND CARD'S DOORS (the rows that open a sheet) --------------------------------------------
#
# ⛔ **A DOOR IS A ROW OF `%TileDetail`, NOT A BUTTON BENEATH IT.** `SubjectDrawerController` registers
# which rows are doors and renders them; this controller answers the three questions it cannot:
# *is this source openable* (the same gates the retired buttons used), *what does the standing summary
# say* (the SAME `_standing_summary_model` call the retired button's second line made), and *what does a
# press open* (`open_door`, through the live-resolution the buttons used, never a captured dict).
#
# **WHICH ROW IS SELECTED IS THE OPEN SHEET'S TO SAY** (`open_door_meta`), never a click memo: every
# way a sheet closes (commit, ✕, Esc, catcher click, a selection change) reaches
# `_on_compose_sheet_closed` or the road card's `popup_hide`, and both announce through
# `door_selection_changed` when the answer moved.

## The open sheet / road card changed, so the land card's selected wash should follow.
signal door_selection_changed

## The meta last announced, so an unchanged answer (every snapshot's refresh) costs no redraw.
var _announced_door_meta: String = ""

func _note_door_selection() -> void:
    var meta := open_door_meta()
    if meta == _announced_door_meta:
        return
    _announced_door_meta = meta
    door_selection_changed.emit()

## The door meta of whichever land sheet is open right now, `""` for none: a forage / deposit / tile
## compose sheet (its `ComposeState` kind + subject) or the road ladder card (its open state + index).
func open_door_meta() -> String:
    if is_compose_sheet_open():
        match _compose.kind():
            ComposeState.KIND_FORAGE:
                return HudDisclosureVocab.door_meta(HudDisclosureVocab.DOOR_KIND_FORAGE, _compose.subject())
            ComposeState.KIND_DEPOSIT:
                return HudDisclosureVocab.door_meta(HudDisclosureVocab.DOOR_KIND_DEPOSIT, _compose.subject())
            ComposeState.KIND_TILE:
                return HudDisclosureVocab.door_meta(HudDisclosureVocab.DOOR_KIND_TILE,
                    _tile_branch_of_key(_compose.subject()))
        return ""
    if _road_ladder != null and is_instance_valid(_road_ladder) and _road_ladder.visible:
        return HudDisclosureVocab.door_meta(HudDisclosureVocab.DOOR_KIND_ROAD, str(_road_ladder_index))
    return ""

## Open the sheet a door row stands for. **Everything is re-resolved off the LIVE selection at press
## time** (`_live_tile_info` / `_live_deposit`, the retired buttons' own rule), so a door whose row was
## drawn a turn ago opens against this turn's figures. `anchor` is the control a road card hangs under.
func open_door(meta: String, anchor: Control) -> void:
    var parts := HudDisclosureVocab.parse_door_meta(meta)
    if parts.is_empty():
        return
    var kind := String(parts[0])
    var subject := String(parts[1])
    var tile := _selection.tile_info()
    match kind:
        HudDisclosureVocab.DOOR_KIND_FORAGE:
            open_forage_compose(_live_tile_info(subject, tile))
        HudDisclosureVocab.DOOR_KIND_DEPOSIT:
            open_deposit_compose(_live_deposit(subject, {}))
        HudDisclosureVocab.DOOR_KIND_ROAD:
            _open_road_ladder(tile, int(subject), anchor)
        HudDisclosureVocab.DOOR_KIND_TILE:
            open_tile_compose(tile, subject)

## Does the Foraging row open the forage sheet? The retired `Assign harvesters ▸` button's own gate.
func forage_door_open(tile_info: Dictionary) -> bool:
    return _forage_compose_available(tile_info)

## Does a working's row open its deposit sheet? The retired deposit button's own gate.
func deposit_door_open(working: Dictionary) -> bool:
    return _deposit_compose_available(working)

## The forage subject key a door meta carries.
func forage_subject_key(tile_info: Dictionary) -> String:
    return _forage_source_key(tile_info)

## The deposit subject key (`x,y:material`) a door meta carries.
func deposit_subject_key(working: Dictionary) -> String:
    return _deposit_source_key(working)

## What the retired `Assign harvesters ▸` button's SECOND LINE said, as the row's facts:
## `{warn: bool, hover: String}`. `{}` where nobody works the patch. The model is the SAME
## `_standing_summary_model` call, so nothing is re-derived.
func forage_standing_facts(tile_info: Dictionary) -> Dictionary:
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var standing := _standing_assignment(SourceForecast.LABOR_KIND_FORAGE, x, y, "")
    if standing.is_empty():
        return {}
    var crew_label := HudFormat.plant_crew_label(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX)
    return _door_facts_of(_standing_summary_model(standing, SourceForecast.LABOR_KIND_FORAGE,
        crew_label.to_lower()))

## …and the working's (the Wood / Stone row's crew mark and hover). `{}` where nobody cuts it.
func deposit_standing_facts(working: Dictionary) -> Dictionary:
    var tile := HudDepositVocab.tile_of(working)
    var standing := _standing_assignment_extract(tile.x, tile.y, HudDepositVocab.material_of(working))
    if standing.is_empty():
        return {}
    var crew_label := HudDepositVocab.crew_noun(HudDepositVocab.branch_of(working))
    return _door_facts_of(_standing_summary_model(standing, HudConst.LABOR_KIND_EXTRACT,
        crew_label.to_lower(), working))

## A standing-summary model as a door row's facts. **The hover carries EVERY line the retired control
## drew**, in the order it drew them: the summary text, the overstaff / material-short note, the muted
## note, then the readout's own tooltip. The `⚠` rides the row's crew clause (`warn`).
func _door_facts_of(model: Dictionary) -> Dictionary:
    var parts: Array[String] = []
    for key in ["text", "note", "muted_note", "tooltip"]:
        var part := String(model.get(key, "")).strip_edges()
        if part != "":
            parts.append(part)
    return {"warn": bool(model.get("warn", false)), "hover": HudSelectionVocab.DOOR_HOVER_SEPARATOR.join(parts)}

## The HERD drawer's read state: the Extend-pen action (a one-click standing action on a built pen —
## NOT a compose, so it stays here rather than hiding behind a sheet), the standing hunt summary, and
## the `Assign hunters ▸` / `Assign herders ▸` button.
func build_herd_drawer_actions(herd: Dictionary) -> void:
    if _herd_assign_controls == null:
        return
    var corralled := RungQuotes.is_enclosed(herd)
    var available := _herd_compose_available(herd)
    # A penned herd always offers Extend-pen, even if it is no longer huntable — so the container
    # stays visible for a pen OR a composable herd.
    _herd_assign_controls.visible = available or corralled
    if not (available or corralled):
        _clear_herd_drawer()
        return
    var herd_id := String(herd.get("id", ""))
    var noun := _herd_crew_noun(herd)
    var summary_model: Dictionary = {}
    if available:
        var standing := _standing_assignment(SourceForecast.LABOR_KIND_HUNT, -1, -1, herd_id)
        if not standing.is_empty():
            summary_model = _standing_summary_model(standing, SourceForecast.LABOR_KIND_HUNT, noun.to_lower())
    var shape := _herd_actions_shape(herd_id, corralled, available, summary_model)
    # **THE SUMMARY COSTS NO CHILD OF ITS OWN — it is the button's second line now.** It also cannot
    # exist without that button on this web: `summary_model` is only ever filled inside the
    # `available` branch above, so a worked-but-uncomposable herd has no orphan readout to place.
    var expected_children := (1 if corralled else 0) + (1 if available else 0)
    # Same shape (extend kind + summary structure + compose button presence) → patch each part in
    # place, so a per-snapshot restate never tears the herd drawer down.
    if shape == _herd_drawer_shape and _herd_assign_controls.get_child_count() == expected_children:
        if available:
            _update_compose_open_button(_herd_assign_controls.get_child(0) as Control,
                noun, herd_id, summary_model)
        return
    _clear_herd_drawer()
    if available:
        _herd_assign_controls.add_child(_build_compose_open_button(
            noun, herd_id, func() -> void: open_herd_compose(_live_herd(herd_id, herd)),
            summary_model))
    _herd_drawer_shape = shape

## Free the herd drawer-actions and forget its shape, so the next build always rebuilds.
func _clear_herd_drawer() -> void:
    if _herd_assign_controls == null:
        return
    for child in _herd_assign_controls.get_children():
        child.queue_free()
    _herd_drawer_shape = []

# ---- THE TILE CARD'S ROAD DOOR (arc #532 slice 13) -----------------------------------------------
#
# ⛔ **THE UNIT THE PLAYER OPENS IS THE LADDER, NOT THE VERB.** One button per verb does not scale —
# highways and railways are RUNGS, not new controls — and a single verb-named control forces ONE
# refusal string, which cannot answer *"paving is out of reach but railroad is not"*. So the door
# opens the whole branch, one row per rung in climb order, each carrying its own price, its own payoff
# and its own gate. A rung added to `intensification_ladder.json` appears as a row with no client edit.
#
# ⛔ **IT IS NOT SHAPED LIKE FORAGE OR HUNT.** Those open a COMPOSE sheet because they take a worker
# count; `grade` / `pave` take none and a trailing count is a parse error — they DECLARE, and the
# hands come separately from `assign_labor <faction> <band> builders <n>`. So this is a plain card, the
# shape the pen's ring price card takes on the work board.
#
# ⛔ **THE DOOR IS THE `Road` READOUT ROW ITSELF** (`SubjectDrawerController` registers it), one per road
# on the hex, so nothing opens on ground with no road. **The GATING is carried by the rows inside the
# ladder**, each disabled with its own reason, never by hiding the door: a branch a player cannot climb
# today is still a branch they must be able to read.

## The tile-card key the road rows travel under, stamped by `MapView._tile_info_at` off
## `road_tile_lookup`. Spelled once here because a typo is a silently absent door.
const TILE_ROADS_KEY := "roads"

## The road rows on this hex, as typed dicts. `[]` on a tile with no road, which is most of the world.
func _tile_roads(tile_info: Dictionary) -> Array[Dictionary]:
    var roads: Array[Dictionary] = []
    for road_variant in Array(tile_info.get(TILE_ROADS_KEY, [])):
        if road_variant is Dictionary:
            roads.append(road_variant as Dictionary)
    return roads

## **THE LADDER CARD — floated over the dock, never drawn into it.**
##
## **A `PopupPanel` BECAUSE IT IS A WINDOW**, exactly as the work board's destination track is: the
## selection card is height-capped and scrolls internally, so a ladder drawn as a block would push the
## card's own rows out of view on the frame it opened. A Window changes no layout at all.
##
## **THE CARD IS REBUILT PER OPEN, NEVER PATCHED.** The track is a function of the road's rung, its
## meter, the faction's knowledge and which band is acting — all four move per snapshot — so a card
## that survived a render would offer a rung the road has since climbed. The panel NODE is reused
## because a Window is expensive; its CONTENT is not.
func _open_road_ladder(tile_info: Dictionary, index: int, anchor: Control) -> void:
    var roads := _tile_roads(tile_info)
    if index < 0 or index >= roads.size():
        return
    var road: Dictionary = roads[index]
    var ladder := HudRouteVocab.route_ladder(_topbar.route_rungs())
    if ladder.is_empty():
        # **THE CATALOG IS PER WORLD AND ARRIVES WITH THE FIRST SNAPSHOT**, so an empty one is a wire
        # this client has not been sent rather than a branch with nothing on it. A card of no rows
        # would read as *this road can never be raised*, which is a claim about the world.
        push_warning("DrawerComposeController: a Road row was pressed with no route rung catalog on " +
            "the wire — the ladder cannot state a single rung")
        return
    # ⛔ **THE ACTING BAND IS PICKED HERE AND THE PLAYER CAN CHANGE IT.** It was
    # `_resolve_assign_band()` alone — *whichever band the left panel happens to be showing* — so a
    # tile graded while reading Band 3's page became Band 3's job for good, a decision the player never
    # made. That is the same defect `_band_working_source` was written to close for the compose sheet,
    # and roads escaped it because a road has no work row to infer an owner from.
    _road_ladder_tile = tile_info
    _road_ladder_index = index
    _road_ladder_band_entity = int(_default_road_band(road, tile_info).get(
        "entity", ComposeState.NO_BAND_ENTITY))
    var card := _ensure_road_ladder()
    _fill_road_ladder()
    card.popup(_road_ladder_anchor_rect(anchor))
    _note_door_selection()

## **THE CARD'S CONTENTS, REBUILT IN PLACE** — the band row, the track, and the abandon row beneath it.
##
func _fill_road_ladder() -> void:
    var margin := _road_ladder_body
    if margin == null or not is_instance_valid(margin):
        return
    var roads := _tile_roads(_road_ladder_tile)
    if _road_ladder_index < 0 or _road_ladder_index >= roads.size():
        return
    var road: Dictionary = roads[_road_ladder_index]
    var ladder := HudRouteVocab.route_ladder(_topbar.route_rungs())
    var band := _band_labor.player_band_by_entity(_road_ladder_band_entity)
    # ⛔ **THE KEEPER'S NAME IS RESOLVED HERE AND NOWHERE ELSE**, the tile card's own rule one block
    # up: a road carries a `band_id`, this client has exactly one band-naming rule, and the gate
    # layer is stateless and holds no roster. `""` for a band outside the player's roster, which the
    # gate reads as *another people* — a real state, since a road may be kept by a people you merely
    # know of.
    var keeper_label := ""
    if HudRouteVocab.has_keeper(road):
        keeper_label = _band_labor.band_label_for_id(HudRouteVocab.keeper_band_id_of(road))
    # ⛔ **ONE GEAR TERM FOR THE WHOLE CARD WOULD QUOTE THE PAVING KIT'S `2.0` ON A `grade`.** Both
    # road tools declare the route branch and each names its own rung, so a card that asked once and
    # spent the answer on every row would promise an uplift the sim pays on exactly one of them. The
    # rows are the rungs, so the ask is per rung and `route_track` looks its row's answer up by key.
    var builders := SourceForecast.build_crew_staffed(road, SourceForecast.BARE_SOURCE_PREFIX)
    var kit_gear := _road_ladder_kit_gear(band, ladder)
    # **WHETHER THIS ROAD IS ALREADY ORDERED IS A FACTION QUESTION, not the acting band's** — the
    # same set the tile card's `Road` row joins on, so the card and the ladder cannot disagree about
    # one road while the picker names a band that is not its keeper.
    var rows := RungLadder.route_track(road, ladder, _player_knowledge(),
        _topbar.knowledge_labels(), band, keeper_label, builders, kit_gear,
        _band_labor.road_queue_tiles())
    # Resolved from the ROWS rather than re-derived, so the face that says *being built* and the
    # press that acts on it read one answer.
    var building_verb := RungLadder.route_building_verb(rows)
    HudWidgets.clear_children(margin)
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    # **THE BAND ROW LEADS THE CARD**, above `RAISE IT TO…`: who keeps it is decided before which
    # rung, and every row below reads differently once it moves.
    var picker := _build_road_band_picker(band)
    if picker != null:
        column.add_child(picker)
    column.add_child(RungLadder.build_track(rows, func(verb: String) -> void:
        # The press closes the card BEFORE it emits, the destination track's own rule: the declaration
        # re-renders the drawer this card is anchored to.
        _dismiss_road_ladder()
        if verb != building_verb:
            _emit_road_improvement(band, _road_ladder_tile, verb)
        # **THE BAND IS THE `Band:` PICKER'S**, which is the band the entry is being made for and
        # whose pool pays for it — the same rule the offered rung's `Work tab` link follows.
        _navigate_to_work_tab(int(band.get("entity", ComposeState.NO_BAND_ENTITY))),
        HudRouteVocab.ROAD_LADDER_TITLE, RungLadder.max_builders_by_rung(_topbar.ladder_rungs())))
    var abandon := _build_road_abandon_row(road, band)
    if abandon != null:
        column.add_child(abandon)
    margin.add_child(column)

## ⛔ **WHICH BAND THE CARD OPENS ON, AND THE DEFAULT IS THE ONE A PLAYER WOULD HAVE PICKED ANYWAY.**
##
## **THE NEAREST BAND, wrap-aware — and the reason is a PRICE rather than a convenience.** Distance
## multiplies BOTH the build pile and the standing bill (`keeper_remoteness`), so the nearest band is
## also the cheapest keeper this road can have.
##
## ⛔ **EXCEPT WHERE THE ROAD ALREADY HAS A KEEPER IN THE PLAYER'S ROSTER — THEN IT IS THE KEEPER.**
## Re-issuing on a road you already keep is the ordinary case (trail → dirt → paved), and defaulting
## away from the keeper would open the card on a rung the sim refuses outright (*another band keeps
## it*) — a card greying its own live row on the frame it opens.
##
## `{}` where the player holds no bands at all, which the rows' `pick a band` gate then states.
func _default_road_band(road: Dictionary, tile_info: Dictionary) -> Dictionary:
    if HudRouteVocab.has_keeper(road):
        var keeper := _band_labor.player_band_by_band_id(HudRouteVocab.keeper_band_id_of(road))
        if not keeper.is_empty():
            return keeper
    var target_x := int(tile_info.get("x", -1))
    var target_y := int(tile_info.get("y", -1))
    var nearest := {}
    var nearest_distance := SourceForecast.HEX_DISTANCE_UNKNOWN
    for band_variant in _band_labor.current_player_bands():
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        var tile := SourceForecast.band_tile(band)
        var distance := SourceForecast.hex_distance_wrapped(tile.x, tile.y, target_x, target_y,
            _band_labor.grid_width(), _band_labor.wrap_horizontal())
        # **A DISTANCE THE GRID CANNOT ANSWER IS NOT A NEAR ONE.** `hex_distance_wrapped` reports
        # `SourceForecast.HEX_DISTANCE_UNKNOWN` for a band or a tile it cannot place, and taking that as zero would
        # make an unplaceable band the default keeper of every road on the map.
        if distance == SourceForecast.HEX_DISTANCE_UNKNOWN:
            continue
        # **THE FIRST BAND IN ROSTER ORDER WINS A TIE**, so the default is deterministic rather than
        # dependent on which row the walk happened to reach first.
        if nearest.is_empty() or distance < nearest_distance:
            nearest = band
            nearest_distance = distance
    # Nobody placeable: the roster's own first band, which is what every other unplaced-band fall-back
    # in this controller answers, and `{}` only where there is no roster at all.
    if nearest.is_empty():
        var bands := _band_labor.current_player_bands()
        return bands[0] if not bands.is_empty() and bands[0] is Dictionary else {}
    return nearest

## The `Band:` row itself — `_build_band_picker`'s own family, so `Band:` here and `Band:` on the
## compose sheet line their value controls up at one declared key width and cannot drift.
##
## `null` where the player holds no bands: an empty selector states nothing, and the rows' own
## `pick a band` gate is what says so in that state.
func _build_road_band_picker(selected: Dictionary) -> HBoxContainer:
    if _band_labor.current_player_bands().is_empty():
        return null
    return _build_band_picker(selected, func(picked: Dictionary) -> void:
        _road_ladder_band_entity = int(picked.get("entity", ComposeState.NO_BAND_ENTITY))
        _fill_road_ladder())

## ⛔ **THE UNDO FOR A ROAD THAT IS ALREADY SOMEBODY'S JOB**, at the bottom of the card.
##
## **OFFERED ONLY WHERE THE KEEPER IS IN THE PLAYER'S ROSTER.** A road nobody keeps has nothing to put
## down, and a road another people keeps is not yours to drop — in both cases the button would emit a
## command the sim refuses, which is the shape the ladder's own rows exist to avoid.
##
## ⛔ **AND IT NAMES A PLACE, SO IT STATES WHAT ELSE GOES DOWN.** `abandon <faction> <x> <y>` drops the
## faction's labor rows on that tile as well as the road's keeper: the sim's own note says why — a tile
## may carry a road AND a patch, and dropping one without the other would be silently partial on
## exactly the tiles where a band both farms and keeps a road. Where a band of the player's is working
## this hex the row carries a second line saying so; on bare ground it is a plain button.
##
## `null` where there is nothing to put down.
func _build_road_abandon_row(road: Dictionary, band: Dictionary) -> VBoxContainer:
    if not HudRouteVocab.has_keeper(road):
        return null
    var keeper := _band_labor.player_band_by_band_id(HudRouteVocab.keeper_band_id_of(road))
    if keeper.is_empty():
        return null
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    var button := Button.new()
    button.text = HudRouteVocab.ROAD_LADDER_ABANDON_LABEL
    button.tooltip_text = HudRouteVocab.ROAD_LADDER_ABANDON_TOOLTIP
    button.set_meta(HudRouteVocab.ROAD_LADDER_ABANDON_META, true)
    button.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    HudStyle.apply_button(button, "ghost")
    HudWidgets.compact(button, HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE,
        HudWorkVocab.WORK_PAGER_PADDING_V)
    button.pressed.connect(func() -> void:
        # The press closes the card BEFORE it emits, the rung presses' own rule: the drop re-renders
        # the drawer this card is anchored to.
        _dismiss_road_ladder()
        _emit_road_abandon(keeper, _road_ladder_tile))
    column.add_child(button)
    return column

## **PUT THE ROAD DOWN — `abandon <faction> <x> <y>`, built by `Main.format_abandon`.**
##
## ⛔ **NO BAND TOKEN, AND THAT IS THE SIM'S GRAMMAR RATHER THAN AN OMISSION.** `abandon` names a
## faction and a place; it drops every band of that faction's holding there. The band dict is read for
## its FACTION alone, exactly as `_emit_road_improvement` reads it for the faction beside the band.
func _emit_road_abandon(band: Dictionary, tile_info: Dictionary) -> void:
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    emit_signal("road_abandon_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "x": x,
        "y": y,
        # `abandon <f> road <x> <y>` — the bare tile is the patch on that hex (`Main.format_abandon`).
        "road": true,
    })

## **DECLARE A ROUTE RUNG — `grade|pave <faction> <band> <x> <y>`, and the command has not moved.**
##
## ⛔ **THE BAND TOKEN IS THE KEEPER.** A patch's keeper is whoever is already foraging it; a road has
## no work row at all, so who will keep the tile has to be said out loud — and issuing the verb
## declares the job and names the keeper in one act. `Main.IMPROVEMENT_NO_BAND` refuses a payload
## carrying none rather than guessing one, which is why the ladder's rows gate on the acting band.
##
## **IT CARRIES NO `pending_entity`, so no optimistic overlay is written.** That overlay's entries are
## keyed to a band's LABOR ROWS, and a road has none — the declaration lands on the road's own build
## meter, which the next snapshot restates.
func _emit_road_improvement(band: Dictionary, tile_info: Dictionary, verb: String) -> void:
    if verb == SourceForecast.IMPROVEMENT_NONE or band.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    emit_signal("road_improvement_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "improvement": verb,
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "x": x,
        "y": y,
    })

## Take the ladder down, if one is up. Idempotent, and safe before the card has ever been built.
func _dismiss_road_ladder() -> void:
    if _road_ladder != null and is_instance_valid(_road_ladder) and _road_ladder.visible:
        _road_ladder.hide()

## Where the card sits: a zero-height rect just under the pressed action, in SCREEN space (what
## `Popup.popup` wants). `get_screen_transform` folds in the window position and the canvas stretch,
## both of which this HUD has.
##
## **AND THE ANCHOR MUST BE IN THE TREE, not merely alive.** A press can arrive on a control the
## render that answered the previous one has already detached, and `get_screen_transform` on a
## detached `CanvasItem` is an engine ERROR plus an identity transform — a card in the top-left corner
## of the screen with nothing to say it came from this row.
func _road_ladder_anchor_rect(anchor: Control) -> Rect2i:
    if anchor == null or not is_instance_valid(anchor) or not anchor.is_inside_tree():
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + HudWorkVocab.RUNG_TRACK_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

## The ladder's Window, built once and reused. Styled through `HudStyle` like every other card here.
##
## **THE CARD'S MARGIN IS THE CHROME AND IS NEVER FREED — its CHILDREN are.** Clearing the Window's
## own children instead frees the margin the open reaches for, and `queue_free` is deferred, so the
## first open renders correctly and every later one opens onto an empty card.
func _ensure_road_ladder() -> PopupPanel:
    if _road_ladder != null and is_instance_valid(_road_ladder):
        return _road_ladder
    var card := PopupPanel.new()
    card.name = HudRouteVocab.ROAD_LADDER_META
    card.set_meta(HudRouteVocab.ROAD_LADDER_META, true)
    card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in DisclosureController.POPOVER_MARGIN_SIDES:
        margin.add_theme_constant_override("margin_%s" % side, HudWorkVocab.RUNG_TRACK_PADDING)
    card.add_child(margin)
    # The card closing by ANY route (a pick, Esc, a click outside) takes the Road row's selected wash down.
    card.popup_hide.connect(_note_door_selection)
    _host.add_child(card)
    _road_ladder = card
    _road_ladder_body = margin
    return card


# ---- THE TILE CARD'S TILE-BRANCH DOORS (belief's monument, food stores' pit, ...) ----------------
#
# ⛔ **NOTHING HERE NAMES A BRANCH.** One row model per `source_kind == "tile"` branch the catalog carries
# and the card offers, its words (rung, noun, lesson, site term) read off the rung rows by `HudTileVocab`.
# The branch's row on the card is its door: pressing it opens the crew sheet (`open_tile_compose`), whose
# commit is the crew line that also PLACES the improvement. A branch is offered when its first buildable
# rung's lesson is known or the tile already holds an improvement of it, so a fresh game's card carries
# neither; a tile that fails the site term keeps the row as a plain readout and states the reason on its
# hover.

## `12,7:belief` -- one tile improvement's compose subject key. The branch is half of it.
const TILE_BRANCH_SUBJECT_KEY_FORMAT := "%d,%d:%s"

static func _tile_subject_key(tile_info: Dictionary, branch: String) -> String:
    return TILE_BRANCH_SUBJECT_KEY_FORMAT % [
        int(tile_info.get("x", -1)), int(tile_info.get("y", -1)), branch]

static func _tile_branch_of_key(subject_key: String) -> String:
    var at := subject_key.find(":")
    return subject_key.substr(at + 1) if at >= 0 else ""

## The branches this tile's card states, as row models. `[]` on an unseen hex and before the catalog has
## arrived. Each model: `branch`, `row_key` (the card row that states it), `rung_name` (the rung the tile
## stands at), `crew` (every player band's hands on it), `refusal` (`{short, long}` of a site term the
## tile fails, `{}` if it suits) and `door` (the row opens the crew sheet: a band to staff it and a tile
## that suits the branch -- `_tile_compose_available`).
func tile_branch_models(tile_info: Dictionary) -> Array[Dictionary]:
    var models: Array[Dictionary] = []
    if _topbar == null:
        return models
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0 or _selectioncard.tile_contents_unseen(tile_info):
        return models
    var ladder := HudTileVocab.tile_ladder(_topbar.ladder_rungs())
    var knowledge := _player_knowledge()
    for branch in HudTileVocab.branches(ladder):
        var crew := _band_labor.tile_crew_total(x, y, branch)
        var improvement := _band_labor.tile_improvement_at(x, y, branch)
        var holds := _band_labor.tile_rows_held(x, y, branch) > HudTileVocab.CREW_NONE \
            or not improvement.is_empty()
        if not HudTileVocab.is_offered(ladder, branch, knowledge, holds):
            continue
        var entry := HudTileVocab.first_buildable(ladder, branch)
        if entry.is_empty():
            continue
        models.append({
            "branch": branch,
            "row_key": HudTileVocab.readout_row_key(entry, _topbar.ladder_branches()),
            "rung_name": HudTileVocab.standing_rung_name(ladder, branch, improvement, x, y),
            "crew": crew,
            "refusal": HudTileVocab.site_refusal(entry, tile_info),
            "door": _tile_compose_available(tile_info, branch),
        })
    return models

## Can a crew sheet be composed on this tile improvement? A real tile, a band to spend, a branch the
## catalog carries a buildable rung for, and ground that meets the branch's site term.
func _tile_compose_available(tile_info: Dictionary, branch: String) -> bool:
    if tile_info.is_empty() or _topbar == null or _resolve_assign_band().is_empty():
        return false
    if int(tile_info.get("x", -1)) < 0 or int(tile_info.get("y", -1)) < 0 \
            or _selectioncard.tile_contents_unseen(tile_info):
        return false
    var ladder := HudTileVocab.tile_ladder(_topbar.ladder_rungs())
    var entry := HudTileVocab.first_buildable(ladder, branch)
    return not entry.is_empty() and HudTileVocab.site_refusal(entry, tile_info).is_empty()

func open_tile_compose(tile_info: Dictionary, branch: String) -> void:
    if not _tile_compose_available(tile_info, branch):
        return
    _ensure_compose_sheet()
    var subject_key := _tile_subject_key(tile_info, branch)
    _compose.set_composing(ComposeState.KIND_TILE, subject_key)
    var content := _compose_sheet.open(
        HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % HudTileVocab.CREW_SHEET_NOUN,
        HudRouteVocab.branch_name(_topbar.ladder_branches(), branch), subject_key,
        _compose_anchor_rect())
    _build_tile_assign_controls(tile_info, branch, content)
    refresh_drawer_actions()

## **THE TILE CREW SHEET, TOP TO BOTTOM**: the `Band:` picker, the crew stepper, a one-line hint, the
## commit. The count the player dials is the count sent -- no cap is invented here beyond the band's own
## working-age hands. A tile the band already crews edits live (the stepper re-sends) and offers
## `Done` / `Stop working here`.
func _build_tile_assign_controls(tile_info: Dictionary, branch: String, target: VBoxContainer) -> void:
    if target == null:
        return
    for child in target.get_children():
        child.queue_free()
    if not _tile_compose_available(tile_info, branch):
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var subject_key := _tile_subject_key(tile_info, branch)
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.has_tile_row(candidate, x, y, branch))
    var source_changed := _compose.tile_key() != subject_key
    if source_changed:
        _compose.begin_tile_source(subject_key,
            int(resolved.get("entity", ComposeState.NO_BAND_ENTITY)))
    var band := _band_labor.player_band_by_entity(_compose.tile_band())
    if band.is_empty():
        band = resolved
        _compose.set_tile_band(int(band.get("entity", ComposeState.NO_BAND_ENTITY)))
    var band_entity := int(band.get("entity", ComposeState.NO_BAND_ENTITY))
    if source_changed or _compose.tile_seeded_band() != band_entity:
        # Seeded from THIS band's own row (its Max, else the crew the sim struck), else a crew of one.
        var standing := _band_labor.tile_assignment_of(band, x, y, branch)
        var seed := int(standing.get("max_workers", 0))
        if seed <= 0:
            seed = _band_labor.effective_tile_workers(band, x, y, branch)
        _compose.seed_tile(maxi(seed, HudTileVocab.CREW_DEFAULT))
    var is_working := _band_labor.has_tile_row(band, x, y, branch)
    var hands := maxi(int(band.get("working_age", 0)), 0)
    _compose.clamp_tile_max(hands, HudTileVocab.CREW_DEFAULT)
    var rebuild := func() -> void:
        _build_tile_assign_controls(_selection.tile_info(), branch, target)
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _compose.set_tile_band(int(picked.get("entity", ComposeState.NO_BAND_ENTITY)))
        rebuild.call()))
    # **THE WORK STRIP's LADDER CHIP** -- the deposit and forage sheets' own control, built from the Work
    # tab's tile row (`tile_strip_row`), so Monument / Storage pit are one press from the sheet. It draws
    # on every state, a band not yet crewing the tile included: the rung card names the missing crew.
    _mount_tile_strip(target, band, x, y, branch)
    var max_row := HBoxContainer.new()
    max_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    max_row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    max_row.set_meta(HudTileVocab.CREW_ROW_META, subject_key)
    max_row.add_child(HudWidgets.build_field_key(HudTileVocab.CREW_STEPPER_LABEL))
    HudWidgets.add_max_stepper(max_row, _compose.tile_max(), hands, func(n: int) -> void:
        _compose.set_tile_max(n)
        if is_working:
            _send_tile_assignment(band, x, y, branch)
        rebuild.call(), false, HudTileVocab.CREW_DEFAULT)
    target.add_child(max_row)
    target.add_child(HudWidgets.alloc_hint_label(HudTileVocab.CREW_HINT))
    var commit := Button.new()
    commit.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    commit.text = HudComposeVocab.STOP_WORKING_BUTTON if is_working else HudTileVocab.COMMIT_LABEL
    commit.disabled = hands < HudTileVocab.CREW_DEFAULT
    HudStyle.apply_button(commit, "primary")
    commit.pressed.connect(func() -> void:
        if is_working:
            _emit_tile_stop(band, x, y, branch)
        else:
            _send_tile_assignment(band, x, y, branch)
        close_compose_sheet())
    _mount_commit_row(target, commit, is_working)

## The tile crew sheet's strip, mounted exactly as `_mount_work_strip` mounts a source's: the strip model
## off the Work tab's row producer, its handlers the board's own (the chip opens `open_tile_track`).
func _mount_tile_strip(parent: VBoxContainer, band: Dictionary, x: int, y: int, branch: String) -> void:
    if _bandpanel == null:
        return
    var row := _bandpanel.tile_strip_row(band, x, y, branch)
    if row.is_empty():
        return
    var strip := WorkStrip.build(WorkStrip.model_from_row(row), _bandpanel.strip_handlers(band, row), true)
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.set_meta(HudTileVocab.STRIP_META, "%d,%d:%s" % [x, y, branch])
    parent.add_child(strip)

## `assign_labor <f> <band> tile <branch> <x> <y> <n>` -- the crew line, which also places the improvement.
func _send_tile_assignment(band: Dictionary, x: int, y: int, branch: String) -> void:
    _note_applied()
    _emit_assign_labor(band, HudConst.LABOR_KIND_TILE, _compose.tile_max(), x, y, "",
        SourceForecast.FLOOR_UNNAMED, branch)

## `stop_labor <f> <band> <branch> <x> <y>` -- `Main.site_address` leads a tile improvement with its branch.
func _emit_tile_stop(band: Dictionary, x: int, y: int, branch: String) -> void:
    stop_labor_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "kind": HudConst.LABOR_KIND_TILE,
        "x": x, "y": y, "herd_id": "", "material": "", "branch": branch,
        # Not a command token: `Hud._on_stop_labor_requested` drops THIS row's pending assign by it
        # (key rebuilt with the branch in the material slot, as `_emit_assign_labor` filed it).
        "pending_entity": int(band.get("entity", -1)),
    })


# ---- THE TILE CARD'S DEPOSIT DOORS (issue #650) -------------------------------------------------
#
# ⛔ **RETIRED — THE `Workings ▸` POPUP, ITS CARD AND EVERY BUILDER UNDER IT** (`_open_workings_card`
# / `_fill_workings_card` / `_build_workings_block` / `_workings_row` / `_build_workings_crew_row` /
# `_build_workings_band_picker` / `_ensure_workings_card` / `_dismiss_workings_card`, and the
# `WORKINGS_*` metas and label). It was a FOURTH UX pattern on a branch whose three surfaces already
# ship: one action opening a Window that was a readout, a stepper and a bill at once, on a card the
# player reaches tile-first.
#
# **WHAT REPLACED IT IS THE THREE SURFACES THIS CLIENT ALREADY HAS**, one per question:
#   • *what is on this ground* — a `Key: value` ROW per material on the tile card itself
#     (`HudDepositVocab.deposit_lines`, rendered by `SubjectDrawerController._tile_terrain_lines`);
#   • *put a crew on it* — the two COMPOSE SHEETS below, the forage sheet's own spine;
#   • *take it up a rung* — the shared `RungLadder` TRACK, on the Work board where `cultivate` and
#     `sow` are pressed.
#
# ⛔ **ONE SHEET PER *WORKING*, AND THE DOOR IS THE WORKING'S OWN ROW.** The registry key is
# `(tile, material)`: a wooded highland carries timber AND rock, and putting foresters on the timber
# is not putting diggers on the rock. So each material's row on the tile card is a door to a sheet
# about ONE working. A single door for the pair is exactly the tile-keyed collapse the `material`
# field exists to prevent.
#
# ⛔ **THE WORD "QUARRY" APPEARS ON NOTHING HERE.** `Quarry` is ONE RUNG of ONE branch, and a sheet
# that opens on any working of either would be naming a coppice after a pit — so the crew nouns are
# `Foresters` and `Diggers` and the material names the thing being worked. `quarrywork` was the
# retired keeping pool's token.

## The tile-card key the working rows travel under, stamped by `MapView._tile_info_at` off
## `deposit_tile_lookup`. Spelled once here because a typo is a silently absent action.
const TILE_DEPOSITS_KEY := "deposits"

## `12,7:stone` — ONE working's compose subject key. **The material is half of it**, for the reason
## every join in this arc carries both halves: a tile-only key would let a hex's Wood sheet and its
## Stone sheet overwrite each other's composition.
const DEPOSIT_SUBJECT_KEY_FORMAT := "%d,%d:%s"

static func _deposit_source_key(deposit: Dictionary) -> String:
    var tile := HudDepositVocab.tile_of(deposit)
    return DEPOSIT_SUBJECT_KEY_FORMAT % [tile.x, tile.y, HudDepositVocab.material_of(deposit)]

## The deposit rows on this hex, as typed dicts. **`[]` MEANS THIS GROUND HOLDS NOTHING** (or the
## faction has never seen it): the section publishes a row for every discovered tile that holds a
## deposit and merges the live working in where a band has opened one, so untouched ground is a row
## like any other — `HudDepositVocab.is_unopened` is what tells the two apart, never the absence.
func _tile_workings(tile_info: Dictionary) -> Array[Dictionary]:
    var workings: Array[Dictionary] = []
    for entry in Array(tile_info.get(TILE_DEPOSITS_KEY, [])):
        if entry is Dictionary:
            workings.append(entry as Dictionary)
    return workings

## …filtered to ONE branch, which is what makes the two containers two containers.
func _tile_workings_of_branch(tile_info: Dictionary, branch: String) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    for working in _tile_workings(tile_info):
        if HudDepositVocab.branch_of(working) == branch:
            rows.append(working)
    return rows

## The player faction's standing `extract` assignment on ONE working — `{}` when nobody works it.
## `_standing_assignment`'s twin, and it is a twin rather than a fourth arm of that function because
## the join is on the `(tile, material)` PAIR: a tile-keyed lookup would answer the Wood row for a
## Stone sheet on the same hex.
func _standing_assignment_extract(x: int, y: int, material: String) -> Dictionary:
    var bands: Array = _band_labor.player_bands() if not _band_labor.player_bands().is_empty() \
        else [_band_labor.player_band()]
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var found := _band_labor.extract_assignment_of(band_variant as Dictionary, x, y, material)
        if not found.is_empty():
            return found
    return {}

## The LIVE row for a working the drawer is showing, re-resolved through the selection's own tile so
## a press acts on this turn's figures — `_live_tile_info`'s rule, one subject in.
func _live_deposit(subject_key: String, fallback: Dictionary) -> Dictionary:
    for working in _tile_workings(_selection.tile_info()):
        if _deposit_source_key(working) == subject_key:
            return working
    return fallback

## Can a sheet be composed on this working at all? — a real row, a branch this client has a crew noun
## for, and a band to spend. **A branch with no noun renders NO action** rather than an unnamed one.
func _deposit_compose_available(deposit: Dictionary) -> bool:
    if deposit.is_empty():
        return false
    if HudDepositVocab.tile_of(deposit).x < 0 or HudDepositVocab.material_of(deposit) == "":
        return false
    return HudDepositVocab.crew_noun(HudDepositVocab.branch_of(deposit)) != ""

## **OPEN THE SHEET ON ONE WORKING.** The eyebrow names the crew this branch staffs
## (`ASSIGN FORESTERS` / `ASSIGN DIGGERS`) and the TITLE beside it is the ground's own terrain label —
## the slot the food-module label fills for a forage patch. **The material is not repeated there**:
## the crew noun already says which of the hex's two workings this sheet is about.
func open_deposit_compose(deposit: Dictionary) -> void:
    if not _deposit_compose_available(deposit):
        return
    _ensure_compose_sheet()
    var subject_key := _deposit_source_key(deposit)
    _compose.set_composing(ComposeState.KIND_DEPOSIT, subject_key)
    var branch := HudDepositVocab.branch_of(deposit)
    var content := _compose_sheet.open(
        HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT
            % HudDepositVocab.crew_noun(branch).to_lower(),
        String(_selection.tile_info().get("terrain_label", "")).strip_edges(),
        subject_key, _compose_anchor_rect())
    _build_deposit_assign_controls(deposit, content)
    refresh_drawer_actions()

## **THE SHEET, TOP TO BOTTOM** — the forage sheet's spine, with the elements a working has no concept
## for simply ABSENT and the escapement dial offered on the half of them that grows back:
##
##   1. the `Band:` picker, unchanged;
##   2. **the floor presets over the draggable chart — ONLY WHERE THE GROUND GROWS BACK**
##      (`HudDepositVocab.renews`). Rock does not come back, so *leave half the seam* on a quarry
##      means never getting half the seam; a finite sheet keeps exactly the shape it had, no presets
##      and no chart, and the chart IS the dial so there is nothing to draw beside them. **The sim
##      does not fork here** — every `extract` row carries a floor — so this fork is the client's, and
##      it is the same `regrowth_rate > 0` one every other surface in this arc takes;
##   3. the crew row, capped at what the working can actually use;
##   4. the `Kit` row, with the crew handed on;
##   5. ⛔ **NO SPECIES CHIPS** — a deposit takes one material by construction;
##   6. the improvement POINTER line, whose `Work tab` is a live link;
##   7. the readout box;
##   8. the commit button.
func _build_deposit_assign_controls(deposit: Dictionary, target: VBoxContainer) -> void:
    if target == null:
        return
    # A rebuild frees whatever the sheet held, so any drag on it is over — the forage builder's rule.
    _end_floor_drag()
    for child in target.get_children():
        child.queue_free()
    if not _deposit_compose_available(deposit):
        return
    var tile := HudDepositVocab.tile_of(deposit)
    var material := HudDepositVocab.material_of(deposit)
    var branch := HudDepositVocab.branch_of(deposit)
    var crew_label := HudDepositVocab.crew_noun(branch)
    var subject_key := _deposit_source_key(deposit)
    # The band the sheet DEFAULTS to: whoever already cuts this working, else the shared ladder's
    # answer. `_band_working_source`'s own contract, asked through the `(tile, material)` pair.
    var resolved := _band_working_source(func(candidate: Dictionary) -> bool:
        return _band_labor.has_extract_row(candidate, tile.x, tile.y, material))
    var source_changed := _compose.deposit_key() != subject_key
    if source_changed:
        _compose.begin_deposit_source(subject_key,
            int(resolved.get("entity", ComposeState.NO_BAND_ENTITY)))
        # **THE COMPOSED KIT IS DROPPED ON A SOURCE CHANGE** (issue #663) — the hunt sheet's
        # `reset_hunt_kit` rule: every render writes the RESOLVED id back, so a `woodcutting` resolved
        # on a wood would read as the player's own choice on the next rock and outrank that
        # working's own `default_kit_id`.
        _compose.set_deposit_kit_id(KitRoster.NO_KIT_ID)
    var band := _band_labor.player_band_by_entity(_compose.deposit_band())
    if band.is_empty():
        band = resolved
        _compose.set_deposit_band(int(band.get("entity", ComposeState.NO_BAND_ENTITY)))
    # **THE ACTOR BAND CHANGING RE-SEEDS THE CREW, exactly as the SOURCE changing does** — the forage
    # sheet's rule and for its reason: a composition is a statement about ONE band's standing row, so
    # the previous band's count surviving a pick turns the commit into an Unassign.
    var band_entity := int(band.get("entity", ComposeState.NO_BAND_ENTITY))
    if source_changed or _compose.deposit_seeded_band() != band_entity:
        # ⛔ **THE FLOOR SEEDS FROM THE ASSIGNMENT, NEVER FROM `DepositState.floor`** — `labor-ui.md`'s
        # crop rule, and for its reason. That wire field is the SOURCE's reading, kept at the deepest
        # floor ANY band cutting this working named, so a sheet seeded from it silently adopts another
        # band's order; and on ground nobody has opened it reads `0`, which is *strip it bare*.
        _compose.seed_deposit(_band_labor.effective_extract_max(band, tile.x, tile.y, material),
            _band_labor.floor_for_extract(band, tile.x, tile.y, material))
        _compose.set_deposit_priority(HudWorkVocab.work_priority_of(
            _band_labor.extract_assignment_of(band, tile.x, tile.y, material).get("priority", "")))
        # **AND THE KIT SEEDS FROM THE ROW** — see `_standing_kit_id`.
        _compose.set_deposit_kit_id(_standing_kit_id(band, _band_labor.pending_key(
            HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, "", material)))
    var current := _band_labor.effective_extract_workers(band, tile.x, tile.y, material)
    var working_row := _band_labor.extract_assignment_of(band, tile.x, tile.y, material)
    var is_working := _band_labor.has_extract_row(band, tile.x, tile.y, material)
    var crew_pool := _band_labor.source_crew_pool_extract(band, tile.x, tile.y, material)
    target.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _compose.set_deposit_band(int(picked.get("entity", ComposeState.NO_BAND_ENTITY)))
        _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target)))
    # **A BAND WITH A LEADER HAS AN ORDER FOR THIS MATERIAL** (#860): the chief's version of the sheet
    # when it is his, today's sheet under an offer strip when it is the player's.
    var sheet_title := String(_selection.tile_info().get("terrain_label", "")).strip_edges()
    var material_order := HudOrdersVocab.order_of(band, material)
    if _orders != null and not material_order.is_empty():
        var orders_ctx := _orders_ctx(band, func() -> void:
            _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target))
        if HudOrdersVocab.is_chief_owned(material_order):
            _compose_sheet.set_header(material.capitalize(), sheet_title)
            var stock_text := (HudOrdersVocab.STOCK_RENEWS_FORMAT if HudDepositVocab.renews(deposit) \
                else HudOrdersVocab.STOCK_FINITE_FORMAT) % [int(HudDepositVocab.stock_of(deposit)),
                int(HudDepositVocab.capacity_of(deposit)), material, material.capitalize()]
            BandOrdersSheets.build_material_chief(target, orders_ctx, material_order, material, tile,
                stock_text)
            return
        target.add_child(BandOrdersSheets.material_offer(orders_ctx, material))
    _compose_sheet.set_header(HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT % crew_label.to_lower(),
        sheet_title)
    # **THE KIT IS RESOLVED UP HERE AND MOUNTED BELOW** — the crew curve is asked AT this kit, and the
    # floor chart's drag re-asks it, so the id has to exist before either. See the kit row below for
    # the picker's own rules (issue #663).
    #
    # ⛔ **THE PICKER OFFERS THIS WORKING'S `offered_kit_ids` AND `none`, AND NOTHING ELSE.** The sim
    # publishes the list per row (the Sled kit and the branch's own kit — `woodcutting` beside it on a
    # wood, `stonework` on stone), so the other branch's kit, whose tool does nothing here, is never
    # offered; `none` rides beside the list as the job's fallback.
    var kits := KitRoster.extract_kits_for_working(_band_labor.kits(), deposit)
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_EXTRACT)
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_EXTRACT, default_kit,
        _compose.deposit_kit_id(), deposit)
    _compose.set_deposit_kit_id(kit_id)
    var ladder := HudDepositVocab.deposit_ladder(
        _topbar.deposit_rungs() if _topbar != null else [])
    var next_entry := HudDepositVocab.ladder_next_entry(ladder, deposit)
    # ⛔ **THE DIAL IS OFFERED ONLY WHERE THE GROUND GROWS BACK.** `renews` is this arc's one fork and
    # every surface in it takes the same one; the sim deliberately does not fork, so a finite working
    # carries a floor on its row and is simply never asked for one here.
    var offers_floor := HudDepositVocab.renews(deposit)
    var floor_value := _compose.deposit_floor() if offers_floor \
        else SourceForecast.DEFAULT_HARVEST_FLOOR
    # ⛔ **THE FLOOR THE COMMAND CARRIES IS A DIFFERENT ANSWER FROM THE ONE THIS SHEET COMPOSES AT**
    # (issue #650). Every reading above is struck at `floor_value`, which on a finite working is the
    # food peak standing in for a dial nobody was offered — inert, because `composed_floor` discards a
    # crew floor on ground that never renews. What may NOT be inert is the wire: sending the default
    # makes `handle_assign_labor` see a floor the player NAMED, so `unnamed_deposit_floor` — the whole
    # sim-side fork for this case — never fires and the row stores a conservation choice nobody made.
    # `FLOOR_UNNAMED` is the sentinel for *the sheet offered no dial*, and `offers_floor` is the ONE
    # reading of that fact on this sheet.
    var named_floor := _compose.deposit_floor() if offers_floor else SourceForecast.FLOOR_UNNAMED
    # ⛔ **THE CREW CURVE — THE ONE SOURCE OF EVERY GEAR-BEARING FIGURE ON THIS SHEET** (issue #663).
    # NEXT TURN, `N of M <kit> available` and the `once felled` deal are all READ off this reply at the
    # stepper's crew; none is composed here. It is asked at the floor the COMMAND would store — the dial
    # on a renewing working, `FLOOR_MIN` on a finite one, where an unnamed floor is stored as *strip it
    # bare* — and at the band's whole pool, so one round trip answers every stepper position.
    var ask_floor := _compose.deposit_floor() if offers_floor else SourceForecast.FLOOR_MIN
    # **A WORKING THE BAND DOES NOT HOLD YET IS PRICED OFF THE SIM'S DRY RUN** (the hunt sheet's rule).
    # A never-renewing dig is worked by exactly its Max in heads, so the Max is raised to the dig floor
    # BEFORE the ask - the sim refuses a dig without a count.
    _compose.clamp_deposit_max(maxi(int(band.get("working_age", 0)), 0))
    if not offers_floor:
        _compose.raise_deposit_max_to(HudDepositVocab.DIG_CREW_MIN)
    var preview_view: Dictionary = {}
    if not is_working:
        preview_view = _staffing_preview_view(band, {
            "source_kind": ForecastQuery.WORK_PARTY_SOURCE_EXTRACT, "x": tile.x, "y": tile.y,
            "material": material}, "extract:" + _deposit_source_key(deposit), kit_id, ask_floor,
            _compose.deposit_priority(), _compose.deposit_max())
    var curve_pool := _preview_curve_pool(crew_pool, is_working, preview_view)
    _deposit_live_crew_view = _deposit_crew_take_view(band, deposit, kit_id, ask_floor, curve_pool)
    var curve_view := _deposit_live_crew_view
    var curve_ready := String(curve_view.get("state", ForecastQuery.STATE_PENDING)) \
        == ForecastQuery.STATE_READY
    var curve_reply: Dictionary = curve_view.get("answer", {})
    # ⛔ **THE CURVE'S `in_range` IS A FLAG AND NOTHING HERE READS IT.** A far working's reply carries
    # the real take at the source (the crew posts a work party rather than being dropped), so no
    # reading on this sheet is hidden, zeroed or disabled by distance.
    # ⛔ **THE CAP IS THE SMALLER OF THE BAND'S HANDS AND WHAT THE WORKING CAN USE, AND IT IS RESOLVED
    # BEFORE THE CHART — the order is load-bearing** (the forage sheet's own finding). A crew takes
    # `min(crew × rate, the room above the composed floor)` in a turn, so a hand beyond that quotient
    # carries nothing home; the chart, both crew targets and the verdict are then all read against a
    # count the stepper below will not clamp away.
    var cap := crew_pool
    # ⛔ **THE "USEFUL" CAP IS THE CURVE'S PLATEAU** (issue #663) — where more hands stop buying take,
    # off the rows the sim priced WITH the band's kit. A bare quotient would divide the room by the
    # bare per-worker rate, which a geared crew out-cuts; with no curve in hand (pending or refused)
    # there is no gear-bearing answer and the pool alone bounds the stepper.
    var useful := HudDepositVocab.CUTTERS_UNCAPPED
    if curve_ready:
        useful = HudDepositVocab.curve_useful_cutters(curve_reply)
    var capped_by_seam := useful != HudDepositVocab.CUTTERS_UNCAPPED and useful < cap
    if capped_by_seam:
        cap = maxi(useful, 0)
    var seam_note := HudDepositVocab.CUTTERS_CAP_NOTE_FORMAT % [cap, crew_label.to_lower()] \
        if capped_by_seam else ""
    # **THE MAX IS CLAMPED TO THE BAND'S WORKING-AGE PEOPLE** (#871), and the crew is the sim's: the
    # snapshot row's `workers` on a working the band holds, else the curve's useful cutters held to the Max.
    _compose.clamp_deposit_max(maxi(int(band.get("working_age", 0)), 0))
    # ⛔ **A SEAM THAT NEVER RENEWS HAS NO `No max`** - the sim works it with exactly its Max in heads, so
    # a Max of 0 staffs nobody. The floor is raised AFTER the clamp (a band of no working-age people still
    # shows 1).
    var is_dig_crew := not offers_floor
    if is_dig_crew:
        _compose.raise_deposit_max_to(HudDepositVocab.DIG_CREW_MIN)
    var staffed_crew := _previewed_crew(is_working, preview_view,
        _staffed_crew_of(is_working, current, cap, _compose.deposit_max()))
    var live_hosts: Array[Dictionary] = []
    # **THE FLOOR PRESETS, THROUGH THE SHARED PICKER**, with each preset's own per-turn take on its
    # hover — the same builder and the same metric shape the forage sheet's picker wears.
    if offers_floor:
        target.add_child(HudWidgets.build_floor_picker(func(picked: float) -> void:
            _compose.set_deposit_floor(picked)
            if is_working:
                _send_deposit_assignment(band, deposit, tile, material)
            _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target),
            floor_value, _deposit_floor_takes(deposit),
            HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS))
    # **THE CHART — the SHARED instrument, fed a working through `HudDepositVocab.forecast_source`.**
    # A deposit's sampled curve never goes negative (there is no Allee term), so its projection has the
    # patch's shape rather than the herd's; that asymmetry comes off the wire, not from here.
    var chart_model := _deposit_chart_model(deposit, ladder, floor_value, staffed_crew) \
        if offers_floor else {}
    _last_chart_model = chart_model
    if bool(chart_model.get("known", false)):
        # A live drag may NOT rebuild the sheet — freeing the chart mid-drag ends the drag on the
        # first pixel of movement — so every reading that follows the floor is refilled in place.
        _floor_drag_refill = func(dragged: float) -> void:
            # **THE CURVE IS FLOOR-DEPENDENT, SO THE DRAG RE-ASKS IT** — rate-limited, exactly as the
            # hunt sheet's drag does; the refill below then reads whatever the seam holds for the
            # dragged floor, which is PENDING until that answer lands.
            _drag_deposit_crew_take(band, deposit, kit_id, dragged, curve_pool)
            _refresh_floor_live(live_hosts, _deposit_chart_model(
                _live_deposit(subject_key, deposit), ladder, dragged, staffed_crew),
                staffed_crew)
        target.add_child(HudWidgets.build_floor_chart(chart_model,
            func(dragged: float, committed: bool) -> void:
                _compose.set_deposit_floor(dragged)
                if committed:
                    _floor_drag_live = false
                    if is_working:
                        _send_deposit_assignment(band, deposit, tile, material)
                    _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target)
                else:
                    _floor_drag_live = true
                    _floor_drag_refill.call(dragged)))
    # **THE CREW THE SIM WILL STAFF, THEN THE PLAYER'S PRIORITY AND MAX** (#871). The section label is
    # the branch's crew noun and its hover says the crew is the TAKE crew, because a player who staffed it
    # expecting the standing bill to be met would watch the working go back anyway.
    _mount_work_strip(target, band, HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, "", material,
        crew_label, HudDepositVocab.CARD_CREW_HINT, "", staffed_crew, deposit)
    _mount_priority_and_max(target, _compose.deposit_priority(), _compose.deposit_max(),
        int(band.get("working_age", 0)),
        func(level: String) -> void:
            _compose.set_deposit_priority(level)
            if is_working:
                _emit_work_priority(band, HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, "",
                    material, level)
            _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target),
        func(n: int) -> void:
            _compose.set_deposit_max(n)
            if is_working:
                _send_deposit_assignment(band, deposit, tile, material)
            _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target),
        is_dig_crew)
    _mount_staffing_preview(target, is_working, preview_view,
        HudComposeVocab.STAFFING_PREVIEW_VERB_EXTRACT, crew_label, _compose.deposit_priority())
    if capped_by_seam:
        target.add_child(HudWidgets.alloc_hint_label(seam_note))
    # **THE KIT ROW** (issue #663). The roster's `extract` job lists the Woodcutting kit (sled + axe)
    # and the Stone kit (sled + wedges) beside `none`, and the picker offers only this working's own,
    # so the selection moves the take and rides the commit as `kit <id>` — see the commit button below.
    # On a FAR working the sled is also what each porter packs the take home on, which the work
    # party's forecast below prices at the picked kit.
    #
    # ⛔ **THE DEFAULT IS THE WORKING'S OWN FIRST.** Each `deposits` row publishes the kit the rung it
    # holds wants (`default_kit_id` — `sledding` on deadfall and gathering, `woodcutting` on felling
    # and coppice, `stonework` on a quarry), so the deposit
    # is passed as the SOURCE and `KitRoster.default_kit_for` — the one precedence — answers both the
    # opening selection and the `(default)` mark off it, exactly as a herd's does on the hunt sheet;
    # the wire's job-level `defaultExtractKitId` stands behind a working that states none.
    #
    # ⛔ **THE CREW IS HANDED ON**, because omitting it is what made the forage sheet's shortfall line
    # mute for the whole life of that line: `crew` then defaults to `KIT_CREW_UNCOMPOSED` and the
    # shortfall falls back to the published `workersOnQuotedJob`, which is `0` on a sheet where
    # nobody is assigned yet.
    #
    # ⛔ **THE AVAILABLE LINE IS THE HUNT'S AND THE FORAGE'S, UNCHANGED.** An extract row claims its
    # WHOLE kit wherever it works — the Woodcutting kit is a sled AND an axe on every rung — so the line
    # is `KitRoster.shortfall_line`'s ordinary reading: complete outfits (the scarcest of the kit's
    # items) against the crew being composed, with the band's committed row on this working handed in
    # so its published `kitWorkersHolding` answers for the committed crew and its own gear is handed
    # back when it is re-composed.
    _mount_kit_row(target, kits, KitRoster.JOB_EXTRACT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_deposit_kit_id(picked)
            if is_working:
                _send_deposit_assignment(band, deposit, tile, material)
            _build_deposit_assign_controls(_live_deposit(subject_key, deposit), target),
        deposit, "", staffed_crew,
        _band_labor.extract_assignment_of(band, tile.x, tile.y, material))
    var curve_row_at_crew := HudDepositVocab.curve_row(curve_reply, staffed_crew) \
        if curve_ready else {}
    # **THE WORK PARTY, PAST THE APRON** — the hunt and forage sheets' own section, asked of the
    # working's `(tile, material)` pair. A far wood or quarry posts the same caravan as a far patch:
    # nothing about wood or stone is special, so nothing here forks. The floor it is asked at is the
    # one the committed row will carry — the player's dial where one was offered, else the bare-strip
    # floor the sim resolves an unnamed floor to on ground that never renews.
    var party_view := {}
    if _is_past_apron(band, tile.x, tile.y):
        party_view = _work_party_view(band, ForecastQuery.WORK_PARTY_SOURCE_EXTRACT, "",
            tile.x, tile.y, [], kit_id, staffed_crew,
            named_floor if offers_floor else SourceForecast.FLOOR_MIN, material)
        _mount_work_party_section(target, party_view)
    # **A WORKING OFFERS `Stop working here`, A NEW ONE ITS VERB** (#871) - see the forage sheet.
    var is_unassign := is_working
    # **THE READOUT** — what this crew brings home next turn (and what it settles at once the seam
    # reaches the floor), what the next rung would pay once it stands, the verdict the branch turns
    # on, and the runway under the dashed rule.
    _mount_deposit_readout(target, live_hosts, deposit, ladder, next_entry, chart_model,
        staffed_crew, curve_reply, curve_row_at_crew, party_view)
    # **THE LOAD WAITING AT THE SITE** - what the crew has dug that is not yet on the road home.
    var dig_load_line := HudWorkVocab.dig_load_line(working_row, material) if is_working else ""
    if dig_load_line != "":
        var dig_load_label := HudWidgets.alloc_hint_label(dig_load_line)
        dig_load_label.set_meta(HudWorkVocab.DIG_LOAD_LINE_META, dig_load_line)
        target.add_child(dig_load_label)
    # ⛔ **NO RANGE REFUSAL** — the forage sheet's retirement, one web over. A working past the picked
    # band's work range posts a work party (the section above) rather than lapsing its crew, so a
    # refusal would forbid the very assignment the caravan exists to make.
    # A dead button is always explained, the `+` stepper's cap note being the precedent.
    var assign_btn := Button.new()
    assign_btn.set_meta(HudWidgets.COMPOSE_COMMIT_META, true)
    assign_btn.text = HudComposeVocab.STOP_WORKING_BUTTON if is_unassign \
        else HudDepositVocab.commit_verb(branch)
    HudStyle.apply_button(assign_btn, "primary")
    # ⛔ **ONE COMMAND, AND IT IS `assign_labor <f> <b> extract <x> <y> <material> [floor] <n>`.** The
    # material rides the `species` token — the slot the sim's own `extract` arm reads it from and half
    # the optimistic overlay's key — and the FLOOR is a validated number in forage's own position and
    # forage's own decimal precision, the stance words being refused by name at parse.
    #
    # **A FINITE WORKING SENDS NO FLOOR AT ALL, because it was never asked** — `named_floor` is
    # `FLOOR_UNNAMED` there and `Main`'s extract arm drops the token, leaving the sim to answer what
    # silence means on ground that never renews. A renewing working rides the player's own dial.
    #
    # **THE KIT RIDES IT** (issue #663): the picker above offers the working's own take kit and
    # `none`, and a selection the line dropped would be a choice the sim never heard.
    # `Main._kit_token` renders it, omitted only where it equals the working's own default.
    assign_btn.pressed.connect(func() -> void:
        if is_unassign:
            _emit_stop_labor(band, HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, "", material)
            close_compose_sheet()
            return
        _send_deposit_assignment(band, deposit, tile, material)
        if _compose.deposit_priority() != HudWorkVocab.WORK_PRIORITY_NORMAL:
            _emit_work_priority(band, HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, "", material,
                _compose.deposit_priority())
        close_compose_sheet())
    _mount_commit_row(target, assign_btn, is_unassign, func() -> void:
        _bandpanel.show_source_on_map(HudConst.LABOR_KIND_EXTRACT, tile.x, tile.y, ""))


## **THE WORKING'S CHART MODEL — the SHARED instrument, composed ONCE per render and once per drag.**
##
## ⛔ **THE FLOOR IT IS COMPOSED AT IS `HudDepositVocab.composed_floor`, and that is the whole of the
## `max` rule.** The rung's own floor and the crew's are the same kind of quantity — an amount left
## standing — so the crew stops at whichever is greater, and the composition happens HERE, once,
## before anything is projected. Everything downstream reads the model's own `floor`
## (`_live_floor`), so no second call site can compose the pair differently: added, they would
## double-count on every rung and draw a gathering crew stopping 85% of a seam short.
##
## ⛔ **THE TEACHING LINE IS THE ONE READING TAKEN AT THE PLAYER'S FLOOR RATHER THAN THE COMPOSED
## ONE**, because that is the floor the SIM prices the lesson at (`systems::labor`'s `Extract` arm
## passes the ROW's own floor to `intensification::learn_multiplier`, while the rung's floor reaches
## only the workability predicate through `reachable_before`). A line composed at the max would
## promise a gathering crew ×1.70 for a dial they set to zero.
func _deposit_chart_model(deposit: Dictionary, ladder: Array[Dictionary], floor: float,
        crew: int) -> Dictionary:
    var labels: Dictionary = _topbar.knowledge_labels() if _topbar != null else {}
    var lesson_known := HudDepositVocab.standing_lesson_known(deposit, ladder, _player_knowledge())
    var model := SourceForecast.floor_chart_model(HudDepositVocab.forecast_source(deposit),
        SourceForecast.SOURCE_KIND_DEPOSIT, HudComposeVocab.BARE_FORECAST_PREFIX,
        HudDepositVocab.composed_floor(deposit, floor), crew, lesson_known)
    if not bool(model.get("known", false)):
        return model
    model["teaching_note"] = SourceForecast.teaching_note(
        HudDepositVocab.standing_lesson(deposit, ladder, labels),
        SourceForecast.clamp_floor(floor),
        SourceForecast.crew_is_taking_next_turn(crew,
            HudDepositVocab.room_next_turn(deposit, floor)), lesson_known)
    # ⛔ **EVERY CREW-DRAW READING IS THE CURVE'S** (issue #663) — the walk, both pills and the verdict
    # are recomposed off `_deposit_live_crew_view`, which is the committed floor's answer on a build
    # and the dragged floor's (or PENDING) on a drag. With no curve they are WITHHELD, never drawn at
    # the bare `perWorkerBiomass × crew` this model was composed at.
    return HudDepositVocab.curve_chart_model(model, deposit, crew, _deposit_live_crew_view)

## **PER-PRESET TAKES FOR THE PICKER'S HOVERS** — what the seam offers above each preset's floor, in
## the working's own material. The forage picker's metric shape (`extractive_take_pair`), asked of a
## working: the metric is the ROOM that floor frees, which is takeable once, exactly as it is there.
func _deposit_floor_takes(deposit: Dictionary) -> Dictionary:
    var takes := {}
    var material := HudDepositVocab.material_of(deposit)
    if material == HudDepositVocab.MATERIAL_NONE:
        return takes
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        takes[preset] = SourceForecast.extractive_take_pair(0.0, 0.0,
            SourceForecast.YIELD_ACCOUNT_NONE, [{
                SourceForecast.MATERIAL_PAYOFF_ID_KEY: material,
                SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY: HudDepositVocab.room_next_turn(
                    deposit, SourceForecast.floor_for_preset(preset)),
            }])
    return takes

## **WHAT THIS CREW CARRIES HOME NEXT TURN, AND WHAT IT SETTLES AT** — the deposit's answer in the
## shared `YIELD_MODEL_*` shape, so the readout's first register is filled by `_fill_yields_host`
## exactly as both food webs fill it.
##
## **THE HEADLINE IS THE ROOM NEXT TURN, not the room standing now** — the sim regrows a whole stage
## before it takes, so a seam held at its floor pays its regrowth while the standing room is empty.
## `HudDepositVocab.room_next_turn` is that projection's first turn, and on a finite working it is
## `reachable` by arithmetic.
##
## **THE `after` READING IS WHAT THE FLOOR MAKES POSSIBLE** — the take once the seam settles at the
## floor, which is the curve's own value there, capped by what the crew can lift. It is attached only
## where the walk actually REACHES the floor (`reaches`, the caller's `_live_reaches`), because
## promising a holding rate to a crew that settles short is the defect that reading exists to fix.
func _deposit_yield_model(deposit: Dictionary, floor: float, crew: int,
        reaches: bool, view: Dictionary) -> Dictionary:
    var material := HudDepositVocab.material_of(deposit)
    # ⛔ **NEXT TURN IS THE CREW CURVE'S `take` AT THIS CREW, AND NOTHING ELSE** (issue #663). It was
    # `perWorkerBiomass × crew` capped by the room, which is the bare rate: a deadfall crew on sleds
    # read the same figure as one without. Not answered → no take at all, the hunt sheet's rule: the
    # sheet says it is waiting, or why it was refused, where the number would be.
    var state := String(view.get("state", ForecastQuery.STATE_PENDING))
    if state == ForecastQuery.STATE_PENDING:
        return {YIELD_MODEL_ROWS: [], YIELD_MODEL_NOTES: [HudDepositVocab.DEPOSIT_TAKE_PENDING]}
    if state == ForecastQuery.STATE_FAILED:
        return {YIELD_MODEL_ROWS: [], YIELD_MODEL_NOTES: [
            HudDepositVocab.take_failed_text(String(view.get("error", "")))]}
    var reply: Dictionary = view.get("answer", {})
    var take := HudDepositVocab.curve_take_at(reply, crew)
    var after := {}
    if reaches:
        # The steady take: what the ground puts back in one turn AT the floor, which is the curve read
        # at the floor's own stock fraction — the same interpolation the projection walks on — capped
        # by what the crew can lift, which is the curve row's take (see
        # `HudDepositVocab.curve_chart_model` for why a room-capped row is still a safe lift).
        after[material] = minf(take, SourceForecast.regrowth_at(
            HudDepositVocab.regrowth_samples_of(deposit),
            HudDepositVocab.composed_floor(deposit, floor)))
    var rows := SourceForecast.yield_rows(0.0, 0.0, SourceForecast.YIELD_ACCOUNT_NONE, after, [{
        SourceForecast.MATERIAL_PAYOFF_ID_KEY: material,
        SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY: take,
    }])
    return {
        YIELD_MODEL_ROWS: rows,
        YIELD_MODEL_TEXT: "",
        # ⛔ **THE `renewable` NOTE IS THE `regrowth_rate > 0` FORK, HERE AS EVERYWHERE ELSE IN THIS
        # ARC.** A quarry renews nothing, so it states neither this note nor the overdraw flag beside
        # it — the runway under the dashed rule is what a finite seam warns with instead.
        YIELD_MODEL_RENEWS: HudDepositVocab.renews(deposit),
        # ⛔ **THE OVER-CUT FLAG IS `actual > sustainable` AND ON THIS BRANCH THAT IS CORRECT** — a
        # deposit take is a RATE with no whole-body lump in it, and there is no `overdraws` flag on
        # the row to read instead. The take is the ASSIGNMENT's, so a crew committed this turn is
        # judged on what it will cut rather than on the zero the working still publishes.
        YIELD_MODEL_OVERDRAW: HudDepositVocab.renews(deposit)
            and HudDepositVocab.stated_take(deposit,
                _standing_assignment_extract(HudDepositVocab.tile_of(deposit).x,
                    HudDepositVocab.tile_of(deposit).y, material))
                > HudDepositVocab.sustainable_take_of(deposit),
    }

## **THE DEPOSIT'S YIELDS, AND ON A FAR WORKING BOTH OF ITS FIGURES.** `_with_home_rate` puts the
## caravan's rate ARRIVING HOME on the headline — the forage and hunt sheets' own substitution, in the
## working's material and under `once running · per turn` — and the crew curve's take AT THE WORKING
## (`model`'s own material row, what the crew cuts there each turn) rides one line beneath it
## (`HudDepositVocab.DEPOSIT_TAKE_AT_SOURCE_FORMAT`). The two differ by what the road carries, and the
## sheet states both. Inside the apron `party_view` is `{}`, the substitution is the identity and no
## second line is drawn.
func _fill_deposit_yields(host: Container, model: Dictionary, party_view: Dictionary,
        material: String) -> void:
    var home := _with_home_rate(model, party_view, material)
    _fill_yields_host(host, home, HudConst.LABOR_KIND_EXTRACT)
    if not bool(home.get(YIELD_MODEL_HOME_RATE, false)):
        return
    for row_variant in model.get(YIELD_MODEL_ROWS, []):
        var row: Dictionary = row_variant
        if String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, "")) != material:
            continue
        var line := HudWidgets.alloc_hint_label(HudDepositVocab.DEPOSIT_TAKE_AT_SOURCE_FORMAT % [
            SourceForecast.format_magnitude(float(row.get(SourceForecast.YIELD_ROW_VALUE, 0.0))),
            material])
        line.set_meta(HudDepositVocab.DEPOSIT_TAKE_AT_SOURCE_META, true)
        host.add_child(line)

## **THE READOUT BOX, BUILT DIRECTLY RATHER THAN THROUGH `_mount_readout`, AND STILL.**
##
## ⛔ **THAT MOUNT DROPS THE VERDICT ON A MODEL THAT IS NOT `known`, which is every FINITE seam.** A
## quarry publishes an all-zero curve — a live reading, *this does not grow* — so there is no
## projection to walk and the shared mount would render the box and then silently omit the one
## sentence the whole branch turns on. This assembles the same four shared widgets in the same order
## and the same registers, and takes the verdict from the walk where there IS one and from
## `HudDepositVocab.deposit_verdict` where there is not.
##
## The caption is `next turn` unless a `now → after` pair is on the row, which
## `SourceForecast.yield_row_header` decides from the rows themselves.
func _mount_deposit_readout(target: VBoxContainer, hosts: Array, deposit: Dictionary,
        ladder: Array[Dictionary], next_entry: Dictionary, model: Dictionary, crew: int,
        curve_reply: Dictionary, curve_row_at_crew: Dictionary,
        party_view: Dictionary = {}) -> void:
    var column := HudWidgets.build_readout_box(target)
    var known := bool(model.get("known", false))
    var tile := HudDepositVocab.tile_of(deposit)
    # **PAST THE APRON THE HEADLINE IS THE RATE ARRIVING HOME**, in the working's own MATERIAL —
    # `_with_home_rate`'s substitution, the forage and hunt sheets' own. Inside the apron `party_view`
    # is `{}` and it is the identity.
    var material := HudDepositVocab.material_of(deposit)
    # **THE BAND'S OWN `extract` ROW, resolved once for every reading that needs a rate the working
    # has not paid out yet.** See `HudDepositVocab`'s three-state table.
    var assignment := _standing_assignment_extract(tile.x, tile.y,
        HudDepositVocab.material_of(deposit))
    var yields_host := VBoxContainer.new()
    yields_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    column.add_child(yields_host)
    if known:
        # **THE VIEW IS READ OFF THE LIVE MEMBER, NOT CAPTURED** — a drag re-asks at the dragged
        # floor and writes `_deposit_live_crew_view`, so the refill states that floor's take (or that
        # it is waiting) rather than the committed floor's.
        _register_live(hosts, yields_host, model, crew,
            func(host: Container, live: Dictionary, count: int) -> void:
                _fill_deposit_yields(host, _deposit_yield_model(deposit, _live_floor(live),
                    count, _live_reaches(live), _deposit_live_crew_view), party_view, material))
    else:
        # No dial, no walk, no holding state to promise — the take alone.
        _fill_deposit_yields(yields_host, _deposit_yield_model(deposit,
            SourceForecast.FLOOR_MIN, crew, false, _deposit_live_crew_view), party_view,
            material)
    # **THE DEAL — ITS OWN BLOCK, NEVER A ROW INSIDE THE YIELDS FLOW.** Two harness contracts read
    # that flow structurally, so a deal term folded in would corrupt both silently; it is
    # `HudWidgets.IMPROVEMENT_DEAL_META`'s own block for that reason. It is deliberately OUT of the
    # live registry, the shared mount's own rule: a payoff is a property of the finished rung and
    # nothing in it moves under a floor drag.
    var deal_label := HudDepositVocab.deal_label(next_entry)
    # **THE FIGURE IS THE CREW CURVE'S `next_rung_take` AT THE STEPPER'S CREW** (issue #663) — gear
    # included, and absent while the curve is in flight, past the top of the branch, or out of range
    # (the caller hands an empty row for all three).
    var deal_value := HudDepositVocab.deal_value(next_entry, deposit, curve_reply,
        curve_row_at_crew)
    if deal_label != "" and deal_value != "":
        column.add_child(HudWidgets.build_improvement_deal(deal_label, deal_value))
    # **A CHARTED SHEET STATES NO VERDICT SENTENCE** — the floor chart above the take already draws
    # where the stock goes. A curveless / finite deposit draws no chart, so `deposit_verdict` is the
    # only reach figure it has and is mounted here.
    if not known:
        var verdict_host := VBoxContainer.new()
        verdict_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.add_child(verdict_host)
        verdict_host.add_child(HudWidgets.build_verdict_line(
            HudDepositVocab.deposit_verdict(deposit, ladder, assignment)))
    var aside_host := VBoxContainer.new()
    aside_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    column.add_child(aside_host)
    if known:
        _register_live(hosts, aside_host, model, crew,
            func(host: Container, live: Dictionary, _count: int) -> void:
                var lines: Array[Dictionary] = []
                lines.append(HudWidgets.readout_aside_line(HudFormat.floor_hint(
                    _live_floor(live), HudConst.LABOR_KIND_EXTRACT)))
                # **THE TEACHING RATE, and the one aside line that can be CYAN.** It states what
                # `learn_multiplier` buys, at the floor the SIM prices the lesson at — the dial's own
                # value, not the composed one. An EMPTY note is a rung that teaches nothing at all,
                # which is a reason to render no line rather than a blank one.
                var teaching: Dictionary = live.get("teaching_note", {})
                var teaching_text := String(teaching.get("text", ""))
                if teaching_text != "":
                    lines.append(HudWidgets.readout_aside_line(teaching_text,
                        HudStyle.SIGNAL if bool(teaching.get("teaching", false))
                            else HudStyle.INK_FAINT, HudWidgets.READOUT_TEACHING_META))
                host.add_child(HudWidgets.build_readout_aside(lines)))
    else:
        # **THE RUNWAY, on the branch that has one** — and it honours all three states: the published
        # count where a turn has resolved, this crew's `reachable ÷ rate` where one has been committed
        # and nothing cut yet, and *nobody is cutting it* where there is no crew at all.
        var aside := HudDepositVocab.runway_aside(deposit, assignment)
        if aside != "":
            aside_host.add_child(HudWidgets.build_readout_aside(
                [HudWidgets.readout_aside_line(aside)]))

## The STANDING-SUMMARY child-slot structure shared by both drawers: `[has_summary, warn, has_note,
## has_muted]` — the full set of optional summary child slots, so any structural change (summary
## appearing/disappearing, a warn/note/muted label appearing) moves the signature and forces a rebuild
## rather than a stale positional patch. Each caller PREPENDS its own subject key (the forage tile key /
## the herd id) so a subject change also forces a rebuild — see `_herd_actions_shape`
## (the land card's door rows, `open_door`, replaced the forage drawer's foot buttons).
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
## ⛔ **`extending` LEFT THIS SIGNATURE WITH THE CONTROL IT SWITCHED** (§4.9 item 12c). It existed to
## tell the `Extend pen` BUTTON from the `Fencing N%` BADGE, which are different node kinds and so had
## to force a rebuild rather than a patch; the drawer draws neither now. `corralled` stays — it still
## decides whether a pen exists at all, which the rows below it read.
func _herd_actions_shape(herd_id: String, corralled: bool, available: bool, summary_model: Dictionary) -> Array:
    return [herd_id, corralled, available] + _standing_actions_shape(summary_model)


## Patch the `Assign … ▸` CELL in place: its first line's noun (herders vs hunters can flip as a herd
## is tamed), its second line's standing summary, and the button's primary/ghost lit-while-composing
## state — without freeing anything, so the `pressed` connection stays intact.
##
## ⛔ **THE PARTS ARE FOUND BY META, NEVER BY CHILD INDEX.** `HudWidgets.build_stacked_action_button`
## nests the face three containers deep inside the cell, so a positional read would be asserting
## against that builder's nesting rather than against the parts.
func _update_compose_open_button(cell: Control, noun: String, subject_key: String,
        summary_model: Dictionary) -> void:
    if cell == null:
        return
    var label := _stacked_part(cell, HudWidgets.STACKED_ACTION_LABEL_META) as Label
    if label != null:
        label.text = HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % noun.to_lower()
    var button := _stacked_button(cell)
    if button != null:
        var composing := is_compose_sheet_open() and _compose.subject() == subject_key
        HudStyle.apply_button(button, "primary" if composing else "ghost")
    if summary_model.is_empty():
        return
    var body := _stacked_part(cell, HudWidgets.STACKED_ACTION_BODY_META) as HFlowContainer
    _update_standing_summary(body, summary_model)
    if button != null:
        button.tooltip_text = String(summary_model["tooltip"])
    # ⛔ **RE-MUTE, because `_update_standing_summary` re-runs `set_label_tooltip` on every note it
    # rewrites and that helper sets `MOUSE_FILTER_STOP`.** A face muted only at build time grows a
    # dead patch over the button the first time a note's text moves.
    HudWidgets.mute_button_face(body)

## The `Assign … ▸` control — an `HudWidgets.build_stacked_action_button` CELL whose first line is
## that label and whose second is the source's standing summary, where this faction works it.
##
## It lights "primary" (SIGNAL cyan — this HUD's LIVE state, as on the Sight chip and the selection
## accent) while ITS sheet is the open one, so the drawer shows which source is being composed rather
## than looking idle behind the sheet; "ghost" at rest. NOT "armed" — that is the destructive/warned
## treatment (DANGER border), and an open sheet is not a warning.
##
## ⛔ **THE SUMMARY IS THE BUTTON'S SECOND LINE ON EVERY WEB, NOT JUST THE DEPOSIT ONE.** Ray named
## foragers and hunters when he asked for the move, and two shapes for one readout is the
## inconsistency this rework keeps removing — so forage, hunt, herd, forestry and extraction all
## carry it inside the control.
##
## **A SOURCE NOBODY WORKS GETS ONE LINE AND NO BLANK SECOND ONE**: a summary exists only where this
## faction already has a standing assignment, and `summary_model` is `{}` there.
func _build_compose_open_button(noun: String, subject_key: String, on_press: Callable,
        summary_model: Dictionary) -> Control:
    var button := Button.new()
    var composing := is_compose_sheet_open() and _compose.subject() == subject_key
    HudStyle.apply_button(button, "primary" if composing else "ghost")
    button.pressed.connect(on_press)
    var body: Control = null
    if not summary_model.is_empty():
        body = _build_standing_summary_from_model(summary_model)
        # **THE HOVER MOVES TO THE BUTTON.** With the face muted the summary's own labels cannot show
        # a tooltip, and the button is the whole control's hit target; the flow keeps its own
        # `tooltip_text` because it costs nothing and it is what the harnesses read the hover off.
        button.tooltip_text = String(summary_model["tooltip"])
    return HudWidgets.build_stacked_action_button(button,
        HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % noun.to_lower(), body)

## A stacked action cell's own `Button` — the child that carries the click, the stylebox and the
## tooltip. It is the cell's first child by construction; found by TYPE rather than by index for the
## reason its siblings are found by meta.
func _stacked_button(cell: Control) -> Button:
    for child in cell.get_children():
        if child is Button:
            return child as Button
    return null

## A stacked action cell's face part carrying `meta`, anywhere under it.
func _stacked_part(root: Control, meta: String) -> Control:
    if root.has_meta(meta):
        return root
    for child in root.get_children():
        if not (child is Control):
            continue
        var found := _stacked_part(child as Control, meta)
        if found != null:
            return found
    return null

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

## The drawer's one-line standing-assignment summary: `♻ 3 harvesters · +2.74 /turn`, with the SAME
## warn/overdraw and overstaff/wasted flags the Band panel's Current-actions rows render, from the
## SAME `SourceForecast.source_yield_readout` call. The rate is never recomputed here.
## The standing-summary's display model — the values `_build_standing_summary_from_model` renders,
## computed ONCE so the drawer-actions shape signature and the in-place patch read one computation.
## `working` is the `deposits` ROW this summary is about, and it is passed by the DEPOSIT caller
## alone (issue #650). The leading mark is a floor ZONE glyph, and a working that does not renew has
## no peak for a floor to sit on and was offered no dial to set one with — so its mark forks on
## `HudDepositVocab.floor_mark`, this arc's one fork, which needs the row the rate lives on. An
## `extract` row arriving without one answers the finite reading (no mark), which is the safe way
## round: the failure it forbids is a quarry claiming ♻.
func _standing_summary_model(assignment: Dictionary, kind: String, noun: String,
        working: Dictionary = {}) -> Dictionary:
    # `has_yield` is the ONE key `SourceForecast.source_yield_readout` reads that is not on the wire assignment —
    # it gates the rate on a CONFIRMED source (`_band_labor.effective_worker_map` sets it false for a
    # pending, yield-less optimistic assign). Everything else — actual/sustainable/realized,
    # `overdraws`, `workers_needed`, `wasted_yield` — is read straight off the assignment the sim sent.
    var m := assignment.duplicate()
    m["has_yield"] = assignment.has("actual_yield")
    var readout := SourceForecast.source_yield_readout(m, kind,
        _overstaff_note_ceiling(assignment, kind), noun.to_lower())
    var floor := float(assignment.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))
    var mark := HudDepositVocab.floor_mark(working, floor) \
        if kind == HudConst.LABOR_KIND_EXTRACT \
        else FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
    var text := HudComposeVocab.STANDING_SUMMARY_FORMAT % [
        mark,
        int(assignment.get("workers", 0)),
        noun,
    ]
    var suffix := String(readout["label_suffix"])
    # HUNTING BY NEED: a need row holds no standing hunters, so its worked line says it hunts when
    # needed (or how many it has sent out) and quotes no yield.
    var is_need_row := kind == SourceForecast.LABOR_KIND_HUNT \
        and bool(assignment.get("move_with_herd", false))
    if is_need_row:
        var out := int(assignment.get("muster_crew", 0))
        text = HudComposeVocab.STANDING_SUMMARY_NEED_OUT_FORMAT % [mark, out, noun] if out > 0 \
            else HudComposeVocab.STANDING_SUMMARY_NEED_WAITING_FORMAT % mark
        suffix = ""
    if suffix != "":
        text += HudComposeVocab.STANDING_SUMMARY_SEPARATOR + suffix
    # MIGRATION MODE: the worked line says so for a band whose row has the flag.
    if bool(assignment.get("move_with_herd", false)):
        text += HudComposeVocab.STANDING_SUMMARY_MOVING_CLAUSE
    # **THE GOOD-SHORTFALL ARM, KEPT IN STEP WITH THE WORK BOARD'S ROW**
    # (`docs/plan_standing_upkeep.md` §2.7). This surface and `BandPanelController`'s inspector render
    # the same `note` / `muted_note` pair, so a third arm added to one and not the other is exactly the
    # drift this repo keeps paying for. `material_short_note` supersedes whatever the readout put in
    # the slot, for the reason it does there: it names a remedy no stepper on this sheet can reach.
    var material_note := HudWorkVocab.material_short_note(
        SourceForecast.material_payoff_rows(assignment.get(
            SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_DEMAND_KEY, [])),
        SourceForecast.material_payoff_rows(assignment.get(
            SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_SUPPLIED_KEY, [])))
    # ⛔ **THE GEAR-SHORTFALL ARM IS DELIBERATELY NOT HERE, and that is not the drift the paragraph
    # above warns about.** The work board's row carries `kit_note` because it has nowhere else to say
    # it; this summary sits on a sheet whose KIT PICKER states the identical sentence one control
    # away, off the same published pair (`KitRoster.shortfall_line`'s committed arm, handed this
    # source's row by `_mount_kit_row`). Adding it here would print one shortfall twice on one sheet
    # — the failure the retired `N of M equipped` clause was, not the one the material arm fixed.
    return {
        "text": text.strip_edges(),
        "tooltip": String(readout["tooltip"]),
        "warn": bool(readout["warn"]),
        "note": material_note if material_note != "" else String(readout["note"]),
        # **THE INK IS MODEL STATE HERE TOO** — DANGER for a missing good, WARN for the staffing note
        # the readout produces. One table (`HudWorkVocab.note_color`) shared with the work board.
        "note_severity": HudWorkVocab.under_kept_note_severity(material_note),
        "muted_note": String(readout["muted_note"]),
    }

## Build the drawer's one-line standing-assignment summary (`♻ 3 harvesters · +2.74 /turn`) from a
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
        flow.add_child(HudWidgets.build_row_note_label(note,
            HudWorkVocab.note_color(String(model.get("note_severity",
                HudWorkVocab.NOTE_SEVERITY_WARN))), tooltip))
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
        # **THE INK IS PATCHED WITH THE TEXT, and skipping it would have been a silent stale colour.**
        # The shape signature carries the note's PRESENCE, not its severity, so a row whose note went
        # from *short of hands* to *short of hurdles* patches in place — and a `font_color` written
        # only at build time would leave the danger sentence wearing the staffing amber.
        note_label.add_theme_color_override("font_color", HudWorkVocab.note_color(
            String(model.get("note_severity", HudWorkVocab.NOTE_SEVERITY_WARN))))
        HudWidgets.set_label_tooltip(note_label, tooltip)
        idx += 1
    var muted_note := String(model["muted_note"])
    if muted_note != "":
        var muted_label := flow.get_child(idx) as Label
        muted_label.text = muted_note
        HudWidgets.set_label_tooltip(muted_label, tooltip)