class_name BandOverlayRenderer
extends RefCounted

## Renders the SELECTED-BAND / SELECTED-HERD overlay family for MapView: the three range
## borders + worked-forage fills + hunted-herd rings and links of the selected player band, the
## dashed-amber optimistic PENDING overlay, the travel-destination line + reticle, the selected
## herd's graze-range ring, the corralled herd's pen footprint, and the deferred per-source
## yield-label batch. Extracted from MapView (composition — MapView owns one and calls its four
## entry points during its _draw pass). Owns only this family's selection-derived state (the
## pushed `_labor_pending` map and the per-frame `_deferred_yield_labels` batch); every draw
## command plus the shared geometry/hex/glyph/pill primitives and the unit/herd/selection state
## stay on MapView and are reached through the `_view` back-ref. Behaviour — and every rendered
## pixel — is identical to the old inlined code: the move was verified by byte-diffing all 56
## `map_preview` frames (plus the `blend_probe` set) before and after, with zero differing frames.
##
## THE YIELD-LABEL BATCH IS A TWO-PHASE CONTRACT and the whole lifecycle lives HERE:
##   1. `draw_band_work_highlights` CLEARS the batch (before its early-outs, so a deselected band
##      leaves nothing stale behind) and QUEUES a label per staffed source;
##   2. `flush_yield_labels` renders + drains it, and MapView must call it LAST in `_draw` — after
##      the markers, rings, links, pending overlays and targeting — because those layers used to
##      paint over the numbers. The far-zoom LOD gate stays at the QUEUE site, never at the flush.
##
## `set_labor_pending` is reached through a thin same-named pass-through on MapView: Main.gd wires
## the HUD's `labor_pending_changed` signal to `MapView.set_labor_pending` BY NAME (has_method /
## Callable) and `tools/map_preview.gd` calls it on the MapView too, so that seam must not move.



