class_name BandOverlayRenderer
extends RefCounted

## Renders the SELECTED-BAND / SELECTED-HERD overlay family for MapView: the three range
## borders + worked-forage fills + hunted-herd rings and links of the selected player band, the
## dashed-amber optimistic PENDING overlay, the travel-destination line + reticle, the selected
## herd's graze-range ring, the corralled herd's pen footprint, the deferred per-source badge
## batch and the SELECTED BAND'S SOURCE ROWS (`compute_source_rows`, the model behind the docked
## `BandSourceList`). Extracted from MapView (composition — MapView owns one and calls its four
## entry points during its _draw pass). Owns only this family's selection-derived state (the
## pushed `_labor_pending` map, the per-frame badge batch and the per-frame row model); every draw
## command plus the shared geometry/hex/glyph/pill primitives and the unit/herd/selection state
## stay on MapView and are reached through the `_view` back-ref. Behaviour — and every rendered
## pixel — is identical to the old inlined code: the move was verified by byte-diffing all 56
## `map_preview` frames (plus the `blend_probe` set) before and after, with zero differing frames.
##
## THE BADGE BATCH IS A TWO-PHASE CONTRACT and the whole lifecycle lives HERE:
##   1. `draw_worked_source_marks` CLEARS the batch and QUEUES one entry per worked source, which is
##      also the one place each source's BUILD STATE is resolved;
##   2. `flush_yield_labels` renders + drains it, and MapView must call it LAST in `_draw` — after
##      the markers, rings, links, pending overlays and targeting — because those layers used to
##      paint over the plates. (The name is older than its contents; see that function.)
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
# link and the per-source ROWS stay selection-only — N bands of links is spaghetti.
const FORAGE_WORKED_COLOR := Color(0.46, 0.96, 0.46, 0.95)
const HUNT_WORKED_COLOR := Color(0.92, 0.34, 0.30, 0.95)
# **AND A THIRD COLOUR FOR THE THIRD WEB** (issue #650): a worked WORKING wears the same ring, link
# and outline as a patch or a herd, in a hue that is in neither food web — quarried slate, one colour
# for both branches, because a felled wood and a quarried rock are the same statement about the same
# account. The COOL HUE is what keeps it out of either food web's language — materials are the cool
# account beside two warm-blooded webs — and it is the only cool mark this pass draws.
#
# ⛔ **BUT THE HUE ALONE WAS NOT ENOUGH, AND THE ARITHMETIC SAYS WHY.** It shipped at
# `Color(0.68, 0.74, 0.80)`, a pale slate whose luminance is **0.732** against the khaki terrain of
# `map_working_beside_herd` at **0.735** — three thousandths apart, i.e. NEAR-ISOLUMINANT with the
# ground, carrying its whole reading on a +0.12 blue excess. Beside it the hunted herd's salmon sits
# **0.274** from that terrain and the forage green 0.083, so both food webs differ from the ground in
# VALUE as well as hue and the working's mark did not. Reported from play as reading paler than the
# herd's, and visible in the parity frame: the deer's ring and tile outline read instantly, the log's
# outline almost not at all.
#
# **So it keeps the hue and takes the VALUE the other two have** — 0.523, i.e. 0.212 from that
# terrain, in the same league as the salmon, with the blue excess doubled to +0.24 on top. Darker
# also separates the ring from the light 🪵 sprite inside it. `_max_blue_excess`, the harness probe
# that exists because this mark is blended past its own ink, only gets a wider margin from this.
const EXTRACT_WORKED_COLOR := Color(0.42, 0.54, 0.66, 0.95)
# Ring radius as a factor of the hex radius. A secondary marker is drawn at SECONDARY_ICON_SIZE_FACTOR
# (0.55) of the hex, so the ring sits just outside its glyph — and deliberately INSIDE the food-harvest
# ring (MapView.FOOD_HARVEST_RING_FACTOR 0.42 measured from the same centre), which is a different
# statement about the same marker and has to read apart from this one.
const WORKED_RING_FACTOR := 0.34
const WORKED_RING_WIDTH_SELECTED := 3.0
const WORKED_RING_WIDTH_OTHER := 1.6
# Segments in the worked ring's circle, and in the BUILD ARC drawn over it — one number, because a
# 28-segment track under a 32-segment arc would show the arc riding slightly off the ring it fills.
const WORKED_RING_SEGMENTS := 28
# **THE BUILD METER IS AN ARC ON THE RING** (issue #650, Part 3). A source with a rung under way draws
# its ring at this fraction of its own alpha and then paints the progress over it, from 12 o'clock
# clockwise. **The faint full ring is what makes 5% readable** — a bright tick against a visible track,
# rather than a nearly-absent arc floating on nothing. A source building nothing is untouched by this.
const RING_TRACK_ALPHA_FACTOR := 0.35
# Where the arc starts: 12 o'clock, so the sweep reads like a clock face rather than from an arbitrary
# point on the rim. `draw_arc` measures angles from 3 o'clock going clockwise in screen space.
const RING_ARC_START_ANGLE := -PI * 0.5
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
# The building face is the VERB GLYPH ALONE. The chevron is deliberately ABSENT: `⌃` means "you could
# start this", and the work has started.
#
# ⛔ **THE PERCENT CAME OFF THE PLATE AND THE METER MOVED TO THE RING** (issue #650). `35%` never
# answered *when will it land* — 35% of a coppice and 35% of a cultivation are weeks apart — so the
# number that a player acts on is the TURN COUNT, and that is on the source list's build cell
# (`DetailFormat.build_countdown_value`, the tile card's own string). What a glance wants from the map
# is *roughly how far*, which is what the ring's arc says (`RING_TRACK_ALPHA_FACTOR` below): colour
# still says which web, sweep says how far, two independent variables on one mark.
const BADGE_BUILDING_FORMAT := "%s "
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
# ---- THE PER-SOURCE RATE: ITS VOCABULARY, NOW READ BY A ROW RATHER THAN A PILL -------------------
#
# ⛔ **THE ON-TILE PILL AND ITS WHOLE CHROME ARE GONE** (issue #650). Selecting a band put ~20
# floating elements over four hexes; a rate pill is ~90px and two markers in one hex's edge slots sit
# ~55px apart, so a rate could not live above its own marker — that is arithmetic, not tuning, and a
# collision/lift pass shipped and was reverted rather than kept. The rate is now a ROW in the docked
# `BandSourceList` beside the selected band, and `YIELD_LABEL_SIZE_FACTOR` / `_MIN_FONT` / `_MAX_FONT`
# / `_OFFSET_FACTOR` / `_PLATE_BG` / `_PLATE_PAD_FACTOR` went with the plate that used them.
#
# **WHAT SURVIVES IS THE VOCABULARY, and it is the ROW's producer now** — the same composition, the
# same one-slot fall-through, the same `⚠`, read by `compute_source_rows`. Decimals mirror Hud's
# `YIELD_DECIMALS` (separate script, so named here rather than shared).
const YIELD_LABEL_DECIMALS := 2
# Below this a component is absent, not zero — the map twin of `SourceForecast.FOOD_FLOW_MIN`, and the
# test that decides WHICH of a hunt's two products a one-slot rate shows (issue #337).
const YIELD_LABEL_COMPONENT_MIN := 0.001
const YIELD_OVERHUNT_FLAG := "⚠"
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
## Per-source BADGES, deferred because they annotate the map and would otherwise be painted over by
## the marker glyphs, rings and pending overlays drawn after this pass. Each also carries the ONE
## resolution of its source's build state — see `_queue_source_badge`.
var _deferred_source_badges: Array[Dictionary] = []
## Per-tile roll-up of the worked sources the marker cap HID this frame: `Vector2i → {worked, ready,
## warn}`. A cap that hides state silently reads as "nothing here", which is the very failure this
## feature exists to fix at a different scale — so the `+N` chip reports what it is covering.
var _hidden_source_state: Dictionary = {}
## THE WORKED WORKINGS this frame — `working_key → {tile, material, crew, selected}` — see
## `compute_worked_workings`, which is the one place it is built.
var _worked_workings: Dictionary = {}
## **THE CREW ON EACH SOURCE THIS FRAME** — `source key → int`, summed across every player band by
## `draw_worked_source_marks`, which is the one place it is built.
##
## ⛔ **PROMOTED TO A MEMBER SO THE ROW PASS CAN READ IT, AND FOR NO OTHER REASON** (issue #650). The
## source list's `⚒N` and the marker badge's `⚒N` are joined by a literal leader line on screen, so
## two producers of that one number is a disagreement the player can see in a single glance. The row
## reads this; it never re-walks the bands.
var _source_crew: Dictionary = {}
## THE SELECTED BAND'S SOURCE ROWS this frame — see `compute_source_rows`. Rebuilt (and emptied) by
## `draw_band_work_highlights`, read by `MapView` for the docked source list and its leader lines.
var _source_rows: Array[Dictionary] = []
## The selected band's token centre in MAP-LOCAL units this frame, `Vector2.ZERO` with nothing
## selected. Held because the source list docks BESIDE the band and MapView has no band walk of its
## own; resolved once, by the pass that already had to resolve it.
var _selected_band_center: Vector2 = Vector2.ZERO
## …and its TILE, for the one consumer that has to ask another renderer about the same band: the
## source list avoids the band's NAMEPLATE, which `BandMarkerRenderer` measures per tile.
## `NO_SELECTED_TILE` with nothing selected.
const NO_SELECTED_TILE := Vector2i(-1, -1)
var _selected_band_tile: Vector2i = NO_SELECTED_TILE
## The band's WHOLE income across every row, already composed — see `compute_source_rows`.
var _source_total_text: String = ""

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
	_deferred_source_badges.clear()
	_hidden_source_state.clear()
	_worked_workings.clear()
	_source_crew.clear()
	_source_rows.clear()
	_selected_band_center = Vector2.ZERO
	_selected_band_tile = NO_SELECTED_TILE
	_source_total_text = ""

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
		if not HudConst.is_player_unit(band):
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
				# **IS ANY CUTTER OVER-CUTTING IT** — the same OR across the bands summed above, and
				# it rides here for `selected`'s reason: the mark pass walks SOURCES and has no
				# entry in hand to read the flag off. The sim writes `overdraws` on an Extract row
				# now (it was a structural `false` when this walk was written), so the `+N` overflow
				# chip's warn state can be answered for a working the marker cap hid — which the two
				# food webs have always answered for their own hidden rows.
				"overdraws": bool(known.get("overdraws", false))
					or yield_label_overdraw(entry),
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
	# the ring docks to. A MEMBER (`_source_crew`), because the row pass reads this answer rather than
	# summing a second one of its own — see the member's own ⛔.
	var crew: Dictionary = _source_crew
	crew.clear()
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
		if not HudConst.is_player_unit(band):
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
						# **THE BADGE IS QUEUED BEFORE THE MARK IS DRAWN, at all four call sites**
						# (issue #650). The queue is where the BUILD STATE is resolved, and the ring
						# has to know it — a source with a rung under way draws its ring as a faint
						# TRACK with the progress arc over it. Resolving the state twice is the one
						# thing forbidden here, so the mark reads the queue's answer instead.
						_queue_source_badge(qcenter, qkey, LABOR_KIND_HUNT, qherd,
							SourceForecast.IMPROVEMENT_NONE, int(crew[qkey]), radius, origin,
							HUNT_WORKED_COLOR, selected, int(builders.get(qkey, 0)))
						_draw_worked_mark(qcenter, qkey, HUNT_WORKED_COLOR, selected, radius)
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
				_queue_source_badge(fcenter, fkey, LABOR_KIND_FORAGE,
					_view.forage_patch_lookup.get(Vector2i(tx, trow), {}),
					String(entry.get("improvement", "")), int(crew[fkey]), radius, origin,
					FORAGE_WORKED_COLOR, selected, int(builders[fkey]))
				_draw_worked_mark(fcenter, fkey, FORAGE_WORKED_COLOR, selected, radius)
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
				_queue_source_badge(hcenter, hkey, LABOR_KIND_HUNT, herd,
					String(entry.get("improvement", "")), int(crew[hkey]), radius, origin,
					HUNT_WORKED_COLOR, selected, int(builders[hkey]))
				_draw_worked_mark(hcenter, hkey, HUNT_WORKED_COLOR, selected, radius)
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
		# **THE PLATE IS THE SHARED `⚒N` SOURCE BADGE, not a shape of its own** — the tile card's
		# deposit row, the band badge and this plate are one spelling of one idea (`BADGE_CREW_GLYPH`
		# → `HudSelectionVocab.SOURCE_CREW_MARK`), so a hex whose marker said one number beside a card
		# row saying another is unwritable. It is queued with an EMPTY source deliberately: the rung
		# answers `_queue_source_badge` reaches for (`RungGates.rung_in_progress` /
		# `next_rung_ready`) are the FOOD webs' — they answer nothing for an `extract` kind — and a
		# working's ladder is declared from the Work board, so the plate states the crew and stops.
		# ⛔ **AND THE CREW LANDS IN `_source_crew`, WHICH IS WHAT THE ROW READS.** The two food webs
		# fill it as they walk their bands; this arm did not, so a working's row printed `⚒0` beside a
		# marker printing `⚒3` — the exact disagreement the leader line between them makes visible, and
		# the reason the class note forbids a second producer. `compute_worked_workings` has already
		# summed this crew across every band on the source, so it is ASSIGNED rather than accumulated.
		crew[key] = int(working.get("crew", 0))
		_queue_source_badge(wcenter, key,
			HudConst.LABOR_KIND_EXTRACT, {}, SourceForecast.IMPROVEMENT_NONE,
			int(crew[key]), radius, origin, EXTRACT_WORKED_COLOR,
			bool(working.get("selected", false)))
		_draw_worked_mark(wcenter, key, EXTRACT_WORKED_COLOR,
			bool(working.get("selected", false)), radius)
		_note_if_hidden(key, wtile, HudConst.LABOR_KIND_EXTRACT, {},
			SourceForecast.IMPROVEMENT_NONE, bool(working.get("overdraws", false)))

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

