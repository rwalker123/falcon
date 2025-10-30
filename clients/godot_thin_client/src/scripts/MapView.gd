extends Node2D
class_name MapView

signal hex_selected(col: int, row: int, terrain_id: int)
signal overlay_legend_changed(legend: Dictionary)

const LOGISTICS_COLOR := Color(0.15, 0.45, 1.0, 1.0)
const SENTIMENT_COLOR := Color(1.0, 0.35, 0.25, 1.0)
const CORRUPTION_COLOR := Color(0.92, 0.58, 0.18, 1.0)
const FOG_COLOR := Color(0.6, 0.78, 0.95, 1.0)
const CULTURE_COLOR := Color(0.72, 0.36, 0.88, 1.0)
const MILITARY_COLOR := Color(0.36, 0.7, 0.43, 1.0)
const CRISIS_COLOR := Color(0.92, 0.24, 0.46, 1.0)
const GRID_COLOR := Color(0.06, 0.08, 0.12, 1.0)
const GRID_LINE_COLOR := Color(0.1, 0.12, 0.18, 0.45)
const SQRT3 := 1.7320508075688772
const SIN_60 := 0.8660254037844386
const MIN_ZOOM_FACTOR := 0.4
const MAX_ZOOM_FACTOR := 4.0
const MOUSE_ZOOM_STEP := 0.2
const KEYBOARD_ZOOM_SPEED := 0.8
const KEYBOARD_PAN_SPEED := 600.0

const OVERLAY_COLORS := {
    "logistics": LOGISTICS_COLOR,
    "sentiment": SENTIMENT_COLOR,
    "corruption": CORRUPTION_COLOR,
    "fog": FOG_COLOR,
    "culture": CULTURE_COLOR,
    "military": MILITARY_COLOR,
    "crisis": CRISIS_COLOR
}

const CRISIS_SEVERITY_COLORS := {
    "critical": Color(0.96, 0.28, 0.38, 0.95),
    "warn": Color(0.97, 0.75, 0.28, 0.92),
    "safe": Color(0.5, 0.82, 0.72, 0.85)
}

const TERRAIN_COLORS := {
    0: Color8(11, 30, 61),
    1: Color8(20, 64, 94),
    2: Color8(28, 88, 114),
    3: Color8(21, 122, 115),
    4: Color8(47, 127, 137),
    5: Color8(184, 176, 138),
    6: Color8(155, 195, 123),
    7: Color8(79, 124, 56),
    8: Color8(92, 140, 99),
    9: Color8(136, 182, 90),
    10: Color8(201, 176, 120),
    11: Color8(211, 165, 77),
    12: Color8(91, 127, 67),
    13: Color8(59, 79, 49),
    14: Color8(100, 85, 106),
    15: Color8(231, 195, 106),
    16: Color8(138, 95, 60),
    17: Color8(164, 135, 85),
    18: Color8(224, 220, 210),
    19: Color8(58, 162, 162),
    20: Color8(166, 199, 207),
    21: Color8(127, 183, 161),
    22: Color8(209, 228, 236),
    23: Color8(192, 202, 214),
    24: Color8(111, 155, 75),
    25: Color8(150, 126, 92),
    26: Color8(122, 127, 136),
    27: Color8(74, 106, 85),
    28: Color8(182, 101, 68),
    29: Color8(140, 52, 45),
    30: Color8(64, 51, 61),
    31: Color8(122, 110, 104),
    32: Color8(76, 137, 145),
    33: Color8(91, 70, 57),
    34: Color8(46, 79, 92),
    35: Color8(79, 75, 51),
    36: Color8(47, 143, 178),
}

const TERRAIN_LABELS := {
    0: "Deep Ocean",
    1: "Continental Shelf",
    2: "Inland Sea",
    3: "Coral Shelf",
    4: "Hydrothermal Vent Field",
    5: "Tidal Flat",
    6: "River Delta",
    7: "Mangrove Swamp",
    8: "Freshwater Marsh",
    9: "Floodplain",
    10: "Alluvial Plain",
    11: "Prairie Steppe",
    12: "Mixed Woodland",
    13: "Boreal Taiga",
    14: "Peatland/Heath",
    15: "Hot Desert Erg",
    16: "Rocky Reg Desert",
    17: "Semi-Arid Scrub",
    18: "Salt Flat",
    19: "Oasis Basin",
    20: "Tundra",
    21: "Periglacial Steppe",
    22: "Glacier",
    23: "Seasonal Snowfield",
    24: "Rolling Hills",
    25: "High Plateau",
    26: "Alpine Mountain",
    27: "Karst Highland",
    28: "Canyon Badlands",
    29: "Active Volcano Slope",
    30: "Basaltic Lava Field",
    31: "Ash Plain",
    32: "Fumarole Basin",
    33: "Impact Crater Field",
    34: "Karst Cavern Mouth",
    35: "Sinkhole Field",
    36: "Aquifer Ceiling",
}

