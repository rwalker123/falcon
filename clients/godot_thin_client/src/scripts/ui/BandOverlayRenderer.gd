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
# WORKED-SOURCE MARKS — one ring grammar for both food webs (docs/plan_worked_source_marks.md §2.1).
# A hunted herd always wore a ring on its own marker while a foraged patch tinted the WHOLE HEX green:
# the same fact in two visual languages, and only one of them survives co-location. A hex holds a
# patch and several herds at once, so a tile-level fill cannot say WHICH of them is worked. Forage
# therefore takes the ring too, in the green it already owned, and the fill is retired.
#
# TWO WEIGHTS, not two shapes: THIN for any player band (persistent, no selection needed — the whole
# point of the mark) and BOLD for the SELECTED band, so selection still wins the eye. The band→herd
# link and the per-source yield labels stay selection-only — N bands of links is spaghetti.
const FORAGE_WORKED_COLOR := Color(0.46, 0.96, 0.46, 0.95)
const HUNT_WORKED_COLOR := Color(0.92, 0.34, 0.30, 0.95)
# Ring radius as a factor of the hex radius. A secondary marker is drawn at SECONDARY_ICON_SIZE_FACTOR
# (0.55) of the hex, so the ring sits just outside its glyph — and deliberately INSIDE the food-harvest
# ring (MapView.FOOD_HARVEST_RING_FACTOR 0.42 measured from the same centre), which is a different
# statement about the same marker and has to read apart from this one.
const WORKED_RING_FACTOR := 0.34
const WORKED_RING_WIDTH_SELECTED := 3.0
const WORKED_RING_WIDTH_OTHER := 1.6
# The selected band's ring gets a faint disc behind it — the one thing carried over from the retired
# whole-hex fill, at the source's own scale instead of the tile's.
const WORKED_RING_GLOW_ALPHA := 0.16
# Alpha applied to an unselected band's ring, so the persistent layer reads as ambient rather than
# competing with the selected band's.
const WORKED_RING_OTHER_ALPHA := 0.5
# The ONE tile-level mark left: a faint hex outline meaning "some work happens on this hex". It is an
# aggregate by design (it does not multiply with source count) and it earns its place on one argument —
# `compute_slots` returns early below ICON_MIN_DETAIL_RADIUS, so at far zoom there are no markers to
# ring and no slots to dock to. This is what survives there, and the fallback whenever a worked source
# is overflowed past the visible cap.
# The outline takes the SOURCE's own colour at this alpha, never a fixed green: a hunted herd's tile
# outlined in forage green says "we gather here", which is a different claim and a wrong one.
const WORKED_TILE_OUTLINE_ALPHA := 0.35
const WORKED_TILE_OUTLINE_WIDTH := 1.4
# THE SOURCE BADGE — one plate per worked source, docked UNDER its marker, carrying the two facts the
# ring cannot: how many people work it, and whether it can climb a rung. One plate rather than two,
# because with three sources on a hex two elements each is six things competing for the same forty
# pixels (docs/plan_worked_source_marks.md §2.2).
#
# BELOW the icon, never upper-right: `MapView.HERD_DISTRESS_BADGE_OFFSET_FACTOR` already owns that
# corner, and a herd can be both penned-and-starving and ready-to-something.
const BADGE_CREW_GLYPH := "⚒"
# The chevron is what makes the mark read "available" rather than "done". It has to be the carrier
# because the verb and standing-rung glyphs COLLIDE — ▦ is both "Sow" and "this is a Field", 🐄 both
# "Corral" and "this is a Pen" — so a bare verb glyph on a marker would say the opposite of the truth.
const BADGE_READY_CHEVRON := "⌃"
const BADGE_OFFSET_FACTOR := 0.42        # of hex radius, below the slot centre
const BADGE_FONT_SIZE_FACTOR := 0.26     # of hex radius
const BADGE_FONT_SIZE_MIN := 9
const BADGE_FONT_SIZE_MAX := 14
const BADGE_PAD_FACTOR := 0.34           # of the font size, per side
const BADGE_BG := Color(0.04, 0.05, 0.07, 0.88)
const BADGE_CREW_COLOR := Color(0.616, 0.690, 0.678, 1.0)   # HudStyle.INK_DIM
# READY wears SIGNAL cyan, NOT amber: amber is trouble in this HUD (overdraw, understaffing, a
# starving pen), and colouring an opportunity amber trains the player to read good news as a warning.
const BADGE_READY_COLOR := Color(0.310, 0.878, 0.812, 1.0)  # HudStyle.SIGNAL
# A rung UNDER WAY reads in the SAME hue one step deeper (`HudStyle.SIGNAL_DEEP`): ready and building
# are one axis in two states, so they belong to one colour family — bright says "act now", deep says
# "already under way". A different hue would file them as unrelated facts, and amber is spoken for.
const BADGE_BUILDING_COLOR := Color(0.122, 0.612, 0.557, 1.0)  # HudStyle.SIGNAL_DEEP
# The building face is `<verb glyph><percent>%`. The chevron is deliberately ABSENT: `⌃` means "you
# could start this", and the work has started. The percent is the whole point — it is what moves every
# turn, and the only number that answers "how much longer?".
const BADGE_BUILDING_FORMAT := "%s%d%% "
const BADGE_BORDER_WIDTH := 1.2
const BADGE_BORDER_IDLE := Color(0.149, 0.212, 0.235, 1.0)  # HudStyle.LINE
# Hunted herds: a thin band→herd link for the SELECTED band (the herd can sit well outside the
# work-range ring — hunt reach = work_range + leash).
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
# Selected-CARNIVORE PREY-SENSE RANGE (Predators Phase 1a): a wolf pack doesn't graze, so its graze
# ring is meaningless — when `prey_sense_radius > 0` we draw THIS ring at that radius INSTEAD (a
# replacement, not an addition), the reach the pack senses/feeds on prey over. Same "perimeter of a hex
# disk of radius N" shape as the graze ring, just a distinct PREDATOR orange (echoing MapView's
# `HUNT_DANGER_OVERLAY_COLOR`) so it reads as "predator" and never as a grazer's gold range.
const PREY_SENSE_RING_FILL := Color(0.93, 0.42, 0.13, 0.20)    # predator orange, translucent region
const PREY_SENSE_RING_OUTLINE := Color(0.98, 0.56, 0.18, 0.85) # clearer orange rim on each sensed tile
const PREY_SENSE_RING_OUTLINE_WIDTH := 2.0
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
# Below this a component is absent, not zero — the map twin of `SourceForecast.FOOD_FLOW_MIN`, and the
# test that decides WHICH of a hunt's two products a one-slot label shows (issue #337).
const YIELD_LABEL_COMPONENT_MIN := 0.001
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
## Per-source BADGES, deferred for the same reason the labels are: they annotate the map and would
## otherwise be painted over by the marker glyphs, rings and pending overlays drawn after this pass.
var _deferred_source_badges: Array[Dictionary] = []
## Per-tile roll-up of the worked sources the marker cap HID this frame: `Vector2i → {worked, ready,
## warn}`. A cap that hides state silently reads as "nothing here", which is the very failure this
## feature exists to fix at a different scale — so the `+N` chip reports what it is covering.
var _hidden_source_state: Dictionary = {}

