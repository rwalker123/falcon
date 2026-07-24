extends CanvasLayer
class_name HudLayer

## Map-zoom rail (bottom-left nav cluster). `map_zoom_step` carries +1 (in) / -1 (out);
## `map_zoom_fit` fits the map to the view. Main wires both to the single MapView zoom path.
signal map_zoom_step(direction: int)
signal map_zoom_fit
## Emitted when the player clears ALL of a band's labor assignments (the "Clear all"
## affordance); carries the band dict so Main can extract faction + entity bits for the
## repurposed `cancel_order` command (now a clear-all → fully idle).
signal cancel_order_requested(band: Dictionary, scope: String)
## Early-Game Labor (docs/plan_early_game_labor.md, slice 3b): assign/unassign
## working-age workers to a source or band-wide role. Payload keys:
## { faction, band, kind ("forage"|"hunt"|"scout"|"warrior"), workers,
##   x, y (forage/hunt readout), herd_id, policy (hunt) }. Main formats the
## `assign_labor …` text command. workers==0 removes/zeroes the assignment.
signal assign_labor_requested(payload: Dictionary)
## The Telling (docs/plan_the_telling.md): the player answered a pending narrative fork.
## Payload keys: { faction, beat_id, choice_id }. Main formats the `answer_fork …` command.
signal answer_fork_requested(payload: Dictionary)
## Emitted after the player picks a destination tile for the selected band's move.
## Payload keys: { faction, band, x, y }. Main formats the `move_band …` command.
signal move_band_requested(payload: Dictionary)
## Scouting expedition (docs/plan_exploration_and_sites.md §2). Sent after the player outfits a
## party on a resident band (a party-size stepper) and clicks a target tile. Payload keys:
## { faction, band, party_workers, x, y }. Main formats the `send_expedition …` command.
signal send_expedition_requested(payload: Dictionary)
## Hunting expedition (docs/plan_exploration_and_sites.md §2b). Sent after the player outfits a party
## on a resident band and clicks a target herd. Payload keys: { faction, band, party_workers,
## fauna_id, fauna_label }. `fauna_id` is the DATABASE KEY the command line addresses the herd with;
## `fauna_label` is its player-facing species name (via `SourceForecast.herd_display_name`), which is what the
## command-feed note must read — a feed line naming `game_deer_07` is a key leaking into the game UI.
## Main formats the `send_hunt_expedition …` command.
signal send_hunt_expedition_requested(payload: Dictionary)
## Emitted when the player recalls the selected in-flight expedition (folds it home). Payload
## keys: { faction, expedition }. Main formats the `recall_expedition …` command.
signal recall_expedition_requested(payload: Dictionary)
## Emitted when the player extends a built pen by one fenced ring (Grazing 2d-γ). Payload keys:
## { faction, x, y } — the pen's anchor tile. Main formats the `extend_pen <faction> <x> <y>` command.
signal extend_pen_requested(payload: Dictionary)
## Optimistic pending-labor state changed (Early-Game Labor slice 3b UX): carries the
## per-band pending map so MapView can draw the pending-action hex highlights. Main forwards
## it to `MapView.set_labor_pending`.
signal labor_pending_changed(pending: Dictionary)
signal next_turn_requested(steps: int)
## Emitted whenever the active command-targeting state changes. Carries a dict
## ({} when inactive) that Main forwards to MapView so the map can draw the
## reticle / valid-target glow / hover ETA.
signal targeting_changed(info: Dictionary)
## Emitted when the player clicks a band alert; Main forwards it to
## MapView.focus_on_tile so the map pans to the band that raised the alert.
signal alert_focus_requested(x: int, y: int)
## Emitted when a roster row (band or wildlife) is selected in the Occupants card.
## `kind` is "unit" (id = entity_id int) or "herd" (id = herd_id String). Main
## forwards it to MapView.select_occupant so the map selection ring follows the
## chosen occupant without a hex click.
signal roster_occupant_selected(kind: String, id: Variant)

## PURE FALLBACK build identifier of THIS client — used only when no git stamp is present.
## The real build id is the git stamp `scripts/run_stack.sh` writes to `res://build_stamp.txt`
## (`<commit-date>-<short-hash>[-dirty]`, mirroring the server's `CORE_SIM_BUILD_ID`), read via
## `ClientBuild.current()`. **No more hand-bumping** — the git stamp is the source of truth, and
## this const matches the server's own `dev-unknown` fallback. Shown in the bottom-centre overlay
## beside the server build so the running client+server builds can be confirmed at a glance.
const CLIENT_BUILD := "dev-unknown"
const ClientBuild := preload("res://src/scripts/ClientBuild.gd")
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var sedentarization_label: Label = %SedentarizationLabel
@onready var demographics_label: Label = %DemographicsLabel
@onready var discoveries_row: HBoxContainer = %DiscoveriesRow
@onready var discoveries_label: Label = %DiscoveriesLabel
@onready var discoveries_strip: HBoxContainer = %DiscoveriesStrip
@onready var intensification_label: Label = %IntensificationLabel
@onready var nav_backing: PanelContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking
@onready var zoom_rail: VBoxContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail
@onready var zoom_in_button2: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomInButton
@onready var zoom_out_button2: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomOutButton
@onready var zoom_fit_button: Button = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomFitButton
@onready var zoom_level_label: Label = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/ZoomRail/ZoomLevelLabel
@onready var terrain_legend_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel as PanelCard
@onready var terrain_legend_scroll: ScrollContainer = %LegendScroll
@onready var terrain_legend_list: VBoxContainer = %LegendList
@onready var terrain_legend_description: Label = %LegendDescription
@onready var victory_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel
@onready var victory_status_label: RichTextLabel = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel/Margin/VictoryLabel
@onready var command_feed_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel as PanelCard
@onready var command_feed_scroll: ScrollContainer = %CommandFeedScroll
@onready var command_feed_label: RichTextLabel = %CommandFeedLabel
@onready var telling_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TellingPanel as PanelCard
@onready var telling_scroll: ScrollContainer = %TellingScroll
@onready var telling_label: RichTextLabel = %TellingLabel
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var tile_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/TilePanel as PanelCard
@onready var tile_detail: RichTextLabel = %TileDetail
@onready var occupant_detail: RichTextLabel = %OccupantDetail
# ONE card, ONE list, ONE drawer (docs/plan_tile_panel_layout.md). The chip strip carries the
# tile's standing condition and never scrolls; `%SubjectList` is the selectable list of subjects on
# this hex — the LAND first, then the bands and wildlife — and `%SubjectScroll` is the single,
# height-capped drawer every one of them fills. Only one drawer is ever open, which is what bounds
# the card: rows are ~30px, a compose block is 300+.
@onready var tile_chips: HFlowContainer = %TileChips
@onready var subject_list: VBoxContainer = %SubjectList
@onready var subject_scroll: ScrollContainer = %SubjectScroll
@onready var subject_body: VBoxContainer = %SubjectBody
# The 1px rule marking where the LIST ends and the DRAWER begins — without it the drawer's first
# row runs straight on from the last wildlife row and the two blocks read as one list.
@onready var subject_divider: Panel = %SubjectDivider
# Early-Game Labor allocation UI (slice 3b), all runtime-populated containers:
# the band's allocation panel (Working/Idle + assignment rows + Scout/Warrior + Move/Clear),
# the herd "assign hunters" controls, and the tile "assign foragers" controls.
@onready var allocation_panel: VBoxContainer = %AllocationPanel
@onready var herd_assign_controls: VBoxContainer = %HerdAssignControls
@onready var forage_assign_controls: VBoxContainer = %ForageAssignControls
@onready var stockpile_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel
@onready var stockpile_title: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileTitle
@onready var stockpile_list: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileList
@onready var left_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack
@onready var right_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack
@onready var right_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll
@onready var turn_orb: TurnOrb = $LayoutRoot/RootColumn/BottomBar/TurnCluster
@onready var minimap_container: MarginContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/MinimapContainer

var tooltip_panel: PanelContainer
var tooltip_label: Label

# The legend card + its terrain-only Name/Count sort header now live in
# ui/hud/LegendController.gd; the command feed card in ui/hud/CommandFeedController.gd.
# These two aliases keep `HudLayer.LEGEND_SORT_FIELD_*` resolvable for external
# callers (e.g. tools/ui_preview.gd) with the controller as the single source of truth.
const LEGEND_SORT_FIELD_NAME := LegendController.SORT_FIELD_NAME
const LEGEND_SORT_FIELD_COUNT := LegendController.SORT_FIELD_COUNT
const STACK_ADDITIONAL_MARGIN := 16.0
# ──────────────────────────────────────────────────────────────────────────────────────────────────

