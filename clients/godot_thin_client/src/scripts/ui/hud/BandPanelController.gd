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
## THE BOUNDARY BACK TO `HudLayer` IS SIX CALLABLES, each retained there for a reason the
## "an injection you still have to hold is relocated, not eliminated" test settles:
##   • `_emit_assign_labor` — owns the `assign_labor_requested` emit, the optimistic pending write and
##     `_after_pending_change()`. So `assign_labor` stays INDIRECT here, while the three commands with
##     no other emitter (`cancel_order` / `send_hunt_expedition` / `recall_expedition`) are signals.
##   • `_herd_label_for_id` — the herd vocabulary, also read by the targeting banner + command feed.
##   • `_on_send_expedition_pressed` + the QUARRY TRIO (`_on_pick_quarry_pressed` /
##     `_cancel_pending_pick_quarry` / `_is_expedition_quarry`) — HudLayer's targeting machinery, which
##     has three other modes and its own `_pending_*` state. Bundling the trio behind a façade was
##     considered and rejected: `HudLayer` would still construct it from the same three Callables.
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
## The word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the
## same convention `HudWidgets` / `HudFormat` / `TopBarReadouts` / `SelectionCardController` /
## `DrawerComposeController` follow — so a phrase is still typed in exactly one place.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# Standing work was cleared for a whole scope — relayed to HudLayer.cancel_order_requested.
signal cancel_order_requested(band: Dictionary, scope: String)
# A hunting party was dispatched from the parties zone — relayed to HudLayer.send_hunt_expedition_requested.
signal send_hunt_expedition_requested(payload: Dictionary)
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

# --- The six retained HudLayer helpers, injected as Callables (see the class header) ---
# Each is reached through a typed adapter below rather than called raw: `Callable.call` returns
# `Variant`, which would push an untyped value into every consumer here.
var _emit_assign_labor_fn: Callable
var _herd_label_for_id_fn: Callable
var _on_send_expedition_pressed_fn: Callable
var _on_pick_quarry_pressed_fn: Callable
var _cancel_pending_pick_quarry_fn: Callable
var _is_expedition_quarry_fn: Callable

# --- Owned state (moved off HudLayer) ---
# The dockable Band/City command center (docs/plan_band_city_dock.md §3), injected by Main through
# HudLayer's `set_band_city_panel` delegator. When present, a selected player band's detail renders
# into IT rather than the Occupants card, and the panel persists across selection changes showing the
# panel band. The panel band itself (re-resolved by entity each snapshot) lives on
# `_band_labor.panel_band()`. PRIVATE — outside readers ask `has_panel()`.
var _panel: BandCityPanel = null
# ---- Band/City zone state (persists across renders, so a filter/tab/page survives a snapshot) ----
## Which sources the work board shows, how it orders them, and which page is on screen.
var _work_filter: StringName = HudLayer.WORK_FILTER_ALL
var _work_sort: StringName = HudLayer.WORK_SORT_YIELD
var _work_page: int = 0
## The source key open in the work inspector strip ("" = none), and whether its policy picker is out.
## One row at a time — the strip costs board rows, which `_work_board_capacity` subtracts.
var _work_open_key: String = ""
var _work_policy_open: bool = false
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
var _band_zone_tier: int = HudLayer.BAND_ZONE_TIER_TALL
## The parties compose sheet: open, and which mission has been picked ("" = none yet, which is what
## keeps the party size / policy / forecast fields hidden until the mission decides them).
var _party_compose_open: bool = false
var _party_compose_mission: String = ""
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = HudLayer.WORKER_STEP
# Compose state for the hunt-expedition launch policy (Sustain/Surplus/Market/Eradicate).
var _send_hunt_policy: String = HudLayer.DEFAULT_HUNT_POLICY

func _init(band_labor: HudBandLaborState, compose: ComposeState,
        selectioncard: SelectionCardController, disclosures: DisclosureController,
        banddetail: BandDetailLines, host: Node,
        emit_assign_labor: Callable, herd_label_for_id: Callable,
        on_send_expedition_pressed: Callable, on_pick_quarry_pressed: Callable,
        cancel_pending_pick_quarry: Callable, is_expedition_quarry: Callable) -> void:
    _band_labor = band_labor
    _compose = compose
    _selectioncard = selectioncard
    _disclosures = disclosures
    _banddetail = banddetail
    _host = host
    _emit_assign_labor_fn = emit_assign_labor
    _herd_label_for_id_fn = herd_label_for_id
    _on_send_expedition_pressed_fn = on_send_expedition_pressed
    _on_pick_quarry_pressed_fn = on_pick_quarry_pressed
    _cancel_pending_pick_quarry_fn = cancel_pending_pick_quarry
    _is_expedition_quarry_fn = is_expedition_quarry

# ---- Typed adapters over the six injected HudLayer helpers -------------------------------------

## Issue a labor assignment. Retained on HudLayer because it owns the `assign_labor_requested` emit,
## the optimistic pending-labor write and `_after_pending_change()`.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String,
        policy: String, species: String = "") -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, policy, species)

## A friendlier label for a herd id. Retained on HudLayer, which also feeds the targeting banner and
## the command feed from it.
func _herd_label_for_id(herd_id: String) -> String:
    return _herd_label_for_id_fn.call(herd_id)

## Outfit a scouting party and enter TILE-targeting for its destination. Retained on HudLayer with the
## `_pending_send_expedition` state and the three other targeting modes.
func _on_send_expedition_pressed(band: Dictionary, party_workers: int) -> void:
    _on_send_expedition_pressed_fn.call(band, party_workers)

## Enter HERD-targeting so the next map click names the hunting party's quarry. Retained on HudLayer
## with `_pending_pick_quarry`.
func _on_pick_quarry_pressed(band: Dictionary) -> void:
    _on_pick_quarry_pressed_fn.call(band)

## Abandon an in-flight quarry pick (the chosen quarry, if any, stays chosen). Retained on HudLayer
## with the targeting refresh it drives.
func _cancel_pending_pick_quarry() -> void:
    _cancel_pending_pick_quarry_fn.call()

## Is `herd` a valid quarry for a DETACHED party from `band` (strictly beyond its hunt reach)? THE
## single definition, retained on HudLayer where the pick and MapView's glow also read it.
func _is_expedition_quarry(band: Dictionary, herd: Dictionary) -> bool:
    return bool(_is_expedition_quarry_fn.call(band, herd))

