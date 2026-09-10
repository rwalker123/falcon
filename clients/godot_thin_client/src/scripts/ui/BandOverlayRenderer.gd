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
# The band-level BUILD pool's row kind (`docs/plan_standing_upkeep.md` §2.5). It works no tile, so it
# is never a worked source here — it is read for the source badge's *is anybody building this* fork.
const LABOR_KIND_BUILDERS := "builders"
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
# **AND A THIRD COLOUR FOR THE THIRD WEB** (issue #650): a worked WORKING wears the same ring, link
# and outline as a patch or a herd, in a hue that is in neither food web — quarried slate, one colour
# for both branches, because a felled wood and a quarried rock are the same statement about the same
# account. Desaturated on purpose: every other mark this pass draws is saturated (forage green, hunt
# red, the azure scout border, the amber pending dashes), so slate is the one thing on the map it
# cannot be mistaken for, and materials are the cool account beside two warm-blooded food webs.
const EXTRACT_WORKED_COLOR := Color(0.68, 0.74, 0.80, 0.95)
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
# **ONE SPELLING, AND IT IS THE CARD'S** (issue #650): the tile card's deposit rows wear the same
# mark, so the glyph lives in `HudSelectionVocab` and both surfaces read it from there. A hex whose
# badge said `⚒4` beside a card row that said nothing about a crew is half of what Ray reported.
const BADGE_CREW_GLYPH := HudSelectionVocab.SOURCE_CREW_MARK
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
# READY wears `HudStyle.SIGNAL` cyan, NOT amber: amber is trouble in this HUD (overdraw,
# understaffing, a starving pen), and colouring an opportunity amber trains the player to read good
# news as a warning. These five badge tints are READ OFF `HudStyle` at their draw sites rather than
# copied into consts here — the copies existed, and had already drifted from the palette they named.
# A rung UNDER WAY reads in the SAME hue one step deeper (`HudStyle.SIGNAL_DEEP`): ready and building
# are one axis in two states, so they belong to one colour family — bright says "act now", deep says
# "already under way". A different hue would file them as unrelated facts, and amber is spoken for.
# The building face is `<verb glyph><percent>%`. The chevron is deliberately ABSENT: `⌃` means "you
# could start this", and the work has started. The percent is the whole point — it is what moves every
# turn, and the only number that answers "how much longer?".
const BADGE_BUILDING_FORMAT := "%s%d%% "
# **THE THIRD FACE: a rung declared with nobody on it.** It keeps the verb glyph and puts a `⚠` where
# the percentage would go, because the percentage is precisely the lie — a `0%` plate over a build the
# player staffed with nobody is pixel-identical to one they started this turn. There is no number to
# put here: nothing is moving, which is the whole message.
const BADGE_UNSTAFFED_FORMAT := "%s⚠ "
# Amber, and this is the one rung face that earns it. The READY note above explains why an
# OPPORTUNITY must never be amber; an unstaffed commitment is not an opportunity — it is the trouble
# channel's own subject, and it wears the `HudStyle.WARN` the overdraw and under-herded marks do.
const BADGE_BORDER_WIDTH := 1.2
# THE SELECTED BAND'S LINK TO A SOURCE IT WORKS — a thin line from its token to the source's own
# marker (the source can sit well outside the work-range ring: hunt reach = work_range + leash, and a
# working is joined to whichever band opened it). ONE alpha and ONE width for every kind, applied to
# the SOURCE's own ring colour by `_draw_worked_link`, so a link can never be a different weight from
# web to web — the ring already carries which source this is.
const WORKED_LINK_ALPHA := 0.60
const WORKED_LINK_WIDTH := 2.5
# A link whose two ends straddle the horizontal seam is a stripe across the whole map rather than a
# join, so it is dropped: past this fraction of the rendered map width the two ends are not
# neighbours. Shared by the CONFIRMED links and the dashed PENDING ones — one seam, one rule.
const LINK_MAX_SPAN_FACTOR := 0.4
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
# `⚠` overhunting flag driven by the assignment's own `overdraws`, the sim's verdict that every
# surface flying this mark reads (never a client-side `actual > sustainable` comparison, which the
# schema forbids). Decimals mirror Hud's `YIELD_DECIMALS` (separate script, so named here rather than
# shared). LOD-suppressed below ICON_MIN_DETAIL_RADIUS.
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
# The plate is drawn with no border at all (`_draw_pill_plate` is passed no border colour), so its
# inked reach is measured with none — `MapView.COUNT_PILL_NO_BORDER_WIDTH`'s note, on this pill.
const YIELD_LABEL_PLATE_NO_BORDER := 0.0
# **HOW FAR A PILL IS LIFTED OFF ONE ALREADY PLACED THIS FRAME**, as a multiple of its own plate
# height: one whole plate plus a quarter, so the two read as two stacked pills with daylight between
# them rather than as one tall plate. See `flush_yield_labels`, which is where the lift happens and
# why it is a lift rather than a cull or a merge.
const YIELD_LABEL_STACK_STEP_FACTOR := 1.25
# Optimistic PENDING actions (Early-Game Labor slice 3b UX): a distinct amber DASHED style
# (clearly apart from the solid confirmed green/cyan/blue/red) marks a just-issued assign/move
# that the snapshot hasn't confirmed yet. Ties to the amber "· pending" rows in the HUD panel.
const LABOR_PENDING_COLOR := Color(0.98, 0.80, 0.30, 0.98)  # amber/gold
const LABOR_PENDING_WIDTH := 2.6
const LABOR_PENDING_DASH := 10.0
const LABOR_PENDING_GAP := 7.0
const LABOR_PENDING_LINK_ALPHA := 0.7
# Travel destination (selected traveling band/expedition): a thin line in the targeting tint from
# the unit's current tile to the wrapped-nearest destination hex + a target reticle on that hex, so
# the player sees where it is headed. Distinct from the pending-amber style — this is a confirmed,
# in-progress move reported by the snapshot (`is_traveling` + `travel_target_x/y`).
#
# It rides `HudStyle.SIGNAL` because a confirmed destination IS a targeting affordance, and it is
# read at the DRAW SITE rather than copied into a const: a copy freezes at whichever palette was
# loaded when this script parsed, so it would stay cyan under every theme. That is not
# hypothetical — this line was exactly that copy, and the three `map_travel_*` preview frames were
# still rendering the old cyan after the theme swap.
const TRAVEL_DEST_ALPHA := 0.85
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
## THE WORKED WORKINGS this frame — `working_key → {tile, material, crew, selected}` — see
## `compute_worked_workings`, which is the one place it is built.
var _worked_workings: Dictionary = {}

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
	_worked_workings.clear()