var grid_width: int = 0
var grid_height: int = 0
var overlay_channels: Dictionary = {}
var overlay_raw_channels: Dictionary = {}
var overlay_channel_labels: Dictionary = {}
var overlay_channel_descriptions: Dictionary = {}
var overlay_placeholder_flags: Dictionary = {}
var overlay_channel_order: PackedStringArray = PackedStringArray()
var active_overlay_key: String = "logistics"
var terrain_overlay: PackedInt32Array = PackedInt32Array()
var terrain_palette: Dictionary = {}
var terrain_tags_overlay: PackedInt32Array = PackedInt32Array()
var terrain_tag_labels: Dictionary = {}
var units: Array = []
var routes: Array = []
var tile_lookup: Dictionary = {}
var trade_links_overlay: Array = []
var trade_overlay_enabled: bool = false
var selected_trade_entity: int = -1
var crisis_annotations: Array = []

var terrain_mode: bool = true

var last_hex_radius: float = 48.0
var last_origin: Vector2 = Vector2.ZERO
var last_map_size: Vector2 = Vector2.ZERO
var last_base_origin: Vector2 = Vector2.ZERO
var base_hex_radius: float = 1.0
var zoom_factor: float = 1.0
var pan_offset: Vector2 = Vector2.ZERO
var base_bounds: Rect2 = Rect2(Vector2.ZERO, Vector2.ONE)
var bounds_dirty: bool = true
var mouse_pan_active: bool = false
var mouse_pan_button: int = -1

var faction_colors: Dictionary = {
    "Aurora": Color(0.55, 0.85, 1.0, 1.0),
    "Obsidian": Color(0.95, 0.62, 0.2, 1.0),
    "Verdant": Color(0.4, 0.9, 0.55, 1.0)
}

func _ready() -> void:
    set_process_unhandled_input(true)
    set_process(true)
    _ensure_input_actions()

func display_snapshot(snapshot: Dictionary) -> Dictionary:
    var previous_width: int = grid_width
    var previous_height: int = grid_height
    var grid: Dictionary = snapshot.get("grid", {})
    var new_width: int = int(grid.get("width", 0))
    var new_height: int = int(grid.get("height", 0))
    var dimensions_changed: bool = previous_width != new_width or previous_height != new_height
    grid_width = new_width
    grid_height = new_height

    var overlays: Dictionary = snapshot.get("overlays", {})
    _ingest_overlay_channels(overlays)
    terrain_overlay = PackedInt32Array(overlays.get("terrain", []))
    var palette_raw: Variant = overlays.get("terrain_palette", {})
    terrain_palette = palette_raw if typeof(palette_raw) == TYPE_DICTIONARY else {}
    terrain_tags_overlay = PackedInt32Array(overlays.get("terrain_tags", []))
    var tag_labels_raw: Variant = overlays.get("terrain_tag_labels", {})
    terrain_tag_labels = tag_labels_raw if typeof(tag_labels_raw) == TYPE_DICTIONARY else {}
    crisis_annotations = []
    var crisis_annotations_variant: Variant = overlays.get("crisis_annotations", [])
    if crisis_annotations_variant is Array:
        for entry in crisis_annotations_variant:
            if entry is Dictionary:
                crisis_annotations.append((entry as Dictionary).duplicate(true))
    units = Array(snapshot.get("units", []))
    routes = Array(snapshot.get("orders", []))

    tile_lookup.clear()
    var tile_entries_variant: Variant = snapshot.get("tiles", [])
    if tile_entries_variant is Array:
        for entry in tile_entries_variant:
            if entry is Dictionary:
                var tile_dict: Dictionary = entry
                var entity_id: int = int(tile_dict.get("entity", -1))
                if entity_id < 0:
                    continue
                var x: int = int(tile_dict.get("x", 0))
                var y: int = int(tile_dict.get("y", 0))
                tile_lookup[entity_id] = Vector2i(x, y)

    if snapshot.has("trade_links"):
        var trade_variant: Variant = snapshot.get("trade_links")
        if trade_variant is Array:
            update_trade_overlay(trade_variant, trade_overlay_enabled)

    if dimensions_changed:
        zoom_factor = 1.0
        pan_offset = Vector2.ZERO
        mouse_pan_active = false
        mouse_pan_button = -1
    bounds_dirty = dimensions_changed

    _update_layout_metrics()
    queue_redraw()
    _emit_overlay_legend()

    return {
        "unit_count": units.size(),
        "avg_logistics": _average_overlay("logistics"),
        "avg_sentiment": _average_overlay("sentiment"),
        "avg_corruption": _average_overlay("corruption"),
        "avg_fog": _average_overlay("fog"),
        "avg_culture": _average_overlay("culture"),
        "avg_military": _average_overlay("military"),
        "avg_crisis": _average_overlay("crisis"),
        "dimensions_changed": dimensions_changed,
        "active_overlay": active_overlay_key
    }