# Selected-player-band labor highlights (Early-Game Labor slice 3b). Distinct styles so
# the layers read apart: the three RANGE BORDERS (clean perimeter outlines = "how far each
# reach extends"), the worked forage tiles (strong green fill = "being worked"), and the
# hunted herds (red ring + link).
# See draw_band_work_highlights.
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"
# Selected-band RANGE BORDERS: three clean PERIMETER outlines (the outer boundary of each hex
# disk, traced edge-by-edge — NOT a filled tile-by-tile mesh), so the band's three reaches read
# apart at a glance: forage (green, ties to the worked-forage fills), hunt (red, ties to the
# hunted-herd rings), and scout sight (azure, the new "sight" color, kept clear of the slate fog
# tint). See _draw_range_border.
const FORAGE_RANGE_OUTLINE := Color(0.46, 0.96, 0.46, 0.85)   # green, tied to FORAGE_WORKED_*
const FORAGE_RANGE_OUTLINE_WIDTH := 2.0
const HUNT_RANGE_OUTLINE := Color(0.94, 0.40, 0.36, 0.85)     # red, tied to HUNT_WORKED_COLOR
const HUNT_RANGE_OUTLINE_WIDTH := 2.0
const SCOUT_RANGE_OUTLINE := Color(0.32, 0.66, 0.99, 0.85)    # azure "sight", distinct from fog slate
const SCOUT_RANGE_OUTLINE_WIDTH := 2.0
# Per-edge axial neighbour deltas in `_hex_points` EDGE order — edge i is the segment
# pts[i]→pts[i+1], facing the direction (angle 60·i+60): 0 SE, 1 SW, 2 W, 3 NW, 4 NE, 5 E.
# Used by _draw_range_border to test whether the tile across each edge is out of the disk (→ the
# edge is on the perimeter). Axial so it round-trips through _offset_to_axial with no odd-r
# parity table; must stay in _hex_points order.
const RANGE_BORDER_EDGE_AXIAL: Array[Vector2i] = [
	Vector2i(0, 1),   # 0 SE
	Vector2i(-1, 1),  # 1 SW
	Vector2i(-1, 0),  # 2 W
	Vector2i(0, -1),  # 3 NW
	Vector2i(1, -1),  # 4 NE
	Vector2i(1, 0),   # 5 E
]
# Worked forage tiles: strong green fill + bold outline (the tiles actually being harvested).
const FORAGE_WORKED_FILL := Color(0.30, 0.80, 0.30, 0.34)
const FORAGE_WORKED_OUTLINE := Color(0.46, 0.96, 0.46, 0.95)
const FORAGE_WORKED_OUTLINE_WIDTH := 3.0
# Hunted herds: red ring on the herd tile + a thin band→herd link (the herd can sit well
# outside the work-range ring — hunt reach = work_range + leash).
const HUNT_WORKED_COLOR := Color(0.92, 0.34, 0.30, 0.95)
const HUNT_WORKED_RING_FACTOR := 0.62   # of hex radius
const HUNT_WORKED_RING_WIDTH := 3.0
const HUNT_WORKED_LINK_COLOR := Color(0.92, 0.34, 0.30, 0.60)
const HUNT_WORKED_LINK_WIDTH := 2.5
# Selected-herd GRAZING RANGE (Grazing Phase 2b-iii): the tiles within `graze_range_radius` of the herd
# — the EXACT ring the sim grazes and derives its carrying capacity K over — as a filled region + tile
# outlines. Warm graze amber, deliberately DISTINCT from the band work-range ring's faint cyan (a herd's
# range is a different thing, and both can be on at once) and readable OVER the Pasture overlay, so the
# ring sits on the actual graze the herd lives on. radius 0 (small game) = the herd's own single tile.
const HERD_RANGE_FILL := Color(0.82, 0.55, 0.14, 0.22)     # warm graze amber, translucent region
const HERD_RANGE_OUTLINE := Color(0.96, 0.72, 0.24, 0.80)  # gold rim on each range tile
const HERD_RANGE_OUTLINE_WIDTH := 2.0
# Selected-CORRALLED-herd PEN FOOTPRINT (Grazing 2d-γ): the fenced hex disk of radius `pen_radius`
# around the pen's anchor (a penned herd's own tile), the ground it grazes to offset its larder bill.
# Deliberately a DISTINCT "fenced" tint — a cool enclosure green — NOT the warm gold of a wild herd's
# roam-range, so a fenced footprint reads as a different thing. Only drawn for a corralled herd (which
# suppresses the roam-range ring), so the two never collide.
const PEN_FOOTPRINT_FILL := Color(0.20, 0.60, 0.42, 0.22)    # enclosed-pasture green, translucent
const PEN_FOOTPRINT_OUTLINE := Color(0.34, 0.82, 0.58, 0.85) # fence-green rim on each fenced tile
const PEN_FOOTPRINT_OUTLINE_WIDTH := 2.0
# On-tile per-source yield annotations on the selected band's worked forage tiles / hunted herds:
# the assignment's `actual_yield` (food/turn) as a small drop-shadow label above the tile center
# (reusing `_draw_marker_glyph` over the shared rounded-pill plate — see `_draw_pill_plate`),
# sign-formatted to 2 decimals, food-income green — with a WARN-amber
# `⚠` overhunting flag when `actual > sustainable + ε` (mirrors the allocation panel; forage is
# renewable so never trips). ε/decimals mirror Hud's `OVERHUNT_EPSILON`/`YIELD_DECIMALS` (separate
# script, so named here rather than shared). LOD-suppressed below ICON_MIN_DETAIL_RADIUS.
# Font scales with the hex radius (clamped) so the label reads at any zoom, not just tiny at big hexes.
const YIELD_LABEL_SIZE_FACTOR := 0.16     # of hex radius
const YIELD_LABEL_MIN_FONT := 11
const YIELD_LABEL_MAX_FONT := 24
const YIELD_LABEL_OFFSET_FACTOR := 0.78   # above the tile center, as a fraction of the hex radius
const YIELD_LABEL_DECIMALS := 2
const YIELD_OVERHUNT_FLAG := "⚠"
# Backing plate: bare drop-shadowed text washed out against light terrain (tan prairie/desert), so the
# label sits on the SAME rounded dark pill chrome as the `×N`/`+N` count badges (`_draw_pill_plate`).
# Slightly translucent so the terrain still reads through. Padding is symmetric about the label's
# existing anchor (so the text does not shift) and scales with the font, like the label itself.
const YIELD_LABEL_PLATE_BG := Color(0.04, 0.05, 0.07, 0.82)
const YIELD_LABEL_PLATE_PAD_FACTOR := 0.45   # horizontal padding per side, as a fraction of the font size
# Optimistic PENDING actions (Early-Game Labor slice 3b UX): a distinct amber DASHED style
# (clearly apart from the solid confirmed green/cyan/blue/red) marks a just-issued assign/move
# that the snapshot hasn't confirmed yet. Ties to the amber "· pending" rows in the HUD panel.
const LABOR_PENDING_COLOR := Color(0.98, 0.80, 0.30, 0.98)  # amber/gold
const LABOR_PENDING_WIDTH := 2.6
const LABOR_PENDING_DASH := 10.0
const LABOR_PENDING_GAP := 7.0
const LABOR_PENDING_LINK_ALPHA := 0.7
# Travel destination (selected traveling band/expedition): a thin cyan line from the unit's
# current tile to the wrapped-nearest destination hex + a target reticle on that hex, so the
# player sees where it is headed. Distinct from the pending-amber style — this is a confirmed,
# in-progress move reported by the snapshot (`is_traveling` + `travel_target_x/y`).
const TRAVEL_DEST_COLOR := Color(0.310, 0.878, 0.812, 0.85)  # SIGNAL cyan
const TRAVEL_DEST_LINE_WIDTH := 2.0
const TRAVEL_DEST_LINE_ALPHA := 0.6           # line reads fainter than the reticle
const TRAVEL_DEST_RETICLE_FACTOR := 0.62      # reticle radius as a factor of hex radius