## Where a source's ANNOTATION lands: its MARKER's slot when it drew in one, the hex centre otherwise.
## It is the source-list row's leader-line target and the pending link's end — and it was the on-tile
## rate pill's anchor before those.
##
## THE HEX CENTRE ALONE WAS A CO-LOCATION BUG. Every pill used to anchor there for both webs, so two
## hunted herds on one hex drew two rates at the identical point, one exactly on top of the other — and
## a herd sharing a hex with a worked patch did the same. The annotations belong to different sources,
## so they hang off the sources; two leader lines converging on one point would read as one source in
## exactly the same way. The hex-centre fallback covers a source with no visible marker.
## `hex_center` arrives resolved for `_draw_worked_mark`'s reason — the wrap image is the caller's to
## pick, and an annotation must hang off the same copy of the hex its ring is drawn on.
func _label_anchor(hex_center: Vector2, key: String, radius: float) -> Vector2:
	var slot := _view.secondary_slot_of(key)
	if slot < 0:
		return hex_center
	return _view.secondary_slot_center(hex_center, slot, radius)

## Queue this source's badge for the deferred flush. A source can be reached by more than one band, so
## the LAST queue for a key wins and carries the running crew total — cheaper and simpler than a second
## aggregation pass, and correct because `crew[key]` is accumulated before this is called.
##
## **THE ENTRY IS RECORDED EVEN WHERE THE MARKER DID NOT DRAW** (`slot_of == -1`: overflowed past the
## visible cap, or LOD-suppressed at far zoom); it simply carries no `center`, and the flush skips it,
## so no PLATE is drawn somewhere arbitrary — what the chip hides is the chip's job to report.
##
## ⛔ **IT IS RECORDED BECAUSE THIS IS THE ONE RESOLUTION OF THE BUILD STATE** (issue #650). The
## plate, the ring's progress ARC and the source list's build cell are three faces of one answer, and
## the list is drawn at EVERY zoom — a queue that returned before resolving would leave a row with no
## countdown on exactly the frames where the map has no plate to read it off either.
## `builders` is this source's BUILD crew, summed across bands — see `_draw_source_badge` for why a
## rung under way has to know it.
## `hex_center` is the plate's ANCHOR HEX in screen space, passed in rather than derived from a
## column here: the food and hunt arms anchor to the BAND's wrap image (`eff_col + delta`, so a
## source across the seam draws beside the band that works it) while a working anchors to its own
## marker's (`_hex_center_wrapped`), and a badge that resolved the wrap for itself would eventually
## disagree with the marker it hangs under.
## `ring_color` / `selected` are the SOURCE's own worked-ring colour and weight, threaded on because
## the build ARC is drawn in the flush and must wear exactly the ring it fills — colour says which
## web, sweep says how far, and a second colour lookup would be free to disagree with the ring itself.
func _queue_source_badge(hex_center: Vector2, key: String, kind: String, source: Dictionary,
		improvement: String, crew: int, radius: float, origin: Vector2,
		ring_color: Color, selected: bool, builders: int = 0) -> void:
	var slot := _view.secondary_slot_of(key)
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
	var progress := float(building.get("progress", 0.0))
	# One entry per source key: a later band working the same source replaces the earlier queue rather
	# than stacking a second plate on the same marker.
	for i in range(_deferred_source_badges.size()):
		if String(_deferred_source_badges[i].get("key", "")) == key:
			_deferred_source_badges.remove_at(i)
			break
	_deferred_source_badges.append({
		"key": key,
		# `Vector2.ZERO` where the marker did not draw — read only behind the `slot >= 0` guard.
		"center": _view.secondary_slot_center(hex_center, slot, radius) if slot >= 0 else Vector2.ZERO,
		"slot": slot,
		"crew": crew, "radius": radius,
		"ring_color": ring_color,
		"selected": selected,
		"ready_glyph": String(ready.get("glyph", "")),
		"building_glyph": String(building.get("glyph", "")),
		"building_progress": progress,
		"stalled": stalled,
		# **THE TILE CARD'S OWN STRING, COMPOSED HERE AND NOWHERE ELSE** — `≈11 turns (60%)`, or
		# whichever sentinel the wire spells (blocked / holding / rotting / queued / no estimate).
		# `DetailFormat.build_countdown_value` answers every one of them, and the documented failure
		# mode in that file is a SECOND fork missing a newly-spelled sentinel — so the source list
		# reads this rather than composing a `▸ N turns` of its own. `""` where no rung is in flight.
		"build_text": DetailFormat.build_countdown_value(
			SourceForecast.build_turns_remaining(source, HudComposeVocab.BARE_FORECAST_PREFIX),
			builders, HudFormat.progress_percent(progress),
			SourceForecast.build_queue_position(source, HudComposeVocab.BARE_FORECAST_PREFIX))
			if not building.is_empty() else "",
	})

