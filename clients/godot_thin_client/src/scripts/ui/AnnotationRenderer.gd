class_name AnnotationRenderer
extends RefCounted

## Renders MapView's ANNOTATION family — the overlays that draw *on top of* the map to say
## something about it rather than to draw the world itself: the Crisis overlay's per-tile/per-path
## annotations, the Terrain tab's "highlight every tile of this type" tool, the per-faction order
## ROUTES, and the command-TARGETING overlay (valid-target glow, reticle, hover ETA label).
## Extracted from MapView (composition — MapView owns one and calls its four entry points during its
## `_draw` pass).
##
## Owns this family's own state and nothing else: `_terrain_highlight_id`,
## `_crisis_annotations`, `_routes`, and the targeting dict + its animation clock. Every draw
## command plus the shared geometry/label primitives (`_hex_center` / `_hex_points` /
## `_hex_center_wrapped` / `_unwrapped_path_points` / `_draw_label` / `_draw_reticle` /
## `_hex_distance` / `_wrapped_col_delta` / `_is_player_unit` / `_get_adjusted_viewport_size`) and
## the world state it reads (units, herds, terrain, `tile_lookup`, `faction_colors`,
## `active_overlay_key`, the hovered tile) stay on MapView and are reached through the `_view`
## back-ref.
##
## TWO PUBLIC SEAMS STAY ON MAPVIEW as thin same-named pass-throughs, because both are reached
## REFLECTIVELY — a rename would not error, it would silently do nothing:
##   * `set_targeting`            — Main.gd connects the HUD's `targeting_changed` signal by name
##                                  (`has_method` + `Callable(map_view, "set_targeting")`)
##   * `set_terrain_highlight`    — TerrainPanel.gd pushes it via `has_method` + `call`
## The pass-throughs store here and MapView owns the `queue_redraw` (the `set_labor_pending` idiom);
## `set_terrain_highlight` returns a bool so its CONDITIONAL redraw is preserved.
##
## **The trade-link seam is gone with the substrate that fed it** (`update_trade_overlay` /
## `set_trade_overlay_enabled` / `set_trade_overlay_selection`, and the `trade_links` snapshot
## section behind them): the sim no longer publishes a link network at all, so the overlay drew the
## empty set on every frame. Issue #232 rebuilds a route-network overlay against a network that
## exists — see `docs/plan_contact_and_logistics.md`.
##
## `_targeting_time` is advanced from MapView's `_process` via `advance_targeting_time`, gated on
## `is_targeting_active()` — the same gate the inlined code used, so an idle client still does no
## per-frame work. Under the preview harness `Engine.time_scale` is 0, which pins the pulse at phase
## 0 (its midpoint, not zero amplitude) and makes the targeting frames a stable pixel reference.

# --- TERRAIN HIGHLIGHT (Terrain tab: "highlight all tiles of this type") ------------------------
# Bright magenta outline + tint, deliberately unlike any biome colour: the tool doubles as a
# worldgen debugging aid, so it must read over every terrain and through Fog of War.
const TERRAIN_HIGHLIGHT_COLOR := Color(1.0, 0.25, 0.9, 1.0)
const TERRAIN_HIGHLIGHT_FILL_ALPHA := 0.35
const TERRAIN_HIGHLIGHT_OUTLINE_WIDTH := 2.5
# Extra hexes scanned past the viewport edge, so a hex whose centre is off-screen but whose body
# is not still gets highlighted.
const TERRAIN_HIGHLIGHT_CULL_MARGIN := 2
# odd-r pointy-top row pitch as a multiple of the hex radius (columns are SQRT3 · radius apart).
const HEX_ROW_PITCH_FACTOR := 1.5

# A map coordinate is a [col, row] PAIR — the size every payload in this file is checked against
# (a crisis path's packed stride, a route waypoint, a unit's `pos`).
const COORD_PAIR_SIZE := 2