func _init(view: MapView) -> void:
	_view = view

## Coordinator push (Hud.labor_pending_changed → Main → MapView.set_labor_pending → here): the
## per-band optimistic pending map. Stored only; the caller owns the redraw.
func set_labor_pending(pending: Dictionary) -> void:
	_labor_pending = pending if pending is Dictionary else {}

## WORLD BOUNDARY (`MapView.reset_world_state`): `_labor_pending` is pushed IN from the HUD and keyed
## by BAND ENTITY, an id the new world reuses — so an old world's optimistic dashed-amber hexes would
## reappear under a new world's band and only clear once that band's next real assignment reconciled.
## The deferred label batch is drained every frame, but it is emptied here too so a reset arriving
## mid-frame can't leave a queued label to paint over the new world.
func reset_world_state() -> void:
	_labor_pending = {}
	_deferred_yield_labels.clear()
	_deferred_source_badges.clear()
	_hidden_source_state.clear()

## EVERY player band's worked sources, drawn whatever is selected (docs/plan_worked_source_marks.md).
##
## THE MARK BELONGS TO THE SOURCE, NOT THE HEX. A hex can hold a forage patch and several herds at
## once, worked by different bands at different rungs, so a tile-level mark has to pick one answer out
## of four and cannot be right. Each mark therefore docks to the ring of the source's OWN secondary
## marker, via the slot `SecondaryMarkerRenderer.compute_slots` already assigned it — which is why
## MapView hoists that call above this one.
##
## ONE GRAMMAR, TWO WEIGHTS: green ring = we forage this, red ring = we hunt this; BOLD (plus a faint
## disc) for the selected band, THIN for every other. What SELECTION still buys is drawn by
## `draw_band_work_highlights` on top of this — the range borders, the band→herd links, the yield
## labels and the pending overlay.
##
## The faint hex OUTLINE is the one tile-level mark, and it is the fallback: a source whose marker did
## not draw (overflowed past `SECONDARY_VISIBLE_CAP`, or LOD-suppressed at far zoom, both reported as
## `slot_of == -1`) has nothing to ring, and the outline is what still says work happens here.
func draw_worked_source_marks(radius: float, origin: Vector2) -> void:
	_deferred_source_badges.clear()
	_hidden_source_state.clear()
	# CREW IS AGGREGATED PER SOURCE, NOT PER BAND — two bands can work one patch, and two badges on one
	# marker would be a lie about a single number. Keyed by the source's own slot key, the same identity
	# the ring docks to.
	var crew: Dictionary = {}
	for unit_variant in _view.units:
		if not (unit_variant is Dictionary):
			continue
		var band: Dictionary = unit_variant
		if not _view._is_player_unit(band):
			continue
		var pos: Array = Array(band.get("pos", []))
		if pos.size() != 2:
			continue
		var band_col := int(pos[0])
		var eff_col := _view._band_effective_col(band_col, radius, origin)
		# The SELECTED band's own sources read louder — selection still wins the eye.
		var selected := int(band.get("entity", -1)) == _view.selected_unit_id
		# A HUNTING EXPEDITION IS WORK ON A SOURCE TOO, and its quarry rides the COHORT rather than a
		# `labor_assignments` row — a detached party follows one herd, so the sim carries the target on
		# the party itself (`expedition_target_herd`). Without this branch a raided herd wore no mark at
		# all: the map showed the party walking and never said what it was walking to.
		#
		# Marked at EVERY phase, outbound included. "This herd is claimed" is exactly what the player
		# needs before assigning a second crew to it, and a party three turns from arrival has claimed
		# it as surely as one standing on it.
		if bool(band.get("is_expedition", false)):
			var quarry := String(band.get("expedition_target_herd", "")).strip_edges()
			if quarry != "":
				var qherd := _view._herd_by_id(quarry)
				if not qherd.is_empty():
					var qx := int(qherd.get("x", -1))
					var qrow := int(qherd.get("y", -1))
					if qx >= 0 and qrow >= 0 and qrow < _view.grid_height:
						var qcol := eff_col + _view._wrapped_col_delta(band_col, qx)
						var qkey := _view.secondary_herd_key(quarry)
						# The party's own people are the crew on that herd, and they SUM with any
						# resident band hunting it — one source, one number.
						crew[qkey] = int(crew.get(qkey, 0)) + int(band.get("size", 0))
						# A DETACHED PARTY BUILDS NOTHING — it follows the herd and hauls food home,
						# so its improvement axis is structurally empty and its quarry's badge can
						# only ever show a rung on OFFER, never one under way (issue #442). It carries
						# an escapement FLOOR (`expedition_floor`), which the rung answers never read.
						_draw_worked_mark(qcol, qrow, qkey, HUNT_WORKED_COLOR, selected, radius, origin)
						_queue_source_badge(qcol, qrow, qkey, LABOR_KIND_HUNT, qherd,
							SourceForecast.IMPROVEMENT_NONE, int(crew[qkey]), radius, origin)
						_note_if_hidden(qkey, Vector2i(qx, qrow), LABOR_KIND_HUNT, qherd,
							SourceForecast.IMPROVEMENT_NONE, false)
			# A party carries no `labor_assignments` of its own; its one source is the quarry above.
			continue
		for entry_variant in _labor_assignments_of_marker(band):
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant
			if int(entry.get("workers", 0)) <= 0:
				continue
			var kind := String(entry.get("kind", "")).strip_edges().to_lower()
			if kind == LABOR_KIND_FORAGE:
				var tx := int(entry.get("target_x", -1))
				var trow := int(entry.get("target_y", -1))
				if tx < 0 or trow < 0 or trow >= _view.grid_height:
					continue
				var tcol := eff_col + _view._wrapped_col_delta(band_col, tx)
				var fkey := _view.secondary_food_key(tx, trow)
				crew[fkey] = int(crew.get(fkey, 0)) + int(entry.get("workers", 0))
				_draw_worked_mark(tcol, trow, fkey, FORAGE_WORKED_COLOR, selected, radius, origin)
				_queue_source_badge(tcol, trow, fkey, LABOR_KIND_FORAGE,
					_view.forage_patch_lookup.get(Vector2i(tx, trow), {}),
					String(entry.get("improvement", "")), int(crew[fkey]), radius, origin)
				_note_if_hidden(fkey, Vector2i(tx, trow), LABOR_KIND_FORAGE,
					_view.forage_patch_lookup.get(Vector2i(tx, trow), {}),
					String(entry.get("improvement", "")), bool(entry.get("overdraws", false)))
			elif kind == LABOR_KIND_HUNT:
				# Herds MIGRATE, so the herd's LIVE tile is the authority; the assignment's launch-time
				# target is only the fallback for a herd that left the visible fauna set.
				var herd_id := String(entry.get("fauna_id", ""))
				var herd := _view._herd_by_id(herd_id)
				var hx := int(entry.get("target_x", -1))
				var hrow := int(entry.get("target_y", -1))
				if not herd.is_empty():
					hx = int(herd.get("x", hx))
					hrow = int(herd.get("y", hrow))
				if hx < 0 or hrow < 0 or hrow >= _view.grid_height:
					continue
				var hcol := eff_col + _view._wrapped_col_delta(band_col, hx)
				var hkey := _view.secondary_herd_key(herd_id)
				crew[hkey] = int(crew.get(hkey, 0)) + int(entry.get("workers", 0))
				_draw_worked_mark(hcol, hrow, hkey, HUNT_WORKED_COLOR, selected, radius, origin)
				_queue_source_badge(hcol, hrow, hkey, LABOR_KIND_HUNT, herd,
					String(entry.get("improvement", "")), int(crew[hkey]), radius, origin)
				_note_if_hidden(hkey, Vector2i(hx, hrow), LABOR_KIND_HUNT, herd,
					String(entry.get("improvement", "")), bool(entry.get("overdraws", false)))

