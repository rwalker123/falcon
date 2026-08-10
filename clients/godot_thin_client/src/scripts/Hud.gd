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
##   x, y (forage/hunt readout), herd_id, floor (the escapement floor, 0..1) }. Main formats the
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
## DENIAL raid (`docs/plan_denial_raid.md`) — the third mission, launched from the parties zone's own
## compose sheet. Payload keys: { faction, band_id, party_workers, fauna_id, fauna_label } and
## **nothing else**: the command grammar `send_denial_raid <faction> <band> <party> <fauna_id>` is
## CLOSED at four tokens, so a floor or a fill target on this payload would be a hard parse error
## rather than an ignored extra. Main formats the `send_denial_raid …` command.
signal send_denial_raid_requested(payload: Dictionary)
## Emitted when the player recalls the selected in-flight expedition (folds it home). Payload
## keys: { faction, expedition }. Main formats the `recall_expedition …` command.
signal recall_expedition_requested(payload: Dictionary)
## Emitted when the player splits a RESIDENT band in two where it stands — fission, not an
## expedition (issue #511, `docs/plan_band_fission.md`). Both halves are ordinary bands the moment
## the command lands: no party, no walk, no arrival. The player picks `workers`, and children,
## elders and every store divide on the share that count implies.
## Payload keys: { faction, band_id, workers } and nothing else — **all three are REQUIRED**, since
## `Main.format_split_band` returns `{}` (a silent no-op, no command sent) when `band_id` or
## `workers` is missing. The grammar `split_band <faction> <band_id> <workers>` is CLOSED at three
## positional tokens.
## Main formats the `split_band …` command.
signal split_band_requested(payload: Dictionary)
## Emitted when the player extends a built pen by one fenced ring (Grazing 2d-γ). Payload keys:
## { faction, x, y } — the pen's anchor tile. Main formats the `extend_pen <faction> <x> <y>` command.
signal extend_pen_requested(payload: Dictionary)
## Emitted when the player commits an IMPROVEMENT — the second axis (issue #442). Payload keys:
## { faction, improvement, x, y, herd_id }. Main formats the matching verb
## (`cultivate` / `sow` / `tame` / `corral`). RELAYED from `DrawerComposeController`, which is its
## only emitter, exactly as `extend_pen_requested` is.
signal improvement_requested(payload: Dictionary)
## Emitted when the player presses **Make** in Materials & Crafting — the recipe goes on the band's
## bench and the sim draws idle workers onto it. **Make IS the assignment**, which is why there is no
## crew argument here and no Crafter role card anywhere. Payload keys: { faction, band_id, recipe_id }.
## Main formats `set_bench <faction> <band> recipe <id>`. RELAYED from `CraftingPanelController`.
signal set_bench_requested(payload: Dictionary)
## Emitted when the bench's `− n +` stepper moves — the job and its progress are left alone. Payload
## keys: { faction, band_id, workers }. Main formats `bench_crew <faction> <band> workers <n>`.
signal bench_crew_requested(payload: Dictionary)
## Emitted when the bench's ✕ is pressed — the job comes off, the crew returns to the idle pool and
## the pile already drawn is spent (the button's tooltip names it, off `drawnInputs`). Payload keys:
## { faction, band_id }. Main formats `clear_bench <faction> <band>`. RELAYED from
## `CraftingPanelController`.
signal clear_bench_requested(payload: Dictionary)
## Optimistic pending-labor state changed (Early-Game Labor slice 3b UX): carries the
## per-band pending map so MapView can draw the pending-action hex highlights. Main forwards
## it to `MapView.set_labor_pending`.
signal labor_pending_changed(pending: Dictionary)
## The player faction's {track: progress} row, pushed to MapView for the worked-source ready marks.
signal faction_knowledge_changed(knowledge: Dictionary)
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

## A CLIENT-SIDE note the player should see — a refused quick-hunt, an unanswered fork, a knowledge
## unlock. It used to be written straight into the left-dock command feed; that feed is retired and
## the surface these land on (`EventDockPanel`, System channel) belongs to `Main`, so the HUD emits
## and `Main` relays. Same shape as every other HudLayer signal: the coordinator mediates.
signal system_note_requested(label: String, detail: String)

## `band_id` (as a String) → the name this HUD gives that band, published on every snapshot from the
## player-band roster. **The snapshot carries no band NAME**: the sim writes a positional
## `Band <BandId>` into a demographic event's label and repeats the id as a `band=` detail token, so
## the event dock can re-label the row with whatever the rest of the HUD calls that band. The client
## name is a ROSTER POSITION and the sim's is a durable id, so the two routinely disagree — the token
## is the only thing that can join them, and this map is the only place the join is possible.
signal band_labels_changed(labels: Dictionary)

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
## **THE ROOM A FREE-FLOATING CARD MAY USE** — `LayoutRoot`'s rect pulled further off any surface
## that OVERLAYS an edge without reserving it (`set_overlay_inset`). It holds no children and draws
## nothing; it exists to BE a rect, so `AutoSizingPanel.room_bounds` goes on being one Control and a
## card's placement and its height fit still fall out of one number. Two rects rather than one
## because the HUD's own layout must NOT move for an overlay — that is what "overlay" means.
@onready var floating_room: Control = $FloatingRoom
@onready var left_dock_region: MarginContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock
@onready var right_dock_region: MarginContainer = $LayoutRoot/RootColumn/ContentRow/RightDock
@onready var bottom_bar: HBoxContainer = $LayoutRoot/RootColumn/BottomBar
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
@onready var left_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack
@onready var right_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack
@onready var right_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll
@onready var turn_orb: TurnOrb = $LayoutRoot/RootColumn/BottomBar/TurnCluster
@onready var minimap_container: MarginContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/MinimapContainer

var tooltip_panel: PanelContainer
var tooltip_label: Label

# The legend card + its terrain-only Name/Count sort header now live in
# ui/hud/LegendController.gd. The left-dock COMMAND FEED is gone: the event dock
# (`ui/EventDockPanel.gd`, its own CanvasLayer) replaced it, and the client-side notes that used to
# land in it are relayed out as `system_note_requested` instead.
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
# Both reference cards start HIDDEN: the right dock is the narrative surface's home, and Victory /
# Terrain Types are look-it-up readouts the player opens on demand (V / L) rather than standing
# furniture competing with the telling for dock height.
const PANEL_SUPPRESSED_BY_DEFAULT := true
const DEFAULT_TRAVEL_SPEED := 3.0
const DEFAULT_TRAVEL_PREVIEW_LIMIT := 12
# The legend card (rows + sort header + suppress state) is owned by _legend; the narrative panel by
# _telling. Hud delegates to both.
var _legend: LegendController = null
## The client-side note sink the three controllers that post one are handed (the top bar's
## knowledge unlock, the turn orb's unanswered fork, targeting's two quarry refusals). It was a
## `CommandFeedController` reference until that feed retired; a Callable onto `note_system_event`
## now, because the panel those notes land on is not the HUD's to hold.
var _note_sink: Callable = Callable(self, "note_system_event")
var _topbar: FactionReadouts = null
var _telling: TellingPanel = null
# Victory's counterpart to the legend's `legend_suppressed` — the player-hidden state of a dock
# card, distinct from "no victory data to show".
var _victory_suppressed: bool = PANEL_SUPPRESSED_BY_DEFAULT
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

