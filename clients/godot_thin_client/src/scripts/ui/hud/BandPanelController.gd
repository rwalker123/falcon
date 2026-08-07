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
## **THE PANEL'S SUBJECT IS THE FACTION PAGE, not a band** (issue #450). The pinned first entry of the
## cycler, and the one bit of state that decides which of `render_band` / `render_faction` every
## re-entry into this panel resolves to — `refresh_snapshot`, `rerender` and `_on_zones_resized`'s
## tier branch all route through it, so a snapshot tick can never drop the player back onto a band.
##
## It lives HERE rather than on `HudBandLaborState` because it is a fact about what this PANEL is
## showing, not about the world — the same test that keeps `_band_zone_tier` and `_work_page` on the
## controller. `_band_labor.panel_band()` is deliberately left ALONE while it is true, so cycling off
## the faction page returns to the band the player was on rather than to the roster's first.
var _panel_is_faction: bool = false
## Which row of the faction page's Work / Parties summaries is expanded, by the entity it is about
## (`FACTION_ROW_NONE` for none). One key for BOTH tabs: the narrow shell shows one zone at a time and
## the wide shell's two lists are about different things, so a row open in each cannot arise.
var _faction_open_row: int = FACTION_ROW_NONE

## No row expanded. Not `AttentionController.OWNER_NONE` (-1), which is a REAL row on this page — the
## faction's own land alerts — so the two sentinels must differ or opening that row is indistinguishable
## from opening none.
const FACTION_ROW_NONE := -2
## The faction page is PINNED FIRST in the cycler, and costs the walk one entry. Pinned rather than
## merely present so its position cannot drift as bands are founded or lost — a page that moved would
## have to be hunted for, which is the opposite of what a standing overview is for.
const FACTION_CYCLER_INDEX := 0

const FACTION_CYCLER_ENTRIES := 1

## **THE TWO SUBJECTS' BODIES, each naming its own zones, its own tab words and its own column
## widths** (`BandCityPanel.set_zone_layout`). A band's page is three zones — who they are, what they
## are doing, who is out; the faction page is those three one scale up plus a fourth, KNOWLEDGE, and
## its first tab reads `Faction` because that is the scope its content is at.
##
## They live HERE rather than on the panel because the panel is a generic dockable shell: it owns the
## zone KEYS (they index a persisted tab and a badge table) and the geometry, and the subject owns
## everything that says what a zone IS. That is what replaced `set_tab_label`, a per-zone label
## override that existed solely to rename one tab on one page.
##
## Written as `const` literals keyed by `BandCityPanel.ZONE_SPEC_*` — the field-name consts are what
## keep a typo from passing silently, and a builder helper on the panel would be a cross-class static
## CALL inside a `const` initializer, which evaluates at class load and is a load-order dependency
## this file does not need.
const BAND_ZONE_LAYOUT: Array[Dictionary] = [
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_BAND,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_BAND,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_BAND_WIDTH},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_WORK,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_WORK,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_WIDTH_EXPAND},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_PARTIES,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_PARTIES,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_PARTY_WIDTH},
]

## **KNOWLEDGE SITS BETWEEN WORK AND PARTIES**, which is the same order the page's own argument puts
## it in: a track is what the faction's hands may ATTEMPT, so it reads immediately after where those
## hands are and before who has left. It was a block at the bottom of the WORK zone until this pass,
## and it moved out because that zone had to carry it, Settling and Discoveries — see
## `FactionRollup.build_knowledge_zone`.
const FACTION_ZONE_LAYOUT: Array[Dictionary] = [
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_BAND,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_FACTION,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_BAND_WIDTH},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_WORK,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_WORK,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_WIDTH_EXPAND},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_KNOWLEDGE,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_KNOWLEDGE,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_KNOWLEDGE_WIDTH},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_PARTIES,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_PARTIES,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_PARTY_WIDTH},
]
## The parties compose sheet: open, and which mission has been picked ("" = none yet, which is what
## keeps the party size / floor / forecast fields hidden until the mission decides them).
var _party_compose_open: bool = false
var _party_compose_mission: String = ""
## The live PARTIES zone column, the parties twin of `_work_zone_host` — held so the deferred
## measurement below can read what the zone's content demands off the REAL laid-out tree rather than
## off a detached one. `HudWidgets.wrap_zone` anchors this column full-rect into the panel's zone host,
## so what it demands is exactly what the host must hold.
var _parties_zone_col: VBoxContainer = null
## The compose sheet built by the current render, held so the measurement a frame later can tell that
## the sheet it is measuring is still the one in the zone.
var _party_compose_sheet: Control = null
## **THE FLOAT'S TRIGGER, AND IT IS A MEASUREMENT — never the dock edge.** What the parties zone's
## whole column demanded — head, party rows, open inspector strip AND the composed sheet — the last
## time the sheet was rendered INSIDE it: the column's own combined minimum height. A short vertical
## dock and a small window hit the same wall as a horizontal one, and an edge test misses both.
##
## **IT IS THE COLUMN'S MINIMUM, NOT THE SHEET'S OFFSET PLUS ITS MINIMUM, and the difference is not
## cosmetic.** The footer is bottom-pinned by an `EXPAND_FILL` spacer, so the spacer absorbs exactly
## the slack and `sheet_top + sheet_minimum == box height` holds BY CONSTRUCTION whenever the content
## fits — the positional read is degenerate at the boundary and answered "2px over" on a column with
## room to spare. A container's combined minimum has no such feedback: it is the sum the layout would
## need, spacer contributing nothing.
##
## **IT IS MEASURED LIVE AND A FRAME LATE, because Godot has no synchronous layout.** An unsorted
## control tree reports an autowrap `Label`'s minimum at a wrap width of ZERO — every word on its own
## line — so a build-time measurement of this sheet over-reports by hundreds of pixels and would float
## it in a side dock that holds it comfortably (measured: **1278px against a laid-out 207**).
## `_measure_party_compose` therefore waits for the deferred layout pass and reads the column the panel
## actually laid out — and it decides that the pass has happened by the SHEET's own width, never the
## column's, the column being anchored and so sized synchronously whether or not anything under it is.
##
## **IT IS A HIGH-WATER MARK for one composing act**, and it is reset by every path that ends that act
## — `_close_party_compose`, a panel-band change, the panel losing its last band. The sheet grows as
## the form is answered (a picked quarry adds the policy rungs, the party stepper, the kit row and the
## forecast), and a mark that tracked every shrink would hop the sheet back into the zone the moment a
## field cleared, which is a layout change under the player's hands.
##
## **AND IT BELONGS TO ONE BOX**, which is what `_party_compose_measured_box` records beside it. The
## mark answers "what did this sheet demand of THAT column"; a dock move from a 265px bottom strip to a
## 1055px side dock asks a different question, so the answer is dropped rather than carried into it.
var _party_compose_needed: float = 0.0
## The parties-zone box `_party_compose_needed` was measured against — `Vector2.ZERO` for "no mark".
## Compared every render by `_note_parties_zone_box`, which is what drops a mark the dock outgrew.
var _party_compose_measured_box: Vector2 = Vector2.ZERO
## One deferred measurement in flight at a time.
var _party_compose_measuring: bool = false
## The compose sheet floated off the zone (see `BandComposeFloat`). A node, so it hangs off `_host` —
## a `RefCounted` cannot parent, the same reason `_confirm_destructive` parents its dialog there.
## Built lazily on the first render that needs it, so a session that never overflows never makes one.
var _compose_float: BandComposeFloat = null
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = HudConst.WORKER_STEP
# Compose state for the hunt-expedition launch FLOOR — where the raid stops, `0.0..=1.0`. **This zone
# is the SECOND launch site of `send_hunt_expedition`**, and the arc's standing rule is that the two
# entry points cannot offer different orders: a lever present on one sheet and absent on the other is
# the same defect as a lever that does nothing. The floor is the ONLY order a raid now carries — the
# fill target that used to ride beside it is retired (issue #491) — so this is the whole of that state.
var _send_hunt_floor: float = SourceForecast.DEFAULT_HARVEST_FLOOR

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