## Player-faction check for a band (a trivial private copy of HudLayer's, the SelectionCardController
## precedent — a one-line predicate is not worth a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudLayer.PLAYER_FACTION_ID)) == HudLayer.PLAYER_FACTION_ID

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
    block.add_child(HudWidgets.alloc_section_label(HudLayer.ALLOC_HEADER_FOOD_OUTLOOK))
    var chart := FoodOutlookChart.new()
    # Drain = the people's meals plus the pens' feed, held flat across the horizon (see the chart's
    # header): the same two debits the Food breakdown itemizes, so the two readouts cannot disagree.
    chart.set_projection(
        DetailFormat.band_provisions(band), arrivals,
        float(band.get("food_consumption", 0.0)) + DetailFormat.band_pen_feed(band), _band_labor.current_turn())
    # A short zone gets a COMPACT chart (same series, same empty marker, less height) rather than a
    # clipped full-height one — the zone's height is fixed, so the chart yields, not the layout.
    if compact:
        chart.custom_minimum_size = Vector2(chart.custom_minimum_size.x, HudLayer.FOOD_CHART_COMPACT_HEIGHT)
    block.add_child(chart)
    return block

## A fresh section-block VBox: the discrete, self-contained unit the Band/City panel arranges (a
## vertical stack when tall, a column-flow when wide). Rows are added into it exactly as they used to
## be added into the flat allocation container — only the parent node changes.
func _make_alloc_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudLayer.ALLOC_BLOCK_SEPARATION)
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
    return HudLayer.ZONE_FALLBACK_SIZE

## Ask before a destructive bulk action. A `ConfirmationDialog` is a Window — like the section menu,
## it cannot disturb any zone's height. The body names what is SPARED, so "unassign all" never reads
## as "undo everything".
func _confirm_destructive(body: String, ok_text: String, on_confirm: Callable) -> void:
    var dialog := ConfirmationDialog.new()
    dialog.dialog_text = body
    dialog.ok_button_text = ok_text
    dialog.title = HudLayer.CONFIRM_DIALOG_TITLE
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
    if _band_zone_tier != HudLayer.BAND_ZONE_TIER_SHORT:
        var outlook := _build_food_outlook_block(band, _band_zone_tier == HudLayer.BAND_ZONE_TIER_COMPACT)
        if outlook != null:
            col.add_child(outlook)
    col.add_child(_build_workforce_block(band, _band_zone_tier == HudLayer.BAND_ZONE_TIER_SHORT))
    return col

