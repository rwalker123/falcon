class_name BandMarkerRenderer
extends RefCounted

## Renders the PRIMARY player-band map markers for MapView: the offset card-stack
## of settlement-stage tokens / expedition flag-discs, the nameplate (the fixed-size
## band NAME PILL at high zoom, the scaled faction banner below it), the food-runway
## dot, the travel/task arrow, and the ×N over-cap count pill. Extracted from MapView (composition — MapView owns one and calls
## draw_primary_bands() during its _draw pass). Every draw command routes through
## the MapView canvas item via the `_view` back-ref, and all shared geometry/glyph/
## pill primitives + the unit/selection state stay on MapView. Behaviour (and the
## rendered pixels) are identical to the old inlined band-marker code.

var _view: MapView = null
# StyleBoxFlat reused across banner draws; constant chrome set once, per-call
# fields (bg_color, corner radius) updated in _draw_band_banner.
var _band_banner_box: StyleBoxFlat = null
# THE NAME-PILL OVERLAP CULL, both halves rebuilt from scratch every draw pass (see
# `_reserve_name_pills`): the rects already claimed this pass, and the tile → rect grants that
# came out of it. Nothing here survives a frame — a stale rect would cull a label that has
# nothing to collide with.
var _label_rects: Array[Rect2] = []
var _label_grants: Dictionary = {}   # Vector2i -> Rect2

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
	# Decide the name pills BEFORE anything is drawn. It has to be a separate pre-pass because the
	# two orders differ and both matter: labels are placed SELECTED-FIRST (a cull must never eat the
	# label of the band the player is looking at), while tokens keep snapshot order so no glyph
	# changes what it stacks over. Resolving placement up front leaves the draw order below byte-for-
	# byte what it was.
	_reserve_name_pills(by_tile, order, radius, origin)
	for tile in order:
		_draw_band_stack(by_tile[tile], radius, origin)

## Which tiles won a name pill in the last pass (see `MapView.band_label_tiles`).
func placed_label_tiles() -> Array:
	return _label_grants.keys()

## THE NAME-PILL RESERVATION PASS. Fixed-size text does not shrink with the map, so at the gate
## radius a 15-character pill spans roughly two hexes and neighbouring bands collide. A pill whose
## rect intersects one already placed is SKIPPED ENTIRELY — no pill, and no fall back to the scaled
## faction bar (two different nameplate shapes in one frame reads as two kinds of band). The token,
## its stack, its ⚠ and its food dot all still draw.
##
## SELECTED FIRST, THEN SNAPSHOT ORDER. The tile holding `_view.selected_unit_id` claims its rect
## before any other, so the band the player is working can never be the one culled. Everything after
## it keeps snapshot order for the reason the secondary slots fill sequentially: placement has to be
## the same answer frame to frame or labels flicker on and off as the array shuffles.
func _reserve_name_pills(by_tile: Dictionary, order: Array, radius: float, origin: Vector2) -> void:
	_label_rects.clear()
	_label_grants.clear()
	if radius < _view.BAND_NAME_PILL_MIN_RADIUS:
		return
	var token_radius := radius * _view.BAND_TOKEN_RADIUS_FACTOR
	for tile in _label_priority_order(by_tile, order):
		var group: Array = by_tile[tile]
		var active: Dictionary = group[_active_index(group)]
		# Expeditions carry their faction on the flag-disc ring and have never worn a nameplate;
		# the pill inherits that exactly.
		if bool(active.get("is_expedition", false)):
			continue
		var center: Vector2 = _view._hex_center_wrapped(tile.x, tile.y, radius, origin)
		var rects := _name_pill_rects(center, token_radius, String(active.get("id", "")), group.size())
		var footprint: Rect2 = rects[NAME_PILL_FOOTPRINT]
		if footprint.size == Vector2.ZERO:
			continue
		var blocked := false
		for placed in _label_rects:
			if placed.intersects(footprint):
				blocked = true
				break
		if blocked:
			continue
		# The cull reasons about the FOOTPRINT; the draw pass is handed the ANCHOR, which is the same
		# plate ending where the `×N` chip has to be centred. See `_name_pill_rects`.
		_label_rects.append(footprint)
		_label_grants[tile] = rects[NAME_PILL_ANCHOR]