## THIS FRAME'S BADGE ENTRY FOR A SOURCE, `{}` when the mark pass queued none. The reader that lets
## the ring's ARC and the source list's build cell come off `_queue_source_badge`'s ONE resolution of
## the build state — see that function's ⛔.
func _badge_entry_for(key: String) -> Dictionary:
	for entry in _deferred_source_badges:
		if String(entry.get("key", "")) == key:
			return entry
	return {}

## The two keys `badge_rung` answers with — the plate's rung RUN and the ink it is drawn in.
const BADGE_RUNG_TEXT := "text"
const BADGE_RUNG_COLOR := "color"

## **THE RUNG HALF OF A SOURCE BADGE'S FACE — ONE PRODUCER, BECAUSE TWO SURFACES READ IT.**
## `""` where the source is merely worked; otherwise at most ONE of the two rung states, a verb being
## neither offered nor under way at once (`next_rung_ready` excludes the verb in flight).
##
## It is PUBLIC and split out of `_draw_source_badge` so a probe can read the face: a draw call
## renders into a canvas, and `map_preview` could otherwise only photograph a plate whose whole claim
## is which characters are on it (`harness-map-probes.md` -> `map_build_arc`). Composing it a second
## time in the harness would be a second producer of the very string under test.
##
## **A PERCENT ON A BUILD NOBODY IS STAFFING IMPLIES PROGRESS THAT IS NOT HAPPENING.** The unstaffed
## face drops the number entirely (see `BADGE_UNSTAFFED_FORMAT`); the plate still says WHICH rung is
## promised here, and stops saying it is being worked.
##
## **A LOSING METER TAKES THE SAME FACE, and it is the wire's verdict rather than a staffing guess**
## (`docs/plan_standing_upkeep.md` 4.6a). A percent that is falling is the same lie as a percent that
## is not moving — but a meter merely PARKED, with its keeping covered, keeps its number, because that
## number is honest and the state is a decision rather than a failure. Both halves are
## `SourceForecast.build_is_stalled`, resolved at QUEUE time; the work board's rung slot calls the
## same function, which is what stops the two surfaces disagreeing.
##
## **AND A CLIMBING BUILD SHOWS NO PERCENT EITHER — THE METER IS THE RING'S ARC NOW** (issue #650).
## See `BADGE_BUILDING_FORMAT`, and `_draw_build_arc` for what carries the number instead.
func badge_rung(entry: Dictionary) -> Dictionary:
	var building_glyph := String(entry.get("building_glyph", ""))
	if building_glyph != "":
		if bool(entry.get("stalled", false)):
			return {BADGE_RUNG_TEXT: BADGE_UNSTAFFED_FORMAT % building_glyph,
				BADGE_RUNG_COLOR: HudStyle.WARN}
		return {BADGE_RUNG_TEXT: BADGE_BUILDING_FORMAT % building_glyph,
			BADGE_RUNG_COLOR: HudStyle.SIGNAL_DEEP}
	var ready_glyph := String(entry.get("ready_glyph", ""))
	if ready_glyph != "":
		return {BADGE_RUNG_TEXT: "%s%s " % [BADGE_READY_CHEVRON, ready_glyph],
			BADGE_RUNG_COLOR: HudStyle.SIGNAL}
	return {BADGE_RUNG_TEXT: "", BADGE_RUNG_COLOR: HudStyle.SIGNAL}