## **THE FORECAST QUERY SEAM** — the client's half of the command socket's request/response channel,
## shared by every sheet that composes a raid. It owns no socket: `Main` injects the sender and pumps
## the replies in (see `forecast_query()`), so the HUD asks questions without reaching the network and
## a harness can drive every state of it with no server at all.
var _forecast_query: ForecastQuery = ForecastQuery.new()
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
## **SURFACES THAT COVER A BAND OF THE WINDOW WITHOUT RESERVING IT** — the event dock today, keyed by
## the same StringName id shape as `_reservations` and holding the same `{edge, size}`. They change
## NOTHING about the HUD's own layout (that is what makes them overlays), and exist so a
## FREE-FLOATING card — placed by arithmetic instead of by a container — can be told which band is
## already covered. See `set_overlay_inset`.
var _overlays: Dictionary = {}
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
# The BOTTOM-BAR CHROME PLACEMENT cluster (issue #324): on a HORIZONTAL Band/City dock it parks the
# nav cluster + turn orb into that panel's single trailing rail and drops `BottomBar` out of layout, so
# `ContentRow` reclaims the ~164px the stacked bar used to cost on top of the panel's own height. A
# vertical dock is untouched. Constructed in `_ready` after `_connect_zoom_rail()`, since it MEASURES
# the nav backing and that call is what applies the stylebox whose padding is part of the measurement.
var _dockrow: DockRowController = null
# The MATERIALS & CRAFTING cluster (`docs/plan_crafting_and_materials.md` §7): its own free-floating
# panel, launched from the Band/City panel HEADER, holding the per-world crafting catalogues and the
# band it is open on. It emits its own two command signals, which HudLayer relays like every other
# controller's; `_bandpanel.crafting_requested` is the launch edge, so the two controllers never talk
# to each other directly.
var _crafting: CraftingPanelController = null
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0
## The per-edge depths `_overlays` covers, measured from each screen edge. **A MAXIMUM, where the
## reservations above are a SUM** — an overlay publishes how far in from the edge it is drawn, which
## already includes whatever displaced it inboard, so adding two would double-count the strip they
## share. Only the two horizontal edges are fed today (the event dock is the only overlay and it
## docks top or bottom), and the other two stay at zero rather than being left out, so a future
## overlay on a side edge needs no new arithmetic here.
var _overlay_left: float = 0.0
var _overlay_right: float = 0.0
var _overlay_top: float = 0.0
var _overlay_bottom: float = 0.0
## The `MarginContainer` theme constant `set_right_column_bottom_clearance` writes, and a sentinel no
## margin can hold, so "not captured yet" is distinguishable from a genuinely zero authored margin.
const RIGHT_DOCK_MARGIN_BOTTOM := &"margin_bottom"
const RIGHT_DOCK_MARGIN_UNCAPTURED := -1
var _right_dock_margin_bottom: int = RIGHT_DOCK_MARGIN_UNCAPTURED
var _right_column_bottom_clearance: float = 0.0

