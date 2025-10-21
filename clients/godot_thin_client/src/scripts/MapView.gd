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
var terrain_overlay: PackedInt32Array = PackedInt32Array()
var terrain_palette: Dictionary = {}
var terrain_tags_overlay: PackedInt32Array = PackedInt32Array()
var terrain_tag_labels: Dictionary = {}
var units: Array = []
var routes: Array = []

var terrain_mode: bool = true

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
    terrain_overlay = PackedInt32Array(overlays.get("terrain", []))
    terrain_palette = overlays.get("terrain_palette", {})
    terrain_tags_overlay = PackedInt32Array(overlays.get("terrain_tags", []))
    terrain_tag_labels = overlays.get("terrain_tag_labels", {})
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
            var final_color: Color = _tile_color(x, y)
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

func _terrain_id_at(x: int, y: int) -> int:
    if terrain_overlay.is_empty() or grid_width == 0:
        return -1
    var index: int = y * grid_width + x
    if index < 0 or index >= terrain_overlay.size():
        return -1
    return int(terrain_overlay[index])

func _tile_color(x: int, y: int) -> Color:
    if terrain_mode:
        var terrain_id := _terrain_id_at(x, y)
        if terrain_id >= 0:
            return _terrain_color_for_id(terrain_id)
    var logistic_value: float = _value_at(logistics_overlay, x, y)
    var sentiment_value: float = _value_at(sentiment_overlay, x, y)
    var base_color: Color = GRID_COLOR
    var with_logistics: Color = base_color.lerp(LOGISTICS_COLOR, logistic_value)
    return with_logistics.lerp(SENTIMENT_COLOR, sentiment_value)

func _terrain_color_for_id(terrain_id: int) -> Color:
    match terrain_id:
        0:
            return Color8(11, 30, 61)
        1:
            return Color8(20, 64, 94)
        2:
            return Color8(28, 88, 114)
        3:
            return Color8(21, 122, 115)
        4:
            return Color8(47, 127, 137)
        5:
            return Color8(184, 176, 138)
        6:
            return Color8(155, 195, 123)
        7:
            return Color8(79, 124, 56)
        8:
            return Color8(92, 140, 99)
        9:
            return Color8(136, 182, 90)
        10:
            return Color8(201, 176, 120)
        11:
            return Color8(211, 165, 77)
        12:
            return Color8(91, 127, 67)
        13:
            return Color8(59, 79, 49)
        14:
            return Color8(100, 85, 106)
        15:
            return Color8(231, 195, 106)
        16:
            return Color8(138, 95, 60)
        17:
            return Color8(164, 135, 85)
        18:
            return Color8(224, 220, 210)
        19:
            return Color8(58, 162, 162)
        20:
            return Color8(166, 199, 207)
        21:
            return Color8(127, 183, 161)
        22:
            return Color8(209, 228, 236)
        23:
            return Color8(192, 202, 214)
        24:
            return Color8(111, 155, 75)
        25:
            return Color8(150, 126, 92)
        26:
            return Color8(122, 127, 136)
        27:
            return Color8(74, 106, 85)
        28:
            return Color8(182, 101, 68)
        29:
            return Color8(140, 52, 45)
        30:
            return Color8(64, 51, 61)
        31:
            return Color8(122, 110, 104)
        32:
            return Color8(76, 137, 145)
        33:
            return Color8(91, 70, 57)
        34:
            return Color8(46, 79, 92)
        35:
            return Color8(79, 75, 51)
        36:
            return Color8(47, 143, 178)
        _:
            return Color(0.2, 0.2, 0.2, 1.0)

func set_terrain_mode(enabled: bool) -> void:
    terrain_mode = enabled
    queue_redraw()

func toggle_terrain_mode() -> void:
    terrain_mode = not terrain_mode
    queue_redraw()

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