# The band's FODDER larder (Flora roster F3): hay stockpiled to feed penned animals — a SECOND stock
# distinct from the food larder above, in fodder/grass units (the raw `FODDER` `LocalStore` value,
# `fodder_per_biomass × biomass` scale, ~25× the food scale — NOT comparable to and never summed onto
# the food larder; only `pen_hay_food` is the food-equivalent conversion). Shown as its own stat line
# beneath Food, but ONLY for a band with a fodder economy (`fodder_store > 0`, or it pays a pen bread
# bill — `pen_feed_upkeep > 0`), so a forager band with no animals never sprouts an empty Fodder line.
# (The larder-runway vocabulary — `DetailFormat.FOOD_UNLIMITED_GLYPH` / `DetailFormat.FOOD_RUNWAY_UNIT`
# — travelled to that module with BOTH its readers: the one renderer (`food_turns_text`) and the one
# Food/Provisions/Carried threshold tint that recognizes the row by looking for that same unit word.
# The tint went dead once already because the two drifted; they are now typed in one file.)
const UI_BALANCE_CONFIG_PATH := "res://src/config/ui_balance.json"
# Dock-card visibility preferences. Reuses the file `NarrativeForkPanel` already writes the voice
# register into — one prefs file, its own section; the path/section constants are borrowed.
const HUD_PANELS_CONFIG_SECTION := "hud_panels"
const CONFIG_KEY_LEGEND_SUPPRESSED := "legend_suppressed"
const CONFIG_KEY_VICTORY_SUPPRESSED := "victory_suppressed"
const CONFIG_KEY_COMMAND_FEED_SUPPRESSED := "command_feed_suppressed"
# Both reference cards start HIDDEN: the right dock is the narrative surface's home, and Victory /
# Terrain Types are look-it-up readouts the player opens on demand (V / L) rather than standing
# furniture competing with the telling for dock height.
const PANEL_SUPPRESSED_BY_DEFAULT := true
const DEFAULT_TRAVEL_SPEED := 3.0
const DEFAULT_TRAVEL_PREVIEW_LIMIT := 12
# The legend card (rows + sort header + suppress state) is owned by _legend; the
# command feed card by _command_feed; the narrative panel by _telling. Hud delegates to all three.
var _legend: LegendController = null
var _command_feed: CommandFeedController = null
var _topbar: TopBarReadouts = null
var _telling: TellingPanel = null
# Victory's counterpart to the legend's `legend_suppressed` — the player-hidden state of a dock
# card, distinct from "no victory data to show".
var _victory_suppressed: bool = PANEL_SUPPRESSED_BY_DEFAULT
var localization_store = null
var campaign_label: Dictionary = {}
var victory_state: Dictionary = {}
# "What the player is looking at" — the selection triplet, lit-row kind, roster, sticky-selection
# guard. Every former `_selected_*` / `_roster_*` / `_selection.choice_tile()` member lives here now.
var _selection: HudSelectionState = null
# "The digested per-snapshot player world + the optimistic pending overlay" — player bands /
# expeditions, world herds, the forage-patch / food-module lookups, grid scalars, the
# losing-population diff, the snapshot turn, and pending labor. Former `_player_*` / `_band_labor.panel_band()` /
# `_band_labor.world_herds()` / `_band_labor.pending_labor()` / `_band_labor.current_turn()` / `_grid_*` / `_band_labor.prev_band_sizes()` /
# `_band_labor.forage_patch_lookup()` / `_band_labor.food_module_by_tile()` members live here now.
var _band_labor: HudBandLaborState = null
# The IDENTITY/LIST half of the selection card (HUD decomposition Phase 2b) — the tile-card header,
# the condition-chip strip, the roster/subject list, the row clicks + the fresh-hex auto-select. It
# is handed the SAME `_selection`/`_band_labor` instances; HudLayer relays its `roster_occupant_selected`
# and re-renders on its `subject_changed`. The DRAWER + compose stay on HudLayer (Phase 2c).
var _selectioncard: SelectionCardController = null
# "What the player is dialing but has not committed" (HUD decomposition Phase 2c-1) — the forage /
# hunt drawer compose blocks, the parties-zone party compose, and the open sheet's subject identity.
# Every former `_forage_assign_*` / `_hunt_assign_*` / `_send_party_*` / `_compose_kind` /
# `_compose_subject` member lives here now. The `ComposeSheet` NODE lives on `DrawerComposeController`
# beside the lifecycle that opens it — a model holds pure data, never a scene handle. This state is
# shared: BOTH HudLayer (the parties zone) and that controller (the drawer) hold the same instance.
var _compose: ComposeState = null
# ---- Selection-card in-place update caches (docs/plan_hud_decomposition.md §2a) --------------
# The selection card re-renders on EVERY snapshot; to avoid a one-frame teardown/reflow flash each
# controller caches the last-rendered STRUCTURE of its widget, so an unchanged restate PATCHES the
# existing nodes in place instead of freeing + rebuilding them (rebuild only on a structural change).
# The chip-slot / roster-row caches (`_tile_chip_slots` / `_subject_row_keys`) moved WITH the
# identity/list code into `SelectionCardController` (Phase 2b), the drawer-ACTIONS shape signatures
# (`_forage_drawer_shape` / `_herd_drawer_shape`) moved WITH the drawer-action builders into
# `DrawerComposeController` (Phase 2c-2b), and the drawer's RENDER diff state (`_tile_detail_lines_cache`
# + the fit-flight/last-height guards) moved WITH the render dispatch into `SubjectDrawerController`
# (Phase 2c-3).
# The Food/Morale disclosure cluster (carets + the shared breakdown popover). Owns `_disclosure_state`
# / the stashed payloads / the `PopupPanel`; `state()` feeds the per-render `DetailFormat.Context`.
# The three per-render tint scalars it used to sit beside (`_selected_band_food_turns` / `_morale` /
# `_output`) are GONE from this file: they were pure out-parameters of one render, so they became
# fields on that context, constructed locally by whichever host is about to render.
var _disclosures: DisclosureController = null
# The band/party detail-line producers (Food / Morale / Output / stockpile rows + the party rows).
# Shared BY REFERENCE with `BandPanelController`, which renders the same rows into the dock.
var _banddetail: BandDetailLines = null
# Generic section header for the outfit block (hosts both the scout + hunt send verbs).

















# The player-faction split (single player band, all player bands, expeditions) captured each
# snapshot lives on `_band_labor` — see `player_band()` / `player_bands()` / `player_expeditions()`.

# The authoritative snapshot turn, the grid scalars, and the optimistic pending-labor overlay all
# live on `_band_labor` (`current_turn()` / `grid_width()` / `grid_height()` / `pending_labor()`).
# The forage / hunt / party compose state (the dialed worker counts, policies, crop, actor bands, the
# party's quarry and the two autofill one-shots) lives on `_compose` — see `ComposeState`.
# The COMMAND-TARGETING cluster (HUD decomposition): the three pending flows (move-band /
# send-expedition / pick-quarry), the floating banner, and the dispatch. It emits its OWN signals;
# HudLayer relays each. HudLayer keeps thin reflective delegators (`is_targeting_active` /
# `cancel_active_targeting` / `try_dispatch`). Constructed AFTER `_drawercompose` + BEFORE `_bandpanel`.
var _targeting: TargetingController = null
var travel_tiles_per_turn: float = DEFAULT_TRAVEL_SPEED
var travel_preview_turn_cap: int = DEFAULT_TRAVEL_PREVIEW_LIMIT
var left_dock: PanelDock
var right_dock: PanelDock
# Edges reserved by docked panels (Inspector, Band/City panel). Each reserver
# registers a (edge, size) contribution keyed by a StringName id; the whole HUD
# insets by the summed per-edge totals.
var _reservations: Dictionary = {}
# ---- The Telling (docs/plan_the_telling.md) --------------------------------
# The turn-orb / attention / fork cluster (HUD decomposition Phase 1b, docs/plan_hud_decomposition.md).
# The pending forks, stance axes, the cached `_band_attention` band half, the auto-opened set, and the
# fork panel all live in the controller now; `update_band_alerts` feeds its band half via
# `set_band_attention`, and the five reflective methods are thin delegators below.
var _turnorb: TurnOrbController = null
# The drawer's COMPOSE half (HUD decomposition Phase 2c-2b): the compose-sheet lifecycle and node, the
# drawer-action builders, the two compose builders and the compose-only forecast/gate/picker layer.
# HudLayer keeps the drawer RENDER DISPATCH and calls in; the two methods Main reaches by name
# (`is_compose_sheet_open` / `close_compose_sheet`) stay here as thin delegators below.
var _drawercompose: DrawerComposeController = null
# The BAND/CITY PANEL (HUD decomposition Phase 2d): the panel handle itself, the three zone builders
# (`band` / `work` / `parties`) and everything under them, the zone state that survives a snapshot
# (filter / sort / page / open strips / party compose), the cycler + snapshot refresh, and the
# map-focus routing the panel's own rows use. HudLayer keeps the drawer dispatch and the legacy flat
# `%AllocationPanel` host that call in; the three methods Main reaches by name
# (`set_band_city_panel` / `cycle_panel_band` / `focus_panel_band`) stay here as thin delegators below.
var _bandpanel: BandPanelController = null
# The selection drawer's RENDER DISPATCH (HUD decomposition Phase 2c-3): the one-drawer land/occupant
# dispatch, the land-drawer terrain-line producer, the `%AllocationPanel` occupant/expedition/band-move
# branches, and the height-capping fit path. HudLayer keeps the reflectively-reached `_render_selection_panel`
# and the two-host `_refresh_disclosure_hosts` calling in, and `_targeting` (its Move button connects to
# `begin_move_band`). Constructed AFTER `_bandpanel` — it dispatches into it and `_drawercompose`.
var _drawer: SubjectDrawerController = null
# The BAND/EXPEDITION ATTENTION PRODUCERS + orb jump-routing (HUD decomposition). Owns the OTHER half
# of the turn-orb attention model from `TurnOrbController`: it PRODUCES the band/expedition rows
# (`build_band_attention`, fed to `_turnorb.set_band_attention` from `update_band_alerts`) and ROUTES
# their "Jump →" (`on_turn_orb_focus`). Constructed AFTER `_bandpanel` (it holds it for the pen/awaiting
# jumps); it emits its own `alert_focus_requested`, which HudLayer relays.
var _attention: AttentionController = null
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0

