extends CanvasLayer
class_name HudLayer

signal ui_zoom_delta(delta: float)
signal ui_zoom_reset
signal unit_scout_requested(x: int, y: int, band_entity_bits: int)
signal unit_found_camp_requested(x: int, y: int)
signal herd_follow_requested(herd_id: String)
signal forage_requested(x: int, y: int, module_key: String)
signal next_turn_requested(steps: int)
## Emitted whenever the active command-targeting state changes. Carries a dict
## ({} when inactive) that Main forwards to MapView so the map can draw the
## reticle / valid-target glow / hover ETA.
signal targeting_changed(info: Dictionary)

## Build identifier of THIS client (GDScript/native). **Bump on client-affecting
## changes.** Shown in the lower-left version overlay next to the server build (streamed
## in the snapshot header) so the running client+server builds can be confirmed at a
## glance. Format: `YYYY-MM-DD.N`.
const CLIENT_BUILD := "2026-07-07.1"
var _build_label: Label = null
var _server_build: String = "?"

@onready var layout_root: Control = $LayoutRoot
@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
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
@onready var command_feed_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel as PanelCard
@onready var command_feed_scroll: ScrollContainer = %CommandFeedScroll
@onready var command_feed_label: RichTextLabel = %CommandFeedLabel
@onready var left_dock_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll
@onready var selection_panel: PanelCard = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel as PanelCard
@onready var selection_detail: RichTextLabel = %SelectionDetail
@onready var unit_buttons: HBoxContainer = %UnitButtons
@onready var unit_scout_button: Button = %UnitScoutButton
@onready var unit_camp_button: Button = %UnitCampButton
@onready var herd_buttons: HBoxContainer = %HerdButtons
@onready var follow_herd_button: Button = %FollowHerdButton
@onready var food_buttons: HBoxContainer = %FoodButtons
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
const HERD_CONSUMPTION_BIOMASS := 250.0
const HERD_PROVISIONS_YIELD_PER_BIOMASS := 0.02
const HERD_TRADE_GOODS_YIELD_PER_BIOMASS := 0.005
const HERD_FOLLOW_MORALE_GAIN := 0.03
const HERD_KNOWLEDGE_PROGRESS_PER_BIOMASS := 0.0004
const HERD_KNOWLEDGE_PROGRESS_CAP := 0.25
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
var _selected_tile_info: Dictionary = {}
var _selected_unit: Dictionary = {}
var _selected_herd: Dictionary = {}
var _selected_food_module: String = ""
var _selected_food_is_hunt: bool = false
var _pending_forage: Dictionary = {}
var _pending_scout_unit: Dictionary = {}
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
    left_dock.add(selection_panel, 10)
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
    if selection_detail != null:
        selection_detail.add_theme_color_override("default_color", HudStyle.INK_DIM)
        selection_detail.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
        selection_detail.add_theme_constant_override("table_h_separation", 16)
        selection_detail.add_theme_constant_override("table_v_separation", 3)
    HudStyle.apply_button(unit_scout_button, "primary")
    HudStyle.apply_button(unit_camp_button, "ghost")
    HudStyle.apply_button(follow_herd_button, "ghost")
    HudStyle.apply_button(forage_button, "primary")
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
    if changed and not _selected_tile_info.is_empty():
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
    if unit_camp_button != null and not unit_camp_button.is_connected("pressed", Callable(self, "_on_unit_camp_pressed")):
        unit_camp_button.pressed.connect(_on_unit_camp_pressed)
    if follow_herd_button != null and not follow_herd_button.is_connected("pressed", Callable(self, "_on_follow_herd_pressed")):
        follow_herd_button.pressed.connect(_on_follow_herd_pressed)
        follow_herd_button.tooltip_text = "Follow the selected herd to gain morale, supplies, fauna lore, and a fog reveal pulse."
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
    _pending_scout_unit = _selected_unit.duplicate(true)
    if not _selected_tile_info.is_empty():
        _try_dispatch_pending_scout(_selected_tile_info)
    _refresh_targeting()

