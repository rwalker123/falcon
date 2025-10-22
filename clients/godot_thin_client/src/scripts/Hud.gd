extends CanvasLayer
class_name HudLayer

@onready var turn_label: Label = $TurnLabel
@onready var metrics_label: Label = $MetricsLabel
@onready var terrain_legend_panel: Panel = $TerrainLegendPanel
@onready var terrain_legend_list: VBoxContainer = $TerrainLegendPanel/LegendScroll/LegendList

func update_overlay(turn: int, metrics: Dictionary) -> void:
    turn_label.text = "Turn %d" % turn
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

func update_terrain_legend(entries: Array) -> void:
    for child in terrain_legend_list.get_children():
        child.queue_free()
    if entries.is_empty():
        terrain_legend_panel.visible = false
        return
    terrain_legend_panel.visible = true
    for entry in entries:
        if typeof(entry) != TYPE_DICTIONARY:
            continue
        var row := HBoxContainer.new()
        row.custom_minimum_size = Vector2(0, 18)
        row.size_flags_horizontal = Control.SIZE_EXPAND_FILL

        var swatch := ColorRect.new()
        swatch.custom_minimum_size = Vector2(16, 16)
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        swatch.color = entry.get("color", Color.WHITE)
        row.add_child(swatch)

        var label := Label.new()
        label.text = "%02d %s" % [int(entry.get("id", 0)), str(entry.get("label", ""))]
        label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        row.add_child(label)

        terrain_legend_list.add_child(row)
