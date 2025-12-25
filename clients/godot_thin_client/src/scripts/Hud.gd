extends CanvasLayer
class_name HudLayer

signal ui_zoom_delta(delta: float)
signal ui_zoom_reset
signal unit_scout_requested(x: int, y: int, band_entity_bits: int)
signal unit_found_camp_requested(x: int, y: int)
signal herd_follow_requested(herd_id: String)
signal forage_requested(x: int, y: int, module_key: String)
signal next_turn_requested(steps: int)

@onready var campaign_title_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignTitleLabel
@onready var campaign_subtitle_label: Label = $LayoutRoot/RootColumn/TopBar/CampaignBlock/CampaignSubtitleLabel
@onready var turn_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/TurnLabel
@onready var metrics_label: Label = $LayoutRoot/RootColumn/TopBar/TurnBlock/MetricsLabel
@onready var zoom_controls: HBoxContainer = $LayoutRoot/RootColumn/TopBar/ZoomControls
@onready var zoom_out_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomOutButton
@onready var zoom_reset_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomResetButton
@onready var zoom_in_button: Button = $LayoutRoot/RootColumn/TopBar/ZoomControls/ZoomInButton
@onready var terrain_legend_panel: Panel = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel
@onready var terrain_legend_container: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel/LegendContainer
@onready var terrain_legend_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel/LegendContainer/LegendScroll
@onready var terrain_legend_list: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel/LegendContainer/LegendScroll/LegendList
@onready var terrain_legend_title: Label = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel/LegendContainer/LegendTitle
@onready var terrain_legend_description: Label = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/TerrainLegendPanel/LegendContainer/LegendDescription
@onready var victory_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel
@onready var victory_status_label: RichTextLabel = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack/VictoryPanel/Margin/VictoryLabel
@onready var command_feed_panel: Panel = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel
@onready var command_feed_heading: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel/MarginContainer/VBoxContainer/CommandFeedHeading
@onready var command_feed_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel/MarginContainer/VBoxContainer/CommandFeedScroll
@onready var command_feed_label: RichTextLabel = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/CommandFeedPanel/MarginContainer/VBoxContainer/CommandFeedScroll/CommandFeedLabel
@onready var selection_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel as PanelContainer
@onready var selection_margin: MarginContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin
@onready var selection_scroll: ScrollContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll
@onready var selection_content: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox
@onready var selection_title: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/SelectionTitle
@onready var selection_detail: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/SelectionDetail
@onready var unit_buttons: HBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/UnitButtons
@onready var unit_scout_button: Button = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/UnitButtons/UnitScoutButton
@onready var unit_camp_button: Button = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/UnitButtons/UnitCampButton
@onready var herd_buttons: HBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/HerdButtons
@onready var follow_herd_button: Button = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/HerdButtons/FollowHerdButton
@onready var food_buttons: HBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/FoodButtons
@onready var forage_button: Button = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/SelectionPanel/Margin/Scroll/VBox/FoodButtons/ForageButton
@onready var stockpile_panel: PanelContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel
@onready var stockpile_title: Label = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileTitle
@onready var stockpile_list: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack/StockpilePanel/StockpileMargin/StockpileVBox/StockpileList
@onready var left_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/LeftDock/LeftScroll/LeftStack
@onready var right_stack: VBoxContainer = $LayoutRoot/RootColumn/ContentRow/RightDock/RightScroll/RightStack
@onready var next_turn_button: Button = $LayoutRoot/RootColumn/BottomBar/NextTurnButton

var tooltip_panel: PanelContainer
var tooltip_label: Label

const LEGEND_SWATCH_FRACTION := 0.75
const LEGEND_MIN_ROW_HEIGHT := 20.0
const LEGEND_ROW_PADDING := 6.0
const LEGEND_MAX_HEIGHT := 640.0
const LEGEND_MIN_WIDTH := 320.0
const LEGEND_WIDTH_PADDING := 120.0
const LEGEND_RIGHT_MARGIN := 16.0
const LEGEND_VERTICAL_PADDING := 16.0
const LEGEND_HEADER_SPACING := 6.0
const STACK_ADDITIONAL_MARGIN := 16.0
const COMMAND_FEED_LIMIT := 6
const COMMAND_FEED_MIN_HEIGHT := 120.0
const COMMAND_FEED_MAX_HEIGHT := 360.0
const COMMAND_FEED_BOTTOM_MARGIN := 24.0
const SELECTION_PANEL_WIDTH := 320.0
const SELECTION_PANEL_MIN_HEIGHT := 140.0
const SELECTION_PANEL_MAX_HEIGHT := 420.0
const SELECTION_PANEL_BOTTOM_MARGIN := 40.0
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
var _stockpile_totals: Dictionary = {}
var travel_tiles_per_turn: float = DEFAULT_TRAVEL_SPEED
var travel_preview_turn_cap: int = DEFAULT_TRAVEL_PREVIEW_LIMIT
var dock_registry := {
    "left": [],
    "right": [],
}

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
    register_dock_panel(selection_panel, "left", 10)
    register_dock_panel(stockpile_panel, "left", 20)
    register_dock_panel(command_feed_panel, "left", 30)
    register_dock_panel(victory_panel, "right", 10)
    register_dock_panel(terrain_legend_panel, "right", 20)
    if stockpile_panel != null:
        stockpile_panel.visible = false
    if stockpile_title != null:
        stockpile_title.text = "Stockpiles"

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