# --- CRISIS ANNOTATIONS (Crisis overlay) --------------------------------------------------------
# Only drawn while the `crisis` channel is the active overlay — the annotations annotate THAT view.
const CRISIS_OVERLAY_KEY := "crisis"
# Per-severity palette; anything not in it falls back to MapView's base crisis tint, read at the use
# site rather than aliased into a const here — `MapView.CRISIS_COLOR` is a themed `static var` now
# (see `ui/HudPalette.gd`), which is not a constant expression.
const CRISIS_SEVERITY_COLORS := {
	"critical": Color(0.96, 0.28, 0.38, 0.95),
	"warn": Color(0.97, 0.75, 0.28, 0.92),
	"safe": Color(0.5, 0.82, 0.72, 0.85)
}
const CRISIS_SEVERITY_DEFAULT := "safe"
# A path arrives either as a PackedInt32Array of FLATTENED col,row pairs (the wire form) or as an
# Array of [col, row] pairs; both are accepted (see draw_crisis_annotations), walked COORD_PAIR_SIZE
# at a time.
# The stroke carries the severity at near-full opacity; the fill is the same hue held translucent.
const CRISIS_STROKE_MIN_ALPHA := 0.9
const CRISIS_FILL_MAX_ALPHA := 0.45
const CRISIS_STROKE_WIDTH_FACTOR := 0.18   # × hex radius, then clamped
const CRISIS_STROKE_WIDTH_MIN := 2.0
const CRISIS_STROKE_WIDTH_MAX := 8.0
# A SINGLE-tile annotation is a place, not a movement: a translucent halo disc with a solid core.
const CRISIS_POINT_HALO_FACTOR := 0.55
const CRISIS_POINT_HALO_MIN_ALPHA := 0.35
const CRISIS_POINT_CORE_FACTOR := 0.32
const CRISIS_POINT_CORE_MIN_ALPHA := 0.85
# A MULTI-tile annotation is a movement: a polyline from tail (where it started, a soft disc) to
# head (where it is now, a solid disc).
const CRISIS_HEAD_FACTOR := 0.28
const CRISIS_HEAD_RADIUS_MIN := 4.0
const CRISIS_HEAD_RADIUS_MAX := 12.0
const CRISIS_TAIL_FACTOR := 0.2
const CRISIS_TAIL_RADIUS_MIN := 3.0
const CRISIS_TAIL_RADIUS_MAX := 10.0
const CRISIS_TAIL_MIN_ALPHA := 0.55
# The label sits just off the head, sized with the zoom so it stays readable without swamping the map.
const CRISIS_LABEL_FONT_FACTOR := 0.5
const CRISIS_LABEL_FONT_MIN := 14.0
const CRISIS_LABEL_FONT_MAX := 26.0
const CRISIS_LABEL_OFFSET_X_FACTOR := 0.3
const CRISIS_LABEL_OFFSET_Y_FACTOR := -0.22
const CRISIS_LABEL_UNWRAPPED := -1.0        # MapView._draw_label's "no max width" sentinel
const CRISIS_LABEL_COLOR := Color(0.95, 0.96, 0.98, 0.95)

# --- ROUTES (order paths) -----------------------------------------------------------------------
const ROUTE_WIDTH := 3.0
const ROUTE_MIN_POINTS := 2                # fewer than this is not a line
# Factions MapView has no colour for (and unowned orders) draw in neutral parchment amber.
const ROUTE_FALLBACK_COLOR := Color(0.95, 0.9, 0.6, 0.8)

# --- THE ROAD NETWORK (the roads in the GROUND, arc #532) ----------------------------------------
#
# ⛔ **NOT the ROUTES family above.** That one draws ORDER PATHS — waypoints a player's own
# movement orders are following, which vanish with the order. This draws roads: world objects in the
# ground, belonging to no faction, that outlive every band that walks them — kept by one band each,
# which is a JOB rather than ownership. The obvious name was taken by the
# other thing first, so this family is spelled `ROAD_*` and its state lives on
# `MapView.road_network` (world state, like `units` and `herds`) rather than here.
#
# ⛔ **A ROAD IS ONE HEX, SO THE DRAW IS A TILE AND NOT A POLYLINE.** Each tile carries its own rung,
# its own meter, its own keeper and its own decay, and the row has no path on it. A run of road
# therefore reads as a run of stamped hexes — which is the honest picture, because the two ends of a
# long road can genuinely be at different rungs and kept by different bands.

## The per-rung STROKE WIDTH ladder, faintest rung to strongest. Four steps because the branch has
## four rungs; the widths climb because a paved road is a bigger thing on the ground than a game
## trail, and the eye reads thickness before it reads tint at map zoom.
const ROAD_WIDTH_PATH := 1.5
const ROAD_WIDTH_TRAIL := 2.0
const ROAD_WIDTH_DIRT_ROAD := 3.0
const ROAD_WIDTH_PAVED_ROAD := 4.0

