extends CanvasLayer
class_name HudLayer

## Map-zoom rail (bottom-left nav cluster). `map_zoom_step` carries +1 (in) / -1 (out);
## `map_zoom_fit` fits the map to the view. Main wires both to the single MapView zoom path.
signal map_zoom_step(direction: int)
signal map_zoom_fit
## Emitted when the player clears ALL of a band's labor assignments (the "Clear all"
## affordance); carries the band dict so Main can extract faction + entity bits for the
## repurposed `cancel_order` command (now a clear-all → fully idle).
signal cancel_order_requested(band: Dictionary)
## Early-Game Labor (docs/plan_early_game_labor.md, slice 3b): assign/unassign
## working-age workers to a source or band-wide role. Payload keys:
## { faction, band, kind ("forage"|"hunt"|"scout"|"warrior"), workers,
##   x, y (forage/hunt readout), herd_id, policy (hunt) }. Main formats the
## `assign_labor …` text command. workers==0 removes/zeroes the assignment.
signal assign_labor_requested(payload: Dictionary)
## Emitted after the player picks a destination tile for the selected band's move.
## Payload keys: { faction, band, x, y }. Main formats the `move_band …` command.
signal move_band_requested(payload: Dictionary)
## Scouting expedition (docs/plan_exploration_and_sites.md §2). Sent after the player outfits a
## party on a resident band (a party-size stepper) and clicks a target tile. Payload keys:
## { faction, band, party_workers, x, y }. Main formats the `send_expedition …` command.
signal send_expedition_requested(payload: Dictionary)
## Hunting expedition (docs/plan_exploration_and_sites.md §2b). Sent after the player outfits a party
## on a resident band and clicks a target herd. Payload keys: { faction, band, party_workers,
## fauna_id }. Main formats the `send_hunt_expedition …` command.
signal send_hunt_expedition_requested(payload: Dictionary)
## Emitted when the player recalls the selected in-flight expedition (folds it home). Payload
## keys: { faction, expedition }. Main formats the `recall_expedition …` command.
signal recall_expedition_requested(payload: Dictionary)
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

## Build identifier of THIS client (GDScript/native). **Bump on client-affecting
## changes.** Shown in the lower-left version overlay next to the server build (streamed
## in the snapshot header) so the running client+server builds can be confirmed at a
## glance. Format: `YYYY-MM-DD.N`.
const CLIENT_BUILD := "2026-07-10.3"
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var sedentarization_label: Label = %SedentarizationLabel
@onready var demographics_label: Label = %DemographicsLabel
@onready var discoveries_label: Label = %DiscoveriesLabel
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
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var tile_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/TilePanel as PanelCard
@onready var tile_detail: RichTextLabel = %TileDetail
@onready var occupants_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/OccupantsPanel as PanelCard
@onready var occupant_detail: RichTextLabel = %OccupantDetail
@onready var roster_list: VBoxContainer = %RosterList
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
@onready var turn_orb: TurnOrb = $LayoutRoot/RootColumn/BottomBar/TurnCluster
@onready var minimap_container: MarginContainer = $LayoutRoot/RootColumn/BottomBar/NavBacking/NavCluster/MinimapContainer
@onready var resource_summary: MarginContainer = $LayoutRoot/RootColumn/BottomBar/ResourceSummary
@onready var resource_hbox: HBoxContainer = $LayoutRoot/RootColumn/BottomBar/ResourceSummary/ResourceHBox
@onready var resource_placeholder: Label = $LayoutRoot/RootColumn/BottomBar/ResourceSummary/ResourceHBox/ResourcePlaceholder

var tooltip_panel: PanelContainer
var tooltip_label: Label

const LEGEND_SWATCH_FRACTION := 0.75
const LEGEND_MIN_ROW_HEIGHT := 20.0
const LEGEND_ROW_PADDING := 6.0
const LEGEND_MAX_HEIGHT := 640.0
const STACK_ADDITIONAL_MARGIN := 16.0
const COMMAND_FEED_LIMIT := 6
# The feed grows to fit its entries, but never past the space left in the dock
# below the panels above it: past that it scrolls internally instead of pushing
# the whole dock to scroll. Genuinely short content still shrinks to fit (no
# empty box). MIN_HEIGHT is a floor on that available-space limit only, so a
# cramped dock still leaves the feed usable rather than collapsing it to nothing.
const COMMAND_FEED_MIN_HEIGHT := 72.0
const COMMAND_FEED_BOTTOM_MARGIN := 12.0
const PLAYER_FACTION_ID := 0
# Turn-orb attention contract (see TurnOrb.gd). The folded-in Alerts panel became
# three producers here: starving (critical), losing_population (warn), idle_workers (warn).
const ATTENTION_KIND_STARVING := "starving"
const ATTENTION_KIND_LOSING_POPULATION := "losing_population"
const ATTENTION_KIND_IDLE_WORKERS := "idle_workers"
const ATTENTION_SEVERITY_CRITICAL := "critical"
const ATTENTION_SEVERITY_WARN := "warn"
# Top-bar glyph for the discovered-Wondrous-Sites readout (a faceted-gem marker).
const DISCOVERIES_GLYPH := "◈"
const FOOD_MODULE_LABELS := {
    "coastal_littoral": "Coastal Littoral",
    "riverine_delta": "Riverine Delta",
    "savanna_grassland": "Savanna Grassland",
    "temperate_forest": "Temperate Forest",
    "boreal_arctic": "Boreal Arctic",
    "montane_highland": "Montane Highland",
    "wetland_swamp": "Wetland Swamp",
    "semi_arid_scrub": "Semi-Arid Scrub",
    "coastal_upwelling": "Coastal Upwelling",
    "mixed_woodland": "Mixed Woodland",
}
const FOOD_ACTION_FORAGE := "forage"
const FOOD_ACTION_HUNT := "hunt"
# Band-status alert types, ordered high → low priority (rendered in this order).
const BAND_ACTIVITY_IDLE := "idle"
# Verb prefixes for the optimistic in-flight label on the disabled cancel button,
# composed with the task action phrase as "<verb> <phrase>…" (e.g. "Cancelling
# Market Hunt…", "Starting Foraging…"). Shown from dispatch until the snapshot
# confirms the band's `activity` CHANGED from its value at dispatch.
const CANCEL_ORDER_PENDING_VERB := "Cancelling"
const START_ORDER_PENDING_VERB := "Starting"
# Why a band is losing population — appended to the losing_population alert label.
const DECLINE_REASON_STARVING := "starving"
const DECLINE_REASON_LOW_MORALE := "low morale"
# Morale-driven loss is now emigration/relocation (people don't die of low morale —
# see docs/plan_civ_wellbeing.md), so a shrink with emigrants last turn reads this.
const DECLINE_REASON_PEOPLE_LEAVING := "people leaving"
# Per-cohort morale cause (snapshot PopulationCohortState.moraleCause; 0 = None).
const MORALE_CAUSE_NONE := 0
const MORALE_CAUSE_TERRAIN := 1
const MORALE_CAUSE_COLD := 2
const MORALE_CAUSE_UNREST := 3
# Plain-language cause labels, shared by the drawer morale line and the alert reason.
# Cold reads "harsh climate" because the server penalty fires on hot OR cold deviation.
const MORALE_CAUSE_LABEL_TERRAIN := "harsh terrain"
const MORALE_CAUSE_LABEL_COLD := "harsh climate"
const MORALE_CAUSE_LABEL_UNREST := "unrest"
# Morale-trend arrow glyphs; |morale_delta| below this (0.5%/turn) reads as flat (no
# arrow), so trivial drift — nearly every tile bleeds a hair today — isn't shown as a decline.
const MORALE_TREND_EPSILON := 0.005
const MORALE_TREND_FALLING_GLYPH := "▼"
const MORALE_TREND_RISING_GLYPH := "▲"
# Civilization Wellbeing (docs/plan_civ_wellbeing.md). Productivity readout: output is the
# modifier-stack result (1.0 = full); the Output row only appears below full, tinted by the
# output.{warn,critical} buckets in BandFoodStatus.
const OUTPUT_FULL := 1.0
# Itemized morale breakdown — the four signed Layer-1 contributions (their sum IS
# morale_delta) rendered as indented sub-lines under the Morale headline when morale is
# concerning or declining. Tinted by sign (▲ positive = healthy, ▼ negative = amber).
const MORALE_BREAKDOWN_INDENT := "    "
const MORALE_CONTRIB_POSITIVE_GLYPH := "▲"
const MORALE_CONTRIB_NEGATIVE_GLYPH := "▼"
const MORALE_CONTRIB_LABEL_SETTLING := "settling"
# Positive unrest contribution reads as "culture" (cohesion), negative as "unrest".
const MORALE_CONTRIB_LABEL_CULTURE := "culture"
# Recovery guidance — a dim line naming the real levers (NOT harvest) when morale is concerning.
const RECOVERY_GUIDANCE_GLYPH := "↑"
const RECOVERY_GUIDANCE_TEXT := "↑ Recover: move to Hospitable ground · Scout · Hunt"
# Positive-lever morale hints on the action buttons (tooltip suffixes).
const MORALE_HINT_SCOUT := "Scout unknown ground — reveals nearby tiles and lifts the band's spirits (+morale)."
const MORALE_HINT_PERSISTENT := "  Hunting a herd also lifts morale each turn (+morale/turn)."
# Occupants roster row chrome.
const ROSTER_DOT_SIZE := 9.0
const ROSTER_ROW_MIN_HEIGHT := 30.0
const ROSTER_ROW_SEPARATION := 8
const ROSTER_ROW_H_PADDING := 10.0
const ROSTER_ACCENT_WIDTH := 3.0
const ROSTER_HEADER_FONT_SIZE := 10
# Per-activity glyph for a player band's roster row. `activity` is the kind with the
# most workers (Early-Game Labor): idle | forage | hunt | scout | warrior. `harvest` /
# `follow` are retained for older snapshots but the live enum emits forage/hunt.
const ACTIVITY_GLYPHS := {
    "idle": "·",
    "forage": "🌾",
    "harvest": "🌾",
    "hunt": "🏹",
    "follow": "🦌",
    "scout": "🧭",
    "warrior": "🛡",
}
# Provisions is the food item under a band's larder `stores`.
const STORE_ITEM_PROVISIONS := "provisions"
const FOOD_UNLIMITED_GLYPH := "∞"
const UI_BALANCE_CONFIG_PATH := "res://src/config/ui_balance.json"
const DEFAULT_TRAVEL_SPEED := 3.0
const DEFAULT_TRAVEL_PREVIEW_LIMIT := 12
var overlay_legend: Dictionary = {}
var legend_suppressed: bool = false
var localization_store = null
var campaign_label: Dictionary = {}
var victory_state: Dictionary = {}
var _command_feed_entries: Array = []
var _command_feed_signatures: Dictionary = {}
# Previous per-band size (entity id -> size) so we can detect population loss
# across snapshots for the "losing population" alert.
var _prev_band_sizes: Dictionary = {}
var _selected_tile_info: Dictionary = {}
var _selected_unit: Dictionary = {}
var _selected_herd: Dictionary = {}
# The assembled Occupants roster for the current hex: full unit markers and herd
# dicts (from `_selected_tile_info`, plus the selected occupant if the tile_info
# doesn't list it — e.g. an inspector-driven herd selection). Rebuilt each render.
var _roster_units: Array = []
var _roster_herds: Array = []
var _selected_food_module: String = ""
var _selected_food_is_hunt: bool = false
# Days-of-food of the currently-selected band's larder, so the detail formatter
# can threshold-tint the Food row. NAN when no band is selected.
var _selected_band_food_days: float = NAN
# Morale (0–1) of the currently-selected player band, so the detail formatter can
# threshold-tint the Morale row. NAN when no player band is selected.
var _selected_band_morale: float = NAN
# Output multiplier (0–1) of the currently-selected player band, so the detail formatter
# can threshold-tint the Output row. NAN when no band with a below-full output is selected.
var _selected_band_output: float = NAN
# Early-Game Labor (docs/plan_early_game_labor.md, slice 3b). Assignment kinds mirror
# the sim's LaborAssignment.kind; the source-centric allocation targets the single
# player band captured from each snapshot (there is exactly one player band today).
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"
const LABOR_KIND_SCOUT := "scout"
const LABOR_KIND_WARRIOR := "warrior"
# Hunt take policies a labor Hunt assignment can carry (no "single" one-shot anymore).
const LABOR_HUNT_POLICIES := ["sustain", "surplus", "market", "eradicate"]
const DEFAULT_HUNT_POLICY := "sustain"
# One worker per −/+ stepper press.
const WORKER_STEP := 1
# Leading label on the assign controls' band-picker dropdown ("which band supplies the workers").
const BAND_PICKER_LABEL := "Band:"
# Worker-stepper row chrome: the fixed-width −/+ buttons, the centered count column,
# and the row separation.
const WORKER_STEPPER_BUTTON_WIDTH := 28.0
const WORKER_STEPPER_VALUE_WIDTH := 32.0
const WORKER_STEPPER_SEPARATION := 6
# Allocation-panel section headers + role hints (make the panel read as a "current actions"
# report and make the standing Scout/Warrior roles discoverable — the −/+ steppers ARE how
# you staff a scout mission now; there is no targeted map action).
const ALLOC_SECTION_FONT_SIZE := 10
const ALLOC_HEADER_ACTIONS := "Current actions"
const ALLOC_HEADER_ROLES := "Band roles"
const ALLOC_NO_SOURCES_HINT := "No sources worked yet — select a tile or herd to assign foragers/hunters."
const SCOUT_ROLE_HINT := "Posts scouts that see around obstacles — more scouts range farther. Staff with −/+."
const WARRIOR_ROLE_HINT := "Guards the band — matters once threats arrive."
# Scouting expedition (docs/plan_exploration_and_sites.md §2). A detached party is a cohort
# tagged Expedition flowing through the same populations[] array as a band; it carries no labor
# in v1, so its drawer shows a dedicated mission/phase/party/provisions readout + Recall/Move
# instead of the labor-allocation panel. The outfit affordance (party stepper + send) lives on a
# resident band's allocation panel.
const EXPEDITION_MISSION_SCOUT := "scout"
const EXPEDITION_MISSION_HUNT := "hunt"
const EXPEDITION_PHASE_AWAITING := "awaiting"
const EXPEDITION_PHASE_RETURNING := "returning"
const EXPEDITION_MISSION_LABELS := {
	"scout": "Scouting expedition",
	"hunt": "Hunting expedition",
}
const EXPEDITION_PHASE_LABELS := {
	"outbound": "Outbound",
	"awaiting": "Awaiting orders",
	"returning": "Returning",
	"hunting": "Hunting",
	"delivering": "Delivering",
}
const SEND_EXPEDITION_TITLE := "Send scouting expedition"
const SEND_EXPEDITION_HINT := "Detach a party to scout distant territory, then click a target tile."
const SEND_EXPEDITION_BUTTON := "Send scouting expedition…"
# Hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party that follows a
# migratory herd, accumulates food, and drops it at the band. Launched from a resident band by
# picking a herd (herd-target click, not a tile), and Recalled like a scout expedition.
const SEND_HUNT_EXPEDITION_TITLE := "Send hunting expedition"
const SEND_HUNT_EXPEDITION_HINT := "Detach a party to follow a migratory herd, then click on the herd."
const SEND_HUNT_EXPEDITION_BUTTON := "Send hunting expedition…"
# Distance-aware herd-hunt affordance (docs/plan_exploration_and_sites.md §2b): clicking a herd
# offers a LOCAL hunt when it's within the SELECTED band's hunt_reach, or a hunting EXPEDITION when
# it's beyond. One compose control (worker/party stepper + policy), two labels/commands keyed off the
# wrap-aware hex distance from the selected band's own tile.
const ASSIGN_LOCAL_HUNT_BUTTON := "Assign Local Hunt"
const SEND_HUNTING_EXPEDITION_BUTTON := "Send Hunting Expedition"
# Range-aware forage assign: foraging is stationary gathering (NO expedition fallback), so a tile
# beyond the selected band's `work_range` disables the button rather than offering an alternative.
const FORAGE_ASSIGN_BUTTON := "Forage"
# Generic section header for the outfit block (hosts both the scout + hunt send verbs).
const SEND_EXPEDITION_SECTION := "Send expedition"
# The hunt party's carry-ceiling FULL badge (shown in the hunt panel when carried ≥ cap).
const HUNT_FULL_BADGE := "· FULL"
# The launch policy (Sustain/Surplus/Market/Eradicate) chosen for a hunting expedition, with a
# one-line behaviour hint so the choice is legible. Reuses `LABOR_HUNT_POLICIES` for the option set.
const SEND_HUNT_POLICY_HINTS := {
	"sustain": "Sustain — one conservative harvest; the herd stays healthy.",
	"surplus": "Surplus — one full haul.",
	"market": "Market — repeated trips; grinds the herd down.",
	"eradicate": "Eradicate — hunt to extinction; no food (denial).",
}
# Suffix marking an optimistic (not-yet-confirmed) allocation row, tinted amber to tie it to
# the amber pending hex on the map.
const PENDING_ROW_SUFFIX := "  · pending"
# The single player band, captured from the latest snapshot populations (there is exactly
# one player band in the current start). assign_labor / move_band / clear-all target it; the
# herd/tile assign controls also read its labor_assignments to show the current staffing.
var _player_band: Dictionary = {}
# Every player-faction band from the latest snapshot (in roster order; first == _player_band).
# The assign controls' band-picker dropdown lists these so an assignment explicitly names WHICH
# band supplies the workers. One entry today (multi-band split is deferred), but built for N.
var _player_bands: Array = []
# Map grid dimensions (width/height/horizontal-wrap), captured each snapshot from the `grid` key
# (Main forwards it via set_grid_dimensions). Feed the wrap-aware hex distance the herd-hunt
# affordance keys its LOCAL-hunt-vs-hunting-EXPEDITION decision off. Grid rides full snapshots only;
# it persists across deltas. Defaults to no-wrap until the first snapshot.
var _grid_width: int = 0
var _grid_height: int = 0
var _grid_wrap_horizontal: bool = false
# The authoritative snapshot turn (header tick), set from update_overlay each snapshot. Used
# to reconcile optimistic pending actions (a newer turn means the server has processed them).
var _current_turn: int = -1
# Optimistic pending labor actions per band entity (Early-Game Labor slice 3b UX): assign/move
# clicks show immediately in the panel AND on the map, then reconcile when a newer-turn snapshot
# confirms them (the snapshot is authoritative — this cleanly absorbs server-side clamping).
#   _pending_labor[entity] = {
#     "turn": <snapshot turn at issue>,
#     "assign": { key -> {kind, workers, x, y, herd_id, policy} },   # key via _pending_key
#     "move": {x, y},                                                # optional
#   }
var _pending_labor: Dictionary = {}
# Move-band targeting: the pending band-relocation tile pick. {} when inactive. Holds the
# band dict whose move is being targeted.
var _pending_move_band: Dictionary = {}
# Send-expedition targeting: the pending expedition-launch tile pick. {} when inactive. Holds the
# resident band being outfitted plus the chosen party size (mirrors `_pending_move_band`).
var _pending_send_expedition: Dictionary = {}
# Send-hunt-expedition targeting: the pending hunt-launch HERD pick (the click resolves to a
# huntable herd on the clicked hex, not a tile). {} when inactive.
var _pending_send_hunt_expedition: Dictionary = {}
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = WORKER_STEP
# Compose state for the hunt-expedition launch policy (Sustain/Surplus/Market/Eradicate).
var _send_hunt_policy: String = DEFAULT_HUNT_POLICY
# Compose state for the herd/tile "Assign" controls — the in-progress worker count (and, for
# hunts, the policy) the player is dialing before pressing Assign. Keyed to the source so a
# per-snapshot re-render preserves the count, but re-initializes from current staffing when
# the selected source changes.
var _forage_assign_key: String = ""
var _forage_assign_count: int = 0
var _hunt_assign_key: String = ""
var _hunt_assign_count: int = 0
var _hunt_assign_policy: String = DEFAULT_HUNT_POLICY
# The band-picker selection (actor band entity) for each assign control, persisted across the
# per-snapshot re-renders of the same source. Re-defaults to _resolve_assign_band() when the
# selected source changes; -1 means "fall back to the resolved band".
var _hunt_assign_band: int = -1
var _forage_assign_band: int = -1
var _targeting_banner: PanelContainer = null
var _targeting_banner_label: RichTextLabel = null
var _stockpile_totals: Dictionary = {}
var travel_tiles_per_turn: float = DEFAULT_TRAVEL_SPEED
var travel_preview_turn_cap: int = DEFAULT_TRAVEL_PREVIEW_LIMIT
var left_dock: PanelDock
var right_dock: PanelDock
# Edges reserved by docked panels (Inspector, Band/City panel). Each reserver
# registers a (edge, size) contribution keyed by a StringName id; the whole HUD
# insets by the summed per-edge totals.
var _reservations: Dictionary = {}
var _inset_left: float = 0.0
var _inset_right: float = 0.0
var _inset_top: float = 0.0
var _inset_bottom: float = 0.0

