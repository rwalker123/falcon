class_name BandMarkerRenderer
extends RefCounted

## Renders the PRIMARY player-band map markers for MapView: the offset card-stack
## of settlement-stage tokens / expedition flag-discs, the faction nameplate
## banner, the food-days dot, the travel/task arrow, and the ×N over-cap count
## pill. Extracted from MapView (composition — MapView owns one and calls
## draw_primary_bands() during its _draw pass). Every draw command routes through
## the MapView canvas item via the `_view` back-ref, and all shared geometry/glyph/
## pill primitives + the unit/selection state stay on MapView. Behaviour (and the
## rendered pixels) are identical to the old inlined band-marker code.

var _view: MapView = null
# StyleBoxFlat reused across banner draws; constant chrome set once, per-call
# fields (bg_color, corner radius) updated in _draw_band_banner.
var _band_banner_box: StyleBoxFlat = null

func _init(view: MapView) -> void:
	_view = view

## PRIMARY marker pass: draw player-band tokens as center card-stacks, one per
## occupied tile. Co-located bands fan up-right (back cards darkened/shrunk); the active band
## (selected, else first) is the opaque, full-brightness top card. There is NO per-token ring —
## the active band reads by brightness alone; selection is the hex-shape outline. Beyond
## _view.BAND_STACK_MAX_CARDS a `×N` count badge notes the hidden bands.
func draw_primary_bands(radius: float, origin: Vector2) -> void:
	# Group _view.units by tile, preserving snapshot order (deterministic stack order).
	var by_tile: Dictionary = {}   # Vector2i -> Array[Dictionary]
	var order: Array = []          # tiles in first-seen order
	for unit in _view.units:
		# Fog: a FOREIGN band on a hex you can't currently see isn't drawn (it used to render straight
		# through the fog). Your OWN bands always draw — see `_view._unit_hidden_by_fog`.
		if _view._unit_hidden_by_fog(unit):
			continue
		var pos: Array = Array(unit.get("pos", []))
		if pos.size() != 2:
			continue
		var tile := Vector2i(int(pos[0]), int(pos[1]))
		if not by_tile.has(tile):
			by_tile[tile] = []
			order.append(tile)
		by_tile[tile].append(unit)
	for tile in order:
		_draw_band_stack(by_tile[tile], radius, origin)

func _draw_band_stack(group: Array, radius: float, origin: Vector2) -> void:
	var count := group.size()
	if count == 0:
		return
	var first: Dictionary = group[0]
	var pos: Array = Array(first.get("pos", []))
	var group_tile := Vector2i(int(pos[0]), int(pos[1]))
	var center: Vector2 = _view._hex_center_wrapped(group_tile.x, group_tile.y, radius, origin)
	# Active band = the selected one on this tile (_view.selected_unit_id is the cycle target),
	# else the first. _view.selected_unit_id already tracks the cycled/roster-picked band.
	var active_idx := 0
	for i in range(count):
		if int((group[i] as Dictionary).get("entity", -1)) == _view.selected_unit_id:
			active_idx = i
			break
	var token_radius := radius * _view.BAND_TOKEN_RADIUS_FACTOR
	# Back cards = every non-active band (decorative depth), active drawn last on top.
	var back_bands: Array = []
	for i in range(count):
		if i != active_idx:
			back_bands.append(group[i])
	var back_to_draw := mini(count, _view.BAND_STACK_MAX_CARDS) - 1
	var back_radius := token_radius * _view.BAND_STACK_BEHIND_SCALE   # shrink back cards for depth
	for j in range(back_to_draw):
		var depth := back_to_draw - j   # furthest (largest offset) drawn first
		var offset := _view.BAND_STACK_CARD_STEP * radius * float(depth)
		_draw_band_token(back_bands[j], center + offset, back_radius, true)
	# Active top card at base position.
	var active: Dictionary = group[active_idx]
	_draw_band_token(active, center, token_radius, false)
	# Faction nameplate banner under the active (primary) card only. Far-zoom LOD-gated with the
	# same threshold that suppresses secondary icons/chips. Returns its rect so the count pill
	# can cap its right end.
	# Expeditions carry their faction on the flag-disc ring, not a settlement nameplate, so skip
	# the banner for them (and thus the banner-anchored count pill falls back to the offset).
	var active_is_expedition := bool(active.get("is_expedition", false))
	var show_banner := radius >= _view.ICON_MIN_DETAIL_RADIUS and not active_is_expedition
	var banner_rect := Rect2()
	if show_banner:
		banner_rect = _draw_band_banner(center, token_radius, _band_faction_color(active))
	# Active band reads by brightness alone now (full-color top card over darkened back cards);
	# the hex selection outline still marks the selected tile. No per-token ring.
	# Decorations on the active band only (expeditions show provisions in their drawer, not a dot).
	if _view._is_player_unit(active) and not active_is_expedition:
		_draw_band_status(active, center, token_radius)
	_draw_band_task_arrow(active, center, radius, origin)
	# Count badge for hidden bands beyond the visible cap (suppressed at far zoom). Folded onto
	# the right end of the banner (nameplate-with-count look); falls back to the old bottom-right
	# offset only if the banner is LOD-suppressed (which shares the same zoom gate, so in practice
	# it always caps the banner).
	if count > _view.BAND_STACK_MAX_CARDS and radius >= _view.ICON_MIN_DETAIL_RADIUS:
		var pill_center := center + _view.BAND_COUNT_BADGE_OFFSET * radius
		if show_banner:
			pill_center = Vector2(banner_rect.position.x + banner_rect.size.x, banner_rect.position.y + banner_rect.size.y * 0.5)
		_view._draw_count_pill(pill_center, "×%d" % count)