## Render (and drain) the deferred badge batch — the crew count, and the ⌃ chevron when the source can
## climb. Drawn in `flush_yield_labels` alongside the build arcs, i.e. LAST in `_draw`.
func _draw_source_badge(entry: Dictionary) -> void:
	# The marker did not draw (overflowed past the visible cap, or LOD-suppressed) — the entry exists
	# for the ROW's sake (see `_queue_source_badge`), and there is nothing on the hex to dock a plate
	# under. The `+N` chip reports what the cap hid.
	if int(entry.get("slot", -1)) < 0:
		return
	var radius := float(entry.get("radius", 0.0))
	var crew := int(entry.get("crew", 0))
	if crew <= 0:
		return
	# MapView is a Node2D, so there is no theme to ask — `ThemeDB.fallback_font` is what every
	# other map-side text draw uses (the count pills, the band name plates).
	var font: Font = ThemeDB.fallback_font
	if font == null:
		return
	var font_size := int(clampf(radius * BADGE_FONT_SIZE_FACTOR, BADGE_FONT_SIZE_MIN, BADGE_FONT_SIZE_MAX))
	var crew_text := "%s%d" % [BADGE_CREW_GLYPH, crew]
	var rung: Dictionary = badge_rung(entry)
	var rung_text := String(rung[BADGE_RUNG_TEXT])
	var rung_color: Color = rung[BADGE_RUNG_COLOR]
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
	# **A SOURCE WITH A RUNG UNDER WAY DRAWS ITS RING AS A TRACK** — the progress arc is painted over
	# it in the badge flush, so nothing later can scribble on the meter. The build state is READ from
	# the badge entry, never resolved a second time (`_badge_entry_for`).
	if String(_badge_entry_for(key).get("building_glyph", "")) != "":
		ring_color.a *= RING_TRACK_ALPHA_FACTOR
	var width := WORKED_RING_WIDTH_SELECTED if selected else WORKED_RING_WIDTH_OTHER
	_view.draw_arc(center, ring_radius, 0, TAU, WORKED_RING_SEGMENTS, ring_color, width)

## THE LEADER LINE from a SOURCE LIST ROW to the source it names, one routine for every kind of
## source. `color` is the source's own RING colour and the alpha is applied here, so a hunted herd's
## link and a worked working's can differ only in the hue the ring already stated — never in weight or
## opacity.
##
## **IT USED TO RUN FROM THE BAND'S TOKEN, AND IT USED TO SKIP THE PLANT WEB** (issue #650). The rates
## left the map for a docked list, so the thing that needs joining to a hex is the ROW — and every
## row draws one now, forage included. The signature is unchanged: the caller passes the row's end
## where the band's centre used to go.
##
## `target` is the source's own MARKER anchor, not its hex centre, wherever the caller has one: two
## workings on one hex are two markers in two edge slots, and links to the hex centre would land the
## pair on the same point and read as one.
##
## A link whose ends straddle the horizontal seam is dropped rather than drawn across the whole map.
func _draw_worked_link(from_point: Vector2, target: Vector2, color: Color) -> void:
	if _link_spans_seam(from_point, target):
		return
	var link := color
	link.a = WORKED_LINK_ALPHA
	_view.draw_line(from_point, target, link, WORKED_LINK_WIDTH)