func _ready() -> void:
    _load_ui_balance_config()
    _connect_zoom_rail()
    _connect_turn_orb()
    _setup_tooltip()
    _refresh_existing_legend_rows()
    _resize_legend_panel(_legend_list_size())
    _refresh_campaign_label()
    _refresh_victory_status()
    _render_command_feed()
    _connect_selection_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    left_dock.add(occupants_panel, 12)
    left_dock.add(stockpile_panel, 20)
    left_dock.add(command_feed_panel, 30)
    right_dock.add(victory_panel, 10)
    right_dock.add(terrain_legend_panel, 20)
    if stockpile_panel != null:
        stockpile_panel.visible = false
    if stockpile_title != null:
        stockpile_title.text = "Stockpiles"
    _apply_hud_style()
    _ensure_targeting_banner()
    _setup_build_overlay()

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
    if stockpile_panel != null:
        stockpile_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    if victory_panel != null:
        victory_panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())

## Floating targeting banner, pinned to the top-centre of the map. Shown only
## while a command is choosing its target; it names the command + what to click
## next and offers Cancel. This is the primary targeting feedback — it replaces
## the easy-to-miss "select a band…" line buried in the selection panel.
func _ensure_targeting_banner() -> void:
    if _targeting_banner != null:
        return
    var center := CenterContainer.new()
    center.name = "TargetingBannerCenter"
    center.anchor_left = 0.0
    center.anchor_right = 1.0
    center.anchor_top = 0.0
    center.anchor_bottom = 0.0
    center.offset_top = 12.0
    # Anchored to the top edge with zero anchored height; grow downward so the
    # container takes its child's (the banner's) height instead of a 0/negative
    # rect that could clip it.
    center.grow_vertical = Control.GROW_DIRECTION_END
    center.mouse_filter = Control.MOUSE_FILTER_IGNORE
    layout_root.add_child(center)

    var banner := PanelContainer.new()
    banner.name = "TargetingBanner"
    banner.add_theme_stylebox_override("panel", HudStyle.banner_stylebox())
    banner.visible = false
    center.add_child(banner)

    var hbox := HBoxContainer.new()
    hbox.add_theme_constant_override("separation", 12)
    banner.add_child(hbox)

    var reticle := Label.new()
    reticle.text = "⌖"  # ⌖ target reticle
    reticle.add_theme_color_override("font_color", HudStyle.SIGNAL)
    reticle.add_theme_font_size_override("font_size", 20)
    reticle.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    hbox.add_child(reticle)

    var label := RichTextLabel.new()
    label.name = "TargetingLabel"
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_OFF
    label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    hbox.add_child(label)

    var cancel := Button.new()
    cancel.text = "Cancel  (Esc)"
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(cancel_active_targeting)
    hbox.add_child(cancel)

    _targeting_banner = banner
    _targeting_banner_label = label

## Recompute targeting state from the pending flows, update the banner, and
## notify listeners (Main -> MapView). Call after any pending change.
func _refresh_targeting() -> void:
    _ensure_targeting_banner()
    var info := _current_targeting_info()
    if info.is_empty():
        _targeting_banner.visible = false
    else:
        _targeting_banner.visible = true
        _targeting_banner_label.text = _targeting_banner_bbcode(info)
    emit_signal("targeting_changed", info)

## The active targeting descriptor, or {} when nothing is targeting. A pending
## harvest/hunt needs a band; a pending scout needs a tile.
## The active targeting descriptor, or {} when nothing is targeting. Move-band is the
## one remaining targeting flow (the single-task Harvest/Hunt/Scout flows were retired
## with the labor-allocation model): it needs a destination tile.
func _current_targeting_info() -> Dictionary:
    if not _pending_move_band.is_empty():
        var pos: Array = Array(_pending_move_band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(_pending_move_band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(_pending_move_band.get("current_y", -1))
        return {
            "active": true,
            "command": "move",
            "need": "tile",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": String(_pending_move_band.get("id", "Band")),
        }
    if not _pending_send_expedition.is_empty():
        var band: Dictionary = _pending_send_expedition.get("band", {})
        var pos: Array = Array(band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
        return {
            "active": true,
            "command": "expedition",
            "need": "tile",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": "%s · %d" % [
                String(band.get("id", "Band")), int(_pending_send_expedition.get("party_workers", 0)),
            ],
        }
    if not _pending_send_hunt_expedition.is_empty():
        var band: Dictionary = _pending_send_hunt_expedition.get("band", {})
        var pos: Array = Array(band.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
        var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
        return {
            "active": true,
            "command": "hunt_expedition",
            "need": "herd",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": "%s · %d" % [
                String(band.get("id", "Band")), int(_pending_send_hunt_expedition.get("party_workers", 0)),
            ],
        }
    return {}

func _targeting_banner_bbcode(info: Dictionary) -> String:
    var cmd := String(info.get("command", "")).to_upper()
    var need := String(info.get("need", ""))
    var ctx := String(info.get("context_label", ""))
    var loc := ""
    if need == "band":
        loc = "  [color=#%s](%d, %d)[/color]" % [
            HudStyle.INK_DIM_HEX, int(info.get("origin_x", 0)), int(info.get("origin_y", 0)),
        ]
    var instruction := ""
    if need == "band":
        instruction = "click a band to send it here"
    elif cmd == "MOVE":
        instruction = "click a destination tile"
    elif cmd == "EXPEDITION":
        instruction = "click a target tile to scout"
    elif cmd == "HUNT_EXPEDITION":
        instruction = "click on a herd to hunt"
    else:
        instruction = "click a tile to survey"
    return "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
        HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
    ]

## Cancel the active targeting (banner Cancel / Esc / right-click all route here).
func cancel_active_targeting() -> void:
    _cancel_pending_move_band()
    _cancel_pending_send_expedition()
    _cancel_pending_send_hunt_expedition()

## Lower-left version overlay showing the client build and the streamed server build,
## so the running builds can be confirmed at a glance. Mouse-transparent so it never
## intercepts map clicks.
func _setup_build_overlay() -> void:
    _build_label = Label.new()
    _build_label.name = "BuildOverlay"
    _build_label.anchor_left = 0.0
    _build_label.anchor_right = 0.0
    _build_label.anchor_top = 1.0
    _build_label.anchor_bottom = 1.0
    _build_label.offset_left = 8.0
    _build_label.offset_top = -26.0
    _build_label.offset_right = 480.0
    _build_label.offset_bottom = -6.0
    _build_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    _build_label.add_theme_color_override("font_color", Color(0.85, 0.9, 1.0, 0.65))
    add_child(_build_label)
    _refresh_build_overlay()

func _refresh_build_overlay() -> void:
    if _build_label != null:
        _build_label.text = "build  cli %s · srv %s" % [CLIENT_BUILD, _server_build]

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
    # Authoritative snapshot turn — drives optimistic-pending reconciliation (see
    # _reconcile_pending, called from update_band_alerts later in the same snapshot cycle).
    _current_turn = turn
    turn_label.text = "Turn %d" % turn
    if turn_orb != null:
        turn_orb.set_turn(turn)
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

## Show the player faction's Sedentarization pressure as a compact top-bar text meter.
## Hidden until the score is meaningful; tinted amber (soft) / cyan (hard) as it climbs.
func update_sedentarization(sedentarization_variant: Variant) -> void:
    if sedentarization_label == null:
        return
    var score := 0.0
    var stage := ""
    if sedentarization_variant is Array:
        for entry in sedentarization_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                score = float(entry.get("score", 0.0))
                stage = String(entry.get("stage", ""))
                break
    if score < 1.0:
        sedentarization_label.visible = false
        return
    sedentarization_label.visible = true
    var suffix := "" if stage == "" or stage == "none" else " · %s" % stage
    sedentarization_label.text = "Sedentarization  %s  %d/100%s" % [_meter_bar(score), int(round(score)), suffix]
    sedentarization_label.add_theme_color_override("font_color", _sedentarization_color(stage))

## Show the player faction's age structure (children / working / elders) and the dependency
## ratio — the core demographic tension. Hidden until the faction has population.
func update_demographics(demographics_variant: Variant) -> void:
    if demographics_label == null:
        return
    var children := 0
    var working := 0
    var elders := 0
    var found := false
    if demographics_variant is Array:
        for entry in demographics_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                children = int(entry.get("children", 0))
                working = int(entry.get("working", 0))
                elders = int(entry.get("elders", 0))
                found = true
                break
    var total := children + working + elders
    if not found or total <= 0:
        demographics_label.visible = false
        return
    demographics_label.visible = true
    # Dependency ratio = dependents (children + elders) per 100 working-age.
    var dependency := 0
    if working > 0:
        dependency = int(round(float(children + elders) / float(working) * 100.0))
    else:
        dependency = 999
    demographics_label.text = "Pop %d  👶%d 🛠%d 🧓%d  dep %d/100" % [total, children, working, elders, dependency]
    # A high dependency ratio (more mouths than hands) is the warning state.
    demographics_label.add_theme_color_override("font_color", _dependency_color(working, dependency))

## Show the player faction's count of discovered Wondrous Sites as a compact top-bar readout
## (`◈ Discoveries N  ⛰ ⛲`, appending the distinct site glyphs so landmark vs settle_site reads
## at a glance). Hidden until at least one site is known.
func update_discoveries(discovered_variant: Variant) -> void:
    if discoveries_label == null:
        return
    var sites: Array = []
    if discovered_variant is Array:
        for entry in discovered_variant:
            if entry is Dictionary and int(entry.get("faction", -1)) == PLAYER_FACTION_ID:
                var faction_sites: Variant = entry.get("sites", [])
                if faction_sites is Array:
                    sites = faction_sites
                break
    if sites.is_empty():
        discoveries_label.visible = false
        return
    discoveries_label.visible = true
    var glyphs: Array[String] = []
    for site in sites:
        if not (site is Dictionary):
            continue
        var glyph := String((site as Dictionary).get("glyph", "")).strip_edges()
        if glyph != "" and not glyphs.has(glyph):
            glyphs.append(glyph)
    var suffix := ""
    if not glyphs.is_empty():
        suffix = "  %s" % " ".join(glyphs)
    discoveries_label.text = "%s Discoveries %d%s" % [DISCOVERIES_GLYPH, sites.size(), suffix]
    discoveries_label.add_theme_color_override("font_color", HudStyle.SIGNAL)

## Tint the dependency readout: amber when dependents outnumber workers, cyan when there is a
## healthy labor surplus, neutral otherwise.
func _dependency_color(working: int, dependency: int) -> Color:
    if working <= 0 or dependency >= 100:
        return HudStyle.WARN
    if dependency <= 60:
        return HudStyle.SIGNAL
    return HudStyle.INK_DIM

## A 10-cell block-glyph bar for a 0–100 score.
func _meter_bar(score: float) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * 10.0))
    return "▰".repeat(filled) + "▱".repeat(10 - filled)