## The vitals readout — the Food / Morale / Output rows with their click-to-expand disclosures. A
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
    detail_label.text = DetailFormat.detail_bbcode(
        _banddetail.unit_summary_lines(band, _selectioncard.selected_terrain_label(), ctx), ctx)
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
        segments.append({"key": HudLayer.PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT, "tooltip": "%d %s" % [children, HudLayer.PEOPLE_LABEL_CHILDREN]})
    if working > 0:
        segments.append({"key": HudLayer.PEOPLE_GLYPH_WORKING, "count": working,
            "color": HudStyle.INK_DIM, "tooltip": "%d %s" % [working, HudLayer.PEOPLE_LABEL_WORKING]})
    if elders > 0:
        segments.append({"key": HudLayer.PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK, "tooltip": "%d %s" % [elders, HudLayer.PEOPLE_LABEL_ELDERS]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudLayer.ZONE_HEADER_PEOPLE, str(total)))
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
    chip.text = HudLayer.PEOPLE_DEPENDENCY_FORMAT % dependents
    chip.add_theme_font_size_override("font_size", HudLayer.COMPOSITION_KEY_FONT_SIZE)
    chip.add_theme_color_override("font_color",
        HudStyle.WARN if per_hundred > HudLayer.PEOPLE_DEPENDENCY_HEAVY else HudStyle.INK_FAINT)
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
            HudLayer.LABOR_KIND_FORAGE: forage_workers += workers
            HudLayer.LABOR_KIND_HUNT: hunt_workers += workers
    var scout_eff := _band_labor.effective_role_workers(band, HudLayer.LABOR_KIND_SCOUT)
    var warrior_eff := _band_labor.effective_role_workers(band, HudLayer.LABOR_KIND_WARRIOR)
    var role_workers := int(scout_eff.get("workers", 0)) + int(warrior_eff.get("workers", 0))
    var party_workers := _band_labor.band_party_workers(band)
    var segments: Array = []
    for spec in [
        [HudLayer.WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [HudLayer.WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        [HudLayer.WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        [HudLayer.WORKFORCE_KEY_PARTIES, party_workers, HudStyle.WARN],
        [HudLayer.WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudLayer.ZONE_HEADER_WORKFORCE,
        HudLayer.WORKFORCE_IDLE_FORMAT % [idle, int(band.get("working_age", 0))],
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM))
    if not segments.is_empty():
        block.add_child(HudWidgets.build_composition_bar(segments))
        block.add_child(HudWidgets.build_composition_key(segments))
    # The two standing roles as CARDS, side by side — a bordered card reads as "a standing role", not
    # as one more worked source in a list (the complaint the card treatment fixes).
    var cards := HBoxContainer.new()
    cards.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    cards.add_theme_constant_override("separation", HudLayer.ROLE_CARD_SEPARATION)
    cards.add_child(_build_role_card(band, HudLayer.ROLE_NAME_SCOUT, HudLayer.SCOUT_ROLE_HINT, HudLayer.LABOR_KIND_SCOUT, scout_eff, idle, compact_cards))
    cards.add_child(_build_role_card(band, HudLayer.ROLE_NAME_WARRIOR, HudLayer.WARRIOR_ROLE_HINT, HudLayer.LABOR_KIND_WARRIOR, warrior_eff, idle, compact_cards))
    block.add_child(cards)
    return block

## One standing-role card: name · one-line hint · the SAME −/+ stepper (same `assign_labor` emit,
## same idle gating) the role rows used to carry.
func _build_role_card(band: Dictionary, role_name: String, hint: String, kind: String, effective: Dictionary, idle: int, compact: bool = false) -> PanelContainer:
    var workers := int(effective.get("workers", 0))
    var pending := bool(effective.get("pending", false))
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    card.add_theme_stylebox_override("panel", HudStyle.role_card_stylebox())
    # In a short zone the hint moves to the card's tooltip — the words survive, the two lines do not.
    card.tooltip_text = hint
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudLayer.ROLE_CARD_SEPARATION)
    card.add_child(col)
    var title := Label.new()
    title.text = role_name
    title.add_theme_font_size_override("font_size", HudLayer.ROLE_CARD_NAME_FONT_SIZE)
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK)
    col.add_child(title)
    if not compact:
        var hint_label := HudWidgets.alloc_hint_label(hint)
        hint_label.custom_minimum_size = Vector2(0.0, HudLayer.ROLE_CARD_HINT_HEIGHT)
        col.add_child(hint_label)
    var stepper := HBoxContainer.new()
    stepper.alignment = BoxContainer.ALIGNMENT_CENTER
    stepper.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    HudWidgets.add_stepper_controls(stepper, workers, idle > 0,
        func(n: int) -> void: _emit_assign_labor(band, kind, n, -1, -1, "", ""))
    col.add_child(stepper)
    return card

# ---- zone `work` (the paged board) ------------------------------------------

## Zone `work`: header · filter chips · the paged board · pager · inspector strip. The column keeps a
## reference to itself so `zones_resized` can RE-PAGE in place rather than re-render the whole panel.
func build_work_zone(band: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudLayer.ZONE_BLOCK_SEPARATION)
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
    if zone_height >= HudLayer.BAND_ZONE_TALL_MIN_HEIGHT:
        return HudLayer.BAND_ZONE_TIER_TALL
    if zone_height >= HudLayer.BAND_ZONE_CHART_MIN_HEIGHT:
        return HudLayer.BAND_ZONE_TIER_COMPACT
    return HudLayer.BAND_ZONE_TIER_SHORT

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
    var income := 0.0
    for m in models:
        income += float((m as Dictionary).get("rate", 0.0))
    col.add_child(_build_work_head(band, models, income))
    # BEFORE the chips are built, so the pressed chip is always one that actually renders.
    _reconcile_work_filter(models)
    col.add_child(_build_work_chips(models))
    var filtered := _filter_work_models(models)
    _sort_work_models(filtered)
    # Drop an inspector pinned to a source that has left the filtered set (unassigned, filtered out).
    var inspected := _find_work_model(filtered, _work_open_key)
    if inspected.is_empty():
        _work_open_key = ""
        _work_policy_open = false
    if filtered.is_empty():
        var hint := HudWidgets.alloc_hint_label(HudLayer.WORK_EMPTY_HINT)
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
    var cols := clampi(int(box.x / HudLayer.WORK_COLUMN_MIN_WIDTH), 1, HudLayer.WORK_MAX_COLUMNS)
    var inspector_h := 0.0 if inspected.is_empty() else _work_inspector_height(inspected)
    var chrome := HudLayer.ZONE_HEAD_HEIGHT + HudLayer.WORK_CHIPS_HEIGHT + inspector_h \
        + float(HudLayer.ZONE_BLOCK_SEPARATION) * HudLayer.WORK_ZONE_GAP_COUNT
    var rows := maxi(1, int((box.y - chrome) / HudLayer.WORK_ROW_HEIGHT))
    var pages := ceili(float(count) / float(maxi(cols * rows, 1)))
    if pages > 1:
        rows = maxi(1, int((box.y - chrome - HudLayer.WORK_PAGER_HEIGHT - float(HudLayer.ZONE_BLOCK_SEPARATION)) / HudLayer.WORK_ROW_HEIGHT))
        pages = ceili(float(count) / float(maxi(cols * rows, 1)))
    return {"cols": cols, "rows_per_col": rows, "page_size": cols * rows, "pages": maxi(pages, 1)}

## The board itself: `cols` column VBoxes filled COLUMN-MAJOR (top of column 1 to its bottom, then
## column 2), separated by a hairline rule. Fixed-height rows, no scroll — the page IS the limit.
func _build_work_board(band: Dictionary, page: Array, cols: int, rows_per_col: int) -> HBoxContainer:
    var board := HBoxContainer.new()
    board.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    board.size_flags_vertical = Control.SIZE_EXPAND_FILL
    board.add_theme_constant_override("separation", HudLayer.WORK_COLUMN_SEPARATION)
    for c in range(cols):
        if c > 0:
            var rule := ColorRect.new()
            rule.color = HudStyle.LINE_SOFT
            rule.custom_minimum_size = Vector2(HudLayer.WORK_COLUMN_RULE_WIDTH, 0.0)
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

## The zone's head row: WORK · n sources · the band's total rate · the `⋯` section menu.
func _build_work_head(band: Dictionary, models: Array, income: float) -> HBoxContainer:
    var menu := HudWidgets.build_section_menu([
        {"label": HudLayer.WORK_MENU_SORT_YIELD, "on_pick": func() -> void: _set_work_sort(HudLayer.WORK_SORT_YIELD)},
        {"label": HudLayer.WORK_MENU_SORT_NAME, "on_pick": func() -> void: _set_work_sort(HudLayer.WORK_SORT_NAME)},
        {"label": HudLayer.WORK_MENU_UNASSIGN_FORMAT % models.size(), "disabled": models.is_empty(),
            "on_pick": func() -> void: _on_work_unassign_all_pressed(band, models.size())},
    ], HudLayer.WORK_MENU_TOOLTIP)
    var head := HudWidgets.zone_head(HudLayer.ZONE_HEADER_WORK, HudLayer.WORK_SOURCES_FORMAT % models.size(), menu)
    # The total sits between the count and the menu, tinted like the Food line's net rate.
    var total := Label.new()
    total.text = SourceForecast.format_yield(income)
    total.add_theme_font_size_override("font_size", HudLayer.ZONE_HEAD_FONT_SIZE)
    total.add_theme_color_override("font_color", HudStyle.HEALTHY if income > 0.0 else HudStyle.INK_DIM)
    HudWidgets.set_label_tooltip(total, HudLayer.WORK_TOTAL_TOOLTIP)
    head.add_child(total)
    head.move_child(total, head.get_child_count() - 2)
    return head

## The filter chips ARE the summary: counts + per-kind rates, and pressing one filters the board.
## **A chip for an EMPTY set never renders** — a kind the band works none of is dead weight in a row
## that is otherwise live summary, and an always-present `⚠ 0` reads as an alarm. `All` always shows
## (it is the reset), so the row is never empty.
func _build_work_chips(models: Array) -> HFlowContainer:
    var chips := HFlowContainer.new()
    chips.custom_minimum_size = Vector2(0.0, HudLayer.WORK_CHIPS_HEIGHT)
    chips.add_theme_constant_override("h_separation", HudLayer.WORK_CHIP_SEPARATION)
    var forage: Array = models.filter(func(m): return String(m["kind"]) == HudLayer.LABOR_KIND_FORAGE)
    var hunt: Array = models.filter(func(m): return String(m["kind"]) == HudLayer.LABOR_KIND_HUNT)
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    chips.add_child(_build_work_chip(HudLayer.WORK_FILTER_ALL, HudLayer.WORK_CHIP_ALL_FORMAT % models.size(), false))
    if not forage.is_empty():
        chips.add_child(_build_work_chip(HudLayer.WORK_FILTER_FORAGE, HudLayer.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.DEFAULT, forage.size(), SourceForecast.format_magnitude(_work_rate_sum(forage))], false))
    if not hunt.is_empty():
        chips.add_child(_build_work_chip(HudLayer.WORK_FILTER_HUNT, HudLayer.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.HUNT, hunt.size(), SourceForecast.format_magnitude(_work_rate_sum(hunt))], false))
    if not attention.is_empty():
        chips.add_child(_build_work_chip(HudLayer.WORK_FILTER_ATTENTION,
            HudLayer.WORK_CHIP_ATTENTION_FORMAT % attention.size(), true))
    return chips