## Fold a worked source the marker cap HID into its tile's roll-up, so the `+N` chip can report it.
## A source with a visible slot returns immediately — its own badge already says everything.
##
## NOT called at far zoom in any meaningful sense: `compute_slots` returns early there, so every key
## answers -1 but `_secondary_overflow` is empty too and no chip draws. The roll-up is therefore only
## ever read where a chip exists, which is exactly what it describes.
## `improvement` is the SECOND AXIS (issue #442) — the verb this crew is BUILDING, "" for none. The
## rung answers key on it, never on the harvest stance, which no longer names a build at all.
func _note_if_hidden(key: String, tile: Vector2i, kind: String, source: Dictionary,
		improvement: String, overdraws: bool) -> void:
	if _view.secondary_slot_of(key) >= 0:
		return
	var state: Dictionary = _hidden_source_state.get(tile, {"worked": false, "ready": false, "warn": false})
	state["worked"] = true
	if overdraws:
		state["warn"] = true
	if not source.is_empty() and not RungGates.next_rung_ready(kind, source, improvement, _view.faction_knowledge).is_empty():
		state["ready"] = true
	_hidden_source_state[tile] = state

## The per-tile roll-up of what the marker cap hid, for `SecondaryMarkerRenderer.draw_secondary_overflow`.
## MapView threads it across, so neither renderer holds the other.
func hidden_source_state() -> Dictionary:
	return _hidden_source_state

