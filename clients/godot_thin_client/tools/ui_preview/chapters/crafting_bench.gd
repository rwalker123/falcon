extends RefCounted

## MATERIALS & CRAFTING — the material rail, the bench and the kit ledger
## (`docs/plan_crafting_and_materials.md` §7).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## **THE FIXTURE IS THE PROTOTYPE'S OWN BAND**, and every string it carries is one the SIM would have
## resolved: the refusals, the life wordings, the grades and the craft names. That is the whole
## discipline this panel is built on — the client renders them verbatim — so a fixture that composed
## any of them here would be testing the harness's spelling rather than the panel's.
##
## It ends by CLOSING the panel and handing the reference band back, so a chapter appended after it
## starts where every other one does.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## `Main`'s reservation rules, borrowed rather than restated — see `_push_band_dock`. `Main` itself is
## never instanced here; only its `static` predicate is asked.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The band this chapter composes for. Its own entity, so it cannot be confused with the reference
## band the rest of the run uses.
const CRAFTING_BAND_ENTITY := 971

## The ladder each craft's `progress` is measured against — the sim's published
## `completion_threshold`, which is what stops the client inventing a scale of its own.
const CRAFT_THRESHOLD := 100.0

## **THE TIERS, AND THE RANKS THE HEADS ARE ORDERED BY.** The shipped roster has exactly one — `flint`
## at rank 0 — so every ledger here renders under a single `Flint` head. `bronze` at rank 1 exists
## only in the second-tier fixture, which is the one shape that can stage a head disagreeing with a
## cell and therefore the only one that can produce an `ownedNote` at all.
const TIER_FLINT := "flint"
const TIER_FLINT_RANK := 0
const TIER_BRONZE := "bronze"
const TIER_BRONZE_RANK := 1

## The heads as the ledger names them — the tier ids capitalized, which is how the panel keys its fold
## state and what the harness looks a head up by.
const HEAD_FLINT := "Flint"
const HEAD_BRONZE := "Bronze"

## **THE TIER WORDS THAT MAY NOT REACH AN OWNED CELL EXCEPT INSIDE `ownedNote`.** The head IS a tier
## word by design, so this claim is scoped to the CELLS; the second-tier fixture publishes a `tier_id`
## on every batch it owns, which is what stops the negative being vacuous.
const TIER_WORDS: Array[String] = [TIER_FLINT, TIER_BRONZE]

## The band's two-tier stock, batch by batch. Spears sit at two GRADES — and the `good` pair at
## different wear, which the cell must sum into one line rather than list twice — while the clubs are
## a single grade at the older tier, which is what the note is about.
const TWO_TIER_SPEARS_GOOD_A := 3
const TWO_TIER_SPEARS_GOOD_B := 2
const TWO_TIER_SPEARS_EXCELLENT := 1
const TWO_TIER_CLUBS_POOR := 4

## The sim's own resolved note for a band carrying the older tier — rendered VERBATIM, never composed.
const TWO_TIER_CLUBS_NOTE := "carrying flint · poor"

## Where each of the three crafts stands. Weaving is DONE (this band gathers), Tanning is climbing (it
## hunts deer) and Bone-working is stalled at 12% — which is not an error state but a band standing in
## country with no bone game. The land decides what you are good at.
## The crew weaving baskets on the running bench — named because the head-count claims below are
## arithmetic ABOUT it, and a literal repeated in three places is a claim nobody can re-derive.
const BENCH_CREW := 2

## The workforce of the bench-bound band: all but one worker is at the bench, which is the only shape
## in which "idle" and "how many could be at the bench" produce a visibly different stepper.
const BENCH_BOUND_WORKING_AGE := 3

## **THE PROGRESS LINE, SPELLED OUT RATHER THAN RECOMPOSED** through the vocab formats the panel
## builds it with — an expectation borrowed from the code under test can only agree with itself. It
## is the running fixture's own reading: 3.0 worker-turns of the 5 a pass costs, one basket already
## delivered, and the grade the pile in flight fixed.
const BENCH_PROGRESS_LINE := "3.0 of 5 worker-turns · 1 finished · this pile → good"

## **THE CHEAPEST GENUINE REFUSAL THERE IS**, and the sim's own wording for it
## (`core_sim/src/snapshot/crafting.rs`): the crew walked off. It is also the reason that SURVIVES the
## rule that nothing short stops a pile already drawn, so a fixture built on it stages a bench the sim
## would really publish stopped.
const BLOCKED_BENCH_CREW := 0
const BLOCKED_BENCH_REASON := "No one at the bench"

## The theme entry a Label's ink is read back out of. `get_theme_color` answers the override where one
## is set, which is how this HUD colours every label.
const FONT_COLOR_THEME_ITEM := "font_color"

const WEAVING_PROGRESS := 100.0
const TANNING_PROGRESS := 41.0
const BONE_WORKING_PROGRESS := 12.0

## **THE CONDITION WORDINGS THE WIRE STILL CARRIES AND THIS PANEL MUST NOT RENDER.** Every one of them
## is a `life` string the fixture below publishes, plus the head of the column that used to show them:
## how worn the gear is has ONE home, the Band panel's WORKFORCE role cards, and this table answers
## what a rebuild costs. The fixture keeps publishing them deliberately — a negative assertion over
## data that is not there proves nothing.
const RETIRED_CONDITION_WORDINGS: Array[String] = [
	"Life left", "Worn out", "Never made", "Untouched", "48 raids left", "~15 turns left",
	"~19 turns left", "~28 turns left", "~42 turns left", "~1 turn left",
]

## The two reservers the height-bound state stands up — a left COLUMN the width of the HUD's own left
## dock (the Inspector's edge) and a bottom STRIP the depth of a docked Band/City panel. Both axes at
## once, because a card bounded on one and not the other looks fixed from the wrong screenshot.
const RESERVER_LEFT := &"crafting_preview_left"
const RESERVER_BOTTOM := &"crafting_preview_bottom"
const RESERVED_LEFT_WIDTH := 360.0
const RESERVED_BOTTOM_HEIGHT := 360.0

## What the card measured at with nothing docked, captured in state 1. The reserved state's claim is
## that the card got SHORTER — without it, a bound that never bit would pass every rect test below.
var _unreserved_card_height: float = 0.0

# ---- state 5: the event bar the card was drawn THROUGH ------------------------------------------

## The notification bar is its own `CanvasLayer`, injected for this state and freed again — the
## `event_dock` chapter's idiom, and for the same reason: nothing else in the run may inherit it.
const EVENT_DOCK_SCENE := preload("res://src/ui/EventDockPanel.tscn")

## The bar's id in the HUD's OVERLAY registry. `Main` owns the real push and is never instanced here,
## so the chapter connects `occupancy_changed` straight to `Hud.set_overlay_inset` — the same hand
## wiring it already does for the reserved-edge fan-out. Kept equal to `Main.EVENT_DOCK_OVERLAY` so
## the harness and the client cannot be releasing different keys.
const EVENT_DOCK_OVERLAY := &"event_dock"

## The bar's own preferences, DECLARED rather than inherited: the dock persists its edge, row count,
## detail floor and channels, and the `event_dock` chapter walks all four before this one runs. A
## state that took whatever it left would render a different bar — and a different bar DEPTH — from
## one run's chapter order to the next.
const EVENT_BAR_ROWS := 2

## **THE CARD'S TOP EDGE BEFORE THE BAR APPEARED.** The vacuity guard for the whole state: unless the
## card was sitting where the bar is about to draw, "the card clears the bar" is a claim about two
## things that were never going to touch.
var _barless_card_top: float = 0.0

## Stand the card up in a room a docked panel has already shortened — the live configuration, since
## the launch button for this panel is on the Band/City dock — and then bring a TOP-docked event bar
## in over it.
##
## **THE BAR IS NOT A RESERVER AND MUST NOT BECOME ONE** (`event-dock.md`): it overlays live map by
## design, so nothing about the HUD's own layout may move for it. What it publishes is how deep it is
## DRAWN, and only the free-floating room shrinks by that. The reported defect is the other half of
## the same fact — the card is placed by arithmetic rather than by a container, so it is the one
## surface that is not simply drawn underneath, and the panel's title was rendered through the bar.
func _event_bar_state() -> void:
	# A short room, so the ledger fills it and the card's top edge IS the room's top edge. In a tall
	# window the card is centred with hundreds of pixels of slack above it and no bar can reach it —
	# which is why the collision was reported from play and not from this harness.
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, RESERVED_BOTTOM_HEIGHT)

	var bar: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	h.add_child(bar)
	await h.get_tree().process_frame
	bar.occupancy_changed.connect(
		func(edge: int, extent: float) -> void:
			h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, edge, extent))
	# Hidden first, so the card is fitted against a room the bar is not in yet and the move it makes
	# when the bar arrives is measurable.
	bar.set_suppressed(true)
	bar.set_dock(SIDE_TOP)
	bar.set_recent_count(EVENT_BAR_ROWS)
	bar.set_detail_level(HudEventVocab.DEFAULT_DETAIL_LEVEL)
	for channel in HudEventVocab.CHANNEL_ORDER:
		bar.set_channel_enabled(String(channel), true)
	bar.set_perpendicular_insets(h._hud.left_column_width(), h._hud.right_column_width())
	bar.ingest_events(_event_bar_fixture())
	# Seed the overlay by hand as well as connecting: a dock that was ALREADY hidden published nothing
	# above, and a state whose premise depends on a signal that may not have fired is not a state.
	h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, bar.get_dock(), bar.occupied_extent())

	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	_barless_card_top = panel.get_global_rect().position.y if panel != null else 0.0

	# **THE RE-FIT, not a re-open.** The bar is toggleable (`R`) and empty before anything is
	# selected, so it arrives and leaves under an already-open card; a room that only bit at opening
	# time would leave this frame exactly as broken as the reported one.
	bar.set_suppressed(false)
	await h._settle()
	_assert_card_clears_the_event_bar(bar)
	await h._save("crafting_panel_event_bar")

	h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, bar.get_dock(), 0.0)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, 0.0)
	bar.queue_free()

## The rows the reported frame carried: a discovery whose detail renders as `Settle site · (58, 34)`,
## plus enough beside it to FILL the bar at the default detail floor. Filling it is the point — the
## bar's depth is content-independent by construction, so an empty strip would prove the geometry
## while showing none of the frame the player complained about. Three rows for two slots because one
## of them is Routine and the default floor drops it; that is the bar behaving normally.
func _event_bar_fixture() -> Array:
	return [
		{"tick": 0, "kind": "site_discovered", "faction": 0, "label": "Verdant Basin",
			"detail": "category=settle_site at (58,34)", "seq": 9701},
		{"tick": 0, "kind": "came_of_age", "faction": 0, "label": "A child came of age in Band 1",
			"detail": "count=1", "seq": 9702},
		{"tick": 0, "kind": "tame", "faction": 0, "label": "The aurochs herd has grown tame",
			"detail": "", "seq": 9703},
	]

# ---- states 6-8: the room a real DOCKED PANEL leaves ---------------------------------------------

## The band panel's key in BOTH of the HUD's registries — `Main.BAND_PANEL_RESERVER`, restated as a
## const so the harness and the client cannot end up pushing and releasing different keys.
const BAND_PANEL_RESERVER := &"band_panel"

## The event dock's key in the OVERLAY registry, kept equal to `Main.EVENT_DOCK_OVERLAY` for the same
## reason. It is the second surface the co-edge state stands up.
const CO_EDGE_BAR_ROWS := 2