func _ready() -> void:
    _selection = HudSelectionState.new()
    _band_labor = HudBandLaborState.new()
    # Both compose floors start on the sim's own default (the food peak); the number stays in
    # `SourceForecast`, not in the model.
    _compose = ComposeState.new(SourceForecast.DEFAULT_HARVEST_FLOOR)
    _legend = LegendController.new(terrain_legend_panel, terrain_legend_scroll, terrain_legend_list, terrain_legend_description)
    # The faction readouts cluster. **IT OWNS NO NODES AT ALL SINCE THE TOP-RIGHT BLOCK WAS RETIRED**
    # (issue #450): the Sedentarization meter, the demographics line, the discovered-sites strip and
    # the knowledge strip were the eight Labels it rendered into, and the faction page's `band` and
    # `knowledge` zones say all of it better. What survives is the INGEST — the per-faction snapshot
    # arrays, filtered to the player and retained, which that page reads back through
    # `faction_tracks` / `faction_sedentarization` / `faction_discovered_sites`.
    # The one injection is `_note_sink`: the knowledge-unlock nudge is a System-channel note, and the
    # panel it lands on is `Main`'s, not the HUD's.
    _topbar = FactionReadouts.new(_note_sink)
    # The telling GROWS TO FIT its current page, capped at `PAGE_MAX_HEIGHT` (docs/plan_the_telling_book_ux.md),
    # so it no longer needs a dock-scroll ceiling to fit against — a page is bounded (one turn's beats), and
    # the right dock's own scroll stacks it above Victory + Terrain Types with no bespoke height math.
    _telling = TellingPanel.new(telling_panel, telling_scroll, telling_label)
    # Turn orb / attention / fork — constructed AFTER _telling (it needs it), handed the HUD
    # CanvasLayer as the host it parents the fork panel into. It emits its OWN signals; HudLayer
    # relays each onto the signals Main connects to (the controller never emits a HudLayer signal).
    _turnorb = TurnOrbController.new(turn_orb, self, _telling, _note_sink)
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
    _drawercompose.improvement_requested.connect(
        func(payload: Dictionary) -> void: improvement_requested.emit(payload))
    # The command-targeting cluster. Constructed AFTER `_drawercompose` (its three close-sheet nudges)
    # and BEFORE `_bandpanel` (which injects `_targeting` — so `_targeting` must exist first). The pick
    # flow's `_bandpanel.rerender()` is therefore a lazily-bound lambda: `_bandpanel` is null now but
    # populated by the time a quarry is picked. It emits its OWN signals; HudLayer relays each (the
    # controller never emits a HudLayer signal). Handed the HUD CanvasLayer as the host it parents the
    # banner into (a RefCounted can't).
    _targeting = TargetingController.new(
        _band_labor, _compose, _drawercompose, _note_sink, self,
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
        _emit_assign_labor, _herd_label_for_id, _targeting, _topbar)
    _bandpanel.cancel_order_requested.connect(
        func(band: Dictionary, scope: String) -> void: cancel_order_requested.emit(band, scope))
    _bandpanel.send_hunt_expedition_requested.connect(
        func(payload: Dictionary) -> void: send_hunt_expedition_requested.emit(payload))
    _bandpanel.send_denial_raid_requested.connect(
        func(payload: Dictionary) -> void: send_denial_raid_requested.emit(payload))
    _bandpanel.recall_expedition_requested.connect(
        func(payload: Dictionary) -> void: recall_expedition_requested.emit(payload))
    _bandpanel.split_band_requested.connect(
        func(payload: Dictionary) -> void: split_band_requested.emit(payload))
    _bandpanel.alert_focus_requested.connect(
        func(x: int, y: int) -> void: alert_focus_requested.emit(x, y))
    _bandpanel.roster_occupant_selected.connect(
        func(kind: String, id: Variant) -> void: roster_occupant_selected.emit(kind, id))
    # MATERIALS & CRAFTING. Constructed after `_bandpanel` because the launch edge comes off it, and
    # handed the SAME labor model plus this CanvasLayer as the node it parents its panel into (a
    # `RefCounted` cannot `add_child` — the `TurnOrbController` pattern). Its two command signals relay
    # onto HudLayer's, like every other controller's; the two controllers are mediated here and never
    # hold each other.
    # `floating_room` rides along as the panel's room: it is `LayoutRoot`'s rect — which the
    # reserved-edge registry insets by every docked panel's strip — pulled further off anything that
    # merely OVERLAYS an edge (`set_overlay_inset`, the event bar). So the card is bounded both by
    # what the map and the HUD are drawn in and by what is drawn OVER them, neither of which the raw
    # viewport describes.
    _crafting = CraftingPanelController.new()
    _crafting.setup(self, _band_labor, floating_room)
    _crafting.set_bench_requested.connect(
        func(payload: Dictionary) -> void: set_bench_requested.emit(payload))
    _crafting.bench_crew_requested.connect(
        func(payload: Dictionary) -> void: bench_crew_requested.emit(payload))
    _crafting.clear_bench_requested.connect(
        func(payload: Dictionary) -> void: clear_bench_requested.emit(payload))
    _bandpanel.crafting_requested.connect(
        func(band: Dictionary) -> void: _crafting.toggle_for(band))
    # The band/expedition attention producers + orb jump-routing. Constructed AFTER `_bandpanel` (its
    # expedition/pen jumps reuse the panel's own focus paths) and handed the ONE retained helper,
    # `_herd_label_for_id`. It emits its OWN `alert_focus_requested`, relayed onto the HudLayer signal
    # (a second relayer into that one signal alongside `_bandpanel`'s is fine — Main connects to it once).
    # The orb's focus signal is wired here, now that `_attention` exists (see the deferred connect above).
    _attention = AttentionController.new(_band_labor, _bandpanel, _herd_label_for_id)
    _attention.alert_focus_requested.connect(
        func(x: int, y: int) -> void: alert_focus_requested.emit(x, y))
    # The faction page's Work and Parties tabs read the same alerts the orb does. Handed over HERE
    # rather than in `_bandpanel`'s construction because `_attention` takes `_bandpanel` itself, so the
    # two cannot both be constructed with the other in hand.
    _bandpanel.set_attention(_attention)
    # **THE FORECAST QUERY SEAM, handed to BOTH raid-composing controllers.** One instance: the drawer's
    # expedition branch and the dock's two sheets ask the same questions of the same sim, and two seams
    # would be two request-id sequences and two staleness rules over one socket. Injected here rather
    # than constructed into either — neither owns it, and `Main` has to reach it to inject the transport
    # (`forecast_query()`), which is the coordinator's job and not a controller's.
    #
    # An answer arrives with no snapshot behind it (a query triggers no re-capture), so `answered` is
    # the ONLY thing that tells a sheet to redraw. Both listen; whichever is not showing that subject
    # rebuilds into the same pixels.
    _drawercompose.set_forecast_query(_forecast_query)
    _bandpanel.set_forecast_query(_forecast_query)
    # …and the OCCUPANTS DRAWER, which renders a launched denial party's `Collapse:` row off the same
    # seam. It is fanned out here rather than in `_drawer`'s own construction for the ordinary reason
    # (`_drawer` is built below, after the controllers it dispatches into), and it re-renders only when
    # the answer is about the party it is showing — a full drawer rebuild on every stepper tick's reply
    # would reflow the card under the player while a compose sheet is being answered.
    _forecast_query.answered.connect(func(subject: String) -> void:
        _drawercompose.refresh_compose_sheet()
        _bandpanel.rerender()
        _drawer.on_forecast_answered(subject))
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
    # AFTER `_connect_zoom_rail()`: that call applies the nav backing's stylebox, hence its padding,
    # hence its minimum size — and this controller's fit gate + declared rail widths are measurements
    # of exactly that. It captures `BottomBar`'s authored minimum height here too.
    _dockrow = DockRowController.new(bottom_bar, nav_backing, turn_orb)
    _setup_tooltip()
    _legend.refresh_rows()
    _refresh_victory_status()
    _telling.render()
    _connect_selection_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    # THE LEFT DOCK IS THE SELECTION CARD'S AGAIN. The command feed that used to sit under it is
    # retired — its events are the event dock's now, and its 40%-of-dock cap existed only to stop it
    # crowding out the card that has the verbs on it.
    # The right dock is the narrative surface's home: the telling owns the top of it and, with both
    # reference cards hidden by default, effectively the whole column.
    right_dock.add(telling_panel, 10)
    right_dock.add(victory_panel, 20)
    right_dock.add(terrain_legend_panel, 30)
    _load_hud_panel_prefs()
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
## action buttons, tint the detail text, and bring the one remaining plain PanelContainer
## (victory) up to the same card chrome the PanelCards already use.
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

func update_victory_state(state: Dictionary) -> void:
    print("[HUD] update_victory_state: ", state.keys())
    victory_state = state.duplicate(true) if state is Dictionary else {}
    _refresh_victory_status()

func update_overlay(turn: int, metrics: Dictionary) -> void:
    # **`metrics` HAS NO READER LEFT, and the parameter stays because `Main` reaches this BY NAME.**
    # It fed the top bar's `Units: N | Logistics: … | Sentiment: …` line, which is retired outright
    # (issue #450) — it named three faction aggregates the player can do nothing with, and nothing
    # replaced it. `Main._hud_invoke` probes this method with `has_method` and a failed probe fails
    # SILENTLY, so the signature is part of the contract even when half of it is unread.
    #
    # The turn is the live half: the orb's face carries it now, and the authoritative snapshot turn
    # drives optimistic-pending reconciliation (`_reconcile_pending`, from `update_band_alerts` later
    # in the same snapshot cycle).
    _band_labor.set_turn(turn)
    _turnorb.set_turn(turn)

## Top-bar faction readouts — thin delegators to the FactionReadouts controller (`_topbar`), which owns
## the Sedentarization / demographics / discoveries / intensification rendering. These
## names stay on HudLayer because Main reaches them by reflection (`_hud_invoke` → has_method+callv).

func update_sedentarization(sedentarization_variant: Variant) -> void:
    _topbar.update_sedentarization(sedentarization_variant)

func update_intensification(intensification_variant: Variant) -> void:
    _topbar.update_intensification(intensification_variant)
    # PUSH the player's knowledge row to MapView, which needs it to decide whether a worked source
    # wears the ⌃ ready mark and holds none of it itself. The knowledge is the ONLY input the map
    # lacks — it already has `forage_patch_lookup` and `herds` — so the push is one small dict rather
    # than a derived mark model, and the derivation stays in `RungGates` where all three surfaces
    # reach it. Mirrors the `labor_pending_changed` → `Main` → `set_labor_pending` path exactly.
    emit_signal("faction_knowledge_changed", _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID))

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

