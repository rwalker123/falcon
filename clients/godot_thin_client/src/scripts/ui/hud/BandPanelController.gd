class_name BandPanelController
extends RefCounted

## The BAND/CITY PANEL (HUD decomposition Phase 2d, docs/plan_hud_decomposition.md): the dockable
## command center's whole render path. It owns the panel HANDLE, the three zone builders
## (`band` / `work` / `parties`) and everything under them, the panel's cycler + snapshot refresh, and
## the map-focus routing the panel's own rows use. `HudLayer` keeps the drawer dispatch that calls IN
## here (`_render_occupant_drawer`), the legacy flat `%AllocationPanel` host (`_build_allocation_panel`,
## which now just stacks this controller's three public zone builders), and the targeting machinery.
##
## Built on the LegendController / TopBarReadouts / TurnOrbController / SelectionCardController /
## DrawerComposeController idiom: `HudLayer` holds one as `_bandpanel`, hands it the shared `RefCounted`
## state models BY REFERENCE (the SAME `HudBandLaborState` / `ComposeState` instances), keeps thin
## delegators for the three methods reached BY NAME (`set_band_city_panel` / `cycle_panel_band` /
## `focus_panel_band` — `Main._wire_band_city_panel` probes all three with `has_method`, and a failed
## probe fails SILENTLY), and RELAYS this controller's own five signals onto the `HudLayer` signals
## `Main` connects to. The controller never emits a `HudLayer` signal directly.
##
## THE PANEL HANDLE IS PRIVATE. Two non-moving `HudLayer` readers only ever asked "is a panel
## injected?" (`_refresh_disclosure_hosts` and `_render_occupant_drawer`, which forks the band detail
## into the dock when one is), so they ask `has_panel()` instead of holding the node.
##
## THE BOUNDARY BACK TO `HudLayer` IS TWO CALLABLES, each retained there for a reason the
## "an injection you still have to hold is relocated, not eliminated" test settles:
##   • `_emit_assign_labor` — owns the `assign_labor_requested` emit, the optimistic pending write and
##     `_after_pending_change()`. So `assign_labor` stays INDIRECT here, while the three commands with
##     no other emitter (`cancel_order` / `send_hunt_expedition` / `recall_expedition`) are signals.
##   • `_herd_label_for_id` — the herd vocabulary, also read by the targeting banner + command feed.
## The send-expedition + quarry (begin / cancel / eligibility) verbs the parties zone drives are no
## longer four Callables into HudLayer — they are a typed `TargetingController` collaborator now.
##
## Everything else arrives as a collaborator: the two state models, the selection card (roster lookup +
## pinning, for the map-focus routing, and the one selection read the vitals rows need —
## `selected_terrain_label`), the disclosure cluster (`wire_label` for the vitals row), the BAND
## DETAIL-LINE producers (`BandDetailLines`, a typed ref — the three `*_fn` Callables it replaced,
## `_unit_summary_lines` / `_expedition_summary_lines` / `_expedition_row_tooltip`, are gone with their
## adapters; the tooltip is a static `DetailFormat` call now), and a HOST node — a `RefCounted` cannot
## `add_child`, and `_confirm_destructive` parents a `ConfirmationDialog` exactly as
## `TurnOrbController` parents its fork panel.
##
## The word tables, formats and thresholds live in the topic vocab modules (`HudConst` / the matching
## `Hud*Vocab`) and the shared `DetailFormat` layer, read as `Module.X` — so a phrase is still typed in
## exactly one place.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# Standing work was cleared for a whole scope — relayed to HudLayer.cancel_order_requested.
signal cancel_order_requested(band: Dictionary, scope: String)
# A hunting party was dispatched from the parties zone — relayed to HudLayer.send_hunt_expedition_requested.
signal send_hunt_expedition_requested(payload: Dictionary)
# A DENIAL raid was dispatched — relayed to HudLayer.send_denial_raid_requested. **Its own signal, not
# a flag on the hunt one**, because its command grammar is closed at four tokens
# (`send_denial_raid <faction> <band> <party_workers> <fauna_id>`) — a fifth is a hard parse error —
# so a payload that could carry a floor or a fill target would be a payload the parser rejects.
signal send_denial_raid_requested(payload: Dictionary)
# A party was ordered home — relayed to HudLayer.recall_expedition_requested.
signal recall_expedition_requested(payload: Dictionary)
# Recenter + select a hex (a zone row / cycler jump) — relayed to HudLayer.alert_focus_requested.
signal alert_focus_requested(x: int, y: int)
# Pin an exact occupant on the map after that recenter — relayed to HudLayer.roster_occupant_selected.
signal roster_occupant_selected(kind: String, id: Variant)

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
# The party compose's quarry + autofill one-shots live on the shared compose state.
var _compose: ComposeState = null
# Roster lookup + map pinning, for the band cycler / labor-source / party jump routing.
var _selectioncard: SelectionCardController = null
# Read for `wire_label` ONLY — the vitals row's Food/Morale carets.
var _disclosures: DisclosureController = null
# The band/party detail-line producers behind the vitals label + the parties inspector strip.
var _banddetail: BandDetailLines = null
# The HUD CanvasLayer, so this RefCounted has a node to parent the confirm dialog into.
var _host: Node = null

# --- The two retained HudLayer helpers, injected as Callables (see the class header) ---
# Each is reached through a typed adapter below rather than called raw: `Callable.call` returns
# `Variant`, which would push an untyped value into every consumer here.
var _emit_assign_labor_fn: Callable
var _herd_label_for_id_fn: Callable
# The command-targeting cluster. The send-expedition + quarry (begin/cancel/eligibility) verbs the
# parties zone drives now live here, not behind Callables into HudLayer.
var _targeting: TargetingController = null

# --- Owned state (moved off HudLayer) ---
# The dockable Band/City command center (docs/plan_band_city_dock.md §3), injected by Main through
# HudLayer's `set_band_city_panel` delegator. When present, a selected player band's detail renders
# into IT rather than the Occupants card, and the panel persists across selection changes showing the
# panel band. The panel band itself (re-resolved by entity each snapshot) lives on
# `_band_labor.panel_band()`. PRIVATE — outside readers ask `has_panel()`.
var _panel: BandCityPanel = null
# ---- Band/City zone state (persists across renders, so a filter/tab/page survives a snapshot) ----
## Which sources the work board shows, how it orders them, and which page is on screen.
## **THE DEFAULT SORT IS ONE THE PLAYER'S OWN EDIT CANNOT MOVE** (issue #460). Yield scales with
## workers, so a yield-sorted board re-ranked on every `+`/`−` press — `_repage_work_zone` re-sorts
## immediately, the row jumped out from under the pointer, and the next press landed on a different
## source. A name is a fact about the source, not about the edit in flight. `Sort by yield` is still
## one pick away in the `⋯` menu, and `set_panel` adopts the player's persisted choice over this.
var _work_filter: StringName = HudWorkVocab.WORK_FILTER_ALL
var _work_sort: StringName = HudWorkVocab.WORK_SORT_NAME
var _work_page: int = 0
## The source key open in the work inspector strip ("" = none), and whether its floor picker is out.
## One row at a time — the strip costs board rows, which `_work_board_capacity` subtracts.
var _work_open_key: String = ""
var _work_floor_open: bool = false
## The party (expedition entity, as a string) whose parties-zone inspector strip is open ("" = none),
## the parties twin of `_work_open_key`. One at a time — clicking a row body toggles it.
var _party_open_key: String = ""
## The live work-zone column + its band, so `zones_resized` can RE-PAGE the board in place instead of
## re-rendering all three zones.
var _work_zone_host: VBoxContainer = null
var _work_zone_band: Dictionary = {}
## The band-zone height tier the current render was built for. Written by `build_band_zone`, read by
## `_on_zones_resized` — the one straddle the band and work halves shared, resolved by keeping BOTH
## ends in this controller.
var _band_zone_tier: int = HudWorkVocab.BAND_ZONE_TIER_TALL
## The parties compose sheet: open, and which mission has been picked ("" = none yet, which is what
## keeps the party size / floor / forecast fields hidden until the mission decides them).
var _party_compose_open: bool = false
var _party_compose_mission: String = ""
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = HudConst.WORKER_STEP
# Compose state for the hunt-expedition launch FLOOR — where the raid stops, `0.0..=1.0`.
var _send_hunt_floor: float = SourceForecast.DEFAULT_HARVEST_FLOOR
# …and its party-side twin (`docs/plan_hunt_through_combat.md` §5.2): the whole animals the party
# waits for, `SourceForecast.NO_FILL_TARGET` for the untargeted raid. **This zone is the SECOND launch
# site of `send_hunt_expedition`**, and the arc's standing rule is that the two entry points cannot
# offer different orders — a lever present on one sheet and absent on the other is the same defect as
# a lever that does nothing. **It lives on `ComposeState` beside the quarry it counts**, not here: it
# was a member of this controller cleared BESIDE the quarry by a `_clear_party_quarry` that had to
# remember to, so the one path that set a quarry without going through it — a re-pick on the map —
# carried the old herd's target onto the new one. Read through `_compose.party_fill_target()`.

func _init(band_labor: HudBandLaborState, compose: ComposeState,
        selectioncard: SelectionCardController, disclosures: DisclosureController,
        banddetail: BandDetailLines, host: Node,
        emit_assign_labor: Callable, herd_label_for_id: Callable,
        targeting: TargetingController, topbar: TopBarReadouts) -> void:
    _topbar = topbar
    _band_labor = band_labor
    _compose = compose
    _selectioncard = selectioncard
    _disclosures = disclosures
    _banddetail = banddetail
    _host = host
    _emit_assign_labor_fn = emit_assign_labor
    _herd_label_for_id_fn = herd_label_for_id
    _targeting = targeting

## `_topbar` is held for `faction_tracks` ONLY — the rung-ready mark on a work row, exactly the narrow
## reason `DrawerComposeController` holds it. A typed collaborator rather than a Callable injection,
## per the extraction rules; do not grow other reads through it.
var _topbar: TopBarReadouts = null

## The player faction's {track: progress} row, threaded into every `RungGates` call.
func _player_knowledge() -> Dictionary:
    return _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) if _topbar != null else {}

# ---- Typed adapters over the two injected HudLayer helpers -------------------------------------

## Issue a labor assignment. Retained on HudLayer because it owns the `assign_labor_requested` emit,
## the optimistic pending-labor write and `_after_pending_change()`.
##
## `improvement` NEVER reaches the command (issue #442) — it is recorded on the OPTIMISTIC PENDING
## overlay alone. The adapter has to carry it anyway: the trailing default is `IMPROVEMENT_NONE`, so
## omitting the argument writes "building nothing" over whatever the source is actually building, and
## `effective_worker_map` then reads that "" back for the rest of the turn.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String,
        floor: float, species: String = "",
        improvement: String = SourceForecast.IMPROVEMENT_NONE) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, floor, species, improvement)

## A friendlier label for a herd id. Retained on HudLayer, which also feeds the targeting banner and
## the command feed from it.
func _herd_label_for_id(herd_id: String) -> String:
    return _herd_label_for_id_fn.call(herd_id)

## Player-faction check for a band (a trivial private copy of HudLayer's, the SelectionCardController
## precedent — a one-line predicate is not worth a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

# ---- The inbound seam: is a panel even injected? ------------------------------------------------

## Is the dockable panel present? The two non-moving HudLayer readers
## (`_refresh_disclosure_hosts`, `_render_occupant_drawer`) only ever asked this, so they ask it here
## rather than holding the node.
func has_panel() -> bool:
    return _panel != null

# ---- Shared section-block helpers -------------------------------------------
#
# Two blocks the band zone and the legacy flat host both build; they sat beside `_build_allocation_panel`
# before the split and travelled with the zone builders that are their only callers.

## "FOOD OUTLOOK" section block: the merged larder projection chart (`FoodOutlookChart`). Returns null
## — the block is omitted — for a non-player band, a band with no real food flow (same gate as the Food
## breakdown), or one whose sources carry no projected schedule. The block is its own section rather
## than a summary line because BBCode cannot host a drawn chart.
func _build_food_outlook_block(band: Dictionary, compact: bool = false) -> VBoxContainer:
    if not (_is_player_unit(band) and DetailFormat.band_has_food_flow(band)):
        return null
    var arrivals := DetailFormat.merged_arrival_schedule(band)
    if arrivals.is_empty():
        return null
    var block := _make_alloc_block()
    block.add_child(HudWidgets.alloc_section_label(HudWorkVocab.ALLOC_HEADER_FOOD_OUTLOOK))
    var chart := FoodOutlookChart.new()
    # Drain = the people's meals plus the pens' feed, held flat across the horizon (see the chart's
    # header): the same two debits the Food breakdown itemizes, so the two readouts cannot disagree.
    chart.set_projection(
        DetailFormat.band_provisions(band), arrivals,
        float(band.get("food_consumption", 0.0)) + DetailFormat.band_pen_feed(band), _band_labor.current_turn())
    # A short zone gets a COMPACT chart (same series, same empty marker, less height) rather than a
    # clipped full-height one — the zone's height is fixed, so the chart yields, not the layout.
    if compact:
        chart.custom_minimum_size = Vector2(chart.custom_minimum_size.x, HudWorkVocab.FOOD_CHART_COMPACT_HEIGHT)
    block.add_child(chart)
    return block

## A fresh section-block VBox: the discrete, self-contained unit the Band/City panel arranges (a
## vertical stack when tall, a column-flow when wide). Rows are added into it exactly as they used to
## be added into the flat allocation container — only the parent node changes.
func _make_alloc_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudWorkVocab.ALLOC_BLOCK_SEPARATION)
    return block

## ============================================================================
## Band/City panel ZONES (docs/band_panel_ux_proposal.html §02/§05)
## ----------------------------------------------------------------------------
## The panel hosts three named zones at a FIXED size (see BandCityPanel): `band`
## (who they are + what they do), `work` (the paged board of worked sources) and
## `parties`. Each builder below returns a bare VBox; `HudWidgets.wrap_zone` anchors it into
## the plain-Control zone host the panel hands out, and the legacy flat host
## (`_build_allocation_panel`, the no-dock ui_preview fallback) simply stacks the
## same three VBoxes — ONE set of builders, never a second layout.
##
## NOTHING here scrolls. Content that can outgrow its box is PAGED against
## `BandCityPanel.work_zone_size()`; a ScrollContainer would reintroduce exactly
## the content-dependent height the panel rework removed.
## ============================================================================

## The interior box a zone's content may fill, in canvas px. The panel answers it from its FIXED
## geometry (`work_zone_size`), so it is a pure function of dock/collapse/window — never of content.
## The fallback keeps the no-dock ui_preview host laying out sensibly.
func _zone_box() -> Vector2:
    if _panel != null:
        var box: Vector2 = _panel.work_zone_size()
        if box.x > 0.0 and box.y > 0.0:
            return box
    return HudWorkVocab.ZONE_FALLBACK_SIZE

## Ask before a destructive bulk action. A `ConfirmationDialog` is a Window — like the section menu,
## it cannot disturb any zone's height. The body names what is SPARED, so "unassign all" never reads
## as "undo everything".
func _confirm_destructive(body: String, ok_text: String, on_confirm: Callable) -> void:
    var dialog := ConfirmationDialog.new()
    dialog.dialog_text = body
    dialog.ok_button_text = ok_text
    dialog.title = HudWorkVocab.CONFIRM_DIALOG_TITLE
    dialog.confirmed.connect(func() -> void:
        on_confirm.call()
        dialog.queue_free())
    dialog.canceled.connect(func() -> void: dialog.queue_free())
    _host.add_child(dialog)
    dialog.popup_centered()

