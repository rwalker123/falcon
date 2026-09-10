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
## The WORKED workings this frame, pushed in by `MapView._draw` — see `set_worked_workings`.
##   key (`working_key`) -> {"tile": Vector2i, "material": String, "crew": int}
var _worked_workings: Dictionary = {}
## Per-tile roll-up of the worked sources the cap hid — see `set_hidden_source_state`.
var _hidden_source_state: Dictionary = {}
# Marks the `+N` chip appends for what it hides, severity-ordered. They deliberately reuse the
# vocabulary the badges and the work rows already use, so the chip needs no legend of its own.
const OVERFLOW_WARN_MARK := "⚠"
const OVERFLOW_READY_MARK := "⌃"
const OVERFLOW_WORKED_MARK := "⚒"

func _init(view: MapView) -> void:
	_view = view

## Assign each SECONDARY marker a fixed edge slot on its hex, once per frame. Priority
## order wonder → food → herd → **working**, sequential fill, so a tile's icons never jump between
## frames. Beyond _view.SECONDARY_VISIBLE_CAP the extras collapse into a `+N` overflow chip
## (drawn in the next slot). Visibility gating matches each category's own rule
## (_view.herds/food Active-only; wonders and workings any explored tile). Skipped entirely at far
## zoom.
##
## **WORKINGS ARE APPENDED LAST, AND THE EXISTING THREE DO NOT MOVE** (issue #650). Sequential fill
## is what stops icons jumping frame-to-frame, so a new category earns the END of the order rather
## than a place in it — and on the rare hex already carrying three secondaries the working falls
## honestly into the `+N` chip, which reports it as `⚒`.
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
		if not _wonder_renders(wsite as Dictionary):
			continue
		_append_secondary(per_tile, Vector2i(wx, wy), _wonder_key(wsite))
	for site in _view.food_sites:
		var fx := int((site as Dictionary).get("x", -1))
		var fy := int((site as Dictionary).get("y", -1))
		if fx < 0 or fy < 0 or not _view._is_tile_visible(fx, fy):
			continue
		_append_secondary(per_tile, Vector2i(fx, fy), food_key(fx, fy))
	for herd in _view.herds:
		var hx := int((herd as Dictionary).get("x", -1))
		var hy := int((herd as Dictionary).get("y", -1))
		if hx < 0 or hy < 0 or not _view._is_tile_visible(hx, hy):
			continue
		_append_secondary(per_tile, Vector2i(hx, hy), herd_key(String((herd as Dictionary).get("id", ""))))
	# THE WORKINGS, LAST. Grouped and then SORTED within each hex rather than taken in the order the
	# crew walk produced them: that order follows the snapshot's BAND array, so a hex whose wood and
	# stone are cut by two different bands would swap its two markers the moment those bands
	# reordered. A sort on the key (which carries the material) is the same answer every frame.
	var workings_per_tile: Dictionary = {}   # Vector2i -> Array[String] of working keys
	for key in _worked_workings:
		var entry: Dictionary = _worked_workings[key]
		var wtile: Vector2i = entry.get("tile", Vector2i(-1, -1))
		if wtile.x < 0 or wtile.y < 0:
			continue
		if not _working_renders(entry):
			continue
		var wlist: Array = workings_per_tile.get(wtile, [])
		wlist.append(key)
		workings_per_tile[wtile] = wlist
	for wtile in workings_per_tile:
		var wkeys: Array = workings_per_tile[wtile]
		wkeys.sort()
		for wkey in wkeys:
			_append_secondary(per_tile, wtile, wkey)
	for tile in per_tile:
		var keys: Array = per_tile[tile]
		for i in range(keys.size()):
			_secondary_slot_lookup[keys[i]] = i if i < _view.SECONDARY_VISIBLE_CAP else -1
		if keys.size() > _view.SECONDARY_VISIBLE_CAP:
			_secondary_overflow[tile] = keys.size() - _view.SECONDARY_VISIBLE_CAP