func _draw_band_token(unit: Dictionary, center: Vector2, token_radius: float, dim: bool) -> void:
	if bool(unit.get("is_expedition", false)):
		# A detached scouting party keeps its distinct hollow flag disc + awaiting-orders pulse
		# (not a settlement glyph). Faction reads off the ring, so no nameplate banner is drawn
		# (guarded in _draw_band_stack). Expeditions are lone on their tile, so `dim` is unused.
		_draw_expedition_body(unit, center, token_radius, _band_faction_color(unit))
		return
	var stage_icon := String(unit.get("settlement_stage_icon", ""))
	var glyph_size := int(maxf(_view.SECONDARY_ICON_MIN_SIZE, token_radius * _view.BAND_STAGE_GLYPH_SIZE_FACTOR))
	# Bundled stage sprite FIRST — the emoji path draws through `ThemeDB.fallback_font`, so the OS
	# emoji font would otherwise decide what a camp/village looks like (the same platform-inconsistency
	# the fauna/site sprites already fixed). Keyed on the server's stable `settlement_stage_id`.
	# This attempt MUST precede the empty-glyph placeholder below: that branch returns early, so a
	# sprite-mapped stage whose glyph happened to be empty would wrongly draw a square.
	var stage_sprite := StageSprites.for_stage(String(unit.get("settlement_stage_id", "")))
	if stage_sprite != null:
		# Undimmed = plain white modulate (unchanged art); a behind card recedes by the same
		# `BAND_STACK_BEHIND_TINT` the glyph path multiplies its colour by.
		var sprite_modulate := Color.WHITE
		if dim:
			sprite_modulate *= _view.BAND_STACK_BEHIND_TINT
		_view._draw_marker_sprite(center, stage_sprite, glyph_size, sprite_modulate)
		return
	if stage_icon == "":
		# Fallback: pre-stage / missing snapshot — a small neutral, NON-circular placeholder
		# square (never a faction disc). Ownership is still carried by the banner below.
		var marker_color := _view.BAND_FALLBACK_MARKER_COLOR
		var outline := _view.BAND_TOKEN_OUTLINE_COLOR
		if dim:
			marker_color *= _view.BAND_STACK_BEHIND_TINT
			outline *= _view.BAND_STACK_BEHIND_TINT
		var side := token_radius * _view.BAND_FALLBACK_MARKER_SIZE_FACTOR
		var square := Rect2(center.x - side * 0.5, center.y - side * 0.5, side, side)
		_view.draw_rect(square, marker_color)
		_view.draw_rect(square, outline, false, _view.BAND_TOKEN_OUTLINE_WIDTH)
		return
	# Stage glyph token: just the shadowed glyph — ownership is carried by the banner, not a ring.
	var glyph_color := _view.BAND_STAGE_GLYPH_COLOR
	if dim:
		glyph_color *= _view.BAND_STACK_BEHIND_TINT
	_view._draw_marker_glyph(center, stage_icon, glyph_size, glyph_color)