var _view: MapView = null
# Optimistic pending-labor map (per band entity), pushed from the HUD via set_labor_pending.
# Drawn for the selected band in a distinct dashed-amber style until the snapshot confirms.
var _labor_pending: Dictionary = {}
# DEFERRED per-source yield labels (see _queue_yield_label / flush_yield_labels). The labels are an
# annotation ON TOP OF the map, so they must be the LAST thing drawn: collected during the
# work-highlight pass, flushed at the very end of MapView's _draw.
var _deferred_yield_labels: Array[Dictionary] = []

func _init(view: MapView) -> void:
	_view = view

## Coordinator push (Hud.labor_pending_changed → Main → MapView.set_labor_pending → here): the
## per-band optimistic pending map. Stored only; the caller owns the redraw.
func set_labor_pending(pending: Dictionary) -> void:
	_labor_pending = pending if pending is Dictionary else {}

## When a player band is selected, surface what it is working (Early-Game Labor slice 3b):
##  - three RANGE BORDERS: a clean perimeter outline of each reach's hex disk (traced
##    edge-by-edge via _draw_range_border, using the sim's true **odd-r hex distance** so the
##    boundary == actually-in-range) — forage (green, `work_range`), hunt (red, `hunt_reach`,
##    only when it extends past `work_range`), and scout sight (azure, `scout_reveal_radius`,
##    only when scouts are staffed). Distinct colors so the nested reaches read apart at a glance.
##  - worked forage tiles: strong green fill on each `forage` assignment's target tile.
##  - hunted herds: a red ring on the herd tile + a band→herd link (the herd can sit outside
##    the forage border — hunt reach = work_range + leash).
## All cleared automatically when the band is deselected (selected_unit_id < 0 → early out).
func draw_band_work_highlights(radius: float, origin: Vector2) -> void:
	# Start every frame's annotation batch empty (cleared BEFORE the early-outs, so a deselected band
	# leaves no stale labels for the flush to paint).
	_deferred_yield_labels.clear()
	if _view.selected_unit_id < 0:
		return
	var band := _selected_player_band()
	if band.is_empty():
		return
	var pos: Array = Array(band.get("pos", []))
	if pos.size() != 2:
		return
	var band_col := int(pos[0])
	var band_row := int(pos[1])
	# Render neighbours in the band's wrapped column frame so the ring stays contiguous
	# across the horizontal seam.
	var eff_col := _view._band_effective_col(band_col, radius, origin)
	var band_center := _view._hex_center(eff_col, band_row, radius, origin)

	# Scouting draws no filled REVEAL DISC: `scout_reveal_radius` carries the band's scout vantage
	# distance (how far forward-observer vantages are posted, `0` with no scouts), not a revealed-area
	# radius. Staffed scouts reveal LOS from vantages that see around obstacles, and that true revealed
	# area — which the client can't reconstruct (it doesn't know the server-side LOS/terrain) — shows
	# directly in the fog. What IS drawn (below) is the azure scout range BORDER: a perimeter outline at
	# `scout_reveal_radius` marking how far the vantage reach extends, not the tiles actually revealed.

	# 1. Range borders — three clean perimeter outlines of the band's reaches (see _draw_range_border):
	#    forage (green), hunt (red, only when it extends past the forage reach), and scout sight
	#    (azure, only when scouts are staffed). Hunt is outermost, forage innermost; distinct colors
	#    so the nested reaches read apart. All at every zoom, like the old work-range ring.
	var work_range := int(band.get("work_range", 0))
	var hunt_reach := int(band.get("hunt_reach", 0))
	var scout_reveal_radius := int(band.get("scout_reveal_radius", 0))
	if work_range > 0:
		_draw_range_border(eff_col, band_row, work_range, FORAGE_RANGE_OUTLINE, FORAGE_RANGE_OUTLINE_WIDTH, radius, origin)
	if hunt_reach > work_range:
		_draw_range_border(eff_col, band_row, hunt_reach, HUNT_RANGE_OUTLINE, HUNT_RANGE_OUTLINE_WIDTH, radius, origin)
	if scout_reveal_radius > 0:
		_draw_range_border(eff_col, band_row, scout_reveal_radius, SCOUT_RANGE_OUTLINE, SCOUT_RANGE_OUTLINE_WIDTH, radius, origin)

	# 2. Worked forage tiles + 3. hunted herds, from the band's assignments. Each staffed source is
	# annotated with its per-turn `actual_yield` (LOD-suppressed at far zoom so tiny hexes stay clean).
	var show_yields := radius >= _view.ICON_MIN_DETAIL_RADIUS
	for entry_variant in _labor_assignments_of_marker(band):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var kind := String(entry.get("kind", "")).strip_edges().to_lower()
		if int(entry.get("workers", 0)) <= 0:
			continue
		if kind == LABOR_KIND_FORAGE:
			var tcol := eff_col + _view._wrapped_col_delta(band_col, int(entry.get("target_x", -1)))
			var trow := int(entry.get("target_y", -1))
			if trow < 0 or trow >= _view.grid_height:
				continue
			_view._fill_hex(tcol, trow, radius, origin, FORAGE_WORKED_FILL)
			_view._outline_hex(tcol, trow, radius, origin, FORAGE_WORKED_OUTLINE, FORAGE_WORKED_OUTLINE_WIDTH)
			# Forage patch: label the take. The ⚠ overhunt flag is the sim-answered `overdraws` bool
			# (policy-driven, false for Sustain), NOT the client-derived `actual > sustainable` — mirrors
			# `SourceForecast.source_yield_readout`. Sustain reads plain green; a Surplus/Market/Eradicate patch
			# trips ⚠.
			if show_yields and (entry.has("realized_yield") or entry.has("actual_yield")):
				var fcenter := _view._hex_center(tcol, trow, radius, origin)
				var forage_overdraw := bool(entry.get("overdraws", false))
				_queue_yield_label(fcenter, _entry_realized_yield(entry), forage_overdraw, radius,
					String(entry.get("policy", "")))
		elif kind == LABOR_KIND_HUNT:
			var herd := _view._herd_by_id(String(entry.get("fauna_id", "")))
			var herd_col := int(entry.get("target_x", -1))
			var herd_row := int(entry.get("target_y", -1))
			if not herd.is_empty():
				herd_col = int(herd.get("x", herd_col))
				herd_row = int(herd.get("y", herd_row))
			if herd_col < 0 or herd_row < 0 or herd_row >= _view.grid_height:
				continue
			var hc := _view._hex_center(eff_col + _view._wrapped_col_delta(band_col, herd_col), herd_row, radius, origin)
			# Link the band to the herd it is hunting (skip a wrap-spanning artifact).
			if absf(band_center.x - hc.x) <= _view.last_map_size.x * 0.4:
				_view.draw_line(band_center, hc, HUNT_WORKED_LINK_COLOR, HUNT_WORKED_LINK_WIDTH)
			_view.draw_arc(hc, radius * HUNT_WORKED_RING_FACTOR, 0, TAU, 28, HUNT_WORKED_COLOR, HUNT_WORKED_RING_WIDTH)
			# Depletable herd: HEADLINE the STEADY realized average (`realized_yield`), NOT the
			# kill-credit PULSE (`actual_yield` is 0 on a wait turn, a spike on a kill turn) — mirrors
			# the Band panel's hunt-headline rule in `SourceForecast.source_yield_readout` (which now reads
			# `realized_yield` for both hunt and forage), so the map label and the Band panel can never
			# disagree. Falls back to the old `sustainable_yield` if `realized_yield` is absent. The
			# overhunt ⚠ flag is the sim-answered `overdraws` bool (policy-driven, false for Sustain) —
			# NOT `actual > sustainable`, which false-positives on a kill turn when a banked animal spikes.
			if show_yields and (entry.has("realized_yield") or entry.has("sustainable_yield")):
				var overhunt := bool(entry.get("overdraws", false))
				var hunt_rate := float(entry["realized_yield"]) if entry.has("realized_yield") \
					else float(entry.get("sustainable_yield", 0.0))
				_queue_yield_label(hc, hunt_rate, overhunt, radius, String(entry.get("policy", "")))

	# 5. Optimistic PENDING actions for this band (dashed amber): a just-issued assign/move that
	#    the snapshot hasn't confirmed yet. Drawn last so it reads on top of the confirmed styles.
	_draw_band_pending(band, band_col, band_row, eff_col, band_center, radius, origin)

	# 6. Travel destination: a confirmed in-progress move the snapshot reports (`is_traveling`).
	#    Line + reticle toward the wrapped-nearest copy of the target, so it follows the short
	#    (possibly seam-crossing) path the sim actually takes. Works for bands AND expeditions.
	_draw_travel_destination(band, band_col, band_row, eff_col, band_center, radius, origin)