# ---- zone `band` ------------------------------------------------------------

## Zone `band`: vitals · people · food outlook · workforce (+ the two role cards).
## `with_vitals` is false for the legacy flat host, whose Occupants card already renders the very
## same Food/Morale/Position rows in its own `%OccupantDetail` drawer above this.
func build_band_zone(band: Dictionary, with_vitals: bool = true) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    _band_zone_tier = _band_zone_tier_for(_zone_box().y)
    if with_vitals:
        col.add_child(_build_vitals_label(band))
    var people := _build_people_block(band)
    if people != null:
        col.add_child(people)
    if _band_zone_tier != HudWorkVocab.BAND_ZONE_TIER_SHORT:
        var outlook := _build_food_outlook_block(band, _band_zone_tier == HudWorkVocab.BAND_ZONE_TIER_COMPACT)
        if outlook != null:
            col.add_child(outlook)
    col.add_child(_build_workforce_block(band, _band_zone_tier == HudWorkVocab.BAND_ZONE_TIER_SHORT))
    return col

## The vitals readout — Food, Fodder, Trade, Morale and Growth, of which Food / Trade / Morale /
## Growth carry the click-to-expand disclosures (Fodder is a plain row, and there is no Output row:
## productivity reads on the WORK zone's head). Which of the optional rows appear is the producer's
## call — see `BandDetailLines.unit_summary_lines` and the `compact` note below. A
## FRESH RichTextLabel each render, so its `meta_clicked` is wired here (bound to ITSELF as the
## popover's anchor). The tint context is likewise fresh per render: it is built here, filled by
## `BandDetailLines.unit_summary_lines` as it emits the rows, and handed straight to the formatter.
func _build_vitals_label(band: Dictionary) -> RichTextLabel:
    var detail_label := RichTextLabel.new()
    detail_label.bbcode_enabled = true
    detail_label.fit_content = true
    detail_label.scroll_active = false
    detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD
    detail_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    _disclosures.wire_label(detail_label)
    var ctx := DetailFormat.Context.new()
    # The SHORT tier drops the Trade row, the same budget call `build_band_zone` makes for the
    # food-outlook chart one block below: a ~300px T/B zone CLIPS what it cannot hold, and the row
    # measures 26px against a zone that is already tight.
    # No Position row either: the coordinates are IDENTITY and the panel HEADER states them
    # (`_panel_position_label`), so a vitals row would be a second telling — and one this zone pays
    # for in height. The drawer host keeps it (it has no header and renders foreign bands).
    detail_label.text = DetailFormat.detail_bbcode(
        _banddetail.unit_summary_lines(band, _selectioncard.selected_terrain_label(), ctx,
            _band_zone_tier == HudWorkVocab.BAND_ZONE_TIER_SHORT, false), ctx)
    return detail_label

## "PEOPLE" — who the band IS: a stacked children/working-age/elders bar plus its key and the
## dependency ratio. Returns null when the snapshot carries no age structure at all, so the block is
## OMITTED rather than rendered from a fabricated split.
## The palette is deliberately MUTED against the Workforce bar below: the two bars share a shape but
## answer different questions (composition vs allocation) and must not read as the same chart twice.
func _build_people_block(band: Dictionary) -> VBoxContainer:
    # The brackets arrive FRACTIONAL (Scalar) — a real band is 9.29 children, 16.54 workers, 4.64
    # elders — so they are APPORTIONED to whole people rather than rounded one at a time. Rounding
    # each independently is what made this panel read 9 + 17 + 5 = 31 beside a top bar reading 30:
    # the same band, counted twice, disagreeing by a person.
    var raw: Array[float] = [
        float(band.get("age_children", 0.0)),
        float(band.get("age_working", 0.0)),
        float(band.get("age_elders", 0.0)),
    ]
    # `age_working` is the age COHORT; `working_age` is the count of ASSIGNABLE workers (a different
    # quantity that happens to track it). Fall back to the latter when the cohort field is absent.
    if raw[1] <= 0.0:
        raw[1] = float(band.get("working_age", 0))
    var whole := HudFormat.apportion_people(raw)
    var children: int = whole[0]
    var working: int = whole[1]
    var elders: int = whole[2]
    var total := children + working + elders
    if total <= 0:
        return null
    var segments: Array = []
    if children > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT, "tooltip": "%d %s" % [children, HudWorkVocab.PEOPLE_LABEL_CHILDREN]})
    if working > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_WORKING, "count": working,
            "color": HudStyle.INK_DIM, "tooltip": "%d %s" % [working, HudWorkVocab.PEOPLE_LABEL_WORKING]})
    if elders > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK, "tooltip": "%d %s" % [elders, HudWorkVocab.PEOPLE_LABEL_ELDERS]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PEOPLE, str(total)))
    block.add_child(HudWidgets.build_composition_bar(segments))
    block.add_child(HudWidgets.build_composition_key(segments, _build_dependency_chip(children, working, elders)))
    return block

## The dependency ratio chip: dependents (children + elders) per 100 working-age adults, WARN-tinted
## once the band carries more mouths than hands. Null when there is no working-age cohort to divide by.
func _build_dependency_chip(children: int, working: int, elders: int) -> Control:
    if working <= 0:
        return null
    var dependents := children + elders
    var per_hundred := HudFormat.dependency_per_hundred(dependents, working)
    var chip := Label.new()
    chip.text = HudWorkVocab.PEOPLE_DEPENDENCY_FORMAT % dependents
    chip.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
    chip.add_theme_color_override("font_color",
        HudStyle.WARN if per_hundred > HudWorkVocab.PEOPLE_DEPENDENCY_HEAVY else HudStyle.INK_FAINT)
    HudWidgets.set_label_tooltip(chip, HudFormat.dependency_tooltip(dependents, working))
    chip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    chip.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    return chip