## The public seam `MapView` draws a row's leader line through — both ends in MAP-LOCAL units. It
## exists so the row pass does not have to reach a private, and so the seam test and the shared
## alpha/width stay this file's, exactly as they are for every other link it draws.
func draw_row_link(row_point: Vector2, target: Vector2, color: Color) -> void:
	_draw_worked_link(row_point, target, color)

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
##  - the SOURCE ROWS (`compute_source_rows`) — the model the docked `BandSourceList` renders and
##    the anchors its leader lines run to. The worked RINGS themselves belong to
##    `draw_worked_source_marks`, which runs for every player band whatever is selected.
## All cleared automatically when the band is deselected (selected_unit_id < 0 → early out).
func draw_band_work_highlights(radius: float, origin: Vector2) -> void:
	# Start every frame's row model empty (cleared BEFORE the early-outs, so a deselected band leaves
	# no stale rows for the source list to render and no stale anchors for its leader lines).
	_source_rows.clear()
	_selected_band_center = Vector2.ZERO
	_selected_band_tile = NO_SELECTED_TILE
	_source_total_text = ""
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

	# 2. THE SOURCE ROWS — the model behind the docked `BandSourceList` (issue #650), one entry per
	#    staffed source across all three webs. The rates left the map: a pill is ~90px and two markers
	#    in one hex's edge slots sit ~55px apart, so a rate could not live above its own marker.
	#
	#    **BUILT AT EVERY ZOOM.** The old pills carried a far-zoom LOD gate (`show_yields`) because a
	#    map-scale label is unreadable; the list is SCREEN-SPACE, so a fixed-size panel is exactly as
	#    readable at hex radius 12 as at 80 — the `BAND_NAME_PILL` property.
	#
	#    **THE LEADER LINES ARE NOT DRAWN HERE.** A link now runs from the ROW to the source's anchor
	#    rather than from the band's token, and the row's y is not known until the panel has been
	#    placed — so `MapView` draws them, in the SAME frame, immediately after placing it.
	_selected_band_center = band_center
	_selected_band_tile = Vector2i(band_col, band_row)
	_source_rows = compute_source_rows(radius, origin)

	# 3. Optimistic PENDING actions for this band (dashed amber): a just-issued assign/move that
	#    the snapshot hasn't confirmed yet. Drawn last so it reads on top of the confirmed styles.
	_draw_band_pending(band, band_col, band_row, eff_col, band_center, radius, origin)

	# 4. Travel destination: a confirmed in-progress move the snapshot reports (`is_traveling`).
	#    Line + reticle toward the wrapped-nearest copy of the target, so it follows the short
	#    (possibly seam-crossing) path the sim actually takes. Works for bands AND expeditions.
	_draw_travel_destination(band, band_col, band_row, eff_col, band_center, radius, origin)

## ---- THE SELECTED BAND'S SOURCE ROWS (issue #650) ---------------------------------------------

## THIS FRAME'S ROWS, for `MapView` to hand to the docked `BandSourceList`. Empty with nothing
## selected.
func source_rows() -> Array[Dictionary]:
	return _source_rows

## The selected band's token centre in MAP-LOCAL units this frame — where the list docks BESIDE.
func selected_band_center() -> Vector2:
	return _selected_band_center

## …and its RAW tile (the snapshot's own column/row, unwrapped), `NO_SELECTED_TILE` with nothing
## selected. `BandMarkerRenderer.name_pill_offset` is keyed by exactly that tile.
func selected_band_tile() -> Vector2i:
	return _selected_band_tile

## **THE BAND'S WHOLE INCOME ACROSS EVERY SOURCE**, already composed, unchanged as the player pages.
##
## ⛔ **IT IS NOT ONE SUMMED NUMBER.** Adding food to fodder to hide is the retired trade axis under a
## new name. `SourceForecast.yield_components` is the signed twin of the work-zone chips' own
## producer: it states every ACCOUNT and renders only the non-zero ones, so a band cutting wood and
## foraging reads `+1.20 /turn · +0.40 wood` rather than a meaningless `1.60`.
func source_total_text() -> String:
	return _source_total_text

## ---- the attention ladder ---------------------------------------------------------------------
##
## Three classes, each a SHIPPED predicate, ranked high-first; `ATTENTION_NONE` is an ordinary row.
## The sort puts them at the top of page 1, which is the whole point: **a row you would act on must
## never be the row that got cut.**
##
## ⛔ **THERE IS NO "IDLE CREW" CLASS.** It was named in the design and has no shipped per-source
## predicate: `idle_workers` is a BAND-level turn-orb row, and `HudDepositVocab.DEPOSIT_RUNWAY_IDLE`
## describes a working with NO crew — which by construction never appears in this list, every row
## having `workers > 0`. Do not invent one.
const ATTENTION_NONE := 0
## Short of keepers — the rung is slipping back down. `DetailFormat.rung_is_at_risk` on a food source,
## `HudDepositVocab.is_at_risk` on a working; both wear the tile card's own words.
const ATTENTION_UNDER_KEPT := 1
## Taking more than the source renews — the sim's own `overdraws` verdict. The RATE cell already
## carries the `⚠` and the WARN ink, so the row spells it once.
const ATTENTION_OVER_CUT := 2
## A declared build that is unstaffed or going backwards (`SourceForecast.build_is_stalled`, read off
## the badge entry). The BUILD cell already carries the sentinel's own face.
const ATTENTION_BUILD_STALLED := 3