## `order`, with the selected band's tile moved to the front — the ONE reordering the cull does, and
## it touches label placement only (the caller still draws tokens in `order`).
func _label_priority_order(by_tile: Dictionary, order: Array) -> Array:
	if _view.selected_unit_id < 0:
		return order
	for tile in order:
		for unit in by_tile[tile]:
			if int((unit as Dictionary).get("entity", -1)) == _view.selected_unit_id:
				var prioritized: Array = [tile]
				for other in order:
					if other != tile:
						prioritized.append(other)
				return prioritized
	return order

## Which card of a co-located group is the ACTIVE (top) one: the selected band if it is on this
## tile, else the first in snapshot order. Shared by the reservation pass and the draw pass so the
## pill can never name a different band than the token it sits under.
func _active_index(group: Array) -> int:
	for i in range(group.size()):
		if int((group[i] as Dictionary).get("entity", -1)) == _view.selected_unit_id:
			return i
	return 0

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
	var active_idx := _active_index(group)
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
	# Nameplate under the active (primary) card only, LOD-gated: suppressed below the same threshold
	# that hides secondary icons/chips. Returns its rect so the count pill can cap its right end.
	# Expeditions carry their faction on the flag-disc ring, not a settlement nameplate, so they get
	# neither form (and thus the nameplate-anchored count pill falls back to the offset).
	var active_is_expedition := bool(active.get("is_expedition", false))
	# ONE nameplate, two forms, chosen by zoom. Above `BAND_NAME_PILL_MIN_RADIUS` the fixed-size NAME
	# PILL replaces the scaled faction bar at the same anchor (and only if the cull granted this tile
	# a rect); between that and `ICON_MIN_DETAIL_RADIUS` the bar draws exactly as it always has; below
	# it, nothing. Either shape hands its Rect2 to the `×N` anchoring below unchanged.
	var granted: Variant = _label_grants.get(group_tile, null)
	var show_banner := radius >= _view.ICON_MIN_DETAIL_RADIUS \
		and radius < _view.BAND_NAME_PILL_MIN_RADIUS and not active_is_expedition
	var banner_rect := Rect2()
	var has_nameplate := false
	if show_banner:
		banner_rect = _draw_band_banner(center, token_radius, _band_faction_color(active))
		has_nameplate = true
	elif granted != null:
		banner_rect = _draw_band_name_pill(granted, String(active.get("id", "")),
			_band_faction_color(active))
		has_nameplate = true
	# Active band reads by brightness alone now (full-color top card over darkened back cards);
	# the hex selection outline still marks the selected tile. No per-token ring.
	# Decorations on the active band only (expeditions show provisions in their drawer, not a dot).
	if _view._is_player_unit(active) and not active_is_expedition:
		_draw_band_status(active, center, token_radius)
		# …and the ⚠ for ground that is killing them, in the one free corner of the token.
		_draw_band_lethal_mark(active, center, token_radius, radius)
	_draw_band_task_arrow(active, center, radius, origin)
	# Count badge for hidden bands beyond the visible cap (suppressed at far zoom). Folded onto
	# the right end of the banner (nameplate-with-count look); falls back to the old bottom-right
	# offset only if the banner is LOD-suppressed (which shares the same zoom gate, so in practice
	# it always caps the banner).
	if count > _view.BAND_STACK_MAX_CARDS and radius >= _view.ICON_MIN_DETAIL_RADIUS:
		var pill_center := center + _view.BAND_COUNT_BADGE_OFFSET * radius
		if has_nameplate:
			pill_center = Vector2(banner_rect.position.x + banner_rect.size.x, banner_rect.position.y + banner_rect.size.y * 0.5)
		_view._draw_count_pill(pill_center, _count_pill_text(count))

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