## Where a source's yield label hangs: its MARKER's slot when it drew in one, the hex centre otherwise.
##
## THE HEX CENTRE ALONE WAS A CO-LOCATION BUG. Every label used to anchor there for both webs, so two
## hunted herds on one hex drew two rates at the identical point, one exactly on top of the other — and
## a herd sharing a hex with a worked patch did the same. The rates belong to different sources, so
## they hang off the sources. The hex-centre fallback covers a source with no visible marker.
func _label_anchor(col: int, row: int, key: String, radius: float, origin: Vector2) -> Vector2:
	var center := _view._hex_center(col, row, radius, origin)
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return center
	return _view.secondary_slot_center(center, slot, radius)

## Queue this source's badge for the deferred flush. A source can be reached by more than one band, so
## the LAST queue for a key wins and carries the running crew total — cheaper and simpler than a second
## aggregation pass, and correct because `crew[key]` is accumulated before this is called.
##
## Skipped entirely when the source's marker did not draw (`slot_of == -1`: overflowed past the visible
## cap, or LOD-suppressed at far zoom). What the chip hides is the chip's job to report, not a badge's
## to draw somewhere arbitrary.
func _queue_source_badge(col: int, row: int, key: String, kind: String, source: Dictionary,
		improvement: String, crew: int, radius: float, origin: Vector2) -> void:
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return
	# BUILDING TAKES PRECEDENCE, and the two are mutually exclusive anyway: `next_rung_ready` excludes
	# the verb already in flight, and `rung_in_progress` answers only for that verb.
	var ready: Dictionary = {}
	var building: Dictionary = {}
	if not source.is_empty():
		building = RungGates.rung_in_progress(kind, source, improvement)
		if building.is_empty():
			ready = RungGates.next_rung_ready(kind, source, improvement, _view.faction_knowledge)
	var center := _view.secondary_slot_center(_view._hex_center(col, row, radius, origin), slot, radius)
	# One entry per source key: a later band working the same source replaces the earlier queue rather
	# than stacking a second plate on the same marker.
	for i in range(_deferred_source_badges.size()):
		if String(_deferred_source_badges[i].get("key", "")) == key:
			_deferred_source_badges.remove_at(i)
			break
	_deferred_source_badges.append({
		"key": key, "center": center, "crew": crew, "radius": radius,
		"ready_glyph": String(ready.get("glyph", "")),
		"building_glyph": String(building.get("glyph", "")),
		"building_progress": float(building.get("progress", 0.0)),
	})