## Two rects computed from the same offsets can differ by a float ULP, so every edge comparison below
## carries the tolerance `event_dock`'s own co-edge block uses.
const DOCK_RECT_EPSILON := 0.5

## **A CARD SHRUNK TO NOTHING CLEARS EVERY PANEL THERE IS**, so the clearance claim is never made
## alone: beside it sits "and it is USING the room", i.e. the card's height is the room's to within
## this. `fit_to_content` clamps to exactly the room when the content overflows, so the tolerance is
## a float one and not a budget.
const ROOM_FILL_EPSILON := 0.5

## How far down the ledger the re-render state scrolls before it ticks the turn. Any offset a short
## room genuinely admits would do — what the claim needs is that it is not the TOP of the table, since
## the top is exactly where a lost offset lands.
const SCROLL_PROBE_OFFSET := 120

## What one turn moves on the ticked band: a pass of work on the bench, and the fibre that pass ate
## off the batch the rail lists first. Both are ordinary readings the panel prints, so the ledger
## re-renders with different numbers in it and the same rows.
const TICKED_BENCH_WORK := 1.0
const TICKED_FIBRE_SPENT := 2.0

## Fan the panel's reservation onto the HUD exactly as `Main._apply_reservation` does — BOTH
## registries, because on an edge the HUD does not yield, the strip is not a reservation at all: it is
## a surface that covers pixels without taking space, the event bar's case reached from the other
## side, and `FloatingRoom` is the only rect that must know.
##
## **BOTH HALVES ARE CALLED, NEITHER IS RESTATED**, which is what makes the states below a test of the
## client rather than of this file: `band_dock_overlays_hud` is the verdict and `push_hud_strip` is
## what the verdict means to the HUD, and both are `static` and node-free for exactly this. A harness
## spelling either one out is how one ends up green while testing a rule the client no longer has.
func _push_band_dock(panel: BandCityPanel, edge: int, size: float) -> void:
	MAIN_SCRIPT.push_hud_strip(h._hud, BAND_PANEL_RESERVER, edge, size,
		MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel))

## The card's own room: `FloatingRoom` with this panel's clearance applied, i.e. the rect
## `CraftingPanel._room()` answers. Restated from the HUD's node rather than read off the panel, so
## the claims below are made against the room the HUD published and not against the panel's opinion
## of it.
func _card_room() -> Rect2:
	return h._hud.floating_room.get_global_rect().grow(-HudCraftingVocab.VIEWPORT_MARGIN)

## **THE THREE DOCK CONFIGURATIONS.** Every one of them is a room that changed shape while the card
## was already open, which is the seam the reserved registry never reached: `set_overlay_inset` re-fit
## an open card and `set_reserved_inset` did not, so a card open while a panel docked, moved or
## collapsed stayed fitted to a room that no longer existed. Reported in play as the ledger sliced
## mid-row through `Wayfinding gear` by a horizontally-docked Band/City panel.
##
## The panel is a REAL `BandCityPanel`, never a literal depth: the reservation, the collapse and the
## HUD's yield verdict are all its own answers, and a literal would prove nothing about two rects
## actually clearing each other.
func _band_dock_states() -> void:
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		_push_band_dock(panel, edge, size))
	# The HUD's bottom chrome parks into the card's own rail on a BOTTOM dock, and `Main` wires that
	# as a SECOND listener on the same signal. It is load-bearing here rather than cosmetic:
	# `band_dock_overlays_hud` asks whether the chrome has left the strip before it lets the HUD keep
	# it, so a harness that never reflowed would be testing the yielding branch instead.
	h._hud.set_band_city_panel(panel)
	panel.reservation_changed.connect(Callable(h._hud, "reflow_dock_row"))
	panel.set_active_tab(BandCityPanel.ZONE_BAND)
	h._hud.update_band_alerts([_crafting_band()])
	panel.set_dock(SIDE_BOTTOM)
	_push_band_dock(panel, panel.get_dock(), panel.current_reservation_size())
	h._hud.reflow_dock_row(panel.get_dock(), panel.current_reservation_size())
	await h._settle()

	# **STATE 6 — THE PANEL DOCKED HORIZONTALLY, which the reservation registry cannot see.** The card
	# is opened AFTER the dock so this frame is the placement rather than the re-fit; states 7 and 8
	# move the room under it.
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	_assert_card_fits_the_band_dock(panel, "BOTTOM", true, true)
	await h._save("crafting_panel_band_dock_bottom")

	# **STATE 7 — THE BAR AND THE PANEL ON ONE EDGE.** `FloatingRoom`'s per-edge total is a MAXIMUM
	# where a reservation is a SUM, on the reasoning that both terms are absolute depths from the same
	# screen edge. On a shared edge that reasoning rests on something specific: the bar publishes
	# `_edge_offset + cross`, and `_edge_offset` is the sum of every reserver on its edge — the band
	# panel included, since it keeps its `_reservations` entry whether or not the HUD yields. So the
	# deeper term CONTAINS the shallower and the max loses nothing. Asserted as arithmetic AND as two
	# rects, because the arithmetic is the claim and the rects are what the player sees.
	var bar: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	h.add_child(bar)
	await h.get_tree().process_frame
	bar.occupancy_changed.connect(
		func(edge: int, extent: float) -> void:
			h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, edge, extent))
	bar.set_dock(SIDE_BOTTOM)
	bar.set_recent_count(CO_EDGE_BAR_ROWS)
	bar.set_detail_level(HudEventVocab.DEFAULT_DETAIL_LEVEL)
	for channel in HudEventVocab.CHANNEL_ORDER:
		bar.set_channel_enabled(String(channel), true)
	bar.set_perpendicular_insets(h._hud.left_column_width(), h._hud.right_column_width())
	bar.ingest_events(_event_bar_fixture())
	# The bar is the innermost thing on its edge by construction, so it is displaced past everything
	# reserving that edge — `Main._update_event_dock_edge_offset`'s sum, with no priority test.
	bar.set_edge_offset(panel.current_reservation_size())
	h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, bar.get_dock(), bar.occupied_extent())
	await h._settle()
	_assert_card_clears_both_surfaces(panel, bar)
	await h._save("crafting_panel_co_edge_bottom")
	h._hud.set_overlay_inset(EVENT_DOCK_OVERLAY, bar.get_dock(), 0.0)
	bar.queue_free()
	await h.get_tree().process_frame

	# **STATE 8 — COLLAPSED WHILE THE CARD IS OPEN.** A railed panel reserves 46 instead of ~360, so
	# the room GROWS under an open card — the direction a re-fit that only ever shrank would pass, and
	# the one a card left at its previous size fails by leaving a band of room unused.
	var open_card_height: float = _crafting_card_rect().size.y
	panel.set_collapsed(true)
	await h._settle()
	# A collapsed panel cannot pay for the HUD's exemption, so this configuration comes back through
	# the RESERVED registry — which is what makes it the state that exercises the other half of the
	# fix. It is also the room growing far enough for the whole ledger to fit, hence `false` for the
	# overflow: the card is its content's height here, not the room's.
	_assert_card_fits_the_band_dock(panel, "collapsed BOTTOM", false, false)
	h._assert_hud("crafting — collapsing the panel under an open card GIVES the card the room (%.0f → %.0f)"
			% [open_card_height, _crafting_card_rect().size.y],
		_crafting_card_rect().size.y > open_card_height)
	await h._save("crafting_panel_band_dock_collapsed")

	# **THE RESERVED HALF, ISOLATED — PNG-less.** Every move above changes BOTH registries at once, so
	# a re-fit driven by `set_overlay_inset` alone satisfies all of them and the `set_reserved_inset`
	# half could be reverted with nothing going red. This one touches no overlay at all: a second
	# reserver arriving on the bottom edge under the open card, which is the Inspector's or the
	# Workbench's ordinary behaviour, and the card must shorten for it.
	var railed_card_height: float = _crafting_card_rect().size.y
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, RESERVED_BOTTOM_HEIGHT)
	await h._settle()
	var reserved_card := _crafting_card_rect()
	h._assert_hud("crafting — a RESERVATION alone re-fits the open card (%.0f → %.0f for a %.0f strip)"
			% [railed_card_height, reserved_card.size.y, RESERVED_BOTTOM_HEIGHT],
		reserved_card.size.y < railed_card_height)
	h._assert_hud("crafting — …and it lands inside the room that reservation left (card bottom %.0f vs room bottom %.0f)"
			% [reserved_card.end.y, _card_room().end.y],
		reserved_card.end.y <= _card_room().end.y + DOCK_RECT_EPSILON)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, 0.0)
	await h._settle()

	# Hand the HUD back: the chrome home, both registries released, the panel gone.
	h._hud.close_crafting_panel()
	panel.set_collapsed(false)
	panel.reservation_changed.disconnect(Callable(h._hud, "reflow_dock_row"))
	h._hud.reflow_dock_row(SIDE_BOTTOM, 0.0)
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(BAND_PANEL_RESERVER, SIDE_BOTTOM, 0.0)
	h._hud.set_overlay_inset(BAND_PANEL_RESERVER, SIDE_BOTTOM, 0.0)
	panel.queue_free()
	await h.get_tree().process_frame
	await h._settle()

## The card's rect, or a zero one when the panel is not up — which fails every claim below honestly
## rather than passing on a card that never rendered.
func _crafting_card_rect() -> Rect2:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	return panel.get_global_rect() if panel != null else Rect2()

## **THE PAIR: it clears the panel AND it leaves no room unused.** Either half alone is satisfied by a
## card that is wrong in the other direction — a card grown through the strip clears nothing, and a
## card shrunk to nothing clears everything — so the fit is only established by both.
##
## **The second half is phrased against the SCROLL, not as "the card equals the room".** A card whose
## ledger already fits is SHORTER than its room for a perfectly good reason; what must never happen is
## room left over while rows are still hidden. `expect_overflow` is which of those two the caller is
## staging, asserted rather than assumed, so neither reading can go vacuous: with it `true` the fill
## claim bites, with it `false` the state is claiming the opposite and says so.
##
## `expect_overlay` is the same discipline one layer down. On a horizontal dock the HUD does NOT yield
## the strip, so nothing in `_reservations` bounds this card and only the OVERLAY registry can; a
## COLLAPSED one cannot pay for that exemption, falls back to insetting, and is therefore the
## configuration that exercises the reserved half of the same seam.
func _assert_card_fits_the_band_dock(panel: BandCityPanel, edge_name: String, expect_overlay: bool,
		expect_overflow: bool) -> void:
	var card := _crafting_card_rect()
	if card.size == Vector2.ZERO:
		h._assert_hud("crafting — the %s-dock panel is open" % edge_name, false)
		return
	var strip: Rect2 = panel._root.get_global_rect()
	var room := _card_room()
	var overflowing := _scroll_is_live(h._hud.crafting_panel().panel())
	h._assert_hud("crafting — precondition: the %s band dock %s the HUD" % [edge_name,
			"overlays" if expect_overlay else "insets"],
		MAIN_SCRIPT.band_dock_overlays_hud(panel.get_dock(), panel.current_reservation_size(),
			h._hud, panel) == expect_overlay)
	h._assert_hud("crafting — precondition: the %s-dock ledger %s its room" % [edge_name,
			"outgrows" if expect_overflow else "fits inside"],
		overflowing == expect_overflow)
	h._assert_hud("crafting — the card clears the %s band dock (card bottom %.0f vs strip top %.0f)"
			% [edge_name, card.end.y, strip.position.y],
		card.end.y <= strip.position.y + DOCK_RECT_EPSILON)
	h._assert_hud("crafting — …and leaves no room unused while rows are hidden (card %.0f of a %.0f room, scrolling %s)"
			% [card.size.y, room.size.y, overflowing],
		card.size.y >= room.size.y - ROOM_FILL_EPSILON or not overflowing)

