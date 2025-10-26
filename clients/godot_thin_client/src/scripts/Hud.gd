extends CanvasLayer
class_name HudLayer

signal ui_zoom_delta(delta: float)
signal ui_zoom_reset

@onready var turn_label: Label = $TurnLabel
@onready var metrics_label: Label = $MetricsLabel
@onready var inspector_font_label: Label = $InspectorFontLabel
@onready var zoom_controls: HBoxContainer = $ZoomControls
@onready var zoom_out_button: Button = $ZoomControls/ZoomOutButton
@onready var zoom_reset_button: Button = $ZoomControls/ZoomResetButton
@onready var zoom_in_button: Button = $ZoomControls/ZoomInButton
@onready var terrain_legend_panel: Panel = $TerrainLegendPanel
@onready var terrain_legend_container: VBoxContainer = $TerrainLegendPanel/LegendContainer
@onready var terrain_legend_scroll: ScrollContainer = $TerrainLegendPanel/LegendContainer/LegendScroll
@onready var terrain_legend_list: VBoxContainer = $TerrainLegendPanel/LegendContainer/LegendScroll/LegendList
@onready var terrain_legend_title: Label = $TerrainLegendPanel/LegendContainer/LegendTitle
@onready var terrain_legend_description: Label = $TerrainLegendPanel/LegendContainer/LegendDescription

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

var overlay_legend: Dictionary = {}

func _ready() -> void:
    set_ui_zoom(1.0)
    _connect_zoom_controls()
    _refresh_existing_legend_rows()
    _resize_legend_panel(_legend_list_size())

func update_overlay(turn: int, metrics: Dictionary) -> void:
    turn_label.text = "Turn %d" % turn
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

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

func _on_zoom_out_pressed() -> void:
    emit_signal("ui_zoom_delta", -1.0)

func _on_zoom_reset_pressed() -> void:
    emit_signal("ui_zoom_reset")

func _on_zoom_in_pressed() -> void:
    emit_signal("ui_zoom_delta", 1.0)

func update_overlay_legend(legend: Dictionary) -> void:
    overlay_legend = legend.duplicate(true) if legend is Dictionary else {}
    for child in terrain_legend_list.get_children():
        child.queue_free()
    if overlay_legend.is_empty():
        terrain_legend_panel.visible = false
        terrain_legend_description.visible = false
        terrain_legend_description.text = ""
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
    for label in [turn_label, metrics_label, inspector_font_label]:
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

    var offset_right: float = -LEGEND_RIGHT_MARGIN
    terrain_legend_panel.offset_right = offset_right
    terrain_legend_panel.offset_left = offset_right - padded_width
    terrain_legend_panel.offset_bottom = terrain_legend_panel.offset_top + clamped_height
    terrain_legend_panel.custom_minimum_size = Vector2(padded_width, clamped_height)

    var scroll_width: float = max(padded_width - (LEGEND_WIDTH_PADDING * 0.5), LEGEND_MIN_WIDTH - LEGEND_RIGHT_MARGIN)
    scroll_width = clamp(scroll_width, LEGEND_MIN_WIDTH * 0.5, padded_width - LEGEND_RIGHT_MARGIN)
    terrain_legend_scroll.custom_minimum_size = Vector2(scroll_width, scroll_height)
    terrain_legend_scroll.scroll_vertical = 0