func register_dock_panel(panel: Control, slot: String, priority: int) -> void:
    if panel == null or not dock_registry.has(slot):
        print("[HUD] register_dock_panel: Invalid panel or slot: ", slot)
        return
    print("[HUD] register_dock_panel: slot=", slot, " priority=", priority, " panel=", panel.name)
    var bucket: Array = dock_registry[slot]
    var found := false
    for entry in bucket:
        if entry.get("panel") == panel:
            entry["priority"] = priority
            found = true
            break
    if not found:
        bucket.append({"panel": panel, "priority": priority})
    bucket.sort_custom(Callable(self, "_dock_sort"))
    _apply_dock_order(slot)

func _dock_sort(a: Dictionary, b: Dictionary) -> bool:
    return int(a.get("priority", 0)) < int(b.get("priority", 0))

func _dock_container(slot: String) -> VBoxContainer:
    if slot == "left":
        return left_stack
    if slot == "right":
        return right_stack
    print("[HUD] _dock_container: Unknown slot: ", slot)
    return null

func _apply_dock_order(slot: String) -> void:
    var container := _dock_container(slot)
    if container == null:
        return
    var bucket: Array = dock_registry.get(slot, [])
    for idx in range(bucket.size()):
        var panel: Control = bucket[idx].get("panel")
        if panel == null:
            continue
        if panel.get_parent() != container:
            container.add_child(panel)
        container.move_child(panel, idx)

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
    terrain_legend_title.text = title
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
    if selection_panel == null or selection_detail == null or selection_title == null:
        return
    selection_panel.visible = true
    var title_text := "Tile"
    if not tile_info.is_empty():
        var x := int(tile_info.get("x", -1))
        var y := int(tile_info.get("y", -1))
        title_text = "Tile (%d, %d)" % [x, y]
    selection_title.text = title_text
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
    selection_detail.text = _join_lines(detail_lines)
    if selection_scroll != null:
        selection_scroll.set_deferred("scroll_vertical", 0)
    if unit_buttons != null:
        unit_buttons.visible = not unit_data.is_empty()
    if herd_buttons != null:
        herd_buttons.visible = not herd_data.is_empty()
    _update_food_buttons(tile_info, not unit_data.is_empty())
    _apply_selection_panel_size()


func _tile_summary_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    var terrain_label := String(tile_info.get("terrain_label", "Unknown"))
    lines.append("Biome: %s" % terrain_label)
    var tags_text := String(tile_info.get("tags_text", "none"))
    lines.append("Tags: %s" % tags_text)
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

func _apply_selection_panel_size() -> void:
    if selection_panel == null or selection_content == null:
        return
    var content_height: float = selection_content.get_combined_minimum_size().y
    var margin_height: float = 0.0
    if selection_margin != null:
        margin_height = float(
            selection_margin.get_theme_constant("margin_top", "MarginContainer")
            + selection_margin.get_theme_constant("margin_bottom", "MarginContainer")
        )
    if selection_panel != null and selection_panel.has_method("fit_to_content"):
        selection_panel.call("fit_to_content", content_height, margin_height, selection_scroll)
        return
    _legacy_selection_panel_size(content_height, margin_height)

func _legacy_selection_panel_size(content_height: float, margin_height: float) -> void:
    var desired_height: float = content_height + margin_height
    var viewport := get_viewport()
    var viewport_height: float = viewport.get_visible_rect().size.y if viewport != null else DisplayServer.window_get_size().y
    var max_available: float = max(
        SELECTION_PANEL_MIN_HEIGHT,
        viewport_height - selection_panel.position.y - SELECTION_PANEL_BOTTOM_MARGIN
    )
    var clamped_height: float = clamp(
        desired_height,
        SELECTION_PANEL_MIN_HEIGHT,
        min(max_available, SELECTION_PANEL_MAX_HEIGHT)
    )
    selection_panel.custom_minimum_size = Vector2(SELECTION_PANEL_WIDTH, clamped_height)
    selection_panel.size = Vector2(SELECTION_PANEL_WIDTH, clamped_height)
    if selection_scroll != null:
        if desired_height > max_available:
            selection_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
        else:
            selection_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
            selection_scroll.scroll_vertical = 0

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
        forage_button.text = button_text
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
        if selection_scroll != null:
            selection_scroll.scroll_vertical = 0
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
    if command_feed_heading != null:
        command_feed_heading.text = "Command Feed"
    if _command_feed_entries.is_empty():
        command_feed_label.text = "[i]No command activity yet.[/i]"
    else:
        command_feed_label.text = "\n\n".join(_command_feed_entries)
    _apply_command_feed_size()