## THE BAND NAME PILL's geometry, resolved WITHOUT drawing — the reservation pass needs the rects
## before it knows whether the pill may be drawn at all, and the draw then reuses the very rect that
## was tested, so a granted label can never land somewhere the cull did not clear.
##
## Fixed screen size: the plate is measured off the text at `BAND_NAME_PILL_FONT_SIZE` and the gap
## below the token is a fixed pixel count, so the only thing zoom moves is the anchor (which follows
## the token's radius, keeping the pill off the glyph at every scale).
##
## ⛔ **A NAME PILL HAS TWO RECTS AND THEY ARE NOT THE SAME ONE.** Collapsing them is what let the
## cull clear two labels whose plates visibly overlapped, and what put the `×N` chip's outer half
## outside the reservation entirely.
##
## - `NAME_PILL_FOOTPRINT` — **every pixel the label inks**: the plate's round END CAPS (which reach a
##   plate half-height further out on each side than the body — see `MapView.pill_half_extent`) and
##   BOTH halves of the `×N` chip. This is what the overlap cull tests and stores.
## - `NAME_PILL_ANCHOR` — the same plate, but ending where the chip must be CENTRED, which is one chip
##   half-width past the plate's rectangular BODY. `_draw_band_stack` centres the chip on this rect's
##   right edge — the identical rule it applies to the faction bar's rect — so one anchoring line
##   serves both nameplate shapes and the mid-zoom path is untouched.
##
## **THE CHIP FOLDS ONTO THE BODY EDGE, NOT THE INKED EDGE**, and that is the look, not an oversight:
## the chip's round left cap nests into the plate's round right cap, so `Thornhollow ×4` reads as one
## nameplate. Anchoring past the inked edge would push the chip a full cap clear of the plate and it
## would read as a separate badge.
##
## Both rects are empty for a band with no name, which gets no pill at all: the name is the sim's, and
## this renderer never invents one.
const NAME_PILL_FOOTPRINT := 0
const NAME_PILL_ANCHOR := 1
func _name_pill_rects(center: Vector2, token_radius: float, band_name: String, count: int) -> Array[Rect2]:
	var text_size := _name_text_size(band_name)
	if text_size == Vector2.ZERO:
		return [Rect2(), Rect2()] as Array[Rect2]
	var half := _name_plate_half(text_size)
	var left: float = center.x - half.x
	var top: float = center.y + token_radius + _view.BAND_NAME_PILL_GAP
	var height: float = half.y * 2.0
	var inked_right: float = center.x + half.x
	var anchor_right: float = inked_right
	var footprint_right: float = inked_right
	if count > _view.BAND_STACK_MAX_CARDS:
		var chip_reach: float = _view.count_pill_reach(_count_pill_text(count))
		anchor_right = center.x + _name_plate_body_half_w(text_size) + chip_reach
		# The chip is centred on `anchor_right` and reaches `chip_reach` further right again. `maxf`
		# because a short name under a big count could still be the narrower of the two.
		footprint_right = maxf(inked_right, anchor_right + chip_reach)
	return [
		Rect2(left, top, footprint_right - left, height),
		Rect2(left, top, anchor_right - left, height),
	] as Array[Rect2]

## The name at `BAND_NAME_PILL_FONT_SIZE`, measured once and threaded through the geometry below.
## `Vector2.ZERO` for no name / no font.
func _name_text_size(band_name: String) -> Vector2:
	var font: Font = ThemeDB.fallback_font
	if font == null or band_name == "":
		return Vector2.ZERO
	return font.get_string_size(band_name, HORIZONTAL_ALIGNMENT_LEFT, -1,
		_view.BAND_NAME_PILL_FONT_SIZE)

## The plate's INKED half-extents, end caps and border included — `MapView.pill_half_extent` asked
## about this pill's padding and border. The plate is centred on `x` within its rect, so this is also
## what the draw pass offsets by to put the plate back under the token.
func _name_plate_half(text_size: Vector2) -> Vector2:
	return _view.pill_half_extent(text_size, _view.BAND_NAME_PILL_PAD_X,
		_view.BAND_NAME_PILL_BORDER_WIDTH)