## **BOTH SURFACES ON ONE EDGE.** The arithmetic claim first — the bar's published extent contains the
## panel's reserved depth, which is the whole reason `FloatingRoom` may take a MAXIMUM over the two
## rather than a sum — then the two rects the player would see it in.
func _assert_card_clears_both_surfaces(panel: BandCityPanel, bar: EventDockPanel) -> void:
	var card := _crafting_card_rect()
	if card.size == Vector2.ZERO or bar._root == null:
		h._assert_hud("crafting — the co-edge panel is open over a live bar", false)
		return
	var strip: Rect2 = panel._root.get_global_rect()
	var band: Rect2 = bar._root.get_global_rect()
	var reserved: float = panel.current_reservation_size()
	h._assert_hud("crafting — precondition: the bar and the panel share the BOTTOM edge and the bar is displaced past it (offset %.0f of %.0f reserved)"
			% [bar._edge_offset, reserved],
		bar.get_dock() == panel.get_dock() and bar._edge_offset >= reserved - DOCK_RECT_EPSILON)
	# THE MAX'S PREMISE, stated as the inequality it rests on: the deeper of the two terms contains
	# the shallower, so taking the maximum loses neither. Were the bar to publish its own height
	# alone, this fails and the card is drawn through whichever surface the max dropped.
	h._assert_hud("crafting — the bar's published extent contains the panel's strip (%.0f >= %.0f)"
			% [bar.occupied_extent(), reserved],
		bar.occupied_extent() >= reserved - DOCK_RECT_EPSILON)
	h._assert_hud("crafting — …so the room's bottom is the deeper of the two, not their sum (%.0f)"
			% h._hud._overlay_bottom,
		is_equal_approx(h._hud._overlay_bottom, bar.occupied_extent()))
	h._assert_hud("crafting — the card clears the co-edge BAR (card bottom %.0f vs bar top %.0f)"
			% [card.end.y, band.position.y],
		card.end.y <= band.position.y + DOCK_RECT_EPSILON)
	h._assert_hud("crafting — …and the panel behind it (card bottom %.0f vs strip top %.0f)"
			% [card.end.y, strip.position.y],
		card.end.y <= strip.position.y + DOCK_RECT_EPSILON)
	# …and the pair the clearance claims need: a card shrunk to nothing clears both surfaces, so the
	# room the two of them left must actually be full — which is only the right claim while rows are
	# still hidden, hence the overflow precondition beside it.
	h._assert_hud("crafting — precondition: the co-edge ledger outgrows the room the pair left",
		_scroll_is_live(h._hud.crafting_panel().panel()))
	h._assert_hud("crafting — …while still USING the room the pair left (card %.0f of a %.0f room)"
			% [card.size.y, _card_room().size.y],
		card.size.y >= _card_room().size.y - ROOM_FILL_EPSILON)

## **THE TURN TICK MAY NOT MOVE THE CARD, AND MAY NOT COST THE PLAYER HIS PLACE IN THE LEDGER** — the
## two halves of the reported *"the Materials & Crafting card shakes when I press Next Turn"*. It is
## PNG-less: both halves are about what happens BETWEEN two frames that look identical.
##
## The turn is ticked through `refresh_snapshot()`, the real per-snapshot seam `Hud` calls, over the
## same fixture — so anything that moves here moved for no reason at all.
##
## **THE RECT IS ASKED TWICE AND THE FIRST ASK IS THE ONE THAT CATCHES THE SHAKE.** `render` mounts its
## content and then AWAITS a whole frame before it can measure it (the content's height being a
## function of the card's width), so whatever it does to the card before that await is DRAWN. Asked
## only once the dust has settled, a card that snapped back to its nominal width, jumped to the top of
## the room and then put itself back reads as perfectly still — which is why the first reading is taken
## the instant `refresh_snapshot` returns, i.e. at `render`'s own await.
## **THE TWO HALVES NEED OPPOSITE FIXTURES, and staging them on one card is how the rect claim goes
## vacuous.** A ledger only scrolls when it did not fit, and a card that did not fit is fitted to the
## whole room — so it is ALREADY at the room's top edge and a park there moves it nowhere. The jump is
## only visible on a card SHORTER than its room, and that is a card whose table fits and has no scroll
## offset to lose. So: the rect on the bare band in an undocked room, the offset on the prototype's
## band in a room a reservation has shortened.
func _rerender_state() -> void:
	h._hud.update_band_alerts([_bare_band()])
	h._hud.open_crafting_panel(_bare_band())
	await h._settle()
	await _assert_the_re_render_moves_nothing()

	# The same short room the height bound stages: the offset half needs a ledger that genuinely
	# overflows, since a card whose table fits has no place in it for the player to lose.
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, RESERVED_LEFT_WIDTH)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, RESERVED_BOTTOM_HEIGHT)
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	await _assert_the_re_render_keeps_the_players_place()

	h._hud.close_crafting_panel()
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, 0.0)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, 0.0)
	await h._settle()

## **THE RECT IS ASKED TWICE AND THE FIRST ASK IS THE ONE THAT CATCHES THE SHAKE**, with the room the
## card is leaving unused as the vacuity guard: a card already filling its room cannot be seen to jump
## to the top of it, and a card whose width never grew past the nominal cannot be seen to snap back to
## it.
func _assert_the_re_render_moves_nothing() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the re-render panel is open", false)
		return
	var before := panel.get_global_rect()
	var room := _card_room()
	h._assert_hud("crafting — precondition: the card leaves room above it to jump to (card top %.0f vs room top %.0f)"
			% [before.position.y, room.position.y],
		before.position.y > room.position.y + DOCK_RECT_EPSILON)
	h._assert_hud("crafting — precondition: the card is wider than its nominal, so a snap back to it would show (%.0f vs %.0f)"
			% [before.size.x, HudCraftingVocab.PANEL_WIDTH],
		before.size.x > HudCraftingVocab.PANEL_WIDTH + DOCK_RECT_EPSILON)

	h._hud.crafting_panel().refresh_snapshot()
	# NOT settled: this is the card as the very next frame DRAWS it, mid-`render` at the await its
	# measurement needs, which is the only place the jump ever existed.
	var during := panel.get_global_rect()
	h._assert_hud("crafting — the re-render draws no frame at a different rect (was %s, drew %s)"
			% [before, during],
		during.is_equal_approx(before))
	await h._settle()
	h._assert_hud("crafting — …and it settles back at the same rect (was %s, settled %s)"
			% [before, panel.get_global_rect()],
		panel.get_global_rect().is_equal_approx(before))

## **A REBUILD MAY NOT COST THE PLAYER HIS PLACE.** The ledger is torn down and rebuilt on every
## snapshot, so a player scrolled down to the bench tools is thrown back to the top once a turn unless
## the offset is carried across it.
func _assert_the_re_render_keeps_the_players_place() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var scroll := _ledger_scroll(panel) if panel != null else null
	if panel == null or scroll == null:
		h._assert_hud("crafting — the re-render panel is open on a scrolling ledger", false)
		return
	# **THE TWO VACUITY GUARDS.** "The offset survived" passes trivially on a ledger that cannot scroll
	# and on one left at the top, so the state has to prove it staged neither.
	h._assert_hud("crafting — precondition: the re-render ledger outgrows its room",
		_scroll_is_live(panel))
	scroll.scroll_vertical = SCROLL_PROBE_OFFSET
	await h._settle()
	var scrolled := scroll.scroll_vertical
	h._assert_hud("crafting — precondition: the player is scrolled off the top of the ledger (%d)"
			% scrolled,
		scrolled > 0)

	var before := panel.get_global_rect()
	# A REAL tick: the same band with its bench a pass on and the fibre it spent gone. An identical
	# payload would leave the ledger nothing to rebuild differently, which is not what Next Turn does.
	h._hud.update_band_alerts([_ticked_crafting_band()])
	h._hud.crafting_panel().refresh_snapshot()
	await h._settle()
	h._assert_hud("crafting — the player's place in the ledger survives the turn tick (%d of %d)"
			% [scroll.scroll_vertical, scrolled],
		scroll.scroll_vertical == scrolled)
	# …and the card it scrolled inside is still where it was, which is the rect claim in the OTHER of
	# the two fit cases — a card fitted to the whole room rather than to its own content.
	h._assert_hud("crafting — …and the filled card is still where it was (was %s, settled %s)"
			% [before, panel.get_global_rect()],
		panel.get_global_rect().is_equal_approx(before))

func run(harness) -> void:
	h = harness
	await _crafting_states()

func _crafting_states() -> void:
	# The panel resolves its subject out of `player_bands()`, so the band goes through the same
	# per-snapshot ingest every other surface reads.
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.update_crafting_catalogues(_materials(), _characteristic_bands(), _recipes(),
		_craft_knowledge())

	# State 1 — the whole panel: the rail's three material groups with their craft tracks, a bench
	# weaving baskets with 2 crafters on it, and the ledger's three groups sorted worn-first.
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	_assert_panel_renders()
	await h._save("crafting_panel")

	# State 2 — the IDLE bench, which is a different statement from a blocked one, on a band that has
	# banked no materials at all. It is the first turn of a world, and the panel must say so rather
	# than render an empty grid.
	h._hud.update_band_alerts([_bare_band()])
	h._hud.open_crafting_panel(_bare_band())
	await h._settle()
	_assert_idle_bench()
	await h._save("crafting_bench_idle")

	# The two head counts, on a band whose bench holds all but one of its workers: the WORKFORCE zone
	# reports `1 idle of 3` with the crew on its own Bench segment, while the panel's crew stepper
	# still reaches all 3. The zone and the stepper in ONE frame is the whole point — they read the
	# same band and answer different questions, and only side by side is that visibly deliberate.
	h._hud.update_band_alerts([_bench_bound_band()])
	h._hud.open_crafting_panel(_bench_bound_band())
	h._hud.show_unit_selection(_bench_bound_band())
	await h._settle()
	_assert_bench_crew_is_not_idle()
	await h._save("crafting_bench_workforce")

	# State 4 — **THE HEIGHT BOUND.** A docked panel does not overlap the game, it RESERVES a strip of
	# one screen edge and every other surface lives in what is left; this card is free-floating and was
	# measuring itself against the whole window, so it grew straight through both the strip and
	# whatever overlays the edge it had just claimed. `Main` owns the reservation fan-out and is never
	# instanced here, so the reservations are pushed into `Hud.set_reserved_inset` by hand — the
	# `event_dock` chapter's idiom — and released again before the chapter hands the HUD back.
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, RESERVED_LEFT_WIDTH)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, RESERVED_BOTTOM_HEIGHT)
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	_assert_card_fits_the_reserved_room()
	await h._save("crafting_panel_reserved_edges")
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, 0.0)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, 0.0)

	await _event_bar_state()
	await _band_dock_states()
	await _rerender_state()
	await _two_tier_states()
	await _blocked_bench_state()
	await _map_gesture_state()

	# Hand everything back: the panel closed, the roster restored to the reference band.
	h._hud.close_crafting_panel()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await h._settle()

# ---- states 12-13: TWO TIERS, which is the only shape the readout can be judged on ---------------