func _ready() -> void:
    _selection = HudSelectionState.new()
    _band_labor = HudBandLaborState.new()
    # Both compose policies start on the default rung; the policy vocabulary stays here, not in the model.
    _compose = ComposeState.new(SourceForecast.DEFAULT_HUNT_POLICY)
    _legend = LegendController.new(terrain_legend_panel, terrain_legend_scroll, terrain_legend_list, terrain_legend_description)
    _command_feed = CommandFeedController.new(command_feed_panel, command_feed_scroll, command_feed_label, left_dock_scroll)
    # Top-bar faction readouts — constructed AFTER _command_feed so it can route the
    # knowledge-unlock nudge straight through it. The ONE shared-beyond-cluster helper that is still a
    # HudLayer METHOD (_meter_bar) stays here and is passed as a Callable; the percent formatter and
    # the stockpile item wording are `HudFormat.progress_percent` / `HudFormat.stockpile_label` now,
    # which the cluster calls directly.
    _topbar = TopBarReadouts.new(
        turn_label, metrics_label, sedentarization_label, demographics_label,
        discoveries_row, discoveries_label, discoveries_strip, intensification_label,
        stockpile_panel, stockpile_list, _command_feed, _meter_bar)
    # The telling GROWS TO FIT its current page, capped at `PAGE_MAX_HEIGHT` (docs/plan_the_telling_book_ux.md),
    # so it no longer needs a dock-scroll ceiling to fit against — a page is bounded (one turn's beats), and
    # the right dock's own scroll stacks it above Victory + Terrain Types with no bespoke height math.
    _telling = TellingPanel.new(telling_panel, telling_scroll, telling_label)
    # Turn orb / attention / fork — constructed AFTER _telling and _command_feed (it needs both), handed
    # the HUD CanvasLayer as the host it parents the fork panel into. It emits its OWN signals; HudLayer
    # relays each onto the signals Main connects to (the controller never emits a HudLayer signal).
    _turnorb = TurnOrbController.new(turn_orb, self, _telling, _command_feed)
    _turnorb.answer_fork_requested.connect(func(payload: Dictionary) -> void: answer_fork_requested.emit(payload))
    _turnorb.advance_requested.connect(func() -> void: next_turn_requested.emit(1))
    # `_turnorb.focus_requested` is wired to `_attention.on_turn_orb_focus` further down, once `_attention`
    # exists (it needs `_bandpanel` for the expedition/pen jumps). The orb never emits during construction,
    # so deferring the connect is safe.
    # The selection card's identity/list half. Handed the three card nodes + the SAME selection/labor
    # models (it reads the labor readers straight off `_band_labor` now). A row/land click emits
    # `subject_changed` (HudLayer closes the compose sheet + re-renders), and `roster_occupant_selected`
    # relays to Main.
    _selectioncard = SelectionCardController.new(
        tile_panel, tile_chips, subject_list, _selection, _band_labor)
    _selectioncard.subject_changed.connect(_on_selection_subject_changed)
    _selectioncard.roster_occupant_selected.connect(func(kind: String, id: Variant) -> void: roster_occupant_selected.emit(kind, id))
    # The drawer's compose half. Handed the SAME state models, the two drawer-action containers it
    # fills, the selection card it anchors the sheet beside, the HUD CanvasLayer as the host it
    # parents that sheet into, and the three HudLayer helpers that keep callers on this side.
    _drawercompose = DrawerComposeController.new(
        _compose, _band_labor, _selection, _topbar, _selectioncard, self,
        herd_assign_controls, forage_assign_controls, tile_panel,
        _resolve_assign_band, _herd_label_for_id, _emit_assign_labor)
    _drawercompose.send_hunt_expedition_requested.connect(
        func(payload: Dictionary) -> void: send_hunt_expedition_requested.emit(payload))
    _drawercompose.extend_pen_requested.connect(
        func(payload: Dictionary) -> void: extend_pen_requested.emit(payload))
    # The command-targeting cluster. Constructed AFTER `_drawercompose` (its three close-sheet nudges)
    # and BEFORE `_bandpanel` (which injects `_targeting` — so `_targeting` must exist first). The pick
    # flow's `_bandpanel.rerender()` is therefore a lazily-bound lambda: `_bandpanel` is null now but
    # populated by the time a quarry is picked. It emits its OWN signals; HudLayer relays each (the
    # controller never emits a HudLayer signal). Handed the HUD CanvasLayer as the host it parents the
    # banner into (a RefCounted can't).
    _targeting = TargetingController.new(
        _band_labor, _compose, _drawercompose, _command_feed, self,
        _resolve_assign_band, _after_pending_change, func() -> void: _bandpanel.rerender())
    _targeting.targeting_changed.connect(func(info: Dictionary) -> void: targeting_changed.emit(info))
    _targeting.move_band_requested.connect(func(payload: Dictionary) -> void: move_band_requested.emit(payload))
    _targeting.send_expedition_requested.connect(
        func(payload: Dictionary) -> void: send_expedition_requested.emit(payload))
    # The detail-row disclosure cluster (the Food/Morale carets + the breakdown popover they open).
    # It owns that cluster's ONLY `add_child`, so it is handed the HUD CanvasLayer as the host it
    # parents the popover into (the `TurnOrbController` pattern), plus `_refresh_disclosure_hosts` —
    # the single inbound re-render edge, which is the one thing about the hosts HudLayer still knows.
    _disclosures = DisclosureController.new()
    _disclosures.setup(self, _refresh_disclosure_hosts)
    # The band/party DETAIL-LINE producers — the stateful half of the detail-line family (the pure
    # half is `DetailFormat`'s statics). Constructed AFTER `_disclosures`, which it registers the
    # Food/Morale rows through, and handed the labor model plus the one genuine injection,
    # `_herd_label_for_id` (it reads three collaborators here, so it cannot fold onto the labor model).
    # BOTH detail hosts render through this one instance: the Occupants-card drawer below, and
    # `BandPanelController`'s vitals label + parties inspector strip.
    _banddetail = BandDetailLines.new(_band_labor, _disclosures, _herd_label_for_id)
    # The Band/City panel. Constructed AFTER `_disclosures` (the vitals row wires its carets through
    # it) and `_banddetail` (it renders its rows), and handed the SAME state models, the selection card
    # it routes map focus through, the HUD CanvasLayer as the host it parents its confirm dialog into,
    # and the six HudLayer helpers that keep callers on this side. It emits its OWN five signals; each
    # relays onto the HudLayer signal Main connects to.
    _bandpanel = BandPanelController.new(
        _band_labor, _compose, _selectioncard, _disclosures, _banddetail, self,
        _emit_assign_labor, _herd_label_for_id, _targeting)
    _bandpanel.cancel_order_requested.connect(
        func(band: Dictionary, scope: String) -> void: cancel_order_requested.emit(band, scope))
    _bandpanel.send_hunt_expedition_requested.connect(
        func(payload: Dictionary) -> void: send_hunt_expedition_requested.emit(payload))
    _bandpanel.recall_expedition_requested.connect(
        func(payload: Dictionary) -> void: recall_expedition_requested.emit(payload))
    _bandpanel.alert_focus_requested.connect(
        func(x: int, y: int) -> void: alert_focus_requested.emit(x, y))
    _bandpanel.roster_occupant_selected.connect(
        func(kind: String, id: Variant) -> void: roster_occupant_selected.emit(kind, id))
    # The band/expedition attention producers + orb jump-routing. Constructed AFTER `_bandpanel` (its
    # expedition/pen jumps reuse the panel's own focus paths) and handed the ONE retained helper,
    # `_herd_label_for_id`. It emits its OWN `alert_focus_requested`, relayed onto the HudLayer signal
    # (a second relayer into that one signal alongside `_bandpanel`'s is fine — Main connects to it once).
    # The orb's focus signal is wired here, now that `_attention` exists (see the deferred connect above).
    _attention = AttentionController.new(_band_labor, _bandpanel, _herd_label_for_id)
    _attention.alert_focus_requested.connect(
        func(x: int, y: int) -> void: alert_focus_requested.emit(x, y))
    _turnorb.focus_requested.connect(_attention.on_turn_orb_focus)
    # The selection drawer's render dispatch. Constructed AFTER `_bandpanel` + `_drawercompose` (it
    # dispatches into both) and handed the SAME selection/labor models, the sibling controllers, the
    # HUD CanvasLayer as the host its fit awaits a frame through (a RefCounted has no `get_tree()`), the
    # drawer scene nodes it writes (kept `@onready` here — a `%Name` node loses `unique_name_in_owner`
    # if reparented), and the targeting controller whose `begin_move_band` its Move button connects to.
    _drawer = SubjectDrawerController.new(
        _selection, _band_labor, _selectioncard, _drawercompose, _bandpanel, _banddetail, self,
        tile_detail, occupant_detail, allocation_panel, herd_assign_controls, forage_assign_controls,
        subject_body, subject_scroll, left_dock_scroll, _targeting)
    _load_ui_balance_config()
    _connect_zoom_rail()
    _setup_tooltip()
    _legend.refresh_rows()
    _refresh_campaign_label()
    _refresh_victory_status()
    _command_feed.render()
    _telling.render()
    _connect_selection_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    left_dock.add(stockpile_panel, 20)
    left_dock.add(command_feed_panel, 30)
    # The right dock is the narrative surface's home: the telling owns the top of it and, with both
    # reference cards hidden by default, effectively the whole column. Sharing the left dock left it
    # cramped under the selection cards + command feed.
    right_dock.add(telling_panel, 10)
    right_dock.add(victory_panel, 20)
    right_dock.add(terrain_legend_panel, 30)
    _load_hud_panel_prefs()
    if stockpile_panel != null:
        stockpile_panel.visible = false
    if stockpile_title != null:
        stockpile_title.text = "Stockpiles"
    _apply_hud_style()
    _setup_build_overlay()
    # The selection drawer's Food/Morale labels are click-to-expand breakdown disclosures.
    _disclosures.wire_label(occupant_detail)
    # Re-cap the drawer whenever its content changes SIZE, whoever changed it — a stepper tick, a
    # policy click, a per-snapshot rebuild. One hookup instead of a refit call sprinkled through
    # every early-return in the three compose builders. No feedback loop: the fit writes the
    # SCROLL's minimum, which is outside the body it measures.
    if subject_body != null:
        subject_body.minimum_size_changed.connect(_drawer.fit_subject_drawer)
    # A window resize changes the dock's height, hence the room the drawer may claim — force the
    # refit past the same-height gate (the content is unchanged, but the room it fits into is not).
    get_viewport().size_changed.connect(_drawer.fit_subject_drawer.bind(true))