## ⛔ **THE RUNG LADDER IS ONE INK AT FOUR OPACITIES, NOT FOUR PALETTE ENTRIES — and that is a
## CORRECTION, made against a rendered frame.** The first cut walked the palette's own ink ladder
## (`LINE_SOFT` → `INK_FAINT` → `INK_DIM` → `INK`), which is faintest-to-strongest **on the HUD's dark
## ground and INVERTED on the map's**: rendered over tan steppe, the path drew as a near-black
## hairline with the most contrast on the frame while the paved road drew as pale grey. A road's rung
## has to read as prominence on ground of ANY tone, and only opacity does that.
##
## `HudStyle.INK` at these four, derived at draw time — the map's own idiom for a themed overlay
## tint (`MapView.SUPPLY_LINK_COLOR` is `SIGNAL` at its own opacity for the same reason). Read live
## rather than cached so a theme swap is picked up with no installation hook of this family's own.
const ROAD_OPACITY_PATH := 0.30
const ROAD_OPACITY_TRAIL := 0.52
const ROAD_OPACITY_DIRT_ROAD := 0.74
const ROAD_OPACITY_PAVED_ROAD := 0.94

## A road whose keeping is short draws in DANGER — the client's existing at-risk ink, not a colour of
## this family's own. It is losing a real investment, which is the same news a starving pen's ring
## carries, so it reads in the same language, and it draws at the TOP of the opacity ladder because
## an alarm that faded with the rung would be quietest on the trail nobody notices going.
const ROAD_AT_RISK_WIDTH := ROAD_WIDTH_DIRT_ROAD
const ROAD_AT_RISK_OPACITY := ROAD_OPACITY_PAVED_ROAD

## ⛔ **HOW BIG THE STAMPED HEX IS, as a fraction of the tile's own radius.** Inset rather than flush,
## for two reasons that both matter at map zoom: a flush ring would sit exactly on the hex grid's own
## edges and read as a GRID LINE rather than as a thing on the ground, and two adjacent road tiles
## would share that edge so a run of road would draw its interior seams twice as heavily as its
## outside. Inset, the run reads as a chain of stamps with the ground showing between them.
const ROAD_TILE_RADIUS_FACTOR := 0.62

# --- COMMAND TARGETING (HUD → map) --------------------------------------------------------------
# What the pending command is asking the player to click. `_draw_targeting` branches on it.
const TARGETING_NEED_BAND := "band"
const TARGETING_NEED_HERD := "herd"
const TARGETING_NEED_TILE := "tile"
# The pulse is the `base + amplitude · sin(t)` idiom, so phase 0 is its MIDPOINT, not zero
# amplitude — which is why freezing time in the preview harness still renders a visible overlay.
const TARGETING_PULSE_BASE := 0.5
const TARGETING_PULSE_AMPLITUDE := 0.5
const TARGETING_PULSE_SPEED := 3.2
const TARGETING_BAND_RING_FACTOR := 0.62   # × hex radius, before the pulse term
const TARGETING_HERD_RING_FACTOR := 0.55
const TARGETING_RING_PULSE_FACTOR := 0.1   # how much of the radius the pulse breathes
const TARGETING_RING_ALPHA_BASE := 0.5
const TARGETING_RING_ALPHA_PULSE := 0.35
const TARGETING_RING_SEGMENTS := 32
const TARGETING_RING_WIDTH := 2.5
const TARGETING_RETICLE_FACTOR := 0.82     # × hex radius
const TARGETING_NO_MIN_DISTANCE := 0       # absent `min_distance` admits every target
const TARGETING_UNKNOWN_DISTANCE := -1     # origin (or target) unknown
# Hover ETA label: a small dark plate pinned up-right of the hovered band, kept on screen.
const TARGETING_LABEL_FONT_SIZE := 13
const TARGETING_LABEL_PADDING := Vector2(8, 5)
const TARGETING_LABEL_OFFSET_FACTOR := 0.7   # × hex radius, right and up from the band centre
const TARGETING_LABEL_SCREEN_MARGIN := 4.0
const TARGETING_LABEL_BASELINE_FACTOR := 0.8
const TARGETING_LABEL_BG := Color(0.03, 0.055, 0.06, 0.95)
const TARGETING_LABEL_BORDER_WIDTH := 1.0
const TARGETING_LABEL_FG := Color(0.87, 0.98, 0.96)
const TARGETING_LABEL_FALLBACK_ID := "Band"
const TARGETING_LABEL_DISTANCE_SUFFIX := " · %d tiles"

var _view: MapView = null

# Terrain id highlighted by the Terrain tab's dropdown; -1 = off.
var _terrain_highlight_id: int = -1
# Crisis annotations and order routes, both re-ingested from every snapshot by MapView's
# display_snapshot (so both are cleared by a snapshot that carries none).
var _crisis_annotations: Array = []
var _routes: Array = []
# The HUD's pending command-targeting state, mirrored via `set_targeting`. Keys: active(bool),
# need("band"|"herd"|"tile"), command(String), origin_x/origin_y(int), min_distance(int).
var _targeting: Dictionary = {}
var _targeting_time: float = 0.0

func _init(view: MapView) -> void:
	_view = view