func _work_rate_sum(models: Array) -> float:
    var total := 0.0
    for m in models:
        total += float((m as Dictionary).get("rate", 0.0))
    return total

func _build_work_chip(filter: StringName, text: String, alert: bool) -> Button:
    var active := _work_filter == filter
    var chip := Button.new()
    chip.text = text
    chip.focus_mode = Control.FOCUS_NONE
    HudStyle.apply_button(chip, "primary" if active else "ghost")
    HudWidgets.compact(chip, HudLayer.WORK_CHIP_FONT_SIZE, HudLayer.WORK_CHIP_PADDING_V)
    if alert and not active:
        chip.add_theme_color_override("font_color", HudStyle.WARN)
    chip.tooltip_text = HudLayer.WORK_CHIP_TOOLTIP
    chip.pressed.connect(func() -> void: _set_work_filter(filter))
    return chip

## ONE-LINE source row: severity stripe · glyph · label (clipped) · rate · policy/⚠ marks · the
## existing −/+ stepper. Clicking anywhere but the stepper opens the row in the inspector strip.
func _build_work_row(band: Dictionary, model: Dictionary) -> PanelContainer:
    var open := String(model.get("key", "")) == _work_open_key
    var row := PanelContainer.new()
    row.custom_minimum_size = Vector2(0.0, HudLayer.WORK_ROW_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.mouse_filter = Control.MOUSE_FILTER_STOP
    row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    row.tooltip_text = String(model.get("tooltip", ""))
    row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(open))
    row.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
            _toggle_work_inspector(String(model.get("key", ""))))
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudLayer.WORK_ROW_SEPARATION)
    row.add_child(line)
    # Severity stripe: WARN when the source is overdrawing or overstaffed, SIGNAL while an edit is
    # still pending, transparent otherwise — so the eye finds trouble without reading a word.
    var stripe := ColorRect.new()
    stripe.custom_minimum_size = Vector2(HudLayer.WORK_ROW_STRIPE_WIDTH, 0.0)
    stripe.size_flags_vertical = Control.SIZE_EXPAND_FILL
    stripe.color = _work_row_stripe_color(model)
    stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(stripe)
    var icon := Label.new()
    icon.text = String(model.get("icon", ""))
    icon.custom_minimum_size = Vector2(HudLayer.WORK_ROW_ICON_WIDTH, 0.0)
    icon.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
    icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(icon)
    var label := Label.new()
    label.text = String(model.get("label", ""))
    label.clip_text = true
    # A label too long even for the widened column ELLIPSISES rather than hard-cutting: `Hunt Woolly
    # Mamm…` reads as a truncation, `Forage (73, 20` reads as a wrong coordinate.
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("pending", false)) else HudStyle.INK)
    label.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    var rate := Label.new()
    rate.text = SourceForecast.format_signed(float(model.get("rate", 0.0))) if bool(model.get("has_yield", false)) else ""
    rate.custom_minimum_size = Vector2(HudLayer.WORK_ROW_RATE_WIDTH, 0.0)
    rate.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    rate.add_theme_color_override("font_color", HudStyle.INK_DIM)
    rate.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
    rate.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(rate)
    var marks := Label.new()
    marks.text = String(model.get("marks", ""))
    marks.custom_minimum_size = Vector2(HudLayer.WORK_ROW_MARKS_WIDTH, 0.0)
    marks.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("warn", false)) else HudStyle.INK_DIM)
    marks.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
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
    pager.custom_minimum_size = Vector2(0.0, HudLayer.WORK_PAGER_HEIGHT)
    pager.add_theme_constant_override("separation", HudLayer.WORK_ROW_SEPARATION)
    var prev := Button.new()
    prev.text = HudLayer.PAGER_PREV_GLYPH
    prev.focus_mode = Control.FOCUS_NONE
    prev.disabled = _work_page <= 0
    prev.tooltip_text = HudLayer.PAGER_PREV_TOOLTIP
    HudStyle.apply_button(prev, "ghost")
    HudWidgets.compact(prev, HudLayer.WORK_CHIP_FONT_SIZE, HudLayer.WORK_PAGER_PADDING_V)
    prev.pressed.connect(func() -> void: _step_work_page(-1))
    pager.add_child(prev)
    var label := Label.new()
    label.text = HudLayer.PAGER_FORMAT % [_work_page + 1, pages]
    label.add_theme_font_size_override("font_size", HudLayer.WORK_CHIP_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    pager.add_child(label)
    var next := Button.new()
    next.text = HudLayer.PAGER_NEXT_GLYPH
    next.focus_mode = Control.FOCUS_NONE
    next.disabled = _work_page >= pages - 1
    next.tooltip_text = HudLayer.PAGER_NEXT_TOOLTIP
    HudStyle.apply_button(next, "ghost")
    HudWidgets.compact(next, HudLayer.WORK_CHIP_FONT_SIZE, HudLayer.WORK_PAGER_PADDING_V)
    next.pressed.connect(func() -> void: _step_work_page(1))
    pager.add_child(next)
    var range_label := Label.new()
    range_label.text = HudLayer.PAGER_RANGE_FORMAT % [start + 1, shown_end, total]
    range_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    range_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    range_label.add_theme_font_size_override("font_size", HudLayer.WORK_CHIP_FONT_SIZE)
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
    col.add_theme_constant_override("separation", HudLayer.ZONE_BLOCK_SEPARATION)
    strip.add_child(col)
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudLayer.WORK_ROW_SEPARATION)
    var title := Label.new()
    title.text = "%s %s" % [String(model.get("icon", "")), String(model.get("label", ""))]
    title.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudLayer.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudLayer.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudLayer.WORK_ROW_FONT_SIZE, HudLayer.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_work_inspector(String(model.get("key", ""))))
    head.add_child(close)
    col.add_child(head)
    col.add_child(HudWidgets.build_status_part(_work_inspector_sentence(model), HudStyle.INK_DIM))
    if bool(model.get("warn", false)):
        col.add_child(HudWidgets.build_status_part(HudLayer.WORK_INSPECT_OVERDRAW_LINE, HudStyle.WARN))
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
    links.add_theme_constant_override("separation", HudLayer.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudLayer.WORK_INSPECT_JUMP, HudStyle.INK, func() -> void:
        _focus_work_source(model)))
    links.add_child(HudWidgets.build_inline_link(HudLayer.WORK_INSPECT_POLICY, HudStyle.INK, func() -> void:
        _work_policy_open = not _work_policy_open
        _repage_work_zone()))
    links.add_child(HudWidgets.build_inline_link(HudLayer.WORK_INSPECT_UNASSIGN, HudStyle.DANGER, func() -> void:
        _work_open_key = ""
        _work_policy_open = false
        _emit_work_assign(band, model, 0)))
    col.add_child(links)
    if _work_policy_open:
        # The four EXTRACTIVE rungs only. The investment rungs (cultivate/sow/tame/corral) are ladder
        # COMMITMENTS made at the source's own compose control, where their knowledge gates and payoff
        # forecasts live; changing an existing assignment's take needs no gate.
        var standing := String(model.get("policy", ""))
        if standing in HudLayer.INVESTMENT_POLICIES:
            # The picker highlights NOTHING on an investment assignment (the standing rung is not one
            # of the four), and an unhighlighted radio reads as unset. This line is what explains it.
            col.add_child(HudWidgets.build_status_part(
                HudLayer.WORK_INSPECT_STANDING_INVESTMENT_FORMAT % HudFormat.policy_face(standing), HudStyle.WARN))
        col.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
            _on_work_policy_picked(band, model, policy),
            standing, HudLayer.LABOR_HUNT_POLICIES, {}, {}, HudLayer.ZONE_POLICY_PICKER_COLUMNS))
    return strip