## **THE HEAD IS WHAT A ROW WOULD BE MADE AT; THE CELL IS WHAT THE BAND HAS.** On the shipped one-tier
## roster the two can never disagree, so no state above can show the readout the Owned column exists
## for — a Clubs row under **Bronze** whose cell says *carrying flint · poor* — and no sim can publish
## an `ownedNote` there either. This fixture is the second tier, and everything downstream of the
## disagreement is asserted against it.
func _two_tier_states() -> void:
	h._hud.update_band_alerts([_two_tier_band()])
	h._hud.open_crafting_panel(_two_tier_band())
	await h._settle()
	_assert_owned_cell_reads_what_the_band_has()
	_assert_no_tier_word_reaches_a_cell()
	await h._save("crafting_panel_two_tiers")

	await _assert_folding_a_head_hides_only_its_own_rows()

	h._hud.close_crafting_panel()
	await h._settle()

## **THE GRADE LINES AND THE NOTE, EACH AS A PAIR.** A one-sided claim passes on a panel that lost the
## thing entirely: "two lines" is satisfied by a cell that lists every batch, so the single-grade row
## is asserted beside it; and "the note is rendered" is satisfied by a panel printing it on every row,
## so the row WITHOUT one is asserted beside that.
func _assert_owned_cell_reads_what_the_band_has() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the two-tier panel is open", false)
		return
	# Two grades ⇒ two lines, best first, with the two `good` batches summed into one — wear is what
	# separates them and wear is not this panel's fact.
	var spears := _owned_cell_texts(panel, "Spears")
	h._assert_hud("crafting — an item owned at two grades renders a line each (%s)" % [spears],
		spears.has(HudCraftingVocab.OWNED_COUNT_FORMAT % TWO_TIER_SPEARS_EXCELLENT)
			and spears.has("excellent")
			and spears.has(HudCraftingVocab.OWNED_COUNT_FORMAT
				% (TWO_TIER_SPEARS_GOOD_A + TWO_TIER_SPEARS_GOOD_B))
			and spears.has("good"))
	# …and the single-grade row renders exactly ONE, which is what says the cell groups by grade rather
	# than listing every batch it was handed.
	var clubs := _owned_cell_texts(panel, "Clubs")
	h._assert_hud("crafting — …while a single-grade item renders exactly one (%s)" % [clubs],
		_count_matching(clubs, HudCraftingVocab.OWNED_COUNT_FORMAT % TWO_TIER_CLUBS_POOR) == 1
			and _count_starting_with(clubs, "×") == 1)
	# **THE NOTE, VERBATIM AND ALONE, on the row whose stock disagrees with its head.** Asked as
	# "everything in the cell that is not a count or a grade", rather than as `has(the note)`: a panel
	# COMPOSING a note of its own — the defect the published field exists to prevent — satisfies a
	# `has` on the row that has one, and satisfies "…and does not carry the OTHER row's note" too.
	h._assert_hud("crafting — the owned note is rendered verbatim on the row that has one (%s)"
			% [_non_grade_texts(clubs)],
		_non_grade_texts(clubs) == [TWO_TIER_CLUBS_NOTE])
	# …and the row the sim published no note for carries NOTHING beside its grade lines, which is the
	# half a panel printing a note on every row fails.
	h._assert_hud("crafting — …and the row that has none carries nothing beside its grades (%s)"
			% [_non_grade_texts(spears)],
		_non_grade_texts(spears).is_empty())

## **NO TIER WORD REACHES A CELL EXCEPT THROUGH `ownedNote`.** Scoped to the Owned CELLS rather than to
## the ledger, because the group head is a tier word by design and a panel-wide scan cannot tell the
## two apart. Non-vacuous by construction: every batch this band owns publishes a `tier_id`, so there
## is a tier word sitting one field away from every cell asserted about.
func _assert_no_tier_word_reaches_a_cell() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the tier-word panel is open", false)
		return
	var notes := _published_owned_notes()
	var cells := _all_owned_cell_texts(panel)
	h._assert_hud("crafting — precondition: the fixture publishes a tier id on every batch it owns",
		_batches_carrying_a_tier(_two_tier_equipment_batches()) > 0 and not cells.is_empty())
	var leaked: Array = []
	for text_variant in cells:
		var text := String(text_variant)
		if notes.has(text):
			continue
		for word in TIER_WORDS:
			if text.to_lower().contains(word):
				leaked.append(text)
	h._assert_hud("crafting — no tier word reaches an Owned cell except through the owned note (%s)"
			% [leaked],
		leaked.is_empty())

## **FOLDING A HEAD HIDES ITS OWN ROWS AND NOTHING ELSE, AND THE HEAD STAYS.** Both halves, and the
## reverse toggle: a panel that hid the whole table satisfies the first alone, and one that never
## restored the rows satisfies the pair on the way down.
func _assert_folding_a_head_hides_only_its_own_rows() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var head := _group_head(panel, HEAD_BRONZE) if panel != null else null
	if panel == null or head == null:
		h._assert_hud("crafting — the two-tier panel offers a %s head" % HEAD_BRONZE, false)
		return
	h._assert_hud("crafting — precondition: both tier groups render their rows open",
		_label_texts(panel).has("Clubs") and _label_texts(panel).has("Traps"))

	head.pressed.emit()
	await h._settle()
	var folded := _label_texts(panel)
	h._assert_hud("crafting — folding a head hides ITS rows (%s under %s)" % ["Clubs", HEAD_BRONZE],
		not folded.has("Clubs") and not folded.has("Spears"))
	h._assert_hud("crafting — …while another group's rows stay visible (%s under %s)"
			% ["Traps", HEAD_FLINT],
		folded.has("Traps"))
	h._assert_hud("crafting — …and the folded head itself remains, dimmed and carrying its caret",
		folded.has(_head_face(HEAD_BRONZE, true)))
	await h._save("crafting_panel_group_folded")

	var reopen := _group_head(panel, HEAD_BRONZE)
	if reopen == null:
		h._assert_hud("crafting — the folded head is still pressable", false)
		return
	reopen.pressed.emit()
	await h._settle()
	var reopened := _label_texts(panel)
	h._assert_hud("crafting — unfolding it brings its rows back",
		reopened.has("Clubs") and reopened.has("Spears") and reopened.has("Traps"))

# ---- state 14: THE BENCH THAT IS STOPPED ---------------------------------------------------------

## **HOW FAR ALONG THE JOB IS AND WHY IT IS NOT MOVING ARE TWO FACTS, AND THE WELL OWES BOTH.** The
## refusal used to be written OVER the progress line, so a bench stopped for a real reason lost the
## reading that says whether clearing the block recovers a nearly-finished item or a barely-started
## one — which is the question a stopped bench actually raises. This is state 1's own bench with its
## crew walked off, so the two frames differ by the reason and nothing else.
func _blocked_bench_state() -> void:
	h._hud.update_band_alerts([_blocked_bench_band()])
	h._hud.open_crafting_panel(_blocked_bench_band())
	await h._settle()
	_assert_a_blocked_bench_still_says_how_far_along_it_is()
	await h._save("crafting_bench_blocked")

# ---- assertions ---------------------------------------------------------------------------------

## **THE CLAIMS NO PICTURE CAN CARRY.** A ledger sorted the wrong way, a refusal re-derived into
## "cannot craft", or a tier chip that moved with the life bar all render as perfectly plausible
## frames; what separates them is the ORDER of the rows and the exact strings in them.
func _assert_panel_renders() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	h._assert_hud("crafting — the panel is open", panel != null and panel.is_open())
	if panel == null:
		return
	var texts := _label_texts(panel)
	# The refusal is rendered VERBATIM, with its number. "Short 4.9 bone", never "cannot craft".
	h._assert_hud("crafting — a refusal names its number",
		texts.has("Short 4.9 bone") and not texts.has("cannot craft"))
	# …and the shrug reads differently from the shortage, which is the whole reason the reason is
	# published rather than derived from `available`.
	h._assert_hud("crafting — the shrug is its own string", texts.has("Not needed yet"))
	# **OWNERSHIP IS STATED AND CONDITION IS NOT — and the two claims are a PAIR.** The condition
	# readout has one home (the Band panel's role cards), so no `life` wording may reach this table;
	# but a table that had also stopped saying whether the band OWNS the thing would satisfy that
	# negative on its own, and the Owned cell is what has to carry the ownership reading alone.
	#
	# **THE TWO CONSEQUENCE WORDINGS ARE THEMSELVES A PAIR**, since a panel printing one of them on
	# every row satisfies either half alone: a kit the band is without is bare hands, a tool it never
	# built is not made, and only both together say the cell is keyed off the published `group`.
	h._assert_hud("crafting — owning none reads Bare hands on a kit AND Not made on a tool",
		texts.has(HudCraftingVocab.OWNED_KIT_NONE) and texts.has(HudCraftingVocab.OWNED_TOOL_NONE))
	h._assert_hud("crafting — no condition wording reaches the ledger",
		_missing_from(texts, RETIRED_CONDITION_WORDINGS) == RETIRED_CONDITION_WORDINGS.size())
	# The ledger is FOUR columns — Item · Owned · Rebuild costs · action — and the action head is blank
	# by design, so the three named ones are what the claim can name.
	h._assert_hud("crafting — the ledger heads its four columns",
		texts.has(HudCraftingVocab.LEDGER_COLUMN_ITEM.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_OWNED.to_upper())
			and texts.has(HudCraftingVocab.LEDGER_COLUMN_COST.to_upper()))
	# **TIER IS A HEAD, NOT A COLUMN** — one `Flint` head over every kit row on the shipped roster,
	# with the two other groups joining it as the same kind of head. All three carry the open caret.
	h._assert_hud("crafting — the three group heads read as one foldable family",
		texts.has(_head_face(HEAD_FLINT, false))
			and texts.has(_head_face(String(HudCraftingVocab.GROUP_HEADS[HudCraftingVocab.GROUP_TOOL]), false))
			and texts.has(_head_face(String(HudCraftingVocab.GROUP_HEADS[HudCraftingVocab.GROUP_STOCK]), false)))
	# The running row's button is SPENT — one job at a time, so it has nothing left to ask for.
	h._assert_hud("crafting — the running row reads On the bench",
		texts.has(HudCraftingVocab.ON_BENCH_LABEL))
	# Sorted by urgency: the worn-out kit leads its group and the untouched one trails it.
	var worn := _index_of(texts, "Wayfinding gear")
	var untouched := _index_of(texts, "Traps")
	h._assert_hud("crafting — worn first, untouched last",
		worn >= 0 and untouched > worn)
	# The rail carries a craft track per material group, at the sim's own spelling of the craft.
	h._assert_hud("crafting — the rail carries its craft tracks",
		_has_prefix(texts, "▰▰▰▰▰ Weaving") and _has_prefix(texts, "▰▰▱▱▱ Tanning"))
	# **THE SHRUG IS DIMMED AND NOTHING ELSE IS** — asserted as a PAIR, because a panel that dimmed
	# every neutral row would satisfy the first half alone and take a sled at 42 turns left down with
	# it. That over-dimming is exactly what shipped in the first cut of this panel.
	var untouched_alpha := _row_alpha(panel, "Traps")
	var used_alpha := _row_alpha(panel, "Sled")
	h._assert_hud("crafting — the untouched row is dimmed and the used one is not",
		untouched_alpha >= 0.0 and untouched_alpha < 1.0 and is_equal_approx(used_alpha, 1.0))
	# **THE UNBLOCKED HALF OF THE BENCH PAIR** (state 14 is the other): this band's bench is running,
	# so the well states its progress and carries no refusal line under it at all — no empty label, no
	# reserved gap. A one-sided claim on the blocked frame alone would pass on a panel that had simply
	# grown a permanent third line.
	h._assert_hud("crafting — a running bench says how far along it is and nothing beneath it",
		_label_with_text(panel, BENCH_PROGRESS_LINE) != null and _blocked_line(panel) == null)
	# The reading state 4 measures its own against: nothing is docked here, so this is the tallest the
	# card ever wants to be.
	_unreserved_card_height = panel.size.y