# =================================================================================================
# STATE PUSHES — every public setter here has a same-named pass-through on MapView (see the header)
# =================================================================================================

## Terrain-tab highlight (pass -1 to clear). Returns true when the id actually changed, so the
## MapView pass-through redraws only on a real change, exactly as the inlined setter did.
func set_terrain_highlight(terrain_id: int) -> bool:
	if _terrain_highlight_id == terrain_id:
		return false
	_terrain_highlight_id = terrain_id
	return true

## Snapshot ingest (MapView.display_snapshot): the Crisis overlay's annotation list, deep-copied so
## a later snapshot mutating its own payload cannot reach into the drawn set.
func set_crisis_annotations(annotations: Variant) -> void:
	_crisis_annotations = []
	if annotations is Array:
		for entry in annotations:
			if entry is Dictionary:
				_crisis_annotations.append((entry as Dictionary).duplicate(true))

## Snapshot ingest (MapView.display_snapshot): the per-faction order paths.
func set_routes(orders: Variant) -> void:
	_routes = Array(orders) if orders is Array else []

## Mirror the HUD's pending command-targeting state so the map can draw the reticle / valid-target
## glow / hover ETA. Pass {} to clear (which also resets the pulse clock).
func set_targeting(info: Dictionary) -> void:
	_targeting = info if info is Dictionary else {}
	if not bool(_targeting.get("active", false)):
		_targeting_time = 0.0

func is_targeting_active() -> bool:
	return bool(_targeting.get("active", false))

## WORLD BOUNDARY (`MapView.reset_world_state`): drop the annotations that describe a world we are
## about to stop showing. `_crisis_annotations` and `_routes` are refilled from every full snapshot,
## so clearing them here only covers the gap between worlds.
## `_targeting` is deliberately NOT cleared here: it MIRRORS the HUD's pending command, and
## `HudLayer.reset_world_state` cancels that through the normal path, which pushes `{}` down to us.
## Clearing it here as well would let the two desync (a banner with no reticle) if that ever changed.
## `_terrain_highlight_id` also stays — a terrain id is stable across worlds.
func reset_world_state() -> void:
	_crisis_annotations = []
	_routes = []

## Advance the targeting pulse. Driven from MapView's `_process` and gated there on
## `is_targeting_active()`, so nothing ticks while no command is being targeted.
func advance_targeting_time(delta: float) -> void:
	_targeting_time += delta

# =================================================================================================
# DRAW PASSES — called from MapView._draw, in this order
# =================================================================================================

## Overlay pass: outline + tint every visible tile matching the highlighted terrain id.
## Draws map-wide (ignores Fog of War) so it doubles as a worldgen debugging tool.
func draw_terrain_highlight(radius: float, origin: Vector2, viewport_size: Vector2) -> void:
	if _terrain_highlight_id < 0 or _view.terrain_overlay.is_empty() or _view.grid_width == 0:
		return
	var hex_col_width := _view.SQRT3 * radius
	var hex_row_height := HEX_ROW_PITCH_FACTOR * radius
	var col_start: int = int((-origin.x) / hex_col_width) - TERRAIN_HIGHLIGHT_CULL_MARGIN
	var col_end: int = int((viewport_size.x - origin.x) / hex_col_width) + TERRAIN_HIGHLIGHT_CULL_MARGIN
	var row_start: int = maxi(0, int((-origin.y) / hex_row_height) - TERRAIN_HIGHLIGHT_CULL_MARGIN)
	var row_end: int = mini(_view.grid_height,
		int((viewport_size.y - origin.y) / hex_row_height) + TERRAIN_HIGHLIGHT_CULL_MARGIN)
	var wraps: bool = _view._wrap_horizontal
	if not wraps:
		col_start = maxi(0, col_start)
		col_end = mini(_view.grid_width, col_end)
	var fill := TERRAIN_HIGHLIGHT_COLOR
	fill.a = TERRAIN_HIGHLIGHT_FILL_ALPHA
	var fill_colors := PackedColorArray([fill, fill, fill, fill, fill, fill])
	for y in range(row_start, row_end):
		for logical_x in range(col_start, col_end):
			var data_x: int = posmod(logical_x, _view.grid_width) if wraps else logical_x
			if not wraps and (logical_x < 0 or logical_x >= _view.grid_width):
				continue
			if _view._terrain_id_at(data_x, y) != _terrain_highlight_id:
				continue
			var center: Vector2 = _view._hex_center(logical_x, y, radius, origin)
			var pts := _view._hex_points(center, radius)
			_view.draw_polygon(pts, fill_colors)
			var outline := PackedVector2Array([pts[0], pts[1], pts[2], pts[3], pts[4], pts[5], pts[0]])
			_view.draw_polyline(outline, TERRAIN_HIGHLIGHT_COLOR, TERRAIN_HIGHLIGHT_OUTLINE_WIDTH, true)

