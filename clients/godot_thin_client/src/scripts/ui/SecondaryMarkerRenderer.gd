class_name SecondaryMarkerRenderer
extends RefCounted

## Renders the SECONDARY map markers for MapView — herds, food sites, discovered
## (wondrous) sites, and the harvest/scout site overlays — plus the per-frame
## edge-slot assignment (`compute_slots`) and the `+N` overflow chip. Extracted
## from MapView (composition — MapView owns one and calls it during its _draw
## pass). Owns only the per-frame slot maps; every draw command + all shared
## geometry/glyph/pill/fog primitives and the marker source arrays stay on MapView
## and are reached through the `_view` back-ref. Behaviour (and the rendered
## pixels) are identical to the old inlined secondary-marker code.

var _view: MapView = null
# Per-frame edge-slot assignment, rebuilt each frame by compute_slots():
#   _secondary_slot_lookup: entry key -> slot index (or -1 = LOD/overflowed)
#   _secondary_overflow:    tile -> count of entries past SECONDARY_VISIBLE_CAP
var _secondary_slot_lookup: Dictionary = {}
var _secondary_overflow: Dictionary = {}

func _init(view: MapView) -> void:
	_view = view

## Assign each SECONDARY marker a fixed edge slot on its hex, once per frame. Priority
## order wonder → food → herd, sequential fill, so a tile's icons never jump between
## frames. Beyond _view.SECONDARY_VISIBLE_CAP the extras collapse into a `+N` overflow chip
## (drawn in the next slot). Visibility gating matches each category's own rule
## (_view.herds/food Active-only; wonders any explored tile). Skipped entirely at far zoom.
func compute_slots() -> void:
	_secondary_slot_lookup.clear()
	_secondary_overflow.clear()
	if _view.last_hex_radius < _view.ICON_MIN_DETAIL_RADIUS:
		return
	var per_tile: Dictionary = {}   # Vector2i -> Array[String] of entry keys, priority order
	for wsite in _view.discovered_sites:
		var wx := int((wsite as Dictionary).get("x", -1))
		var wy := int((wsite as Dictionary).get("y", -1))
		if wx < 0 or wy < 0:
			continue
		if _view._visibility_state_at(wx, wy) == "unexplored":
			continue
		if String((wsite as Dictionary).get("glyph", "")) == "":
			continue
		_append_secondary(per_tile, Vector2i(wx, wy), _wonder_key(wsite))
	for site in _view.food_sites:
		var fx := int((site as Dictionary).get("x", -1))
		var fy := int((site as Dictionary).get("y", -1))
		if fx < 0 or fy < 0 or not _view._is_tile_visible(fx, fy):
			continue
		_append_secondary(per_tile, Vector2i(fx, fy), _food_key(fx, fy))
	for herd in _view.herds:
		var hx := int((herd as Dictionary).get("x", -1))
		var hy := int((herd as Dictionary).get("y", -1))
		if hx < 0 or hy < 0 or not _view._is_tile_visible(hx, hy):
			continue
		_append_secondary(per_tile, Vector2i(hx, hy), _herd_key(String((herd as Dictionary).get("id", ""))))
	for tile in per_tile:
		var keys: Array = per_tile[tile]
		for i in range(keys.size()):
			_secondary_slot_lookup[keys[i]] = i if i < _view.SECONDARY_VISIBLE_CAP else -1
		if keys.size() > _view.SECONDARY_VISIBLE_CAP:
			_secondary_overflow[tile] = keys.size() - _view.SECONDARY_VISIBLE_CAP

func _append_secondary(per_tile: Dictionary, tile: Vector2i, key: String) -> void:
	var list: Array = per_tile.get(tile, [])
	list.append(key)
	per_tile[tile] = list

func _wonder_key(wsite: Dictionary) -> String:
	var fallback := "%d,%d" % [int(wsite.get("x", -1)), int(wsite.get("y", -1))]
	return "wonder:%s" % String(wsite.get("site_id", fallback))

func _food_key(x: int, y: int) -> String:
	return "food:%d,%d" % [x, y]

func _herd_key(herd_id: String) -> String:
	return "herd:%s" % herd_id

func _secondary_icon_size(radius: float) -> int:
	return int(maxf(_view.SECONDARY_ICON_MIN_SIZE, radius * _view.SECONDARY_ICON_SIZE_FACTOR))

func _secondary_slot_center(tile_center: Vector2, slot: int, radius: float) -> Vector2:
	return tile_center + _view.SECONDARY_SLOT_OFFSETS[slot] * radius