## **THE PAIRING, ON ONE FRAME.** A blocked bench owes the reason AND the progress, and either claim
## alone passes on a well that lost the other: asserting the refusal alone is exactly what the
## overwrite this state exists for would have satisfied, and asserting the progress alone is satisfied
## by a panel that never renders a refusal. Both are read off the LABEL rather than off a text scan —
## the reason is a sim string this chapter must not compose, and the danger ink is worn by every
## refused row in the ledger below, so only the meta the panel stamps can name the bench's own line.
func _assert_a_blocked_bench_still_says_how_far_along_it_is() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the blocked-bench panel is open", false)
		return
	var reason := _blocked_line(panel)
	h._assert_hud("crafting — a stopped bench states its reason verbatim, in the danger ink (%s)"
		% [reason.text if reason != null else "no line at all"],
		reason != null and reason.text == BLOCKED_BENCH_REASON
			and reason.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.DANGER)
	var progress := _label_with_text(panel, BENCH_PROGRESS_LINE)
	h._assert_hud("crafting — …AND still says how much of the job is banked (%s)"
		% BENCH_PROGRESS_LINE,
		progress != null and progress.get_theme_color(FONT_COLOR_THEME_ITEM) == HudStyle.INK_DIM)

## **THE CARD IS BOUNDED BY THE ROOM, NOT BY THE WINDOW.** `LayoutRoot` is the node the reserved-edge
## registry insets, so it IS the room the map and the rest of the HUD are drawn in; a card that fits
## inside it cannot be drawn over a docked panel. Asserted on the RECT rather than on the height
## alone, because the placement and the fit are two different pieces of arithmetic and either can
## strand the card outside a room the other sized it for.
##
## **The "it got shorter" claim is the vacuity guard.** Every rect test here passes trivially on a
## fixture whose ledger already fitted, so the fixture has to be one the bound actually bites: the
## same band as state 1, against a room 360px shorter.
func _assert_card_fits_the_reserved_room() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the reserved-room panel is open", false)
		return
	var room: Rect2 = h._hud.layout_root.get_global_rect()
	var card: Rect2 = panel.get_global_rect()
	h._assert_hud("crafting — the card is shorter once an edge is reserved",
		_unreserved_card_height > 0.0 and card.size.y < _unreserved_card_height)
	h._assert_hud("crafting — the card's top clears the reserved room's top",
		card.position.y >= room.position.y)
	h._assert_hud("crafting — the card's bottom clears the reserved room's bottom",
		card.end.y <= room.end.y)
	h._assert_hud("crafting — the card sits inside the reserved room horizontally",
		card.position.x >= room.position.x and card.end.x <= room.end.x)
	# The scroll is what pays for the shorter card: the ledger did not fit, so it scrolls INTERNALLY
	# rather than the card growing past the room. Without this the bound could be "fits" by dropping
	# rows.
	h._assert_hud("crafting — the ledger scrolls inside the bounded card",
		_scroll_is_live(panel))

## **THE REPORTED COLLISION, JUDGED AS TWO RECTS.** The bar is a `CanvasLayer` above the HUD's, so a
## card drawn into its band is not merely adjacent to it — the card's header is UNDER it, which is
## exactly what a screenshot shows and what no reading of the card's own size can. Asked of the
## global rects for that reason.
##
## **Three claims, and the first two are what stop the third being decorative.** A card centred in a
## tall room clears a top bar for free, so the state proves the collision was live (`_barless_card_top`
## sat inside the bar's band) and that the two share a horizontal band at all, before claiming the
## card now clears it. Remove the room's overlay inset and the third fails while the first two stay
## green — which is the shape a vacuity guard has to have.
func _assert_card_clears_the_event_bar(bar: EventDockPanel) -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null or bar._root == null:
		h._assert_hud("crafting — the event-bar panel is open over a live bar", false)
		return
	var card: Rect2 = panel.get_global_rect()
	var strip: Rect2 = bar._root.get_global_rect()
	h._assert_hud("crafting — the bar draws where the card WAS (card top %.0f vs bar %.0f..%.0f)"
			% [_barless_card_top, strip.position.y, strip.end.y],
		_barless_card_top > 0.0 and _barless_card_top < strip.end.y)
	h._assert_hud("crafting — the card and the bar share a horizontal band (card %.0f..%.0f vs bar %.0f..%.0f)"
			% [card.position.x, card.end.x, strip.position.x, strip.end.x],
		card.position.x < strip.end.x and strip.position.x < card.end.x)
	h._assert_hud("crafting — the card's top clears the event bar's bottom (card top %.0f vs bar bottom %.0f)"
			% [card.position.y, strip.end.y],
		card.position.y >= strip.end.y)

## Whether the panel's own `ScrollContainer` is scrolling — `fit_to_content` turns it on exactly when
## the content did not fit the room it was given.
func _scroll_is_live(node: Node) -> bool:
	var scroll := _ledger_scroll(node)
	return scroll != null and scroll.vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED

## The panel's one `ScrollContainer`, found by walking rather than read off the panel's member, so the
## two questions asked of it — is it scrolling, and where is the player in it — resolve to the same
## node by the same route.
func _ledger_scroll(node: Node) -> ScrollContainer:
	if node is ScrollContainer:
		return node as ScrollContainer
	for child in node.get_children():
		var found := _ledger_scroll(child)
		if found != null:
			return found
	return null

## How many of `needles` are absent from `texts`. A COUNT rather than a bool so a failure says how
## many wordings leaked rather than only that one did.
func _missing_from(texts: Array, needles: Array[String]) -> int:
	var missing := 0
	for needle in needles:
		if not texts.has(needle):
			missing += 1
	return missing

func _assert_idle_bench() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the idle panel is open", false)
		return
	var texts := _label_texts(panel)
	h._assert_hud("crafting — an idle bench says so",
		texts.has(HudCraftingVocab.BENCH_IDLE_TITLE))
	h._assert_hud("crafting — an empty rail says so",
		texts.has(HudCraftingVocab.RAIL_EMPTY))

## **IDLE AND BENCHABLE ARE TWO DIFFERENT NUMBERS, AND THE PANEL READS THE SECOND.** A worker at the
## bench is assigned labor, so `effective_idle` nets the crew out exactly as the sim's
## `BandWorkforce::idle()` does — but re-crewing does not have to free those hands first, so the
## stepper's ceiling is `idle + the crew already there` (`benchable()`).
##
## **THE PAIR IS THE CLAIM, and the fixture is what makes it discriminating**: on a band with three
## working-age people and two of them at the bench, idle is 1 and benchable is 3, so a panel handed
## `effective_idle` would cap the stepper AT the crew standing on it and grey the `+` — which is a
## perfectly plausible-looking frame. The rendered half is asserted for that reason; the arithmetic
## half is what says the subtraction happened at all.
func _assert_bench_crew_is_not_idle() -> void:
	var band := _bench_bound_band()
	var labor: HudBandLaborState = h._hud._band_labor
	h._assert_hud("crafting — the bench crew is not idle",
		labor.effective_idle(band) == BENCH_BOUND_WORKING_AGE - BENCH_CREW)
	h._assert_hud("crafting — the crew stepper's ceiling keeps the crew already at the bench",
		labor.benchable_workers(band) == BENCH_BOUND_WORKING_AGE)
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	if panel == null:
		h._assert_hud("crafting — the bench-bound panel is open", false)
		return
	h._assert_hud("crafting — `+` is live at a crew the band's idle count alone could not reach",
		not _crew_button_disabled(panel, HudCraftingVocab.BENCH_CREW_INCREMENT))
	# **THE ZONE SAYS IT TOO, and its two claims are a PAIR.** The head is what the player reads as
	# "hands I can spend", and the segment is where the hands it stopped counting went — a head that
	# nets the bench out over a bar that never names it drops the crew off a chart whose segments are
	# supposed to add up to the same workforce.
	var hud_texts := _label_texts(h._hud)
	h._assert_hud("crafting — the WORKFORCE head does not count the bench crew as idle",
		hud_texts.has(HudWorkVocab.WORKFORCE_IDLE_FORMAT
			% [BENCH_BOUND_WORKING_AGE - BENCH_CREW, BENCH_BOUND_WORKING_AGE]))
	h._assert_hud("crafting — …and the bar names the crew it stopped counting",
		hud_texts.has("%s %d" % [HudWorkVocab.WORKFORCE_KEY_BENCH, BENCH_CREW]))

## The `disabled` state of the crew stepper's `−`/`+`, found by its face — the two are the only
## buttons in the panel wearing those single glyphs. `true` when no such button was found, which
## fails the live claim honestly rather than passing on a stepper that never rendered.
func _crew_button_disabled(node: Node, face: String) -> bool:
	if node is Button and (node as Button).text == face:
		return (node as Button).disabled
	for child in node.get_children():
		if not _crew_button_disabled(child, face):
			return false
	return true

## The bench's refusal line, found by the meta the panel stamps on it rather than by its face — the
## reason is a string the SIM resolved and this chapter may not predict, and a search for the danger
## ink would find every refused row in the ledger below just as readily. `null` when the well renders
## none, which is what the running bench's half of the pair asks for.
func _blocked_line(node: Node) -> Label:
	if node is Label and node.has_meta(HudCraftingVocab.BENCH_BLOCKED_META):
		return node as Label
	for child in node.get_children():
		var found := _blocked_line(child)
		if found != null:
			return found
	return null

## The Label carrying an exact face, so a claim can be made about its INK as well as its presence.
func _label_with_text(node: Node, face: String) -> Label:
	if node is Label and (node as Label).text == face:
		return node as Label
	for child in node.get_children():
		var found := _label_with_text(child, face)
		if found != null:
			return found
	return null

func _label_texts(node: Node) -> Array:
	var texts: Array = []
	if node is Label:
		texts.append((node as Label).text)
	if node is Button:
		texts.append((node as Button).text)
	for child in node.get_children():
		texts.append_array(_label_texts(child))
	return texts

func _index_of(texts: Array, needle: String) -> int:
	return texts.find(needle)

## The `modulate.a` of the ledger row naming `item_name` — how the dimming is applied, so it is what
## the claim has to read. `1.0` when no such row is found, which fails the dimmed half honestly.
##
## **IT LOOKS FOR THE INNERMOST MATCHING `HBoxContainer`, and that is not fussiness.** The zones row
## is an `HBoxContainer` too and every ledger row is a descendant of it, so a walk that took the first
## match from the top answered the ZONES row's alpha — a flat `1.0` — for every item in the table, and
## the dimming claim failed against a frame that was rendering it correctly.
func _row_alpha(node: Node, item_name: String) -> float:
	for child in node.get_children():
		var found := _row_alpha(child, item_name)
		if found >= 0.0:
			return found
	if node is HBoxContainer and _label_texts(node).has(item_name):
		return (node as HBoxContainer).modulate.a
	return -1.0