## Apply the shared HudStyle console look to the selection panel: restyle its
## action buttons, tint the detail text, and bring the two plain PanelContainers
## (stockpile, victory) up to the same card chrome the PanelCards already use.
func _apply_hud_style() -> void:
    for detail in [tile_detail, occupant_detail]:
        if detail != null:
            detail.add_theme_color_override("default_color", HudStyle.INK_DIM)
            detail.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
            detail.add_theme_constant_override("table_h_separation", 16)
            detail.add_theme_constant_override("table_v_separation", 3)
    # The list ↔ drawer hairline: the palette owns the rule, the node owns its thickness.
    if subject_divider != null:
        subject_divider.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
        subject_divider.custom_minimum_size = Vector2(0.0, HudSelectionVocab.SUBJECT_DIVIDER_HEIGHT)
        subject_divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
    if stockpile_panel != null:
        stockpile_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    if victory_panel != null:
        victory_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())

## True while any command-targeting flow is armed. Reflective delegator: Main._unhandled_input probes it
## BY NAME so Esc yields to MapView's targeting-cancel, and a has_method probe fails SILENTLY — so it must
## resolve on the HUD node.
func is_targeting_active() -> bool:
    return _targeting.is_targeting_active()

## Cancel the active targeting (banner Cancel / Esc / right-click all route here). Reflective delegator:
## Main relays MapView's targeting_cancel_requested to it BY NAME.
func cancel_active_targeting() -> void:
    _targeting.cancel_active_targeting()

## Bottom-CENTRE version overlay showing the client build and the streamed server build,
## so the running builds can be confirmed at a glance. It lives centre-bottom rather than
## lower-left because the minimap + zoom rail own the lower-left corner and hid it. Spans the
## full width with centred text (so it can never collide with the corner clusters) and is
## mouse-transparent so it never intercepts map clicks.
func _setup_build_overlay() -> void:
    _build_label = Label.new()
    _build_label.name = "BuildOverlay"
    _build_label.anchor_left = 0.0
    _build_label.anchor_right = 1.0
    _build_label.anchor_top = 1.0
    _build_label.anchor_bottom = 1.0
    _build_label.offset_left = 0.0
    _build_label.offset_top = -26.0
    _build_label.offset_right = 0.0
    _build_label.offset_bottom = -6.0
    _build_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    _build_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    _build_label.add_theme_color_override("font_color", Color(0.85, 0.9, 1.0, 0.65))
    add_child(_build_label)
    _refresh_build_overlay()

func _refresh_build_overlay() -> void:
    if _build_label != null:
        _build_label.text = "build  cli %s · srv %s" % [ClientBuild.current(CLIENT_BUILD), _server_build]

## Called from Main with the server build id from each snapshot header.
func update_build_info(server_build: String) -> void:
    _server_build = server_build if server_build != "" else "?"
    _refresh_build_overlay()

func set_localization_store(store) -> void:
    localization_store = store
    _refresh_campaign_label()

func update_campaign_label(label: Dictionary) -> void:
    campaign_label = label.duplicate(true) if label is Dictionary else {}
    _refresh_campaign_label()

func update_victory_state(state: Dictionary) -> void:
    print("[HUD] update_victory_state: ", state.keys())
    victory_state = state.duplicate(true) if state is Dictionary else {}
    _refresh_victory_status()

func update_overlay(turn: int, metrics: Dictionary) -> void:
    # A HudLayer fan-out: the top-bar labels render through the TopBarReadouts controller; the turn orb
    # and the authoritative snapshot turn (which drives optimistic-pending reconciliation — see
    # _reconcile_pending, called from update_band_alerts later in the same snapshot cycle) stay here.
    _topbar.render_overlay(turn, metrics)
    _band_labor.set_turn(turn)
    _turnorb.set_turn(turn)

## A block-glyph bar for a 0–100 score. `cells` is passed by every caller — the Sedentarization meter
## (via TopBarReadouts) at the standard width, the knowledge strip narrower, the herd-drawer danger
## rows narrower still. Kept on HudLayer because THREE clusters read it; handed to the TopBarReadouts
## controller as a Callable and called as `HudLayer._meter_bar` by `DetailFormat`'s danger bars.
## `static` so that all-static module can reach it without a Callable injection — it touches no member.
static func _meter_bar(score: float, cells: int) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * float(cells)))
    return "▰".repeat(filled) + "▱".repeat(cells - filled)

## Top-bar faction readouts — thin delegators to the TopBarReadouts controller (`_topbar`), which owns
## the Sedentarization / demographics / discoveries / intensification / stockpile rendering. These
## names stay on HudLayer because Main reaches them by reflection (`_hud_invoke` → has_method+callv).
func update_stockpiles(faction_inventory_variant: Variant) -> void:
    _topbar.update_stockpiles(faction_inventory_variant)

func update_sedentarization(sedentarization_variant: Variant) -> void:
    _topbar.update_sedentarization(sedentarization_variant)

func update_demographics(demographics_variant: Variant) -> void:
    _topbar.update_demographics(demographics_variant)

func update_intensification(intensification_variant: Variant) -> void:
    _topbar.update_intensification(intensification_variant)

func update_discoveries(discovered_variant: Variant) -> void:
    _topbar.update_discoveries(discovered_variant)

## Render the live map-zoom readout (e.g. "1.6×"). Driven by MapView.zoom_changed
## via Main, so it reflects the rail buttons, the wheel, and the Q/E keys alike.
func set_zoom_readout(zoom_factor: float) -> void:
    if zoom_level_label != null:
        zoom_level_label.text = "%.1f×" % zoom_factor

## Wire the bottom-left zoom rail: ＋/－ step the map zoom, ⊡ fits to view. Every
## button is styled through HudStyle (no raw default-theme buttons); the readout
## label reads as tabular cyan mono.
func _connect_zoom_rail() -> void:
    if nav_backing != null:
        nav_backing.add_theme_stylebox_override("panel", HudStyle.nav_backing_stylebox())
    HudStyle.apply_button(zoom_in_button2, "ghost")
    HudStyle.apply_button(zoom_out_button2, "ghost")
    HudStyle.apply_button(zoom_fit_button, "ghost")
    if zoom_level_label != null:
        zoom_level_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
    if zoom_in_button2 != null and not zoom_in_button2.is_connected("pressed", Callable(self, "_on_zoom_in_pressed")):
        zoom_in_button2.pressed.connect(_on_zoom_in_pressed)
    if zoom_out_button2 != null and not zoom_out_button2.is_connected("pressed", Callable(self, "_on_zoom_out_pressed")):
        zoom_out_button2.pressed.connect(_on_zoom_out_pressed)
    if zoom_fit_button != null and not zoom_fit_button.is_connected("pressed", Callable(self, "_on_zoom_fit_pressed")):
        zoom_fit_button.pressed.connect(_on_zoom_fit_pressed)

# ---- The Telling: turn-orb / attention / fork delegators -------------------
# The cluster lives in `_turnorb` (TurnOrbController, HUD decomposition Phase 1b). These five methods
# stay reachable on HudLayer because Main reaches them by reflection; each is a thin delegator.

func update_pending_forks(forks_variant: Variant) -> void:
    _turnorb.update_pending_forks(forks_variant)

func update_stance_axes(axes_variant: Variant) -> void:
    _turnorb.update_stance_axes(axes_variant)

func update_voice_medium(medium_variant: Variant) -> void:
    _turnorb.update_voice_medium(medium_variant)

## Is a fork holding the turn? Read by the Inspector-path advance note (the dev toolbar and
## autoplay are deliberately NOT gated — see docs/plan_the_telling.md).
func has_pending_fork() -> bool:
    return _turnorb.has_pending_fork()

## The dev toolbar / autoplay advanced past an unanswered fork. Not a gate — a RECEIPT: the
## server will expire the fork to its defer branch, which is a real narrative outcome, so a
## developer who skipped the question must be able to see that they did.
func note_unanswered_fork() -> void:
    _turnorb.note_unanswered_fork()

## The labor-allocation UI (allocation panel, herd/tile assign controls) is built at
## runtime with its own per-widget signal connections, so there are no static selection
## buttons left to wire here. Kept as a hook for future static selection controls.
func _connect_selection_buttons() -> void:
    pass