func _ingest_overlay_channels(overlays: Variant) -> void:
    overlay_channels.clear()
    overlay_raw_channels.clear()
    overlay_channel_labels.clear()
    overlay_channel_descriptions.clear()
    overlay_placeholder_flags.clear()
    overlay_channel_order = PackedStringArray()

    if not (overlays is Dictionary):
        if overlay_channels.is_empty():
            active_overlay_key = ""
        return

    var overlay_dict: Dictionary = overlays
    if overlay_dict.has("channels"):
        var channel_variant: Variant = overlay_dict["channels"]
        if channel_variant is Dictionary:
            var channel_dict: Dictionary = channel_variant
            for raw_key in channel_dict.keys():
                var key := String(raw_key)
                var info_variant: Variant = channel_dict[raw_key]
                if not (info_variant is Dictionary):
                    continue
                var channel_info: Dictionary = info_variant
                overlay_channels[key] = PackedFloat32Array(channel_info.get("normalized", PackedFloat32Array()))
                overlay_raw_channels[key] = PackedFloat32Array(channel_info.get("raw", PackedFloat32Array()))
                overlay_channel_labels[key] = String(channel_info.get("label", key.capitalize()))
                overlay_channel_descriptions[key] = String(channel_info.get("description", ""))
                overlay_placeholder_flags[key] = bool(channel_info.get("placeholder", false))

    var placeholder_variant: Variant = overlay_dict.get("placeholder_channels", PackedStringArray())
    if placeholder_variant is PackedStringArray:
        var placeholder_array: PackedStringArray = placeholder_variant
        for raw_placeholder_key in placeholder_array:
            var placeholder_key := String(raw_placeholder_key)
            overlay_placeholder_flags[placeholder_key] = true

    var order_variant: Variant = overlay_dict.get("channel_order", PackedStringArray())
    overlay_channel_order = PackedStringArray()
    if order_variant is PackedStringArray:
        var order_array: PackedStringArray = order_variant
        for raw_channel_key in order_array:
            overlay_channel_order.append(String(raw_channel_key))
    if overlay_channel_order.size() == 0:
        var keys: Array = overlay_channels.keys()
        keys.sort()
        for key in keys:
            overlay_channel_order.append(String(key))

    if overlay_channels.is_empty():
        active_overlay_key = ""
        return

    var default_variant: Variant = overlay_dict.get("default_channel", active_overlay_key)
    var default_key: String = String(default_variant)
    if overlay_channels.has(active_overlay_key):
        return
    if overlay_channels.has(default_key):
        active_overlay_key = default_key
        return
    if overlay_channel_order.size() > 0:
        active_overlay_key = String(overlay_channel_order[0])
    else:
        var keys_list: Array = overlay_channels.keys()
        if keys_list.size() > 0:
            active_overlay_key = String(keys_list[0])
        else:
            active_overlay_key = ""
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

    _draw_trade_overlay(radius, origin)
    _draw_crisis_annotations(radius, origin)

    for unit in units:
        _draw_unit(unit, radius, origin)

    for order in routes:
        _draw_route(order, radius, origin)