## Render (and drain) the deferred badge batch — the crew count, and the ⌃ chevron when the source can
## climb. Drawn in `flush_yield_labels` alongside the yield labels, i.e. LAST in `_draw`.
func _draw_source_badge(entry: Dictionary) -> void:
	var radius := float(entry.get("radius", 0.0))
	var crew := int(entry.get("crew", 0))
	if crew <= 0:
		return
	var ready_glyph := String(entry.get("ready_glyph", ""))
	# MapView is a Node2D, so there is no theme to ask — `ThemeDB.fallback_font` is what every
	# other map-side text draw uses (`_draw_yield_label`, the count pills).
	var font: Font = ThemeDB.fallback_font
	if font == null:
		return
	var font_size := int(clampf(radius * BADGE_FONT_SIZE_FACTOR, BADGE_FONT_SIZE_MIN, BADGE_FONT_SIZE_MAX))
	var crew_text := "%s%d" % [BADGE_CREW_GLYPH, crew]
	# THE RUNG FACE — at most one of the two, a verb being neither offered nor under way at once.
	var rung_text := ""
	var rung_color := BADGE_READY_COLOR
	var building_glyph := String(entry.get("building_glyph", ""))
	if building_glyph != "":
		rung_text = BADGE_BUILDING_FORMAT % [building_glyph,
			int(round(float(entry.get("building_progress", 0.0)) * HudConst.PROGRESS_PERCENT_SCALE))]
		rung_color = BADGE_BUILDING_COLOR
	elif ready_glyph != "":
		rung_text = "%s%s " % [BADGE_READY_CHEVRON, ready_glyph]
	var text := rung_text + crew_text
	var run: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var pad := font_size * BADGE_PAD_FACTOR
	var center: Vector2 = entry["center"] + Vector2(0.0, radius * BADGE_OFFSET_FACTOR)
	var box := Rect2(center - Vector2(run.x * 0.5 + pad, run.y * 0.5 + pad * 0.5),
		Vector2(run.x + pad * 2.0, run.y + pad))
	_view.draw_rect(box, BADGE_BG, true)
	# THE BORDER carries the rung state, so the plate reads at a glance without the eye having to
	# resolve a small glyph: SIGNAL cyan when a rung is on OFFER, SIGNAL_DEEP while one is UNDER WAY,
	# the quiet line colour when the source is merely worked.
	_view.draw_rect(box, rung_color if rung_text != "" else BADGE_BORDER_IDLE, false, BADGE_BORDER_WIDTH)
	var baseline := center + Vector2(-run.x * 0.5, run.y * 0.5 - font.get_descent(font_size))
	if rung_text != "":
		var rung_run: Vector2 = font.get_string_size(rung_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
		_view.draw_string(font, baseline, rung_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, rung_color)
		baseline.x += rung_run.x
	_view.draw_string(font, baseline, crew_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, BADGE_CREW_COLOR)

## One source's worked mark: the ring on its marker's slot, plus the tile-level outline underneath.
## `slot_of(key) == -1` means the marker did not draw at all (overflowed or far zoom), so only the
## outline renders — the mark degrades to the aggregate rather than landing somewhere arbitrary.
func _draw_worked_mark(col: int, row: int, key: String, color: Color, selected: bool,
		radius: float, origin: Vector2) -> void:
	var outline := color
	outline.a = WORKED_TILE_OUTLINE_ALPHA
	_view._outline_hex(col, row, radius, origin, outline, WORKED_TILE_OUTLINE_WIDTH)
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return
	var center := _view.secondary_slot_center(_view._hex_center(col, row, radius, origin), slot, radius)
	var ring_radius := radius * WORKED_RING_FACTOR
	var ring_color := color
	if selected:
		var glow := color
		glow.a = WORKED_RING_GLOW_ALPHA
		_view.draw_circle(center, ring_radius, glow)
	else:
		ring_color.a = color.a * WORKED_RING_OTHER_ALPHA
	var width := WORKED_RING_WIDTH_SELECTED if selected else WORKED_RING_WIDTH_OTHER
	_view.draw_arc(center, ring_radius, 0, TAU, 28, ring_color, width)

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
			# (The worked ring itself is drawn by `draw_worked_source_marks`, for EVERY player band.)
			# Forage patch: label the take. The ⚠ overhunt flag is the sim-answered `overdraws` bool
			# (it answers the crew's own floor), NOT the client-derived `actual > sustainable` — mirrors
			# `SourceForecast.source_yield_readout`. Sustain reads plain green; a Surplus/Deplete/Eradicate patch
			# trips ⚠.
			if show_yields and (entry.has("realized_yield") or entry.has("actual_yield")):
				var fcenter := _label_anchor(tcol, trow, _view.secondary_food_key(int(entry.get("target_x", -1)), trow), radius, origin)
				var forage_overdraw := bool(entry.get("overdraws", false))
				# The FODDER component rides along for the one-slot rule in `_draw_yield_label`; a
				# forage patch normally pays food, so it changes nothing here — except on the patch
				# this exists for, a sown hay Field, which pays fodder alone.
				_queue_yield_label(fcenter, _entry_realized_yield(entry), forage_overdraw, radius,
					_entry_floor_glyph(entry), _entry_fodder(entry))
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
			# (The worked ring itself is drawn by `draw_worked_source_marks`, for EVERY player band.)
			# Depletable herd: HEADLINE the STEADY realized average (`realized_yield`), NOT the
			# kill-credit PULSE (`actual_yield` is 0 on a wait turn, a spike on a kill turn) — mirrors
			# the Band panel's hunt-headline rule in `SourceForecast.source_yield_readout` (which now reads
			# `realized_yield` for both hunt and forage), so the map label and the Band panel can never
			# disagree. Falls back to the old `sustainable_yield` if `realized_yield` is absent. The
			# overhunt ⚠ flag is the sim-answered `overdraws` bool (it answers the crew's own floor) —
			# NOT `actual > sustainable`, which false-positives on a kill turn when a banked animal spikes.
			if show_yields and (entry.has("realized_yield") or entry.has("sustainable_yield")):
				var hlabel := _label_anchor(eff_col + _view._wrapped_col_delta(band_col, herd_col), herd_row,
					_view.secondary_herd_key(String(entry.get("fauna_id", ""))), radius, origin)
				var overhunt := bool(entry.get("overdraws", false))
				var hunt_rate := float(entry["realized_yield"]) if entry.has("realized_yield") \
					else float(entry.get("sustainable_yield", 0.0))
				# NO FODDER ARGUMENT, and that is a decision rather than an omission (issue #449): no
				# animal is harvested for feed, so a hunt row's fodder is a structural zero and passing
				# it would only offer the label a fall-through it can never take. **An INEDIBLE quarry
				# therefore has no fall-through at all since arc #527** — its steady food rate is
				# honestly 0 and the trade rate it used to fall through to is retired.
				_queue_yield_label(hlabel, hunt_rate, overhunt, radius, _entry_floor_glyph(entry))

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
##
## CARNIVORE PREY-SENSE (Predators Phase 1a): a wolf pack doesn't graze, so `prey_sense_radius > 0`
## (the sim's carnivore signal AND ring radius) REPLACES the graze ring — same disk shape, drawn at
## the prey-sense radius in a distinct predator orange. A herbivore (`prey_sense_radius == 0`) is
## unchanged: it draws its gold graze ring.
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
	# A predator (`prey_sense_radius > 0`) draws its prey-sense ring INSTEAD of the graze ring — the
	# radius and the "this is a carnivore" test are the same wire field; a herbivore keeps the graze ring.
	var prey_sense_radius := int(herd.get("prey_sense_radius", 0))
	var is_predator := prey_sense_radius > 0
	var range_radius := prey_sense_radius if is_predator else int(herd.get("graze_range_radius", 0))
	var fill_color := PREY_SENSE_RING_FILL if is_predator else HERD_RANGE_FILL
	var outline_color := PREY_SENSE_RING_OUTLINE if is_predator else HERD_RANGE_OUTLINE
	var outline_width := PREY_SENSE_RING_OUTLINE_WIDTH if is_predator else HERD_RANGE_OUTLINE_WIDTH
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
			_view._fill_hex(col, row, radius, origin, fill_color)
			_view._outline_hex(col, row, radius, origin, outline_color, outline_width)

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

## Its FODDER twin (issue #449) — and there is deliberately NO realized fallback to make: only the
## animal web projects a steady rate, and fodder is paid by the plant web alone, so the actual IS the
## honest rate (`SourceForecast.fodder_rate_of` is the one definition and says why at length). 0 on
## every hunt entry and on any patch growing no feed, which is what suppresses the component.
func _entry_fodder(entry: Dictionary) -> float:
	return float(entry.get("fodder_yield", 0.0))

## DEFER a per-source yield label instead of drawing it inline. The label is an annotation OVER the
## map: drawn during the highlight pass it was painted over by every later layer (the dashed-amber
## pending overlays, the band→herd links, the hunted-herd rings, and the secondary herd/food glyphs —
## a deer glyph landing squarely on the number). Callers queue here; `flush_yield_labels` renders the
## batch at the very END of `_draw`, on top of everything. The far-zoom LOD gate stays at the CALL
## SITE (`show_yields`), so a suppressed label is never queued and deferral can't bypass it.
## The assignment's harvest MARK — its floor's zone glyph, the same one the Band panel's work row
## draws, so a worked source reads alike on the map and in the panel. `assign_labor` always carries a
## floor and the decoder always inserts it, so an absent one means the wire never described this
## assignment; the sim's own default is then the honest reading.
func _entry_floor_glyph(entry: Dictionary) -> String:
	return FoodIcons.for_floor_zone(SourceForecast.floor_zone(
		float(entry.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))))

