extends Control
class_name TickSparkline

@export var stroke_color: Color = Color(0.2, 0.8, 1.0, 1.0)
@export var fill_color: Color = Color(0.2, 0.8, 1.0, 0.18)
@export var axis_color: Color = Color(0.35, 0.45, 0.6, 1.0)
@export var background_color: Color = Color(0.05, 0.07, 0.1, 0.35)

var _samples: PackedFloat32Array = PackedFloat32Array()
var _max_value: float = 1.0

func set_samples(values: Array) -> void:
    var converted: PackedFloat32Array = PackedFloat32Array()
    for value in values:
        converted.append(float(value))
    _samples = converted
    _max_value = 0.0
    for sample in _samples:
        _max_value = max(_max_value, sample)
    if _max_value <= 0.0:
        _max_value = 1.0
    queue_redraw()

func clear_samples() -> void:
    _samples = PackedFloat32Array()
    _max_value = 1.0
    queue_redraw()

func _draw() -> void:
    var rect: Rect2 = Rect2(Vector2.ZERO, size)
    if rect.size.x <= 2.0 or rect.size.y <= 2.0:
        return
    draw_rect(rect, background_color, true)
    var baseline: float = rect.size.y - 1.0
    draw_line(Vector2(0.0, baseline), Vector2(rect.size.x, baseline), axis_color, 1.0)
    if _samples.is_empty():
        return
    var count: int = _samples.size()
    if count == 1:
        var single_ratio: float = float(clamp(_samples[0] / _max_value, 0.0, 1.0))
        var y_single: float = rect.size.y - single_ratio * rect.size.y
        draw_circle(Vector2(rect.size.x, float(clamp(y_single, 0.0, rect.size.y))), 3.0, stroke_color)
        return
    var points: PackedVector2Array = PackedVector2Array()
    for idx in range(count):
        var t: float = float(idx) / float(max(count - 1, 1))
        var x: float = rect.size.x * t
        var value: float = float(_samples[idx])
        var ratio: float = 0.0 if _max_value <= 0.0 else float(clamp(value / _max_value, 0.0, 1.0))
        var y: float = rect.size.y - (ratio * rect.size.y)
        points.append(Vector2(x, float(clamp(y, 0.0, rect.size.y))))
    if points.size() >= 2:
        var fill_points: PackedVector2Array = PackedVector2Array(points)
        fill_points.append(Vector2(rect.size.x, rect.size.y))
        fill_points.append(Vector2(0.0, rect.size.y))
        fill_points.append(points[0])
        draw_colored_polygon(fill_points, fill_color)
        draw_polyline(points, stroke_color, 2.0, true)