## The Crisis overlay's annotations: a single tile draws as a halo+core disc ("here"), a path draws
## as a tail→head polyline ("moving this way"). Both forms of the wire `path` payload are accepted.
func draw_crisis_annotations(radius: float, origin: Vector2) -> void:
	if _view.active_overlay_key != CRISIS_OVERLAY_KEY:
		return
	if _crisis_annotations.is_empty():
		return
	for entry_variant in _crisis_annotations:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var severity := String(entry.get("severity", CRISIS_SEVERITY_DEFAULT))
		var color: Color = CRISIS_SEVERITY_COLORS.get(severity, MapView.CRISIS_COLOR)
		var stroke_color: Color = color
		stroke_color.a = max(color.a, CRISIS_STROKE_MIN_ALPHA)
		var fill_color: Color = color
		fill_color.a = min(color.a, CRISIS_FILL_MAX_ALPHA)
		var coords: Array[Vector2] = []
		var tiles: Array[Vector2i] = []
		var path_variant: Variant = entry.get("path", PackedInt32Array())
		if path_variant is PackedInt32Array:
			var packed: PackedInt32Array = path_variant
			var length: int = packed.size()
			if length < COORD_PAIR_SIZE:
				continue
			for idx in range(0, length, COORD_PAIR_SIZE):
				if idx + 1 >= length:
					break
				var col := int(packed[idx])
				var row := int(packed[idx + 1])
				tiles.append(Vector2i(col, row))
		elif path_variant is Array:
			var arr: Array = path_variant
			if arr.is_empty():
				continue
			for step in arr:
				if step is Array and step.size() >= COORD_PAIR_SIZE:
					var col := int(step[0])
					var row := int(step[1])
					tiles.append(Vector2i(col, row))
		# Both ingest forms carry DATA columns, so the COUNT picks the seam idiom. One coordinate is
		# a MARKER on a named tile — `_hex_center_wrapped` stamps it on the copy of that column the
		# viewport is actually over, instead of a map width off-frame (and the live sim publishes
		# exactly one, so this is the case every real annotation takes). Two or more are a connected
		# PATH, and `_unwrapped_path_points` lifts them into one column frame so a seam-crossing leg
		# does not draw back across the whole map — the same fix `_draw_route` carries below.
		if tiles.size() == 1:
			coords.append(_view._hex_center_wrapped(tiles[0].x, tiles[0].y, radius, origin))
		elif tiles.size() > 1:
			for point in _view._unwrapped_path_points(tiles, radius, origin):
				coords.append(point)
		if coords.is_empty():
			continue
		var stroke_width: float = clamp(radius * CRISIS_STROKE_WIDTH_FACTOR,
			CRISIS_STROKE_WIDTH_MIN, CRISIS_STROKE_WIDTH_MAX)
		if coords.size() == 1:
			var center: Vector2 = coords[0]
			var halo_color: Color = fill_color
			halo_color.a = max(fill_color.a, CRISIS_POINT_HALO_MIN_ALPHA)
			_view.draw_circle(center, radius * CRISIS_POINT_HALO_FACTOR, halo_color)
			var core_color: Color = stroke_color
			core_color.a = max(stroke_color.a, CRISIS_POINT_CORE_MIN_ALPHA)
			_view.draw_circle(center, radius * CRISIS_POINT_CORE_FACTOR, core_color)
		else:
			var polyline := PackedVector2Array()
			for point in coords:
				polyline.append(point)
			_view.draw_polyline(polyline, stroke_color, stroke_width, true)
			var head: Vector2 = coords[coords.size() - 1]
			var tail: Vector2 = coords[0]
			var head_radius: float = clamp(radius * CRISIS_HEAD_FACTOR,
				CRISIS_HEAD_RADIUS_MIN, CRISIS_HEAD_RADIUS_MAX)
			var tail_radius: float = clamp(radius * CRISIS_TAIL_FACTOR,
				CRISIS_TAIL_RADIUS_MIN, CRISIS_TAIL_RADIUS_MAX)
			_view.draw_circle(head, head_radius, stroke_color)
			var tail_color: Color = fill_color
			tail_color.a = max(fill_color.a, CRISIS_TAIL_MIN_ALPHA)
			_view.draw_circle(tail, tail_radius, tail_color)
		var label: String = String(entry.get("label", ""))
		if label != "":
			var anchor: Vector2 = coords[coords.size() - 1]
			var font_size: int = int(round(clamp(radius * CRISIS_LABEL_FONT_FACTOR,
				CRISIS_LABEL_FONT_MIN, CRISIS_LABEL_FONT_MAX)))
			_view._draw_label(
				anchor + Vector2(radius * CRISIS_LABEL_OFFSET_X_FACTOR, radius * CRISIS_LABEL_OFFSET_Y_FACTOR),
				label, CRISIS_LABEL_UNWRAPPED, font_size, CRISIS_LABEL_COLOR)