func _queue_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float, floor_glyph: String = "",
		fodder: float = 0.0) -> void:
	_deferred_yield_labels.append({
		"tile_center": tile_center,
		"value": value,
		"overhunt": overhunt,
		"radius": radius,
		"floor_glyph": floor_glyph,
		"fodder": fodder,
	})

## Render (and drain) the deferred yield-label batch. Called LAST in `_draw` — after the markers,
## rings, links, pending overlays and targeting — so nothing paints over the labels.
func flush_yield_labels() -> void:
	for badge in _deferred_source_badges:
		_draw_source_badge(badge)
	_deferred_source_badges.clear()
	for label in _deferred_yield_labels:
		_draw_yield_label(label["tile_center"], label["value"], label["overhunt"], label["radius"],
			label["floor_glyph"], float(label.get("fodder", 0.0)))
	_deferred_yield_labels.clear()

## A small drop-shadow per-source yield label above a worked tile's center (reuses `_draw_marker_glyph`
## for legibility over terrain). Food-income green normally; WARN amber + a `⚠` suffix when `overhunt`.
## `floor_glyph` is a RESOLVED GLYPH, appended verbatim after the rate — the floor-zone mark
## (`_entry_floor_glyph`), the same one the Hud's floor-picker buttons and the work board's mark column
## show, so a worked source reads "+0.38 ♻" on the map; "" = no glyph.
##
## **IT IS APPENDED, NEVER LOOKED UP AGAIN.** This parameter was named `policy` and ran back through
## `FoodIcons.for_policy` — a table keyed on the four IMPROVEMENT verbs since #442, which a floor-zone
## glyph is never a key of — so the lookup answered `""` and **the map drew no harvest mark at all**.
## A glyph resolved once and re-resolved is a mark that silently disappears the next time either table
## is re-keyed; the argument arrives resolved and is spent as-is.
##
## ONE COMPONENT ONLY, and deliberately so (issues #337 / #449): a source pays a VECTOR — food and
## fodder — but a map label sits on a hex a few pixels wide beside a floor mark and a ⚠, and there is
## no room for a second rate. It shows the one the source actually PAYS, in the wire's own order: food
## when there is food (every edible quarry and every forage patch), else the fodder rate spelled with
## the WORD (fodder has no glyph). A sown hay Field therefore reads `+0.40 fodder ♻` rather than the
## `+0.00` that said it was worth nothing. (A trade branch sat between the two until arc #527 retired
## that account.)
func _draw_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float, floor_glyph: String = "",
		fodder: float = 0.0) -> void:
	var text := _yield_label_rate_text(value, fodder)
	var color := HudStyle.HEALTHY
	if overhunt:
		text += " " + YIELD_OVERHUNT_FLAG
		color = HudStyle.WARN
	if floor_glyph != "":
		text += " " + floor_glyph
	var font_size := clampi(int(radius * YIELD_LABEL_SIZE_FACTOR), YIELD_LABEL_MIN_FONT, YIELD_LABEL_MAX_FONT)
	var label_center := tile_center + Vector2(0.0, -radius * YIELD_LABEL_OFFSET_FACTOR)
	# Dark rounded plate behind the text so the label pops on ANY terrain (bare text washed out on the
	# light tan biomes). Same pill chrome as the count badges, sized to the MEASURED text+glyph run.
	var font: Font = ThemeDB.fallback_font
	if font != null:
		var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
		_view._draw_pill_plate(label_center, text_size, font_size * YIELD_LABEL_PLATE_PAD_FACTOR, YIELD_LABEL_PLATE_BG)
	_view._draw_marker_glyph(label_center, text, font_size, color)