## Resolve the band that assignment/move/clear commands target, in three rungs:
## **selected player unit → the PANEL band → the first player band**.
##
## The middle rung is what makes this correct once the player has more than one band. Selecting a
## HERD or a TILE — which is exactly when a compose sheet opens — leaves `_selection.unit()` empty,
## so the resolver used to fall straight through to `player_band()`, the FIRST player-faction cohort
## captured in `update_band_alerts`. With a second band founded (issue #510) the sheet then composed
## for the PARENT band while the Band/City panel read the colony: every number under it was honest
## and about the wrong band, capping the stepper at the parent's near-exhausted idle workers.
##
## The panel band is the right middle rung because it is the band the player has in FOCUS and it
## survives everything the sheet does: selecting a herd or a tile deliberately leaves it intact (the
## panel persists across selection changes), the faction page deliberately leaves it alone as the
## subject the cycler walks back into, and `refresh_snapshot` re-resolves it against every snapshot.
##
## **It is re-resolved LIVE by entity rather than returned as stored, and the stored dict is never
## returned at all.** `set_panel_band` keeps a deep copy taken at render time, and this answer feeds
## `assignable_hunt_workers` / `assignable_forage_workers` — the very idle counts the steppers cap
## against — so handing that copy back would put a stale-by-one-turn crew under the same steppers this
## exists to fix. And an entity the roster no longer lists is not merely stale, it is a band that is
## GONE: the panel band is only ever set from `player_bands()` and re-resolved by `refresh_snapshot`,
## so a failed lookup means the cohort left the world, and an assignment addressed to it would name a
## band the sim cannot find. The last rung takes that case, as it took every case before.
##
## `{}` when the player has no band at all.
func _resolve_assign_band() -> Dictionary:
    if not _selection.unit().is_empty() and _is_player_unit(_selection.unit()):
        return _selection.unit()
    var panel := _band_labor.panel_band()
    if not panel.is_empty() and _is_player_unit(panel):
        var live := _band_labor.player_band_by_entity(int(panel.get("entity", -1)))
        if not live.is_empty():
            return live
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

## The world's KIT ROSTER and the FOUR job defaults (`docs/plan_denial_raid.md`) — the four compose
## sheets' picker list, ingested once per world onto `_band_labor` (`kits()` / `default_kit_id()`).
## `Main` forwards the wire keys together because they are one fact; a roster whose defaults named
## kits it did not contain would open every picker on an entry it cannot show. Scout and Warrior
## joined the list when the roster gained gear for them — until then the band-wide roles had no kit
## axis and published `""`.
func update_kit_roster(kits_variant: Variant, default_hunt: Variant, default_forage: Variant,
        default_scout: Variant, default_warrior: Variant) -> void:
    _band_labor.set_kit_roster(kits_variant, String(default_hunt), String(default_forage),
        String(default_scout), String(default_warrior))

## The world's CRAFTING CATALOGUES (`docs/plan_crafting_and_materials.md` §7) — the materials, the
## shared rating vocabulary, the recipe book and each faction's craft knowledge. Forwarded by `Main`
## as ONE call for the reason the kit roster is: they are one fact, and a recipe book ingested without
## its materials would render a rail with no craft tracks and costs in materials the panel cannot name.
## They live on `CraftingPanelController` rather than on a state model — one cluster reads them.
func update_crafting_catalogues(materials: Variant, characteristic_bands: Variant,
        recipes: Variant, craft_knowledge: Variant) -> void:
    _crafting.set_catalogues(materials, characteristic_bands, recipes, craft_knowledge)

## Open Materials & Crafting on `band`. Reached BY NAME from the preview harnesses, which stand the
## panel up without a Band/City panel to launch it from.
func open_crafting_panel(band: Dictionary) -> void:
    _crafting.open_for(band)

func close_crafting_panel() -> void:
    _crafting.close()

## The panel's controller, for the harnesses' assertions.
func crafting_panel() -> CraftingPanelController:
    return _crafting

## **THE FORECAST QUERY SEAM, for `Main` to wire the transport into.** `Main` owns the command client,
## so it injects the sender and pumps `CommandBridge.poll_query_replies` in once a frame; nothing in
## the HUD reaches the socket itself. Also the harnesses' handle for driving a canned answer.
func forecast_query() -> ForecastQuery:
    return _forecast_query

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
## the tile's dominant legal plant for me", the same absent-means-default convention the floor has.
##
## `improvement` NEVER reaches the command (issue #442) — `assign_labor` sets the STANCE and the crew
## and deliberately does not touch the second axis; that is what makes a crew edit stop re-asserting a
## build and re-running its gates. It is recorded on the PENDING overlay alone, so an optimistic edit
## keeps showing the build the source is already doing instead of blanking it for a turn.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String,
        floor: float, species: String = "",
        improvement: String = SourceForecast.IMPROVEMENT_NONE,
        kit_id: String = KitRoster.NO_KIT_ID) -> void:
    # TWO handles, and they are not interchangeable. `band_id` is the DURABLE id the command names —
    # the sim resolves a band by it and by nothing else, because ECS entity bits are renumbered by a
    # rollback. `entity` is the CLIENT-LOCAL key the optimistic pending overlay is filed under (every
    # `pending_assigns_for` reader looks a band up by `entity`), so it must not follow the command.
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var entity := int(band.get("entity", -1))
    if band_id == HudConst.NO_BAND_ID or entity < 0:
        return
    var clamped: int = max(0, workers)
    emit_signal("assign_labor_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": band_id,
        "kind": kind,
        "workers": clamped,
        "x": x,
        "y": y,
        "herd_id": herd_id,
        # WHERE THIS CREW STOPS, as a fraction of the source's carrying capacity — the whole of the
        # harvest axis since the four stances were deleted. `Main` renders it as the optional numeric
        # token `assign_labor` takes; the sim REJECTS the four stance words by name, so a stale
        # emitter fails loudly rather than being silently reinterpreted.
        "floor": SourceForecast.clamp_floor(floor),
        "species": species,
        # **THE CREW'S KIT, AND THE DEFAULT IT IS MEASURED AGAINST** (`docs/plan_denial_raid.md`).
        # Both travel, because `Main._kit_token` OMITS the token when the two agree — that is what
        # keeps today's command lines byte-identical where the player named no kit, and the builder
        # cannot know the default on its own (it is world data, not payload data).
        #
        # **IT IS THE HERD'S OWN DEFAULT ON A HUNT ROW, because that is what an ABSENT token means to
        # the sim now** (`equipment.md` → "It is resolved SIM-side"): `handle_assign_labor` resolves
        # `quarry_default_hunt_kit` for a Hunt target. Measuring against the JOB default instead would
        # omit the token for a player who deliberately picked Stalking on a warren, and the sim would
        # then run Trapping — the silent substitution the named path refuses outright, arriving
        # through the absent-token door. `default_kit_for` answers the job default for every other
        # role and for a herd the snapshot does not carry.
        "kit_id": kit_id,
        "default_kit_id": KitRoster.default_kit_for(kind,
            _band_labor.find_world_herd(herd_id), _band_labor.default_kit_id(kind)),
    })
    _band_labor.record_pending_assign(entity, kind, clamped, x, y, herd_id, floor, improvement)
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
    # **`rerender`, NEVER `render_band` — this must re-render whichever SUBJECT the panel is on.**
    # Opening a disclosure re-renders its hosts so the caret can flip ▸→▾, and rendering a BAND here
    # threw the FACTION page away every time one of its own carets was clicked: that page deliberately
    # keeps `panel_band()` intact (it is what the cycler walks back into), so the old guard was
    # satisfied and the panel silently changed subject instead of opening the popover. `rerender` is
    # the routing method that exists for exactly this, and it carries both guards internally.
    _bandpanel.rerender()
    # No `from_selection` here, deliberately: a caret flip is the archetypal PASSIVE re-render, and the
    # drawer's band branch must not re-assert the selected band as the panel's subject on one.
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
## ALL of the player band's currently-idle workers to hunt `herd_id` at the default floor (the food
## peak). A no-op (with a command-feed note) when there's no player band or no idle workers,
## so the shortcut never silently does nothing.
func quick_assign_hunters(herd_id: String) -> void:
    if herd_id.strip_edges() == "":
        return
    var band := _resolve_assign_band()
    if band.is_empty():
        note_system_event("Quick-hunt", "No player band to assign.")
        return
    var idle := int(band.get("idle_workers", 0))
    if idle <= 0:
        note_system_event("Quick-hunt", "No idle workers to assign to %s." % herd_id)
        return
    # The improvement the band is ALREADY building on this herd rides the edit (issue #442): the
    # shortcut sets a crew and a floor, and letting the pending overlay default to `IMPROVEMENT_NONE`
    # would flash a running pen off the work board (and drop the herding-crew floor) for a turn.
    # Double-clicking a herd nobody hunts yet answers "" here, which is the honest value.
    _emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, idle,
        int(band.get("current_x", -1)), int(band.get("current_y", -1)), herd_id,
        SourceForecast.DEFAULT_HARVEST_FLOOR, "",
        _band_labor.improvement_for_hunt(band, herd_id))