## The starving-pen distress badge: a filled DANGER disc with a dark rim and a HAND-DRAWN white "!",
## pinned to the upper-right of a marker glyph. Hand-drawn for the same reason `MagnifierButton` is —
## a font ⚠/❗ renders as an emoji blob at this size — and geometric so it reads OVER the full-color
## emoji it annotates. Sized off `icon_size`, so it shrinks with the marker at far zoom (and the
## caller is already LOD-gated by the secondary-slot system).
func _draw_distress_badge(icon_center: Vector2, icon_size: int) -> void:
	var badge_r: float = float(icon_size) * _view.HERD_DISTRESS_BADGE_RADIUS_FACTOR
	var center := icon_center + _view.HERD_DISTRESS_BADGE_OFFSET_FACTOR * float(icon_size)
	_view.draw_circle(center, badge_r, _view.HERD_DISTRESS_COLOR)
	_view.draw_arc(center, badge_r, 0, TAU, _view.HERD_DISTRESS_BADGE_SEGMENTS,
		_view.HERD_DISTRESS_BADGE_RIM_COLOR, _view.HERD_DISTRESS_BADGE_RIM_WIDTH)
	# The "!": a stem (a rect, so it stays crisp at small sizes) over a dot.
	var stem_w: float = badge_r * _view.HERD_DISTRESS_BANG_STEM_WIDTH
	var stem_top: float = badge_r * _view.HERD_DISTRESS_BANG_STEM_TOP
	var stem_bottom: float = badge_r * _view.HERD_DISTRESS_BANG_STEM_BOTTOM
	_view.draw_rect(Rect2(
		center + Vector2(-stem_w * 0.5, stem_top),
		Vector2(stem_w, stem_bottom - stem_top)), _view.HERD_DISTRESS_BANG_COLOR)
	_view.draw_circle(center + Vector2(0.0, badge_r * _view.HERD_DISTRESS_BANG_DOT_Y),
		badge_r * _view.HERD_DISTRESS_BANG_DOT_RADIUS, _view.HERD_DISTRESS_BANG_COLOR)

## Per-tile `+N` overflow chip pass (secondaries beyond _view.SECONDARY_VISIBLE_CAP).
func draw_secondary_overflow(radius: float, origin: Vector2) -> void:
	if _view.SECONDARY_VISIBLE_CAP >= _view.SECONDARY_SLOT_OFFSETS.size():
		return
	for tile in _secondary_overflow:
		var tile_center: Vector2 = _view._hex_center_wrapped(tile.x, tile.y, radius, origin)
		var chip_center := _secondary_slot_center(tile_center, _view.SECONDARY_VISIBLE_CAP, radius)
		_view._draw_count_pill(chip_center, "+%d" % int(_secondary_overflow[tile]))

func draw_herd(herd: Dictionary, radius: float, origin: Vector2) -> void:
	var herd_id := String(herd.get("id", ""))
	var x: int = int(herd.get("x", -1))
	var y: int = int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _view._is_tile_visible(x, y):
		return
	var slot: int = _secondary_slot_lookup.get(_herd_key(herd_id), -1)
	if slot < 0:
		return   # far-zoom LOD or overflowed into the +N chip
	# Herd trail stays centered on the hex path (a route, not a marker), but only
	# when the herd icon itself draws — no orphaned trail for an LOD-suppressed or
	# overflowed herd (its slot is gone).
	_view._draw_herd_trail(herd_id, radius, origin)
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	var herd_icon := FoodIcons.for_herd(String(herd.get("label", herd.get("id", "Herd"))))
	var icon_size := _secondary_icon_size(radius)
	# A starving pen's DANGER ring goes UNDER the glyph (it frames the animal); the badge goes OVER it
	# (it must never be occluded by a wide emoji). REJECTED: tinting the glyph — a herd marker is a
	# full-color emoji, so `modulate` just yields a slightly-darker brown animal (rendered, looked at,
	# reverted). The distress read has to be geometry the emoji cannot swallow.
	var starving := PenStatus.herd_is_starving(herd)
	if starving:
		_view.draw_arc(icon_center, radius * _view.HERD_DISTRESS_RING_FACTOR, 0, TAU, _view.HERD_DISTRESS_RING_SEGMENTS,
			_view.HERD_DISTRESS_COLOR, _view.HERD_DISTRESS_RING_WIDTH)
	_view._draw_marker_glyph(icon_center, herd_icon, icon_size, _view.SECONDARY_ICON_COLOR)
	if starving:
		_draw_distress_badge(icon_center, icon_size)

	# Migration arrow — thinner, and only on the hovered/selected herd tile to cut clutter.
	var tile := Vector2i(x, y)
	if tile == _view._hovered_tile or tile == _view.selected_tile:
		var next_x := int(herd.get("next_x", -1))
		var next_y := int(herd.get("next_y", -1))
		if next_x >= 0 and next_y >= 0:
			var next_center := _view._hex_center_wrapped(next_x, next_y, radius, origin)
			var line_too_long: bool = abs(tile_center.x - next_center.x) > _view.last_map_size.x * 0.4
			if not line_too_long:
				_view.draw_line(tile_center, next_center, _view.HERD_MIGRATION_ARROW_COLOR, _view.HERD_MIGRATION_ARROW_WIDTH)
				_view._draw_arrowhead(tile_center, next_center, _view.HERD_MIGRATION_ARROW_COLOR)