## "WORKFORCE" — what the band DOES: a stacked Forage/Hunt/Roles/Parties/Idle bar, its key, and the
## two standing-role CARDS. Saturated against People's muted palette (see `_build_people_block`).
func _build_workforce_block(band: Dictionary, compact_cards: bool) -> VBoxContainer:
    var idle := _band_labor.effective_idle(band)
    var forage_workers := 0
    var hunt_workers := 0
    var merged := _band_labor.effective_worker_map(band)
    for key in merged:
        var m: Dictionary = merged[key]
        var workers := int(m.get("workers", 0))
        match String(m.get("kind", "")):
            SourceForecast.LABOR_KIND_FORAGE: forage_workers += workers
            SourceForecast.LABOR_KIND_HUNT: hunt_workers += workers
    var scout_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_SCOUT)
    var warrior_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_WARRIOR)
    var role_workers := int(scout_eff.get("workers", 0)) + int(warrior_eff.get("workers", 0))
    var party_workers := _band_labor.band_party_workers(band)
    var segments: Array = []
    for spec in [
        [HudWorkVocab.WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [HudWorkVocab.WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        [HudWorkVocab.WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        [HudWorkVocab.WORKFORCE_KEY_PARTIES, party_workers, HudStyle.WARN],
        [HudWorkVocab.WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORKFORCE,
        HudWorkVocab.WORKFORCE_IDLE_FORMAT % [idle, int(band.get("working_age", 0))],
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM))
    if not segments.is_empty():
        block.add_child(HudWidgets.build_composition_bar(segments))
        block.add_child(HudWidgets.build_composition_key(segments))
    # The two standing roles as CARDS, side by side — a bordered card reads as "a standing role", not
    # as one more worked source in a list (the complaint the card treatment fixes).
    var cards := HBoxContainer.new()
    cards.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    cards.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    cards.add_child(_build_role_card(band, HudWorkVocab.ROLE_NAME_SCOUT, HudWorkVocab.SCOUT_ROLE_HINT, HudConst.LABOR_KIND_SCOUT, scout_eff, idle, compact_cards))
    # A visible predator within raid range turns the Warrior card's static hint into a live crimson
    # alert naming the on-guard count — the guarding role is only legible when the threat it answers is.
    var warrior_threat := _band_predator_threat_present(band)
    var warrior_hint := HudWorkVocab.WARRIOR_ROLE_HINT
    if warrior_threat:
        warrior_hint = HudWorkVocab.WARRIOR_THREAT_ALERT_FORMAT % int(warrior_eff.get("workers", 0))
    cards.add_child(_build_role_card(band, HudWorkVocab.ROLE_NAME_WARRIOR, warrior_hint, HudConst.LABOR_KIND_WARRIOR, warrior_eff, idle, compact_cards, warrior_threat))
    block.add_child(cards)
    return block

## Predators Phase 3 — is a VISIBLE, camp-threatening predator within exact raid range of this band?
## A predator is any herd with `prey_sense_radius > 0`; it MENACES the camp when `attack × aggression`
## is positive (the same THREAT product the map overlay draws); and it can raid this band's larder when
## its tile is within `raid_radius` (the sim's echoed `predators.raid_radius`, per cohort) hex-distance
## of the band's tile. Herd telemetry is fog-filtered, so `world_herds()` already holds only VISIBLE
## herds — exactly the predators the player can see and should be warned about. Uses the shared wrap-aware
## `SourceForecast.hex_distance_wrapped` (never a hand-rolled distance) with the band's grid dims.
func _band_predator_threat_present(band: Dictionary) -> bool:
    var raid_radius := int(band.get("raid_radius", 0))
    if raid_radius <= 0:
        return false
    var origin := SourceForecast.band_tile(band)
    if origin.x < 0 or origin.y < 0:
        return false
    var grid_width := _band_labor.grid_width()
    var wrap := _band_labor.wrap_horizontal()
    for herd_variant in _band_labor.world_herds():
        if not (herd_variant is Dictionary):
            continue
        var herd: Dictionary = herd_variant
        if int(herd.get("prey_sense_radius", 0)) <= 0:
            continue
        if float(herd.get("attack", 0.0)) * float(herd.get("aggression", 0.0)) <= 0.0:
            continue
        var dist := SourceForecast.hex_distance_wrapped(
            origin.x, origin.y, int(herd.get("x", -1)), int(herd.get("y", -1)), grid_width, wrap)
        if dist >= 0 and dist <= raid_radius:
            return true
    return false

## One standing-role card: name · one-line hint · the SAME −/+ stepper (same `assign_labor` emit,
## same idle gating) the role rows used to carry.
## `alert` (Predators Phase 3) tints the hint crimson — the Warrior card wears it when a predator is
## within raid range, so the live "Predator nearby" warning reads as danger, not routine guidance.
func _build_role_card(band: Dictionary, role_name: String, hint: String, kind: String, effective: Dictionary, idle: int, compact: bool = false, alert: bool = false) -> PanelContainer:
    var workers := int(effective.get("workers", 0))
    var pending := bool(effective.get("pending", false))
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    card.add_theme_stylebox_override("panel", HudStyle.role_card_stylebox())
    # In a short zone the hint moves to the card's tooltip — the words survive, the two lines do not.
    card.tooltip_text = hint
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    card.add_child(col)
    var title := Label.new()
    title.text = role_name
    title.add_theme_font_size_override("font_size", HudWorkVocab.ROLE_CARD_NAME_FONT_SIZE)
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK)
    col.add_child(title)
    if not compact:
        var hint_label := HudWidgets.alloc_hint_label(hint)
        if alert:
            hint_label.add_theme_color_override("font_color", HudStyle.THREAT_ACCENT)
        hint_label.custom_minimum_size = Vector2(0.0, HudWorkVocab.ROLE_CARD_HINT_HEIGHT)
        col.add_child(hint_label)
    var stepper := HBoxContainer.new()
    stepper.alignment = BoxContainer.ALIGNMENT_CENTER
    stepper.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    HudWidgets.add_stepper_controls(stepper, workers, idle > 0,
        # A BAND-WIDE ROLE (scout / warrior) works no source, so it has no escapement floor to set.
        # The sim ignores the token on those branches; the default is the honest thing to send.
        func(n: int) -> void: _emit_assign_labor(
            band, kind, n, -1, -1, "", SourceForecast.DEFAULT_HARVEST_FLOOR))
    col.add_child(stepper)
    return card

# ---- zone `work` (the paged board) ------------------------------------------

## Zone `work`: header · filter chips · the paged board · pager · inspector strip. The column keeps a
## reference to itself so `zones_resized` can RE-PAGE in place rather than re-render the whole panel.
func build_work_zone(band: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    _work_zone_host = col
    _work_zone_band = band
    _fill_work_zone(col, band)
    return col

## The panel's `zones_resized` handler. Re-paging the work board is the cheap common case, but the
## BAND zone yields by height tier too (chart / role-card hints), so a tier change needs the zones
## rebuilt rather than the board re-paged — otherwise a tall-shell band zone lands in a short box and
## is silently clipped by its host.
func _on_zones_resized() -> void:
    if _band_zone_tier != _band_zone_tier_for(_zone_box().y):
        rerender()
        return
    _repage_work_zone()

## Which content tier the band zone's height affords (see `BAND_ZONE_*_MIN_HEIGHT`).
func _band_zone_tier_for(zone_height: float) -> int:
    if zone_height >= HudWorkVocab.BAND_ZONE_TALL_MIN_HEIGHT:
        return HudWorkVocab.BAND_ZONE_TIER_TALL
    if zone_height >= HudWorkVocab.BAND_ZONE_CHART_MIN_HEIGHT:
        return HudWorkVocab.BAND_ZONE_TIER_COMPACT
    return HudWorkVocab.BAND_ZONE_TIER_SHORT

## Re-page the live work board against the panel's new zone box. Only the board is rebuilt — the
## other two zones are untouched.
func _repage_work_zone() -> void:
    if _work_zone_host == null or not is_instance_valid(_work_zone_host) or _work_zone_band.is_empty():
        return
    HudWidgets.clear_children(_work_zone_host)
    _fill_work_zone(_work_zone_host, _work_zone_band)

func _fill_work_zone(col: VBoxContainer, band: Dictionary) -> void:
    var idle := _band_labor.effective_idle(band)
    var models := _work_source_models(band, idle)
    col.add_child(_build_work_head(band, models,
        _work_component_sum(models, "rate"), _work_component_sum(models, "trade_rate")))
    # BEFORE the chips are built, so the pressed chip is always one that actually renders.
    _reconcile_work_filter(models)
    col.add_child(_build_work_chips(models))
    var filtered := _filter_work_models(models)
    _sort_work_models(filtered)
    # Drop an inspector pinned to a source that has left the filtered set (unassigned, filtered out).
    var inspected := _find_work_model(filtered, _work_open_key)
    if inspected.is_empty():
        _work_open_key = ""
        _work_floor_open = false
    if filtered.is_empty():
        var hint := HudWidgets.alloc_hint_label(HudWorkVocab.WORK_EMPTY_HINT)
        hint.size_flags_vertical = Control.SIZE_EXPAND_FILL
        col.add_child(hint)
        return
    var capacity := _work_board_capacity(filtered.size(), inspected)
    var page_size := int(capacity["page_size"])
    var pages := int(capacity["pages"])
    _work_page = clampi(_work_page, 0, maxi(pages - 1, 0))
    var start := _work_page * page_size
    col.add_child(_build_work_board(band, filtered.slice(start, start + page_size),
        int(capacity["cols"]), int(capacity["rows_per_col"])))
    if pages > 1:
        col.add_child(_build_work_pager(pages, start, mini(start + page_size, filtered.size()), filtered.size()))
    if not inspected.is_empty():
        col.add_child(_build_work_inspector(band, inspected))

## Board capacity, derived ENTIRELY from the fixed zone box:
##   cols        = zone width / WORK_COLUMN_MIN_WIDTH, clamped to 1..WORK_MAX_COLUMNS
##   rows_per_col = remaining height / WORK_ROW_HEIGHT, after the head, chips, inspector and (when it
##                  is actually needed) the pager — each of which reserves the very height it draws at.
## The pager is circular (it only exists when one page cannot hold everything, but it costs a row), so
## it is resolved in two passes: measure without it, and if that still needs more than one page, remeasure.
## `inspected` is the open inspector's model, EMPTY when none is open.
func _work_board_capacity(count: int, inspected: Dictionary) -> Dictionary:
    var box := _zone_box()
    var inspector_h := 0.0 if inspected.is_empty() else _work_inspector_height(inspected)
    var chrome := HudWorkVocab.ZONE_HEAD_HEIGHT + HudWorkVocab.WORK_CHIPS_HEIGHT + inspector_h \
        + float(HudWorkVocab.ZONE_BLOCK_SEPARATION) * HudWorkVocab.WORK_ZONE_GAP_COUNT
    var rows := maxi(1, int((box.y - chrome) / HudWorkVocab.WORK_ROW_HEIGHT))
    var cols := _declare_work_columns(count, rows)
    var pages := ceili(float(count) / float(maxi(cols * rows, 1)))
    if pages > 1:
        rows = maxi(1, int((box.y - chrome - HudWorkVocab.WORK_PAGER_HEIGHT - float(HudWorkVocab.ZONE_BLOCK_SEPARATION)) / HudWorkVocab.WORK_ROW_HEIGHT))
        cols = _declare_work_columns(count, rows)
        pages = ceili(float(count) / float(maxi(cols * rows, 1)))
    return {"cols": cols, "rows_per_col": rows, "page_size": cols * rows, "pages": maxi(pages, 1)}

## How many board columns this band's sources actually WANT, declared to the panel so the card can be
## drawn that wide (issue #377), and answered back for the board to fill.
##
## **THE DIRECTION INVERTED HERE, and that is the whole point.** `cols` used to be read OFF the zone's
## width — the panel spanned the monitor, so on an ultrawide the board got four columns whether or not
## the band had anything to put in them, and a band with no sources at all got an empty zone stretched
## across two feet of screen. It is now derived from the SOURCE COUNT and the rows a column holds, and
## the panel sizes its card to the answer.
##
## **It stays acyclic because `rows` comes from the zone's HEIGHT**, which a horizontal dock fixes and
## which nothing here can move. Width follows count; count never follows width.
##
## Without a panel (the `ui_preview` no-dock fallback) there is nobody to declare to, so it falls back
## to measuring the box exactly as before — that host is a fixed-width card with no card to resize.
## **The panel's ANSWER is what gets built, not the want.** `set_work_columns` clamps to what the strip
## can actually pay for — a 380px side dock affords one column however many sources there are — and a
## board built to the unclamped want overflows its clipping zone host silently.
func _declare_work_columns(count: int, rows: int) -> int:
    if _panel == null:
        return clampi(int(_zone_box().x / HudWorkVocab.WORK_COLUMN_MIN_WIDTH), 1, HudWorkVocab.WORK_MAX_COLUMNS)
    var wanted := clampi(ceili(float(count) / float(maxi(rows, 1))), 1, HudWorkVocab.WORK_MAX_COLUMNS)
    return _panel.set_work_columns(wanted)

## The board itself: `cols` column VBoxes filled COLUMN-MAJOR (top of column 1 to its bottom, then
## column 2), separated by a hairline rule. Fixed-height rows, no scroll — the page IS the limit.
func _build_work_board(band: Dictionary, page: Array, cols: int, rows_per_col: int) -> HBoxContainer:
    var board := HBoxContainer.new()
    board.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    board.size_flags_vertical = Control.SIZE_EXPAND_FILL
    board.add_theme_constant_override("separation", HudWorkVocab.WORK_COLUMN_SEPARATION)
    for c in range(cols):
        if c > 0:
            var rule := ColorRect.new()
            rule.color = HudStyle.LINE_SOFT
            rule.custom_minimum_size = Vector2(HudWorkVocab.WORK_COLUMN_RULE_WIDTH, 0.0)
            rule.size_flags_vertical = Control.SIZE_EXPAND_FILL
            rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
            board.add_child(rule)
        var column := VBoxContainer.new()
        column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.size_flags_vertical = Control.SIZE_FILL
        column.add_theme_constant_override("separation", 0)
        board.add_child(column)
        for r in range(rows_per_col):
            var index := c * rows_per_col + r
            if index >= page.size():
                break
            column.add_child(_build_work_row(band, page[index]))
    return board

## The zone's head row: WORK · n sources · the band's total rate(s) · the `⋯` section menu.
func _build_work_head(band: Dictionary, models: Array, income: float, trade_income: float) -> HBoxContainer:
    # The two sorts are a mutually exclusive SET, so they carry the radio mark and `Unassign all` — an
    # action, not a member — does not. Without it the menu offered two sorts and stated neither, which
    # is what made the board's default order unreadable. `_repage_work_zone` rebuilds this head, so a
    # pick refreshes the mark with no extra wiring.
    var menu := HudWidgets.build_section_menu([
        {"label": HudWorkVocab.WORK_MENU_SORT_YIELD,
            HudWidgets.MENU_ENTRY_CHECKED: _work_sort == HudWorkVocab.WORK_SORT_YIELD,
            "on_pick": func() -> void: _set_work_sort(HudWorkVocab.WORK_SORT_YIELD)},
        {"label": HudWorkVocab.WORK_MENU_SORT_NAME,
            HudWidgets.MENU_ENTRY_CHECKED: _work_sort == HudWorkVocab.WORK_SORT_NAME,
            "on_pick": func() -> void: _set_work_sort(HudWorkVocab.WORK_SORT_NAME)},
        {"label": HudWorkVocab.WORK_MENU_UNASSIGN_FORMAT % models.size(), "disabled": models.is_empty(),
            "on_pick": func() -> void: _on_work_unassign_all_pressed(band, models.size())},
    ], HudWorkVocab.WORK_MENU_TOOLTIP)
    var head := HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORK, HudWorkVocab.WORK_SOURCES_FORMAT % models.size(), menu)
    # The total sits between the count and the menu, tinted like the Food line's net rate.
    var total := Label.new()
    total.text = SourceForecast.format_yield(income)
    total.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
    total.add_theme_color_override("font_color", HudStyle.HEALTHY if income > 0.0 else HudStyle.INK_DIM)
    HudWidgets.set_label_tooltip(total, HudWorkVocab.WORK_TOTAL_TOOLTIP)
    head.add_child(total)
    head.move_child(total, head.get_child_count() - 2)
    # THE TRADE TOTAL IS A SIBLING, NEVER A SUMMAND (issue #337). The food figure beside it is
    # `actual_yield`-denominated because that is the sim's larder identity (`larder_delta ==
    # food_income − food_consumption − pen_feed_upkeep`); folding trade in would break the one
    # invariant this arc preserved. But leaving it out entirely made the header VISIBLY not add up —
    # a trade-only wolf row sat directly beneath a total that excluded it, so the one source paying
    # only trade read as contributing nothing. So: the arc's own rule, one level up. Rendered only
    # when non-zero, hence a band with no trade-paying source renders exactly as it did before.
    if SourceForecast.has_component(trade_income):
        var trade_total := Label.new()
        trade_total.text = SourceForecast.format_trade(trade_income)
        trade_total.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
        trade_total.add_theme_color_override("font_color", HudStyle.HEALTHY)
        HudWidgets.set_label_tooltip(trade_total, HudWorkVocab.WORK_TRADE_TOTAL_TOOLTIP)
        head.add_child(trade_total)
        head.move_child(trade_total, head.get_child_count() - 2)
    # THE OUTPUT ITEM — a THIRD sibling, and it qualifies the two beside it rather than adding to
    # them: `output_multiplier` is the discontent modifier every rate on this board is already scaled
    # by, so it belongs where its consequence is visible and not as a row of the height-capped band
    # zone. Same gate the vitals row carried — only BELOW full output — because a head item
    # permanently reading `Output 100%` is noise on a row that is otherwise live summary. It trails
    # the rates deliberately: it is a note ABOUT them.
    var output: float = float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    if output < SourceForecast.OUTPUT_FULL:
        var output_item := Label.new()
        output_item.text = HudWorkVocab.WORK_OUTPUT_FORMAT % int(round(output * 100.0))
        output_item.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
        output_item.add_theme_color_override("font_color", BandFoodStatus.color_for_output(output))
        HudWidgets.set_label_tooltip(output_item, HudWorkVocab.WORK_OUTPUT_TOOLTIP)
        head.add_child(output_item)
        head.move_child(output_item, head.get_child_count() - 2)
    return head

## The filter chips ARE the summary: counts + per-kind rates, and pressing one filters the board.
## **A chip for an EMPTY set never renders** — a kind the band works none of is dead weight in a row
## that is otherwise live summary, and an always-present `⚠ 0` reads as an alarm. `All` always shows
## (it is the reset), so the row is never empty.
func _build_work_chips(models: Array) -> HFlowContainer:
    var chips := HFlowContainer.new()
    chips.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_CHIPS_HEIGHT)
    chips.add_theme_constant_override("h_separation", HudWorkVocab.WORK_CHIP_SEPARATION)
    var forage: Array = models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_FORAGE)
    var hunt: Array = models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_HUNT)
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_ALL, HudWorkVocab.WORK_CHIP_ALL_FORMAT % models.size(), false))
    # A chip's rate is BOTH products, each only when non-zero (issue #337) — the chip is a per-kind
    # summary of the same rows the head totals, so counting `🦌 2` sources and then quoting only the
    # food-paying one's rate is the same arithmetic that visibly failed in the header.
    if not forage.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_FORAGE, HudWorkVocab.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.DEFAULT, forage.size(), _work_chip_rate_text(forage)], false))
    if not hunt.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_HUNT, HudWorkVocab.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.HUNT, hunt.size(), _work_chip_rate_text(hunt)], false))
    if not attention.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_ATTENTION,
            HudWorkVocab.WORK_CHIP_ATTENTION_FORMAT % attention.size(), true))
    # The READY chip is its own count beside the attention one, never folded into it: trouble and
    # opportunity are different questions, and it is what makes the knowledge-completion moment legible
    # — a track finishes and a dozen rows light up at once.
    var ready: Array = models.filter(func(m): return String(m["ready_policy"]) != "")
    if not ready.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_READY,
            HudWorkVocab.WORK_CHIP_READY_FORMAT % ready.size(), false))
    return chips

## A filter chip's rate face: this kind's food total and trade total, each rendered only when non-zero.
func _work_chip_rate_text(models: Array) -> String:
    return SourceForecast.magnitude_components(
        _work_component_sum(models, "rate"), _work_component_sum(models, "trade_rate"))

## Σ of ONE yield component over a model set — the zone's single summing primitive, so the head's two
## totals and every chip's two totals are added up the same way over the same rows and cannot drift.
## `key` names a model's yield component (`"rate"` = food, `"trade_rate"`), never a rate itself.
func _work_component_sum(models: Array, key: String) -> float:
    var total := 0.0
    for m in models:
        total += float((m as Dictionary).get(key, 0.0))
    return total

func _build_work_chip(filter: StringName, text: String, alert: bool) -> Button:
    var active := _work_filter == filter
    var chip := Button.new()
    chip.text = text
    chip.focus_mode = Control.FOCUS_NONE
    HudStyle.apply_button(chip, "primary" if active else "ghost")
    HudWidgets.compact(chip, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_CHIP_PADDING_V)
    if alert and not active:
        chip.add_theme_color_override("font_color", HudStyle.WARN)
    chip.tooltip_text = HudWorkVocab.WORK_CHIP_TOOLTIP
    chip.pressed.connect(func() -> void: _set_work_filter(filter))
    return chip

## ONE-LINE source row: severity stripe · glyph · label (clipped) · rate · SOURCE-RUNG mark ·
## policy/⚠ marks · the existing −/+ stepper. Clicking anywhere but the stepper opens the row in the
## inspector strip.
##
## The rung mark and the policy marks are TWO AXES and both are needed: the rung says what the source
## IS (wild / Tended Patch / Field, wild / pastoral / penned), the marks say what is being done to it
## right now. A Tended Patch on Sustain and a Tended Patch on Deplete are different situations.
func _build_work_row(band: Dictionary, model: Dictionary) -> PanelContainer:
    var open := String(model.get("key", "")) == _work_open_key
    var row := PanelContainer.new()
    row.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_ROW_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.mouse_filter = Control.MOUSE_FILTER_STOP
    row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    row.tooltip_text = String(model.get("tooltip", ""))
    row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(open))
    row.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
            _toggle_work_inspector(String(model.get("key", ""))))
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    row.add_child(line)
    # Severity stripe: WARN when the source is overdrawing or overstaffed, SIGNAL while an edit is
    # still pending, transparent otherwise — so the eye finds trouble without reading a word.
    var stripe := ColorRect.new()
    stripe.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_STRIPE_WIDTH, 0.0)
    stripe.size_flags_vertical = Control.SIZE_EXPAND_FILL
    stripe.color = _work_row_stripe_color(model)
    stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(stripe)
    # The SOURCE mark: bundled art where the client has it, the emoji where it does not. The column
    # is the same fixed `WORK_ROW_ICON_WIDTH` either way, so a board mixing art and emoji rows still
    # lines up down the icon column (issue #439).
    line.add_child(HudWidgets.build_marker_icon(
        model.get("icon_texture") as Texture2D, String(model.get("icon", "")),
        HudWorkVocab.WORK_ROW_ICON_WIDTH, HudWorkVocab.WORK_ROW_FONT_SIZE))
    var label := Label.new()
    label.text = String(model.get("label", ""))
    label.clip_text = true
    # A label too long even for the widened column ELLIPSISES rather than hard-cutting: `Hunt Woolly
    # Mamm…` reads as a truncation, `Forage (73, 20` reads as a wrong coordinate.
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("pending", false)) else HudStyle.INK)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    var rate := Label.new()
    # ONE COMPONENT in this fixed-width column (issue #337): a board row has a single narrow rate slot
    # beside the marks and the stepper, so it shows the product the source actually PAYS — food when
    # there is food (every forage patch and every edible quarry, so this is unchanged for them), else
    # the trade rate marked with `FoodIcons.TRADE_GOODS_GLYPH`. A wolf row therefore reads `⇄+0.22`
    # rather than the `+0.00` that said the hunt was worth nothing. The inspector strip below states
    # BOTH components in full, which is where a deer's trade shows.
    rate.text = _work_row_rate_text(model)
    rate.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_RATE_WIDTH, 0.0)
    rate.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    rate.add_theme_color_override("font_color", HudStyle.INK_DIM)
    rate.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    rate.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(rate)
    # THE SOURCE-RUNG MARK, in its own reserved slot left of the marks — what the source IS, beside
    # what the band is DOING to it. Tinted SIGNAL because a standing rung is a completed investment,
    # the same treatment `DetailFormat.cultivation_value_hex` / `field_value_hex` / `corral_value_hex`
    # give it in the detail readouts; that colour is also what keeps the two glyph families from
    # reading as one compound mark at this size. Empty text on a wild source — the slot stays reserved
    # so the right-anchored furniture lines up down the board.
    var rung := Label.new()
    rung.text = String(model.get("rung_glyph", ""))
    rung.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_RUNG_WIDTH, 0.0)
    rung.add_theme_color_override("font_color", HudStyle.SIGNAL)
    rung.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    # PASS, not IGNORE: a Label needs a non-IGNORE filter for its own `tooltip_text` to ever show.
    # Deliberately NOT `HudWidgets.set_label_tooltip`, which sets STOP — the whole row is a click
    # target, and STOP here would make the rung slot a dead 16px hole in it. PASS shows the tooltip
    # AND lets the press bubble to the row's `gui_input`.
    rung.mouse_filter = Control.MOUSE_FILTER_PASS if rung.text != "" else Control.MOUSE_FILTER_IGNORE
    rung.tooltip_text = String(model.get("rung_tooltip", ""))
    rung.set_meta(HudWorkVocab.WORK_ROW_RUNG_META, String(model.get("rung_glyph", "")))
    line.add_child(rung)
    # THE RUNG ON OFFER — the third and last glyph axis on a row, and it is deliberately a SEPARATE
    # slot from the two beside it: `rung_glyph` is what the source IS, `marks` is what the band is
    # DOING, and this is what it COULD BE. Folding it into either would collapse a distinction the
    # whole feature exists to draw.
    #
    # It does NOT touch the severity stripe. That stripe means WARN (overdrawing, overstaffed) or
    # SIGNAL (a pending edit); an opportunity in the same channel would give the one control for
    # finding trouble two meanings.
    var ready := Label.new()
    var ready_glyph := String(model.get("ready_glyph", ""))
    var building_glyph := String(model.get("building_glyph", ""))
    # UNDER WAY beats ON OFFER in this slot. Before this the slot was empty while a verb was being
    # worked, so a patch you were actively cultivating looked emptier than the untouched one beside it
    # advertising `⌃` — the state the player is WAITING ON was the one state with no mark.
    if building_glyph != "":
        ready.text = HudWorkVocab.WORK_ROW_BUILDING_FORMAT % [building_glyph,
            HudFormat.progress_percent(float(model.get("building_progress", 0.0)))]
    else:
        ready.text = "" if ready_glyph == "" else HudWorkVocab.WORK_ROW_READY_FORMAT % ready_glyph
    ready.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_READY_WIDTH, 0.0)
    ready.add_theme_color_override("font_color",
        HudStyle.SIGNAL_DEEP if building_glyph != "" else HudStyle.SIGNAL)
    ready.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    ready.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(ready)
    var marks := Label.new()
    marks.text = String(model.get("marks", ""))
    marks.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_MARKS_WIDTH, 0.0)
    # Amber for an overdraw (⚠) OR an under-contained managed herd (its shed ⚠) — both are trouble the
    # eye must find; INK_DIM otherwise (a plain policy glyph).
    marks.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("warn", false)) or bool(model.get("under_herded", false)) else HudStyle.INK_DIM)
    marks.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    marks.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(marks)
    HudWidgets.add_stepper_controls(line, int(model.get("workers", 0)), bool(model.get("can_add", false)),
        func(n: int) -> void: _emit_work_assign(band, model, n), true)
    return row