## A rung picked in the work inspector. On the ordinary (EXTRACTIVE) standing policy this re-sends the
## assignment immediately, exactly as it always has. On an INVESTMENT one the pick DISCARDS a ladder
## build worth ~25 turns, so it asks first — the same `_confirm_destructive` treatment "Unassign all
## work" and "Recall all parties" get. The picker stays open until the answer comes back, so a cancel
## leaves the frame exactly as it was rather than silently closing on a change that never happened.
func _on_work_policy_picked(band: Dictionary, model: Dictionary, policy: String) -> void:
    if String(model.get("policy", "")) in HudLayer.INVESTMENT_POLICIES:
        _confirm_destructive(
            HudLayer.WORK_INSPECT_END_INVESTMENT_CONFIRM_FORMAT % [
                HudFormat.policy_face(String(model.get("policy", ""))),
                String(model.get("label", "")),
                HudFormat.policy_face(policy)],
            HudLayer.WORK_INSPECT_END_INVESTMENT_CONFIRM_OK,
            func() -> void: _commit_work_policy(band, model, policy))
        return
    _commit_work_policy(band, model, policy)

func _commit_work_policy(band: Dictionary, model: Dictionary, policy: String) -> void:
    _work_policy_open = false
    _emit_work_assign(band, model, int(model.get("workers", 0)), policy)

## The height the open inspector reserves — BOTH what `_work_board_capacity` subtracts from the board
## and what the strip actually draws at, so the page can never overflow its zone (the work-board rule).
func _work_inspector_height(model: Dictionary) -> float:
    if not _work_policy_open:
        return HudLayer.WORK_INSPECTOR_HEIGHT
    if String(model.get("policy", "")) in HudLayer.INVESTMENT_POLICIES:
        return HudLayer.WORK_INSPECTOR_POLICY_HEIGHT + HudLayer.WORK_INSPECTOR_STANDING_LINE_HEIGHT
    return HudLayer.WORK_INSPECTOR_POLICY_HEIGHT

## The inspector's one-sentence readout: rate · policy in WORDS · status · assigned workers.
func _work_inspector_sentence(model: Dictionary) -> String:
    var parts: Array[String] = []
    if bool(model.get("has_yield", false)):
        parts.append(SourceForecast.format_yield(float(model.get("rate", 0.0))))
    var policy := String(model.get("policy", ""))
    if policy != "":
        parts.append(policy.capitalize())
    parts.append(HudFormat.status_label(FoodIcons.STATUS_PENDING if bool(model.get("pending", false)) \
        else FoodIcons.STATUS_WORKING))
    parts.append(HudLayer.WORK_INSPECT_ASSIGNED_FORMAT % int(model.get("workers", 0)))
    return HudLayer.WORK_INSPECT_SENTENCE_SEPARATOR.join(parts)

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
        if not (kind == HudLayer.LABOR_KIND_FORAGE or kind == HudLayer.LABOR_KIND_HUNT):
            continue
        if workers <= 0 and not pending:
            continue
        var yld := SourceForecast.source_yield_readout(m, kind)
        var x := int(m.get("x", -1))
        var y := int(m.get("y", -1))
        var herd_id := String(m.get("herd_id", ""))
        var policy := String(m.get("policy", "")).strip_edges().to_lower()
        var icon := ""
        var label := ""
        var cap := {}
        if kind == HudLayer.LABOR_KIND_FORAGE:
            if not (policy in HudLayer.FORAGE_POLICY_OPTIONS):
                policy = HudLayer.DEFAULT_HUNT_POLICY
            # The board draws the glyph in its OWN fixed column, so it takes the RAW icon — not
            # `HudFormat.source_icon_prefix`, which welds it to the label with a trailing space for the
            # single-label row this replaced.
            icon = _band_labor.food_module_icon(x, y)
            label = HudLayer.WORK_ROW_FORAGE_FORMAT % [x, y]
            cap = SourceForecast.source_worker_cap_state(SourceForecast.forecast_inputs(
                _band_labor.forage_patch_lookup().get(Vector2i(x, y), {}), HudLayer.SOURCE_KIND_FORAGE,
                HudLayer.BARE_FORECAST_PREFIX, policy), workers, idle)
        else:
            if not (policy in HudLayer.HUNT_POLICY_OPTIONS):
                policy = _band_labor.policy_for_hunt(band, herd_id)
            var herd_label := _herd_label_for_id(herd_id)
            icon = FoodIcons.for_herd(herd_label)
            label = HudLayer.WORK_ROW_HUNT_FORMAT % herd_label
            # Herds MIGRATE, so the cap reads the herd's LIVE dict from `_band_labor.world_herds()` rather than the
            # assignment's launch-time target.
            cap = SourceForecast.source_worker_cap_state(SourceForecast.forecast_inputs(
                _band_labor.find_world_herd(herd_id), HudLayer.SOURCE_KIND_HERD,
                HudLayer.BARE_FORECAST_PREFIX, policy), workers, idle)
        var note := String(yld.get("note", ""))
        var marks := FoodIcons.for_policy(policy)
        if bool(yld.get("warn", false)):
            marks += " " + HudLayer.OVERHUNT_FLAG
        models.append({
            "key": String(key), "kind": kind, "icon": icon, "label": label,
            "rate": float(yld.get("rate", 0.0)), "has_yield": bool(m.get("has_yield", false)),
            "workers": workers, "pending": pending, "warn": bool(yld.get("warn", false)),
            "note": note, "muted_note": String(yld.get("muted_note", "")), "marks": marks,
            "policy": policy, "x": x, "y": y, "herd_id": herd_id,
            "can_add": bool(cap.get("can_add", idle > 0)),
            "schedule": HudBandLaborState.as_schedule(m.get("arrival_schedule", null)),
            "tooltip": HudFormat.join_tooltip_lines([String(yld.get("tooltip", "")),
                _policy_hint(kind, policy), String(cap.get("note", "")), HudLayer.WORK_ROW_OPEN_HINT]),
            # A source wants attention when it overdraws, wastes workers, or is still unacknowledged.
            "attention": bool(yld.get("warn", false)) or note != "" or pending,
        })
    return models