func update_overlay_legend(legend: Dictionary) -> void:
    _legend.update(legend)
func get_upper_stack_height() -> float:
    var max_bottom := 0.0
    # **THE TOP-BAR TERMS WENT WITH THE TOP BAR** (issue #450). This measured the `Turn N` / `Units`
    # readouts' bottom edge, and those Labels no longer exist; what is left is the Victory card, which
    # lives in the RIGHT DOCK and is hidden by default — so on an ordinary frame this falls through to
    # the `max_bottom <= 0` floor, which is the honest answer for a HUD with no top furniture at all.
    for label in [victory_status_label]:
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
    _apply_room_rects()
    _refit_floating_cards()

## **A SURFACE THAT COVERS PIXELS WITHOUT TAKING SPACE** — the twin of `set_reserved_inset`, for the
## one kind of neighbour that one cannot express. The event dock overlays the map by design
## (`event-dock.md`): the HUD and the map go on laying out underneath it, so it must NOT inset
## `LayoutRoot` — doing that would push the whole layout down and undo a decision that has nothing to
## do with whoever is colliding with the bar. But a FREE-FLOATING card is not laid out by a
## container; it places itself by arithmetic against a rect, and a rect that ignores the bar puts the
## card's header underneath it (reported in play: the Materials & Crafting title drawn through a
## top-docked event bar).
##
## So the overlay publishes what it covers and only the FREE-FLOATING ROOM shrinks. `size` is the
## depth covered measured from the screen `edge`, absolute — it already includes any displacement
## that pushed the surface inboard — and `size <= 0` releases the overlay, which is what a hidden one
## publishes. Re-registering the same `id` on a different edge MOVES it, exactly as a reservation
## moves, so a bar that flips top→bottom frees the edge it left.
func set_overlay_inset(id: StringName, edge: int, size: float) -> void:
    if size <= 0.0:
        if not _overlays.has(id):
            return
        _overlays.erase(id)
    else:
        var held: Dictionary = _overlays.get(id, {})
        if int(held.get("edge", -1)) == edge and is_equal_approx(float(held.get("size", 0.0)), size):
            return
        _overlays[id] = {"edge": edge, "size": size}
    _recompute_overlays()
    _apply_room_rects()
    _refit_floating_cards()

## **A ROOM THAT CHANGED SHAPE UNDER AN OPEN CARD.** A card already open was placed and sized against
## the OLD rect, and nothing else will move it: a panel can dock, change edge, collapse or be
## released, and the event bar can appear, flip edge, grow a row or be hidden with `R`, at any time —
## none of which is a snapshot. So BOTH writers of `FloatingRoom` end here, and that symmetry is the
## point: the reserved half was missing it, so a card left open while the Band/City panel docked
## stayed fitted to a room that no longer existed and was sliced mid-row by the panel.
##
## **Re-FIT, never re-render.** The payload has not changed, and rebuilding the ledger to answer a
## question about geometry would throw away the player's scroll position.
func _refit_floating_cards() -> void:
    if _crafting != null:
        _crafting.refit_room()

## Write the two rects every other surface measures itself against: `LayoutRoot`, which the docked
## reservations inset and the HUD lays out inside, and `FloatingRoom`, which is that rect pulled
## further off any OVERLAY. One writer, so the two can never be inset from different registries.
func _apply_room_rects() -> void:
    if layout_root != null:
        layout_root.offset_left = _inset_left
        layout_root.offset_top = _inset_top
        layout_root.offset_right = -_inset_right
        layout_root.offset_bottom = -_inset_bottom
    if floating_room != null:
        # `max`, not `+`: both terms are depths from the SAME screen edge, so a reserved strip and an
        # overlay drawn inboard of it overlap rather than stack, and summing them would hold clear a
        # band twice the size of anything on screen.
        floating_room.offset_left = maxf(_inset_left, _overlay_left)
        floating_room.offset_top = maxf(_inset_top, _overlay_top)
        floating_room.offset_right = -maxf(_inset_right, _overlay_right)
        floating_room.offset_bottom = -maxf(_inset_bottom, _overlay_bottom)

## The RIGHT dock's own bottom clearance, in pixels — how far above `ContentRow`'s bottom edge the
## right column's CARDS must stop. `Main` pushes it; `size <= 0` releases it.
##
## **`set_reserved_inset` above cannot express this, which is the whole reason this exists.** That one
## offsets `LayoutRoot` on a whole EDGE, so a bottom reservation shortens both lateral columns across
## the entire window — and on a bottom band dock exactly one of them has to yield. The band card's
## parked chrome (minimap + zoom rail + turn orb) sits at the strip's TRAILING end, flush to the
## screen, which is the same corner the right dock's cards occupy; the LEFT column has nothing in that
## strip and must keep running to the window's bottom edge, that being the clipping the conditional
## inset was written to fix.
##
## **It is a MARGIN on the dock's own `MarginContainer`, not a height on the region.** The region is a
## cell of `ContentRow` and the row writes its rect every layout pass, so a height written here would
## be overwritten; the margin is the container's own declared padding, and shortening `RightScroll`
## through it is what keeps the cards inside — the scroll then scrolls rather than overflowing, so no
## card can draw into the strip however tall its content grows. `right_dock_region`'s RECT is
## untouched, so `lateral_column_widths()` and `right_column_width()` answer exactly as before and no
## clearance can feed back into the yield rule that produced it.
func set_right_column_bottom_clearance(size: float) -> void:
    if right_dock_region == null:
        return
    var clearance: float = maxf(size, 0.0)
    if _right_dock_margin_bottom == RIGHT_DOCK_MARGIN_UNCAPTURED:
        # Captured ONCE, before the first override: a theme constant read back after an override
        # answers the override, so the scene's authored margin has to be taken while it is still the
        # only value there.
        _right_dock_margin_bottom = right_dock_region.get_theme_constant(RIGHT_DOCK_MARGIN_BOTTOM)
    elif is_equal_approx(clearance, _right_column_bottom_clearance):
        return
    _right_column_bottom_clearance = clearance
    right_dock_region.add_theme_constant_override(RIGHT_DOCK_MARGIN_BOTTOM,
        _right_dock_margin_bottom + int(roundf(clearance)))
    _refit_right_dock()