func _sedentarization_color(stage: String) -> Color:
    match stage:
        "hard":
            return HudStyle.SIGNAL
        "soft":
            return HudStyle.WARN
        _:
            return HudStyle.INK_DIM

func update_stockpiles(faction_inventory_variant: Variant) -> void:
    if stockpile_panel == null:
        return
    var faction_array: Array = faction_inventory_variant if faction_inventory_variant is Array else []
    var next_totals: Dictionary = {}
    for faction_entry in faction_array:
        if not (faction_entry is Dictionary):
            continue
        if int(faction_entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        var inventory_variant: Variant = faction_entry.get("inventory", [])
        if inventory_variant is Array:
            var inventory_entries: Array = inventory_variant
            for stock_entry in inventory_entries:
                if not (stock_entry is Dictionary):
                    continue
                var item_name := String(stock_entry.get("item", "")).strip_edges()
                if item_name == "":
                    continue
                next_totals[item_name] = int(stock_entry.get("quantity", 0))
        break
    var combined_keys: Array = []
    for key in _stockpile_totals.keys():
        if not combined_keys.has(key):
            combined_keys.append(key)
    for key in next_totals.keys():
        if not combined_keys.has(key):
            combined_keys.append(key)
    combined_keys.sort()
    var panel_entries: Array = []
    for key in combined_keys:
        var amount := int(next_totals.get(key, 0))
        var previous := int(_stockpile_totals.get(key, 0))
        if amount == 0 and previous == 0:
            continue
        var delta := float(amount - previous)
        panel_entries.append({
            "label": _format_stockpile_label(key),
            "amount": amount,
            "delta": delta,
        })
    _stockpile_totals = next_totals
    if stockpile_list == null or stockpile_panel == null:
        return
    for child in stockpile_list.get_children():
        child.queue_free()
    if panel_entries.is_empty():
        stockpile_panel.visible = false
        return
    stockpile_panel.visible = true
    for entry in panel_entries:
        stockpile_list.add_child(_build_stockpile_row(entry))

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

## Wire the turn orb: it re-emits the existing advance/jump signals, so the Main
## wiring (next_turn_requested / alert_focus_requested → MapView.focus_on_tile) is
## unchanged — the orb just replaces the old advance-turn button as their source.
func _connect_turn_orb() -> void:
    if turn_orb == null:
        return
    if not turn_orb.is_connected("focus_requested", Callable(self, "_on_turn_orb_focus")):
        turn_orb.focus_requested.connect(_on_turn_orb_focus)
    if not turn_orb.is_connected("advance_requested", Callable(self, "_on_turn_orb_advance")):
        turn_orb.advance_requested.connect(_on_turn_orb_advance)

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

func _on_turn_orb_focus(x: int, y: int) -> void:
    emit_signal("alert_focus_requested", x, y)

func _on_turn_orb_advance() -> void:
    emit_signal("next_turn_requested", 1)

# ---- Early-Game Labor allocation (slice 3b) --------------------------------
# Source-centric worker allocation for the single player band. The allocation panel
# (band drawer), the herd "assign hunters" controls, and the tile "assign foragers"
# controls are all built at runtime here; each emits `assign_labor_requested` (Main
# formats the `assign_labor …` command). Clear-all reuses `cancel_order_requested`.

## Resolve the band that assignment/move/clear commands target. The selected band when
## it is a player band; otherwise the single player band captured from the snapshot (so
## herd/tile assign controls still target it while a herd/tile is selected). {} if none.
func _resolve_assign_band() -> Dictionary:
    if not _selected_unit.is_empty() and _is_player_unit(_selected_unit):
        return _selected_unit
    return _player_band

## The player bands the band-picker lists. Normally `_player_bands` (captured each snapshot);
## falls back to `[_player_band]` when only the single band was seeded (e.g. the ui_preview
## harness, or before the first alerts pass) so the dropdown is always populated.
func _current_player_bands() -> Array:
    if not _player_bands.is_empty():
        return _player_bands
    return [_player_band] if not _player_band.is_empty() else []

## Resolve a listed player band by its entity id; {} if it is no longer present.
func _player_band_by_entity(entity: int) -> Dictionary:
    for b in _current_player_bands():
        if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
            return b
    return {}

## Max workers a band can commit to ONE source: its idle workers plus any it already has on
## that source (the assign REPLACES that count, so re-editing an existing assignment isn't
## capped below its current staffing). Reduces to `idle_workers` for a fresh source.
func _assignable_hunt_workers(band: Dictionary, herd_id: String) -> int:
    return int(band.get("idle_workers", 0)) + _workers_for_hunt(band, herd_id)

func _assignable_forage_workers(band: Dictionary, x: int, y: int) -> int:
    return int(band.get("idle_workers", 0)) + _workers_for_forage(band, x, y)

## Map grid dimensions captured each snapshot (Main forwards the snapshot `grid` key). Width + wrap
## feed the wrap-aware hex distance the herd-hunt affordance keys its local-vs-expedition decision
## off. Grid rides full snapshots only; persists across deltas (fields default to the last value).
func set_grid_dimensions(grid: Variant) -> void:
    if not (grid is Dictionary):
        return
    var g: Dictionary = grid
    _grid_width = int(g.get("width", _grid_width))
    _grid_height = int(g.get("height", _grid_height))
    _grid_wrap_horizontal = bool(g.get("wrap_horizontal", _grid_wrap_horizontal))

## The band's current tile (col,row), reading the raw cohort `current_x/y` (snapshot entries) or the
## MapView marker's `pos` fallback; (-1,-1) when unknown.
func _band_tile(band: Dictionary) -> Vector2i:
    var cx := int(band.get("current_x", -1))
    var cy := int(band.get("current_y", -1))
    if cx >= 0 and cy >= 0:
        return Vector2i(cx, cy)
    var pos_variant: Variant = band.get("pos", [])
    if pos_variant is Array and (pos_variant as Array).size() == 2:
        return Vector2i(int((pos_variant as Array)[0]), int((pos_variant as Array)[1]))
    return Vector2i(-1, -1)

## Shortest signed column delta from→to honoring horizontal wrap (mirrors MapView._wrapped_col_delta),
## so a herd across the seam measures by its short wrapped distance, not the long way across the map.
## Mirrors the sim's `grid_utils::shortest_delta_x` exactly (magnitude only here, no live
## direction effect): keep the direct delta when within half the width, else shift by one width.
## The exact-half tie (`abs(d) == width/2`) resolves POSITIVE like the sim, NOT `round()`'s
## half-away-from-zero — kept consistent with MapView._wrapped_col_delta.
func _wrapped_col_delta(from_col: int, to_col: int) -> int:
    var d := to_col - from_col
    if _grid_wrap_horizontal and _grid_width > 0:
        # Integer half-width mirrors the sim's `w / 2` truncation.
        var half_width := _grid_width / 2
        if d > half_width:
            d -= _grid_width
        elif d < -half_width:
            d += _grid_width
    return d

## odd-r offset (col,row) → axial (mirrors MapView._offset_to_axial).
func _offset_to_axial(col: int, row: int) -> Vector2i:
    var q := col - ((row - (row & 1)) >> 1)
    return Vector2i(q, row)

## Wrap-aware true odd-r hex distance between two offset tiles (mirrors the sim's `hex_distance_wrapped`
## / MapView._hex_distance): bring the target into the source's column frame via _wrapped_col_delta,
## then odd-r offset→axial→cube distance. Returns -1 when either tile is unknown.
func _hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
    if a_col < 0 or a_row < 0 or b_col < 0 or b_row < 0:
        return -1
    var b_eff_col := a_col + _wrapped_col_delta(a_col, b_col)
    var a := _offset_to_axial(a_col, a_row)
    var b := _offset_to_axial(b_eff_col, b_row)
    var dq: int = a.x - b.x
    var dr: int = a.y - b.y
    return int((abs(dq) + abs(dr) + abs(dq + dr)) / 2)

## Max party the band can detach as a hunting expedition: min(idle_workers, max_expedition_party_size),
## falling back to idle when the cap is absent/0 (mirrors _build_send_expedition_controls' party_max).
func _expedition_party_cap(band: Dictionary) -> int:
    var idle := int(band.get("idle_workers", 0))
    var cap := int(band.get("max_expedition_party_size", 0))
    return mini(idle, cap) if cap > 0 else idle

## A "Band: [▼]" dropdown row for the assign controls: lists every player band (positional
## "Band N" names, matching the roster) and selects `selected_band`; `on_pick` fires with the
## chosen band dict. The actor band is always explicit — shown even with one band (single-item
## dropdown). NOTE: lists ALL player bands; in-range filtering (Forage within work_range / Hunt
## within work_range + leash) is deferred to the multi-band slice (needs the hunt-leash reach in
## the snapshot, and can't be exercised until a 2nd band can exist).
func _build_band_picker(selected_band: Dictionary, on_pick: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var name_label := Label.new()
    name_label.text = BAND_PICKER_LABEL
    name_label.add_theme_color_override("font_color", HudStyle.INK)
    row.add_child(name_label)
    var picker := OptionButton.new()
    picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var bands := _current_player_bands()
    var selected_entity := int(selected_band.get("entity", -1))
    var selected_idx := 0
    for i in bands.size():
        var b: Dictionary = bands[i]
        picker.add_item(_band_display_name(b, i + 1))
        picker.set_item_metadata(i, int(b.get("entity", -1)))
        if int(b.get("entity", -1)) == selected_entity:
            selected_idx = i
    picker.select(selected_idx)
    picker.item_selected.connect(func(idx: int) -> void:
        on_pick.call(_player_band_by_entity(int(picker.get_item_metadata(idx)))))
    row.add_child(picker)
    return row

func _labor_assignments_of(band: Dictionary) -> Array:
    var v: Variant = band.get("labor_assignments", [])
    return v if v is Array else []

## Workers currently on a band-wide role (scout/warrior); 0 when unstaffed.
func _workers_for_role(band: Dictionary, kind: String) -> int:
    for entry in _labor_assignments_of(band):
        if entry is Dictionary and String((entry as Dictionary).get("kind", "")).to_lower() == kind:
            return int((entry as Dictionary).get("workers", 0))
    return 0

## Workers currently foraging a specific in-range tile; 0 when unstaffed.
func _workers_for_forage(band: Dictionary, x: int, y: int) -> int:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_FORAGE \
                and int(a.get("target_x", -1)) == x and int(a.get("target_y", -1)) == y:
            return int(a.get("workers", 0))
    return 0

## Workers currently hunting a specific herd; 0 when unstaffed.
func _workers_for_hunt(band: Dictionary, herd_id: String) -> int:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
            return int(a.get("workers", 0))
    return 0

## The take policy of the band's existing hunt on `herd_id`, else the default.
func _policy_for_hunt(band: Dictionary, herd_id: String) -> String:
    for entry in _labor_assignments_of(band):
        if not (entry is Dictionary):
            continue
        var a: Dictionary = entry
        if String(a.get("kind", "")).to_lower() == LABOR_KIND_HUNT and String(a.get("fauna_id", "")) == herd_id:
            var policy := String(a.get("policy", "")).strip_edges().to_lower()
            if policy in LABOR_HUNT_POLICIES:
                return policy
    return DEFAULT_HUNT_POLICY

## A friendlier label for a herd id — the roster/selected herd's label when known.
func _herd_label_for_id(herd_id: String) -> String:
    var herd := _find_roster_herd(herd_id)
    if not herd.is_empty():
        return String(herd.get("species", herd.get("label", herd_id)))
    if String(_selected_herd.get("id", "")) == herd_id:
        return String(_selected_herd.get("species", _selected_herd.get("label", herd_id)))
    return herd_id

## Emit an assign_labor request for the given band, and record it as an OPTIMISTIC pending
## action so the panel + map reflect the change immediately (reconciled by the next
## newer-turn snapshot). Main formats the text command from the emitted payload.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String) -> void:
    var bits := int(band.get("entity", -1))
    if bits < 0:
        return
    var clamped: int = max(0, workers)
    emit_signal("assign_labor_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": bits,
        "kind": kind,
        "workers": clamped,
        "x": x,
        "y": y,
        "herd_id": herd_id,
        "policy": policy,
    })
    _record_pending_assign(bits, kind, clamped, x, y, herd_id, policy)
    _after_pending_change()

# ---- Optimistic pending labor (slice 3b UX) --------------------------------

## Stable key identifying a source/role within a band's assignment set.
func _pending_key(kind: String, x: int, y: int, herd_id: String) -> String:
    match kind:
        LABOR_KIND_FORAGE:
            return "forage:%d,%d" % [x, y]
        LABOR_KIND_HUNT:
            return "hunt:%s" % herd_id
        _:
            return kind  # scout / warrior — one band-wide role each

func _record_pending_assign(entity: int, kind: String, workers: int, x: int, y: int, herd_id: String, policy: String) -> void:
    if entity < 0:
        return
    var entry: Dictionary = _pending_labor.get(entity, {})
    entry["turn"] = _current_turn
    var assigns: Dictionary = entry.get("assign", {})
    assigns[_pending_key(kind, x, y, herd_id)] = {
        "kind": kind, "workers": max(0, workers), "x": x, "y": y, "herd_id": herd_id, "policy": policy,
    }
    entry["assign"] = assigns
    _pending_labor[entity] = entry

func _record_pending_move(entity: int, x: int, y: int) -> void:
    if entity < 0:
        return
    var entry: Dictionary = _pending_labor.get(entity, {})
    entry["turn"] = _current_turn
    entry["move"] = {"x": x, "y": y}
    _pending_labor[entity] = entry

## Re-render the current selection (so pending shows in the panel) and push the pending map
## to MapView (so pending hexes show), after any optimistic change.
func _after_pending_change() -> void:
    if not _selected_tile_info.is_empty() or not _selected_unit.is_empty() or not _selected_herd.is_empty():
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    emit_signal("labor_pending_changed", _pending_labor)

## Drop pending entries the server has already processed: a snapshot with a turn NEWER than
## the entry's issue turn is authoritative confirmation (and reflects any clamping). Called
## each snapshot from update_band_alerts, after update_overlay has set _current_turn.
func _reconcile_pending() -> void:
    if _pending_labor.is_empty():
        return
    var changed := false
    for entity in _pending_labor.keys():
        var entry: Dictionary = _pending_labor[entity]
        if int(entry.get("turn", -1)) < _current_turn:
            _pending_labor.erase(entity)
            changed = true
    if changed:
        emit_signal("labor_pending_changed", _pending_labor)

func _pending_assigns_for(entity: int) -> Dictionary:
    var e: Variant = _pending_labor.get(entity, {})
    if not (e is Dictionary):
        return {}
    var a: Variant = (e as Dictionary).get("assign", {})
    return a if a is Dictionary else {}

## Confirmed labor assignments overlaid with this band's pending assigns, keyed by source/role.
## Each value: {kind, workers, x, y, herd_id, policy, pending: bool}.
func _effective_worker_map(band: Dictionary) -> Dictionary:
    var merged: Dictionary = {}
    for a in _labor_assignments_of(band):
        if not (a is Dictionary):
            continue
        var kind := String((a as Dictionary).get("kind", "")).strip_edges().to_lower()
        var key := _pending_key(kind, int(a.get("target_x", -1)), int(a.get("target_y", -1)), String(a.get("fauna_id", "")))
        merged[key] = {
            "kind": kind, "workers": int(a.get("workers", 0)),
            "x": int(a.get("target_x", -1)), "y": int(a.get("target_y", -1)),
            "herd_id": String(a.get("fauna_id", "")), "policy": String(a.get("policy", "")), "pending": false,
        }
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    for key in pend:
        var pd: Dictionary = pend[key]
        merged[key] = {
            "kind": String(pd.get("kind", "")), "workers": int(pd.get("workers", 0)),
            "x": int(pd.get("x", -1)), "y": int(pd.get("y", -1)),
            "herd_id": String(pd.get("herd_id", "")), "policy": String(pd.get("policy", "")), "pending": true,
        }
    return merged

## Effective worker count for one role/source, overlaying any pending value.
func _effective_role_workers(band: Dictionary, kind: String) -> Dictionary:
    var key := _pending_key(kind, -1, -1, "")
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    if pend.has(key):
        return {"workers": int((pend[key] as Dictionary).get("workers", 0)), "pending": true}
    return {"workers": _workers_for_role(band, kind), "pending": false}

func _effective_forage_workers(band: Dictionary, x: int, y: int) -> int:
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    var key := _pending_key(LABOR_KIND_FORAGE, x, y, "")
    if pend.has(key):
        return int((pend[key] as Dictionary).get("workers", 0))
    return _workers_for_forage(band, x, y)

func _effective_hunt_workers(band: Dictionary, herd_id: String) -> int:
    var pend := _pending_assigns_for(int(band.get("entity", -1)))
    var key := _pending_key(LABOR_KIND_HUNT, -1, -1, herd_id)
    if pend.has(key):
        return int((pend[key] as Dictionary).get("workers", 0))
    return _workers_for_hunt(band, herd_id)

## Optimistic idle = working-age minus the sum of effective worker counts.
func _effective_idle(band: Dictionary) -> int:
    var assigned := 0
    var merged := _effective_worker_map(band)
    for key in merged:
        assigned += int((merged[key] as Dictionary).get("workers", 0))
    return max(0, int(band.get("working_age", 0)) - assigned)

## A "<label>   − N +" worker-count row. `on_change` is called with the new count
## when either stepper is pressed. `plus_enabled` gates the + (e.g. no idle workers).
## `pending` marks an optimistic (not-yet-confirmed) row: the label reads amber with a
## "· pending" suffix, tying it to the amber pending hex on the map.
func _build_worker_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, pending: bool = false) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var name_label := Label.new()
    name_label.text = label_text + (PENDING_ROW_SUFFIX if pending else "")
    name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    name_label.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK)
    row.add_child(name_label)
    var minus := Button.new()
    minus.text = "−"
    minus.custom_minimum_size = Vector2(WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = count <= 0
    minus.pressed.connect(func() -> void: on_change.call(count - WORKER_STEP))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(count)
    value.custom_minimum_size = Vector2(WORKER_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if count > 0 else HudStyle.INK_FAINT)
    row.add_child(value)
    var plus := Button.new()
    plus.text = "+"
    plus.custom_minimum_size = Vector2(WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = not plus_enabled
    plus.pressed.connect(func() -> void: on_change.call(count + WORKER_STEP))
    row.add_child(plus)
    return row

## The band allocation panel: Working/Idle header, one −/+ row per staffed Forage/Hunt
## source, the always-present Scout + Warrior band-wide role rows, and Move / Clear-all.
## Each source/role row re-sends assign_labor with the new count (0 removes).
## A dim uppercase section header inside the allocation panel ("Current actions" / "Band roles").
func _alloc_section_label(text: String) -> Label:
    var label := Label.new()
    label.text = text.to_upper()
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    return label

## A dim wrapping hint line (role explanation / empty-state prompt).
func _alloc_hint_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ALLOC_SECTION_FONT_SIZE)
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    return label

func _build_allocation_panel(band: Dictionary) -> void:
    if allocation_panel == null:
        return
    for child in allocation_panel.get_children():
        child.queue_free()
    var is_player := not band.is_empty() and _is_player_unit(band)
    allocation_panel.visible = is_player
    if not is_player:
        return
    var population := int(band.get("size", 0))
    var working := int(band.get("working_age", 0))
    # Idle counts OPTIMISTICALLY (confirmed idle overlaid with any pending changes) so the
    # math reflects a just-issued assignment immediately.
    var idle := _effective_idle(band)
    var can_add := idle > 0
    # Clarified header: population (all people) vs the working-age labor split, so nobody
    # expects "Idle" to equal the 30 people — only the ~16 workers labor (children/elders eat
    # but don't work). E.g. "Population 30 · Workers 16 (Idle 16)".
    var header := Label.new()
    header.text = "Population %d · Workers %d (Idle %d)" % [population, working, idle]
    header.add_theme_color_override("font_color", HudStyle.SIGNAL if can_add else HudStyle.INK_DIM)
    header.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    header.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    allocation_panel.add_child(header)
    # "Current actions" — the report of what each group is doing (confirmed + optimistic).
    allocation_panel.add_child(_alloc_section_label(ALLOC_HEADER_ACTIONS))
    var merged := _effective_worker_map(band)
    var has_source := false
    for key in merged:
        var m: Dictionary = merged[key]
        var kind := String(m.get("kind", "")).strip_edges().to_lower()
        var workers := int(m.get("workers", 0))
        var pending := bool(m.get("pending", false))
        # Show a source row when it's staffed, or while its removal/change is still pending.
        if kind == LABOR_KIND_FORAGE and (workers > 0 or pending):
            has_source = true
            var fx := int(m.get("x", -1))
            var fy := int(m.get("y", -1))
            allocation_panel.add_child(_build_worker_stepper(
                "Forage (%d, %d)" % [fx, fy], workers, can_add,
                func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_FORAGE, n, fx, fy, "", ""),
                pending))
        elif kind == LABOR_KIND_HUNT and (workers > 0 or pending):
            has_source = true
            var herd_id := String(m.get("herd_id", ""))
            var hx := int(m.get("x", -1))
            var hy := int(m.get("y", -1))
            var policy := String(m.get("policy", ""))
            if not (policy in LABOR_HUNT_POLICIES):
                policy = _policy_for_hunt(band, herd_id)
            allocation_panel.add_child(_build_worker_stepper(
                "Hunt %s [%s]" % [_herd_label_for_id(herd_id), policy], workers, can_add,
                func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_HUNT, n, hx, hy, herd_id, policy),
                pending))
    if not has_source:
        allocation_panel.add_child(_alloc_hint_label(ALLOC_NO_SOURCES_HINT))
    # Scout + Warrior are standing band-wide roles: always shown (even at 0 workers), each
    # with a one-line hint so the −/+ steppers read as "this is how you staff this role".
    allocation_panel.add_child(_alloc_section_label(ALLOC_HEADER_ROLES))
    var scout_eff := _effective_role_workers(band, LABOR_KIND_SCOUT)
    allocation_panel.add_child(_build_worker_stepper(
        "Scout", int(scout_eff.get("workers", 0)), can_add,
        func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_SCOUT, n, -1, -1, "", ""),
        bool(scout_eff.get("pending", false))))
    allocation_panel.add_child(_alloc_hint_label(SCOUT_ROLE_HINT))
    var warrior_eff := _effective_role_workers(band, LABOR_KIND_WARRIOR)
    allocation_panel.add_child(_build_worker_stepper(
        "Warrior", int(warrior_eff.get("workers", 0)), can_add,
        func(n: int) -> void: _emit_assign_labor(band, LABOR_KIND_WARRIOR, n, -1, -1, "", ""),
        bool(warrior_eff.get("pending", false))))
    allocation_panel.add_child(_alloc_hint_label(WARRIOR_ROLE_HINT))
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "primary")
    move_btn.tooltip_text = "Relocate the band, then click a destination tile."
    move_btn.pressed.connect(_on_move_band_pressed)
    actions.add_child(move_btn)
    var clear_btn := Button.new()
    clear_btn.text = "Clear all"
    HudStyle.apply_button(clear_btn, "ghost")
    clear_btn.tooltip_text = "Return every worker to idle (clears all assignments)."
    clear_btn.disabled = not has_source and idle >= working
    clear_btn.pressed.connect(_on_clear_all_pressed.bind(band))
    actions.add_child(clear_btn)
    allocation_panel.add_child(actions)
    # Outfit affordance: detach a party from this band's idle workers as a scouting expedition.
    _build_send_expedition_controls(band, idle)