func _apply_command_feed_size() -> void:
    if command_feed_panel == null or command_feed_label == null:
        return
    command_feed_label.reset_size()
    var heading_height: float = 0.0
    if command_feed_heading != null:
        heading_height = command_feed_heading.get_combined_minimum_size().y
    var content_height: float = command_feed_label.get_content_height()
    command_feed_label.custom_minimum_size.y = content_height
    if command_feed_scroll != null:
        command_feed_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
    if command_feed_panel != null and command_feed_panel.has_method("fit_to_content"):
        command_feed_panel.call("fit_to_content", content_height, heading_height + 24.0, command_feed_scroll)
        return
    _legacy_command_feed_size(content_height, heading_height)

func _legacy_command_feed_size(content_height: float, heading_height: float) -> void:
    var desired_height: float = heading_height + content_height + 24.0
    var viewport := get_viewport()
    var viewport_height: float = viewport.get_visible_rect().size.y if viewport != null else DisplayServer.window_get_size().y
    var max_available: float = viewport_height - command_feed_panel.offset_top - COMMAND_FEED_BOTTOM_MARGIN
    var clamped_height: float = clamp(
        desired_height,
        COMMAND_FEED_MIN_HEIGHT,
        min(COMMAND_FEED_MAX_HEIGHT, max_available)
    )
    var top := command_feed_panel.offset_top
    command_feed_panel.offset_bottom = top + clamped_height
    if command_feed_scroll != null:
        if desired_height > clamped_height + 0.5:
            command_feed_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
        else:
            command_feed_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
            command_feed_scroll.scroll_vertical = 0

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

func _cancel_pending_forage(refresh: bool) -> void:
    _pending_forage.clear()
    if refresh:
        _render_selection_panel(_selected_tile_info, _selected_unit, _selected_herd)

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

func _resize_legend_panel(list_size: Vector2) -> void:
    if terrain_legend_panel == null or terrain_legend_scroll == null:
        return
    var header_width: float = 0.0
    if terrain_legend_title != null:
        header_width = max(header_width, terrain_legend_title.get_combined_minimum_size().x)
    if terrain_legend_description != null and terrain_legend_description.visible:
        header_width = max(header_width, terrain_legend_description.get_combined_minimum_size().x)
    var content_width: float = max(list_size.x, header_width)
    var title_height: float = 0.0
    if terrain_legend_title != null:
        title_height += terrain_legend_title.get_combined_minimum_size().y
    if terrain_legend_description != null and terrain_legend_description.visible:
        title_height += LEGEND_HEADER_SPACING
        title_height += terrain_legend_description.get_combined_minimum_size().y
    var list_height: float = terrain_legend_list.get_combined_minimum_size().y
    var padded_width: float = max(content_width + LEGEND_WIDTH_PADDING, LEGEND_MIN_WIDTH)
    var header_and_padding: float = title_height + LEGEND_VERTICAL_PADDING
    var padded_height: float = header_and_padding + list_height
    var min_height: float = header_and_padding + LEGEND_MIN_ROW_HEIGHT
    var clamped_height: float = clamp(padded_height, min_height, LEGEND_MAX_HEIGHT)
    var available_for_rows: float = clamped_height - header_and_padding
    var scroll_height: float = clamp(available_for_rows, LEGEND_MIN_ROW_HEIGHT, LEGEND_MAX_HEIGHT - header_and_padding)

    terrain_legend_panel.set_anchors_preset(Control.PRESET_TOP_RIGHT)
    terrain_legend_panel.offset_left = -padded_width - LEGEND_RIGHT_MARGIN
    terrain_legend_panel.offset_right = -LEGEND_RIGHT_MARGIN
    terrain_legend_panel.offset_top = 0
    terrain_legend_panel.offset_bottom = clamped_height
    terrain_legend_panel.custom_minimum_size = Vector2(padded_width, clamped_height)

    var scroll_width: float = max(padded_width - (LEGEND_WIDTH_PADDING * 0.5), LEGEND_MIN_WIDTH - LEGEND_RIGHT_MARGIN)
    scroll_width = clamp(scroll_width, LEGEND_MIN_WIDTH * 0.5, padded_width - LEGEND_RIGHT_MARGIN)
    terrain_legend_scroll.custom_minimum_size = Vector2(scroll_width, scroll_height)
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

func show_tooltip(info: Dictionary) -> void:
    if tooltip_panel == null:
        return
        
    if info.is_empty():
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