## Where the plate's rectangular BODY ends, measured from the pill's centre — i.e. the plate half-
## extent WITHOUT its end cap. Only the chip anchor wants this (see `_name_pill_rects`); everything
## measuring the label's size wants `_name_plate_half`.
func _name_plate_body_half_w(text_size: Vector2) -> float:
	return text_size.x * 0.5 + _view.BAND_NAME_PILL_PAD_X + _view.BAND_NAME_PILL_BORDER_WIDTH

## The over-cap count chip's text, written once so the string the caller DRAWS and the string the
## pill MEASURES its reservation against can never drift apart.
func _count_pill_text(count: int) -> String:
	return "×%d" % count

## Draw the band's name into the ANCHOR rect `_name_pill_rects` measured and the cull cleared.
## The plate is the shared `_draw_pill_plate` — the same family as the `×N`/`+N` badges — with the
## FACTION COLOR on its border rather than its fill: the fill has to stay dark for `MARKER_BADGE_FG`
## text to read, and tinting it per faction would make every faction's label a different style.
## Returns that rect so the caller can centre the `×N` count pill on its right end, exactly as the
## faction bar does.
func _draw_band_name_pill(rect: Rect2, band_name: String, faction_color: Color) -> Rect2:
	var font: Font = ThemeDB.fallback_font
	var text_size := _name_text_size(band_name)
	if font == null or text_size == Vector2.ZERO:
		return Rect2()
	# The plate is LEFT-aligned in the rect, not centred in it: the rect ends at the CHIP ANCHOR, not
	# at the plate (see `_name_pill_rects`), while the plate itself still has to sit under the token.
	# `rect.position.x` is `center.x - half.x`, so this puts it back exactly on the hex centre.
	var half := _name_plate_half(text_size)
	var pill_center := Vector2(rect.position.x + half.x, rect.position.y + rect.size.y * 0.5)
	_view._draw_pill_plate(pill_center, text_size, _view.BAND_NAME_PILL_PAD_X, _view.MARKER_BADGE_BG,
		faction_color, _view.BAND_NAME_PILL_BORDER_WIDTH)
	_view.draw_string(font,
		Vector2(pill_center.x - text_size.x * 0.5, pill_center.y + text_size.y * NAME_PILL_BASELINE_FACTOR),
		band_name, HORIZONTAL_ALIGNMENT_LEFT, -1, _view.BAND_NAME_PILL_FONT_SIZE, _view.MARKER_BADGE_FG)
	return rect

## Where the text baseline sits below the plate's centre, as a fraction of the measured text height —
## the same optical centring `MapView._draw_count_pill` uses, so the two pill families sit their text
## identically.
const NAME_PILL_BASELINE_FACTOR := 0.32

## Faction-colored nameplate banner drawn under the PRIMARY band token (caller draws it for the
## active top card only — never the dimmed back cards). Ownership reads off the fill color, so no
## ring/disc is needed. This is the MID-zoom form only: it is sized off the token radius, so it
## cannot hold text at any legible size, and above `BAND_NAME_PILL_MIN_RADIUS` the name pill replaces
## it at the same anchor. Returns the bar Rect2 so the caller can anchor the `×N` count pill to its
## right end.
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
## glyph (scout = ⚑ flag, hunt = 🏹 bow, denial = 💀 skull, trade = 📦 pack). Phase decorations: a scout `awaiting` party pulses an
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
## glyph (scout = ⚑ flag, hunt = 🏹 bow, denial = 💀 skull, trade = 📦 pack). Phase decorations: a scout `awaiting` party pulses an
## amber ring (needs a command); a hunt `delivering` party shows a green food pip (carrying a haul
## home). The shared label / travel arrow / selection ring stay in `_draw_unit`.
func _draw_expedition_body(unit: Dictionary, center: Vector2, marker_radius: float, color: Color) -> void:
	var mission := String(unit.get("expedition_mission", ""))
	var is_hunt := mission == _view.EXPEDITION_HUNT_MISSION
	# A DENIAL raid gets its own mark rather than the bow: it engages like a hunt party and brings
	# nothing home, so a bow on the map would promise a delivery that is never coming. Its phase
	# decorations stay OFF (`is_hunt` gates those below) — the green food pip is a haul cue, and a
	# denial party's haul is a rounding error it should not advertise.
	var glyph := _view.EXPEDITION_GLYPH
	if is_hunt:
		glyph = _view.EXPEDITION_HUNT_GLYPH
	elif mission == _view.EXPEDITION_DENY_MISSION:
		glyph = _view.EXPEDITION_DENY_GLYPH
	# A SHIPMENT gets its own mark for the denial raid's reason: it engages nothing and brings nothing
	# home, so both the bow and the skull would misdescribe it. Its phase decorations stay off too —
	# the green pip means "carrying a haul HOME", and a shipment's goods are going the other way.
	elif mission == _view.EXPEDITION_TRADE_MISSION:
		glyph = _view.EXPEDITION_TRADE_GLYPH
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