## Outfit affordance on a resident band's allocation panel (docs/plan_exploration_and_sites.md §2):
## a party-size stepper (1..party_max, where party_max = min(idle_workers, max_expedition_party_size))
## + a "Send scouting expedition" button that enters tile-targeting; the target click emits
## `send_expedition_requested`. Hidden when the band has no idle workers to spare. The server still
## rejects a genuinely over-cap request with a feed message as a backstop.
func _build_send_expedition_controls(band: Dictionary, idle: int) -> void:
    if allocation_panel == null or idle <= 0:
        return
    allocation_panel.add_child(_alloc_section_label(SEND_EXPEDITION_SECTION))
    # The party max is the smaller of the band's idle workers and the server's hard party-size cap
    # (from the expedition config). Guard defensively: a missing/0 cap (older server, or the field
    # absent) falls back to idle so the stepper is never clamped to 0.
    var cap := int(band.get("max_expedition_party_size", 0))
    var party_max: int = mini(idle, cap) if cap > 0 else idle
    # Clamp the persisted party size into 1..party_max (both can shrink between renders).
    _send_expedition_count = clampi(_send_expedition_count, WORKER_STEP, party_max)
    allocation_panel.add_child(_build_worker_stepper(
        "Party", _send_expedition_count, _send_expedition_count < party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, WORKER_STEP, party_max)
            _build_allocation_panel(band)))
    # Both expedition verbs share the one party stepper above (they detach the same workers); the
    # scout targets a tile, the hunt targets a herd.
    var send_btn := Button.new()
    send_btn.text = SEND_EXPEDITION_BUTTON
    HudStyle.apply_button(send_btn, "primary")
    send_btn.tooltip_text = SEND_EXPEDITION_HINT
    send_btn.pressed.connect(func() -> void: _on_send_expedition_pressed(band, _send_expedition_count))
    allocation_panel.add_child(send_btn)
    # Hunt verb: a policy radio (Sustain/Surplus/Market/Eradicate, default Sustain) + a one-line
    # behaviour hint for the picked policy, then the launch button. The policy is the trailing arg.
    if not (_send_hunt_policy in LABOR_HUNT_POLICIES):
        _send_hunt_policy = DEFAULT_HUNT_POLICY
    allocation_panel.add_child(_build_policy_picker(func(policy: String) -> void:
        _send_hunt_policy = policy
        _build_allocation_panel(band), _send_hunt_policy))
    allocation_panel.add_child(_alloc_hint_label(String(SEND_HUNT_POLICY_HINTS.get(_send_hunt_policy, ""))))
    var hunt_btn := Button.new()
    hunt_btn.text = SEND_HUNT_EXPEDITION_BUTTON
    HudStyle.apply_button(hunt_btn, "primary")
    hunt_btn.tooltip_text = SEND_HUNT_EXPEDITION_HINT
    hunt_btn.pressed.connect(func() -> void: _on_send_hunt_expedition_pressed(band, _send_expedition_count, _send_hunt_policy))
    allocation_panel.add_child(hunt_btn)

## The dedicated panel for a selected in-flight expedition (no labor in v1): an awaiting-orders
## callout (echoing the pulsing map ring) plus Move (retarget via move_band on the expedition
## entity) and Recall. Reuses the allocation-panel host; player expeditions only.
func _build_expedition_panel(expedition: Dictionary) -> void:
    if allocation_panel == null:
        return
    for child in allocation_panel.get_children():
        child.queue_free()
    var is_player := not expedition.is_empty() and _is_player_unit(expedition)
    allocation_panel.visible = is_player
    if not is_player:
        return
    var phase := String(expedition.get("expedition_phase", "")).strip_edges().to_lower()
    if phase == EXPEDITION_PHASE_AWAITING:
        var callout := _alloc_hint_label("Reached its objective — Recall it home, or Move it onward.")
        callout.add_theme_color_override("font_color", HudStyle.WARN)
        allocation_panel.add_child(callout)
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = "Send the expedition onward, then click a target tile."
    move_btn.pressed.connect(_on_move_band_pressed)
    actions.add_child(move_btn)
    # Already homeward-bound: the button reads its state ("Returning", disabled) rather than a
    # mysterious grayed-out "Recall". Otherwise it's an enabled "Recall" that folds the party home.
    var returning := phase == EXPEDITION_PHASE_RETURNING
    var recall_btn := Button.new()
    recall_btn.text = "Returning" if returning else "Recall"
    HudStyle.apply_button(recall_btn, "primary")
    recall_btn.tooltip_text = "Heading home — folds workers + provisions back on arrival." if returning \
        else "Order the expedition home (folds workers + provisions back on arrival)."
    recall_btn.disabled = returning
    recall_btn.pressed.connect(_on_recall_expedition_pressed.bind(expedition))
    actions.add_child(recall_btn)
    allocation_panel.add_child(actions)

## Recall the selected in-flight expedition (folds it home). Emits recall_expedition_requested;
## Main formats the `recall_expedition …` command.
func _on_recall_expedition_pressed(expedition: Dictionary) -> void:
    if expedition.is_empty():
        return
    emit_signal("recall_expedition_requested", {
        "faction": int(expedition.get("faction", PLAYER_FACTION_ID)),
        "expedition": int(expedition.get("entity", -1)),
    })

## The herd "Assign hunters" controls (compose a count + policy, then Assign). Shown
## only for a huntable herd while a player band exists to staff it.
func _build_herd_assign_controls(herd: Dictionary) -> void:
    if herd_assign_controls == null:
        return
    for child in herd_assign_controls.get_children():
        child.queue_free()
    var resolved := _resolve_assign_band()
    var can_assign := bool(herd.get("huntable", false)) and not resolved.is_empty()
    herd_assign_controls.visible = can_assign
    if not can_assign:
        return
    var herd_id := String(herd.get("id", ""))
    # When the selected herd changes, default the actor band to the resolved band (and re-seed
    # the compose count/policy from its staffing); otherwise preserve the picked band + count
    # across per-snapshot re-renders of the same herd.
    var source_changed := _hunt_assign_key != herd_id
    if source_changed:
        _hunt_assign_key = herd_id
        _hunt_assign_band = int(resolved.get("entity", -1))
    # The actor is the band-picker selection; fall back to the resolved band if it has vanished.
    var band := _player_band_by_entity(_hunt_assign_band)
    if band.is_empty():
        band = resolved
        _hunt_assign_band = int(band.get("entity", -1))
    if source_changed:
        var staffed := _workers_for_hunt(band, herd_id)
        _hunt_assign_count = staffed if staffed > 0 else WORKER_STEP
        _hunt_assign_policy = _policy_for_hunt(band, herd_id)
    # Show the effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _effective_hunt_workers(band, herd_id)
    var pending := _pending_assigns_for(int(band.get("entity", -1))).has(_pending_key(LABOR_KIND_HUNT, -1, -1, herd_id))
    var title := Label.new()
    title.text = "Assign hunters" + ("  (now %d%s)" % [current, " · pending" if pending else ""] if current > 0 or pending else "")
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
    herd_assign_controls.add_child(title)
    # Which band supplies the hunters (above the worker/party stepper, so it reads "which band →
    # how many workers"). Switching bands re-runs the distance-aware branch below for that band.
    herd_assign_controls.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _hunt_assign_band = int(picked.get("entity", -1))
        _build_herd_assign_controls(herd)))
    # Distance-aware: a LOCAL hunt when the herd is within the SELECTED band's hunt_reach, a hunting
    # EXPEDITION when it's beyond. Distance is wrap-aware from the picked band's OWN tile — every part
    # of the decision (distance, reach, and the command's band target) keys off `band` explicitly, so
    # the right band drives it even with multiple bands (single-band playtest can't surface a mixup).
    var herd_x := int(herd.get("x", -1))
    var herd_y := int(herd.get("y", -1))
    var band_tile := _band_tile(band)
    var reach := int(band.get("hunt_reach", 0))
    var distance := _hex_distance_wrapped(band_tile.x, band_tile.y, herd_x, herd_y)
    # Beyond reach → expedition. Unknown distance (missing tiles) falls back to the local hunt.
    var is_expedition := distance >= 0 and distance > reach
    # Local hunt caps at the band's assignable hunt workers; an expedition caps at the party ceiling.
    var cap := _expedition_party_cap(band) if is_expedition else _assignable_hunt_workers(band, herd_id)
    _hunt_assign_count = clampi(_hunt_assign_count, 0, cap)
    herd_assign_controls.add_child(_build_worker_stepper(
        "Party" if is_expedition else "Hunters", _hunt_assign_count, _hunt_assign_count < cap,
        func(n: int) -> void:
            _hunt_assign_count = clampi(n, 0, cap)
            _build_herd_assign_controls(herd)))
    herd_assign_controls.add_child(_build_policy_picker(func(policy: String) -> void:
        _hunt_assign_policy = policy
        _build_herd_assign_controls(herd)))
    if is_expedition:
        herd_assign_controls.add_child(_alloc_hint_label(
            "%s is %d tiles away — beyond this band's hunt reach (%d). Detach a party to follow it." \
            % [_herd_label_for_id(herd_id), distance, reach]))
    var assign_btn := Button.new()
    assign_btn.text = SEND_HUNTING_EXPEDITION_BUTTON if is_expedition else ASSIGN_LOCAL_HUNT_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    if is_expedition:
        # A hunting expedition needs a positive party; a local hunt allows 0 (removes the assignment).
        assign_btn.disabled = _hunt_assign_count <= 0
        assign_btn.pressed.connect(func() -> void:
            if _hunt_assign_count <= 0:
                return
            emit_signal("send_hunt_expedition_requested", {
                "faction": int(band.get("faction", PLAYER_FACTION_ID)),
                "band": int(band.get("entity", -1)),
                "party_workers": _hunt_assign_count,
                "fauna_id": herd_id,
                "policy": _hunt_assign_policy if _hunt_assign_policy in LABOR_HUNT_POLICIES else DEFAULT_HUNT_POLICY,
            }))
    else:
        assign_btn.pressed.connect(func() -> void:
            _emit_assign_labor(band, LABOR_KIND_HUNT, _hunt_assign_count,
                herd_x, herd_y, herd_id, _hunt_assign_policy))
    herd_assign_controls.add_child(assign_btn)