## Draw the selected herd's GRAZING RANGE — the hex tiles within `graze_range_radius` of its tile — as
## a filled + outlined region (Grazing Phase 2b-iii). This is the EXACT ring the sim grazes / derives K
## over, so the player sees the ground that sets the herd's carrying capacity; over the Pasture overlay
## it sits on the actual graze. `graze_range_radius == 0` (small game) → the herd's own single tile.
## Reuses the same hex-distance / fill / outline primitives as the band work-range ring (styled
## distinctly). A CORRALLED herd draws NOTHING — a penned herd doesn't roam-graze a range.
func draw_herd_range_highlights(radius: float, origin: Vector2) -> void:
	if _view.selected_herd_id == "":
		return
	var herd := _view._herd_by_id(_view.selected_herd_id)
	if herd.is_empty():
		return
	if bool(herd.get("corralled", false)):
		return
	var x := int(herd.get("x", -1))
	var y := int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _view._is_tile_visible(x, y):
		return
	var range_radius := int(herd.get("graze_range_radius", 0))
	# Render in the herd's wrapped column frame so the ring stays contiguous across the seam (mirrors
	# the band work-range ring). A ±range_radius col/row bounding box is a superset of the hex disc;
	# keep only tiles whose true odd-r hex distance from the herd is within range (radius 0 → its tile).
	var eff_col := _view._band_effective_col(x, radius, origin)
	for drow in range(-range_radius, range_radius + 1):
		var row := y + drow
		if row < 0 or row >= _view.grid_height:
			continue
		for dcol in range(-range_radius, range_radius + 1):
			var col := eff_col + dcol
			if _view._hex_distance(eff_col, y, col, row) > range_radius:
				continue
			if not _view._wrap_horizontal and (col < 0 or col >= _view.grid_width):
				continue
			_view._fill_hex(col, row, radius, origin, HERD_RANGE_FILL)
			_view._outline_hex(col, row, radius, origin, HERD_RANGE_OUTLINE, HERD_RANGE_OUTLINE_WIDTH)