## The LETHAL-GROUND mark: a ⚠ up-left of a player band's token while it stands where the sim is
## killing people (issue #614).
##
## **A STATE, NOT AN EVENT.** It is true for as long as the band is on that hex and gone the turn it
## moves off — so there is no edge to gate, and none of the *"has it camped or is it passing
## through?"* judgement an event would have needed. Nothing accumulates and nothing has to be
## dismissed.
##
## **`TileSurvivability.is_lethal` IS THE TEST**, the same authority the tile chip's ⚠ and the
## temperature overlay's hatch read, off the temperature `MapView` already decoded. A hex the world
## has no reading for draws nothing — unknown is not deadly.
##
## LOD-gated on `BAND_LETHAL_MARK_MIN_RADIUS` — its OWN threshold, higher than the banner's, because
## a ⚠ has to resolve as a triangle to mean anything and a dot or a nameplate does not. See that
## constant for the measurement. The temperature OVERLAY still hatches that ground at any zoom, which
## is the map-scale answer to the same question.
func _draw_band_lethal_mark(unit: Dictionary, center: Vector2, token_radius: float,
		radius: float) -> void:
	if radius < _view.BAND_LETHAL_MARK_MIN_RADIUS or not TileSurvivability.has_model():
		return
	var temperature: Variant = _view.tile_temperature.get(
		Vector2i(int(unit.get("current_x", -1)), int(unit.get("current_y", -1))), null)
	if temperature == null or not TileSurvivability.is_lethal(float(temperature)):
		return
	var size := int(round(token_radius * _view.BAND_LETHAL_MARK_SIZE_FACTOR))
	# Out along the up-left diagonal, far enough that the glyph's BOX clears the token — see
	# `BAND_LETHAL_MARK_CLEARANCE_FACTOR`.
	var mark_center := center + Vector2(-1.0, -1.0).normalized() \
		* (token_radius + float(size) * _view.BAND_LETHAL_MARK_CLEARANCE_FACTOR)
	_view._draw_marker_glyph(mark_center, _view.BAND_LETHAL_MARK_GLYPH, size, HudStyle.DANGER)

## One decoration on a player band marker: a food-runway dot (green/amber/red by
## the shared BandFoodStatus thresholds) up-and-right of the marker.
func _draw_band_status(unit: Dictionary, center: Vector2, marker_radius: float) -> void:
	var turns: float = float(unit.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
	var dot_color := BandFoodStatus.color_for_turns(turns)
	var dot_radius: float = marker_radius * _view.BAND_FOOD_DOT_RADIUS_FACTOR
	var dot_center := center + Vector2(marker_radius, -marker_radius) * _view.BAND_FOOD_DOT_OFFSET_FACTOR
	_view.draw_circle(dot_center, dot_radius, dot_color)
	_view.draw_arc(dot_center, dot_radius, 0, TAU, 10, Color(0, 0, 0, 0.5), 1.0)