## What the right column is currently holding clear at its bottom. Read by the harnesses, which have no
## `Main` to ask.
func right_column_bottom_clearance() -> float:
    return _right_column_bottom_clearance

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

## Reduce the registered overlays to the deepest one per edge. See `_overlay_left` for why this is a
## maximum where `_recompute_insets` is a sum.
func _recompute_overlays() -> void:
    _overlay_left = 0.0
    _overlay_right = 0.0
    _overlay_top = 0.0
    _overlay_bottom = 0.0
    for overlay in _overlays.values():
        var size: float = float(overlay["size"])
        match int(overlay["edge"]):
            SIDE_LEFT:
                _overlay_left = maxf(_overlay_left, size)
            SIDE_TOP:
                _overlay_top = maxf(_overlay_top, size)
            SIDE_RIGHT:
                _overlay_right = maxf(_overlay_right, size)
            SIDE_BOTTOM:
                _overlay_bottom = maxf(_overlay_bottom, size)

## WORLD BOUNDARY (`Main._reset_per_world_state`): the snapshot about to be applied describes a
## DIFFERENT world, so every HUD cache keyed to the old one is dropped. Coordinator ONLY — each
## module resets ITSELF; nothing but delegation belongs here.
func reset_world_state() -> void:
    _topbar.reset_world_state()
    # The Telling is deliberately NOT reset per snapshot (see `TellingPanel.ingest_events`), and this
    # is the one exception: a world CHANGE is not another snapshot of the same story, it is a
    # different story, and the previous world's beats are not part of it.
    _telling.reset()
    # Targeting addresses a band/tile of the world being replaced. Cancelling through the normal path
    # also clears MapView's reticle (via `targeting_changed`), so the two can't desync.
    cancel_active_targeting()
    # The forecast seam holds answers keyed by band + herd id, and a NEW WORLD REUSES BOTH — band ids
    # restart low and herd ids are derived from species + index — so a held answer matches the new
    # world's composed key exactly and renders the old world's numbers as a live forecast. See
    # `ForecastQuery.reset`.
    _forecast_query.reset()
    # Materials & Crafting is open on a BAND ENTITY, and a new world renumbers entities from the same
    # low range — so a panel left open would silently re-resolve onto a different band's bench and
    # rail. Closing is the honest answer: nothing about the previous world's crafting survives.
    _crafting.close()
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
    # **THE ONE `from_selection` CALLER.** This is the player picking an occupant — a map-marker click,
    # a roster pick relayed from the map, the cycler's own hop through `_select_band_on_map` — so a
    # player band chosen here becomes the Band/City panel's subject even if the faction page is up.
    _render_selection_panel(tile_info, _selection.unit(), {}, true)

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

## The map's select-then-cycle reached the LAND stop of an occupied hex — the third map→HUD
## selection entry point beside `show_unit_selection` / `show_herd_selection`. It takes no payload:
## the `tile_selected` → `show_tile_selection` of the SAME click already seated this hex's
## `tile_info`, which is the whole of the land subject.
##
## `select_land_subject` (not a bare `select_land`) is the load-bearing part — it records the CHOICE
## TILE, so the re-render below and every later `reapply_selection("tile", …)` see a DECIDED hex
## rather than a fresh one and leave the land alone. Without it the auto-pick inside
## `SelectionCardController.render` takes the selection straight back to the first band and the
## cycle's land stop is invisible.
func show_land_selection() -> void:
    # A selection change invalidates the subject being composed (§15).
    close_compose_sheet()
    _selectioncard.select_land_subject()
    _render_selection_panel(_selection.tile_info(), {}, {})

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

## **`from_selection` MARKS THE ONE CALLER WHERE THE PLAYER JUST PICKED THE OCCUPANT.** The drawer's
## player-band branch makes the selected band the Band/City panel's subject, and that must win over a
## standing faction page — but only when it IS a pick. Every other caller here is a RESTATE of the
## same selection (a snapshot's `reapply_selection`, a pending-edit re-render, a deselect), and a
## restate must leave the page alone. See `SubjectDrawerController.render_subject_drawer`.
func _render_selection_panel(_tile_info: Dictionary, _unit_data: Dictionary, _herd_data: Dictionary,
        from_selection: bool = false) -> void:
    if tile_panel == null or tile_detail == null:
        return
    # No tint context is reset here any more: it is no longer a member that outlives a render. Each
    # host below (the drawer, the panel's vitals label) constructs its own `DetailFormat.Context`
    # immediately before it renders, so there is nothing stale for this orchestrator to clear.
    # The identity/list half — roster assembly, tile-card header + chips, auto-select, subject list —
    # lives in the controller (HUD decomposition Phase 2b); the DRAWER stays here (Phase 2c).
    _selectioncard.render(_selection.tile_info())
    _drawer.render_subject_drawer(from_selection)

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

## Injected by Main: the dockable Band/City panel a player band's detail renders into. It goes to the
## dock-row controller too — that one parks the bottom-bar chrome into the panel's single trailing rail
## on a horizontal dock (issue #324).
func set_band_city_panel(panel: BandCityPanel) -> void:
    _bandpanel.set_panel(panel)
    if _dockrow != null:
        _dockrow.set_panel(panel)

## Reflow the bottom-bar chrome into the Band/City panel's row on a horizontal dock (issue #324).
## `Main` probes this BY NAME, so it stays a thin delegator on `HudLayer`.
func reflow_dock_row(edge: int, size: float) -> void:
    if _dockrow != null:
        _dockrow.apply(edge, size)

## WILL the bottom-bar chrome vacate the row for a panel reserving `size` on `edge`? Asked by
## `Main.band_dock_overlays_hud` before it decides whether the HUD may keep a BOTTOM dock's strip: the
## strip is only free of HUD furniture once the chrome has moved into the card's rail. A thin delegator
## for the same reason `reflow_dock_row` is one — `Main` probes it by name.
func bottom_chrome_parks_for(edge: int, size: float) -> bool:
    return _dockrow != null and _dockrow.parks_for(edge, size)

## …and how wide a rail it will ask that card for (0 when it parks nothing). The band card's own
## affordability test has to subtract the chrome column BEFORE the chrome has been pushed, so it takes
## the width from here rather than from whatever the panel was last told.
func bottom_chrome_rail_width(edge: int, size: float) -> float:
    return 0.0 if _dockrow == null else _dockrow.rail_width_for(edge, size)

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