## A sustain/surplus/market/eradicate policy radio; `on_pick` fires with the chosen policy. The
## highlighted option is `selected` (defaults to the herd-assign compose policy so existing callers
## are unchanged; the send-hunt-expedition picker passes `_send_hunt_policy`).
func _build_policy_picker(on_pick: Callable, selected: String = "") -> HBoxContainer:
    var current := selected if selected != "" else _hunt_assign_policy
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", WORKER_STEPPER_SEPARATION)
    for policy in LABOR_HUNT_POLICIES:
        var btn := Button.new()
        btn.text = String(policy).capitalize()
        HudStyle.apply_button(btn, "primary" if policy == current else "ghost")
        btn.pressed.connect(func() -> void: on_pick.call(policy))
        row.add_child(btn)
    return row

## The tile "Assign foragers" controls (compose a count, then Assign). Shown only for a
## tile with a food module while a player band exists to staff it.
func _build_forage_assign_controls(tile_info: Dictionary) -> void:
    if forage_assign_controls == null:
        return
    for child in forage_assign_controls.get_children():
        child.queue_free()
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    var resolved := _resolve_assign_band()
    var can_assign := module_key != "" and not resolved.is_empty()
    forage_assign_controls.visible = can_assign
    if not can_assign:
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var key := "%d,%d" % [x, y]
    # When the selected tile changes, default the actor band to the resolved band (and re-seed
    # the count from its staffing); otherwise preserve the picked band + count across the
    # per-snapshot re-renders of the same tile.
    var source_changed := _forage_assign_key != key
    if source_changed:
        _forage_assign_key = key
        _forage_assign_band = int(resolved.get("entity", -1))
    var band := _player_band_by_entity(_forage_assign_band)
    if band.is_empty():
        band = resolved
        _forage_assign_band = int(band.get("entity", -1))
    if source_changed:
        var staffed := _workers_for_forage(band, x, y)
        _forage_assign_count = staffed if staffed > 0 else WORKER_STEP
    # Effective (pending-aware) staffing so re-selecting reflects a just-issued assign.
    var current := _effective_forage_workers(band, x, y)
    var pending := _pending_assigns_for(int(band.get("entity", -1))).has(_pending_key(LABOR_KIND_FORAGE, x, y, ""))
    var label := String(tile_info.get("food_module_label", module_key)).strip_edges()
    if label == "":
        label = module_key.capitalize()
    var title := Label.new()
    title.text = "Assign foragers — %s" % label + ("  (now %d%s)" % [current, " · pending" if pending else ""] if current > 0 or pending else "")
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK_DIM)
    forage_assign_controls.add_child(title)
    # Which band supplies the foragers (above the stepper). Switching re-runs the range check below
    # for that band.
    forage_assign_controls.add_child(_build_band_picker(band, func(picked: Dictionary) -> void:
        _forage_assign_band = int(picked.get("entity", -1))
        _build_forage_assign_controls(tile_info)))
    var cap := _assignable_forage_workers(band, x, y)
    _forage_assign_count = clampi(_forage_assign_count, 0, cap)
    forage_assign_controls.add_child(_build_worker_stepper(
        "Foragers", _forage_assign_count, _forage_assign_count < cap,
        func(n: int) -> void:
            _forage_assign_count = clampi(n, 0, cap)
            _build_forage_assign_controls(tile_info)))
    # Range-aware: foraging is stationary gathering (there is NO forage-expedition alternative), so a
    # tile beyond the SELECTED band's work_range DISABLES the button + shows an out-of-range hint,
    # rather than a fallback. Distance is wrap-aware from the picked band's OWN tile — distance,
    # work_range, and the target band all key off `band` explicitly (never the faction's default band).
    var band_tile := _band_tile(band)
    var work_range := int(band.get("work_range", 0))
    var distance := _hex_distance_wrapped(band_tile.x, band_tile.y, x, y)
    var out_of_range := distance >= 0 and distance > work_range
    if out_of_range:
        forage_assign_controls.add_child(_alloc_hint_label(
            "(%d,%d) is %d tiles away — beyond this band's forage range (%d)." % [x, y, distance, work_range]))
    var assign_btn := Button.new()
    assign_btn.text = FORAGE_ASSIGN_BUTTON
    HudStyle.apply_button(assign_btn, "primary")
    # Out of range → disabled (no expedition fallback for stationary gathering).
    assign_btn.disabled = out_of_range
    assign_btn.pressed.connect(func() -> void:
        _emit_assign_labor(band, LABOR_KIND_FORAGE, _forage_assign_count, x, y, "", ""))
    forage_assign_controls.add_child(assign_btn)

## Move-band: enter tile-targeting; the destination click emits move_band_requested.
func _on_move_band_pressed() -> void:
    var band := _resolve_assign_band()
    if band.is_empty():
        return
    _pending_move_band = band.duplicate(true)
    _refresh_targeting()

func _cancel_pending_move_band() -> void:
    if _pending_move_band.is_empty():
        return
    _pending_move_band = {}
    _refresh_targeting()

func _try_dispatch_pending_move_band(tile_info: Dictionary) -> void:
    if _pending_move_band.is_empty() or tile_info.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    var band := _pending_move_band
    var bits := int(band.get("entity", -1))
    emit_signal("move_band_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": bits,
        "x": x,
        "y": y,
    })
    _pending_move_band = {}
    _refresh_targeting()
    # Optimistic feedback: mark the destination pending until a newer-turn snapshot confirms.
    _record_pending_move(bits, x, y)
    _after_pending_change()

## Send-expedition: outfit `band` with `party_workers` and enter tile-targeting; the next tile
## click emits send_expedition_requested. Mirrors the move-band pending flow.
func _on_send_expedition_pressed(band: Dictionary, party_workers: int) -> void:
    if band.is_empty() or party_workers <= 0:
        return
    _pending_send_expedition = {"band": band.duplicate(true), "party_workers": party_workers}
    _refresh_targeting()

func _cancel_pending_send_expedition() -> void:
    if _pending_send_expedition.is_empty():
        return
    _pending_send_expedition = {}
    _refresh_targeting()

func _try_dispatch_pending_send_expedition(tile_info: Dictionary) -> void:
    if _pending_send_expedition.is_empty() or tile_info.is_empty():
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    var band: Dictionary = _pending_send_expedition.get("band", {})
    emit_signal("send_expedition_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": int(band.get("entity", -1)),
        "party_workers": int(_pending_send_expedition.get("party_workers", 0)),
        "x": x,
        "y": y,
    })
    _pending_send_expedition = {}
    _refresh_targeting()

## Send-hunt-expedition (docs/plan_exploration_and_sites.md §2b): outfit `band` with `party_workers`
## and enter HERD-targeting; the next click resolves to a huntable herd on the clicked hex and emits
## send_hunt_expedition_requested. Mirrors the scout send flow, but targets a herd not a tile.
func _on_send_hunt_expedition_pressed(band: Dictionary, party_workers: int, policy: String) -> void:
    if band.is_empty() or party_workers <= 0:
        return
    var chosen := policy if policy in LABOR_HUNT_POLICIES else DEFAULT_HUNT_POLICY
    _pending_send_hunt_expedition = {
        "band": band.duplicate(true), "party_workers": party_workers, "policy": chosen,
    }
    _refresh_targeting()

func _cancel_pending_send_hunt_expedition() -> void:
    if _pending_send_hunt_expedition.is_empty():
        return
    _pending_send_hunt_expedition = {}
    _refresh_targeting()

func _try_dispatch_pending_send_hunt_expedition(tile_info: Dictionary) -> void:
    if _pending_send_hunt_expedition.is_empty() or tile_info.is_empty():
        return
    # Resolve the target from the clicked hex's herds (herd markers occupy the hex, so a click on a
    # herd lands here). Pick the first huntable herd on the tile; if none, keep targeting and nudge.
    var fauna_id := _huntable_herd_id_on_tile(tile_info)
    if fauna_id == "":
        _note_command_feed("Hunt expedition", "No huntable herd there — click on a herd.")
        return
    var band: Dictionary = _pending_send_hunt_expedition.get("band", {})
    emit_signal("send_hunt_expedition_requested", {
        "faction": int(band.get("faction", PLAYER_FACTION_ID)),
        "band": int(band.get("entity", -1)),
        "party_workers": int(_pending_send_hunt_expedition.get("party_workers", 0)),
        "fauna_id": fauna_id,
        "policy": String(_pending_send_hunt_expedition.get("policy", DEFAULT_HUNT_POLICY)),
    })
    _pending_send_hunt_expedition = {}
    _refresh_targeting()

## The id of the first huntable herd on a clicked hex's tile_info (the herds the tile carries), or
## "" when the hex holds no huntable herd. Used to resolve a hunt-expedition target click.
func _huntable_herd_id_on_tile(tile_info: Dictionary) -> String:
    var herds_variant: Variant = tile_info.get("herds", [])
    if not (herds_variant is Array):
        return ""
    for herd_variant in (herds_variant as Array):
        if herd_variant is Dictionary and bool((herd_variant as Dictionary).get("huntable", false)):
            var id := String((herd_variant as Dictionary).get("id", "")).strip_edges()
            if id != "":
                return id
    return ""

## Clear-all: return every worker to idle (repurposed cancel_order).
func _on_clear_all_pressed(band: Dictionary) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band)

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
    _emit_assign_labor(band, LABOR_KIND_HUNT, idle,
        int(band.get("current_x", -1)), int(band.get("current_y", -1)), herd_id, DEFAULT_HUNT_POLICY)


func update_overlay_legend(legend: Dictionary) -> void:
    # print("[HUD] update_overlay_legend: ", legend.keys())  # Commented out to reduce log spam
    overlay_legend = legend.duplicate(true) if legend is Dictionary else {}
    if legend_suppressed:
        _hide_legend_panel()
        return
    for child in terrain_legend_list.get_children():
        child.queue_free()
    if overlay_legend.is_empty():
        _hide_legend_panel()
        return
    terrain_legend_panel.visible = true
    var title := String(overlay_legend.get("title", "Map Legend"))
    terrain_legend_panel.set_card_title(title)
    var description := String(overlay_legend.get("description", "")).strip_edges()
    if description == "":
        terrain_legend_description.visible = false
        terrain_legend_description.text = ""
    else:
        terrain_legend_description.visible = true
        terrain_legend_description.text = description
    var rows: Array = overlay_legend.get("rows", [])
    if rows.is_empty():
        terrain_legend_panel.visible = false
        terrain_legend_description.visible = false
        terrain_legend_description.text = ""
        return
    var row_height := _legend_row_height()
    var swatch_size := _legend_swatch_size(row_height)
    for entry in rows:
        if typeof(entry) != TYPE_DICTIONARY:
            continue
        var row := HBoxContainer.new()
        row.custom_minimum_size = Vector2(0, row_height)
        row.size_flags_horizontal = Control.SIZE_EXPAND_FILL

        var swatch := ColorRect.new()
        swatch.custom_minimum_size = swatch_size
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        swatch.color = entry.get("color", Color.WHITE)
        row.add_child(swatch)

        var label := Label.new()
        var label_text := str(entry.get("label", ""))
        var value_text := str(entry.get("value_text", "")).strip_edges()
        if value_text != "":
            if label_text == "":
                label.text = value_text
            else:
                label.text = "%s — %s" % [label_text, value_text]
        else:
            label.text = label_text
        label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        row.add_child(label)

        terrain_legend_list.add_child(row)
    _resize_legend_panel(_legend_list_size())

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

func _legend_row_height() -> float:
    return LEGEND_MIN_ROW_HEIGHT + LEGEND_ROW_PADDING

func _legend_swatch_size(row_height: float) -> Vector2:
    var side: float = max(row_height * LEGEND_SWATCH_FRACTION, LEGEND_MIN_ROW_HEIGHT * 0.6)
    return Vector2(side, side)

func _refresh_existing_legend_rows() -> void:
    var row_height := _legend_row_height()
    var swatch_size := _legend_swatch_size(row_height)
    for child in terrain_legend_list.get_children():
        if child is HBoxContainer:
            var row := child as HBoxContainer
            row.custom_minimum_size = Vector2(0, row_height)
            for grandchild in row.get_children():
                if grandchild is ColorRect:
                    (grandchild as ColorRect).custom_minimum_size = swatch_size
    _resize_legend_panel(_legend_list_size())

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

func reset_command_feed() -> void:
    _command_feed_entries.clear()
    _command_feed_signatures.clear()
    _render_command_feed()

func show_tile_selection(tile_info: Dictionary) -> void:
    _selected_tile_info = tile_info.duplicate(true) if tile_info is Dictionary else {}
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(_selected_tile_info, {}, {})
    _try_dispatch_pending_move_band(_selected_tile_info)
    _try_dispatch_pending_send_expedition(_selected_tile_info)
    _try_dispatch_pending_send_hunt_expedition(_selected_tile_info)

func notify_hex_selected(tile_info: Dictionary) -> void:
    if tile_info.is_empty():
        return
    _try_dispatch_pending_move_band(tile_info)
    _try_dispatch_pending_send_expedition(tile_info)
    _try_dispatch_pending_send_hunt_expedition(tile_info)

func show_unit_selection(unit_data: Dictionary) -> void:
    var tile_info: Dictionary = {}
    var tile_variant: Variant = unit_data.get("tile_info", {})
    if tile_variant is Dictionary:
        tile_info = (tile_variant as Dictionary).duplicate(true)
    else:
        tile_info = _selected_tile_info
    _selected_tile_info = tile_info
    _selected_unit = unit_data.duplicate(true)
    _selected_herd.clear()
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, _selected_unit, {})

