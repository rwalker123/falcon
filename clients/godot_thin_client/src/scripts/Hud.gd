extends CanvasLayer
class_name HudLayer

const Typography = preload("res://src/scripts/Typography.gd")

@onready var turn_label: Label = $TurnLabel
@onready var metrics_label: Label = $MetricsLabel
@onready var inspector_font_label: Label = $InspectorFontLabel
@onready var terrain_legend_panel: Panel = $TerrainLegendPanel
@onready var terrain_legend_scroll: ScrollContainer = $TerrainLegendPanel/LegendScroll
@onready var terrain_legend_list: VBoxContainer = $TerrainLegendPanel/LegendScroll/LegendList
@onready var terrain_legend_title: Label = $TerrainLegendPanel/LegendTitle

const LEGEND_SWATCH_FRACTION := 0.75
const LEGEND_MIN_ROW_HEIGHT := 20.0
const LEGEND_ROW_PADDING := 6.0
const LEGEND_BASE_PADDING := 56.0
const LEGEND_MAX_HEIGHT := 640.0
const LEGEND_MIN_WIDTH := 320.0
const LEGEND_WIDTH_PADDING := 120.0
const LEGEND_RIGHT_MARGIN := 16.0
const STACK_ADDITIONAL_MARGIN := 16.0

func _ready() -> void:
    Typography.initialize()
    apply_typography()

func apply_typography() -> void:
    Typography.initialize()
    _apply_typography_style([turn_label, metrics_label], Typography.STYLE_BODY)
    _apply_typography_style([inspector_font_label], Typography.STYLE_CAPTION)
    _apply_typography_style([terrain_legend_title], Typography.STYLE_SUBHEADING)
    if terrain_legend_panel != null:
        Typography.apply_theme(terrain_legend_panel)
    _refresh_existing_legend_rows()
    _resize_legend_panel(_legend_list_size())

func update_overlay(turn: int, metrics: Dictionary) -> void:
    turn_label.text = "Turn %d" % turn
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

func set_inspector_font_size(font_size: int) -> void:
    if inspector_font_label == null:
        return
    inspector_font_label.text = "Inspector font: %d" % font_size

func update_terrain_legend(entries: Array) -> void:
    for child in terrain_legend_list.get_children():
        child.queue_free()
    if entries.is_empty():
        terrain_legend_panel.visible = false
        return
    terrain_legend_panel.visible = true
    var row_height := _legend_row_height()
    var swatch_size := _legend_swatch_size(row_height)
    for entry in entries:
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
        label.text = "%02d %s" % [int(entry.get("id", 0)), str(entry.get("label", ""))]
        label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        Typography.apply(label, Typography.STYLE_LEGEND)
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
            size = Typography.line_height(Typography.STYLE_BODY)
        max_bottom = max(max_bottom, top + size)
    if max_bottom <= 0.0:
        max_bottom = Typography.line_height(Typography.STYLE_HEADING)
    return max_bottom + STACK_ADDITIONAL_MARGIN

func _legend_row_height() -> float:
    return max(
        Typography.line_height(Typography.STYLE_LEGEND) + LEGEND_ROW_PADDING,
        LEGEND_MIN_ROW_HEIGHT
    )

func _legend_swatch_size(row_height: float) -> Vector2:
    var side: float = max(row_height * LEGEND_SWATCH_FRACTION, LEGEND_MIN_ROW_HEIGHT * 0.6)
    return Vector2(side, side)

func _apply_typography_style(controls: Array, style: StringName) -> void:
    for control in controls:
        if control is Control:
            Typography.apply(control, style)

func _refresh_existing_legend_rows() -> void:
    var row_height := _legend_row_height()
    var swatch_size := _legend_swatch_size(row_height)
    for child in terrain_legend_list.get_children():
        if child is HBoxContainer:
            var row := child as HBoxContainer
            row.custom_minimum_size = Vector2(0, row_height)
            for grandchild in row.get_children():
                if grandchild is Label:
                    Typography.apply(grandchild, Typography.STYLE_LEGEND)
                elif grandchild is ColorRect:
                    (grandchild as ColorRect).custom_minimum_size = swatch_size
    _resize_legend_panel(_legend_list_size())

func _legend_list_size() -> Vector2:
    if terrain_legend_list == null:
        return Vector2.ZERO
    return terrain_legend_list.get_combined_minimum_size()

func _resize_legend_panel(list_size: Vector2) -> void:
    if terrain_legend_panel == null or terrain_legend_scroll == null:
        return
    var content_width: float = list_size.x
    var content_height: float = list_size.y
    var padded_width: float = max(content_width + LEGEND_WIDTH_PADDING, LEGEND_MIN_WIDTH)
    var padded_height: float = LEGEND_BASE_PADDING + content_height
    var clamped_height: float = clamp(padded_height, LEGEND_BASE_PADDING + LEGEND_MIN_ROW_HEIGHT, LEGEND_MAX_HEIGHT)
    var scroll_height: float = clamp(clamped_height - LEGEND_BASE_PADDING, LEGEND_MIN_ROW_HEIGHT, LEGEND_MAX_HEIGHT - LEGEND_BASE_PADDING)

    var offset_right: float = -LEGEND_RIGHT_MARGIN
    terrain_legend_panel.offset_right = offset_right
    terrain_legend_panel.offset_left = offset_right - padded_width
    terrain_legend_panel.offset_bottom = terrain_legend_panel.offset_top + clamped_height
    terrain_legend_panel.custom_minimum_size = Vector2(padded_width, clamped_height)

    var scroll_width: float = max(padded_width - (LEGEND_WIDTH_PADDING * 0.5), LEGEND_MIN_WIDTH - LEGEND_RIGHT_MARGIN)
    scroll_width = clamp(scroll_width, LEGEND_MIN_WIDTH * 0.5, padded_width - LEGEND_RIGHT_MARGIN)
    terrain_legend_scroll.custom_minimum_size = Vector2(scroll_width, scroll_height)
    terrain_legend_scroll.scroll_vertical = 0