func update_trade_overlay(trade_links: Array, enabled: bool = trade_overlay_enabled) -> void:
    trade_links_overlay = []
    if trade_links is Array:
        for entry in trade_links:
            if entry is Dictionary:
                trade_links_overlay.append((entry as Dictionary).duplicate(true))
    trade_overlay_enabled = enabled
    queue_redraw()

func set_trade_overlay_enabled(enabled: bool) -> void:
    trade_overlay_enabled = enabled
    queue_redraw()

func set_trade_overlay_selection(entity_id: int) -> void:
    selected_trade_entity = entity_id
    if trade_overlay_enabled:
        queue_redraw()

func set_overlay_channel(key: String) -> void:
    if not overlay_channels.has(key):
        return
    if active_overlay_key == key:
        return
    active_overlay_key = key
    queue_redraw()
    _emit_overlay_legend()

func _draw_crisis_annotations(radius: float, origin: Vector2) -> void:
    if active_overlay_key != "crisis":
        return
    if crisis_annotations.is_empty():
        return
    for entry_variant in crisis_annotations:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var severity := String(entry.get("severity", "safe"))
        var color: Color = CRISIS_SEVERITY_COLORS.get(severity, CRISIS_COLOR)
        var stroke_color: Color = color
        stroke_color.a = max(color.a, 0.9)
        var fill_color: Color = color
        fill_color.a = min(color.a, 0.45)
        var coords: Array[Vector2] = []
        var path_variant: Variant = entry.get("path", PackedInt32Array())
        if path_variant is PackedInt32Array:
            var packed: PackedInt32Array = path_variant
            var length: int = packed.size()
            if length < 2:
                continue
            for idx in range(0, length, 2):
                if idx + 1 >= length:
                    break
                var col := int(packed[idx])
                var row := int(packed[idx + 1])
                coords.append(_hex_center(col, row, radius, origin))
        elif path_variant is Array:
            var arr: Array = path_variant
            if arr.is_empty():
                continue
            for step in arr:
                if step is Array and step.size() >= 2:
                    var col := int(step[0])
                    var row := int(step[1])
                    coords.append(_hex_center(col, row, radius, origin))
        if coords.is_empty():
            continue
        var stroke_width: float = clamp(radius * 0.18, 2.0, 8.0)
        if coords.size() == 1:
            var center: Vector2 = coords[0]
            var halo_color: Color = fill_color
            halo_color.a = max(fill_color.a, 0.35)
            draw_circle(center, radius * 0.55, halo_color)
            var core_color: Color = stroke_color
            core_color.a = max(stroke_color.a, 0.85)
            draw_circle(center, radius * 0.32, core_color)
        else:
            var polyline := PackedVector2Array()
            for point in coords:
                polyline.append(point)
            draw_polyline(polyline, stroke_color, stroke_width, true)
            var head: Vector2 = coords[coords.size() - 1]
            var tail: Vector2 = coords[0]
            var head_radius: float = clamp(radius * 0.28, 4.0, 12.0)
            var tail_radius: float = clamp(radius * 0.2, 3.0, 10.0)
            draw_circle(head, head_radius, stroke_color)
            var tail_color: Color = fill_color
            tail_color.a = max(fill_color.a, 0.55)
            draw_circle(tail, tail_radius, tail_color)
        var label: String = String(entry.get("label", ""))
        if label != "":
            var font: Font = ThemeDB.fallback_font
            if font != null:
                var anchor: Vector2 = coords[coords.size() - 1]
                var text_color: Color = Color(0.95, 0.96, 0.98, 0.95)
                var font_size: int = int(round(clamp(radius * 0.5, 14.0, 26.0)))
                draw_string(font, anchor + Vector2(radius * 0.3, -radius * 0.22), label, HORIZONTAL_ALIGNMENT_LEFT, -1.0, font_size, text_color)