func show_herd_selection(herd_data: Dictionary) -> void:
    var tile_info: Dictionary = {}
    var tile_variant: Variant = herd_data.get("tile_info", {})
    if tile_variant is Dictionary and not (tile_variant as Dictionary).is_empty():
        tile_info = (tile_variant as Dictionary).duplicate(true)
    elif _herd_matches_selected_tile(herd_data):
        # Same hex as the currently-selected tile (a map click on a hex that has
        # both a gather module and a fauna group): surface Harvest alongside the
        # herd verbs. A herd picked from the inspector (no tile_info, unrelated tile
        # selected) falls through to herd-only so Harvest can't mis-target.
        tile_info = _selected_tile_info
    _selected_tile_info = tile_info
    _selected_herd = herd_data.duplicate(true)
    _selected_unit.clear()
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, {}, _selected_herd)

## True when the currently-selected tile is the same hex the herd occupies, so it
## is safe to keep showing that tile's Harvest verb alongside the herd verbs.
func _herd_matches_selected_tile(herd_data: Dictionary) -> bool:
    if _selected_tile_info.is_empty():
        return false
    return int(_selected_tile_info.get("x", -1)) == int(herd_data.get("x", -2)) \
        and int(_selected_tile_info.get("y", -1)) == int(herd_data.get("y", -2))

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
            _selected_unit = data.duplicate(true) if data is Dictionary else {}
            _selected_herd.clear()
            _adopt_tile_info_from(_selected_unit)
            _render_selection_panel(_selected_tile_info, _selected_unit, {})
        "herd":
            _selected_herd = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _adopt_tile_info_from(_selected_herd)
            _render_selection_panel(_selected_tile_info, {}, _selected_herd)
        "tile":
            _selected_tile_info = data.duplicate(true) if data is Dictionary else {}
            _selected_unit.clear()
            _selected_herd.clear()
            _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()
            _render_selection_panel(_selected_tile_info, {}, {})
        _:
            # Selected occupant vanished (e.g. the band expired). Drop to its last tile
            # if known, else hide both cards. Intentionally does not touch pending state.
            _selected_unit.clear()
            _selected_herd.clear()
            if _selected_tile_info.is_empty():
                if tile_panel != null:
                    tile_panel.visible = false
                if forage_assign_controls != null:
                    forage_assign_controls.visible = false
                _set_occupants_relevant(false)
            else:
                _render_selection_panel(_selected_tile_info, {}, {})

## Pull the fresh tile_info a refresh payload carries alongside the occupant, so the tile
## card + roster render against the same snapshot the occupant came from.
func _adopt_tile_info_from(occupant: Dictionary) -> void:
    var ti_variant: Variant = occupant.get("tile_info", {})
    if ti_variant is Dictionary and not (ti_variant as Dictionary).is_empty():
        _selected_tile_info = (ti_variant as Dictionary).duplicate(true)
    _selected_food_module = String(_selected_tile_info.get("food_module", "")).strip_edges()

func _render_selection_panel(_tile_info: Dictionary, _unit_data: Dictionary, _herd_data: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    # Reset the band-food/morale/output tint context; `_unit_summary_lines` re-sets it if
    # a band is being rendered into the drawer.
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _assemble_roster(_selected_tile_info)
    _render_tile_card(_selected_tile_info)
    _render_occupants_card()

## Assemble the roster for the current hex from the tile's `units`/`herds`, then
## ensure the currently-selected occupant is represented even when the tile_info
## doesn't list it (an inspector-driven herd selection carries an empty tile_info).
func _assemble_roster(tile_info: Dictionary) -> void:
    _roster_units = []
    _roster_herds = []
    var units_variant: Variant = tile_info.get("units", [])
    if units_variant is Array:
        for entry in units_variant:
            if entry is Dictionary:
                _roster_units.append(entry)
    var herds_variant: Variant = tile_info.get("herds", [])
    if herds_variant is Array:
        for entry in herds_variant:
            if entry is Dictionary:
                _roster_herds.append(entry)
    if not _selected_unit.is_empty() and _find_roster_unit(int(_selected_unit.get("entity", -1))).is_empty():
        _roster_units.append(_selected_unit)
    if not _selected_herd.is_empty() and _find_roster_herd(String(_selected_herd.get("id", ""))).is_empty():
        _roster_herds.append(_selected_herd)

## The Tile card: the place. Terrain rows + the "Assign foragers" controls (its only
## action). Kind stays "Tile" even when an occupant is selected.
func _render_tile_card(tile_info: Dictionary) -> void:
    if tile_panel == null or tile_detail == null:
        return
    tile_panel.visible = true
    tile_panel.set_card_kind("Tile")
    var title_text := "—"
    if not tile_info.is_empty():
        title_text = "(%d, %d)" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]
    tile_panel.set_card_title(title_text)
    tile_detail.text = _format_detail_bbcode(_tile_terrain_lines(tile_info))
    _build_forage_assign_controls(tile_info)

## The Occupants card: a selectable roster of bands + wildlife on the hex, plus a
## detail drawer for the selected occupant. Hidden (dock reflows) on an empty hex.
func _render_occupants_card() -> void:
    if occupants_panel == null:
        return
    if _roster_units.is_empty() and _roster_herds.is_empty():
        _set_occupants_relevant(false)
        if allocation_panel != null:
            allocation_panel.visible = false
        if herd_assign_controls != null:
            herd_assign_controls.visible = false
        return
    _set_occupants_relevant(true)
    occupants_panel.set_card_kind("Occupants")
    occupants_panel.set_card_title("on this hex")
    # Auto-select the first occupant on a fresh tile click (nothing selected yet),
    # driving the drawer + the map ring through the same signal a click would.
    if _selected_unit.is_empty() and _selected_herd.is_empty():
        if not _roster_units.is_empty():
            _selected_unit = (_roster_units[0] as Dictionary).duplicate(true)
            emit_signal("roster_occupant_selected", "unit", int(_selected_unit.get("entity", -1)))
        else:
            _selected_herd = (_roster_herds[0] as Dictionary).duplicate(true)
            emit_signal("roster_occupant_selected", "herd", String(_selected_herd.get("id", "")))
    _rebuild_roster()
    _render_occupant_drawer()

func _set_occupants_relevant(relevant: bool) -> void:
    if left_dock != null:
        left_dock.set_relevant(occupants_panel, relevant)
    elif occupants_panel != null:
        occupants_panel.visible = relevant

## Terrain-only tile readout: FoW redaction, Biome/Height/Tags, and the tile's
## gather module relabeled `Forage:` (occupant/harvester/scout listings moved to
## the roster + drawer). Keeps the forage-pending hint here (Forage is a tile action).
func _tile_terrain_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    # Fog of War: never-seen tiles reveal nothing; remembered (Discovered) tiles
    # show only their last-known terrain, not current contents. See MapView
    # _apply_visibility_to_info, which redacts the hidden fields before this runs.
    var visibility_state := String(tile_info.get("visibility_state", ""))
    if visibility_state == "unexplored":
        lines.append("Undiscovered tile")
        lines.append("Not yet scouted — send a band to reveal this area.")
        return lines
    var terrain_label := String(tile_info.get("terrain_label", "Unknown"))
    lines.append("Biome: %s" % terrain_label)
    if tile_info.has("height_display"):
        lines.append("Height: %s" % String(tile_info["height_display"]))
    var tags_text := String(tile_info.get("tags_text", "none"))
    lines.append("Tags: %s" % tags_text)
    # Habitability is terrain-intrinsic (band-independent), so it's fine on a remembered
    # tile — surface it before the discovered early-return. Only when the snapshot carries
    # the field (a rehydrated tile may lack it) so we never invent a rating.
    if tile_info.has("habitability"):
        var drain := float(tile_info["habitability"])
        lines.append("Habitability: %s" % TileHabitability.rating_for(drain))
    # Climate is the tile's latitude+elevation temperature band (informational, not a
    # warning). Terrain-intrinsic, so fine on a remembered tile; only when the snapshot
    # carries the field (a rehydrated tile may lack it) so we never invent a band.
    if tile_info.has("temperature"):
        var temperature := float(tile_info["temperature"])
        lines.append("Climate: %s" % TileClimate.band_for(temperature))
    # A discovered Wondrous Site is known knowledge — fine on a remembered tile — so surface
    # it before the discovered early-return. Only when the field is present.
    var site_name := String(tile_info.get("site_name", "")).strip_edges()
    if site_name != "":
        lines.append("Site: %s" % site_name)
    if visibility_state == "discovered":
        lines.append("Last seen — information incomplete. Scout to update.")
        return lines
    var food_label := String(tile_info.get("food_module_label", "None")).strip_edges()
    if food_label == "":
        food_label = "None"
    var weight: float = float(tile_info.get("food_module_weight", 0.0))
    var food_kind := String(tile_info.get("food_kind", "")).strip_edges()
    var food_line := "Forage: %s" % food_label
    if food_kind != "":
        food_line = "%s — %s" % [food_line, _format_food_kind_label(food_kind)]
    if weight > 0.0:
        food_line += " (weight %.2f)" % weight
    lines.append(food_line)
    return lines

# ---- Occupants roster ------------------------------------------------------

## Rebuild the roster rows: a `Bands (N)` sub-group and a `Wildlife (N)` sub-group,
## each a dim uppercase header + one selectable row per occupant. The row matching
## the current selection is styled as selected.
func _rebuild_roster() -> void:
    if roster_list == null:
        return
    for child in roster_list.get_children():
        child.queue_free()
    if not _roster_units.is_empty():
        roster_list.add_child(_roster_group_header("Bands", _roster_units.size()))
        for unit in _roster_units:
            roster_list.add_child(_build_band_row(unit))
    if not _roster_herds.is_empty():
        roster_list.add_child(_roster_group_header("Wildlife", _roster_herds.size()))
        for herd in _roster_herds:
            roster_list.add_child(_build_herd_row(herd))

func _roster_group_header(title: String, count: int) -> Label:
    var label := Label.new()
    label.text = "%s (%d)" % [title.to_upper(), count]
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", ROSTER_HEADER_FONT_SIZE)
    return label

## One selectable band row. A Button (row click) hosts a mouse-transparent HBox
## laying out: a selection accent, a vitality dot (BandFoodStatus color for a
## player band, neutral for others), the name, the size, and an activity glyph.
func _build_band_row(unit: Dictionary) -> Button:
    var entity_id := int(unit.get("entity", -1))
    var is_player := _is_player_unit(unit)
    var selected := not _selected_unit.is_empty() and int(_selected_unit.get("entity", -1)) == entity_id
    # Neutral tint for a non-player band's vitality dot (we can't see their larder).
    var dot_color := HudStyle.INK_FAINT
    var glyph := ""
    if is_player:
        dot_color = BandFoodStatus.color_for_days(float(unit.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS)))
        glyph = _activity_glyph(String(unit.get("activity", "")))
    var button := _make_roster_button(selected)
    var row := _make_roster_row(selected, dot_color)
    row.add_child(_roster_name_label(String(unit.get("id", "Band")), selected))
    row.add_child(_roster_meta_label(str(int(unit.get("size", 0)))))
    if glyph != "":
        row.add_child(_roster_glyph_label(glyph, String(unit.get("activity", "")) == BAND_ACTIVITY_IDLE))
    # Surface the data-driven settlement-stage label (e.g. "Nomadic band") on hover; omit when
    # the band has no resolved stage (pre-stage / missing snapshot).
    var stage_label := String(unit.get("settlement_stage_label", "")).strip_edges()
    if stage_label != "":
        button.tooltip_text = stage_label
    button.add_child(row)
    button.pressed.connect(_on_roster_row_selected.bind("unit", entity_id))
    return button

## One selectable wildlife row: an ecology-tier dot, the species glyph + name, and
## the size-class label. Selecting it drives the drawer + the map ring to the herd.
func _build_herd_row(herd: Dictionary) -> Button:
    var herd_id := String(herd.get("id", ""))
    var selected := not _selected_herd.is_empty() and String(_selected_herd.get("id", "")) == herd_id
    var dot_color := _ecology_tier_color(String(herd.get("ecology_phase", "")))
    var button := _make_roster_button(selected)
    var row := _make_roster_row(selected, dot_color)
    var label := String(herd.get("label", herd.get("id", "Herd")))
    var glyph := FoodIcons.for_herd(label)
    var name_text := String(herd.get("species", label))
    row.add_child(_roster_name_label("%s %s" % [glyph, name_text], selected))
    var size_class := String(herd.get("size_class", "")).strip_edges()
    if size_class != "":
        row.add_child(_roster_meta_label("%s game" % size_class.capitalize()))
    button.tooltip_text = label
    button.add_child(row)
    button.pressed.connect(_on_roster_row_selected.bind("herd", herd_id))
    return button

## A roster row's clickable Button shell: selected rows read as "primary", others
## as "ghost". Toggle_mode is off — selection is driven by a rebuild, not the
## button's own toggle state, so re-clicking the selected row can't un-highlight it.
func _make_roster_button(selected: bool) -> Button:
    var button := Button.new()
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(0, ROSTER_ROW_MIN_HEIGHT)
    HudStyle.apply_button(button, "primary" if selected else "ghost")
    return button

## The mouse-transparent HBox overlaying a roster button, anchored to fill it,
## carrying the left selection accent + the vitality/ecology dot.
func _make_roster_row(selected: bool, dot_color: Color) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.set_anchors_preset(Control.PRESET_FULL_RECT)
    row.offset_left = ROSTER_ROW_H_PADDING
    row.offset_right = -ROSTER_ROW_H_PADDING
    row.add_theme_constant_override("separation", ROSTER_ROW_SEPARATION)
    var accent := ColorRect.new()
    accent.custom_minimum_size = Vector2(ROSTER_ACCENT_WIDTH, 0)
    accent.color = HudStyle.SIGNAL if selected else Color(0, 0, 0, 0)
    accent.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(accent)
    var dot := ColorRect.new()
    dot.custom_minimum_size = Vector2(ROSTER_DOT_SIZE, ROSTER_DOT_SIZE)
    dot.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    dot.color = dot_color
    dot.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(dot)
    return row

func _roster_name_label(text: String, selected: bool) -> Label:
    var label := Label.new()
    label.text = text
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK if selected else HudStyle.INK_DIM)
    return label

func _roster_meta_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    return label

func _roster_glyph_label(glyph: String, dim: bool) -> Label:
    var label := Label.new()
    label.text = glyph
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT if dim else HudStyle.INK_DIM)
    return label

func _activity_glyph(activity: String) -> String:
    return String(ACTIVITY_GLYPHS.get(activity.strip_edges().to_lower(), ACTIVITY_GLYPHS[BAND_ACTIVITY_IDLE]))

## Shared green/amber/red tier for a herd's ecology phase, matching the band
## food dot so map/roster/drawer agree: thriving→green, stressed→amber,
## collapsing→red. Matched on the phase stems from `EcologyPhase::as_str`.
func _ecology_tier_color(phase: String) -> Color:
    var normalized := phase.strip_edges().to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER
    if normalized.contains("stress"):
        return HudStyle.WARN
    return HudStyle.HEALTHY

func _find_roster_unit(entity_id: int) -> Dictionary:
    for unit in _roster_units:
        if unit is Dictionary and int((unit as Dictionary).get("entity", -1)) == entity_id:
            return unit
    return {}

func _find_roster_herd(herd_id: String) -> Dictionary:
    if herd_id == "":
        return {}
    for herd in _roster_herds:
        if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
            return herd
    return {}

## A roster row was clicked: make it the selected occupant, refresh the cards, and
## notify the map so the selection ring follows.
func _on_roster_row_selected(kind: String, id: Variant) -> void:
    _select_roster_occupant(kind, id)
    emit_signal("roster_occupant_selected", kind, id)

func _select_roster_occupant(kind: String, id: Variant) -> void:
    if kind == "unit":
        _selected_unit = _find_roster_unit(int(id)).duplicate(true)
        _selected_herd = {}
    else:
        _selected_herd = _find_roster_herd(String(id)).duplicate(true)
        _selected_unit = {}
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    _rebuild_roster()
    _render_occupant_drawer()