func _work_row_stripe_color(model: Dictionary) -> Color:
    if bool(model.get("warn", false)) or String(model.get("note", "")) != "":
        return HudStyle.WARN
    if bool(model.get("pending", false)):
        return HudStyle.SIGNAL
    return Color(0.0, 0.0, 0.0, 0.0)

## The pager, shown only when one page cannot hold the filtered set.
func _build_work_pager(pages: int, start: int, shown_end: int, total: int) -> HBoxContainer:
    var pager := HBoxContainer.new()
    pager.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_PAGER_HEIGHT)
    pager.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var prev := Button.new()
    prev.text = HudWorkVocab.PAGER_PREV_GLYPH
    prev.focus_mode = Control.FOCUS_NONE
    prev.disabled = _work_page <= 0
    prev.tooltip_text = HudWorkVocab.PAGER_PREV_TOOLTIP
    HudStyle.apply_button(prev, "ghost")
    HudWidgets.compact(prev, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    prev.pressed.connect(func() -> void: _step_work_page(-1))
    pager.add_child(prev)
    var label := Label.new()
    label.text = HudWorkVocab.PAGER_FORMAT % [_work_page + 1, pages]
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    pager.add_child(label)
    var next := Button.new()
    next.text = HudWorkVocab.PAGER_NEXT_GLYPH
    next.focus_mode = Control.FOCUS_NONE
    next.disabled = _work_page >= pages - 1
    next.tooltip_text = HudWorkVocab.PAGER_NEXT_TOOLTIP
    HudStyle.apply_button(next, "ghost")
    HudWidgets.compact(next, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    next.pressed.connect(func() -> void: _step_work_page(1))
    pager.add_child(next)
    var range_label := Label.new()
    range_label.text = HudWorkVocab.PAGER_RANGE_FORMAT % [start + 1, shown_end, total]
    range_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    range_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    range_label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    range_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    pager.add_child(range_label)
    return pager

## The inspector strip — the row's SECOND and THIRD lines, relocated to one place at the bottom of the
## zone so the board itself stays one line per source. Spells the yield/policy/status out in words,
## carries the warning lines and the arrival strip, and offers the three inline actions.
## `Unassign` lives HERE (not as a hover `✕` on the row) — a destructive control beside the `−`
## stepper would be a mis-click hazard; this is the labelled version.
func _build_work_inspector(band: Dictionary, model: Dictionary) -> PanelContainer:
    var strip := PanelContainer.new()
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.custom_minimum_size = Vector2(0.0, _work_inspector_height(model))
    strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    strip.add_child(col)
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    # The mark is its own child rather than a prefix welded into the title's text: a texture cannot
    # live inside a `Label.text`, and splitting it is what lets the strip show the same art as the
    # row it belongs to. `WORK_ROW_SEPARATION` on `head` is what spaces them, as it did the string.
    head.add_child(HudWidgets.build_marker_icon(
        model.get("icon_texture") as Texture2D, String(model.get("icon", "")),
        HudWorkVocab.WORK_ROW_ICON_WIDTH, HudWorkVocab.WORK_ROW_FONT_SIZE))
    var title := Label.new()
    title.text = String(model.get("label", ""))
    title.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudWorkVocab.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_work_inspector(String(model.get("key", ""))))
    head.add_child(close)
    col.add_child(head)
    col.add_child(HudWidgets.build_status_part(_work_inspector_sentence(model), HudStyle.INK_DIM))
    if bool(model.get("warn", false)):
        col.add_child(HudWidgets.build_status_part(HudWorkVocab.WORK_INSPECT_OVERDRAW_LINE, HudStyle.WARN))
    if String(model.get("note", "")) != "":
        col.add_child(HudWidgets.build_status_part(String(model.get("note", "")), HudStyle.WARN))
    if String(model.get("muted_note", "")) != "":
        col.add_child(HudWidgets.build_status_part(String(model.get("muted_note", "")), HudStyle.INK_FAINT))
    var schedule: PackedFloat32Array = model.get("schedule", PackedFloat32Array())
    if ArrivalStrip.has_gap(schedule):
        var arrivals := ArrivalStrip.new()
        arrivals.set_schedule(schedule, _band_labor.current_turn())
        col.add_child(arrivals)
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", HudWorkVocab.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_JUMP, HudStyle.INK, func() -> void:
        _focus_work_source(model)))
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_POLICY, HudStyle.INK, func() -> void:
        _work_floor_open = not _work_floor_open
        _repage_work_zone()))
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_UNASSIGN, HudStyle.DANGER, func() -> void:
        _work_open_key = ""
        _work_floor_open = false
        _emit_work_assign(band, model, 0)))
    col.add_child(links)
    if _work_floor_open:
        # THE THREE FLOOR PRESETS, and nothing else to say about them. **DELIBERATELY NO SLIDER HERE**:
        # this zone is a fixed-width box the compose sheet is not, and re-pointing a standing crew from
        # the board is a coarse decision — the fine dial lives on the source's own compose sheet, where
        # the forecast that would justify a 5% move renders beside it.
        col.add_child(HudWidgets.build_floor_picker(func(floor: float) -> void:
            _commit_work_floor(band, model, floor),
            float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)), {},
            HudWorkVocab.ZONE_POLICY_PICKER_COLUMNS))
    return strip

func _commit_work_floor(band: Dictionary, model: Dictionary, floor: float) -> void:
    _work_floor_open = false
    _emit_work_assign(band, model, int(model.get("workers", 0)), floor)

## The height the open inspector reserves — BOTH what `_work_board_capacity` subtracts from the board
## and what the strip actually draws at, so the page can never overflow its zone (the work-board rule).
func _work_inspector_height(_model: Dictionary) -> float:
    # ONE open height now: the standing-investment line the taller variant reserved room for is gone
    # with the axis split (see `_build_work_inspector`), so every open picker is the same four rungs.
    return HudWorkVocab.WORK_INSPECTOR_POLICY_HEIGHT if _work_floor_open \
        else HudWorkVocab.WORK_INSPECTOR_HEIGHT

## The board row's single-slot rate string — food when the source pays food, else its trade rate with
## the trade glyph, and "" when the row carries no confirmed yield at all. One definition, since the
## row and its severity reading must agree on which number is being shown.
func _work_row_rate_text(model: Dictionary) -> String:
    if not bool(model.get("has_yield", false)):
        return ""
    var food := float(model.get("rate", 0.0))
    var trade := float(model.get("trade_rate", 0.0))
    if not SourceForecast.has_component(food) and SourceForecast.has_component(trade):
        return FoodIcons.TRADE_GOODS_GLYPH + SourceForecast.format_signed(trade)
    return SourceForecast.format_signed(food)

## The inspector's one-sentence readout: rate · the floor in WORDS · status · assigned workers.
func _work_inspector_sentence(model: Dictionary) -> String:
    var parts: Array[String] = []
    if bool(model.get("has_yield", false)):
        # Both products, each only when non-zero (issue #337): an inedible quarry's sentence leads with
        # its trade rate instead of asserting "+0.00 /turn".
        parts.append(SourceForecast.yield_components(
            float(model.get("rate", 0.0)), float(model.get("trade_rate", 0.0))))
    # The floor as the player set it — `50% left standing`, the same phrasing the picker's tooltips
    # and the slider caption use, so one number is never worded two ways.
    parts.append(HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(
        float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))))
    parts.append(HudFormat.status_label(FoodIcons.STATUS_PENDING if bool(model.get("pending", false)) \
        else FoodIcons.STATUS_WORKING))
    parts.append(HudWorkVocab.WORK_INSPECT_ASSIGNED_FORMAT % int(model.get("workers", 0)))
    return HudWorkVocab.WORK_INSPECT_SENTENCE_SEPARATOR.join(parts)

# ---- work-zone models + state ----------------------------------------------