func _draw_trade_overlay(radius: float, origin: Vector2) -> void:
    if not trade_overlay_enabled:
        return
    if trade_links_overlay.is_empty():
        return
    if tile_lookup.is_empty():
        return

    for entry in trade_links_overlay:
        if not (entry is Dictionary):
            continue
        var link: Dictionary = entry
        var from_tile: int = int(link.get("from_tile", -1))
        var to_tile: int = int(link.get("to_tile", -1))
        if not tile_lookup.has(from_tile) or not tile_lookup.has(to_tile):
            continue
        var from_pos: Vector2i = tile_lookup[from_tile]
        var to_pos: Vector2i = tile_lookup[to_tile]
        var start: Vector2 = _hex_center(from_pos.x, from_pos.y, radius, origin)
        var end: Vector2 = _hex_center(to_pos.x, to_pos.y, radius, origin)
        var knowledge_variant: Variant = link.get("knowledge", {})
        var openness: float = 0.0
        var leak_timer: int = 0
        if knowledge_variant is Dictionary:
            var knowledge_dict: Dictionary = knowledge_variant
            openness = float(knowledge_dict.get("openness", 0.0))
            leak_timer = int(knowledge_dict.get("leak_timer", 0))
        var throughput: float = float(link.get("throughput", 0.0))
        var intensity: float = clamp(abs(throughput) * 0.25, 0.0, 2.5)
        var opacity: float = clamp(0.25 + openness * 0.6, 0.3, 0.95)
        var base_color: Color = Color(0.95, 0.74, 0.22, opacity)
        var width: float = 2.0 + intensity
        var entity_id: int = int(link.get("entity", -1))
        if entity_id == selected_trade_entity:
            base_color = Color(0.3, 0.95, 0.7, 0.95)
            width += 2.0

        draw_line(start, end, base_color, width)

        if leak_timer <= 1:
            var midpoint: Vector2 = start.lerp(end, 0.5)
            draw_circle(midpoint, 4.5, Color(1.0, 0.35, 0.28, 0.85))

func _unhandled_input(event: InputEvent) -> void:
    if grid_width == 0 or grid_height == 0:
        return
    if event is InputEventMouseButton:
        var mouse_event: InputEventMouseButton = event
        if mouse_event.button_index == MOUSE_BUTTON_WHEEL_UP and mouse_event.pressed:
            _apply_zoom(MOUSE_ZOOM_STEP, get_local_mouse_position())
            _mark_input_handled()
            return
        elif mouse_event.button_index == MOUSE_BUTTON_WHEEL_DOWN and mouse_event.pressed:
            _apply_zoom(-MOUSE_ZOOM_STEP, get_local_mouse_position())
            _mark_input_handled()
            return
        elif (mouse_event.button_index == MOUSE_BUTTON_MIDDLE or mouse_event.button_index == MOUSE_BUTTON_RIGHT):
            if mouse_event.pressed:
                _begin_mouse_pan(mouse_event.button_index)
            else:
                _end_mouse_pan(mouse_event.button_index)
            _mark_input_handled()
            return
        elif mouse_event.button_index == MOUSE_BUTTON_LEFT and mouse_event.pressed:
            var local_position: Vector2 = get_local_mouse_position()
            _update_layout_metrics()
            var offset := _point_to_offset(local_position)
            var col: int = offset.x
            var row: int = offset.y
            if col < 0 or col >= grid_width or row < 0 or row >= grid_height:
                return
            var terrain_id: int = _terrain_id_at(col, row)
            emit_signal("hex_selected", col, row, terrain_id)
            _mark_input_handled()
            return
    elif event is InputEventMouseMotion:
        var motion: InputEventMouseMotion = event
        if mouse_pan_active:
            _apply_pan(motion.relative)
            _mark_input_handled()
    elif event is InputEventPanGesture:
        var gesture: InputEventPanGesture = event
        if gesture.alt_pressed:
            return
        _apply_pan(-gesture.delta)
        _mark_input_handled()
    elif event is InputEventMagnifyGesture:
        var magnify: InputEventMagnifyGesture = event
        var amount: float = (magnify.factor - 1.0) * KEYBOARD_ZOOM_SPEED
        if not is_zero_approx(amount):
            _apply_zoom(amount, get_local_mouse_position())
            _mark_input_handled()

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

func _overlay_array(key: String) -> PackedFloat32Array:
    var variant: Variant = overlay_channels.get(key, null)
    if variant is PackedFloat32Array:
        return variant
    return PackedFloat32Array()

func _overlay_raw_array(key: String) -> PackedFloat32Array:
    var variant: Variant = overlay_raw_channels.get(key, null)
    if variant is PackedFloat32Array:
        return variant
    return PackedFloat32Array()