func _on_unit_camp_pressed() -> void:
    if _selected_unit.is_empty():
        return
    var position: Array = Array(_selected_unit.get("pos", []))
    if position.size() != 2:
        return
    emit_signal("unit_found_camp_requested", int(position[0]), int(position[1]))

func _on_follow_herd_pressed() -> void:
    if _selected_herd.is_empty():
        return
    var herd_id := String(_selected_herd.get("id", ""))
    if herd_id == "":
        return
    emit_signal("herd_follow_requested", herd_id)

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
    if tile_variant is Dictionary:
        tile_info = (tile_variant as Dictionary).duplicate(true)
    _selected_tile_info = tile_info
    _selected_herd = herd_data.duplicate(true)
    _selected_unit.clear()
    _selected_food_module = String(tile_info.get("food_module", "")).strip_edges()
    _render_selection_panel(tile_info, {}, _selected_herd)

func _render_selection_panel(tile_info: Dictionary, unit_data: Dictionary, herd_data: Dictionary) -> void:
    if selection_panel == null or selection_detail == null:
        return
    selection_panel.visible = true
    # Title carries a colored "kind" eyebrow (TILE / BAND / HERD) plus a concise
    # identifier — the new Tile Banner look.
    var kind_text := "Tile"
    var title_text := "—"
    if not tile_info.is_empty():
        var x := int(tile_info.get("x", -1))
        var y := int(tile_info.get("y", -1))
        title_text = "(%d, %d)" % [x, y]
    if not unit_data.is_empty():
        kind_text = "Band"
        title_text = String(unit_data.get("id", "Band"))
    elif not herd_data.is_empty():
        kind_text = "Herd"
        title_text = String(herd_data.get("id", "Herd"))
    selection_panel.set_card_kind(kind_text)
    selection_panel.set_card_title(title_text)
    var food_kind_value := ""
    if not tile_info.is_empty():
        food_kind_value = String(tile_info.get("food_kind", "")).strip_edges()
    _selected_food_is_hunt = food_kind_value == "game_trail"
    var detail_lines: Array[String] = _tile_summary_lines(tile_info)
    if not unit_data.is_empty():
        if not detail_lines.is_empty():
            detail_lines.append("")
        detail_lines.append_array(_unit_summary_lines(unit_data))
    elif not herd_data.is_empty():
        if not detail_lines.is_empty():
            detail_lines.append("")
        detail_lines.append_array(_herd_summary_lines(herd_data))
    selection_detail.text = _format_detail_bbcode(detail_lines)
    if unit_buttons != null:
        unit_buttons.visible = not unit_data.is_empty()
    if herd_buttons != null:
        herd_buttons.visible = not herd_data.is_empty()
    _update_food_buttons(tile_info, not unit_data.is_empty())