## Faction color lookup for a band token, with a neutral fallback for unknown factions.
func _band_faction_color(unit: Dictionary) -> Color:
	return _view.faction_colors.get(unit.get("faction", ""), _view.BAND_FACTION_FALLBACK_COLOR)

## Faction-colored nameplate banner drawn under the PRIMARY band token (caller draws it for the
## active top card only — never the dimmed back cards). Ownership reads off the fill color, so no
## ring/disc is needed. The bar is sized to later host an optional faction/band NAME LABEL drawn
## on top of it (this bar is the substrate); keep it wide/structured enough for that. Returns the
## bar Rect2 so the caller can anchor the `×N` count pill to its right end.
func _draw_band_banner(center: Vector2, token_radius: float, faction_color: Color) -> Rect2:
	var width := token_radius * _view.BAND_BANNER_WIDTH_FACTOR
	var height := token_radius * _view.BAND_BANNER_HEIGHT_FACTOR
	var top := center.y + token_radius + token_radius * _view.BAND_BANNER_GAP_FACTOR
	var rect := Rect2(center.x - width * 0.5, top, width, height)
	if _band_banner_box == null:
		# Constant chrome (border) set once; per-call fields updated below.
		_band_banner_box = StyleBoxFlat.new()
		_band_banner_box.border_color = _view.BAND_BANNER_OUTLINE_COLOR
		_band_banner_box.set_border_width_all(int(_view.BAND_BANNER_OUTLINE_WIDTH))
	_band_banner_box.bg_color = faction_color
	_band_banner_box.set_corner_radius_all(int(maxf(0.0, height * _view.BAND_BANNER_CORNER_RADIUS_FACTOR)))
	_view.draw_style_box(_band_banner_box, rect)
	return rect

## Travel/task destination arrow for a band, extracted so the stack draws it for the
## active card only. Skips the arrow when the band is already at its destination or the
## line would span the wrap seam.
func _draw_band_task_arrow(unit: Dictionary, center: Vector2, radius: float, origin: Vector2) -> void:
	var pos: Array = Array(unit.get("pos", []))
	if pos.size() != 2:
		return
	var dest_x: int = int(unit.get("dest_x", -1))
	var dest_y: int = int(unit.get("dest_y", -1))
	if dest_x < 0 or dest_y < 0:
		return
	if int(pos[0]) == dest_x and int(pos[1]) == dest_y:
		return
	var dest_center: Vector2 = _view._hex_center_wrapped(dest_x, dest_y, radius, origin)
	if abs(center.x - dest_center.x) > _view.last_map_size.x * 0.4:
		return
	var arrow_color: Color = _travel_arrow_color(String(unit.get("travel_task_kind", "")))
	_view.draw_line(center, dest_center, arrow_color, _view.BAND_TASK_ARROW_WIDTH)
	_view._draw_arrowhead(center, dest_center, arrow_color)

## Draw an expedition's map body (docs/plan_exploration_and_sites.md §2 / §2b): a hollow,
## faction-tinted disc — visually distinct from a resident band's solid dot — carrying a mission
## glyph (scout = ⚑ flag, hunt = 🏹 bow). Phase decorations: a scout `awaiting` party pulses an
## amber ring (needs a command); a hunt `delivering` party shows a green food pip (carrying a haul
## home). The shared label / travel arrow / selection ring stay in `_draw_unit`.

func _travel_arrow_color(task_kind: String) -> Color:
	match task_kind:
		"harvest":
			return Color(0.3, 0.8, 0.3, 0.85)  # Green
		"hunt":
			return Color(0.8, 0.3, 0.3, 0.85)  # Red
		"scout":
			return Color(0.3, 0.6, 0.9, 0.85)  # Blue
		_:
			return Color(0.7, 0.7, 0.7, 0.85)  # Gray