## The detail drawer + action buttons for the currently-selected occupant.
func _render_occupant_drawer() -> void:
    if occupant_detail == null:
        return
    _selected_band_food_days = NAN
    _selected_band_morale = NAN
    _selected_band_output = NAN
    var lines: Array[String] = []
    if not _selected_unit.is_empty():
        lines = _unit_summary_lines(_selected_unit)
    elif not _selected_herd.is_empty():
        lines = _herd_summary_lines(_selected_herd)
    occupant_detail.text = _format_detail_bbcode(lines)
    var is_band := not _selected_unit.is_empty()
    var is_herd := not _selected_herd.is_empty()
    # Expedition → dedicated Recall/Move panel (no labor in v1); player band → labor allocation
    # panel; herd → assign-hunters controls. All mutually exclusive with the current selection.
    if is_band and bool(_selected_unit.get("is_expedition", false)):
        _build_expedition_panel(_selected_unit)
    elif is_band and _is_player_unit(_selected_unit):
        _build_allocation_panel(_selected_unit)
    elif allocation_panel != null:
        allocation_panel.visible = false
    if is_herd:
        _build_herd_assign_controls(_selected_herd)
    elif herd_assign_controls != null:
        herd_assign_controls.visible = false

## Player-faction check for a roster/drawer band (mirrors MapView._is_player_unit).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", PLAYER_FACTION_ID)) == PLAYER_FACTION_ID

func _unit_summary_lines(unit_data: Dictionary) -> Array[String]:
    if bool(unit_data.get("is_expedition", false)):
        return _expedition_summary_lines(unit_data)
    var lines: Array[String] = []
    var label := String(unit_data.get("id", "Band"))
    lines.append("Unit: %s" % label)
    var size_value: int = int(unit_data.get("size", 0))
    lines.append("Size: %d" % size_value)
    lines.append(_band_food_line(unit_data))
    # Morale is our own bands' business only (a non-player band's morale isn't ours
    # to see); morale drives productivity + migration (a harsh tile erodes it until
    # people begin leaving), while deaths stay starvation/cold-driven.
    if _is_player_unit(unit_data):
        lines.append(_band_morale_line(unit_data))
        # Productivity ties visibly to morale: show the Output row when discontent is
        # dragging yield below full (near Morale, tinted by how low it is).
        var output_line := _band_output_line(unit_data)
        if output_line != "":
            lines.append(output_line)
        # When morale is concerning/declining, itemize why (the Layer-1 contributions)
        # and name the real recovery levers.
        lines.append_array(_morale_breakdown_lines(unit_data))
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    # Per-source labor is now shown by the allocation panel (a real −/+ control set),
    # not as drawer text; the old single-task harvest/scout summaries are retired.
    var stockpile_variant: Variant = unit_data.get("accessible_stockpile", {})
    if stockpile_variant is Dictionary:
        var stockpile_lines := _accessible_stockpile_lines(stockpile_variant)
        if not stockpile_lines.is_empty():
            lines.append("")
            lines.append_array(stockpile_lines)
    return lines

## Drawer readout for a selected expedition (docs/plan_exploration_and_sites.md §2 / §2b):
## mission, humanized phase, party size, and carried food (from stores/daysOfFood). A hunt
## expedition (§2b) also lists the target herd it follows. Expeditions have no labor in v1, so
## this replaces the band's labor/morale rows entirely.
func _expedition_summary_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var mission := String(unit_data.get("expedition_mission", ""))
    var is_hunt := mission == EXPEDITION_MISSION_HUNT
    lines.append("Unit: %s" % String(unit_data.get("id", "Expedition")))
    lines.append("Mission: %s" % _expedition_mission_label(mission))
    if is_hunt:
        # The migratory herd it follows (species label from the fauna_id, falling back to the id).
        var herd_id := String(unit_data.get("expedition_target_herd", "")).strip_edges()
        if herd_id != "":
            lines.append("Target: %s" % _herd_label_for_id(herd_id))
        # The launched take policy (Sustain/Surplus/Market/Eradicate).
        var policy := String(unit_data.get("expedition_hunt_policy", "")).strip_edges()
        if policy != "":
            lines.append("Policy: %s" % policy.capitalize())
    var phase := String(unit_data.get("expedition_phase", "")).strip_edges()
    if phase != "":
        lines.append("Phase: %s" % _expedition_phase_label(phase))
    lines.append("Party: %d" % int(unit_data.get("size", 0)))
    # Food it carries — larder-drawn provisions for a scout, the hunted haul for a hunt party —
    # days from daysOfFood. Reuse the food-days tint context (`_selected_band_food_days`, read
    # back in `_format_detail_bbcode`).
    var days: float = float(unit_data.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
    _selected_band_food_days = days
    var carried := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        if is_hunt:
            # The hunt party lives off its own kills; its store item key isn't fixed, so total it.
            for qty in (stores_variant as Dictionary).values():
                carried += int(round(float(qty)))
        else:
            carried = int(round(float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))))
    if is_hunt:
        # Carried X / cap + a FULL badge at the carry ceiling (the party heads home when full).
        var cap := int(round(float(unit_data.get("expedition_carry_cap", 0.0))))
        if cap > 0:
            var full_badge := "  %s" % HUNT_FULL_BADGE if carried >= cap else ""
            lines.append("Carried: %d / %d  (%s)%s" % [carried, cap, _food_days_text(days), full_badge])
        else:
            lines.append("Carried: %d  (%s)" % [carried, _food_days_text(days)])
    else:
        lines.append("Provisions: %d  (%s)" % [carried, _food_days_text(days)])
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    return lines