## Every order route in the current snapshot, as a per-faction polyline.
func draw_routes(radius: float, origin: Vector2) -> void:
	for order in _routes:
		_draw_route(order, radius, origin)

func _draw_route(order: Dictionary, radius: float, origin: Vector2) -> void:
	var path: Array = order.get("path", [])
	if path.is_empty():
		return
	var color: Color = _view.faction_colors.get(order.get("faction", ""), ROUTE_FALLBACK_COLOR)
	var tiles: Array = []
	for waypoint in path:
		if waypoint.size() != COORD_PAIR_SIZE:
			continue
		tiles.append(Vector2i(int(waypoint[0]), int(waypoint[1])))
	if tiles.size() < ROUTE_MIN_POINTS:
		return
	# A route's waypoints are DATA columns, so a seam-crossing leg drawn from the raw `_hex_center`
	# would shoot back across the whole map — the herd-trail bug, in the same shape. Unwrap the whole
	# path into one column frame first (`MapView._unwrapped_path_points`).
	var points := _view._unwrapped_path_points(tiles, radius, origin)
	for i in range(points.size() - 1):
		_view.draw_line(points[i], points[i + 1], color, ROUTE_WIDTH)

## **EVERY ROAD IN THE GROUND, one stamped HEX apiece.**
##
## Reads `MapView.road_network` through the back-ref rather than holding a copy: a road is world
## state, like `units` and `herds`, and this renderer already reads both that way. It is therefore
## cleared by `MapView.reset_world_state` with the rest of the world, not by this file's own reset.
func draw_road_network(radius: float, origin: Vector2) -> void:
	for road in _view.road_network:
		if road is Dictionary:
			_draw_road(road, radius, origin)

## One road — one hex. Styled by the RUNG IT HOLDS, never by the meter beside it, which belongs to the
## rung above (`HudRouteVocab`'s own rule: the rung string is the bool).
func _draw_road(road: Dictionary, radius: float, origin: Vector2) -> void:
	var tile: Vector2i = road.get(MapView.ROAD_TILE_KEY, MapView.ROAD_TILE_NONE)
	if tile.x < 0 or tile.y < 0:
		return
	# ⛔ **ONE ROAD, ONE VISIBILITY QUESTION.** The per-SEGMENT gate this replaced existed because a
	# stored path could run off the explored map with only one of its tiles seen; a road is a single
	# hex now, so the sim's own gate — *have you seen that tile* — is the whole of the test.
	if not _road_tile_known(tile):
		return
	var rung := HudRouteVocab.rung_of(road)
	var short := HudRouteVocab.is_short(road)
	# **AT RISK OUTRANKS THE RUNG LADDER**, and it takes the client's existing danger ink rather than
	# a colour of its own: a road washing out is losing an investment the player paid for, and the
	# one thing the map has to say about it is that it is being lost. The width holds at a mid rung's
	# so the alarm cannot also be a claim about which rung is standing.
	var color := Color(HudStyle.DANGER, ROAD_AT_RISK_OPACITY) if short \
		else Color(HudStyle.INK, _road_rung_opacity(rung))
	var width := ROAD_AT_RISK_WIDTH if short else _road_rung_width(rung)
	# A road names its tile by its DATA column, so the stamp goes on the copy of that column the
	# viewport is actually over — `_hex_center_wrapped`, the single-tile idiom every marker family
	# here uses. The polygon is then struck at the INSET radius off that true centre, which is why the
	# two calls take different radii: the placement is the tile's, the drawing is the stamp's.
	var center: Vector2 = _view._hex_center_wrapped(tile.x, tile.y, radius, origin)
	_view._outline_hex_at(center, radius * ROAD_TILE_RADIUS_FACTOR, color, width)

## Has the player's faction ever stood on this tile? `Discovered` OR `Active` — the same gate the sim
## publishes a road under, and deliberately NOT `_is_tile_visible`, which demands `Active`: a road
## does not wander off, so a remembered one is remembered truly. `""` is fog-off, which knows
## everything. **One call per ROAD now**, the per-segment walk having gone with the stored path.
func _road_tile_known(tile: Vector2i) -> bool:
	var state := _view._visibility_state_at(tile.x, tile.y)
	return state != HudConst.VISIBILITY_UNEXPLORED