func _average_overlay(key: String) -> float:
    return _average(_overlay_raw_array(key))

func _value_at_overlay(key: String, x: int, y: int) -> float:
    return _value_at(_overlay_array(key), x, y)

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
    if active_overlay_key == "":
        return GRID_COLOR
    var overlay_value: float = _value_at_overlay(active_overlay_key, x, y)
    var overlay_color: Color = OVERLAY_COLORS.get(active_overlay_key, LOGISTICS_COLOR)
    return GRID_COLOR.lerp(overlay_color, overlay_value)

func _terrain_color_for_id(terrain_id: int) -> Color:
    if TERRAIN_COLORS.has(terrain_id):
        return TERRAIN_COLORS[terrain_id]
    return Color(0.2, 0.2, 0.2, 1.0)

func terrain_palette_entries() -> Array:
    var ids: Array = []
    if terrain_palette.size() > 0:
        ids = Array(terrain_palette.keys())
    else:
        ids = Array(TERRAIN_COLORS.keys())
    ids.sort()
    var entries: Array = []
    for raw_id in ids:
        var id := int(raw_id)
        var label := ""
        if terrain_palette.has(id):
            label = str(terrain_palette[id])
        if label == "":
            label = TERRAIN_LABELS.get(id, "Unknown")
        var color := _terrain_color_for_id(id)
        entries.append({
            "id": id,
            "label": label,
            "color": color,
        })
    return entries

func _emit_overlay_legend() -> void:
    emit_signal("overlay_legend_changed", _legend_for_current_view())

func refresh_overlay_legend() -> void:
    _emit_overlay_legend()

func overlay_stats_for_key(key: String) -> Dictionary:
    if not overlay_channels.has(key):
        return {}
    var normalized: PackedFloat32Array = _overlay_array(key)
    var raw: PackedFloat32Array = _overlay_raw_array(key)
    return _overlay_stats(normalized, raw)

func _legend_for_current_view() -> Dictionary:
    if terrain_mode:
        return _build_terrain_legend()
    if active_overlay_key == "" or not overlay_channels.has(active_overlay_key):
        return {}
    return _build_scalar_overlay_legend(active_overlay_key)

func _build_terrain_legend() -> Dictionary:
    var rows: Array = []
    for entry in terrain_palette_entries():
        if typeof(entry) != TYPE_DICTIONARY:
            continue
        rows.append({
            "color": entry.get("color", Color.WHITE),
            "label": str(entry.get("label", "")),
            "value_text": "#%02d" % int(entry.get("id", 0)),
        })
    return {
        "key": "terrain",
        "title": "Terrain Types",
        "description": "Biome palette applied directly to tiles.",
        "rows": rows,
        "stats": {},
    }

func _build_scalar_overlay_legend(key: String) -> Dictionary:
    var normalized: PackedFloat32Array = _overlay_array(key)
    var raw: PackedFloat32Array = _overlay_raw_array(key)
    var stats: Dictionary = _overlay_stats(normalized, raw)
    var overlay_color: Color = OVERLAY_COLORS.get(key, LOGISTICS_COLOR)
    var label: String = String(overlay_channel_labels.get(key, key.capitalize()))
    var description: String = String(overlay_channel_descriptions.get(key, ""))
    var placeholder: bool = bool(overlay_placeholder_flags.get(key, false))
    var rows: Array = []
    var has_values: bool = bool(stats.get("has_values", false))
    var raw_range: float = float(stats.get("raw_range", 0.0))

    if placeholder and not has_values:
        rows.append({
            "color": GRID_COLOR,
            "label": "No data",
            "value_text": "Channel awaiting telemetry",
        })
    elif key == "crisis" and not has_values:
        rows.append({
            "color": GRID_COLOR,
            "label": "No active crises",
            "value_text": "Awaiting crisis incidents",
        })
    elif not has_values:
        rows.append({
            "color": GRID_COLOR.lerp(overlay_color, 0.2),
            "label": "No variation",
            "value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
        })
    elif raw_range <= 0.0001:
        var tint: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
        rows.append({
            "color": GRID_COLOR.lerp(overlay_color, tint),
            "label": "Uniform",
            "value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
        })
    else:
        var low_t: float = clamp(float(stats.get("normalized_min", 0.0)), 0.0, 1.0)
        var mid_t: float = clamp(float(stats.get("normalized_avg", 0.0)), 0.0, 1.0)
        var high_t: float = clamp(float(stats.get("normalized_max", 0.0)), 0.0, 1.0)
        rows.append({
            "color": GRID_COLOR.lerp(overlay_color, low_t),
            "label": "Low",
            "value_text": _format_legend_value(float(stats.get("raw_min", 0.0))),
        })
        rows.append({
            "color": GRID_COLOR.lerp(overlay_color, mid_t),
            "label": "Average",
            "value_text": _format_legend_value(float(stats.get("raw_avg", 0.0))),
        })
        rows.append({
            "color": GRID_COLOR.lerp(overlay_color, high_t),
            "label": "High",
            "value_text": _format_legend_value(float(stats.get("raw_max", 0.0))),
        })

    return {
        "key": key,
        "title": label,
        "description": description,
        "rows": rows,
        "stats": {
            "min": stats.get("raw_min", 0.0),
            "max": stats.get("raw_max", 0.0),
            "avg": stats.get("raw_avg", 0.0),
        },
        "placeholder": placeholder,
    }