## **ONE ROW PER STAFFED SOURCE, ACROSS ALL THREE WEBS** — the model behind the docked
## `BandSourceList`, and the anchors its leader lines run to.
##
## ⛔ **IT RE-DERIVES NOTHING THE MARK PASS ALREADY RESOLVED.** The crew comes off `_source_crew` and
## the build state off `_badge_entry_for`, both filled by `draw_worked_source_marks` — which MapView
## calls first. The row's `⚒N` and the marker badge's `⚒N` are joined by a literal leader line on
## screen, so two producers of one number is a disagreement a player sees in a single glance.
##
## The `anchor` is MAP-LOCAL and is exactly the point the retired rate pill hung over
## (`_label_anchor`): the source's own marker slot, falling back to the hex centre where no marker
## drew. Two workings on one hex are two markers in two edge slots, so anchoring to the hex centre
## would land both leader lines on one point and read as one source.
##
## **THE ROW BUILD IS ALSO WHERE THE FOOTER'S TOTAL IS ACCUMULATED** (`_source_total_text`), because
## this is the one walk that visits every one of the band's sources.
func compute_source_rows(radius: float, origin: Vector2) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	_source_total_text = ""
	if _view.selected_unit_id < 0:
		return rows
	var band := _selected_player_band()
	if band.is_empty():
		return rows
	var pos: Array = Array(band.get("pos", []))
	if pos.size() != 2:
		return rows
	var band_col := int(pos[0])
	var eff_col := _view._band_effective_col(band_col, radius, origin)
	var total_food := 0.0
	var total_fodder := 0.0
	# The per-source material vectors, merged by id at the end — never summed across ids.
	var material_sets: Array = []
	for entry_variant in _labor_assignments_of_marker(band):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if int(entry.get("workers", 0)) <= 0:
			continue
		var kind := String(entry.get("kind", "")).strip_edges().to_lower()
		# **WHICH ACCOUNT'S ZERO THIS ROW MAY PRINT**, resolved once for every arm off the SHARED seam
		# rather than per branch — the same answer the tile card's deposit rows and the work row's
		# second line are held to (`SourceForecast.row_zero_account`).
		var zero_account := SourceForecast.row_zero_account(entry, kind)
		var materials := _entry_materials(entry)
		var row := {}
		if kind == LABOR_KIND_FORAGE:
			var tx := int(entry.get("target_x", -1))
			var trow := int(entry.get("target_y", -1))
			# **THE COLUMN IS GUARDED WITH THE ROW, the shape both siblings use** — the hunt arm
			# below and `draw_worked_source_marks`' own forage arm. A row with `target_x == -1` is
			# keyed on tile `(-1, y)` and hangs its leader line off an anchor resolved from a
			# negative column: the row's CLICK is refused one layer out (`BandSourceList`), the LINE
			# is not, so the drop belongs here where the row is made.
			if tx < 0 or trow < 0 or trow >= _view.grid_height:
				continue
			# The two food arms keep their `has()` gates: a row the wire never described a take for
			# has no rate to state, exactly as the retired pill required.
			if not (entry.has("realized_yield") or entry.has("actual_yield")):
				continue
			var tile := Vector2i(tx, trow)
			var key := _view.secondary_food_key(tx, trow)
			var food := _entry_realized_yield(entry)
			var fodder := _entry_fodder(entry)
			total_food += food
			total_fodder += fodder
			material_sets.append(materials)
			var patch: Dictionary = _view.forage_patch_lookup.get(tile, {})
			row = _source_row(key, tile,
				_label_anchor(_view._hex_center(eff_col + _view._wrapped_col_delta(band_col, tx),
					trow, radius, origin), key, radius),
				# **THE ROW'S ICON IS THE MARKER'S OWN FACE**, through the one resolver, so the
				# leader line cannot run from a mushroom to a fish.
				SecondaryMarkerRenderer.face_for_food_site(_view.food_site_lookup.get(tile, {})),
				FORAGE_WORKED_COLOR, entry, food, fodder, materials, zero_account,
				# **THE TWO FOOD WEBS NEVER DROP A MATERIAL'S NOUN.** A deer's icon says nothing
				# about `hide`, so the noun is the only thing naming that account.
				MARKER_NAMES_NO_MATERIAL, _entry_floor_glyph(entry),
				_food_attention_text(patch, SourceForecast.SOURCE_KIND_FORAGE),
				food)
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
			if not (entry.has("realized_yield") or entry.has("sustainable_yield")):
				continue
			var key := _view.secondary_herd_key(herd_id)
			# HEADLINE the STEADY realized average, not the kill-credit PULSE (`actual_yield` is 0 on
			# a wait turn and a spike on a kill turn) — the Band panel's own hunt-headline rule, so
			# the row and the panel can never disagree.
			var hunt_rate := float(entry["realized_yield"]) if entry.has("realized_yield") \
				else float(entry.get("sustainable_yield", 0.0))
			total_food += hunt_rate
			material_sets.append(materials)
			row = _source_row(key, Vector2i(hx, hrow),
				_label_anchor(_view._hex_center(eff_col + _view._wrapped_col_delta(band_col, hx),
					hrow, radius, origin), key, radius),
				SecondaryMarkerRenderer.face_for_herd(herd),
				# NO FODDER, and that is a decision rather than an omission (issue #449): no animal
				# is harvested for feed, so a hunt row's fodder is a structural zero.
				HUNT_WORKED_COLOR, entry, hunt_rate, 0.0, materials, zero_account,
				MARKER_NAMES_NO_MATERIAL, _entry_floor_glyph(entry),
				_food_attention_text(herd, SourceForecast.SOURCE_KIND_HERD),
				hunt_rate)
		elif kind == HudConst.LABOR_KIND_EXTRACT:
			var material := String(entry.get("material", "")).strip_edges()
			var wtile := Vector2i(int(entry.get("target_x", -1)), int(entry.get("target_y", -1)))
			var key := _view.secondary_working_key(wtile.x, wtile.y, material)
			# **THE `(tile, material)` JOIN IS ASKED ONCE, IN `compute_worked_workings`**, and this
			# arm reads its answer: a row pointing at a hex the `deposits` section carries no such
			# working on drew no marker and no ring, so a row and a leader line into that empty air
			# would be the phantom the join exists to refuse.
			if not _worked_workings.has(key):
				continue
			# **NO `has()` GATE, unlike the two food arms** — a working ALWAYS pays into its material
			# account (the material is half its identity), so there is always a rate to state, and
			# `zero_account` is what makes an empty take read `+0.00 wood` rather than claim a food
			# zero.
			var food := _entry_realized_yield(entry)
			total_food += food
			material_sets.append(materials)
			var deposit := _working_row(wtile, material)
			var face := SecondaryMarkerRenderer.face_for_material(material)
			row = _source_row(key, wtile,
				_label_anchor(_view._hex_center_wrapped(wtile.x, wtile.y, radius, origin),
					key, radius),
				face, EXTRACT_WORKED_COLOR, entry, food, 0.0, materials, zero_account,
				# **THE ROW DROPS THE MATERIAL'S NOUN WHERE THE ROW ITSELF DRAWS THE MATERIAL'S
				# MARK.** `marker_names_material` is the caller's statement about its OWN surface,
				# re-answered here: the pill asked whether the HEX's marker drew, and a row asks
				# whether the ROW's icon does. A face with neither sprite nor glyph keeps the noun,
				# which is the only thing naming the account there.
				SecondaryMarkerRenderer.face_renders(face),
				# **THE MARK FORKS ON THE GROUND'S OWN RENEWAL RATE, NOT ON THE FLOOR ALONE** — a
				# finite seam is offered no dial, so `♻` over a quarry would claim a renewal the rock
				# cannot make.
				HudDepositVocab.floor_mark(deposit, _entry_floor(entry)),
				HudDepositVocab.hazard_clause(deposit),
				# **THE WORKING'S OWN MATERIAL RATE, which is what this row HEADLINES** — `food` is
				# a structural zero here (see the sort key's own note on `_source_row`).
				_entry_material_rate(entry, material))
		if row.is_empty():
			continue
		rows.append(row)
	# ATTENTION FIRST, THEN YIELD, THEN THE KEY — a STABLE, TOTAL order, so page 1 cannot reshuffle
	# between frames while the player is reading it.
	rows.sort_custom(_row_precedes)
	_source_total_text = SourceForecast.yield_components(total_food, total_fodder,
		SourceForecast.YIELD_ACCOUNT_FOOD, SourceForecast.merged_material_rows(material_sets))
	return rows