## **WHICH WORKINGS ARE BEING CUT, AND BY HOW MANY** — `working_key → {tile, material, crew,
## selected}`, summed across EVERY player band (issue #650). `MapView._draw` calls this FIRST and hands the answer to
## `SecondaryMarkerRenderer.set_worked_workings`, because a working's marker exists only where a crew
## is on it: the slot pass needs this pass's answer, which is the reverse of the food/herd order.
##
## **IT IS THE MAP-SIDE TWIN OF `SubjectDrawerController._cutters_on_working`, DELIBERATELY LOCAL.**
## That helper sums the same `(tile, material)` crew across every player band, and the tile card's
## `⚒N` clause is its face — but it reaches it through `HudBandLaborState`, and a renderer must not
## depend on the HUD's band-labor model (the rule `_labor_assignments_of_marker` beside it already
## follows). So this reads the same rows off the MARKERS' own `labor_assignments`, exactly as the
## forage and hunt arms below do, and the two surfaces agree because they sum the same wire field
## over the same set of bands rather than because one calls the other.
##
## **NOT PENDING-AWARE, matching the forage and hunt arms**: the map's marks report the crew the
## SNAPSHOT confirms, and the dashed-amber pending overlay is the separate statement about an order
## that has not landed yet. (The tile card's clause IS pending-aware; the disagreement lasts one
## frame and is the same one the existing worked-source badges already have.)
##
## **A WORKING THE `deposits` SECTION DOES NOT KNOW ABOUT DRAWS NOTHING.** The map states a working
## the snapshot carries, never one a labor row asserts — the same `(tile, material)` join the tile
## card and the Workings roster make — so a lapsing row pointing at a hex with no such deposit cannot
## put a phantom marker on it.
func compute_worked_workings() -> Dictionary:
	_worked_workings.clear()
	for unit_variant in _view.units:
		if not (unit_variant is Dictionary):
			continue
		var band: Dictionary = unit_variant
		if not _view._is_player_unit(band):
			continue
		# A DETACHED PARTY CUTS NOTHING — it carries no `labor_assignments` of its own, its one
		# source being the quarry the mark pass reads off the cohort.
		if bool(band.get("is_expedition", false)):
			continue
		for entry_variant in _labor_assignments_of_marker(band):
			if not (entry_variant is Dictionary):
				continue
			var entry: Dictionary = entry_variant
			var workers := int(entry.get("workers", 0))
			if workers <= 0:
				continue
			if String(entry.get("kind", "")).strip_edges().to_lower() != HudConst.LABOR_KIND_EXTRACT:
				continue
			var material := String(entry.get("material", "")).strip_edges()
			if material == "":
				continue
			var tile := Vector2i(int(entry.get("target_x", -1)), int(entry.get("target_y", -1)))
			if tile.x < 0 or tile.y < 0 or tile.y >= _view.grid_height:
				continue
			if not _working_on_tile(tile, material):
				continue
			var key := _view.secondary_working_key(tile.x, tile.y, material)
			var known: Dictionary = _worked_workings.get(key, {})
			_worked_workings[key] = {
				"tile": tile,
				"material": material,
				"crew": int(known.get("crew", 0)) + workers,
				# **IS THE SELECTED BAND ONE OF THE CUTTERS** — an OR across the bands summed above,
				# and it rides here for the same reason the crew does: this walk is the one place a
				# working is joined to the bands working it, and the mark pass walks SOURCES rather
				# than bands (the crew is already summed) so it has no band in hand to ask. The food
				# webs reach the same answer per (band × source) as they draw, which is why they
				# carry no such flag. It is what picks the ring's WEIGHT — bold for the selected
				# band, thin for any other — exactly as `selected` does on the two food arms.
				"selected": bool(known.get("selected", false))
					or int(band.get("entity", -1)) == _view.selected_unit_id,
			}
	return _worked_workings

## Does the `deposits` section carry a working of `material` on this hex — the `(tile, material)` join
## every other surface makes, restated here because this renderer holds no deposit model.
func _working_on_tile(tile: Vector2i, material: String) -> bool:
	return not _working_row(tile, material).is_empty()