## A head's rendered face — the caret the fold state picks, then the name, uppercased exactly as the
## panel builds it. Composed through the panel's own vocabulary rather than typed out, so the claim
## cannot pass on a head that renders a different caret from the one the panel means.
func _head_face(head_name: String, folded: bool) -> String:
	return (HudCraftingVocab.GROUP_HEAD_FORMAT % [
		HudCraftingVocab.GROUP_HEAD_CARET_FOLDED if folded else HudCraftingVocab.GROUP_HEAD_CARET_OPEN,
		head_name]).to_upper()

## A group head, by IDENTITY — its `GROUP_HEAD_META`, never its face. A face carries the caret, which
## is exactly the thing the fold assertions are about.
func _group_head(node: Node, head_name: String) -> Button:
	if node is Button and (node as Button).get_meta(HudCraftingVocab.GROUP_HEAD_META, "") == head_name:
		return node as Button
	for child in node.get_children():
		var found := _group_head(child, head_name)
		if found != null:
			return found
	return null

## The ledger row naming `item_name` — the INNERMOST matching `HBoxContainer`, the same walk
## `_row_alpha` needs and for the same reason: the zones row is an `HBoxContainer` too, and so is every
## grade line inside an Owned cell, so a match taken from the top answers about the wrong node.
func _ledger_row(node: Node, item_name: String) -> Node:
	for child in node.get_children():
		var found := _ledger_row(child, item_name)
		if found != null:
			return found
	if node is HBoxContainer and _label_texts(node).has(item_name):
		return node
	return null

## Every text inside ONE row's Owned cell, reached through the cell's own meta. An empty array when
## the row or its cell was not found, which fails a positive claim honestly.
func _owned_cell_texts(panel: Node, item_name: String) -> Array:
	var row := _ledger_row(panel, item_name)
	if row == null:
		return []
	var cell := _owned_cell(row)
	return _label_texts(cell) if cell != null else []

func _owned_cell(node: Node) -> Node:
	if node.has_meta(HudCraftingVocab.OWNED_CELL_META):
		return node
	for child in node.get_children():
		var found := _owned_cell(child)
		if found != null:
			return found
	return null

## Every text in EVERY Owned cell the panel rendered — the scope the tier-word negative is asked over.
func _all_owned_cell_texts(node: Node) -> Array:
	var texts: Array = []
	if node.has_meta(HudCraftingVocab.OWNED_CELL_META):
		return _label_texts(node)
	for child in node.get_children():
		texts.append_array(_all_owned_cell_texts(child))
	return texts

## What an Owned cell says BESIDE its grade lines — everything that is neither a `×n` count nor one of
## the published legend's band words. On a row the sim gave a note that is the note and nothing else;
## on a row it gave none that is empty. Derived rather than listed, so a cell growing a line nobody
## asked for shows up here instead of slipping past a `has()`.
func _non_grade_texts(cell_texts: Array) -> Array:
	var bands: Array = []
	for band in _characteristic_bands():
		bands.append(String((band as Dictionary).get("name", "")))
	var rest: Array = []
	for text_variant in cell_texts:
		var text := String(text_variant)
		if text.begins_with("×") or bands.has(text):
			continue
		rest.append(text)
	return rest

func _count_matching(texts: Array, needle: String) -> int:
	var found := 0
	for text in texts:
		if String(text) == needle:
			found += 1
	return found

func _count_starting_with(texts: Array, prefix: String) -> int:
	var found := 0
	for text in texts:
		if String(text).begins_with(prefix):
			found += 1
	return found

## Every note the two-tier fixture publishes — the whitelist the tier-word negative subtracts, since a
## note is the one place a tier word is meant to reach a cell.
func _published_owned_notes() -> Array:
	var notes: Array = []
	for offer in _two_tier_offers():
		var note := String((offer as Dictionary).get("owned_note", ""))
		if note != "" and not notes.has(note):
			notes.append(note)
	return notes

func _batches_carrying_a_tier(batches: Array) -> int:
	var carrying := 0
	for batch in batches:
		if String((batch as Dictionary).get("tier_id", "")) != "":
			carrying += 1
	return carrying

func _has_prefix(texts: Array, prefix: String) -> bool:
	for text in texts:
		if String(text).begins_with(prefix):
			return true
	return false

# ---- fixtures -----------------------------------------------------------------------------------

## The per-world MATERIAL catalogue. A material is the generic thing and owns its craft, its axes IN
## DECLARED ORDER, and the tool that bounds it at the bench.
func _materials() -> Array:
	return [
		{"id": "fibre", "craft": "weaving", "axes": ["fine", "strong"],
			"hand_workable": true, "tool_item_id": "loom"},
		{"id": "hide", "craft": "tanning", "axes": ["tough", "supple"],
			"hand_workable": true, "tool_item_id": ""},
		{"id": "bone", "craft": "bone_working", "axes": ["dense", "long"],
			"hand_workable": true, "tool_item_id": "bone_awl"},
	]

## The shared rating vocabulary, ascending — the panel reads only its two ENDS, to decide which chips
## read as a strength and which as a weakness.
func _characteristic_bands() -> Array:
	return [
		{"name": "poor", "from": 0.0},
		{"name": "fair", "from": 0.30},
		{"name": "good", "from": 0.55},
		{"name": "excellent", "from": 0.80},
	]

## The craft tracks, per faction. The display names are the SIM's spelling — "Bone-working", hyphenated
## and capitalized sim-side — because the client never maps a craft id to English.
func _craft_knowledge() -> Array:
	return [
		{"faction": 0, "craft_id": "weaving", "display_name": "Weaving", "known": true,
			"progress": WEAVING_PROGRESS, "completion_threshold": CRAFT_THRESHOLD},
		{"faction": 0, "craft_id": "tanning", "display_name": "Tanning", "known": false,
			"progress": TANNING_PROGRESS, "completion_threshold": CRAFT_THRESHOLD},
		{"faction": 0, "craft_id": "bone_working", "display_name": "Bone-working", "known": false,
			"progress": BONE_WORKING_PROGRESS, "completion_threshold": CRAFT_THRESHOLD},
	]

## The recipe book. One structure — `inputs → outputs` — whether the output is a kit item, a bench
## tool or a pile of stock.
func _recipes() -> Array:
	return [
		_kit_recipe("wayfinding", "Wayfinding gear", "weaving", [["fibre", 6.0, ""]]),
		_kit_recipe("baskets", "Baskets", "weaving", [["fibre", 20.0, "strong"]]),
		_kit_recipe("spears", "Spears", "bone_working", [["fibre", 12.0, ""], ["bone", 8.0, "dense"]]),
		_kit_recipe("husbandry_gear", "Husbandry gear", "tanning",
			[["hide", 12.0, "tough"], ["fibre", 8.0, ""]]),
		_kit_recipe("sled", "Sled", "tanning", [["hide", 18.0, "tough"], ["fibre", 10.0, ""]]),
		_kit_recipe("clubs", "Clubs", "bone_working", [["bone", 10.0, "dense"]]),
		_kit_recipe("traps", "Traps", "weaving", [["fibre", 14.0, ""], ["hide", 6.0, ""]]),
		_tool_recipe("loom", "Loom", "tanning", [["hide", 14.0, ""]]),
		_tool_recipe("bone_awl", "Bone awl", "weaving", [["fibre", 12.0, ""], ["hide", 6.0, ""]]),
		{
			"id": "cordage", "display_name": "Cordage", "craft": "weaving",
			"group": HudCraftingVocab.GROUP_STOCK, "work": 3.0, "requires_knowledge": [],
			"inputs": [{"material_id": "fibre", "amount": 12.0, "reads_axis": "strong"}],
			"outputs": [{"equipment_id": "", "material_id": "cordage", "amount": 6.0}],
		},
	]

func _kit_recipe(id: String, display_name: String, craft: String, inputs: Array) -> Dictionary:
	return _equipment_recipe(id, display_name, craft, HudCraftingVocab.GROUP_KIT, inputs)

func _tool_recipe(id: String, display_name: String, craft: String, inputs: Array) -> Dictionary:
	return _equipment_recipe(id, display_name, craft, HudCraftingVocab.GROUP_TOOL, inputs)

func _equipment_recipe(id: String, display_name: String, craft: String, group: String,
		inputs: Array) -> Dictionary:
	var rows: Array = []
	for input in inputs:
		rows.append({
			"material_id": String(input[0]), "amount": float(input[1]),
			"reads_axis": String(input[2]),
		})
	return {
		"id": id, "display_name": display_name, "craft": craft, "group": group,
		"work": 5.0, "requires_knowledge": [], "inputs": rows,
		"outputs": [{"equipment_id": id, "material_id": "", "amount": 1.0}],
	}

## The band the panel is composed for — the prototype's own, carrying every state the ledger has to
## tell apart at once.
func _crafting_band() -> Dictionary:
	var band := BandFx.with_band_id({
		"id": "Band 1",
		"entity": CRAFTING_BAND_ENTITY,
		"faction": 0,
		"size": 30,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		"working_age": 16,
		"idle_workers": 6,
		"turns_of_food": 22.0,
		"morale": 0.8,
		"labor_assignments": [],
	})
	band["material_batches"] = _material_batches()
	band["bench"] = _bench()
	band["craft_offers"] = _craft_offers()
	band["equipment_batches"] = _equipment_batches()
	return band

## A band that has banked nothing and has nothing on its bench — the first turn of a world.
func _bare_band() -> Dictionary:
	var band := _crafting_band()
	band["material_batches"] = []
	band["bench"] = _idle_bench()
	band["equipment_batches"] = []
	band["craft_offers"] = []
	return band

## **THE SAME BAND ONE TURN LATER** — the bench a pass further along and the fibre it spent gone from
## the rail. The re-render state ticks with this rather than with the identical fixture, because a
## turn tick is what the player pressed and an unchanged payload is a re-render that had nothing to
## rebuild differently.
func _ticked_crafting_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = band["bench"]
	bench["progress"] = float(bench["progress"]) + TICKED_BENCH_WORK
	band["bench"] = bench
	var batches: Array = band["material_batches"]
	var spent: Dictionary = batches[0]
	spent["amount"] = maxf(float(spent["amount"]) - TICKED_FIBRE_SPENT, 0.0)
	batches[0] = spent
	band["material_batches"] = batches
	return band

## **A BAND WHOSE BENCH HOLDS ALL BUT ONE OF ITS WORKERS.** `working_age` is what is lowered, never
## `idle_workers`: `effective_idle` derives idle from the workforce, its assignments and the bench, so
## writing the published field alone would leave every claim below reading the reference band's 16.
func _bench_bound_band() -> Dictionary:
	var band := _crafting_band()
	band["working_age"] = BENCH_BOUND_WORKING_AGE
	band["idle_workers"] = BENCH_BOUND_WORKING_AGE - BENCH_CREW
	return band

## **THE SAME JOB, STOPPED.** State 1's band with the crew walked off its bench and the refusal the
## sim publishes for exactly that. The pile is still `drawn`, which is what keeps the reason genuine —
## a drawn pile is short of nothing, so the crew's own refusal is all that remains to stop it — and
## every other field is state 1's, so the frames differ by the reason alone.
func _blocked_bench_band() -> Dictionary:
	var band := _crafting_band()
	var bench: Dictionary = _bench()
	bench["workers"] = BLOCKED_BENCH_CREW
	bench["blocked_reason"] = BLOCKED_BENCH_REASON
	band["bench"] = bench
	return band