## THE ONE-SLOT CHOICE, on its own so it can be asserted: which of the accounts this label states, and
## how it is spelled. Split out of `_draw_yield_label` because a draw call renders to a canvas and a
## harness cannot read a glyph back off one — the fall-through order (food → fodder) is the claim, and
## it needs somewhere to be asked. `YIELD_LABEL_COMPONENT_MIN` is the same threshold on both, so no
## account can be shown at a magnitude the other would have been hidden at.
func _yield_label_rate_text(value: float, fodder: float) -> String:
	if absf(value) < YIELD_LABEL_COMPONENT_MIN and fodder >= YIELD_LABEL_COMPONENT_MIN:
		return SourceForecast.PICKER_FODDER_PRODUCT_FORMAT % _format_yield_signed(fodder)
	return _format_yield_signed(value)

## Signed, fixed-decimal food-rate string for the on-tile yield labels ("+0.48" / "-0.30"). Mirrors
## the HUD's `SourceForecast.format_signed`; actual yields are ≥0 but the sign keeps it explicit.
func _format_yield_signed(value: float) -> String:
	var magnitude := String.num(absf(value), YIELD_LABEL_DECIMALS).pad_decimals(YIELD_LABEL_DECIMALS)
	return ("+" if value >= 0.0 else "-") + magnitude