## Draw an expedition's map body (docs/plan_exploration_and_sites.md §2 / §2b): a hollow,
## faction-tinted disc — visually distinct from a resident band's solid dot — carrying a mission
## glyph (scout = ⚑ flag, hunt = 🏹 bow). Phase decorations: a scout `awaiting` party pulses an
## amber ring (needs a command); a hunt `delivering` party shows a green food pip (carrying a haul
## home). The shared label / travel arrow / selection ring stay in `_draw_unit`.
func _draw_expedition_body(unit: Dictionary, center: Vector2, marker_radius: float, color: Color) -> void:
	var is_hunt := String(unit.get("expedition_mission", "")) == _view.EXPEDITION_HUNT_MISSION
	var glyph := _view.EXPEDITION_HUNT_GLYPH if is_hunt else _view.EXPEDITION_GLYPH
	# Dark backing disc keeps the glyph legible over any terrain (mirrors the site/herd markers).
	_view.draw_circle(center, marker_radius, Color(0.04, 0.06, 0.07, _view.EXPEDITION_DISC_ALPHA))
	# Hollow faction ring — no solid fill, so it never reads as a resident band's dot.
	_view.draw_arc(center, marker_radius * _view.EXPEDITION_RING_FACTOR, 0, TAU, 24, color, _view.EXPEDITION_RING_WIDTH)
	# Mission glyph at the center.
	var font: Font = ThemeDB.fallback_font
	if font != null:
		var glyph_size: int = int(maxf(12.0, marker_radius * _view.EXPEDITION_GLYPH_SIZE_FACTOR * 2.0))
		var text_size: Vector2 = font.get_string_size(glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, glyph_size)
		var pos := Vector2(center.x - text_size.x * 0.5, center.y + glyph_size * 0.34)
		_view.draw_string(font, pos, glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, glyph_size, _view.EXPEDITION_GLYPH_COLOR)

	# Hunt phase decoration: hauling a haul home (delivering/returning) → a solid green food pip;
	# gathering at the herd (hunting) → a small red "working" cue ring. Mutually exclusive phases.
	if is_hunt:
		var hphase := String(unit.get("expedition_phase", ""))
		if hphase == _view.EXPEDITION_PHASE_DELIVERING or hphase == _view.EXPEDITION_PHASE_RETURNING:
			var pip_center := center + Vector2(marker_radius, marker_radius) * _view.EXPEDITION_DELIVER_PIP_OFFSET
			var pip_radius := marker_radius * _view.EXPEDITION_DELIVER_PIP_FACTOR
			_view.draw_circle(pip_center, pip_radius, HudStyle.HEALTHY)
			_view.draw_arc(pip_center, pip_radius, 0, TAU, 10, Color(0, 0, 0, 0.5), 1.0)
		elif hphase == _view.EXPEDITION_PHASE_HUNTING:
			var cue_center := center + Vector2(marker_radius, marker_radius) * _view.EXPEDITION_GATHER_CUE_OFFSET
			var cue_radius := marker_radius * _view.EXPEDITION_GATHER_CUE_FACTOR
			_view.draw_arc(cue_center, cue_radius, 0, TAU, 12, HudStyle.DANGER, _view.EXPEDITION_GATHER_CUE_WIDTH)

	# Awaiting-orders idle indicator (scout): a pulsing amber ring (needs a command).
	if String(unit.get("expedition_phase", "")) == _view.EXPEDITION_PHASE_AWAITING:
		var pulse: float = 0.5 + 0.5 * sin(_view._expedition_time * _view.EXPEDITION_AWAITING_PULSE_SPEED)
		var ring_radius: float = marker_radius * (_view.EXPEDITION_AWAITING_RING_FACTOR + _view.EXPEDITION_AWAITING_PULSE_AMPLITUDE * pulse)
		var ring_color := Color(HudStyle.WARN.r, HudStyle.WARN.g, HudStyle.WARN.b, 0.45 + 0.4 * pulse)
		_view.draw_arc(center, ring_radius, 0, TAU, 28, ring_color, _view.EXPEDITION_AWAITING_RING_WIDTH)

## One decoration on a player band marker: a food-days dot (green/amber/red by
## the shared BandFoodStatus thresholds) up-and-right of the marker.
func _draw_band_status(unit: Dictionary, center: Vector2, marker_radius: float) -> void:
	var days: float = float(unit.get("days_of_food", BandFoodStatus.UNLIMITED_DAYS))
	var dot_color := BandFoodStatus.color_for_days(days)
	var dot_radius: float = marker_radius * _view.BAND_FOOD_DOT_RADIUS_FACTOR
	var dot_center := center + Vector2(marker_radius, -marker_radius) * _view.BAND_FOOD_DOT_OFFSET_FACTOR
	_view.draw_circle(dot_center, dot_radius, dot_color)
	_view.draw_arc(dot_center, dot_radius, 0, TAU, 10, Color(0, 0, 0, 0.5), 1.0)