func draw_food_site(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _view._is_tile_visible(x, y):
		return
	var slot: int = _secondary_slot_lookup.get(_food_key(x, y), -1)
	if slot < 0:
		return
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	var module_key := String(site.get("module", ""))
	var kind := String(site.get("kind", ""))
	var is_hunt := kind == "game_trail"
	var icon := FoodIcons.for_site(module_key, is_hunt)
	if _view._food_harvest_active(x, y):
		_view.draw_arc(icon_center, radius * _view.FOOD_HARVEST_RING_FACTOR, 0, TAU, 20, Color(HudStyle.SIGNAL, 0.9), _view.FOOD_HARVEST_RING_WIDTH)
	_view._draw_marker_glyph(icon_center, icon, _secondary_icon_size(radius), _view.SECONDARY_ICON_COLOR)

func draw_discovered_site(site: Dictionary, radius: float, origin: Vector2) -> void:
	var x: int = int(site.get("x", -1))
	var y: int = int(site.get("y", -1))
	if x < 0 or y < 0:
		return
	# A discovered site is permanent geographic knowledge, not current-state info — unlike a
	# herd (moves) or food site (Active-only). Persist its marker on any known/remembered tile
	# (Discovered or Active), not only Active, so it stays visible once found even under fog.
	if _view._visibility_state_at(x, y) == "unexplored":
		return
	var slot: int = _secondary_slot_lookup.get(_wonder_key(site), -1)
	if slot < 0:
		return
	var glyph := String(site.get("glyph", ""))
	if glyph == "":
		return
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := _secondary_slot_center(tile_center, slot, radius)
	_view._draw_marker_glyph(icon_center, glyph, _secondary_icon_size(radius), _view.SECONDARY_ICON_COLOR)

func draw_harvest_markers(radius: float, origin: Vector2) -> void:
	if _view.harvest_sites.is_empty():
		return
	for key in _view.harvest_sites.keys():
		var entries_variant: Variant = _view.harvest_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _view._hex_center_wrapped(key.x, key.y, radius, origin)
		var module_key := String((entries[0] as Dictionary).get("module", ""))
		var style: Dictionary = _view.FOOD_SITE_STYLE_DEFAULT
		var base_site: Variant = _view.food_site_lookup.get(key, null)
		if base_site is Dictionary:
			var kind := String((base_site as Dictionary).get("kind", ""))
			style = _view.FOOD_SITE_STYLES.get(kind, _view.FOOD_SITE_STYLE_DEFAULT)
		var color: Color = style.get("color", _view.FOOD_SITE_STYLE_DEFAULT["color"])
		var glow_color := color
		glow_color.a = 0.25
		_view.draw_circle(center, radius * 0.65, glow_color)
		var stroke_color := color
		stroke_color.a = 0.95
		_view.draw_arc(center, radius * 0.55, 0, TAU, 32, stroke_color, 3.0)
		if entries.size() > 1:
			var label := "x%d" % entries.size()
			_view._draw_label(center + Vector2(-radius * 0.25, radius * 0.05), label, radius * 0.6, int(radius * 0.4), Color(0, 0, 0, 0.85))
		if not (base_site is Dictionary) and _view._selected_tile_matches_food(key.x, key.y, module_key):
			var highlight_color := Color(1.0, 1.0, 1.0, 0.9)
			_view.draw_arc(center, radius * 0.45, 0, TAU, 32, highlight_color, 2.5)

func draw_scout_markers(radius: float, origin: Vector2) -> void:
	if _view.scout_sites.is_empty():
		return
	for key in _view.scout_sites.keys():
		var entries_variant: Variant = _view.scout_sites.get(key, null)
		if not (entries_variant is Array):
			continue
		var entries: Array = entries_variant
		if entries.is_empty():
			continue
		var center := _view._hex_center_wrapped(key.x, key.y, radius, origin)
		var base_color := Color(0.8, 0.92, 1.0, 0.4)
		_view.draw_circle(center, radius * 0.4, base_color)
		var stroke_color := Color(0.9, 0.97, 1.0, 0.95)
		_view.draw_arc(center, radius * 0.5, 0, TAU, 24, stroke_color, 2.0)