## Which edge slot a source's marker drew in this frame: `0..SECONDARY_VISIBLE_CAP-1`, or **-1** for
## "not drawn" — either overflowed past the cap or LOD-suppressed (`compute_slots` returns early below
## `ICON_MIN_DETAIL_RADIUS`, leaving the lookup empty, so every key answers -1 at far zoom).
##
## PUBLIC because the worked-source marks (`BandOverlayRenderer.draw_worked_source_marks`) dock a ring
## to the SOURCE's own marker rather than to its hex — a hex holds a patch and several herds at once,
## so a tile-level mark cannot say which of them is worked. A -1 answer is the caller's cue to fall
## back to the tile-level outline.
func slot_of(key: String) -> int:
	return int(_secondary_slot_lookup.get(key, -1))

## How many sources on this tile were pushed past the visible cap (0 when none were). PUBLIC for the
## same reason as `slot_of`: what the `+N` chip hides still has state worth reporting.
func overflow_at(tile: Vector2i) -> int:
	return int(_secondary_overflow.get(tile, 0))

func _append_secondary(per_tile: Dictionary, tile: Vector2i, key: String) -> void:
	var list: Array = per_tile.get(tile, [])
	list.append(key)
	per_tile[tile] = list

## Whether a discovered site will render ANYTHING: bundled PNG art for its `site_id`, or the
## server's emoji glyph. `compute_slots` (slot eligibility) and `draw_discovered_site` (the draw
## guard) MUST agree on this — a site denied a slot can never draw, whatever art it has — so both
## ask HERE rather than repeating the condition and drifting. Testing glyph alone (as slot
## eligibility once did) silently dropped sprite-only sites: they were skipped before the draw
## path's sprite check could ever run. The `WonderSprites` lookup is a cached dictionary hit
## (`IconSprites`), not a load, so calling it from the per-frame slot pass is cheap.
static func _wonder_renders(wsite: Dictionary) -> bool:
	if WonderSprites.for_site_id(String(wsite.get("site_id", ""))) != null:
		return true
	return String(wsite.get("glyph", "")) != ""

func _wonder_key(wsite: Dictionary) -> String:
	var fallback := "%d,%d" % [int(wsite.get("x", -1)), int(wsite.get("y", -1))]
	return "wonder:%s" % String(wsite.get("site_id", fallback))

func food_key(x: int, y: int) -> String:
	return "food:%d,%d" % [x, y]

func herd_key(herd_id: String) -> String:
	return "herd:%s" % herd_id

## …and the WORKING's, whose identity is the `(tile, material)` PAIR (issue #650) — a hex cutting
## timber AND quarrying rock holds two workings, and a tile-only key would collapse them into one
## marker. The same pair `HudBandLaborState.extract_assignment_of` and the tile card's rows key on.
func working_key(x: int, y: int, material: String) -> String:
	return "working:%d,%d:%s" % [x, y, material]

## ⛔ **THE MARKER EXISTS ONLY WHERE A CREW IS ON THE WORKING** (issue #650, Ray's decision). Nearly
## every land tile carries stone once the scrub-wood rows are gone, so marking UNWORKED deposits
## would bury the map under a mark that means nothing is happening. `MapView._draw` therefore pushes
## the worked set — and only it — in here, ahead of `compute_slots`, and a hex nobody is cutting gets
## no working marker at all.
##
## **THE ORDER IS INVERTED FROM THE OTHER THREE CATEGORIES, and that is why this is a pushed input
## rather than a source array read here.** A herd's marker exists because the herd exists; a
## working's exists because somebody is WORKING it, which is a fact about the bands' labor rows —
## the answer `BandOverlayRenderer` derives for the worked-source marks. So the mark pass's input has
## to be computed BEFORE the slot pass rather than after it, and it is threaded across rather than
## held, exactly like `set_hidden_source_state` in the other direction; neither renderer holds the
## other.
func set_worked_workings(entries: Dictionary) -> void:
	_worked_workings = entries if entries is Dictionary else {}