func _on_zoom_out_pressed() -> void:
    emit_signal("map_zoom_step", -1)

func _on_zoom_in_pressed() -> void:
    emit_signal("map_zoom_step", 1)

func _on_zoom_fit_pressed() -> void:
    emit_signal("map_zoom_fit")

# ---- Early-Game Labor allocation (slice 3b) --------------------------------
# Source-centric worker allocation for the single player band. The allocation panel
# (band drawer), the herd "assign hunters" controls, and the tile "assign foragers"
# controls are all built at runtime here; each emits `assign_labor_requested` (Main
# formats the `assign_labor …` command). The Work zone's bulk unassign reuses
# `cancel_order_requested`, scoped `work`.

## Resolve the band that assignment/move/clear commands target. The selected band when
## it is a player band; otherwise the single player band captured from the snapshot (so
## herd/tile assign controls still target it while a herd/tile is selected). {} if none.
func _resolve_assign_band() -> Dictionary:
    if not _selection.unit().is_empty() and _is_player_unit(_selection.unit()):
        return _selection.unit()
    return _band_labor.player_band()

## Map grid dimensions captured each snapshot (Main forwards the snapshot `grid` key). Width + wrap
## feed the wrap-aware hex distance the herd-hunt affordance keys its local-vs-expedition decision
## off. Grid rides full snapshots only; persists across deltas (fields default to the last value).
func set_grid_dimensions(grid: Variant) -> void:
    if not (grid is Dictionary):
        return
    var g: Dictionary = grid
    _band_labor.set_grid(int(g.get("width", _band_labor.grid_width())), int(g.get("height", _band_labor.grid_height())),
        bool(g.get("wrap_horizontal", _band_labor.wrap_horizontal())))

## The world's herds captured each snapshot (Main forwards the snapshot `herds` key, the same array
## `MapView._rebuild_herd_markers` consumes). Herds MIGRATE every turn, so this — not a hunt
## assignment's launch-time `target_x/target_y` — is the authority on where a hunted herd IS.
func update_herds(herds_variant: Variant) -> void:
    if not (herds_variant is Array):
        return
    _band_labor.set_world_herds(herds_variant)

## Ingests MapView's terrain-stamped food sites (x/y/module/kind + terrain_id) into the per-tile map
## the Forage row reads, so its glyph matches the map marker (riverine split included). The per-tile
## lookup lives on `_band_labor` (`food_module_by_tile()`).
func update_food_modules(modules_variant: Variant) -> void:
    _band_labor.set_food_modules(modules_variant)

## Ingests the snapshot forage patches into the per-tile lookup the Current-actions Forage row reads
## to cap its worker stepper at max-useful, mirroring MapView's `forage_patch_lookup` ingest. The
## per-tile lookup lives on `_band_labor` (`forage_patch_lookup()`).
func update_forage_patches(patches_variant: Variant) -> void:
    _band_labor.set_forage_patches(patches_variant)

## The player's starting band tile (col,row) — the first player-faction band captured this snapshot
## into `_band_labor.player_band()` (via update_band_alerts). Returns (-1,-1) when there is no player band, so a
## caller (Main's startup-view centering) can defensively skip the focus. Reads the same `current_x/y`
## cohort fields `SourceForecast.band_tile` does.
func get_player_band_tile() -> Vector2i:
    if _band_labor.player_band().is_empty():
        return Vector2i(-1, -1)
    return SourceForecast.band_tile(_band_labor.player_band())




## Wrap-aware odd-r hex distance between two offset tiles, supplying the snapshot's grid geometry to
## the ONE implementation (`SourceForecast.hex_distance_wrapped`). This pass-through exists precisely
## because the module is stateless: the grid pair (`grid_width`, `wrap_horizontal`) lives on
## `_band_labor` (fed by `set_grid_dimensions`), and the distance readouts that call this (herd reach,
## expedition range, work-range checks) have no other business knowing about it. -1 for an unknown tile.
func _hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
    return SourceForecast.hex_distance_wrapped(
        a_col, a_row, b_col, b_row, _band_labor.grid_width(), _band_labor.wrap_horizontal())

## The band's labor-assignment array, or [] when the snapshot carried none. `static` so `DetailFormat`
## can read it as `HudLayer._labor_assignments_of` for the Gathered/Hunted sums rather than keeping a
## fourth private copy of the same two-line accessor.
static func _labor_assignments_of(band: Dictionary) -> Array:
    var v: Variant = band.get("labor_assignments", [])
    return v if v is Array else []

## A friendlier label for a herd id — the roster/selected herd's label when known, else the
## snapshot-wide herd list (a hunted herd usually sits on a DIFFERENT hex than the one selected,
## so the roster alone left those rows reading the raw `game_deer_07` id).
func _herd_label_for_id(herd_id: String) -> String:
    var herd := _selectioncard.find_roster_herd(herd_id)
    if not herd.is_empty():
        return String(herd.get("species", herd.get("label", herd_id)))
    if String(_selection.herd().get("id", "")) == herd_id:
        return String(_selection.herd().get("species", _selection.herd().get("label", herd_id)))
    var world_herd := _band_labor.find_world_herd(herd_id)
    if not world_herd.is_empty():
        return String(world_herd.get("species", world_herd.get("label", herd_id)))
    return herd_id

## Emit an assign_labor request for the given band, and record it as an OPTIMISTIC pending
## action so the panel + map reflect the change immediately (reconciled by the next
## newer-turn snapshot). Main formats the text command from the emitted payload.
## `species` is the FORAGE-only crop selection (flora roster S1) — which named plant a Cultivate/Sow
## should commit the patch to. Empty (the default, and what every non-forage caller sends) means "pick
## the tile's dominant legal plant for me", the same absent-means-default convention `policy` has.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String, species: String = "") -> void:
    var bits := int(band.get("entity", -1))
    if bits < 0:
        return
    var clamped: int = max(0, workers)
    emit_signal("assign_labor_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band": bits,
        "kind": kind,
        "workers": clamped,
        "x": x,
        "y": y,
        "herd_id": herd_id,
        "policy": policy,
        "species": species,
    })
    _band_labor.record_pending_assign(bits, kind, clamped, x, y, herd_id, policy)
    _after_pending_change()

# ---- Optimistic pending labor (slice 3b UX) --------------------------------
# The pending-overlay DATA (record / reconcile / the effective-worker maps + `as_schedule`) lives on
# `_band_labor`; the HUD keeps only the orchestration around it — the re-render and the
# `labor_pending_changed` push to MapView.

## Re-render the current selection (so pending shows in the Occupants/Tile cards) and push the
## pending map to MapView (so pending hexes show), after any optimistic change. Also re-render the
## Band/City panel keyed off the panel band — a worker-stepper edit in the panel must show its
## optimistic pending even when the current selection is a foreign hex (never blank it).
func _after_pending_change() -> void:
    if not _selection.tile_info().is_empty() or not _selection.unit().is_empty() or not _selection.herd().is_empty():
        _render_selection_panel(_selection.tile_info(), _selection.unit(), _selection.herd())
    _bandpanel.rerender()
    emit_signal("labor_pending_changed", _band_labor.pending_labor())

## Drop pending entries the server has already processed: a snapshot with a turn NEWER than the
## entry's issue turn is authoritative confirmation (and reflects any clamping). Called each snapshot
## from update_band_alerts, after update_overlay has set the turn. The DATA drop lives on the model;
## the HUD pushes the pruned overlay to MapView when the model reports anything changed.
func _reconcile_pending() -> void:
    if _band_labor.reconcile_pending(_band_labor.current_turn()):
        emit_signal("labor_pending_changed", _band_labor.pending_labor())

## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Both
## hosts, unconditionally — that is the `is_panel` fork this change exists to remove.
func _refresh_disclosure_hosts() -> void:
    if _bandpanel.has_panel() and not _band_labor.panel_band().is_empty():
        _bandpanel.render_band(_band_labor.panel_band())
    _drawer.render_subject_drawer()

# ---- THE COMPOSE SHEET: the two reflective delegators -----------------------------------------
#
# The sheet itself (lifecycle, drawer actions, compose builders, forecast/gate/picker layer) lives in
# `DrawerComposeController`. These two methods are probed BY NAME from outside the HUD — Esc
# precedence in `Main._unhandled_input` and the ui_preview harness — and a `has_method` probe fails
# SILENTLY, so they must keep resolving on the HUD node itself.

## Is a compose sheet open? `Main._unhandled_input` asks this FIRST on Esc — the sheet is the
## innermost surface, so it claims the key ahead of targeting-cancel and the pause menu (§15).
func is_compose_sheet_open() -> bool:
    return _drawercompose.is_compose_sheet_open()

## Close any open sheet and return to the read state. Idempotent, so every close reason (commit, ✕,
## catcher click, Esc, selection change, targeting) can call it unconditionally.
func close_compose_sheet() -> void:
    _drawercompose.close_compose_sheet()


## Map double-click convenience (Main forwards `MapView.herd_quick_hunt_requested`): assign
## ALL of the player band's currently-idle workers to hunt `herd_id` at the default Sustain
## policy. A no-op (with a command-feed note) when there's no player band or no idle workers,
## so the shortcut never silently does nothing.
func quick_assign_hunters(herd_id: String) -> void:
    if herd_id.strip_edges() == "":
        return
    var band := _resolve_assign_band()
    if band.is_empty():
        _note_command_feed("Quick-hunt", "No player band to assign.")
        return
    var idle := int(band.get("idle_workers", 0))
    if idle <= 0:
        _note_command_feed("Quick-hunt", "No idle workers to assign to %s." % herd_id)
        return
    _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, idle,
        int(band.get("current_x", -1)), int(band.get("current_y", -1)), herd_id, SourceForecast.DEFAULT_HUNT_POLICY)