func _overlay_stats(normalized: PackedFloat32Array, raw: PackedFloat32Array) -> Dictionary:
    var n_min: float = INF
    var n_max: float = -INF
    var n_sum: float = 0.0
    var n_count: int = 0
    for value in normalized:
        var v: float = float(value)
        if not is_finite(v):
            continue
        n_min = min(n_min, v)
        n_max = max(n_max, v)
        n_sum += v
        n_count += 1
    if n_count == 0:
        n_min = 0.0
        n_max = 0.0

    var r_min: float = INF
    var r_max: float = -INF
    var r_sum: float = 0.0
    var r_count: int = 0
    for value in raw:
        var rv: float = float(value)
        if not is_finite(rv):
            continue
        r_min = min(r_min, rv)
        r_max = max(r_max, rv)
        r_sum += rv
        r_count += 1
    if r_count == 0:
        r_min = 0.0
        r_max = 0.0

    var has_values: bool = n_count > 0 and r_count > 0
    var raw_avg: float = 0.0
    if r_count > 0:
        raw_avg = r_sum / float(r_count)
    var normalized_avg: float = 0.0
    if n_count > 0:
        normalized_avg = n_sum / float(n_count)

    return {
        "normalized_min": n_min,
        "normalized_max": n_max,
        "normalized_avg": normalized_avg,
        "raw_min": r_min,
        "raw_max": r_max,
        "raw_avg": raw_avg,
        "raw_range": r_max - r_min,
        "has_values": has_values,
    }

func _format_legend_value(value: float) -> String:
    return "%0.3f" % value

func set_terrain_mode(enabled: bool) -> void:
    terrain_mode = enabled
    queue_redraw()
    _emit_overlay_legend()

func toggle_terrain_mode() -> void:
    terrain_mode = not terrain_mode
    queue_redraw()
    _emit_overlay_legend()

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

func _axial_to_offset(q: int, r: int) -> Vector2i:
    var col: int = q + ((r - (r & 1)) >> 1)
    return Vector2i(col, r)

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
    if viewport_size.x <= 0.0 or viewport_size.y <= 0.0:
        return
    if bounds_dirty:
        base_bounds = _compute_bounds(1.0)
        bounds_dirty = false
    if base_bounds.size.x <= 0.0 or base_bounds.size.y <= 0.0:
        return
    var radius_from_width: float = viewport_size.x / base_bounds.size.x
    var radius_from_height: float = viewport_size.y / base_bounds.size.y
    base_hex_radius = min(radius_from_width, radius_from_height)
    last_hex_radius = clamp(base_hex_radius * zoom_factor, base_hex_radius * MIN_ZOOM_FACTOR, base_hex_radius * MAX_ZOOM_FACTOR)
    var scaled_bounds := Rect2(base_bounds.position * last_hex_radius, base_bounds.size * last_hex_radius)
    last_map_size = scaled_bounds.size
    last_base_origin = (viewport_size - last_map_size) * 0.5 - scaled_bounds.position
    last_origin = last_base_origin + pan_offset

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