## One dict per worked source, carrying everything the row, the chips and the inspector need — built
## ONCE per render off `_band_labor.effective_worker_map` (confirmed + optimistic pending), so the board, the
## chip counts and the totals can never disagree.
func _work_source_models(band: Dictionary, idle: int) -> Array:
    var models: Array = []
    var merged := _band_labor.effective_worker_map(band)
    for key in merged:
        var m: Dictionary = merged[key]
        var kind := String(m.get("kind", "")).strip_edges().to_lower()
        var workers := int(m.get("workers", 0))
        var pending := bool(m.get("pending", false))
        if not (kind == SourceForecast.LABOR_KIND_FORAGE or kind == SourceForecast.LABOR_KIND_HUNT):
            continue
        if workers <= 0 and not pending:
            continue
        var yld := SourceForecast.source_yield_readout(m, kind)
        var x := int(m.get("x", -1))
        var y := int(m.get("y", -1))
        var herd_id := String(m.get("herd_id", ""))
        var floor := SourceForecast.clamp_floor(
            float(m.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))
        # THE SECOND AXIS (issue #442) — what this crew is BUILDING here, "" for nothing. It is what
        # the rung marks and the herding-crew floor key on; the escapement floor is purely how hard
        # this crew pulls.
        var improvement := String(m.get("improvement", "")).strip_edges().to_lower()
        var icon := ""
        # The row's bundled ART, resolved BESIDE the emoji rather than instead of it (issue #439):
        # the emoji stays as the fallback, it is not replaced. BOTH webs fill it, and that is
        # deliberate — hunt and forage rows share ONE list and ONE icon column, so spriting only the
        # hunt half would leave a board that is half art and half emoji, a new inconsistency
        # introduced by the fix. `null` where the client has no art for this source, which is the
        # case `HudWidgets.build_marker_icon` renders the glyph for.
        var icon_texture: Texture2D = null
        var label := ""
        var cap := {}
        var live_herd := {}
        var patch := {}
        if kind == SourceForecast.LABOR_KIND_FORAGE:
            # The board draws the glyph in its OWN fixed column, so it takes the RAW icon — not
            # `HudFormat.source_icon_prefix`, which welds it to the label with a trailing space for the
            # single-label row this replaced.
            icon = _band_labor.food_module_icon(x, y)
            icon_texture = _band_labor.food_module_sprite(x, y)
            # Held in a local because the RUNG mark reads it too — `forage_patch_lookup` spells its keys
            # BARE (`is_cultivated` / `is_field`), unlike the `patch_`-prefixed `tile_info` cross-ref.
            patch = _band_labor.forage_patch_lookup().get(Vector2i(x, y), {})
            # THE ROW'S VERB FOLLOWS THE STANDING RUNG, through the same `HudFormat.plant_crew_label`
            # the compose sheet's noun does: a crew on a Tended Patch or a Field is TENDING, not
            # foraging, so `Forage (27, 26)` would name an activity the sim does not run there. The
            # rung MARK beside it answers a different question (what the source IS) and both stay.
            label = String(HudWorkVocab.WORK_ROW_PLANT_FORMATS.get(
                HudFormat.plant_crew_label(patch, HudComposeVocab.BARE_FORECAST_PREFIX), "")) % [x, y]
            # THE ROW'S OWN IMPROVEMENT DIPS ITS CEILING, and its rung's build crew floors the count —
            # the plant twins of the herd branch below, and the same reason: while a build runs the sim
            # caps the take at `stance ceiling × buildFraction` and asks for `max(build crew, take
            # crew)` hands. A stance-only forecast here let the board's `+` add workers the sim then
            # reported idle on the same row.
            cap = SourceForecast.source_worker_cap_state(SourceForecast.forecast_inputs(
                patch, SourceForecast.SOURCE_KIND_FORAGE,
                HudComposeVocab.BARE_FORECAST_PREFIX, floor, improvement), workers, idle,
                SourceForecast.plant_crew_floor(
                    patch, HudComposeVocab.BARE_FORECAST_PREFIX, improvement))
        else:
            var herd_label := _herd_label_for_id(herd_id)
            icon = FoodIcons.for_herd(herd_label)
            icon_texture = FaunaSprites.for_herd(herd_label)
            label = HudWorkVocab.WORK_ROW_HUNT_FORMAT % herd_label
            # Herds MIGRATE, so the cap reads the herd's LIVE dict from `_band_labor.world_herds()` rather than the
            # assignment's launch-time target.
            live_herd = _band_labor.find_world_herd(herd_id)
            # The IMPROVEMENT dips the ceiling here too (see the forage branch): a crew building a pen
            # is paid `stance × corralBuildFraction`, so a stance-only forecast caps this row above
            # what the sim will pay it.
            var hunt_forecast := SourceForecast.forecast_inputs(
                live_herd, SourceForecast.SOURCE_KIND_HERD,
                HudComposeVocab.BARE_FORECAST_PREFIX, floor, improvement)
            # A MANAGED herd's crew requirement floors this row's ceiling, exactly as it floors the
            # compose stepper's — otherwise the row renders the under-herded ⚠ below and disables the
            # `+` that would clear it. `SourceForecast.herd_crew_floor` is the one definition of the
            # number; the forage branch above passes none, a patch owing no crew.
            cap = SourceForecast.source_worker_cap_state(hunt_forecast, workers, idle,
                SourceForecast.herd_crew_floor(
                    live_herd, improvement != SourceForecast.IMPROVEMENT_NONE))
        var note := String(yld.get("note", ""))
        var rung := _work_source_rung(kind, patch, live_herd)
        # THE RUNG ON OFFER — a third axis, orthogonal to both `marks` (the verb in flight) and
        # `rung_glyph` (the rung the source STANDS on). Same `RungGates` answer the map's badge and the
        # compose sheet's gates use, so the three surfaces cannot disagree about what is climbable.
        var rung_source: Dictionary = patch if kind == SourceForecast.LABOR_KIND_FORAGE else live_herd
        # A rung UNDER WAY takes the slot from a rung on OFFER — they are one axis in two states, and
        # mutually exclusive by construction (`next_rung_ready` excludes the verb in flight).
        var building := RungGates.rung_in_progress(kind, rung_source, improvement)
        var ready := {} if not building.is_empty() \
            else RungGates.next_rung_ready(kind, rung_source, improvement, _player_knowledge())
        # The row's HARVEST mark is the floor's ZONE glyph — where this crew's floor sits relative to
        # the food peak. A continuous number cannot wear one glyph per value, and the zone is the whole
        # of what one mark can honestly say about it; the exact percent is in the row tooltip.
        var marks := FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
        if bool(yld.get("warn", false)):
            marks += " " + HudComposeVocab.OVERHUNT_FLAG
        # UNDER-CONTAINED managed herd (fauna neglect-escape arc): fewer herders staffed than the herd
        # needs → it sheds whole animals into the wild. Flag it wherever the herd is LISTED, not only in
        # its drawer, with the established overhunt ⚠. `assigned_herders_for` is the SAME actual/staged
        # count the herd drawer reads, so the panel and the drawer can never disagree about it.
        var herders_needed := int(live_herd.get("herders_needed", 0))
        var under_herded := herders_needed > 0 \
            and _band_labor.assigned_herders_for(herd_id) < herders_needed
        if under_herded:
            if not marks.contains(HudComposeVocab.OVERHUNT_FLAG):
                marks += " " + HudComposeVocab.OVERHUNT_FLAG
            if note == "":
                note = HudWorkVocab.WORK_ROW_UNDER_HERDED_NOTE
        models.append({
            "key": String(key), "kind": kind, "icon": icon, "icon_texture": icon_texture,
            "label": label,
            "rate": float(yld.get("rate", 0.0)),
            # The row's TRADE component (issue #337), 0 when the source pays none. Carried so the
            # inspector sentence states the same two products the row headline does.
            "trade_rate": float(yld.get("trade_rate", 0.0)), "has_yield": bool(m.get("has_yield", false)),
            "workers": workers, "pending": pending, "warn": bool(yld.get("warn", false)),
            "under_herded": under_herded,
            "note": note, "muted_note": String(yld.get("muted_note", "")), "marks": marks,
            # The source's STANDING RUNG — orthogonal to `marks`, which carries the verb in flight.
            "rung_glyph": String(rung.get("glyph", "")), "rung_tooltip": String(rung.get("tooltip", "")),
            # The rung this source could climb NOW ("" for none) — see `ready` above.
            "ready_policy": String(ready.get("policy", "")), "ready_glyph": String(ready.get("glyph", "")),
            # The rung it is BUILDING right now, and how far in. The board shows the percent the map
            # badge shows, off the same `RungGates` answer.
            "building_policy": String(building.get("policy", "")),
            "building_glyph": String(building.get("glyph", "")),
            "building_progress": float(building.get("progress", 0.0)),
            "floor": floor, "improvement": improvement, "x": x, "y": y, "herd_id": herd_id,
            "can_add": bool(cap.get("can_add", idle > 0)),
            "schedule": HudBandLaborState.as_schedule(m.get("arrival_schedule", null)),
            "tooltip": HudFormat.join_tooltip_lines([String(yld.get("tooltip", "")),
                HudFormat.floor_hint(floor, kind), String(cap.get("note", "")),
                "" if ready.is_empty() else HudWorkVocab.WORK_ROW_READY_TOOLTIP_FORMAT % HudFormat.policy_face(String(ready.get("policy", ""))),
                "" if building.is_empty() else HudWorkVocab.WORK_ROW_BUILDING_TOOLTIP_FORMAT % [
                    HudFormat.policy_face(String(building.get("policy", ""))),
                    HudFormat.progress_percent(float(building.get("progress", 0.0)))],
                HudWorkVocab.WORK_ROW_OPEN_HINT]),
            # A source wants attention when it overdraws, wastes workers, or is still unacknowledged.
            "attention": bool(yld.get("warn", false)) or note != "" or pending,
        })
    return models

## The source's STANDING RUNG as `{glyph, tooltip}` — `{}` for WILD ground / a wild herd, which is the
## honest default and keeps the common row unmarked (see `HudWorkVocab.WORK_ROW_RUNG_TENDED_TOOLTIP`).
##
## **THE HIGHER RUNG IS TESTED FIRST, and that ordering is load-bearing**: a Field is ALSO
## `is_cultivated` and a penned herd is ALSO fully domesticated, so testing rung 2 first would mark
## every rung-3 source as a rung-2 one — collapsing exactly the distinction this mark exists to draw.
##
## The dicts are the RAW wire ones (`forage_patch_lookup` / `world_herds`), so every key is spelled
## BARE. Do NOT reach for the `patch_`-prefixed `tile_info` spellings here.
func _work_source_rung(kind: String, patch: Dictionary, herd: Dictionary) -> Dictionary:
    if kind == SourceForecast.LABOR_KIND_FORAGE:
        var crop := String(patch.get("committed_display_name", "")).strip_edges()
        if bool(patch.get("is_field", false)):
            return {
                "glyph": DetailFormat.field_glyph(),
                "tooltip": HudWorkVocab.WORK_ROW_RUNG_FIELD_TOOLTIP if crop == "" \
                    else HudWorkVocab.WORK_ROW_RUNG_FIELD_CROP_FORMAT % crop,
            }
        if bool(patch.get("is_cultivated", false)):
            return {
                "glyph": DetailFormat.CULTIVATION_GLYPH,
                "tooltip": HudWorkVocab.WORK_ROW_RUNG_TENDED_TOOLTIP if crop == "" \
                    else HudWorkVocab.WORK_ROW_RUNG_TENDED_CROP_FORMAT % crop,
            }
        return {}
    if bool(herd.get("corralled", false)):
        return {"glyph": DetailFormat.CORRAL_GLYPH, "tooltip": HudWorkVocab.WORK_ROW_RUNG_PENNED_TOOLTIP}
    if float(herd.get("domestication", 0.0)) >= DetailFormat.HUSBANDRY_PROGRESS_COMPLETE:
        return {"glyph": DetailFormat.pastoral_glyph(), "tooltip": HudWorkVocab.WORK_ROW_RUNG_PASTORAL_TOOLTIP}
    return {}

## Reset a filter that now selects nothing back to `All`. A kind/attention chip is hidden once its set
## empties (the last herd is unassigned, the last ⚠ clears), so a standing filter would otherwise
## strand the player on an empty board with no chip left to press to get back out of it.
func _reconcile_work_filter(models: Array) -> void:
    if _work_filter == HudWorkVocab.WORK_FILTER_ALL:
        return
    if _work_models_matching(_work_filter, models).is_empty():
        _work_filter = HudWorkVocab.WORK_FILTER_ALL

func _filter_work_models(models: Array) -> Array:
    return _work_models_matching(_work_filter, models)

func _work_models_matching(filter: StringName, models: Array) -> Array:
    match filter:
        HudWorkVocab.WORK_FILTER_FORAGE:
            return models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_FORAGE)
        HudWorkVocab.WORK_FILTER_HUNT:
            return models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_HUNT)
        HudWorkVocab.WORK_FILTER_ATTENTION:
            return models.filter(func(m): return bool(m["attention"]))
        HudWorkVocab.WORK_FILTER_READY:
            return models.filter(func(m): return String(m["ready_policy"]) != "")
    return models.duplicate()

func _sort_work_models(models: Array) -> void:
    if _work_sort == HudWorkVocab.WORK_SORT_NAME:
        models.sort_custom(_work_name_sorts_before)
    else:
        models.sort_custom(func(a, b): return _work_sorts_before(a as Dictionary, b as Dictionary))

## "Sort by name" — KIND FIRST, then label, then `key`.
##
## **THE LABEL PREFIX IS NOT A PROXY FOR THE KIND, so alphabetical order alone SPLITS A KIND IN TWO.**
## A forage row whose Cultivate improvement is done renders through `WORK_ROW_TEND_FORMAT`
## ("Tend (%d, %d)"), which is display only — its `kind` is still `forage`. With three live prefixes
## and "Forage" < "Hunt" < "Tend", a band working a wild patch, a herd and a Tended Patch would read
## Forage → Hunt → Tend, i.e. the forage block interrupted by the hunt block. The `Forage`/`Hunt`
## filter chips select on `kind` (`_work_models_matching`), so the unsorted-by-kind board would not
## match the blocks those chips name. Leading with the kind makes the board agree with the chips
## whatever a row's label says.
##
## The `key` tiebreak makes it a TOTAL ORDER. `sort_custom` is NOT stable in Godot and a label tie is
## genuinely reachable — two herds of the same species render the identical `WORK_ROW_HUNT_FORMAT`
## label — so without it two tied rows could swap on any unrelated re-render (a snapshot tick, a zone
## resize), which is the same row-jumps-under-the-pointer failure the default sort exists to remove.
## `key` is the source identity `_work_source_models` already assigns, i.e. the one available field no
## game state moves.
func _work_name_sorts_before(a: Dictionary, b: Dictionary) -> bool:
    # A BOOLEAN TIER, the same idiom `_work_sorts_before` uses, because there are exactly two labor
    # kinds. A third kind cannot be expressed this way — it would need an explicit rank table, since
    # a bool can only say "this one first".
    var a_is_forage := String(a.get("kind", "")) == SourceForecast.LABOR_KIND_FORAGE
    var b_is_forage := String(b.get("kind", "")) == SourceForecast.LABOR_KIND_FORAGE
    if a_is_forage != b_is_forage:
        return a_is_forage
    var by_label := String(a.get("label", "")).naturalnocasecmp_to(String(b.get("label", "")))
    if by_label != 0:
        return by_label < 0
    return String(a.get("key", "")) < String(b.get("key", ""))

## "Sort by yield", in TWO TIERS (issue #337): every FOOD-paying source first, ordered by its food
## figure descending, then the trade-only sources, ordered by their trade figure descending.
##
## **THIS IS NOT A RAW MAGNITUDE SORT, AND MUST NOT BE "FIXED" INTO ONE.** Ranking a wolf's 0.22 trade
## above a patch's 0.15 food compares two quantities the sim publishes NO exchange rate between, and
## under a control labelled "sort by yield" that ordering asserts the wolf is the more productive
## source — a claim the game does not make and the player cannot check. Tiering asserts nothing about
## an exchange rate; it only fixes the ORDER OF ATTENTION.
##
## Why food takes the first tier is NOT "food is worth more per unit". It is that the larder is the
## live survival constraint the player is deciding against every turn, while trade is still
## economically thin — the design doc's own Deferred section says trade goods do little yet, and this
## arc commits only to PRODUCING them honestly. Revisit the tiering when trade acquires a sink, not
## before. (Sorting on food ALONE was the original bug: it interleaved trade-only sources among the
## zero-food rows at the bottom of the board, off page one on a busy band, which is the same "an
## inedible quarry is worth nothing" reading the per-row work removed.)
##
## A source paying NEITHER component sorts into the trade tier at 0.0, i.e. last — unchanged.
##
## **THE `key` TIEBREAK MAKES IT A TOTAL ORDER, and that is a correctness fix**: `sort_custom` is NOT
## stable in Godot and equal rates are common — two patches at the same food figure inside the food
## tier, and every source paying neither component, all of which sit together at 0.0 in the trade
## tier. Tied rows could otherwise swap on any unrelated re-render. The tiebreak rides BELOW the tier
## + rate comparisons and changes neither.
func _work_sorts_before(a: Dictionary, b: Dictionary) -> bool:
    var a_pays_food := SourceForecast.has_component(float(a.get("rate", 0.0)))
    var b_pays_food := SourceForecast.has_component(float(b.get("rate", 0.0)))
    if a_pays_food != b_pays_food:
        return a_pays_food
    # Exact `!=` rather than `is_equal_approx`: an epsilon tie test is NOT transitive (a≈b and b≈c
    # without a≈c), which would break the strict weak ordering `sort_custom` requires — the very
    # property this tiebreak exists to establish.
    if a_pays_food:
        if float(a.get("rate", 0.0)) != float(b.get("rate", 0.0)):
            return float(a.get("rate", 0.0)) > float(b.get("rate", 0.0))
        return String(a.get("key", "")) < String(b.get("key", ""))
    if float(a.get("trade_rate", 0.0)) != float(b.get("trade_rate", 0.0)):
        return float(a.get("trade_rate", 0.0)) > float(b.get("trade_rate", 0.0))
    return String(a.get("key", "")) < String(b.get("key", ""))

func _find_work_model(models: Array, key: String) -> Dictionary:
    if key == "":
        return {}
    for m in models:
        if String((m as Dictionary).get("key", "")) == key:
            return m
    return {}

## Re-send this source's `assign_labor` at a new worker count (and optionally a new policy) — the
## same emit the old Current-actions stepper made.
##
## **THE IMPROVEMENT RIDES EVERY CREW EDIT** (issue #442). `assign_labor` deliberately does not touch
## the second axis, so a `+`/`−`/Unassign/stance pick that let the pending overlay default to
## `IMPROVEMENT_NONE` would blank the axis for the rest of the turn: the row's build badge and its
## `⌃`-vs-progress slot would flip back to advertising the very rung already under way, and
## `herd_crew_floor` would drop from `herders_needed_if_managed` to the ownership-gated
## `herders_needed`, capping the `+` below the keepers the sim demands. The row MODEL already carries
## the value `effective_worker_map` resolved (confirmed assignment overlaid with any pending edit), so
## it is restated from there rather than re-derived — re-deriving could disagree with the board the
## player is clicking on.
## `floor` defaults to `RESTATE_STANDING_FLOOR` — a sentinel outside the legal `0..1` range, so
## "leave the floor alone" is expressible on an axis where every real value including `0` is a
## meaningful choice. A crew-size edit must not silently re-point the crew.
const RESTATE_STANDING_FLOOR := -1.0

func _emit_work_assign(band: Dictionary, model: Dictionary, workers: int,
        floor: float = RESTATE_STANDING_FLOOR) -> void:
    var kind := String(model.get("kind", ""))
    var standing := float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))
    _emit_assign_labor(band, kind, workers, int(model.get("x", -1)), int(model.get("y", -1)),
        String(model.get("herd_id", "")),
        standing if floor == RESTATE_STANDING_FLOOR else floor,
        "", String(model.get("improvement", "")))