func update_overlay_legend(legend: Dictionary) -> void:
    _legend.update(legend)
func get_upper_stack_height() -> float:
    var max_bottom := 0.0
    for label in [campaign_title_label, campaign_subtitle_label, turn_label, metrics_label, victory_status_label]:
        if label == null:
            continue
        var top: float = label.position.y
        var size: float = label.get_combined_minimum_size().y
        if size <= 0.0:
            size = label.size.y
        if size <= 0.0:
            size = 20.0
        max_bottom = max(max_bottom, top + size)
    if max_bottom <= 0.0:
        max_bottom = 24.0
    return max_bottom + STACK_ADDITIONAL_MARGIN

## Reserve a strip of one edge for a docked panel (keyed by reserver id). The
## panels keep their natural docks; the whole layout just lives in the smaller
## rectangle, matching the shrunk map area. `edge` is a Godot Side const
## (SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM); `size <= 0` releases the reserver.
func set_reserved_inset(id: StringName, edge: int, size: float) -> void:
    if size <= 0.0:
        _reservations.erase(id)
    else:
        _reservations[id] = {"edge": edge, "size": size}
    _recompute_insets()
    if layout_root != null:
        layout_root.offset_left = _inset_left
        layout_root.offset_top = _inset_top
        layout_root.offset_right = -_inset_right
        layout_root.offset_bottom = -_inset_bottom

## Sum the registered reservations into the four per-edge totals.
func _recompute_insets() -> void:
    _inset_left = 0.0
    _inset_right = 0.0
    _inset_top = 0.0
    _inset_bottom = 0.0
    for reservation in _reservations.values():
        var size: float = float(reservation["size"])
        match int(reservation["edge"]):
            SIDE_LEFT:
                _inset_left += size
            SIDE_TOP:
                _inset_top += size
            SIDE_RIGHT:
                _inset_right += size
            SIDE_BOTTOM:
                _inset_bottom += size
func _refresh_campaign_label() -> void:
    if campaign_title_label == null or campaign_subtitle_label == null:
        return
    var title_text := _resolve_localized_field("title")
    var subtitle_text := _resolve_localized_field("subtitle")
    var has_title := title_text.strip_edges() != ""
    var has_subtitle := subtitle_text.strip_edges() != ""
    campaign_title_label.visible = has_title
    campaign_subtitle_label.visible = has_subtitle
    campaign_title_label.text = title_text if has_title else ""
    campaign_subtitle_label.text = subtitle_text if has_subtitle else ""

## Clear the command FEED only — a full snapshot re-seeds it from the server's ring, so keeping
## stale receipts would double them up. The Telling panel is deliberately NOT reset here: its
## signature de-dup makes re-ingesting the ring harmless, and clearing would throw away every
## telling that has already scrolled past the server's 32-entry ring.
func reset_command_feed() -> void:
    _command_feed.reset()