## The `deposits` row for this `(tile, material)` pair, `{}` where the section carries none. The join
## every other surface makes, restated here because this renderer holds no deposit model — and it
## answers the ROW rather than a bool because the pill needs a FIELD off it (`regrowth_rate`, through
## `HudDepositVocab.floor_mark`) and not only the row's existence.
func _working_row(tile: Vector2i, material: String) -> Dictionary:
	var rows: Variant = _view.deposit_tile_lookup.get(tile, null)
	if not (rows is Array):
		return {}
	for row in (rows as Array):
		if row is Dictionary and HudDepositVocab.material_of(row as Dictionary) == material:
			return row as Dictionary
	return {}

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
	# **AND THE BUILD CREW IS AGGREGATED THE SAME WAY, because "nobody is building this" is a claim
	# about the SOURCE and not about one band's row.** One band's declaration with no builders is
	# covered by another band's builders on the same rung, so the badge asks the summed count.
	#
	# **THE COUNT IS THE BAND'S `builders` POOL, not a per-source crew** (`docs/plan_standing_upkeep.md`
	# §2.5): a verb declares and names no hands. Added once per (band × source it works), which is the
	# set of bands that could hold this source in a queue at all.
	var builders: Dictionary = {}
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
						var qcenter := _view._hex_center(qcol, qrow, radius, origin)
						_draw_worked_mark(qcenter, qkey, HUNT_WORKED_COLOR, selected, radius)
						_queue_source_badge(qcenter, qkey, LABOR_KIND_HUNT, qherd,
							SourceForecast.IMPROVEMENT_NONE, int(crew[qkey]), radius, origin,
							int(builders.get(qkey, 0)))
						_note_if_hidden(qkey, Vector2i(qx, qrow), LABOR_KIND_HUNT, qherd,
							SourceForecast.IMPROVEMENT_NONE, false)
			# A party carries no `labor_assignments` of its own; its one source is the quarry above.
			continue
		# This band's whole build pool, resolved once and credited to every source it works — the
		# queue that spends it is the band's, and this renderer has no queue.
		var band_builders := _builders_pool_of_marker(band)
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
				builders[fkey] = int(builders.get(fkey, 0)) + band_builders
				var fcenter := _view._hex_center(tcol, trow, radius, origin)
				_draw_worked_mark(fcenter, fkey, FORAGE_WORKED_COLOR, selected, radius)
				_queue_source_badge(fcenter, fkey, LABOR_KIND_FORAGE,
					_view.forage_patch_lookup.get(Vector2i(tx, trow), {}),
					String(entry.get("improvement", "")), int(crew[fkey]), radius, origin,
					int(builders[fkey]))
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
				builders[hkey] = int(builders.get(hkey, 0)) + band_builders
				var hcenter := _view._hex_center(hcol, hrow, radius, origin)
				_draw_worked_mark(hcenter, hkey, HUNT_WORKED_COLOR, selected, radius)
				_queue_source_badge(hcenter, hkey, LABOR_KIND_HUNT, herd,
					String(entry.get("improvement", "")), int(crew[hkey]), radius, origin,
					int(builders[hkey]))
				_note_if_hidden(hkey, Vector2i(hx, hrow), LABOR_KIND_HUNT, herd,
					String(entry.get("improvement", "")), bool(entry.get("overdraws", false)))
	# **THE WORKED WORKINGS** (issue #650), off the set `compute_worked_workings` resolved before the
	# slot pass — this walk is over SOURCES rather than bands because the crew is already summed.
	#
	# **THE SAME `_draw_worked_mark` THE TWO FOOD WEBS GO THROUGH, in the working's own colour.** The
	# first cut queued the badge alone on the argument that a working's marker exists only where a
	# crew is on it, so a ring would state *we work this* twice — which is true of the ring's
	# INFORMATION and false of the map: beside a hunted herd wearing a ring, a hex outline, a link
	# and a rate, a lone `⚒1` plate read as a different and lesser kind of thing (Ray, issue #650).
	# Parity is the requirement, so a working takes every part a patch and a herd take, through the
	# same routines, and `EXTRACT_WORKED_COLOR` is what keeps it out of either food web's language —
	# including on the tile-level outline, which is therefore its LOD/overflow fallback too.
	for key in _worked_workings:
		var working: Dictionary = _worked_workings[key]
		var wtile: Vector2i = working.get("tile", Vector2i(-1, -1))
		# The working's OWN wrap image, the one its marker and its badge are drawn on.
		var wcenter := _view._hex_center_wrapped(wtile.x, wtile.y, radius, origin)
		_draw_worked_mark(wcenter, key, EXTRACT_WORKED_COLOR,
			bool(working.get("selected", false)), radius)
		# **THE PLATE IS THE SHARED `⚒N` SOURCE BADGE, not a shape of its own** — the tile card's
		# deposit row, the band badge and this plate are one spelling of one idea (`BADGE_CREW_GLYPH`
		# → `HudSelectionVocab.SOURCE_CREW_MARK`), so a hex whose marker said one number beside a card
		# row saying another is unwritable. It is queued with an EMPTY source deliberately: the rung
		# answers `_queue_source_badge` reaches for (`RungGates.rung_in_progress` /
		# `next_rung_ready`) are the FOOD webs' — they answer nothing for an `extract` kind — and a
		# working's ladder is declared from the Work board, so the plate states the crew and stops.
		_queue_source_badge(wcenter, key,
			HudConst.LABOR_KIND_EXTRACT, {}, SourceForecast.IMPROVEMENT_NONE,
			int(working.get("crew", 0)), radius, origin)
		_note_if_hidden(key, wtile, HudConst.LABOR_KIND_EXTRACT, {},
			SourceForecast.IMPROVEMENT_NONE, false)

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
## `hex_center` arrives resolved for `_draw_worked_mark`'s reason — the wrap image is the caller's to
## pick, and a label must hang off the same copy of the hex its ring is drawn on.
func _label_anchor(hex_center: Vector2, key: String, radius: float) -> Vector2:
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return hex_center
	return _view.secondary_slot_center(hex_center, slot, radius)