## Jump the map to a worked source — a fixed forage tile, or a herd at its LIVE (migrated) tile.
func _focus_work_source(model: Dictionary) -> void:
    if String(model.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        _focus_hunt_source(String(model.get("herd_id", "")), int(model.get("x", -1)), int(model.get("y", -1)))
    else:
        focus_labor_source(int(model.get("x", -1)), int(model.get("y", -1)))

## One inspector row at a time — opening a second closes the first (and opening one costs the board
## rows, which is why `_work_board_capacity` subtracts the strip's height).
func _toggle_work_inspector(key: String) -> void:
    _work_open_key = "" if _work_open_key == key else key
    _work_floor_open = false
    _repage_work_zone()

func _set_work_filter(filter: StringName) -> void:
    if _work_filter == filter:
        return
    _work_filter = filter
    _work_page = 0
    _repage_work_zone()

func _set_work_sort(sort: StringName) -> void:
    if _work_sort == sort:
        return
    _work_sort = sort
    # A sort is a standing preference, not a per-session mood — persist it through the panel, which
    # owns the prefs file.
    if _panel != null:
        _panel.set_work_sort_pref(String(sort))
    _work_page = 0
    _repage_work_zone()

func _step_work_page(delta: int) -> void:
    _work_page = maxi(_work_page + delta, 0)
    _repage_work_zone()

## The Work menu's destructive entry. Scoped `work`: Forage + Hunt only — standing roles, parties and
## an in-progress move are untouched, which is exactly what the confirm promises.
func _on_work_unassign_all_pressed(band: Dictionary, count: int) -> void:
    if band.is_empty() or count <= 0:
        return
    _confirm_destructive(HudWorkVocab.WORK_UNASSIGN_CONFIRM_FORMAT % count, HudWorkVocab.WORK_UNASSIGN_CONFIRM_OK,
        func() -> void: _emit_cancel_order(band, HudComposeVocab.CANCEL_SCOPE_WORK))

## Clear labor for a band at `scope` (`all` / `work` / `roles`). Main formats the
## `cancel_order <faction> <band> <scope>` command.
func _emit_cancel_order(band: Dictionary, scope: String) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band, scope)

# ---- zone `parties` ---------------------------------------------------------

## Zone `parties`: head + `⋯` menu · one row per party in the field · the compose footer.
func build_parties_zone(band: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    var parties := _band_labor.band_parties(band)
    var menu := HudWidgets.build_section_menu([
        {"label": HudComposeVocab.PARTY_RECALL_ALL_FORMAT % parties.size(), "disabled": parties.is_empty(),
            "on_pick": func() -> void: _on_recall_all_parties_pressed(parties)},
    ], HudComposeVocab.PARTY_MENU_TOOLTIP)
    col.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PARTIES,
        HudComposeVocab.PARTIES_HEADER_FORMAT % [parties.size(), _band_labor.band_party_workers(band)], menu))
    if parties.is_empty():
        col.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.PARTIES_EMPTY_HINT))
    else:
        for exp in parties:
            col.add_child(_build_party_row(exp))
    # Order: rows → inspector (if open) → an EXPAND_FILL spacer → footer, so the Scout/Hunt footer
    # stays pinned to the BOTTOM of the zone with the strip sitting under the clicked row (the strip is
    # a row → detail disclosure, the parties twin of the work board's inspector). Drop a strip pinned to
    # a party that has left the list (recalled, moved to another band), mirroring `_fill_work_zone`'s
    # stale-key clear. The strip's own line separation is tightened (PARTIES_INSPECTOR_LINE_SEPARATION)
    # so strip + a row + the pinned footer still fit the height-capped T/B parties zone.
    var inspected := _party_by_open_key(parties)
    if inspected.is_empty():
        _party_open_key = ""
    else:
        col.add_child(_build_parties_inspector(inspected))
    var spacer := Control.new()
    spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    col.add_child(spacer)
    col.add_child(_build_party_footer(band))
    return col

## The party in `parties` whose entity matches `_party_open_key`, or `{}` when none is open / the open
## one has left the list (the caller then clears the stale key).
func _party_by_open_key(parties: Array) -> Dictionary:
    if _party_open_key == "":
        return {}
    for exp_variant in parties:
        if exp_variant is Dictionary:
            var exp: Dictionary = exp_variant
            if str(int(exp.get("entity", -1))) == _party_open_key:
                return exp
    return {}

## Toggle the parties inspector strip open/closed for `key` (an expedition entity as a string), then
## re-render the parties zone in place — the same path the footer mission buttons already drive.
func _toggle_parties_inspector(key: String) -> void:
    _party_open_key = "" if _party_open_key == key else key
    rerender()

## The parties inspector strip — the full Mission/Target/Policy/Phase/Carried/Next-delivery/Position
## detail for one party, opened by a row click. Mirrors `_build_work_inspector`: a titled header with a
## close `✕`, the detail lines as dim status parts, and inline Jump/Recall links.
func _build_parties_inspector(exp: Dictionary) -> PanelContainer:
    var strip := PanelContainer.new()
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudComposeVocab.PARTIES_INSPECTOR_LINE_SEPARATION)
    strip.add_child(col)
    var entity := int(exp.get("entity", -1))
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var title := Label.new()
    title.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id)
    title.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudWorkVocab.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    head.add_child(close)
    col.add_child(head)
    for line in _banddetail.expedition_summary_lines(exp):
        col.add_child(HudWidgets.build_status_part(line, HudStyle.INK_DIM))
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", HudWorkVocab.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudComposeVocab.PARTY_INSPECT_JUMP, HudStyle.INK, func() -> void:
        select_expedition(entity, x, y)))
    links.add_child(HudWidgets.build_inline_link(HudComposeVocab.PARTY_INSPECT_RECALL, HudStyle.DANGER, func() -> void:
        confirm_recall_expedition(exp)))
    col.add_child(links)
    return strip

## One party row: mission glyph · subject · phase chip · an always-visible recall `✕` (dimmed at rest,
## bright on hover) as the quick removal path. Clicking the row BODY toggles the parties inspector
## strip (the full Mission/Target/…/Next-delivery detail), mirroring the work board's row → inspector.
func _build_party_row(exp: Dictionary) -> HBoxContainer:
    var phase := HudFormat.expedition_phase_key(exp)
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var body := Button.new()
    body.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id)
    body.alignment = HORIZONTAL_ALIGNMENT_LEFT
    body.focus_mode = Control.FOCUS_NONE
    body.clip_text = true
    body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(body, "ghost")
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        body.add_theme_color_override("font_color", HudStyle.WARN)
    body.tooltip_text = DetailFormat.expedition_row_tooltip(
        exp, phase, _band_labor.expedition_target_herd(exp))
    var entity := int(exp.get("entity", -1))
    body.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    row.add_child(body)
    var recall := Button.new()
    recall.text = HudComposeVocab.PARTY_RECALL_GLYPH
    recall.focus_mode = Control.FOCUS_NONE
    recall.tooltip_text = HudComposeVocab.PARTY_RECALL_TOOLTIP
    recall.custom_minimum_size = Vector2(HudComposeVocab.PARTY_RECALL_WIDTH, 0.0)
    HudStyle.apply_button(recall, "ghost")
    # DANGER-red like the Work inspector's destructive "Unassign" link — it removes a party. The steady
    # red already reads as destructive, so it rests at full opacity (no alpha dim) and brightens no
    # further on hover. Confirms before recalling (its own single-party prompt, NOT the raw emit).
    recall.add_theme_color_override("font_color", HudStyle.DANGER)
    recall.pressed.connect(func() -> void: confirm_recall_expedition(exp))
    row.add_child(recall)
    return row

## Confirm a SINGLE party's recall, then emit. Wraps the button handlers (row ✕, inspector Recall,
## drawer Recall) — NOT the shared `_on_recall_expedition_pressed` emit, which "Recall all" loops under
## its own one confirm. The prompt names the party (hunt → its herd, scout → the mission word).
func confirm_recall_expedition(exp: Dictionary) -> void:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var label := _herd_label_for_id(String(exp.get("expedition_target_herd", "")).strip_edges()) \
        if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT \
        else HudComposeVocab.PARTY_RECALL_SCOUT_LABEL
    _confirm_destructive(HudComposeVocab.PARTY_RECALL_ONE_CONFIRM_FORMAT % label, HudComposeVocab.PARTY_RECALL_ONE_CONFIRM_OK,
        func() -> void: _on_recall_expedition_pressed(exp))

## Recall every party in one go — there is no bulk verb on the wire and parties are few, so this is
## one `recall_expedition` per party through the existing signal.
func _on_recall_all_parties_pressed(parties: Array) -> void:
    if parties.is_empty():
        return
    _confirm_destructive(HudComposeVocab.PARTY_RECALL_CONFIRM_FORMAT % parties.size(), HudComposeVocab.PARTY_RECALL_CONFIRM_OK,
        func() -> void:
            for exp in parties:
                _on_recall_expedition_pressed(exp))

## The parties footer: the two missions offered DIRECTLY (Scout / Hunt), each opening the compose
## sheet already on that mission, or the compose sheet in their place. With no idle workers the
## buttons stay VISIBLE and DISABLED with their reason — the section vanishing is what made
## expeditions look like they had been removed from the game.
func _build_party_footer(band: Dictionary) -> VBoxContainer:
    var idle := _band_labor.effective_idle(band)
    var foot := HudWidgets.make_zone_block()
    if _party_compose_open and _party_compose_mission != "" and idle > 0:
        foot.add_child(_build_compose_sheet(band, idle))
        return foot
    var missions := HBoxContainer.new()
    missions.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_SCOUT,
        HudComposeVocab.COMPOSE_MISSION_LABEL_SCOUT, HudComposeVocab.SEND_EXPEDITION_HINT, idle))
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_HUNT,
        HudComposeVocab.COMPOSE_MISSION_LABEL_HUNT, HudComposeVocab.SEND_HUNT_EXPEDITION_HINT, idle))
    # **THE THIRD VERB** (`docs/plan_denial_raid.md` §3). It sits beside the other two rather than
    # inside the hunt form, because what it changes is a BOUND and not a number: `floor = 0` still
    # only kills what the party can haul, so denial had to become a mission to have anything to
    # unclamp. Same button, same idle gate — the difference is entirely in the form it opens.
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_DENY,
        HudComposeVocab.COMPOSE_MISSION_LABEL_DENY, HudComposeVocab.SEND_DENIAL_RAID_HINT, idle))
    foot.add_child(missions)
    if idle <= 0:
        foot.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.SEND_PARTY_NO_IDLE_REASON))
    return foot

## One footer mission button: opens the compose sheet already committed to `mission`.
func _build_mission_launch_button(mission: String, label: String, hint: String,
        idle: int) -> Button:
    var btn := Button.new()
    btn.text = label
    btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(btn, "primary")
    btn.tooltip_text = hint
    btn.disabled = idle <= 0
    btn.pressed.connect(func() -> void:
        _party_compose_open = true
        _party_compose_mission = mission
        # A fresh compose act starts with no quarry — never a herd left over from a cancelled one.
        _clear_party_quarry()
        rerender())
    return btn

## The compose sheet. The mission is already settled by the footer button that opened it, so the
## sheet titles itself by mission and the policy picker is unreachable except under Hunt (it used to
## sit above the scouting button and read as if it modified it). `✕` is the only way back.
func _build_compose_sheet(band: Dictionary, idle: int) -> VBoxContainer:
    var is_hunt := _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_HUNT
    var is_deny := _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_DENY
    var sheet := HudWidgets.make_zone_block()
    var head := HBoxContainer.new()
    var title := Label.new()
    title.text = HudComposeVocab.COMPOSE_TITLE_SCOUT
    if is_hunt:
        title.text = HudComposeVocab.COMPOSE_TITLE_HUNT
    elif is_deny:
        title.text = HudComposeVocab.COMPOSE_TITLE_DENY
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var cancel := Button.new()
    cancel.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    cancel.focus_mode = Control.FOCUS_NONE
    cancel.tooltip_text = HudComposeVocab.COMPOSE_CANCEL_TOOLTIP
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(func() -> void:
        _close_party_compose())
    head.add_child(cancel)
    sheet.add_child(head)
    if is_hunt:
        _fill_hunt_compose_sheet(sheet, band, idle)
        return sheet
    if is_deny:
        _fill_denial_compose_sheet(sheet, band, idle)
        return sheet
    # SCOUT — a single input. Its only question is party size, and nothing about a scouting party
    # depends on where it is going, so the destination is still picked on the map after the send.
    var party_max := _scout_party_max(band, idle)
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.SEND_EXPEDITION_HINT))
    var confirm := Button.new()
    confirm.text = HudComposeVocab.SEND_EXPEDITION_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(confirm, "primary")
    confirm.tooltip_text = HudComposeVocab.SEND_EXPEDITION_HINT
    confirm.pressed.connect(func() -> void:
        _close_party_compose()
        _targeting.begin_send_expedition(band, _send_expedition_count))
    sheet.add_child(confirm)
    return sheet