## What the band holds, per rating. **The band rates the AXIS, not the material**: the second hide is
## excellent at being tough and poor at being supple, which is right for a sled and wrong for cordage.
func _material_batches() -> Array:
	return [
		_batch("fibre", 8.4, [["fine", 0.88, "excellent"], ["strong", 0.40, "fair"]]),
		_batch("fibre", 14.4, [["fine", 0.18, "poor"], ["strong", 0.62, "good"]]),
		_batch("hide", 14.2, [["tough", 0.45, "fair"], ["supple", 0.58, "good"]]),
		_batch("hide", 2.6, [["tough", 0.90, "excellent"], ["supple", 0.15, "poor"]]),
		_batch("bone", 3.1, [["dense", 0.82, "excellent"], ["long", 0.35, "fair"]]),
	]

func _batch(material_id: String, amount: float, readings: Array) -> Dictionary:
	var rows: Array = []
	for reading in readings:
		rows.append({"axis": String(reading[0]), "value": float(reading[1]),
			"band_name": String(reading[2])})
	return {"material_id": material_id, "amount": amount, "readings": rows, "variety_name": ""}

func _bench() -> Dictionary:
	return {
		"recipe_id": "baskets", "display_name": "Baskets", "workers": BENCH_CREW,
		"progress": 3.0, "work": 5.0, "teaches": "weaving", "blocked_reason": "",
		"shortfalls": [], "items_completed": 1, "drawn": true, "output_grade": "good",
	}

func _idle_bench() -> Dictionary:
	return {
		"recipe_id": "", "display_name": "", "workers": 0, "progress": 0.0, "work": 0.0,
		"teaches": "", "blocked_reason": "", "shortfalls": [], "items_completed": 0,
		"drawn": false, "output_grade": "",
	}