## Reset a filter that now selects nothing back to `All`. A kind/attention chip is hidden once its set
## empties (the last herd is unassigned, the last ⚠ clears), so a standing filter would otherwise
## strand the player on an empty board with no chip left to press to get back out of it.
func _reconcile_work_filter(models: Array) -> void:
    if _work_filter == HudLayer.WORK_FILTER_ALL:
        return
    if _work_models_matching(_work_filter, models).is_empty():
        _work_filter = HudLayer.WORK_FILTER_ALL

func _filter_work_models(models: Array) -> Array:
    return _work_models_matching(_work_filter, models)

func _work_models_matching(filter: StringName, models: Array) -> Array:
    match filter:
        HudLayer.WORK_FILTER_FORAGE:
            return models.filter(func(m): return String(m["kind"]) == HudLayer.LABOR_KIND_FORAGE)
        HudLayer.WORK_FILTER_HUNT:
            return models.filter(func(m): return String(m["kind"]) == HudLayer.LABOR_KIND_HUNT)
        HudLayer.WORK_FILTER_ATTENTION:
            return models.filter(func(m): return bool(m["attention"]))
    return models.duplicate()

func _sort_work_models(models: Array) -> void:
    if _work_sort == HudLayer.WORK_SORT_NAME:
        models.sort_custom(func(a, b): return String(a["label"]).naturalnocasecmp_to(String(b["label"])) < 0)
    else:
        models.sort_custom(func(a, b): return float(a["rate"]) > float(b["rate"]))

func _find_work_model(models: Array, key: String) -> Dictionary:
    if key == "":
        return {}
    for m in models:
        if String((m as Dictionary).get("key", "")) == key:
            return m
    return {}

## Re-send this source's `assign_labor` at a new worker count (and optionally a new policy) — the
## same emit the old Current-actions stepper made.
func _emit_work_assign(band: Dictionary, model: Dictionary, workers: int, policy: String = "") -> void:
    var kind := String(model.get("kind", ""))
    _emit_assign_labor(band, kind, workers, int(model.get("x", -1)), int(model.get("y", -1)),
        String(model.get("herd_id", "")),
        policy if policy != "" else String(model.get("policy", "")))

## Jump the map to a worked source — a fixed forage tile, or a herd at its LIVE (migrated) tile.
func _focus_work_source(model: Dictionary) -> void:
    if String(model.get("kind", "")) == HudLayer.LABOR_KIND_HUNT:
        _focus_hunt_source(String(model.get("herd_id", "")), int(model.get("x", -1)), int(model.get("y", -1)))
    else:
        focus_labor_source(int(model.get("x", -1)), int(model.get("y", -1)))

## One inspector row at a time — opening a second closes the first (and opening one costs the board
## rows, which is why `_work_board_capacity` subtracts the strip's height).
func _toggle_work_inspector(key: String) -> void:
    _work_open_key = "" if _work_open_key == key else key
    _work_policy_open = false
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
    _confirm_destructive(HudLayer.WORK_UNASSIGN_CONFIRM_FORMAT % count, HudLayer.WORK_UNASSIGN_CONFIRM_OK,
        func() -> void: _emit_cancel_order(band, HudLayer.CANCEL_SCOPE_WORK))

## Clear labor for a band at `scope` (`all` / `work` / `roles`). Main formats the
## `cancel_order <faction> <band> <scope>` command.
func _emit_cancel_order(band: Dictionary, scope: String) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band, scope)

## The behaviour hint for a source's take policy, so the row's policy GLYPH is spelled out on hover.
## Reuses the picker's existing hint strings (kind-specific: gathering a patch vs culling a herd) —
## the same sentence the player read when they chose the policy. A worked source row is ALWAYS a
## resident band's standing assignment, so the hunt side reads the LOCAL hints (never the expedition
## set, whose payoffs differ). Only `_work_source_models` asks, so it travelled with the board.
func _policy_hint(kind: String, policy: String) -> String:
    var key := policy.strip_edges().to_lower()
    if kind == HudLayer.LABOR_KIND_FORAGE:
        return String(HudLayer.FORAGE_POLICY_HINTS.get(key, ""))
    return String(HudLayer.LOCAL_HUNT_POLICY_HINTS.get(key, ""))

# ---- zone `parties` ---------------------------------------------------------