## One row, assembled from the parts its arm resolved. Everything common to the three webs is HERE —
## the rate composition, the overdraw flag, the crew, the build cell and the attention rank — so the
## arms differ only in the facts that genuinely differ between webs.
func _source_row(key: String, tile: Vector2i, anchor: Vector2, face: Dictionary, color: Color,
		entry: Dictionary, food: float, fodder: float, materials: Array, zero_account: String,
		names_material: bool, floor_glyph: String, attention_text: String,
		sort_yield: float) -> Dictionary:
	var badge := _badge_entry_for(key)
	var overdraw := yield_label_overdraw(entry)
	# Composed EXACTLY as the retired pill composed it, through the same function: the rate, then the
	# overdraw flag, then the floor mark. `""` where the source pays into no account at all —
	# `row_zero_account`'s own caller contract, and the row then states no rate.
	var rate_text := _yield_label_rate_text(food, fodder, materials, zero_account, names_material)
	if rate_text != "":
		if overdraw:
			rate_text += " " + YIELD_OVERHUNT_FLAG
		if floor_glyph != "":
			rate_text += " " + floor_glyph
	var attention := ATTENTION_NONE
	if bool(badge.get("stalled", false)):
		attention = ATTENTION_BUILD_STALLED
		# The build cell already carries the sentinel's own face; spelling it twice on one row wastes
		# the only column that can say anything else.
		attention_text = ""
	elif overdraw:
		attention = ATTENTION_OVER_CUT
		# The rate cell already carries the `⚠` and the WARN ink.
		attention_text = ""
	elif attention_text != "":
		attention = ATTENTION_UNDER_KEPT
	return {
		"key": key,
		"tile": tile,
		"anchor": anchor,
		"color": color,
		"sprite": face.get("sprite"),
		"glyph": String(face.get("glyph", "")),
		"crew": int(_source_crew.get(key, 0)),
		"rate_text": rate_text,
		"overdraw": overdraw,
		"build_text": String(badge.get("build_text", "")),
		"attention": attention,
		"attention_text": attention_text,
		# **THE SORT KEY IS THE FIGURE THE ROW HEADLINES, not a second reading of the entry.** It was
		# `_entry_realized_yield` — which is the headline on a forage row but NOT on a HUNT one,
		# where the headline falls back to `sustainable_yield` when the wire published no realized
		# average. A deer showing `+0.20` then sorted on its `actual_yield` of 0.46 and landed ABOVE
		# a patch showing `+0.27`: a list that states it is ordered by yield, printing its numbers
		# out of order. One number, read once, and the order is the one on screen — **which is why
		# the ARM supplies it** rather than this composer picking a field.
		#
		# ⛔ **AND A WORKING'S HEADLINE IS ITS MATERIAL RATE, NOT ITS FOOD.** `systems::labor`'s
		# `Extract` arm leaves `SourceYield::ZERO` on the row by design — a deposit must not pollute
		# `food_income` — so a working paying `+3.00 wood` sorted at zero, below every patch paying
		# `+0.01 /turn` and tied with every other working. **The key is therefore a PER-TURN RATE in
		# the row's own account, and it orders three webs that do not share a unit**: it says *how
		# much this source pays per turn as the row states it*, and NEVER that a unit of wood is
		# worth a unit of food. There is no conversion to make — the trade axis this client could
		# have asked one from is retired (arc #527) — so the honest reading of the order is *the
		# sources each web pays most from, interleaved*, which is what a single list of three webs
		# can mean at all.
		"sort_yield": sort_yield,
	}

## **IS THIS FOOD SOURCE SHORT OF KEEPERS, AND IN WHICH WEB'S WORDS** — `⚠ slipping` on the plant web,
## `⚠ drifting` on the animal one, `""` when its keeping is paid. The tile card's own pair
## (`DetailFormat.rung_under_kept_word`), so a row and the card cannot name one failure two ways.
##
## **THE RUNG ARGUMENT IS THE SOURCE'S OWN AT-RISK RUNG.** `rung_is_at_risk` is ROUTING plus a
## verdict — it decides which of a CARD's two rung rows may wear the mark — and a list row is the
## whole SOURCE rather than one of its rungs, so it asks about the rung the shortfall is actually
## about. `SourceForecast.at_risk_rung` is that answer and is the same one the card routes on.
##
## The prefix is BARE because the dicts in hand here are the map's own — a `forage_patches` row and a
## `herds` row — not the `patch_`-prefixed `tile_info` the compose sheets carry. `RungGates` reads
## these same dicts with the same prefix one function over.
func _food_attention_text(src: Dictionary, source_kind: String) -> String:
	if src.is_empty():
		return ""
	var prefix := HudComposeVocab.BARE_FORECAST_PREFIX
	if not DetailFormat.rung_is_at_risk(src, prefix, source_kind,
			SourceForecast.at_risk_rung(src, prefix, source_kind)):
		return ""
	# The glyph-then-state-word pair, in the one format the workings' own hazard clause already uses
	# — one spelling of "hazard mark, then what is happening to it".
	return HudDepositVocab.DEPOSIT_HAZARD_CLAUSE_FORMAT % [
		HudSelectionVocab.RUNG_HAZARD_GLYPH, DetailFormat.rung_under_kept_word(source_kind)]

## The row order: attention descending, then realized yield descending, then the KEY ascending. The
## key tie-break is what makes it TOTAL — two calm rows at identical yields would otherwise be free
## to swap places between frames, and the pager's page 1 must hold still while it is being read.
static func _row_precedes(a: Dictionary, b: Dictionary) -> bool:
	var a_rank := int(a.get("attention", ATTENTION_NONE))
	var b_rank := int(b.get("attention", ATTENTION_NONE))
	if a_rank != b_rank:
		return a_rank > b_rank
	var a_yield := float(a.get("sort_yield", 0.0))
	var b_yield := float(b.get("sort_yield", 0.0))
	if not is_equal_approx(a_yield, b_yield):
		return a_yield > b_yield
	return String(a.get("key", "")) < String(b.get("key", ""))

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
		if int(unit.get("entity", -1)) == _view.selected_unit_id and HudConst.is_player_unit(unit):
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

## The STEADY per-source rate a source row headlines: the assignment's `realized_yield` (the honest
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