## Draw the selected CORRALLED herd's PEN FOOTPRINT (Grazing 2d-γ) — the fenced hex disk of radius
## `pen_radius` around the pen's anchor (a penned herd sits AT `corralled_at`, so its own tile is the
## anchor). This is the ground the pen grazes to offset its larder bill; a distinct enclosure-green
## tint keeps it apart from a wild herd's gold roam-range. Reuses the range ring's wrapped-column /
## hex-distance / fill / outline primitives, so it clamps to map bounds the same way — the disk region
## is drawn from `pen_radius` (bounds-clamped by the loop), NOT from the server's `pen_footprint_tiles`
## count (which the DRAWER displays verbatim). Only a corralled herd draws it (the roam-range ring
## early-returns on `corralled`, so the two are mutually exclusive).
func draw_pen_footprint_highlight(radius: float, origin: Vector2) -> void:
	if _view.selected_herd_id == "":
		return
	var herd := _view._herd_by_id(_view.selected_herd_id)
	if herd.is_empty():
		return
	if not bool(herd.get("corralled", false)):
		return
	var x := int(herd.get("x", -1))
	var y := int(herd.get("y", -1))
	if x < 0 or y < 0:
		return
	if not _view._is_tile_visible(x, y):
		return
	var pen_radius := int(herd.get("pen_radius", 0))
	var eff_col := _view._band_effective_col(x, radius, origin)
	for drow in range(-pen_radius, pen_radius + 1):
		var row := y + drow
		if row < 0 or row >= _view.grid_height:
			continue
		for dcol in range(-pen_radius, pen_radius + 1):
			var col := eff_col + dcol
			if _view._hex_distance(eff_col, y, col, row) > pen_radius:
				continue
			if not _view._wrap_horizontal and (col < 0 or col >= _view.grid_width):
				continue
			_view._fill_hex(col, row, radius, origin, PEN_FOOTPRINT_FILL)
			_view._outline_hex(col, row, radius, origin, PEN_FOOTPRINT_OUTLINE, PEN_FOOTPRINT_OUTLINE_WIDTH)