## `_topbar` is held for **the player faction's own three readouts and nothing else** — its knowledge
## `faction_tracks` (the rung-ready mark on a work row, the narrow reason `DrawerComposeController`
## holds it), and since the four-zone body its `faction_sedentarization` / `faction_discovered_sites`,
## which are the Knowledge zone's other two blocks. A typed collaborator rather than a Callable
## injection, per the extraction rules.
##
## **The set is bounded by what that cluster IS, not by a count.** It is the FACTION-scope readout
## cluster; a read of anything else — a label node, a per-band figure, the turn — is a different
## collaborator's and does not belong here.
var _topbar: TopBarReadouts = null

## `_attention` is held for `build_band_attention` ONLY — the faction page's Work and Parties tabs
## group that array by owner. A typed collaborator, and read for nothing else: the alerts are the
## attention model's answer, and this controller must not grow a second opinion about them.
var _attention: AttentionController = null

## Injected by `HudLayer._ready`, once `_attention` exists there.
func set_attention(attention: AttentionController) -> void:
    _attention = attention

## The player faction's {track: progress} row, threaded into every `RungGates` call.
func _player_knowledge() -> Dictionary:
    return _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) if _topbar != null else {}

## The player faction's sedentarization entry (`{score, stage}`), for the Knowledge zone's SETTLING
## block. `{}` when the snapshot has not carried one — the block renders nothing rather than a zero.
func _faction_settling() -> Dictionary:
    return _topbar.faction_sedentarization() if _topbar != null else {}

## The player faction's discovered Wondrous Sites, for the Knowledge zone's DISCOVERIES block. The
## raw site array the top bar's own strip is built from, so the two cannot disagree about what has
## been found.
func _faction_discoveries() -> Array:
    return _topbar.faction_discovered_sites() if _topbar != null else []

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
        improvement: String = SourceForecast.IMPROVEMENT_NONE,
        kit_id: String = KitRoster.NO_KIT_ID) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, floor, species, improvement,
        kit_id)

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

## The PARTIES zone's own box. Its HEIGHT is `_zone_box()`'s — every zone shares the card's one body
## height — but the wide shell's parties flank is a FIXED width where the work board's column expands,
## and the compose sheet is authored for, measured in and floated at THIS column, not that one.
func _parties_zone_box() -> Vector2:
    var box := _parties_zone_box_known()
    return box if box != Vector2.ZERO else HudWorkVocab.ZONE_FALLBACK_SIZE

