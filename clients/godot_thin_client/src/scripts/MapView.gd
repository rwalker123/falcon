extends Node2D
class_name MapView

const LOGISTICS_COLOR := Color(0.15, 0.45, 1.0, 1.0)
const SENTIMENT_COLOR := Color(1.0, 0.35, 0.25, 1.0)
const GRID_COLOR := Color(0.06, 0.08, 0.12, 1.0)
const GRID_LINE_COLOR := Color(0.1, 0.12, 0.18, 0.45)
const SQRT3 := 1.7320508075688772
const SIN_60 := 0.8660254037844386

var grid_width: int = 0
var grid_height: int = 0
var logistics_overlay: PackedFloat32Array = PackedFloat32Array()
var sentiment_overlay: PackedFloat32Array = PackedFloat32Array()
var units: Array = []
var routes: Array = []

var last_hex_radius: float = 48.0
var last_origin: Vector2 = Vector2.ZERO
var last_map_size: Vector2 = Vector2.ZERO

var faction_colors: Dictionary = {
    "Aurora": Color(0.55, 0.85, 1.0, 1.0),
    "Obsidian": Color(0.95, 0.62, 0.2, 1.0),
    "Verdant": Color(0.4, 0.9, 0.55, 1.0)
}

func display_snapshot(snapshot: Dictionary) -> Dictionary:
    var grid: Dictionary = snapshot.get("grid", {})
    grid_width = int(grid.get("width", 0))
    grid_height = int(grid.get("height", 0))

    var overlays: Dictionary = snapshot.get("overlays", {})
    logistics_overlay = PackedFloat32Array(overlays.get("logistics", []))
    sentiment_overlay = PackedFloat32Array(overlays.get("contrast", []))
    units = Array(snapshot.get("units", []))
    routes = Array(snapshot.get("orders", []))

    _update_layout_metrics()
    queue_redraw()

    return {
        "unit_count": units.size(),
        "avg_logistics": _average(logistics_overlay),
        "avg_sentiment": _average(sentiment_overlay)
    }

func _draw() -> void:
    if grid_width == 0 or grid_height == 0:
        return

    _update_layout_metrics()

    var radius: float = last_hex_radius
    var origin: Vector2 = last_origin

    for y in range(grid_height):
        for x in range(grid_width):
            var center: Vector2 = _hex_center(x, y, radius, origin)
            var logistic_value: float = _value_at(logistics_overlay, x, y)
            var sentiment_value: float = _value_at(sentiment_overlay, x, y)

            var base_color: Color = GRID_COLOR
            var with_logistics: Color = base_color.lerp(LOGISTICS_COLOR, logistic_value)
            var final_color: Color = with_logistics.lerp(SENTIMENT_COLOR, sentiment_value)
            var polygon_points := _hex_points(center, radius)
            draw_polygon(polygon_points, PackedColorArray([final_color, final_color, final_color, final_color, final_color, final_color]))
            draw_polyline(_hex_points(center, radius, true), GRID_LINE_COLOR, 2.0, true)

    for unit in units:
        _draw_unit(unit, radius, origin)

    for order in routes:
        _draw_route(order, radius, origin)

func _draw_unit(unit: Dictionary, radius: float, origin: Vector2) -> void:
    var position: Array = Array(unit.get("pos", [0, 0]))
    if position.size() != 2:
        return
    var center: Vector2 = _hex_center(int(position[0]), int(position[1]), radius, origin)
    var marker_radius: float = radius * 0.45
    var color: Color = faction_colors.get(unit.get("faction", ""), Color(0.9, 0.9, 0.9, 1.0))
    draw_circle(center, marker_radius, color)
    draw_arc(center, marker_radius, 0, TAU, 12, Color(0, 0, 0, 0.4), 2.5)

    var label: String = str(unit.get("id", ""))
    if label != "":
        var font: Font = ThemeDB.fallback_font
        if font != null:
            draw_string(font, center + Vector2(-marker_radius, marker_radius * 0.1), label, HORIZONTAL_ALIGNMENT_LEFT, marker_radius * 2.0, 16, Color(0.05, 0.05, 0.05, 0.85))