## Draw the dashed-amber pending overlay for a band: pending forage tiles, pending hunted herds
## (dashed ring + dashed link), and a pending move destination (dashed tile + dashed link).
func _draw_band_pending(band: Dictionary, band_col: int, band_row: int, eff_col: int, band_center: Vector2, radius: float, origin: Vector2) -> void:
	var entity := int(band.get("entity", -1))
	var pend_variant: Variant = _labor_pending.get(entity, {})
	if not (pend_variant is Dictionary):
		return
	var pend: Dictionary = pend_variant
	var link_color := LABOR_PENDING_COLOR
	link_color.a = LABOR_PENDING_LINK_ALPHA
	var assigns_variant: Variant = pend.get("assign", {})
	if assigns_variant is Dictionary:
		for key in (assigns_variant as Dictionary):
			var a: Dictionary = (assigns_variant as Dictionary)[key]
			var kind := String(a.get("kind", "")).strip_edges().to_lower()
			if kind == LABOR_KIND_FORAGE:
				var trow := int(a.get("y", -1))
				if trow < 0 or trow >= _view.grid_height:
					continue
				var tcol := eff_col + _view._wrapped_col_delta(band_col, int(a.get("x", -1)))
				_draw_dashed_hex(tcol, trow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
			elif kind == LABOR_KIND_HUNT:
				var herd := _view._herd_by_id(String(a.get("herd_id", "")))
				if herd.is_empty():
					continue
				var hrow := int(herd.get("y", -1))
				if hrow < 0 or hrow >= _view.grid_height:
					continue
				var hcol := eff_col + _view._wrapped_col_delta(band_col, int(herd.get("x", -1)))
				var hc := _view._hex_center(hcol, hrow, radius, origin)
				_draw_dashed_hex(hcol, hrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
				if absf(band_center.x - hc.x) <= _view.last_map_size.x * 0.4:
					_draw_dashed_line(band_center, hc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)
	var move_variant: Variant = pend.get("move", {})
	if move_variant is Dictionary and not (move_variant as Dictionary).is_empty():
		var mrow := int((move_variant as Dictionary).get("y", -1))
		if mrow >= 0 and mrow < _view.grid_height:
			var mcol := eff_col + _view._wrapped_col_delta(band_col, int((move_variant as Dictionary).get("x", -1)))
			var mc := _view._hex_center(mcol, mrow, radius, origin)
			_draw_dashed_hex(mcol, mrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
			if absf(band_center.x - mc.x) <= _view.last_map_size.x * 0.4:
				_draw_dashed_line(band_center, mc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)

## Draw the selected traveling unit's destination: a thin cyan line from its current tile to the
## wrapped-nearest copy of the `travel_target` hex + a target reticle on that hex. Only the target
## coords are read when `is_traveling` (they are `0,0` otherwise). Bringing the target into the
## band's effective column frame via `_wrapped_col_delta` makes the line follow the SHORT wrapped
## path (matching the sim's seam-crossing pathing) rather than shooting the long way across the map.
func _draw_travel_destination(unit: Dictionary, band_col: int, band_row: int, eff_col: int, band_center: Vector2, radius: float, origin: Vector2) -> void:
	if not bool(unit.get("is_traveling", false)):
		return
	var target_x := int(unit.get("travel_target_x", 0))
	var target_y := int(unit.get("travel_target_y", 0))
	if target_y < 0 or target_y >= _view.grid_height:
		return
	# Already on the destination tile — nothing to draw (also guards a `0,0` slip-through).
	if target_x == band_col and target_y == band_row:
		return
	var dest_col := eff_col + _view._wrapped_col_delta(band_col, target_x)
	var dest_center := _view._hex_center(dest_col, target_y, radius, origin)
	var line_color := TRAVEL_DEST_COLOR
	line_color.a = TRAVEL_DEST_LINE_ALPHA
	_view.draw_line(band_center, dest_center, line_color, TRAVEL_DEST_LINE_WIDTH)
	# Reticle marks the destination hex; no pulse (this is a steady, confirmed heading, unlike the
	# animated targeting reticle).
	_view._draw_reticle(dest_center, radius * TRAVEL_DEST_RETICLE_FACTOR, TRAVEL_DEST_COLOR, 1.0)

## A dashed line a→b (used for pending links). `dash`/`gap` are pixel lengths.
func _draw_dashed_line(a: Vector2, b: Vector2, color: Color, width: float, dash: float, gap: float) -> void:
	var delta := b - a
	var length := delta.length()
	if length <= 0.001:
		return
	var dir := delta / length
	var pos := 0.0
	while pos < length:
		var seg_end: float = minf(pos + dash, length)
		_view.draw_line(a + dir * pos, a + dir * seg_end, color, width)
		pos = seg_end + gap

## A hex outline drawn as dashed edges (pending-tile marker).
func _draw_dashed_hex(col: int, row: int, radius: float, origin: Vector2, color: Color, width: float) -> void:
	var center := _view._hex_center(col, row, radius, origin)
	var pts := _view._hex_points(center, radius)
	for i in range(6):
		_draw_dashed_line(pts[i], pts[(i + 1) % 6], color, width, LABOR_PENDING_DASH, LABOR_PENDING_GAP)

## The selected band, if it is one of the player's own; {} otherwise.
func _selected_player_band() -> Dictionary:
	if _view.selected_unit_id < 0:
		return {}
	for unit in _view.units:
		if int(unit.get("entity", -1)) == _view.selected_unit_id and _view._is_player_unit(unit):
			return unit
	return {}

# Deliberately a LOCAL copy, NOT HudBandLaborState.labor_assignments_of: this is a MapView-side renderer
# and must not depend on the HUD's band-labor model (that would be a wrong-direction cross-layer
# coupling). Don't "finish" the dedupe by pointing it at the HUD.
func _labor_assignments_of_marker(band: Dictionary) -> Array:
	var v: Variant = band.get("labor_assignments", [])
	return v if v is Array else []

## True if (col, row) is on-map AND within hex distance `r_range` of the band — the membership test
## for a range disk. Both coords share the band's effective column frame (see _band_effective_col),
## so the delta is seam-correct; off-map tiles (row/col out of bounds, sans wrap) count as OUTSIDE,
## which is what lets a disk clipped by the map edge trace along that edge as its own border.
func _in_range_disk(eff_col: int, band_row: int, col: int, row: int, r_range: int) -> bool:
	if row < 0 or row >= _view.grid_height:
		return false
	if not _view._wrap_horizontal and (col < 0 or col >= _view.grid_width):
		return false
	return _view._hex_distance(eff_col, band_row, col, row) <= r_range

## Draw the clean PERIMETER of the hex disk of radius `r_range` centered on the band's
## (eff_col, band_row): for every in-range tile, draw each of its 6 edges ONLY when the neighbour
## across that edge is out of the disk (or off-map), which traces the exact outer boundary as one
## thin line — NOT a filled tile-by-tile mesh. Reuses the true odd-r `_hex_distance` membership test
## (via _in_range_disk) and the shared `_hex_points` vertex geometry, and is seam-wrap-correct
## because every column is measured in the band's effective frame. Shared by all three borders.
func _draw_range_border(eff_col: int, band_row: int, r_range: int, color: Color, width: float, radius: float, origin: Vector2) -> void:
	if r_range <= 0:
		return
	# A ±r_range col/row bounding box is a superset of the hex disk; _in_range_disk filters it.
	for drow in range(-r_range, r_range + 1):
		var row := band_row + drow
		if row < 0 or row >= _view.grid_height:
			continue
		for dcol in range(-r_range, r_range + 1):
			var col := eff_col + dcol
			if not _in_range_disk(eff_col, band_row, col, row, r_range):
				continue
			var axial := _view._offset_to_axial(col, row)
			var center := _view._hex_center(col, row, radius, origin)
			var pts := _view._hex_points(center, radius)
			for edge in range(6):
				var d: Vector2i = RANGE_BORDER_EDGE_AXIAL[edge]
				var noff := _view._axial_to_offset(axial.x + d.x, axial.y + d.y)
				if _in_range_disk(eff_col, band_row, noff.x, noff.y, r_range):
					continue
				_view.draw_line(pts[edge], pts[(edge + 1) % 6], color, width, true)

## The STEADY per-source rate a yield label headlines: the assignment's `realized_yield` (the honest
## long-run average of its lumpy `actual_yield`), falling back to `actual_yield` if absent (older
## snapshot). Reading the steady average keeps the map label and the Band panel row in lockstep.
func _entry_realized_yield(entry: Dictionary) -> float:
	if entry.has("realized_yield"):
		return float(entry["realized_yield"])
	return float(entry.get("actual_yield", 0.0))

## DEFER a per-source yield label instead of drawing it inline. The label is an annotation OVER the
## map: drawn during the highlight pass it was painted over by every later layer (the dashed-amber
## pending overlays, the band→herd links, the hunted-herd rings, and the secondary herd/food glyphs —
## a deer glyph landing squarely on the number). Callers queue here; `flush_yield_labels` renders the
## batch at the very END of `_draw`, on top of everything. The far-zoom LOD gate stays at the CALL
## SITE (`show_yields`), so a suppressed label is never queued and deferral can't bypass it.
func _queue_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float, policy: String = "") -> void:
	_deferred_yield_labels.append({
		"tile_center": tile_center,
		"value": value,
		"overhunt": overhunt,
		"radius": radius,
		"policy": policy,
	})

## Render (and drain) the deferred yield-label batch. Called LAST in `_draw` — after the markers,
## rings, links, pending overlays and targeting — so nothing paints over the labels.
func flush_yield_labels() -> void:
	for label in _deferred_yield_labels:
		_draw_yield_label(label["tile_center"], label["value"], label["overhunt"], label["radius"],
			label["policy"])
	_deferred_yield_labels.clear()

## A small drop-shadow per-source yield label above a worked tile's center (reuses `_draw_marker_glyph`
## for legibility over terrain). Food-income green normally; WARN amber + a `⚠` suffix when `overhunt`.
## `policy` (the assignment's take policy) appends the shared `FoodIcons` policy glyph — the SAME icon
## the Hud policy-picker buttons show — so the worked source reads "+0.38 ♻" on the map; "" = no glyph.
func _draw_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float, policy: String = "") -> void:
	var text := _format_yield_signed(value)
	var color := HudStyle.HEALTHY
	if overhunt:
		text += " " + YIELD_OVERHUNT_FLAG
		color = HudStyle.WARN
	var policy_icon := FoodIcons.for_policy(policy)
	if policy_icon != "":
		text += " " + policy_icon
	var font_size := clampi(int(radius * YIELD_LABEL_SIZE_FACTOR), YIELD_LABEL_MIN_FONT, YIELD_LABEL_MAX_FONT)
	var label_center := tile_center + Vector2(0.0, -radius * YIELD_LABEL_OFFSET_FACTOR)
	# Dark rounded plate behind the text so the label pops on ANY terrain (bare text washed out on the
	# light tan biomes). Same pill chrome as the count badges, sized to the MEASURED text+glyph run.
	var font: Font = ThemeDB.fallback_font
	if font != null:
		var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
		_view._draw_pill_plate(label_center, text_size, font_size * YIELD_LABEL_PLATE_PAD_FACTOR, YIELD_LABEL_PLATE_BG)
	_view._draw_marker_glyph(label_center, text, font_size, color)

## Signed, fixed-decimal food-rate string for the on-tile yield labels ("+0.48" / "-0.30"). Mirrors
## the HUD's `SourceForecast.format_signed`; actual yields are ≥0 but the sign keeps it explicit.
func _format_yield_signed(value: float) -> String:
	var magnitude := String.num(absf(value), YIELD_LABEL_DECIMALS).pad_decimals(YIELD_LABEL_DECIMALS)
	return ("+" if value >= 0.0 else "-") + magnitude
