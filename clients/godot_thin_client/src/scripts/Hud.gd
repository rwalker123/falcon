extends CanvasLayer
class_name HudLayer

signal ui_zoom_delta(delta: float)
signal ui_zoom_reset
signal unit_scout_requested(x: int, y: int, band_entity_bits: int)
## Emitted when the player cancels a band's active task; carries the band dict so
## Main can extract faction + entity bits for the `cancel_order` command.
signal cancel_order_requested(band: Dictionary)
signal herd_follow_requested(herd_id: String)
signal forage_requested(x: int, y: int, module_key: String)
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
const CLIENT_BUILD := "2026-07-08.1"
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var sedentarization_label: Label = %SedentarizationLabel
@onready var demographics_label: Label = %DemographicsLabel
@onready var zoom_controls: HBoxContainer = $LayoutRoot/RootColumn/TopBar/ZoomControls
@onready var zoom_out_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomOutButton
@onready var zoom_reset_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomResetButton
@onready var zoom_in_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomInButton
@onready var terrain_legend_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel as PanelCard
@onready var terrain_legend_scroll: ScrollContainer = %LegendScroll
@onready var terrain_legend_list: VBoxContainer = %LegendList
@onready var terrain_legend_description: Label = %LegendDescription
@onready var victory_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel
@onready var victory_status_label: RichTextLabel = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel/Margin/VictoryLabel
@onready var alerts_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/AlertsPanel as PanelCard
@onready var alerts_label: RichTextLabel = %AlertsLabel
@onready var command_feed_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel as PanelCard
@onready var command_feed_scroll: ScrollContainer = %CommandFeedScroll
@onready var command_feed_label: RichTextLabel = %CommandFeedLabel
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var tile_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/TilePanel as PanelCard
@onready var tile_detail: RichTextLabel = %TileDetail
@onready var occupants_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/OccupantsPanel as PanelCard
@onready var occupant_detail: RichTextLabel = %OccupantDetail
@onready var roster_list: VBoxContainer = %RosterList
@onready var unit_buttons: HBoxContainer = %UnitButtons
@onready var unit_scout_button: Button = %UnitScoutButton
@onready var unit_cancel_button: Button = %UnitCancelButton
@onready var herd_buttons: HBoxContainer = %HerdButtons
@onready var hunt_herd_button: Button = %HuntHerdButton
@onready var hunt_policy_buttons: HBoxContainer = %HuntPolicyButtons
@onready var single_button: Button = %FollowSingleButton
@onready var follow_sustain_button: Button = %FollowSustainButton
@onready var follow_surplus_button: Button = %FollowSurplusButton
@onready var follow_market_button: Button = %FollowMarketButton
@onready var follow_eradicate_button: Button = %FollowEradicateButton
@onready var forage_button: Button = %ForageButton
@onready var stockpile_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel
@onready var stockpile_title: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileTitle
@onready var stockpile_list: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileList
@onready var left_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack
@onready var right_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack
@onready var next_turn_button: Button = $LayoutRoot/RootColumn/BottomBar/NextTurnButton
@onready var minimap_container: MarginContainer = $LayoutRoot/RootColumn/BottomBar/MinimapContainer
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
const ALERT_TYPE_STARVING := "starving"
const ALERT_TYPE_LOSING_POPULATION := "losing_population"
const ALERT_TYPE_IDLE := "idle"
const ALERT_PRIORITY := [ALERT_TYPE_STARVING, ALERT_TYPE_LOSING_POPULATION, ALERT_TYPE_IDLE]
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
const MORALE_HINT_PERSISTENT := "  Following a herd also lifts morale each turn (+morale/turn)."
# Occupants roster row chrome.
const ROSTER_DOT_SIZE := 9.0
const ROSTER_ROW_MIN_HEIGHT := 30.0
const ROSTER_ROW_SEPARATION := 8
const ROSTER_ROW_H_PADDING := 10.0
const ROSTER_ACCENT_WIDTH := 3.0
const ROSTER_HEADER_FONT_SIZE := 10
# Per-activity glyph for a player band's roster row (activity string from the
# snapshot: idle | harvest | hunt | follow | scout).
const ACTIVITY_GLYPHS := {
    "idle": "·",
    "harvest": "🌾",
    "hunt": "🏹",
    "follow": "🦌",
    "scout": "🧭",
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
var _pending_forage: Dictionary = {}
var _pending_scout_unit: Dictionary = {}
var _pending_hunt: Dictionary = {}
var _pending_follow: Dictionary = {}
# HUD-local optimistic map of bands with an order transition in flight (start OR
# cancel — same pattern: wait for `activity` to change). Keyed by band `entity`,
# value `{ "before": <normalized activity at dispatch>, "label": <button text> }`.
# An entry clears itself the first render the band's activity differs from `before`
# (the server confirm), whereupon the normal Scout/Cancel state renders.
var _pending_transition_bands: Dictionary = {}
# The herd action is one Hunt verb + a policy radio led by "single". Single = a
# one-shot `hunt_fauna`; every other policy = a persistent `follow_herd <policy>`
# that auto-hunts each turn. Default is the one-shot Single.
const HUNT_POLICY_SINGLE := "single"
const HUNT_POLICIES := ["single", "sustain", "surplus", "market", "eradicate"]
var _hunt_policy: String = HUNT_POLICY_SINGLE
var _targeting_banner: PanelContainer = null
var _targeting_banner_label: RichTextLabel = null
var _stockpile_totals: Dictionary = {}
var travel_tiles_per_turn: float = DEFAULT_TRAVEL_SPEED
var travel_preview_turn_cap: int = DEFAULT_TRAVEL_PREVIEW_LIMIT
var left_dock: PanelDock
var right_dock: PanelDock
# Left-edge space reserved for the docked Inspector; the whole HUD insets by it.
var _left_inset: float = 0.0

func _ready() -> void:
    _load_ui_balance_config()
    set_ui_zoom(1.0)
    _connect_zoom_controls()
    _setup_tooltip()
    _refresh_existing_legend_rows()
    _resize_legend_panel(_legend_list_size())
    _refresh_campaign_label()
    _refresh_victory_status()
    _render_command_feed()
    _connect_selection_buttons()
    _connect_control_buttons()
    left_dock = PanelDock.new(left_stack)
    right_dock = PanelDock.new(right_stack)
    left_dock.add(tile_panel, 10)
    left_dock.add(occupants_panel, 12)
    left_dock.add(alerts_panel, 15)
    left_dock.add(stockpile_panel, 20)
    left_dock.add(command_feed_panel, 30)
    _connect_alerts_panel()
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
    HudStyle.apply_button(unit_scout_button, "primary")
    HudStyle.apply_button(unit_cancel_button, "armed")
    HudStyle.apply_button(hunt_herd_button, "primary")
    HudStyle.apply_button(forage_button, "primary")
    _refresh_hunt_policy_buttons()
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
func _current_targeting_info() -> Dictionary:
    if not _pending_forage.is_empty():
        var action := _pending_forage_action()
        return {
            "active": true,
            "command": "hunt" if action == FOOD_ACTION_HUNT else "harvest",
            "need": "band",
            "origin_x": int(_pending_forage.get("x", -1)),
            "origin_y": int(_pending_forage.get("y", -1)),
            "context_label": String(_pending_forage.get("module_label", "food source")),
        }
    if not _pending_scout_unit.is_empty():
        var pos: Array = Array(_pending_scout_unit.get("pos", []))
        var ox := int(pos[0]) if pos.size() == 2 else -1
        var oy := int(pos[1]) if pos.size() == 2 else -1
        return {
            "active": true,
            "command": "scout",
            "need": "tile",
            "origin_x": ox,
            "origin_y": oy,
            "context_label": String(_pending_scout_unit.get("id", "Band")),
        }
    if not _pending_hunt.is_empty():
        return {
            "active": true,
            "command": "hunt",
            "need": "band",
            "origin_x": int(_pending_hunt.get("x", -1)),
            "origin_y": int(_pending_hunt.get("y", -1)),
            "context_label": String(_pending_hunt.get("label", "herd")),
        }
    if not _pending_follow.is_empty():
        return {
            "active": true,
            # Persistence is a Hunt policy now (the standalone "Follow" verb is retired);
            # the chosen policy rides in context_label, e.g. "HUNT … · Sustain".
            "command": "hunt",
            "need": "band",
            "origin_x": int(_pending_follow.get("x", -1)),
            "origin_y": int(_pending_follow.get("y", -1)),
            "context_label": "%s · %s" % [
                String(_pending_follow.get("label", "herd")),
                String(_pending_follow.get("policy", "sustain")).capitalize(),
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
    var instruction := "click a band to send it here" if need == "band" else "click a tile to survey"
    return "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
        HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
    ]

## Cancel whichever command is currently targeting (banner Cancel / Esc /
## right-click all route here).
func cancel_active_targeting() -> void:
    var changed := false
    if not _pending_forage.is_empty():
        _cancel_pending_forage(false)
        changed = true
    if not _pending_scout_unit.is_empty():
        _cancel_pending_scout()
        changed = true
    if not _pending_hunt.is_empty():
        _cancel_pending_hunt(false)
        changed = true
    if not _pending_follow.is_empty():
        _cancel_pending_follow(false)
        changed = true
    # Re-render whenever anything is selected — not just a tile — so cancelling a
    # pending Hunt/Follow on an inspector-selected herd (empty tile_info) still
    # clears the buttons' "Cancel …" state.
    if changed and (not _selected_tile_info.is_empty() or not _selected_herd.is_empty() or not _selected_unit.is_empty()):
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    # Note: _cancel_pending_forage / _cancel_pending_scout already call
    # _refresh_targeting(), so no extra refresh (and duplicate targeting_changed
    # emission) is needed here.

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
    turn_label.text = "Turn %d" % turn
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

func set_ui_zoom(scale: float) -> void:
    if zoom_reset_button != null:
        zoom_reset_button.text = "%.0f%%" % (scale * 100.0)


func _connect_zoom_controls() -> void:
    if zoom_out_button != null and not zoom_out_button.is_connected("pressed", Callable(self, "_on_zoom_out_pressed")):
        zoom_out_button.pressed.connect(_on_zoom_out_pressed)
    if zoom_reset_button != null and not zoom_reset_button.is_connected("pressed", Callable(self, "_on_zoom_reset_pressed")):
        zoom_reset_button.pressed.connect(_on_zoom_reset_pressed)
    if zoom_in_button != null and not zoom_in_button.is_connected("pressed", Callable(self, "_on_zoom_in_pressed")):
        zoom_in_button.pressed.connect(_on_zoom_in_pressed)

func _connect_selection_buttons() -> void:
    if unit_scout_button != null and not unit_scout_button.is_connected("pressed", Callable(self, "_on_unit_scout_pressed")):
        unit_scout_button.pressed.connect(_on_unit_scout_pressed)
        # Scouting is a positive morale lever (docs/plan_civ_wellbeing.md) — surface it.
        unit_scout_button.tooltip_text = MORALE_HINT_SCOUT
    if unit_cancel_button != null and not unit_cancel_button.is_connected("pressed", Callable(self, "_on_unit_cancel_pressed")):
        unit_cancel_button.pressed.connect(_on_unit_cancel_pressed)
        unit_cancel_button.tooltip_text = "Stop the band's current task and return it to idle."
    if hunt_herd_button != null and not hunt_herd_button.is_connected("pressed", Callable(self, "_on_hunt_herd_pressed")):
        hunt_herd_button.pressed.connect(_on_hunt_herd_pressed)
        hunt_herd_button.tooltip_text = "Send a band to hunt the selected herd."
    if single_button != null and not single_button.is_connected("pressed", Callable(self, "_on_hunt_policy_pressed")):
        single_button.pressed.connect(_on_hunt_policy_pressed.bind("single"))
        single_button.tooltip_text = "One hunt, then the band goes idle."
    if follow_sustain_button != null and not follow_sustain_button.is_connected("pressed", Callable(self, "_on_hunt_policy_pressed")):
        follow_sustain_button.pressed.connect(_on_hunt_policy_pressed.bind("sustain"))
        follow_sustain_button.tooltip_text = "Hunt each turn at ≈ regrowth — the herd stays roughly stable." + MORALE_HINT_PERSISTENT
    if follow_surplus_button != null and not follow_surplus_button.is_connected("pressed", Callable(self, "_on_hunt_policy_pressed")):
        follow_surplus_button.pressed.connect(_on_hunt_policy_pressed.bind("surplus"))
        follow_surplus_button.tooltip_text = "Hunt extra each turn for provisions/trade — the herd slowly declines." + MORALE_HINT_PERSISTENT
    if follow_market_button != null and not follow_market_button.is_connected("pressed", Callable(self, "_on_hunt_policy_pressed")):
        follow_market_button.pressed.connect(_on_hunt_policy_pressed.bind("market"))
        follow_market_button.tooltip_text = "Commercially over-hunt each turn for a trade windfall — the herd declines fast." + MORALE_HINT_PERSISTENT
    if follow_eradicate_button != null and not follow_eradicate_button.is_connected("pressed", Callable(self, "_on_hunt_policy_pressed")):
        follow_eradicate_button.pressed.connect(_on_hunt_policy_pressed.bind("eradicate"))
        follow_eradicate_button.tooltip_text = "Hunt maximally each turn — drives the herd toward local extinction." + MORALE_HINT_PERSISTENT
    if forage_button != null and not forage_button.is_connected("pressed", Callable(self, "_on_forage_pressed")):
        forage_button.pressed.connect(_on_forage_pressed)

func _connect_control_buttons() -> void:
    if next_turn_button != null and not next_turn_button.is_connected("pressed", Callable(self, "_on_next_turn_pressed")):
        next_turn_button.pressed.connect(_on_next_turn_pressed)

func _on_zoom_out_pressed() -> void:
    emit_signal("ui_zoom_delta", -1.0)

func _on_zoom_reset_pressed() -> void:
    emit_signal("ui_zoom_reset")

func _on_zoom_in_pressed() -> void:
    emit_signal("ui_zoom_delta", 1.0)

func _on_next_turn_pressed() -> void:
    emit_signal("next_turn_requested", 1)

func _on_unit_scout_pressed() -> void:
    if _selected_unit.is_empty():
        _cancel_pending_scout()
        return
    var entity_id := int(_selected_unit.get("entity", -1))
    if entity_id < 0:
        return
    if not _pending_scout_unit.is_empty() and int(_pending_scout_unit.get("entity", -1)) == entity_id:
        _cancel_pending_scout()
        return
    _clear_other_pending("scout")
    _pending_scout_unit = _selected_unit.duplicate(true)
    if not _selected_tile_info.is_empty():
        _try_dispatch_pending_scout(_selected_tile_info)
    _refresh_targeting()

func _on_unit_cancel_pressed() -> void:
    if _selected_unit.is_empty():
        return
    # Optimistic feedback: mark a transition whose `before` is the band's current
    # (task) activity, so the disabled "Cancelling <phrase>…" holds until the snapshot
    # reports the band idle (activity changed) and the panel reverts to Scout.
    var phrase := _task_action_phrase(
        String(_selected_unit.get("activity", "")).strip_edges().to_lower(),
        String(_selected_unit.get("hunt_mode", "")))
    _begin_band_transition(_selected_unit, CANCEL_ORDER_PENDING_VERB, phrase)
    emit_signal("cancel_order_requested", _selected_unit)

## Register an optimistic order transition for `band` and, if that band's drawer is
## the one on screen, flip the cancel button to the disabled in-flight label
## ("<verb> <phrase>…") immediately (no wait for the confirming render). Shared by
## start (Scout/Forage) and cancel — both wait for the band's `activity` to change
## from its dispatch value.
func _begin_band_transition(band: Dictionary, verb: String, phrase: String) -> void:
    var entity_id := int(band.get("entity", -1))
    if entity_id < 0:
        return
    var label := "%s %s…" % [verb, phrase]
    var before := String(band.get("activity", "")).strip_edges().to_lower()
    _pending_transition_bands[entity_id] = {"before": before, "label": label}
    if int(_selected_unit.get("entity", -1)) != entity_id:
        return
    if unit_scout_button != null:
        unit_scout_button.visible = false
    if unit_cancel_button != null:
        unit_cancel_button.visible = true
        unit_cancel_button.disabled = true
        unit_cancel_button.text = label

## The bare action phrase for a band's task, keyed off its coarse `activity` and
## (for fauna pursuit) its `hunt_mode` sub-mode: "Scouting" / "Foraging" /
## "<Mode> Hunt" / "Hunt" / "Task". Single source of truth for both the active
## "Cancel <phrase>" button and the in-flight "Cancelling <phrase>…" transition.
func _task_action_phrase(activity: String, hunt_mode: String) -> String:
    match activity:
        "scout":
            return "Scouting"
        "harvest":
            return "Foraging"
        "hunt", "follow":
            var mode := hunt_mode.strip_edges()
            if mode == "":
                return "Hunt"
            return "%s Hunt" % mode.capitalize()
        _:
            return "Task"

## Text for the active cancel button — "Cancel " + the shared action phrase.
func _cancel_label_for(activity: String, hunt_mode: String) -> String:
    return "Cancel %s" % _task_action_phrase(activity, hunt_mode)

## Toggle a player band's action buttons by task state: idle bands offer the
## default Scout action; bands on any task offer a single labelled Cancel button.
func _update_unit_task_buttons(unit: Dictionary) -> void:
    var entity_id := int(unit.get("entity", -1))
    var activity := String(unit.get("activity", "")).strip_edges().to_lower()
    var on_task := activity != "" and activity != BAND_ACTIVITY_IDLE
    # Optimistic order transition (start OR cancel) in flight for this band: hold the
    # disabled in-flight label while the band's activity still matches its dispatch
    # value; once it changes (server confirm) clear the entry and fall through to the
    # normal Scout/Cancel state.
    if entity_id >= 0 and _pending_transition_bands.has(entity_id):
        var transition: Dictionary = _pending_transition_bands[entity_id]
        if activity == String(transition.get("before", "")):
            if unit_scout_button != null:
                unit_scout_button.visible = false
            if unit_cancel_button != null:
                unit_cancel_button.visible = true
                unit_cancel_button.disabled = true
                unit_cancel_button.text = String(transition.get("label", ""))
            return
        _pending_transition_bands.erase(entity_id)
    if unit_scout_button != null:
        unit_scout_button.visible = not on_task
    if unit_cancel_button != null:
        # Re-enable in case this button was left disabled by a prior band's transition.
        unit_cancel_button.disabled = false
        unit_cancel_button.visible = on_task
        if on_task:
            unit_cancel_button.text = _cancel_label_for(activity, String(unit.get("hunt_mode", "")))

## The single Hunt verb, routed by the active policy: Single → a one-shot
## `hunt_fauna` (`_pending_hunt`); any other policy → a persistent `follow_herd`
## (`_pending_follow`). Toggling the button while *either* kind is pending for this
## herd cancels it (matching the unified "Cancel Hunt" label), regardless of policy.
func _on_hunt_herd_pressed() -> void:
    if _selected_herd.is_empty():
        return
    var herd_id := String(_selected_herd.get("id", ""))
    if herd_id == "":
        return
    # The button reads "Cancel Hunt" whenever *either* pending kind is active for
    # this herd (see _update_herd_buttons), so honor that contract: cancel any
    # pending hunt/follow before routing by policy, rather than only the kind that
    # happens to match the current policy (which would re-target instead of cancel).
    if not _pending_hunt.is_empty() and String(_pending_hunt.get("herd_id", "")) == herd_id:
        _cancel_pending_hunt(true)
        return
    if not _pending_follow.is_empty() and String(_pending_follow.get("herd_id", "")) == herd_id:
        _cancel_pending_follow(true)
        return
    if _hunt_policy == HUNT_POLICY_SINGLE:
        _begin_pending_hunt(herd_id)
    else:
        _begin_pending_follow(herd_id, _hunt_policy)

func _on_hunt_policy_pressed(policy: String) -> void:
    if policy in HUNT_POLICIES:
        _hunt_policy = policy
    _refresh_hunt_policy_buttons()
    # If a hunt is already being targeted, switching policy re-derives the pending
    # action through the same begin path — this converts single↔persistent in place
    # (dropping the other pending kind) so the banner matches the new policy.
    if not _selected_herd.is_empty() and (not _pending_hunt.is_empty() or not _pending_follow.is_empty()):
        var herd_id := String(_selected_herd.get("id", ""))
        if herd_id != "":
            if _hunt_policy == HUNT_POLICY_SINGLE:
                _begin_pending_hunt(herd_id)
            else:
                _begin_pending_follow(herd_id, _hunt_policy)

## Restyle the policy radio so the active policy reads as selected.
func _refresh_hunt_policy_buttons() -> void:
    var buttons := {
        "single": single_button,
        "sustain": follow_sustain_button,
        "surplus": follow_surplus_button,
        "market": follow_market_button,
        "eradicate": follow_eradicate_button,
    }
    for policy in buttons:
        var btn: Button = buttons[policy]
        if btn == null:
            continue
        HudStyle.apply_button(btn, "primary" if policy == _hunt_policy else "ghost")

func _on_forage_pressed() -> void:
    if _selected_food_module == "":
        return
    var x := int(_selected_tile_info.get("x", -1))
    var y := int(_selected_tile_info.get("y", -1))
    if x < 0 or y < 0:
        return
    var module_key := _selected_food_module
    var action := FOOD_ACTION_HUNT if _selected_food_is_hunt else FOOD_ACTION_FORAGE
    if _pending_forage_matches_coords(x, y, module_key):
        if _pending_forage_action() == action:
            _cancel_pending_forage(true)
        else:
            _begin_pending_forage(x, y, module_key, action)
    else:
        _begin_pending_forage(x, y, module_key, action)


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

## Inset the entire HUD from the left edge to reserve room for the docked
## Inspector. The panels keep their natural docks; the whole layout just lives in
## the narrower rectangle, matching the shrunk map area.
func set_left_inset(px: float) -> void:
    _left_inset = max(px, 0.0)
    if layout_root != null:
        layout_root.offset_left = _left_inset

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
    _try_dispatch_pending_scout(_selected_tile_info)

func notify_hex_selected(tile_info: Dictionary) -> void:
    if tile_info.is_empty():
        return
    _try_dispatch_pending_scout(tile_info)

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
                if forage_button != null:
                    forage_button.visible = false
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

## The Tile card: the place. Terrain rows + the Forage action (its only action).
## Kind stays "Tile" even when an occupant is selected.
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
    _update_food_buttons(tile_info)

## The Occupants card: a selectable roster of bands + wildlife on the hex, plus a
## detail drawer for the selected occupant. Hidden (dock reflows) on an empty hex.
func _render_occupants_card() -> void:
    if occupants_panel == null:
        return
    if _roster_units.is_empty() and _roster_herds.is_empty():
        _set_occupants_relevant(false)
        if unit_buttons != null:
            unit_buttons.visible = false
        if herd_buttons != null:
            herd_buttons.visible = false
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
    if _pending_forage_matches_tile(tile_info):
        var pending_action := _pending_forage_action()
        var verb := "Hunt" if pending_action == FOOD_ACTION_HUNT else "Harvest"
        lines.append("%s pending: select a band to send here." % verb)
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
    if unit_buttons != null:
        # Scout is a player-band action only.
        var is_player_band := is_band and _is_player_unit(_selected_unit)
        unit_buttons.visible = is_player_band
        if is_player_band:
            _update_unit_task_buttons(_selected_unit)
    if herd_buttons != null:
        herd_buttons.visible = is_herd
    if is_herd:
        _update_herd_buttons(_selected_herd)

## Player-faction check for a roster/drawer band (mirrors MapView._is_player_unit).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", PLAYER_FACTION_ID)) == PLAYER_FACTION_ID

func _unit_summary_lines(unit_data: Dictionary) -> Array[String]:
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
    if _pending_scout_for_entity(int(unit_data.get("entity", -1))):
        lines.append("")
        lines.append("Scout pending: select a target tile.")
    var harvest_variant: Variant = unit_data.get("harvest", {})
    if harvest_variant is Dictionary and not (harvest_variant as Dictionary).is_empty():
        lines.append("")
        lines.append_array(_harvest_summary_lines(harvest_variant))
    var scout_variant: Variant = unit_data.get("scout", {})
    if scout_variant is Dictionary and not (scout_variant as Dictionary).is_empty():
        lines.append("")
        lines.append_array(_scout_summary_lines(scout_variant))
    var stockpile_variant: Variant = unit_data.get("accessible_stockpile", {})
    if stockpile_variant is Dictionary:
        var stockpile_lines := _accessible_stockpile_lines(stockpile_variant)
        if not stockpile_lines.is_empty():
            lines.append("")
            lines.append_array(stockpile_lines)
    return lines

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

func _harvest_summary_lines(harvest: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var module_key := String(harvest.get("module", "")).strip_edges()
    var module_label := _format_food_module_label(module_key)
    var action := String(harvest.get("action", FOOD_ACTION_FORAGE)).strip_edges()
    var action_label := "Harvest" if action != FOOD_ACTION_HUNT else "Hunt"
    var status := action_label
    var travel_remaining := int(harvest.get("travel_remaining", 0))
    var travel_total: int = max(int(harvest.get("travel_total", 0)), travel_remaining)
    var gather_remaining := int(harvest.get("gather_remaining", 0))
    var gather_total: int = max(int(harvest.get("gather_total", 0)), gather_remaining)
    if travel_remaining > 0:
        status = "Traveling"
    elif gather_remaining > 0:
        status = action_label
    else:
        status = "Finishing"
    lines.append("%s: %s" % [status, module_label])
    if travel_total > 0:
        lines.append("Travel: %d/%d turns" % [travel_total - travel_remaining, travel_total])
    if gather_total > 0:
        lines.append("Gather: %d/%d turns" % [gather_total - gather_remaining, gather_total])
    var eta: int = max(travel_remaining, 0) + max(gather_remaining, 0)
    if eta > 0:
        lines.append("ETA: %d turn%s" % [eta, "s" if eta != 1 else ""])
    var provisions := int(harvest.get("provisions_reward", 0))
    var trade_goods := int(harvest.get("trade_goods_reward", 0))
    var reward_parts: Array[String] = []
    if provisions > 0:
        reward_parts.append("+%d provisions" % provisions)
    if trade_goods > 0:
        reward_parts.append("+%d trade goods" % trade_goods)
    if not reward_parts.is_empty():
        lines.append("Reward: %s" % ", ".join(reward_parts))
    return lines

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

func _scout_summary_lines(task: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var travel_remaining: int = int(task.get("travel_remaining", 0))
    var travel_total: int = max(int(task.get("travel_total", 0)), travel_remaining)
    var status := "Scouting"
    if travel_remaining > 0:
        status = "Traveling"
    lines.append("%s" % status)
    lines.append("Travel: %d/%d turns" % [travel_total - travel_remaining, travel_total])
    var radius := int(task.get("reveal_radius", 0))
    if radius > 0:
        lines.append("Reveal radius: %d" % radius)
    var morale := float(task.get("morale_gain", 0.0))
    if morale > 0.0:
        lines.append("Morale +%.2f" % morale)
    return lines

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
            if String(kv[0]) == "Food":
                # The band larder row (drawer) tints by the food-days thresholds;
                # its value carries a day count or the ∞ glyph.
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

## Herd action: one Hunt verb (gated on `huntable`) + the policy radio. The button
## enters targeting mode to pick a band and flips to a unified "Cancel Hunt" while
## either the one-shot (`_pending_hunt`) or persistent (`_pending_follow`) hunt is pending.
func _update_herd_buttons(herd_data: Dictionary) -> void:
    if herd_data.is_empty():
        return
    var herd_id := String(herd_data.get("id", ""))
    # Fail closed: only offer Hunt when the snapshot explicitly allows it. The Hunt
    # button and its policy radio hide together on a non-huntable herd, so the radio
    # never orphans without a button to commit it (Single/Sustain/… all hunt).
    var huntable := bool(herd_data.get("huntable", false))
    if hunt_herd_button != null:
        hunt_herd_button.visible = huntable
        var hunt_pending := not _pending_hunt.is_empty() and String(_pending_hunt.get("herd_id", "")) == herd_id
        var follow_pending := not _pending_follow.is_empty() and String(_pending_follow.get("herd_id", "")) == herd_id
        var pending := hunt_pending or follow_pending
        HudStyle.apply_button(hunt_herd_button, "armed" if pending else "primary")
        hunt_herd_button.text = "Cancel Hunt" if pending else "Hunt"
    if hunt_policy_buttons != null:
        hunt_policy_buttons.visible = huntable
    _refresh_hunt_policy_buttons()

func _update_food_buttons(tile_info: Dictionary) -> void:
    if forage_button == null:
        return
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    if module_key == "":
        forage_button.visible = false
        _selected_food_module = ""
        _selected_food_is_hunt = false
        return
    _selected_food_module = module_key
    _selected_food_is_hunt = false
    var label := String(tile_info.get("food_module_label", "Harvest")).strip_edges()
    if label == "":
        label = module_key.capitalize()
    var pending_active := _pending_forage_matches_tile(tile_info)
    HudStyle.apply_button(forage_button, "armed" if pending_active else "primary")
    if pending_active:
        forage_button.text = "Cancel Harvest"
        forage_button.tooltip_text = "Cancel the pending harvest assignment for this tile."
    else:
        var turns := _travel_turns_for_tile(tile_info)
        var button_text := "Harvest %s" % label
        if turns > 0:
            button_text += " (~%d turns)" % turns
        forage_button.text = "%s  %s" % [FoodIcons.for_site(module_key, false), button_text]
        var hint := _travel_eta_hint(tile_info)
        if hint == "":
            hint = "Select a band after clicking to send them here."
        forage_button.tooltip_text = hint
    forage_button.disabled = false
    forage_button.visible = true

func clear_selection() -> void:
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = ""
    _selected_food_is_hunt = false
    if not _pending_forage.is_empty():
        _cancel_pending_forage(false)
    # keep pending scout so user can still choose a tile after deselecting
    if _selected_tile_info.is_empty():
        if tile_panel != null:
            tile_panel.visible = false
        if forage_button != null:
            forage_button.visible = false
        _set_occupants_relevant(false)
    else:
        _render_selection_panel(_selected_tile_info, {}, {})
    if unit_buttons != null:
        unit_buttons.visible = false
    if herd_buttons != null:
        herd_buttons.visible = false

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

## Wire the alerts panel's link clicks and hide it until the first alert arrives.
func _connect_alerts_panel() -> void:
    if alerts_label != null and not alerts_label.is_connected("meta_clicked", Callable(self, "_on_alert_meta_clicked")):
        alerts_label.meta_clicked.connect(_on_alert_meta_clicked)
    if left_dock != null and alerts_panel != null:
        left_dock.set_relevant(alerts_panel, false)

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
    var alerts: Array = []
    var band_index := 0
    for entry_variant in populations:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        if int(entry.get("faction", -1)) != PLAYER_FACTION_ID:
            continue
        band_index += 1
        var entity := int(entry.get("entity", -1))
        var size := int(entry.get("size", 0))
        var days := float(entry.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
        var morale := float(entry.get("morale", 1.0))
        var morale_cause := int(entry.get("morale_cause", MORALE_CAUSE_NONE))
        var last_emigrated := int(entry.get("last_emigrated", 0))
        var activity := String(entry.get("activity", "")).strip_edges()
        var x := int(entry.get("current_x", -1))
        var y := int(entry.get("current_y", -1))
        var band_name := _band_display_name(entry, band_index)
        new_sizes[entity] = size
        if BandFoodStatus.is_critical(days):
            alerts.append({"type": ALERT_TYPE_STARVING, "band": band_name, "x": x, "y": y, "days": days})
        if _prev_band_sizes.has(entity) and size < int(_prev_band_sizes[entity]):
            alerts.append({
                "type": ALERT_TYPE_LOSING_POPULATION, "band": band_name, "x": x, "y": y,
                "reason": _decline_reason(days, morale, morale_cause, last_emigrated),
            })
        if activity == BAND_ACTIVITY_IDLE:
            alerts.append({"type": ALERT_TYPE_IDLE, "band": band_name, "x": x, "y": y})
    _prev_band_sizes = new_sizes
    _render_alerts(alerts)

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

## Best-effort readable band name: the label carried on an active harvest/scout
## task, else a positional "Band N". (Cohorts carry no top-level band label in the
## snapshot yet — see the server-side follow-up.)
func _band_display_name(entry: Dictionary, index: int) -> String:
    for task_key in ["harvest", "scout"]:
        var task_variant: Variant = entry.get(task_key, {})
        if task_variant is Dictionary:
            var label := String((task_variant as Dictionary).get("band_label", "")).strip_edges()
            if label != "":
                return label
    return "Band %d" % index

func _render_alerts(alerts: Array) -> void:
    if alerts_panel == null or alerts_label == null:
        return
    if alerts.is_empty():
        if left_dock != null:
            left_dock.set_relevant(alerts_panel, false)
        else:
            alerts_panel.visible = false
        return
    alerts.sort_custom(func(a, b): return ALERT_PRIORITY.find(String(a.get("type"))) < ALERT_PRIORITY.find(String(b.get("type"))))
    var lines := PackedStringArray()
    for alert_variant in alerts:
        lines.append(_format_alert_line(alert_variant))
    alerts_label.text = "\n".join(lines)
    if left_dock != null:
        left_dock.set_relevant(alerts_panel, true)
    else:
        alerts_panel.visible = true

## One clickable alert row: a `[url=x,y]` link (so a click focuses the map on the
## band) colored by severity — starving red, population-loss amber, idle quiet dim.
func _format_alert_line(alert: Dictionary) -> String:
    var type := String(alert.get("type", ""))
    var band_name := String(alert.get("band", "Band"))
    var x := int(alert.get("x", -1))
    var y := int(alert.get("y", -1))
    var meta := "%d,%d" % [x, y]
    var text := ""
    var color_hex := HudStyle.INK_HEX
    match type:
        ALERT_TYPE_STARVING:
            text = "⚠ %s starving — %s" % [band_name, _food_days_text(float(alert.get("days", 0.0)))]
            color_hex = HudStyle.DANGER_HEX
        ALERT_TYPE_LOSING_POPULATION:
            text = "⚠ %s losing population" % band_name
            var reason := String(alert.get("reason", ""))
            if reason != "":
                text += " — %s" % reason
            color_hex = HudStyle.WARN_HEX
        ALERT_TYPE_IDLE:
            text = "%s idle" % band_name
            color_hex = HudStyle.INK_DIM_HEX
        _:
            text = band_name
    return "[url=%s][color=#%s]%s[/color][/url]" % [meta, color_hex, text]

func _on_alert_meta_clicked(meta: Variant) -> void:
    var parts := String(meta).split(",")
    if parts.size() != 2:
        return
    var x := int(parts[0])
    var y := int(parts[1])
    if x < 0 or y < 0:
        return
    emit_signal("alert_focus_requested", x, y)

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

func consume_pending_forage(unit_data: Dictionary) -> Dictionary:
    if _pending_forage.is_empty():
        return {}
    var x := int(_pending_forage.get("x", -1))
    var y := int(_pending_forage.get("y", -1))
    var module_key := String(_pending_forage.get("module", "")).strip_edges()
    var action := String(_pending_forage.get("action", FOOD_ACTION_FORAGE))
    if x < 0 or y < 0 or (module_key == "" and action != FOOD_ACTION_HUNT):
        _pending_forage.clear()
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
        _refresh_targeting()
        return {}
    var payload := _pending_forage.duplicate(true)
    payload["action"] = action
    var unit_label := String(unit_data.get("id", unit_data.get("entity", "Band")))
    payload["unit_label"] = unit_label
    var entity_bits_variant: Variant = unit_data.get("entity", -1)
    if typeof(entity_bits_variant) == TYPE_INT:
        payload["band_entity_bits"] = int(entity_bits_variant)
    payload["unit_id"] = entity_bits_variant
    _pending_forage.clear()
    # Forage/Hunt-game is dispatched to the just-selected band: show "Starting
    # Foraging…" (or "Starting Hunt…" for the hunt-game variant) until its activity
    # flips to the ordered task. Phrases route through `_task_action_phrase` (no bare
    # literals). `_selected_unit` is this band (set by the preceding show_unit_selection),
    # so the transition renders on the drawer below.
    var start_phrase := _task_action_phrase("hunt", "") if action == FOOD_ACTION_HUNT else _task_action_phrase("harvest", "")
    _begin_band_transition(unit_data, START_ORDER_PENDING_VERB, start_phrase)
    _render_selection_panel(_selected_tile_info, unit_data, _selected_herd)
    _refresh_targeting()
    return payload

func _pending_forage_matches_tile(tile_info: Dictionary) -> bool:
    if tile_info.is_empty():
        return false
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    return _pending_forage_matches_coords(x, y, module_key)

func _pending_forage_matches_coords(x: int, y: int, module_key: String) -> bool:
    if _pending_forage.is_empty():
        return false
    if x != int(_pending_forage.get("x", -1)) or y != int(_pending_forage.get("y", -1)):
        return false
    var pending_module := String(_pending_forage.get("module", "")).strip_edges()
    if pending_module == "":
        return module_key == "" or module_key == pending_module
    if module_key == "":
        return true
    return module_key == pending_module

func _pending_forage_action() -> String:
    if _pending_forage.is_empty():
        return FOOD_ACTION_FORAGE
    return String(_pending_forage.get("action", FOOD_ACTION_FORAGE))

func _pending_scout_for_entity(entity_id: int) -> bool:
    if entity_id < 0 or _pending_scout_unit.is_empty():
        return false
    return int(_pending_scout_unit.get("entity", -1)) == entity_id

func _cancel_pending_scout() -> void:
    if _pending_scout_unit.is_empty():
        return
    _pending_scout_unit.clear()
    _refresh_targeting()

func _try_dispatch_pending_scout(tile_info: Dictionary) -> void:
    if _pending_scout_unit.is_empty() or tile_info.is_empty():
        return
    var target_x := int(tile_info.get("x", -1))
    var target_y := int(tile_info.get("y", -1))
    if target_x < 0 or target_y < 0:
        return
    var unit_pos: Array = Array(_pending_scout_unit.get("pos", []))
    if unit_pos.size() == 2 and target_x == int(unit_pos[0]) and target_y == int(unit_pos[1]):
        return
    var band_bits := int(_pending_scout_unit.get("entity", -1))
    if band_bits < 0:
        return
    emit_signal("unit_scout_requested", target_x, target_y, band_bits)
    # Scout is a band-panel order for the still-selected band: show "Starting
    # Scouting…" from the moment the command is dispatched (not while picking the tile).
    _begin_band_transition(_pending_scout_unit, START_ORDER_PENDING_VERB, _task_action_phrase("scout", ""))
    _pending_scout_unit.clear()
    _refresh_targeting()

func _begin_pending_forage(x: int, y: int, module_key: String, action: String) -> void:
    _clear_other_pending("forage")
    var module_label := String(_selected_tile_info.get("food_module_label", module_key)).strip_edges()
    if module_label == "":
        module_label = module_key.capitalize()
    _pending_forage = {
        "x": x,
        "y": y,
        "module": module_key,
        "module_label": module_label,
        "action": action if action != "" else FOOD_ACTION_FORAGE,
    }
    _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

func _cancel_pending_forage(refresh: bool) -> void:
    _pending_forage.clear()
    if refresh:
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

## Only one command targets at a time — clear any other pending action so the
## banner + band glow are unambiguous.
func _clear_other_pending(keep: String) -> void:
    if keep != "forage":
        _pending_forage.clear()
    if keep != "scout":
        _pending_scout_unit.clear()
    if keep != "hunt":
        _pending_hunt.clear()
    if keep != "follow":
        _pending_follow.clear()

func _begin_pending_hunt(herd_id: String) -> void:
    _clear_other_pending("hunt")
    _pending_hunt = {
        "herd_id": herd_id,
        "x": int(_selected_herd.get("x", -1)),
        "y": int(_selected_herd.get("y", -1)),
        "label": String(_selected_herd.get("label", herd_id)),
    }
    _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

func _cancel_pending_hunt(refresh: bool) -> void:
    _pending_hunt.clear()
    if refresh:
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

func consume_pending_hunt(unit_data: Dictionary) -> Dictionary:
    if _pending_hunt.is_empty():
        return {}
    var herd_id := String(_pending_hunt.get("herd_id", ""))
    if herd_id == "":
        _pending_hunt.clear()
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
        _refresh_targeting()
        return {}
    var payload := {"herd_id": herd_id}
    var entity_bits_variant: Variant = unit_data.get("entity", -1)
    if typeof(entity_bits_variant) == TYPE_INT:
        payload["band_entity_bits"] = int(entity_bits_variant)
    _pending_hunt.clear()
    # One-shot hunt dispatched to the just-selected band (its live hunt_mode is
    # "single"): show "Starting Single Hunt…" until the snapshot confirms the task.
    _begin_band_transition(unit_data, START_ORDER_PENDING_VERB, _task_action_phrase("hunt", HUNT_POLICY_SINGLE))
    _render_selection_panel(_selected_tile_info, unit_data, _selected_herd)
    _refresh_targeting()
    return payload

func _begin_pending_follow(herd_id: String, policy: String) -> void:
    _clear_other_pending("follow")
    _pending_follow = {
        "herd_id": herd_id,
        "policy": policy if (policy in HUNT_POLICIES and policy != HUNT_POLICY_SINGLE) else "sustain",
        "x": int(_selected_herd.get("x", -1)),
        "y": int(_selected_herd.get("y", -1)),
        "label": String(_selected_herd.get("label", herd_id)),
    }
    _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

func _cancel_pending_follow(refresh: bool) -> void:
    _pending_follow.clear()
    if refresh:
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
    _refresh_targeting()

func consume_pending_follow(unit_data: Dictionary) -> Dictionary:
    if _pending_follow.is_empty():
        return {}
    var herd_id := String(_pending_follow.get("herd_id", ""))
    if herd_id == "":
        _pending_follow.clear()
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)
        _refresh_targeting()
        return {}
    var policy := String(_pending_follow.get("policy", "sustain"))
    var payload := {
        "herd_id": herd_id,
        "policy": policy,
    }
    var entity_bits_variant: Variant = unit_data.get("entity", -1)
    if typeof(entity_bits_variant) == TYPE_INT:
        payload["band_entity_bits"] = int(entity_bits_variant)
    _pending_follow.clear()
    # Persistent follow-hunt dispatched to the just-selected band (its live activity
    # is "follow" + hunt_mode=<policy>): show "Starting <Policy> Hunt…" until confirmed.
    _begin_band_transition(unit_data, START_ORDER_PENDING_VERB, _task_action_phrase("follow", policy))
    _render_selection_panel(_selected_tile_info, unit_data, _selected_herd)
    _refresh_targeting()
    return payload

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