func _point_to_offset(point: Vector2) -> Vector2i:
    if grid_width <= 0 or grid_height <= 0:
        return Vector2i(-1, -1)
    var radius: float = max(last_hex_radius, 0.0001)
    var relative: Vector2 = (point - last_origin) / radius
    var qf: float = (SQRT3 / 3.0) * relative.x - (1.0 / 3.0) * relative.y
    var rf: float = (2.0 / 3.0) * relative.y
    var axial: Vector2i = _cube_round(qf, rf)
    return _axial_to_offset(axial.x, axial.y)

func _cube_round(qf: float, rf: float) -> Vector2i:
    var sf: float = -qf - rf
    var rq: float = round(qf)
    var rr: float = round(rf)
    var rs: float = round(sf)

    var q_diff: float = abs(rq - qf)
    var r_diff: float = abs(rr - rf)
    var s_diff: float = abs(rs - sf)

    if q_diff > r_diff and q_diff > s_diff:
        rq = -rr - rs
    elif r_diff > s_diff:
        rr = -rq - rs
    else:
        rs = -rq - rr

    return Vector2i(int(rq), int(rr))

func _process(delta: float) -> void:
    if grid_width == 0 or grid_height == 0:
        return
    if mouse_pan_active and mouse_pan_button != -1 and not Input.is_mouse_button_pressed(mouse_pan_button):
        mouse_pan_active = false
        mouse_pan_button = -1
    var pan_input := Vector2(
        Input.get_action_strength("map_pan_right") - Input.get_action_strength("map_pan_left"),
        Input.get_action_strength("map_pan_down") - Input.get_action_strength("map_pan_up")
    )
    if pan_input != Vector2.ZERO:
        if pan_input.length_squared() > 1.0:
            pan_input = pan_input.normalized()
        _apply_pan(pan_input * KEYBOARD_PAN_SPEED * delta)
    var zoom_direction: float = Input.get_action_strength("map_zoom_in") - Input.get_action_strength("map_zoom_out")
    if not is_zero_approx(zoom_direction):
        var viewport_center: Vector2 = get_viewport_rect().size * 0.5
        _apply_zoom(zoom_direction * KEYBOARD_ZOOM_SPEED * delta, viewport_center)

func _apply_pan(delta: Vector2) -> void:
    if delta == Vector2.ZERO:
        return
    pan_offset += delta
    _update_layout_metrics()
    queue_redraw()

func _apply_zoom(delta_zoom: float, pivot: Vector2) -> void:
    if is_zero_approx(delta_zoom):
        return
    _update_layout_metrics()
    var previous_zoom: float = zoom_factor
    var previous_radius: float = max(last_hex_radius, 0.0001)
    var previous_origin: Vector2 = last_origin
    zoom_factor = clamp(zoom_factor + delta_zoom, MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
    if is_equal_approx(zoom_factor, previous_zoom):
        return
    var unit_position: Vector2 = (pivot - previous_origin) / previous_radius
    _update_layout_metrics()
    var new_radius: float = last_hex_radius
    var new_base_origin: Vector2 = last_base_origin
    pan_offset = pivot - new_base_origin - unit_position * new_radius
    _update_layout_metrics()
    queue_redraw()

func _begin_mouse_pan(button_index: int) -> void:
    mouse_pan_active = true
    mouse_pan_button = button_index

func _end_mouse_pan(button_index: int) -> void:
    if mouse_pan_active and mouse_pan_button == button_index:
        mouse_pan_active = false
        mouse_pan_button = -1

func _mark_input_handled() -> void:
    var viewport := get_viewport()
    if viewport != null:
        viewport.set_input_as_handled()

func _ensure_input_actions() -> void:
    var action_keys := {
        "map_pan_left": KEY_A,
        "map_pan_right": KEY_D,
        "map_pan_up": KEY_W,
        "map_pan_down": KEY_S,
        "map_zoom_in": KEY_E,
        "map_zoom_out": KEY_Q,
    }
    for action in action_keys.keys():
        if not InputMap.has_action(action):
            InputMap.add_action(action)
        var keycode: int = action_keys[action]
        var needs_event: bool = true
        for existing_event in InputMap.action_get_events(action):
            if existing_event is InputEventKey and existing_event.keycode == keycode:
                needs_event = false
                break
        if needs_event:
            var key_event := InputEventKey.new()
            key_event.keycode = keycode
            key_event.physical_keycode = keycode
            InputMap.action_add_event(action, key_event)