## Humanize an expedition mission id ("scout" → "Scouting expedition"); falls back to a
## capitalized token for an unknown/future mission (e.g. PR 2's "hunt").
func _expedition_mission_label(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if EXPEDITION_MISSION_LABELS.has(key):
        return EXPEDITION_MISSION_LABELS[key]
    return key.capitalize() if key != "" else "Expedition"

## Humanize an expedition phase id ("awaiting" → "Awaiting orders").
func _expedition_phase_label(phase: String) -> String:
    var key := phase.strip_edges().to_lower()
    if EXPEDITION_PHASE_LABELS.has(key):
        return EXPEDITION_PHASE_LABELS[key]
    return key.capitalize()

## Selection-panel band food row: "Food  <provisions>  (<days>)" — provisions from
## the band's larder stores, days from `days_of_food` (∞ when not food-limited).
## Stashes the days on `_selected_band_food_days` so `_format_detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
func _band_food_line(unit_data: Dictionary) -> String:
    var days: float = float(unit_data.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
    _selected_band_food_days = days
    var provisions := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        provisions = int(round(float((stores_variant as Dictionary).get(STORE_ITEM_PROVISIONS, 0.0))))
    return "Food: %d  (%s)" % [provisions, _food_days_text(days)]

## Selection-panel band morale row: "Morale: 41% ▼ — harsh terrain (Karst Cavern Mouth)".
## Morale, its per-turn trend, and the dominant cause come from the snapshot cohort dict
## (decoded in `native/src/lib.rs population_to_dict`). A falling trend appends the named
## cause; Terrain names the band's tile (the "it's the hex you're on" payload). A rehydrated
## save reports delta 0 / cause None for one turn, so the row degrades to a bare percentage.
## Stashes morale on `_selected_band_morale` so `_format_detail_bbcode` tints the value.
func _band_morale_line(unit_data: Dictionary) -> String:
    var morale: float = float(unit_data.get("morale", 1.0))
    _selected_band_morale = morale
    var text := "Morale: %d%%" % int(round(morale * 100.0))
    var delta: float = float(unit_data.get("morale_delta", 0.0))
    if delta <= -MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_FALLING_GLYPH
        # Name the cause only when morale is actually concerning — a healthy band
        # drifting slowly (nearly every tile bleeds a little today) shouldn't be
        # branded "harsh climate/terrain". Below the warn threshold, spell it out.
        if morale < BandFoodStatus.warn_morale():
            var cause := int(unit_data.get("morale_cause", MORALE_CAUSE_NONE))
            var cause_label := _morale_cause_label(cause)
            if cause_label != "":
                if cause == MORALE_CAUSE_TERRAIN:
                    var terrain_label := String(_selected_tile_info.get("terrain_label", "")).strip_edges()
                    if terrain_label != "":
                        cause_label = "%s (%s)" % [cause_label, terrain_label]
                text += " — %s" % cause_label
    elif delta >= MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_RISING_GLYPH
    return text

## Selection-panel band productivity row: "Output: 56%" — the modifier-stack result
## (snapshot `output_multiplier`, discontent being Phase 1's sole modifier). Only shown
## below full output; stashes the value on `_selected_band_output` so `_format_detail_bbcode`
## tints it by the output.{warn,critical} buckets (ink → amber → red).
func _band_output_line(unit_data: Dictionary) -> String:
    var output: float = float(unit_data.get("output_multiplier", OUTPUT_FULL))
    if output >= OUTPUT_FULL:
        return ""
    _selected_band_output = output
    return "Output: %d%%" % int(round(output * 100.0))

## True when the band's morale warrants surfacing the itemized breakdown + recovery
## guidance: below the warn threshold, or falling by more than the trend epsilon.
func _morale_is_concerning(unit_data: Dictionary) -> bool:
    var morale := float(unit_data.get("morale", 1.0))
    var delta := float(unit_data.get("morale_delta", 0.0))
    return morale < BandFoodStatus.warn_morale() or delta <= -MORALE_TREND_EPSILON

## Itemized morale breakdown: the four signed Layer-1 contributions (their sum IS
## morale_delta) as indented sub-lines, plus a recovery-guidance line. Shown only when
## morale is concerning/declining. Each contribution above the breakdown epsilon renders
## as `    ▲ +1.0%  settling`; `_format_detail_bbcode` tints the row by its sign glyph.
func _morale_breakdown_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if not _morale_is_concerning(unit_data):
        return lines
    var terrain_label := String(_selected_tile_info.get("terrain_label", "")).strip_edges()
    var terrain_row_label := MORALE_CAUSE_LABEL_TERRAIN
    if terrain_label != "":
        terrain_row_label = "%s (%s)" % [MORALE_CAUSE_LABEL_TERRAIN, terrain_label]
    var unrest_value := float(unit_data.get("morale_unrest", 0.0))
    # (value, label) in the display order of the spec: settling, terrain, climate, unrest.
    var contributions := [
        [float(unit_data.get("morale_settling", 0.0)), MORALE_CONTRIB_LABEL_SETTLING],
        [float(unit_data.get("morale_terrain", 0.0)), terrain_row_label],
        [float(unit_data.get("morale_climate", 0.0)), MORALE_CAUSE_LABEL_COLD],
        [unrest_value, MORALE_CONTRIB_LABEL_CULTURE if unrest_value > 0.0 else MORALE_CAUSE_LABEL_UNREST],
    ]
    var epsilon := BandFoodStatus.morale_breakdown_epsilon()
    for entry in contributions:
        var value: float = entry[0]
        if absf(value) < epsilon:
            continue
        var glyph := MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else MORALE_CONTRIB_NEGATIVE_GLYPH
        var sign_str := "+" if value > 0.0 else "−"
        lines.append("%s%s %s%.1f%%  %s" % [
            MORALE_BREAKDOWN_INDENT, glyph, sign_str, absf(value) * 100.0, entry[1],
        ])
    lines.append(RECOVERY_GUIDANCE_TEXT)
    return lines

## Plain-language label for a morale cause (0=None,1=Terrain,2=Cold,3=Unrest); "" for None
## or unknown. Shared by the drawer morale line and the losing-population alert reason.
func _morale_cause_label(cause: int) -> String:
    match cause:
        MORALE_CAUSE_TERRAIN:
            return MORALE_CAUSE_LABEL_TERRAIN
        MORALE_CAUSE_COLD:
            return MORALE_CAUSE_LABEL_COLD
        MORALE_CAUSE_UNREST:
            return MORALE_CAUSE_LABEL_UNREST
        _:
            return ""

## Human-readable days-of-food: the ∞ glyph when the band is not food-limited,
## otherwise a whole-day count.
func _food_days_text(days: float) -> String:
    if not BandFoodStatus.is_limited(days):
        return FOOD_UNLIMITED_GLYPH
    return "%d days" % int(round(days))

func _format_food_module_label(module_key: String) -> String:
    if module_key == "":
        return "Unknown"
    return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

func _format_stockpile_label(raw_value: String) -> String:
    var trimmed := raw_value.strip_edges()
    if trimmed == "":
        return "Stockpile"
    var tokens: PackedStringArray = trimmed.split("_", false)
    if tokens.is_empty():
        return trimmed.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return trimmed.capitalize()
    return " ".join(parts)

func _build_stockpile_row(entry: Dictionary) -> Control:
    var row := HBoxContainer.new()
    row.custom_minimum_size = Vector2(0, 24)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var label := Label.new()
    label.text = String(entry.get("label", "Stockpile"))
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(label)
    var amount_label := Label.new()
    amount_label.text = str(entry.get("amount", 0))
    amount_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    amount_label.custom_minimum_size = Vector2(60, 0)
    row.add_child(amount_label)
    var delta := float(entry.get("delta", 0.0))
    if not is_equal_approx(delta, 0.0):
        var delta_label := Label.new()
        delta_label.text = ("+%.0f" % delta) if delta > 0.0 else ("%.0f" % delta)
        delta_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        delta_label.custom_minimum_size = Vector2(60, 0)
        delta_label.modulate = Color(0.6, 0.9, 0.6) if delta > 0.0 else Color(0.95, 0.6, 0.5)
        row.add_child(delta_label)
    return row

func _accessible_stockpile_lines(stockpile: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var radius := int(stockpile.get("radius", 0))
    var entries_variant: Variant = stockpile.get("entries", [])
    var entries: Array = entries_variant if entries_variant is Array else []
    if entries.is_empty():
        return lines
    var formatted: Array[String] = []
    for entry in entries:
        if not (entry is Dictionary):
            continue
        var item := String(entry.get("item", ""))
        var qty := int(entry.get("quantity", 0))
        if item == "" and qty == 0:
            continue
        formatted.append("%d %s" % [qty, _format_stockpile_label(item)])
    if formatted.is_empty():
        return lines
    lines.append("Stockpile: radius %d" % radius)
    lines.append("Available: %s" % ", ".join(formatted))
    return lines

func _format_food_kind_label(kind_value: String) -> String:
    if kind_value == "":
        return ""
    var tokens: PackedStringArray = kind_value.split("_", false)
    if tokens.is_empty():
        return kind_value.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return kind_value.capitalize()
    return " ".join(parts)

func _herd_summary_lines(herd_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var label: String = String(herd_data.get("label", herd_data.get("id", "Herd")))
    lines.append("Herd: %s" % label)
    var species := String(herd_data.get("species", ""))
    if species != "":
        lines.append("Species: %s" % species)
    var size_class := String(herd_data.get("size_class", "")).strip_edges()
    if size_class != "":
        lines.append("Size: %s game" % size_class.capitalize())
    var biomass: float = float(herd_data.get("biomass", 0.0))
    if biomass > 0.0:
        lines.append("Biomass: %.0f" % biomass)
    var phase := String(herd_data.get("ecology_phase", "")).strip_edges().to_lower()
    if phase != "":
        lines.append("Ecology: %s" % _ecology_phase_label(phase))
    var domestication := float(herd_data.get("domestication", 0.0))
    if domestication > 0.0:
        lines.append("Husbandry: %s" % _husbandry_label(domestication))
    var x := int(herd_data.get("x", -1))
    var y := int(herd_data.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Position: (%d, %d)" % [x, y])
    var next_x := int(herd_data.get("next_x", -1))
    var next_y := int(herd_data.get("next_y", -1))
    if next_x >= 0 and next_y >= 0:
        lines.append("Next waypoint: (%d, %d)" % [next_x, next_y])
    return lines

## Player-facing label for a herd's ecology phase. Stressed/Collapsing carry a warning
## glyph; `_format_detail_bbcode` additionally tints the value (see `_ecology_value_hex`).
func _ecology_phase_label(phase: String) -> String:
    match phase:
        "collapsing":
            return "⚠ Collapsing"
        "stressed":
            return "⚠ Stressed"
        "thriving":
            return "Thriving"
        _:
            return phase.capitalize()

## BBCode hex for an "Ecology" value: red for a collapsing group, amber for stressed,
## normal ink otherwise. Matched on the lowercased phase stems ("collaps"/"stress" from
## `EcologyPhase::as_str`) so tinting survives glyph/capitalization tweaks to the label.
func _ecology_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER_HEX
    if normalized.contains("stress"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Player-facing husbandry label from domestication progress (0.0–1.0). Fully tamed shows
## a livestock glyph; in-progress shows the percentage. `_format_detail_bbcode` tints a
## Domesticated value via `_husbandry_value_hex`.
func _husbandry_label(progress: float) -> String:
    if progress >= 1.0:
        return "🐄 Domesticated"
    return "Domesticating %d%%" % int(round(progress * 100.0))

## BBCode hex for a "Husbandry" value: signal (positive) for a domesticated herd, normal
## ink while it's still being tamed. Matched on the label produced by `_husbandry_label`.
func _husbandry_value_hex(value: String) -> String:
    if value.to_lower().contains("domesticated"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

func _join_lines(lines: Array) -> String:
    var packed := PackedStringArray()
    for line in lines:
        packed.append(String(line))
    return "\n".join(packed)

## Render the selection detail lines as BBCode: consecutive "Key: value" rows
## become a 2-column table (dim key, bright value; Food value in amber) so the
## data aligns into columns, while sentences/section lines stay full-width and
## muted. Matches the mockup's Tile Banner body.
func _format_detail_bbcode(lines: Array) -> String:
    var out := ""
    var table_open := false
    for raw in lines:
        var line := String(raw)
        if line == "":
            if table_open:
                out += "[/table]"
                table_open = false
            out += "\n"
            continue
        # Itemized morale breakdown sub-lines render full-width, tinted by their sign
        # glyph (▲ positive = healthy, ▼ negative = amber) — kept two-tone, not a rainbow.
        if line.begins_with(MORALE_BREAKDOWN_INDENT):
            if table_open:
                out += "[/table]"
                table_open = false
            var row_hex := HudStyle.HEALTHY_HEX if line.contains(MORALE_CONTRIB_POSITIVE_GLYPH) else HudStyle.WARN_HEX
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        var kv := _split_detail_kv(line)
        if kv.is_empty():
            if table_open:
                out += "[/table]"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.INK_DIM_HEX, line]
        else:
            if not table_open:
                out += "[table=2]"
                table_open = true
            var value_hex := HudStyle.INK_HEX
            if String(kv[0]) == "Food" or String(kv[0]) == "Provisions" or String(kv[0]) == "Carried":
                # The band larder / expedition provisions / hunt-party carried-food row tints by the
                # food-days thresholds; its value carries a day count or the ∞ glyph.
                var food_value := String(kv[1])
                if not is_nan(_selected_band_food_days) and (food_value.contains("day") or food_value.contains(FOOD_UNLIMITED_GLYPH)):
                    value_hex = BandFoodStatus.hex_for_days(_selected_band_food_days)
            elif String(kv[0]) == "Morale":
                # The player band's morale row tints by the morale thresholds.
                if not is_nan(_selected_band_morale):
                    value_hex = BandFoodStatus.hex_for_morale(_selected_band_morale)
            elif String(kv[0]) == "Output":
                # The productivity row tints by the output buckets (ink → amber → red).
                if not is_nan(_selected_band_output):
                    value_hex = BandFoodStatus.hex_for_output(_selected_band_output)
            elif String(kv[0]) == "Forage":
                # The tile's gather module reads in the success/ETA amber.
                value_hex = HudStyle.WARN_HEX
            elif String(kv[0]) == "Habitability":
                # The tile's habitability rating tints by its bucket (green→red).
                value_hex = TileHabitability.hex_for_rating(String(kv[1]))
            elif String(kv[0]) == "Ecology":
                value_hex = _ecology_value_hex(String(kv[1]))
            elif String(kv[0]) == "Husbandry":
                value_hex = _husbandry_value_hex(String(kv[1]))
            out += "[cell][color=#%s]%s[/color][/cell][cell][color=#%s]%s[/color][/cell]" % [
                HudStyle.INK_DIM_HEX, kv[0], value_hex, kv[1],
            ]
    if table_open:
        out += "[/table]"
    return out

## Split a "Key: value" data line into [key, value]; returns [] for sentence
## lines (trailing period), long keys, or non-matching text so those stay
## full-width rather than becoming a lopsided table row.
func _split_detail_kv(line: String) -> Array:
    if line.ends_with("."):
        return []
    # The recovery-guidance line reads as a dim sentence, not a lopsided table row.
    if line.begins_with(RECOVERY_GUIDANCE_GLYPH):
        return []
    var idx := line.find(": ")
    if idx <= 0:
        return []
    var key := line.substr(0, idx)
    if key.length() > 16:
        return []
    var value := line.substr(idx + 2)
    if value.strip_edges() == "":
        return []
    return [key, value]

func clear_selection() -> void:
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = ""
    _selected_food_is_hunt = false
    # Keep pending move-band so the user can still choose a destination after deselecting.
    if _selected_tile_info.is_empty():
        if tile_panel != null:
            tile_panel.visible = false
        if forage_assign_controls != null:
            forage_assign_controls.visible = false
        _set_occupants_relevant(false)
    else:
        _render_selection_panel(_selected_tile_info, {}, {})
    if allocation_panel != null:
        allocation_panel.visible = false
    if herd_assign_controls != null:
        herd_assign_controls.visible = false

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

func ingest_command_events(events_variant: Variant) -> void:
    if command_feed_label == null or not (events_variant is Array):
        return
    var events_array: Array = events_variant
    for entry_variant in events_array:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var tick: int = int(entry.get("tick", -1))
        var kind: String = String(entry.get("kind", "")).strip_edges()
        var label: String = String(entry.get("label", "")).strip_edges()
        var detail: String = String(entry.get("detail", "")).strip_edges()
        var signature := "%d|%s|%s|%s" % [tick, kind, label, detail]
        if _command_feed_signatures.has(signature):
            continue
        _command_feed_signatures[signature] = true
        _append_command_feed_entry(tick, kind, label, detail)
    _render_command_feed()

## Rebuild the actionable-alerts list from the player faction's bands each
## snapshot. Alerts are (band, type) deduped by construction — each band yields at
## most one of each type — and cleared automatically when the condition resolves
## (the list is rebuilt from scratch). Population loss is detected against the
## per-band sizes remembered from the previous snapshot.
func update_band_alerts(populations_variant: Variant) -> void:
    if not (populations_variant is Array):
        return
    var populations: Array = populations_variant
    var new_sizes: Dictionary = {}
    # Turn-orb attention registry: one loop over the player faction feeds three producers
    # per band (starving / losing_population / idle_workers). Pushed to the orb below, which
    # severity-sorts (critical floats up). New producers (wars/decisions/…) append here later.
    var attention: Array = []
    var band_index := 0
    # Capture the player bands each snapshot; the labor-allocation UI targets them (assign/move/
    # clear) and reads their labor_assignments for the herd/tile assign controls. `player_band`
    # (first) stays the default actor; `player_bands` backs the assign controls' band-picker.
    var player_band: Dictionary = {}
    var player_bands: Array = []
    for entry_variant in populations:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        if int(entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        band_index += 1
        if player_band.is_empty():
            player_band = entry
        player_bands.append(entry)
        var entity := int(entry.get("entity", -1))
        var size := int(entry.get("size", 0))
        var days := float(entry.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
        var morale := float(entry.get("morale", 1.0))
        var morale_cause := int(entry.get("morale_cause", MORALE_CAUSE_NONE))
        var last_emigrated := int(entry.get("last_emigrated", 0))
        var x := int(entry.get("current_x", -1))
        var y := int(entry.get("current_y", -1))
        var band_name := _band_display_name(entry, band_index)
        new_sizes[entity] = size
        # Producer 1 — starving: larder below the critical threshold (red/critical).
        if BandFoodStatus.is_critical(days):
            attention.append({
                "kind": ATTENTION_KIND_STARVING,
                "severity": ATTENTION_SEVERITY_CRITICAL,
                "label": "%s starving" % band_name,
                "detail": _food_days_text(days),
                "x": x, "y": y,
            })
        # Producer 2 — losing population: shrank vs the previous snapshot (amber/warn).
        if _prev_band_sizes.has(entity) and size < int(_prev_band_sizes[entity]):
            attention.append({
                "kind": ATTENTION_KIND_LOSING_POPULATION,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": "%s losing population" % band_name,
                "detail": _decline_reason(days, morale, morale_cause, last_emigrated),
                "x": x, "y": y,
            })
        # Producer 3 — idle labor: working-age workers unassigned (amber/warn). Supersedes
        # the old activity==idle alert (a worker count is more actionable than a state flag).
        var idle_workers := int(entry.get("idle_workers", 0))
        if idle_workers > 0:
            attention.append({
                "kind": ATTENTION_KIND_IDLE_WORKERS,
                "severity": ATTENTION_SEVERITY_WARN,
                "label": "%d idle worker%s" % [idle_workers, "" if idle_workers == 1 else "s"],
                "detail": band_name,
                "x": x, "y": y,
            })
    _prev_band_sizes = new_sizes
    _player_band = player_band
    _player_bands = player_bands
    if turn_orb != null:
        turn_orb.set_attention(attention)
    # This snapshot is authoritative: drop optimistic pending actions the server has now
    # processed (issued on an older turn), then let the panels render the confirmed state.
    _reconcile_pending()
    # Keep the on-screen allocation panel / assign controls live as the band's staffing
    # changes turn to turn (the coordinator re-renders occupant/tile cards separately, but
    # a herd/tile selection reads _player_band, which only just refreshed here).
    if not _selected_herd.is_empty():
        _build_herd_assign_controls(_selected_herd)
    elif not _selected_tile_info.is_empty() and _selected_unit.is_empty():
        _build_forage_assign_controls(_selected_tile_info)

## Why a band is shrinking: a food crisis (larder below critical) reads "starving" first;
## then, since morale no longer kills (discontent relocates people — see
## docs/plan_civ_wellbeing.md), a shrink with emigrants last turn reads "people leaving".
## Otherwise the dominant morale cause names it in plain language ("harsh terrain" /
## "harsh climate" / "unrest"). When no cause is attributed (morale steady/rising — e.g.
## a rehydrated save, or shrinkage from cold deaths / an aging cohort at healthy morale)
## only say "low morale" if morale is actually low, else leave it plain rather than
## asserting a false reason.
func _decline_reason(days: float, morale: float, morale_cause: int, last_emigrated: int) -> String:
    if BandFoodStatus.is_limited(days) and days < BandFoodStatus.critical_days():
        return DECLINE_REASON_STARVING
    if last_emigrated > 0:
        return DECLINE_REASON_PEOPLE_LEAVING
    var cause_label := _morale_cause_label(morale_cause)
    if cause_label != "":
        return cause_label
    if morale < BandFoodStatus.warn_morale():
        return DECLINE_REASON_LOW_MORALE
    return ""

## Best-effort readable band name: a positional "Band N". (Cohorts carry no top-level
## band label in the snapshot yet — see the server-side follow-up.)
func _band_display_name(_entry: Dictionary, index: int) -> String:
    return "Band %d" % index

## Post a short local note to the command feed (no server round-trip) — used when a
## client-side shortcut can't act (e.g. quick-hunt with no idle workers) so it never
## silently no-ops.
func _note_command_feed(label: String, detail: String) -> void:
    _append_command_feed_entry(-1, "", label, detail)
    _render_command_feed()

func _append_command_feed_entry(tick: int, kind: String, label: String, detail: String) -> void:
    var prefix := kind.capitalize() if kind != "" else "Command"
    var summary := label if label != "" else prefix
    var turn_fragment := ""
    if tick >= 0:
        turn_fragment = "[color=#8fd4ff]Turn %d[/color]  " % tick
    var message := "%s[b]%s[/b]" % [turn_fragment, prefix]
    if summary != "" and summary != prefix:
        message += " — %s" % summary
    if detail != "":
        message += "\n[i]%s[/i]" % detail
    _command_feed_entries.append(message)
    while _command_feed_entries.size() > COMMAND_FEED_LIMIT:
        _command_feed_entries.pop_front()

func _render_command_feed() -> void:
    if command_feed_panel == null or command_feed_label == null:
        return
    command_feed_panel.visible = true
    if _command_feed_entries.is_empty():
        command_feed_label.text = "[i]No command activity yet.[/i]"
    else:
        command_feed_label.text = "\n\n".join(_command_feed_entries)
    # The feed grows to fit but stays within the dock so only it scrolls, not the
    # whole stack; the label needs a frame to re-lay out before its content height
    # and position are accurate.
    call_deferred("_resize_command_feed")

## Grow the feed's scroll region to fit its entries, capped to the space
## remaining in the dock below the panels above it (so the feed scrolls
## internally rather than dragging the fixed panels through the dock scroll),
## then scroll to the newest (bottom) entry.
func _resize_command_feed() -> void:
    if command_feed_scroll == null or command_feed_label == null:
        return
    var cap: float = command_feed_label.get_content_height()
    if left_dock_scroll != null and left_dock_scroll.size.y > 0.0:
        var top_in_dock: float = command_feed_scroll.global_position.y - left_dock_scroll.global_position.y
        var available: float = left_dock_scroll.size.y - top_in_dock - COMMAND_FEED_BOTTOM_MARGIN
        cap = min(cap, max(available, COMMAND_FEED_MIN_HEIGHT))
    command_feed_scroll.custom_minimum_size.y = max(cap, 0.0)
    command_feed_scroll.set_deferred("scroll_vertical", 1000000)

func _refresh_victory_status() -> void:
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

func _legend_list_size() -> Vector2:
    if terrain_legend_list == null:
        return Vector2.ZERO
    return terrain_legend_list.get_combined_minimum_size()

## Cap the legend's inner scroll so a long list scrolls internally instead of
## stretching the whole right dock. Width and placement come from the PanelCard
## + dock; this only bounds the row list's height.
func _resize_legend_panel(_list_size: Vector2) -> void:
    if terrain_legend_scroll == null or terrain_legend_list == null:
        return
    var list_height: float = terrain_legend_list.get_combined_minimum_size().y
    var clamped_height: float = clamp(list_height, LEGEND_MIN_ROW_HEIGHT, LEGEND_MAX_HEIGHT)
    terrain_legend_scroll.custom_minimum_size.y = clamped_height
    terrain_legend_scroll.scroll_vertical = 0

func toggle_legend() -> void:
    legend_suppressed = not legend_suppressed
    if legend_suppressed:
        _hide_legend_panel()
    else:
        update_overlay_legend(overlay_legend)

func _hide_legend_panel() -> void:
    if terrain_legend_panel != null:
        terrain_legend_panel.visible = false
    if terrain_legend_description != null:
        terrain_legend_description.visible = false
        terrain_legend_description.text = ""

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

## Update the bottom-bar resource summary display.
## Called with key stockpile totals or other important metrics.
func update_resource_summary(summary: Dictionary) -> void:
    if resource_placeholder == null:
        return
    var parts: PackedStringArray = []
    for key in summary.keys():
        var value = summary[key]
        if value is int:
            parts.append("%s: %d" % [_format_stockpile_label(key), value])
        elif value is float:
            parts.append("%s: %.1f" % [_format_stockpile_label(key), value])
    if parts.is_empty():
        resource_placeholder.text = "Resources: --"
    else:
        resource_placeholder.text = " | ".join(parts)