## **ONE ROW PER RECIPE, ALWAYS**, each carrying the reason and the severity the SIM resolved. Every
## string here is one the sim's own vocabulary produces (`.claude/rules/core_sim/crafting.md`).
func _craft_offers() -> Array:
	return [
		_offer("wayfinding", "Wayfinding gear", HudCraftingVocab.GROUP_KIT, "wayfinding", true,
			"Scouts see 1 tile, not 2", HudCraftingVocab.SEVERITY_DANGER),
		_offer("baskets", "Baskets", HudCraftingVocab.GROUP_KIT, "baskets", true,
			"Reed, no loom → fair", HudCraftingVocab.SEVERITY_NEUTRAL, [], true),
		_offer("spears", "Spears", HudCraftingVocab.GROUP_KIT, "spears", false,
			"Short 4.9 bone", HudCraftingVocab.SEVERITY_DANGER,
			[{"material_id": "bone", "required": 8.0, "held": 3.1, "short": 4.9}]),
		_offer("husbandry_gear", "Husbandry gear", HudCraftingVocab.GROUP_KIT, "husbandry_gear", true,
			"Hide + tanning frame → good", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("sled", "Sled", HudCraftingVocab.GROUP_KIT, "sled", true,
			"Mammoth hide → excellent", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("clubs", "Clubs", HudCraftingVocab.GROUP_KIT, "clubs", false,
			"Short 6.9 bone", HudCraftingVocab.SEVERITY_DANGER,
			[{"material_id": "bone", "required": 10.0, "held": 3.1, "short": 6.9}]),
		_offer("traps", "Traps", HudCraftingVocab.GROUP_KIT, "traps", true,
			"Not needed yet", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("loom", "Loom", HudCraftingVocab.GROUP_TOOL, "loom", true,
			"Unlocks excellent fibre work", HudCraftingVocab.SEVERITY_GOOD),
		_offer("bone_awl", "Bone awl", HudCraftingVocab.GROUP_TOOL, "bone_awl", true,
			"Bone costs −25%", HudCraftingVocab.SEVERITY_NEUTRAL),
		_offer("cordage", "Cordage", HudCraftingVocab.GROUP_STOCK, "", true,
			"Reed .70 → strong", HudCraftingVocab.SEVERITY_NEUTRAL),
	]

## **EVERY OFFER CARRIES ITS GROUP HEAD AND ITS OWNED NOTE**, both resolved sim-side. On the shipped
## one-tier roster the head is `flint` at rank 0 for every equipment recipe and the note is `""` on
## every one of them — a note is published only when what the band carries disagrees with what it
## could now make, which one tier can never produce. The second-tier fixture below is what stages the
## disagreement.
func _offer(recipe_id: String, display_name: String, group: String, output_item_id: String,
		available: bool, reason: String, severity: String, shortfalls: Array = [],
		on_bench: bool = false, tier_name: String = TIER_FLINT, tier_rank: int = TIER_FLINT_RANK,
		owned_note: String = "") -> Dictionary:
	return {
		"recipe_id": recipe_id, "display_name": display_name, "group": group,
		"output_item_id": output_item_id, "available": available, "reason": reason,
		"severity": severity, "shortfalls": shortfalls, "output_grade": "", "on_bench": on_bench,
		"output_tier_name": tier_name if output_item_id != "" else "",
		"output_tier_rank": tier_rank, "owned_note": owned_note,
	}

## What the band OWNS, and how much life is in it. **`count == 0` means it owns none**, and `life` is
## what tells a worn-out item from one that was never made.
func _equipment_batches() -> Array:
	return [
		_batch_row("wayfinding", "", "", 0, 0.0, "Worn out", HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("baskets", TIER_FLINT, "good", 4, 8.0, "~1 turn left",
			HudCraftingVocab.LIFE_SEVERITY_DANGER),
		_batch_row("spears", TIER_FLINT, "good", 6, 34.0, "~15 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("husbandry_gear", TIER_FLINT, "good", 2, 62.0, "~28 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("sled", TIER_FLINT, "fair", 1, 71.0, "~42 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("clubs", TIER_FLINT, "excellent", 5, 96.0, "48 raids left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("traps", TIER_FLINT, "good", 8, 100.0, "Untouched",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("loom", "", "", 0, 0.0, "Never made", HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("bone_awl", TIER_FLINT, "poor", 1, 47.0, "~19 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
	]

# ---- the SECOND-TIER fixture --------------------------------------------------------------------

## **A BAND THAT KNOWS BRONZE AND IS STILL CARRYING FLINT.** The one shape in which the head and the
## cell can disagree, which is the readout the Owned column exists for — and the only one in which the
## sim publishes an `ownedNote` at all. Everything in it is what the sim would have resolved: the note
## verbatim, the tier ranks its own, the grades the shared `characteristic_bands` words.
func _two_tier_band() -> Dictionary:
	var band := _crafting_band()
	band["craft_offers"] = _two_tier_offers()
	band["equipment_batches"] = _two_tier_equipment_batches()
	return band

## Two heads over the kit group — `Bronze` above `Flint`, rank descending — plus the bench tools' own.
## Traps sit at flint because their recipe has no bronze rung, which is what makes the fold claim a
## statement about ONE group rather than about the table.
func _two_tier_offers() -> Array:
	return [
		_offer("spears", "Spears", HudCraftingVocab.GROUP_KIT, "spears", true,
			"Bone + bone awl → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			TIER_BRONZE, TIER_BRONZE_RANK),
		_offer("clubs", "Clubs", HudCraftingVocab.GROUP_KIT, "clubs", true,
			"Bone → good", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			TIER_BRONZE, TIER_BRONZE_RANK, TWO_TIER_CLUBS_NOTE),
		_offer("traps", "Traps", HudCraftingVocab.GROUP_KIT, "traps", true,
			"Reed → fair", HudCraftingVocab.SEVERITY_NEUTRAL, [], false,
			TIER_FLINT, TIER_FLINT_RANK),
		_offer("loom", "Loom", HudCraftingVocab.GROUP_TOOL, "loom", true,
			"Unlocks excellent fibre work", HudCraftingVocab.SEVERITY_GOOD, [], false,
			TIER_FLINT, TIER_FLINT_RANK),
	]

## **THE SPEARS ARE THREE BATCHES AND TWO LINES.** Two of them are `good` at different wear and merge
## into one `×5`; the third is `excellent` and gets its own. The clubs are one batch at the OLDER tier,
## which is what the note is reporting. Every owned batch states its `tier_id`, so the negative that no
## tier word reaches a cell is asked over data that could leak one.
func _two_tier_equipment_batches() -> Array:
	return [
		_batch_row("spears", TIER_BRONZE, "good", TWO_TIER_SPEARS_GOOD_A, 71.0, "~42 turns left",
			HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("spears", TIER_BRONZE, "good", TWO_TIER_SPEARS_GOOD_B, 34.0, "~15 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("spears", TIER_BRONZE, "excellent", TWO_TIER_SPEARS_EXCELLENT, 96.0,
			"48 raids left", HudCraftingVocab.LIFE_SEVERITY_HEALTHY),
		_batch_row("clubs", TIER_FLINT, "poor", TWO_TIER_CLUBS_POOR, 47.0, "~19 turns left",
			HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("traps", "", "", 0, 0.0, "Never made", HudCraftingVocab.LIFE_SEVERITY_WARN),
		_batch_row("loom", "", "", 0, 0.0, "Never made", HudCraftingVocab.LIFE_SEVERITY_WARN),
	]

func _batch_row(item_id: String, tier_id: String, grade: String, count: int, remaining: float,
		life: String, life_severity: String) -> Dictionary:
	return {
		"item_id": item_id, "tier_id": tier_id, "grade": grade, "count": count,
		"remaining": remaining, "quanta_left": remaining, "quantum_noun": "uses",
		"life": life, "life_severity": life_severity,
	}

# ---- the last state: A SCROLL OVER THE CARD MUST NOT ALSO DRIVE THE MAP -------------------------

## **THE GUI PASS STOPS A PRESS FOR US AND STOPS A SCROLL FOR NOBODY.** Reported from play: scrolling
## the Materials & Crafting ledger scrolled the panel *and* panned the map underneath. The panel is not
## the bug — its root and its card are both `MOUSE_FILTER_STOP` — and neither is anything crafting-
## specific: **every floating or docked surface in this client has it**, because the three POINTER
## navigation inputs are routed differently from a press.
##
## Measured in Godot 4.7 by pushing each event over a `STOP` card through `Viewport.push_input`:
##
## | event | over a STOP card |
## |---|---|
## | left press | consumed by the GUI pass |
## | `InputEventPanGesture` (a macOS two-finger scroll) | **survives** to `_unhandled_input` |
## | `InputEventMagnifyGesture` (a pinch) | **survives** |
## | wheel button | **survives** — Godot deliberately propagates a wheel past a `STOP`, so an OUTER
##   scroll container can still take it, and only a `ScrollContainer` that really MOVED accepts one |
##
## So the map declines them itself (`MapView._pointer_claimed_by_ui`), and this is the state that
## proves it. **It is judged by EFFECT, never off a `mouse_filter`** — the `band_panel_preview` idiom —
## because a filter read back says only what a node was configured as.
func _map_gesture_state() -> void:
	# The SHORT room, so the ledger genuinely overflows: "the card still scrolls" is unfalsifiable on a
	# table that fits, and that half is where this state's whole value sits.
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, RESERVED_LEFT_WIDTH)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, RESERVED_BOTTOM_HEIGHT)
	h._hud.update_band_alerts([_crafting_band()])
	h._hud.open_crafting_panel(_crafting_band())
	await h._settle()
	await _assert_a_scroll_over_the_card_leaves_the_map_alone()
	h._hud.set_reserved_inset(RESERVER_LEFT, SIDE_LEFT, 0.0)
	h._hud.set_reserved_inset(RESERVER_BOTTOM, SIDE_BOTTOM, 0.0)
	await h._settle()

## **EVERY CLAIM IS A PAIRING ON THE SAME FRAME**, because a one-sided one passes on a map that has
## stopped answering gestures altogether: over the card the map must hold still, over open map the same
## event must still move it. The scroll offset carries the other half of the over-the-card claim — the
## event was taken by the RIGHT surface rather than dropped on the floor.
##
## The map is a REAL `MapView` (`visible = false`, data only — the `band_panel_preview` idiom), stood
## up for this state and freed again: nothing else in the run may inherit it, and its minimap is its
## own `CanvasLayer`, which `visible = false` would not hide. It is never handed a HUD reference, so
## that minimap is never built at all.
func _assert_a_scroll_over_the_card_leaves_the_map_alone() -> void:
	var panel: CraftingPanel = h._hud.crafting_panel().panel()
	var scroll := _ledger_scroll(panel) if panel != null else null
	if panel == null or scroll == null:
		h._assert_hud("crafting — the gesture probe opens on a scrolling ledger", false)
		return
	h._assert_hud("crafting — precondition: the probed ledger outgrows its room, so it has somewhere to scroll",
		_scroll_is_live(panel))

	var card := panel.get_global_rect()
	# Found BEFORE the map exists, and by a LEFT press rather than by asking the hover: a press is the
	# one pointer input the GUI pass really does stop, so "a press here reached `_unhandled_input`" is
	# an independent reading of "this pixel is open map" — independent, in particular, of the very
	# hover the fix under assertion is built on.
	var open_map := await _find_open_map_point(card)
	if open_map == NO_OPEN_MAP_POINT:
		h._assert_hud("crafting — precondition: the frame offers a pixel of open map to probe", false)
		return
	# The card's own top-left chrome — inside the card, OUTSIDE the ledger's scroll. That distinction
	# is the whole reason this point exists: a live `ScrollContainer` accepts a scroll itself, so a
	# probe aimed at the ledger measures GODOT's routing and says nothing about the map's guard. Over
	# the chrome nothing but the card claims the pixel, which is exactly the reported surface.
	var chrome := card.position + Vector2(CARD_CHROME_PROBE_INSET, CARD_CHROME_PROBE_INSET)
	print("ui_preview: crafting — gesture probe: card chrome %s, ledger %s, open map %s"
		% [chrome, card.get_center(), open_map])

	var view: Node2D = MAP_VIEW_SCRIPT.new()
	view.visible = false
	h.add_child(view)
	# `_unhandled_input` early-outs on an empty grid, so the map has to believe it has one. Nothing
	# below reads a tile — every claim is about `pan_offset` / `zoom_factor` — so a bare grid is the
	# whole world this probe needs.
	view.grid_width = GESTURE_PROBE_GRID.x
	view.grid_height = GESTURE_PROBE_GRID.y
	await h.get_tree().process_frame

	await _assert_gesture_pair(view, scroll, "a two-finger scroll", true, chrome, card.get_center(),
		open_map,
		func(point: Vector2) -> void:
			InputProbe.pan_gesture(h.get_viewport(), point, GESTURE_PROBE_PAN_DELTA))
	# A PINCH IS NOT A SCROLL REQUEST, so the ledger is asserted to be UNMOVED by it rather than
	# excused from the claim. That is the same reading as the two beside it — the event went where it
	# belonged — and it is what stops the flag reading as a licence to skip an inconvenient half.
	await _assert_gesture_pair(view, scroll, "a pinch", false, chrome, card.get_center(), open_map,
		func(point: Vector2) -> void:
			InputProbe.magnify_gesture(h.get_viewport(), point, GESTURE_PROBE_MAGNIFY_FACTOR))
	await _assert_gesture_pair(view, scroll, "a wheel notch", true, chrome, card.get_center(), open_map,
		func(point: Vector2) -> void:
			InputProbe.wheel(h.get_viewport(), point, MOUSE_BUTTON_WHEEL_DOWN))

	view.queue_free()
	await h.get_tree().process_frame

## One event kind, at the FOUR points that between them tell the map's guard from Godot's own routing.
##
## **THE LEDGER PROBES ARE THE WEAK ONES AND THEY ARE HERE FOR THE SCROLL READING, NOT FOR THE MAP'S.**
## A `ScrollContainer` with room left accepts a scroll and a wheel itself, so over a scrolling ledger
## the map holds still whatever `MapView` does — measured: with the guard removed, only the PINCH
## (which no scroll container takes) failed at that point. So the map claim is carried by the card's
## own chrome and by a ledger PARKED AT ITS FLOOR, where the container has nothing left to accept.
##
## `moved` reads pan OR zoom together because `_apply_zoom` pivots on the cursor and therefore moves
## `pan_offset` too — asking about one axis alone would call a zoom "still", or a pan "moved",
## depending only on which the event happened to drive.
func _assert_gesture_pair(view: Node2D, scroll: ScrollContainer, what: String, scrolls_the_ledger: bool,
		over_chrome: Vector2, over_ledger: Vector2, over_map: Vector2, deliver: Callable) -> void:
	# 1. THE CARD'S OWN CHROME — nothing under the pointer but the card, so this is the map's guard
	#    and nothing else, for every one of the three event kinds.
	var moved_over_chrome := await _map_moves(view, over_chrome, deliver)
	h._assert_hud("crafting — %s over the card's chrome leaves the map where it was" % what,
		not moved_over_chrome)

	# 2. THE LEDGER AT ITS TOP — where the event goes when the card has somewhere to put it. Parked
	#    first because a ledger already at its own floor cannot move, and would read as one that
	#    dropped the event: measured, a single pan gesture takes this table all the way down.
	scroll.scroll_vertical = 0
	await h.get_tree().process_frame
	var scrolled_before: int = scroll.scroll_vertical
	var moved_over_ledger := await _map_moves(view, over_ledger, deliver)
	h._assert_hud("crafting — …%s over the ledger leaves it where it was too" % what, not moved_over_ledger)
	var scrolled: bool = scroll.scroll_vertical != scrolled_before
	if scrolls_the_ledger:
		h._assert_hud("crafting — …and the ledger took it (%d → %d)" % [scrolled_before, scroll.scroll_vertical],
			scrolled)
	else:
		h._assert_hud("crafting — …and the ledger rightly ignored it (%d → %d)"
				% [scrolled_before, scroll.scroll_vertical],
			not scrolled)

	# 3. THE LEDGER SCROLLED TO ITS FLOOR — the reported case. A container with nothing left to give
	#    stops accepting a pan gesture, which is exactly when a player at the bottom of the ledger
	#    keeps scrolling and the map lurches out from under the card.
	scroll.scroll_vertical = LEDGER_FLOOR_PARK
	await h.get_tree().process_frame
	h._assert_hud("crafting — precondition: the ledger really parks at a floor below its top (%d)"
			% scroll.scroll_vertical,
		scroll.scroll_vertical > 0)
	var moved_at_the_floor := await _map_moves(view, over_ledger, deliver)
	h._assert_hud("crafting — …and %s with the ledger already at its floor still leaves the map alone" % what,
		not moved_at_the_floor)

	# 4. OPEN MAP — without which every claim above is satisfied by a map that answers nothing at all.
	var moved_over_map := await _map_moves(view, over_map, deliver)
	h._assert_hud("crafting — …while %s over open map still drives the map" % what, moved_over_map)

## Hover the point (a gesture is routed to the HOVERED control, so the hover is part of the event
## rather than setup around it), deliver, and answer whether the map's own pan or zoom moved.
func _map_moves(view: Node2D, point: Vector2, deliver: Callable) -> bool:
	var window_point := InputProbe.canvas_to_window(h.get_viewport(), h.get_window(), point)
	InputProbe.hover(h.get_viewport(), window_point)
	await h.get_tree().process_frame
	var pan_before: Vector2 = view.pan_offset
	var zoom_before: float = view.zoom_factor
	deliver.call(window_point)
	await h.get_tree().process_frame
	return view.pan_offset != pan_before or view.zoom_factor != zoom_before

## The first lattice point outside the card at which a LEFT press survives the GUI pass — i.e. a pixel
## the live client would have picked a hex on. SEARCHED rather than hard-coded: this frame carries a
## left dock, a bottom bar and a centred card, and a literal point becomes a silent lie the day any of
## them moves. `NO_OPEN_MAP_POINT` when the frame offers none, which fails the state loudly instead of
## leaving the pairing half-made.
func _find_open_map_point(card: Rect2) -> Vector2:
	var canvas: Vector2 = h.get_viewport().get_visible_rect().size
	var park := InputProbe.canvas_to_window(h.get_viewport(), h.get_window(),
		canvas * OPEN_MAP_PARK_FRACTION)
	var y := OPEN_MAP_PROBE_STEP
	while y < canvas.y:
		var x := OPEN_MAP_PROBE_STEP
		while x < canvas.x:
			var point := Vector2(x, y)
			if not card.has_point(point):
				var window_point := InputProbe.canvas_to_window(h.get_viewport(), h.get_window(), point)
				h._unhandled_press_seen = false
				InputProbe.left_click(h.get_viewport(), window_point, park)
				await h.get_tree().process_frame
				if h._unhandled_press_seen:
					return point
			x += OPEN_MAP_PROBE_STEP
		y += OPEN_MAP_PROBE_STEP
	return NO_OPEN_MAP_POINT

## The real map, instanced for the one state that needs an input TARGET rather than a picture.
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")

## The shared pointer-input layer — every event below goes through the engine's real dispatch.
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## A grid for the stand-in map to believe in. `MapView._unhandled_input` returns immediately on a zero
## grid and nothing here reads a tile, so the two numbers only have to be non-zero; a small square
## keeps the pan clamp's arithmetic legible if this ever has to be debugged.
const GESTURE_PROBE_GRID := Vector2i(20, 20)

## One downward two-finger scroll, in the units `InputEventPanGesture.delta` carries. Large enough that
## a `ScrollContainer` moves by a whole pixel and the map's own clamp cannot absorb it.
const GESTURE_PROBE_PAN_DELTA := Vector2(0.0, 40.0)

## One pinch-out. `MapView` scales `(factor - 1.0)`, so anything but exactly 1.0 is a real zoom.
const GESTURE_PROBE_MAGNIFY_FACTOR := 1.25

## How far inside the card's top-left corner the chrome probe sits, in canvas px. Small enough to land
## in the `PanelContainer`'s own border + content margin, which is the band of the card that belongs to
## no child at all.
const CARD_CHROME_PROBE_INSET := 4.0

## Where the ledger is parked for the scroll-exhausted probe. `ScrollContainer` clamps to its own
## maximum, so any value past it means "as far down as this table goes" without the harness having to
## restate a height the panel decides.
const LEDGER_FLOOR_PARK := 1000000

## The lattice `_find_open_map_point` walks, in canvas px. Coarse enough that the search is a handful
## of probes rather than thousands, fine enough to find the gaps between this frame's HUD furniture.
const OPEN_MAP_PROBE_STEP := 60.0

## Where the probe's pointer is parked between clicks — the middle of the canvas, which in this state
## is under the card, so the cancelled release lands on a surface with nothing to fire.
const OPEN_MAP_PARK_FRACTION := 0.5

## "The frame offered no open map." A real answer is a point inside the canvas, so the sentinel sits
## outside every canvas this harness renders at.
const NO_OPEN_MAP_POINT := Vector2(-1.0, -1.0)