## The HUNT form, in the order the decision is actually made: QUARRY → POLICY → PARTY → forecast →
## send. The quarry leads because it is what makes every field under it answerable — the per-policy
## metrics on the picker, the max-useful party cap, the trip forecast and the no-surplus verdict are
## all functions of the herd. Every one of those comes from the SAME helper the herd drawer's
## beyond-reach branch uses, so the two entry points cannot quote different numbers.
func _fill_hunt_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    # Re-resolve the quarry LIVE each render: a herd can be hunted out or leave the snapshot while the
    # sheet is open, and rendering a form against a stale id would forecast a herd that is gone. A herd
    # that MIGRATES into the band's hunt reach fails for the same reason — it is no longer a party's
    # job — so it falls back to the `Choose…` empty state rather than forecasting a raid the player
    # should not make.
    var herd := _band_labor.find_world_herd(_compose.party_quarry_id())
    if herd.is_empty() or not _targeting.is_expedition_quarry(band, herd):
        herd = {}
        _clear_party_quarry()
    sheet.add_child(_build_quarry_row(band, herd))
    if _compose.party_quarry_id() == "":
        # Visible-and-disabled-with-its-reason, the same convention as the idle-0 footer: the send is
        # shown so the shape of the form is legible, and it says why it is not yet pressable.
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = SourceForecast.SEND_HUNTING_EXPEDITION_BUTTON
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = HudComposeVocab.COMPOSE_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.COMPOSE_FIELD_POLICY))
    # With a herd in hand the presets finally carry their metric — the same
    # `SourceForecast.expedition_policy_takes` the herd drawer feeds its picker. **NO SLIDER in this
    # zone**, for the reason the work inspector has none: a fixed-width dock strip is not where a
    # continuous dial belongs, and the herd drawer's own sheet has the room.
    sheet.add_child(HudWidgets.build_floor_picker(func(floor: float) -> void:
        _send_hunt_floor = floor
        # Auto-max on a floor click, exactly as the herd drawer does: "give me everything this herd
        # can spare" — zero waste, full rate. Consumed on the next rebuild, never set by a −/+ tick.
        _compose.arm_party_autofill()
        rerender(), _send_hunt_floor,
        SourceForecast.expedition_policy_takes(band, herd, _band_labor.grid_width(), _band_labor.wrap_horizontal()), HudWorkVocab.ZONE_POLICY_PICKER_COLUMNS))
    sheet.add_child(HudWidgets.alloc_hint_label(
        HudFormat.floor_hint(_send_hunt_floor, SourceForecast.LABOR_KIND_HUNT, true)))
    # Party size, capped at the raid's max-useful plateau for THIS herd + floor (the herd drawer's
    # own cap), so extra hunters can no longer be sent to stand idle at the kill.
    var assignable := _scout_party_max(band, idle)
    var capped := SourceForecast.expedition_useful_cap(band, herd, _send_hunt_floor, assignable)
    var cap: int = maxi(int(capped["cap"]), HudConst.WORKER_STEP)
    if _compose.consume_party_autofill():
        _send_expedition_count = cap
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, cap)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, cap,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, cap)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    var cap_note := String(capped["note"])
    if cap_note != "":
        sheet.add_child(HudWidgets.alloc_hint_label(cap_note))
    # **THE FILL TARGET, under the party it is priced by** (§5.2) — the same control and the same
    # composed axis the herd drawer's expedition branch renders, so neither entry point can offer
    # orders the other cannot. Its axis comes off the UNTARGETED raid, and the clamped value is
    # written straight back so the control, the forecast and the launch payload are one number.
    var fill_target_model := SourceForecast.raid_fill_target_model(band, herd, _send_hunt_floor,
        _send_expedition_count, _band_labor.grid_width(), _band_labor.wrap_horizontal(),
        _compose.party_fill_target())
    _compose.set_party_fill_target(int(fill_target_model.get("target", SourceForecast.NO_FILL_TARGET)))
    if bool(fill_target_model.get("available", false)):
        sheet.add_child(HudWidgets.build_fill_target_control(fill_target_model,
            func(new_target: int) -> void:
                _compose.set_party_fill_target(new_target)
                rerender()))
    # LIVE raid forecast for the quarry + floor + party + target now dialed — the same trip lookup and
    # the same one-line renderer the herd drawer uses.
    var trip := SourceForecast.hunt_trip_forecast(band, herd, _send_hunt_floor, _send_expedition_count,
        _band_labor.grid_width(), _band_labor.wrap_horizontal(), _compose.party_fill_target())
    var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip, SourceForecast.herd_display_name(herd))
    if forecast_line != "":
        sheet.add_child(HudWidgets.forecast_label(forecast_line))
    # **WHICH STOP ENDS THE TRIP, as its own quiet line.** This zone's forecast is the ONE-LINE form,
    # which is already dense with five facts; the herd drawer folds the same clause into its readout
    # verdict instead. Both read `SourceForecast.trip_bound_clause`, so the two surfaces cannot
    # describe one stop differently, and a forecast carrying no bound renders no line at all.
    var bound_clause := SourceForecast.trip_bound_clause(trip)
    if bound_clause != "":
        sheet.add_child(HudWidgets.alloc_hint_label(bound_clause))
    # WHY an empty raid is empty comes off the sim's `bound`, so the reason takes the TRIP beside the
    # herd — "wait for the herd to rebuild" and "send more hunters" are opposite instructions.
    var returns_empty := SourceForecast.hunt_trip_returns_empty(trip)
    var reason := SourceForecast.hunt_empty_refusal_reason(trip, herd) if returns_empty else ""
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The button carries the verdict: slow/long/denial raids stay ENABLED and warn-styled, and only a
    # herd with no surplus disables. `SourceForecast.style_send_hunt_button` owns the text in every branch.
    SourceForecast.style_send_hunt_button(confirm, trip, reason)
    confirm.set_meta(HudWidgets.SEND_HUNT_CONFIRM_META, true)
    if returns_empty:
        sheet.add_child(HudWidgets.alloc_hint_label(reason))
    var quarry_id := _compose.party_quarry_id()
    confirm.pressed.connect(func() -> void:
        emit_signal("send_hunt_expedition_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": SourceForecast.herd_display_name(herd),
            "floor": _send_hunt_floor,
            "fill_target": _compose.party_fill_target(),
        })
        _close_party_compose())
    sheet.add_child(confirm)