## Queue this source's badge for the deferred flush. A source can be reached by more than one band, so
## the LAST queue for a key wins and carries the running crew total — cheaper and simpler than a second
## aggregation pass, and correct because `crew[key]` is accumulated before this is called.
##
## Skipped entirely when the source's marker did not draw (`slot_of == -1`: overflowed past the visible
## cap, or LOD-suppressed at far zoom). What the chip hides is the chip's job to report, not a badge's
## to draw somewhere arbitrary.
## `builders` is this source's BUILD crew, summed across bands — see `_draw_source_badge` for why a
## rung under way has to know it.
## `hex_center` is the plate's ANCHOR HEX in screen space, passed in rather than derived from a
## column here: the food and hunt arms anchor to the BAND's wrap image (`eff_col + delta`, so a
## source across the seam draws beside the band that works it) while a working anchors to its own
## marker's (`_hex_center_wrapped`), and a badge that resolved the wrap for itself would eventually
## disagree with the marker it hangs under.
func _queue_source_badge(hex_center: Vector2, key: String, kind: String, source: Dictionary,
		improvement: String, crew: int, radius: float, origin: Vector2,
		builders: int = 0) -> void:
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return
	# BUILDING TAKES PRECEDENCE, and the two are mutually exclusive anyway: `next_rung_ready` excludes
	# the verb already in flight, and `rung_in_progress` answers only for that verb.
	var ready: Dictionary = {}
	var building: Dictionary = {}
	# **IS THAT RUNG STALLED — unstaffed, or going backwards?** Asked ONLY where a rung is under way,
	# off the meter `rung_in_progress` has just resolved, so the plate's warning and its glyph provably
	# describe the same verb and this renderer resolves nothing about the ladder for itself.
	#
	# **ONE FUNCTION ANSWERS IT, AND THAT IS THE POINT** (`docs/plan_standing_upkeep.md` §4.6a). The
	# two halves used to be composed here — `unstaffed_build_of` for *declared and never started*,
	# `build_is_losing` for the wire's own rot verdict — and the WORK BOARD, which had no such fork at
	# all, went on printing a confident percent whatever the staffing. `SourceForecast.build_is_stalled`
	# is now the single producer both surfaces call, so the map cannot show an alert the Work tab does
	# not. It is asked with the BUILDERS because `BUILD_METER_HOLDS` covers both a crew treading water
	# and a build parked on purpose — and only the first is news.
	var stalled := false
	if not source.is_empty():
		building = RungGates.rung_in_progress(kind, source, improvement)
		# **AND THE PLATE STATES THE LEG IN FLIGHT** (`docs/plan_standing_upkeep.md` §2.8), the same
		# re-pointing the Work tab's two readouts take. A `sow` on untended ground is one entry and
		# two legs, so the declared rung's meter reads 0% for the whole first leg — the badge would
		# sit at `▦0%` while the crew cleared the ground. **The pairing is why it is here rather than
		# only on the board**: this plate and the work row are held to ONE verdict by
		# `band_panel_preview._assert_work_row_and_badge_agree`, so a leg-aware board beside a
		# destination-bound badge is the two-surface disagreement `build_is_stalled` exists to stop.
		building = RungGates.leg_in_progress(source, building)
		if building.is_empty():
			ready = RungGates.next_rung_ready(kind, source, improvement, _view.faction_knowledge)
		else:
			stalled = SourceForecast.build_is_stalled(
				source, float(building.get("progress", 0.0)), builders)
	var center := _view.secondary_slot_center(hex_center, slot, radius)
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
		"stalled": stalled,
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
	var rung_color := HudStyle.SIGNAL
	var building_glyph := String(entry.get("building_glyph", ""))
	if building_glyph != "":
		# **A PERCENT ON A BUILD NOBODY IS STAFFING IMPLIES PROGRESS THAT IS NOT HAPPENING.** The
		# unstaffed face drops the number entirely (see `BADGE_UNSTAFFED_FORMAT`); the plate still says
		# WHICH rung is promised here, and stops saying it is being worked.
		#
		# **A LOSING METER TAKES THE SAME FACE, and it is the wire's verdict rather than a staffing
		# guess** (§4.6a). A percent that is falling is the same lie as a percent that is not moving —
		# but a meter merely PARKED, with its keeping covered, keeps its number, because that number is
		# honest and the state is a decision rather than a failure. Both halves are
		# `SourceForecast.build_is_stalled`, resolved at queue time; the work board's rung slot calls
		# the same function, which is what stops the two surfaces disagreeing.
		if bool(entry.get("stalled", false)):
			rung_text = BADGE_UNSTAFFED_FORMAT % building_glyph
			rung_color = HudStyle.WARN
		else:
			rung_text = BADGE_BUILDING_FORMAT % [building_glyph,
				int(round(float(entry.get("building_progress", 0.0)) * HudConst.PROGRESS_PERCENT_SCALE))]
			rung_color = HudStyle.SIGNAL_DEEP
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
	_view.draw_rect(box, rung_color if rung_text != "" else HudStyle.LINE, false, BADGE_BORDER_WIDTH)
	var baseline := center + Vector2(-run.x * 0.5, run.y * 0.5 - font.get_descent(font_size))
	if rung_text != "":
		var rung_run: Vector2 = font.get_string_size(rung_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
		_view.draw_string(font, baseline, rung_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, rung_color)
		baseline.x += rung_run.x
	_view.draw_string(font, baseline, crew_text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, HudStyle.INK_DIM)

## One source's worked mark: the ring on its marker's slot, plus the tile-level outline underneath.
## `slot_of(key) == -1` means the marker did not draw at all (overflowed or far zoom), so only the
## outline renders — the mark degrades to the aggregate rather than landing somewhere arbitrary.
## `hex_center` is the source's ANCHOR HEX in screen space, passed in rather than derived from a
## column here for `_queue_source_badge`'s reason one function down: the food and hunt arms anchor to
## the BAND's wrap image (`eff_col + delta`, so a source across the seam draws beside the band that
## works it) while a working anchors to its own marker's (`_hex_center_wrapped`). A mark that resolved
## the wrap for itself would eventually disagree with the marker it rings.
func _draw_worked_mark(hex_center: Vector2, key: String, color: Color, selected: bool,
		radius: float) -> void:
	var outline := color
	outline.a = WORKED_TILE_OUTLINE_ALPHA
	_view._outline_hex_at(hex_center, radius, outline, WORKED_TILE_OUTLINE_WIDTH)
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return
	var center := _view.secondary_slot_center(hex_center, slot, radius)
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

## THE SELECTED BAND → SOURCE LINK, one routine for every kind of source (issue #650). `color` is the
## source's own RING colour and the alpha is applied here, so a hunted herd's link and a worked
## working's can differ only in the hue the ring already stated — never in weight or opacity.
##
## `target` is the source's own MARKER anchor, not its hex centre, wherever the caller has one: two
## workings on one hex are two markers in two edge slots, and links to the hex centre would land the
## pair on the same point and read as one.
##
## A link whose ends straddle the horizontal seam is dropped rather than drawn across the whole map.
func _draw_worked_link(band_center: Vector2, target: Vector2, color: Color) -> void:
	if _link_spans_seam(band_center, target):
		return
	var link := color
	link.a = WORKED_LINK_ALPHA
	_view.draw_line(band_center, target, link, WORKED_LINK_WIDTH)

## Are these two ends on opposite sides of the horizontal seam — i.e. would a line between them be a
## stripe across the whole map rather than a join? One test for every link this renderer draws.
func _link_spans_seam(a: Vector2, b: Vector2) -> bool:
	return absf(a.x - b.x) > _view.last_map_size.x * LINK_MAX_SPAN_FACTOR

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
		# **WHICH ACCOUNT'S ZERO THIS ROW'S LABEL MAY PRINT**, resolved once for every arm off the
		# SHARED seam (`SourceForecast.row_zero_account`) rather than per branch. A food row answers
		# `food` and nothing about the two webs changes; an `extract` row answers its own MATERIAL,
		# which is what stops a working that took nothing this turn reading `+0.00` in an account it
		# does not pay into. The tile card's deposit rows and the work row's second line are held to
		# the same answer by the same function, which is the whole reason it is not spelled here.
		var zero_account := SourceForecast.row_zero_account(entry, kind)
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
				var fcenter := _label_anchor(_view._hex_center(tcol, trow, radius, origin),
					_view.secondary_food_key(int(entry.get("target_x", -1)), trow), radius)
				var forage_overdraw := yield_label_overdraw(entry)
				# The FODDER component rides along for the one-slot rule in `_draw_yield_label`; a
				# forage patch normally pays food, so it changes nothing here — except on the patch
				# this exists for, a sown hay Field, which pays fodder alone.
				_queue_yield_label(fcenter, _entry_realized_yield(entry), forage_overdraw, radius,
					_entry_floor_glyph(entry), _entry_fodder(entry), _entry_materials(entry),
					zero_account)
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
			# **THE LINK DOCKS TO THE HERD'S OWN MARKER**, the same anchor its rate pill hangs off, so
			# the herd and a worked working state one relationship in one geometry (issue #650). It
			# ran to the HEX CENTRE until the workings arrived — which overshoots past the marker on
			# any hex whose herd sits in an edge slot, and draws two hunted herds on one hex as two
			# lines to a single point. `_label_anchor` falls back to that centre where there is no
			# marker to dock to, which is the case the old behaviour was always right for.
			var hanchor := _label_anchor(hc, _view.secondary_herd_key(String(entry.get("fauna_id", ""))), radius)
			# Link the band to the herd it is hunting.
			_draw_worked_link(band_center, hanchor, HUNT_WORKED_COLOR)
			# (The worked ring itself is drawn by `draw_worked_source_marks`, for EVERY player band.)
			# Depletable herd: HEADLINE the STEADY realized average (`realized_yield`), NOT the
			# kill-credit PULSE (`actual_yield` is 0 on a wait turn, a spike on a kill turn) — mirrors
			# the Band panel's hunt-headline rule in `SourceForecast.source_yield_readout` (which now reads
			# `realized_yield` for both hunt and forage), so the map label and the Band panel can never
			# disagree. Falls back to the old `sustainable_yield` if `realized_yield` is absent. The
			# overhunt ⚠ flag is the sim-answered `overdraws` bool (it answers the crew's own floor) —
			# NOT `actual > sustainable`, which false-positives on a kill turn when a banked animal spikes.
			if show_yields and (entry.has("realized_yield") or entry.has("sustainable_yield")):
				var overhunt := yield_label_overdraw(entry)
				var hunt_rate := float(entry["realized_yield"]) if entry.has("realized_yield") \
					else float(entry.get("sustainable_yield", 0.0))
				# NO FODDER ARGUMENT, and that is a decision rather than an omission (issue #449): no
				# animal is harvested for feed, so a hunt row's fodder is a structural zero and passing
				# it would only offer the label a fall-through it can never take. **THE MATERIALS ARE
				# PASSED, and they are the arm that closes the inedible quarry's `+0.00`** — a wolf's
				# steady food rate is honestly 0, and its pelts are the whole of what the hunt pays.
				_queue_yield_label(hanchor, hunt_rate, overhunt, radius, _entry_floor_glyph(entry),
					0.0, _entry_materials(entry), zero_account)
		elif kind == HudConst.LABOR_KIND_EXTRACT:
			# **4. THE WORKED WORKINGS' HALF OF WHAT SELECTION BUYS** (issue #650) — the link back to
			# the band's token and the rate pill, the two parts the ring pass cannot draw because
			# neither is a fact about the SOURCE alone. Through the same two routines the hunt arm
			# above goes through, so a worked wood and a hunted herd cannot drift apart in dash,
			# weight, plate or offset.
			var material := String(entry.get("material", "")).strip_edges()
			var wtile := Vector2i(int(entry.get("target_x", -1)), int(entry.get("target_y", -1)))
			var wkey := _view.secondary_working_key(wtile.x, wtile.y, material)
			# **THE `(tile, material)` JOIN IS ASKED ONCE, IN `compute_worked_workings`**, and this
			# arm reads its answer rather than restating it: a row pointing at a hex the `deposits`
			# section carries no such working on drew no marker and no ring, so a link and a pill
			# hanging in that empty air would be the phantom the join exists to refuse.
			if not _worked_workings.has(wkey):
				continue
			# The working's OWN wrap image and its OWN marker slot — two workings on one hex sit in
			# two edge slots, so anchoring to the hex centre would land both links and both pills on
			# one point.
			var wanchor := _label_anchor(_view._hex_center_wrapped(wtile.x, wtile.y, radius, origin),
				wkey, radius)
			_draw_worked_link(band_center, wanchor, EXTRACT_WORKED_COLOR)
			# **NO `has()` GATE, unlike the two food arms**, and it is the same asymmetry as the
			# fodder argument one branch up read the other way round: a working ALWAYS pays into its
			# material account — the material is half its identity — so there is always a rate to
			# state, and `zero_account` is what makes an empty take read `+0.00 wood` rather than
			# claim a food zero. Fodder is the structural zero here, as it is on a hunt row.
			# **THE MARK FORKS ON THE GROUND'S OWN RENEWAL RATE, NOT ON THE FLOOR ALONE** (issue
			# #650) — `HudDepositVocab.floor_mark`, this arc's one fork, asked of the working's own
			# row. A finite seam is offered no dial, so the floor on its row is a default nobody
			# chose, and `♻` over a quarry claims a renewal the rock cannot make.
			#
			# **AND THE MATERIAL DOES NOT NAME ITSELF WHERE ITS MARKER ALREADY DOES.** A slot of
			# `0..cap-1` means this working's marker DREW — and a working's marker IS its material's
			# mark (`SecondaryMarkerRenderer._working_renders` denies a slot to a material this
			# client has no glyph for) — so the pill hangs under a 🪵 or a 🪨 and the noun repeated
			# it. `-1` (LOD-suppressed, overflowed into the `+N` chip, or an unmarked material) is
			# the case where nothing else on the hex says which account this is, and there the noun
			# stays: the badge pass skips on the same test for the same reason.
			if show_yields:
				_queue_yield_label(wanchor, _entry_realized_yield(entry), yield_label_overdraw(entry),
					radius, HudDepositVocab.floor_mark(_working_row(wtile, material),
						_entry_floor(entry)),
					0.0, _entry_materials(entry), zero_account,
					_view.secondary_slot_of(wkey) >= 0)

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
				if not _link_spans_seam(band_center, hc):
					_draw_dashed_line(band_center, hc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)
			elif kind == HudConst.LABOR_KIND_EXTRACT:
				# **A JUST-ORDERED CREW ON A WORKING GETS THE SAME OPTIMISTIC PAIR** (issue #650). The
				# record has always carried `extract` rows — `HudBandLaborState.record_pending_assign`
				# takes the `material` half-key for exactly this row kind — and this pass read only
				# the two food kinds, so ordering a crew onto a wood or a rock was the one assign in
				# the client that drew NOTHING until the snapshot confirmed it.
				var wrow := int(a.get("y", -1))
				if wrow < 0 or wrow >= _view.grid_height:
					continue
				var wcol := eff_col + _view._wrapped_col_delta(band_col, int(a.get("x", -1)))
				# The pending link hangs off the working's marker where one exists and off the hex
				# centre where it does not — which is the ORDINARY case here, a working's marker
				# existing only once a CONFIRMED crew is on it (`compute_worked_workings`). That is
				# what `_label_anchor`'s fallback is, so no branch is written for it.
				var wc := _label_anchor(_view._hex_center(wcol, wrow, radius, origin),
					_view.secondary_working_key(int(a.get("x", -1)), wrow,
						String(a.get("material", "")).strip_edges()), radius)
				_draw_dashed_hex(wcol, wrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
				if not _link_spans_seam(band_center, wc):
					_draw_dashed_line(band_center, wc, link_color, LABOR_PENDING_WIDTH, LABOR_PENDING_DASH, LABOR_PENDING_GAP)
	var move_variant: Variant = pend.get("move", {})
	if move_variant is Dictionary and not (move_variant as Dictionary).is_empty():
		var mrow := int((move_variant as Dictionary).get("y", -1))
		if mrow >= 0 and mrow < _view.grid_height:
			var mcol := eff_col + _view._wrapped_col_delta(band_col, int((move_variant as Dictionary).get("x", -1)))
			var mc := _view._hex_center(mcol, mrow, radius, origin)
			_draw_dashed_hex(mcol, mrow, radius, origin, LABOR_PENDING_COLOR, LABOR_PENDING_WIDTH)
			if not _link_spans_seam(band_center, mc):
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
	var dest_color := Color(HudStyle.SIGNAL, TRAVEL_DEST_ALPHA)
	var line_color := Color(HudStyle.SIGNAL, TRAVEL_DEST_LINE_ALPHA)
	_view.draw_line(band_center, dest_center, line_color, TRAVEL_DEST_LINE_WIDTH)
	# Reticle marks the destination hex; no pulse (this is a steady, confirmed heading, unlike the
	# animated targeting reticle).
	_view._draw_reticle(dest_center, radius * TRAVEL_DEST_RETICLE_FACTOR, dest_color, 1.0)

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

## **THIS BAND'S `builders` POOL** (`docs/plan_standing_upkeep.md` §2.5) — an ordinary standing-role
## row of `labor_assignments`, like `scout`. It is what the source badge asks now that a verb names no
## crew: the same LOCAL-copy rule as the reader above, for the same reason.
func _builders_pool_of_marker(band: Dictionary) -> int:
	for entry_variant in _labor_assignments_of_marker(band):
		if entry_variant is Dictionary \
				and String((entry_variant as Dictionary).get("kind", "")).strip_edges().to_lower() \
					== LABOR_KIND_BUILDERS:
			return maxi(int((entry_variant as Dictionary).get("workers", 0)), 0)
	return 0

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

## Its MATERIAL twin, a VECTOR (arc #527 follow-up) — what this source actually credited to the band's
## `MaterialStore` this turn, per material. Same "no realized fallback" reasoning: it is a resolved
## take rather than a projection, and the sim seeds it empty pre-commit by design. It is what an
## INEDIBLE quarry pays, and the reason a hunted wolf pack stops reading `+0.00` on the map.
func _entry_materials(entry: Dictionary) -> Array:
	return SourceForecast.material_rows_of(entry)

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
	return FoodIcons.for_floor_zone(SourceForecast.floor_zone(_entry_floor(entry)))

## The assignment's own escapement floor. Split out because the WORKING arm needs the NUMBER rather
## than the mark — a working's mark forks on the ground's renewal rate as well as on the floor
## (`HudDepositVocab.floor_mark`) — and two readers spelling the same default is how the food webs'
## floor and the workings' floor would come to disagree about what an absent field means.
func _entry_floor(entry: Dictionary) -> float:
	return float(entry.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))

func _queue_yield_label(tile_center: Vector2, value: float, overhunt: bool, radius: float, floor_glyph: String = "",
		fodder: float = 0.0, materials: Array = [],
		zero_account: String = SourceForecast.YIELD_ACCOUNT_FOOD,
		marker_names_material: bool = false) -> void:
	_deferred_yield_labels.append({
		"tile_center": tile_center,
		"value": value,
		"overhunt": overhunt,
		"radius": radius,
		"floor_glyph": floor_glyph,
		"fodder": fodder,
		"materials": materials,
		"zero_account": zero_account,
		"marker_names_material": marker_names_material,
	})

## Render (and drain) the deferred yield-label batch. Called LAST in `_draw` — after the markers,
## rings, links, pending overlays and targeting — so nothing paints over the labels.
##
## ⛔ **ONE PILL PER SOURCE, AND TWO CROWDED PILLS ARE LIFTED APART RATHER THAN MERGED** (issue #650).
## Every label in this batch is already anchored to its own source's MARKER (`_label_anchor`) and
## drawn on its own plate — there has never been a grouping pass — but the plate has no border and
## every plate is the same ink, so two that OVERLAP ink one continuous dark shape. Ray read a wood
## and a rock as `+0.40 stone ♻  +0.30 wood ♻` on a single plate for exactly that reason: two
## workings sit in two EDGE SLOTS of one hex (or on two adjacent hexes), which puts their anchors
## about 1.2 hex radii apart in x and at the SAME y, while a plate stating a material ran wider than
## that. Two numbers on one plate have nothing saying which belongs to which marker.
##
## So the batch is PLACED as well as drawn: a pill whose inked footprint would intersect one already
## placed this frame is lifted straight UP by `YIELD_LABEL_STACK_STEP_FACTOR` plate heights, and
## re-tested. **The lift is vertical because the x is the association** — a pill sits directly over
## its own marker, so moving it sideways is the one direction that would break the thing the split is
## for.
##
## ⛔ **AND IT IS A LIFT RATHER THAN A CULL.** The band NAME PILL family answers crowding by dropping
## the later label outright (`BandMarkerRenderer._reserve_name_pills`), which is right for a name the
## player can read off the card instead; a rate is the whole of what selection buys on this source and
## there is nowhere else on the map to read it. Placement is in QUEUE order — snapshot order, the
## same rule the secondary slots fill in — so a pill does not flicker between rows frame to frame.
func flush_yield_labels() -> void:
	for badge in _deferred_source_badges:
		_draw_source_badge(badge)
	_deferred_source_badges.clear()
	var placed: Array[Rect2] = []
	for label in _deferred_yield_labels:
		_draw_yield_label(label, placed)
	_deferred_yield_labels.clear()

## Where this pill lands: its anchored centre, or as far above it as it takes to clear every pill
## already placed this frame. Bounded by the number placed — each pass clears at least the topmost
## rect it collided with — so a crowded frame terminates rather than looping.
func _lift_clear_of_placed(center: Vector2, half: Vector2, placed: Array[Rect2]) -> Vector2:
	var step := half.y * 2.0 * YIELD_LABEL_STACK_STEP_FACTOR
	var lifted := center
	for _attempt in range(placed.size() + 1):
		var blocked := false
		for taken in placed:
			if taken.intersects(Rect2(lifted - half, half * 2.0)):
				blocked = true
				break
		if not blocked:
			return lifted
		lifted.y -= step
	return lifted

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
## ONE ACCOUNT ONLY, and deliberately so (issues #337 / #449 / #527): a source pays a VECTOR — food,
## fodder and materials — but a map label sits on a hex a few pixels wide beside a floor mark and a ⚠,
## and there is no room for a second rate. It shows the one the source actually PAYS, in the wire's own
## order: food when there is food (every edible quarry and every forage patch), else the fodder rate
## spelled with the WORD (fodder has no glyph), else the MATERIALS, each naming itself. A sown hay
## Field therefore reads `+0.40 fodder ♻` and a hunted wolf pack `+0.22 hide ⇊`, rather than the
## `+0.00` that said either was worth nothing.
##
## ⛔ **`marker_names_material` DROPS THE MATERIAL'S NOUN, AND ONLY THE NOUN.** A worked WORKING's
## pill hangs over a marker that IS the material (🪵 / 🪨), so `+0.40 stone ♻` said the same
## thing twice in the one place on the map with no room to; it reads `+0.40 ♻`, the shape a worked
## patch beside it already had. The flag is the caller's answer about its own SURFACE, never a fact
## about the rows — the hunt arm passes nothing, a deer's marker saying nothing about `hide`. (A trade branch sat between food and fodder until arc
## #527 retired that account; the material vector is what replaced it, and it is a vector because a
## mammoth hide and a hare pelt are both `hide` and are not the same thing.)
func _draw_yield_label(label: Dictionary, placed: Array[Rect2]) -> void:
	var radius := float(label.get("radius", 0.0))
	var text := _yield_label_rate_text(float(label.get("value", 0.0)),
		float(label.get("fodder", 0.0)), label.get("materials", []),
		String(label.get("zero_account", SourceForecast.YIELD_ACCOUNT_FOOD)),
		bool(label.get("marker_names_material", false)))
	# **A SOURCE WITH NO ACCOUNT TO BE EMPTY IN STATES NO RATE** — `row_zero_account`'s own caller
	# contract, and the only way `_yield_label_rate_text` answers "". A plate drawn around it would be
	# an empty pill claiming a reading the source cannot make.
	if text == "":
		return
	var color := HudStyle.HEALTHY
	if bool(label.get("overhunt", false)):
		text += " " + YIELD_OVERHUNT_FLAG
		color = HudStyle.WARN
	var floor_glyph := String(label.get("floor_glyph", ""))
	if floor_glyph != "":
		text += " " + floor_glyph
	var font_size := clampi(int(radius * YIELD_LABEL_SIZE_FACTOR), YIELD_LABEL_MIN_FONT, YIELD_LABEL_MAX_FONT)
	var tile_center: Vector2 = label.get("tile_center", Vector2.ZERO)
	var label_center := tile_center + Vector2(0.0, -radius * YIELD_LABEL_OFFSET_FACTOR)
	# Dark rounded plate behind the text so the label pops on ANY terrain (bare text washed out on the
	# light tan biomes). Same pill chrome as the count badges, sized to the MEASURED text+glyph run.
	var font: Font = ThemeDB.fallback_font
	if font != null:
		var text_size: Vector2 = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
		var pad := font_size * YIELD_LABEL_PLATE_PAD_FACTOR
		# **THE INKED HALF-EXTENT, END CAPS INCLUDED — `MapView.pill_half_extent` and never a
		# re-derivation.** A plate reaches a further half-height left and right than the body it was
		# measured from, which is the term every "how wide is this label" calculation forgets and the
		# one that decides whether two pills are touching.
		var half := _view.pill_half_extent(text_size, pad, YIELD_LABEL_PLATE_NO_BORDER)
		label_center = _lift_clear_of_placed(label_center, half, placed)
		placed.append(Rect2(label_center - half, half * 2.0))
		_view._draw_pill_plate(label_center, text_size, pad, YIELD_LABEL_PLATE_BG)
	_view._draw_marker_glyph(label_center, text, font_size, color)

## THE ONE-SLOT CHOICE, on its own so it can be asserted: which of the accounts this label states, and
## how it is spelled. Split out of `_draw_yield_label` because a draw call renders to a canvas and a
## harness cannot read a glyph back off one — the fall-through order (food → fodder → materials) is
## the claim, and it needs somewhere to be asked. `YIELD_LABEL_COMPONENT_MIN` is the same threshold on
## the two scalars, so neither can be shown at a magnitude the other would have been hidden at; the
## material arm is gated by `signed_material_components` answering `""`, which is the HUD's own
## display floor and the same gate the work board's rate column tests.
##
## **THE MATERIAL ARM STATES EVERY MATERIAL.** Naming one of a vector picks a winner the sim does not
## name, and summing them is the retired trade axis under a new name. The plate sizes to the MEASURED
## run (`_draw_pill_plate`), so a two-material label is wide rather than clipped — which is a
## legibility question for `map_band_label_overlap`, not a reason to state less than the truth.
##
## ⛔ **`marker_names_material` IS SAFE ONLY BECAUSE A WORKING TAKES ONE MATERIAL** — its identity is
## the `(tile, material)` pair — so the un-named join states exactly one figure. Two un-named figures
## would be two numbers with nothing between them, which `SourceForecast.MATERIAL_UNNAMED` says at
## more length. It is threaded to BOTH material arms below (the rate and the account's zero), because
## a pill that dropped the noun off one and kept it on the other would name the account only on the
## turns the source produced nothing.
func _yield_label_rate_text(value: float, fodder: float, materials: Array = [],
		zero_account: String = SourceForecast.YIELD_ACCOUNT_FOOD,
		marker_names_material: bool = false) -> String:
	if absf(value) < YIELD_LABEL_COMPONENT_MIN and fodder >= YIELD_LABEL_COMPONENT_MIN:
		return SourceForecast.PICKER_FODDER_PRODUCT_FORMAT % _format_yield_signed(fodder)
	if absf(value) < YIELD_LABEL_COMPONENT_MIN and fodder < YIELD_LABEL_COMPONENT_MIN:
		var material_text := SourceForecast.signed_material_components(materials,
			SourceForecast.MATERIAL_UNNAMED if marker_names_material \
				else SourceForecast.MATERIAL_NAMED)
		if material_text != "":
			return material_text
		# **NOTHING TOOK, SO THE ZERO NAMES THE ACCOUNT THIS SOURCE PAYS INTO** (issue #650). Every
		# other arm above states a rate the take produced; this is the one place the label speaks
		# for a take that produced none, and a bare `+0.00` is a claim about FOOD — false on a
		# working, which pays a material and no food, and the third surface that fault has been
		# fixed on. `SourceForecast.row_zero_account` is the shared answer, and the material names
		# itself in the same idiom `signed_material_components` just declined to use.
		if zero_account == SourceForecast.YIELD_ACCOUNT_NONE:
			return ""
		if zero_account != SourceForecast.YIELD_ACCOUNT_FOOD:
			# **AND THE ZERO DROPS THE NOUN ON THE SAME CONDITION THE RATE DOES.** This arm exists
			# because a bare `+0.00` is a claim about FOOD — but that is only true where nothing else
			# says otherwise, and a pill hanging under a 🪵 is not such a place. The mark under the
			# figure names the account whether the take was 0.30 or nothing at all.
			if marker_names_material:
				return _format_yield_signed(value)
			return SourceForecast.PICKER_MATERIAL_PRODUCT_FORMAT % [
				_format_yield_signed(value), zero_account]
	return _format_yield_signed(value)

## **THE ⚠ THIS LABEL FLIES — `LaborAssignment.overdraws`, read and never derived.** Split out for
## `_yield_label_rate_text`'s reason one field over: the plate is drawn into MapView's canvas, so a
## harness has no way to read a glyph back off it, and *whether the map agrees with the tile card and
## the compose sheet about one source* is exactly the claim this arc exists to make. It is also the
## one place the key is spelled on this renderer, so the forage and hunt branches cannot come to read
## it differently. **STATIC** — it consults the entry and nothing else, which is what lets it be asked
## without standing a MapView up.
static func yield_label_overdraw(entry: Dictionary) -> bool:
	return bool(entry.get("overdraws", false))

## Signed, fixed-decimal food-rate string for the on-tile yield labels ("+0.48" / "-0.30"). Mirrors
## the HUD's `SourceForecast.format_signed`; actual yields are ≥0 but the sign keeps it explicit.
func _format_yield_signed(value: float) -> String:
	var magnitude := String.num(absf(value), YIELD_LABEL_DECIMALS).pad_decimals(YIELD_LABEL_DECIMALS)
	return ("+" if value >= 0.0 else "-") + magnitude