func show_tile_selection(tile_info: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _selection.select_tile(tile_info.duplicate(true) if tile_info is Dictionary else {})
    _render_selection_panel(_selection.tile_info(), {}, {})
    _targeting.try_dispatch(_selection.tile_info())

func notify_hex_selected(tile_info: Dictionary) -> void:
    if tile_info.is_empty():
        return
    _targeting.try_dispatch(tile_info)

func show_unit_selection(unit_data: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    var tile_info: Dictionary = {}
    var tile_variant: Variant = unit_data.get("tile_info", {})
    if tile_variant is Dictionary:
        tile_info = (tile_variant as Dictionary).duplicate(true)
    else:
        tile_info = _selection.tile_info()
    _selection.set_tile_info(tile_info)
    _selection.select_unit(unit_data.duplicate(true))
    _render_selection_panel(tile_info, _selection.unit(), {})

func show_herd_selection(herd_data: Dictionary) -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    var tile_info: Dictionary = {}
    var tile_variant: Variant = herd_data.get("tile_info", {})
    if tile_variant is Dictionary and not (tile_variant as Dictionary).is_empty():
        tile_info = (tile_variant as Dictionary).duplicate(true)
    elif _herd_matches_selected_tile(herd_data):
        # Same hex as the currently-selected tile (a map click on a hex that has
        # both a gather module and a fauna group): surface Harvest alongside the
        # herd verbs. A herd picked from the inspector (no tile_info, unrelated tile
        # selected) falls through to herd-only so Harvest can't mis-target.
        tile_info = _selection.tile_info()
    _selection.set_tile_info(tile_info)
    _selection.select_herd(herd_data.duplicate(true))
    _render_selection_panel(tile_info, {}, _selection.herd())

## True when the currently-selected tile is the same hex the herd occupies, so it
## is safe to keep showing that tile's Harvest verb alongside the herd verbs.
func _herd_matches_selected_tile(herd_data: Dictionary) -> bool:
    if _selection.tile_info().is_empty():
        return false
    return int(_selection.tile_info().get("x", -1)) == int(herd_data.get("x", -2)) \
        and int(_selection.tile_info().get("y", -1)) == int(herd_data.get("y", -2))

## Coordinator: render both left-dock cards from the current selection state.
## The two cards are two scene nodes driven by one script — the Tile card is the
## place (terrain + Forage), the Occupants card is the selectable band/wildlife
## roster + a detail drawer for the chosen occupant. The `*_data` params mirror
## the members the show_*/pending flows already set; the members are authoritative.
## Re-render the selection panel for the still-selected occupant/tile using fresh
## snapshot data (called from Main after each snapshot via MapView.refresh_selection_payload).
## Unlike the show_* entry points this runs NO click-time side effects — no pending-scout
## dispatch, no forage/hunt/follow consumption — so refreshing every turn can't misfire a
## pending command. Keeps the panel live across turn advances instead of going stale until
## the user reselects the hex. "none" means the selected band/herd is gone → drop to its
## tile if we still have one, else hide the cards (without cancelling pending forage).
func reapply_selection(kind: String, data: Dictionary) -> void:
    match kind:
        "unit":
            _selection.select_unit(data.duplicate(true) if data is Dictionary else {})
            _adopt_tile_info_from(_selection.unit())
            _render_selection_panel(_selection.tile_info(), _selection.unit(), {})
        "herd":
            _selection.select_herd(data.duplicate(true) if data is Dictionary else {})
            _adopt_tile_info_from(_selection.herd())
            _render_selection_panel(_selection.tile_info(), {}, _selection.herd())
        "tile":
            _selection.select_tile(data.duplicate(true) if data is Dictionary else {})
            _render_selection_panel(_selection.tile_info(), {}, {})
        _:
            # Selected occupant vanished (e.g. the band expired). Drop to its last tile
            # if known, else hide the card. Intentionally does not touch pending state.
            _selection.select_land()
            if _selection.tile_info().is_empty():
                _hide_selection_card()
            else:
                _render_selection_panel(_selection.tile_info(), {}, {})

## Pull the fresh tile_info a refresh payload carries alongside the occupant, so the tile
## card + roster render against the same snapshot the occupant came from.
func _adopt_tile_info_from(occupant: Dictionary) -> void:
    var ti_variant: Variant = occupant.get("tile_info", {})
    if ti_variant is Dictionary and not (ti_variant as Dictionary).is_empty():
        _selection.set_tile_info((ti_variant as Dictionary).duplicate(true))

func _render_selection_panel(_tile_info: Dictionary, _unit_data: Dictionary, _herd_data: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    # No tint context is reset here any more: it is no longer a member that outlives a render. Each
    # host below (the drawer, the panel's vitals label) constructs its own `DetailFormat.Context`
    # immediately before it renders, so there is nothing stale for this orchestrator to clear.
    # The identity/list half — roster assembly, tile-card header + chips, auto-select, subject list —
    # lives in the controller (HUD decomposition Phase 2b); the DRAWER stays here (Phase 2c).
    _selectioncard.render(_selection.tile_info())
    _drawer.render_subject_drawer()

## The controller changed the lit subject via a roster/land CLICK. Re-render BOTH halves: close the
## compose sheet (a selection change invalidates the subject being composed, §15) then re-run the whole
## panel (which resets the tint context, re-renders the list accent, and re-renders the drawer for the
## new subject). The auto-pick does NOT route here — it emits only `roster_occupant_selected`, since it
## runs mid-`render`.
func _on_selection_subject_changed() -> void:
    close_compose_sheet()
    _render_selection_panel(_selection.tile_info(), {}, {})

## Hide the whole selection card (no tile, no occupant). One place, so the drawer's three
## compose blocks can't be left visible behind a hidden card.
func _hide_selection_card() -> void:
    if tile_panel != null:
        tile_panel.visible = false
    _hide_drawer_blocks()

func _hide_drawer_blocks() -> void:
    if forage_assign_controls != null:
        forage_assign_controls.visible = false
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false

# ---- THE BAND/CITY PANEL: the three reflective delegators -------------------------------------
#
# The panel itself (its handle, the three zone builders, the zone state, the cycler + snapshot
# refresh) lives in `BandPanelController`. These three methods are reached BY NAME from
# `Main._wire_band_city_panel` — `has_method` probes, and the latter two are then bound to
# `BandCityPanel`'s `cycle_requested` / `subject_activated` signals as `Callable(hud, "…")`. A failed
# probe fails SILENTLY, so they must keep resolving on the HUD node itself.

## Injected by Main: the dockable Band/City panel a player band's detail renders into.
func set_band_city_panel(panel: BandCityPanel) -> void:
    _bandpanel.set_panel(panel)

## Walk to the next/prev player band (the panel cycler's ◀/▶).
func cycle_panel_band(delta: int) -> void:
    _bandpanel.cycle_band(delta)

## Jump to the panel band on the map (the panel header's "jump to my band" affordance).
func focus_panel_band() -> void:
    _bandpanel.focus_band()

## Player-faction check for a roster/drawer band (mirrors MapView._is_player_unit).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

func clear_selection() -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _selection.select_land()
    # Keep pending move-band so the user can still choose a destination after deselecting.
    if _selection.tile_info().is_empty():
        _hide_selection_card()
    else:
        _render_selection_panel(_selection.tile_info(), {}, {})

func _travel_eta_hint(tile_info: Dictionary) -> String:
    var distance := int(tile_info.get("nearest_unit_distance", -1))
    if distance < 0:
        return ""
    var turns := _estimate_travel_turns(distance)
    if turns < 0:
        return ""
    var label := String(tile_info.get("nearest_unit_label", "")).strip_edges()
    if label == "":
        label = "Band"
    return "Nearest band %s is %d tiles away (~%d turns)." % [label, distance, turns]

func _travel_turns_for_tile(tile_info: Dictionary) -> int:
    var distance := int(tile_info.get("nearest_unit_distance", -1))
    return _estimate_travel_turns(distance)

func _estimate_travel_turns(distance: int) -> int:
    if distance < 0:
        return -1
    if travel_tiles_per_turn <= 0.0:
        return distance
    var turns := int(ceil(float(distance) / travel_tiles_per_turn))
    if travel_preview_turn_cap > 0:
        turns = min(turns, travel_preview_turn_cap)
    return turns

func _load_ui_balance_config() -> void:
    if not FileAccess.file_exists(UI_BALANCE_CONFIG_PATH):
        return
    var file := FileAccess.open(UI_BALANCE_CONFIG_PATH, FileAccess.READ)
    if file == null:
        return
    var text := file.get_as_text()
    file.close()
    var data: Variant = JSON.parse_string(text)
    if not (data is Dictionary):
        return
    var travel_dict_variant: Variant = data.get("travel", {})
    if travel_dict_variant is Dictionary:
        var travel_dict: Dictionary = travel_dict_variant
        var speed_value := float(travel_dict.get("tiles_per_turn", travel_tiles_per_turn))
        if speed_value > 0.0:
            travel_tiles_per_turn = speed_value
        var cap_value := int(travel_dict.get("max_preview_turns", travel_preview_turn_cap))
        if cap_value > 0:
            travel_preview_turn_cap = cap_value

## Fan one batch of command events out to BOTH surfaces. Each controller filters for the kinds it
## owns (the split's one definition is `TellingPanel.handles_kind`), so passing the whole array to
## both is correct and keeps each one's own retention + de-duplication.
##
## This is also the Telling panel's BACKFILL: a full snapshot carries the server's whole
## `commandEvents` ring, so a player opening the client mid-session sees recent history.
func ingest_command_events(events_variant: Variant) -> void:
    _command_feed.ingest_events(events_variant)
    _telling.ingest_events(events_variant)
func update_band_alerts(populations_variant: Variant) -> void:
    if not (populations_variant is Array):
        return
    var populations: Array = populations_variant
    # 1. PURE roster split — no attention built here anymore (it moved to `AttentionController`).
    # Capture the player bands each snapshot; the labor-allocation UI targets them (assign/move/
    # clear) and reads their labor_assignments for the herd/tile assign controls. `player_band`
    # (first) stays the default actor; `player_bands` backs the assign controls' band-picker.
    # Split expeditions out of the band roster: they are detached scout/hunt parties, never a labor
    # actor band, and must not be counted by the cycler, listed in the band-picker, or given
    # band-style attention labels. The attention producers key off the bands-only list, so an
    # expedition never surfaces as "Band N starving/losing/idle".
    var new_sizes: Dictionary = {}
    var player_band: Dictionary = {}
    var player_bands: Array = []
    var player_expeditions: Array = []
    for entry_variant in populations:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        if int(entry.get("faction", -1)) != HudConst.PLAYER_FACTION_ID:
            continue
        if bool(entry.get("is_expedition", false)):
            player_expeditions.append(entry)
            continue
        if player_band.is_empty():
            player_band = entry
        player_bands.append(entry)
        new_sizes[int(entry.get("entity", -1))] = int(entry.get("size", 0))
    # 2. Attention BEFORE ingest — a load-bearing ordering. Producer 2 (losing-population) reads
    # `_band_labor.prev_band_sizes()`, which `ingest_snapshot_bands` OVERWRITES for next turn, so the
    # build must run against the PRE-INGEST sizes or every band silently stops reporting decline.
    var attention := _attention.build_band_attention(player_bands, player_expeditions)
    # 3. Ingest (overwrites prev_band_sizes) — unchanged.
    _band_labor.ingest_snapshot_bands(new_sizes, player_band, player_bands, player_expeditions)
    # 4. Feed the band/expedition half to the turn-orb controller, which caches it and pushes the whole
    # registry (bands + the fork producer) as ONE replace — set_attention is wholesale, so a separate
    # call would wipe these rows.
    _turnorb.set_band_attention(attention)
    # This snapshot is authoritative: drop optimistic pending actions the server has now
    # processed (issued on an older turn), then let the panels render the confirmed state.
    _reconcile_pending()
    # Keep the dockable Band/City panel a persistent, live command center: shown whenever ≥1
    # player band exists, re-rendering the current _band_labor.panel_band() so its steppers/idle stay current.
    _bandpanel.refresh_snapshot()
    # Keep the on-screen allocation panel / assign controls live as the band's staffing
    # changes turn to turn (the coordinator re-renders occupant/tile cards separately, but
    # a herd/tile selection reads _band_labor.player_band(), which only just refreshed here).
    _drawercompose.refresh_drawer_actions()
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject — it must not close on a
    # snapshot, or it would be unusable under autoplay (§15). It closes only if its subject is gone.
    _drawercompose.refresh_compose_sheet()

func _note_command_feed(label: String, detail: String) -> void:
    _command_feed.note(label, detail)
func _refresh_victory_status() -> void:
    # A data refresh never un-hides a card the player suppressed.
    _apply_victory_visibility()
    if victory_status_label == null:
        return
    if victory_state.is_empty():
        victory_status_label.visible = false
        victory_status_label.text = ""
        return
    victory_status_label.visible = true
    var lines: Array = ["[b]Victory[/b]"]
    var winner_variant: Variant = victory_state.get("winner", {})
    if winner_variant is Dictionary and not (winner_variant as Dictionary).is_empty():
        var winner_dict: Dictionary = winner_variant
        var label_text := String(winner_dict.get("label", winner_dict.get("mode", "Victory")))
        var tick := int(winner_dict.get("tick", 0))
        lines.append("[color=gold]Winner:[/color] %s · Tick %d" % [label_text, tick])
    else:
        lines.append("[color=gray]No victory declared.[/color]")
    var modes_variant: Variant = victory_state.get("modes", [])
    if modes_variant is Array:
        var sorted_modes: Array = _sorted_victory_modes(modes_variant as Array)
        var limit: int = min(sorted_modes.size(), 3)
        for idx in range(limit):
            var mode_dict: Dictionary = sorted_modes[idx]
            var label_text := String(mode_dict.get("label", mode_dict.get("id", "Mode")))
            if label_text.strip_edges() == "":
                label_text = _format_victory_label(String(mode_dict.get("id", mode_dict.get("kind", "Mode"))))
            var pct: float = clamp(float(mode_dict.get("progress_pct", 0.0)), 0.0, 1.0) * 100.0
            var achieved := bool(mode_dict.get("achieved", false))
            var prefix := "✔" if achieved else "•"
            lines.append("%s %s — %.1f%%" % [prefix, label_text, pct])
    victory_status_label.bbcode_enabled = true
    victory_status_label.text = String("\n".join(lines))

func _sorted_victory_modes(source: Array) -> Array:
    var entries: Array = []
    for entry in source:
        if entry is Dictionary:
            entries.append((entry as Dictionary).duplicate(true))
    entries.sort_custom(Callable(self, "_victory_mode_sort"))
    return entries

func _victory_mode_sort(a: Dictionary, b: Dictionary) -> bool:
    var pct_a := float(a.get("progress_pct", 0.0))
    var pct_b := float(b.get("progress_pct", 0.0))
    if is_equal_approx(pct_a, pct_b):
        var label_a := _format_victory_label(String(a.get("label", a.get("id", ""))))
        var label_b := _format_victory_label(String(b.get("label", b.get("id", ""))))
        return label_a < label_b
    return pct_a > pct_b

func _format_victory_label(raw: String) -> String:
    var trimmed := raw.strip_edges()
    if trimmed == "":
        return "Victory Mode"
    var sanitized := trimmed.replace("_", " ").replace("-", " ").replace(".", " ")
    var parts: Array = sanitized.split(" ", false)
    for i in range(parts.size()):
        parts[i] = String(parts[i]).capitalize()
    return String(" ".join(parts)).strip_edges()

func _resolve_localized_field(field: String) -> String:
    var text := String(campaign_label.get(field, ""))
    var loc_key_field := "%s_loc_key" % field
    var loc_key := String(campaign_label.get(loc_key_field, ""))
    if localization_store != null and loc_key != "":
        var localized: String = localization_store.resolve(loc_key, text)
        if localized.strip_edges() != "":
            return localized
    return text

func _on_legend_sort_pressed(field: String) -> void:
    _legend.on_sort_pressed(field)

func toggle_legend() -> void:
    _legend.toggle_suppressed()
    _refit_right_dock()
    _save_panel_pref(CONFIG_KEY_LEGEND_SUPPRESSED, _legend.legend_suppressed)

## Victory's counterpart to `toggle_legend` (bound to `V` in Main). Hides/shows the card through the
## dock so the stack reflows with no gap, and remembers the choice for next session.
func toggle_victory() -> void:
    _victory_suppressed = not _victory_suppressed
    _apply_victory_visibility()
    _save_panel_pref(CONFIG_KEY_VICTORY_SUPPRESSED, _victory_suppressed)

## The command feed's counterpart to `toggle_legend` / `toggle_victory` (bound to `R` in Main). The
## feed holds six read-only receipts and NO verbs, so hiding it absorbs nothing — it simply hands
## its dock height to the selection card, which is where the actions are. Hiding goes through the
## controller (not a bare `visible = false`) so the dock reflows with no gap AND the next command
## receipt can't re-show a card the player closed.
func toggle_command_feed() -> void:
    if _command_feed == null:
        return
    _command_feed.toggle_suppressed()
    _refit_left_dock()
    _save_panel_pref(CONFIG_KEY_COMMAND_FEED_SUPPRESSED, _command_feed.feed_suppressed)

func _apply_victory_visibility() -> void:
    if victory_panel == null:
        return
    var should_show := not _victory_suppressed
    if right_dock != null:
        right_dock.set_relevant(victory_panel, should_show)
    else:
        victory_panel.visible = should_show
    _refit_right_dock()

## The Telling panel grows to fit its own (bounded) page, so a sibling's visibility flip no longer
## changes its height — `refit()` just re-syncs the page geometry and re-fits the current page's height
## (it does NOT touch the inner scroll). Kept so this call stays valid and the right dock reflows the
## toggleable cards below it.
func _refit_right_dock() -> void:
    if _telling != null:
        _telling.refit()

## The left dock's twin, for the one event that moves BOTH of its growing cards at once: the `R`
## toggle. The drawer sizes itself against whatever the feed below it reserves, so on a toggle the
## two must settle in a fixed order or each measures the other mid-flight and their sum overspills
## the dock. Release the drawer's claim → let the feed re-fit into the freed column → then let the
## drawer take exactly the remainder. Ordinary selection changes need none of this: the feed is
## already settled and `_drawer.fit_subject_drawer` alone fits into what is left.
func _refit_left_dock() -> void:
    if subject_scroll != null:
        subject_scroll.custom_minimum_size.y = 0.0
    await get_tree().process_frame
    if _command_feed != null:
        _command_feed.refit()
    await get_tree().process_frame
    # The feed just changed the room the drawer may claim, so force past the same-height gate.
    _drawer.fit_subject_drawer(true)

# ---- dock-card visibility persistence --------------------------------------

func _load_hud_panel_prefs() -> void:
    var cfg := ConfigFile.new()
    if cfg.load(NarrativeForkPanel.config_path()) == OK:
        if _legend != null:
            _legend.set_suppressed(bool(cfg.get_value(
                HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_LEGEND_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT)))
        _victory_suppressed = bool(cfg.get_value(
            HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_VICTORY_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT))
        if _command_feed != null:
            _command_feed.set_suppressed(bool(cfg.get_value(
                HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_COMMAND_FEED_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT)))
    else:
        # No prefs file yet (or unreadable): fall back to the hidden-by-default layout.
        if _legend != null:
            _legend.set_suppressed(PANEL_SUPPRESSED_BY_DEFAULT)
        if _command_feed != null:
            _command_feed.set_suppressed(PANEL_SUPPRESSED_BY_DEFAULT)
    _apply_victory_visibility()

## Persist ONE panel's preference — never the whole section.
##
## Writing both keys on either toggle is how a transient state becomes a stored preference: pressing
## `V` used to also write whatever the legend happened to be showing at that instant. That is fine
## while both values are genuine user choices, but it makes the file a snapshot of live UI state
## rather than of decisions, and anything that sets visibility WITHOUT intending to persist it (a
## preview harness, a future "peek" affordance) silently corrupts the other panel's preference. A
## toggle owns its own key and nothing else.
func _save_panel_pref(key: String, suppressed: bool) -> void:
    var cfg := ConfigFile.new()
    cfg.load(NarrativeForkPanel.config_path())   # preserve every other section/key; ignore load errors
    cfg.set_value(HUD_PANELS_CONFIG_SECTION, key, suppressed)
    cfg.save(NarrativeForkPanel.config_path())
func _setup_tooltip() -> void:
    tooltip_panel = PanelContainer.new()
    tooltip_panel.visible = false
    tooltip_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
    tooltip_panel.z_index = 100 # Ensure on top
    
    var style := StyleBoxFlat.new()
    style.bg_color = Color(0.1, 0.1, 0.1, 0.9)
    style.border_width_left = 1
    style.border_width_top = 1
    style.border_width_right = 1
    style.border_width_bottom = 1
    style.border_color = Color(0.4, 0.4, 0.4, 0.8)
    style.corner_radius_top_left = 4
    style.corner_radius_top_right = 4
    style.corner_radius_bottom_right = 4
    style.corner_radius_bottom_left = 4
    style.content_margin_left = 8
    style.content_margin_top = 4
    style.content_margin_right = 8
    style.content_margin_bottom = 4
    tooltip_panel.add_theme_stylebox_override("panel", style)
    
    tooltip_label = Label.new()
    tooltip_label.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
    tooltip_panel.add_child(tooltip_label)
    
    add_child(tooltip_panel)

func _process(_delta: float) -> void:
    _suppress_tooltip_over_ui()

## Hide the hex tooltip whenever the pointer is over an interactive HUD control
## (panel, button, minimap, inspector). The map cannot detect this itself: those
## controls are MOUSE_FILTER_STOP and consume the motion events, so the map never
## receives a "moved away" event to clear its tooltip and it would otherwise stay
## frozen on top of the panel.
func _suppress_tooltip_over_ui() -> void:
    if tooltip_panel == null or not tooltip_panel.visible:
        return
    var viewport := get_viewport()
    if viewport != null and viewport.gui_get_hovered_control() != null:
        tooltip_panel.visible = false

## MapView.tile_hovered lands here — the hex tooltip. The hovered hex is no longer recorded: its only
## reader was the targeting banner's pre-launch raid forecast, which moved INTO the compose sheet once
## the quarry is picked first (the sheet has the real party size and policy; a hover never did).
func show_tooltip(info: Dictionary) -> void:
    if tooltip_panel == null:
        return

    if info.is_empty():
        tooltip_panel.visible = false
        return

    # Never show over interactive HUD controls (see _suppress_tooltip_over_ui).
    var hover_viewport := get_viewport()
    if hover_viewport != null and hover_viewport.gui_get_hovered_control() != null:
        tooltip_panel.visible = false
        return

    var lines: PackedStringArray = []
    
    # Coordinates
    var x := int(info.get("x", -1))
    var y := int(info.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Hex: %d, %d" % [x, y])
        
    # Terrain
    var terrain := String(info.get("terrain_label", ""))
    if terrain != "":
        lines.append("Terrain: %s" % terrain)

    # Hex-edge rivers: which SIDES of the hovered hex carry water. Permanent geography, so it
    # reads on a remembered tile too — hence above the "(last seen)" note. Same RiverEdges
    # formatter as the Tile card; [] on a riverless tile, so no empty row.
    if info.has("river_edges"):
        for river_line in RiverEdges.summary_lines(int(info["river_edges"])):
            lines.append(river_line)

    # Remembered (Discovered) tiles: flag that contents are stale/incomplete.
    if String(info.get("visibility_state", "")) == "discovered":
        lines.append("(last seen — incomplete)")

    # Food
    var food := String(info.get("food_module_label", ""))
    if food != "" and food != "None":
        lines.append("Food: %s" % food)
        
    # Units
    var unit_count := int(info.get("unit_count", 0))
    if unit_count > 0:
        lines.append("Units: %d" % unit_count)
        
    # Herds
    var herd_count := int(info.get("herd_count", 0))
    if herd_count > 0:
        lines.append("Herds: %d" % herd_count)
        
    if lines.is_empty():
        tooltip_panel.visible = false
        return
        
    tooltip_label.text = "\n".join(lines)
    tooltip_panel.visible = true
    
    # Position near mouse
    var mouse_pos := get_viewport().get_mouse_position()
    var viewport_size := get_viewport().get_visible_rect().size
    var panel_size := tooltip_panel.get_combined_minimum_size()
    
    var pos := mouse_pos + Vector2(16, 16)
    
    # Keep within bounds
    if pos.x + panel_size.x > viewport_size.x:
        pos.x = mouse_pos.x - panel_size.x - 16
    if pos.y + panel_size.y > viewport_size.y:
        pos.y = mouse_pos.y - panel_size.y - 16
        
    tooltip_panel.position = pos

## Returns the minimap container for embedding the minimap panel.
## Returns null if container not found.
func get_minimap_container() -> Control:
    return minimap_container