func _draw_route(order: Dictionary, radius: float, origin: Vector2) -> void:
    var path: Array = order.get("path", [])
    if path.is_empty():
        return
    var color: Color = faction_colors.get(order.get("faction", ""), Color(0.95, 0.9, 0.6, 0.8))
    var points: Array[Vector2] = []
    for waypoint in path:
        if waypoint.size() != 2:
            continue
        points.append(_hex_center(int(waypoint[0]), int(waypoint[1]), radius, origin))
    if points.size() < 2:
        return
    for i in range(points.size() - 1):
        draw_line(points[i], points[i + 1], color, 3.0)

func _value_at(data: PackedFloat32Array, x: int, y: int) -> float:
    if data.is_empty() or grid_width == 0:
        return 0.0
    var index: int = y * grid_width + x
    if index < 0 or index >= data.size():
        return 0.0
    return clamp(float(data[index]), 0.0, 1.0)

func _average(data: PackedFloat32Array) -> float:
    if data.is_empty():
        return 0.0
    var total: float = 0.0
    for value in data:
        total += float(value)
    return total / data.size()

func _hex_center(col: int, row: int, radius: float, origin: Vector2) -> Vector2:
    var axial := _offset_to_axial(col, row)
    return origin + _axial_center(axial.x, axial.y, radius)

func _axial_center(q: int, r: int, radius: float) -> Vector2:
    var fq := float(q)
    var fr := float(r)
    var x: float = radius * (SQRT3 * fq + SQRT3 * 0.5 * fr)
    var y: float = radius * (1.5 * fr)
    return Vector2(x, y)

func _offset_to_axial(col: int, row: int) -> Vector2i:
    # odd-r horizontal layout (flat-top hexes)
    var q := col - ((row - (row & 1)) >> 1)
    var r := row
    return Vector2i(q, r)

func _hex_points(center: Vector2, radius: float, closed: bool = false) -> PackedVector2Array:
    var points := PackedVector2Array()
    for i in range(6):
        var angle := deg_to_rad(60.0 * float(i) + 30.0)
        points.append(center + Vector2(radius * cos(angle), radius * sin(angle)))
    if closed:
        points.append(points[0])
    return points

func _update_layout_metrics() -> void:
    if grid_width <= 0 or grid_height <= 0:
        return
    var viewport_size: Vector2 = get_viewport_rect().size
    var unit_bounds := _compute_bounds(1.0)
    if unit_bounds.size.x <= 0.0 or unit_bounds.size.y <= 0.0:
        return
    var radius_from_width: float = viewport_size.x / unit_bounds.size.x
    var radius_from_height: float = viewport_size.y / unit_bounds.size.y
    last_hex_radius = min(radius_from_width, radius_from_height)
    var scaled_bounds := _compute_bounds(last_hex_radius)
    last_map_size = scaled_bounds.size
    last_origin = (viewport_size - last_map_size) * 0.5 - scaled_bounds.position

func get_world_center() -> Vector2:
    return last_origin + last_map_size * 0.5

func get_hex_radius() -> float:
    return last_hex_radius

func _compute_bounds(radius: float) -> Rect2:
    var min_x := INF
    var max_x := -INF
    var min_y := INF
    var max_y := -INF
    for col in range(grid_width):
        for row in range(grid_height):
            var axial := _offset_to_axial(col, row)
            var center := _axial_center(axial.x, axial.y, radius)
            min_x = min(min_x, center.x - radius)
            max_x = max(max_x, center.x + radius)
            min_y = min(min_y, center.y - radius)
            max_y = max(max_y, center.y + radius)
    if min_x == INF:
        return Rect2(Vector2.ZERO, Vector2.ONE)
    return Rect2(Vector2(min_x, min_y), Vector2(max_x - min_x, max_y - min_y))