## The Telling's half of the command-event stream. It filters for the kinds it OWNS (the split's one
## definition is `TellingPanel.handles_kind`), and the event dock — which `Main` feeds the same array
## — skips exactly those, so a kind can never be claimed by both surfaces or dropped by both.
##
## This is also the Telling panel's BACKFILL: a full snapshot carries the server's whole
## `commandEvents` ring, so a player opening the client mid-session sees recent history.
func ingest_command_events(events_variant: Variant) -> void:
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
    # 3a. Publish this roster's band NAMES for the event dock (see `band_labels_changed`). Keyed by
    # the durable `band_id` the sim puts in an event's `band=` token, valued with the same
    # `HudFormat.band_display_name` the cycler, the picker and the orb's rows all use — so one band
    # has one name across every surface. Rebuilt each snapshot, so a roster change relabels the
    # dock's already-held rows too.
    var band_labels: Dictionary = {}
    for i in range(player_bands.size()):
        var roster_band: Dictionary = player_bands[i]
        band_labels[str(int(roster_band.get("band_id", -1)))] = HudFormat.band_display_name(roster_band, i + 1)
    band_labels_changed.emit(band_labels)
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
    # Materials & Crafting is a live surface too: its bench progress, its material rail and the
    # ledger's life all move every turn. Refreshed from the same seam as the dock, so the two can
    # never be a turn apart about the same band.
    _crafting.refresh_snapshot()
    # Keep the on-screen allocation panel / assign controls live as the band's staffing
    # changes turn to turn (the coordinator re-renders occupant/tile cards separately, but
    # a herd/tile selection reads _band_labor.player_band(), which only just refreshed here).
    _drawercompose.refresh_drawer_actions()
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject — it must not close on a
    # snapshot, or it would be unusable under autoplay (§15). It closes only if its subject is gone.
    _drawercompose.refresh_compose_sheet()

## HOW WIDE THE HUD'S OWN FURNITURE IS ON EACH SIDE, inside whatever strip the docks have already
## reserved. A horizontal panel that spans the HUD's width draws over these — the event dock's bar
## does exactly that, reported live as a bar sitting on top of the `Turn N` / `Units` / `Pop`
## readouts — so it asks here and stops short of them.
##
## **BOTH ARE AUTHORED, NOT MEASURED, and that is the whole point.** They read
## `custom_minimum_size.x` off the scene's own regions, which no content can move: `PanelDock`
## zeroes the dock stacks' horizontal minimum on construction, so a card cannot widen its column,
## and the top-bar readout block carries an authored minimum of its own for the same reason. A bar
## whose edge tracked a MEASURED width would jump every time the player selected a tile and the
## selection card appeared, or the metrics string gained a digit — the same flicker rule that keeps
## `BandCityPanel`'s reservation content-independent, and worse here, because an event arrives every
## turn. `ui_preview` asserts the live rects never exceed these, so a scene edit that outgrows them
## fails loudly instead of the bar quietly overlapping again.
func left_column_width() -> float:
    return left_dock_region.custom_minimum_size.x if left_dock_region != null else 0.0

## The right side is ONE region now — the right dock. It was the wider authored minimum of a PAIR, the
## dock and the top-bar readout block above it (in Ray's report the readouts were the wider); the block
## is retired with the top bar (issue #450), so there is nothing left to take a `max` against.
func right_column_width() -> float:
    return right_dock_region.custom_minimum_size.x if right_dock_region != null else 0.0

## **THE WIDEST THE RIGHT-HAND COLUMN CAN EVER GET, and the ONE bound a rule may reason FORWARD from.**
##
## `right_column_width()` above is what the scene RESERVES; `lateral_column_widths()` below is what the
## column currently OCCUPIES; and where a live readout outgrows its authored minimum the two disagree.
## That gap is not cosmetic, because two different consumers ask the two different questions about the
## same column: `Main.band_dock_overlays_hud` decides whether the HUD keeps its bottom strip from the
## authored pair (it must — a live read there would depend on the rule's own output, the cycle
## `348e5c09` sabotage-verified), and the card is then placed against the live pair. Every pixel of
## daylight is a band of window widths in which the HUD keeps its strip believing the card can afford
## the wide shell while the card, paying the larger bound, collapses to the narrow tabbed shell — the
## exact trade that rule exists to REFUSE. Measured with a 344 authored minimum against a 419 live
## readout: a stable, reproducible 75px band (logical widths 2215-2289) in which every bottom dock
## rendered the tabbed shell over a HUD that had kept its columns.
##
## So the rule reads a CEILING instead: a constant no live content can exceed, which keeps the answer
## acyclic and jitter-free (both properties the live pair lacks) while making it CONSERVATIVE — where
## it says the card can afford the bounds, the card really can, with the ceiling's own slack to spare.
##
## **IT IS DERIVED FROM THE SCENE'S OWN AUTHORED WIDTHS, WHICH IS WHY IT CAN BE EXACT.** It was `561.0`
## for a long time, and that number measured a surface that no longer exists: the top-bar KNOWLEDGE
## STRIP's first row (`⚒ Your people know:` plus two in-progress tracks with meters and percents), the
## widest line the retired readouts could produce. `TurnBlock`, `TopBar` and all eight of their Labels
## are gone from `HudLayer.tscn` (issue #450), so nothing in the client renders that line, and 561
## carried ~209px of headroom that measured nothing at all.
##
## **What the column IS now is the RIGHT DOCK alone, and it stacks in exactly three terms:**
##
## | term | what it is |
## |---|---|
## | `RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH` | the widest AUTHORED card minimum in `RightStack` — `TellingPanel.custom_minimum_size.x` |
## | `RIGHT_DOCK_SCROLLBAR_SPAN` | `RightScroll`'s vertical scrollbar, which its own minimum width includes while the stack overflows |
## | `RIGHT_DOCK_MARGIN_SPAN` | `RightDock`'s authored horizontal margins, `margin_left` 8 + `margin_right` 16 |
##
## `PanelDock._configure_scroll` disables HORIZONTAL scrolling on `RightScroll` and zeroes the stack's
## horizontal minimum, so the scroll's minimum width is its widest visible CARD's minimum plus its
## scrollbar, and the region is that plus its margins. The first and third terms are also exactly the
## `344` authored on `RightDock.custom_minimum_size.x` — i.e. the reservation is this derivation
## MINUS the scrollbar, which is the whole of the gap between `right_column_width()` and this.
##
## **THE COLUMN IS CONTENT-DERIVED ABOVE THAT FLOOR, AND THE SWEEP SAYS WHERE THE CONTENT STOPS.**
## `lateral_column_widths().y` read **344** in every one of `band_panel_preview`'s 84 states and every
## one of `ui_preview`'s 274, at every viewport those harnesses stage (1280→2560 logical) and at
## `ui_scale` 1.0 and 1.35 alike — the scrollbar is absent until the stack overflows. Staged at the
## widest content the dock can hold (the Victory card beside a Terrain Types legend long enough to
## reach `LegendController.LEGEND_MAX_HEIGHT`) it reads **352**, which is this derivation exactly.
## Measured beside the Telling panel's 320 in that state: the Victory card's minimum is **50** and the
## legend's **228**, so the Telling panel's authored minimum is the binding term and the other two are
## nowhere near it.
##
## **A HARNESS SWEEP BOUNDS THE FIXTURES, NOT THE SNAPSHOT — so the content paths were probed
## individually, and all but one are structurally bounded.** A legend row's label is a plain `Label`
## whose minimum IS its text width, which looks like an unbounded path and is not: `LegendScroll`
## leaves horizontal scrolling on `AUTO`, so a row's width never reaches the card. The Victory card's
## `RichTextLabel` sets no `fit_content`, so its minimum ignores its text. Probed with pathological
## content — 11 rows of 75-character terrain names, and a 120-character unbroken victory string — the
## column did not move off 352.
##
## **THE ONE PATH THAT COULD EXCEED IT WAS A CARD TITLE, AND IT IS NOW BOUNDED AT THE CARD.**
## `PanelCard._header` was a `RichTextLabel` with `fit_content = true` and `AUTOWRAP_OFF`, which
## reports its full unwrapped text width as a minimum on BOTH axes with no per-axis switch — so a
## title was a hard minimum on its card and widened the whole column. Probed against the widest
## staging: a 58-character legend title took the column to **489**, i.e. 137px past this ceiling, and
## a margin cannot bound a string (an unexplained pad is exactly what 561 became). The header is a
## `Label` now, with `clip_text` and `OVERRUN_TRIM_ELLIPSIS`, so it reports a ~zero width minimum and
## trims instead; the same probe reads **352** after. **Nothing the player can see was traded for
## that**: every title the client can actually author was swept through the real card — the legend's
## `Terrain Types` / `Terrain Tags` / `Provinces` / `No Overlay` plus all thirteen overlay-channel
## labels the native decoder ships, the longest being `Forage (Human Food Capacity)` — and the widest
## leaves the legend card at **253**, 67px BELOW the Telling panel's 320, so the ellipsis never
## engages on a real title and the title term contributes nothing to this derivation at all.
##
## **THE DANGEROUS DIRECTION IS DOWN.** Too high only makes `affords_wide_shell_with_bounds`
## conservative — it refuses the wide shell in windows where the card would have fitted, costing layout
## quality. Too low means a column that renders wider than the bound gets DRAWN THROUGH by the card.
## So when a right-dock card is authored wider than 320, the fix is to raise
## `RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH`, not to pad this. (A long TITLE is no longer one of the ways
## that happens — see above — so the only thing that can move this number is an authored minimum.)
##
## **NOTHING CHARGES IT TODAY, so the retune moved no behaviour.** Its one consumer is
## `Main.band_dock_overlays_hud`, which reaches `affords_wide_shell_with_bounds` only on a **BOTTOM**
## dock (a top dock answers `true` before that line, every other edge `false`) — and
## `BandCityPanel._trailing_bound_for` charges a bottom dock NO trailing bound, so the number is passed
## in and discarded. The fork is `wide_shell_min_width() + rail span + the LEADING ceiling` and does not
## contain this term at all.
##
## **The guard that can see an overrun does not depend on the value**:
## `band_panel_preview._assert_ceilings_cover_the_widest_right_column` stages the dock at its widest and
## asserts the ceiling still covers what the column occupies, so a card that outgrew it fails the run
## rather than silently re-opening the band the day something charges this again. It reads `352 / 352`
## now, where against 561 it passed with 209px of slack and tested nothing.
const RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH := 320.0
const RIGHT_DOCK_SCROLLBAR_SPAN := 8.0
const RIGHT_DOCK_MARGIN_SPAN := 24.0
const RIGHT_COLUMN_CEILING := RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH \
    + RIGHT_DOCK_SCROLLBAR_SPAN + RIGHT_DOCK_MARGIN_SPAN