## Whether a worked working will render ANYTHING on this hex — a MATERIAL MARK for its material, on
## ground that is not unexplored. `compute_slots` (slot eligibility) and `draw_workings` (the draw
## guard) MUST agree on this, so both ask HERE: a working denied a slot can never draw, and one given
## a slot it then declines to draw leaves a hole in the ring and pushes a real marker into the chip.
## That is `_wonder_renders`' rule, and the sprite-only-site bug it was written for.
##
## **THE FOG GATE IS THE WONDER'S, NOT THE HERD'S** — a working does not wander off, so any explored
## hex may carry one, and the sim publishes a working only to a faction that has DISCOVERED its tile
## (`MapView._workings_on_tile`). An unexplored hex is the one place a stale labor row could put a
## marker on ground the player has never seen.
func _working_renders(entry: Dictionary) -> bool:
	if FoodIcons.for_material(String(entry.get("material", ""))) == "":
		return false
	var tile: Vector2i = entry.get("tile", Vector2i(-1, -1))
	return _view._visibility_state_at(tile.x, tile.y) != "unexplored"

## Every WORKED working's marker: the material's own mark in the edge slot `compute_slots` gave it.
##
## **THE MARKER'S PRESENCE IS THE STATEMENT** — it is drawn only where a crew is on the working, so
## it needs no second "being worked" decoration on top. What the crew COUNT is rides the shared
## source badge under the marker (`BandOverlayRenderer._draw_source_badge`, `⚒N`), the same plate a
## worked patch or a hunted herd wears, so one hex cannot state a crew two ways.
func draw_workings(radius: float, origin: Vector2) -> void:
	for key in _worked_workings:
		var entry: Dictionary = _worked_workings[key]
		if not _working_renders(entry):
			continue
		var slot: int = _secondary_slot_lookup.get(key, -1)
		if slot < 0:
			continue   # far-zoom LOD or overflowed into the +N chip
		var tile: Vector2i = entry.get("tile", Vector2i(-1, -1))
		var tile_center: Vector2 = _view._hex_center_wrapped(tile.x, tile.y, radius, origin)
		_view._draw_marker_glyph(slot_center(tile_center, slot, radius),
			FoodIcons.for_material(String(entry.get("material", ""))),
			_secondary_icon_size(radius), _view.SECONDARY_ICON_COLOR)

func _secondary_icon_size(radius: float) -> int:
	return int(maxf(_view.SECONDARY_ICON_MIN_SIZE, radius * _view.SECONDARY_ICON_SIZE_FACTOR))

func slot_center(tile_center: Vector2, slot: int, radius: float) -> Vector2:
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
## THE CHIP CARRIES WHAT IT HIDES (docs/plan_worked_source_marks.md §2.3). Three slots is the right
## budget — six badges on a hex is not a map — but a cap that drops state silently reads as "nothing
## here", the very failure the worked-source marks exist to fix at a different scale. So the `+N` chip
## rolls up the hidden sources' state in SEVERITY ORDER and at most two marks wide: `⚠` if any hidden
## source is in trouble, `⌃` if any can climb a rung, `⚒` if any is merely worked.
##
## Reaching a hidden source is NOT this chip's job: re-clicking the hex cycles the whole occupant stack
## (issue #429). The chip's job is to say there is something in there worth the click.
func draw_secondary_overflow(radius: float, origin: Vector2) -> void:
	if _view.SECONDARY_VISIBLE_CAP >= _view.SECONDARY_SLOT_OFFSETS.size():
		return
	for tile in _secondary_overflow:
		var tile_center: Vector2 = _view._hex_center_wrapped(tile.x, tile.y, radius, origin)
		var chip_center := slot_center(tile_center, _view.SECONDARY_VISIBLE_CAP, radius)
		_view._draw_count_pill(chip_center, "+%d%s" % [int(_secondary_overflow[tile]), _hidden_marks(tile)])

## The rolled-up marks for one tile's hidden sources, "" when it hides nothing worked.
func _hidden_marks(tile: Vector2i) -> String:
	var state: Dictionary = _hidden_source_state.get(tile, {})
	if state.is_empty() or not bool(state.get("worked", false)):
		return ""
	var marks := ""
	if bool(state.get("warn", false)):
		marks += OVERFLOW_WARN_MARK
	if bool(state.get("ready", false)):
		marks += OVERFLOW_READY_MARK
	if marks == "":
		marks = OVERFLOW_WORKED_MARK
	return " " + marks

## Pushed each frame by `MapView._draw` from `BandOverlayRenderer.hidden_source_state()` — threaded
## across rather than held, so neither renderer depends on the other.
func set_hidden_source_state(state: Dictionary) -> void:
	_hidden_source_state = state if state is Dictionary else {}