## The DENIAL form (`docs/plan_denial_raid.md` §3): QUARRY → PARTY → the collapse verdict → send.
##
## **WHAT IS ABSENT IS THE SPECIFICATION.** No floor picker, no floor hint, no fill target, no crew
## preset, no max-useful cap — a denial party never stops engaging, so there is no escapement to dial
## and no pack to fill, and any of those controls would be a lever the command grammar
## (`send_denial_raid`, closed at four tokens) cannot even carry. The player chooses a herd and a
## party size; everything else on this sheet is a READOUT.
##
## The quarry row and its picker are the hunt form's, reused verbatim. **THE BEYOND-REACH RULE IS
## NOT**, and this is the one place the two missions genuinely differ about what a quarry is
## (`TargetingController.is_expedition_quarry`): a hunting party exists for game the band cannot work
## from home, so a nearer herd is a local hunt — but denial is not a way of GETTING food, it is a way
## of ERASING a herd, and hunting the warren next door at floor 0 cannot express that (a hunt is
## carry-bounded and stops at the pack). A denial raid may therefore name any herd the band can see
## and reach. It is still an EXPEDITION and deliberately not a labor assignment: the party detaches,
## spends turns killing and comes back, and it has no floor and no rate to put on the assign dialog.
func _fill_denial_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    # Re-resolved LIVE every render for the hunt form's reasons: a herd can be raided out or leave the
    # snapshot while the sheet is open, and a form rendered against a stale id would forecast a
    # collapse for a herd that is gone. **A herd that MIGRATES INTO REACH no longer clears the form** —
    # under denial that was never a reason to drop it.
    var herd := _band_labor.find_world_herd(_compose.party_quarry_id())
    if herd.is_empty() or not _targeting.is_expedition_quarry(band, herd,
            HudComposeVocab.COMPOSE_MISSION_DENY):
        herd = {}
        _clear_party_quarry()
    sheet.add_child(_build_quarry_row(band, herd))
    if _compose.party_quarry_id() == "":
        # Visible-and-disabled-with-its-reason, the footer's own convention.
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_DENY_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = String(SourceForecast.DENIAL_VERDICTS[
            SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["button"])
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = HudComposeVocab.COMPOSE_DENY_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    # **THE PARTY IS CAPPED BY THE BAND'S OWN IDLE WORKERS, AND BY NOTHING ELSE.** There is
    # deliberately no `expedition_useful_cap` twin here: that cap exists because a hunting raid's
    # delivered payload PLATEAUS once the herd's surplus binds, and a denial raid has no payload to
    # plateau. More hands always break the herd sooner, which is the whole lever this form offers.
    #
    # **`max_expedition_party_size` IS NOT A RULES CAP AND MUST NOT BE APPLIED HERE**
    # (`snapshot.fbs` → `denialEstimates`). It is the wire echo of `expedition_config.estimate_party_sizes`,
    # i.e. the SAMPLING AXIS of the estimate tables, and the sim deleted the rules cap for all three
    # launch verbs — so `_scout_party_max` was the last thing enforcing it, and a band with 16 idle
    # workers was clamped to 8 while this sheet told it to send more hunters. The hunt and scout
    # forms still call that helper; only denial reads the supply directly.
    var party_max := idle
    # **SEEDED ON THE SIM'S OWN REQUIREMENT, ONCE PER QUARRY.** Below `denialPartyNeeded` a raid
    # accomplishes literally nothing however long it runs, and nothing else on the sheet said which
    # number crossed that line — so the stepper opens there rather than on a guess. The one-shot is
    # the hunt form's `arm_party_autofill` (armed by `TargetingController.choose_quarry`, the ONE
    # adoption of a quarry on either route), so a manual −/+ tick survives every later rerender.
    #
    # **NEVER SEEDED TO 0.** `DENIAL_PARTY_NEEDED_NONE` means the sim quotes no party that drives this
    # herd down at all — it is not "send nobody" — so the count is left where it was and the verdict
    # line carries the answer. And the clamp to `party_max` is deliberate: a requirement ABOVE the
    # band's idle workers opens on the most it can field, which is honest, because the sheet shows
    # both numbers and the verdict still says it is not enough.
    if _compose.consume_party_autofill():
        var needed := SourceForecast.denial_party_needed(herd)
        if needed > SourceForecast.DENIAL_PARTY_NEEDED_NONE:
            _send_expedition_count = clampi(needed, HudConst.WORKER_STEP, party_max)
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    # THE COLLAPSE VERDICT — the sim's `denialEstimates` row for this party size, on the clock the
    # player is on. **The band and the grid pair are passed for the OUTBOUND WALK**: the table counts
    # raiding turns, and this sheet's hunt form has always headlined a round-trip total, so a verdict
    # quoting bare raiding turns beside it named a shorter span in the same words.
    var forecast := SourceForecast.denial_forecast(herd, _send_expedition_count, band,
        _band_labor.grid_width(), _band_labor.wrap_horizontal())
    var quarry_name := SourceForecast.herd_display_name(herd)
    var verdict := SourceForecast.denial_verdict_bbcode(forecast, quarry_name)
    if verdict != "":
        sheet.add_child(HudWidgets.forecast_label(verdict))
        # The caveat rides under the verdict WHENEVER THERE IS A NUMBER TO CAVEAT — the band is an
        # integral over many stochastic draws and a lucky run really can finish sooner than the
        # reported low. A verdict with no turn count (a repelled party, an unbounded horizon) has
        # nothing for it to qualify, and a caveat about an absent number reads as one that is there.
        if SourceForecast.denial_turns_phrase(forecast) != "":
            sheet.add_child(HudWidgets.alloc_hint_label(SourceForecast.DENIAL_ESTIMATE_CAVEAT))
    # …and the take beneath it: what the raid kills, what little it hauls, and what it leaves on the
    # range. Quiet ink — the waste IS the mission, not a warning about it.
    var take := SourceForecast.denial_take_bbcode(forecast, quarry_name)
    if take != "":
        sheet.add_child(HudWidgets.forecast_label(take))
    var reason := SourceForecast.denial_refusal_reason(forecast, herd)
    if reason != "":
        sheet.add_child(HudWidgets.alloc_hint_label(reason))
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The button carries the verdict and NEVER disables: a raid that cannot break the herd still
    # works it until recalled, so the launch verdict warns and the player is trusted.
    SourceForecast.style_send_denial_button(confirm, forecast)
    confirm.tooltip_text = reason if reason != "" else HudComposeVocab.SEND_DENIAL_RAID_HINT
    confirm.set_meta(HudWidgets.SEND_DENIAL_CONFIRM_META, true)
    var quarry_id := _compose.party_quarry_id()
    confirm.pressed.connect(func() -> void:
        emit_signal("send_denial_raid_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": quarry_name,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## Drop the composed quarry AND the fill target it was counted in. **They are one act** — a target is
## a count of a SPECIFIC herd's animals, so a target outliving its quarry would be handed to the next
## one, where `raid_load` answers a target at or above capacity by returning the pack — which is why
## the pairing now lives inside `ComposeState.clear_party_quarry` rather than being spelled out here:
## the map re-pick sets a quarry WITHOUT reaching this function, and did carry the stale target over.
## `ComposeState.seed_hunt` makes the same pairing on the herd drawer's side.
func _clear_party_quarry() -> void:
    _compose.clear_party_quarry()

## The Quarry row — the Party row's shape, with a button instead of a stepper. Unpicked it invites
## (`Choose…`, primary); picked it states the herd and stays available for a re-pick (ghost).
func _build_quarry_row(band: Dictionary, herd: Dictionary) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = HudComposeVocab.COMPOSE_FIELD_QUARRY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    var pick := Button.new()
    pick.focus_mode = Control.FOCUS_NONE
    # EXPAND_FILL is load-bearing on the picked branch: `clip_text` drops the button's minimum width
    # to ~0, so beside an EXPAND_FILL key label it collapses to a sliver. Both branches take it so the
    # row does not resize as a quarry is chosen.
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    if herd.is_empty():
        pick.text = HudComposeVocab.COMPOSE_QUARRY_CHOOSE
        pick.tooltip_text = HudComposeVocab.SEND_HUNT_EXPEDITION_HINT
        HudStyle.apply_button(pick, "primary")
    else:
        var name_text := SourceForecast.herd_display_name(herd)
        # The picked quarry wears the species' bundled ART where there is any (issue #439). A Button
        # takes an icon natively, so this is its `icon` PROPERTY rather than a glyph welded into the
        # face — and only the emoji branch keeps the format string, so a species with art loses the
        # leading glyph instead of carrying both. `icon_max_width` is what stops the 256px source
        # setting the button's minimum and dragging the compose row wide; `expand_icon` then fits it
        # to the button's own height. UNTINTED: `apply_button` sets no `icon_*_color`, and the stock
        # theme's is opaque white, so the animal renders in its own colours like every other marker.
        var quarry_sprite := FaunaSprites.for_herd(name_text)
        if quarry_sprite != null:
            pick.icon = quarry_sprite
            pick.expand_icon = true
            pick.add_theme_constant_override("icon_max_width",
                HudComposeVocab.COMPOSE_QUARRY_ICON_MAX_WIDTH)
            pick.text = name_text
        else:
            pick.text = HudComposeVocab.COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text]
        pick.clip_text = true
        pick.tooltip_text = HudComposeVocab.COMPOSE_QUARRY_TOOLTIP_FORMAT % [
            name_text, int(herd.get("x", -1)), int(herd.get("y", -1)),
        ]
        HudStyle.apply_button(pick, "ghost")
    # **THE OPEN SHEET'S MISSION DECIDES WHAT COUNTS AS A QUARRY**, so it rides with the pick rather
    # than being re-guessed at the click: a hunt's quarry must lie beyond the band's reach and a
    # denial raid's need not (`TargetingController.is_expedition_quarry`).
    var mission := _party_compose_mission
    pick.pressed.connect(func() -> void: _targeting.begin_pick_quarry(band, mission))
    row.add_child(pick)
    # **THE HEX MAY HOLD MORE THAN ONE HERD, AND THE MAP CANNOT SAY WHICH** — `try_dispatch` is handed
    # a TILE, so a rabbit warren sharing a hex with a wolf pack resolves to whichever the snapshot
    # lists first and re-clicking resolves to the same one. The chooser is the way to the others, and
    # it lives HERE rather than at the click because the choice is made against the forecast: the
    # collapse verdict, the raid payload and the useful party size are all functions of the herd, and
    # they exist only once the form is rendered. Absent with one candidate, so the common case renders
    # exactly as it did.
    if not herd.is_empty():
        var candidates := _targeting.eligible_quarries_on_tile(
            band, int(herd.get("x", -1)), int(herd.get("y", -1)), mission)
        if candidates.size() > 1:
            row.add_child(_build_quarry_choices_menu(band, herd, candidates, mission))
            # **THE CHOOSER'S WIDTH COMES OUT OF THE KEY, NOT OUT OF THE SPECIES' NAME.** The key and
            # the pick both EXPAND, so a third control on the row costs the name half of what it takes
            # — measured, `🐇 Rabbit Warren` came back clipped to `Rabbit Warre` on the very frame the
            # chooser exists to serve. `Quarry` is a fixed word that needs no more room than it
            # occupies, so it stops expanding whenever the row has three children; the pick, still the
            # only expanding child, takes everything the chooser leaves. Confined to this branch, so
            # the one-quarry row is untouched.
            key.size_flags_horizontal = Control.SIZE_FILL
    return row

## The quarry chooser: the `⋯` menu the zone heads already use, so the panel keeps ONE "there are
## choices here" glyph, with the candidates as radio-check items — a menu of plain items could not say
## which herd is the current one. A pick routes through `TargetingController.choose_quarry`, the SAME
## adoption the map click makes, so switching herds here and picking one there leave the composition
## in one state — which is also why `mission` is threaded down to it rather than defaulted: the
## adoption re-runs the eligibility test, and under denial the candidates include herds a hunt's rule
## would refuse.
func _build_quarry_choices_menu(band: Dictionary, chosen: Dictionary,
        candidates: Array, mission: String) -> MenuButton:
    var chosen_id := String(chosen.get("id", ""))
    var entries: Array = []
    for candidate_variant in candidates:
        var candidate: Dictionary = candidate_variant as Dictionary
        var name_text := SourceForecast.herd_display_name(candidate)
        # The item names the herd exactly as the picked-quarry button does — bundled ART where the
        # species has any, the emoji only where it does not — so the row and the menu cannot describe
        # one herd two ways, and two species sharing an emoji (Unicode ships ONE deer) stay apart.
        var sprite := FaunaSprites.for_herd(name_text)
        var entry := {
            "label": name_text if sprite != null \
                else HudComposeVocab.COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text],
            HudWidgets.MENU_ENTRY_CHECKED: String(candidate.get("id", "")) == chosen_id,
            "on_pick": func() -> void: _targeting.choose_quarry(band, candidate, mission),
        }
        if sprite != null:
            entry[HudWidgets.MENU_ENTRY_ICON] = sprite
        entries.append(entry)
    var menu := HudWidgets.build_section_menu(entries,
        HudComposeVocab.COMPOSE_QUARRY_CHOICES_TOOLTIP)
    menu.set_meta(HudWidgets.QUARRY_CHOICES_META, true)
    return menu

## The party size the band can field at all: idle workers, capped by the server's party-size limit.
func _scout_party_max(band: Dictionary, idle: int) -> int:
    var cap := int(band.get("max_expedition_party_size", 0))
    return mini(idle, cap) if cap > 0 else idle

## Leave the compose sheet — every flag together, so `open` / `mission` / `quarry` can never disagree.
## Also disarms any in-flight quarry pick: the ✕ can be pressed while a docked-sheet quarry pick is
## armed (the pick leaves this sheet open, unlike the floating one), so closing must tear down the
## targeting banner + herd glow too, else they persist over no sheet and a later click still fills a
## closed sheet. The call no-ops when no pick is armed.
func _close_party_compose() -> void:
    _party_compose_open = false
    _party_compose_mission = ""
    _clear_party_quarry()
    _targeting.cancel_pick_quarry()
    rerender()

# ---- badges -----------------------------------------------------------------

## Push the narrow shell's tab badges: Work carries its attention count (hot) or its source count,
## Parties its size (hot while any party is awaiting orders). Band carries none — it is always there.
func _push_zone_badges(band: Dictionary) -> void:
    if _panel == null:
        return
    var models := _work_source_models(band, _band_labor.effective_idle(band))
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    _panel.set_tab_badge(BandCityPanel.ZONE_BAND, "", false)
    _panel.set_tab_badge(BandCityPanel.ZONE_WORK,
        str(attention.size()) if not attention.is_empty() else str(models.size()),
        not attention.is_empty())
    var parties := _band_labor.band_parties(band)
    var awaiting := false
    for exp in parties:
        if HudFormat.expedition_phase_key(exp) == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

## Recall the selected in-flight expedition (folds it home). Emits recall_expedition_requested;
## Main formats the `recall_expedition …` command.
func _on_recall_expedition_pressed(expedition: Dictionary) -> void:
    if expedition.is_empty():
        return
    # A detached party is a band too, and `recall_expedition <faction> <expedition_band_id>` names it
    # by the same durable id — never its ECS entity bits.
    emit_signal("recall_expedition_requested", {
        "faction": int(expedition.get("faction", HudConst.PLAYER_FACTION_ID)),
        "expedition_band_id": int(expedition.get("band_id", HudConst.NO_BAND_ID)),
    })

## Render a player band's detail + labor allocation into the dockable Band/City panel and
## populate its header/cycler. The single place the panel's subject is set — shared by roster/map
## selection (`_render_occupant_drawer`) and the per-snapshot refresh (`refresh_snapshot`), so
## the panel is a persistent command center that survives selection changes.
func render_band(unit: Dictionary) -> void:
    if _panel == null or unit.is_empty():
        return
    # A quarry is chosen FOR a band (its travel time and useful party size are band-relative), so the
    # cycler swapping the panel subject must not carry one across.
    if int(unit.get("entity", -1)) != int(_band_labor.panel_band().get("entity", -1)):
        _clear_party_quarry()
    # DEEP-COPY the subject: the panel band must NOT alias the selection's unit dict (the
    # selection path passes it in). The panel persists across selection changes, so it needs its
    # own stable copy — a later selection swap (or an in-place edit of the selection's unit dict)
    # must not mutate or blank it. The zone closures below also capture this stable copy, so they
    # keep targeting the panel band regardless of the current selection.
    _band_labor.set_panel_band(unit.duplicate(true))
    # No tint-context reset here either: `_build_vitals_label` (inside the band zone below) builds its
    # own `DetailFormat.Context` per render, so the context cannot survive from the previous one.
    # The three zone contents. Ownership passes to the panel, which frees the previous render's zones
    # and parents these into whichever shell (wide columns / narrow tabs) its width selected.
    _panel.set_zones(
        HudWidgets.wrap_zone(build_band_zone(_band_labor.panel_band())),
        HudWidgets.wrap_zone(build_work_zone(_band_labor.panel_band())),
        HudWidgets.wrap_zone(build_parties_zone(_band_labor.panel_band())))
    _push_zone_badges(_band_labor.panel_band())
    # Header: settlement stage + name + stage label. The stage `id` is the panel's sprite key
    # (bundled art), the `icon` its emoji fallback for a stage with no art; both already flow
    # onto the marker/cohort dict. A missing stage falls back to a neutral glyph.
    var stage_id := String(_band_labor.panel_band().get("settlement_stage_id", "")).strip_edges()
    var glyph := String(_band_labor.panel_band().get("settlement_stage_icon", "")).strip_edges()
    var stage_label := String(_band_labor.panel_band().get("settlement_stage_label", "")).strip_edges()
    var index := _index_of_player_band(int(_band_labor.panel_band().get("entity", -1)))
    _panel.set_header(stage_id, glyph, HudFormat.band_display_name(_band_labor.panel_band(), index + 1), stage_label,
        _panel_position_label(_band_labor.panel_band()))
    _panel.set_cycler(index, _band_labor.player_bands().size())
    # `set_zones` above already flipped the panel to band-present; just make sure it is shown.
    _panel.set_shown(true)

## The band's hex coordinates for the panel header — the ONE place they are resolved, because the two
## paths that reach this panel spell them DIFFERENTLY and used to render differently because of it.
## The per-snapshot refresh hands over the cohort dict the native decoder built
## (`native/src/dict/population.rs`), which carries `current_x` / `current_y` and NO `pos`; a click on
## the band's map marker hands over MapView's marker copy, which carries a two-element `pos` array.
## So the snapshot path rendered no coordinates at all and the map path did, and a turn tick then took
## them away again. Preferring the cohort keys and falling back to `pos` makes both paths produce the
## identical header; neither resolvable ⇒ `""`, which the panel renders as nothing.
func _panel_position_label(band: Dictionary) -> String:
    if band.has("current_x") and band.has("current_y"):
        return HudFormat.BAND_HEADER_POSITION_FORMAT % [int(band["current_x"]), int(band["current_y"])]
    var pos_array: Array = Array(band.get("pos", []))
    if pos_array.size() == 2:
        return HudFormat.BAND_HEADER_POSITION_FORMAT % [int(pos_array[0]), int(pos_array[1])]
    return ""

## Select an expedition (from the panel's Active-expeditions list) on the map: recenter + select
## its hex (rebuilds that hex's roster), then pin the exact expedition so the map ring moves and the
## Occupants card renders its expedition drawer. Mirrors `cycle_band`'s routing. The Band/City
## panel itself stays on its band (expeditions detail in the Occupants card, per the existing split);
## a co-located band auto-select can't hijack it — we restore the panel band if it changed.
func select_expedition(entity: int, x: int, y: int) -> void:
    var panel_band_keep: Dictionary = _band_labor.panel_band().duplicate(true) if not _band_labor.panel_band().is_empty() else {}
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _selectioncard.find_roster_unit(entity).is_empty():
        _selectioncard.select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    if not panel_band_keep.is_empty() and int(_band_labor.panel_band().get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        render_band(panel_band_keep)

## A Current-actions row's label was clicked: show the source the band is working. Recenter + select
## its hex (`alert_focus_requested` → `MapView.focus_and_select_tile`) and, for a hunted herd, pin
## the herd itself (`roster_occupant_selected` → `MapView.select_occupant`) so its drawer opens on
## the herd rather than whatever occupant the hex auto-selects. This is exactly the routing the
## Active-expeditions rows and the turn-orb "Jump →" use — no new path. The Band/City panel stays on
## its band: focusing a hex that hosts another band would otherwise hijack the panel.
func focus_labor_source(x: int, y: int, herd_id: String = "") -> void:
    if x < 0 or y < 0:
        return
    var panel_band_keep: Dictionary = _band_labor.panel_band().duplicate(true) if not _band_labor.panel_band().is_empty() else {}
    emit_signal("alert_focus_requested", x, y)
    # The focus above rebuilt the hex's roster, so the subject is resolvable now.
    if herd_id != "" and not _selectioncard.find_roster_herd(herd_id).is_empty():
        _selectioncard.select_roster_occupant("herd", herd_id)
        emit_signal("roster_occupant_selected", "herd", herd_id)
    elif herd_id == "":
        # A FORAGE ROW NAMES THE LAND, exactly as a hunt row names its herd. Focusing the tile alone
        # left the hex's AUTO-PICK to choose the subject, which on a shared hex opens whichever band or
        # herd happens to stand there rather than the patch the player clicked — the row jumping to a
        # place but not to a THING. The land is the patch's subject (its rung rows and its Sow control
        # live on the land card), and `SUBJECT_LAND` is the established third kind on the `(kind, id)`
        # contract, so this is the forage twin of the herd branch above and not a new mechanism.
        _selectioncard.select_land_subject()
        emit_signal("roster_occupant_selected", HudSelectionState.SUBJECT_LAND, HudConst.LAND_SUBJECT_ID)
    if not panel_band_keep.is_empty() and int(_band_labor.panel_band().get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        render_band(panel_band_keep)

## Show a hunted herd. Herds MIGRATE each turn, so the hunt assignment's `target_x/target_y` is a
## stale launch position: resolve the herd's LIVE tile from the snapshot herd list first, exactly as
## `BandOverlayRenderer.draw_band_work_highlights` resolves the hunted-herd ring (`_herd_by_id`, falling back to
## the assignment target when the herd is unknown — e.g. it left the visible fauna set).
func _focus_hunt_source(herd_id: String, fallback_x: int, fallback_y: int) -> void:
    var herd := _band_labor.find_world_herd(herd_id)
    var x := int(herd.get("x", fallback_x))
    var y := int(herd.get("y", fallback_y))
    focus_labor_source(x, y, herd_id)

## Re-render the panel band into the panel container, keyed off `_band_labor.panel_band()` (never the current
## selection). The panel's own allocation rebuilds (optimistic pending, etc.) route through this so
## they stay pinned to the panel's subject even when a foreign hex is selected.
func rerender() -> void:
    if _panel == null or _band_labor.panel_band().is_empty():
        return
    render_band(_band_labor.panel_band())

## Keep the panel a live, persistent command center each snapshot: hide it when there are no
## player bands, else re-resolve the shown band against the fresh snapshot (so steppers/idle stay
## current) and re-render it. Called from update_band_alerts after _band_labor.player_band()(s) refresh.
func refresh_snapshot() -> void:
    if _panel == null:
        return
    if _band_labor.player_bands().is_empty():
        _band_labor.set_panel_band({})
        _panel.set_band_present(false)
        _panel.set_shown(false)
        return
    render_band(_resolve_panel_band())

## The band the panel should show: the same one across snapshots (re-fetched live by entity), or
## the first player band (the default actor) when the shown band is gone / unset.
func _resolve_panel_band() -> Dictionary:
    if not _band_labor.panel_band().is_empty():
        var entity := int(_band_labor.panel_band().get("entity", -1))
        for b in _band_labor.player_bands():
            if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
                return b
    return _band_labor.player_bands()[0] if not _band_labor.player_bands().is_empty() else {}

## Index of a band (by entity) within `_band_labor.player_bands()`, or -1 if absent.
func _index_of_player_band(entity: int) -> int:
    for i in range(_band_labor.player_bands().size()):
        if int((_band_labor.player_bands()[i] as Dictionary).get("entity", -1)) == entity:
            return i
    return -1

## Injected by Main: the dockable Band/City panel the band drawer renders into.
## (The Food/Morale disclosure `meta_clicked` is wired per-render on the fresh summary RichTextLabel
## in `render_band`, since main's section-block model rebuilds that label each render.)
func set_panel(panel: BandCityPanel) -> void:
    _panel = panel
    # THE PANEL OWNS THE FILE, THIS CONTROLLER OWNS THE VOCABULARY. The panel stores the work sort as
    # an opaque string, so validating it is this side's job: an empty (never chosen) or unknown value
    # — a hand-edited prefs file, a sort retired since it was written — leaves the default standing.
    # Without the guard it would not produce a broken board but a YIELD-sorted one: `_sort_work_models`
    # branches on `== WORK_SORT_NAME`, so anything else falls through to yield, silently reinstating
    # the re-ranking-under-your-own-edit behaviour issue #460 removed.
    if panel != null:
        var stored := StringName(panel.work_sort_pref())
        if HudWorkVocab.WORK_SORTS.has(stored):
            _work_sort = stored
    # The panel re-reports its zone box on a shell flip / dock change / collapse / window resize.
    # Re-PAGE the work board on it — the other two zones are unaffected by a box change.
    if panel != null and not panel.zones_resized.is_connected(_on_zones_resized):
        panel.zones_resized.connect(_on_zones_resized)

## Walk to the next/prev player band (cycler ◀/▶). Routes through the SAME band-selection a roster
## click uses — recenter + select the band's hex (rebuilding that hex's roster), then pin the exact
## band — so the map ring, Tile card, roster, and this panel all land on the cycled band.
func cycle_band(delta: int) -> void:
    if _panel == null or _band_labor.player_bands().size() <= 1:
        return
    var idx := _index_of_player_band(int(_band_labor.panel_band().get("entity", -1)))
    if idx < 0:
        idx = 0
    var n := _band_labor.player_bands().size()
    var next_band: Dictionary = _band_labor.player_bands()[((idx + delta) % n + n) % n]
    _select_band_on_map(next_band)

## Jump to the panel band on the map (the header title is a "jump to my band" affordance): recenter
## + select its hex and move the ring, WITHOUT changing which band the panel shows (it's already
## `_band_labor.panel_band()`). No-op when there is no panel band.
func focus_band() -> void:
    _select_band_on_map(_band_labor.panel_band())

## Select a band's hex on the map — recenter + select the hex (rebuilding its roster) via
## `alert_focus_requested` (→ MapView.focus_and_select_tile) then pin the exact band so the map ring,
## Tile card, roster, and panel all agree. Shared by the cycler and the header "jump to band". A band
## with no live roster entry (no tile_info) is rendered directly into the panel instead.
func _select_band_on_map(band: Dictionary) -> void:
    if band.is_empty():
        return
    var entity := int(band.get("entity", -1))
    var x := int(band.get("current_x", -1))
    var y := int(band.get("current_y", -1))
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _selectioncard.find_roster_unit(entity).is_empty():
        _selectioncard.select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    else:
        render_band(band)