## The `maxf` cannot bind while the derivation above holds — the constant is the reservation PLUS the
## scrollbar, so it is the larger of the two by construction. It stays as the floor guard for the case
## that breaks that: a scene edit that raises `RightDock.custom_minimum_size.x` without raising
## `RIGHT_DOCK_WIDEST_CARD_MIN_WIDTH` with it, where a ceiling under its own reservation would be the
## dangerous direction.
func right_column_ceiling() -> float:
    return maxf(right_column_width(), RIGHT_COLUMN_CEILING)

## …and the leading column's ceiling, which needs no constant of its own: the left dock's cards are
## authored to its own `custom_minimum_size.x` and measure exactly that live, so the reservation IS the
## ceiling there. It exists as a named pair with `right_column_ceiling` so the rule reads one concept
## on both ends, and so a left column that ever outgrows its minimum has an obvious home rather than a
## call site to rewrite.
func left_column_ceiling() -> float:
    return left_column_width()

## The room the two HUD side columns actually occupy, as `(leading, trailing)` — what a panel sharing
## the HUD's strip must keep clear of (issue #377, `Main._update_band_panel_lateral_bounds`).
##
## **It takes the LIVE rect where that exceeds the authored minimum, unlike `left_column_width` /
## `right_column_width` above, and the difference is deliberate.** Those two bound the EVENT DOCK's edge,
## which must not move every turn — so they are authored, and a column that draws wider than its minimum
## merely overlaps a little. This bound decides whether a CARD is drawn THROUGH a column, where being a
## little wrong is not cosmetic.
##
## **A RULE MUST NOT REASON FORWARD FROM THIS.** It is a measurement of the moment, so it disagrees
## with `right_column_width` whenever a live line exceeds its reservation — and a decision that reads
## one while the card is placed against the other is wrong across a whole band of window widths (see
## `right_column_ceiling`, which exists for exactly that caller and bounds this from above).
##
## **The case that made it live is gone, and the rule is not.** It was the top-bar readouts: measured at
## 1920 they rendered 419px against a 344px authored minimum, because `Units: 0 | Logistics: 0.00 |
## Sentiment: 0.00` is simply longer than the minimum allows for, so an authored bound put the card
## straight through them. That block is retired (issue #450) and the surviving regions are the two
## DOCKS, whose stacks have their horizontal minimum zeroed by `PanelDock` — so today the live read
## rarely exceeds the authored one. Keep it live anyway: a card is what this bounds, a dock card that
## outgrows its column is the same failure one surface along, and the band card re-lays-out per
## snapshot regardless so tracking the live width costs it nothing.
func lateral_column_widths() -> Vector2:
    var lead: float = left_column_width()
    if left_dock_region != null:
        lead = maxf(lead, left_dock_region.get_global_rect().size.x)
    var trail: float = right_column_width()
    if right_dock_region != null:
        trail = maxf(trail, right_dock_region.get_global_rect().size.x)
    return Vector2(lead, trail)

## A CLIENT-SIDE note — a refusal, a nudge, a knowledge unlock. It used to land in the left-dock
## command feed; it is a System-channel event on the event dock now, which is `Main`'s panel, so the
## HUD EMITS rather than reaching for it. `_note_sink` is the Callable the three controllers that
## post these were handed in place of the retired feed.
func note_system_event(label: String, detail: String) -> void:
    system_note_requested.emit(label, detail)
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

# ---- dock-card visibility persistence --------------------------------------

func _load_hud_panel_prefs() -> void:
    var cfg := ConfigFile.new()
    if cfg.load(NarrativeForkPanel.config_path()) == OK:
        if _legend != null:
            _legend.set_suppressed(bool(cfg.get_value(
                HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_LEGEND_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT)))
        _victory_suppressed = bool(cfg.get_value(
            HUD_PANELS_CONFIG_SECTION, CONFIG_KEY_VICTORY_SUPPRESSED, PANEL_SUPPRESSED_BY_DEFAULT))
    else:
        # No prefs file yet (or unreadable): fall back to the hidden-by-default layout.
        if _legend != null:
            _legend.set_suppressed(PANEL_SUPPRESSED_BY_DEFAULT)
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