func draw_herd(herd: Dictionary, radius: float, origin: Vector2) -> void:
	var herd_id := String(herd.get("id", ""))
	var x: int = int(herd.get("x", -1))
	var y: int = int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _view._is_tile_visible(x, y):
		return
	var slot: int = _secondary_slot_lookup.get(herd_key(herd_id), -1)
	if slot < 0:
		return   # far-zoom LOD or overflowed into the +N chip
	# Herd trail stays centered on the hex path (a route, not a marker), but only
	# when the herd icon itself draws — no orphaned trail for an LOD-suppressed or
	# overflowed herd (its slot is gone).
	_view._draw_herd_trail(herd_id, radius, origin)
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := slot_center(tile_center, slot, radius)
	var herd_label := String(herd.get("label", herd.get("id", "Herd")))
	# Bundled PNG art where we have it (identical on every OS), OS emoji for the species that
	# don't have art yet — FaunaSprites returns null for those and we fall through unchanged.
	var herd_sprite := FaunaSprites.for_herd(herd_label)
	var herd_icon := FoodIcons.for_herd(herd_label)
	var icon_size := _secondary_icon_size(radius)
	# A starving pen's DANGER ring goes UNDER the glyph (it frames the animal); the badge goes OVER it
	# (it must never be occluded by a wide emoji). REJECTED: tinting the glyph — a herd marker is a
	# full-color emoji, so `modulate` just yields a slightly-darker brown animal (rendered, looked at,
	# reverted). The distress read has to be geometry the emoji cannot swallow.
	var starving := PenStatus.herd_is_starving(herd)
	if starving:
		_view.draw_arc(icon_center, radius * _view.HERD_DISTRESS_RING_FACTOR, 0, TAU, _view.HERD_DISTRESS_RING_SEGMENTS,
			_view.HERD_DISTRESS_COLOR, _view.HERD_DISTRESS_RING_WIDTH)
	if herd_sprite != null:
		_view._draw_marker_sprite(icon_center, herd_sprite, icon_size)
	else:
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
	var slot: int = _secondary_slot_lookup.get(food_key(x, y), -1)
	if slot < 0:
		return
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := slot_center(tile_center, slot, radius)
	var module_key := String(site.get("module", ""))
	var kind := String(site.get("kind", ""))
	var is_hunt := kind == "game_trail"
	var terrain_id := int(site.get("terrain_id", -1))
	# Bundled PNG art where we have it (identical on every OS), OS emoji otherwise — SiteSprites
	# returns null for an unmapped art key and we fall through unchanged. Both resolve the same
	# module/hunt/terrain triple through `FoodIcons.site_key_for`, so they cannot disagree.
	var site_sprite := SiteSprites.for_site(module_key, is_hunt, terrain_id)
	var icon := FoodIcons.for_site(module_key, is_hunt, terrain_id)
	if _view._food_harvest_active(x, y):
		_view.draw_arc(icon_center, radius * _view.FOOD_HARVEST_RING_FACTOR, 0, TAU, 20, Color(HudStyle.SIGNAL, 0.9), _view.FOOD_HARVEST_RING_WIDTH)
	if site_sprite != null:
		_view._draw_marker_sprite(icon_center, site_sprite, _secondary_icon_size(radius))
	else:
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
	# Same renders-anything test compute_slots used to allot the slot (see _wonder_renders): a site
	# we have art for must still draw even if the server sent no glyph, so this cannot be a bare
	# empty-glyph guard. Bundled PNG art where we have it (identical on every OS), the server's
	# emoji otherwise — WonderSprites returns null for a site_id with no art and we fall through.
	if not _wonder_renders(site):
		return
	var site_sprite := WonderSprites.for_site_id(String(site.get("site_id", "")))
	var glyph := String(site.get("glyph", ""))
	var tile_center: Vector2 = _view._hex_center_wrapped(x, y, radius, origin)
	var icon_center := slot_center(tile_center, slot, radius)
	if site_sprite != null:
		_view._draw_marker_sprite(icon_center, site_sprite, _secondary_icon_size(radius))
	else:
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