## The parties zone's box **or `Vector2.ZERO` meaning "the panel cannot answer yet"** — the honest
## reading its guessed-fallback twin above cannot give. `BandCityPanel.zone_size()` returns
## ZERO while the panel is collapsed, hidden, or simply has not laid out yet, which is every frame
## before the first layout pass.
##
## **THE FLOAT DECISION MUST READ THIS ONE, NEVER THE FALLBACK.** `ZONE_FALLBACK_SIZE` is 340×360 —
## fine as a layout guess for a no-dock host, and nothing at all like the ~1055px a tall side dock
## really offers — so deciding against it turns "I do not know yet" into "this sheet overflows", and
## the float latches (see `_party_compose_needed`). Reported from play: an EMPTY compose sheet, a
## couple of hundred px tall, floated out of a left dock that held it four times over.
func _parties_zone_box_known() -> Vector2:
    if _panel == null:
        return Vector2.ZERO
    var box: Vector2 = _panel.zone_size(BandCityPanel.ZONE_PARTIES)
    return box if box.x > 0.0 and box.y > 0.0 else Vector2.ZERO

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
    # The faction page has no height tier and no paged board — its two lists are bounded by a row COUNT
    # rather than measured against the box — so a resize is simply a re-render. Falling through would
    # compare against the tier the last BAND render left behind and, on a match, re-page a work host
    # this page does not own.
    if _panel_is_faction:
        rerender()
        return
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
            float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)), {}))
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
            # **THE KIT THIS CREW IS ALREADY WORKING UNDER** (`LaborAssignment.kitId`, always a real
            # roster id on a forage/hunt row). It rides the model for one reason: `_emit_work_assign`
            # RESTATES it, so a `+`/`−` on the board cannot silently re-kit a crew back to the job
            # default — the same rule, and the same failure, as the improvement axis beside it.
            "kit_id": String(m.get("kit_id", KitRoster.NO_KIT_ID)),
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
        "", String(model.get("improvement", "")),
        # **THE KIT RIDES EVERY CREW EDIT, for the improvement's reason.** An omitted `kit <id>` token
        # means "the job's default" to the parser, so a `+`/`−` that dropped it would re-kit a crew
        # the player deliberately sent out bare-handed. Restated from the row model, which carries the
        # assignment's own `kit_id`.
        String(model.get("kit_id", KitRoster.NO_KIT_ID)))

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
    # BEFORE anything reads the latched float requirement below: a box change invalidates the mark.
    _note_parties_zone_box()
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    # Held for the deferred compose-sheet measurement, which needs the zone's own laid-out rect to
    # know where the footer ended up inside it (see `_party_compose_needed`).
    _parties_zone_col = col
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
    links.add_child(HudWidgets.build_inline_link(recall_verb(exp), HudStyle.DANGER, func() -> void:
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
    # The GLYPH is the same either way (a ✕ removes the row on both branches); the tooltip is what says
    # whether the press cancels an order that never took effect or sends the party on a walk home.
    recall.tooltip_text = recall_tooltip(exp)
    recall.custom_minimum_size = Vector2(HudComposeVocab.PARTY_RECALL_WIDTH, 0.0)
    HudStyle.apply_button(recall, "ghost")
    # DANGER-red like the Work inspector's destructive "Unassign" link — it removes a party. The steady
    # red already reads as destructive, so it rests at full opacity (no alpha dim) and brightens no
    # further on hover. Confirms before recalling (its own single-party prompt, NOT the raw emit).
    recall.add_theme_color_override("font_color", HudStyle.DANGER)
    recall.pressed.connect(func() -> void: confirm_recall_expedition(exp))
    row.add_child(recall)
    return row

## The verb a single-party recall control wears for `exp` — `Cancel` where the sim will fold the party
## back on the spot, `Recall` where it will walk home. The row ✕, the parties inspector link and the
## Occupants drawer's button all read THIS, so they cannot promise different things about one press.
func recall_verb(exp: Dictionary) -> String:
    return HudComposeVocab.PARTY_CANCEL_VERB if _band_labor.party_cancels_in_camp(exp) \
        else HudComposeVocab.PARTY_RECALL_VERB

## The tooltip that goes with `recall_verb` — same fork, same one reading of the predicate.
func recall_tooltip(exp: Dictionary) -> String:
    return HudComposeVocab.PARTY_CANCEL_TOOLTIP if _band_labor.party_cancels_in_camp(exp) \
        else HudComposeVocab.PARTY_RECALL_TOOLTIP

## Act on a SINGLE party's recall. Wraps the button handlers (row ✕, inspector link, drawer button) —
## NOT the shared `_on_recall_expedition_pressed` emit, which "Recall all" loops under its own one
## confirm. The prompt names the party (hunt → its herd, scout → the mission word).
##
## **A CANCEL ASKS NOTHING AND FIRES ON THE PRESS.** `_confirm_destructive` is for an action that LOSES
## something — the work board's unassign-all, a real recall abandoning a trip in progress. A party still
## standing in its home band's camp has spent no travel and abandoned no haul, and re-launching it is
## one press of the same footer button, so a modal there is ceremony over a decision the player can
## simply re-make. The bulk `Recall all` keeps its single confirm regardless: it acts over a MIXED set,
## where the prompt is the only place the whole scope is stated.
func confirm_recall_expedition(exp: Dictionary) -> void:
    if _band_labor.party_cancels_in_camp(exp):
        _on_recall_expedition_pressed(exp)
        return
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
        var sheet := _build_compose_sheet(band, idle)
        _party_compose_sheet = sheet
        # **THE ONE FORK, AND IT IS DECIDED BY A MEASUREMENT** (`_party_compose_needed` carries the
        # whole rationale): the sheet the zone cannot hold is the SAME sheet, from the same builders in
        # the same order, rendered in a card floated beside the panel instead of sliced by a
        # `clip_contents` host. Nothing about the form changes — only which node it is parented into.
        if _party_compose_floats():
            _mount_compose_float(sheet)
        else:
            _dismiss_compose_float()
            foot.add_child(sheet)
        return foot
    # No sheet open (or no idle workers to compose one with) ⇒ no float. Every teardown path — the ✕,
    # a cancel, a send, a panel-band change, the last idle worker leaving — reaches the footer builder,
    # so the float dies here rather than on a list of conditionals that can miss one.
    _party_compose_sheet = null
    _dismiss_compose_float()
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
    btn.set_meta(HudWidgets.MISSION_LAUNCH_META, mission)
    btn.pressed.connect(func() -> void:
        _party_compose_open = true
        _party_compose_mission = mission
        # A fresh compose act starts with no quarry — never a herd left over from a cancelled one.
        _clear_party_quarry()
        # **THE DENIAL SHEET ALWAYS OPENS ON THE PARTY THE SIM QUOTES**, so the seed is armed by the
        # sheet OPENING as well as by a quarry being adopted — a sheet that came back up on a quarry
        # it still remembered would otherwise present whatever count the last composition left behind.
        # Same one-shot either way (`consume_party_autofill`), so a manual −/+ tick still survives
        # every rerender while the sheet stays open, and it is still never seeded to 0.
        if mission == HudComposeVocab.COMPOSE_MISSION_DENY:
            _compose.arm_party_autofill()
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
    # **THE CEILING IS THE BAND'S IDLE WORKERS**, as it is on all three launch verbs: the sim carries
    # no rules cap on party size, and `max_expedition_party_size` is the wire echo of the estimate
    # tables' sampling axis rather than a limit anyone may send under.
    var party_max := idle
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
    # **THE KIT, RESOLVED BEFORE ANYTHING IS QUOTED AND MOUNTED UNDER THE PARTY STEPPER.** Its
    # selection decides whether the sim's `huntTripEstimates` table applies to this raid at all, and
    # the picker's ROW belongs beneath the crew it describes — so the resolve is here and the mount is
    # further down. `party_kit_id` is shared with the denial mission (one sheet, two missions, both on
    # the `hunt` job) and re-validated every render.
    var kits := _band_labor.kits()
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_HUNT, default_kit,
        _compose.party_kit_id())
    _compose.set_party_kit_id(kit_id)
    # **THE HONESTY GATE.** `huntTripEstimates` is quoted for ONE kit (the hunt job's default) and is
    # not repriced per kit, so this sheet may present it as the answer only when the ids agree. Compare
    # them — never assume the default is selected.
    var trip_quoted := KitRoster.estimates_apply_to(herd, KitRoster.HERD_TRIP_ESTIMATES_KIT_KEY,
        default_kit, kit_id)
    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.COMPOSE_FIELD_POLICY))
    # With a herd in hand the presets finally carry their metric — the same
    # `SourceForecast.expedition_policy_takes` the herd drawer feeds its picker.
    #
    # **THE METRICS GO WITH THE TABLE THEY COME FROM.** `expedition_policy_takes` is a reading of
    # `huntTripEstimates`, so under a kit the table is not quoted for it would put a fourth figure
    # priced at a different kit on a sheet whose own note says none are. `{}` is the picker's supported
    # degrade (a herd the wire does not describe), so the rungs render bare rather than wrong.
    #
    # **THREE ACROSS, the shared default** — the zone's own 2-column clamp is retired. It existed
    # because the long preset faces (`💀 Take everything`) could not fit three in a 354px column and
    # wrapped `↑ Learn from it` onto a second row; the faces are one word each now
    # (`HudComposeVocab.FLOOR_PRESET_LABELS`), so the picker reads as one row here and in the drawer.
    sheet.add_child(HudWidgets.build_floor_picker(func(floor: float) -> void:
        _send_hunt_floor = floor
        # Auto-max on a floor click, exactly as the herd drawer does: "give me everything this herd
        # can spare" — zero waste, full rate. Consumed on the next rebuild, never set by a −/+ tick.
        _compose.arm_party_autofill()
        rerender(), _send_hunt_floor,
        SourceForecast.expedition_policy_takes(band, herd, _band_labor.grid_width(), _band_labor.wrap_horizontal()) if trip_quoted else {}))
    # Party size, capped at the raid's max-useful plateau for THIS herd + floor (the herd drawer's
    # own cap), so extra hunters can no longer be sent to stand idle at the kill. **The SUPPLY side is
    # the band's idle workers alone** — `max_expedition_party_size` is a sampling axis, not a rules
    # cap — and `expedition_useful_cap` is the DEMAND side the stepper takes the tighter of.
    #
    # **THE DEMAND SIDE IS ALSO A READING OF THE TABLE**, so under a kit mismatch the stepper falls
    # back to supply alone: with no table for this kit the plateau is unknown, and clamping to another
    # kit's plateau would refuse a party this one may well need.
    #
    # **THE CAP IS RESOLVED HERE, ABOVE THE CHART, AND THE ROW IT FEEDS IS MOUNTED FURTHER DOWN.** The
    # chart's projection, its two crew targets and its verdict are all read against a CREW, so
    # composing them ahead of the clamp states a verdict for a party the stepper beneath then refuses
    # to show — visible for exactly one frame, on the render where autofill arms (a floor click, a
    # committed drag, a fresh quarry), which is the render a player is always looking at. The forage
    # sheet's twin ordering, and the assertion that judges both, are in `labor-ui.md`.
    var assignable := idle
    var capped := SourceForecast.expedition_useful_cap(band, herd, _send_hunt_floor, assignable) \
        if trip_quoted else {"cap": assignable, "note": ""}
    var cap: int = maxi(int(capped["cap"]), HudConst.WORKER_STEP)
    if _compose.consume_party_autofill():
        _send_expedition_count = cap
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, cap)
    # **THE CHART AND ITS DRAGGABLE FLOOR — the same builder and the same model the herd drawer's raid
    # uses**, because the two entry points compose one decision and had no business presenting it two
    # ways. `improvement` is `IMPROVEMENT_NONE` and the crew noun is the party's: a detached party
    # builds nothing, exactly as the drawer's expedition branch already assumes.
    #
    # **GATED ON THE ZONE HAVING ROOM, the `_build_food_outlook_block` idiom.** A horizontal dock's
    # parties zone is height-capped and CLIPS, and the chart is ~150px of it — so the SHORT tier keeps
    # the presets alone, exactly as it keeps the band zone's outlook chart out. The drag goes with it:
    # since slice 4b there is no plain-slider control left to keep, the chart's own floor flag IS the
    # dial (see `HudWidgets.build_floor_chart`).
    var chart_model := SourceForecast.floor_chart_model(herd, SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, _send_hunt_floor, _send_expedition_count,
        SourceForecast.IMPROVEMENT_NONE, HudComposeVocab.COMPOSE_FIELD_PARTY.to_lower(),
        SourceForecast.rung_lesson_known(SourceForecast.SOURCE_KIND_HERD, herd,
            HudComposeVocab.BARE_FORECAST_PREFIX, _player_knowledge()))
    if bool(chart_model.get("known", false)) and _band_zone_tier != HudWorkVocab.BAND_ZONE_TIER_SHORT:
        sheet.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _send_hunt_floor = floor
                # **ONLY A COMMITTED CHANGE REBUILDS**, the drawer's expedition rule: a rebuild frees
                # the chart and the drag in flight dies with it, and this sheet has no live-refresh
                # registry to update in place (the raid's numbers are a lookup into a table sampled at
                # five floors, so most of a drag moves nothing anyway).
                if committed:
                    _compose.arm_party_autofill()
                    rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(
        HudFormat.floor_hint(_send_hunt_floor, SourceForecast.LABOR_KIND_HUNT, true)))
    # The stepper ROW, mounted where the form reads it — under the chart the settled count above was
    # composed into, and above the kit picker the party carries.
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, cap,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, cap)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    var cap_note := String(capped["note"])
    if cap_note != "":
        sheet.add_child(HudWidgets.alloc_hint_label(cap_note))
    _mount_kit_row(sheet, kits, KitRoster.JOB_HUNT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_party_kit_id(picked)
            rerender())
    var quarry_id := _compose.party_quarry_id()
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    confirm.set_meta(HudWidgets.SEND_HUNT_CONFIRM_META, true)
    if not trip_quoted:
        # **THE KIT-MISMATCH SHEET.** The table is not an answer for this party, so nothing derived
        # from it renders: no forecast line, no bound clause, no empty-raid refusal. What DOES render
        # is honest for any kit — the
        # combat gate, composed from wire terms — plus the sentence naming whose numbers were
        # suppressed. The send stays live: the raid is perfectly launchable, we simply cannot quote
        # its length.
        _mount_kit_gate_line(sheet, kits, kit_id, band, herd,
            SourceForecast.herd_display_name(herd))
        sheet.add_child(HudWidgets.alloc_hint_label(KitRoster.estimates_quoted_note(kits, herd,
            KitRoster.HERD_TRIP_ESTIMATES_KIT_KEY, default_kit, kit_id,
            HudComposeVocab.KIT_TRIP_ESTIMATES_QUOTED_FORMAT)))
        SourceForecast.style_send_hunt_button(confirm, {}, "")
    else:
        # **THE TRIP READOUT — the herd drawer's boxed section, from the shared builder.** This zone
        # answered with a one-line bbcode sentence and a standalone bound clause beside it, which is
        # what let the two entry points drift: on a Wild Fowl flock the drawer laid out a full box
        # here and this sheet rendered nothing at all. The box's own VERDICT folds the bound clause
        # in (`SourceForecast.hunt_trip_verdict`), so the standalone line went with the sentence —
        # keeping both would have printed one fact twice.
        var trip := SourceForecast.hunt_trip_forecast(band, herd, _send_hunt_floor, _send_expedition_count,
            _band_labor.grid_width(), _band_labor.wrap_horizontal())
        if SourceForecast.hunt_trip_delivers(trip):
            HudWidgets.mount_trip_readout(sheet, trip, SourceForecast.herd_display_name(herd),
                _send_hunt_floor)
        else:
            # A raid with nothing to lay out in rows — no estimate, a denial quarry, a herd at its
            # floor — keeps the ONE-LINE form, exactly as the drawer's branch does. An empty box is
            # worse than the sentence it would replace.
            var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip,
                SourceForecast.herd_display_name(herd))
            if forecast_line != "":
                sheet.add_child(HudWidgets.forecast_label(forecast_line))
        # **WHICH PARTY THE FIGURES ABOVE WERE COSTED FOR**, whenever the ladder rounded the selected
        # one to a neighbouring rung. The take scales with party size, so a row read against a party it
        # was not computed for misstates it; the kit line's idiom, for the kit line's reason.
        var party_note := SourceForecast.quoted_party_note(trip, _send_expedition_count,
            HudComposeVocab.PARTY_TRIP_ESTIMATES_QUOTED_FORMAT)
        if party_note != "":
            sheet.add_child(HudWidgets.alloc_hint_label(party_note))
        # WHY an empty raid is empty comes off the sim's `bound`, so the reason takes the TRIP beside
        # the herd — "wait for the herd to rebuild" and "send more hunters" are opposite instructions.
        var returns_empty := SourceForecast.hunt_trip_returns_empty(trip)
        var reason := SourceForecast.hunt_empty_refusal_reason(trip, herd) if returns_empty else ""
        # The button carries the verdict: slow/long/denial raids stay ENABLED and warn-styled, and only
        # a herd with no surplus disables. `style_send_hunt_button` owns the text in every branch.
        SourceForecast.style_send_hunt_button(confirm, trip, reason)
        if returns_empty:
            sheet.add_child(HudWidgets.alloc_hint_label(reason))
    confirm.pressed.connect(func() -> void:
        emit_signal("send_hunt_expedition_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": SourceForecast.herd_display_name(herd),
            "floor": _send_hunt_floor,
            # The kit the party walks out with, and the job default `Main` omits the token for.
            "kit_id": kit_id,
            "default_kit_id": default_kit,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## Mount the kit row where a sheet wants it — a no-op when the roster offers this job no kit at all,
## so a sheet rendered before the first snapshot (or against a world whose roster does not cover the
## verb) is byte-identical to what it was before the picker existed.
func _mount_kit_row(sheet: VBoxContainer, kits: Array, job: String, kit_id: String,
        default_kit: String, band: Dictionary, on_pick: Callable) -> void:
    var row := KitRoster.build_kit_row(kits, job, kit_id, default_kit, band, on_pick)
    if row != null:
        sheet.add_child(row)

## **THE ONE FORECAST THAT STAYS HONEST FOR EVERY KIT**, rendered where the estimate tables have been
## suppressed. It is composed from wire terms — `max(0, attack − defense)` against the species'
## durability — at the SELECTED kit's effective attack rather than at the band's default-kit tier, so
## a bare-handed party against a defended species reads the plain refusal instead of a blank sheet.
## Same ink and same meta as the herd drawer's gate line, so the two surfaces cannot state one fight
## two ways.
func _mount_kit_gate_line(sheet: VBoxContainer, kits: Array, kit_id: String, band: Dictionary,
        herd: Dictionary, quarry: String) -> void:
    var tiers := KitRoster.effective_tiers(kits, KitRoster.kit_by_id(kits, kit_id), band)
    var gate := SourceForecast.hunt_gate_model_at(float(tiers["attack"]), herd, quarry)
    # **ONLY THE REFUSAL RENDERS.** The winnable branch used to state the effort in hunter-turns; that
    # face is retired (a species constant beside a forecast that already prices the trip), so a fight
    # this party CAN take says nothing here and the sheet's remaining lines are the answer.
    if not bool(gate["blocked"]):
        return
    var gate_label := HudWidgets.forecast_label("[color=#%s]%s[/color]" % [
        HudStyle.DANGER_HEX, String(gate["text"])])
    gate_label.set_meta(HudWidgets.HUNT_GATE_META, true)
    sheet.add_child(gate_label)

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
    # (`snapshot.fbs` → `denialEstimates`). It is the wire echo of the LAST RUNG of
    # `expedition_config.estimate_party_sizes` — the top of the estimate tables' SAMPLED party axis, and
    # the sole quoting bound (it absorbed the retired `deny.max_party_quoted`) — and the sim deleted the
    # rules cap for all three launch verbs, so the client's own clamp was the last thing enforcing it. A
    # party past the top rung is quoted at that rung with a note naming it, never refused; and a band with 16 idle
    # workers was clamped to 8 while this sheet told it to send more hunters. All three launch forms
    # read the supply the same way now, which is why the `_scout_party_max` helper no longer exists.
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
    var quarry_name := SourceForecast.herd_display_name(herd)
    # **THE KIT, DIRECTLY UNDER THE PARTY IT DESCRIBES.** It is the only order this closed-grammar
    # mission still has to give besides the party size, and it moves every figure below it — a `none`
    # raid against a defended species has an effective attack of ZERO and no party size works at all.
    var kits := _band_labor.kits()
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_HUNT, default_kit,
        _compose.party_kit_id())
    _compose.set_party_kit_id(kit_id)
    _mount_kit_row(sheet, kits, KitRoster.JOB_HUNT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_party_kit_id(picked)
            rerender())
    # **THE HONESTY GATE — COMPARE THE IDS, NEVER ASSUME THE DEFAULT IS SELECTED.** `denialEstimates`
    # is quoted for ONE kit and repricing it per kit was scoped out, so everything below that reads the
    # table — the collapse verdict, its caveat, the take line, the repelled refusal, the short-handed
    # disable — is suppressed rather than shown for a raid the player is not sending.
    var quoted := KitRoster.estimates_apply_to(herd, KitRoster.HERD_DENIAL_ESTIMATES_KIT_KEY,
        default_kit, kit_id)
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    confirm.set_meta(HudWidgets.SEND_DENIAL_CONFIRM_META, true)
    var reason := ""
    if not quoted:
        # What survives is the combat GATE, which is composed from wire terms and is therefore honest
        # for any kit, plus the sentence naming the kit whose numbers were withheld. The send stays
        # live and plainly styled: the raid launches, we simply cannot say how long it takes.
        _mount_kit_gate_line(sheet, kits, kit_id, band, herd, quarry_name)
        sheet.add_child(HudWidgets.alloc_hint_label(KitRoster.estimates_quoted_note(kits, herd,
            KitRoster.HERD_DENIAL_ESTIMATES_KIT_KEY, default_kit, kit_id,
            HudComposeVocab.KIT_DENIAL_ESTIMATES_QUOTED_FORMAT)))
        SourceForecast.style_send_denial_button(confirm, {}, false)
    else:
        # THE COLLAPSE VERDICT — the sim's `denialEstimates` row for this party size, on the clock the
        # player is on. **The band and the grid pair are passed for the OUTBOUND WALK**: the table
        # counts raiding turns, and this sheet's hunt form has always headlined a round-trip total, so
        # a verdict quoting bare raiding turns beside it named a shorter span in the same words.
        var forecast := SourceForecast.denial_forecast(herd, _send_expedition_count, band,
            _band_labor.grid_width(), _band_labor.wrap_horizontal())
        var verdict := SourceForecast.denial_verdict_bbcode(forecast, quarry_name)
        if verdict != "":
            sheet.add_child(HudWidgets.forecast_label(verdict))
            # The caveat rides under the verdict WHENEVER THERE IS A NUMBER TO CAVEAT — the band is an
            # integral over many stochastic draws and a lucky run really can finish sooner than the
            # reported low. A verdict with no turn count (a repelled party, an unbounded horizon) has
            # nothing for it to qualify, and a caveat about an absent number reads as one that is there.
            if SourceForecast.denial_turns_phrase(forecast) != "":
                sheet.add_child(HudWidgets.alloc_hint_label(SourceForecast.DENIAL_ESTIMATE_CAVEAT))
        # …and the take beneath it: what the raid kills, what little it hauls, and what it leaves on
        # the range. Quiet ink — the waste IS the mission, not a warning about it.
        var take := SourceForecast.denial_take_bbcode(forecast, quarry_name)
        if take != "":
            sheet.add_child(HudWidgets.forecast_label(take))
        # **WHICH PARTY THE VERDICT AND THE TAKE WERE COSTED FOR**, when the denial axis rounded the
        # stepper's count to a neighbouring rung. It rides ABOVE the refusal below it deliberately:
        # the refusal names the party the HERD requires, and this names the party the FIGURES describe
        # — two different numbers, and the one qualifying what is on screen comes first.
        var party_note := SourceForecast.quoted_party_note(forecast, _send_expedition_count,
            HudComposeVocab.PARTY_DENIAL_ESTIMATES_QUOTED_FORMAT)
        if party_note != "":
            sheet.add_child(HudWidgets.alloc_hint_label(party_note))
        # **THE SHORT-HANDED SENTENCE SUPERSEDES THE REFUSAL, it does not join it.** Both name the
        # party the sim quotes (one reading, `denial_party_needed`), so printing the pair would state
        # the requirement twice; the short-handed form also says what the band actually has.
        var short_handed := SourceForecast.denial_is_short_handed(herd, idle)
        reason = SourceForecast.denial_short_handed_reason(herd, idle)
        if reason == "":
            reason = SourceForecast.denial_refusal_reason(forecast, herd)
        if reason != "":
            sheet.add_child(HudWidgets.alloc_hint_label(reason))
        # The button carries the verdict, and disables in EXACTLY ONE case — a band that cannot field
        # the party this herd requires at all. A party the player CHOSE to under-size still launches:
        # it works the herd until recalled, so that case warns and the player is trusted.
        SourceForecast.style_send_denial_button(confirm, forecast, short_handed)
    confirm.tooltip_text = reason if reason != "" else HudComposeVocab.SEND_DENIAL_RAID_HINT
    var quarry_id := _compose.party_quarry_id()
    confirm.pressed.connect(func() -> void:
        emit_signal("send_denial_raid_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": quarry_name,
            # The party's kit, and the job default `Main` omits the `kit <id>` token for — the only
            # order the four-token grammar admits beyond the two it already carries.
            "kit_id": kit_id,
            "default_kit_id": default_kit,
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

## The Quarry row — the Band and Kit rows' shape, with a button instead of a picker. Unpicked it
## invites (`Choose…`, primary); picked it states the herd and stays available for a re-pick (ghost).
##
## **IT IS PRESENTED AS ONE OF THAT FAMILY AND IT IS NOT ONE OF THEIR KIND, and both halves of that
## are deliberate.** It takes the shared key label (`HudWidgets.build_field_key`, one declared width),
## the same ghost chrome and therefore the same height and the same left-aligned face — so the three
## field rows on a sheet read as one stack rather than three different-looking widgets. What it must
## NEVER take is dropdown chrome: pressing it ARMS A MAP PICK. Quarries are chosen spatially — glow
## rings on the eligible herds, the targeting banner, the in-reach refusal nudge — and the candidates
## are scattered across the map rather than enumerable in a sensible list, so an arrow here would
## promise a list that never opens, which is worse than the inconsistency it would paper over. The one
## list this row does offer is the `⋯` chooser at the end, and it appears only where a hex genuinely
## holds more than one eligible quarry.
func _build_quarry_row(band: Dictionary, herd: Dictionary) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.add_child(HudWidgets.build_field_key(HudComposeVocab.COMPOSE_FIELD_QUARRY))
    var pick := Button.new()
    pick.focus_mode = Control.FOCUS_NONE
    # EXPAND_FILL is load-bearing on the picked branch: `clip_text` drops the button's minimum width
    # to ~0, so beside the key label it collapses to a sliver. Both branches take it so the row does
    # not resize as a quarry is chosen.
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # LEFT, the alignment an `OptionButton` takes for itself — a `Button`'s stock CENTER would put the
    # quarry's name in the middle of its box beside two pickers whose values start hard against the
    # box's leading edge. It also puts the species ART immediately before the name it belongs to
    # rather than at the far end of the button.
    pick.alignment = HORIZONTAL_ALIGNMENT_LEFT
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
        # **THE CHOOSER'S WIDTH COMES OUT OF THE PICK, NOT OUT OF THE KEY**, and that is structural
        # now rather than a per-branch override. `build_field_key` takes a DECLARED width and does not
        # expand, so the pick is the row's only expanding child whether the row has two children or
        # three — the chooser simply takes its own width out of the pick's share. The defect this
        # replaced was the key EXPANDING too, which halved the name's room the moment a third control
        # appeared: `🐇 Rabbit Warren` came back clipped to `Rabbit Warre` on the very frame the
        # chooser exists to serve, and the cure was a `SIZE_FILL` written into this branch alone.
        if candidates.size() > 1:
            row.add_child(_build_quarry_choices_menu(band, herd, candidates, mission))
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
    # The measured requirement belongs to ONE composing act — see `_party_compose_needed`. Carrying a
    # closed form's high-water mark into the next one would float a sheet that has not been measured.
    _party_compose_needed = 0.0
    _party_compose_measured_box = Vector2.ZERO
    # Explicitly, as well as through the render below: `rerender()` is a no-op with no panel or no
    # panel band, and a float outliving its sheet is the worst outcome available here.
    _party_compose_sheet = null
    _dismiss_compose_float()
    rerender()

# ---- the compose sheet's FLOAT (see `ui/hud/BandComposeFloat.gd`) --------------------------------

## Does the composed sheet have to leave the zone? **A MEASUREMENT, never a dock-edge test** —
## `_party_compose_needed` is what the parties column demanded the last time the zone actually held the
## sheet; the box is the zone the panel currently offers.
##
## **AN UNKNOWN BOX ANSWERS `false`, and that asymmetry is deliberate.** Floating is the drastic,
## instantly-visible branch, so it has to be POSITIVELY justified — never taken on a guessed
## `ZONE_FALLBACK_SIZE` that stands in for a box the panel has not laid out yet. The worst case of
## staying inline is one clipped frame, which is what shipped for months and is strictly better than a
## sheet leaping onto the map.
func _party_compose_floats() -> bool:
    var box := _parties_zone_box_known()
    if box == Vector2.ZERO:
        return false
    return _party_compose_needed > box.y + HudComposeVocab.COMPOSE_FLOAT_SLACK

## Float `sheet` beside the panel card. Builds the float on first use — a session whose sheets always
## fit never makes one — and parents it on the HUD `CanvasLayer`, since a `RefCounted` cannot.
func _mount_compose_float(sheet: Control) -> void:
    if _host == null or _panel == null:
        return
    if _compose_float == null or not is_instance_valid(_compose_float):
        _compose_float = BandComposeFloat.new()
        _host.add_child(_compose_float)
    _compose_float.mount(sheet, _panel.card_rect(),
        BandComposeFloat.map_facing_side(_panel.get_dock()), _parties_zone_box().x)

func _dismiss_compose_float() -> void:
    if _compose_float != null and is_instance_valid(_compose_float):
        _compose_float.dismiss()

## Is the compose sheet currently floated? Read by `band_panel_preview`, which has to assert BOTH that
## the sheet left the zone and that it fits the viewport beside the card.
func compose_is_floating() -> bool:
    return _compose_float != null and is_instance_valid(_compose_float) and _compose_float.is_floating()

## The float node, or `null` if one was never needed. For the harness's rect assertions.
func compose_float() -> BandComposeFloat:
    return _compose_float

## **MEASURE THE SHEET WHERE THE PANEL ACTUALLY PUT IT, ONCE IT HAS BEEN LAID OUT.** Godot lays out
## through the message queue, so nothing built during a render has a rect (or, for an autowrap `Label`,
## an honest minimum height — a detached one shapes at a wrap width of ZERO and reports every word on
## its own line). Waiting for the deferred sort is what gives the sheet its real width and makes
## `get_combined_minimum_size()` re-shape against it.
##
## Only the IN-ZONE render is measured. A floated sheet is measured at the float's own column, which is
## never narrower than the zone's, so trusting it could report a height the zone would not reproduce
## and hand the sheet back into a box that then clips it — the oscillation this narrow rule removes.
## While floating, the latched requirement stands and the fork is re-decided against the live box, so a
## zone that GROWS (a dock change, a taller window) takes its sheet back on the very next render.
##
## **A READING TAKEN BEFORE THE LAYOUT PASS IS NOT RECORDED AT ALL, AND IT IS THE SHEET THAT SAYS SO.**
## The mark is a high-water mark for one composing act (it must be, or the sheet hops back into the
## zone as a field clears — a layout change under the player's hands), so ONE bad reading latches until
## the sheet closes. The two ways to take one are the two guards in `_party_compose_measurable`: a zone
## box the panel cannot state yet, and a sheet with no honest rect.
##
## **ASKING THE ZONE COLUMN INSTEAD IS WHAT LET THIS DEFECT BE REPORTED TWICE.** The column is anchored
## `PRESET_FULL_RECT` into its zone host, so Godot hands it the host's width SYNCHRONOUSLY the instant
## it is reparented; everything under it is sized by the container sort, which is DEFERRED. So the two
## are established by different mechanisms and the column's width says nothing about whether its
## contents have been laid out — measured on the empty hunt form, `col.size.x == 356` (a wholly
## plausible reading) beside `col.get_combined_minimum_size().y == 1278`, where the laid-out answer is
## 207. 1278 floats that sheet out of every dock this client has, and the high-water mark then holds it
## there for the rest of the composition, which is exactly the reported picture: `Quarry: Choose…`, one
## hint, a disabled Send, floating out of a dock with 800px to spare.
##
## **AND IT WAITS RATHER THAN GIVING UP AFTER ONE FRAME.** One `process_frame` is the normal cost, but
## whether the deferred sort has been flushed by the time this coroutine resumes depends on where in
## the frame the render that armed it ran — so the wait is a bounded RETRY
## (`COMPOSE_MEASURE_MAX_FRAMES`) rather than a single look. Waiting another frame is cheap; recording
## a phantom costs the rest of the composing act, and returning unmeasured leaves the mark to whatever
## render happens to arm it next.
func _measure_party_compose() -> void:
    if _party_compose_measuring or _host == null:
        return
    _party_compose_measuring = true
    var measurable := false
    for _frame in range(HudComposeVocab.COMPOSE_MEASURE_MAX_FRAMES):
        await _host.get_tree().process_frame
        if not _party_compose_still_measuring():
            _party_compose_measuring = false
            return
        if _party_compose_measurable():
            measurable = true
            break
    _party_compose_measuring = false
    if not measurable:
        return
    var needed: float = _parties_zone_col.get_combined_minimum_size().y
    if needed <= _party_compose_needed:
        return
    _party_compose_needed = needed
    _party_compose_measured_box = _parties_zone_box_known()
    if _party_compose_floats():
        rerender()

## Is there still an in-zone sheet to measure? Re-asked every frame of the retry above, because a
## composing act can end (or float) while the coroutine is waiting for a layout pass.
func _party_compose_still_measuring() -> bool:
    if not _party_compose_open or compose_is_floating():
        return false
    if _party_compose_sheet == null or not is_instance_valid(_party_compose_sheet) \
            or not _party_compose_sheet.is_inside_tree():
        return false
    return _parties_zone_col != null and is_instance_valid(_parties_zone_col) \
        and _parties_zone_col.is_inside_tree()

## May the deferred measurement be RECORDED this frame? All three terms are about whether a number
## taken now could be honest at all, never about its size: the panel must be able to state the box the
## mark will be compared against, the parties column must have a rect at all, and **THE SHEET MUST HAVE
## BEEN FITTED TO THAT COLUMN**.
##
## That last term is the one that decides it, and it is a RELATION rather than a floor. The column's
## own width is set synchronously by its anchors and says nothing about the deferred container sort
## (see `COMPOSE_MEASURE_MIN_COLUMN_WIDTH`), and a bare floor on the SHEET does not close it either —
## an unsorted `Control` still clamps its size up to its own combined minimum, so the unlaid-out sheet
## reports a plausible 220px against a 356px column, wide enough to pass any floor and narrow enough
## that its labels are still wrapping at the wrong width. Once the sort has run, a `VBoxContainer`
## fits every child to its own width, so `sheet.size.x >= col.size.x` holds exactly — and it is the
## only reading that distinguishes "laid out" from "clamped to its own minimum".
func _party_compose_measurable() -> bool:
    if _parties_zone_box_known() == Vector2.ZERO:
        return false
    if _party_compose_sheet == null or not is_instance_valid(_party_compose_sheet):
        return false
    if _parties_zone_col.size.x < HudComposeVocab.COMPOSE_MEASURE_MIN_COLUMN_WIDTH:
        return false
    return _party_compose_sheet.size.x >= _parties_zone_col.size.x

## Drop the latched requirement when the parties zone's BOX changes — a dock move, a collapse, a window
## resize. The mark answers "what did this sheet demand of THAT column", so carried across a box change
## it is an answer to a question nobody asked: a mark latched in a 265px bottom dock would keep the
## sheet floating in the 1055px left dock it was just moved into. Called from the zone builder, i.e.
## every render, so it cannot be missed by a path that forgot to call it.
func _note_parties_zone_box() -> void:
    var box := _parties_zone_box_known()
    if box == Vector2.ZERO or box == _party_compose_measured_box:
        return
    _party_compose_needed = 0.0
    _party_compose_measured_box = box

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
    # Leaving the faction page is a subject change like any other, so the composing act it interrupted
    # is closed on the way back the same way a band-to-band cycle closes one. The page itself composes
    # nothing, but the player may have opened a sheet, cycled away to read the rollup and cycled back.
    if _panel_is_faction:
        _panel_is_faction = false
        _clear_party_quarry()
        _party_compose_open = false
        _party_compose_mission = ""
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
    # A quarry is chosen FOR a band (its travel time and useful party size are band-relative), so the
    # cycler swapping the panel subject must not carry one across — and neither may the rest of the
    # composing act: the party size, the mission and the MEASURED requirement that floated the sheet
    # all belong to the band that was being composed for. Closed inline rather than through
    # `_close_party_compose`, which re-renders, and this IS the render.
    if int(unit.get("entity", -1)) != int(_band_labor.panel_band().get("entity", -1)):
        _clear_party_quarry()
        _party_compose_open = false
        _party_compose_mission = ""
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
    # DEEP-COPY the subject: the panel band must NOT alias the selection's unit dict (the
    # selection path passes it in). The panel persists across selection changes, so it needs its
    # own stable copy — a later selection swap (or an in-place edit of the selection's unit dict)
    # must not mutate or blank it. The zone closures below also capture this stable copy, so they
    # keep targeting the panel band regardless of the current selection.
    _band_labor.set_panel_band(unit.duplicate(true))
    # **THE LAYOUT IS DECLARED BEFORE THE ZONES ARE BUILT**, and the order is load-bearing: the shell
    # threshold is a sum over the declared zones, so arriving from the four-zone faction page can flip
    # the shell — and every builder below pages against `zone_size()`, which the flip moves.
    _panel.set_zone_layout(BAND_ZONE_LAYOUT)
    # No tint-context reset here either: `_build_vitals_label` (inside the band zone below) builds its
    # own `DetailFormat.Context` per render, so the context cannot survive from the previous one.
    # The zone contents. Ownership passes to the panel, which frees the previous render's zones
    # and parents these into whichever shell (wide columns / narrow tabs) its width selected.
    _panel.set_zones({
        BandCityPanel.ZONE_BAND: HudWidgets.wrap_zone(build_band_zone(_band_labor.panel_band())),
        BandCityPanel.ZONE_WORK: HudWidgets.wrap_zone(build_work_zone(_band_labor.panel_band())),
        BandCityPanel.ZONE_PARTIES: HudWidgets.wrap_zone(build_parties_zone(_band_labor.panel_band())),
    })
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
    _panel.set_cycler(_cycler_index_of_band(index), _cycler_count())
    # A band HAS a tile, and its `band` zone is a band's, so both header affordances come back on. Both
    # setters early-out on an unchanged value, so a band-to-band cycle costs nothing.
    _panel.set_subject_jumpable(true)
    # `set_zones` above already flipped the panel to band-present; just make sure it is shown.
    _panel.set_shown(true)
    # THE TRIGGER'S MEASUREMENT, taken a frame from now against the tree this render just handed over
    # — see `_party_compose_needed`. Armed unconditionally: it costs one awaited frame and answers
    # immediately when no sheet is open.
    _measure_party_compose()

## Render the FACTION PAGE — the all-band rollup pinned as the cycler's first entry (issue #450).
##
## It fills the SAME three zones a band does, one scale up: `band` is who the faction is and what it
## holds, `work` is the whole workforce plus where those hands are and what the faction knows, and
## `parties` is everyone who is out. The arithmetic is `FactionRollup`'s — an all-`static` layer, this
## page carrying no state of its own — and every total it prints is a SUM over the per-band answers,
## so a band's page and this one cannot disagree about a number.
##
## **`_band_labor.panel_band()` IS DELIBERATELY LEFT ALONE.** It is what the cycler walks back into, so
## cycling faction → next returns to the band the player was reading rather than to the roster's first,
## and `_resolve_panel_band` still has a subject to re-resolve when the page is left.
func render_faction() -> void:
    if _panel == null or _band_labor.player_bands().is_empty():
        return
    _panel_is_faction = true
    # A composing act belongs to the BAND it was opened on, so leaving that band for this page ends it
    # — the identical rule `render_band` applies to a band-to-band cycle, and the float must come down
    # with it (it lives outside the panel and no zone rebuild reaches it).
    _clear_party_quarry()
    _party_compose_open = false
    _party_compose_mission = ""
    _party_compose_needed = 0.0
    _party_compose_measured_box = Vector2.ZERO
    _party_compose_sheet = null
    _dismiss_compose_float()
    # This page builds no work BOARD, so the re-page path must have nothing to re-page: `_on_zones_resized`
    # would otherwise rebuild the previous band's board into a host `set_zones` is about to free.
    _work_zone_host = null
    _work_zone_band = {}
    _parties_zone_col = null
    # The faction's alerts, read from the ONE model the turn orb reads — so the orb, the map and this
    # page can never disagree about which band needs the player.
    var attention := _attention.build_band_attention(
        _band_labor.player_bands(), _band_labor.player_expeditions())
    # FOUR zones here against a band's three, declared BEFORE the builders run — see `render_band`.
    _panel.set_zone_layout(FACTION_ZONE_LAYOUT)
    _panel.set_zones({
        BandCityPanel.ZONE_BAND:
            HudWidgets.wrap_zone(FactionRollup.build_band_zone(_band_labor, _disclosures)),
        BandCityPanel.ZONE_WORK:
            HudWidgets.wrap_zone(FactionRollup.build_work_zone(_band_labor,
                attention, _faction_open_row, _toggle_faction_row, jump_to_band_entity)),
        BandCityPanel.ZONE_KNOWLEDGE:
            HudWidgets.wrap_zone(FactionRollup.build_knowledge_zone(_player_knowledge(),
                _faction_settling(), _faction_discoveries())),
        BandCityPanel.ZONE_PARTIES:
            HudWidgets.wrap_zone(FactionRollup.build_parties_zone(_band_labor, _herd_label_for_id,
                attention, _faction_open_row, _toggle_faction_row, _jump_to_party_entity)),
    })
    _push_faction_zone_badges()
    # No stage id ⇒ no bundled art resolves and the emoji stands; the band count takes the stage word's
    # slot, and the empty position label hides the coordinate slot outright.
    _panel.set_header("", HudFormat.FACTION_PAGE_GLYPH, HudFormat.FACTION_PAGE_NAME,
        HudFormat.faction_bands_label(_band_labor.player_bands().size()), "")
    _panel.set_cycler(FACTION_CYCLER_INDEX, _cycler_count())
    # A faction has no tile to jump to. (The narrow shell's first tab reads `Faction` rather than
    # `Band` because `FACTION_ZONE_LAYOUT` says so — a subject names its own zone labels.)
    _panel.set_subject_jumpable(false)
    _panel.set_shown(true)

## The tab badges for the faction page: the totals its three zones answer, so the narrow shell states
## them without the player having to open each tab. `work` counts BANDS rather than sources — this
## zone's list is the roster, not a board — and `parties` keeps the band page's `hot` rule, an awaiting
## party being a demand on the player wherever it is standing.
func _push_faction_zone_badges() -> void:
    if _panel == null:
        return
    var population := 0
    for band_variant in _band_labor.player_bands():
        if band_variant is Dictionary:
            population += int((band_variant as Dictionary).get("size", 0))
    _panel.set_tab_badge(BandCityPanel.ZONE_BAND, str(population) if population > 0 else "", false)
    _panel.set_tab_badge(BandCityPanel.ZONE_WORK, str(_band_labor.player_bands().size()), false)
    var parties := _band_labor.player_expeditions()
    var awaiting := false
    for party in parties:
        if HudFormat.expedition_phase_key(party) == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

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
    if _panel == null:
        return
    # The faction page is a SUBJECT, not a band, so every re-render path has to ask which one is up:
    # falling through to `render_band` here would drop the player back onto a band on a caret click, a
    # zone resize or any other in-place refresh.
    if _panel_is_faction:
        render_faction()
        return
    if _band_labor.panel_band().is_empty():
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
        # A faction with no band has no rollup either — the page is pinned to the cycler, and the
        # cycler is gone with the panel. Cleared here so a later band does not bring the page back as
        # the panel's subject without the player having asked for it.
        _panel_is_faction = false
        _panel.set_band_present(false)
        _panel.set_shown(false)
        # No band ⇒ no zones are rebuilt, so the footer builder's teardown never runs. The float is
        # the one piece of this panel that lives OUTSIDE it, and it must go down with the panel.
        _party_compose_open = false
        _party_compose_mission = ""
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
        _party_compose_sheet = null
        _dismiss_compose_float()
        return
    # The page SURVIVES a snapshot, exactly as a band subject does — its totals are what the tick just
    # moved, so a tick is precisely when it must re-render rather than hand the panel back to a band.
    if _panel_is_faction:
        render_faction()
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
    # A faction drill-down row is a link to a BAND, and making that band the panel's subject is this
    # controller's job — the disclosure controller must not know the band panel exists.
    if _disclosures != null:
        _disclosures.set_faction_band_jump(jump_to_band_entity)

## Is the panel showing the FACTION page? Asked by the drawer, which otherwise re-asserts the selected
## band as the panel's subject on every render and would steal the page out from under a caret click.
func is_faction_page() -> bool:
    return _panel_is_faction

## Expand or collapse a faction summary row. ONE row open at a time — the zones clip, and two open
## details would push the second list off the bottom of a horizontal dock's box.
func _toggle_faction_row(owner: int) -> void:
    _faction_open_row = FACTION_ROW_NONE if _faction_open_row == owner else owner
    rerender()

## Jump to a PARTY from the faction page's Parties row — the same routing the band page's own parties
## rows use (`select_expedition`), so a party is reached identically from both.
func _jump_to_party_entity(entity: int) -> void:
    for party_variant in _band_labor.player_expeditions():
        var party: Dictionary = party_variant
        if int(party.get("entity", -1)) == entity:
            select_expedition(entity, int(party.get("current_x", -1)), int(party.get("current_y", -1)))
            return

## Make a band the panel's subject, by entity — the faction page's drill-down rows route here, so a
## popover row reaches a band the same way the cycler does (recenter, pin, render), rather than by a
## second path that could drift from it. Unknown entity ⇒ no-op.
func jump_to_band_entity(entity: int) -> void:
    var band := _band_labor.player_band_by_entity(entity)
    if band.is_empty():
        return
    _select_band_on_map(band)

## Walk to the next/prev subject (cycler ◀/▶) over `[the faction page] + player_bands()`.
##
## A band routes through the SAME band-selection a roster click uses — recenter + select the band's hex
## (rebuilding that hex's roster), then pin the exact band — so the map ring, Tile card, roster and this
## panel all land on the cycled band.
##
## **THE FACTION PAGE MOVES NO CAMERA, and that is a documented exception to decision 2 of
## `docs/plan_band_city_dock.md`** ("panel cycling recenters the map on the cycled settlement"). It has
## no tile: there is nothing to centre on, and recentring on the band the player happened to leave
## would move the map for a page that says nothing about where it is.
func cycle_band(delta: int) -> void:
    if _panel == null:
        return
    var n := _cycler_count()
    # One band ⇒ two entries, so the cycler is live where it used to be dead: the faction page is
    # reachable from the first band a faction ever has.
    if n <= 1:
        return
    var next := ((_cycler_index() + delta) % n + n) % n
    if next == FACTION_CYCLER_INDEX:
        render_faction()
        return
    _select_band_on_map(_band_labor.player_bands()[next - FACTION_CYCLER_ENTRIES])

## How many entries the cycler walks: every player band plus the pinned faction page.
func _cycler_count() -> int:
    return _band_labor.player_bands().size() + FACTION_CYCLER_ENTRIES

## Where the panel's current subject sits in that walk.
func _cycler_index() -> int:
    if _panel_is_faction:
        return FACTION_CYCLER_INDEX
    return _cycler_index_of_band(_index_of_player_band(int(_band_labor.panel_band().get("entity", -1))))

## A band's roster index as a CYCLER index — the pinned page's entries shift every band along by one.
## A band absent from the roster (mid-swap, or a marker click on one the snapshot has since dropped)
## resolves to the first, which is `_resolve_panel_band`'s own fallback.
func _cycler_index_of_band(roster_index: int) -> int:
    return FACTION_CYCLER_ENTRIES + (roster_index if roster_index >= 0 else 0)

## Jump to the panel band on the map (the header title is a "jump to my band" affordance): recenter
## + select its hex and move the ring, WITHOUT changing which band the panel shows (it's already
## `_band_labor.panel_band()`). No-op when there is no panel band.
##
## Silent on the faction page. The panel already refuses the click there (`set_subject_jumpable`), so
## this is the second half of one rule rather than the only guard — but `focus_panel_band` is reached
## BY NAME through `Main`'s `has_method` probe, so the verb must be safe to call in every state.
func focus_band() -> void:
    if _panel_is_faction:
        return
    _select_band_on_map(_band_labor.panel_band())

## Select a band's hex on the map — recenter + select the hex (rebuilding its roster) via
## `alert_focus_requested` (→ MapView.focus_and_select_tile) then pin the exact band so the map ring,
## Tile card, roster, and panel all agree. Shared by the cycler and the header "jump to band". A band
## with no live roster entry (no tile_info) is rendered directly into the panel instead.
func _select_band_on_map(band: Dictionary) -> void:
    if band.is_empty():
        return
    # **LEAVING THE FACTION PAGE IS THE FIRST THING THIS DOES, and the order matters.** This is the
    # explicit "make this band the subject" act — the cycler's ▶, a drill-down link, the header jump.
    # Its usual route is `roster_occupant_selected` → the drawer → the drawer's band branch, and that
    # branch is now GATED on the panel not being on the faction page (a passive re-render must not
    # steal the page). Clearing the flag here is what tells the gate this render is the wanted one;
    # without it the cycler walked off the page and the panel silently stayed on it.
    _panel_is_faction = false
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