## How solid one rung's road draws — see the opacity ladder above for why prominence rides opacity
## rather than four palette entries.
static func _road_rung_opacity(rung: String) -> float:
	match rung:
		HudRouteVocab.RUNG_KEY_PAVED_ROAD:
			return ROAD_OPACITY_PAVED_ROAD
		HudRouteVocab.RUNG_KEY_DIRT_ROAD:
			return ROAD_OPACITY_DIRT_ROAD
		HudRouteVocab.RUNG_KEY_TRAIL:
			return ROAD_OPACITY_TRAIL
	# The path AND any rung this client has never heard of: the faintest step there is. A road
	# is a real thing whatever it is called, so an unknown rung draws rather than vanishing.
	return ROAD_OPACITY_PATH

static func _road_rung_width(rung: String) -> float:
	match rung:
		HudRouteVocab.RUNG_KEY_PAVED_ROAD:
			return ROAD_WIDTH_PAVED_ROAD
		HudRouteVocab.RUNG_KEY_DIRT_ROAD:
			return ROAD_WIDTH_DIRT_ROAD
		HudRouteVocab.RUNG_KEY_TRAIL:
			return ROAD_WIDTH_TRAIL
	return ROAD_WIDTH_PATH

## The command-targeting overlay: which things on the map are valid targets for the command the HUD
## is currently asking the player to aim, plus a reticle on the hovered hex.
func draw_targeting(radius: float, origin: Vector2) -> void:
	if not is_targeting_active():
		return
	var need := String(_targeting.get("need", ""))
	var pulse: float = TARGETING_PULSE_BASE + TARGETING_PULSE_AMPLITUDE * sin(_targeting_time * TARGETING_PULSE_SPEED)
	var cyan := HudStyle.SIGNAL
	if need == TARGETING_NEED_BAND:
		# Only the player's own bands can fulfill a harvest/hunt, so only they get
		# the valid-target glow / ETA — not other factions' visible units.
		for unit in _view.units:
			if not _view._is_player_unit(unit):
				continue
			var pos: Array = Array(unit.get("pos", []))
			if pos.size() != COORD_PAIR_SIZE:
				continue
			var center: Vector2 = _view._hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)
			var ring_radius: float = radius * (TARGETING_BAND_RING_FACTOR + TARGETING_RING_PULSE_FACTOR * pulse)
			var ring_color := Color(cyan.r, cyan.g, cyan.b,
				TARGETING_RING_ALPHA_BASE + TARGETING_RING_ALPHA_PULSE * pulse)
			_view.draw_arc(center, ring_radius, 0, TAU, TARGETING_RING_SEGMENTS, ring_color, TARGETING_RING_WIDTH)
		if _view._hovered_tile.x >= 0 and _view._hovered_tile.y >= 0:
			for unit in _view.units:
				if not _view._is_player_unit(unit):
					continue
				var hpos: Array = Array(unit.get("pos", []))
				if hpos.size() == COORD_PAIR_SIZE \
						and int(hpos[0]) == _view._hovered_tile.x and int(hpos[1]) == _view._hovered_tile.y:
					_draw_targeting_hover_label(unit, radius, origin)
					break
	elif need == TARGETING_NEED_HERD:
		# Quarry targeting: glow the herds that are valid targets + reticle the hovered hex, so it
		# reads "click on a herd".
		# `min_distance` is `TargetingController.quarry_min_distance` for the MISSION being composed, and
		# this test is the RENDER-SIDE MIRROR of `TargetingController.is_expedition_quarry`. On a HUNT it
		# is the outfitting band's `hunt_reach` — a herd within reach is a LOCAL hunt, not a party's job,
		# and `TargetingController._try_pick_quarry` refuses it. On a DENIAL raid it is
		# `TargetingController.QUARRY_NO_REACH_BOUND` (`-1`), because breaking the herd next door is a
		# legal order; at `-1` the same strict comparison admits a herd on the band's OWN tile and still
		# skips the unknown distance. The halo must never promise a target the pick will refuse, nor hide
		# one it would accept, so the two tests must be changed together.
		# Absent (every other targeting mode omits the key) it defaults to 0 and admits everything.
		var min_distance := int(_targeting.get("min_distance", TARGETING_NO_MIN_DISTANCE))
		for herd in _view.herds:
			if not bool(herd.get("huntable", false)):
				continue
			var hx := int(herd.get("x", -1))
			var hy := int(herd.get("y", -1))
			# Fog-gated like the herd marker itself: glowing a herd you can't see would BE the leak
			# (it would draw a "valid target here" halo onto an empty-looking fogged hex).
			if hx < 0 or hy < 0 or not _view._is_tile_visible(hx, hy):
				continue
			# An UNKNOWN distance (`-1`, origin missing) skips too — `_is_expedition_quarry` also
			# refuses one, so the mirror holds at the degenerate end as well.
			if _targeting_distance(hx, hy) <= min_distance:
				continue
			var hcenter: Vector2 = _view._hex_center_wrapped(hx, hy, radius, origin)
			var hring_radius: float = radius * (TARGETING_HERD_RING_FACTOR + TARGETING_RING_PULSE_FACTOR * pulse)
			var hring_color := Color(cyan.r, cyan.g, cyan.b,
				TARGETING_RING_ALPHA_BASE + TARGETING_RING_ALPHA_PULSE * pulse)
			_view.draw_arc(hcenter, hring_radius, 0, TAU, TARGETING_RING_SEGMENTS, hring_color, TARGETING_RING_WIDTH)
		if _view._hovered_tile.x >= 0 and _view._hovered_tile.y >= 0:
			var herd_reticle: Vector2 = _view._hex_center_wrapped(
				_view._hovered_tile.x, _view._hovered_tile.y, radius, origin)
			_view._draw_reticle(herd_reticle, radius * TARGETING_RETICLE_FACTOR, cyan, pulse)
	elif need == TARGETING_NEED_TILE:
		if _view._hovered_tile.x >= 0 and _view._hovered_tile.y >= 0:
			var reticle_center: Vector2 = _view._hex_center_wrapped(
				_view._hovered_tile.x, _view._hovered_tile.y, radius, origin)
			_view._draw_reticle(reticle_center, radius * TARGETING_RETICLE_FACTOR, cyan, pulse)