## Zone `parties`: head + `⋯` menu · one row per party in the field · the compose footer.
func build_parties_zone(band: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudLayer.ZONE_BLOCK_SEPARATION)
    var parties := _band_labor.band_parties(band)
    var menu := HudWidgets.build_section_menu([
        {"label": HudLayer.PARTY_RECALL_ALL_FORMAT % parties.size(), "disabled": parties.is_empty(),
            "on_pick": func() -> void: _on_recall_all_parties_pressed(parties)},
    ], HudLayer.PARTY_MENU_TOOLTIP)
    col.add_child(HudWidgets.zone_head(HudLayer.ZONE_HEADER_PARTIES,
        HudLayer.PARTIES_HEADER_FORMAT % [parties.size(), _band_labor.band_party_workers(band)], menu))
    if parties.is_empty():
        col.add_child(HudWidgets.alloc_hint_label(HudLayer.PARTIES_EMPTY_HINT))
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
    col.add_theme_constant_override("separation", HudLayer.PARTIES_INSPECTOR_LINE_SEPARATION)
    strip.add_child(col)
    var entity := int(exp.get("entity", -1))
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudLayer.WORK_ROW_SEPARATION)
    var title := Label.new()
    title.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id)
    title.add_theme_font_size_override("font_size", HudLayer.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudLayer.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudLayer.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudLayer.WORK_ROW_FONT_SIZE, HudLayer.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    head.add_child(close)
    col.add_child(head)
    for line in _banddetail.expedition_summary_lines(exp):
        col.add_child(HudWidgets.build_status_part(line, HudStyle.INK_DIM))
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", HudLayer.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudLayer.PARTY_INSPECT_JUMP, HudStyle.INK, func() -> void:
        select_expedition(entity, x, y)))
    links.add_child(HudWidgets.build_inline_link(HudLayer.PARTY_INSPECT_RECALL, HudStyle.DANGER, func() -> void:
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
    row.add_theme_constant_override("separation", HudLayer.WORK_ROW_SEPARATION)
    var body := Button.new()
    body.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id)
    body.alignment = HORIZONTAL_ALIGNMENT_LEFT
    body.focus_mode = Control.FOCUS_NONE
    body.clip_text = true
    body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(body, "ghost")
    if phase == HudLayer.EXPEDITION_PHASE_AWAITING:
        body.add_theme_color_override("font_color", HudStyle.WARN)
    body.tooltip_text = DetailFormat.expedition_row_tooltip(
        exp, phase, _band_labor.expedition_target_herd(exp))
    var entity := int(exp.get("entity", -1))
    body.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    row.add_child(body)
    var recall := Button.new()
    recall.text = HudLayer.PARTY_RECALL_GLYPH
    recall.focus_mode = Control.FOCUS_NONE
    recall.tooltip_text = HudLayer.PARTY_RECALL_TOOLTIP
    recall.custom_minimum_size = Vector2(HudLayer.PARTY_RECALL_WIDTH, 0.0)
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
        if mission == HudLayer.EXPEDITION_MISSION_HUNT \
        else HudLayer.PARTY_RECALL_SCOUT_LABEL
    _confirm_destructive(HudLayer.PARTY_RECALL_ONE_CONFIRM_FORMAT % label, HudLayer.PARTY_RECALL_ONE_CONFIRM_OK,
        func() -> void: _on_recall_expedition_pressed(exp))

## Recall every party in one go — there is no bulk verb on the wire and parties are few, so this is
## one `recall_expedition` per party through the existing signal.
func _on_recall_all_parties_pressed(parties: Array) -> void:
    if parties.is_empty():
        return
    _confirm_destructive(HudLayer.PARTY_RECALL_CONFIRM_FORMAT % parties.size(), HudLayer.PARTY_RECALL_CONFIRM_OK,
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
    missions.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    missions.add_child(_build_mission_launch_button(HudLayer.COMPOSE_MISSION_SCOUT,
        HudLayer.COMPOSE_MISSION_LABEL_SCOUT, HudLayer.SEND_EXPEDITION_HINT, idle))
    missions.add_child(_build_mission_launch_button(HudLayer.COMPOSE_MISSION_HUNT,
        HudLayer.COMPOSE_MISSION_LABEL_HUNT, HudLayer.SEND_HUNT_EXPEDITION_HINT, idle))
    foot.add_child(missions)
    if idle <= 0:
        foot.add_child(HudWidgets.alloc_hint_label(HudLayer.SEND_PARTY_NO_IDLE_REASON))
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
        _compose.clear_party_quarry()
        rerender())
    return btn

## The compose sheet. The mission is already settled by the footer button that opened it, so the
## sheet titles itself by mission and the policy picker is unreachable except under Hunt (it used to
## sit above the scouting button and read as if it modified it). `✕` is the only way back.
func _build_compose_sheet(band: Dictionary, idle: int) -> VBoxContainer:
    var is_hunt := _party_compose_mission == HudLayer.COMPOSE_MISSION_HUNT
    var sheet := HudWidgets.make_zone_block()
    var head := HBoxContainer.new()
    var title := Label.new()
    title.text = HudLayer.COMPOSE_TITLE_HUNT if is_hunt else HudLayer.COMPOSE_TITLE_SCOUT
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var cancel := Button.new()
    cancel.text = HudLayer.INSPECTOR_CLOSE_GLYPH
    cancel.focus_mode = Control.FOCUS_NONE
    cancel.tooltip_text = HudLayer.COMPOSE_CANCEL_TOOLTIP
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(func() -> void:
        _close_party_compose())
    head.add_child(cancel)
    sheet.add_child(head)
    if is_hunt:
        _fill_hunt_compose_sheet(sheet, band, idle)
        return sheet
    # SCOUT — a single input. Its only question is party size, and nothing about a scouting party
    # depends on where it is going, so the destination is still picked on the map after the send.
    var party_max := _scout_party_max(band, idle)
    _send_expedition_count = clampi(_send_expedition_count, HudLayer.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudLayer.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudLayer.COMPOSE_OF_IDLE_FORMAT % idle))
    sheet.add_child(HudWidgets.alloc_hint_label(HudLayer.SEND_EXPEDITION_HINT))
    var confirm := Button.new()
    confirm.text = HudLayer.SEND_EXPEDITION_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(confirm, "primary")
    confirm.tooltip_text = HudLayer.SEND_EXPEDITION_HINT
    confirm.pressed.connect(func() -> void:
        _close_party_compose()
        _on_send_expedition_pressed(band, _send_expedition_count))
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
    if herd.is_empty() or not _is_expedition_quarry(band, herd):
        herd = {}
        _compose.clear_party_quarry()
    sheet.add_child(_build_quarry_row(band, herd))
    if _compose.party_quarry_id() == "":
        # Visible-and-disabled-with-its-reason, the same convention as the idle-0 footer: the send is
        # shown so the shape of the form is legible, and it says why it is not yet pressable.
        sheet.add_child(HudWidgets.alloc_hint_label(HudLayer.COMPOSE_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = HudLayer.SEND_HUNTING_EXPEDITION_BUTTON
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = HudLayer.COMPOSE_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    if not (_send_hunt_policy in HudLayer.LABOR_HUNT_POLICIES):
        _send_hunt_policy = HudLayer.DEFAULT_HUNT_POLICY
    sheet.add_child(HudWidgets.alloc_section_label(HudLayer.COMPOSE_FIELD_POLICY))
    # With a herd in hand the four rungs finally carry their ascending per-policy metric — the same
    # `SourceForecast.expedition_policy_takes` the herd drawer feeds its picker.
    sheet.add_child(HudWidgets.build_policy_picker(func(policy: String) -> void:
        _send_hunt_policy = policy
        # Auto-max on policy select, exactly as the herd drawer does: "give me everything this herd
        # sustains" — zero waste, full rate. Consumed on the next rebuild, never set by a −/+ tick.
        _compose.arm_party_autofill()
        rerender(), _send_hunt_policy, HudLayer.LABOR_HUNT_POLICIES,
        {}, SourceForecast.expedition_policy_takes(band, herd, _band_labor.grid_width(), _band_labor.wrap_horizontal()), HudLayer.ZONE_POLICY_PICKER_COLUMNS))
    sheet.add_child(HudWidgets.alloc_hint_label(String(HudLayer.SEND_HUNT_POLICY_HINTS.get(_send_hunt_policy, ""))))
    # Party size, capped at the raid's max-useful plateau for THIS herd + policy (the herd drawer's
    # own cap), so extra hunters can no longer be sent to stand idle at the kill.
    var assignable := _scout_party_max(band, idle)
    var capped := SourceForecast.expedition_useful_cap(band, herd, _send_hunt_policy, assignable)
    var cap: int = maxi(int(capped["cap"]), HudLayer.WORKER_STEP)
    if _compose.consume_party_autofill():
        _send_expedition_count = cap
    _send_expedition_count = clampi(_send_expedition_count, HudLayer.WORKER_STEP, cap)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, cap,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudLayer.WORKER_STEP, cap)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudLayer.COMPOSE_OF_IDLE_FORMAT % idle))
    var cap_note := String(capped["note"])
    if cap_note != "":
        sheet.add_child(HudWidgets.alloc_hint_label(cap_note))
    # LIVE raid forecast for the quarry + policy + party now dialed — the same trip lookup and the
    # same one-line renderer the herd drawer uses.
    var trip := SourceForecast.hunt_trip_forecast(band, herd, _send_hunt_policy, _send_expedition_count,
        _band_labor.grid_width(), _band_labor.wrap_horizontal())
    var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip, SourceForecast.herd_display_name(herd))
    if forecast_line != "":
        sheet.add_child(HudWidgets.forecast_label(forecast_line))
    var no_surplus := SourceForecast.hunt_trip_no_surplus(trip)
    var reason := SourceForecast.hunt_no_surplus_reason(herd) if no_surplus else ""
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The button carries the verdict: slow/long/denial raids stay ENABLED and warn-styled, and only a
    # herd with no surplus disables. `SourceForecast.style_send_hunt_button` owns the text in every branch.
    SourceForecast.style_send_hunt_button(confirm, trip, reason)
    if no_surplus:
        sheet.add_child(HudWidgets.alloc_hint_label(reason))
    var quarry_id := _compose.party_quarry_id()
    confirm.pressed.connect(func() -> void:
        emit_signal("send_hunt_expedition_requested", {
            "faction": int(band.get("faction", HudLayer.PLAYER_FACTION_ID)),
            "band": int(band.get("entity", -1)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": SourceForecast.herd_display_name(herd),
            "policy": _send_hunt_policy,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## The Quarry row — the Party row's shape, with a button instead of a stepper. Unpicked it invites
## (`Choose…`, primary); picked it states the herd and stays available for a re-pick (ghost).
func _build_quarry_row(band: Dictionary, herd: Dictionary) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = HudLayer.COMPOSE_FIELD_QUARRY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    var pick := Button.new()
    pick.focus_mode = Control.FOCUS_NONE
    # EXPAND_FILL is load-bearing on the picked branch: `clip_text` drops the button's minimum width
    # to ~0, so beside an EXPAND_FILL key label it collapses to a sliver. Both branches take it so the
    # row does not resize as a quarry is chosen.
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    if herd.is_empty():
        pick.text = HudLayer.COMPOSE_QUARRY_CHOOSE
        pick.tooltip_text = HudLayer.SEND_HUNT_EXPEDITION_HINT
        HudStyle.apply_button(pick, "primary")
    else:
        var name_text := SourceForecast.herd_display_name(herd)
        pick.text = HudLayer.COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text]
        pick.clip_text = true
        pick.tooltip_text = HudLayer.COMPOSE_QUARRY_TOOLTIP_FORMAT % [
            name_text, int(herd.get("x", -1)), int(herd.get("y", -1)),
        ]
        HudStyle.apply_button(pick, "ghost")
    pick.pressed.connect(func() -> void: _on_pick_quarry_pressed(band))
    row.add_child(pick)
    return row

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
    _compose.clear_party_quarry()
    _cancel_pending_pick_quarry()
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
        if HudFormat.expedition_phase_key(exp) == HudLayer.EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

## Recall the selected in-flight expedition (folds it home). Emits recall_expedition_requested;
## Main formats the `recall_expedition …` command.
func _on_recall_expedition_pressed(expedition: Dictionary) -> void:
    if expedition.is_empty():
        return
    emit_signal("recall_expedition_requested", {
        "faction": int(expedition.get("faction", HudLayer.PLAYER_FACTION_ID)),
        "expedition": int(expedition.get("entity", -1)),
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
        _compose.clear_party_quarry()
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
    _panel.set_header(stage_id, glyph, HudFormat.band_display_name(_band_labor.panel_band(), index + 1), stage_label)
    _panel.set_cycler(index, _band_labor.player_bands().size())
    # `set_zones` above already flipped the panel to band-present; just make sure it is shown.
    _panel.set_shown(true)

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
    # The focus above rebuilt the hex's roster, so the herd is resolvable now.
    if herd_id != "" and not _selectioncard.find_roster_herd(herd_id).is_empty():
        _selectioncard.select_roster_occupant("herd", herd_id)
        emit_signal("roster_occupant_selected", "herd", herd_id)
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
