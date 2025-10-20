extends CanvasLayer
class_name HudLayer

@onready var turn_label: Label = $TurnLabel
@onready var metrics_label: Label = $MetricsLabel

func update_overlay(turn: int, metrics: Dictionary) -> void:
    turn_label.text = "Turn %d" % turn
    var unit_count: int = int(metrics.get("unit_count", 0))
    var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
    var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
    metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]