## **WHAT ONE MATERIAL OF THAT VECTOR PAYS PER TURN** — the working's OWN account, picked out by id.
##
## ⛔ **NEVER A SUM ACROSS THE ROWS.** One materials/turn figure is the retired trade axis under a new
## name (`SourceForecast.signed_material_components`' own ⛔), so this reads the ONE id the row is
## about — a working takes one material by construction, that pair being its identity — and asks
## nothing about the others. `MATERIAL_RATE_NONE` where the vector carries no row for it, which is a
## crew that has cut nothing yet and is exactly what the row headlines there (`+0.00 wood`).
static func _entry_material_rate(entry: Dictionary, material: String) -> float:
	for row in SourceForecast.material_rows_of(entry):
		if String(row[SourceForecast.MATERIAL_PAYOFF_ID_KEY]) == material:
			return float(row[SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY])
	return MATERIAL_RATE_NONE

## Nothing has come out of this working yet — a measured zero, not a sentinel.
const MATERIAL_RATE_NONE := 0.0

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

## **THE BUILD METER, AS AN ARC ON THE SOURCE'S OWN RING** (issue #650, Part 3). Drawn in the badge
## flush — so it comes off the SAME resolution of the build state the plate does, and so nothing
## painted later can scribble across it — over the faint track `_draw_worked_mark` left behind.
##
## From 12 o'clock clockwise through `progress · TAU`, at the ring's own width and in the ring's own
## colour: **colour still says which web this is and sweep says how far along, two independent
## variables on one mark.** A STALLED build turns the arc `HudStyle.DANGER` — the plate's `⚠` says the
## same thing in the same frame, which is the point: one state, two faces, one resolution.
##
## ⛔ **THE PERCENT THIS REPLACED WAS ON THE PLATE AND ANSWERED THE WRONG QUESTION.** `35%` never said
## *when will it land* — 35% of a coppice and 35% of a cultivation are weeks apart — so the turn count
## moved to the source list's build cell, and what a glance wants from the map is *roughly how far*.
func _draw_build_arc(entry: Dictionary) -> void:
	if int(entry.get("slot", -1)) < 0:
		return
	if String(entry.get("building_glyph", "")) == "":
		return
	var progress := clampf(float(entry.get("building_progress", 0.0)), 0.0, 1.0)
	var radius := float(entry.get("radius", 0.0))
	var selected := bool(entry.get("selected", false))
	var arc_color: Color = entry.get("ring_color", HudStyle.SIGNAL)
	if not selected:
		arc_color.a *= WORKED_RING_OTHER_ALPHA
	if bool(entry.get("stalled", false)):
		arc_color = Color(HudStyle.DANGER, arc_color.a)
	_view.draw_arc(entry.get("center", Vector2.ZERO), radius * WORKED_RING_FACTOR,
		RING_ARC_START_ANGLE, RING_ARC_START_ANGLE + progress * TAU, WORKED_RING_SEGMENTS,
		arc_color, WORKED_RING_WIDTH_SELECTED if selected else WORKED_RING_WIDTH_OTHER)

## Render (and drain) the deferred SOURCE-BADGE batch — the `⚒N` plate under each worked marker, and
## the BUILD ARC on the ring above it. Called LAST in `_draw` — after the markers, rings, links,
## pending overlays and targeting — so nothing paints over either.
##
## **THE NAME OUTLIVED THE LABELS ON PURPOSE.** The per-source RATE PILLS this batch was built for are
## gone: they became the docked SOURCE LIST beside the selected band (`BandSourceList`, issue #650),
## because a ~90px pill cannot live over its own marker when two markers in one hex's edge slots sit
## ~55px apart — that is arithmetic, and a collision/lift pass shipped here and was reverted (Ray, on
## the live frame: *"having 1 way up there is worse then letting them overlapp a bit"*). What survives
## here is the half that always fitted: a count and a rung glyph. `MapView` reaches this seam by name,
## so the name stays.
##
## ⛔ **DO NOT RE-ADD A PILL COLLISION / LIFT / STAGGER PASS.** Nothing in this file places anything
## relative to anything else; every mark lands on its own source's marker and two that overlap are
## left to overlap.
##
## The ARC goes first and the PLATE second: the arc rides the ring (above the marker) and the plate
## hangs below it, so they never touch — the order is stated rather than incidental.
##
## **IT RENDERS AND DOES NOT DRAIN, which is `_source_rows`' lifecycle exactly.** The batch is cleared
## at the top of `draw_worked_source_marks` — the pass that refills it, and the first thing `MapView`
## calls every frame — so a clear here was redundant, and dropping it leaves THIS frame's badge
## entries readable AFTER the frame. That is what lets `map_preview` ask the plate what it says
## (`badge_rung`) rather than photograph it; a draw call renders into a canvas and a harness cannot
## read a glyph back off one.
func flush_yield_labels() -> void:
	for badge in _deferred_source_badges:
		_draw_build_arc(badge)
	for badge in _deferred_source_badges:
		_draw_source_badge(badge)

## THE ONE-SLOT CHOICE, on its own so it can be asserted: which of the accounts this label states, and
## how it is spelled. Split out of the retired on-tile pill because a draw call renders to a canvas and a
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
## **A FOOD WEB'S MARKER NEVER NAMES A MATERIAL** — the `marker_names_material` argument's answer for
## the forage and hunt arms, spelled once rather than as a bare `false` at two call sites.
##
## ⛔ **IT IS NOT `SourceForecast.MATERIAL_NAMED`, AND THE TWO BOOLEANS ARE OPPOSITES.**
## `MATERIAL_NAMED` answers *does this label WRITE the noun* (`signed_material_components`' own
## argument); `marker_names_material` answers *does the MARK already say it, so the label need not*.
## The food arms were handed `MATERIAL_NAMED` — `true` — under a comment saying the food webs never
## drop the noun, which is exactly what it made them do: a hunt paying only `hide` printed a bare
## `+0.22`, naming the account nowhere at all, because a deer's icon says nothing about hide.
const MARKER_NAMES_NO_MATERIAL := false

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
		# other arm above states a rate the take produced; this is the one place the row speaks
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

## Signed, fixed-decimal food-rate string for a source row's rate cell ("+0.48" / "-0.30"). Mirrors
## the HUD's `SourceForecast.format_signed`; actual yields are ≥0 but the sign keeps it explicit.
func _format_yield_signed(value: float) -> String:
	var magnitude := String.num(absf(value), YIELD_LABEL_DECIMALS).pad_decimals(YIELD_LABEL_DECIMALS)
	return ("+" if value >= 0.0 else "-") + magnitude