func _tile_summary_lines(tile_info: Dictionary) -> Array[String]:
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
    if visibility_state == "discovered":
        lines.append("Last seen — information incomplete. Scout to update.")
        return lines
    var food_label := String(tile_info.get("food_module_label", "None")).strip_edges()
    if food_label == "":
        food_label = "None"
    var weight: float = float(tile_info.get("food_module_weight", 0.0))
    var food_kind := String(tile_info.get("food_kind", "")).strip_edges()
    var food_line := "Food: %s" % food_label
    if food_kind != "":
        food_line = "%s — %s" % [food_line, _format_food_kind_label(food_kind)]
    if weight > 0.0:
        food_line += " (weight %.2f)" % weight
    lines.append(food_line)
    var unit_entries_variant: Variant = tile_info.get("units", [])
    if unit_entries_variant is Array:
        var unit_entries: Array = unit_entries_variant
        if not unit_entries.is_empty():
            lines.append("Units (%d): %s" % [unit_entries.size(), _format_unit_list(unit_entries)])
    var herd_entries_variant: Variant = tile_info.get("herds", [])
    if herd_entries_variant is Array:
        var herd_entries: Array = herd_entries_variant
        if not herd_entries.is_empty():
            lines.append("Herds (%d): %s" % [herd_entries.size(), _format_herd_list(herd_entries)])
    var harvest_entries_variant: Variant = tile_info.get("harvest_tasks", [])
    if harvest_entries_variant is Array:
        var harvest_entries: Array = harvest_entries_variant
        if not harvest_entries.is_empty():
            var labels := PackedStringArray()
            for entry in harvest_entries:
                if not (entry is Dictionary):
                    continue
                var entry_dict: Dictionary = entry
                var module_key := String(entry_dict.get("module", ""))
                var module_label := _format_food_module_label(module_key)
                var action := String(entry_dict.get("action", "harvest")).strip_edges()
                if action == FOOD_ACTION_HUNT:
                    labels.append("%s (Hunt)" % module_label)
                else:
                    labels.append(module_label)
            if not labels.is_empty():
                lines.append("Harvesters (%d): %s" % [labels.size(), ", ".join(labels)])
    var scout_entries_variant: Variant = tile_info.get("scout_tasks", [])
    if scout_entries_variant is Array:
        var scout_entries: Array = scout_entries_variant
        if not scout_entries.is_empty():
            lines.append("Scouts (%d)" % scout_entries.size())
    if _pending_forage_matches_tile(tile_info):
        var pending_action := _pending_forage_action()
        var verb := "Hunt" if pending_action == FOOD_ACTION_HUNT else "Harvest"
        lines.append("%s pending: select a band to send here." % verb)
    if _pending_scout_active():
        lines.append("Scout pending: choose a tile to survey.")
    var travel_line := _travel_eta_line(tile_info)
    if travel_line != "":
        lines.append(travel_line)
    return lines