func _draw_targeting_hover_label(unit: Dictionary, radius: float, origin: Vector2) -> void:
	var pos: Array = Array(unit.get("pos", []))
	if pos.size() != COORD_PAIR_SIZE:
		return
	var center: Vector2 = _view._hex_center_wrapped(int(pos[0]), int(pos[1]), radius, origin)
	var text: String = str(unit.get("id", TARGETING_LABEL_FALLBACK_ID))
	var dist := _targeting_distance(int(pos[0]), int(pos[1]))
	if dist >= 0:
		text += TARGETING_LABEL_DISTANCE_SUFFIX % dist
	var font: Font = ThemeDB.fallback_font
	if font == null:
		return
	var font_size := TARGETING_LABEL_FONT_SIZE
	var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var pad := TARGETING_LABEL_PADDING
	var box_pos: Vector2 = center + Vector2(radius * TARGETING_LABEL_OFFSET_FACTOR,
		-radius * TARGETING_LABEL_OFFSET_FACTOR - text_size.y - pad.y * 2)
	box_pos.x = clampf(box_pos.x, TARGETING_LABEL_SCREEN_MARGIN,
		_view._get_adjusted_viewport_size().x - text_size.x - pad.x * 2 - TARGETING_LABEL_SCREEN_MARGIN)
	box_pos.y = maxf(box_pos.y, TARGETING_LABEL_SCREEN_MARGIN)
	var rect := Rect2(box_pos, text_size + pad * 2)
	_view.draw_rect(rect, TARGETING_LABEL_BG)
	_view.draw_rect(rect, HudStyle.SIGNAL, false, TARGETING_LABEL_BORDER_WIDTH)
	_view.draw_string(font, box_pos + Vector2(pad.x, pad.y + text_size.y * TARGETING_LABEL_BASELINE_FACTOR),
		text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, TARGETING_LABEL_FG)

## Wrap-aware hex distance from the targeting ORIGIN to (col,row), the render-side mirror of
## Hud._hex_distance_wrapped (which TargetingController.is_expedition_quarry — the authoritative quarry pick —
## routes through). Bring the target into the origin's column frame via _wrapped_col_delta BEFORE
## the row-parity-sensitive offset→axial conversion (the same pre-wrap the work-range rings use), so
## a herd across the horizontal wrap seam measures the SHORT way round. Without this the herd-glow
## filter could halo a herd the pick refuses (or hide one it accepts) near the seam. Returns -1 when
## the origin (or the target) is unknown, matching the Hud helper.
func _targeting_distance(col: int, row: int) -> int:
	var ox := int(_targeting.get("origin_x", -1))
	var oy := int(_targeting.get("origin_y", -1))
	if ox < 0 or oy < 0 or col < 0 or row < 0:
		return TARGETING_UNKNOWN_DISTANCE
	var eff_col := ox + _view._wrapped_col_delta(ox, col)
	return _view._hex_distance(ox, oy, eff_col, row)