func _unit_summary_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var label := String(unit_data.get("id", "Band"))
    lines.append("Unit: %s" % label)
    var size_value: int = int(unit_data.get("size", 0))
    lines.append("Size: %d" % size_value)
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
    var biomass: float = float(herd_data.get("biomass", 0.0))
    if biomass > 0.0:
        lines.append("Biomass: %.0f" % biomass)
        lines.append_array(_follow_herd_reward_lines(biomass))
    var x := int(herd_data.get("x", -1))
    var y := int(herd_data.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Position: (%d, %d)" % [x, y])
    var next_x := int(herd_data.get("next_x", -1))
    var next_y := int(herd_data.get("next_y", -1))
    if next_x >= 0 and next_y >= 0:
        lines.append("Next waypoint: (%d, %d)" % [next_x, next_y])
    return lines

func _follow_herd_reward_lines(biomass: float) -> Array[String]:
    var lines: Array[String] = []
    if biomass <= 0.0:
        return lines
    var consumption: float = min(biomass, HERD_CONSUMPTION_BIOMASS)
    if consumption <= 0.0:
        return lines
    var provisions: float = round(consumption * HERD_PROVISIONS_YIELD_PER_BIOMASS)
    var trade_goods: float = round(consumption * HERD_TRADE_GOODS_YIELD_PER_BIOMASS)
    var lore_progress: float = min(
        consumption * HERD_KNOWLEDGE_PROGRESS_PER_BIOMASS,
        HERD_KNOWLEDGE_PROGRESS_CAP
    )
    if lines.is_empty():
        lines.append("Follow Herd rewards:")
    lines.append("  - Morale +%.2f per band" % HERD_FOLLOW_MORALE_GAIN)
    if provisions > 0 or trade_goods > 0:
        lines.append("  - Supplies: +%d provisions, +%d trade goods" % [int(provisions), int(trade_goods)])
    if lore_progress > 0.0:
        lines.append("  - Fauna lore +%.1f%% progress" % (lore_progress * 100.0))
    lines.append("  - Reveals nearby fog (scouting pulse)")
    return lines

func _format_unit_list(entries: Array) -> String:
    var labels := PackedStringArray()
    for entry in entries:
        if not (entry is Dictionary):
            continue
        labels.append(_unit_label(entry))
    if labels.is_empty():
        return "—"
    return ", ".join(labels)

func _format_herd_list(entries: Array) -> String:
    var labels := PackedStringArray()
    for entry in entries:
        if not (entry is Dictionary):
            continue
        var label: String = String(entry.get("label", entry.get("id", "Herd")))
        labels.append(label)
    if labels.is_empty():
        return "—"
    return ", ".join(labels)

func _unit_label(entry: Dictionary) -> String:
    var label := String(entry.get("id", "Band"))
    var size_value: int = int(entry.get("size", 0))
    if size_value > 0:
        return "%s[%d]" % [label, size_value]
    return label

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
            var value_hex := HudStyle.WARN_HEX if String(kv[0]) == "Food" else HudStyle.INK_HEX
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

func _update_food_buttons(tile_info: Dictionary, has_unit: bool) -> void:
    if food_buttons == null or forage_button == null:
        return
    var module_key := String(tile_info.get("food_module", "")).strip_edges()
    if module_key == "":
        food_buttons.visible = false
        _selected_food_module = ""
        _selected_food_is_hunt = false
        return
    _selected_food_module = module_key
    var food_kind_value := String(tile_info.get("food_kind", "")).strip_edges()
    var is_game_trail := food_kind_value == "game_trail"
    _selected_food_is_hunt = is_game_trail
    var label := String(tile_info.get("food_module_label", "Harvest")).strip_edges()
    if label == "":
        label = module_key.capitalize()
    var pending_active := _pending_forage_matches_tile(tile_info)
    HudStyle.apply_button(forage_button, "armed" if pending_active else "primary")
    if pending_active:
        var pending_action := _pending_forage_action()
        if pending_action == FOOD_ACTION_HUNT:
            forage_button.text = "Cancel Hunt"
            forage_button.tooltip_text = "Cancel the pending hunt assignment for this tile."
        else:
            forage_button.text = "Cancel Harvest"
            forage_button.tooltip_text = "Cancel the pending harvest assignment for this tile."
    else:
        var turns := _travel_turns_for_tile(tile_info)
        var button_text := ""
        if is_game_trail:
            button_text = "Hunt Game"
        else:
            button_text = "Harvest %s" % label
            if turns > 0:
                button_text += " (~%d turns)" % turns
        forage_button.text = "%s  %s" % [FoodIcons.for_site(module_key, is_game_trail), button_text]
        var hint := _travel_eta_hint(tile_info)
        if hint == "":
            if is_game_trail:
                hint = "Select a band after clicking to send them on a hunt here."
            else:
                hint = "Select a band after clicking to send them here."
        forage_button.tooltip_text = hint
    forage_button.disabled = false
    food_buttons.visible = true

func clear_selection() -> void:
    _selected_unit.clear()
    _selected_herd.clear()
    _selected_food_module = ""
    _selected_food_is_hunt = false
    if not _pending_forage.is_empty():
        _cancel_pending_forage(false)
    # keep pending scout so user can still choose a tile after deselecting
    if _selected_tile_info.is_empty():
        if selection_panel != null:
            selection_panel.visible = false
        if food_buttons != null:
            food_buttons.visible = false
    else:
        _render_selection_panel(_selected_tile_info, {}, {})
    if unit_buttons != null:
        unit_buttons.visible = false
    if herd_buttons != null:
        herd_buttons.visible = false

func _travel_eta_line(tile_info: Dictionary) -> String:
    var distance := int(tile_info.get("nearest_unit_distance", -1))
    if distance < 0:
        return ""
    var label := String(tile_info.get("nearest_unit_label", "")).strip_edges()
    if label == "":
        label = "Band"
    var turns := _estimate_travel_turns(distance)
    if turns < 0:
        return ""
    var travel_text := "Nearest band: %s — %d tiles (~%d turns)" % [label, distance, turns]
    return travel_text

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

func _pending_scout_active() -> bool:
    return not _pending_scout_unit.is_empty()

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
    _pending_scout_unit.clear()
    _refresh_targeting()

func _begin_pending_forage(x: int, y: int, module_key: String, action: String) -> void:
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
